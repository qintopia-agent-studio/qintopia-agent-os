//! Password UI and authenticated local tool broker; no production fallback.
use super::{Actor, KnowledgeWrite, Store};
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ContextRequest {
    scope: Uuid,
    #[serde(default = "general")]
    topic: String,
}
fn general() -> String {
    "general".into()
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct DispatchRequest {
    scope: Uuid,
    operation_id: Uuid,
    expected_version: i64,
    brief: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WorkRequest {
    work_item_id: Uuid,
    action: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TalkRequest {
    scope: Uuid,
    text: String,
    operation_id: Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct WithdrawRequest {
    operation_id: Uuid,
    scope: Uuid,
    revision_id: Uuid,
    expected_version: i64,
}

pub(super) async fn dispatch(
    store: &Store,
    actor: &Actor,
    path: &str,
    body: &[u8],
) -> Result<Value> {
    ensure!(
        std::env::var("QINTOPIA_FOUNDATION_LOCAL_ENABLE").as_deref() == Ok("1"),
        "foundation_disabled"
    );
    if !body.is_empty() {
        crate::strict_json::parse_strict_bounded_slice(
            body,
            crate::strict_json::registry_json_limits(256 * 1024),
        )?;
    }
    match path {
        "/api/foundation/state" => {
            let person = store.verified_person(actor).await?;
            let welcome = crate::resident_welcome::store::Store {
                pool: store.pool.clone(),
            }
            .foundation_state(&store.tenant, person)
            .await?;
            Ok(
                json!({"configuration":store.state(actor).await?,"identity":store.identity_context(actor).await?,"memory":store.memory_context(actor,"general").await?,"history":store.person_history(actor).await?,"welcome":welcome,"works":store.foundation_work_list(actor).await?}),
            )
        }
        "/api/foundation/context" => {
            let r: ContextRequest = serde_json::from_slice(body)?;
            store.foundation_context(actor, r.scope, &r.topic).await
        }
        "/api/foundation/rule" => {
            let r: KnowledgeWrite = serde_json::from_slice(body)?;
            store.knowledge_save(actor, &r, false).await
        }
        "/api/foundation/rule/withdraw" => {
            let r: WithdrawRequest = serde_json::from_slice(body)?;
            store
                .knowledge_withdraw(
                    actor,
                    r.operation_id,
                    r.scope,
                    r.revision_id,
                    r.expected_version,
                )
                .await
        }
        "/api/foundation/memory" => {
            let r: super::store::MemoryCommand = serde_json::from_slice(body)?;
            let evidence = store.turn_evidence(actor, r.operation_id).await?;
            store.remember(actor, &r, &evidence).await
        }
        "/api/foundation/dispatch" => {
            let r: DispatchRequest = serde_json::from_slice(body)?;
            store
                .dispatch_context(actor, r.scope, r.operation_id, r.expected_version, &r.brief)
                .await
        }
        "/api/foundation/work" => {
            let r: WorkRequest = serde_json::from_slice(body)?;
            store.foundation_work_status(actor, r.work_item_id).await?;
            match r.action.as_str() {
                "status" => store.foundation_work_status(actor, r.work_item_id).await,
                "execute" => store.foundation_execute_work(r.work_item_id).await,
                "approve" => store.foundation_approve_rule(actor, r.work_item_id).await,
                _ => anyhow::bail!("unknown_action"),
            }
        }
        "/api/foundation/talk" => {
            let r: TalkRequest = serde_json::from_slice(body)?;
            scripted_turn(store, actor, &r).await
        }
        "/api/foundation/welcome" => welcome(store, actor, serde_json::from_slice(body)?).await,
        _ => anyhow::bail!("unknown_route"),
    }
}

async fn scripted_turn(store: &Store, actor: &Actor, r: &TalkRequest) -> Result<Value> {
    use super::store::{MemoryChange, MemoryCommand, ReplyCondition, ReplyStyle};
    ensure!(r.text.chars().count() <= 4000, "invalid_text");
    let text = r.text.trim().trim_end_matches(['。', '！']);
    let turn_hash = super::digest(&serde_json::to_vec(&json!({"scope":r.scope,"text":text}))?);
    if text.contains("欢迎")
        || text.starts_with("批准卡片版本 ")
        || text.starts_with("批准文案版本 ")
    {
        return scripted_welcome_turn(store, actor, r, text, &turn_hash).await;
    }
    let memory_change = match text {
        "以后回答我简短一点" => Some(MemoryChange::Set {
            condition: ReplyCondition::General,
            style: ReplyStyle::Brief,
        }),
        "涉及费用还是详细说清楚" => Some(MemoryChange::Set {
            condition: ReplyCondition::Fees,
            style: ReplyStyle::Detailed,
        }),
        "以后不用记我的回复习惯" => Some(MemoryChange::Stop),
        _ => None,
    };
    let self_only = memory_change.is_some()
        || ["认识我吗", "我现在的回复习惯是什么", "查询费用说明"].contains(&text);
    let context = if self_only {
        json!({"identity":store.identity_context(actor).await?,"memory":store.memory_context(actor,if r.text.contains("费用"){"fees"}else{"general"}).await?})
    } else {
        store.foundation_context(actor, r.scope, "general").await?
    };
    let (reply, result) = if let Some(change) = memory_change {
        let evidence = store.turn_evidence(actor, r.operation_id).await?;
        let stopped = matches!(change, MemoryChange::Stop);
        let input = store
            .foundation_turn_input(
                actor,
                r.operation_id,
                &turn_hash,
                &serde_json::to_value(MemoryCommand {
                    operation_id: r.operation_id,
                    expected_version: context["memory"]["version"].as_i64().unwrap_or(0),
                    change,
                })?,
            )
            .await?;
        let mut saved = store
            .remember(actor, &serde_json::from_value(input)?, &evidence)
            .await?;
        saved["current_memory"] = store.memory_context(actor, "general").await?;
        let reply = if saved["replayed"] == true {
            if saved["current_memory"]["status"] == "stopped" {
                "这是旧请求的保存回执；当前已停止使用回复习惯，未恢复旧偏好。"
            } else {
                "这是旧请求的保存回执；当前使用最新有效偏好，未重新保存旧请求。"
            }
        } else if stopped {
            "已停止使用你的回复习惯；下次对话也会按最新选择处理。"
        } else {
            "已保存你的回复习惯，下次对话会读取这次选择。"
        };
        (reply.to_string(), saved)
    } else if text == "把本栋厨房关闭时间改成晚上十点" {
        let version = context["knowledge_items"]
            .as_array()
            .and_then(|a| {
                a.iter()
                    .find(|v| v["key"] == "kitchen" && v["scope"] == json!(r.scope))
            })
            .and_then(|v| v["latest_version"].as_i64())
            .unwrap_or(0);
        let input = store
            .foundation_turn_input(
                actor,
                r.operation_id,
                &turn_hash,
                &serde_json::to_value(KnowledgeWrite {
                    operation_id: r.operation_id,
                    expected_version: version,
                    scope: r.scope,
                    key: "kitchen".into(),
                    kind: "rule".into(),
                    shared: false,
                    case_ref: None,
                    content: json!({"text":"本栋厨房每天晚上十点关闭。"}),
                    effective_at: None,
                    effective_until: None,
                })?,
            )
            .await?;
        let saved = store
            .knowledge_save(actor, &serde_json::from_value(input)?, false)
            .await?;
        let reply = if saved["replayed"] == true {
            "这是原规则请求的持久回执，未创建新版本；请以当前有效知识为准。".into()
        } else if saved["status"] == "saved" {
            format!(
                "已保存本栋厨房规则：每天晚上十点关闭。从 {} 起生效。尚无群通知约定。",
                saved["knowledge"]["effective_at"]
                    .as_str()
                    .unwrap_or("本次保存")
            )
        } else {
            "请求已保存，等待当前权限指定的确认人批准；现行规则保持不变。".into()
        };
        (reply, saved)
    } else if text == "我觉得十点关可能更好" {
        (
            "这是一个建议，现行规则尚未修改。明确决定后可说“把本栋厨房关闭时间改成晚上十点”。"
                .into(),
            json!({"status":"proposal","persisted_rule":false}),
        )
    } else if text == "把厨房关闭时间改早一点" {
        (
            "请说明要改为几点关闭。".into(),
            json!({"status":"needs_time","persisted_rule":false}),
        )
    } else if [
        "认识我吗",
        "我现在的回复习惯是什么",
        "本栋有什么规则",
        "查询费用说明",
    ]
    .contains(&text)
    {
        let preference = context["memory"]["reply_style"]
            .as_str()
            .unwrap_or("unknown");
        let label = context["identity"]["preferred_name"]
            .as_str()
            .unwrap_or("已核验的你");
        (
            format!(
                "我认得你是{label}。本次只读取当前范围内的知识。回复习惯：{}。",
                match preference {
                    "brief" => "简短",
                    "detailed" => "详细说明",
                    _ => "没有适用的已保存偏好",
                }
            ),
            context,
        )
    } else {
        ("本地合成对话仅支持页面列出的验收例句。任意自然语言理解由 Hermes 的实际模型工具入口验证；此处未调用真实模型。".into(),json!({"status":"unsupported_synthetic_expression"}))
    };
    Ok(json!({"reply":reply,"tool_result":result,"model_evidence":"scripted_local_adapter"}))
}

fn welcome_version_code(artifact: &Value) -> String {
    let prefix = if artifact["kind"] == "welcome_card" {
        "A"
    } else {
        "T"
    };
    let identity = artifact["artifact_ref"]
        .as_str()
        .unwrap_or_default()
        .replace('-', "");
    format!(
        "{prefix}{}",
        identity.chars().take(10).collect::<String>().to_uppercase()
    )
}

fn welcome_examples() -> Vec<String> {
    [
        "查看本栋欢迎待办",
        "以后本栋欢迎直接发，卡片和文案",
        "以后本栋欢迎先审，卡片和文案",
        "本次欢迎直接发，卡片和文案",
        "本次欢迎先审，卡片和文案",
        "为本次欢迎制卡",
        "查看欢迎内容版本",
        "推进本次欢迎",
    ]
    .into_iter()
    .map(str::to_string)
    .collect()
}

fn welcome_response(reply: String, result: Value, target: Option<&Value>) -> Value {
    let mut attachments = Vec::new();
    let mut suggestions = welcome_examples();
    if let Some(target) = target {
        for artifact in target["artifacts"].as_array().into_iter().flatten() {
            let code = welcome_version_code(artifact);
            let image = artifact["kind"] == "welcome_card";
            let label = format!("{}版本 {code}", if image { "卡片" } else { "文案" });
            attachments.push(json!({"kind":if image {"image"}else{"text"},"artifact_ref":artifact["artifact_ref"],"content_hash":artifact["content_hash"],"label":label,"text":artifact["text"]}));
            let pending_review = target["progress"]
                .as_array()
                .into_iter()
                .flatten()
                .any(|p| {
                    p["waiting"].as_array().into_iter().flatten().any(|w| {
                        w["artifact_ref"] == artifact["artifact_ref"]
                            && matches!(
                                w["reason"].as_str(),
                                Some("content_approval_required" | "upper_confirmation_required")
                            )
                    })
                });
            if pending_review {
                suggestions.push(format!(
                    "批准{}版本 {code}",
                    if image { "卡片" } else { "文案" }
                ));
            }
        }
    }
    json!({"reply":reply,"tool_result":result,"attachments":attachments,"suggestions":suggestions,"model_evidence":"scripted_local_adapter"})
}

async fn welcome_target_state(store: &Store, actor: &Actor, scope: Uuid) -> Result<Vec<Value>> {
    store.foundation_assert_scope(actor, scope).await?;
    let person = store.verified_person(actor).await?;
    let state = crate::resident_welcome::store::Store {
        pool: store.pool.clone(),
    }
    .foundation_state(&store.tenant, person)
    .await?;
    Ok(state["targets"]
        .as_array()
        .into_iter()
        .flatten()
        .filter(|t| t["scope_ref"] == json!(scope))
        .cloned()
        .collect())
}

fn welcome_summary(target: &Value) -> String {
    let settings = target["rule"]["content"]
        .as_object()
        .map(|_| {
            format!(
                "持续设置：{}，规则第{}版。",
                if target["rule"]["content"]["mode"] == "direct" {
                    "按已获授权直接发送"
                } else {
                    "具体内容先审"
                },
                target["rule"]["version"]
            )
        })
        .unwrap_or_else(|| "尚无持续设置。".into());
    let cases = target["cases"].as_array().map_or(0, Vec::len);
    let mut details = vec![];
    for rule in target["case_rules"].as_array().into_iter().flatten() {
        details.push(format!(
            "本次设置记录：{}，第{}版",
            if rule["content"]["mode"] == "review" {
                "具体内容先审"
            } else {
                "按已获授权直接发送"
            },
            rule["version"]
        ));
    }
    for action in target["actions"].as_array().into_iter().flatten() {
        let status = match action["status"].as_str().unwrap_or("") {
            "prepared" => "已备妥，待推进",
            "succeeded" => "本地测试发送成功",
            "unknown" => "结果待核对，不会自动重发",
            "cancelled" => "已取消",
            "retryable" => "明确未发送，等待重新评估",
            _ => "等待处理",
        };
        details.push(format!(
            "{}：{status}",
            if action["part"] == "image" {
                "卡片"
            } else {
                "文案"
            }
        ));
    }
    for progress in target["progress"].as_array().into_iter().flatten() {
        for pending in progress["waiting"].as_array().into_iter().flatten() {
            let reason = welcome_reason(pending["reason"].as_str().unwrap_or(""));
            details.push(format!(
                "{}：{reason}",
                if pending["part"] == "image" {
                    "卡片"
                } else {
                    "文案"
                }
            ));
        }
    }
    format!(
        "{}。{settings}当前有{cases}项住宿欢迎。{}",
        target["label"].as_str().unwrap_or("本栋"),
        details.join("；")
    )
}

fn welcome_reason(reason: &str) -> &'static str {
    match reason {
        "content_approval_required" => "等待具体内容批准",
        "upper_confirmation_required" => "等待上层指定确认人批准",
        "content_required" => "内容尚未备妥",
        "source_stale" | "pms_stale" | "source_not_ready" => {
            "来源时效不足或尚未就绪，等待可靠来源更新"
        }
        "card_not_eligible" | "case_not_ready" => "有效申请、身份或住宿条件尚未齐备",
        "actual_check_in_required" => "等待实际入住",
        "current_target_membership_required" => "等待确认已加入这个目标群",
        "welcome_version_conflict" | "review_version_conflict" | "content_version_changed" => {
            "事项或内容版本已改变，请重新查看当前版本"
        }
        "designated_reviewer_required" => "需要当前指定确认人批准",
        "publish_authority_required"
        | "review_authority_required"
        | "upper_confirmation_authority_required"
        | "scope_access_denied" => "当前人员没有这项有效授权，请由管理员核实",
        "content_review_not_required" | "publish_confirmation_not_required" => {
            "该部分已有适用授权，无需增加这项批准"
        }
        "executor_unavailable" | "agent_runtime_unavailable" | "agent_runtime_timeout" => {
            "执行智能体暂不可用，任务等待恢复"
        }
        _ => "当前条件尚未齐备，需核对事项来源或执行记录",
    }
}

async fn scripted_welcome_turn(
    store: &Store,
    actor: &Actor,
    r: &TalkRequest,
    text: &str,
    hash: &str,
) -> Result<Value> {
    let targets = welcome_target_state(store, actor, r.scope).await?;
    if targets.len() != 1 {
        return Ok(welcome_response(
            if targets.is_empty() {
                "当前范围没有可处理的欢迎目标，请先由管理员完成本栋群和来源绑定。"
            } else {
                "当前范围有多个欢迎目标，请先选定具体楼栋范围；我不会按姓名猜测目标。"
            }
            .into(),
            json!({"status":"needs_target"}),
            None,
        ));
    }
    let target = &targets[0];
    if matches!(text, "查看本栋欢迎待办" | "查看欢迎内容版本") {
        return Ok(welcome_response(
            welcome_summary(target),
            json!({"status":"current_state","welcome":target}),
            Some(target),
        ));
    }
    let cases = target["cases"].as_array().cloned().unwrap_or_default();
    let continuous = text.starts_with("以后本栋欢迎");
    let setting = if continuous {
        text.strip_prefix("以后本栋欢迎")
    } else {
        text.strip_prefix("本次欢迎")
    }
    .and_then(|s| match s {
        "直接发，卡片和文案" => Some("direct"),
        "先审，卡片和文案" => Some("review"),
        _ => None,
    });
    if !continuous && cases.len() != 1 {
        return Ok(welcome_response(
            "本栋当前不是唯一一项住宿欢迎，请先明确要处理的事项；不会按姓名合并或默认选择。".into(),
            json!({"status":"needs_case","welcome":target}),
            Some(target),
        ));
    }
    let target_ref: Uuid = serde_json::from_value(target["target_ref"].clone())?;
    let case = cases
        .first()
        .map(|v| serde_json::from_value::<Uuid>(v["case_ref"].clone()))
        .transpose()?;
    let request = if let Some(mode) = setting {
        let case_ref = if continuous { None } else { case };
        let version = if continuous {
            target["latest_rule_version"].as_i64().unwrap_or(0)
        } else {
            target["case_rules"]
                .as_array()
                .into_iter()
                .flatten()
                .find(|v| v["case_ref"] == json!(case_ref))
                .and_then(|v| v["version"].as_i64())
                .unwrap_or(0)
        };
        json!({"action":"configure","target_ref":target_ref,"write":KnowledgeWrite{operation_id:r.operation_id,expected_version:version,scope:r.scope,key:"resident_welcome".into(),kind:"rule".into(),shared:false,case_ref,content:json!({"mode":mode,"parts":["image","text"],"phase":"formal","text_template":"欢迎 {name} 来到社区，一起从容生活。"}),effective_at:None,effective_until:None}})
    } else if matches!(text, "为本次欢迎制卡" | "推进本次欢迎") {
        let case = case.ok_or_else(|| anyhow::anyhow!("welcome_case_required"))?;
        let row=sqlx::query("SELECT c.version,a.revision,coalesce(p.preferred_name,p.display_name) AS name FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_applications a ON a.id=c.application_id JOIN qintopia_identity.persons p ON p.id=c.person_id JOIN qintopia_agent_os.welcome_sources s ON s.source_instance=c.source_instance AND s.property_id=c.property_id WHERE c.id=$1 AND s.mode='synthetic'").bind(case).fetch_one(&store.pool).await?;
        if text == "为本次欢迎制卡" {
            json!({"action":"render","target_ref":target_ref,"case_ref":case,"expected_case_version":row.get::<i64,_>("version"),"expected_application_revision":row.get::<i64,_>("revision"),"material":{"display_name":row.get::<String,_>("name"),"description":"本地虚构资料，仅用于欢迎协作验收。","welcome_text":"欢迎来到社区，一起从容生活。"}})
        } else {
            let mut tx = store.pool.begin().await?;
            let rule = super::effective_knowledge_in(
                &mut tx,
                &store.tenant,
                r.scope,
                "resident_welcome",
                Some(case),
            )
            .await?;
            let Some(rule) = rule else {
                return Ok(welcome_response(
                    "请先说明本次或持续、直接发或先审，例如“本次欢迎先审，卡片和文案”。".into(),
                    json!({"status":"needs_setting"}),
                    Some(target),
                ));
            };
            json!({"action":"advance","target_ref":target_ref,"case_ref":case,"phase":rule.content["phase"],"expected_case_version":row.get::<i64,_>("version"),"expected_rule_ref":rule.id})
        }
    } else if let Some(code) = text
        .strip_prefix("批准卡片版本 ")
        .or_else(|| text.strip_prefix("批准文案版本 "))
    {
        let image = text.starts_with("批准卡片版本 ");
        let artifacts: Vec<_> = target["artifacts"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|a| {
                welcome_version_code(a) == code
                    && (a["kind"] == "welcome_card") == image
                    && a["case_ref"] == json!(case)
            })
            .collect();
        if artifacts.len() != 1 {
            return Ok(welcome_response(
                "该版本不在当前唯一事项的有效内容中，请重新查看欢迎内容版本，再明确批准所见版本。"
                    .into(),
                json!({"status":"needs_current_content_version"}),
                Some(target),
            ));
        }
        let artifact = artifacts[0];
        json!({"action":"approve","target_ref":target_ref,"artifact_ref":artifact["artifact_ref"],"target_version":artifact["target_version"],"content_hash":artifact["content_hash"],"phase":"formal","operation_id":r.operation_id})
    } else {
        return Ok(welcome_response("本地对话只识别列出的完整例句。请明确本次或持续、直接发或先审；审批必须指明回复中展示的卡片或文案版本。“同意”“都发吧”不会形成批准。".into(),json!({"status":"needs_explicit_welcome_instruction"}),Some(target)));
    };
    let fixed = store
        .foundation_turn_input(
            actor,
            r.operation_id,
            hash,
            &json!({"welcome_request":request}),
        )
        .await?;
    ensure!(
        fixed.get("welcome_request").is_some(),
        "idempotency_conflict"
    );
    let mut guard = store.pool.begin().await?;
    sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
        .bind(format!("welcome-turn/{}/{}", store.tenant, r.operation_id))
        .execute(&mut *guard)
        .await?;
    let current:Value=sqlx::query_scalar("SELECT command FROM qintopia_agent_os.collaboration_turn_sources WHERE tenant_key=$1 AND message_ref=$2").bind(&store.tenant).bind(r.operation_id).fetch_one(&mut *guard).await?;
    let replayed = current.get("welcome_result").is_some();
    let result = if let Some(result) = current.get("welcome_result") {
        result.clone()
    } else if current["welcome_started"] == true {
        json!({"status":"outcome_unknown","reason":"prior_attempt_requires_state_check"})
    } else {
        // Commit an attempt marker before invoking the consumer. A process crash
        // cannot turn an acknowledgement loss into a new render or send attempt.
        sqlx::query("UPDATE qintopia_agent_os.collaboration_turn_sources SET command=jsonb_set(command,'{welcome_started}','true') WHERE tenant_key=$1 AND message_ref=$2").bind(&store.tenant).bind(r.operation_id).execute(&store.pool).await?;
        let outcome = match welcome(store, actor, current["welcome_request"].clone()).await {
            Ok(v) => v,
            Err(e) => json!({"status":"blocked","reason":e.to_string()}),
        };
        sqlx::query("UPDATE qintopia_agent_os.collaboration_turn_sources SET command=jsonb_set(command,'{welcome_result}',$3) WHERE tenant_key=$1 AND message_ref=$2").bind(&store.tenant).bind(r.operation_id).bind(&outcome).execute(&store.pool).await?;
        outcome
    };
    guard.commit().await?;
    let latest = welcome_target_state(store, actor, r.scope).await?;
    let target = latest.iter().find(|t| t["target_ref"] == json!(target_ref));
    let action = fixed["welcome_request"]["action"].as_str().unwrap_or("");
    let reply = if replayed {
        "这是原请求的持久回执，未再次制卡、批准或发送。"
    } else if result["status"] == "outcome_unknown" {
        "上次处理结果尚未完成核对，未重试。请先查看本栋欢迎待办。"
    } else if result["status"] == "blocked" {
        "当前条件不足，已保留原因，未绕过授权或审批。"
    } else {
        match action {
            "configure" => "已保存本栋欢迎设置；具体内容审批和执行仍按当前授权判断。",
            "render" => {
                "已完成本地制卡和申请附件测试回读。请查看具体内容版本；尚未因此批准或发送。"
            }
            "approve" => "已记录你对这个具体版本的批准。其余部分仍独立判断，可继续推进本次欢迎。",
            _ => "已推进本次欢迎；只执行当前条件和授权均齐备的部分，等待项继续保留。",
        }
    };
    let reason = if result["status"] == "blocked" {
        format!(
            "原因：{}。",
            welcome_reason(result["reason"].as_str().unwrap_or(""))
        )
    } else {
        String::new()
    };
    Ok(welcome_response(
        format!(
            "{reply}{reason}{}",
            target.map(welcome_summary).unwrap_or_default()
        ),
        json!({"status":result["status"],"replayed":replayed,"result":result,"welcome":target}),
        target,
    ))
}

async fn welcome(store: &Store, actor: &Actor, request: Value) -> Result<Value> {
    let person = store.verified_person(actor).await?;
    let service = crate::resident_welcome::store::Store {
        pool: store.pool.clone(),
    };
    let action = request["action"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("invalid_command"))?;
    if action == "refresh_fixture" {
        // This is an explicitly synthetic source adapter, never a production freshness bypass.
        service.foundation_state(&store.tenant, person).await?;
        service.refresh_foundation_fixture(&store.tenant).await?;
        return Ok(json!({"refreshed":true,"source":"synthetic_fixture"}));
    }
    let parse = |key: &str| -> Result<Uuid> { Ok(serde_json::from_value(request[key].clone())?) };
    let target = if matches!(action, "execute" | "reconcile_unknown") {
        sqlx::query_scalar("SELECT target_id FROM qintopia_agent_os.welcome_actions WHERE id=$1")
            .bind(parse("action_ref")?)
            .fetch_one(&store.pool)
            .await?
    } else if action == "render" {
        let case = parse("case_ref")?;
        let rows:Vec<Uuid>=sqlx::query_scalar("SELECT f.scope_id FROM qintopia_agent_os.welcome_foundation_targets f JOIN qintopia_agent_os.welcome_targets t ON t.id=f.target_id JOIN qintopia_agent_os.welcome_cases c ON c.source_instance=t.source_instance AND c.property_id=t.property_id WHERE f.tenant_key=$1 AND c.id=$2")
            .bind(&store.tenant).bind(case).fetch_all(&store.pool).await?;
        let mut permitted = false;
        for scope in rows {
            if store.foundation_assert_scope(actor, scope).await.is_ok() {
                permitted = true;
                break;
            }
        }
        ensure!(permitted, "scope_access_denied");
        let material = serde_json::from_value(request["material"].clone())?;
        if let (Some(version), Some(revision)) = (
            request["expected_case_version"].as_i64(),
            request["expected_application_revision"].as_i64(),
        ) {
            return service
                .foundation_render_exact(case, &material, version, revision)
                .await;
        }
        return service.foundation_render(case, &material).await;
    } else {
        parse("target_ref")?
    };
    let scope:Uuid=sqlx::query_scalar("SELECT scope_id FROM qintopia_agent_os.welcome_foundation_targets WHERE tenant_key=$1 AND target_id=$2").bind(&store.tenant).bind(target).fetch_one(&store.pool).await?;
    store.foundation_assert_scope(actor, scope).await?;
    match action {
        "configure" => {
            let mut input = request.clone();
            input.as_object_mut().unwrap().remove("action");
            input.as_object_mut().unwrap().remove("target_ref");
            let input = if let Some(write) = input.get("write") {
                write.clone()
            } else {
                input
            };
            let write: KnowledgeWrite = serde_json::from_value(input)?;
            service
                .foundation_configure(&store.tenant, person, target, &write)
                .await
        }
        "prepare" | "advance" => {
            let prepared = if let (Some(version), Some(rule)) = (
                request["expected_case_version"].as_i64(),
                request["expected_rule_ref"].as_str(),
            ) {
                service
                    .foundation_prepare_exact(
                        parse("case_ref")?,
                        target,
                        request["phase"].as_str().unwrap_or("formal"),
                        version,
                        Uuid::parse_str(rule)?,
                    )
                    .await?
            } else {
                service
                    .foundation_prepare(
                        parse("case_ref")?,
                        target,
                        request["phase"].as_str().unwrap_or("formal"),
                    )
                    .await?
            };
            if action == "prepare" {
                return Ok(prepared);
            }
            let mut executed = vec![];
            for action in prepared["actions"].as_array().into_iter().flatten() {
                let action: Uuid = serde_json::from_value(action.clone())?;
                executed.push(
                    service
                        .foundation_execute(
                            action,
                            crate::resident_welcome::foundation::SyntheticOutcome::Success,
                        )
                        .await?,
                );
            }
            Ok(
                json!({"status":prepared["status"],"prepared":prepared,"executed":executed,"external_adapter":"synthetic"}),
            )
        }
        "approve" => {
            let r = crate::resident_welcome::review::ReviewRequest {
                operation: request
                    .get("operation_id")
                    .and_then(|v| v.as_str())
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .unwrap_or_else(Uuid::new_v4),
                artifact: parse("artifact_ref")?,
                target,
                phase: request["phase"].as_str().unwrap_or("formal").into(),
                target_version: request["target_version"]
                    .as_i64()
                    .ok_or_else(|| anyhow::anyhow!("invalid_version"))?,
                content_hash: request["content_hash"].as_str().unwrap_or("").into(),
            };
            service
                .foundation_approve_as(&store.tenant, person, &r, request["approval_kind"].as_str())
                .await
        }
        "execute" => {
            service
                .foundation_execute(
                    parse("action_ref")?,
                    crate::resident_welcome::foundation::SyntheticOutcome::Success,
                )
                .await
        }
        "reconcile_unknown" => {
            service
                .foundation_reconcile_unknown(parse("action_ref")?)
                .await
        }
        _ => anyhow::bail!("unknown_action"),
    }
}

pub(super) async fn card(store: &Store, actor: &Actor, artifact: Uuid) -> Result<Vec<u8>> {
    let rows=sqlx::query("SELECT f.scope_id,s.content FROM qintopia_agent_os.welcome_local_artifact_data s JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=s.artifact_id JOIN qintopia_agent_os.welcome_cases c ON c.id=b.case_id JOIN qintopia_agent_os.welcome_targets t ON t.source_instance=c.source_instance AND t.property_id=c.property_id JOIN qintopia_agent_os.welcome_foundation_targets f ON f.target_id=t.id JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id WHERE f.tenant_key=$1 AND s.artifact_id=$2 AND b.revoked_at IS NULL AND (b.target_id IS NULL OR b.target_id=t.id) AND (t.kind<>'building' OR v.projection->>'building'=t.building_code)")
        .bind(&store.tenant).bind(artifact).fetch_all(&store.pool).await?;
    for row in rows {
        if store
            .foundation_assert_scope(actor, row.get("scope_id"))
            .await
            .is_ok()
        {
            return Ok(row.get("content"));
        }
    }
    anyhow::bail!("scope_access_denied")
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct TrustedContext {
    platform: String,
    chat_type: String,
    chat_id: String,
    sender_id: String,
    message_id: String,
    gateway_id: String,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(super) struct ToolRequest {
    operation: String,
    schema_version: u32,
    agent: String,
    tool: String,
    trusted_context: TrustedContext,
    arguments: Value,
    token: String,
}

pub(super) fn parse_broker_request(raw: &[u8]) -> Result<ToolRequest> {
    let value = crate::strict_json::parse_strict_bounded_slice(
        raw,
        crate::strict_json::registry_json_limits(256 * 1024),
    )?;
    Ok(serde_json::from_value(value)?)
}

pub(super) async fn broker(store: Store) -> Result<()> {
    use std::os::unix::fs::{MetadataExt, PermissionsExt};
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let socket = std::env::var("QINTOPIA_FOUNDATION_SOCKET")?;
    let token = zeroize::Zeroizing::new(std::env::var("QINTOPIA_FOUNDATION_TOKEN")?);
    let gateway = std::env::var("QINTOPIA_FOUNDATION_GATEWAY_ID")?;
    let profile = std::env::var("QINTOPIA_FOUNDATION_PROFILE").unwrap_or_else(|_| "erhua".into());
    let path = std::path::Path::new(&socket);
    ensure!(
        path.is_absolute() && !path.exists() && token.len() >= 32,
        "private_foundation_socket_required"
    );
    let parent = path
        .parent()
        .ok_or_else(|| anyhow::anyhow!("invalid_socket_path"))?;
    let meta = std::fs::symlink_metadata(parent)?;
    ensure!(
        meta.is_dir() && !meta.file_type().is_symlink() && meta.mode() & 0o022 == 0,
        "private_foundation_socket_directory_required"
    );
    let listener = tokio::net::UnixListener::bind(path)?;
    std::fs::set_permissions(path, std::fs::Permissions::from_mode(0o600))?;
    let owner = std::fs::metadata(path)?.uid();
    loop {
        let (stream, _) = listener.accept().await?;
        if stream.peer_cred()?.uid() != owner {
            continue;
        }
        let (read, mut writer) = stream.into_split();
        let mut reader = BufReader::new(read);
        let mut raw = Vec::new();
        // Incremental bounded read; an unterminated hostile request cannot allocate without limit.
        let read = tokio::time::timeout(std::time::Duration::from_secs(5), async {
            loop {
                let chunk = reader.fill_buf().await?;
                if chunk.is_empty() {
                    anyhow::bail!("invalid_request");
                }
                let n = chunk
                    .iter()
                    .position(|b| *b == b'\n')
                    .map_or(chunk.len(), |i| i + 1);
                ensure!(raw.len() + n <= 256 * 1024, "request_too_large");
                raw.extend_from_slice(&chunk[..n]);
                reader.consume(n);
                if raw.ends_with(b"\n") {
                    break;
                }
            }
            Ok::<_, anyhow::Error>(())
        })
        .await;
        if !matches!(read, Ok(Ok(()))) {
            continue;
        }
        let parsed = parse_broker_request(&raw);
        use zeroize::Zeroize;
        raw.zeroize();
        let result = match parsed {
            Ok(request) => {
                if !super::digest(request.token.as_bytes()).eq(&super::digest(token.as_bytes())) {
                    Err(anyhow::anyhow!("authentication_required"))
                } else {
                    broker_invoke(&store, &gateway, &profile, request).await
                }
            }
            Err(_) => Err(anyhow::anyhow!("invalid_request")),
        };
        let response = match result {
            Ok(result) => json!({"ok":true,"result":result}),
            Err(e) => json!({"ok":false,"error":{"code":error_code(&e)}}),
        };
        let mut bytes = serde_json::to_vec(&response)?;
        bytes.push(b'\n');
        let _ = writer.write_all(&bytes).await;
    }
}

pub(super) async fn broker_invoke(
    store: &Store,
    gateway: &str,
    profile: &str,
    r: ToolRequest,
) -> Result<Value> {
    ensure!(
        r.operation == "person_foundation_tool" && r.schema_version == 1 && r.agent == profile,
        "agent_tool_denied"
    );
    let t = r.trusted_context;
    ensure!(
        t.gateway_id == gateway
            && t.platform == "qiwe"
            && matches!(t.chat_type.as_str(), "direct" | "group")
            && !t.message_id.is_empty()
            && t.message_id.len() <= 240
            && t.sender_id.len() <= 240,
        "trusted_context_unavailable"
    );
    let actor = store.gateway_actor(gateway, &t.sender_id).await?;
    let scope = store.gateway_scope(&actor).await?;
    if t.chat_type == "group" {
        let bound:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id WHERE b.tenant_key=$1 AND b.scope_id=$2 AND b.revoked_at IS NULL AND c.chat_id=$3 AND c.status='active')")
            .bind(&store.tenant).bind(scope).bind(&t.chat_id).fetch_one(&store.pool).await?;
        ensure!(bound, "gateway_scope_mismatch");
    }
    let source=sqlx::query("SELECT m.id,m.sent_at FROM qintopia_messages.messages m JOIN qintopia_messages.raw_events r ON r.id=m.raw_event_id WHERE m.tenant_id=$1 AND m.platform=$2 AND m.chat_id=$3 AND m.sender_id=$4 AND m.message_id=$5 AND m.chat_type=$6 AND m.sent_at IS NOT NULL AND r.ingress_auth_verified AND r.subject='qintopia.qiwe.raw.authenticated'")
        .bind(&store.tenant).bind(&t.platform).bind(&t.chat_id).bind(&t.sender_id).bind(&t.message_id).bind(&t.chat_type).fetch_optional(&store.pool).await?.ok_or_else(||anyhow::anyhow!("trusted_message_evidence_required"))?;
    let a = r.arguments;
    ensure!(a.is_object(), "invalid_arguments");
    match r.tool.as_str() {
        "context" => {
            only_keys(&a, &["purpose", "topic"])?;
            ensure!(
                a["purpose"].as_str().unwrap_or("reply") == "reply",
                "invalid_purpose"
            );
            let topic = a["topic"].as_str().unwrap_or("general");
            ensure!(matches!(topic, "general" | "fees"), "invalid_topic");
            let mut result = match store.foundation_context(&actor, scope, topic).await {
                Ok(context) => context,
                Err(error) if error.to_string() == "scope_access_denied" => json!({
                    "identity":store.identity_context(&actor).await?,
                    "memory":store.memory_context(&actor,topic).await?,
                    "knowledge":[],"permissions":[],"purpose":"personal_reply"
                }),
                Err(error) => return Err(error),
            };
            if t.chat_type == "group" {
                result["identity"] = json!({"identity_status":"confirmed"});
                result["memory"] = json!({"reply_style":result["memory"]["reply_style"],"purpose":"reply_style_only","disclosure_allowed":false});
            }
            Ok(result)
        }
        "history" => {
            ensure!(profile == "erhua", "agent_tool_denied");
            only_keys(&a, &["purpose"])?;
            ensure!(
                t.chat_type == "direct"
                    && a["purpose"].as_str().unwrap_or("self_history") == "self_history",
                "private_history_only"
            );
            store.person_history(&actor).await
        }
        "remember" => {
            ensure!(profile == "erhua", "agent_tool_denied");
            let command = serde_json::from_value(a)?;
            let evidence =
                super::store::MemoryEvidence::trusted(source.get("id"), source.get("sent_at"));
            store.remember(&actor, &command, &evidence).await
        }
        "save_rule" => {
            ensure!(profile == "erhua", "agent_tool_denied");
            only_keys(
                &a,
                &[
                    "operation_id",
                    "expected_version",
                    "key",
                    "content",
                    "kind",
                    "shared",
                    "effective_at",
                    "effective_until",
                    "case_ref",
                ],
            )?;
            let mut a = a;
            a["scope"] = json!(scope);
            a["kind"] = json!(a["kind"].as_str().unwrap_or("rule"));
            if a["content"].is_string() {
                a["content"] = json!({"text":a["content"]});
            }
            store
                .knowledge_save(&actor, &serde_json::from_value(a)?, false)
                .await
        }
        "dispatch" => {
            ensure!(
                matches!(profile, "default" | "silaoshi"),
                "agent_tool_denied"
            );
            only_keys(
                &a,
                &["operation_id", "expected_version", "capability", "brief"],
            )?;
            ensure!(
                a["capability"] == "erhua.foundation_context",
                "unknown_capability"
            );
            store
                .dispatch_context(
                    &actor,
                    scope,
                    serde_json::from_value(a["operation_id"].clone())?,
                    a["expected_version"]
                        .as_i64()
                        .ok_or_else(|| anyhow::anyhow!("invalid_version"))?,
                    a["brief"].as_str().unwrap_or(""),
                )
                .await
        }
        "task_status" => {
            only_keys(&a, &["work_item_id"])?;
            store
                .foundation_work_status(&actor, serde_json::from_value(a["work_item_id"].clone())?)
                .await
        }
        _ => anyhow::bail!("unknown_tool"),
    }
}
fn only_keys(value: &Value, keys: &[&str]) -> Result<()> {
    ensure!(
        value
            .as_object()
            .is_some_and(|o| o.keys().all(|k| keys.contains(&k.as_str()))),
        "invalid_arguments"
    );
    Ok(())
}

pub(super) fn error_code(error: &anyhow::Error) -> String {
    let value = error.to_string();
    if !value.is_empty()
        && value.len() <= 100
        && value
            .bytes()
            .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == b'_')
    {
        value
    } else {
        "foundation_operation_failed".into()
    }
}
