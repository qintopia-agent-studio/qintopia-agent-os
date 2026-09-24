//! Exact business scopes on the existing Person/grant authority, never a role fallback.
use super::{Actor, Store};
use crate::person_collaboration::{digest, model::*};
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Serialize)]
pub(crate) struct BusinessAuthority {
    pub operation_grant: Uuid,
    pub binding: Uuid,
    pub binding_version: i64,
    pub property: String,
    pub source: String,
    pub scope: Uuid,
    pub operation: String,
    pub action: String,
}

pub(crate) fn operation(key: &str) -> Result<Value> {
    let catalog: Value = serde_json::from_str(include_str!(
        "../../../../../skills/pms-operations/operations.json"
    ))?;
    let value = &catalog["operations"][key];
    ensure!(value.is_object(), "unsupported_business_operation");
    Ok(value.clone())
}

/// A deliberately bounded business-language parser. Unrecognized or incomplete prose
/// is not approval; the assistant can present a unique current plan for natural confirmation.
fn explicit_booking_intent(text: &str) -> Option<Value> {
    let re=regex::Regex::new(r"^请为「([^「」\r\n]{1,80})」（昵称「([^「」\r\n]{1,80})」）预订「([^「」\r\n]{1,80})」整间，([0-9]{4}-[0-9]{2}-[0-9]{2})入住，([0-9]{4}-[0-9]{2}-[0-9]{2})离店，1位住客，总价([0-9]{1,8})\.([0-9]{2})元，企微渠道，无会员权益，不加其他安排[。！]?$" ).ok()?;
    let c = re.captures(text.trim())?;
    let minor = c[6]
        .parse::<i64>()
        .ok()?
        .checked_mul(100)?
        .checked_add(c[7].parse::<i64>().ok()?)?;
    Some(
        json!({"guest":c[1],"nickname":c[2],"unit_code":c[3],"arrival":c[4],"departure":c[5],"amount_minor":minor}),
    )
}
fn explicit_collection_intent(text: &str) -> Option<Value> {
    let re=regex::Regex::new(r"^请为订单「([^「」\r\n]{1,160})」登记(企微|银行转账)收款([0-9]{1,8})\.([0-9]{2})元，流水号「([^「」\r\n]{1,160})」[。！]?$" ).ok()?;
    let c = re.captures(text.trim())?;
    let minor = c[3]
        .parse::<i64>()
        .ok()?
        .checked_mul(100)?
        .checked_add(c[4].parse::<i64>().ok()?)?;
    Some(
        json!({"orderId":c[1],"method":if &c[2]=="企微" {"WECOM"}else{"BANK_TRANSFER"},"amountMinor":minor,"transactionReference":c[5]}),
    )
}
fn intent_covers_collection(intent: &Value, input: &Value, effect: &Value) -> bool {
    input.as_object().is_some_and(|o| {
        o.len() == 5
            && o.keys().all(|k| {
                matches!(
                    k.as_str(),
                    "propertyId" | "orderId" | "method" | "amountMinor" | "transactionReference"
                )
            })
    }) && ["orderId", "method", "amountMinor", "transactionReference"]
        .iter()
        .all(|k| !intent[k].is_null() && intent[k] == input[k] && intent[k] == effect[k])
        && effect["currency"] == "CNY"
        && effect["note"].as_str().is_none_or(str::is_empty)
}
fn intent_covers_booking(intent: &Value, input: &Value, effect: &Value) -> bool {
    input.as_object().is_some_and(|o| {
        o.keys().all(|k| {
            matches!(
                k.as_str(),
                "propertyId" | "quoteId" | "primaryGuest" | "bookingChannelCode"
            )
        })
    }) && input["primaryGuest"]
        .as_object()
        .is_some_and(|o| o.len() == 2)
        && effect["primaryGuest"]
            .as_object()
            .is_some_and(|o| o.len() == 2)
        && effect["primaryGuest"]["fullName"] == intent["guest"]
        && effect["primaryGuest"]["nickname"] == intent["nickname"]
        && effect["inventoryUnit"]["code"] == intent["unit_code"]
        && effect["inventoryUnit"]["kind"] == "ROOM"
        && effect["inventoryUnit"]["propertyId"] == input["propertyId"]
        && effect["arrivalDate"] == intent["arrival"]
        && effect["departureDate"] == intent["departure"]
        && effect["occupants"].as_array().is_some_and(|v| v.len() == 1)
        && effect["pricing"]["currentContractAmount"]["currency"] == "CNY"
        && effect["pricing"]["currentContractAmount"]["minorUnits"] == intent["amount_minor"]
        && effect["pricing"]["coverageSet"]
            .as_array()
            .is_some_and(Vec::is_empty)
        && effect["pricingDecision"]["manualAdjustmentMinor"] == 0
        && effect["memberId"].is_null()
        && effect["memberContractId"].is_null()
        && effect["freeStayReason"].is_null()
        && effect["freeStayCategoryCode"].is_null()
        && effect["channelOrderReference"].is_null()
        && effect["bookingChannelCode"] == "WECOM"
        && effect["stayType"] == "TRANSIENT"
}

/// WorkItem summarizes the independent action ledger; it never rolls back PMS facts.
async fn sync_work(tx: &mut Transaction<'_, Postgres>, tenant: &str, work: Uuid) -> Result<()> {
    sqlx::query("UPDATE qintopia_agent_os.work_items SET status=(SELECT CASE WHEN bool_or(phase IN ('executing','previewing','unknown')) THEN 'processing' WHEN bool_or(phase IN ('awaiting_confirmation','paused','manual_handoff')) THEN 'awaiting_review' WHEN bool_or(phase='draft') THEN 'queued' WHEN bool_or(phase IN ('not_executed','preview_rejected')) THEN 'failed' WHEN bool_or(phase IN ('completed','manual_completed')) THEN 'completed' ELSE 'cancelled' END FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND work_item_id=$2),updated_at=clock_timestamp() WHERE id=$2")
        .bind(tenant).bind(work).execute(&mut **tx).await?;
    sqlx::query("UPDATE qintopia_agent_os.work_items s SET status=c.status,updated_at=clock_timestamp() FROM qintopia_agent_os.work_items c WHERE c.id=$1 AND s.metadata->>'business_work_ref'=c.id::text AND s.status<>'cancelled'")
        .bind(work).execute(&mut **tx).await?;
    Ok(())
}

/// Both authorization trees must remain valid: organizational grant and exact operation grant.
async fn operation_in(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    policy: &Policy,
    person: Uuid,
    binding: Uuid,
    key: &str,
    now: DateTime<Utc>,
) -> Result<BusinessAuthority> {
    let spec = operation(key)?;
    let action = spec["action"].as_str().unwrap();
    let row = sqlx::query("SELECT scope_id,property_id,source_instance,version FROM qintopia_agent_os.business_property_bindings WHERE tenant_key=$1 AND id=$2 AND active")
        .bind(tenant).bind(binding).fetch_optional(&mut **tx).await?
        .ok_or_else(||anyhow::anyhow!("business_binding_unavailable"))?;
    let scope: Uuid = row.get("scope_id");
    let grants: Vec<_> = policy
        .grants
        .iter()
        .filter(|g| {
            g.person == person
                && g.agent == "anan"
                && g.domain == "hospitality"
                && g.action == action
                && g.active
                && policy.in_scope(scope, g.scope, g.descendants)
        })
        .collect();
    ensure!(grants.len() == 1, "business_authority_denied");
    let base = grants[0];
    ensure!(
        policy.effective(base) && base.mode == PermissionMode::Autonomous,
        "business_authority_denied"
    );
    let rows=sqlx::query("SELECT id FROM qintopia_agent_os.business_operation_grants WHERE tenant_key=$1 AND authority_grant_id=$2 AND binding_id=$3 AND operation_key=$4 AND revoked_at IS NULL AND (valid_until IS NULL OR valid_until>$5)")
        .bind(tenant).bind(base.id).bind(binding).bind(key).bind(now).fetch_all(&mut **tx).await?;
    ensure!(rows.len() == 1, "business_operation_denied");
    let original: Uuid = rows[0].get("id");
    let mut current = Some(original);
    let mut seen = std::collections::BTreeSet::new();
    let mut child_issuer = None;
    while let Some(id) = current {
        ensure!(
            seen.insert(id) && seen.len() <= 16,
            "business_delegation_invalid"
        );
        let r=sqlx::query("SELECT authority_grant_id,parent_id,issued_by FROM qintopia_agent_os.business_operation_grants WHERE tenant_key=$1 AND id=$2 AND binding_id=$3 AND operation_key=$4 AND revoked_at IS NULL AND (valid_until IS NULL OR valid_until>$5)")
            .bind(tenant).bind(id).bind(binding).bind(key).bind(now).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("business_operation_revoked"))?;
        let grant_id: Uuid = r.get("authority_grant_id");
        let grant = policy
            .grants
            .iter()
            .find(|g| g.id == grant_id)
            .ok_or_else(|| anyhow::anyhow!("business_authority_denied"))?;
        ensure!(
            grant.agent == "anan"
                && grant.domain == "hospitality"
                && grant.action == action
                && grant.mode == PermissionMode::Autonomous
                && policy.effective(grant)
                && policy.in_scope(scope, grant.scope, grant.descendants),
            "business_authority_denied"
        );
        if let Some(issuer) = child_issuer {
            ensure!(grant.person == issuer, "business_delegation_invalid");
        }
        child_issuer = Some(r.get::<Uuid, _>("issued_by"));
        current = r.get("parent_id");
    }
    Ok(BusinessAuthority {
        operation_grant: original,
        binding,
        binding_version: row.get("version"),
        property: row.get("property_id"),
        source: row.get("source_instance"),
        scope,
        operation: key.into(),
        action: action.into(),
    })
}

impl Store {
    pub(crate) async fn business_authorize(
        &self,
        actor: &Actor,
        binding: Uuid,
        key: &str,
    ) -> Result<BusinessAuthority> {
        // The registered identity is mandatory even for the synthetic local executor.
        ensure!(agents().contains(&"anan"), "business_agent_not_registered");
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let policy = super::foundation::load_policy(&mut tx, &self.tenant, now).await?;
        let result = operation_in(
            &mut tx,
            &self.tenant,
            &policy,
            actor.person,
            binding,
            key,
            now,
        )
        .await?;
        if let Some((_, _, scope)) = actor.gateway {
            ensure!(scope == result.scope, "gateway_scope_mismatch");
        }
        tx.commit().await?;
        Ok(result)
    }

    /// A manager can delegate only an operation they personally hold now.
    pub(crate) async fn business_delegate(
        &self,
        actor: &Actor,
        target_grant: Uuid,
        binding: Uuid,
        key: &str,
        until: DateTime<Utc>,
    ) -> Result<Uuid> {
        ensure!(agents().contains(&"anan"), "business_agent_not_registered");
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        ensure!(until > now, "invalid_effective_window");
        let policy = super::foundation::load_policy(&mut tx, &self.tenant, now).await?;
        let held = operation_in(
            &mut tx,
            &self.tenant,
            &policy,
            actor.person,
            binding,
            key,
            now,
        )
        .await?;
        let target = policy
            .grants
            .iter()
            .find(|g| g.id == target_grant)
            .ok_or_else(|| anyhow::anyhow!("business_authority_denied"))?;
        ensure!(
            target.agent == "anan"
                && target.domain == "hospitality"
                && target.action == held.action
                && policy.effective(target)
                && policy.in_scope(held.scope, target.scope, target.descendants),
            "business_authority_denied"
        );
        ensure!(
            policy
                .manager(
                    actor.person,
                    target.scope,
                    &target.agent,
                    &target.domain,
                    &target.action
                )
                .is_some(),
            "management_denied"
        );
        let id=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_operation_grants(tenant_key,authority_grant_id,binding_id,operation_key,parent_id,issued_by,valid_until) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
            .bind(&self.tenant).bind(target_grant).bind(binding).bind(key).bind(held.operation_grant).bind(actor.person).bind(until).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(id)
    }
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct HostTurn {
    pub platform: String,
    pub chat_type: String,
    pub chat_id: String,
    pub sender_id: String,
    pub message_id: String,
    pub text: String,
}

impl Store {
    /// Called only by the separately authenticated host ingress, never a model tool.
    pub(crate) async fn business_capture_turn(
        &self,
        gateway: &str,
        turn: &HostTurn,
    ) -> Result<Value> {
        ensure!(
            matches!(turn.platform.as_str(), "wecom" | "qiwe")
                && matches!(turn.chat_type.as_str(), "direct" | "group"),
            "trusted_context_unavailable"
        );
        for id in [&turn.chat_id, &turn.sender_id, &turn.message_id] {
            ensure!(
                !id.is_empty() && id.len() <= 240 && !id.chars().any(char::is_whitespace),
                "trusted_context_unavailable"
            );
        }
        ensure!(turn.text.len() <= 16000, "invalid_arguments");
        let actor = self.gateway_actor(gateway, &turn.sender_id).await?;
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, &actor).await?;
        let subject:String=sqlx::query_scalar("SELECT subject_type FROM qintopia_identity.person_identity_gateways WHERE tenant_key=$1 AND gateway_key=$2 AND active")
            .bind(&self.tenant).bind(gateway).fetch_one(&mut *tx).await?;
        ensure!(
            (turn.platform == "wecom"
                && matches!(subject.as_str(), "wecom_internal" | "wecom_external"))
                || (turn.platform == "qiwe" && subject == "qiwe_sender"),
            "gateway_platform_mismatch"
        );
        if turn.chat_type == "group" {
            let bound:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id WHERE b.tenant_key=$1 AND b.scope_id=$2 AND b.revoked_at IS NULL AND c.chat_id=$3 AND c.platform=$4 AND c.status='active')")
                .bind(&self.tenant).bind(actor.gateway.as_ref().unwrap().2).bind(&turn.chat_id).bind(&turn.platform).fetch_one(&mut *tx).await?;
            ensure!(bound, "gateway_scope_mismatch");
        }
        let message_hash = digest(turn.message_id.as_bytes());
        let chat_hash = digest(turn.chat_id.as_bytes());
        let content_hash = digest(turn.text.as_bytes());
        let natural_command = match turn.text.trim() {
            "确认预订" | "就按这个方案订房" => Some("pms.command.CREATE_ORDER"),
            "确认登记这笔收款" => Some("pms.command.RECORD_COLLECTION"),
            "确认已到店，办理入住" => Some("pms.command.CHECK_IN"),
            "确认办理退房" => Some("pms.command.CHECK_OUT"),
            "确认改期" => Some("pms.command.RESCHEDULE_STAY"),
            "确认续住" => Some("pms.command.EXTEND_STAY"),
            "确认缩短住宿" => Some("pms.command.SHORTEN_STAY"),
            "确认换房" => Some("pms.command.MOVE_UNIT"),
            "确认取消预订" => Some("pms.command.CANCEL_ORDER"),
            "按这个方案办理" | "确认这个方案" => Some(""),
            _ => None,
        };
        let mut confirmation = turn
            .text
            .trim()
            .strip_prefix("确认方案 ")
            .filter(|s| s.len() == 32 && s.bytes().all(|b| b.is_ascii_hexdigit()))
            .map(str::to_owned);
        if let Some(command) = natural_command {
            let candidates:Vec<String>=sqlx::query_scalar("SELECT a.confirmation_code FROM qintopia_agent_os.business_actions a JOIN qintopia_agent_os.business_turn_evidence e ON e.id=a.source_evidence_id AND e.tenant_key=a.tenant_key WHERE a.tenant_key=$1 AND a.actor_person_id=$2 AND e.gateway_key=$3 AND e.chat_hash=$4 AND a.phase='awaiting_confirmation' AND ($5='' OR a.operation_key=$5)")
                .bind(&self.tenant).bind(actor.person).bind(gateway).bind(&chat_hash).bind(command).fetch_all(&mut *tx).await?;
            if candidates.len() == 1 {
                confirmation = Some(candidates[0].clone());
            }
        }
        let mut intent = explicit_booking_intent(&turn.text)
            .or_else(|| explicit_collection_intent(&turn.text))
            .or_else(|| {
                let order = turn
                    .text
                    .trim()
                    .strip_prefix("已在PMS办理这个方案，订单号")?;
                (!order.is_empty()
                    && order.len() <= 160
                    && order
                        .bytes()
                        .all(|b| b.is_ascii_alphanumeric() || b == b'_' || b == b'-'))
                .then(|| json!({"manual_order":order}))
            });
        if turn.text.trim() == "确认关联" {
            let proposals:Vec<String>=sqlx::query_scalar("SELECT metadata->'business_link_proposal'->>'hash' FROM qintopia_agent_os.work_items WHERE metadata->'business_link_proposal'->>'tenant'=$1 AND metadata->'business_link_proposal'->>'person'=$2 AND metadata->'business_link_proposal'->>'gateway'=$3 AND metadata->'business_link_proposal'->>'chat'=$4 AND (metadata->'business_link_proposal'->>'expires')::timestamptz>clock_timestamp()")
                .bind(&self.tenant).bind(actor.person.to_string()).bind(gateway).bind(&chat_hash).fetch_all(&mut *tx).await?;
            if proposals.len() == 1 {
                intent = Some(json!({"source_link":proposals[0]}));
            }
        }
        if turn.text.trim() == "取消关联" {
            sqlx::query("UPDATE qintopia_agent_os.work_items SET metadata=metadata-'business_link_proposal' WHERE metadata->'business_link_proposal'->>'tenant'=$1 AND metadata->'business_link_proposal'->>'person'=$2 AND metadata->'business_link_proposal'->>'gateway'=$3 AND metadata->'business_link_proposal'->>'chat'=$4")
                .bind(&self.tenant).bind(actor.person.to_string()).bind(gateway).bind(&chat_hash).execute(&mut *tx).await?;
        }
        if let Some(r)=sqlx::query("SELECT id,person_id,content_hash,chat_hash FROM qintopia_agent_os.business_turn_evidence WHERE tenant_key=$1 AND gateway_key=$2 AND message_hash=$3")
            .bind(&self.tenant).bind(gateway).bind(&message_hash).fetch_optional(&mut *tx).await? {
            ensure!(r.get::<Uuid,_>("person_id")==actor.person && r.get::<String,_>("content_hash")==content_hash && r.get::<String,_>("chat_hash")==chat_hash,"trusted_message_conflict");
            return Ok(json!({"evidence":r.get::<Uuid,_>("id"),"replayed":true}));
        }
        let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_turn_evidence(tenant_key,gateway_key,platform,chat_hash,chat_type,message_hash,person_id,identity_id,identity_version,content_hash,confirmation_code,explicit_intent) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12) RETURNING id")
            .bind(&self.tenant).bind(gateway).bind(&turn.platform).bind(chat_hash).bind(&turn.chat_type).bind(message_hash).bind(actor.person).bind(actor.link).bind(actor.identity_version).bind(content_hash).bind(confirmation).bind(intent).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"evidence":id,"replayed":false}))
    }
}

impl Store {
    async fn business_evidence_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        gateway: &str,
        message: &str,
        chat: &str,
    ) -> Result<Uuid> {
        let row=sqlx::query("SELECT id,identity_id,identity_version,observed_at,chat_type,platform FROM qintopia_agent_os.business_turn_evidence WHERE tenant_key=$1 AND gateway_key=$2 AND message_hash=$3 AND chat_hash=$4 AND person_id=$5")
            .bind(&self.tenant).bind(gateway).bind(digest(message.as_bytes())).bind(digest(chat.as_bytes())).bind(actor.person).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("trusted_message_evidence_required"))?;
        ensure!(
            row.get::<Uuid, _>("identity_id") == actor.link
                && row.get::<i64, _>("identity_version") == actor.identity_version,
            "identity_version_conflict"
        );
        if row.get::<String, _>("chat_type") == "group" {
            let bound:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id WHERE b.tenant_key=$1 AND b.scope_id=$2 AND b.revoked_at IS NULL AND c.chat_id=$3 AND c.platform=$4 AND c.status='active')")
                .bind(&self.tenant).bind(actor.gateway.as_ref().map(|g|g.2)).bind(chat).bind(row.get::<String,_>("platform")).fetch_one(&mut **tx).await?;
            ensure!(bound, "gateway_scope_mismatch");
        }
        Ok(row.get("id"))
    }

    pub(crate) async fn business_invoke(
        &self,
        actor: &Actor,
        message: &str,
        chat: &str,
        tool: &str,
        a: &Value,
    ) -> Result<Value> {
        let gateway = &actor
            .gateway
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("trusted_context_unavailable"))?
            .0;
        ensure!(agents().contains(&"anan"), "business_agent_not_registered");
        ensure!(
            a.is_object() && serde_json::to_vec(a)?.len() <= 256 * 1024,
            "invalid_arguments"
        );
        let allowed: &[&str] = match tool {
            "pms_authorize" => &["binding", "operation"],
            "pms_start" => &[
                "binding",
                "operation",
                "input",
                "reason",
                "work_item",
                "source_payment",
            ],
            "pms_event_context" => &["binding", "operation", "work_item"],
            "pms_save_preview" => &["action", "claim", "preview"],
            "pms_reject_preview" => &["action", "claim"],
            "pms_save_result" => &["action", "claim", "result", "readback"],
            "pms_save_manual" => &["action", "readback"],
            "pms_manual_context" => &["action"],
            "pms_link" => &["action", "work_item", "readback"],
            "pms_link_context" => &["action"],
            "pms_status" | "pms_claim_preview" | "pms_claim_execute" | "pms_recovery"
            | "pms_pause" | "pms_resume" | "pms_cancel" | "pms_handoff" => &["action"],
            _ => anyhow::bail!("unknown_tool"),
        };
        ensure!(
            a.as_object()
                .unwrap()
                .keys()
                .all(|key| allowed.contains(&key.as_str())),
            "invalid_arguments"
        );
        let parse = |key: &str| -> Result<Uuid> { Ok(serde_json::from_value(a[key].clone())?) };
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let evidence = self
            .business_evidence_in(&mut tx, actor, gateway, message, chat)
            .await?;
        let policy = super::foundation::load_policy(&mut tx, &self.tenant, now).await?;
        if matches!(tool, "pms_authorize" | "pms_start" | "pms_event_context") {
            let key = a["operation"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("invalid_arguments"))?;
            let auth = operation_in(
                &mut tx,
                &self.tenant,
                &policy,
                actor.person,
                parse("binding")?,
                key,
                now,
            )
            .await?;
            ensure!(
                actor.gateway.as_ref().is_some_and(|g| g.2 == auth.scope),
                "gateway_scope_mismatch"
            );
            if tool == "pms_event_context" {
                let row=sqlx::query("SELECT subject_ref FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND binding_id=$2 AND binding_version=$3 AND work_item_id=$4 AND source_instance=$5 AND property_id=$6 AND feed='pms.payments.v1' AND NOT baseline AND payload->>'kind'='COLLECTION' AND payload->>'eventType'='DISCOVERED'")
                    .bind(&self.tenant).bind(auth.binding).bind(auth.binding_version).bind(parse("work_item")?).bind(&auth.source).bind(&auth.property).fetch_optional(&mut *tx).await?;
                let result = match row {
                    Some(row) => {
                        ensure!(
                            key == "pms.command.RECORD_COLLECTION",
                            "business_work_denied"
                        );
                        json!({"bill_id":row.get::<String,_>("subject_ref"),"property":auth.property})
                    }
                    None => {
                        if self
                            .application_business_source(&mut tx, &auth, parse("work_item")?)
                            .await?
                        {
                            json!({"source_kind":"application","property":auth.property})
                        } else {
                            Value::Null
                        }
                    }
                };
                tx.commit().await?;
                return Ok(result);
            }
            if tool == "pms_authorize" {
                tx.commit().await?;
                return Ok(serde_json::to_value(auth)?);
            }
            ensure!(
                operation(key)?["command"].is_string(),
                "unsupported_business_operation"
            );
            let reason = &a["reason"];
            ensure!(
                reason.as_object().is_some_and(|r| r.len() == 2)
                    && reason["code"]
                        .as_str()
                        .is_some_and(|v| !v.is_empty() && v.len() <= 160)
                    && reason["note"].as_str().is_some_and(|v| v.len() <= 8000),
                "invalid_arguments"
            );
            let mut input = a["input"].clone();
            ensure!(
                input.is_object() && input.get("propertyId").is_none(),
                "invalid_arguments"
            );
            input["propertyId"] = json!(auth.property);
            if key == "pms.command.RECORD_COLLECTION" {
                if let Some(reference) = input["transactionReference"]
                    .as_str()
                    .filter(|s| !s.is_empty())
                {
                    if let Some(prior)=sqlx::query("SELECT id,input FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND binding_id=$2 AND operation_key=$3 AND input->>'transactionReference'=$4 AND input->>'method'=$5 AND phase NOT IN ('cancelled','preview_rejected','not_executed') LIMIT 1")
                        .bind(&self.tenant).bind(auth.binding).bind(key).bind(reference).bind(input["method"].as_str()).fetch_optional(&mut *tx).await? {
                        let old:Value=prior.get("input");
                        ensure!(old["orderId"]==input["orderId"] && old["amountMinor"]==input["amountMinor"],"business_payment_conflict");
                        let id:Uuid=prior.get("id");tx.commit().await?;return Ok(json!({"action":id,"replayed":true}));
                    }
                }
            }
            let hash = digest(&serde_json::to_vec(
                &json!({"input":input,"operation":key,"binding":auth.binding}),
            )?);
            if let Some(row)=sqlx::query("SELECT id FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND source_evidence_id=$2 AND operation_key=$3 AND request_hash=$4")
                .bind(&self.tenant).bind(evidence).bind(key).bind(&hash).fetch_optional(&mut *tx).await? {
                let id:Uuid=row.get("id");tx.commit().await?;return Ok(json!({"action":id,"replayed":true}));
            }
            let work = if a.get("work_item").is_some() {
                let work = parse("work_item")?;
                let owned:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND work_item_id=$2 AND binding_id=$3 AND actor_person_id=$4)")
                    .bind(&self.tenant).bind(work).bind(auth.binding).bind(actor.person).fetch_one(&mut *tx).await?;
                let source_payment = &a["source_payment"];
                let event_owned = if !owned && key == "pms.command.RECORD_COLLECTION" {
                    sqlx::query_scalar::<_,bool>("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND binding_id=$2 AND binding_version=$3 AND work_item_id=$4 AND feed='pms.payments.v1' AND subject_ref=$5 AND source_instance=$6 AND property_id=$7 AND NOT baseline AND payload->>'kind'='COLLECTION' AND payload->>'eventType'='DISCOVERED')")
                        .bind(&self.tenant).bind(auth.binding).bind(auth.binding_version).bind(work).bind(source_payment["id"].as_str().unwrap_or("")).bind(&auth.source).bind(&auth.property).fetch_one(&mut *tx).await?
                        && source_payment["status"]=="AVAILABLE" && source_payment["kind"]=="COLLECTION"
                        && input["method"]=="WECOM" && source_payment["reference"]==input["transactionReference"]
                        && source_payment["amountMinor"]==input["amountMinor"]
                } else {
                    false
                };
                let application_owned = self
                    .application_business_source(&mut tx, &auth, work)
                    .await?;
                if application_owned && key == "pms.command.CREATE_ORDER" {
                    let linked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.application_intake_states i JOIN qintopia_agent_os.welcome_cases c ON c.application_id=i.application_id AND c.source_instance=i.source_instance AND c.property_id=i.property_id WHERE i.tenant_key=$1 AND i.anan_work_id=$2)")
                        .bind(&self.tenant).bind(work).fetch_one(&mut *tx).await?;
                    ensure!(!linked, "application_order_already_linked");
                }
                ensure!(
                    owned || event_owned || application_owned,
                    "business_work_denied"
                );
                self.canonical_business_work(&mut tx, work, &input, key)
                    .await?
            } else {
                sqlx::query_scalar("INSERT INTO qintopia_agent_os.work_items(work_item_type,status,requester_agent,target_agent,capability_key,brief_summary,purpose,dedupe_key,idempotency_key,payload,metadata) VALUES('business_operation','queued','anan','anan','anan.pms','客房办理','synthetic_business',$1,$1,'{}','{\"local_only\":true}') RETURNING id")
                    .bind(format!("business/{}/{evidence}/{hash}",self.tenant)).fetch_one(&mut *tx).await?
            };
            // Quote identity and an explicitly selected source matter survive new
            // messages and process restarts. Do not replace an unknown booking.
            if key == "pms.command.CREATE_ORDER" {
                let prior=sqlx::query("SELECT id,request_hash,actor_person_id,source_evidence_id FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND binding_id=$2 AND operation_key=$3 AND phase NOT IN ('cancelled','preview_rejected','not_executed') AND (work_item_id=$4 OR input->>'quoteId'=$5) ORDER BY created_at LIMIT 1")
                    .bind(&self.tenant).bind(auth.binding).bind(key).bind(work).bind(input["quoteId"].as_str()).fetch_optional(&mut *tx).await?;
                if let Some(prior) = prior {
                    let same_origin:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_turn_evidence WHERE tenant_key=$1 AND id=$2 AND gateway_key=$3 AND chat_hash=$4)")
                        .bind(&self.tenant).bind(prior.get::<Uuid,_>("source_evidence_id")).bind(gateway).bind(digest(chat.as_bytes())).fetch_one(&mut *tx).await?;
                    ensure!(
                        same_origin && prior.get::<Uuid, _>("actor_person_id") == actor.person,
                        "business_conversation_mismatch"
                    );
                    ensure!(
                        prior.get::<String, _>("request_hash") == hash,
                        "booking_stage_already_exists"
                    );
                    let id: Uuid = prior.get("id");
                    // A new ad-hoc task has no actions and is rolled back here.
                    return Ok(json!({"action":id,"replayed":true}));
                }
            }
            let id = Uuid::new_v4();
            sqlx::query("INSERT INTO qintopia_agent_os.business_actions(id,tenant_key,work_item_id,binding_id,binding_version,operation_key,actor_person_id,actor_identity_id,identity_version,source_evidence_id,authority_operation_id,request_hash,input,phase,preview_key,execution_key,resolution_key,correlation_id,confirmation_code,reason) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11,$12,$13,'draft',$14,$15,$16,$17,$18,$19)")
                .bind(id).bind(&self.tenant).bind(work).bind(auth.binding).bind(auth.binding_version).bind(key).bind(actor.person).bind(actor.link).bind(actor.identity_version).bind(evidence).bind(auth.operation_grant).bind(hash).bind(input)
                .bind(format!("anan_preview_{id}")).bind(format!("anan_execute_{id}")).bind(format!("anan_resolve_{id}")).bind(format!("anan_correlation_{id}")).bind(Uuid::new_v4().simple().to_string()).bind(&a["reason"]).execute(&mut *tx).await?;
            super::foundation::work_event(
                &mut tx,
                work,
                "accepted",
                "anan",
                &json!({"action":id,"operation":key}),
            )
            .await?;
            sync_work(&mut tx, &self.tenant, work).await?;
            tx.commit().await?;
            return Ok(json!({"action":id,"work_item":work,"replayed":false}));
        }
        let id = parse("action")?;
        let row=sqlx::query("SELECT * FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND id=$2 FOR UPDATE")
            .bind(&self.tenant).bind(id).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("business_action_unavailable"))?;
        let key: String = row.get("operation_key");
        let binding: Uuid = row.get("binding_id");
        let auth = operation_in(
            &mut tx,
            &self.tenant,
            &policy,
            actor.person,
            binding,
            &key,
            now,
        )
        .await?;
        ensure!(
            actor.gateway.as_ref().is_some_and(|g| g.2 == auth.scope),
            "gateway_scope_mismatch"
        );
        ensure!(
            row.get::<i64, _>("binding_version") == auth.binding_version,
            "business_binding_changed"
        );
        let phase: String = row.get("phase");
        let work: Uuid = row.get("work_item_id");
        let preview: Option<Value> = row.get("preview");
        let result: Option<Value> = row.get("result");
        let claim: Option<Uuid> = row.get("claim_id");
        let source: Uuid = row.get("source_evidence_id");
        let same_chat:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_turn_evidence WHERE tenant_key=$1 AND id=$2 AND gateway_key=$3 AND chat_hash=$4)")
            .bind(&self.tenant).bind(source).bind(gateway).bind(digest(chat.as_bytes())).fetch_one(&mut *tx).await?;
        ensure!(same_chat, "business_conversation_mismatch");
        let original_person: Uuid = row.get("actor_person_id");
        // Revalidate the initiator's authority too; a second person cannot revive a revoked plan.
        if matches!(tool, "pms_claim_preview" | "pms_claim_execute") {
            let original_authority = operation_in(
                &mut tx,
                &self.tenant,
                &policy,
                original_person,
                binding,
                &key,
                now,
            )
            .await?;
            ensure!(
                original_authority.operation_grant == row.get::<Uuid, _>("authority_operation_id"),
                "business_authority_changed"
            );
        }
        let identity_valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links WHERE id=$1 AND person_id=$2 AND version=$3 AND status='confirmed' AND evidence_ref IS NOT NULL AND confirmed_by IS NOT NULL)")
            .bind(row.get::<Uuid,_>("actor_identity_id")).bind(original_person).bind(row.get::<i64,_>("identity_version")).fetch_one(&mut *tx).await?;
        if matches!(tool, "pms_claim_preview" | "pms_claim_execute") {
            ensure!(identity_valid, "identity_version_conflict");
        }
        let mut reply = json!({"action":id,"work_item":work,"phase":phase,"operation":key,"version":row.get::<i64,_>("version"),"preview":preview,"result":result,"readback":row.get::<Option<Value>,_>("readback")});
        match tool {
            "pms_link_context" => {
                ensure!(
                    key == "pms.command.CREATE_ORDER" && phase == "completed",
                    "completed_booking_required"
                );
                let readback: Option<Value> = row.get("readback");
                let order = readback
                    .as_ref()
                    .and_then(|r| r["order"]["id"].as_str())
                    .ok_or_else(|| anyhow::anyhow!("booking_readback_required"))?;
                reply = json!({"property":auth.property,"order_ref":order});
            }
            "pms_link" => {
                reply = self
                    .link_business_source(
                        &mut tx,
                        &auth,
                        parse("work_item")?,
                        id,
                        evidence,
                        &a["readback"],
                    )
                    .await?;
                sync_work(&mut tx, &self.tenant, work).await?;
            }
            "pms_status" => {
                if phase == "awaiting_confirmation" {
                    reply["confirmation_code"] = json!(row.get::<String, _>("confirmation_code"));
                }
            }
            "pms_resume" => {
                ensure!(phase == "paused", "business_not_paused");
                // Resume needs a fresh preview and confirmation; never reactivate an old approval.
                sqlx::query("UPDATE qintopia_agent_os.business_actions SET phase='draft',preview=NULL,confirmation_evidence_id=NULL,confirmed_by=NULL,confirmation_code=$3,preview_key=$4,version=version+1,updated_at=clock_timestamp() WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(id).bind(Uuid::new_v4().simple().to_string()).bind(format!("anan_preview_{}",Uuid::new_v4())).execute(&mut *tx).await?;
                reply = json!({"action":id,"phase":"draft"});
            }
            "pms_claim_preview" => {
                ensure!(phase == "draft", "business_action_not_draft");
                let claimed = Uuid::new_v4();
                sqlx::query("UPDATE qintopia_agent_os.business_actions SET phase='previewing',claim_id=$3,updated_at=clock_timestamp() WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(id).bind(claimed).execute(&mut *tx).await?;
                reply = json!({"action":id,"claim":claimed,"input":row.get::<Value,_>("input"),"operation":key,"preview_key":row.get::<String,_>("preview_key"),"execution_key":row.get::<String,_>("execution_key"),"correlation":row.get::<String,_>("correlation_id")});
            }
            "pms_reject_preview" => {
                ensure!(
                    phase == "previewing" && claim == Some(parse("claim")?),
                    "business_claim_invalid"
                );
                sqlx::query("UPDATE qintopia_agent_os.business_actions SET phase='preview_rejected',result='{\"stage\":\"preview\",\"code\":\"pms_preview_rejected\"}',updated_at=clock_timestamp() WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(id).execute(&mut *tx).await?;
                reply = json!({"action":id,"phase":"preview_rejected","result":{"stage":"preview","code":"pms_preview_rejected"}});
            }
            "pms_save_preview" => {
                ensure!(
                    phase == "previewing" && claim == Some(parse("claim")?),
                    "business_claim_invalid"
                );
                let p = &a["preview"];
                ensure!(
                    p["propertyId"] == auth.property
                        && p["commandType"] == operation(&key)?["command"]
                        && p["effectHash"].as_str().is_some_and(
                            |s| s.len() == 64 && s.bytes().all(|b| b.is_ascii_hexdigit())
                        )
                        && p["previewId"].is_string(),
                    "invalid_pms_preview"
                );
                let expiry: DateTime<Utc> = p["expiresAt"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("invalid_pms_preview"))?
                    .parse()?;
                sqlx::query("UPDATE qintopia_agent_os.business_actions SET phase='awaiting_confirmation',preview=$3,claim_id=NULL,updated_at=clock_timestamp() WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(id).bind(p).execute(&mut *tx).await?;
                let source_evidence=sqlx::query("SELECT explicit_intent,observed_at FROM qintopia_agent_os.business_turn_evidence WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(source).fetch_one(&mut *tx).await?;
                let intent: Option<Value> = source_evidence.get("explicit_intent");
                let bindings:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.business_property_bindings WHERE tenant_key=$1 AND scope_id=$2 AND active")
                    .bind(&self.tenant).bind(auth.scope).fetch_one(&mut *tx).await?;
                let covered = expiry > now
                    && bindings == 1
                    && row.get::<i64, _>("version") == 1
                    && source_evidence.get::<DateTime<Utc>, _>("observed_at")
                        > now - chrono::Duration::minutes(15)
                    && intent.as_ref().is_some_and(|i| match key.as_str() {
                        "pms.command.CREATE_ORDER" => {
                            intent_covers_booking(i, &row.get::<Value, _>("input"), &p["effect"])
                        }
                        "pms.command.RECORD_COLLECTION" => {
                            intent_covers_collection(i, &row.get::<Value, _>("input"), &p["effect"])
                        }
                        _ => false,
                    });
                if covered {
                    sqlx::query("UPDATE qintopia_agent_os.business_actions SET confirmation_evidence_id=$3,confirmed_by=$4 WHERE tenant_key=$1 AND id=$2")
                        .bind(&self.tenant).bind(id).bind(source).bind(original_person).execute(&mut *tx).await?;
                }
                reply = json!({"action":id,"phase":"awaiting_confirmation","preview":p,"confirmation_code":row.get::<String,_>("confirmation_code"),"confirmation_reused":covered,"confirmation_hint":if covered {"原明确交办已覆盖本方案，无需再次确认"} else {"请确认当前唯一方案；有多个方案时先取消或暂停其他方案"}});
            }
            "pms_claim_execute" => {
                ensure!(
                    phase == "awaiting_confirmation",
                    "business_not_awaiting_confirmation"
                );
                let p = preview
                    .as_ref()
                    .ok_or_else(|| anyhow::anyhow!("pms_preview_required"))?;
                let expiry: DateTime<Utc> = p["expiresAt"].as_str().unwrap_or("").parse()?;
                ensure!(expiry > now, "pms_preview_expired");
                let confirmation:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_turn_evidence WHERE tenant_key=$1 AND id=$2 AND confirmation_code=$3 AND observed_at>=$4 AND observed_at>$5)")
                    .bind(&self.tenant).bind(evidence).bind(row.get::<String,_>("confirmation_code")).bind(row.get::<DateTime<Utc>,_>("updated_at")).bind(now-chrono::Duration::minutes(15)).fetch_one(&mut *tx).await?;
                let reuse = row.get::<Option<Uuid>, _>("confirmation_evidence_id") == Some(source)
                    && row.get::<Option<Uuid>, _>("confirmed_by") == Some(actor.person)
                    && row.get::<DateTime<Utc>, _>("created_at")
                        > now - chrono::Duration::minutes(15);
                ensure!(confirmation || reuse, "human_confirmation_required");
                let confirmed_evidence = if reuse { source } else { evidence };
                let claimed = Uuid::new_v4();
                sqlx::query("UPDATE qintopia_agent_os.business_actions SET phase='executing',confirmation_evidence_id=$3,confirmed_by=$4,claim_id=$5,updated_at=clock_timestamp() WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(id).bind(confirmed_evidence).bind(actor.person).bind(claimed).execute(&mut *tx).await?;
                reply = json!({"action":id,"claim":claimed,"preview":p,"input":row.get::<Value,_>("input"),"reason":row.get::<Value,_>("reason"),"execution_key":row.get::<String,_>("execution_key"),"correlation":row.get::<String,_>("correlation_id")});
            }
            "pms_recovery" => {
                ensure!(
                    matches!(phase.as_str(), "executing" | "unknown" | "previewing"),
                    "business_recovery_not_needed"
                );
                reply = json!({"action":id,"claim":claim,"operation":key,"property":auth.property,"phase":phase,"preview_key":row.get::<String,_>("preview_key"),"input":row.get::<Value,_>("input"),"execution_key":row.get::<String,_>("execution_key"),"resolution_key":row.get::<String,_>("resolution_key"),"correlation":row.get::<String,_>("correlation_id")});
            }
            "pms_save_result" => {
                ensure!(
                    (matches!(phase.as_str(), "executing" | "unknown")
                        || (phase == "previewing"
                            && (key == "pms.quote"
                                || a["result"]["executionStatus"] != "EXECUTED")))
                        && claim == Some(parse("claim")?),
                    "business_claim_invalid"
                );
                let r = &a["result"];
                let status = r["executionStatus"].as_str().unwrap_or("UNKNOWN");
                ensure!(
                    matches!(status, "EXECUTED" | "NOT_EXECUTED" | "UNKNOWN"),
                    "invalid_pms_result"
                );
                let next = match status {
                    "EXECUTED" => {
                        ensure!(
                            r["businessCommitted"] == true
                                && r["receiptId"].is_string()
                                && r["commandId"].is_string(),
                            "invalid_pms_result"
                        );
                        "completed"
                    }
                    "NOT_EXECUTED" => {
                        ensure!(
                            r["businessCommitted"] == false && r["receiptId"].is_string(),
                            "invalid_pms_result"
                        );
                        "not_executed"
                    }
                    _ if phase == "previewing" => "previewing",
                    _ => "unknown",
                };
                sqlx::query("UPDATE qintopia_agent_os.business_actions SET phase=$3,result=$4,readback=$5,updated_at=clock_timestamp() WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(id).bind(next).bind(r).bind(a.get("readback")).execute(&mut *tx).await?;
                super::foundation::work_event(
                    &mut tx,
                    work,
                    "business_result",
                    "anan",
                    &json!({"action":id,"phase":next,"receipt":r["receiptId"]}),
                )
                .await?;
                reply = json!({"action":id,"phase":next,"result":r,"readback":a.get("readback")});
            }
            "pms_manual_context" | "pms_save_manual" => {
                ensure!(phase == "manual_handoff", "business_handoff_required");
                let intent:Option<Value>=sqlx::query_scalar("SELECT explicit_intent FROM qintopia_agent_os.business_turn_evidence WHERE tenant_key=$1 AND id=$2 AND observed_at>$3")
                    .bind(&self.tenant).bind(evidence).bind(now-chrono::Duration::minutes(15)).fetch_one(&mut *tx).await?;
                let intent =
                    intent.ok_or_else(|| anyhow::anyhow!("human_manual_reference_required"))?;
                let reference = intent["manual_order"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("human_manual_reference_required"))?;
                let input: Value = row.get("input");
                ensure!(
                    input["orderId"] == reference,
                    "business_manual_reference_mismatch"
                );
                if tool == "pms_manual_context" {
                    reply = json!({"action":id,"property":auth.property,"order_ref":reference});
                } else {
                    let readback = &a["readback"];
                    ensure!(
                        readback["order"]["id"] == reference
                            && readback["order"]["property_id"] == auth.property,
                        "invalid_pms_readback"
                    );
                    let expected = match key.as_str() {
                        "pms.command.CHECK_IN" => "CHECKED_IN",
                        "pms.command.CHECK_OUT" => "CHECKED_OUT",
                        "pms.command.CANCEL_ORDER" => "CANCELLED",
                        _ => anyhow::bail!("manual_effect_verification_required"),
                    };
                    ensure!(
                        readback["order"]["status"] == expected,
                        "manual_effect_not_observed"
                    );
                    sqlx::query("UPDATE qintopia_agent_os.business_actions SET phase='manual_completed',readback=$3,confirmation_evidence_id=$4,confirmed_by=$5,updated_at=clock_timestamp() WHERE tenant_key=$1 AND id=$2")
                        .bind(&self.tenant).bind(id).bind(readback).bind(evidence).bind(actor.person).execute(&mut *tx).await?;
                    super::foundation::work_event(
                        &mut tx,
                        work,
                        "human_pms_completion_observed",
                        "anan",
                        &json!({"action":id,"order_ref":reference}),
                    )
                    .await?;
                    reply = json!({"action":id,"phase":"manual_completed","readback":readback});
                }
            }
            "pms_pause" | "pms_cancel" | "pms_handoff" => {
                ensure!(
                    !matches!(phase.as_str(), "executing" | "unknown" | "previewing"),
                    "business_reconcile_first"
                );
                ensure!(
                    !matches!(
                        phase.as_str(),
                        "completed" | "not_executed" | "manual_completed" | "preview_rejected"
                    ),
                    "business_action_terminal"
                );
                let next = match tool {
                    "pms_pause" => "paused",
                    "pms_cancel" => "cancelled",
                    _ => "manual_handoff",
                };
                sqlx::query("UPDATE qintopia_agent_os.business_actions SET phase=$3,updated_at=clock_timestamp() WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(id).bind(next).execute(&mut *tx).await?;
                super::foundation::work_event(
                    &mut tx,
                    work,
                    "business_waiting",
                    "anan",
                    &json!({"action":id,"phase":next}),
                )
                .await?;
                reply = json!({"action":id,"phase":next,"pms_reversed":false});
            }
            _ => anyhow::bail!("unknown_tool"),
        }
        if tool != "pms_status" {
            sync_work(&mut tx, &self.tenant, work).await?;
        }
        tx.commit().await?;
        Ok(reply)
    }
}
