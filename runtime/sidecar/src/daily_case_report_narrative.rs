//! Daily case-report narrative generation (PR 4 of the Rust migration plan
//! `docs/plans/active/xiaoman-daily-case-report-rust-migration.md`).
//!
//! Ports the LLM roast/normal narrative layer from
//! `workflows/xiaoman-daily-case-report/narrative_generator.py` into the Rust
//! sidecar, using the existing `bounded_http` client for the provider call.
//!
//! This module is *not* wired into the MCP tool or scheduled worker yet; PR 6
//! performs the cutover. This PR only exposes a preview CLI command and
//! fixture-level parity tests.

use std::io::{self, Read};
use std::path::Path;
use std::time::Duration;

use anyhow::{anyhow, bail, Context, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use serde_json::json;
use url::Url;

use crate::bounded_http::{HttpClient, HttpRequestError};
use crate::config::Cli;
use zeroize::Zeroizing;

const DEFAULT_MODEL: &str = "gpt-4o-mini";
const DEFAULT_TEMPERATURE: f32 = 0.8;
const DEFAULT_MAX_TOKENS: usize = 12000;
const DEFAULT_TIMEOUT_SECONDS: u64 = 180;
const MAX_EMPTY_CONTENT_RETRIES: usize = 2;
const MAX_QUOTES: usize = 40;
const MAX_IMAGES: usize = 6;

const LLM_BASE_URL_ENVS: &[&str] = &[
    "QINTOPIA_LLM_BASE_URL",
    "QINTOPIA_XIAOMAN_LLM_BASE_URL",
    "OPENAI_BASE_URL",
];
const LLM_API_KEY_ENVS: &[&str] = &[
    "QINTOPIA_LLM_API_KEY",
    "QINTOPIA_XIAOMAN_LLM_API_KEY",
    "OPENAI_API_KEY",
];
const LLM_MODEL_ENVS: &[&str] = &[
    "QINTOPIA_LLM_MODEL",
    "QINTOPIA_XIAOMAN_LLM_MODEL",
    "OPENAI_MODEL",
];

/// Input shape: the subset of the deterministic report that the narrative
/// layer reads. Callers (including the preview CLI) supply sanitized JSON.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NarrativeReport {
    pub group_name: String,
    pub report_date: String,
    pub time_range: String,
    pub message_count: usize,
    pub participant_count: usize,
    pub cases: Vec<NarrativeCase>,
    pub characters: Vec<NarrativeCharacter>,
    #[serde(default)]
    pub hot_topics: Vec<NarrativeHotTopic>,
    #[serde(default)]
    pub messages: Vec<NarrativeMessage>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NarrativeCase {
    pub case_no: String,
    pub title: String,
    pub time_label: String,
    pub summary: String,
    pub message_count: usize,
    pub participant_count: usize,
    pub top_speaker: String,
    #[serde(default)]
    pub bullets: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NarrativeCharacter {
    pub name: String,
    pub role_label: String,
    pub one_liner: String,
    #[serde(default)]
    pub story_function: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NarrativeHotTopic {
    pub keyword: String,
    pub message_count: usize,
    pub participant_count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct NarrativeMessage {
    #[serde(default)]
    pub sender_name: String,
    #[serde(default)]
    pub text: String,
}

/// Sanitized grounding block: only facts the model may use.
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct Grounding {
    pub group_name: String,
    pub report_date: String,
    pub time_range: String,
    pub message_count: usize,
    pub participant_count: usize,
    pub cases: Vec<GroundingCase>,
    pub characters: Vec<GroundingCharacter>,
    pub hot_topics: Vec<GroundingHotTopic>,
    pub quotes: Vec<GroundingQuote>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GroundingCase {
    pub case_no: String,
    pub title: String,
    pub time_label: String,
    pub summary: String,
    pub message_count: usize,
    pub participant_count: usize,
    pub top_speaker: String,
    pub bullets: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GroundingCharacter {
    pub name: String,
    pub role_label: String,
    pub one_liner: String,
    pub story_function: String,
    pub evidence: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GroundingHotTopic {
    pub keyword: String,
    pub message_count: usize,
    pub participant_count: usize,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct GroundingQuote {
    pub speaker: String,
    pub text: String,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct ImageGrounding {
    pub src: String,
    pub caption: String,
}

/// LLM endpoint configuration.
#[derive(Debug, Clone)]
pub struct NarrativeConfig {
    pub base_url: String,
    pub api_key: Zeroizing<String>,
    pub model: String,
    pub temperature: f32,
    pub max_tokens: usize,
    pub http_timeout: Duration,
    pub allow_insecure_http: bool,
}

impl NarrativeConfig {
    #[allow(dead_code)]
    pub fn from_env() -> Result<Self> {
        Self::from_env_with_overrides(None, None)
    }

    pub fn from_env_with_overrides(base_url: Option<&str>, model: Option<&str>) -> Result<Self> {
        let resolved_base = base_url
            .map(|value| value.to_string())
            .or_else(|| first_env_value(LLM_BASE_URL_ENVS))
            .ok_or_else(|| {
                anyhow!(
                    "narrative generation requires an OpenAI-compatible endpoint; set one of {} and {}",
                    LLM_BASE_URL_ENVS.join("/"),
                    LLM_API_KEY_ENVS.join("/")
                )
            })?;
        let resolved_key = first_env_value(LLM_API_KEY_ENVS).ok_or_else(|| {
            anyhow!(
                "narrative generation requires an OpenAI-compatible API key; set one of {}",
                LLM_API_KEY_ENVS.join("/")
            )
        })?;
        let resolved_model = model
            .map(|value| value.to_string())
            .or_else(|| first_env_value(LLM_MODEL_ENVS))
            .unwrap_or_else(|| DEFAULT_MODEL.to_string());

        Ok(Self {
            base_url: resolved_base.trim_end_matches('/').to_string(),
            api_key: Zeroizing::new(resolved_key),
            model: resolved_model,
            temperature: DEFAULT_TEMPERATURE,
            max_tokens: DEFAULT_MAX_TOKENS,
            http_timeout: Duration::from_secs(DEFAULT_TIMEOUT_SECONDS),
            allow_insecure_http: false,
        })
    }

    pub fn from_cli(cli: &Cli) -> Result<Self> {
        let mut config = Self::from_env_with_overrides(
            cli.daily_case_report_narrative_base_url.as_deref(),
            cli.daily_case_report_narrative_model.as_deref(),
        )?;
        config.http_timeout = Duration::from_secs(cli.daily_case_report_narrative_timeout_seconds);
        config.max_tokens = cli.daily_case_report_narrative_max_tokens;
        Self::validate_production_base_url(&config.base_url, config.allow_insecure_http)?;
        Ok(config)
    }

    fn validate_production_base_url(base_url: &str, allow_insecure_http: bool) -> Result<()> {
        if !allow_insecure_http && !base_url.starts_with("https://") {
            bail!(
                "narrative generation base URL must use HTTPS in production; got {}",
                base_url
            );
        }
        Ok(())
    }
}

fn first_env_value(names: &[&str]) -> Option<String> {
    for name in names {
        if let Ok(value) = std::env::var(name) {
            let trimmed = value.trim();
            if !trimmed.is_empty() {
                return Some(trimmed.to_string());
            }
        }
    }
    None
}

fn quote_regex() -> Regex {
    Regex::new(r#"^[\s>"'「『]|^\[?引用|转述"#).expect("quote regex must compile")
}

fn clean_quote(text: &str, limit: usize) -> String {
    let collapsed: String = text.split_whitespace().collect::<Vec<_>>().join(" ");
    collapsed.chars().take(limit).collect()
}

pub fn build_grounding(report: &NarrativeReport) -> Grounding {
    let cases: Vec<GroundingCase> = report
        .cases
        .iter()
        .map(|case| GroundingCase {
            case_no: case.case_no.clone(),
            title: case.title.clone(),
            time_label: case.time_label.clone(),
            summary: clean_quote(&case.summary, 160),
            message_count: case.message_count,
            participant_count: case.participant_count,
            top_speaker: case.top_speaker.clone(),
            bullets: case.bullets.iter().take(8).cloned().collect(),
        })
        .collect();

    let characters: Vec<GroundingCharacter> = report
        .characters
        .iter()
        .map(|character| GroundingCharacter {
            name: character.name.clone(),
            role_label: character.role_label.clone(),
            one_liner: character.one_liner.clone(),
            story_function: character.story_function.clone(),
            evidence: clean_quote(&character.evidence, 120),
        })
        .collect();

    let hot_topics: Vec<GroundingHotTopic> = report
        .hot_topics
        .iter()
        .map(|topic| GroundingHotTopic {
            keyword: topic.keyword.clone(),
            message_count: topic.message_count,
            participant_count: topic.participant_count,
        })
        .collect();

    let re = quote_regex();
    let mut quotes = Vec::new();
    for message in &report.messages {
        let text = message.text.trim();
        let text_len = text.chars().count();
        if !(6..=40).contains(&text_len) {
            continue;
        }
        if re.is_match(text) {
            continue;
        }
        quotes.push(GroundingQuote {
            speaker: if message.sender_name.trim().is_empty() {
                "?".to_string()
            } else {
                message.sender_name.clone()
            },
            text: clean_quote(text, 40),
        });
        if quotes.len() >= MAX_QUOTES {
            break;
        }
    }

    Grounding {
        group_name: report.group_name.clone(),
        report_date: report.report_date.clone(),
        time_range: report.time_range.clone(),
        message_count: report.message_count,
        participant_count: report.participant_count,
        cases,
        characters,
        hot_topics,
        quotes,
    }
}

pub fn format_grounding_markdown(grounding: &Grounding) -> String {
    let mut lines = vec![
        format!("群名：{}", grounding.group_name),
        format!(
            "日期：{}（{}）",
            grounding.report_date, grounding.time_range
        ),
        format!(
            "消息总量：{} 条，活跃：{} 人",
            grounding.message_count, grounding.participant_count
        ),
        String::new(),
        String::from("## 今日主线（确定性聚类结果，事实来源）"),
    ];

    for case in &grounding.cases {
        lines.push(format!(
            "- {} {}（{}）：{} 条 / {} 人参与，牵头 {}",
            case.case_no,
            case.title,
            case.time_label,
            case.message_count,
            case.participant_count,
            case.top_speaker
        ));
        if !case.summary.is_empty() {
            lines.push(format!("  - 摘要：{}", case.summary));
        }
        for bullet in case.bullets.iter().take(3) {
            lines.push(format!("  - {}", clean_quote(bullet, 80)));
        }
    }

    lines.push(String::new());
    lines.push(String::from("## 今日人物（确定性角色标签，事实来源）"));
    for character in &grounding.characters {
        lines.push(format!(
            "- {}（{}）：{}",
            character.name, character.role_label, character.one_liner
        ));
    }

    lines.push(String::new());
    lines.push(String::from("## 真实语录（可直接引用的原文片段）"));
    for quote in &grounding.quotes {
        lines.push(format!("- {}：「{}」", quote.speaker, quote.text));
    }

    lines.join("\n")
}

pub fn extract_image_grounding(reviewed_image_dir: Option<&Path>) -> Vec<ImageGrounding> {
    let Some(dir) = reviewed_image_dir else {
        return Vec::new();
    };

    let entries: Vec<_> = match std::fs::read_dir(dir) {
        Ok(read_dir) => read_dir
            .filter_map(|entry| entry.ok())
            .map(|entry| entry.path())
            .filter(|path| {
                path.extension()
                    .and_then(|ext| ext.to_str())
                    .map(|ext| {
                        let lower = ext.to_lowercase();
                        matches!(lower.as_str(), "png" | "jpg" | "jpeg" | "webp")
                    })
                    .unwrap_or(false)
            })
            .collect(),
        Err(_) => return Vec::new(),
    };

    let mut entries = entries;
    entries.sort();

    entries
        .into_iter()
        .take(MAX_IMAGES)
        .map(|path| ImageGrounding {
            src: path.to_string_lossy().to_string(),
            caption: String::new(),
        })
        .collect()
}

const ROAST_SYSTEM_PROMPT: &str = "你是一位社区报纸的吐槽版主笔，为微信群「秦托邦的小伙伴（新）」撰写每日吐槽日报。\n风格要求（严格遵循，这是产品定位）：\n- 像读报纸专栏：有起承转合、有包袱、有金句，不是会议纪要，不是数据摘要。\n- 标题/章节标题本身就是观点或玩笑，不是干瘪的话题名。\n- 每章至少一句\"转折句\"（读者以为要夸时拆穿），至少一句可单独提取的金句。\n- 用真实群聊的语录和细节，不要编造人名、事件或对话。\n- 语气可以毒舌，但只调侃群体行为/语言现象，绝不评价外貌、健康、职业、收入、年龄、性别、地域、身份属性。\n- 可以出现发言榜，但如果当天故事性强，优先讲故事，榜单可省略。\n- 输出纯 Markdown，结构见用户指令中的模板。";

const ROAST_USER_TEMPLATE: &str = "以下是今日群聊的确定性聚类结果（全部为事实，作为你叙事的锚点，不得脱离）：\n\n{grounding}\n\n请按下面的 roast 模板写出今天的吐槽日报 Markdown：\n\n```\n# 秦托邦吐槽日报 | {date} | {{副标题：当天最大荒诞事件的一句话包袱}}\n\n**战报**：{{消息数}}条消息，{{发言人数}}人开口。{{一句话概括当天荒诞指数或群体行为模式}}\n\n---\n\n## 第一章：{{章节标题——观点或包袱，不是话题名}}\n\n{{叙事段落：前两句正经预期，第三句拐弯；中间展开细节，用角色或对话链；至少一句转折句+一句金句}}\n\n![[{{图片路径|可选宽}}]]\n_{{发送者} {时间} — {一句话 caption，本身是吐槽或梗}}_\n\n---\n\n## 第N章：{{同上}}\n\n{{每篇 4-7 章，按时间或话题组织；最后一章可以是\"明日线索\"或\"今日金句\"}}\n\n---\n\n## 今日人物速写\n\n> **{{人物名}}**\n> {{一句话角色定位}}。{{当天最能代表这个人的一个梗或行为}}。\n\n## 今日金句\n\n**\"{{引用原文}}\"** ——{{发言者}}，{{为什么是今日最佳}}\n\n---\n\n*秦托邦 · 小满吐槽日报。所有引用可回溯至当天 quote-map。*\n```\n\n只输出 Markdown，不要解释。";

const NORMAL_SYSTEM_PROMPT: &str = "你为微信群「秦托邦的小伙伴（新）」撰写每日内部日报，风格像本地生活故事，\n不像会议纪要或摘要报告。用真实群聊细节，不编造。输出纯 Markdown，结构见用户指令。";

const NORMAL_USER_TEMPLATE: &str = "以下是今日群聊的确定性聚类结果（全部为事实，作为叙事锚点，不得脱离）：\n\n{grounding}\n\n请写出今天的内部日报 Markdown：以「今日一句话」开头，下面用 3-5 个有故事性的章节组织，\n每章可引用真实语录。可以省略发言榜，优先讲故事。只输出 Markdown。";

fn build_prompt(style: &str, grounding: &Grounding) -> Result<(String, String)> {
    let grounding_md = format_grounding_markdown(grounding);

    let (system, user) = match style {
        "roast" => {
            let user = ROAST_USER_TEMPLATE
                .replace("{grounding}", &grounding_md)
                .replace("{date}", &grounding.report_date);
            (ROAST_SYSTEM_PROMPT.to_string(), user)
        }
        "normal" => {
            let user = NORMAL_USER_TEMPLATE.replace("{grounding}", &grounding_md);
            (NORMAL_SYSTEM_PROMPT.to_string(), user)
        }
        _ => bail!("unsupported narrative style: {style}; expected roast or normal"),
    };

    Ok((system, user))
}

fn append_image_grounding(user: &mut String, images: &[ImageGrounding]) {
    if images.is_empty() {
        return;
    }
    let mut block =
        String::from("\n\n可选配图（仅当与叙事强相关时使用，caption 本身也要是吐槽或梗）：\n");
    for image in images {
        block.push_str(&format!(
            "![[{}|150]]\n_{}_\n",
            image.src,
            if image.caption.is_empty() {
                "（待补 caption）"
            } else {
                &image.caption
            }
        ));
    }
    user.push_str(&block);
}

fn chat_completion_url(base_url: &str) -> Result<Url> {
    Url::parse(&format!("{base_url}/chat/completions"))
        .with_context(|| "parse chat completions endpoint")
}

fn build_request_body(
    config: &NarrativeConfig,
    messages: Vec<serde_json::Value>,
) -> Result<Vec<u8>> {
    serde_json::to_vec(&json!({
        "model": config.model,
        "messages": messages,
        "temperature": config.temperature,
        "max_tokens": config.max_tokens,
    }))
    .with_context(|| "serialize chat completion request")
}

fn chat_completion(
    client: &HttpClient,
    config: &NarrativeConfig,
    system: &str,
    user: &str,
    attempt: usize,
) -> Result<String> {
    chat_completion_with_form(client, config, system, user, attempt, false)
}

fn chat_completion_with_fallback(
    client: &HttpClient,
    config: &NarrativeConfig,
    system: &str,
    user: &str,
    attempt: usize,
) -> Result<String> {
    chat_completion_with_form(client, config, system, user, attempt, true)
}

fn chat_completion_with_form(
    client: &HttpClient,
    config: &NarrativeConfig,
    system: &str,
    user: &str,
    attempt: usize,
    use_fallback: bool,
) -> Result<String> {
    let url = chat_completion_url(&config.base_url)?;
    let max_tokens = config.max_tokens.saturating_add(attempt * 4000);

    let messages: Vec<serde_json::Value> = if use_fallback {
        let combined = format!("{system}\n\n{user}");
        vec![json!({"role": "user", "content": combined})]
    } else {
        vec![
            json!({"role": "system", "content": system}),
            json!({"role": "user", "content": user}),
        ]
    };
    let body = build_request_body(
        &NarrativeConfig {
            max_tokens,
            ..config.clone()
        },
        messages,
    )?;

    let stage = if use_fallback {
        "provider_request_fallback"
    } else {
        "provider_request"
    };
    let response = client
        .request(
            "POST",
            &url,
            &[
                (
                    "Authorization",
                    format!("Bearer {}", config.api_key.as_str()),
                ),
                ("Content-Type", "application/json".to_string()),
                ("Accept", "application/json".to_string()),
            ],
            &body,
            max_response_body_bytes(),
        )
        .map_err(|error| classify_http_error(error, stage))?;

    if response.status == 400 && !use_fallback && attempt == 0 {
        return chat_completion_with_fallback(client, config, system, user, attempt);
    }

    if !(200..300).contains(&response.status) {
        bail!(
            "provider returned non-success status {status}; response_keys={response_keys:?}",
            status = response.status,
            response_keys = parse_response_keys(&response.body)
        );
    }

    parse_completion_response(
        client,
        &response.body,
        config,
        system,
        user,
        attempt,
        use_fallback,
    )
}

fn parse_completion_response(
    client: &HttpClient,
    body: &[u8],
    config: &NarrativeConfig,
    system: &str,
    user: &str,
    attempt: usize,
    use_fallback: bool,
) -> Result<String> {
    let data: serde_json::Value =
        serde_json::from_slice(body).with_context(|| "parse provider response as JSON")?;

    let choices = data
        .get("choices")
        .and_then(|value| value.as_array())
        .ok_or_else(|| {
            anyhow!(
                "no choices in response: keys={keys:?}",
                keys = value_keys(&data)
            )
        })?;

    if choices.is_empty() {
        bail!("empty choices in response");
    }

    let message = choices[0].get("message").ok_or_else(|| {
        anyhow!(
            "missing message in first choice; keys={keys:?}",
            keys = value_keys(&choices[0])
        )
    })?;

    let content = message
        .get("content")
        .and_then(|value| value.as_str())
        .unwrap_or("")
        .trim();

    if content.is_empty() {
        let finish_reason = choices[0]
            .get("finish_reason")
            .and_then(|value| value.as_str())
            .unwrap_or("unknown");
        if attempt < MAX_EMPTY_CONTENT_RETRIES {
            return chat_completion_with_form(
                client,
                config,
                system,
                user,
                attempt + 1,
                use_fallback,
            );
        }
        bail!(
            "empty content in message; keys={keys:?}; finish={finish_reason}; response_keys={response_keys:?}",
            keys = value_keys(message),
            response_keys = value_keys(&data)
        );
    }

    Ok(content.to_string())
}

fn client_from_config(config: &NarrativeConfig) -> Result<HttpClient> {
    Ok(HttpClient::production_with_timeout(config.http_timeout))
}

fn max_response_body_bytes() -> usize {
    // A generous upper bound for chat completion markdown output.
    2 * 1024 * 1024
}

fn value_keys(value: &serde_json::Value) -> Vec<String> {
    value
        .as_object()
        .map(|object| object.keys().map(String::from).collect())
        .unwrap_or_default()
}

fn parse_response_keys(body: &[u8]) -> Vec<String> {
    serde_json::from_slice::<serde_json::Value>(body)
        .map(|value| value_keys(&value))
        .unwrap_or_default()
}

fn classify_http_error(error: HttpRequestError, stage: &str) -> anyhow::Error {
    // Preserve the original error chain while adding stage context.
    error.into_source().context(format!("{stage} failed"))
}

pub fn generate_narrative(
    style: &str,
    report: &NarrativeReport,
    config: &NarrativeConfig,
    reviewed_image_dir: Option<&Path>,
) -> Result<String> {
    let client = client_from_config(config)?;
    generate_narrative_with_client(&client, style, report, config, reviewed_image_dir)
}

pub fn generate_narrative_with_client(
    client: &HttpClient,
    style: &str,
    report: &NarrativeReport,
    config: &NarrativeConfig,
    reviewed_image_dir: Option<&Path>,
) -> Result<String> {
    let grounding = build_grounding(report);
    let (system, mut user) = build_prompt(style, &grounding)?;

    let images = extract_image_grounding(reviewed_image_dir);
    append_image_grounding(&mut user, &images);

    chat_completion(client, config, &system, &user, 0)
}

pub async fn run_preview_cli(cli: &Cli, style: &str) -> Result<()> {
    let config = NarrativeConfig::from_cli(cli)?;
    let mut stdin = io::stdin();
    let mut input = String::new();
    stdin
        .read_to_string(&mut input)
        .with_context(|| "read narrative report JSON from stdin")?;
    let report: NarrativeReport =
        serde_json::from_str(&input).with_context(|| "parse narrative report JSON from stdin")?;
    let markdown = generate_narrative(style, &report, &config, None)?;
    println!("{markdown}");
    Ok(())
}

#[cfg(test)]
mod tests {
    use std::io::Write;
    use std::net::{TcpListener, TcpStream};
    use std::thread;

    use super::*;

    // Serializes tests that mutate process-wide environment variables so parallel
    // test runners do not observe each other's (potentially conflicting) state.
    static ENV_LOCK: std::sync::Mutex<()> = std::sync::Mutex::new(());

    fn sample_report() -> NarrativeReport {
        NarrativeReport {
            group_name: "秦托邦的小伙伴（新）".to_string(),
            report_date: "2026年08月08日".to_string(),
            time_range: "00:00-23:59".to_string(),
            message_count: 42,
            participant_count: 7,
            cases: vec![NarrativeCase {
                case_no: "CASE 01".to_string(),
                title: "Rust 迁移".to_string(),
                time_label: "09:00-10:30".to_string(),
                summary: "讨论把日报中间层迁到 Rust。".to_string(),
                message_count: 12,
                participant_count: 3,
                top_speaker: "小满".to_string(),
                bullets: vec!["先迁 analyzer".to_string(), "再迁 narrative".to_string()],
            }],
            characters: vec![NarrativeCharacter {
                name: "小满".to_string(),
                role_label: "活动推进者".to_string(),
                one_liner: "把松散聊天推成下一步行动".to_string(),
                story_function: "推进剧情".to_string(),
                evidence: "今天我们先把 analyzer 跑通。".to_string(),
            }],
            hot_topics: vec![NarrativeHotTopic {
                keyword: "Rust".to_string(),
                message_count: 15,
                participant_count: 4,
            }],
            messages: vec![
                NarrativeMessage {
                    sender_name: "小满".to_string(),
                    text: "今天我们先把 analyzer 跑通".to_string(),
                },
                NarrativeMessage {
                    sender_name: "阿亮".to_string(),
                    text: "赞同".to_string(),
                },
                NarrativeMessage {
                    sender_name: "匿名".to_string(),
                    text: "> 引用前文".to_string(),
                },
            ],
        }
    }

    #[test]
    fn grounding_extracts_quotes_with_length_filter() {
        let report = sample_report();
        let grounding = build_grounding(&report);

        assert_eq!(grounding.quotes.len(), 1);
        assert_eq!(grounding.quotes[0].speaker, "小满");
        assert_eq!(grounding.quotes[0].text, "今天我们先把 analyzer 跑通");
    }

    #[test]
    fn grounding_ignores_quote_prefixed_messages() {
        let mut report = sample_report();
        report.messages.push(NarrativeMessage {
            sender_name: "某人".to_string(),
            text: "转述一下昨天的结论".to_string(),
        });
        let grounding = build_grounding(&report);

        assert!(!grounding
            .quotes
            .iter()
            .any(|quote| quote.text.contains("转述")));
    }

    #[test]
    fn grounding_cleans_quote_whitespace() {
        let mut report = sample_report();
        report.messages.push(NarrativeMessage {
            sender_name: "小满".to_string(),
            text: "Rust   迁移\t分三步".to_string(),
        });
        let grounding = build_grounding(&report);

        assert!(grounding
            .quotes
            .iter()
            .any(|quote| quote.text == "Rust 迁移 分三步"));
    }

    #[test]
    fn grounding_caps_bullets_and_quotes() {
        let mut report = sample_report();
        report.cases[0].bullets = (0..20).map(|index| format!("bullet {index}")).collect();
        report.messages = (0..60)
            .map(|index| NarrativeMessage {
                sender_name: "user".to_string(),
                text: format!("message {index}"),
            })
            .collect();
        let grounding = build_grounding(&report);

        assert_eq!(grounding.cases[0].bullets.len(), 8);
        assert_eq!(grounding.quotes.len(), MAX_QUOTES);
    }

    #[test]
    fn format_grounding_markdown_contains_sections() {
        let grounding = build_grounding(&sample_report());
        let markdown = format_grounding_markdown(&grounding);

        assert!(markdown.contains("## 今日主线"));
        assert!(markdown.contains("## 今日人物"));
        assert!(markdown.contains("## 真实语录"));
        assert!(markdown.contains("CASE 01"));
        assert!(markdown.contains("小满"));
    }

    #[test]
    fn roast_prompt_replaces_only_real_placeholders() {
        let grounding = build_grounding(&sample_report());
        let (_system, user) = build_prompt("roast", &grounding).expect("roast prompt builds");

        assert!(!user.contains("{grounding}"));
        assert!(!user.contains("{date}"));
        // Literal {{…}} examples shown to the model must survive.
        assert!(user.contains("{{副标题：当天最大荒诞事件的一句话包袱}}"));
        assert!(user.contains("{{消息数}}条消息"));
    }

    #[test]
    fn normal_prompt_has_no_date_placeholder() {
        let grounding = build_grounding(&sample_report());
        let (_system, user) = build_prompt("normal", &grounding).expect("normal prompt builds");

        assert!(!user.contains("{grounding}"));
        assert!(!user.contains("{date}"));
    }

    #[test]
    fn unsupported_style_fails() {
        let grounding = build_grounding(&sample_report());
        let error = build_prompt("dramatic", &grounding).expect_err("unsupported style");
        assert!(error.to_string().contains("unsupported narrative style"));
    }

    fn fixture(name: &str) -> String {
        let path = std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
            .join("fixtures")
            .join(name);
        std::fs::read_to_string(&path).unwrap_or_else(|_| panic!("read fixture {name}"))
    }

    #[test]
    fn roast_prompt_matches_fixture() {
        let input: NarrativeReport =
            serde_json::from_str(&fixture("daily_case_report_narrative_preview_input.json"))
                .expect("parse fixture input");
        let expected: serde_json::Value =
            serde_json::from_str(&fixture("daily_case_report_narrative_roast_prompt.json"))
                .expect("parse roast fixture");
        let grounding = build_grounding(&input);
        let (system, user) = build_prompt("roast", &grounding).expect("build roast prompt");

        assert_eq!(system, expected["system"].as_str().unwrap());
        assert_eq!(user, expected["user"].as_str().unwrap());
    }

    #[test]
    fn normal_prompt_matches_fixture() {
        let input: NarrativeReport =
            serde_json::from_str(&fixture("daily_case_report_narrative_preview_input.json"))
                .expect("parse fixture input");
        let expected: serde_json::Value =
            serde_json::from_str(&fixture("daily_case_report_narrative_normal_prompt.json"))
                .expect("parse normal fixture");
        let grounding = build_grounding(&input);
        let (system, user) = build_prompt("normal", &grounding).expect("build normal prompt");

        assert_eq!(system, expected["system"].as_str().unwrap());
        assert_eq!(user, expected["user"].as_str().unwrap());
    }

    #[test]
    fn image_grounding_empty_when_dir_none() {
        let images = extract_image_grounding(None);
        assert!(images.is_empty());
    }

    #[test]
    fn image_grounding_lists_supported_images() {
        let tmp = tempfile::tempdir().expect("temp dir");
        std::fs::write(tmp.path().join("a.png"), b"png").expect("write png");
        std::fs::write(tmp.path().join("b.jpg"), b"jpg").expect("write jpg");
        std::fs::write(tmp.path().join("c.txt"), b"txt").expect("write txt");

        let images = extract_image_grounding(Some(tmp.path()));
        assert_eq!(images.len(), 2);
        assert!(images[0].src.ends_with("a.png"));
        assert!(images[1].src.ends_with("b.jpg"));
    }

    #[test]
    fn image_grounding_caps_at_six() {
        let tmp = tempfile::tempdir().expect("temp dir");
        for index in 0..10 {
            std::fs::write(tmp.path().join(format!("{index}.png")), b"x").expect("write png");
        }
        let images = extract_image_grounding(Some(tmp.path()));
        assert_eq!(images.len(), MAX_IMAGES);
    }

    fn test_config(endpoint: &str) -> NarrativeConfig {
        NarrativeConfig {
            base_url: endpoint.to_string(),
            api_key: Zeroizing::new("test-key".to_string()),
            model: "gpt-4o-mini".to_string(),
            temperature: DEFAULT_TEMPERATURE,
            max_tokens: DEFAULT_MAX_TOKENS,
            http_timeout: Duration::from_secs(60),
            allow_insecure_http: true,
        }
    }

    fn json_response(body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    fn error_response(status: u16, body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 {status} Error\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    fn read_request(stream: &mut TcpStream) -> Vec<u8> {
        let mut buffer = [0_u8; 8192];
        let mut request = Vec::new();
        loop {
            let count = stream.read(&mut buffer).expect("read request");
            if count == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..count]);
            let request_str = String::from_utf8_lossy(&request);
            if request_str.contains("\r\n\r\n") {
                // Best-effort: keep reading until body arrives for small requests.
                if request_str.contains("Content-Length: 0") {
                    break;
                }
                if let Some(length) = request_str
                    .lines()
                    .find_map(|line| line.strip_prefix("Content-Length: "))
                    .and_then(|value| value.parse::<usize>().ok())
                {
                    let header_end = request_str.find("\r\n\r\n").unwrap() + 4;
                    if request.len() >= header_end + length {
                        break;
                    }
                }
            }
        }
        request
    }

    fn fake_server(response: Vec<u8>) -> (String, thread::JoinHandle<()>) {
        fake_server_with_responses(vec![response])
    }

    fn fake_server_with_responses(responses: Vec<Vec<u8>>) -> (String, thread::JoinHandle<()>) {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
        let port = listener.local_addr().expect("server address").port();
        let handle = thread::spawn(move || {
            for response in responses {
                let (mut stream, _) = listener.accept().expect("accept fake request");
                read_request(&mut stream);
                stream.write_all(&response).expect("write fake response");
            }
        });
        (format!("http://127.0.0.1:{port}"), handle)
    }

    #[test]
    fn successful_llm_call_returns_content() {
        let body = json!({
            "choices": [{"message": {"content": "# 日报标题\n\n内容"}, "finish_reason": "stop"}]
        })
        .to_string();
        let (endpoint, handle) = fake_server(json_response(&body));

        let report = sample_report();
        let config = test_config(&endpoint);
        let client = HttpClient::test_only();
        let (system, user) = build_prompt("roast", &build_grounding(&report)).unwrap();
        let result = chat_completion(&client, &config, &system, &user, 0)
            .expect("successful LLM call returns content");

        handle.join().expect("fake server joins");
        assert_eq!(result, "# 日报标题\n\n内容");
    }

    #[test]
    fn empty_choices_returns_terminal_error() {
        let body = json!({"choices": []}).to_string();
        let (endpoint, handle) = fake_server(json_response(&body));

        let report = sample_report();
        let config = test_config(&endpoint);
        let client = HttpClient::test_only();
        let (system, user) = build_prompt("roast", &build_grounding(&report)).unwrap();
        let error = chat_completion(&client, &config, &system, &user, 0)
            .expect_err("empty choices must fail");

        handle.join().expect("fake server joins");
        assert!(error.to_string().contains("empty choices"));
    }

    #[test]
    fn empty_content_retries_then_fails() {
        let body = json!({
            "choices": [{"message": {"content": ""}, "finish_reason": "length"}]
        })
        .to_string();
        let responses = vec![json_response(&body); MAX_EMPTY_CONTENT_RETRIES + 1];
        let (endpoint, handle) = fake_server_with_responses(responses);

        let report = sample_report();
        let config = test_config(&endpoint);
        let client = HttpClient::test_only();
        let (system, user) = build_prompt("roast", &build_grounding(&report)).unwrap();
        let error = chat_completion(&client, &config, &system, &user, 0)
            .expect_err("empty content must fail after retries");

        handle.join().expect("fake server joins");
        assert!(error.to_string().contains("empty content"));
    }

    #[test]
    fn non_json_response_returns_terminal_error() {
        let (endpoint, handle) = fake_server(json_response("not json"));

        let report = sample_report();
        let config = test_config(&endpoint);
        let client = HttpClient::test_only();
        let (system, user) = build_prompt("roast", &build_grounding(&report)).unwrap();
        let error =
            chat_completion(&client, &config, &system, &user, 0).expect_err("non-JSON must fail");

        handle.join().expect("fake server joins");
        assert!(error
            .to_string()
            .contains("parse provider response as JSON"));
    }

    #[test]
    fn provider_error_status_returns_terminal_error() {
        let body = json!({"error": "rate limited"}).to_string();
        let (endpoint, handle) = fake_server(error_response(429, &body));

        let report = sample_report();
        let config = test_config(&endpoint);
        let client = HttpClient::test_only();
        let (system, user) = build_prompt("roast", &build_grounding(&report)).unwrap();
        let error = chat_completion(&client, &config, &system, &user, 0)
            .expect_err("error status must fail");

        handle.join().expect("fake server joins");
        assert!(error.to_string().contains("non-success status 429"));
    }

    #[test]
    fn generate_narrative_does_not_leak_raw_input() {
        let body = json!({
            "choices": [{"message": {"content": "日报"}, "finish_reason": "stop"}]
        })
        .to_string();
        let (endpoint, handle) = fake_server(json_response(&body));

        let report = sample_report();
        let config = test_config(&endpoint);
        let client = HttpClient::test_only();
        let result = generate_narrative_with_client(&client, "roast", &report, &config, None)
            .expect("generate");

        handle.join().expect("fake server joins");
        assert!(!result.contains("今天我们先把 analyzer 跑通"));
        assert!(!result.contains("小满"));
    }

    #[test]
    fn config_env_resolution_and_requirements() {
        // Serialize with other tests that mutate process-wide env vars.
        let _guard = ENV_LOCK.lock().unwrap();

        struct CleanEnv;
        impl Drop for CleanEnv {
            fn drop(&mut self) {
                for name in LLM_BASE_URL_ENVS {
                    std::env::remove_var(name);
                }
                for name in LLM_API_KEY_ENVS {
                    std::env::remove_var(name);
                }
            }
        }
        let _clean = CleanEnv;

        // Clear relevant env vars to establish a clean baseline.
        for name in LLM_BASE_URL_ENVS {
            std::env::remove_var(name);
        }
        for name in LLM_API_KEY_ENVS {
            std::env::remove_var(name);
        }

        let error = NarrativeConfig::from_env().expect_err("missing config must fail");
        assert!(error
            .to_string()
            .contains("requires an OpenAI-compatible endpoint"));

        std::env::set_var("QINTOPIA_LLM_BASE_URL", "https://example.com/v1");
        std::env::set_var("QINTOPIA_XIAOMAN_LLM_API_KEY", "key-from-xiaoman");

        let config = NarrativeConfig::from_env().expect("config from env");
        assert_eq!(config.base_url, "https://example.com/v1");
        assert_eq!(config.api_key.as_str(), "key-from-xiaoman");
        assert_eq!(config.model, DEFAULT_MODEL);
    }

    #[test]
    fn production_base_url_requires_https() {
        NarrativeConfig::validate_production_base_url("https://example.com/v1", false)
            .expect("https allowed");
        let error = NarrativeConfig::validate_production_base_url("http://example.com/v1", false)
            .expect_err("http must fail");
        assert!(error.to_string().contains("HTTPS"));
        NarrativeConfig::validate_production_base_url("http://127.0.0.1:0", true)
            .expect("http allowed when insecure explicitly enabled");
    }
}
