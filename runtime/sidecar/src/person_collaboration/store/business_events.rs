//! Host-only payment inbox. Events never impersonate people or authorize PMS writes.
//! Push never advances a contiguous feed checkpoint.
use super::Store;
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub(crate) const PAYMENT_SCHEMA: &str = "pms.payments.v1";
pub(crate) fn cursor(value: &str) -> Result<i64> {
    ensure!(
        !value.is_empty()
            && value.len() <= 19
            && value.bytes().all(|b| b.is_ascii_digit())
            && (value == "0" || !value.starts_with('0')),
        "invalid_feed_cursor"
    );
    let n: i64 = value
        .parse()
        .map_err(|_| anyhow::anyhow!("invalid_feed_cursor"))?;
    ensure!(n >= 0, "invalid_feed_cursor");
    Ok(n)
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PaymentEvent {
    pub event_id: String,
    pub bill_id: String,
    pub kind: String,
    pub event_type: String,
    pub occurred_at: DateTime<Utc>,
    pub sequence: String,
}
impl PaymentEvent {
    pub(crate) fn validate(&self) -> Result<()> {
        ensure!(
            cursor(&self.sequence)? > 0
                && !self.bill_id.is_empty()
                && self.bill_id.len() <= 160
                && self
                    .bill_id
                    .bytes()
                    .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-')
                && self.event_id == format!("payment:{}:{}", self.bill_id, self.event_type)
                && matches!(self.event_type.as_str(), "DISCOVERED" | "MATCHED")
                && matches!(self.kind.as_str(), "COLLECTION" | "REFUND"),
            "invalid_feed_event"
        );
        Ok(())
    }
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PaymentPage {
    pub schema_version: String,
    pub property_id: String,
    pub events: Vec<PaymentEvent>,
    pub next_cursor: String,
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
pub(crate) struct PaymentHead {
    pub schema_version: String,
    pub source_instance: String,
    pub property_id: String,
    pub binding_version: i64,
    pub head_cursor: String,
}
struct Binding {
    source: String,
    property: String,
    version: i64,
}
async fn binding_in(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    binding: Uuid,
) -> Result<Binding> {
    let r=sqlx::query("SELECT source_instance,property_id,version FROM qintopia_agent_os.business_property_bindings WHERE tenant_key=$1 AND id=$2 AND active FOR SHARE")
        .bind(tenant).bind(binding).fetch_optional(&mut **tx).await?
        .ok_or_else(|| anyhow::anyhow!("business_binding_unavailable"))?;
    Ok(Binding {
        source: r.get("source_instance"),
        property: r.get("property_id"),
        version: r.get("version"),
    })
}
async fn checkpoint_in(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    binding: Uuid,
    b: &Binding,
) -> Result<(String, i64)> {
    let r=sqlx::query("SELECT cursor_value,baseline_cursor,source_instance,property_id,binding_version FROM qintopia_agent_os.business_feed_checkpoints WHERE tenant_key=$1 AND binding_id=$2 AND feed=$3 FOR UPDATE")
        .bind(tenant).bind(binding).bind(PAYMENT_SCHEMA).fetch_optional(&mut **tx).await?
        .ok_or_else(|| anyhow::anyhow!("source_activation_snapshot_required"))?;
    ensure!(
        r.get::<String, _>("source_instance") == b.source
            && r.get::<String, _>("property_id") == b.property
            && r.get::<i64, _>("binding_version") == b.version,
        "business_binding_changed"
    );
    Ok((
        r.get("cursor_value"),
        cursor(&r.get::<String, _>("baseline_cursor"))?,
    ))
}

// The same typed event is used by push and feed. No transport-envelope byte hash here.
async fn accept_in(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    binding: Uuid,
    b: &Binding,
    baseline: i64,
    event: &PaymentEvent,
    live: bool,
) -> Result<Value> {
    event.validate()?;
    let payload = serde_json::to_value(event)?;
    let hash = crate::person_collaboration::digest(&serde_json::to_vec(&payload)?);
    if let Some(r)=sqlx::query("SELECT id,event_hash FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND source_instance=$2 AND property_id=$3 AND feed=$4 AND event_id=$5")
        .bind(tenant).bind(&b.source).bind(&b.property).bind(PAYMENT_SCHEMA).bind(&event.event_id).fetch_optional(&mut **tx).await? {
        ensure!(r.get::<String,_>("event_hash")==hash,"feed_event_conflict");
        return Ok(json!({"event_id":event.event_id,"status":"duplicate","receipt_id":r.get::<Uuid,_>("id")}));
    }
    let sequence_used:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND source_instance=$2 AND property_id=$3 AND feed=$4 AND source_revision=$5)")
        .bind(tenant).bind(&b.source).bind(&b.property).bind(PAYMENT_SCHEMA).bind(&event.sequence).fetch_one(&mut **tx).await?;
    ensure!(!sequence_used, "feed_sequence_conflict");
    let historical = cursor(&event.sequence)? <= baseline;
    let previous=sqlx::query("SELECT work_item_id,payload->>'eventType' AS event_type FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND source_instance=$2 AND property_id=$3 AND feed=$4 AND subject_ref=$5")
        .bind(tenant).bind(&b.source).bind(&b.property).bind(PAYMENT_SCHEMA).bind(&event.bill_id).fetch_all(&mut **tx).await?;
    let matched = previous
        .iter()
        .any(|r| r.get::<String, _>("event_type") == "MATCHED");
    let prior_work = previous
        .iter()
        .find_map(|r| r.get::<Option<Uuid>, _>("work_item_id"));
    let work = if let Some(work) = prior_work {
        Some(work)
    } else if !historical
        && !matched
        && event.kind == "COLLECTION"
        && event.event_type == "DISCOVERED"
    {
        let work_key = crate::person_collaboration::digest(&serde_json::to_vec(&json!([
            tenant,
            b.source,
            b.property,
            event.bill_id
        ]))?);
        let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.work_items(work_item_type,status,requester_agent,target_agent,capability_key,brief_summary,purpose,dedupe_key,idempotency_key,payload,metadata) VALUES('business_operation','awaiting_review','anan','anan','anan.pms','新收款待核对',$1,$2,$2,$3,$4) RETURNING id")
            .bind(if live { "hospitality_business" } else { "synthetic_business" })
            .bind(format!("payment/{work_key}"))
            .bind(json!({"binding":binding,"bill_ref":event.bill_id,"contact_status":"pending_verified_channel","readback_required":true,"source_matched":false}))
            .bind(if live { json!({"mode":"live","event_is_not_authority":true}) } else { json!({"local_only":true,"event_is_not_authority":true}) })
            .fetch_one(&mut **tx).await?;
        Some(id)
    } else {
        None
    };
    let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_event_inbox(tenant_key,binding_id,source_instance,property_id,binding_version,feed,event_id,subject_ref,source_revision,event_hash,payload,baseline,work_item_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13) RETURNING id")
        .bind(tenant).bind(binding).bind(&b.source).bind(&b.property).bind(b.version).bind(PAYMENT_SCHEMA).bind(&event.event_id)
        .bind(&event.bill_id).bind(&event.sequence).bind(hash).bind(payload).bind(historical).bind(work).fetch_one(&mut **tx).await?;
    if let Some(work) = work {
        if event.event_type == "MATCHED" {
            // Do not manufacture a financial receipt or overwrite an action ledger's status.
            sqlx::query("UPDATE qintopia_agent_os.work_items SET payload=payload || '{\"source_matched\":true,\"readback_required\":true,\"contact_status\":\"suppressed_pending_readback\"}'::jsonb,updated_at=clock_timestamp() WHERE id=$1")
                .bind(work).execute(&mut **tx).await?;
        }
        super::foundation::work_event(
            tx,
            work,
            "payment_source_observed",
            "anan",
            &json!({"event_ref":event.event_id,"readback_required":true}),
        )
        .await?;
    }
    Ok(json!({"event_id":event.event_id,"status":"accepted","receipt_id":id}))
}
impl Store {
    /// Resolve only the newly accepted, non-baseline receipt from this live binding.
    pub(crate) async fn business_payment_receipt_workitem(
        &self,
        binding: Uuid,
        receipt: Uuid,
    ) -> Result<Option<Uuid>> {
        ensure!(self.is_live(), "live_tenant_required");
        sqlx::query_scalar(
            r#"
            SELECT e.work_item_id FROM qintopia_agent_os.business_event_inbox e
            JOIN qintopia_agent_os.business_property_bindings b
              ON b.id=e.binding_id AND b.tenant_key=e.tenant_key
              AND b.source_instance=e.source_instance AND b.property_id=e.property_id
              AND b.version=e.binding_version AND b.active
            WHERE e.id=$1 AND e.tenant_key=$2 AND e.binding_id=$3
              AND e.feed=$4 AND NOT e.baseline
              AND e.payload->>'kind'='COLLECTION'
              AND e.payload->>'eventType'='DISCOVERED'
              AND e.work_item_id IS NOT NULL
            "#,
        )
        .bind(receipt)
        .bind(&self.tenant)
        .bind(binding)
        .bind(PAYMENT_SCHEMA)
        .fetch_optional(&self.pool)
        .await
        .map_err(Into::into)
    }

    /// A host may inspect only the live payment work attached to its current binding.
    /// The single SELECT keeps source, gateway, and work state in one read snapshot.
    pub(crate) async fn business_payment_workitem_read(
        &self,
        gateway: &str,
        binding: Uuid,
        work_item: Uuid,
    ) -> Result<Value> {
        ensure!(self.is_live(), "live_tenant_required");
        let row = sqlx::query(
            r#"
            SELECT w.status,w.payload,
              EXISTS(
                SELECT 1 FROM qintopia_agent_os.business_event_inbox matched
                WHERE matched.tenant_key=e.tenant_key
                  AND matched.binding_id=e.binding_id
                  AND matched.source_instance=e.source_instance
                  AND matched.property_id=e.property_id
                  AND matched.binding_version=e.binding_version
                  AND matched.feed=e.feed
                  AND matched.subject_ref=e.subject_ref
                  AND matched.work_item_id=w.id
                  AND matched.payload->>'eventType'='MATCHED'
                  AND NOT matched.baseline
              ) AS source_matched
            FROM qintopia_agent_os.business_event_inbox e
            JOIN qintopia_agent_os.collaboration_tenants tenant
              ON tenant.tenant_key=e.tenant_key AND tenant.mode='live'
              AND tenant.initialized AND tenant.identity_namespace=$4
            JOIN qintopia_agent_os.business_property_bindings b
              ON b.tenant_key=e.tenant_key AND b.id=e.binding_id
              AND b.source_instance=e.source_instance
              AND b.property_id=e.property_id AND b.version=e.binding_version
              AND b.active
            JOIN qintopia_identity.person_identity_gateways g
              ON g.tenant_key=b.tenant_key AND g.scope_id=b.scope_id
              AND g.gateway_key=$2 AND g.namespace=$4
              AND g.subject_type='wecom_internal' AND g.account_kind='employee'
              AND g.active
            JOIN qintopia_agent_os.collaboration_scopes s
              ON s.tenant_key=g.tenant_key AND s.id=g.scope_id AND s.status='active'
            JOIN qintopia_agent_os.work_items w ON w.id=e.work_item_id
            WHERE e.tenant_key=$1 AND e.binding_id=$3 AND e.work_item_id=$5
              AND e.feed=$6 AND NOT e.baseline
              AND e.payload->>'kind'='COLLECTION'
              AND e.payload->>'eventType'='DISCOVERED'
              AND w.work_item_type='business_operation'
              AND w.requester_agent='anan' AND w.target_agent='anan'
              AND w.capability_key='anan.pms'
              AND w.purpose='hospitality_business'
              AND w.metadata->>'mode'='live'
              AND w.payload->>'binding'=$3::text
              AND EXISTS(
                SELECT 1 FROM qintopia_agent_os.collaboration_scope_bindings sb
                JOIN qintopia_messages.conversations c ON c.id=sb.conversation_id
                WHERE sb.tenant_key=$1 AND sb.scope_id=g.scope_id
                  AND sb.revoked_at IS NULL AND c.tenant_id=$1
                  AND c.platform='wecom' AND c.status='active'
              )
              AND NOT EXISTS(
                SELECT 1 FROM qintopia_identity.person_identity_gateways other
                WHERE other.tenant_key<>$1 AND other.namespace=g.namespace
                  AND other.subject_type='wecom_internal' AND other.active
              )
              AND NOT EXISTS(
                SELECT 1 FROM qintopia_agent_os.collaboration_tenants other
                WHERE other.tenant_key<>$1 AND other.identity_namespace=g.namespace
              )
            LIMIT 1
            "#,
        )
        .bind(&self.tenant)
        .bind(gateway)
        .bind(binding)
        .bind(&self.identity_namespace)
        .bind(work_item)
        .bind(PAYMENT_SCHEMA)
        .fetch_optional(&self.pool)
        .await?
        .ok_or_else(|| anyhow::anyhow!("payment_workitem_unavailable"))?;
        let status: String = row.get("status");
        let payload: Value = row.get("payload");
        let readback_required = payload["readback_required"]
            .as_bool()
            .ok_or_else(|| anyhow::anyhow!("payment_workitem_invalid"))?;
        let contact_status = match payload["contact_status"].as_str() {
            Some("pending_verified_channel") => "pending_verified_channel",
            Some("suppressed_pending_readback") => "suppressed_pending_readback",
            _ => "unknown",
        };
        let terminal = matches!(status.as_str(), "completed" | "failed" | "cancelled");
        Ok(json!({
            "work_item_id":work_item,
            "kind":"payment",
            "status":status,
            "readback_required":readback_required,
            "contact_status":contact_status,
            "requires_human_confirmation":!terminal && !row.get::<bool,_>("source_matched")
        }))
    }

    pub(crate) async fn business_feed_context(&self, binding: Uuid) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        let b = binding_in(&mut tx, &self.tenant, binding).await?;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_feed_checkpoints WHERE tenant_key=$1 AND binding_id=$2 AND feed=$3)")
            .bind(&self.tenant).bind(binding).bind(PAYMENT_SCHEMA).fetch_one(&mut *tx).await?;
        let state = if exists {
            let (checkpoint, baseline) = checkpoint_in(&mut tx, &self.tenant, binding, &b).await?;
            json!({"cursor":checkpoint,"baselineCursor":baseline.to_string()})
        } else {
            Value::Null
        };
        let result = json!({"schemaVersion":PAYMENT_SCHEMA,"sourceInstance":b.source,"propertyId":b.property,"bindingVersion":b.version,"state":state});
        tx.commit().await?;
        Ok(result)
    }
    /// Existing snapshots are read before a source is sampled; missing baseline requires explicit head.
    pub(crate) async fn business_feed_open(
        &self,
        binding: Uuid,
        baseline: Option<&PaymentHead>,
    ) -> Result<Value> {
        // begin() locks the tenant row. First initialization is serialized with all ingestion.
        let (mut tx, _, _) = self.begin().await?;
        let b = binding_in(&mut tx, &self.tenant, binding).await?;
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_feed_checkpoints WHERE tenant_key=$1 AND binding_id=$2 AND feed=$3)")
            .bind(&self.tenant).bind(binding).bind(PAYMENT_SCHEMA).fetch_one(&mut *tx).await?;
        if let Some(head) = baseline {
            ensure!(
                head.schema_version == PAYMENT_SCHEMA
                    && head.source_instance == b.source
                    && head.property_id == b.property
                    && head.binding_version == b.version,
                "business_binding_changed"
            );
        }
        if !exists {
            let h = baseline
                .ok_or_else(|| anyhow::anyhow!("source_activation_snapshot_required"))?
                .head_cursor
                .as_str();
            cursor(h)?;
            sqlx::query("INSERT INTO qintopia_agent_os.business_feed_checkpoints(tenant_key,binding_id,feed,source_instance,property_id,binding_version,baseline_cursor,cursor_value) VALUES($1,$2,$3,$4,$5,$6,$7,$7) ON CONFLICT(tenant_key,binding_id,feed) DO NOTHING")
                .bind(&self.tenant).bind(binding).bind(PAYMENT_SCHEMA).bind(&b.source).bind(&b.property).bind(b.version).bind(h).execute(&mut *tx).await?;
        }
        let (checkpoint, initial) = checkpoint_in(&mut tx, &self.tenant, binding, &b).await?;
        if let Some(h) = baseline {
            ensure!(
                cursor(&h.head_cursor)? == initial,
                "feed_baseline_immutable"
            );
        }
        let result = json!({"schemaVersion":PAYMENT_SCHEMA,"sourceInstance":b.source,"propertyId":b.property,"bindingVersion":b.version,"baselineCursor":initial.to_string(),"cursor":checkpoint});
        tx.commit().await?;
        Ok(result)
    }
    pub(crate) async fn business_accept_payment(
        &self,
        binding: Uuid,
        source: &str,
        property: &str,
        event: &PaymentEvent,
    ) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        let b = binding_in(&mut tx, &self.tenant, binding).await?;
        ensure!(
            b.source == source && b.property == property,
            "feed_property_mismatch"
        );
        let (_, baseline) = checkpoint_in(&mut tx, &self.tenant, binding, &b).await?;
        let ack = accept_in(
            &mut tx,
            &self.tenant,
            binding,
            &b,
            baseline,
            event,
            self.is_live(),
        )
        .await?;
        // Push is intentionally independent of the contiguous feed checkpoint.
        tx.commit().await?;
        Ok(ack)
    }
    pub(crate) async fn business_feed_page(
        &self,
        binding: Uuid,
        source: &str,
        expected: &str,
        page: PaymentPage,
    ) -> Result<Value> {
        ensure!(
            page.schema_version == PAYMENT_SCHEMA && page.events.len() <= 100,
            "invalid_feed_page"
        );
        let start = cursor(expected)?;
        let end = cursor(&page.next_cursor)?;
        let (mut tx, _, _) = self.begin().await?;
        let b = binding_in(&mut tx, &self.tenant, binding).await?;
        ensure!(
            b.source == source && b.property == page.property_id,
            "feed_property_mismatch"
        );
        let (checkpoint, baseline) = checkpoint_in(&mut tx, &self.tenant, binding, &b).await?;
        ensure!(checkpoint == expected, "feed_checkpoint_conflict");
        let mut last = start;
        let mut receipts = Vec::new();
        for event in &page.events {
            let sequence = cursor(&event.sequence)?;
            ensure!(
                Some(sequence) == last.checked_add(1) && sequence <= end,
                "feed_sequence_gap"
            );
            receipts.push(
                accept_in(
                    &mut tx,
                    &self.tenant,
                    binding,
                    &b,
                    baseline,
                    event,
                    self.is_live(),
                )
                .await?,
            );
            last = sequence;
        }
        ensure!(last == end, "invalid_feed_cursor");
        sqlx::query("UPDATE qintopia_agent_os.business_feed_checkpoints SET cursor_value=$3,version=version+1 WHERE tenant_key=$1 AND binding_id=$2 AND feed='pms.payments.v1'")
            .bind(&self.tenant).bind(binding).bind(&page.next_cursor).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"cursor":page.next_cursor,"receipts":receipts}))
    }
}
