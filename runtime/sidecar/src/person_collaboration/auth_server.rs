//! Password entrypoint, intentionally restricted to the synthetic loopback Store.
use super::{
    store::{AccountCommand, Credentials},
    Store,
};
use crate::local_http::{request, respond};
use anyhow::{ensure, Result};
use serde_json::json;
use tokio::net::TcpStream;
use uuid::Uuid;
use zeroize::Zeroizing;

pub async fn bootstrap(person: Uuid, username: &str) -> Result<()> {
    use std::io::Read;
    ensure!(
        std::env::var("QINTOPIA_COLLABORATION_LOCAL_ENABLE").as_deref() == Ok("1"),
        "collaboration_local_disabled"
    );
    let db = std::env::var("QINTOPIA_COLLABORATION_LOCAL_DATABASE_URL")?;
    let tenant = std::env::var("QINTOPIA_COLLABORATION_LOCAL_TENANT")?;
    let store = Store::local(&db, &tenant).await?;
    let mut password = Zeroizing::new(String::new());
    std::io::stdin().take(1024).read_to_string(&mut password)?;
    ensure!(password.len() < 1024, "password_length");
    store
        .bootstrap_account(person, username, password.trim_end_matches(['\r', '\n']))
        .await?;
    println!("Account initialized; no business permissions added.");
    Ok(())
}

pub(super) async fn handle(stream: &mut TcpStream, store: &Store, port: u16) -> Result<()> {
    let mut r = match request(stream, port).await {
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
    // Exact Origin + strict cookies + JSON-only writes; includes login CSRF protection.
    if r.method == "POST"
        && (r.headers.get("origin") != Some(&format!("http://127.0.0.1:{port}"))
            || r.headers.get("content-type").map(String::as_str) != Some("application/json"))
    {
        return respond(
            stream,
            403,
            "application/json",
            br#"{"code":"cross_site_request_denied"}"#,
            None,
        )
        .await;
    }
    if r.method == "GET" {
        let asset = match r.path.as_str() {
            "/login" => Some(("text/html; charset=utf-8", include_str!("login.html"))),
            "/account" => Some(("text/html; charset=utf-8", include_str!("account.html"))),
            "/auth.js" => Some(("text/javascript; charset=utf-8", include_str!("auth.js"))),
            "/auth.css" => Some(("text/css; charset=utf-8", include_str!("auth.css"))),
            "/workbench.css" => Some(("text/css; charset=utf-8", include_str!("workbench.css"))),
            "/workbench.js" => Some((
                "text/javascript; charset=utf-8",
                include_str!("workbench.js"),
            )),
            "/workbench-view.js" => Some((
                "text/javascript; charset=utf-8",
                include_str!("workbench-view.js"),
            )),
            "/workbench-catalog.js" => Some((
                "text/javascript; charset=utf-8",
                include_str!("workbench-catalog.js"),
            )),
            "/workbench-ontology.js" => Some((
                "text/javascript; charset=utf-8",
                include_str!("workbench-ontology.js"),
            )),
            "/workbench-steward.js" => Some((
                "text/javascript; charset=utf-8",
                include_str!("workbench-steward.js"),
            )),
            "/workbench-organization.js" => Some((
                "text/javascript; charset=utf-8",
                include_str!("workbench-organization.js"),
            )),
            _ => None,
        };
        if let Some((mime, body)) = asset {
            return respond(stream, 200, mime, body.as_bytes(), None).await;
        }
    }
    if r.method == "POST" && r.path == "/api/login" {
        let credentials = serde_json::from_slice::<Credentials>(&r.body);
        use zeroize::Zeroize;
        r.body.zeroize();
        let result = match credentials {
            Ok(c) => store.login(&c).await,
            Err(_) => Err(anyhow::anyhow!("invalid_request")),
        };
        return match result {
            Ok(token) => {
                respond(
                    stream,
                    200,
                    "application/json",
                    br#"{"ok":true}"#,
                    Some(("collaboration-session", &token)),
                )
                .await
            }
            Err(e) => failure(stream, &e).await,
        };
    }
    let tokens: Vec<_> = r
        .headers
        .get("cookie")
        .into_iter()
        .flat_map(|s| s.split(';'))
        .filter_map(|s| s.trim().strip_prefix("collaboration-session="))
        .collect();
    let actor = if tokens.len() == 1 {
        store.session_actor(tokens[0]).await
    } else {
        Err(anyhow::anyhow!("authentication_required"))
    };
    let actor = match actor {
        Ok(a) => a,
        Err(_) => {
            if r.method == "GET" && matches!(r.path.as_str(), "/" | "/foundation") {
                return respond(
                    stream,
                    200,
                    "text/html; charset=utf-8",
                    include_bytes!("login.html"),
                    None,
                )
                .await;
            }
            return respond(
                stream,
                401,
                "application/json",
                br#"{"code":"authentication_required"}"#,
                None,
            )
            .await;
        }
    };
    if r.method == "GET" && r.path == "/" {
        return respond(
            stream,
            200,
            "text/html; charset=utf-8",
            super::local_server::HTML.as_bytes(),
            None,
        )
        .await;
    }
    if r.method == "GET"
        && matches!(
            r.path.as_str(),
            "/foundation" | "/foundation.js" | "/foundation.css"
        )
    {
        let (mime, body) = match r.path.as_str() {
            "/foundation" => ("text/html; charset=utf-8", include_str!("foundation.html")),
            "/foundation.js" => (
                "text/javascript; charset=utf-8",
                include_str!("foundation.js"),
            ),
            _ => ("text/css; charset=utf-8", include_str!("foundation.css")),
        };
        return respond(stream, 200, mime, body.as_bytes(), None).await;
    }
    if r.method == "GET" && r.path.starts_with("/api/foundation/card?id=") {
        let result = async {
            let id = Uuid::parse_str(r.path.trim_start_matches("/api/foundation/card?id="))?;
            super::foundation_server::card(store, &actor, id).await
        }
        .await;
        return match result {
            Ok(bytes) => respond(stream, 200, "image/png", &bytes, None).await,
            Err(_) => {
                respond(
                    stream,
                    403,
                    "application/json",
                    br#"{"code":"scope_access_denied"}"#,
                    None,
                )
                .await
            }
        };
    }
    if r.path.starts_with("/api/foundation/")
        && (r.method == "POST" || (r.method == "GET" && r.path == "/api/foundation/state"))
    {
        let result = super::foundation_server::dispatch(store, &actor, &r.path, &r.body).await;
        let (status, body) = match result {
            Ok(v) => (200, v),
            Err(e) => (
                400,
                json!({"code":super::foundation_server::error_code(&e)}),
            ),
        };
        return respond(
            stream,
            status,
            "application/json",
            &serde_json::to_vec(&body)?,
            None,
        )
        .await;
    }
    let result: Result<serde_json::Value> = match (r.method.as_str(), r.path.as_str()) {
        ("GET", "/api/me") => store.me(&actor).await,
        ("GET", "/api/accounts") => store.accounts(&actor).await,
        ("POST", "/api/logout") => store.logout(&actor).await.map(|_| json!({"ok":true})),
        ("POST", "/api/password") => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Passwords {
                current_password: String,
                new_password: String,
            }
            let parsed = serde_json::from_slice::<Passwords>(&r.body);
            use zeroize::Zeroize;
            r.body.zeroize();
            match parsed {
                Ok(p) => {
                    let current = Zeroizing::new(p.current_password);
                    let new = Zeroizing::new(p.new_password);
                    store
                        .change_password(&actor, &current, &new)
                        .await
                        .map(|_| json!({"ok":true}))
                }
                Err(_) => Err(anyhow::anyhow!("invalid_request")),
            }
        }
        ("POST", "/api/accounts") => {
            let parsed = serde_json::from_slice::<AccountCommand>(&r.body);
            use zeroize::Zeroize;
            r.body.zeroize();
            match parsed {
                Ok(c) => store.account_command(&actor, &c).await,
                Err(_) => Err(anyhow::anyhow!("invalid_request")),
            }
        }
        _ => return super::local_server::dispatch(stream, store, &actor, r).await,
    };
    match result {
        Ok(value) => {
            respond(
                stream,
                200,
                "application/json",
                &serde_json::to_vec(&value)?,
                if r.path == "/api/logout" || r.path == "/api/password" {
                    Some(("collaboration-session", ""))
                } else {
                    None
                },
            )
            .await
        }
        Err(e) => failure(stream, &e).await,
    }
}

async fn failure(stream: &mut TcpStream, error: &anyhow::Error) -> Result<()> {
    let message = error.to_string();
    let (status, code) = match message.as_str() {
        "invalid_credentials" => (401, "invalid_credentials"),
        "authentication_required" | "identity_changed_or_revoked" => {
            (401, "authentication_required")
        }
        "login_rate_limited" => (429, "login_rate_limited"),
        "account_management_denied" => (403, "account_management_denied"),
        "password_length" | "invalid_username" | "invalid_request" | "account_conflict"
        | "account_not_active" => (400, message.as_str()),
        _ => (400, "account_operation_failed"),
    };
    respond(
        stream,
        status,
        "application/json",
        &serde_json::to_vec(&json!({"code":code}))?,
        None,
    )
    .await
}
