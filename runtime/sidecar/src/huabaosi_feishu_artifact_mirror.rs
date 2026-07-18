use std::{collections::BTreeSet, env, path::Path};

use anyhow::{bail, Context, Result};
use chrono::{DateTime, Utc};
use image::GenericImageView;
use md5::Md5;
use serde::Serialize;
use serde_json::{json, Map, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use url::Url;
use uuid::Uuid;

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
use std::fs;
#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
use zeroize::{Zeroize, Zeroizing};

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
use crate::bounded_http::HttpClient;
use crate::{config::Cli, db, url_policy};

const WORKER_ID: &str = "huabaosi-feishu-artifact-mirror-worker";
const PROVIDER: &str = "feishu_base_huabaosi_generated_image";
const SCHEMA_VERSION: &str = "huabaosi-generated-image-v1";
const REQUIRED_APPROVAL: &str = "approved-huabaosi-feishu-artifact-mirror";
const REQUIRED_MIME_TYPE: &str = "image/jpeg";
const REQUIRED_SOURCE_MIME_TYPE: &str = "image/png";
const REQUIRED_TRANSFORM: &str = "png_to_jpeg_white_background_q92_v1";
const REQUIRED_GENERATOR: &str = "huabaosi-image-generation-worker";
const REQUIRED_PROVIDER: &str = "openai-compatible";
const REQUIRED_MODEL: &str = "gpt-image-2";
const REQUIRED_JPEG_QUALITY: i64 = 92;
const REQUIRED_ALPHA_BACKGROUND: &str = "#ffffff";
const REQUIRED_WIDTH: i64 = 1024;
const REQUIRED_HEIGHT: i64 = 1024;
const DEFAULT_MAX_MEDIA_BYTES: usize = 10 * 1024 * 1024;
const MAX_FEISHU_RESPONSE_BYTES: usize = 1024 * 1024;
const OFFICIAL_FEISHU_API_ROOT: &str = "https://open.feishu.cn/open-apis/";

const ENABLE_ENV: &str = "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED";
const APPROVAL_ENV: &str = "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL";
const PRODUCTION_RELEASE_SHA_ENV: &str = "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA";
const DEPLOYED_COMMIT_SHA_ENV: &str = "QINTOPIA_DEPLOYED_COMMIT_SHA";
const DATABASE_HASH_ENV: &str = "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256";
const BASE_TOKEN_ENV: &str = "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN";
const BASE_TOKEN_ALLOWLIST_ENV: &str = "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS";
const TABLE_ID_ENV: &str = "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID";
const TABLE_ID_ALLOWLIST_ENV: &str = "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS";
const PROFILE_ENV_PATH_ENV: &str = "QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH";
const SCHEMA_VERSION_ENV: &str = "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION";
const MEDIA_HOSTS_ENV: &str = "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS";
const MEDIA_MAX_BYTES_ENV: &str = "QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES";

const REQUIRED_CONFIGURATION_NAMES: &[&str] = &[
    "QINTOPIA_SIDECAR_DATABASE_URL",
    APPROVAL_ENV,
    PRODUCTION_RELEASE_SHA_ENV,
    DEPLOYED_COMMIT_SHA_ENV,
    DATABASE_HASH_ENV,
    BASE_TOKEN_ENV,
    BASE_TOKEN_ALLOWLIST_ENV,
    TABLE_ID_ENV,
    TABLE_ID_ALLOWLIST_ENV,
    PROFILE_ENV_PATH_ENV,
    SCHEMA_VERSION_ENV,
    MEDIA_HOSTS_ENV,
];

#[derive(Debug, Serialize)]
pub struct MirrorReport {
    success: bool,
    dry_run: bool,
    apply_requested: bool,
    fixture_mode: bool,
    worker: &'static str,
    action_status: String,
    artifact_id: Option<Uuid>,
    work_item_id: Option<Uuid>,
    workflow_root_id: Option<Uuid>,
    review_status: Option<String>,
    schema_version: &'static str,
    external_calls_executed: bool,
    database_writes_executed: bool,
    sensitive_fields_redacted: bool,
    guardrails: Vec<String>,
}

#[derive(Debug, Serialize)]
struct MirrorPreflightReport {
    success: bool,
    worker: &'static str,
    action_status: &'static str,
    adapter_compiled: bool,
    mirror_enabled: bool,
    config_valid: bool,
    schema_version: &'static str,
    media_allowed_host_count: usize,
    missing_configuration: Vec<&'static str>,
    external_calls_executed: bool,
    database_writes_executed: bool,
    sensitive_fields_redacted: bool,
}

#[derive(Debug, Serialize)]
struct PrimaryStorageRevalidationReport {
    success: bool,
    worker: &'static str,
    action_status: String,
    artifact_id: Uuid,
    work_item_id: Uuid,
    schema_version: &'static str,
    content_hash: String,
    byte_size: usize,
    width: u32,
    height: u32,
    external_calls_executed: bool,
    database_writes_executed: bool,
    sensitive_fields_redacted: bool,
    guardrails: Vec<String>,
}

#[derive(Debug, Clone)]
struct MirrorArtifact {
    id: Uuid,
    work_item_id: Uuid,
    review_status: String,
    title: String,
    artifact_uri: String,
    content_hash: String,
    source_ids: Value,
    metadata: Value,
    creation_event_data: Value,
    reviewed_at: Option<DateTime<Utc>>,
    reviewed_by: Option<String>,
    review_decision_reason: Option<String>,
    created_at: DateTime<Utc>,
    updated_at: DateTime<Utc>,
    last_synced_at: Option<DateTime<Utc>>,
    workbench_status: Option<String>,
}

#[derive(Debug, Clone)]
struct ValidatedArtifact {
    artifact_uri: Url,
    file_md5: String,
    source_content_hash: String,
    byte_size: usize,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone)]
struct ValidatedPrimaryStorageArtifact {
    file_md5: String,
    source_content_hash: String,
    byte_size: usize,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone)]
struct MirrorPolicy {
    media_allowed_hosts: BTreeSet<String>,
    max_media_bytes: usize,
    #[cfg(test)]
    allow_insecure_http: bool,
}

#[derive(Debug, Clone)]
struct MirrorConfig {
    policy: MirrorPolicy,
    base_token: String,
    table_id: String,
    profile_env_path: String,
    api_root: Url,
}

#[derive(Debug, Clone)]
#[cfg_attr(
    not(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    )),
    allow(dead_code)
)]
pub(crate) struct FeishuPrimaryStorageConfig {
    base_token: String,
    table_id: String,
    profile_env_path: String,
    api_root: Url,
    max_media_bytes: usize,
}

#[cfg_attr(
    not(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    )),
    allow(dead_code)
)]
pub(crate) struct FeishuPrimaryStorageImage<'a> {
    pub artifact_id: Uuid,
    pub workflow_root_id: Uuid,
    pub work_item_id: Uuid,
    pub content_hash: &'a str,
    pub file_md5: &'a str,
    pub source_content_hash: &'a str,
    pub bytes: &'a [u8],
    pub width: u32,
    pub height: u32,
}

pub(crate) struct FeishuPrimaryStorageResult {
    pub artifact_uri: String,
    pub record_id: String,
}

#[derive(Debug)]
struct MirrorFailure {
    stage: &'static str,
    code: &'static str,
    external_write_executed: Option<bool>,
    automatic_retry_allowed: bool,
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
#[derive(Debug)]
struct FeishuCredentials {
    app_id: Zeroizing<String>,
    app_secret: Zeroizing<String>,
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
#[derive(Debug)]
struct FeishuClient {
    tenant_token: Zeroizing<String>,
    http: HttpClient,
    api_root: Url,
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
#[derive(Debug)]
struct FeishuRecord {
    record_id: String,
    fields: Value,
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
#[derive(Serialize)]
struct FeishuAuthRequest<'a> {
    app_id: &'a str,
    app_secret: &'a str,
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
#[derive(serde::Deserialize)]
struct FeishuAuthResponse {
    code: i64,
    #[serde(default)]
    tenant_access_token: String,
}

pub async fn run(
    cli: &Cli,
    once: bool,
    artifact_id: Option<Uuid>,
    apply: bool,
    dry_run: bool,
    fixture_mode: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    if !once {
        bail!("Huabaosi Feishu artifact mirror currently supports --once only");
    }
    if fixture_mode {
        if apply {
            bail!("fixture-mode cannot be used with --apply");
        }
        println!("{}", serde_json::to_string_pretty(&fixture_report())?);
        return Ok(());
    }

    let apply_requested = apply && !dry_run;
    if apply_requested && !cfg!(feature = "huabaosi-feishu-mirror-adapter") {
        bail!("Huabaosi Feishu mirror adapter is not compiled into this binary");
    }

    let database_url = cli.database_url_required()?;
    let policy = MirrorPolicy::from_env()?;
    let config = if apply_requested {
        Some(MirrorConfig::from_env(database_url, policy.clone())?)
    } else {
        None
    };
    let pool = db::connect(database_url, cli.db_max_connections).await?;
    let report = if apply_requested {
        run_apply(
            &pool,
            artifact_id,
            config.context("mirror config is required")?,
        )
        .await?
    } else {
        run_preview(&pool, artifact_id, &policy).await?
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

pub fn run_preflight() -> Result<()> {
    let adapter_compiled = cfg!(feature = "huabaosi-feishu-mirror-adapter");
    let mirror_enabled = env_flag(ENABLE_ENV);
    let missing_configuration = missing_configuration();
    let config_valid = missing_configuration.is_empty() && validate_local_configuration().is_ok();
    let media_allowed_host_count = env::var(MEDIA_HOSTS_ENV)
        .ok()
        .and_then(|value| parse_allowed_hosts(&value).ok())
        .map(|hosts| hosts.len())
        .unwrap_or_default();
    let (success, action_status) = if !adapter_compiled {
        (false, "adapter_not_compiled")
    } else if !mirror_enabled {
        (false, "mirror_disabled")
    } else if !config_valid {
        (false, "adapter_not_configured")
    } else {
        (true, "adapter_config_ready")
    };
    let report = MirrorPreflightReport {
        success,
        worker: WORKER_ID,
        action_status,
        adapter_compiled,
        mirror_enabled,
        config_valid,
        schema_version: SCHEMA_VERSION,
        media_allowed_host_count,
        missing_configuration,
        external_calls_executed: false,
        database_writes_executed: false,
        sensitive_fields_redacted: true,
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.success {
        Ok(())
    } else {
        bail!("Huabaosi Feishu mirror preflight configuration is invalid")
    }
}

pub fn run_observation_preflight() -> Result<()> {
    let report = observation_preflight_report();
    println!("{}", serde_json::to_string_pretty(&report)?);
    if report.success {
        Ok(())
    } else {
        bail!("Huabaosi Feishu mirror observation boundary is invalid")
    }
}

pub async fn run_primary_storage_revalidation(cli: &Cli, artifact_id: Uuid) -> Result<()> {
    #[cfg(not(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    )))]
    {
        let _ = (cli, artifact_id);
        bail!("Huabaosi Feishu primary-storage revalidation adapter is not compiled");
    }

    #[cfg(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    ))]
    {
        let database_url = cli.database_url_required()?;
        let config = FeishuPrimaryStorageConfig::from_env(database_url)?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        let artifact = peek_candidate(&pool, Some(artifact_id))
            .await?
            .context("Huabaosi generated image artifact was not found")?;
        let validated = validate_primary_storage_artifact(&artifact)?;
        let report = revalidate_primary_storage_artifact(&artifact, &validated, &config)
            .map_err(primary_storage_error)?;
        println!("{}", serde_json::to_string_pretty(&report)?);
        Ok(())
    }
}

fn observation_preflight_report() -> MirrorPreflightReport {
    let adapter_compiled = cfg!(feature = "huabaosi-feishu-mirror-adapter");
    let mirror_enabled = env_flag(ENABLE_ENV);
    let deployed_sha_valid = required_env(DEPLOYED_COMMIT_SHA_ENV)
        .map(|sha| is_lower_hex(&sha, 40))
        .unwrap_or(false);
    let (success, action_status) = if !adapter_compiled {
        (false, "adapter_not_compiled")
    } else if !deployed_sha_valid {
        (false, "invalid_deployed_release_sha")
    } else if mirror_enabled {
        (true, "observation_enabled_boundary_ready")
    } else {
        (true, "observation_disabled_boundary_ready")
    };
    MirrorPreflightReport {
        success,
        worker: WORKER_ID,
        action_status,
        adapter_compiled,
        mirror_enabled,
        config_valid: false,
        schema_version: SCHEMA_VERSION,
        media_allowed_host_count: 0,
        missing_configuration: Vec::new(),
        external_calls_executed: false,
        database_writes_executed: false,
        sensitive_fields_redacted: true,
    }
}

fn fixture_report() -> MirrorReport {
    MirrorReport {
        success: true,
        dry_run: true,
        apply_requested: false,
        fixture_mode: true,
        worker: WORKER_ID,
        action_status: "fixture_mirror_preview".to_string(),
        artifact_id: Some(Uuid::nil()),
        work_item_id: Some(Uuid::nil()),
        workflow_root_id: Some(Uuid::nil()),
        review_status: Some("pending".to_string()),
        schema_version: SCHEMA_VERSION,
        external_calls_executed: false,
        database_writes_executed: false,
        sensitive_fields_redacted: true,
        guardrails: mirror_guardrails(),
    }
}

fn empty_report(apply_requested: bool, action_status: &str) -> MirrorReport {
    MirrorReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        fixture_mode: false,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        artifact_id: None,
        work_item_id: None,
        workflow_root_id: None,
        review_status: None,
        schema_version: SCHEMA_VERSION,
        external_calls_executed: false,
        database_writes_executed: false,
        sensitive_fields_redacted: true,
        guardrails: mirror_guardrails(),
    }
}

fn artifact_report(
    artifact: &MirrorArtifact,
    workflow_root_id: Uuid,
    apply_requested: bool,
    action_status: &str,
    external_calls_executed: bool,
    database_writes_executed: bool,
) -> MirrorReport {
    MirrorReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        fixture_mode: false,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        artifact_id: Some(artifact.id),
        work_item_id: Some(artifact.work_item_id),
        workflow_root_id: Some(workflow_root_id),
        review_status: Some(artifact.review_status.clone()),
        schema_version: SCHEMA_VERSION,
        external_calls_executed,
        database_writes_executed,
        sensitive_fields_redacted: true,
        guardrails: mirror_guardrails(),
    }
}

fn mirror_guardrails() -> Vec<String> {
    vec![
        "Postgres remains the system fact source".to_string(),
        "only the fixed Huabaosi generated-image table schema is writable".to_string(),
        "the exact immutable final JPEG is revalidated before Feishu upload".to_string(),
        "mirror writes cannot approve, publish, or send an artifact".to_string(),
        "Base tokens, table ids, file tokens, credentials, and raw responses are redacted"
            .to_string(),
        "the production timer requires separate explicit owner activation".to_string(),
    ]
}

impl MirrorPolicy {
    fn from_env() -> Result<Self> {
        let media_allowed_hosts = parse_allowed_hosts(&required_env(MEDIA_HOSTS_ENV)?)?;
        let max_media_bytes = env::var(MEDIA_MAX_BYTES_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                value
                    .parse::<usize>()
                    .context("Huabaosi Feishu mirror media byte limit must be numeric")
            })
            .transpose()?
            .unwrap_or(DEFAULT_MAX_MEDIA_BYTES);
        if max_media_bytes == 0 || max_media_bytes > DEFAULT_MAX_MEDIA_BYTES {
            bail!("Huabaosi Feishu mirror media byte limit is outside the reviewed bound");
        }
        Ok(Self {
            media_allowed_hosts,
            max_media_bytes,
            #[cfg(test)]
            allow_insecure_http: false,
        })
    }

    #[cfg(test)]
    fn test_only(host: &str, max_media_bytes: usize) -> Self {
        Self {
            media_allowed_hosts: BTreeSet::from([normalize_host(host)]),
            max_media_bytes,
            allow_insecure_http: true,
        }
    }
}

impl MirrorConfig {
    fn from_env(database_url: &str, policy: MirrorPolicy) -> Result<Self> {
        if !env_flag(ENABLE_ENV) {
            bail!("Huabaosi Feishu mirror is disabled");
        }
        if env::var(APPROVAL_ENV).ok().as_deref() != Some(REQUIRED_APPROVAL) {
            bail!("Huabaosi Feishu mirror owner approval is invalid");
        }
        validate_release_binding(
            &required_env(PRODUCTION_RELEASE_SHA_ENV)?,
            &required_env(DEPLOYED_COMMIT_SHA_ENV)?,
        )?;
        validate_database_hash(database_url, &required_env(DATABASE_HASH_ENV)?)?;

        let base_token = required_env(BASE_TOKEN_ENV)?;
        validate_external_identifier(&base_token, "Feishu Base token")?;
        let allowed_base_tokens = parse_exact_allowlist(&required_env(BASE_TOKEN_ALLOWLIST_ENV)?)?;
        if !allowed_base_tokens.contains(&base_token) {
            bail!("Feishu Base token is not explicitly allowlisted");
        }

        let table_id = required_env(TABLE_ID_ENV)?;
        validate_external_identifier(&table_id, "Feishu artifact table id")?;
        let allowed_table_ids = parse_exact_allowlist(&required_env(TABLE_ID_ALLOWLIST_ENV)?)?;
        if !allowed_table_ids.contains(&table_id) {
            bail!("Feishu artifact table id is not explicitly allowlisted");
        }

        let profile_env_path = required_env(PROFILE_ENV_PATH_ENV)?;
        validate_profile_env_path(&profile_env_path)?;
        if required_env(SCHEMA_VERSION_ENV)? != SCHEMA_VERSION {
            bail!("Huabaosi Feishu mirror schema version is not reviewed");
        }

        Ok(Self {
            policy,
            base_token,
            table_id,
            profile_env_path,
            api_root: Url::parse(OFFICIAL_FEISHU_API_ROOT)
                .context("parse fixed Feishu API root")?,
        })
    }

    #[cfg(test)]
    fn test_only(policy: MirrorPolicy, api_root: Url, profile_env_path: String) -> Self {
        Self {
            policy,
            base_token: "baseTokenFixture".to_string(),
            table_id: "tblFixture".to_string(),
            profile_env_path,
            api_root,
        }
    }
}

impl FeishuPrimaryStorageConfig {
    pub(crate) fn from_env(database_url: &str) -> Result<Self> {
        if !env_flag(ENABLE_ENV) {
            bail!("Huabaosi Feishu storage is disabled");
        }
        if env::var(APPROVAL_ENV).ok().as_deref() != Some(REQUIRED_APPROVAL) {
            bail!("Huabaosi Feishu storage owner approval is invalid");
        }
        validate_release_binding(
            &required_env(PRODUCTION_RELEASE_SHA_ENV)?,
            &required_env(DEPLOYED_COMMIT_SHA_ENV)?,
        )?;
        validate_database_hash(database_url, &required_env(DATABASE_HASH_ENV)?)?;

        let base_token = required_env(BASE_TOKEN_ENV)?;
        validate_external_identifier(&base_token, "Feishu Base token")?;
        if !parse_exact_allowlist(&required_env(BASE_TOKEN_ALLOWLIST_ENV)?)?.contains(&base_token) {
            bail!("Feishu Base token is not explicitly allowlisted");
        }

        let table_id = required_env(TABLE_ID_ENV)?;
        validate_external_identifier(&table_id, "Feishu artifact table id")?;
        if !parse_exact_allowlist(&required_env(TABLE_ID_ALLOWLIST_ENV)?)?.contains(&table_id) {
            bail!("Feishu artifact table id is not explicitly allowlisted");
        }

        let profile_env_path = required_env(PROFILE_ENV_PATH_ENV)?;
        validate_profile_env_path(&profile_env_path)?;
        if required_env(SCHEMA_VERSION_ENV)? != SCHEMA_VERSION {
            bail!("Huabaosi Feishu storage schema version is not reviewed");
        }
        let max_media_bytes = env::var(MEDIA_MAX_BYTES_ENV)
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| {
                value
                    .parse::<usize>()
                    .context("Huabaosi Feishu storage media byte limit must be numeric")
            })
            .transpose()?
            .unwrap_or(DEFAULT_MAX_MEDIA_BYTES);
        if max_media_bytes == 0 || max_media_bytes > DEFAULT_MAX_MEDIA_BYTES {
            bail!("Huabaosi Feishu storage media byte limit is outside the reviewed bound");
        }

        Ok(Self {
            base_token,
            table_id,
            profile_env_path,
            api_root: Url::parse(OFFICIAL_FEISHU_API_ROOT)
                .context("parse fixed Feishu API root")?,
            max_media_bytes,
        })
    }

    pub(crate) fn max_media_bytes(&self) -> usize {
        self.max_media_bytes
    }

    #[cfg(test)]
    pub(crate) fn test_only(
        api_root: Url,
        profile_env_path: String,
        max_media_bytes: usize,
    ) -> Self {
        Self {
            base_token: "baseTokenFixture".to_string(),
            table_id: "tblFixture".to_string(),
            profile_env_path,
            api_root,
            max_media_bytes,
        }
    }
}

pub(crate) fn primary_storage_missing_configuration() -> Vec<&'static str> {
    [
        ENABLE_ENV,
        APPROVAL_ENV,
        PRODUCTION_RELEASE_SHA_ENV,
        DEPLOYED_COMMIT_SHA_ENV,
        DATABASE_HASH_ENV,
        BASE_TOKEN_ENV,
        BASE_TOKEN_ALLOWLIST_ENV,
        TABLE_ID_ENV,
        TABLE_ID_ALLOWLIST_ENV,
        PROFILE_ENV_PATH_ENV,
        SCHEMA_VERSION_ENV,
    ]
    .into_iter()
    .filter(|name| {
        env::var(name)
            .ok()
            .map(|value| is_missing_or_placeholder(&value))
            .unwrap_or(true)
    })
    .collect()
}

fn validate_local_configuration() -> Result<()> {
    if env::var(APPROVAL_ENV).ok().as_deref() != Some(REQUIRED_APPROVAL) {
        bail!("owner approval is invalid");
    }
    validate_release_binding(
        &required_env(PRODUCTION_RELEASE_SHA_ENV)?,
        &required_env(DEPLOYED_COMMIT_SHA_ENV)?,
    )?;
    let database_url = required_env("QINTOPIA_SIDECAR_DATABASE_URL")?;
    validate_database_hash(&database_url, &required_env(DATABASE_HASH_ENV)?)?;
    let base_token = required_env(BASE_TOKEN_ENV)?;
    validate_external_identifier(&base_token, "Feishu Base token")?;
    if !parse_exact_allowlist(&required_env(BASE_TOKEN_ALLOWLIST_ENV)?)?.contains(&base_token) {
        bail!("Base token is not allowlisted");
    }
    let table_id = required_env(TABLE_ID_ENV)?;
    validate_external_identifier(&table_id, "Feishu artifact table id")?;
    if !parse_exact_allowlist(&required_env(TABLE_ID_ALLOWLIST_ENV)?)?.contains(&table_id) {
        bail!("table id is not allowlisted");
    }
    validate_profile_env_path(&required_env(PROFILE_ENV_PATH_ENV)?)?;
    if required_env(SCHEMA_VERSION_ENV)? != SCHEMA_VERSION {
        bail!("schema version is invalid");
    }
    MirrorPolicy::from_env()?;
    Ok(())
}

fn missing_configuration() -> Vec<&'static str> {
    REQUIRED_CONFIGURATION_NAMES
        .iter()
        .copied()
        .filter(|name| {
            env::var(name)
                .ok()
                .map(|value| is_missing_or_placeholder(&value))
                .unwrap_or(true)
        })
        .collect()
}

fn is_missing_or_placeholder(value: &str) -> bool {
    let value = value.trim().to_ascii_lowercase();
    value.is_empty()
        || value.contains("replace-with")
        || value.contains("changeme")
        || value.contains("change-me")
}

fn env_flag(name: &str) -> bool {
    env::var(name)
        .ok()
        .map(|value| value.trim() == "1")
        .unwrap_or(false)
}

fn required_env(name: &str) -> Result<String> {
    env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !is_missing_or_placeholder(value))
        .with_context(|| format!("{name} is required"))
}

fn parse_allowed_hosts(value: &str) -> Result<BTreeSet<String>> {
    let hosts = value
        .split(',')
        .map(normalize_host)
        .filter(|host| !host.is_empty())
        .collect::<BTreeSet<_>>();
    if hosts.is_empty() {
        bail!("Huabaosi media host allowlist must not be empty");
    }
    if hosts
        .iter()
        .any(|host| host.contains('/') || host.contains(':'))
    {
        bail!("Huabaosi media host allowlist contains an invalid host");
    }
    Ok(hosts)
}

fn parse_exact_allowlist(value: &str) -> Result<BTreeSet<String>> {
    let values = value
        .split(',')
        .map(str::trim)
        .filter(|item| !item.is_empty())
        .map(str::to_string)
        .collect::<BTreeSet<_>>();
    if values.is_empty() {
        bail!("explicit allowlist must not be empty");
    }
    Ok(values)
}

fn normalize_host(value: &str) -> String {
    value.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn validate_external_identifier(value: &str, label: &str) -> Result<()> {
    if value.is_empty()
        || value.len() > 128
        || !value
            .bytes()
            .all(|byte| byte.is_ascii_alphanumeric() || matches!(byte, b'_' | b'-'))
    {
        bail!("{label} is invalid");
    }
    Ok(())
}

fn validate_profile_env_path(value: &str) -> Result<()> {
    let path = Path::new(value);
    if !path.is_absolute() || path.file_name().and_then(|name| name.to_str()) != Some(".env") {
        bail!("Huabaosi Feishu profile env path must be an absolute .env path");
    }
    let segments = path
        .components()
        .filter_map(|component| component.as_os_str().to_str())
        .collect::<Vec<_>>();
    if !segments.ends_with(&[".hermes", "profiles", "huabaosi", ".env"]) {
        bail!("Huabaosi Feishu profile env path must use .hermes/profiles/huabaosi/.env");
    }
    Ok(())
}

fn validate_database_hash(database_url: &str, expected: &str) -> Result<()> {
    if !is_lower_hex(expected, 64) {
        bail!("Huabaosi Feishu database URL SHA-256 must be canonical");
    }
    if sha256_hex(database_url.as_bytes()) != expected {
        bail!("Huabaosi Feishu database URL SHA-256 does not match");
    }
    Ok(())
}

fn validate_release_binding(expected: &str, deployed: &str) -> Result<()> {
    if !is_lower_hex(expected, 40) || !is_lower_hex(deployed, 40) {
        bail!("Huabaosi Feishu production release SHA must be canonical");
    }
    if expected != deployed {
        bail!("Huabaosi Feishu production release SHA does not match deployed commit");
    }
    Ok(())
}

fn validate_artifact(
    artifact: &MirrorArtifact,
    policy: &MirrorPolicy,
) -> Result<ValidatedArtifact> {
    if !matches!(
        artifact.review_status.as_str(),
        "pending" | "approved" | "rejected" | "changes_requested"
    ) {
        bail!("generated image review status is not mirrorable");
    }
    if !is_canonical_sha256(&artifact.content_hash) {
        bail!("generated image content hash must be canonical");
    }
    url_policy::reject_path_separator_ambiguity(&artifact.artifact_uri, "generated image URI")?;
    let artifact_uri =
        Url::parse(&artifact.artifact_uri).context("generated image URI must be a valid URL")?;
    if !policy.allows_url_scheme(artifact_uri.scheme())
        || !artifact_uri.username().is_empty()
        || artifact_uri.password().is_some()
        || artifact_uri.query().is_some()
        || artifact_uri.fragment().is_some()
    {
        bail!("generated image URI must be a stable HTTPS URL");
    }
    let host = normalize_host(
        artifact_uri
            .host_str()
            .context("generated image URI must include a host")?,
    );
    if !policy.media_allowed_hosts.contains(&host) {
        bail!("generated image URI host is not allowlisted");
    }
    let path = artifact_uri.path().to_ascii_lowercase();
    if !(path.ends_with(".jpg") || path.ends_with(".jpeg")) {
        bail!("generated image URI must reference a JPEG object");
    }

    let metadata = artifact
        .metadata
        .as_object()
        .context("generated image metadata must be an object")?;
    let mime_type = metadata_text(metadata, "mime_type")?;
    let generated_by = metadata_text(metadata, "generated_by")?;
    let provider = metadata_text(metadata, "provider")?;
    let model = metadata_text(metadata, "model")?;
    let file_md5 = metadata_text(metadata, "file_md5")?;
    let source_mime_type = metadata_text(metadata, "provider_source_mime_type")?;
    let source_content_hash = metadata_text(metadata, "provider_source_content_hash")?;
    let media_transform = metadata_text(metadata, "media_transform")?;
    let alpha_background = metadata_text(metadata, "alpha_background")?;
    let approved_brief_id = metadata_text(metadata, "approved_brief_artifact_id")?;
    let approved_brief_hash = metadata_text(metadata, "approved_brief_content_hash")?;
    let prompt_hash = metadata_text(metadata, "prompt_hash")?;
    let jpeg_quality = metadata_i64(metadata, "jpeg_quality")?;
    let width = metadata_i64(metadata, "width")?;
    let height = metadata_i64(metadata, "height")?;
    let byte_size = metadata_i64(metadata, "byte_size")?;
    if generated_by != REQUIRED_GENERATOR
        || provider != REQUIRED_PROVIDER
        || model != REQUIRED_MODEL
        || mime_type != REQUIRED_MIME_TYPE
        || source_mime_type != REQUIRED_SOURCE_MIME_TYPE
        || media_transform != REQUIRED_TRANSFORM
        || alpha_background != REQUIRED_ALPHA_BACKGROUND
        || jpeg_quality != REQUIRED_JPEG_QUALITY
    {
        bail!("generated image metadata does not match the reviewed JPEG contract");
    }
    if Uuid::parse_str(&approved_brief_id).is_err()
        || !is_canonical_sha256(&approved_brief_hash)
        || !is_canonical_sha256(&prompt_hash)
        || !is_lower_hex(&file_md5, 32)
        || !is_canonical_sha256(&source_content_hash)
    {
        bail!("generated image file identity metadata is not canonical");
    }
    validate_source_ids(artifact, &approved_brief_id, &approved_brief_hash)?;
    if width != REQUIRED_WIDTH || height != REQUIRED_HEIGHT {
        bail!("generated image dimensions do not match the reviewed JPEG contract");
    }
    let byte_size = usize::try_from(byte_size)
        .ok()
        .filter(|size| *size > 0 && *size <= policy.max_media_bytes)
        .context("generated image byte size is outside the reviewed bound")?;
    validate_creation_event(artifact, metadata)?;

    Ok(ValidatedArtifact {
        artifact_uri,
        file_md5,
        source_content_hash,
        byte_size,
        width: u32::try_from(width).context("generated image width is invalid")?,
        height: u32::try_from(height).context("generated image height is invalid")?,
    })
}

fn validate_source_ids(
    artifact: &MirrorArtifact,
    approved_brief_id: &str,
    approved_brief_hash: &str,
) -> Result<()> {
    let source = artifact
        .source_ids
        .as_array()
        .and_then(|items| (items.len() == 1).then(|| &items[0]))
        .and_then(Value::as_object)
        .context("generated image source ids must contain one approved brief")?;
    if source
        .get("approved_brief_artifact_id")
        .and_then(Value::as_str)
        != Some(approved_brief_id)
        || source
            .get("approved_brief_content_hash")
            .and_then(Value::as_str)
            != Some(approved_brief_hash)
    {
        bail!("generated image source ids do not match artifact provenance");
    }
    Ok(())
}

impl MirrorPolicy {
    fn allows_url_scheme(&self, scheme: &str) -> bool {
        if scheme == "https" {
            return true;
        }
        #[cfg(test)]
        if self.allow_insecure_http && scheme == "http" {
            return true;
        }
        false
    }
}

fn validate_creation_event(artifact: &MirrorArtifact, metadata: &Map<String, Value>) -> Result<()> {
    let event = artifact
        .creation_event_data
        .as_object()
        .context("generated image creation audit is missing")?;
    for key in [
        "mime_type",
        "file_md5",
        "provider_source_mime_type",
        "provider_source_content_hash",
        "media_transform",
        "jpeg_quality",
        "alpha_background",
        "width",
        "height",
        "byte_size",
    ] {
        if event.get(key) != metadata.get(key) {
            bail!("generated image creation audit does not match artifact metadata");
        }
    }
    if event.get("content_hash").and_then(Value::as_str) != Some(&artifact.content_hash) {
        bail!("generated image creation audit does not match artifact content hash");
    }
    if event
        .get("external_publish_executed")
        .and_then(Value::as_bool)
        != Some(false)
    {
        bail!("generated image creation audit does not prove unpublished state");
    }
    Ok(())
}

fn metadata_text(metadata: &Map<String, Value>, key: &str) -> Result<String> {
    metadata
        .get(key)
        .and_then(Value::as_str)
        .map(str::to_string)
        .with_context(|| format!("generated image metadata is missing {key}"))
}

fn metadata_i64(metadata: &Map<String, Value>, key: &str) -> Result<i64> {
    metadata
        .get(key)
        .and_then(Value::as_i64)
        .with_context(|| format!("generated image metadata is missing {key}"))
}

fn build_feishu_fields(
    artifact: &MirrorArtifact,
    validated: &ValidatedArtifact,
    workflow_root_id: Uuid,
    attachment_token: Option<&str>,
) -> Value {
    let mut fields = Map::new();
    fields.insert("产物标题".to_string(), json!(artifact.title));
    fields.insert("AgentOS产物ID".to_string(), json!(artifact.id.to_string()));
    fields.insert(
        "AgentOS工作项ID".to_string(),
        json!(workflow_root_id.to_string()),
    );
    fields.insert(
        "图片请求ID".to_string(),
        json!(artifact.work_item_id.to_string()),
    );
    fields.insert(
        "稳定媒体URI".to_string(),
        json!({"link": artifact.artifact_uri, "text": "AgentOS最终JPEG"}),
    );
    fields.insert("JPEG SHA-256".to_string(), json!(artifact.content_hash));
    fields.insert("文件MD5".to_string(), json!(validated.file_md5));
    fields.insert("字节数".to_string(), json!(validated.byte_size));
    fields.insert("宽度".to_string(), json!(validated.width));
    fields.insert("高度".to_string(), json!(validated.height));
    fields.insert("MIME类型".to_string(), json!(REQUIRED_MIME_TYPE));
    fields.insert(
        "源PNG SHA-256".to_string(),
        json!(validated.source_content_hash),
    );
    fields.insert("转换规则".to_string(), json!(REQUIRED_TRANSFORM));
    fields.insert(
        "审核状态".to_string(),
        json!(review_status_display(&artifact.review_status)),
    );
    if let Some(reviewed_by) = artifact.reviewed_by.as_deref() {
        fields.insert("审核人".to_string(), json!(reviewed_by));
    }
    if let Some(reviewed_at) = artifact.reviewed_at {
        fields.insert(
            "审核时间".to_string(),
            json!(reviewed_at.timestamp_millis()),
        );
    }
    if let Some(reason) = artifact.review_decision_reason.as_deref() {
        fields.insert("审核意见".to_string(), json!(reason));
    }
    fields.insert(
        "生成时间".to_string(),
        json!(artifact.created_at.timestamp_millis()),
    );
    fields.insert(
        "更新时间".to_string(),
        json!(artifact.updated_at.timestamp_millis()),
    );
    if let Some(file_token) = attachment_token {
        fields.insert("最终JPEG".to_string(), json!([{"file_token": file_token}]));
    }
    Value::Object(fields)
}

fn review_status_display(status: &str) -> &'static str {
    match status {
        "approved" => "已通过",
        "rejected" => "已拒绝",
        "changes_requested" => "需调整",
        _ => "待审核",
    }
}

fn is_canonical_sha256(value: &str) -> bool {
    value
        .strip_prefix("sha256:")
        .map(|digest| is_lower_hex(digest, 64))
        .unwrap_or(false)
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || matches!(byte, b'a'..=b'f'))
}

fn sha256_hex(value: &[u8]) -> String {
    format!("{:x}", Sha256::digest(value))
}

fn md5_hex(value: &[u8]) -> String {
    format!("{:x}", Md5::digest(value))
}

async fn run_preview(
    pool: &PgPool,
    artifact_id: Option<Uuid>,
    policy: &MirrorPolicy,
) -> Result<MirrorReport> {
    let Some(artifact) = peek_candidate(pool, artifact_id).await? else {
        return Ok(empty_report(false, "no_mirrorable_generated_images"));
    };
    validate_artifact(&artifact, policy)?;
    let workflow_root_id = resolve_workflow_root_pool(pool, artifact.work_item_id).await?;
    let action_status = if mirror_is_current(&artifact) {
        "already_synced"
    } else {
        "mirror_preview_ready"
    };
    Ok(artifact_report(
        &artifact,
        workflow_root_id,
        false,
        action_status,
        false,
        false,
    ))
}

async fn run_apply(
    pool: &PgPool,
    artifact_id: Option<Uuid>,
    config: MirrorConfig,
) -> Result<MirrorReport> {
    let mut tx = pool
        .begin()
        .await
        .context("begin Huabaosi Feishu mirror transaction")?;
    let Some(artifact) = lock_candidate(&mut tx, artifact_id).await? else {
        tx.commit()
            .await
            .context("commit empty Huabaosi Feishu mirror transaction")?;
        return Ok(empty_report(true, "no_mirrorable_generated_images"));
    };
    let validated = match validate_artifact(&artifact, &config.policy) {
        Ok(validated) => validated,
        Err(_) => {
            let failure = MirrorFailure::policy("artifact_validation_failed");
            append_failure_event(&mut tx, &artifact, &failure).await?;
            tx.commit()
                .await
                .context("commit Huabaosi Feishu mirror policy failure")?;
            bail!(
                "Huabaosi Feishu mirror failed at {} with {}",
                failure.stage,
                failure.code
            );
        }
    };
    let workflow_root_id = resolve_workflow_root_tx(&mut tx, artifact.work_item_id).await?;
    if mirror_is_current(&artifact) {
        tx.commit()
            .await
            .context("commit current Huabaosi Feishu mirror transaction")?;
        return Ok(artifact_report(
            &artifact,
            workflow_root_id,
            true,
            "already_synced",
            false,
            false,
        ));
    }

    #[cfg(any(test, feature = "huabaosi-feishu-mirror-adapter"))]
    let external_result = mirror_to_feishu(&artifact, &validated, workflow_root_id, &config);
    #[cfg(not(any(test, feature = "huabaosi-feishu-mirror-adapter")))]
    let external_result: std::result::Result<String, MirrorFailure> =
        Err(MirrorFailure::policy("adapter_not_compiled"));

    let record_id = match external_result {
        Ok(record_id) => record_id,
        Err(failure) => {
            append_failure_event(&mut tx, &artifact, &failure).await?;
            tx.commit()
                .await
                .context("commit Huabaosi Feishu mirror external failure")?;
            bail!(
                "Huabaosi Feishu mirror failed at {} with {}",
                failure.stage,
                failure.code
            );
        }
    };

    let ref_id = upsert_workbench_ref(&mut tx, &artifact, &record_id, workflow_root_id).await?;
    append_success_event(&mut tx, &artifact, ref_id).await?;
    tx.commit()
        .await
        .context("commit Huabaosi Feishu mirror transaction")?;
    Ok(artifact_report(
        &artifact,
        workflow_root_id,
        true,
        "generated_image_mirrored",
        true,
        true,
    ))
}

impl MirrorFailure {
    fn policy(code: &'static str) -> Self {
        Self {
            stage: "policy",
            code,
            external_write_executed: Some(false),
            automatic_retry_allowed: false,
        }
    }

    #[cfg(any(
        test,
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    ))]
    fn external(
        stage: &'static str,
        code: &'static str,
        external_write_executed: Option<bool>,
        automatic_retry_allowed: bool,
    ) -> Self {
        Self {
            stage,
            code,
            external_write_executed,
            automatic_retry_allowed,
        }
    }
}

fn mirror_is_current(artifact: &MirrorArtifact) -> bool {
    artifact.workbench_status.as_deref() == Some("active")
        && artifact
            .last_synced_at
            .map(|synced| synced >= artifact.updated_at)
            .unwrap_or(false)
}

const CANDIDATE_SELECT: &str = r#"
    SELECT
        artifact.id,
        artifact.work_item_id,
        artifact.review_status,
        artifact.title,
        artifact.artifact_uri,
        artifact.content_hash,
        artifact.source_ids,
        artifact.metadata,
        artifact.reviewed_at,
        artifact.reviewed_by,
        artifact.review_decision_reason,
        artifact.created_at,
        artifact.updated_at,
        mirror.last_synced_at,
        mirror.status AS workbench_status,
        creation.data AS creation_event_data
    FROM qintopia_agent_os.artifacts artifact
    JOIN qintopia_agent_os.work_items item
      ON item.id = artifact.work_item_id
    LEFT JOIN qintopia_agent_os.human_workbench_refs mirror
      ON mirror.artifact_id = artifact.id
     AND mirror.provider = 'feishu_base_huabaosi_generated_image'
     AND mirror.external_id = artifact.id::text
    LEFT JOIN LATERAL (
        SELECT event.data
        FROM qintopia_agent_os.work_item_events event
        WHERE event.artifact_id = artifact.id
          AND event.event_type = 'generated_image_created'
          AND event.actor_type = 'worker'
          AND event.actor_id = 'huabaosi-image-generation-worker'
        ORDER BY event.created_at DESC, event.id DESC
        LIMIT 1
    ) creation ON true
    WHERE artifact.artifact_type = 'generated_image'
      AND artifact.created_by_agent = 'huabaosi'
      AND item.work_item_type = 'image_generation_request'
      AND item.capability_key = 'huabaosi.generate_image_asset'
      AND item.target_agent = 'huabaosi'
      AND (
          ($1::uuid IS NOT NULL AND artifact.id = $1)
          OR (
              $1::uuid IS NULL
              AND (
                  mirror.id IS NULL
                  OR mirror.status <> 'active'
                  OR mirror.last_synced_at IS NULL
                  OR mirror.last_synced_at < artifact.updated_at
              )
          )
      )
    ORDER BY artifact.updated_at ASC, artifact.created_at ASC
    LIMIT 1
"#;

async fn peek_candidate(
    pool: &PgPool,
    artifact_id: Option<Uuid>,
) -> Result<Option<MirrorArtifact>> {
    let row = sqlx::query(CANDIDATE_SELECT)
        .bind(artifact_id)
        .fetch_optional(pool)
        .await
        .context("peek Huabaosi generated image for Feishu mirror")?;
    row.map(artifact_from_row).transpose()
}

async fn lock_candidate(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    artifact_id: Option<Uuid>,
) -> Result<Option<MirrorArtifact>> {
    let query = format!("{CANDIDATE_SELECT} FOR UPDATE OF artifact");
    let row = sqlx::query(&query)
        .bind(artifact_id)
        .fetch_optional(&mut **tx)
        .await
        .context("lock Huabaosi generated image for Feishu mirror")?;
    row.map(artifact_from_row).transpose()
}

fn artifact_from_row(row: sqlx::postgres::PgRow) -> Result<MirrorArtifact> {
    Ok(MirrorArtifact {
        id: row.try_get("id")?,
        work_item_id: row.try_get("work_item_id")?,
        review_status: row.try_get("review_status")?,
        title: row.try_get("title")?,
        artifact_uri: row
            .try_get::<Option<String>, _>("artifact_uri")?
            .context("generated image artifact URI is missing")?,
        content_hash: row
            .try_get::<Option<String>, _>("content_hash")?
            .context("generated image content hash is missing")?,
        source_ids: row.try_get("source_ids")?,
        metadata: row.try_get("metadata")?,
        creation_event_data: row
            .try_get::<Option<Value>, _>("creation_event_data")?
            .unwrap_or(Value::Null),
        reviewed_at: row.try_get("reviewed_at")?,
        reviewed_by: row.try_get("reviewed_by")?,
        review_decision_reason: row.try_get("review_decision_reason")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
        last_synced_at: row.try_get("last_synced_at")?,
        workbench_status: row.try_get("workbench_status")?,
    })
}

const WORKFLOW_ROOT_SELECT: &str = r#"
    WITH RECURSIVE lineage AS (
        SELECT id, parent_work_item_id
        FROM qintopia_agent_os.work_items
        WHERE id = $1

        UNION ALL

        SELECT parent.id, parent.parent_work_item_id
        FROM qintopia_agent_os.work_items parent
        JOIN lineage child ON child.parent_work_item_id = parent.id
    )
    SELECT id
    FROM lineage
    WHERE parent_work_item_id IS NULL
    LIMIT 1
"#;

pub(crate) async fn resolve_workflow_root_pool(pool: &PgPool, work_item_id: Uuid) -> Result<Uuid> {
    sqlx::query_scalar(WORKFLOW_ROOT_SELECT)
        .bind(work_item_id)
        .fetch_optional(pool)
        .await
        .context("resolve Huabaosi workflow root")?
        .context("Huabaosi image request has no workflow root")
}

async fn resolve_workflow_root_tx(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Uuid,
) -> Result<Uuid> {
    sqlx::query_scalar(WORKFLOW_ROOT_SELECT)
        .bind(work_item_id)
        .fetch_optional(&mut **tx)
        .await
        .context("resolve locked Huabaosi workflow root")?
        .context("Huabaosi image request has no workflow root")
}

async fn upsert_workbench_ref(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    artifact: &MirrorArtifact,
    record_id: &str,
    workflow_root_id: Uuid,
) -> Result<Uuid> {
    upsert_workbench_ref_values(
        tx,
        artifact.work_item_id,
        artifact.id,
        &artifact.title,
        &artifact.review_status,
        &artifact.content_hash,
        record_id,
        workflow_root_id,
    )
    .await
}

pub(crate) async fn record_primary_storage_workbench_ref(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Uuid,
    artifact_id: Uuid,
    content_hash: &str,
    record_id: &str,
    workflow_root_id: Uuid,
) -> Result<Uuid> {
    upsert_workbench_ref_values(
        tx,
        work_item_id,
        artifact_id,
        "活动海报图片（待审核）",
        "pending",
        content_hash,
        record_id,
        workflow_root_id,
    )
    .await
}

#[allow(clippy::too_many_arguments)]
async fn upsert_workbench_ref_values(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    work_item_id: Uuid,
    artifact_id: Uuid,
    title: &str,
    review_status: &str,
    content_hash: &str,
    record_id: &str,
    workflow_root_id: Uuid,
) -> Result<Uuid> {
    let record_id_hash = format!("sha256:{}", sha256_hex(record_id.as_bytes()));
    let row = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.human_workbench_refs
            (
                work_item_id,
                artifact_id,
                provider,
                external_id,
                external_url,
                display_title,
                status,
                metadata,
                last_synced_at
            )
        VALUES ($1, $2, $3, $4, '', $5, 'active', $6, now())
        ON CONFLICT (provider, external_id)
        DO UPDATE SET
            work_item_id = EXCLUDED.work_item_id,
            artifact_id = EXCLUDED.artifact_id,
            display_title = EXCLUDED.display_title,
            status = 'active',
            metadata = EXCLUDED.metadata,
            last_synced_at = now(),
            updated_at = now()
        WHERE qintopia_agent_os.human_workbench_refs.work_item_id = EXCLUDED.work_item_id
          AND qintopia_agent_os.human_workbench_refs.artifact_id = EXCLUDED.artifact_id
        RETURNING id
        "#,
    )
    .bind(work_item_id)
    .bind(artifact_id)
    .bind(PROVIDER)
    .bind(artifact_id.to_string())
    .bind(title)
    .bind(json!({
        "schema_version": SCHEMA_VERSION,
        "workflow_root_id": workflow_root_id,
        "review_status": review_status,
        "content_hash": content_hash,
        "record_id_hash": record_id_hash,
        "attachment_present": true,
        "sensitive_fields_redacted": true,
    }))
    .fetch_optional(&mut **tx)
    .await
    .context("upsert Huabaosi Feishu workbench ref")?
    .context("existing Huabaosi Feishu workbench ref belongs to another artifact")?;
    Ok(row.get("id"))
}

async fn append_success_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    artifact: &MirrorArtifact,
    ref_id: Uuid,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
        VALUES
            ($1, $2, 'generated_image_feishu_mirrored', 'worker', $3,
             'generated image mirrored to the Feishu artifact workbench', $4)
        "#,
    )
    .bind(artifact.work_item_id)
    .bind(artifact.id)
    .bind(WORKER_ID)
    .bind(json!({
        "provider": PROVIDER,
        "workbench_ref_id": ref_id,
        "schema_version": SCHEMA_VERSION,
        "review_status": artifact.review_status,
        "content_hash": artifact.content_hash,
        "external_write_executed": true,
        "artifact_review_changed": false,
        "external_publish_executed": false,
        "external_send_executed": false,
        "sensitive_fields_redacted": true,
    }))
    .execute(&mut **tx)
    .await
    .context("append Huabaosi Feishu mirror success event")?;
    Ok(())
}

async fn append_failure_event(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    artifact: &MirrorArtifact,
    failure: &MirrorFailure,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
        VALUES
            ($1, $2, 'generated_image_feishu_mirror_failed', 'worker', $3,
             'generated image Feishu mirror failed without changing artifact state', $4)
        "#,
    )
    .bind(artifact.work_item_id)
    .bind(artifact.id)
    .bind(WORKER_ID)
    .bind(json!({
        "provider": PROVIDER,
        "schema_version": SCHEMA_VERSION,
        "failure_stage": failure.stage,
        "failure_code": failure.code,
        "external_write_executed": failure.external_write_executed,
        "automatic_retry_allowed": failure.automatic_retry_allowed,
        "artifact_review_changed": false,
        "external_publish_executed": false,
        "external_send_executed": false,
        "sensitive_fields_redacted": true,
    }))
    .execute(&mut **tx)
    .await
    .context("append Huabaosi Feishu mirror failure event")?;
    Ok(())
}

#[cfg(any(test, feature = "huabaosi-feishu-mirror-adapter"))]
fn mirror_to_feishu(
    artifact: &MirrorArtifact,
    validated: &ValidatedArtifact,
    workflow_root_id: Uuid,
    config: &MirrorConfig,
) -> std::result::Result<String, MirrorFailure> {
    let credentials = read_feishu_credentials(&config.profile_env_path)?;
    let client = FeishuClient::authenticate(&config.api_root, &credentials)?;
    let existing = client.search_record(&config.base_token, &config.table_id, artifact.id)?;
    let mut media_bytes = client.fetch_media(validated, config.policy.max_media_bytes)?;
    validate_media_bytes(artifact, validated, &media_bytes)?;

    let fields_without_attachment =
        build_feishu_fields(artifact, validated, workflow_root_id, None);
    let record = match existing {
        Some(record) => record,
        None => client.create_record(
            &config.base_token,
            &config.table_id,
            &fields_without_attachment,
        )?,
    };
    let file_token = client.upload_media(&config.base_token, artifact.id, &media_bytes)?;
    let fields = build_feishu_fields(
        artifact,
        validated,
        workflow_root_id,
        Some(file_token.as_str()),
    );
    client.update_record(
        &config.base_token,
        &config.table_id,
        &record.record_id,
        &fields,
    )?;
    media_bytes.zeroize();
    Ok(record.record_id)
}

pub(crate) fn store_primary_generated_image(
    config: &FeishuPrimaryStorageConfig,
    image: &FeishuPrimaryStorageImage<'_>,
) -> Result<FeishuPrimaryStorageResult> {
    #[cfg(not(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    )))]
    {
        let _ = (config, image);
        bail!("Huabaosi Feishu storage adapter is not compiled into this binary");
    }

    #[cfg(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    ))]
    {
        validate_primary_storage_image(image, config.max_media_bytes)?;
        let credentials =
            read_feishu_credentials(&config.profile_env_path).map_err(primary_storage_error)?;
        let client = FeishuClient::authenticate(&config.api_root, &credentials)
            .map_err(primary_storage_error)?;
        let existing = client
            .search_record(&config.base_token, &config.table_id, image.artifact_id)
            .map_err(primary_storage_error)?;
        let file_token = client
            .upload_media(&config.base_token, image.artifact_id, image.bytes)
            .map_err(primary_storage_error)?;
        let mut readback = client
            .download_media(file_token.as_str(), config.max_media_bytes)
            .map_err(primary_storage_error)?;
        if readback.as_slice() != image.bytes {
            readback.zeroize();
            bail!("Huabaosi Feishu storage readback did not match uploaded JPEG");
        }
        validate_primary_storage_bytes(image, &readback)?;
        readback.zeroize();

        let fields = build_primary_storage_fields(image, file_token.as_str());
        let record = match existing {
            Some(record) => {
                client
                    .update_record(
                        &config.base_token,
                        &config.table_id,
                        &record.record_id,
                        &fields,
                    )
                    .map_err(primary_storage_error)?;
                record
            }
            None => client
                .create_record(&config.base_token, &config.table_id, &fields)
                .map_err(primary_storage_error)?,
        };

        Ok(FeishuPrimaryStorageResult {
            artifact_uri: format!(
                "feishu-base://huabaosi-generated-image/{}",
                image.artifact_id
            ),
            record_id: record.record_id,
        })
    }
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn validate_primary_storage_image(
    image: &FeishuPrimaryStorageImage<'_>,
    max_media_bytes: usize,
) -> Result<()> {
    if image.bytes.is_empty() || image.bytes.len() > max_media_bytes {
        bail!("Huabaosi Feishu storage image bytes are outside the reviewed bound");
    }
    if image.width != REQUIRED_WIDTH as u32 || image.height != REQUIRED_HEIGHT as u32 {
        bail!("Huabaosi Feishu storage image dimensions are invalid");
    }
    if !is_canonical_sha256(image.content_hash)
        || !is_lower_hex(image.file_md5, 32)
        || !is_canonical_sha256(image.source_content_hash)
    {
        bail!("Huabaosi Feishu storage image identity is not canonical");
    }
    validate_primary_storage_bytes(image, image.bytes)
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn validate_primary_storage_bytes(
    image: &FeishuPrimaryStorageImage<'_>,
    bytes: &[u8],
) -> Result<()> {
    if format!("sha256:{}", sha256_hex(bytes)) != image.content_hash
        || md5_hex(bytes) != image.file_md5
    {
        bail!("Huabaosi Feishu storage image digest does not match JPEG bytes");
    }
    let decoded = image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg)
        .context("decode Huabaosi Feishu storage JPEG")?;
    if decoded.dimensions() != (image.width, image.height) {
        bail!("Huabaosi Feishu storage JPEG dimensions do not match");
    }
    Ok(())
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn build_primary_storage_fields(image: &FeishuPrimaryStorageImage<'_>, file_token: &str) -> Value {
    let now = Utc::now().timestamp_millis();
    json!({
        "产物标题": "活动海报图片（待审核）",
        "AgentOS产物ID": image.artifact_id.to_string(),
        "AgentOS工作项ID": image.workflow_root_id.to_string(),
        "图片请求ID": image.work_item_id.to_string(),
        "最终JPEG": [{"file_token": file_token}],
        "JPEG SHA-256": image.content_hash,
        "文件MD5": image.file_md5,
        "字节数": image.bytes.len(),
        "宽度": image.width,
        "高度": image.height,
        "MIME类型": REQUIRED_MIME_TYPE,
        "源PNG SHA-256": image.source_content_hash,
        "转换规则": REQUIRED_TRANSFORM,
        "审核状态": "待审核",
        "生成时间": now,
        "更新时间": now,
    })
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn primary_storage_error(failure: MirrorFailure) -> anyhow::Error {
    anyhow::anyhow!(
        "Huabaosi Feishu storage failed at {} with {}",
        failure.stage,
        failure.code
    )
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn validate_primary_storage_artifact(
    artifact: &MirrorArtifact,
) -> Result<ValidatedPrimaryStorageArtifact> {
    let expected_uri = format!("feishu-base://huabaosi-generated-image/{}", artifact.id);
    if artifact.artifact_uri != expected_uri {
        bail!("Huabaosi Feishu-backed artifact URI does not match the generated image id");
    }
    if !is_canonical_sha256(&artifact.content_hash) {
        bail!("Huabaosi Feishu-backed artifact content hash is not canonical");
    }
    let metadata = artifact
        .metadata
        .as_object()
        .context("Huabaosi Feishu-backed metadata must be an object")?;
    let mime_type = metadata_text(metadata, "mime_type")?;
    let generated_by = metadata_text(metadata, "generated_by")?;
    let provider = metadata_text(metadata, "provider")?;
    let model = metadata_text(metadata, "model")?;
    let file_md5 = metadata_text(metadata, "file_md5")?;
    let source_mime_type = metadata_text(metadata, "provider_source_mime_type")?;
    let source_content_hash = metadata_text(metadata, "provider_source_content_hash")?;
    let media_transform = metadata_text(metadata, "media_transform")?;
    let alpha_background = metadata_text(metadata, "alpha_background")?;
    let approved_brief_id = metadata_text(metadata, "approved_brief_artifact_id")?;
    let approved_brief_hash = metadata_text(metadata, "approved_brief_content_hash")?;
    let prompt_hash = metadata_text(metadata, "prompt_hash")?;
    let jpeg_quality = metadata_i64(metadata, "jpeg_quality")?;
    let width = metadata_i64(metadata, "width")?;
    let height = metadata_i64(metadata, "height")?;
    let byte_size = metadata_i64(metadata, "byte_size")?;
    if generated_by != REQUIRED_GENERATOR
        || provider != REQUIRED_PROVIDER
        || model != REQUIRED_MODEL
        || mime_type != REQUIRED_MIME_TYPE
        || source_mime_type != REQUIRED_SOURCE_MIME_TYPE
        || media_transform != REQUIRED_TRANSFORM
        || alpha_background != REQUIRED_ALPHA_BACKGROUND
        || jpeg_quality != REQUIRED_JPEG_QUALITY
    {
        bail!("Huabaosi Feishu-backed metadata does not match the reviewed JPEG contract");
    }
    if Uuid::parse_str(&approved_brief_id).is_err()
        || !is_canonical_sha256(&approved_brief_hash)
        || !is_canonical_sha256(&prompt_hash)
        || !is_lower_hex(&file_md5, 32)
        || !is_canonical_sha256(&source_content_hash)
    {
        bail!("Huabaosi Feishu-backed identity metadata is not canonical");
    }
    validate_source_ids(artifact, &approved_brief_id, &approved_brief_hash)?;
    validate_creation_event(artifact, metadata)?;
    if width != REQUIRED_WIDTH || height != REQUIRED_HEIGHT {
        bail!("Huabaosi Feishu-backed dimensions do not match the reviewed JPEG contract");
    }
    let byte_size = usize::try_from(byte_size)
        .ok()
        .filter(|size| *size > 0 && *size <= DEFAULT_MAX_MEDIA_BYTES)
        .context("Huabaosi Feishu-backed byte size is outside the reviewed bound")?;
    Ok(ValidatedPrimaryStorageArtifact {
        file_md5,
        source_content_hash,
        byte_size,
        width: u32::try_from(width).context("Huabaosi Feishu-backed width is invalid")?,
        height: u32::try_from(height).context("Huabaosi Feishu-backed height is invalid")?,
    })
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn revalidate_primary_storage_artifact(
    artifact: &MirrorArtifact,
    validated: &ValidatedPrimaryStorageArtifact,
    config: &FeishuPrimaryStorageConfig,
) -> std::result::Result<PrimaryStorageRevalidationReport, MirrorFailure> {
    let credentials = read_feishu_credentials(&config.profile_env_path)?;
    let client = FeishuClient::authenticate(&config.api_root, &credentials)?;
    let record = client
        .search_record(&config.base_token, &config.table_id, artifact.id)?
        .ok_or_else(|| {
            MirrorFailure::external("record_search", "record_missing", Some(false), false)
        })?;
    let fields = record.fields.as_object().ok_or_else(|| {
        MirrorFailure::external("record_search", "record_fields_missing", Some(false), false)
    })?;
    validate_primary_storage_record_fields(artifact, validated, fields)?;
    let file_token = primary_storage_attachment_token(fields)?;
    let mut readback = client.download_media(file_token.as_str(), config.max_media_bytes)?;
    validate_primary_storage_readback(artifact, validated, &readback)?;
    readback.zeroize();
    Ok(PrimaryStorageRevalidationReport {
        success: true,
        worker: WORKER_ID,
        action_status: "feishu_primary_storage_revalidated".to_string(),
        artifact_id: artifact.id,
        work_item_id: artifact.work_item_id,
        schema_version: SCHEMA_VERSION,
        content_hash: artifact.content_hash.clone(),
        byte_size: validated.byte_size,
        width: validated.width,
        height: validated.height,
        external_calls_executed: true,
        database_writes_executed: false,
        sensitive_fields_redacted: true,
        guardrails: primary_storage_revalidation_guardrails(),
    })
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn validate_primary_storage_record_fields(
    artifact: &MirrorArtifact,
    validated: &ValidatedPrimaryStorageArtifact,
    fields: &Map<String, Value>,
) -> std::result::Result<(), MirrorFailure> {
    for (field, expected) in [
        ("AgentOS产物ID", artifact.id.to_string()),
        ("图片请求ID", artifact.work_item_id.to_string()),
        ("JPEG SHA-256", artifact.content_hash.clone()),
        ("文件MD5", validated.file_md5.clone()),
        ("MIME类型", REQUIRED_MIME_TYPE.to_string()),
        ("源PNG SHA-256", validated.source_content_hash.clone()),
        ("转换规则", REQUIRED_TRANSFORM.to_string()),
    ] {
        if field_text(fields, field).as_deref() != Some(expected.as_str()) {
            return Err(MirrorFailure::external(
                "record_search",
                "record_identity_mismatch",
                Some(false),
                false,
            ));
        }
    }
    for (field, expected) in [
        (
            "字节数",
            i64::try_from(validated.byte_size).unwrap_or(i64::MAX),
        ),
        ("宽度", i64::from(validated.width)),
        ("高度", i64::from(validated.height)),
    ] {
        if field_i64(fields, field) != Some(expected) {
            return Err(MirrorFailure::external(
                "record_search",
                "record_identity_mismatch",
                Some(false),
                false,
            ));
        }
    }
    Ok(())
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn primary_storage_attachment_token(
    fields: &Map<String, Value>,
) -> std::result::Result<Zeroizing<String>, MirrorFailure> {
    let attachments = fields
        .get("最终JPEG")
        .and_then(Value::as_array)
        .map(Vec::as_slice)
        .ok_or_else(|| {
            MirrorFailure::external("record_search", "attachment_missing", Some(false), false)
        })?;
    if attachments.len() != 1 {
        return Err(MirrorFailure::external(
            "record_search",
            "attachment_count_invalid",
            Some(false),
            false,
        ));
    }
    let token = attachments[0]
        .get("file_token")
        .and_then(Value::as_str)
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            MirrorFailure::external(
                "record_search",
                "attachment_token_missing",
                Some(false),
                false,
            )
        })?;
    validate_external_identifier(token, "Feishu attachment token").map_err(|_| {
        MirrorFailure::external(
            "record_search",
            "attachment_token_invalid",
            Some(false),
            false,
        )
    })?;
    Ok(Zeroizing::new(token.to_string()))
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn validate_primary_storage_readback(
    artifact: &MirrorArtifact,
    validated: &ValidatedPrimaryStorageArtifact,
    bytes: &[u8],
) -> std::result::Result<(), MirrorFailure> {
    if bytes.len() != validated.byte_size
        || format!("sha256:{}", sha256_hex(bytes)) != artifact.content_hash
        || md5_hex(bytes) != validated.file_md5
    {
        return Err(MirrorFailure::external(
            "media_readback",
            "file_identity_mismatch",
            Some(false),
            false,
        ));
    }
    let image =
        image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).map_err(|_| {
            MirrorFailure::external("media_readback", "jpeg_decode_failed", Some(false), false)
        })?;
    if image.dimensions() != (validated.width, validated.height) {
        return Err(MirrorFailure::external(
            "media_readback",
            "jpeg_dimensions_mismatch",
            Some(false),
            false,
        ));
    }
    Ok(())
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn field_text(fields: &Map<String, Value>, name: &str) -> Option<String> {
    fields.get(name).and_then(Value::as_str).map(str::to_string)
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn field_i64(fields: &Map<String, Value>, name: &str) -> Option<i64> {
    fields.get(name).and_then(|value| {
        value
            .as_i64()
            .or_else(|| value.as_f64().map(|number| number as i64))
    })
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn primary_storage_revalidation_guardrails() -> Vec<String> {
    vec![
        "Postgres remains the system fact source".to_string(),
        "the Feishu Base record is resolved only by generated_image_artifact_id".to_string(),
        "the attachment is downloaded through authenticated Feishu media API".to_string(),
        "revalidation does not approve, publish, call QiWe, or send".to_string(),
        "attachment tokens, record ids, credentials, and raw responses are redacted".to_string(),
    ]
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
impl FeishuClient {
    fn authenticate(
        api_root: &Url,
        credentials: &FeishuCredentials,
    ) -> std::result::Result<Self, MirrorFailure> {
        let http = http_client_for(api_root);
        let endpoint = api_endpoint(api_root, "auth/v3/tenant_access_token/internal")?;
        let body = Zeroizing::new(
            serde_json::to_vec(&FeishuAuthRequest {
                app_id: credentials.app_id.as_str(),
                app_secret: credentials.app_secret.as_str(),
            })
            .map_err(|_| {
                MirrorFailure::external(
                    "feishu_auth",
                    "request_serialization_failed",
                    Some(false),
                    false,
                )
            })?,
        );
        let response =
            request_json_bytes(&http, "POST", &endpoint, None, &body, "feishu_auth", false)?;
        let mut parsed: FeishuAuthResponse =
            serde_json::from_slice(&response.body).map_err(|_| {
                MirrorFailure::external(
                    "feishu_auth",
                    "credential_response_invalid",
                    Some(false),
                    true,
                )
            })?;
        if parsed.code != 0
            || parsed.tenant_access_token.is_empty()
            || parsed.tenant_access_token.len() > 4096
        {
            parsed.tenant_access_token.zeroize();
            return Err(MirrorFailure::external(
                "feishu_auth",
                "credential_response_invalid",
                Some(false),
                true,
            ));
        }
        let tenant_token = Zeroizing::new(std::mem::take(&mut parsed.tenant_access_token));
        Ok(Self {
            tenant_token,
            http,
            api_root: api_root.clone(),
        })
    }

    fn search_record(
        &self,
        base_token: &str,
        table_id: &str,
        artifact_id: Uuid,
    ) -> std::result::Result<Option<FeishuRecord>, MirrorFailure> {
        let endpoint = self.bitable_endpoint(base_token, table_id, "records/search")?;
        let request = json!({
            "field_names": ["AgentOS产物ID"],
            "filter": {
                "conjunction": "and",
                "conditions": [{
                    "field_name": "AgentOS产物ID",
                    "operator": "is",
                    "value": [artifact_id.to_string()],
                }]
            },
            "page_size": 20,
            "automatic_fields": false,
        });
        let parsed = request_json(
            &self.http,
            "POST",
            &endpoint,
            Some(self.tenant_token.as_str()),
            Some(&request),
            "record_search",
            false,
        )?;
        let items = parsed
            .pointer("/data/items")
            .and_then(Value::as_array)
            .map(Vec::as_slice)
            .ok_or_else(|| {
                MirrorFailure::external("record_search", "record_list_missing", Some(false), true)
            })?;
        if items.len() > 1
            || parsed.pointer("/data/has_more").and_then(Value::as_bool) == Some(true)
        {
            return Err(MirrorFailure::external(
                "record_search",
                "duplicate_artifact_records",
                Some(false),
                false,
            ));
        }
        let Some(record) = items.first() else {
            return Ok(None);
        };
        let record_id = record
            .get("record_id")
            .or_else(|| record.get("id"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                MirrorFailure::external("record_search", "record_id_missing", Some(false), false)
            })?;
        validate_external_identifier(record_id, "Feishu record id").map_err(|_| {
            MirrorFailure::external("record_search", "record_id_invalid", Some(false), false)
        })?;
        Ok(Some(FeishuRecord {
            record_id: record_id.to_string(),
            fields: record.get("fields").cloned().unwrap_or(Value::Null),
        }))
    }

    fn create_record(
        &self,
        base_token: &str,
        table_id: &str,
        fields: &Value,
    ) -> std::result::Result<FeishuRecord, MirrorFailure> {
        let endpoint = self.bitable_endpoint(base_token, table_id, "records")?;
        let parsed = request_json(
            &self.http,
            "POST",
            &endpoint,
            Some(self.tenant_token.as_str()),
            Some(&json!({"fields": fields})),
            "record_create",
            true,
        )?;
        let record_id = parsed
            .pointer("/data/record/record_id")
            .or_else(|| parsed.pointer("/data/record/id"))
            .or_else(|| parsed.pointer("/data/record_id"))
            .and_then(Value::as_str)
            .ok_or_else(|| {
                MirrorFailure::external("record_create", "record_id_missing", None, true)
            })?;
        validate_external_identifier(record_id, "Feishu record id").map_err(|_| {
            MirrorFailure::external("record_create", "record_id_invalid", None, true)
        })?;
        Ok(FeishuRecord {
            record_id: record_id.to_string(),
            fields: Value::Null,
        })
    }

    fn update_record(
        &self,
        base_token: &str,
        table_id: &str,
        record_id: &str,
        fields: &Value,
    ) -> std::result::Result<(), MirrorFailure> {
        validate_external_identifier(record_id, "Feishu record id").map_err(|_| {
            MirrorFailure::external("record_update", "record_id_invalid", Some(false), false)
        })?;
        let endpoint =
            self.bitable_endpoint(base_token, table_id, &format!("records/{record_id}"))?;
        request_json(
            &self.http,
            "PUT",
            &endpoint,
            Some(self.tenant_token.as_str()),
            Some(&json!({"fields": fields})),
            "record_update",
            true,
        )?;
        Ok(())
    }

    fn fetch_media(
        &self,
        validated: &ValidatedArtifact,
        max_media_bytes: usize,
    ) -> std::result::Result<Zeroizing<Vec<u8>>, MirrorFailure> {
        let mut response = self
            .http
            .request("GET", &validated.artifact_uri, &[], &[], max_media_bytes)
            .map_err(|error| {
                MirrorFailure::external(
                    "media_readback",
                    if error.transport {
                        "transport_error"
                    } else {
                        "request_invalid"
                    },
                    Some(false),
                    error.transport,
                )
            })?;
        if response.status != 200 {
            return Err(MirrorFailure::external(
                "media_readback",
                "http_status_rejected",
                Some(false),
                true,
            ));
        }
        Ok(Zeroizing::new(std::mem::take(&mut response.body)))
    }

    fn upload_media(
        &self,
        base_token: &str,
        artifact_id: Uuid,
        bytes: &[u8],
    ) -> std::result::Result<Zeroizing<String>, MirrorFailure> {
        let endpoint = api_endpoint(&self.api_root, "drive/v1/medias/upload_all")?;
        let boundary = format!("qintopia-{artifact_id}");
        let body = multipart_image_body(&boundary, base_token, artifact_id, bytes);
        let mut headers = vec![
            (
                "Content-Type",
                format!("multipart/form-data; boundary={boundary}"),
            ),
            (
                "Authorization",
                format!("Bearer {}", self.tenant_token.as_str()),
            ),
            ("Accept", "application/json".to_string()),
        ];
        let response = self.http.request(
            "POST",
            &endpoint,
            &headers,
            &body,
            MAX_FEISHU_RESPONSE_BYTES,
        );
        for (_, value) in &mut headers {
            value.zeroize();
        }
        let mut response = response.map_err(|error| {
            MirrorFailure::external(
                "media_upload",
                if error.transport {
                    "transport_error"
                } else {
                    "request_invalid"
                },
                if error.request_may_have_been_sent() {
                    None
                } else {
                    Some(false)
                },
                true,
            )
        })?;
        let mut parsed = parse_feishu_response(&mut response, "media_upload", true)?;
        let file_token = parsed
            .pointer_mut("/data/file_token")
            .map(Value::take)
            .and_then(|value| value.as_str().map(str::to_string))
            .filter(|value| !value.is_empty() && value.len() <= 4096)
            .ok_or_else(|| {
                MirrorFailure::external("media_upload", "file_token_missing", None, true)
            })?;
        Ok(Zeroizing::new(file_token))
    }

    #[cfg(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    ))]
    fn download_media(
        &self,
        file_token: &str,
        max_media_bytes: usize,
    ) -> std::result::Result<Zeroizing<Vec<u8>>, MirrorFailure> {
        validate_external_identifier(file_token, "Feishu file token").map_err(|_| {
            MirrorFailure::external("media_readback", "file_token_invalid", Some(false), false)
        })?;
        let endpoint = api_endpoint(
            &self.api_root,
            &format!("drive/v1/medias/{file_token}/download"),
        )?;
        let mut headers = vec![(
            "Authorization",
            format!("Bearer {}", self.tenant_token.as_str()),
        )];
        let response = self
            .http
            .request("GET", &endpoint, &headers, &[], max_media_bytes);
        for (_, value) in &mut headers {
            value.zeroize();
        }
        let mut response = response.map_err(|error| {
            MirrorFailure::external(
                "media_readback",
                if error.transport {
                    "transport_error"
                } else {
                    "request_invalid"
                },
                Some(false),
                error.transport,
            )
        })?;
        if response.status != 200 {
            return Err(MirrorFailure::external(
                "media_readback",
                "http_status_rejected",
                Some(false),
                true,
            ));
        }
        Ok(Zeroizing::new(std::mem::take(&mut response.body)))
    }

    fn bitable_endpoint(
        &self,
        base_token: &str,
        table_id: &str,
        suffix: &str,
    ) -> std::result::Result<Url, MirrorFailure> {
        validate_external_identifier(base_token, "Feishu Base token")
            .and_then(|_| validate_external_identifier(table_id, "Feishu table id"))
            .map_err(|_| {
                MirrorFailure::external("configuration", "identifier_invalid", Some(false), false)
            })?;
        api_endpoint(
            &self.api_root,
            &format!("bitable/v1/apps/{base_token}/tables/{table_id}/{suffix}"),
        )
    }
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn http_client_for(_api_root: &Url) -> HttpClient {
    #[cfg(test)]
    if _api_root.scheme() == "http" {
        return HttpClient::test_only();
    }
    let client = HttpClient::production();
    debug_assert!(!client.allows_insecure_http());
    client
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn api_endpoint(api_root: &Url, path: &str) -> std::result::Result<Url, MirrorFailure> {
    api_root.join(path).map_err(|_| {
        MirrorFailure::external("configuration", "api_endpoint_invalid", Some(false), false)
    })
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn request_json(
    http: &HttpClient,
    method: &str,
    endpoint: &Url,
    bearer_token: Option<&str>,
    body: Option<&Value>,
    stage: &'static str,
    external_write: bool,
) -> std::result::Result<Value, MirrorFailure> {
    let body = body
        .map(serde_json::to_vec)
        .transpose()
        .map_err(|_| {
            MirrorFailure::external(stage, "request_serialization_failed", Some(false), false)
        })?
        .unwrap_or_default();
    let body = Zeroizing::new(body);
    let mut response = request_json_bytes(
        http,
        method,
        endpoint,
        bearer_token,
        &body,
        stage,
        external_write,
    )?;
    parse_feishu_response(&mut response, stage, external_write)
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn request_json_bytes(
    http: &HttpClient,
    method: &str,
    endpoint: &Url,
    bearer_token: Option<&str>,
    body: &[u8],
    stage: &'static str,
    external_write: bool,
) -> std::result::Result<crate::bounded_http::HttpResponse, MirrorFailure> {
    let mut headers = vec![
        (
            "Content-Type",
            "application/json; charset=utf-8".to_string(),
        ),
        ("Accept", "application/json".to_string()),
    ];
    if let Some(token) = bearer_token {
        headers.push(("Authorization", format!("Bearer {token}")));
    }
    let response = http.request(method, endpoint, &headers, body, MAX_FEISHU_RESPONSE_BYTES);
    for (_, value) in &mut headers {
        value.zeroize();
    }
    response.map_err(|error| {
        let transport = error.transport;
        let may_have_written = external_write && error.request_may_have_been_sent();
        let _ = error.into_source();
        MirrorFailure::external(
            stage,
            if transport {
                "transport_error"
            } else {
                "request_invalid"
            },
            if may_have_written { None } else { Some(false) },
            transport || may_have_written,
        )
    })
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn parse_feishu_response(
    response: &mut crate::bounded_http::HttpResponse,
    stage: &'static str,
    external_write: bool,
) -> std::result::Result<Value, MirrorFailure> {
    if !(200..300).contains(&response.status) {
        return Err(MirrorFailure::external(
            stage,
            "http_status_rejected",
            if external_write { None } else { Some(false) },
            true,
        ));
    }
    let parsed: Value = serde_json::from_slice(&response.body).map_err(|_| {
        MirrorFailure::external(
            stage,
            "response_invalid",
            if external_write { None } else { Some(false) },
            true,
        )
    })?;
    if parsed.get("code").and_then(Value::as_i64) != Some(0) {
        return Err(MirrorFailure::external(
            stage,
            "api_rejected",
            if external_write { None } else { Some(false) },
            true,
        ));
    }
    Ok(parsed)
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn read_feishu_credentials(path: &str) -> std::result::Result<FeishuCredentials, MirrorFailure> {
    validate_profile_env_path(path).map_err(|_| {
        MirrorFailure::external(
            "credential_load",
            "profile_path_invalid",
            Some(false),
            false,
        )
    })?;
    let canonical_path = fs::canonicalize(path).map_err(|_| {
        MirrorFailure::external("credential_load", "profile_read_failed", Some(false), false)
    })?;
    let canonical_path_text = canonical_path.to_str().ok_or_else(|| {
        MirrorFailure::external(
            "credential_load",
            "profile_path_invalid",
            Some(false),
            false,
        )
    })?;
    validate_profile_env_path(canonical_path_text).map_err(|_| {
        MirrorFailure::external(
            "credential_load",
            "profile_path_invalid",
            Some(false),
            false,
        )
    })?;
    let mut text = fs::read_to_string(&canonical_path).map_err(|_| {
        MirrorFailure::external("credential_load", "profile_read_failed", Some(false), false)
    })?;
    let mut app_id: Option<Zeroizing<String>> = None;
    let mut app_secret: Option<Zeroizing<String>> = None;
    let mut duplicate_key = false;
    for line in text.lines() {
        let trimmed = line.trim();
        if trimmed.is_empty() || trimmed.starts_with('#') {
            continue;
        }
        let Some((key, value)) = trimmed.split_once('=') else {
            continue;
        };
        let value = Zeroizing::new(
            value
                .trim()
                .trim_matches('"')
                .trim_matches('\'')
                .to_string(),
        );
        match key.trim() {
            "FEISHU_APP_ID" | "LARK_APP_ID" => {
                duplicate_key |= app_id.replace(value).is_some();
            }
            "FEISHU_APP_SECRET" | "LARK_APP_SECRET" => {
                duplicate_key |= app_secret.replace(value).is_some();
            }
            _ => {}
        }
    }
    text.zeroize();
    if duplicate_key {
        return Err(MirrorFailure::external(
            "credential_load",
            "credential_key_ambiguous",
            Some(false),
            false,
        ));
    }
    let app_id = app_id.filter(|value| !value.is_empty()).ok_or_else(|| {
        MirrorFailure::external("credential_load", "app_id_missing", Some(false), false)
    })?;
    let app_secret = app_secret
        .filter(|value| !value.is_empty())
        .ok_or_else(|| {
            MirrorFailure::external("credential_load", "app_secret_missing", Some(false), false)
        })?;
    Ok(FeishuCredentials { app_id, app_secret })
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn validate_media_bytes(
    artifact: &MirrorArtifact,
    validated: &ValidatedArtifact,
    bytes: &[u8],
) -> std::result::Result<(), MirrorFailure> {
    if bytes.len() != validated.byte_size
        || format!("sha256:{}", sha256_hex(bytes)) != artifact.content_hash
        || md5_hex(bytes) != validated.file_md5
    {
        return Err(MirrorFailure::external(
            "media_readback",
            "file_identity_mismatch",
            Some(false),
            false,
        ));
    }
    let image =
        image::load_from_memory_with_format(bytes, image::ImageFormat::Jpeg).map_err(|_| {
            MirrorFailure::external("media_readback", "jpeg_decode_failed", Some(false), false)
        })?;
    if image.dimensions() != (validated.width, validated.height) {
        return Err(MirrorFailure::external(
            "media_readback",
            "jpeg_dimensions_mismatch",
            Some(false),
            false,
        ));
    }
    Ok(())
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn multipart_image_body(
    boundary: &str,
    base_token: &str,
    artifact_id: Uuid,
    bytes: &[u8],
) -> Zeroizing<Vec<u8>> {
    let filename = format!("generated-image-{artifact_id}.jpg");
    let mut body = Zeroizing::new(Vec::with_capacity(bytes.len() + 1024));
    append_multipart_text(&mut body, boundary, "file_name", &filename);
    append_multipart_text(&mut body, boundary, "parent_type", "bitable_image");
    append_multipart_text(&mut body, boundary, "parent_node", base_token);
    append_multipart_text(&mut body, boundary, "size", &bytes.len().to_string());
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"file\"; filename=\"{filename}\"\r\n")
            .as_bytes(),
    );
    body.extend_from_slice(b"Content-Type: image/jpeg\r\n\r\n");
    body.extend_from_slice(bytes);
    body.extend_from_slice(format!("\r\n--{boundary}--\r\n").as_bytes());
    body
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter",
    feature = "huabaosi-feishu-mirror-adapter"
))]
fn append_multipart_text(body: &mut Vec<u8>, boundary: &str, name: &str, value: &str) {
    body.extend_from_slice(format!("--{boundary}\r\n").as_bytes());
    body.extend_from_slice(
        format!("Content-Disposition: form-data; name=\"{name}\"\r\n\r\n").as_bytes(),
    );
    body.extend_from_slice(value.as_bytes());
    body.extend_from_slice(b"\r\n");
}

#[cfg(test)]
mod tests {
    #[cfg(feature = "huabaosi-feishu-mirror-adapter")]
    use std::sync::{Mutex, MutexGuard, OnceLock};
    use std::{
        fs,
        io::{Read, Write},
        net::{TcpListener, TcpStream},
        thread,
    };

    use chrono::TimeZone;
    use image::{codecs::jpeg::JpegEncoder, DynamicImage, Rgb, RgbImage};
    use tempfile::tempdir;

    use super::*;
    #[cfg(feature = "postgres-integration-tests")]
    use crate::db;

    #[cfg(feature = "huabaosi-feishu-mirror-adapter")]
    struct EnvGuard {
        _lock: MutexGuard<'static, ()>,
        saved: Vec<(&'static str, Option<String>)>,
    }

    #[cfg(feature = "huabaosi-feishu-mirror-adapter")]
    impl EnvGuard {
        fn new(keys: &[&'static str]) -> Self {
            static ENV_LOCK: OnceLock<Mutex<()>> = OnceLock::new();
            let lock = ENV_LOCK
                .get_or_init(|| Mutex::new(()))
                .lock()
                .expect("env lock");
            let saved = keys
                .iter()
                .map(|key| (*key, env::var(key).ok()))
                .collect::<Vec<_>>();
            for key in keys {
                env::remove_var(key);
            }
            Self { _lock: lock, saved }
        }
    }

    #[cfg(feature = "huabaosi-feishu-mirror-adapter")]
    impl Drop for EnvGuard {
        fn drop(&mut self) {
            for (key, value) in &self.saved {
                if let Some(value) = value {
                    env::set_var(key, value);
                } else {
                    env::remove_var(key);
                }
            }
        }
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
        let parsed = Url::parse(&database_url).expect("integration database URL must parse");
        assert!(
            matches!(parsed.host_str(), Some("127.0.0.1" | "[::1]")),
            "PostgreSQL integration test requires a literal loopback host"
        );
        assert_eq!(
            parsed.path().trim_start_matches('/'),
            "qintopia_test",
            "PostgreSQL integration test may only use qintopia_test"
        );
        database_url
    }

    #[test]
    fn huabaosi_feishu_artifact_mirror_fixture_report_is_sanitized() {
        let report = fixture_report();
        let raw = serde_json::to_string(&report).expect("fixture report serializes");

        assert!(report.success);
        assert_eq!(report.action_status, "fixture_mirror_preview");
        assert!(!report.external_calls_executed);
        assert!(!report.database_writes_executed);
        for forbidden in [
            "base_token",
            "table_id",
            "file_token",
            "app_secret",
            "tenant_access_token",
            "artifact_uri",
        ] {
            assert!(
                !raw.contains(forbidden),
                "fixture report leaked {forbidden}"
            );
        }
    }

    #[cfg(feature = "huabaosi-feishu-mirror-adapter")]
    #[test]
    fn huabaosi_feishu_artifact_mirror_observation_preflight_uses_non_secret_boundary() {
        let mut keys = REQUIRED_CONFIGURATION_NAMES.to_vec();
        keys.push(ENABLE_ENV);
        let _env = EnvGuard::new(&keys);
        env::set_var(ENABLE_ENV, "1");
        env::set_var(DEPLOYED_COMMIT_SHA_ENV, "a".repeat(40));

        let report = observation_preflight_report();
        let serialized = serde_json::to_string(&report).expect("serialize observation preflight");

        assert!(report.success);
        assert!(report.adapter_compiled);
        assert!(report.mirror_enabled);
        assert!(!report.config_valid);
        assert_eq!(report.action_status, "observation_enabled_boundary_ready");
        assert!(report.missing_configuration.is_empty());
        assert_eq!(report.media_allowed_host_count, 0);
        assert!(!report.external_calls_executed);
        assert!(!report.database_writes_executed);
        for forbidden in [
            "postgres://",
            "base_token",
            "table_id",
            "tenant_access_token",
            "file_token",
        ] {
            assert!(!serialized.contains(forbidden), "leaked {forbidden}");
        }
    }

    #[test]
    fn huabaosi_feishu_artifact_mirror_requires_exact_production_release() {
        let sha = "a".repeat(40);
        assert!(validate_release_binding(&sha, &sha).is_ok());
        assert!(validate_release_binding(&sha, &"b".repeat(40)).is_err());
        assert!(validate_release_binding("release-main", "release-main").is_err());
        assert!(validate_release_binding(&"A".repeat(40), &"A".repeat(40)).is_err());
    }

    #[test]
    fn huabaosi_feishu_artifact_mirror_builds_fixed_version_fields() {
        let bytes = jpeg_fixture();
        let artifact = artifact_fixture(&bytes, "https://media.example.com/poster/final.jpg");
        let policy = MirrorPolicy::test_only("media.example.com", DEFAULT_MAX_MEDIA_BYTES);
        let validated = validate_artifact(&artifact, &policy).expect("artifact validates");
        let fields = build_feishu_fields(&artifact, &validated, Uuid::nil(), Some("fileFixture"));

        assert_eq!(fields["AgentOS产物ID"], artifact.id.to_string());
        assert_eq!(fields["图片请求ID"], artifact.work_item_id.to_string());
        assert_eq!(fields["审核状态"], "待审核");
        assert_eq!(fields["最终JPEG"][0]["file_token"], "fileFixture");
        assert_eq!(fields["MIME类型"], "image/jpeg");
        assert_eq!(fields["转换规则"], REQUIRED_TRANSFORM);
        assert!(fields.get("生成 Prompt").is_none());
    }

    #[test]
    fn huabaosi_feishu_artifact_mirror_matches_repository_schema_contract() {
        let schema: Value = serde_json::from_str(include_str!(
            "../../../mcp/feishu/config/huabaosi-generated-image-v1.json"
        ))
        .expect("schema contract parses");
        assert_eq!(schema["schema_version"], SCHEMA_VERSION);
        let declared = schema["fields"]
            .as_array()
            .expect("schema fields")
            .iter()
            .filter_map(|field| field["name"].as_str())
            .collect::<BTreeSet<_>>();

        let bytes = jpeg_fixture();
        let artifact = artifact_fixture(&bytes, "https://media.example.com/poster/final.jpg");
        let policy = MirrorPolicy::test_only("media.example.com", DEFAULT_MAX_MEDIA_BYTES);
        let validated = validate_artifact(&artifact, &policy).expect("artifact validates");
        let fields = build_feishu_fields(&artifact, &validated, Uuid::nil(), Some("fileFixture"));
        for field_name in fields.as_object().expect("mirror fields").keys() {
            assert!(
                declared.contains(field_name.as_str()),
                "worker field is absent from schema: {field_name}"
            );
        }
        for required in [
            "AgentOS产物ID",
            "AgentOS工作项ID",
            "图片请求ID",
            "最终JPEG",
            "JPEG SHA-256",
            "审核状态",
        ] {
            assert!(
                fields.get(required).is_some(),
                "missing required field {required}"
            );
        }
    }

    #[test]
    fn huabaosi_feishu_artifact_mirror_rejects_untrusted_or_drifted_artifacts() {
        let bytes = jpeg_fixture();
        let policy = MirrorPolicy::test_only("media.example.com", DEFAULT_MAX_MEDIA_BYTES);

        let mut wrong_host =
            artifact_fixture(&bytes, "https://untrusted.example.com/poster/final.jpg");
        assert!(validate_artifact(&wrong_host, &policy).is_err());

        wrong_host.artifact_uri =
            "https://media.example.com/poster%2Fprivate/final.jpg".to_string();
        assert!(validate_artifact(&wrong_host, &policy).is_err());

        let mut mismatched_audit =
            artifact_fixture(&bytes, "https://media.example.com/poster/final.jpg");
        mismatched_audit.creation_event_data["file_md5"] = json!("0".repeat(32));
        assert!(validate_artifact(&mismatched_audit, &policy).is_err());

        let mut noncanonical_hash =
            artifact_fixture(&bytes, "https://media.example.com/poster/final.jpg");
        noncanonical_hash.content_hash = format!("sha256:{}", "A".repeat(64));
        assert!(validate_artifact(&noncanonical_hash, &policy).is_err());

        let mut published = artifact_fixture(&bytes, "https://media.example.com/poster/final.jpg");
        published.creation_event_data["external_publish_executed"] = json!(true);
        let failure = validate_artifact(&published, &policy)
            .expect_err("published creation audit must fail closed");
        assert!(failure.to_string().contains("unpublished state"));
    }

    #[test]
    fn huabaosi_feishu_artifact_mirror_revalidates_exact_jpeg_bytes() {
        let bytes = jpeg_fixture();
        let artifact = artifact_fixture(&bytes, "https://media.example.com/poster/final.jpg");
        let policy = MirrorPolicy::test_only("media.example.com", DEFAULT_MAX_MEDIA_BYTES);
        let validated = validate_artifact(&artifact, &policy).expect("artifact validates");

        validate_media_bytes(&artifact, &validated, &bytes).expect("exact JPEG validates");
        let mut changed = bytes.clone();
        let last = changed.len() - 1;
        changed[last] ^= 1;
        let failure = validate_media_bytes(&artifact, &validated, &changed)
            .expect_err("changed JPEG must fail");
        assert_eq!(failure.stage, "media_readback");
        assert_eq!(failure.code, "file_identity_mismatch");
    }

    #[test]
    fn huabaosi_feishu_artifact_mirror_is_idempotent_by_artifact_search_before_create() {
        let bytes = jpeg_fixture();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake Feishu server");
        let address = listener.local_addr().expect("fake server address");
        let server_bytes = bytes.clone();
        let server = thread::spawn(move || {
            let expected = [
                "/open-apis/auth/v3/tenant_access_token/internal",
                "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records/search",
                "/media/final.jpg",
                "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records",
                "/open-apis/drive/v1/medias/upload_all",
                "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records/recFixture",
            ];
            for path in expected {
                let (mut stream, _) = listener.accept().expect("accept fake request");
                let request = read_http_request(&mut stream);
                let request_text = String::from_utf8_lossy(&request);
                assert!(
                    request_text
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .contains(path),
                    "unexpected request path: {}",
                    request_text.lines().next().unwrap_or_default()
                );
                let (content_type, body) = match path {
                    "/open-apis/auth/v3/tenant_access_token/internal" => (
                        "application/json",
                        br#"{"code":0,"tenant_access_token":"tenantFixture"}"#.to_vec(),
                    ),
                    "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records/search" => {
                        assert!(request_text.contains("AgentOS产物ID"));
                        ("application/json", br#"{"code":0,"data":{"items":[]}}"#.to_vec())
                    }
                    "/media/final.jpg" => ("image/jpeg", server_bytes.clone()),
                    "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records" => (
                        "application/json",
                        br#"{"code":0,"data":{"record":{"record_id":"recFixture"}}}"#.to_vec(),
                    ),
                    "/open-apis/drive/v1/medias/upload_all" => {
                        assert!(request_text.contains("bitable_image"));
                        assert!(contains_bytes(&request, &server_bytes));
                        (
                            "application/json",
                            br#"{"code":0,"data":{"file_token":"fileFixture"}}"#.to_vec(),
                        )
                    }
                    _ => {
                        assert!(request_text.contains("fileFixture"));
                        ("application/json", br#"{"code":0,"data":{}}"#.to_vec())
                    }
                };
                write_http_response(&mut stream, content_type, &body);
            }
        });

        let temp = tempdir().expect("temp credential root");
        let profile_dir = temp.path().join(".hermes/profiles/huabaosi");
        fs::create_dir_all(&profile_dir).expect("create Huabaosi profile dir");
        let profile_path = profile_dir.join(".env");
        fs::write(
            &profile_path,
            "FEISHU_APP_ID=appFixture\nFEISHU_APP_SECRET=secretFixture\n",
        )
        .expect("write fake credentials");

        let uri = format!("http://{address}/media/final.jpg");
        let artifact = artifact_fixture(&bytes, &uri);
        let policy = MirrorPolicy::test_only("127.0.0.1", DEFAULT_MAX_MEDIA_BYTES);
        let validated = validate_artifact(&artifact, &policy).expect("artifact validates");
        let config = MirrorConfig::test_only(
            policy,
            Url::parse(&format!("http://{address}/open-apis/")).expect("fake API root"),
            profile_path.to_string_lossy().to_string(),
        );

        let record_id =
            mirror_to_feishu(&artifact, &validated, Uuid::nil(), &config).expect("mirror succeeds");
        assert_eq!(record_id, "recFixture");
        server.join().expect("fake Feishu server completes");
    }

    #[test]
    #[cfg(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    ))]
    fn huabaosi_feishu_primary_storage_uploads_reads_back_and_upserts() {
        let bytes = jpeg_fixture();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake Feishu server");
        let address = listener.local_addr().expect("fake server address");
        let server_bytes = bytes.clone();
        let server = thread::spawn(move || {
            let expected = [
                "/open-apis/auth/v3/tenant_access_token/internal",
                "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records/search",
                "/open-apis/drive/v1/medias/upload_all",
                "/open-apis/drive/v1/medias/fileFixture/download",
                "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records",
            ];
            for path in expected {
                let (mut stream, _) = listener.accept().expect("accept fake request");
                let request = read_http_request(&mut stream);
                let request_text = String::from_utf8_lossy(&request);
                assert!(
                    request_text
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .contains(path),
                    "unexpected request path: {}",
                    request_text.lines().next().unwrap_or_default()
                );
                let (content_type, body) = match path {
                    "/open-apis/auth/v3/tenant_access_token/internal" => (
                        "application/json",
                        br#"{"code":0,"tenant_access_token":"tenantFixture"}"#.to_vec(),
                    ),
                    "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records/search" => {
                        assert!(request_text.contains("AgentOS产物ID"));
                        ("application/json", br#"{"code":0,"data":{"items":[]}}"#.to_vec())
                    }
                    "/open-apis/drive/v1/medias/upload_all" => {
                        assert!(contains_bytes(&request, &server_bytes));
                        (
                            "application/json",
                            br#"{"code":0,"data":{"file_token":"fileFixture"}}"#.to_vec(),
                        )
                    }
                    "/open-apis/drive/v1/medias/fileFixture/download" => {
                        assert!(request_text.contains("Authorization: Bearer tenantFixture"));
                        ("image/jpeg", server_bytes.clone())
                    }
                    _ => {
                        assert!(request_text.contains("fileFixture"));
                        assert!(request_text.contains("待审核"));
                        (
                            "application/json",
                            br#"{"code":0,"data":{"record":{"record_id":"recFixture"}}}"#
                                .to_vec(),
                        )
                    }
                };
                write_http_response(&mut stream, content_type, &body);
            }
        });

        let temp = tempdir().expect("temp credential root");
        let profile_dir = temp.path().join(".hermes/profiles/huabaosi");
        fs::create_dir_all(&profile_dir).expect("create Huabaosi profile dir");
        let profile_path = profile_dir.join(".env");
        fs::write(
            &profile_path,
            "FEISHU_APP_ID=appFixture\nFEISHU_APP_SECRET=secretFixture\n",
        )
        .expect("write fake credentials");

        let artifact_id = Uuid::new_v4();
        let config = FeishuPrimaryStorageConfig::test_only(
            Url::parse(&format!("http://{address}/open-apis/")).expect("fake API root"),
            profile_path.to_string_lossy().to_string(),
            DEFAULT_MAX_MEDIA_BYTES,
        );
        let result = store_primary_generated_image(
            &config,
            &FeishuPrimaryStorageImage {
                artifact_id,
                workflow_root_id: Uuid::new_v4(),
                work_item_id: Uuid::new_v4(),
                content_hash: &format!("sha256:{}", sha256_hex(&bytes)),
                file_md5: &md5_hex(&bytes),
                source_content_hash: &format!("sha256:{}", "a".repeat(64)),
                bytes: &bytes,
                width: REQUIRED_WIDTH as u32,
                height: REQUIRED_HEIGHT as u32,
            },
        )
        .expect("primary Feishu storage succeeds");

        assert_eq!(
            result.artifact_uri,
            format!("feishu-base://huabaosi-generated-image/{artifact_id}")
        );
        server.join().expect("fake Feishu server completes");
    }

    #[test]
    #[cfg(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    ))]
    fn huabaosi_feishu_primary_storage_revalidates_authenticated_attachment() {
        let bytes = jpeg_fixture();
        let mut artifact = artifact_fixture(&bytes, "feishu-base://pending");
        artifact.artifact_uri = format!("feishu-base://huabaosi-generated-image/{}", artifact.id);
        let validated =
            validate_primary_storage_artifact(&artifact).expect("Feishu-backed artifact validates");
        let fields = build_primary_storage_fields(
            &FeishuPrimaryStorageImage {
                artifact_id: artifact.id,
                workflow_root_id: Uuid::new_v4(),
                work_item_id: artifact.work_item_id,
                content_hash: &artifact.content_hash,
                file_md5: &validated.file_md5,
                source_content_hash: &validated.source_content_hash,
                bytes: &bytes,
                width: validated.width,
                height: validated.height,
            },
            "fileFixture",
        );
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake Feishu server");
        let address = listener.local_addr().expect("fake server address");
        let search_body = serde_json::to_vec(&json!({
            "code": 0,
            "data": {
                "items": [{
                    "record_id": "recFixture",
                    "fields": fields,
                }],
                "has_more": false,
            },
        }))
        .expect("serialize search body");
        let server_bytes = bytes.clone();
        let server = thread::spawn(move || {
            let expected = [
                "/open-apis/auth/v3/tenant_access_token/internal",
                "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records/search",
                "/open-apis/drive/v1/medias/fileFixture/download",
            ];
            for path in expected {
                let (mut stream, _) = listener.accept().expect("accept fake request");
                let request = read_http_request(&mut stream);
                let request_text = String::from_utf8_lossy(&request);
                assert!(
                    request_text
                        .lines()
                        .next()
                        .unwrap_or_default()
                        .contains(path),
                    "unexpected request path: {}",
                    request_text.lines().next().unwrap_or_default()
                );
                let (content_type, body) = match path {
                    "/open-apis/auth/v3/tenant_access_token/internal" => (
                        "application/json",
                        br#"{"code":0,"tenant_access_token":"tenantFixture"}"#.to_vec(),
                    ),
                    "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records/search" => {
                        assert!(request_text.contains("AgentOS产物ID"));
                        ("application/json", search_body.clone())
                    }
                    _ => {
                        assert!(request_text.contains("Authorization: Bearer tenantFixture"));
                        ("image/jpeg", server_bytes.clone())
                    }
                };
                write_http_response(&mut stream, content_type, &body);
            }
        });

        let temp = tempdir().expect("temp credential root");
        let profile_dir = temp.path().join(".hermes/profiles/huabaosi");
        fs::create_dir_all(&profile_dir).expect("create Huabaosi profile dir");
        let profile_path = profile_dir.join(".env");
        fs::write(
            &profile_path,
            "FEISHU_APP_ID=appFixture\nFEISHU_APP_SECRET=secretFixture\n",
        )
        .expect("write fake credentials");
        let config = FeishuPrimaryStorageConfig::test_only(
            Url::parse(&format!("http://{address}/open-apis/")).expect("fake API root"),
            profile_path.to_string_lossy().to_string(),
            DEFAULT_MAX_MEDIA_BYTES,
        );

        let report = revalidate_primary_storage_artifact(&artifact, &validated, &config)
            .expect("primary storage revalidation succeeds");
        let serialized = serde_json::to_string(&report).expect("serialize revalidation report");

        assert!(report.success);
        assert_eq!(report.action_status, "feishu_primary_storage_revalidated");
        assert_eq!(report.artifact_id, artifact.id);
        assert_eq!(report.work_item_id, artifact.work_item_id);
        assert_eq!(report.content_hash, artifact.content_hash);
        assert!(report.external_calls_executed);
        assert!(!report.database_writes_executed);
        assert!(report.sensitive_fields_redacted);
        for forbidden in [
            "fileFixture",
            "recFixture",
            "tenantFixture",
            "secretFixture",
            "baseTokenFixture",
            "tblFixture",
            "file_token",
            "record_id",
        ] {
            assert!(!serialized.contains(forbidden), "leaked {forbidden}");
        }
        server.join().expect("fake Feishu server completes");
    }

    #[test]
    #[cfg(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    ))]
    fn huabaosi_feishu_primary_storage_revalidation_fails_closed_on_drift() {
        let bytes = jpeg_fixture();
        let mut artifact = artifact_fixture(&bytes, "feishu-base://pending");
        artifact.artifact_uri = format!("feishu-base://huabaosi-generated-image/{}", artifact.id);
        let validated =
            validate_primary_storage_artifact(&artifact).expect("Feishu-backed artifact validates");
        let mut fields = build_primary_storage_fields(
            &FeishuPrimaryStorageImage {
                artifact_id: artifact.id,
                workflow_root_id: Uuid::new_v4(),
                work_item_id: artifact.work_item_id,
                content_hash: &artifact.content_hash,
                file_md5: &validated.file_md5,
                source_content_hash: &validated.source_content_hash,
                bytes: &bytes,
                width: validated.width,
                height: validated.height,
            },
            "fileFixture",
        );
        let fields = fields.as_object_mut().expect("primary storage fields");

        fields.insert(
            "JPEG SHA-256".to_string(),
            json!(format!("sha256:{}", "0".repeat(64))),
        );
        let failure = validate_primary_storage_record_fields(&artifact, &validated, fields)
            .expect_err("record hash drift must fail");
        assert_eq!(failure.stage, "record_search");
        assert_eq!(failure.code, "record_identity_mismatch");

        fields.insert(
            "JPEG SHA-256".to_string(),
            json!(artifact.content_hash.clone()),
        );
        fields.insert(
            "最终JPEG".to_string(),
            json!([{"file_token": "fileFixture"}, {"file_token": "fileFixture2"}]),
        );
        let failure =
            primary_storage_attachment_token(fields).expect_err("multiple attachments must fail");
        assert_eq!(failure.stage, "record_search");
        assert_eq!(failure.code, "attachment_count_invalid");

        fields.insert(
            "最终JPEG".to_string(),
            json!([{"file_token": "fileFixture"}]),
        );
        let mut changed = bytes.clone();
        let last = changed.len() - 1;
        changed[last] ^= 1;
        let failure = validate_primary_storage_readback(&artifact, &validated, &changed)
            .expect_err("changed readback bytes must fail");
        assert_eq!(failure.stage, "media_readback");
        assert_eq!(failure.code, "file_identity_mismatch");
    }

    #[test]
    #[cfg(not(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    )))]
    fn huabaosi_feishu_primary_storage_requires_compiled_adapter() {
        let bytes = jpeg_fixture();
        let config = FeishuPrimaryStorageConfig::test_only(
            Url::parse("https://open.feishu.cn/open-apis/").expect("fixed Feishu API root"),
            "/not/read/without/adapter".to_string(),
            DEFAULT_MAX_MEDIA_BYTES,
        );
        let error = match store_primary_generated_image(
            &config,
            &FeishuPrimaryStorageImage {
                artifact_id: Uuid::new_v4(),
                workflow_root_id: Uuid::new_v4(),
                work_item_id: Uuid::new_v4(),
                content_hash: &format!("sha256:{}", sha256_hex(&bytes)),
                file_md5: &md5_hex(&bytes),
                source_content_hash: &format!("sha256:{}", "a".repeat(64)),
                bytes: &bytes,
                width: REQUIRED_WIDTH as u32,
                height: REQUIRED_HEIGHT as u32,
            },
        ) {
            Ok(_) => panic!("default build accepted Feishu primary storage"),
            Err(error) => error,
        };

        assert!(error.to_string().contains("not compiled"));
    }

    #[test]
    fn huabaosi_feishu_artifact_mirror_multipart_contains_required_parent_and_exact_file() {
        let bytes = b"exact-jpeg-fixture";
        let artifact_id = Uuid::nil();
        let body = multipart_image_body("fixture-boundary", "baseFixture", artifact_id, bytes);
        let text = String::from_utf8_lossy(&body);

        assert!(text.contains("name=\"parent_type\"\r\n\r\nbitable_image"));
        assert!(text.contains("name=\"parent_node\"\r\n\r\nbaseFixture"));
        assert!(text.contains("Content-Type: image/jpeg"));
        assert!(contains_bytes(&body, bytes));
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
    async fn postgres_mirror_state_is_idempotent_and_redacted() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 1)
            .await
            .expect("connect guarded integration database");
        db::run_migrations(&pool)
            .await
            .expect("migrate guarded integration database");

        let suffix = Uuid::new_v4();
        let root_id = Uuid::new_v4();
        let request_id = Uuid::new_v4();
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (id, work_item_type, status, requester_agent, target_agent,
                 capability_key, brief_summary, source_type, dedupe_key,
                 idempotency_key, payload, review_policy)
            VALUES
                ($1, 'visual_asset_request', 'completed', 'xiaoman', 'huabaosi',
                 'huabaosi.create_visual_asset', 'Feishu mirror integration root',
                 'integration_test', $2, $2, '{}'::jsonb, 'before_external_use'),
                ($3, 'image_generation_request', 'awaiting_review', 'xiaoman', 'huabaosi',
                 'huabaosi.generate_image_asset', 'Feishu mirror integration request',
                 'integration_test', $4, $4, '{}'::jsonb, 'before_external_use')
            "#,
        )
        .bind(root_id)
        .bind(format!("huabaosi-feishu-root:{suffix}"))
        .bind(request_id)
        .bind(format!("huabaosi-feishu-request:{suffix}"))
        .execute(&pool)
        .await
        .expect("insert mirror work items");
        sqlx::query(
            "UPDATE qintopia_agent_os.work_items SET parent_work_item_id = $1 WHERE id = $2",
        )
        .bind(root_id)
        .bind(request_id)
        .execute(&pool)
        .await
        .expect("link mirror request to workflow root");

        let bytes = jpeg_fixture();
        let mut fixture =
            artifact_fixture(&bytes, "https://media.example.com/integration/final.jpg");
        fixture.work_item_id = request_id;
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.artifacts
                (id, work_item_id, artifact_type, review_status, created_by_agent,
                 title, artifact_uri, content_hash, source_ids, metadata,
                 created_at, updated_at)
            VALUES
                ($1, $2, 'generated_image', 'pending', 'huabaosi', $3, $4, $5,
                 $6, $7, $8, $9)
            "#,
        )
        .bind(fixture.id)
        .bind(fixture.work_item_id)
        .bind(&fixture.title)
        .bind(&fixture.artifact_uri)
        .bind(&fixture.content_hash)
        .bind(&fixture.source_ids)
        .bind(&fixture.metadata)
        .bind(fixture.created_at)
        .bind(fixture.updated_at)
        .execute(&pool)
        .await
        .expect("insert generated image fixture");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_item_events
                (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
            VALUES
                ($1, $2, 'generated_image_created', 'worker', $3,
                 'integration generated image created', $4)
            "#,
        )
        .bind(fixture.work_item_id)
        .bind(fixture.id)
        .bind(REQUIRED_GENERATOR)
        .bind(&fixture.creation_event_data)
        .execute(&pool)
        .await
        .expect("insert generated image creation event");

        let candidate = peek_candidate(&pool, Some(fixture.id))
            .await
            .expect("execute candidate SQL")
            .expect("load generated image candidate");
        let policy = MirrorPolicy::test_only("media.example.com", DEFAULT_MAX_MEDIA_BYTES);
        validate_artifact(&candidate, &policy).expect("candidate provenance validates");

        let mut tx = pool.begin().await.expect("begin mirror state transaction");
        let locked = lock_candidate(&mut tx, Some(fixture.id))
            .await
            .expect("execute locking candidate SQL")
            .expect("lock generated image candidate");
        assert_eq!(
            resolve_workflow_root_tx(&mut tx, locked.work_item_id)
                .await
                .expect("resolve recursive workflow root"),
            root_id
        );
        let first_ref = upsert_workbench_ref(&mut tx, &locked, "recSensitiveFixture", root_id)
            .await
            .expect("insert workbench reference");
        let second_ref = upsert_workbench_ref(&mut tx, &locked, "recSensitiveFixture", root_id)
            .await
            .expect("idempotently update workbench reference");
        assert_eq!(first_ref, second_ref);
        append_success_event(&mut tx, &locked, first_ref)
            .await
            .expect("append sanitized mirror event");
        tx.commit().await.expect("commit mirror state transaction");

        let state: (i64, Value, Value) = sqlx::query_as(
            r#"
            SELECT
                (
                    SELECT count(*)
                    FROM qintopia_agent_os.human_workbench_refs ref
                    WHERE ref.artifact_id = $1
                      AND ref.provider = $2
                ),
                (
                    SELECT ref.metadata
                    FROM qintopia_agent_os.human_workbench_refs ref
                    WHERE ref.artifact_id = $1
                      AND ref.provider = $2
                    LIMIT 1
                ),
                (
                    SELECT event.data
                    FROM qintopia_agent_os.work_item_events event
                    WHERE event.artifact_id = $1
                      AND event.event_type = 'generated_image_feishu_mirrored'
                    ORDER BY event.created_at DESC, event.id DESC
                    LIMIT 1
                )
            "#,
        )
        .bind(fixture.id)
        .bind(PROVIDER)
        .fetch_one(&pool)
        .await
        .expect("read mirror integration state");
        assert_eq!(state.0, 1);
        let serialized = format!("{}{}", state.1, state.2);
        assert!(!serialized.contains("recSensitiveFixture"));
        assert!(!serialized.contains("file_token"));
        assert!(!serialized.contains("base_token"));
        assert_eq!(state.1["schema_version"], SCHEMA_VERSION);
        assert_eq!(state.2["external_send_executed"], false);

        sqlx::query("DELETE FROM qintopia_agent_os.work_items WHERE id = $1")
            .bind(request_id)
            .execute(&pool)
            .await
            .expect("delete mirror integration request");
        sqlx::query("DELETE FROM qintopia_agent_os.work_items WHERE id = $1")
            .bind(root_id)
            .execute(&pool)
            .await
            .expect("delete mirror integration root");
    }

    fn artifact_fixture(bytes: &[u8], uri: &str) -> MirrorArtifact {
        let content_hash = format!("sha256:{}", sha256_hex(bytes));
        let file_md5 = md5_hex(bytes);
        let source_hash = format!("sha256:{}", "b".repeat(64));
        let metadata = json!({
            "generated_by": "huabaosi-image-generation-worker",
            "provider": "openai-compatible",
            "model": "gpt-image-2",
            "mime_type": REQUIRED_MIME_TYPE,
            "file_md5": file_md5,
            "provider_source_mime_type": REQUIRED_SOURCE_MIME_TYPE,
            "provider_source_content_hash": source_hash,
            "media_transform": REQUIRED_TRANSFORM,
            "jpeg_quality": 92,
            "alpha_background": "#ffffff",
            "width": REQUIRED_WIDTH,
            "height": REQUIRED_HEIGHT,
            "byte_size": bytes.len(),
            "approved_brief_artifact_id": Uuid::nil(),
            "approved_brief_content_hash": format!("sha256:{}", "c".repeat(64)),
            "prompt_hash": format!("sha256:{}", "d".repeat(64)),
        });
        let creation_event_data = json!({
            "content_hash": content_hash,
            "mime_type": REQUIRED_MIME_TYPE,
            "file_md5": file_md5,
            "provider_source_mime_type": REQUIRED_SOURCE_MIME_TYPE,
            "provider_source_content_hash": source_hash,
            "media_transform": REQUIRED_TRANSFORM,
            "jpeg_quality": 92,
            "alpha_background": "#ffffff",
            "width": REQUIRED_WIDTH,
            "height": REQUIRED_HEIGHT,
            "byte_size": bytes.len(),
            "external_publish_executed": false,
        });
        MirrorArtifact {
            id: Uuid::new_v4(),
            work_item_id: Uuid::new_v4(),
            review_status: "pending".to_string(),
            title: "活动海报图片（待审核）".to_string(),
            artifact_uri: uri.to_string(),
            content_hash,
            source_ids: json!([{
                "approved_brief_artifact_id": Uuid::nil(),
                "approved_brief_content_hash": format!("sha256:{}", "c".repeat(64)),
            }]),
            metadata,
            creation_event_data,
            reviewed_at: None,
            reviewed_by: None,
            review_decision_reason: None,
            created_at: Utc
                .with_ymd_and_hms(2026, 7, 15, 8, 0, 0)
                .single()
                .expect("fixture created time"),
            updated_at: Utc
                .with_ymd_and_hms(2026, 7, 15, 8, 1, 0)
                .single()
                .expect("fixture updated time"),
            last_synced_at: None,
            workbench_status: None,
        }
    }

    fn jpeg_fixture() -> Vec<u8> {
        let image = RgbImage::from_pixel(
            REQUIRED_WIDTH as u32,
            REQUIRED_HEIGHT as u32,
            Rgb([248, 248, 248]),
        );
        let mut bytes = Vec::new();
        JpegEncoder::new_with_quality(&mut bytes, 92)
            .encode_image(&DynamicImage::ImageRgb8(image))
            .expect("encode fixture JPEG");
        bytes
    }

    fn read_http_request(stream: &mut TcpStream) -> Vec<u8> {
        stream
            .set_read_timeout(Some(std::time::Duration::from_secs(5)))
            .expect("set fake read timeout");
        let mut request = Vec::new();
        let mut buffer = [0_u8; 8192];
        loop {
            let read = stream.read(&mut buffer).expect("read fake request");
            if read == 0 {
                break;
            }
            request.extend_from_slice(&buffer[..read]);
            let Some(header_end) = find_bytes(&request, b"\r\n\r\n") else {
                continue;
            };
            let headers = String::from_utf8_lossy(&request[..header_end]);
            let content_length = headers
                .lines()
                .find_map(|line| {
                    let (name, value) = line.split_once(':')?;
                    name.eq_ignore_ascii_case("content-length")
                        .then(|| value.trim().parse::<usize>().ok())
                        .flatten()
                })
                .unwrap_or_default();
            if request.len() >= header_end + 4 + content_length {
                break;
            }
        }
        request
    }

    fn write_http_response(stream: &mut TcpStream, content_type: &str, body: &[u8]) {
        write!(
            stream,
            "HTTP/1.1 200 OK\r\nContent-Type: {content_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .expect("write fake response headers");
        stream.write_all(body).expect("write fake response body");
        stream.flush().expect("flush fake response");
    }

    fn find_bytes(haystack: &[u8], needle: &[u8]) -> Option<usize> {
        haystack
            .windows(needle.len())
            .position(|window| window == needle)
    }

    fn contains_bytes(haystack: &[u8], needle: &[u8]) -> bool {
        find_bytes(haystack, needle).is_some()
    }
}
