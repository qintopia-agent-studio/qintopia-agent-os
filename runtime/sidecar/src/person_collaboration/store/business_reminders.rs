//! Bounded host-only local reminders. WorkItem owns state; cron owns wakeups.
use super::{foundation, Store};
use crate::person_collaboration::{digest, model::PermissionMode};
use anyhow::{ensure, Result};
use chrono::{DateTime, Datelike, Duration, Timelike, Utc};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

const RULE: &str = "anan.pms.reminders";
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Rule {
    collaboration: Uuid,
    group: Uuid,
    kinds: Vec<String>,
    required_fields: Vec<String>,
    weekdays: Vec<u32>,
    start_minute: u32,
    end_minute: u32,
    utc_offset_minutes: i64,
    arrival_due_minute: u32,
    initial_delay_seconds: i64,
    repeat_seconds: i64,
    escalation: Option<Escalation>,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Escalation {
    after_seconds: i64,
    group: Uuid,
}
impl Rule {
    fn validate(&self) -> Result<()> {
        ensure!(
            !self.kinds.is_empty()
                && self.kinds.len() <= 3
                && self.kinds.iter().all(|s| matches!(
                    s.as_str(),
                    "missing_information" | "collection" | "arrival"
                ))
                && self.required_fields.len() <= 8
                && self.required_fields.iter().all(|s| matches!(
                    s.as_str(),
                    "name"
                        | "nickname"
                        | "arrival"
                        | "nights"
                        | "room_type"
                        | "occupation"
                        | "interests"
                ))
                && !self.weekdays.is_empty()
                && self.weekdays.len() <= 7
                && self.weekdays.iter().all(|d| (1..=7).contains(d))
                && self.start_minute < self.end_minute
                && self.end_minute <= 1440
                && self.arrival_due_minute < 1440
                && (-720..=840).contains(&self.utc_offset_minutes)
                && (0..=31_536_000).contains(&self.initial_delay_seconds)
                && (1..=31_536_000).contains(&self.repeat_seconds)
                && self
                    .escalation
                    .as_ref()
                    .is_none_or(|e| (1..=31_536_000).contains(&e.after_seconds)),
            "invalid_reminder_rule"
        );
        Ok(())
    }
}
fn hash(v: &Value) -> Result<String> {
    Ok(digest(&serde_json::to_vec(v)?))
}
fn time(v: &Value) -> Option<DateTime<Utc>> {
    v.as_str().and_then(|s| s.parse().ok())
}

pub(crate) async fn invoke(store: &Store, gateway: &str, a: Value) -> Result<Value> {
    ensure!(
        std::env::var("QINTOPIA_PMS_REMINDERS_LOCAL_ENABLE").as_deref() == Ok("1")
            && std::env::var("QINTOPIA_PMS_LOCAL_ENABLE").as_deref() == Ok("1")
            && std::env::var("QINTOPIA_FOUNDATION_LOCAL_ENABLE").as_deref() == Ok("1"),
        "reminder_disabled"
    );
    let binding = std::env::var("QINTOPIA_PMS_REMINDER_BINDING")?.parse()?;
    store.business_reminder(gateway, binding, &a).await
}

impl Store {
    pub(super) async fn reminder_context_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        binding: Uuid,
        work: Uuid,
    ) -> Result<Value> {
        let b=sqlx::query("SELECT b.scope_id,b.property_id,b.version FROM qintopia_agent_os.business_property_bindings b JOIN qintopia_agent_os.collaboration_scopes s ON s.id=b.scope_id AND s.tenant_key=b.tenant_key WHERE b.tenant_key=$1 AND b.id=$2 AND b.active AND s.status='active'")
            .bind(&self.tenant).bind(binding).fetch_one(&mut **tx).await?;
        // Membership is established by owned persisted sources/actions, never by caller metadata.
        let own:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND binding_id=$2 AND work_item_id=$3) OR EXISTS(SELECT 1 FROM qintopia_agent_os.application_intake_states WHERE tenant_key=$1 AND binding_id=$2 AND anan_work_id=$3) OR EXISTS(SELECT 1 FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND binding_id=$2 AND work_item_id=$3)")
            .bind(&self.tenant).bind(binding).bind(work).fetch_one(&mut **tx).await?;
        ensure!(own, "reminder_work_outside_binding");
        let stale:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND binding_id=$2 AND work_item_id=$3 AND binding_version<>$4) OR EXISTS(SELECT 1 FROM qintopia_agent_os.application_intake_states WHERE tenant_key=$1 AND binding_id=$2 AND anan_work_id=$3 AND binding_version<>$4) OR EXISTS(SELECT 1 FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND binding_id=$2 AND work_item_id=$3 AND binding_version<>$4)")
            .bind(&self.tenant).bind(binding).bind(work).bind(b.get::<i64,_>("version")).fetch_one(&mut **tx).await?;
        ensure!(!stale, "reminder_binding_changed");
        let w=sqlx::query("SELECT status,metadata,created_at FROM qintopia_agent_os.work_items WHERE id=$1 AND target_agent='anan' AND capability_key='anan.pms' FOR UPDATE")
            .bind(work).fetch_one(&mut **tx).await?;
        let meta: Value = w.get("metadata");
        let orders:Vec<String>=sqlx::query_scalar("SELECT DISTINCT coalesce(readback->'order'->>'id',input->>'orderId') FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND binding_id=$2 AND work_item_id=$3 AND coalesce(readback->'order'->>'id',input->>'orderId') IS NOT NULL")
            .bind(&self.tenant).bind(binding).bind(work).fetch_all(&mut **tx).await?;
        ensure!(orders.len() <= 1, "reminder_ambiguous_order");
        let applications:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('record',i.record_ref,'resource_alias',i.resource_alias,'revision',a.revision,'field_hash',a.field_hash,'valid',a.valid,'source_version',i.source_version) FROM qintopia_agent_os.application_intake_states i JOIN qintopia_agent_os.welcome_applications a ON a.id=i.application_id JOIN qintopia_agent_os.work_items w ON w.id=i.anan_work_id WHERE i.tenant_key=$1 AND i.binding_id=$2 AND (i.anan_work_id=$3 OR w.metadata->>'business_work_ref'=$3::text) ORDER BY i.id")
            .bind(&self.tenant).bind(binding).bind(work).fetch_all(&mut **tx).await?;
        let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND work_item_id=$2 AND phase IN ('paused','executing','unknown','previewing','manual_handoff'))")
            .bind(&self.tenant).bind(work).fetch_one(&mut **tx).await?;
        let linked_controls:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('phase',metadata->'pms_reminder'->'phase','last_sent',metadata->'pms_reminder'->'last_sent','snooze_until',metadata->'pms_reminder'->'snooze_until','pending_since',metadata->'pms_reminder'->'pending_since') FROM qintopia_agent_os.work_items WHERE metadata->>'business_work_ref'=$1::text ORDER BY id")
            .bind(work).fetch_all(&mut **tx).await?;
        Ok(
            json!({"work":work,"scope":b.get::<Uuid,_>("scope_id"),"property":b.get::<String,_>("property_id"),"binding_version":b.get::<i64,_>("version"),"status":w.get::<String,_>("status"),"created":w.get::<DateTime<Utc>,_>("created_at"),"merged_into":meta["business_work_ref"],"order":orders.first(),"applications":applications,"blocked":blocked,"linked_controls":linked_controls,"state":meta["pms_reminder"]}),
        )
    }

    async fn reminder_plan_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        binding: Uuid,
        context: &Value,
        observed: &Value,
        now: DateTime<Utc>,
    ) -> Result<Value> {
        let scope = serde_json::from_value(context["scope"].clone())?;
        let Some(knowledge) =
            foundation::effective_knowledge_in(tx, &self.tenant, scope, RULE, None).await?
        else {
            return Ok(json!({"status":"waiting_rule"}));
        };
        let rule: Rule = serde_json::from_value(knowledge.content.clone())?;
        rule.validate()?;
        let policy = foundation::load_policy(tx, &self.tenant, now).await?;
        ensure!(
            policy
                .grants
                .iter()
                .any(|g| g.id == knowledge.authority_grant
                    && g.person == knowledge.author
                    && g.action == "change_rules"
                    && g.mode == PermissionMode::Autonomous
                    && policy.effective(g)
                    && policy.decision(g.collaboration, "change_rules")["status"] == "autonomous"
                    && policy.in_scope(scope, g.scope, g.descendants)),
            "reminder_rule_authority_unavailable"
        );
        let grants: Vec<_> = policy
            .grants
            .iter()
            .filter(|g| {
                g.collaboration == rule.collaboration
                    && g.agent == "anan"
                    && g.domain == "hospitality"
                    && g.action == "read_business"
                    && policy.decision(g.collaboration, "read_business")["status"] == "autonomous"
                    && g.mode == PermissionMode::Autonomous
                    && policy.effective(g)
                    && policy.in_scope(scope, g.scope, g.descendants)
            })
            .collect();
        ensure!(grants.len() == 1, "reminder_authority_unavailable");
        if context["order"].is_string() {
            super::business::operation_in(
                tx,
                &self.tenant,
                &policy,
                grants[0].person,
                binding,
                "pms.read.order",
                now,
            )
            .await?;
        }
        let audience:Value=sqlx::query_scalar("SELECT configuration FROM qintopia_agent_os.collaboration_audiences WHERE tenant_key=$1 AND collaboration_id=$2")
            .bind(&self.tenant).bind(rule.collaboration).fetch_one(&mut **tx).await?;
        ensure!(
            audience["proactive"] == "autonomous",
            "reminder_audience_denied"
        );
        let local = now + Duration::minutes(rule.utc_offset_minutes);
        let mut pending = Vec::new();
        let mut basis = context.clone();
        basis.as_object_mut().unwrap().remove("state");
        let terminal = context["status"] == "cancelled"
            || context["merged_into"].is_string()
            || context["status"] == "completed" && context["order"].is_null();
        if !terminal {
            let sources = context["applications"].as_array().unwrap();
            let reads = observed["applications"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("reminder_application_read_required"))?;
            ensure!(
                reads.len() == sources.len(),
                "reminder_application_read_required"
            );
            let mut missing = false;
            for (source, read) in sources.iter().zip(reads) {
                ensure!(
                    read["record"] == source["record"]
                        && read["resource_alias"] == source["resource_alias"]
                        && read["source_version"] == source["source_version"]
                        && hash(
                            &json!({"fields":read["fields"],"valid":read["valid"],"consent_active":read["consent_active"]})
                        )? == source["field_hash"].as_str().unwrap_or(""),
                    "reminder_application_changed"
                );
                if source["valid"] == true {
                    missing |= rule.required_fields.iter().any(|f| {
                        read["fields"][f].is_null()
                            || read["fields"][f].as_str().is_some_and(str::is_empty)
                    });
                }
            }
            if missing && rule.kinds.iter().any(|k| k == "missing_information") {
                pending.push("missing_information");
            }
            if let Some(order) = context["order"].as_str() {
                let read = &observed["order"];
                ensure!(
                    read["order"]["id"] == order
                        && read["order"]["property_id"] == context["property"]
                        && read["order"]["version"].as_i64().is_some_and(|v| v > 0),
                    "reminder_order_read_required"
                );
                let status = read["order"]["status"].as_str().unwrap_or("");
                ensure!(
                    matches!(
                        status,
                        "RESERVED"
                            | "CHECKED_IN"
                            | "CHECKED_OUT"
                            | "CANCELLED"
                            | "NO_SHOW"
                            | "CHECK_IN_REVOKED"
                    ),
                    "reminder_unknown_order_status"
                );
                if status == "CANCELLED" {
                    pending.clear();
                }
                if status != "CANCELLED" {
                    let amounts = &read["amounts"];
                    let contract = amounts["currentContractAmount"]["minorUnits"]
                        .as_i64()
                        .ok_or_else(|| anyhow::anyhow!("reminder_amounts_required"))?;
                    let net = amounts["netRecordedCollection"]["minorUnits"]
                        .as_i64()
                        .ok_or_else(|| anyhow::anyhow!("reminder_amounts_required"))?;
                    ensure!(
                        amounts["currentContractAmount"]["currency"]
                            == amounts["netRecordedCollection"]["currency"]
                            && amounts["currentContractAmount"]["currency"].is_string(),
                        "reminder_currency_mismatch"
                    );
                    if contract > net && rule.kinds.iter().any(|k| k == "collection") {
                        pending.push("collection");
                    }
                    let arrival = read["order"]["arrival_date"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("reminder_arrival_required"))?
                        .parse::<chrono::NaiveDate>()?;
                    if status == "RESERVED"
                        && (arrival < local.date_naive()
                            || arrival == local.date_naive()
                                && local.hour() * 60 + local.minute() >= rule.arrival_due_minute)
                        && rule.kinds.iter().any(|k| k == "arrival")
                    {
                        pending.push("arrival");
                    }
                }
            }
        }
        let mut controls = context["linked_controls"]
            .as_array()
            .cloned()
            .unwrap_or_default();
        let linked_inflight = controls
            .iter()
            .any(|s| matches!(s["phase"].as_str(), Some("claimed" | "unknown")));
        controls.push(context["state"].clone());
        let pending_since = if pending.is_empty() {
            None
        } else {
            Some(
                controls
                    .iter()
                    .filter_map(|s| time(&s["pending_since"]))
                    .min()
                    .unwrap_or(now),
            )
        };
        let snooze_until = controls
            .iter()
            .filter_map(|s| time(&s["snooze_until"]))
            .max();
        let last_sent = controls.iter().filter_map(|s| time(&s["last_sent"])).max();
        let age = pending_since.map_or(0, |t| (now - t).num_seconds());
        let group = rule
            .escalation
            .as_ref()
            .filter(|e| age >= e.after_seconds)
            .map_or(rule.group, |e| e.group);
        ensure!(
            audience["groups"]
                .as_array()
                .is_some_and(|g| g.contains(&json!(group))),
            "reminder_group_not_in_audience"
        );
        let destination:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('group',c.id,'platform',c.platform,'chat_id',c.chat_id,'version',b.version) FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id AND c.tenant_id=b.tenant_key WHERE b.tenant_key=$1 AND b.scope_id=$2 AND b.conversation_id=$3 AND b.revoked_at IS NULL AND c.status='active' AND c.chat_type='group' AND c.platform='wecom' AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger l WHERE l.tenant_key=$1 AND l.kind='group' AND l.object_ref=c.id::text AND l.status<>'active')")
            .bind(&self.tenant).bind(scope).bind(group).fetch_optional(&mut **tx).await?;
        let destination =
            destination.ok_or_else(|| anyhow::anyhow!("reminder_group_unavailable"))?;
        let minute = local.hour() * 60 + local.minute();
        let due = !pending.is_empty()
            && context["blocked"] != true
            && !linked_inflight
            && snooze_until.is_none_or(|t| t <= now)
            && age >= rule.initial_delay_seconds
            && rule
                .weekdays
                .contains(&local.weekday().number_from_monday())
            && minute >= rule.start_minute
            && minute < rule.end_minute
            && last_sent.is_none_or(|t| now >= t + Duration::seconds(rule.repeat_seconds));
        let labels: Vec<_> = pending
            .iter()
            .map(|kind| match *kind {
                "collection" => "待收款",
                "arrival" => "待到店",
                _ => "待补充办理信息",
            })
            .collect();
        let text = format!(
            "模拟客房待办 {}：{}",
            context["work"].as_str().unwrap_or(""),
            labels.join("、")
        );
        let signature = hash(
            &json!({"basis":basis,"facts":hash(observed)?,"text":text,"rule":knowledge.id,"content":knowledge.content,"authority":grants[0].id,"audience":audience,"destination":destination,"pending":pending}),
        )?;
        Ok(
            json!({"status":if pending.is_empty(){"stopped"}else if due{"due"}else{"waiting"},"pending":pending,"pending_since":pending_since,"signature":signature,"destination":destination,"rule":knowledge.id,"profile":"anan","text":text,"simulated":true}),
        )
    }

    pub(crate) async fn business_reminder(
        &self,
        gateway: &str,
        binding: Uuid,
        a: &Value,
    ) -> Result<Value> {
        ensure!(
            a.is_object()
                && a.as_object().unwrap().keys().all(|k| [
                    "action",
                    "work",
                    "after",
                    "observation",
                    "signature",
                    "claim",
                    "receipt"
                ]
                .contains(&k.as_str())),
            "invalid_arguments"
        );
        let (mut tx, _, now) = self.begin().await?;
        let gateway_ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.person_identity_gateways g JOIN qintopia_agent_os.business_property_bindings b ON b.tenant_key=g.tenant_key AND b.scope_id=g.scope_id WHERE g.tenant_key=$1 AND g.gateway_key=$2 AND b.id=$3 AND g.active AND b.active)")
            .bind(&self.tenant).bind(gateway).bind(binding).fetch_one(&mut *tx).await?;
        ensure!(gateway_ok, "reminder_gateway_mismatch");
        if a["action"] == "list" {
            ensure!(a["after"].is_null(), "reminder_scan_cursor_not_supported");
            let works:Vec<Uuid>=sqlx::query_scalar("SELECT w.id FROM qintopia_agent_os.work_items w WHERE w.id IN (SELECT work_item_id FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND binding_id=$2 UNION SELECT anan_work_id FROM qintopia_agent_os.application_intake_states WHERE tenant_key=$1 AND binding_id=$2 UNION SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND binding_id=$2) ORDER BY w.metadata->>'pms_reminder_scanned_at' NULLS FIRST,w.id LIMIT 100")
                .bind(&self.tenant).bind(binding).fetch_all(&mut *tx).await?;
            sqlx::query("UPDATE qintopia_agent_os.work_items SET metadata=jsonb_set(metadata,'{pms_reminder_scanned_at}',to_jsonb($2::text)) WHERE id=ANY($1)").bind(&works).bind(now.to_rfc3339()).execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(json!({"works":works,"next":null,"bounded_batch":100}));
        }
        let work: Uuid = serde_json::from_value(a["work"].clone())?;
        let context = self.reminder_context_in(&mut tx, binding, work).await?;
        if a["action"] == "context" {
            return Ok(context);
        }
        let mut state = context["state"].as_object().cloned().unwrap_or_default();
        let result = match a["action"].as_str().unwrap_or("") {
            "observe" | "claim" | "validate" => {
                let plan = self
                    .reminder_plan_in(&mut tx, binding, &context, &a["observation"], now)
                    .await?;
                let active = matches!(
                    state.get("phase").and_then(Value::as_str),
                    Some("claimed" | "unknown")
                );
                match a["action"].as_str().unwrap() {
                    "observe" => {
                        state.insert("pending_since".into(), plan["pending_since"].clone());
                        state.insert("current_status".into(), plan["status"].clone());
                        if !active {
                            state.insert("plan".into(), plan.clone());
                        }
                        plan
                    }
                    "claim" => {
                        if active {
                            json!({"status":"unknown","claim":state.get("claim")})
                        } else if plan["status"] != "due" {
                            plan
                        } else {
                            ensure!(
                                a["signature"] == plan["signature"]
                                    && state
                                        .get("plan")
                                        .is_some_and(|p| p["signature"] == plan["signature"]),
                                "reminder_plan_changed"
                            );
                            let claim = Uuid::new_v4();
                            state.insert("claim".into(), json!(claim));
                            state.insert("phase".into(), json!("claimed"));
                            state.insert("plan".into(), plan.clone());
                            json!({"status":"claimed","claim":claim,"plan":plan})
                        }
                    }
                    _ => {
                        ensure!(
                            state.get("phase") == Some(&json!("claimed"))
                                && state.get("claim") == Some(&a["claim"]),
                            "reminder_claim_unavailable"
                        );
                        if plan["status"] != "due"
                            || state["plan"]["signature"] != plan["signature"]
                        {
                            state.insert("phase".into(), json!("suppressed"));
                            json!({"status":"suppressed"})
                        } else {
                            // Commit UNKNOWN before the only external boundary. No lease expiry replays it.
                            state.insert("phase".into(), json!("unknown"));
                            state.insert("attempted_at".into(), json!(now));
                            json!({"status":"send","claim":a["claim"],"plan":plan,"text":plan["text"]})
                        }
                    }
                }
            }
            "settle" => {
                ensure!(
                    state.get("claim") == Some(&a["claim"])
                        && matches!(
                            state.get("phase").and_then(Value::as_str),
                            Some("unknown" | "sent")
                        ),
                    "reminder_claim_unavailable"
                );
                let receipt = &a["receipt"];
                ensure!(
                    receipt["claim"] == a["claim"]
                        && receipt["profile"] == "anan"
                        && receipt["simulated"] == true
                        && receipt["destination"] == state["plan"]["destination"]
                        && receipt["signature"] == state["plan"]["signature"]
                        && receipt["message_id"]
                            .as_str()
                            .is_some_and(|s| !s.is_empty() && s.len() <= 160),
                    "invalid_reminder_receipt"
                );
                if state.get("phase") == Some(&json!("sent")) {
                    ensure!(
                        state.get("receipt") == Some(receipt),
                        "reminder_receipt_conflict"
                    );
                } else {
                    state.insert("phase".into(), json!("sent"));
                    state.insert("last_sent".into(), json!(now));
                    state.insert("receipt".into(), receipt.clone());
                }
                json!({"status":"sent","simulated":true})
            }
            _ => anyhow::bail!("invalid_arguments"),
        };
        let audit = json!({"action":a["action"],"status":result["status"],"claim":state.get("claim"),"signature":state.get("plan").map(|p|&p["signature"]),"destination":state.get("plan").map(|p|&p["destination"]),"receipt":if a["action"]=="settle"{a["receipt"].clone()}else{Value::Null},"simulated":true});
        sqlx::query("UPDATE qintopia_agent_os.work_items SET metadata=jsonb_set(metadata,'{pms_reminder}',$2),updated_at=clock_timestamp() WHERE id=$1")
            .bind(work).bind(Value::Object(state)).execute(&mut *tx).await?;
        foundation::work_event(&mut tx, work, "pms_reminder", "anan", &audit).await?;
        tx.commit().await?;
        Ok(result)
    }
}
