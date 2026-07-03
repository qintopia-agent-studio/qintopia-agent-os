use anyhow::{Context, Result};
use serde_json::json;
use tracing::info;

use crate::config::Cli;
use crate::db;

pub async fn check(cli: &Cli) -> Result<()> {
    let client = async_nats::connect(&cli.nats_url)
        .await
        .with_context(|| format!("connect NATS at {}", cli.nats_url))?;
    let jetstream = async_nats::jetstream::new(client);
    let mut stream = jetstream
        .get_stream(&cli.nats_stream)
        .await
        .with_context(|| format!("get JetStream stream {}", cli.nats_stream))?;
    let info = stream.info().await.context("read stream info")?;
    info!(
        stream = %info.config.name,
        subjects = ?info.config.subjects,
        messages = info.state.messages,
        bytes = info.state.bytes,
        "NATS JetStream check passed"
    );

    if let Some(database_url) = &cli.database_url {
        let pool = db::connect(database_url, 1).await?;
        let row: (i64,) = sqlx::query_as("select 1::bigint")
            .fetch_one(&pool)
            .await
            .context("run Postgres health query")?;
        let db_check = db::check(&pool).await?;
        info!(
            result = row.0,
            database = %db_check.database,
            schema_exists = db_check.schema_exists,
            messages_table_exists = db_check.messages_table_exists,
            "Postgres check passed"
        );
    } else {
        info!("Postgres check skipped because QINTOPIA_SIDECAR_DATABASE_URL is unset");
    }

    println!(
        "{}",
        serde_json::to_string_pretty(&json!({
            "ok": true,
            "nats_url": cli.nats_url,
            "stream": cli.nats_stream,
            "postgres_checked": cli.database_url.is_some()
        }))?
    );
    Ok(())
}
