use std::{
    fs,
    os::unix::fs::{FileTypeExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    time::timeout,
};
use uuid::Uuid;

use crate::{
    config::Cli,
    conversation_ingress::{self, SignedIngressEnvelope},
    conversation_policy::{POSTER_PRODUCTION_CAPABILITY, POSTER_STATUS_CAPABILITY},
    db,
    operations::{self, OperationsPolicy, WorkItemCreateRequest, WorkflowStartRequest},
};

const LEGACY_PROTOCOL_VERSION: u8 = 2;
const PROTOCOL_VERSION: u8 = 3;
const MAX_MESSAGE_BYTES: u64 = 64 * 1024;
const READ_TIMEOUT: Duration = Duration::from_millis(750);
const HANDLE_TIMEOUT: Duration = Duration::from_millis(3_500);
const WRITE_TIMEOUT: Duration = Duration::from_millis(500);

#[derive(Debug, Clone, Deserialize)]
struct TrustedSession {
    platform: String,
    #[serde(default)]
    conversation_type: String,
    conversation_id: String,
    requester_user_id: String,
    source_message_id: String,
    #[serde(skip)]
    policy_version: i64,
}

#[derive(Debug, Clone, Default, Deserialize)]
struct ActivityFacts {
    #[serde(default)]
    source: String,
    #[serde(default)]
    title: String,
    #[serde(default)]
    schedule: String,
    #[serde(default)]
    location: String,
    #[serde(default)]
    conflict_fields: Vec<String>,
}

#[derive(Debug)]
struct FactAssessment {
    status: &'static str,
    source: &'static str,
    title: String,
    schedule: String,
    location: String,
    missing_fields: Vec<&'static str>,
    conflict_fields: Vec<String>,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum IntakeRequest {
    PosterProductionRequest {
        schema_version: u8,
        request: String,
        #[serde(default)]
        priority: String,
        #[serde(default)]
        activity_record_ref: String,
        #[serde(default)]
        activity_facts: ActivityFacts,
        session: TrustedSession,
        #[serde(default)]
        idempotency_key: String,
    },
    PosterRevisionRequest {
        schema_version: u8,
        request: String,
        workflow_root_id: Uuid,
        revision_of_artifact_id: Uuid,
        session: TrustedSession,
        #[serde(default)]
        idempotency_key: String,
    },
    WorkflowStatus {
        schema_version: u8,
        workflow_root_id: Uuid,
        session: TrustedSession,
    },
}

#[derive(Debug)]
enum WireRequest {
    Legacy(Box<IntakeRequest>),
    FeishuMessage(SignedIngressEnvelope),
}

#[derive(Debug, Serialize)]
struct ErrorResponse<'a> {
    success: bool,
    accepted: bool,
    error: &'a str,
    message: &'a str,
    external_send_executed: bool,
}

struct SocketGuard(PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        if path_is_socket(&self.0) {
            let _ = fs::remove_file(&self.0);
        }
    }
}

pub async fn run(cli: &Cli, socket_path: PathBuf) -> Result<()> {
    prepare_socket(&socket_path)?;
    let ingress_config =
        conversation_ingress::IngressConfig::from_env_optional()?.map(std::sync::Arc::new);
    let pool = db::connect(cli.database_url_required()?, cli.db_max_connections).await?;
    let policy = OperationsPolicy::from_cli(cli, true);
    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("bind operations intake socket {}", socket_path.display()))?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o600))
        .context("set operations intake socket permissions")?;
    let _guard = SocketGuard(socket_path.clone());
    tracing::info!(socket_path = %socket_path.display(), "operations intake started");

    loop {
        let (stream, _) = listener
            .accept()
            .await
            .context("accept operations intake")?;
        let pool = pool.clone();
        let policy = policy.clone();
        let ingress_config = ingress_config.clone();
        tokio::spawn(async move {
            if handle_connection(stream, &pool, &policy, ingress_config.as_deref())
                .await
                .is_err()
            {
                tracing::warn!(
                    error_code = "intake_connection_failed",
                    "operations intake request failed"
                );
            }
        });
    }
}

async fn handle_connection(
    mut stream: UnixStream,
    pool: &PgPool,
    policy: &OperationsPolicy,
    ingress_config: Option<&conversation_ingress::IngressConfig>,
) -> Result<()> {
    let request = match read_request(&mut stream).await {
        Ok(request) => request,
        Err(_) => {
            return write_response(
                &mut stream,
                &ErrorResponse {
                    success: false,
                    accepted: false,
                    error: "invalid_request",
                    message: "poster intake request is invalid",
                    external_send_executed: false,
                },
            )
            .await;
        }
    };
    let future = async {
        match request {
            WireRequest::Legacy(request) => handle_request(pool, policy, *request).await,
            WireRequest::FeishuMessage(envelope) => {
                conversation_ingress::handle(pool, ingress_config, envelope).await
            }
        }
    };
    let response = match timeout(HANDLE_TIMEOUT, future).await {
        Ok(Ok(value)) => value,
        Ok(Err(_)) => json!({
            "success": false,
            "accepted": false,
            "error": "agentos_intake_rejected",
            "message": "AgentOS rejected the poster request",
            "external_send_executed": false
        }),
        Err(_) => json!({
            "success": false,
            "accepted": false,
            "error": "agentos_intake_timeout",
            "message": "AgentOS did not accept the poster request in time",
            "external_send_executed": false
        }),
    };
    write_response(&mut stream, &response).await
}

async fn read_request(stream: &mut UnixStream) -> Result<WireRequest> {
    let mut bytes = Vec::new();
    let mut reader = BufReader::new(stream.take(MAX_MESSAGE_BYTES + 1));
    let count = timeout(READ_TIMEOUT, reader.read_until(b'\n', &mut bytes))
        .await
        .context("intake read timed out")??;
    if count == 0 || bytes.len() as u64 > MAX_MESSAGE_BYTES {
        bail!("intake request length is invalid");
    }
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    let value: Value = serde_json::from_slice(&bytes).context("parse intake request")?;
    if value.get("body_base64").is_some() {
        return serde_json::from_value(value)
            .map(WireRequest::FeishuMessage)
            .context("parse signed Feishu message ingress envelope");
    }
    serde_json::from_value(value)
        .map(Box::new)
        .map(WireRequest::Legacy)
        .context("parse legacy operations intake request")
}

async fn write_response(stream: &mut UnixStream, response: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec(response).context("serialize intake response")?;
    bytes.push(b'\n');
    timeout(WRITE_TIMEOUT, stream.write_all(&bytes))
        .await
        .context("intake write timed out")??;
    Ok(())
}

async fn handle_request(
    pool: &PgPool,
    policy: &OperationsPolicy,
    request: IntakeRequest,
) -> Result<Value> {
    match request {
        IntakeRequest::PosterProductionRequest {
            schema_version,
            request,
            priority,
            activity_record_ref,
            activity_facts,
            session,
            idempotency_key,
        } => {
            validate_protocol(schema_version)?;
            let session =
                resolve_session(pool, schema_version, session, POSTER_PRODUCTION_CAPABILITY)
                    .await?;
            let request = validate_text("request", request, 2_000)?;
            let priority = normalize_priority(priority)?;
            let activity_record_ref = validate_optional_ref(activity_record_ref)?;
            let expected_key = source_idempotency_key(&session);
            if !idempotency_key.is_empty() && idempotency_key != expected_key {
                bail!("idempotency key does not match trusted source message");
            }
            let origin_ref = origin_ref(&session);
            let actor_ref = actor_ref(&session);
            let source_message_ref = source_message_ref(&session);
            let fact_assessment =
                assess_activity_facts(pool, &session, &activity_record_ref, activity_facts).await?;
            let workflow_summary = fact_assessment.workflow_summary(&request);
            upsert_return_target(pool, &origin_ref, &session).await?;
            let source_refs = if activity_record_ref.is_empty() {
                json!({"source_message_ref": source_message_ref})
            } else {
                json!({
                    "source_message_ref": source_message_ref,
                    "activity_record_ref": activity_record_ref
                })
            };
            let report = operations::start_workflow(
                pool,
                WorkflowStartRequest {
                    actor_agent: "xiaoman".to_string(),
                    workflow_type: "activity_promotion".to_string(),
                    request_text: workflow_summary,
                    source_type: "feishu_direct_request".to_string(),
                    source_refs,
                    human_owner: actor_ref.clone(),
                    priority,
                    idempotency_key: expected_key,
                    metadata: json!({
                        "intake_channel": "xiaoman_feishu_direct",
                        "origin_conversation_ref": origin_ref,
                        "poster_fact_gate": fact_assessment.metadata(),
                        "generation_authorization": {
                            "mode": "originating_generation_request",
                            "actor_ref": actor_ref,
                            "source_message_ref": source_message_ref,
                            "group_send_authorized": false
                        }
                    }),
                },
                true,
                policy,
            )
            .await?;
            let workflow_root_id = report
                .parent_work_item
                .work_item_id
                .context("workflow root id is missing")?;
            let visual_work_item_id = report
                .child_work_items
                .iter()
                .find(|item| item.work_item_type == "visual_asset_request")
                .and_then(|item| item.work_item_id)
                .context("visual work item id is missing")?;
            record_generation_authorization(
                pool,
                workflow_root_id,
                &actor_ref,
                &source_message_ref,
            )
            .await?;
            record_fact_assessment(pool, workflow_root_id, &fact_assessment).await?;
            let status = operations::work_item_status_tree(pool, workflow_root_id).await?;
            let needs_clarification = fact_assessment.status == "needs_clarification";
            Ok(json!({
                "success": true,
                "accepted": true,
                "deduped": report.action_status == "idempotent_existing",
                "workflow_root_id": workflow_root_id,
                "visual_work_item_id": visual_work_item_id,
                "workflow_status": status.root.status,
                "current_blocking_point": status.current_blocking_point,
                "user_status": if needs_clarification { "需补充" } else { "已接单" },
                "missing_fields": fact_assessment.missing_fields,
                "conflict_fields": fact_assessment.conflict_fields,
                "external_send_executed": false
            }))
        }
        IntakeRequest::WorkflowStatus {
            schema_version,
            workflow_root_id,
            session,
        } => {
            validate_protocol(schema_version)?;
            let session =
                resolve_session(pool, schema_version, session, POSTER_STATUS_CAPABILITY).await?;
            authorize_status_read(pool, workflow_root_id, &session).await?;
            let mut value = serde_json::to_value(
                operations::work_item_status_tree(pool, workflow_root_id).await?,
            )
            .context("serialize workflow status")?;
            value["user_status"] = json!(poster_user_status(pool, workflow_root_id).await?);
            Ok(value)
        }
        IntakeRequest::PosterRevisionRequest {
            schema_version,
            request,
            workflow_root_id,
            revision_of_artifact_id,
            session,
            idempotency_key,
        } => {
            validate_protocol(schema_version)?;
            let session =
                resolve_session(pool, schema_version, session, POSTER_PRODUCTION_CAPABILITY)
                    .await?;
            let instruction = validate_text("request", request, 2_000)?;
            let expected_key = source_idempotency_key(&session);
            if !idempotency_key.is_empty() && idempotency_key != expected_key {
                bail!("idempotency key does not match trusted source message");
            }
            create_revision_request(
                pool,
                policy,
                &session,
                workflow_root_id,
                revision_of_artifact_id,
                instruction,
                expected_key,
            )
            .await
        }
    }
}

async fn poster_user_status(pool: &PgPool, workflow_root_id: Uuid) -> Result<&'static str> {
    let row = sqlx::query(
        r#"
        SELECT
            EXISTS (
                SELECT 1 FROM qintopia_agent_os.work_item_events event
                WHERE event.work_item_id = $1
                  AND event.event_type = 'poster_facts_need_clarification'
            ) AS facts_need_clarification,
            EXISTS (
                SELECT 1 FROM qintopia_agent_os.poster_review_actions action
                JOIN qintopia_agent_os.poster_notifications notification
                  ON notification.id = action.notification_id
                WHERE notification.workflow_root_id = $1 AND action.decision = 'rejected'
            ) AS abandoned,
            EXISTS (
                SELECT 1 FROM qintopia_agent_os.poster_review_actions action
                JOIN qintopia_agent_os.poster_notifications notification
                  ON notification.id = action.notification_id
                WHERE notification.workflow_root_id = $1 AND action.decision = 'approved'
            ) AS approved,
            EXISTS (
                SELECT 1 FROM qintopia_agent_os.artifacts artifact
                JOIN qintopia_agent_os.work_items item ON item.id = artifact.work_item_id
                JOIN qintopia_agent_os.work_items visual ON visual.id = item.parent_work_item_id
                WHERE visual.parent_work_item_id = $1
                  AND artifact.artifact_type = 'generated_image'
                  AND artifact.review_status = 'changes_requested'
            ) AS needs_clarification,
            EXISTS (
                SELECT 1 FROM qintopia_agent_os.poster_notifications notification
                WHERE notification.workflow_root_id = $1 AND notification.status = 'delivered'
            ) AS awaiting_review,
            EXISTS (
                WITH RECURSIVE descendants AS (
                    SELECT id, status FROM qintopia_agent_os.work_items WHERE id = $1
                    UNION ALL
                    SELECT child.id, child.status FROM qintopia_agent_os.work_items child
                    JOIN descendants parent ON child.parent_work_item_id = parent.id
                ) SELECT 1 FROM descendants WHERE status = 'failed'
            ) AS failed,
            EXISTS (
                SELECT 1 FROM qintopia_agent_os.work_items image_request
                JOIN qintopia_agent_os.work_items visual ON visual.id = image_request.parent_work_item_id
                WHERE visual.parent_work_item_id = $1
                  AND image_request.work_item_type = 'image_generation_request'
                  AND image_request.status IN ('queued', 'processing')
            ) AS generating
        "#,
    )
    .bind(workflow_root_id)
    .fetch_one(pool)
    .await
    .context("derive poster user status")?;
    Ok(if row.try_get::<bool, _>("abandoned")? {
        "已放弃"
    } else if row.try_get::<bool, _>("approved")? {
        "已通过"
    } else if row.try_get::<bool, _>("facts_need_clarification")?
        || row.try_get::<bool, _>("needs_clarification")?
    {
        "需补充"
    } else if row.try_get::<bool, _>("awaiting_review")? {
        "待你审稿"
    } else if row.try_get::<bool, _>("failed")? {
        "生成失败"
    } else if row.try_get::<bool, _>("generating")? {
        "生成中"
    } else {
        "已接单"
    })
}

impl FactAssessment {
    fn metadata(&self) -> Value {
        json!({
            "status": self.status,
            "source": self.source,
            "missing_fields": self.missing_fields,
            "conflict_fields": self.conflict_fields,
        })
    }

    fn workflow_summary(&self, original_request: &str) -> String {
        if self.status == "complete" {
            return format!(
                "{}｜{}｜{}｜按原始指令生成活动海报",
                self.title, self.schedule, self.location
            )
            .chars()
            .take(500)
            .collect();
        }
        let request_ref = digest(&["poster-request-v1", original_request]);
        format!("海报生成请求待补充活动事实（{request_ref}）")
    }
}

async fn assess_activity_facts(
    pool: &PgPool,
    session: &TrustedSession,
    activity_record_ref: &str,
    facts: ActivityFacts,
) -> Result<FactAssessment> {
    let source = match facts.source.as_str() {
        "originating_request" => "originating_request",
        "trusted_activity_record" => "trusted_activity_record",
        "" => "unavailable",
        _ => bail!("activity fact source is invalid"),
    };
    let title = validate_optional_fact("title", facts.title, 200)?;
    let schedule = validate_optional_fact("schedule", facts.schedule, 200)?;
    let location = validate_optional_fact("location", facts.location, 240)?;
    let conflict_fields = validate_conflict_fields(facts.conflict_fields)?;
    let source_text = match source {
        "originating_request" => load_originating_message(pool, session).await?,
        "trusted_activity_record" => {
            load_trusted_activity_record_summary(pool, activity_record_ref).await?
        }
        _ => None,
    };
    let source_text = source_text.unwrap_or_default();
    let mut missing_fields = Vec::new();
    for (field, value) in [
        ("活动标题", title.as_str()),
        ("活动时间", schedule.as_str()),
        ("活动地点", location.as_str()),
    ] {
        if value.is_empty() || source_text.is_empty() || !source_text.contains(value) {
            missing_fields.push(field);
        }
    }
    if source == "trusted_activity_record" && activity_record_ref.is_empty() {
        missing_fields.push("可信活动记录");
    }
    missing_fields.sort_unstable();
    missing_fields.dedup();
    Ok(FactAssessment {
        status: if missing_fields.is_empty() && conflict_fields.is_empty() {
            "complete"
        } else {
            "needs_clarification"
        },
        source,
        title,
        schedule,
        location,
        missing_fields,
        conflict_fields,
    })
}

async fn load_originating_message(
    pool: &PgPool,
    session: &TrustedSession,
) -> Result<Option<String>> {
    sqlx::query_scalar(
        r#"
        SELECT text
        FROM qintopia_messages.messages
        WHERE platform = 'feishu'
          AND message_id = $1
          AND chat_id = $2
          AND chat_type IN ('direct', 'p2p', 'private')
          AND sender_id = $3
          AND NULLIF(btrim(text), '') IS NOT NULL
        LIMIT 1
        "#,
    )
    .bind(&session.source_message_id)
    .bind(&session.conversation_id)
    .bind(&session.requester_user_id)
    .fetch_optional(pool)
    .await
    .context("load trusted originating Feishu message")
}

async fn load_trusted_activity_record_summary(
    pool: &PgPool,
    activity_record_ref: &str,
) -> Result<Option<String>> {
    if !valid_activity_record_ref(activity_record_ref) {
        return Ok(None);
    }
    sqlx::query_scalar(
        r#"
        SELECT brief_summary
        FROM qintopia_agent_os.work_items
        WHERE source_type = 'xiaoman_activity'
          AND source_refs->>'source_record_ref' = $1
          AND requester_agent = 'xiaoman'
          AND capability_key = 'huabaosi.create_visual_asset'
        ORDER BY created_at DESC
        LIMIT 1
        "#,
    )
    .bind(activity_record_ref)
    .fetch_optional(pool)
    .await
    .context("load trusted AgentOS activity record summary")
}

async fn record_fact_assessment(
    pool: &PgPool,
    workflow_root_id: Uuid,
    assessment: &FactAssessment,
) -> Result<()> {
    let event_type = if assessment.status == "complete" {
        "poster_facts_validated"
    } else {
        "poster_facts_need_clarification"
    };
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, event_type, actor_type, actor_id, message, data)
        SELECT $1, $2, 'system', 'operations-intake',
               'poster activity facts evaluated before worker execution', $3
        WHERE NOT EXISTS (
            SELECT 1 FROM qintopia_agent_os.work_item_events
            WHERE work_item_id = $1 AND event_type = $2
        )
        "#,
    )
    .bind(workflow_root_id)
    .bind(event_type)
    .bind(assessment.metadata())
    .execute(pool)
    .await
    .context("record poster fact assessment")?;
    Ok(())
}

fn validate_optional_fact(name: &str, value: String, max_chars: usize) -> Result<String> {
    let value = value.trim().to_string();
    if value.chars().count() > max_chars || value.contains('\0') {
        bail!("activity fact {name} is invalid");
    }
    Ok(value)
}

fn validate_conflict_fields(fields: Vec<String>) -> Result<Vec<String>> {
    let mut validated = Vec::new();
    for field in fields {
        let field = field.trim().to_string();
        if !matches!(field.as_str(), "title" | "schedule" | "location") {
            bail!("activity fact conflict field is invalid");
        }
        if !validated.contains(&field) {
            validated.push(field);
        }
    }
    Ok(validated)
}

fn valid_activity_record_ref(value: &str) -> bool {
    let Some((role, digest)) = value.split_once(':') else {
        return false;
    };
    matches!(role, "activity_plan" | "activity_occurrence")
        && digest.len() == 12
        && digest.bytes().all(|byte| byte.is_ascii_hexdigit())
}

async fn create_revision_request(
    pool: &PgPool,
    policy: &OperationsPolicy,
    session: &TrustedSession,
    workflow_root_id: Uuid,
    revision_of_artifact_id: Uuid,
    instruction: String,
    idempotency_key: String,
) -> Result<Value> {
    authorize_status_read(pool, workflow_root_id, session).await?;
    let row = sqlx::query(
        r#"
        SELECT visual.id AS visual_work_item_id,
               brief.id AS approved_brief_artifact_id,
               brief.content_hash AS approved_brief_content_hash,
               root.human_owner,
               root.priority,
               root.source_event_signal_id
        FROM qintopia_agent_os.work_items root
        JOIN qintopia_agent_os.work_items visual
          ON visual.parent_work_item_id = root.id
         AND visual.work_item_type = 'visual_asset_request'
        JOIN qintopia_agent_os.artifacts brief
          ON brief.work_item_id = visual.id
         AND brief.artifact_type = 'poster_brief'
         AND brief.review_status = 'approved'
        JOIN qintopia_agent_os.artifacts current_image
          ON current_image.id = $2
         AND current_image.artifact_type = 'generated_image'
         AND current_image.review_status = 'changes_requested'
        JOIN qintopia_agent_os.work_items current_request
          ON current_request.id = current_image.work_item_id
         AND current_request.parent_work_item_id = visual.id
         AND current_request.work_item_type = 'image_generation_request'
        JOIN qintopia_agent_os.poster_notifications notification
          ON notification.generated_image_artifact_id = current_image.id
         AND notification.workflow_root_id = root.id
         AND notification.status = 'delivered'
        WHERE root.id = $1
          AND root.metadata #>> '{workflow_metadata,intake_channel}' = 'xiaoman_feishu_direct'
        ORDER BY brief.updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(workflow_root_id)
    .bind(revision_of_artifact_id)
    .fetch_optional(pool)
    .await
    .context("load trusted poster revision context")?
    .context("poster revision does not match a delivered changes-requested image")?;
    let visual_work_item_id: Uuid = row.try_get("visual_work_item_id")?;
    let brief_id: Uuid = row.try_get("approved_brief_artifact_id")?;
    let brief_hash: String = row.try_get("approved_brief_content_hash")?;
    let human_owner: String = row.try_get("human_owner")?;
    let actor_ref = actor_ref(session);
    if human_owner != actor_ref {
        bail!("poster revision actor does not match the originating requester");
    }
    let message_ref = source_message_ref(session);
    let revision_hash = digest(&[
        "poster-revision-v1",
        &revision_of_artifact_id.to_string(),
        &instruction,
        &message_ref,
    ]);
    let prompt_hash = digest(&[&brief_hash, &revision_hash]);
    let report = operations::create_work_item(
        pool,
        WorkItemCreateRequest {
            requester_agent: "xiaoman".to_string(),
            target_agent: "huabaosi".to_string(),
            capability_key: "huabaosi.generate_image_asset".to_string(),
            work_item_type: "image_generation_request".to_string(),
            brief_summary: "根据原发起人的明确修改意见生成下一版活动海报".to_string(),
            purpose: "activity_image_revision_request".to_string(),
            human_owner: human_owner.clone(),
            priority: row.try_get("priority")?,
            source_type: "feishu_direct_revision_request".to_string(),
            source_refs: json!({
                "source_message_ref": message_ref,
                "revision_of_artifact_id": revision_of_artifact_id
            }),
            source_event_signal_id: row.try_get("source_event_signal_id")?,
            payload: json!({
                "workflow_type": "activity_promotion",
                "activity_phase": "pre_event",
                "activity_route": "promotion",
                "planner_intent": "revise_image_from_originating_user_instruction",
                "approved_brief_artifact_id": brief_id,
                "approved_brief_content_hash": brief_hash,
                "image_specification": "community_poster_1024x1024",
                "prompt_hash": prompt_hash,
                "revision_of_artifact_id": revision_of_artifact_id,
                "revision_instruction": instruction,
                "revision_instruction_hash": revision_hash,
                "external_publish_executed": false,
                "group_send_authorized": false
            }),
            payload_redaction_policy: "summary_only".to_string(),
            idempotency_key,
            dedupe_key: String::new(),
            metadata: json!({
                "workflow_type": "activity_promotion",
                "workflow_step": "image_revision",
                "workflow_root_id": workflow_root_id,
                "visual_work_item_id": visual_work_item_id,
                "revision_of_artifact_id": revision_of_artifact_id,
                "revision_instruction_hash": revision_hash,
                "group_send_authorized": false,
                "external_publish_executed": false
            }),
            parent_work_item_id: Some(visual_work_item_id),
            approved_artifact_id: Some(brief_id),
        },
        true,
        policy,
    )
    .await?;
    let image_generation_work_item_id = report
        .work_item_id
        .context("poster revision image work item id is missing")?;
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.poster_revision_requests
            (workflow_root_id, source_artifact_id, source_message_ref, actor_ref,
             instruction_text, image_generation_work_item_id, status)
        VALUES ($1, $2, $3, $4, $5, $6, 'queued')
        ON CONFLICT (source_message_ref) DO NOTHING
        "#,
    )
    .bind(workflow_root_id)
    .bind(revision_of_artifact_id)
    .bind(&message_ref)
    .bind(&actor_ref)
    .bind(&instruction)
    .bind(image_generation_work_item_id)
    .execute(pool)
    .await
    .context("record poster revision request")?;
    record_generation_authorization(pool, workflow_root_id, &actor_ref, &message_ref).await?;
    Ok(json!({
        "success": true,
        "accepted": true,
        "deduped": report.existing,
        "workflow_root_id": workflow_root_id,
        "visual_work_item_id": visual_work_item_id,
        "image_generation_work_item_id": image_generation_work_item_id,
        "workflow_status": "生成中",
        "external_send_executed": false,
        "group_send_authorized": false
    }))
}

async fn upsert_return_target(
    pool: &PgPool,
    origin_ref: &str,
    session: &TrustedSession,
) -> Result<()> {
    let conversation_ref = if session.policy_version == 0 {
        origin_ref.to_string()
    } else {
        crate::conversation_policy::conversation_ref(&session.platform, &session.conversation_id)
    };
    let result = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.poster_return_targets
            (origin_ref, platform, conversation_type, conversation_id,
             requester_user_id, source_message_id, audience_class,
             conversation_ref, policy_version, delivery_mode, thread_root_message_id)
        VALUES ($1, $2, $3, $4, $5, $6, 'private', $7, $8, 'direct_chat', NULL)
        ON CONFLICT (origin_ref) DO UPDATE SET
            source_message_id = EXCLUDED.source_message_id,
            updated_at = now()
        WHERE qintopia_agent_os.poster_return_targets.platform = EXCLUDED.platform
          AND qintopia_agent_os.poster_return_targets.conversation_type = EXCLUDED.conversation_type
          AND qintopia_agent_os.poster_return_targets.conversation_id = EXCLUDED.conversation_id
          AND qintopia_agent_os.poster_return_targets.requester_user_id = EXCLUDED.requester_user_id
          AND qintopia_agent_os.poster_return_targets.audience_class = EXCLUDED.audience_class
          AND qintopia_agent_os.poster_return_targets.conversation_ref = EXCLUDED.conversation_ref
          AND qintopia_agent_os.poster_return_targets.policy_version = EXCLUDED.policy_version
          AND qintopia_agent_os.poster_return_targets.delivery_mode = EXCLUDED.delivery_mode
        "#,
    )
    .bind(origin_ref)
    .bind(&session.platform)
    .bind(&session.conversation_type)
    .bind(&session.conversation_id)
    .bind(&session.requester_user_id)
    .bind(&session.source_message_id)
    .bind(conversation_ref)
    .bind(session.policy_version)
    .execute(pool)
    .await
    .context("store trusted poster return target")?;
    if result.rows_affected() != 1 {
        bail!("trusted poster return target conflicts with its existing binding");
    }
    Ok(())
}

async fn record_generation_authorization(
    pool: &PgPool,
    workflow_root_id: Uuid,
    actor_ref: &str,
    source_message_ref: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, event_type, actor_type, actor_id, message, data)
        SELECT $1, 'poster_generation_authorized', 'human', $2,
               'originating direct-chat request authorized poster generation',
               jsonb_build_object(
                   'authorization_mode', 'originating_generation_request',
                   'source_message_ref', $3,
                   'group_send_authorized', false
               )
        WHERE NOT EXISTS (
            SELECT 1
            FROM qintopia_agent_os.work_item_events
            WHERE work_item_id = $1
              AND event_type = 'poster_generation_authorized'
              AND data->>'source_message_ref' = $3
        )
        "#,
    )
    .bind(workflow_root_id)
    .bind(actor_ref)
    .bind(source_message_ref)
    .execute(pool)
    .await
    .context("record poster generation authorization")?;
    Ok(())
}

async fn authorize_status_read(
    pool: &PgPool,
    workflow_root_id: Uuid,
    session: &TrustedSession,
) -> Result<()> {
    let allowed: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM qintopia_agent_os.work_items root
            JOIN qintopia_agent_os.poster_return_targets target
              ON target.origin_ref = root.metadata #>> '{workflow_metadata,origin_conversation_ref}'
            WHERE root.id = $1
              AND root.parent_work_item_id IS NULL
              AND target.platform = $2
              AND target.conversation_type = 'direct'
              AND target.conversation_id = $3
              AND target.requester_user_id = $4
        )
        "#,
    )
    .bind(workflow_root_id)
    .bind(&session.platform)
    .bind(&session.conversation_id)
    .bind(&session.requester_user_id)
    .fetch_one(pool)
    .await
    .context("authorize poster workflow status read")?;
    if !allowed {
        bail!("workflow does not belong to the current direct conversation");
    }
    Ok(())
}

fn validate_protocol(version: u8) -> Result<()> {
    if !matches!(version, LEGACY_PROTOCOL_VERSION | PROTOCOL_VERSION) {
        bail!("unsupported intake protocol version");
    }
    Ok(())
}

async fn resolve_session(
    pool: &PgPool,
    schema_version: u8,
    mut session: TrustedSession,
    required_capability: &str,
) -> Result<TrustedSession> {
    if !matches!(
        required_capability,
        POSTER_PRODUCTION_CAPABILITY | POSTER_STATUS_CAPABILITY
    ) {
        bail!("V3 poster intake capability is invalid");
    }
    validate_session_identity(&session)?;
    if schema_version == LEGACY_PROTOCOL_VERSION {
        validate_session(&session)?;
        session.policy_version = 0;
        return Ok(session);
    }
    if !session.conversation_type.is_empty() {
        bail!("V3 poster intake does not accept caller-provided conversation type");
    }
    let expected_message_ref = crate::conversation_policy::source_message_ref(
        &session.platform,
        &session.source_message_id,
    );
    let expected_conversation_ref =
        crate::conversation_policy::conversation_ref(&session.platform, &session.conversation_id);
    let row = sqlx::query(
        r#"
        SELECT message.chat_type, receipt.policy_version
        FROM qintopia_agent_os.feishu_message_ingress_receipts receipt
        JOIN qintopia_messages.messages message
          ON message.id = receipt.message_row_id
        JOIN qintopia_agent_os.conversation_policies policy
          ON policy.id = receipt.policy_id
         AND policy.policy_version = receipt.policy_version
         AND policy.enabled
        WHERE receipt.source_message_ref = $1
          AND receipt.conversation_ref = $2
          AND message.platform = 'feishu'
          AND message.message_id = $3
          AND message.chat_id = $4
          AND message.sender_id = $5
          AND message.sender_type = 'user'
          AND message.chat_type = 'direct'
          AND message.should_trigger
          AND policy.platform = 'feishu'
          AND policy.conversation_ref = receipt.conversation_ref
          AND policy.conversation_type = 'direct'
          AND policy.audience_class = 'private'
          AND policy.return_mode = 'direct_chat'
          AND policy.initiation_rule = 'direct_message'
          AND policy.status_visibility = 'requester'
          AND $6 = ANY(policy.allowed_capabilities)
        LIMIT 1
        "#,
    )
    .bind(expected_message_ref)
    .bind(expected_conversation_ref)
    .bind(&session.source_message_id)
    .bind(&session.conversation_id)
    .bind(&session.requester_user_id)
    .bind(required_capability)
    .fetch_optional(pool)
    .await
    .context("resolve authenticated V3 poster session")?
    .context("authenticated V3 direct-message policy binding is unavailable")?;
    session.conversation_type = row.try_get("chat_type")?;
    session.policy_version = row.try_get("policy_version")?;
    validate_session(&session)?;
    Ok(session)
}

fn validate_session_identity(session: &TrustedSession) -> Result<()> {
    if session.platform != "feishu" {
        bail!("trusted Feishu session is required");
    }
    for (name, value, max) in [
        ("conversation_id", &session.conversation_id, 200usize),
        ("requester_user_id", &session.requester_user_id, 200usize),
        ("source_message_id", &session.source_message_id, 240usize),
    ] {
        if value.is_empty()
            || value.len() > max
            || !value.bytes().all(|byte| {
                byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':' | b'.')
            })
        {
            bail!("{name} is invalid");
        }
    }
    Ok(())
}

fn validate_session(session: &TrustedSession) -> Result<()> {
    if session.platform != "feishu" || session.conversation_type != "direct" {
        bail!("trusted Feishu direct session is required");
    }
    validate_session_identity(session)
}

fn validate_text(name: &str, value: String, max_chars: usize) -> Result<String> {
    let value = value.trim().to_string();
    if value.is_empty() || value.chars().count() > max_chars || value.contains('\0') {
        bail!("{name} is invalid");
    }
    Ok(value)
}

fn normalize_priority(value: String) -> Result<String> {
    let value = if value.trim().is_empty() {
        "normal".to_string()
    } else {
        value.trim().to_string()
    };
    if !matches!(value.as_str(), "low" | "normal" | "high" | "urgent") {
        bail!("priority is invalid");
    }
    Ok(value)
}

fn validate_optional_ref(value: String) -> Result<String> {
    let value = value.trim().to_string();
    if value.len() > 240
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':' | b'.'))
    {
        bail!("activity record reference is invalid");
    }
    Ok(value)
}

fn digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn origin_ref(session: &TrustedSession) -> String {
    if session.policy_version > 0 {
        return digest(&[
            "poster-origin-v3",
            &session.platform,
            &session.source_message_id,
            crate::conversation_policy::POSTER_PRODUCTION_CAPABILITY,
        ]);
    }
    digest(&[
        "poster-origin-v1",
        &session.platform,
        &session.conversation_id,
        &session.requester_user_id,
    ])
}

fn actor_ref(session: &TrustedSession) -> String {
    digest(&[
        "poster-actor-v1",
        &session.platform,
        &session.requester_user_id,
    ])
}

fn source_message_ref(session: &TrustedSession) -> String {
    digest(&[
        "poster-message-v1",
        &session.platform,
        &session.source_message_id,
    ])
}

fn source_idempotency_key(session: &TrustedSession) -> String {
    format!(
        "poster_production_request:{}",
        digest(&[&session.platform, &session.source_message_id])
    )
}

fn prepare_socket(path: &Path) -> Result<()> {
    if !path.is_absolute()
        || path.file_name().and_then(|name| name.to_str()) != Some("operations-intake.sock")
    {
        bail!("operations intake socket path is invalid");
    }
    let parent = path
        .parent()
        .context("operations intake socket parent is missing")?;
    if !parent.exists() || !parent.is_dir() {
        bail!("operations intake socket parent is unavailable");
    }
    if path.exists() {
        if !path_is_socket(path) {
            bail!("operations intake path exists and is not a socket");
        }
        fs::remove_file(path).context("remove stale operations intake socket")?;
    }
    Ok(())
}

fn path_is_socket(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_socket())
        .unwrap_or(false)
}

#[cfg(all(test, feature = "postgres-integration-tests"))]
pub(crate) struct V3PosterIntegrationInput<'a> {
    pub conversation_id: &'a str,
    pub requester_user_id: &'a str,
    pub source_message_id: &'a str,
    pub request: &'a str,
    pub title: &'a str,
    pub schedule: &'a str,
    pub location: &'a str,
}

#[cfg(all(test, feature = "postgres-integration-tests"))]
pub(crate) async fn submit_v3_poster_for_postgres_integration(
    pool: &PgPool,
    input: V3PosterIntegrationInput<'_>,
) -> Result<Value> {
    let session = TrustedSession {
        platform: "feishu".to_string(),
        conversation_type: String::new(),
        conversation_id: input.conversation_id.to_string(),
        requester_user_id: input.requester_user_id.to_string(),
        source_message_id: input.source_message_id.to_string(),
        policy_version: 0,
    };
    handle_request(
        pool,
        &OperationsPolicy::dry_run(),
        IntakeRequest::PosterProductionRequest {
            schema_version: PROTOCOL_VERSION,
            request: input.request.to_string(),
            priority: "normal".to_string(),
            activity_record_ref: String::new(),
            activity_facts: ActivityFacts {
                source: "originating_request".to_string(),
                title: input.title.to_string(),
                schedule: input.schedule.to_string(),
                location: input.location.to_string(),
                conflict_fields: Vec::new(),
            },
            session,
            idempotency_key: String::new(),
        },
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "postgres-integration-tests")]
    use crate::poster_notification::ReviewCallbackIntegrationInput;

    #[cfg(feature = "postgres-integration-tests")]
    fn postgres_integration_database_url() -> String {
        assert_eq!(
            std::env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE").as_deref(),
            Ok("1"),
            "PostgreSQL integration test requires the explicit apply-smoke guard"
        );
        let database_url = std::env::var("QINTOPIA_SIDECAR_DATABASE_URL")
            .expect("PostgreSQL integration test requires QINTOPIA_SIDECAR_DATABASE_URL");
        let parsed = url::Url::parse(&database_url).expect("integration database URL must parse");
        assert!(
            matches!(parsed.scheme(), "postgres" | "postgresql"),
            "PostgreSQL integration test requires a postgres URL"
        );
        assert!(
            matches!(parsed.host_str(), Some("127.0.0.1" | "::1")),
            "PostgreSQL integration test may only use a literal loopback database"
        );
        assert_eq!(
            parsed.path().trim_start_matches('/'),
            "qintopia_test",
            "PostgreSQL integration test may only use qintopia_test"
        );
        database_url
    }

    #[cfg(feature = "postgres-integration-tests")]
    async fn seed_delivered_review_image(
        pool: &PgPool,
        source_image_work_item_id: Uuid,
        fixture_label: &str,
    ) -> (Uuid, Uuid, Uuid) {
        let image_work_item_id = Uuid::new_v4();
        let artifact_id = Uuid::new_v4();
        let fixture_ref = digest(&["poster-review-fixture-v1", fixture_label]);
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (id, parent_work_item_id, work_item_type, status, requester_agent,
                 target_agent, capability_key, human_owner, priority, brief_summary,
                 purpose, source_type, source_refs, dedupe_key, idempotency_key,
                 risk_level, information_class, payload, payload_redaction_policy,
                 review_policy, metadata)
            SELECT $2, parent_work_item_id, work_item_type, 'awaiting_review',
                   requester_agent, target_agent, capability_key, human_owner, priority,
                   brief_summary, $3, source_type, source_refs, $4, $5, risk_level,
                   information_class, payload, payload_redaction_policy, review_policy,
                   metadata
            FROM qintopia_agent_os.work_items
            WHERE id = $1
            "#,
        )
        .bind(source_image_work_item_id)
        .bind(image_work_item_id)
        .bind(format!("poster_review_{fixture_label}_fixture"))
        .bind(format!("{fixture_ref}:dedupe"))
        .bind(format!("{fixture_ref}:idempotency"))
        .execute(pool)
        .await
        .expect("insert reviewable image work item fixture");

        let content_hash = digest(&["poster-review-image-v1", fixture_label]);
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.artifacts
                (id, work_item_id, artifact_type, review_status, created_by_agent,
                 title, summary, artifact_uri, content_hash, information_class, metadata)
            VALUES ($1, $2, 'generated_image', 'pending', 'huabaosi',
                    'integration review poster', 'sanitized review fixture',
                    $3, $4, 'internal_ops', '{}'::jsonb)
            "#,
        )
        .bind(artifact_id)
        .bind(image_work_item_id)
        .bind(format!(
            "https://media.example.test/posters/{fixture_label}.jpg"
        ))
        .bind(&content_hash)
        .execute(pool)
        .await
        .expect("insert reviewable generated-image fixture");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_item_events
                (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
            VALUES ($1, $2, 'generated_image_created', 'worker',
                    'huabaosi-image-generation-worker',
                    'integration review image created',
                    jsonb_build_object('content_hash', $3::text,
                                       'external_publish_executed', false))
            "#,
        )
        .bind(image_work_item_id)
        .bind(artifact_id)
        .bind(&content_hash)
        .execute(pool)
        .await
        .expect("insert reviewable image creation event");

        crate::poster_notification::run_starter_for_postgres_integration(pool, image_work_item_id)
            .await
            .expect("create review fixture notification");
        let notification = sqlx::query(
            r#"
            SELECT id, work_item_id
            FROM qintopia_agent_os.poster_notifications
            WHERE source_work_item_id = $1 AND notification_kind = 'image_ready'
            "#,
        )
        .bind(image_work_item_id)
        .fetch_one(pool)
        .await
        .expect("load review fixture notification");
        let notification_id: Uuid = notification.try_get("id").unwrap();
        let notification_work_item_id: Uuid = notification.try_get("work_item_id").unwrap();
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.poster_notification_attempts
                (notification_id, attempt_number, claim_token, status, audit_metadata,
                 send_started_at, completed_at)
            VALUES ($1, 1, $2, 'delivered',
                    '{"external_send_outcome":"accepted","automatic_retry_allowed":false}'::jsonb,
                    now(), now())
            "#,
        )
        .bind(notification_id)
        .bind(format!("integration-{fixture_label}-delivered"))
        .execute(pool)
        .await
        .expect("record review fixture delivery attempt");
        sqlx::query(
            "UPDATE qintopia_agent_os.poster_notifications SET status='delivered', delivered_at=now(), updated_at=now() WHERE id=$1",
        )
        .bind(notification_id)
        .execute(pool)
        .await
        .expect("mark review fixture notification delivered");
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET status='completed', updated_at=now() WHERE id=$1",
        )
        .bind(notification_work_item_id)
        .execute(pool)
        .await
        .expect("complete review fixture notification work item");
        (image_work_item_id, artifact_id, notification_id)
    }

    fn session() -> TrustedSession {
        TrustedSession {
            platform: "feishu".to_string(),
            conversation_type: "direct".to_string(),
            conversation_id: "oc_chat_fixture".to_string(),
            requester_user_id: "ou_user_fixture".to_string(),
            source_message_id: "om_message_fixture".to_string(),
            policy_version: 0,
        }
    }

    #[test]
    fn trusted_refs_are_stable_and_separate() {
        let session = session();
        assert_eq!(origin_ref(&session), origin_ref(&session));
        assert_ne!(origin_ref(&session), actor_ref(&session));
        assert_ne!(actor_ref(&session), source_message_ref(&session));
        assert!(!origin_ref(&session).contains("oc_chat_fixture"));
    }

    #[test]
    fn v3_origin_is_message_scoped_while_v2_remains_conversation_scoped() {
        let first_v2 = session();
        let mut second_v2 = first_v2.clone();
        second_v2.source_message_id = "om_second_fixture".to_string();
        assert_eq!(origin_ref(&first_v2), origin_ref(&second_v2));

        let mut first_v3 = first_v2;
        first_v3.policy_version = 1;
        let mut second_v3 = second_v2;
        second_v3.policy_version = 1;
        assert_ne!(origin_ref(&first_v3), origin_ref(&second_v3));

        second_v3.conversation_id = "oc_other_fixture".to_string();
        second_v3.requester_user_id = "ou_other_fixture".to_string();
        second_v3.source_message_id = first_v3.source_message_id.clone();
        assert_eq!(origin_ref(&first_v3), origin_ref(&second_v3));
    }

    #[test]
    fn group_and_non_feishu_sessions_are_rejected() {
        let mut grouped = session();
        grouped.conversation_type = "group".to_string();
        assert!(validate_session(&grouped).is_err());
        let mut wecom = session();
        wecom.platform = "wecom".to_string();
        assert!(validate_session(&wecom).is_err());
    }

    #[test]
    fn idempotency_comes_only_from_source_message() {
        let mut other_chat = session();
        other_chat.conversation_id = "oc_other".to_string();
        assert_eq!(
            source_idempotency_key(&session()),
            source_idempotency_key(&other_chat)
        );
        other_chat.source_message_id = "om_other".to_string();
        assert_ne!(
            source_idempotency_key(&session()),
            source_idempotency_key(&other_chat)
        );
    }

    #[test]
    fn fact_assessment_summary_contains_only_validated_activity_facts() {
        let assessment = FactAssessment {
            status: "complete",
            source: "originating_request",
            title: "周末晚餐".to_string(),
            schedule: "周六18:00".to_string(),
            location: "秦托邦会客厅".to_string(),
            missing_fields: Vec::new(),
            conflict_fields: Vec::new(),
        };
        let summary = assessment.workflow_summary("含有不应复制的模型补充内容");
        assert_eq!(
            summary,
            "周末晚餐｜周六18:00｜秦托邦会客厅｜按原始指令生成活动海报"
        );
        assert!(!summary.contains("模型补充"));
    }

    #[test]
    fn activity_record_refs_are_strictly_bounded() {
        assert!(valid_activity_record_ref("activity_plan:012345abcdef"));
        assert!(valid_activity_record_ref(
            "activity_occurrence:abcdef012345"
        ));
        assert!(!valid_activity_record_ref("activity_plan:rec_sensitive"));
        assert!(!valid_activity_record_ref("event_signal:012345abcdef"));
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
    async fn postgres_poster_intake_is_idempotent_authorized_and_never_group_sends() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect guarded poster integration database");
        db::run_migrations(&pool)
            .await
            .expect("migrate guarded poster integration database");

        let suffix = Uuid::new_v4().simple().to_string();
        let session = TrustedSession {
            platform: "feishu".to_string(),
            conversation_type: "direct".to_string(),
            conversation_id: format!("oc_{suffix}"),
            requester_user_id: format!("ou_{suffix}"),
            source_message_id: format!("om_{suffix}"),
            policy_version: 0,
        };
        let source_text = "请为周末晚餐生成海报，时间周六18:00，地点秦托邦会客厅";
        sqlx::query(
            r#"
            INSERT INTO qintopia_messages.messages
                (platform, message_id, event_id, chat_id, chat_type, sender_id,
                 message_kind, text, received_at)
            VALUES ('feishu', $1, $2, $3, 'direct', $4, 'text', $5, now())
            "#,
        )
        .bind(&session.source_message_id)
        .bind(format!("evt_{suffix}"))
        .bind(&session.conversation_id)
        .bind(&session.requester_user_id)
        .bind(source_text)
        .execute(&pool)
        .await
        .expect("insert trusted originating Feishu message");

        let request = || IntakeRequest::PosterProductionRequest {
            schema_version: LEGACY_PROTOCOL_VERSION,
            request: source_text.to_string(),
            priority: "normal".to_string(),
            activity_record_ref: String::new(),
            activity_facts: ActivityFacts {
                source: "originating_request".to_string(),
                title: "周末晚餐".to_string(),
                schedule: "周六18:00".to_string(),
                location: "秦托邦会客厅".to_string(),
                conflict_fields: Vec::new(),
            },
            session: session.clone(),
            idempotency_key: source_idempotency_key(&session),
        };
        let policy = OperationsPolicy::dry_run();

        let mut clarification_session = session.clone();
        clarification_session.source_message_id = format!("om_clarification_{suffix}");
        sqlx::query(
            r#"
            INSERT INTO qintopia_messages.messages
                (platform, message_id, event_id, chat_id, chat_type, sender_id,
                 message_kind, text, received_at)
            VALUES ('feishu', $1, $2, $3, 'direct', $4, 'text', '请生成活动海报', now())
            "#,
        )
        .bind(&clarification_session.source_message_id)
        .bind(format!("evt_clarification_{suffix}"))
        .bind(&clarification_session.conversation_id)
        .bind(&clarification_session.requester_user_id)
        .execute(&pool)
        .await
        .expect("insert incomplete originating Feishu message");
        let clarification = handle_request(
            &pool,
            &policy,
            IntakeRequest::PosterProductionRequest {
                schema_version: LEGACY_PROTOCOL_VERSION,
                request: "请生成活动海报".to_string(),
                priority: "normal".to_string(),
                activity_record_ref: String::new(),
                activity_facts: ActivityFacts {
                    source: "originating_request".to_string(),
                    ..ActivityFacts::default()
                },
                session: clarification_session.clone(),
                idempotency_key: source_idempotency_key(&clarification_session),
            },
        )
        .await
        .expect("accept poster request that needs clarification");
        assert_eq!(clarification["user_status"], "需补充");
        let clarification_visual_id =
            Uuid::parse_str(clarification["visual_work_item_id"].as_str().unwrap()).unwrap();
        let initial_clarification_status: String =
            sqlx::query_scalar("SELECT status FROM qintopia_agent_os.work_items WHERE id=$1")
                .bind(clarification_visual_id)
                .fetch_one(&pool)
                .await
                .expect("load initial clarification visual status");
        assert_eq!(initial_clarification_status, "awaiting_review");
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET status='queued', updated_at=now() WHERE id=$1",
        )
        .bind(clarification_visual_id)
        .execute(&pool)
        .await
        .expect("simulate an incorrectly requeued incomplete visual");
        let incomplete_worker =
            crate::collaboration::run_once_for_postgres_integration(&pool, clarification_visual_id)
                .await;
        assert!(
            incomplete_worker.is_err(),
            "missing fact gate must not be selectable by the visual worker"
        );
        let mut clarification_payload: Value =
            sqlx::query_scalar("SELECT payload FROM qintopia_agent_os.work_items WHERE id=$1")
                .bind(clarification_visual_id)
                .fetch_one(&pool)
                .await
                .expect("load clarification visual payload");
        clarification_payload["poster_fact_gate"] = json!({
            "status": "complete",
            "source": "originating_request",
            "missing_fields": [],
            "conflict_fields": ["活动时间"]
        });
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET payload=$2, updated_at=now() WHERE id=$1",
        )
        .bind(clarification_visual_id)
        .bind(clarification_payload)
        .execute(&pool)
        .await
        .expect("replace missing fact gate with conflicting fact gate");
        let conflicting_worker =
            crate::collaboration::run_once_for_postgres_integration(&pool, clarification_visual_id)
                .await;
        assert!(
            conflicting_worker.is_err(),
            "conflicting fact gate must not be selectable by the visual worker"
        );
        let blocked_visual_state: (String, i32, i64) = sqlx::query_as(
            r#"
            SELECT visual.status, visual.attempts, count(artifact.id)
            FROM qintopia_agent_os.work_items visual
            LEFT JOIN qintopia_agent_os.artifacts artifact ON artifact.work_item_id=visual.id
            WHERE visual.id=$1
            GROUP BY visual.status, visual.attempts
            "#,
        )
        .bind(clarification_visual_id)
        .fetch_one(&pool)
        .await
        .expect("verify clarification visual remains unclaimed");
        assert_eq!(blocked_visual_state, ("queued".to_string(), 0, 0));

        let first = handle_request(&pool, &policy, request())
            .await
            .expect("accept first poster request");
        let duplicate = handle_request(&pool, &policy, request())
            .await
            .expect("dedupe repeated poster request");
        assert_eq!(first["accepted"], true);
        assert_eq!(first["user_status"], "已接单");
        assert_eq!(first["workflow_root_id"], duplicate["workflow_root_id"]);
        assert_eq!(
            first["visual_work_item_id"],
            duplicate["visual_work_item_id"]
        );
        assert_eq!(duplicate["deduped"], true);

        let root_id = Uuid::parse_str(first["workflow_root_id"].as_str().unwrap()).unwrap();
        let visual_id = Uuid::parse_str(first["visual_work_item_id"].as_str().unwrap()).unwrap();
        let evidence_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM qintopia_agent_os.work_items WHERE parent_work_item_id=$1 AND work_item_type='evidence_request'",
        )
        .bind(root_id)
        .fetch_one(&pool)
        .await
        .expect("load evidence child");
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET status='completed', updated_at=now() WHERE id=$1",
        )
        .bind(evidence_id)
        .execute(&pool)
        .await
        .expect("complete evidence fixture");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.artifacts
                (work_item_id, artifact_type, review_status, created_by_agent, title,
                 summary, content_text, content_hash, information_class, metadata)
            VALUES ($1, 'evidence_summary', 'not_required', 'wenyuange',
                    'integration evidence', 'source-grounded fixture', 'fixture', $2,
                    'internal_ops', '{}'::jsonb)
            "#,
        )
        .bind(evidence_id)
        .bind(format!("sha256:{}", "d".repeat(64)))
        .execute(&pool)
        .await
        .expect("insert evidence fixture");

        crate::collaboration::run_once_for_postgres_integration(&pool, visual_id)
            .await
            .expect("create and authorize poster brief");
        crate::operations::run_xiaoman_poster_image_starter_for_postgres_integration(
            &pool, visual_id,
        )
        .await
        .expect("create image-generation request");
        crate::operations::run_xiaoman_poster_image_starter_for_postgres_integration(
            &pool, visual_id,
        )
        .await
        .expect("repeat image starter idempotently");

        let counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT
                count(DISTINCT item.id) FILTER (WHERE item.id=$1),
                count(DISTINCT artifact.id) FILTER (
                    WHERE artifact.artifact_type='poster_brief'
                      AND artifact.review_status='approved'
                ),
                count(DISTINCT image.id) FILTER (
                    WHERE image.work_item_type='image_generation_request'
                ),
                count(DISTINCT event.id) FILTER (
                    WHERE event.event_type='poster_generation_authorized'
                ),
                count(DISTINCT group_item.id) FILTER (
                    WHERE group_item.work_item_type='group_message_request'
                )
            FROM qintopia_agent_os.work_items item
            LEFT JOIN qintopia_agent_os.artifacts artifact ON artifact.work_item_id=$2
            LEFT JOIN qintopia_agent_os.work_items image ON image.parent_work_item_id=$2
            LEFT JOIN qintopia_agent_os.work_item_events event ON event.work_item_id=$1
            LEFT JOIN qintopia_agent_os.work_items group_item ON group_item.parent_work_item_id=$1
            WHERE item.id=$1
            "#,
        )
        .bind(root_id)
        .bind(visual_id)
        .fetch_one(&pool)
        .await
        .expect("read poster integration counts");
        assert_eq!(counts, (1, 1, 1, 1, 0));

        let image_row = sqlx::query(
            r#"
            SELECT id, payload
            FROM qintopia_agent_os.work_items
            WHERE parent_work_item_id=$1 AND work_item_type='image_generation_request'
            "#,
        )
        .bind(visual_id)
        .fetch_one(&pool)
        .await
        .expect("load image-generation request");
        let image_id: Uuid = image_row.try_get("id").unwrap();
        let image_payload: Value = image_row.try_get("payload").unwrap();
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET status='awaiting_review', updated_at=now() WHERE id=$1",
        )
        .bind(image_id)
        .execute(&pool)
        .await
        .expect("mark generated image awaiting review");

        let artifact_id = Uuid::new_v4();
        let content_hash = format!("sha256:{}", "e".repeat(64));
        let brief_id = image_payload["approved_brief_artifact_id"].clone();
        let brief_hash = image_payload["approved_brief_content_hash"].clone();
        let prompt_hash = image_payload["prompt_hash"].clone();
        let metadata = json!({
            "generated_by": "huabaosi-image-generation-worker",
            "provider": "openai-compatible",
            "model": "gpt-image-2",
            "mime_type": "image/jpeg",
            "file_md5": "e2c865db4162bed963bfaa9ef6ac18f0",
            "provider_source_mime_type": "image/png",
            "provider_source_content_hash": format!("sha256:{}", "f".repeat(64)),
            "media_transform": "png_to_jpeg_white_background_q92_v1",
            "jpeg_quality": 92,
            "alpha_background": "#ffffff",
            "width": 1254,
            "height": 1254,
            "byte_size": 4096,
            "approved_brief_artifact_id": brief_id,
            "approved_brief_content_hash": brief_hash,
            "prompt_hash": prompt_hash,
        });
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.artifacts
                (id, work_item_id, artifact_type, review_status, created_by_agent,
                 title, summary, artifact_uri, content_hash, source_ids, risk_labels,
                 information_class, metadata)
            VALUES
                ($1, $2, 'generated_image', 'pending', 'huabaosi',
                 'integration generated poster', 'sanitized fixture',
                 'https://media.example.test/posters/image.jpg', $3,
                 jsonb_build_array(jsonb_build_object(
                     'approved_brief_artifact_id', $4::jsonb,
                     'approved_brief_content_hash', $5::jsonb
                 )),
                 ARRAY['external_use_review_required','generated_media']::text[],
                 'internal_ops', $6)
            "#,
        )
        .bind(artifact_id)
        .bind(image_id)
        .bind(&content_hash)
        .bind(&brief_id)
        .bind(&brief_hash)
        .bind(&metadata)
        .execute(&pool)
        .await
        .expect("insert pending generated image");
        let mut creation_data = metadata.as_object().unwrap().clone();
        creation_data.insert("content_hash".to_string(), json!(content_hash));
        creation_data.insert("external_publish_executed".to_string(), json!(false));
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_item_events
                (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
            VALUES ($1, $2, 'generated_image_created', 'worker',
                    'huabaosi-image-generation-worker',
                    'integration generated image created', $3)
            "#,
        )
        .bind(image_id)
        .bind(artifact_id)
        .bind(Value::Object(creation_data))
        .execute(&pool)
        .await
        .expect("insert generated-image creation audit");

        crate::poster_notification::run_starter_for_postgres_integration(&pool, image_id)
            .await
            .expect("create durable poster notification");
        crate::poster_notification::run_starter_for_postgres_integration(&pool, image_id)
            .await
            .expect("repeat notification starter idempotently");
        let notification = sqlx::query(
            r#"
            SELECT notification.id, notification.work_item_id
            FROM qintopia_agent_os.poster_notifications notification
            WHERE notification.source_work_item_id=$1
              AND notification.notification_kind='image_ready'
            "#,
        )
        .bind(image_id)
        .fetch_one(&pool)
        .await
        .expect("load durable poster notification");
        let notification_id: Uuid = notification.try_get("id").unwrap();
        let notification_work_item_id: Uuid = notification.try_get("work_item_id").unwrap();
        let notification_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM qintopia_agent_os.poster_notifications WHERE source_work_item_id=$1",
        )
        .bind(image_id)
        .fetch_one(&pool)
        .await
        .expect("count durable notifications");
        assert_eq!(notification_count, 1);

        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.poster_notification_attempts
                (notification_id, attempt_number, claim_token, status, audit_metadata,
                 send_started_at, completed_at)
            VALUES ($1, 1, 'integration-delivered', 'delivered',
                    '{"external_send_outcome":"accepted","automatic_retry_allowed":false}'::jsonb,
                    now(), now())
            "#,
        )
        .bind(notification_id)
        .execute(&pool)
        .await
        .expect("record fake-server delivery attempt");
        sqlx::query(
            "UPDATE qintopia_agent_os.poster_notifications SET status='delivered', delivered_at=now(), updated_at=now() WHERE id=$1",
        )
        .bind(notification_id)
        .execute(&pool)
        .await
        .expect("record fake-server notification delivery");
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET status='completed', updated_at=now() WHERE id=$1",
        )
        .bind(notification_work_item_id)
        .execute(&pool)
        .await
        .expect("complete notification work item");

        let wrong_actor = format!("ou_wrong_{suffix}");
        let forged = crate::poster_notification::process_review_callback_for_postgres_integration(
            &pool,
            &database_url,
            ReviewCallbackIntegrationInput {
                callback_event_id: &format!("evt_wrong_{suffix}"),
                notification_id,
                artifact_id,
                conversation_id: &session.conversation_id,
                actor_user_id: &wrong_actor,
                action: "approve",
            },
        )
        .await;
        assert!(forged.is_err(), "wrong user callback must be rejected");

        let callback_event_id = format!("evt_review_{suffix}");
        let first_review =
            crate::poster_notification::process_review_callback_for_postgres_integration(
                &pool,
                &database_url,
                ReviewCallbackIntegrationInput {
                    callback_event_id: &callback_event_id,
                    notification_id,
                    artifact_id,
                    conversation_id: &session.conversation_id,
                    actor_user_id: &session.requester_user_id,
                    action: "approve",
                },
            )
            .await
            .expect("approve delivered poster from originating user");
        assert!(!first_review);
        let wrong_chat_after_review =
            crate::poster_notification::process_review_callback_for_postgres_integration(
                &pool,
                &database_url,
                ReviewCallbackIntegrationInput {
                    callback_event_id: &callback_event_id,
                    notification_id,
                    artifact_id,
                    conversation_id: &format!("oc_wrong_{suffix}"),
                    actor_user_id: &session.requester_user_id,
                    action: "approve",
                },
            )
            .await;
        assert!(
            wrong_chat_after_review.is_err(),
            "an idempotent callback must still match the originating conversation"
        );
        let duplicate_review =
            crate::poster_notification::process_review_callback_for_postgres_integration(
                &pool,
                &database_url,
                ReviewCallbackIntegrationInput {
                    callback_event_id: &callback_event_id,
                    notification_id,
                    artifact_id,
                    conversation_id: &session.conversation_id,
                    actor_user_id: &session.requester_user_id,
                    action: "approve",
                },
            )
            .await
            .expect("dedupe repeated poster review callback");
        assert!(duplicate_review);
        let alternate_event_duplicate =
            crate::poster_notification::process_review_callback_for_postgres_integration(
                &pool,
                &database_url,
                ReviewCallbackIntegrationInput {
                    callback_event_id: &format!("evt_review_retry_{suffix}"),
                    notification_id,
                    artifact_id,
                    conversation_id: &session.conversation_id,
                    actor_user_id: &session.requester_user_id,
                    action: "approve",
                },
            )
            .await
            .expect("dedupe a new Feishu event id for the same review card decision");
        assert!(alternate_event_duplicate);
        let changed_decision =
            crate::poster_notification::process_review_callback_for_postgres_integration(
                &pool,
                &database_url,
                ReviewCallbackIntegrationInput {
                    callback_event_id: &format!("evt_review_changed_{suffix}"),
                    notification_id,
                    artifact_id,
                    conversation_id: &session.conversation_id,
                    actor_user_id: &session.requester_user_id,
                    action: "modify",
                },
            )
            .await;
        assert!(
            changed_decision.is_err(),
            "the same notification cannot be rebound to another decision"
        );
        let rebound = crate::poster_notification::process_review_callback_for_postgres_integration(
            &pool,
            &database_url,
            ReviewCallbackIntegrationInput {
                callback_event_id: &callback_event_id,
                notification_id,
                artifact_id,
                conversation_id: &session.conversation_id,
                actor_user_id: &wrong_actor,
                action: "approve",
            },
        )
        .await;
        assert!(
            rebound.is_err(),
            "callback id cannot be rebound to another actor"
        );

        let review_state: (String, String, i64) = sqlx::query_as(
            r#"
            SELECT artifact.review_status, image.status,
                   count(action.callback_event_id)
            FROM qintopia_agent_os.artifacts artifact
            JOIN qintopia_agent_os.work_items image ON image.id=artifact.work_item_id
            LEFT JOIN qintopia_agent_os.poster_review_actions action
              ON action.artifact_id=artifact.id
            WHERE artifact.id=$1
            GROUP BY artifact.review_status, image.status
            "#,
        )
        .bind(artifact_id)
        .fetch_one(&pool)
        .await
        .expect("read persisted poster review state");
        assert_eq!(
            review_state,
            ("approved".to_string(), "completed".to_string(), 1)
        );

        let (modify_image_id, modify_artifact_id, modify_notification_id) =
            seed_delivered_review_image(&pool, image_id, &format!("modify-{suffix}")).await;
        let modify_event_id = format!("evt_modify_{suffix}");
        let modify_deduped =
            crate::poster_notification::process_review_callback_for_postgres_integration(
                &pool,
                &database_url,
                ReviewCallbackIntegrationInput {
                    callback_event_id: &modify_event_id,
                    notification_id: modify_notification_id,
                    artifact_id: modify_artifact_id,
                    conversation_id: &session.conversation_id,
                    actor_user_id: &session.requester_user_id,
                    action: "modify",
                },
            )
            .await
            .expect("request poster changes from originating user");
        assert!(!modify_deduped);
        let modify_state: (String, String) = sqlx::query_as(
            r#"
            SELECT artifact.review_status, image.status
            FROM qintopia_agent_os.artifacts artifact
            JOIN qintopia_agent_os.work_items image ON image.id = artifact.work_item_id
            WHERE artifact.id = $1
            "#,
        )
        .bind(modify_artifact_id)
        .fetch_one(&pool)
        .await
        .expect("read persisted poster modification state");
        assert_eq!(
            modify_state,
            (
                "changes_requested".to_string(),
                "awaiting_review".to_string()
            )
        );

        let mut revision_session = session.clone();
        revision_session.source_message_id = format!("om_revision_{suffix}");
        let revision_instruction = "标题缩短，并把活动时间放到主视觉下方";
        sqlx::query(
            r#"
            INSERT INTO qintopia_messages.messages
                (platform, message_id, event_id, chat_id, chat_type, sender_id,
                 message_kind, text, received_at)
            VALUES ('feishu', $1, $2, $3, 'direct', $4, 'text', $5, now())
            "#,
        )
        .bind(&revision_session.source_message_id)
        .bind(format!("evt_revision_message_{suffix}"))
        .bind(&revision_session.conversation_id)
        .bind(&revision_session.requester_user_id)
        .bind(revision_instruction)
        .execute(&pool)
        .await
        .expect("insert trusted poster revision message");
        let revision_request = || IntakeRequest::PosterRevisionRequest {
            schema_version: LEGACY_PROTOCOL_VERSION,
            request: revision_instruction.to_string(),
            workflow_root_id: root_id,
            revision_of_artifact_id: modify_artifact_id,
            session: revision_session.clone(),
            idempotency_key: source_idempotency_key(&revision_session),
        };
        let first_revision = handle_request(&pool, &policy, revision_request())
            .await
            .expect("accept explicit poster revision instruction");
        let duplicate_revision = handle_request(&pool, &policy, revision_request())
            .await
            .expect("dedupe explicit poster revision instruction");
        assert_eq!(first_revision["accepted"], true);
        assert_eq!(
            first_revision["image_generation_work_item_id"],
            duplicate_revision["image_generation_work_item_id"]
        );
        assert_eq!(duplicate_revision["deduped"], true);
        let revision_state: (i64, i64, String, Value) = sqlx::query_as(
            r#"
            SELECT
                count(DISTINCT revision.id),
                count(DISTINCT image.id),
                min(revision.instruction_text),
                (array_agg(image.payload))[1]
            FROM qintopia_agent_os.poster_revision_requests revision
            JOIN qintopia_agent_os.work_items image
              ON image.id = revision.image_generation_work_item_id
            WHERE revision.workflow_root_id = $1
              AND revision.source_artifact_id = $2
              AND revision.source_message_ref = $3
            "#,
        )
        .bind(root_id)
        .bind(modify_artifact_id)
        .bind(source_message_ref(&revision_session))
        .fetch_one(&pool)
        .await
        .expect("read persisted poster revision request");
        assert_eq!(revision_state.0, 1);
        assert_eq!(revision_state.1, 1);
        assert_eq!(revision_state.2, revision_instruction);
        assert_eq!(
            revision_state.3["revision_of_artifact_id"],
            json!(modify_artifact_id)
        );
        assert_eq!(revision_state.3["group_send_authorized"], false);
        assert_ne!(modify_image_id, image_id);

        let (abandon_image_id, abandon_artifact_id, abandon_notification_id) =
            seed_delivered_review_image(&pool, image_id, &format!("abandon-{suffix}")).await;
        let abandon_event_id = format!("evt_abandon_{suffix}");
        let abandon_deduped =
            crate::poster_notification::process_review_callback_for_postgres_integration(
                &pool,
                &database_url,
                ReviewCallbackIntegrationInput {
                    callback_event_id: &abandon_event_id,
                    notification_id: abandon_notification_id,
                    artifact_id: abandon_artifact_id,
                    conversation_id: &session.conversation_id,
                    actor_user_id: &session.requester_user_id,
                    action: "abandon",
                },
            )
            .await
            .expect("abandon poster from originating user");
        assert!(!abandon_deduped);
        let abandon_state: (String, String) = sqlx::query_as(
            r#"
            SELECT artifact.review_status, image.status
            FROM qintopia_agent_os.artifacts artifact
            JOIN qintopia_agent_os.work_items image ON image.id = artifact.work_item_id
            WHERE artifact.id = $1
            "#,
        )
        .bind(abandon_artifact_id)
        .fetch_one(&pool)
        .await
        .expect("read persisted poster abandonment state");
        assert_eq!(
            abandon_state,
            ("rejected".to_string(), "cancelled".to_string())
        );
        assert_ne!(abandon_image_id, image_id);

        let action_counts: (i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT
                count(*) FILTER (WHERE decision = 'approved'),
                count(*) FILTER (WHERE decision = 'changes_requested'),
                count(*) FILTER (WHERE decision = 'rejected')
            FROM qintopia_agent_os.poster_review_actions
            WHERE notification_id IN ($1, $2, $3)
            "#,
        )
        .bind(notification_id)
        .bind(modify_notification_id)
        .bind(abandon_notification_id)
        .fetch_one(&pool)
        .await
        .expect("count persisted poster review actions");
        assert_eq!(action_counts, (1, 1, 1));

        let forbidden_events: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM qintopia_agent_os.work_item_events
            WHERE event_type IN ('send_executed', 'external_published')
              AND work_item_id IN (
                  SELECT id FROM qintopia_agent_os.work_items
                  WHERE id=$1 OR parent_work_item_id=$1 OR parent_work_item_id=$2
              )
            "#,
        )
        .bind(root_id)
        .bind(visual_id)
        .fetch_one(&pool)
        .await
        .expect("verify poster workflow has no send events");
        assert_eq!(forbidden_events, 0);
        let group_work_items: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM qintopia_agent_os.work_items
            WHERE work_item_type = 'group_message_request'
              AND (id = $1 OR parent_work_item_id = $1 OR parent_work_item_id = $2)
            "#,
        )
        .bind(root_id)
        .bind(visual_id)
        .fetch_one(&pool)
        .await
        .expect("verify poster review and revision created no group-send work");
        assert_eq!(group_work_items, 0);
    }
}
