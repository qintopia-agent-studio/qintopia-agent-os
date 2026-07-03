use std::{
    collections::HashSet,
    fs::File,
    io::{BufRead, BufReader},
    path::{Path, PathBuf},
};

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::{config::Cli, db};

const MAX_LIMIT: i64 = 10;

#[derive(Debug, Clone)]
pub(crate) struct KnowledgeSearchRequest {
    pub query: String,
    pub limit: i64,
    pub include_internal: bool,
    pub include_member_scoped: bool,
    pub required_terms: Vec<String>,
}

#[derive(Debug, Clone)]
pub(crate) struct KnowledgeSearchResponse {
    pub results: Vec<KnowledgeResult>,
    pub trace: Value,
}

#[derive(Debug, Clone)]
pub(crate) struct KnowledgeResult {
    pub chunk_id: Uuid,
    pub document_id: Uuid,
    pub source_id: Uuid,
    pub title: String,
    pub canonical_url: String,
    pub source_type: String,
    pub source_key: String,
    pub document_type: String,
    pub content: String,
    pub information_class: String,
    pub visibility: String,
    pub source_locator: Value,
    pub source_updated_at: Option<chrono::DateTime<chrono::Utc>>,
    pub rank_score: i32,
}

pub(crate) async fn search_knowledge(
    pool: &PgPool,
    request: KnowledgeSearchRequest,
) -> Result<KnowledgeSearchResponse> {
    let query = clean_text(&request.query, 500);
    if query.is_empty() {
        bail!("query is required");
    }
    let terms = knowledge_query_terms(&query);
    let required_terms = request
        .required_terms
        .iter()
        .map(|term| clean_text(term, 80))
        .filter(|term| !term.is_empty())
        .collect::<Vec<_>>();
    if terms.is_empty() {
        return Ok(KnowledgeSearchResponse {
            results: Vec::new(),
            trace: json!({
                "search_method": "knowledge_keyword",
                "success": false,
                "skipped": true,
                "error": "no searchable query terms"
            }),
        });
    }
    let allowed_classes =
        allowed_information_classes(request.include_internal, request.include_member_scoped);
    let limit = request.limit.clamp(1, MAX_LIMIT);
    let rows = sqlx::query(
        r#"
        SELECT
            c.id AS chunk_id,
            d.id AS document_id,
            s.id AS source_id,
            d.title,
            COALESCE(d.canonical_url, '') AS canonical_url,
            s.source_type,
            s.source_key,
            d.document_type,
            c.content,
            c.information_class,
            c.visibility,
            c.source_locator,
            d.source_updated_at,
            (
                CASE
                    WHEN d.title ILIKE '%' || $1 || '%' THEN 100
                    ELSE 0
                END
                +
                (
                    SELECT COALESCE(SUM(
                        CASE
                            WHEN c.content ILIKE '%' || term || '%' THEN 10
                            ELSE 0
                        END
                    ), 0)::integer
                    FROM unnest($2::text[]) AS term
                )
            ) AS rank_score
        FROM qintopia_knowledge.knowledge_chunks c
        JOIN qintopia_knowledge.knowledge_documents d ON d.id = c.document_id
        JOIN qintopia_knowledge.knowledge_sources s ON s.id = d.source_id
        WHERE c.information_class = ANY($3::text[])
          AND d.status = 'active'
          AND s.status = 'active'
          AND (
              d.title ILIKE '%' || $1 || '%'
              OR EXISTS (
                  SELECT 1
                  FROM unnest($2::text[]) AS term
                  WHERE c.content ILIKE '%' || term || '%'
              )
          )
          AND NOT EXISTS (
              SELECT 1
              FROM unnest($5::text[]) AS required_term
              WHERE d.title NOT ILIKE '%' || required_term || '%'
                AND c.content NOT ILIKE '%' || required_term || '%'
          )
        ORDER BY rank_score DESC, d.source_updated_at DESC NULLS LAST, d.title ASC, c.chunk_index ASC
        LIMIT $4
        "#,
    )
    .bind(&query)
    .bind(&terms)
    .bind(&allowed_classes)
    .bind(limit)
    .bind(&required_terms)
    .fetch_all(pool)
    .await
    .context("search qintopia knowledge chunks")?;

    let results = rows
        .into_iter()
        .map(|row| KnowledgeResult {
            chunk_id: row.get("chunk_id"),
            document_id: row.get("document_id"),
            source_id: row.get("source_id"),
            title: row.get("title"),
            canonical_url: row.get("canonical_url"),
            source_type: row.get("source_type"),
            source_key: row.get("source_key"),
            document_type: row.get("document_type"),
            content: row.get("content"),
            information_class: row.get("information_class"),
            visibility: row.get("visibility"),
            source_locator: row.get("source_locator"),
            source_updated_at: row.get("source_updated_at"),
            rank_score: row.get("rank_score"),
        })
        .collect::<Vec<_>>();

    Ok(KnowledgeSearchResponse {
        trace: json!({
            "search_method": "knowledge_keyword",
            "success": true,
            "skipped": false,
            "result_count": results.len(),
            "detail": {
                "query_terms": terms,
                "required_terms": required_terms,
                "allowed_information_classes": allowed_classes
            }
        }),
        results,
    })
}

pub(crate) async fn run_import_snapshot(
    cli: &Cli,
    public_jsonl: Option<String>,
    internal_jsonl: Option<String>,
    member_scoped_jsonl: Option<String>,
    source_key: String,
    source_title: String,
) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let source_id = upsert_snapshot_source(&pool, &source_key, &source_title).await?;
    let mut imported_documents = 0usize;
    let mut imported_chunks = 0usize;
    for (path, default_class) in [
        (public_jsonl, "Public"),
        (internal_jsonl, "Internal"),
        (member_scoped_jsonl, "Member-scoped"),
    ] {
        let Some(path) = path else {
            continue;
        };
        let path = PathBuf::from(path);
        let stats = import_snapshot_file(&pool, source_id, &path, default_class).await?;
        imported_documents += stats.documents;
        imported_chunks += stats.chunks;
    }
    println!(
        "imported knowledge snapshot: documents={imported_documents} chunks={imported_chunks}"
    );
    Ok(())
}

async fn upsert_snapshot_source(pool: &PgPool, source_key: &str, title: &str) -> Result<Uuid> {
    let source_key = clean_text(source_key, 200);
    if source_key.is_empty() {
        bail!("source_key is required");
    }
    let title = clean_text(title, 300);
    let row = sqlx::query(
        r#"
        INSERT INTO qintopia_knowledge.knowledge_sources
            (source_type, source_key, title, information_class, visibility, sync_status, metadata, last_synced_at)
        VALUES
            ('markdown_snapshot', $1, $2, 'Public', 'public', 'completed', '{"importer":"qintopia-message-sidecar"}'::jsonb, now())
        ON CONFLICT (source_type, source_key) DO UPDATE SET
            title = EXCLUDED.title,
            sync_status = 'completed',
            last_synced_at = now(),
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(source_key)
    .bind(title)
    .fetch_one(pool)
    .await
    .context("upsert knowledge snapshot source")?;
    Ok(row.get("id"))
}

async fn import_snapshot_file(
    pool: &PgPool,
    source_id: Uuid,
    path: &Path,
    default_class: &str,
) -> Result<ImportStats> {
    let file = File::open(path).with_context(|| format!("open {}", path.display()))?;
    let reader = BufReader::new(file);
    let mut documents = 0usize;
    let mut chunks = 0usize;
    for (line_number, line) in reader.lines().enumerate() {
        let line =
            line.with_context(|| format!("read {} line {}", path.display(), line_number + 1))?;
        if line.trim().is_empty() {
            continue;
        }
        let snapshot: SnapshotDocument = serde_json::from_str(&line)
            .with_context(|| format!("parse {} line {}", path.display(), line_number + 1))?;
        let information_class = clean_text(
            snapshot
                .information_class
                .as_deref()
                .unwrap_or(default_class),
            80,
        );
        let visibility = visibility_for_class(&information_class);
        let content_hash = hex_sha256(snapshot.body.as_bytes());
        let document_id = upsert_snapshot_document(
            pool,
            source_id,
            &snapshot,
            &information_class,
            visibility,
            &content_hash,
        )
        .await?;
        replace_document_chunks(
            pool,
            document_id,
            &snapshot,
            &information_class,
            visibility,
            &content_hash,
        )
        .await?;
        documents += 1;
        chunks += 1;
    }
    Ok(ImportStats { documents, chunks })
}

async fn upsert_snapshot_document(
    pool: &PgPool,
    source_id: Uuid,
    snapshot: &SnapshotDocument,
    information_class: &str,
    visibility: &str,
    content_hash: &str,
) -> Result<Uuid> {
    let external_document_id = clean_text(
        snapshot
            .source_id
            .as_deref()
            .filter(|value| !value.trim().is_empty())
            .unwrap_or(&snapshot.path),
        300,
    );
    let source_updated_at = snapshot
        .updated_at
        .as_deref()
        .and_then(|value| chrono::DateTime::parse_from_rfc3339(value).ok())
        .map(|value| value.with_timezone(&chrono::Utc));
    let row = sqlx::query(
        r#"
        INSERT INTO qintopia_knowledge.knowledge_documents
            (
                source_id,
                external_document_id,
                title,
                title_path,
                document_type,
                version_key,
                canonical_url,
                content_hash,
                information_class,
                visibility,
                status,
                metadata,
                source_updated_at,
                indexed_at
            )
        VALUES
            (
                $1, $2, $3, ARRAY[$3]::text[], 'markdown_snapshot', $4, $5, $6,
                $7, $8, 'active', $9::jsonb, $10, now()
            )
        ON CONFLICT (source_id, external_document_id) DO UPDATE SET
            title = EXCLUDED.title,
            title_path = EXCLUDED.title_path,
            version_key = EXCLUDED.version_key,
            canonical_url = EXCLUDED.canonical_url,
            content_hash = EXCLUDED.content_hash,
            information_class = EXCLUDED.information_class,
            visibility = EXCLUDED.visibility,
            status = 'active',
            metadata = EXCLUDED.metadata,
            source_updated_at = EXCLUDED.source_updated_at,
            indexed_at = now(),
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(source_id)
    .bind(external_document_id)
    .bind(clean_text(&snapshot.title, 300))
    .bind(snapshot.checksum.as_deref().unwrap_or(content_hash))
    .bind(clean_text(&snapshot.path, 600))
    .bind(content_hash)
    .bind(information_class)
    .bind(visibility)
    .bind(json!({
        "path": snapshot.path,
        "external_allowed": snapshot.external_allowed,
        "snapshot_source_id": snapshot.source_id,
    }))
    .bind(source_updated_at)
    .fetch_one(pool)
    .await
    .context("upsert knowledge snapshot document")?;
    Ok(row.get("id"))
}

async fn replace_document_chunks(
    pool: &PgPool,
    document_id: Uuid,
    snapshot: &SnapshotDocument,
    information_class: &str,
    visibility: &str,
    content_hash: &str,
) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("begin replace knowledge chunks")?;
    sqlx::query("DELETE FROM qintopia_knowledge.knowledge_chunks WHERE document_id = $1")
        .bind(document_id)
        .execute(&mut *tx)
        .await
        .context("delete existing knowledge chunks")?;
    sqlx::query(
        r#"
        INSERT INTO qintopia_knowledge.knowledge_chunks
            (
                document_id,
                chunk_index,
                chunk_key,
                chunk_kind,
                heading_path,
                content,
                content_hash,
                information_class,
                visibility,
                allowed_audiences,
                source_locator,
                metadata
            )
        VALUES
            ($1, 0, $2, 'document', ARRAY[]::text[], $3, $4, $5, $6, $7::text[], $8::jsonb, $9::jsonb)
        "#,
    )
    .bind(document_id)
    .bind(clean_text(&snapshot.path, 600))
    .bind(clean_text(&snapshot.body, 200_000))
    .bind(content_hash)
    .bind(information_class)
    .bind(visibility)
    .bind(allowed_audiences(information_class))
    .bind(json!({
        "path": snapshot.path,
        "title": snapshot.title,
        "source_kind": "snapshot_jsonl"
    }))
    .bind(json!({ "external_allowed": snapshot.external_allowed }))
    .execute(&mut *tx)
    .await
    .context("insert knowledge chunk")?;
    tx.commit().await.context("commit replace knowledge chunks")
}

fn allowed_information_classes(include_internal: bool, include_member_scoped: bool) -> Vec<String> {
    let mut classes = vec!["Public".to_string()];
    if include_internal {
        classes.push("Internal".to_string());
    }
    if include_member_scoped {
        classes.push("Member-scoped".to_string());
    }
    classes
}

fn allowed_audiences(information_class: &str) -> Vec<String> {
    match information_class {
        "Public" => vec![
            "erhua".to_string(),
            "wenyuange".to_string(),
            "internal_agent".to_string(),
            "public".to_string(),
        ],
        "Internal" => vec!["wenyuange".to_string(), "internal_agent".to_string()],
        "Member-scoped" => vec!["wenyuange".to_string()],
        _ => Vec::new(),
    }
}

fn visibility_for_class(information_class: &str) -> &'static str {
    match information_class {
        "Public" => "public",
        "Member-scoped" => "member_scoped",
        _ => "internal",
    }
}

fn knowledge_query_terms(query: &str) -> Vec<String> {
    let mut terms = query
        .split(|ch: char| ch.is_whitespace() || ch.is_ascii_punctuation())
        .map(|part| clean_text(part, 80))
        .filter(|part| !part.is_empty())
        .collect::<Vec<_>>();
    let compact = clean_text(query, 120).replace(char::is_whitespace, "");
    if !compact.is_empty() && !terms.iter().any(|term| term == &compact) {
        terms.push(compact);
    }
    for keyword in [
        "wifi",
        "wi-fi",
        "无线",
        "密码",
        "电话",
        "位置",
        "订餐",
        "赵姐",
        "山泡茶",
        "山泡",
        "泡茶",
        "外卖",
        "无人机",
    ] {
        if query.to_lowercase().contains(&keyword.to_lowercase())
            && !terms.iter().any(|term| term == keyword)
        {
            terms.push(keyword.to_string());
        }
    }
    let mut seen = HashSet::new();
    terms.retain(|term| seen.insert(term.to_lowercase()));
    terms.truncate(12);
    terms
}

fn hex_sha256(bytes: &[u8]) -> String {
    let digest = Sha256::digest(bytes);
    digest.iter().map(|byte| format!("{byte:02x}")).collect()
}

fn clean_text(value: &str, max_len: usize) -> String {
    value.trim().chars().take(max_len).collect()
}

#[derive(Debug, Deserialize)]
struct SnapshotDocument {
    #[serde(default)]
    source_id: Option<String>,
    title: String,
    path: String,
    #[serde(default)]
    information_class: Option<String>,
    #[serde(default)]
    external_allowed: bool,
    #[serde(default)]
    updated_at: Option<String>,
    #[serde(default)]
    checksum: Option<String>,
    #[serde(default)]
    body: String,
}

#[derive(Debug, Clone, Copy)]
struct ImportStats {
    documents: usize,
    chunks: usize,
}

#[cfg(test)]
mod tests {
    use super::{allowed_information_classes, knowledge_query_terms, visibility_for_class};

    #[test]
    fn knowledge_terms_include_compact_and_domain_terms() {
        let terms = knowledge_query_terms("WiFi 密码是什么");
        assert!(terms.iter().any(|term| term == "WiFi"));
        assert!(terms.iter().any(|term| term == "WiFi密码是什么"));
        assert!(terms.iter().any(|term| term.to_lowercase() == "wifi"));
        assert!(terms.iter().any(|term| term == "密码"));
    }

    #[test]
    fn allowed_classes_default_to_public() {
        assert_eq!(allowed_information_classes(false, false), vec!["Public"]);
        assert_eq!(
            allowed_information_classes(true, false),
            vec!["Public", "Internal"]
        );
    }

    #[test]
    fn visibility_matches_information_class() {
        assert_eq!(visibility_for_class("Public"), "public");
        assert_eq!(visibility_for_class("Internal"), "internal");
        assert_eq!(visibility_for_class("Member-scoped"), "member_scoped");
    }
}
