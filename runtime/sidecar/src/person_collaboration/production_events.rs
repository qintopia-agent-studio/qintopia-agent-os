//! Fixed, loopback-only production payment transport inside the existing Sidecar.
use super::{Config, Store};
use crate::local_http::{request, respond};
use anyhow::{ensure, Result};
use hmac::{Hmac, Mac};
use serde_json::json;
use sha2::Sha256;
use std::{net::Ipv4Addr, path::Path, time::Duration};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use uuid::Uuid;
use zeroize::Zeroizing;

const WAKE_PATH: &str = "/webhooks/anan-workitem-wake";
const RECEIVE_TIMEOUT: Duration = Duration::from_secs(5);
const WAKE_TIMEOUT: Duration = Duration::from_secs(2);

pub(super) struct WakeConfig {
    pub(super) port: u16,
    pub(super) gateway: String,
    pub(super) secret: Zeroizing<Vec<u8>>,
}

impl WakeConfig {
    pub(super) fn from_environment(payment: &Config) -> Result<Option<Self>> {
        match std::env::var("QINTOPIA_PMS_WORKITEM_WAKE_ENABLE").as_deref() {
            Err(_) | Ok("0") => return Ok(None),
            Ok("1") => {}
            _ => anyhow::bail!("invalid_payment_wake_config"),
        }
        let port = std::env::var("QINTOPIA_PMS_WORKITEM_WAKE_PORT")?.parse::<u16>()?;
        let gateway = std::env::var("QINTOPIA_PMS_WORKITEM_WAKE_GATEWAY")?;
        ensure!(
            port != 0 && crate::resident_welcome::protocol::reference(&gateway),
            "invalid_payment_wake_config"
        );
        let file = std::env::var("QINTOPIA_PMS_WORKITEM_WAKE_KEY_FILE")?;
        let path = Path::new(&file);
        ensure!(path.is_absolute(), "invalid_payment_wake_key_file");
        let meta = std::fs::symlink_metadata(path)?;
        ensure!(
            meta.is_file() && meta.len() <= 512,
            "invalid_payment_wake_key_file"
        );
        #[cfg(unix)]
        {
            use std::os::unix::fs::PermissionsExt;
            ensure!(
                meta.permissions().mode() & 0o777 == 0o600,
                "invalid_payment_wake_key_file"
            );
        }
        #[cfg(not(unix))]
        anyhow::bail!("payment_wake_key_permissions_unsupported");
        let raw = Zeroizing::new(std::fs::read_to_string(path)?);
        let secret = raw.trim_end_matches(['\r', '\n']).as_bytes().to_vec();
        ensure!(
            (32..=512).contains(&secret.len()) && !secret.iter().any(u8::is_ascii_control),
            "invalid_payment_wake_key_file"
        );
        ensure!(
            secret.as_slice() != payment.key.secret.as_slice(),
            "payment_wake_key_not_independent"
        );
        for name in [
            "QINTOPIA_FOUNDATION_TOKEN",
            "QINTOPIA_FOUNDATION_HOST_TOKEN",
            "GREENPMS_API_TOKEN",
        ] {
            if let Ok(other) = std::env::var(name) {
                ensure!(
                    secret != other.as_bytes(),
                    "payment_wake_key_not_independent"
                );
            }
        }
        Ok(Some(Self {
            port,
            gateway,
            secret: Zeroizing::new(secret),
        }))
    }
}

pub(super) async fn eligible_work(
    store: &Store,
    config: &Config,
    wake: &WakeConfig,
    receipt: Uuid,
) -> Result<Option<Uuid>> {
    let Some(work) = store
        .business_payment_receipt_workitem(config.binding, receipt)
        .await?
    else {
        return Ok(None);
    };
    let state = store
        .business_payment_workitem_read(&wake.gateway, config.binding, work)
        .await?;
    Ok((state["status"] == "awaiting_review"
        && state["kind"] == "payment"
        && state["readback_required"] == true
        && state["requires_human_confirmation"] == true)
        .then_some(work))
}

async fn send_hint(wake: &WakeConfig, work: Uuid) -> Result<()> {
    let body = format!(r#"{{"schema_version":1,"work_item_id":"{work}"}}"#);
    let timestamp = chrono::Utc::now().timestamp().to_string();
    let mut mac = Hmac::<Sha256>::new_from_slice(&wake.secret)?;
    mac.update(timestamp.as_bytes());
    mac.update(b".");
    mac.update(body.as_bytes());
    let signature = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    let wire = format!(
        "POST {WAKE_PATH} HTTP/1.1\r\nHost: 127.0.0.1:{}\r\nContent-Type: application/json\r\nContent-Length: {}\r\nX-Request-ID: {work}\r\nX-Webhook-Timestamp: {timestamp}\r\nX-Webhook-Signature-V2: {signature}\r\nConnection: close\r\n\r\n{body}",
        wake.port, body.len()
    );
    let mut stream = TcpStream::connect((Ipv4Addr::LOCALHOST, wake.port)).await?;
    stream.write_all(wire.as_bytes()).await?;
    let mut response = Vec::new();
    let mut chunk = [0_u8; 1024];
    loop {
        let count = stream.read(&mut chunk).await?;
        if count == 0 {
            break;
        }
        ensure!(
            response.len() + count <= 8192,
            "payment_wake_response_too_large"
        );
        response.extend_from_slice(&chunk[..count]);
    }
    ensure!(
        response.starts_with(b"HTTP/1.1 "),
        "invalid_payment_wake_response"
    );
    let parsed = crate::bounded_http::parse_http_response(response, 4096)?;
    ensure!(
        parsed
            .headers
            .get("content-type")
            .is_some_and(|v| v.starts_with("application/json")),
        "invalid_payment_wake_response"
    );
    if let Some(length) = parsed.headers.get("content-length") {
        ensure!(
            length.parse::<usize>()? == parsed.body.len(),
            "invalid_payment_wake_response"
        );
    }
    let result: serde_json::Value = serde_json::from_slice(&parsed.body)?;
    ensure!(
        result["delivery_id"] == work.to_string(),
        "payment_wake_rejected"
    );
    ensure!(
        matches!(
            (parsed.status, result["status"].as_str()),
            (202, Some("accepted")) | (200, Some("duplicate"))
        ),
        "payment_wake_rejected"
    );
    Ok(())
}

pub(super) async fn handle(
    stream: &mut TcpStream,
    store: &Store,
    config: &Config,
    port: u16,
    wake: Option<&WakeConfig>,
) -> Result<()> {
    let receipt = tokio::time::timeout(RECEIVE_TIMEOUT, async {
        let incoming = match request(stream, port).await {
            Ok(value) => value,
            Err(error) => {
                let status = if error.to_string() == "body_too_large" {
                    413
                } else {
                    400
                };
                respond(
                    stream,
                    status,
                    "application/json",
                    &serde_json::to_vec(&json!({"code":"invalid_request"}))?,
                    None,
                )
                .await?;
                return Ok(None);
            }
        };
        let response =
            super::receive(store, config, &incoming, chrono::Utc::now().timestamp()).await;
        let receipt = if response.status == 202 && response.body["status"] == "accepted" {
            response.body["receipt_id"]
                .as_str()
                .and_then(|id| Uuid::parse_str(id).ok())
        } else {
            None
        };
        respond(
            stream,
            response.status,
            "application/json",
            &serde_json::to_vec(&response.body)?,
            None,
        )
        .await?;
        stream.flush().await?;
        Ok::<_, anyhow::Error>(receipt)
    })
    .await
    .map_err(|_| anyhow::anyhow!("payment_receive_timeout"))??;
    if let (Some(wake), Some(receipt)) = (wake, receipt) {
        match tokio::time::timeout(WAKE_TIMEOUT, async {
            if let Some(work) = eligible_work(store, config, wake, receipt).await? {
                send_hint(wake, work).await?;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await
        {
            Ok(Ok(())) => {}
            Ok(Err(_)) => eprintln!("pms_workitem_wake_failed"),
            Err(_) => eprintln!("pms_workitem_wake_timeout"),
        }
    }
    Ok(())
}

pub(super) async fn run(store: Store, port: u16) -> Result<()> {
    ensure!(
        store.is_live() && port != 0,
        "payment_listener_configuration_required"
    );
    let config = Config::production()?;
    let wake = WakeConfig::from_environment(&config)?;
    let context = store.business_feed_context(config.binding).await?;
    ensure!(
        context["sourceInstance"] == config.key.source_instance
            && context["propertyId"]
                .as_str()
                .is_some_and(|property| config.key.properties.contains(property)),
        "payment_listener_scope_mismatch"
    );
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, port))
        .await
        .map_err(|_| anyhow::anyhow!("payment_listener_unavailable"))?;
    loop {
        let (mut stream, peer) = listener
            .accept()
            .await
            .map_err(|_| anyhow::anyhow!("payment_listener_unavailable"))?;
        if !peer.ip().is_loopback() {
            continue;
        }
        // A slow or malformed sender occupies only this payment listener task.
        let _ = handle(&mut stream, &store, &config, port, wake.as_ref()).await;
    }
}
