//! Reconcile only an already-linked source application to existing welcome cases.
use super::Store;
use crate::person_collaboration::welcome_model::ReviewOpen;
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

impl Store {
    pub(crate) async fn application_reconcile_welcome(
        &self,
        binding: Uuid,
        alias: &str,
        record: &str,
    ) -> Result<Value> {
        super::applications::references(alias, record)?;
        let (mut tx, _, _) = self.begin().await?;
        let row=sqlx::query("SELECT i.application_id,i.source_instance,i.property_id,b.scope_id,a.valid FROM qintopia_agent_os.application_intake_states i JOIN qintopia_agent_os.business_property_bindings b ON b.tenant_key=i.tenant_key AND b.id=i.binding_id AND b.version=i.binding_version AND b.source_instance=i.source_instance AND b.property_id=i.property_id JOIN qintopia_agent_os.welcome_applications a ON a.id=i.application_id AND a.source_instance=i.source_instance AND a.resource_ref=i.resource_alias AND a.record_ref=i.record_ref WHERE i.tenant_key=$1 AND i.binding_id=$2 AND i.resource_alias=$3 AND i.record_ref=$4 AND b.active FOR SHARE OF i,b,a")
            .bind(&self.tenant).bind(binding).bind(alias).bind(record).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            return Ok(json!({"status":"awaiting_readback","work_items":[]}));
        };
        let application: Uuid = row.get("application_id");
        let scope: Uuid = row.get("scope_id");
        if self
            .application_identity_basis(&mut tx, scope, application)
            .await?
            .is_none()
        {
            return Ok(json!({"status":"awaiting_readback","work_items":[]}));
        }
        if !row.get::<bool, _>("valid") {
            return Ok(json!({"status":"source_withdrawn","work_items":[]}));
        }
        let cases:Vec<Uuid>=sqlx::query_scalar("SELECT c.id FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id JOIN qintopia_agent_os.welcome_sources s ON s.source_instance=c.source_instance AND s.property_id=c.property_id WHERE c.application_id=$1 AND EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_applications a JOIN qintopia_identity.source_identity_links l ON l.id=c.identity_link_id JOIN qintopia_agent_os.welcome_identity_scopes i ON i.namespace=l.namespace AND i.source_instance=c.source_instance AND i.property_id=c.property_id JOIN qintopia_identity.persons p ON p.id=l.person_id AND p.status='active' WHERE a.id=c.application_id AND a.person_id=c.person_id AND l.person_id=c.person_id AND l.status='confirmed' AND l.version=c.identity_version AND l.subject_type='pms_occupant' AND l.source_ref=c.occupant_id) AND c.source_instance=$2 AND c.property_id=$3 AND c.admitted AND NOT v.invalidated AND s.mode=$6 AND s.enabled AND NOT s.rebuilding AND v.projection->>'state' IN ('Reserved','InHouse') AND v.projection->>'current_arrangement'='true' AND v.projection->>'inventory_reserved'='true' AND v.projection->>'stay'=c.stay_id AND EXISTS(SELECT 1 FROM jsonb_array_elements(v.projection->'occupants') o WHERE o->>'id'=c.occupant_id AND o->>'active'='true') AND EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_foundation_targets f JOIN qintopia_agent_os.welcome_targets t ON t.id=f.target_id WHERE f.tenant_key=$4 AND f.scope_id=$5 AND t.source_instance=c.source_instance AND t.property_id=c.property_id AND t.enabled AND (t.kind<>'building' OR t.building_code=v.projection->>'building')) ORDER BY c.id LIMIT 101 FOR SHARE OF c,v,s")
            .bind(application).bind(row.get::<String,_>("source_instance")).bind(row.get::<String,_>("property_id")).bind(&self.tenant).bind(scope).bind(self.mode.as_str()).fetch_all(&mut *tx).await?;
        ensure!(cases.len() <= 100, "application_welcome_case_limit");
        if cases.is_empty() {
            return Ok(json!({"status":"awaiting_reliable_stay_link","work_items":[]}));
        }
        let configured:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_review_settings WHERE tenant_key=$1 AND scope_id=$2)")
            .bind(&self.tenant).bind(scope).fetch_one(&mut *tx).await?;
        if !configured {
            return Ok(json!({"status":"awaiting_operations_configuration","work_items":[]}));
        }
        let mut requests = Vec::new();
        for case in cases {
            let work = crate::resident_welcome::store::create_work(
                &mut tx,
                &format!("application-welcome/{}/{case}/{application}", self.tenant),
                "welcome_event",
                "anan",
                "awaiting_review",
            )
            .await?;
            let payload = json!({"tenant_key":self.tenant,"scope_ref":scope,"case_ref":case,"application_ref":application});
            let bound = sqlx::query("UPDATE qintopia_agent_os.work_items SET payload=$2,metadata=metadata || $3 WHERE id=$1 AND work_item_type='welcome_event' AND requester_agent='anan' AND target_agent='anan' AND (payload='{}'::jsonb OR payload=$2)")
                .bind(work).bind(payload)
                .bind(json!({"local_only":!self.is_live(),"application_link_from_store":true}))
                .execute(&mut *tx).await?;
            ensure!(
                bound.rows_affected() == 1,
                "application_welcome_work_conflict"
            );
            let artifacts:Vec<Uuid>=sqlx::query_scalar("SELECT DISTINCT ON (ar.artifact_type) ar.id FROM qintopia_agent_os.artifacts ar JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=ar.id JOIN qintopia_agent_os.welcome_applications a ON a.id=b.application_id JOIN qintopia_agent_os.welcome_cases c ON c.id=b.case_id WHERE b.case_id=$1 AND b.application_id=$2 AND b.revoked_at IS NULL AND b.application_revision=a.revision AND b.consent_version=a.consent_version AND b.case_version=c.version AND ar.content_hash IS NOT NULL ORDER BY ar.artifact_type,ar.created_at DESC,ar.id DESC LIMIT 8")
                .bind(case).bind(application).fetch_all(&mut *tx).await?;
            requests.push(ReviewOpen {
                work_item: work,
                scope,
                case_ref: case,
                application,
                artifacts,
            });
        }
        tx.commit().await?;
        let mut results = Vec::new();
        for request in requests {
            // A failure after task creation is recoverable by the same source wake:
            // next run derives the same task and the public operation is idempotent.
            results.push(self.welcome_review_open_task(None, &request).await?);
        }
        Ok(
            json!({"status":"awaiting_operations_confirmation","work_items":results,"external_send":false}),
        )
    }
}
