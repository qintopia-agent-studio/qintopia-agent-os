//! Shared person/Agent control plane with separate local and live entry points.
mod application_ingress;
pub mod auth_server;
mod business_ingress;
mod foundation_server;
pub(crate) use foundation_server::error_code as foundation_error_code;
pub mod local_server;
mod model;
mod store;
pub(crate) mod welcome_model;
pub use model::{Assignment, Change, Command, Delegation};
pub use store::foundation::{
    authorize_current, can_inspect_current, effective_knowledge_in, put_knowledge_in, Authority,
    KnowledgeVersion, KnowledgeWrite,
};
pub(crate) use store::shared_reply_context;
pub(crate) use store::steward::{content_reviewer, delegated_scopes_in};
pub use store::{Actor, Store};
pub use store::{IdentityCommand, QiweConversion};
pub use store::{MemoryChange, MemoryCommand, MemoryEvidence, ReplyCondition, ReplyStyle};
#[cfg(test)]
mod auth_tests;
#[cfg(test)]
mod duty_store_tests;
#[cfg(test)]
mod foundation_consumer_tests;
#[cfg(test)]
mod foundation_review_tests;
#[cfg(test)]
mod foundation_server_tests;
#[cfg(test)]
mod identity_memory_tests;
#[cfg(test)]
mod identity_ui_tests;
#[cfg(test)]
mod ontology_ui_tests;
#[cfg(test)]
mod tests;

use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, PgPool};
use tokio::net::TcpListener;
use url::Url;

pub fn digest(input: &[u8]) -> String {
    format!("{:x}", Sha256::digest(input))
}

pub fn validate_local_database(input: &str) -> Result<()> {
    let u = Url::parse(input).map_err(|_| anyhow::anyhow!("invalid_local_database"))?;
    ensure!(
        matches!(u.scheme(), "postgres" | "postgresql")
            && matches!(u.host_str(), Some("127.0.0.1" | "[::1]"))
            && u.path() == "/qintopia_test"
            && u.query().is_none()
            && u.fragment().is_none(),
        "isolated_loopback_database_required"
    );
    Ok(())
}

async fn connect_local(input: &str) -> Result<PgPool> {
    validate_local_database(input)?;
    PgPoolOptions::new()
        .max_connections(5)
        .connect(input)
        .await
        .map_err(|_| anyhow::anyhow!("local_database_unavailable"))
}

async fn production_store() -> Result<Store> {
    ensure!(
        std::env::var("QINTOPIA_FOUNDATION_PRODUCTION_ENABLE").as_deref() == Ok("1"),
        "production_foundation_disabled"
    );
    let tenant = std::env::var("QINTOPIA_FOUNDATION_TENANT")?;
    let namespace = std::env::var("QINTOPIA_FOUNDATION_IDENTITY_NAMESPACE")?;
    let database = std::env::var("QINTOPIA_FOUNDATION_DATABASE_URL")?;
    let url = Url::parse(&database).map_err(|_| anyhow::anyhow!("invalid_foundation_database"))?;
    ensure!(
        matches!(url.scheme(), "postgres" | "postgresql")
            && url.host_str().is_some()
            && !matches!(url.path(), "" | "/" | "/qintopia_test")
            && url.fragment().is_none(),
        "production_database_required"
    );
    let pool = PgPoolOptions::new()
        .max_connections(5)
        .connect(&database)
        .await
        .map_err(|_| anyhow::anyhow!("foundation_database_unavailable"))?;
    Store::live(pool, &tenant, &namespace).await
}

pub async fn run_production_broker() -> Result<()> {
    let store = production_store().await?;
    foundation_server::broker_live(store).await
}

pub async fn run_production_ui(port: u16) -> Result<()> {
    let public_origin = auth_server::UiOrigin::from_env()?;
    let store = production_store().await?;
    ensure!(
        std::env::var("QINTOPIA_COLLABORATION_LOCAL_ENABLE").as_deref() != Ok("1")
            && std::env::var("QINTOPIA_FOUNDATION_LOCAL_ENABLE").as_deref() != Ok("1"),
        "production_ui_local_mode_denied"
    );
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    let mut terminate = tokio::signal::unix::signal(tokio::signal::unix::SignalKind::terminate())?;
    serve_production_ui_until(listener, &store, &public_origin, async move {
        terminate.recv().await;
    })
    .await
}

#[derive(Default)]
pub(super) struct ConfigurationWrite {
    started: std::sync::atomic::AtomicBool,
    correlation: std::sync::Mutex<Option<(&'static str, uuid::Uuid)>>,
}

impl ConfigurationWrite {
    fn start(&self, correlation: Option<(&'static str, uuid::Uuid)>) {
        *self.correlation.lock().expect("write tracker mutex") = correlation;
        self.started
            .store(true, std::sync::atomic::Ordering::Release);
    }

    fn finish(&self) {
        self.started
            .store(false, std::sync::atomic::Ordering::Release);
    }

    fn pending(&self) -> bool {
        self.started.load(std::sync::atomic::Ordering::Acquire)
    }

    fn log_unknown(&self, code: &str) {
        if let Some((key, value)) = *self.correlation.lock().expect("write tracker mutex") {
            eprintln!("{code} {key}={value}");
        } else {
            eprintln!("{code}");
        }
    }
}

async fn serve_production_ui_until(
    listener: TcpListener,
    store: &Store,
    public_origin: &auth_server::UiOrigin,
    shutdown: impl std::future::Future<Output = ()>,
) -> Result<()> {
    tokio::pin!(shutdown);
    loop {
        let (mut stream, peer) = tokio::select! {
            biased;
            _ = &mut shutdown => return Ok(()),
            accepted = listener.accept() => accepted?,
        };
        if !peer.ip().is_loopback() {
            continue;
        }
        let configuration_write_started = ConfigurationWrite::default();
        let request = auth_server::handle_with_origin_tracked(
            &mut stream,
            store,
            public_origin,
            &configuration_write_started,
        );
        tokio::pin!(request);
        let result = tokio::select! {
            biased;
            _ = &mut shutdown => {
                drop(listener);
                return drain_production_ui_request(
                    request,
                    &configuration_write_started,
                    std::time::Duration::from_secs(30),
                ).await;
            }
            result = tokio::time::timeout(std::time::Duration::from_secs(5), &mut request) => result,
        };
        if !matches!(result, Ok(Ok(()))) && configuration_write_started.pending() {
            configuration_write_started.log_unknown("production_ui_request_outcome_unknown");
            anyhow::bail!("production_ui_request_outcome_unknown");
        }
    }
}

async fn drain_production_ui_request(
    request: impl std::future::Future<Output = Result<()>>,
    configuration_write_started: &ConfigurationWrite,
    limit: std::time::Duration,
) -> Result<()> {
    match tokio::time::timeout(limit, request).await {
        Ok(Ok(())) => Ok(()),
        Ok(Err(_)) | Err(_) if !configuration_write_started.pending() => Ok(()),
        Ok(Err(_)) | Err(_) => {
            configuration_write_started.log_unknown("production_ui_stop_outcome_unknown");
            anyhow::bail!("production_ui_stop_outcome_unknown")
        }
    }
}

#[cfg(test)]
mod production_ui_stop_tests {
    use super::drain_production_ui_request;
    use std::{future::pending, time::Duration};

    #[tokio::test]
    async fn drain_distinguishes_unfinished_reads_from_uncertain_writes() {
        let limit = Duration::from_millis(5);
        let read = super::ConfigurationWrite::default();
        drain_production_ui_request(pending(), &read, limit)
            .await
            .unwrap();
        drain_production_ui_request(async { anyhow::bail!("read_failed") }, &read, limit)
            .await
            .unwrap();

        let write = super::ConfigurationWrite::default();
        write.start(Some(("operation_id", uuid::Uuid::new_v4())));
        assert_eq!(
            drain_production_ui_request(pending(), &write, limit)
                .await
                .unwrap_err()
                .to_string(),
            "production_ui_stop_outcome_unknown"
        );
        assert_eq!(
            drain_production_ui_request(async { anyhow::bail!("write_failed") }, &write, limit)
                .await
                .unwrap_err()
                .to_string(),
            "production_ui_stop_outcome_unknown"
        );
        drain_production_ui_request(async { Ok(()) }, &write, limit)
            .await
            .unwrap();
    }
}

pub async fn bootstrap_production_account(person: uuid::Uuid, username: &str) -> Result<()> {
    use std::io::Read;
    use zeroize::Zeroizing;
    let store = production_store().await?;
    ensure!(
        std::env::var("QINTOPIA_COLLABORATION_LOCAL_ENABLE").as_deref() != Ok("1"),
        "production_ui_local_mode_denied"
    );
    let mut password = Zeroizing::new(String::new());
    std::io::stdin().take(1024).read_to_string(&mut password)?;
    ensure!(password.len() < 1024, "password_length");
    store
        .bootstrap_account(person, username, password.trim_end_matches(['\r', '\n']))
        .await?;
    println!("Account initialized; no business permissions added.");
    Ok(())
}

pub async fn run_production_payment_events(port: u16) -> Result<()> {
    let store = production_store().await?;
    business_ingress::run_production_events(store, port).await
}

#[derive(Deserialize)]
#[serde(
    tag = "area",
    content = "command",
    rename_all = "snake_case",
    deny_unknown_fields
)]
enum ProductionConfiguration {
    Collaboration(model::Command),
    Business(store::business_config::BusinessConfigCommand),
    PersonDraft(ProductionPersonDraft),
    Identity(store::IdentityUiCommand),
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct ProductionPersonDraft {
    source_link: uuid::Uuid,
    command: model::Command,
}

pub async fn run_production_configuration(apply: bool, status: bool) -> Result<()> {
    use std::io::Read;

    let store = production_store().await?;
    let gateway = std::env::var("QINTOPIA_FOUNDATION_ADMIN_GATEWAY_ID")?;
    let sender = std::env::var("QINTOPIA_FOUNDATION_ADMIN_SENDER_ID")?;
    let actor = store.production_admin_actor(&gateway, &sender).await?;
    if status {
        ensure!(!apply, "invalid_configuration_request");
        let state = store.state(&actor).await?;
        let business = store.business_configuration_state(&actor).await?;
        let identity = store.production_identities(&actor).await?;
        println!(
            "{}",
            serde_json::to_string(&json!({
                "version":state["version"],
                "people":state["people"],
                "scopes":state["scopes"],
                "roles":state["roles"],
                "duties":state["duties"],
                "relations":state["relations"],
                "grants":state["grants"],
                "business":business,
                "identity":identity
            }))?
        );
        return Ok(());
    }
    let mut raw = Vec::new();
    std::io::stdin().take(64 * 1024 + 1).read_to_end(&mut raw)?;
    ensure!(
        !raw.is_empty() && raw.len() <= 64 * 1024,
        "invalid_configuration_request"
    );
    let value = crate::strict_json::parse_strict_bounded_slice(
        &raw,
        crate::strict_json::registry_json_limits(64 * 1024),
    )?;
    let command: ProductionConfiguration = serde_json::from_value(value)?;
    let result = match command {
        ProductionConfiguration::Collaboration(command) => {
            ensure!(
                matches!(
                    &command.change,
                    model::Change::Assign(_)
                        | model::Change::ConfigureWork { .. }
                        | model::Change::RevokeGrant { .. }
                        | model::Change::EndAppointment { .. }
                        | model::Change::EndCollaboration { .. }
                        | model::Change::SetGroups { .. }
                ),
                "production_configuration_change_denied"
            );
            store.command(&actor, &command, apply).await?
        }
        ProductionConfiguration::Business(command) => {
            store.business_configure(&actor, &command, apply).await?
        }
        ProductionConfiguration::PersonDraft(request) => {
            store
                .production_person_draft(&actor, request.source_link, &request.command, apply)
                .await?
        }
        ProductionConfiguration::Identity(command) => {
            store
                .production_identity_change(&actor, &command, apply)
                .await?
        }
    };
    println!("{}", serde_json::to_string(&result)?);
    Ok(())
}

#[cfg(test)]
mod rule_lifecycle_tests;

#[cfg(test)]
pub(crate) mod business_tests;

#[cfg(test)]
mod welcome_review_tests;

pub(crate) use store::welcome_review::assert_operations_review as assert_welcome_operations_review;
