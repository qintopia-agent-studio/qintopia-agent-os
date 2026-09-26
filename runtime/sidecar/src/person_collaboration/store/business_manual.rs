//! PMS history evidence for manual takeover; neither status nor equal amounts prove execution.
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};

fn identified(v: &Value, key: &str) -> bool {
    v[key].as_str().is_some_and(|s| !s.is_empty())
}
fn after(v: &Value, start: DateTime<Utc>, now: DateTime<Utc>) -> bool {
    v["created_at"]
        .as_str()
        .and_then(|s| s.parse::<DateTime<Utc>>().ok())
        .is_some_and(|t| t >= start && t <= now)
}
pub(super) fn baseline(
    read: &Value,
    order: &str,
    property: &str,
    now: DateTime<Utc>,
) -> Result<Value> {
    ensure!(
        read["order"]["id"] == order
            && read["order"]["property_id"] == property
            && read["order"]["version"].as_i64().is_some_and(|v| v > 0),
        "invalid_pms_readback"
    );
    let facts = read["collectionFacts"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid_pms_readback"))?;
    ensure!(
        facts
            .iter()
            .all(|f| identified(f, "fact_id") && f["order_id"] == order),
        "invalid_pms_readback"
    );
    Ok(
        json!({"order":{"id":order,"property_id":property,"version":read["order"]["version"]},
        "handoff_at":now,"fact_ids":facts.iter().map(|f|f["fact_id"].clone()).collect::<Vec<_>>()}),
    )
}

pub(super) fn verify(
    key: &str,
    preview: &Value,
    base: &Value,
    read: &Value,
    now: DateTime<Utc>,
) -> Result<Value> {
    let order = base["order"]["id"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("manual_effect_verification_required"))?;
    ensure!(
        read["order"]["id"] == order
            && read["order"]["property_id"] == base["order"]["property_id"],
        "invalid_pms_readback"
    );
    let version = base["order"]["version"]
        .as_i64()
        .ok_or_else(|| anyhow::anyhow!("manual_effect_verification_required"))?;
    let start: DateTime<Utc> = base["handoff_at"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("manual_effect_verification_required"))?
        .parse()?;
    let effect = &preview["effect"];
    ensure!(
        effect.is_object() && effect["orderId"] == order,
        "manual_effect_verification_required"
    );
    let command = key.strip_prefix("pms.command.").unwrap_or("");
    if command == "RECORD_COLLECTION" {
        let has_reference = effect["transactionReference"]
            .as_str()
            .is_some_and(|s| !s.trim().is_empty());
        let facts = read["collectionFacts"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("manual_effect_not_observed"))?;
        let old = base["fact_ids"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("manual_effect_verification_required"))?;
        // Count all facts with the reference, including old/reversed ones. Ambiguity is not success.
        let matches: Vec<_> = facts
            .iter()
            .filter(|f| {
                f["order_id"] == order
                    && f["fact_type"] == "COLLECTION"
                    && if has_reference {
                        f["transaction_reference"] == effect["transactionReference"]
                    } else {
                        f["transaction_reference"].is_null()
                            && !old.contains(&f["fact_id"])
                            && after(f, start, now)
                            && f["method"] == effect["method"]
                            && f["amount_minor"] == effect["amountMinor"]
                            && f["currency"] == effect["currency"]
                    }
            })
            .collect();
        ensure!(matches.len() == 1, "manual_effect_not_observed");
        let f = matches[0];
        ensure!(
            identified(f, "fact_id")
                && identified(f, "command_id")
                && !old.contains(&f["fact_id"])
                && after(f, start, now)
                && f["method"] == effect["method"]
                && f["amount_minor"] == effect["amountMinor"]
                && f["net_effect_minor"] == effect["amountMinor"]
                && f["currency"] == effect["currency"]
                && effect["amountMinor"].as_i64().is_some_and(|a| a > 0)
                && f["references_fact_id"].is_null()
                && f["reverses_fact_id"].is_null()
                && f["transfer"].is_null()
                && facts
                    .iter()
                    .all(|other| other["reverses_fact_id"] != f["fact_id"]),
            "manual_effect_not_observed"
        );
        // A note supplied by the original plan is part of its effect too.
        ensure!(
            effect["note"].as_str().is_none_or(|note| f["note"] == note),
            "manual_effect_not_observed"
        );
        return Ok(
            json!({"fact_id":f["fact_id"],"command_id":f["command_id"],"handoff_at":start,
            "requires_fact_confirmation":!has_reference,"summary":{"id":f["fact_id"],"orderId":order,"method":f["method"],"amountMinor":f["amount_minor"],"currency":f["currency"],"occurredAt":f["created_at"]}}),
        );
    }
    ensure!(
        matches!(
            command,
            "CHECK_IN"
                | "CHECK_OUT"
                | "CANCEL_ORDER"
                | "RESCHEDULE_STAY"
                | "EXTEND_STAY"
                | "SHORTEN_STAY"
                | "MOVE_UNIT"
        ),
        "manual_effect_verification_required"
    );
    let history = read["amendments"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("manual_effect_not_observed"))?;
    let matches: Vec<_> = history
        .iter()
        .filter(|h| {
            h["order_id"] == order
                && h["amendment_type"] == command
                && h["payload"] == *effect
                && h["prior_version"].as_i64().is_some_and(|v| v >= version)
                && after(h, start, now)
        })
        .collect();
    ensure!(matches.len() == 1, "manual_effect_not_observed");
    let h = matches[0];
    let prior = h["prior_version"].as_i64().unwrap();
    ensure!(
        identified(h, "id")
            && identified(h, "command_id")
            && h["new_version"].as_i64() == prior.checked_add(1)
            && h["sequence"] == h["new_version"]
            && h["new_version"] == read["order"]["version"],
        "manual_effect_not_observed"
    );
    let status = match command {
        "CHECK_IN" => Some("CHECKED_IN"),
        "CHECK_OUT" => Some("CHECKED_OUT"),
        "CANCEL_ORDER" => Some("CANCELLED"),
        _ => None,
    };
    ensure!(
        status.is_none_or(|s| read["order"]["status"] == s),
        "manual_effect_not_observed"
    );
    Ok(
        json!({"amendment_id":h["id"],"command_id":h["command_id"],"new_version":h["new_version"],"handoff_at":start}),
    )
}

pub(super) async fn adopt_order(
    store: &super::Store,
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    actor: &super::Actor,
    row: &sqlx::postgres::PgRow,
    auth: &super::business::BusinessAuthority,
    evidence: uuid::Uuid,
    current: &Value,
) -> Result<Value> {
    use sqlx::Row;
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let id: uuid::Uuid = row.get("id");
    let work: uuid::Uuid = row.get("work_item_id");
    let preview: Option<Value> = row.get("preview");
    let preview = preview.ok_or_else(|| anyhow::anyhow!("pms_preview_required"))?;
    let observed = &current["booking"];
    let order = observed["id"]
        .as_str()
        .filter(|s| !s.is_empty())
        .ok_or_else(|| anyhow::anyhow!("invalid_pms_readback"))?;
    ensure!(
        current["order"]["id"] == order
            && observed["propertyId"] == auth.property
            && current["order"]["property_id"] == auth.property
            && current["order"]["version"] == observed["version"]
            && observed["version"].as_i64().is_some_and(|v| v > 0)
            && matches!(observed["status"].as_str(), Some("RESERVED" | "CHECKED_IN"))
            && [
                "inventoryUnitId",
                "unit_code",
                "arrivalDate",
                "departureDate",
                "bookingChannelCode",
                "currency",
                "stayType",
                "quoteId"
            ]
            .iter()
            .all(|k| identified(observed, k))
            && identified(&observed["primaryGuest"], "fullName")
            && observed["amountMinor"].as_i64().is_some_and(|v| v >= 0)
            && observed["segments"]
                .as_array()
                .is_some_and(|v| !v.is_empty())
            && observed["occupants"]
                .as_array()
                .is_some_and(|v| !v.is_empty()),
        "manual_effect_verification_required"
    );
    // Tenant transaction serializes competing adoption proposals and confirmations.
    let conflict:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_actions a JOIN qintopia_agent_os.business_property_bindings b ON b.id=a.binding_id WHERE a.tenant_key=$1 AND b.source_instance=$2 AND b.property_id=$3 AND a.work_item_id<>$4 AND ((a.operation_key='pms.command.CREATE_ORDER' AND a.input->>'quoteId'=$6 AND a.phase IN ('executing','unknown','previewing')) OR (a.readback->'order'->>'id'=$5 AND a.phase IN ('completed','manual_completed')) OR (a.phase='manual_handoff' AND a.readback->'order_proposal'->>'order'=$5 AND (a.readback->'order_proposal'->>'expires')::timestamptz>clock_timestamp())))")
        .bind(&store.tenant).bind(&auth.source).bind(&auth.property).bind(work).bind(order).bind(observed["quoteId"].as_str().unwrap()).fetch_one(&mut **tx).await?;
    ensure!(!conflict, "manual_order_conflict");
    let turn=sqlx::query("SELECT explicit_intent,gateway_key,chat_hash,observed_at FROM qintopia_agent_os.business_turn_evidence WHERE tenant_key=$1 AND id=$2")
        .bind(&store.tenant).bind(evidence).fetch_one(&mut **tx).await?;
    let intent: Option<Value> = turn.get("explicit_intent");
    let intent = intent.unwrap_or(Value::Null);
    let gateway: String = turn.get("gateway_key");
    let chat: String = turn.get("chat_hash");
    let effect = &preview["effect"];
    let mut original = json!({"primaryGuest":effect["primaryGuest"],"inventoryUnitId":effect["inventoryUnit"]["id"],"unit_code":effect["inventoryUnit"]["code"],"arrivalDate":effect["arrivalDate"],"departureDate":effect["departureDate"],"bookingChannelCode":effect["bookingChannelCode"],"amountMinor":effect["pricing"]["currentContractAmount"]["minorUnits"],"currency":effect["pricing"]["currentContractAmount"]["currency"],"stayType":effect["stayType"],"memberId":effect["memberId"],"occupants":effect["occupants"]});
    let names = |v: &Value| -> Value {
        let mut m = serde_json::Map::new();
        for k in ["fullName", "nickname"] {
            if let Some(v) = v.get(k) {
                m.insert(k.into(), v.clone());
            }
        }
        Value::Object(m)
    };
    original["primaryGuest"] = names(&effect["primaryGuest"]);
    original["occupants"] = json!(effect["occupants"]
        .as_array()
        .map(|rows| rows
            .iter()
            .map(|v| {
                let mut n = names(v);
                if let Some(role) = v.get("role") {
                    n["role"] = role.clone();
                }
                n
            })
            .collect::<Vec<_>>())
        .unwrap_or_default());
    let differences: Vec<_> = [
        ("primaryGuest", "住客辨识"),
        ("inventoryUnitId", "房间"),
        ("arrivalDate", "入住日期"),
        ("departureDate", "离店日期"),
        ("bookingChannelCode", "渠道"),
        ("amountMinor", "合同金额"),
        ("currency", "币种"),
        ("stayType", "住宿类型"),
        ("memberId", "会员关联"),
        ("occupants", "入住人员"),
    ]
    .iter()
    .filter(|(k, _)| original[*k] != observed[*k])
    .map(|(_, label)| *label)
    .collect();
    let hash = crate::person_collaboration::digest(&serde_json::to_vec(
        &json!({"action":id,"version":row.get::<i64,_>("version"),"preview":preview,"order":observed,"person":actor.business_id(),"gateway":gateway,"chat":chat}),
    )?);
    let mut base: Value = row.get::<Option<Value>, _>("readback").unwrap_or(json!({}));
    let proposal = &base["order_proposal"];
    let current_proposal = proposal["hash"] == hash
        && proposal["expires"]
            .as_str()
            .and_then(|s| s.parse::<DateTime<Utc>>().ok())
            .is_some_and(|t| t > now);
    let fresh = turn.get::<DateTime<Utc>, _>("observed_at")
        > row.get::<DateTime<Utc>, _>("updated_at")
        && turn.get::<DateTime<Utc>, _>("observed_at") > now - chrono::Duration::minutes(15);
    let candidates:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.business_actions a JOIN qintopia_agent_os.business_turn_evidence e ON e.id=a.source_evidence_id WHERE a.tenant_key=$1 AND a.phase='manual_handoff' AND a.operation_key='pms.command.CREATE_ORDER' AND a.actor_person_id IS NOT DISTINCT FROM $2::uuid AND a.actor_work_account_id IS NOT DISTINCT FROM $5::uuid AND e.gateway_key=$3 AND e.chat_hash=$4")
        .bind(&store.tenant).bind(actor.business_person()).bind(&gateway).bind(&chat).bind(actor.work_account.map(|v|v.0)).fetch_one(&mut **tx).await?;
    let exact = &intent["manual_booking_exact"];
    let complete_report = candidates == 1
        && intent["manual_order"] == order
        && exact.is_object()
        && [
            "primaryGuest",
            "unit_code",
            "arrivalDate",
            "departureDate",
            "bookingChannelCode",
            "amountMinor",
        ]
        .iter()
        .all(|k| exact[k] == observed[k])
        && observed["stayType"] == "TRANSIENT"
        && observed["currency"] == "CNY"
        && observed["memberId"].is_null()
        && observed["occupants"]
            .as_array()
            .is_some_and(|v| v.len() == 1)
        && observed["segments"]
            .as_array()
            .is_some_and(|v| v.len() == 1);
    let approved =
        fresh && ((current_proposal && intent["manual_order_adoption"] == hash) || complete_report);
    if !approved {
        if !current_proposal {
            base["order_proposal"] = json!({"hash":hash,"order":order,"person":actor.business_id(),"gateway":gateway,"chat":chat,"created":now,"expires":now+chrono::Duration::minutes(15)});
            sqlx::query("UPDATE qintopia_agent_os.business_actions SET readback=$3 WHERE tenant_key=$1 AND id=$2")
                .bind(&store.tenant).bind(id).bind(base).execute(&mut **tx).await?;
        }
        return Ok(
            json!({"action":id,"phase":"manual_handoff","original_plan":original,"current_order":observed,"differences":differences,"confirmation_hint":"核对具体订单与以上差异后，确认此订单承接原订房事项。仅记录人工采纳，不表示原预览执行成功，也不认人、收款或办理入住"}),
        );
    }
    let saved = json!({"order":current["order"],"manual_evidence":{"kind":"human_order_adoption","original_version":row.get::<i64,_>("version"),"previewId":preview["previewId"],"proposal_hash":hash,"accepted_order":observed,"differences":differences,"confirmed_by_account":actor.work_account.map(|v|v.0),"confirmed_by_person":actor.business_person(),"confirmed_at":now}});
    sqlx::query("UPDATE qintopia_agent_os.business_actions SET phase='manual_completed',readback=$3,confirmation_evidence_id=$4,confirmed_by=$5,confirmed_by_work_account=$6,updated_at=clock_timestamp() WHERE tenant_key=$1 AND id=$2")
        .bind(&store.tenant).bind(id).bind(saved).bind(evidence).bind(actor.business_person()).bind(actor.work_account.map(|v|v.0)).execute(&mut **tx).await?;
    super::foundation::work_event(tx,work,"human_pms_order_adopted","anan",&json!({"action":id,"order_ref":order,"version":observed["version"],"evidence":evidence,"decision":"adopt_existing_order"})).await?;
    Ok(
        json!({"action":id,"phase":"manual_completed","readback":{"order":current["order"]},"completion_basis":"human_order_adoption"}),
    )
}
