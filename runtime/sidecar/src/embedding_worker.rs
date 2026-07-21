use std::{
    io::{Read, Write},
    net::TcpStream,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, Stream};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{config::Cli, db};

const JOB_TYPE: &str = "embedding_pending";
const STALE_PROCESSING_SECONDS: i64 = 900;

pub async fn run(cli: Cli, check_only: bool) -> Result<()> {
    let config = WorkerConfig::from_cli(&cli, !check_only)?;
    let pool = db::connect(&config.database_url, cli.db_max_connections).await?;
    check_schema(&pool).await?;

    if check_only {
        let counts = read_counts(&pool).await?;
        println!(
            "{}",
            serde_json::to_string_pretty(&json!({
                "ok": true,
                "mode": "message_embedding_worker_check",
                "base_url": config.base_url,
                "endpoint": config.endpoint,
                "model": config.model,
                "batch_size": config.batch_size,
                "poll_seconds": config.poll_seconds,
                "request_delay_ms": config.request_delay_ms,
                "max_attempts": config.max_attempts,
                "api_key_configured": !config.api_key.trim().is_empty(),
                "api_key_placeholder": api_key_is_placeholder(&config.api_key),
                "pending_embedding_jobs": counts.pending_embedding_jobs,
                "message_embeddings": counts.message_embeddings
            }))?
        );
        return Ok(());
    }

    if config.api_key.trim().is_empty() || api_key_is_placeholder(&config.api_key) {
        bail!("QINTOPIA_EMBEDDING_API_KEY must not be empty");
    }

    let client = EmbeddingClient::new(config.endpoint.clone(), config.api_key.clone())?;
    info!(
        model = %config.model,
        batch_size = config.batch_size,
        poll_seconds = config.poll_seconds,
        "message embedding worker started"
    );

    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("shutdown signal received");
                break;
            }
            result = run_once(&pool, &client, &config) => {
                let processed = match result {
                    Ok(processed) => processed,
                    Err(error) => {
                        error!(error = %error, "embedding worker loop failed");
                        0
                    }
                };
                if processed == 0 {
                    tokio::time::sleep(Duration::from_secs(config.poll_seconds)).await;
                }
            }
        }
    }

    Ok(())
}

async fn run_once(pool: &PgPool, client: &EmbeddingClient, config: &WorkerConfig) -> Result<usize> {
    let jobs = claim_jobs(pool, config.batch_size).await?;
    if jobs.is_empty() {
        return Ok(0);
    }

    let mut processed = 0;
    for job in jobs {
        if let Err(error) = process_job(pool, client, config, job.clone()).await {
            warn!(
                job_id = %job.job_id,
                message_id = %job.message_id,
                error = %error,
                "message embedding job failed"
            );
            mark_failed(pool, &job, &format!("{error:?}"), config.max_attempts).await?;
        }
        processed += 1;
        if config.request_delay_ms > 0 {
            tokio::time::sleep(Duration::from_millis(config.request_delay_ms)).await;
        }
    }

    Ok(processed)
}

async fn check_schema(pool: &PgPool) -> Result<()> {
    let db_check = db::check(pool).await?;
    if !db_check.schema_exists || !db_check.messages_table_exists {
        bail!("qintopia_messages schema is not migrated; run migrate first");
    }

    let jobs_table_exists: (bool,) = sqlx::query_as(
        "select to_regclass('qintopia_messages.message_processing_jobs') is not null",
    )
    .fetch_one(pool)
    .await
    .context("check message_processing_jobs table")?;
    let embeddings_table_exists: (bool,) =
        sqlx::query_as("select to_regclass('qintopia_messages.message_embeddings') is not null")
            .fetch_one(pool)
            .await
            .context("check message_embeddings table")?;

    if !jobs_table_exists.0 || !embeddings_table_exists.0 {
        bail!("message embedding worker tables are not migrated");
    }
    Ok(())
}

async fn read_counts(pool: &PgPool) -> Result<QueueCounts> {
    let pending_embedding_jobs: (i64,) = sqlx::query_as(
        r#"
        SELECT count(*)::bigint
        FROM qintopia_messages.message_processing_jobs
        WHERE job_type = 'embedding_pending'
          AND status = 'pending'
          AND available_at <= now()
        "#,
    )
    .fetch_one(pool)
    .await
    .context("read pending embedding job count")?;

    let message_embeddings: (i64,) =
        sqlx::query_as("SELECT count(*)::bigint FROM qintopia_messages.message_embeddings")
            .fetch_one(pool)
            .await
            .context("read message embedding count")?;

    Ok(QueueCounts {
        pending_embedding_jobs: pending_embedding_jobs.0,
        message_embeddings: message_embeddings.0,
    })
}

async fn claim_jobs(pool: &PgPool, batch_size: i64) -> Result<Vec<ClaimedJob>> {
    let limit = batch_size.max(1);
    let rows = sqlx::query_as::<_, (Uuid, Uuid, i32)>(
        r#"
        WITH claimed AS (
            SELECT id
            FROM qintopia_messages.message_processing_jobs
            WHERE job_type = $1
              AND (
                  (status = 'pending' AND available_at <= now())
                  OR (
                      status = 'processing'
                      AND locked_at <= now() - ($3::text || ' seconds')::interval
                  )
              )
            ORDER BY available_at ASC, created_at ASC
            LIMIT $2
            FOR UPDATE SKIP LOCKED
        )
        UPDATE qintopia_messages.message_processing_jobs jobs
        SET
            status = 'processing',
            locked_at = now(),
            updated_at = now()
        FROM claimed
        WHERE jobs.id = claimed.id
        RETURNING jobs.id, jobs.message_id, jobs.attempts
        "#,
    )
    .bind(JOB_TYPE)
    .bind(limit)
    .bind(STALE_PROCESSING_SECONDS)
    .fetch_all(pool)
    .await
    .context("claim embedding jobs")?;

    Ok(rows
        .into_iter()
        .map(|(job_id, message_id, attempts)| ClaimedJob {
            job_id,
            message_id,
            attempts,
        })
        .collect())
}

async fn process_job(
    pool: &PgPool,
    client: &EmbeddingClient,
    config: &WorkerConfig,
    job: ClaimedJob,
) -> Result<()> {
    let text = load_message_text(pool, job.message_id).await?;
    let normalized = text.trim();
    if normalized.is_empty() {
        complete_empty_text(pool, &job).await?;
        info!(
            job_id = %job.job_id,
            message_id = %job.message_id,
            "completed embedding job with empty message text"
        );
        return Ok(());
    }

    let content_hash = content_hash(normalized);
    let embedding = client.embed(&config.model, normalized).await?;
    let dimension = i32::try_from(embedding.len()).context("embedding dimension exceeds i32")?;
    if dimension <= 0 {
        bail!("embedding API returned an empty embedding vector");
    }

    write_embedding(pool, &job, &config.model, &content_hash, &embedding).await?;
    info!(
        job_id = %job.job_id,
        message_id = %job.message_id,
        model = %config.model,
        dimension = dimension,
        "message embedding job completed"
    );
    Ok(())
}

async fn load_message_text(pool: &PgPool, message_id: Uuid) -> Result<String> {
    let row = sqlx::query_as::<_, (Option<String>,)>(
        "SELECT text FROM qintopia_messages.messages WHERE id = $1",
    )
    .bind(message_id)
    .fetch_optional(pool)
    .await
    .context("load message text")?;

    row.map(|(text,)| text.unwrap_or_default())
        .ok_or_else(|| anyhow!("message {message_id} not found"))
}

async fn write_embedding(
    pool: &PgPool,
    job: &ClaimedJob,
    model: &str,
    content_hash: &str,
    embedding: &[f32],
) -> Result<()> {
    let dimension = i32::try_from(embedding.len()).context("embedding dimension exceeds i32")?;
    let embedding_literal = pgvector_literal(embedding);
    let metadata = json!({
        "worker": "qintopia-message-embedding-worker",
        "provider": "openai-compatible",
        "job_type": JOB_TYPE,
    });

    let mut tx = pool.begin().await.context("begin embedding transaction")?;
    sqlx::query(
        r#"
        INSERT INTO qintopia_messages.message_embeddings
            (
                message_id,
                embedding_model,
                embedding_dimension,
                embedding,
                content_hash,
                metadata,
                source_job_id
            )
        VALUES ($1, $2, $3, $4::qintopia_messages.vector, $5, $6, $7)
        ON CONFLICT (message_id, embedding_model, content_hash) DO UPDATE SET
            embedding_dimension = EXCLUDED.embedding_dimension,
            embedding = EXCLUDED.embedding,
            metadata = qintopia_messages.message_embeddings.metadata || EXCLUDED.metadata,
            source_job_id = EXCLUDED.source_job_id
        "#,
    )
    .bind(job.message_id)
    .bind(model)
    .bind(dimension)
    .bind(&embedding_literal)
    .bind(content_hash)
    .bind(metadata)
    .bind(job.job_id)
    .execute(&mut *tx)
    .await
    .context("upsert message embedding")?;

    complete_job_tx(&mut tx, job.job_id)
        .await
        .context("complete embedding job")?;
    tx.commit().await.context("commit embedding transaction")?;
    Ok(())
}

async fn complete_empty_text(pool: &PgPool, job: &ClaimedJob) -> Result<()> {
    let mut tx = pool.begin().await.context("begin empty-text transaction")?;
    sqlx::query(
        r#"
        UPDATE qintopia_messages.message_processing_jobs
        SET
            status = 'completed',
            completed_at = now(),
            locked_at = NULL,
            error = 'skipped: empty message text',
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(job.job_id)
    .execute(&mut *tx)
    .await
    .context("complete empty-text embedding job")?;
    tx.commit().await.context("commit empty-text transaction")?;
    Ok(())
}

async fn complete_job_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    job_id: Uuid,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE qintopia_messages.message_processing_jobs
        SET
            status = 'completed',
            completed_at = now(),
            locked_at = NULL,
            error = NULL,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(job_id)
    .execute(&mut **tx)
    .await
    .context("mark embedding job completed")?;
    Ok(())
}

async fn mark_failed(
    pool: &PgPool,
    job: &ClaimedJob,
    error: &str,
    max_attempts: i32,
) -> Result<()> {
    let next_attempts = job.attempts.saturating_add(1);
    let terminal = next_attempts >= max_attempts.max(1);
    let status = if terminal { "failed" } else { "pending" };
    let delay_seconds = if terminal {
        0
    } else {
        backoff_seconds(next_attempts)
    };
    let trimmed_error = trim_error(error);

    sqlx::query(
        r#"
        UPDATE qintopia_messages.message_processing_jobs
        SET
            status = $2,
            attempts = $3,
            available_at = now() + ($4::text || ' seconds')::interval,
            locked_at = NULL,
            error = $5,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(job.job_id)
    .bind(status)
    .bind(next_attempts)
    .bind(delay_seconds)
    .bind(trimmed_error)
    .execute(pool)
    .await
    .context("mark embedding job failed")?;
    Ok(())
}

fn content_hash(text: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(text.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
}

pub(crate) fn pgvector_literal(values: &[f32]) -> String {
    let body = values
        .iter()
        .map(|value| {
            if value.is_finite() {
                value.to_string()
            } else {
                "0".to_string()
            }
        })
        .collect::<Vec<_>>()
        .join(",");
    format!("[{body}]")
}

fn backoff_seconds(attempts: i32) -> i64 {
    let exponent = attempts.saturating_sub(1).min(6) as u32;
    60_i64.saturating_mul(2_i64.saturating_pow(exponent))
}

fn trim_error(error: &str) -> String {
    const MAX_ERROR_LEN: usize = 2000;
    if error.len() <= MAX_ERROR_LEN {
        return error.to_string();
    }
    error.chars().take(MAX_ERROR_LEN).collect()
}

#[derive(Debug, Clone)]
struct WorkerConfig {
    database_url: String,
    base_url: String,
    endpoint: String,
    api_key: String,
    model: String,
    batch_size: i64,
    poll_seconds: u64,
    request_delay_ms: u64,
    max_attempts: i32,
}

impl WorkerConfig {
    fn from_cli(cli: &Cli, require_api_key: bool) -> Result<Self> {
        let database_url = cli.database_url_required()?.to_string();
        if require_api_key {
            let _api_key = cli.embedding_api_key_required()?;
        }
        let batch_size = cli.message_embedding_batch_size.max(1);
        let poll_seconds = cli.message_embedding_poll_seconds.max(1);
        let max_attempts = cli.message_embedding_max_attempts.max(1);

        Ok(Self {
            database_url,
            base_url: cli.embedding_base_url.trim_end_matches('/').to_string(),
            endpoint: message_embedding_endpoint(cli),
            api_key: cli.embedding_api_key.clone().unwrap_or_default(),
            model: cli.message_embedding_model.clone(),
            batch_size,
            poll_seconds,
            request_delay_ms: cli.message_embedding_request_delay_ms,
            max_attempts,
        })
    }
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

#[derive(Debug, Clone)]
struct QueueCounts {
    pending_embedding_jobs: i64,
    message_embeddings: i64,
}

fn api_key_is_placeholder(api_key: &str) -> bool {
    matches!(
        api_key.trim(),
        "" | "replace-with-server-secret" | "change-me" | "placeholder"
    )
}

#[derive(Debug, Clone)]
struct ClaimedJob {
    job_id: Uuid,
    message_id: Uuid,
    attempts: i32,
}

#[derive(Debug, Clone)]
pub(crate) struct EmbeddingClient {
    endpoint: String,
    api_key: String,
    tls_config: Arc<ClientConfig>,
}

impl EmbeddingClient {
    pub(crate) fn new(endpoint: String, api_key: String) -> Result<Self> {
        if endpoint.trim().is_empty() {
            bail!("message embedding endpoint must not be empty");
        }
        let tls_config = Arc::new(tls_config());
        Ok(Self {
            endpoint: endpoint.trim().to_string(),
            api_key,
            tls_config,
        })
    }

    pub(crate) async fn embed(&self, model: &str, text: &str) -> Result<Vec<f32>> {
        let endpoint = self.endpoint.clone();
        let api_key = self.api_key.clone();
        let tls_config = Arc::clone(&self.tls_config);
        let request = EmbeddingRequest {
            model: model.to_string(),
            input: text.to_string(),
        };

        let parsed = tokio::task::spawn_blocking(move || {
            let body = post_embedding_request(&endpoint, &api_key, &request, tls_config)?;
            serde_json::from_str::<EmbeddingResponse>(&body).context("parse embedding response")
        })
        .await
        .context("join embedding API task")??;
        let first = parsed
            .data
            .into_iter()
            .next()
            .ok_or_else(|| anyhow!("embedding API returned no data"))?;
        Ok(first.embedding)
    }
}

fn tls_config() -> ClientConfig {
    let root_store = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());

    ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth()
}

fn post_embedding_request(
    endpoint: &str,
    api_key: &str,
    request: &EmbeddingRequest,
    tls_config: Arc<ClientConfig>,
) -> Result<String> {
    let url = url::Url::parse(endpoint).context("parse embedding endpoint URL")?;
    if url.scheme() != "https" {
        bail!("QINTOPIA_EMBEDDING_BASE_URL must use https");
    }
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("embedding endpoint URL is missing host"))?;
    let port = url.port_or_known_default().unwrap_or(443);
    let mut path = url.path().to_string();
    if path.is_empty() {
        path = "/".to_string();
    }
    if let Some(query) = url.query() {
        path.push('?');
        path.push_str(query);
    }

    let body = serde_json::to_string(request).context("serialize embedding request")?;
    let http_request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Authorization: Bearer {api_key}\r\n\
         Content-Type: application/json\r\n\
         Accept: application/json\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    );

    let server_name =
        ServerName::try_from(host.to_string()).context("validate embedding endpoint host")?;
    let mut connection =
        ClientConnection::new(tls_config, server_name).context("create TLS connection")?;
    let mut socket = TcpStream::connect((host, port)).context("connect embedding endpoint")?;
    socket
        .set_read_timeout(Some(Duration::from_secs(60)))
        .context("set embedding read timeout")?;
    socket
        .set_write_timeout(Some(Duration::from_secs(60)))
        .context("set embedding write timeout")?;

    let mut stream = Stream::new(&mut connection, &mut socket);
    stream
        .write_all(http_request.as_bytes())
        .context("write embedding request")?;
    stream.flush().context("flush embedding request")?;

    let mut response = Vec::new();
    stream
        .read_to_end(&mut response)
        .context("read embedding response")?;
    parse_http_response(&response)
}

fn parse_http_response(response: &[u8]) -> Result<String> {
    let response_text =
        String::from_utf8(response.to_vec()).context("embedding response was not UTF-8")?;
    let (headers, body) = response_text
        .split_once("\r\n\r\n")
        .ok_or_else(|| anyhow!("embedding response missing header terminator"))?;
    let status_line = headers
        .lines()
        .next()
        .ok_or_else(|| anyhow!("embedding response missing status line"))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow!("embedding response missing status code"))?;

    if status != "200" {
        bail!("embedding API returned HTTP {status}: {}", trim_error(body));
    }

    if headers
        .lines()
        .any(|line| line.eq_ignore_ascii_case("transfer-encoding: chunked"))
    {
        return decode_chunked_body(body.as_bytes());
    }

    Ok(body.to_string())
}

fn decode_chunked_body(body: &[u8]) -> Result<String> {
    let mut cursor = 0;
    let mut decoded = Vec::new();
    loop {
        let line_end = find_crlf(body, cursor)
            .ok_or_else(|| anyhow!("chunked embedding response missing chunk size"))?;
        let size_text =
            std::str::from_utf8(&body[cursor..line_end]).context("chunk size was not UTF-8")?;
        let size_hex = size_text.split(';').next().unwrap_or(size_text).trim();
        let size =
            usize::from_str_radix(size_hex, 16).context("parse chunked response chunk size")?;
        cursor = line_end + 2;
        if size == 0 {
            break;
        }
        let chunk_end = cursor
            .checked_add(size)
            .ok_or_else(|| anyhow!("chunked embedding response overflow"))?;
        if chunk_end + 2 > body.len() {
            bail!("chunked embedding response ended early");
        }
        decoded.extend_from_slice(&body[cursor..chunk_end]);
        cursor = chunk_end + 2;
    }

    String::from_utf8(decoded).context("decoded chunked embedding response was not UTF-8")
}

fn find_crlf(bytes: &[u8], start: usize) -> Option<usize> {
    bytes
        .get(start..)?
        .windows(2)
        .position(|window| window == b"\r\n")
        .map(|offset| start + offset)
}

#[derive(Debug, Serialize)]
struct EmbeddingRequest {
    model: String,
    input: String,
}

#[derive(Debug, Deserialize)]
struct EmbeddingResponse {
    data: Vec<EmbeddingData>,
    #[allow(dead_code)]
    model: Option<String>,
    #[allow(dead_code)]
    usage: Option<Value>,
}

#[derive(Debug, Deserialize)]
struct EmbeddingData {
    embedding: Vec<f32>,
}

#[cfg(test)]
mod tests {
    use super::{
        api_key_is_placeholder, backoff_seconds, content_hash, decode_chunked_body,
        parse_http_response, pgvector_literal, trim_error,
    };

    #[test]
    fn content_hash_is_stable_sha256() {
        assert_eq!(
            content_hash("hello"),
            "sha256:2cf24dba5fb0a30e26e83b2ac5b9e29e1b161e5c1fa7425e73043362938b9824"
        );
    }

    #[test]
    fn vector_literal_replaces_non_finite_values() {
        assert_eq!(
            pgvector_literal(&[0.25, -1.5, f32::NAN, f32::INFINITY]),
            "[0.25,-1.5,0,0]"
        );
    }

    #[test]
    fn backoff_is_exponential_and_capped() {
        assert_eq!(backoff_seconds(1), 60);
        assert_eq!(backoff_seconds(2), 120);
        assert_eq!(backoff_seconds(7), 3840);
        assert_eq!(backoff_seconds(100), 3840);
    }

    #[test]
    fn trim_error_limits_long_text() {
        let long = "x".repeat(2500);
        assert_eq!(trim_error(&long).len(), 2000);
    }

    #[test]
    fn detects_empty_and_placeholder_api_keys() {
        assert!(api_key_is_placeholder(""));
        assert!(api_key_is_placeholder("replace-with-server-secret"));
        assert!(api_key_is_placeholder(" change-me "));
        assert!(!api_key_is_placeholder("sk-live-real"));
    }

    #[test]
    fn parses_successful_http_response() {
        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 11\r\n\r\nhello world";
        assert_eq!(parse_http_response(response).unwrap(), "hello world");
    }

    #[test]
    fn decodes_chunked_http_response_body() {
        assert_eq!(
            decode_chunked_body(b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n").unwrap(),
            "hello world"
        );
    }
}
