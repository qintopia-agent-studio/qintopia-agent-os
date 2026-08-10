use std::path::PathBuf;

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

use crate::{config::Cli, db};

const APPROVAL_PHRASE: &str = "approved-production-erhua-member-safe-alias";
const IDENTITY_APPROVAL_PHRASE: &str = "approved-production-erhua-member-safe-identity";
const ALIAS_TYPE: &str = "reviewed_safe_name";
const ALIAS_SOURCE: &str = "erhua_member_recognition_review";

#[derive(Debug, Clone)]
pub struct SafeAliasOptions {
    pub apply: bool,
    pub dry_run: bool,
    pub payload_json: Option<String>,
    pub payload_file: Option<PathBuf>,
    pub approval: Option<String>,
}

#[derive(Debug, Clone)]
pub struct SafeIdentityOptions {
    pub apply: bool,
    pub dry_run: bool,
    pub payload_json: Option<String>,
    pub payload_file: Option<PathBuf>,
    pub approval: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SafeAliasPayload {
    aliases: Vec<SafeAliasRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SafeIdentityPayload {
    identities: Vec<SafeIdentityRequest>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SafeAliasRequest {
    person_key: String,
    alias: String,
    #[serde(default)]
    source_display_name: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct SafeIdentityRequest {
    identity_key: String,
    safe_display_name: String,
    #[serde(default)]
    person_key: Option<String>,
    #[serde(default)]
    reason: Option<String>,
}

#[derive(Debug, Serialize)]
struct SafeAliasReport {
    dry_run: bool,
    aliases_total: usize,
    aliases_inserted: i64,
    aliases_already_present: i64,
    results: Vec<SafeAliasResult>,
}

#[derive(Debug, Serialize)]
struct SafeIdentityReport {
    dry_run: bool,
    identities_total: usize,
    persons_created: i64,
    identities_linked: i64,
    aliases_inserted: i64,
    platform_identities_materialized: i64,
    messages_updated: i64,
    results: Vec<SafeIdentityResult>,
}

#[derive(Debug, Serialize)]
struct SafeAliasResult {
    person_key: String,
    alias: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    linked_safe_qiwe_identities: i64,
}

#[derive(Debug, Serialize)]
struct SafeIdentityResult {
    identity_key: String,
    safe_display_name: String,
    status: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    person_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
}

struct AliasTarget {
    person_id: Uuid,
    linked_safe_qiwe_identities: i64,
}

struct IdentityTarget {
    identity_id: Uuid,
    person_id: Option<Uuid>,
}

struct SafeIdentityApplyResult {
    person_id: Uuid,
    person_created: bool,
    identity_linked: bool,
    alias_inserted: bool,
    platform_identity_materialized: bool,
    messages_updated: i64,
}

pub async fn run(cli: &Cli, options: SafeAliasOptions) -> Result<()> {
    if options.apply && options.dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let apply = options.apply && !options.dry_run;
    if apply && options.approval.as_deref() != Some(APPROVAL_PHRASE) {
        bail!("Erhua member safe alias owner approval is required");
    }
    let payload = load_payload(&options)?;
    validate_payload(&payload)?;
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let report = run_safe_aliases(&pool, &payload, apply).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub async fn run_safe_identity(cli: &Cli, options: SafeIdentityOptions) -> Result<()> {
    if options.apply && options.dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    let apply = options.apply && !options.dry_run;
    if apply && options.approval.as_deref() != Some(IDENTITY_APPROVAL_PHRASE) {
        bail!("Erhua member safe identity owner approval is required");
    }
    let payload = load_identity_payload(&options)?;
    validate_identity_payload(&payload)?;
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let report = run_safe_identities(&pool, &payload, apply).await?;
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn load_payload(options: &SafeAliasOptions) -> Result<SafeAliasPayload> {
    match (&options.payload_json, &options.payload_file) {
        (Some(_), Some(_)) => bail!("use either --payload-json or --payload-file, not both"),
        (Some(payload), None) => parse_payload(payload),
        (None, Some(path)) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("read safe alias payload {}", path.display()))?;
            parse_payload(&text)
        }
        (None, None) => bail!("safe alias payload is required"),
    }
}

fn parse_payload(text: &str) -> Result<SafeAliasPayload> {
    serde_json::from_str(text).context("parse safe alias payload")
}

fn load_identity_payload(options: &SafeIdentityOptions) -> Result<SafeIdentityPayload> {
    match (&options.payload_json, &options.payload_file) {
        (Some(_), Some(_)) => bail!("use either --payload-json or --payload-file, not both"),
        (Some(payload), None) => parse_identity_payload(payload),
        (None, Some(path)) => {
            let text = std::fs::read_to_string(path)
                .with_context(|| format!("read safe identity payload {}", path.display()))?;
            parse_identity_payload(&text)
        }
        (None, None) => bail!("safe identity payload is required"),
    }
}

fn parse_identity_payload(text: &str) -> Result<SafeIdentityPayload> {
    serde_json::from_str(text).context("parse safe identity payload")
}

fn validate_payload(payload: &SafeAliasPayload) -> Result<()> {
    if payload.aliases.is_empty() {
        bail!("safe alias payload must include at least one alias");
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut seen_aliases = std::collections::BTreeSet::new();
    for item in &payload.aliases {
        validate_person_key(&item.person_key)?;
        validate_safe_alias(&item.alias)?;
        let alias_key = normalize_alias_key(&item.alias);
        let key = (item.person_key.trim().to_lowercase(), alias_key.clone());
        if !seen.insert(key) {
            bail!("safe alias payload contains duplicate person_key and alias");
        }
        if !seen_aliases.insert(alias_key) {
            bail!("safe alias payload contains duplicate alias");
        }
    }
    Ok(())
}

fn validate_identity_payload(payload: &SafeIdentityPayload) -> Result<()> {
    if payload.identities.is_empty() {
        bail!("safe identity payload must include at least one identity");
    }
    let mut seen = std::collections::BTreeSet::new();
    let mut seen_identity_keys = std::collections::BTreeSet::new();
    let mut seen_safe_names = std::collections::BTreeSet::new();
    for item in &payload.identities {
        validate_identity_key(&item.identity_key)?;
        validate_safe_alias(&item.safe_display_name)?;
        if let Some(person_key) = item.person_key.as_deref() {
            validate_person_key(person_key)?;
        }
        let identity_key = item.identity_key.trim().to_lowercase();
        let safe_name_key = normalize_alias_key(&item.safe_display_name);
        let key = (identity_key.clone(), safe_name_key.clone());
        if !seen.insert(key) {
            bail!("safe identity payload contains duplicate identity_key and safe_display_name");
        }
        if !seen_identity_keys.insert(identity_key) {
            bail!("safe identity payload contains duplicate identity_key");
        }
        if !seen_safe_names.insert(safe_name_key) {
            bail!("safe identity payload contains duplicate safe_display_name");
        }
    }
    Ok(())
}

async fn run_safe_aliases(
    pool: &PgPool,
    payload: &SafeAliasPayload,
    apply: bool,
) -> Result<SafeAliasReport> {
    let mut results = Vec::with_capacity(payload.aliases.len());
    let mut aliases_inserted = 0;
    let mut aliases_already_present = 0;
    for item in &payload.aliases {
        let person_key = item.person_key.trim().to_lowercase();
        let alias = normalize_alias(&item.alias);
        let target = load_alias_target(pool, &person_key).await?;
        ensure_alias_is_unique(pool, target.person_id, &alias).await?;
        let already_present = alias_already_present(pool, target.person_id, &alias).await?;
        if already_present {
            aliases_already_present += 1;
            results.push(SafeAliasResult {
                person_key,
                alias,
                status: "already_present".to_string(),
                reason: None,
                linked_safe_qiwe_identities: target.linked_safe_qiwe_identities,
            });
            continue;
        }
        if apply {
            insert_safe_alias(pool, target.person_id, item, &alias).await?;
            aliases_inserted += 1;
            results.push(SafeAliasResult {
                person_key,
                alias,
                status: "inserted".to_string(),
                reason: None,
                linked_safe_qiwe_identities: target.linked_safe_qiwe_identities,
            });
        } else {
            results.push(SafeAliasResult {
                person_key,
                alias,
                status: "would_insert".to_string(),
                reason: item
                    .reason
                    .as_ref()
                    .map(|value| redact_sensitive_text(value)),
                linked_safe_qiwe_identities: target.linked_safe_qiwe_identities,
            });
        }
    }
    Ok(SafeAliasReport {
        dry_run: !apply,
        aliases_total: payload.aliases.len(),
        aliases_inserted,
        aliases_already_present,
        results,
    })
}

async fn run_safe_identities(
    pool: &PgPool,
    payload: &SafeIdentityPayload,
    apply: bool,
) -> Result<SafeIdentityReport> {
    let mut results = Vec::with_capacity(payload.identities.len());
    let mut persons_created = 0;
    let mut identities_linked = 0;
    let mut aliases_inserted = 0;
    let mut platform_identities_materialized = 0;
    let mut messages_updated = 0;

    for item in &payload.identities {
        let identity_key = item.identity_key.trim().to_lowercase();
        let safe_display_name = normalize_alias(&item.safe_display_name);
        let target = load_identity_target(pool, &identity_key).await?;
        let requested_person_id = match item.person_key.as_deref() {
            Some(person_key) => Some(load_person_id_by_key(pool, person_key).await?),
            None => None,
        };
        let qiwe_user_person_id = load_unique_qiwe_user_person(pool, target.identity_id).await?;
        let person_id = resolve_safe_identity_person(
            target.person_id,
            requested_person_id,
            qiwe_user_person_id,
        )?;

        if let Some(person_id) = person_id {
            ensure_alias_is_unique(pool, person_id, &safe_display_name).await?;
        } else {
            ensure_alias_available_for_new_person(pool, &safe_display_name).await?;
        }

        if !apply {
            results.push(SafeIdentityResult {
                identity_key,
                safe_display_name,
                status: dry_run_safe_identity_status(target.person_id, person_id).to_string(),
                person_key: match person_id {
                    Some(person_id) => Some(load_person_key_by_id(pool, person_id).await?),
                    None => None,
                },
                reason: item
                    .reason
                    .as_ref()
                    .map(|value| redact_sensitive_text(value)),
            });
            continue;
        }

        let applied = apply_safe_identity(
            pool,
            item,
            &target,
            person_id,
            &identity_key,
            &safe_display_name,
        )
        .await?;
        persons_created += bool_count(applied.person_created);
        identities_linked += bool_count(applied.identity_linked);
        aliases_inserted += bool_count(applied.alias_inserted);
        platform_identities_materialized += bool_count(applied.platform_identity_materialized);
        messages_updated += applied.messages_updated;
        results.push(SafeIdentityResult {
            identity_key,
            safe_display_name,
            status: applied.status().to_string(),
            person_key: Some(load_person_key_by_id(pool, applied.person_id).await?),
            reason: None,
        });
    }

    Ok(SafeIdentityReport {
        dry_run: !apply,
        identities_total: payload.identities.len(),
        persons_created,
        identities_linked,
        aliases_inserted,
        platform_identities_materialized,
        messages_updated,
        results,
    })
}

impl SafeIdentityApplyResult {
    fn status(&self) -> &'static str {
        if self.person_created && self.identity_linked {
            "created_person_linked"
        } else if self.identity_linked {
            "linked_existing_person"
        } else if self.alias_inserted {
            "alias_inserted"
        } else {
            "already_present"
        }
    }
}

fn bool_count(value: bool) -> i64 {
    if value {
        1
    } else {
        0
    }
}

async fn load_identity_target(pool: &PgPool, identity_key: &str) -> Result<IdentityTarget> {
    let rows = sqlx::query(
        r#"
        SELECT ci.id, ci.person_id
        FROM qintopia_identity.channel_identities ci
        WHERE md5(ci.id::text) LIKE $1 || '%'
          AND ci.platform = 'qiwe'
          AND ci.chat_id <> ''
          AND ci.metadata->>'current_qiwe_room_member' = 'true'
          AND COALESCE(ci.is_bot, false) = false
          AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
          AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
        ORDER BY ci.id
        LIMIT 2
        "#,
    )
    .bind(identity_key)
    .fetch_all(pool)
    .await
    .context("load safe identity target")?;
    if rows.is_empty() {
        bail!("safe identity identity_key did not match one current QiWe room member identity");
    }
    if rows.len() > 1 {
        bail!("safe identity identity_key is ambiguous");
    }
    Ok(IdentityTarget {
        identity_id: rows[0].try_get("id")?,
        person_id: rows[0].try_get("person_id")?,
    })
}

async fn load_person_id_by_key(pool: &PgPool, person_key: &str) -> Result<Uuid> {
    let person_key = person_key.trim().to_lowercase();
    let rows = sqlx::query(
        r#"
        SELECT p.id
        FROM qintopia_identity.persons p
        WHERE md5(p.id::text) LIKE $1 || '%'
        ORDER BY p.id
        LIMIT 2
        "#,
    )
    .bind(&person_key)
    .fetch_all(pool)
    .await
    .context("load safe identity target person")?;
    if rows.is_empty() {
        bail!("safe identity person_key did not match any person");
    }
    if rows.len() > 1 {
        bail!("safe identity person_key is ambiguous");
    }
    rows[0]
        .try_get("id")
        .context("read safe identity person id")
}

async fn load_person_key_by_id(pool: &PgPool, person_id: Uuid) -> Result<String> {
    let row = sqlx::query_as::<_, (String,)>(
        r#"
        SELECT md5($1::uuid::text)
        "#,
    )
    .bind(person_id)
    .fetch_one(pool)
    .await
    .context("load safe identity person key")?;
    Ok(row.0)
}

async fn load_unique_qiwe_user_person(pool: &PgPool, identity_id: Uuid) -> Result<Option<Uuid>> {
    let rows = sqlx::query(
        r#"
        WITH target AS (
            SELECT platform, channel_user_id
            FROM qintopia_identity.channel_identities
            WHERE id = $1
              AND platform = 'qiwe'
              AND COALESCE(channel_user_id, '') <> ''
        )
        SELECT ci.person_id
        FROM qintopia_identity.channel_identities ci
        JOIN target
          ON target.platform = ci.platform
         AND target.channel_user_id = ci.channel_user_id
        WHERE ci.person_id IS NOT NULL
        GROUP BY ci.person_id
        ORDER BY ci.person_id
        LIMIT 2
        "#,
    )
    .bind(identity_id)
    .fetch_all(pool)
    .await
    .context("load unique QiWe user person for safe identity")?;
    if rows.len() > 1 {
        bail!("safe identity target QiWe user already maps to multiple people");
    }
    rows.first()
        .map(|row| row.try_get("person_id").context("read QiWe user person id"))
        .transpose()
}

fn resolve_safe_identity_person(
    identity_person_id: Option<Uuid>,
    requested_person_id: Option<Uuid>,
    qiwe_user_person_id: Option<Uuid>,
) -> Result<Option<Uuid>> {
    let mut resolved = None;
    for candidate in [identity_person_id, requested_person_id, qiwe_user_person_id]
        .into_iter()
        .flatten()
    {
        if let Some(existing) = resolved {
            if existing != candidate {
                bail!("safe identity target already resolves to a different person");
            }
        } else {
            resolved = Some(candidate);
        }
    }
    Ok(resolved)
}

fn dry_run_safe_identity_status(
    identity_person_id: Option<Uuid>,
    person_id: Option<Uuid>,
) -> &'static str {
    match (identity_person_id, person_id) {
        (Some(_), Some(_)) => "would_verify_existing_link",
        (None, Some(_)) => "would_link_existing_person",
        (None, None) => "would_create_person_and_link",
        (Some(_), None) => "would_verify_existing_link",
    }
}

async fn ensure_alias_available_for_new_person(pool: &PgPool, alias: &str) -> Result<()> {
    let conflict: (i64,) = sqlx::query_as(
        r#"
        WITH matches AS (
            SELECT p.id AS person_id
            FROM qintopia_identity.persons p
            WHERE lower(regexp_replace(btrim(COALESCE(p.display_name, '')), '[[:space:]]+', ' ', 'g')) = lower($1)
               OR lower(regexp_replace(btrim(COALESCE(p.primary_name, '')), '[[:space:]]+', ' ', 'g')) = lower($1)
               OR lower(regexp_replace(btrim(COALESCE(p.preferred_name, '')), '[[:space:]]+', ' ', 'g')) = lower($1)

            UNION

            SELECT a.person_id
            FROM qintopia_identity.person_aliases a
            WHERE lower(regexp_replace(btrim(COALESCE(a.alias, '')), '[[:space:]]+', ' ', 'g')) = lower($1)

            UNION

            SELECT ci.person_id
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND ci.person_id IS NOT NULL
              AND COALESCE(ci.is_bot, false) = false
              AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
              AND (
                  lower(ci.normalized_display_name) = lower($1)
                  OR lower(regexp_replace(btrim(COALESCE(ci.display_name, '')), '[[:space:]]+', ' ', 'g')) = lower($1)
              )
        )
        SELECT count(DISTINCT person_id)::bigint
        FROM matches
        "#,
    )
    .bind(alias)
    .fetch_one(pool)
    .await
    .context("check safe identity new person alias uniqueness")?;
    if conflict.0 > 0 {
        bail!("safe identity safe_display_name already resolves to an existing person");
    }
    Ok(())
}

async fn apply_safe_identity(
    pool: &PgPool,
    request: &SafeIdentityRequest,
    target: &IdentityTarget,
    person_id: Option<Uuid>,
    identity_key: &str,
    safe_display_name: &str,
) -> Result<SafeIdentityApplyResult> {
    let mut tx = pool
        .begin()
        .await
        .context("begin safe identity transaction")?;
    let (person_id, person_created) = match person_id {
        Some(person_id) => (person_id, false),
        None => (
            create_safe_identity_person(&mut tx, request, target.identity_id, safe_display_name)
                .await?,
            true,
        ),
    };
    let identity_linked = link_safe_identity(&mut tx, target.identity_id, person_id).await?;
    let alias_preexisting = alias_already_present_tx(&mut tx, person_id, safe_display_name).await?;
    if !alias_preexisting {
        insert_safe_identity_alias(&mut tx, person_id, request, identity_key, safe_display_name)
            .await?;
    }
    let platform_identity_materialized =
        materialize_safe_identity_platform_identity(&mut tx, target.identity_id, person_id).await?
            > 0;
    let messages_updated =
        backfill_safe_identity_messages(&mut tx, target.identity_id, person_id).await?;
    tx.commit()
        .await
        .context("commit safe identity transaction")?;
    Ok(SafeIdentityApplyResult {
        person_id,
        person_created,
        identity_linked,
        alias_inserted: !alias_preexisting,
        platform_identity_materialized,
        messages_updated,
    })
}

async fn create_safe_identity_person(
    tx: &mut Transaction<'_, Postgres>,
    request: &SafeIdentityRequest,
    identity_id: Uuid,
    safe_display_name: &str,
) -> Result<Uuid> {
    let row = sqlx::query_as::<_, (Uuid,)>(
        r#"
        INSERT INTO qintopia_identity.persons
            (display_name, primary_name, preferred_name, metadata)
        VALUES (
            $1,
            $1,
            $1,
            jsonb_build_object(
                'bootstrap_source', 'erhua_member_recognition_safe_identity',
                'review_boundary', 'erhua_member_recognition_safe_identity',
                'channel_identity_id', $2::text,
                'review_reason', $3::text,
                'person_merge_status', 'unmerged'
            )
        )
        RETURNING id
        "#,
    )
    .bind(safe_display_name)
    .bind(identity_id)
    .bind(
        request
            .reason
            .as_deref()
            .map(redact_sensitive_text)
            .unwrap_or_default(),
    )
    .fetch_one(&mut **tx)
    .await
    .context("create safe identity person")?;
    Ok(row.0)
}

async fn link_safe_identity(
    tx: &mut Transaction<'_, Postgres>,
    identity_id: Uuid,
    person_id: Uuid,
) -> Result<bool> {
    let result = sqlx::query(
        r#"
        UPDATE qintopia_identity.channel_identities
        SET person_id = $2,
            metadata = metadata || jsonb_build_object(
                'review_boundary', 'erhua_member_recognition_safe_identity',
                'reviewed_safe_identity_linked_at', now()
            ),
            updated_at = now()
        WHERE id = $1
          AND (person_id IS NULL OR person_id = $2)
          AND person_id IS DISTINCT FROM $2
        "#,
    )
    .bind(identity_id)
    .bind(person_id)
    .execute(&mut **tx)
    .await
    .context("link safe identity to person")?;
    let linked = result.rows_affected() > 0;
    let current = sqlx::query_as::<_, (Option<Uuid>,)>(
        r#"
        SELECT person_id
        FROM qintopia_identity.channel_identities
        WHERE id = $1
        "#,
    )
    .bind(identity_id)
    .fetch_one(&mut **tx)
    .await
    .context("verify safe identity link")?;
    if current.0 != Some(person_id) {
        bail!("safe identity target could not be linked to reviewed person");
    }
    Ok(linked)
}

async fn alias_already_present_tx(
    tx: &mut Transaction<'_, Postgres>,
    person_id: Uuid,
    alias: &str,
) -> Result<bool> {
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT count(*)::bigint
        FROM qintopia_identity.person_aliases a
        WHERE a.person_id = $1
          AND a.alias_type = $3
          AND lower(regexp_replace(btrim(COALESCE(a.alias, '')), '[[:space:]]+', ' ', 'g')) = lower($2)
        "#,
    )
    .bind(person_id)
    .bind(alias)
    .bind(ALIAS_TYPE)
    .fetch_one(&mut **tx)
    .await
    .context("check existing safe identity alias")?;
    Ok(count.0 > 0)
}

async fn insert_safe_identity_alias(
    tx: &mut Transaction<'_, Postgres>,
    person_id: Uuid,
    request: &SafeIdentityRequest,
    identity_key: &str,
    safe_display_name: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_identity.person_aliases
            (person_id, alias, alias_type, source, confidence, metadata)
        VALUES (
            $1,
            $2,
            $3,
            $4,
            1.0,
            jsonb_build_object(
                'review_boundary', 'erhua_member_recognition_safe_identity',
                'channel_identity_key', $5::text,
                'review_reason', $6::text
            )
        )
        ON CONFLICT (person_id, alias, alias_type) DO UPDATE SET
            source = EXCLUDED.source,
            confidence = GREATEST(qintopia_identity.person_aliases.confidence, EXCLUDED.confidence),
            metadata = qintopia_identity.person_aliases.metadata || EXCLUDED.metadata
        "#,
    )
    .bind(person_id)
    .bind(safe_display_name)
    .bind(ALIAS_TYPE)
    .bind(ALIAS_SOURCE)
    .bind(identity_key)
    .bind(
        request
            .reason
            .as_deref()
            .map(redact_sensitive_text)
            .unwrap_or_default(),
    )
    .execute(&mut **tx)
    .await
    .context("insert reviewed safe identity alias")?;
    Ok(())
}

async fn materialize_safe_identity_platform_identity(
    tx: &mut Transaction<'_, Postgres>,
    identity_id: Uuid,
    person_id: Uuid,
) -> Result<i64> {
    let row = sqlx::query_as::<_, (i64,)>(
        r#"
        WITH source_identity AS (
            SELECT ci.*
            FROM qintopia_identity.channel_identities ci
            WHERE ci.id = $1
              AND ci.platform = 'qiwe'
              AND ci.person_id = $2
              AND COALESCE(ci.channel_user_id, '') <> ''
        ),
        person_candidates AS (
            SELECT ci.person_id
            FROM qintopia_identity.channel_identities ci
            JOIN source_identity source
              ON source.platform = ci.platform
             AND source.channel_user_id = ci.channel_user_id
            WHERE ci.person_id IS NOT NULL
            GROUP BY ci.person_id
        ),
        unique_person AS (
            SELECT $2::uuid AS person_id
            WHERE (SELECT count(*) FROM person_candidates) = 1
              AND EXISTS (
                  SELECT 1
                  FROM person_candidates
                  WHERE person_id = $2
              )
        ),
        upserted AS (
            INSERT INTO qintopia_identity.channel_identities
                (
                    person_id,
                    platform,
                    channel_user_id,
                    chat_id,
                    display_name,
                    normalized_display_name,
                    identity_source,
                    confidence,
                    first_seen_at,
                    last_seen_at,
                    metadata
                )
            SELECT
                source.person_id,
                source.platform,
                source.channel_user_id,
                '',
                source.display_name,
                source.normalized_display_name,
                source.identity_source,
                source.confidence,
                source.first_seen_at,
                source.last_seen_at,
                source.metadata
                    || jsonb_build_object(
                        'identity_scope', 'qiwe_platform_user',
                        'materialized_from_channel_identity_id', source.id::text,
                        'materialized_at', now()
                    )
            FROM source_identity source
            JOIN unique_person ON unique_person.person_id = source.person_id
            ON CONFLICT (platform, channel_user_id, chat_id) DO UPDATE SET
                person_id = EXCLUDED.person_id,
                display_name = CASE
                    WHEN qintopia_identity.channel_identities.person_id IS NULL
                      OR qintopia_identity.identity_source_rank(EXCLUDED.identity_source) >= qintopia_identity.identity_source_rank(qintopia_identity.channel_identities.identity_source)
                    THEN EXCLUDED.display_name
                    ELSE qintopia_identity.channel_identities.display_name
                END,
                normalized_display_name = CASE
                    WHEN qintopia_identity.channel_identities.person_id IS NULL
                      OR qintopia_identity.identity_source_rank(EXCLUDED.identity_source) >= qintopia_identity.identity_source_rank(qintopia_identity.channel_identities.identity_source)
                    THEN EXCLUDED.normalized_display_name
                    ELSE qintopia_identity.channel_identities.normalized_display_name
                END,
                identity_source = CASE
                    WHEN qintopia_identity.channel_identities.person_id IS NULL
                      OR qintopia_identity.identity_source_rank(EXCLUDED.identity_source) >= qintopia_identity.identity_source_rank(qintopia_identity.channel_identities.identity_source)
                    THEN EXCLUDED.identity_source
                    ELSE qintopia_identity.channel_identities.identity_source
                END,
                confidence = GREATEST(qintopia_identity.channel_identities.confidence, EXCLUDED.confidence),
                last_seen_at = GREATEST(qintopia_identity.channel_identities.last_seen_at, EXCLUDED.last_seen_at),
                metadata = qintopia_identity.channel_identities.metadata || EXCLUDED.metadata,
                updated_at = now()
            WHERE qintopia_identity.channel_identities.person_id IS NULL
               OR qintopia_identity.channel_identities.person_id = EXCLUDED.person_id
            RETURNING id
        )
        SELECT count(*)::bigint FROM upserted
        "#,
    )
    .bind(identity_id)
    .bind(person_id)
    .fetch_one(&mut **tx)
    .await
    .context("materialize safe identity QiWe platform identity")?;
    Ok(row.0)
}

async fn backfill_safe_identity_messages(
    tx: &mut Transaction<'_, Postgres>,
    identity_id: Uuid,
    person_id: Uuid,
) -> Result<i64> {
    let result = sqlx::query(
        r#"
        UPDATE qintopia_messages.messages
        SET sender_person_id = $2,
            updated_at = now()
        WHERE sender_channel_identity_id = $1
          AND sender_person_id IS NULL
        "#,
    )
    .bind(identity_id)
    .bind(person_id)
    .execute(&mut **tx)
    .await
    .context("backfill safe identity sender_person_id")?;
    i64::try_from(result.rows_affected()).context("safe identity message update overflow")
}

async fn load_alias_target(pool: &PgPool, person_key: &str) -> Result<AliasTarget> {
    let rows = sqlx::query(
        r#"
        SELECT
            p.id,
            (
                SELECT count(*)::bigint
                FROM qintopia_identity.channel_identities ci
                WHERE ci.person_id = p.id
                  AND ci.platform = 'qiwe'
                  AND COALESCE(ci.is_bot, false) = false
                  AND COALESCE(ci.display_name, '') !~ '[0-9]{7,}'
                  AND COALESCE(ci.display_name, '') !~ '[[:cntrl:]]'
                  AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
                  AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
            ) AS linked_safe_qiwe_identities
        FROM qintopia_identity.persons p
        WHERE md5(p.id::text) LIKE $1 || '%'
        ORDER BY p.id
        LIMIT 2
        "#,
    )
    .bind(person_key)
    .fetch_all(pool)
    .await
    .context("load safe alias target person")?;
    if rows.is_empty() {
        bail!("safe alias person_key did not match any person");
    }
    if rows.len() > 1 {
        bail!("safe alias person_key is ambiguous");
    }
    let row = &rows[0];
    let linked_safe_qiwe_identities: i64 = row.try_get("linked_safe_qiwe_identities")?;
    if linked_safe_qiwe_identities <= 0 {
        bail!("safe alias target is not linked to a safe QiWe identity");
    }
    Ok(AliasTarget {
        person_id: row.try_get("id")?,
        linked_safe_qiwe_identities,
    })
}

async fn ensure_alias_is_unique(pool: &PgPool, person_id: Uuid, alias: &str) -> Result<()> {
    let conflict: (i64,) = sqlx::query_as(
        r#"
        WITH other_matches AS (
            SELECT p.id AS person_id
            FROM qintopia_identity.persons p
            WHERE p.id <> $1
              AND (
                  lower(regexp_replace(btrim(COALESCE(p.display_name, '')), '[[:space:]]+', ' ', 'g')) = lower($2)
                  OR lower(regexp_replace(btrim(COALESCE(p.primary_name, '')), '[[:space:]]+', ' ', 'g')) = lower($2)
                  OR lower(regexp_replace(btrim(COALESCE(p.preferred_name, '')), '[[:space:]]+', ' ', 'g')) = lower($2)
              )

            UNION

            SELECT a.person_id
            FROM qintopia_identity.person_aliases a
            WHERE a.person_id <> $1
              AND lower(regexp_replace(btrim(COALESCE(a.alias, '')), '[[:space:]]+', ' ', 'g')) = lower($2)

            UNION

            SELECT ci.person_id
            FROM qintopia_identity.channel_identities ci
            WHERE ci.person_id <> $1
              AND ci.platform = 'qiwe'
              AND ci.person_id IS NOT NULL
              AND COALESCE(ci.is_bot, false) = false
              AND COALESCE(ci.display_name, '') !~ '[0-9]{7,}'
              AND COALESCE(ci.display_name, '') !~ '[[:cntrl:]]'
              AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
              AND (
                  lower(ci.normalized_display_name) = lower($2)
                  OR lower(regexp_replace(btrim(COALESCE(ci.display_name, '')), '[[:space:]]+', ' ', 'g')) = lower($2)
              )
        )
        SELECT count(DISTINCT person_id)::bigint
        FROM other_matches
        "#,
    )
    .bind(person_id)
    .bind(alias)
    .fetch_one(pool)
    .await
    .context("check safe alias uniqueness")?;
    if conflict.0 > 0 {
        bail!("safe alias already resolves to another person");
    }
    Ok(())
}

async fn alias_already_present(pool: &PgPool, person_id: Uuid, alias: &str) -> Result<bool> {
    let count: (i64,) = sqlx::query_as(
        r#"
        SELECT count(*)::bigint
        FROM qintopia_identity.person_aliases a
        WHERE a.person_id = $1
          AND a.alias_type = $3
          AND lower(regexp_replace(btrim(COALESCE(a.alias, '')), '[[:space:]]+', ' ', 'g')) = lower($2)
        "#,
    )
    .bind(person_id)
    .bind(alias)
    .bind(ALIAS_TYPE)
    .fetch_one(pool)
    .await
    .context("check existing safe alias")?;
    Ok(count.0 > 0)
}

async fn insert_safe_alias(
    pool: &PgPool,
    person_id: Uuid,
    request: &SafeAliasRequest,
    alias: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_identity.person_aliases
            (person_id, alias, alias_type, source, confidence, metadata)
        VALUES (
            $1,
            $2,
            $3,
            $4,
            1.0,
            jsonb_build_object(
                'review_boundary', 'erhua_member_recognition_safe_alias',
                'source_display_name', $5::text,
                'review_reason', $6::text
            )
        )
        ON CONFLICT (person_id, alias, alias_type) DO UPDATE SET
            source = EXCLUDED.source,
            confidence = GREATEST(qintopia_identity.person_aliases.confidence, EXCLUDED.confidence),
            metadata = qintopia_identity.person_aliases.metadata || EXCLUDED.metadata
        "#,
    )
    .bind(person_id)
    .bind(alias)
    .bind(ALIAS_TYPE)
    .bind(ALIAS_SOURCE)
    .bind(
        request
            .source_display_name
            .as_deref()
            .map(redact_sensitive_text)
            .unwrap_or_default(),
    )
    .bind(
        request
            .reason
            .as_deref()
            .map(redact_sensitive_text)
            .unwrap_or_default(),
    )
    .execute(pool)
    .await
    .context("insert reviewed safe member alias")?;
    Ok(())
}

fn validate_person_key(value: &str) -> Result<()> {
    let value = value.trim();
    if !(12..=32).contains(&value.len()) || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("person_key must be a 12-32 character md5 hex prefix");
    }
    Ok(())
}

fn validate_identity_key(value: &str) -> Result<()> {
    let value = value.trim();
    if !(12..=32).contains(&value.len()) || !value.chars().all(|ch| ch.is_ascii_hexdigit()) {
        bail!("identity_key must be a 12-32 character md5 hex prefix");
    }
    Ok(())
}

fn validate_safe_alias(value: &str) -> Result<()> {
    let alias = normalize_alias(value);
    let count = alias.chars().count();
    if !(2..=40).contains(&count) {
        bail!("safe alias must be 2-40 characters");
    }
    if alias.chars().any(char::is_control) {
        bail!("safe alias must not contain control characters");
    }
    if alias.chars().all(|ch| ch.is_ascii_digit()) {
        bail!("safe alias must not be numeric-only");
    }
    if has_sensitive_digit_run(&alias) {
        bail!("safe alias must not contain phone-like digit runs");
    }
    if alias == "企业微信团队"
        || alias == "秦托邦小客服"
        || alias == "二花"
        || alias.eq_ignore_ascii_case("sidecar smoke")
    {
        bail!("safe alias must not be a system or test display name");
    }
    Ok(())
}

fn normalize_alias(value: &str) -> String {
    value.split_whitespace().collect::<Vec<_>>().join(" ")
}

fn normalize_alias_key(value: &str) -> String {
    normalize_alias(value).to_lowercase()
}

fn redact_sensitive_text(text: &str) -> String {
    let mut out = String::new();
    let mut digit_run = String::new();
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            digit_run.push(ch);
            continue;
        }
        flush_digit_run(&mut out, &mut digit_run);
        if !ch.is_control() {
            out.push(ch);
        }
    }
    flush_digit_run(&mut out, &mut digit_run);
    out
}

fn has_sensitive_digit_run(text: &str) -> bool {
    let mut count = 0;
    for ch in text.chars() {
        if ch.is_ascii_digit() {
            count += 1;
            if count >= 7 {
                return true;
            }
        } else {
            count = 0;
        }
    }
    false
}

fn flush_digit_run(out: &mut String, digit_run: &mut String) {
    if digit_run.is_empty() {
        return;
    }
    if digit_run.len() >= 7 {
        out.push_str("[敏感数字]");
    } else {
        out.push_str(digit_run);
    }
    digit_run.clear();
}

#[cfg(test)]
mod tests {
    use super::{
        normalize_alias, normalize_alias_key, parse_identity_payload, parse_payload,
        redact_sensitive_text, validate_identity_key, validate_identity_payload, validate_payload,
        validate_person_key, validate_safe_alias,
    };

    #[test]
    fn safe_alias_accepts_reviewed_human_names() {
        validate_safe_alias("小白君").expect("Chinese alias should be accepted");
        validate_safe_alias("Paxon").expect("ASCII alias should be accepted");
        assert_eq!(normalize_alias(" 小白   君 "), "小白 君");
        assert_eq!(normalize_alias_key(" PAXON "), "paxon");
    }

    #[test]
    fn safe_alias_rejects_unsafe_display_text() {
        assert!(validate_safe_alias("000").is_err());
        assert!(validate_safe_alias("Joey17336786728").is_err());
        assert!(validate_safe_alias("企业微信团队").is_err());
        assert!(validate_safe_alias("二花").is_err());
        assert!(validate_safe_alias("Sidecar Smoke").is_err());
        assert!(validate_safe_alias("A\u{0007}B").is_err());
    }

    #[test]
    fn person_key_requires_md5_prefix() {
        validate_person_key("fc2c1a46c0af").expect("12-char md5 prefix should pass");
        assert!(validate_person_key("fc2").is_err());
        assert!(validate_person_key("not-a-hex-key").is_err());
    }

    #[test]
    fn identity_key_requires_md5_prefix() {
        validate_identity_key("ab2c1a46c0af").expect("12-char md5 prefix should pass");
        assert!(validate_identity_key("ab2").is_err());
        assert!(validate_identity_key("not-a-hex-key").is_err());
    }

    #[test]
    fn alias_metadata_redacts_sensitive_text() {
        assert_eq!(
            redact_sensitive_text("review Joey17336786728\u{0091}"),
            "review Joey[敏感数字]"
        );
    }

    #[test]
    fn payload_rejects_unknown_sensitive_fields() {
        let error = parse_payload(
            r#"{"aliases":[{"person_key":"fc2c1a46c0af","alias":"小白君","channel_user_id":"secret"}]}"#,
        )
        .expect_err("unknown fields must be rejected");
        assert!(format!("{error:#}").contains("unknown field"));
    }

    #[test]
    fn payload_rejects_duplicate_alias_entries() {
        let payload = parse_payload(
            r#"{"aliases":[{"person_key":"fc2c1a46c0af","alias":"Paxon"},{"person_key":"FC2C1A46C0AF","alias":" paxon "}]}"#,
        )
        .expect("payload should parse");
        let error = validate_payload(&payload).expect_err("duplicate entries should fail");
        assert!(format!("{error:#}").contains("duplicate"));
    }

    #[test]
    fn payload_rejects_duplicate_alias_across_people() {
        let payload = parse_payload(
            r#"{"aliases":[{"person_key":"fc2c1a46c0af","alias":"Paxon"},{"person_key":"ab2c1a46c0af","alias":" paxon "}]}"#,
        )
        .expect("payload should parse");
        let error = validate_payload(&payload).expect_err("duplicate aliases should fail");
        assert!(format!("{error:#}").contains("duplicate alias"));
    }

    #[test]
    fn identity_payload_rejects_unknown_sensitive_fields() {
        let error = parse_identity_payload(
            r#"{"identities":[{"identity_key":"ab2c1a46c0af","safe_display_name":"Paxon","channel_user_id":"secret"}]}"#,
        )
        .expect_err("unknown fields must be rejected");
        assert!(format!("{error:#}").contains("unknown field"));
    }

    #[test]
    fn identity_payload_rejects_duplicate_entries() {
        let payload = parse_identity_payload(
            r#"{"identities":[{"identity_key":"ab2c1a46c0af","safe_display_name":"Paxon"},{"identity_key":"AB2C1A46C0AF","safe_display_name":" paxon "}]}"#,
        )
        .expect("payload should parse");
        let error = validate_identity_payload(&payload).expect_err("duplicate entries should fail");
        assert!(format!("{error:#}").contains("duplicate"));
    }

    #[test]
    fn identity_payload_rejects_duplicate_safe_names() {
        let payload = parse_identity_payload(
            r#"{"identities":[{"identity_key":"ab2c1a46c0af","safe_display_name":"Paxon"},{"identity_key":"cd2c1a46c0af","safe_display_name":" paxon "}]}"#,
        )
        .expect("payload should parse");
        let error =
            validate_identity_payload(&payload).expect_err("duplicate safe names should fail");
        assert!(format!("{error:#}").contains("duplicate safe_display_name"));
    }
}
