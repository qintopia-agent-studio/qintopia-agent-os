use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    path::PathBuf,
    sync::Arc,
    time::Duration as StdDuration,
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::{FixedOffset, TimeZone};
use rustls::{ClientConfig, ClientConnection, OwnedTrustAnchor, RootCertStore, ServerName, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};

use crate::{
    config::Cli,
    operations::{self, WorkItemCreateReport, WorkItemCreateRequest},
};

const ACTOR_AGENT: &str = "xiaoman";
const READ_ONLY_OPERATIONS: &[&str] = &["record-get", "list-by-date", "material-summary"];
const WRITE_OPERATIONS: &[&str] = &["status-update", "gap-update", "handoff-create"];
const TABLE_ROLES: &[&str] = &["activity_plan", "activity_occurrence"];
const FEISHU_BASE_API: &str = "https://open.feishu.cn/open-apis/bitable/v1/apps";
const FEISHU_AUTH_API: &str =
    "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";
const HANDOFF_TYPES: &[&str] = &[
    "visual_asset_request",
    "ops_followup",
    "member_notice",
    "human_confirmation",
    "activity_recap",
];
const HANDOFF_TARGETS: &[&str] = &["huabaosi", "silaoshi", "erhua", "default"];
const FEISHU_READ_LIMITATION: &str = "Feishu Base read is allowlisted and read-only; write parity, audit, and webhook shadow validation are still required before removing the legacy raw Base read path";

#[derive(Debug, Clone)]
pub struct ActivityRuntimeConfig {
    pub fixture_path: Option<PathBuf>,
    pub feishu_base: Option<ActivityFeishuBaseConfig>,
}

#[derive(Debug, Clone)]
pub struct ActivityFeishuBaseConfig {
    base_token: String,
    plan_table_id: String,
    occurrence_table_id: String,
    profile_env_path: String,
}

#[derive(Debug, Deserialize)]
struct ActivityPayload {
    actor_agent: String,
    operation: String,
    #[serde(default)]
    record_id: String,
    #[serde(default)]
    source_record_id: String,
    #[serde(default)]
    date: String,
    #[serde(default)]
    table_role: String,
    #[serde(default)]
    status: String,
    #[serde(default)]
    gap_summary: String,
    #[serde(default)]
    handoff_type: String,
    #[serde(default)]
    target_agent: String,
    #[serde(default)]
    brief_summary: String,
}

#[derive(Debug, Clone)]
struct ActivityRecord {
    record_id: String,
    table_role: String,
    title: String,
    activity_date: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    location: Option<String>,
    status: Option<String>,
    promotion_status: Option<String>,
    owner_name: Option<String>,
    initiator_name: Option<String>,
    material_summary: Option<String>,
    gap_summary: Option<String>,
    notes: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
struct ActivityRecordView {
    table_role: String,
    record_ref: String,
    title: String,
    activity_date: Option<String>,
    start_time: Option<String>,
    end_time: Option<String>,
    location: Option<String>,
    status: Option<String>,
    promotion_status: Option<String>,
    owner_name: Option<String>,
    initiator_name: Option<String>,
    material_summary: Option<String>,
    gap_summary: Option<String>,
    notes: Option<String>,
    updated_at: Option<String>,
}

#[derive(Debug, Clone)]
struct FeishuClient {
    tenant_token: String,
    tls_config: Arc<ClientConfig>,
}

#[derive(Debug, Clone, Deserialize)]
struct FeishuProfileEnv {
    app_id: String,
    app_secret: String,
}

#[derive(Debug, Serialize)]
struct ActivityWorkerReport {
    success: bool,
    worker: &'static str,
    operation: String,
    source: String,
    actor_agent: String,
    dry_run: bool,
    apply_requested: bool,
    validation_status: String,
    action_status: String,
    safe_for_chat: bool,
    record_count: usize,
    records: Vec<ActivityRecordView>,
    summaries: Vec<String>,
    operations_work_item: Option<WorkItemCreateReport>,
    limitations: Vec<String>,
    guardrails: Vec<String>,
}

pub async fn run(
    cli: &Cli,
    operation: String,
    payload_json: String,
    apply: bool,
    dry_run: bool,
    fixture_path: Option<PathBuf>,
    use_feishu_base: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let payload: ActivityPayload = serde_json::from_str(&payload_json)?;
    validate(&operation, &payload)?;
    let config = ActivityRuntimeConfig {
        fixture_path,
        feishu_base: if use_feishu_base {
            Some(activity_feishu_base_config(cli)?)
        } else {
            None
        },
    };
    let report = execute_with_config(cli, operation, payload, apply, dry_run, &config).await?;

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn execute_with_config(
    cli: &Cli,
    operation: String,
    payload: ActivityPayload,
    apply: bool,
    dry_run: bool,
    config: &ActivityRuntimeConfig,
) -> Result<ActivityWorkerReport> {
    let operation_is_read = READ_ONLY_OPERATIONS.contains(&operation.as_str());
    let apply_requested = apply && !dry_run;
    let mut report = base_report(operation.clone(), &payload, apply_requested);

    if operation_is_read && apply_requested {
        execute_read_operation(&mut report, &operation, &payload, config)?;
    } else if operation_is_read {
        report.action_status = "dry_run_ok".to_string();
        report.limitations.push(
            "read operation was validated only; use --apply with an approved source for replay"
                .to_string(),
        );
    } else if operation == "handoff-create" {
        execute_handoff_create(cli, &mut report, &payload, apply_requested).await?;
    } else if apply_requested {
        report.action_status = "apply_not_implemented".to_string();
        report.limitations.push(
            "status-update and gap-update writes are intentionally blocked until field allowlists, idempotency, and audit writes are implemented".to_string(),
        );
    } else {
        report.action_status = "dry_run_ok".to_string();
    }

    Ok(report)
}

fn base_report(
    operation: String,
    payload: &ActivityPayload,
    apply_requested: bool,
) -> ActivityWorkerReport {
    ActivityWorkerReport {
        success: true,
        worker: "xiaoman-activity",
        operation,
        source: "validation_only".to_string(),
        actor_agent: payload.actor_agent.clone(),
        dry_run: !apply_requested,
        apply_requested,
        validation_status: "ok".to_string(),
        action_status: "validation_ok".to_string(),
        safe_for_chat: false,
        record_count: 0,
        records: Vec::new(),
        summaries: Vec::new(),
        operations_work_item: None,
        limitations: Vec::new(),
        guardrails: vec![
            "validates wrapper payload before any data access".to_string(),
            "fixture replay is allowed for local acceptance only".to_string(),
            "status-update and gap-update writes remain blocked until field allowlists and audit writes are implemented".to_string(),
            "technical report is for logs, not WeCom users".to_string(),
            "record_ref is hashed; raw Base record ids are not included in records".to_string(),
        ],
    }
}

async fn execute_handoff_create(
    cli: &Cli,
    report: &mut ActivityWorkerReport,
    payload: &ActivityPayload,
    apply_requested: bool,
) -> Result<()> {
    let request = handoff_work_item_request(payload)?;
    let work_item_report = if apply_requested {
        let database_url = cli.database_url_required()?;
        let pool = crate::db::connect(database_url, cli.db_max_connections).await?;
        let policy = operations::OperationsPolicy::from_cli(cli, true);
        operations::create_work_item(&pool, request, true, &policy).await?
    } else {
        operations::create_work_item_dry_run(request)?
    };

    report.action_status = format!("operations_{}", work_item_report.action_status);
    report.source = "agentos_operations_control_plane".to_string();
    report.safe_for_chat = false;
    report.operations_work_item = Some(work_item_report);
    report.guardrails.push(
        "handoff-create creates a capability-governed work item instead of raw prompt handoff"
            .to_string(),
    );
    report
        .guardrails
        .push("Feishu Task board mirroring remains a separate sync worker boundary".to_string());
    Ok(())
}

fn handoff_work_item_request(payload: &ActivityPayload) -> Result<WorkItemCreateRequest> {
    let (capability_key, work_item_type) = match (
        payload.handoff_type.as_str(),
        payload.target_agent.as_str(),
    ) {
        ("visual_asset_request", "huabaosi") => (
            "huabaosi.create_visual_asset".to_string(),
            "visual_asset_request".to_string(),
        ),
        _ => bail!(
            "handoff-create is not mapped to an operations capability for handoff_type={} target_agent={}",
            payload.handoff_type,
            payload.target_agent
        ),
    };

    let table_role = if payload.table_role.trim().is_empty() {
        "activity_occurrence"
    } else {
        payload.table_role.as_str()
    };
    if !TABLE_ROLES.contains(&table_role) {
        bail!("table_role is not allowed");
    }

    let source_record_ref = activity_record_ref(table_role, &payload.source_record_id);
    let source_refs = json!({
        "source_record_ref": source_record_ref.clone(),
        "table_role": table_role,
        "wrapper_operation": payload.operation,
    });
    let payload_value = json!({
        "handoff_type": payload.handoff_type,
        "source_record_ref": source_record_ref.clone(),
        "table_role": table_role,
    });

    Ok(WorkItemCreateRequest {
        requester_agent: payload.actor_agent.clone(),
        target_agent: payload.target_agent.clone(),
        capability_key,
        work_item_type,
        brief_summary: payload.brief_summary.clone(),
        purpose: "activity_operations_handoff".to_string(),
        human_owner: String::new(),
        priority: "normal".to_string(),
        source_type: "xiaoman_activity".to_string(),
        source_refs,
        source_event_signal_id: None,
        payload: payload_value,
        payload_redaction_policy: "summary_only".to_string(),
        idempotency_key: String::new(),
        dedupe_key: String::new(),
        metadata: json!({
            "created_by_wrapper": "xiaoman-activity",
            "source_record_ref": source_record_ref,
        }),
        parent_work_item_id: None,
        approved_artifact_id: None,
    })
}

fn activity_record_ref(table_role: &str, record_id: &str) -> String {
    let digest = Sha256::digest(format!("{table_role}:{record_id}"));
    let mut suffix = String::new();
    for byte in &digest[..6] {
        suffix.push_str(&format!("{byte:02x}"));
    }
    format!("{table_role}:{suffix}")
}

fn execute_read_operation(
    report: &mut ActivityWorkerReport,
    operation: &str,
    payload: &ActivityPayload,
    config: &ActivityRuntimeConfig,
) -> Result<()> {
    let (records, source, limitation) = if let Some(path) = config.fixture_path.as_deref() {
        (
            load_fixture_records(path)?,
            "fixture".to_string(),
            "fixture-backed read proves wrapper shape and filtering; it is not production Base parity by itself".to_string(),
        )
    } else if let Some(base_config) = config.feishu_base.as_ref() {
        (
            load_feishu_records(base_config, &payload.table_role)?,
            "feishu_base_read_only".to_string(),
            FEISHU_READ_LIMITATION.to_string(),
        )
    } else {
        report.action_status = "read_source_not_configured".to_string();
        report.limitations.push(
            "set QINTOPIA_XIAOMAN_ACTIVITY_FIXTURE_PATH/--fixture-path for replay or enable QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE with an allowlisted Base config".to_string(),
        );
        return Ok(());
    };
    report.source = source;
    report.limitations.push(limitation);

    let matched: Vec<ActivityRecord> = match operation {
        "record-get" | "material-summary" => records
            .into_iter()
            .filter(|record| {
                record.table_role == payload.table_role && record.record_id == payload.record_id
            })
            .collect(),
        "list-by-date" => records
            .into_iter()
            .filter(|record| {
                record.table_role == payload.table_role && record.matches_date(&payload.date)
            })
            .collect(),
        _ => unreachable!("read operation allowlist checked above"),
    };

    report.record_count = matched.len();
    report.records = matched.iter().map(ActivityRecord::view).collect();
    report.summaries = matched.iter().map(ActivityRecord::safe_summary).collect();
    report.action_status = if matched.is_empty() {
        "record_not_found"
    } else {
        "read_ok"
    }
    .to_string();
    Ok(())
}

fn load_fixture_records(path: &std::path::Path) -> Result<Vec<ActivityRecord>> {
    let text = fs::read_to_string(path).with_context(|| format!("read {}", path.display()))?;
    let value: Value = serde_json::from_str(&text).context("parse Xiaoman activity fixture")?;
    let items = if let Some(items) = value.as_array() {
        items.clone()
    } else {
        value
            .get("records")
            .and_then(Value::as_array)
            .cloned()
            .context("fixture must be an array or an object with records")?
    };
    items
        .iter()
        .map(ActivityRecord::from_fixture_value)
        .collect()
}

fn load_feishu_records(
    config: &ActivityFeishuBaseConfig,
    table_role: &str,
) -> Result<Vec<ActivityRecord>> {
    let table_id = config.table_id_for_role(table_role)?;
    let client = FeishuClient::from_profile_env(&config.profile_env_path)?;
    client
        .list_records(&config.base_token, table_id, 200)?
        .iter()
        .map(|record| ActivityRecord::from_feishu_value(record, table_role))
        .collect()
}

fn activity_feishu_base_config(cli: &Cli) -> Result<ActivityFeishuBaseConfig> {
    let base_token = cli
        .xiaoman_activity_feishu_base_token
        .clone()
        .context("QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN is required")?;
    let allowed_tokens = cli.xiaoman_activity_allowed_feishu_base_tokens();
    if allowed_tokens.is_empty() || !allowed_tokens.iter().any(|token| token == &base_token) {
        bail!("Xiaoman activity Feishu Base token must be explicitly allowlisted");
    }
    Ok(ActivityFeishuBaseConfig {
        base_token,
        plan_table_id: cli
            .xiaoman_activity_feishu_plan_table_id
            .clone()
            .context("QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_PLAN_TABLE_ID is required")?,
        occurrence_table_id: cli
            .xiaoman_activity_feishu_occurrence_table_id
            .clone()
            .context("QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_OCCURRENCE_TABLE_ID is required")?,
        profile_env_path: cli.xiaoman_activity_feishu_profile_env_path.clone(),
    })
}

impl ActivityFeishuBaseConfig {
    fn table_id_for_role(&self, table_role: &str) -> Result<&str> {
        match table_role {
            "activity_plan" => Ok(&self.plan_table_id),
            "activity_occurrence" => Ok(&self.occurrence_table_id),
            _ => bail!("table_role is not allowed"),
        }
    }
}

impl ActivityRecord {
    fn from_feishu_value(value: &Value, table_role: &str) -> Result<Self> {
        let mut enriched = value.clone();
        if let Some(map) = enriched.as_object_mut() {
            map.entry("table_role".to_string())
                .or_insert_with(|| Value::String(table_role.to_string()));
        }
        let mut record = Self::from_fixture_value(&enriched)?;
        record.table_role = table_role.to_string();
        Ok(record)
    }

    fn from_fixture_value(value: &Value) -> Result<Self> {
        let fields = value.get("fields").unwrap_or(value);
        let record_id = string_at(value, &["record_id", "id"])
            .context("activity fixture record missing record_id")?;
        let table_role = string_at(value, &["table_role"])
            .or_else(|| field_string(fields, &["table_role"]))
            .context("activity fixture record missing table_role")?;
        if !TABLE_ROLES.contains(&table_role.as_str()) {
            bail!("fixture table_role is not allowed");
        }
        let title = field_string(
            fields,
            &[
                "活动名称",
                "活动标题",
                "活动信息",
                "活动内容",
                "标题",
                "name",
                "title",
            ],
        )
        .unwrap_or_else(|| "未命名活动".to_string());

        Ok(Self {
            record_id,
            table_role,
            title,
            activity_date: field_string(
                fields,
                &[
                    "活动日期",
                    "日期",
                    "计划日期",
                    "发生日期",
                    "date",
                    "activity_date",
                ],
            ),
            start_time: field_string(
                fields,
                &[
                    "开始时间",
                    "活动时间",
                    "计划时间",
                    "活动计划时间",
                    "start_time",
                    "startTime",
                ],
            ),
            end_time: field_string(fields, &["结束时间", "end_time", "endTime"]),
            location: field_string(fields, &["地点", "活动地点", "location"]),
            status: field_string(fields, &["小满运营状态", "活动状态", "状态", "status"]),
            promotion_status: field_string(fields, &["宣发判断", "宣发状态", "promotion_status"]),
            owner_name: field_string(fields, &["负责人", "负责同学", "owner", "owner_name"]),
            initiator_name: field_string(fields, &["发起人", "组织者", "initiator"]),
            material_summary: field_string(
                fields,
                &[
                    "素材照片",
                    "活动照片",
                    "素材",
                    "素材情况",
                    "material_summary",
                ],
            ),
            gap_summary: field_string(fields, &["补录缺口", "缺口", "gap_summary"]),
            notes: field_string(fields, &["小满备注", "备注", "notes"]),
            updated_at: field_string(fields, &["更新时间", "updated_at"]),
        })
    }

    fn matches_date(&self, date: &str) -> bool {
        self.activity_date.as_deref() == Some(date)
            || self
                .start_time
                .as_deref()
                .map(|item| item.starts_with(date))
                .unwrap_or(false)
    }

    fn view(&self) -> ActivityRecordView {
        ActivityRecordView {
            table_role: self.table_role.clone(),
            record_ref: self.record_ref(),
            title: self.title.clone(),
            activity_date: self.activity_date.clone(),
            start_time: self.start_time.clone(),
            end_time: self.end_time.clone(),
            location: self.location.clone(),
            status: self.status.clone(),
            promotion_status: self.promotion_status.clone(),
            owner_name: self.owner_name.clone(),
            initiator_name: self.initiator_name.clone(),
            material_summary: self.material_summary.clone(),
            gap_summary: self.gap_summary.clone(),
            notes: self.notes.clone(),
            updated_at: self.updated_at.clone(),
        }
    }

    fn record_ref(&self) -> String {
        let digest = Sha256::digest(format!("{}:{}", self.table_role, self.record_id));
        let mut suffix = String::new();
        for byte in &digest[..6] {
            suffix.push_str(&format!("{byte:02x}"));
        }
        format!("{}:{suffix}", self.table_role)
    }

    fn safe_summary(&self) -> String {
        let date = self
            .activity_date
            .clone()
            .or_else(|| self.start_time.clone())
            .unwrap_or_else(|| "日期未定".to_string());
        let location = self
            .location
            .clone()
            .unwrap_or_else(|| "地点未定".to_string());
        let status = self
            .status
            .clone()
            .unwrap_or_else(|| "状态未定".to_string());
        format!("{}｜{}｜{}｜{}", self.title, date, location, status)
    }
}

fn field_string(fields: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        fields
            .get(*name)
            .and_then(|value| field_cell_as_string(name, value))
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
    })
}

fn string_at(value: &Value, names: &[&str]) -> Option<String> {
    names.iter().find_map(|name| {
        value
            .get(*name)
            .and_then(feishu_cell_as_string)
            .map(|item| item.trim().to_string())
            .filter(|item| !item.is_empty())
    })
}

fn field_cell_as_string(name: &str, value: &Value) -> Option<String> {
    if is_datetime_field(name) {
        if let Some(text) = feishu_timestamp_as_shanghai_datetime(value) {
            return Some(text);
        }
    }
    feishu_cell_as_string(value)
}

fn is_datetime_field(name: &str) -> bool {
    matches!(
        name,
        "开始时间"
            | "活动时间"
            | "计划时间"
            | "活动计划时间"
            | "结束时间"
            | "更新时间"
            | "start_time"
            | "startTime"
            | "end_time"
            | "endTime"
            | "updated_at"
    )
}

fn feishu_timestamp_as_shanghai_datetime(value: &Value) -> Option<String> {
    let millis = match value {
        Value::Number(number) => number.as_i64()?,
        Value::String(text) => text.parse::<i64>().ok()?,
        _ => return None,
    };
    if millis.abs() < 100_000_000_000 {
        return None;
    }
    let timezone = FixedOffset::east_opt(8 * 3600)?;
    let timestamp = timezone.timestamp_millis_opt(millis).single()?;
    Some(timestamp.format("%Y-%m-%d %H:%M").to_string())
}

fn feishu_cell_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Number(number) => Some(number.to_string()),
        Value::Bool(flag) => Some(flag.to_string()),
        Value::Array(items) => {
            let values: Vec<String> = items.iter().filter_map(feishu_cell_as_string).collect();
            if values.is_empty() {
                None
            } else {
                Some(values.join(", "))
            }
        }
        Value::Object(map) => {
            for key in ["text", "name", "value", "url", "link"] {
                if let Some(text) = map.get(key).and_then(feishu_cell_as_string) {
                    return Some(text);
                }
            }
            None
        }
        Value::Null => None,
    }
}

impl FeishuClient {
    fn from_profile_env(path: &str) -> Result<Self> {
        let profile = read_feishu_profile_env(path)?;
        let tls_config = Arc::new(tls_config());
        let response = post_json(
            FEISHU_AUTH_API,
            None,
            &json!({"app_id": profile.app_id, "app_secret": profile.app_secret}),
            tls_config.clone(),
        )
        .context("request Feishu tenant access token")?;
        let parsed: Value =
            serde_json::from_str(&response).context("parse Feishu token response")?;
        if parsed.get("code").and_then(Value::as_i64) != Some(0) {
            bail!("Feishu token response error: {parsed}");
        }
        let tenant_token = parsed
            .get("tenant_access_token")
            .and_then(Value::as_str)
            .context("Feishu response missing tenant_access_token")?
            .to_string();
        Ok(Self {
            tenant_token,
            tls_config,
        })
    }

    fn list_records(
        &self,
        base_token: &str,
        table_id: &str,
        page_size: usize,
    ) -> Result<Vec<Value>> {
        let mut page_token = String::new();
        let mut out = Vec::new();
        loop {
            let mut url = format!(
                "{FEISHU_BASE_API}/{base_token}/tables/{table_id}/records?page_size={}",
                page_size.min(500)
            );
            if !page_token.is_empty() {
                url.push_str("&page_token=");
                url.push_str(&page_token);
            }
            let parsed = self.request_json("GET", &url, None)?;
            let data = parsed.get("data").cloned().unwrap_or_else(|| json!({}));
            if let Some(items) = data.get("items").and_then(Value::as_array) {
                out.extend(items.iter().cloned());
            }
            if data.get("has_more").and_then(Value::as_bool) != Some(true) {
                break;
            }
            page_token = data
                .get("page_token")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string();
            if page_token.is_empty() {
                break;
            }
        }
        Ok(out)
    }

    fn request_json(&self, method: &str, url: &str, body: Option<&Value>) -> Result<Value> {
        let response = request_json(
            method,
            url,
            Some(&self.tenant_token),
            body,
            self.tls_config.clone(),
        )
        .with_context(|| format!("call Feishu API {method} {}", sanitize_feishu_url(url)))?;
        let parsed: Value = serde_json::from_str(&response).context("parse Feishu API response")?;
        if parsed.get("code").and_then(Value::as_i64) != Some(0) {
            bail!("Feishu API response error: {parsed}");
        }
        Ok(parsed)
    }
}

fn read_feishu_profile_env(path: &str) -> Result<FeishuProfileEnv> {
    let text = fs::read_to_string(path).with_context(|| format!("read {path}"))?;
    let mut app_id = None;
    let mut app_secret = None;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let value = value
            .trim()
            .trim_matches('"')
            .trim_matches('\'')
            .to_string();
        match key.trim() {
            "FEISHU_APP_ID" | "LARK_APP_ID" | "APP_ID" => app_id = Some(value),
            "FEISHU_APP_SECRET" | "LARK_APP_SECRET" | "APP_SECRET" => app_secret = Some(value),
            _ => {}
        }
    }
    Ok(FeishuProfileEnv {
        app_id: app_id.context("missing FEISHU_APP_ID in profile env")?,
        app_secret: app_secret.context("missing FEISHU_APP_SECRET in profile env")?,
    })
}

fn request_json(
    method: &str,
    endpoint: &str,
    bearer_token: Option<&str>,
    body: Option<&Value>,
    tls_config: Arc<ClientConfig>,
) -> Result<String> {
    let request_body = body
        .map(serde_json::to_string)
        .transpose()
        .context("serialize Feishu request")?
        .unwrap_or_default();
    let mut headers = vec![
        "Content-Type: application/json".to_string(),
        "Accept: application/json".to_string(),
        "Accept-Encoding: identity".to_string(),
    ];
    if let Some(token) = bearer_token {
        headers.push(format!("Authorization: Bearer {token}"));
    }
    post_raw(method, endpoint, &headers, &request_body, tls_config)
}

fn post_json(
    endpoint: &str,
    bearer_token: Option<&str>,
    request: &Value,
    tls_config: Arc<ClientConfig>,
) -> Result<String> {
    request_json("POST", endpoint, bearer_token, Some(request), tls_config)
}

fn post_raw(
    method: &str,
    endpoint: &str,
    headers: &[String],
    body: &str,
    tls_config: Arc<ClientConfig>,
) -> Result<String> {
    let url = url::Url::parse(endpoint).context("parse Feishu API URL")?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("Feishu API URL is missing host"))?;
    let port = url
        .port_or_known_default()
        .unwrap_or(if url.scheme() == "https" { 443 } else { 80 });
    let mut path = url.path().to_string();
    if path.is_empty() {
        path = "/".to_string();
    }
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }
    let mut request = format!(
        "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\nConnection: close\r\n",
        body.as_bytes().len()
    );
    for header in headers {
        request.push_str(header);
        request.push_str("\r\n");
    }
    request.push_str("\r\n");
    request.push_str(body);

    if url.scheme() != "https" {
        bail!("Feishu API URL must use https");
    }
    let server_name = ServerName::try_from(host).context("validate Feishu API host")?;
    let mut connection =
        ClientConnection::new(tls_config, server_name).context("create TLS connection")?;
    let mut socket = TcpStream::connect((host, port)).context("connect Feishu API")?;
    socket
        .set_read_timeout(Some(StdDuration::from_secs(30)))
        .context("set Feishu read timeout")?;
    socket
        .set_write_timeout(Some(StdDuration::from_secs(30)))
        .context("set Feishu write timeout")?;
    let mut stream = Stream::new(&mut connection, &mut socket);
    stream
        .write_all(request.as_bytes())
        .context("write Feishu request")?;
    stream.flush().context("flush Feishu request")?;
    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .context("read Feishu response")?;
    parse_http_response(&response)
}

fn sanitize_feishu_url(endpoint: &str) -> String {
    match url::Url::parse(endpoint) {
        Ok(parsed) => parsed.path_segments().map_or_else(
            || {
                format!(
                    "{}://{}",
                    parsed.scheme(),
                    parsed.host_str().unwrap_or("unknown")
                )
            },
            |segments| {
                let mut redact_next = false;
                let mut redacted_segments = Vec::new();
                for segment in segments {
                    if redact_next {
                        redacted_segments.push("redacted".to_string());
                        redact_next = false;
                        continue;
                    }
                    redacted_segments.push(segment.to_string());
                    if segment == "apps" || segment == "tables" {
                        redact_next = true;
                    }
                }
                format!(
                    "{}://{}/{}",
                    parsed.scheme(),
                    parsed.host_str().unwrap_or("unknown"),
                    redacted_segments.join("/")
                )
            },
        ),
        Err(_) => "invalid-feishu-url".to_string(),
    }
}

fn parse_http_response(response: &[u8]) -> Result<String> {
    let header_end = find_header_end(response).ok_or_else(|| anyhow!("invalid HTTP response"))?;
    let head = String::from_utf8_lossy(&response[..header_end]);
    let body = &response[header_end + 4..];
    let status_line = head.lines().next().unwrap_or_default();
    if !status_line.contains(" 200 ") {
        bail!(
            "HTTP request failed: {status_line}; body={}",
            String::from_utf8_lossy(body)
        );
    }
    let is_chunked = head
        .lines()
        .any(|line| line.to_ascii_lowercase() == "transfer-encoding: chunked");
    if is_chunked {
        decode_chunked_body(body)
    } else {
        String::from_utf8(body.to_vec()).context("decode HTTP response utf8")
    }
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn decode_chunked_body(body: &[u8]) -> Result<String> {
    let mut index = 0usize;
    let mut decoded = Vec::new();
    while index < body.len() {
        while body.get(index..index + 2) == Some(b"\r\n") {
            index += 2;
        }
        if index >= body.len() {
            break;
        }
        let line_end = find_crlf(body, index).ok_or_else(|| anyhow!("invalid chunked body"))?;
        let size_text = std::str::from_utf8(&body[index..line_end])
            .context("decode chunk size")?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim();
        if size_text.is_empty() {
            break;
        }
        let size = usize::from_str_radix(size_text, 16).context("parse chunk size")?;
        index = line_end + 2;
        if size == 0 {
            break;
        }
        if index + size > body.len() {
            bail!("chunk exceeds body length");
        }
        decoded.extend_from_slice(&body[index..index + size]);
        index += size;
        if body.get(index..index + 2) != Some(b"\r\n") {
            bail!("chunk missing trailing CRLF");
        }
        index += 2;
    }
    String::from_utf8(decoded).context("decode chunked response utf8")
}

fn find_crlf(bytes: &[u8], start: usize) -> Option<usize> {
    bytes
        .get(start..)?
        .windows(2)
        .position(|pair| pair == b"\r\n")
        .map(|offset| start + offset)
}

fn tls_config() -> ClientConfig {
    let mut roots = RootCertStore::empty();
    roots.add_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.iter().map(|anchor| {
        OwnedTrustAnchor::from_subject_spki_name_constraints(
            anchor.subject,
            anchor.spki,
            anchor.name_constraints,
        )
    }));
    ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

fn validate(operation: &str, payload: &ActivityPayload) -> Result<()> {
    if operation != payload.operation {
        bail!("operation mismatch between CLI and payload");
    }
    if !READ_ONLY_OPERATIONS.contains(&operation) && !WRITE_OPERATIONS.contains(&operation) {
        bail!("operation is not allowed");
    }
    if payload.actor_agent != ACTOR_AGENT {
        bail!("actor_agent must be xiaoman");
    }
    if !payload.table_role.is_empty() && !TABLE_ROLES.contains(&payload.table_role.as_str()) {
        bail!("table_role is not allowed");
    }
    match operation {
        "record-get" => require_fields(&[
            ("record_id", &payload.record_id),
            ("table_role", &payload.table_role),
        ])?,
        "list-by-date" => {
            require_fields(&[("date", &payload.date), ("table_role", &payload.table_role)])?
        }
        "status-update" => require_fields(&[
            ("record_id", &payload.record_id),
            ("table_role", &payload.table_role),
            ("status", &payload.status),
        ])?,
        "gap-update" => require_fields(&[
            ("record_id", &payload.record_id),
            ("table_role", &payload.table_role),
            ("gap_summary", &payload.gap_summary),
        ])?,
        "handoff-create" => {
            require_fields(&[
                ("source_record_id", &payload.source_record_id),
                ("handoff_type", &payload.handoff_type),
                ("target_agent", &payload.target_agent),
                ("brief_summary", &payload.brief_summary),
            ])?;
            if !HANDOFF_TYPES.contains(&payload.handoff_type.as_str()) {
                bail!("handoff_type is not allowed");
            }
            if !HANDOFF_TARGETS.contains(&payload.target_agent.as_str()) {
                bail!("target_agent is not allowed");
            }
        }
        "material-summary" => require_fields(&[
            ("record_id", &payload.record_id),
            ("table_role", &payload.table_role),
        ])?,
        _ => unreachable!("operation allowlist checked above"),
    }
    Ok(())
}

fn require_fields(fields: &[(&str, &str)]) -> Result<()> {
    for (name, value) in fields {
        if value.trim().is_empty() {
            bail!("{name} is required");
        }
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use clap::Parser;
    use serde_json::json;

    fn payload(value: serde_json::Value) -> ActivityPayload {
        serde_json::from_value(value).expect("test payload should deserialize")
    }

    fn feishu_config() -> ActivityFeishuBaseConfig {
        ActivityFeishuBaseConfig {
            base_token: "app_test".to_string(),
            plan_table_id: "tbl_plan".to_string(),
            occurrence_table_id: "tbl_occurrence".to_string(),
            profile_env_path: "/tmp/xiaoman.env".to_string(),
        }
    }

    fn runtime_with_fixture() -> ActivityRuntimeConfig {
        ActivityRuntimeConfig {
            fixture_path: Some(fixture_path()),
            feishu_base: None,
        }
    }

    fn runtime_without_source() -> ActivityRuntimeConfig {
        ActivityRuntimeConfig {
            fixture_path: None,
            feishu_base: None,
        }
    }

    fn fixture_path() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("fixtures/xiaoman_activity_records.json")
    }

    #[tokio::test]
    async fn record_get_reads_fixture_without_raw_record_id_in_view() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "record-get",
            "record_id": "rec_plan_20260628",
            "table_role": "activity_plan"
        }));

        validate("record-get", &payload).expect("record-get payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "record-get".to_string(),
            payload,
            true,
            false,
            &runtime_with_fixture(),
        )
        .await
        .expect("fixture read should succeed");

        assert_eq!(report.action_status, "read_ok");
        assert_eq!(report.source, "fixture");
        assert_eq!(report.record_count, 1);
        assert_eq!(report.records[0].title, "周日共创晚餐");
        assert!(!serde_json::to_string(&report.records)
            .unwrap()
            .contains("rec_plan_20260628"));
    }

    #[tokio::test]
    async fn list_by_date_filters_fixture_records() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "list-by-date",
            "date": "2026-06-28",
            "table_role": "activity_plan"
        }));

        validate("list-by-date", &payload).expect("list payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "list-by-date".to_string(),
            payload,
            true,
            false,
            &runtime_with_fixture(),
        )
        .await
        .expect("fixture list should succeed");

        assert_eq!(report.action_status, "read_ok");
        assert_eq!(report.record_count, 1);
        assert!(report.summaries[0].contains("周日共创晚餐"));
    }

    #[tokio::test]
    async fn material_summary_reads_fixture_material_fields() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "material-summary",
            "record_id": "rec_occurrence_20260628",
            "table_role": "activity_occurrence"
        }));

        validate("material-summary", &payload).expect("material-summary payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "material-summary".to_string(),
            payload,
            true,
            false,
            &runtime_with_fixture(),
        )
        .await
        .expect("fixture material summary should succeed");

        assert_eq!(report.action_status, "read_ok");
        assert_eq!(
            report.records[0].material_summary.as_deref(),
            Some("现场照片 6 张，待筛选 2 张可用于复盘")
        );
    }

    #[test]
    fn feishu_record_mapping_uses_table_role_and_hides_raw_id_in_view() {
        let value = json!({
            "record_id": "rec_feishu_plan_1",
            "fields": {
                "活动名称": [{"text": "周一共学"}],
                "活动日期": "2026-06-29",
                "地点": "秦托邦一楼",
                "小满运营状态": "待补素材"
            }
        });

        let record = ActivityRecord::from_feishu_value(&value, "activity_plan")
            .expect("Feishu record should map");
        let view = record.view();

        assert_eq!(view.table_role, "activity_plan");
        assert_eq!(view.title, "周一共学");
        assert_ne!(view.record_ref, "rec_feishu_plan_1");
        assert!(!serde_json::to_string(&view)
            .unwrap()
            .contains("rec_feishu_plan_1"));
    }

    #[test]
    fn feishu_record_mapping_normalizes_timestamp_activity_time() {
        let value = json!({
            "record_id": "rec_feishu_occurrence_1",
            "fields": {
                "活动信息": "夏至晚餐",
                "活动计划时间": 1782630000000_i64,
                "活动照片": [{"name": "dinner.jpg"}],
                "发起人": "小满"
            }
        });

        let record = ActivityRecord::from_feishu_value(&value, "activity_occurrence")
            .expect("Feishu occurrence should map");

        assert_eq!(record.title, "夏至晚餐");
        assert_eq!(record.start_time.as_deref(), Some("2026-06-28 15:00"));
        assert!(record.matches_date("2026-06-28"));
        assert_eq!(record.material_summary.as_deref(), Some("dinner.jpg"));
    }

    #[test]
    fn feishu_timestamp_normalization_ignores_plain_numbers() {
        assert_eq!(feishu_timestamp_as_shanghai_datetime(&json!(42)), None);
        assert_eq!(
            feishu_timestamp_as_shanghai_datetime(&json!("1782630000000")),
            Some("2026-06-28 15:00".to_string())
        );
    }

    #[test]
    fn feishu_config_maps_only_allowed_table_roles() {
        let config = feishu_config();

        assert_eq!(
            config.table_id_for_role("activity_plan").unwrap(),
            "tbl_plan"
        );
        assert_eq!(
            config.table_id_for_role("activity_occurrence").unwrap(),
            "tbl_occurrence"
        );
        assert!(config.table_id_for_role("arbitrary").is_err());
    }

    #[tokio::test]
    async fn read_apply_without_source_reports_configuration_gap() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "record-get",
            "record_id": "rec_activity_1",
            "table_role": "activity_occurrence"
        }));

        validate("record-get", &payload).expect("record-get payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "record-get".to_string(),
            payload,
            true,
            false,
            &runtime_without_source(),
        )
        .await
        .expect("validation-only read should succeed");

        assert_eq!(report.action_status, "read_source_not_configured");
        assert_eq!(report.record_count, 0);
    }

    #[tokio::test]
    async fn status_update_apply_is_not_implemented_yet() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "status-update",
            "record_id": "rec_activity_1",
            "table_role": "activity_plan",
            "status": "待人工确认"
        }));

        validate("status-update", &payload).expect("status-update payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "status-update".to_string(),
            payload,
            true,
            false,
            &runtime_without_source(),
        )
        .await
        .expect("write apply should stop at guardrail");

        assert!(report.success);
        assert!(report.apply_requested);
        assert!(!report.dry_run);
        assert_eq!(report.action_status, "apply_not_implemented");
        assert!(!report.safe_for_chat);
    }

    #[tokio::test]
    async fn handoff_create_dry_run_maps_visual_request_to_operations_work_item() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "handoff-create",
            "source_record_id": "rec_activity_1",
            "table_role": "activity_occurrence",
            "handoff_type": "visual_asset_request",
            "target_agent": "huabaosi",
            "brief_summary": "做一组活动前宣海报 brief"
        }));

        validate("handoff-create", &payload).expect("handoff payload should be valid");
        let report = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "handoff-create".to_string(),
            payload,
            false,
            true,
            &runtime_without_source(),
        )
        .await
        .expect("handoff dry-run should create operations preview");

        let work_item = report
            .operations_work_item
            .expect("operations work item report should exist");
        assert_eq!(report.action_status, "operations_dry_run_ok");
        assert_eq!(work_item.capability_key, "huabaosi.create_visual_asset");
        assert_eq!(work_item.work_item_type, "visual_asset_request");
        assert_eq!(work_item.requester_agent, "xiaoman");
        assert_eq!(work_item.target_agent, "huabaosi");
        assert!(work_item.idempotency_key.starts_with("ops:"));
        assert!(serde_json::to_string(&work_item)
            .unwrap()
            .contains("feishu_task"));
        assert!(!serde_json::to_string(&work_item)
            .unwrap()
            .contains("rec_activity_1"));
    }

    #[tokio::test]
    async fn handoff_create_rejects_unmapped_capability_pair() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "handoff-create",
            "source_record_id": "rec_activity_1",
            "handoff_type": "member_notice",
            "target_agent": "erhua",
            "brief_summary": "提醒成员报名"
        }));

        validate("handoff-create", &payload).expect("payload shape should be valid");
        let err = execute_with_config(
            &Cli::parse_from(["qintopia-message-sidecar", "check"]),
            "handoff-create".to_string(),
            payload,
            false,
            true,
            &runtime_without_source(),
        )
        .await
        .expect_err("unmapped handoff should be rejected");

        assert!(err
            .to_string()
            .contains("handoff-create is not mapped to an operations capability"));
    }

    #[test]
    fn handoff_rejects_unapproved_target_agent() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "handoff-create",
            "source_record_id": "rec_activity_1",
            "handoff_type": "visual_asset_request",
            "target_agent": "unknown",
            "brief_summary": "做一组活动前宣海报 brief"
        }));

        let err = validate("handoff-create", &payload).expect_err("target should be rejected");
        assert!(err.to_string().contains("target_agent is not allowed"));
    }

    #[test]
    fn rejects_non_xiaoman_actor() {
        let payload = payload(json!({
            "actor_agent": "erhua",
            "operation": "record-get",
            "record_id": "rec_activity_1",
            "table_role": "activity_occurrence"
        }));

        let err = validate("record-get", &payload).expect_err("actor should be rejected");
        assert!(err.to_string().contains("actor_agent must be xiaoman"));
    }

    #[test]
    fn rejects_disallowed_table_role() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "list-by-date",
            "date": "2026-06-28",
            "table_role": "arbitrary_table"
        }));

        let err = validate("list-by-date", &payload).expect_err("table role should be rejected");
        assert!(err.to_string().contains("table_role is not allowed"));
    }

    #[test]
    fn rejects_operation_mismatch_between_cli_and_payload() {
        let payload = payload(json!({
            "actor_agent": "xiaoman",
            "operation": "record-get",
            "record_id": "rec_activity_1",
            "table_role": "activity_occurrence"
        }));

        let err = validate("list-by-date", &payload).expect_err("operation mismatch should fail");
        assert!(err
            .to_string()
            .contains("operation mismatch between CLI and payload"));
    }

    #[test]
    fn decodes_chunked_body() {
        let body = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        assert_eq!(decode_chunked_body(body).unwrap(), "hello world");
    }

    #[test]
    fn decodes_chunked_body_with_extensions_and_trailer() {
        let body = b"\r\n5;foo=bar\r\nhello\r\n6\r\n world\r\n0\r\nx-trace: ok\r\n\r\n";
        assert_eq!(decode_chunked_body(body).unwrap(), "hello world");
    }

    #[test]
    fn parses_chunked_http_response_as_bytes() {
        let response = concat!(
            "HTTP/1.1 200 OK\r\n",
            "Transfer-Encoding: chunked\r\n",
            "\r\n",
            "10\r\n",
            "{\"msg\":\"你好\"}",
            "\r\n0\r\n\r\n"
        )
        .as_bytes();
        assert_eq!(parse_http_response(response).unwrap(), "{\"msg\":\"你好\"}");
    }

    #[test]
    fn sanitizes_feishu_url_in_error_context() {
        let sanitized = sanitize_feishu_url(
            "https://open.feishu.cn/open-apis/bitable/v1/apps/app_token_123/tables/tbl_secret_456/records?page_size=200&page_token=next",
        );

        assert!(sanitized
            .contains("open.feishu.cn/open-apis/bitable/v1/apps/redacted/tables/redacted/records"));
        assert!(!sanitized.contains("app_token_123"));
        assert!(!sanitized.contains("tbl_secret_456"));
        assert!(!sanitized.contains("page_token"));
    }

    #[test]
    fn feishu_read_limitation_does_not_expose_internal_tool_names() {
        for forbidden in [
            "lark-base",
            "execute_code",
            "terminal",
            "skill_view",
            "Dangerous command requires approval",
            "Working",
        ] {
            assert!(
                !FEISHU_READ_LIMITATION.contains(forbidden),
                "forbidden term leaked in Feishu read limitation: {forbidden}"
            );
        }
    }
}
