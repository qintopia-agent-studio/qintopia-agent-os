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
    space_configuration::{self, ProgrammingExtensionRequest, TrustedSpaceSession},
    space_programming_extension::{
        self, BrokerRequest as ProgrammingExtensionBrokerRequest,
        DispatchConfig as ProgrammingExtensionDispatchConfig,
    },
    space_turn_policy,
};

const LEGACY_PROTOCOL_VERSION: u8 = 2;
const PROTOCOL_VERSION: u8 = 3;
const SPACE_CHANGE_PROTOCOL_VERSION: u8 = 1;
const SPACE_SESSION_RECEIPT_MAX_AGE_MINUTES: i64 = 10;
const SPACE_SESSION_RECEIPT_LOOKUP_ATTEMPTS: usize = 6;
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
    #[serde(skip)]
    binding: Option<ConversationBinding>,
}

#[derive(Debug, Clone)]
struct ConversationBinding {
    policy_id: Uuid,
    conversation_ref: String,
    audience_class: String,
    delivery_mode: String,
    status_visibility: String,
    thread_root_message_id: Option<String>,
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

#[derive(Debug)]
struct RevisionWinner {
    source_message_ref: String,
    actor_ref: String,
    image_generation_work_item_id: Uuid,
}

#[derive(Debug)]
struct WorkflowAccess {
    conversation_type: String,
    conversation_id: String,
    requester_user_id: String,
    conversation_ref: String,
    policy_version: i64,
    delivery_mode: String,
    thread_root_message_id: Option<String>,
}

pub(crate) struct PosterReviewActor {
    pub(crate) actor_ref: String,
    pub(crate) policy_version: i64,
}

#[derive(Debug, Deserialize)]
#[serde(tag = "operation", rename_all = "snake_case")]
enum IntakeRequest {
    SpaceChangePrepare {
        schema_version: u8,
        intent: Value,
        session: TrustedSession,
    },
    SpaceProgrammingExtensionPrepare {
        schema_version: u8,
        request: ProgrammingExtensionRequest,
        session: TrustedSession,
    },
    SpaceProgrammingExtensionContinuationIntent {
        schema_version: u8,
        request_id: Uuid,
        session: TrustedSession,
    },
    SpaceProgrammingExtensionShadowPrepare {
        schema_version: u8,
        request_id: Uuid,
        intent: Value,
        session: TrustedSession,
    },
    SpaceChangeConfirm {
        schema_version: u8,
        proposal_id: Uuid,
        confirmation_code: String,
        session: TrustedSession,
    },
    SpaceChangeStatus {
        schema_version: u8,
        request_id: Uuid,
        session: TrustedSession,
    },
    SpaceTurnPolicyContext {
        schema_version: u8,
        session: TrustedSession,
    },
    SpaceTurnCapabilityAuthorize {
        schema_version: u8,
        capability_key: String,
        session: TrustedSession,
    },
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
    SpaceProgrammingExtension(ProgrammingExtensionBrokerRequest),
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
    let programming_extension_dispatch = ProgrammingExtensionDispatchConfig::from_env()?;
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
            if handle_connection(
                stream,
                &pool,
                &policy,
                ingress_config.as_deref(),
                programming_extension_dispatch,
            )
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
    programming_extension_dispatch: ProgrammingExtensionDispatchConfig,
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
    let authenticated_ingress_enabled = ingress_config.is_some();
    let internal_group_enabled =
        ingress_config.is_some_and(conversation_ingress::IngressConfig::internal_group_enabled);
    let future = async {
        match request {
            WireRequest::Legacy(request) => {
                handle_request(
                    pool,
                    policy,
                    *request,
                    authenticated_ingress_enabled,
                    internal_group_enabled,
                )
                .await
            }
            WireRequest::FeishuMessage(envelope) => {
                conversation_ingress::handle(pool, ingress_config, envelope).await
            }
            WireRequest::SpaceProgrammingExtension(request) => {
                space_programming_extension::handle(pool, programming_extension_dispatch, request)
                    .await
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
    parse_wire_request(value)
}

fn parse_wire_request(value: Value) -> Result<WireRequest> {
    if value.get("body_base64").is_some() {
        return serde_json::from_value(value)
            .map(WireRequest::FeishuMessage)
            .context("parse signed Feishu message ingress envelope");
    }
    if matches!(
        value.get("operation").and_then(Value::as_str),
        Some("space_programming_extension_claim" | "space_programming_extension_finish")
    ) {
        return serde_json::from_value(value)
            .map(WireRequest::SpaceProgrammingExtension)
            .context("parse Space programming extension broker request");
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
    authenticated_ingress_enabled: bool,
    internal_group_enabled: bool,
) -> Result<Value> {
    match request {
        IntakeRequest::SpaceChangePrepare {
            schema_version,
            intent,
            session,
        } => {
            validate_space_change_protocol(schema_version)?;
            let session = resolve_trusted_space_session(pool, session).await?;
            space_configuration::prepare(pool, session, intent).await
        }
        IntakeRequest::SpaceProgrammingExtensionPrepare {
            schema_version,
            request,
            session,
        } => {
            validate_space_change_protocol(schema_version)?;
            let session = resolve_trusted_space_session(pool, session).await?;
            space_configuration::prepare_programming_extension(pool, session, request).await
        }
        IntakeRequest::SpaceProgrammingExtensionContinuationIntent {
            schema_version,
            request_id,
            session,
        } => {
            validate_space_change_protocol(schema_version)?;
            let session = resolve_trusted_space_session(pool, session).await?;
            space_configuration::programming_extension_continuation_intent(
                pool, session, request_id,
            )
            .await
        }
        IntakeRequest::SpaceProgrammingExtensionShadowPrepare {
            schema_version,
            request_id,
            intent,
            session,
        } => {
            validate_space_change_protocol(schema_version)?;
            let session = resolve_trusted_space_session(pool, session).await?;
            space_configuration::prepare_programming_extension_shadow(
                pool, session, request_id, intent,
            )
            .await
        }
        IntakeRequest::SpaceChangeConfirm {
            schema_version,
            proposal_id,
            confirmation_code,
            session,
        } => {
            validate_space_change_protocol(schema_version)?;
            let session = resolve_trusted_space_session(pool, session).await?;
            space_configuration::confirm(pool, session, proposal_id, confirmation_code).await
        }
        IntakeRequest::SpaceChangeStatus {
            schema_version,
            request_id,
            session,
        } => {
            validate_space_change_protocol(schema_version)?;
            let session = resolve_trusted_space_session(pool, session).await?;
            space_configuration::status(pool, session, request_id).await
        }
        IntakeRequest::SpaceTurnPolicyContext {
            schema_version,
            session,
        } => {
            validate_space_change_protocol(schema_version)?;
            let session = resolve_trusted_space_session(pool, session).await?;
            space_turn_policy::context(pool, &session).await
        }
        IntakeRequest::SpaceTurnCapabilityAuthorize {
            schema_version,
            capability_key,
            session,
        } => {
            validate_space_change_protocol(schema_version)?;
            let session = resolve_trusted_space_session(pool, session).await?;
            space_turn_policy::authorize(pool, &session, &capability_key).await
        }
        IntakeRequest::PosterProductionRequest {
            schema_version,
            request,
            priority,
            activity_record_ref,
            activity_facts,
            session,
            idempotency_key,
        } => {
            validate_protocol(schema_version, authenticated_ingress_enabled)?;
            let session = resolve_session(
                pool,
                schema_version,
                session,
                POSTER_PRODUCTION_CAPABILITY,
                internal_group_enabled,
            )
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
            let source_type = session_source_type(&session, false);
            let intake_channel = session_intake_channel(&session);
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
                    source_type: source_type.to_string(),
                    source_refs,
                    human_owner: actor_ref.clone(),
                    priority,
                    idempotency_key: expected_key,
                    metadata: json!({
                        "intake_channel": intake_channel,
                        "origin_conversation_ref": origin_ref,
                        "poster_fact_gate": fact_assessment.metadata(),
                        "generation_authorization": {
                            "mode": "originating_generation_request",
                            "actor_ref": actor_ref,
                            "source_message_ref": source_message_ref,
                            "conversation_type": session.conversation_type,
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
            snapshot_workflow_participants(pool, workflow_root_id, &session).await?;
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
                "conversation_type": session.conversation_type,
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
            validate_protocol(schema_version, authenticated_ingress_enabled)?;
            let session = resolve_session(
                pool,
                schema_version,
                session,
                POSTER_STATUS_CAPABILITY,
                internal_group_enabled,
            )
            .await?;
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
            validate_protocol(schema_version, authenticated_ingress_enabled)?;
            let session = resolve_session(
                pool,
                schema_version,
                session,
                POSTER_PRODUCTION_CAPABILITY,
                internal_group_enabled,
            )
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
            )
            .await
        }
    }
}

fn validate_space_change_protocol(schema_version: u8) -> Result<()> {
    if schema_version != SPACE_CHANGE_PROTOCOL_VERSION {
        bail!("unsupported Space configuration intake schema version");
    }
    Ok(())
}

fn trusted_space_session(session: TrustedSession) -> TrustedSpaceSession {
    TrustedSpaceSession {
        platform: session.platform,
        conversation_type: session.conversation_type,
        conversation_id: session.conversation_id,
        requester_user_id: session.requester_user_id,
        source_message_id: session.source_message_id,
        source_message_text: None,
    }
}

async fn resolve_trusted_space_session(
    pool: &PgPool,
    session: TrustedSession,
) -> Result<TrustedSpaceSession> {
    let mut session = trusted_space_session(session);
    if session.platform.trim() != "qiwe"
        || session.conversation_type.trim() != "group"
        || [
            (session.conversation_id.as_str(), 200usize),
            (session.requester_user_id.as_str(), 200usize),
            (session.source_message_id.as_str(), 240usize),
        ]
        .iter()
        .any(|(value, max_len)| {
            value.is_empty()
                || value.len() > *max_len
                || value.chars().any(char::is_whitespace)
                || value.chars().any(char::is_control)
        })
    {
        bail!("trusted current QiWe group session is required");
    }

    for attempt in 0..SPACE_SESSION_RECEIPT_LOOKUP_ATTEMPTS {
        let source_message_text: Option<String> = sqlx::query_scalar::<_, String>(
            r#"
            SELECT message.text
            FROM qintopia_messages.messages message
            JOIN qintopia_messages.conversations conversation
              ON conversation.id = message.conversation_id
             AND conversation.tenant_id = 'qintopia'
             AND conversation.platform = 'qiwe'
             AND conversation.chat_id = $1
             AND conversation.chat_type = 'group'
             AND conversation.status = 'active'
            JOIN qintopia_messages.raw_events raw_event
              ON raw_event.id = message.raw_event_id
             AND raw_event.source = 'qiwe'
             AND raw_event.space_id = conversation.id
             AND raw_event.ingress_auth_verified
            WHERE message.platform = 'qiwe'
              AND message.chat_type = 'group'
              AND message.chat_id = $1
              AND message.sender_id = $2
              AND message.message_id = $3
              AND message.should_trigger
              AND NULLIF(btrim(message.text), '') IS NOT NULL
              AND message.received_at >= now()
                  - make_interval(mins => $4)
              AND message.received_at <= now() + interval '1 minute'
            LIMIT 1
            "#,
        )
        .bind(&session.conversation_id)
        .bind(&session.requester_user_id)
        .bind(&session.source_message_id)
        .bind(SPACE_SESSION_RECEIPT_MAX_AGE_MINUTES as i32)
        .fetch_optional(pool)
        .await
        .context("verify trusted QiWe Space session receipt")?;
        if let Some(source_message_text) = source_message_text {
            session.source_message_text = Some(source_message_text);
            return Ok(session);
        }
        if attempt + 1 < SPACE_SESSION_RECEIPT_LOOKUP_ATTEMPTS {
            tokio::time::sleep(Duration::from_millis(50)).await;
        }
    }

    bail!("trusted QiWe source message receipt was not found")
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
                  AND NOT EXISTS (
                      SELECT 1
                      FROM qintopia_agent_os.poster_revision_requests revision
                      WHERE revision.source_artifact_id = artifact.id
                        AND revision.status IN ('accepted', 'queued', 'completed')
                  )
            ) AS needs_clarification,
            EXISTS (
                SELECT 1 FROM qintopia_agent_os.poster_notifications notification
                JOIN qintopia_agent_os.artifacts artifact
                  ON artifact.id = notification.generated_image_artifact_id
                WHERE notification.workflow_root_id = $1
                  AND notification.notification_kind = 'image_ready'
                  AND notification.status = 'delivered'
                  AND artifact.review_status = 'pending'
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
          AND sender_id = $3
          AND chat_type = $4
          AND (
              $5::bigint = 0
              OR (
                  sender_type = 'user'
                  AND should_trigger
                  AND (chat_type = 'direct' OR is_mention_bot)
              )
          )
          AND NULLIF(btrim(text), '') IS NOT NULL
        LIMIT 1
        "#,
    )
    .bind(&session.source_message_id)
    .bind(&session.conversation_id)
    .bind(&session.requester_user_id)
    .bind(&session.conversation_type)
    .bind(session.policy_version)
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
) -> Result<Value> {
    authorize_workflow_mutation(pool, workflow_root_id, session, "poster_revision_request").await?;
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
          AND root.metadata #>> '{workflow_metadata,intake_channel}' IN (
              'xiaoman_feishu_direct',
              'xiaoman_feishu_internal_group'
          )
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
    if let Some(existing) =
        load_existing_revision(pool, workflow_root_id, revision_of_artifact_id).await?
    {
        record_poster_mutation_noop(
            pool,
            workflow_root_id,
            "poster_revision_duplicate_rejected",
            &source_message_ref(session),
            &actor_ref,
            "first_revision_instruction_already_accepted",
        )
        .await?;
        return revision_response(pool, workflow_root_id, visual_work_item_id, existing, true)
            .await;
    }
    let message_ref = source_message_ref(session);
    let revision_hash = digest(&[
        "poster-revision-v1",
        &revision_of_artifact_id.to_string(),
        &instruction,
        &message_ref,
    ]);
    let prompt_hash = digest(&[&brief_hash, &revision_hash]);
    let report = operations::create_work_item_routed(
        pool,
        WorkItemCreateRequest {
            requester_agent: "xiaoman".to_string(),
            target_agent: String::new(),
            capability_key: "huabaosi.generate_image_asset".to_string(),
            work_item_type: "image_generation_request".to_string(),
            brief_summary: "根据原发起人的明确修改意见生成下一版活动海报".to_string(),
            purpose: "activity_image_revision_request".to_string(),
            human_owner: human_owner.clone(),
            priority: row.try_get("priority")?,
            source_type: session_source_type(session, true).to_string(),
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
                "revision_actor_ref": actor_ref,
                "revision_source_message_ref": message_ref,
                "external_publish_executed": false,
                "group_send_authorized": false
            }),
            payload_redaction_policy: "summary_only".to_string(),
            idempotency_key: operations::poster_revision_idempotency_key(revision_of_artifact_id),
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
             instruction_text, image_generation_work_item_id, status,
             first_revision_guarded)
        SELECT $1, $2,
               image.source_refs->>'source_message_ref',
               image.payload->>'revision_actor_ref',
               image.payload->>'revision_instruction',
               image.id,
               'queued',
               true
        FROM qintopia_agent_os.work_items image
        WHERE image.id = $3
          AND image.parent_work_item_id = $4
          AND image.work_item_type = 'image_generation_request'
          AND image.capability_key = 'huabaosi.generate_image_asset'
          AND image.payload->>'revision_of_artifact_id' = $2::text
          AND image.source_refs->>'source_message_ref'
              = image.payload->>'revision_source_message_ref'
          AND image.payload->>'revision_actor_ref' ~ '^sha256:[0-9a-f]{64}$'
          AND NULLIF(btrim(image.payload->>'revision_instruction'), '') IS NOT NULL
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(workflow_root_id)
    .bind(revision_of_artifact_id)
    .bind(image_generation_work_item_id)
    .bind(visual_work_item_id)
    .execute(pool)
    .await
    .context("record poster revision request")?;
    let winner = load_existing_revision(pool, workflow_root_id, revision_of_artifact_id)
        .await?
        .context("poster revision winner is missing after routed work creation")?;
    record_generation_authorization(
        pool,
        workflow_root_id,
        &winner.actor_ref,
        &winner.source_message_ref,
    )
    .await?;
    let deduped =
        report.existing || image_generation_work_item_id != winner.image_generation_work_item_id;
    revision_response(pool, workflow_root_id, visual_work_item_id, winner, deduped).await
}

async fn load_existing_revision(
    pool: &PgPool,
    workflow_root_id: Uuid,
    source_artifact_id: Uuid,
) -> Result<Option<RevisionWinner>> {
    let row = sqlx::query(
        r#"
        SELECT source_message_ref, actor_ref, image_generation_work_item_id
        FROM qintopia_agent_os.poster_revision_requests
        WHERE workflow_root_id = $1 AND source_artifact_id = $2
        ORDER BY created_at, id
        LIMIT 1
        "#,
    )
    .bind(workflow_root_id)
    .bind(source_artifact_id)
    .fetch_optional(pool)
    .await
    .context("load first accepted poster revision")?;
    row.map(|row| {
        Ok(RevisionWinner {
            source_message_ref: row.try_get("source_message_ref")?,
            actor_ref: row.try_get("actor_ref")?,
            image_generation_work_item_id: row.try_get("image_generation_work_item_id")?,
        })
    })
    .transpose()
}

async fn revision_response(
    pool: &PgPool,
    workflow_root_id: Uuid,
    visual_work_item_id: Uuid,
    winner: RevisionWinner,
    deduped: bool,
) -> Result<Value> {
    Ok(json!({
        "success": true,
        "accepted": true,
        "deduped": deduped,
        "workflow_root_id": workflow_root_id,
        "visual_work_item_id": visual_work_item_id,
        "image_generation_work_item_id": winner.image_generation_work_item_id,
        "workflow_status": poster_user_status(pool, workflow_root_id).await?,
        "external_send_executed": false,
        "group_send_authorized": false
    }))
}

async fn snapshot_workflow_participants(
    pool: &PgPool,
    workflow_root_id: Uuid,
    session: &TrustedSession,
) -> Result<()> {
    let Some(binding) = session.binding.as_ref() else {
        return Ok(());
    };
    let requester_ref = actor_ref(session);
    let mut tx = pool
        .begin()
        .await
        .context("begin poster participant snapshot")?;
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.poster_workflow_participants
            (workflow_root_id, actor_ref, participant_role, conversation_ref,
             policy_id, policy_version)
        VALUES ($1, $2, 'requester', $3, $4, $5)
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(workflow_root_id)
    .bind(&requester_ref)
    .bind(&binding.conversation_ref)
    .bind(binding.policy_id)
    .bind(session.policy_version)
    .execute(&mut *tx)
    .await
    .context("snapshot poster requester")?;
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.poster_workflow_participants
            (workflow_root_id, actor_ref, participant_role, conversation_ref,
             policy_id, policy_version)
        SELECT $1, policy_actor.actor_ref, 'reviewer', $2, $3, $4
        FROM qintopia_agent_os.conversation_policy_actors policy_actor
        WHERE policy_actor.policy_id = $3
          AND policy_actor.actor_role = 'reviewer'
        ON CONFLICT DO NOTHING
        "#,
    )
    .bind(workflow_root_id)
    .bind(&binding.conversation_ref)
    .bind(binding.policy_id)
    .bind(session.policy_version)
    .execute(&mut *tx)
    .await
    .context("snapshot poster reviewers")?;
    let counts: (i64, i64, i64, i64) = sqlx::query_as(
        r#"
        SELECT
            count(*) FILTER (WHERE participant.participant_role = 'requester'),
            count(*) FILTER (WHERE participant.participant_role = 'reviewer'),
            (
                SELECT count(*)
                FROM qintopia_agent_os.conversation_policy_actors policy_actor
                WHERE policy_actor.policy_id = $2
                  AND policy_actor.actor_role = 'reviewer'
            ),
            count(*) FILTER (
                WHERE participant.conversation_ref <> $3
                   OR participant.policy_id <> $2
                   OR participant.policy_version <> $4
            )
        FROM qintopia_agent_os.poster_workflow_participants participant
        WHERE participant.workflow_root_id = $1
        "#,
    )
    .bind(workflow_root_id)
    .bind(binding.policy_id)
    .bind(&binding.conversation_ref)
    .bind(session.policy_version)
    .fetch_one(&mut *tx)
    .await
    .context("verify poster participant snapshot")?;
    if counts.0 != 1 || counts.1 != counts.2 || counts.3 != 0 {
        bail!("poster workflow participant snapshot conflicts with its policy version");
    }
    tx.commit()
        .await
        .context("commit poster participant snapshot")?;
    Ok(())
}

fn session_source_type(session: &TrustedSession, revision: bool) -> &'static str {
    match (session.conversation_type.as_str(), revision) {
        ("group", false) => "feishu_internal_group_request",
        ("group", true) => "feishu_internal_group_revision_request",
        ("direct", true) => "feishu_direct_revision_request",
        _ => "feishu_direct_request",
    }
}

fn session_intake_channel(session: &TrustedSession) -> &'static str {
    if session.conversation_type == "group" {
        "xiaoman_feishu_internal_group"
    } else {
        "xiaoman_feishu_direct"
    }
}

async fn upsert_return_target(
    pool: &PgPool,
    origin_ref: &str,
    session: &TrustedSession,
) -> Result<()> {
    let (conversation_ref, audience_class, delivery_mode, thread_root_message_id) =
        match session.binding.as_ref() {
            Some(binding) => (
                binding.conversation_ref.clone(),
                binding.audience_class.as_str(),
                binding.delivery_mode.as_str(),
                binding.thread_root_message_id.as_deref(),
            ),
            None => (origin_ref.to_string(), "private", "direct_chat", None),
        };
    let result = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.poster_return_targets
            (origin_ref, platform, conversation_type, conversation_id,
             requester_user_id, source_message_id, audience_class,
             conversation_ref, policy_version, delivery_mode, thread_root_message_id)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
        ON CONFLICT (origin_ref) DO UPDATE SET
            source_message_id = CASE
                WHEN EXCLUDED.policy_version = 0 THEN EXCLUDED.source_message_id
                ELSE qintopia_agent_os.poster_return_targets.source_message_id
            END,
            updated_at = now()
        WHERE qintopia_agent_os.poster_return_targets.platform = EXCLUDED.platform
          AND qintopia_agent_os.poster_return_targets.conversation_type = EXCLUDED.conversation_type
          AND qintopia_agent_os.poster_return_targets.conversation_id = EXCLUDED.conversation_id
          AND qintopia_agent_os.poster_return_targets.requester_user_id = EXCLUDED.requester_user_id
          AND (
              EXCLUDED.policy_version = 0
              OR qintopia_agent_os.poster_return_targets.source_message_id = EXCLUDED.source_message_id
          )
          AND qintopia_agent_os.poster_return_targets.audience_class = EXCLUDED.audience_class
          AND qintopia_agent_os.poster_return_targets.conversation_ref = EXCLUDED.conversation_ref
          AND qintopia_agent_os.poster_return_targets.policy_version = EXCLUDED.policy_version
          AND qintopia_agent_os.poster_return_targets.delivery_mode = EXCLUDED.delivery_mode
          AND qintopia_agent_os.poster_return_targets.thread_root_message_id
              IS NOT DISTINCT FROM EXCLUDED.thread_root_message_id
        "#,
    )
    .bind(origin_ref)
    .bind(&session.platform)
    .bind(&session.conversation_type)
    .bind(&session.conversation_id)
    .bind(&session.requester_user_id)
    .bind(&session.source_message_id)
    .bind(audience_class)
    .bind(conversation_ref)
    .bind(session.policy_version)
    .bind(delivery_mode)
    .bind(thread_root_message_id)
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
               'originating trusted conversation request authorized poster generation',
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
    let access = load_workflow_access(pool, workflow_root_id).await?;
    let allowed = workflow_conversation_matches(&access, session)
        && match session.binding.as_ref() {
            None => {
                access.policy_version == 0
                    && access.conversation_type == "direct"
                    && access.requester_user_id == session.requester_user_id
            }
            Some(binding) if access.conversation_type == "direct" => {
                binding.status_visibility == "requester"
                    && access.requester_user_id == session.requester_user_id
            }
            Some(binding) if access.conversation_type == "group" => {
                binding.status_visibility == "conversation_members"
            }
            Some(_) => false,
        };
    if !allowed {
        bail!("workflow is not visible to the current trusted conversation");
    }
    Ok(())
}

async fn authorize_workflow_mutation(
    pool: &PgPool,
    workflow_root_id: Uuid,
    session: &TrustedSession,
    mutation_type: &str,
) -> Result<()> {
    let access = load_workflow_access(pool, workflow_root_id).await?;
    let conversation_matches = workflow_conversation_matches(&access, session);
    let actor_ref = actor_ref(session);
    let allowed = if !conversation_matches {
        false
    } else if session.binding.is_none() {
        access.policy_version == 0
            && access.conversation_type == "direct"
            && access.requester_user_id == session.requester_user_id
    } else {
        let roles: &[&str] = if access.conversation_type == "group" {
            &["requester", "reviewer"]
        } else {
            &["requester"]
        };
        let participant: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM qintopia_agent_os.poster_workflow_participants participant
                WHERE participant.workflow_root_id = $1
                  AND participant.actor_ref = $2
                  AND participant.conversation_ref = $3
                  AND participant.policy_version = $4
                  AND participant.participant_role = ANY($5::text[])
            )
            "#,
        )
        .bind(workflow_root_id)
        .bind(&actor_ref)
        .bind(&access.conversation_ref)
        .bind(access.policy_version)
        .bind(roles)
        .fetch_one(pool)
        .await
        .context("authorize poster workflow participant mutation")?;
        participant
            && (access.conversation_type != "group"
                || access.thread_root_message_id
                    == session
                        .binding
                        .as_ref()
                        .and_then(|binding| binding.thread_root_message_id.clone()))
    };
    if !allowed {
        record_poster_mutation_noop(
            pool,
            workflow_root_id,
            "poster_mutation_rejected",
            &source_message_ref(session),
            &actor_ref,
            "actor_or_conversation_not_authorized",
        )
        .await?;
        bail!("poster workflow mutation is not authorized");
    }
    if !matches!(
        mutation_type,
        "poster_revision_request" | "poster_review_decision"
    ) {
        bail!("poster mutation type is invalid");
    }
    Ok(())
}

pub(crate) async fn authorize_poster_review_actor(
    pool: &PgPool,
    workflow_root_id: Uuid,
    conversation_id: &str,
    actor_user_id: &str,
    callback_event_ref: &str,
    audit_rejection: bool,
) -> Result<PosterReviewActor> {
    let access = load_workflow_access(pool, workflow_root_id).await?;
    let actor_ref = crate::conversation_policy::actor_ref("feishu", actor_user_id);
    let conversation_matches = access.conversation_id == conversation_id
        && matches!(
            (
                access.conversation_type.as_str(),
                access.delivery_mode.as_str()
            ),
            ("direct", "direct_chat") | ("group", "thread_reply")
        );
    let allowed = if !conversation_matches {
        false
    } else if access.policy_version == 0 {
        access.conversation_type == "direct" && access.requester_user_id == actor_user_id
    } else {
        let roles: &[&str] = if access.conversation_type == "group" {
            &["requester", "reviewer"]
        } else {
            &["requester"]
        };
        sqlx::query_scalar(
            r#"
            SELECT EXISTS (
                SELECT 1
                FROM qintopia_agent_os.poster_workflow_participants participant
                WHERE participant.workflow_root_id = $1
                  AND participant.actor_ref = $2
                  AND participant.conversation_ref = $3
                  AND participant.policy_version = $4
                  AND participant.participant_role = ANY($5::text[])
            )
            "#,
        )
        .bind(workflow_root_id)
        .bind(&actor_ref)
        .bind(&access.conversation_ref)
        .bind(access.policy_version)
        .bind(roles)
        .fetch_one(pool)
        .await
        .context("authorize poster review callback participant")?
    };
    if !allowed {
        if audit_rejection {
            record_poster_mutation_noop(
                pool,
                workflow_root_id,
                "poster_mutation_rejected",
                callback_event_ref,
                &actor_ref,
                "review_actor_or_conversation_not_authorized",
            )
            .await?;
        }
        bail!("poster review actor is not authorized for this workflow");
    }
    Ok(PosterReviewActor {
        actor_ref,
        policy_version: access.policy_version,
    })
}

async fn load_workflow_access(pool: &PgPool, workflow_root_id: Uuid) -> Result<WorkflowAccess> {
    let row = sqlx::query(
        r#"
        SELECT target.conversation_type, target.conversation_id,
               target.requester_user_id, target.conversation_ref,
               target.policy_version, target.delivery_mode,
               target.thread_root_message_id
        FROM qintopia_agent_os.work_items root
        JOIN qintopia_agent_os.poster_return_targets target
          ON target.origin_ref = root.metadata #>> '{workflow_metadata,origin_conversation_ref}'
        WHERE root.id = $1
          AND root.parent_work_item_id IS NULL
          AND target.platform = 'feishu'
        "#,
    )
    .bind(workflow_root_id)
    .fetch_optional(pool)
    .await
    .context("load poster workflow conversation access")?
    .context("poster workflow return target is unavailable")?;
    Ok(WorkflowAccess {
        conversation_type: row.try_get("conversation_type")?,
        conversation_id: row.try_get("conversation_id")?,
        requester_user_id: row.try_get("requester_user_id")?,
        conversation_ref: row.try_get("conversation_ref")?,
        policy_version: row.try_get("policy_version")?,
        delivery_mode: row.try_get("delivery_mode")?,
        thread_root_message_id: row.try_get("thread_root_message_id")?,
    })
}

fn workflow_conversation_matches(access: &WorkflowAccess, session: &TrustedSession) -> bool {
    if access.conversation_type != session.conversation_type
        || access.conversation_id != session.conversation_id
    {
        return false;
    }
    match session.binding.as_ref() {
        None => {
            access.policy_version == 0
                && access.conversation_type == "direct"
                && access.delivery_mode == "direct_chat"
        }
        Some(binding) => {
            access.policy_version > 0
                && access.conversation_ref == binding.conversation_ref
                && access.delivery_mode == binding.delivery_mode
        }
    }
}

pub(crate) async fn record_poster_mutation_noop(
    pool: &PgPool,
    workflow_root_id: Uuid,
    event_type: &str,
    source_ref: &str,
    actor_ref: &str,
    reason_code: &str,
) -> Result<()> {
    if !valid_opaque_ref(source_ref)
        || !valid_opaque_ref(actor_ref)
        || !matches!(
            event_type,
            "poster_mutation_rejected"
                | "poster_revision_duplicate_rejected"
                | "poster_review_callback_duplicate_rejected"
                | "poster_review_callback_conflict_rejected"
        )
    {
        bail!("poster mutation audit identity is invalid");
    }
    let mutation_ref = digest(&[
        "poster-mutation-noop-v3",
        event_type,
        source_ref,
        actor_ref,
        reason_code,
    ]);
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, event_type, actor_type, actor_id, message, data)
        SELECT $1, $2, 'human', $3,
               'poster mutation was rejected without changing workflow state',
               jsonb_build_object(
                   'mutation_ref', $4::text,
                   'source_ref', $5::text,
                   'reason_code', $6::text,
                   'mutation_applied', false,
                   'group_send_authorized', false,
                   'external_send_executed', false
               )
        WHERE NOT EXISTS (
            SELECT 1
            FROM qintopia_agent_os.work_item_events event
            WHERE event.work_item_id = $1
              AND event.event_type = $2
              AND event.data->>'mutation_ref' = $4
        )
        "#,
    )
    .bind(workflow_root_id)
    .bind(event_type)
    .bind(actor_ref)
    .bind(mutation_ref)
    .bind(source_ref)
    .bind(reason_code)
    .execute(pool)
    .await
    .context("record rejected poster mutation audit")?;
    Ok(())
}

fn validate_protocol(version: u8, authenticated_ingress_enabled: bool) -> Result<()> {
    match (authenticated_ingress_enabled, version) {
        (false, LEGACY_PROTOCOL_VERSION) | (true, PROTOCOL_VERSION) => Ok(()),
        (true, LEGACY_PROTOCOL_VERSION) => {
            bail!("legacy poster intake is disabled after authenticated ingress cutover")
        }
        (false, PROTOCOL_VERSION) => {
            bail!("authenticated poster intake is disabled before ingress cutover")
        }
        _ => bail!("unsupported intake protocol version"),
    }
}

async fn resolve_session(
    pool: &PgPool,
    schema_version: u8,
    mut session: TrustedSession,
    required_capability: &str,
    internal_group_enabled: bool,
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
        session.binding = None;
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
        SELECT message.chat_type, message.thread_root_message_id,
               policy.id AS policy_id, receipt.conversation_ref,
               receipt.policy_version, policy.audience_class,
               policy.return_mode, policy.status_visibility
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
          AND message.should_trigger
          AND policy.platform = 'feishu'
          AND policy.conversation_ref = receipt.conversation_ref
          AND policy.conversation_type = message.chat_type
          AND $6 = ANY(policy.allowed_capabilities)
          AND (
              (
                  message.chat_type = 'direct'
                  AND NOT message.is_mention_bot
                  AND policy.audience_class = 'private'
                  AND policy.return_mode = 'direct_chat'
                  AND policy.initiation_rule = 'direct_message'
                  AND policy.status_visibility = 'requester'
              )
              OR
              (
                  $7::boolean
                  AND message.chat_type = 'group'
                  AND message.is_mention_bot
                  AND NULLIF(btrim(message.thread_root_message_id), '') IS NOT NULL
                  AND policy.audience_class = 'internal_collaboration'
                  AND policy.return_mode = 'thread_reply'
                  AND policy.initiation_rule = 'explicit_bot_mention'
                  AND policy.status_visibility = 'conversation_members'
              )
          )
        LIMIT 1
        "#,
    )
    .bind(expected_message_ref)
    .bind(expected_conversation_ref)
    .bind(&session.source_message_id)
    .bind(&session.conversation_id)
    .bind(&session.requester_user_id)
    .bind(required_capability)
    .bind(internal_group_enabled)
    .fetch_optional(pool)
    .await
    .context("resolve authenticated V3 poster session")?
    .context("authenticated V3 direct-message policy binding is unavailable")?;
    session.conversation_type = row.try_get("chat_type")?;
    session.policy_version = row.try_get("policy_version")?;
    let thread_root_message_id = if session.conversation_type == "group" {
        row.try_get("thread_root_message_id")?
    } else {
        None
    };
    session.binding = Some(ConversationBinding {
        policy_id: row.try_get("policy_id")?,
        conversation_ref: row.try_get("conversation_ref")?,
        audience_class: row.try_get("audience_class")?,
        delivery_mode: row.try_get("return_mode")?,
        status_visibility: row.try_get("status_visibility")?,
        thread_root_message_id,
    });
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
    if session.platform != "feishu"
        || !matches!(session.conversation_type.as_str(), "direct" | "group")
        || (session.conversation_type == "group" && session.binding.is_none())
    {
        bail!("trusted Feishu conversation session is required");
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
    crate::conversation_policy::actor_ref(&session.platform, &session.requester_user_id)
}

fn source_message_ref(session: &TrustedSession) -> String {
    crate::conversation_policy::source_message_ref(&session.platform, &session.source_message_id)
}

fn source_idempotency_key(session: &TrustedSession) -> String {
    if session.policy_version > 0 {
        return format!(
            "poster_production_request:{}",
            digest(&[
                "poster-intake-v3",
                &session.platform,
                &session.source_message_id,
                POSTER_PRODUCTION_CAPABILITY,
            ])
        );
    }
    format!(
        "poster_production_request:{}",
        digest(&[&session.platform, &session.source_message_id])
    )
}

fn valid_opaque_ref(value: &str) -> bool {
    value.len() == 71
        && value.starts_with("sha256:")
        && value[7..]
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
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
        binding: None,
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
        true,
        true,
    )
    .await
}

#[cfg(test)]
mod tests {
    use super::*;
    #[cfg(feature = "postgres-integration-tests")]
    use crate::poster_notification::ReviewCallbackIntegrationInput;

    #[test]
    fn shared_intake_socket_rejects_agent_turn_broker_operations() {
        for operation in [
            "space_agent_turn_claim",
            "space_agent_turn_finish",
            "space_agent_turn_status",
        ] {
            let error = parse_wire_request(json!({
                "operation": operation,
                "schema_version": 1
            }))
            .expect_err("shared intake must not expose Agent-turn runner operations");
            assert!(error
                .to_string()
                .contains("legacy operations intake request"));
        }
    }

    #[test]
    fn ordinary_space_turn_policy_wire_contract_uses_only_trusted_session_scope() {
        let context = serde_json::from_value::<IntakeRequest>(json!({
            "operation": "space_turn_policy_context",
            "schema_version": 1,
            "session": {
                "platform": "qiwe",
                "conversation_type": "group",
                "conversation_id": "trusted-room",
                "requester_user_id": "trusted-user",
                "source_message_id": "trusted-message"
            }
        }))
        .expect("parse Space turn policy context");
        assert!(matches!(
            context,
            IntakeRequest::SpaceTurnPolicyContext {
                schema_version: 1,
                ..
            }
        ));

        let authorization = serde_json::from_value::<IntakeRequest>(json!({
            "operation": "space_turn_capability_authorize",
            "schema_version": 1,
            "capability_key": "erhua.qiwe_send_location_card",
            "session": {
                "platform": "qiwe",
                "conversation_type": "group",
                "conversation_id": "trusted-room",
                "requester_user_id": "trusted-user",
                "source_message_id": "trusted-message"
            }
        }))
        .expect("parse Space turn capability authorization");
        let IntakeRequest::SpaceTurnCapabilityAuthorize { capability_key, .. } = authorization
        else {
            panic!("expected Space turn capability authorization");
        };
        assert_eq!(capability_key, "erhua.qiwe_send_location_card");
    }

    #[test]
    fn programming_extension_wire_contract_is_bounded() {
        let request: IntakeRequest = serde_json::from_value(json!({
            "operation": "space_programming_extension_prepare",
            "schema_version": 1,
            "request": {
                "intent": "Handle one unknown QiWe provider event.",
                "provider": "qiwe",
                "research_query": "unknown QiWe event",
                "official_sources": ["https://doc.qiweapi.com/doc-7331304"],
                "research_digest": "a".repeat(64)
            },
            "session": {
                "platform": "qiwe",
                "conversation_type": "group",
                "conversation_id": "trusted-room",
                "requester_user_id": "trusted-user",
                "source_message_id": "trusted-message"
            }
        }))
        .expect("bounded programming extension intake request");
        let IntakeRequest::SpaceProgrammingExtensionPrepare { request, .. } = request else {
            panic!("expected programming extension request");
        };
        assert_eq!(request.provider, "qiwe");

        let forged = serde_json::from_value::<IntakeRequest>(json!({
            "operation": "space_programming_extension_prepare",
            "schema_version": 1,
            "request": {
                "intent": "Handle one unknown QiWe provider event.",
                "provider": "qiwe",
                "research_query": "unknown QiWe event",
                "official_sources": ["https://doc.qiweapi.com/doc-7331304"],
                "research_digest": "a".repeat(64),
                "target_group_id": "forged-room"
            },
            "session": {
                "platform": "qiwe",
                "conversation_type": "group",
                "conversation_id": "trusted-room",
                "requester_user_id": "trusted-user",
                "source_message_id": "trusted-message"
            }
        }));
        assert!(forged.is_err());
    }

    #[test]
    fn programming_extension_shadow_continuation_wire_contract_is_internal_and_scoped() {
        let request_id = Uuid::new_v4();
        let session = json!({
            "platform": "qiwe",
            "conversation_type": "group",
            "conversation_id": "trusted-room",
            "requester_user_id": "trusted-user",
            "source_message_id": "trusted-message"
        });
        let continuation = serde_json::from_value::<IntakeRequest>(json!({
            "operation": "space_programming_extension_continuation_intent",
            "schema_version": 1,
            "request_id": request_id,
            "session": session.clone()
        }))
        .expect("parse internal continuation intent request");
        assert!(matches!(
            continuation,
            IntakeRequest::SpaceProgrammingExtensionContinuationIntent {
                schema_version: 1,
                request_id: parsed_id,
                ..
            } if parsed_id == request_id
        ));

        let shadow = serde_json::from_value::<IntakeRequest>(json!({
            "operation": "space_programming_extension_shadow_prepare",
            "schema_version": 1,
            "request_id": request_id,
            "intent": {"summary": "shadow", "changes": []},
            "session": session
        }))
        .expect("parse internal shadow prepare request");
        assert!(matches!(
            shadow,
            IntakeRequest::SpaceProgrammingExtensionShadowPrepare {
                schema_version: 1,
                request_id: parsed_id,
                ..
            } if parsed_id == request_id
        ));
    }

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
            binding: None,
        }
    }

    fn v3_group_session() -> TrustedSession {
        TrustedSession {
            platform: "feishu".to_string(),
            conversation_type: "group".to_string(),
            conversation_id: "oc_group_fixture".to_string(),
            requester_user_id: "ou_group_requester_fixture".to_string(),
            source_message_id: "om_group_message_fixture".to_string(),
            policy_version: 3,
            binding: Some(ConversationBinding {
                policy_id: Uuid::new_v4(),
                conversation_ref: format!("sha256:{}", "a".repeat(64)),
                audience_class: "internal_collaboration".to_string(),
                delivery_mode: "thread_reply".to_string(),
                status_visibility: "conversation_members".to_string(),
                thread_root_message_id: Some("om_group_root_fixture".to_string()),
            }),
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
    fn only_policy_resolved_group_and_feishu_sessions_are_accepted() {
        let mut grouped = session();
        grouped.conversation_type = "group".to_string();
        assert!(validate_session(&grouped).is_err());
        assert!(validate_session(&v3_group_session()).is_ok());
        let mut wecom = session();
        wecom.platform = "wecom".to_string();
        assert!(validate_session(&wecom).is_err());
    }

    #[test]
    fn v3_workflow_access_is_scoped_to_the_originating_conversation() {
        let session = v3_group_session();
        let binding = session.binding.as_ref().unwrap();
        let access = WorkflowAccess {
            conversation_type: session.conversation_type.clone(),
            conversation_id: session.conversation_id.clone(),
            requester_user_id: session.requester_user_id.clone(),
            conversation_ref: binding.conversation_ref.clone(),
            policy_version: session.policy_version,
            delivery_mode: binding.delivery_mode.clone(),
            thread_root_message_id: binding.thread_root_message_id.clone(),
        };
        assert!(workflow_conversation_matches(&access, &session));

        let mut other_conversation = session.clone();
        other_conversation.conversation_id = "oc_other_group_fixture".to_string();
        other_conversation
            .binding
            .as_mut()
            .unwrap()
            .conversation_ref = format!("sha256:{}", "b".repeat(64));
        assert!(!workflow_conversation_matches(&access, &other_conversation));

        let mut other_delivery_mode = session;
        other_delivery_mode.binding.as_mut().unwrap().delivery_mode = "direct_chat".to_string();
        assert!(!workflow_conversation_matches(
            &access,
            &other_delivery_mode
        ));
    }

    #[test]
    fn protocol_cutover_never_downgrades_between_v2_and_v3() {
        assert!(validate_protocol(LEGACY_PROTOCOL_VERSION, false).is_ok());
        assert!(validate_protocol(PROTOCOL_VERSION, true).is_ok());
        assert!(validate_protocol(LEGACY_PROTOCOL_VERSION, true).is_err());
        assert!(validate_protocol(PROTOCOL_VERSION, false).is_err());
        assert!(validate_protocol(1, false).is_err());
        assert!(validate_protocol(4, true).is_err());
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

        let mut v3 = session();
        v3.policy_version = 1;
        assert_ne!(
            source_idempotency_key(&session()),
            source_idempotency_key(&v3)
        );
        assert_eq!(
            session_source_type(&v3_group_session(), false),
            "feishu_internal_group_request"
        );
        assert_eq!(
            session_source_type(&v3_group_session(), true),
            "feishu_internal_group_revision_request"
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
            binding: None,
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
            false,
            false,
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

        let first = handle_request(&pool, &policy, request(), false, false)
            .await
            .expect("accept first poster request");
        let duplicate = handle_request(&pool, &policy, request(), false, false)
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
        let dry_run_events_before: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM qintopia_agent_os.work_item_events
            WHERE work_item_id=$1
              AND event_type IN (
                  'poster_mutation_rejected',
                  'poster_review_callback_duplicate_rejected',
                  'poster_review_callback_conflict_rejected'
              )
            "#,
        )
        .bind(root_id)
        .fetch_one(&pool)
        .await
        .expect("count poster mutation audits before callback dry-run");
        let dry_run_callback_event_id = format!("evt_dry_run_{suffix}");
        let dry_run = crate::poster_notification::preview_review_callback_for_postgres_integration(
            &pool,
            &database_url,
            ReviewCallbackIntegrationInput {
                callback_event_id: &dry_run_callback_event_id,
                notification_id,
                artifact_id,
                conversation_id: &session.conversation_id,
                actor_user_id: &session.requester_user_id,
                action: "approve",
            },
        )
        .await
        .expect("preview valid poster review callback without mutation");
        assert!(!dry_run);
        let dry_run_wrong_actor =
            crate::poster_notification::preview_review_callback_for_postgres_integration(
                &pool,
                &database_url,
                ReviewCallbackIntegrationInput {
                    callback_event_id: &format!("evt_dry_run_wrong_actor_{suffix}"),
                    notification_id,
                    artifact_id,
                    conversation_id: &session.conversation_id,
                    actor_user_id: &wrong_actor,
                    action: "approve",
                },
            )
            .await;
        assert!(
            dry_run_wrong_actor.is_err(),
            "dry-run must reject an unauthorized reviewer"
        );
        let dry_run_wrong_chat =
            crate::poster_notification::preview_review_callback_for_postgres_integration(
                &pool,
                &database_url,
                ReviewCallbackIntegrationInput {
                    callback_event_id: &format!("evt_dry_run_wrong_chat_{suffix}"),
                    notification_id,
                    artifact_id,
                    conversation_id: &format!("oc_dry_run_wrong_{suffix}"),
                    actor_user_id: &session.requester_user_id,
                    action: "approve",
                },
            )
            .await;
        assert!(
            dry_run_wrong_chat.is_err(),
            "dry-run must reject a mismatched conversation"
        );
        let dry_run_state: (String, i64, i64) = sqlx::query_as(
            r#"
            SELECT artifact.review_status,
                   count(DISTINCT action.callback_event_id),
                   count(DISTINCT event.id) FILTER (
                       WHERE event.event_type IN (
                           'poster_mutation_rejected',
                           'poster_review_callback_duplicate_rejected',
                           'poster_review_callback_conflict_rejected'
                       )
                   )
            FROM qintopia_agent_os.artifacts artifact
            LEFT JOIN qintopia_agent_os.poster_review_actions action
              ON action.artifact_id=artifact.id
            LEFT JOIN qintopia_agent_os.work_item_events event
              ON event.work_item_id=$2
            WHERE artifact.id=$1
            GROUP BY artifact.review_status
            "#,
        )
        .bind(artifact_id)
        .bind(root_id)
        .fetch_one(&pool)
        .await
        .expect("verify poster callback dry-run remains read-only");
        assert_eq!(dry_run_state.0, "pending");
        assert_eq!(dry_run_state.1, 0);
        assert_eq!(dry_run_state.2, dry_run_events_before);
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
        let first_revision = handle_request(&pool, &policy, revision_request(), false, false)
            .await
            .expect("accept explicit poster revision instruction");
        let duplicate_revision = handle_request(&pool, &policy, revision_request(), false, false)
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

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
    async fn postgres_v3_direct_snapshots_participants_and_isolates_status() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect guarded V3 direct integration database");
        db::run_migrations(&pool)
            .await
            .expect("migrate guarded V3 direct integration database");

        let suffix = Uuid::new_v4().simple().to_string();
        let chat_id = format!("oc_direct_v3_{suffix}");
        let requester_id = format!("ou_direct_requester_{suffix}");
        let reviewer_id = format!("ou_direct_reviewer_{suffix}");
        let message_id = format!("om_direct_v3_{suffix}");
        let policy_id = Uuid::new_v4();
        let conversation_ref = crate::conversation_policy::conversation_ref("feishu", &chat_id);
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.conversation_policies
                (id, platform, conversation_ref, conversation_type, audience_class,
                 allowed_capabilities, return_mode, initiation_rule, status_visibility,
                 policy_version, policy_digest, enabled)
            VALUES ($1, 'feishu', $2, 'direct', 'private',
                    ARRAY['poster_production_request','poster_workflow_status']::text[],
                    'direct_chat', 'direct_message', 'requester', 1, $3, true)
            "#,
        )
        .bind(policy_id)
        .bind(&conversation_ref)
        .bind(digest(&["direct-policy-v3", &suffix]))
        .execute(&pool)
        .await
        .expect("insert V3 direct conversation policy");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.conversation_policy_actors
                (policy_id, actor_ref, actor_role)
            VALUES ($1, $2, 'reviewer')
            "#,
        )
        .bind(policy_id)
        .bind(crate::conversation_policy::actor_ref(
            "feishu",
            &reviewer_id,
        ))
        .execute(&pool)
        .await
        .expect("insert V3 direct policy reviewer");
        let request_text = "请为 AgentOS 私聊验收生成海报，时间 2026-08-01 16:00，地点线上";
        let message_row_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_messages.messages
                (platform, message_id, event_id, chat_id, chat_type, sender_id,
                 sender_type, message_kind, text, is_mention_bot, should_trigger,
                 trigger_reason, received_at, raw)
            VALUES ('feishu', $1, $2, $3, 'direct', $4, 'user', 'text', $5,
                    false, true, 'xiaoman_authenticated_feishu_message_v3', now(),
                    '{"authenticated":true,"schema_version":3}'::jsonb)
            RETURNING id
            "#,
        )
        .bind(&message_id)
        .bind(format!("evt_direct_v3_{suffix}"))
        .bind(&chat_id)
        .bind(&requester_id)
        .bind(request_text)
        .fetch_one(&pool)
        .await
        .expect("insert authenticated V3 direct message");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.feishu_message_ingress_receipts
                (source_message_ref, message_row_id, conversation_ref, policy_id,
                 policy_version, payload_hash)
            VALUES ($1, $2, $3, $4, 1, $5)
            "#,
        )
        .bind(crate::conversation_policy::source_message_ref(
            "feishu",
            &message_id,
        ))
        .bind(message_row_id)
        .bind(&conversation_ref)
        .bind(policy_id)
        .bind(digest(&["direct-message-payload-v3", &message_id]))
        .execute(&pool)
        .await
        .expect("insert authenticated V3 direct receipt");

        let session = TrustedSession {
            platform: "feishu".to_string(),
            conversation_type: String::new(),
            conversation_id: chat_id.clone(),
            requester_user_id: requester_id.clone(),
            source_message_id: message_id.clone(),
            policy_version: 0,
            binding: None,
        };
        let request = || IntakeRequest::PosterProductionRequest {
            schema_version: PROTOCOL_VERSION,
            request: request_text.to_string(),
            priority: "normal".to_string(),
            activity_record_ref: String::new(),
            activity_facts: ActivityFacts {
                source: "originating_request".to_string(),
                title: "AgentOS 私聊验收".to_string(),
                schedule: "2026-08-01 16:00".to_string(),
                location: "线上".to_string(),
                conflict_fields: Vec::new(),
            },
            session: session.clone(),
            idempotency_key: String::new(),
        };
        let policy = OperationsPolicy::dry_run();
        let first = handle_request(&pool, &policy, request(), true, false)
            .await
            .expect("accept authenticated V3 direct poster request");
        let duplicate = handle_request(&pool, &policy, request(), true, false)
            .await
            .expect("dedupe authenticated V3 direct poster request");
        assert_eq!(first["conversation_type"], "direct");
        assert_eq!(duplicate["deduped"], true);
        assert_eq!(first["workflow_root_id"], duplicate["workflow_root_id"]);
        let root_id = Uuid::parse_str(first["workflow_root_id"].as_str().unwrap()).unwrap();

        let participant_state: (i64, i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT count(*),
                   count(*) FILTER (WHERE participant_role='requester'),
                   count(*) FILTER (WHERE participant_role='reviewer'),
                   count(*) FILTER (
                       WHERE participant_role='requester' AND actor_ref=$2
                   )
            FROM qintopia_agent_os.poster_workflow_participants
            WHERE workflow_root_id=$1
            "#,
        )
        .bind(root_id)
        .bind(crate::conversation_policy::actor_ref(
            "feishu",
            &requester_id,
        ))
        .fetch_one(&pool)
        .await
        .expect("read immutable V3 direct participant snapshot");
        assert_eq!(participant_state, (2, 1, 1, 1));

        let routed_targets: Vec<String> = sqlx::query_scalar(
            r#"
            SELECT work_item_type || ':' || target_agent
            FROM qintopia_agent_os.work_items
            WHERE id=$1 OR parent_work_item_id=$1
            ORDER BY work_item_type
            "#,
        )
        .bind(root_id)
        .fetch_all(&pool)
        .await
        .expect("read capability-routed V3 direct work items");
        assert_eq!(
            routed_targets,
            vec![
                "activity_promotion_request:xiaoman".to_string(),
                "evidence_request:wenyuange".to_string(),
                "visual_asset_request:huabaosi".to_string(),
            ]
        );

        let same_conversation_status = handle_request(
            &pool,
            &policy,
            IntakeRequest::WorkflowStatus {
                schema_version: PROTOCOL_VERSION,
                workflow_root_id: root_id,
                session: session.clone(),
            },
            true,
            false,
        )
        .await
        .expect("originating direct requester reads V3 workflow status");
        assert_eq!(same_conversation_status["user_status"], "已接单");

        let other_chat_id = format!("oc_other_direct_v3_{suffix}");
        let other_message_id = format!("om_other_direct_v3_{suffix}");
        let other_policy_id = Uuid::new_v4();
        let other_conversation_ref =
            crate::conversation_policy::conversation_ref("feishu", &other_chat_id);
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.conversation_policies
                (id, platform, conversation_ref, conversation_type, audience_class,
                 allowed_capabilities, return_mode, initiation_rule, status_visibility,
                 policy_version, policy_digest, enabled)
            VALUES ($1, 'feishu', $2, 'direct', 'private',
                    ARRAY['poster_workflow_status']::text[], 'direct_chat',
                    'direct_message', 'requester', 1, $3, true)
            "#,
        )
        .bind(other_policy_id)
        .bind(&other_conversation_ref)
        .bind(digest(&["other-direct-policy-v3", &suffix]))
        .execute(&pool)
        .await
        .expect("insert other V3 direct policy");
        let other_message_row_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_messages.messages
                (platform, message_id, event_id, chat_id, chat_type, sender_id,
                 sender_type, message_kind, text, is_mention_bot, should_trigger,
                 trigger_reason, received_at, raw)
            VALUES ('feishu', $1, $2, $3, 'direct', $4, 'user', 'text',
                    '查询海报状态', false, true,
                    'xiaoman_authenticated_feishu_message_v3', now(),
                    '{"authenticated":true,"schema_version":3}'::jsonb)
            RETURNING id
            "#,
        )
        .bind(&other_message_id)
        .bind(format!("evt_other_direct_v3_{suffix}"))
        .bind(&other_chat_id)
        .bind(&requester_id)
        .fetch_one(&pool)
        .await
        .expect("insert other authenticated V3 direct message");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.feishu_message_ingress_receipts
                (source_message_ref, message_row_id, conversation_ref, policy_id,
                 policy_version, payload_hash)
            VALUES ($1, $2, $3, $4, 1, $5)
            "#,
        )
        .bind(crate::conversation_policy::source_message_ref(
            "feishu",
            &other_message_id,
        ))
        .bind(other_message_row_id)
        .bind(&other_conversation_ref)
        .bind(other_policy_id)
        .bind(digest(&[
            "other-direct-message-payload-v3",
            &other_message_id,
        ]))
        .execute(&pool)
        .await
        .expect("insert other authenticated V3 direct receipt");
        let wrong_conversation_status = handle_request(
            &pool,
            &policy,
            IntakeRequest::WorkflowStatus {
                schema_version: PROTOCOL_VERSION,
                workflow_root_id: root_id,
                session: TrustedSession {
                    platform: "feishu".to_string(),
                    conversation_type: String::new(),
                    conversation_id: other_chat_id,
                    requester_user_id: requester_id,
                    source_message_id: other_message_id,
                    policy_version: 0,
                    binding: None,
                },
            },
            true,
            false,
        )
        .await;
        assert!(
            wrong_conversation_status.is_err(),
            "another authorized direct conversation must not read this workflow"
        );

        let publication_counts: (i64, i64, i64) = sqlx::query_as(
            r#"
            WITH RECURSIVE descendants AS (
                SELECT id FROM qintopia_agent_os.work_items WHERE id=$1
                UNION ALL
                SELECT child.id
                FROM qintopia_agent_os.work_items child
                JOIN descendants parent ON child.parent_work_item_id=parent.id
            )
            SELECT
                (SELECT count(*) FROM qintopia_agent_os.work_items item
                  WHERE item.id IN (SELECT id FROM descendants)
                    AND item.work_item_type='group_message_request'),
                (SELECT count(*) FROM qintopia_agent_os.work_item_events event
                  WHERE event.work_item_id IN (SELECT id FROM descendants)
                    AND event.event_type='send_executed'),
                (SELECT count(*) FROM qintopia_agent_os.work_item_events event
                  WHERE event.work_item_id IN (SELECT id FROM descendants)
                    AND event.event_type='external_published')
            "#,
        )
        .bind(root_id)
        .fetch_one(&pool)
        .await
        .expect("verify V3 direct workflow has no publication facts");
        assert_eq!(publication_counts, (0, 0, 0));
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
    async fn postgres_internal_group_snapshots_authority_and_accepts_only_first_revision() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 4)
            .await
            .expect("connect guarded internal-group integration database");
        db::run_migrations(&pool)
            .await
            .expect("migrate guarded internal-group integration database");

        let suffix = Uuid::new_v4().simple().to_string();
        let chat_id = format!("oc_group_{suffix}");
        let requester_id = format!("ou_requester_{suffix}");
        let reviewer_id = format!("ou_reviewer_{suffix}");
        let member_id = format!("ou_member_{suffix}");
        let root_message_id = format!("om_group_root_{suffix}");
        let policy_id = Uuid::new_v4();
        let policy_ref = crate::conversation_policy::conversation_ref("feishu", &chat_id);
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.conversation_policies
                (id, platform, conversation_ref, conversation_type, audience_class,
                 allowed_capabilities, return_mode, initiation_rule, status_visibility,
                 policy_version, policy_digest, enabled)
            VALUES ($1, 'feishu', $2, 'group', 'internal_collaboration',
                    ARRAY['poster_production_request','poster_workflow_status']::text[],
                    'thread_reply', 'explicit_bot_mention', 'conversation_members',
                    1, $3, true)
            "#,
        )
        .bind(policy_id)
        .bind(&policy_ref)
        .bind(digest(&["group-policy-v3", &suffix]))
        .execute(&pool)
        .await
        .expect("insert internal-group conversation policy");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.conversation_policy_actors
                (policy_id, actor_ref, actor_role)
            VALUES ($1, $2, 'reviewer')
            "#,
        )
        .bind(policy_id)
        .bind(crate::conversation_policy::actor_ref(
            "feishu",
            &reviewer_id,
        ))
        .execute(&pool)
        .await
        .expect("insert configured group reviewer");

        struct GroupMessageSeed<'a> {
            policy_id: Uuid,
            policy_version: i64,
            policy_ref: &'a str,
            chat_id: &'a str,
            actor_id: &'a str,
            message_id: &'a str,
            thread_root_message_id: &'a str,
            text: &'a str,
        }

        async fn seed_group_message(pool: &PgPool, seed: GroupMessageSeed<'_>) {
            let row_id: Uuid = sqlx::query_scalar(
                r#"
                INSERT INTO qintopia_messages.messages
                    (platform, message_id, event_id, chat_id, chat_type, sender_id,
                     sender_type, message_kind, text, is_mention_bot, should_trigger,
                     trigger_reason, received_at, thread_root_message_id, raw)
                VALUES ('feishu', $1, $2, $3, 'group', $4, 'user', 'text', $5,
                        true, true, 'xiaoman_authenticated_feishu_message_v3', now(),
                        $6, '{"authenticated":true,"schema_version":3}'::jsonb)
                RETURNING id
                "#,
            )
            .bind(seed.message_id)
            .bind(format!("evt_{}", seed.message_id))
            .bind(seed.chat_id)
            .bind(seed.actor_id)
            .bind(seed.text)
            .bind(seed.thread_root_message_id)
            .fetch_one(pool)
            .await
            .expect("insert authenticated internal-group message");
            sqlx::query(
                r#"
                INSERT INTO qintopia_agent_os.feishu_message_ingress_receipts
                    (source_message_ref, message_row_id, conversation_ref, policy_id,
                     policy_version, payload_hash)
                VALUES ($1, $2, $3, $4, $5, $6)
                "#,
            )
            .bind(crate::conversation_policy::source_message_ref(
                "feishu",
                seed.message_id,
            ))
            .bind(row_id)
            .bind(seed.policy_ref)
            .bind(seed.policy_id)
            .bind(seed.policy_version)
            .bind(digest(&["group-message-payload-v3", seed.message_id]))
            .execute(pool)
            .await
            .expect("insert authenticated internal-group receipt");
        }

        let request_text = "@小满 请为 AgentOS 群协作验收生成海报，时间 2026-08-01 16:00，地点线上";
        seed_group_message(
            &pool,
            GroupMessageSeed {
                policy_id,
                policy_version: 1,
                policy_ref: &policy_ref,
                chat_id: &chat_id,
                actor_id: &requester_id,
                message_id: &root_message_id,
                thread_root_message_id: &root_message_id,
                text: request_text,
            },
        )
        .await;
        let group_session = |actor_id: &str, message_id: &str| TrustedSession {
            platform: "feishu".to_string(),
            conversation_type: String::new(),
            conversation_id: chat_id.clone(),
            requester_user_id: actor_id.to_string(),
            source_message_id: message_id.to_string(),
            policy_version: 0,
            binding: None,
        };
        let policy = OperationsPolicy::dry_run();
        let first = handle_request(
            &pool,
            &policy,
            IntakeRequest::PosterProductionRequest {
                schema_version: PROTOCOL_VERSION,
                request: request_text.to_string(),
                priority: "normal".to_string(),
                activity_record_ref: String::new(),
                activity_facts: ActivityFacts {
                    source: "originating_request".to_string(),
                    title: "AgentOS 群协作验收".to_string(),
                    schedule: "2026-08-01 16:00".to_string(),
                    location: "线上".to_string(),
                    conflict_fields: Vec::new(),
                },
                session: group_session(&requester_id, &root_message_id),
                idempotency_key: String::new(),
            },
            true,
            true,
        )
        .await
        .expect("accept internal-group poster request");
        let duplicate = handle_request(
            &pool,
            &policy,
            IntakeRequest::PosterProductionRequest {
                schema_version: PROTOCOL_VERSION,
                request: request_text.to_string(),
                priority: "normal".to_string(),
                activity_record_ref: String::new(),
                activity_facts: ActivityFacts {
                    source: "originating_request".to_string(),
                    title: "AgentOS 群协作验收".to_string(),
                    schedule: "2026-08-01 16:00".to_string(),
                    location: "线上".to_string(),
                    conflict_fields: Vec::new(),
                },
                session: group_session(&requester_id, &root_message_id),
                idempotency_key: String::new(),
            },
            true,
            true,
        )
        .await
        .expect("dedupe internal-group poster request");
        assert_eq!(first["conversation_type"], "group");
        assert_eq!(duplicate["deduped"], true);
        assert_eq!(first["workflow_root_id"], duplicate["workflow_root_id"]);
        let root_id = Uuid::parse_str(first["workflow_root_id"].as_str().unwrap()).unwrap();
        let visual_id = Uuid::parse_str(first["visual_work_item_id"].as_str().unwrap()).unwrap();

        let participant_state: (i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT count(*),
                   count(*) FILTER (WHERE participant_role='requester'),
                   count(*) FILTER (WHERE participant_role='reviewer')
            FROM qintopia_agent_os.poster_workflow_participants
            WHERE workflow_root_id=$1
            "#,
        )
        .bind(root_id)
        .fetch_one(&pool)
        .await
        .expect("read immutable group participant snapshot");
        assert_eq!(participant_state, (2, 1, 1));
        let target: (String, String, String, Option<String>) = sqlx::query_as(
            r#"
            SELECT conversation_type, audience_class, delivery_mode, thread_root_message_id
            FROM qintopia_agent_os.poster_return_targets
            WHERE origin_ref = (
                SELECT metadata #>> '{workflow_metadata,origin_conversation_ref}'
                FROM qintopia_agent_os.work_items WHERE id=$1
            )
            "#,
        )
        .bind(root_id)
        .fetch_one(&pool)
        .await
        .expect("read message-scoped group return target");
        assert_eq!(
            target,
            (
                "group".to_string(),
                "internal_collaboration".to_string(),
                "thread_reply".to_string(),
                Some(root_message_id.clone())
            )
        );

        let evidence_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM qintopia_agent_os.work_items WHERE parent_work_item_id=$1 AND work_item_type='evidence_request'",
        )
        .bind(root_id)
        .fetch_one(&pool)
        .await
        .expect("load internal-group evidence child");
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET status='completed', updated_at=now() WHERE id=$1",
        )
        .bind(evidence_id)
        .execute(&pool)
        .await
        .expect("complete internal-group evidence child");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.artifacts
                (work_item_id, artifact_type, review_status, created_by_agent, title,
                 summary, content_text, content_hash, information_class, metadata)
            VALUES ($1, 'evidence_summary', 'not_required', 'wenyuange',
                    'group integration evidence', 'source-grounded fixture', 'fixture',
                    $2, 'internal_ops', '{}'::jsonb)
            "#,
        )
        .bind(evidence_id)
        .bind(digest(&["group-evidence-v3", &suffix]))
        .execute(&pool)
        .await
        .expect("insert internal-group evidence artifact");
        crate::collaboration::run_once_for_postgres_integration(&pool, visual_id)
            .await
            .expect("create authorized internal-group poster brief");
        crate::operations::run_xiaoman_poster_image_starter_for_postgres_integration(
            &pool, visual_id,
        )
        .await
        .expect("create routed internal-group image request");
        let source_image_id: Uuid = sqlx::query_scalar(
            "SELECT id FROM qintopia_agent_os.work_items WHERE parent_work_item_id=$1 AND work_item_type='image_generation_request'",
        )
        .bind(visual_id)
        .fetch_one(&pool)
        .await
        .expect("load routed internal-group image request");
        let (_review_image_id, review_artifact_id, notification_id) =
            seed_delivered_review_image(&pool, source_image_id, &format!("group-{suffix}")).await;
        let notification_route: (String, String) = sqlx::query_as(
            r#"
            SELECT item.capability_key, item.target_agent
            FROM qintopia_agent_os.poster_notifications notification
            JOIN qintopia_agent_os.work_items item ON item.id=notification.work_item_id
            WHERE notification.id=$1
            "#,
        )
        .bind(notification_id)
        .fetch_one(&pool)
        .await
        .expect("read group conversation notification capability");
        assert_eq!(
            notification_route,
            (
                "xiaoman.notify_conversation".to_string(),
                "xiaoman".to_string()
            )
        );

        let modify_event_id = format!("evt_group_modify_{suffix}");
        let modified =
            crate::poster_notification::process_review_callback_for_postgres_integration(
                &pool,
                &database_url,
                ReviewCallbackIntegrationInput {
                    callback_event_id: &modify_event_id,
                    notification_id,
                    artifact_id: review_artifact_id,
                    conversation_id: &chat_id,
                    actor_user_id: &reviewer_id,
                    action: "modify",
                },
            )
            .await
            .expect("configured group reviewer requests changes");
        assert!(!modified);
        let repeated =
            crate::poster_notification::process_review_callback_for_postgres_integration(
                &pool,
                &database_url,
                ReviewCallbackIntegrationInput {
                    callback_event_id: &modify_event_id,
                    notification_id,
                    artifact_id: review_artifact_id,
                    conversation_id: &chat_id,
                    actor_user_id: &reviewer_id,
                    action: "modify",
                },
            )
            .await
            .expect("duplicate group callback is an audited no-op");
        assert!(repeated);
        let conflicting =
            crate::poster_notification::process_review_callback_for_postgres_integration(
                &pool,
                &database_url,
                ReviewCallbackIntegrationInput {
                    callback_event_id: &format!("evt_group_conflict_{suffix}"),
                    notification_id,
                    artifact_id: review_artifact_id,
                    conversation_id: &chat_id,
                    actor_user_id: &requester_id,
                    action: "abandon",
                },
            )
            .await;
        assert!(
            conflicting.is_err(),
            "the first group review decision must win"
        );

        let requester_revision_message = format!("om_requester_revision_{suffix}");
        let reviewer_revision_message = format!("om_reviewer_revision_{suffix}");
        let requester_instruction = "标题缩短，活动时间放到主视觉下方";
        let reviewer_instruction = "标题保留，活动时间改到右下角";
        seed_group_message(
            &pool,
            GroupMessageSeed {
                policy_id,
                policy_version: 1,
                policy_ref: &policy_ref,
                chat_id: &chat_id,
                actor_id: &requester_id,
                message_id: &requester_revision_message,
                thread_root_message_id: &root_message_id,
                text: requester_instruction,
            },
        )
        .await;
        seed_group_message(
            &pool,
            GroupMessageSeed {
                policy_id,
                policy_version: 1,
                policy_ref: &policy_ref,
                chat_id: &chat_id,
                actor_id: &reviewer_id,
                message_id: &reviewer_revision_message,
                thread_root_message_id: &root_message_id,
                text: reviewer_instruction,
            },
        )
        .await;
        let requester_revision = handle_request(
            &pool,
            &policy,
            IntakeRequest::PosterRevisionRequest {
                schema_version: PROTOCOL_VERSION,
                request: requester_instruction.to_string(),
                workflow_root_id: root_id,
                revision_of_artifact_id: review_artifact_id,
                session: group_session(&requester_id, &requester_revision_message),
                idempotency_key: String::new(),
            },
            true,
            true,
        );
        let reviewer_revision = handle_request(
            &pool,
            &policy,
            IntakeRequest::PosterRevisionRequest {
                schema_version: PROTOCOL_VERSION,
                request: reviewer_instruction.to_string(),
                workflow_root_id: root_id,
                revision_of_artifact_id: review_artifact_id,
                session: group_session(&reviewer_id, &reviewer_revision_message),
                idempotency_key: String::new(),
            },
            true,
            true,
        );
        let (requester_result, reviewer_result) =
            tokio::join!(requester_revision, reviewer_revision);
        let requester_result = requester_result.expect("requester revision resolves");
        let reviewer_result = reviewer_result.expect("reviewer revision resolves");
        assert_eq!(
            requester_result["image_generation_work_item_id"],
            reviewer_result["image_generation_work_item_id"]
        );
        assert!(requester_result["deduped"] == true || reviewer_result["deduped"] == true);
        let revision_state: (i64, i64, String, String, bool) = sqlx::query_as(
            r#"
            SELECT count(DISTINCT revision.id), count(DISTINCT image.id),
                   min(revision.instruction_text),
                   min(image.payload->>'revision_instruction'),
                   bool_and(revision.first_revision_guarded)
            FROM qintopia_agent_os.poster_revision_requests revision
            JOIN qintopia_agent_os.work_items image
              ON image.id=revision.image_generation_work_item_id
            WHERE revision.source_artifact_id=$1
            "#,
        )
        .bind(review_artifact_id)
        .fetch_one(&pool)
        .await
        .expect("read first-valid group revision state");
        assert_eq!(revision_state.0, 1);
        assert_eq!(revision_state.1, 1);
        assert_eq!(revision_state.2, revision_state.3);
        assert!(revision_state.4);
        assert!(matches!(
            revision_state.2.as_str(),
            "标题缩短，活动时间放到主视觉下方" | "标题保留，活动时间改到右下角"
        ));

        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.poster_revision_requests
                (workflow_root_id, source_artifact_id, source_message_ref, actor_ref,
                 instruction_text, status, first_revision_guarded)
            VALUES
                ($1, $2, $3, $5, 'legacy revision fixture one', 'accepted', false),
                ($1, $2, $4, $5, 'legacy revision fixture two', 'accepted', false)
            "#,
        )
        .bind(root_id)
        .bind(review_artifact_id)
        .bind(digest(&["legacy-revision-one", &suffix]))
        .bind(digest(&["legacy-revision-two", &suffix]))
        .bind(crate::conversation_policy::actor_ref(
            "feishu",
            &requester_id,
        ))
        .execute(&pool)
        .await
        .expect("preserve historical unguarded revisions beside the guarded winner");
        let revision_guard_counts: (i64, i64) = sqlx::query_as(
            r#"
            SELECT count(*) FILTER (WHERE first_revision_guarded),
                   count(*) FILTER (WHERE NOT first_revision_guarded)
            FROM qintopia_agent_os.poster_revision_requests
            WHERE source_artifact_id=$1
            "#,
        )
        .bind(review_artifact_id)
        .fetch_one(&pool)
        .await
        .expect("verify guarded revision migration compatibility");
        assert_eq!(revision_guard_counts, (1, 2));

        let revised_policy_id = Uuid::new_v4();
        sqlx::query(
            "UPDATE qintopia_agent_os.conversation_policies SET enabled=false, updated_at=now() WHERE id=$1",
        )
        .bind(policy_id)
        .execute(&pool)
        .await
        .expect("disable original policy after participant snapshot");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.conversation_policies
                (id, platform, conversation_ref, conversation_type, audience_class,
                 allowed_capabilities, return_mode, initiation_rule, status_visibility,
                 policy_version, policy_digest, enabled)
            VALUES ($1, 'feishu', $2, 'group', 'internal_collaboration',
                    ARRAY['poster_production_request','poster_workflow_status']::text[],
                    'thread_reply', 'explicit_bot_mention', 'conversation_members',
                    2, $3, true)
            "#,
        )
        .bind(revised_policy_id)
        .bind(&policy_ref)
        .bind(digest(&["group-policy-v3-revised", &suffix]))
        .execute(&pool)
        .await
        .expect("insert revised internal-group policy version");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.conversation_policy_actors
                (policy_id, actor_ref, actor_role)
            VALUES ($1, $2, 'reviewer'), ($1, $3, 'reviewer')
            "#,
        )
        .bind(revised_policy_id)
        .bind(crate::conversation_policy::actor_ref(
            "feishu",
            &reviewer_id,
        ))
        .bind(crate::conversation_policy::actor_ref("feishu", &member_id))
        .execute(&pool)
        .await
        .expect("change policy after workflow participant snapshot");
        let member_status_message = format!("om_member_status_{suffix}");
        seed_group_message(
            &pool,
            GroupMessageSeed {
                policy_id: revised_policy_id,
                policy_version: 2,
                policy_ref: &policy_ref,
                chat_id: &chat_id,
                actor_id: &member_id,
                message_id: &member_status_message,
                thread_root_message_id: &format!("om_other_thread_{suffix}"),
                text: "@小满 查询海报状态",
            },
        )
        .await;
        let status = handle_request(
            &pool,
            &policy,
            IntakeRequest::WorkflowStatus {
                schema_version: PROTOCOL_VERSION,
                workflow_root_id: root_id,
                session: group_session(&member_id, &member_status_message),
            },
            true,
            true,
        )
        .await
        .expect("same-group human member reads workflow status");
        assert_eq!(status["user_status"], "生成中");

        let member_revision_message = format!("om_member_revision_{suffix}");
        seed_group_message(
            &pool,
            GroupMessageSeed {
                policy_id: revised_policy_id,
                policy_version: 2,
                policy_ref: &policy_ref,
                chat_id: &chat_id,
                actor_id: &member_id,
                message_id: &member_revision_message,
                thread_root_message_id: &root_message_id,
                text: "@小满 再换一种配色",
            },
        )
        .await;
        let unauthorized_revision = handle_request(
            &pool,
            &policy,
            IntakeRequest::PosterRevisionRequest {
                schema_version: PROTOCOL_VERSION,
                request: "再换一种配色".to_string(),
                workflow_root_id: root_id,
                revision_of_artifact_id: review_artifact_id,
                session: group_session(&member_id, &member_revision_message),
                idempotency_key: String::new(),
            },
            true,
            true,
        )
        .await;
        assert!(
            unauthorized_revision.is_err(),
            "post-snapshot policy actor must not gain mutation authority"
        );

        let final_counts: (i64, i64, i64, i64, i64) = sqlx::query_as(
            r#"
            WITH RECURSIVE descendants AS (
                SELECT id FROM qintopia_agent_os.work_items WHERE id=$1
                UNION ALL
                SELECT child.id
                FROM qintopia_agent_os.work_items child
                JOIN descendants parent ON child.parent_work_item_id=parent.id
            )
            SELECT
                (SELECT count(*)
                   FROM qintopia_agent_os.work_items item
                  WHERE item.work_item_type='group_message_request'
                    AND item.id IN (SELECT id FROM descendants)),
                (SELECT count(*)
                   FROM qintopia_agent_os.work_item_events event
                  WHERE event.work_item_id IN (SELECT id FROM descendants)
                    AND event.event_type='send_executed'),
                (SELECT count(*)
                   FROM qintopia_agent_os.work_item_events event
                  WHERE event.work_item_id IN (SELECT id FROM descendants)
                    AND event.event_type='external_published'),
                (SELECT count(*)
                   FROM qintopia_agent_os.work_item_events event
                  WHERE event.work_item_id IN (SELECT id FROM descendants)
                    AND event.event_type='poster_review_callback_duplicate_rejected'),
                (SELECT count(*)
                   FROM qintopia_agent_os.work_item_events event
                  WHERE event.work_item_id IN (SELECT id FROM descendants)
                    AND event.event_type IN (
                        'poster_review_callback_conflict_rejected',
                        'poster_mutation_rejected'
                    ))
            "#,
        )
        .bind(root_id)
        .fetch_one(&pool)
        .await
        .expect("verify group workflow mutation audit and zero publication facts");
        assert_eq!(final_counts.0, 0);
        assert_eq!(final_counts.1, 0);
        assert_eq!(final_counts.2, 0);
        assert!(final_counts.3 >= 1);
        assert!(final_counts.4 >= 2);
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
    async fn qiwe_space_session_requires_an_authenticated_persisted_message_receipt() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect Space receipt integration database");
        db::run_migrations(&pool)
            .await
            .expect("migrate Space receipt integration database");

        let suffix = Uuid::new_v4().simple().to_string();
        let chat_id = format!("space-receipt-chat-{suffix}");
        let sender_id = format!("space-receipt-sender-{suffix}");
        let message_id = format!("space-receipt-message-{suffix}");
        let conversation_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_messages.conversations
                (tenant_id, platform, chat_id, chat_type, status)
            VALUES ('qintopia', 'qiwe', $1, 'group', 'active')
            RETURNING id
            "#,
        )
        .bind(&chat_id)
        .fetch_one(&pool)
        .await
        .expect("seed Space receipt conversation");
        let raw_event_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_messages.raw_events
                (event_id, source, subject, received_at, payload, space_id,
                 ingress_auth_verified)
            VALUES ($1, 'qiwe', 'integration.space.receipt', now(), '{}'::jsonb, $2, false)
            RETURNING id
            "#,
        )
        .bind(&message_id)
        .bind(conversation_id)
        .fetch_one(&pool)
        .await
        .expect("seed unauthenticated Space receipt raw event");
        sqlx::query(
            r#"
            INSERT INTO qintopia_messages.messages
                (platform, message_id, event_id, chat_id, chat_type, sender_id,
                 message_kind, text, should_trigger, received_at, raw_event_id,
                 conversation_id)
            VALUES ('qiwe', $1, $1, $2, 'group', $3, 'text',
                    '确认 A1B2C3D4', true, now(), $4, $5)
            "#,
        )
        .bind(&message_id)
        .bind(&chat_id)
        .bind(&sender_id)
        .bind(raw_event_id)
        .bind(conversation_id)
        .execute(&pool)
        .await
        .expect("seed Space receipt message");

        let session = TrustedSession {
            platform: "qiwe".to_string(),
            conversation_type: "group".to_string(),
            conversation_id: chat_id.clone(),
            requester_user_id: sender_id.clone(),
            source_message_id: message_id.clone(),
            policy_version: 0,
            binding: None,
        };
        assert!(resolve_trusted_space_session(&pool, session.clone())
            .await
            .is_err());
        assert!(
            handle_request(
                &pool,
                &OperationsPolicy::dry_run(),
                IntakeRequest::SpaceTurnPolicyContext {
                    schema_version: 1,
                    session: session.clone(),
                },
                false,
                false,
            )
            .await
            .is_err(),
            "ordinary Space context must reject an unauthenticated receipt"
        );

        sqlx::query(
            "UPDATE qintopia_messages.raw_events SET ingress_auth_verified = true WHERE id = $1",
        )
        .bind(raw_event_id)
        .execute(&pool)
        .await
        .expect("authenticate Space receipt raw event");
        let resolved = resolve_trusted_space_session(&pool, session.clone())
            .await
            .expect("resolve authenticated Space receipt");
        assert_eq!(resolved.conversation_id, chat_id);
        assert_eq!(resolved.requester_user_id, sender_id);
        assert_eq!(
            resolved.source_message_text.as_deref(),
            Some("确认 A1B2C3D4")
        );
        let context = handle_request(
            &pool,
            &OperationsPolicy::dry_run(),
            IntakeRequest::SpaceTurnPolicyContext {
                schema_version: 1,
                session: session.clone(),
            },
            false,
            false,
        )
        .await
        .expect("load ordinary Space context from authenticated receipt");
        assert_eq!(context["policy_found"], false);
        assert_eq!(context["effective_capabilities"], json!([]));

        let mut forged = session;
        forged.requester_user_id = format!("forged-{suffix}");
        assert!(resolve_trusted_space_session(&pool, forged.clone())
            .await
            .is_err());
        assert!(
            handle_request(
                &pool,
                &OperationsPolicy::dry_run(),
                IntakeRequest::SpaceTurnPolicyContext {
                    schema_version: 1,
                    session: forged,
                },
                false,
                false,
            )
            .await
            .is_err(),
            "ordinary Space context must reject a forged operator"
        );
    }
}
