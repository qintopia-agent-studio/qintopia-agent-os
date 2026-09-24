use super::{
    state::{evaluate_tx, Snapshot, StayState},
    store::{audit, create_work, Store},
};
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub struct DeliveryClaim {
    pub action: Uuid,
    pub worker: Uuid,
    pub fence: i64,
    pub attempt: Uuid,
}
#[derive(Clone, Copy)]
pub enum Outcome {
    Success,
    DefiniteNoSend,
    Unknown,
    PermanentFailure,
}

impl Store {
    /// Periodic recovery and every source/review/membership wakeup call the same
    /// convergence function. Preparation does not execute an external effect.
    pub async fn reconcile_case(&self, case: Uuid) -> Result<Vec<Uuid>> {
        let foundation_targets:Vec<Uuid>=sqlx::query_scalar("SELECT t.id FROM qintopia_agent_os.welcome_foundation_targets f JOIN qintopia_agent_os.welcome_targets t ON t.id=f.target_id JOIN qintopia_agent_os.welcome_cases c ON c.source_instance=t.source_instance AND c.property_id=t.property_id WHERE c.id=$1")
            .bind(case).fetch_all(&self.pool).await?;
        if !foundation_targets.is_empty() {
            self.evaluate(case).await?;
            let mut actions = vec![];
            for target in foundation_targets {
                for phase in ["formal", "preview"] {
                    match self.foundation_prepare(case, target, phase).await {
                        Ok(result) => {
                            for action in result["actions"].as_array().into_iter().flatten() {
                                if let Some(id) =
                                    action.as_str().and_then(|v| Uuid::parse_str(v).ok())
                                {
                                    actions.push(id);
                                }
                            }
                        }
                        Err(e) if e.downcast_ref::<sqlx::Error>().is_some() => return Err(e),
                        Err(_) => (),
                    }
                }
            }
            return Ok(actions);
        }
        // Persist invalidation even when prepare/claim fails closed and rolls
        // back its own transaction. Success and unknown are never reset here.
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT id FROM qintopia_agent_os.welcome_cases WHERE id=$1 FOR UPDATE")
            .bind(case)
            .fetch_one(&mut *tx)
            .await?;
        let pending=sqlx::query("SELECT id,approval_id,publish_grant_id,work_item_id FROM qintopia_agent_os.welcome_actions WHERE case_id=$1 AND status IN ('prepared','claimed','retryable') FOR UPDATE")
            .bind(case).fetch_all(&mut *tx).await?;
        for row in pending {
            if check_eligibility(
                &mut tx,
                case,
                row.get("approval_id"),
                row.get("publish_grant_id"),
            )
            .await
            .is_err()
            {
                sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status='cancelled',version=version+1 WHERE id=$1").bind(row.get::<Uuid,_>("id")).execute(&mut *tx).await?;
                sqlx::query("UPDATE qintopia_agent_os.work_items SET status='cancelled',claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL WHERE id=$1").bind(row.get::<Uuid,_>("work_item_id")).execute(&mut *tx).await?;
            }
        }
        tx.commit().await?;
        if !self.evaluate(case).await?.card_ready {
            return Ok(vec![]);
        }
        self.request_card(case).await?;
        let rows=sqlx::query("SELECT DISTINCT ON (ap.target_id,ap.phase,a.artifact_type) ap.id,pg.id AS publish_grant FROM qintopia_agent_os.welcome_approvals ap JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=ap.artifact_id JOIN qintopia_agent_os.artifacts a ON a.id=ap.artifact_id JOIN qintopia_agent_os.welcome_grants pg ON pg.target_id=ap.target_id AND pg.action='publish' WHERE b.case_id=$1 AND ap.revoked_at IS NULL AND b.revoked_at IS NULL AND pg.revoked_at IS NULL AND pg.valid_from<=now() AND pg.expires_at>now() ORDER BY ap.target_id,ap.phase,a.artifact_type,ap.created_at DESC,pg.expires_at DESC")
            .bind(case).fetch_all(&self.pool).await?;
        let mut actions = vec![];
        for row in rows {
            match self
                .prepare_delivery(case, row.get("id"), row.get("publish_grant"))
                .await
            {
                Ok(action) => actions.push(action),
                Err(error) if error.downcast_ref::<sqlx::Error>().is_some() => return Err(error),
                Err(_) => (), // Structured current-state gate remains pending.
            }
        }
        Ok(actions)
    }

    pub async fn reconcile_scope(&self, source: &str, property: &str) -> Result<usize> {
        let cases:Vec<Uuid>=sqlx::query_scalar("SELECT c.id FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_sources s ON s.source_instance=c.source_instance AND s.property_id=c.property_id WHERE c.source_instance=$1 AND c.property_id=$2 AND s.mode<>'live'")
            .bind(source).bind(property).fetch_all(&self.pool).await?;
        let mut count = 0;
        for case in cases {
            count += self.reconcile_case(case).await?.len();
        }
        Ok(count)
    }

    pub async fn resolve_unknown(
        &self,
        actor: &super::store::Actor,
        operation: Uuid,
        action: Uuid,
        expected: i64,
        sent: bool,
        evidence_hash: &str,
    ) -> Result<Value> {
        ensure!(
            evidence_hash.len() == 64 && evidence_hash.bytes().all(|b| b.is_ascii_hexdigit()),
            "receipt_evidence_required"
        );
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT a.*,c.source_instance,c.property_id FROM qintopia_agent_os.welcome_actions a JOIN qintopia_agent_os.welcome_cases c ON c.id=a.case_id WHERE a.id=$1 FOR UPDATE OF a")
            .bind(action).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<String, _>("source_instance") == actor.source
                && row.get::<String, _>("property_id") == actor.property,
            "resolution_scope_forbidden"
        );
        super::store::authorize(&mut tx, actor, "publish", Some(row.get("target_id"))).await?;
        let input = json!(["resolve", action, expected, sent, evidence_hash]);
        if let Some(result) =
            super::store::operation_start(&mut tx, actor, operation, &input).await?
        {
            return Ok(result);
        }
        ensure!(
            row.get::<String, _>("status") == "unknown" && row.get::<i64, _>("version") == expected,
            "resolution_version_conflict"
        );
        let status = if sent { "succeeded" } else { "retryable" };
        sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status=$2,receipt_hash=$3,version=version+1 WHERE id=$1")
            .bind(action).bind(status).bind(evidence_hash).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items SET status=$2,claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL WHERE id=$1")
            .bind(row.get::<Uuid,_>("work_item_id")).bind(if sent {"completed"} else {"awaiting_publish"}).execute(&mut *tx).await?;
        let result = json!({"action_ref":action,"status":status,"version":expected+1});
        super::store::operation_finish(&mut tx, actor, operation, &input, &result).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn prepare_delivery(
        &self,
        case: Uuid,
        approval: Uuid,
        publish_grant: Uuid,
    ) -> Result<Uuid> {
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT a.*,ar.artifact_type,c.source_instance,c.property_id FROM qintopia_agent_os.welcome_approvals a JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=a.artifact_id JOIN qintopia_agent_os.artifacts ar ON ar.id=b.artifact_id JOIN qintopia_agent_os.welcome_cases c ON c.id=b.case_id WHERE a.id=$1 AND c.id=$2 FOR UPDATE OF c,a")
            .bind(approval).bind(case).fetch_one(&mut *tx).await?;
        let target: Uuid = row.get("target_id");
        let phase: String = row.get("phase");
        let part = if row.get::<String, _>("artifact_type") == "welcome_card" {
            "image"
        } else {
            "text"
        };
        check_eligibility(&mut tx, case, approval, publish_grant).await?;
        let key = format!("welcome-v1/{case}/{phase}/{target}/{part}");
        let work = create_work(
            &mut tx,
            &key,
            "welcome_delivery",
            if phase == "internal" {
                "silaoshi"
            } else {
                "erhua"
            },
            "awaiting_publish",
        )
        .await?;
        let epoch:i64=sqlx::query_scalar("SELECT execution_epoch FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2")
            .bind(row.get::<String,_>("source_instance")).bind(row.get::<String,_>("property_id")).fetch_one(&mut *tx).await?;
        let grant_version: i64 =
            sqlx::query_scalar("SELECT version FROM qintopia_agent_os.welcome_grants WHERE id=$1")
                .bind(publish_grant)
                .fetch_one(&mut *tx)
                .await?;
        let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_actions(case_id,target_id,phase,part,work_item_id,approval_id,publish_grant_id,publish_grant_version,target_version,execution_epoch) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (case_id,phase,target_id,part) DO UPDATE SET approval_id=CASE WHEN qintopia_agent_os.welcome_actions.status IN ('prepared','cancelled','retryable') THEN EXCLUDED.approval_id ELSE qintopia_agent_os.welcome_actions.approval_id END,publish_grant_id=CASE WHEN qintopia_agent_os.welcome_actions.status IN ('prepared','cancelled','retryable') THEN EXCLUDED.publish_grant_id ELSE qintopia_agent_os.welcome_actions.publish_grant_id END,publish_grant_version=CASE WHEN qintopia_agent_os.welcome_actions.status IN ('prepared','cancelled','retryable') THEN EXCLUDED.publish_grant_version ELSE qintopia_agent_os.welcome_actions.publish_grant_version END,target_version=CASE WHEN qintopia_agent_os.welcome_actions.status IN ('prepared','cancelled','retryable') THEN EXCLUDED.target_version ELSE qintopia_agent_os.welcome_actions.target_version END,status=CASE WHEN qintopia_agent_os.welcome_actions.status IN ('prepared','cancelled','retryable') THEN 'prepared' ELSE qintopia_agent_os.welcome_actions.status END RETURNING id")
            .bind(case).bind(target).bind(&phase).bind(part).bind(work).bind(approval).bind(publish_grant).bind(grant_version).bind(row.get::<i64,_>("target_version")).bind(epoch).fetch_one(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items w SET status='awaiting_publish' FROM qintopia_agent_os.welcome_actions a WHERE a.id=$1 AND a.work_item_id=w.id AND a.status='prepared'").bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn claim_delivery(&self, action: Uuid, worker: Uuid) -> Result<DeliveryClaim> {
        let mut tx = self.pool.begin().await?;
        lock_action_case(&mut tx, action).await?;
        let row=sqlx::query("SELECT a.*,w.claim_expires_at FROM qintopia_agent_os.welcome_actions a JOIN qintopia_agent_os.work_items w ON w.id=a.work_item_id WHERE a.id=$1 FOR UPDATE OF a,w")
            .bind(action).fetch_one(&mut *tx).await?;
        let state = row.get::<String, _>("status");
        ensure!(
            matches!(state.as_str(), "prepared" | "retryable")
                || state == "claimed"
                    && row
                        .get::<Option<DateTime<Utc>>, _>("claim_expires_at")
                        .is_some_and(|t| t <= Utc::now()),
            "action_not_claimable"
        );
        if row.get::<Option<Value>, _>("foundation_basis").is_some() {
            super::foundation::check_action(&self.pool, &mut tx, &row).await?;
        } else {
            check_eligibility(
                &mut tx,
                row.get("case_id"),
                row.get("approval_id"),
                row.get("publish_grant_id"),
            )
            .await?;
        }
        let claim = DeliveryClaim {
            action,
            worker,
            fence: row.get::<i64, _>("fence") + 1,
            attempt: Uuid::new_v4(),
        };
        sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status='claimed',fence=$2,attempt_id=$3,version=version+1 WHERE id=$1")
            .bind(action).bind(claim.fence).bind(claim.attempt).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items SET status='processing',claimed_by=$2,locked_at=now(),claim_expires_at=now()+interval '60 seconds',attempts=attempts+1 WHERE id=$1")
            .bind(row.get::<Uuid,_>("work_item_id")).bind(worker.to_string()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(claim)
    }

    /// This is the durable boundary before invoking a provider. No provider is
    /// connected in V1 local mode. Test code may simulate outcomes after this call.
    pub async fn begin_synthetic_attempt(&self, claim: &DeliveryClaim) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        let row = lock_claim(&mut tx, claim, true).await?;
        ensure!(
            row.get::<String, _>("status") == "claimed",
            "already_started"
        );
        if row.get::<Option<Value>, _>("foundation_basis").is_some() {
            super::foundation::check_action(&self.pool, &mut tx, &row).await?;
        } else {
            check_eligibility(
                &mut tx,
                row.get("case_id"),
                row.get("approval_id"),
                row.get("publish_grant_id"),
            )
            .await?;
        }
        let source=sqlx::query("SELECT s.mode,s.execution_epoch FROM qintopia_agent_os.welcome_sources s JOIN qintopia_agent_os.welcome_cases c ON c.source_instance=s.source_instance AND c.property_id=s.property_id WHERE c.id=$1 FOR SHARE OF s")
            .bind(row.get::<Uuid,_>("case_id")).fetch_one(&mut *tx).await?;
        ensure!(
            source.get::<String, _>("mode") == "synthetic"
                && source.get::<i64, _>("execution_epoch") == row.get::<i64, _>("execution_epoch"),
            "execution_epoch_or_mode_denied"
        );
        if row.get::<Option<Value>, _>("foundation_basis").is_none() {
            let grant_version: i64 = sqlx::query_scalar(
                "SELECT version FROM qintopia_agent_os.welcome_grants WHERE id=$1",
            )
            .bind(row.get::<Uuid, _>("publish_grant_id"))
            .fetch_one(&mut *tx)
            .await?;
            ensure!(
                grant_version == row.get::<i64, _>("publish_grant_version"),
                "publish_grant_stale"
            );
        }
        sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status='sending',version=version+1 WHERE id=$1").bind(claim.action).execute(&mut *tx).await?;
        audit(
            &mut tx,
            Some(row.get("work_item_id")),
            "welcome_attempt_started",
            None,
            json!({"attempt_ref":claim.attempt,"fence":claim.fence,"synthetic":true}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Known results accept the exact attempt after lease expiry; a new worker may
    /// not replace a sending/unknown attempt. Raw provider receipts never persist.
    pub async fn record_outcome(
        &self,
        claim: &DeliveryClaim,
        outcome: Outcome,
        receipt_hash: Option<&str>,
    ) -> Result<()> {
        if let Some(hash) = receipt_hash {
            ensure!(
                hash.len() == 64 && hash.bytes().all(|b| b.is_ascii_hexdigit()),
                "invalid_receipt_hash"
            )
        }
        if matches!(outcome, Outcome::Success) {
            ensure!(receipt_hash.is_some(), "success_requires_receipt")
        }
        let mut tx = self.pool.begin().await?;
        let row = lock_claim(&mut tx, claim, false).await?;
        ensure!(
            row.get::<String, _>("status") == "sending",
            "outcome_requires_inflight_attempt"
        );
        let (state, work_state) = match outcome {
            Outcome::Success => ("succeeded", "completed"),
            Outcome::DefiniteNoSend => ("retryable", "awaiting_publish"),
            Outcome::Unknown => ("unknown", "awaiting_review"),
            Outcome::PermanentFailure => ("failed", "failed"),
        };
        sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status=$2,receipt_hash=$3,version=version+1 WHERE id=$1")
            .bind(claim.action).bind(state).bind(receipt_hash).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items SET status=$2,claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL WHERE id=$1")
            .bind(row.get::<Uuid,_>("work_item_id")).bind(work_state).execute(&mut *tx).await?;
        audit(&mut tx,Some(row.get("work_item_id")),"welcome_attempt_result",None,json!({"attempt_ref":claim.attempt,"outcome":state,"external_send_executed":null,"synthetic":true})).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn recover_sending(&self, source: &str, property: &str) -> Result<u64> {
        let mut tx = self.pool.begin().await?;
        let rows=sqlx::query("SELECT a.id,a.work_item_id FROM qintopia_agent_os.welcome_actions a JOIN qintopia_agent_os.work_items w ON w.id=a.work_item_id JOIN qintopia_agent_os.welcome_cases c ON c.id=a.case_id WHERE c.source_instance=$1 AND c.property_id=$2 AND a.status='sending' AND w.claim_expires_at<=now() FOR UPDATE OF a,w SKIP LOCKED")
            .bind(source).bind(property).fetch_all(&mut *tx).await?;
        for row in &rows {
            sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status='unknown',version=version+1 WHERE id=$1").bind(row.get::<Uuid,_>("id")).execute(&mut *tx).await?;
            sqlx::query("UPDATE qintopia_agent_os.work_items SET status='awaiting_review',claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL WHERE id=$1").bind(row.get::<Uuid,_>("work_item_id")).execute(&mut *tx).await?;
            audit(
                &mut tx,
                Some(row.get("work_item_id")),
                "welcome_attempt_unknown",
                None,
                json!({"external_send_executed":null}),
            )
            .await?;
        }
        tx.commit().await?;
        Ok(rows.len() as u64)
    }
}

async fn lock_claim(
    tx: &mut Transaction<'_, Postgres>,
    claim: &DeliveryClaim,
    unexpired: bool,
) -> Result<sqlx::postgres::PgRow> {
    lock_action_case(tx, claim.action).await?;
    let row=sqlx::query("SELECT a.*,w.claimed_by,w.claim_expires_at FROM qintopia_agent_os.welcome_actions a JOIN qintopia_agent_os.work_items w ON w.id=a.work_item_id WHERE a.id=$1 FOR UPDATE OF a,w")
        .bind(claim.action).fetch_one(&mut **tx).await?;
    ensure!(
        row.get::<i64, _>("fence") == claim.fence
            && row.get::<Option<Uuid>, _>("attempt_id") == Some(claim.attempt)
            && row.get::<Option<String>, _>("claimed_by").as_deref()
                == Some(claim.worker.to_string().as_str()),
        "stale_delivery_claim"
    );
    ensure!(
        !unexpired
            || row
                .get::<Option<DateTime<Utc>>, _>("claim_expires_at")
                .is_some_and(|t| t > Utc::now()),
        "expired_delivery_claim"
    );
    Ok(row)
}

async fn lock_action_case(tx: &mut Transaction<'_, Postgres>, action: Uuid) -> Result<()> {
    let tenant: Option<String> = sqlx::query_scalar(
        "SELECT foundation_basis->>'tenant' FROM qintopia_agent_os.welcome_actions WHERE id=$1",
    )
    .bind(action)
    .fetch_one(&mut **tx)
    .await?;
    if let Some(tenant) = tenant {
        sqlx::query(
            "SELECT 1 FROM qintopia_agent_os.collaboration_tenants WHERE tenant_key=$1 FOR UPDATE",
        )
        .bind(tenant)
        .fetch_one(&mut **tx)
        .await?;
    }
    sqlx::query("SELECT c.id FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_actions a ON a.case_id=c.id WHERE a.id=$1 FOR UPDATE OF c").bind(action).fetch_one(&mut **tx).await?;
    Ok(())
}

async fn check_eligibility(
    tx: &mut Transaction<'_, Postgres>,
    case: Uuid,
    approval: Uuid,
    publish: Uuid,
) -> Result<()> {
    let shared:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_approvals a JOIN qintopia_agent_os.welcome_foundation_targets f ON f.target_id=a.target_id WHERE a.id=$1)")
        .bind(approval).fetch_one(&mut **tx).await?;
    ensure!(!shared, "shared_authority_required");
    ensure!(evaluate_tx(tx, case).await?.card_ready, "case_not_ready");
    let row=sqlx::query("SELECT ap.phase,ap.target_id,t.kind,t.building_code,c.person_id,v.projection FROM qintopia_agent_os.welcome_approvals ap JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=ap.artifact_id JOIN qintopia_agent_os.artifacts ar ON ar.id=b.artifact_id JOIN qintopia_agent_os.welcome_cases c ON c.id=b.case_id JOIN qintopia_agent_os.welcome_applications app ON app.id=c.application_id JOIN qintopia_agent_os.welcome_targets t ON t.id=ap.target_id AND t.source_instance=c.source_instance AND t.property_id=c.property_id JOIN qintopia_agent_os.welcome_grants rg ON rg.id=ap.grant_id JOIN qintopia_agent_os.welcome_grants pg ON pg.id=$3 JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id WHERE ap.id=$2 AND c.id=$1 AND ap.revoked_at IS NULL AND b.revoked_at IS NULL AND ar.review_status='approved' AND ar.content_hash=ap.content_hash AND b.case_version=c.version AND b.application_revision=app.revision AND b.consent_version=app.consent_version AND t.version=ap.target_version AND t.enabled AND rg.action='review' AND rg.person_id=ap.approved_by AND rg.target_id=t.id AND rg.version=ap.grant_version AND pg.action='publish' AND pg.target_id=t.id AND pg.source_instance=c.source_instance AND pg.property_id=c.property_id AND rg.source_instance=c.source_instance AND rg.property_id=c.property_id AND rg.revoked_at IS NULL AND pg.revoked_at IS NULL AND rg.valid_from<=now() AND pg.valid_from<=now() AND rg.expires_at>now() AND pg.expires_at>now() AND NOT EXISTS (SELECT 1 FROM qintopia_agent_os.welcome_grants g LEFT JOIN qintopia_identity.person_memberships m ON m.id=g.membership_id JOIN qintopia_identity.persons p ON p.id=g.person_id WHERE g.id IN (rg.id,pg.id) AND (p.status<>'active' OR (g.membership_id IS NOT NULL AND (m.id IS NULL OR m.person_id<>g.person_id OR m.status<>'active' OR m.started_at>now() OR m.ended_at<=now())))) FOR SHARE OF ap,b,ar,app,t,rg,pg")
        .bind(case).bind(approval).bind(publish).fetch_optional(&mut **tx).await?;
    let row = row.ok_or_else(|| anyhow::anyhow!("approval_or_grant_stale"))?;
    let s: Snapshot = serde_json::from_value(row.get::<Value, _>("projection"))?;
    ensure!(
        s.observed_at >= Utc::now() - chrono::Duration::seconds(60)
            && s.observed_at <= Utc::now() + chrono::Duration::seconds(5),
        "pms_refresh_required"
    );
    let phase: String = row.get("phase");
    let kind: String = row.get("kind");
    let target: Uuid = row.get("target_id");
    ensure!(
        kind != "building" || row.get::<String, _>("building_code") == s.building,
        "building_changed"
    );
    ensure!(
        phase != "internal" || kind == "internal",
        "internal_target_required"
    );
    ensure!(
        kind != "internal" || phase == "internal",
        "internal_manual_phase_required"
    );
    if phase == "formal" {
        ensure!(s.state == StayState::InHouse, "actual_check_in_required");
        let current:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_members m JOIN qintopia_identity.source_identity_links l ON l.id=m.identity_link_id WHERE m.target_id=$1 AND l.person_id=$2 AND l.status='confirmed' AND m.current AND m.observed_at>=now()-interval '60 seconds' AND m.observed_at<=now()+interval '5 seconds')")
            .bind(target).bind(row.get::<Uuid,_>("person_id")).fetch_one(&mut **tx).await?;
        ensure!(current, "current_target_membership_required");
    } else if phase == "preview" {
        ensure!(
            s.state == StayState::Reserved && s.inventory_reserved,
            "reserved_inventory_required"
        )
    }
    if kind == "building" {
        let already_elsewhere:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_actions a JOIN qintopia_agent_os.welcome_targets t ON t.id=a.target_id WHERE a.case_id=$1 AND a.phase=$2 AND t.kind='building' AND a.target_id<>$3 AND a.status IN ('succeeded','sending','unknown'))")
            .bind(case).bind(&phase).bind(target).fetch_one(&mut **tx).await?;
        ensure!(
            !already_elsewhere,
            "changed_building_requires_manual_resolution"
        );
    }
    Ok(())
}
