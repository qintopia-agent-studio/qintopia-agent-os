//! Host-only fixed application source. Callback/model JSON cannot select a binding.
use super::{store::applications::Observation, Store};
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::Value;
use sqlx::Row;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Request {
    action: String,
    resource_alias: String,
    record: String,
    read_token: Option<Uuid>,
    observation: Option<Observation>,
}

fn enabled(store: &Store) -> bool {
    let local = std::env::var("QINTOPIA_APPLICATION_LOCAL_ENABLE").as_deref() == Ok("1")
        && std::env::var("QINTOPIA_FOUNDATION_LOCAL_ENABLE").as_deref() == Ok("1");
    let production = std::env::var("QINTOPIA_APPLICATION_PRODUCTION_ENABLE").as_deref() == Ok("1")
        && std::env::var("QINTOPIA_FOUNDATION_PRODUCTION_ENABLE").as_deref() == Ok("1");
    if store.is_live() {
        production
            && std::env::var("QINTOPIA_APPLICATION_LOCAL_ENABLE").as_deref() != Ok("1")
            && std::env::var("QINTOPIA_FOUNDATION_LOCAL_ENABLE").as_deref() != Ok("1")
    } else {
        local
            && std::env::var("QINTOPIA_APPLICATION_PRODUCTION_ENABLE").as_deref() != Ok("1")
            && std::env::var("QINTOPIA_FOUNDATION_PRODUCTION_ENABLE").as_deref() != Ok("1")
    }
}

pub(super) async fn authorize_host(
    store: &Store,
    gateway: &str,
    binding: Uuid,
    alias: &str,
) -> Result<()> {
    ensure!(enabled(store), "application_intake_disabled");
    ensure!(
        std::env::var("QINTOPIA_FOUNDATION_GATEWAY_ID").as_deref() == Ok(gateway)
            && std::env::var("QINTOPIA_APPLICATION_BINDING")?.parse::<Uuid>()? == binding
            && std::env::var("QINTOPIA_APPLICATION_RESOURCE_ALIAS").as_deref() == Ok(alias),
        "application_source_mismatch"
    );
    let row = sqlx::query("SELECT b.id FROM qintopia_agent_os.business_property_bindings b JOIN qintopia_agent_os.collaboration_scopes s ON s.tenant_key=b.tenant_key AND s.id=b.scope_id JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=b.tenant_key AND g.scope_id=b.scope_id WHERE b.tenant_key=$1 AND b.id=$2 AND b.active AND s.status='active' AND g.gateway_key=$3 AND g.active AND (NOT $4 OR (g.subject_type='wecom_internal' AND g.account_kind='employee'))")
        .bind(&store.tenant).bind(binding).bind(gateway).bind(store.is_live()).fetch_optional(&store.pool).await?;
    ensure!(
        row.is_some_and(|r| r.get::<Uuid, _>("id") == binding),
        "application_source_mismatch"
    );
    Ok(())
}

pub(super) async fn invoke(store: &Store, gateway: &str, arguments: Value) -> Result<Value> {
    let binding = Uuid::parse_str(&std::env::var("QINTOPIA_APPLICATION_BINDING")?)?;
    let alias = std::env::var("QINTOPIA_APPLICATION_RESOURCE_ALIAS")?;
    let request: Request = serde_json::from_value(arguments)?;
    ensure!(
        request.resource_alias == alias,
        "application_source_mismatch"
    );
    authorize_host(store, gateway, binding, &alias).await?;
    match request.action.as_str() {
        "open" if request.read_token.is_none() && request.observation.is_none() => {
            store
                .application_read_open(binding, &alias, &request.record)
                .await
        }
        "reconcile_welcome" if request.read_token.is_none() && request.observation.is_none() => {
            store
                .application_reconcile_welcome(binding, &alias, &request.record)
                .await
        }
        "save" => {
            store
                .application_read_save(
                    binding,
                    &alias,
                    &request.record,
                    request
                        .read_token
                        .ok_or_else(|| anyhow::anyhow!("application_read_token_required"))?,
                    &request
                        .observation
                        .ok_or_else(|| anyhow::anyhow!("application_observation_required"))?,
                )
                .await
        }
        _ => anyhow::bail!("invalid_arguments"),
    }
}

#[cfg(all(test, feature = "postgres-integration-tests"))]
#[path = "application_live_tests.rs"]
mod live_tests;
