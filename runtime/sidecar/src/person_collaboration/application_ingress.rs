//! Host-only fixed application source. Callback/model JSON cannot select a binding.
use super::{store::applications::Observation, Store};
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::Value;
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

pub(super) async fn invoke(store: &Store, arguments: Value) -> Result<Value> {
    ensure!(
        std::env::var("QINTOPIA_APPLICATION_LOCAL_ENABLE").as_deref() == Ok("1")
            && std::env::var("QINTOPIA_FOUNDATION_LOCAL_ENABLE").as_deref() == Ok("1"),
        "application_intake_disabled"
    );
    let binding = Uuid::parse_str(&std::env::var("QINTOPIA_APPLICATION_BINDING")?)?;
    let alias = std::env::var("QINTOPIA_APPLICATION_RESOURCE_ALIAS")?;
    let request: Request = serde_json::from_value(arguments)?;
    ensure!(
        request.resource_alias == alias,
        "application_source_mismatch"
    );
    match request.action.as_str() {
        "open" if request.read_token.is_none() && request.observation.is_none() => {
            store
                .application_read_open(binding, &alias, &request.record)
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
