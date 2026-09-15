use super::{
    digest,
    protocol::{reject, VerifiedEvent},
};
use anyhow::{bail, ensure, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

pub struct Store {
    pub(crate) pool: PgPool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Ack {
    pub status: String,
    pub event_id: String,
    pub receipt_id: Uuid,
}
#[derive(Clone)]
pub struct Claim {
    pub inbox_id: Uuid,
    pub work_item_id: Uuid,
    pub worker: Uuid,
    pub fence: i64,
}

impl Store {
    pub async fn local(database_url: &str) -> Result<Self> {
        Ok(Self {
            pool: super::connect_local(database_url).await?,
        })
    }

    pub async fn accept(&self, event: &VerifiedEvent) -> Result<Ack> {
        let mut tx = self.pool.begin().await?;
        let ack = accept_tx(&mut tx, event).await?;
        tx.commit().await?;
        Ok(ack)
    }

    /// Cursor compare-and-swap is serialized with all rows in this page. A crash or
    /// any conflict leaves the old cursor; push and pull share accept_tx.
    pub async fn accept_page(
        &self,
        source: &str,
        property: &str,
        expected: Option<&str>,
        next: &str,
        events: &[VerifiedEvent],
    ) -> Result<()> {
        ensure!(
            !next.is_empty() && next.len() <= 4096 && events.len() <= 100,
            "invalid page"
        );
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT cursor FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2 FOR UPDATE")
            .bind(source).bind(property).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<Option<String>, _>("cursor").as_deref() == expected,
            "cursor_conflict"
        );
        for event in events {
            ensure!(
                event.envelope.source_instance == source && event.envelope.property_id == property,
                "page_scope_mismatch"
            );
            accept_tx(&mut tx, event).await?;
        }
        sqlx::query("UPDATE qintopia_agent_os.welcome_sources SET cursor=$3 WHERE source_instance=$1 AND property_id=$2")
            .bind(source).bind(property).bind(next).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn require_rebuild(
        &self,
        source: &str,
        property: &str,
        head: Option<&str>,
    ) -> Result<()> {
        let n = sqlx::query("UPDATE qintopia_agent_os.welcome_sources SET rebuilding=true, enabled=false, rebuild_head=$3 WHERE source_instance=$1 AND property_id=$2")
            .bind(source).bind(property).bind(head).execute(&self.pool).await?.rows_affected();
        ensure!(n == 1, "unknown source");
        Ok(())
    }

    pub async fn claim(&self, source: &str, property: &str, worker: Uuid) -> Result<Option<Claim>> {
        let mut tx = self.pool.begin().await?;
        let row = sqlx::query("SELECT i.id,i.work_item_id,i.fence FROM qintopia_agent_os.welcome_inbox i JOIN qintopia_agent_os.work_items w ON w.id=i.work_item_id WHERE i.source_instance=$1 AND i.property_id=$2 AND ((i.status='queued' AND w.available_at<=now()) OR (i.status='processing' AND w.claim_expires_at<=now())) ORDER BY i.received_at FOR UPDATE OF i,w SKIP LOCKED LIMIT 1")
            .bind(source).bind(property).fetch_optional(&mut *tx).await?;
        let Some(row) = row else { return Ok(None) };
        let claim = Claim {
            inbox_id: row.get("id"),
            work_item_id: row.get("work_item_id"),
            worker,
            fence: row.get::<i64, _>("fence") + 1,
        };
        sqlx::query(
            "UPDATE qintopia_agent_os.welcome_inbox SET status='processing',fence=$2 WHERE id=$1",
        )
        .bind(claim.inbox_id)
        .bind(claim.fence)
        .execute(&mut *tx)
        .await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items SET status='processing',claimed_by=$2,locked_at=now(),claim_expires_at=now()+interval '60 seconds',attempts=attempts+1 WHERE id=$1")
            .bind(claim.work_item_id).bind(worker.to_string()).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(Some(claim))
    }

    pub async fn finish(&self, claim: &Claim, quarantine: bool) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        fence_tx(&mut tx, claim).await?;
        complete_tx(&mut tx, claim, quarantine).await?;
        tx.commit().await?;
        Ok(())
    }
}

pub(crate) async fn audit(
    tx: &mut Transaction<'_, Postgres>,
    work: Option<Uuid>,
    event: &str,
    actor: Option<Uuid>,
    data: Value,
) -> Result<()> {
    sqlx::query("INSERT INTO qintopia_agent_os.work_item_events(work_item_id,event_type,actor_type,actor_id,data) VALUES ($1,$2,$3,$4,$5)")
        .bind(work).bind(event).bind(if actor.is_some() {"human"} else {"system"})
        .bind(actor.map(|id|id.to_string()).unwrap_or_default()).bind(data).execute(&mut **tx).await?;
    Ok(())
}

pub(crate) async fn create_work(
    tx: &mut Transaction<'_, Postgres>,
    key: &str,
    kind: &str,
    target: &str,
    status: &str,
) -> Result<Uuid> {
    ensure!(
        matches!(
            kind,
            "welcome_event" | "welcome_review" | "welcome_delivery" | "welcome_card"
        ),
        "unsupported welcome task"
    );
    ensure!(
        matches!(target, "silaoshi" | "erhua" | "huabaosi"),
        "unsupported welcome agent"
    );
    let row = sqlx::query("INSERT INTO qintopia_agent_os.work_items(work_item_type,status,requester_agent,target_agent,capability_key,brief_summary,dedupe_key,idempotency_key,source_type) VALUES ($1,$2,'silaoshi',$3,'resident_welcome.coordinate','Resident welcome controlled task',$4,$4,'resident_welcome') ON CONFLICT (idempotency_key) DO NOTHING RETURNING id")
        .bind(kind).bind(status).bind(target).bind(key).fetch_optional(&mut **tx).await?;
    if let Some(row) = row {
        let id = row.get("id");
        audit(
            tx,
            Some(id),
            "welcome_task_created",
            None,
            json!({"kind":kind}),
        )
        .await?;
        Ok(id)
    } else {
        Ok(sqlx::query_scalar(
            "SELECT id FROM qintopia_agent_os.work_items WHERE idempotency_key=$1",
        )
        .bind(key)
        .fetch_one(&mut **tx)
        .await?)
    }
}

async fn accept_tx(tx: &mut Transaction<'_, Postgres>, event: &VerifiedEvent) -> Result<Ack> {
    let e = &event.envelope;
    let mode: String = sqlx::query_scalar("SELECT mode FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2")
        .bind(&e.source_instance).bind(&e.property_id).fetch_optional(&mut **tx).await?
        .ok_or_else(|| reject(403,"unregistered_source"))?;
    ensure!(mode != "live", "live_ingress_not_activated");
    let receipt = Uuid::new_v4();
    let inserted = sqlx::query("INSERT INTO qintopia_agent_os.welcome_inbox(id,source_instance,property_id,event_id,body_hash,envelope) VALUES ($1,$2,$3,$4,$5,$6) ON CONFLICT (source_instance,event_id) DO NOTHING")
        .bind(receipt).bind(&e.source_instance).bind(&e.property_id).bind(&e.event_id)
        .bind(&event.body_hash).bind(serde_json::to_value(e)?).execute(&mut **tx).await?.rows_affected();
    if inserted == 0 {
        let row = sqlx::query("SELECT id,body_hash FROM qintopia_agent_os.welcome_inbox WHERE source_instance=$1 AND event_id=$2")
            .bind(&e.source_instance).bind(&e.event_id).fetch_one(&mut **tx).await?;
        if row.get::<String, _>("body_hash") != event.body_hash {
            return Err(reject(409, "event_body_conflict").into());
        }
        return Ok(Ack {
            status: "duplicate".into(),
            event_id: e.event_id.clone(),
            receipt_id: row.get("id"),
        });
    }
    let key = format!("welcome-inbox/{receipt}");
    let work = create_work(tx, &key, "welcome_event", "silaoshi", "queued").await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_inbox SET work_item_id=$2 WHERE id=$1")
        .bind(receipt)
        .bind(work)
        .execute(&mut **tx)
        .await?;
    Ok(Ack {
        status: "accepted".into(),
        event_id: e.event_id.clone(),
        receipt_id: receipt,
    })
}

pub(crate) async fn fence_tx(tx: &mut Transaction<'_, Postgres>, claim: &Claim) -> Result<()> {
    let ok: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_inbox i JOIN qintopia_agent_os.work_items w ON w.id=i.work_item_id WHERE i.id=$1 AND w.id=$2 AND i.fence=$3 AND i.status='processing' AND w.status='processing' AND w.claimed_by=$4 AND w.claim_expires_at>now() FOR UPDATE OF i,w)")
        .bind(claim.inbox_id).bind(claim.work_item_id).bind(claim.fence).bind(claim.worker.to_string()).fetch_one(&mut **tx).await?;
    ensure!(ok, "stale_claim");
    Ok(())
}

pub(crate) async fn complete_tx(
    tx: &mut Transaction<'_, Postgres>,
    claim: &Claim,
    quarantine: bool,
) -> Result<()> {
    sqlx::query("UPDATE qintopia_agent_os.welcome_inbox SET status=$2 WHERE id=$1")
        .bind(claim.inbox_id)
        .bind(if quarantine {
            "quarantined"
        } else {
            "completed"
        })
        .execute(&mut **tx)
        .await?;
    let n = sqlx::query("UPDATE qintopia_agent_os.work_items SET status=$2,claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL,updated_at=now() WHERE id=$1")
        .bind(claim.work_item_id).bind(if quarantine {"awaiting_review"} else {"completed"}).execute(&mut **tx).await?.rows_affected();
    ensure!(n == 1, "work_item_missing");
    audit(
        tx,
        Some(claim.work_item_id),
        "welcome_event_processed",
        None,
        json!({"quarantined":quarantine,"fence":claim.fence}),
    )
    .await
}

/// Authenticated session material; deliberately not Deserialize. Channel ingress
/// must verify the individual human, not a display name or a shared work account.
pub struct Actor {
    pub(crate) person: Uuid,
    pub(crate) source: String,
    pub(crate) property: String,
    pub(crate) expires: DateTime<Utc>,
}
impl Store {
    pub async fn actor_from_verified_identity(
        &self,
        link: Uuid,
        source: &str,
        property: &str,
        individually_verified: bool,
    ) -> Result<Actor> {
        ensure!(individually_verified, "individual_operator_required");
        let person: Uuid = sqlx::query_scalar("SELECT l.person_id FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.persons p ON p.id=l.person_id WHERE l.id=$1 AND l.status='confirmed' AND p.status='active' AND l.subject_type IN ('wecom_internal','feishu_open')")
            .bind(link).fetch_one(&self.pool).await?;
        Ok(Actor {
            person,
            source: source.into(),
            property: property.into(),
            expires: Utc::now() + chrono::Duration::minutes(5),
        })
    }

    // The transaction binds actor, idempotency and optimistic version separately
    // from the exact link/person/evidence mutation; none is an optional guard.
    #[allow(clippy::too_many_arguments)]
    pub async fn change_identity(
        &self,
        actor: &Actor,
        operation: Uuid,
        link: Uuid,
        person: Uuid,
        expected: i64,
        evidence: Uuid,
        revoke: bool,
    ) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        authorize(&mut tx, actor, "identity", None).await?;
        let request = json!([link, person, expected, evidence, revoke]);
        if let Some(result) = operation_start(&mut tx, actor, operation, &request).await? {
            return Ok(result);
        }
        let row = sqlx::query("SELECT version,person_id,namespace,status FROM qintopia_identity.source_identity_links WHERE id=$1 FOR UPDATE")
            .bind(link).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<i64, _>("version") == expected,
            "identity_version_conflict"
        );
        // Source namespace must be explicitly assigned to this operational scope.
        let allowed: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_identity_scopes WHERE namespace=$1 AND source_instance=$2 AND property_id=$3)")
            .bind(row.get::<String,_>("namespace")).bind(&actor.source).bind(&actor.property).fetch_one(&mut *tx).await?;
        ensure!(allowed, "identity_scope_forbidden");
        if !revoke
            && row.get::<String, _>("status") == "confirmed"
            && row.get::<Option<Uuid>, _>("person_id") != Some(person)
        {
            bail!("revoke_conflicting_link_first");
        }
        if revoke {
            ensure!(
                row.get::<Option<Uuid>, _>("person_id") == Some(person),
                "revoke_person_mismatch"
            );
        }
        invalidate_link(&mut tx, link).await?;
        sqlx::query("UPDATE qintopia_identity.source_identity_links SET person_id=$2,status=$3,version=version+1,evidence_ref=$4,confirmed_by=$5,updated_at=now() WHERE id=$1")
            .bind(link).bind(person).bind(if revoke {"revoked"} else {"confirmed"}).bind(evidence).bind(actor.person).execute(&mut *tx).await?;
        let result = json!({"link_ref":link,"version":expected+1,"status":if revoke {"revoked"} else {"confirmed"}});
        operation_finish(&mut tx, actor, operation, &request, &result).await?;
        tx.commit().await?;
        Ok(result)
    }
}

pub(crate) async fn invalidate_link(tx: &mut Transaction<'_, Postgres>, link: Uuid) -> Result<()> {
    let cases:Vec<Uuid> = sqlx::query_scalar("UPDATE qintopia_agent_os.welcome_cases c SET version=version+1,manual_hold=true WHERE (identity_link_id=$1 OR person_id=(SELECT person_id FROM qintopia_identity.source_identity_links WHERE id=$1)) AND EXISTS (SELECT 1 FROM qintopia_agent_os.welcome_identity_scopes s JOIN qintopia_identity.source_identity_links l ON l.namespace=s.namespace WHERE l.id=$1 AND s.source_instance=c.source_instance AND s.property_id=c.property_id) RETURNING c.id")
        .bind(link).fetch_all(&mut **tx).await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_artifact_bindings SET revoked_at=now() WHERE case_id=ANY($1)")
        .bind(&cases).execute(&mut **tx).await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status='cancelled',version=version+1 WHERE case_id=ANY($1) AND status IN ('prepared','claimed','retryable')")
        .bind(&cases).execute(&mut **tx).await?;
    sqlx::query("UPDATE qintopia_agent_os.work_items w SET status='cancelled',claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL FROM qintopia_agent_os.welcome_actions a WHERE a.work_item_id=w.id AND a.case_id=ANY($1) AND a.status='cancelled' AND w.status<>'cancelled'")
        .bind(&cases).execute(&mut **tx).await?;
    Ok(())
}

pub(crate) async fn authorize(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    action: &str,
    target: Option<Uuid>,
) -> Result<(Uuid, i64)> {
    ensure!(actor.expires > Utc::now(), "session_expired");
    let row=sqlx::query("SELECT g.id,g.version FROM qintopia_agent_os.welcome_grants g JOIN qintopia_identity.persons p ON p.id=g.person_id LEFT JOIN qintopia_identity.person_memberships m ON m.id=g.membership_id WHERE g.person_id=$1 AND g.source_instance=$2 AND g.property_id=$3 AND g.action=$4 AND g.target_id IS NOT DISTINCT FROM $5 AND g.revoked_at IS NULL AND g.valid_from<=now() AND g.expires_at>now() AND p.status='active' AND (g.membership_id IS NULL OR (m.person_id=g.person_id AND m.status='active' AND (m.started_at IS NULL OR m.started_at<=now()) AND (m.ended_at IS NULL OR m.ended_at>now()))) ORDER BY g.expires_at DESC LIMIT 1 FOR SHARE OF g,p")
        .bind(actor.person).bind(&actor.source).bind(&actor.property).bind(action).bind(target).fetch_optional(&mut **tx).await?;
    let row = row.ok_or_else(|| anyhow::anyhow!("role_scope_forbidden"))?;
    Ok((row.get("id"), row.get("version")))
}

pub(crate) async fn operation_start(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    id: Uuid,
    request: &Value,
) -> Result<Option<Value>> {
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(id.to_string())
        .execute(&mut **tx)
        .await?;
    let row=sqlx::query("SELECT actor_id,request_hash,result FROM qintopia_agent_os.welcome_operations WHERE operation_id=$1").bind(id).fetch_optional(&mut **tx).await?;
    if let Some(row) = row {
        ensure!(
            row.get::<Uuid, _>("actor_id") == actor.person
                && row.get::<String, _>("request_hash") == digest(&serde_json::to_vec(request)?),
            "operation_conflict"
        );
        Ok(Some(row.get("result")))
    } else {
        Ok(None)
    }
}
pub(crate) async fn operation_finish(
    tx: &mut Transaction<'_, Postgres>,
    actor: &Actor,
    id: Uuid,
    request: &Value,
    result: &Value,
) -> Result<()> {
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_operations(operation_id,actor_id,request_hash,result) VALUES ($1,$2,$3,$4)")
        .bind(id).bind(actor.person).bind(digest(&serde_json::to_vec(request)?)).bind(result).execute(&mut **tx).await?;
    audit(
        tx,
        None,
        "welcome_human_operation",
        Some(actor.person),
        json!({"operation_ref":id}),
    )
    .await
}
