use std::{
    collections::{BTreeMap, BTreeSet},
    env,
    io::{self, Read},
};

use anyhow::{bail, Context, Result};
use serde::{Deserialize, Serialize};
#[cfg(test)]
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::{config::Cli, db};

const POLICY_SCHEMA_VERSION: u8 = 3;
const MAX_POLICY_INPUT_BYTES: u64 = 64 * 1024;
const POLICY_APPROVAL_ENV: &str = "QINTOPIA_XIAOMAN_CONVERSATION_POLICY_APPROVAL";
const POLICY_APPROVAL_PHRASE: &str = "approved-production-xiaoman-conversation-policy-v3";
const POLICY_DATABASE_HASH_ENV: &str = "QINTOPIA_XIAOMAN_CONVERSATION_POLICY_DATABASE_URL_SHA256";
pub(crate) const INGRESS_ALLOWED_CHAT_IDS_ENV: &str =
    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS";
pub(crate) const INGRESS_ALLOWED_USER_IDS_ENV: &str =
    "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS";
pub(crate) const POSTER_PRODUCTION_CAPABILITY: &str = "poster_production_request";
pub(crate) const POSTER_STATUS_CAPABILITY: &str = "poster_workflow_status";

#[derive(Debug, Deserialize)]
#[serde(deny_unknown_fields)]
struct PolicyApplyInput {
    schema_version: u8,
    policies: Vec<ConversationPolicyInput>,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(deny_unknown_fields)]
struct ConversationPolicyInput {
    platform: String,
    chat_id: String,
    conversation_type: String,
    audience_class: String,
    allowed_capabilities: Vec<String>,
    return_mode: String,
    initiation_rule: String,
    status_visibility: String,
    enabled: bool,
    #[serde(default)]
    reviewer_user_ids: Vec<String>,
}

#[derive(Debug)]
struct ValidatedPolicy {
    platform: String,
    conversation_ref: String,
    conversation_type: String,
    audience_class: String,
    allowed_capabilities: Vec<String>,
    return_mode: String,
    initiation_rule: String,
    status_visibility: String,
    enabled: bool,
    reviewer_actor_refs: Vec<String>,
    policy_digest: String,
}

#[derive(Debug, Serialize)]
struct AppliedPolicy {
    conversation_ref: String,
    policy_digest: String,
    policy_version: i64,
    enabled: bool,
    deduped: bool,
    reviewer_count: usize,
}

#[derive(Debug, Serialize)]
struct PolicyApplyReport {
    success: bool,
    action_status: &'static str,
    input_count: usize,
    created_version_count: usize,
    deduped_count: usize,
    policies: Vec<AppliedPolicy>,
    database_url_sha256: String,
    approved_database_url_sha256_matched: bool,
    external_calls_executed: bool,
    sensitive_fields_redacted: bool,
}

#[derive(Debug)]
struct PolicyApplyBoundary {
    allowed_chat_ids: BTreeSet<String>,
    allowed_user_ids: BTreeSet<String>,
    database_url_sha256: String,
}

impl PolicyApplyBoundary {
    fn from_env(database_url: &str) -> Result<Self> {
        if env::var(POLICY_APPROVAL_ENV).ok().as_deref() != Some(POLICY_APPROVAL_PHRASE) {
            bail!("Xiaoman conversation policy owner approval is required");
        }
        let expected_hash = required_env(POLICY_DATABASE_HASH_ENV)?;
        let actual_hash = sha256_hex(database_url.as_bytes());
        if !is_lower_hex(&expected_hash, 64) || expected_hash != actual_hash {
            bail!("Xiaoman conversation policy database binding is invalid");
        }
        Ok(Self {
            allowed_chat_ids: required_identifier_set(INGRESS_ALLOWED_CHAT_IDS_ENV)?,
            allowed_user_ids: required_identifier_set(INGRESS_ALLOWED_USER_IDS_ENV)?,
            database_url_sha256: actual_hash,
        })
    }
}

pub async fn run_apply(cli: &Cli, stdin: bool) -> Result<()> {
    if !stdin {
        bail!("conversation-policy-apply requires --stdin");
    }
    let database_url = cli.database_url_required()?;
    let boundary = PolicyApplyBoundary::from_env(database_url)?;
    let input = read_policy_input()?;
    let policies = validate_policy_input(input, &boundary)?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let applied = apply_policies(&pool, policies).await?;
    let created_version_count = applied.iter().filter(|item| !item.deduped).count();
    let deduped_count = applied.len() - created_version_count;
    println!(
        "{}",
        serde_json::to_string_pretty(&PolicyApplyReport {
            success: true,
            action_status: "conversation_policies_applied",
            input_count: applied.len(),
            created_version_count,
            deduped_count,
            policies: applied,
            database_url_sha256: boundary.database_url_sha256,
            approved_database_url_sha256_matched: true,
            external_calls_executed: false,
            sensitive_fields_redacted: true,
        })?
    );
    Ok(())
}

fn read_policy_input() -> Result<PolicyApplyInput> {
    let mut bytes = Vec::new();
    io::stdin()
        .take(MAX_POLICY_INPUT_BYTES + 1)
        .read_to_end(&mut bytes)
        .context("read Xiaoman conversation policies")?;
    if bytes.is_empty() || bytes.len() as u64 > MAX_POLICY_INPUT_BYTES {
        bail!("conversation policy input length is invalid");
    }
    serde_json::from_slice(&bytes).context("parse Xiaoman conversation policies")
}

fn validate_policy_input(
    input: PolicyApplyInput,
    boundary: &PolicyApplyBoundary,
) -> Result<Vec<ValidatedPolicy>> {
    if input.schema_version != POLICY_SCHEMA_VERSION {
        bail!("unsupported conversation policy schema version");
    }
    if input.policies.is_empty() || input.policies.len() > 100 {
        bail!("conversation policy count is invalid");
    }
    let mut by_ref = BTreeMap::new();
    for policy in input.policies {
        let validated = validate_policy(policy, boundary)?;
        if by_ref
            .insert(validated.conversation_ref.clone(), validated)
            .is_some()
        {
            bail!("conversation policy input contains a duplicate conversation");
        }
    }
    Ok(by_ref.into_values().collect())
}

fn validate_policy(
    policy: ConversationPolicyInput,
    boundary: &PolicyApplyBoundary,
) -> Result<ValidatedPolicy> {
    if policy.platform != "feishu" || !valid_external_id(&policy.chat_id) {
        bail!("conversation policy platform or chat identity is invalid");
    }
    if !boundary.allowed_chat_ids.contains(&policy.chat_id) {
        bail!("conversation policy chat exceeds the deployment allowlist");
    }

    let mut allowed_capabilities = policy
        .allowed_capabilities
        .into_iter()
        .map(|value| value.trim().to_string())
        .collect::<BTreeSet<_>>();
    if allowed_capabilities.iter().any(|value| {
        !matches!(
            value.as_str(),
            POSTER_PRODUCTION_CAPABILITY | POSTER_STATUS_CAPABILITY
        )
    }) {
        bail!("conversation policy contains an unsupported capability");
    }
    if policy.audience_class == "external_community" {
        if !allowed_capabilities.is_empty() {
            bail!("external community policy cannot allow poster capabilities");
        }
    } else if !allowed_capabilities.remove(POSTER_PRODUCTION_CAPABILITY) {
        bail!("trusted conversation policy must allow poster production");
    } else {
        allowed_capabilities.insert(POSTER_PRODUCTION_CAPABILITY.to_string());
    }
    let allowed_capabilities = allowed_capabilities.into_iter().collect::<Vec<_>>();

    let expected = match policy.audience_class.as_str() {
        "private" => ("direct", "direct_chat", "direct_message", "requester"),
        "internal_collaboration" => (
            "group",
            "thread_reply",
            "explicit_bot_mention",
            "conversation_members",
        ),
        "external_community" => (
            policy.conversation_type.as_str(),
            "none",
            "disabled",
            "none",
        ),
        _ => bail!("conversation audience class is invalid"),
    };
    if !matches!(policy.conversation_type.as_str(), "direct" | "group")
        || policy.conversation_type != expected.0
        || policy.return_mode != expected.1
        || policy.initiation_rule != expected.2
        || policy.status_visibility != expected.3
    {
        bail!("conversation policy semantics are invalid");
    }

    if policy.reviewer_user_ids.len() > 100 {
        bail!("conversation reviewer count is invalid");
    }
    if policy.audience_class != "internal_collaboration" && !policy.reviewer_user_ids.is_empty() {
        bail!("only internal collaboration policies may configure reviewers");
    }
    let mut reviewer_actor_refs = BTreeSet::new();
    for reviewer in policy.reviewer_user_ids {
        if !valid_external_id(&reviewer) || !boundary.allowed_user_ids.contains(&reviewer) {
            bail!("conversation reviewer exceeds the deployment allowlist");
        }
        reviewer_actor_refs.insert(actor_ref(&policy.platform, &reviewer));
    }
    let reviewer_actor_refs = reviewer_actor_refs.into_iter().collect::<Vec<_>>();
    let conversation_ref = conversation_ref(&policy.platform, &policy.chat_id);
    let policy_digest = digest(&[
        "conversation-policy-v3",
        &policy.platform,
        &conversation_ref,
        &policy.conversation_type,
        &policy.audience_class,
        &allowed_capabilities.join(","),
        &policy.return_mode,
        &policy.initiation_rule,
        &policy.status_visibility,
        if policy.enabled {
            "enabled"
        } else {
            "disabled"
        },
        &reviewer_actor_refs.join(","),
    ]);
    Ok(ValidatedPolicy {
        platform: policy.platform,
        conversation_ref,
        conversation_type: policy.conversation_type,
        audience_class: policy.audience_class,
        allowed_capabilities,
        return_mode: policy.return_mode,
        initiation_rule: policy.initiation_rule,
        status_visibility: policy.status_visibility,
        enabled: policy.enabled,
        reviewer_actor_refs,
        policy_digest,
    })
}

async fn apply_policies(
    pool: &PgPool,
    policies: Vec<ValidatedPolicy>,
) -> Result<Vec<AppliedPolicy>> {
    let mut tx = pool
        .begin()
        .await
        .context("begin conversation policy apply")?;
    let mut applied = Vec::with_capacity(policies.len());
    for policy in policies {
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1, 0))")
            .bind(&policy.conversation_ref)
            .execute(&mut *tx)
            .await
            .context("lock conversation policy identity")?;
        let latest = sqlx::query(
            r#"
            SELECT id, policy_version, policy_digest, enabled
            FROM qintopia_agent_os.conversation_policies
            WHERE platform = $1 AND conversation_ref = $2
            ORDER BY policy_version DESC
            LIMIT 1
            FOR UPDATE
            "#,
        )
        .bind(&policy.platform)
        .bind(&policy.conversation_ref)
        .fetch_optional(&mut *tx)
        .await
        .context("load latest conversation policy")?;
        if let Some(row) = latest.as_ref() {
            let latest_digest: String = row.try_get("policy_digest")?;
            let latest_enabled: bool = row.try_get("enabled")?;
            if latest_digest == policy.policy_digest && latest_enabled == policy.enabled {
                applied.push(AppliedPolicy {
                    conversation_ref: policy.conversation_ref,
                    policy_digest: policy.policy_digest,
                    policy_version: row.try_get("policy_version")?,
                    enabled: policy.enabled,
                    deduped: true,
                    reviewer_count: policy.reviewer_actor_refs.len(),
                });
                continue;
            }
        }

        let next_version = latest
            .as_ref()
            .map(|row| row.try_get::<i64, _>("policy_version"))
            .transpose()?
            .unwrap_or(0)
            + 1;
        sqlx::query(
            r#"
            UPDATE qintopia_agent_os.conversation_policies
            SET enabled = false, updated_at = now()
            WHERE platform = $1 AND conversation_ref = $2 AND enabled
            "#,
        )
        .bind(&policy.platform)
        .bind(&policy.conversation_ref)
        .execute(&mut *tx)
        .await
        .context("disable previous conversation policy")?;
        let policy_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_agent_os.conversation_policies
                (platform, conversation_ref, conversation_type, audience_class,
                 allowed_capabilities, return_mode, initiation_rule, status_visibility,
                 policy_version, policy_digest, enabled)
            VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11)
            RETURNING id
            "#,
        )
        .bind(&policy.platform)
        .bind(&policy.conversation_ref)
        .bind(&policy.conversation_type)
        .bind(&policy.audience_class)
        .bind(&policy.allowed_capabilities)
        .bind(&policy.return_mode)
        .bind(&policy.initiation_rule)
        .bind(&policy.status_visibility)
        .bind(next_version)
        .bind(&policy.policy_digest)
        .bind(policy.enabled)
        .fetch_one(&mut *tx)
        .await
        .context("insert conversation policy version")?;
        for actor_ref in &policy.reviewer_actor_refs {
            sqlx::query(
                r#"
                INSERT INTO qintopia_agent_os.conversation_policy_actors
                    (policy_id, actor_ref, actor_role)
                VALUES ($1, $2, 'reviewer')
                "#,
            )
            .bind(policy_id)
            .bind(actor_ref)
            .execute(&mut *tx)
            .await
            .context("insert conversation policy reviewer")?;
        }
        applied.push(AppliedPolicy {
            conversation_ref: policy.conversation_ref,
            policy_digest: policy.policy_digest,
            policy_version: next_version,
            enabled: policy.enabled,
            deduped: false,
            reviewer_count: policy.reviewer_actor_refs.len(),
        });
    }
    tx.commit().await.context("commit conversation policies")?;
    Ok(applied)
}

pub(crate) fn conversation_ref(platform: &str, chat_id: &str) -> String {
    digest(&["conversation-ref-v3", platform, chat_id])
}

pub(crate) fn actor_ref(platform: &str, user_id: &str) -> String {
    digest(&["poster-actor-v1", platform, user_id])
}

pub(crate) fn source_message_ref(platform: &str, message_id: &str) -> String {
    digest(&["poster-message-v1", platform, message_id])
}

fn digest(parts: &[&str]) -> String {
    let mut hasher = Sha256::new();
    for part in parts {
        hasher.update(part.as_bytes());
        hasher.update([0]);
    }
    format!("sha256:{:x}", hasher.finalize())
}

fn required_env(name: &str) -> Result<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty())
        .with_context(|| format!("{name} is required"))
}

pub(crate) fn required_identifier_set(name: &str) -> Result<BTreeSet<String>> {
    let values = required_env(name)?
        .split(',')
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if values.is_empty() || values.iter().any(|value| !valid_external_id(value)) {
        bail!("{name} contains an invalid identifier");
    }
    Ok(values)
}

pub(crate) fn valid_external_id(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 240
        && value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-' | b':' | b'.'))
}

fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[cfg(feature = "postgres-integration-tests")]
    fn postgres_integration_database_url() -> String {
        assert_eq!(
            env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE").as_deref(),
            Ok("1"),
            "PostgreSQL integration test requires the explicit apply-smoke guard"
        );
        let database_url = env::var("QINTOPIA_SIDECAR_DATABASE_URL")
            .expect("PostgreSQL integration test requires QINTOPIA_SIDECAR_DATABASE_URL");
        let parsed = url::Url::parse(&database_url).expect("integration database URL must parse");
        assert!(
            matches!(parsed.host_str(), Some("127.0.0.1" | "::1")),
            "PostgreSQL integration test requires a literal loopback database"
        );
        assert_eq!(parsed.path().trim_start_matches('/'), "qintopia_test");
        database_url
    }

    fn boundary() -> PolicyApplyBoundary {
        PolicyApplyBoundary {
            allowed_chat_ids: ["oc_direct", "oc_internal", "oc_external"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            allowed_user_ids: ["ou_requester", "ou_reviewer"]
                .into_iter()
                .map(str::to_string)
                .collect(),
            database_url_sha256: "a".repeat(64),
        }
    }

    fn internal_policy() -> ConversationPolicyInput {
        ConversationPolicyInput {
            platform: "feishu".to_string(),
            chat_id: "oc_internal".to_string(),
            conversation_type: "group".to_string(),
            audience_class: "internal_collaboration".to_string(),
            allowed_capabilities: vec![
                POSTER_STATUS_CAPABILITY.to_string(),
                POSTER_PRODUCTION_CAPABILITY.to_string(),
            ],
            return_mode: "thread_reply".to_string(),
            initiation_rule: "explicit_bot_mention".to_string(),
            status_visibility: "conversation_members".to_string(),
            enabled: true,
            reviewer_user_ids: vec!["ou_reviewer".to_string()],
        }
    }

    #[test]
    fn policy_hashes_raw_chat_and_reviewer_ids() {
        let policy = validate_policy(internal_policy(), &boundary()).expect("policy validates");
        let serialized = serde_json::to_string(&json!({
            "conversation_ref": policy.conversation_ref,
            "policy_digest": policy.policy_digest,
            "reviewers": policy.reviewer_actor_refs,
        }))
        .unwrap();
        assert!(!serialized.contains("oc_internal"));
        assert!(!serialized.contains("ou_reviewer"));
        assert!(serialized.contains("sha256:"));
    }

    #[test]
    fn policy_digest_is_order_independent_for_capabilities_and_reviewers() {
        let mut first = internal_policy();
        first.reviewer_user_ids = vec!["ou_reviewer".to_string(), "ou_requester".to_string()];
        let mut second = first.clone();
        second.allowed_capabilities.reverse();
        second.reviewer_user_ids.reverse();
        let first = validate_policy(first, &boundary()).unwrap();
        let second = validate_policy(second, &boundary()).unwrap();
        assert_eq!(first.policy_digest, second.policy_digest);
    }

    #[test]
    fn external_community_cannot_enable_poster_or_reviewers() {
        let mut policy = internal_policy();
        policy.chat_id = "oc_external".to_string();
        policy.audience_class = "external_community".to_string();
        policy.return_mode = "none".to_string();
        policy.initiation_rule = "disabled".to_string();
        policy.status_visibility = "none".to_string();
        assert!(validate_policy(policy, &boundary()).is_err());
    }

    #[test]
    fn deployment_allowlists_are_an_unavoidable_ceiling() {
        let mut wrong_chat = internal_policy();
        wrong_chat.chat_id = "oc_not_allowed".to_string();
        assert!(validate_policy(wrong_chat, &boundary()).is_err());
        let mut wrong_reviewer = internal_policy();
        wrong_reviewer.reviewer_user_ids = vec!["ou_not_allowed".to_string()];
        assert!(validate_policy(wrong_reviewer, &boundary()).is_err());
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
    async fn postgres_policy_apply_is_versioned_and_idempotent() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect guarded conversation policy database");
        db::run_migrations(&pool)
            .await
            .expect("migrate guarded conversation policy database");
        let suffix = Uuid::new_v4().simple().to_string();
        let chat_id = format!("oc_{suffix}");
        let reviewer_id = format!("ou_{suffix}");
        let boundary = PolicyApplyBoundary {
            allowed_chat_ids: [chat_id.clone()].into_iter().collect(),
            allowed_user_ids: [reviewer_id.clone()].into_iter().collect(),
            database_url_sha256: "a".repeat(64),
        };
        let input = || ConversationPolicyInput {
            platform: "feishu".to_string(),
            chat_id: chat_id.clone(),
            conversation_type: "group".to_string(),
            audience_class: "internal_collaboration".to_string(),
            allowed_capabilities: vec![POSTER_PRODUCTION_CAPABILITY.to_string()],
            return_mode: "thread_reply".to_string(),
            initiation_rule: "explicit_bot_mention".to_string(),
            status_visibility: "conversation_members".to_string(),
            enabled: true,
            reviewer_user_ids: vec![reviewer_id.clone()],
        };
        let first = apply_policies(
            &pool,
            vec![validate_policy(input(), &boundary).expect("validate first policy")],
        )
        .await
        .expect("apply first policy");
        let duplicate = apply_policies(
            &pool,
            vec![validate_policy(input(), &boundary).expect("validate duplicate policy")],
        )
        .await
        .expect("dedupe policy");
        let mut disabled_input = input();
        disabled_input.enabled = false;
        let disabled = apply_policies(
            &pool,
            vec![validate_policy(disabled_input, &boundary).expect("validate disabled policy")],
        )
        .await
        .expect("disable policy with a new version");
        assert_eq!(first[0].policy_version, 1);
        assert!(!first[0].deduped);
        assert_eq!(duplicate[0].policy_version, 1);
        assert!(duplicate[0].deduped);
        assert_eq!(disabled[0].policy_version, 2);
        assert!(!disabled[0].enabled);
        let policy_ref = conversation_ref("feishu", &chat_id);
        let counts: (i64, i64, i64) = sqlx::query_as(
            r#"
            SELECT count(*), count(*) FILTER (WHERE enabled),
                   count(actor.actor_ref)
            FROM qintopia_agent_os.conversation_policies policy
            LEFT JOIN qintopia_agent_os.conversation_policy_actors actor
              ON actor.policy_id = policy.id
            WHERE policy.conversation_ref = $1
            "#,
        )
        .bind(policy_ref)
        .fetch_one(&pool)
        .await
        .expect("read versioned conversation policies");
        assert_eq!(counts, (2, 0, 2));
    }
}
