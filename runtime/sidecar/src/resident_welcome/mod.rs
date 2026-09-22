//! Governed resident welcome. No live entrypoint is installed by this module.
pub mod artifact;
pub mod channels;
pub mod client;
pub mod delivery;
pub mod foundation;
mod foundation_fixture;
#[cfg(test)]
mod foundation_tests;
pub mod ingress;
#[cfg(feature = "welcome-synthetic-driver")]
pub mod local_driver;
pub mod local_server;
pub mod projection;
pub mod protocol;
pub mod recovery;
pub mod review;
mod runtime_adapter;
pub mod state;
pub mod store;
#[cfg(test)]
mod tests;
pub mod workbench;

use anyhow::{bail, ensure, Result};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, PgPool};
use url::Url;

pub fn digest(bytes: &[u8]) -> String {
    format!("{:x}", Sha256::digest(bytes))
}

/// No ambient SIDECAR_DATABASE_URL or .env fallback. Synthetic and shadow stores
/// are separate databases; this V1 local adapter cannot open a live database.
pub async fn connect_local(database_url: &str) -> Result<PgPool> {
    let url = Url::parse(database_url).map_err(|_| anyhow::anyhow!("invalid local database"))?;
    ensure!(
        matches!(url.scheme(), "postgres" | "postgresql"),
        "local database required"
    );
    ensure!(
        matches!(url.host_str(), Some("127.0.0.1" | "[::1]")),
        "literal loopback required"
    );
    ensure!(
        url.path() == "/qintopia_test" && url.query().is_none(),
        "isolated qintopia_test required"
    );
    PgPoolOptions::new()
        .max_connections(5)
        .connect(database_url)
        .await
        .map_err(|_| anyhow::anyhow!("local database unavailable"))
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub enum ExecutionMode {
    Shadow,
    Synthetic,
    Live,
}

pub fn external_gate(mode: ExecutionMode) -> Result<()> {
    match mode {
        ExecutionMode::Shadow => bail!("shadow_external_effect_disabled"),
        ExecutionMode::Live => bail!("live_adapter_not_activated"),
        ExecutionMode::Synthetic => bail!("synthetic_requires_fixture_adapter"),
    }
}
