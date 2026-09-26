//! Signed payment ingress. Events do not grant business authority.
#[path = "production_events.rs"]
mod production_events;
#[cfg(test)]
#[path = "production_events_tests.rs"]
mod production_events_tests;
use super::{
    store::business_events::{PaymentEvent, PAYMENT_SCHEMA},
    Store,
};
use crate::resident_welcome::{
    ingress::Response,
    protocol::{self, SigningKey},
};
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde::Deserialize;
use serde_json::json;
use std::path::Path;
use uuid::Uuid;

pub(super) struct Config {
    pub binding: Uuid,
    pub key: SigningKey,
}
impl Config {
    pub(super) fn local() -> Result<Self> {
        ensure!(
            std::env::var("QINTOPIA_PMS_EVENTS_LOCAL_ENABLE").as_deref() == Ok("1")
                && std::env::var("QINTOPIA_FOUNDATION_LOCAL_ENABLE").as_deref() == Ok("1")
                && std::env::var("QINTOPIA_PMS_EVENTS_PRODUCTION_ENABLE").as_deref() != Ok("1")
                && std::env::var("QINTOPIA_FOUNDATION_PRODUCTION_ENABLE").as_deref() != Ok("1"),
            "payment_ingress_disabled"
        );
        Self::from_environment(false)
    }

    pub(super) fn production() -> Result<Self> {
        ensure!(
            std::env::var("QINTOPIA_PMS_EVENTS_PRODUCTION_ENABLE").as_deref() == Ok("1")
                && std::env::var("QINTOPIA_FOUNDATION_PRODUCTION_ENABLE").as_deref() == Ok("1")
                && std::env::var("QINTOPIA_PMS_EVENTS_LOCAL_ENABLE").as_deref() != Ok("1")
                && std::env::var("QINTOPIA_FOUNDATION_LOCAL_ENABLE").as_deref() != Ok("1"),
            "payment_ingress_disabled"
        );
        Self::from_environment(true)
    }

    fn from_environment(production: bool) -> Result<Self> {
        let binding = Uuid::parse_str(&std::env::var("QINTOPIA_PMS_EVENT_BINDING")?)?;
        let source = std::env::var("QINTOPIA_PMS_EVENT_SOURCE")?;
        let property = std::env::var("QINTOPIA_PMS_EVENT_PROPERTY")?;
        let key_id = std::env::var("QINTOPIA_PMS_EVENT_KEY_ID")?;
        ensure!(
            [&source, &property, &key_id]
                .iter()
                .all(|s| protocol::reference(s)),
            "invalid_payment_config"
        );
        let path = std::env::var("QINTOPIA_PMS_EVENT_KEY_FILE")?;
        let path = Path::new(&path);
        ensure!(path.is_absolute(), "invalid_payment_key_file");
        let metadata = std::fs::symlink_metadata(path)?;
        ensure!(
            metadata.is_file() && metadata.len() <= 512,
            "invalid_payment_key_file"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                metadata.permissions().mode() & 0o077 == 0
                    && (!production || metadata.permissions().mode() & 0o777 == 0o600),
                "invalid_payment_key_file"
            );
        }
        #[cfg(not(unix))]
        anyhow::bail!("payment_key_permissions_unsupported");
        let secret = zeroize::Zeroizing::new(std::fs::read_to_string(path)?);
        let trimmed = secret.trim_end_matches(['\r', '\n']);
        ensure!(
            (32..=256).contains(&trimmed.len()) && !trimmed.chars().any(char::is_control),
            "invalid_payment_key_file"
        );
        Ok(Self {
            binding,
            key: SigningKey {
                key_id,
                secret: zeroize::Zeroizing::new(trimmed.as_bytes().to_vec()),
                source_instance: source,
                properties: [property].into_iter().collect(),
            },
        })
    }
}

/// Called by the existing Sidecar service as an isolated task; failure is local to this listener.
#[allow(dead_code)] // The shared production entry point is wired by the foundation owner.
pub(super) async fn run_production_events(store: Store, port: u16) -> Result<()> {
    production_events::run(store, port).await
}
#[derive(Deserialize)]
#[serde(rename_all = "camelCase", deny_unknown_fields)]
struct Envelope {
    schema_version: String,
    source_instance: String,
    property_id: String,
    event_id: String,
    sequence: String,
    bill_id: String,
    kind: String,
    event_type: String,
    occurred_at: DateTime<Utc>,
}
fn response(status: u16, code: &str) -> Response {
    Response {
        status,
        body: json!({"code":code}),
    }
}
pub(super) async fn receive(
    store: &Store,
    config: &Config,
    request: &crate::local_http::Request,
    now: i64,
) -> Response {
    let crate::local_http::Request {
        method,
        path,
        headers,
        body: raw,
    } = request;
    if method != "POST" || path != protocol::PATH {
        return response(404, "not_found");
    }
    // Authenticate exact bytes using the existing, shared HMAC contract before decoding fields.
    if let Err(e) = protocol::authenticate(raw, headers, std::slice::from_ref(&config.key), now) {
        return response(e.status, e.code);
    }
    let value = match protocol::parse_unique(raw) {
        Ok(v) => v,
        Err(e) => return response(e.status, e.code),
    };
    let envelope: Envelope = match serde_json::from_value(value) {
        Ok(v) => v,
        Err(_) => return response(422, "invalid_payment_envelope"),
    };
    if envelope.schema_version != PAYMENT_SCHEMA {
        return response(422, "unsupported_schema");
    }
    if envelope.source_instance != config.key.source_instance
        || !config.key.properties.contains(&envelope.property_id)
    {
        return response(403, "source_scope_mismatch");
    }
    let event = PaymentEvent {
        event_id: envelope.event_id,
        bill_id: envelope.bill_id,
        kind: envelope.kind,
        event_type: envelope.event_type,
        occurred_at: envelope.occurred_at,
        sequence: envelope.sequence,
    };
    if event.validate().is_err() {
        return response(422, "invalid_payment_contract");
    }
    match store
        .business_accept_payment(
            config.binding,
            &envelope.source_instance,
            &envelope.property_id,
            &event,
        )
        .await
    {
        Ok(ack) => Response {
            status: if ack["status"] == "accepted" {
                202
            } else {
                200
            },
            body: ack,
        },
        Err(e) => match e.to_string().as_str() {
            "business_binding_unavailable" | "feed_property_mismatch" => {
                response(403, "source_scope_mismatch")
            }
            "business_binding_changed" => response(409, "business_binding_changed"),
            "feed_event_conflict" | "feed_sequence_conflict" => {
                response(409, "payment_event_conflict")
            }
            "source_activation_snapshot_required" => response(503, "payment_baseline_required"),
            _ => response(503, "inbox_unavailable"),
        },
    }
}

// Called only inside the independently authenticated host-ingress branch.
pub(super) async fn feed(
    store: &Store,
    arguments: &serde_json::Value,
) -> Result<serde_json::Value> {
    let config = if store.is_live() {
        Config::production()?
    } else {
        Config::local()?
    };
    let object = arguments
        .as_object()
        .ok_or_else(|| anyhow::anyhow!("invalid_arguments"))?;
    ensure!(
        object
            .keys()
            .all(|k| matches!(k.as_str(), "action" | "head" | "expected" | "page")),
        "invalid_arguments"
    );
    let context = store.business_feed_context(config.binding).await?;
    ensure!(
        context["sourceInstance"] == config.key.source_instance
            && context["propertyId"]
                .as_str()
                .is_some_and(|p| config.key.properties.contains(p)),
        "source_scope_mismatch"
    );
    match arguments["action"].as_str() {
        Some("context") if object.len() == 1 => Ok(context),
        Some("open") if object.len() == 1 || (object.len() == 2 && object.contains_key("head")) => {
            let head = arguments
                .get("head")
                .cloned()
                .map(serde_json::from_value)
                .transpose()?;
            store
                .business_feed_open(config.binding, head.as_ref())
                .await
        }
        Some("page") if object.len() == 3 && object.contains_key("page") => {
            store
                .business_feed_page(
                    config.binding,
                    &config.key.source_instance,
                    arguments["expected"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("invalid_arguments"))?,
                    serde_json::from_value(arguments["page"].clone())?,
                )
                .await
        }
        _ => anyhow::bail!("invalid_arguments"),
    }
}
