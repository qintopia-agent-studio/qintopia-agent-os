use std::collections::HashMap;

use anyhow::{anyhow, bail, Context, Result};
use chrono::{DateTime, Utc};
use clap::ValueEnum;
use serde::{Deserialize, Serialize};
use serde_json::json;
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::{
    config::Cli,
    db,
    embedding_worker::{pgvector_literal, EmbeddingClient},
};

pub(crate) const TOOL_NAME: &str = "qintopia_message_store_search";
const DEFAULT_LIMIT: i64 = 20;
const MAX_LIMIT: i64 = 50;
const DEFAULT_SEMANTIC_CANDIDATE_LIMIT: i64 = 40;
const MAX_SEMANTIC_CANDIDATE_LIMIT: i64 = 100;

#[derive(Debug, Clone, Deserialize)]
pub(crate) struct SearchRequest {
    #[serde(default)]
    pub query: String,
    #[serde(default)]
    pub search_mode: SearchMode,
    #[serde(default)]
    pub chat_id: String,
    #[serde(default)]
    pub sender_id: String,
    #[serde(default)]
    pub chat_type: String,
    #[serde(default)]
    pub message_kind: String,
    #[serde(default)]
    pub since: Option<DateTime<Utc>>,
    #[serde(default)]
    pub until: Option<DateTime<Utc>>,
    #[serde(default)]
    pub limit: Option<i64>,
    #[serde(default)]
    pub caller: String,
    #[serde(default)]
    pub purpose: String,
}

#[derive(Debug, Clone, Copy, Deserialize, Serialize, PartialEq, Eq, ValueEnum)]
#[serde(rename_all = "snake_case")]
pub(crate) enum SearchMode {
    Hybrid,
    Semantic,
    Keyword,
    Recent,
}

impl Default for SearchMode {
    fn default() -> Self {
        Self::Hybrid
    }
}

#[derive(Debug, Clone)]
pub(crate) struct SearchConfig {
    pub database_url: String,
    pub db_max_connections: u32,
    pub embedding_endpoint: String,
    pub embedding_api_key: String,
    pub embedding_model: String,
    pub allowed_caller: String,
}

impl SearchConfig {
    pub(crate) fn from_cli(cli: &Cli) -> Result<Self> {
        Ok(Self {
            database_url: cli.database_url_required()?.to_string(),
            db_max_connections: cli.db_max_connections,
            embedding_endpoint: message_embedding_endpoint(cli),
            embedding_api_key: cli.embedding_api_key.clone().unwrap_or_default(),
            embedding_model: cli.message_embedding_model.clone(),
            allowed_caller: cli.message_store_mcp_allowed_caller.clone(),
        })
    }
}

pub(crate) async fn run_cli(cli: &Cli, request: SearchRequest) -> Result<()> {
    let config = SearchConfig::from_cli(cli)?;
    let pool = db::connect(&config.database_url, config.db_max_connections).await?;
    let result = search_messages(&pool, &config, request).await?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

pub(crate) async fn search_messages(
    pool: &PgPool,
    config: &SearchConfig,
    request: SearchRequest,
) -> Result<SearchResponse> {
    validate_request(config, &request)?;
    sqlx::query("SET search_path TO qintopia_messages, public")
        .execute(pool)
        .await
        .context("set message search search_path")?;

    let query = clean_text(&request.query, 500);
    let query_terms = message_store_query_terms(&query);
    let limit = request.limit.unwrap_or(DEFAULT_LIMIT).clamp(1, MAX_LIMIT);
    let filters = SearchFilters::from_request(&request);
    let mut trace = Vec::new();
    let mut rows_by_method = Vec::new();

    if matches!(
        request.search_mode,
        SearchMode::Hybrid | SearchMode::Semantic
    ) {
        let (rows, semantic_trace) = fetch_semantic_rows(
            pool,
            config,
            &query,
            &filters,
            semantic_candidate_limit(limit),
        )
        .await;
        if !rows.is_empty() {
            rows_by_method.push((RetrievalMethod::Semantic, rows));
        }
        trace.push(semantic_trace);
    }

    if matches!(
        request.search_mode,
        SearchMode::Hybrid | SearchMode::Keyword
    ) {
        let (rows, keyword_trace) = fetch_keyword_rows(pool, &query_terms, &filters, limit).await?;
        if !rows.is_empty() {
            rows_by_method.push((RetrievalMethod::Keyword, rows));
        }
        trace.push(keyword_trace);
    }

    if matches!(request.search_mode, SearchMode::Hybrid | SearchMode::Recent)
        || (matches!(request.search_mode, SearchMode::Semantic) && rows_by_method.is_empty())
    {
        let (rows, recent_trace) = fetch_recent_rows(pool, &filters, limit).await?;
        if !rows.is_empty() {
            rows_by_method.push((RetrievalMethod::Recent, rows));
        }
        trace.push(recent_trace);
    }

    let messages = merge_rows(rows_by_method, &query_terms, limit);
    Ok(SearchResponse {
        success: true,
        tool: TOOL_NAME.to_string(),
        source: "postgres_qintopia_messages".to_string(),
        read_only: true,
        query,
        query_terms,
        search_mode: request.search_mode,
        retrieval_trace: trace,
        filters,
        result_count: messages.len(),
        messages,
    })
}

fn validate_request(config: &SearchConfig, request: &SearchRequest) -> Result<()> {
    let caller = clean_text(&request.caller, 80);
    if caller != config.allowed_caller {
        bail!("{TOOL_NAME} is only available to {}", config.allowed_caller);
    }
    if clean_text(&request.purpose, 500).is_empty() {
        bail!("purpose is required");
    }
    let has_filter = [
        &request.query,
        &request.chat_id,
        &request.sender_id,
        &request.chat_type,
        &request.message_kind,
    ]
    .iter()
    .any(|value| !clean_text(value, 500).is_empty())
        || request.since.is_some()
        || request.until.is_some();
    if !has_filter {
        bail!("at least one message search filter is required");
    }
    Ok(())
}

async fn fetch_semantic_rows(
    pool: &PgPool,
    config: &SearchConfig,
    query: &str,
    filters: &SearchFilters,
    limit: i64,
) -> (Vec<SearchRow>, RetrievalTrace) {
    if query.trim().is_empty() {
        return (
            Vec::new(),
            RetrievalTrace::skipped("semantic", "semantic search requires query"),
        );
    }
    if config.embedding_api_key.trim().is_empty() || config.embedding_endpoint.trim().is_empty() {
        return (
            Vec::new(),
            RetrievalTrace::skipped(
                "semantic",
                "embedding endpoint or API key is not configured",
            ),
        );
    }

    let client = match EmbeddingClient::new(
        config.embedding_endpoint.clone(),
        config.embedding_api_key.clone(),
    ) {
        Ok(client) => client,
        Err(error) => {
            return (
                Vec::new(),
                RetrievalTrace::failed("semantic", &format!("{error:#}")),
            )
        }
    };
    let embedding = match client.embed(&config.embedding_model, query).await {
        Ok(embedding) if !embedding.is_empty() => embedding,
        Ok(_) => {
            return (
                Vec::new(),
                RetrievalTrace::failed("semantic", "embedding API returned an empty vector"),
            )
        }
        Err(error) => {
            return (
                Vec::new(),
                RetrievalTrace::failed("semantic", &format!("{error:#}")),
            )
        }
    };

    let vector = pgvector_literal(&embedding);
    let rows = sqlx::query(
        r#"
        SELECT
            m.id,
            m.platform,
            m.message_id,
            m.chat_id,
            m.chat_type,
            m.sender_id,
            m.sender_name,
            m.message_kind,
            m.text,
            m.sent_at,
            m.received_at,
            m.created_at,
            MIN(e.embedding <=> $1::qintopia_messages.vector) AS semantic_distance
        FROM qintopia_messages.message_embeddings e
        JOIN qintopia_messages.messages m ON m.id = e.message_id
        WHERE m.platform = 'qiwe'
          AND COALESCE(m.processing_hints->>'raw_archived', 'false') <> 'true'
          AND ($2::text = '' OR m.chat_id = $2)
          AND ($3::text = '' OR m.sender_id = $3)
          AND ($4::text = '' OR m.chat_type = $4)
          AND ($5::text = '' OR m.message_kind = $5)
          AND ($6::timestamptz IS NULL OR COALESCE(m.sent_at, m.received_at) >= $6)
          AND ($7::timestamptz IS NULL OR COALESCE(m.sent_at, m.received_at) <= $7)
          AND e.embedding_model = $8
          AND e.embedding_dimension = $9
        GROUP BY
            m.id,
            m.platform,
            m.message_id,
            m.chat_id,
            m.chat_type,
            m.sender_id,
            m.sender_name,
            m.message_kind,
            m.text,
            m.sent_at,
            m.received_at,
            m.created_at
        ORDER BY semantic_distance ASC, COALESCE(m.sent_at, m.received_at) DESC
        LIMIT $10
        "#,
    )
    .bind(vector)
    .bind(&filters.chat_id)
    .bind(&filters.sender_id)
    .bind(&filters.chat_type)
    .bind(&filters.message_kind)
    .bind(filters.since)
    .bind(filters.until)
    .bind(&config.embedding_model)
    .bind(i32::try_from(embedding.len()).unwrap_or(i32::MAX))
    .bind(limit)
    .fetch_all(pool)
    .await;

    match rows {
        Ok(rows) => {
            let mapped: Vec<SearchRow> = rows.into_iter().map(SearchRow::from_pg_row).collect();
            let result_count = mapped.len();
            (
                mapped,
                RetrievalTrace {
                    search_method: "semantic".to_string(),
                    success: true,
                    skipped: false,
                    result_count: Some(result_count),
                    error: String::new(),
                    detail: json!({
                        "embedding_model": config.embedding_model,
                        "embedding_dimension": embedding.len(),
                        "candidate_limit": limit
                    }),
                },
            )
        }
        Err(error) => (
            Vec::new(),
            RetrievalTrace::failed("semantic", &format!("{error:#}")),
        ),
    }
}

async fn fetch_keyword_rows(
    pool: &PgPool,
    terms: &[String],
    filters: &SearchFilters,
    limit: i64,
) -> Result<(Vec<SearchRow>, RetrievalTrace)> {
    if terms.is_empty() {
        return Ok((
            Vec::new(),
            RetrievalTrace::skipped("keyword", "keyword search requires query terms"),
        ));
    }
    let rows = sqlx::query(
        r#"
        SELECT
            m.id,
            m.platform,
            m.message_id,
            m.chat_id,
            m.chat_type,
            m.sender_id,
            m.sender_name,
            m.message_kind,
            m.text,
            m.sent_at,
            m.received_at,
            m.created_at,
            NULL::double precision AS semantic_distance
        FROM qintopia_messages.messages m
        WHERE m.platform = 'qiwe'
          AND COALESCE(m.processing_hints->>'raw_archived', 'false') <> 'true'
          AND ($1::text = '' OR m.chat_id = $1)
          AND ($2::text = '' OR m.sender_id = $2)
          AND ($3::text = '' OR m.chat_type = $3)
          AND ($4::text = '' OR m.message_kind = $4)
          AND ($5::timestamptz IS NULL OR COALESCE(m.sent_at, m.received_at) >= $5)
          AND ($6::timestamptz IS NULL OR COALESCE(m.sent_at, m.received_at) <= $6)
          AND EXISTS (
              SELECT 1
              FROM unnest($7::text[]) AS term
              WHERE m.text ILIKE '%' || term || '%'
          )
        ORDER BY COALESCE(m.sent_at, m.received_at) DESC, m.created_at DESC
        LIMIT $8
        "#,
    )
    .bind(&filters.chat_id)
    .bind(&filters.sender_id)
    .bind(&filters.chat_type)
    .bind(&filters.message_kind)
    .bind(filters.since)
    .bind(filters.until)
    .bind(terms)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("run keyword message search")?;

    let result_count = rows.len();
    Ok((
        rows.into_iter().map(SearchRow::from_pg_row).collect(),
        RetrievalTrace {
            search_method: "keyword".to_string(),
            success: true,
            skipped: false,
            result_count: Some(result_count),
            error: String::new(),
            detail: json!({ "query_terms": terms }),
        },
    ))
}

async fn fetch_recent_rows(
    pool: &PgPool,
    filters: &SearchFilters,
    limit: i64,
) -> Result<(Vec<SearchRow>, RetrievalTrace)> {
    let rows = sqlx::query(
        r#"
        SELECT
            m.id,
            m.platform,
            m.message_id,
            m.chat_id,
            m.chat_type,
            m.sender_id,
            m.sender_name,
            m.message_kind,
            m.text,
            m.sent_at,
            m.received_at,
            m.created_at,
            NULL::double precision AS semantic_distance
        FROM qintopia_messages.messages m
        WHERE m.platform = 'qiwe'
          AND COALESCE(m.processing_hints->>'raw_archived', 'false') <> 'true'
          AND ($1::text = '' OR m.chat_id = $1)
          AND ($2::text = '' OR m.sender_id = $2)
          AND ($3::text = '' OR m.chat_type = $3)
          AND ($4::text = '' OR m.message_kind = $4)
          AND ($5::timestamptz IS NULL OR COALESCE(m.sent_at, m.received_at) >= $5)
          AND ($6::timestamptz IS NULL OR COALESCE(m.sent_at, m.received_at) <= $6)
        ORDER BY COALESCE(m.sent_at, m.received_at) DESC, m.created_at DESC
        LIMIT $7
        "#,
    )
    .bind(&filters.chat_id)
    .bind(&filters.sender_id)
    .bind(&filters.chat_type)
    .bind(&filters.message_kind)
    .bind(filters.since)
    .bind(filters.until)
    .bind(limit)
    .fetch_all(pool)
    .await
    .context("run recent message search")?;

    let result_count = rows.len();
    Ok((
        rows.into_iter().map(SearchRow::from_pg_row).collect(),
        RetrievalTrace {
            search_method: "recent".to_string(),
            success: true,
            skipped: false,
            result_count: Some(result_count),
            error: String::new(),
            detail: json!({}),
        },
    ))
}

fn merge_rows(
    rows_by_method: Vec<(RetrievalMethod, Vec<SearchRow>)>,
    query_terms: &[String],
    limit: i64,
) -> Vec<MessageSearchResult> {
    let mut merged: HashMap<Uuid, MessageSearchResult> = HashMap::new();
    for (method, rows) in rows_by_method {
        for (rank, row) in rows.into_iter().enumerate() {
            let item = merged
                .entry(row.id)
                .or_insert_with(|| MessageSearchResult::from_row(&row));
            if !item.retrieval_methods.contains(&method) {
                item.retrieval_methods.push(method);
            }
            let mut score = method.base_score() - rank as f64;
            if method == RetrievalMethod::Semantic {
                if let Some(distance) = row.semantic_distance {
                    item.semantic_distance = Some(distance);
                    score += (1.0 - distance).max(0.0) * 100.0;
                }
            }
            if method == RetrievalMethod::Keyword {
                let text = row.text.to_lowercase();
                let matched = query_terms
                    .iter()
                    .filter(|term| text.contains(&term.to_lowercase()))
                    .cloned()
                    .collect::<Vec<_>>();
                item.matched_terms = matched;
                score += item.matched_terms.len() as f64 * 10.0;
            }
            item.retrieval_score = item.retrieval_score.max(score);
        }
    }

    let mut messages = merged.into_values().collect::<Vec<_>>();
    messages.sort_by(|a, b| {
        b.retrieval_score
            .partial_cmp(&a.retrieval_score)
            .unwrap_or(std::cmp::Ordering::Equal)
            .then_with(|| b.sort_timestamp.cmp(&a.sort_timestamp))
    });
    messages.truncate(limit as usize);
    messages
}

fn semantic_candidate_limit(limit: i64) -> i64 {
    (limit * 3)
        .max(DEFAULT_SEMANTIC_CANDIDATE_LIMIT)
        .min(MAX_SEMANTIC_CANDIDATE_LIMIT)
}

fn message_embedding_endpoint(cli: &Cli) -> String {
    cli.message_embedding_endpoint
        .as_deref()
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(ToOwned::to_owned)
        .unwrap_or_else(|| {
            format!(
                "{}/v1/embeddings",
                cli.embedding_base_url.trim_end_matches('/')
            )
        })
}

fn message_store_query_terms(query: &str) -> Vec<String> {
    let mut terms = query
        .split(|ch: char| ch.is_whitespace() || ch.is_ascii_punctuation())
        .map(|part| clean_text(part, 80))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let compact = clean_text(query, 120).replace(char::is_whitespace, "");
    if !compact.is_empty() && !terms.iter().any(|term| term == &compact) {
        terms.push(compact);
    }
    terms.truncate(8);
    terms
}

fn clean_text<T: ToString>(value: T, max_len: usize) -> String {
    value
        .to_string()
        .trim()
        .chars()
        .take(max_len)
        .collect::<String>()
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SearchResponse {
    pub success: bool,
    pub tool: String,
    pub source: String,
    pub read_only: bool,
    pub query: String,
    pub query_terms: Vec<String>,
    pub search_mode: SearchMode,
    pub retrieval_trace: Vec<RetrievalTrace>,
    pub filters: SearchFilters,
    pub result_count: usize,
    pub messages: Vec<MessageSearchResult>,
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct RetrievalTrace {
    pub search_method: String,
    pub success: bool,
    pub skipped: bool,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub result_count: Option<usize>,
    #[serde(skip_serializing_if = "String::is_empty")]
    pub error: String,
    pub detail: serde_json::Value,
}

impl RetrievalTrace {
    fn skipped(method: &str, error: &str) -> Self {
        Self {
            search_method: method.to_string(),
            success: false,
            skipped: true,
            result_count: None,
            error: error.to_string(),
            detail: json!({}),
        }
    }

    fn failed(method: &str, error: &str) -> Self {
        Self {
            search_method: method.to_string(),
            success: false,
            skipped: false,
            result_count: None,
            error: clean_text(error, 500),
            detail: json!({}),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct SearchFilters {
    pub chat_id: String,
    pub sender_id: String,
    pub chat_type: String,
    pub message_kind: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub since: Option<DateTime<Utc>>,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub until: Option<DateTime<Utc>>,
}

impl SearchFilters {
    fn from_request(request: &SearchRequest) -> Self {
        Self {
            chat_id: clean_text(&request.chat_id, 200),
            sender_id: clean_text(&request.sender_id, 200),
            chat_type: match clean_text(&request.chat_type, 40).as_str() {
                "group" => "group".to_string(),
                "direct" => "direct".to_string(),
                _ => String::new(),
            },
            message_kind: clean_text(&request.message_kind, 80),
            since: request.since,
            until: request.until,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum RetrievalMethod {
    Semantic,
    Keyword,
    Recent,
}

impl RetrievalMethod {
    fn base_score(&self) -> f64 {
        match self {
            Self::Semantic => 1000.0,
            Self::Keyword => 500.0,
            Self::Recent => 0.0,
        }
    }
}

#[derive(Debug, Clone)]
struct SearchRow {
    id: Uuid,
    platform: String,
    message_id: String,
    chat_id: String,
    chat_type: String,
    sender_id: String,
    sender_name: Option<String>,
    message_kind: String,
    text: String,
    sent_at: Option<DateTime<Utc>>,
    received_at: DateTime<Utc>,
    created_at: DateTime<Utc>,
    semantic_distance: Option<f64>,
}

impl SearchRow {
    fn from_pg_row(row: sqlx::postgres::PgRow) -> Self {
        Self {
            id: row.get("id"),
            platform: row.get("platform"),
            message_id: row.get("message_id"),
            chat_id: row.get("chat_id"),
            chat_type: row.get("chat_type"),
            sender_id: row.get("sender_id"),
            sender_name: row.get("sender_name"),
            message_kind: row.get("message_kind"),
            text: row.get::<Option<String>, _>("text").unwrap_or_default(),
            sent_at: row.get("sent_at"),
            received_at: row.get("received_at"),
            created_at: row.get("created_at"),
            semantic_distance: row.get("semantic_distance"),
        }
    }
}

#[derive(Debug, Clone, Serialize)]
pub(crate) struct MessageSearchResult {
    pub id: Uuid,
    pub message_id: String,
    pub platform: String,
    pub chat_id: String,
    pub chat_type: String,
    pub sender_id: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sender_name: Option<String>,
    pub message_kind: String,
    pub text_preview: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub sent_at: Option<DateTime<Utc>>,
    pub received_at: DateTime<Utc>,
    pub retrieval_methods: Vec<RetrievalMethod>,
    pub retrieval_score: f64,
    #[serde(skip_serializing_if = "Option::is_none")]
    pub semantic_distance: Option<f64>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    pub matched_terms: Vec<String>,
    #[serde(skip)]
    sort_timestamp: DateTime<Utc>,
}

impl MessageSearchResult {
    fn from_row(row: &SearchRow) -> Self {
        Self {
            id: row.id,
            message_id: row.message_id.clone(),
            platform: row.platform.clone(),
            chat_id: row.chat_id.clone(),
            chat_type: row.chat_type.clone(),
            sender_id: row.sender_id.clone(),
            sender_name: row.sender_name.clone(),
            message_kind: row.message_kind.clone(),
            text_preview: clean_text(row.text.replace(char::is_whitespace, " "), 240),
            sent_at: row.sent_at,
            received_at: row.received_at,
            retrieval_methods: Vec::new(),
            retrieval_score: 0.0,
            semantic_distance: None,
            matched_terms: Vec::new(),
            sort_timestamp: row.sent_at.unwrap_or(row.received_at).max(row.created_at),
        }
    }
}

pub(crate) fn request_from_json(arguments: serde_json::Value) -> Result<SearchRequest> {
    serde_json::from_value(arguments).map_err(|error| anyhow!("invalid tool arguments: {error}"))
}

pub(crate) fn tool_input_schema() -> serde_json::Value {
    json!({
        "type": "object",
        "properties": {
            "query": {
                "type": "string",
                "description": "Natural language query or keywords. Required for semantic and keyword search."
            },
            "search_mode": {
                "type": "string",
                "enum": ["hybrid", "semantic", "keyword", "recent"],
                "description": "Retrieval mode. Defaults to hybrid."
            },
            "chat_id": {
                "type": "string",
                "description": "Optional QiWe chat/group id filter."
            },
            "sender_id": {
                "type": "string",
                "description": "Optional QiWe sender id filter."
            },
            "chat_type": {
                "type": "string",
                "enum": ["group", "direct"],
                "description": "Optional chat type filter."
            },
            "message_kind": {
                "type": "string",
                "description": "Optional message kind filter, for example text."
            },
            "since": {
                "type": "string",
                "description": "Optional lower timestamp bound. RFC3339 is preferred."
            },
            "until": {
                "type": "string",
                "description": "Optional upper timestamp bound. RFC3339 is preferred."
            },
            "limit": {
                "type": "integer",
                "minimum": 1,
                "maximum": MAX_LIMIT,
                "description": "Maximum messages to return. Defaults to 20."
            },
            "caller": {
                "type": "string",
                "description": "Calling profile id. v1 expects wenyuange."
            },
            "purpose": {
                "type": "string",
                "description": "Why this message search is needed. Required."
            }
        },
        "additionalProperties": false
    })
}

#[cfg(test)]
mod tests {
    use super::{message_store_query_terms, SearchMode, SearchRequest};

    #[test]
    fn search_mode_defaults_to_hybrid() {
        let request: SearchRequest = serde_json::from_value(serde_json::json!({
            "caller": "wenyuange",
            "purpose": "test",
            "query": "wifi 密码"
        }))
        .unwrap();
        assert_eq!(request.search_mode, SearchMode::Hybrid);
    }

    #[test]
    fn query_terms_include_compact_chinese_phrase() {
        let terms = message_store_query_terms("wifi 密码是什么");
        assert!(terms.iter().any(|term| term == "wifi"));
        assert!(terms.iter().any(|term| term == "密码是什么"));
        assert!(terms.iter().any(|term| term == "wifi密码是什么"));
    }

    #[test]
    fn default_search_sql_excludes_archived_raw_messages() {
        let source = include_str!("message_search.rs");
        assert!(source.matches("raw_archived").count() >= 3);
    }
}
