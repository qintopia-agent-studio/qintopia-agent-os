//! Live payment persistence uses the same simulated transport with an isolated database.
#![cfg(feature = "postgres-integration-tests")]

use super::{receive, Config};
use crate::person_collaboration::{
    store::business_events::{PaymentEvent, PaymentHead, PaymentPage},
    Store,
};
use anyhow::{ensure, Result};
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use sqlx::Row;
use std::{
    net::Ipv4Addr,
    sync::{Arc, Mutex},
    time::Duration,
};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use uuid::Uuid;

fn signed_current(event: &PaymentEvent, delivery: &str) -> Result<crate::local_http::Request> {
    let mut request = signed(event, "property_a", delivery)?;
    let timestamp = Utc::now().timestamp().to_string();
    let mut mac = Hmac::<Sha256>::new_from_slice(b"simulated-payment-signing-secret-32")?;
    mac.update(
        format!(
            "POST\n/api/v1/ingress/pms/events\n{timestamp}\n{delivery}\n{}",
            crate::person_collaboration::digest(&request.body)
        )
        .as_bytes(),
    );
    let signature = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect();
    request.headers.insert("x-qt-sent-at".into(), timestamp);
    request.headers.insert("x-qt-signature".into(), signature);
    Ok(request)
}

async fn through_listener(
    store: Arc<Store>,
    config: Arc<Config>,
    wake: Option<Arc<super::production_events::WakeConfig>>,
    signed: crate::local_http::Request,
) -> Result<(u16, Value, tokio::task::JoinHandle<Result<()>>)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let port = listener.local_addr()?.port();
    let server = tokio::spawn(async move {
        let (mut stream, _) = listener.accept().await?;
        super::production_events::handle(&mut stream, &store, &config, port, wake.as_deref()).await
    });
    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await?;
    let mut wire = format!(
        "{} {} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Length: {}\r\n",
        signed.method,
        signed.path,
        signed.body.len()
    )
    .into_bytes();
    for (name, value) in signed.headers {
        wire.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    wire.extend_from_slice(b"\r\n");
    wire.extend_from_slice(&signed.body);
    client.write_all(&wire).await?;
    let mut header = Vec::new();
    let mut byte = [0_u8; 1];
    while !header.ends_with(b"\r\n\r\n") {
        ensure!(header.len() < 8192, "payment_ack_header_too_large");
        client.read_exact(&mut byte).await?;
        header.push(byte[0]);
    }
    let text = std::str::from_utf8(&header)?;
    let status: u16 = text.split_whitespace().nth(1).unwrap().parse()?;
    let length: usize = text
        .lines()
        .find_map(|line| line.strip_prefix("Content-Length: "))
        .unwrap()
        .trim()
        .parse()?;
    let mut body = vec![0; length];
    client.read_exact(&mut body).await?;
    Ok((status, serde_json::from_slice(&body)?, server))
}

#[cfg(unix)]
async fn official_probe(
    store: Arc<Store>,
    config: Arc<Config>,
    gateway: &str,
    script: &str,
) -> Result<()> {
    use std::{
        io::{BufRead, BufReader},
        os::unix::fs::PermissionsExt,
        process::{Command, Stdio},
    };
    let pool = store.pool.clone();
    let tenant = &store.tenant;
    let namespace = &store.identity_namespace;
    let binding = config.binding;
    let python = std::env::var("ANAN_HERMES_PYTHON")?;
    let source = std::env::var("ANAN_HERMES_SOURCE")?;
    ensure!(
        std::path::Path::new(script).is_absolute()
            && std::path::Path::new(&python).is_absolute()
            && std::path::Path::new(&source).is_absolute(),
        "official_probe_paths_required"
    );
    let socket_dir = tempfile::tempdir()?;
    let socket = socket_dir.path().join("foundation.sock");
    let evidence_dir = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .join(".local-workspace/anan/production");
    let private_dir = tempfile::Builder::new()
        .prefix("joint-")
        .tempdir_in(evidence_dir)?;
    let credentials_file = private_dir.path().canonicalize()?.join(".env");
    let wake_secret = b"simulated-independent-webhook-secret-32";
    let model_token = format!("simulated-model-{}", Uuid::new_v4().simple());
    let host_token = format!("simulated-host-{}", Uuid::new_v4().simple());
    let pms_token = format!("simulated-pms-{}", Uuid::new_v4().simple());
    std::fs::write(
        &credentials_file,
        serde_json::to_vec(&json!({
            "GREENPMS_API_TOKEN":pms_token,
            "QINTOPIA_FOUNDATION_TOKEN":model_token,
            "QINTOPIA_FOUNDATION_HOST_TOKEN":host_token,
        }))?,
    )?;
    std::fs::set_permissions(&credentials_file, std::fs::Permissions::from_mode(0o600))?;
    for (key, value) in [
        ("QINTOPIA_FOUNDATION_SOCKET", socket.display().to_string()),
        ("QINTOPIA_FOUNDATION_TOKEN", model_token.clone()),
        ("QINTOPIA_FOUNDATION_HOST_TOKEN", host_token.clone()),
        ("QINTOPIA_FOUNDATION_GATEWAY_ID", gateway.to_string()),
        ("QINTOPIA_FOUNDATION_PROFILE", "anan".into()),
        ("QINTOPIA_FOUNDATION_PRODUCTION_ENABLE", "1".into()),
        ("QINTOPIA_PMS_EVENTS_PRODUCTION_ENABLE", "1".into()),
        ("QINTOPIA_PMS_EVENT_BINDING", binding.to_string()),
    ] {
        std::env::set_var(key, value);
    }
    std::env::remove_var("QINTOPIA_FOUNDATION_LOCAL_ENABLE");
    std::env::remove_var("QINTOPIA_PMS_EVENTS_LOCAL_ENABLE");
    let broker_store = Store::live(pool.clone(), tenant, namespace).await?;
    let broker = tokio::spawn(super::super::foundation_server::broker_live(broker_store));
    let result: Result<()> = async {
        tokio::time::timeout(Duration::from_secs(3), async {
            while !socket.exists() {
                ensure!(!broker.is_finished(), "official_probe_broker_unavailable");
                tokio::time::sleep(Duration::from_millis(20)).await;
            }
            Ok::<_, anyhow::Error>(())
        })
        .await??;
        let mut child = Command::new(python)
            .arg(script)
            .env("ANAN_HERMES_SOURCE", source)
            .env("ANAN_WORKITEM_JOINT_SIMULATE", "1")
            .env(
                "ANAN_WORKITEM_SIMULATED_WEBHOOK_SECRET",
                std::str::from_utf8(wake_secret)?,
            )
            .env("QINTOPIA_PMS_CREDENTIALS_FILE", &credentials_file)
            .env_remove("GREENPMS_API_TOKEN")
            .env_remove("QINTOPIA_FOUNDATION_TOKEN")
            .env_remove("QINTOPIA_FOUNDATION_HOST_TOKEN")
            .stdout(Stdio::piped())
            .stderr(Stdio::null())
            .spawn()?;
        let outcome: Result<()> = async {
            let output = child
                .stdout
                .take()
                .ok_or_else(|| anyhow::anyhow!("official_probe_stdout_unavailable"))?;
            let (reader, ready) = tokio::time::timeout(
                Duration::from_secs(10),
                tokio::task::spawn_blocking(move || {
                    let mut reader = BufReader::new(output);
                    let mut line = String::new();
                    reader.read_line(&mut line)?;
                    Ok::<_, anyhow::Error>((reader, line))
                }),
            )
            .await???;
            let ready: Value = serde_json::from_str(&ready)?;
            ensure!(ready["ready"] == true, "official_probe_not_ready");
            let port = ready["port"]
                .as_u64()
                .and_then(|v| u16::try_from(v).ok())
                .filter(|v| *v != 0)
                .ok_or_else(|| anyhow::anyhow!("official_probe_invalid_port"))?;
            let wake = Arc::new(super::production_events::WakeConfig {
                port,
                gateway: gateway.to_string(),
                secret: zeroize::Zeroizing::new(wake_secret.to_vec()),
            });
            let event = payment(43, "official_probe_bill", "COLLECTION", "DISCOVERED");
            let (status, ack, task) = through_listener(
                store,
                config,
                Some(wake),
                signed_current(&event, "official-probe")?,
            )
            .await?;
            ensure!(
                status == 202 && ack["status"] == "accepted",
                "official_probe_ack_failed"
            );
            let receipt: Uuid = serde_json::from_value(ack["receipt_id"].clone())?;
            let work: Uuid = sqlx::query_scalar(
                "SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE id=$1",
            )
            .bind(receipt)
            .fetch_one(&pool)
            .await?;
            task.await??;
            let (_reader, result) = tokio::time::timeout(
                Duration::from_secs(10),
                tokio::task::spawn_blocking(move || {
                    let mut reader = reader;
                    let mut line = String::new();
                    reader.read_line(&mut line)?;
                    Ok::<_, anyhow::Error>((reader, line))
                }),
            )
            .await???;
            let result: Value = serde_json::from_str(&result)?;
            ensure!(
                result["ok"] == true
                    && result["work_item_id"] == work.to_string()
                    && result["projection"]["work_item_id"] == work.to_string()
                    && result["projection"]["kind"] == "payment"
                    && result["projection"]["status"] == "awaiting_review"
                    && result["projection"]["readback_required"] == true,
                "official_probe_readback_mismatch"
            );
            tokio::time::timeout(Duration::from_secs(3), async {
                while child.try_wait()?.is_none() {
                    tokio::time::sleep(Duration::from_millis(20)).await;
                }
                Ok::<_, anyhow::Error>(())
            })
            .await??;
            ensure!(
                child.try_wait()?.is_some_and(|status| status.success()),
                "official_probe_failed"
            );
            Ok(())
        }
        .await;
        let _ = child.kill();
        let _ = child.wait();
        outcome
    }
    .await;
    broker.abort();
    let _ = broker.await;
    result
}

#[tokio::test]
#[ignore = "explicit task-isolated PostgreSQL and loopback listener required"]
async fn production_payment_postcommit_wake_preserves_ack_and_scope() -> Result<()> {
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let setup = Store::local(
        &database,
        &format!("synthetic-collaboration-wake-{}", Uuid::new_v4()),
    )
    .await?;
    crate::db::run_migrations(&setup.pool).await?;
    let pool = setup.pool.clone();
    let tenant = format!("live-wake-{}", Uuid::new_v4());
    let namespace = format!("live-wake-identity-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_tenants(tenant_key,identity_namespace,mode,initialized) VALUES($1,$2,'live',true)")
        .bind(&tenant).bind(&namespace).execute(&pool).await?;
    let scope: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,label,kind) VALUES($1,'模拟客房','community') RETURNING id")
        .bind(&tenant).fetch_one(&pool).await?;
    let gateway = format!("wake-gateway-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'wecom_internal',$4,'employee',true)")
        .bind(&tenant).bind(&gateway).bind(&namespace).bind(scope).execute(&pool).await?;
    let binding: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) VALUES($1,$2,'simulated-pms','property_a') RETURNING id")
        .bind(&tenant).bind(scope).fetch_one(&pool).await?;
    let conversation: Uuid = sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type) VALUES($1,'wecom',$2,'group') RETURNING id")
        .bind(&tenant).bind(format!("wake-group-{tenant}")).fetch_one(&pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3)")
        .bind(&tenant).bind(scope).bind(conversation).execute(&pool).await?;
    let store = Arc::new(Store::live(pool.clone(), &tenant, &namespace).await?);
    let head = PaymentHead {
        schema_version: "pms.payments.v1".into(),
        source_instance: "simulated-pms".into(),
        property_id: "property_a".into(),
        binding_version: 1,
        head_cursor: "42".into(),
    };
    store.business_feed_open(binding, Some(&head)).await?;
    let config = Arc::new(Config {
        binding,
        key: crate::resident_welcome::protocol::SigningKey {
            key_id: "test".into(),
            secret: zeroize::Zeroizing::new(b"simulated-payment-signing-secret-32".to_vec()),
            source_instance: "simulated-pms".into(),
            properties: ["property_a".into()].into_iter().collect(),
        },
    });
    if let Ok(script) = std::env::var("ANAN_WORKITEM_WAKE_PROBE") {
        return official_probe(store, config, &gateway, &script).await;
    }
    let webhook = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let webhook_port = webhook.local_addr()?.port();
    let wake = Arc::new(super::production_events::WakeConfig {
        port: webhook_port,
        gateway: gateway.clone(),
        secret: zeroize::Zeroizing::new(b"independent-simulated-webhook-secret-32".to_vec()),
    });
    let current = payment(43, "wake_bill", "COLLECTION", "DISCOVERED");
    let (status, ack, task) = through_listener(
        store.clone(),
        config.clone(),
        Some(wake.clone()),
        signed_current(&current, "wake-one")?,
    )
    .await?;
    assert_eq!(status, 202);
    assert_eq!(ack["status"], "accepted");
    let receipt: Uuid = serde_json::from_value(ack["receipt_id"].clone())?;
    let work: Uuid = sqlx::query_scalar(
        "SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE id=$1",
    )
    .bind(receipt)
    .fetch_one(&pool)
    .await?;
    let prior_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id=$1",
    )
    .bind(work)
    .fetch_one(&pool)
    .await?;
    let (mut called, _) = tokio::time::timeout(Duration::from_secs(1), webhook.accept()).await??;
    let request = crate::local_http::request(&mut called, webhook_port).await?;
    assert_eq!(request.method, "POST");
    assert_eq!(request.path, "/webhooks/anan-workitem-wake");
    let expected = format!(r#"{{"schema_version":1,"work_item_id":"{work}"}}"#);
    assert_eq!(request.body, expected.as_bytes());
    assert_eq!(request.headers["x-request-id"], work.to_string());
    assert_eq!(request.headers["content-type"], "application/json");
    let timestamp = &request.headers["x-webhook-timestamp"];
    let mut mac = Hmac::<Sha256>::new_from_slice(&wake.secret)?;
    mac.update(format!("{timestamp}.{expected}").as_bytes());
    let signature = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    assert_eq!(request.headers["x-webhook-signature-v2"], signature);
    crate::local_http::respond(
        &mut called,
        202,
        "application/json",
        serde_json::to_string(&json!({"status":"accepted","delivery_id":work}))?.as_bytes(),
        None,
    )
    .await?;
    drop(called);
    task.await??;
    let after_events: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id=$1",
    )
    .bind(work)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        after_events, prior_events,
        "wake must not modify the work item"
    );

    for (event, delivery, enabled) in [
        (current.clone(), "wake-duplicate", true),
        (
            payment(42, "old_bill", "COLLECTION", "DISCOVERED"),
            "wake-historical",
            true,
        ),
        (
            payment(44, "refund_bill", "REFUND", "DISCOVERED"),
            "wake-refund",
            true,
        ),
        (
            payment(45, "wake_bill", "COLLECTION", "MATCHED"),
            "wake-matched",
            true,
        ),
        (
            payment(46, "disabled_bill", "COLLECTION", "DISCOVERED"),
            "wake-disabled",
            false,
        ),
    ] {
        let (status, _, task) = through_listener(
            store.clone(),
            config.clone(),
            enabled.then(|| wake.clone()),
            signed_current(&event, delivery)?,
        )
        .await?;
        assert!(matches!(status, 200 | 202));
        task.await??;
        assert!(
            tokio::time::timeout(Duration::from_millis(50), webhook.accept())
                .await
                .is_err()
        );
    }

    let failed = payment(47, "failed_hint", "COLLECTION", "DISCOVERED");
    let (status, ack, task) = through_listener(
        store.clone(),
        config.clone(),
        Some(wake.clone()),
        signed_current(&failed, "wake-failed")?,
    )
    .await?;
    assert_eq!(status, 202);
    let failed_receipt: Uuid = serde_json::from_value(ack["receipt_id"].clone())?;
    let (mut called, _) = tokio::time::timeout(Duration::from_secs(1), webhook.accept()).await??;
    let _ = crate::local_http::request(&mut called, webhook_port).await?;
    crate::local_http::respond(
        &mut called,
        500,
        "application/json",
        b"{\"status\":\"error\"}",
        None,
    )
    .await?;
    drop(called);
    task.await??;
    let persisted: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_event_inbox WHERE id=$1 AND work_item_id IS NOT NULL)")
        .bind(failed_receipt).fetch_one(&pool).await?;
    assert!(persisted, "rejected hint cannot retract the PMS receipt");

    let duplicate_hint = payment(48, "duplicate_hint", "COLLECTION", "DISCOVERED");
    let (status, _, task) = through_listener(
        store.clone(),
        config.clone(),
        Some(wake.clone()),
        signed_current(&duplicate_hint, "wake-hint-duplicate")?,
    )
    .await?;
    assert_eq!(status, 202);
    let (mut called, _) = tokio::time::timeout(Duration::from_secs(1), webhook.accept()).await??;
    let request = crate::local_http::request(&mut called, webhook_port).await?;
    let duplicate_work = request.headers["x-request-id"].clone();
    crate::local_http::respond(
        &mut called,
        200,
        "application/json",
        serde_json::to_string(&json!({"status":"duplicate","delivery_id":duplicate_work}))?
            .as_bytes(),
        None,
    )
    .await?;
    drop(called);
    task.await??;

    let invalid_hint = payment(49, "invalid_hint", "COLLECTION", "DISCOVERED");
    let (status, invalid_ack, task) = through_listener(
        store.clone(),
        config.clone(),
        Some(wake.clone()),
        signed_current(&invalid_hint, "wake-hint-invalid")?,
    )
    .await?;
    assert_eq!(status, 202);
    let (mut called, _) = tokio::time::timeout(Duration::from_secs(1), webhook.accept()).await??;
    let request = crate::local_http::request(&mut called, webhook_port).await?;
    crate::local_http::respond(
        &mut called,
        202,
        "application/json",
        serde_json::to_string(
            &json!({"status":"duplicate","delivery_id":request.headers["x-request-id"]}),
        )?
        .as_bytes(),
        None,
    )
    .await?;
    drop(called);
    task.await??;
    let invalid_receipt: Uuid = serde_json::from_value(invalid_ack["receipt_id"].clone())?;
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_event_inbox WHERE id=$1)"
        )
        .bind(invalid_receipt)
        .fetch_one(&pool)
        .await?
    );

    let timed_out = payment(50, "timeout_hint", "COLLECTION", "DISCOVERED");
    let (status, timeout_ack, task) = through_listener(
        store.clone(),
        config.clone(),
        Some(wake.clone()),
        signed_current(&timed_out, "wake-hint-timeout")?,
    )
    .await?;
    assert_eq!(status, 202);
    let (called, _) = tokio::time::timeout(Duration::from_secs(1), webhook.accept()).await??;
    tokio::time::timeout(Duration::from_secs(3), task).await???;
    drop(called);
    let timeout_receipt: Uuid = serde_json::from_value(timeout_ack["receipt_id"].clone())?;
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_event_inbox WHERE id=$1)"
        )
        .bind(timeout_receipt)
        .fetch_one(&pool)
        .await?
    );

    let failed_work: Uuid = sqlx::query_scalar(
        "SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE id=$1",
    )
    .bind(failed_receipt)
    .fetch_one(&pool)
    .await?;
    assert_eq!(
        super::production_events::eligible_work(&store, &config, &wake, failed_receipt).await?,
        Some(failed_work)
    );
    sqlx::query("UPDATE qintopia_identity.person_identity_gateways SET active=false WHERE tenant_key=$1 AND gateway_key=$2")
        .bind(&tenant).bind(&gateway).execute(&pool).await?;
    assert!(store
        .business_payment_workitem_read(&gateway, binding, work)
        .await
        .is_err());
    assert!(
        super::production_events::eligible_work(&store, &config, &wake, failed_receipt)
            .await
            .is_err()
    );
    sqlx::query("UPDATE qintopia_identity.person_identity_gateways SET active=true WHERE tenant_key=$1 AND gateway_key=$2")
        .bind(&tenant).bind(&gateway).execute(&pool).await?;
    sqlx::query("UPDATE qintopia_agent_os.business_property_bindings SET active=false WHERE id=$1")
        .bind(binding)
        .execute(&pool)
        .await?;
    assert!(
        super::production_events::eligible_work(&store, &config, &wake, failed_receipt)
            .await?
            .is_none()
    );
    Ok(())
}

static PAYMENT_ENV_LOCK: Mutex<()> = Mutex::new(());

#[test]
#[cfg(unix)]
#[ignore = "isolated child process for payment configuration environment"]
fn production_config_rejects_mixed_modes_and_insecure_key() -> Result<()> {
    const CHILD: &str = "QINTOPIA_PAYMENT_CONFIG_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let output = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "person_collaboration::business_ingress::production_events_tests::production_config_rejects_mixed_modes_and_insecure_key",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .output()?;
        ensure!(
            output.status.success() && String::from_utf8_lossy(&output.stdout).contains("1 passed"),
            "payment_config_child_failed"
        );
        return Ok(());
    }
    let _guard = PAYMENT_ENV_LOCK.lock().unwrap();
    struct Restore(Vec<(&'static str, Option<std::ffi::OsString>)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                if let Some(value) = value {
                    std::env::set_var(key, value);
                } else {
                    std::env::remove_var(key);
                }
            }
        }
    }
    let dir = tempfile::tempdir()?;
    let key_file = dir.path().join("payment.key");
    std::fs::write(&key_file, b"simulated-payment-signing-secret-32")?;
    use std::os::unix::fs::PermissionsExt;
    std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o600))?;
    let settings = [
        ("QINTOPIA_PMS_EVENTS_PRODUCTION_ENABLE", "1".to_string()),
        ("QINTOPIA_FOUNDATION_PRODUCTION_ENABLE", "1".to_string()),
        ("QINTOPIA_PMS_EVENTS_LOCAL_ENABLE", "0".to_string()),
        ("QINTOPIA_FOUNDATION_LOCAL_ENABLE", "0".to_string()),
        ("QINTOPIA_PMS_EVENT_BINDING", Uuid::new_v4().to_string()),
        ("QINTOPIA_PMS_EVENT_SOURCE", "simulated-pms".to_string()),
        ("QINTOPIA_PMS_EVENT_PROPERTY", "property_a".to_string()),
        ("QINTOPIA_PMS_EVENT_KEY_ID", "test".to_string()),
        (
            "QINTOPIA_PMS_EVENT_KEY_FILE",
            key_file.display().to_string(),
        ),
    ];
    let mut restore = Restore(Vec::new());
    for (key, value) in settings {
        restore.0.push((key, std::env::var_os(key)));
        std::env::set_var(key, value);
    }
    assert!(Config::production().is_ok());
    assert!(
        super::production_events::WakeConfig::from_environment(&Config::production()?)?.is_none()
    );
    let wake_key = dir.path().join("wake.key");
    std::fs::write(&wake_key, b"independent-simulated-webhook-secret-32")?;
    std::fs::set_permissions(&wake_key, std::fs::Permissions::from_mode(0o600))?;
    for (key, value) in [
        ("QINTOPIA_PMS_WORKITEM_WAKE_ENABLE", "1".to_string()),
        ("QINTOPIA_PMS_WORKITEM_WAKE_PORT", "0".to_string()),
        (
            "QINTOPIA_PMS_WORKITEM_WAKE_GATEWAY",
            "simulated-gateway".to_string(),
        ),
        (
            "QINTOPIA_PMS_WORKITEM_WAKE_KEY_FILE",
            wake_key.display().to_string(),
        ),
    ] {
        restore.0.push((key, std::env::var_os(key)));
        std::env::set_var(key, value);
    }
    assert!(
        super::production_events::WakeConfig::from_environment(&Config::production()?).is_err()
    );
    std::env::set_var("QINTOPIA_PMS_WORKITEM_WAKE_PORT", "10001");
    assert!(
        super::production_events::WakeConfig::from_environment(&Config::production()?)?.is_some()
    );
    std::fs::set_permissions(&wake_key, std::fs::Permissions::from_mode(0o644))?;
    assert!(
        super::production_events::WakeConfig::from_environment(&Config::production()?).is_err()
    );
    std::fs::set_permissions(&wake_key, std::fs::Permissions::from_mode(0o600))?;
    std::env::set_var("QINTOPIA_PMS_WORKITEM_WAKE_KEY_FILE", &key_file);
    assert!(
        super::production_events::WakeConfig::from_environment(&Config::production()?).is_err()
    );
    std::env::set_var("QINTOPIA_PMS_EVENTS_LOCAL_ENABLE", "1");
    assert_eq!(
        Config::production().err().unwrap().to_string(),
        "payment_ingress_disabled"
    );
    std::env::set_var("QINTOPIA_FOUNDATION_LOCAL_ENABLE", "1");
    assert_eq!(
        Config::local().err().unwrap().to_string(),
        "payment_ingress_disabled"
    );
    std::env::set_var("QINTOPIA_PMS_EVENTS_LOCAL_ENABLE", "0");
    std::env::set_var("QINTOPIA_FOUNDATION_LOCAL_ENABLE", "0");
    std::fs::set_permissions(&key_file, std::fs::Permissions::from_mode(0o644))?;
    assert_eq!(
        Config::production().err().unwrap().to_string(),
        "invalid_payment_key_file"
    );
    Ok(())
}

fn payment(sequence: i64, bill: &str, kind: &str, event_type: &str) -> PaymentEvent {
    PaymentEvent {
        event_id: format!("payment:{bill}:{event_type}"),
        bill_id: bill.into(),
        kind: kind.into(),
        event_type: event_type.into(),
        occurred_at: Utc::now(),
        sequence: sequence.to_string(),
    }
}

fn signed(
    event: &PaymentEvent,
    property: &str,
    delivery: &str,
) -> Result<crate::local_http::Request> {
    let mut envelope = serde_json::to_value(event)?;
    envelope["schemaVersion"] = json!("pms.payments.v1");
    envelope["sourceInstance"] = json!("simulated-pms");
    envelope["propertyId"] = json!(property);
    let raw = serde_json::to_vec(&envelope)?;
    let mut mac = Hmac::<Sha256>::new_from_slice(b"simulated-payment-signing-secret-32").unwrap();
    mac.update(
        format!(
            "POST\n/api/v1/ingress/pms/events\n1000\n{delivery}\n{}",
            crate::person_collaboration::digest(&raw)
        )
        .as_bytes(),
    );
    let signature = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    Ok(crate::local_http::Request {
        method: "POST".into(),
        path: crate::resident_welcome::protocol::PATH.into(),
        body: raw,
        headers: [
            ("content-type", "application/json".into()),
            ("x-qt-key-id", "test".into()),
            ("x-qt-sent-at", "1000".into()),
            ("x-qt-delivery-id", delivery.into()),
            ("x-qt-signature", signature),
        ]
        .into_iter()
        .map(|(key, value)| (key.into(), value))
        .collect(),
    })
}

async fn signed_over_tcp(
    store: &Store,
    config: &Config,
    signed: &crate::local_http::Request,
    host: Option<&str>,
) -> Result<(u16, Value)> {
    let listener = TcpListener::bind((Ipv4Addr::LOCALHOST, 0)).await?;
    let port = listener.local_addr()?.port();
    let mut client = TcpStream::connect((Ipv4Addr::LOCALHOST, port)).await?;
    let wrong_host = host.is_some();
    let body: &[u8] = if wrong_host {
        &[]
    } else {
        signed.body.as_slice()
    };
    let host = host
        .map(str::to_owned)
        .unwrap_or_else(|| format!("127.0.0.1:{port}"));
    let mut wire = format!(
        "{} {} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\n",
        signed.method,
        signed.path,
        body.len()
    )
    .into_bytes();
    for (name, value) in &signed.headers {
        wire.extend_from_slice(format!("{name}: {value}\r\n").as_bytes());
    }
    wire.extend_from_slice(b"\r\n");
    wire.extend_from_slice(body);
    client.write_all(&wire).await?;
    let (mut server, _) = listener.accept().await?;
    let response = match crate::local_http::request(&mut server, port).await {
        Ok(request) => receive(store, config, &request, 1000).await,
        Err(error) => {
            ensure!(
                wrong_host && error.to_string() == "host_forbidden",
                "unexpected_http_rejection"
            );
            crate::resident_welcome::ingress::Response {
                status: 400,
                body: json!({"code":"invalid_request"}),
            }
        }
    };
    crate::local_http::respond(
        &mut server,
        response.status,
        "application/json",
        &serde_json::to_vec(&response.body)?,
        None,
    )
    .await?;
    drop(server);
    let mut bytes = Vec::new();
    client.read_to_end(&mut bytes).await?;
    let split = bytes
        .windows(4)
        .position(|part| part == b"\r\n\r\n")
        .unwrap()
        + 4;
    let header = std::str::from_utf8(&bytes[..split])?;
    let status = header.split_whitespace().nth(1).unwrap().parse()?;
    Ok((status, serde_json::from_slice(&bytes[split..])?))
}

#[tokio::test]
#[ignore = "explicit task-isolated PostgreSQL required"]
async fn live_payment_http_ack_and_feed_preserve_baseline_and_review_only() -> Result<()> {
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let setup = Store::local(
        &database,
        &format!("synthetic-collaboration-payment-{}", Uuid::new_v4()),
    )
    .await?;
    crate::db::run_migrations(&setup.pool).await?;
    let tenant = format!("live-payment-{}", Uuid::new_v4());
    let namespace = format!("live-payment-identity-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_tenants(tenant_key,identity_namespace,mode,initialized) VALUES($1,$2,'live',true)")
        .bind(&tenant).bind(&namespace).execute(&setup.pool).await?;
    let scope: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,label,kind) VALUES($1,'模拟社区','community') RETURNING id")
        .bind(&tenant).fetch_one(&setup.pool).await?;
    let binding: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) VALUES($1,$2,'simulated-pms','property_a') RETURNING id")
        .bind(&tenant).bind(scope).fetch_one(&setup.pool).await?;
    let store = Store::live(setup.pool.clone(), &tenant, &namespace).await?;
    let config = Config {
        binding,
        key: crate::resident_welcome::protocol::SigningKey {
            key_id: "test".into(),
            secret: zeroize::Zeroizing::new(b"simulated-payment-signing-secret-32".to_vec()),
            source_instance: "simulated-pms".into(),
            properties: ["property_a".into()].into_iter().collect(),
        },
    };
    let current = payment(43, "new_bill", "COLLECTION", "DISCOVERED");
    assert_eq!(
        receive(
            &store,
            &config,
            &signed(&current, "property_a", "before")?,
            1000
        )
        .await
        .status,
        503
    );
    let head = PaymentHead {
        schema_version: "pms.payments.v1".into(),
        source_instance: "simulated-pms".into(),
        property_id: "property_a".into(),
        binding_version: 1,
        head_cursor: "42".into(),
    };
    assert_eq!(
        store.business_feed_open(binding, Some(&head)).await?["cursor"],
        "42"
    );
    assert!(store
        .business_feed_open(
            binding,
            Some(&PaymentHead {
                head_cursor: "43".into(),
                ..head
            })
        )
        .await
        .is_err());
    let historical = payment(42, "old_bill", "COLLECTION", "DISCOVERED");
    assert_eq!(
        receive(
            &store,
            &config,
            &signed(&historical, "property_a", "old")?,
            1000
        )
        .await
        .status,
        202
    );
    assert_eq!(
        receive(&store, &config, &signed(&current, "other", "wrong")?, 1000)
            .await
            .status,
        403
    );
    let accepted = receive(
        &store,
        &config,
        &signed(&current, "property_a", "one")?,
        1000,
    )
    .await;
    assert_eq!(accepted.status, 202);
    let receipt: Uuid = serde_json::from_value(accepted.body["receipt_id"].clone())?;
    let row = sqlx::query("SELECT w.purpose,w.metadata FROM qintopia_agent_os.business_event_inbox i JOIN qintopia_agent_os.work_items w ON w.id=i.work_item_id WHERE i.id=$1")
        .bind(receipt).fetch_one(&store.pool).await?;
    assert_eq!(row.get::<String, _>("purpose"), "hospitality_business");
    let metadata: serde_json::Value = row.get("metadata");
    assert_eq!(metadata["mode"], "live");
    assert_eq!(metadata["event_is_not_authority"], true);
    assert!(metadata.get("local_only").is_none());
    let duplicate = receive(
        &store,
        &config,
        &signed(&current, "property_a", "again")?,
        1000,
    )
    .await;
    assert_eq!(duplicate.status, 200);
    assert_eq!(duplicate.body["receipt_id"], accepted.body["receipt_id"]);
    let mut changed = current.clone();
    changed.kind = "REFUND".into();
    assert_eq!(
        receive(
            &store,
            &config,
            &signed(&changed, "property_a", "conflict")?,
            1000
        )
        .await
        .status,
        409
    );
    assert_eq!(
        store.business_feed_context(binding).await?["state"]["cursor"],
        "42"
    );
    let gap = PaymentPage {
        schema_version: "pms.payments.v1".into(),
        property_id: "property_a".into(),
        events: vec![
            current.clone(),
            payment(45, "gap", "COLLECTION", "DISCOVERED"),
        ],
        next_cursor: "45".into(),
    };
    assert!(store
        .business_feed_page(binding, "simulated-pms", "42", gap)
        .await
        .is_err());
    assert_eq!(
        store.business_feed_context(binding).await?["state"]["cursor"],
        "42"
    );
    let page = PaymentPage {
        schema_version: "pms.payments.v1".into(),
        property_id: "property_a".into(),
        events: vec![current],
        next_cursor: "43".into(),
    };
    let result = store
        .business_feed_page(binding, "simulated-pms", "42", page)
        .await?;
    assert_eq!(result["cursor"], "43");
    assert_eq!(
        result["receipts"][0]["receipt_id"],
        accepted.body["receipt_id"]
    );
    let work_count: i64 = sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND work_item_id IS NOT NULL")
        .bind(&tenant).fetch_one(&store.pool).await?;
    assert_eq!(work_count, 1);
    let tcp_event = payment(44, "tcp_bill", "COLLECTION", "DISCOVERED");
    let (accepted_status, accepted_body) = signed_over_tcp(
        &store,
        &config,
        &signed(&tcp_event, "property_a", "tcp-one")?,
        None,
    )
    .await?;
    assert_eq!(accepted_status, 202);
    let tcp_receipt: Uuid = serde_json::from_value(accepted_body["receipt_id"].clone())?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_event_inbox WHERE id=$1)",
    )
    .bind(tcp_receipt)
    .fetch_one(&store.pool)
    .await?;
    assert!(exists);
    let (duplicate_status, duplicate_body) = signed_over_tcp(
        &store,
        &config,
        &signed(&tcp_event, "property_a", "tcp-two")?,
        None,
    )
    .await?;
    assert_eq!(duplicate_status, 200);
    assert_eq!(duplicate_body["receipt_id"], accepted_body["receipt_id"]);
    let rejected = signed_over_tcp(
        &store,
        &config,
        &signed(
            &payment(45, "bad_host", "COLLECTION", "DISCOVERED"),
            "property_a",
            "tcp-host",
        )?,
        Some("pms.qintopia.cn"),
    )
    .await?;
    assert_eq!(rejected.0, 400);
    assert_eq!(rejected.1["code"], "invalid_request");
    let denied_count: i64 = sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND subject_ref='bad_host'")
        .bind(&tenant).fetch_one(&store.pool).await?;
    assert_eq!(denied_count, 0);
    Ok(())
}
