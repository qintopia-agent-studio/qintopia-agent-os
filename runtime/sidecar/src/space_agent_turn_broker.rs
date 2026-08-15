use std::{
    collections::{BTreeMap, BTreeSet},
    fs,
    os::unix::fs::{FileTypeExt, MetadataExt, PermissionsExt},
    path::{Path, PathBuf},
    time::Duration,
};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use serde::{de::Error as _, Deserialize, Deserializer, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Postgres, Row, Transaction};
use tokio::{
    io::{AsyncBufReadExt, AsyncReadExt, AsyncWriteExt, BufReader},
    net::{UnixListener, UnixStream},
    time::timeout,
};
use uuid::Uuid;
use zeroize::Zeroize;

use crate::{
    config::Cli,
    db, space_agent_turn,
    strict_json::{parse_strict_bounded_slice, JsonLimits},
};

const ENABLE_ENV: &str = "QINTOPIA_SPACE_AGENT_TURN_RUNNER_ENABLED";
const APPROVAL_ENV: &str = "QINTOPIA_SPACE_AGENT_TURN_RUNNER_APPROVAL";
const APPROVAL_PHRASE: &str = "approved-production-space-agent-turn-runner";
const DATABASE_URL_SHA256_ENV: &str = "QINTOPIA_SPACE_AGENT_TURN_RUNNER_DATABASE_URL_SHA256";
const RUNNER_TOKEN_SHA256_ENV: &str = "QINTOPIA_SPACE_AGENT_TURN_RUNNER_TOKEN_SHA256";
const RUNNER_UID_ENV: &str = "QINTOPIA_SPACE_AGENT_TURN_RUNNER_UID";
const RUNNER_GID_ENV: &str = "QINTOPIA_SPACE_AGENT_TURN_RUNNER_GID";
const CLAIMED_BY: &str = space_agent_turn::RUNNER_IDENTITY;
const WORK_ITEM_TYPE: &str = "space_agent_turn";
const CAPABILITY_KEY: &str = "erhua.space_agent_turn";
const PROTOCOL_VERSION: u8 = space_agent_turn::RUNNER_CONTRACT_VERSION;
const CLAIM_TTL_MINUTES: i64 = 15;
const MAX_INVALID_CANDIDATES: usize = 16;
const EXPIRED_CLAIM_BATCH_SIZE: usize = 32;
const MAX_RECONCILIATION_BATCHES: usize = 32;
const RECONCILIATION_INTERVAL: Duration = Duration::from_secs(30);
const MAX_MESSAGE_BYTES: u64 = 128 * 1024;
const READ_TIMEOUT: Duration = Duration::from_secs(2);
const HANDLE_TIMEOUT: Duration = Duration::from_secs(10);
const WRITE_TIMEOUT: Duration = Duration::from_secs(1);
const MAX_CAPABILITY_USAGE: usize = 32;
const MAX_CAPABILITY_CALLS: u16 = 64;
const CAPABILITY_CALL_WORK_ITEM_TYPE: &str = "space_agent_turn_capability_call";
#[cfg(test)]
const SUBJECT_IDENTITY_CAPABILITY_KEY: &str = "erhua.space_subject_identity_lookup";
const SUBJECT_IDENTITY_RECIPE: &str = "trigger_subject_identity_lookup_v1";
const MAX_MEMBER_DISPLAY_NAME_CHARS: usize = 200;
const CURRENT_ROSTER_MAX_AGE_HOURS: i32 = 24;
const BROKER_JSON_LIMITS: JsonLimits = JsonLimits {
    max_bytes: MAX_MESSAGE_BYTES as usize,
    max_depth: 16,
    max_nodes: 5_000,
    max_string_bytes: 64 * 1024,
    max_key_bytes: 128,
};

#[derive(Clone)]
struct BrokerConfig {
    runner_token_sha256: String,
    runner_uid: u32,
    runner_gid: u32,
}

impl BrokerConfig {
    fn from_env(database_url: &str) -> Result<Self> {
        let enable = std::env::var(ENABLE_ENV).ok();
        let approval = std::env::var(APPROVAL_ENV).ok();
        let database_url_sha256 = std::env::var(DATABASE_URL_SHA256_ENV).ok();
        let runner_token_sha256 = std::env::var(RUNNER_TOKEN_SHA256_ENV).ok();
        let runner_uid = std::env::var(RUNNER_UID_ENV).ok();
        let runner_gid = std::env::var(RUNNER_GID_ENV).ok();
        Self::from_values(
            database_url,
            enable.as_deref(),
            approval.as_deref(),
            database_url_sha256.as_deref(),
            runner_token_sha256.as_deref(),
            runner_uid.as_deref(),
            runner_gid.as_deref(),
        )
    }

    fn from_values(
        database_url: &str,
        enable: Option<&str>,
        approval: Option<&str>,
        database_url_sha256: Option<&str>,
        runner_token_sha256: Option<&str>,
        runner_uid: Option<&str>,
        runner_gid: Option<&str>,
    ) -> Result<Self> {
        match enable {
            Some("1") => {}
            None | Some("") | Some("0") => bail!("Space agent-turn runner is disabled"),
            _ => bail!("{ENABLE_ENV} must be unset, 0, or 1"),
        }
        if approval != Some(APPROVAL_PHRASE) {
            bail!("Space agent-turn runner owner approval is required");
        }
        let expected_database = validated_sha256(
            database_url_sha256,
            "Space agent-turn runner database URL hash",
        )?;
        if !constant_time_eq(
            expected_database.as_bytes(),
            sha256_hex(database_url.as_bytes()).as_bytes(),
        ) {
            bail!("Space agent-turn runner database URL hash is not owner-approved");
        }
        Ok(Self {
            runner_token_sha256: validated_sha256(
                runner_token_sha256,
                "Space agent-turn runner token hash",
            )?,
            runner_uid: validated_os_id(runner_uid, "Space agent-turn runner uid")?,
            runner_gid: validated_os_id(runner_gid, "Space agent-turn runner gid")?,
        })
    }

    fn authenticate(&self, runner_identity: &str, runner_token: &SensitiveString) -> Result<()> {
        if runner_identity != space_agent_turn::RUNNER_IDENTITY {
            bail!("Space agent-turn runner identity is not registered");
        }
        validate_secret_shape(runner_token.expose())?;
        if !constant_time_eq(
            self.runner_token_sha256.as_bytes(),
            sha256_hex(runner_token.expose().as_bytes()).as_bytes(),
        ) {
            bail!("Space agent-turn runner authentication failed");
        }
        Ok(())
    }
}

struct SensitiveString(String);

impl SensitiveString {
    fn expose(&self) -> &str {
        &self.0
    }
}

impl Drop for SensitiveString {
    fn drop(&mut self) {
        self.0.zeroize();
    }
}

impl<'de> Deserialize<'de> for SensitiveString {
    fn deserialize<D>(deserializer: D) -> std::result::Result<Self, D::Error>
    where
        D: Deserializer<'de>,
    {
        let value = String::deserialize(deserializer)?;
        if value.is_empty() {
            return Err(D::Error::custom("runner token is empty"));
        }
        Ok(Self(value))
    }
}

#[derive(Deserialize)]
#[serde(tag = "operation", deny_unknown_fields)]
enum BrokerRequest {
    #[serde(rename = "space_agent_turn_claim")]
    Claim {
        schema_version: u8,
        runner_identity: String,
        runner_token: SensitiveString,
    },
    #[serde(rename = "space_agent_turn_finish")]
    Finish {
        schema_version: u8,
        runner_identity: String,
        runner_token: SensitiveString,
        work_item_id: Uuid,
        claim_token: SensitiveString,
        result: RunnerResult,
    },
    #[serde(rename = "space_agent_turn_invoke")]
    Invoke {
        schema_version: u8,
        runner_identity: String,
        runner_token: SensitiveString,
        work_item_id: Uuid,
        claim_token: SensitiveString,
        call_id: Uuid,
        capability_key: String,
        input: Value,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(tag = "outcome", rename_all = "snake_case", deny_unknown_fields)]
enum RunnerResult {
    Succeeded {
        output: Value,
        #[serde(default)]
        capability_usage: Vec<CapabilityUsage>,
    },
    Failed {
        failure_code: String,
    },
}

#[derive(Debug, Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct CapabilityUsage {
    capability_key: String,
    call_count: u16,
}

#[derive(Debug, Clone, Serialize)]
struct CapabilityDescriptor {
    capability_key: String,
    input_schema: Value,
    output_schema: Value,
    risk_level: String,
    review_policy: String,
    #[serde(skip_serializing)]
    execution_recipe: String,
}

#[derive(Debug, Serialize)]
struct ClaimEnvelope {
    schema_version: u8,
    claimed: bool,
    runner_identity: &'static str,
    work_item_id: Uuid,
    claim_token: String,
    claim_expires_at: DateTime<Utc>,
    goal: String,
    trigger: Value,
    output_contract: Value,
    output_contract_sha256: String,
    capabilities: Vec<CapabilityDescriptor>,
}

#[derive(Debug)]
struct LiveTurn {
    work_item_id: Uuid,
    parent_work_item_id: Uuid,
    space_id: Uuid,
    conversation_chat_id: String,
    goal: String,
    trigger: Value,
    output_contract: Value,
    output_contract_sha256: String,
    capabilities: Vec<CapabilityDescriptor>,
}

struct SocketGuard(PathBuf);

impl Drop for SocketGuard {
    fn drop(&mut self) {
        if path_is_socket(&self.0) {
            let _ = fs::remove_file(&self.0);
        }
    }
}

pub(crate) async fn run(cli: &Cli, socket_path: PathBuf) -> Result<()> {
    let database_url = cli.database_url_required()?;
    let config = BrokerConfig::from_env(database_url)?;
    let socket_owner_uid = prepare_socket(&socket_path, config.runner_uid, config.runner_gid)?;
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    db::run_migrations(&pool).await?;
    let listener = UnixListener::bind(&socket_path)
        .with_context(|| format!("bind Space agent-turn broker {}", socket_path.display()))?;
    fs::set_permissions(&socket_path, fs::Permissions::from_mode(0o660))
        .context("set Space agent-turn broker socket permissions")?;
    validate_bound_socket(&socket_path, socket_owner_uid, config.runner_gid)?;
    let _guard = SocketGuard(socket_path.clone());
    tracing::info!(
        socket_path = %socket_path.display(),
        runner_identity = space_agent_turn::RUNNER_IDENTITY,
        "Space agent-turn broker started"
    );

    tokio::try_join!(
        serve_connections(listener, pool.clone(), config),
        reconcile_expired_claims_loop(pool)
    )?;
    Ok(())
}

async fn serve_connections(
    listener: UnixListener,
    pool: PgPool,
    config: BrokerConfig,
) -> Result<()> {
    loop {
        let (stream, _) = listener
            .accept()
            .await
            .context("accept Space agent-turn broker connection")?;
        let pool = pool.clone();
        let config = config.clone();
        tokio::spawn(async move {
            if handle_connection(stream, &pool, &config).await.is_err() {
                tracing::warn!(
                    error_code = "space_agent_turn_broker_connection_failed",
                    "Space agent-turn broker request failed"
                );
            }
        });
    }
}

async fn reconcile_expired_claims_loop(pool: PgPool) -> Result<()> {
    loop {
        reconcile_all_expired_claims(&pool).await?;
        tokio::time::sleep(RECONCILIATION_INTERVAL).await;
    }
}

async fn reconcile_all_expired_claims(pool: &PgPool) -> Result<usize> {
    let mut total = 0;
    for _ in 0..MAX_RECONCILIATION_BATCHES {
        let mut tx = pool
            .begin()
            .await
            .context("begin Space agent-turn expiry reconciliation")?;
        let terminalized = terminalize_expired_claims(&mut tx).await?;
        tx.commit()
            .await
            .context("commit Space agent-turn expiry reconciliation")?;
        total += terminalized;
        if terminalized < EXPIRED_CLAIM_BATCH_SIZE {
            break;
        }
    }
    Ok(total)
}

async fn handle_connection(
    mut stream: UnixStream,
    pool: &PgPool,
    config: &BrokerConfig,
) -> Result<()> {
    validate_peer(&stream, config.runner_uid, config.runner_gid)?;
    let request = match read_request(&mut stream).await {
        Ok(request) => request,
        Err(_) => {
            return write_response(
                &mut stream,
                &json!({
                    "success": false,
                    "accepted": false,
                    "error": "invalid_request",
                    "external_send_executed": false
                }),
            )
            .await;
        }
    };
    let response = match timeout(HANDLE_TIMEOUT, handle(pool, config, request)).await {
        Ok(Ok(value)) => value,
        Ok(Err(_)) => json!({
            "success": false,
            "accepted": false,
            "error": "space_agent_turn_broker_rejected",
            "external_send_executed": false
        }),
        Err(_) => json!({
            "success": false,
            "accepted": false,
            "error": "space_agent_turn_broker_timeout",
            "external_send_executed": false
        }),
    };
    write_response(&mut stream, &response).await
}

async fn handle(pool: &PgPool, config: &BrokerConfig, request: BrokerRequest) -> Result<Value> {
    match request {
        BrokerRequest::Claim {
            schema_version,
            runner_identity,
            runner_token,
        } => {
            validate_protocol(schema_version)?;
            config.authenticate(&runner_identity, &runner_token)?;
            claim(pool).await
        }
        BrokerRequest::Finish {
            schema_version,
            runner_identity,
            runner_token,
            work_item_id,
            claim_token,
            result,
        } => {
            validate_protocol(schema_version)?;
            config.authenticate(&runner_identity, &runner_token)?;
            finish(pool, work_item_id, claim_token.expose(), result).await
        }
        BrokerRequest::Invoke {
            schema_version,
            runner_identity,
            runner_token,
            work_item_id,
            claim_token,
            call_id,
            capability_key,
            input,
        } => {
            validate_protocol(schema_version)?;
            config.authenticate(&runner_identity, &runner_token)?;
            invoke(
                pool,
                work_item_id,
                claim_token.expose(),
                call_id,
                &capability_key,
                input,
            )
            .await
        }
    }
}

async fn claim(pool: &PgPool) -> Result<Value> {
    let mut tx = pool.begin().await.context("begin Space agent-turn claim")?;
    terminalize_expired_claims(&mut tx).await?;
    for _ in 0..MAX_INVALID_CANDIDATES {
        let candidate = sqlx::query(
            r#"
            SELECT id
            FROM qintopia_agent_os.work_items
            WHERE work_item_type = $1
              AND capability_key = $2
              AND requester_agent = 'system'
              AND target_agent = 'erhua'
              AND status = 'queued'
              AND attempts = 0
              AND available_at <= now()
              AND space_id IS NOT NULL
              AND claimed_by IS NULL
              AND locked_at IS NULL
              AND claim_expires_at IS NULL
              AND metadata @> $3::jsonb
            ORDER BY available_at, created_at, id
            FOR UPDATE SKIP LOCKED
            LIMIT 1
            "#,
        )
        .bind(WORK_ITEM_TYPE)
        .bind(CAPABILITY_KEY)
        .bind(required_metadata())
        .fetch_optional(&mut *tx)
        .await
        .context("select queued Space agent-turn")?;
        let Some(candidate) = candidate else {
            tx.commit().await.context("commit empty agent-turn claim")?;
            return Ok(json!({
                "schema_version": PROTOCOL_VERSION,
                "claimed": false,
                "runner_identity": space_agent_turn::RUNNER_IDENTITY
            }));
        };
        let work_item_id: Uuid = candidate.try_get("id")?;
        let turn = match load_live_turn(&mut tx, work_item_id, "queued").await {
            Ok(turn) => turn,
            Err(_) => {
                terminalize_invalid_candidate(&mut tx, work_item_id).await?;
                continue;
            }
        };
        let claim_token = new_claim_token();
        let claim_expires_at: DateTime<Utc> = sqlx::query_scalar(
            r#"
            UPDATE qintopia_agent_os.work_items
            SET status = 'processing', claimed_by = $2, locked_at = now(),
                claim_expires_at = now() + make_interval(mins => $3),
                attempts = attempts + 1,
                metadata = jsonb_set(
                    metadata,
                    '{space_agent_turn_claim}',
                    jsonb_build_object(
                        'schema_version', $4::integer,
                        'runner_identity', $2::text,
                        'token_sha256', $5::text,
                        'output_contract_sha256', $6::text
                    ),
                    true
                ),
                updated_at = now()
            WHERE id = $1 AND status = 'queued' AND attempts = 0
              AND claimed_by IS NULL AND locked_at IS NULL AND claim_expires_at IS NULL
            RETURNING claim_expires_at
            "#,
        )
        .bind(work_item_id)
        .bind(CLAIMED_BY)
        .bind(CLAIM_TTL_MINUTES as i32)
        .bind(PROTOCOL_VERSION as i32)
        .bind(sha256_hex(claim_token.as_bytes()))
        .bind(&turn.output_contract_sha256)
        .fetch_optional(&mut *tx)
        .await
        .context("claim Space agent-turn work item")?
        .context("Space agent-turn claim changed concurrently")?;
        append_event(
            &mut tx,
            work_item_id,
            "space_agent_turn_claimed",
            "Bounded runner claimed one Space agent turn",
            json!({
                "runner_identity": space_agent_turn::RUNNER_IDENTITY,
                "lease_minutes": CLAIM_TTL_MINUTES,
                "capability_count": turn.capabilities.len(),
                "output_contract_sha256": turn.output_contract_sha256,
                "external_send_executed": false,
                "automatic_retry_allowed": false
            }),
        )
        .await?;
        tx.commit().await.context("commit Space agent-turn claim")?;
        return serde_json::to_value(ClaimEnvelope {
            schema_version: PROTOCOL_VERSION,
            claimed: true,
            runner_identity: space_agent_turn::RUNNER_IDENTITY,
            work_item_id: turn.work_item_id,
            claim_token,
            claim_expires_at,
            goal: turn.goal,
            trigger: turn.trigger,
            output_contract: turn.output_contract,
            output_contract_sha256: turn.output_contract_sha256,
            capabilities: turn.capabilities,
        })
        .context("serialize Space agent-turn claim");
    }
    tx.commit()
        .await
        .context("commit rejected Space agent-turn candidates")?;
    Ok(json!({
        "schema_version": PROTOCOL_VERSION,
        "claimed": false,
        "runner_identity": space_agent_turn::RUNNER_IDENTITY
    }))
}

async fn invoke(
    pool: &PgPool,
    work_item_id: Uuid,
    claim_token: &str,
    call_id: Uuid,
    capability_key: &str,
    input: Value,
) -> Result<Value> {
    validate_secret_shape(claim_token)?;
    let mut tx = pool
        .begin()
        .await
        .context("begin Space agent-turn capability invocation")?;
    let claim = sqlx::query(
        r#"
        SELECT parent_work_item_id,
               metadata #>> '{space_agent_turn_claim,token_sha256}' AS token_sha256,
               claim_expires_at <= now() AS claim_expired
        FROM qintopia_agent_os.work_items
        WHERE id = $1
          AND work_item_type = $2
          AND capability_key = $3
          AND status = 'processing'
          AND attempts = 1
          AND claimed_by = $4
          AND claim_expires_at IS NOT NULL
          AND metadata @> $5::jsonb
        FOR UPDATE
        "#,
    )
    .bind(work_item_id)
    .bind(WORK_ITEM_TYPE)
    .bind(CAPABILITY_KEY)
    .bind(CLAIMED_BY)
    .bind(required_metadata())
    .fetch_optional(&mut *tx)
    .await
    .context("lock Space agent-turn capability claim")?
    .context("Space agent-turn capability claim is missing")?;
    let stored_token_sha256: String = claim.try_get("token_sha256")?;
    if !constant_time_eq(
        stored_token_sha256.as_bytes(),
        sha256_hex(claim_token.as_bytes()).as_bytes(),
    ) {
        bail!("Space agent-turn capability claim token does not match");
    }
    if claim.try_get::<bool, _>("claim_expired")? {
        terminalize_exact_expired_claim(
            &mut tx,
            work_item_id,
            claim.try_get("parent_work_item_id")?,
            &stored_token_sha256,
        )
        .await?;
        tx.commit()
            .await
            .context("commit expired Space agent-turn capability invocation")?;
        bail!("Space agent-turn capability claim expired");
    }

    let turn = load_live_turn(&mut tx, work_item_id, "processing").await?;
    let capability = select_live_capability(&turn, capability_key)?;
    space_agent_turn::validate_output(&capability.input_schema, &input)
        .context("Space agent-turn capability input does not match its contract")?;

    let idempotency_key = format!("space-agent-turn-call:{work_item_id}:{call_id}");
    if let Some(output) = load_existing_capability_call(
        &mut tx,
        work_item_id,
        &idempotency_key,
        call_id,
        capability_key,
        &input,
    )
    .await?
    {
        let output_sha256 =
            sha256_hex(&serde_json::to_vec(&output).context("encode replayed capability output")?);
        tx.commit()
            .await
            .context("commit replayed Space agent-turn capability invocation")?;
        return Ok(capability_response(
            call_id,
            capability_key,
            output,
            output_sha256,
            true,
        ));
    }

    let call_count: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM qintopia_agent_os.work_items
        WHERE parent_work_item_id = $1
          AND work_item_type = $2
          AND source_type = 'space_agent_turn_broker'
        "#,
    )
    .bind(work_item_id)
    .bind(CAPABILITY_CALL_WORK_ITEM_TYPE)
    .fetch_one(&mut *tx)
    .await
    .context("count Space agent-turn capability calls")?;
    if call_count >= i64::from(MAX_CAPABILITY_CALLS) {
        bail!("Space agent-turn capability call limit was reached");
    }

    let output = execute_capability(&mut tx, &turn, &capability, &input).await?;
    space_agent_turn::validate_output(&capability.output_schema, &output)
        .context("Space agent-turn capability output does not match its contract")?;
    let input_sha256 = sha256_hex(
        &serde_json::to_vec(&input).context("encode Space agent-turn capability input")?,
    );
    let output_sha256 = sha256_hex(
        &serde_json::to_vec(&output).context("encode Space agent-turn capability output")?,
    );
    let call_work_item_id = Uuid::new_v4();
    let inserted = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_items
            (id, parent_work_item_id, space_id, work_item_type, status,
             requester_agent, target_agent, capability_key, human_owner, priority,
             available_at, brief_summary, purpose, source_type, source_refs,
             dedupe_key, idempotency_key, risk_level, information_class, payload,
             payload_redaction_policy, review_policy, attempts, metadata)
        VALUES
            ($1, $2, $3, $4, 'completed',
             $5, 'erhua', $6, '', 'normal', now(),
             'Bounded Space agent-turn capability call',
             'space_agent_turn_capability_call', 'space_agent_turn_broker',
             jsonb_build_object('space_agent_turn_work_item_id', $2::uuid,
                                'call_id', $7::uuid),
             $8, $8, $9, 'internal_ops',
             jsonb_build_object('schema_version', $10::integer,
                                'call_id', $7::uuid,
                                'capability_key', $6::text,
                                'input', $11::jsonb,
                                'output', $12::jsonb),
             'summary_only', $13, 1,
             jsonb_build_object('space_bound', true,
                                'definition_bound', true,
                                'runner_identity', $5::text,
                                'execution_recipe', $14::text,
                                'input_sha256', $15::text,
                                'output_sha256', $16::text,
                                'external_send_executed', false,
                                'automatic_retry_allowed', false))
        ON CONFLICT (idempotency_key) DO NOTHING
        "#,
    )
    .bind(call_work_item_id)
    .bind(work_item_id)
    .bind(turn.space_id)
    .bind(CAPABILITY_CALL_WORK_ITEM_TYPE)
    .bind(CLAIMED_BY)
    .bind(capability_key)
    .bind(call_id)
    .bind(&idempotency_key)
    .bind(&capability.risk_level)
    .bind(PROTOCOL_VERSION as i32)
    .bind(&input)
    .bind(&output)
    .bind(&capability.review_policy)
    .bind(&capability.execution_recipe)
    .bind(&input_sha256)
    .bind(&output_sha256)
    .execute(&mut *tx)
    .await
    .context("persist Space agent-turn capability receipt")?;
    if inserted.rows_affected() != 1 {
        bail!("Space agent-turn capability receipt changed concurrently");
    }
    append_event(
        &mut tx,
        work_item_id,
        "space_agent_turn_capability_completed",
        "Bounded Space agent-turn capability completed",
        json!({
            "call_work_item_id": call_work_item_id,
            "call_id": call_id,
            "capability_key": capability_key,
            "input_sha256": input_sha256,
            "output_sha256": output_sha256,
            "execution_recipe": capability.execution_recipe,
            "external_send_executed": false,
            "automatic_retry_allowed": false
        }),
    )
    .await?;
    tx.commit()
        .await
        .context("commit Space agent-turn capability invocation")?;
    Ok(capability_response(
        call_id,
        capability_key,
        output,
        output_sha256,
        false,
    ))
}

fn select_live_capability(turn: &LiveTurn, capability_key: &str) -> Result<CapabilityDescriptor> {
    turn.capabilities
        .iter()
        .find(|candidate| candidate.capability_key == capability_key)
        .cloned()
        .context("Space agent-turn capability is outside the live catalog")
}

async fn load_existing_capability_call(
    tx: &mut Transaction<'_, Postgres>,
    parent_work_item_id: Uuid,
    idempotency_key: &str,
    call_id: Uuid,
    capability_key: &str,
    input: &Value,
) -> Result<Option<Value>> {
    let row = sqlx::query(
        r#"
        SELECT capability_key, source_refs, payload, metadata
        FROM qintopia_agent_os.work_items
        WHERE idempotency_key = $1
        FOR SHARE
        "#,
    )
    .bind(idempotency_key)
    .fetch_optional(&mut **tx)
    .await
    .context("load existing Space agent-turn capability receipt")?;
    let Some(row) = row else {
        return Ok(None);
    };
    let source_refs: Value = row.try_get("source_refs")?;
    let payload: Value = row.try_get("payload")?;
    let metadata: Value = row.try_get("metadata")?;
    if row.try_get::<String, _>("capability_key")? != capability_key
        || source_refs.get("space_agent_turn_work_item_id")
            != Some(&Value::String(parent_work_item_id.to_string()))
        || source_refs.get("call_id") != Some(&Value::String(call_id.to_string()))
        || payload.get("schema_version") != Some(&json!(PROTOCOL_VERSION))
        || payload.get("call_id") != Some(&Value::String(call_id.to_string()))
        || payload.get("capability_key") != Some(&Value::String(capability_key.to_string()))
        || payload.get("input") != Some(input)
        || metadata.get("runner_identity") != Some(&Value::String(CLAIMED_BY.to_string()))
        || metadata.get("external_send_executed") != Some(&Value::Bool(false))
    {
        bail!("Space agent-turn capability replay does not match its receipt");
    }
    payload
        .get("output")
        .cloned()
        .context("Space agent-turn capability receipt output is missing")
        .map(Some)
}

async fn execute_capability(
    tx: &mut Transaction<'_, Postgres>,
    turn: &LiveTurn,
    capability: &CapabilityDescriptor,
    input: &Value,
) -> Result<Value> {
    match capability.execution_recipe.as_str() {
        SUBJECT_IDENTITY_RECIPE => execute_subject_identity_lookup(tx, turn, input).await,
        _ => bail!("Space agent-turn capability recipe is not registered"),
    }
}

async fn execute_subject_identity_lookup(
    tx: &mut Transaction<'_, Postgres>,
    turn: &LiveTurn,
    input: &Value,
) -> Result<Value> {
    if input.get("scope").and_then(Value::as_str) != Some("trigger_subjects") {
        bail!("Space agent-turn identity lookup scope is invalid");
    }
    let subject_ids = turn
        .trigger
        .get("subject_user_ids")
        .and_then(Value::as_array)
        .map(|values| {
            values
                .iter()
                .map(|value| {
                    value
                        .as_str()
                        .map(ToString::to_string)
                        .context("Space agent-turn trigger subject is invalid")
                })
                .collect::<Result<Vec<_>>>()
        })
        .transpose()?
        .unwrap_or_default();
    if subject_ids.is_empty() {
        return Ok(json!({"members": []}));
    }
    let rows = sqlx::query(
        r#"
        SELECT channel_user_id, display_name
        FROM qintopia_identity.channel_identities
        WHERE platform = 'qiwe'
          AND chat_id = $1
          AND channel_user_id = ANY($2)
          AND COALESCE(is_bot, false) = false
          AND metadata->>'current_qiwe_room_member' = 'true'
          AND updated_at >= now() - make_interval(hours => $3)
        "#,
    )
    .bind(&turn.conversation_chat_id)
    .bind(&subject_ids)
    .bind(CURRENT_ROSTER_MAX_AGE_HOURS)
    .fetch_all(&mut **tx)
    .await
    .context("resolve exact current-Space trigger subjects")?;
    let mut names = std::collections::BTreeMap::<String, String>::new();
    for row in rows {
        let user_id: String = row.try_get("channel_user_id")?;
        let display_name: Option<String> = row.try_get("display_name")?;
        let display_name = display_name.unwrap_or_default().trim().to_string();
        if !display_name.is_empty()
            && display_name.chars().count() <= MAX_MEMBER_DISPLAY_NAME_CHARS
            && !display_name.chars().any(char::is_control)
        {
            names.insert(user_id, display_name);
        }
    }
    Ok(json!({
        "members": subject_ids
            .into_iter()
            .map(|user_id| {
                let display_name = names.get(&user_id).cloned().unwrap_or_default();
                json!({
                    "user_id": user_id,
                    "display_name": display_name,
                    "resolved": !display_name.is_empty()
                })
            })
            .collect::<Vec<_>>()
    }))
}

fn capability_response(
    call_id: Uuid,
    capability_key: &str,
    output: Value,
    output_sha256: String,
    replayed: bool,
) -> Value {
    json!({
        "schema_version": PROTOCOL_VERSION,
        "accepted": true,
        "status": "completed",
        "call_id": call_id,
        "capability_key": capability_key,
        "output": output,
        "output_sha256": output_sha256,
        "replayed": replayed
    })
}

async fn finish(
    pool: &PgPool,
    work_item_id: Uuid,
    claim_token: &str,
    result: RunnerResult,
) -> Result<Value> {
    validate_secret_shape(claim_token)?;
    let mut tx = pool
        .begin()
        .await
        .context("begin Space agent-turn finish")?;
    let row = sqlx::query(
        r#"
        SELECT parent_work_item_id,
               metadata #>> '{space_agent_turn_claim,token_sha256}' AS token_sha256,
               claim_expires_at <= now() AS claim_expired
        FROM qintopia_agent_os.work_items
        WHERE id = $1
          AND work_item_type = $2
          AND capability_key = $3
          AND status = 'processing'
          AND attempts = 1
          AND claimed_by = $4
          AND claim_expires_at IS NOT NULL
          AND metadata @> $5::jsonb
        FOR UPDATE
        "#,
    )
    .bind(work_item_id)
    .bind(WORK_ITEM_TYPE)
    .bind(CAPABILITY_KEY)
    .bind(CLAIMED_BY)
    .bind(required_metadata())
    .fetch_optional(&mut *tx)
    .await
    .context("lock Space agent-turn claim")?
    .context("Space agent-turn claim is missing")?;
    let stored_token_sha256: String = row.try_get("token_sha256")?;
    if !constant_time_eq(
        stored_token_sha256.as_bytes(),
        sha256_hex(claim_token.as_bytes()).as_bytes(),
    ) {
        bail!("Space agent-turn claim token does not match");
    }
    let parent_work_item_id: Option<Uuid> = row.try_get("parent_work_item_id")?;
    if row.try_get::<bool, _>("claim_expired")? {
        terminalize_exact_expired_claim(
            &mut tx,
            work_item_id,
            parent_work_item_id,
            &stored_token_sha256,
        )
        .await?;
        tx.commit()
            .await
            .context("commit expired Space agent-turn finish")?;
        return Ok(expired_finish_response());
    }
    let parent_work_item_id =
        parent_work_item_id.context("Space agent-turn lost its parent binding")?;

    let turn = match load_live_turn(&mut tx, work_item_id, "processing").await {
        Ok(turn) => turn,
        Err(_) => {
            fail_claim(
                &mut tx,
                work_item_id,
                parent_work_item_id,
                "authorization_revoked_before_finish",
            )
            .await?;
            tx.commit()
                .await
                .context("commit revoked Space agent-turn finish")?;
            return Ok(finish_response("failed"));
        }
    };
    match result {
        RunnerResult::Failed { failure_code } => {
            validate_failure_code(&failure_code)?;
            fail_claim(
                &mut tx,
                work_item_id,
                turn.parent_work_item_id,
                &failure_code,
            )
            .await?;
            tx.commit()
                .await
                .context("commit failed Space agent-turn")?;
            Ok(finish_response("failed"))
        }
        RunnerResult::Succeeded {
            output,
            capability_usage,
        } => {
            let allowed = turn
                .capabilities
                .iter()
                .map(|capability| capability.capability_key.as_str())
                .collect::<BTreeSet<_>>();
            let persisted_capability_usage =
                load_persisted_capability_usage(&mut tx, &turn, &allowed).await?;
            let persisted_usage = capability_usage_map(&persisted_capability_usage, &allowed)?;
            let reported_usage_matches = capability_usage_map(&capability_usage, &allowed)
                .is_ok_and(|reported| reported == persisted_usage);
            if space_agent_turn::validate_output(&turn.output_contract, &output).is_err()
                || !reported_usage_matches
            {
                fail_claim(
                    &mut tx,
                    work_item_id,
                    turn.parent_work_item_id,
                    "runner_result_contract_invalid",
                )
                .await?;
                tx.commit()
                    .await
                    .context("commit invalid Space agent-turn result")?;
                return Ok(finish_response("failed"));
            }
            let output_bytes = serde_json::to_vec(&output).context("encode agent-turn result")?;
            let output_sha256 = sha256_hex(&output_bytes);
            let artifact_id = Uuid::new_v4();
            sqlx::query(
                r#"
                INSERT INTO qintopia_agent_os.artifacts
                    (id, work_item_id, artifact_type, review_status, created_by_agent,
                     title, summary, content_hash, source_ids, risk_labels,
                     information_class, metadata)
                VALUES
                    ($1, $2, 'space_agent_turn_result', 'not_required', $3,
                     'Space agent-turn result', 'Validated against the active business output contract.',
                     $4, '[]'::jsonb, ARRAY[]::text[], 'internal_ops', $5)
                "#,
            )
            .bind(artifact_id)
            .bind(work_item_id)
            .bind(CLAIMED_BY)
            .bind(&output_sha256)
            .bind(result_artifact_metadata(
                &output,
                &turn.output_contract_sha256,
                &persisted_capability_usage,
            ))
            .execute(&mut *tx)
            .await
            .context("persist Space agent-turn result artifact")?;
            let updated = sqlx::query(
                r#"
                UPDATE qintopia_agent_os.work_items
                SET status = 'completed', claimed_by = NULL, locked_at = NULL,
                    claim_expires_at = NULL, last_error = NULL,
                    metadata = (metadata - 'space_agent_turn_claim')
                        || jsonb_build_object(
                            'space_agent_turn_result',
                            jsonb_build_object(
                                'schema_version', $2::integer,
                                'artifact_id', $3::uuid,
                                'output_sha256', $4::text,
                                'output_contract_sha256', $5::text,
                                'runner_identity', $6::text,
                                'content_trust', 'untrusted_agent_output',
                                'execution_eligible', false,
                                'routing_authority', 'none'
                            )
                        ),
                    updated_at = now()
                WHERE id = $1 AND status = 'processing' AND attempts = 1
                  AND claimed_by = $6
                "#,
            )
            .bind(work_item_id)
            .bind(PROTOCOL_VERSION as i32)
            .bind(artifact_id)
            .bind(&output_sha256)
            .bind(&turn.output_contract_sha256)
            .bind(CLAIMED_BY)
            .execute(&mut *tx)
            .await
            .context("complete Space agent-turn work item")?;
            if updated.rows_affected() != 1 {
                bail!("Space agent-turn completion lost its exact claim");
            }
            append_event(
                &mut tx,
                work_item_id,
                "space_agent_turn_completed",
                "Bounded runner completed one Space agent turn",
                json!({
                    "artifact_id": artifact_id,
                    "output_sha256": output_sha256,
                    "output_contract_sha256": turn.output_contract_sha256,
                    "capability_usage_count": persisted_capability_usage.len(),
                    "runner_identity": space_agent_turn::RUNNER_IDENTITY,
                    "external_send_executed": Value::Null,
                    "automatic_retry_allowed": false
                }),
            )
            .await?;
            append_parent_event(
                &mut tx,
                turn.parent_work_item_id,
                "space_agent_turn_child_completed",
                "Space automation agent-turn child completed",
                json!({
                    "child_work_item_id": work_item_id,
                    "result_artifact_id": artifact_id,
                    "output_sha256": output_sha256,
                    "output_contract_sha256": turn.output_contract_sha256,
                    "external_send_executed": Value::Null,
                    "automatic_retry_allowed": false
                }),
            )
            .await?;
            tx.commit()
                .await
                .context("commit completed Space agent-turn")?;
            Ok(finish_response("completed"))
        }
    }
}

async fn load_persisted_capability_usage(
    tx: &mut Transaction<'_, Postgres>,
    turn: &LiveTurn,
    allowed: &BTreeSet<&str>,
) -> Result<Vec<CapabilityUsage>> {
    let idempotency_prefix = format!("space-agent-turn-call:{}:", turn.work_item_id);
    let rows = sqlx::query(
        r#"
        SELECT capability_key, count(*)::bigint AS call_count,
               bool_and(
                   space_id = $2
                   AND status = 'completed'
                   AND requester_agent = $3
                   AND target_agent = 'erhua'
                   AND source_type = 'space_agent_turn_broker'
                   AND source_refs->>'space_agent_turn_work_item_id' = $1::text
                   AND source_refs->>'call_id' = payload->>'call_id'
                   AND payload->>'capability_key' = capability_key
                   AND payload->>'schema_version' = $4::text
                   AND idempotency_key = $5 || (payload->>'call_id')
                   AND metadata->>'runner_identity' = $3
                   AND metadata->>'external_send_executed' = 'false'
                   AND metadata->>'automatic_retry_allowed' = 'false'
               ) AS receipt_valid
        FROM qintopia_agent_os.work_items
        WHERE parent_work_item_id = $1
          AND work_item_type = $6
        GROUP BY capability_key
        ORDER BY capability_key
        "#,
    )
    .bind(turn.work_item_id)
    .bind(turn.space_id)
    .bind(CLAIMED_BY)
    .bind(PROTOCOL_VERSION as i32)
    .bind(&idempotency_prefix)
    .bind(CAPABILITY_CALL_WORK_ITEM_TYPE)
    .fetch_all(&mut **tx)
    .await
    .context("load persisted Space agent-turn capability usage")?;
    let usage = rows
        .into_iter()
        .map(|row| {
            if !row.try_get::<bool, _>("receipt_valid")? {
                bail!("Space agent-turn capability receipt binding is invalid");
            }
            let call_count: i64 = row.try_get("call_count")?;
            let call_count = u16::try_from(call_count)
                .context("Space agent-turn capability receipt count is invalid")?;
            Ok(CapabilityUsage {
                capability_key: row.try_get("capability_key")?,
                call_count,
            })
        })
        .collect::<Result<Vec<_>>>()?;
    validate_capability_usage(&usage, allowed)?;
    Ok(usage)
}

async fn load_live_turn(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Uuid,
    expected_status: &str,
) -> Result<LiveTurn> {
    let row = sqlx::query(
        r#"
        SELECT child.id, child.parent_work_item_id, child.space_id, child.payload,
               parent.payload AS parent_payload,
               automation.id AS automation_id,
               automation.definition_digest AS automation_digest,
               automation.trigger_kind,
               automation.channel_event_mapping_id,
               business.id AS business_id,
               business.definition_digest AS business_digest,
               business.definition AS business_definition,
               business.allowed_capabilities AS business_allowed_capabilities,
               policy.id AS policy_id,
               policy.definition_digest AS policy_digest,
               policy.policy_config,
               conversation.chat_id AS conversation_chat_id,
               mapping.definition_digest AS mapping_digest
        FROM qintopia_agent_os.work_items child
        JOIN qintopia_agent_os.work_items parent
          ON parent.id = child.parent_work_item_id
         AND parent.space_id = child.space_id
         AND parent.work_item_type = 'space_automation_run'
         AND parent.status = 'completed'
        JOIN qintopia_agent_os.automation_definition_versions automation
          ON automation.id::text = child.payload->>'automation_definition_id'
         AND automation.space_id = child.space_id
         AND automation.status = 'active'
        JOIN qintopia_agent_os.business_definition_versions business
          ON business.id::text = child.payload->>'business_definition_id'
         AND business.id = automation.business_definition_id
         AND business.space_id = child.space_id
         AND business.status = 'active'
         AND business.execution_mode = 'agent_turn'
        JOIN qintopia_agent_os.space_policy_versions policy
          ON policy.id::text = child.payload->>'space_policy_version_id'
         AND policy.space_id = child.space_id
         AND policy.definition_key = 'default'
         AND policy.status = 'active'
        JOIN qintopia_messages.conversations conversation
          ON conversation.id = child.space_id
         AND conversation.platform = 'qiwe'
         AND conversation.chat_type = 'group'
         AND conversation.status = 'active'
        JOIN qintopia_agent_os.capabilities handoff
          ON handoff.capability_key = child.capability_key
         AND handoff.enabled
         AND handoff.provider_agent = 'erhua'
         AND 'system' = ANY(handoff.allowed_callers)
         AND 'space_agent_turn' = ANY(handoff.allowed_work_item_types)
         AND handoff.metadata ->> 'space_invocable' = 'true'
         AND handoff.metadata ->> 'space_scope_binding' = 'work_item_space_id'
         AND handoff.metadata ->> 'invocation_boundary' = 'erhua.execute_space_business'
         AND handoff.metadata ->> 'runner_contract' = 'dedicated_broker_v1'
         AND handoff.capability_key = ANY(business.allowed_capabilities)
         AND COALESCE(policy.policy_config->'capability_grants', '[]'::jsonb)
             ? handoff.capability_key
        LEFT JOIN qintopia_agent_os.channel_event_mapping_versions mapping
          ON mapping.id = automation.channel_event_mapping_id
        WHERE child.id = $1
          AND child.status = $2
          AND child.work_item_type = $3
          AND child.capability_key = $4
          AND child.requester_agent = 'system'
          AND child.target_agent = 'erhua'
          AND child.space_id IS NOT NULL
          AND child.metadata @> $5::jsonb
          AND (
              (automation.trigger_kind = 'schedule'
               AND automation.channel_event_mapping_id IS NULL)
              OR
              (automation.trigger_kind = 'event'
               AND automation.channel_event_mapping_id IS NOT NULL
               AND mapping.id IS NOT NULL
               AND mapping.status = 'active')
          )
        FOR UPDATE OF child
        "#,
    )
    .bind(work_item_id)
    .bind(expected_status)
    .bind(WORK_ITEM_TYPE)
    .bind(CAPABILITY_KEY)
    .bind(required_metadata())
    .fetch_optional(&mut **tx)
    .await
    .context("load live Space agent-turn binding")?
    .context("Space agent-turn live binding is no longer valid")?;
    let payload: Value = row.try_get("payload")?;
    validate_payload_shape(&payload)?;
    let business_definition: Value = row.try_get("business_definition")?;
    let goal = business_definition
        .get("goal")
        .and_then(Value::as_str)
        .map(str::trim)
        .filter(|value| !value.is_empty() && value.chars().count() <= 4_000)
        .context("live agent-turn goal is invalid")?
        .to_string();
    if goal.chars().any(char::is_control)
        || payload.get("goal").and_then(Value::as_str) != Some(goal.as_str())
    {
        bail!("Space agent-turn goal drifted from its business definition");
    }
    let output_contract = business_definition
        .get("output_contract")
        .cloned()
        .context("live agent-turn output_contract is missing")?;
    space_agent_turn::validate_output_contract(&output_contract)?;
    if payload.get("output_contract") != Some(&output_contract) {
        bail!("Space agent-turn output contract drifted from its business definition");
    }
    let output_contract_sha256 = space_agent_turn::output_contract_digest(&output_contract)?;
    verify_text_binding(
        &payload,
        "automation_definition_id",
        row.try_get::<Uuid, _>("automation_id")?,
    )?;
    verify_digest_binding(
        &payload,
        "automation_definition_digest",
        &row.try_get::<String, _>("automation_digest")?,
    )?;
    verify_text_binding(
        &payload,
        "business_definition_id",
        row.try_get::<Uuid, _>("business_id")?,
    )?;
    verify_digest_binding(
        &payload,
        "business_definition_digest",
        &row.try_get::<String, _>("business_digest")?,
    )?;
    verify_text_binding(
        &payload,
        "space_policy_version_id",
        row.try_get::<Uuid, _>("policy_id")?,
    )?;
    verify_digest_binding(
        &payload,
        "space_policy_digest",
        &row.try_get::<String, _>("policy_digest")?,
    )?;
    let trigger = validate_trigger(
        payload
            .get("trigger")
            .context("Space agent-turn trigger is missing")?,
        row.try_get::<String, _>("trigger_kind")?.as_str(),
    )?;
    validate_mapping_binding(
        &payload,
        row.try_get("channel_event_mapping_id")?,
        row.try_get("mapping_digest")?,
    )?;
    let parent_payload: Value = row.try_get("parent_payload")?;
    validate_parent_binding(&parent_payload, &payload, &trigger)?;
    let business_capabilities: Vec<String> = row.try_get("business_allowed_capabilities")?;
    let policy_config: Value = row.try_get("policy_config")?;
    let capabilities = load_capability_catalog(tx, &business_capabilities, &policy_config).await?;
    let expected_keys = capabilities
        .iter()
        .map(|capability| capability.capability_key.as_str())
        .collect::<Vec<_>>();
    let payload_keys = payload_capability_keys(&payload)?;
    if payload_keys != expected_keys {
        bail!("Space agent-turn capability ceiling changed after handoff");
    }
    Ok(LiveTurn {
        work_item_id,
        parent_work_item_id: row
            .try_get::<Option<Uuid>, _>("parent_work_item_id")?
            .context("Space agent-turn parent is missing")?,
        space_id: row.try_get("space_id")?,
        conversation_chat_id: row.try_get("conversation_chat_id")?,
        goal,
        trigger,
        output_contract,
        output_contract_sha256,
        capabilities,
    })
}

async fn load_capability_catalog(
    tx: &mut Transaction<'_, Postgres>,
    business_capabilities: &[String],
    policy_config: &Value,
) -> Result<Vec<CapabilityDescriptor>> {
    let grants = policy_config
        .get("capability_grants")
        .and_then(Value::as_array)
        .context("active Space policy capability_grants must be an array")?
        .iter()
        .map(|value| {
            value
                .as_str()
                .map(ToString::to_string)
                .context("Space policy capability grant must be a string")
        })
        .collect::<Result<BTreeSet<_>>>()?;
    let candidates = business_capabilities
        .iter()
        .filter(|key| key.as_str() != CAPABILITY_KEY && grants.contains(*key))
        .cloned()
        .collect::<Vec<_>>();
    if candidates.is_empty() {
        return Ok(Vec::new());
    }
    let rows = sqlx::query(
        r#"
        SELECT capability_key, input_schema, output_schema, risk_level, review_policy,
               metadata ->> 'space_agent_turn_recipe' AS execution_recipe
        FROM qintopia_agent_os.capabilities
        WHERE enabled
          AND capability_key = ANY($1)
          AND provider_agent = 'erhua'
          AND 'erhua' = ANY(allowed_callers)
          AND 'space_agent_turn' = ANY(allowed_work_item_types)
          AND metadata ->> 'space_invocable' = 'true'
          AND metadata ->> 'space_scope_binding' = 'work_item_space_id'
          AND metadata ->> 'invocation_boundary' = 'erhua.space_agent_turn'
          AND metadata ->> 'runner_access' = 'bounded_catalog_v1'
        ORDER BY capability_key
        "#,
    )
    .bind(&candidates)
    .fetch_all(&mut **tx)
    .await
    .context("load current Space agent-turn capability catalog")?;
    rows.into_iter()
        .map(|row| {
            let input_schema: Value = row.try_get("input_schema")?;
            let output_schema: Value = row.try_get("output_schema")?;
            if serde_json::to_vec(&input_schema)?.len() > 32 * 1024
                || serde_json::to_vec(&output_schema)?.len() > 32 * 1024
            {
                bail!("Space agent-turn capability schema exceeds the broker limit");
            }
            space_agent_turn::validate_output_contract(&input_schema)
                .context("Space agent-turn capability input schema is invalid")?;
            space_agent_turn::validate_output_contract(&output_schema)
                .context("Space agent-turn capability output schema is invalid")?;
            let execution_recipe: String = row.try_get("execution_recipe")?;
            if execution_recipe != SUBJECT_IDENTITY_RECIPE {
                bail!("Space agent-turn capability recipe is not registered");
            }
            Ok(CapabilityDescriptor {
                capability_key: row.try_get("capability_key")?,
                input_schema,
                output_schema,
                risk_level: row.try_get("risk_level")?,
                review_policy: row.try_get("review_policy")?,
                execution_recipe,
            })
        })
        .collect()
}

async fn terminalize_expired_claims(tx: &mut Transaction<'_, Postgres>) -> Result<usize> {
    let rows = sqlx::query(
        r#"
        WITH stale AS (
            SELECT id, parent_work_item_id
            FROM qintopia_agent_os.work_items
            WHERE work_item_type = $1 AND capability_key = $2
              AND status = 'processing' AND attempts = 1
              AND claimed_by = $3 AND claim_expires_at <= now()
              AND metadata @> $4::jsonb
            ORDER BY claim_expires_at, id
            FOR UPDATE SKIP LOCKED
            LIMIT $5
        )
        UPDATE qintopia_agent_os.work_items item
        SET status = 'failed', claimed_by = NULL, locked_at = NULL,
            claim_expires_at = NULL, last_error = 'runner_claim_expired_unknown',
            metadata = (metadata - 'space_agent_turn_claim')
                || jsonb_build_object(
                    'space_agent_turn_result',
                    jsonb_build_object(
                        'outcome', 'failed',
                        'failure_code', 'runner_claim_expired_unknown',
                        'automatic_retry_allowed', false
                    )
                ),
            updated_at = now()
        FROM stale
        WHERE item.id = stale.id
        RETURNING item.id, stale.parent_work_item_id
        "#,
    )
    .bind(WORK_ITEM_TYPE)
    .bind(CAPABILITY_KEY)
    .bind(CLAIMED_BY)
    .bind(required_metadata())
    .bind(EXPIRED_CLAIM_BATCH_SIZE as i64)
    .fetch_all(&mut **tx)
    .await
    .context("terminalize expired Space agent-turn claims")?;
    let terminalized = rows.len();
    for row in rows {
        let work_item_id: Uuid = row.try_get("id")?;
        let parent_work_item_id: Option<Uuid> = row.try_get("parent_work_item_id")?;
        append_expired_claim_events(tx, work_item_id, parent_work_item_id).await?;
    }
    Ok(terminalized)
}

async fn terminalize_exact_expired_claim(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Uuid,
    parent_work_item_id: Option<Uuid>,
    token_sha256: &str,
) -> Result<()> {
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = 'failed', claimed_by = NULL, locked_at = NULL,
            claim_expires_at = NULL, last_error = 'runner_claim_expired_unknown',
            metadata = (metadata - 'space_agent_turn_claim')
                || jsonb_build_object(
                    'space_agent_turn_result',
                    jsonb_build_object(
                        'outcome', 'failed',
                        'failure_code', 'runner_claim_expired_unknown',
                        'automatic_retry_allowed', false
                    )
                ),
            updated_at = now()
        WHERE id = $1 AND work_item_type = $2 AND capability_key = $3
          AND status = 'processing' AND attempts = 1 AND claimed_by = $4
          AND claim_expires_at <= now()
          AND metadata @> $5::jsonb
          AND metadata #>> '{space_agent_turn_claim,token_sha256}' = $6
        "#,
    )
    .bind(work_item_id)
    .bind(WORK_ITEM_TYPE)
    .bind(CAPABILITY_KEY)
    .bind(CLAIMED_BY)
    .bind(required_metadata())
    .bind(token_sha256)
    .execute(&mut **tx)
    .await
    .context("terminalize exact expired Space agent-turn claim")?;
    if updated.rows_affected() != 1 {
        bail!("expired Space agent-turn claim changed concurrently");
    }
    append_expired_claim_events(tx, work_item_id, parent_work_item_id).await
}

async fn append_expired_claim_events(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Uuid,
    parent_work_item_id: Option<Uuid>,
) -> Result<()> {
    append_event(
        tx,
        work_item_id,
        "space_agent_turn_claim_expired",
        "Space agent-turn claim expired with unknown capability outcome",
        json!({
            "failure_code": "runner_claim_expired_unknown",
            "external_send_executed": Value::Null,
            "automatic_retry_allowed": false
        }),
    )
    .await?;
    if let Some(parent_work_item_id) = parent_work_item_id {
        append_parent_event(
            tx,
            parent_work_item_id,
            "space_agent_turn_child_failed",
            "Space automation agent-turn child claim expired",
            json!({
                "child_work_item_id": work_item_id,
                "failure_code": "runner_claim_expired_unknown",
                "external_send_executed": Value::Null,
                "automatic_retry_allowed": false
            }),
        )
        .await?;
    }
    Ok(())
}

async fn terminalize_invalid_candidate(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Uuid,
) -> Result<()> {
    let row = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = 'failed', attempts = 1, last_error = 'invalid_runner_contract',
            claimed_by = NULL, locked_at = NULL, claim_expires_at = NULL,
            metadata = metadata || jsonb_build_object(
                'space_agent_turn_result',
                jsonb_build_object(
                    'outcome', 'failed',
                    'failure_code', 'invalid_runner_contract',
                    'automatic_retry_allowed', false
                )
            ),
            updated_at = now()
        WHERE id = $1 AND status = 'queued' AND attempts = 0
        RETURNING parent_work_item_id
        "#,
    )
    .bind(work_item_id)
    .fetch_optional(&mut **tx)
    .await
    .context("terminalize invalid Space agent-turn candidate")?
    .context("invalid Space agent-turn candidate changed concurrently")?;
    append_event(
        tx,
        work_item_id,
        "space_agent_turn_rejected",
        "Space agent-turn failed the live runner contract",
        json!({
            "failure_code": "invalid_runner_contract",
            "external_send_executed": false,
            "automatic_retry_allowed": false
        }),
    )
    .await?;
    if let Some(parent_work_item_id) = row.try_get::<Option<Uuid>, _>("parent_work_item_id")? {
        append_parent_event(
            tx,
            parent_work_item_id,
            "space_agent_turn_child_failed",
            "Space automation agent-turn child was rejected",
            json!({
                "child_work_item_id": work_item_id,
                "failure_code": "invalid_runner_contract",
                "external_send_executed": false,
                "automatic_retry_allowed": false
            }),
        )
        .await?;
    }
    Ok(())
}

async fn fail_claim(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Uuid,
    parent_work_item_id: Uuid,
    failure_code: &str,
) -> Result<()> {
    let failure_code = validated_failure_code(failure_code)?;
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET status = 'failed', claimed_by = NULL, locked_at = NULL,
            claim_expires_at = NULL, last_error = $2,
            metadata = (metadata - 'space_agent_turn_claim')
                || jsonb_build_object(
                    'space_agent_turn_result',
                    jsonb_build_object(
                        'outcome', 'failed',
                        'failure_code', $2::text,
                        'runner_identity', $3::text,
                        'automatic_retry_allowed', false
                    )
                ),
            updated_at = now()
        WHERE id = $1 AND status = 'processing' AND attempts = 1
          AND claimed_by = $3
        "#,
    )
    .bind(work_item_id)
    .bind(&failure_code)
    .bind(CLAIMED_BY)
    .execute(&mut **tx)
    .await
    .context("fail Space agent-turn claim")?;
    if updated.rows_affected() != 1 {
        bail!("Space agent-turn failure lost its exact claim");
    }
    append_event(
        tx,
        work_item_id,
        "space_agent_turn_failed",
        "Bounded runner failed one Space agent turn",
        json!({
            "failure_code": failure_code,
            "runner_identity": space_agent_turn::RUNNER_IDENTITY,
            "external_send_executed": Value::Null,
            "automatic_retry_allowed": false
        }),
    )
    .await?;
    append_parent_event(
        tx,
        parent_work_item_id,
        "space_agent_turn_child_failed",
        "Space automation agent-turn child failed",
        json!({
            "child_work_item_id": work_item_id,
            "failure_code": failure_code,
            "external_send_executed": Value::Null,
            "automatic_retry_allowed": false
        }),
    )
    .await
}

async fn append_event(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Uuid,
    event_type: &str,
    message: &str,
    data: Value,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, event_type, actor_type, actor_id, message, data)
        VALUES ($1, $2, 'agent', $3, $4, $5)
        "#,
    )
    .bind(work_item_id)
    .bind(event_type)
    .bind(CLAIMED_BY)
    .bind(message)
    .bind(data)
    .execute(&mut **tx)
    .await
    .context("append Space agent-turn broker event")?;
    Ok(())
}

async fn append_parent_event(
    tx: &mut Transaction<'_, Postgres>,
    parent_work_item_id: Uuid,
    event_type: &str,
    message: &str,
    data: Value,
) -> Result<()> {
    append_event(tx, parent_work_item_id, event_type, message, data).await
}

fn validate_payload_shape(payload: &Value) -> Result<()> {
    let object = payload
        .as_object()
        .context("Space agent-turn payload must be an object")?;
    let allowed = [
        "schema_version",
        "automation_definition_id",
        "automation_definition_digest",
        "business_definition_id",
        "business_definition_digest",
        "space_policy_version_id",
        "space_policy_digest",
        "channel_event_mapping_id",
        "channel_event_mapping_digest",
        "goal",
        "trigger",
        "allowed_capabilities",
        "output_contract",
    ];
    if object.keys().any(|key| !allowed.contains(&key.as_str()))
        || object.get("schema_version").and_then(Value::as_u64) != Some(1)
    {
        bail!("Space agent-turn payload shape is invalid");
    }
    Ok(())
}

fn validate_trigger(value: &Value, expected_kind: &str) -> Result<Value> {
    let object = value
        .as_object()
        .context("Space agent-turn trigger must be an object")?;
    let kind = object
        .get("kind")
        .and_then(Value::as_str)
        .context("Space agent-turn trigger kind is missing")?;
    if kind != expected_kind {
        bail!("Space agent-turn trigger kind drifted");
    }
    let allowed: &[&str] = match kind {
        "event" => &[
            "kind",
            "event_type",
            "provider_event_ref",
            "subject_user_ids",
            "occurred_at",
        ],
        "schedule" => &["kind", "scheduled_for_utc"],
        _ => bail!("Space agent-turn trigger kind is unsupported"),
    };
    if object.keys().any(|key| !allowed.contains(&key.as_str())) {
        bail!("Space agent-turn trigger contains an unsupported field");
    }
    match kind {
        "event" => {
            if object.len() != allowed.len()
                || !safe_event_type(required_trigger_text(object, "event_type", 128)?)
            {
                bail!("Space agent-turn event trigger is incomplete or invalid");
            }
            let provider_event_ref = required_trigger_text(object, "provider_event_ref", 512)?;
            validate_opaque_trigger_value(provider_event_ref, "provider_event_ref")?;
            let subjects = object
                .get("subject_user_ids")
                .and_then(Value::as_array)
                .context("Space agent-turn subject_user_ids must be an array")?;
            if subjects.is_empty() || subjects.len() > 64 {
                bail!("Space agent-turn subject_user_ids count is invalid");
            }
            let mut seen = BTreeSet::new();
            for subject in subjects {
                let subject = subject
                    .as_str()
                    .context("Space agent-turn subject_user_ids must contain strings")?;
                validate_opaque_trigger_value(subject, "subject_user_id")?;
                if subject.len() > 256 || !seen.insert(subject) {
                    bail!("Space agent-turn subject_user_ids are invalid or duplicated");
                }
            }
            validate_rfc3339(required_trigger_text(object, "occurred_at", 64)?)?;
        }
        "schedule" => {
            if object.len() != allowed.len() {
                bail!("Space agent-turn schedule trigger is incomplete");
            }
            validate_rfc3339(required_trigger_text(object, "scheduled_for_utc", 64)?)?;
        }
        _ => unreachable!(),
    }
    let encoded = serde_json::to_vec(value).context("encode Space agent-turn trigger")?;
    if encoded.len() > 32 * 1024 {
        bail!("Space agent-turn trigger exceeds the broker limit");
    }
    Ok(value.clone())
}

fn validate_parent_binding(parent: &Value, child: &Value, trigger: &Value) -> Result<()> {
    for key in ["automation_definition_id", "business_definition_id"] {
        if parent.get(key) != child.get(key) {
            bail!("Space agent-turn parent binding drifted");
        }
    }
    if parent.get("trigger") != Some(trigger) {
        bail!("Space agent-turn trigger drifted from its parent work item");
    }
    Ok(())
}

fn required_trigger_text<'a>(
    object: &'a serde_json::Map<String, Value>,
    key: &str,
    maximum_bytes: usize,
) -> Result<&'a str> {
    let value = object
        .get(key)
        .and_then(Value::as_str)
        .with_context(|| format!("Space agent-turn trigger {key} must be a string"))?;
    if value.is_empty() || value.len() > maximum_bytes || value.trim() != value {
        bail!("Space agent-turn trigger {key} is outside bounded limits");
    }
    Ok(value)
}

fn validate_opaque_trigger_value(value: &str, name: &str) -> Result<()> {
    if value
        .chars()
        .any(|character| character.is_control() || character.is_whitespace())
    {
        bail!("Space agent-turn trigger {name} is invalid");
    }
    Ok(())
}

fn safe_event_type(value: &str) -> bool {
    let mut characters = value.chars();
    matches!(characters.next(), Some(character) if character.is_ascii_lowercase() || character.is_ascii_digit())
        && characters.all(|character| {
            character.is_ascii_lowercase()
                || character.is_ascii_digit()
                || "._:-".contains(character)
        })
}

fn validate_rfc3339(value: &str) -> Result<()> {
    DateTime::parse_from_rfc3339(value)
        .context("Space agent-turn trigger timestamp must be RFC3339")?;
    Ok(())
}

fn validate_mapping_binding(
    payload: &Value,
    mapping_id: Option<Uuid>,
    mapping_digest: Option<String>,
) -> Result<()> {
    match (mapping_id, mapping_digest) {
        (Some(id), Some(digest)) => {
            verify_text_binding(payload, "channel_event_mapping_id", id)?;
            verify_digest_binding(payload, "channel_event_mapping_digest", &digest)
        }
        (None, None)
            if payload
                .get("channel_event_mapping_id")
                .is_none_or(Value::is_null)
                && payload
                    .get("channel_event_mapping_digest")
                    .is_none_or(Value::is_null) =>
        {
            Ok(())
        }
        _ => bail!("Space agent-turn event-mapping binding is incomplete"),
    }
}

fn payload_capability_keys(payload: &Value) -> Result<Vec<&str>> {
    let values = payload
        .get("allowed_capabilities")
        .and_then(Value::as_array)
        .context("Space agent-turn allowed_capabilities must be an array")?;
    if values.len() > MAX_CAPABILITY_USAGE {
        bail!("Space agent-turn capability ceiling is too large");
    }
    let keys = values
        .iter()
        .map(|value| {
            value
                .as_str()
                .context("Space agent-turn capability keys must be strings")
        })
        .collect::<Result<Vec<_>>>()?;
    if keys.windows(2).any(|pair| pair[0] >= pair[1]) {
        bail!("Space agent-turn capability keys must be sorted and unique");
    }
    Ok(keys)
}

fn validate_capability_usage(usage: &[CapabilityUsage], allowed: &BTreeSet<&str>) -> Result<()> {
    if usage.len() > MAX_CAPABILITY_USAGE {
        bail!("Space agent-turn capability usage is too large");
    }
    let mut seen = BTreeSet::new();
    for item in usage {
        if !allowed.contains(item.capability_key.as_str())
            || item.call_count == 0
            || item.call_count > MAX_CAPABILITY_CALLS
            || !seen.insert(item.capability_key.as_str())
        {
            bail!("Space agent-turn reported unauthorized capability usage");
        }
    }
    Ok(())
}

fn capability_usage_map(
    usage: &[CapabilityUsage],
    allowed: &BTreeSet<&str>,
) -> Result<BTreeMap<String, u16>> {
    validate_capability_usage(usage, allowed)?;
    Ok(usage
        .iter()
        .map(|item| (item.capability_key.clone(), item.call_count))
        .collect())
}

fn verify_text_binding(payload: &Value, key: &str, expected: Uuid) -> Result<()> {
    if payload.get(key).and_then(Value::as_str) != Some(expected.to_string().as_str()) {
        bail!("Space agent-turn UUID binding drifted");
    }
    Ok(())
}

fn verify_digest_binding(payload: &Value, key: &str, expected: &str) -> Result<()> {
    if !valid_sha256(expected) || payload.get(key).and_then(Value::as_str) != Some(expected) {
        bail!("Space agent-turn digest binding drifted");
    }
    Ok(())
}

fn validate_protocol(schema_version: u8) -> Result<()> {
    if schema_version != PROTOCOL_VERSION {
        bail!("unsupported Space agent-turn broker schema version");
    }
    Ok(())
}

fn validate_failure_code(value: &str) -> Result<()> {
    validated_failure_code(value).map(|_| ())
}

fn validated_failure_code(value: &str) -> Result<String> {
    let value = value.trim();
    if value.is_empty()
        || value.len() > 80
        || !value.is_ascii()
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'_')
    {
        bail!("Space agent-turn failure_code is invalid");
    }
    Ok(value.to_string())
}

fn validate_secret_shape(value: &str) -> Result<()> {
    if !(32..=512).contains(&value.len()) || value.chars().any(char::is_whitespace) {
        bail!("Space agent-turn secret shape is invalid");
    }
    Ok(())
}

fn validated_sha256(value: Option<&str>, name: &str) -> Result<String> {
    let value = value
        .map(str::trim)
        .filter(|value| valid_sha256(value))
        .with_context(|| format!("{name} is required"))?;
    Ok(value.to_ascii_lowercase())
}

fn validated_os_id(value: Option<&str>, name: &str) -> Result<u32> {
    let value = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .with_context(|| format!("{name} is required"))?;
    let value = value
        .parse::<u32>()
        .with_context(|| format!("{name} must be an unsigned integer"))?;
    if value == 0 {
        bail!("{name} must not be root");
    }
    Ok(value)
}

fn valid_sha256(value: &str) -> bool {
    value.len() == 64 && value.bytes().all(|byte| byte.is_ascii_hexdigit())
}

fn new_claim_token() -> String {
    let first = Uuid::new_v4().simple().to_string();
    let second = Uuid::new_v4().simple().to_string();
    format!("{first}{second}")
}

fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn constant_time_eq(left: &[u8], right: &[u8]) -> bool {
    if left.len() != right.len() {
        return false;
    }
    left.iter()
        .zip(right)
        .fold(0_u8, |difference, (left, right)| {
            difference | (left ^ right)
        })
        == 0
}

fn required_metadata() -> Value {
    json!({
        "space_bound": true,
        "definition_bound": true,
        "handoff_state": space_agent_turn::HANDOFF_STATE,
        "executor_boundary": space_agent_turn::EXECUTOR_BOUNDARY,
        "runner_identity": space_agent_turn::RUNNER_IDENTITY,
        "runner_contract_version": space_agent_turn::RUNNER_CONTRACT_VERSION
    })
}

fn result_artifact_metadata(
    output: &Value,
    output_contract_sha256: &str,
    capability_usage: &[CapabilityUsage],
) -> Value {
    json!({
        "schema_version": PROTOCOL_VERSION,
        "output": output,
        "output_contract_sha256": output_contract_sha256,
        "capability_usage": capability_usage,
        "content_trust": "untrusted_agent_output",
        "execution_eligible": false,
        "routing_authority": "none",
        "downstream_scope_source": "trusted_space_context_only"
    })
}

fn finish_response(status: &str) -> Value {
    json!({
        "schema_version": PROTOCOL_VERSION,
        "accepted": true,
        "status": status,
        "external_send_executed": Value::Null,
        "automatic_retry_allowed": false
    })
}

fn expired_finish_response() -> Value {
    json!({
        "schema_version": PROTOCOL_VERSION,
        "accepted": true,
        "status": "failed",
        "failure_code": "runner_claim_expired_unknown",
        "external_send_executed": Value::Null,
        "automatic_retry_allowed": false
    })
}

async fn read_request(stream: &mut UnixStream) -> Result<BrokerRequest> {
    let mut bytes = Vec::new();
    let mut reader = BufReader::new(stream.take(MAX_MESSAGE_BYTES + 1));
    let count = timeout(READ_TIMEOUT, reader.read_until(b'\n', &mut bytes))
        .await
        .context("Space agent-turn broker read timed out")??;
    if count == 0 || bytes.len() as u64 > MAX_MESSAGE_BYTES {
        bail!("Space agent-turn broker request length is invalid");
    }
    while matches!(bytes.last(), Some(b'\n' | b'\r')) {
        bytes.pop();
    }
    decode_request(&bytes)
}

fn decode_request(bytes: &[u8]) -> Result<BrokerRequest> {
    let value = parse_strict_bounded_slice(bytes, BROKER_JSON_LIMITS)
        .context("parse strict Space agent-turn broker request")?;
    serde_json::from_value(value).context("decode Space agent-turn broker request")
}

async fn write_response(stream: &mut UnixStream, response: &impl Serialize) -> Result<()> {
    let mut bytes = serde_json::to_vec(response).context("serialize agent-turn response")?;
    bytes.push(b'\n');
    timeout(WRITE_TIMEOUT, stream.write_all(&bytes))
        .await
        .context("Space agent-turn broker write timed out")??;
    Ok(())
}

fn prepare_socket(path: &Path, runner_uid: u32, runner_gid: u32) -> Result<u32> {
    if !path.is_absolute()
        || path.file_name().and_then(|name| name.to_str()) != Some("space-agent-turn.sock")
    {
        bail!("Space agent-turn broker socket path is invalid");
    }
    let parent = path
        .parent()
        .context("Space agent-turn broker socket parent is missing")?;
    let parent_metadata = fs::symlink_metadata(parent)
        .context("Space agent-turn broker socket parent is unavailable")?;
    if parent_metadata.file_type().is_symlink() || !parent_metadata.is_dir() {
        bail!("Space agent-turn broker socket parent is invalid");
    }
    if parent_metadata.permissions().mode() & 0o7777 != 0o750 {
        bail!("Space agent-turn broker socket parent mode must be 0750");
    }
    if parent_metadata.uid() == runner_uid || parent_metadata.gid() != runner_gid {
        bail!("Space agent-turn broker socket parent ownership is invalid");
    }
    if path.exists() {
        if !path_is_socket(path) {
            bail!("Space agent-turn broker path exists and is not a socket");
        }
        fs::remove_file(path).context("remove stale Space agent-turn broker socket")?;
    }
    Ok(parent_metadata.uid())
}

fn validate_bound_socket(path: &Path, owner_uid: u32, runner_gid: u32) -> Result<()> {
    let metadata = fs::symlink_metadata(path).context("inspect bound Space agent-turn socket")?;
    if !metadata.file_type().is_socket()
        || metadata.uid() != owner_uid
        || metadata.gid() != runner_gid
        || metadata.permissions().mode() & 0o7777 != 0o660
    {
        bail!("Space agent-turn broker socket ownership or mode is invalid");
    }
    Ok(())
}

fn validate_peer(stream: &UnixStream, runner_uid: u32, runner_gid: u32) -> Result<()> {
    let credentials = stream
        .peer_cred()
        .context("inspect Space agent-turn runner peer credentials")?;
    if credentials.uid() != runner_uid || credentials.gid() != runner_gid {
        bail!("Space agent-turn runner peer identity is not authorized");
    }
    Ok(())
}

fn path_is_socket(path: &Path) -> bool {
    fs::symlink_metadata(path)
        .map(|metadata| metadata.file_type().is_socket())
        .unwrap_or(false)
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::*;

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
        assert!(matches!(parsed.scheme(), "postgres" | "postgresql"));
        assert!(matches!(parsed.host_str(), Some("127.0.0.1" | "::1")));
        assert_eq!(parsed.path().trim_start_matches('/'), "qintopia_test");
        database_url
    }

    #[cfg(feature = "postgres-integration-tests")]
    async fn seed_expiring_claim(
        pool: &PgPool,
        space_id: Uuid,
        parent_work_item_id: Uuid,
        suffix: &str,
        token: &str,
        expires_at: DateTime<Utc>,
    ) -> Uuid {
        let mut metadata = required_metadata();
        metadata["space_agent_turn_claim"] = json!({
            "schema_version": PROTOCOL_VERSION,
            "runner_identity": space_agent_turn::RUNNER_IDENTITY,
            "token_sha256": sha256_hex(token.as_bytes()),
            "output_contract_sha256": "a".repeat(64)
        });
        sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (parent_work_item_id, space_id, work_item_type, status,
                 requester_agent, target_agent, capability_key, brief_summary,
                 dedupe_key, idempotency_key, payload, metadata, attempts,
                 claimed_by, locked_at, claim_expires_at)
            VALUES
                ($1, $2, $3, 'processing', 'system', 'erhua', $4,
                 'bounded agent turn', $5, $5, '{}'::jsonb, $6, 1, $7,
                 now() - interval '1 minute', $8)
            RETURNING id
            "#,
        )
        .bind(parent_work_item_id)
        .bind(space_id)
        .bind(WORK_ITEM_TYPE)
        .bind(CAPABILITY_KEY)
        .bind(format!("space-agent-turn-expiry-{suffix}"))
        .bind(metadata)
        .bind(CLAIMED_BY)
        .bind(expires_at)
        .fetch_one(pool)
        .await
        .expect("seed expiring Space agent-turn claim")
    }

    #[cfg(feature = "postgres-integration-tests")]
    async fn seed_claimable_turn(
        pool: &PgPool,
        space_id: Uuid,
        created_by_person_id: Uuid,
        suffix: &str,
        runtime_capability_keys: &[&str],
    ) -> (Uuid, Uuid) {
        let policy_digest = "b".repeat(64);
        let business_digest = "c".repeat(64);
        let automation_digest = "d".repeat(64);
        let mut allowed_capabilities = vec![CAPABILITY_KEY.to_string()];
        allowed_capabilities.extend(
            runtime_capability_keys
                .iter()
                .map(|capability| (*capability).to_string()),
        );
        allowed_capabilities.sort();
        allowed_capabilities.dedup();
        let mut runtime_capabilities = runtime_capability_keys
            .iter()
            .map(|capability| (*capability).to_string())
            .collect::<Vec<_>>();
        runtime_capabilities.sort();
        runtime_capabilities.dedup();
        let policy_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_agent_os.space_policy_versions
                (space_id, definition_key, version, policy_config, status,
                 definition_digest, created_by_person_id, activated_at)
            VALUES
                ($1, $2, 1, $3, 'active', $4, $5, now())
            RETURNING id
            "#,
        )
        .bind(space_id)
        .bind("default")
        .bind(json!({"capability_grants": allowed_capabilities}))
        .bind(&policy_digest)
        .bind(created_by_person_id)
        .fetch_one(pool)
        .await
        .expect("seed active Space agent-turn policy");
        let business_definition = json!({
            "goal": "Produce one bounded broker integration-test result.",
            "output_contract": output_contract()
        });
        let business_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_agent_os.business_definition_versions
                (space_id, definition_key, version, execution_mode, definition,
                 allowed_capabilities, approval_policy, status, definition_digest,
                 created_by_person_id, activated_at)
            VALUES
                ($1, $2, 1, 'agent_turn', $3, $4, 'space_admin_confirmation',
                 'active', $5, $6, now())
            RETURNING id
            "#,
        )
        .bind(space_id)
        .bind(format!("broker_business_{suffix}"))
        .bind(&business_definition)
        .bind(&allowed_capabilities)
        .bind(&business_digest)
        .bind(created_by_person_id)
        .fetch_one(pool)
        .await
        .expect("seed active Space agent-turn business");
        let automation_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_agent_os.automation_definition_versions
                (space_id, definition_key, version, business_definition_id,
                 trigger_kind, trigger_config, timezone, misfire_policy, status,
                 definition_digest, created_by_person_id, activated_at)
            VALUES
                ($1, $2, 1, $3, 'schedule', '{"cron":"* * * * *"}'::jsonb,
                 'UTC', 'run_once', 'active', $4, $5, now())
            RETURNING id
            "#,
        )
        .bind(space_id)
        .bind(format!("broker_automation_{suffix}"))
        .bind(business_id)
        .bind(&automation_digest)
        .bind(created_by_person_id)
        .fetch_one(pool)
        .await
        .expect("seed active Space agent-turn automation");
        let trigger = json!({
            "kind": "schedule",
            "scheduled_for_utc": "2026-08-15T00:00:00Z"
        });
        let parent_payload = json!({
            "automation_definition_id": automation_id,
            "business_definition_id": business_id,
            "trigger": trigger
        });
        let parent_work_item_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (space_id, work_item_type, status, requester_agent, target_agent,
                 capability_key, brief_summary, dedupe_key, idempotency_key, payload)
            VALUES
                ($1, 'space_automation_run', 'completed', 'system', 'erhua',
                 'erhua.execute_space_business', 'claimable parent Space automation run',
                 $2, $2, $3)
            RETURNING id
            "#,
        )
        .bind(space_id)
        .bind(format!("space-agent-turn-claim-parent-{suffix}"))
        .bind(&parent_payload)
        .fetch_one(pool)
        .await
        .expect("seed completed parent Space automation run");
        let child_payload = json!({
            "schema_version": 1,
            "automation_definition_id": automation_id,
            "automation_definition_digest": automation_digest,
            "business_definition_id": business_id,
            "business_definition_digest": business_digest,
            "space_policy_version_id": policy_id,
            "space_policy_digest": policy_digest,
            "goal": business_definition["goal"],
            "trigger": parent_payload["trigger"],
            "allowed_capabilities": runtime_capabilities,
            "output_contract": business_definition["output_contract"]
        });
        let child_work_item_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (parent_work_item_id, space_id, work_item_type, status,
                 requester_agent, target_agent, capability_key, available_at,
                 brief_summary, dedupe_key, idempotency_key, payload, metadata)
            VALUES
                ($1, $2, $3, 'queued', 'system', 'erhua', $4, '-infinity'::timestamptz,
                 'claimable bounded agent turn', $5, $5, $6, $7)
            RETURNING id
            "#,
        )
        .bind(parent_work_item_id)
        .bind(space_id)
        .bind(WORK_ITEM_TYPE)
        .bind(CAPABILITY_KEY)
        .bind(format!("space-agent-turn-claim-child-{suffix}"))
        .bind(child_payload)
        .bind(required_metadata())
        .fetch_one(pool)
        .await
        .expect("seed claimable Space agent-turn child");
        (child_work_item_id, parent_work_item_id)
    }

    fn output_contract() -> Value {
        json!({
            "type": "object",
            "additionalProperties": false,
            "required": ["summary"],
            "properties": {
                "summary": {"type": "string", "minLength": 1, "maxLength": 200}
            }
        })
    }

    #[test]
    fn default_disabled_owner_and_database_gates_are_exact() {
        let database_url = "postgresql://example.invalid/qintopia";
        let database_hash = sha256_hex(database_url.as_bytes());
        let runner_hash = "a".repeat(64);
        assert!(BrokerConfig::from_values(
            database_url,
            None,
            Some(APPROVAL_PHRASE),
            Some(&database_hash),
            Some(&runner_hash),
            Some("2001"),
            Some("2002")
        )
        .is_err());
        assert!(BrokerConfig::from_values(
            database_url,
            Some("1"),
            Some("wrong"),
            Some(&database_hash),
            Some(&runner_hash),
            Some("2001"),
            Some("2002")
        )
        .is_err());
        BrokerConfig::from_values(
            database_url,
            Some("1"),
            Some(APPROVAL_PHRASE),
            Some(&database_hash),
            Some(&runner_hash),
            Some("2001"),
            Some("2002"),
        )
        .expect("exact reviewed broker configuration");
        assert!(BrokerConfig::from_values(
            database_url,
            Some("1"),
            Some(APPROVAL_PHRASE),
            Some(&database_hash),
            Some(&runner_hash),
            Some("0"),
            Some("2002"),
        )
        .is_err());
    }

    #[test]
    fn wire_contract_rejects_unknown_fields_and_wrong_identity() {
        let request = serde_json::from_value::<BrokerRequest>(json!({
            "operation": "space_agent_turn_claim",
            "schema_version": 1,
            "runner_identity": space_agent_turn::RUNNER_IDENTITY,
            "runner_token": "x".repeat(64)
        }))
        .expect("bounded claim request");
        assert!(matches!(request, BrokerRequest::Claim { .. }));
        assert!(serde_json::from_value::<BrokerRequest>(json!({
            "operation": "space_agent_turn_claim",
            "schema_version": 1,
            "runner_identity": space_agent_turn::RUNNER_IDENTITY,
            "runner_token": "x".repeat(64),
            "space_id": Uuid::new_v4()
        }))
        .is_err());
        for forbidden in ["target", "destination", "url"] {
            let mut request = json!({
                "operation": "space_agent_turn_claim",
                "schema_version": 1,
                "runner_identity": space_agent_turn::RUNNER_IDENTITY,
                "runner_token": "x".repeat(64)
            });
            request[forbidden] = json!("forged");
            assert!(serde_json::from_value::<BrokerRequest>(request).is_err());
        }

        let token = "runner-secret-with-more-than-thirty-two-bytes";
        let config = BrokerConfig {
            runner_token_sha256: sha256_hex(token.as_bytes()),
            runner_uid: 2001,
            runner_gid: 2002,
        };
        config
            .authenticate(
                space_agent_turn::RUNNER_IDENTITY,
                &SensitiveString(token.to_string()),
            )
            .expect("fixed runner identity and secret");
        assert!(config
            .authenticate("forged-runner", &SensitiveString(token.to_string()))
            .is_err());
    }

    #[test]
    fn wire_contract_rejects_duplicate_keys_before_deserialization() {
        let duplicate = format!(
            r#"{{"operation":"space_agent_turn_claim","schema_version":1,"schema_version":2,"runner_identity":"{}","runner_token":"{}"}}"#,
            space_agent_turn::RUNNER_IDENTITY,
            "x".repeat(64)
        );
        assert!(decode_request(duplicate.as_bytes()).is_err());
    }

    #[test]
    fn invoke_wire_contract_is_strict_and_target_free() {
        let work_item_id = Uuid::new_v4();
        let call_id = Uuid::new_v4();
        let request = json!({
            "operation": "space_agent_turn_invoke",
            "schema_version": PROTOCOL_VERSION,
            "runner_identity": space_agent_turn::RUNNER_IDENTITY,
            "runner_token": "x".repeat(64),
            "work_item_id": work_item_id,
            "claim_token": "y".repeat(64),
            "call_id": call_id,
            "capability_key": SUBJECT_IDENTITY_CAPABILITY_KEY,
            "input": {"scope": "trigger_subjects"}
        });
        let decoded = serde_json::from_value::<BrokerRequest>(request.clone())
            .expect("strict capability invocation request");
        assert!(matches!(
            decoded,
            BrokerRequest::Invoke {
                work_item_id: decoded_work_item_id,
                call_id: decoded_call_id,
                ..
            } if decoded_work_item_id == work_item_id && decoded_call_id == call_id
        ));
        for forbidden in ["space_id", "chat_id", "target", "destination", "url"] {
            let mut forged = request.clone();
            forged[forbidden] = json!("forged");
            assert!(serde_json::from_value::<BrokerRequest>(forged).is_err());
        }
    }

    #[test]
    fn capability_catalog_and_contract_reject_forged_scope() {
        let capability = CapabilityDescriptor {
            capability_key: SUBJECT_IDENTITY_CAPABILITY_KEY.to_string(),
            input_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["scope"],
                "properties": {
                    "scope": {"type": "string", "const": "trigger_subjects"}
                }
            }),
            output_schema: json!({
                "type": "object",
                "additionalProperties": false,
                "required": ["members"],
                "properties": {
                    "members": {"type": "array", "maxItems": 64, "items": {
                        "type": "object",
                        "additionalProperties": false,
                        "required": ["user_id", "display_name", "resolved"],
                        "properties": {
                            "user_id": {"type": "string", "minLength": 1, "maxLength": 256},
                            "display_name": {"type": "string", "maxLength": 200},
                            "resolved": {"type": "boolean"}
                        }
                    }}
                }
            }),
            risk_level: "low".to_string(),
            review_policy: "definition_policy".to_string(),
            execution_recipe: SUBJECT_IDENTITY_RECIPE.to_string(),
        };
        let turn = LiveTurn {
            work_item_id: Uuid::new_v4(),
            parent_work_item_id: Uuid::new_v4(),
            space_id: Uuid::new_v4(),
            conversation_chat_id: "trusted-current-room".to_string(),
            goal: "Resolve trigger subjects".to_string(),
            trigger: json!({"kind": "schedule", "scheduled_for_utc": "2026-08-15T00:00:00Z"}),
            output_contract: output_contract(),
            output_contract_sha256: "a".repeat(64),
            capabilities: vec![capability.clone()],
        };
        assert!(select_live_capability(&turn, SUBJECT_IDENTITY_CAPABILITY_KEY).is_ok());
        assert!(select_live_capability(&turn, "erhua.forged_capability").is_err());
        space_agent_turn::validate_output(
            &capability.input_schema,
            &json!({"scope": "trigger_subjects"}),
        )
        .expect("fixed trigger-subject scope");
        for forged in [
            json!({"scope": "arbitrary_users"}),
            json!({"scope": "trigger_subjects", "user_ids": ["forged"]}),
            json!({"scope": "trigger_subjects", "target": "forged-room"}),
        ] {
            assert!(space_agent_turn::validate_output(&capability.input_schema, &forged).is_err());
        }
        space_agent_turn::validate_output(
            &capability.output_schema,
            &json!({"members": [{
                "user_id": "9007199254740993",
                "display_name": "Member",
                "resolved": true
            }]}),
        )
        .expect("bounded identity output");
    }

    #[test]
    fn completion_contract_rejects_unauthorized_capability_usage() {
        let allowed = BTreeSet::from(["erhua.safe_lookup"]);
        validate_capability_usage(
            &[CapabilityUsage {
                capability_key: "erhua.safe_lookup".to_string(),
                call_count: 1,
            }],
            &allowed,
        )
        .expect("authorized capability usage");
        assert!(validate_capability_usage(
            &[CapabilityUsage {
                capability_key: "erhua.qiwe_send_direct_message".to_string(),
                call_count: 1,
            }],
            &allowed,
        )
        .is_err());
        space_agent_turn::validate_output(&output_contract(), &json!({"summary": "done"}))
            .expect("runner result honors business contract");

        let persisted = [CapabilityUsage {
            capability_key: "erhua.safe_lookup".to_string(),
            call_count: 2,
        }];
        assert_ne!(
            capability_usage_map(
                &[CapabilityUsage {
                    capability_key: "erhua.safe_lookup".to_string(),
                    call_count: 1,
                }],
                &allowed,
            )
            .expect("reported usage"),
            capability_usage_map(&persisted, &allowed).expect("persisted usage")
        );
    }

    #[test]
    fn broker_metadata_has_no_room_or_target_binding() {
        let metadata = required_metadata();
        let encoded = metadata.to_string();
        assert!(!encoded.contains("space_id"));
        assert!(!encoded.contains("chat_id"));
        assert!(!encoded.contains("target"));
        assert_eq!(
            metadata["runner_identity"],
            space_agent_turn::RUNNER_IDENTITY
        );
    }

    #[test]
    fn completed_output_is_inert_and_never_a_routing_authority() {
        let metadata = result_artifact_metadata(
            &json!({"summary": "https://example.invalid/looks-like-a-target"}),
            &"a".repeat(64),
            &[],
        );
        assert_eq!(metadata["content_trust"], "untrusted_agent_output");
        assert_eq!(metadata["execution_eligible"], false);
        assert_eq!(metadata["routing_authority"], "none");
        assert_eq!(
            metadata["downstream_scope_source"],
            "trusted_space_context_only"
        );
    }

    #[test]
    fn trigger_and_parent_bindings_are_closed_and_exact() {
        let automation_id = Uuid::new_v4();
        let business_id = Uuid::new_v4();
        let trigger = json!({
            "kind": "event",
            "event_type": "group_member_add",
            "provider_event_ref": "qiwe:event-1",
            "subject_user_ids": ["9007199254740993"],
            "occurred_at": "2026-08-14T00:00:00Z"
        });
        let validated = validate_trigger(&trigger, "event").expect("bounded event trigger");
        let parent = json!({
            "automation_definition_id": automation_id,
            "business_definition_id": business_id,
            "trigger": trigger
        });
        let child = parent.clone();
        validate_parent_binding(&parent, &child, &validated).expect("exact parent binding");

        let mut forged = child.clone();
        forged["trigger"]["provider_event_ref"] = json!("qiwe:other-event");
        assert!(validate_parent_binding(&parent, &forged, &forged["trigger"]).is_err());
        forged["trigger"]["target"] = json!("forged-room");
        assert!(validate_trigger(&forged["trigger"], "event").is_err());
        forged["trigger"] = json!({
            "kind": "event",
            "event_type": "group_member_add",
            "provider_event_ref": "qiwe:event-1",
            "subject_user_ids": ["9007199254740993"],
            "occurred_at": "not-a-time"
        });
        assert!(validate_trigger(&forged["trigger"], "event").is_err());
    }

    #[cfg(feature = "postgres-integration-tests")]
    #[tokio::test]
    #[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
    async fn postgres_claim_expiry_and_reconciliation_contract() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 4)
            .await
            .expect("connect disposable PostgreSQL");
        db::run_migrations(&pool)
            .await
            .expect("migrate disposable PostgreSQL");
        reconcile_all_expired_claims(&pool)
            .await
            .expect("clear prior expired Space agent-turn fixtures");

        let suffix = Uuid::new_v4().simple().to_string();
        let space_chat_id = format!("space-agent-turn-expiry-{suffix}");
        let space_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_messages.conversations
                (platform, chat_id, chat_type, display_name)
            VALUES ('qiwe', $1, 'group', 'Agent-turn expiry integration Space')
            RETURNING id
            "#,
        )
        .bind(&space_chat_id)
        .fetch_one(&pool)
        .await
        .expect("seed Space agent-turn integration conversation");
        let parent_work_item_id: Uuid = sqlx::query_scalar(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (space_id, work_item_type, status, requester_agent, target_agent,
                 capability_key, brief_summary, dedupe_key, idempotency_key)
            VALUES
                ($1, 'space_automation_run', 'processing', 'system', 'erhua',
                 'erhua.execute_space_business', 'parent Space automation run',
                 $2, $2)
            RETURNING id
            "#,
        )
        .bind(space_id)
        .bind(format!("space-agent-turn-parent-{suffix}"))
        .fetch_one(&pool)
        .await
        .expect("seed parent Space automation run");

        let created_by_person_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO qintopia_identity.persons (id, display_name, primary_name)
            VALUES ($1, 'Broker integration owner', 'Broker integration owner')
            "#,
        )
        .bind(created_by_person_id)
        .execute(&pool)
        .await
        .expect("seed broker integration owner");
        sqlx::query(
            "UPDATE qintopia_agent_os.capabilities SET enabled = true WHERE capability_key = $1",
        )
        .bind(CAPABILITY_KEY)
        .execute(&pool)
        .await
        .expect("enable Space agent-turn capability in disposable database");
        let (claimable_work_item_id, claimable_parent_work_item_id) =
            seed_claimable_turn(&pool, space_id, created_by_person_id, &suffix, &[]).await;
        let database_before_claim: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&pool)
            .await
            .expect("read PostgreSQL clock before claim");
        let claimed = claim(&pool).await.expect("claim live Space agent turn");
        let database_after_claim: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&pool)
            .await
            .expect("read PostgreSQL clock after claim");
        assert_eq!(claimed["claimed"], true);
        assert_eq!(
            claimed["work_item_id"].as_str(),
            Some(claimable_work_item_id.to_string().as_str())
        );
        let returned_claim_expiry = DateTime::parse_from_rfc3339(
            claimed["claim_expires_at"]
                .as_str()
                .expect("claim expiry timestamp"),
        )
        .expect("claim expiry must be RFC3339")
        .with_timezone(&Utc);
        assert!(
            returned_claim_expiry
                >= database_before_claim + chrono::Duration::minutes(CLAIM_TTL_MINUTES)
        );
        assert!(
            returned_claim_expiry
                <= database_after_claim + chrono::Duration::minutes(CLAIM_TTL_MINUTES)
        );
        let stored_claim: (DateTime<Utc>, bool, String, i32) = sqlx::query_as(
            r#"
            SELECT claim_expires_at,
                   claim_expires_at - locked_at = make_interval(mins => $2),
                   status, attempts
            FROM qintopia_agent_os.work_items
            WHERE id = $1
            "#,
        )
        .bind(claimable_work_item_id)
        .bind(CLAIM_TTL_MINUTES as i32)
        .fetch_one(&pool)
        .await
        .expect("load database-owned claim lease");
        assert_eq!(stored_claim.0, returned_claim_expiry);
        assert!(
            stored_claim.1,
            "PostgreSQL must own the exact lease interval"
        );
        assert_eq!(stored_claim.2, "processing");
        assert_eq!(stored_claim.3, 1);
        finish(
            &pool,
            claimable_work_item_id,
            claimed["claim_token"].as_str().expect("claim token"),
            RunnerResult::Failed {
                failure_code: "runner_failed".to_string(),
            },
        )
        .await
        .expect("finish database-clock claim");
        let claimable_parent_event_count: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM qintopia_agent_os.work_item_events
            WHERE work_item_id = $1 AND event_type = 'space_agent_turn_child_failed'
              AND data->>'child_work_item_id' = $2
            "#,
        )
        .bind(claimable_parent_work_item_id)
        .bind(claimable_work_item_id.to_string())
        .fetch_one(&pool)
        .await
        .expect("count live finish parent event");
        assert_eq!(claimable_parent_event_count, 1);

        sqlx::query(
            "UPDATE qintopia_agent_os.capabilities SET enabled = true WHERE capability_key = $1",
        )
        .bind(SUBJECT_IDENTITY_CAPABILITY_KEY)
        .execute(&pool)
        .await
        .expect("enable bounded subject identity capability in disposable database");
        let subject_user_id = "9007199254740993";
        let other_chat_id = format!("space-agent-turn-other-{suffix}");
        sqlx::query(
            r#"
            INSERT INTO qintopia_identity.channel_identities
                (platform, channel_user_id, chat_id, display_name, identity_source, metadata)
            VALUES
                ('qiwe', $1, $2, 'Current Space Member', 'test_fixture',
                 '{"current_qiwe_room_member":true}'::jsonb),
                ('qiwe', $1, $3, 'Other Space Member', 'test_fixture',
                 '{"current_qiwe_room_member":true}'::jsonb)
            "#,
        )
        .bind(subject_user_id)
        .bind(&space_chat_id)
        .bind(&other_chat_id)
        .execute(&pool)
        .await
        .expect("seed cross-Space identity fixtures");
        let mut identity_tx = pool.begin().await.expect("begin identity lookup fixture");
        let identity_turn = LiveTurn {
            work_item_id: Uuid::new_v4(),
            parent_work_item_id: Uuid::new_v4(),
            space_id,
            conversation_chat_id: space_chat_id.clone(),
            goal: "Resolve the current trigger subject".to_string(),
            trigger: json!({
                "kind": "event",
                "event_type": "group_member_add",
                "provider_event_ref": format!("qiwe:{suffix}"),
                "subject_user_ids": [subject_user_id],
                "occurred_at": "2026-08-15T00:00:00Z"
            }),
            output_contract: output_contract(),
            output_contract_sha256: "a".repeat(64),
            capabilities: vec![],
        };
        let identity_output = execute_subject_identity_lookup(
            &mut identity_tx,
            &identity_turn,
            &json!({"scope": "trigger_subjects"}),
        )
        .await
        .expect("resolve only the exact current-Space trigger subject");
        identity_tx
            .rollback()
            .await
            .expect("rollback identity lookup transaction");
        assert_eq!(identity_output["members"].as_array().map(Vec::len), Some(1));
        assert_eq!(identity_output["members"][0]["user_id"], subject_user_id);
        assert_eq!(
            identity_output["members"][0]["display_name"],
            "Current Space Member"
        );

        let (lookup_work_item_id, _) = seed_claimable_turn(
            &pool,
            space_id,
            created_by_person_id,
            &format!("lookup-{suffix}"),
            &[SUBJECT_IDENTITY_CAPABILITY_KEY],
        )
        .await;
        let lookup_claim = claim(&pool).await.expect("claim bounded capability turn");
        assert_eq!(
            lookup_claim["work_item_id"].as_str(),
            Some(lookup_work_item_id.to_string().as_str())
        );
        assert_eq!(
            lookup_claim["capabilities"][0]["capability_key"],
            SUBJECT_IDENTITY_CAPABILITY_KEY
        );
        let lookup_call_id = Uuid::new_v4();
        let first_lookup = invoke(
            &pool,
            lookup_work_item_id,
            lookup_claim["claim_token"]
                .as_str()
                .expect("lookup claim token"),
            lookup_call_id,
            SUBJECT_IDENTITY_CAPABILITY_KEY,
            json!({"scope": "trigger_subjects"}),
        )
        .await
        .expect("execute bounded capability once");
        assert_eq!(first_lookup["replayed"], false);
        assert_eq!(first_lookup["output"], json!({"members": []}));
        let replayed_lookup = invoke(
            &pool,
            lookup_work_item_id,
            lookup_claim["claim_token"]
                .as_str()
                .expect("lookup claim token"),
            lookup_call_id,
            SUBJECT_IDENTITY_CAPABILITY_KEY,
            json!({"scope": "trigger_subjects"}),
        )
        .await
        .expect("replay the same bounded capability call");
        assert_eq!(replayed_lookup["replayed"], true);
        assert_eq!(replayed_lookup["output"], first_lookup["output"]);
        let receipt_count: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*)
            FROM qintopia_agent_os.work_items
            WHERE parent_work_item_id = $1
              AND work_item_type = $2
            "#,
        )
        .bind(lookup_work_item_id)
        .bind(CAPABILITY_CALL_WORK_ITEM_TYPE)
        .fetch_one(&pool)
        .await
        .expect("count idempotent capability receipts");
        assert_eq!(receipt_count, 1);
        let forged_finish = finish(
            &pool,
            lookup_work_item_id,
            lookup_claim["claim_token"]
                .as_str()
                .expect("lookup claim token"),
            RunnerResult::Succeeded {
                output: json!({"summary": "runner omitted its persisted capability usage"}),
                capability_usage: vec![],
            },
        )
        .await
        .expect("terminalize forged capability usage");
        assert_eq!(forged_finish["status"], "failed");
        let forged_failure: (String, Option<String>) = sqlx::query_as(
            "SELECT status, last_error FROM qintopia_agent_os.work_items WHERE id = $1",
        )
        .bind(lookup_work_item_id)
        .fetch_one(&pool)
        .await
        .expect("load forged capability usage result");
        assert_eq!(
            forged_failure,
            (
                "failed".to_string(),
                Some("runner_result_contract_invalid".to_string())
            )
        );

        let finish_token = "finish-token-with-at-least-thirty-two-bytes-000000000001";
        let finish_work_item_id = seed_expiring_claim(
            &pool,
            space_id,
            parent_work_item_id,
            &format!("finish-{suffix}"),
            finish_token,
            Utc::now() - chrono::Duration::minutes(1),
        )
        .await;
        let wrong_token = "wrong-token-with-at-least-thirty-two-bytes-000000000002";
        finish(
            &pool,
            finish_work_item_id,
            wrong_token,
            RunnerResult::Failed {
                failure_code: "runner_failed".to_string(),
            },
        )
        .await
        .expect_err("wrong token must not terminalize an expired claim");
        let untouched: (String, Option<String>, bool, bool, bool, Value) = sqlx::query_as(
            r#"
            SELECT status, last_error, claimed_by IS NOT NULL, locked_at IS NOT NULL,
                   claim_expires_at IS NOT NULL, metadata
            FROM qintopia_agent_os.work_items WHERE id = $1
            "#,
        )
        .bind(finish_work_item_id)
        .fetch_one(&pool)
        .await
        .expect("load wrong-token claim state");
        assert_eq!(untouched.0, "processing");
        assert_eq!(untouched.1, None);
        assert!(untouched.2 && untouched.3 && untouched.4);
        assert!(untouched.5.get("space_agent_turn_claim").is_some());
        let premature_events: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM qintopia_agent_os.work_item_events
            WHERE (work_item_id = $1 AND event_type = 'space_agent_turn_claim_expired')
               OR (work_item_id = $2 AND event_type = 'space_agent_turn_child_failed')
            "#,
        )
        .bind(finish_work_item_id)
        .bind(parent_work_item_id)
        .fetch_one(&pool)
        .await
        .expect("count premature expiry events");
        assert_eq!(premature_events, 0);

        let expired = finish(
            &pool,
            finish_work_item_id,
            finish_token,
            RunnerResult::Succeeded {
                output: json!({"summary": "late result must be ignored"}),
                capability_usage: vec![],
            },
        )
        .await
        .expect("correct token must terminalize an expired claim");
        assert_eq!(expired["accepted"], true);
        assert_eq!(expired["status"], "failed");
        assert_eq!(expired["failure_code"], "runner_claim_expired_unknown");
        let terminal: (String, Option<String>, bool, bool, bool, i32, Value) = sqlx::query_as(
            r#"
                SELECT status, last_error, claimed_by IS NULL, locked_at IS NULL,
                       claim_expires_at IS NULL, attempts, metadata
                FROM qintopia_agent_os.work_items WHERE id = $1
                "#,
        )
        .bind(finish_work_item_id)
        .fetch_one(&pool)
        .await
        .expect("load terminal expired claim");
        assert_eq!(terminal.0, "failed");
        assert_eq!(terminal.1.as_deref(), Some("runner_claim_expired_unknown"));
        assert!(terminal.2 && terminal.3 && terminal.4);
        assert_eq!(terminal.5, 1);
        assert!(terminal.6.get("space_agent_turn_claim").is_none());
        assert_eq!(
            terminal.6["space_agent_turn_result"]["failure_code"],
            "runner_claim_expired_unknown"
        );
        let artifact_count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM qintopia_agent_os.artifacts WHERE work_item_id = $1",
        )
        .bind(finish_work_item_id)
        .fetch_one(&pool)
        .await
        .expect("count late result artifacts");
        assert_eq!(artifact_count, 0);
        let child_event_count: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM qintopia_agent_os.work_item_events
            WHERE work_item_id = $1 AND event_type = 'space_agent_turn_claim_expired'
              AND data->'external_send_executed' = 'null'::jsonb
              AND data->>'automatic_retry_allowed' = 'false'
            "#,
        )
        .bind(finish_work_item_id)
        .fetch_one(&pool)
        .await
        .expect("count exact child expiry events");
        assert_eq!(child_event_count, 1);
        let parent_event_count: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM qintopia_agent_os.work_item_events
            WHERE work_item_id = $1 AND event_type = 'space_agent_turn_child_failed'
              AND data->>'child_work_item_id' = $2
              AND data->'external_send_executed' = 'null'::jsonb
              AND data->>'automatic_retry_allowed' = 'false'
            "#,
        )
        .bind(parent_work_item_id)
        .bind(finish_work_item_id.to_string())
        .fetch_one(&pool)
        .await
        .expect("count exact parent expiry events");
        assert_eq!(parent_event_count, 1);
        finish(
            &pool,
            finish_work_item_id,
            finish_token,
            RunnerResult::Failed {
                failure_code: "runner_failed".to_string(),
            },
        )
        .await
        .expect_err("a terminal claim must reject duplicate finish");

        let periodic_token = "periodic-token-with-at-least-thirty-two-bytes-0000000003";
        let periodic_work_item_id = seed_expiring_claim(
            &pool,
            space_id,
            parent_work_item_id,
            &format!("periodic-{suffix}"),
            periodic_token,
            Utc::now() - chrono::Duration::minutes(1),
        )
        .await;
        let live_token = "live-token-with-at-least-thirty-two-bytes-000000000000004";
        let live_work_item_id = seed_expiring_claim(
            &pool,
            space_id,
            parent_work_item_id,
            &format!("live-{suffix}"),
            live_token,
            Utc::now() + chrono::Duration::hours(1),
        )
        .await;
        assert_eq!(
            reconcile_all_expired_claims(&pool)
                .await
                .expect("periodically reconcile expired claims"),
            1
        );
        assert_eq!(
            reconcile_all_expired_claims(&pool)
                .await
                .expect("repeat expiry reconciliation idempotently"),
            0
        );
        let periodic_state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, last_error FROM qintopia_agent_os.work_items WHERE id = $1",
        )
        .bind(periodic_work_item_id)
        .fetch_one(&pool)
        .await
        .expect("load periodically terminalized claim");
        assert_eq!(periodic_state.0, "failed");
        assert_eq!(
            periodic_state.1.as_deref(),
            Some("runner_claim_expired_unknown")
        );
        let live_state: (String, Option<String>) = sqlx::query_as(
            "SELECT status, last_error FROM qintopia_agent_os.work_items WHERE id = $1",
        )
        .bind(live_work_item_id)
        .fetch_one(&pool)
        .await
        .expect("load unexpired claim");
        assert_eq!(live_state, ("processing".to_string(), None));
        let periodic_child_events: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM qintopia_agent_os.work_item_events
            WHERE work_item_id = $1 AND event_type = 'space_agent_turn_claim_expired'
            "#,
        )
        .bind(periodic_work_item_id)
        .fetch_one(&pool)
        .await
        .expect("count periodic child expiry events");
        assert_eq!(periodic_child_events, 1);
        let duplicate_parent_events: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM qintopia_agent_os.work_item_events
            WHERE work_item_id = $1 AND event_type = 'space_agent_turn_child_failed'
              AND data->>'child_work_item_id' IN ($2, $3)
            "#,
        )
        .bind(parent_work_item_id)
        .bind(finish_work_item_id.to_string())
        .bind(periodic_work_item_id.to_string())
        .fetch_one(&pool)
        .await
        .expect("count parent expiry events without duplicates");
        assert_eq!(duplicate_parent_events, 2);
    }
}
