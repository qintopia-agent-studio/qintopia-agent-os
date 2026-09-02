use std::time::Duration;

use anyhow::{Context, Result};
use async_nats::jetstream::{
    self,
    consumer::{self, PullConsumer},
    AckKind,
};
use futures::StreamExt;
use sqlx::postgres::PgPool;
use tracing::{debug, error, info, warn};

use crate::{
    channel_event_mapping,
    config::Cli,
    db,
    event::{dead_letter_payload_summary, NormalizedMessageEvent, RawQiweEvent},
    nats_connection,
};

pub async fn run(cli: Cli) -> Result<()> {
    let database_url = cli.database_url_required()?.to_string();
    let pool = db::connect(&database_url, cli.db_max_connections).await?;
    db::run_migrations(&pool).await?;

    nats_connection::validate_connection_config(&cli)?;
    let client = nats_connection::connect(&cli).await?;
    let jetstream = jetstream::new(client);
    let mut stream = jetstream
        .get_stream(&cli.nats_stream)
        .await
        .with_context(|| format!("get JetStream stream {}", cli.nats_stream))?;
    let info = stream.info().await.context("read stream info")?;
    info!(
        stream = %info.config.name,
        subjects = ?info.config.subjects,
        messages = info.state.messages,
        "connected to NATS stream"
    );

    let mut filter_subjects = vec![cli.raw_subject.clone(), cli.message_subject.clone()];
    if cli.trust_authenticated_raw_subject {
        filter_subjects.push(cli.authenticated_raw_subject.clone());
    }
    let consumer: PullConsumer = stream
        .get_or_create_consumer(
            &cli.consumer,
            consumer::pull::Config {
                durable_name: Some(cli.consumer.clone()),
                name: Some(cli.consumer.clone()),
                description: Some("Persist QiWe/Hermes messages into Postgres".to_string()),
                ack_policy: consumer::AckPolicy::Explicit,
                ack_wait: Duration::from_secs(60),
                max_deliver: 20,
                filter_subjects,
                ..Default::default()
            },
        )
        .await
        .with_context(|| format!("get or create durable consumer {}", cli.consumer))?;

    info!(
        consumer = %cli.consumer,
        raw_subject = %cli.raw_subject,
        authenticated_raw_subject = %cli.authenticated_raw_subject,
        trusted_raw_enabled = cli.trust_authenticated_raw_subject,
        message_subject = %cli.message_subject,
        "sidecar consumer started"
    );

    let mut messages = consumer
        .messages()
        .await
        .context("open consumer messages")?;
    loop {
        tokio::select! {
            _ = tokio::signal::ctrl_c() => {
                info!("shutdown signal received");
                break;
            }
            maybe_message = messages.next() => {
                match maybe_message {
                    Some(Ok(message)) => {
                        if let Err(error) = handle_nats_message(&pool, &cli, message).await {
                            error!(error = %error, "failed to handle NATS message");
                        }
                    }
                    Some(Err(error)) => {
                        warn!(error = %error, "consumer stream yielded error");
                        tokio::time::sleep(Duration::from_secs(1)).await;
                    }
                    None => {
                        warn!("consumer stream ended; reopening");
                        messages = consumer.messages().await.context("reopen consumer messages")?;
                    }
                }
            }
        }
    }

    Ok(())
}

async fn handle_nats_message(pool: &PgPool, cli: &Cli, message: jetstream::Message) -> Result<()> {
    let subject = message.subject.to_string();
    let stream_sequence = message.info().ok().map(|info| info.stream_sequence);
    let outcome = process_payload(pool, cli, &subject, stream_sequence, &message.payload).await;

    match outcome {
        Ok(()) => {
            ack(&message).await.context("ack message")?;
            debug!(subject = %subject, stream_sequence = ?stream_sequence, "message acked");
        }
        Err(ProcessError::InvalidPayload { kind, error }) => {
            warn!(
                subject = %subject,
                stream_sequence = ?stream_sequence,
                kind = %kind,
                error = %error,
                "invalid payload moved to dead letter"
            );
            db::insert_dead_letter(
                pool,
                &subject,
                stream_sequence,
                &cli.consumer,
                &kind,
                &error,
                &dead_letter_payload_summary(&message.payload),
            )
            .await?;
            ack(&message).await.context("ack dead-lettered message")?;
        }
        Err(ProcessError::Retryable(error)) => {
            warn!(
                subject = %subject,
                stream_sequence = ?stream_sequence,
                error = %error,
                delay_seconds = cli.nak_delay_seconds,
                "retryable processing error; NAK with delay"
            );
            nak(&message, Duration::from_secs(cli.nak_delay_seconds))
                .await
                .context("nak message")?;
        }
    }

    Ok(())
}

async fn ack(message: &jetstream::Message) -> Result<()> {
    message
        .ack()
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

async fn nak(message: &jetstream::Message, delay: Duration) -> Result<()> {
    message
        .ack_with(AckKind::Nak(Some(delay)))
        .await
        .map_err(|error| anyhow::anyhow!(error.to_string()))
}

async fn process_payload(
    pool: &PgPool,
    cli: &Cli,
    subject: &str,
    _stream_sequence: Option<u64>,
    payload: &[u8],
) -> std::result::Result<(), ProcessError> {
    if let Some(ingress_auth_verified) = raw_subject_authentication(
        &cli.raw_subject,
        &cli.authenticated_raw_subject,
        cli.trust_authenticated_raw_subject,
        subject,
    ) {
        let mut event =
            RawQiweEvent::from_slice(payload).map_err(|error| ProcessError::InvalidPayload {
                kind: "raw_parse_failed".to_string(),
                error: error.to_string(),
            })?;
        event.set_ingress_auth_verified(ingress_auth_verified);
        let raw_event_id = db::persist_raw_event(pool, subject, &event)
            .await
            .map_err(ProcessError::Retryable)?;
        let persisted_event = db::load_raw_event(pool, raw_event_id)
            .await
            .map_err(ProcessError::Retryable)?;
        channel_event_mapping::process_persisted_raw_event(pool, raw_event_id, &persisted_event)
            .await
            .map_err(ProcessError::Retryable)?;
        return Ok(());
    }

    if subject == cli.message_subject {
        let event = NormalizedMessageEvent::from_slice(payload).map_err(|error| {
            ProcessError::InvalidPayload {
                kind: "message_parse_failed".to_string(),
                error: error.to_string(),
            }
        })?;
        db::persist_message(pool, subject, &event)
            .await
            .map_err(ProcessError::Retryable)?;
        return Ok(());
    }

    Err(ProcessError::InvalidPayload {
        kind: "unknown_subject".to_string(),
        error: format!("unexpected subject {subject}"),
    })
}

fn raw_subject_authentication(
    raw_subject: &str,
    authenticated_raw_subject: &str,
    trust_authenticated_raw_subject: bool,
    subject: &str,
) -> Option<bool> {
    if subject == raw_subject {
        return Some(false);
    }
    (trust_authenticated_raw_subject && subject == authenticated_raw_subject).then_some(true)
}

#[derive(Debug)]
enum ProcessError {
    InvalidPayload { kind: String, error: String },
    Retryable(anyhow::Error),
}

#[cfg(test)]
mod tests {
    use super::raw_subject_authentication;

    #[test]
    fn legacy_raw_subject_can_never_assert_authenticated_ingress() {
        assert_eq!(
            raw_subject_authentication(
                "qintopia.qiwe.raw",
                "qintopia.qiwe.raw.authenticated",
                true,
                "qintopia.qiwe.raw",
            ),
            Some(false)
        );
    }

    #[test]
    fn authenticated_subject_requires_explicit_consumer_trust() {
        assert_eq!(
            raw_subject_authentication(
                "qintopia.qiwe.raw",
                "qintopia.qiwe.raw.authenticated",
                false,
                "qintopia.qiwe.raw.authenticated",
            ),
            None
        );
        assert_eq!(
            raw_subject_authentication(
                "qintopia.qiwe.raw",
                "qintopia.qiwe.raw.authenticated",
                true,
                "qintopia.qiwe.raw.authenticated",
            ),
            Some(true)
        );
    }
}
