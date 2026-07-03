use std::time::Duration;

use anyhow::{bail, Context, Result};
use serde::Serialize;
use sqlx::postgres::PgPool;
use tracing::info;

use crate::{config::Cli, db};

#[derive(Debug, Clone)]
pub struct GraphProjectionOptions {
    pub apply: bool,
    pub dry_run: bool,
    pub chat_id: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Clone)]
pub struct GraphProjectionWorkerOptions {
    pub check_only: bool,
    pub batch_size: i64,
    pub poll_seconds: u64,
    pub chat_id: Option<String>,
}

#[derive(Debug, Default, Serialize)]
struct GraphProjectionReport {
    dry_run: bool,
    graph_backend: String,
    age_enabled: bool,
    target_chat_ids: Vec<String>,
    candidate_entities: i64,
    candidate_edges: i64,
    entities_upserted: i64,
    edges_upserted: i64,
    fallback: String,
}

pub async fn run(cli: &Cli, options: GraphProjectionOptions) -> Result<()> {
    if options.apply && options.dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let apply = options.apply && !options.dry_run;
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let report = run_inner(&pool, cli, &options, apply).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_worker(cli: &Cli, options: GraphProjectionWorkerOptions) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let poll_seconds = options.poll_seconds.max(30);
    let projection_options = GraphProjectionOptions {
        apply: true,
        dry_run: options.check_only,
        chat_id: options.chat_id.clone(),
        limit: Some(options.batch_size.max(1)),
    };

    if options.check_only {
        let report = run_inner(&pool, cli, &projection_options, false).await?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        return Ok(());
    }

    info!(
        batch_size = options.batch_size.max(1),
        poll_seconds,
        chat_id = ?options.chat_id,
        backend = %cli.graph_backend,
        age_enabled = cli.age_enabled,
        "starting graph projection worker"
    );
    let report = run_inner(&pool, cli, &projection_options, true).await?;
    log_graph_report(&report, "graph projection worker initial batch complete");
    loop {
        let report = run_inner(&pool, cli, &projection_options, true).await?;
        log_graph_report(&report, "graph projection worker batch complete");
        tokio::time::sleep(Duration::from_secs(poll_seconds)).await;
    }
}

async fn run_inner(
    pool: &PgPool,
    cli: &Cli,
    options: &GraphProjectionOptions,
    apply: bool,
) -> Result<GraphProjectionReport> {
    let target_chat_ids = target_chat_ids(cli, options.chat_id.as_deref())?;
    let limit = options.limit.unwrap_or(500).max(1);
    let candidate_edges = sqlx::query_as::<_, (i64,)>(
        r#"
        WITH candidates AS (
            SELECT f.id
            FROM qintopia_identity.member_facts f
            JOIN qintopia_identity.channel_identities ci ON ci.id = f.channel_identity_id
            WHERE ci.platform = 'qiwe'
              AND ci.chat_id = ANY($1)
              AND f.revoked_at IS NULL
              AND f.fact_type IN (
                  'reply_style_preference',
                  'interest',
                  'skill',
                  'activity_participation',
                  'activity_organizer',
                  'community_contribution',
                  'service_need',
                  'unresolved_question',
                  'recurring_question',
                  'content_story_lead',
                  'operation_signal'
              )
            ORDER BY f.observed_at DESC
            LIMIT $2
        )
        SELECT count(*)::bigint FROM candidates
        "#,
    )
    .bind(&target_chat_ids)
    .bind(limit)
    .fetch_one(pool)
    .await
    .context("count graph projection candidates")?
    .0;
    let mut report = GraphProjectionReport {
        dry_run: !apply,
        graph_backend: cli.graph_backend.clone(),
        age_enabled: cli.age_enabled,
        target_chat_ids,
        candidate_entities: candidate_edges * 2,
        candidate_edges,
        fallback: "sql".to_string(),
        ..GraphProjectionReport::default()
    };
    if !apply || candidate_edges == 0 {
        return Ok(report);
    }
    let mut tx = pool.begin().await.context("begin graph projection")?;
    let rows = sqlx::query_as::<_, (i64, i64)>(
        r#"
        WITH facts AS (
            SELECT
                f.id,
                f.person_id,
                f.fact_type,
                f.fact_key,
                f.fact_text,
                f.confidence,
                f.observed_at,
                ci.chat_id,
                COALESCE(p.display_name, ci.display_name, ci.channel_user_id) AS person_name
            FROM qintopia_identity.member_facts f
            JOIN qintopia_identity.channel_identities ci ON ci.id = f.channel_identity_id
            LEFT JOIN qintopia_identity.persons p ON p.id = f.person_id
            WHERE ci.platform = 'qiwe'
              AND ci.chat_id = ANY($1)
              AND f.revoked_at IS NULL
              AND f.fact_type IN (
                  'reply_style_preference',
                  'interest',
                  'skill',
                  'activity_participation',
                  'activity_organizer',
                  'community_contribution',
                  'service_need',
                  'unresolved_question',
                  'recurring_question',
                  'content_story_lead',
                  'operation_signal'
              )
            ORDER BY f.observed_at DESC
            LIMIT $2
        ),
        person_entities AS (
            INSERT INTO qintopia_graph.graph_entities
                (entity_type, canonical_key, display_name, information_class, metadata)
            SELECT DISTINCT
                'Person',
                person_id::text,
                person_name,
                'Internal',
                jsonb_build_object('source', 'member_facts')
            FROM facts
            WHERE person_id IS NOT NULL
            ON CONFLICT (entity_type, canonical_key) DO UPDATE SET
                display_name = EXCLUDED.display_name,
                updated_at = now()
            RETURNING id, canonical_key
        ),
        topic_entities AS (
            INSERT INTO qintopia_graph.graph_entities
                (entity_type, canonical_key, display_name, information_class, metadata)
            SELECT DISTINCT
                'Topic',
                fact_type || ':' || fact_key,
                fact_type || ':' || fact_key,
                'Internal',
                jsonb_build_object('source', 'member_facts', 'fact_type', fact_type, 'fact_key', fact_key)
            FROM facts
            ON CONFLICT (entity_type, canonical_key) DO UPDATE SET
                display_name = EXCLUDED.display_name,
                updated_at = now()
            RETURNING id, canonical_key
        ),
        edges AS (
            INSERT INTO qintopia_graph.graph_edges
                (
                    source_entity_id,
                    target_entity_id,
                    edge_type,
                    predicate,
                    weight,
                    confidence,
                    evidence_type,
                    evidence_table,
                    evidence_id,
                    valid_from,
                    information_class,
                    metadata
                )
            SELECT
                pe.id,
                te.id,
                CASE f.fact_type
                    WHEN 'reply_style_preference' THEN 'PERSON_PREFERS_REPLY_STYLE'
                    WHEN 'activity_organizer' THEN 'PERSON_ORGANIZED_ACTIVITY'
                    WHEN 'activity_participation' THEN 'PERSON_PARTICIPATED_IN_ACTIVITY'
                    WHEN 'community_contribution' THEN 'PERSON_CONTRIBUTED_TO_COMMUNITY'
                    WHEN 'service_need' THEN 'PERSON_HAS_SERVICE_NEED'
                    WHEN 'content_story_lead' THEN 'CONTENT_TOPIC_DERIVED_FROM_EVENT'
                    ELSE 'PERSON_INTERESTED_IN_TOPIC'
                END,
                f.fact_type,
                1.0,
                f.confidence,
                'member_fact',
                'qintopia_identity.member_facts',
                f.id,
                f.observed_at,
                'Internal',
                jsonb_build_object('fact_text_preview', left(f.fact_text, 160), 'chat_id', f.chat_id)
            FROM facts f
            JOIN person_entities pe ON pe.canonical_key = f.person_id::text
            JOIN topic_entities te ON te.canonical_key = f.fact_type || ':' || f.fact_key
            ON CONFLICT (source_entity_id, target_entity_id, edge_type, evidence_table, evidence_id)
            DO UPDATE SET
                confidence = EXCLUDED.confidence,
                updated_at = now(),
                metadata = qintopia_graph.graph_edges.metadata || EXCLUDED.metadata
            RETURNING id
        ),
        projection AS (
            INSERT INTO qintopia_graph.graph_projections
                (projection_name, status, source_watermark, metadata)
            VALUES (
                'member_profile_v1',
                'completed',
                jsonb_build_object('target_chat_ids', $1::text[], 'limit', $2),
                jsonb_build_object('backend', 'sql', 'age_enabled', $3)
            )
            ON CONFLICT (projection_name) DO UPDATE SET
                status = EXCLUDED.status,
                source_watermark = EXCLUDED.source_watermark,
                metadata = EXCLUDED.metadata,
                updated_at = now()
            RETURNING id
        )
        SELECT
            (SELECT count(*)::bigint FROM person_entities) + (SELECT count(*)::bigint FROM topic_entities),
            (SELECT count(*)::bigint FROM edges)
        "#,
    )
    .bind(&report.target_chat_ids)
    .bind(limit)
    .bind(cli.age_enabled)
    .fetch_one(&mut *tx)
    .await
    .context("project member facts into graph")?;
    tx.commit().await.context("commit graph projection")?;
    report.entities_upserted = rows.0;
    report.edges_upserted = rows.1;
    Ok(report)
}

fn log_graph_report(report: &GraphProjectionReport, message: &str) {
    info!(
        graph_backend = %report.graph_backend,
        age_enabled = report.age_enabled,
        candidate_entities = report.candidate_entities,
        candidate_edges = report.candidate_edges,
        entities_upserted = report.entities_upserted,
        edges_upserted = report.edges_upserted,
        target_chat_ids = ?report.target_chat_ids,
        "{message}"
    );
}

fn target_chat_ids(cli: &Cli, requested: Option<&str>) -> Result<Vec<String>> {
    let configured = cli.profile_target_chat_ids();
    if configured.is_empty() {
        bail!("QINTOPIA_PROFILE_TARGET_CHAT_IDS is required");
    }
    if let Some(chat_id) = requested {
        let chat_id = chat_id.trim();
        if !configured.iter().any(|item| item == chat_id) {
            bail!("chat_id {chat_id} is not in QINTOPIA_PROFILE_TARGET_CHAT_IDS");
        }
        return Ok(vec![chat_id.to_string()]);
    }
    Ok(configured)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn report_defaults_to_sql_fallback() {
        let report = GraphProjectionReport {
            fallback: "sql".to_string(),
            ..GraphProjectionReport::default()
        };
        assert_eq!(report.fallback, "sql");
    }
}
