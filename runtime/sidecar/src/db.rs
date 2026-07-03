use std::path::{Path, PathBuf};

use anyhow::{Context, Result};
use chrono::Utc;
use serde_json::Value;
use sqlx::{
    postgres::{PgPool, PgPoolOptions},
    Postgres, Transaction,
};
use uuid::Uuid;

use crate::event::{
    mention_display_name, mention_key, mention_user_id, NormalizedMessageEvent, RawQiweEvent,
    SenderIdentityEvent,
};

const PROCESSING_JOBS: [&str; 3] = [
    "embedding_pending",
    "entity_extract_pending",
    "graph_projection_pending",
];

pub async fn connect(database_url: &str, max_connections: u32) -> Result<PgPool> {
    PgPoolOptions::new()
        .max_connections(max_connections)
        .connect(database_url)
        .await
        .context("connect Postgres")
}

pub async fn run_migrations(pool: &PgPool) -> Result<()> {
    let schema_exists: (bool,) =
        sqlx::query_as("select to_regnamespace('qintopia_messages') is not null")
            .fetch_one(pool)
            .await
            .context("check qintopia_messages schema before migrations")?;

    if !schema_exists.0 {
        sqlx::query("CREATE SCHEMA qintopia_messages")
            .execute(pool)
            .await
            .context(
                "create qintopia_messages schema; grant database CREATE or pre-create the schema",
            )?;
    }

    let vector_extension_exists: (bool,) =
        sqlx::query_as("select exists(select 1 from pg_extension where extname = 'vector')")
            .fetch_one(pool)
            .await
            .context("check vector extension before migrations")?;
    let pgcrypto_extension_exists: (bool,) =
        sqlx::query_as("select exists(select 1 from pg_extension where extname = 'pgcrypto')")
            .fetch_one(pool)
            .await
            .context("check pgcrypto extension before migrations")?;

    if !vector_extension_exists.0 {
        sqlx::query("CREATE EXTENSION vector WITH SCHEMA qintopia_messages")
            .execute(pool)
            .await
            .context(
                "create vector extension; pre-install it if this user cannot create extensions",
            )?;
    }
    if !pgcrypto_extension_exists.0 {
        sqlx::query("CREATE EXTENSION pgcrypto WITH SCHEMA qintopia_messages")
            .execute(pool)
            .await
            .context(
                "create pgcrypto extension; pre-install it if this user cannot create extensions",
            )?;
    }

    let mut connection = pool
        .acquire()
        .await
        .context("acquire migration connection")?;
    sqlx::query("SET search_path TO qintopia_messages, public")
        .execute(&mut *connection)
        .await
        .context("set migration search_path")?;

    let migrator = sqlx::migrate::Migrator::new(migrations_dir().as_path())
        .await
        .context("load migrations")?;
    migrator
        .run(&mut *connection)
        .await
        .context("run migrations")
}

fn migrations_dir() -> PathBuf {
    if let Ok(path) = std::env::var("QINTOPIA_SIDECAR_MIGRATIONS_DIR") {
        return PathBuf::from(path);
    }

    let manifest_dir = Path::new(env!("CARGO_MANIFEST_DIR"));
    let monorepo_dir = manifest_dir.join("../postgres/migrations");
    if monorepo_dir.exists() {
        return monorepo_dir;
    }

    manifest_dir.join("migrations")
}

pub async fn check(pool: &PgPool) -> Result<DbCheck> {
    let database: (String,) = sqlx::query_as("select current_database()")
        .fetch_one(pool)
        .await
        .context("read current database")?;
    let schema_exists: (bool,) =
        sqlx::query_as("select exists(select 1 from information_schema.schemata where schema_name = 'qintopia_messages')")
            .fetch_one(pool)
            .await
            .context("check qintopia_messages schema")?;
    let messages_table_exists: (bool,) =
        sqlx::query_as("select to_regclass('qintopia_messages.messages') is not null")
            .fetch_one(pool)
            .await
            .context("check messages table")?;
    Ok(DbCheck {
        database: database.0,
        schema_exists: schema_exists.0,
        messages_table_exists: messages_table_exists.0,
    })
}

#[derive(Debug, Clone)]
pub struct DbCheck {
    pub database: String,
    pub schema_exists: bool,
    pub messages_table_exists: bool,
}

pub async fn persist_raw_event(pool: &PgPool, subject: &str, event: &RawQiweEvent) -> Result<Uuid> {
    let mut tx = pool.begin().await.context("begin raw event transaction")?;
    let id: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO qintopia_messages.raw_events
            (event_id, source, subject, received_at, payload)
        VALUES ($1, $2, $3, $4, $5)
        ON CONFLICT (source, event_id) DO UPDATE SET
            subject = EXCLUDED.subject,
            received_at = EXCLUDED.received_at,
            payload = EXCLUDED.payload,
            last_seen_at = now(),
            duplicate_count = qintopia_messages.raw_events.duplicate_count + 1
        RETURNING id
        "#,
    )
    .bind(&event.event_id)
    .bind(&event.source)
    .bind(subject)
    .bind(event.received_at)
    .bind(&event.payload)
    .fetch_one(&mut *tx)
    .await
    .context("upsert raw event")?;
    tx.commit().await.context("commit raw event transaction")?;
    Ok(id.0)
}

pub async fn persist_message(
    pool: &PgPool,
    subject: &str,
    event: &NormalizedMessageEvent,
) -> Result<Uuid> {
    let mut tx = pool.begin().await.context("begin message transaction")?;
    let raw_event_id = ensure_raw_placeholder(&mut tx, subject, event)
        .await
        .context("ensure raw event placeholder")?;
    let message_id: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO qintopia_messages.messages
            (
                platform,
                message_id,
                event_id,
                chat_id,
                chat_type,
                sender_id,
                sender_name,
                message_kind,
                text,
                is_mention_bot,
                should_trigger,
                trigger_reason,
                sent_at,
                received_at,
                raw_event_id,
                raw
            )
        VALUES
            ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, $15, $16)
        ON CONFLICT (platform, message_id) DO UPDATE SET
            event_id = EXCLUDED.event_id,
            chat_id = EXCLUDED.chat_id,
            chat_type = EXCLUDED.chat_type,
            sender_id = EXCLUDED.sender_id,
            sender_name = EXCLUDED.sender_name,
            message_kind = EXCLUDED.message_kind,
            text = EXCLUDED.text,
            is_mention_bot = EXCLUDED.is_mention_bot,
            should_trigger = EXCLUDED.should_trigger,
            trigger_reason = EXCLUDED.trigger_reason,
            sent_at = EXCLUDED.sent_at,
            received_at = EXCLUDED.received_at,
            raw_event_id = COALESCE(EXCLUDED.raw_event_id, qintopia_messages.messages.raw_event_id),
            raw = EXCLUDED.raw,
            updated_at = now(),
            last_seen_at = now(),
            duplicate_count = qintopia_messages.messages.duplicate_count + 1
        RETURNING id
        "#,
    )
    .bind(&event.platform)
    .bind(&event.message_id)
    .bind(&event.event_id)
    .bind(&event.chat_id)
    .bind(&event.chat_type)
    .bind(&event.sender_id)
    .bind(&event.sender_name)
    .bind(&event.message_kind)
    .bind(&event.text)
    .bind(event.is_mention_bot)
    .bind(event.should_trigger)
    .bind(&event.trigger_reason)
    .bind(event.sent_at)
    .bind(event.received_at)
    .bind(raw_event_id)
    .bind(&event.raw)
    .fetch_one(&mut *tx)
    .await
    .context("upsert message")?;

    sqlx::query("DELETE FROM qintopia_messages.message_mentions WHERE message_id = $1")
        .bind(message_id.0)
        .execute(&mut *tx)
        .await
        .context("delete existing mentions")?;

    for (index, mention) in event.mentions.iter().enumerate() {
        sqlx::query(
            r#"
            INSERT INTO qintopia_messages.message_mentions
                (message_id, mention_key, platform_user_id, display_name, raw)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (message_id, mention_key) DO UPDATE SET
                platform_user_id = EXCLUDED.platform_user_id,
                display_name = EXCLUDED.display_name,
                raw = EXCLUDED.raw
            "#,
        )
        .bind(message_id.0)
        .bind(mention_key(mention, index))
        .bind(mention_user_id(mention))
        .bind(mention_display_name(mention))
        .bind(mention)
        .execute(&mut *tx)
        .await
        .context("insert mention")?;
    }

    if let Some(identity) = event.sender_identity.as_ref() {
        let identity_id = upsert_channel_identity(
            &mut tx,
            identity,
            &event.platform,
            &event.chat_id,
            &event.sender_id,
            Some(message_id.0),
            Some(&event.event_id),
        )
        .await
        .context("upsert sender identity")?;
        sqlx::query(
            r#"
            UPDATE qintopia_messages.messages
            SET sender_channel_identity_id = $1,
                sender_person_id = COALESCE(sender_person_id, (
                    SELECT person_id
                    FROM qintopia_identity.channel_identities
                    WHERE id = $1
                )),
                sender_name = $2,
                updated_at = now()
            WHERE id = $3
            "#,
        )
        .bind(identity_id)
        .bind(&identity.display_name)
        .bind(message_id.0)
        .execute(&mut *tx)
        .await
        .context("link message sender identity")?;
    }

    for job_type in PROCESSING_JOBS {
        sqlx::query(
            r#"
            INSERT INTO qintopia_messages.message_processing_jobs
                (message_id, job_type)
            VALUES ($1, $2)
            ON CONFLICT (message_id, job_type) DO NOTHING
            "#,
        )
        .bind(message_id.0)
        .bind(job_type)
        .execute(&mut *tx)
        .await
        .with_context(|| format!("insert processing job {job_type}"))?;
    }

    tx.commit().await.context("commit message transaction")?;
    Ok(message_id.0)
}

pub async fn upsert_channel_identity(
    tx: &mut Transaction<'_, Postgres>,
    identity: &SenderIdentityEvent,
    fallback_platform: &str,
    fallback_chat_id: &str,
    fallback_user_id: &str,
    source_message_id: Option<Uuid>,
    source_event_id: Option<&str>,
) -> Result<Uuid> {
    let platform = non_empty(&identity.platform).unwrap_or(fallback_platform);
    let chat_id = non_empty(&identity.chat_id).unwrap_or(fallback_chat_id);
    let channel_user_id = non_empty(&identity.channel_user_id).unwrap_or(fallback_user_id);
    if platform.is_empty() || chat_id.is_empty() || channel_user_id.is_empty() {
        anyhow::bail!("sender identity missing platform/chat/user id");
    }
    if identity.display_name.trim().is_empty() {
        anyhow::bail!("sender identity missing display_name");
    }
    let normalized_display_name = normalize_display_name(&identity.display_name);
    let confidence = identity_confidence(&identity.identity_source);
    let observed_at = identity.resolved_at.unwrap_or_else(Utc::now);

    let id: (Uuid,) = sqlx::query_as(
        r#"
        INSERT INTO qintopia_identity.channel_identities
            (
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
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $8, $9)
        ON CONFLICT (platform, channel_user_id, chat_id) DO UPDATE SET
            display_name = CASE
                WHEN qintopia_identity.identity_source_rank(EXCLUDED.identity_source) >= qintopia_identity.identity_source_rank(qintopia_identity.channel_identities.identity_source)
                THEN EXCLUDED.display_name
                ELSE qintopia_identity.channel_identities.display_name
            END,
            normalized_display_name = CASE
                WHEN qintopia_identity.identity_source_rank(EXCLUDED.identity_source) >= qintopia_identity.identity_source_rank(qintopia_identity.channel_identities.identity_source)
                THEN EXCLUDED.normalized_display_name
                ELSE qintopia_identity.channel_identities.normalized_display_name
            END,
            identity_source = CASE
                WHEN qintopia_identity.identity_source_rank(EXCLUDED.identity_source) >= qintopia_identity.identity_source_rank(qintopia_identity.channel_identities.identity_source)
                THEN EXCLUDED.identity_source
                ELSE qintopia_identity.channel_identities.identity_source
            END,
            confidence = GREATEST(qintopia_identity.channel_identities.confidence, EXCLUDED.confidence),
            last_seen_at = GREATEST(qintopia_identity.channel_identities.last_seen_at, EXCLUDED.last_seen_at),
            metadata = qintopia_identity.channel_identities.metadata || EXCLUDED.metadata,
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(platform)
    .bind(channel_user_id)
    .bind(chat_id)
    .bind(&identity.display_name)
    .bind(&normalized_display_name)
    .bind(&identity.identity_source)
    .bind(confidence)
    .bind(observed_at)
    .bind(&identity.metadata)
    .fetch_one(&mut **tx)
    .await
    .context("upsert channel identity")?;

    sqlx::query(
        r#"
        INSERT INTO qintopia_identity.channel_identity_observations
            (
                channel_identity_id,
                platform,
                chat_id,
                channel_user_id,
                observed_display_name,
                normalized_display_name,
                observation_source,
                source_message_id,
                source_event_id,
                observed_at,
                confidence,
                metadata
            )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12)
        "#,
    )
    .bind(id.0)
    .bind(platform)
    .bind(chat_id)
    .bind(channel_user_id)
    .bind(&identity.display_name)
    .bind(&normalized_display_name)
    .bind(&identity.identity_source)
    .bind(source_message_id)
    .bind(source_event_id)
    .bind(observed_at)
    .bind(confidence)
    .bind(&identity.metadata)
    .execute(&mut **tx)
    .await
    .context("insert channel identity observation")?;

    Ok(id.0)
}

pub fn normalize_display_name(name: &str) -> String {
    name.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn non_empty(value: &str) -> Option<&str> {
    let trimmed = value.trim();
    if trimmed.is_empty() {
        None
    } else {
        Some(trimmed)
    }
}

fn identity_confidence(source: &str) -> f64 {
    match source {
        "room_member" => 1.0,
        "contact" => 0.95,
        "webhook" => 0.9,
        "current_backfill" => 0.85,
        _ => 0.5,
    }
}

pub async fn insert_dead_letter(
    pool: &PgPool,
    subject: &str,
    stream_sequence: Option<u64>,
    consumer: &str,
    error_kind: &str,
    error: &str,
    payload_text: &str,
) -> Result<()> {
    let stream_sequence = stream_sequence.and_then(|value| i64::try_from(value).ok());
    sqlx::query(
        r#"
        INSERT INTO qintopia_messages.dead_letter_events
            (subject, stream_sequence, consumer, error_kind, error, payload_text)
        VALUES ($1, $2, $3, $4, $5, $6)
        "#,
    )
    .bind(subject)
    .bind(stream_sequence)
    .bind(consumer)
    .bind(error_kind)
    .bind(error)
    .bind(payload_text)
    .execute(pool)
    .await
    .context("insert dead letter")?;
    Ok(())
}

async fn ensure_raw_placeholder(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    subject: &str,
    event: &NormalizedMessageEvent,
) -> Result<Option<Uuid>> {
    let id: (Uuid,) = sqlx::query_as(
        r#"
        WITH inserted AS (
            INSERT INTO qintopia_messages.raw_events
                (event_id, source, subject, received_at, payload)
            VALUES ($1, $2, $3, $4, $5)
            ON CONFLICT (source, event_id) DO NOTHING
            RETURNING id
        )
        SELECT id FROM inserted
        UNION ALL
        SELECT id FROM qintopia_messages.raw_events
            WHERE source = $2 AND event_id = $1
        LIMIT 1
        "#,
    )
    .bind(&event.event_id)
    .bind(&event.platform)
    .bind(subject)
    .bind(Utc::now())
    .bind(raw_placeholder_payload(event))
    .fetch_one(&mut **tx)
    .await
    .context("insert/select raw placeholder")?;
    Ok(Some(id.0))
}

fn raw_placeholder_payload(event: &NormalizedMessageEvent) -> Value {
    if event.raw.is_null() {
        Value::Object(Default::default())
    } else {
        event.raw.clone()
    }
}
