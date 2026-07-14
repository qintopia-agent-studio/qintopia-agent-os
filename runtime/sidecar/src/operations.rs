use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::{config::Cli, db, url_policy};

const ALLOWED_WORK_ITEM_TYPES: &[&str] = &[
    "visual_asset_request",
    "image_generation_request",
    "group_message_request",
    "activity_promotion_request",
    "evidence_request",
];

const ALLOWED_STATUSES: &[&str] = &[
    "queued",
    "processing",
    "awaiting_review",
    "awaiting_publish",
    "completed",
    "cancelled",
    "failed",
];

const ALLOWED_PRIORITIES: &[&str] = &["low", "normal", "high", "urgent"];
const ALLOWED_RISK_LEVELS: &[&str] = &["low", "medium", "high"];
const ALLOWED_SOURCE_TYPES: &[&str] = &[
    "manual_request",
    "apply_smoke",
    "xiaoman_activity",
    "event_signal",
    "operations_workflow",
];
const DRY_RUN_ALLOWED_GROUP_ALIASES: &[&str] = &["community_activity_group"];
const DRY_RUN_ALLOWED_GROUP_IDS: &[&str] = &[];
const BUILTIN_CAPABILITY_KEYS: &[&str] = &[
    "huabaosi.create_visual_asset",
    "huabaosi.generate_image_asset",
    "erhua.send_group_message",
    "wenyuange.retrieve_evidence",
    "xiaoman.create_activity_request",
];
const GENERATED_IMAGE_ARTIFACT_TYPE: &str = "generated_image";
const GENERATED_IMAGE_CAPABILITY_KEY: &str = "huabaosi.generate_image_asset";
const GENERATED_IMAGE_WORK_ITEM_TYPE: &str = "image_generation_request";
const GENERATED_IMAGE_WORKER_ID: &str = "huabaosi-image-generation-worker";
const MAX_APPROVABLE_GENERATED_IMAGE_BYTES: i64 = 25 * 1024 * 1024;

#[derive(Debug, Clone, Deserialize)]
pub struct WorkItemCreateRequest {
    pub requester_agent: String,
    pub target_agent: String,
    pub capability_key: String,
    pub work_item_type: String,
    pub brief_summary: String,
    #[serde(default)]
    pub purpose: String,
    #[serde(default)]
    pub human_owner: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub source_refs: Value,
    #[serde(default)]
    pub source_event_signal_id: Option<Uuid>,
    #[serde(default)]
    pub payload: Value,
    #[serde(default = "default_payload_redaction_policy")]
    pub payload_redaction_policy: String,
    #[serde(default)]
    pub idempotency_key: String,
    #[serde(default)]
    pub dedupe_key: String,
    #[serde(default)]
    pub metadata: Value,
    #[serde(default)]
    pub parent_work_item_id: Option<Uuid>,
    #[serde(default)]
    pub approved_artifact_id: Option<Uuid>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkItemCreateReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub action_status: String,
    pub current_status: String,
    pub work_item_id: Option<Uuid>,
    pub parent_work_item_id: Option<Uuid>,
    pub existing: bool,
    pub capability_key: String,
    pub work_item_type: String,
    pub requester_agent: String,
    pub target_agent: String,
    pub idempotency_key: String,
    pub dedupe_key: String,
    pub risk_level: String,
    pub review_policy: String,
    pub human_workbench: HumanWorkbenchPlan,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct HumanWorkbenchPlan {
    pub provider: String,
    pub intended_tasklist_name: String,
    pub dry_run_only: bool,
    pub title: String,
    pub description_fields: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationsReadinessReport {
    pub success: bool,
    pub action_status: String,
    pub profile: String,
    pub strict: bool,
    pub ready_for_production_adapters: bool,
    pub ready_for_apply_smoke: bool,
    pub checks: Vec<OperationsReadinessCheck>,
    pub missing_required: Vec<String>,
    pub warnings: Vec<String>,
    pub next_actions: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct OperationsReadinessCheck {
    pub key: String,
    pub status: String,
    pub required_for: Vec<String>,
    pub configured: bool,
    pub configured_count: usize,
    pub detail: String,
}

#[derive(Debug, Clone, Deserialize)]
pub struct ArtifactReviewDecisionRequest {
    pub artifact_id: Uuid,
    pub reviewer_id: String,
    pub decision: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct ArtifactReviewDecisionReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub action_status: String,
    pub artifact_id: Uuid,
    pub work_item_id: Option<Uuid>,
    pub previous_review_status: Option<String>,
    pub review_status: String,
    pub reviewer_id: String,
    pub reason_required: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct GroupMessageConfirmRequest {
    pub work_item_id: Uuid,
    pub confirmer_id: String,
    pub decision: String,
    #[serde(default)]
    pub reason: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct GroupMessageConfirmReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub action_status: String,
    pub work_item_id: Uuid,
    pub previous_status: Option<String>,
    pub current_status: String,
    pub confirmer_id: String,
    pub decision: String,
    pub send_executed: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkbenchEventRecordRequest {
    pub work_item_id: Uuid,
    #[serde(default)]
    pub artifact_id: Option<Uuid>,
    #[serde(default)]
    pub provider: String,
    pub external_id: String,
    #[serde(default)]
    pub external_event_id: String,
    pub event_type: String,
    pub actor_id: String,
    #[serde(default)]
    pub comment_text: String,
    #[serde(default)]
    pub requested_status: String,
    #[serde(default)]
    pub review_decision: String,
    #[serde(default)]
    pub confirmation_decision: String,
    #[serde(default)]
    pub source: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkbenchEventRecordReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub action_status: String,
    pub work_item_id: Uuid,
    pub artifact_id: Option<Uuid>,
    pub provider: String,
    pub external_id: String,
    pub external_event_id: String,
    pub event_type: String,
    pub actor_id: String,
    pub mutates_work_item_state: bool,
    pub recommended_command: Option<String>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkbenchEventProcessReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub action_status: String,
    pub event_id: Uuid,
    pub work_item_id: Uuid,
    pub artifact_id: Option<Uuid>,
    pub workbench_event_type: String,
    pub command_executed: Option<String>,
    pub state_mutation_recorded: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkbenchEventWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub worker: &'static str,
    pub action_status: String,
    pub event_id: Option<Uuid>,
    pub process_report: Option<WorkbenchEventProcessReport>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkItemStatusTreeReport {
    pub success: bool,
    pub queried_work_item_id: Uuid,
    pub root_work_item_id: Uuid,
    pub root: WorkItemStatusNode,
    pub children: Vec<WorkItemStatusNode>,
    pub child_count: usize,
    pub descendants: Vec<WorkItemStatusNode>,
    pub descendant_count: usize,
    pub current_blocking_point: Option<String>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowSyncReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub action_status: String,
    pub root_work_item_id: Uuid,
    pub child_count: usize,
    pub descendant_count: usize,
    pub aggregate_status: String,
    pub current_blocking_point: Option<String>,
    pub child_status_refs: Vec<WorkflowChildStatusRef>,
    pub descendant_status_refs: Vec<WorkflowChildStatusRef>,
    pub event_id: Option<Uuid>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowSyncWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub worker: &'static str,
    pub action_status: String,
    pub requested_work_item_id: Option<Uuid>,
    pub root_work_item_id: Option<Uuid>,
    pub sync_report: Option<WorkflowSyncReport>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct XiaomanActivityPromotionStarterWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub check_only: bool,
    pub worker: &'static str,
    pub source: &'static str,
    pub action_status: String,
    pub requested_work_item_id: Option<Uuid>,
    pub scanned_count: usize,
    pub created_count: usize,
    pub existing_count: usize,
    pub missing_child_count: usize,
    pub safe_for_chat: bool,
    pub work_items: Vec<WorkItemCreateReport>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct XiaomanActivitySendRequestStarterWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub check_only: bool,
    pub worker: &'static str,
    pub source: &'static str,
    pub action_status: String,
    pub requested_work_item_id: Option<Uuid>,
    pub scanned_count: usize,
    pub created_count: usize,
    pub existing_count: usize,
    pub missing_child_count: usize,
    pub safe_for_chat: bool,
    pub work_items: Vec<WorkItemCreateReport>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct XiaomanActivityImageGenerationStarterWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub check_only: bool,
    pub worker: &'static str,
    pub source: &'static str,
    pub action_status: String,
    pub requested_work_item_id: Option<Uuid>,
    pub scanned_count: usize,
    pub created_count: usize,
    pub existing_count: usize,
    pub missing_child_count: usize,
    pub safe_for_chat: bool,
    pub work_items: Vec<WorkItemCreateReport>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone)]
struct XiaomanActivityPromotionCandidate {
    id: Uuid,
    brief_summary: String,
    source_type: String,
    source_refs: Value,
    source_event_signal_id: Option<Uuid>,
    priority: String,
    human_owner: String,
    missing_evidence_child: bool,
    missing_visual_child: bool,
}

#[derive(Debug, Clone)]
struct XiaomanActivitySendRequestCandidate {
    parent_id: Uuid,
    visual_work_item_id: Uuid,
    image_generation_work_item_id: Uuid,
    approved_artifact_id: Uuid,
    brief_summary: String,
    source_type: String,
    source_refs: Value,
    source_event_signal_id: Option<Uuid>,
    priority: String,
    human_owner: String,
}

#[derive(Debug, Clone)]
struct XiaomanActivityImageGenerationCandidate {
    visual_work_item_id: Uuid,
    approved_artifact_id: Uuid,
    approved_brief_hash: String,
    evidence_content_hash: Option<String>,
    brief_summary: String,
    source_type: String,
    source_refs: Value,
    source_event_signal_id: Option<Uuid>,
    priority: String,
    human_owner: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowChildStatusRef {
    pub work_item_id: Uuid,
    pub parent_work_item_id: Option<Uuid>,
    pub depth: i32,
    pub work_item_type: String,
    pub status: String,
    pub capability_key: String,
    pub blocking_reason: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkItemStatusNode {
    pub work_item_id: Uuid,
    pub parent_work_item_id: Option<Uuid>,
    pub depth: i32,
    pub work_item_type: String,
    pub status: String,
    pub requester_agent: String,
    pub target_agent: String,
    pub capability_key: String,
    pub risk_level: String,
    pub review_policy: String,
    pub artifact_count: i64,
    pub pending_artifact_count: i64,
    pub approved_artifact_count: i64,
    pub latest_event_type: Option<String>,
    pub latest_event_at: Option<String>,
    pub blocking_reason: Option<String>,
}

#[derive(Debug, Clone)]
struct WorkItemStatusRow {
    node: WorkItemStatusNode,
}

#[derive(Debug, Clone)]
struct ArtifactApprovalContext {
    work_item_id: Uuid,
    artifact_type: String,
    review_status: String,
    created_by_agent: String,
    artifact_uri: Option<String>,
    content_hash: Option<String>,
    source_ids: Value,
    risk_labels: Vec<String>,
    information_class: String,
    metadata: Value,
    work_item_type: String,
    capability_key: String,
    work_item_status: String,
    work_item_payload: Value,
    creation_event_matches: bool,
}

#[derive(Debug, Clone)]
struct RecordedWorkbenchEvent {
    id: Uuid,
    work_item_id: Uuid,
    artifact_id: Option<Uuid>,
    actor_id: String,
    provider: String,
    external_id: String,
    external_event_id: String,
    workbench_event_type: String,
    comment_text: String,
    requested_status: String,
    review_decision: String,
    confirmation_decision: String,
    metadata: Value,
}

#[derive(Debug, Clone)]
struct WorkbenchAttachment {
    title: String,
    summary: String,
    content_text: String,
    uri: String,
    metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityListReport {
    pub success: bool,
    pub source: String,
    pub capability_count: usize,
    pub capabilities: Vec<CapabilityListItem>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct CapabilityListItem {
    pub capability_key: String,
    pub provider_agent: String,
    pub display_name: String,
    pub description: String,
    pub allowed_callers: Vec<String>,
    pub allowed_work_item_types: Vec<String>,
    pub risk_level: String,
    pub review_policy: String,
    pub enabled: bool,
    pub safe_for_non_technical_request: bool,
}

#[derive(Debug, Clone, Deserialize)]
pub struct RequestPlanInput {
    pub actor_agent: String,
    pub request_text: String,
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub source_refs: Value,
    #[serde(default)]
    pub human_owner: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestPlanReport {
    pub success: bool,
    pub action_status: String,
    pub planner: &'static str,
    pub requester_agent: String,
    pub original_request: String,
    pub selected_capability: Option<CapabilityListItem>,
    pub work_item_request: Option<WorkItemCreateRequestPreview>,
    pub work_item_preview: Option<WorkItemCreateReport>,
    pub clarification_questions: Vec<String>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkItemCreateRequestPreview {
    pub parent_work_item_id: Option<Uuid>,
    pub requester_agent: String,
    pub target_agent: String,
    pub capability_key: String,
    pub work_item_type: String,
    pub brief_summary: String,
    pub purpose: String,
    pub priority: String,
    pub source_type: String,
    pub source_refs: Value,
    pub payload: Value,
    pub payload_redaction_policy: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RequestSubmitReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub action_status: String,
    pub planner: &'static str,
    pub requester_agent: String,
    pub original_request: String,
    pub selected_capability: Option<CapabilityListItem>,
    pub work_item_request: Option<WorkItemCreateRequestPreview>,
    pub work_item_result: Option<WorkItemCreateReport>,
    pub clarification_questions: Vec<String>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Deserialize)]
pub struct WorkflowStartRequest {
    pub actor_agent: String,
    pub workflow_type: String,
    pub request_text: String,
    #[serde(default)]
    pub source_type: String,
    #[serde(default)]
    pub source_refs: Value,
    #[serde(default)]
    pub human_owner: String,
    #[serde(default = "default_priority")]
    pub priority: String,
    #[serde(default)]
    pub idempotency_key: String,
    #[serde(default)]
    pub metadata: Value,
}

#[derive(Debug, Clone, Serialize)]
pub struct WorkflowStartReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub action_status: String,
    pub workflow_type: String,
    pub parent_work_item: WorkItemCreateReport,
    pub child_work_items: Vec<WorkItemCreateReport>,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone)]
struct Capability {
    capability_key: String,
    provider_agent: String,
    display_name: String,
    description: String,
    allowed_callers: Vec<String>,
    allowed_work_item_types: Vec<String>,
    risk_level: String,
    review_policy: String,
    enabled: bool,
}

#[derive(Debug, Clone)]
pub struct OperationsPolicy {
    allowed_group_aliases: Vec<String>,
    allowed_group_ids: Vec<String>,
    allowed_reviewer_ids: Vec<String>,
    allowed_confirmer_ids: Vec<String>,
    allowed_owner_ids: Vec<String>,
    allowed_attachment_hosts: Vec<String>,
    require_approved_artifact_lookup: bool,
}

pub async fn run_create(cli: &Cli, payload_json: String, apply: bool, dry_run: bool) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let request: WorkItemCreateRequest =
        serde_json::from_str(&payload_json).context("parse operations work item payload")?;
    let apply_requested = apply && !dry_run;
    let report = if apply_requested {
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        let policy = OperationsPolicy::from_cli(cli, true);
        create_work_item(&pool, request, true, &policy).await?
    } else {
        create_work_item_dry_run(request)?
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_review_decision(
    cli: &Cli,
    payload_json: String,
    apply: bool,
    dry_run: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let request: ArtifactReviewDecisionRequest =
        serde_json::from_str(&payload_json).context("parse artifact review decision payload")?;
    let apply_requested = apply && !dry_run;
    let report = if apply_requested {
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        let policy = OperationsPolicy::from_cli(cli, true);
        record_artifact_review_decision(&pool, request, true, &policy).await?
    } else {
        let policy = OperationsPolicy::from_cli(cli, false);
        record_artifact_review_decision_dry_run(request, &policy)?
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_group_message_confirm(
    cli: &Cli,
    payload_json: String,
    apply: bool,
    dry_run: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let request: GroupMessageConfirmRequest =
        serde_json::from_str(&payload_json).context("parse group message confirmation payload")?;
    let apply_requested = apply && !dry_run;
    let report = if apply_requested {
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        let policy = OperationsPolicy::from_cli(cli, true);
        record_group_message_confirmation(&pool, request, true, &policy).await?
    } else {
        let policy = OperationsPolicy::from_cli(cli, false);
        record_group_message_confirmation_dry_run(request, &policy)?
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_workbench_event_record(
    cli: &Cli,
    payload_json: String,
    apply: bool,
    dry_run: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let request: WorkbenchEventRecordRequest =
        serde_json::from_str(&payload_json).context("parse workbench event payload")?;
    let apply_requested = apply && !dry_run;
    let report = if apply_requested {
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        record_workbench_event(&pool, request, true).await?
    } else {
        record_workbench_event_dry_run(request)?
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_workbench_event_process(
    cli: &Cli,
    event_id: Uuid,
    apply: bool,
    dry_run: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let apply_requested = apply && !dry_run;
    let policy = OperationsPolicy::from_cli(cli, true);
    let report = process_workbench_event(&pool, event_id, apply_requested, &policy).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_workbench_event_worker(
    cli: &Cli,
    once: bool,
    event_id: Option<Uuid>,
    apply: bool,
    dry_run: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    if !once {
        bail!("workbench event worker currently supports --once only");
    }
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let apply_requested = apply && !dry_run;
    let policy = OperationsPolicy::from_cli(cli, true);
    let report = run_workbench_event_worker_once(&pool, event_id, apply_requested, &policy).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_work_item_status(cli: &Cli, work_item_id: Uuid) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let report = work_item_status_tree(&pool, work_item_id).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_workflow_sync(
    cli: &Cli,
    work_item_id: Uuid,
    apply: bool,
    dry_run: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let report = sync_workflow_status(&pool, work_item_id, apply && !dry_run).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_workflow_sync_worker(
    cli: &Cli,
    once: bool,
    work_item_id: Option<Uuid>,
    apply: bool,
    dry_run: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    if !once {
        bail!("workflow sync worker currently supports --once only");
    }
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let report = run_workflow_sync_worker_once(&pool, work_item_id, apply && !dry_run).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_xiaoman_activity_promotion_starter_worker(
    cli: &Cli,
    check_only: bool,
    once: bool,
    apply: bool,
    batch_size: i64,
    work_item_id: Option<Uuid>,
) -> Result<()> {
    if check_only && apply {
        bail!("use either --check-only or --apply, not both");
    }
    if !once && !check_only {
        bail!(
            "xiaoman activity promotion starter worker currently supports --once or --check-only"
        );
    }
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let policy = OperationsPolicy::from_cli(cli, true);
    let report = run_xiaoman_activity_promotion_starter_batch(
        &pool,
        check_only,
        apply && !check_only,
        batch_size,
        work_item_id,
        &policy,
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

#[expect(
    clippy::too_many_arguments,
    reason = "the CLI command maps its reviewed flags directly to the protected worker boundary"
)]
pub async fn run_xiaoman_activity_send_request_starter_worker(
    cli: &Cli,
    check_only: bool,
    once: bool,
    apply: bool,
    batch_size: i64,
    work_item_id: Option<Uuid>,
    target_group_alias: String,
    message_text: String,
) -> Result<()> {
    if check_only && apply {
        bail!("use either --check-only or --apply, not both");
    }
    if !once && !check_only {
        bail!("xiaoman activity send request starter worker currently supports --once or --check-only");
    }
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let policy = if apply {
        OperationsPolicy::from_cli(cli, true)
    } else {
        OperationsPolicy::dry_run()
    };
    let report = run_xiaoman_activity_send_request_starter_batch(
        &pool,
        check_only,
        apply && !check_only,
        batch_size,
        work_item_id,
        &target_group_alias,
        &message_text,
        &policy,
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_xiaoman_activity_image_generation_starter_worker(
    cli: &Cli,
    check_only: bool,
    once: bool,
    apply: bool,
    batch_size: i64,
    work_item_id: Option<Uuid>,
) -> Result<()> {
    if check_only && apply {
        bail!("use either --check-only or --apply, not both");
    }
    if !once && !check_only {
        bail!(
            "xiaoman activity image generation starter worker currently supports --once or --check-only"
        );
    }
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let policy = if apply {
        OperationsPolicy::from_cli(cli, true)
    } else {
        OperationsPolicy::dry_run()
    };
    let report = run_xiaoman_activity_image_generation_starter_batch(
        &pool,
        check_only,
        apply && !check_only,
        batch_size,
        work_item_id,
        &policy,
    )
    .await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_capability_list(cli: &Cli, use_db: bool) -> Result<()> {
    let report = if use_db {
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        capability_list_from_db(&pool).await?
    } else {
        capability_list_from_builtin()
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub fn run_readiness_check(cli: &Cli, profile: String, strict: bool) -> Result<()> {
    let report = readiness_report(cli, &profile, strict)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if strict && !report.success {
        bail!(
            "operations readiness check failed: missing required configuration: {}",
            report.missing_required.join(", ")
        );
    }
    Ok(())
}

async fn work_item_status_tree(
    pool: &PgPool,
    work_item_id: Uuid,
) -> Result<WorkItemStatusTreeReport> {
    let root_id = resolve_root_work_item_id(pool, work_item_id).await?;
    let mut rows = load_work_item_status_tree_rows(pool, root_id).await?;
    let root = rows
        .first()
        .cloned()
        .ok_or_else(|| anyhow::anyhow!("workflow root is not found"))?;
    if root.node.work_item_id != root_id || root.node.depth != 0 {
        bail!("workflow status tree did not start at the resolved root");
    }
    let descendants = rows.split_off(1);
    let children = descendants
        .iter()
        .filter(|row| row.node.parent_work_item_id == Some(root_id))
        .cloned()
        .collect::<Vec<_>>();
    let mut ordered_nodes = Vec::with_capacity(descendants.len() + 1);
    ordered_nodes.push(root.clone());
    ordered_nodes.extend(descendants.iter().cloned());
    let current_blocking_point = current_blocking_point(&ordered_nodes);
    let child_count = children.len();
    let descendant_count = descendants.len();
    Ok(WorkItemStatusTreeReport {
        success: true,
        queried_work_item_id: work_item_id,
        root_work_item_id: root_id,
        root: root.node,
        children: children.into_iter().map(|row| row.node).collect(),
        child_count,
        descendants: descendants.into_iter().map(|row| row.node).collect(),
        descendant_count,
        current_blocking_point,
        limitations: vec![
            "status tree is read-only and does not execute workers".to_string(),
            "recursive status reporting is not a general DAG scheduler".to_string(),
            "Feishu Task sync state is represented only by AgentOS mirror events and refs"
                .to_string(),
        ],
        guardrails: vec![
            "Postgres remains the operations source of truth".to_string(),
            "external send-ready does not mean send executed".to_string(),
            "human workbench edits must be validated before mutating AgentOS".to_string(),
        ],
    })
}

async fn sync_workflow_status(
    pool: &PgPool,
    work_item_id: Uuid,
    apply_requested: bool,
) -> Result<WorkflowSyncReport> {
    let status_tree = work_item_status_tree(pool, work_item_id).await?;
    validate_syncable_workflow(&status_tree)?;
    let aggregate_status = workflow_aggregate_status(&status_tree);
    let child_status_refs = workflow_child_status_refs(&status_tree);
    let descendant_status_refs = workflow_descendant_status_refs(&status_tree);
    let mut event_id = None;

    if apply_requested {
        let mut tx = pool
            .begin()
            .await
            .context("begin workflow status sync transaction")?;
        sqlx::query(
            r#"
            UPDATE qintopia_agent_os.work_items
            SET
                status = $2,
                metadata = metadata || $3,
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(status_tree.root_work_item_id)
        .bind(&aggregate_status)
        .bind(json!({
            "workflow_summary": {
                "aggregate_status": aggregate_status,
                "current_blocking_point": status_tree.current_blocking_point,
                "child_count": status_tree.child_count,
                "descendant_count": status_tree.descendant_count,
                "child_status_refs": child_status_refs,
                "descendant_status_refs": descendant_status_refs,
                "synced_by": "operations-workflow-sync"
            }
        }))
        .execute(&mut *tx)
        .await
        .context("update workflow parent summary metadata")?;
        event_id = Some(
            append_event_in_tx(
                &mut tx,
                Some(status_tree.root_work_item_id),
                None,
                "workflow_status_synced",
                "system",
                "operations-workflow-sync",
                "workflow parent status summary synced from child work items",
                json!({
                    "aggregate_status": aggregate_status,
                    "current_blocking_point": status_tree.current_blocking_point,
                    "child_count": status_tree.child_count,
                    "descendant_count": status_tree.descendant_count,
                    "child_status_refs": child_status_refs,
                    "descendant_status_refs": descendant_status_refs,
                    "does_not_execute_workers": true,
                    "does_not_call_external_systems": true
                }),
            )
            .await?,
        );
        tx.commit()
            .await
            .context("commit workflow status sync transaction")?;
    }

    Ok(WorkflowSyncReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        action_status: if apply_requested {
            "workflow_status_synced".to_string()
        } else {
            "dry_run_ok".to_string()
        },
        root_work_item_id: status_tree.root_work_item_id,
        child_count: status_tree.child_count,
        descendant_count: status_tree.descendant_count,
        aggregate_status,
        current_blocking_point: status_tree.current_blocking_point,
        child_status_refs,
        descendant_status_refs,
        event_id,
        limitations: vec![
            "workflow sync updates only the AgentOS parent summary; it does not execute workers"
                .to_string(),
            "recursive summary reporting is not a general DAG scheduler".to_string(),
            "Feishu Task remains a workbench mirror, not the source of truth".to_string(),
        ],
        guardrails: vec![
            "Postgres remains the operations source of truth".to_string(),
            "external send-ready does not mean send executed".to_string(),
            "Hermes Kanban is not used as a workflow fallback".to_string(),
        ],
    })
}

async fn run_workflow_sync_worker_once(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
    apply_requested: bool,
) -> Result<WorkflowSyncWorkerReport> {
    let root_work_item_id = match work_item_id {
        Some(id) => Some(resolve_root_work_item_id(pool, id).await?),
        None => next_syncable_workflow_parent_id(pool).await?,
    };
    let Some(root_work_item_id) = root_work_item_id else {
        return Ok(workflow_sync_worker_report(
            apply_requested,
            work_item_id,
            None,
            None,
            "no_syncable_workflow",
        ));
    };

    let sync_report = sync_workflow_status(pool, root_work_item_id, apply_requested).await?;
    let action_status = sync_report.action_status.clone();
    Ok(workflow_sync_worker_report(
        apply_requested,
        work_item_id,
        Some(root_work_item_id),
        Some(sync_report),
        &action_status,
    ))
}

async fn run_xiaoman_activity_promotion_starter_batch(
    pool: &PgPool,
    check_only: bool,
    apply_requested: bool,
    batch_size: i64,
    work_item_id: Option<Uuid>,
    policy: &OperationsPolicy,
) -> Result<XiaomanActivityPromotionStarterWorkerReport> {
    let candidates =
        load_xiaoman_activity_promotion_candidates(pool, work_item_id, batch_size).await?;
    let mut work_items = Vec::new();
    let mut missing_child_count = 0;

    for candidate in &candidates {
        let requests = xiaoman_activity_promotion_child_requests(candidate);
        missing_child_count += requests.len();
        for request in requests {
            let report = if apply_requested {
                create_work_item(pool, request, true, policy).await?
            } else {
                create_work_item_dry_run(request)?
            };
            work_items.push(report);
        }
    }

    let existing_count = work_items.iter().filter(|item| item.existing).count();
    let created_count = work_items.len().saturating_sub(existing_count);
    let action_status = if candidates.is_empty() {
        "no_eligible_activity_requests"
    } else if apply_requested {
        "activity_promotion_children_created"
    } else {
        "activity_promotion_children_preview"
    };

    Ok(xiaoman_activity_promotion_starter_report(
        check_only,
        apply_requested,
        work_item_id,
        candidates.len(),
        created_count,
        existing_count,
        missing_child_count,
        action_status,
        work_items,
    ))
}

#[expect(
    clippy::too_many_arguments,
    reason = "the batch receives explicit queue selection, payload, and policy inputs"
)]
async fn run_xiaoman_activity_send_request_starter_batch(
    pool: &PgPool,
    check_only: bool,
    apply_requested: bool,
    batch_size: i64,
    work_item_id: Option<Uuid>,
    target_group_alias: &str,
    message_text: &str,
    policy: &OperationsPolicy,
) -> Result<XiaomanActivitySendRequestStarterWorkerReport> {
    let candidates =
        load_xiaoman_activity_send_request_candidates(pool, work_item_id, batch_size).await?;
    let mut work_items = Vec::new();

    for candidate in &candidates {
        let request = xiaoman_activity_send_request(candidate, target_group_alias, message_text)?;
        let report = if apply_requested {
            create_work_item(pool, request, true, policy).await?
        } else {
            create_work_item_dry_run(request)?
        };
        work_items.push(report);
    }

    let existing_count = work_items.iter().filter(|item| item.existing).count();
    let created_count = work_items.len().saturating_sub(existing_count);
    let action_status = if candidates.is_empty() {
        "no_eligible_approved_generated_images"
    } else if apply_requested {
        "group_message_requests_created"
    } else {
        "group_message_requests_preview"
    };

    Ok(xiaoman_activity_send_request_starter_report(
        check_only,
        apply_requested,
        work_item_id,
        candidates.len(),
        created_count,
        existing_count,
        candidates.len(),
        action_status,
        work_items,
    ))
}

async fn run_xiaoman_activity_image_generation_starter_batch(
    pool: &PgPool,
    check_only: bool,
    apply_requested: bool,
    batch_size: i64,
    work_item_id: Option<Uuid>,
    policy: &OperationsPolicy,
) -> Result<XiaomanActivityImageGenerationStarterWorkerReport> {
    let candidates =
        load_xiaoman_activity_image_generation_candidates(pool, work_item_id, batch_size).await?;
    let mut work_items = Vec::new();

    for candidate in &candidates {
        let request = xiaoman_activity_image_generation_request(candidate);
        let report = if apply_requested {
            create_work_item(pool, request, true, policy).await?
        } else {
            create_work_item_dry_run(request)?
        };
        work_items.push(report);
    }

    let existing_count = work_items.iter().filter(|item| item.existing).count();
    let created_count = work_items.len().saturating_sub(existing_count);
    let action_status = if candidates.is_empty() {
        "no_eligible_approved_visual_artifacts"
    } else if apply_requested {
        "image_generation_requests_created"
    } else {
        "image_generation_requests_preview"
    };

    Ok(xiaoman_activity_image_generation_starter_report(
        check_only,
        apply_requested,
        work_item_id,
        candidates.len(),
        created_count,
        existing_count,
        candidates.len(),
        action_status,
        work_items,
    ))
}

async fn load_xiaoman_activity_promotion_candidates(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
    batch_size: i64,
) -> Result<Vec<XiaomanActivityPromotionCandidate>> {
    let rows = sqlx::query(
        r#"
        SELECT
            parent.id,
            parent.brief_summary,
            parent.source_type,
            parent.source_refs,
            parent.source_event_signal_id,
            parent.priority,
            parent.human_owner,
            NOT EXISTS (
                SELECT 1
                FROM qintopia_agent_os.work_items child
                WHERE child.parent_work_item_id = parent.id
                  AND child.capability_key = 'wenyuange.retrieve_evidence'
                  AND child.work_item_type = 'evidence_request'
            ) AS missing_evidence_child,
            NOT EXISTS (
                SELECT 1
                FROM qintopia_agent_os.work_items child
                WHERE child.parent_work_item_id = parent.id
                  AND child.capability_key = 'huabaosi.create_visual_asset'
                  AND child.work_item_type = 'visual_asset_request'
            ) AS missing_visual_child
        FROM qintopia_agent_os.work_items parent
        WHERE parent.capability_key = 'xiaoman.create_activity_request'
          AND parent.work_item_type = 'activity_promotion_request'
          AND parent.target_agent = 'xiaoman'
          AND ($1::uuid IS NULL OR parent.id = $1)
          AND (
            NOT EXISTS (
                SELECT 1
                FROM qintopia_agent_os.work_items child
                WHERE child.parent_work_item_id = parent.id
                  AND child.capability_key = 'wenyuange.retrieve_evidence'
                  AND child.work_item_type = 'evidence_request'
            )
            OR NOT EXISTS (
                SELECT 1
                FROM qintopia_agent_os.work_items child
                WHERE child.parent_work_item_id = parent.id
                  AND child.capability_key = 'huabaosi.create_visual_asset'
                  AND child.work_item_type = 'visual_asset_request'
            )
          )
        ORDER BY parent.created_at ASC, parent.id ASC
        LIMIT $2
        "#,
    )
    .bind(work_item_id)
    .bind(batch_size.max(1))
    .fetch_all(pool)
    .await
    .context("load Xiaoman activity promotion starter candidates")?;

    rows.into_iter()
        .map(|row| {
            Ok(XiaomanActivityPromotionCandidate {
                id: row.try_get("id")?,
                brief_summary: row.try_get("brief_summary")?,
                source_type: row.try_get("source_type")?,
                source_refs: row.try_get("source_refs")?,
                source_event_signal_id: row.try_get("source_event_signal_id")?,
                priority: row.try_get("priority")?,
                human_owner: row.try_get("human_owner")?,
                missing_evidence_child: row.try_get("missing_evidence_child")?,
                missing_visual_child: row.try_get("missing_visual_child")?,
            })
        })
        .collect()
}

async fn load_xiaoman_activity_send_request_candidates(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
    batch_size: i64,
) -> Result<Vec<XiaomanActivitySendRequestCandidate>> {
    let rows = sqlx::query(
        r#"
        SELECT
            parent.id AS parent_id,
            visual.id AS visual_work_item_id,
            image_request.id AS image_generation_work_item_id,
            generated_image.id AS approved_artifact_id,
            parent.brief_summary,
            parent.source_type,
            parent.source_refs,
            parent.source_event_signal_id,
            parent.priority,
            parent.human_owner
        FROM qintopia_agent_os.work_items parent
        JOIN qintopia_agent_os.work_items visual
          ON visual.parent_work_item_id = parent.id
         AND visual.capability_key = 'huabaosi.create_visual_asset'
         AND visual.work_item_type = 'visual_asset_request'
         AND visual.status = 'completed'
        JOIN qintopia_agent_os.work_items image_request
          ON image_request.parent_work_item_id = visual.id
         AND image_request.capability_key = 'huabaosi.generate_image_asset'
         AND image_request.work_item_type = 'image_generation_request'
         AND image_request.status = 'completed'
        JOIN LATERAL (
            SELECT id
            FROM qintopia_agent_os.artifacts generated_image
            WHERE generated_image.work_item_id = image_request.id
              AND generated_image.artifact_type = 'generated_image'
              AND generated_image.review_status = 'approved'
            ORDER BY generated_image.updated_at DESC, generated_image.created_at DESC
            LIMIT 1
        ) generated_image ON true
        WHERE parent.capability_key = 'xiaoman.create_activity_request'
          AND parent.work_item_type = 'activity_promotion_request'
          AND parent.target_agent = 'xiaoman'
          AND ($1::uuid IS NULL OR parent.id = $1 OR visual.id = $1 OR image_request.id = $1)
          AND NOT EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_items child
              WHERE child.parent_work_item_id = parent.id
                AND child.capability_key = 'erhua.send_group_message'
                AND child.work_item_type = 'group_message_request'
          )
        ORDER BY parent.created_at ASC
        LIMIT $2
        "#,
    )
    .bind(work_item_id)
    .bind(batch_size.max(1))
    .fetch_all(pool)
    .await
    .context("load Xiaoman activity send request candidates")?;

    rows.into_iter()
        .map(|row| {
            Ok(XiaomanActivitySendRequestCandidate {
                parent_id: row.try_get("parent_id")?,
                visual_work_item_id: row.try_get("visual_work_item_id")?,
                image_generation_work_item_id: row.try_get("image_generation_work_item_id")?,
                approved_artifact_id: row.try_get("approved_artifact_id")?,
                brief_summary: row.try_get("brief_summary")?,
                source_type: row.try_get("source_type")?,
                source_refs: row.try_get("source_refs")?,
                source_event_signal_id: row.try_get("source_event_signal_id")?,
                priority: row.try_get("priority")?,
                human_owner: row.try_get("human_owner")?,
            })
        })
        .collect()
}

async fn load_xiaoman_activity_image_generation_candidates(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
    batch_size: i64,
) -> Result<Vec<XiaomanActivityImageGenerationCandidate>> {
    let rows = sqlx::query(
        r#"
        SELECT
            visual.id AS visual_work_item_id,
            artifact.id AS approved_artifact_id,
            artifact.content_hash AS approved_brief_hash,
            artifact.metadata->>'evidence_content_hash' AS evidence_content_hash,
            visual.brief_summary,
            visual.source_type,
            visual.source_refs,
            visual.source_event_signal_id,
            visual.priority,
            visual.human_owner
        FROM qintopia_agent_os.work_items visual
        JOIN qintopia_agent_os.work_items parent
          ON parent.id = visual.parent_work_item_id
         AND parent.capability_key = 'xiaoman.create_activity_request'
         AND parent.work_item_type = 'activity_promotion_request'
         AND parent.target_agent = 'xiaoman'
        JOIN LATERAL (
            SELECT id, content_hash, metadata
            FROM qintopia_agent_os.artifacts artifact
            WHERE artifact.work_item_id = visual.id
              AND artifact.artifact_type = 'poster_brief'
              AND artifact.review_status = 'approved'
              AND artifact.content_hash IS NOT NULL
              AND artifact.content_hash <> ''
            ORDER BY artifact.updated_at DESC, artifact.created_at DESC
            LIMIT 1
        ) artifact ON true
        WHERE visual.capability_key = 'huabaosi.create_visual_asset'
          AND visual.work_item_type = 'visual_asset_request'
          AND visual.target_agent = 'huabaosi'
          AND visual.status = 'completed'
          AND ($1::uuid IS NULL OR visual.id = $1 OR artifact.id = $1)
          AND NOT EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_items child
              WHERE child.parent_work_item_id = visual.id
                AND child.capability_key = 'huabaosi.generate_image_asset'
                AND child.work_item_type = 'image_generation_request'
          )
        ORDER BY visual.created_at ASC, visual.id ASC
        LIMIT $2
        "#,
    )
    .bind(work_item_id)
    .bind(batch_size.max(1))
    .fetch_all(pool)
    .await
    .context("load Xiaoman activity image generation starter candidates")?;

    rows.into_iter()
        .map(|row| {
            Ok(XiaomanActivityImageGenerationCandidate {
                visual_work_item_id: row.try_get("visual_work_item_id")?,
                approved_artifact_id: row.try_get("approved_artifact_id")?,
                approved_brief_hash: row.try_get("approved_brief_hash")?,
                evidence_content_hash: row.try_get("evidence_content_hash")?,
                brief_summary: row.try_get("brief_summary")?,
                source_type: row.try_get("source_type")?,
                source_refs: row.try_get("source_refs")?,
                source_event_signal_id: row.try_get("source_event_signal_id")?,
                priority: row.try_get("priority")?,
                human_owner: row.try_get("human_owner")?,
            })
        })
        .collect()
}

async fn next_syncable_workflow_parent_id(pool: &PgPool) -> Result<Option<Uuid>> {
    sqlx::query_scalar(
        r#"
        SELECT parent.id
        FROM qintopia_agent_os.work_items parent
        WHERE parent.parent_work_item_id IS NULL
          AND parent.work_item_type = 'activity_promotion_request'
          AND EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_items child
              WHERE child.parent_work_item_id = parent.id
          )
        ORDER BY parent.updated_at ASC, parent.created_at ASC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await
    .context("load next workflow parent for sync")
}

fn validate_syncable_workflow(status_tree: &WorkItemStatusTreeReport) -> Result<()> {
    if status_tree.root.work_item_type != "activity_promotion_request" {
        bail!("workflow sync currently supports activity_promotion_request parents only");
    }
    if status_tree.child_count == 0 {
        bail!("workflow parent has no child work items to summarize");
    }
    Ok(())
}

fn workflow_aggregate_status(status_tree: &WorkItemStatusTreeReport) -> String {
    if status_tree
        .descendants
        .iter()
        .any(|child| child.status == "failed")
    {
        return "failed".to_string();
    }
    if status_tree
        .descendants
        .iter()
        .any(|child| child.status == "cancelled")
    {
        return "cancelled".to_string();
    }
    if status_tree.current_blocking_point.is_some() {
        return "processing".to_string();
    }
    if status_tree
        .descendants
        .iter()
        .all(|child| child.status == "completed")
    {
        return "completed".to_string();
    }
    "processing".to_string()
}

fn workflow_child_status_refs(
    status_tree: &WorkItemStatusTreeReport,
) -> Vec<WorkflowChildStatusRef> {
    workflow_status_refs(&status_tree.children)
}

fn workflow_descendant_status_refs(
    status_tree: &WorkItemStatusTreeReport,
) -> Vec<WorkflowChildStatusRef> {
    workflow_status_refs(&status_tree.descendants)
}

fn workflow_status_refs(nodes: &[WorkItemStatusNode]) -> Vec<WorkflowChildStatusRef> {
    nodes
        .iter()
        .map(|node| WorkflowChildStatusRef {
            work_item_id: node.work_item_id,
            parent_work_item_id: node.parent_work_item_id,
            depth: node.depth,
            work_item_type: node.work_item_type.clone(),
            status: node.status.clone(),
            capability_key: node.capability_key.clone(),
            blocking_reason: node.blocking_reason.clone(),
        })
        .collect()
}

async fn resolve_root_work_item_id(pool: &PgPool, work_item_id: Uuid) -> Result<Uuid> {
    let row = sqlx::query(
        r#"
        WITH RECURSIVE ancestry AS (
            SELECT id, parent_work_item_id, ARRAY[id] AS path
            FROM qintopia_agent_os.work_items
            WHERE id = $1

            UNION ALL

            SELECT parent.id, parent.parent_work_item_id, ancestry.path || parent.id
            FROM qintopia_agent_os.work_items parent
            JOIN ancestry ON parent.id = ancestry.parent_work_item_id
            WHERE NOT parent.id = ANY(ancestry.path)
        )
        SELECT id AS root_work_item_id
        FROM ancestry
        WHERE parent_work_item_id IS NULL
        LIMIT 1
        "#,
    )
    .bind(work_item_id)
    .fetch_optional(pool)
    .await
    .context("load work item root")?
    .ok_or_else(|| anyhow::anyhow!("work item is not found or its parent chain is cyclic"))?;
    Ok(row.get("root_work_item_id"))
}

async fn load_work_item_status_tree_rows(
    pool: &PgPool,
    root_work_item_id: Uuid,
) -> Result<Vec<WorkItemStatusRow>> {
    let rows = sqlx::query(
        r#"
        WITH RECURSIVE status_tree AS (
            SELECT id, 0::integer AS depth, ARRAY[id] AS path
            FROM qintopia_agent_os.work_items
            WHERE id = $1

            UNION ALL

            SELECT child.id, parent.depth + 1, parent.path || child.id
            FROM qintopia_agent_os.work_items child
            JOIN status_tree parent ON child.parent_work_item_id = parent.id
            WHERE NOT child.id = ANY(parent.path)
        )
        SELECT
            wi.id,
            wi.parent_work_item_id,
            status_tree.depth,
            wi.work_item_type,
            wi.status,
            wi.requester_agent,
            wi.target_agent,
            wi.capability_key,
            wi.risk_level,
            wi.review_policy,
            COUNT(a.id)::bigint AS artifact_count,
            COUNT(a.id) FILTER (WHERE a.review_status = 'pending')::bigint AS pending_artifact_count,
            COUNT(a.id) FILTER (WHERE a.review_status = 'approved')::bigint AS approved_artifact_count,
            (
                SELECT event_type
                FROM qintopia_agent_os.work_item_events e
                WHERE e.work_item_id = wi.id
                ORDER BY e.created_at DESC
                LIMIT 1
            ) AS latest_event_type,
            (
                SELECT e.created_at::text
                FROM qintopia_agent_os.work_item_events e
                WHERE e.work_item_id = wi.id
                ORDER BY e.created_at DESC
                LIMIT 1
            ) AS latest_event_at,
            (
                SELECT count(*)::bigint
                FROM qintopia_agent_os.work_item_events e
                WHERE e.work_item_id = wi.id
                  AND e.event_type = 'group_message_send_ready_recorded'
                  AND e.data->>'send_executed' = 'false'
            ) AS send_ready_event_count
        FROM status_tree
        JOIN qintopia_agent_os.work_items wi ON wi.id = status_tree.id
        LEFT JOIN qintopia_agent_os.artifacts a ON a.work_item_id = wi.id
        GROUP BY wi.id, status_tree.depth
        ORDER BY status_tree.depth ASC, wi.created_at ASC, wi.id ASC
        "#,
    )
        .bind(root_work_item_id)
        .fetch_all(pool)
        .await
        .context("load recursive work item status tree")?;
    rows.into_iter().map(status_row_from_row).collect()
}

fn status_row_from_row(row: sqlx::postgres::PgRow) -> Result<WorkItemStatusRow> {
    let status: String = row.try_get("status")?;
    let work_item_type: String = row.try_get("work_item_type")?;
    let pending_artifact_count: i64 = row.try_get("pending_artifact_count")?;
    let send_ready_event_count: i64 = row.try_get("send_ready_event_count")?;
    let blocking_reason = blocking_reason_for(
        &status,
        &work_item_type,
        pending_artifact_count,
        send_ready_event_count,
    );
    Ok(WorkItemStatusRow {
        node: WorkItemStatusNode {
            work_item_id: row.try_get("id")?,
            parent_work_item_id: row.try_get("parent_work_item_id")?,
            depth: row.try_get("depth")?,
            work_item_type,
            status,
            requester_agent: row.try_get("requester_agent")?,
            target_agent: row.try_get("target_agent")?,
            capability_key: row.try_get("capability_key")?,
            risk_level: row.try_get("risk_level")?,
            review_policy: row.try_get("review_policy")?,
            artifact_count: row.try_get("artifact_count")?,
            pending_artifact_count,
            approved_artifact_count: row.try_get("approved_artifact_count")?,
            latest_event_type: row.try_get("latest_event_type")?,
            latest_event_at: row.try_get("latest_event_at")?,
            blocking_reason,
        },
    })
}

fn blocking_reason_for(
    status: &str,
    work_item_type: &str,
    pending_artifact_count: i64,
    send_ready_event_count: i64,
) -> Option<String> {
    if status == "failed" {
        return Some("failed_requires_human_or_worker_retry".to_string());
    }
    if status == "awaiting_publish" {
        return Some("waiting_for_human_final_confirmation".to_string());
    }
    if pending_artifact_count > 0 {
        return Some("waiting_for_artifact_review".to_string());
    }
    if status == "awaiting_review" {
        return Some("waiting_for_review_or_next_step".to_string());
    }
    if work_item_type == "group_message_request" && status == "queued" && send_ready_event_count > 0
    {
        return Some("send_ready_waiting_for_production_send_adapter".to_string());
    }
    if status == "queued" {
        return Some("waiting_for_worker".to_string());
    }
    if status == "processing" {
        return Some("worker_processing_or_claim_expiry".to_string());
    }
    None
}

fn current_blocking_point(rows: &[WorkItemStatusRow]) -> Option<String> {
    let actionable_child_blocker = rows
        .iter()
        .filter(|row| row.node.parent_work_item_id.is_some())
        .filter_map(|row| {
            row.node
                .blocking_reason
                .as_ref()
                .map(|reason| format!("{}:{}", row.node.work_item_type, reason))
        })
        .next();
    if actionable_child_blocker.is_some() {
        return actionable_child_blocker;
    }
    rows.iter().find_map(|row| {
        row.node
            .blocking_reason
            .as_ref()
            .map(|reason| format!("{}:{}", row.node.work_item_type, reason))
    })
}

async fn load_workbench_event(pool: &PgPool, event_id: Uuid) -> Result<RecordedWorkbenchEvent> {
    let row = sqlx::query(
        r#"
        SELECT id, work_item_id, artifact_id, actor_id, data
        FROM qintopia_agent_os.work_item_events
        WHERE id = $1
          AND event_type = 'human_workbench_event_recorded'
        "#,
    )
    .bind(event_id)
    .fetch_optional(pool)
    .await
    .context("load recorded human workbench event")?
    .ok_or_else(|| anyhow::anyhow!("human_workbench_event_recorded event is not found"))?;
    let data: Value = row.try_get("data")?;
    Ok(RecordedWorkbenchEvent {
        id: row.try_get("id")?,
        work_item_id: row.try_get("work_item_id")?,
        artifact_id: row.try_get("artifact_id")?,
        actor_id: row.try_get("actor_id")?,
        provider: string_field(&data, "provider"),
        external_id: string_field(&data, "external_id"),
        external_event_id: string_field(&data, "external_event_id"),
        workbench_event_type: string_field(&data, "workbench_event_type"),
        comment_text: string_field(&data, "comment_text"),
        requested_status: string_field(&data, "requested_status"),
        review_decision: string_field(&data, "review_decision"),
        confirmation_decision: string_field(&data, "confirmation_decision"),
        metadata: data.get("metadata").cloned().unwrap_or_else(|| json!({})),
    })
}

async fn workbench_event_processed(pool: &PgPool, event_id: Uuid) -> Result<bool> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM qintopia_agent_os.work_item_events
            WHERE event_type = 'human_workbench_event_processed'
              AND data->>'source_event_id' = $1
        )
        "#,
    )
    .bind(event_id.to_string())
    .fetch_one(pool)
    .await
    .context("check processed workbench event")
}

fn validate_processable_workbench_event(event: &RecordedWorkbenchEvent) -> Result<()> {
    match event.workbench_event_type.as_str() {
        "review_decision_requested" => {
            if event.artifact_id.is_none() {
                bail!("artifact_id is required for review events");
            }
            if !["approved", "rejected", "changes_requested"]
                .contains(&event.review_decision.as_str())
            {
                bail!("review_decision is not allowed");
            }
        }
        "final_confirmation_requested" => {
            if !["confirmed", "cancelled"].contains(&event.confirmation_decision.as_str()) {
                bail!("confirmation_decision is not allowed");
            }
        }
        "status_change_requested" => validate_workbench_status_change_event(event)?,
        "owner_changed" => validate_workbench_owner_change_event(event)?,
        "attachment_added" => validate_workbench_attachment_event(event)?,
        _ => bail!("workbench event_type is not processable"),
    }
    Ok(())
}

fn command_for_recorded_workbench_event(event: &RecordedWorkbenchEvent) -> Result<String> {
    match event.workbench_event_type.as_str() {
        "review_decision_requested" => Ok("operations-artifact-review-decision".to_string()),
        "final_confirmation_requested" => Ok("operations-group-message-confirm".to_string()),
        "status_change_requested" => Ok("operations-workbench-status-change".to_string()),
        "owner_changed" => Ok("operations-workbench-owner-change".to_string()),
        "attachment_added" => Ok("operations-workbench-attachment-add".to_string()),
        _ => bail!("workbench event has no processable command"),
    }
}

fn string_field(data: &Value, key: &str) -> String {
    data.get(key)
        .and_then(Value::as_str)
        .unwrap_or_default()
        .to_string()
}

pub fn run_request_plan(payload_json: String) -> Result<()> {
    let request: RequestPlanInput =
        serde_json::from_str(&payload_json).context("parse operations request plan payload")?;
    let report = plan_request(request)?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_request_submit(
    cli: &Cli,
    payload_json: String,
    apply: bool,
    dry_run: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let input: RequestPlanInput =
        serde_json::from_str(&payload_json).context("parse operations request submit payload")?;
    let apply_requested = apply && !dry_run;
    let report = if apply_requested {
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        let policy = OperationsPolicy::from_cli(cli, true);
        submit_request(&pool, input, true, &policy).await?
    } else {
        submit_request_dry_run(input)?
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_workflow_start(
    cli: &Cli,
    payload_json: String,
    apply: bool,
    dry_run: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let request: WorkflowStartRequest =
        serde_json::from_str(&payload_json).context("parse operations workflow start payload")?;
    let apply_requested = apply && !dry_run;
    let report = if apply_requested {
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        let policy = OperationsPolicy::from_cli(cli, true);
        start_workflow(&pool, request, true, &policy).await?
    } else {
        start_workflow_dry_run(request)?
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub fn record_artifact_review_decision_dry_run(
    mut request: ArtifactReviewDecisionRequest,
    policy: &OperationsPolicy,
) -> Result<ArtifactReviewDecisionReport> {
    normalize_review_request(&mut request);
    validate_review_request(&request)?;
    validate_reviewer_authorized(&request.reviewer_id, policy)?;
    Ok(review_report(&request, false, "dry_run_ok", None, None))
}

pub async fn record_artifact_review_decision(
    pool: &PgPool,
    mut request: ArtifactReviewDecisionRequest,
    apply_requested: bool,
    policy: &OperationsPolicy,
) -> Result<ArtifactReviewDecisionReport> {
    normalize_review_request(&mut request);
    validate_review_request(&request)?;
    if let Err(err) = validate_reviewer_authorized(&request.reviewer_id, policy) {
        if apply_requested {
            append_review_policy_denial(
                pool,
                &request,
                None,
                "artifact review decision denied by reviewer allowlist",
                &err.to_string(),
                "artifact_review_reviewer_allowlist",
            )
            .await?;
        }
        return Err(err);
    }

    if !apply_requested {
        return Ok(review_report(&request, false, "dry_run_ok", None, None));
    }

    let mut tx = pool
        .begin()
        .await
        .context("begin artifact review transaction")?;
    let row = sqlx::query(
        r#"
        SELECT
            artifact.id,
            artifact.work_item_id,
            artifact.artifact_type,
            artifact.review_status,
            artifact.created_by_agent,
            artifact.artifact_uri,
            artifact.content_hash,
            artifact.source_ids,
            artifact.risk_labels,
            artifact.information_class,
            artifact.metadata,
            work_item.work_item_type,
            work_item.capability_key,
            work_item.status AS work_item_status,
            work_item.payload AS work_item_payload,
            EXISTS (
                SELECT 1
                FROM qintopia_agent_os.work_item_events event
                WHERE event.work_item_id = artifact.work_item_id
                  AND event.artifact_id = artifact.id
                  AND event.event_type = 'generated_image_created'
                  AND event.actor_type = 'worker'
                  AND event.actor_id = $2
                  AND event.data->>'content_hash' = artifact.content_hash
                  AND event.data->>'mime_type' = artifact.metadata->>'mime_type'
                  AND event.data->>'file_md5' = artifact.metadata->>'file_md5'
                  AND event.data->>'provider_source_mime_type' = artifact.metadata->>'provider_source_mime_type'
                  AND event.data->>'provider_source_content_hash' = artifact.metadata->>'provider_source_content_hash'
                  AND event.data->>'media_transform' = artifact.metadata->>'media_transform'
                  AND event.data->>'jpeg_quality' = artifact.metadata->>'jpeg_quality'
                  AND event.data->>'alpha_background' = artifact.metadata->>'alpha_background'
                  AND event.data->>'width' = artifact.metadata->>'width'
                  AND event.data->>'height' = artifact.metadata->>'height'
                  AND event.data->>'byte_size' = artifact.metadata->>'byte_size'
                  AND event.data->>'external_publish_executed' = 'false'
            ) AS creation_event_matches
        FROM qintopia_agent_os.artifacts artifact
        JOIN qintopia_agent_os.work_items work_item ON work_item.id = artifact.work_item_id
        WHERE artifact.id = $1
        FOR UPDATE OF artifact, work_item
        "#,
    )
    .bind(request.artifact_id)
    .bind(GENERATED_IMAGE_WORKER_ID)
    .fetch_optional(&mut *tx)
    .await
    .context("load artifact for review")?
    .ok_or_else(|| anyhow::anyhow!("artifact is not found"))?;

    let approval_context = artifact_approval_context_from_row(&row)?;
    if let Err(error) = validate_generated_image_approval(&approval_context, &request.decision) {
        tx.rollback()
            .await
            .context("rollback denied generated image approval")?;
        append_review_policy_denial(
            pool,
            &request,
            Some(approval_context.work_item_id),
            "generated image approval denied by integrity policy",
            "generated_image artifact integrity validation failed",
            "generated_image_approval_integrity",
        )
        .await?;
        return Err(error);
    }

    let work_item_id = approval_context.work_item_id;
    let previous_review_status = approval_context.review_status;
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.artifacts
        SET
            review_status = $2,
            reviewed_at = now(),
            reviewed_by = $3,
            review_decision_reason = $4,
            metadata = metadata || $5,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(request.artifact_id)
    .bind(&request.decision)
    .bind(&request.reviewer_id)
    .bind(&request.reason)
    .bind(json!({
        "review_source": request.source,
        "review_metadata": request.metadata,
    }))
    .execute(&mut *tx)
    .await
    .context("update artifact review decision")?;

    append_event_in_tx(
        &mut tx,
        Some(work_item_id),
        Some(request.artifact_id),
        "review_decision_recorded",
        "human",
        &request.reviewer_id,
        "artifact review decision recorded",
        json!({
            "previous_review_status": previous_review_status,
            "review_status": request.decision,
            "reason": request.reason,
            "source": request.source,
            "does_not_publish": true,
        }),
    )
    .await?;
    update_work_item_after_review(&mut tx, work_item_id, &request).await?;
    tx.commit()
        .await
        .context("commit artifact review transaction")?;

    Ok(review_report(
        &request,
        true,
        "review_recorded",
        Some(work_item_id),
        Some(previous_review_status),
    ))
}

async fn update_work_item_after_review(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Uuid,
    request: &ArtifactReviewDecisionRequest,
) -> Result<()> {
    let (next_status, last_error): (&str, Option<String>) = match request.decision.as_str() {
        "approved" => ("completed", None),
        "rejected" => ("cancelled", Some(trim_error(&request.reason))),
        "changes_requested" => ("awaiting_review", Some(trim_error(&request.reason))),
        _ => return Ok(()),
    };
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = $2,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = $3,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(work_item_id)
    .bind(next_status)
    .bind(last_error)
    .execute(&mut **tx)
    .await
    .context("update work item after artifact review decision")?;
    Ok(())
}

pub fn record_group_message_confirmation_dry_run(
    mut request: GroupMessageConfirmRequest,
    policy: &OperationsPolicy,
) -> Result<GroupMessageConfirmReport> {
    normalize_group_message_confirm_request(&mut request);
    validate_group_message_confirm_request(&request)?;
    validate_confirmer_authorized(&request.confirmer_id, policy)?;
    let current_status = status_after_group_message_confirmation(&request);
    Ok(group_message_confirm_report(
        &request,
        false,
        "dry_run_ok",
        None,
        &current_status,
    ))
}

pub async fn record_group_message_confirmation(
    pool: &PgPool,
    mut request: GroupMessageConfirmRequest,
    apply_requested: bool,
    policy: &OperationsPolicy,
) -> Result<GroupMessageConfirmReport> {
    normalize_group_message_confirm_request(&mut request);
    validate_group_message_confirm_request(&request)?;
    if let Err(err) = validate_confirmer_authorized(&request.confirmer_id, policy) {
        if apply_requested {
            append_group_confirmation_policy_denial(
                pool,
                &request,
                "group message final confirmation denied by confirmer allowlist",
                &err.to_string(),
            )
            .await?;
        }
        return Err(err);
    }

    if !apply_requested {
        let current_status = status_after_group_message_confirmation(&request);
        return Ok(group_message_confirm_report(
            &request,
            false,
            "dry_run_ok",
            None,
            &current_status,
        ));
    }

    let mut tx = pool
        .begin()
        .await
        .context("begin group message confirmation transaction")?;
    let row = sqlx::query(
        r#"
        SELECT id, status, work_item_type, capability_key, review_policy
        FROM qintopia_agent_os.work_items
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(request.work_item_id)
    .fetch_optional(&mut *tx)
    .await
    .context("load group message work item for confirmation")?
    .ok_or_else(|| anyhow::anyhow!("work_item_id is not found"))?;

    let previous_status: String = row.get("status");
    let work_item_type: String = row.get("work_item_type");
    let capability_key: String = row.get("capability_key");
    let review_policy: String = row.get("review_policy");
    validate_group_message_confirm_work_item(
        &previous_status,
        &work_item_type,
        &capability_key,
        &review_policy,
    )?;

    let current_status = status_after_group_message_confirmation(&request);
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = $2,
            updated_at = now(),
            metadata = metadata || $3
        WHERE id = $1
        "#,
    )
    .bind(request.work_item_id)
    .bind(&current_status)
    .bind(json!({
        "group_message_confirmation": {
            "decision": request.decision,
            "confirmer_id": request.confirmer_id,
            "source": request.source
        }
    }))
    .execute(&mut *tx)
    .await
    .context("update group message work item confirmation status")?;

    append_event_in_tx(
        &mut tx,
        Some(request.work_item_id),
        None,
        "group_message_final_confirmation_recorded",
        "human",
        &request.confirmer_id,
        "group message final confirmation recorded",
        json!({
            "previous_status": previous_status,
            "current_status": current_status,
            "decision": request.decision,
            "reason": request.reason,
            "source": request.source,
            "metadata": request.metadata,
            "send_executed": false,
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit group message confirmation transaction")?;

    Ok(group_message_confirm_report(
        &request,
        true,
        "confirmation_recorded",
        Some(previous_status),
        &current_status,
    ))
}

pub fn record_workbench_event_dry_run(
    mut request: WorkbenchEventRecordRequest,
) -> Result<WorkbenchEventRecordReport> {
    normalize_workbench_event_request(&mut request);
    validate_workbench_event_request(&request)?;
    Ok(workbench_event_report(&request, false, "dry_run_ok"))
}

pub async fn record_workbench_event(
    pool: &PgPool,
    mut request: WorkbenchEventRecordRequest,
    apply_requested: bool,
) -> Result<WorkbenchEventRecordReport> {
    normalize_workbench_event_request(&mut request);
    validate_workbench_event_request(&request)?;

    if !apply_requested {
        return Ok(workbench_event_report(&request, false, "dry_run_ok"));
    }

    let mut tx = pool
        .begin()
        .await
        .context("begin workbench event transaction")?;
    let work_item_exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM qintopia_agent_os.work_items
            WHERE id = $1
        )
        "#,
    )
    .bind(request.work_item_id)
    .fetch_one(&mut *tx)
    .await
    .context("check work item for workbench event")?;
    if !work_item_exists {
        bail!("work_item_id is not found");
    }

    let workbench_ref_exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM qintopia_agent_os.human_workbench_refs
            WHERE work_item_id = $1
              AND provider = $2
              AND external_id = $3
              AND status = 'active'
        )
        "#,
    )
    .bind(request.work_item_id)
    .bind(&request.provider)
    .bind(&request.external_id)
    .fetch_one(&mut *tx)
    .await
    .context("check active workbench ref for workbench event")?;
    if !workbench_ref_exists {
        bail!("active human_workbench_ref is not found for provider/external_id/work_item_id");
    }

    if let Some(artifact_id) = request.artifact_id {
        let artifact_matches: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM qintopia_agent_os.artifacts
                WHERE id = $1 AND work_item_id = $2
            )
            "#,
        )
        .bind(artifact_id)
        .bind(request.work_item_id)
        .fetch_one(&mut *tx)
        .await
        .context("check artifact for workbench event")?;
        if !artifact_matches {
            bail!("artifact_id does not belong to work_item_id");
        }
    }

    if !request.external_event_id.is_empty() {
        let already_recorded: bool = sqlx::query_scalar(
            r#"
            SELECT EXISTS(
                SELECT 1
                FROM qintopia_agent_os.work_item_events
                WHERE event_type = 'human_workbench_event_recorded'
                  AND data->>'provider' = $1
                  AND data->>'external_id' = $2
                  AND data->>'external_event_id' = $3
            )
            "#,
        )
        .bind(&request.provider)
        .bind(&request.external_id)
        .bind(&request.external_event_id)
        .fetch_one(&mut *tx)
        .await
        .context("check duplicate workbench event")?;
        if already_recorded {
            tx.commit()
                .await
                .context("commit idempotent workbench event transaction")?;
            return Ok(workbench_event_report(
                &request,
                true,
                "idempotent_existing",
            ));
        }
    }

    append_event_in_tx(
        &mut tx,
        Some(request.work_item_id),
        request.artifact_id,
        "human_workbench_event_recorded",
        "human",
        &request.actor_id,
        "human workbench event recorded after validation",
        json!({
            "provider": request.provider,
            "external_id": request.external_id,
            "external_event_id": request.external_event_id,
            "workbench_event_type": request.event_type,
            "comment_text": request.comment_text,
            "requested_status": request.requested_status,
            "review_decision": request.review_decision,
            "confirmation_decision": request.confirmation_decision,
            "source": request.source,
            "metadata": request.metadata,
            "mutates_work_item_state": false,
            "recommended_command": recommended_command_for_workbench_event(&request),
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit workbench event transaction")?;

    Ok(workbench_event_report(&request, true, "event_recorded"))
}

pub async fn process_workbench_event(
    pool: &PgPool,
    event_id: Uuid,
    apply_requested: bool,
    policy: &OperationsPolicy,
) -> Result<WorkbenchEventProcessReport> {
    let event = load_workbench_event(pool, event_id).await?;
    validate_processable_workbench_event(&event)?;
    if workbench_event_processed(pool, event_id).await? {
        return Ok(workbench_event_process_report(
            &event,
            apply_requested,
            "idempotent_existing",
            Some(command_for_recorded_workbench_event(&event)?),
            false,
        ));
    }
    if !apply_requested {
        return Ok(workbench_event_process_report(
            &event,
            false,
            "dry_run_ok",
            Some(command_for_recorded_workbench_event(&event)?),
            false,
        ));
    }

    let command = command_for_recorded_workbench_event(&event)?;
    match command.as_str() {
        "operations-artifact-review-decision" => {
            let artifact_id = event
                .artifact_id
                .ok_or_else(|| anyhow::anyhow!("artifact_id is required for review events"))?;
            let request = ArtifactReviewDecisionRequest {
                artifact_id,
                reviewer_id: event.actor_id.clone(),
                decision: event.review_decision.clone(),
                reason: non_empty_or_default(
                    &event.comment_text,
                    "workbench review event processed",
                ),
                source: format!("workbench_event:{}", event.id),
                metadata: json!({
                    "workbench_event_id": event.id,
                    "provider": event.provider,
                    "external_id": event.external_id,
                    "external_event_id": event.external_event_id,
                }),
            };
            record_artifact_review_decision(pool, request, true, policy).await?;
        }
        "operations-group-message-confirm" => {
            let request = GroupMessageConfirmRequest {
                work_item_id: event.work_item_id,
                confirmer_id: event.actor_id.clone(),
                decision: event.confirmation_decision.clone(),
                reason: non_empty_or_default(
                    &event.comment_text,
                    "workbench final confirmation event processed",
                ),
                source: format!("workbench_event:{}", event.id),
                metadata: json!({
                    "workbench_event_id": event.id,
                    "provider": event.provider,
                    "external_id": event.external_id,
                    "external_event_id": event.external_event_id,
                }),
            };
            record_group_message_confirmation(pool, request, true, policy).await?;
        }
        "operations-workbench-status-change" => {
            record_workbench_status_change(pool, &event).await?;
        }
        "operations-workbench-owner-change" => {
            record_workbench_owner_change(pool, &event, policy).await?;
        }
        "operations-workbench-attachment-add" => {
            record_workbench_attachment(pool, &event, policy).await?;
        }
        _ => bail!("workbench event has no processable command"),
    }

    let mut tx = pool
        .begin()
        .await
        .context("begin workbench event processed transaction")?;
    append_event_in_tx(
        &mut tx,
        Some(event.work_item_id),
        event.artifact_id,
        "human_workbench_event_processed",
        "system",
        "operations-workbench-event-process",
        "human workbench event processed through a policy-checked AgentOS command",
        json!({
            "source_event_id": event.id,
            "workbench_event_type": event.workbench_event_type,
            "command_executed": command,
            "state_mutation_recorded": true,
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit workbench event processed transaction")?;

    Ok(workbench_event_process_report(
        &event,
        true,
        "processed",
        Some(command),
        true,
    ))
}

async fn record_workbench_status_change(
    pool: &PgPool,
    event: &RecordedWorkbenchEvent,
) -> Result<()> {
    validate_workbench_status_change_event(event)?;
    let mut tx = pool
        .begin()
        .await
        .context("begin workbench status change transaction")?;
    let row = sqlx::query(
        r#"
        SELECT id, status
        FROM qintopia_agent_os.work_items
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(event.work_item_id)
    .fetch_optional(&mut *tx)
    .await
    .context("load work item for workbench status change")?
    .ok_or_else(|| anyhow::anyhow!("work_item_id is not found"))?;
    let previous_status: String = row.get("status");
    if let Err(err) =
        validate_workbench_status_transition(&previous_status, &event.requested_status)
    {
        append_event_in_tx(
            &mut tx,
            Some(event.work_item_id),
            event.artifact_id,
            "denied_by_policy",
            "human",
            &event.actor_id,
            "human workbench status change denied by transition policy",
            json!({
                "reason": err.to_string(),
                "policy": "workbench_status_change_transition",
                "source_event_id": event.id,
                "previous_status": previous_status,
                "requested_status": event.requested_status,
                "provider": event.provider,
                "external_id": event.external_id,
                "external_event_id": event.external_event_id,
            }),
        )
        .await?;
        tx.commit()
            .await
            .context("commit denied workbench status change transaction")?;
        return Err(err);
    }

    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = $2,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = $3,
            metadata = metadata || $4,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(event.work_item_id)
    .bind(&event.requested_status)
    .bind(trim_error(&event.comment_text))
    .bind(json!({
        "workbench_status_change": {
            "source_event_id": event.id,
            "actor_id": event.actor_id,
            "provider": event.provider,
            "external_id": event.external_id,
            "external_event_id": event.external_event_id,
        }
    }))
    .execute(&mut *tx)
    .await
    .context("update work item from workbench status change")?;

    append_event_in_tx(
        &mut tx,
        Some(event.work_item_id),
        event.artifact_id,
        "workbench_status_change_recorded",
        "human",
        &event.actor_id,
        "human workbench status change recorded after policy validation",
        json!({
            "source_event_id": event.id,
            "previous_status": previous_status,
            "current_status": event.requested_status,
            "comment_text": event.comment_text,
            "provider": event.provider,
            "external_id": event.external_id,
            "external_event_id": event.external_event_id,
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit workbench status change transaction")?;
    Ok(())
}

async fn record_workbench_owner_change(
    pool: &PgPool,
    event: &RecordedWorkbenchEvent,
    policy: &OperationsPolicy,
) -> Result<()> {
    let new_human_owner = workbench_event_new_human_owner(event)?;
    let mut tx = pool
        .begin()
        .await
        .context("begin workbench owner change transaction")?;
    let row = sqlx::query(
        r#"
        SELECT id, human_owner
        FROM qintopia_agent_os.work_items
        WHERE id = $1
        FOR UPDATE
        "#,
    )
    .bind(event.work_item_id)
    .fetch_optional(&mut *tx)
    .await
    .context("load work item for workbench owner change")?
    .ok_or_else(|| anyhow::anyhow!("work_item_id is not found"))?;
    let previous_human_owner: String = row.get("human_owner");
    if !policy.owner_allowed(&new_human_owner) {
        let reason = "human_owner is not allowed for workbench owner changes";
        append_event_in_tx(
            &mut tx,
            Some(event.work_item_id),
            event.artifact_id,
            "denied_by_policy",
            "human",
            &event.actor_id,
            "human workbench owner change denied by owner allowlist",
            json!({
                "reason": reason,
                "policy": "workbench_owner_allowlist",
                "source_event_id": event.id,
                "previous_human_owner": previous_human_owner,
                "requested_human_owner": new_human_owner,
                "provider": event.provider,
                "external_id": event.external_id,
                "external_event_id": event.external_event_id,
            }),
        )
        .await?;
        tx.commit()
            .await
            .context("commit denied workbench owner change transaction")?;
        bail!("{reason}");
    }

    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            human_owner = $2,
            metadata = metadata || $3,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(event.work_item_id)
    .bind(&new_human_owner)
    .bind(json!({
        "workbench_owner_change": {
            "source_event_id": event.id,
            "actor_id": event.actor_id,
            "provider": event.provider,
            "external_id": event.external_id,
            "external_event_id": event.external_event_id,
        }
    }))
    .execute(&mut *tx)
    .await
    .context("update work item human owner from workbench event")?;

    append_event_in_tx(
        &mut tx,
        Some(event.work_item_id),
        event.artifact_id,
        "workbench_owner_change_recorded",
        "human",
        &event.actor_id,
        "human workbench owner change recorded after policy validation",
        json!({
            "source_event_id": event.id,
            "previous_human_owner": previous_human_owner,
            "human_owner": new_human_owner,
            "comment_text": event.comment_text,
            "provider": event.provider,
            "external_id": event.external_id,
            "external_event_id": event.external_event_id,
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit workbench owner change transaction")?;
    Ok(())
}

async fn record_workbench_attachment(
    pool: &PgPool,
    event: &RecordedWorkbenchEvent,
    policy: &OperationsPolicy,
) -> Result<()> {
    let attachment = workbench_event_attachment(event)?;
    let mut tx = pool
        .begin()
        .await
        .context("begin workbench attachment transaction")?;
    let work_item_exists: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS(
            SELECT 1
            FROM qintopia_agent_os.work_items
            WHERE id = $1
        )
        "#,
    )
    .bind(event.work_item_id)
    .fetch_one(&mut *tx)
    .await
    .context("check work item for workbench attachment")?;
    if !work_item_exists {
        bail!("work_item_id is not found");
    }
    let attachment_host = attachment_uri_host(&attachment.uri)?;
    if !policy.attachment_host_allowed(&attachment_host) {
        let reason = "attachment_uri host is not allowed for workbench attachments";
        append_event_in_tx(
            &mut tx,
            Some(event.work_item_id),
            event.artifact_id,
            "denied_by_policy",
            "human",
            &event.actor_id,
            "human workbench attachment denied by attachment host allowlist",
            json!({
                "reason": reason,
                "policy": "workbench_attachment_host_allowlist",
                "source_event_id": event.id,
                "attachment_host": attachment_host,
                "attachment_uri": attachment.uri,
                "provider": event.provider,
                "external_id": event.external_id,
                "external_event_id": event.external_event_id,
            }),
        )
        .await?;
        tx.commit()
            .await
            .context("commit denied workbench attachment transaction")?;
        bail!("{reason}");
    }

    let content_hash = content_hash_text(&format!(
        "{}|workbench_attachment|{}|{}|{}",
        event.work_item_id, attachment.title, attachment.summary, attachment.uri
    ));
    let row = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.artifacts
            (
                work_item_id,
                artifact_type,
                review_status,
                created_by_agent,
                title,
                summary,
                content_text,
                artifact_uri,
                content_hash,
                source_ids,
                information_class,
                metadata,
                review_requested_at
            )
        VALUES
            ($1, 'workbench_attachment', 'pending', 'human_workbench',
             $2, $3, $4, $5, $6, $7, 'internal_ops', $8, now())
        ON CONFLICT (work_item_id, content_hash) WHERE content_hash IS NOT NULL AND content_hash <> ''
        DO UPDATE SET
            title = EXCLUDED.title,
            summary = EXCLUDED.summary,
            content_text = EXCLUDED.content_text,
            artifact_uri = EXCLUDED.artifact_uri,
            metadata = qintopia_agent_os.artifacts.metadata || EXCLUDED.metadata,
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(event.work_item_id)
    .bind(&attachment.title)
    .bind(&attachment.summary)
    .bind(&attachment.content_text)
    .bind(&attachment.uri)
    .bind(&content_hash)
    .bind(json!([{
        "provider": event.provider,
        "external_id": event.external_id,
        "external_event_id": event.external_event_id,
    }]))
    .bind(json!({
        "source_event_id": event.id,
        "actor_id": event.actor_id,
        "provider": event.provider,
        "external_id": event.external_id,
        "external_event_id": event.external_event_id,
        "attachment_metadata": attachment.metadata,
    }))
    .fetch_one(&mut *tx)
    .await
    .context("upsert workbench attachment artifact")?;
    let artifact_id: Uuid = row.get("id");

    append_event_in_tx(
        &mut tx,
        Some(event.work_item_id),
        Some(artifact_id),
        "workbench_attachment_artifact_recorded",
        "human",
        &event.actor_id,
        "human workbench attachment recorded as pending artifact",
        json!({
            "source_event_id": event.id,
            "artifact_type": "workbench_attachment",
            "review_status": "pending",
            "title": attachment.title,
            "artifact_uri": attachment.uri,
            "content_hash": content_hash,
            "provider": event.provider,
            "external_id": event.external_id,
            "external_event_id": event.external_event_id,
            "does_not_publish": true,
            "does_not_send": true,
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit workbench attachment transaction")?;
    Ok(())
}

async fn run_workbench_event_worker_once(
    pool: &PgPool,
    event_id: Option<Uuid>,
    apply_requested: bool,
    policy: &OperationsPolicy,
) -> Result<WorkbenchEventWorkerReport> {
    let event_id = match event_id {
        Some(event_id) => Some(event_id),
        None => next_processable_workbench_event_id(pool).await?,
    };
    let Some(event_id) = event_id else {
        return Ok(workbench_event_worker_report(
            apply_requested,
            "no_processable_workbench_event",
            None,
            None,
        ));
    };

    let process_report = process_workbench_event(pool, event_id, apply_requested, policy).await?;
    let action_status = if apply_requested {
        match process_report.action_status.as_str() {
            "processed" => "processed",
            "idempotent_existing" => "idempotent_existing",
            _ => "validated",
        }
    } else {
        "dry_run_ok"
    };
    Ok(workbench_event_worker_report(
        apply_requested,
        action_status,
        Some(event_id),
        Some(process_report),
    ))
}

async fn next_processable_workbench_event_id(pool: &PgPool) -> Result<Option<Uuid>> {
    let row = sqlx::query(
        r#"
        SELECT e.id
        FROM qintopia_agent_os.work_item_events e
        WHERE e.event_type = 'human_workbench_event_recorded'
          AND e.data->>'workbench_event_type' IN (
              'review_decision_requested',
              'final_confirmation_requested',
              'status_change_requested',
              'owner_changed',
              'attachment_added'
          )
          AND NOT EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_item_events processed
              WHERE processed.event_type = 'human_workbench_event_processed'
                AND processed.data->>'source_event_id' = e.id::text
          )
          AND NOT EXISTS (
              SELECT 1
              FROM qintopia_agent_os.work_item_events denied
              WHERE denied.event_type = 'denied_by_policy'
                AND denied.data->>'source_event_id' = e.id::text
          )
        ORDER BY e.created_at ASC
        LIMIT 1
        "#,
    )
    .fetch_optional(pool)
    .await
    .context("load next processable workbench event")?;
    Ok(row.map(|row| row.get("id")))
}

impl OperationsPolicy {
    pub fn from_cli(cli: &Cli, require_approved_artifact_lookup: bool) -> Self {
        Self {
            allowed_group_aliases: split_csv(&cli.operations_allowed_group_aliases),
            allowed_group_ids: split_csv(&cli.operations_allowed_group_ids),
            allowed_reviewer_ids: normalize_actor_ids(cli.operations_allowed_reviewer_ids()),
            allowed_confirmer_ids: normalize_actor_ids(cli.operations_allowed_confirmer_ids()),
            allowed_owner_ids: normalize_actor_ids(cli.operations_allowed_owner_ids()),
            allowed_attachment_hosts: normalize_hosts(cli.operations_allowed_attachment_hosts()),
            require_approved_artifact_lookup,
        }
    }

    pub fn dry_run() -> Self {
        Self {
            allowed_group_aliases: DRY_RUN_ALLOWED_GROUP_ALIASES
                .iter()
                .map(|item| item.to_string())
                .collect(),
            allowed_group_ids: DRY_RUN_ALLOWED_GROUP_IDS
                .iter()
                .map(|item| item.to_string())
                .collect(),
            allowed_reviewer_ids: Vec::new(),
            allowed_confirmer_ids: Vec::new(),
            allowed_owner_ids: Vec::new(),
            allowed_attachment_hosts: Vec::new(),
            require_approved_artifact_lookup: false,
        }
    }

    fn group_allowed(&self, alias: Option<&str>, group_id: Option<&str>) -> bool {
        alias
            .map(normalize_key)
            .filter(|item| {
                self.allowed_group_aliases
                    .iter()
                    .any(|allowed| allowed == item)
            })
            .is_some()
            || group_id
                .map(str::trim)
                .filter(|item| {
                    self.allowed_group_ids
                        .iter()
                        .any(|allowed| allowed == *item)
                })
                .is_some()
    }

    fn reviewer_allowed(&self, reviewer_id: &str) -> bool {
        actor_allowed(&self.allowed_reviewer_ids, reviewer_id)
    }

    fn confirmer_allowed(&self, confirmer_id: &str) -> bool {
        actor_allowed(&self.allowed_confirmer_ids, confirmer_id)
    }

    fn owner_allowed(&self, owner_id: &str) -> bool {
        actor_allowed(&self.allowed_owner_ids, owner_id)
    }

    fn attachment_host_allowed(&self, host: &str) -> bool {
        self.allowed_attachment_hosts.is_empty()
            || self
                .allowed_attachment_hosts
                .iter()
                .any(|allowed| allowed == &normalize_host(host))
    }
}

fn readiness_report(cli: &Cli, profile: &str, strict: bool) -> Result<OperationsReadinessReport> {
    let normalized_profile = normalize_key(profile);
    if !matches!(normalized_profile.as_str(), "production" | "apply_smoke") {
        bail!("readiness profile must be production or apply_smoke");
    }

    let database_url_configured = cli
        .database_url
        .as_ref()
        .filter(|value| !value.trim().is_empty())
        .is_some();
    let group_alias_count = split_csv(&cli.operations_allowed_group_aliases).len();
    let group_id_count = split_csv(&cli.operations_allowed_group_ids).len();
    let reviewer_ids = normalize_actor_ids(cli.operations_allowed_reviewer_ids());
    let confirmer_ids = normalize_actor_ids(cli.operations_allowed_confirmer_ids());
    let owner_ids = normalize_actor_ids(cli.operations_allowed_owner_ids());
    let reviewer_count = reviewer_ids.len();
    let confirmer_count = confirmer_ids.len();
    let owner_count = owner_ids.len();
    let reviewer_ids_human = actor_ids_all_human(&reviewer_ids);
    let confirmer_ids_human = actor_ids_all_human(&confirmer_ids);
    let owner_ids_human = actor_ids_all_human(&owner_ids);
    let attachment_host_count = normalize_hosts(cli.operations_allowed_attachment_hosts()).len();
    let apply_smoke_enabled = std::env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE")
        .map(|value| value.trim() == "1")
        .unwrap_or(false);

    let checks = vec![
        readiness_check(
            "postgres_database_url",
            database_url_configured,
            usize::from(database_url_configured),
            &["apply_smoke", "production"],
            "QINTOPIA_SIDECAR_DATABASE_URL is required for migrations, apply smoke, and production workers",
        ),
        readiness_check(
            "apply_smoke_enable",
            apply_smoke_enabled,
            if apply_smoke_enabled { 1 } else { 0 },
            &["apply_smoke"],
            "QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1 is required only when intentionally writing AgentOS test rows",
        ),
        readiness_check(
            "allowed_group_targets",
            group_alias_count + group_id_count > 0,
            group_alias_count + group_id_count,
            &["production"],
            "QINTOPIA_OPERATIONS_ALLOWED_GROUP_ALIASES or QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS must be configured before Erhua send adapters are enabled",
        ),
        readiness_check(
            "allowed_reviewers",
            reviewer_count > 0 && reviewer_ids_human,
            reviewer_count,
            &["production"],
            "QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS must contain real human actor ids, not app/bot ids, before real artifact review sync is trusted",
        ),
        readiness_check(
            "allowed_confirmers",
            confirmer_count > 0 && confirmer_ids_human,
            confirmer_count,
            &["production"],
            "QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS must contain real human actor ids, not app/bot ids, before external send confirmation is trusted",
        ),
        readiness_check(
            "allowed_owners",
            owner_count > 0 && owner_ids_human,
            owner_count,
            &["production"],
            "QINTOPIA_OPERATIONS_ALLOWED_OWNER_IDS must contain real human actor ids, not app/bot ids, before workbench owner changes are trusted",
        ),
        readiness_check(
            "allowed_attachment_hosts",
            attachment_host_count > 0,
            attachment_host_count,
            &["production"],
            "QINTOPIA_OPERATIONS_ALLOWED_ATTACHMENT_HOSTS must be configured before workbench attachments are accepted from real Feishu events",
        ),
    ];

    let missing_required: Vec<String> = checks
        .iter()
        .filter(|check| {
            check.status != "ok"
                && check
                    .required_for
                    .iter()
                    .any(|item| item == &normalized_profile)
        })
        .map(|check| check.key.clone())
        .collect();

    let ready_for_apply_smoke = checks
        .iter()
        .filter(|check| check.required_for.iter().any(|item| item == "apply_smoke"))
        .all(|check| check.status == "ok");
    let ready_for_production_adapters = checks
        .iter()
        .filter(|check| check.required_for.iter().any(|item| item == "production"))
        .all(|check| check.status == "ok");

    let success = missing_required.is_empty();
    let mut warnings = vec![
        "This check is read-only and does not verify live Feishu, Huabaosi, Wenyuange, QiWe, or Erhua adapter credentials.".to_string(),
        "Do not treat dry-run workbench refs as proof that real Feishu Task API integration is configured.".to_string(),
    ];
    if normalized_profile == "production" && apply_smoke_enabled {
        warnings.push(
            "QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1 is set; unset it after intentional smoke writes to avoid accidental test-row creation.".to_string(),
        );
    }

    let next_actions = if success {
        vec![
            "Run scripts/operations-control-plane-smoke.sh before deployment.".to_string(),
            "Run scripts/operations-control-plane-apply-smoke.sh only with explicit production/staging approval.".to_string(),
            "Keep real external adapters disabled until their credentials and scoped permissions are separately verified.".to_string(),
        ]
    } else {
        missing_required
            .iter()
            .map(|key| readiness_next_action(key))
            .collect()
    };

    Ok(OperationsReadinessReport {
        success,
        action_status: if success {
            "ready".to_string()
        } else {
            "missing_required_configuration".to_string()
        },
        profile: normalized_profile,
        strict,
        ready_for_production_adapters,
        ready_for_apply_smoke,
        checks,
        missing_required,
        warnings,
        next_actions,
    })
}

fn readiness_check(
    key: &str,
    configured: bool,
    configured_count: usize,
    required_for: &[&str],
    detail: &str,
) -> OperationsReadinessCheck {
    OperationsReadinessCheck {
        key: key.to_string(),
        status: if configured { "ok" } else { "missing" }.to_string(),
        required_for: required_for.iter().map(|item| item.to_string()).collect(),
        configured,
        configured_count,
        detail: detail.to_string(),
    }
}

fn readiness_next_action(key: &str) -> String {
    match key {
        "postgres_database_url" => {
            "Configure QINTOPIA_SIDECAR_DATABASE_URL from the server env before apply smoke or production workers.".to_string()
        }
        "apply_smoke_enable" => {
            "Set QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1 only for the intentional guarded Postgres apply smoke run.".to_string()
        }
        "allowed_group_targets" => {
            "Configure QINTOPIA_OPERATIONS_ALLOWED_GROUP_ALIASES or QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS for approved Erhua send targets.".to_string()
        }
        "allowed_reviewers" => {
            "Configure QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS with production reviewer identities.".to_string()
        }
        "allowed_confirmers" => {
            "Configure QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS with production final confirmer identities.".to_string()
        }
        "allowed_owners" => {
            "Configure QINTOPIA_OPERATIONS_ALLOWED_OWNER_IDS with workbench-assignable owner identities.".to_string()
        }
        "allowed_attachment_hosts" => {
            "Configure QINTOPIA_OPERATIONS_ALLOWED_ATTACHMENT_HOSTS with trusted HTTPS attachment hosts.".to_string()
        }
        _ => format!("Resolve missing readiness check: {key}"),
    }
}

fn split_csv(value: &str) -> Vec<String> {
    value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(normalize_key)
        .collect()
}

fn normalize_actor_ids(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|item| normalize_key(&item))
        .filter(|item| !item.is_empty())
        .collect()
}

fn normalize_hosts(values: Vec<String>) -> Vec<String> {
    values
        .into_iter()
        .map(|item| normalize_host(&item))
        .filter(|item| !item.is_empty())
        .collect()
}

fn normalize_host(value: &str) -> String {
    value.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn actor_allowed(allowed_actor_ids: &[String], actor_id: &str) -> bool {
    allowed_actor_ids.is_empty()
        || allowed_actor_ids
            .iter()
            .any(|item| item == &normalize_key(actor_id))
}

fn is_human_actor_id(actor_id: &str) -> bool {
    let normalized = normalize_key(actor_id);
    if normalized.is_empty() {
        return true;
    }
    !(normalized.starts_with("cli_")
        || normalized.starts_with("app_")
        || normalized.starts_with("bot_")
        || normalized == "system"
        || normalized.starts_with("system-")
        || normalized.starts_with("system_")
        || normalized.starts_with("system:")
        || normalized == "service"
        || normalized.starts_with("service-")
        || normalized.starts_with("service_")
        || normalized.starts_with("service:")
        || normalized == "worker"
        || normalized.starts_with("worker-")
        || normalized.starts_with("worker_")
        || normalized.starts_with("worker:"))
}

fn validate_human_actor_id(field: &str, actor_id: &str) -> Result<()> {
    if !is_human_actor_id(actor_id) {
        bail!("{field} must be a human actor id, not a bot/app/service identity");
    }
    Ok(())
}

fn actor_ids_all_human(actor_ids: &[String]) -> bool {
    actor_ids.iter().all(|actor_id| is_human_actor_id(actor_id))
}

pub fn create_work_item_dry_run(
    mut request: WorkItemCreateRequest,
) -> Result<WorkItemCreateReport> {
    normalize_request(&mut request);
    let capability = builtin_capability(&request.capability_key)
        .ok_or_else(|| anyhow::anyhow!("capability is not registered"))?;
    let policy = OperationsPolicy::dry_run();
    validate_request(&request, &capability, &policy)?;
    let current_status = initial_status_for(&request, &capability);
    Ok(report_from_request(
        &request,
        &capability,
        false,
        false,
        "dry_run_ok",
        None,
        false,
        &current_status,
    ))
}

pub fn plan_request(mut input: RequestPlanInput) -> Result<RequestPlanReport> {
    normalize_plan_input(&mut input);
    validate_plan_input(&input)?;

    let Some(request) = planned_request_from_input(&input)? else {
        return Ok(request_plan_needs_clarification(
            &input,
            vec![
                "请明确你希望哪个 Agent 做什么，例如生成海报、检索资料，或发送已审核内容到哪个白名单群。"
                    .to_string(),
            ],
        ));
    };

    let preview = create_work_item_dry_run(request.clone())?;
    let capability = builtin_capability(&request.capability_key)
        .map(|capability| capability_list_item(&capability));
    Ok(RequestPlanReport {
        success: true,
        action_status: "planned".to_string(),
        planner: "operations-request-plan",
        requester_agent: request.requester_agent.clone(),
        original_request: input.request_text,
        selected_capability: capability,
        work_item_request: Some(work_item_request_preview(&request)),
        work_item_preview: Some(preview),
        clarification_questions: Vec::new(),
        limitations: request_plan_limitations(),
        guardrails: request_plan_guardrails(),
    })
}

pub fn submit_request_dry_run(input: RequestPlanInput) -> Result<RequestSubmitReport> {
    let plan = plan_request(input)?;
    Ok(request_submit_report_from_plan(plan, false, None))
}

pub async fn submit_request(
    pool: &PgPool,
    mut input: RequestPlanInput,
    apply_requested: bool,
    policy: &OperationsPolicy,
) -> Result<RequestSubmitReport> {
    normalize_plan_input(&mut input);
    validate_plan_input(&input)?;

    let Some(request) = planned_request_from_input(&input)? else {
        let plan = request_plan_needs_clarification(
            &input,
            vec![
                "请明确你希望哪个 Agent 做什么，例如生成海报、检索资料，或发送已审核内容到哪个白名单群。"
                    .to_string(),
            ],
        );
        return Ok(request_submit_report_from_plan(plan, apply_requested, None));
    };
    let selected_capability = builtin_capability(&request.capability_key)
        .map(|capability| capability_list_item(&capability));
    let result = create_work_item(pool, request.clone(), apply_requested, policy).await?;
    Ok(RequestSubmitReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        action_status: if apply_requested {
            result.action_status.clone()
        } else {
            "dry_run_ok".to_string()
        },
        planner: "operations-request-submit",
        requester_agent: request.requester_agent.clone(),
        original_request: input.request_text,
        selected_capability,
        work_item_request: Some(work_item_request_preview(&request)),
        work_item_result: Some(result),
        clarification_questions: Vec::new(),
        limitations: request_submit_limitations(),
        guardrails: request_submit_guardrails(),
    })
}

pub fn start_workflow_dry_run(mut request: WorkflowStartRequest) -> Result<WorkflowStartReport> {
    normalize_workflow_start_request(&mut request);
    validate_workflow_start_request(&request)?;
    let (parent_request, child_requests) = workflow_work_item_requests(&request, None);
    let parent_report = create_work_item_dry_run(parent_request)?;
    let child_reports = child_requests
        .into_iter()
        .map(create_work_item_dry_run)
        .collect::<Result<Vec<_>>>()?;
    Ok(workflow_start_report(
        &request,
        false,
        "dry_run_ok",
        parent_report,
        child_reports,
    ))
}

pub async fn start_workflow(
    pool: &PgPool,
    mut request: WorkflowStartRequest,
    apply_requested: bool,
    policy: &OperationsPolicy,
) -> Result<WorkflowStartReport> {
    normalize_workflow_start_request(&mut request);
    validate_workflow_start_request(&request)?;
    if !apply_requested {
        return start_workflow_dry_run(request);
    }

    let (parent_request, _) = workflow_work_item_requests(&request, None);
    let parent_report = create_work_item(pool, parent_request, true, policy).await?;
    let parent_work_item_id = parent_report
        .work_item_id
        .ok_or_else(|| anyhow::anyhow!("parent work item id missing after create"))?;
    let (_, child_requests) = workflow_work_item_requests(&request, Some(parent_work_item_id));
    let mut child_reports = Vec::new();
    for child_request in child_requests {
        child_reports.push(create_work_item(pool, child_request, true, policy).await?);
    }
    let all_children_existing = child_reports.iter().all(|report| report.existing);
    let action_status = if parent_report.existing && all_children_existing {
        "idempotent_existing"
    } else {
        "created"
    };
    Ok(workflow_start_report(
        &request,
        true,
        action_status,
        parent_report,
        child_reports,
    ))
}

pub async fn create_work_item(
    pool: &PgPool,
    mut request: WorkItemCreateRequest,
    apply_requested: bool,
    policy: &OperationsPolicy,
) -> Result<WorkItemCreateReport> {
    normalize_request(&mut request);
    let capability = match load_capability(pool, &request.capability_key).await? {
        Some(capability) => capability,
        None => {
            if apply_requested {
                append_policy_denial(
                    pool,
                    &request,
                    None,
                    "work item rejected because capability is not registered",
                    "capability is not registered",
                )
                .await?;
            }
            bail!("capability is not registered");
        }
    };
    if let Err(err) = validate_request(&request, &capability, policy) {
        if apply_requested {
            append_policy_denial(
                pool,
                &request,
                Some(&capability),
                "work item rejected by capability policy",
                &err.to_string(),
            )
            .await?;
        }
        return Err(err);
    }
    if apply_requested {
        if let Err(err) = validate_apply_policy(pool, &request, &capability, policy).await {
            append_policy_denial(
                pool,
                &request,
                Some(&capability),
                "work item rejected by apply-time capability policy",
                &err.to_string(),
            )
            .await?;
            return Err(err);
        }
    }

    if !apply_requested {
        let current_status = initial_status_for(&request, &capability);
        return Ok(report_from_request(
            &request,
            &capability,
            false,
            false,
            "dry_run_ok",
            None,
            false,
            &current_status,
        ));
    }

    let mut tx = pool.begin().await.context("begin work item transaction")?;
    let initial_status = initial_status_for(&request, &capability);
    let existing = sqlx::query(
        r#"
        SELECT id, status
        FROM qintopia_agent_os.work_items
        WHERE idempotency_key = $1
        "#,
    )
    .bind(&request.idempotency_key)
    .fetch_optional(&mut *tx)
    .await
    .context("lookup work item idempotency key")?;

    let (work_item_id, current_status, existing) = if let Some(row) = existing {
        (
            row.get::<Uuid, _>("id"),
            row.get::<String, _>("status"),
            true,
        )
    } else {
        let row = sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (
                    parent_work_item_id,
                    work_item_type,
                    status,
                    requester_agent,
                    target_agent,
                    capability_key,
                    human_owner,
                    priority,
                    brief_summary,
                    purpose,
                    source_event_signal_id,
                    source_type,
                    source_refs,
                    dedupe_key,
                    idempotency_key,
                    risk_level,
                    information_class,
                    payload,
                    payload_redaction_policy,
                    review_policy,
                    metadata
                )
            VALUES
                ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14,
                 $15, $16, $17, $18, $19, $20, $21)
            RETURNING id
            "#,
        )
        .bind(request.parent_work_item_id)
        .bind(&request.work_item_type)
        .bind(&initial_status)
        .bind(&request.requester_agent)
        .bind(&request.target_agent)
        .bind(&request.capability_key)
        .bind(&request.human_owner)
        .bind(&request.priority)
        .bind(&request.brief_summary)
        .bind(&request.purpose)
        .bind(request.source_event_signal_id)
        .bind(&request.source_type)
        .bind(&request.source_refs)
        .bind(&request.dedupe_key)
        .bind(&request.idempotency_key)
        .bind(&capability.risk_level)
        .bind("internal_ops")
        .bind(&request.payload)
        .bind(&request.payload_redaction_policy)
        .bind(&capability.review_policy)
        .bind(&request.metadata)
        .fetch_one(&mut *tx)
        .await
        .context("insert work item")?;
        let work_item_id = row.get::<Uuid, _>("id");
        append_event_in_tx(
            &mut tx,
            Some(work_item_id),
            None,
            "created",
            "agent",
            &request.requester_agent,
            "work item created through capability request",
            json!({
                "capability_key": request.capability_key,
                "work_item_type": request.work_item_type,
                "status": initial_status,
                "target_agent": request.target_agent,
                "parent_work_item_id": request.parent_work_item_id,
                "human_workbench_provider": "feishu_task",
                "requires_human_final_confirmation": initial_status == "awaiting_publish"
            }),
        )
        .await?;
        (work_item_id, initial_status, false)
    };

    tx.commit().await.context("commit work item transaction")?;
    Ok(report_from_request(
        &request,
        &capability,
        false,
        true,
        if existing {
            "idempotent_existing"
        } else {
            "created"
        },
        Some(work_item_id),
        existing,
        &current_status,
    ))
}

async fn append_policy_denial(
    pool: &PgPool,
    request: &WorkItemCreateRequest,
    capability: Option<&Capability>,
    message: &str,
    reason: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
        VALUES (NULL, NULL, 'denied_by_policy', 'agent', $1, $2, $3)
        "#,
    )
    .bind(&request.requester_agent)
    .bind(message)
    .bind(json!({
        "reason": reason,
        "capability_key": request.capability_key,
        "capability_registered": capability.is_some(),
        "work_item_type": request.work_item_type,
        "requester_agent": request.requester_agent,
        "target_agent": request.target_agent,
        "source_type": request.source_type,
        "idempotency_key": request.idempotency_key,
        "dedupe_key": request.dedupe_key,
    }))
    .execute(pool)
    .await
    .context("append policy denial event")?;
    Ok(())
}

async fn append_review_policy_denial(
    pool: &PgPool,
    request: &ArtifactReviewDecisionRequest,
    work_item_id: Option<Uuid>,
    message: &str,
    reason: &str,
    policy: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
        VALUES ($1, $2, 'denied_by_policy', 'human', $3, $4, $5)
        "#,
    )
    .bind(work_item_id)
    .bind(request.artifact_id)
    .bind(&request.reviewer_id)
    .bind(message)
    .bind(json!({
        "reason": reason,
        "policy": policy,
        "artifact_id": request.artifact_id,
        "reviewer_id": request.reviewer_id,
        "decision": request.decision,
        "source": request.source,
        "metadata": request.metadata,
    }))
    .execute(pool)
    .await
    .context("append artifact review policy denial event")?;
    Ok(())
}

async fn append_group_confirmation_policy_denial(
    pool: &PgPool,
    request: &GroupMessageConfirmRequest,
    message: &str,
    reason: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
        VALUES ($1, NULL, 'denied_by_policy', 'human', $2, $3, $4)
        "#,
    )
    .bind(request.work_item_id)
    .bind(&request.confirmer_id)
    .bind(message)
    .bind(json!({
        "reason": reason,
        "policy": "group_message_confirmer_allowlist",
        "work_item_id": request.work_item_id,
        "confirmer_id": request.confirmer_id,
        "decision": request.decision,
        "source": request.source,
        "metadata": request.metadata,
    }))
    .execute(pool)
    .await
    .context("append group message confirmation policy denial event")?;
    Ok(())
}

async fn load_capability(pool: &PgPool, capability_key: &str) -> Result<Option<Capability>> {
    let Some(row) = sqlx::query(
        r#"
        SELECT capability_key, provider_agent, display_name, description,
               allowed_callers, allowed_work_item_types, risk_level, review_policy, enabled
        FROM qintopia_agent_os.capabilities
        WHERE capability_key = $1
        "#,
    )
    .bind(capability_key)
    .fetch_optional(pool)
    .await
    .context("load capability")?
    else {
        return Ok(None);
    };

    Ok(Some(Capability {
        capability_key: row.get("capability_key"),
        provider_agent: row.get("provider_agent"),
        display_name: row.get("display_name"),
        description: row.get("description"),
        allowed_callers: row.get("allowed_callers"),
        allowed_work_item_types: row.get("allowed_work_item_types"),
        risk_level: row.get("risk_level"),
        review_policy: row.get("review_policy"),
        enabled: row.get("enabled"),
    }))
}

async fn capability_list_from_db(pool: &PgPool) -> Result<CapabilityListReport> {
    let rows = sqlx::query(
        r#"
        SELECT capability_key, provider_agent, display_name, description,
               allowed_callers, allowed_work_item_types, risk_level, review_policy, enabled
        FROM qintopia_agent_os.capabilities
        ORDER BY provider_agent ASC, capability_key ASC
        "#,
    )
    .fetch_all(pool)
    .await
    .context("load capability list")?;
    let capabilities = rows
        .into_iter()
        .map(|row| {
            capability_list_item(&Capability {
                capability_key: row.get("capability_key"),
                provider_agent: row.get("provider_agent"),
                display_name: row.get("display_name"),
                description: row.get("description"),
                allowed_callers: row.get("allowed_callers"),
                allowed_work_item_types: row.get("allowed_work_item_types"),
                risk_level: row.get("risk_level"),
                review_policy: row.get("review_policy"),
                enabled: row.get("enabled"),
            })
        })
        .collect::<Vec<_>>();
    Ok(capability_list_report("postgres", capabilities))
}

fn capability_list_from_builtin() -> CapabilityListReport {
    let mut capabilities = BUILTIN_CAPABILITY_KEYS
        .iter()
        .filter_map(|key| builtin_capability(key))
        .map(|capability| capability_list_item(&capability))
        .collect::<Vec<_>>();
    capabilities.sort_by(|left, right| {
        left.provider_agent
            .cmp(&right.provider_agent)
            .then(left.capability_key.cmp(&right.capability_key))
    });
    capability_list_report("builtin", capabilities)
}

fn capability_list_report(
    source: &str,
    capabilities: Vec<CapabilityListItem>,
) -> CapabilityListReport {
    CapabilityListReport {
        success: true,
        source: source.to_string(),
        capability_count: capabilities.len(),
        capabilities,
        limitations: vec![
            "capability list is a discovery surface, not permission by itself".to_string(),
            "all execution must still create capability-governed work items".to_string(),
            "high-risk external send/publish capabilities require human review or final confirmation".to_string(),
        ],
        guardrails: vec![
            "do not use raw prompt handoff between Agents".to_string(),
            "Postgres remains the operations source of truth".to_string(),
            "payloads must use redacted summaries and source refs".to_string(),
        ],
    }
}

fn capability_list_item(capability: &Capability) -> CapabilityListItem {
    CapabilityListItem {
        capability_key: capability.capability_key.clone(),
        provider_agent: capability.provider_agent.clone(),
        display_name: capability.display_name.clone(),
        description: capability.description.clone(),
        allowed_callers: capability.allowed_callers.clone(),
        allowed_work_item_types: capability.allowed_work_item_types.clone(),
        risk_level: capability.risk_level.clone(),
        review_policy: capability.review_policy.clone(),
        enabled: capability.enabled,
        safe_for_non_technical_request: capability.enabled,
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "work item event columns are explicit at the shared transaction boundary"
)]
async fn append_event_in_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Option<Uuid>,
    artifact_id: Option<Uuid>,
    event_type: &str,
    actor_type: &str,
    actor_id: &str,
    message: &str,
    data: Value,
) -> Result<Uuid> {
    let row = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
        VALUES ($1, $2, $3, $4, $5, $6, $7)
        RETURNING id
        "#,
    )
    .bind(work_item_id)
    .bind(artifact_id)
    .bind(event_type)
    .bind(actor_type)
    .bind(actor_id)
    .bind(message)
    .bind(data)
    .fetch_one(&mut **tx)
    .await
    .context("append work item event")?;
    Ok(row.get("id"))
}

fn normalize_request(request: &mut WorkItemCreateRequest) {
    request.requester_agent = normalize_key(&request.requester_agent);
    request.target_agent = normalize_key(&request.target_agent);
    request.capability_key = request.capability_key.trim().to_string();
    request.work_item_type = normalize_key(&request.work_item_type);
    request.priority = normalize_key(&request.priority);
    request.payload_redaction_policy = normalize_key(&request.payload_redaction_policy);
    request.brief_summary = request.brief_summary.trim().to_string();
    request.purpose = request.purpose.trim().to_string();
    request.human_owner = request.human_owner.trim().to_string();
    request.source_type = normalize_key(&request.source_type);
    if request.source_refs.is_null() {
        request.source_refs = json!({});
    }
    if request.payload.is_null() {
        request.payload = json!({});
    }
    if request.metadata.is_null() {
        request.metadata = json!({});
    }
    if request.dedupe_key.trim().is_empty() {
        request.dedupe_key = dedupe_key(request);
    }
    if request.idempotency_key.trim().is_empty() {
        request.idempotency_key = request.dedupe_key.clone();
    } else {
        request.idempotency_key = request.idempotency_key.trim().to_string();
    }
}

fn initial_status_for(request: &WorkItemCreateRequest, capability: &Capability) -> String {
    if capability.capability_key == "erhua.send_group_message"
        && request.work_item_type == "group_message_request"
    {
        "awaiting_publish".to_string()
    } else {
        "queued".to_string()
    }
}

fn validate_request(
    request: &WorkItemCreateRequest,
    capability: &Capability,
    policy: &OperationsPolicy,
) -> Result<()> {
    require_non_empty("requester_agent", &request.requester_agent)?;
    require_non_empty("target_agent", &request.target_agent)?;
    require_non_empty("capability_key", &request.capability_key)?;
    require_non_empty("work_item_type", &request.work_item_type)?;
    require_non_empty("brief_summary", &request.brief_summary)?;
    if request.brief_summary.chars().count() > 500 {
        bail!("brief_summary must be 500 characters or fewer");
    }
    if !ALLOWED_WORK_ITEM_TYPES.contains(&request.work_item_type.as_str()) {
        bail!("work_item_type is not allowed");
    }
    if !ALLOWED_PRIORITIES.contains(&request.priority.as_str()) {
        bail!("priority is not allowed");
    }
    validate_source_policy(
        &request.source_type,
        &request.source_refs,
        request.source_event_signal_id,
    )?;
    if !capability.enabled {
        bail!("capability is disabled");
    }
    if capability.capability_key != request.capability_key {
        bail!("capability mismatch");
    }
    if capability.provider_agent != request.target_agent {
        bail!("target_agent does not match capability provider");
    }
    if !capability
        .allowed_callers
        .iter()
        .any(|caller| caller == &request.requester_agent)
    {
        bail!("requester_agent is not allowed for capability");
    }
    if !capability
        .allowed_work_item_types
        .iter()
        .any(|item| item == &request.work_item_type)
    {
        bail!("work_item_type is not allowed for capability");
    }
    if !ALLOWED_RISK_LEVELS.contains(&capability.risk_level.as_str()) {
        bail!("capability risk_level is not allowed");
    }
    if capability.capability_key == "huabaosi.generate_image_asset" {
        validate_image_generation_request(request)?;
    }
    if contains_sensitive_value(&request.payload)
        || contains_sensitive_value(&request.source_refs)
        || contains_sensitive_value(&request.metadata)
        || contains_sensitive_text(&request.brief_summary)
    {
        bail!("payload contains disallowed sensitive or raw internal content");
    }
    validate_high_risk_send(request, capability, policy)?;
    Ok(())
}

fn validate_image_generation_request(request: &WorkItemCreateRequest) -> Result<()> {
    let artifact_id = image_generation_approved_artifact_id(request)?;
    let payload_artifact_id = request
        .payload
        .get("approved_brief_artifact_id")
        .and_then(Value::as_str)
        .ok_or_else(|| anyhow::anyhow!("approved_brief_artifact_id is required"))?;
    if Uuid::parse_str(payload_artifact_id).context("approved_brief_artifact_id must be a uuid")?
        != artifact_id
    {
        bail!("approved_artifact_id must match payload approved_brief_artifact_id");
    }
    for field in ["approved_brief_content_hash", "prompt_hash"] {
        let value = request
            .payload
            .get(field)
            .and_then(Value::as_str)
            .unwrap_or_default();
        if !value.starts_with("sha256:") || value.len() <= "sha256:".len() {
            bail!("{field} must be a sha256 hash");
        }
    }
    if request
        .payload
        .get("image_specification")
        .and_then(Value::as_str)
        != Some("community_poster_1024x1024")
    {
        bail!("image_specification is not allowed");
    }
    Ok(())
}

fn normalize_plan_input(input: &mut RequestPlanInput) {
    input.actor_agent = normalize_key(&input.actor_agent);
    input.request_text = input.request_text.trim().to_string();
    input.source_type = normalize_key(&input.source_type);
    input.human_owner = input.human_owner.trim().to_string();
    input.priority = normalize_key(&input.priority);
    if input.source_refs.is_null() {
        input.source_refs = json!({});
    }
    if input.metadata.is_null() {
        input.metadata = json!({});
    }
}

fn normalize_workflow_start_request(request: &mut WorkflowStartRequest) {
    request.actor_agent = normalize_key(&request.actor_agent);
    request.workflow_type = normalize_key(&request.workflow_type);
    request.request_text = request.request_text.trim().to_string();
    request.source_type = normalize_key(&request.source_type);
    request.human_owner = request.human_owner.trim().to_string();
    request.priority = normalize_key(&request.priority);
    request.idempotency_key = request.idempotency_key.trim().to_string();
    if request.source_refs.is_null() {
        request.source_refs = json!({});
    }
    if request.metadata.is_null() {
        request.metadata = json!({});
    }
    if request.idempotency_key.is_empty() {
        let mut hasher = Sha256::new();
        hasher.update(request.actor_agent.as_bytes());
        hasher.update(b"|");
        hasher.update(request.workflow_type.as_bytes());
        hasher.update(b"|");
        hasher.update(request.source_type.as_bytes());
        hasher.update(b"|");
        hasher.update(request.source_refs.to_string().as_bytes());
        hasher.update(b"|");
        hasher.update(request.request_text.as_bytes());
        request.idempotency_key = format!("workflow:{:x}", hasher.finalize());
    }
}

fn validate_plan_input(input: &RequestPlanInput) -> Result<()> {
    require_non_empty("actor_agent", &input.actor_agent)?;
    require_non_empty("request_text", &input.request_text)?;
    if input.request_text.chars().count() > 500 {
        bail!("request_text must be 500 characters or fewer");
    }
    if !ALLOWED_PRIORITIES.contains(&input.priority.as_str()) {
        bail!("priority is not allowed");
    }
    validate_source_policy(&input.source_type, &input.source_refs, None)?;
    if contains_sensitive_text(&input.request_text)
        || contains_sensitive_value(&input.source_refs)
        || contains_sensitive_value(&input.metadata)
    {
        bail!("request plan payload contains disallowed sensitive or raw internal content");
    }
    Ok(())
}

fn validate_workflow_start_request(request: &WorkflowStartRequest) -> Result<()> {
    require_non_empty("actor_agent", &request.actor_agent)?;
    require_non_empty("workflow_type", &request.workflow_type)?;
    require_non_empty("request_text", &request.request_text)?;
    if request.workflow_type != "activity_promotion" {
        bail!("workflow_type is not supported");
    }
    if request.request_text.chars().count() > 500 {
        bail!("request_text must be 500 characters or fewer");
    }
    if !ALLOWED_PRIORITIES.contains(&request.priority.as_str()) {
        bail!("priority is not allowed");
    }
    validate_source_policy(&request.source_type, &request.source_refs, None)?;
    if request.actor_agent != "xiaoman" && request.actor_agent != "default" {
        bail!("actor_agent is not allowed to start activity_promotion workflow");
    }
    if contains_sensitive_text(&request.request_text)
        || contains_sensitive_value(&request.source_refs)
        || contains_sensitive_value(&request.metadata)
    {
        bail!("workflow start payload contains disallowed sensitive or raw internal content");
    }
    Ok(())
}

fn validate_source_policy(
    source_type: &str,
    source_refs: &Value,
    source_event_signal_id: Option<Uuid>,
) -> Result<()> {
    require_non_empty("source_type", source_type)?;
    if !ALLOWED_SOURCE_TYPES.contains(&source_type) {
        bail!("source_type is not allowed for operations work items");
    }
    if !source_refs.is_object() {
        bail!("source_refs must be an object");
    }
    match source_type {
        "event_signal" => {
            let has_event_signal_ref = source_event_signal_id.is_some()
                || non_empty_object_text(source_refs, "event_signal_id").is_some()
                || non_empty_object_text(source_refs, "source_event_signal_id").is_some();
            if !has_event_signal_ref {
                bail!("event_signal source requires event_signal_id in source_refs or source_event_signal_id");
            }
        }
        "manual_request" | "apply_smoke" | "xiaoman_activity" | "operations_workflow" => {
            if non_empty_object_text(source_refs, "source_record_ref").is_none() {
                bail!("source_refs.source_record_ref is required for this source_type");
            }
        }
        _ => bail!("source_type is not allowed for operations work items"),
    }
    Ok(())
}

fn request_from_plan_input(input: &RequestPlanInput) -> Result<Option<WorkItemCreateRequest>> {
    let text = input.request_text.to_lowercase();
    if contains_any(&text, &["发群", "群发", "发送到群", "发到群", "send group"]) {
        let Some(artifact_id) = find_uuid_in_text(&input.request_text) else {
            return Ok(None);
        };
        return Ok(Some(base_planned_request(
            input,
            "erhua",
            "erhua.send_group_message",
            "group_message_request",
            "group_message_send_request",
            json!({
                "planner_intent": "send_group_message",
                "approved_artifact_id": artifact_id,
                "target_channel": "qiwe",
                "target_group_alias": "community_activity_group",
                "message_text": input.request_text,
            }),
        )));
    }
    if contains_any(&text, &["海报", "画报", "视觉", "poster", "素材", "宣发图"]) {
        return Ok(Some(base_planned_request(
            input,
            "huabaosi",
            "huabaosi.create_visual_asset",
            "visual_asset_request",
            "activity_visual_asset_request",
            json!({
                "planner_intent": "create_visual_asset",
                "requested_output": "poster_or_visual_draft",
            }),
        )));
    }
    if contains_any(
        &text,
        &["资料", "证据", "背景", "查找", "检索", "复盘", "evidence"],
    ) {
        return Ok(Some(base_planned_request(
            input,
            "wenyuange",
            "wenyuange.retrieve_evidence",
            "evidence_request",
            "operations_evidence_request",
            json!({
                "planner_intent": "retrieve_evidence",
                "question": input.request_text,
            }),
        )));
    }
    Ok(None)
}

fn planned_request_from_input(input: &RequestPlanInput) -> Result<Option<WorkItemCreateRequest>> {
    let Some(mut request) = request_from_plan_input(input)? else {
        return Ok(None);
    };
    request.human_owner = input.human_owner.clone();
    request.priority = input.priority.clone();
    request.source_type = input.source_type.clone();
    request.source_refs = input.source_refs.clone();
    request.metadata = json!({
        "planner": "operations-request-plan",
        "original_request": input.request_text,
        "planner_metadata": input.metadata,
    });
    Ok(Some(request))
}

fn base_planned_request(
    input: &RequestPlanInput,
    target_agent: &str,
    capability_key: &str,
    work_item_type: &str,
    purpose: &str,
    payload: Value,
) -> WorkItemCreateRequest {
    WorkItemCreateRequest {
        requester_agent: input.actor_agent.clone(),
        target_agent: target_agent.to_string(),
        capability_key: capability_key.to_string(),
        work_item_type: work_item_type.to_string(),
        brief_summary: input.request_text.clone(),
        purpose: purpose.to_string(),
        human_owner: input.human_owner.clone(),
        priority: input.priority.clone(),
        source_type: input.source_type.clone(),
        source_refs: input.source_refs.clone(),
        source_event_signal_id: None,
        payload,
        payload_redaction_policy: "summary_only".to_string(),
        idempotency_key: String::new(),
        dedupe_key: String::new(),
        metadata: json!({}),
        parent_work_item_id: None,
        approved_artifact_id: None,
    }
}

fn workflow_work_item_requests(
    request: &WorkflowStartRequest,
    parent_work_item_id: Option<Uuid>,
) -> (WorkItemCreateRequest, Vec<WorkItemCreateRequest>) {
    let parent = WorkItemCreateRequest {
        requester_agent: "default".to_string(),
        target_agent: "xiaoman".to_string(),
        capability_key: "xiaoman.create_activity_request".to_string(),
        work_item_type: "activity_promotion_request".to_string(),
        brief_summary: request.request_text.clone(),
        purpose: "activity_promotion_workflow".to_string(),
        human_owner: request.human_owner.clone(),
        priority: request.priority.clone(),
        source_type: request.source_type.clone(),
        source_refs: request.source_refs.clone(),
        source_event_signal_id: None,
        payload: json!({
            "workflow_type": request.workflow_type,
            "requested_by": request.actor_agent,
            "request_text": request.request_text,
        }),
        payload_redaction_policy: "summary_only".to_string(),
        idempotency_key: format!("{}:parent", request.idempotency_key),
        dedupe_key: String::new(),
        metadata: json!({
            "workflow_type": request.workflow_type,
            "workflow_starter": "operations-workflow-start",
            "workflow_metadata": request.metadata,
        }),
        parent_work_item_id: None,
        approved_artifact_id: None,
    };
    let evidence_child = WorkItemCreateRequest {
        requester_agent: "xiaoman".to_string(),
        target_agent: "wenyuange".to_string(),
        capability_key: "wenyuange.retrieve_evidence".to_string(),
        work_item_type: "evidence_request".to_string(),
        brief_summary: format!("{} - 活动宣发背景资料", request.request_text),
        purpose: "activity_evidence_request".to_string(),
        human_owner: request.human_owner.clone(),
        priority: request.priority.clone(),
        source_type: request.source_type.clone(),
        source_refs: request.source_refs.clone(),
        source_event_signal_id: None,
        payload: json!({
            "workflow_type": request.workflow_type,
            "planner_intent": "retrieve_evidence",
            "question": format!("请整理活动宣发前需要引用的背景资料和证据：{}", request.request_text),
            "request_text": request.request_text,
        }),
        payload_redaction_policy: "summary_only".to_string(),
        idempotency_key: format!("{}:evidence-child", request.idempotency_key),
        dedupe_key: String::new(),
        metadata: json!({
            "workflow_type": request.workflow_type,
            "workflow_starter": "operations-workflow-start",
            "workflow_step": "evidence",
            "workflow_metadata": request.metadata,
        }),
        parent_work_item_id,
        approved_artifact_id: None,
    };
    let visual_child = WorkItemCreateRequest {
        requester_agent: "xiaoman".to_string(),
        target_agent: "huabaosi".to_string(),
        capability_key: "huabaosi.create_visual_asset".to_string(),
        work_item_type: "visual_asset_request".to_string(),
        brief_summary: request.request_text.clone(),
        purpose: "activity_visual_asset_request".to_string(),
        human_owner: request.human_owner.clone(),
        priority: request.priority.clone(),
        source_type: request.source_type.clone(),
        source_refs: request.source_refs.clone(),
        source_event_signal_id: None,
        payload: json!({
            "workflow_type": request.workflow_type,
            "planner_intent": "create_visual_asset",
            "requested_output": "poster_or_visual_draft",
            "request_text": request.request_text,
        }),
        payload_redaction_policy: "summary_only".to_string(),
        idempotency_key: format!("{}:visual-child", request.idempotency_key),
        dedupe_key: String::new(),
        metadata: json!({
            "workflow_type": request.workflow_type,
            "workflow_starter": "operations-workflow-start",
            "workflow_step": "visual_asset",
            "workflow_metadata": request.metadata,
        }),
        parent_work_item_id,
        approved_artifact_id: None,
    };
    (parent, vec![evidence_child, visual_child])
}

fn xiaoman_activity_promotion_child_requests(
    candidate: &XiaomanActivityPromotionCandidate,
) -> Vec<WorkItemCreateRequest> {
    let mut requests = Vec::new();
    if candidate.missing_evidence_child {
        requests.push(xiaoman_activity_promotion_child_request(
            candidate,
            "wenyuange",
            "wenyuange.retrieve_evidence",
            "evidence_request",
            "activity_evidence_request",
            format!("{} - 活动宣发背景资料", candidate.brief_summary),
            "evidence-child",
            json!({
                "workflow_type": "activity_promotion",
                "planner_intent": "retrieve_evidence",
                "question": format!("请整理活动宣发前需要引用的背景资料和证据：{}", candidate.brief_summary),
                "request_text": candidate.brief_summary,
            }),
        ));
    }
    if candidate.missing_visual_child {
        requests.push(xiaoman_activity_promotion_child_request(
            candidate,
            "huabaosi",
            "huabaosi.create_visual_asset",
            "visual_asset_request",
            "activity_visual_asset_request",
            candidate.brief_summary.clone(),
            "visual-child",
            json!({
                "workflow_type": "activity_promotion",
                "planner_intent": "create_visual_asset",
                "requested_output": "poster_or_visual_draft",
                "request_text": candidate.brief_summary,
            }),
        ));
    }
    requests
}

#[expect(
    clippy::too_many_arguments,
    reason = "child request creation keeps capability and idempotency fields explicit"
)]
fn xiaoman_activity_promotion_child_request(
    candidate: &XiaomanActivityPromotionCandidate,
    target_agent: &str,
    capability_key: &str,
    work_item_type: &str,
    purpose: &str,
    brief_summary: String,
    idempotency_suffix: &str,
    payload: Value,
) -> WorkItemCreateRequest {
    WorkItemCreateRequest {
        requester_agent: "xiaoman".to_string(),
        target_agent: target_agent.to_string(),
        capability_key: capability_key.to_string(),
        work_item_type: work_item_type.to_string(),
        brief_summary,
        purpose: purpose.to_string(),
        human_owner: candidate.human_owner.clone(),
        priority: candidate.priority.clone(),
        source_type: candidate.source_type.clone(),
        source_refs: candidate.source_refs.clone(),
        source_event_signal_id: candidate.source_event_signal_id,
        payload,
        payload_redaction_policy: "summary_only".to_string(),
        idempotency_key: format!(
            "xiaoman_activity_promotion:{}:{}",
            candidate.id, idempotency_suffix
        ),
        dedupe_key: String::new(),
        metadata: json!({
            "workflow_type": "activity_promotion",
            "workflow_starter": "run-xiaoman-activity-promotion-starter-worker",
            "workflow_step": idempotency_suffix,
            "parent_activity_request_work_item_id": candidate.id,
        }),
        parent_work_item_id: Some(candidate.id),
        approved_artifact_id: None,
    }
}

fn xiaoman_activity_send_request(
    candidate: &XiaomanActivitySendRequestCandidate,
    target_group_alias: &str,
    message_text: &str,
) -> Result<WorkItemCreateRequest> {
    let target_group_alias = normalize_key(target_group_alias);
    let message_text = message_text.trim();
    require_non_empty("target_group_alias", &target_group_alias)?;
    require_non_empty("message_text", message_text)?;
    if message_text.chars().count() > 500 {
        bail!("message_text must be 500 characters or fewer");
    }
    if contains_sensitive_text(message_text) {
        bail!("message_text contains disallowed sensitive or raw internal content");
    }

    Ok(WorkItemCreateRequest {
        requester_agent: "xiaoman".to_string(),
        target_agent: "erhua".to_string(),
        capability_key: "erhua.send_group_message".to_string(),
        work_item_type: "group_message_request".to_string(),
        brief_summary: format!("发送已审核活动图片：{}", candidate.brief_summary),
        purpose: "activity_group_message_request".to_string(),
        human_owner: candidate.human_owner.clone(),
        priority: candidate.priority.clone(),
        source_type: candidate.source_type.clone(),
        source_refs: candidate.source_refs.clone(),
        source_event_signal_id: candidate.source_event_signal_id,
        payload: json!({
            "workflow_type": "activity_promotion",
            "planner_intent": "send_group_message_after_final_confirmation",
            "approved_artifact_id": candidate.approved_artifact_id,
            "approved_artifact_type": "generated_image",
            "visual_work_item_id": candidate.visual_work_item_id,
            "image_generation_work_item_id": candidate.image_generation_work_item_id,
            "target_channel": "qiwe",
            "target_group_alias": target_group_alias,
            "message_text": message_text,
            "send_executed": false,
        }),
        payload_redaction_policy: "summary_only".to_string(),
        idempotency_key: format!(
            "xiaoman_activity_promotion:{}:group-message-child",
            candidate.parent_id
        ),
        dedupe_key: String::new(),
        metadata: json!({
            "workflow_type": "activity_promotion",
            "workflow_starter": "run-xiaoman-activity-send-request-starter-worker",
            "workflow_step": "group_message",
            "parent_activity_request_work_item_id": candidate.parent_id,
            "visual_work_item_id": candidate.visual_work_item_id,
            "image_generation_work_item_id": candidate.image_generation_work_item_id,
            "approved_artifact_id": candidate.approved_artifact_id,
            "approved_artifact_type": "generated_image",
            "requires_human_final_confirmation": true,
            "send_executed": false,
        }),
        parent_work_item_id: Some(candidate.parent_id),
        approved_artifact_id: Some(candidate.approved_artifact_id),
    })
}

fn xiaoman_activity_image_generation_request(
    candidate: &XiaomanActivityImageGenerationCandidate,
) -> WorkItemCreateRequest {
    const SPECIFICATION: &str = "community_poster_1024x1024";
    let prompt_hash = format!(
        "sha256:{}",
        content_hash_text(&format!(
            "{}|{}|{}",
            candidate.approved_artifact_id, SPECIFICATION, candidate.approved_brief_hash
        ))
    );
    WorkItemCreateRequest {
        requester_agent: "xiaoman".to_string(),
        target_agent: "huabaosi".to_string(),
        capability_key: "huabaosi.generate_image_asset".to_string(),
        work_item_type: "image_generation_request".to_string(),
        brief_summary: format!("生成已审核活动海报图片：{}", candidate.brief_summary),
        purpose: "activity_image_generation_request".to_string(),
        human_owner: candidate.human_owner.clone(),
        priority: candidate.priority.clone(),
        source_type: candidate.source_type.clone(),
        source_refs: candidate.source_refs.clone(),
        source_event_signal_id: candidate.source_event_signal_id,
        payload: json!({
            "workflow_type": "activity_promotion",
            "planner_intent": "generate_image_after_approved_poster_brief",
            "approved_brief_artifact_id": candidate.approved_artifact_id,
            "approved_brief_content_hash": candidate.approved_brief_hash,
            "evidence_content_hash": candidate.evidence_content_hash,
            "image_specification": SPECIFICATION,
            "prompt_hash": prompt_hash,
            "external_publish_executed": false,
        }),
        payload_redaction_policy: "summary_only".to_string(),
        idempotency_key: format!(
            "huabaosi_image:{}:{}:{}",
            candidate.approved_artifact_id, SPECIFICATION, prompt_hash
        ),
        dedupe_key: String::new(),
        metadata: json!({
            "workflow_type": "activity_promotion",
            "workflow_starter": "run-xiaoman-activity-image-generation-starter-worker",
            "workflow_step": "image_generation",
            "visual_work_item_id": candidate.visual_work_item_id,
            "approved_brief_artifact_id": candidate.approved_artifact_id,
            "approved_brief_content_hash": candidate.approved_brief_hash,
            "evidence_content_hash": candidate.evidence_content_hash,
            "image_specification": SPECIFICATION,
            "prompt_hash": prompt_hash,
            "external_publish_executed": false,
        }),
        parent_work_item_id: Some(candidate.visual_work_item_id),
        approved_artifact_id: Some(candidate.approved_artifact_id),
    }
}

fn work_item_request_preview(request: &WorkItemCreateRequest) -> WorkItemCreateRequestPreview {
    WorkItemCreateRequestPreview {
        parent_work_item_id: request.parent_work_item_id,
        requester_agent: request.requester_agent.clone(),
        target_agent: request.target_agent.clone(),
        capability_key: request.capability_key.clone(),
        work_item_type: request.work_item_type.clone(),
        brief_summary: request.brief_summary.clone(),
        purpose: request.purpose.clone(),
        priority: request.priority.clone(),
        source_type: request.source_type.clone(),
        source_refs: request.source_refs.clone(),
        payload: request.payload.clone(),
        payload_redaction_policy: request.payload_redaction_policy.clone(),
    }
}

fn request_plan_needs_clarification(
    input: &RequestPlanInput,
    clarification_questions: Vec<String>,
) -> RequestPlanReport {
    RequestPlanReport {
        success: true,
        action_status: "needs_clarification".to_string(),
        planner: "operations-request-plan",
        requester_agent: input.actor_agent.clone(),
        original_request: input.request_text.clone(),
        selected_capability: None,
        work_item_request: None,
        work_item_preview: None,
        clarification_questions,
        limitations: request_plan_limitations(),
        guardrails: request_plan_guardrails(),
    }
}

fn request_plan_limitations() -> Vec<String> {
    vec![
        "planner is deterministic and rule-based; it does not call an LLM".to_string(),
        "planner output is a dry-run plan, not execution".to_string(),
        "ambiguous requests must be clarified before work item creation".to_string(),
    ]
}

fn request_plan_guardrails() -> Vec<String> {
    vec![
        "plans must resolve to a registered capability".to_string(),
        "raw prompt handoff is not allowed".to_string(),
        "payloads must use redacted summaries and source refs".to_string(),
        "high-risk send/publish requests require approved artifacts and final confirmation"
            .to_string(),
    ]
}

fn request_submit_report_from_plan(
    plan: RequestPlanReport,
    apply_requested: bool,
    work_item_result: Option<WorkItemCreateReport>,
) -> RequestSubmitReport {
    RequestSubmitReport {
        success: plan.success,
        dry_run: !apply_requested,
        apply_requested,
        action_status: if plan.action_status == "needs_clarification" {
            "needs_clarification".to_string()
        } else if apply_requested {
            work_item_result
                .as_ref()
                .map(|report| report.action_status.clone())
                .unwrap_or_else(|| "created".to_string())
        } else {
            "dry_run_ok".to_string()
        },
        planner: "operations-request-submit",
        requester_agent: plan.requester_agent,
        original_request: plan.original_request,
        selected_capability: plan.selected_capability,
        work_item_request: plan.work_item_request,
        work_item_result: work_item_result.or(plan.work_item_preview),
        clarification_questions: plan.clarification_questions,
        limitations: request_submit_limitations(),
        guardrails: request_submit_guardrails(),
    }
}

fn request_submit_limitations() -> Vec<String> {
    vec![
        "request submit uses the deterministic operations planner; it does not call an LLM"
            .to_string(),
        "ambiguous requests are not created and must be clarified first".to_string(),
        "apply mode creates only AgentOS work items; it does not execute workers or external sends"
            .to_string(),
    ]
}

fn request_submit_guardrails() -> Vec<String> {
    vec![
        "created requests still pass capability and work item policy validation".to_string(),
        "Postgres remains the operations source of truth".to_string(),
        "high-risk send/publish requests require approved artifacts and final confirmation"
            .to_string(),
    ]
}

fn workflow_start_report(
    request: &WorkflowStartRequest,
    apply_requested: bool,
    action_status: &str,
    parent_work_item: WorkItemCreateReport,
    child_work_items: Vec<WorkItemCreateReport>,
) -> WorkflowStartReport {
    WorkflowStartReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        action_status: action_status.to_string(),
        workflow_type: request.workflow_type.clone(),
        parent_work_item,
        child_work_items,
        limitations: vec![
            "workflow starter creates control-plane work items only; it does not run workers"
                .to_string(),
            "v1 activity_promotion starts with evidence_request and visual_asset_request children"
                .to_string(),
            "later group-message steps must be requested after an artifact is approved".to_string(),
            "workflow starter does not create high-risk external send requests automatically"
                .to_string(),
        ],
        guardrails: vec![
            "parent and child work items still pass capability policy validation".to_string(),
            "Postgres remains the workflow source of truth".to_string(),
            "Hermes Kanban is not used as a fallback".to_string(),
        ],
    }
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn find_uuid_in_text(text: &str) -> Option<String> {
    text.split(|ch: char| !(ch.is_ascii_hexdigit() || ch == '-'))
        .find(|part| Uuid::parse_str(part).is_ok())
        .map(ToString::to_string)
}

async fn validate_apply_policy(
    pool: &PgPool,
    request: &WorkItemCreateRequest,
    capability: &Capability,
    policy: &OperationsPolicy,
) -> Result<()> {
    if capability.capability_key == "huabaosi.generate_image_asset" {
        let artifact_id = image_generation_approved_artifact_id(request)?;
        let row = sqlx::query(
            r#"
            SELECT work_item_id, artifact_type, review_status, content_hash
            FROM qintopia_agent_os.artifacts
            WHERE id = $1
            "#,
        )
        .bind(artifact_id)
        .fetch_optional(pool)
        .await
        .context("load approved poster brief for image generation request")?
        .ok_or_else(|| anyhow::anyhow!("approved_brief_artifact_id does not exist"))?;
        let artifact_work_item_id: Uuid = row.get("work_item_id");
        let artifact_type: String = row.get("artifact_type");
        let review_status: String = row.get("review_status");
        let content_hash: Option<String> = row.get("content_hash");
        if artifact_type != "poster_brief" || review_status != "approved" {
            bail!("approved_brief_artifact_id must reference an approved poster_brief");
        }
        if request.parent_work_item_id != Some(artifact_work_item_id) {
            bail!("image_generation_request parent must be the approved poster_brief work item");
        }
        if content_hash.as_deref()
            != request
                .payload
                .get("approved_brief_content_hash")
                .and_then(Value::as_str)
        {
            bail!("approved_brief_content_hash must match the approved poster_brief");
        }
        return Ok(());
    }
    if capability.capability_key != "erhua.send_group_message" {
        return Ok(());
    }
    if policy.require_approved_artifact_lookup {
        let artifact_id = approved_artifact_id(request)?;
        let row = sqlx::query(
            r#"
            SELECT artifact_type, review_status
            FROM qintopia_agent_os.artifacts
            WHERE id = $1
            "#,
        )
        .bind(artifact_id)
        .fetch_optional(pool)
        .await
        .context("load approved artifact for group message request")?
        .ok_or_else(|| anyhow::anyhow!("approved_artifact_id does not exist"))?;
        let artifact_type: String = row.get("artifact_type");
        let review_status: String = row.get("review_status");
        if review_status != "approved" {
            bail!("approved_artifact_id must reference an approved artifact");
        }
        if request.payload.get("workflow_type").and_then(Value::as_str)
            == Some("activity_promotion")
            && artifact_type != "generated_image"
        {
            bail!("activity promotion group message requests require an approved generated_image");
        }
    }
    Ok(())
}

fn normalize_review_request(request: &mut ArtifactReviewDecisionRequest) {
    request.reviewer_id = request.reviewer_id.trim().to_string();
    request.decision = normalize_key(&request.decision);
    request.reason = request.reason.trim().to_string();
    request.source = request.source.trim().to_string();
    if request.source.is_empty() {
        request.source = "manual_cli".to_string();
    }
    if request.metadata.is_null() {
        request.metadata = json!({});
    }
}

fn validate_review_request(request: &ArtifactReviewDecisionRequest) -> Result<()> {
    require_non_empty("reviewer_id", &request.reviewer_id)?;
    validate_human_actor_id("reviewer_id", &request.reviewer_id)?;
    if !["approved", "rejected", "changes_requested"].contains(&request.decision.as_str()) {
        bail!("review decision is not allowed");
    }
    if ["rejected", "changes_requested"].contains(&request.decision.as_str())
        && request.reason.trim().is_empty()
    {
        bail!("reason is required for rejected or changes_requested decisions");
    }
    if request.reason.chars().count() > 1000 {
        bail!("reason must be 1000 characters or fewer");
    }
    if contains_sensitive_value(&request.metadata) || contains_sensitive_text(&request.reason) {
        bail!("review payload contains disallowed sensitive or raw internal content");
    }
    Ok(())
}

fn artifact_approval_context_from_row(
    row: &sqlx::postgres::PgRow,
) -> Result<ArtifactApprovalContext> {
    Ok(ArtifactApprovalContext {
        work_item_id: row.try_get("work_item_id")?,
        artifact_type: row.try_get("artifact_type")?,
        review_status: row.try_get("review_status")?,
        created_by_agent: row.try_get("created_by_agent")?,
        artifact_uri: row.try_get("artifact_uri")?,
        content_hash: row.try_get("content_hash")?,
        source_ids: row.try_get("source_ids")?,
        risk_labels: row.try_get("risk_labels")?,
        information_class: row.try_get("information_class")?,
        metadata: row.try_get("metadata")?,
        work_item_type: row.try_get("work_item_type")?,
        capability_key: row.try_get("capability_key")?,
        work_item_status: row.try_get("work_item_status")?,
        work_item_payload: row.try_get("work_item_payload")?,
        creation_event_matches: row.try_get("creation_event_matches")?,
    })
}

fn validate_generated_image_approval(
    context: &ArtifactApprovalContext,
    decision: &str,
) -> Result<()> {
    if decision != "approved" || context.artifact_type != GENERATED_IMAGE_ARTIFACT_TYPE {
        return Ok(());
    }
    if context.review_status != "pending" {
        bail!("generated_image approval requires pending review status");
    }
    if context.work_item_type != GENERATED_IMAGE_WORK_ITEM_TYPE
        || context.capability_key != GENERATED_IMAGE_CAPABILITY_KEY
        || context.work_item_status != "awaiting_review"
    {
        bail!("generated_image approval requires an awaiting-review image request");
    }
    if context.created_by_agent != "huabaosi"
        || context.information_class != "internal_ops"
        || !context
            .risk_labels
            .iter()
            .any(|label| label == "generated_media")
        || !context
            .risk_labels
            .iter()
            .any(|label| label == "external_use_review_required")
    {
        bail!("generated_image approval requires controlled artifact provenance");
    }

    validate_generated_image_uri(context.artifact_uri.as_deref().unwrap_or_default())?;
    validate_canonical_sha256(
        context.content_hash.as_deref().unwrap_or_default(),
        "generated_image content_hash",
    )?;
    validate_canonical_md5(
        json_string(&context.metadata, "file_md5").unwrap_or_default(),
        "generated_image file_md5",
    )?;

    for (key, expected) in [
        ("generated_by", GENERATED_IMAGE_WORKER_ID),
        ("provider", "openai-compatible"),
        ("model", "gpt-image-2"),
        ("mime_type", "image/jpeg"),
        ("provider_source_mime_type", "image/png"),
        ("media_transform", "png_to_jpeg_white_background_q92_v1"),
        ("alpha_background", "#ffffff"),
    ] {
        if json_string(&context.metadata, key) != Some(expected) {
            bail!("generated_image approval requires canonical worker metadata");
        }
    }
    if json_i64(&context.metadata, "width") != Some(1024)
        || json_i64(&context.metadata, "height") != Some(1024)
    {
        bail!("generated_image approval requires 1024x1024 JPEG metadata");
    }
    let provider_source_content_hash =
        json_string(&context.metadata, "provider_source_content_hash").unwrap_or_default();
    validate_canonical_sha256(
        provider_source_content_hash,
        "generated_image provider source content hash",
    )?;
    if json_i64(&context.metadata, "jpeg_quality") != Some(92) {
        bail!("generated_image approval requires the reviewed JPEG quality");
    }
    let byte_size = json_i64(&context.metadata, "byte_size").unwrap_or_default();
    if !(1..=MAX_APPROVABLE_GENERATED_IMAGE_BYTES).contains(&byte_size) {
        bail!("generated_image approval requires bounded positive byte_size metadata");
    }

    let brief_id = matching_uuid_field(
        &context.metadata,
        &context.work_item_payload,
        "approved_brief_artifact_id",
    )?;
    let brief_hash = matching_string_field(
        &context.metadata,
        &context.work_item_payload,
        "approved_brief_content_hash",
    )?;
    validate_canonical_sha256(brief_hash, "approved brief content hash")?;
    let prompt_hash =
        matching_string_field(&context.metadata, &context.work_item_payload, "prompt_hash")?;
    validate_canonical_sha256(prompt_hash, "image prompt hash")?;
    if json_string(&context.work_item_payload, "image_specification")
        != Some("community_poster_1024x1024")
    {
        bail!("generated_image approval requires the reviewed image specification");
    }
    if !source_ids_match_approved_brief(&context.source_ids, brief_id, brief_hash) {
        bail!("generated_image approval requires matching approved brief source refs");
    }
    if !context.creation_event_matches {
        bail!("generated_image approval requires a matching creation audit");
    }
    Ok(())
}

fn validate_generated_image_uri(value: &str) -> Result<()> {
    url_policy::reject_path_separator_ambiguity(value, "generated_image artifact_uri")?;
    let uri = url::Url::parse(value).context("generated_image artifact_uri must be a valid URL")?;
    if uri.scheme() != "https"
        || uri.host_str().is_none()
        || !uri.username().is_empty()
        || uri.password().is_some()
        || uri.query().is_some()
        || uri.fragment().is_some()
        || matches!(uri.path(), "" | "/")
    {
        bail!("generated_image artifact_uri must be a stable HTTPS media URL");
    }
    let path = uri.path().to_ascii_lowercase();
    if !path.ends_with(".jpg") && !path.ends_with(".jpeg") {
        bail!("generated_image artifact_uri must reference a JPEG object");
    }
    Ok(())
}

fn validate_canonical_sha256(value: &str, label: &str) -> Result<()> {
    let Some(hex) = value.strip_prefix("sha256:") else {
        bail!("{label} must be a canonical sha256");
    };
    if hex.len() != 64
        || !hex
            .chars()
            .all(|character| matches!(character, '0'..='9' | 'a'..='f'))
    {
        bail!("{label} must be a canonical sha256");
    }
    Ok(())
}

fn validate_canonical_md5(value: &str, label: &str) -> Result<()> {
    if value.len() != 32
        || !value
            .chars()
            .all(|character| matches!(character, '0'..='9' | 'a'..='f'))
    {
        bail!("{label} must be a canonical md5");
    }
    Ok(())
}

fn matching_uuid_field(metadata: &Value, payload: &Value, key: &str) -> Result<Uuid> {
    let metadata_value = json_string(metadata, key)
        .ok_or_else(|| anyhow::anyhow!("generated_image metadata is missing {key}"))?;
    let payload_value = json_string(payload, key)
        .ok_or_else(|| anyhow::anyhow!("image request payload is missing {key}"))?;
    if metadata_value != payload_value {
        bail!("generated_image source metadata does not match its image request");
    }
    Uuid::parse_str(metadata_value).with_context(|| format!("{key} must be a uuid"))
}

fn matching_string_field<'a>(
    metadata: &'a Value,
    payload: &'a Value,
    key: &str,
) -> Result<&'a str> {
    let metadata_value = json_string(metadata, key)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("generated_image metadata is missing {key}"))?;
    let payload_value = json_string(payload, key)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| anyhow::anyhow!("image request payload is missing {key}"))?;
    if metadata_value != payload_value {
        bail!("generated_image source metadata does not match its image request");
    }
    Ok(metadata_value)
}

fn json_string<'a>(value: &'a Value, key: &str) -> Option<&'a str> {
    value.get(key).and_then(Value::as_str)
}

fn json_i64(value: &Value, key: &str) -> Option<i64> {
    value.get(key).and_then(Value::as_i64)
}

fn source_ids_match_approved_brief(source_ids: &Value, brief_id: Uuid, brief_hash: &str) -> bool {
    source_ids.as_array().is_some_and(|sources| {
        sources.iter().any(|source| {
            json_string(source, "approved_brief_artifact_id")
                .and_then(|value| Uuid::parse_str(value).ok())
                == Some(brief_id)
                && json_string(source, "approved_brief_content_hash") == Some(brief_hash)
        })
    })
}

fn validate_reviewer_authorized(reviewer_id: &str, policy: &OperationsPolicy) -> Result<()> {
    if !policy.reviewer_allowed(reviewer_id) {
        bail!("reviewer_id is not allowed for artifact review decisions");
    }
    Ok(())
}

fn normalize_group_message_confirm_request(request: &mut GroupMessageConfirmRequest) {
    request.confirmer_id = request.confirmer_id.trim().to_string();
    request.decision = normalize_key(&request.decision);
    request.reason = request.reason.trim().to_string();
    request.source = request.source.trim().to_string();
    if request.source.is_empty() {
        request.source = "manual_cli".to_string();
    }
    if request.metadata.is_null() {
        request.metadata = json!({});
    }
}

fn validate_group_message_confirm_request(request: &GroupMessageConfirmRequest) -> Result<()> {
    require_non_empty("confirmer_id", &request.confirmer_id)?;
    validate_human_actor_id("confirmer_id", &request.confirmer_id)?;
    if !["confirmed", "cancelled"].contains(&request.decision.as_str()) {
        bail!("group message confirmation decision is not allowed");
    }
    if request.decision == "cancelled" && request.reason.trim().is_empty() {
        bail!("reason is required when cancelling a group message request");
    }
    if request.reason.chars().count() > 1000 {
        bail!("reason must be 1000 characters or fewer");
    }
    if contains_sensitive_value(&request.metadata) || contains_sensitive_text(&request.reason) {
        bail!("confirmation payload contains disallowed sensitive or raw internal content");
    }
    Ok(())
}

fn validate_confirmer_authorized(confirmer_id: &str, policy: &OperationsPolicy) -> Result<()> {
    if !policy.confirmer_allowed(confirmer_id) {
        bail!("confirmer_id is not allowed for group message final confirmation");
    }
    Ok(())
}

fn validate_group_message_confirm_work_item(
    status: &str,
    work_item_type: &str,
    capability_key: &str,
    review_policy: &str,
) -> Result<()> {
    if work_item_type != "group_message_request" {
        bail!("work item is not a group_message_request");
    }
    if capability_key != "erhua.send_group_message" {
        bail!("work item does not use erhua.send_group_message");
    }
    if review_policy != "human_final_confirmation" {
        bail!("work item does not require human_final_confirmation");
    }
    if status != "awaiting_publish" {
        bail!("group message request must be awaiting_publish before final confirmation");
    }
    Ok(())
}

fn validate_workbench_status_change_event(event: &RecordedWorkbenchEvent) -> Result<()> {
    if event.requested_status != "cancelled" {
        bail!("status_change_requested can only request cancelled status");
    }
    require_non_empty("comment_text", &event.comment_text)?;
    Ok(())
}

fn validate_workbench_status_transition(
    previous_status: &str,
    requested_status: &str,
) -> Result<()> {
    if requested_status != "cancelled" {
        bail!("workbench status changes can only cancel work items");
    }
    if ["completed", "cancelled"].contains(&previous_status) {
        bail!("terminal work items cannot be changed from the human workbench");
    }
    Ok(())
}

fn validate_workbench_owner_change_event(event: &RecordedWorkbenchEvent) -> Result<()> {
    workbench_event_new_human_owner(event)?;
    Ok(())
}

fn workbench_event_new_human_owner(event: &RecordedWorkbenchEvent) -> Result<String> {
    let owner = event
        .metadata
        .get("new_human_owner")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .ok_or_else(|| anyhow::anyhow!("metadata.new_human_owner is required for owner_changed"))?;
    if owner.chars().count() > 100 {
        bail!("metadata.new_human_owner must be 100 characters or fewer");
    }
    if contains_sensitive_text(owner) {
        bail!("metadata.new_human_owner contains disallowed sensitive or raw internal content");
    }
    validate_human_actor_id("metadata.new_human_owner", owner)?;
    Ok(owner.to_string())
}

fn validate_workbench_attachment_event(event: &RecordedWorkbenchEvent) -> Result<()> {
    workbench_event_attachment(event)?;
    Ok(())
}

fn workbench_event_attachment(event: &RecordedWorkbenchEvent) -> Result<WorkbenchAttachment> {
    let title = metadata_text(&event.metadata, "attachment_title")
        .or_else(|| non_empty_text(&event.comment_text))
        .ok_or_else(|| {
            anyhow::anyhow!(
                "metadata.attachment_title or comment_text is required for attachment_added"
            )
        })?;
    if title.chars().count() > 200 {
        bail!("attachment title must be 200 characters or fewer");
    }
    let summary = metadata_text(&event.metadata, "attachment_summary")
        .unwrap_or_else(|| "Human workbench attachment awaiting review".to_string());
    if summary.chars().count() > 1000 {
        bail!("attachment summary must be 1000 characters or fewer");
    }
    let uri = metadata_text(&event.metadata, "attachment_uri").ok_or_else(|| {
        anyhow::anyhow!("metadata.attachment_uri is required for attachment_added")
    })?;
    if uri.chars().count() > 1000 {
        bail!("metadata.attachment_uri must be 1000 characters or fewer");
    }
    attachment_uri_host(&uri)?;
    let content_text = metadata_text(&event.metadata, "attachment_text").unwrap_or_default();
    if content_text.chars().count() > 4000 {
        bail!("attachment text must be 4000 characters or fewer");
    }
    if contains_sensitive_text(&title)
        || contains_sensitive_text(&summary)
        || contains_sensitive_text(&uri)
        || contains_sensitive_text(&content_text)
    {
        bail!("workbench attachment contains disallowed sensitive or raw internal content");
    }
    let has_attachment_text = !content_text.is_empty();
    Ok(WorkbenchAttachment {
        title,
        summary,
        content_text,
        uri,
        metadata: json!({
            "attachment_title": metadata_text(&event.metadata, "attachment_title"),
            "attachment_summary": metadata_text(&event.metadata, "attachment_summary"),
            "attachment_uri": metadata_text(&event.metadata, "attachment_uri"),
            "has_attachment_text": has_attachment_text,
        }),
    })
}

fn attachment_uri_host(uri: &str) -> Result<String> {
    let parsed = url::Url::parse(uri).context("metadata.attachment_uri must be a valid URL")?;
    if parsed.scheme() != "https" {
        bail!("metadata.attachment_uri must be an https URL");
    }
    let host = parsed
        .host_str()
        .map(normalize_host)
        .filter(|item| !item.is_empty())
        .ok_or_else(|| anyhow::anyhow!("metadata.attachment_uri must include a host"))?;
    Ok(host)
}

fn metadata_text(value: &Value, key: &str) -> Option<String> {
    value
        .get(key)
        .and_then(Value::as_str)
        .and_then(non_empty_text)
}

fn non_empty_text(value: &str) -> Option<String> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed.to_string())
    }
}

fn normalize_workbench_event_request(request: &mut WorkbenchEventRecordRequest) {
    request.provider = normalize_key(&request.provider);
    request.external_id = request.external_id.trim().to_string();
    request.external_event_id = request.external_event_id.trim().to_string();
    request.event_type = normalize_key(&request.event_type);
    request.actor_id = request.actor_id.trim().to_string();
    request.comment_text = request.comment_text.trim().to_string();
    request.requested_status = normalize_key(&request.requested_status);
    request.review_decision = normalize_key(&request.review_decision);
    request.confirmation_decision = normalize_key(&request.confirmation_decision);
    request.source = request.source.trim().to_string();
    if request.provider.is_empty() {
        request.provider = "feishu_task".to_string();
    }
    if request.source.is_empty() {
        request.source = "human_workbench_sync".to_string();
    }
    if request.metadata.is_null() {
        request.metadata = json!({});
    }
}

fn validate_workbench_event_request(request: &WorkbenchEventRecordRequest) -> Result<()> {
    require_non_empty("provider", &request.provider)?;
    require_non_empty("external_id", &request.external_id)?;
    require_non_empty("event_type", &request.event_type)?;
    require_non_empty("actor_id", &request.actor_id)?;
    if ![
        "comment_added",
        "status_change_requested",
        "review_decision_requested",
        "final_confirmation_requested",
        "owner_changed",
        "attachment_added",
    ]
    .contains(&request.event_type.as_str())
    {
        bail!("workbench event_type is not allowed");
    }
    if request.comment_text.chars().count() > 2000 {
        bail!("comment_text must be 2000 characters or fewer");
    }
    if request.requested_status.chars().count() > 100 {
        bail!("requested_status must be 100 characters or fewer");
    }
    if !request.requested_status.is_empty() && !_allowed_status(&request.requested_status) {
        bail!("requested_status is not an AgentOS work item status");
    }
    if !request.review_decision.is_empty()
        && !["approved", "rejected", "changes_requested"]
            .contains(&request.review_decision.as_str())
    {
        bail!("review_decision is not allowed");
    }
    if !request.confirmation_decision.is_empty()
        && !["confirmed", "cancelled"].contains(&request.confirmation_decision.as_str())
    {
        bail!("confirmation_decision is not allowed");
    }
    if request.event_type == "status_change_requested" && request.requested_status.is_empty() {
        bail!("requested_status is required for status_change_requested");
    }
    if request.event_type == "review_decision_requested" && request.review_decision.is_empty() {
        bail!("review_decision is required for review_decision_requested");
    }
    if request.event_type == "final_confirmation_requested"
        && request.confirmation_decision.is_empty()
    {
        bail!("confirmation_decision is required for final_confirmation_requested");
    }
    validate_workbench_event_request_payload(request)?;
    if contains_sensitive_value(&request.metadata)
        || contains_sensitive_text(&request.comment_text)
        || contains_sensitive_text(&request.external_id)
        || contains_sensitive_text(&request.external_event_id)
    {
        bail!("workbench event contains disallowed sensitive or raw internal content");
    }
    Ok(())
}

fn validate_workbench_event_request_payload(request: &WorkbenchEventRecordRequest) -> Result<()> {
    match request.event_type.as_str() {
        "status_change_requested" => {
            if request.requested_status != "cancelled" {
                bail!("status_change_requested can only request cancelled status");
            }
            require_non_empty("comment_text", &request.comment_text)?;
        }
        "owner_changed" => {
            let owner = request
                .metadata
                .get("new_human_owner")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .ok_or_else(|| {
                    anyhow::anyhow!("metadata.new_human_owner is required for owner_changed")
                })?;
            if owner.chars().count() > 100 {
                bail!("metadata.new_human_owner must be 100 characters or fewer");
            }
            validate_human_actor_id("metadata.new_human_owner", owner)?;
        }
        "attachment_added"
            if request
                .metadata
                .get("attachment_uri")
                .and_then(Value::as_str)
                .map(str::trim)
                .filter(|item| !item.is_empty())
                .is_none() =>
        {
            bail!("metadata.attachment_uri is required for attachment_added");
        }
        _ => {}
    }
    Ok(())
}

fn status_after_group_message_confirmation(request: &GroupMessageConfirmRequest) -> String {
    if request.decision == "confirmed" {
        "queued".to_string()
    } else {
        "cancelled".to_string()
    }
}

fn validate_high_risk_send(
    request: &WorkItemCreateRequest,
    capability: &Capability,
    policy: &OperationsPolicy,
) -> Result<()> {
    if capability.capability_key != "erhua.send_group_message" {
        return Ok(());
    }
    if request.work_item_type != "group_message_request" {
        bail!("erhua.send_group_message requires group_message_request");
    }
    approved_artifact_id(request)?;
    for field in ["target_channel", "message_text"] {
        if request
            .payload
            .get(field)
            .and_then(Value::as_str)
            .map(str::trim)
            .filter(|item| !item.is_empty())
            .is_none()
        {
            bail!("{field} is required for group message requests");
        }
    }
    let target_group_alias = request
        .payload
        .get("target_group_alias")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty());
    let target_group_id = request
        .payload
        .get("target_group_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty());
    if target_group_alias.is_none() && target_group_id.is_none() {
        bail!("target_group_alias or target_group_id is required for group message requests");
    }
    if !policy.group_allowed(target_group_alias, target_group_id) {
        bail!("target group is not allowlisted for group message requests");
    }
    Ok(())
}

fn approved_artifact_id(request: &WorkItemCreateRequest) -> Result<Uuid> {
    if let Some(id) = request.approved_artifact_id {
        return Ok(id);
    }
    let Some(text) = request
        .payload
        .get("approved_artifact_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
    else {
        bail!("approved_artifact_id is required for group message requests");
    };
    Uuid::parse_str(text).context("approved_artifact_id must be a uuid")
}

fn image_generation_approved_artifact_id(request: &WorkItemCreateRequest) -> Result<Uuid> {
    if let Some(id) = request.approved_artifact_id {
        return Ok(id);
    }
    let Some(text) = request
        .payload
        .get("approved_brief_artifact_id")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
    else {
        bail!("approved_brief_artifact_id is required for image generation requests");
    };
    Uuid::parse_str(text).context("approved_brief_artifact_id must be a uuid")
}

#[expect(
    clippy::too_many_arguments,
    reason = "work item reports preserve explicit lifecycle outcome fields"
)]
fn report_from_request(
    request: &WorkItemCreateRequest,
    capability: &Capability,
    _dry_run_only_workbench: bool,
    apply_requested: bool,
    action_status: &str,
    work_item_id: Option<Uuid>,
    existing: bool,
    current_status: &str,
) -> WorkItemCreateReport {
    WorkItemCreateReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        action_status: action_status.to_string(),
        current_status: current_status.to_string(),
        work_item_id,
        parent_work_item_id: request.parent_work_item_id,
        existing,
        capability_key: request.capability_key.clone(),
        work_item_type: request.work_item_type.clone(),
        requester_agent: request.requester_agent.clone(),
        target_agent: request.target_agent.clone(),
        idempotency_key: request.idempotency_key.clone(),
        dedupe_key: request.dedupe_key.clone(),
        risk_level: capability.risk_level.clone(),
        review_policy: capability.review_policy.clone(),
        human_workbench: HumanWorkbenchPlan {
            provider: "feishu_task".to_string(),
            intended_tasklist_name: "AgentOS · 运营协作工作台".to_string(),
            dry_run_only: true,
            title: format!("[{}] {}", request.work_item_type, request.brief_summary),
            description_fields: vec![
                "work_item_id".to_string(),
                "work_item_type".to_string(),
                "capability_key".to_string(),
                "requester_agent".to_string(),
                "target_agent".to_string(),
                "human_owner".to_string(),
                "source_refs".to_string(),
                "risk_level".to_string(),
                "review_policy".to_string(),
                "artifact_refs".to_string(),
                "current_status".to_string(),
            ],
        },
        limitations: vec![
            "Feishu Task mirror is planned through human_workbench_refs; this command does not create external tasks yet".to_string(),
            "Hermes Kanban is not a fallback path for new operations work items".to_string(),
        ],
        guardrails: vec![
            "cross-agent work must use capability_key and work_item_type".to_string(),
            "Postgres is the operations source of truth".to_string(),
            "external send/publish requires explicit high-risk capability policy".to_string(),
            "payloads must be redacted summaries with source refs, not raw private text".to_string(),
        ],
    }
}

fn review_report(
    request: &ArtifactReviewDecisionRequest,
    apply_requested: bool,
    action_status: &str,
    work_item_id: Option<Uuid>,
    previous_review_status: Option<String>,
) -> ArtifactReviewDecisionReport {
    ArtifactReviewDecisionReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        action_status: action_status.to_string(),
        artifact_id: request.artifact_id,
        work_item_id,
        previous_review_status,
        review_status: request.decision.clone(),
        reviewer_id: request.reviewer_id.clone(),
        reason_required: ["rejected", "changes_requested"].contains(&request.decision.as_str()),
        limitations: vec![
            "review decision recording does not publish, send, or mutate external channels"
                .to_string(),
            "Feishu Task comments and sections still require a separate sync worker".to_string(),
        ],
        guardrails: vec![
            "only approved, rejected, or changes_requested decisions are accepted".to_string(),
            "rejected and changes_requested decisions require a reason".to_string(),
            "generated_image approvals revalidate worker provenance and media metadata".to_string(),
            "review decisions are audited through work_item_events".to_string(),
        ],
    }
}

fn group_message_confirm_report(
    request: &GroupMessageConfirmRequest,
    apply_requested: bool,
    action_status: &str,
    previous_status: Option<String>,
    current_status: &str,
) -> GroupMessageConfirmReport {
    GroupMessageConfirmReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        action_status: action_status.to_string(),
        work_item_id: request.work_item_id,
        previous_status,
        current_status: current_status.to_string(),
        confirmer_id: request.confirmer_id.clone(),
        decision: request.decision.clone(),
        send_executed: false,
        limitations: vec![
            "final confirmation only updates AgentOS state; it does not send a group message"
                .to_string(),
            "a separate Erhua send worker must claim queued group_message_request items"
                .to_string(),
            "Feishu Task comments and sections still require a separate sync worker".to_string(),
        ],
        guardrails: vec![
            "only awaiting_publish group_message_request work items can be confirmed".to_string(),
            "confirmed moves the request to queued; cancelled moves it to cancelled".to_string(),
            "confirmation decisions are audited through work_item_events".to_string(),
        ],
    }
}

fn workbench_event_report(
    request: &WorkbenchEventRecordRequest,
    apply_requested: bool,
    action_status: &str,
) -> WorkbenchEventRecordReport {
    WorkbenchEventRecordReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        action_status: action_status.to_string(),
        work_item_id: request.work_item_id,
        artifact_id: request.artifact_id,
        provider: request.provider.clone(),
        external_id: request.external_id.clone(),
        external_event_id: request.external_event_id.clone(),
        event_type: request.event_type.clone(),
        actor_id: request.actor_id.clone(),
        mutates_work_item_state: false,
        recommended_command: recommended_command_for_workbench_event(request),
        limitations: vec![
            "workbench event recording is an audit intake path; it does not mutate work item state"
                .to_string(),
            "Feishu Task remains a human workbench, not the AgentOS source of truth".to_string(),
            "review, final confirmation, controlled cancellation, owner changes, and attachments use dedicated AgentOS policy paths after validation".to_string(),
        ],
        guardrails: vec![
            "apply mode requires an active human_workbench_ref for provider/external_id/work_item_id"
                .to_string(),
            "external_event_id provides idempotency when the upstream workbench supplies one"
                .to_string(),
            "comments and metadata are filtered for sensitive content before audit write".to_string(),
        ],
    }
}

fn workbench_event_process_report(
    event: &RecordedWorkbenchEvent,
    apply_requested: bool,
    action_status: &str,
    command_executed: Option<String>,
    state_mutation_recorded: bool,
) -> WorkbenchEventProcessReport {
    WorkbenchEventProcessReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        action_status: action_status.to_string(),
        event_id: event.id,
        work_item_id: event.work_item_id,
        artifact_id: event.artifact_id,
        workbench_event_type: event.workbench_event_type.clone(),
        command_executed,
        state_mutation_recorded,
        limitations: vec![
            "review, final-confirmation, controlled cancellation status-change, owner-change, and attachment events are processable".to_string(),
            "processing delegates to existing policy-checked AgentOS commands".to_string(),
            "comments and non-cancellation status-change events remain audit-only or policy-denied".to_string(),
        ],
        guardrails: vec![
            "human_workbench_event_processed provides idempotency for processed events".to_string(),
            "Feishu Task is still not the system source of truth".to_string(),
            "external send execution is not performed by this command".to_string(),
        ],
    }
}

fn workbench_event_worker_report(
    apply_requested: bool,
    action_status: &str,
    event_id: Option<Uuid>,
    process_report: Option<WorkbenchEventProcessReport>,
) -> WorkbenchEventWorkerReport {
    WorkbenchEventWorkerReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        worker: "workbench-event-worker",
        action_status: action_status.to_string(),
        event_id,
        process_report,
        limitations: vec![
            "worker currently supports --once only".to_string(),
            "review, final-confirmation, controlled cancellation status-change, owner-change, and attachment workbench events are processable".to_string(),
            "comments and non-cancellation status-change requests remain audit-only or policy-denied".to_string(),
        ],
        guardrails: vec![
            "processing delegates to existing AgentOS policy commands".to_string(),
            "processed events are idempotent through human_workbench_event_processed".to_string(),
            "no Feishu, QiWe, or external publish adapter is called".to_string(),
        ],
    }
}

fn workflow_sync_worker_report(
    apply_requested: bool,
    requested_work_item_id: Option<Uuid>,
    root_work_item_id: Option<Uuid>,
    sync_report: Option<WorkflowSyncReport>,
    action_status: &str,
) -> WorkflowSyncWorkerReport {
    WorkflowSyncWorkerReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        worker: "workflow-sync-worker",
        action_status: action_status.to_string(),
        requested_work_item_id,
        root_work_item_id,
        sync_report,
        limitations: vec![
            "workflow sync worker updates only AgentOS parent summaries".to_string(),
            "worker currently supports --once only; systemd/timer scheduling is external"
                .to_string(),
            "recursive summary reporting is not a general DAG scheduler".to_string(),
        ],
        guardrails: vec![
            "worker does not execute child workers or external adapters".to_string(),
            "Postgres remains the operations source of truth".to_string(),
            "Hermes Kanban is not used as a workflow fallback".to_string(),
        ],
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the worker report exposes each queue count explicitly"
)]
fn xiaoman_activity_promotion_starter_report(
    check_only: bool,
    apply_requested: bool,
    requested_work_item_id: Option<Uuid>,
    scanned_count: usize,
    created_count: usize,
    existing_count: usize,
    missing_child_count: usize,
    action_status: &str,
    work_items: Vec<WorkItemCreateReport>,
) -> XiaomanActivityPromotionStarterWorkerReport {
    XiaomanActivityPromotionStarterWorkerReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        check_only,
        worker: "xiaoman-activity-promotion-starter-worker",
        source: "agentos_work_items",
        action_status: action_status.to_string(),
        requested_work_item_id,
        scanned_count,
        created_count,
        existing_count,
        missing_child_count,
        safe_for_chat: false,
        work_items,
        limitations: vec![
            "starter worker only creates missing AgentOS evidence/visual child work_items"
                .to_string(),
            "starter worker does not execute evidence, collaboration, group-send, or external adapters"
                .to_string(),
            "systemd/timer scheduling is intentionally out of scope for this worker change"
                .to_string(),
        ],
        guardrails: vec![
            "Postgres AgentOS work_items remain the source of truth".to_string(),
            "child work items use parent work_item_id based idempotency keys".to_string(),
            "Feishu, QiWe, visual generation, and external sends are not called".to_string(),
        ],
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the worker report exposes each queue count explicitly"
)]
fn xiaoman_activity_send_request_starter_report(
    check_only: bool,
    apply_requested: bool,
    requested_work_item_id: Option<Uuid>,
    scanned_count: usize,
    created_count: usize,
    existing_count: usize,
    missing_child_count: usize,
    action_status: &str,
    work_items: Vec<WorkItemCreateReport>,
) -> XiaomanActivitySendRequestStarterWorkerReport {
    XiaomanActivitySendRequestStarterWorkerReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        check_only,
        worker: "xiaoman-activity-send-request-starter-worker",
        source: "agentos_work_items",
        action_status: action_status.to_string(),
        requested_work_item_id,
        scanned_count,
        created_count,
        existing_count,
        missing_child_count,
        safe_for_chat: false,
        work_items,
        limitations: vec![
            "starter worker only creates awaiting_publish AgentOS group_message_request work_items"
                .to_string(),
            "starter worker does not confirm, queue, send, publish, or call QiWe/Erhua adapters"
                .to_string(),
            "only approved poster_brief artifacts under Xiaoman activity promotion parents are eligible"
                .to_string(),
        ],
        guardrails: vec![
            "group_message_request still requires human final confirmation before it can be queued"
                .to_string(),
            "approved artifact and target group allowlist policies are enforced by operations create"
                .to_string(),
            "Postgres AgentOS work_items remain the source of truth".to_string(),
        ],
    }
}

#[expect(
    clippy::too_many_arguments,
    reason = "the worker report exposes each queue count explicitly"
)]
fn xiaoman_activity_image_generation_starter_report(
    check_only: bool,
    apply_requested: bool,
    requested_work_item_id: Option<Uuid>,
    scanned_count: usize,
    created_count: usize,
    existing_count: usize,
    missing_child_count: usize,
    action_status: &str,
    work_items: Vec<WorkItemCreateReport>,
) -> XiaomanActivityImageGenerationStarterWorkerReport {
    XiaomanActivityImageGenerationStarterWorkerReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        check_only,
        worker: "xiaoman-activity-image-generation-starter-worker",
        source: "agentos_work_items",
        action_status: action_status.to_string(),
        requested_work_item_id,
        scanned_count,
        created_count,
        existing_count,
        missing_child_count,
        safe_for_chat: false,
        work_items,
        limitations: vec![
            "starter worker only creates AgentOS image_generation_request work_items from approved poster_brief artifacts".to_string(),
            "starter worker does not call image providers, upload media, write Feishu, send QiWe, or publish".to_string(),
            "runtime scheduling may only invoke this starter and must not invoke the image provider worker".to_string(),
        ],
        guardrails: vec![
            "Postgres AgentOS work_items and artifact review remain the source of truth".to_string(),
            "idempotency includes the approved brief, output specification, and redacted prompt hash".to_string(),
            "an image-generation worker must remain disabled until its provider and media boundary are owner-reviewed".to_string(),
        ],
    }
}

fn recommended_command_for_workbench_event(
    request: &WorkbenchEventRecordRequest,
) -> Option<String> {
    match request.event_type.as_str() {
        "review_decision_requested" => Some("operations-artifact-review-decision".to_string()),
        "final_confirmation_requested" => Some("operations-group-message-confirm".to_string()),
        "status_change_requested" => Some("operations-workbench-status-change".to_string()),
        "owner_changed" => Some("operations-workbench-owner-change".to_string()),
        "attachment_added" => Some("operations-workbench-attachment-add".to_string()),
        _ => None,
    }
}

fn builtin_capability(capability_key: &str) -> Option<Capability> {
    match capability_key {
        "huabaosi.create_visual_asset" => Some(Capability {
            capability_key: capability_key.to_string(),
            provider_agent: "huabaosi".to_string(),
            display_name: "画报司生成视觉素材".to_string(),
            description: "根据脱敏运营上下文生成海报 brief、视觉 prompt 或宣发文案草稿".to_string(),
            allowed_callers: vec![
                "xiaoman".to_string(),
                "silaoshi".to_string(),
                "default".to_string(),
            ],
            allowed_work_item_types: vec![
                "visual_asset_request".to_string(),
                "activity_promotion_request".to_string(),
            ],
            risk_level: "medium".to_string(),
            review_policy: "before_external_use".to_string(),
            enabled: true,
        }),
        "huabaosi.generate_image_asset" => Some(Capability {
            capability_key: capability_key.to_string(),
            provider_agent: "huabaosi".to_string(),
            display_name: "画报司生成审核前图片素材".to_string(),
            description: "基于已审核海报 brief 创建受控图片生成请求；生成结果仍需人工审核"
                .to_string(),
            allowed_callers: vec!["xiaoman".to_string(), "default".to_string()],
            allowed_work_item_types: vec!["image_generation_request".to_string()],
            risk_level: "high".to_string(),
            review_policy: "before_external_use".to_string(),
            enabled: true,
        }),
        "erhua.send_group_message" => Some(Capability {
            capability_key: capability_key.to_string(),
            provider_agent: "erhua".to_string(),
            display_name: "二花受控群发消息".to_string(),
            description: "把已审核产物发送到白名单社群；默认需要人工最终确认".to_string(),
            allowed_callers: vec![
                "xiaoman".to_string(),
                "silaoshi".to_string(),
                "default".to_string(),
            ],
            allowed_work_item_types: vec!["group_message_request".to_string()],
            risk_level: "high".to_string(),
            review_policy: "human_final_confirmation".to_string(),
            enabled: true,
        }),
        "wenyuange.retrieve_evidence" => Some(Capability {
            capability_key: capability_key.to_string(),
            provider_agent: "wenyuange".to_string(),
            display_name: "文渊阁检索运营证据".to_string(),
            description: "为运营任务检索可追溯资料和背景证据，不修改业务数据".to_string(),
            allowed_callers: vec![
                "xiaoman".to_string(),
                "huabaosi".to_string(),
                "silaoshi".to_string(),
                "default".to_string(),
            ],
            allowed_work_item_types: vec![
                "evidence_request".to_string(),
                "activity_promotion_request".to_string(),
            ],
            risk_level: "medium".to_string(),
            review_policy: "not_required".to_string(),
            enabled: true,
        }),
        "xiaoman.create_activity_request" => Some(Capability {
            capability_key: capability_key.to_string(),
            provider_agent: "xiaoman".to_string(),
            display_name: "小满创建活动运营请求".to_string(),
            description: "从活动信号或人工输入创建结构化活动运营请求".to_string(),
            allowed_callers: vec!["default".to_string(), "silaoshi".to_string()],
            allowed_work_item_types: vec!["activity_promotion_request".to_string()],
            risk_level: "medium".to_string(),
            review_policy: "before_external_use".to_string(),
            enabled: true,
        }),
        _ => None,
    }
}

fn require_non_empty(name: &str, value: &str) -> Result<()> {
    if value.trim().is_empty() {
        bail!("{name} is required");
    }
    Ok(())
}

fn non_empty_or_default(value: &str, default_value: &str) -> String {
    if value.trim().is_empty() {
        default_value.to_string()
    } else {
        value.trim().to_string()
    }
}

fn non_empty_object_text(value: &Value, field: &str) -> Option<String> {
    value
        .get(field)
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(ToString::to_string)
}

fn normalize_key(value: &str) -> String {
    value.trim().to_ascii_lowercase().replace(' ', "_")
}

fn dedupe_key(request: &WorkItemCreateRequest) -> String {
    let mut hasher = Sha256::new();
    hasher.update(request.requester_agent.as_bytes());
    hasher.update(b"|");
    hasher.update(request.target_agent.as_bytes());
    hasher.update(b"|");
    hasher.update(request.capability_key.as_bytes());
    hasher.update(b"|");
    hasher.update(request.work_item_type.as_bytes());
    hasher.update(b"|");
    hasher.update(request.source_type.as_bytes());
    hasher.update(b"|");
    hasher.update(request.source_refs.to_string().as_bytes());
    hasher.update(b"|");
    hasher.update(request.brief_summary.as_bytes());
    format!("ops:{:x}", hasher.finalize())
}

fn contains_sensitive_value(value: &Value) -> bool {
    match value {
        Value::String(text) => contains_sensitive_text(text),
        Value::Array(items) => items.iter().any(contains_sensitive_value),
        Value::Object(map) => map
            .iter()
            .any(|(key, value)| contains_sensitive_key(key) || contains_sensitive_value(value)),
        _ => false,
    }
}

fn contains_sensitive_key(key: &str) -> bool {
    let key = key.to_ascii_lowercase();
    [
        "token",
        "secret",
        "app_secret",
        "app_token",
        "table_id",
        "base_token",
        "system_prompt",
        "raw_chat_text",
        "member_dossier",
    ]
    .iter()
    .any(|needle| key.contains(needle))
}

fn contains_sensitive_text(text: &str) -> bool {
    let lower = text.to_ascii_lowercase();
    [
        "app_token",
        "tenant_access_token",
        "authorization: bearer",
        "base table",
        "system prompt",
        "raw private chat",
        "member dossier",
        "lark-base",
        "hermes kanban",
    ]
    .iter()
    .any(|needle| lower.contains(needle))
}

fn trim_error(error: &str) -> String {
    const MAX: usize = 500;
    let trimmed = error.trim();
    if trimmed.chars().count() <= MAX {
        return trimmed.to_string();
    }
    trimmed.chars().take(MAX).collect()
}

fn content_hash_text(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn default_priority() -> String {
    "normal".to_string()
}

fn default_payload_redaction_policy() -> String {
    "summary_only".to_string()
}

#[allow(dead_code)]
fn _allowed_status(value: &str) -> bool {
    ALLOWED_STATUSES.contains(&value)
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use serde_json::json;

    fn request(value: Value) -> WorkItemCreateRequest {
        serde_json::from_value(value).expect("request should deserialize")
    }

    fn xiaoman_promotion_candidate(
        missing_evidence_child: bool,
        missing_visual_child: bool,
    ) -> XiaomanActivityPromotionCandidate {
        XiaomanActivityPromotionCandidate {
            id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            brief_summary: "AgentOS 小满活动宣发".to_string(),
            source_type: "event_signal".to_string(),
            source_refs: json!({"event_signal_id": "evt_xiaoman_activity"}),
            source_event_signal_id: Some(
                Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            ),
            priority: "normal".to_string(),
            human_owner: "xiaoman-owner".to_string(),
            missing_evidence_child,
            missing_visual_child,
        }
    }

    fn xiaoman_send_candidate() -> XiaomanActivitySendRequestCandidate {
        XiaomanActivitySendRequestCandidate {
            parent_id: Uuid::parse_str("11111111-1111-4111-8111-111111111111").unwrap(),
            visual_work_item_id: Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
            image_generation_work_item_id: Uuid::parse_str("55555555-5555-4555-8555-555555555555")
                .unwrap(),
            approved_artifact_id: Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap(),
            brief_summary: "AgentOS 小满活动宣发".to_string(),
            source_type: "event_signal".to_string(),
            source_refs: json!({"event_signal_id": "evt_xiaoman_activity"}),
            source_event_signal_id: Some(
                Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            ),
            priority: "normal".to_string(),
            human_owner: "xiaoman-owner".to_string(),
        }
    }

    fn xiaoman_image_candidate() -> XiaomanActivityImageGenerationCandidate {
        XiaomanActivityImageGenerationCandidate {
            visual_work_item_id: Uuid::parse_str("33333333-3333-4333-8333-333333333333").unwrap(),
            approved_artifact_id: Uuid::parse_str("44444444-4444-4444-8444-444444444444").unwrap(),
            approved_brief_hash: "sha256:approved-poster-brief".to_string(),
            evidence_content_hash: Some("sha256:evidence-summary".to_string()),
            brief_summary: "AgentOS 小满活动宣发".to_string(),
            source_type: "event_signal".to_string(),
            source_refs: json!({"event_signal_id": "evt_xiaoman_activity"}),
            source_event_signal_id: Some(
                Uuid::parse_str("22222222-2222-4222-8222-222222222222").unwrap(),
            ),
            priority: "normal".to_string(),
            human_owner: "xiaoman-owner".to_string(),
        }
    }

    #[test]
    fn xiaoman_activity_promotion_starter_preview_builds_two_children() {
        let candidate = xiaoman_promotion_candidate(true, true);
        let requests = xiaoman_activity_promotion_child_requests(&candidate);

        assert_eq!(requests.len(), 2);
        assert_eq!(requests[0].capability_key, "wenyuange.retrieve_evidence");
        assert_eq!(requests[0].work_item_type, "evidence_request");
        assert_eq!(requests[0].parent_work_item_id, Some(candidate.id));
        assert_eq!(
            requests[0].idempotency_key,
            format!("xiaoman_activity_promotion:{}:evidence-child", candidate.id)
        );
        assert_eq!(requests[1].capability_key, "huabaosi.create_visual_asset");
        assert_eq!(requests[1].work_item_type, "visual_asset_request");
        assert_eq!(requests[1].parent_work_item_id, Some(candidate.id));
        assert_eq!(
            requests[1].idempotency_key,
            format!("xiaoman_activity_promotion:{}:visual-child", candidate.id)
        );
    }

    #[test]
    fn xiaoman_activity_promotion_starter_only_builds_missing_visual_child() {
        let candidate = xiaoman_promotion_candidate(false, true);
        let requests = xiaoman_activity_promotion_child_requests(&candidate);

        assert_eq!(requests.len(), 1);
        assert_eq!(requests[0].capability_key, "huabaosi.create_visual_asset");
        assert_eq!(
            requests[0].source_event_signal_id,
            candidate.source_event_signal_id
        );
        assert_eq!(requests[0].priority, "normal");
        assert_eq!(requests[0].human_owner, "xiaoman-owner");
    }

    #[test]
    fn xiaoman_activity_promotion_starter_builds_no_duplicate_children() {
        let candidate = xiaoman_promotion_candidate(false, false);
        let requests = xiaoman_activity_promotion_child_requests(&candidate);

        assert!(requests.is_empty());
    }

    #[test]
    fn xiaoman_activity_promotion_evidence_child_preserves_event_signal_scope() {
        let candidate = xiaoman_promotion_candidate(true, false);
        let requests = xiaoman_activity_promotion_child_requests(&candidate);
        let evidence = &requests[0];

        assert_eq!(requests.len(), 1);
        assert_eq!(evidence.target_agent, "wenyuange");
        assert_eq!(evidence.source_type, "event_signal");
        assert_eq!(
            evidence.source_event_signal_id,
            candidate.source_event_signal_id
        );
        assert_eq!(evidence.payload["workflow_type"], "activity_promotion");
        assert_eq!(evidence.payload["planner_intent"], "retrieve_evidence");
        assert_eq!(
            evidence.metadata["parent_activity_request_work_item_id"],
            candidate.id.to_string()
        );
    }

    #[test]
    fn xiaoman_image_generation_request_uses_approved_brief_and_stable_idempotency() {
        let candidate = xiaoman_image_candidate();
        let first = xiaoman_activity_image_generation_request(&candidate);
        let second = xiaoman_activity_image_generation_request(&candidate);
        let report = create_work_item_dry_run(first.clone())
            .expect("image generation request should validate in dry-run");
        let payload = first.payload.to_string();
        let metadata = first.metadata.to_string();

        assert_eq!(first.requester_agent, "xiaoman");
        assert_eq!(first.target_agent, "huabaosi");
        assert_eq!(first.capability_key, "huabaosi.generate_image_asset");
        assert_eq!(first.work_item_type, "image_generation_request");
        assert_eq!(report.risk_level, "high");
        assert_eq!(
            first.parent_work_item_id,
            Some(candidate.visual_work_item_id)
        );
        assert_eq!(
            first.approved_artifact_id,
            Some(candidate.approved_artifact_id)
        );
        assert_eq!(first.idempotency_key, second.idempotency_key);
        assert!(first.idempotency_key.starts_with(&format!(
            "huabaosi_image:{}:community_poster_1024x1024:sha256:",
            candidate.approved_artifact_id
        )));
        assert_eq!(
            first.payload["approved_brief_content_hash"],
            candidate.approved_brief_hash
        );
        assert_eq!(
            first.payload["evidence_content_hash"],
            candidate.evidence_content_hash.unwrap()
        );
        assert!(!payload.contains("api_key"));
        assert!(!payload.contains("table_id"));
        assert!(!metadata.contains("message_id"));
    }

    #[test]
    fn image_generation_request_rejects_mismatched_approved_artifact() {
        let candidate = xiaoman_image_candidate();
        let mut request = xiaoman_activity_image_generation_request(&candidate);
        request.approved_artifact_id = Some(Uuid::new_v4());

        let err = create_work_item_dry_run(request)
            .expect_err("mismatched approved artifact ids must be rejected");
        assert!(err
            .to_string()
            .contains("approved_artifact_id must match payload"));
    }

    #[test]
    fn xiaoman_activity_promotion_starter_report_is_safe_for_chat() {
        let report = xiaoman_activity_promotion_starter_report(
            true,
            false,
            None,
            0,
            0,
            0,
            0,
            "no_eligible_activity_requests",
            Vec::new(),
        );
        let raw = serde_json::to_string(&report).expect("report serializes");

        assert_eq!(report.worker, "xiaoman-activity-promotion-starter-worker");
        assert_eq!(report.source, "agentos_work_items");
        assert!(!report.safe_for_chat);
        assert!(!raw.contains("token"));
        assert!(!raw.contains("table_id"));
        assert!(!raw.contains("message_id"));
    }

    #[test]
    fn readiness_report_requires_production_allowlists() {
        let cli = Cli::parse_from(["qintopia-message-sidecar", "operations-readiness-check"]);
        let report = readiness_report(&cli, "production", false).expect("report should build");

        assert!(!report.success);
        assert_eq!(report.action_status, "missing_required_configuration");
        assert!(report
            .missing_required
            .contains(&"postgres_database_url".to_string()));
        assert!(report
            .missing_required
            .contains(&"allowed_group_targets".to_string()));
        assert!(report
            .missing_required
            .contains(&"allowed_reviewers".to_string()));
        assert!(report
            .missing_required
            .contains(&"allowed_confirmers".to_string()));
        assert!(report
            .missing_required
            .contains(&"allowed_owners".to_string()));
        assert!(report
            .missing_required
            .contains(&"allowed_attachment_hosts".to_string()));
    }

    #[test]
    fn readiness_report_accepts_configured_production_gate() {
        let cli = Cli::parse_from([
            "qintopia-message-sidecar",
            "--database-url",
            "postgres://example.invalid/qintopia",
            "--operations-allowed-group-aliases",
            "community_activity_group",
            "--operations-allowed-reviewer-ids",
            "reviewer-a",
            "--operations-allowed-confirmer-ids",
            "confirmer-a",
            "--operations-allowed-owner-ids",
            "owner-a",
            "--operations-allowed-attachment-hosts",
            "assets.example.com",
            "operations-readiness-check",
        ]);
        let report = readiness_report(&cli, "production", true).expect("report should build");

        assert!(report.success);
        assert!(report.ready_for_production_adapters);
        assert_eq!(report.missing_required.len(), 0);
        assert!(report
            .checks
            .iter()
            .any(|check| check.key == "allowed_attachment_hosts"
                && check.configured
                && check.configured_count == 1));
    }

    #[test]
    fn readiness_report_rejects_unknown_profile() {
        let cli = Cli::parse_from(["qintopia-message-sidecar", "operations-readiness-check"]);
        let err = readiness_report(&cli, "staging", false).expect_err("unknown profile rejected");
        assert!(err
            .to_string()
            .contains("readiness profile must be production or apply_smoke"));
    }

    #[test]
    fn dry_run_accepts_xiaoman_to_huabaosi_visual_request() {
        let report = create_work_item_dry_run(request(json!({
            "requester_agent": "xiaoman",
            "target_agent": "huabaosi",
            "capability_key": "huabaosi.create_visual_asset",
            "work_item_type": "visual_asset_request",
            "brief_summary": "周末共创晚餐活动运营海报",
            "source_type": "event_signal",
            "source_refs": {"event_signal_id": "evt_demo"}
        })))
        .expect("visual request should validate");

        assert_eq!(report.action_status, "dry_run_ok");
        assert_eq!(report.current_status, "queued");
        assert_eq!(report.review_policy, "before_external_use");
        assert_eq!(report.human_workbench.provider, "feishu_task");
        assert!(report
            .limitations
            .iter()
            .any(|item| item.contains("does not create external tasks yet")));
    }

    #[test]
    fn builtin_capability_list_exposes_first_operations_capabilities() {
        let report = capability_list_from_builtin();

        assert_eq!(report.source, "builtin");
        assert_eq!(report.capability_count, 5);
        assert!(report.capabilities.iter().any(|item| {
            item.capability_key == "huabaosi.create_visual_asset"
                && item.provider_agent == "huabaosi"
                && item.safe_for_non_technical_request
        }));
        assert!(report.capabilities.iter().any(|item| {
            item.capability_key == "erhua.send_group_message"
                && item.risk_level == "high"
                && item.review_policy == "human_final_confirmation"
        }));
        assert!(report.capabilities.iter().any(|item| {
            item.capability_key == "huabaosi.generate_image_asset"
                && item
                    .allowed_work_item_types
                    .contains(&"image_generation_request".to_string())
        }));
    }

    #[test]
    fn request_plan_maps_poster_request_to_huabaosi() {
        let report = plan_request(
            serde_json::from_value(json!({
                "actor_agent": "xiaoman",
                "request_text": "请根据周末活动生成一张运营海报",
                "source_type": "manual_request",
                "source_refs": {"source_record_ref": "activity_occurrence:test"}
            }))
            .expect("plan input should deserialize"),
        )
        .expect("poster request should plan");

        assert_eq!(report.action_status, "planned");
        let request = report
            .work_item_request
            .expect("work item request should exist");
        assert_eq!(request.capability_key, "huabaosi.create_visual_asset");
        assert_eq!(request.work_item_type, "visual_asset_request");
        assert!(report.work_item_preview.is_some());
    }

    #[test]
    fn request_submit_dry_run_maps_poster_request_to_work_item_preview() {
        let report = submit_request_dry_run(
            serde_json::from_value(json!({
                "actor_agent": "xiaoman",
                "request_text": "请根据周末活动生成一张运营海报",
                "source_type": "manual_request",
                "source_refs": {"source_record_ref": "activity_occurrence:test"}
            }))
            .expect("plan input should deserialize"),
        )
        .expect("poster request should submit dry-run");

        assert_eq!(report.action_status, "dry_run_ok");
        let result = report
            .work_item_result
            .expect("work item result should exist");
        assert_eq!(result.capability_key, "huabaosi.create_visual_asset");
        assert_eq!(result.current_status, "queued");
        assert!(result.dry_run);
    }

    #[test]
    fn request_plan_maps_evidence_request_to_wenyuange() {
        let report = plan_request(
            serde_json::from_value(json!({
                "actor_agent": "xiaoman",
                "request_text": "帮我检索这个活动的历史复盘资料",
                "source_type": "manual_request",
                "source_refs": {"source_record_ref": "activity_occurrence:test"}
            }))
            .expect("plan input should deserialize"),
        )
        .expect("evidence request should plan");

        let request = report
            .work_item_request
            .expect("work item request should exist");
        assert_eq!(request.capability_key, "wenyuange.retrieve_evidence");
        assert_eq!(request.work_item_type, "evidence_request");
    }

    #[test]
    fn request_plan_asks_clarification_for_group_send_without_artifact() {
        let report = plan_request(
            serde_json::from_value(json!({
                "actor_agent": "xiaoman",
                "request_text": "请让二花把活动海报发群",
                "source_type": "manual_request",
                "source_refs": {"source_record_ref": "activity_occurrence:test"}
            }))
            .expect("plan input should deserialize"),
        )
        .expect("ambiguous send should not fail");

        assert_eq!(report.action_status, "needs_clarification");
        assert!(report.work_item_request.is_none());
        assert!(!report.clarification_questions.is_empty());
    }

    #[test]
    fn request_submit_dry_run_keeps_ambiguous_group_send_as_clarification() {
        let report = submit_request_dry_run(
            serde_json::from_value(json!({
                "actor_agent": "xiaoman",
                "request_text": "请让二花把活动海报发群",
                "source_type": "manual_request",
                "source_refs": {"source_record_ref": "activity_occurrence:test"}
            }))
            .expect("plan input should deserialize"),
        )
        .expect("ambiguous send should not fail");

        assert_eq!(report.action_status, "needs_clarification");
        assert!(report.work_item_result.is_none());
        assert!(!report.clarification_questions.is_empty());
    }

    #[test]
    fn xiaoman_send_request_starter_builds_awaiting_publish_group_message() {
        let candidate = xiaoman_send_candidate();
        let request = xiaoman_activity_send_request(
            &candidate,
            "community_activity_group",
            "活动海报已审核，请确认是否发送。",
        )
        .expect("request should build");
        let report = create_work_item_dry_run(request.clone()).expect("request should validate");

        assert_eq!(report.capability_key, "erhua.send_group_message");
        assert_eq!(report.work_item_type, "group_message_request");
        assert_eq!(report.current_status, "awaiting_publish");
        assert_eq!(report.review_policy, "human_final_confirmation");
        assert_eq!(report.parent_work_item_id, Some(candidate.parent_id));
        assert_eq!(request.payload["approved_artifact_type"], "generated_image");
        assert_eq!(
            request.payload["image_generation_work_item_id"],
            candidate.image_generation_work_item_id.to_string()
        );
        assert_eq!(
            report.idempotency_key,
            format!(
                "xiaoman_activity_promotion:{}:group-message-child",
                candidate.parent_id
            )
        );
    }

    #[test]
    fn xiaoman_send_request_starter_rejects_sensitive_message_text() {
        let candidate = xiaoman_send_candidate();
        let err = xiaoman_activity_send_request(
            &candidate,
            "community_activity_group",
            "raw private chat transcript",
        )
        .expect_err("sensitive message should be rejected");

        assert!(err
            .to_string()
            .contains("message_text contains disallowed sensitive"));
    }

    #[test]
    fn xiaoman_send_request_starter_rejects_empty_group_alias() {
        let candidate = xiaoman_send_candidate();
        let err =
            xiaoman_activity_send_request(&candidate, "  ", "活动海报已审核，请确认是否发送。")
                .expect_err("target group alias is required");

        assert!(err.to_string().contains("target_group_alias is required"));
    }

    #[test]
    fn xiaoman_send_request_starter_rejects_overlong_message_text() {
        let candidate = xiaoman_send_candidate();
        let long_message = "活动".repeat(251);
        let err =
            xiaoman_activity_send_request(&candidate, "community_activity_group", &long_message)
                .expect_err("message text length must be bounded");

        assert!(err
            .to_string()
            .contains("message_text must be 500 characters or fewer"));
    }

    #[test]
    fn xiaoman_send_request_starter_report_is_safe_for_chat() {
        let report = xiaoman_activity_send_request_starter_report(
            true,
            false,
            None,
            1,
            0,
            0,
            1,
            "group_message_requests_preview",
            Vec::new(),
        );

        assert_eq!(
            report.worker,
            "xiaoman-activity-send-request-starter-worker"
        );
        assert_eq!(report.source, "agentos_work_items");
        assert!(!report.safe_for_chat);
        assert!(report
            .limitations
            .iter()
            .any(|item| item.contains("does not confirm")));
    }

    #[test]
    fn workflow_start_dry_run_creates_activity_parent_evidence_and_visual_children() {
        let report = start_workflow_dry_run(
            serde_json::from_value(json!({
                "actor_agent": "xiaoman",
                "workflow_type": "activity_promotion",
                "request_text": "请根据周末活动生成一张运营海报",
                "source_type": "manual_request",
                "source_refs": {"source_record_ref": "activity_occurrence:test"}
            }))
            .expect("workflow request should deserialize"),
        )
        .expect("workflow should validate");

        assert_eq!(report.action_status, "dry_run_ok");
        assert_eq!(
            report.parent_work_item.work_item_type,
            "activity_promotion_request"
        );
        assert_eq!(
            report.parent_work_item.capability_key,
            "xiaoman.create_activity_request"
        );
        assert_eq!(report.child_work_items.len(), 2);
        assert_eq!(
            report.child_work_items[0].capability_key,
            "wenyuange.retrieve_evidence"
        );
        assert_eq!(
            report.child_work_items[0].work_item_type,
            "evidence_request"
        );
        assert_eq!(
            report.child_work_items[1].capability_key,
            "huabaosi.create_visual_asset"
        );
        assert_eq!(
            report.child_work_items[1].work_item_type,
            "visual_asset_request"
        );
        assert!(report.dry_run);
    }

    #[test]
    fn workflow_start_rejects_unknown_workflow_type() {
        let err = start_workflow_dry_run(
            serde_json::from_value(json!({
                "actor_agent": "xiaoman",
                "workflow_type": "unknown",
                "request_text": "请生成活动海报"
            }))
            .expect("workflow request should deserialize"),
        )
        .expect_err("unknown workflow should be rejected");

        assert!(err.to_string().contains("workflow_type is not supported"));
    }

    #[test]
    fn workflow_start_rejects_sensitive_request_text() {
        let err = start_workflow_dry_run(
            serde_json::from_value(json!({
                "actor_agent": "xiaoman",
                "workflow_type": "activity_promotion",
                "request_text": "请根据 app_token 生成活动海报",
                "source_type": "manual_request",
                "source_refs": {"source_record_ref": "activity_occurrence:test"}
            }))
            .expect("workflow request should deserialize"),
        )
        .expect_err("sensitive workflow request should be rejected");

        assert!(err
            .to_string()
            .contains("workflow start payload contains disallowed sensitive"));
    }

    #[test]
    fn request_plan_rejects_sensitive_text() {
        let err = plan_request(
            serde_json::from_value(json!({
                "actor_agent": "xiaoman",
                "request_text": "这里包含 app_token",
                "source_type": "manual_request",
                "source_refs": {"source_record_ref": "activity_occurrence:test"}
            }))
            .expect("plan input should deserialize"),
        )
        .expect_err("sensitive text should be rejected");

        assert!(err
            .to_string()
            .contains("request plan payload contains disallowed sensitive"));
    }

    #[test]
    fn dry_run_accepts_xiaoman_to_erhua_group_send_with_approved_artifact() {
        let report = create_work_item_dry_run(request(json!({
            "requester_agent": "xiaoman",
            "target_agent": "erhua",
            "capability_key": "erhua.send_group_message",
            "work_item_type": "group_message_request",
            "brief_summary": "发送已审核活动海报到社区活动群",
            "source_type": "manual_request",
            "source_refs": {"source_record_ref": "activity_occurrence:test"},
            "payload": {
                "approved_artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
                "target_channel": "qiwe",
                "target_group_alias": "community_activity_group",
                "message_text": "周末共创晚餐报名开始啦"
            }
        })))
        .expect("group send should validate");

        assert_eq!(report.risk_level, "high");
        assert_eq!(report.review_policy, "human_final_confirmation");
        assert_eq!(report.current_status, "awaiting_publish");
    }

    #[test]
    fn rejects_group_send_without_approved_artifact() {
        let err = create_work_item_dry_run(request(json!({
            "requester_agent": "xiaoman",
            "target_agent": "erhua",
            "capability_key": "erhua.send_group_message",
            "work_item_type": "group_message_request",
            "brief_summary": "发送活动海报",
            "source_type": "manual_request",
            "source_refs": {"source_record_ref": "activity_occurrence:test"},
            "payload": {
                "target_channel": "qiwe",
                "target_group_alias": "community_activity_group",
                "message_text": "周末共创晚餐报名开始啦"
            }
        })))
        .expect_err("approved artifact should be required");

        assert!(err.to_string().contains("approved_artifact_id is required"));
    }

    #[test]
    fn rejects_group_send_to_non_allowlisted_group() {
        let err = create_work_item_dry_run(request(json!({
            "requester_agent": "xiaoman",
            "target_agent": "erhua",
            "capability_key": "erhua.send_group_message",
            "work_item_type": "group_message_request",
            "brief_summary": "发送活动海报",
            "source_type": "manual_request",
            "source_refs": {"source_record_ref": "activity_occurrence:test"},
            "payload": {
                "approved_artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
                "target_channel": "qiwe",
                "target_group_alias": "unknown_group",
                "message_text": "周末共创晚餐报名开始啦"
            }
        })))
        .expect_err("group alias should be allowlisted");

        assert!(err.to_string().contains("target group is not allowlisted"));
    }

    #[test]
    fn rejects_group_send_with_non_uuid_approved_artifact() {
        let err = create_work_item_dry_run(request(json!({
            "requester_agent": "xiaoman",
            "target_agent": "erhua",
            "capability_key": "erhua.send_group_message",
            "work_item_type": "group_message_request",
            "brief_summary": "发送活动海报",
            "source_type": "manual_request",
            "source_refs": {"source_record_ref": "activity_occurrence:test"},
            "payload": {
                "approved_artifact_id": "not-a-uuid",
                "target_channel": "qiwe",
                "target_group_alias": "community_activity_group",
                "message_text": "周末共创晚餐报名开始啦"
            }
        })))
        .expect_err("approved artifact should be uuid");

        assert!(err
            .to_string()
            .contains("approved_artifact_id must be a uuid"));
    }

    #[test]
    fn rejects_target_that_does_not_provide_capability() {
        let err = create_work_item_dry_run(request(json!({
            "requester_agent": "xiaoman",
            "target_agent": "erhua",
            "capability_key": "huabaosi.create_visual_asset",
            "work_item_type": "visual_asset_request",
            "brief_summary": "周末活动海报",
            "source_type": "manual_request",
            "source_refs": {"source_record_ref": "activity_occurrence:test"}
        })))
        .expect_err("target must match provider");

        assert!(err
            .to_string()
            .contains("target_agent does not match capability provider"));
    }

    #[test]
    fn rejects_caller_not_allowed_for_capability() {
        let err = create_work_item_dry_run(request(json!({
            "requester_agent": "erhua",
            "target_agent": "huabaosi",
            "capability_key": "huabaosi.create_visual_asset",
            "work_item_type": "visual_asset_request",
            "brief_summary": "周末活动海报",
            "source_type": "manual_request",
            "source_refs": {"source_record_ref": "activity_occurrence:test"}
        })))
        .expect_err("caller should be rejected");

        assert!(err
            .to_string()
            .contains("requester_agent is not allowed for capability"));
    }

    #[test]
    fn rejects_sensitive_payload_content() {
        let err = create_work_item_dry_run(request(json!({
            "requester_agent": "xiaoman",
            "target_agent": "huabaosi",
            "capability_key": "huabaosi.create_visual_asset",
            "work_item_type": "visual_asset_request",
            "brief_summary": "周末活动海报",
            "source_type": "manual_request",
            "source_refs": {"source_record_ref": "activity_occurrence:test"},
            "payload": {
                "app_token": "app_secret_value",
                "summary": "do not leak"
            }
        })))
        .expect_err("sensitive payload should be rejected");

        assert!(err
            .to_string()
            .contains("payload contains disallowed sensitive"));
    }

    #[test]
    fn idempotency_key_defaults_to_dedupe_key() {
        let report = create_work_item_dry_run(request(json!({
            "requester_agent": "xiaoman",
            "target_agent": "wenyuange",
            "capability_key": "wenyuange.retrieve_evidence",
            "work_item_type": "evidence_request",
            "brief_summary": "查找这次活动过往复盘资料",
            "source_type": "manual_request",
            "source_refs": {"source_record_ref": "activity_occurrence:test"},
            "payload": {"question": "有哪些类似活动复盘"}
        })))
        .expect("evidence request should validate");

        assert!(report.idempotency_key.starts_with("ops:"));
        assert_eq!(report.idempotency_key, report.dedupe_key);
    }

    #[test]
    fn rejects_unknown_source_type() {
        let err = create_work_item_dry_run(request(json!({
            "requester_agent": "xiaoman",
            "target_agent": "huabaosi",
            "capability_key": "huabaosi.create_visual_asset",
            "work_item_type": "visual_asset_request",
            "brief_summary": "从日报生成活动海报",
            "source_type": "daily_digest",
            "source_refs": {"source_record_ref": "daily_digests.markdown"}
        })))
        .expect_err("unknown source type should be rejected");

        assert!(err
            .to_string()
            .contains("source_type is not allowed for operations work items"));
    }

    #[test]
    fn rejects_manual_source_without_record_ref() {
        let err = plan_request(
            serde_json::from_value(json!({
                "actor_agent": "xiaoman",
                "request_text": "请根据周末活动生成一张运营海报",
                "source_type": "manual_request",
                "source_refs": {}
            }))
            .expect("plan input should deserialize"),
        )
        .expect_err("manual source should require source_record_ref");

        assert!(err
            .to_string()
            .contains("source_refs.source_record_ref is required"));
    }

    #[test]
    fn rejects_event_signal_source_without_event_ref() {
        let err = create_work_item_dry_run(request(json!({
            "requester_agent": "xiaoman",
            "target_agent": "huabaosi",
            "capability_key": "huabaosi.create_visual_asset",
            "work_item_type": "visual_asset_request",
            "brief_summary": "活动信号触发海报",
            "source_type": "event_signal",
            "source_refs": {}
        })))
        .expect_err("event signal source should require event signal ref");

        assert!(err
            .to_string()
            .contains("event_signal source requires event_signal_id"));
    }

    #[test]
    fn review_decision_dry_run_accepts_approved() {
        let report = record_artifact_review_decision_dry_run(
            serde_json::from_value(json!({
                "artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
                "reviewer_id": "human-owner-1",
                "decision": "approved",
                "reason": "可用于活动宣发"
            }))
            .expect("review request should deserialize"),
            &OperationsPolicy::dry_run(),
        )
        .expect("approved decision should validate");

        assert_eq!(report.action_status, "dry_run_ok");
        assert_eq!(report.review_status, "approved");
        assert!(!report.reason_required);
        assert!(report
            .limitations
            .iter()
            .any(|item| item.contains("does not publish")));
    }

    fn generated_image_approval_context() -> ArtifactApprovalContext {
        let brief_id = Uuid::new_v4();
        let brief_hash = format!("sha256:{}", "b".repeat(64));
        let prompt_hash = format!("sha256:{}", "c".repeat(64));
        ArtifactApprovalContext {
            work_item_id: Uuid::new_v4(),
            artifact_type: GENERATED_IMAGE_ARTIFACT_TYPE.to_string(),
            review_status: "pending".to_string(),
            created_by_agent: "huabaosi".to_string(),
            artifact_uri: Some("https://media.example.test/posters/image.jpg".to_string()),
            content_hash: Some(format!("sha256:{}", "a".repeat(64))),
            source_ids: json!([{
                "approved_brief_artifact_id": brief_id,
                "approved_brief_content_hash": brief_hash,
            }]),
            risk_labels: vec![
                "external_use_review_required".to_string(),
                "generated_media".to_string(),
            ],
            information_class: "internal_ops".to_string(),
            metadata: json!({
                "generated_by": GENERATED_IMAGE_WORKER_ID,
                "provider": "openai-compatible",
                "model": "gpt-image-2",
                "mime_type": "image/jpeg",
                "file_md5": "e2c865db4162bed963bfaa9ef6ac18f0",
                "provider_source_mime_type": "image/png",
                "provider_source_content_hash": format!("sha256:{}", "d".repeat(64)),
                "media_transform": "png_to_jpeg_white_background_q92_v1",
                "jpeg_quality": 92,
                "alpha_background": "#ffffff",
                "width": 1024,
                "height": 1024,
                "byte_size": 4096,
                "approved_brief_artifact_id": brief_id,
                "approved_brief_content_hash": brief_hash,
                "prompt_hash": prompt_hash,
            }),
            work_item_type: GENERATED_IMAGE_WORK_ITEM_TYPE.to_string(),
            capability_key: GENERATED_IMAGE_CAPABILITY_KEY.to_string(),
            work_item_status: "awaiting_review".to_string(),
            work_item_payload: json!({
                "approved_brief_artifact_id": brief_id,
                "approved_brief_content_hash": brief_hash,
                "prompt_hash": prompt_hash,
                "image_specification": "community_poster_1024x1024",
            }),
            creation_event_matches: true,
        }
    }

    #[test]
    fn generated_image_approval_accepts_complete_worker_provenance() {
        validate_generated_image_approval(&generated_image_approval_context(), "approved")
            .expect("complete generated image should be approvable");
    }

    #[test]
    fn generated_image_approval_rejects_missing_creation_audit() {
        let mut context = generated_image_approval_context();
        context.creation_event_matches = false;

        let error = validate_generated_image_approval(&context, "approved")
            .expect_err("creation audit must be required");

        assert!(error.to_string().contains("matching creation audit"));
    }

    #[test]
    fn generated_image_approval_rejects_mismatched_source_brief() {
        let mut context = generated_image_approval_context();
        context.metadata["approved_brief_artifact_id"] = json!(Uuid::new_v4());

        let error = validate_generated_image_approval(&context, "approved")
            .expect_err("source brief must match the image request");

        assert!(error.to_string().contains("source metadata does not match"));
    }

    #[test]
    fn generated_image_approval_rejects_unreviewed_transform_metadata() {
        let mut context = generated_image_approval_context();
        context.metadata["media_transform"] = json!("different-transform");

        let error = validate_generated_image_approval(&context, "approved")
            .expect_err("transform identity must be fixed");

        assert!(error.to_string().contains("canonical worker metadata"));
    }

    #[test]
    fn generated_image_approval_rejects_invalid_source_hash() {
        let mut context = generated_image_approval_context();
        context.metadata["provider_source_content_hash"] = json!("sha256:invalid");

        let error = validate_generated_image_approval(&context, "approved")
            .expect_err("source PNG hash must be canonical");

        assert!(error.to_string().contains("provider source content hash"));
    }

    #[test]
    fn generated_image_approval_rejects_noncanonical_media_identity() {
        let mut context = generated_image_approval_context();
        context.artifact_uri = Some("https://media.example.test/image.jpg?token=value".to_string());
        let uri_error = validate_generated_image_approval(&context, "approved")
            .expect_err("media URL query must be rejected");
        assert!(uri_error.to_string().contains("stable HTTPS media URL"));

        let mut context = generated_image_approval_context();
        context.artifact_uri = Some("https://media.example.test/posters%2Fimage.jpg".to_string());
        let encoded_separator_error = validate_generated_image_approval(&context, "approved")
            .expect_err("encoded path separators must be rejected");
        assert!(encoded_separator_error
            .to_string()
            .contains("ambiguous path separators"));

        let mut context = generated_image_approval_context();
        context.content_hash = Some("sha256:not-a-real-hash".to_string());
        let hash_error = validate_generated_image_approval(&context, "approved")
            .expect_err("noncanonical hash must be rejected");
        assert!(hash_error.to_string().contains("canonical sha256"));

        let mut context = generated_image_approval_context();
        context.metadata["file_md5"] = json!("not-a-canonical-md5");
        let md5_error = validate_generated_image_approval(&context, "approved")
            .expect_err("noncanonical QiWe file MD5 must be rejected");
        assert!(md5_error.to_string().contains("canonical md5"));

        let mut context = generated_image_approval_context();
        context.artifact_uri = Some("https://media.example.test/image.png".to_string());
        let mime_error = validate_generated_image_approval(&context, "approved")
            .expect_err("non-JPEG artifact path must be rejected");
        assert!(mime_error.to_string().contains("JPEG object"));
    }

    #[test]
    fn generated_image_rejection_does_not_require_complete_provenance() {
        let mut context = generated_image_approval_context();
        context.artifact_uri = None;
        context.creation_event_matches = false;

        validate_generated_image_approval(&context, "rejected")
            .expect("humans must be able to reject malformed artifacts");
    }

    #[test]
    fn review_decision_rejects_rejected_without_reason() {
        let err = record_artifact_review_decision_dry_run(
            serde_json::from_value(json!({
                "artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
                "reviewer_id": "human-owner-1",
                "decision": "rejected"
            }))
            .expect("review request should deserialize"),
            &OperationsPolicy::dry_run(),
        )
        .expect_err("rejected decision should require a reason");

        assert!(err.to_string().contains("reason is required"));
    }

    #[test]
    fn review_decision_rejects_sensitive_reason() {
        let err = record_artifact_review_decision_dry_run(
            serde_json::from_value(json!({
                "artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
                "reviewer_id": "human-owner-1",
                "decision": "changes_requested",
                "reason": "contains app_token"
            }))
            .expect("review request should deserialize"),
            &OperationsPolicy::dry_run(),
        )
        .expect_err("sensitive review reason should be rejected");

        assert!(err
            .to_string()
            .contains("review payload contains disallowed sensitive"));
    }

    #[test]
    fn review_decision_rejects_non_allowlisted_reviewer_when_configured() {
        let mut policy = OperationsPolicy::dry_run();
        policy.allowed_reviewer_ids = vec!["lead-reviewer".to_string()];

        let err = record_artifact_review_decision_dry_run(
            serde_json::from_value(json!({
                "artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
                "reviewer_id": "human-owner-1",
                "decision": "approved",
                "reason": "可用于活动宣发"
            }))
            .expect("review request should deserialize"),
            &policy,
        )
        .expect_err("reviewer should be rejected when allowlist is configured");

        assert!(err.to_string().contains("reviewer_id is not allowed"));
    }

    #[test]
    fn review_decision_rejects_bot_reviewer_identity() {
        let err = record_artifact_review_decision_dry_run(
            serde_json::from_value(json!({
                "artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
                "reviewer_id": "cli_xiaoman_app",
                "decision": "approved",
                "reason": "可用于活动宣发"
            }))
            .expect("review request should deserialize"),
            &OperationsPolicy::dry_run(),
        )
        .expect_err("bot/app reviewer should be rejected");

        assert!(err.to_string().contains("reviewer_id must be a human"));
    }

    #[test]
    fn group_message_confirmation_confirmed_moves_to_queued() {
        let report = record_group_message_confirmation_dry_run(
            serde_json::from_value(json!({
                "work_item_id": "12fd56fa-51a7-4637-829c-6bf77f35b3fb",
                "confirmer_id": "human-owner-1",
                "decision": "confirmed",
                "reason": "确认发送窗口和内容"
            }))
            .expect("confirmation request should deserialize"),
            &OperationsPolicy::dry_run(),
        )
        .expect("confirmation should validate");

        assert_eq!(report.action_status, "dry_run_ok");
        assert_eq!(report.current_status, "queued");
        assert_eq!(report.decision, "confirmed");
        assert!(!report.send_executed);
    }

    #[test]
    fn group_message_confirmation_cancel_requires_reason() {
        let err = record_group_message_confirmation_dry_run(
            serde_json::from_value(json!({
                "work_item_id": "12fd56fa-51a7-4637-829c-6bf77f35b3fb",
                "confirmer_id": "human-owner-1",
                "decision": "cancelled"
            }))
            .expect("confirmation request should deserialize"),
            &OperationsPolicy::dry_run(),
        )
        .expect_err("cancelling should require a reason");

        assert!(err
            .to_string()
            .contains("reason is required when cancelling"));
    }

    #[test]
    fn group_message_confirmation_rejects_sensitive_reason() {
        let err = record_group_message_confirmation_dry_run(
            serde_json::from_value(json!({
                "work_item_id": "12fd56fa-51a7-4637-829c-6bf77f35b3fb",
                "confirmer_id": "human-owner-1",
                "decision": "confirmed",
                "reason": "contains app_token"
            }))
            .expect("confirmation request should deserialize"),
            &OperationsPolicy::dry_run(),
        )
        .expect_err("sensitive reason should be rejected");

        assert!(err
            .to_string()
            .contains("confirmation payload contains disallowed sensitive"));
    }

    #[test]
    fn group_message_confirmation_rejects_non_allowlisted_confirmer_when_configured() {
        let mut policy = OperationsPolicy::dry_run();
        policy.allowed_confirmer_ids = vec!["ops-lead".to_string()];

        let err = record_group_message_confirmation_dry_run(
            serde_json::from_value(json!({
                "work_item_id": "12fd56fa-51a7-4637-829c-6bf77f35b3fb",
                "confirmer_id": "human-owner-1",
                "decision": "confirmed",
                "reason": "确认发送窗口和内容"
            }))
            .expect("confirmation request should deserialize"),
            &policy,
        )
        .expect_err("confirmer should be rejected when allowlist is configured");

        assert!(err.to_string().contains("confirmer_id is not allowed"));
    }

    #[test]
    fn group_message_confirmation_rejects_bot_confirmer_identity() {
        let err = record_group_message_confirmation_dry_run(
            serde_json::from_value(json!({
                "work_item_id": "12fd56fa-51a7-4637-829c-6bf77f35b3fb",
                "confirmer_id": "cli_erhua_app",
                "decision": "confirmed",
                "reason": "确认发送窗口和内容"
            }))
            .expect("confirmation request should deserialize"),
            &OperationsPolicy::dry_run(),
        )
        .expect_err("bot/app confirmer should be rejected");

        assert!(err.to_string().contains("confirmer_id must be a human"));
    }

    #[test]
    fn group_message_confirmation_requires_awaiting_publish_status() {
        let err = validate_group_message_confirm_work_item(
            "queued",
            "group_message_request",
            "erhua.send_group_message",
            "human_final_confirmation",
        )
        .expect_err("queued item should not be confirmed again");

        assert!(err
            .to_string()
            .contains("must be awaiting_publish before final confirmation"));
    }

    #[test]
    fn workbench_event_dry_run_records_comment_without_state_mutation() {
        let report = record_workbench_event_dry_run(
            serde_json::from_value(json!({
                "work_item_id": "12fd56fa-51a7-4637-829c-6bf77f35b3fb",
                "provider": "feishu_task",
                "external_id": "task_fixture_1",
                "external_event_id": "comment_fixture_1",
                "event_type": "comment_added",
                "actor_id": "human-owner-1",
                "comment_text": "请把标题再收紧一点"
            }))
            .expect("workbench event should deserialize"),
        )
        .expect("comment event should validate");

        assert_eq!(report.action_status, "dry_run_ok");
        assert!(!report.mutates_work_item_state);
        assert_eq!(report.recommended_command, None);
    }

    #[test]
    fn workbench_event_review_request_recommends_dedicated_review_command() {
        let report = record_workbench_event_dry_run(
            serde_json::from_value(json!({
                "work_item_id": "12fd56fa-51a7-4637-829c-6bf77f35b3fb",
                "artifact_id": "02dd5f47-81f8-4b8c-898d-b4c926fcf9b5",
                "provider": "feishu_task",
                "external_id": "task_fixture_1",
                "external_event_id": "review_fixture_1",
                "event_type": "review_decision_requested",
                "actor_id": "human-owner-1",
                "review_decision": "approved",
                "comment_text": "审核通过"
            }))
            .expect("workbench event should deserialize"),
        )
        .expect("review event should validate");

        assert_eq!(
            report.recommended_command.as_deref(),
            Some("operations-artifact-review-decision")
        );
        assert!(!report.mutates_work_item_state);
    }

    #[test]
    fn workbench_event_final_confirmation_recommends_dedicated_confirm_command() {
        let report = record_workbench_event_dry_run(
            serde_json::from_value(json!({
                "work_item_id": "12fd56fa-51a7-4637-829c-6bf77f35b3fb",
                "provider": "feishu_task",
                "external_id": "task_fixture_1",
                "external_event_id": "confirm_fixture_1",
                "event_type": "final_confirmation_requested",
                "actor_id": "human-owner-1",
                "confirmation_decision": "confirmed",
                "comment_text": "确认发送"
            }))
            .expect("workbench event should deserialize"),
        )
        .expect("confirmation event should validate");

        assert_eq!(
            report.recommended_command.as_deref(),
            Some("operations-group-message-confirm")
        );
        assert!(!report.mutates_work_item_state);
    }

    #[test]
    fn workbench_event_status_change_recommends_status_change_command() {
        let report = record_workbench_event_dry_run(
            serde_json::from_value(json!({
                "work_item_id": "12fd56fa-51a7-4637-829c-6bf77f35b3fb",
                "provider": "feishu_task",
                "external_id": "task_fixture_1",
                "external_event_id": "status_fixture_1",
                "event_type": "status_change_requested",
                "actor_id": "human-owner-1",
                "requested_status": "cancelled",
                "comment_text": "活动取消，停止继续制作"
            }))
            .expect("workbench event should deserialize"),
        )
        .expect("status change event should validate");

        assert_eq!(
            report.recommended_command.as_deref(),
            Some("operations-workbench-status-change")
        );
        assert!(!report.mutates_work_item_state);
    }

    #[test]
    fn workbench_event_owner_change_recommends_owner_change_command() {
        let report = record_workbench_event_dry_run(
            serde_json::from_value(json!({
                "work_item_id": "12fd56fa-51a7-4637-829c-6bf77f35b3fb",
                "provider": "feishu_task",
                "external_id": "task_fixture_1",
                "external_event_id": "owner_fixture_1",
                "event_type": "owner_changed",
                "actor_id": "human-owner-1",
                "comment_text": "改由运营 A 跟进",
                "metadata": {"new_human_owner": "ops-owner-a"}
            }))
            .expect("workbench event should deserialize"),
        )
        .expect("owner change event should validate");

        assert_eq!(
            report.recommended_command.as_deref(),
            Some("operations-workbench-owner-change")
        );
        assert!(!report.mutates_work_item_state);
    }

    #[test]
    fn workbench_event_attachment_recommends_attachment_command() {
        let report = record_workbench_event_dry_run(
            serde_json::from_value(json!({
                "work_item_id": "12fd56fa-51a7-4637-829c-6bf77f35b3fb",
                "provider": "feishu_task",
                "external_id": "task_fixture_1",
                "external_event_id": "attachment_fixture_1",
                "event_type": "attachment_added",
                "actor_id": "human-owner-1",
                "comment_text": "补充活动现场图",
                "metadata": {
                    "attachment_title": "活动现场参考图",
                    "attachment_summary": "人工补充的视觉参考素材，待审核后使用。",
                    "attachment_uri": "https://example.com/workbench/attachment.png"
                }
            }))
            .expect("workbench event should deserialize"),
        )
        .expect("attachment event should validate");

        assert_eq!(
            report.recommended_command.as_deref(),
            Some("operations-workbench-attachment-add")
        );
        assert!(!report.mutates_work_item_state);
    }

    #[test]
    fn workbench_event_rejects_sensitive_comment() {
        let err = record_workbench_event_dry_run(
            serde_json::from_value(json!({
                "work_item_id": "12fd56fa-51a7-4637-829c-6bf77f35b3fb",
                "provider": "feishu_task",
                "external_id": "task_fixture_1",
                "event_type": "comment_added",
                "actor_id": "human-owner-1",
                "comment_text": "这里包含 app_token"
            }))
            .expect("workbench event should deserialize"),
        )
        .expect_err("sensitive comment should be rejected");

        assert!(err
            .to_string()
            .contains("workbench event contains disallowed sensitive"));
    }

    fn recorded_workbench_event(event_type: &str) -> RecordedWorkbenchEvent {
        RecordedWorkbenchEvent {
            id: Uuid::new_v4(),
            work_item_id: Uuid::new_v4(),
            artifact_id: Some(Uuid::new_v4()),
            actor_id: "human-owner-1".to_string(),
            provider: "feishu_task".to_string(),
            external_id: "task_fixture_1".to_string(),
            external_event_id: "event_fixture_1".to_string(),
            workbench_event_type: event_type.to_string(),
            comment_text: "审核通过".to_string(),
            requested_status: String::new(),
            review_decision: "approved".to_string(),
            confirmation_decision: "confirmed".to_string(),
            metadata: json!({}),
        }
    }

    #[test]
    fn processable_workbench_review_event_maps_to_review_command() {
        let event = recorded_workbench_event("review_decision_requested");

        validate_processable_workbench_event(&event).expect("event should validate");
        assert_eq!(
            command_for_recorded_workbench_event(&event)
                .expect("command should exist")
                .as_str(),
            "operations-artifact-review-decision"
        );
    }

    #[test]
    fn processable_workbench_final_confirmation_maps_to_confirm_command() {
        let mut event = recorded_workbench_event("final_confirmation_requested");
        event.artifact_id = None;

        validate_processable_workbench_event(&event).expect("event should validate");
        assert_eq!(
            command_for_recorded_workbench_event(&event)
                .expect("command should exist")
                .as_str(),
            "operations-group-message-confirm"
        );
    }

    #[test]
    fn processable_workbench_status_change_maps_to_status_change_command() {
        let mut event = recorded_workbench_event("status_change_requested");
        event.artifact_id = None;
        event.requested_status = "cancelled".to_string();
        event.comment_text = "活动取消，停止继续执行".to_string();

        validate_processable_workbench_event(&event).expect("event should validate");
        assert_eq!(
            command_for_recorded_workbench_event(&event)
                .expect("command should exist")
                .as_str(),
            "operations-workbench-status-change"
        );
    }

    #[test]
    fn processable_workbench_status_change_rejects_completion_request() {
        let mut event = recorded_workbench_event("status_change_requested");
        event.artifact_id = None;
        event.requested_status = "completed".to_string();
        event.comment_text = "直接完成".to_string();

        let err = validate_processable_workbench_event(&event)
            .expect_err("completed status should not be workbench-processable");

        assert!(err
            .to_string()
            .contains("can only request cancelled status"));
    }

    #[test]
    fn workbench_status_transition_allows_non_terminal_cancellation() {
        validate_workbench_status_transition("queued", "cancelled")
            .expect("queued work item can be cancelled from workbench");
        validate_workbench_status_transition("awaiting_review", "cancelled")
            .expect("awaiting review work item can be cancelled from workbench");
    }

    #[test]
    fn workbench_status_transition_rejects_terminal_or_non_cancel_status() {
        let err = validate_workbench_status_transition("completed", "cancelled")
            .expect_err("terminal work item should not change");
        assert!(err.to_string().contains("terminal work items"));

        let err = validate_workbench_status_transition("awaiting_review", "completed")
            .expect_err("workbench cannot mark completion");
        assert!(err.to_string().contains("can only cancel work items"));
    }

    #[test]
    fn processable_workbench_owner_change_maps_to_owner_change_command() {
        let mut event = recorded_workbench_event("owner_changed");
        event.artifact_id = None;
        event.metadata = json!({"new_human_owner": "ops-owner-a"});

        validate_processable_workbench_event(&event).expect("event should validate");
        assert_eq!(
            command_for_recorded_workbench_event(&event)
                .expect("command should exist")
                .as_str(),
            "operations-workbench-owner-change"
        );
        assert_eq!(
            workbench_event_new_human_owner(&event).expect("owner should parse"),
            "ops-owner-a"
        );
    }

    #[test]
    fn processable_workbench_owner_change_requires_new_owner() {
        let mut event = recorded_workbench_event("owner_changed");
        event.artifact_id = None;
        event.metadata = json!({});

        let err = validate_processable_workbench_event(&event)
            .expect_err("owner change should require new_human_owner");

        assert!(err
            .to_string()
            .contains("metadata.new_human_owner is required"));
    }

    #[test]
    fn processable_workbench_owner_change_rejects_bot_owner() {
        let mut event = recorded_workbench_event("owner_changed");
        event.artifact_id = None;
        event.metadata = json!({"new_human_owner": "cli_huabaosi_app"});

        let err = validate_processable_workbench_event(&event)
            .expect_err("bot/app owner should not be workbench-processable");

        assert!(err
            .to_string()
            .contains("metadata.new_human_owner must be a human"));
    }

    #[test]
    fn owner_policy_rejects_non_allowlisted_owner_when_configured() {
        let mut policy = OperationsPolicy::dry_run();
        policy.allowed_owner_ids = vec!["ops-owner-a".to_string()];

        assert!(policy.owner_allowed("ops-owner-a"));
        assert!(!policy.owner_allowed("ops-owner-b"));
    }

    #[test]
    fn readiness_rejects_bot_ids_in_human_allowlists() {
        let cli = Cli::try_parse_from([
            "test",
            "--database-url",
            "postgres://example.invalid/qintopia",
            "--operations-allowed-group-aliases",
            "ops_test_group",
            "--operations-allowed-reviewer-ids",
            "cli_xiaoman_app",
            "--operations-allowed-confirmer-ids",
            "human-confirmer",
            "--operations-allowed-owner-ids",
            "human-owner",
            "--operations-allowed-attachment-hosts",
            "example.com",
            "operations-readiness-check",
        ])
        .expect("cli should parse");

        let report = readiness_report(&cli, "production", false)
            .expect("readiness report should be generated");

        assert!(!report.success);
        assert!(report
            .missing_required
            .contains(&"allowed_reviewers".to_string()));
        assert!(report
            .checks
            .iter()
            .any(|check| check.key == "allowed_reviewers" && check.status == "missing"));
    }

    #[test]
    fn processable_workbench_attachment_maps_to_attachment_command() {
        let mut event = recorded_workbench_event("attachment_added");
        event.artifact_id = None;
        event.comment_text = "补充参考图".to_string();
        event.metadata = json!({
            "attachment_title": "活动参考图",
            "attachment_summary": "人工补充的参考素材",
            "attachment_uri": "https://example.com/workbench/reference.png",
            "attachment_text": "仅供内部审核参考"
        });

        validate_processable_workbench_event(&event).expect("event should validate");
        assert_eq!(
            command_for_recorded_workbench_event(&event)
                .expect("command should exist")
                .as_str(),
            "operations-workbench-attachment-add"
        );
        let attachment = workbench_event_attachment(&event).expect("attachment should parse");
        assert_eq!(attachment.title, "活动参考图");
        assert_eq!(
            attachment.uri,
            "https://example.com/workbench/reference.png"
        );
    }

    #[test]
    fn processable_workbench_attachment_requires_https_uri() {
        let mut event = recorded_workbench_event("attachment_added");
        event.artifact_id = None;
        event.metadata = json!({
            "attachment_title": "活动参考图",
            "attachment_uri": "http://example.com/workbench/reference.png"
        });

        let err = validate_processable_workbench_event(&event)
            .expect_err("attachment uri should require https");

        assert!(err
            .to_string()
            .contains("metadata.attachment_uri must be an https URL"));
    }

    #[test]
    fn attachment_uri_host_normalizes_https_host() {
        assert_eq!(
            attachment_uri_host("https://Example.COM./workbench/reference.png")
                .expect("host should parse"),
            "example.com"
        );
    }

    #[test]
    fn attachment_host_policy_rejects_non_allowlisted_host_when_configured() {
        let mut policy = OperationsPolicy::dry_run();
        policy.allowed_attachment_hosts = vec!["assets.example.com".to_string()];

        assert!(policy.attachment_host_allowed("assets.example.com"));
        assert!(policy.attachment_host_allowed("ASSETS.EXAMPLE.COM."));
        assert!(!policy.attachment_host_allowed("example.com"));
    }

    #[test]
    fn processable_workbench_event_rejects_comment_only_event() {
        let event = recorded_workbench_event("comment_added");
        let err = validate_processable_workbench_event(&event)
            .expect_err("comment events are audit-only");

        assert!(err.to_string().contains("not processable"));
    }

    fn status_row(
        work_item_type: &str,
        status: &str,
        pending_artifact_count: i64,
        send_ready_event_count: i64,
        parent_work_item_id: Option<Uuid>,
    ) -> WorkItemStatusRow {
        status_row_at_depth(
            work_item_type,
            status,
            pending_artifact_count,
            send_ready_event_count,
            parent_work_item_id,
            i32::from(parent_work_item_id.is_some()),
        )
    }

    fn status_row_at_depth(
        work_item_type: &str,
        status: &str,
        pending_artifact_count: i64,
        send_ready_event_count: i64,
        parent_work_item_id: Option<Uuid>,
        depth: i32,
    ) -> WorkItemStatusRow {
        WorkItemStatusRow {
            node: WorkItemStatusNode {
                work_item_id: Uuid::new_v4(),
                parent_work_item_id,
                depth,
                work_item_type: work_item_type.to_string(),
                status: status.to_string(),
                requester_agent: "xiaoman".to_string(),
                target_agent: "huabaosi".to_string(),
                capability_key: "huabaosi.create_visual_asset".to_string(),
                risk_level: "medium".to_string(),
                review_policy: "before_external_use".to_string(),
                artifact_count: pending_artifact_count,
                pending_artifact_count,
                approved_artifact_count: 0,
                latest_event_type: None,
                latest_event_at: None,
                blocking_reason: blocking_reason_for(
                    status,
                    work_item_type,
                    pending_artifact_count,
                    send_ready_event_count,
                ),
            },
        }
    }

    fn status_tree(children: Vec<WorkItemStatusNode>) -> WorkItemStatusTreeReport {
        let descendants = children.clone();
        WorkItemStatusTreeReport {
            success: true,
            queried_work_item_id: Uuid::new_v4(),
            root_work_item_id: Uuid::new_v4(),
            root: status_row("activity_promotion_request", "processing", 0, 0, None).node,
            child_count: children.len(),
            children,
            descendant_count: descendants.len(),
            descendants,
            current_blocking_point: None,
            limitations: vec![],
            guardrails: vec![],
        }
    }

    #[test]
    fn status_tree_reports_artifact_review_as_blocking_point() {
        let parent_id = Uuid::new_v4();
        let rows = vec![
            status_row("activity_promotion_request", "queued", 0, 0, None),
            status_row(
                "visual_asset_request",
                "awaiting_review",
                1,
                0,
                Some(parent_id),
            ),
        ];

        assert_eq!(
            current_blocking_point(&rows).as_deref(),
            Some("visual_asset_request:waiting_for_artifact_review")
        );
        assert_eq!(
            rows[1].node.blocking_reason.as_deref(),
            Some("waiting_for_artifact_review")
        );
    }

    #[test]
    fn status_tree_reports_send_ready_waiting_for_adapter() {
        let rows = vec![status_row("group_message_request", "queued", 0, 1, None)];

        assert_eq!(
            current_blocking_point(&rows).as_deref(),
            Some("group_message_request:send_ready_waiting_for_production_send_adapter")
        );
    }

    #[test]
    fn workflow_aggregate_status_completes_when_all_children_complete() {
        let tree = status_tree(vec![
            status_row("evidence_request", "completed", 0, 0, Some(Uuid::new_v4())).node,
            status_row(
                "visual_asset_request",
                "completed",
                0,
                0,
                Some(Uuid::new_v4()),
            )
            .node,
        ]);

        assert_eq!(workflow_aggregate_status(&tree), "completed");
    }

    #[test]
    fn workflow_aggregate_status_prioritizes_failed_child() {
        let tree = status_tree(vec![
            status_row("evidence_request", "completed", 0, 0, Some(Uuid::new_v4())).node,
            status_row("visual_asset_request", "failed", 0, 0, Some(Uuid::new_v4())).node,
        ]);

        assert_eq!(workflow_aggregate_status(&tree), "failed");
    }

    #[test]
    fn workflow_aggregate_status_includes_failed_nested_descendant() {
        let root_id = Uuid::new_v4();
        let visual =
            status_row_at_depth("visual_asset_request", "completed", 0, 0, Some(root_id), 1).node;
        let image = status_row_at_depth(
            "image_generation_request",
            "failed",
            0,
            0,
            Some(visual.work_item_id),
            2,
        )
        .node;
        let mut tree = status_tree(vec![visual.clone()]);
        tree.descendants = vec![visual, image];
        tree.descendant_count = tree.descendants.len();

        assert_eq!(workflow_aggregate_status(&tree), "failed");
    }

    #[test]
    fn workflow_child_status_refs_keep_safe_summary_fields() {
        let tree = status_tree(vec![
            status_row(
                "group_message_request",
                "queued",
                0,
                1,
                Some(Uuid::new_v4()),
            )
            .node,
        ]);

        let refs = workflow_child_status_refs(&tree);

        assert_eq!(refs.len(), 1);
        assert_eq!(refs[0].depth, 1);
        assert_eq!(refs[0].work_item_type, "group_message_request");
        assert_eq!(
            refs[0].blocking_reason.as_deref(),
            Some("send_ready_waiting_for_production_send_adapter")
        );
    }

    #[test]
    fn workflow_descendant_status_refs_preserve_nested_parent_and_depth() {
        let root_id = Uuid::new_v4();
        let visual =
            status_row_at_depth("visual_asset_request", "completed", 0, 0, Some(root_id), 1).node;
        let image = status_row_at_depth(
            "image_generation_request",
            "awaiting_review",
            1,
            0,
            Some(visual.work_item_id),
            2,
        )
        .node;
        let mut tree = status_tree(vec![visual.clone()]);
        tree.descendants = vec![visual.clone(), image.clone()];
        tree.descendant_count = tree.descendants.len();

        let refs = workflow_descendant_status_refs(&tree);

        assert_eq!(refs.len(), 2);
        assert_eq!(refs[1].work_item_id, image.work_item_id);
        assert_eq!(refs[1].parent_work_item_id, Some(visual.work_item_id));
        assert_eq!(refs[1].depth, 2);
    }

    #[test]
    fn workflow_sync_worker_report_explains_empty_queue() {
        let report = workflow_sync_worker_report(false, None, None, None, "no_syncable_workflow");

        assert!(report.dry_run);
        assert_eq!(report.worker, "workflow-sync-worker");
        assert_eq!(report.action_status, "no_syncable_workflow");
        assert!(report.sync_report.is_none());
    }
}
