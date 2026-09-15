use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use serde_json::{json, Value};
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::space_configuration::TrustedSpaceSession;

const DEFAULT_POLICY_KEY: &str = "default";
const SPACE_TURN_CALLER: &str = "erhua";
const SPACE_TURN_WORK_ITEM_TYPE: &str = "qiwe_group_turn";
const SPACE_TURN_SCOPE_BINDING: &str = "trusted_session_space_id";
const SPACE_TURN_INVOCATION_BOUNDARY: &str = "erhua.space_turn";
const MAX_IDENTITY_BYTES: usize = 4_000;
const MAX_KNOWLEDGE_SCOPES: usize = 32;
const MAX_KNOWLEDGE_SCOPE_BYTES: usize = 128;
const MAX_CAPABILITY_GRANTS: usize = 32;
const MAX_CAPABILITY_KEY_BYTES: usize = 160;
const MAX_QUOTA_LIMITS: usize = 16;
const MAX_QUOTA_LIMIT: u64 = 1_000_000_000;

pub(crate) const SPACE_TURN_CAPABILITY_KEYS: [&str; 11] = [
    "erhua.knowledge.community",
    "erhua.knowledge.public",
    "erhua.qiwe_handoff_to_human",
    "erhua.qiwe_request_direct_contact",
    "erhua.qiwe_revoke_message",
    "erhua.qiwe_send_direct_message",
    "erhua.qiwe_send_location_card",
    "erhua.qiwe_send_rich_message",
    "erhua.qiwe_voice_to_text",
    "erhua.workflow.complaint",
    "erhua.workflow.sales",
];

#[derive(Debug, Clone, Default, PartialEq, Eq)]
struct PolicyProjection {
    identity: String,
    knowledge_scope: Vec<String>,
    capability_grants: BTreeSet<String>,
}

#[derive(Debug)]
struct ResolvedPolicy {
    _space_id: Uuid,
    policy_found: bool,
    projection: PolicyProjection,
}

pub(crate) async fn context(pool: &PgPool, session: &TrustedSpaceSession) -> Result<Value> {
    let resolved = resolve_policy(pool, session).await?;
    let effective_capabilities = load_effective_capabilities(pool, &resolved.projection).await?;
    Ok(context_response(
        resolved.policy_found,
        resolved.projection,
        effective_capabilities,
    ))
}

pub(crate) async fn authorize(
    pool: &PgPool,
    session: &TrustedSpaceSession,
    capability_key: &str,
) -> Result<Value> {
    validate_capability_key(capability_key)?;
    let resolved = resolve_policy(pool, session).await?;
    let authorized = if is_space_turn_capability(capability_key)
        && resolved
            .projection
            .capability_grants
            .contains(capability_key)
    {
        capability_is_globally_invocable(pool, capability_key).await?
    } else {
        false
    };
    Ok(authorization_response(capability_key, authorized))
}

async fn resolve_policy(pool: &PgPool, session: &TrustedSpaceSession) -> Result<ResolvedPolicy> {
    validate_trusted_session(session)?;
    let row = sqlx::query(
        r#"
        SELECT conversation.id AS space_id, policy.policy_config
        FROM qintopia_messages.conversations conversation
        LEFT JOIN qintopia_agent_os.space_policy_versions policy
          ON policy.space_id = conversation.id
         AND policy.definition_key = $3
         AND policy.status = 'active'
        WHERE conversation.tenant_id = 'qintopia'
          AND conversation.platform = $1
          AND conversation.chat_id = $2
          AND conversation.chat_type = 'group'
          AND conversation.status = 'active'
        LIMIT 1
        "#,
    )
    .bind(&session.platform)
    .bind(&session.conversation_id)
    .bind(DEFAULT_POLICY_KEY)
    .fetch_optional(pool)
    .await
    .context("resolve trusted current Space turn policy")?
    .context("trusted current QiWe group Space is not registered")?;

    let policy_config: Option<Value> = row.try_get("policy_config")?;
    let policy_found = policy_config.is_some();
    let projection = policy_config
        .as_ref()
        .map(project_policy)
        .transpose()?
        .unwrap_or_default();
    Ok(ResolvedPolicy {
        _space_id: row.try_get("space_id")?,
        policy_found,
        projection,
    })
}

async fn load_effective_capabilities(
    pool: &PgPool,
    projection: &PolicyProjection,
) -> Result<Vec<String>> {
    let candidates = policy_catalog_candidates(projection);
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        r#"
        SELECT capability_key
        FROM qintopia_agent_os.capabilities
        WHERE capability_key = ANY($1)
          AND enabled
          AND provider_agent = 'erhua'
          AND $2 = ANY(allowed_callers)
          AND $3 = ANY(allowed_work_item_types)
          AND metadata ->> 'space_turn_invocable' = 'true'
          AND metadata ->> 'space_scope_binding' = $4
          AND metadata ->> 'invocation_boundary' = $5
          AND (
                capability_key <> 'erhua.knowledge.community'
                OR metadata ->> 'knowledge_scope_enforced' = 'true'
              )
        ORDER BY capability_key
        "#,
    )
    .bind(&candidates)
    .bind(SPACE_TURN_CALLER)
    .bind(SPACE_TURN_WORK_ITEM_TYPE)
    .bind(SPACE_TURN_SCOPE_BINDING)
    .bind(SPACE_TURN_INVOCATION_BOUNDARY)
    .fetch_all(pool)
    .await
    .context("load effective current Space turn capabilities")?;
    rows.into_iter()
        .map(|row| {
            row.try_get("capability_key")
                .context("read Space turn capability key")
        })
        .collect()
}

async fn capability_is_globally_invocable(pool: &PgPool, capability_key: &str) -> Result<bool> {
    sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM qintopia_agent_os.capabilities
            WHERE capability_key = $1
              AND enabled
              AND provider_agent = 'erhua'
              AND $2 = ANY(allowed_callers)
              AND $3 = ANY(allowed_work_item_types)
              AND metadata ->> 'space_turn_invocable' = 'true'
              AND metadata ->> 'space_scope_binding' = $4
              AND metadata ->> 'invocation_boundary' = $5
              AND (
                    capability_key <> 'erhua.knowledge.community'
                    OR metadata ->> 'knowledge_scope_enforced' = 'true'
                  )
        )
        "#,
    )
    .bind(capability_key)
    .bind(SPACE_TURN_CALLER)
    .bind(SPACE_TURN_WORK_ITEM_TYPE)
    .bind(SPACE_TURN_SCOPE_BINDING)
    .bind(SPACE_TURN_INVOCATION_BOUNDARY)
    .fetch_one(pool)
    .await
    .context("authorize current Space turn capability")
}

fn validate_trusted_session(session: &TrustedSpaceSession) -> Result<()> {
    if session.platform != "qiwe" || session.conversation_type != "group" {
        bail!("trusted current QiWe group session is required");
    }
    for (name, value, max_bytes) in [
        (
            "conversation_id",
            session.conversation_id.as_str(),
            200usize,
        ),
        (
            "requester_user_id",
            session.requester_user_id.as_str(),
            200usize,
        ),
        (
            "source_message_id",
            session.source_message_id.as_str(),
            240usize,
        ),
    ] {
        if value.is_empty()
            || value.len() > max_bytes
            || value.chars().any(char::is_whitespace)
            || value.chars().any(char::is_control)
        {
            bail!("trusted Space turn session {name} is invalid");
        }
    }
    Ok(())
}

pub(crate) fn validate_policy_config(value: &Value) -> Result<()> {
    project_policy(value)?;
    Ok(())
}

fn project_policy(value: &Value) -> Result<PolicyProjection> {
    let object = value
        .as_object()
        .context("active Space policy must be an object")?;
    for key in object.keys() {
        if !matches!(
            key.as_str(),
            "identity"
                | "knowledge_scope"
                | "capability_grants"
                | "capability_revocations"
                | "quota_declaration"
        ) {
            bail!("active Space policy contains an unsupported field");
        }
    }
    let identity = match object.get("identity") {
        None => String::new(),
        Some(value) => {
            let value = value
                .as_str()
                .context("active Space policy identity must be a string")?
                .trim();
            if value.len() > MAX_IDENTITY_BYTES || value.chars().any(invalid_context_character) {
                bail!("active Space policy identity is invalid");
            }
            value.to_string()
        }
    };
    let knowledge_scope = project_string_array(
        object.get("knowledge_scope"),
        "knowledge_scope",
        MAX_KNOWLEDGE_SCOPES,
        MAX_KNOWLEDGE_SCOPE_BYTES,
        false,
    )?;
    let mut capability_grants = project_string_array(
        object.get("capability_grants"),
        "capability_grants",
        MAX_CAPABILITY_GRANTS,
        MAX_CAPABILITY_KEY_BYTES,
        true,
    )?
    .into_iter()
    .collect::<BTreeSet<_>>();
    let capability_revocations = project_string_array(
        object.get("capability_revocations"),
        "capability_revocations",
        MAX_CAPABILITY_GRANTS,
        MAX_CAPABILITY_KEY_BYTES,
        true,
    )?;
    for capability_key in capability_revocations {
        capability_grants.remove(&capability_key);
    }
    validate_quota_declaration(object.get("quota_declaration"))?;
    Ok(PolicyProjection {
        identity,
        knowledge_scope,
        capability_grants,
    })
}

fn validate_quota_declaration(value: Option<&Value>) -> Result<()> {
    let Some(value) = value else {
        return Ok(());
    };
    let object = value
        .as_object()
        .context("Space policy quota_declaration must be an object")?;
    if object.get("enforcement").and_then(Value::as_str) != Some("reserved_non_enforced")
        || object
            .keys()
            .any(|key| !matches!(key.as_str(), "enforcement" | "limits"))
    {
        bail!("Space policy quota_declaration must be explicitly reserved and non-enforced");
    }
    let Some(limits) = object.get("limits") else {
        return Ok(());
    };
    let limits = limits
        .as_object()
        .context("Space policy quota_declaration limits must be an object")?;
    if limits.len() > MAX_QUOTA_LIMITS {
        bail!("Space policy quota_declaration has too many limits");
    }
    for (key, value) in limits {
        validate_knowledge_scope(key)?;
        value
            .as_u64()
            .filter(|limit| *limit > 0 && *limit <= MAX_QUOTA_LIMIT)
            .context("Space policy quota_declaration limit is invalid")?;
    }
    Ok(())
}

fn project_string_array(
    value: Option<&Value>,
    field: &str,
    max_items: usize,
    max_bytes: usize,
    capability_keys: bool,
) -> Result<Vec<String>> {
    let Some(value) = value else {
        return Ok(Vec::new());
    };
    let values = value
        .as_array()
        .with_context(|| format!("active Space policy {field} must be an array"))?;
    if values.len() > max_items {
        bail!("active Space policy {field} has too many entries");
    }
    let mut projected = Vec::with_capacity(values.len());
    for value in values {
        let value = value
            .as_str()
            .with_context(|| format!("active Space policy {field} entries must be strings"))?
            .trim();
        if value.is_empty() || value.len() > max_bytes || value.chars().any(char::is_control) {
            bail!("active Space policy {field} contains an invalid entry");
        }
        if capability_keys {
            validate_capability_key(value)?;
        } else {
            validate_knowledge_scope(value)?;
        }
        projected.push(value.to_string());
    }
    projected.sort();
    projected.dedup();
    Ok(projected)
}

fn invalid_context_character(character: char) -> bool {
    character.is_control() && !matches!(character, '\n' | '\r' | '\t')
}

fn validate_capability_key(value: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > MAX_CAPABILITY_KEY_BYTES
        || !value.is_ascii()
        || !value.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || b"_.-".contains(&byte)
        })
    {
        bail!("Space turn capability key is invalid");
    }
    Ok(())
}

fn validate_knowledge_scope(value: &str) -> Result<()> {
    if value.len() > MAX_KNOWLEDGE_SCOPE_BYTES
        || !value.is_ascii()
        || !value.bytes().enumerate().all(|(index, byte)| {
            byte.is_ascii_lowercase()
                || byte.is_ascii_digit()
                || (index > 0 && b"._:-".contains(&byte))
        })
    {
        bail!("Space turn knowledge scope key is invalid");
    }
    Ok(())
}

fn is_space_turn_capability(capability_key: &str) -> bool {
    SPACE_TURN_CAPABILITY_KEYS
        .binary_search(&capability_key)
        .is_ok()
}

fn policy_catalog_candidates(projection: &PolicyProjection) -> Vec<String> {
    SPACE_TURN_CAPABILITY_KEYS
        .iter()
        .filter(|key| {
            projection
                .capability_grants
                .iter()
                .any(|grant| grant.as_str() == **key)
        })
        .map(|key| (*key).to_string())
        .collect()
}

fn context_response(
    policy_found: bool,
    projection: PolicyProjection,
    effective_capabilities: Vec<String>,
) -> Value {
    json!({
        "success": true,
        "policy_found": policy_found,
        "identity": projection.identity,
        "knowledge_scope": projection.knowledge_scope,
        "effective_capabilities": effective_capabilities,
        "external_send_executed": false
    })
}

fn authorization_response(capability_key: &str, authorized: bool) -> Value {
    json!({
        "success": true,
        "authorized": authorized,
        "capability_key": capability_key,
        "external_send_executed": false
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    fn session(chat_id: &str) -> TrustedSpaceSession {
        TrustedSpaceSession {
            platform: "qiwe".to_string(),
            conversation_type: "group".to_string(),
            conversation_id: chat_id.to_string(),
            requester_user_id: "current-user".to_string(),
            source_message_id: "current-message".to_string(),
            source_message_text: None,
        }
    }

    #[test]
    fn policy_projection_is_strict_bounded_and_deduplicated() {
        let projection = project_policy(&json!({
            "identity": "  Community assistant  ",
            "knowledge_scope": ["building-a", "public", "building-a"],
            "capability_grants": [
                "erhua.qiwe_send_location_card",
                "erhua.unrelated_capability",
                "erhua.qiwe_send_location_card"
            ],
            "quota_declaration": {
                "enforcement": "reserved_non_enforced",
                "limits": {"daily_invocations": 10}
            }
        }))
        .expect("project policy");
        assert_eq!(projection.identity, "Community assistant");
        assert_eq!(projection.knowledge_scope, vec!["building-a", "public"]);
        assert_eq!(
            policy_catalog_candidates(&projection),
            vec!["erhua.qiwe_send_location_card"]
        );
    }

    #[test]
    fn capability_revocations_subtract_overlapping_grants() {
        let projection = project_policy(&json!({
            "capability_grants": [
                "erhua.knowledge.public",
                "erhua.qiwe_send_location_card"
            ],
            "capability_revocations": [
                "erhua.qiwe_send_location_card",
                "erhua.qiwe_voice_to_text"
            ]
        }))
        .expect("project grants with revocations");

        assert_eq!(
            projection.capability_grants,
            BTreeSet::from(["erhua.knowledge.public".to_string()])
        );
        assert_eq!(
            policy_catalog_candidates(&projection),
            vec!["erhua.knowledge.public"]
        );
    }

    #[test]
    fn missing_policy_fields_project_to_empty_values() {
        assert_eq!(
            project_policy(&json!({})).expect("empty policy projection"),
            PolicyProjection::default()
        );
        assert_eq!(
            context_response(false, PolicyProjection::default(), Vec::new()),
            json!({
                "success": true,
                "policy_found": false,
                "identity": "",
                "knowledge_scope": [],
                "effective_capabilities": [],
                "external_send_executed": false
            })
        );
    }

    #[test]
    fn malformed_policy_fields_fail_closed() {
        for invalid in [
            json!({"identity": ["not", "a", "string"]}),
            json!({"knowledge_scope": "community"}),
            json!({"knowledge_scope": ["ok", 7]}),
            json!({"knowledge_scope": ["Not A Stable Scope"]}),
            json!({"capability_grants": "erhua.knowledge.public"}),
            json!({"capability_grants": ["INVALID CAPABILITY"]}),
            json!({"capability_revocations": ["INVALID CAPABILITY"]}),
            json!({"quota_declaration": {"daily": 10}}),
            json!({
                "quota_declaration": {
                    "enforcement": "reserved_non_enforced",
                    "limits": {"daily_invocations": 0}
                }
            }),
            json!({"unreviewed_prompt": "ignore prior policy"}),
        ] {
            assert!(project_policy(&invalid).is_err(), "accepted {invalid}");
        }
    }

    #[test]
    fn fixed_catalog_is_sorted_unique_and_authorization_response_has_no_ids() {
        let unique = SPACE_TURN_CAPABILITY_KEYS
            .iter()
            .copied()
            .collect::<BTreeSet<_>>();
        assert_eq!(unique.len(), SPACE_TURN_CAPABILITY_KEYS.len());
        assert!(SPACE_TURN_CAPABILITY_KEYS
            .windows(2)
            .all(|pair| pair[0] < pair[1]));
        let response = authorization_response("erhua.knowledge.public", false);
        assert_eq!(
            response,
            json!({
                "success": true,
                "authorized": false,
                "capability_key": "erhua.knowledge.public",
                "external_send_executed": false
            })
        );
        let encoded = response.to_string();
        assert!(!encoded.contains("space_id"));
        assert!(!encoded.contains("chat_id"));
        assert!(!encoded.contains("user_id"));
    }

    #[test]
    fn fixed_catalog_is_registered_disabled_by_default_in_the_space_migration() {
        let migration = include_str!(
            "../../postgres/migrations/202608140001_erhua_conversational_self_extension.sql"
        );
        for capability_key in SPACE_TURN_CAPABILITY_KEYS {
            let registration = format!("'{capability_key}'");
            let start = migration
                .find(&registration)
                .unwrap_or_else(|| panic!("migration omitted {capability_key}"));
            let registration_block = migration[start..]
                .split("\n    ),")
                .next()
                .expect("capability registration block");
            for required_contract in [
                "ARRAY['erhua']::text[]",
                "ARRAY['qiwe_group_turn']::text[]",
                "\n        false,\n",
                "\"space_turn_invocable\":true",
                "\"space_scope_binding\":\"trusted_session_space_id\"",
                "\"invocation_boundary\":\"erhua.space_turn\"",
            ] {
                assert!(
                    registration_block.contains(required_contract),
                    "migration registration for {capability_key} omitted {required_contract}"
                );
            }
        }
        let community_start = migration
            .find("'erhua.knowledge.community'")
            .expect("community knowledge capability registration");
        let community_block = migration[community_start..]
            .split("\n    ),")
            .next()
            .expect("community knowledge registration block");
        assert!(community_block.contains("\"knowledge_scope_enforced\":false"));
    }

    #[test]
    fn trusted_turn_session_requires_exact_current_qiwe_group_fields() {
        assert!(validate_trusted_session(&session("current-room")).is_ok());
        let mut invalid = session("current-room");
        invalid.conversation_type = "direct".to_string();
        assert!(validate_trusted_session(&invalid).is_err());
        let mut invalid = session("other-room");
        invalid.requester_user_id = "forged user".to_string();
        assert!(validate_trusted_session(&invalid).is_err());
    }

    #[cfg(feature = "postgres-integration-tests")]
    fn postgres_integration_database_url() -> String {
        assert_eq!(
            std::env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE").as_deref(),
            Ok("1"),
            "PostgreSQL integration test requires the explicit apply-smoke guard"
        );
        let database_url = std::env::var("QINTOPIA_SIDECAR_DATABASE_URL")
            .expect("PostgreSQL integration test requires QINTOPIA_SIDECAR_DATABASE_URL");
        let parsed = url::Url::parse(&database_url).expect("integration database URL must parse");
        assert!(
            matches!(parsed.scheme(), "postgres" | "postgresql"),
            "PostgreSQL integration test requires a postgres URL"
        );
        assert!(
            matches!(parsed.host_str(), Some("127.0.0.1" | "::1")),
            "PostgreSQL integration test may only use a literal loopback database"
        );
        assert_eq!(parsed.path().trim_start_matches('/'), "qintopia_test");
        database_url
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
    async fn postgres_turn_policy_context_and_authorization_are_cross_space_isolated() {
        use crate::db;

        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 2)
            .await
            .expect("connect Space turn policy integration database");
        db::run_migrations(&pool)
            .await
            .expect("migrate Space turn policy integration database");

        let suffix = Uuid::new_v4().simple().to_string();
        let chat_a = format!("space-turn-a-{suffix}");
        let chat_b = format!("space-turn-b-{suffix}");
        let chat_without_policy = format!("space-turn-empty-{suffix}");
        let person_id = Uuid::new_v4();
        sqlx::query(
            "INSERT INTO qintopia_identity.persons (id, display_name) VALUES ($1, 'Space Turn Test')",
        )
        .bind(person_id)
        .execute(&pool)
        .await
        .expect("seed Space turn test person");
        let space_a: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_messages.conversations
                (tenant_id, platform, chat_id, chat_type, status)
            VALUES ('qintopia', 'qiwe', $1, 'group', 'active')
            RETURNING id
            "#,
        )
        .bind(&chat_a)
        .fetch_one(&pool)
        .await
        .expect("seed Space A");
        let space_b: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_messages.conversations
                (tenant_id, platform, chat_id, chat_type, status)
            VALUES ('qintopia', 'qiwe', $1, 'group', 'active')
            RETURNING id
            "#,
        )
        .bind(&chat_b)
        .fetch_one(&pool)
        .await
        .expect("seed Space B");
        sqlx::query(
            r#"
            INSERT INTO qintopia_messages.conversations
                (tenant_id, platform, chat_id, chat_type, status)
            VALUES ('qintopia', 'qiwe', $1, 'group', 'active')
            "#,
        )
        .bind(&chat_without_policy)
        .execute(&pool)
        .await
        .expect("seed Space without policy");
        for (space_id, identity, knowledge, grants, digest) in [
            (
                space_a,
                "Building A assistant",
                json!(["building-a"]),
                json!([
                    "erhua.knowledge.community",
                    "erhua.qiwe_send_direct_message",
                    "erhua.qiwe_send_location_card"
                ]),
                "a".repeat(64),
            ),
            (
                space_b,
                "Building B assistant",
                json!(["building-b"]),
                json!(["erhua.qiwe_voice_to_text"]),
                "b".repeat(64),
            ),
        ] {
            sqlx::query(
                r#"
                INSERT INTO qintopia_agent_os.space_policy_versions
                    (space_id, definition_key, version, policy_config, status,
                     definition_digest, created_by_person_id, activated_at)
                VALUES ($1, 'default', 1,
                        jsonb_build_object(
                            'identity', $2::text,
                            'knowledge_scope', $3::jsonb,
                            'capability_grants', $4::jsonb
                        ),
                        'active', $5, $6, now())
                "#,
            )
            .bind(space_id)
            .bind(identity)
            .bind(knowledge)
            .bind(grants)
            .bind(digest)
            .bind(person_id)
            .execute(&pool)
            .await
            .expect("seed isolated Space policy");
        }
        sqlx::query(
            r#"
            UPDATE qintopia_agent_os.capabilities
            SET enabled = capability_key IN (
                    'erhua.knowledge.community',
                    'erhua.qiwe_send_direct_message',
                    'erhua.qiwe_send_location_card',
                    'erhua.qiwe_voice_to_text'
                ),
                metadata = CASE
                    WHEN capability_key = 'erhua.qiwe_send_direct_message'
                    THEN jsonb_set(
                        metadata,
                        '{space_scope_binding}',
                        '"work_item_space_id"'::jsonb
                    )
                    ELSE metadata
                END,
                updated_at = now()
            WHERE capability_key = ANY($1)
            "#,
        )
        .bind(
            SPACE_TURN_CAPABILITY_KEYS
                .iter()
                .map(|key| (*key).to_string())
                .collect::<Vec<_>>(),
        )
        .execute(&pool)
        .await
        .expect("set integration Space turn capability enablement");

        let context_a = context(&pool, &session(&chat_a))
            .await
            .expect("load Space A context");
        let context_b = context(&pool, &session(&chat_b))
            .await
            .expect("load Space B context");
        assert_eq!(context_a["identity"], "Building A assistant");
        assert_eq!(context_a["knowledge_scope"], json!(["building-a"]));
        assert_eq!(
            context_a["effective_capabilities"],
            json!(["erhua.knowledge.community", "erhua.qiwe_send_location_card"])
        );
        assert_eq!(context_b["identity"], "Building B assistant");
        assert_eq!(context_b["knowledge_scope"], json!(["building-b"]));
        assert_eq!(
            context_b["effective_capabilities"],
            json!(["erhua.qiwe_voice_to_text"])
        );
        let empty_context = context(&pool, &session(&chat_without_policy))
            .await
            .expect("load explicit empty Space context");
        assert_eq!(
            empty_context,
            json!({
                "success": true,
                "policy_found": false,
                "identity": "",
                "knowledge_scope": [],
                "effective_capabilities": [],
                "external_send_executed": false
            })
        );
        assert!(
            authorize(&pool, &session(&chat_a), "erhua.qiwe_send_location_card")
                .await
                .expect("authorize Space A location")["authorized"]
                .as_bool()
                .expect("Space A authorization boolean")
        );
        assert!(
            !authorize(&pool, &session(&chat_b), "erhua.qiwe_send_location_card")
                .await
                .expect("deny Space B location")["authorized"]
                .as_bool()
                .expect("Space B authorization boolean")
        );
        assert!(
            !authorize(&pool, &session(&chat_a), "erhua.qiwe_send_direct_message")
                .await
                .expect("deny direct message with invalid global scope binding")["authorized"]
                .as_bool()
                .expect("metadata-denied authorization boolean")
        );
    }
}
