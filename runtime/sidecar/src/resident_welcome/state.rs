//! Internal normalized facts. These are NOT a second PMS wire DTO: the PMS mapping
//! remains blocked until D-02/D-04/D-05 are closed in the single shared contract.
use super::{
    digest,
    store::{
        audit, authorize, complete_tx, fence_tx, operation_finish, operation_start, Actor, Claim,
        Store,
    },
};
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Clone, Serialize, Deserialize, PartialEq, Eq)]
pub enum StayState {
    Reserved,
    InHouse,
    Terminated,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Occupant {
    pub id: String,
    pub active: bool,
}
#[derive(Clone, Serialize, Deserialize)]
pub struct Snapshot {
    #[serde(default)]
    pub source_hash: Option<String>,
    pub source: String,
    pub property: String,
    pub order: String,
    pub revision: String,
    pub stay: String,
    pub state: StayState,
    pub inventory_reserved: bool,
    pub current_arrangement: bool,
    pub building: String,
    pub occupants: Vec<Occupant>,
    pub related_revisions: std::collections::BTreeMap<String, String>,
    pub observed_at: DateTime<Utc>,
    pub business_date: chrono::NaiveDate,
}
impl Snapshot {
    fn validate(&self) -> Result<()> {
        ensure!(
            super::protocol::decimal(&self.revision)
                && self
                    .related_revisions
                    .values()
                    .all(|v| super::protocol::decimal(v)),
            "invalid_revision"
        );
        ensure!(
            [&self.source, &self.property, &self.order, &self.stay]
                .iter()
                .all(|s| super::protocol::reference(s)),
            "invalid_source_ref"
        );
        let mut seen = std::collections::HashSet::new();
        ensure!(
            self.occupants.len() <= 100
                && self
                    .occupants
                    .iter()
                    .all(|o| super::protocol::reference(&o.id) && seen.insert(&o.id)),
            "invalid_occupants"
        );
        Ok(())
    }
    fn stable_hash(&self) -> Result<String> {
        if let Some(hash) = &self.source_hash {
            return Ok(hash.clone());
        }
        let mut value = serde_json::to_value(self)?;
        for field in [
            "observed_at",
            "business_date",
            "current_arrangement",
            "building",
        ] {
            value.as_object_mut().unwrap().remove(field);
        }
        // The internal fixture model freezes stable occupancy facts only. PMS wire
        // hash must be checked by its versioned adapter, not replaced by this hash.
        Ok(digest(&serde_json::to_vec(&value)?))
    }
}
pub fn compare_revision(a: &str, b: &str) -> std::cmp::Ordering {
    a.len().cmp(&b.len()).then_with(|| a.cmp(b))
}

impl Store {
    pub async fn consume_entity(&self, claim: &Claim, projection: &Value) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        fence_tx(&mut tx, claim).await?;
        let envelope: Value =
            sqlx::query_scalar("SELECT envelope FROM qintopia_agent_os.welcome_inbox WHERE id=$1")
                .bind(claim.inbox_id)
                .fetch_one(&mut *tx)
                .await?;
        let e: super::protocol::Envelope = serde_json::from_value(envelope)?;
        ensure!(
            matches!(e.aggregate_type.as_str(), "member" | "inventory_unit"),
            "entity_event_required"
        );
        let p = super::projection::entity(
            projection,
            &e.source_instance,
            &e.property_id,
            &e.aggregate_type,
            &e.aggregate_id,
        )?;
        let revision = p[if e.aggregate_type == "member" {
            "member_revision"
        } else {
            "inventory_revision"
        }]
        .as_str()
        .unwrap();
        ensure!(
            compare_revision(revision, &e.aggregate_revision).is_ge(),
            "entity_projection_behind"
        );
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!(
                "welcome-entity/{}/{}/{}/{}",
                e.source_instance, e.property_id, e.aggregate_type, e.aggregate_id
            ))
            .execute(&mut *tx)
            .await?;
        let old=sqlx::query("SELECT revision::text,projection_hash FROM qintopia_agent_os.welcome_source_versions WHERE source_instance=$1 AND property_id=$2 AND aggregate_type=$3 AND aggregate_id=$4 FOR UPDATE")
            .bind(&e.source_instance).bind(&e.property_id).bind(&e.aggregate_type).bind(&e.aggregate_id).fetch_optional(&mut *tx).await?;
        if let Some(old) = old {
            let old_revision: String = old.get("revision");
            if compare_revision(revision, &old_revision).is_le() {
                let conflict = revision == old_revision
                    && p["projection_hash"] != old.get::<String, _>("projection_hash");
                if conflict {
                    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET invalidated=true,conflicted=true WHERE source_instance=$1 AND property_id=$2 AND aggregate_type=$3 AND aggregate_id=$4")
                        .bind(&e.source_instance).bind(&e.property_id).bind(&e.aggregate_type).bind(&e.aggregate_id).execute(&mut *tx).await?;
                    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET invalidated=true WHERE source_instance=$1 AND property_id=$2 AND aggregate_type='order' AND projection->'related_revisions' ? $3")
                        .bind(&e.source_instance).bind(&e.property_id).bind(format!("{}/{}",e.aggregate_type,e.aggregate_id)).execute(&mut *tx).await?;
                }
                complete_tx(&mut tx, claim, conflict).await?;
                tx.commit().await?;
                return Ok(());
            }
        }
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_source_versions(source_instance,property_id,aggregate_type,aggregate_id,revision,projection_hash,projection,invalidated) VALUES ($1,$2,$3,$4,$5::numeric,$6,$7,$8) ON CONFLICT (source_instance,property_id,aggregate_type,aggregate_id) DO UPDATE SET revision=EXCLUDED.revision,projection_hash=EXCLUDED.projection_hash,projection=EXCLUDED.projection,invalidated=EXCLUDED.invalidated")
            .bind(&e.source_instance).bind(&e.property_id).bind(&e.aggregate_type).bind(&e.aggregate_id).bind(revision).bind(p["projection_hash"].as_str().unwrap()).bind(&p).bind(p["resource_state"]=="tombstone").execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET conflicted=false WHERE source_instance=$1 AND property_id=$2 AND aggregate_type=$3 AND aggregate_id=$4")
            .bind(&e.source_instance).bind(&e.property_id).bind(&e.aggregate_type).bind(&e.aggregate_id).execute(&mut *tx).await?;
        let key = format!("{}/{}", e.aggregate_type, e.aggregate_id);
        sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET invalidated=true WHERE source_instance=$1 AND property_id=$2 AND aggregate_type='order' AND projection->'related_revisions' ? $3")
            .bind(&e.source_instance).bind(&e.property_id).bind(key).execute(&mut *tx).await?;
        let cases:Vec<Uuid>=sqlx::query_scalar("SELECT c.id FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id WHERE c.source_instance=$1 AND c.property_id=$2 AND v.invalidated")
            .bind(&e.source_instance).bind(&e.property_id).fetch_all(&mut *tx).await?;
        for case in cases {
            evaluate_tx(&mut tx, case).await?;
        }
        complete_tx(&mut tx, claim, false).await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn consume_snapshot(&self, claim: &Claim, snapshot: &Snapshot) -> Result<Vec<Uuid>> {
        self.apply_snapshot(Some(claim), snapshot).await
    }

    /// Baseline and readback never invent an upstream event or grant admission.
    /// The caller must have durably saved the readback request before this call.
    pub(crate) async fn apply_snapshot(
        &self,
        claim: Option<&Claim>,
        snapshot: &Snapshot,
    ) -> Result<Vec<Uuid>> {
        snapshot.validate()?;
        let mut tx = self.pool.begin().await?;
        let e = if let Some(claim) = claim {
            fence_tx(&mut tx, claim).await?;
            let envelope: Value = sqlx::query_scalar(
                "SELECT envelope FROM qintopia_agent_os.welcome_inbox WHERE id=$1",
            )
            .bind(claim.inbox_id)
            .fetch_one(&mut *tx)
            .await?;
            let e: super::protocol::Envelope = serde_json::from_value(envelope)?;
            ensure!(
                e.source_instance == snapshot.source
                    && e.property_id == snapshot.property
                    && e.aggregate_type == "order"
                    && e.aggregate_id == snapshot.order,
                "projection_scope_mismatch"
            );
            ensure!(
                compare_revision(&snapshot.revision, &e.aggregate_revision).is_ge(),
                "projection_behind_event"
            );
            Some(e)
        } else {
            None
        };
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!(
                "welcome-order/{}/{}/{}",
                snapshot.source, snapshot.property, snapshot.order
            ))
            .execute(&mut *tx)
            .await?;
        let existing=sqlx::query("SELECT revision::text,projection_hash,projection,conflicted FROM qintopia_agent_os.welcome_source_versions WHERE source_instance=$1 AND property_id=$2 AND aggregate_type='order' AND aggregate_id=$3 FOR UPDATE")
            .bind(&snapshot.source).bind(&snapshot.property).bind(&snapshot.order).fetch_optional(&mut *tx).await?;
        let hash = snapshot.stable_hash()?;
        let mut stale = false;
        let mut building_changed = false;
        if let Some(row) = existing {
            let old: Snapshot = serde_json::from_value(row.get("projection"))?;
            building_changed = old.building != snapshot.building;
            // A higher order revision may replace an inventory reference; a
            // higher related revision may replace a parent/reference set. Missing
            // obsolete keys alone must not make every later readback stale.
            let reference_change_versioned = compare_revision(&snapshot.revision, &old.revision)
                .is_gt()
                || old.related_revisions.iter().any(|(key, v)| {
                    snapshot
                        .related_revisions
                        .get(key)
                        .is_some_and(|n| compare_revision(n, v).is_gt())
                });
            stale = compare_revision(&snapshot.revision, &old.revision).is_lt()
                || old.related_revisions.iter().any(|(key, v)| {
                    snapshot
                        .related_revisions
                        .get(key)
                        .map_or(!reference_change_versioned, |new| {
                            compare_revision(new, v).is_lt()
                        })
                })
                || snapshot.observed_at < old.observed_at;
            if !stale
                && snapshot.revision == old.revision
                && snapshot.related_revisions == old.related_revisions
                && (row.get::<String, _>("projection_hash") != hash
                    || row.get::<bool, _>("conflicted"))
            {
                sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET invalidated=true,conflicted=true WHERE source_instance=$1 AND property_id=$2 AND aggregate_type='order' AND aggregate_id=$3")
                    .bind(&snapshot.source).bind(&snapshot.property).bind(&snapshot.order).execute(&mut *tx).await?;
                if let Some(claim) = claim {
                    complete_tx(&mut tx, claim, true).await?;
                } else {
                    tx.commit().await?;
                    anyhow::bail!("readback_projection_conflict");
                }
                tx.commit().await?;
                return Ok(vec![]);
            }
        }
        if stale {
            if let Some(claim) = claim {
                complete_tx(&mut tx, claim, false).await?;
            }
            tx.commit().await?;
            return Ok(vec![]);
        }
        if building_changed {
            sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET version=version+1 WHERE source_instance=$1 AND property_id=$2 AND order_id=$3")
                .bind(&snapshot.source).bind(&snapshot.property).bind(&snapshot.order).execute(&mut *tx).await?;
            sqlx::query("UPDATE qintopia_agent_os.welcome_artifact_bindings b SET revoked_at=now() FROM qintopia_agent_os.welcome_cases c WHERE b.case_id=c.id AND c.source_instance=$1 AND c.property_id=$2 AND c.order_id=$3")
                .bind(&snapshot.source).bind(&snapshot.property).bind(&snapshot.order).execute(&mut *tx).await?;
            sqlx::query("UPDATE qintopia_agent_os.welcome_actions a SET status='cancelled',version=a.version+1 FROM qintopia_agent_os.welcome_cases c WHERE a.case_id=c.id AND c.source_instance=$1 AND c.property_id=$2 AND c.order_id=$3 AND a.status IN ('prepared','claimed','retryable')")
                .bind(&snapshot.source).bind(&snapshot.property).bind(&snapshot.order).execute(&mut *tx).await?;
        }
        for (key, revision) in &snapshot.related_revisions {
            let (kind, id) = key
                .split_once('/')
                .ok_or_else(|| anyhow::anyhow!("invalid_version_vector"))?;
            let current:Option<String>=sqlx::query_scalar("SELECT revision::text FROM qintopia_agent_os.welcome_source_versions WHERE source_instance=$1 AND property_id=$2 AND aggregate_type=$3 AND aggregate_id=$4")
                .bind(&snapshot.source).bind(&snapshot.property).bind(kind).bind(id).fetch_optional(&mut *tx).await?;
            ensure!(
                current.is_none_or(|current| compare_revision(revision, &current).is_ge()),
                "related_projection_behind"
            );
            let conflicted:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_source_versions WHERE source_instance=$1 AND property_id=$2 AND aggregate_type=$3 AND aggregate_id=$4 AND (conflicted OR invalidated))")
                .bind(&snapshot.source).bind(&snapshot.property).bind(kind).bind(id).fetch_one(&mut *tx).await?;
            ensure!(!conflicted, "related_entity_requires_correction");
        }
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_source_versions(source_instance,property_id,aggregate_type,aggregate_id,revision,projection_hash,projection) VALUES ($1,$2,'order',$3,$4::numeric,$5,$6) ON CONFLICT (source_instance,property_id,aggregate_type,aggregate_id) DO UPDATE SET revision=EXCLUDED.revision,projection_hash=EXCLUDED.projection_hash,projection=EXCLUDED.projection")
            .bind(&snapshot.source).bind(&snapshot.property).bind(&snapshot.order).bind(&snapshot.revision).bind(&hash).bind(serde_json::to_value(snapshot)?).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET invalidated=false,conflicted=false WHERE source_instance=$1 AND property_id=$2 AND aggregate_type='order' AND aggregate_id=$3")
            .bind(&snapshot.source).bind(&snapshot.property).bind(&snapshot.order).execute(&mut *tx).await?;
        let source=sqlx::query("SELECT mode,rebuilding,enabled,admission_after FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2 FOR SHARE")
            .bind(&snapshot.source).bind(&snapshot.property).fetch_one(&mut *tx).await?;
        let admitted = e.as_ref().is_some_and(|e| {
            e.origin == "live"
                && matches!(
                    e.event_type.as_str(),
                    "pms.order.created" | "pms.stay.checked_in" | "pms.stay.arrangement_changed"
                )
                && source
                    .get::<Option<DateTime<Utc>>, _>("admission_after")
                    .is_some_and(|t| e.recorded_at >= t)
        }) && source.get::<String, _>("mode") == "synthetic"
            && !source.get::<bool, _>("rebuilding")
            && source.get::<bool, _>("enabled");
        let mut cases = vec![];
        let namespace = format!("pms/{}/{}/occupant", snapshot.source, snapshot.property);
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_identity_scopes(namespace,source_instance,property_id) VALUES ($1,$2,$3) ON CONFLICT DO NOTHING")
            .bind(&namespace).bind(&snapshot.source).bind(&snapshot.property).execute(&mut *tx).await?;
        for occupant in &snapshot.occupants {
            // A source observation creates only a pending typed identity. It does
            // not turn a booking member or a display name into a confirmed Person.
            sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref) VALUES ($1,'pms_occupant',$2) ON CONFLICT DO NOTHING")
                .bind(&namespace).bind(&occupant.id).execute(&mut *tx).await?;
            let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_cases(source_instance,property_id,order_id,stay_id,occupant_id,admitted,manual_hold) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (source_instance,property_id,stay_id,occupant_id) DO UPDATE SET order_id=EXCLUDED.order_id,manual_hold=qintopia_agent_os.welcome_cases.manual_hold OR EXCLUDED.manual_hold RETURNING id")
                .bind(&snapshot.source).bind(&snapshot.property).bind(&snapshot.order).bind(&snapshot.stay).bind(&occupant.id).bind(admitted)
                .bind(!occupant.active || e.as_ref().is_some_and(|e| matches!(e.event_type.as_str(),"pms.stay.check_in_revoked"|"pms.stay.check_out_revoked"|"pms.order.occupants_changed")))
                .fetch_one(&mut *tx).await?;
            cases.push(id);
        }
        // Missing/removed occupants and terminal facts cancel eligibility. Retain
        // cases/actions as tombstones; a later identity merge cannot recreate them.
        let active: Vec<&str> = snapshot
            .occupants
            .iter()
            .filter(|o| o.active)
            .map(|o| o.id.as_str())
            .collect();
        sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET manual_hold=true,version=version+1 WHERE source_instance=$1 AND property_id=$2 AND order_id=$3 AND NOT manual_hold AND (NOT occupant_id=ANY($4) OR $5)")
            .bind(&snapshot.source).bind(&snapshot.property).bind(&snapshot.order).bind(&active).bind(snapshot.state==StayState::Terminated).execute(&mut *tx).await?;
        let affected:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.welcome_cases WHERE source_instance=$1 AND property_id=$2 AND order_id=$3")
            .bind(&snapshot.source).bind(&snapshot.property).bind(&snapshot.order).fetch_all(&mut *tx).await?;
        for id in &affected {
            evaluate_tx(&mut tx, *id).await?;
        }
        if let Some(claim) = claim {
            complete_tx(&mut tx, claim, false).await?;
        }
        tx.commit().await?;
        for case in &affected {
            self.reconcile_case(*case).await?;
        }
        Ok(cases)
    }

    pub async fn bind_case(
        &self,
        actor: &Actor,
        operation: Uuid,
        case: Uuid,
        link: Uuid,
        application: Uuid,
        expected: i64,
    ) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        authorize(&mut tx, actor, "identity", None).await?;
        let request = json!(["bind_case", case, link, application, expected]);
        if let Some(result) = operation_start(&mut tx, actor, operation, &request).await? {
            return Ok(result);
        }
        let row=sqlx::query("SELECT c.version,l.person_id,l.version AS link_version FROM qintopia_agent_os.welcome_cases c JOIN qintopia_identity.source_identity_links l ON l.id=$2 AND l.subject_type='pms_occupant' AND l.source_ref=c.occupant_id AND l.status='confirmed' JOIN qintopia_agent_os.welcome_identity_scopes s ON s.namespace=l.namespace AND s.source_instance=c.source_instance AND s.property_id=c.property_id JOIN qintopia_agent_os.welcome_applications a ON a.id=$3 AND a.source_instance=c.source_instance AND a.person_id=l.person_id AND a.valid WHERE c.id=$1 AND c.source_instance=$4 AND c.property_id=$5 FOR UPDATE OF c,l,a")
            .bind(case).bind(link).bind(application).bind(&actor.source).bind(&actor.property).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<i64, _>("version") == expected,
            "case_version_conflict"
        );
        sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET person_id=$2,identity_link_id=$3,identity_version=$4,application_id=$5,version=version+1,manual_hold=false WHERE id=$1")
            .bind(case).bind(row.get::<Uuid,_>("person_id")).bind(link).bind(row.get::<i64,_>("link_version")).bind(application).execute(&mut *tx).await?;
        evaluate_tx(&mut tx, case).await?;
        let result = json!({"case_ref":case,"version":expected+1});
        operation_finish(&mut tx, actor, operation, &request, &result).await?;
        tx.commit().await?;
        self.reconcile_case(case).await?;
        Ok(result)
    }

    pub async fn evaluate(&self, case: Uuid) -> Result<Readiness> {
        let mut tx = self.pool.begin().await?;
        let r = evaluate_tx(&mut tx, case).await?;
        tx.commit().await?;
        Ok(r)
    }
}

#[derive(Clone, Serialize)]
pub struct Readiness {
    pub card_ready: bool,
    pub reasons: Vec<String>,
}
pub(crate) async fn evaluate_tx(
    tx: &mut Transaction<'_, Postgres>,
    case: Uuid,
) -> Result<Readiness> {
    let row=sqlx::query("SELECT c.*,a.valid,a.consent_active,a.person_id AS application_person,l.status AS link_status,l.version AS link_version,l.person_id AS link_person,v.projection,s.rebuilding,s.enabled,s.mode FROM qintopia_agent_os.welcome_cases c LEFT JOIN qintopia_agent_os.welcome_applications a ON a.id=c.application_id AND a.source_instance=c.source_instance LEFT JOIN qintopia_identity.source_identity_links l ON l.id=c.identity_link_id LEFT JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id JOIN qintopia_agent_os.welcome_sources s ON s.source_instance=c.source_instance AND s.property_id=c.property_id WHERE c.id=$1 FOR UPDATE OF c")
        .bind(case).fetch_one(&mut **tx).await?;
    let mut reasons = vec![];
    let invalidated:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id WHERE c.id=$1 AND v.invalidated)")
        .bind(case).fetch_one(&mut **tx).await?;
    if invalidated {
        reasons.push("source_projection_invalidated".into());
    }
    if row.get::<bool, _>("manual_hold") {
        reasons.push("manual_hold".into())
    }
    if !row.get::<bool, _>("admitted") {
        reasons.push("not_admitted".into())
    }
    if row.get::<bool, _>("rebuilding") || !row.get::<bool, _>("enabled") {
        reasons.push("source_not_ready".into())
    }
    if row.get::<Option<bool>, _>("valid") != Some(true) {
        reasons.push("valid_application_required".into())
    }
    if row.get::<Option<bool>, _>("consent_active") != Some(true) {
        reasons.push("consent_required".into())
    }
    let person = row.get::<Option<Uuid>, _>("person_id");
    if let Some(person) = person {
        let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.persons WHERE id=$1 AND status='active')").bind(person).fetch_one(&mut **tx).await?;
        if !active {
            reasons.push("person_inactive".into());
        }
    }
    if person.is_none()
        || row.get::<Option<String>, _>("link_status").as_deref() != Some("confirmed")
        || row.get::<Option<i64>, _>("identity_version")
            != row.get::<Option<i64>, _>("link_version")
        || row.get::<Option<Uuid>, _>("link_person") != person
        || row.get::<Option<Uuid>, _>("application_person") != person
    {
        reasons.push("identity_binding_required".into());
    }
    let snapshot: Option<Snapshot> = row
        .get::<Option<Value>, _>("projection")
        .map(serde_json::from_value)
        .transpose()?;
    if snapshot.as_ref().is_none_or(|s| {
        s.stay != row.get::<String, _>("stay_id")
            || !s.current_arrangement
            || !s.inventory_reserved
            || s.state == StayState::Terminated
            || !s
                .occupants
                .iter()
                .any(|o| o.id == row.get::<String, _>("occupant_id") && o.active)
    }) {
        reasons.push("applicable_arrangement_required".into());
    }
    let ready = reasons.is_empty();
    sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET readiness_reasons=$2 WHERE id=$1")
        .bind(case)
        .bind(json!(reasons))
        .execute(&mut **tx)
        .await?;
    if !ready {
        sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status='cancelled',version=version+1 WHERE case_id=$1 AND status IN ('prepared','claimed','retryable')").bind(case).execute(&mut **tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items w SET status='cancelled',claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL FROM qintopia_agent_os.welcome_actions a WHERE a.work_item_id=w.id AND a.case_id=$1 AND a.status='cancelled'").bind(case).execute(&mut **tx).await?;
    }
    if row.get::<Value, _>("readiness_reasons") != json!(reasons) {
        audit(
            tx,
            None,
            "welcome_case_evaluated",
            None,
            json!({"case_ref":case,"card_ready":ready}),
        )
        .await?;
    }
    Ok(Readiness {
        card_ready: ready,
        reasons,
    })
}
