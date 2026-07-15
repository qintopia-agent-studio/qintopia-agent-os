use std::{collections::HashMap, time::Duration as StdDuration};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Duration, NaiveDate, Utc};
use serde::Serialize;
use serde_json::json;
use sqlx::{postgres::PgPool, Row};
use tracing::{info, warn};
use uuid::Uuid;

use crate::{activity_lifecycle::initial_phase_for_signal, config::Cli, db};

const EXTRACTION_VERSION: &str = "event_signal_v2_rule_20260627";
const JUDGE_MODEL: &str = "rule_v2";

#[derive(Debug, Clone)]
pub struct EventSignalOptions {
    pub apply: bool,
    pub dry_run: bool,
    pub chat_id: Option<String>,
    pub date: Option<NaiveDate>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct EventSignalWorkerOptions {
    pub check_only: bool,
    pub once: bool,
    pub chat_id: Option<String>,
    pub date: Option<NaiveDate>,
    pub poll_seconds: u64,
    pub limit: i64,
}

#[derive(Debug, Clone, Serialize)]
pub struct EventSignalReport {
    dry_run: bool,
    target_chat_id: String,
    signal_date: NaiveDate,
    messages_scanned: i64,
    candidates: Vec<CandidatePreview>,
    candidate_count: i64,
    accepted_events: Vec<EventPreview>,
    accepted_event_count: i64,
    candidates_written: i64,
    events_written: i64,
}

#[derive(Debug, Clone, Serialize)]
struct CandidatePreview {
    message_id: Uuid,
    sender_name: Option<String>,
    labels: Vec<String>,
    score: f64,
    text: String,
    filter_reason: String,
}

#[derive(Debug, Clone, Serialize)]
struct EventPreview {
    signal_type: String,
    activity_phase: Option<String>,
    title: String,
    summary: String,
    owner_agent: String,
    priority: String,
    confidence: f64,
    source_message_ids: Vec<Uuid>,
    judge_reason: String,
}

#[derive(Debug, Clone)]
pub(crate) struct EventSignalRow {
    pub id: Uuid,
    pub signal_type: String,
    pub title: String,
    pub summary: String,
    pub related_member_names: Vec<String>,
    pub owner_name: String,
    pub owner_agent: String,
    pub priority: String,
    pub status: String,
    pub confidence: f64,
    pub source_message_ids: Vec<Uuid>,
    pub source_window_start: Option<DateTime<Utc>>,
    pub source_window_end: Option<DateTime<Utc>>,
    pub dedupe_key: String,
    pub judge_model: String,
    pub judge_reason: String,
    pub risk_level: String,
    pub external_publish_status: String,
    pub feishu_record_id: Option<String>,
}

#[derive(Debug, Clone)]
struct MessageRow {
    id: Uuid,
    sender_person_id: Option<Uuid>,
    sender_channel_identity_id: Option<Uuid>,
    sender_name: Option<String>,
    sender_is_bot: bool,
    text: String,
    received_at: DateTime<Utc>,
}

#[derive(Debug, Clone)]
struct Candidate {
    message: MessageRow,
    labels: Vec<String>,
    score: f64,
    filter_reason: String,
}

#[derive(Debug, Clone)]
struct AcceptedEvent {
    signal_type: String,
    title: String,
    summary: String,
    owner_name: String,
    owner_agent: String,
    priority: String,
    confidence: f64,
    source_candidate_ids: Vec<Uuid>,
    source_message_ids: Vec<Uuid>,
    source_window_start: Option<DateTime<Utc>>,
    source_window_end: Option<DateTime<Utc>>,
    related_member_names: Vec<String>,
    dedupe_key: String,
    judge_reason: String,
    risk_level: String,
    external_publish_status: String,
}

pub async fn run(cli: &Cli, options: EventSignalOptions) -> Result<()> {
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

pub async fn run_worker(cli: &Cli, options: EventSignalWorkerOptions) -> Result<()> {
    if options.check_only || options.once {
        let one_shot = EventSignalOptions {
            apply: !options.check_only,
            dry_run: options.check_only,
            chat_id: options.chat_id.clone(),
            date: options.date,
            limit: Some(options.limit),
        };
        return run(cli, one_shot).await;
    }
    info!(
        poll_seconds = options.poll_seconds,
        "starting event signal worker"
    );
    loop {
        let one_shot = EventSignalOptions {
            apply: true,
            dry_run: false,
            chat_id: options.chat_id.clone(),
            date: options.date,
            limit: Some(options.limit),
        };
        if let Err(error) = run(cli, one_shot).await {
            warn!(error = %error, "event signal worker cycle failed");
        }
        tokio::time::sleep(StdDuration::from_secs(options.poll_seconds)).await;
    }
}

async fn run_inner(
    pool: &PgPool,
    cli: &Cli,
    options: &EventSignalOptions,
    apply: bool,
) -> Result<EventSignalReport> {
    let target_chat_id = target_chat_ids(cli, options.chat_id.as_deref())?
        .into_iter()
        .next()
        .context("event signal extraction requires one target chat id")?;
    let signal_date = options
        .date
        .unwrap_or_else(|| default_signal_date(&cli.daily_digest_timezone));
    let (period_start, period_end) = digest_utc_window(signal_date, &cli.daily_digest_timezone)?;
    let messages = load_messages_between(
        pool,
        &target_chat_id,
        period_start,
        period_end,
        options.limit,
    )
    .await?;
    let messages_scanned = messages.len() as i64;
    let candidates = messages
        .into_iter()
        .filter_map(classify_candidate)
        .collect::<Vec<_>>();
    let accepted = judge_candidates(&candidates);
    let mut report = EventSignalReport {
        dry_run: !apply,
        target_chat_id,
        signal_date,
        messages_scanned,
        candidate_count: candidates.len() as i64,
        candidates: candidates.iter().map(candidate_preview).collect(),
        accepted_event_count: accepted.len() as i64,
        accepted_events: accepted.iter().map(event_preview).collect(),
        candidates_written: 0,
        events_written: 0,
    };
    if apply {
        let candidate_ids =
            upsert_candidates(pool, &report.target_chat_id, signal_date, &candidates).await?;
        let accepted = attach_candidate_ids(accepted, &candidate_ids);
        report.candidates_written = candidate_ids.len() as i64;
        report.events_written =
            upsert_events(pool, &report.target_chat_id, signal_date, &accepted).await?;
        retire_stale_events(pool, &report.target_chat_id, signal_date, &accepted).await?;
    }
    Ok(report)
}

async fn load_messages_between(
    pool: &PgPool,
    chat_id: &str,
    period_start: DateTime<Utc>,
    period_end: DateTime<Utc>,
    limit: Option<i64>,
) -> Result<Vec<MessageRow>> {
    let limit = limit.unwrap_or(2000).max(1);
    let rows = sqlx::query(
        r#"
        SELECT
            m.id,
            m.sender_person_id,
            m.sender_channel_identity_id,
            m.sender_name,
            COALESCE(ci.is_bot, false) AS sender_is_bot,
            COALESCE(m.text, '') AS text,
            m.received_at
        FROM qintopia_messages.messages m
        LEFT JOIN qintopia_identity.channel_identities ci
          ON ci.id = m.sender_channel_identity_id
        WHERE m.platform = 'qiwe'
          AND m.chat_id = $1
          AND m.received_at >= $2
          AND m.received_at < $3
          AND COALESCE(m.text, '') <> ''
        ORDER BY m.received_at ASC
        LIMIT $4
        "#,
    )
    .bind(chat_id)
    .bind(period_start)
    .bind(period_end)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("load messages for event signals")?;
    rows.into_iter()
        .map(|row| {
            Ok(MessageRow {
                id: row.try_get("id")?,
                sender_person_id: row.try_get("sender_person_id")?,
                sender_channel_identity_id: row.try_get("sender_channel_identity_id")?,
                sender_name: row.try_get("sender_name")?,
                sender_is_bot: row.try_get("sender_is_bot")?,
                text: row.try_get("text")?,
                received_at: row.try_get("received_at")?,
            })
        })
        .collect()
}

fn classify_candidate(message: MessageRow) -> Option<Candidate> {
    let text = message.text.trim();
    if message.sender_is_bot || is_system_or_placeholder_message(text) || is_trivial_chat(text) {
        return None;
    }
    let mut labels = Vec::new();
    let mut score: f64 = 0.0;
    if is_activity_event(text) {
        labels.push("activity".to_string());
        score = score.max(0.9);
    }
    if is_service_facility_event(text) {
        labels.push("service".to_string());
        score = score.max(0.85);
    }
    if is_unanswered_question_candidate(text) {
        labels.push("question".to_string());
        score = score.max(0.68);
    }
    if is_faq_sop_candidate(text) {
        labels.push("faq_sop".to_string());
        score = score.max(0.78);
    }
    if is_content_lead(text) {
        labels.push("content".to_string());
        score = score.max(0.75);
    }
    if labels.is_empty() {
        return None;
    }
    labels.sort();
    labels.dedup();
    Some(Candidate {
        message,
        labels,
        score,
        filter_reason: "rule_prefilter".to_string(),
    })
}

fn judge_candidates(candidates: &[Candidate]) -> Vec<AcceptedEvent> {
    let mut events: Vec<AcceptedEvent> = Vec::new();
    let mut index_by_dedupe_key: HashMap<String, usize> = HashMap::new();
    for event in candidates.iter().filter_map(judge_candidate) {
        if let Some(index) = index_by_dedupe_key.get(&event.dedupe_key).copied() {
            merge_event(&mut events[index], event);
            continue;
        }
        index_by_dedupe_key.insert(event.dedupe_key.clone(), events.len());
        events.push(event);
    }
    events
}

fn merge_event(existing: &mut AcceptedEvent, incoming: AcceptedEvent) {
    for message_id in incoming.source_message_ids {
        if !existing.source_message_ids.contains(&message_id) {
            existing.source_message_ids.push(message_id);
        }
    }
    for candidate_id in incoming.source_candidate_ids {
        if !existing.source_candidate_ids.contains(&candidate_id) {
            existing.source_candidate_ids.push(candidate_id);
        }
    }
    for name in incoming.related_member_names {
        if !existing.related_member_names.contains(&name) {
            existing.related_member_names.push(name);
        }
    }
    existing.source_window_start =
        match (existing.source_window_start, incoming.source_window_start) {
            (Some(left), Some(right)) => Some(left.min(right)),
            (None, value) => value,
            (value, None) => value,
        };
    existing.source_window_end = match (existing.source_window_end, incoming.source_window_end) {
        (Some(left), Some(right)) => Some(left.max(right)),
        (None, value) => value,
        (value, None) => value,
    };
    existing.confidence = existing.confidence.max(incoming.confidence);
}

fn judge_candidate(candidate: &Candidate) -> Option<AcceptedEvent> {
    let text = candidate.message.text.trim();
    let sender = candidate
        .message
        .sender_name
        .clone()
        .unwrap_or_else(|| "成员".to_string());
    if candidate.labels.iter().any(|label| label == "activity") {
        return Some(event_from_candidate(
            candidate,
            "活动/聚会",
            &activity_title(text),
            &format!(
                "{sender} 发起或更新活动/聚会线索：{}",
                clean_snippet(text, 220)
            ),
            "小满",
            "xiaoman",
            "中",
            0.88,
            "规则识别为可复盘的活动或聚会线索。",
            "低",
            "未评估",
        ));
    }
    if candidate.labels.iter().any(|label| label == "service") {
        return Some(event_from_candidate(
            candidate,
            "服务/设施",
            &service_title(text),
            &format!(
                "{sender} 提出社区服务/设施相关需求：{}",
                clean_snippet(text, 220)
            ),
            "小管家",
            "xiaoguanjia",
            "高",
            0.84,
            "规则识别为公共空间、设施或服务需求，需要确认处理状态。",
            "低",
            "未评估",
        ));
    }
    if candidate.labels.iter().any(|label| label == "faq_sop") {
        return Some(event_from_candidate(
            candidate,
            "FAQ/SOP",
            &faq_title(text),
            &format!(
                "{sender} 的消息可作为 FAQ/SOP 候选：{}",
                clean_snippet(text, 220)
            ),
            "文渊阁",
            "wenyuange",
            "中",
            0.78,
            "规则识别为规则、流程、常见问题或活动模板沉淀候选。",
            "无",
            "未评估",
        ));
    }
    if candidate.labels.iter().any(|label| label == "content") {
        return Some(event_from_candidate(
            candidate,
            "内容线索",
            &content_title(text),
            &format!("{sender} 提供内容/素材线索：{}", clean_snippet(text, 220)),
            "画报司",
            "huabaosi",
            "中",
            0.76,
            "规则识别为可进入素材收集或内容规划的线索。",
            "低",
            "可规划",
        ));
    }
    if candidate.labels.iter().any(|label| label == "question") && is_actionable_question(text) {
        return Some(event_from_candidate(
            candidate,
            "未回答问题",
            &question_title(text),
            &format!(
                "{sender} 提出可能需要运营确认的问题：{}",
                clean_snippet(text, 220)
            ),
            "文渊阁",
            "wenyuange",
            "中",
            0.72,
            "规则识别为与社区运营、服务或活动相关的明确问题。",
            "无",
            "未评估",
        ));
    }
    None
}

#[expect(
    clippy::too_many_arguments,
    reason = "rule classification keeps each accepted event field explicit for review"
)]
fn event_from_candidate(
    candidate: &Candidate,
    signal_type: &str,
    title: &str,
    summary: &str,
    owner_name: &str,
    owner_agent: &str,
    priority: &str,
    confidence: f64,
    judge_reason: &str,
    risk_level: &str,
    external_publish_status: &str,
) -> AcceptedEvent {
    let related_member_names = candidate
        .message
        .sender_name
        .clone()
        .map(|name| vec![name])
        .unwrap_or_default();
    AcceptedEvent {
        signal_type: signal_type.to_string(),
        title: title.to_string(),
        summary: summary.to_string(),
        owner_name: owner_name.to_string(),
        owner_agent: owner_agent.to_string(),
        priority: priority.to_string(),
        confidence,
        source_candidate_ids: Vec::new(),
        source_message_ids: vec![candidate.message.id],
        source_window_start: Some(candidate.message.received_at),
        source_window_end: Some(candidate.message.received_at),
        related_member_names,
        dedupe_key: dedupe_key(signal_type, title, &[candidate.message.id]),
        judge_reason: judge_reason.to_string(),
        risk_level: risk_level.to_string(),
        external_publish_status: external_publish_status.to_string(),
    }
}

fn attach_candidate_ids(
    mut accepted: Vec<AcceptedEvent>,
    candidate_ids: &[(Uuid, Uuid)],
) -> Vec<AcceptedEvent> {
    for event in &mut accepted {
        let ids = event
            .source_message_ids
            .iter()
            .filter_map(|message_id| {
                candidate_ids
                    .iter()
                    .find(|(candidate_message_id, _)| candidate_message_id == message_id)
                    .map(|(_, candidate_id)| *candidate_id)
            })
            .collect::<Vec<_>>();
        event.source_candidate_ids = ids;
    }
    accepted
}

async fn upsert_candidates(
    pool: &PgPool,
    chat_id: &str,
    signal_date: NaiveDate,
    candidates: &[Candidate],
) -> Result<Vec<(Uuid, Uuid)>> {
    let mut out = Vec::new();
    for candidate in candidates {
        let row: (Uuid,) = sqlx::query_as(
            r#"
            INSERT INTO qintopia_agent_os.event_signal_candidates
                (
                    platform, chat_id, signal_date, source_message_id,
                    sender_person_id, sender_channel_identity_id, sender_name,
                    message_received_at, message_text, candidate_labels,
                    candidate_score, filter_reason, extraction_version,
                    status, judge_status, judge_model, judge_reason, metadata
                )
            VALUES
                (
                    'qiwe', $1, $2, $3, $4, $5, $6, $7, $8, $9,
                    $10, $11, $12, 'accepted', 'accepted', $13, $14, $15
                )
            ON CONFLICT (platform, chat_id, signal_date, source_message_id, extraction_version)
            DO UPDATE SET
                sender_person_id = EXCLUDED.sender_person_id,
                sender_channel_identity_id = EXCLUDED.sender_channel_identity_id,
                sender_name = EXCLUDED.sender_name,
                message_received_at = EXCLUDED.message_received_at,
                message_text = EXCLUDED.message_text,
                candidate_labels = EXCLUDED.candidate_labels,
                candidate_score = EXCLUDED.candidate_score,
                filter_reason = EXCLUDED.filter_reason,
                status = EXCLUDED.status,
                judge_status = EXCLUDED.judge_status,
                judge_model = EXCLUDED.judge_model,
                judge_reason = EXCLUDED.judge_reason,
                metadata = EXCLUDED.metadata,
                updated_at = now()
            RETURNING id
            "#,
        )
        .bind(chat_id)
        .bind(signal_date)
        .bind(candidate.message.id)
        .bind(candidate.message.sender_person_id)
        .bind(candidate.message.sender_channel_identity_id)
        .bind(&candidate.message.sender_name)
        .bind(candidate.message.received_at)
        .bind(&candidate.message.text)
        .bind(&candidate.labels)
        .bind(candidate.score)
        .bind(&candidate.filter_reason)
        .bind(EXTRACTION_VERSION)
        .bind(JUDGE_MODEL)
        .bind("rule accepted")
        .bind(json!({
            "message_text_hash": sha256_hex(&candidate.message.text),
            "source": "event_signal_v2_rule"
        }))
        .fetch_one(pool)
        .await
        .context("upsert event signal candidate")?;
        out.push((candidate.message.id, row.0));
    }
    Ok(out)
}

async fn upsert_events(
    pool: &PgPool,
    chat_id: &str,
    signal_date: NaiveDate,
    events: &[AcceptedEvent],
) -> Result<i64> {
    let mut written = 0;
    for event in events {
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.event_signals
                (
                    platform, chat_id, signal_date, signal_type, title, summary,
                    related_member_names, owner_name, owner_agent, priority, status,
                    confidence, source_candidate_ids, source_message_ids,
                    source_window_start, source_window_end, dedupe_key, judge_model,
                    judge_reason, extraction_version, risk_level, external_publish_status,
                    activity_phase, metadata
                )
            VALUES
                (
                    'qiwe', $1, $2, $3, $4, $5, $6, $7, $8, $9, '待处理',
                    $10, $11, $12, $13, $14, $15, $16, $17, $18, $19, $20, $21, $22
                )
            ON CONFLICT (platform, chat_id, signal_date, dedupe_key, extraction_version)
            DO UPDATE SET
                signal_type = EXCLUDED.signal_type,
                title = EXCLUDED.title,
                summary = EXCLUDED.summary,
                related_member_names = EXCLUDED.related_member_names,
                owner_name = EXCLUDED.owner_name,
                owner_agent = EXCLUDED.owner_agent,
                priority = EXCLUDED.priority,
                confidence = EXCLUDED.confidence,
                source_candidate_ids = EXCLUDED.source_candidate_ids,
                source_message_ids = EXCLUDED.source_message_ids,
                source_window_start = EXCLUDED.source_window_start,
                source_window_end = EXCLUDED.source_window_end,
                judge_model = EXCLUDED.judge_model,
                judge_reason = EXCLUDED.judge_reason,
                risk_level = EXCLUDED.risk_level,
                external_publish_status = EXCLUDED.external_publish_status,
                metadata = EXCLUDED.metadata,
                updated_at = now()
            "#,
        )
        .bind(chat_id)
        .bind(signal_date)
        .bind(&event.signal_type)
        .bind(&event.title)
        .bind(&event.summary)
        .bind(&event.related_member_names)
        .bind(&event.owner_name)
        .bind(&event.owner_agent)
        .bind(&event.priority)
        .bind(event.confidence)
        .bind(&event.source_candidate_ids)
        .bind(&event.source_message_ids)
        .bind(event.source_window_start)
        .bind(event.source_window_end)
        .bind(&event.dedupe_key)
        .bind(JUDGE_MODEL)
        .bind(&event.judge_reason)
        .bind(EXTRACTION_VERSION)
        .bind(&event.risk_level)
        .bind(&event.external_publish_status)
        .bind(initial_phase_for_signal(&event.signal_type).map(|phase| phase.as_str()))
        .bind(json!({"source": "event_signal_v2_rule"}))
        .execute(pool)
        .await
        .context("upsert event signal")?;
        written += 1;
    }
    Ok(written)
}

async fn retire_stale_events(
    pool: &PgPool,
    chat_id: &str,
    signal_date: NaiveDate,
    accepted_events: &[AcceptedEvent],
) -> Result<()> {
    let accepted_dedupe_keys = accepted_events
        .iter()
        .map(|event| event.dedupe_key.clone())
        .collect::<Vec<_>>();
    sqlx::query(
        r#"
        DELETE FROM qintopia_agent_os.event_signals
        WHERE platform = 'qiwe'
          AND chat_id = $1
          AND signal_date = $2
          AND extraction_version = $3
          AND NOT (dedupe_key = ANY($4))
          AND feishu_record_id IS NULL
          AND metadata->>'source' = 'event_signal_v2_rule'
        "#,
    )
    .bind(chat_id)
    .bind(signal_date)
    .bind(EXTRACTION_VERSION)
    .bind(&accepted_dedupe_keys)
    .execute(pool)
    .await
    .context("delete stale unpublished event signals")?;

    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.event_signals
        SET status = '已关闭',
            metadata = metadata || jsonb_build_object('stale_auto_retired', true),
            updated_at = now()
        WHERE platform = 'qiwe'
          AND chat_id = $1
          AND signal_date = $2
          AND extraction_version = $3
          AND NOT (dedupe_key = ANY($4))
          AND feishu_record_id IS NOT NULL
          AND metadata->>'source' = 'event_signal_v2_rule'
        "#,
    )
    .bind(chat_id)
    .bind(signal_date)
    .bind(EXTRACTION_VERSION)
    .bind(&accepted_dedupe_keys)
    .execute(pool)
    .await
    .context("close stale published event signals")?;
    Ok(())
}

pub(crate) async fn load_event_signals_for_digest(
    pool: &PgPool,
    chat_id: &str,
    signal_date: NaiveDate,
) -> Result<Vec<EventSignalRow>> {
    let rows = sqlx::query(
        r#"
        SELECT id, signal_type, title, summary, related_member_names, owner_name,
               owner_agent, priority, status, confidence, source_message_ids,
               source_window_start, source_window_end, dedupe_key, judge_model,
               judge_reason, risk_level, external_publish_status, feishu_record_id
        FROM qintopia_agent_os.event_signals
        WHERE platform = 'qiwe'
          AND chat_id = $1
          AND signal_date = $2
          AND status <> '已关闭'
        ORDER BY
          CASE priority WHEN '高' THEN 0 WHEN '中' THEN 1 ELSE 2 END,
          source_window_start ASC NULLS LAST,
          created_at ASC
        "#,
    )
    .bind(chat_id)
    .bind(signal_date)
    .fetch_all(pool)
    .await
    .context("load event signals for digest")?;
    rows.into_iter()
        .map(|row| {
            Ok(EventSignalRow {
                id: row.try_get("id")?,
                signal_type: row.try_get("signal_type")?,
                title: row.try_get("title")?,
                summary: row.try_get("summary")?,
                related_member_names: row.try_get("related_member_names")?,
                owner_name: row.try_get("owner_name")?,
                owner_agent: row.try_get("owner_agent")?,
                priority: row.try_get("priority")?,
                status: row.try_get("status")?,
                confidence: row.try_get("confidence")?,
                source_message_ids: row.try_get("source_message_ids")?,
                source_window_start: row.try_get("source_window_start")?,
                source_window_end: row.try_get("source_window_end")?,
                dedupe_key: row.try_get("dedupe_key")?,
                judge_model: row.try_get("judge_model")?,
                judge_reason: row.try_get("judge_reason")?,
                risk_level: row.try_get("risk_level")?,
                external_publish_status: row.try_get("external_publish_status")?,
                feishu_record_id: row.try_get("feishu_record_id")?,
            })
        })
        .collect()
}

fn candidate_preview(candidate: &Candidate) -> CandidatePreview {
    CandidatePreview {
        message_id: candidate.message.id,
        sender_name: candidate.message.sender_name.clone(),
        labels: candidate.labels.clone(),
        score: candidate.score,
        text: clean_snippet(&candidate.message.text, 160),
        filter_reason: candidate.filter_reason.clone(),
    }
}

fn event_preview(event: &AcceptedEvent) -> EventPreview {
    EventPreview {
        signal_type: event.signal_type.clone(),
        activity_phase: initial_phase_for_signal(&event.signal_type)
            .map(|phase| phase.as_str().to_string()),
        title: event.title.clone(),
        summary: event.summary.clone(),
        owner_agent: event.owner_agent.clone(),
        priority: event.priority.clone(),
        confidence: event.confidence,
        source_message_ids: event.source_message_ids.clone(),
        judge_reason: event.judge_reason.clone(),
    }
}

fn is_activity_event(text: &str) -> bool {
    is_solitaire_message(text)
        || contains_any(
            text,
            &[
                "报名",
                "接龙",
                "一起去",
                "组队",
                "分享活动",
                "瑜伽",
                "爬山",
                "Family Day",
                "聚会",
                "出发",
            ],
        ) && contains_any(
            text,
            &[
                "今天", "明天", "今晚", "早上", "下午", "晚上", "周末", "点", "活动",
            ],
        )
}

fn is_service_facility_event(text: &str) -> bool {
    if contains_any(text, &["找到了", "已解决", "解决了", "谢谢", "不用了"]) {
        return false;
    }
    if contains_any(
        text,
        &[
            "吃火锅",
            "洗好了",
            "肉马上就回来了",
            "锅底",
            "开吃",
            "买新鲜的肉",
        ],
    ) {
        return false;
    }
    contains_any(
        text,
        &[
            "吧台",
            "厨房",
            "房间",
            "卫生间",
            "门锁",
            "空调",
            "水电",
            "维修",
            "坏了",
            "整理",
            "打扫",
            "客服",
            "求助",
            "帮我",
        ],
    ) && contains_any(
        text,
        &[
            "社区", "公共", "吧台", "厨房", "房间", "整理", "维修", "坏了", "客服", "求助",
        ],
    )
}

fn is_unanswered_question_candidate(text: &str) -> bool {
    if text.chars().count() < 12 || is_casual_question(text) {
        return false;
    }
    contains_any(
        text,
        &[
            "有没有",
            "哪里",
            "什么时候",
            "能不能",
            "可以吗",
            "求推荐",
            "谁知道",
            "需要准备",
        ],
    )
}

fn is_actionable_question(text: &str) -> bool {
    contains_any(
        text,
        &[
            "社区", "活动", "报名", "出发", "准备", "推荐", "房", "餐", "菜", "车", "带", "书",
            "捡到", "归还",
        ],
    ) && !is_casual_question(text)
}

fn is_faq_sop_candidate(text: &str) -> bool {
    contains_any(
        text,
        &["规则", "流程", "SOP", "sop", "常见问题", "须知", "模板"],
    ) || (is_solitaire_message(text) && contains_any(text, &["活动", "接龙", "报名"]))
}

fn is_content_lead(text: &str) -> bool {
    if contains_any(text, &["有直播吗", "照片拍的挺好", "照片拍得挺好"]) {
        return false;
    }
    contains_any(
        text,
        &[
            "素材",
            "故事",
            "小红书",
            "公众号",
            "拍照",
            "海报",
            "视频",
            "直播",
            "照片",
        ],
    )
}

fn is_casual_question(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.chars().count() < 8
        || contains_any(
            trimmed,
            &["哈哈", "嘿哈", "破涕为笑", "粉色的吗", "噶蛋吗", "有直播吗"],
        )
}

fn is_trivial_chat(text: &str) -> bool {
    let trimmed = text.trim();
    trimmed.chars().count() <= 3
        || contains_any(trimmed, &["[动画表情]", "[表情]", "哈哈哈", "收到", "好的"])
}

fn is_system_or_placeholder_message(text: &str) -> bool {
    let text = text.trim();
    text.starts_with("[我发起了一笔群收款")
        || text.starts_with("[收到一条")
        || text.contains("该版本暂不支持查看")
}

fn is_solitaire_message(text: &str) -> bool {
    text.contains("#接龙")
}

fn activity_title(text: &str) -> String {
    let normalized = text.replace("#接龙", "");
    if normalized.contains("催眠活动") {
        return "团体催眠活动".to_string();
    }
    if normalized.contains("19:30") && normalized.contains("分享活动") {
        return "19:30 社群一楼分享活动".to_string();
    }
    if normalized.contains("Family Day") || normalized.contains("family day") {
        return "Family Day：石井镇大集 + 火锅 + 陶陶居喝茶".to_string();
    }
    if normalized.contains("瑜伽") {
        return "社区三楼瑜伽".to_string();
    }
    if normalized.contains("凤凰山") {
        return "涝峪凤凰山徒步".to_string();
    }
    if normalized.contains("晚上六点出去组队觅食") {
        return "晚上六点组队觅食".to_string();
    }
    if normalized.contains("晚饭组队") || normalized.contains("聚丰餐厅") {
        return "晚饭组队：聚丰餐厅".to_string();
    }
    if normalized.contains("陶陶居") && normalized.contains("喝茶") {
        return "陶陶居喝茶聊聊".to_string();
    }
    if normalized.contains("余下") && normalized.contains("买新鲜的肉") {
        return "余下买菜采购".to_string();
    }
    first_sentence_or_snippet(&normalized, 80)
}

fn service_title(text: &str) -> String {
    first_sentence_or_snippet(text, 60)
}

fn faq_title(text: &str) -> String {
    first_sentence_or_snippet(text, 60)
}

fn content_title(text: &str) -> String {
    first_sentence_or_snippet(text, 60)
}

fn question_title(text: &str) -> String {
    first_sentence_or_snippet(text, 60)
}

fn first_sentence_or_snippet(text: &str, max_chars: usize) -> String {
    let cleaned = clean_snippet(text, max_chars);
    cleaned
        .split(['。', '；', '\n'])
        .next()
        .filter(|value| !value.trim().is_empty())
        .unwrap_or(&cleaned)
        .chars()
        .take(max_chars)
        .collect()
}

fn clean_snippet(text: &str, max_chars: usize) -> String {
    text.split_whitespace()
        .collect::<Vec<_>>()
        .join(" ")
        .chars()
        .take(max_chars)
        .collect()
}

fn dedupe_key(signal_type: &str, title: &str, message_ids: &[Uuid]) -> String {
    let message_part = if signal_type == "活动/聚会" || signal_type == "FAQ/SOP" {
        "event-level".to_string()
    } else {
        message_ids
            .iter()
            .map(Uuid::to_string)
            .collect::<Vec<_>>()
            .join(",")
    };
    sha256_hex(&format!(
        "{signal_type}|{}|{message_part}",
        normalize_key(title)
    ))
}

fn normalize_key(value: &str) -> String {
    value
        .chars()
        .filter(|ch| !ch.is_whitespace())
        .collect::<String>()
        .to_lowercase()
}

fn contains_any(text: &str, needles: &[&str]) -> bool {
    needles.iter().any(|needle| text.contains(needle))
}

fn sha256_hex(value: &str) -> String {
    use sha2::{Digest, Sha256};
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn target_chat_ids(cli: &Cli, requested: Option<&str>) -> Result<Vec<String>> {
    let configured = cli.profile_target_chat_ids();
    if configured.is_empty() {
        bail!("QINTOPIA_PROFILE_TARGET_CHAT_IDS is required");
    }
    if let Some(chat_id) = requested {
        if !configured.iter().any(|item| item == chat_id) {
            bail!("chat_id {chat_id} is not in QINTOPIA_PROFILE_TARGET_CHAT_IDS");
        }
        return Ok(vec![chat_id.to_string()]);
    }
    Ok(configured)
}

fn default_signal_date(timezone: &str) -> NaiveDate {
    local_now_for_timezone(timezone).date_naive() - Duration::days(1)
}

fn digest_utc_window(
    digest_date: NaiveDate,
    timezone: &str,
) -> Result<(DateTime<Utc>, DateTime<Utc>)> {
    let local_start = digest_date
        .and_hms_opt(0, 0, 0)
        .context("invalid digest date")?;
    let offset = timezone_offset_seconds(timezone)?;
    let start =
        DateTime::<Utc>::from_naive_utc_and_offset(local_start - Duration::seconds(offset), Utc);
    Ok((start, start + Duration::days(1)))
}

fn local_now_for_timezone(timezone: &str) -> DateTime<Utc> {
    let now = Utc::now();
    match timezone {
        "Asia/Shanghai" | "UTC+8" | "+08:00" => now + Duration::hours(8),
        _ => now,
    }
}

fn timezone_offset_seconds(timezone: &str) -> Result<i64> {
    match timezone {
        "Asia/Shanghai" | "UTC+8" | "+08:00" => Ok(8 * 3600),
        "UTC" | "+00:00" => Ok(0),
        other => bail!("unsupported event signal timezone for V2: {other}"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn msg(text: &str) -> MessageRow {
        MessageRow {
            id: Uuid::new_v4(),
            sender_person_id: Some(Uuid::new_v4()),
            sender_channel_identity_id: Some(Uuid::new_v4()),
            sender_name: Some("成员".to_string()),
            sender_is_bot: false,
            text: text.to_string(),
            received_at: Utc::now(),
        }
    }

    #[test]
    fn filters_casual_short_questions() {
        assert!(classify_candidate(msg("粉色的吗")).is_none());
        assert!(classify_candidate(msg("噶蛋吗")).is_none());
    }

    #[test]
    fn accepts_service_facility_event() {
        let candidate =
            classify_candidate(msg("社区有没有特别擅长整理的人，可以帮我整理吧台吗")).unwrap();
        let event = judge_candidate(&candidate).unwrap();
        assert_eq!(event.signal_type, "服务/设施");
        assert_eq!(event.owner_agent, "xiaoguanjia");
    }

    #[test]
    fn filters_resolved_service_followup() {
        assert!(classify_candidate(msg("找到了@秦托邦小客服")).is_none());
    }

    #[test]
    fn filters_weak_content_comments() {
        assert!(classify_candidate(msg("有直播吗？想云上参与[嘿哈]")).is_none());
        assert!(classify_candidate(msg("照片拍的挺好")).is_none());
    }

    #[test]
    fn accepts_activity_event() {
        let candidate = classify_candidate(msg("明早九点社区三楼瑜伽，大家可以报名参加")).unwrap();
        let event = judge_candidate(&candidate).unwrap();
        assert_eq!(event.signal_type, "活动/聚会");
        assert_eq!(event.owner_agent, "xiaoman");
    }

    #[test]
    fn activity_dedupe_key_is_event_level() {
        let one = dedupe_key("活动/聚会", "社区三楼瑜伽", &[Uuid::new_v4()]);
        let two = dedupe_key("活动/聚会", "社区三楼瑜伽", &[Uuid::new_v4()]);
        assert_eq!(one, two);
    }

    #[test]
    fn aggregates_duplicate_activity_events() {
        let candidates = vec![
            classify_candidate(msg("明早九点社区三楼瑜伽，大家可以报名参加")).unwrap(),
            classify_candidate(msg("明早九点社区三楼瑜伽，大家可以报名参加 2. 新成员")).unwrap(),
        ];
        let events = judge_candidates(&candidates);
        assert_eq!(events.len(), 1);
        assert_eq!(events[0].signal_type, "活动/聚会");
        assert_eq!(events[0].source_message_ids.len(), 2);
    }

    #[test]
    fn normalizes_dinner_activity_title() {
        assert_eq!(
            activity_title(
                "晚上六点出去组队觅食，寻求老朋友帮助选店，人数不限！ 1. 喵喵的喵 2. 殊途"
            ),
            "晚上六点组队觅食"
        );
        assert_eq!(
            activity_title("晚饭组队 随机打卡聚丰餐厅[机智] 接龙 1. 创业者-可总"),
            "晚饭组队：聚丰餐厅"
        );
    }

    #[test]
    fn does_not_treat_meal_preparation_as_facility_issue() {
        assert!(classify_candidate(msg(
            "吃火锅的小伙伴可以带着自己的蔬菜去厨房洗好了肉马上就回来了"
        ))
        .is_none());
    }
}
