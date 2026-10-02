//! Password entrypoint for the synthetic loopback and explicitly configured live UI.
use super::{
    store::{AccountCommand, Credentials},
    Store,
};
use crate::local_http::{request_for_host, respond, respond_with_cookie_options};
use anyhow::{ensure, Result};
use serde_json::json;
use tokio::net::TcpStream;
use url::Url;
use uuid::Uuid;
use zeroize::Zeroizing;

pub(super) struct UiOrigin {
    origin: String,
    host: String,
    secure_cookie: bool,
}

impl UiOrigin {
    fn local(port: u16) -> Self {
        Self {
            origin: format!("http://127.0.0.1:{port}"),
            host: format!("127.0.0.1:{port}"),
            secure_cookie: false,
        }
    }

    pub(super) fn from_env() -> Result<Self> {
        let value = std::env::var("QINTOPIA_COLLABORATION_PUBLIC_ORIGIN")
            .map_err(|_| anyhow::anyhow!("production_ui_public_origin_required"))?;
        Self::production(&value)
    }

    pub(super) fn production(value: &str) -> Result<Self> {
        let url = Url::parse(value).map_err(|_| anyhow::anyhow!("invalid_public_origin"))?;
        let origin = url.origin().ascii_serialization();
        ensure!(
            url.scheme() == "https" && value == origin,
            "invalid_public_origin"
        );
        let domain = match url.host() {
            Some(url::Host::Domain(domain)) => domain,
            _ => anyhow::bail!("invalid_public_origin"),
        };
        ensure!(
            domain.contains('.')
                && domain.split('.').all(|label| {
                    !label.is_empty()
                        && !label.starts_with('-')
                        && !label.ends_with('-')
                        && label
                            .bytes()
                            .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
                }),
            "invalid_public_origin"
        );
        let host = origin
            .strip_prefix("https://")
            .ok_or_else(|| anyhow::anyhow!("invalid_public_origin"))?
            .to_string();
        Ok(Self {
            origin,
            host,
            secure_cookie: true,
        })
    }
}

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
    handle_with_origin(stream, store, &UiOrigin::local(port)).await
}

pub(super) async fn handle_with_origin(
    stream: &mut TcpStream,
    store: &Store,
    public_origin: &UiOrigin,
) -> Result<()> {
    let configuration_write_started = super::ConfigurationWrite::default();
    handle_with_origin_tracked(stream, store, public_origin, &configuration_write_started).await
}

pub(super) async fn handle_with_origin_tracked(
    stream: &mut TcpStream,
    store: &Store,
    public_origin: &UiOrigin,
    configuration_write_started: &super::ConfigurationWrite,
) -> Result<()> {
    let mut r = match request_for_host(stream, &public_origin.host).await {
        Ok(r) => r,
        Err(error) => {
            return respond(
                stream,
                if error.to_string() == "body_too_large" {
                    413
                } else {
                    400
                },
                "application/json",
                br#"{"code":"invalid_request"}"#,
                None,
            )
            .await
        }
    };
    // A single independently signed ingress, never a cookie/CSRF bypass for UI routes.
    if !store.is_live() && r.path == crate::resident_welcome::protocol::PATH {
        let response = match super::business_ingress::Config::local() {
            Ok(config) => {
                super::business_ingress::receive(store, &config, &r, chrono::Utc::now().timestamp())
                    .await
            }
            Err(_) => crate::resident_welcome::ingress::Response {
                status: 503,
                body: json!({"code":"payment_ingress_unavailable"}),
            },
        };
        return respond(
            stream,
            response.status,
            "application/json",
            &serde_json::to_vec(&response.body)?,
            None,
        )
        .await;
    }
    // Exact Origin + strict cookies + JSON-only writes; includes login CSRF protection.
    if r.method == "POST"
        && (r.headers.get("origin") != Some(&public_origin.origin)
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
            "/workbench-business.js" => Some((
                "text/javascript; charset=utf-8",
                include_str!("workbench-business.js"),
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
                respond_with_cookie_options(
                    stream,
                    200,
                    "application/json",
                    br#"{"ok":true}"#,
                    Some(("collaboration-session", &token)),
                    public_origin.secure_cookie,
                    false,
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
        Err(error) => {
            if r.path.starts_with("/api/workspace/candidates")
                && error.chain().any(|cause| cause.is::<sqlx::Error>())
            {
                return respond(
                    stream,
                    503,
                    "application/json",
                    br#"{"code":"candidate_source_unavailable"}"#,
                    None,
                )
                .await;
            }
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
    if store.is_live()
        && (matches!(
            r.path.as_str(),
            "/foundation" | "/foundation.js" | "/foundation.css"
        ) || (r.path.starts_with("/api/foundation/")
            && !super::foundation_server::formal_route(&r.path)))
    {
        return respond(
            stream,
            403,
            "application/json",
            br#"{"code":"production_configuration_change_denied"}"#,
            None,
        )
        .await;
    }
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
    if r.method == "GET"
        && (r.path == "/api/workspace/candidates"
            || r.path.starts_with("/api/workspace/candidates?"))
    {
        let result = async {
            let query = super::store::workspace_candidates::Query::from_path(&r.path)?;
            store.workspace_candidates(&actor, &query).await
        }
        .await;
        let (status, body) = match result {
            Ok(value) => (200, value),
            Err(error) => {
                let (status, code) = super::store::workspace_candidates::http_error(&error);
                (status, json!({"code":code}))
            }
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
    if r.path.starts_with("/api/foundation/")
        && (r.method == "POST" || (r.method == "GET" && r.path == "/api/foundation/state"))
    {
        if super::foundation_server::formal_route(&r.path)
            && (r.path.ends_with("/change") || r.path.ends_with("/decision"))
        {
            let body: Option<serde_json::Value> = serde_json::from_slice(&r.body).ok();
            let operation = body
                .as_ref()
                .and_then(|v| v["operation_id"].as_str())
                .and_then(|id| Uuid::parse_str(id).ok());
            configuration_write_started.start(Some(
                operation
                    .map(|id| ("operation_id", id))
                    .unwrap_or(("actor_ref", actor.person_ref())),
            ));
        }
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
        ("POST", "/api/logout") => {
            configuration_write_started.start(Some(("actor_ref", actor.person_ref())));
            store.logout(&actor).await.map(|_| json!({"ok":true}))
        }
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
                    configuration_write_started.start(Some(("actor_ref", actor.person_ref())));
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
                Ok(c) => {
                    let correlation = match &c {
                        AccountCommand::Create { person, .. } => ("person_ref", *person),
                        AccountCommand::Reset { account, .. }
                        | AccountCommand::Disable { account } => ("account_ref", *account),
                    };
                    configuration_write_started.start(Some(correlation));
                    store.account_command(&actor, &c).await
                }
                Err(_) => Err(anyhow::anyhow!("invalid_request")),
            }
        }
        _ => {
            return super::local_server::dispatch_tracked(
                stream,
                store,
                &actor,
                r,
                configuration_write_started,
            )
            .await;
        }
    };
    if let Err(e) = &result {
        if store.is_live() && e.to_string() == "configuration_commit_outcome_unknown" {
            return Err(anyhow::anyhow!("configuration_commit_outcome_unknown"));
        }
    }
    configuration_write_started.finish();
    match result {
        Ok(value) => {
            respond_with_cookie_options(
                stream,
                200,
                "application/json",
                &serde_json::to_vec(&value)?,
                if r.path == "/api/logout" || r.path == "/api/password" {
                    Some(("collaboration-session", ""))
                } else {
                    None
                },
                public_origin.secure_cookie,
                r.path == "/api/logout" || r.path == "/api/password",
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

#[cfg(test)]
mod tests {
    use super::UiOrigin;

    #[test]
    fn production_origin_requires_one_canonical_https_origin() {
        let configured = UiOrigin::production("https://admin.example.test:8443").unwrap();
        assert_eq!(configured.origin, "https://admin.example.test:8443");
        assert_eq!(configured.host, "admin.example.test:8443");
        assert!(configured.secure_cookie);
        for value in [
            "http://admin.example.test",
            "https://admin.example.test/agent-os/",
            "https://admin.example.test/",
            "https://admin.example.test?mode=1",
            "https://admin.example.test#section",
            "https://user:pass@admin.example.test",
            "https://admin.example.test:443",
            "https://*.example.test",
            "admin.example.test",
            "",
        ] {
            assert!(UiOrigin::production(value).is_err(), "accepted {value}");
        }
        let local = UiOrigin::local(18875);
        assert_eq!(local.origin, "http://127.0.0.1:18875");
        assert_eq!(local.host, "127.0.0.1:18875");
        assert!(!local.secure_cookie);
    }
}
