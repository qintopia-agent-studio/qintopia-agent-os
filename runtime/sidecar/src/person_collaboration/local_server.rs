//! Synthetic-only local configuration listener.
use super::{
    model::Command,
    store::{Actor, Store},
};
#[cfg(all(test, feature = "postgres-integration-tests"))]
use crate::local_http::request;
use crate::local_http::respond;
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use tokio::{
    io::AsyncWriteExt,
    net::{TcpListener, TcpStream},
};
use uuid::Uuid;

const TENANT: &str = "synthetic-collaboration-ui-v1";
pub(super) const HTML: &str = include_str!("workbench.html");

pub async fn run(port: u16, init_fixture: bool) -> Result<()> {
    ensure!(
        std::env::var("QINTOPIA_COLLABORATION_LOCAL_ENABLE").as_deref() == Ok("1"),
        "collaboration_local_disabled"
    );
    let database = std::env::var("QINTOPIA_COLLABORATION_LOCAL_DATABASE_URL")
        .map_err(|_| anyhow::anyhow!("explicit_local_database_required"))?;
    let tenant =
        std::env::var("QINTOPIA_COLLABORATION_LOCAL_TENANT").unwrap_or_else(|_| TENANT.into());
    let store = Store::local(&database, &tenant).await?;
    if init_fixture {
        crate::db::run_migrations(&store.pool).await?;
        store
            .bootstrap_fixture()
            .await
            .map_err(|_| anyhow::anyhow!("fixture_initialization_failed_or_already_exists"))?;
        let actor = store.actor(store.fixture_operator().await?).await?;
        store.organization_fixture(&actor).await?;
    }
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    println!("Collaboration local settings: http://127.0.0.1:{port}/");
    loop {
        let (mut stream, peer) = listener.accept().await?;
        if !peer.ip().is_loopback() {
            continue;
        }
        if !matches!(
            tokio::time::timeout(
                std::time::Duration::from_secs(5),
                super::auth_server::handle(&mut stream, &store, port)
            )
            .await,
            Ok(Ok(()))
        ) {
            let _ = stream.shutdown().await;
        }
    }
}

#[cfg(all(test, feature = "postgres-integration-tests"))]
pub(super) async fn handle(
    stream: &mut TcpStream,
    store: &Store,
    actor: &Actor,
    port: u16,
    csrf: &str,
) -> Result<()> {
    let r = match request(stream, port).await {
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
    if r.method == "GET" && r.path == "/" {
        return respond(
            stream,
            200,
            "text/html; charset=utf-8",
            HTML.as_bytes(),
            Some(("collaboration-local", csrf)),
        )
        .await;
    }
    if r.method == "GET" {
        let asset = match r.path.as_str() {
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
            "/workbench-organization.js" => Some((
                "text/javascript; charset=utf-8",
                include_str!("workbench-organization.js"),
            )),
            _ => None,
        };
        if let Some((content_type, content)) = asset {
            return respond(stream, 200, content_type, content.as_bytes(), None).await;
        }
    }
    let session = r.headers.get("cookie").is_some_and(|c| {
        c.split(';')
            .any(|part| part.trim() == format!("collaboration-local={csrf}"))
    });
    let origin = r
        .headers
        .get("origin")
        .is_some_and(|o| o == &format!("http://127.0.0.1:{port}"));
    if !session
        || (r.method == "POST"
            && (!origin
                || r.headers.get("content-type").map(String::as_str) != Some("application/json")))
    {
        return respond(
            stream,
            403,
            "application/json",
            br#"{"code":"local_session_required"}"#,
            None,
        )
        .await;
    }
    dispatch(stream, store, actor, r).await
}

pub(super) async fn dispatch(
    stream: &mut TcpStream,
    store: &Store,
    actor: &Actor,
    r: crate::local_http::Request,
) -> Result<()> {
    let result: Result<Value> = async {
        match (r.method.as_str(), r.path.as_str()) {
            ("GET", "/api/state") => store.state(actor).await,
            ("POST", "/api/contact-decision") => {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct Contact {
                    collaboration: Uuid,
                    kind: String,
                    target: Uuid,
                    proactive: bool,
                }
                let request: Contact = serde_json::from_slice(&r.body)
                    .map_err(|_| anyhow::anyhow!("invalid_command"))?;
                store
                    .contact_decision(
                        actor,
                        request.collaboration,
                        &request.kind,
                        request.target,
                        request.proactive,
                    )
                    .await
            }
            ("POST", "/api/decision") => {
                #[derive(serde::Deserialize)]
                #[serde(deny_unknown_fields)]
                struct DecisionRequest {
                    collaboration: Uuid,
                    action: String,
                }
                let request: DecisionRequest = serde_json::from_slice(&r.body)
                    .map_err(|_| anyhow::anyhow!("invalid_command"))?;
                store
                    .decision(actor, request.collaboration, &request.action)
                    .await
            }
            ("POST", "/api/preview" | "/api/save") => {
                let command: Command = serde_json::from_slice(&r.body)
                    .map_err(|_| anyhow::anyhow!("invalid_command"))?;
                if let super::model::Change::Assign(a) = &command.change {
                    ensure!(
                        a.duty.is_some() && a.actions.is_empty(),
                        "explicit_duty_permissions_required"
                    );
                }
                store.command(actor, &command, r.path == "/api/save").await
            }
            _ => anyhow::bail!("unknown_route"),
        }
    }
    .await;
    let (status, body) = match result {
        Ok(value) => (200, value),
        Err(e) => {
            let message = e.to_string();
            let code = match message.as_str() {
                "command_already_processed_refresh_state"
                | "authentication_required"
                | "scope_access_denied"
                | "configuration_version_conflict"
                | "position_has_children"
                | "position_history_dimensions_fixed"
                | "invalid_position_parent"
                | "only_unused_draft_deletable"
                | "ledger_identity_immutable"
                | "person_not_verified"
                | "catalog_not_active"
                | "group_outside_scope"
                | "group_not_verified"
                | "invalid_text"
                | "explicit_duty_permissions_required"
                | "catalog_management_required"
                | "catalog_in_use"
                | "duty_actions_in_use"
                | "role_duties_in_use"
                | "scope_has_active_bindings"
                | "root_scope_fixed"
                | "duty_not_found"
                | "role_not_active"
                | "scope_not_active"
                | "catalog_retired"
                | "catalog_not_found"
                | "duty_not_available_for_role"
                | "duty_domain_mismatch"
                | "action_outside_duty"
                | "reviewer_required"
                | "reviewer_mode_mismatch"
                | "self_confirmation_forbidden"
                | "mixed_permission_formats"
                | "reviewer_not_authorized"
                | "technical_duty_domain_required"
                | "collaboration_not_active"
                | "bootstrap_relation_cannot_be_rewritten"
                | "action_outside_role"
                | "invalid_role_actions"
                | "idempotency_conflict"
                | "management_denied"
                | "identity_changed_or_revoked"
                | "verified_person_required"
                | "delegation_exceeds_authority"
                | "existing_term_differs"
                | "invalid_proxy_appointment"
                | "proxy_requires_expiry"
                | "group_binding_management_required"
                | "organization_management_required"
                | "invalid_label"
                | "invalid_command"
                | "unknown_agent_or_domain"
                | "invalid_actions"
                | "expired_term"
                | "delegation_envelope_required"
                | "invalid_delegation" => message.as_str(),
                _ => "configuration_not_saved",
            };
            (
                if code == "authentication_required" {
                    401
                } else if matches!(
                    code,
                    "scope_access_denied" | "management_denied" | "catalog_management_required"
                ) {
                    403
                } else {
                    409
                },
                json!({"code":code}),
            )
        }
    };
    respond(
        stream,
        status,
        "application/json",
        &serde_json::to_vec(&body)?,
        None,
    )
    .await
}
