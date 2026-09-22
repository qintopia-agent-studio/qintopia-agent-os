//! Shared person/Agent control plane. F1 exposes only an isolated synthetic UI.
pub mod auth_server;
mod foundation_server;
pub mod local_server;
mod model;
mod store;
pub use model::{Assignment, Change, Command, Delegation};
pub use store::foundation::{
    authorize_current, can_inspect_current, effective_knowledge_in, put_knowledge_in, Authority,
    KnowledgeVersion, KnowledgeWrite,
};
pub(crate) use store::shared_reply_context;
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
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPoolOptions, PgPool};
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
