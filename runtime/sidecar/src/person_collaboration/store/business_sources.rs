//! Persisted source/task relations; no similarity-based person or payment matching.
use super::{business::BusinessAuthority, Store};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

impl Store {
    pub(super) async fn application_business_source(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        auth: &BusinessAuthority,
        work: Uuid,
    ) -> Result<bool> {
        let row = sqlx::query("SELECT i.application_id,a.valid,i.binding_id,i.binding_version FROM qintopia_agent_os.application_intake_states i JOIN qintopia_agent_os.welcome_applications a ON a.id=i.application_id WHERE i.tenant_key=$1 AND i.anan_work_id=$2")
            .bind(&self.tenant).bind(work).fetch_optional(&mut **tx).await?;
        let Some(row) = row else { return Ok(false) };
        ensure!(
            row.get::<Uuid, _>("binding_id") == auth.binding
                && row.get::<i64, _>("binding_version") == auth.binding_version
                && row.get::<bool, _>("valid"),
            "application_business_source_unavailable"
        );
        ensure!(
            self.application_identity_basis(tx, auth.scope, row.get("application_id"))
                .await?
                .is_some(),
            "application_readback_required"
        );
        Ok(true)
    }

    pub(super) async fn canonical_business_work(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        work: Uuid,
        input: &Value,
        key: &str,
    ) -> Result<Uuid> {
        let metadata: Value = sqlx::query_scalar(
            "SELECT metadata FROM qintopia_agent_os.work_items WHERE id=$1 FOR UPDATE",
        )
        .bind(work)
        .fetch_one(&mut **tx)
        .await?;
        if let Some(reference) = metadata.get("business_work_ref") {
            ensure!(
                key != "pms.command.CREATE_ORDER",
                "application_order_already_linked"
            );
            ensure!(
                input["orderId"] == metadata["business_order_ref"],
                "business_order_mismatch"
            );
            return Ok(serde_json::from_value(reference.clone())?);
        }
        // A task with a committed booking may only execute later stages for that
        // exact order. A missing readback must be recovered, never guessed.
        let orders: Vec<Value>=sqlx::query_scalar("SELECT readback FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND work_item_id=$2 AND operation_key='pms.command.CREATE_ORDER' AND phase IN ('completed','manual_completed')")
            .bind(&self.tenant).bind(work).fetch_all(&mut **tx).await?;
        if key != "pms.command.CREATE_ORDER" && !orders.is_empty() {
            ensure!(
                orders.len() == 1
                    && orders[0]["order"]["id"].is_string()
                    && input["orderId"] == orders[0]["order"]["id"],
                "business_order_mismatch"
            );
        }
        Ok(work)
    }

    pub(super) async fn link_business_source(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        auth: &BusinessAuthority,
        source: Uuid,
        action: Uuid,
        evidence: Uuid,
        current: &Value,
    ) -> Result<Value> {
        let application = self.application_business_source(tx, auth, source).await?;
        let payment:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND binding_id=$2 AND binding_version=$3 AND work_item_id=$4 AND source_instance=$5 AND property_id=$6 AND feed='pms.payments.v1' AND NOT baseline)")
            .bind(&self.tenant).bind(auth.binding).bind(auth.binding_version).bind(source).bind(&auth.source).bind(&auth.property).fetch_one(&mut **tx).await?;
        ensure!(application || payment, "business_work_denied");
        let row=sqlx::query("SELECT work_item_id,readback,result,phase,confirmed_by,confirmed_by_work_account FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND id=$2 AND binding_id=$3 AND binding_version=$4 AND operation_key='pms.command.CREATE_ORDER' AND phase IN ('completed','manual_completed')")
            .bind(&self.tenant).bind(action).bind(auth.binding).bind(auth.binding_version).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("completed_booking_required"))?;
        let work: Uuid = row.get("work_item_id");
        sqlx::query(
            "SELECT id FROM qintopia_agent_os.work_items WHERE id=ANY($1) ORDER BY id FOR UPDATE",
        )
        .bind(vec![source, work])
        .fetch_all(&mut **tx)
        .await?;
        let readback: Value = row.get("readback");
        let result: Value = row.get::<Option<Value>, _>("result").unwrap_or(Value::Null);
        let order = readback["order"]["id"]
            .as_str()
            .filter(|s| !s.is_empty())
            .ok_or_else(|| anyhow::anyhow!("booking_readback_required"))?;
        ensure!(
            (row.get::<String, _>("phase") == "completed"
                && result["executionStatus"] == "EXECUTED"
                && result["businessCommitted"] == true)
                || (row.get::<String, _>("phase") == "manual_completed"
                    && (row.get::<Option<Uuid>, _>("confirmed_by").is_some()
                        || row
                            .get::<Option<Uuid>, _>("confirmed_by_work_account")
                            .is_some())
                    && readback["manual_evidence"]["kind"] == "human_order_adoption"),
            "completed_booking_required"
        );
        ensure!(source != work, "source_already_canonical");
        let untouched:bool=sqlx::query_scalar("SELECT NOT EXISTS(SELECT 1 FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND work_item_id=$2)")
            .bind(&self.tenant).bind(source).fetch_one(&mut **tx).await?;
        ensure!(untouched, "source_has_independent_actions");
        ensure!(
            current["order"]["id"] == order
                && current["order"]["property_id"] == auth.property
                && current["order"]["version"].is_i64(),
            "current_order_readback_required"
        );
        let evidence_row=sqlx::query("SELECT person_id,work_account_id,gateway_key,chat_hash,content_hash,explicit_intent,observed_at,observed_at>clock_timestamp()-interval '15 minutes' AS fresh FROM qintopia_agent_os.business_turn_evidence WHERE tenant_key=$1 AND id=$2")
            .bind(&self.tenant).bind(evidence).fetch_one(&mut **tx).await?;
        let source_row=sqlx::query("SELECT metadata,brief_summary FROM qintopia_agent_os.work_items WHERE id=$1 FOR UPDATE")
            .bind(source).fetch_one(&mut **tx).await?;
        let metadata: Value = source_row.get("metadata");
        if metadata.get("business_work_ref").is_some() {
            ensure!(
                metadata["business_work_ref"] == json!(work)
                    && metadata["business_order_ref"] == order,
                "source_link_conflict"
            );
            return Ok(json!({"work_item":work,"orderId":order,"linked":true,"replayed":true}));
        }
        // Snapshot exact source revision and the current PMS order version in the
        // proposal. Natural confirmation is captured by the trusted host only.
        let application_state:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('application',a.id,'revision',a.revision,'hash',a.field_hash,'person',a.person_id) FROM qintopia_agent_os.application_intake_states i JOIN qintopia_agent_os.welcome_applications a ON a.id=i.application_id WHERE i.tenant_key=$1 AND i.anan_work_id=$2")
            .bind(&self.tenant).bind(source).fetch_optional(&mut **tx).await?;
        let person: Uuid = evidence_row
            .get::<Option<Uuid>, _>("person_id")
            .or_else(|| evidence_row.get("work_account_id"))
            .ok_or_else(|| anyhow::anyhow!("business_subject_changed"))?;
        let gateway: String = evidence_row.get("gateway_key");
        let chat: String = evidence_row.get("chat_hash");
        let snapshot = json!({"source":source,"action":action,"order":order,"version":current["order"]["version"],"status":current["order"]["status"],"application":application_state,"binding_version":auth.binding_version,"person":person,"gateway":gateway,"chat":chat});
        let hash = super::super::digest(&serde_json::to_vec(&snapshot)?);
        let expected = super::super::digest(
            format!(
                "将来源事项「{source}」关联到办理动作「{action}」的订单版本「{}」。",
                current["order"]["version"]
            )
            .as_bytes(),
        );
        let intent: Option<Value> = evidence_row.get("explicit_intent");
        let exact_after_proposal = metadata["business_link_proposal"]["hash"] == hash
            && metadata["business_link_proposal"]["created"]
                .as_str()
                .and_then(|s| s.parse::<chrono::DateTime<chrono::Utc>>().ok())
                .is_some_and(|created| {
                    evidence_row.get::<chrono::DateTime<chrono::Utc>, _>("observed_at") > created
                })
            && evidence_row.get::<String, _>("content_hash") == expected;
        let confirmed = evidence_row.get::<bool, _>("fresh")
            && (exact_after_proposal || intent.as_ref().is_some_and(|i| i["source_link"] == hash));
        if !confirmed {
            let proposal = json!({"hash":hash,"tenant":self.tenant,"person":person,"gateway":gateway,"chat":chat,"created":chrono::Utc::now().to_rfc3339(),"expires":(chrono::Utc::now()+chrono::Duration::minutes(15)).to_rfc3339()});
            sqlx::query("UPDATE qintopia_agent_os.work_items SET metadata=metadata || jsonb_build_object('business_link_proposal',$2::jsonb) WHERE id=$1")
                .bind(source).bind(proposal).execute(&mut **tx).await?;
            return Ok(
                json!({"phase":"awaiting_confirmation","source_summary":source_row.get::<String,_>("brief_summary"),"order":current["order"],"confirmation_hint":"请核对该来源属于此订单；确认关联后不新增订单或收款批准"}),
            );
        }
        let bound=sqlx::query("UPDATE qintopia_agent_os.work_items SET metadata=(metadata-'business_link_proposal') || $2 WHERE id=$1 AND work_item_type='business_operation' AND target_agent='anan' AND (metadata->>'business_work_ref' IS NULL OR (metadata->>'business_work_ref'=$3 AND metadata->>'business_order_ref'=$4))")
            .bind(source).bind(json!({"business_work_ref":work,"business_order_ref":order})).bind(work.to_string()).bind(order).execute(&mut **tx).await?;
        ensure!(bound.rows_affected() == 1, "source_link_conflict");
        super::foundation::work_event(tx,source,"business_source_linked","human",&json!({"work_item":work,"action":action,"order":order,"evidence":evidence,"is_business_confirmation":false})).await?;
        Ok(json!({"work_item":work,"orderId":order,"linked":true}))
    }
}
