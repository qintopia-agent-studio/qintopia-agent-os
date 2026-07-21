use std::{
    fs,
    io::{Read, Write},
    net::TcpStream,
    sync::Arc,
    time::Duration as StdDuration,
};

use anyhow::{anyhow, bail, Context, Result};
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{postgres::PgPool, Row};
use tokio::time::sleep;
use uuid::Uuid;

use crate::{config::Cli, db, event_signal};

const TOOL_NAME: &str = "qintopia_daily_digest_publish";
const REQUIRED_OWNER_AGENT: &str = "xiaoman";
const FEISHU_BASE_API: &str = "https://open.feishu.cn/open-apis/bitable/v1/apps";
const FEISHU_AUTH_API: &str =
    "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal";

#[derive(Debug, Clone)]
pub struct PublishOptions {
    pub apply: bool,
    pub dry_run: bool,
    pub digest_id: Uuid,
    pub actor_agent: String,
}

#[derive(Debug, Clone)]
pub struct PublisherWorkerOptions {
    pub check_only: bool,
    pub once: bool,
    pub poll_seconds: u64,
    pub batch_size: i64,
    pub actor_agent: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct PublishReport {
    dry_run: bool,
    digest_id: String,
    actor_agent: String,
    owner_agent: String,
    chat_id: String,
    title: String,
    base_token: Option<String>,
    daily_table_id: Option<String>,
    signal_table_id: Option<String>,
    archive_table_id: Option<String>,
    previous_publish_status: String,
    publish_status: String,
    daily_record_id: Option<String>,
    archive_record_id: Option<String>,
    signal_records_planned: usize,
    signal_records_written: usize,
    feishu_document_token: Option<String>,
    feishu_document_url: Option<String>,
    error: Option<String>,
    guardrails: Vec<String>,
}

#[derive(Debug, Clone)]
struct DigestRow {
    id: Uuid,
    owner_agent: String,
    platform: String,
    chat_id: String,
    digest_date: chrono::NaiveDate,
    title: String,
    markdown: String,
    feishu_document_token: Option<String>,
    feishu_document_url: Option<String>,
    publish_status: String,
    message_count: i64,
    useful_signal_count: i64,
    generated_at: chrono::DateTime<chrono::Utc>,
    metadata: Value,
}

#[derive(Debug, Clone)]
struct FeishuBaseConfig {
    base_token: String,
    daily_table_id: String,
    signal_table_id: String,
    archive_table_id: String,
    profile_env_path: String,
}

#[derive(Debug, Clone)]
struct DigestPublishModel {
    chat_name: String,
    summary: String,
    markdown_summary: String,
    signals: Vec<SignalRecord>,
}

#[derive(Debug, Clone, Serialize)]
struct SignalRecord {
    event_signal_id: Uuid,
    signal_type: String,
    title: String,
    summary: String,
    related_members: String,
    owner_name: String,
    owner_agent: String,
    priority: String,
    status: String,
    evidence: String,
    source_message_ids: String,
    risk_level: String,
    external_publish_status: String,
    feishu_record_id: Option<String>,
}

#[derive(Debug, Clone)]
struct FeishuRecord {
    record_id: String,
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

pub async fn run(cli: &Cli, options: PublishOptions) -> Result<()> {
    if options.apply && options.dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let apply = options.apply && !options.dry_run;
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let report = run_inner(&pool, cli, &options, apply).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_worker(cli: &Cli, options: PublisherWorkerOptions) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let poll_seconds = options.poll_seconds.max(30);

    if options.check_only || options.once {
        let reports = publish_pending_batch(
            &pool,
            cli,
            &options.actor_agent,
            !options.check_only,
            options.batch_size,
        )
        .await?;
        println!("{}", serde_json::to_string_pretty(&reports)?);
        return Ok(());
    }

    tracing::info!(
        poll_seconds,
        batch_size = options.batch_size,
        actor_agent = %options.actor_agent,
        "starting daily digest publisher worker"
    );
    loop {
        match publish_pending_batch(&pool, cli, &options.actor_agent, true, options.batch_size)
            .await
        {
            Ok(reports) => {
                tracing::info!(
                    published = reports
                        .iter()
                        .filter(|item| item.publish_status == "published")
                        .count(),
                    failed = reports
                        .iter()
                        .filter(|item| item.publish_status == "publish_failed"
                            || item.publish_status == "publish_denied")
                        .count(),
                    scanned = reports.len(),
                    "daily digest publisher worker batch complete"
                );
            }
            Err(error) => {
                tracing::warn!(error = %error, "daily digest publisher worker batch failed");
            }
        }
        sleep(StdDuration::from_secs(poll_seconds)).await;
    }
}

async fn publish_pending_batch(
    pool: &PgPool,
    cli: &Cli,
    actor_agent: &str,
    apply: bool,
    batch_size: i64,
) -> Result<Vec<PublishReport>> {
    let digest_ids = load_publishable_digest_ids(pool, batch_size).await?;
    let mut reports = Vec::new();
    for digest_id in digest_ids {
        let options = PublishOptions {
            apply,
            dry_run: !apply,
            digest_id,
            actor_agent: actor_agent.to_string(),
        };
        reports.push(run_inner(pool, cli, &options, apply).await?);
    }
    Ok(reports)
}

async fn load_publishable_digest_ids(pool: &PgPool, batch_size: i64) -> Result<Vec<Uuid>> {
    let rows = sqlx::query(
        r#"
        SELECT id
        FROM qintopia_agent_os.daily_digests
        WHERE owner_agent = $1
          AND platform = 'qiwe'
          AND (
              publish_status IN ('pending_feishu_publish', 'pending_feishu_parent_node')
              OR (
                  publish_status = 'publish_failed'
                  AND publish_attempts < 5
                  AND updated_at <= now() - interval '5 minutes'
              )
          )
        ORDER BY digest_date ASC, updated_at ASC
        LIMIT $2
        "#,
    )
    .bind(REQUIRED_OWNER_AGENT)
    .bind(batch_size.max(1))
    .fetch_all(pool)
    .await
    .context("load publishable daily digest ids")?;
    rows.into_iter()
        .map(|row| row.try_get("id").context("read digest id"))
        .collect()
}

async fn run_inner(
    pool: &PgPool,
    cli: &Cli,
    options: &PublishOptions,
    apply: bool,
) -> Result<PublishReport> {
    let digest = load_digest(pool, options.digest_id)
        .await?
        .context("daily digest not found")?;
    let guardrails = publish_guardrails();
    let mut model = build_publish_model(pool, &digest).await?;
    let base_config = feishu_base_config(cli);

    if let Some(error) =
        validate_publish_request(cli, &digest, &options.actor_agent, &base_config, apply)
    {
        if apply {
            record_publish_failure(pool, &digest, &options.actor_agent, &error, "guard_denied")
                .await?;
        }
        return Ok(report_from_digest(
            &digest,
            options,
            base_config.as_ref(),
            "publish_denied",
            Some(error),
            guardrails,
            None,
            None,
            model.signals.len(),
            0,
            !apply,
        ));
    }

    if !apply {
        return Ok(report_from_digest(
            &digest,
            options,
            base_config.as_ref(),
            "dry_run_publish_ready",
            None,
            guardrails,
            None,
            None,
            model.signals.len(),
            0,
            true,
        ));
    }

    let base_config = base_config.expect("validated base config");

    let client = match FeishuClient::from_profile_env(&base_config.profile_env_path) {
        Ok(client) => client,
        Err(error) => {
            let error = format!("load Feishu client: {error:#}");
            record_publish_failure(
                pool,
                &digest,
                &options.actor_agent,
                &error,
                "feishu_client_config_error",
            )
            .await?;
            return Ok(report_from_digest(
                &digest,
                options,
                Some(&base_config),
                "publish_failed",
                Some(error),
                guardrails,
                None,
                None,
                model.signals.len(),
                0,
                false,
            ));
        }
    };

    let publish_result = publish_to_feishu(&client, &base_config, &digest, &mut model);
    let (archive_record_id, daily_record_id, signal_written) = match publish_result {
        Ok(result) => result,
        Err(error) => {
            let error = format!("{error:#}");
            record_publish_failure(
                pool,
                &digest,
                &options.actor_agent,
                &error,
                "publish_failed",
            )
            .await?;
            return Ok(report_from_digest(
                &digest,
                options,
                Some(&base_config),
                "publish_failed",
                Some(error),
                guardrails,
                None,
                None,
                model.signals.len(),
                0,
                false,
            ));
        }
    };
    update_event_signal_publish_state(pool, &model.signals).await?;

    let feishu_url = format!(
        "https://ranuox3qst4.feishu.cn/wiki/JXAOwfu5UibrpMkb61yc1npenGh?table={}",
        base_config.daily_table_id
    );
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.daily_digests
        SET publish_status = 'published',
            feishu_document_token = $2,
            feishu_document_url = $3,
            publish_error = NULL,
            publish_attempts = publish_attempts + 1,
            published_at = now(),
            metadata = metadata || $4::jsonb,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(digest.id)
    .bind(&daily_record_id)
    .bind(&feishu_url)
    .bind(json!({
        "published_by": TOOL_NAME,
        "publisher_agent": options.actor_agent,
        "feishu_base_token": base_config.base_token,
        "daily_record_id": daily_record_id,
        "archive_record_id": archive_record_id,
        "signal_records_written": signal_written,
        "markdown_sha256": sha256_hex(&digest.markdown)
    }))
    .execute(pool)
    .await
    .context("mark daily digest published")?;
    insert_audit(
        pool,
        &digest,
        &options.actor_agent,
        "publish",
        "published",
        Some(&daily_record_id),
        Some(&feishu_url),
        None,
        json!({
            "tool_boundary": "narrow_daily_digest_publish",
            "feishu_base_token": base_config.base_token,
            "daily_table_id": base_config.daily_table_id,
            "signal_table_id": base_config.signal_table_id,
            "archive_table_id": base_config.archive_table_id,
            "archive_record_id": archive_record_id,
            "signal_records_written": signal_written
        }),
    )
    .await?;

    Ok(report_from_digest(
        &digest,
        options,
        Some(&base_config),
        "published",
        None,
        guardrails,
        Some(daily_record_id),
        Some(archive_record_id),
        model.signals.len(),
        signal_written,
        false,
    ))
}

fn publish_to_feishu(
    client: &FeishuClient,
    config: &FeishuBaseConfig,
    digest: &DigestRow,
    model: &mut DigestPublishModel,
) -> Result<(String, String, usize)> {
    let now_cell = datetime_cell(chrono::Utc::now().timestamp_millis());
    let digest_date_cell = date_cell(digest.digest_date);
    let generated_cell = datetime_cell(digest.generated_at.timestamp_millis());
    let markdown_hash = sha256_hex(&digest.markdown);
    let archive_fields = json!({
        "文档标题": digest.title,
        "digest_id": digest.id.to_string(),
        "日报日期": digest_date_cell.clone(),
        "群名称": model.chat_name,
        "群ID": digest.chat_id,
        "飞书文档链接": url_cell(digest.feishu_document_url.as_deref(), &digest.title),
        "文档 token": digest.feishu_document_token.clone().unwrap_or_default(),
        "Markdown 摘要": model.markdown_summary,
        "Markdown SHA256": markdown_hash,
        "发布状态": "已发布",
        "发布错误": "",
        "发布人/Agent": REQUIRED_OWNER_AGENT,
        "发布时间": now_cell.clone(),
        "创建时间": now_cell.clone()
    });
    let archive = client.upsert_by_text_field(
        &config.base_token,
        &config.archive_table_id,
        "digest_id",
        &digest.id.to_string(),
        archive_fields,
    )?;

    let daily_fields = json!({
        "标题": digest.title,
        "日报日期": digest_date_cell.clone(),
        "群名称": model.chat_name,
        "群ID": digest.chat_id,
        "Owner Agent": digest.owner_agent,
        "今日摘要": model.summary,
        "消息数": digest.message_count,
        "有效信号数": digest.useful_signal_count,
        "日报文档": [archive.record_id],
        "发布状态": "已发布",
        "生成状态": "已生成",
        "digest_id": digest.id.to_string(),
        "生成时间": generated_cell.clone(),
        "更新时间": now_cell.clone(),
        "备注": ""
    });
    let daily = client.upsert_by_text_field(
        &config.base_token,
        &config.daily_table_id,
        "digest_id",
        &digest.id.to_string(),
        daily_fields,
    )?;

    let mut written = 0usize;
    for signal in &mut model.signals {
        let key = signal.event_signal_id.to_string();
        let fields = json!({
            "事件标题": signal.title,
            "关联日报": [daily.record_id],
            "事件日期": digest_date_cell.clone(),
            "群名称": model.chat_name,
            "群ID": digest.chat_id,
            "信号类型": signal.signal_type,
            "事件摘要": signal.summary,
            "相关成员": signal.related_members,
            "建议负责人": signal.owner_name,
            "建议 Agent": signal.owner_agent,
            "优先级": signal.priority,
            "处理状态": signal.status,
            "截止时间": Value::Null,
            "证据摘要": signal.evidence,
            "source_message_ids": key,
            "风险级别": signal.risk_level,
            "是否适合对外发布": false,
            "外部发布状态": signal.external_publish_status,
            "创建时间": now_cell.clone(),
            "更新时间": now_cell.clone()
        });
        let record = if let Some(record_id) = signal.feishu_record_id.as_deref() {
            client.update_record(
                &config.base_token,
                &config.signal_table_id,
                record_id,
                fields,
            )?;
            FeishuRecord {
                record_id: record_id.to_string(),
            }
        } else {
            client.create_record(&config.base_token, &config.signal_table_id, fields)?
        };
        signal.feishu_record_id = Some(record.record_id);
        written += 1;
    }
    Ok((archive.record_id, daily.record_id, written))
}

fn publish_guardrails() -> Vec<String> {
    vec![
        "digest_id only; no arbitrary document write".to_string(),
        "owner_agent and actor_agent must be xiaoman".to_string(),
        "chat_id must be configured in QINTOPIA_PROFILE_TARGET_CHAT_IDS".to_string(),
        "Feishu Base token must be explicitly allowlisted".to_string(),
        "publisher writes only the configured daily/signal/archive tables".to_string(),
        "publisher does not post to QiWe groups".to_string(),
    ]
}

#[expect(
    clippy::too_many_arguments,
    reason = "the report mirrors the explicit publisher outcome contract"
)]
fn report_from_digest(
    digest: &DigestRow,
    options: &PublishOptions,
    config: Option<&FeishuBaseConfig>,
    status: &str,
    error: Option<String>,
    guardrails: Vec<String>,
    daily_record_id: Option<String>,
    archive_record_id: Option<String>,
    signals_planned: usize,
    signals_written: usize,
    dry_run: bool,
) -> PublishReport {
    PublishReport {
        dry_run,
        digest_id: digest.id.to_string(),
        actor_agent: options.actor_agent.clone(),
        owner_agent: digest.owner_agent.clone(),
        chat_id: digest.chat_id.clone(),
        title: digest.title.clone(),
        base_token: config.map(|item| item.base_token.clone()),
        daily_table_id: config.map(|item| item.daily_table_id.clone()),
        signal_table_id: config.map(|item| item.signal_table_id.clone()),
        archive_table_id: config.map(|item| item.archive_table_id.clone()),
        previous_publish_status: digest.publish_status.clone(),
        publish_status: status.to_string(),
        daily_record_id,
        archive_record_id,
        signal_records_planned: signals_planned,
        signal_records_written: signals_written,
        feishu_document_token: digest.feishu_document_token.clone(),
        feishu_document_url: digest.feishu_document_url.clone(),
        error,
        guardrails,
    }
}

fn validate_publish_request(
    cli: &Cli,
    digest: &DigestRow,
    actor_agent: &str,
    base_config: &Option<FeishuBaseConfig>,
    apply: bool,
) -> Option<String> {
    if actor_agent != REQUIRED_OWNER_AGENT {
        return Some(format!("actor_agent must be {REQUIRED_OWNER_AGENT}"));
    }
    if digest.owner_agent != REQUIRED_OWNER_AGENT {
        return Some(format!("owner_agent must be {REQUIRED_OWNER_AGENT}"));
    }
    if digest.platform != "qiwe" {
        return Some("only qiwe daily digests are publishable in V1".to_string());
    }
    let target_chats = cli.profile_target_chat_ids();
    if !target_chats
        .iter()
        .any(|chat_id| chat_id == &digest.chat_id)
    {
        return Some("chat_id is not in QINTOPIA_PROFILE_TARGET_CHAT_IDS".to_string());
    }
    if !matches!(
        digest.publish_status.as_str(),
        "pending_feishu_publish" | "pending_feishu_parent_node" | "publish_failed" | "published"
    ) {
        return Some(
            "publish_status must be pending_feishu_publish, pending_feishu_parent_node, publish_failed, or published"
                .to_string(),
        );
    }
    let Some(config) = base_config.as_ref() else {
        return apply.then(|| "Feishu Base publisher config is incomplete".to_string());
    };
    let allowed_tokens = cli.daily_digest_allowed_feishu_base_tokens();
    if allowed_tokens.is_empty()
        || !allowed_tokens
            .iter()
            .any(|token| token == &config.base_token)
    {
        return Some("feishu base token is not allowlisted".to_string());
    }
    None
}

async fn load_digest(pool: &PgPool, digest_id: Uuid) -> Result<Option<DigestRow>> {
    let row = sqlx::query(
        r#"
        SELECT id, owner_agent, platform, chat_id, digest_date, title, markdown,
               feishu_document_token, feishu_document_url, publish_status,
               message_count, useful_signal_count, generated_at, metadata
        FROM qintopia_agent_os.daily_digests
        WHERE id = $1
        "#,
    )
    .bind(digest_id)
    .fetch_optional(pool)
    .await
    .context("load daily digest")?;
    Ok(row.map(|row| DigestRow {
        id: row.get("id"),
        owner_agent: row.get("owner_agent"),
        platform: row.get("platform"),
        chat_id: row.get("chat_id"),
        digest_date: row.get("digest_date"),
        title: row.get("title"),
        markdown: row.get("markdown"),
        feishu_document_token: row.get("feishu_document_token"),
        feishu_document_url: row.get("feishu_document_url"),
        publish_status: row.get("publish_status"),
        message_count: row.get("message_count"),
        useful_signal_count: row.get("useful_signal_count"),
        generated_at: row.get("generated_at"),
        metadata: row.get("metadata"),
    }))
}

fn feishu_base_config(cli: &Cli) -> Option<FeishuBaseConfig> {
    Some(FeishuBaseConfig {
        base_token: cli.daily_digest_feishu_base_token.clone()?,
        daily_table_id: cli.daily_digest_feishu_daily_table_id.clone()?,
        signal_table_id: cli.daily_digest_feishu_signal_table_id.clone()?,
        archive_table_id: cli.daily_digest_feishu_archive_table_id.clone()?,
        profile_env_path: cli.daily_digest_feishu_profile_env_path.clone(),
    })
}

async fn build_publish_model(pool: &PgPool, digest: &DigestRow) -> Result<DigestPublishModel> {
    let event_rows =
        event_signal::load_event_signals_for_digest(pool, &digest.chat_id, digest.digest_date)
            .await?;
    let chat_name = digest
        .metadata
        .get("chat_display_name")
        .and_then(Value::as_str)
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&digest.chat_id)
        .to_string();
    let summary = if event_rows.is_empty() {
        "今日无显著群聊运营信号。".to_string()
    } else {
        format!("今日识别到 {} 条结构化运营事件。", event_rows.len())
    };
    let markdown_summary = digest.markdown.chars().take(1200).collect::<String>();
    let signals = event_rows
        .into_iter()
        .map(|event| SignalRecord {
            event_signal_id: event.id,
            signal_type: event.signal_type,
            title: event.title,
            summary: event.summary,
            related_members: event.related_member_names.join("、"),
            owner_name: event.owner_name,
            owner_agent: event.owner_agent,
            priority: event.priority,
            status: event.status,
            evidence: format!(
                "{}；模型：{}；原因：{}",
                event.dedupe_key, event.judge_model, event.judge_reason
            ),
            source_message_ids: event
                .source_message_ids
                .iter()
                .map(Uuid::to_string)
                .collect::<Vec<_>>()
                .join("\n"),
            risk_level: event.risk_level,
            external_publish_status: event.external_publish_status,
            feishu_record_id: event.feishu_record_id,
        })
        .collect();
    Ok(DigestPublishModel {
        chat_name,
        summary,
        markdown_summary,
        signals,
    })
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

    fn upsert_by_text_field(
        &self,
        base_token: &str,
        table_id: &str,
        field_name: &str,
        value: &str,
        fields: Value,
    ) -> Result<FeishuRecord> {
        if let Some(record) =
            self.find_record_by_text_field(base_token, table_id, field_name, value)?
        {
            self.update_record(base_token, table_id, &record.record_id, fields)?;
            Ok(record)
        } else {
            self.create_record(base_token, table_id, fields)
        }
    }

    fn find_record_by_text_field(
        &self,
        base_token: &str,
        table_id: &str,
        field_name: &str,
        value: &str,
    ) -> Result<Option<FeishuRecord>> {
        let records = self.list_records(base_token, table_id, 200)?;
        let found = records.into_iter().find(|record| {
            record
                .get("fields")
                .and_then(|fields| fields.get(field_name))
                .and_then(feishu_cell_as_string)
                .as_deref()
                == Some(value)
        });
        Ok(found.and_then(|record| {
            let record_id = record
                .get("record_id")
                .or_else(|| record.get("id"))
                .and_then(Value::as_str)?;
            Some(FeishuRecord {
                record_id: record_id.to_string(),
            })
        }))
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
            let parsed = self.get(&url)?;
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

    fn create_record(
        &self,
        base_token: &str,
        table_id: &str,
        fields: Value,
    ) -> Result<FeishuRecord> {
        let parsed = self.post(
            &format!("{FEISHU_BASE_API}/{base_token}/tables/{table_id}/records"),
            &json!({"fields": fields}),
        )?;
        record_from_response(&parsed).context("Feishu create record response missing record id")
    }

    fn update_record(
        &self,
        base_token: &str,
        table_id: &str,
        record_id: &str,
        fields: Value,
    ) -> Result<()> {
        self.put(
            &format!("{FEISHU_BASE_API}/{base_token}/tables/{table_id}/records/{record_id}"),
            &json!({"fields": fields}),
        )?;
        Ok(())
    }

    fn get(&self, url: &str) -> Result<Value> {
        self.request_json("GET", url, None)
    }

    fn post(&self, url: &str, body: &Value) -> Result<Value> {
        self.request_json("POST", url, Some(body))
    }

    fn put(&self, url: &str, body: &Value) -> Result<Value> {
        self.request_json("PUT", url, Some(body))
    }

    fn request_json(&self, method: &str, url: &str, body: Option<&Value>) -> Result<Value> {
        let response = request_json(
            method,
            url,
            Some(&self.tenant_token),
            body,
            self.tls_config.clone(),
        )
        .with_context(|| format!("call Feishu API {method} {url}"))?;
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

fn record_from_response(value: &Value) -> Option<FeishuRecord> {
    let data = value.get("data")?;
    let record_id = data
        .get("record")
        .and_then(|record| record.get("record_id").or_else(|| record.get("id")))
        .or_else(|| data.get("record_id"))
        .or_else(|| data.get("id"))
        .and_then(Value::as_str)?;
    Some(FeishuRecord {
        record_id: record_id.to_string(),
    })
}

fn feishu_cell_as_string(value: &Value) -> Option<String> {
    match value {
        Value::String(text) => Some(text.clone()),
        Value::Array(items) => {
            let mut parts = Vec::new();
            for item in items {
                if let Some(text) = item.get("text").and_then(Value::as_str) {
                    parts.push(text.to_string());
                } else if let Some(text) = item.as_str() {
                    parts.push(text.to_string());
                }
            }
            if parts.is_empty() {
                None
            } else {
                Some(parts.join(""))
            }
        }
        _ => None,
    }
}

async fn record_publish_failure(
    pool: &PgPool,
    digest: &DigestRow,
    actor_agent: &str,
    error: &str,
    action: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.daily_digests
        SET publish_status = 'publish_failed',
            publish_error = $2,
            publish_attempts = publish_attempts + 1,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(digest.id)
    .bind(error)
    .execute(pool)
    .await
    .context("mark daily digest publish failed")?;
    insert_audit(
        pool,
        digest,
        actor_agent,
        action,
        "failed",
        None,
        None,
        Some(error),
        json!({}),
    )
    .await
}

#[expect(
    clippy::too_many_arguments,
    reason = "the audit record fields remain explicit at the database boundary"
)]
async fn insert_audit(
    pool: &PgPool,
    digest: &DigestRow,
    actor_agent: &str,
    action: &str,
    status: &str,
    token: Option<&str>,
    url: Option<&str>,
    error: Option<&str>,
    metadata: serde_json::Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.daily_digest_publish_audit
            (digest_id, actor_agent, tool_name, action, status, feishu_document_token,
             feishu_document_url, error, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9)
        "#,
    )
    .bind(digest.id)
    .bind(actor_agent)
    .bind(TOOL_NAME)
    .bind(action)
    .bind(status)
    .bind(token)
    .bind(url)
    .bind(error)
    .bind(metadata)
    .execute(pool)
    .await
    .context("insert daily digest publish audit")?;
    Ok(())
}

async fn update_event_signal_publish_state(pool: &PgPool, signals: &[SignalRecord]) -> Result<()> {
    for signal in signals {
        let Some(record_id) = signal.feishu_record_id.as_deref() else {
            continue;
        };
        sqlx::query(
            r#"
            UPDATE qintopia_agent_os.event_signals
            SET feishu_record_id = $2,
                last_published_at = now(),
                updated_at = now()
            WHERE id = $1
            "#,
        )
        .bind(signal.event_signal_id)
        .bind(record_id)
        .execute(pool)
        .await
        .context("update event signal Feishu publish state")?;
    }
    Ok(())
}

fn date_cell(date: chrono::NaiveDate) -> Value {
    let millis = date
        .and_hms_opt(0, 0, 0)
        .expect("midnight is valid")
        .and_utc()
        .timestamp_millis();
    datetime_cell(millis)
}

fn datetime_cell(timestamp_millis: i64) -> Value {
    json!(timestamp_millis)
}

fn url_cell(url: Option<&str>, text: &str) -> Value {
    let Some(url) = url.map(str::trim).filter(|value| !value.is_empty()) else {
        return Value::Null;
    };
    json!({
        "link": url,
        "text": text
    })
}

fn sha256_hex(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
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
        body.len()
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
    let server_name = ServerName::try_from(host.to_string()).context("validate Feishu API host")?;
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

fn parse_http_response(response: &[u8]) -> Result<String> {
    let text = String::from_utf8_lossy(response);
    let (head, body) = text
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow!("invalid HTTP response"))?;
    let status_line = head.lines().next().unwrap_or_default();
    if !status_line.contains(" 200 ") {
        bail!("HTTP request failed: {status_line}; body={body}");
    }
    let is_chunked = head
        .lines()
        .any(|line| line.eq_ignore_ascii_case("transfer-encoding: chunked"));
    if is_chunked {
        decode_chunked_body(body.as_bytes())
    } else {
        Ok(body.to_string())
    }
}

fn decode_chunked_body(body: &[u8]) -> Result<String> {
    let mut index = 0usize;
    let mut decoded = Vec::new();
    while index < body.len() {
        let line_end = find_crlf(body, index).ok_or_else(|| anyhow!("invalid chunked body"))?;
        let size_text = std::str::from_utf8(&body[index..line_end])
            .context("decode chunk size")?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim();
        let size = usize::from_str_radix(size_text, 16).context("parse chunk size")?;
        index = line_end + 2;
        if size == 0 {
            break;
        }
        if index + size > body.len() {
            bail!("chunk exceeds body length");
        }
        decoded.extend_from_slice(&body[index..index + size]);
        index += size + 2;
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
    let roots = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    ClientConfig::builder()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn date_cell_uses_millisecond_timestamp_number() {
        let date = chrono::NaiveDate::from_ymd_opt(2026, 6, 26).unwrap();
        assert_eq!(date_cell(date), json!(1782432000000_i64));
    }

    #[test]
    fn url_cell_uses_link_object_when_present() {
        assert_eq!(
            url_cell(Some("https://example.com/doc"), "日报"),
            json!({"link": "https://example.com/doc", "text": "日报"})
        );
        assert_eq!(url_cell(None, "日报"), Value::Null);
    }

    #[test]
    fn feishu_link_cells_use_record_id_string_lists() {
        let value = json!({"关联日报": ["rec123"]});
        assert_eq!(value["关联日报"], json!(["rec123"]));
    }

    #[test]
    fn decodes_chunked_body() {
        let body = b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n";
        assert_eq!(decode_chunked_body(body).unwrap(), "hello world");
    }
}
