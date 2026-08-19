//! Xiaoman daily case-report cutover (PR 6 of the Rust migration).
//!
//! Wires the PR 2-5 Rust modules into a single production pipeline:
//! collect -> analyze -> narrative (best-effort) -> render -> rasterize -> upload -> publish.
//! The Python workflow remains available as a fallback via
//! `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_USE_PYTHON_PIPELINE=1`.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};
use std::time::Duration;

use anyhow::{bail, Context, Result};
use chrono::{DateTime, FixedOffset, NaiveDate, Utc};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::PgPool;

use crate::config::Cli;
use crate::daily_case_report::{
    collect_for_report, CharacterMemoryRow, CreativeProfileRow, MessageRow,
};
use crate::daily_case_report_analyze::{
    analyze, AnalyzeInput, CharacterMemoryInput, CreativeProfileInput, InputMessage,
};
use crate::daily_case_report_narrative::{generate_narrative, NarrativeConfig, NarrativeReport};
use crate::daily_case_report_render::{assembly, render, RenderInput, ReportData};
use crate::operations::{
    create_daily_case_report_auto_publish, daily_case_report_media_upload,
    DailyCaseReportAutoPublishCreateRequest, DailyCaseReportMediaUploadRequest,
    DailyCaseReportStorageBackend,
};

const DEFAULT_GROUP_NAME: &str = "秦托邦的小伙伴（新）";
const DEFAULT_REPORT_TITLE: &str = "小满群聊日报";
const DEFAULT_TEMPLATE: &str = "roast-long-image";
const DEFAULT_IMAGE_FORMAT: &str = "jpeg";
const DEFAULT_TIMEZONE: &str = "Asia/Shanghai";
const DEFAULT_TIMEZONE_OFFSET_SECONDS: i32 = 8 * 3600;
const DEFAULT_OUTPUT_WIDTH: usize = 1080;
const DEFAULT_NARRATIVE_STYLE: &str = "roast";
const MAX_RASTERIZE_OUTPUT_BYTES: usize = 8 * 1024 * 1024;
const RASTERIZE_TIMEOUT_SECONDS: u64 = 300;

const MEMORY_FACT_ROLE_LABELS: &[(&str, &str)] = &[
    ("activity_organizer", "活动推进者"),
    ("activity_participation", "活动在场者"),
    ("content_story_lead", "故事线雷达"),
    ("operation_signal", "规则观察员"),
    ("resource_scout", "资料投喂员"),
    ("service_need", "需求提醒人"),
    ("unresolved_question", "问题发射台"),
];

/// Runtime options for one pipeline run.
#[derive(Debug, Clone)]
pub struct PipelineOptions {
    pub chat_id: String,
    pub date: Option<String>,
    pub template: String,
    pub narrative_style: String,
    pub output_dir: PathBuf,
    pub apply: bool,
    pub group_name: Option<String>,
    pub report_title: Option<String>,
    pub width: usize,
}

impl PipelineOptions {
    pub fn from_env() -> Result<Self> {
        let chat_id = std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID")
            .context("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID is required")?;
        let date = std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_DATE").ok();
        if let Some(date) = &date {
            NaiveDate::parse_from_str(date, "%Y-%m-%d")
                .with_context(|| format!("invalid backfill date: {date}"))?;
            if std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_BACKFILL_APPROVAL").as_deref()
                != Ok("approved-production-xiaoman-daily-case-report-auto-publish-backfill")
            {
                bail!("date override requires approved-production-xiaoman-daily-case-report-auto-publish-backfill");
            }
        }
        Ok(Self {
            chat_id,
            date,
            template: std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TEMPLATE")
                .unwrap_or_else(|_| DEFAULT_TEMPLATE.to_string()),
            narrative_style: std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_NARRATIVE")
                .unwrap_or_else(|_| DEFAULT_NARRATIVE_STYLE.to_string()),
            output_dir: std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_OUTPUT_DIR")
                .map(PathBuf::from)
                .unwrap_or_else(|_| {
                    PathBuf::from(
                        "/home/ubuntu/.local/state/qintopia-agentos/xiaoman-daily-case-report",
                    )
                }),
            apply: false,
            group_name: std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_GROUP_NAME").ok(),
            report_title: std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_REPORT_TITLE").ok(),
            width: std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_OUTPUT_WIDTH")
                .ok()
                .and_then(|value| value.parse().ok())
                .unwrap_or(DEFAULT_OUTPUT_WIDTH),
        })
    }
}

/// Resolve a script path relative to the release root that owns the sidecar binary.
pub fn resolve_release_path(script: &Path) -> PathBuf {
    if script.is_absolute() {
        return script.to_path_buf();
    }
    if let Some(release_current) = std::env::var_os("QINTOPIA_AGENT_OS_RELEASE_CURRENT") {
        let candidate = PathBuf::from(release_current).join(script);
        if candidate.is_file() {
            return candidate;
        }
    }
    if let Ok(current_exe) = std::env::current_exe() {
        if let Some(sidecar_dir) = current_exe.parent() {
            if let Some(release_dir) = sidecar_dir.parent() {
                let candidate = release_dir.join(script);
                if candidate.is_file() {
                    return candidate;
                }
            }
        }
    }
    script.to_path_buf()
}

fn report_timezone_offset() -> FixedOffset {
    FixedOffset::east_opt(DEFAULT_TIMEZONE_OFFSET_SECONDS).expect("valid +08:00 offset")
}

/// Return (start, end, display_date) for the report window.
fn resolve_window(
    date_override: Option<&str>,
) -> Result<(DateTime<FixedOffset>, DateTime<FixedOffset>, String)> {
    let tz = report_timezone_offset();
    if let Some(date_str) = date_override {
        let base = NaiveDate::parse_from_str(date_str, "%Y-%m-%d")
            .with_context(|| format!("date must be YYYY-MM-DD, got {date_str}"))?;
        let start = base
            .and_hms_opt(0, 0, 0)
            .context("invalid start time")?
            .and_local_timezone(tz)
            .single()
            .context("ambiguous timezone start")?;
        let end = start + chrono::Duration::days(1);
        let display = start.format("%Y年%m月%d日").to_string();
        return Ok((start, end, display));
    }

    let now = Utc::now().with_timezone(&tz);
    let report_day = (now - chrono::Duration::days(1)).date_naive();
    let start = report_day
        .and_hms_opt(0, 0, 0)
        .context("invalid start time")?
        .and_local_timezone(tz)
        .single()
        .context("ambiguous timezone start")?;
    let end = start + chrono::Duration::days(1);
    let display = start.format("%Y年%m月%d日").to_string();
    Ok((start, end, display))
}

fn time_range_label(start: DateTime<FixedOffset>, end: DateTime<FixedOffset>) -> String {
    let end_display = end - chrono::Duration::seconds(1);
    if start.date_naive() == end_display.date_naive() {
        format!("{}–{}", start.format("%H:%M"), end_display.format("%H:%M"))
    } else {
        format!(
            "{} {}–{} {}",
            start.format("%m/%d"),
            start.format("%H:%M"),
            end_display.format("%m/%d"),
            end_display.format("%H:%M")
        )
    }
}

fn sha256_str(value: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(value.as_bytes());
    format!("{:x}", hasher.finalize())
}

fn source_chat_ref(chat_id: &str) -> Value {
    if chat_id.trim().is_empty() {
        return Value::Null;
    }
    json!({
        "kind": "sha256",
        "value": format!("sha256:{}", sha256_str(chat_id)),
    })
}

fn input_messages_from_rows(rows: &[MessageRow]) -> Vec<InputMessage> {
    rows.iter()
        .map(|row| InputMessage {
            id: row.id.clone(),
            sender_id: row.sender_id.clone(),
            sender_name: if row.sender_name.trim().is_empty() {
                "匿名".to_string()
            } else {
                row.sender_name.clone()
            },
            text: row.text.clone(),
            sent_at: Some(row.report_time),
            message_kind: "text".to_string(),
            person_id: row.sender_person_id.clone(),
        })
        .collect()
}

fn dominant_role_label(fact_type: &str) -> String {
    MEMORY_FACT_ROLE_LABELS
        .iter()
        .find(|(key, _)| *key == fact_type)
        .map(|(_, label)| label.to_string())
        .unwrap_or_else(|| "长期在场者".to_string())
}

fn character_memory_inputs_from_rows(
    rows: &[CharacterMemoryRow],
) -> HashMap<String, CharacterMemoryInput> {
    rows.iter()
        .map(|row| {
            (
                row.person_id.clone(),
                CharacterMemoryInput {
                    person_id: row.person_id.clone(),
                    recent_fact_count: row.recent_fact_count,
                    lifetime_fact_count: row.lifetime_fact_count,
                    dominant_role_label: dominant_role_label(&row.dominant_fact_type),
                    recurrence_label: String::new(),
                    depth_label: String::new(),
                    memory_weight_label: String::new(),
                    callback_seed: String::new(),
                },
            )
        })
        .collect()
}

fn safe_creative_text(value: &Value, limit: usize) -> String {
    let text = value.as_str().unwrap_or_default();
    let cleaned = text
        .chars()
        .filter(|c| !matches!(c, '`' | '$' | '<' | '>' | '{' | '}' | '\x00'..='\x08' | '\x0b' | '\x0c' | '\x0e'..='\x1f' | '\x7f'))
        .collect::<String>()
        .trim()
        .to_string();
    let lowered = cleaned.to_lowercase();
    if ["raw_message", "fact_text", "profile_text", "database_url"]
        .iter()
        .any(|marker| lowered.contains(marker))
    {
        return String::new();
    }
    cleaned.chars().take(limit).collect()
}

fn reviewed_public_expressive_label(safe_reply_hints: &Value) -> String {
    let labels = safe_reply_hints.get("public_expressive_labels");
    let labels = match labels {
        Some(Value::Object(map)) => map,
        _ => return String::new(),
    };
    if labels
        .get("public_surface_allowed")
        .and_then(Value::as_bool)
        != Some(true)
    {
        return String::new();
    }
    let status = labels
        .get("review_status")
        .and_then(Value::as_str)
        .unwrap_or_default();
    if !matches!(status, "reviewed" | "approved") {
        return String::new();
    }
    safe_creative_text(
        labels
            .get("relationship_tension")
            .or_else(|| labels.get("callback_label"))
            .or_else(|| labels.get("roast_label"))
            .unwrap_or(&Value::Null),
        48,
    )
}

fn safe_creative_int(value: &Value) -> i32 {
    value
        .as_i64()
        .map(|value| value.clamp(0, 1000) as i32)
        .unwrap_or(0)
}

fn creative_memory_inputs_from_rows(
    rows: &[CreativeProfileRow],
) -> HashMap<String, CreativeProfileInput> {
    rows.iter()
        .filter_map(|row| {
            let role_label = safe_creative_text(
                &row.safe_reply_hints
                    .get("role_label")
                    .or_else(|| row.communication_style.get("role_label"))
                    .cloned()
                    .unwrap_or_default(),
                32,
            );
            if role_label.is_empty() {
                return None;
            }
            Some((
                row.person_id.clone(),
                CreativeProfileInput {
                    person_id: row.person_id.clone(),
                    role_label,
                    story_function: safe_creative_text(
                        &row.safe_reply_hints
                            .get("story_function")
                            .or_else(|| row.communication_style.get("story_function"))
                            .cloned()
                            .unwrap_or_default(),
                        48,
                    ),
                    daily_arc: safe_creative_text(
                        &row.safe_reply_hints
                            .get("daily_arc")
                            .cloned()
                            .unwrap_or_default(),
                        120,
                    ),
                    memory_weight_label: safe_creative_text(
                        &row.safe_reply_hints
                            .get("memory_weight_label")
                            .cloned()
                            .unwrap_or_default(),
                        64,
                    ),
                    meme_seed: safe_creative_text(
                        &row.safe_reply_hints
                            .get("meme_seed")
                            .cloned()
                            .unwrap_or_default(),
                        80,
                    ),
                    callback_hint: safe_creative_text(
                        &row.safe_reply_hints
                            .get("callback_hint")
                            .cloned()
                            .unwrap_or_default(),
                        120,
                    ),
                    expressive_label: reviewed_public_expressive_label(&row.safe_reply_hints),
                    evidence_anchor: safe_creative_text(
                        &row.safe_reply_hints
                            .get("evidence_anchor")
                            .cloned()
                            .unwrap_or_default(),
                        80,
                    ),
                    recurrence_evidence_count: safe_creative_int(
                        &row.safe_reply_hints
                            .get("recurrence_evidence_count")
                            .cloned()
                            .unwrap_or_default(),
                    ),
                },
            ))
        })
        .collect()
}

fn build_analyze_input(
    messages: &[MessageRow],
    character_rows: &[CharacterMemoryRow],
    creative_rows: &[CreativeProfileRow],
    start: DateTime<Utc>,
) -> AnalyzeInput {
    AnalyzeInput {
        messages: input_messages_from_rows(messages),
        character_memory_by_person: character_memory_inputs_from_rows(character_rows),
        creative_memory_by_person: creative_memory_inputs_from_rows(creative_rows),
        start: Some(start),
    }
}

fn build_report_data(
    analyze_report: &crate::daily_case_report_analyze::AnalyzeReport,
    group_name: &str,
    report_title: &str,
    report_date: &str,
    time_range: &str,
    window_start: DateTime<FixedOffset>,
    window_end: DateTime<FixedOffset>,
) -> Result<ReportData> {
    let mut report = ReportData {
        group_name: group_name.to_string(),
        report_title: report_title.to_string(),
        report_date: report_date.to_string(),
        time_range: time_range.to_string(),
        member_count: analyze_report.participant_count,
        message_count: analyze_report.message_count,
        participant_count: analyze_report.participant_count,
        case_count: analyze_report.case_count,
        suspect_count: analyze_report.suspect_count,
        character_count: analyze_report.character_count,
        hourly_counts: analyze_report.hourly_counts.clone(),
        cases: analyze_report.cases.clone(),
        suspects: analyze_report.suspects.clone(),
        characters: analyze_report.characters.clone(),
        hot_topics: analyze_report.hot_topics.clone(),
        highlight: analyze_report.highlight.clone(),
        character_universe: Value::Null,
        window_start: window_start.to_rfc3339(),
        window_end: window_end.to_rfc3339(),
        timezone: DEFAULT_TIMEZONE.to_string(),
    };
    report.character_universe = assembly::build_character_universe(&report)?;
    Ok(report)
}

fn build_narrative_report(report: &ReportData, messages: &[InputMessage]) -> NarrativeReport {
    NarrativeReport {
        group_name: report.group_name.clone(),
        report_date: report.report_date.clone(),
        time_range: report.time_range.clone(),
        message_count: report.message_count,
        participant_count: report.participant_count,
        cases: report
            .cases
            .iter()
            .map(|case| crate::daily_case_report_narrative::NarrativeCase {
                case_no: case.case_no.clone(),
                title: case.title.clone(),
                time_label: case.time_label.clone(),
                summary: case.summary.clone(),
                message_count: case.message_count,
                participant_count: case.participant_count,
                top_speaker: case.top_speaker.clone(),
                bullets: case.bullets.clone(),
            })
            .collect(),
        characters: report
            .characters
            .iter()
            .map(
                |character| crate::daily_case_report_narrative::NarrativeCharacter {
                    name: character.name.clone(),
                    role_label: character.role_label.clone(),
                    one_liner: character.one_liner.clone(),
                    story_function: character.story_function.clone(),
                    evidence: character.evidence.clone(),
                },
            )
            .collect(),
        hot_topics: report
            .hot_topics
            .iter()
            .map(
                |topic| crate::daily_case_report_narrative::NarrativeHotTopic {
                    keyword: topic.keyword.clone(),
                    message_count: topic.message_count,
                    participant_count: topic.participant_count,
                },
            )
            .collect(),
        messages: messages
            .iter()
            .map(
                |message| crate::daily_case_report_narrative::NarrativeMessage {
                    sender_name: message.sender_name.clone(),
                    text: message.text.clone(),
                },
            )
            .collect(),
    }
}

fn image_mime_type(image_format: &str) -> String {
    if image_format == "jpeg" {
        "image/jpeg".to_string()
    } else {
        "image/png".to_string()
    }
}

fn image_extension(image_format: &str) -> String {
    if image_format == "jpeg" {
        "jpg".to_string()
    } else {
        "png".to_string()
    }
}

fn template_version(template: &str) -> String {
    match template {
        "v3" => "xiaoman-daily-case-report-v3".to_string(),
        "newspaper-elegant" | "newspaper" => "xiaoman-daily-case-report-v4-newspaper".to_string(),
        _ => "xiaoman-daily-case-report-v5-roast-long-image".to_string(),
    }
}

fn private_review_bundle() -> Value {
    json!({
        "schema_version": "xiaoman-daily-private-review-bundle-v1",
        "source": "wx_cli_style_daily_migration",
        "public_surface_allowed": false,
        "review_required": true,
        "raw_message_rows_included": false,
        "profile_fact_text_included": false,
        "raw_message_payload_read": false,
        "attachment_public_surface_allowed": false,
        "quote_map_entry_count": 0,
        "wiki_counts": {},
        "draft_counts": {},
    })
}

fn public_output_style_contract() -> Value {
    json!({
        "schema_version": "xiaoman-daily-public-output-style-v1",
        "source": "wx_cli_style_daily_migration",
        "character_daily_layout": true,
        "storyline_first": true,
        "cast_notes_enabled": true,
        "meme_callback_section_enabled": true,
        "relationship_section_enabled": true,
        "owner_reviewed_expressive_labels_only": true,
        "image_first_delivery": true,
        "pdf_default_delivery": false,
        "roast_review_boundary": true,
        "private_draft_only": true,
        "public_surface_contains_private_draft": false,
    })
}

fn character_universe_summary(universe: &Value) -> Value {
    let creative_universe_candidates = universe.get("creative_universe_candidates");
    let expressive_label_candidates = universe
        .get("expressive_label_candidates")
        .and_then(Value::as_array)
        .cloned()
        .unwrap_or_default();
    json!({
        "schema_version": universe.get("schema_version").cloned().unwrap_or_default(),
        "source": universe.get("source").cloned().unwrap_or_default(),
        "retained_source_policy": universe.get("retained_source_policy").cloned().unwrap_or_default(),
        "raw_messages_included": universe.get("raw_messages_included").and_then(Value::as_bool) == Some(true),
        "profile_fact_text_included": universe.get("profile_fact_text_included").and_then(Value::as_bool) == Some(true),
        "people_count": universe.get("people").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "topic_count": universe.get("topics").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "event_count": universe.get("events").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "meme_count": universe.get("memes").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "callback_count": universe.get("callbacks").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "relationship_count": universe.get("relationships").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "expressive_label_candidate_count": expressive_label_candidates.len(),
        "reviewed_public_expressive_label_count": expressive_label_candidates.iter().filter(|item| {
            item.get("public_surface_allowed").and_then(Value::as_bool) == Some(true)
                && item.get("review_status").and_then(Value::as_str) == Some("reviewed")
        }).count(),
        "creative_profile_candidate_count": universe.get("creative_profile_candidates").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "creative_profile_public_surface_allowed": creative_universe_candidates
            .and_then(|value| value.get("public_surface_allowed"))
            .and_then(Value::as_bool) == Some(true),
        "creative_universe_candidate_count": creative_universe_candidates
            .and_then(|value| value.get("candidate_count"))
            .and_then(Value::as_i64)
            .unwrap_or(0),
        "creative_universe_public_surface_allowed": creative_universe_candidates
            .and_then(|value| value.get("public_surface_allowed"))
            .and_then(Value::as_bool) == Some(true),
        "unreviewed_expressive_labels_public_surface_allowed": expressive_label_candidates.iter().any(|item| {
            item.get("public_surface_allowed").and_then(Value::as_bool) == Some(true)
                && item.get("review_status").and_then(Value::as_str) != Some("reviewed")
        }),
        "storyline_candidate_count": universe.get("storyline_candidates").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
        "edge_count": universe.get("edges").and_then(Value::as_array).map(Vec::len).unwrap_or(0),
    })
}

fn build_render_summary(
    report: &ReportData,
    image_path: &Path,
    rasterize_output: &RasterizeOutput,
    template: &str,
    chat_id: &str,
) -> Value {
    let image_format = &rasterize_output.image_format;
    let mime_type = image_mime_type(image_format);
    let content_metrics = json!({
        "message_count": report.message_count,
        "participant_count": report.participant_count,
        "case_count": report.case_count,
        "character_count": report.character_count,
        "hot_topic_count": report.hot_topics.len(),
    });
    let universe_summary = character_universe_summary(&report.character_universe);
    json!({
        "success": true,
        "skill": "xiaoman_daily_case_report",
        "external_send_executed": false,
        "requires_human_confirmation": false,
        "auto_publish_ready": false,
        "group_name": report.group_name,
        "report_date": report.report_date,
        "time_range": report.time_range,
        "message_count": report.message_count,
        "participant_count": report.participant_count,
        "case_count": report.case_count,
        "character_count": report.character_count,
        "suspect_count": report.suspect_count,
        "deliverable_path": image_path.to_string_lossy(),
        "image_path": image_path.to_string_lossy(),
        "image_format": image_format,
        "image_mime_type": mime_type,
        "png_path": if image_format == "png" { Some(image_path.to_string_lossy().to_string()) } else { None },
        "html_path": null,
        "daily_report_markdown_path": null,
        "character_universe_path": null,
        "quote_map_path": null,
        "wiki_bundle_path": null,
        "draft_bundle_path": null,
        "run_manifest_path": null,
        "review_report_path": null,
        "creative_profile_review_payload_path": null,
        "public_output_style": public_output_style_contract(),
        "character_universe": report.character_universe,
        "character_universe_summary": universe_summary,
        "quote_map": {},
        "wiki_bundle": { "counts": {} },
        "draft_bundle": { "counts": {} },
        "run_manifest": {},
        "private_review_bundle": private_review_bundle(),
        "artifact_candidate": {
            "artifact_type": "generated_image",
            "workflow_type": "daily_case_report",
            "template_version": template_version(template),
            "mime_type": mime_type,
            "filename": image_path.file_name().and_then(|value| value.to_str()).unwrap_or("xiaoman-daily-case-report.jpg"),
            "content_hash": format!("sha256:{}", rasterize_output.sha256),
            "file_md5": rasterize_output.md5,
            "byte_size": rasterize_output.byte_size,
            "render": {
                "image_format": image_format,
                "width": rasterize_output.width,
                "jpeg_quality": if image_format == "jpeg" { json!(92) } else { Value::Null },
            },
            "report_window": {
                "start": report.window_start,
                "end": report.window_end,
                "display": report.report_date,
                "time_range": report.time_range,
                "timezone": report.timezone,
            },
            "content_metrics": content_metrics,
            "source_chat_ref": source_chat_ref(chat_id),
            "retained_source_policy": "sanitized_metadata_only",
        },
    })
}

#[derive(Debug, Clone)]
#[allow(dead_code)]
pub struct RasterizeOutput {
    pub image_path: PathBuf,
    pub image_format: String,
    pub mime_type: String,
    pub byte_size: usize,
    pub width: usize,
    pub height: usize,
    pub sha256: String,
    pub md5: String,
}

fn md5_hex(bytes: &[u8]) -> String {
    use md5::Digest as _;
    format!("{:x}", md5::Md5::digest(bytes))
}

fn sha256_hex(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// Write the rendered HTML to a private temp directory and invoke the Python
/// rasterizer as a bounded subprocess.
pub async fn rasterize_html(
    rasterize_py: &Path,
    html: &str,
    output_dir: &Path,
    template: &str,
    width: usize,
    image_format: &str,
) -> Result<(PathBuf, RasterizeOutput)> {
    if !rasterize_py.is_file() {
        bail!("rasterize script is missing: {}", rasterize_py.display());
    }

    let tmp = tempfile::Builder::new()
        .prefix("xiaoman-daily-case-report-rasterize-")
        .tempdir_in(output_dir)
        .context("create rasterize temp dir")?;
    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        std::fs::set_permissions(tmp.path(), std::fs::Permissions::from_mode(0o700))
            .with_context(|| format!("chmod 0700 rasterize dir {}", tmp.path().display()))?;
    }

    let html_path = tmp.path().join("daily-report.html");
    let output_path = tmp
        .path()
        .join(format!("daily-report.{}", image_extension(image_format)));
    std::fs::write(&html_path, html).context("write rasterize HTML")?;

    let request = json!({
        "html_path": html_path.to_string_lossy(),
        "output_path": output_path.to_string_lossy(),
        "width": width,
        "image_format": image_format,
        "quality": 92,
    });

    let script = rasterize_py.to_path_buf();
    let request_json = request.to_string();
    let output_dir_owned = output_dir.to_path_buf();
    let _image_format_owned = image_format.to_string();
    let template_owned = template.to_string();
    let output_path_owned = output_path.clone();

    let result = tokio::task::spawn_blocking(move || {
        let mut command = Command::new("python3");
        command
            .arg(&script)
            .arg(&template_owned)
            .stdin(Stdio::piped())
            .stdout(Stdio::piped())
            .stderr(Stdio::piped())
            .current_dir(&output_dir_owned);
        let mut child = command.spawn().context("spawn rasterize subprocess")?;
        if let Some(mut stdin) = child.stdin.take() {
            use std::io::Write;
            stdin
                .write_all(request_json.as_bytes())
                .context("write rasterize request")?;
        }
        let output = child
            .wait_with_output()
            .context("wait for rasterize subprocess")?;
        if output.stdout.len() > MAX_RASTERIZE_OUTPUT_BYTES {
            bail!("rasterize output exceeded safety limit");
        }
        Ok::<_, anyhow::Error>(output)
    })
    .await
    .context("rasterize task join")?;

    let output = tokio::time::timeout(Duration::from_secs(RASTERIZE_TIMEOUT_SECONDS), async {
        result
    })
    .await
    .context("rasterize timed out")?;

    let output = output.context("rasterize subprocess")?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if !output.status.success() {
        bail!(
            "rasterize failed (status {}): {}",
            output.status,
            stderr.trim()
        );
    }

    let parsed: Value = serde_json::from_slice(&output.stdout).context("parse rasterize JSON")?;
    if parsed.get("success").and_then(Value::as_bool) != Some(true) {
        bail!(
            "rasterize reported failure: {}",
            parsed
                .get("error")
                .and_then(Value::as_str)
                .unwrap_or("unknown")
        );
    }

    let produced_path = parsed
        .get("image_path")
        .and_then(Value::as_str)
        .map(PathBuf::from)
        .context("rasterize did not return image_path")?;
    if produced_path != output_path_owned {
        bail!("rasterize returned an unexpected image path");
    }

    let bytes = std::fs::read(&produced_path).context("read rasterized image")?;
    let byte_size = bytes.len();

    let final_image_path =
        output_dir.join(format!("daily-report.{}", image_extension(image_format)));
    std::fs::copy(&produced_path, &final_image_path)
        .with_context(|| format!("copy rasterized image to {}", final_image_path.display()))?;

    let width = parsed
        .get("width")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(width);
    let height = parsed
        .get("height")
        .and_then(Value::as_u64)
        .map(|value| value as usize)
        .unwrap_or(0);
    let image_format = parsed
        .get("image_format")
        .and_then(Value::as_str)
        .unwrap_or(image_format)
        .to_string();
    let mime_type = parsed
        .get("mime_type")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| image_mime_type(&image_format));

    Ok((
        final_image_path.clone(),
        RasterizeOutput {
            image_path: final_image_path,
            image_format,
            mime_type,
            byte_size,
            width,
            height,
            sha256: sha256_hex(&bytes),
            md5: md5_hex(&bytes),
        },
    ))
}

fn media_upload_request_from_summary(
    render: &Value,
    image_path: &Path,
) -> Result<DailyCaseReportMediaUploadRequest> {
    let candidate = render
        .get("artifact_candidate")
        .cloned()
        .unwrap_or_default();
    Ok(DailyCaseReportMediaUploadRequest {
        image_path: image_path.to_path_buf(),
        content_hash: candidate
            .get("content_hash")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        file_md5: candidate
            .get("file_md5")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        byte_size: candidate
            .get("byte_size")
            .and_then(Value::as_u64)
            .map(|value| value as usize),
        filename: candidate
            .get("filename")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        report_window: candidate.get("report_window").cloned().unwrap_or_default(),
        source_chat_ref: candidate
            .get("source_chat_ref")
            .cloned()
            .unwrap_or_default(),
        template_version: candidate
            .get("template_version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        metadata: json!({"created_by": "daily_case_report_cutover"}),
    })
}

/// Dynamic one-line chat intro delivered before the report image so the group
/// knows what the image is before opening it. Mirrors the pre-cutover Python
/// `_default_intro_text`; operators may still override the whole line with
/// `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MESSAGE_TEXT`.
fn default_intro_text(render: &Value) -> String {
    let report_date = render
        .get("report_date")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let group_name = render
        .get("group_name")
        .and_then(Value::as_str)
        .unwrap_or_default()
        .trim();
    let message_count = render
        .get("message_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let participant_count = render
        .get("participant_count")
        .and_then(Value::as_u64)
        .unwrap_or(0);
    let date_part = if report_date.is_empty() {
        "昨天".to_string()
    } else {
        report_date.to_string()
    };
    let group_part = if group_name.is_empty() {
        "咱们群".to_string()
    } else {
        format!("「{group_name}」")
    };
    format!(
        "小满日报来啦 📰 {date_part} {group_part}的群聊，共 {message_count} 条消息、{participant_count} 位邻居发言。昨天的新鲜事都在下面这张长图里，点开看看 👇"
    )
}

fn auto_publish_request_from_summary(
    render: &Value,
    artifact_uri: &str,
    evidence: &crate::operations::DailyCaseReportMediaUploadEvidence,
) -> Result<DailyCaseReportAutoPublishCreateRequest> {
    let candidate = render
        .get("artifact_candidate")
        .cloned()
        .unwrap_or_default();
    let window = candidate.get("report_window").cloned().unwrap_or_default();
    let report_date = window
        .get("display")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            render
                .get("report_date")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
    let time_range = window
        .get("time_range")
        .and_then(Value::as_str)
        .map(str::to_string)
        .unwrap_or_else(|| {
            render
                .get("time_range")
                .and_then(Value::as_str)
                .unwrap_or_default()
                .to_string()
        });
    let target_group_id = std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID")
        .context("auto-publish requires QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID")?;
    if target_group_id.trim().is_empty() {
        bail!("auto-publish target group id is empty");
    }

    Ok(DailyCaseReportAutoPublishCreateRequest {
        window_start: window
            .get("start")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        window_end: window
            .get("end")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        report_date,
        time_range,
        artifact_uri: artifact_uri.to_string(),
        content_hash: evidence.content_hash.clone(),
        file_md5: evidence.file_md5.clone(),
        byte_size: evidence.byte_size,
        mime_type: evidence.mime_type.clone(),
        width: evidence.width,
        height: evidence.height,
        filename: evidence.filename.clone(),
        target_group_id,
        message_text: std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MESSAGE_TEXT")
            .unwrap_or_else(|_| default_intro_text(render)),
        title: render
            .get("report_date")
            .and_then(Value::as_str)
            .map(|date| format!("小满日报 {}", date))
            .unwrap_or_else(|| "小满日报".to_string()),
        summary: format!(
            "消息 {} 条 / 活跃 {} 人 / 案件 {} 起 / 人物 {} 位",
            render
                .get("message_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            render
                .get("participant_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            render
                .get("case_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
            render
                .get("character_count")
                .and_then(Value::as_u64)
                .unwrap_or(0),
        ),
        priority: "normal".to_string(),
        source_chat_ref: candidate
            .get("source_chat_ref")
            .cloned()
            .unwrap_or_default(),
        template_version: candidate
            .get("template_version")
            .and_then(Value::as_str)
            .unwrap_or_default()
            .to_string(),
        metadata: json!({
            "created_by_command": "run-daily-case-report-auto-publish-worker",
            "render_metrics": candidate.get("content_metrics").cloned().unwrap_or_default(),
            "character_universe": render.get("character_universe_summary").cloned().unwrap_or_default(),
            "public_output_style": render.get("public_output_style").cloned().unwrap_or_default(),
            "private_review_bundle": render.get("private_review_bundle").cloned().unwrap_or_default(),
        }),
        media_upload_evidence: Some(evidence.clone()),
    })
}

fn send_gate_allows(template: &str, message_count: usize, participant_count: usize) -> bool {
    if std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_SEND_GATE_BYPASS").as_deref() == Ok("1") {
        return true;
    }
    template == "roast-long-image" && message_count > 0 && participant_count > 0
}

/// Run the full production pipeline once and return a render summary compatible
/// with the Python `_result_json` / `_summary_result_json` shape.
pub async fn run_pipeline(
    pool: &PgPool,
    options: &PipelineOptions,
    rasterize_py: &Path,
    database_url: Option<&str>,
    narrative_config: Option<&NarrativeConfig>,
) -> Result<Value> {
    let (window_start, window_end, report_date) = resolve_window(options.date.as_deref())?;
    let time_range = time_range_label(window_start, window_end);
    let start_utc = window_start.with_timezone(&Utc);
    let end_utc = window_end.with_timezone(&Utc);

    let (messages, character_rows, creative_rows) =
        collect_for_report(pool, &options.chat_id, start_utc, end_utc).await?;

    let analyze_input = build_analyze_input(&messages, &character_rows, &creative_rows, start_utc);
    let analyze_report = analyze(&analyze_input);

    let group_name = options
        .group_name
        .as_deref()
        .unwrap_or(DEFAULT_GROUP_NAME)
        .to_string();
    let report_title = options
        .report_title
        .as_deref()
        .unwrap_or(DEFAULT_REPORT_TITLE)
        .to_string();
    let report = build_report_data(
        &analyze_report,
        &group_name,
        &report_title,
        &report_date,
        &time_range,
        window_start,
        window_end,
    )?;

    let narrative_md = if options.narrative_style != "none" {
        if let Some(config) = narrative_config {
            let narrative_report = build_narrative_report(&report, &analyze_input.messages);
            generate_narrative(&options.narrative_style, &narrative_report, config, None).ok()
        } else {
            eprintln!("WARN: narrative config unavailable, skipping LLM roast");
            None
        }
    } else {
        None
    };

    let render_input = RenderInput {
        report: report.clone(),
        template: options.template.clone(),
        width: options.width,
        narrative_md,
        image_format: DEFAULT_IMAGE_FORMAT.to_string(),
    };
    let render_output = render(&render_input).context("render daily report HTML")?;

    std::fs::create_dir_all(&options.output_dir).context("create output directory")?;
    let (image_path, rasterize_output) = rasterize_html(
        rasterize_py,
        &render_output.html,
        &options.output_dir,
        &options.template,
        options.width,
        DEFAULT_IMAGE_FORMAT,
    )
    .await
    .context("rasterize rendered HTML")?;

    let mut render_summary = build_render_summary(
        &report,
        &image_path,
        &rasterize_output,
        &options.template,
        &options.chat_id,
    );

    if !options.apply {
        return Ok(render_summary);
    }

    if !send_gate_allows(
        &options.template,
        report.message_count,
        report.participant_count,
    ) {
        bail!(
            "send gate rejected template={} message_count={} participant_count={}",
            options.template,
            report.message_count,
            report.participant_count
        );
    }

    let storage_backend = DailyCaseReportStorageBackend::from_env()?;
    let upload_request = media_upload_request_from_summary(&render_summary, &image_path)?;
    let upload_report = daily_case_report_media_upload(
        upload_request,
        true,
        storage_backend,
        database_url,
        Some(pool),
    )
    .await
    .context("daily case report media upload")?;

    let artifact_uri = upload_report
        .artifact_uri
        .as_ref()
        .context("media upload did not return artifact_uri")?;
    let evidence = upload_report
        .media_upload_evidence
        .as_ref()
        .context("media upload did not return evidence")?;
    let publish_request =
        auto_publish_request_from_summary(&render_summary, artifact_uri, evidence)?;
    let database_url = database_url.context("auto-publish requires a database URL")?;
    let publish_report =
        create_daily_case_report_auto_publish(pool, database_url, publish_request, true)
            .await
            .context("daily case report auto-publish create")?;

    // Augment the summary with the same post-publish fields the shell worker prints.
    render_summary["media_uploaded"] = json!(upload_report.action_status == "media_uploaded");
    render_summary["auto_publish_created"] = json!(
        publish_report.action_status == "auto_publish_created"
            || publish_report.action_status == "already_created"
    );
    render_summary["requires_human_final_confirmation"] =
        json!(publish_report.requires_human_final_confirmation);
    render_summary["send_ready_recorded"] = json!(publish_report.send_ready_recorded);
    render_summary["external_send_executed"] = json!(publish_report.external_send_executed);
    render_summary["artifact_type"] = json!(publish_report.artifact_type);
    render_summary["review_status"] = json!(publish_report.review_status);
    render_summary["content_hash"] = json!(publish_report.content_hash);
    render_summary["idempotency_key"] = json!(publish_report.idempotency_key);

    Ok(render_summary)
}

/// Entrypoint for the scheduled/auto-publish worker command.
pub async fn run_worker(cli: &Cli, once: bool, apply: bool) -> Result<()> {
    if std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED").as_deref()
        != Ok("1")
    {
        eprintln!("xiaoman daily case report auto-publish skipped: persistent enablement is not 1");
        return Ok(());
    }

    for key in [
        "QINTOPIA_SIDECAR_DATABASE_URL",
        "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID",
        "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE",
        "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND",
        "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID",
    ] {
        if std::env::var(key).unwrap_or_default().trim().is_empty() {
            bail!("xiaoman daily case report auto-publish requires {key}");
        }
    }
    if std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE").as_deref() != Ok("1")
    {
        bail!("xiaoman daily case report production read-through must be explicitly enabled");
    }

    let storage_backend = std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND")?;
    if storage_backend == "https-public" {
        for key in [
            "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_UPLOAD_ENDPOINT",
            "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_PUBLIC_BASE_URL",
            "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_ALLOWED_HOSTS",
        ] {
            if std::env::var(key).unwrap_or_default().trim().is_empty() {
                bail!("xiaoman daily case report https-public storage requires {key}");
            }
        }
    } else if storage_backend != "feishu-base" {
        bail!("xiaoman daily case report storage backend is not reviewed");
    }

    let database_url = cli.database_url_required()?;
    let pool = crate::db::connect(database_url, cli.db_max_connections).await?;
    let mut options = PipelineOptions::from_env()?;
    options.apply = apply;

    let narrative_config = if options.narrative_style != "none" {
        NarrativeConfig::from_cli(cli).ok()
    } else {
        None
    };

    let rasterize_py = resolve_release_path(
        &cli.daily_case_report_mcp_rasterize_py
            .as_deref()
            .map(PathBuf::from)
            .unwrap_or_else(|| PathBuf::from("workflows/xiaoman-daily-case-report/rasterize.py")),
    );

    let poll_seconds: u64 = std::env::var("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_POLL_SECONDS")
        .ok()
        .and_then(|value| value.parse().ok())
        .unwrap_or(60);

    loop {
        match run_pipeline(
            &pool,
            &options,
            &rasterize_py,
            Some(database_url),
            narrative_config.as_ref(),
        )
        .await
        {
            Ok(summary) => {
                println!("{}", serde_json::to_string_pretty(&summary)?);
            }
            Err(error) => {
                eprintln!(
                    "qintopia_runtime_one_shot_safe_failure=xiaoman-daily-case-report-auto-publish: {error}"
                );
                return Err(error);
            }
        }
        if once {
            break;
        }
        tokio::time::sleep(Duration::from_secs(poll_seconds)).await;
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use chrono::TimeZone;

    #[test]
    fn resolve_window_uses_yesterday_by_default() {
        let (start, end, display) = resolve_window(None).unwrap();
        assert_eq!(end - start, chrono::Duration::days(1));
        assert!(!display.is_empty());
    }

    #[test]
    fn resolve_window_parses_backfill_date() {
        let (start, end, display) = resolve_window(Some("2026-08-15")).unwrap();
        assert_eq!(
            start.timestamp(),
            DateTime::parse_from_rfc3339("2026-08-14T16:00:00+00:00")
                .unwrap()
                .timestamp()
        );
        assert_eq!(end - start, chrono::Duration::days(1));
        assert_eq!(display, "2026年08月15日");
    }

    #[test]
    fn time_range_label_for_full_day_uses_same_day_label() {
        let tz = report_timezone_offset();
        let start = tz.with_ymd_and_hms(2026, 8, 15, 0, 0, 0).unwrap();
        let end = start + chrono::Duration::days(1);
        let label = time_range_label(start, end);
        assert!(label.contains("00:00"));
        assert!(label.contains("23:59"));
    }

    #[test]
    fn dominant_role_label_maps_known_fact_types() {
        assert_eq!(dominant_role_label("activity_organizer"), "活动推进者");
        assert_eq!(dominant_role_label("unknown"), "长期在场者");
    }

    #[test]
    fn source_chat_ref_hashes_non_empty_chat_id() {
        let value = source_chat_ref("chat-123");
        assert_eq!(value.get("kind"), Some(&json!("sha256")));
        assert!(value
            .get("value")
            .and_then(Value::as_str)
            .unwrap()
            .starts_with("sha256:"));
    }

    #[test]
    fn template_version_matches_python() {
        assert_eq!(template_version("v3"), "xiaoman-daily-case-report-v3");
        assert_eq!(
            template_version("newspaper-elegant"),
            "xiaoman-daily-case-report-v4-newspaper"
        );
        assert_eq!(
            template_version("roast-long-image"),
            "xiaoman-daily-case-report-v5-roast-long-image"
        );
    }

    #[test]
    fn send_gate_allows_only_roast_long_image() {
        assert!(send_gate_allows("roast-long-image", 5, 3));
        assert!(!send_gate_allows("newspaper-elegant", 5, 3));
        assert!(!send_gate_allows("roast-long-image", 0, 3));
    }

    #[test]
    fn default_intro_text_matches_python_copy() {
        let render = json!({
            "report_date": "2026年08月18日",
            "group_name": "秦托邦的小伙伴（新）",
            "message_count": 38,
            "participant_count": 12,
        });
        let intro = default_intro_text(&render);
        assert_eq!(
            intro,
            "小满日报来啦 📰 2026年08月18日 「秦托邦的小伙伴（新）」的群聊，共 38 条消息、12 位邻居发言。昨天的新鲜事都在下面这张长图里，点开看看 👇"
        );
    }

    #[test]
    fn default_intro_text_falls_back_for_missing_fields() {
        let render = json!({});
        let intro = default_intro_text(&render);
        assert!(intro.contains("昨天 咱们群的群聊"));
        assert!(intro.contains("共 0 条消息、0 位邻居发言"));
    }
}
