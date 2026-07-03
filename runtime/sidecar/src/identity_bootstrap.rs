use anyhow::{Context, Result};
use serde::Serialize;
use sqlx::postgres::PgPool;

use crate::{config::Cli, db};

#[derive(Debug, Clone)]
pub struct BootstrapOptions {
    pub apply: bool,
    pub dry_run: bool,
    pub chat_id: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Default, Serialize)]
struct BootstrapReport {
    total_channel_identities: i64,
    persons_created: i64,
    channel_identities_linked: i64,
    aliases_inserted: i64,
    messages_updated: i64,
    dry_run: bool,
}

pub async fn run(cli: &Cli, options: BootstrapOptions) -> Result<()> {
    if options.apply && options.dry_run {
        anyhow::bail!("use either --apply or --dry-run, not both");
    }
    let apply = options.apply && !options.dry_run;
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let report = run_bootstrap(&pool, &options, apply).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn run_bootstrap(
    pool: &PgPool,
    options: &BootstrapOptions,
    apply: bool,
) -> Result<BootstrapReport> {
    let limit = options.limit.unwrap_or(500).max(1);
    let total = sqlx::query_as::<_, (i64,)>(
        r#"
        SELECT count(*)::bigint
        FROM qintopia_identity.channel_identities
        WHERE platform = 'qiwe'
          AND person_id IS NULL
          AND COALESCE(display_name, '') <> ''
          AND ($1::text IS NULL OR chat_id = $1)
        "#,
    )
    .bind(options.chat_id.as_deref())
    .fetch_one(pool)
    .await
    .context("count channel identities needing person bootstrap")?
    .0;

    let mut report = BootstrapReport {
        total_channel_identities: total,
        dry_run: !apply,
        ..BootstrapReport::default()
    };
    if !apply || total == 0 {
        return Ok(report);
    }

    let mut tx = pool.begin().await.context("begin person bootstrap")?;
    let rows = sqlx::query_as::<_, (i64, i64, i64)>(
        r#"
        WITH candidates AS (
            SELECT id, display_name, normalized_display_name
            FROM qintopia_identity.channel_identities
            WHERE platform = 'qiwe'
              AND person_id IS NULL
              AND COALESCE(display_name, '') <> ''
              AND ($1::text IS NULL OR chat_id = $1)
            ORDER BY updated_at DESC, id
            LIMIT $2
            FOR UPDATE SKIP LOCKED
        ),
        created AS (
            INSERT INTO qintopia_identity.persons
                (display_name, primary_name, preferred_name, metadata)
            SELECT
                c.display_name,
                c.normalized_display_name,
                c.display_name,
                jsonb_build_object(
                    'bootstrap_source', 'qiwe_channel_identity',
                    'channel_identity_id', c.id::text,
                    'person_merge_status', 'unmerged'
                )
            FROM candidates c
            RETURNING id, (metadata->>'channel_identity_id')::uuid AS channel_identity_id
        ),
        linked AS (
            UPDATE qintopia_identity.channel_identities ci
            SET person_id = created.id,
                updated_at = now()
            FROM created
            WHERE ci.id = created.channel_identity_id
            RETURNING ci.id AS channel_identity_id, created.id AS person_id, ci.display_name
        ),
        aliases AS (
            INSERT INTO qintopia_identity.person_aliases
                (person_id, alias, alias_type, source, confidence, metadata)
            SELECT
                linked.person_id,
                linked.display_name,
                'nickname',
                'qiwe_channel_identity',
                1.0,
                jsonb_build_object('channel_identity_id', linked.channel_identity_id::text)
            FROM linked
            WHERE linked.display_name !~ '^[0-9]+$'
            ON CONFLICT (person_id, alias, alias_type) DO UPDATE SET
                last_seen_at = now(),
                source = EXCLUDED.source,
                confidence = GREATEST(qintopia_identity.person_aliases.confidence, EXCLUDED.confidence),
                metadata = qintopia_identity.person_aliases.metadata || EXCLUDED.metadata
            RETURNING id
        ),
        updated_messages AS (
            UPDATE qintopia_messages.messages m
            SET sender_person_id = linked.person_id,
                updated_at = now()
            FROM linked
            WHERE m.sender_channel_identity_id = linked.channel_identity_id
              AND m.sender_person_id IS NULL
            RETURNING m.id
        )
        SELECT
            (SELECT count(*)::bigint FROM linked) AS linked_count,
            (SELECT count(*)::bigint FROM aliases) AS alias_count,
            (SELECT count(*)::bigint FROM updated_messages) AS message_count
        "#,
    )
    .bind(options.chat_id.as_deref())
    .bind(limit)
    .fetch_one(&mut *tx)
    .await
    .context("bootstrap persons from channel identities")?;

    tx.commit().await.context("commit person bootstrap")?;
    report.persons_created = rows.0;
    report.channel_identities_linked = rows.0;
    report.aliases_inserted = rows.1;
    report.messages_updated = rows.2;
    Ok(report)
}
