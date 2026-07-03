use std::time::{Duration, Instant};

use anyhow::{anyhow, Context, Result};
use async_nats::jetstream::{self, context::Publish};
use chrono::Utc;
use serde::Serialize;
use serde_json::json;
use sqlx::postgres::PgPool;
use tracing::info;
use uuid::Uuid;

use crate::{config::Cli, db};

pub async fn run(cli: &Cli, timeout_seconds: u64, poll_interval_ms: u64) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let db_check = db::check(&pool).await?;
    if !db_check.schema_exists || !db_check.messages_table_exists {
        return Err(anyhow!(
            "qintopia_messages schema is not migrated; run `qintopia-message-sidecar migrate` first"
        ));
    }

    let client = async_nats::connect(&cli.nats_url)
        .await
        .with_context(|| format!("connect NATS at {}", cli.nats_url))?;
    let jetstream = jetstream::new(client);
    let _stream = jetstream
        .get_stream(&cli.nats_stream)
        .await
        .with_context(|| format!("get JetStream stream {}", cli.nats_stream))?;

    let event_id = format!("smoke-{}", Uuid::new_v4());
    let message_id = format!("{event_id}-message");
    let received_at = Utc::now();
    let sent_at = received_at;
    let text = format!("qintopia message sidecar smoke {message_id}");

    let raw_payload = json!({
        "event_id": event_id,
        "received_at": received_at.to_rfc3339(),
        "source": "qiwe",
        "payload": {
            "smoke": true,
            "msgUniqueIdentifier": message_id,
            "data": {
                "fromRoomId": "smoke-room",
                "senderId": "smoke-user",
                "text": text,
            }
        }
    });

    let message_payload = json!({
        "event_id": event_id,
        "message_id": message_id,
        "platform": "qiwe",
        "chat_id": "smoke-room",
        "chat_type": "group",
        "sender_id": "smoke-user",
        "sender_name": "Sidecar Smoke",
        "sender_identity": {
            "platform": "qiwe",
            "chat_id": "smoke-room",
            "channel_user_id": "smoke-user",
            "display_name": "Sidecar Smoke",
            "identity_source": "webhook",
            "resolved_at": received_at.to_rfc3339()
        },
        "text": text,
        "message_kind": "text",
        "is_mention_bot": true,
        "should_trigger": true,
        "trigger_reason": "smoke",
        "sent_at": sent_at.to_rfc3339(),
        "received_at": received_at.to_rfc3339(),
        "raw": {
            "smoke": true,
            "msgData": {
                "atList": [
                    {"userId": "smoke-bot", "nickname": "二花"}
                ]
            }
        }
    });

    publish_json(
        &jetstream,
        &cli.raw_subject,
        &format!("{message_id}:raw"),
        &raw_payload,
    )
    .await
    .context("publish smoke raw event")?;
    publish_json(
        &jetstream,
        &cli.message_subject,
        &message_id,
        &message_payload,
    )
    .await
    .context("publish smoke message event")?;

    let timeout = Duration::from_secs(timeout_seconds);
    let poll_interval = Duration::from_millis(poll_interval_ms.max(100));
    let result = wait_for_persisted_message(&pool, &message_id, timeout, poll_interval).await?;

    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

pub async fn inspect_message(cli: &Cli, platform: &str, message_id: &str) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let result = lookup_message_result(&pool, platform, message_id)
        .await?
        .ok_or_else(|| anyhow!("message not found platform={platform} message_id={message_id}"))?;
    println!("{}", serde_json::to_string_pretty(&result)?);
    Ok(())
}

async fn publish_json<T: Serialize>(
    jetstream: &jetstream::Context,
    subject: &str,
    message_id: &str,
    payload: &T,
) -> Result<()> {
    let bytes = serde_json::to_vec(payload).context("serialize smoke payload")?;
    let ack = jetstream
        .send_publish(
            subject.to_string(),
            Publish::build()
                .payload(bytes.into())
                .message_id(message_id),
        )
        .await
        .with_context(|| format!("publish to subject {subject}"))?
        .await
        .with_context(|| format!("wait for publish ack on subject {subject}"))?;
    info!(
        subject = %subject,
        stream = %ack.stream,
        sequence = ack.sequence,
        duplicate = ack.duplicate,
        "smoke event published"
    );
    Ok(())
}

async fn wait_for_persisted_message(
    pool: &PgPool,
    message_id: &str,
    timeout: Duration,
    poll_interval: Duration,
) -> Result<SmokeResult> {
    let deadline = Instant::now() + timeout;
    loop {
        if let Some(result) = lookup_message_result(pool, "qiwe", message_id).await? {
            return Ok(result);
        }
        if Instant::now() >= deadline {
            return Err(anyhow!(
                "smoke message {message_id} was not persisted within {} seconds",
                timeout.as_secs()
            ));
        }
        tokio::time::sleep(poll_interval).await;
    }
}

async fn lookup_message_result(
    pool: &PgPool,
    platform: &str,
    message_id: &str,
) -> Result<Option<SmokeResult>> {
    let row = sqlx::query_as::<
        _,
        (
            Uuid,
            String,
            String,
            String,
            String,
            bool,
            bool,
            i32,
            i64,
            i64,
        ),
    >(
        r#"
        SELECT
            m.id,
            m.message_id,
            m.chat_id,
            m.chat_type,
            m.sender_id,
            m.is_mention_bot,
            m.should_trigger,
            m.duplicate_count,
            COALESCE(mm.mention_count, 0) AS mention_count,
            COALESCE(j.job_count, 0) AS processing_job_count
        FROM qintopia_messages.messages m
        LEFT JOIN (
            SELECT message_id, count(*)::bigint AS mention_count
            FROM qintopia_messages.message_mentions
            GROUP BY message_id
        ) mm ON mm.message_id = m.id
        LEFT JOIN (
            SELECT message_id, count(*)::bigint AS job_count
            FROM qintopia_messages.message_processing_jobs
            GROUP BY message_id
        ) j ON j.message_id = m.id
        WHERE m.platform = $1 AND m.message_id = $2
        LIMIT 1
        "#,
    )
    .bind(platform)
    .bind(message_id)
    .fetch_optional(pool)
    .await
    .context("lookup smoke message")?;

    Ok(row.map(
        |(
            id,
            message_id,
            chat_id,
            chat_type,
            sender_id,
            is_mention_bot,
            should_trigger,
            duplicate_count,
            mention_count,
            processing_job_count,
        )| SmokeResult {
            ok: true,
            message_uuid: id.to_string(),
            message_id,
            chat_id,
            chat_type,
            sender_id,
            is_mention_bot,
            should_trigger,
            duplicate_count,
            mention_count,
            processing_job_count,
        },
    ))
}

#[derive(Debug, Serialize)]
struct SmokeResult {
    ok: bool,
    message_uuid: String,
    message_id: String,
    chat_id: String,
    chat_type: String,
    sender_id: String,
    is_mention_bot: bool,
    should_trigger: bool,
    duplicate_count: i32,
    mention_count: i64,
    processing_job_count: i64,
}
