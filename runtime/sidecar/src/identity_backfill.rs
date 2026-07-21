use std::{
    collections::{HashMap, HashSet},
    io::{Read, Write},
    net::TcpStream,
    sync::{Arc, Mutex},
    time::{Duration, Instant},
};

use anyhow::{anyhow, bail, Context, Result};
use chrono::Utc;
use rustls::{pki_types::ServerName, ClientConfig, ClientConnection, RootCertStore, Stream};
use serde::Serialize;
use serde_json::{json, Value};
use sqlx::postgres::PgPool;
use tracing::{info, warn};

use crate::{config::Cli, db, event::SenderIdentityEvent};

#[derive(Debug, Clone)]
pub struct BackfillOptions {
    pub apply: bool,
    pub dry_run: bool,
    pub refresh: bool,
    pub limit: Option<i64>,
    pub chat_id: Option<String>,
    pub sender_id: Option<String>,
    pub request_delay_ms: u64,
}

#[derive(Debug, Clone)]
pub struct IdentityWorkerOptions {
    pub check_only: bool,
    pub batch_size: i64,
    pub poll_seconds: u64,
    pub chat_id: Option<String>,
    pub member_map_ttl_seconds: u64,
}

pub async fn run(cli: &Cli, options: BackfillOptions) -> Result<()> {
    if options.apply && options.dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let apply = options.apply && !options.dry_run;
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let qiwe = QiWeBackfillClient::from_cli(cli)?;
    let resolver = IdentityResolver::new(qiwe, cli.identity_member_map_ttl_seconds);
    let report = run_batch(&pool, &resolver, &options, apply).await?;

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_worker(cli: &Cli, options: IdentityWorkerOptions) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let qiwe = QiWeBackfillClient::from_cli(cli)?;
    let resolver = IdentityResolver::new(qiwe, options.member_map_ttl_seconds);
    let batch_size = options.batch_size.max(1);
    let poll_seconds = options.poll_seconds.max(5);
    let backfill_options = BackfillOptions {
        apply: true,
        dry_run: options.check_only,
        refresh: false,
        limit: Some(batch_size),
        chat_id: options.chat_id.clone(),
        sender_id: None,
        request_delay_ms: 0,
    };

    let report = run_batch(&pool, &resolver, &backfill_options, !options.check_only).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    if options.check_only {
        return Ok(());
    }

    info!(
        batch_size,
        poll_seconds,
        chat_id = ?options.chat_id,
        "starting identity worker"
    );
    loop {
        let report = run_batch(&pool, &resolver, &backfill_options, true).await?;
        info!(
            total_identity_keys = report.total_identity_keys,
            resolved = report.resolved,
            unresolved = report.unresolved,
            messages_updated = report.messages_updated,
            "identity worker batch complete"
        );
        tokio::time::sleep(Duration::from_secs(poll_seconds)).await;
    }
}

async fn run_batch(
    pool: &PgPool,
    resolver: &IdentityResolver,
    options: &BackfillOptions,
    apply: bool,
) -> Result<BackfillReport> {
    let keys = load_identity_keys(pool, options).await?;
    let mut report = BackfillReport {
        total_identity_keys: keys.len(),
        source: "current_qiwe_identity".to_string(),
        dry_run: !apply,
        ..BackfillReport::default()
    };

    let mut pending_by_chat: HashMap<String, Vec<IdentityKey>> = HashMap::new();
    for key in keys {
        if !options.refresh {
            if let Some(existing) = lookup_existing_identity(pool, &key).await? {
                report.resolved += 1;
                if apply {
                    let applied = apply_identity(pool, &key, &existing).await?;
                    report.messages_updated += applied.messages_updated;
                    report.platform_identities_materialized +=
                        applied.platform_identities_materialized;
                }
                continue;
            }
        }

        pending_by_chat
            .entry(key.chat_id.clone())
            .or_default()
            .push(key);
    }

    let mut first_chat = true;
    for (chat_id, chat_keys) in pending_by_chat {
        if options.request_delay_ms > 0 {
            if !first_chat {
                tokio::time::sleep(Duration::from_millis(options.request_delay_ms)).await;
            }
            first_chat = false;
        }

        let sender_ids = chat_keys
            .iter()
            .map(|key| key.sender_id.clone())
            .collect::<Vec<_>>();
        match resolver.resolve_chat(&chat_id, &sender_ids, options.refresh) {
            Ok(resolved_by_sender) => {
                for key in chat_keys {
                    match resolved_by_sender.get(&key.sender_id) {
                        Some(resolved) => {
                            report.resolved += 1;
                            if apply {
                                let applied = apply_identity(pool, &key, resolved).await?;
                                report.messages_updated += applied.messages_updated;
                                report.platform_identities_materialized +=
                                    applied.platform_identities_materialized;
                            }
                        }
                        None => {
                            report.unresolved += 1;
                            if apply {
                                mark_unresolved(pool, &key, "not_found").await?;
                            }
                            report.unresolved_keys.push(key);
                        }
                    }
                }
            }
            Err(error) => {
                let error_chain = error_chain(&error);
                for key in chat_keys {
                    warn!(
                        chat_id = %key.chat_id,
                        sender_id = %key.sender_id,
                        error = %error_chain,
                        "identity backfill resolve failed"
                    );
                    report.unresolved += 1;
                    if apply {
                        mark_unresolved(pool, &key, "resolve_error").await?;
                    }
                    report.unresolved_keys.push(key);
                }
            }
        }
    }

    Ok(report)
}

async fn load_identity_keys(pool: &PgPool, options: &BackfillOptions) -> Result<Vec<IdentityKey>> {
    let limit = options.limit.unwrap_or(500).max(1);
    let rows = sqlx::query_as::<_, (String, String, i64)>(
        r#"
        SELECT m.chat_id, m.sender_id, count(*)::bigint AS message_count
        FROM qintopia_messages.messages m
        WHERE m.platform = 'qiwe'
          AND m.sender_id <> ''
          AND ($1::text IS NULL OR m.chat_id = $1)
          AND ($4::text IS NULL OR m.sender_id = $4)
          AND (
            $2::boolean
            OR m.sender_channel_identity_id IS NULL
            OR COALESCE(m.sender_name, '') = ''
          )
          AND (
            $2::boolean
            OR COALESCE((m.processing_hints->>'identity_unresolved_until')::timestamptz, '-infinity'::timestamptz) <= now()
          )
        GROUP BY m.chat_id, m.sender_id
        ORDER BY count(*) DESC, m.chat_id, m.sender_id
        LIMIT $3
        "#,
    )
    .bind(options.chat_id.as_deref())
    .bind(options.refresh)
    .bind(limit)
    .bind(options.sender_id.as_deref())
    .fetch_all(pool)
    .await
    .context("load identity backfill keys")?;

    Ok(rows
        .into_iter()
        .map(|(chat_id, sender_id, message_count)| IdentityKey {
            chat_id,
            sender_id,
            message_count,
        })
        .collect())
}

async fn mark_unresolved(pool: &PgPool, key: &IdentityKey, reason: &str) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE qintopia_messages.messages
        SET processing_hints = processing_hints
            || jsonb_build_object(
                'identity_unresolved_at', now(),
                'identity_unresolved_until', now() + interval '1 day',
                'identity_unresolved_reason', $1::text
            ),
            updated_at = now()
        WHERE platform = 'qiwe'
          AND chat_id = $2
          AND sender_id = $3
          AND (
            sender_channel_identity_id IS NULL
            OR COALESCE(sender_name, '') = ''
          )
        "#,
    )
    .bind(reason)
    .bind(&key.chat_id)
    .bind(&key.sender_id)
    .execute(pool)
    .await
    .context("mark unresolved identity")?;
    Ok(())
}

async fn apply_identity(
    pool: &PgPool,
    key: &IdentityKey,
    resolved: &ResolvedIdentity,
) -> Result<AppliedIdentity> {
    let mut tx = pool
        .begin()
        .await
        .context("begin identity backfill transaction")?;
    let identity = SenderIdentityEvent {
        platform: "qiwe".to_string(),
        chat_id: key.chat_id.clone(),
        channel_user_id: key.sender_id.clone(),
        display_name: resolved.display_name.clone(),
        identity_source: resolved.source.clone(),
        resolved_at: Some(Utc::now()),
        metadata: json!({
            "backfill": true,
            "backfill_source": "current_qiwe_identity",
        }),
    };
    let identity_id = db::upsert_channel_identity(
        &mut tx,
        &identity,
        "qiwe",
        &key.chat_id,
        &key.sender_id,
        None,
        None,
    )
    .await?;
    let update_filter = if resolved.refresh_existing_messages {
        r#"
          AND (
            sender_channel_identity_id IS NULL
            OR COALESCE(sender_name, '') = ''
            OR sender_person_id IS NULL
            OR sender_name IS DISTINCT FROM $1
          )
        "#
    } else {
        r#"
          AND (
            sender_channel_identity_id IS NULL
            OR COALESCE(sender_name, '') = ''
            OR sender_person_id IS NULL
          )
        "#
    };
    let result = sqlx::query(&format!(
        r#"
        UPDATE qintopia_messages.messages
        SET sender_name = $1,
            sender_channel_identity_id = $2,
            sender_person_id = COALESCE(sender_person_id, (
                SELECT person_id
                FROM qintopia_identity.channel_identities
                WHERE id = $2
            )),
            processing_hints = processing_hints
                || $5::jsonb,
            updated_at = now()
        WHERE platform = 'qiwe'
          AND chat_id = $3
          AND sender_id = $4
          {update_filter}
        "#
    ))
    .bind(&resolved.display_name)
    .bind(identity_id)
    .bind(&key.chat_id)
    .bind(&key.sender_id)
    .bind(json!({
        "sender_name_backfilled": true,
        "sender_name_backfill_source": "current_qiwe_identity"
    }))
    .execute(&mut *tx)
    .await
    .context("update backfilled messages")?;
    let platform_identities_materialized =
        materialize_platform_identity(&mut tx, &key.sender_id).await?;
    tx.commit()
        .await
        .context("commit identity backfill transaction")?;
    Ok(AppliedIdentity {
        messages_updated: i64::try_from(result.rows_affected())
            .context("rows_affected overflow")?,
        platform_identities_materialized,
    })
}

async fn materialize_platform_identity(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    sender_id: &str,
) -> Result<i64> {
    let row = sqlx::query_as::<_, (i64,)>(
        r#"
        WITH person_candidates AS (
            SELECT ci.person_id
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND ci.channel_user_id = $1
              AND ci.person_id IS NOT NULL
            GROUP BY ci.person_id
        ),
        unique_person AS (
            SELECT person_id
            FROM person_candidates
            WHERE (SELECT count(*) FROM person_candidates) = 1
        ),
        source_identity AS (
            SELECT ci.*
            FROM qintopia_identity.channel_identities ci
            JOIN unique_person up ON up.person_id = ci.person_id
            WHERE ci.platform = 'qiwe'
              AND ci.channel_user_id = $1
              AND COALESCE(ci.display_name, '') <> ''
            ORDER BY
              CASE WHEN ci.chat_id = '' THEN 0 ELSE 1 END,
              qintopia_identity.identity_source_rank(ci.identity_source) DESC,
              ci.updated_at DESC
            LIMIT 1
        ),
        upserted AS (
            INSERT INTO qintopia_identity.channel_identities
                (
                    person_id,
                    platform,
                    channel_user_id,
                    chat_id,
                    display_name,
                    normalized_display_name,
                    identity_source,
                    confidence,
                    first_seen_at,
                    last_seen_at,
                    metadata
                )
            SELECT
                source_identity.person_id,
                'qiwe',
                $1,
                '',
                source_identity.display_name,
                source_identity.normalized_display_name,
                source_identity.identity_source,
                source_identity.confidence,
                source_identity.first_seen_at,
                source_identity.last_seen_at,
                source_identity.metadata
                    || jsonb_build_object(
                        'identity_scope', 'qiwe_platform_user',
                        'materialized_from_channel_identity_id', source_identity.id::text,
                        'materialized_at', now()
                    )
            FROM source_identity
            ON CONFLICT (platform, channel_user_id, chat_id) DO UPDATE SET
                person_id = EXCLUDED.person_id,
                display_name = CASE
                    WHEN qintopia_identity.channel_identities.person_id IS NULL
                      OR qintopia_identity.identity_source_rank(EXCLUDED.identity_source) >= qintopia_identity.identity_source_rank(qintopia_identity.channel_identities.identity_source)
                    THEN EXCLUDED.display_name
                    ELSE qintopia_identity.channel_identities.display_name
                END,
                normalized_display_name = CASE
                    WHEN qintopia_identity.channel_identities.person_id IS NULL
                      OR qintopia_identity.identity_source_rank(EXCLUDED.identity_source) >= qintopia_identity.identity_source_rank(qintopia_identity.channel_identities.identity_source)
                    THEN EXCLUDED.normalized_display_name
                    ELSE qintopia_identity.channel_identities.normalized_display_name
                END,
                identity_source = CASE
                    WHEN qintopia_identity.channel_identities.person_id IS NULL
                      OR qintopia_identity.identity_source_rank(EXCLUDED.identity_source) >= qintopia_identity.identity_source_rank(qintopia_identity.channel_identities.identity_source)
                    THEN EXCLUDED.identity_source
                    ELSE qintopia_identity.channel_identities.identity_source
                END,
                confidence = GREATEST(qintopia_identity.channel_identities.confidence, EXCLUDED.confidence),
                last_seen_at = GREATEST(qintopia_identity.channel_identities.last_seen_at, EXCLUDED.last_seen_at),
                metadata = qintopia_identity.channel_identities.metadata || EXCLUDED.metadata,
                updated_at = now()
            WHERE qintopia_identity.channel_identities.person_id IS NULL
               OR qintopia_identity.channel_identities.person_id = EXCLUDED.person_id
            RETURNING id, person_id
        ),
        linked_channel_identities AS (
            UPDATE qintopia_identity.channel_identities ci
            SET person_id = upserted.person_id,
                updated_at = now()
            FROM upserted
            WHERE ci.platform = 'qiwe'
              AND ci.channel_user_id = $1
              AND ci.chat_id <> ''
              AND ci.person_id IS NULL
            RETURNING ci.id
        ),
        linked_messages AS (
            UPDATE qintopia_messages.messages m
            SET sender_person_id = upserted.person_id,
                processing_hints = m.processing_hints
                    || jsonb_build_object('sender_person_materialized_from_qiwe_userid', true),
                updated_at = now()
            FROM upserted
            WHERE m.platform = 'qiwe'
              AND m.sender_id = $1
              AND m.sender_person_id IS NULL
            RETURNING m.id
        )
        SELECT count(*)::bigint FROM upserted
        "#,
    )
    .bind(sender_id)
    .fetch_one(&mut **tx)
    .await
    .context("materialize QiWe platform user identity")?;
    Ok(row.0)
}

async fn lookup_existing_identity(
    pool: &PgPool,
    key: &IdentityKey,
) -> Result<Option<ResolvedIdentity>> {
    let row = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT display_name, identity_source
        FROM qintopia_identity.channel_identities
        WHERE platform = 'qiwe'
          AND chat_id = $1
          AND channel_user_id = $2
          AND COALESCE(display_name, '') <> ''
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(&key.chat_id)
    .bind(&key.sender_id)
    .fetch_optional(pool)
    .await
    .context("lookup existing channel identity")?;

    Ok(row.map(|(display_name, source)| ResolvedIdentity {
        display_name,
        source,
        refresh_existing_messages: false,
    }))
}

struct QiWeBackfillClient {
    api_url: String,
    token: String,
    guid: String,
    tls_config: Arc<ClientConfig>,
}

struct IdentityResolver {
    qiwe: QiWeBackfillClient,
    member_map_ttl: Duration,
    member_maps: Mutex<HashMap<String, CachedMemberMap>>,
}

#[derive(Debug, Clone)]
struct CachedMemberMap {
    loaded_at: Instant,
    identities: HashMap<String, ResolvedIdentity>,
}

impl IdentityResolver {
    fn new(qiwe: QiWeBackfillClient, ttl_seconds: u64) -> Self {
        Self {
            qiwe,
            member_map_ttl: Duration::from_secs(ttl_seconds.max(1)),
            member_maps: Mutex::new(HashMap::new()),
        }
    }

    fn resolve_chat(
        &self,
        chat_id: &str,
        sender_ids: &[String],
        refresh: bool,
    ) -> Result<HashMap<String, ResolvedIdentity>> {
        let wanted = sender_ids.iter().cloned().collect::<HashSet<_>>();
        let member_map = self.member_map(chat_id, refresh)?;
        let mut resolved = HashMap::new();
        for sender_id in &wanted {
            if let Some(identity) = member_map.get(sender_id) {
                resolved.insert(sender_id.clone(), identity.clone());
            }
        }
        let unresolved = wanted
            .iter()
            .filter(|sender_id| !resolved.contains_key(*sender_id))
            .cloned()
            .collect::<HashSet<_>>();
        if unresolved.is_empty() {
            return Ok(resolved);
        }
        resolved.extend(self.qiwe.lookup_contacts(&unresolved)?);
        Ok(resolved)
    }

    fn member_map(
        &self,
        chat_id: &str,
        refresh: bool,
    ) -> Result<HashMap<String, ResolvedIdentity>> {
        let now = Instant::now();
        let mut guard = self
            .member_maps
            .lock()
            .map_err(|_| anyhow!("member map cache lock poisoned"))?;
        if !refresh {
            if let Some(cached) = guard.get(chat_id) {
                if now.duration_since(cached.loaded_at) <= self.member_map_ttl {
                    return Ok(cached.identities.clone());
                }
            }
        }
        let identities = self.qiwe.lookup_room_member_map(chat_id)?;
        guard.insert(
            chat_id.to_string(),
            CachedMemberMap {
                loaded_at: now,
                identities: identities.clone(),
            },
        );
        Ok(identities)
    }
}

impl QiWeBackfillClient {
    fn from_cli(cli: &Cli) -> Result<Self> {
        let token = cli
            .qiwe_token
            .as_deref()
            .ok_or_else(|| anyhow!("QIWE_TOKEN or --qiwe-token is required for identity backfill"))?
            .to_string();
        Ok(Self {
            api_url: cli.qiwe_api_url.clone(),
            token,
            guid: cli.qiwe_guid.clone().unwrap_or_default(),
            tls_config: Arc::new(tls_config()),
        })
    }

    fn lookup_room_member_map(&self, chat_id: &str) -> Result<HashMap<String, ResolvedIdentity>> {
        let response =
            self.call_qiwe_api("/room/batchGetRoomDetail", json!({"roomIdList": [chat_id]}))?;
        Ok(parse_room_member_map(&response))
    }

    fn lookup_contacts(
        &self,
        wanted: &HashSet<String>,
    ) -> Result<HashMap<String, ResolvedIdentity>> {
        if wanted.is_empty() {
            return Ok(HashMap::new());
        }
        let user_id_list = wanted.iter().cloned().collect::<Vec<_>>();
        let response = self.call_qiwe_api(
            "/contact/batchGetUserinfo",
            json!({"userIdList": user_id_list}),
        )?;
        Ok(parse_contact_identities(&response, wanted))
    }

    fn call_qiwe_api(&self, method: &str, params: Value) -> Result<Value> {
        let mut params = params.as_object().cloned().unwrap_or_default();
        if !self.guid.is_empty() && !params.contains_key("guid") {
            params.insert("guid".to_string(), Value::String(self.guid.clone()));
        }
        let request = json!({
            "method": method,
            "params": Value::Object(params),
        });
        let body = post_json(
            &self.api_url,
            &self.token,
            &request,
            self.tls_config.clone(),
        )
        .with_context(|| format!("call QiWe API {method}"))?;
        let parsed: Value = serde_json::from_str(&body).context("parse QiWe API response")?;
        if let Some(code) = parsed.get("code").and_then(value_code) {
            if code != 0 && code != 200 {
                bail!("QiWe business error: {parsed}");
            }
        }
        Ok(parsed)
    }
}

fn post_json(
    endpoint: &str,
    token: &str,
    request: &Value,
    tls_config: Arc<ClientConfig>,
) -> Result<String> {
    let url = url::Url::parse(endpoint).context("parse QiWe API URL")?;
    let host = url
        .host_str()
        .ok_or_else(|| anyhow!("QiWe API URL is missing host"))?;
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
    let body = serde_json::to_string(request).context("serialize QiWe request")?;
    let http_request = format!(
        "POST {path} HTTP/1.1\r\n\
         Host: {host}\r\n\
         Content-Type: application/json\r\n\
         Accept: application/json\r\n\
         x-qiwei-token: {token}\r\n\
         Content-Length: {}\r\n\
         Connection: close\r\n\
         \r\n\
         {body}",
        body.len()
    );

    if url.scheme() == "https" {
        let server_name =
            ServerName::try_from(host.to_string()).context("validate QiWe API host")?;
        let mut connection =
            ClientConnection::new(tls_config, server_name).context("create TLS connection")?;
        let mut socket = TcpStream::connect((host, port)).context("connect QiWe API")?;
        socket
            .set_read_timeout(Some(Duration::from_secs(30)))
            .context("set QiWe read timeout")?;
        socket
            .set_write_timeout(Some(Duration::from_secs(30)))
            .context("set QiWe write timeout")?;
        let mut stream = Stream::new(&mut connection, &mut socket);
        stream
            .write_all(http_request.as_bytes())
            .context("write QiWe request")?;
        stream.flush().context("flush QiWe request")?;
        let mut response = Vec::new();
        stream
            .read_to_end(&mut response)
            .context("read QiWe response")?;
        return parse_http_response(&response);
    }

    if url.scheme() != "http" {
        bail!("QiWe API URL must use http or https");
    }
    let mut socket = TcpStream::connect((host, port)).context("connect QiWe API")?;
    socket
        .set_read_timeout(Some(Duration::from_secs(30)))
        .context("set QiWe read timeout")?;
    socket
        .set_write_timeout(Some(Duration::from_secs(30)))
        .context("set QiWe write timeout")?;
    socket
        .write_all(http_request.as_bytes())
        .context("write QiWe request")?;
    socket.flush().context("flush QiWe request")?;
    let mut response = Vec::new();
    socket
        .read_to_end(&mut response)
        .context("read QiWe response")?;
    parse_http_response(&response)
}

fn parse_http_response(response: &[u8]) -> Result<String> {
    let header_end = find_header_end(response)
        .ok_or_else(|| anyhow!("QiWe response missing header terminator"))?;
    let headers = String::from_utf8_lossy(&response[..header_end]);
    let body = &response[header_end + 4..];
    let status_line = headers
        .lines()
        .next()
        .ok_or_else(|| anyhow!("QiWe response missing status line"))?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .ok_or_else(|| anyhow!("QiWe response missing status code"))?;
    if status != "200" {
        let body_text = String::from_utf8_lossy(body);
        bail!("QiWe API returned HTTP {status}: {body_text}");
    }
    if header_has_token(&headers, "transfer-encoding", "chunked") {
        return decode_chunked_body(body);
    }
    String::from_utf8(body.to_vec()).context("QiWe response body was not UTF-8")
}

fn decode_chunked_body(body: &[u8]) -> Result<String> {
    let mut cursor = 0;
    let mut decoded = Vec::new();
    loop {
        let line_end = find_crlf(body, cursor)
            .ok_or_else(|| anyhow!("chunked QiWe response missing chunk size"))?;
        let size_text =
            std::str::from_utf8(&body[cursor..line_end]).context("chunk size was not UTF-8")?;
        let size_hex = size_text.split(';').next().unwrap_or(size_text).trim();
        let size = usize::from_str_radix(size_hex, 16).context("parse chunk size")?;
        cursor = line_end + 2;
        if size == 0 {
            break;
        }
        let chunk_end = cursor
            .checked_add(size)
            .ok_or_else(|| anyhow!("chunk overflow"))?;
        if chunk_end + 2 > body.len() {
            bail!("chunked QiWe response ended early");
        }
        decoded.extend_from_slice(&body[cursor..chunk_end]);
        cursor = chunk_end + 2;
    }
    String::from_utf8(decoded).context("decoded chunked response was not UTF-8")
}

fn find_header_end(bytes: &[u8]) -> Option<usize> {
    bytes.windows(4).position(|window| window == b"\r\n\r\n")
}

fn find_crlf(bytes: &[u8], start: usize) -> Option<usize> {
    bytes
        .get(start..)?
        .windows(2)
        .position(|window| window == b"\r\n")
        .map(|offset| start + offset)
}

fn header_has_token(headers: &str, name: &str, token: &str) -> bool {
    headers.lines().any(|line| {
        let Some((header_name, value)) = line.split_once(':') else {
            return false;
        };
        header_name.trim().eq_ignore_ascii_case(name)
            && value
                .split(',')
                .any(|part| part.trim().eq_ignore_ascii_case(token))
    })
}

fn tls_config() -> ClientConfig {
    let root_store = RootCertStore::from_iter(webpki_roots::TLS_SERVER_ROOTS.iter().cloned());
    ClientConfig::builder()
        .with_root_certificates(root_store)
        .with_no_client_auth()
}

fn first_mapping(value: &Value) -> Option<&serde_json::Map<String, Value>> {
    if let Some(object) = value.as_object() {
        return Some(object);
    }
    value.as_array()?.first()?.as_object()
}

fn parse_room_member_map(response: &Value) -> HashMap<String, ResolvedIdentity> {
    let mut resolved = HashMap::new();
    let room_list = response
        .get("data")
        .and_then(first_mapping)
        .and_then(|data| data.get("roomList"))
        .and_then(Value::as_array);
    let Some(room_list) = room_list else {
        return resolved;
    };
    for room in room_list {
        let Some(members) = room.get("memberList").and_then(Value::as_array) else {
            continue;
        };
        for member in members {
            let Some(user_id) = value_text(member.get("userId")) else {
                continue;
            };
            if resolved.contains_key(&user_id) {
                continue;
            }
            if let Some(name) =
                value_text(member.get("name")).or_else(|| value_text(member.get("roomRemarkName")))
            {
                resolved.insert(
                    user_id,
                    ResolvedIdentity {
                        display_name: name,
                        source: "room_member".to_string(),
                        refresh_existing_messages: true,
                    },
                );
            }
        }
    }
    resolved
}

fn parse_contact_identities(
    response: &Value,
    wanted: &HashSet<String>,
) -> HashMap<String, ResolvedIdentity> {
    let mut resolved = HashMap::new();
    let contact_list = response
        .get("data")
        .and_then(first_mapping)
        .and_then(|data| data.get("contactList"))
        .and_then(Value::as_array);
    let Some(contact_list) = contact_list else {
        return resolved;
    };
    for contact in contact_list {
        let Some(user_id) = value_text(contact.get("userId")) else {
            continue;
        };
        if !wanted.contains(&user_id) || resolved.contains_key(&user_id) {
            continue;
        }
        if let Some(name) =
            value_text(contact.get("nickname")).or_else(|| value_text(contact.get("realName")))
        {
            resolved.insert(
                user_id,
                ResolvedIdentity {
                    display_name: name,
                    source: "contact".to_string(),
                    refresh_existing_messages: true,
                },
            );
        }
    }
    resolved
}

fn value_text(value: Option<&Value>) -> Option<String> {
    match value? {
        Value::String(text) if !text.trim().is_empty() => Some(text.trim().to_string()),
        Value::Number(number) => Some(number.to_string()),
        _ => None,
    }
}

fn value_code(value: &Value) -> Option<i64> {
    match value {
        Value::Number(number) => number.as_i64(),
        Value::String(text) => text.trim().parse::<i64>().ok(),
        _ => None,
    }
}

fn error_chain(error: &anyhow::Error) -> String {
    error
        .chain()
        .map(ToString::to_string)
        .collect::<Vec<_>>()
        .join(": ")
}

#[cfg(test)]
fn can_materialize_platform_identity(person_ids: &[uuid::Uuid]) -> bool {
    person_ids.iter().copied().collect::<HashSet<_>>().len() == 1
}

#[derive(Debug, Clone, Serialize)]
struct IdentityKey {
    chat_id: String,
    sender_id: String,
    message_count: i64,
}

#[derive(Debug, Clone)]
struct ResolvedIdentity {
    display_name: String,
    source: String,
    refresh_existing_messages: bool,
}

#[derive(Debug, Default)]
struct AppliedIdentity {
    messages_updated: i64,
    platform_identities_materialized: i64,
}

#[derive(Debug, Default, Serialize)]
struct BackfillReport {
    total_identity_keys: usize,
    resolved: usize,
    unresolved: usize,
    messages_updated: i64,
    platform_identities_materialized: i64,
    source: String,
    dry_run: bool,
    unresolved_keys: Vec<IdentityKey>,
}

#[cfg(test)]
mod tests {
    use std::collections::HashSet;

    use serde_json::json;

    use super::{
        can_materialize_platform_identity, decode_chunked_body, parse_contact_identities,
        parse_http_response, parse_room_member_map, IdentityResolver, QiWeBackfillClient,
    };
    use uuid::Uuid;

    #[test]
    fn parses_chunked_qiwe_response_when_utf8_character_spans_chunks() {
        let body_text = r#"{"code":0,"msg":"成功","data":{"roomList":[{"memberList":[{"userId":"7881300369960035","name":"醒醒Wake（采风官）"}]}]}}"#;
        let body = body_text.as_bytes();
        let marker = "醒".as_bytes();
        let split_at = body
            .windows(marker.len())
            .position(|window| window == marker)
            .expect("fixture should contain Chinese display name")
            + 1;
        let chunked = format!("{:x}\r\n", split_at).into_bytes();
        let mut response = b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\nContent-Type: application/json; charset=UTF-8\r\n\r\n".to_vec();
        response.extend_from_slice(&chunked);
        response.extend_from_slice(&body[..split_at]);
        response.extend_from_slice(b"\r\n");
        response.extend_from_slice(format!("{:x}\r\n", body.len() - split_at).as_bytes());
        response.extend_from_slice(&body[split_at..]);
        response.extend_from_slice(b"\r\n0\r\n\r\n");

        let parsed = parse_http_response(&response).unwrap();

        assert!(parsed.contains("醒醒Wake"));
    }

    #[test]
    fn decodes_chunked_body() {
        assert_eq!(
            decode_chunked_body(b"5\r\nhello\r\n6\r\n world\r\n0\r\n\r\n").unwrap(),
            "hello world"
        );
    }

    #[test]
    fn parses_room_member_map_for_all_senders() {
        let response = json!({
            "code": 0,
            "data": {
                "roomList": [{
                    "memberList": [
                        {"userId": "u1", "name": "醒醒Wake", "roomRemarkName": ""},
                        {"userId": "u2", "name": "", "roomRemarkName": "群备注名"},
                        {"userId": "u4", "name": "ignored"}
                    ]
                }]
            }
        });

        let resolved = parse_room_member_map(&response);

        assert_eq!(resolved.len(), 3);
        assert_eq!(resolved["u1"].display_name, "醒醒Wake");
        assert_eq!(resolved["u1"].source, "room_member");
        assert_eq!(resolved["u2"].display_name, "群备注名");
        assert_eq!(resolved["u4"].display_name, "ignored");
        assert!(!resolved.contains_key("u3"));
    }

    #[test]
    fn parses_contact_identities_from_batch_userinfo() {
        let wanted = HashSet::from(["u1".to_string(), "u2".to_string(), "u3".to_string()]);
        let response = json!({
            "code": 0,
            "data": {
                "contactList": [
                    {"userId": "u1", "nickname": "小倩", "realName": ""},
                    {"userId": "u2", "nickname": "", "realName": "真实名"},
                    {"userId": "u4", "nickname": "ignored"}
                ]
            }
        });

        let resolved = parse_contact_identities(&response, &wanted);

        assert_eq!(resolved.len(), 2);
        assert_eq!(resolved["u1"].display_name, "小倩");
        assert_eq!(resolved["u1"].source, "contact");
        assert_eq!(resolved["u2"].display_name, "真实名");
        assert!(!resolved.contains_key("u3"));
        assert!(!resolved.contains_key("u4"));
    }

    #[test]
    fn platform_identity_materialization_requires_one_unique_person() {
        let person_1 = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let person_2 = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();

        assert!(can_materialize_platform_identity(&[person_1, person_1]));
        assert!(!can_materialize_platform_identity(&[]));
        assert!(!can_materialize_platform_identity(&[person_1, person_2]));
    }

    #[test]
    fn member_map_cache_reuses_loaded_room_members() {
        let resolver = IdentityResolver::new(
            QiWeBackfillClient {
                api_url: "http://127.0.0.1".to_string(),
                token: "unused".to_string(),
                guid: String::new(),
                tls_config: std::sync::Arc::new(super::tls_config()),
            },
            1200,
        );
        let mut identities = std::collections::HashMap::new();
        identities.insert(
            "u1".to_string(),
            super::ResolvedIdentity {
                display_name: "弦默".to_string(),
                source: "room_member".to_string(),
                refresh_existing_messages: true,
            },
        );
        resolver.member_maps.lock().unwrap().insert(
            "room-1".to_string(),
            super::CachedMemberMap {
                loaded_at: std::time::Instant::now(),
                identities,
            },
        );

        let resolved = resolver
            .resolve_chat("room-1", &["u1".to_string()], false)
            .unwrap();

        assert_eq!(resolved["u1"].display_name, "弦默");
    }
}
