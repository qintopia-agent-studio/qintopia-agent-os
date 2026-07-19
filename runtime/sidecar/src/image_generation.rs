use std::{collections::BTreeSet, fmt, io, time::Duration};

#[cfg(any(test, feature = "huabaosi-staging-adapter"))]
use std::net::IpAddr;

#[cfg(test)]
use std::collections::BTreeMap;

use anyhow::{anyhow, bail, Context, Result};
#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter"
))]
use base64ct::{Base64, Encoding};
use image::{
    codecs::jpeg::JpegEncoder, ExtendedColorType, GenericImageView, ImageFormat, ImageReader,
    Limits, RgbaImage,
};
use md5::Md5;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Postgres, Row, Transaction};
use url::Url;
use uuid::Uuid;
use zeroize::Zeroize;

use crate::{
    config::Cli,
    db,
    huabaosi_feishu_artifact_mirror::{
        primary_storage_missing_configuration, record_primary_storage_workbench_ref,
        FeishuPrimaryStorageConfig,
    },
    url_policy,
};

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter"
))]
use crate::huabaosi_feishu_artifact_mirror::{
    store_primary_generated_image, FeishuPrimaryStorageImage,
};

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter"
))]
use crate::huabaosi_feishu_artifact_mirror::resolve_workflow_root_pool;

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter"
))]
use crate::bounded_http::{HttpClient, HttpResponse};

#[cfg(test)]
use crate::bounded_http::{
    decode_chunked_body, parse_http_response, read_response_limited, request_io_error,
    validate_http_header, MAX_HTTP_RESPONSE_HEADER_BYTES,
};

const WORKER_ID: &str = "huabaosi-image-generation-worker";
const CAPABILITY_KEY: &str = "huabaosi.generate_image_asset";
const WORK_ITEM_TYPE: &str = "image_generation_request";
const SPECIFICATION: &str = "community_poster_1024x1024";
const IMAGE_SIZE: &str = "1024x1024";
const PROVIDER_SOURCE_MIME_TYPE: &str = "image/png";
const FINAL_IMAGE_MIME_TYPE: &str = "image/jpeg";
const MEDIA_TRANSFORM: &str = "png_to_jpeg_white_background_q92_v1";
const JPEG_QUALITY: u8 = 92;
const ALPHA_BACKGROUND: &str = "#ffffff";
const DEFAULT_MAX_MEDIA_BYTES: usize = 10 * 1024 * 1024;
const MAX_IMAGE_DECODER_ALLOC_BYTES: u64 = 32 * 1024 * 1024;
const MAX_MEDIA_UPLOAD_RESPONSE_BYTES: usize = 64 * 1024;
const PROVIDER_RESPONSE_OVERHEAD_BYTES: usize = 64 * 1024;
const MAX_GENERATION_ATTEMPTS: i32 = 3;
const BASE_RETRY_DELAY_SECONDS: i64 = 60;
const DEFAULT_IMAGE_HTTP_TIMEOUT_SECONDS: u64 = 180;
const MIN_IMAGE_HTTP_TIMEOUT_SECONDS: u64 = 60;
const MAX_IMAGE_HTTP_TIMEOUT_SECONDS: u64 = 240;
const IMAGE_HTTP_TIMEOUT_ENV: &str = "QINTOPIA_HUABAOSI_IMAGE_HTTP_TIMEOUT_SECONDS";
const _: () = assert!(MAX_IMAGE_HTTP_TIMEOUT_SECONDS < 10 * 60);
#[cfg(feature = "huabaosi-staging-adapter")]
const STAGING_APPROVAL_ENV: &str = "QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL";
#[cfg(any(test, feature = "huabaosi-staging-adapter"))]
const STAGING_APPROVAL_PHRASE: &str = "approved-staging-image-generation";
const PRODUCTION_APPROVAL_ENV: &str = "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_APPROVAL";
const PRODUCTION_APPROVAL_PHRASE: &str = "approved-production-image-generation";
const PRODUCTION_RELEASE_SHA_ENV: &str = "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA";
const PRODUCTION_DATABASE_URL_SHA256_ENV: &str =
    "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_DATABASE_URL_SHA256";
const DEPLOYED_COMMIT_SHA_ENV: &str = "QINTOPIA_DEPLOYED_COMMIT_SHA";
const STORAGE_BACKEND_ENV: &str = "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND";
const HTTP_STORAGE_BACKEND: &str = "http-media";
const FEISHU_STORAGE_BACKEND: &str = "feishu-base";
#[cfg(any(test, feature = "huabaosi-staging-adapter"))]
const REVIEWED_DATABASE_URL_SHA256_ALLOWLIST: &[&str] =
    &["c6dc2730b2a3fdabf05d88e021340b748c5c5b5d06d8ec24b38feef387d39330"];
const REQUIRED_IMAGE_CONFIGURATION: &[&str] = &[
    "QINTOPIA_HUABAOSI_IMAGE_PROVIDER",
    "QINTOPIA_HUABAOSI_IMAGE_MODEL",
    "QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL",
    "QINTOPIA_HUABAOSI_IMAGE_API_KEY",
];
const REQUIRED_HTTP_MEDIA_CONFIGURATION: &[&str] = &[
    "QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT",
    "QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
];

#[derive(Debug, Serialize)]
pub struct ImageGenerationWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub fixture_mode: bool,
    pub worker: &'static str,
    pub action_status: String,
    pub work_item_id: Option<Uuid>,
    pub artifact_ids: Vec<Uuid>,
    pub artifact_preview: Option<GeneratedImagePreview>,
    pub safe_for_chat: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct ImageGenerationPreflightReport {
    pub success: bool,
    pub worker: &'static str,
    pub action_status: &'static str,
    pub generation_enabled: bool,
    pub adapter_compiled: bool,
    pub adapter_mode: &'static str,
    pub config_valid: bool,
    pub media_allowed_host_count: usize,
    pub missing_configuration: Vec<&'static str>,
    pub safe_for_chat: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct GeneratedImagePreview {
    pub artifact_type: &'static str,
    pub review_status: &'static str,
    pub content_hash: String,
    pub mime_type: &'static str,
    pub width: u32,
    pub height: u32,
    pub byte_size: usize,
    pub image_specification: String,
}

#[derive(Debug, Clone)]
struct ImageGenerationWorkItem {
    id: Uuid,
    claim_token: Option<String>,
    attempts: i32,
    approved_brief_artifact_id: Uuid,
    approved_brief_content_hash: String,
    approved_brief_text: String,
    image_specification: String,
    prompt_hash: String,
}

#[derive(Debug, Clone)]
struct AdapterConfig {
    model: String,
    provider_endpoint: Url,
    api_key: String,
    storage: StorageConfig,
    max_media_bytes: usize,
    http_timeout: Duration,
}

#[derive(Debug, Clone)]
enum StorageConfig {
    Http(HttpMediaConfig),
    Feishu(FeishuPrimaryStorageConfig),
}

#[derive(Debug, Clone)]
struct HttpMediaConfig {
    media_upload_endpoint: Url,
    media_public_base_url: Url,
    media_allowed_hosts: BTreeSet<String>,
}

impl Drop for AdapterConfig {
    fn drop(&mut self) {
        self.api_key.zeroize();
    }
}

#[derive(Debug)]
struct GeneratedImage {
    artifact_id: Uuid,
    workflow_root_id: Uuid,
    bytes: Vec<u8>,
    content_hash: String,
    file_md5: String,
    provider_source_content_hash: String,
    width: u32,
    height: u32,
    artifact_uri: String,
    storage_provider: &'static str,
    feishu_record_id: Option<String>,
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter"
))]
#[derive(Debug, Deserialize)]
struct ProviderResponse {
    data: Vec<ProviderImage>,
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter"
))]
#[derive(Debug, Deserialize)]
struct ProviderImage {
    b64_json: Option<String>,
}

#[derive(Debug, Deserialize)]
struct MediaUploadResponse {
    uri: String,
    content_hash: String,
    mime_type: String,
    byte_size: usize,
    width: u32,
    height: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum GenerationFailureClass {
    RetryableProvider,
    AmbiguousProvider,
    Terminal,
}

impl GenerationFailureClass {
    fn as_str(self) -> &'static str {
        match self {
            Self::RetryableProvider => "retryable_provider",
            Self::AmbiguousProvider => "ambiguous_provider",
            Self::Terminal => "terminal",
        }
    }
}

#[derive(Debug)]
struct GenerationAttemptError {
    class: GenerationFailureClass,
    stage: &'static str,
    source: anyhow::Error,
}

type GenerationAttemptResult<T> = std::result::Result<T, GenerationAttemptError>;

impl GenerationAttemptError {
    fn retryable_provider(stage: &'static str, source: anyhow::Error) -> Self {
        Self {
            class: GenerationFailureClass::RetryableProvider,
            stage,
            source,
        }
    }

    fn ambiguous_provider(stage: &'static str, source: anyhow::Error) -> Self {
        Self {
            class: GenerationFailureClass::AmbiguousProvider,
            stage,
            source,
        }
    }

    fn terminal(stage: &'static str, source: anyhow::Error) -> Self {
        Self {
            class: GenerationFailureClass::Terminal,
            stage,
            source,
        }
    }

    fn failure(&self) -> GenerationFailure {
        GenerationFailure {
            class: self.class,
            stage: self.stage,
        }
    }
}

impl fmt::Display for GenerationAttemptError {
    fn fmt(&self, formatter: &mut fmt::Formatter<'_>) -> fmt::Result {
        self.source.fmt(formatter)
    }
}

#[derive(Debug, Clone, Copy)]
struct GenerationFailure {
    class: GenerationFailureClass,
    stage: &'static str,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum FailureRecordOutcome {
    RetryScheduled,
    RetryExhausted,
    Ambiguous,
    Failed,
    StaleClaim,
}

enum ImageGenerationClaimOutcome {
    Claimed(ImageGenerationWorkItem),
    ReconciledAmbiguous(Uuid),
    Empty,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum ImageAdapterMode {
    Disabled,
    Staging,
    Production,
    Invalid,
}

impl ImageAdapterMode {
    const fn as_str(self) -> &'static str {
        match self {
            Self::Disabled => "disabled",
            Self::Staging => "staging",
            Self::Production => "production",
            Self::Invalid => "invalid",
        }
    }

    const fn is_compiled(self) -> bool {
        matches!(self, Self::Staging | Self::Production)
    }
}

pub async fn run(
    cli: &Cli,
    once: bool,
    work_item_id: Option<Uuid>,
    apply: bool,
    dry_run: bool,
    fixture_mode: bool,
) -> Result<()> {
    if apply && dry_run {
        bail!("use either --apply or --dry-run, not both");
    }
    if !once {
        bail!("Huabaosi image generation worker currently supports --once only");
    }

    let apply_requested = apply && !dry_run;
    let adapter_mode = huabaosi_adapter_mode();
    if apply_requested && !adapter_mode.is_compiled() {
        let action_status = if adapter_mode == ImageAdapterMode::Invalid {
            "adapter_feature_conflict"
        } else {
            "live_adapter_not_compiled"
        };
        let report = report(
            false,
            true,
            false,
            action_status,
            work_item_id,
            Vec::new(),
            None,
        );
        println!("{}", serde_json::to_string_pretty(&report)?);
        if adapter_mode == ImageAdapterMode::Invalid {
            bail!("Huabaosi live adapter features conflict");
        }
        bail!("Huabaosi live adapter is not compiled into this binary");
    }

    let report = if fixture_mode {
        if apply {
            bail!("fixture-mode cannot be used with --apply");
        }
        fixture_report()
    } else {
        let database_url = cli.database_url_required()?;
        let adapter_config = if apply_requested && image_generation_enabled() {
            Some(live_apply_config(adapter_mode, database_url)?)
        } else {
            None
        };
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        run_once(&pool, apply_requested, work_item_id, adapter_config).await?
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
}

fn live_apply_config(mode: ImageAdapterMode, _database_url: &str) -> Result<AdapterConfig> {
    match mode {
        #[cfg(feature = "huabaosi-staging-adapter")]
        ImageAdapterMode::Staging => staging_apply_config(_database_url),
        #[cfg(feature = "huabaosi-production-adapter")]
        ImageAdapterMode::Production => production_apply_config(_database_url),
        ImageAdapterMode::Invalid => bail!("Huabaosi live adapter features conflict"),
        _ => bail!("Huabaosi live adapter is not compiled into this binary"),
    }
}

pub fn run_preflight() -> Result<()> {
    let adapter_mode = huabaosi_adapter_mode();
    let report = match preflight_adapter_config(adapter_mode) {
        Ok(config) => preflight_report(
            true,
            image_generation_enabled(),
            adapter_mode,
            config.media_allowed_host_count(),
            Vec::new(),
        ),
        Err(_) => preflight_report(
            false,
            image_generation_enabled(),
            adapter_mode,
            0,
            missing_image_configuration(),
        ),
    };
    println!("{}", serde_json::to_string_pretty(&report)?);
    ensure_preflight_success(&report)
}

fn preflight_adapter_config(mode: ImageAdapterMode) -> Result<AdapterConfig> {
    let database_url = std::env::var("QINTOPIA_SIDECAR_DATABASE_URL").unwrap_or_default();
    let config = AdapterConfig::from_env(&database_url)?;
    if mode == ImageAdapterMode::Production {
        let database_url = required_env("QINTOPIA_SIDECAR_DATABASE_URL")?;
        validate_production_owner_approval(std::env::var(PRODUCTION_APPROVAL_ENV).ok().as_deref())?;
        validate_production_release_boundary(
            std::env::var(DEPLOYED_COMMIT_SHA_ENV).ok().as_deref(),
            std::env::var(PRODUCTION_RELEASE_SHA_ENV).ok().as_deref(),
        )?;
        validate_production_database_boundary(
            &database_url,
            std::env::var(PRODUCTION_DATABASE_URL_SHA256_ENV)
                .ok()
                .as_deref(),
        )?;
    }
    Ok(config)
}

fn ensure_preflight_success(report: &ImageGenerationPreflightReport) -> Result<()> {
    if report.success {
        return Ok(());
    }
    bail!("image adapter preflight configuration is invalid")
}

fn preflight_report(
    config_valid: bool,
    generation_enabled: bool,
    adapter_mode: ImageAdapterMode,
    media_allowed_host_count: usize,
    missing_configuration: Vec<&'static str>,
) -> ImageGenerationPreflightReport {
    let adapter_compiled = adapter_mode.is_compiled();
    let feature_state_valid =
        generation_enabled == adapter_compiled && adapter_mode != ImageAdapterMode::Invalid;
    ImageGenerationPreflightReport {
        success: config_valid && feature_state_valid,
        worker: WORKER_ID,
        action_status: if !config_valid {
            "adapter_not_configured"
        } else if adapter_mode == ImageAdapterMode::Invalid {
            "adapter_feature_conflict"
        } else if generation_enabled && !adapter_compiled {
            "live_adapter_not_compiled"
        } else if adapter_compiled && !generation_enabled {
            "live_adapter_compiled_requires_owner_review"
        } else {
            "adapter_config_ready"
        },
        generation_enabled,
        adapter_compiled,
        adapter_mode: adapter_mode.as_str(),
        config_valid,
        media_allowed_host_count,
        missing_configuration,
        safe_for_chat: false,
        limitations: vec![
            "this preflight validates local configuration only; it does not prove provider or storage reachability".to_string(),
            "a ready configuration does not authorize publishing, artifact approval, or QiWe sends".to_string(),
        ],
        guardrails: vec![
            "this command does not open network or database connections".to_string(),
            "credentials, service addresses, hosts, prompts, Feishu identifiers, and QiWe payloads are not emitted".to_string(),
            "live generation requires exactly one reviewed adapter feature plus explicit environment and command-entry approval".to_string(),
        ],
    }
}

fn fixture_report() -> ImageGenerationWorkerReport {
    let work_item = fixture_work_item();
    report(
        true,
        false,
        true,
        "fixture_image_generation_preview",
        Some(work_item.id),
        Vec::new(),
        Some(image_preview(&work_item)),
    )
}

fn fixture_work_item() -> ImageGenerationWorkItem {
    ImageGenerationWorkItem {
        id: Uuid::nil(),
        claim_token: None,
        attempts: 0,
        approved_brief_artifact_id: Uuid::nil(),
        approved_brief_content_hash: "sha256:fixture-approved-brief".to_string(),
        approved_brief_text: "活动主题：fixture 活动。".to_string(),
        image_specification: SPECIFICATION.to_string(),
        prompt_hash: "sha256:fixture-prompt".to_string(),
    }
}

async fn run_once(
    pool: &PgPool,
    apply_requested: bool,
    work_item_id: Option<Uuid>,
    adapter_config: Option<AdapterConfig>,
) -> Result<ImageGenerationWorkerReport> {
    if !apply_requested {
        let Some(work_item) = load_work_item(pool, work_item_id).await? else {
            return Ok(report(
                true,
                false,
                false,
                "no_claimable_image_request",
                None,
                Vec::new(),
                None,
            ));
        };
        return Ok(report(
            true,
            false,
            false,
            "image_generation_preview",
            Some(work_item.id),
            Vec::new(),
            Some(image_preview(&work_item)),
        ));
    }

    if !image_generation_enabled() {
        let Some(work_item) = load_work_item(pool, work_item_id).await? else {
            return Ok(report(
                true,
                true,
                false,
                "no_claimable_image_request",
                None,
                Vec::new(),
                None,
            ));
        };
        return Ok(report(
            true,
            true,
            false,
            "image_generation_disabled",
            Some(work_item.id),
            Vec::new(),
            Some(image_preview(&work_item)),
        ));
    }

    let Some(config) = adapter_config else {
        let work_item = load_work_item(pool, work_item_id).await?;
        return Ok(report(
            false,
            true,
            false,
            "adapter_not_configured",
            work_item.as_ref().map(|item| item.id),
            Vec::new(),
            work_item.as_ref().map(image_preview),
        ));
    };

    #[cfg(not(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter"
    )))]
    {
        drop(config);
        Ok(report(
            false,
            true,
            false,
            "live_adapter_not_compiled",
            None,
            Vec::new(),
            None,
        ))
    }

    #[cfg(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter"
    ))]
    {
        let work_item = match claim_work_item(pool, work_item_id).await? {
            ImageGenerationClaimOutcome::Claimed(work_item) => work_item,
            ImageGenerationClaimOutcome::ReconciledAmbiguous(stale_id) => {
                return Ok(report(
                    false,
                    true,
                    false,
                    "image_generation_outcome_ambiguous",
                    Some(stale_id),
                    Vec::new(),
                    None,
                ))
            }
            ImageGenerationClaimOutcome::Empty => {
                return Ok(report(
                    true,
                    true,
                    false,
                    "no_claimable_image_request",
                    None,
                    Vec::new(),
                    None,
                ))
            }
        };
        let work_item_id = work_item.id;
        let workflow_root_id = match resolve_workflow_root_pool(pool, work_item.id).await {
            Ok(workflow_root_id) => workflow_root_id,
            Err(_) => {
                return record_generation_failure_report(
                    pool,
                    &work_item,
                    GenerationFailure {
                        class: GenerationFailureClass::Terminal,
                        stage: "workflow_root_resolution",
                    },
                )
                .await
            }
        };
        let worker_input = work_item.clone();
        let generated = match tokio::task::spawn_blocking(move || {
            generate_and_store(&config, &worker_input, workflow_root_id)
        })
        .await
        {
            Ok(generated) => generated,
            Err(_) => {
                return record_generation_failure_report(
                    pool,
                    &work_item,
                    GenerationFailure {
                        class: GenerationFailureClass::Terminal,
                        stage: "worker_execution",
                    },
                )
                .await
            }
        };

        let generated = match generated {
            Ok(generated) => generated,
            Err(error) => {
                return record_generation_failure_report(pool, &work_item, error.failure()).await
            }
        };

        let persisted = persist_generated_image(pool, &work_item, &generated).await;
        match persisted {
            Ok(artifact_id) => Ok(report(
                true,
                true,
                false,
                "generated_image_created",
                Some(work_item_id),
                vec![artifact_id],
                Some(generated.preview()),
            )),
            Err(_) => {
                record_generation_failure_report(
                    pool,
                    &work_item,
                    GenerationFailure {
                        class: GenerationFailureClass::Terminal,
                        stage: "persistence",
                    },
                )
                .await
            }
        }
    }
}

#[cfg(feature = "huabaosi-staging-adapter")]
fn staging_apply_config(database_url: &str) -> Result<AdapterConfig> {
    validate_staging_owner_approval(std::env::var(STAGING_APPROVAL_ENV).ok().as_deref())?;
    let disposable_test = disposable_postgres_smoke_enabled();
    validate_staging_database_boundary(database_url, disposable_test)?;
    let config = AdapterConfig::from_env(database_url)
        .context("Huabaosi staging adapter configuration is invalid")?;
    if disposable_test {
        validate_disposable_test_adapter_boundary(&config)?;
    }
    Ok(config)
}

#[cfg(feature = "huabaosi-production-adapter")]
fn production_apply_config(database_url: &str) -> Result<AdapterConfig> {
    validate_production_owner_approval(std::env::var(PRODUCTION_APPROVAL_ENV).ok().as_deref())?;
    validate_production_release_boundary(
        std::env::var(DEPLOYED_COMMIT_SHA_ENV).ok().as_deref(),
        std::env::var(PRODUCTION_RELEASE_SHA_ENV).ok().as_deref(),
    )?;
    validate_production_database_boundary(
        database_url,
        std::env::var(PRODUCTION_DATABASE_URL_SHA256_ENV)
            .ok()
            .as_deref(),
    )?;
    AdapterConfig::from_env(database_url)
        .context("Huabaosi production adapter configuration is invalid")
}

#[cfg(any(test, feature = "huabaosi-staging-adapter"))]
fn validate_staging_owner_approval(value: Option<&str>) -> Result<()> {
    if value != Some(STAGING_APPROVAL_PHRASE) {
        bail!("Huabaosi staging owner approval is required");
    }
    Ok(())
}

fn validate_production_owner_approval(value: Option<&str>) -> Result<()> {
    if value != Some(PRODUCTION_APPROVAL_PHRASE) {
        bail!("Huabaosi production owner approval is required");
    }
    Ok(())
}

fn validate_production_release_boundary(
    deployed_sha: Option<&str>,
    approved_sha: Option<&str>,
) -> Result<()> {
    let deployed_sha = deployed_sha.context("deployed release SHA is required")?;
    let approved_sha = approved_sha.context("approved production release SHA is required")?;
    if !is_lower_hex(deployed_sha, 40) || !is_lower_hex(approved_sha, 40) {
        bail!("production release SHA must be a 40-character lowercase Git SHA");
    }
    if deployed_sha != approved_sha {
        bail!("production image generation is not approved for the deployed release");
    }
    Ok(())
}

fn validate_production_database_boundary(
    database_url: &str,
    approved_hash: Option<&str>,
) -> Result<()> {
    let approved_hash =
        approved_hash.context("approved production database URL hash is required")?;
    if !is_lower_hex(approved_hash, 64) {
        bail!("production database URL hash must be lowercase SHA-256");
    }
    let actual_hash = format!("{:x}", Sha256::digest(database_url.as_bytes()));
    if actual_hash != approved_hash {
        bail!("production database URL hash does not match the approved boundary");
    }

    let parsed = Url::parse(database_url).context("parse production database URL")?;
    if !matches!(parsed.scheme(), "postgres" | "postgresql") || parsed.host_str().is_none() {
        bail!("production database URL must use PostgreSQL and include a host");
    }
    let database_name = parsed
        .path()
        .strip_prefix('/')
        .filter(|value| !value.is_empty() && !value.contains('/'))
        .ok_or_else(|| anyhow!("production database URL must name exactly one database"))?;
    let normalized_name = database_name.to_ascii_lowercase();
    if normalized_name.contains("staging")
        || normalized_name.contains("test")
        || normalized_name.contains("dev")
    {
        bail!("production image generation rejects non-production database names");
    }
    Ok(())
}

fn is_lower_hex(value: &str, length: usize) -> bool {
    value.len() == length
        && value
            .bytes()
            .all(|byte| byte.is_ascii_digit() || (b'a'..=b'f').contains(&byte))
}

#[cfg(any(test, feature = "huabaosi-staging-adapter"))]
fn validate_staging_database_boundary(
    database_url: &str,
    allow_disposable_test: bool,
) -> Result<()> {
    validate_staging_database_boundary_with_allowlist(
        database_url,
        allow_disposable_test,
        REVIEWED_DATABASE_URL_SHA256_ALLOWLIST,
    )
}

#[cfg(any(test, feature = "huabaosi-staging-adapter"))]
fn validate_staging_database_boundary_with_allowlist(
    database_url: &str,
    allow_disposable_test: bool,
    reviewed_hashes: &[&str],
) -> Result<()> {
    let database_hash = format!("{:x}", Sha256::digest(database_url.as_bytes()));
    if !reviewed_hashes.iter().any(|hash| *hash == database_hash) {
        bail!("database URL hash is not in the reviewed allowlist");
    }
    let parsed = Url::parse(database_url).context("parse staging database URL")?;
    if !matches!(parsed.scheme(), "postgres" | "postgresql") || parsed.host_str().is_none() {
        bail!("staging database URL must use PostgreSQL and include a host");
    }
    let database_name = parsed
        .path()
        .strip_prefix('/')
        .filter(|value| !value.is_empty() && !value.contains('/'))
        .ok_or_else(|| anyhow!("staging database URL must name exactly one database"))?;
    if allow_disposable_test
        && database_name == "qintopia_test"
        && parsed.query().is_none()
        && parsed.host_str().is_some_and(is_literal_loopback_host)
    {
        return Ok(());
    }
    if !database_name.to_ascii_lowercase().contains("staging") {
        bail!("database must be staging or the guarded loopback qintopia_test fixture");
    }
    Ok(())
}

#[cfg(feature = "huabaosi-staging-adapter")]
fn disposable_postgres_smoke_enabled() -> bool {
    cfg!(feature = "postgres-integration-tests")
        && std::env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE")
            .is_ok_and(|value| value.trim() == "1")
}

#[cfg(any(test, feature = "huabaosi-staging-adapter"))]
fn validate_disposable_test_adapter_boundary(config: &AdapterConfig) -> Result<()> {
    let StorageConfig::Http(storage) = &config.storage else {
        bail!("disposable image adapter requires loopback HTTP media storage");
    };
    for endpoint in [
        &config.provider_endpoint,
        &storage.media_upload_endpoint,
        &storage.media_public_base_url,
    ] {
        if !endpoint.host_str().is_some_and(is_literal_loopback_host) {
            bail!("disposable image adapter endpoints must be loopback-only");
        }
    }
    if storage
        .media_allowed_hosts
        .iter()
        .any(|host| !is_literal_loopback_host(host))
    {
        bail!("disposable image media allowlist must be loopback-only");
    }
    Ok(())
}

#[cfg(any(test, feature = "huabaosi-staging-adapter"))]
fn is_literal_loopback_host(host: &str) -> bool {
    host.parse::<IpAddr>()
        .is_ok_and(|address| address.is_loopback())
}

async fn record_generation_failure_report(
    pool: &PgPool,
    work_item: &ImageGenerationWorkItem,
    failure: GenerationFailure,
) -> Result<ImageGenerationWorkerReport> {
    let action_status = match record_generation_failure(pool, work_item, failure).await? {
        FailureRecordOutcome::RetryScheduled => "image_generation_retry_scheduled",
        FailureRecordOutcome::RetryExhausted => "image_generation_retry_exhausted",
        FailureRecordOutcome::Ambiguous => "image_generation_outcome_ambiguous",
        FailureRecordOutcome::Failed => "image_generation_failed",
        FailureRecordOutcome::StaleClaim => "image_generation_stale_claim",
    };
    Ok(report(
        false,
        true,
        false,
        action_status,
        Some(work_item.id),
        Vec::new(),
        None,
    ))
}

impl AdapterConfig {
    fn from_env(database_url: &str) -> Result<Self> {
        let provider = required_env("QINTOPIA_HUABAOSI_IMAGE_PROVIDER")?;
        if provider != "openai-compatible" {
            bail!("image provider must be openai-compatible");
        }
        let model = required_env("QINTOPIA_HUABAOSI_IMAGE_MODEL")?;
        if model != "gpt-image-2" {
            bail!("image model must be gpt-image-2");
        }
        let provider_endpoint =
            image_provider_endpoint(&required_env("QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL")?)?;
        let http_timeout =
            image_http_timeout(std::env::var(IMAGE_HTTP_TIMEOUT_ENV).ok().as_deref())?;
        let max_media_bytes = std::env::var("QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.parse::<usize>().context("parse media max bytes"))
            .transpose()?
            .unwrap_or(DEFAULT_MAX_MEDIA_BYTES);
        if max_media_bytes == 0 || max_media_bytes > 25 * 1024 * 1024 {
            bail!("media max bytes must be between 1 and 26214400");
        }
        let storage_backend =
            std::env::var(STORAGE_BACKEND_ENV).unwrap_or_else(|_| HTTP_STORAGE_BACKEND.to_string());
        let storage = match storage_backend.trim() {
            HTTP_STORAGE_BACKEND => {
                let media_upload_endpoint = https_url(
                    &required_env("QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT")?,
                    "media upload endpoint",
                )?;
                let media_public_base_url = https_url(
                    &required_env("QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL")?,
                    "media public base URL",
                )?;
                let media_allowed_hosts =
                    parse_allowed_hosts(&required_env("QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS")?)?;
                for url in [&media_upload_endpoint, &media_public_base_url] {
                    let host = normalized_url_host(url)?;
                    if !media_allowed_hosts.contains(&host) {
                        bail!("media endpoint host is not allowlisted");
                    }
                }
                StorageConfig::Http(HttpMediaConfig {
                    media_upload_endpoint,
                    media_public_base_url,
                    media_allowed_hosts,
                })
            }
            FEISHU_STORAGE_BACKEND => {
                let config = FeishuPrimaryStorageConfig::from_env(database_url)?;
                if config.max_media_bytes() != max_media_bytes {
                    bail!("image and Feishu storage byte limits must match");
                }
                StorageConfig::Feishu(config)
            }
            _ => bail!("image storage backend must be http-media or feishu-base"),
        };

        Ok(Self {
            model,
            provider_endpoint,
            api_key: required_env("QINTOPIA_HUABAOSI_IMAGE_API_KEY")?,
            storage,
            max_media_bytes,
            http_timeout,
        })
    }

    fn media_allowed_host_count(&self) -> usize {
        match &self.storage {
            StorageConfig::Http(config) => config.media_allowed_hosts.len(),
            StorageConfig::Feishu(_) => 0,
        }
    }
}

fn image_http_timeout(value: Option<&str>) -> Result<Duration> {
    let seconds = value
        .map(str::trim)
        .filter(|value| !value.is_empty())
        .map(|value| {
            value
                .parse::<u64>()
                .context("parse image HTTP timeout seconds")
        })
        .transpose()?
        .unwrap_or(DEFAULT_IMAGE_HTTP_TIMEOUT_SECONDS);
    if !(MIN_IMAGE_HTTP_TIMEOUT_SECONDS..=MAX_IMAGE_HTTP_TIMEOUT_SECONDS).contains(&seconds) {
        bail!("image HTTP timeout seconds must be between 60 and 240");
    }
    Ok(Duration::from_secs(seconds))
}

fn required_env(name: &str) -> Result<String> {
    let value = std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !is_placeholder(value))
        .ok_or_else(|| anyhow!("required image adapter configuration is missing"))?;
    Ok(value)
}

fn missing_image_configuration() -> Vec<&'static str> {
    let mut missing = missing_required_configuration_with(REQUIRED_IMAGE_CONFIGURATION, |name| {
        std::env::var(name).ok()
    });
    match std::env::var(STORAGE_BACKEND_ENV).as_deref() {
        Ok(FEISHU_STORAGE_BACKEND) => missing.extend(primary_storage_missing_configuration()),
        _ => missing.extend(missing_required_configuration_with(
            REQUIRED_HTTP_MEDIA_CONFIGURATION,
            |name| std::env::var(name).ok(),
        )),
    }
    missing
}

fn missing_required_configuration_with<F>(
    required: &'static [&'static str],
    mut read: F,
) -> Vec<&'static str>
where
    F: FnMut(&str) -> Option<String>,
{
    required
        .iter()
        .copied()
        .filter(|name| {
            read(name)
                .map(|value| value.trim().to_string())
                .is_none_or(|value| value.is_empty() || is_placeholder(&value))
        })
        .collect()
}

fn is_placeholder(value: &str) -> bool {
    let normalized = value.trim().to_ascii_lowercase();
    normalized.contains("replace-with") || normalized == "change-me" || normalized == "placeholder"
}

fn image_provider_endpoint(base_url: &str) -> Result<Url> {
    let mut base = https_url(base_url, "image provider base URL")?;
    let path = base.path().trim_end_matches('/');
    base.set_path(&format!("{path}/"));
    base.join("images/generations")
        .context("build image provider endpoint")
}

fn https_url(value: &str, label: &str) -> Result<Url> {
    url_policy::reject_path_separator_ambiguity(value, label)?;
    let url = Url::parse(value).with_context(|| format!("parse {label}"))?;
    if url.scheme() != "https"
        || url.host_str().is_none()
        || !url.username().is_empty()
        || url.password().is_some()
    {
        bail!("{label} must be an HTTPS URL without user credentials");
    }
    if url.query().is_some() || url.fragment().is_some() {
        bail!("{label} must not contain a query or fragment");
    }
    Ok(url)
}

fn parse_allowed_hosts(value: &str) -> Result<BTreeSet<String>> {
    let hosts = value
        .split(',')
        .map(normalize_host)
        .filter(|host| !host.is_empty())
        .collect::<BTreeSet<_>>();
    if hosts.is_empty() {
        bail!("media allowed hosts must not be empty");
    }
    Ok(hosts)
}

fn normalize_host(value: &str) -> String {
    value.trim().trim_end_matches('.').to_ascii_lowercase()
}

fn normalized_url_host(url: &Url) -> Result<String> {
    url.host_str()
        .map(normalize_host)
        .filter(|host| !host.is_empty())
        .ok_or_else(|| anyhow!("URL host is required"))
}

fn build_prompt(work_item: &ImageGenerationWorkItem) -> Result<String> {
    let brief = work_item.approved_brief_text.trim();
    if brief.is_empty() || brief.len() > 12_000 || brief.contains('\0') {
        bail!("approved poster brief is not safe for image generation");
    }
    Ok(format!(
        "Create a factual community activity poster image at {IMAGE_SIZE}. Use only the approved brief below. Do not invent event facts, people, brands, or contact details.\n\nApproved brief:\n{brief}"
    ))
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter"
))]
fn parse_provider_response(response: &HttpResponse) -> Result<String> {
    ensure_success(response, "image provider")?;
    let payload: ProviderResponse =
        serde_json::from_slice(&response.body).context("parse image provider response")?;
    payload
        .data
        .into_iter()
        .next()
        .and_then(|image| image.b64_json)
        .filter(|value| !value.trim().is_empty())
        .ok_or_else(|| anyhow!("image provider response did not contain b64_json"))
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter"
))]
fn parse_media_upload_response(response: &HttpResponse) -> Result<MediaUploadResponse> {
    ensure_success(response, "media upload")?;
    serde_json::from_slice(&response.body).context("parse media upload response")
}

fn validate_media_response(
    config: &HttpMediaConfig,
    media: &MediaUploadResponse,
    content_hash: &str,
    metadata: &ImageMetadata,
    byte_size: usize,
    allow_insecure_http: bool,
) -> Result<Url> {
    let uri = media_response_url(&media.uri, allow_insecure_http)?;
    if uri.query().is_some() {
        bail!("media response URI must not contain a query");
    }
    let path = uri.path().to_ascii_lowercase();
    if !path.ends_with(".jpg") && !path.ends_with(".jpeg") {
        bail!("media response URI must reference a JPEG object");
    }
    let host = normalized_url_host(&uri)?;
    if !config.media_allowed_hosts.contains(&host)
        || !same_public_base(&config.media_public_base_url, &uri)
    {
        bail!("media response URI is outside the configured media boundary");
    }
    if media.content_hash != content_hash
        || media.mime_type != FINAL_IMAGE_MIME_TYPE
        || media.byte_size != byte_size
        || media.width != metadata.width
        || media.height != metadata.height
    {
        bail!("media upload metadata did not match generated image");
    }
    Ok(uri)
}

fn media_response_url(value: &str, allow_insecure_http: bool) -> Result<Url> {
    url_policy::reject_path_separator_ambiguity(value, "media response URI")?;
    if allow_insecure_http {
        let url = Url::parse(value).context("parse test media response URI")?;
        if !matches!(url.scheme(), "http" | "https") || url.host_str().is_none() {
            bail!("test media response URI must be an HTTP URL with a host");
        }
        return Ok(url);
    }
    https_url(value, "media response URI")
}

fn same_public_base(base: &Url, candidate: &Url) -> bool {
    let base_path = base.path().trim_end_matches('/');
    let candidate_path = candidate.path();
    base.scheme() == candidate.scheme()
        && normalized_url_host(base).ok() == normalized_url_host(candidate).ok()
        && base.port_or_known_default() == candidate.port_or_known_default()
        && (base_path.is_empty()
            || candidate_path == base_path
            || candidate_path
                .strip_prefix(base_path)
                .is_some_and(|suffix| suffix.starts_with('/')))
}

#[derive(Debug)]
struct ImageMetadata {
    width: u32,
    height: u32,
}

fn decode_provider_png(bytes: &[u8], max_bytes: usize) -> Result<RgbaImage> {
    if bytes.is_empty() || bytes.len() > max_bytes {
        bail!("image bytes are outside the configured size limit");
    }
    let image = decode_image_with_limits(bytes, ImageFormat::Png, "decode generated PNG")?;
    let (width, height) = image.dimensions();
    if width != 1024 || height != 1024 {
        bail!("generated image dimensions must match the requested specification");
    }
    Ok(image.to_rgba8())
}

fn encode_final_jpeg(source: &RgbaImage, max_bytes: usize) -> Result<Vec<u8>> {
    let (width, height) = source.dimensions();
    if width != 1024 || height != 1024 {
        bail!("generated image dimensions must match the requested specification");
    }
    let rgb = composite_rgba_over_white(source);

    let mut bytes = Vec::new();
    JpegEncoder::new_with_quality(&mut bytes, JPEG_QUALITY)
        .encode(&rgb, width, height, ExtendedColorType::Rgb8)
        .context("encode generated JPEG")?;
    inspect_final_jpeg(&bytes, max_bytes)?;
    Ok(bytes)
}

fn composite_rgba_over_white(source: &RgbaImage) -> Vec<u8> {
    let mut rgb = Vec::with_capacity(source.width() as usize * source.height() as usize * 3);
    for pixel in source.pixels() {
        let alpha = u32::from(pixel[3]);
        for channel in &pixel.0[..3] {
            let channel = u32::from(*channel);
            let composited = (channel * alpha + 255 * (255 - alpha) + 127) / 255;
            rgb.push(composited as u8);
        }
    }
    rgb
}

fn inspect_final_jpeg(bytes: &[u8], max_bytes: usize) -> Result<ImageMetadata> {
    if bytes.is_empty() || bytes.len() > max_bytes {
        bail!("image bytes are outside the configured size limit");
    }
    let image = decode_image_with_limits(bytes, ImageFormat::Jpeg, "decode final JPEG")?;
    let (width, height) = image.dimensions();
    if width != 1024 || height != 1024 {
        bail!("generated image dimensions must match the requested specification");
    }
    Ok(ImageMetadata { width, height })
}

fn decode_image_with_limits(
    bytes: &[u8],
    format: ImageFormat,
    error_context: &'static str,
) -> Result<image::DynamicImage> {
    let mut limits = Limits::default();
    limits.max_image_width = Some(1024);
    limits.max_image_height = Some(1024);
    limits.max_alloc = Some(MAX_IMAGE_DECODER_ALLOC_BYTES);
    let mut reader = ImageReader::with_format(io::Cursor::new(bytes), format);
    reader.limits(limits);
    reader.decode().context(error_context)
}

impl GeneratedImage {
    fn preview(&self) -> GeneratedImagePreview {
        GeneratedImagePreview {
            artifact_type: "generated_image",
            review_status: "pending",
            content_hash: self.content_hash.clone(),
            mime_type: FINAL_IMAGE_MIME_TYPE,
            width: self.width,
            height: self.height,
            byte_size: self.bytes.len(),
            image_specification: SPECIFICATION.to_string(),
        }
    }
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter"
))]
fn ensure_success(response: &HttpResponse, service: &str) -> Result<()> {
    if !(200..300).contains(&response.status) {
        bail!("{service} returned HTTP {}", response.status);
    }
    Ok(())
}

async fn load_work_item(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
) -> Result<Option<ImageGenerationWorkItem>> {
    let row = sqlx::query(
        r#"
        SELECT
            request.id,
            NULL::text AS claim_token,
            request.attempts,
            artifact.id AS approved_brief_artifact_id,
            artifact.content_hash AS approved_brief_content_hash,
            COALESCE(artifact.content_text, artifact.summary) AS approved_brief_text,
            request.payload->>'image_specification' AS image_specification,
            request.payload->>'prompt_hash' AS prompt_hash
        FROM qintopia_agent_os.work_items request
        JOIN qintopia_agent_os.artifacts artifact
          ON artifact.id = (request.payload->>'approved_brief_artifact_id')::uuid
         AND artifact.work_item_id = request.parent_work_item_id
         AND artifact.artifact_type = 'poster_brief'
         AND artifact.review_status = 'approved'
         AND artifact.content_hash = request.payload->>'approved_brief_content_hash'
        WHERE request.capability_key = $1
          AND request.work_item_type = $2
          AND request.requester_agent = 'xiaoman'
          AND request.target_agent = 'huabaosi'
          AND request.payload->>'image_specification' = $3
          AND request.payload->>'prompt_hash' LIKE 'sha256:%'
          AND request.attempts < $4
          AND request.status = 'queued'
          AND request.available_at <= now()
          AND ($5::uuid IS NULL OR request.id = $5)
        ORDER BY request.priority DESC, request.available_at ASC, request.created_at ASC
        LIMIT 1
        "#,
    )
    .bind(CAPABILITY_KEY)
    .bind(WORK_ITEM_TYPE)
    .bind(SPECIFICATION)
    .bind(MAX_GENERATION_ATTEMPTS)
    .bind(work_item_id)
    .fetch_optional(pool)
    .await
    .context("load approved Huabaosi image generation request")?;
    row.map(work_item_from_row).transpose()
}

async fn claim_work_item(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
) -> Result<ImageGenerationClaimOutcome> {
    let claim_token = new_claim_token();
    let mut tx = pool
        .begin()
        .await
        .context("begin Huabaosi image generation claim transaction")?;

    // Lease loss cannot prove whether the provider or media service accepted a request.
    if let Some(stale_id) = terminalize_one_stale_processing_claim(&mut tx, work_item_id).await? {
        tx.commit()
            .await
            .context("commit stale Huabaosi image generation reconciliation")?;
        return Ok(ImageGenerationClaimOutcome::ReconciledAmbiguous(stale_id));
    }

    let row = sqlx::query(
        r#"
        WITH claimable AS (
            SELECT
                request.id,
                artifact.id AS approved_brief_artifact_id,
                artifact.content_hash AS approved_brief_content_hash,
                COALESCE(artifact.content_text, artifact.summary) AS approved_brief_text,
                request.payload->>'image_specification' AS image_specification,
                request.payload->>'prompt_hash' AS prompt_hash
            FROM qintopia_agent_os.work_items request
            JOIN qintopia_agent_os.artifacts artifact
              ON artifact.id = (request.payload->>'approved_brief_artifact_id')::uuid
             AND artifact.work_item_id = request.parent_work_item_id
             AND artifact.artifact_type = 'poster_brief'
             AND artifact.review_status = 'approved'
             AND artifact.content_hash = request.payload->>'approved_brief_content_hash'
            WHERE request.capability_key = $1
              AND request.work_item_type = $2
              AND request.requester_agent = 'xiaoman'
              AND request.target_agent = 'huabaosi'
              AND request.payload->>'image_specification' = $3
              AND request.payload->>'prompt_hash' LIKE 'sha256:%'
              AND request.attempts < $4
              AND request.status = 'queued'
              AND request.available_at <= now()
              AND ($5::uuid IS NULL OR request.id = $5)
            ORDER BY request.priority DESC, request.available_at ASC, request.created_at ASC
            LIMIT 1
            FOR UPDATE OF request SKIP LOCKED
        )
        UPDATE qintopia_agent_os.work_items request
        SET
            status = 'processing',
            claimed_by = $6,
            locked_at = now(),
            claim_expires_at = now() + interval '10 minutes',
            attempts = attempts + 1,
            updated_at = now()
        FROM claimable
        WHERE request.id = claimable.id
        RETURNING
            request.id,
            request.claimed_by AS claim_token,
            request.attempts,
            claimable.approved_brief_artifact_id,
            claimable.approved_brief_content_hash,
            claimable.approved_brief_text,
            claimable.image_specification,
            claimable.prompt_hash
        "#,
    )
    .bind(CAPABILITY_KEY)
    .bind(WORK_ITEM_TYPE)
    .bind(SPECIFICATION)
    .bind(MAX_GENERATION_ATTEMPTS)
    .bind(work_item_id)
    .bind(&claim_token)
    .fetch_optional(&mut *tx)
    .await
    .context("claim Huabaosi image generation request")?;
    let work_item = row.map(work_item_from_row).transpose()?;
    tx.commit()
        .await
        .context("commit Huabaosi image generation claim transaction")?;
    Ok(match work_item {
        Some(work_item) => ImageGenerationClaimOutcome::Claimed(work_item),
        None => ImageGenerationClaimOutcome::Empty,
    })
}

async fn terminalize_one_stale_processing_claim(
    tx: &mut Transaction<'_, Postgres>,
    work_item_id: Option<Uuid>,
) -> Result<Option<Uuid>> {
    let stale = sqlx::query(
        r#"
        WITH candidate AS (
            SELECT request.id, request.attempts
            FROM qintopia_agent_os.work_items request
            WHERE request.capability_key = $1
              AND request.work_item_type = $2
              AND request.requester_agent = 'xiaoman'
              AND request.target_agent = 'huabaosi'
              AND request.status = 'processing'
              AND (
                  request.claimed_by IS NULL
                  OR request.locked_at IS NULL
                  OR request.claim_expires_at IS NULL
                  OR request.claim_expires_at <= now()
              )
              AND ($3::uuid IS NULL OR request.id = $3)
            ORDER BY request.updated_at ASC, request.created_at ASC
            LIMIT 1
            FOR UPDATE OF request SKIP LOCKED
        )
        UPDATE qintopia_agent_os.work_items request
        SET
            status = 'failed',
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = 'image generation external outcome ambiguous after claim loss; automatic retry disabled',
            updated_at = now()
        FROM candidate
        WHERE request.id = candidate.id
        RETURNING request.id, candidate.attempts
        "#,
    )
    .bind(CAPABILITY_KEY)
    .bind(WORK_ITEM_TYPE)
    .bind(work_item_id)
    .fetch_optional(&mut **tx)
    .await
    .context("terminalize stale Huabaosi image generation claim")?;

    let Some(row) = stale else {
        return Ok(None);
    };
    let stale_id: Uuid = row.try_get("id")?;
    let attempts: i32 = row.try_get("attempts")?;
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, event_type, actor_type, actor_id, message, data)
        VALUES
            ($1, 'image_generation_outcome_ambiguous', 'worker', $2,
             'lost image generation claim has an unknown external outcome', $3)
        "#,
    )
    .bind(stale_id)
    .bind(WORKER_ID)
    .bind(json!({
        "attempt_number": attempts,
        "failure_class": "ambiguous",
        "failure_stage": "claim_lost",
        "automatic_retry_allowed": false,
        "external_generation_executed": null,
        "external_media_upload_executed": null,
        "external_publish_executed": false,
        "sensitive_fields_redacted": true,
    }))
    .execute(&mut **tx)
    .await
    .context("append stale image generation claim event")?;
    Ok(Some(stale_id))
}

fn work_item_from_row(row: sqlx::postgres::PgRow) -> Result<ImageGenerationWorkItem> {
    Ok(ImageGenerationWorkItem {
        id: row.try_get("id")?,
        claim_token: row.try_get("claim_token")?,
        attempts: row.try_get("attempts")?,
        approved_brief_artifact_id: row.try_get("approved_brief_artifact_id")?,
        approved_brief_content_hash: row.try_get("approved_brief_content_hash")?,
        approved_brief_text: row.try_get("approved_brief_text")?,
        image_specification: row.try_get("image_specification")?,
        prompt_hash: row.try_get("prompt_hash")?,
    })
}

fn new_claim_token() -> String {
    format!("{WORKER_ID}:{}", Uuid::new_v4())
}

fn generated_image_artifact_id(work_item_id: Uuid, content_hash: &str) -> Uuid {
    let mut hasher = Sha256::new();
    hasher.update(b"qintopia:huabaosi:generated-image:v1\0");
    hasher.update(work_item_id.as_bytes());
    hasher.update(content_hash.as_bytes());
    let digest = hasher.finalize();
    let mut bytes = [0_u8; 16];
    bytes.copy_from_slice(&digest[..16]);
    bytes[6] = (bytes[6] & 0x0f) | 0x80;
    bytes[8] = (bytes[8] & 0x3f) | 0x80;
    Uuid::from_bytes(bytes)
}

async fn persist_generated_image(
    pool: &PgPool,
    work_item: &ImageGenerationWorkItem,
    generated: &GeneratedImage,
) -> Result<Uuid> {
    let mut tx = pool
        .begin()
        .await
        .context("begin generated image transaction")?;
    let claim_token = work_item
        .claim_token
        .as_deref()
        .ok_or_else(|| anyhow!("image generation apply requires a claim token"))?;
    let claim_is_current: Option<Uuid> = sqlx::query_scalar(
        r#"
        SELECT id
        FROM qintopia_agent_os.work_items
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
          AND claim_expires_at > now()
        FOR UPDATE
        "#,
    )
    .bind(work_item.id)
    .bind(claim_token)
    .fetch_optional(&mut *tx)
    .await
    .context("lock current image generation claim")?;
    if claim_is_current.is_none() {
        bail!("image generation claim is no longer current");
    }
    let still_approved: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1
            FROM qintopia_agent_os.artifacts
            WHERE id = $1
              AND work_item_id = (
                  SELECT parent_work_item_id
                  FROM qintopia_agent_os.work_items
                  WHERE id = $2
              )
              AND artifact_type = 'poster_brief'
              AND review_status = 'approved'
              AND content_hash = $3
        )
        "#,
    )
    .bind(work_item.approved_brief_artifact_id)
    .bind(work_item.id)
    .bind(&work_item.approved_brief_content_hash)
    .fetch_one(&mut *tx)
    .await
    .context("recheck approved poster brief")?;
    if !still_approved {
        bail!("approved poster brief changed before generated image could be recorded");
    }

    let inserted = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.artifacts
            (
                id,
                work_item_id,
                artifact_type,
                review_status,
                created_by_agent,
                title,
                summary,
                artifact_uri,
                content_hash,
                source_ids,
                risk_labels,
                information_class,
                metadata,
                review_requested_at
            )
        VALUES
            (
                $1,
                $2,
                'generated_image',
                'pending',
                'huabaosi',
                '活动海报图片（待审核）',
                '由已审核 poster_brief 生成，等待人工审核后才能被下游引用。',
                $3,
                $4,
                $5,
                ARRAY['external_use_review_required','generated_media']::text[],
                'internal_ops',
                $6,
                now()
            )
        ON CONFLICT (work_item_id, content_hash) WHERE content_hash IS NOT NULL AND content_hash <> ''
        DO NOTHING
        RETURNING id
        "#,
    )
    .bind(generated.artifact_id)
    .bind(work_item.id)
    .bind(&generated.artifact_uri)
    .bind(&generated.content_hash)
    .bind(json!([{
        "approved_brief_artifact_id": work_item.approved_brief_artifact_id,
        "approved_brief_content_hash": work_item.approved_brief_content_hash,
    }]))
    .bind(generated_image_metadata(work_item, generated))
    .fetch_optional(&mut *tx)
    .await
    .context("insert generated image artifact")?;
    let (artifact_id, artifact_created) = match inserted {
        Some(row) => (row.get("id"), true),
        None => {
            let row = sqlx::query(
                r#"
                SELECT id, review_status, artifact_uri, source_ids, metadata
                FROM qintopia_agent_os.artifacts
                WHERE work_item_id = $1
                  AND content_hash = $2
                  AND artifact_type = 'generated_image'
                "#,
            )
            .bind(work_item.id)
            .bind(&generated.content_hash)
            .fetch_optional(&mut *tx)
            .await
            .context("load existing generated image artifact")?
            .ok_or_else(|| {
                anyhow!("generated image content hash conflicts with another artifact")
            })?;
            let review_status: String = row.get("review_status");
            let existing_artifact_id: Uuid = row.get("id");
            let artifact_uri: Option<String> = row.try_get("artifact_uri")?;
            let source_ids: Value = row.try_get("source_ids")?;
            let metadata: Value = row.try_get("metadata")?;
            if existing_artifact_id != generated.artifact_id
                || !existing_generated_image_matches(
                    &review_status,
                    artifact_uri.as_deref(),
                    &source_ids,
                    &metadata,
                    work_item,
                    generated,
                )
            {
                bail!(
                    "refusing to reuse a reviewed, stale, or mismatched generated image artifact"
                );
            }
            (row.get("id"), false)
        }
    };
    let workbench_ref_id = match generated.feishu_record_id.as_deref() {
        Some(record_id) => Some(
            record_primary_storage_workbench_ref(
                &mut tx,
                work_item.id,
                artifact_id,
                &generated.content_hash,
                record_id,
                generated.workflow_root_id,
            )
            .await
            .context("record Huabaosi Feishu primary storage reference")?,
        ),
        None => None,
    };

    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'awaiting_review',
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = NULL,
            updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
          AND claim_expires_at > now()
        "#,
    )
    .bind(work_item.id)
    .bind(claim_token)
    .execute(&mut *tx)
    .await
    .context("mark image generation request awaiting review")?;
    if updated.rows_affected() != 1 {
        bail!("image generation claim changed before pending artifact could be recorded");
    }
    if artifact_created {
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_item_events
                (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
            VALUES ($1, $2, 'generated_image_created', 'worker', $3, 'generated image stored pending human review', $4)
            "#,
        )
        .bind(work_item.id)
        .bind(artifact_id)
        .bind(WORKER_ID)
        .bind(generated_image_creation_event_data(generated))
        .execute(&mut *tx)
        .await
        .context("append generated image event")?;
        if let Some(ref_id) = workbench_ref_id {
            sqlx::query(
                r#"
                INSERT INTO qintopia_agent_os.work_item_events
                    (work_item_id, artifact_id, event_type, actor_type, actor_id, message, data)
                VALUES
                    ($1, $2, 'generated_image_feishu_stored', 'worker', $3,
                     'generated image stored in the Feishu artifact workbench', $4)
                "#,
            )
            .bind(work_item.id)
            .bind(artifact_id)
            .bind(WORKER_ID)
            .bind(json!({
                "workbench_ref_id": ref_id,
                "schema_version": "huabaosi-generated-image-v1",
                "content_hash": generated.content_hash,
                "review_status": "pending",
                "external_write_executed": true,
                "external_publish_executed": false,
                "external_send_executed": false,
                "sensitive_fields_redacted": true,
            }))
            .execute(&mut *tx)
            .await
            .context("append Huabaosi Feishu primary storage event")?;
        }
    }
    tx.commit()
        .await
        .context("commit generated image transaction")?;
    Ok(artifact_id)
}

fn can_reuse_existing_generated_image(review_status: &str) -> bool {
    review_status == "pending"
}

fn generated_image_metadata(
    work_item: &ImageGenerationWorkItem,
    generated: &GeneratedImage,
) -> Value {
    json!({
        "generated_by": WORKER_ID,
        "provider": "openai-compatible",
        "model": "gpt-image-2",
        "mime_type": FINAL_IMAGE_MIME_TYPE,
        "file_md5": generated.file_md5,
        "provider_source_mime_type": PROVIDER_SOURCE_MIME_TYPE,
        "provider_source_content_hash": generated.provider_source_content_hash,
        "media_transform": MEDIA_TRANSFORM,
        "jpeg_quality": JPEG_QUALITY,
        "alpha_background": ALPHA_BACKGROUND,
        "width": generated.width,
        "height": generated.height,
        "byte_size": generated.bytes.len(),
        "approved_brief_artifact_id": work_item.approved_brief_artifact_id,
        "approved_brief_content_hash": work_item.approved_brief_content_hash,
        "prompt_hash": work_item.prompt_hash,
        "storage_provider": generated.storage_provider,
    })
}

fn generated_image_creation_event_data(generated: &GeneratedImage) -> Value {
    json!({
        "content_hash": generated.content_hash,
        "mime_type": FINAL_IMAGE_MIME_TYPE,
        "file_md5": generated.file_md5,
        "provider_source_mime_type": PROVIDER_SOURCE_MIME_TYPE,
        "provider_source_content_hash": generated.provider_source_content_hash,
        "media_transform": MEDIA_TRANSFORM,
        "jpeg_quality": JPEG_QUALITY,
        "alpha_background": ALPHA_BACKGROUND,
        "width": generated.width,
        "height": generated.height,
        "byte_size": generated.bytes.len(),
        "external_publish_executed": false,
        "storage_provider": generated.storage_provider,
    })
}

fn existing_generated_image_matches(
    review_status: &str,
    artifact_uri: Option<&str>,
    source_ids: &Value,
    metadata: &Value,
    work_item: &ImageGenerationWorkItem,
    generated: &GeneratedImage,
) -> bool {
    can_reuse_existing_generated_image(review_status)
        && artifact_uri == Some(generated.artifact_uri.as_str())
        && source_ids
            == &json!([{
                "approved_brief_artifact_id": work_item.approved_brief_artifact_id,
                "approved_brief_content_hash": work_item.approved_brief_content_hash,
            }])
        && metadata == &generated_image_metadata(work_item, generated)
}

async fn record_generation_failure(
    pool: &PgPool,
    work_item: &ImageGenerationWorkItem,
    failure: GenerationFailure,
) -> Result<FailureRecordOutcome> {
    let mut tx = pool
        .begin()
        .await
        .context("begin image generation failure transaction")?;
    let claim_token = work_item
        .claim_token
        .as_deref()
        .ok_or_else(|| anyhow!("image generation failure handling requires a claim token"))?;
    let retry_scheduled = should_retry_generation(failure.class, work_item.attempts);
    let retry_exhausted =
        failure.class == GenerationFailureClass::RetryableProvider && !retry_scheduled;
    let ambiguous = failure.class == GenerationFailureClass::AmbiguousProvider;
    let external_generation_executed = failure_external_generation_executed(failure);
    let external_media_write_executed = failure_external_media_write_executed(failure);
    let status = if retry_scheduled { "queued" } else { "failed" };
    let retry_delay_seconds = if retry_scheduled {
        generation_retry_delay_seconds(work_item.attempts)
    } else {
        0
    };
    let last_error = if retry_scheduled {
        "retryable image provider failure; retry scheduled"
    } else if retry_exhausted {
        "retryable image provider failure; retry attempts exhausted"
    } else if ambiguous {
        "image provider outcome ambiguous; automatic retry disabled"
    } else {
        "image generation failed; inspect sanitized worker metrics"
    };
    let updated = sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = $3,
            available_at = now() + ($4::text || ' seconds')::interval,
            claimed_by = NULL,
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = $5,
            updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
          AND claim_expires_at > now()
        "#,
    )
    .bind(work_item.id)
    .bind(claim_token)
    .bind(status)
    .bind(retry_delay_seconds)
    .bind(last_error)
    .execute(&mut *tx)
    .await
    .context("record image generation request failure")?;
    if updated.rows_affected() != 1 {
        tx.commit()
            .await
            .context("commit stale image generation claim")?;
        return Ok(FailureRecordOutcome::StaleClaim);
    }
    let event_type = if retry_scheduled {
        "image_generation_retry_scheduled"
    } else if ambiguous {
        "image_generation_outcome_ambiguous"
    } else {
        "failed"
    };
    let message = if retry_scheduled {
        "retryable image provider failure scheduled for another attempt"
    } else if retry_exhausted {
        "image generation retry attempts exhausted before pending artifact creation"
    } else if ambiguous {
        "image provider request outcome is unknown and automatic retry is disabled"
    } else {
        "image generation failed before pending artifact creation"
    };
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, event_type, actor_type, actor_id, message, data)
        VALUES ($1, $2, 'worker', $3, $4, $5)
        "#,
    )
    .bind(work_item.id)
    .bind(event_type)
    .bind(WORKER_ID)
    .bind(message)
    .bind(json!({
        "attempt_number": work_item.attempts,
        "max_attempts": MAX_GENERATION_ATTEMPTS,
        "failure_class": failure.class.as_str(),
        "failure_stage": failure.stage,
        "retry_delay_seconds": retry_delay_seconds,
        "retry_scheduled": retry_scheduled,
        "retry_exhausted": retry_exhausted,
        "automatic_retry_allowed": retry_scheduled,
        "external_generation_executed": external_generation_executed,
        "external_media_write_executed": external_media_write_executed,
        "external_publish_executed": false,
        "sensitive_fields_redacted": true,
    }))
    .execute(&mut *tx)
    .await
    .context("append image generation failure event")?;
    tx.commit()
        .await
        .context("commit image generation failure transaction")?;
    Ok(if retry_scheduled {
        FailureRecordOutcome::RetryScheduled
    } else if retry_exhausted {
        FailureRecordOutcome::RetryExhausted
    } else if ambiguous {
        FailureRecordOutcome::Ambiguous
    } else {
        FailureRecordOutcome::Failed
    })
}

fn failure_external_generation_executed(failure: GenerationFailure) -> Option<bool> {
    if failure.class == GenerationFailureClass::AmbiguousProvider {
        return None;
    }
    match failure.stage {
        "workflow_root_resolution"
        | "prompt_validation"
        | "provider_request"
        | "provider_transport" => Some(false),
        "provider_response" | "provider_payload" | "media_transform" | "media_upload"
        | "media_readback" | "feishu_storage" | "persistence" => Some(true),
        _ => None,
    }
}

fn failure_external_media_write_executed(failure: GenerationFailure) -> Option<bool> {
    match failure.stage {
        "workflow_root_resolution"
        | "prompt_validation"
        | "provider_request"
        | "provider_transport"
        | "provider_response"
        | "provider_payload"
        | "media_transform" => Some(false),
        "media_upload" | "feishu_storage" => None,
        "media_readback" | "persistence" => Some(true),
        _ => None,
    }
}

fn generation_retry_delay_seconds(attempts: i32) -> i64 {
    let exponent = attempts.saturating_sub(1).clamp(0, 6) as u32;
    BASE_RETRY_DELAY_SECONDS.saturating_mul(2_i64.saturating_pow(exponent))
}

fn should_retry_generation(class: GenerationFailureClass, attempts: i32) -> bool {
    class == GenerationFailureClass::RetryableProvider && attempts < MAX_GENERATION_ATTEMPTS
}

fn image_preview(work_item: &ImageGenerationWorkItem) -> GeneratedImagePreview {
    let content_hash = content_hash_text(&format!(
        "{}|{}|{}|{}|{}",
        work_item.approved_brief_artifact_id,
        work_item.approved_brief_content_hash,
        work_item.image_specification,
        work_item.prompt_hash,
        MEDIA_TRANSFORM
    ));
    GeneratedImagePreview {
        artifact_type: "generated_image",
        review_status: "pending",
        content_hash,
        mime_type: FINAL_IMAGE_MIME_TYPE,
        width: 1024,
        height: 1024,
        byte_size: 0,
        image_specification: work_item.image_specification.clone(),
    }
}

fn image_generation_enabled() -> bool {
    std::env::var("QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED")
        .map(|value| image_generation_enabled_value(&value))
        .unwrap_or(false)
}

fn image_generation_enabled_value(value: &str) -> bool {
    value.trim() == "1"
}

const fn huabaosi_adapter_mode() -> ImageAdapterMode {
    match (
        cfg!(feature = "huabaosi-staging-adapter"),
        cfg!(feature = "huabaosi-production-adapter"),
    ) {
        (false, false) => ImageAdapterMode::Disabled,
        (true, false) => ImageAdapterMode::Staging,
        (false, true) => ImageAdapterMode::Production,
        (true, true) => ImageAdapterMode::Invalid,
    }
}

fn report(
    success: bool,
    apply_requested: bool,
    fixture_mode: bool,
    action_status: &str,
    work_item_id: Option<Uuid>,
    artifact_ids: Vec<Uuid>,
    artifact_preview: Option<GeneratedImagePreview>,
) -> ImageGenerationWorkerReport {
    ImageGenerationWorkerReport {
        success,
        dry_run: !apply_requested,
        apply_requested,
        fixture_mode,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        work_item_id,
        artifact_ids,
        artifact_preview,
        safe_for_chat: false,
        limitations: vec![
            "generation requires an explicit enable flag plus reviewed provider and media configuration".to_string(),
            "generated images remain pending human review and are never published by this worker".to_string(),
            "only recoverable provider failures are retried, with at most three total attempts".to_string(),
            "provider responses, prompts, credentials, Feishu identifiers, and QiWe payloads are not emitted".to_string(),
        ],
        guardrails: vec![
            "only approved poster_brief artifacts are eligible".to_string(),
            "production endpoints must use HTTPS and media hosts must be allowlisted".to_string(),
            "retry events contain only sanitized failure class, stage, attempt, and delay".to_string(),
            "generated image bytes are uploaded and read back before the pending artifact is recorded".to_string(),
        ],
    }
}

fn content_hash_text(value: &str) -> String {
    content_hash_bytes(value.as_bytes())
}

fn content_hash_bytes(value: &[u8]) -> String {
    format!("sha256:{:x}", Sha256::digest(value))
}

fn md5_hex_bytes(value: &[u8]) -> String {
    format!("{:x}", Md5::digest(value))
}

fn media_upload_idempotency_key(prompt_hash: &str, final_content_hash: &str) -> String {
    content_hash_text(&format!(
        "{prompt_hash}|{MEDIA_TRANSFORM}|{final_content_hash}"
    ))
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter"
))]
fn generate_and_store_with_client(
    config: &AdapterConfig,
    work_item: &ImageGenerationWorkItem,
    workflow_root_id: Uuid,
    client: HttpClient,
) -> GenerationAttemptResult<GeneratedImage> {
    generate_and_store_with(config, work_item, workflow_root_id, &client)
}

#[cfg(any(
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter"
))]
fn generate_and_store(
    config: &AdapterConfig,
    work_item: &ImageGenerationWorkItem,
    workflow_root_id: Uuid,
) -> GenerationAttemptResult<GeneratedImage> {
    generate_and_store_with_client(
        config,
        work_item,
        workflow_root_id,
        HttpClient::production_with_timeout(config.http_timeout),
    )
}

#[cfg(any(
    test,
    feature = "huabaosi-production-adapter",
    feature = "huabaosi-staging-adapter"
))]
fn generate_and_store_with(
    config: &AdapterConfig,
    work_item: &ImageGenerationWorkItem,
    workflow_root_id: Uuid,
    client: &HttpClient,
) -> GenerationAttemptResult<GeneratedImage> {
    let prompt = build_prompt(work_item)
        .map_err(|source| GenerationAttemptError::terminal("prompt_validation", source))?;
    let provider_body = serde_json::to_vec(&json!({
        "model": config.model,
        "prompt": prompt,
        "size": IMAGE_SIZE,
        "response_format": "b64_json",
    }))
    .context("serialize image provider request")
    .map_err(|source| GenerationAttemptError::terminal("provider_request", source))?;
    let provider_response = client
        .request(
            "POST",
            &config.provider_endpoint,
            &[
                ("Authorization", format!("Bearer {}", config.api_key)),
                ("Content-Type", "application/json".to_string()),
                ("Accept", "application/json".to_string()),
            ],
            &provider_body,
            provider_response_limit(config.max_media_bytes),
        )
        .map_err(|error| {
            let request_may_have_been_sent = error.request_may_have_been_sent();
            let retryable = error.transport && !request_may_have_been_sent;
            let source = error.into_source();
            if retryable {
                GenerationAttemptError::retryable_provider("provider_transport", source)
            } else if request_may_have_been_sent {
                GenerationAttemptError::ambiguous_provider("provider_transport", source)
            } else {
                GenerationAttemptError::terminal("provider_request", source)
            }
        })?;
    let provider_class = classify_provider_response(provider_response.status);
    let provider_response =
        parse_provider_response(&provider_response).map_err(|source| match provider_class {
            GenerationFailureClass::RetryableProvider => {
                GenerationAttemptError::retryable_provider("provider_response", source)
            }
            GenerationFailureClass::Terminal | GenerationFailureClass::AmbiguousProvider => {
                GenerationAttemptError::terminal("provider_response", source)
            }
        })?;
    let provider_bytes = Base64::decode_vec(&provider_response)
        .map_err(|_| anyhow!("decode image provider b64_json response"))
        .map_err(|source| GenerationAttemptError::terminal("provider_payload", source))?;
    let source_image = decode_provider_png(&provider_bytes, config.max_media_bytes)
        .map_err(|source| GenerationAttemptError::terminal("provider_payload", source))?;
    let provider_source_content_hash = content_hash_bytes(&provider_bytes);
    let bytes = encode_final_jpeg(&source_image, config.max_media_bytes)
        .map_err(|source| GenerationAttemptError::terminal("media_transform", source))?;
    let metadata = inspect_final_jpeg(&bytes, config.max_media_bytes)
        .map_err(|source| GenerationAttemptError::terminal("media_transform", source))?;
    let content_hash = content_hash_bytes(&bytes);
    let file_md5 = md5_hex_bytes(&bytes);
    let artifact_id = generated_image_artifact_id(work_item.id, &content_hash);
    let (artifact_uri, storage_provider, feishu_record_id) = match &config.storage {
        StorageConfig::Http(storage) => {
            let upload_idempotency_key =
                media_upload_idempotency_key(&work_item.prompt_hash, &content_hash);
            let upload_response = client
                .request(
                    "POST",
                    &storage.media_upload_endpoint,
                    &[
                        ("Content-Type", FINAL_IMAGE_MIME_TYPE.to_string()),
                        ("Accept", "application/json".to_string()),
                        ("X-Qintopia-Content-Hash", content_hash.clone()),
                        ("X-Qintopia-Byte-Size", bytes.len().to_string()),
                        ("X-Qintopia-Width", metadata.width.to_string()),
                        ("X-Qintopia-Height", metadata.height.to_string()),
                        ("X-Qintopia-Work-Item-Id", work_item.id.to_string()),
                        ("X-Qintopia-Idempotency-Key", upload_idempotency_key),
                    ],
                    &bytes,
                    MAX_MEDIA_UPLOAD_RESPONSE_BYTES,
                )
                .map_err(|error| {
                    GenerationAttemptError::terminal("media_upload", error.into_source())
                })?;
            let media = parse_media_upload_response(&upload_response)
                .map_err(|source| GenerationAttemptError::terminal("media_upload", source))?;
            let media_url = validate_media_response(
                storage,
                &media,
                &content_hash,
                &metadata,
                bytes.len(),
                client.allows_insecure_http(),
            )
            .map_err(|source| GenerationAttemptError::terminal("media_upload", source))?;
            let readback = client
                .request("GET", &media_url, &[], &[], config.max_media_bytes)
                .map_err(|error| {
                    GenerationAttemptError::terminal("media_readback", error.into_source())
                })?;
            ensure_success(&readback, "media readback")
                .map_err(|source| GenerationAttemptError::terminal("media_readback", source))?;
            let content_type = readback
                .headers
                .get("content-type")
                .map(|value| value.split(';').next().unwrap_or_default().trim())
                .unwrap_or_default();
            if content_type != FINAL_IMAGE_MIME_TYPE {
                return Err(GenerationAttemptError::terminal(
                    "media_readback",
                    anyhow!("media readback returned an unexpected MIME type"),
                ));
            }
            let readback_metadata = inspect_final_jpeg(&readback.body, config.max_media_bytes)
                .map_err(|source| GenerationAttemptError::terminal("media_readback", source))?;
            if readback.body != bytes
                || content_hash_bytes(&readback.body) != content_hash
                || readback_metadata.width != metadata.width
                || readback_metadata.height != metadata.height
            {
                return Err(GenerationAttemptError::terminal(
                    "media_readback",
                    anyhow!("media readback did not match uploaded image"),
                ));
            }
            (media.uri, HTTP_STORAGE_BACKEND, None)
        }
        StorageConfig::Feishu(storage) => {
            let result = store_primary_generated_image(
                storage,
                &FeishuPrimaryStorageImage {
                    artifact_id,
                    workflow_root_id,
                    work_item_id: work_item.id,
                    content_hash: &content_hash,
                    file_md5: &file_md5,
                    source_content_hash: &provider_source_content_hash,
                    bytes: &bytes,
                    width: metadata.width,
                    height: metadata.height,
                },
            )
            .map_err(|source| GenerationAttemptError::terminal("feishu_storage", source))?;
            (
                result.artifact_uri,
                FEISHU_STORAGE_BACKEND,
                Some(result.record_id),
            )
        }
    };
    Ok(GeneratedImage {
        artifact_id,
        workflow_root_id,
        bytes,
        content_hash,
        file_md5,
        provider_source_content_hash,
        width: metadata.width,
        height: metadata.height,
        artifact_uri,
        storage_provider,
        feishu_record_id,
    })
}

fn classify_provider_response(status: u16) -> GenerationFailureClass {
    if status == 408 || status == 429 || (500..600).contains(&status) {
        GenerationFailureClass::RetryableProvider
    } else {
        GenerationFailureClass::Terminal
    }
}

fn provider_response_limit(max_media_bytes: usize) -> usize {
    max_media_bytes
        .checked_mul(4)
        .and_then(|value| value.checked_div(3))
        .and_then(|value| value.checked_add(PROVIDER_RESPONSE_OVERHEAD_BYTES))
        .expect("configured media size limit keeps provider response limit representable")
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Cursor, Read, Write},
        net::TcpListener,
        thread,
    };

    #[cfg(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    ))]
    use std::fs;

    use super::*;
    #[cfg(feature = "postgres-integration-tests")]
    use crate::db;
    #[cfg(not(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter"
    )))]
    use clap::Parser;
    use image::ImageEncoder;
    #[cfg(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    ))]
    use tempfile::tempdir;

    #[cfg(feature = "postgres-integration-tests")]
    fn postgres_integration_database_url() -> String {
        assert_eq!(
            std::env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE").as_deref(),
            Ok("1"),
            "PostgreSQL integration test requires the explicit apply-smoke guard"
        );
        let database_url = std::env::var("QINTOPIA_SIDECAR_DATABASE_URL")
            .expect("PostgreSQL integration test requires QINTOPIA_SIDECAR_DATABASE_URL");
        validate_postgres_integration_database_url(&database_url)
            .expect("integration database URL must use the guarded test boundary");
        database_url
    }

    #[cfg(feature = "postgres-integration-tests")]
    fn validate_postgres_integration_database_url(database_url: &str) -> Result<()> {
        let parsed = Url::parse(database_url).context("integration database URL must parse")?;
        if !matches!(parsed.host_str(), Some("127.0.0.1" | "[::1]")) {
            bail!("integration database host must be a literal loopback address");
        }
        if parsed.path().trim_start_matches('/') != "qintopia_test" {
            bail!("integration database must be named qintopia_test");
        }
        Ok(())
    }

    #[cfg(feature = "postgres-integration-tests")]
    async fn insert_processing_claim_fixture(pool: &PgPool) -> (Uuid, Uuid, String) {
        let parent_id = Uuid::new_v4();
        let request_id = Uuid::new_v4();
        let brief_id = Uuid::new_v4();
        let suffix = Uuid::new_v4();
        let claim_token = format!("{WORKER_ID}:integration-sensitive-{suffix}");
        let brief_hash = "sha256:aaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaaa";
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (id, work_item_type, status, requester_agent, target_agent,
                 capability_key, brief_summary, source_type, dedupe_key,
                 idempotency_key, payload, review_policy)
            VALUES
                ($1, 'visual_asset_request', 'completed', 'xiaoman', 'huabaosi',
                 'huabaosi.create_visual_asset', 'stale claim integration parent',
                 'integration_test', $2, $2, '{}'::jsonb, 'before_external_use')
            "#,
        )
        .bind(parent_id)
        .bind(format!("huabaosi-stale-parent:{suffix}"))
        .execute(pool)
        .await
        .expect("insert stale claim parent");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.artifacts
                (id, work_item_id, artifact_type, review_status, created_by_agent,
                 title, summary, content_text, content_hash)
            VALUES
                ($1, $2, 'poster_brief', 'approved', 'huabaosi',
                 'stale claim integration brief', 'sanitized fixture',
                 'Sanitized approved poster brief.', $3)
            "#,
        )
        .bind(brief_id)
        .bind(parent_id)
        .bind(brief_hash)
        .execute(pool)
        .await
        .expect("insert approved poster brief");
        sqlx::query(
            r#"
            INSERT INTO qintopia_agent_os.work_items
                (id, parent_work_item_id, work_item_type, status, requester_agent,
                 target_agent, capability_key, brief_summary, source_type, dedupe_key,
                 idempotency_key, payload, review_policy, attempts, claimed_by,
                 locked_at, claim_expires_at)
            VALUES
                ($1, $2, 'image_generation_request', 'processing', 'xiaoman',
                 'huabaosi', 'huabaosi.generate_image_asset',
                 'stale image generation integration request', 'integration_test',
                 $3, $3, $4, 'before_external_use', 1, $5,
                 now() - interval '11 minutes', now() - interval '1 minute')
            "#,
        )
        .bind(request_id)
        .bind(parent_id)
        .bind(format!("huabaosi-stale-request:{suffix}"))
        .bind(json!({
            "approved_brief_artifact_id": brief_id,
            "approved_brief_content_hash": brief_hash,
            "image_specification": SPECIFICATION,
            "prompt_hash": "sha256:bbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbbb"
        }))
        .bind(&claim_token)
        .execute(pool)
        .await
        .expect("insert stale image generation request");
        (parent_id, request_id, claim_token)
    }

    #[cfg(feature = "postgres-integration-tests")]
    async fn delete_processing_claim_fixture(pool: &PgPool, parent_id: Uuid, request_id: Uuid) {
        sqlx::query("DELETE FROM qintopia_agent_os.work_items WHERE id = $1")
            .bind(request_id)
            .execute(pool)
            .await
            .expect("delete stale image generation request");
        sqlx::query("DELETE FROM qintopia_agent_os.work_items WHERE id = $1")
            .bind(parent_id)
            .execute(pool)
            .await
            .expect("delete stale claim parent");
    }

    #[test]
    fn fixture_preview_is_pending_and_safe_for_chat_is_false() {
        let report = fixture_report();
        let raw = serde_json::to_string(&report).expect("report serializes");

        assert_eq!(report.action_status, "fixture_image_generation_preview");
        assert!(report.dry_run);
        assert!(!report.safe_for_chat);
        assert!(report.artifact_ids.is_empty());
        assert_eq!(
            report.artifact_preview.as_ref().unwrap().artifact_type,
            "generated_image"
        );
        assert_eq!(
            report.artifact_preview.as_ref().unwrap().review_status,
            "pending"
        );
        assert!(!raw.contains("api_key"));
        assert!(!raw.contains("table_id"));
        assert!(!raw.contains("message_id"));
    }

    #[test]
    fn image_preview_is_stable_for_the_same_approved_brief_and_prompt_hash() {
        let work_item = fixture_work_item();
        assert_eq!(
            image_preview(&work_item).content_hash,
            image_preview(&work_item).content_hash
        );
    }

    #[test]
    fn default_image_generation_flag_is_disabled() {
        assert!(!image_generation_enabled_value(""));
        assert!(!image_generation_enabled_value("0"));
        assert!(image_generation_enabled_value("1"));
    }

    #[test]
    fn preflight_reports_only_sanitized_configuration_state() {
        let report = preflight_report(true, false, ImageAdapterMode::Disabled, 2, Vec::new());
        let raw = serde_json::to_string(&report).expect("report serializes");

        assert!(report.success);
        assert_eq!(report.worker, WORKER_ID);
        assert_eq!(report.action_status, "adapter_config_ready");
        assert!(!report.generation_enabled);
        assert!(!report.adapter_compiled);
        assert_eq!(report.adapter_mode, "disabled");
        assert!(report.config_valid);
        assert_eq!(report.media_allowed_host_count, 2);
        assert!(report.missing_configuration.is_empty());
        assert!(!report.safe_for_chat);
        assert!(ensure_preflight_success(&report).is_ok());
        assert!(!raw.contains("api_key"));
        assert!(!raw.contains("endpoint"));
        assert!(!raw.contains("table_id"));
        assert!(!raw.contains("message_id"));
    }

    #[test]
    fn preflight_reports_missing_configuration_without_enabling_generation() {
        let report = preflight_report(
            false,
            false,
            ImageAdapterMode::Disabled,
            0,
            vec!["QINTOPIA_HUABAOSI_IMAGE_API_KEY"],
        );

        assert!(!report.success);
        assert_eq!(report.action_status, "adapter_not_configured");
        assert!(!report.config_valid);
        assert!(!report.generation_enabled);
        assert!(!report.adapter_compiled);
        assert_eq!(report.adapter_mode, "disabled");
        assert_eq!(report.media_allowed_host_count, 0);
        assert_eq!(
            report.missing_configuration,
            vec!["QINTOPIA_HUABAOSI_IMAGE_API_KEY"]
        );
        assert!(!report.safe_for_chat);
        assert!(ensure_preflight_success(&report).is_err());
    }

    #[test]
    fn image_preflight_missing_configuration_is_public_and_deterministic() {
        let missing = missing_required_configuration_with(
            &["PUBLIC_READY", "PUBLIC_PLACEHOLDER", "PUBLIC_ABSENT"],
            |name| match name {
                "PUBLIC_READY" => Some("configured".to_string()),
                "PUBLIC_PLACEHOLDER" => Some("replace-with-secret".to_string()),
                _ => None,
            },
        );

        assert_eq!(missing, vec!["PUBLIC_PLACEHOLDER", "PUBLIC_ABSENT"]);
    }

    #[test]
    fn preflight_rejects_feature_and_enablement_mismatches() {
        let production_misconfigured =
            preflight_report(true, true, ImageAdapterMode::Disabled, 1, Vec::new());
        assert!(!production_misconfigured.success);
        assert_eq!(
            production_misconfigured.action_status,
            "live_adapter_not_compiled"
        );

        let staging_binary_not_enabled =
            preflight_report(true, false, ImageAdapterMode::Staging, 1, Vec::new());
        assert!(!staging_binary_not_enabled.success);
        assert_eq!(
            staging_binary_not_enabled.action_status,
            "live_adapter_compiled_requires_owner_review"
        );

        let reviewed_staging =
            preflight_report(true, true, ImageAdapterMode::Staging, 1, Vec::new());
        assert!(reviewed_staging.success);
        assert_eq!(reviewed_staging.action_status, "adapter_config_ready");

        let conflicting_features =
            preflight_report(true, true, ImageAdapterMode::Invalid, 1, Vec::new());
        assert!(!conflicting_features.success);
        assert_eq!(
            conflicting_features.action_status,
            "adapter_feature_conflict"
        );
    }

    #[test]
    fn staging_owner_approval_requires_exact_phrase() {
        assert!(validate_staging_owner_approval(Some(STAGING_APPROVAL_PHRASE)).is_ok());
        assert!(validate_staging_owner_approval(None).is_err());
        assert!(validate_staging_owner_approval(Some("approved-production-generation")).is_err());
        assert!(
            validate_staging_owner_approval(Some("approved-staging-image-generation\n")).is_err()
        );
    }

    #[test]
    fn production_owner_approval_requires_exact_phrase() {
        assert!(validate_production_owner_approval(Some(PRODUCTION_APPROVAL_PHRASE)).is_ok());
        assert!(validate_production_owner_approval(None).is_err());
        assert!(validate_production_owner_approval(Some(STAGING_APPROVAL_PHRASE)).is_err());
        assert!(
            validate_production_owner_approval(Some("approved-production-image-generation\n"))
                .is_err()
        );
    }

    #[test]
    fn production_release_boundary_requires_exact_deployed_sha() {
        let sha = "0123456789abcdef0123456789abcdef01234567";
        assert!(validate_production_release_boundary(Some(sha), Some(sha)).is_ok());
        assert!(validate_production_release_boundary(None, Some(sha)).is_err());
        assert!(validate_production_release_boundary(Some(sha), Some(&"f".repeat(40))).is_err());
        assert!(
            validate_production_release_boundary(Some(&sha.to_uppercase()), Some(sha)).is_err()
        );
    }

    #[test]
    fn production_database_boundary_requires_hash_and_production_name() {
        let database_url = "postgres://user:secret@127.0.0.1:5432/qintopia";
        let database_hash = format!("{:x}", Sha256::digest(database_url.as_bytes()));
        assert!(validate_production_database_boundary(database_url, Some(&database_hash)).is_ok());
        assert!(validate_production_database_boundary(database_url, Some("0")).is_err());
        assert!(validate_production_database_boundary(
            "postgres://user:secret@127.0.0.1:5432/qintopia_staging",
            Some(&format!(
                "{:x}",
                Sha256::digest("postgres://user:secret@127.0.0.1:5432/qintopia_staging".as_bytes())
            ))
        )
        .is_err());
    }

    #[test]
    fn staging_database_boundary_requires_reviewed_hash_allowlist() {
        let database_url = "postgres://user:secret@127.0.0.1:5432/qintopia_staging";
        let reviewed_hash = format!("{:x}", Sha256::digest(database_url.as_bytes()));

        assert!(validate_staging_database_boundary(database_url, false).is_err());
        assert!(validate_staging_database_boundary_with_allowlist(
            database_url,
            false,
            &[reviewed_hash.as_str()]
        )
        .is_ok());
        assert!(validate_staging_database_boundary_with_allowlist(
            "postgres://user:secret@127.0.0.1:5432/qintopia_production",
            false,
            &[reviewed_hash.as_str()],
        )
        .is_err());
        assert!(
            validate_staging_database_boundary_with_allowlist(database_url, false, &["0"]).is_err()
        );
    }

    #[test]
    fn disposable_database_exception_requires_exact_loopback_test_boundary() {
        let loopback_url = "postgres://user:secret@127.0.0.1:5432/qintopia_test";
        let loopback_hash = format!("{:x}", Sha256::digest(loopback_url.as_bytes()));
        assert!(validate_staging_database_boundary_with_allowlist(
            loopback_url,
            true,
            &[loopback_hash.as_str()]
        )
        .is_ok());
        assert!(validate_staging_database_boundary(loopback_url, true).is_err());
        assert!(validate_staging_database_boundary(loopback_url, false).is_err());

        let named_loopback_url = "postgres://user:secret@localhost:5432/qintopia_test";
        let named_loopback_hash = format!("{:x}", Sha256::digest(named_loopback_url.as_bytes()));
        assert!(validate_staging_database_boundary_with_allowlist(
            named_loopback_url,
            true,
            &[named_loopback_hash.as_str()]
        )
        .is_err());

        let remote_url = "postgres://user:secret@db.example.test:5432/qintopia_test";
        let remote_hash = format!("{:x}", Sha256::digest(remote_url.as_bytes()));
        assert!(validate_staging_database_boundary_with_allowlist(
            remote_url,
            true,
            &[remote_hash.as_str()]
        )
        .is_err());
    }

    #[test]
    fn ci_disposable_database_url_is_reviewed_explicitly() {
        let ci_loopback_url = "postgres://postgres:postgres@127.0.0.1:5432/qintopia_test";
        assert!(validate_staging_database_boundary(ci_loopback_url, true).is_ok());
    }

    #[test]
    fn disposable_adapter_exception_rejects_every_external_host() {
        assert!(validate_disposable_test_adapter_boundary(&test_http_config(1)).is_ok());
        assert!(validate_disposable_test_adapter_boundary(&test_config(
            "https://media.example.test/public"
        ))
        .is_err());
    }

    #[cfg(not(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter"
    )))]
    #[test]
    fn default_build_excludes_huabaosi_live_adapter() {
        assert_eq!(huabaosi_adapter_mode(), ImageAdapterMode::Disabled);
    }

    #[cfg(all(
        feature = "huabaosi-staging-adapter",
        not(feature = "huabaosi-production-adapter")
    ))]
    #[test]
    fn staging_feature_reports_huabaosi_adapter_compiled() {
        assert_eq!(huabaosi_adapter_mode(), ImageAdapterMode::Staging);
    }

    #[cfg(all(
        feature = "huabaosi-production-adapter",
        not(feature = "huabaosi-staging-adapter")
    ))]
    #[test]
    fn production_feature_reports_huabaosi_adapter_compiled() {
        assert_eq!(huabaosi_adapter_mode(), ImageAdapterMode::Production);
    }

    #[cfg(not(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter"
    )))]
    #[tokio::test]
    async fn default_apply_stops_before_database_and_network() {
        let cli = Cli::parse_from([
            "qintopia-message-sidecar",
            "run-huabaosi-image-generation-worker",
            "--once",
            "--apply",
        ]);

        let error = run(&cli, true, None, true, false, false)
            .await
            .expect_err("default apply must fail before requiring a database URL");

        assert!(error.to_string().contains("not compiled"));
        assert!(!error.to_string().contains("DATABASE_URL"));
    }

    #[test]
    fn production_endpoints_reject_http() {
        assert!(https_url("http://127.0.0.1:8080/images", "provider").is_err());
    }

    #[test]
    fn image_http_timeout_is_bounded_below_claim_lease() {
        assert_eq!(
            image_http_timeout(None).expect("default timeout"),
            Duration::from_secs(180)
        );
        assert_eq!(
            image_http_timeout(Some("60")).expect("minimum timeout"),
            Duration::from_secs(60)
        );
        assert_eq!(
            image_http_timeout(Some("240")).expect("maximum timeout"),
            Duration::from_secs(240)
        );
        assert!(image_http_timeout(Some("59")).is_err());
        assert!(image_http_timeout(Some("241")).is_err());
        assert!(image_http_timeout(Some("invalid")).is_err());
    }

    #[test]
    fn production_endpoints_reject_query_parameters() {
        assert!(https_url("https://media.example.test/upload?token=secret", "media").is_err());
        assert!(https_url("https://media.example.test/public%2Fupload", "media").is_err());
        assert!(https_url("https://media.example.test/public%5Cupload", "media").is_err());
    }

    #[test]
    fn request_headers_reject_injection_characters_before_connecting() {
        let header_value = "Bearer test\r\nX-Injected: true";
        let value_error = validate_http_header("Authorization", header_value)
            .expect_err("header values must reject CR/LF");
        assert!(value_error.to_string().contains("invalid header value"));
        assert!(!value_error.to_string().contains("X-Injected"));

        let name_error = validate_http_header("X-Test\nInjected", "safe")
            .expect_err("header names must reject CR/LF");
        assert!(name_error.to_string().contains("invalid header name"));
    }

    #[test]
    fn invalid_provider_headers_are_terminal_and_never_retried() {
        let mut config = test_config("https://media.example.test/public");
        config.api_key = "test\r\nX-Injected: true".to_string();

        let error = generate_and_store_with_client(
            &config,
            &fixture_work_item(),
            Uuid::nil(),
            HttpClient::test_only(),
        )
        .expect_err("invalid header must fail before connecting");

        assert_eq!(error.class, GenerationFailureClass::Terminal);
        assert_eq!(error.stage, "provider_request");
        assert!(error.to_string().contains("invalid header value"));
        assert!(!should_retry_generation(error.class, 1));
    }

    #[test]
    fn refused_provider_connection_is_retryable_transport() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("reserve loopback port");
        let port = listener.local_addr().expect("loopback address").port();
        drop(listener);
        let mut config = test_config("https://media.example.test/public");
        config.provider_endpoint =
            Url::parse(&format!("https://127.0.0.1:{port}/v1/images/generations"))
                .expect("loopback provider URL");

        let error = generate_and_store_with_client(
            &config,
            &fixture_work_item(),
            Uuid::nil(),
            HttpClient::production(),
        )
        .expect_err("refused loopback provider must fail");

        assert_eq!(error.class, GenerationFailureClass::RetryableProvider);
        assert_eq!(error.stage, "provider_transport");
        assert!(should_retry_generation(error.class, 1));
    }

    #[test]
    fn provider_timeout_after_send_is_ambiguous_and_never_retried() {
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake provider");
        let port = listener.local_addr().expect("fake provider address").port();
        let server = thread::spawn(move || {
            let (mut stream, _) = listener.accept().expect("accept provider request");
            let mut request = [0_u8; 4096];
            assert!(stream.read(&mut request).expect("read provider request") > 0);
            thread::sleep(Duration::from_millis(150));
        });
        let mut config = test_config("https://media.example.test/public");
        config.provider_endpoint =
            Url::parse(&format!("http://127.0.0.1:{port}/v1/images/generations"))
                .expect("fake provider URL");

        let error = generate_and_store_with_client(
            &config,
            &fixture_work_item(),
            Uuid::nil(),
            HttpClient::test_only_with_timeout(Duration::from_millis(50)),
        )
        .expect_err("post-send provider timeout must fail");
        server.join().expect("fake provider joins");

        assert_eq!(error.class, GenerationFailureClass::AmbiguousProvider);
        assert_eq!(error.stage, "provider_transport");
        assert!(!should_retry_generation(error.class, 1));
    }

    #[test]
    fn claims_use_unique_per_attempt_tokens() {
        let first = new_claim_token();
        let second = new_claim_token();

        assert!(first.starts_with(&format!("{WORKER_ID}:")));
        assert_ne!(first, second);
    }

    #[test]
    fn generated_image_artifact_id_is_stable_and_content_bound() {
        let work_item_id = Uuid::new_v4();
        let first = generated_image_artifact_id(work_item_id, "sha256:first");
        let repeated = generated_image_artifact_id(work_item_id, "sha256:first");
        let changed = generated_image_artifact_id(work_item_id, "sha256:changed");

        assert_eq!(first, repeated);
        assert_ne!(first, changed);
        assert_eq!(first.get_version_num(), 8);
    }

    #[test]
    #[cfg(feature = "postgres-integration-tests")]
    fn postgres_integration_database_guard_rejects_hostname_resolution() {
        assert!(validate_postgres_integration_database_url(
            "postgres://postgres:postgres@127.0.0.1:5432/qintopia_test"
        )
        .is_ok());
        assert!(validate_postgres_integration_database_url(
            "postgres://postgres:postgres@[::1]:5432/qintopia_test"
        )
        .is_ok());
        assert!(validate_postgres_integration_database_url(
            "postgres://postgres:postgres@localhost:5432/qintopia_test"
        )
        .is_err());
        assert!(validate_postgres_integration_database_url(
            "postgres://postgres:postgres@127.0.0.1:5432/qintopia"
        )
        .is_err());
    }

    #[tokio::test]
    #[cfg(feature = "postgres-integration-tests")]
    #[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
    async fn postgres_stale_processing_claim_is_terminal_ambiguous() {
        let database_url = postgres_integration_database_url();
        let pool = db::connect(&database_url, 1)
            .await
            .expect("connect to guarded integration database");
        db::run_migrations(&pool)
            .await
            .expect("run integration migrations");
        let (parent_id, request_id, prior_claim_token) =
            insert_processing_claim_fixture(&pool).await;

        let preview = load_work_item(&pool, Some(request_id))
            .await
            .expect("preview stale request safely");
        assert!(preview.is_none());
        let claimed = claim_work_item(&pool, Some(request_id))
            .await
            .expect("reconcile stale request");
        assert!(matches!(
            claimed,
            ImageGenerationClaimOutcome::ReconciledAmbiguous(id) if id == request_id
        ));

        let state: (String, i32, bool, bool, bool, Option<String>) = sqlx::query_as(
            r#"
            SELECT status, attempts, claimed_by IS NULL, locked_at IS NULL,
                   claim_expires_at IS NULL, last_error
            FROM qintopia_agent_os.work_items
            WHERE id = $1
            "#,
        )
        .bind(request_id)
        .fetch_one(&pool)
        .await
        .expect("read terminal stale request");
        assert_eq!(state.0, "failed");
        assert_eq!(state.1, 1);
        assert!(state.2 && state.3 && state.4);
        assert_eq!(
            state.5.as_deref(),
            Some(
                "image generation external outcome ambiguous after claim loss; automatic retry disabled"
            )
        );

        let events: Vec<(Value,)> = sqlx::query_as(
            r#"
            SELECT data
            FROM qintopia_agent_os.work_item_events
            WHERE work_item_id = $1
              AND event_type = 'image_generation_outcome_ambiguous'
            "#,
        )
        .bind(request_id)
        .fetch_all(&pool)
        .await
        .expect("read ambiguous image generation event");
        assert_eq!(events.len(), 1);
        let event = &events[0].0;
        assert_eq!(event["attempt_number"], 1);
        assert_eq!(event["failure_class"], "ambiguous");
        assert_eq!(event["failure_stage"], "claim_lost");
        assert_eq!(event["automatic_retry_allowed"], false);
        assert_eq!(event["external_generation_executed"], Value::Null);
        assert_eq!(event["external_media_upload_executed"], Value::Null);
        assert_eq!(event["external_publish_executed"], false);
        assert_eq!(event["sensitive_fields_redacted"], true);
        let serialized = serde_json::to_string(event).expect("serialize sanitized event");
        assert!(!serialized.contains(&prior_claim_token));

        let duplicate = claim_work_item(&pool, Some(request_id))
            .await
            .expect("terminal request remains unclaimable");
        assert!(matches!(duplicate, ImageGenerationClaimOutcome::Empty));
        let counts: (i64, i64) = sqlx::query_as(
            r#"
            SELECT
                (SELECT count(*) FROM qintopia_agent_os.work_item_events
                 WHERE work_item_id = $1
                   AND event_type = 'image_generation_outcome_ambiguous'),
                (SELECT count(*) FROM qintopia_agent_os.artifacts
                 WHERE work_item_id = $1 AND artifact_type = 'generated_image')
            "#,
        )
        .bind(request_id)
        .fetch_one(&pool)
        .await
        .expect("read idempotent stale claim counts");
        assert_eq!(counts, (1, 0));

        delete_processing_claim_fixture(&pool, parent_id, request_id).await;
    }

    #[test]
    fn retry_policy_is_bounded_to_recoverable_provider_failures() {
        for status in [408, 429, 500, 503, 599] {
            assert_eq!(
                classify_provider_response(status),
                GenerationFailureClass::RetryableProvider
            );
        }
        for status in [200, 400, 401, 403, 404] {
            assert_eq!(
                classify_provider_response(status),
                GenerationFailureClass::Terminal
            );
        }
        assert_eq!(generation_retry_delay_seconds(1), 60);
        assert_eq!(generation_retry_delay_seconds(2), 120);
        assert_eq!(generation_retry_delay_seconds(3), 240);
        assert!(should_retry_generation(
            GenerationFailureClass::RetryableProvider,
            1
        ));
        assert!(!should_retry_generation(
            GenerationFailureClass::RetryableProvider,
            3
        ));
        assert!(!should_retry_generation(
            GenerationFailureClass::Terminal,
            1
        ));
        assert!(!should_retry_generation(
            GenerationFailureClass::AmbiguousProvider,
            1
        ));

        let pre_send = GenerationFailure {
            class: GenerationFailureClass::RetryableProvider,
            stage: "provider_transport",
        };
        assert_eq!(failure_external_generation_executed(pre_send), Some(false));
        assert_eq!(failure_external_media_write_executed(pre_send), Some(false));

        let ambiguous = GenerationFailure {
            class: GenerationFailureClass::AmbiguousProvider,
            stage: "provider_transport",
        };
        assert_eq!(failure_external_generation_executed(ambiguous), None);
        assert_eq!(
            failure_external_media_write_executed(ambiguous),
            Some(false)
        );

        let media_upload = GenerationFailure {
            class: GenerationFailureClass::Terminal,
            stage: "media_upload",
        };
        assert_eq!(
            failure_external_generation_executed(media_upload),
            Some(true)
        );
        assert_eq!(failure_external_media_write_executed(media_upload), None);

        let media_readback = GenerationFailure {
            class: GenerationFailureClass::Terminal,
            stage: "media_readback",
        };
        assert_eq!(
            failure_external_generation_executed(media_readback),
            Some(true)
        );
        assert_eq!(
            failure_external_media_write_executed(media_readback),
            Some(true)
        );

        let persistence = GenerationFailure {
            class: GenerationFailureClass::Terminal,
            stage: "persistence",
        };
        assert_eq!(
            failure_external_generation_executed(persistence),
            Some(true)
        );
        assert_eq!(
            failure_external_media_write_executed(persistence),
            Some(true)
        );

        let worker_execution = GenerationFailure {
            class: GenerationFailureClass::Terminal,
            stage: "worker_execution",
        };
        assert_eq!(failure_external_generation_executed(worker_execution), None);
        assert_eq!(
            failure_external_media_write_executed(worker_execution),
            None
        );
    }

    #[test]
    fn response_reading_and_decoding_enforce_size_limits() {
        let mut oversized_headers = Cursor::new(vec![0_u8; MAX_HTTP_RESPONSE_HEADER_BYTES + 1]);
        let read_error = read_response_limited(&mut oversized_headers, 0)
            .expect_err("raw response must be capped before parsing");
        assert!(read_error.to_string().contains("size limit"));
        assert!(!read_error.transport);

        let response = b"HTTP/1.1 200 OK\r\nContent-Length: 5\r\n\r\n12345".to_vec();
        let parse_error = parse_http_response(response, 4)
            .err()
            .expect("content length must be capped before body handling");
        assert!(parse_error.to_string().contains("size limit"));

        let mut oversized_header = b"HTTP/1.1 200 OK\r\nX-Padding: ".to_vec();
        oversized_header.resize(MAX_HTTP_RESPONSE_HEADER_BYTES + 1, b'a');
        oversized_header.extend_from_slice(b"\r\n\r\n");
        let header_error = parse_http_response(oversized_header, 1024)
            .err()
            .expect("response headers must have an independent cap");
        assert!(header_error.to_string().contains("headers exceeded"));

        let chunked_error = decode_chunked_body(b"5\r\n12345\r\n0\r\n\r\n", 4)
            .expect_err("chunked bodies must use the same cap");
        assert!(chunked_error.to_string().contains("size limit"));
    }

    #[test]
    fn invalid_tls_or_protocol_io_is_terminal() {
        let invalid_data = request_io_error(
            "test TLS validation",
            io::Error::new(io::ErrorKind::InvalidData, "invalid certificate"),
        );
        let timed_out = request_io_error(
            "test provider timeout",
            io::Error::new(io::ErrorKind::TimedOut, "timed out"),
        );

        assert!(!invalid_data.transport);
        assert!(timed_out.transport);
    }

    #[test]
    fn only_pending_generated_images_can_be_reused() {
        assert!(can_reuse_existing_generated_image("pending"));
        assert!(!can_reuse_existing_generated_image("approved"));
        assert!(!can_reuse_existing_generated_image("rejected"));
        assert!(!can_reuse_existing_generated_image("changes_requested"));
    }

    #[test]
    fn pending_generated_image_reuse_requires_exact_immutable_metadata() {
        let work_item = fixture_work_item();
        let generated = GeneratedImage {
            artifact_id: Uuid::new_v4(),
            workflow_root_id: Uuid::nil(),
            bytes: vec![1, 2, 3],
            content_hash: format!("sha256:{}", "a".repeat(64)),
            file_md5: "5289df737df57326fcdd22597afb1fac".to_string(),
            provider_source_content_hash: format!("sha256:{}", "b".repeat(64)),
            width: 1024,
            height: 1024,
            artifact_uri: "https://media.example.test/public/image.jpg".to_string(),
            storage_provider: HTTP_STORAGE_BACKEND,
            feishu_record_id: None,
        };
        let source_ids = json!([{
            "approved_brief_artifact_id": work_item.approved_brief_artifact_id,
            "approved_brief_content_hash": work_item.approved_brief_content_hash,
        }]);
        let metadata = generated_image_metadata(&work_item, &generated);

        assert!(existing_generated_image_matches(
            "pending",
            Some(&generated.artifact_uri),
            &source_ids,
            &metadata,
            &work_item,
            &generated,
        ));

        let mut stale_metadata = metadata.clone();
        stale_metadata["provider_source_content_hash"] =
            json!(format!("sha256:{}", "c".repeat(64)));
        assert!(!existing_generated_image_matches(
            "pending",
            Some(&generated.artifact_uri),
            &source_ids,
            &stale_metadata,
            &work_item,
            &generated,
        ));
        assert!(!existing_generated_image_matches(
            "approved",
            Some(&generated.artifact_uri),
            &source_ids,
            &metadata,
            &work_item,
            &generated,
        ));
    }

    #[test]
    fn media_response_must_stay_within_public_base_and_allowlist() {
        let config = test_config("https://media.example.test/public");
        let metadata = ImageMetadata {
            width: 1024,
            height: 1024,
        };
        let media = MediaUploadResponse {
            uri: "https://other.example.test/public/image.jpg".to_string(),
            content_hash: "sha256:abc".to_string(),
            mime_type: FINAL_IMAGE_MIME_TYPE.to_string(),
            byte_size: 12,
            width: 1024,
            height: 1024,
        };
        assert!(validate_media_response(
            http_storage(&config),
            &media,
            "sha256:abc",
            &metadata,
            12,
            false,
        )
        .is_err());
    }

    #[test]
    fn media_response_cannot_escape_public_path_prefix() {
        let config = test_config("https://media.example.test/public");
        let metadata = ImageMetadata {
            width: 1024,
            height: 1024,
        };
        let media = MediaUploadResponse {
            uri: "https://media.example.test/publicity/image.jpg".to_string(),
            content_hash: "sha256:abc".to_string(),
            mime_type: FINAL_IMAGE_MIME_TYPE.to_string(),
            byte_size: 12,
            width: 1024,
            height: 1024,
        };
        assert!(validate_media_response(
            http_storage(&config),
            &media,
            "sha256:abc",
            &metadata,
            12,
            false,
        )
        .is_err());

        let media = MediaUploadResponse {
            uri: "https://media.example.test/public%2Fprivate/image.jpg".to_string(),
            content_hash: "sha256:abc".to_string(),
            mime_type: FINAL_IMAGE_MIME_TYPE.to_string(),
            byte_size: 12,
            width: 1024,
            height: 1024,
        };
        assert!(validate_media_response(
            http_storage(&config),
            &media,
            "sha256:abc",
            &metadata,
            12,
            false,
        )
        .is_err());
    }

    #[test]
    fn provider_error_does_not_echo_response_body() {
        let response = HttpResponse {
            status: 401,
            headers: BTreeMap::new(),
            body: b"secret provider detail".to_vec(),
        };
        let error = parse_provider_response(&response).expect_err("provider should fail");
        assert!(error.to_string().contains("HTTP 401"));
        assert!(!error.to_string().contains("secret provider detail"));
    }

    #[test]
    fn provider_response_requires_b64_json_without_echoing_body() {
        let response = HttpResponse {
            status: 200,
            headers: BTreeMap::new(),
            body: br#"{"data":[{"url":"https://provider.example.test/private"}]}"#.to_vec(),
        };
        let error = parse_provider_response(&response).expect_err("b64_json is required");

        assert!(error.to_string().contains("did not contain b64_json"));
        assert!(!error.to_string().contains("provider.example.test"));
    }

    #[test]
    fn media_upload_metadata_must_match_generated_image() {
        let config = test_config("https://media.example.test/public");
        let metadata = ImageMetadata {
            width: 1024,
            height: 1024,
        };
        let media = MediaUploadResponse {
            uri: "https://media.example.test/public/image.jpg".to_string(),
            content_hash: "sha256:unexpected".to_string(),
            mime_type: FINAL_IMAGE_MIME_TYPE.to_string(),
            byte_size: 12,
            width: 1024,
            height: 1024,
        };
        let error = validate_media_response(
            http_storage(&config),
            &media,
            "sha256:expected",
            &metadata,
            12,
            false,
        )
        .expect_err("upload metadata must match generated image");

        assert!(error.to_string().contains("metadata did not match"));
    }

    #[test]
    fn media_upload_uri_must_name_the_final_jpeg_object() {
        let config = test_config("https://media.example.test/public");
        let metadata = ImageMetadata {
            width: 1024,
            height: 1024,
        };
        let media = MediaUploadResponse {
            uri: "https://media.example.test/public/image.png".to_string(),
            content_hash: "sha256:expected".to_string(),
            mime_type: FINAL_IMAGE_MIME_TYPE.to_string(),
            byte_size: 12,
            width: 1024,
            height: 1024,
        };

        let error = validate_media_response(
            http_storage(&config),
            &media,
            "sha256:expected",
            &metadata,
            12,
            false,
        )
        .expect_err("final media URI must use a JPEG suffix");

        assert!(error.to_string().contains("JPEG object"));
    }

    #[test]
    fn png_to_jpeg_conversion_is_deterministic_and_bounded() {
        let source = fixture_png();
        let decoded =
            decode_provider_png(&source, DEFAULT_MAX_MEDIA_BYTES).expect("fixture PNG must decode");
        let first = encode_final_jpeg(&decoded, DEFAULT_MAX_MEDIA_BYTES)
            .expect("fixture must encode to JPEG");
        let second = encode_final_jpeg(&decoded, DEFAULT_MAX_MEDIA_BYTES)
            .expect("same fixture must encode identically");
        let metadata =
            inspect_final_jpeg(&first, DEFAULT_MAX_MEDIA_BYTES).expect("final JPEG must decode");

        assert_eq!(first, second);
        assert!(first.starts_with(&[0xff, 0xd8]));
        assert!(first.ends_with(&[0xff, 0xd9]));
        assert_eq!((metadata.width, metadata.height), (1024, 1024));
        assert!(first.len() <= DEFAULT_MAX_MEDIA_BYTES);
    }

    #[test]
    fn alpha_compositing_uses_exact_white_background_integer_rule() {
        let source = RgbaImage::from_vec(
            3,
            1,
            vec![10, 20, 30, 255, 10, 20, 30, 0, 40, 120, 200, 128],
        )
        .expect("fixture dimensions match");

        assert_eq!(
            composite_rgba_over_white(&source),
            vec![10, 20, 30, 255, 255, 255, 147, 187, 227]
        );
    }

    #[test]
    fn png_decoder_rejects_header_only_fixture() {
        let mut bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec();
        bytes.extend_from_slice(&1024_u32.to_be_bytes());
        bytes.extend_from_slice(&1024_u32.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);

        assert!(decode_provider_png(&bytes, DEFAULT_MAX_MEDIA_BYTES).is_err());
    }

    #[test]
    fn decoder_limits_reject_oversized_dimensions() {
        let oversized = fixture_png_with_dimensions(1025, 1, [1, 2, 3, 255]);

        let error = decode_provider_png(&oversized, DEFAULT_MAX_MEDIA_BYTES)
            .expect_err("decoder must reject oversized dimensions before conversion");

        assert!(error.to_string().contains("decode generated PNG"));
    }

    #[test]
    fn upload_idempotency_includes_transform_and_final_hash() {
        let first = media_upload_idempotency_key("sha256:prompt", "sha256:jpeg-a");
        let repeated = media_upload_idempotency_key("sha256:prompt", "sha256:jpeg-a");
        let changed = media_upload_idempotency_key("sha256:prompt", "sha256:jpeg-b");

        assert_eq!(first, repeated);
        assert_ne!(first, changed);
        assert!(first.starts_with("sha256:"));
    }

    #[test]
    fn fake_provider_png_is_converted_before_media_round_trip() {
        let source_png = fixture_png();
        let final_jpeg = fixture_jpeg(&source_png);
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
        let port = listener.local_addr().expect("fake server address").port();
        let source_for_server = source_png.clone();
        let final_for_server = final_jpeg.clone();
        let handle = thread::spawn(move || {
            for expected_path in ["/v1/images/generations", "/upload", "/public/image.jpg"] {
                let (mut stream, _) = listener.accept().expect("accept fake request");
                let request = read_request(&mut stream);
                assert!(request.headers.starts_with(&format!(
                    "{} ",
                    if expected_path == "/public/image.jpg" {
                        "GET"
                    } else {
                        "POST"
                    }
                )));
                assert!(request.headers.contains(expected_path));
                let response = match expected_path {
                    "/v1/images/generations" => {
                        let encoded = Base64::encode_string(&source_for_server);
                        json_response(&json!({"data":[{"b64_json": encoded}]}).to_string())
                    }
                    "/upload" => {
                        assert!(request
                            .headers
                            .to_ascii_lowercase()
                            .contains("x-qintopia-content-hash: sha256:"));
                        assert!(request.headers.contains("Content-Type: image/jpeg"));
                        assert_eq!(request.body, final_for_server);
                        let uri = format!("http://127.0.0.1:{port}/public/image.jpg");
                        json_response(
                            &json!({
                                "uri": uri,
                                "content_hash": content_hash_bytes(&final_for_server),
                                "mime_type": FINAL_IMAGE_MIME_TYPE,
                                "byte_size": final_for_server.len(),
                                "width": 1024,
                                "height": 1024,
                            })
                            .to_string(),
                        )
                    }
                    _ => binary_response(&final_for_server, FINAL_IMAGE_MIME_TYPE),
                };
                stream.write_all(&response).expect("write fake response");
            }
        });

        let base = Url::parse(&format!("http://127.0.0.1:{port}/v1/")).expect("fake provider URL");
        let config = AdapterConfig {
            model: "gpt-image-2".to_string(),
            provider_endpoint: base
                .join("images/generations")
                .expect("fake provider endpoint"),
            api_key: "test-key".to_string(),
            storage: StorageConfig::Http(HttpMediaConfig {
                media_upload_endpoint: Url::parse(&format!("http://127.0.0.1:{port}/upload"))
                    .expect("fake upload URL"),
                media_public_base_url: Url::parse(&format!("http://127.0.0.1:{port}/public"))
                    .expect("fake public URL"),
                media_allowed_hosts: BTreeSet::from(["127.0.0.1".to_string()]),
            }),
            max_media_bytes: DEFAULT_MAX_MEDIA_BYTES,
            http_timeout: Duration::from_secs(DEFAULT_IMAGE_HTTP_TIMEOUT_SECONDS),
        };
        let generated = generate_and_store_with_client(
            &config,
            &fixture_work_item(),
            Uuid::nil(),
            HttpClient::test_only(),
        )
        .expect("fake image generation succeeds");
        handle.join().expect("fake server joins");

        assert_eq!(generated.bytes, final_jpeg);
        assert_eq!(generated.file_md5, md5_hex_bytes(&generated.bytes));
        assert_eq!(
            generated.provider_source_content_hash,
            content_hash_bytes(&source_png)
        );
        assert_eq!(generated.width, 1024);
        assert_eq!(generated.height, 1024);
        assert!(generated.artifact_uri.contains("/public/image.jpg"));
    }

    #[test]
    fn fake_media_readback_must_match_uploaded_bytes() {
        let source_png = fixture_png();
        let final_jpeg = fixture_jpeg(&source_png);
        let different_jpeg = fixture_jpeg(&fixture_png_with_color([1, 2, 3, 255]));
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
        let port = listener.local_addr().expect("fake server address").port();
        let source_for_server = source_png.clone();
        let final_for_server = final_jpeg.clone();
        let handle = thread::spawn(move || {
            for expected_path in ["/v1/images/generations", "/upload", "/public/image.jpg"] {
                let (mut stream, _) = listener.accept().expect("accept fake request");
                let request = read_request(&mut stream);
                let response = match expected_path {
                    "/v1/images/generations" => json_response(
                        &json!({"data":[{"b64_json": Base64::encode_string(&source_for_server)}]})
                            .to_string(),
                    ),
                    "/upload" => json_response(
                        &json!({
                            "uri": format!("http://127.0.0.1:{port}/public/image.jpg"),
                            "content_hash": content_hash_bytes(&final_for_server),
                            "mime_type": FINAL_IMAGE_MIME_TYPE,
                            "byte_size": final_for_server.len(),
                            "width": 1024,
                            "height": 1024,
                        })
                        .to_string(),
                    ),
                    _ => binary_response(&different_jpeg, FINAL_IMAGE_MIME_TYPE),
                };
                if expected_path == "/upload" {
                    assert_eq!(request.body, final_for_server);
                }
                stream.write_all(&response).expect("write fake response");
            }
        });

        let config = test_http_config(port);
        let error = generate_and_store_with_client(
            &config,
            &fixture_work_item(),
            Uuid::nil(),
            HttpClient::test_only(),
        )
        .expect_err("readback with different bytes must fail");
        handle.join().expect("fake server joins");

        assert!(error.to_string().contains("did not match uploaded image"));
    }

    #[test]
    #[cfg(any(
        feature = "huabaosi-production-adapter",
        feature = "huabaosi-staging-adapter",
        feature = "huabaosi-feishu-mirror-adapter"
    ))]
    fn fake_provider_stores_final_jpeg_in_feishu_before_returning_artifact() {
        let source_png = fixture_png();
        let final_jpeg = fixture_jpeg(&source_png);
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
        let port = listener.local_addr().expect("fake server address").port();
        let source_for_server = source_png.clone();
        let final_for_server = final_jpeg.clone();
        let handle = thread::spawn(move || {
            let expected_paths = [
                "/v1/images/generations",
                "/open-apis/auth/v3/tenant_access_token/internal",
                "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records/search",
                "/open-apis/drive/v1/medias/upload_all",
                "/open-apis/drive/v1/medias/fileFixture/download",
                "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records",
            ];
            for path in expected_paths {
                let (mut stream, _) = listener.accept().expect("accept fake request");
                let request = read_request(&mut stream);
                assert!(request
                    .headers
                    .lines()
                    .next()
                    .unwrap_or_default()
                    .contains(path));
                let response = match path {
                    "/v1/images/generations" => json_response(
                        &json!({"data":[{"b64_json": Base64::encode_string(&source_for_server)}]})
                            .to_string(),
                    ),
                    "/open-apis/auth/v3/tenant_access_token/internal" => json_response(
                        r#"{"code":0,"tenant_access_token":"tenantFixture"}"#,
                    ),
                    "/open-apis/bitable/v1/apps/baseTokenFixture/tables/tblFixture/records/search" => {
                        assert!(String::from_utf8_lossy(&request.body).contains("AgentOS产物ID"));
                        json_response(r#"{"code":0,"data":{"items":[]}}"#)
                    }
                    "/open-apis/drive/v1/medias/upload_all" => {
                        assert!(request
                            .body
                            .windows(final_for_server.len())
                            .any(|window| window == final_for_server));
                        json_response(r#"{"code":0,"data":{"file_token":"fileFixture"}}"#)
                    }
                    "/open-apis/drive/v1/medias/fileFixture/download" => {
                        assert!(request.headers.contains("Authorization: Bearer tenantFixture"));
                        binary_response(&final_for_server, FINAL_IMAGE_MIME_TYPE)
                    }
                    _ => {
                        let body = String::from_utf8_lossy(&request.body);
                        assert!(body.contains("fileFixture"));
                        assert!(body.contains("待审核"));
                        json_response(
                            r#"{"code":0,"data":{"record":{"record_id":"recFixture"}}}"#,
                        )
                    }
                };
                stream.write_all(&response).expect("write fake response");
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

        let config = AdapterConfig {
            model: "gpt-image-2".to_string(),
            provider_endpoint: Url::parse(&format!(
                "http://127.0.0.1:{port}/v1/images/generations"
            ))
            .expect("fake provider endpoint"),
            api_key: "test-key".to_string(),
            storage: StorageConfig::Feishu(FeishuPrimaryStorageConfig::test_only(
                Url::parse(&format!("http://127.0.0.1:{port}/open-apis/"))
                    .expect("fake Feishu API root"),
                profile_path.to_string_lossy().to_string(),
                DEFAULT_MAX_MEDIA_BYTES,
            )),
            max_media_bytes: DEFAULT_MAX_MEDIA_BYTES,
            http_timeout: Duration::from_secs(DEFAULT_IMAGE_HTTP_TIMEOUT_SECONDS),
        };
        let work_item = fixture_work_item();
        let generated = generate_and_store_with_client(
            &config,
            &work_item,
            Uuid::nil(),
            HttpClient::test_only(),
        )
        .expect("fake Feishu-backed image generation succeeds");
        handle.join().expect("fake server joins");

        assert_eq!(generated.bytes, final_jpeg);
        assert_eq!(generated.storage_provider, FEISHU_STORAGE_BACKEND);
        assert_eq!(generated.feishu_record_id.as_deref(), Some("recFixture"));
        assert_eq!(
            generated.artifact_uri,
            format!(
                "feishu-base://huabaosi-generated-image/{}",
                generated.artifact_id
            )
        );
    }

    fn fixture_work_item() -> ImageGenerationWorkItem {
        ImageGenerationWorkItem {
            id: Uuid::new_v4(),
            claim_token: None,
            attempts: 0,
            approved_brief_artifact_id: Uuid::new_v4(),
            approved_brief_content_hash: "sha256:approved-brief".to_string(),
            approved_brief_text: "活动主题：周末共创晚餐。时间地点以已审核活动信息为准。"
                .to_string(),
            image_specification: SPECIFICATION.to_string(),
            prompt_hash: "sha256:prompt".to_string(),
        }
    }

    fn test_config(public_base: &str) -> AdapterConfig {
        AdapterConfig {
            model: "gpt-image-2".to_string(),
            provider_endpoint: Url::parse("https://provider.example.test/v1/images/generations")
                .unwrap(),
            api_key: "test-key".to_string(),
            storage: StorageConfig::Http(HttpMediaConfig {
                media_upload_endpoint: Url::parse("https://media.example.test/upload").unwrap(),
                media_public_base_url: Url::parse(public_base).unwrap(),
                media_allowed_hosts: BTreeSet::from(["media.example.test".to_string()]),
            }),
            max_media_bytes: DEFAULT_MAX_MEDIA_BYTES,
            http_timeout: Duration::from_secs(DEFAULT_IMAGE_HTTP_TIMEOUT_SECONDS),
        }
    }

    fn http_storage(config: &AdapterConfig) -> &HttpMediaConfig {
        let StorageConfig::Http(storage) = &config.storage else {
            panic!("test requires HTTP media storage");
        };
        storage
    }

    fn test_http_config(port: u16) -> AdapterConfig {
        AdapterConfig {
            model: "gpt-image-2".to_string(),
            provider_endpoint: Url::parse(&format!(
                "http://127.0.0.1:{port}/v1/images/generations"
            ))
            .expect("fake provider endpoint"),
            api_key: "test-key".to_string(),
            storage: StorageConfig::Http(HttpMediaConfig {
                media_upload_endpoint: Url::parse(&format!("http://127.0.0.1:{port}/upload"))
                    .expect("fake upload endpoint"),
                media_public_base_url: Url::parse(&format!("http://127.0.0.1:{port}/public"))
                    .expect("fake public base"),
                media_allowed_hosts: BTreeSet::from(["127.0.0.1".to_string()]),
            }),
            max_media_bytes: DEFAULT_MAX_MEDIA_BYTES,
            http_timeout: Duration::from_secs(DEFAULT_IMAGE_HTTP_TIMEOUT_SECONDS),
        }
    }

    fn fixture_png() -> Vec<u8> {
        fixture_png_with_color([40, 120, 200, 128])
    }

    fn fixture_png_with_color(color: [u8; 4]) -> Vec<u8> {
        fixture_png_with_dimensions(1024, 1024, color)
    }

    fn fixture_png_with_dimensions(width: u32, height: u32, color: [u8; 4]) -> Vec<u8> {
        let image = RgbaImage::from_pixel(width, height, image::Rgba(color));
        let mut bytes = Vec::new();
        image::codecs::png::PngEncoder::new(&mut bytes)
            .write_image(image.as_raw(), width, height, ExtendedColorType::Rgba8)
            .expect("encode fixture PNG");
        bytes
    }

    fn fixture_jpeg(source_png: &[u8]) -> Vec<u8> {
        let source =
            decode_provider_png(source_png, DEFAULT_MAX_MEDIA_BYTES).expect("fixture PNG decodes");
        encode_final_jpeg(&source, DEFAULT_MAX_MEDIA_BYTES).expect("fixture JPEG encodes")
    }

    struct TestRequest {
        headers: String,
        body: Vec<u8>,
    }

    fn read_request(stream: &mut std::net::TcpStream) -> TestRequest {
        stream
            .set_read_timeout(Some(Duration::from_secs(2)))
            .unwrap();
        let mut request = Vec::new();
        let mut buffer = [0_u8; 1024];
        while !request.windows(4).any(|window| window == b"\r\n\r\n") {
            let count = stream.read(&mut buffer).unwrap();
            assert_ne!(count, 0, "request ended before headers");
            request.extend_from_slice(&buffer[..count]);
        }
        let header_end = request
            .windows(4)
            .position(|window| window == b"\r\n\r\n")
            .unwrap()
            + 4;
        let headers = String::from_utf8(request[..header_end].to_vec()).unwrap();
        let content_length = headers
            .lines()
            .find_map(|line| line.strip_prefix("Content-Length: "))
            .unwrap_or("0")
            .parse::<usize>()
            .unwrap();
        while request.len() < header_end + content_length {
            let count = stream.read(&mut buffer).unwrap();
            assert_ne!(count, 0, "request ended before body");
            request.extend_from_slice(&buffer[..count]);
        }
        TestRequest {
            headers,
            body: request[header_end..header_end + content_length].to_vec(),
        }
    }

    fn json_response(body: &str) -> Vec<u8> {
        format!(
            "HTTP/1.1 200 OK\r\nContent-Type: application/json\r\nContent-Length: {}\r\nConnection: close\r\n\r\n{body}",
            body.len()
        )
        .into_bytes()
    }

    fn binary_response(body: &[u8], mime_type: &str) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: {mime_type}\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        response
    }
}
