//! Explicit development listener. No ambient production config, migration or
//! source registration, and no external provider calls.
use super::{protocol, store::Store, workbench};
use crate::local_http::{request, respond};
use anyhow::{ensure, Result};
use chrono::Utc;
use serde_json::{json, Value};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};
use uuid::Uuid;

pub async fn run(port: u16) -> Result<()> {
    ensure!(
        std::env::var("QINTOPIA_WELCOME_LOCAL_ENABLE").as_deref() == Ok("1"),
        "welcome local disabled"
    );
    let database = std::env::var("QINTOPIA_WELCOME_LOCAL_DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("explicit local database required"))?;
    let source = std::env::var("QINTOPIA_WELCOME_LOCAL_SOURCE")?;
    let property = std::env::var("QINTOPIA_WELCOME_LOCAL_PROPERTY")?;
    let operator = Uuid::parse_str(&std::env::var("QINTOPIA_WELCOME_LOCAL_OPERATOR_LINK")?)?;
    let store = Store::local(&database).await?;
    let mode:String=sqlx::query_scalar("SELECT mode FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2")
        .bind(&source).bind(&property).fetch_one(&store.pool).await?;
    ensure!(
        mode == "synthetic",
        "local_server_requires_synthetic_source"
    );
    // The operator is chosen by the local process owner, never browser JSON.
    store
        .actor_from_verified_identity(operator, &source, &property, true)
        .await?;
    let key = protocol::SigningKey {
        key_id: "local-synthetic".into(),
        secret: zeroize::Zeroizing::new(
            std::env::var("QINTOPIA_WELCOME_LOCAL_SIGNING_KEY")?.into_bytes(),
        ),
        source_instance: source.clone(),
        properties: [property.clone()].into_iter().collect(),
    };
    ensure!(key.secret.len() >= 32, "local signing key too short");
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let csrf = Uuid::new_v4().to_string();
    // Print only the loopback UI URL; never log requests, cookies or source IDs.
    println!("Welcome local workbench: http://127.0.0.1:{port}/");
    let mut recovery = tokio::time::interval(std::time::Duration::from_secs(15));
    loop {
        let (mut stream, peer) = tokio::select! {
            accepted = listener.accept() => accepted?,
            _ = recovery.tick() => {
                // Recover committed state even when its original wakeup was lost.
                // This listener has no external adapters and cannot execute actions.
                if store.recover_sending(&source,&property).await.is_err() || store.reconcile_scope(&source,&property).await.is_err() {
                    eprintln!("welcome_local_reconciliation_deferred");
                }
                continue;
            }
        };
        if !peer.ip().is_loopback() {
            continue;
        }
        let actor = store
            .actor_from_verified_identity(operator, &source, &property, true)
            .await?;
        let result = tokio::time::timeout(
            std::time::Duration::from_secs(5),
            handle(
                &mut stream,
                &store,
                &actor,
                port,
                &csrf,
                std::slice::from_ref(&key),
            ),
        )
        .await;
        if !matches!(result, Ok(Ok(()))) {
            let _ = stream.shutdown().await;
        }
    }
}

async fn handle(
    stream: &mut TcpStream,
    store: &Store,
    actor: &super::store::Actor,
    port: u16,
    csrf: &str,
    keys: &[protocol::SigningKey],
) -> Result<()> {
    let request = match request(stream, port).await {
        Ok(r) => r,
        Err(_) => {
            return respond(
                stream,
                400,
                "application/json",
                br#"{"code":"invalid_request"}"#,
                None,
            )
            .await
        }
    };
    if request.method == "POST" && request.path == protocol::PATH {
        let r = super::ingress::receive(
            store,
            &request.method,
            &request.path,
            &request.headers,
            &request.body,
            keys,
            Utc::now().timestamp(),
        )
        .await;
        return respond(
            stream,
            r.status,
            "application/json",
            &serde_json::to_vec(&r.body)?,
            None,
        )
        .await;
    }
    if request.method == "GET" && request.path == "/" {
        return respond(
            stream,
            200,
            "text/html; charset=utf-8",
            workbench::HTML.as_bytes(),
            Some(("welcome-local", csrf)),
        )
        .await;
    }
    let cookie = request.headers.get("cookie").is_some_and(|s| {
        s.split(';')
            .any(|pair| pair.trim() == format!("welcome-local={csrf}"))
    });
    let origin = request
        .headers
        .get("origin")
        .is_some_and(|s| s == &format!("http://127.0.0.1:{port}"));
    if !cookie || (request.method == "POST" && !origin) {
        return respond(
            stream,
            403,
            "application/json",
            br#"{"code":"local_session_required"}"#,
            None,
        )
        .await;
    }
    let result: Result<Value> = async {
        match (request.method.as_str(), request.path.as_str()) {
            ("GET", "/welcome/local/state") => store.workbench_state(actor).await,
            ("POST", "/welcome/local/bind") => {
                let r = serde_json::from_value(protocol::parse_unique(&request.body)?)?;
                store.workbench_bind(actor, &r).await
            }
            ("POST", "/welcome/local/identity") => {
                let r = serde_json::from_value(protocol::parse_unique(&request.body)?)?;
                store.workbench_identity(actor, &r).await
            }
            ("POST", "/welcome/local/approve") => {
                let r = serde_json::from_value(protocol::parse_unique(&request.body)?)?;
                store.workbench_approve(actor, &r).await
            }
            ("GET", path) if path.starts_with("/welcome/local/people?") => {
                let url = url::Url::parse(&format!("http://127.0.0.1:{port}{path}"))?;
                let query = url
                    .query_pairs()
                    .find(|(k, _)| k == "query")
                    .map(|(_, v)| v.to_string())
                    .unwrap_or_default();
                Ok(Value::Array(
                    store
                        .search_people(actor, &query, 20)
                        .await?
                        .into_iter()
                        .map(|p| json!({"ref":p["person_ref"],"label":p["preferred_name"]}))
                        .collect(),
                ))
            }
            _ => anyhow::bail!("route_not_found"),
        }
    }
    .await;
    match result {
        Ok(value) => {
            respond(
                stream,
                200,
                "application/json",
                &serde_json::to_vec(&value)?,
                None,
            )
            .await
        }
        Err(_) => {
            respond(
                stream,
                409,
                "application/json",
                br#"{"code":"operation_not_authorized_or_state_changed"}"#,
                None,
            )
            .await
        }
    }
}
