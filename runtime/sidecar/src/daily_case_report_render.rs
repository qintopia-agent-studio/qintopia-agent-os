//! Daily case-report render: report assembly + HTML templates (PR 5 of the Rust migration).
//!
//! Mirrors the deterministic logic of `workflows/xiaoman-daily-case-report/report_builder.py`
//! and the HTML half of `renderer.py`/`roast_long_image.py`/`newspaper_elegant.py`.
//! No DB, network, LLM, or rasterization calls are made here.

use std::io::{self, Read};
use std::path::Path;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sha2::{Digest, Sha256};

use crate::config::Cli;
use crate::daily_case_report_analyze::{
    case_storyline_label, clean_text, node_key, CaseCard, CharacterCard, HotTopic, Suspect,
};

const DEFAULT_TIMEZONE: &str = "Asia/Shanghai";
const DEFAULT_IMAGE_FORMAT: &str = "jpeg";
const DEFAULT_JPEG_QUALITY: u8 = 92;

// ---------------------------------------------------------------------------
// Data types
// ---------------------------------------------------------------------------

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ReportData {
    pub group_name: String,
    pub report_title: String,
    pub report_date: String,
    pub time_range: String,
    pub member_count: usize,
    pub message_count: usize,
    pub participant_count: usize,
    pub case_count: usize,
    pub suspect_count: usize,
    pub character_count: usize,
    pub hourly_counts: Vec<i32>,
    pub cases: Vec<CaseCard>,
    pub suspects: Vec<Suspect>,
    pub characters: Vec<CharacterCard>,
    pub hot_topics: Vec<HotTopic>,
    pub highlight: Option<String>,
    #[serde(default)]
    pub character_universe: serde_json::Value,
    #[serde(default)]
    pub window_start: String,
    #[serde(default)]
    pub window_end: String,
    #[serde(default = "default_timezone")]
    pub timezone: String,
}

fn default_timezone() -> String {
    DEFAULT_TIMEZONE.to_string()
}

#[derive(Debug, Clone, Deserialize)]
pub struct RenderInput {
    pub report: ReportData,
    pub template: String,
    pub width: usize,
    pub narrative_md: Option<String>,
    #[serde(default = "default_image_format")]
    pub image_format: String,
}

fn default_image_format() -> String {
    DEFAULT_IMAGE_FORMAT.to_string()
}

#[derive(Debug, Clone, Serialize)]
pub struct RasterizationRequest {
    pub template: String,
    pub width: usize,
    pub image_format: String,
    pub quality: u8,
    pub html_path: String,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderMetadata {
    pub report_date: String,
    pub time_range: String,
    pub message_count: usize,
    pub participant_count: usize,
    pub case_count: usize,
    pub character_count: usize,
}

#[derive(Debug, Clone, Serialize)]
pub struct RenderOutput {
    pub html: String,
    pub raster_request: RasterizationRequest,
    pub metadata: RenderMetadata,
}

// ---------------------------------------------------------------------------
// Public entry point
// ---------------------------------------------------------------------------

pub fn render(input: &RenderInput) -> Result<RenderOutput> {
    let html = match input.template.as_str() {
        "roast-long-image" => templates::render_roast_long_image(input)?,
        "newspaper-elegant" => templates::render_newspaper_elegant(&input.report, input.width)?,
        "newspaper" => templates::render_newspaper(&input.report, input.width)?,
        _ => templates::render_v3(&input.report, input.width)?,
    };

    let raster_request = RasterizationRequest {
        template: input.template.clone(),
        width: input.width,
        image_format: input.image_format.clone(),
        quality: DEFAULT_JPEG_QUALITY,
        html_path: "daily-report.html".to_string(),
        output_path: "daily-report.jpg".to_string(),
    };

    let metadata = RenderMetadata {
        report_date: input.report.report_date.clone(),
        time_range: input.report.time_range.clone(),
        message_count: input.report.message_count,
        participant_count: input.report.participant_count,
        case_count: input.report.case_count,
        character_count: input.report.character_count,
    };

    Ok(RenderOutput {
        html,
        raster_request,
        metadata,
    })
}

// ---------------------------------------------------------------------------
// Report assembly
// ---------------------------------------------------------------------------

pub(crate) mod assembly {
    #![allow(dead_code)]
    #![allow(
        clippy::obfuscated_if_else,
        clippy::needless_borrow,
        clippy::too_many_arguments
    )]

    use super::*;

    const DEFAULT_MIN_CASE_MESSAGES: usize = 3;
    const DEFAULT_SUSPECT_LIMIT: usize = 5;
    const DEFAULT_CHARACTER_LIMIT: usize = 4;
    const REVIEW_DRAFT_REVIEWED_BY: &str = "xiaoman-daily-case-report-review-draft";
    pub const TEMPLATE_VERSION: &str = "xiaoman-daily-case-report-v5-roast-long-image";
    const MEMORY_LOOKBACK_DAYS: i64 = 90;

    const LOCAL_LIFE_HINTS: &[&str] = &[
        "活动", "饭局", "聚餐", "茶", "酒", "咖啡", "店", "地点", "场地", "本地", "社区", "市集",
        "报名", "接龙", "天气", "路线", "交通",
    ];

    const RISK_ITEMS: &[&str] = &[
        "所有直接引用必须回溯到 quote-map 后才能公开使用",
        "人物动态只作为今日出场，不自动升级为长期画像",
        "公众号候选文发布前必须人工审核隐私和人物边界",
    ];

    const SECTION_KEYS: &[&str] = &[
        "天气背景",
        "今日一句话",
        "主要话题",
        "人物动态",
        "地点/本地生活线索",
        "待解决问题",
        "不可公开/需人工复核素材",
        "候选公众号选题",
        "今日台词",
        "今日剧中人",
        "梗和回调候选",
        "同场关系",
        "今日主线",
    ];

    pub fn main_storyline_label(report: &ReportData) -> String {
        let lead = report
            .cases
            .first()
            .map(case_storyline_label)
            .unwrap_or_default();
        let top_character = report.characters.first();
        if let Some(top) = top_character {
            if !lead.is_empty() && !top.relationship_hint.is_empty() {
                return format!("{}，{}{}", lead, top.name, top.relationship_hint);
            }
            if !lead.is_empty() {
                return format!("{}，{}以「{}」出场", lead, top.name, top.role_label);
            }
            if let Some(topic) = report.hot_topics.first() {
                return format!("{}，{}接住今日话题", topic.keyword, top.name);
            }
            return format!("{}的今日出场", top.name);
        }
        if let Some(topic) = report.hot_topics.first() {
            return topic.keyword.clone();
        }
        if !lead.is_empty() {
            return lead;
        }
        "今天群里先把日常续上".to_string()
    }

    pub fn daily_opening_line(report: &ReportData) -> String {
        let storyline = main_storyline_label(report);
        if report.message_count == 0 {
            return "今天暂时没有形成可沉淀的群聊主线，日报保留空窗记录。".to_string();
        }
        let cast_line = if report.characters.is_empty() {
            String::new()
        } else {
            let cast = report
                .characters
                .iter()
                .take(3)
                .map(|c| format!("{}（{}）", c.name, c.role_label))
                .collect::<Vec<_>>()
                .join("、");
            format!(" 核心出场是 {}。", cast)
        };
        format!(
            "今天的主线是「{}」：{} 条消息、{} 位活跃成员，把信息、提问和现场反应压成一页可回看的群聊切片。{}",
            storyline, report.message_count, report.participant_count, cast_line
        )
    }

    pub fn meme_callback_candidates(report: &ReportData, limit: usize) -> Vec<String> {
        let mut candidates: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for topic in &report.hot_topics {
            let label = topic.keyword.trim();
            if !label.is_empty() && seen.insert(label.to_string()) {
                candidates.push(format!(
                    "「{}」：{} 条消息，{} 人参与",
                    label, topic.message_count, topic.participant_count
                ));
            }
        }
        for character in &report.characters {
            let label = character
                .meme_seed
                .trim()
                .to_string()
                .is_empty()
                .then(|| character.role_label.trim())
                .unwrap_or(character.meme_seed.trim());
            if !label.is_empty() && seen.insert(label.to_string()) {
                let mut detail = character.callback_hint.clone();
                if !character.relationship_hint.is_empty() {
                    detail = format!("{}；{}", detail, character.relationship_hint);
                }
                candidates.push(format!("「{}」：{}", label, detail));
            }
        }
        for case in &report.cases {
            let label = case_storyline_label(case);
            if !label.is_empty() && seen.insert(label.clone()) {
                candidates.push(format!("「{}」：{}", label, case.summary));
            }
        }
        candidates.into_iter().take(limit).collect()
    }

    pub fn relationship_candidates(report: &ReportData, limit: usize) -> Vec<String> {
        let mut candidates: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        if let Some(relationships) = report
            .character_universe
            .get("relationships")
            .and_then(|v| v.as_array())
        {
            for relationship in relationships {
                let label = relationship
                    .get("label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                let topic = relationship
                    .get("topic")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .trim()
                    .to_string();
                if label.is_empty() || !seen.insert(label.clone()) {
                    continue;
                }
                candidates.push(format!(
                    "{}（公开话题：{}）",
                    label,
                    topic_or_unknown(&topic)
                ));
                if candidates.len() >= limit {
                    return candidates;
                }
            }
        }
        if !candidates.is_empty() {
            return candidates;
        }
        for character in &report.characters {
            let label = character.relationship_hint.trim();
            if label.is_empty() || !seen.insert(label.to_string()) {
                continue;
            }
            let topic = character.relationship_topic.trim();
            candidates.push(format!(
                "{}（公开话题：{}）",
                label,
                topic_or_unknown(&topic)
            ));
            if candidates.len() >= limit {
                break;
            }
        }
        candidates
    }

    fn topic_or_unknown(topic: &str) -> String {
        if topic.is_empty() {
            "未标注".to_string()
        } else {
            topic.to_string()
        }
    }

    pub fn ordinary_digest_topic_cards(report: &ReportData) -> Vec<serde_json::Value> {
        let mut cards: Vec<serde_json::Value> = Vec::new();
        if !report.cases.is_empty() {
            for case in &report.cases {
                cards.push(serde_json::json!({
                    "title": case_storyline_label(case),
                    "participants": case.participant_count,
                    "message_count": case.message_count,
                    "summary": case.summary,
                    "anchors": case.bullets.iter().take(3).collect::<Vec<_>>(),
                    "message_ids": [],
                    "attachment_pointers": [],
                    "media_links": [],
                    "media_notes": {
                        "status": "omitted_no_reviewed_attachment_source",
                        "raw_message_payload_read": false,
                    },
                    "top_speaker": case.top_speaker,
                    "status": "candidate",
                }));
            }
            return cards.into_iter().take(6).collect();
        }
        for topic in &report.hot_topics {
            cards.push(serde_json::json!({
                "title": topic.keyword,
                "participants": topic.participant_count,
                "message_count": topic.message_count,
                "summary": format!("{} 条消息，{} 人参与", topic.message_count, topic.participant_count),
                "anchors": [],
                "message_ids": [],
                "attachment_pointers": [],
                "media_links": [],
                "media_notes": {
                    "status": "omitted_no_reviewed_attachment_source",
                    "raw_message_payload_read": false,
                },
                "top_speaker": "",
                "status": "candidate",
            }));
        }
        cards.into_iter().take(6).collect()
    }

    pub fn ordinary_digest_open_questions(report: &ReportData) -> Vec<String> {
        let mut questions: Vec<String> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for case in &report.cases {
            for bullet in &case.bullets {
                let text = clean_text(bullet);
                if text.contains('?')
                    || text.contains('？')
                    || text.contains("求助")
                    || text.contains("请问")
                    || text.contains("有没有")
                    || text.contains("怎么")
                {
                    let question: String = text.chars().take(120).collect();
                    if !question.is_empty() && seen.insert(question.clone()) {
                        questions.push(question);
                    }
                }
                if questions.len() >= 5 {
                    return questions;
                }
            }
        }
        for character in &report.characters {
            if character.role_label == "问题发射台" {
                let question: String = character.evidence.chars().take(120).collect();
                if !question.is_empty() && seen.insert(question.clone()) {
                    questions.push(question);
                }
            }
            if questions.len() >= 5 {
                break;
            }
        }
        questions
    }

    pub fn ordinary_digest_local_life_notes(report: &ReportData) -> Vec<serde_json::Value> {
        let mut notes: Vec<serde_json::Value> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for case in &report.cases {
            let mut candidate_texts: Vec<String> = vec![case_storyline_label(case)];
            candidate_texts.extend(case.bullets.iter().take(3).cloned());
            for text in candidate_texts {
                let cleaned = clean_text(&text);
                if cleaned.is_empty() || !LOCAL_LIFE_HINTS.iter().any(|hint| cleaned.contains(hint))
                {
                    continue;
                }
                let label: String = cleaned.chars().take(80).collect();
                if seen.insert(label.clone()) {
                    notes.push(serde_json::json!({
                        "label": label,
                        "source": case.case_no,
                        "status": "candidate",
                    }));
                }
                if notes.len() >= 5 {
                    return notes;
                }
            }
        }
        notes
    }

    pub fn ordinary_digest_candidate_topics(report: &ReportData) -> Vec<serde_json::Value> {
        let mut candidates: Vec<serde_json::Value> = Vec::new();
        let main_storyline = main_storyline_label(report);
        if report.case_count > 0 {
            candidates.push(serde_json::json!({
                "title": format!("{}的一天", main_storyline),
                "source": "daily_storyline",
                "reason": "当天已有可归档主线和 quote-map 候选证据",
                "review_required": true,
            }));
        }
        for callback in meme_callback_candidates(report, 3) {
            let label = callback
                .split('：')
                .next()
                .unwrap_or("")
                .trim()
                .trim_start_matches('「')
                .trim_end_matches('』');
            if !label.is_empty() {
                candidates.push(serde_json::json!({
                    "title": format!("围绕{}的群聊回看", label),
                    "source": "meme_callback_candidate",
                    "reason": "梗或回调候选需要人工判断是否适合公开文章",
                    "review_required": true,
                }));
            }
        }
        candidates.into_iter().take(5).collect()
    }

    fn character_key(character: &CharacterCard) -> String {
        if character.node_key.is_empty() {
            node_key(&character.name)
        } else {
            character.node_key.clone()
        }
    }

    fn character_anchor(character: &CharacterCard) -> String {
        if character.evidence_anchor.is_empty() {
            format!("daily_character_note:{}", character_key(character))
        } else {
            character.evidence_anchor.clone()
        }
    }

    fn character_evidence_count(character: &CharacterCard) -> i32 {
        if character.profile_evidence_count > 0 {
            character.profile_evidence_count
        } else if character.message_count >= 2 {
            1
        } else {
            0
        }
    }

    fn profile_upgrade_status(evidence_count: i32) -> &'static str {
        if evidence_count >= 2 {
            "eligible_for_review"
        } else {
            "daily_note_only"
        }
    }

    fn profile_upgrade_reason(
        evidence_count: i32,
        message_count: usize,
        topic_count: usize,
        relationship_hint: &str,
    ) -> String {
        if evidence_count < 2 {
            return "只有单日轻量信号，不能升级为长期人物画像".to_string();
        }
        let mut reasons: Vec<String> = Vec::new();
        reasons.push(format!("近{}天已有长期角色复现信号", MEMORY_LOOKBACK_DAYS));
        if message_count >= 2 {
            reasons.push(format!("今日同一身份 {} 条发言支撑", message_count));
        }
        if topic_count >= 2 {
            reasons.push(format!("今日跨 {} 个公开话题出现", topic_count));
        }
        if !relationship_hint.is_empty() {
            reasons.push("今日存在同场关系候选".to_string());
        }
        if reasons.is_empty() {
            "达到最小复现证据".to_string()
        } else {
            reasons.into_iter().take(3).collect::<Vec<_>>().join("；")
        }
    }

    fn character_upgrade_status(character: &CharacterCard) -> String {
        if character.profile_upgrade_status.is_empty() {
            profile_upgrade_status(character_evidence_count(character)).to_string()
        } else {
            character.profile_upgrade_status.clone()
        }
    }

    fn character_upgrade_reason(character: &CharacterCard) -> String {
        if !character.profile_upgrade_reason.is_empty() {
            return character.profile_upgrade_reason.clone();
        }
        profile_upgrade_reason(
            character_evidence_count(character),
            character.message_count,
            character.topic_count,
            &character.relationship_hint,
        )
    }

    pub fn build_character_universe(report: &ReportData) -> Result<serde_json::Value> {
        let people: Vec<serde_json::Value> = report
            .characters
            .iter()
            .map(|character| {
                serde_json::json!({
                    "type": "people",
                    "key": character_key(character),
                    "label": character.name,
                    "role_label": character.role_label,
                    "daily_line": character.one_liner,
                    "evidence": character.evidence,
                    "message_count": character.message_count,
                    "topic_count": character.topic_count,
                    "memory_label": character.memory_label,
                    "story_function": character.story_function,
                    "callback_hint": character.callback_hint,
                    "arc_label": character.arc_label,
                    "relationship_hint": character.relationship_hint,
                    "meme_seed": character.meme_seed,
                    "expressive_label": character.expressive_label,
                    "memory_weight_label": character.memory_weight_label,
                    "evidence_anchor": character_anchor(character),
                    "profile_evidence_count": character_evidence_count(character),
                    "profile_upgrade_status": character_upgrade_status(character),
                    "creative_profile_label": character.creative_profile_label,
                    "creative_profile_status": character.creative_profile_status,
                    "risk": "internal",
                })
            })
            .collect();

        let topics: Vec<serde_json::Value> = report
            .hot_topics
            .iter()
            .map(|topic| {
                serde_json::json!({
                    "type": "topics",
                    "key": node_key(&topic.keyword),
                    "label": topic.keyword,
                    "message_count": topic.message_count,
                    "participant_count": topic.participant_count,
                    "risk": "public_safe_summary",
                })
            })
            .collect();

        let events: Vec<serde_json::Value> = report
            .cases
            .iter()
            .map(|case| {
                serde_json::json!({
                    "type": "events",
                    "key": node_key(&case.title),
                    "label": case.title,
                    "case_no": case.case_no,
                    "time_label": case.time_label,
                    "summary": case.summary,
                    "top_speaker": case.top_speaker,
                    "evidence": case.bullets.iter().take(3).collect::<Vec<_>>(),
                    "risk": "internal",
                })
            })
            .collect();

        let storyline_candidates: Vec<serde_json::Value> = report
            .cases
            .iter()
            .filter(|case| case.message_count >= DEFAULT_MIN_CASE_MESSAGES)
            .map(|case| {
                serde_json::json!({
                    "type": "storylines",
                    "key": node_key(&case.title),
                    "label": case.title.replace("关于「", "").replace("」的讨论", ""),
                    "status": "candidate",
                    "last_seen": report.report_date,
                    "reason": format!("{} 条消息，{} 人参与", case.message_count, case.participant_count),
                    "related_event": case.case_no,
                    "risk": "internal_review_required",
                })
            })
            .collect();

        let mut memes: Vec<serde_json::Value> = Vec::new();
        let mut seen_meme_keys: std::collections::HashSet<String> =
            std::collections::HashSet::new();
        for character in &report.characters {
            let label = character.meme_seed.trim();
            if label.is_empty() {
                continue;
            }
            let key = node_key(label);
            if !seen_meme_keys.insert(key.clone()) {
                continue;
            }
            memes.push(serde_json::json!({
                "type": "memes",
                "key": key,
                "label": label,
                "source": "daily_character_note",
                "related_people": [character_key(character)],
                "status": "candidate",
                "risk": "internal_review_required",
            }));
        }
        for topic in &report.hot_topics {
            let label = format!("「{}」今日高频回调", topic.keyword);
            let key = node_key(&label);
            if !seen_meme_keys.insert(key.clone()) {
                continue;
            }
            memes.push(serde_json::json!({
                "type": "memes",
                "key": key,
                "label": label,
                "source": "daily_hot_topic",
                "message_count": topic.message_count,
                "participant_count": topic.participant_count,
                "status": "candidate",
                "risk": "internal_review_required",
            }));
        }

        let callbacks: Vec<serde_json::Value> = report
            .characters
            .iter()
            .filter(|character| !character.callback_hint.is_empty())
            .map(|character| {
                serde_json::json!({
                    "type": "callbacks",
                    "key": node_key(&format!("{}-{}-callback", character.node_key, character.role_label)),
                    "label": character.callback_hint,
                    "related_people": [character_key(character)],
                    "memory_weight_label": character.memory_weight_label,
                    "status": "candidate",
                    "risk": "internal_review_required",
                })
            })
            .collect();

        let creative_profile_candidates: Vec<serde_json::Value> = report
            .characters
            .iter()
            .filter(|character| !character.role_label.is_empty())
            .map(|character| {
                let blocked_reason = if character_upgrade_status(character) == "daily_note_only" {
                    character_upgrade_reason(character)
                } else {
                    String::new()
                };
                serde_json::json!({
                    "type": "creative_profile_candidates",
                    "key": node_key(&format!("{}-{}-creative-profile", character.node_key, character.role_label)),
                    "profile_kind": "creative_profile",
                    "profile_version": "daily-character-v1",
                    "related_person": character_key(character),
                    "candidate_role_label": character.role_label,
                    "story_function": character.story_function,
                    "daily_arc": character.arc_label,
                    "memory_weight_label": character.memory_weight_label,
                    "meme_seed": character.meme_seed,
                    "callback_hint": character.callback_hint,
                    "expressive_label": character.expressive_label,
                    "evidence_anchor": character_anchor(character),
                    "recurrence_evidence_count": character_evidence_count(character),
                    "minimum_recurrence_met": character_evidence_count(character) >= 2,
                    "profile_upgrade_status": character_upgrade_status(character),
                    "profile_upgrade_reason": character_upgrade_reason(character),
                    "blocked_reason": blocked_reason,
                    "evidence_policy": "daily_character_note_or_quote_map",
                    "minimum_recurrence": 2,
                    "status": "candidate",
                    "public_surface_allowed": false,
                    "risk": "internal_review_required",
                })
            })
            .collect();

        let selected_people_keys: std::collections::HashSet<String> =
            report.characters.iter().map(character_key).collect();
        let mut relationships: Vec<serde_json::Value> = Vec::new();
        let mut seen_relationships: std::collections::HashSet<Vec<String>> =
            std::collections::HashSet::new();
        for character in &report.characters {
            let source = character_key(character);
            let target = character.relationship_target_key.trim();
            let topic = character.relationship_topic.trim();
            if target.is_empty() || !selected_people_keys.contains(target) || source == target {
                continue;
            }
            let mut relation_key = vec![source.clone(), target.to_string()];
            relation_key.sort();
            relation_key.push(topic.to_string());
            if !seen_relationships.insert(relation_key.clone()) {
                continue;
            }
            relationships.push(serde_json::json!({
                "type": "relationships",
                "key": node_key(&relation_key.join("-")),
                "source": source,
                "target": target,
                "relation": "co_discusses_topic",
                "label": character.relationship_hint,
                "topic": topic,
                "risk": "public_safe_summary",
            }));
        }

        let creative_meme_candidates: Vec<serde_json::Value> = memes
            .iter()
            .take(8)
            .map(|item| {
                serde_json::json!({
                    "type": "creative_meme_candidate",
                    "key": node_key(&format!("creative-meme-{}", item.get("key").and_then(|v| v.as_str()).unwrap_or(""))),
                    "label": item.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                    "related_people": item.get("related_people").cloned().unwrap_or(serde_json::Value::Array(vec![])),
                    "source": item.get("source").and_then(|v| v.as_str()).unwrap_or(""),
                    "lookback_days": [7, 14, 30],
                    "evidence_policy": "daily_meme_candidate_or_quote_map",
                    "status": "pending_review",
                    "public_surface_allowed": false,
                    "risk": "internal_review_required",
                })
            })
            .collect();

        let creative_relationship_candidates: Vec<serde_json::Value> = relationships
            .iter()
            .take(8)
            .map(|item| {
                serde_json::json!({
                    "type": "creative_relationship_candidate",
                    "key": node_key(&format!("creative-relationship-{}", item.get("key").and_then(|v| v.as_str()).unwrap_or(""))),
                    "source": item.get("source").and_then(|v| v.as_str()).unwrap_or(""),
                    "target": item.get("target").and_then(|v| v.as_str()).unwrap_or(""),
                    "topic": item.get("topic").and_then(|v| v.as_str()).unwrap_or(""),
                    "candidate_label": item.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                    "evidence_policy": "same_topic_co_presence_only",
                    "status": "pending_review",
                    "public_surface_allowed": false,
                    "risk": "internal_review_required",
                })
            })
            .collect();

        let creative_timeline_candidates: Vec<serde_json::Value> = storyline_candidates
            .iter()
            .take(8)
            .filter(|item| {
                item.get("label")
                    .and_then(|v| v.as_str())
                    .map(|s| !s.is_empty())
                    .unwrap_or(false)
            })
            .map(|item| {
                let label = item.get("label").and_then(|v| v.as_str()).unwrap_or("");
                serde_json::json!({
                    "type": "creative_timeline_candidate",
                    "key": node_key(&format!("creative-timeline-{}", item.get("key").and_then(|v| v.as_str()).unwrap_or(""))),
                    "label": label,
                    "last_seen": item.get("last_seen").and_then(|v| v.as_str()).unwrap_or(&report.report_date),
                    "related_event": item.get("related_event").and_then(|v| v.as_str()).unwrap_or(""),
                    "lookback_days": [7, 14, 30],
                    "candidate_arc": format!("连续观察「{}」是否复现", label),
                    "evidence_policy": "daily_storyline_or_wiki_timeline",
                    "status": "pending_review",
                    "public_surface_allowed": false,
                    "risk": "internal_review_required",
                })
            })
            .collect();

        let creative_universe_candidate_sets = serde_json::json!({
            "cross_day_memes": creative_meme_candidates,
            "relationship_labels": creative_relationship_candidates,
            "timeline_threads": creative_timeline_candidates,
        });
        let creative_universe_candidate_count = creative_meme_candidates.len()
            + creative_relationship_candidates.len()
            + creative_timeline_candidates.len();

        let expressive_label_candidates: Vec<serde_json::Value> = report
            .characters
            .iter()
            .filter(|character| {
                !character.role_label.is_empty()
                    && (!character.expressive_label.is_empty()
                        || !character.meme_seed.is_empty()
                        || !character.relationship_hint.is_empty()
                        || !character.callback_hint.is_empty())
            })
            .map(|character| {
                let candidate_label = if !character.expressive_label.is_empty() {
                    character.expressive_label.clone()
                } else if !character.meme_seed.is_empty() {
                    character.meme_seed.clone()
                } else if !character.relationship_hint.is_empty() {
                    character.relationship_hint.clone()
                } else {
                    character.callback_hint.clone()
                };
                let (label_kind, review_status, public_surface_allowed) =
                    if !character.expressive_label.is_empty() {
                        ("reviewed_public", "reviewed", true)
                    } else {
                        (
                            "draft_requires_owner_review",
                            "candidate",
                            false,
                        )
                    };
                serde_json::json!({
                    "type": "expressive_label_candidate",
                    "key": node_key(&format!("expressive-{}-{}", character_key(character), character.role_label)),
                    "related_person": character_key(character),
                    "candidate_label": candidate_label,
                    "label_kind": label_kind,
                    "review_status": review_status,
                    "public_surface_allowed": public_surface_allowed,
                    "evidence_anchor": character_anchor(character),
                    "risk": "field_level_owner_review_required",
                })
            })
            .collect();

        let mut edges: Vec<serde_json::Value> = Vec::new();
        let bullets_joined = |case: &CaseCard| case.bullets.join(" ");
        for character in &report.characters {
            let character_key_value = character_key(character);
            for case in &report.cases {
                if character.name == case.top_speaker
                    || bullets_joined(case).contains(&character.name)
                {
                    edges.push(serde_json::json!({
                        "source": character_key_value.clone(),
                        "target": node_key(&case.title),
                        "relation": "appears_in",
                        "evidence": case.case_no,
                    }));
                }
            }
            for topic in &report.hot_topics {
                if character.evidence.contains(&topic.keyword) {
                    edges.push(serde_json::json!({
                        "source": character_key_value.clone(),
                        "target": node_key(&topic.keyword),
                        "relation": "mentions_topic",
                        "evidence": "daily_character_note",
                    }));
                }
            }
            if !character.meme_seed.is_empty() {
                edges.push(serde_json::json!({
                    "source": character_key_value.clone(),
                    "target": node_key(&character.meme_seed),
                    "relation": "seeds_callback",
                    "evidence": "daily_character_note",
                }));
            }
        }
        for relationship in &relationships {
            edges.push(serde_json::json!({
                "source": relationship.get("source").and_then(|v| v.as_str()).unwrap_or(""),
                "target": relationship.get("target").and_then(|v| v.as_str()).unwrap_or(""),
                "relation": relationship.get("relation").and_then(|v| v.as_str()).unwrap_or(""),
                "evidence": relationship.get("topic").and_then(|v| v.as_str()).unwrap_or(""),
            }));
        }

        Ok(serde_json::json!({
            "schema_version": "xiaoman-character-universe-v1",
            "source": "daily_case_report_second_pass",
            "retained_source_policy": "curated_summary_only",
            "raw_messages_included": false,
            "profile_fact_text_included": false,
            "people": people,
            "topics": topics,
            "events": events,
            "memes": memes,
            "callbacks": callbacks,
            "relationships": relationships,
            "expressive_label_candidates": expressive_label_candidates,
            "creative_profile_candidates": creative_profile_candidates,
            "creative_universe_candidates": {
                "schema_version": "xiaoman-daily-creative-universe-candidates-v1",
                "source": "daily_case_report_second_pass",
                "apply_mode": "candidate_only",
                "public_surface_allowed": false,
                "review_required": true,
                "raw_messages_included": false,
                "profile_fact_text_included": false,
                "writes_member_profile_snapshots": false,
                "candidate_count": creative_universe_candidate_count,
                "candidate_sets": creative_universe_candidate_sets,
            },
            "creative_profile_candidate_policy": {
                "profile_kind": "creative_profile",
                "apply_mode": "candidate_only",
                "writes_member_profile_snapshots": false,
                "public_surface_allowed": false,
                "evidence_policy": "daily_character_note_or_quote_map",
                "review_required": true,
            },
            "expressive_label_policy": {
                "apply_mode": "candidate_only",
                "public_surface_allowed_requires_owner_review": true,
                "public_render_requires_reviewed_safe_reply_hints": true,
                "writes_member_profile_snapshots": false,
                "review_required": true,
            },
            "storyline_candidates": storyline_candidates,
            "edges": edges,
        }))
    }

    fn quote_entry(
        index: usize,
        source_kind: &str,
        excerpt: &str,
        speaker_label: &str,
        speaker_key: &str,
        related_people: Vec<String>,
        related_topics: Vec<String>,
        related_events: Vec<String>,
        related_memes: Vec<String>,
        source_anchor: &str,
    ) -> Option<serde_json::Value> {
        let cleaned = clean_text(excerpt);
        if cleaned.is_empty() {
            return None;
        }
        let truncated = if cleaned.chars().count() > 120 {
            format!("{}...", cleaned.chars().take(120).collect::<String>())
        } else {
            cleaned.clone()
        };
        let related_people = if related_people.is_empty() && !speaker_key.is_empty() {
            vec![speaker_key.to_string()]
        } else {
            related_people
        };
        Some(serde_json::json!({
            "key": format!("quote-{index:03}"),
            "source_kind": source_kind,
            "speaker_label": speaker_label,
            "speaker_key": speaker_key,
            "excerpt": truncated,
            "related_people": related_people,
            "related_topics": related_topics,
            "related_events": related_events,
            "related_memes": related_memes,
            "source_anchor": source_anchor,
            "review_status": "candidate",
            "public_surface_allowed": false,
        }))
    }

    pub fn build_quote_map(report: &ReportData) -> Result<serde_json::Value> {
        let mut entries: Vec<serde_json::Value> = Vec::new();
        let mut next_index = 1;

        if let Some(highlight) = &report.highlight {
            let related_topics = report
                .hot_topics
                .iter()
                .take(2)
                .map(|topic| node_key(&topic.keyword))
                .collect();
            if let Some(entry) = quote_entry(
                next_index,
                "daily_highlight",
                highlight,
                "",
                "",
                vec![],
                related_topics,
                vec![],
                vec![],
                "",
            ) {
                entries.push(entry);
                next_index += 1;
            }
        }

        for character in &report.characters {
            let person_key = if character.node_key.is_empty() {
                node_key(&character.name)
            } else {
                character.node_key.clone()
            };
            let related_memes = if character.meme_seed.is_empty() {
                vec![]
            } else {
                vec![node_key(&character.meme_seed)]
            };
            if let Some(entry) = quote_entry(
                next_index,
                "daily_character_note",
                &character.evidence,
                &character.name,
                &person_key,
                vec![],
                vec![],
                vec![],
                related_memes,
                &character.evidence_anchor,
            ) {
                entries.push(entry);
                next_index += 1;
            }
        }

        for case in &report.cases {
            let event_key = node_key(&case.title);
            for bullet in case.bullets.iter().take(2) {
                if let Some(entry) = quote_entry(
                    next_index,
                    "daily_case_bullet",
                    bullet,
                    &case.top_speaker,
                    "",
                    vec![],
                    vec![],
                    vec![event_key.clone()],
                    vec![],
                    "",
                ) {
                    entries.push(entry);
                    next_index += 1;
                }
            }
        }

        Ok(serde_json::json!({
            "schema_version": "xiaoman-daily-quote-map-v1",
            "source": "daily_case_report_private_review_bundle",
            "retained_source_policy": "private_curated_excerpts_only",
            "raw_message_rows_included": false,
            "profile_fact_text_included": false,
            "curated_excerpts_included": true,
            "public_surface_allowed": false,
            "review_required": true,
            "entry_count": entries.len(),
            "entries": entries,
        }))
    }

    fn lookback_callback_candidates(report: &ReportData) -> Vec<serde_json::Value> {
        let mut callbacks: Vec<serde_json::Value> = Vec::new();
        let mut seen: std::collections::HashSet<String> = std::collections::HashSet::new();
        for character in &report.characters {
            let seed = character
                .meme_seed
                .clone()
                .is_empty()
                .then(|| {
                    if character.callback_hint.is_empty() {
                        character.role_label.clone()
                    } else {
                        character.callback_hint.clone()
                    }
                })
                .unwrap_or(character.meme_seed.clone());
            let seed = seed.trim();
            if seed.is_empty() {
                continue;
            }
            for days in [7, 14, 30] {
                let person_key = if character.node_key.is_empty() {
                    character.name.clone()
                } else {
                    character.node_key.clone()
                };
                let key = node_key(&format!("{}-{}-{}d", person_key, seed, days));
                if !seen.insert(key.clone()) {
                    continue;
                }
                callbacks.push(serde_json::json!({
                    "key": key,
                    "lookback_days": days,
                    "label": format!("{}的「{}」{}天回看候选", character.name, seed, days),
                    "related_person": character.node_key.clone().is_empty().then(|| node_key(&character.name)).unwrap_or(character.node_key.clone()),
                    "trigger": if character.callback_hint.is_empty() { character.arc_label.clone() } else { character.callback_hint.clone() },
                    "status": "candidate",
                    "risk": "internal_review_required",
                }));
                if callbacks.len() >= 9 {
                    return callbacks;
                }
            }
        }
        for case in &report.cases {
            let label = case_storyline_label(case);
            if label.is_empty() {
                continue;
            }
            let key = node_key(&format!("{}-7d-lookback", label));
            if !seen.insert(key.clone()) {
                continue;
            }
            callbacks.push(serde_json::json!({
                "key": key,
                "lookback_days": 7,
                "label": format!("「{}」7天回看候选", label),
                "related_event": node_key(&case.title),
                "trigger": case.summary,
                "status": "candidate",
                "risk": "internal_review_required",
            }));
        }
        callbacks
    }

    fn ordinary_digest_people_notes(report: &ReportData) -> Vec<serde_json::Value> {
        report
            .characters
            .iter()
            .map(|character| {
                serde_json::json!({
                    "person_key": character.node_key.clone().is_empty().then(|| node_key(&character.name)).unwrap_or(character.node_key.clone()),
                    "display_label": character.name,
                    "role_label": character.role_label,
                    "story_function": character.story_function,
                    "daily_arc": character.arc_label,
                    "evidence_anchor": character.evidence_anchor,
                    "quote": character.evidence,
                    "memory_weight_label": character.memory_weight_label,
                    "status": "candidate",
                })
            })
            .collect()
    }

    pub fn build_draft_bundle(
        report: &ReportData,
        quote_map: &serde_json::Value,
        wiki_bundle: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let quote_keys: Vec<String> = quote_map
            .get("entries")
            .and_then(|v| v.as_array())
            .map(|entries| {
                entries
                    .iter()
                    .filter_map(|entry| entry.get("key").and_then(|v| v.as_str()).map(String::from))
                    .filter(|key| !key.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let main_storyline = main_storyline_label(report);
        let callback_candidates = meme_callback_candidates(report, 5);
        let relationship_candidates = relationship_candidates(report, 4);
        let character_cards: Vec<serde_json::Value> = report
            .characters
            .iter()
            .map(|character| {
                serde_json::json!({
                    "person_key": character.node_key.clone().is_empty().then(|| node_key(&character.name)).unwrap_or(character.node_key.clone()),
                    "display_label": character.name,
                    "role_label": character.role_label,
                    "story_function": character.story_function,
                    "daily_arc": character.arc_label,
                    "callback_hint": character.callback_hint,
                    "memory_weight_label": character.memory_weight_label,
                    "quote_anchor": character.evidence_anchor,
                    "status": "candidate",
                    "risk": "internal_review_required",
                })
            })
            .collect();

        let title_candidates = vec![
            format!("小满群聊日报｜{}", main_storyline),
            format!(
                "{} 位剧中人，把今天的群聊推成一条主线",
                report.character_count
            ),
            format!("今日回看：{}", main_storyline),
        ];

        let mut opening_candidates = vec![daily_opening_line(report)];
        if let Some(highlight) = &report.highlight {
            opening_candidates.push(format!("今天可以先从这句看起：{}", highlight));
        }

        let ordinary_topic_cards = ordinary_digest_topic_cards(report);
        let ordinary_people_notes = ordinary_digest_people_notes(report);
        let ordinary_local_life_notes = ordinary_digest_local_life_notes(report);
        let ordinary_open_questions = ordinary_digest_open_questions(report);
        let ordinary_candidate_topics = ordinary_digest_candidate_topics(report);

        let storyline_timeline: Vec<serde_json::Value> = report
            .cases
            .iter()
            .map(|case| {
                serde_json::json!({
                    "date": report.report_date,
                    "case_no": case.case_no,
                    "storyline": case_storyline_label(case),
                    "message_count": case.message_count,
                    "participant_count": case.participant_count,
                    "status": "candidate",
                    "risk": "internal_review_required",
                })
            })
            .collect();

        let lookback_callbacks = lookback_callback_candidates(report);

        let mut bundle = serde_json::json!({
            "schema_version": "xiaoman-daily-draft-bundle-v1",
            "source": "daily_case_report_private_review_bundle",
            "retained_source_policy": "private_curated_drafts_only",
            "raw_message_rows_included": false,
            "profile_fact_text_included": false,
            "public_surface_allowed": false,
            "review_required": true,
            "ordinary_digest": {
                "status": "candidate",
                "title": format!("小满群聊日报｜{}｜{}", report.report_date, main_storyline),
                "main_storyline": main_storyline,
                "weather_context": {
                    "status": "omitted_no_reviewed_weather_source",
                    "public_surface_allowed": false,
                },
                "one_sentence_summary": daily_opening_line(report),
                "main_topics": ordinary_topic_cards,
                "people_notes": ordinary_people_notes,
                "local_life_notes": ordinary_local_life_notes,
                "open_questions": ordinary_open_questions,
                "risk_items": RISK_ITEMS,
                "candidate_public_topics": ordinary_candidate_topics,
                "section_keys": SECTION_KEYS,
                "quote_keys": quote_keys.iter().take(12).collect::<Vec<_>>(),
            },
            "roast_digest": {
                "status": "candidate_requires_owner_review",
                "tone": "轻吐槽人物群像",
                "character_cards": character_cards,
                "callback_angles": callback_candidates,
                "boundary": {
                    "criticize_behavior_not_identity": true,
                    "single_day_trait_blocked": true,
                    "sensitive_attributes_blocked": true,
                },
            },
            "public_draft": {
                "status": "candidate_requires_owner_review",
                "title_candidates": title_candidates,
                "opening_candidates": opening_candidates.iter().take(3).collect::<Vec<_>>(),
                "storyline_links": wiki_bundle
                    .get("storylines")
                    .and_then(|v| v.as_array())
                    .map(|arr| {
                        arr.iter()
                            .filter_map(|item| item.get("key").and_then(|v| v.as_str()).map(String::from))
                            .take(8)
                            .collect::<Vec<_>>()
                    })
                    .unwrap_or_default(),
                "quote_keys": quote_keys.iter().take(8).collect::<Vec<_>>(),
            },
            "storyline_memory": {
                "active_storyline_candidates": wiki_bundle.get("storylines").cloned().unwrap_or(serde_json::Value::Array(vec![])),
                "timeline": storyline_timeline,
                "lookback_callbacks": lookback_callbacks,
                "relationship_candidates": relationship_candidates,
            },
        });

        if let Some(obj) = bundle.as_object_mut() {
            obj.insert(
                "counts".to_string(),
                serde_json::json!({
                    "ordinary_digest_section_count": SECTION_KEYS.len(),
                    "ordinary_digest_topic_count": ordinary_topic_cards.len(),
                    "ordinary_digest_people_note_count": ordinary_people_notes.len(),
                    "ordinary_digest_local_life_note_count": ordinary_local_life_notes.len(),
                    "ordinary_digest_open_question_count": ordinary_open_questions.len(),
                    "ordinary_digest_candidate_public_topic_count": ordinary_candidate_topics.len(),
                    "roast_profile_candidate_count": character_cards.len(),
                    "public_draft_title_count": title_candidates.len(),
                    "storyline_timeline_count": storyline_timeline.len(),
                    "lookback_callback_count": lookback_callbacks.len(),
                }),
            );
        }

        Ok(bundle)
    }

    pub fn build_wiki_bundle(
        report: &ReportData,
        quote_map: &serde_json::Value,
    ) -> Result<serde_json::Value> {
        let universe = &report.character_universe;

        let mut event_quote_keys: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let mut people_quote_keys: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();
        let mut meme_quote_keys: std::collections::HashMap<String, Vec<String>> =
            std::collections::HashMap::new();

        if let Some(entries) = quote_map.get("entries").and_then(|v| v.as_array()) {
            for entry in entries {
                let quote_key = entry.get("key").and_then(|v| v.as_str()).unwrap_or("");
                if quote_key.is_empty() {
                    continue;
                }
                if let Some(events) = entry.get("related_events").and_then(|v| v.as_array()) {
                    for event in events {
                        if let Some(event_key) = event.as_str() {
                            event_quote_keys
                                .entry(event_key.to_string())
                                .or_default()
                                .push(quote_key.to_string());
                        }
                    }
                }
                if let Some(people) = entry.get("related_people").and_then(|v| v.as_array()) {
                    for person in people {
                        if let Some(person_key) = person.as_str() {
                            people_quote_keys
                                .entry(person_key.to_string())
                                .or_default()
                                .push(quote_key.to_string());
                        }
                    }
                }
                if let Some(memes) = entry.get("related_memes").and_then(|v| v.as_array()) {
                    for meme in memes {
                        if let Some(meme_key) = meme.as_str() {
                            meme_quote_keys
                                .entry(meme_key.to_string())
                                .or_default()
                                .push(quote_key.to_string());
                        }
                    }
                }
            }
        }

        let mut people: Vec<serde_json::Value> = Vec::new();
        if let Some(universe_people) = universe.get("people").and_then(|v| v.as_array()) {
            for item in universe_people {
                let key = item
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                people.push(serde_json::json!({
                    "type": "wiki_person",
                    "key": key,
                    "label": item.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                    "role_label": item.get("role_label").and_then(|v| v.as_str()).unwrap_or(""),
                    "daily_arc": item.get("arc_label").and_then(|v| v.as_str()).unwrap_or(""),
                    "story_function": item.get("story_function").and_then(|v| v.as_str()).unwrap_or(""),
                    "callback_hint": item.get("callback_hint").and_then(|v| v.as_str()).unwrap_or(""),
                    "memory_weight_label": item.get("memory_weight_label").and_then(|v| v.as_str()).unwrap_or(""),
                    "evidence_anchor": item.get("evidence_anchor").and_then(|v| v.as_str()).unwrap_or(""),
                    "profile_upgrade_status": item.get("profile_upgrade_status").and_then(|v| v.as_str()).unwrap_or(""),
                    "creative_profile_status": item.get("creative_profile_status").and_then(|v| v.as_str()).unwrap_or(""),
                    "quote_keys": people_quote_keys.get(&key).cloned().unwrap_or_default(),
                    "status": "candidate",
                    "risk": "internal_review_required",
                }));
            }
        }

        let topics: Vec<serde_json::Value> = universe
            .get("topics")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|item| {
                        serde_json::json!({
                            "type": "wiki_topic",
                            "key": item.get("key").and_then(|v| v.as_str()).unwrap_or(""),
                            "label": item.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                            "message_count": item.get("message_count").and_then(|v| v.as_u64()).unwrap_or(0),
                            "participant_count": item.get("participant_count").and_then(|v| v.as_u64()).unwrap_or(0),
                            "status": "candidate",
                            "risk": "public_safe_summary",
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let mut events: Vec<serde_json::Value> = Vec::new();
        if let Some(universe_events) = universe.get("events").and_then(|v| v.as_array()) {
            for item in universe_events {
                let key = item
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                events.push(serde_json::json!({
                    "type": "wiki_event",
                    "key": key,
                    "label": item.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                    "case_no": item.get("case_no").and_then(|v| v.as_str()).unwrap_or(""),
                    "time_label": item.get("time_label").and_then(|v| v.as_str()).unwrap_or(""),
                    "summary": item.get("summary").and_then(|v| v.as_str()).unwrap_or(""),
                    "quote_keys": event_quote_keys.get(&key).cloned().unwrap_or_default(),
                    "status": "candidate",
                    "risk": "internal_review_required",
                }));
            }
        }

        let mut memes: Vec<serde_json::Value> = Vec::new();
        if let Some(universe_memes) = universe.get("memes").and_then(|v| v.as_array()) {
            for item in universe_memes {
                let key = item
                    .get("key")
                    .and_then(|v| v.as_str())
                    .unwrap_or("")
                    .to_string();
                memes.push(serde_json::json!({
                    "type": "wiki_meme",
                    "key": key,
                    "label": item.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                    "source": item.get("source").and_then(|v| v.as_str()).unwrap_or(""),
                    "related_people": item.get("related_people").cloned().unwrap_or(serde_json::Value::Array(vec![])),
                    "quote_keys": meme_quote_keys.get(&key).cloned().unwrap_or_default(),
                    "status": "candidate",
                    "risk": "internal_review_required",
                }));
            }
        }

        let relationships: Vec<serde_json::Value> = universe
            .get("relationships")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|item| {
                        serde_json::json!({
                            "type": "wiki_relationship",
                            "key": item.get("key").and_then(|v| v.as_str()).unwrap_or(""),
                            "source": item.get("source").and_then(|v| v.as_str()).unwrap_or(""),
                            "target": item.get("target").and_then(|v| v.as_str()).unwrap_or(""),
                            "relation": item.get("relation").and_then(|v| v.as_str()).unwrap_or(""),
                            "label": item.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                            "topic": item.get("topic").and_then(|v| v.as_str()).unwrap_or(""),
                            "status": "candidate",
                            "risk": "public_safe_summary",
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let storylines: Vec<serde_json::Value> = universe
            .get("storyline_candidates")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .map(|item| {
                        serde_json::json!({
                            "type": "wiki_storyline",
                            "key": item.get("key").and_then(|v| v.as_str()).unwrap_or(""),
                            "label": item.get("label").and_then(|v| v.as_str()).unwrap_or(""),
                            "last_seen": item.get("last_seen").and_then(|v| v.as_str()).unwrap_or(&report.report_date),
                            "reason": item.get("reason").and_then(|v| v.as_str()).unwrap_or(""),
                            "related_event": item.get("related_event").and_then(|v| v.as_str()).unwrap_or(""),
                            "status": "candidate",
                            "risk": "internal_review_required",
                        })
                    })
                    .collect()
            })
            .unwrap_or_default();

        let timeline: Vec<serde_json::Value> = report
            .cases
            .iter()
            .map(|case| {
                serde_json::json!({
                    "type": "daily_timeline_entry",
                    "key": node_key(&format!("{}-{}", report.report_date, case.case_no)),
                    "date": report.report_date,
                    "case_no": case.case_no,
                    "label": case_storyline_label(case),
                    "time_label": case.time_label,
                    "message_count": case.message_count,
                    "participant_count": case.participant_count,
                    "status": "candidate",
                    "risk": "internal_review_required",
                })
            })
            .collect();

        let counts = serde_json::json!({
            "people": people.len(),
            "topics": topics.len(),
            "events": events.len(),
            "memes": memes.len(),
            "relationships": relationships.len(),
            "storylines": storylines.len(),
            "timeline": timeline.len(),
        });

        let mut bundle = serde_json::json!({
            "schema_version": "xiaoman-daily-wiki-bundle-v1",
            "source": "daily_case_report_private_review_bundle",
            "retained_source_policy": "candidate_nodes_and_quote_keys_only",
            "raw_message_rows_included": false,
            "profile_fact_text_included": false,
            "public_surface_allowed": false,
            "review_required": true,
            "people": people,
            "topics": topics,
            "events": events,
            "memes": memes,
            "relationships": relationships,
            "storylines": storylines,
            "timeline": timeline,
        });

        if let Some(obj) = bundle.as_object_mut() {
            obj.insert("counts".to_string(), counts);
        }

        Ok(bundle)
    }

    fn source_chat_ref(chat_id: Option<&str>) -> Option<serde_json::Value> {
        let chat_id = chat_id?;
        if chat_id.is_empty() {
            return None;
        }
        let mut hasher = Sha256::new();
        hasher.update(chat_id.as_bytes());
        let digest = format!("{:x}", hasher.finalize());
        Some(serde_json::json!({
            "kind": "sha256",
            "value": format!("sha256:{}", digest),
        }))
    }

    pub fn build_run_manifest(
        report: &ReportData,
        quote_map: &serde_json::Value,
        wiki_bundle: &serde_json::Value,
        draft_bundle: Option<&serde_json::Value>,
        source_chat_id: Option<&str>,
    ) -> Result<serde_json::Value> {
        let universe = &report.character_universe;
        let draft_counts = draft_bundle
            .and_then(|bundle| bundle.get("counts"))
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        let creative_universe_candidates = universe
            .get("creative_universe_candidates")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        let long_term_member_facts_used = report
            .characters
            .iter()
            .any(|character| character.member_fact_memory_used);
        let reviewed_creative_profiles_used = report
            .characters
            .iter()
            .any(|character| character.creative_profile_status == "active_reviewed");

        let wiki_counts = wiki_bundle
            .get("counts")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        let creative_profile_candidates = universe
            .get("creative_profile_candidates")
            .and_then(|v| v.as_array())
            .map(|v| v.len())
            .unwrap_or(0);
        let expressive_label_candidates = universe
            .get("expressive_label_candidates")
            .and_then(|v| v.as_array())
            .map(|v| v.len())
            .unwrap_or(0);
        let reviewed_public_expressive_label_count = universe
            .get("expressive_label_candidates")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter(|item| {
                        item.get("public_surface_allowed")
                            .and_then(|v| v.as_bool())
                            .unwrap_or(false)
                            && item
                                .get("review_status")
                                .and_then(|v| v.as_str())
                                .unwrap_or("")
                                == "reviewed"
                    })
                    .count()
            })
            .unwrap_or(0);
        let creative_universe_candidate_count = creative_universe_candidates
            .get("candidate_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;

        let creative_profile_public_surface_allowed = universe
            .get("creative_profile_candidate_policy")
            .and_then(|v| v.get("public_surface_allowed"))
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let creative_universe_public_surface_allowed = creative_universe_candidates
            .get("public_surface_allowed")
            .and_then(|v| v.as_bool())
            .unwrap_or(false);
        let unreviewed_expressive_labels_public_surface_allowed = universe
            .get("expressive_label_candidates")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter().any(|item| {
                    item.get("public_surface_allowed")
                        .and_then(|v| v.as_bool())
                        .unwrap_or(false)
                        && item
                            .get("review_status")
                            .and_then(|v| v.as_str())
                            .unwrap_or("")
                            != "reviewed"
                })
            })
            .unwrap_or(false);

        Ok(serde_json::json!({
            "schema_version": "xiaoman-daily-run-manifest-v1",
            "source": "daily_case_report",
            "template_version": TEMPLATE_VERSION,
            "report_date": report.report_date,
            "time_range": report.time_range,
            "window_start": report.window_start,
            "window_end": report.window_end,
            "timezone": report.timezone,
            "source_chat_ref": source_chat_ref(source_chat_id),
            "inputs": {
                "message_count": report.message_count,
                "participant_count": report.participant_count,
                "latest_chat_records_preserved": true,
                "long_term_member_facts_used": long_term_member_facts_used,
                "reviewed_creative_profiles_used": reviewed_creative_profiles_used,
                "long_term_member_fact_text_included": false,
            },
            "outputs": {
                "poster": "generated_at_runtime",
                "daily_markdown": "private_review_file",
                "character_universe": "private_review_json",
                "quote_map": "private_review_json",
                "wiki_bundle": "private_review_json",
                "draft_bundle": "private_review_json",
                "review_report": "private_review_markdown",
            },
            "reference_workshop_steps": {
                "attachment_index": "omitted_no_reviewed_attachment_source",
                "media_prepare": "omitted_no_reviewed_attachment_source",
                "media_notes": "omitted_no_reviewed_attachment_source",
                "media_link_check": "omitted_no_reviewed_attachment_source",
                "weather_context": "omitted_no_reviewed_weather_source",
                "history_profiles": "reviewed_creative_profiles_or_member_fact_counts_only",
                "traceability": "quote_map_and_private_manifest_only",
                "raw_message_payload_read": false,
                "attachment_public_surface_allowed": false,
            },
            "counts": {
                "case_count": report.case_count,
                "character_count": report.character_count,
                "hot_topic_count": report.hot_topics.len(),
                "quote_map_entry_count": quote_map.get("entry_count").and_then(|v| v.as_u64()).unwrap_or(0),
                "wiki_people_count": wiki_counts.get("people").and_then(|v| v.as_u64()).unwrap_or(0),
                "wiki_event_count": wiki_counts.get("events").and_then(|v| v.as_u64()).unwrap_or(0),
                "wiki_storyline_count": wiki_counts.get("storylines").and_then(|v| v.as_u64()).unwrap_or(0),
                "draft_roast_profile_candidate_count": draft_counts.get("roast_profile_candidate_count").and_then(|v| v.as_u64()).unwrap_or(0),
                "draft_storyline_timeline_count": draft_counts.get("storyline_timeline_count").and_then(|v| v.as_u64()).unwrap_or(0),
                "draft_lookback_callback_count": draft_counts.get("lookback_callback_count").and_then(|v| v.as_u64()).unwrap_or(0),
                "creative_profile_candidate_count": creative_profile_candidates,
                "expressive_label_candidate_count": expressive_label_candidates,
                "reviewed_public_expressive_label_count": reviewed_public_expressive_label_count,
                "creative_universe_candidate_count": creative_universe_candidate_count,
            },
            "privacy": {
                "public_surface_allowed": false,
                "raw_message_rows_included": false,
                "profile_fact_text_included": false,
                "creative_profile_public_surface_allowed": creative_profile_public_surface_allowed,
                "creative_universe_public_surface_allowed": creative_universe_public_surface_allowed,
                "unreviewed_expressive_labels_public_surface_allowed": unreviewed_expressive_labels_public_surface_allowed,
                "writes_member_profile_snapshots": false,
                "raw_message_payload_read": false,
                "attachment_public_surface_allowed": false,
            },
            "review_required": true,
        }))
    }

    pub fn render_daily_markdown(report: &ReportData) -> String {
        let main_storyline = main_storyline_label(report);
        let callback_candidates = meme_callback_candidates(report, 5);
        let relationship_candidates = relationship_candidates(report, 4);
        let local_life_notes = ordinary_digest_local_life_notes(report);
        let open_questions = ordinary_digest_open_questions(report);

        let mut lines: Vec<String> = vec![
            format!("# 小满群聊日报｜{}｜{}", report.report_date, main_storyline),
            String::new(),
            "## 今日一句话".to_string(),
            String::new(),
            daily_opening_line(report),
            String::new(),
            "## 基本信息".to_string(),
            String::new(),
            format!("- 日期：{}", report.report_date),
            format!("- 时间范围：{}", report.time_range),
            format!("- 消息：{} 条", report.message_count),
            format!("- 活跃：{} 人", report.participant_count),
            format!("- 可归档主线：{} 条", report.case_count),
            format!("- 今日剧中人：{} 位", report.character_count),
            String::new(),
            "## 天气背景".to_string(),
            String::new(),
            "今日未接入已审核天气来源，公开日报不硬塞天气。".to_string(),
            String::new(),
        ];

        let topic_cards = ordinary_digest_topic_cards(report);
        if !topic_cards.is_empty() {
            lines.extend(["## 主要话题".to_string(), String::new()]);
            for topic in &topic_cards {
                let title = topic.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let summary = topic.get("summary").and_then(|v| v.as_str()).unwrap_or("");
                let participants = topic
                    .get("participants")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                lines.push(format!(
                    "- **{}**：{}；参与者 {} 人",
                    title, summary, participants
                ));
            }
            lines.push(String::new());
        }

        if let Some(highlight) = &report.highlight {
            lines.extend([
                "## 今日台词".to_string(),
                String::new(),
                format!("> {}", highlight),
                String::new(),
            ]);
        }

        if !report.characters.is_empty() {
            lines.extend(["## 今日剧中人".to_string(), String::new()]);
            for character in &report.characters {
                let memory = if character.memory_label.is_empty() {
                    String::new()
                } else {
                    format!("（{}）", character.memory_label)
                };
                lines.push(format!(
                    "- **{}（{}）**：{}。{}。{}{}",
                    character.name,
                    character.role_label,
                    character.story_function,
                    if character.arc_label.is_empty() {
                        character.one_liner.clone()
                    } else {
                        character.arc_label.clone()
                    },
                    character.callback_hint,
                    memory
                ));
                lines.push(format!("> {}", character.evidence));
                lines.push(String::new());
                if !character.relationship_hint.is_empty() {
                    lines.push(format!("  同场接力：{}", character.relationship_hint));
                    lines.push(String::new());
                }
                if !character.expressive_label.is_empty() {
                    lines.push(format!("  已审核公开标签：{}", character.expressive_label));
                    lines.push(String::new());
                }
            }
        }

        if !callback_candidates.is_empty() {
            lines.extend(["## 梗和回调候选".to_string(), String::new()]);
            for candidate in &callback_candidates {
                lines.push(format!("- {}", candidate));
            }
            lines.push(String::new());
        }

        if !relationship_candidates.is_empty() {
            lines.extend(["## 同场关系".to_string(), String::new()]);
            for candidate in &relationship_candidates {
                lines.push(format!("- {}", candidate));
            }
            lines.push(String::new());
        }

        if !local_life_notes.is_empty() {
            lines.extend(["## 地点 / 本地生活线索".to_string(), String::new()]);
            for item in &local_life_notes {
                let label = item.get("label").and_then(|v| v.as_str()).unwrap_or("");
                let source = item.get("source").and_then(|v| v.as_str()).unwrap_or("");
                lines.push(format!("- {}（{}）", label, source));
            }
            lines.push(String::new());
        }

        if !open_questions.is_empty() {
            lines.extend(["## 待解决问题".to_string(), String::new()]);
            for question in &open_questions {
                lines.push(format!("- {}", question));
            }
            lines.push(String::new());
        }

        let candidate_topics = ordinary_digest_candidate_topics(report);
        if !candidate_topics.is_empty() {
            lines.extend(["## 候选公众号选题".to_string(), String::new()]);
            for topic in &candidate_topics {
                let title = topic.get("title").and_then(|v| v.as_str()).unwrap_or("");
                let reason = topic.get("reason").and_then(|v| v.as_str()).unwrap_or("");
                lines.push(format!("- {}：{}", title, reason));
            }
            lines.push(String::new());
        }

        if !report.cases.is_empty() {
            lines.extend(["## 今日主线".to_string(), String::new()]);
            for case in &report.cases {
                lines.extend([
                    format!("### {}｜{}", case.case_no, case_storyline_label(case)),
                    String::new(),
                    format!("- 时间：{}", case.time_label),
                    format!("- 规模：{}", case.summary),
                    format!("- 主讲：{}", case.top_speaker),
                    String::new(),
                ]);
                for bullet in case.bullets.iter().take(3) {
                    lines.push(format!("- {}", bullet));
                }
                lines.push(String::new());
            }
        }

        if !report.suspects.is_empty() {
            lines.extend(["## 发言出场榜".to_string(), String::new()]);
            for suspect in &report.suspects {
                lines.push(format!(
                    "- {}. {}：{} 条 / {} 字",
                    suspect.rank, suspect.name, suspect.message_count, suspect.word_count
                ));
            }
            lines.push(String::new());
        }

        let universe = &report.character_universe;
        if let Some(storyline_candidates) = universe
            .get("storyline_candidates")
            .and_then(|v| v.as_array())
        {
            if !storyline_candidates.is_empty() {
                lines.extend(["## 可沉淀故事线".to_string(), String::new()]);
                for item in storyline_candidates.iter().take(5) {
                    let label = item.get("label").and_then(|v| v.as_str()).unwrap_or("");
                    let reason = item.get("reason").and_then(|v| v.as_str()).unwrap_or("");
                    lines.push(format!("- [[{}]]：{}", label, reason));
                }
                lines.push(String::new());
            }
        }

        if let Some(profile_candidates) = universe
            .get("creative_profile_candidates")
            .and_then(|v| v.as_array())
        {
            if !profile_candidates.is_empty() {
                lines.extend(["## 可审核人物画像候选".to_string(), String::new()]);
                for item in profile_candidates.iter().take(5) {
                    let role = item
                        .get("candidate_role_label")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let function = item
                        .get("story_function")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let arc = item.get("daily_arc").and_then(|v| v.as_str()).unwrap_or("");
                    let status = item
                        .get("profile_upgrade_status")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    let evidence_count = item
                        .get("recurrence_evidence_count")
                        .and_then(|v| v.as_u64())
                        .unwrap_or(0);
                    let policy = item
                        .get("evidence_policy")
                        .and_then(|v| v.as_str())
                        .unwrap_or("");
                    lines.push(format!(
                        "- {} / {}：{}（{}；evidence_count={}；{}）",
                        role, function, arc, status, evidence_count, policy
                    ));
                }
                lines.push(String::new());
            }
        }

        lines.extend([
            "## 公开边界".to_string(),
            String::new(),
            "- 本日报由小满根据最新群聊窗口自动整理。".to_string(),
            "- 长期画像只以角色复现计数参与，不展示内部画像原文。".to_string(),
            "- creative_profile_candidates 仅供内部审核，不写入长期画像表，不允许直接公开展示。".to_string(),
            "- expressive_label_candidates 只有 owner-reviewed safe_reply_hints 字段可进入公开文案。".to_string(),
            "- raw_messages_included=false；profile_fact_text_included=false。".to_string(),
        ]);

        lines.join("\n")
    }

    pub fn render_review_report(
        report: &ReportData,
        quote_map: &serde_json::Value,
        wiki_bundle: &serde_json::Value,
        draft_bundle: &serde_json::Value,
        run_manifest: &serde_json::Value,
    ) -> String {
        let universe = &report.character_universe;
        let wiki_counts = wiki_bundle
            .get("counts")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        let draft_counts = draft_bundle
            .get("counts")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));
        let profile_candidates = universe
            .get("creative_profile_candidates")
            .and_then(|v| v.as_array())
            .cloned()
            .unwrap_or_default();
        let creative_universe_candidates = universe
            .get("creative_universe_candidates")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        let reviewed_creative_profiles_count = report
            .characters
            .iter()
            .filter(|character| character.creative_profile_status == "active_reviewed")
            .count();
        let quote_map_entry_count = quote_map
            .get("entry_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let wiki_people_count = wiki_counts
            .get("people")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let wiki_events_count = wiki_counts
            .get("events")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let wiki_memes_count = wiki_counts
            .get("memes")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let wiki_relationships_count = wiki_counts
            .get("relationships")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let wiki_storylines_count = wiki_counts
            .get("storylines")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let roast_profile_candidate_count = draft_counts
            .get("roast_profile_candidate_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let storyline_timeline_count = draft_counts
            .get("storyline_timeline_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let lookback_callback_count = draft_counts
            .get("lookback_callback_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let creative_universe_candidate_count = creative_universe_candidates
            .get("candidate_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0);
        let reviewed_public_expressive_label_count = run_manifest
            .get("counts")
            .and_then(|v| v.get("reviewed_public_expressive_label_count"))
            .and_then(|v| v.as_u64())
            .unwrap_or(0);

        let privacy = run_manifest
            .get("privacy")
            .cloned()
            .unwrap_or(serde_json::Value::Object(serde_json::Map::new()));

        let mut lines: Vec<String> = vec![
            format!("# 小满日报私有审核包｜{}", report.report_date),
            String::new(),
            "## 生成范围".to_string(),
            String::new(),
            format!("- 时间范围：{}", report.time_range),
            format!(
                "- 最新聊天记录：保留，{} 条消息 / {} 位活跃成员",
                report.message_count, report.participant_count
            ),
            format!(
                "- 已审核创意画像复用：{} 位",
                reviewed_creative_profiles_count
            ),
            format!("- 今日主线：{} 条", report.case_count),
            format!("- 今日剧中人：{} 位", report.character_count),
            format!("- 引用映射：{} 条候选证据", quote_map_entry_count),
            format!(
                "- Wiki 候选：people={} / events={} / memes={} / relationships={} / storylines={}",
                wiki_people_count,
                wiki_events_count,
                wiki_memes_count,
                wiki_relationships_count,
                wiki_storylines_count
            ),
            format!(
                "- 草稿候选：roast_profiles={} / storyline_timeline={} / lookback_callbacks={}",
                roast_profile_candidate_count, storyline_timeline_count, lookback_callback_count
            ),
            format!(
                "- 创作资产候选：{} 条（梗 / 关系标签 / 时间线，仅供审核）",
                creative_universe_candidate_count
            ),
            format!(
                "- 已审核公开表达标签：{} 条",
                reviewed_public_expressive_label_count
            ),
            String::new(),
            "## 审核清单".to_string(),
            String::new(),
            "- [ ] 公开日报是否只使用群聊窗口内的当日内容和安全衍生标签".to_string(),
            "- [ ] 已审核 creative_profile 只作为风格/回调提示，不能覆盖当日消息证据".to_string(),
            "- [ ] 今日剧中人的角色是否有 quote-map 或 case bullet 支撑".to_string(),
            "- [ ] eligible_for_review 是否满足最小复现证据；daily_note_only 不得写入长期画像"
                .to_string(),
            "- [ ] creative_profile_candidates 是否仍为 candidate_only，没有写入长期画像表"
                .to_string(),
            "- [ ] 同名成员是否按 person_id 优先分组，缺失 person_id 才使用展示名兜底".to_string(),
            "- [ ] meme / relationship / storyline 是否只是候选，没有被当作事实发布".to_string(),
            "- [ ] roast/public draft 是否仍为 owner-review 候选，没有进入自动公开发送面"
                .to_string(),
            "- [ ] 附件/图片素材步骤是否仍为 omitted，未读取 raw payload 或猜测图片内容"
                .to_string(),
            String::new(),
            "## 隐私边界".to_string(),
            String::new(),
            format!(
                "- raw_message_rows_included={}",
                privacy_bool(&privacy, "raw_message_rows_included")
            ),
            format!(
                "- profile_fact_text_included={}",
                privacy_bool(&privacy, "profile_fact_text_included")
            ),
            format!(
                "- creative_profile_public_surface_allowed={}",
                privacy_bool(&privacy, "creative_profile_public_surface_allowed")
            ),
            format!(
                "- creative_universe_public_surface_allowed={}",
                privacy_bool(&privacy, "creative_universe_public_surface_allowed")
            ),
            format!(
                "- unreviewed_expressive_labels_public_surface_allowed={}",
                privacy_bool(
                    &privacy,
                    "unreviewed_expressive_labels_public_surface_allowed"
                )
            ),
            format!(
                "- writes_member_profile_snapshots={}",
                privacy_bool(&privacy, "writes_member_profile_snapshots")
            ),
            format!(
                "- raw_message_payload_read={}",
                privacy_bool(&privacy, "raw_message_payload_read")
            ),
            format!(
                "- attachment_public_surface_allowed={}",
                privacy_bool(&privacy, "attachment_public_surface_allowed")
            ),
            String::new(),
            "## 可审核人物画像候选".to_string(),
            String::new(),
        ];

        if !profile_candidates.is_empty() {
            for item in profile_candidates.iter().take(8) {
                let related_person = item
                    .get("related_person")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let candidate_role_label = item
                    .get("candidate_role_label")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let story_function = item
                    .get("story_function")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let daily_arc = item.get("daily_arc").and_then(|v| v.as_str()).unwrap_or("");
                let profile_upgrade_status = item
                    .get("profile_upgrade_status")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                let recurrence_evidence_count = item
                    .get("recurrence_evidence_count")
                    .and_then(|v| v.as_u64())
                    .unwrap_or(0);
                let evidence_anchor = item
                    .get("evidence_anchor")
                    .and_then(|v| v.as_str())
                    .unwrap_or("");
                lines.push(format!(
                    "- {}：{} / {} / {}（status={}; evidence_count={}; anchor={}）",
                    related_person,
                    candidate_role_label,
                    story_function,
                    daily_arc,
                    profile_upgrade_status,
                    recurrence_evidence_count,
                    evidence_anchor
                ));
            }
        } else {
            lines.push("- 今日没有形成可审核人物画像候选。".to_string());
        }

        lines.extend([
            String::new(),
            "## 产物策略".to_string(),
            String::new(),
            "- 画报和日报 Markdown 用于人工查看。".to_string(),
            "- 已审核 `creative_profile` 只读取 safe_reply_hints / communication_style 的安全字段，不读取 summary。".to_string(),
            "- quote-map / wiki-bundle / run-manifest 只用于内部审核和后续人工确认。".to_string(),
            "- draft-bundle 承载普通日报、轻吐槽素材和公众号候选素材，但只作为 owner review 输入。".to_string(),
            "- worker-run evidence 只能保留 presence/count/privacy flags，不能保留 quote、wiki 节点正文或人物画像文本。".to_string(),
        ]);

        lines.join("\n")
    }

    fn privacy_bool(privacy: &serde_json::Value, key: &str) -> String {
        privacy
            .get(key)
            .and_then(|v| v.as_bool())
            .map(|b| b.to_string().to_lowercase())
            .unwrap_or_else(|| "false".to_string())
    }
}

// ---------------------------------------------------------------------------
// HTML templates
// ---------------------------------------------------------------------------

mod templates {
    #![allow(dead_code)]

    use super::*;

    const CASE_CARD_COLORS: &[(&str, &str)] = &[
        ("#fef3c7", "#92400e"), // amber
        ("#fee2e2", "#991b1b"), // red
        ("#dbeafe", "#1e40af"), // blue
        ("#dcfce7", "#166534"), // green
        ("#f3e8ff", "#6b21a8"), // purple
        ("#ffedd5", "#9a3412"), // orange
    ];

    /// Match Python `html.escape` default behavior: escape `&`, `<`, `>`, `"`.
    pub fn escape_html(text: &str) -> String {
        text.chars()
            .map(|c| match c {
                '&' => "&amp;".to_string(),
                '<' => "&lt;".to_string(),
                '>' => "&gt;".to_string(),
                '"' => "&quot;".to_string(),
                _ => c.to_string(),
            })
            .collect()
    }

    /// Escape HTML then convert the small inline-Markdown subset the roast
    /// narrative uses (`**bold**`, `*italic*`, `` `code` ``) into real tags.
    ///
    /// The LLM roast narrative is authored in Markdown, but the long-image
    /// renderer emits HTML. Without this, raw `**` markers leak into the
    /// poster (visible as stray asterisks) instead of rendering as bold.
    ///
    /// Conversion is strictly pair-based: a marker only becomes a tag when a
    /// matching closing marker exists later in the same text. Lone or
    /// unmatched markers (e.g. the `*` in `2 * 3`, a wildcard, or an
    /// unterminated `**bold`) are emitted as plain escaped text, so they can
    /// never swallow characters or inject an unclosed tag that breaks the
    /// rest of the poster. Inline code is dropped to plain text (no `<code>`
    /// styling in the poster), matching the old Pillow `_strip_md_inline`
    /// behaviour of keeping the text but dropping the markers.
    pub fn render_inline(text: &str) -> String {
        fn push_escaped(out: &mut String, c: char) {
            match c {
                '&' => out.push_str("&amp;"),
                '<' => out.push_str("&lt;"),
                '>' => out.push_str("&gt;"),
                '"' => out.push_str("&quot;"),
                _ => out.push(c),
            }
        }

        // Find the byte index of the matching closer for a marker that opens
        // at `from`. `delim` is either "*" or "**"; a closer must contain at
        // least one non-delimiter, non-whitespace character before it so we do
        // not treat `** **` or `* *` as a valid span. For the single `*`
        // delimiter the closer must not be part of a `**` run, otherwise the
        // `*` in `2 * 3 **bold**` would greedily swallow text up to the bold
        // marker.
        fn find_closer(text: &str, from: usize, delim: &str) -> Option<usize> {
            let mut idx = from;
            while let Some(pos) = text[idx..].find(delim) {
                let abs = idx + pos;
                if delim == "*" {
                    let after = &text[abs + 1..];
                    let before_ok = !text[..abs].ends_with('*');
                    let after_ok = !after.starts_with('*');
                    if !(before_ok && after_ok) {
                        idx = abs + 1;
                        continue;
                    }
                }
                let between = &text[idx..abs];
                if between.chars().any(|c| !c.is_whitespace() && c != '*') {
                    return Some(abs);
                }
                idx = abs + delim.len();
            }
            None
        }

        let mut out = String::with_capacity(text.len());
        let mut i = 0;
        let len = text.len();
        while i < len {
            let rest = &text[i..];
            if rest.starts_with('`') {
                // Toggle inline code only when a closing backtick exists;
                // otherwise emit the backtick as a normal character.
                if let Some(close) = rest[1..].find('`') {
                    let inner = &rest[1..1 + close];
                    for c in inner.chars() {
                        push_escaped(&mut out, c);
                    }
                    i += 1 + close + 1;
                } else {
                    out.push('`');
                    i += 1;
                }
                continue;
            }
            if rest.starts_with("**") {
                if let Some(close) = find_closer(text, i + 2, "**") {
                    out.push_str("<strong>");
                    let inner = &text[i + 2..close];
                    for c in inner.chars() {
                        push_escaped(&mut out, c);
                    }
                    out.push_str("</strong>");
                    i = close + 2;
                } else {
                    out.push_str("**");
                    i += 2;
                }
                continue;
            }
            if rest.starts_with('*') {
                if let Some(close) = find_closer(text, i + 1, "*") {
                    out.push_str("<em>");
                    let inner = &text[i + 1..close];
                    for c in inner.chars() {
                        push_escaped(&mut out, c);
                    }
                    out.push_str("</em>");
                    i = close + 1;
                } else {
                    out.push('*');
                    i += 1;
                }
                continue;
            }
            let c = rest.chars().next().unwrap();
            push_escaped(&mut out, c);
            i += c.len_utf8();
        }
        out
    }

    /// Match Python `_bar_svg` exactly, including integer truncation of bar heights.
    pub fn bar_svg(counts: &[i32], max_count: i32, width: usize, height: usize) -> String {
        if counts.is_empty() || max_count == 0 {
            return String::new();
        }
        let bar_width = width / counts.len();
        let gap = 2usize;
        let effective_width = bar_width.saturating_sub(gap).max(1);
        let mut bars = Vec::new();
        for (idx, &count) in counts.iter().enumerate() {
            let h = ((count as f64 / max_count as f64) * height as f64) as usize;
            let x = idx * bar_width + gap / 2;
            let y = height.saturating_sub(h);
            bars.push(format!(
                r##"<rect x="{}" y="{}" width="{}" height="{}" rx="2" fill="#1a2744"/>"##,
                x, y, effective_width, h
            ));
        }
        bars.join("\n")
    }

    // -----------------------------------------------------------------------
    // Roast long-image
    // -----------------------------------------------------------------------

    #[derive(Debug, Default)]
    struct RoastChapter {
        title: String,
        paragraphs: Vec<String>,
        golden_quote: String,
    }

    #[derive(Debug, Default)]
    struct RoastCharacter {
        name: String,
        desc: String,
    }

    #[derive(Debug, Default)]
    struct FinalQuote {
        text: String,
        author: String,
    }

    #[derive(Debug, Default)]
    struct RoastNarrative {
        kicker: String,
        date_line: String,
        title: String,
        war_report: String,
        chapters: Vec<RoastChapter>,
        tomorrow: String,
        characters: Vec<RoastCharacter>,
        final_quote: FinalQuote,
    }

    fn split_title_line(line: &str) -> (String, String, String) {
        let text = line.trim_start_matches('#').trim();
        let parts: Vec<&str> = text.split('|').map(str::trim).collect();
        if parts.len() >= 3 {
            (
                parts[0].to_string(),
                parts[1].to_string(),
                parts[2].to_string(),
            )
        } else if parts.len() == 2 {
            (parts[0].to_string(), parts[1].to_string(), String::new())
        } else {
            (text.to_string(), String::new(), String::new())
        }
    }

    fn parse_roast_narrative(md: &str) -> RoastNarrative {
        let lines: Vec<&str> = md.lines().collect();
        let mut cleaned: Vec<&str> = Vec::new();
        for line in &lines {
            let stripped = line.trim();
            if stripped == "---" {
                continue;
            }
            if stripped.starts_with('*') && stripped.ends_with('*') && stripped.contains("吐槽") {
                continue;
            }
            cleaned.push(line);
        }
        let mut text = cleaned.join("\n");

        let mut kicker = String::new();
        let mut date_line = String::new();
        let mut title = String::new();

        // Extract `# title line`.
        let title_re = regex::Regex::new(r"(?m)^#\s+(.+)$").unwrap();
        if let Some(cap) = title_re.captures(&text) {
            let line = cap.get(1).unwrap().as_str();
            (kicker, date_line, title) = split_title_line(line);
            let end = cap.get(0).unwrap().end();
            text = text[end..].trim_start_matches('\n').to_string();
        }

        // Extract `**战报**：...`.
        let mut war_report = String::new();
        let war_re = regex::Regex::new(r"(?m)^\*\*战报\*\*[：:]\s*(.+)$").unwrap();
        if let Some(cap) = war_re.captures(&text) {
            war_report = cap.get(1).unwrap().as_str().trim().to_string();
            let start = cap.get(0).unwrap().start();
            let end = cap.get(0).unwrap().end();
            text = format!("{}{}", &text[..start], &text[end..]);
            text = text.trim_start_matches('\n').to_string();
        }

        // Split by `## ` sections.
        let section_re = regex::Regex::new(r"(?m)^##\s+").unwrap();
        let raw_sections: Vec<&str> = section_re.split(&text).collect();

        let mut chapters: Vec<RoastChapter> = Vec::new();
        let mut tomorrow = String::new();
        let mut characters: Vec<RoastCharacter> = Vec::new();
        let mut final_quote = FinalQuote::default();

        let chapter_re = regex::Regex::new(r"第.{1,3}章").unwrap();
        for section in raw_sections.iter().skip(1) {
            if section.trim().is_empty() {
                continue;
            }
            let (section_title, section_body) = match section.find('\n') {
                Some(idx) => (section[..idx].trim(), section[idx + 1..].trim()),
                None => (section.trim(), ""),
            };
            if chapter_re.is_match(section_title) {
                chapters.push(parse_roast_chapter(section_title, section_body));
            } else if section_title.contains("明日") || section_title.contains("前瞻") {
                tomorrow = section_body.to_string();
            } else if section_title.contains("人物速写") || section_title.contains("人物") {
                characters = parse_roast_characters(section_body);
            } else if section_title.contains("金句") || section_title.contains("最佳") {
                final_quote = parse_roast_final_quote(section_body);
            }
        }

        RoastNarrative {
            kicker,
            date_line,
            title,
            war_report,
            chapters,
            tomorrow,
            characters,
            final_quote,
        }
    }

    /// Build a deterministic roast narrative structure from the report data.
    ///
    /// Mirrors Python `roast_long_image.build_fallback_parsed`: when the LLM
    /// narrative is unavailable we still render the roast layout, but the text
    /// is assembled from deterministic report data instead of LLM prose.
    fn build_fallback_narrative(report: &ReportData) -> RoastNarrative {
        let group = if report.group_name.is_empty() {
            "秦托邦"
        } else {
            &report.group_name
        };
        let war_report = format!(
            "{}条消息 · {}人开口",
            report.message_count, report.participant_count
        );

        let chapters: Vec<RoastChapter> = report
            .cases
            .iter()
            .take(5)
            .filter_map(|case| {
                let mut paragraphs = vec![case.summary.clone()];
                paragraphs.extend(case.bullets.iter().cloned());
                paragraphs.retain(|p| !p.trim().is_empty());
                if paragraphs.is_empty() {
                    return None;
                }
                Some(RoastChapter {
                    title: if case.title.is_empty() {
                        "当日话题".to_string()
                    } else {
                        case.title.clone()
                    },
                    paragraphs,
                    golden_quote: String::new(),
                })
            })
            .collect();

        let characters: Vec<RoastCharacter> = report
            .characters
            .iter()
            .take(6)
            .map(|c| {
                let mut desc = if !c.one_liner.is_empty() {
                    c.one_liner.clone()
                } else {
                    c.story_function.clone()
                };
                if !c.role_label.is_empty() {
                    desc = if desc.is_empty() {
                        c.role_label.clone()
                    } else {
                        format!("{}｜{}", c.role_label, desc)
                    };
                }
                RoastCharacter {
                    name: c.name.clone(),
                    desc,
                }
            })
            .collect();

        let final_quote = report
            .highlight
            .as_ref()
            .map(|h| FinalQuote {
                text: h.clone(),
                author: String::new(),
            })
            .unwrap_or_default();

        RoastNarrative {
            kicker: format!("{}吐槽日报", group),
            date_line: report.report_date.clone(),
            title: "今日群聊观察".to_string(),
            war_report,
            chapters,
            tomorrow: String::new(),
            characters,
            final_quote,
        }
    }

    fn parse_roast_chapter(title: &str, body: &str) -> RoastChapter {
        let mut paragraphs: Vec<String> = Vec::new();
        let mut golden_quote = String::new();
        let blocks: Vec<&str> = body.split("\n\n").collect();
        let q1 = regex::Regex::new(r"^\*\*金句[：:]\s*(.+)\*\*$").unwrap();
        let q2 = regex::Regex::new(r"^金句[：:]\s*\*\*(.+)\*\*$").unwrap();
        let q3 = regex::Regex::new(r"^金句[：:]\s*(.+)$").unwrap();
        for block in blocks {
            let block = block.trim();
            if block.is_empty() {
                continue;
            }
            if block.starts_with("![[") {
                continue;
            }
            if let Some(cap) = q1.captures(block) {
                golden_quote = cap
                    .get(1)
                    .unwrap()
                    .as_str()
                    .trim()
                    .trim_end_matches('*')
                    .to_string();
                continue;
            }
            if let Some(cap) = q2.captures(block) {
                golden_quote = cap
                    .get(1)
                    .unwrap()
                    .as_str()
                    .trim()
                    .trim_end_matches('*')
                    .to_string();
                continue;
            }
            if let Some(cap) = q3.captures(block) {
                golden_quote = cap
                    .get(1)
                    .unwrap()
                    .as_str()
                    .trim()
                    .trim_end_matches('*')
                    .to_string();
                continue;
            }
            if block.starts_with('_') && block.ends_with('_') && block.contains('—') {
                continue;
            }
            paragraphs.push(block.to_string());
        }
        RoastChapter {
            title: title.to_string(),
            paragraphs,
            golden_quote,
        }
    }

    fn parse_roast_characters(body: &str) -> Vec<RoastCharacter> {
        let mut characters: Vec<RoastCharacter> = Vec::new();
        let blocks: Vec<&str> = body.split("\n\n").collect();
        let name_re = regex::Regex::new(r"\*\*(.+?)\*\*").unwrap();
        for block in blocks {
            let block = block.trim();
            if !block.starts_with('>') {
                continue;
            }
            let lines: Vec<&str> = block
                .lines()
                .map(|l| l.trim_start_matches('>').trim())
                .collect();
            if lines.is_empty() {
                continue;
            }
            let first = lines[0];
            if let Some(cap) = name_re.captures(first) {
                let name = cap.get(1).unwrap().as_str().to_string();
                let desc = lines[1..].join(" ").trim().to_string();
                characters.push(RoastCharacter { name, desc });
            }
        }
        characters
    }

    fn parse_roast_final_quote(body: &str) -> FinalQuote {
        let body = body.trim();
        let re = regex::Regex::new(r#"^\*\*\"(.+?)\"\*\*\s*[—\-]{1,3}\s*(.+)$"#).unwrap();
        if let Some(cap) = re.captures(body) {
            return FinalQuote {
                text: cap.get(1).unwrap().as_str().trim().to_string(),
                author: cap.get(2).unwrap().as_str().trim().to_string(),
            };
        }
        FinalQuote {
            text: body.to_string(),
            author: String::new(),
        }
    }

    fn roast_css(width: usize) -> String {
        format!(
            r#"  * {{ margin: 0; padding: 0; box-sizing: border-box; }}

  body {{
    width: {width}px;
    background: #fbfaf6;
    font-family: "Songti SC", "Noto Serif SC", "STSong", "SimSun", serif;
    color: #2a2a2a;
    line-height: 2.0;
    padding: 70px 110px 60px;
  }}

  .kicker {{
    text-align: center;
    font-size: 18px;
    letter-spacing: 10px;
    color: #8b1f2f;
    font-weight: 600;
    margin-bottom: 18px;
  }}

  .title {{
    text-align: center;
    font-size: 38px;
    font-weight: 700;
    color: #1a1a1a;
    line-height: 1.5;
    margin-bottom: 14px;
  }}

  .date-line {{
    text-align: center;
    font-size: 17px;
    color: #999;
    margin-bottom: 36px;
    letter-spacing: 1px;
  }}

  .divider-top {{
    border: none;
    border-top: 4px solid #8b1f2f;
    margin: 0 0 36px;
  }}
  .divider-top::after {{
    content: "";
    display: block;
    border-top: 1px solid #8b1f2f;
    margin-top: 4px;
  }}

  .war-report {{
    background: #f5f0e8;
    border-left: 5px solid #8b1f2f;
    padding: 24px 28px;
    font-size: 20px;
    line-height: 2.0;
    margin-bottom: 42px;
    border-radius: 0 8px 8px 0;
  }}
  .war-report .label {{
    font-size: 15px;
    color: #8b1f2f;
    font-weight: 600;
    letter-spacing: 3px;
    margin-bottom: 8px;
  }}

  .chapter {{
    margin-bottom: 40px;
  }}

  .chapter h2 {{
    font-size: 26px;
    font-weight: 700;
    color: #1a1a1a;
    margin-bottom: 16px;
    padding-left: 16px;
    border-left: 5px solid #8b1f2f;
    line-height: 1.5;
  }}

  .chapter p {{
    font-size: 21px;
    line-height: 2.05;
    margin-bottom: 14px;
    text-align: justify;
  }}

  .golden-quote {{
    font-size: 20px;
    font-weight: 600;
    color: #8b1f2f;
    margin-top: 18px;
    padding: 12px 0;
    border-top: 1px solid #e0d5c5;
    border-bottom: 1px solid #e0d5c5;
    text-align: center;
  }}

  .tomorrow {{
    background: #f0ede5;
    padding: 24px 28px;
    border-radius: 10px;
    margin-bottom: 42px;
  }}
  .tomorrow h2 {{
    font-size: 23px;
    color: #8b1f2f;
    margin-bottom: 10px;
  }}
  .tomorrow p {{
    font-size: 20px;
    line-height: 2.0;
  }}

  .characters {{
    margin-bottom: 42px;
  }}
  .characters h2 {{
    font-size: 26px;
    font-weight: 700;
    margin-bottom: 20px;
    padding-left: 16px;
    border-left: 5px solid #8b1f2f;
  }}
  .char-card {{
    background: #fff;
    border: 1px solid #e8e0d0;
    border-radius: 10px;
    padding: 18px 24px;
    margin-bottom: 14px;
  }}
  .char-card .name {{
    font-size: 20px;
    font-weight: 700;
    color: #8b1f2f;
    margin-bottom: 6px;
  }}
  .char-card .desc {{
    font-size: 19px;
    line-height: 1.9;
    color: #444;
  }}

  .final-quote {{
    text-align: center;
    margin-bottom: 42px;
    padding: 28px;
    background: linear-gradient(135deg, #8b1f2f 0%, #6b1722 100%);
    border-radius: 12px;
    color: #fff;
  }}
  .final-quote .label {{
    font-size: 15px;
    letter-spacing: 5px;
    opacity: 0.85;
    margin-bottom: 10px;
  }}
  .final-quote .text {{
    font-size: 24px;
    font-weight: 600;
    line-height: 1.6;
    margin-bottom: 10px;
  }}
  .final-quote .author {{
    font-size: 17px;
    opacity: 0.85;
  }}

  .divider-bottom {{
    border: none;
    border-top: 1px solid #d0c8b8;
    margin: 24px 0;
  }}
  .footer {{
    text-align: center;
    font-size: 15px;
    color: #aaa;
    line-height: 1.8;
  }}"#
        )
    }

    pub fn render_roast_long_image(input: &RenderInput) -> Result<String> {
        let parsed = match input.narrative_md.as_deref() {
            Some(md) if !md.trim().is_empty() => parse_roast_narrative(md),
            _ => {
                // Deterministic fallback: when the LLM narrative is unavailable,
                // build the same roast layout from the deterministic report data.
                build_fallback_narrative(&input.report)
            }
        };
        let width = input.width;
        let kicker = if parsed.kicker.is_empty() {
            "秦托邦吐槽日报"
        } else {
            &parsed.kicker
        };
        let kicker_spaced = kicker
            .chars()
            .map(|c| c.to_string())
            .collect::<Vec<_>>()
            .join(" ");
        let title = if parsed.title.is_empty() {
            "今日群聊观察".to_string()
        } else {
            parsed.title
        };
        let date_line = if parsed.date_line.is_empty() {
            input.report.report_date.clone()
        } else {
            parsed.date_line
        };

        let war_html = if parsed.war_report.is_empty() {
            String::new()
        } else {
            format!(
                r#"<div class="war-report">
  <div class="label">战 报</div>
  {}
</div>
"#,
                render_inline(&parsed.war_report)
            )
        };

        let mut chapters_html = String::new();
        for ch in &parsed.chapters {
            let paras = ch
                .paragraphs
                .iter()
                .map(|p| format!("  <p>{}</p>\n", render_inline(p)))
                .collect::<String>();
            let gq = if ch.golden_quote.is_empty() {
                String::new()
            } else {
                format!(
                    r#"  <div class="golden-quote">{}</div>
"#,
                    render_inline(&ch.golden_quote)
                )
            };
            chapters_html.push_str(&format!(
                r#"<div class="chapter">
  <h2>{}</h2>
{}{}</div>
"#,
                render_inline(&ch.title),
                paras,
                gq
            ));
        }

        let tomorrow_html = if parsed.tomorrow.is_empty() {
            String::new()
        } else {
            format!(
                r#"  <div class="tomorrow">
  <h2>明日线索</h2>
  <p>{}</p>
</div>
"#,
                render_inline(&parsed.tomorrow)
            )
        };

        let characters_html = if parsed.characters.is_empty() {
            String::new()
        } else {
            let cards = parsed
                .characters
                .iter()
                .map(|c| {
                    format!(
                        r#"    <div class="char-card">
      <div class="name">{}</div>
      <div class="desc">{}</div>
    </div>
"#,
                        render_inline(&c.name),
                        render_inline(&c.desc)
                    )
                })
                .collect::<String>();
            format!(
                r#"<div class="characters">
  <h2>今日人物速写</h2>
{}</div>
"#,
                cards
            )
        };

        let final_quote_html = if parsed.final_quote.text.is_empty() {
            String::new()
        } else {
            let author = if parsed.final_quote.author.is_empty() {
                String::new()
            } else {
                format!("—— {}", render_inline(&parsed.final_quote.author))
            };
            format!(
                r#"<div class="final-quote">
  <div class="label">今 日 金 句</div>
  <div class="text">"{}"</div>
  <div class="author">{}</div>
</div>
"#,
                render_inline(&parsed.final_quote.text),
                author
            )
        };

        let css = roast_css(width);
        Ok(format!(
            r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="UTF-8">
<meta name="viewport" content="width=device-width, initial-scale=1.0">
<style>
{css}
</style>
</head>
<body>
  <div class="kicker">{}</div>
  <h1 class="title">{}</h1>
  <div class="date-line">{}</div>
  <hr class="divider-top">
  {}  {}{}    {}  {}  <hr class="divider-bottom">
  <div class="footer">
    秦托邦 · 小满吐槽日报<br>
    所有引用可回溯至当天 quote-map
  </div>
</body>
</html>"#,
            escape_html(&kicker_spaced),
            escape_html(&title),
            escape_html(&date_line),
            war_html,
            chapters_html,
            tomorrow_html,
            characters_html,
            final_quote_html
        ))
    }

    // -----------------------------------------------------------------------
    // Newspaper elegant
    // -----------------------------------------------------------------------

    fn build_newspaper_elegant_input(report: &ReportData, width: usize) -> serde_json::Value {
        let universe = &report.character_universe;
        let hourly = &report.hourly_counts;
        let peak_count = hourly.iter().copied().max().unwrap_or(0);
        let max_hourly = peak_count.max(1);
        let chart_w = (width as i32 - 120).max(120) as usize;
        let hourly_truncated: Vec<i32> = hourly.iter().copied().take(24).collect();
        let hourly_svg = bar_svg(&hourly_truncated, max_hourly, chart_w, 90);

        let topic_cards = assembly::ordinary_digest_topic_cards(report);
        let open_questions = assembly::ordinary_digest_open_questions(report);
        let local_life_notes = assembly::ordinary_digest_local_life_notes(report);

        let characters: Vec<serde_json::Value> = report
            .characters
            .iter()
            .take(6)
            .map(|c| {
                serde_json::json!({
                    "name": c.name,
                    "role": c.role_label,
                    "evidence": c.evidence,
                    "rank": c.rank,
                })
            })
            .collect();

        let callbacks: Vec<String> = universe
            .get("callbacks")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|cb| {
                        cb.as_object()
                            .and_then(|o| o.get("label"))
                            .and_then(|v| v.as_str())
                            .map(String::from)
                    })
                    .filter(|s| !s.is_empty())
                    .collect()
            })
            .unwrap_or_default();

        let relationships: Vec<String> = universe
            .get("relationships")
            .and_then(|v| v.as_array())
            .map(|arr| {
                arr.iter()
                    .filter_map(|r| {
                        let o = r.as_object()?;
                        let label = o.get("label").and_then(|v| v.as_str());
                        if let Some(label) = label {
                            if !label.is_empty() {
                                return Some(label.to_string());
                            }
                        }
                        let source = o.get("source").and_then(|v| v.as_str()).unwrap_or("");
                        let target = o.get("target").and_then(|v| v.as_str()).unwrap_or("");
                        let topic = o.get("topic").and_then(|v| v.as_str()).unwrap_or("");
                        Some(format!("{} 与 {}：{}", source, target, topic))
                    })
                    .collect()
            })
            .unwrap_or_default();

        let cases: Vec<serde_json::Value> = report
            .cases
            .iter()
            .take(4)
            .map(|case| {
                serde_json::json!({
                    "case_no": case.case_no,
                    "title": case.title,
                    "summary": case.summary,
                })
            })
            .collect();

        serde_json::json!({
            "width": width,
            "group_name": report.group_name,
            "report_title": report.report_title,
            "report_date": report.report_date,
            "time_range": report.time_range,
            "message_count": report.message_count,
            "participant_count": report.participant_count,
            "case_count": report.case_count,
            "character_count": report.character_count,
            "main_storyline": assembly::main_storyline_label(report),
            "opening_line": assembly::daily_opening_line(report),
            "highlight": report.highlight,
            "topic_cards": topic_cards,
            "characters": characters,
            "callbacks": callbacks,
            "relationships": relationships,
            "local_life_notes": local_life_notes,
            "open_questions": open_questions,
            "cases": cases,
            "hourly_svg": hourly_svg,
        })
    }

    fn ns_section_card(kicker: &str, title: &str, body: &str, extra_class: &str) -> String {
        format!(
            r#"<section class="ns-card {extra_class}">
      <div class="ns-kicker">{}</div>
      <h3 class="ns-section-title">{}</h3>
      <div class="ns-card-body">{}</div>
    </section>"#,
            escape_html(kicker),
            escape_html(title),
            body
        )
    }

    fn ns_li_item(text: &str) -> String {
        format!("<li>{}</li>", escape_html(text))
    }

    fn ns_build_topics_section(topic_cards: &[serde_json::Value]) -> String {
        if topic_cards.is_empty() {
            return String::new();
        }
        let mut rows = String::new();
        for topic in topic_cards {
            let title = topic.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let summary = topic.get("summary").and_then(|v| v.as_str()).unwrap_or("");
            let participants = topic
                .get("participants")
                .and_then(|v| v.as_u64())
                .unwrap_or(0);
            rows.push_str(&format!(
                r#"
        <div class="ns-topic-row">
          <strong>{}</strong>
          <span>{}</span>
          <small>{} 人参与</small>
        </div>"#,
                escape_html(title),
                escape_html(summary),
                participants
            ));
        }
        ns_section_card(
            "COMMUNITY DESK",
            "主要话题",
            &format!(r#"<div class="ns-topic-list">{}</div>"#, rows),
            "",
        )
    }

    fn ns_build_characters_section(characters: &[serde_json::Value]) -> String {
        if characters.is_empty() {
            return String::new();
        }
        let mut rows = String::new();
        for character in characters.iter().take(6) {
            let name = character.get("name").and_then(|v| v.as_str()).unwrap_or("");
            let role = character.get("role").and_then(|v| v.as_str()).unwrap_or("");
            let evidence = character
                .get("evidence")
                .and_then(|v| v.as_str())
                .unwrap_or("");
            let rank = character.get("rank").and_then(|v| v.as_u64()).unwrap_or(0);
            rows.push_str(&format!(
                r#"
        <div class="ns-cast-row">
          <div class="ns-cast-rank">{}</div>
          <div class="ns-cast-copy">
            <strong>{}</strong>
            <span>{}</span>
            <small>{}</small>
          </div>
        </div>"#,
                rank,
                escape_html(name),
                escape_html(role),
                escape_html(evidence)
            ));
        }
        ns_section_card(
            "CAST NOTES",
            "人物出场表",
            &format!(r#"<div class="ns-cast-list">{}</div>"#, rows),
            "",
        )
    }

    fn ns_build_highlight_section(highlight: &str) -> String {
        if highlight.is_empty() {
            return String::new();
        }
        ns_section_card(
            "QUOTE ANCHOR",
            "今日台词",
            &format!(
                r#"<blockquote class="ns-quote">{}</blockquote>"#,
                escape_html(highlight)
            ),
            "",
        )
    }

    fn ns_build_list_section(kicker: &str, title: &str, items: &[String]) -> String {
        if items.is_empty() {
            return String::new();
        }
        let body = format!(
            r#"<ul class="ns-list">{}</ul>"#,
            items
                .iter()
                .take(6)
                .map(|s| ns_li_item(s))
                .collect::<String>()
        );
        ns_section_card(kicker, title, &body, "")
    }

    fn ns_build_cases_section(cases: &[serde_json::Value]) -> String {
        if cases.is_empty() {
            return String::new();
        }
        let mut rows = String::new();
        for case in cases.iter().take(4) {
            let case_no = case
                .get("case_no")
                .and_then(|v| v.as_str())
                .unwrap_or("")
                .replace("CASE ", "");
            let title = case.get("title").and_then(|v| v.as_str()).unwrap_or("");
            let summary = case.get("summary").and_then(|v| v.as_str()).unwrap_or("");
            rows.push_str(&format!(
                r#"
        <div class="ns-case-row">
          <span class="ns-case-no">{}</span>
          <strong>{}</strong>
          <small>{}</small>
        </div>"#,
                escape_html(&case_no),
                escape_html(title),
                escape_html(summary)
            ));
        }
        ns_section_card(
            "STORYLINE FILES",
            "故事线候选",
            &format!(r#"<div class="ns-case-list">{}</div>"#, rows),
            "",
        )
    }

    pub fn render_newspaper_elegant(report: &ReportData, width: usize) -> Result<String> {
        let input = build_newspaper_elegant_input(report, width);
        let group_name = input
            .get("group_name")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let report_title = input
            .get("report_title")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let report_date = input
            .get("report_date")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let time_range = input
            .get("time_range")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let message_count = input
            .get("message_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let participant_count = input
            .get("participant_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let case_count = input
            .get("case_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let character_count = input
            .get("character_count")
            .and_then(|v| v.as_u64())
            .unwrap_or(0) as usize;
        let main_storyline = input
            .get("main_storyline")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let opening_line = input
            .get("opening_line")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let highlight = input
            .get("highlight")
            .and_then(|v| v.as_str())
            .unwrap_or("");
        let topic_cards = input
            .get("topic_cards")
            .and_then(|v| v.as_array())
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let characters = input
            .get("characters")
            .and_then(|v| v.as_array())
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let callbacks = input
            .get("callbacks")
            .and_then(|v| v.as_array())
            .map(|v| {
                v.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let relationships = input
            .get("relationships")
            .and_then(|v| v.as_array())
            .map(|v| {
                v.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let local_life_notes = input
            .get("local_life_notes")
            .and_then(|v| v.as_array())
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let open_questions = input
            .get("open_questions")
            .and_then(|v| v.as_array())
            .map(|v| {
                v.iter()
                    .filter_map(|x| x.as_str().map(String::from))
                    .collect::<Vec<_>>()
            })
            .unwrap_or_default();
        let cases = input
            .get("cases")
            .and_then(|v| v.as_array())
            .map(|v| v.as_slice())
            .unwrap_or(&[]);
        let hourly_svg = input
            .get("hourly_svg")
            .and_then(|v| v.as_str())
            .unwrap_or("");

        let masthead_sub = if group_name.is_empty() {
            "QINTOPIA REVIEW"
        } else {
            group_name
        };
        let masthead_title = if report_title.is_empty() {
            "秦托邦时报"
        } else {
            report_title
        };
        let edition = format!("{} 版", report_date);
        let page_meta = format!("{} · {} 人出场", time_range, participant_count);

        let topics_html = ns_build_topics_section(topic_cards);
        let characters_html = ns_build_characters_section(characters);
        let highlight_html = ns_build_highlight_section(highlight);
        let callbacks_html = ns_build_list_section("MEME MAP", "梗和回调候选", &callbacks);
        let relationships_html =
            ns_build_list_section("ENSEMBLE LINKS", "同场关系", &relationships);
        let local_life_items: Vec<String> = local_life_notes
            .iter()
            .map(|item| {
                let label = item.get("label").and_then(|v| v.as_str()).unwrap_or("");
                let source = item.get("source").and_then(|v| v.as_str()).unwrap_or("");
                format!("{}（{}）", label, source)
            })
            .collect();
        let local_life_html =
            ns_build_list_section("LOCAL THREADS", "地点 / 本地生活线索", &local_life_items);
        let questions_html = ns_build_list_section("OPEN LOOPS", "待解决问题", &open_questions);
        let cases_html = ns_build_cases_section(cases);

        let stats_items = [
            ("消息", message_count, "当日素材"),
            ("出场", participant_count, "活跃成员"),
            ("主线", case_count, "可归档"),
            ("人物", character_count, "群像卡"),
        ];
        let stats_html = stats_items
            .iter()
            .map(|(label, value, caption)| {
                format!(
                    r#"
        <div class="ns-stat">
          <span class="ns-stat-value">{}</span>
          <span class="ns-stat-label">{}</span>
          <span class="ns-stat-caption">{}</span>
        </div>"#,
                    value,
                    escape_html(label),
                    escape_html(caption)
                )
            })
            .collect::<String>();

        Ok(format!(
            r#"<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  html, body {{
    margin: 0;
    background: #d5d4ce;
    color: #171717;
    font-family: "PingFang SC", "Noto Sans CJK SC", "Heiti SC", sans-serif;
  }}
  .ns-page {{
    width: {width}px;
    margin: 0 auto;
    padding: 38px 32px 32px;
    background:
      linear-gradient(90deg, rgba(23, 23, 23, 0.018) 1px, transparent 1px) 0 0 / 60px 60px,
      linear-gradient(#fbfaf6, #fbfaf6);
    position: relative;
  }}
  .ns-page::after {{
    content: "";
    position: absolute;
    inset: 18px;
    border: 1px solid #d6d2c8;
    pointer-events: none;
  }}
  .ns-header {{
    position: relative;
    z-index: 1;
    border-bottom: 2px solid #242424;
    margin-bottom: 22px;
  }}
  .ns-nameplate {{
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: end;
    gap: 18px;
    padding-bottom: 10px;
    text-transform: uppercase;
    color: #63625d;
    font-size: 13px;
    letter-spacing: 0.08em;
  }}
  .ns-nameplate strong {{
    color: #171717;
    font-family: "Songti SC", "STSong", "Noto Serif CJK SC", serif;
    font-size: 44px;
    line-height: 0.95;
    font-weight: 900;
    letter-spacing: 0;
    white-space: nowrap;
  }}
  .ns-nameplate span:last-child {{ text-align: right; }}
  .ns-meta {{
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 14px;
    padding: 8px 0 9px;
    border-top: 1px solid #d6d2c8;
    color: #63625d;
    font-size: 13px;
  }}
  .ns-meta span:first-child {{ color: #171717; font-weight: 800; }}
  .ns-meta span:nth-child(2) {{ text-align: center; }}
  .ns-meta span:last-child {{ text-align: right; color: #171717; font-weight: 800; }}
  .ns-hero {{
    position: relative;
    z-index: 1;
    padding: 18px 20px 20px;
    margin-bottom: 22px;
    background: #f1f0ea;
    border-left: 6px solid #8b1f2f;
  }}
  .ns-hero-kicker {{
    color: #8b1f2f;
    font-size: 12px;
    font-weight: 900;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }}
  .ns-hero h2 {{
    margin-top: 6px;
    font-family: "Songti SC", "STSong", "Noto Serif CJK SC", serif;
    font-size: 36px;
    line-height: 1.05;
    font-weight: 900;
  }}
  .ns-deck {{
    margin-top: 12px;
    color: #373530;
    font-size: 16px;
    line-height: 1.45;
    font-weight: 700;
  }}
  .ns-hero-meta {{
    margin-top: 12px;
    padding-top: 8px;
    border-top: 1px solid #d6d2c8;
    font-size: 11px;
    color: #555;
  }}
  .ns-grid {{
    position: relative;
    z-index: 1;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
  }}
  .ns-card {{
    padding: 14px 14px 16px;
    border: 1px solid #d6d2c8;
    background: #ffffff;
  }}
  .ns-kicker {{
    color: #8b1f2f;
    font-size: 11px;
    font-weight: 900;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }}
  .ns-section-title {{
    margin-top: 6px;
    padding-bottom: 6px;
    border-bottom: 1px solid #242424;
    font-family: "Songti SC", "STSong", "Noto Serif CJK SC", serif;
    font-size: 20px;
    line-height: 1.15;
    font-weight: 900;
  }}
  .ns-card-body {{
    margin-top: 10px;
    font-size: 13px;
    line-height: 1.55;
  }}
  .ns-list {{
    list-style: none;
    padding: 0;
  }}
  .ns-list li {{
    margin: 0 0 7px;
    padding-left: 14px;
    position: relative;
  }}
  .ns-list li::before {{
    content: "";
    position: absolute;
    left: 0;
    top: 0.65em;
    width: 5px;
    height: 5px;
    background: #8b1f2f;
  }}
  .ns-quote {{
    margin: 0;
    padding: 12px 14px;
    background: #fff;
    border-top: 3px solid #8b1f2f;
    font-size: 15px;
    line-height: 1.5;
    font-weight: 700;
  }}
  .ns-topic-row {{
    display: grid;
    gap: 2px;
    padding: 7px 0;
    border-bottom: 1px solid #f1f0ea;
  }}
  .ns-topic-row:last-child {{ border-bottom: 0; }}
  .ns-topic-row strong {{ font-size: 14px; }}
  .ns-topic-row span {{ color: #555; font-size: 12px; }}
  .ns-topic-row small {{ color: #8b1f2f; font-size: 10px; font-weight: 800; }}
  .ns-cast-row {{
    display: grid;
    grid-template-columns: 28px 1fr;
    gap: 10px;
    padding: 7px 0;
    border-bottom: 1px solid #f1f0ea;
  }}
  .ns-cast-row:last-child {{ border-bottom: 0; }}
  .ns-cast-rank {{
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border: 1px solid #171717;
    background: #f1f0ea;
    font-size: 12px;
    font-weight: 900;
  }}
  .ns-cast-copy strong {{ font-size: 14px; }}
  .ns-cast-copy span {{ display: block; color: #8b1f2f; font-size: 11px; font-weight: 700; }}
  .ns-cast-copy small {{ display: block; color: #555; font-size: 11px; margin-top: 2px; }}
  .ns-case-row {{
    display: grid;
    gap: 2px;
    padding: 7px 0;
    border-bottom: 1px solid #f1f0ea;
  }}
  .ns-case-row:last-child {{ border-bottom: 0; }}
  .ns-case-no {{
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border: 1px solid #171717;
    border-radius: 50%;
    background: #f1f0ea;
    font-size: 10px;
    font-weight: 900;
  }}
  .ns-case-row strong {{ font-size: 13px; }}
  .ns-case-row small {{ color: #555; font-size: 11px; }}
  .ns-bottom {{
    position: relative;
    z-index: 1;
    margin-top: 22px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
  }}
  .ns-timeline {{
    padding: 14px;
    border: 1px solid #d6d2c8;
    background: #fff;
  }}
  .ns-timeline h4 {{
    font-family: "Songti SC", "STSong", "Noto Serif CJK SC", serif;
    font-size: 16px;
    margin-bottom: 8px;
  }}
  .ns-timeline svg {{ display: block; width: 100%; height: 90px; }}
  .ns-stats {{
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 1px;
    background: #d6d2c8;
    border: 1px solid #d6d2c8;
  }}
  .ns-stat {{
    background: #fff;
    padding: 12px 8px;
    text-align: center;
  }}
  .ns-stat-value {{ font-size: 26px; font-weight: 900; color: #171717; }}
  .ns-stat-label {{ display: block; font-size: 11px; color: #8b1f2f; font-weight: 800; margin-top: 4px; }}
  .ns-stat-caption {{ display: block; font-size: 9px; color: #63625d; margin-top: 2px; }}
  .ns-footer {{
    position: relative;
    z-index: 1;
    margin-top: 22px;
    padding-top: 10px;
    border-top: 2px solid #242424;
    color: #63625d;
    font-size: 11px;
    display: flex;
    justify-content: space-between;
  }}
  @media screen and (max-width: 780px) {{
    .ns-grid {{ grid-template-columns: 1fr; }}
    .ns-bottom {{ grid-template-columns: 1fr; }}
    .ns-nameplate strong {{ font-size: 32px; }}
    .ns-hero h2 {{ font-size: 28px; }}
  }}
</style>
</head>
<body>
<div class="ns-page">
  <header class="ns-header">
    <div class="ns-nameplate">
      <span>{}</span>
      <strong>{}</strong>
      <span>{}</span>
    </div>
    <div class="ns-meta">
      <span>COMMUNITY DAILY</span>
      <span>{}</span>
      <span>{}</span>
    </div>
  </header>
  <section class="ns-hero">
    <div class="ns-hero-kicker">COVER STORY</div>
    <h2>{}</h2>
    <p class="ns-deck">{}</p>
    <div class="ns-hero-meta">{} 条素材 · {} 条主线 · {} 位剧中人</div>
  </section>
  <div class="ns-grid">
    
    {}
    
    {}
    
    {}
    
    {}
    
    {}
    
    {}
    
    {}
    
    {}
  </div>
  <div class="ns-bottom">
    <section class="ns-timeline">
      <h4>24H 活跃节奏</h4>
      <svg viewBox="0 0 {} 90" aria-label="24小时活跃节奏">{}</svg>
    </section>
    <div class="ns-stats">{}</div>
  </div>
  <footer class="ns-footer">
    <span>本报告由小满根据最新群聊窗口自动整理 · 长期画像只以公开安全的角色复现计数参与</span>
    <span>{}</span>
  </footer>
</div>
</body>
</html>"#,
            escape_html(masthead_sub),
            escape_html(masthead_title),
            escape_html(report_date),
            escape_html(&page_meta),
            escape_html(&edition),
            escape_html(main_storyline),
            escape_html(opening_line),
            message_count,
            case_count,
            character_count,
            topics_html,
            characters_html,
            highlight_html,
            callbacks_html,
            relationships_html,
            local_life_html,
            questions_html,
            cases_html,
            width - 120,
            hourly_svg,
            stats_html,
            escape_html(report_title)
        ))
    }

    // -----------------------------------------------------------------------
    // Newspaper
    // -----------------------------------------------------------------------

    pub fn render_newspaper(report: &ReportData, width: usize) -> Result<String> {
        let main_storyline = assembly::main_storyline_label(report);
        let opening = assembly::daily_opening_line(report);
        let highlight = escape_html(report.highlight.as_deref().unwrap_or(""));

        let mut lead_paragraphs: Vec<String> = Vec::new();
        if !opening.is_empty() {
            lead_paragraphs.push(escape_html(&opening));
        }
        for case in report.cases.iter().take(3) {
            let mut para = format!(
                "【{}】{}。",
                escape_html(&case.title),
                escape_html(&case.summary)
            );
            let bullets = case
                .bullets
                .iter()
                .take(3)
                .map(|b| escape_html(b))
                .collect::<Vec<_>>()
                .join("；");
            if !bullets.is_empty() {
                para.push_str(&bullets);
                para.push('。');
            }
            para.push_str(&format!(
                "（{} 人参与，{} 条消息）",
                case.participant_count, case.message_count
            ));
            lead_paragraphs.push(para);
        }
        if !report.characters.is_empty() {
            let names = report
                .characters
                .iter()
                .take(4)
                .map(|c| escape_html(&c.name))
                .collect::<Vec<_>>()
                .join("、");
            lead_paragraphs.push(format!("今日活跃的剧中人包括 {}。", names));
        }
        let lead_article_html = lead_paragraphs
            .iter()
            .map(|p| format!("<p>{}</p>", p))
            .collect::<String>();

        let character_html = report
            .characters
            .iter()
            .take(6)
            .enumerate()
            .map(|(i, c)| {
                let (bg, fg) = CASE_CARD_COLORS[i % CASE_CARD_COLORS.len()];
                format!(
                    r#"<div class="profile">
          <div class="profile-avatar" style="background:{};color:{}">{}</div>
          <div class="profile-copy">
            <h4>{}</h4>
            <p>{} · {}</p>
          </div>
        </div>"#,
                    escape_html(bg),
                    escape_html(fg),
                    escape_html(&c.name.chars().next().unwrap_or_default().to_string()),
                    escape_html(&c.name),
                    escape_html(&c.role_label),
                    escape_html(&c.story_function)
                )
            })
            .collect::<String>();

        let case_cards_html = report
            .cases
            .iter()
            .take(4)
            .map(|case| {
                format!(
                    r#"<article class="story-card">
          <div class="story-kicker">{}</div>
          <h4>{}</h4>
          <p>{}</p>
        </article>"#,
                    escape_html(&case_storyline_label(case)),
                    escape_html(&case.title),
                    escape_html(&case.summary)
                )
            })
            .collect::<String>();

        let stats = [
            ("消息", report.message_count.to_string()),
            ("活跃人数", report.participant_count.to_string()),
            ("主线", report.case_count.to_string()),
            ("剧中人", report.character_count.to_string()),
        ];
        let stats_html = stats
            .iter()
            .map(|(k, v)| {
                format!(
                    r#"<div class="stat-box"><span>{}</span><strong>{}</strong></div>"#,
                    escape_html(k),
                    escape_html(v)
                )
            })
            .collect::<String>();

        let chart_width = width - 84;
        let peak_count = *report.hourly_counts.iter().max().unwrap_or(&0);
        let max_hourly = peak_count.max(1);
        let peak_idx = report
            .hourly_counts
            .iter()
            .position(|&c| c == peak_count)
            .unwrap_or(0);
        let chart_svg = bar_svg(&report.hourly_counts, max_hourly, chart_width, 80);

        let mvp_html = report
            .suspects
            .iter()
            .take(5)
            .map(|s| {
                format!(
                    r#"<div class="mvp-row"><span>{}</span><strong>{}</strong><em>{} 条 / {} 字</em></div>"#,
                    s.rank,
                    escape_html(&s.name),
                    s.message_count,
                    s.word_count
                )
            })
            .collect::<String>();

        let hot_html = report
            .hot_topics
            .iter()
            .take(5)
            .map(|t| {
                format!(
                    r#"<div class="hot-row"><span>{}</span><strong>{}</strong><small>{} 次</small></div>"#,
                    t.rank,
                    escape_html(&t.keyword),
                    t.message_count
                )
            })
            .collect::<String>();
        let hot_section = if hot_html.is_empty() {
            String::new()
        } else {
            format!(
                r#"<div class="sidebar-section"><h3>热词</h3>{}</div>"#,
                hot_html
            )
        };

        let highlight_block = if highlight.is_empty() {
            String::new()
        } else {
            format!(
                r#"<div class="highlight-box"><blockquote>“{}”</blockquote></div>"#,
                highlight
            )
        };

        Ok(format!(
            r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ background: #e8e4dc; color: #1a1a1a; font-family: "Noto Serif CJK SC", "Source Han Serif SC", "STSong", "SimSun", "Songti SC", "AR PL UMing CN", serif; }}
  .paper {{ width: {width}px; margin: 0 auto; background: #f7f5f0; padding: 36px 42px; box-sizing: border-box; }}
  .masthead {{ text-align: center; border-bottom: 3px solid #1a1a1a; padding-bottom: 10px; margin-bottom: 24px; }}
  .masthead-kicker {{ font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif; font-size: 10px; letter-spacing: 4px; text-transform: uppercase; color: #555; }}
  .masthead h1 {{ font-size: 56px; font-weight: 900; letter-spacing: 12px; margin: 6px 0 4px; }}
  .masthead-meta {{ display: flex; justify-content: center; gap: 48px; font-family: sans-serif; font-size: 12px; color: #444; margin-top: 6px; }}
  .headline {{ margin: 28px 0 18px; }}
  .headline h2 {{ font-size: 42px; line-height: 1.25; font-weight: 900; margin-bottom: 10px; }}
  .headline .deck {{ font-size: 18px; line-height: 1.55; color: #444; font-style: italic; }}
  .content {{ display: grid; grid-template-columns: 2fr 1fr; gap: 32px; margin-bottom: 32px; }}
  .lead {{ column-count: 2; column-gap: 28px; font-size: 14px; line-height: 1.85; }}
  .lead p {{ margin-bottom: 14px; text-align: justify; }}
  .lead p:first-child::first-letter {{ font-size: 42px; float: left; line-height: 1; margin-right: 6px; margin-top: 4px; font-weight: 900; }}
  .sidebar {{ border-left: 2px solid #1a1a1a; padding-left: 24px; }}
  .sidebar-section {{ margin-bottom: 22px; }}
  .sidebar-section h3 {{ font-family: sans-serif; font-size: 12px; letter-spacing: 1px; text-transform: uppercase; border-bottom: 2px solid #1a1a1a; padding-bottom: 4px; margin-bottom: 10px; }}
  .stat-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; }}
  .stat-box {{ border: 2px solid #1a1a1a; padding: 8px; text-align: center; }}
  .stat-box span {{ font-family: sans-serif; font-size: 10px; color: #555; display: block; margin-bottom: 2px; }}
  .stat-box strong {{ font-size: 24px; font-weight: 900; }}
  .timeline-chart svg {{ display: block; width: 100%; height: 80px; }}
  .timeline-chart .peak {{ font-family: sans-serif; font-size: 11px; color: #555; margin-top: 6px; }}
  .mvp-row, .hot-row {{ display: flex; align-items: baseline; gap: 8px; font-size: 13px; margin-bottom: 6px; }}
  .mvp-row span, .hot-row span {{ font-family: sans-serif; font-size: 10px; font-weight: 900; min-width: 16px; }}
  .section {{ margin-bottom: 28px; }}
  .section-kicker {{ font-family: sans-serif; font-size: 11px; letter-spacing: 2px; text-transform: uppercase; color: #b91c1c; font-weight: 800; margin-bottom: 6px; }}
  .section h3 {{ font-size: 22px; font-weight: 900; margin-bottom: 14px; }}
  .character-grid {{ display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; }}
  .profile {{ display: flex; gap: 10px; align-items: flex-start; border: 2px solid #1a1a1a; padding: 10px; background: #fff; }}
  .profile-avatar {{ width: 36px; height: 36px; border-radius: 50%; display: grid; place-items: center; font-family: sans-serif; font-size: 16px; font-weight: 900; flex-shrink: 0; }}
  .profile-copy h4 {{ font-size: 14px; font-weight: 900; margin-bottom: 2px; }}
  .profile-copy p {{ font-size: 11px; color: #444; line-height: 1.4; }}
  .story-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 18px; }}
  .story-card {{ border: 2px solid #1a1a1a; padding: 14px; background: #fff; }}
  .story-card .story-kicker {{ font-family: sans-serif; font-size: 10px; color: #b91c1c; font-weight: 800; letter-spacing: 1px; text-transform: uppercase; margin-bottom: 4px; }}
  .story-card h4 {{ font-size: 16px; font-weight: 900; margin-bottom: 6px; }}
  .story-card p {{ font-size: 12px; line-height: 1.55; color: #333; }}
  .highlight-box {{ border-top: 3px solid #1a1a1a; border-bottom: 3px solid #1a1a1a; padding: 16px 0; margin-bottom: 28px; }}
  .highlight-box blockquote {{ font-size: 22px; font-style: italic; line-height: 1.5; }}
  .footer {{ border-top: 2px solid #1a1a1a; padding-top: 10px; font-family: sans-serif; font-size: 10px; color: #555; display: flex; justify-content: space-between; }}
</style>
</head>
<body>
<main class="paper">
  <header class="masthead">
    <div class="masthead-kicker">QINTOPIA COMMUNITY DAILY</div>
    <h1>小满时报</h1>
    <div class="masthead-meta">
      <span>{}</span>
      <span>{}</span>
      <span>第 1 版</span>
    </div>
  </header>
  <div class="headline">
    <h2>{}</h2>
    <p class="deck">{}</p>
  </div>
  <div class="content">
    <article class="lead">
      {}
    </article>
    <aside class="sidebar">
      <div class="sidebar-section">
        <h3>数据</h3>
        <div class="stat-grid">
          {}
        </div>
      </div>
      <div class="sidebar-section">
        <h3>24H 活跃</h3>
        <div class="timeline-chart">
          <svg viewBox="0 0 {chart_width} 80" preserveAspectRatio="none">{}</svg>
          <div class="peak">峰值 {} 条 / {:02}:00</div>
        </div>
      </div>
      <div class="sidebar-section">
        <h3>发言榜</h3>
        {}
      </div>
      {}
    </aside>
  </div>
  {}
  <section class="section">
    <div class="section-kicker">Cast Notes</div>
    <h3>人物出场表</h3>
    <div class="character-grid">
      {}
    </div>
  </section>
  <section class="section">
    <div class="section-kicker">Storylines</div>
    <h3>今日主线</h3>
    <div class="story-grid">
      {}
    </div>
  </section>
  <footer class="footer">
    <span>本报告由小满自动整理，仅反映已审核公开安全的群聊片段。</span>
    <span>{}</span>
  </footer>
</main>
</body>
</html>"#,
            escape_html(&report.report_date),
            escape_html(&report.time_range),
            escape_html(&main_storyline),
            escape_html(&opening),
            lead_article_html,
            stats_html,
            chart_svg,
            peak_count,
            peak_idx,
            mvp_html,
            hot_section,
            highlight_block,
            character_html,
            case_cards_html,
            escape_html(&report.report_date)
        ))
    }

    // -----------------------------------------------------------------------
    // V3
    // -----------------------------------------------------------------------

    pub fn render_v3(report: &ReportData, width: usize) -> Result<String> {
        let chart_width = width - 96;
        let peak_count = *report.hourly_counts.iter().max().unwrap_or(&0);
        let max_hourly = peak_count.max(1);
        let peak_idx = report
            .hourly_counts
            .iter()
            .position(|&c| c == peak_count)
            .unwrap_or(0);
        let timeline_svg = bar_svg(&report.hourly_counts, max_hourly, chart_width, 68);
        let timeline_labels = (0..=24)
            .step_by(4)
            .map(|idx| {
                format!(
                    r##"<text x="{}" y="94" font-size="9" fill="#4a4a4a" text-anchor="middle">{:02}</text>"##,
                    (idx as f64 / 24.0 * chart_width as f64) as usize,
                    idx
                )
            })
            .collect::<String>();
        let peak_x = peak_idx * (chart_width / 24) + (chart_width / 48);
        let peak_svg = format!(
            r##"<text x="{}" y="12" font-size="10" fill="#f25a18" font-weight="700" text-anchor="middle">{}</text>"##,
            peak_x, peak_count
        );
        let main_storyline = assembly::main_storyline_label(report);
        let opening_line = assembly::daily_opening_line(report);
        let callback_candidates = assembly::meme_callback_candidates(report, 5);
        let relationship_candidates = assembly::relationship_candidates(report, 4);
        let local_life_notes = assembly::ordinary_digest_local_life_notes(report);
        let open_questions = assembly::ordinary_digest_open_questions(report);

        let story_index_html = report
            .cases
            .iter()
            .take(4)
            .enumerate()
            .map(|(index, case)| {
                format!(
                    r#"
      <div class="story-index-item">
        <span>{:02}</span>
        <strong>{}</strong>
        <small>{}</small>
      </div>"#,
                    index + 1,
                    escape_html(&case_storyline_label(case)),
                    escape_html(&case.summary)
                )
            })
            .collect::<Vec<_>>()
            .join("\n");
        let story_index_section = if story_index_html.is_empty() {
            String::new()
        } else {
            format!(
                r#"
  <section class="story-index">
    <div class="story-index-heading"><span>DAILY WORKSHOP INDEX</span><strong>{} 条素材 / {} 位出场 / {} 条主线 / {} 张人物卡</strong></div>
    <div class="story-index-grid">{}</div>
  </section>"#,
                report.message_count,
                report.participant_count,
                report.case_count,
                report.character_count,
                story_index_html
            )
        };

        let stats_html = [
            ("消息", report.message_count, "当日素材"),
            ("出场", report.participant_count, "活跃成员"),
            ("主线", report.case_count, "可归档"),
            ("人物", report.character_count, "群像卡"),
        ]
        .iter()
        .map(|(label, value, caption)| {
            format!(
                r#"
      <div class="stat">
        <div class="stat-label">{}</div>
        <div class="stat-value">{}</div>
        <div class="stat-caption">{}</div>
      </div>"#,
                label, value, caption
            )
        })
        .collect::<Vec<_>>()
        .join("\n");

        let case_cards = report
            .cases
            .iter()
            .map(|case| {
                format!(
                    r#"
      <article class="case-card">
        <div class="case-head">
          <span class="case-number">{}</span>
          <span class="case-time">{}</span>
        </div>
        <h3>{}</h3>
        <p class="case-summary">{}</p>
        <ul class="case-notes">{}</ul>
      </article>"#,
                    escape_html(&case.case_no.replace("CASE ", "")),
                    escape_html(&case.time_label),
                    escape_html(&case_storyline_label(case)),
                    escape_html(&case.summary),
                    case.bullets
                        .iter()
                        .take(3)
                        .map(|b| format!("<li>{}</li>", escape_html(b)))
                        .collect::<String>()
                )
            })
            .collect::<String>();
        let cases_html = if case_cards.is_empty() {
            String::new()
        } else {
            format!(
                r#"
  
  <section class="section cases-section">
    <div class="section-kicker">STORYLINE FILES</div>
    <h2>故事线候选</h2>
    <div class="cases">{}</div>
  </section>"#,
                case_cards
            )
        };

        let suspects_html = report
            .suspects
            .iter()
            .map(|suspect| {
                format!(
                    r#"
      <div class="mvp-card">
        <div class="mvp-rank">{}</div>
        <div class="mvp-copy">
          <div class="mvp-name">{}</div>
          <div class="mvp-meta">{} 条 / {} 字</div>
        </div>
        <div class="mvp-score">{}</div>
      </div>"#,
                    suspect.rank,
                    escape_html(&suspect.name),
                    suspect.message_count,
                    suspect.word_count,
                    suspect.message_count
                )
            })
            .collect::<String>();
        let mvp_html = if suspects_html.is_empty() {
            String::new()
        } else {
            format!(
                r#"
  <section class="section mvp-section">
    <div class="section-kicker">VOICE INDEX</div>
    <h2>发言出场榜</h2>
    <div class="mvp-grid">{}</div>
  </section>"#,
                suspects_html
            )
        };

        let highlight_html = if let Some(highlight) = &report.highlight {
            format!(
                r#"
  
  <section class="highlight">
    <div class="highlight-kicker">QUOTE ANCHOR</div>
    <div class="highlight-title">今日台词</div>
    <p>“{}”</p>
  </section>"#,
                escape_html(highlight)
            )
        } else {
            String::new()
        };

        let callbacks_html = if callback_candidates.is_empty() {
            String::new()
        } else {
            format!(
                r#"
  
  <section class="hotlist">
    <div class="hotlist-heading"><span>MEME MAP</span><h2>梗和回调候选</h2></div>
    <div class="hotlist-grid">{}</div>
  </section>"#,
                callback_candidates
                    .iter()
                    .enumerate()
                    .map(|(index, candidate)| {
                        let mut parts = candidate.splitn(2, '：');
                        let label = parts.next().unwrap_or(candidate).to_string();
                        let detail = parts.next().unwrap_or(candidate).to_string();
                        format!(
                            r#"<div class="hot-topic"><span class="hot-rank">{}</span><strong>{}</strong><small>{}</small></div>"#,
                            index + 1,
                            escape_html(&label),
                            escape_html(&detail)
                        )
                    })
                    .collect::<String>()
            )
        };

        let relationships_html = if relationship_candidates.is_empty() {
            String::new()
        } else {
            format!(
                r#"
  
  <section class="relationships">
    <div class="relationships-heading"><span>ENSEMBLE LINKS</span><h2>同场关系</h2></div>
    <div class="relationship-list">{}</div>
  </section>"#,
                relationship_candidates
                    .iter()
                    .enumerate()
                    .map(|(index, candidate)| {
                        format!(
                            r#"<div class="relationship-row"><span>{}</span><p>{}</p></div>"#,
                            index + 1,
                            escape_html(candidate)
                        )
                    })
                    .collect::<String>()
            )
        };

        let local_life_html = if local_life_notes.is_empty() {
            String::new()
        } else {
            format!(
                r#"
  
  <section class="reference-notes">
    <div class="reference-heading"><span>LOCAL THREADS</span><h2>地点 / 本地生活线索</h2></div>
    <div class="reference-list">{}</div>
  </section>"#,
                local_life_notes
                    .iter()
                    .enumerate()
                    .map(|(index, item)| {
                        let label = item.get("label").and_then(|v| v.as_str()).unwrap_or("");
                        format!(
                            r#"<div class="reference-row"><span>{}</span><p>{}</p></div>"#,
                            index + 1,
                            escape_html(label)
                        )
                    })
                    .collect::<String>()
            )
        };

        let open_questions_html = if open_questions.is_empty() {
            String::new()
        } else {
            format!(
                r#"
  
  <section class="reference-notes questions">
    <div class="reference-heading"><span>OPEN LOOPS</span><h2>待解决问题</h2></div>
    <div class="reference-list">{}</div>
  </section>"#,
                open_questions
                    .iter()
                    .enumerate()
                    .map(|(index, question)| {
                        format!(
                            r#"<div class="reference-row"><span>{}</span><p>{}</p></div>"#,
                            index + 1,
                            escape_html(question)
                        )
                    })
                    .collect::<String>()
            )
        };

        let characters_html = if report.characters.is_empty() {
            String::new()
        } else {
            format!(
                r#"
  
  <section class="characters">
    <div class="characters-heading"><span>CAST NOTES</span><h2>人物出场表</h2></div>
    <div class="character-grid">{}</div>
  </section>"#,
                report
                    .characters
                    .iter()
                    .map(|character| {
                        let arc_or_one_liner = if character.arc_label.is_empty() {
                            &character.one_liner
                        } else {
                            &character.arc_label
                        };
                        let mut small = String::new();
                        small.push_str(&escape_html(&character.callback_hint));
                        if !character.relationship_hint.is_empty() {
                            small.push_str(" · ");
                            small.push_str(&escape_html(&character.relationship_hint));
                        }
                        if !character.expressive_label.is_empty() {
                            small.push_str(" · 已审核标签：");
                            small.push_str(&escape_html(&character.expressive_label));
                        }
                        if !character.memory_weight_label.is_empty() {
                            small.push_str(" · ");
                            small.push_str(&escape_html(&character.memory_weight_label));
                        }
                        format!(
                            r#"<article class="character-card"><div class="character-rank">{}</div><div class="character-copy"><h3>{}</h3><strong>{} · {}</strong><p>{}</p><blockquote>{}</blockquote><small>{}</small></div></article>"#,
                            character.rank,
                            escape_html(&character.name),
                            escape_html(&character.role_label),
                            escape_html(&character.story_function),
                            escape_html(arc_or_one_liner),
                            escape_html(&character.evidence),
                            small
                        )
                    })
                    .collect::<String>()
            )
        };

        Ok(format!(
            r#"<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ background: #ddd8ce; color: #111111; font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif; }}
  .daily-paper {{ width: {width}px; margin: 18px auto; background: #fff8df; border: 9px solid #111111; }}
  .topline {{ min-height: 42px; display: flex; align-items: center; justify-content: space-between; padding: 0 24px; background: #111111; color: #ffd92e; font-size: 11px; font-weight: 800; }}
  .hero {{ position: relative; min-height: 196px; padding: 22px 154px 20px 24px; background: #ffd92e; border-bottom: 4px solid #111111; }}
  .hero-group {{ font-size: 25px; font-weight: 800; line-height: 1.25; }}
  .hero-title {{ margin-top: 7px; font-size: 42px; font-weight: 900; line-height: 1; }}
  .hero-mainline {{ margin-top: 14px; font-size: 18px; font-weight: 900; line-height: 1.45; }}
  .hero-opening {{ margin-top: 8px; color: #2b2b2b; font-size: 13px; font-weight: 700; line-height: 1.55; }}
  .hero-time {{ margin-top: 12px; padding-top: 6px; border-top: 4px solid #111111; font-size: 11px; }}
  .hero-badge {{ position: absolute; right: 24px; top: 24px; display: grid; width: 106px; height: 106px; place-items: center; border: 4px solid #111111; border-radius: 12px; background: #88d7ff; font-size: 21px; font-weight: 900; text-align: center; line-height: 1.1; }}
  .story-index {{ padding: 16px 24px 18px; background: #111111; color: #fff8df; }}
  .story-index-heading {{ display: flex; align-items: baseline; justify-content: space-between; gap: 18px; margin-bottom: 11px; }}
  .story-index-heading span {{ color: #ffd92e; font-size: 11px; font-weight: 900; }}
  .story-index-heading strong {{ color: #fff0a6; font-size: 12px; font-weight: 800; text-align: right; }}
  .story-index-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; }}
  .story-index-item {{ display: grid; grid-template-columns: 32px 1fr; gap: 8px; min-height: 54px; padding: 8px; border: 2px solid #fff8df; background: #1c1c1c; }}
  .story-index-item span {{ display: grid; width: 28px; height: 28px; place-items: center; border: 2px solid #ffd92e; border-radius: 50%; color: #ffd92e; font-size: 11px; font-weight: 900; }}
  .story-index-item strong {{ min-width: 0; font-size: 13px; font-weight: 900; line-height: 1.25; }}
  .story-index-item small {{ grid-column: 2; color: #c9c9c9; font-size: 10px; line-height: 1.35; }}
  .stats {{ display: grid; grid-template-columns: repeat(4, 1fr); margin: 22px 24px 0; border: 3px solid #111111; background: #ffffff; color: #111111; }}
  .stat {{ min-height: 70px; padding: 13px 16px; border-right: 2px solid #111111; }}
  .stat:last-child {{ border-right: 0; }}
  .stat-label, .section-kicker, .highlight-kicker, .hotlist-heading span {{ color: #ffd92e; font-size: 11px; font-weight: 800; }}
  .stat .stat-label {{ color: #f25a18; }}
  .stat-value {{ margin-top: 4px; font-size: 26px; font-weight: 900; line-height: 1; }}
  .stat-caption {{ margin-top: 4px; color: #555555; font-size: 10px; }}
  .timeline {{ margin: 22px 24px 0; padding: 18px 18px 12px; border: 4px solid #111111; background: #fff0a6; }}
  .timeline-head {{ display: flex; align-items: baseline; justify-content: space-between; }}
  .timeline h2, .section h2 {{ font-size: 26px; font-weight: 900; line-height: 1.1; }}
  .peak {{ font-size: 12px; font-weight: 700; }}
  .timeline svg {{ display: block; width: 100%; height: 106px; margin-top: 8px; }}
  .highlight {{ display: grid; grid-template-columns: 154px 1fr; gap: 18px; margin: 34px 24px 0; padding: 18px 20px; border: 4px solid #111111; background: #f25a18; color: #fff8df; }}
  .highlight-kicker {{ grid-column: 1; color: #ffd92e; }}
  .highlight-title {{ grid-column: 1; align-self: center; font-size: 25px; font-weight: 900; }}
  .highlight p {{ grid-column: 2; grid-row: 1 / span 2; align-self: center; font-size: 15px; font-weight: 700; line-height: 1.65; }}
  .hotlist {{ margin: 20px 24px 0; padding: 14px 16px 16px; border: 4px solid #111111; background: #fff8df; }}
  .hotlist-heading {{ display: flex; align-items: baseline; gap: 10px; margin-bottom: 12px; }}
  .hotlist-heading span {{ color: #f25a18; }}
  .hotlist-heading h2 {{ font-size: 20px; font-weight: 900; }}
  .hotlist-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; }}
  .hot-topic {{ display: grid; grid-template-columns: 25px 94px 1fr; align-items: center; gap: 7px; min-height: 48px; padding: 6px 8px; border: 2px solid #111111; background: #fff0a6; }}
  .hot-rank {{ display: grid; width: 23px; height: 23px; place-items: center; border: 2px solid #111111; border-radius: 50%; background: #ffd92e; font-size: 11px; font-weight: 900; }}
  .hot-topic strong {{ min-width: 0; font-size: 14px; }}
  .hot-topic small {{ color: #555555; font-size: 10px; line-height: 1.35; }}
  .relationships {{ margin: 20px 24px 0; padding: 14px 16px 16px; border: 4px solid #111111; background: #88d7ff; }}
  .relationships-heading {{ display: flex; align-items: baseline; gap: 10px; margin-bottom: 12px; }}
  .relationships-heading span {{ color: #111111; font-size: 11px; font-weight: 800; }}
  .relationships-heading h2 {{ font-size: 20px; font-weight: 900; }}
  .relationship-list {{ display: grid; gap: 8px; }}
  .relationship-row {{ display: grid; grid-template-columns: 28px 1fr; align-items: center; min-height: 42px; border: 2px solid #111111; background: #ffffff; }}
  .relationship-row span {{ display: grid; height: 100%; place-items: center; border-right: 2px solid #111111; background: #ffd92e; font-size: 11px; font-weight: 900; }}
  .relationship-row p {{ padding: 8px 10px; font-size: 12px; font-weight: 700; line-height: 1.45; }}
  .reference-notes {{ margin: 20px 24px 0; padding: 14px 16px 16px; border: 4px solid #111111; background: #ffffff; }}
  .reference-notes.questions {{ background: #fff0a6; }}
  .reference-heading {{ display: flex; align-items: baseline; gap: 10px; margin-bottom: 12px; }}
  .reference-heading span {{ color: #f25a18; font-size: 11px; font-weight: 800; }}
  .reference-heading h2 {{ font-size: 20px; font-weight: 900; }}
  .reference-list {{ display: grid; gap: 8px; }}
  .reference-row {{ display: grid; grid-template-columns: 28px 1fr; align-items: center; min-height: 42px; border: 2px solid #111111; background: #fff8df; }}
  .reference-row span {{ display: grid; height: 100%; place-items: center; border-right: 2px solid #111111; background: #88d7ff; font-size: 11px; font-weight: 900; }}
  .reference-row p {{ padding: 8px 10px; font-size: 12px; font-weight: 700; line-height: 1.45; }}
  .characters {{ margin: 22px 24px 0; padding: 18px 16px 16px; border: 4px solid #111111; background: #ffffff; }}
  .characters-heading {{ display: flex; align-items: baseline; gap: 10px; margin-bottom: 14px; }}
  .characters-heading span {{ color: #f25a18; font-size: 11px; font-weight: 800; }}
  .characters-heading h2 {{ font-size: 21px; font-weight: 900; }}
  .character-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; }}
  .character-card {{ display: grid; grid-template-columns: 34px 1fr; min-height: 142px; border: 3px solid #111111; background: #fff8df; }}
  .character-rank {{ display: grid; place-items: center; border-right: 2px solid #111111; background: #88d7ff; font-size: 16px; font-weight: 900; }}
  .character-copy {{ min-width: 0; padding: 10px 12px; }}
  .character-copy h3 {{ font-size: 16px; font-weight: 900; line-height: 1.25; }}
  .character-copy strong {{ display: block; margin-top: 4px; color: #f25a18; font-size: 12px; }}
  .character-copy p {{ margin-top: 5px; color: #333333; font-size: 11px; line-height: 1.45; }}
  .character-copy blockquote {{ margin-top: 7px; padding-left: 8px; border-left: 3px solid #111111; font-size: 10px; line-height: 1.45; }}
  .character-copy small {{ display: block; margin-top: 6px; color: #555555; font-size: 10px; }}
  .section {{ margin: 34px 24px 0; }}
  .section-kicker {{ color: #f25a18; }}
  .section h2 {{ margin-top: 6px; }}
  .cases {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 18px; margin-top: 18px; }}
  .case-card {{ min-height: 214px; padding: 16px; border: 4px solid #111111; background: #ffffff; }}
  .case-head {{ display: flex; align-items: center; gap: 12px; }}
  .case-number {{ display: grid; width: 39px; height: 39px; place-items: center; border: 3px solid #111111; border-radius: 50%; background: #ffd92e; font-size: 12px; font-weight: 900; }}
  .case-time {{ color: #f25a18; font-size: 11px; font-weight: 800; }}
  .case-card h3 {{ margin-top: 13px; font-size: 18px; line-height: 1.35; }}
  .case-summary {{ margin-top: 8px; color: #555555; font-size: 12px; line-height: 1.5; }}
  .case-notes {{ margin-top: 14px; padding: 10px 12px 8px 26px; background: #fff0a6; font-size: 11px; line-height: 1.55; }}
  .case-notes li + li {{ margin-top: 4px; }}
  .mvp-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; margin-top: 18px; }}
  .mvp-card {{ display: grid; grid-template-columns: 34px 1fr 48px; align-items: center; min-height: 70px; border: 3px solid #111111; background: #ffffff; }}
  .mvp-rank {{ display: grid; height: 100%; place-items: center; border-right: 2px solid #111111; font-size: 16px; font-weight: 900; }}
  .mvp-copy {{ padding: 8px 10px; }}
  .mvp-name {{ font-size: 15px; font-weight: 800; }}
  .mvp-meta {{ margin-top: 3px; color: #555555; font-size: 10px; }}
  .mvp-score {{ padding-right: 10px; text-align: right; font-size: 28px; font-weight: 900; }}
  .footer {{ margin-top: 34px; padding: 14px 24px; background: #111111; color: #ffd92e; font-size: 10px; }}
</style>
</head>
<body>
<main class="daily-paper">
  <div class="topline"><span>XIAOMAN CHARACTER DAILY</span><span>{}</span></div>
  <header class="hero">
    <div class="hero-group">{}</div>
    <div class="hero-title">小满群聊日报</div>
    <div class="hero-mainline">今日主线：{}</div>
    <div class="hero-opening">{}</div>
    <div class="hero-time">{} · {} 名成员</div>
    <div class="hero-badge">人物<br>主线</div>
  </header>
  {}{}{}{}{}{}{}{}
  <section class="stats">{}</section>
  <section class="timeline">
    <div class="timeline-head"><h2>24H 活跃节奏</h2><div class="peak">峰值 {} 条 / {:02}:00</div></div>
    <svg viewBox="0 0 {chart_width} 106" aria-label="24小时活跃节奏">{}{}{}</svg>
  </section>
  {}
  <footer class="footer">本报告由小满根据最新群聊窗口自动整理 · 长期画像只以公开安全的角色复现计数参与</footer>
</main>
</body>
</html>"#,
            escape_html(&report.report_date),
            escape_html(&report.group_name),
            escape_html(&main_storyline),
            escape_html(&opening_line),
            escape_html(&report.time_range),
            report.member_count,
            story_index_section,
            characters_html,
            highlight_html,
            callbacks_html,
            relationships_html,
            local_life_html,
            open_questions_html,
            cases_html,
            stats_html,
            peak_count,
            peak_idx,
            timeline_svg,
            peak_svg,
            timeline_labels,
            mvp_html
        ))
    }
}

// ---------------------------------------------------------------------------
// Preview CLI
// ---------------------------------------------------------------------------

pub async fn run_render_preview_cli(cli: &Cli) -> Result<()> {
    let (template, width, narrative_file, image_format, html_path, output_path) = match &cli.command
    {
        crate::config::Command::DailyCaseReportRenderPreview {
            template,
            width,
            narrative_file,
            image_format,
            html_path,
            output_path,
        } => (
            template,
            *width,
            narrative_file,
            image_format,
            html_path,
            output_path,
        ),
        _ => bail!("unexpected command"),
    };

    let mut input_json = String::new();
    io::stdin()
        .read_to_string(&mut input_json)
        .context("read ReportData JSON from stdin")?;
    let report: ReportData = serde_json::from_str(&input_json).context("parse ReportData JSON")?;

    let narrative_md = if let Some(path) = narrative_file {
        let path: &Path = path.as_ref();
        Some(
            std::fs::read_to_string(path)
                .with_context(|| format!("read narrative file {}", path.display()))?,
        )
    } else {
        None
    };

    let input = RenderInput {
        report,
        template: template.clone(),
        width,
        narrative_md,
        image_format: image_format.clone(),
    };

    let mut output = render(&input)?;
    output.raster_request.html_path = html_path.clone();
    output.raster_request.output_path = output_path.clone();
    println!("{}", serde_json::to_string_pretty(&output)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::daily_case_report_render::assembly;

    fn sample_character(name: &str, role: &str, rank: usize) -> CharacterCard {
        CharacterCard {
            rank,
            name: name.to_string(),
            role_label: role.to_string(),
            one_liner: "one-liner".to_string(),
            evidence: format!("{} 的证据", name),
            message_count: 3,
            topic_count: 2,
            node_key: node_key(name),
            memory_label: String::new(),
            member_fact_memory_used: false,
            story_function: "推进剧情".to_string(),
            callback_hint: format!("{} 的回调", name),
            arc_label: "今日出场".to_string(),
            relationship_hint: String::new(),
            relationship_target_key: String::new(),
            relationship_topic: String::new(),
            meme_seed: format!("{} 梗", name),
            memory_weight_label: "只按今日表现呈现".to_string(),
            evidence_anchor: format!("daily_character_note:{}", node_key(name)),
            expressive_label: String::new(),
            profile_evidence_count: 1,
            profile_upgrade_status: "daily_note_only".to_string(),
            profile_upgrade_reason: "只有单日轻量信号".to_string(),
            creative_profile_label: String::new(),
            creative_profile_status: String::new(),
            color_bg: "#fef3c7".to_string(),
            color_text: "#92400e".to_string(),
        }
    }

    fn sample_case(title: &str, case_no: &str) -> CaseCard {
        CaseCard {
            case_no: case_no.to_string(),
            title: title.to_string(),
            time_label: "09:00–10:00".to_string(),
            summary: "3 条消息，2 人参与".to_string(),
            bullets: vec![
                "第一个 bullet".to_string(),
                "第二个 bullet".to_string(),
                "第三个 bullet 有没有问题？".to_string(),
            ],
            message_count: 3,
            participant_count: 2,
            top_speaker: "张三".to_string(),
            color_bg: "#fef3c7".to_string(),
            color_text: "#92400e".to_string(),
        }
    }

    fn sample_hot_topic(keyword: &str) -> HotTopic {
        HotTopic {
            rank: 1,
            keyword: keyword.to_string(),
            message_count: 5,
            participant_count: 3,
        }
    }

    fn sample_report() -> ReportData {
        ReportData {
            group_name: "秦托邦的小伙伴（新）".to_string(),
            report_title: "小满群聊日报".to_string(),
            report_date: "2026年08月08日".to_string(),
            time_range: "00:00–23:59".to_string(),
            member_count: 100,
            message_count: 42,
            participant_count: 10,
            case_count: 1,
            suspect_count: 3,
            character_count: 2,
            hourly_counts: vec![1; 24],
            cases: vec![sample_case("关于「Solidity」的讨论", "CASE 01")],
            suspects: vec![],
            characters: vec![
                {
                    let mut c = sample_character("张三", "资料投喂员", 1);
                    c.relationship_target_key = node_key("李四");
                    c.relationship_hint = "和李四围绕「Solidity」同场接力".to_string();
                    c.relationship_topic = "Solidity".to_string();
                    c
                },
                {
                    let mut c = sample_character("李四", "问题发射台", 2);
                    c.relationship_target_key = node_key("张三");
                    c.relationship_hint = "和张三围绕「Solidity」同场接力".to_string();
                    c.relationship_topic = "Solidity".to_string();
                    c
                },
            ],
            hot_topics: vec![sample_hot_topic("Solidity")],
            highlight: Some("今日_highlight".to_string()),
            character_universe: serde_json::Value::Null,
            window_start: "2026-08-08T00:00:00+08:00".to_string(),
            window_end: "2026-08-09T00:00:00+08:00".to_string(),
            timezone: DEFAULT_TIMEZONE.to_string(),
        }
    }

    #[test]
    fn render_output_serializes() {
        let report = ReportData {
            group_name: "G".to_string(),
            report_title: "T".to_string(),
            report_date: "2026年08月08日".to_string(),
            time_range: "00:00–23:59".to_string(),
            member_count: 100,
            message_count: 10,
            participant_count: 5,
            case_count: 2,
            suspect_count: 3,
            character_count: 4,
            hourly_counts: vec![1; 24],
            cases: vec![],
            suspects: vec![],
            characters: vec![],
            hot_topics: vec![],
            highlight: None,
            character_universe: serde_json::Value::Null,
            window_start: String::new(),
            window_end: String::new(),
            timezone: DEFAULT_TIMEZONE.to_string(),
        };
        let input = RenderInput {
            report,
            template: "v3".to_string(),
            width: 750,
            narrative_md: None,
            image_format: "jpeg".to_string(),
        };
        let out = render(&input).unwrap();
        assert!(!out.html.is_empty());
        assert!(out.html.contains("<!DOCTYPE html>"));
    }

    #[test]
    fn build_character_universe_shape() {
        let report = sample_report();
        let universe = assembly::build_character_universe(&report).unwrap();
        assert_eq!(
            universe.get("schema_version").and_then(|v| v.as_str()),
            Some("xiaoman-character-universe-v1")
        );
        assert_eq!(
            universe
                .get("people")
                .and_then(|v| v.as_array())
                .map(|v| v.len()),
            Some(2)
        );
        assert_eq!(
            universe
                .get("events")
                .and_then(|v| v.as_array())
                .map(|v| v.len()),
            Some(1)
        );
        assert!(!universe
            .get("creative_profile_candidates")
            .and_then(|v| v.as_array())
            .unwrap()
            .is_empty());
    }

    #[test]
    fn build_quote_map_counts() {
        let report = sample_report();
        let quote_map = assembly::build_quote_map(&report).unwrap();
        let entries = quote_map.get("entries").and_then(|v| v.as_array()).unwrap();
        assert!(!entries.is_empty());
        let entry_count = quote_map
            .get("entry_count")
            .and_then(|v| v.as_u64())
            .unwrap();
        assert_eq!(entries.len() as u64, entry_count);
    }

    #[test]
    fn build_wiki_bundle_counts() {
        let mut report = sample_report();
        report.character_universe = assembly::build_character_universe(&report).unwrap();
        let quote_map = assembly::build_quote_map(&report).unwrap();
        let wiki_bundle = assembly::build_wiki_bundle(&report, &quote_map).unwrap();
        let counts = wiki_bundle.get("counts").unwrap();
        assert_eq!(counts.get("people").and_then(|v| v.as_u64()), Some(2));
        assert_eq!(counts.get("events").and_then(|v| v.as_u64()), Some(1));
        assert_eq!(
            wiki_bundle
                .get("timeline")
                .and_then(|v| v.as_array())
                .map(|v| v.len()),
            Some(1)
        );
    }

    #[test]
    fn build_draft_bundle_counts() {
        let mut report = sample_report();
        report.character_universe = assembly::build_character_universe(&report).unwrap();
        let quote_map = assembly::build_quote_map(&report).unwrap();
        let wiki_bundle = assembly::build_wiki_bundle(&report, &quote_map).unwrap();
        let draft_bundle = assembly::build_draft_bundle(&report, &quote_map, &wiki_bundle).unwrap();
        assert!(draft_bundle.get("ordinary_digest").is_some());
        assert!(draft_bundle.get("roast_digest").is_some());
        assert!(draft_bundle.get("public_draft").is_some());
        assert!(draft_bundle.get("storyline_memory").is_some());
        let counts = draft_bundle.get("counts").unwrap();
        assert_eq!(
            counts
                .get("ordinary_digest_topic_count")
                .and_then(|v| v.as_u64()),
            Some(1)
        );
    }

    #[test]
    fn build_run_manifest_shape() {
        let mut report = sample_report();
        report.character_universe = assembly::build_character_universe(&report).unwrap();
        let quote_map = assembly::build_quote_map(&report).unwrap();
        let wiki_bundle = assembly::build_wiki_bundle(&report, &quote_map).unwrap();
        let draft_bundle = assembly::build_draft_bundle(&report, &quote_map, &wiki_bundle).unwrap();
        let manifest = assembly::build_run_manifest(
            &report,
            &quote_map,
            &wiki_bundle,
            Some(&draft_bundle),
            Some("chat-123"),
        )
        .unwrap();
        assert_eq!(
            manifest.get("schema_version").and_then(|v| v.as_str()),
            Some("xiaoman-daily-run-manifest-v1")
        );
        assert!(manifest.get("inputs").is_some());
        assert!(manifest.get("outputs").is_some());
        assert!(manifest.get("reference_workshop_steps").is_some());
        assert!(manifest.get("counts").is_some());
        assert!(manifest.get("privacy").is_some());
        assert!(manifest.get("source_chat_ref").is_some());
    }

    #[test]
    fn render_daily_markdown_contains_key_sections() {
        let report = sample_report();
        let markdown = assembly::render_daily_markdown(&report);
        assert!(markdown.contains("# 小满群聊日报｜"));
        assert!(markdown.contains("## 今日一句话"));
        assert!(markdown.contains("## 基本信息"));
        assert!(markdown.contains("## 主要话题"));
        assert!(markdown.contains("## 今日台词"));
        assert!(markdown.contains("## 今日剧中人"));
        assert!(markdown.contains("## 梗和回调候选"));
        assert!(markdown.contains("## 同场关系"));
        assert!(markdown.contains("## 今日主线"));
        assert!(markdown.contains("## 公开边界"));
    }

    #[test]
    fn render_review_report_contains_required_sections() {
        let mut report = sample_report();
        report.character_universe = assembly::build_character_universe(&report).unwrap();
        let quote_map = assembly::build_quote_map(&report).unwrap();
        let wiki_bundle = assembly::build_wiki_bundle(&report, &quote_map).unwrap();
        let draft_bundle = assembly::build_draft_bundle(&report, &quote_map, &wiki_bundle).unwrap();
        let manifest = assembly::build_run_manifest(
            &report,
            &quote_map,
            &wiki_bundle,
            Some(&draft_bundle),
            None,
        )
        .unwrap();
        let review = assembly::render_review_report(
            &report,
            &quote_map,
            &wiki_bundle,
            &draft_bundle,
            &manifest,
        );
        assert!(review.contains("# 小满日报私有审核包｜"));
        assert!(review.contains("## 生成范围"));
        assert!(review.contains("## 审核清单"));
        assert!(review.contains("## 隐私边界"));
        assert!(review.contains("## 可审核人物画像候选"));
        assert!(review.contains("## 产物策略"));
    }

    #[test]
    fn main_storyline_label_prefers_case_and_character() {
        let report = sample_report();
        let label = assembly::main_storyline_label(&report);
        assert!(label.contains("Solidity"));
    }

    #[test]
    fn daily_opening_line_contains_message_count() {
        let report = sample_report();
        let line = assembly::daily_opening_line(&report);
        assert!(line.contains("42 条消息"));
        assert!(line.contains("10 位活跃成员"));
    }

    #[test]
    fn ordinary_digest_open_questions_finds_question_bullets() {
        let report = sample_report();
        let questions = assembly::ordinary_digest_open_questions(&report);
        assert!(!questions.is_empty());
        assert!(questions
            .iter()
            .any(|q| q.contains('?') || q.contains('？')));
    }

    #[test]
    fn escape_html_matches_python_html_escape() {
        use crate::daily_case_report_render::templates::escape_html;
        assert_eq!(
            escape_html("a < b & c > d \"e\" f"),
            "a &lt; b &amp; c &gt; d &quot;e&quot; f"
        );
        assert_eq!(escape_html("it's ok"), "it's ok");
        assert_eq!(escape_html(""), "");
        assert_eq!(escape_html("无特殊字符"), "无特殊字符");
    }

    #[test]
    fn render_inline_converts_bold_italic_code_and_escapes() {
        use crate::daily_case_report_render::templates::render_inline;
        // Bold converts to <strong>, no stray asterisks.
        assert_eq!(
            render_inline("**明日线索**：据透露"),
            "<strong>明日线索</strong>：据透露"
        );
        // Italic converts to <em>.
        assert_eq!(render_inline("这是*重点*内容"), "这是<em>重点</em>内容");
        // Inline code markers are dropped, content kept.
        assert_eq!(render_inline("运行`cargo test`即可"), "运行cargo test即可");
        // HTML is still escaped alongside markdown conversion.
        assert_eq!(
            render_inline("**a < b** & \"c\""),
            "<strong>a &lt; b</strong> &amp; &quot;c&quot;"
        );
        // Plain text passes through unchanged.
        assert_eq!(render_inline("无标记文本"), "无标记文本");
    }

    #[test]
    fn render_inline_leaves_unmatched_markers_as_plain_text() {
        use crate::daily_case_report_render::templates::render_inline;
        // A lone `*` (multiplication, wildcard) stays a literal asterisk and
        // does NOT open an <em> that would swallow the rest of the poster.
        assert_eq!(render_inline("2 * 3 = 6"), "2 * 3 = 6");
        assert_eq!(render_inline("匹配 *.log 文件"), "匹配 *.log 文件");
        // An unterminated bold marker is emitted verbatim, no <strong> injected.
        assert_eq!(render_inline("这是**没闭合的加粗"), "这是**没闭合的加粗");
        assert_eq!(render_inline("这是*没闭合的斜体"), "这是*没闭合的斜体");
        // An empty span `** **` is not treated as bold; markers stay literal.
        assert_eq!(render_inline("a ** ** b"), "a ** ** b");
        // An unmatched backtick stays literal too.
        assert_eq!(render_inline("命令 ` 没闭合"), "命令 ` 没闭合");
        // A later valid pair still converts even after an earlier lone marker.
        assert_eq!(
            render_inline("2 * 3 然后**加粗**"),
            "2 * 3 然后<strong>加粗</strong>"
        );
    }

    #[test]
    fn bar_svg_matches_python_golden() {
        use crate::daily_case_report_render::templates::bar_svg;
        let svg = bar_svg(
            &[
                0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15, 16, 17, 18, 19, 20, 21, 22,
                23,
            ],
            23,
            600,
            80,
        );
        assert!(svg.contains(r#"width="23""#));
        assert!(svg.contains(r#"height="80""#));
        assert!(svg.contains(r#"x="301""#));
        assert!(svg.contains(r#"y="0""#));
        assert!(svg.contains(r#"width="23""#));
        assert!(svg.contains(r#"height="80""#));
        assert!(svg.starts_with("<rect "));
        assert_eq!(svg.lines().count(), 24);
    }

    #[test]
    fn render_v3_on_sample_report_returns_html() {
        let report = sample_report();
        let html = templates::render_v3(&report, 750).unwrap();
        assert!(!html.is_empty());
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("小满群聊日报"));
        assert!(html.contains("24H 活跃节奏"));
    }

    #[test]
    fn render_newspaper_on_sample_report_returns_html() {
        let report = sample_report();
        let html = templates::render_newspaper(&report, 750).unwrap();
        assert!(!html.is_empty());
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("小满时报"));
        assert!(html.contains("人物出场表"));
    }

    #[test]
    fn render_newspaper_elegant_on_sample_report_returns_html() {
        let report = sample_report();
        let html = templates::render_newspaper_elegant(&report, 750).unwrap();
        assert!(!html.is_empty());
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("小满群聊日报"));
        assert!(html.contains("24H 活跃节奏"));
    }

    #[test]
    fn render_roast_long_image_on_sample_report_returns_html() {
        let report = sample_report();
        let narrative_md = r#"# 秦托邦 | 2026年08月08日 | 今日群聊观察

**战报**：群里今天把 Solidity 讨论续上了。

## 第一章 开场

今天聊到 Solidity 的新特性。

## 明日线索

明天继续看合约。

## 人物速写

> **张三**：资料投喂员，把链接甩出来。

## 今日金句

**"所有讨论都该有个落脚点。"** —— 李四
"#;
        let input = RenderInput {
            report,
            template: "roast-long-image".to_string(),
            width: 750,
            narrative_md: Some(narrative_md.to_string()),
            image_format: "jpeg".to_string(),
        };
        let html = templates::render_roast_long_image(&input).unwrap();
        assert!(!html.is_empty());
        assert!(html.starts_with("<!DOCTYPE html>"));
        assert!(html.contains("秦 托 邦"));
        assert!(html.contains("今日群聊观察"));
        assert!(html.contains(" Solidity "));
        assert!(html.contains("张三"));
        assert!(html.contains("所有讨论都该有个落脚点"));
    }

    fn load_fixture_report() -> ReportData {
        serde_json::from_str(include_str!(
            "../fixtures/daily_case_report_render_preview_input.json"
        ))
        .expect("fixture input must parse")
    }

    #[test]
    fn render_v3_matches_golden_fixture() {
        let report = load_fixture_report();
        let input = RenderInput {
            report,
            template: "v3".to_string(),
            width: 750,
            narrative_md: None,
            image_format: "jpeg".to_string(),
        };
        let output = render(&input).unwrap();
        let expected =
            include_str!("../fixtures/daily_case_report_render_preview_v3_expected.html");
        assert_eq!(
            output.html, expected,
            "v3 HTML does not match golden fixture"
        );
    }

    #[test]
    fn render_newspaper_matches_golden_fixture() {
        let report = load_fixture_report();
        let input = RenderInput {
            report,
            template: "newspaper".to_string(),
            width: 1080,
            narrative_md: None,
            image_format: "jpeg".to_string(),
        };
        let output = render(&input).unwrap();
        let expected =
            include_str!("../fixtures/daily_case_report_render_preview_newspaper_expected.html");
        assert_eq!(
            output.html, expected,
            "newspaper HTML does not match golden fixture"
        );
    }

    #[test]
    fn render_newspaper_elegant_matches_golden_fixture() {
        let report = load_fixture_report();
        let input = RenderInput {
            report,
            template: "newspaper-elegant".to_string(),
            width: 1080,
            narrative_md: None,
            image_format: "jpeg".to_string(),
        };
        let output = render(&input).unwrap();
        let expected = include_str!(
            "../fixtures/daily_case_report_render_preview_newspaper_elegant_expected.html"
        );
        assert_eq!(
            output.html, expected,
            "newspaper-elegant HTML does not match golden fixture"
        );
    }

    #[test]
    fn render_roast_long_image_matches_golden_fixture() {
        let report = load_fixture_report();
        let narrative_md =
            include_str!("../fixtures/daily_case_report_render_preview_roast_narrative.md");
        let input = RenderInput {
            report,
            template: "roast-long-image".to_string(),
            width: 1080,
            narrative_md: Some(narrative_md.to_string()),
            image_format: "jpeg".to_string(),
        };
        let output = render(&input).unwrap();
        let expected = include_str!(
            "../fixtures/daily_case_report_render_preview_roast_long_image_expected.html"
        );
        assert_eq!(
            output.html, expected,
            "roast-long-image HTML does not match golden fixture"
        );
    }

    #[test]
    #[ignore = "regenerates the roast golden fixture on demand"]
    fn regenerate_roast_long_image_golden_fixture() {
        let report = load_fixture_report();
        let narrative_md =
            include_str!("../fixtures/daily_case_report_render_preview_roast_narrative.md");
        let input = RenderInput {
            report,
            template: "roast-long-image".to_string(),
            width: 1080,
            narrative_md: Some(narrative_md.to_string()),
            image_format: "jpeg".to_string(),
        };
        let output = render(&input).unwrap();
        std::fs::write(
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/fixtures/daily_case_report_render_preview_roast_long_image_expected.html"
            ),
            output.html,
        )
        .unwrap();
    }

    #[test]
    fn render_roast_long_image_uses_deterministic_fallback_without_narrative() {
        let report = sample_report();
        let input = RenderInput {
            report,
            template: "roast-long-image".to_string(),
            width: 1080,
            narrative_md: None,
            image_format: "jpeg".to_string(),
        };
        let output = render(&input).unwrap();
        assert!(output.html.contains("<!DOCTYPE html>"));
        assert!(output.html.contains("<title>") || output.html.contains("吐槽日报"));
        assert!(output.html.contains("42条消息 · 10人开口"));
        // Uses the deterministic case summary instead of LLM prose.
        assert!(output.html.contains("关于「Solidity」的讨论"));
        assert_eq!(output.raster_request.image_format, "jpeg");
    }

    #[test]
    fn render_preserves_non_default_image_format() {
        let report = sample_report();
        let input = RenderInput {
            report,
            template: "v3".to_string(),
            width: 750,
            narrative_md: None,
            image_format: "png".to_string(),
        };
        let output = render(&input).unwrap();
        assert_eq!(output.raster_request.image_format, "png");
    }
}
