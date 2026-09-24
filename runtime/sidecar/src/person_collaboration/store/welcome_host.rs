//! Local host-only presentation state; authenticated group replies use the UI command.
use super::{
    welcome_review::{artifact_ids, identity_relations_valid, snapshot, WelcomeSubject},
    Store,
};
use crate::person_collaboration::{
    foundation_server::TrustedContext, welcome_model::ReviewDecision,
};
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;
#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Request {
    Pending,
    Prepare {
        work_item: Uuid,
    },
    Claim {
        presentation: Uuid,
    },
    Receipt {
        presentation: Uuid,
        claim: Uuid,
        outcome: String,
        receipt: Option<String>,
    },
    Status {
        presentation: Uuid,
    },
    Callback,
    ConfirmationContext,
}
impl Store {
    async fn welcome_host_scope(&self, gateway: &str) -> Result<Uuid> {
        let rows:Vec<Uuid>=sqlx::query_scalar("SELECT scope_id FROM qintopia_identity.person_identity_gateways WHERE tenant_key=$1 AND gateway_key=$2 AND active").bind(&self.tenant).bind(gateway).fetch_all(&self.pool).await?;
        ensure!(rows.len() == 1, "gateway_scope_mismatch");
        Ok(rows[0])
    }
    async fn presentation_basis(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: Uuid,
        work: Uuid,
    ) -> Result<Value> {
        let row=sqlx::query("SELECT i.*,s.version AS config,s.configured_by,s.authority_refs,s.conversation_id,c.platform,c.chat_id,c.display_name FROM qintopia_agent_os.welcome_review_items i JOIN qintopia_agent_os.welcome_review_settings s ON s.tenant_key=i.tenant_key AND s.scope_id=i.scope_id JOIN qintopia_messages.conversations c ON c.id=s.conversation_id JOIN qintopia_agent_os.collaboration_scope_bindings b ON b.tenant_key=i.tenant_key AND b.scope_id=i.scope_id AND b.conversation_id=c.id WHERE i.tenant_key=$1 AND i.scope_id=$2 AND i.work_item_id=$3 AND c.status='active' AND b.revoked_at IS NULL FOR SHARE OF i,s,c,b").bind(&self.tenant).bind(scope).bind(work).fetch_one(&mut **tx).await?;
        ensure!(
            row.get::<i64, _>("configuration_version") == row.get::<i64, _>("config"),
            "configuration_version_conflict"
        );
        ensure!(
            matches!(
                row.get::<String, _>("status").as_str(),
                "pending" | "identity_confirmed" | "confirmed"
            ),
            "welcome_matter_closed"
        );
        let now = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut **tx)
            .await?;
        let policy = self.policy(tx, now).await?;
        let issuer: Uuid = row.get("configured_by");
        let refs: Vec<Uuid> = row.get("authority_refs");
        for effect in ["identity", "review"] {
            ensure!(
                policy
                    .manager(issuer, scope, "anan", "hospitality", effect)
                    .is_some_and(|g| refs.contains(&g.id)),
                "welcome_authority_revoked"
            );
        }
        let saved: Value = row.get("snapshot");
        let current = snapshot(
            tx,
            self,
            scope,
            row.get("case_id"),
            row.get("application_id"),
            &artifact_ids(&saved)?,
            row.get("confirmed_channel"),
        )
        .await?;
        ensure!(
            current == saved && current["valid"] == true && current["consent_active"] == true,
            "welcome_content_version_conflict"
        );
        let channels=sqlx::query("SELECT l.id,l.version,coalesce(nullif(ci.display_name,''),nullif(l.adapter_metadata->>'display_name',''),'待核对账号') AS label FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type LEFT JOIN qintopia_identity.channel_identities ci ON ci.id=l.channel_identity_id WHERE g.tenant_key=$1 AND g.scope_id=$2 AND g.active AND g.account_kind<>'shared' AND l.status<>'revoked' AND l.adapter_metadata ? 'first_observation_ref' ORDER BY l.id LIMIT 21").bind(&self.tenant).bind(scope).fetch_all(&mut **tx).await?;
        // A bounded presentation never silently pretends to contain every account.
        let has_more = channels.len() > 20;
        let source=sqlx::query("SELECT p.application_revision,p.identity_hash,p.field_hash,p.hints->>'name' AS name,p.hints->>'nickname' AS nickname FROM qintopia_agent_os.welcome_source_projections p JOIN qintopia_agent_os.business_property_bindings b ON b.id=p.binding_id AND b.tenant_key=p.tenant_key AND b.version=p.binding_version AND b.active WHERE p.tenant_key=$1 AND p.scope_id=$2 AND p.application_id=$3 AND p.expires_at>clock_timestamp() AND p.application_revision=$4 AND p.field_hash=$5").bind(&self.tenant).bind(scope).bind(row.get::<Uuid,_>("application_id")).bind(saved["application_revision"].as_i64()).bind(saved["application_hash"].as_str()).fetch_optional(&mut **tx).await?;
        let hint=source.map(|r|json!({"revision":r.get::<i64,_>("application_revision"),"identity_hash":r.get::<String,_>("identity_hash"),"field_hash":r.get::<String,_>("field_hash"),"label":format!("{} · {}",r.get::<String,_>("nickname"),r.get::<String,_>("name"))}));
        let contacts = self
            .welcome_contacts_for_application(tx, scope, row.get("application_id"))
            .await?;
        Ok(
            json!({"application":row.get::<Uuid,_>("application_id"),"case_ref":row.get::<Uuid,_>("case_id"),"contacts":contacts["evidence"],"work_item":work,"version":row.get::<i64,_>("version"),"configuration_version":row.get::<i64,_>("config"),"configured_by":issuer,"authority_refs":refs,"conversation":row.get::<Uuid,_>("conversation_id"),"platform":row.get::<String,_>("platform"),"chat_id":row.get::<String,_>("chat_id"),"group_label":row.get::<String,_>("display_name"),"review":saved,"candidates":row.get::<Value,_>("candidates"),"channels":channels.iter().take(20).map(|r|json!({"id":r.get::<Uuid,_>("id"),"version":r.get::<i64,_>("version"),"label":r.get::<String,_>("label")})).collect::<Vec<_>>(),"channels_has_more":has_more,"source_hint":hint}),
        )
    }
    pub(super) async fn welcome_presentation_current(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: Uuid,
        work: Uuid,
        id: Uuid,
        require_contacts: bool,
    ) -> Result<Value> {
        let row=sqlx::query("SELECT snapshot,status FROM qintopia_agent_os.welcome_group_presentations WHERE tenant_key=$1 AND scope_id=$2 AND work_item_id=$3 AND id=$4 FOR UPDATE").bind(&self.tenant).bind(scope).bind(work).bind(id).fetch_one(&mut **tx).await?;
        ensure!(
            !matches!(row.get::<String, _>("status").as_str(), "failed" | "stale"),
            "presentation_stale"
        );
        let saved: Value = row.get("snapshot");
        let current = self.presentation_basis(tx, scope, work).await?;
        let mut old = saved.clone();
        let mut new = current.clone();
        old.as_object_mut().unwrap().remove("contacts");
        new.as_object_mut().unwrap().remove("contacts");
        ensure!(
            old == new && (!require_contacts || saved["contacts"] == current["contacts"]),
            "presentation_stale"
        );
        Ok(saved)
    }
    pub(in crate::person_collaboration) async fn welcome_host(
        &self,
        gateway: &str,
        t: TrustedContext,
        r: Request,
    ) -> Result<Value> {
        ensure!(t.gateway_id == gateway, "gateway_scope_mismatch");
        let scope = self.welcome_host_scope(gateway).await?;
        if matches!(r, Request::ConfirmationContext) {
            return self.welcome_confirmation_context(gateway, t, scope).await;
        }
        if matches!(r, Request::Callback) {
            return self.welcome_group_callback(gateway, t, scope).await;
        }
        let (mut tx, _, _) = self.begin().await?;
        if matches!(r, Request::Pending) {
            let rows:Vec<Uuid>=sqlx::query_scalar("SELECT work_item_id FROM qintopia_agent_os.welcome_review_items WHERE tenant_key=$1 AND scope_id=$2 AND status IN ('pending','identity_confirmed') ORDER BY work_item_id LIMIT 50").bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
            return Ok(json!({"work_items":rows,"local_only":true}));
        }
        if let Request::Prepare { work_item } = r {
            let mut basis = self.presentation_basis(&mut tx, scope, work_item).await?;
            if let Some(row)=sqlx::query("SELECT id,reference,status,snapshot FROM qintopia_agent_os.welcome_group_presentations WHERE work_item_id=$1 AND review_version=$2 AND configuration_version=$3").bind(work_item).bind(basis["version"].as_i64()).bind(basis["configuration_version"].as_i64()).fetch_optional(&mut *tx).await? {
                let status:String=row.get("status");
                if row.get::<Value,_>("snapshot")==basis || matches!(status.as_str(),"claimed"|"unknown"|"failed") {
                    return Ok(json!({"presentation":row.get::<Uuid,_>("id"),"reference":row.get::<String,_>("reference"),"status":status,"replayed":true}));
                }
                // A changed candidate display needs a new immutable reference/version.
                // Never replace an unresolved external attempt or mutate a sent reference.
                sqlx::query("UPDATE qintopia_agent_os.welcome_group_presentations SET status='stale',updated_at=clock_timestamp() WHERE id=$1").bind(row.get::<Uuid,_>("id")).execute(&mut *tx).await?;
                let version:i64=sqlx::query_scalar("UPDATE qintopia_agent_os.welcome_review_items SET version=version+1 WHERE work_item_id=$1 RETURNING version").bind(work_item).fetch_one(&mut *tx).await?;
                basis["version"]=json!(version);
            }
            let id = Uuid::new_v4();
            let reference = format!("W-{}", &id.simple().to_string()[..12]);
            sqlx::query("INSERT INTO qintopia_agent_os.welcome_group_presentations(id,reference,tenant_key,scope_id,work_item_id,review_version,configuration_version,conversation_id,subject_kind,subject_id,subject_version,subject_proof,snapshot) VALUES($1,$2,$3,$4,$5,$6,$7,$8,'person',$9,$7,$10,$11)").bind(id).bind(&reference).bind(&self.tenant).bind(scope).bind(work_item).bind(basis["version"].as_i64()).bind(basis["configuration_version"].as_i64()).bind(serde_json::from_value::<Uuid>(basis["conversation"].clone())?).bind(serde_json::from_value::<Uuid>(basis["configured_by"].clone())?).bind(json!({"configuration_authority":basis["authority_refs"],"approval":false})).bind(&basis).execute(&mut *tx).await?;
            tx.commit().await?;
            return Ok(
                json!({"presentation":id,"reference":reference,"status":"pending","text":render(&reference,&basis),"local_only":true}),
            );
        }
        let id = match &r {
            Request::Claim { presentation }
            | Request::Receipt { presentation, .. }
            | Request::Status { presentation } => *presentation,
            _ => unreachable!(),
        };
        let row=sqlx::query("SELECT *,claim_until<clock_timestamp() AS expired FROM qintopia_agent_os.welcome_group_presentations WHERE tenant_key=$1 AND scope_id=$2 AND id=$3 FOR UPDATE").bind(&self.tenant).bind(scope).bind(id).fetch_one(&mut *tx).await?;
        let mut status: String = row.get("status");
        if status == "claimed" && row.get::<Option<bool>, _>("expired") == Some(true) {
            sqlx::query("UPDATE qintopia_agent_os.welcome_group_presentations SET status='unknown',updated_at=clock_timestamp() WHERE id=$1").bind(id).execute(&mut *tx).await?;
            status = "unknown".into();
        }
        let reference: String = row.get("reference");
        match r {
            Request::Status { .. } => {
                tx.commit().await?;
                Ok(
                    json!({"presentation":id,"reference":reference,"status":status,"receipt":row.get::<Option<String>,_>("delivery_receipt")}),
                )
            }
            Request::Claim { .. } => {
                if status != "pending" {
                    tx.commit().await?;
                    return Ok(json!({"presentation":id,"status":status,"send":false}));
                }
                let current = self
                    .welcome_presentation_current(&mut tx, scope, row.get("work_item_id"), id, true)
                    .await;
                let Ok(basis) = current else {
                    sqlx::query("UPDATE qintopia_agent_os.welcome_group_presentations SET status='stale',updated_at=clock_timestamp() WHERE id=$1").bind(id).execute(&mut *tx).await?;
                    tx.commit().await?;
                    return Ok(json!({"presentation":id,"status":"stale","send":false}));
                };
                let claim = Uuid::new_v4();
                sqlx::query("UPDATE qintopia_agent_os.welcome_group_presentations SET status='claimed',claim_id=$2,claim_until=clock_timestamp()+interval '60 seconds',updated_at=clock_timestamp() WHERE id=$1").bind(id).bind(claim).execute(&mut *tx).await?;
                tx.commit().await?;
                Ok(
                    json!({"presentation":id,"claim":claim,"status":"claimed","send":true,"platform":basis["platform"],"chat_id":basis["chat_id"],"text":render(&reference,&basis),"artifacts":basis["review"]["artifacts"],"local_only":true}),
                )
            }
            Request::Receipt {
                claim,
                outcome,
                receipt,
                ..
            } => {
                ensure!(
                    row.get::<Option<Uuid>, _>("claim_id") == Some(claim),
                    "claim_mismatch"
                );
                ensure!(
                    matches!(outcome.as_str(), "delivered" | "failed" | "unknown")
                        && receipt
                            .as_ref()
                            .is_none_or(|v| !v.is_empty() && v.chars().count() <= 200),
                    "invalid_receipt"
                );
                ensure!(
                    outcome != "delivered" || receipt.is_some(),
                    "delivery_receipt_required"
                );
                if status == outcome && row.get::<Option<String>, _>("delivery_receipt") == receipt
                {
                    tx.commit().await?;
                    return Ok(json!({"status":status,"replayed":true}));
                }
                // UNKNOWN may only settle the original receipt, never become retryable.
                ensure!(
                    status == "claimed" || (status == "unknown" && outcome == "delivered"),
                    "delivery_outcome_conflict"
                );
                sqlx::query("UPDATE qintopia_agent_os.welcome_group_presentations SET status=$2,delivery_receipt=$3,updated_at=clock_timestamp() WHERE id=$1").bind(id).bind(&outcome).bind(receipt).execute(&mut *tx).await?;
                tx.commit().await?;
                Ok(json!({"status":outcome,"send":false}))
            }
            _ => unreachable!(),
        }
    }
    async fn group_decision(
        &self,
        gateway: &str,
        t: TrustedContext,
        scope: Uuid,
    ) -> Result<(WelcomeSubject, ReviewDecision, Uuid, Value, bool)> {
        ensure!(
            t.chat_type == "group" && matches!(t.platform.as_str(), "wecom" | "qiwe"),
            "configured_operations_group_required"
        );
        let row=sqlx::query("SELECT m.id,m.text,s.version FROM qintopia_messages.messages m JOIN qintopia_messages.raw_events r ON r.id=m.raw_event_id JOIN qintopia_agent_os.welcome_review_settings s ON s.tenant_key=m.tenant_id AND s.scope_id=$6 JOIN qintopia_messages.conversations c ON c.id=s.conversation_id AND c.platform=m.platform AND c.chat_id=m.chat_id JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=s.tenant_key AND g.scope_id=s.scope_id AND g.gateway_key=$7 AND g.active WHERE m.tenant_id=$1 AND m.platform=$2 AND m.chat_id=$3 AND m.sender_id=$4 AND m.message_id=$5 AND m.chat_type='group' AND m.sent_at IS NOT NULL AND r.ingress_auth_verified AND r.subject=$8 AND ((m.platform='qiwe' AND g.subject_type='qiwe_sender') OR (m.platform='wecom' AND g.subject_type IN ('wecom_internal','wecom_external')))").bind(&self.tenant).bind(&t.platform).bind(&t.chat_id).bind(&t.sender_id).bind(&t.message_id).bind(scope).bind(gateway).bind(format!("qintopia.{}.raw.authenticated",t.platform)).fetch_one(&self.pool).await?;
        let text: String = row.get::<Option<String>, _>("text").unwrap_or_default();
        let reference = text
            .split_whitespace()
            .nth(1)
            .ok_or_else(|| anyhow::anyhow!("explicit_group_confirmation_required"))?;
        let presentation=sqlx::query("SELECT id,snapshot,status FROM qintopia_agent_os.welcome_group_presentations WHERE tenant_key=$1 AND scope_id=$2 AND reference=$3").bind(&self.tenant).bind(scope).bind(reference).fetch_one(&self.pool).await?;
        ensure!(
            matches!(
                presentation.get::<String, _>("status").as_str(),
                "delivered" | "unknown" | "claimed"
            ),
            "presentation_not_delivered"
        );
        let decision = parse(
            &text,
            &presentation.get::<Value, _>("snapshot"),
            row.get("id"),
        )?;
        let subject = WelcomeSubject::Group {
            subject: Box::new(self.welcome_gateway_subject(gateway, &t.sender_id).await?),
            scope,
            platform: t.platform,
            chat: t.chat_id,
            configuration_version: row.get("version"),
        };
        Ok((
            subject,
            decision,
            presentation.get("id"),
            presentation.get("snapshot"),
            text.split_whitespace().any(|w| w == "人工核对"),
        ))
    }
    pub(super) async fn welcome_confirmation_context(
        &self,
        gateway: &str,
        t: TrustedContext,
        scope: Uuid,
    ) -> Result<Value> {
        let (subject, r, presentation, saved, manual) =
            self.group_decision(gateway, t, scope).await?;
        let (mut tx, _, _) = self.begin().await?;
        let mut effects = Vec::new();
        if r.confirm_application_stay
            || r.confirm_channel_person
            || r.decision == "create_person"
            || r.decision == "revoke"
        {
            effects.push("identity");
        }
        if r.confirm_content || matches!(r.decision.as_str(), "reject" | "revoke") {
            effects.push("review");
        }
        ensure!(!effects.is_empty(), "explicit_effect_required");
        self.welcome_authorize(&mut tx, &subject, scope, &effects)
            .await?;
        if let Some(receipt)=sqlx::query("SELECT request_hash,subject_kind,subject_id FROM qintopia_agent_os.welcome_review_receipts WHERE id=$1 AND tenant_key=$2").bind(r.operation_id).bind(&self.tenant).fetch_optional(&mut *tx).await? {
            let reference=subject.reference();
            ensure!(receipt.get::<String,_>("request_hash")==crate::person_collaboration::digest(&serde_json::to_vec(&r)?) && receipt.get::<String,_>("subject_kind")==reference.kind() && receipt.get::<Uuid,_>("subject_id")==reference.id(),"idempotency_conflict");
            return Ok(json!({"requires_contacts":false,"work_item":null,"presentation":presentation,"replayed":true}));
        }
        self.welcome_presentation_current(&mut tx, scope, r.work_item, presentation, false)
            .await?;
        let requires = contact_dependency(&saved, &r, manual);
        Ok(
            json!({"requires_contacts":requires,"work_item":if requires {saved["contacts"]["work_item"].clone()} else {Value::Null},"presentation":presentation,"replayed":false}),
        )
    }
    async fn welcome_group_callback(
        &self,
        gateway: &str,
        t: TrustedContext,
        scope: Uuid,
    ) -> Result<Value> {
        let (subject, r, presentation, _, _) = self.group_decision(gateway, t, scope).await?;
        self.welcome_review_decide_presented(&subject, &r, Some(presentation))
            .await
    }
}
pub(super) fn contact_dependency(saved: &Value, r: &ReviewDecision, manual: bool) -> bool {
    if manual || saved["contacts"].is_null() {
        return false;
    }
    if r.decision == "create_person" {
        return true;
    }
    if r.decision != "confirm" || !r.confirm_application_stay {
        return false;
    }
    !serde_json::from_value::<Uuid>(saved["application"].clone())
        .ok()
        .is_some_and(|application| {
            identity_relations_valid(&saved["review"], application, r.person)
        })
}
fn render(reference: &str, v: &Value) -> String {
    let mut s = format!(
        "岸岸：请核对客房事项 {reference}\n申请：{}；楼栋：{}\n",
        v["source_hint"]["label"].as_str().unwrap_or("待核对申请"),
        v["review"]["building"].as_str().unwrap_or("待核对")
    );
    if !v["contacts"].is_null() {
        let selected = v["case_ref"].as_str().unwrap_or("");
        let reason = match v["contacts"]["comparisons"][selected].as_str() {
            Some("match") => "申请与本次入住人的手机号一致，仍需核对本人",
            Some("different") => "申请与本次入住人的手机号不同，请核对是否填错或换号",
            Some("missing") => "本次入住人缺少手机号，需核对原始入住资料",
            Some("unusable") => "本次入住人手机号格式无法核对，需核对原始资料",
            Some("application_missing") => "历史申请缺少手机号，可核对原始资料",
            Some("application_unusable") => "申请手机号格式无法核对，可核对原始资料",
            _ => "此入住人没有当前电话核对依据，请核对原始资料",
        };
        s.push_str(&format!("电话核对：{reason}。\n"));
        s.push_str("电话不能证明微信账号归属。提交时自动复验；若已独立核对原始资料，可在命令末尾加“人工核对”。\n");
        if v["contacts"]["ambiguous"] == true {
            s.push_str("多个入住候选使用相同电话，请核对实际入住人。\n");
        }
    } else {
        s.push_str("本事项尚未使用完整电话核对，请依据原始入住资料确认。\n");
    }
    for (i, c) in v["candidates"].as_array().into_iter().flatten().enumerate() {
        s.push_str(&format!(
            "人员{}：{}\n",
            i + 1,
            c["label"].as_str().unwrap_or("待核对人员")
        ));
    }
    for (i, c) in v["channels"].as_array().into_iter().flatten().enumerate() {
        s.push_str(&format!(
            "账号{}：{}\n",
            i + 1,
            c["label"].as_str().unwrap_or("待核对账号")
        ));
    }
    for a in v["review"]["artifacts"].as_array().into_iter().flatten() {
        if let Some(t) = a["text"].as_str() {
            s.push_str(&format!("内容：{t}\n"));
        }
    }
    let create = if v["source_hint"]["label"].is_string()
        && v["review"]["occupant_source"]["status"] == "pending"
        && v["review"]["relations"]["application_person"].is_null()
    {
        format!("按上方申请姓名建档并确认所选实际入住人来源：建档 {reference}（仅此效果）\n")
    } else {
        String::new()
    };
    s.push_str(&format!("候选不是已确认身份。只写你核对过的项目，例如：\n确认 {reference} 人员1 账号1 关联住宿 关联账号 内容\n可省略未核对的项目；仅核对住宿：确认 {reference} 人员1 关联住宿\n{create}退回：退回 {reference}；撤销：撤销 {reference}\n不能确定请保留待办；账号未列全可在页面搜索。确认不等于发布。"));
    s
}
fn parse(text: &str, v: &Value, operation: Uuid) -> Result<ReviewDecision> {
    let words: Vec<_> = text.split_whitespace().collect();
    ensure!(
        words.len() >= 2 && words.len() <= 8,
        "explicit_group_confirmation_required"
    );
    let decision = match words[0] {
        "确认" => "confirm",
        "退回" => "reject",
        "撤销" => "revoke",
        "建档" => "create_person",
        _ => anyhow::bail!("explicit_group_confirmation_required"),
    };
    let mut r = ReviewDecision {
        operation_id: operation,
        work_item: serde_json::from_value(v["work_item"].clone())?,
        expected_version: v["version"]
            .as_i64()
            .ok_or_else(|| anyhow::anyhow!("presentation_stale"))?,
        person: None,
        channel: None,
        confirm_application_stay: false,
        confirm_channel_person: false,
        confirm_content: false,
        decision: decision.into(),
    };
    let mut seen = std::collections::HashSet::new();
    for w in &words[2..] {
        ensure!(seen.insert(*w), "explicit_group_confirmation_required");
        match *w {
            "人工核对" => {}
            "关联住宿" => r.confirm_application_stay = true,
            "关联账号" => r.confirm_channel_person = true,
            "内容" => r.confirm_content = true,
            _ => {
                let (prefix, key, id) = if w.starts_with("人员") {
                    ("人员", "candidates", "person")
                } else if w.starts_with("账号") {
                    ("账号", "channels", "id")
                } else {
                    anyhow::bail!("explicit_group_confirmation_required")
                };
                let index: usize = w.trim_start_matches(prefix).parse()?;
                ensure!(index > 0, "candidate_selection_required");
                let value = v[key]
                    .as_array()
                    .and_then(|a| a.get(index - 1))
                    .ok_or_else(|| anyhow::anyhow!("candidate_selection_required"))?;
                let selected = serde_json::from_value(value[id].clone())?;
                if key == "candidates" {
                    ensure!(r.person.is_none(), "candidate_selection_required");
                    r.person = Some(selected);
                } else {
                    ensure!(r.channel.is_none(), "candidate_selection_required");
                    r.channel = Some(selected);
                }
            }
        }
    }
    ensure!(
        decision == "confirm"
            || words.len() == 2
            || (decision == "create_person" && words.len() == 3 && words[2] == "人工核对"),
        "explicit_group_confirmation_required"
    );
    Ok(r)
}
