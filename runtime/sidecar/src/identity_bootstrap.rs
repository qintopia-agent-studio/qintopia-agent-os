use anyhow::{Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::postgres::PgPool;
use std::collections::{BTreeMap, BTreeSet};

use crate::{config::Cli, db};

const COVERAGE_SAMPLE_LIMIT: i64 = 10;
const COVERAGE_SAMPLE_CANDIDATE_LIMIT: i64 = COVERAGE_SAMPLE_LIMIT * 10;

const ACTION_CREATE_PERSON: &str = "create_person";
const ACTION_REUSE_EXISTING_QIWE_USER: &str = "reuse_existing_qiwe_user";
const ACTION_REUSE_UNIQUE_DISPLAY_NAME_OR_ALIAS: &str = "reuse_unique_display_name_or_alias";
const ACTION_MANUAL_MERGE_MULTIPLE_QIWE_USER_PEOPLE: &str =
    "manual_merge_multiple_qiwe_user_people";
const ACTION_MANUAL_MERGE_AMBIGUOUS_DISPLAY_NAME_OR_ALIAS: &str =
    "manual_merge_ambiguous_display_name_or_alias";

#[derive(Debug, Clone)]
pub struct BootstrapOptions {
    pub apply: bool,
    pub dry_run: bool,
    pub chat_id: Option<String>,
    pub limit: Option<i64>,
}

#[derive(Debug, Default, Serialize)]
struct BootstrapReport {
    #[serde(skip_serializing_if = "Option::is_none")]
    scope_fingerprint: Option<String>,
    qiwe_channel_identities_raw_total: i64,
    qiwe_room_channel_identities_raw_total: i64,
    qiwe_room_channel_identities_total: i64,
    qiwe_room_channel_identities_linked: i64,
    qiwe_room_channel_identities_excluded: i64,
    qiwe_room_potential_member_identities_total: i64,
    qiwe_room_potential_member_identities_linked: i64,
    qiwe_room_potential_member_identities_unlinked: i64,
    total_channel_identities: i64,
    qiwe_channel_identities_total: i64,
    qiwe_channel_identities_linked: i64,
    qiwe_channel_identities_excluded: i64,
    channel_identities_with_existing_person: i64,
    channel_identities_with_existing_name: i64,
    ambiguous_channel_identities_skipped: i64,
    linked_aliases_missing: i64,
    linked_messages_missing_sender_person: i64,
    linked_people_total: i64,
    linked_people_with_active_profile: i64,
    linked_people_without_active_profile: i64,
    qiwe_platform_identity_materializable_users: i64,
    qiwe_platform_identities_missing: i64,
    qiwe_platform_identity_ambiguous_users: i64,
    linked_people_without_qiwe_platform_identity: i64,
    linked_people_with_running_facts: i64,
    running_people_with_profile_running_hint: i64,
    running_people_profile_missing_running_hint: i64,
    answer_context_canary_specs_total: i64,
    answer_context_canary_people_total: i64,
    answer_context_speaker_canary_specs_total: i64,
    answer_context_speaker_canary_people_total: i64,
    answer_context_referenced_canary_specs_total: i64,
    answer_context_referenced_canary_people_total: i64,
    linked_people_without_answer_context_canary_spec: i64,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    unlinked_channel_identity_samples: Vec<CoverageSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    ambiguous_channel_identity_samples: Vec<CoverageSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    linked_aliases_missing_samples: Vec<CoverageSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    linked_messages_missing_sender_person_samples: Vec<CoverageSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    qiwe_room_potential_member_identities_unlinked_samples: Vec<CoverageSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    linked_people_without_active_profile_samples: Vec<CoverageSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    qiwe_platform_identities_missing_samples: Vec<CoverageSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    running_people_profile_missing_running_hint_samples: Vec<CoverageSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    linked_people_without_answer_context_canary_spec_samples: Vec<CoverageSample>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    answer_context_canary_specs: Vec<AnswerContextCanarySpec>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    answer_context_speaker_canary_specs: Vec<AnswerContextSpeakerCanarySpec>,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    answer_context_referenced_canary_specs: Vec<AnswerContextReferencedCanarySpec>,
    persons_created: i64,
    channel_identities_linked: i64,
    platform_identities_materialized: i64,
    aliases_inserted: i64,
    messages_updated: i64,
    dry_run: bool,
}

#[derive(Debug, Serialize)]
struct CoverageSample {
    display_name: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    identity_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    person_key: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    person_id: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    reason: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    count: Option<i64>,
}

#[derive(Debug, Serialize)]
struct AnswerContextCanarySpec {
    id: i64,
    canary_type: &'static str,
    expected_mention: String,
    canonical_key: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    required_profile_terms: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AnswerContextSpeakerCanarySpec {
    id: i64,
    canary_type: &'static str,
    expected_speaker_label: String,
    canonical_key: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    required_profile_terms: Vec<String>,
}

#[derive(Debug, Serialize)]
struct AnswerContextReferencedCanarySpec {
    id: i64,
    canary_type: &'static str,
    expected_referenced_label: String,
    canonical_key: String,
    #[serde(skip_serializing_if = "Vec::is_empty")]
    required_profile_terms: Vec<String>,
}

#[derive(Debug, Serialize)]
struct SpeakerCanarySenderMapReport {
    private_sensitive_sender_ids: bool,
    do_not_retain: bool,
    scope_fingerprint: String,
    sender_count: i64,
    senders: Vec<SpeakerCanarySenderMapEntry>,
}

#[derive(Debug, Serialize)]
struct SpeakerCanarySenderMapEntry {
    canonical_key: String,
    sender_id: String,
}

fn evidence_display_name(display_name: &str) -> String {
    let redacted = redact_sensitive_text(display_name);
    let trimmed = redacted.trim();
    if trimmed.is_empty() {
        "[不可展示名称]".to_string()
    } else {
        trimmed.to_string()
    }
}

fn is_safe_answer_context_canary_mention(mention: &str) -> bool {
    let mention = mention.trim();
    if mention.is_empty()
        || mention.chars().any(char::is_control)
        || has_sensitive_digit_run(mention)
        || mention == "企业微信团队"
        || mention == "秦托邦小客服"
        || mention == "二花"
        || mention.eq_ignore_ascii_case("sidecar smoke")
    {
        return false;
    }
    true
}

fn scope_fingerprint(chat_id: &str) -> String {
    let mut hasher = Sha256::new();
    hasher.update(b"qintopia-erhua-member-recognition-scope-v1\0");
    hasher.update(chat_id.as_bytes());
    format!("sha256:{:x}", hasher.finalize())
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

#[derive(Debug, Clone, PartialEq, Eq)]
struct UnlinkedIdentityAction {
    reason: &'static str,
    match_count: i64,
    ambiguous: bool,
}

fn classify_unlinked_identity_action(
    existing_person_count: i64,
    existing_name_count: i64,
) -> UnlinkedIdentityAction {
    if existing_person_count == 1 {
        return UnlinkedIdentityAction {
            reason: ACTION_REUSE_EXISTING_QIWE_USER,
            match_count: existing_person_count,
            ambiguous: false,
        };
    }
    if existing_person_count > 1 {
        return UnlinkedIdentityAction {
            reason: ACTION_MANUAL_MERGE_MULTIPLE_QIWE_USER_PEOPLE,
            match_count: existing_person_count,
            ambiguous: true,
        };
    }
    if existing_name_count == 1 {
        return UnlinkedIdentityAction {
            reason: ACTION_REUSE_UNIQUE_DISPLAY_NAME_OR_ALIAS,
            match_count: existing_name_count,
            ambiguous: false,
        };
    }
    if existing_name_count > 1 {
        return UnlinkedIdentityAction {
            reason: ACTION_MANUAL_MERGE_AMBIGUOUS_DISPLAY_NAME_OR_ALIAS,
            match_count: existing_name_count,
            ambiguous: true,
        };
    }
    UnlinkedIdentityAction {
        reason: ACTION_CREATE_PERSON,
        match_count: 0,
        ambiguous: false,
    }
}

fn speaker_canary_specs_from_mentions(
    mention_specs: &[AnswerContextCanarySpec],
) -> Vec<AnswerContextSpeakerCanarySpec> {
    let mut people = BTreeMap::<String, (String, BTreeSet<String>)>::new();
    for spec in mention_specs {
        let entry = people
            .entry(spec.canonical_key.clone())
            .or_insert_with(|| (spec.expected_mention.clone(), BTreeSet::new()));
        for term in &spec.required_profile_terms {
            entry.1.insert(term.clone());
        }
    }

    people
        .into_iter()
        .enumerate()
        .map(
            |(index, (canonical_key, (expected_speaker_label, required_profile_terms)))| {
                AnswerContextSpeakerCanarySpec {
                    id: 1_000_002 + index as i64,
                    canary_type: "speaker_self",
                    expected_speaker_label,
                    canonical_key,
                    required_profile_terms: required_profile_terms.into_iter().collect(),
                }
            },
        )
        .collect()
}

fn referenced_canary_specs_from_mentions(
    mention_specs: &[AnswerContextCanarySpec],
) -> Vec<AnswerContextReferencedCanarySpec> {
    let mut people = BTreeMap::<String, (String, BTreeSet<String>)>::new();
    for spec in mention_specs {
        let entry = people
            .entry(spec.canonical_key.clone())
            .or_insert_with(|| (spec.expected_mention.clone(), BTreeSet::new()));
        for term in &spec.required_profile_terms {
            entry.1.insert(term.clone());
        }
    }

    people
        .into_iter()
        .enumerate()
        .map(
            |(index, (canonical_key, (expected_referenced_label, required_profile_terms)))| {
                AnswerContextReferencedCanarySpec {
                    id: 2_000_002 + index as i64,
                    canary_type: "referenced_member",
                    expected_referenced_label,
                    canonical_key,
                    required_profile_terms: required_profile_terms.into_iter().collect(),
                }
            },
        )
        .collect()
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

pub async fn run_speaker_canary_sender_map(cli: &Cli, chat_id: String) -> Result<()> {
    let chat_id = chat_id.trim();
    if chat_id.is_empty() {
        anyhow::bail!("--chat-id is required");
    }
    let database_url = cli.database_url_required()?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let rows = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT DISTINCT ON (ci.person_id)
            ci.person_id::text AS person_id,
            ci.channel_user_id
        FROM qintopia_identity.channel_identities ci
        WHERE ci.platform = 'qiwe'
          AND ci.chat_id = $1
          AND ci.metadata->>'current_qiwe_room_member' = 'true'
          AND ci.person_id IS NOT NULL
          AND COALESCE(ci.is_bot, false) = false
          AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
          AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
        ORDER BY ci.person_id, ci.last_seen_at DESC, ci.updated_at DESC
        "#,
    )
    .bind(chat_id)
    .fetch_all(&pool)
    .await
    .context("load Erhua speaker canary sender map")?;
    let senders = rows
        .into_iter()
        .map(|(person_id, sender_id)| SpeakerCanarySenderMapEntry {
            canonical_key: format!("person:{person_id}"),
            sender_id,
        })
        .collect::<Vec<_>>();
    let report = SpeakerCanarySenderMapReport {
        private_sensitive_sender_ids: true,
        do_not_retain: true,
        scope_fingerprint: scope_fingerprint(chat_id),
        sender_count: senders.len() as i64,
        senders,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

async fn run_bootstrap(
    pool: &PgPool,
    options: &BootstrapOptions,
    apply: bool,
) -> Result<BootstrapReport> {
    let limit = options.limit.unwrap_or(500).max(1);
    let preflight =
        sqlx::query_as::<_, (i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64, i64)>(
        r#"
        WITH all_qiwe_identities AS (
            SELECT
                ci.*,
                NULLIF(
                    regexp_replace(
                        btrim(COALESCE(NULLIF(ci.normalized_display_name, ''), ci.display_name, '')),
                        '[[:space:]]+',
                        ' ',
                        'g'
                    ),
                    ''
                ) AS lookup_name,
                (
                    COALESCE(ci.is_bot, false)
                    OR COALESCE(ci.display_name, '') ~ '[0-9]{7,}'
                    OR COALESCE(ci.display_name, '') ~ '[[:cntrl:]]'
                    OR btrim(COALESCE(ci.display_name, '')) IN ('企业微信团队', '秦托邦小客服', '二花')
                    OR lower(btrim(COALESCE(ci.display_name, ''))) = 'sidecar smoke'
                ) AS bootstrap_excluded
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
        ),
        qiwe_identities AS (
            SELECT *
            FROM all_qiwe_identities
            WHERE bootstrap_excluded = false
        ),
        potential_member_identities AS (
            SELECT *
            FROM all_qiwe_identities
            WHERE COALESCE(is_bot, false) = false
              AND btrim(COALESCE(display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(COALESCE(display_name, ''))) <> 'sidecar smoke'
        ),
        unlinked AS (
            SELECT
                ci.id,
                ci.platform,
                ci.channel_user_id,
                COALESCE(existing.person_count, 0) AS existing_person_count,
                COALESCE(existing_name.person_count, 0) AS existing_name_count
            FROM qiwe_identities ci
            LEFT JOIN LATERAL (
                SELECT count(DISTINCT existing_ci.person_id)::bigint AS person_count
                FROM qintopia_identity.channel_identities existing_ci
                WHERE existing_ci.platform = ci.platform
                  AND existing_ci.channel_user_id = ci.channel_user_id
                  AND existing_ci.person_id IS NOT NULL
            ) existing ON true
            LEFT JOIN LATERAL (
                SELECT count(DISTINCT name_match.person_id)::bigint AS person_count
                FROM (
                    SELECT p.id AS person_id
                    FROM qintopia_identity.persons p
                    WHERE ci.lookup_name IS NOT NULL
                      AND (
                          lower(regexp_replace(btrim(COALESCE(p.display_name, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)
                          OR lower(regexp_replace(btrim(COALESCE(p.primary_name, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)
                          OR lower(regexp_replace(btrim(COALESCE(p.preferred_name, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)
                      )

                    UNION

                    SELECT a.person_id
                    FROM qintopia_identity.person_aliases a
                    WHERE ci.lookup_name IS NOT NULL
                      AND lower(regexp_replace(btrim(COALESCE(a.alias, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)

                    UNION

                    SELECT existing_ci.person_id
                    FROM qintopia_identity.channel_identities existing_ci
                    WHERE ci.lookup_name IS NOT NULL
                      AND existing_ci.platform = ci.platform
                      AND existing_ci.person_id IS NOT NULL
                      AND (
                          lower(existing_ci.normalized_display_name) = lower(ci.lookup_name)
                          OR lower(regexp_replace(btrim(COALESCE(existing_ci.display_name, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)
                      )
                ) name_match
            ) existing_name ON true
            WHERE ci.person_id IS NULL
              AND COALESCE(ci.display_name, '') <> ''
        ),
        missing_aliases AS (
            SELECT DISTINCT ci.person_id, ci.display_name
            FROM qiwe_identities ci
            WHERE ci.person_id IS NOT NULL
              AND COALESCE(ci.display_name, '') <> ''
              AND ci.display_name !~ '^[0-9]+$'
              AND NOT EXISTS (
                  SELECT 1
                  FROM qintopia_identity.person_aliases a
                  WHERE a.person_id = ci.person_id
                    AND a.alias = ci.display_name
                    AND a.alias_type = 'nickname'
              )
        ),
        linked_people AS (
            SELECT DISTINCT ci.person_id
            FROM potential_member_identities ci
            WHERE ci.person_id IS NOT NULL
        ),
        active_profiles AS (
            SELECT DISTINCT s.person_id
            FROM qintopia_identity.member_profile_snapshots s
            WHERE s.profile_kind = 'reply_context'
              AND s.status = 'active'
              AND (s.valid_until IS NULL OR s.valid_until > now())
        )
        SELECT
            (SELECT count(*)::bigint FROM qiwe_identities),
            (SELECT count(*)::bigint FROM qiwe_identities WHERE person_id IS NOT NULL),
            (SELECT count(*)::bigint FROM all_qiwe_identities WHERE bootstrap_excluded = true),
            (SELECT count(*)::bigint FROM unlinked),
            (SELECT count(*)::bigint FROM unlinked WHERE existing_person_count = 1),
            (SELECT count(*)::bigint FROM unlinked WHERE existing_person_count = 0 AND existing_name_count = 1),
            (
                SELECT count(*)::bigint
                FROM unlinked
                WHERE existing_person_count > 1
                   OR (existing_person_count = 0 AND existing_name_count > 1)
            ),
            (SELECT count(*)::bigint FROM missing_aliases),
            (
                SELECT count(*)::bigint
                FROM qintopia_messages.messages m
                JOIN qintopia_identity.channel_identities ci
                  ON ci.id = m.sender_channel_identity_id
                WHERE ci.platform = 'qiwe'
                  AND ci.person_id IS NOT NULL
                  AND m.sender_person_id IS NULL
                  AND (
                    ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
                  )
            ),
            (SELECT count(*)::bigint FROM linked_people),
            (
                SELECT count(*)::bigint
                FROM linked_people lp
                JOIN active_profiles ap ON ap.person_id = lp.person_id
            ),
            (
                SELECT count(*)::bigint
                FROM linked_people lp
                LEFT JOIN active_profiles ap ON ap.person_id = lp.person_id
                WHERE ap.person_id IS NULL
            ),
            (SELECT count(*)::bigint FROM all_qiwe_identities)
        "#,
    )
    .bind(options.chat_id.as_deref())
    .fetch_one(pool)
    .await
    .context("count identity bootstrap gaps")?;
    let room_preflight = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        r#"
        WITH all_qiwe_identities AS (
            SELECT
                ci.*,
                (
                    COALESCE(ci.is_bot, false)
                    OR COALESCE(ci.display_name, '') ~ '[0-9]{7,}'
                    OR COALESCE(ci.display_name, '') ~ '[[:cntrl:]]'
                    OR btrim(COALESCE(ci.display_name, '')) IN ('企业微信团队', '秦托邦小客服', '二花')
                    OR lower(btrim(COALESCE(ci.display_name, ''))) = 'sidecar smoke'
                ) AS bootstrap_excluded
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
        ),
        qiwe_identities AS (
            SELECT *
            FROM all_qiwe_identities
            WHERE bootstrap_excluded = false
        )
        SELECT
            (SELECT count(*)::bigint FROM all_qiwe_identities),
            (SELECT count(*)::bigint FROM qiwe_identities),
            (SELECT count(*)::bigint FROM qiwe_identities WHERE person_id IS NOT NULL),
            (SELECT count(*)::bigint FROM all_qiwe_identities WHERE bootstrap_excluded = true)
        "#,
    )
    .bind(options.chat_id.as_deref())
    .fetch_one(pool)
    .await
    .context("count current-room identity bootstrap gaps")?;
    let potential_member_preflight = sqlx::query_as::<_, (i64, i64, i64)>(
        r#"
        WITH potential_member_identities AS (
            SELECT ci.*
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND COALESCE(ci.is_bot, false) = false
              AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
        )
        SELECT
            (SELECT count(*)::bigint FROM potential_member_identities),
            (SELECT count(*)::bigint FROM potential_member_identities WHERE person_id IS NOT NULL),
            (SELECT count(*)::bigint FROM potential_member_identities WHERE person_id IS NULL)
        "#,
    )
    .bind(options.chat_id.as_deref())
    .fetch_one(pool)
    .await
    .context("count current-room potential member identity gaps")?;
    let running_profile = sqlx::query_as::<_, (i64, i64, i64)>(
        r#"
        WITH qiwe_people AS (
            SELECT DISTINCT ci.person_id
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND ci.person_id IS NOT NULL
              AND COALESCE(ci.is_bot, false) = false
              AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
        ),
        running_people AS (
            SELECT DISTINCT f.person_id
            FROM qintopia_identity.member_facts f
            JOIN qiwe_people qp ON qp.person_id = f.person_id
            WHERE f.revoked_at IS NULL
              AND (
                  f.fact_key = 'running_activity'
                  OR f.fact_text ILIKE '%跑步%'
                  OR f.fact_text ILIKE '%慢跑%'
                  OR f.fact_text ILIKE '%晨跑%'
                  OR f.fact_text ILIKE '%夜跑%'
              )
        ),
        active_profiles AS (
            SELECT DISTINCT s.person_id
            FROM qintopia_identity.member_profile_snapshots s
            WHERE s.profile_kind = 'reply_context'
              AND s.status = 'active'
              AND (s.valid_until IS NULL OR s.valid_until > now())
              AND (
                  s.summary ILIKE '%跑步%'
                  OR s.safe_reply_hints::text ILIKE '%跑步%'
                  OR s.safe_reply_hints::text ILIKE '%running_activity%'
              )
        )
        SELECT
            (SELECT count(*)::bigint FROM running_people),
            (
                SELECT count(*)::bigint
                FROM running_people rp
                JOIN active_profiles ap ON ap.person_id = rp.person_id
            ),
            (
                SELECT count(*)::bigint
                FROM running_people rp
                LEFT JOIN active_profiles ap ON ap.person_id = rp.person_id
                WHERE ap.person_id IS NULL
            )
        "#,
    )
    .bind(options.chat_id.as_deref())
    .fetch_one(pool)
    .await
    .context("count running profile coverage gaps")?;
    let platform_identity = sqlx::query_as::<_, (i64, i64, i64, i64)>(
        r#"
        WITH scoped_qiwe_identities AS (
            SELECT ci.*
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND ci.person_id IS NOT NULL
              AND COALESCE(ci.is_bot, false) = false
              AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
        ),
        linked_people AS (
            SELECT DISTINCT person_id
            FROM scoped_qiwe_identities
        ),
        linked_user_people AS (
            SELECT
                ci.channel_user_id,
                min(ci.person_id::text)::uuid AS person_id,
                count(DISTINCT ci.person_id)::bigint AS person_count
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND ci.person_id IS NOT NULL
              AND ci.channel_user_id IN (
                  SELECT scoped.channel_user_id
                  FROM scoped_qiwe_identities scoped
              )
            GROUP BY ci.channel_user_id
        ),
        materializable AS (
            SELECT channel_user_id, person_id
            FROM linked_user_people
            WHERE person_count = 1
        ),
        missing_platform_identities AS (
            SELECT m.channel_user_id, m.person_id
            FROM materializable m
            WHERE NOT EXISTS (
                SELECT 1
                FROM qintopia_identity.channel_identities ci
                WHERE ci.platform = 'qiwe'
                  AND ci.chat_id = ''
                  AND ci.channel_user_id = m.channel_user_id
                  AND ci.person_id = m.person_id
            )
        ),
        people_with_platform_identity AS (
            SELECT DISTINCT scoped.person_id
            FROM scoped_qiwe_identities scoped
            JOIN qintopia_identity.channel_identities platform_identity
              ON platform_identity.platform = 'qiwe'
             AND platform_identity.chat_id = ''
             AND platform_identity.channel_user_id = scoped.channel_user_id
             AND platform_identity.person_id = scoped.person_id
        ),
        linked_people_without_qiwe_platform_identity AS (
            SELECT lp.person_id
            FROM linked_people lp
            LEFT JOIN people_with_platform_identity pip ON pip.person_id = lp.person_id
            WHERE pip.person_id IS NULL
        )
        SELECT
            (SELECT count(*)::bigint FROM materializable),
            (SELECT count(*)::bigint FROM missing_platform_identities),
            (SELECT count(*)::bigint FROM linked_user_people WHERE person_count > 1),
            (SELECT count(*)::bigint FROM linked_people_without_qiwe_platform_identity)
        "#,
    )
    .bind(options.chat_id.as_deref())
    .fetch_one(pool)
    .await
    .context("count QiWe platform identity coverage gaps")?;
    let samples = load_coverage_samples(pool, options.chat_id.as_deref()).await?;
    let answer_context_canary_specs =
        load_answer_context_canary_specs(pool, options.chat_id.as_deref()).await?;
    let answer_context_speaker_canary_specs =
        speaker_canary_specs_from_mentions(&answer_context_canary_specs);
    let answer_context_referenced_canary_specs =
        referenced_canary_specs_from_mentions(&answer_context_canary_specs);
    let answer_context_canary_people_total = answer_context_canary_specs
        .iter()
        .map(|spec| spec.canonical_key.as_str())
        .collect::<BTreeSet<_>>()
        .len() as i64;
    let answer_context_speaker_canary_people_total = answer_context_speaker_canary_specs
        .iter()
        .map(|spec| spec.canonical_key.as_str())
        .collect::<BTreeSet<_>>()
        .len() as i64;
    let answer_context_referenced_canary_people_total = answer_context_referenced_canary_specs
        .iter()
        .map(|spec| spec.canonical_key.as_str())
        .collect::<BTreeSet<_>>()
        .len() as i64;

    let mut report = BootstrapReport {
        scope_fingerprint: options.chat_id.as_deref().map(scope_fingerprint),
        qiwe_channel_identities_raw_total: preflight.12,
        qiwe_room_channel_identities_raw_total: room_preflight.0,
        qiwe_room_channel_identities_total: room_preflight.1,
        qiwe_room_channel_identities_linked: room_preflight.2,
        qiwe_room_channel_identities_excluded: room_preflight.3,
        qiwe_room_potential_member_identities_total: potential_member_preflight.0,
        qiwe_room_potential_member_identities_linked: potential_member_preflight.1,
        qiwe_room_potential_member_identities_unlinked: potential_member_preflight.2,
        total_channel_identities: preflight.3,
        qiwe_channel_identities_total: preflight.0,
        qiwe_channel_identities_linked: preflight.1,
        qiwe_channel_identities_excluded: preflight.2,
        channel_identities_with_existing_person: preflight.4,
        channel_identities_with_existing_name: preflight.5,
        ambiguous_channel_identities_skipped: preflight.6,
        linked_aliases_missing: preflight.7,
        linked_messages_missing_sender_person: preflight.8,
        linked_people_total: preflight.9,
        linked_people_with_active_profile: preflight.10,
        linked_people_without_active_profile: preflight.11,
        qiwe_platform_identity_materializable_users: platform_identity.0,
        qiwe_platform_identities_missing: platform_identity.1,
        qiwe_platform_identity_ambiguous_users: platform_identity.2,
        linked_people_without_qiwe_platform_identity: platform_identity.3,
        linked_people_with_running_facts: running_profile.0,
        running_people_with_profile_running_hint: running_profile.1,
        running_people_profile_missing_running_hint: running_profile.2,
        answer_context_canary_specs_total: answer_context_canary_specs.len() as i64,
        answer_context_canary_people_total,
        answer_context_speaker_canary_specs_total: answer_context_speaker_canary_specs.len() as i64,
        answer_context_speaker_canary_people_total,
        answer_context_referenced_canary_specs_total: answer_context_referenced_canary_specs.len()
            as i64,
        answer_context_referenced_canary_people_total,
        linked_people_without_answer_context_canary_spec: preflight
            .9
            .saturating_sub(answer_context_canary_people_total),
        unlinked_channel_identity_samples: samples.unlinked_channel_identity_samples,
        ambiguous_channel_identity_samples: samples.ambiguous_channel_identity_samples,
        linked_aliases_missing_samples: samples.linked_aliases_missing_samples,
        linked_messages_missing_sender_person_samples: samples
            .linked_messages_missing_sender_person_samples,
        linked_people_without_active_profile_samples: samples
            .linked_people_without_active_profile_samples,
        qiwe_platform_identities_missing_samples: samples.qiwe_platform_identities_missing_samples,
        running_people_profile_missing_running_hint_samples: samples
            .running_people_profile_missing_running_hint_samples,
        linked_people_without_answer_context_canary_spec_samples: samples
            .linked_people_without_answer_context_canary_spec_samples,
        qiwe_room_potential_member_identities_unlinked_samples: samples
            .qiwe_room_potential_member_identities_unlinked_samples,
        answer_context_canary_specs,
        answer_context_speaker_canary_specs,
        answer_context_referenced_canary_specs,
        dry_run: !apply,
        ..BootstrapReport::default()
    };
    if !apply {
        return Ok(report);
    }

    let mut tx = pool.begin().await.context("begin person bootstrap")?;
    let rows = sqlx::query_as::<_, (i64, i64, i64, i64, i64)>(
        r#"
        WITH candidate_sources AS (
            SELECT
                ci.*,
                NULLIF(
                    regexp_replace(
                        btrim(COALESCE(NULLIF(ci.normalized_display_name, ''), ci.display_name, '')),
                        '[[:space:]]+',
                        ' ',
                        'g'
                    ),
                    ''
                ) AS lookup_name
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND ci.person_id IS NULL
              AND COALESCE(ci.display_name, '') <> ''
              AND COALESCE(ci.is_bot, false) = false
              AND ci.display_name !~ '[0-9]{7,}'
              AND ci.display_name !~ '[[:cntrl:]]'
              AND btrim(ci.display_name) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(ci.display_name)) <> 'sidecar smoke'
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
            ORDER BY ci.updated_at DESC, ci.id
            LIMIT $2
            FOR UPDATE SKIP LOCKED
        ),
        candidates AS (
            SELECT
                ci.id,
                ci.platform,
                ci.channel_user_id,
                ci.display_name,
                ci.normalized_display_name,
                ci.identity_source,
                ci.updated_at,
                COALESCE(existing.person_count, 0) AS existing_person_count,
                COALESCE(existing_name.person_count, 0) AS existing_name_count,
                CASE
                    WHEN COALESCE(existing.person_count, 0) = 1 THEN existing.person_id
                    WHEN COALESCE(existing.person_count, 0) = 0
                     AND COALESCE(existing_name.person_count, 0) = 1 THEN existing_name.person_id
                    ELSE NULL
                END AS existing_person_id
            FROM candidate_sources ci
            LEFT JOIN LATERAL (
                SELECT
                    min(existing_ci.person_id::text)::uuid AS person_id,
                    count(DISTINCT existing_ci.person_id)::bigint AS person_count
                FROM qintopia_identity.channel_identities existing_ci
                WHERE existing_ci.platform = ci.platform
                  AND existing_ci.channel_user_id = ci.channel_user_id
                  AND existing_ci.person_id IS NOT NULL
            ) existing ON true
            LEFT JOIN LATERAL (
                SELECT
                    min(name_match.person_id::text)::uuid AS person_id,
                    count(DISTINCT name_match.person_id)::bigint AS person_count
                FROM (
                    SELECT p.id AS person_id
                    FROM qintopia_identity.persons p
                    WHERE ci.lookup_name IS NOT NULL
                      AND (
                          lower(regexp_replace(btrim(COALESCE(p.display_name, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)
                          OR lower(regexp_replace(btrim(COALESCE(p.primary_name, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)
                          OR lower(regexp_replace(btrim(COALESCE(p.preferred_name, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)
                      )

                    UNION

                    SELECT a.person_id
                    FROM qintopia_identity.person_aliases a
                    WHERE ci.lookup_name IS NOT NULL
                      AND lower(regexp_replace(btrim(COALESCE(a.alias, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)

                    UNION

                    SELECT existing_ci.person_id
                    FROM qintopia_identity.channel_identities existing_ci
                    WHERE ci.lookup_name IS NOT NULL
                      AND existing_ci.platform = ci.platform
                      AND existing_ci.person_id IS NOT NULL
                      AND (
                          lower(existing_ci.normalized_display_name) = lower(ci.lookup_name)
                          OR lower(regexp_replace(btrim(COALESCE(existing_ci.display_name, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)
                      )
                ) name_match
            ) existing_name ON true
        ),
        existing_links AS (
            UPDATE qintopia_identity.channel_identities ci
            SET person_id = candidates.existing_person_id,
                updated_at = now()
            FROM candidates
            WHERE ci.id = candidates.id
              AND candidates.existing_person_id IS NOT NULL
            RETURNING
                ci.id AS channel_identity_id,
                candidates.existing_person_id AS person_id,
                ci.platform,
                ci.channel_user_id,
                ci.display_name,
                ci.normalized_display_name,
                ci.identity_source,
                ci.confidence,
                ci.first_seen_at,
                ci.last_seen_at,
                ci.metadata
        ),
        create_sources AS (
            SELECT DISTINCT ON (platform, channel_user_id)
                id,
                platform,
                channel_user_id,
                display_name,
                normalized_display_name
            FROM candidates
            WHERE existing_person_count = 0
              AND existing_name_count = 0
            ORDER BY
                platform,
                channel_user_id,
                qintopia_identity.identity_source_rank(identity_source) DESC,
                updated_at DESC,
                id
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
                    'platform', c.platform,
                    'channel_user_id', c.channel_user_id,
                    'channel_identity_id', c.id::text,
                    'person_merge_status', 'unmerged'
                )
            FROM create_sources c
            RETURNING
                id,
                metadata->>'platform' AS platform,
                metadata->>'channel_user_id' AS channel_user_id
        ),
        created_links AS (
            UPDATE qintopia_identity.channel_identities ci
            SET person_id = created.id,
                updated_at = now()
            FROM created
            JOIN candidates
              ON candidates.platform = created.platform
             AND candidates.channel_user_id = created.channel_user_id
             AND candidates.existing_person_count = 0
             AND candidates.existing_name_count = 0
            WHERE ci.id = candidates.id
            RETURNING
                ci.id AS channel_identity_id,
                created.id AS person_id,
                ci.platform,
                ci.channel_user_id,
                ci.display_name,
                ci.normalized_display_name,
                ci.identity_source,
                ci.confidence,
                ci.first_seen_at,
                ci.last_seen_at,
                ci.metadata
        ),
        linked AS (
            SELECT * FROM existing_links
            UNION ALL
            SELECT * FROM created_links
        ),
        scoped_linked_qiwe_identities AS (
            SELECT
                ci.id AS channel_identity_id,
                ci.person_id,
                ci.platform,
                ci.channel_user_id,
                ci.display_name,
                ci.normalized_display_name,
                ci.identity_source,
                ci.confidence,
                ci.first_seen_at,
                ci.last_seen_at,
                ci.metadata
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND ci.person_id IS NOT NULL
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )

            UNION

            SELECT
                linked.channel_identity_id,
                linked.person_id,
                linked.platform,
                linked.channel_user_id,
                linked.display_name,
                linked.normalized_display_name,
                linked.identity_source,
                linked.confidence,
                linked.first_seen_at,
                linked.last_seen_at,
                linked.metadata
            FROM linked
            WHERE platform = 'qiwe'
        ),
        platform_identity_candidates AS (
            SELECT
                platform,
                channel_user_id,
                person_id
            FROM qintopia_identity.channel_identities
            WHERE platform = 'qiwe'
              AND person_id IS NOT NULL
              AND channel_user_id IN (
                  SELECT channel_user_id
                  FROM scoped_linked_qiwe_identities
              )

            UNION

            SELECT platform, channel_user_id, person_id
            FROM linked
            WHERE platform = 'qiwe'
        ),
        materializable_platform_identities AS (
            SELECT platform, channel_user_id, min(person_id::text)::uuid AS person_id
            FROM platform_identity_candidates
            GROUP BY platform, channel_user_id
            HAVING count(DISTINCT person_id) = 1
        ),
        platform_identity_sources AS (
            SELECT DISTINCT ON (source.platform, source.channel_user_id)
                source.*
            FROM scoped_linked_qiwe_identities source
            JOIN materializable_platform_identities materializable
              ON materializable.platform = source.platform
             AND materializable.channel_user_id = source.channel_user_id
             AND materializable.person_id = source.person_id
            ORDER BY
                source.platform,
                source.channel_user_id,
                qintopia_identity.identity_source_rank(source.identity_source) DESC,
                source.last_seen_at DESC
        ),
        platform_identities AS (
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
                        'materialized_from_channel_identity_id', source.channel_identity_id::text,
                        'materialized_at', now()
                    )
            FROM platform_identity_sources source
            ON CONFLICT (platform, channel_user_id, chat_id) DO UPDATE SET
                person_id = EXCLUDED.person_id,
                display_name = EXCLUDED.display_name,
                normalized_display_name = EXCLUDED.normalized_display_name,
                identity_source = EXCLUDED.identity_source,
                confidence = GREATEST(qintopia_identity.channel_identities.confidence, EXCLUDED.confidence),
                last_seen_at = GREATEST(qintopia_identity.channel_identities.last_seen_at, EXCLUDED.last_seen_at),
                metadata = qintopia_identity.channel_identities.metadata || EXCLUDED.metadata,
                updated_at = now()
            WHERE qintopia_identity.channel_identities.person_id IS NULL
               OR qintopia_identity.channel_identities.person_id = EXCLUDED.person_id
            RETURNING id
        ),
        alias_sources AS (
            SELECT DISTINCT
                ci.person_id,
                ci.id AS channel_identity_id,
                ci.display_name,
                ci.first_seen_at,
                ci.last_seen_at
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND ci.person_id IS NOT NULL
              AND COALESCE(ci.display_name, '') <> ''
              AND ci.display_name !~ '^[0-9]+$'
              AND COALESCE(ci.is_bot, false) = false
              AND ci.display_name !~ '[0-9]{7,}'
              AND ci.display_name !~ '[[:cntrl:]]'
              AND btrim(ci.display_name) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(ci.display_name)) <> 'sidecar smoke'
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )

            UNION

            SELECT DISTINCT
                linked.person_id,
                linked.channel_identity_id,
                linked.display_name,
                linked.first_seen_at,
                linked.last_seen_at
            FROM linked
            WHERE linked.display_name !~ '^[0-9]+$'
        ),
        aliases AS (
            INSERT INTO qintopia_identity.person_aliases
                (person_id, alias, alias_type, source, confidence, first_seen_at, last_seen_at, metadata)
            SELECT
                alias_sources.person_id,
                alias_sources.display_name,
                'nickname',
                'qiwe_channel_identity',
                1.0,
                alias_sources.first_seen_at,
                alias_sources.last_seen_at,
                jsonb_build_object('channel_identity_id', alias_sources.channel_identity_id::text)
            FROM alias_sources
            ON CONFLICT (person_id, alias, alias_type) DO UPDATE SET
                last_seen_at = GREATEST(qintopia_identity.person_aliases.last_seen_at, EXCLUDED.last_seen_at),
                source = EXCLUDED.source,
                confidence = GREATEST(qintopia_identity.person_aliases.confidence, EXCLUDED.confidence),
                metadata = qintopia_identity.person_aliases.metadata || EXCLUDED.metadata
            RETURNING id
        ),
        updated_messages AS (
            UPDATE qintopia_messages.messages m
            SET sender_person_id = ci.person_id,
                updated_at = now()
            FROM qintopia_identity.channel_identities ci
            WHERE m.sender_channel_identity_id = ci.id
              AND ci.person_id IS NOT NULL
              AND m.sender_person_id IS NULL
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
            RETURNING m.id
        )
        SELECT
            (SELECT count(*)::bigint FROM created) AS created_count,
            (SELECT count(*)::bigint FROM linked) AS linked_count,
            (SELECT count(*)::bigint FROM platform_identities) AS platform_identity_count,
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
    report.channel_identities_linked = rows.1;
    report.platform_identities_materialized = rows.2;
    report.aliases_inserted = rows.3;
    report.messages_updated = rows.4;
    Ok(report)
}

#[derive(Debug, Default)]
struct CoverageSamples {
    unlinked_channel_identity_samples: Vec<CoverageSample>,
    ambiguous_channel_identity_samples: Vec<CoverageSample>,
    linked_aliases_missing_samples: Vec<CoverageSample>,
    linked_messages_missing_sender_person_samples: Vec<CoverageSample>,
    qiwe_room_potential_member_identities_unlinked_samples: Vec<CoverageSample>,
    linked_people_without_active_profile_samples: Vec<CoverageSample>,
    qiwe_platform_identities_missing_samples: Vec<CoverageSample>,
    running_people_profile_missing_running_hint_samples: Vec<CoverageSample>,
    linked_people_without_answer_context_canary_spec_samples: Vec<CoverageSample>,
}

async fn load_coverage_samples(pool: &PgPool, chat_id: Option<&str>) -> Result<CoverageSamples> {
    Ok(CoverageSamples {
        unlinked_channel_identity_samples: load_unlinked_channel_identity_samples(
            pool, chat_id, false,
        )
        .await?,
        ambiguous_channel_identity_samples: load_unlinked_channel_identity_samples(
            pool, chat_id, true,
        )
        .await?,
        linked_aliases_missing_samples: load_missing_alias_samples(pool, chat_id).await?,
        linked_messages_missing_sender_person_samples: load_missing_sender_person_message_samples(
            pool, chat_id,
        )
        .await?,
        qiwe_room_potential_member_identities_unlinked_samples:
            load_unlinked_potential_member_identity_samples(pool, chat_id).await?,
        linked_people_without_active_profile_samples: load_missing_profile_samples(pool, chat_id)
            .await?,
        qiwe_platform_identities_missing_samples: load_missing_platform_identity_samples(
            pool, chat_id,
        )
        .await?,
        running_people_profile_missing_running_hint_samples: load_running_profile_hint_gap_samples(
            pool, chat_id,
        )
        .await?,
        linked_people_without_answer_context_canary_spec_samples: load_missing_canary_name_samples(
            pool, chat_id,
        )
        .await?,
    })
}

async fn load_unlinked_channel_identity_samples(
    pool: &PgPool,
    chat_id: Option<&str>,
    ambiguous_only: bool,
) -> Result<Vec<CoverageSample>> {
    let rows = sqlx::query_as::<_, (String, i64, i64)>(
        r#"
        WITH qiwe_identities AS (
            SELECT
                ci.*,
                NULLIF(
                    regexp_replace(
                        btrim(COALESCE(NULLIF(ci.normalized_display_name, ''), ci.display_name, '')),
                        '[[:space:]]+',
                        ' ',
                        'g'
                    ),
                    ''
                ) AS lookup_name
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND COALESCE(ci.is_bot, false) = false
              AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
        ),
        unlinked AS (
            SELECT
                ci.display_name,
                COALESCE(existing.person_count, 0) AS existing_person_count,
                COALESCE(existing_name.person_count, 0) AS existing_name_count,
                max(ci.updated_at) AS updated_at
            FROM qiwe_identities ci
            LEFT JOIN LATERAL (
                SELECT count(DISTINCT existing_ci.person_id)::bigint AS person_count
                FROM qintopia_identity.channel_identities existing_ci
                WHERE existing_ci.platform = ci.platform
                  AND existing_ci.channel_user_id = ci.channel_user_id
                  AND existing_ci.person_id IS NOT NULL
            ) existing ON true
            LEFT JOIN LATERAL (
                SELECT count(DISTINCT name_match.person_id)::bigint AS person_count
                FROM (
                    SELECT p.id AS person_id
                    FROM qintopia_identity.persons p
                    WHERE ci.lookup_name IS NOT NULL
                      AND (
                          lower(regexp_replace(btrim(COALESCE(p.display_name, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)
                          OR lower(regexp_replace(btrim(COALESCE(p.primary_name, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)
                          OR lower(regexp_replace(btrim(COALESCE(p.preferred_name, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)
                      )

                    UNION

                    SELECT a.person_id
                    FROM qintopia_identity.person_aliases a
                    WHERE ci.lookup_name IS NOT NULL
                      AND lower(regexp_replace(btrim(COALESCE(a.alias, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)

                    UNION

                    SELECT existing_ci.person_id
                    FROM qintopia_identity.channel_identities existing_ci
                    WHERE ci.lookup_name IS NOT NULL
                      AND existing_ci.platform = ci.platform
                      AND existing_ci.person_id IS NOT NULL
                      AND (
                          lower(existing_ci.normalized_display_name) = lower(ci.lookup_name)
                          OR lower(regexp_replace(btrim(COALESCE(existing_ci.display_name, '')), '[[:space:]]+', ' ', 'g')) = lower(ci.lookup_name)
                      )
                ) name_match
            ) existing_name ON true
            WHERE ci.person_id IS NULL
              AND COALESCE(ci.display_name, '') <> ''
            GROUP BY
                ci.display_name,
                COALESCE(existing.person_count, 0),
                COALESCE(existing_name.person_count, 0)
        )
        SELECT
            display_name,
            existing_person_count,
            existing_name_count
        FROM unlinked
        ORDER BY
            GREATEST(existing_person_count, existing_name_count) DESC,
            updated_at DESC,
            display_name
        LIMIT $2
        "#,
    )
    .bind(chat_id)
    .bind(COVERAGE_SAMPLE_CANDIDATE_LIMIT)
    .fetch_all(pool)
    .await
    .context("load unlinked channel identity samples")?;
    Ok(rows
        .into_iter()
        .filter_map(
            |(display_name, existing_person_count, existing_name_count)| {
                let action =
                    classify_unlinked_identity_action(existing_person_count, existing_name_count);
                if action.ambiguous != ambiguous_only {
                    return None;
                }
                Some(CoverageSample {
                    display_name: evidence_display_name(&display_name),
                    identity_key: None,
                    person_key: None,
                    person_id: None,
                    reason: Some(action.reason.to_string()),
                    count: Some(action.match_count),
                })
            },
        )
        .take(COVERAGE_SAMPLE_LIMIT as usize)
        .collect())
}

async fn load_missing_alias_samples(
    pool: &PgPool,
    chat_id: Option<&str>,
) -> Result<Vec<CoverageSample>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT DISTINCT ci.display_name, ci.person_id::text AS person_id
        FROM qintopia_identity.channel_identities ci
        WHERE ci.platform = 'qiwe'
          AND ci.person_id IS NOT NULL
          AND COALESCE(ci.display_name, '') <> ''
          AND ci.display_name !~ '^[0-9]+$'
          AND COALESCE(ci.is_bot, false) = false
          AND ci.display_name !~ '[0-9]{7,}'
          AND ci.display_name !~ '[[:cntrl:]]'
          AND btrim(ci.display_name) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
          AND lower(btrim(ci.display_name)) <> 'sidecar smoke'
          AND (
            ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
          )
          AND NOT EXISTS (
              SELECT 1
              FROM qintopia_identity.person_aliases a
              WHERE a.person_id = ci.person_id
                AND a.alias = ci.display_name
                AND a.alias_type = 'nickname'
          )
        ORDER BY ci.display_name, ci.person_id::text
        LIMIT $2
        "#,
    )
    .bind(chat_id)
    .bind(COVERAGE_SAMPLE_LIMIT)
    .fetch_all(pool)
    .await
    .context("load missing alias samples")?;
    Ok(rows
        .into_iter()
        .map(|(display_name, person_id)| CoverageSample {
            display_name: evidence_display_name(&display_name),
            identity_key: None,
            person_key: None,
            person_id: Some(person_id),
            reason: Some("linked_display_name_missing_alias".to_string()),
            count: None,
        })
        .collect())
}

async fn load_missing_sender_person_message_samples(
    pool: &PgPool,
    chat_id: Option<&str>,
) -> Result<Vec<CoverageSample>> {
    let rows = sqlx::query_as::<_, (String, String, i64)>(
        r#"
        SELECT
            COALESCE(ci.display_name, p.display_name, '未知成员') AS display_name,
            ci.person_id::text AS person_id,
            count(*)::bigint AS message_count
        FROM qintopia_messages.messages m
        JOIN qintopia_identity.channel_identities ci
          ON ci.id = m.sender_channel_identity_id
        JOIN qintopia_identity.persons p ON p.id = ci.person_id
        WHERE ci.platform = 'qiwe'
          AND ci.person_id IS NOT NULL
          AND COALESCE(ci.is_bot, false) = false
          AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
          AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
          AND m.sender_person_id IS NULL
          AND (
            ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
          )
        GROUP BY ci.person_id, COALESCE(ci.display_name, p.display_name, '未知成员')
        ORDER BY message_count DESC, display_name
        LIMIT $2
        "#,
    )
    .bind(chat_id)
    .bind(COVERAGE_SAMPLE_LIMIT)
    .fetch_all(pool)
    .await
    .context("load missing sender_person_id message samples")?;
    Ok(rows
        .into_iter()
        .map(|(display_name, person_id, count)| CoverageSample {
            display_name: evidence_display_name(&display_name),
            identity_key: None,
            person_key: None,
            person_id: Some(person_id),
            reason: Some("linked_messages_missing_sender_person".to_string()),
            count: Some(count),
        })
        .collect())
}

async fn load_unlinked_potential_member_identity_samples(
    pool: &PgPool,
    chat_id: Option<&str>,
) -> Result<Vec<CoverageSample>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        r#"
        SELECT
            COALESCE(NULLIF(ci.display_name, ''), '未知成员') AS display_name,
            substr(md5(ci.id::text), 1, 12) AS identity_key
        FROM qintopia_identity.channel_identities ci
        WHERE ci.platform = 'qiwe'
          AND ci.person_id IS NULL
          AND COALESCE(ci.is_bot, false) = false
          AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
          AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
          AND (
            ($1::text IS NULL AND ci.chat_id <> '')
            OR (
                $1::text IS NOT NULL
                AND ci.chat_id = $1
                AND ci.metadata->>'current_qiwe_room_member' = 'true'
            )
          )
        ORDER BY ci.updated_at DESC, ci.id
        "#,
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await
    .context("load unlinked potential member identity samples")?;
    Ok(rows
        .into_iter()
        .map(|(display_name, identity_key)| CoverageSample {
            display_name: evidence_display_name(&display_name),
            identity_key: Some(identity_key),
            person_key: None,
            person_id: None,
            reason: Some("potential_member_identity_unlinked".to_string()),
            count: None,
        })
        .collect())
}

async fn load_missing_profile_samples(
    pool: &PgPool,
    chat_id: Option<&str>,
) -> Result<Vec<CoverageSample>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        r#"
        WITH linked_people AS (
            SELECT DISTINCT ci.person_id
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND ci.person_id IS NOT NULL
              AND COALESCE(ci.is_bot, false) = false
              AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
        ),
        active_profiles AS (
            SELECT DISTINCT s.person_id
            FROM qintopia_identity.member_profile_snapshots s
            WHERE s.profile_kind = 'reply_context'
              AND s.status = 'active'
              AND (s.valid_until IS NULL OR s.valid_until > now())
        )
        SELECT
            COALESCE(identity_name.display_name, p.display_name, '未知成员') AS display_name,
            p.id::text AS person_id
        FROM linked_people lp
        JOIN qintopia_identity.persons p ON p.id = lp.person_id
        LEFT JOIN active_profiles ap ON ap.person_id = p.id
        LEFT JOIN LATERAL (
            SELECT ci.display_name
            FROM qintopia_identity.channel_identities ci
            WHERE ci.person_id = p.id
              AND ci.platform = 'qiwe'
              AND COALESCE(ci.display_name, '') <> ''
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
            ORDER BY ci.last_seen_at DESC, ci.updated_at DESC
            LIMIT 1
        ) identity_name ON true
        WHERE ap.person_id IS NULL
        ORDER BY display_name, p.id
        LIMIT $2
        "#,
    )
    .bind(chat_id)
    .bind(COVERAGE_SAMPLE_LIMIT)
    .fetch_all(pool)
    .await
    .context("load missing profile samples")?;
    Ok(rows
        .into_iter()
        .map(|(display_name, person_id)| CoverageSample {
            display_name: evidence_display_name(&display_name),
            identity_key: None,
            person_key: None,
            person_id: Some(person_id),
            reason: Some("missing_active_reply_context_profile".to_string()),
            count: None,
        })
        .collect())
}

async fn load_missing_platform_identity_samples(
    pool: &PgPool,
    chat_id: Option<&str>,
) -> Result<Vec<CoverageSample>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        r#"
        WITH scoped_qiwe_identities AS (
            SELECT ci.*
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND ci.person_id IS NOT NULL
              AND COALESCE(ci.is_bot, false) = false
              AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
        ),
        linked_user_people AS (
            SELECT
                ci.channel_user_id,
                min(ci.person_id::text)::uuid AS person_id,
                count(DISTINCT ci.person_id)::bigint AS person_count
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND ci.person_id IS NOT NULL
              AND ci.channel_user_id IN (
                  SELECT scoped.channel_user_id
                  FROM scoped_qiwe_identities scoped
              )
            GROUP BY ci.channel_user_id
        ),
        materializable AS (
            SELECT channel_user_id, person_id
            FROM linked_user_people
            WHERE person_count = 1
        ),
        missing_platform_identities AS (
            SELECT m.channel_user_id, m.person_id
            FROM materializable m
            WHERE NOT EXISTS (
                SELECT 1
                FROM qintopia_identity.channel_identities ci
                WHERE ci.platform = 'qiwe'
                  AND ci.chat_id = ''
                  AND ci.channel_user_id = m.channel_user_id
                  AND ci.person_id = m.person_id
            )
        )
        SELECT
            COALESCE(identity_name.display_name, p.display_name, '未知成员') AS display_name,
            p.id::text AS person_id
        FROM missing_platform_identities missing
        JOIN qintopia_identity.persons p ON p.id = missing.person_id
        LEFT JOIN LATERAL (
            SELECT ci.display_name
            FROM scoped_qiwe_identities ci
            WHERE ci.person_id = p.id
              AND COALESCE(ci.display_name, '') <> ''
            ORDER BY ci.last_seen_at DESC, ci.updated_at DESC
            LIMIT 1
        ) identity_name ON true
        ORDER BY display_name, p.id
        LIMIT $2
        "#,
    )
    .bind(chat_id)
    .bind(COVERAGE_SAMPLE_LIMIT)
    .fetch_all(pool)
    .await
    .context("load missing QiWe platform identity samples")?;
    Ok(rows
        .into_iter()
        .map(|(display_name, person_id)| CoverageSample {
            display_name: evidence_display_name(&display_name),
            identity_key: None,
            person_key: None,
            person_id: Some(person_id),
            reason: Some("missing_qiwe_platform_identity".to_string()),
            count: None,
        })
        .collect())
}

async fn load_running_profile_hint_gap_samples(
    pool: &PgPool,
    chat_id: Option<&str>,
) -> Result<Vec<CoverageSample>> {
    let rows = sqlx::query_as::<_, (String, String, i64)>(
        r#"
        WITH qiwe_people AS (
            SELECT DISTINCT ci.person_id
            FROM qintopia_identity.channel_identities ci
            WHERE ci.platform = 'qiwe'
              AND ci.person_id IS NOT NULL
              AND COALESCE(ci.is_bot, false) = false
              AND btrim(COALESCE(ci.display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(COALESCE(ci.display_name, ''))) <> 'sidecar smoke'
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
        ),
        running_people AS (
            SELECT f.person_id, count(*)::bigint AS fact_count
            FROM qintopia_identity.member_facts f
            JOIN qiwe_people qp ON qp.person_id = f.person_id
            WHERE f.revoked_at IS NULL
              AND (
                  f.fact_key = 'running_activity'
                  OR f.fact_text ILIKE '%跑步%'
                  OR f.fact_text ILIKE '%慢跑%'
                  OR f.fact_text ILIKE '%晨跑%'
                  OR f.fact_text ILIKE '%夜跑%'
              )
            GROUP BY f.person_id
        ),
        active_profiles AS (
            SELECT DISTINCT s.person_id
            FROM qintopia_identity.member_profile_snapshots s
            WHERE s.profile_kind = 'reply_context'
              AND s.status = 'active'
              AND (s.valid_until IS NULL OR s.valid_until > now())
              AND (
                  s.summary ILIKE '%跑步%'
                  OR s.safe_reply_hints::text ILIKE '%跑步%'
                  OR s.safe_reply_hints::text ILIKE '%running_activity%'
              )
        )
        SELECT
            COALESCE(identity_name.display_name, p.display_name, '未知成员') AS display_name,
            p.id::text AS person_id,
            rp.fact_count
        FROM running_people rp
        JOIN qintopia_identity.persons p ON p.id = rp.person_id
        LEFT JOIN active_profiles ap ON ap.person_id = p.id
        LEFT JOIN LATERAL (
            SELECT ci.display_name
            FROM qintopia_identity.channel_identities ci
            WHERE ci.person_id = p.id
              AND ci.platform = 'qiwe'
              AND COALESCE(ci.display_name, '') <> ''
              AND (
                ($1::text IS NULL AND ci.chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND ci.chat_id = $1
                    AND ci.metadata->>'current_qiwe_room_member' = 'true'
                )
              )
            ORDER BY ci.last_seen_at DESC, ci.updated_at DESC
            LIMIT 1
        ) identity_name ON true
        WHERE ap.person_id IS NULL
        ORDER BY rp.fact_count DESC, display_name
        LIMIT $2
        "#,
    )
    .bind(chat_id)
    .bind(COVERAGE_SAMPLE_LIMIT)
    .fetch_all(pool)
    .await
    .context("load running profile hint gap samples")?;
    Ok(rows
        .into_iter()
        .map(|(display_name, person_id, count)| CoverageSample {
            display_name: evidence_display_name(&display_name),
            identity_key: None,
            person_key: None,
            person_id: Some(person_id),
            reason: Some("running_facts_missing_profile_hint".to_string()),
            count: Some(count),
        })
        .collect())
}

async fn load_missing_canary_name_samples(
    pool: &PgPool,
    chat_id: Option<&str>,
) -> Result<Vec<CoverageSample>> {
    let rows = sqlx::query_as::<_, (String, String)>(
        r#"
        WITH scoped_qiwe_identities AS (
            SELECT *
            FROM qintopia_identity.channel_identities
            WHERE platform = 'qiwe'
              AND person_id IS NOT NULL
              AND COALESCE(is_bot, false) = false
              AND btrim(COALESCE(display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(COALESCE(display_name, ''))) <> 'sidecar smoke'
              AND (
                ($1::text IS NULL AND chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND chat_id = $1
                    AND metadata->>'current_qiwe_room_member' = 'true'
                )
              )
        ),
        safe_qiwe_identity_mentions AS (
            SELECT *
            FROM scoped_qiwe_identities
            WHERE COALESCE(display_name, '') !~ '[0-9]{7,}'
              AND COALESCE(display_name, '') !~ '[[:cntrl:]]'
        ),
        linked_people AS (
            SELECT DISTINCT person_id
            FROM scoped_qiwe_identities
        ),
        mention_sources AS (
            SELECT
                ci.person_id,
                ci.display_name AS mention_text
            FROM safe_qiwe_identity_mentions ci
            WHERE COALESCE(ci.display_name, '') <> ''
              AND ci.display_name !~ '^[0-9]+$'

            UNION

            SELECT
                a.person_id,
                a.alias AS mention_text
            FROM qintopia_identity.person_aliases a
            JOIN linked_people lp ON lp.person_id = a.person_id
            WHERE COALESCE(a.alias, '') <> ''
              AND a.alias !~ '^[0-9]+$'

            UNION

            SELECT
                p.id AS person_id,
                p.display_name AS mention_text
            FROM qintopia_identity.persons p
            JOIN linked_people lp ON lp.person_id = p.id
            WHERE COALESCE(p.display_name, '') <> ''
              AND p.display_name !~ '^[0-9]+$'
        ),
        safe_canary_people AS (
            SELECT DISTINCT person_id
            FROM mention_sources
            WHERE btrim(mention_text) <> ''
              AND mention_text !~ '[0-9]{7,}'
              AND mention_text !~ '[[:cntrl:]]'
              AND btrim(mention_text) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(mention_text)) <> 'sidecar smoke'
        )
        SELECT
            COALESCE(identity_name.display_name, p.display_name, '未知成员') AS display_name,
            substr(md5(p.id::text), 1, 12) AS person_key
        FROM linked_people lp
        JOIN qintopia_identity.persons p ON p.id = lp.person_id
        LEFT JOIN safe_canary_people scp ON scp.person_id = p.id
        LEFT JOIN LATERAL (
            SELECT ci.display_name
            FROM scoped_qiwe_identities ci
            WHERE ci.person_id = p.id
              AND COALESCE(ci.display_name, '') <> ''
            ORDER BY ci.last_seen_at DESC, ci.updated_at DESC
            LIMIT 1
        ) identity_name ON true
        WHERE scp.person_id IS NULL
        ORDER BY display_name, person_key
        "#,
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await
    .context("load missing answer-context canary name samples")?;
    Ok(rows
        .into_iter()
        .map(|(display_name, person_key)| CoverageSample {
            display_name: evidence_display_name(&display_name),
            identity_key: None,
            person_key: Some(person_key),
            person_id: None,
            reason: Some("missing_safe_answer_context_canary_name".to_string()),
            count: None,
        })
        .collect())
}

async fn load_answer_context_canary_specs(
    pool: &PgPool,
    chat_id: Option<&str>,
) -> Result<Vec<AnswerContextCanarySpec>> {
    let rows = sqlx::query_as::<_, (String, String, Vec<String>)>(
        r#"
        WITH scoped_qiwe_identities AS (
            SELECT *
            FROM qintopia_identity.channel_identities
            WHERE platform = 'qiwe'
              AND person_id IS NOT NULL
              AND COALESCE(is_bot, false) = false
              AND btrim(COALESCE(display_name, '')) NOT IN ('企业微信团队', '秦托邦小客服', '二花')
              AND lower(btrim(COALESCE(display_name, ''))) <> 'sidecar smoke'
              AND (
                ($1::text IS NULL AND chat_id <> '')
                OR (
                    $1::text IS NOT NULL
                    AND chat_id = $1
                    AND metadata->>'current_qiwe_room_member' = 'true'
                )
              )
        ),
        safe_qiwe_identity_mentions AS (
            SELECT *
            FROM scoped_qiwe_identities
            WHERE COALESCE(display_name, '') !~ '[0-9]{7,}'
              AND COALESCE(display_name, '') !~ '[[:cntrl:]]'
        ),
        linked_people AS (
            SELECT DISTINCT person_id
            FROM scoped_qiwe_identities
        ),
        safe_profile_term_patterns(term, needles) AS (
            VALUES
                ('跑步'::text, ARRAY['跑步','慢跑','晨跑','夜跑']::text[]),
                ('徒步'::text, ARRAY['徒步']::text[]),
                ('骑行'::text, ARRAY['骑行','骑车']::text[]),
                ('健身'::text, ARRAY['健身','力量训练']::text[]),
                ('瑜伽'::text, ARRAY['瑜伽']::text[]),
                ('游泳'::text, ARRAY['游泳']::text[]),
                ('羽毛球'::text, ARRAY['羽毛球']::text[]),
                ('篮球'::text, ARRAY['篮球']::text[]),
                ('足球'::text, ARRAY['足球']::text[]),
                ('摄影'::text, ARRAY['摄影','拍照']::text[]),
                ('视频'::text, ARRAY['视频','剪辑']::text[]),
                ('写作'::text, ARRAY['写作','写文章']::text[]),
                ('阅读'::text, ARRAY['阅读','读书']::text[]),
                ('音乐'::text, ARRAY['音乐','唱歌','乐器']::text[]),
                ('绘画'::text, ARRAY['绘画','画画']::text[]),
                ('AI'::text, ARRAY['AI','人工智能']::text[]),
                ('编程'::text, ARRAY['编程','写代码','Python','Rust','Go']::text[]),
                ('小红书'::text, ARRAY['小红书']::text[]),
                ('公众号'::text, ARRAY['公众号']::text[]),
                ('英语'::text, ARRAY['英语']::text[])
        ),
        required_profile_terms AS (
            SELECT f.person_id, array_agg(DISTINCT p.term ORDER BY p.term) AS terms
            FROM qintopia_identity.member_facts f
            JOIN linked_people lp ON lp.person_id = f.person_id
            JOIN safe_profile_term_patterns p ON EXISTS (
                SELECT 1
                FROM unnest(p.needles) AS needle
                WHERE f.fact_text ILIKE '%' || needle || '%'
            )
            WHERE f.revoked_at IS NULL
              AND (
                  f.fact_type = 'interest'
                  OR f.fact_key = 'running_activity'
              )
            GROUP BY f.person_id
        ),
        mention_sources AS (
            SELECT
                ci.person_id,
                ci.display_name AS mention_text
            FROM safe_qiwe_identity_mentions ci
            WHERE COALESCE(ci.display_name, '') <> ''
              AND ci.display_name !~ '^[0-9]+$'

            UNION

            SELECT
                a.person_id,
                a.alias AS mention_text
            FROM qintopia_identity.person_aliases a
            JOIN linked_people lp ON lp.person_id = a.person_id
            WHERE COALESCE(a.alias, '') <> ''
              AND a.alias !~ '^[0-9]+$'

            UNION

            SELECT
                p.id AS person_id,
                p.display_name AS mention_text
            FROM qintopia_identity.persons p
            JOIN linked_people lp ON lp.person_id = p.id
            WHERE COALESCE(p.display_name, '') <> ''
              AND p.display_name !~ '^[0-9]+$'
        ),
        normalized_mentions AS (
            SELECT DISTINCT
                ms.person_id,
                regexp_replace(btrim(ms.mention_text), '[[:space:]]+', ' ', 'g') AS mention_text
            FROM mention_sources ms
            WHERE COALESCE(ms.mention_text, '') <> ''
        )
        SELECT
            nm.person_id::text AS person_id,
            nm.mention_text,
            COALESCE(rpt.terms, ARRAY[]::text[]) AS required_profile_terms
        FROM normalized_mentions nm
        LEFT JOIN required_profile_terms rpt ON rpt.person_id = nm.person_id
        WHERE nm.mention_text <> ''
        ORDER BY nm.person_id::text, nm.mention_text
        "#,
    )
    .bind(chat_id)
    .fetch_all(pool)
    .await
    .context("load answer context canary specs")?;

    Ok(rows
        .into_iter()
        .filter(|(_, expected_mention, _)| is_safe_answer_context_canary_mention(expected_mention))
        .enumerate()
        .map(
            |(index, (person_id, expected_mention, required_profile_terms))| {
                AnswerContextCanarySpec {
                    id: (index + 2) as i64,
                    canary_type: "mentioned_member",
                    expected_mention,
                    canonical_key: format!("person:{person_id}"),
                    required_profile_terms,
                }
            },
        )
        .collect())
}

#[cfg(test)]
mod tests {
    use super::{
        classify_unlinked_identity_action, evidence_display_name,
        is_safe_answer_context_canary_mention, referenced_canary_specs_from_mentions,
        scope_fingerprint, speaker_canary_specs_from_mentions, AnswerContextCanarySpec,
        ACTION_CREATE_PERSON, ACTION_MANUAL_MERGE_AMBIGUOUS_DISPLAY_NAME_OR_ALIAS,
        ACTION_MANUAL_MERGE_MULTIPLE_QIWE_USER_PEOPLE, ACTION_REUSE_EXISTING_QIWE_USER,
        ACTION_REUSE_UNIQUE_DISPLAY_NAME_OR_ALIAS,
    };

    #[test]
    fn unlinked_identity_action_creates_person_for_new_qiwe_user_and_name() {
        let action = classify_unlinked_identity_action(0, 0);

        assert_eq!(action.reason, ACTION_CREATE_PERSON);
        assert_eq!(action.match_count, 0);
        assert!(!action.ambiguous);
    }

    #[test]
    fn unlinked_identity_action_reuses_existing_qiwe_user_before_name_match() {
        let action = classify_unlinked_identity_action(1, 2);

        assert_eq!(action.reason, ACTION_REUSE_EXISTING_QIWE_USER);
        assert_eq!(action.match_count, 1);
        assert!(!action.ambiguous);
    }

    #[test]
    fn unlinked_identity_action_reuses_unique_display_name_or_alias() {
        let action = classify_unlinked_identity_action(0, 1);

        assert_eq!(action.reason, ACTION_REUSE_UNIQUE_DISPLAY_NAME_OR_ALIAS);
        assert_eq!(action.match_count, 1);
        assert!(!action.ambiguous);
    }

    #[test]
    fn unlinked_identity_action_skips_conflicting_qiwe_user() {
        let action = classify_unlinked_identity_action(2, 1);

        assert_eq!(action.reason, ACTION_MANUAL_MERGE_MULTIPLE_QIWE_USER_PEOPLE);
        assert_eq!(action.match_count, 2);
        assert!(action.ambiguous);
    }

    #[test]
    fn unlinked_identity_action_skips_ambiguous_display_name_or_alias() {
        let action = classify_unlinked_identity_action(0, 3);

        assert_eq!(
            action.reason,
            ACTION_MANUAL_MERGE_AMBIGUOUS_DISPLAY_NAME_OR_ALIAS
        );
        assert_eq!(action.match_count, 3);
        assert!(action.ambiguous);
    }

    #[test]
    fn evidence_display_name_redacts_sensitive_digits_and_controls() {
        assert_eq!(
            evidence_display_name("Joey17336786728\u{0091}"),
            "Joey[敏感数字]"
        );
    }

    #[test]
    fn bootstrap_scope_fingerprint_is_stable_and_non_raw() {
        let fingerprint = scope_fingerprint("room-1");

        assert_eq!(
            fingerprint,
            "sha256:c5c4e70d823efa23b83de70ce5008d746e76bdce54e37605b967b4bfd4036356"
        );
        assert!(!fingerprint.contains("room-1"));
    }

    #[test]
    fn answer_context_canary_mention_requires_safe_display_text() {
        assert!(is_safe_answer_context_canary_mention("Paxon"));
        assert!(!is_safe_answer_context_canary_mention("Joey17336786728"));
        assert!(!is_safe_answer_context_canary_mention("企业微信团队"));
        assert!(!is_safe_answer_context_canary_mention("秦托邦小客服"));
        assert!(!is_safe_answer_context_canary_mention("二花"));
        assert!(!is_safe_answer_context_canary_mention("Sidecar Smoke"));
    }

    #[test]
    fn speaker_canary_specs_are_one_per_person_and_keep_required_terms() {
        let specs = speaker_canary_specs_from_mentions(&[
            AnswerContextCanarySpec {
                id: 2,
                canary_type: "mentioned_member",
                expected_mention: "小乔".to_string(),
                canonical_key: "person:paxon".to_string(),
                required_profile_terms: vec!["跑步".to_string()],
            },
            AnswerContextCanarySpec {
                id: 3,
                canary_type: "mentioned_member",
                expected_mention: "Paxon".to_string(),
                canonical_key: "person:paxon".to_string(),
                required_profile_terms: vec!["AI".to_string()],
            },
            AnswerContextCanarySpec {
                id: 4,
                canary_type: "mentioned_member",
                expected_mention: "Cici".to_string(),
                canonical_key: "person:cici".to_string(),
                required_profile_terms: vec![],
            },
        ]);

        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].canary_type, "speaker_self");
        assert_eq!(specs[0].canonical_key, "person:cici");
        assert_eq!(specs[1].canonical_key, "person:paxon");
        assert_eq!(specs[1].expected_speaker_label, "小乔");
        assert_eq!(specs[1].required_profile_terms, vec!["AI", "跑步"]);
    }

    #[test]
    fn referenced_canary_specs_are_one_per_person_and_keep_required_terms() {
        let specs = referenced_canary_specs_from_mentions(&[
            AnswerContextCanarySpec {
                id: 2,
                canary_type: "mentioned_member",
                expected_mention: "小乔".to_string(),
                canonical_key: "person:paxon".to_string(),
                required_profile_terms: vec!["跑步".to_string()],
            },
            AnswerContextCanarySpec {
                id: 3,
                canary_type: "mentioned_member",
                expected_mention: "Paxon".to_string(),
                canonical_key: "person:paxon".to_string(),
                required_profile_terms: vec!["AI".to_string()],
            },
            AnswerContextCanarySpec {
                id: 4,
                canary_type: "mentioned_member",
                expected_mention: "Cici".to_string(),
                canonical_key: "person:cici".to_string(),
                required_profile_terms: vec![],
            },
        ]);

        assert_eq!(specs.len(), 2);
        assert_eq!(specs[0].canary_type, "referenced_member");
        assert_eq!(specs[0].canonical_key, "person:cici");
        assert_eq!(specs[1].canonical_key, "person:paxon");
        assert_eq!(specs[1].expected_referenced_label, "小乔");
        assert_eq!(specs[1].required_profile_terms, vec!["AI", "跑步"]);
    }
}
