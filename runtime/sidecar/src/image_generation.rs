use std::{
    collections::{BTreeMap, BTreeSet},
    io::{Read, Write},
    net::TcpStream,
    sync::Arc,
    time::Duration,
};

use anyhow::{anyhow, bail, Context, Result};
use base64ct::{Base64, Encoding};
use rustls::{ClientConfig, ClientConnection, OwnedTrustAnchor, RootCertStore, ServerName, Stream};
use serde::{Deserialize, Serialize};
use serde_json::json;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use url::Url;
use uuid::Uuid;

use crate::{config::Cli, db};

const WORKER_ID: &str = "huabaosi-image-generation-worker";
const CAPABILITY_KEY: &str = "huabaosi.generate_image_asset";
const WORK_ITEM_TYPE: &str = "image_generation_request";
const SPECIFICATION: &str = "community_poster_1024x1024";
const IMAGE_SIZE: &str = "1024x1024";
const PNG_MIME_TYPE: &str = "image/png";
const DEFAULT_MAX_MEDIA_BYTES: usize = 10 * 1024 * 1024;

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
    media_upload_endpoint: Url,
    media_public_base_url: Url,
    media_allowed_hosts: BTreeSet<String>,
    max_media_bytes: usize,
}

#[derive(Debug)]
struct GeneratedImage {
    bytes: Vec<u8>,
    content_hash: String,
    width: u32,
    height: u32,
    artifact_uri: String,
}

#[derive(Debug)]
struct HttpClient {
    tls_config: Arc<ClientConfig>,
    allow_insecure_http: bool,
}

#[derive(Debug)]
struct HttpResponse {
    status: u16,
    headers: BTreeMap<String, String>,
    body: Vec<u8>,
}

#[derive(Debug, Deserialize)]
struct ProviderResponse {
    data: Vec<ProviderImage>,
}

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

    let report = if fixture_mode {
        if apply {
            bail!("fixture-mode cannot be used with --apply");
        }
        fixture_report()
    } else {
        let database_url = cli.database_url_required()?;
        let pool = db::connect(database_url, cli.db_max_connections).await?;
        run_once(&pool, apply && !dry_run, work_item_id).await?
    };

    println!("{}", serde_json::to_string_pretty(&report)?);
    Ok(())
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
    let preview = image_preview(&work_item);
    if !image_generation_enabled() {
        return Ok(report(
            true,
            true,
            false,
            "image_generation_disabled",
            Some(work_item.id),
            Vec::new(),
            Some(preview),
        ));
    }

    let config = match AdapterConfig::from_env() {
        Ok(config) => config,
        Err(_) => {
            return Ok(report(
                true,
                true,
                false,
                "adapter_not_configured",
                Some(work_item.id),
                Vec::new(),
                Some(preview),
            ));
        }
    };

    let Some(work_item) = claim_work_item(pool, work_item_id).await? else {
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
    let work_item_id = work_item.id;
    let worker_input = work_item.clone();
    let generated = tokio::task::spawn_blocking(move || generate_and_store(&config, &worker_input))
        .await
        .context("join image provider task")?;

    let generated = match generated {
        Ok(generated) => generated,
        Err(_) => {
            mark_work_item_failed(pool, work_item_id).await?;
            return Ok(report(
                false,
                true,
                false,
                "image_generation_failed",
                Some(work_item_id),
                Vec::new(),
                None,
            ));
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
            mark_work_item_failed(pool, work_item_id).await?;
            Ok(report(
                false,
                true,
                false,
                "image_generation_failed",
                Some(work_item_id),
                Vec::new(),
                None,
            ))
        }
    }
}

impl AdapterConfig {
    fn from_env() -> Result<Self> {
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
        let max_media_bytes = std::env::var("QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES")
            .ok()
            .filter(|value| !value.trim().is_empty())
            .map(|value| value.parse::<usize>().context("parse media max bytes"))
            .transpose()?
            .unwrap_or(DEFAULT_MAX_MEDIA_BYTES);
        if max_media_bytes == 0 || max_media_bytes > 25 * 1024 * 1024 {
            bail!("media max bytes must be between 1 and 26214400");
        }

        Ok(Self {
            model,
            provider_endpoint,
            api_key: required_env("QINTOPIA_HUABAOSI_IMAGE_API_KEY")?,
            media_upload_endpoint,
            media_public_base_url,
            media_allowed_hosts,
            max_media_bytes,
        })
    }
}

fn required_env(name: &str) -> Result<String> {
    let value = std::env::var(name)
        .ok()
        .map(|value| value.trim().to_string())
        .filter(|value| !value.is_empty() && !is_placeholder(value))
        .ok_or_else(|| anyhow!("required image adapter configuration is missing"))?;
    Ok(value)
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

fn parse_media_upload_response(response: &HttpResponse) -> Result<MediaUploadResponse> {
    ensure_success(response, "media upload")?;
    serde_json::from_slice(&response.body).context("parse media upload response")
}

fn validate_media_response(
    config: &AdapterConfig,
    media: &MediaUploadResponse,
    content_hash: &str,
    metadata: &PngMetadata,
    byte_size: usize,
    allow_insecure_http: bool,
) -> Result<Url> {
    let uri = media_response_url(&media.uri, allow_insecure_http)?;
    if uri.query().is_some() {
        bail!("media response URI must not contain a query");
    }
    let host = normalized_url_host(&uri)?;
    if !config.media_allowed_hosts.contains(&host)
        || !same_public_base(&config.media_public_base_url, &uri)
    {
        bail!("media response URI is outside the configured media boundary");
    }
    if media.content_hash != content_hash
        || media.mime_type != PNG_MIME_TYPE
        || media.byte_size != byte_size
        || media.width != metadata.width
        || media.height != metadata.height
    {
        bail!("media upload metadata did not match generated image");
    }
    Ok(uri)
}

fn media_response_url(value: &str, allow_insecure_http: bool) -> Result<Url> {
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
struct PngMetadata {
    width: u32,
    height: u32,
}

fn inspect_png(bytes: &[u8], max_bytes: usize) -> Result<PngMetadata> {
    if bytes.is_empty() || bytes.len() > max_bytes {
        bail!("image bytes are outside the configured size limit");
    }
    if bytes.len() < 24 || &bytes[..8] != b"\x89PNG\r\n\x1a\n" || &bytes[12..16] != b"IHDR" {
        bail!("generated image must be a PNG with an IHDR header");
    }
    let width = u32::from_be_bytes(bytes[16..20].try_into().expect("PNG width length"));
    let height = u32::from_be_bytes(bytes[20..24].try_into().expect("PNG height length"));
    if width != 1024 || height != 1024 {
        bail!("generated image dimensions must match the requested specification");
    }
    Ok(PngMetadata { width, height })
}

impl GeneratedImage {
    fn preview(&self) -> GeneratedImagePreview {
        GeneratedImagePreview {
            artifact_type: "generated_image",
            review_status: "pending",
            content_hash: self.content_hash.clone(),
            mime_type: PNG_MIME_TYPE,
            width: self.width,
            height: self.height,
            byte_size: self.bytes.len(),
            image_specification: SPECIFICATION.to_string(),
        }
    }
}

impl HttpClient {
    fn production() -> Self {
        Self {
            tls_config: Arc::new(tls_config()),
            allow_insecure_http: false,
        }
    }

    #[cfg(test)]
    fn test_only() -> Self {
        Self {
            tls_config: Arc::new(tls_config()),
            allow_insecure_http: true,
        }
    }

    fn request(
        &self,
        method: &str,
        endpoint: &Url,
        headers: &[(&str, String)],
        body: &[u8],
    ) -> Result<HttpResponse> {
        let host = endpoint
            .host_str()
            .ok_or_else(|| anyhow!("HTTP endpoint is missing a host"))?;
        let port = endpoint
            .port_or_known_default()
            .ok_or_else(|| anyhow!("HTTP endpoint has no known port"))?;
        let mut path = endpoint.path().to_string();
        if path.is_empty() {
            path = "/".to_string();
        }
        if let Some(query) = endpoint.query() {
            path.push('?');
            path.push_str(query);
        }
        let mut request = format!(
            "{method} {path} HTTP/1.1\r\nHost: {host}\r\nContent-Length: {}\r\nConnection: close\r\n",
            body.len()
        );
        for (name, value) in headers {
            request.push_str(name);
            request.push_str(": ");
            request.push_str(value);
            request.push_str("\r\n");
        }
        request.push_str("\r\n");
        let mut request_bytes = request.into_bytes();
        request_bytes.extend_from_slice(body);

        let response = match endpoint.scheme() {
            "https" => {
                let server_name = ServerName::try_from(host).context("validate HTTPS host")?;
                let mut connection =
                    ClientConnection::new(Arc::clone(&self.tls_config), server_name)
                        .context("create image adapter TLS connection")?;
                let mut socket =
                    TcpStream::connect((host, port)).context("connect image adapter endpoint")?;
                configure_socket(&socket)?;
                let mut stream = Stream::new(&mut connection, &mut socket);
                stream
                    .write_all(&request_bytes)
                    .context("write image adapter request")?;
                stream.flush().context("flush image adapter request")?;
                let mut response = Vec::new();
                stream
                    .read_to_end(&mut response)
                    .context("read image adapter response")?;
                response
            }
            "http" if self.allow_insecure_http => {
                let mut socket = TcpStream::connect((host, port))
                    .context("connect test image adapter endpoint")?;
                configure_socket(&socket)?;
                socket
                    .write_all(&request_bytes)
                    .context("write test image adapter request")?;
                socket.flush().context("flush test image adapter request")?;
                let mut response = Vec::new();
                socket
                    .read_to_end(&mut response)
                    .context("read test image adapter response")?;
                response
            }
            _ => bail!("image adapter endpoints must use HTTPS"),
        };
        parse_http_response(&response)
    }
}

fn tls_config() -> ClientConfig {
    let mut roots = RootCertStore::empty();
    roots.add_trust_anchors(webpki_roots::TLS_SERVER_ROOTS.iter().map(|anchor| {
        OwnedTrustAnchor::from_subject_spki_name_constraints(
            anchor.subject,
            anchor.spki,
            anchor.name_constraints,
        )
    }));
    ClientConfig::builder()
        .with_safe_defaults()
        .with_root_certificates(roots)
        .with_no_client_auth()
}

fn configure_socket(socket: &TcpStream) -> Result<()> {
    socket
        .set_read_timeout(Some(Duration::from_secs(60)))
        .context("set image adapter read timeout")?;
    socket
        .set_write_timeout(Some(Duration::from_secs(60)))
        .context("set image adapter write timeout")?;
    Ok(())
}

fn parse_http_response(bytes: &[u8]) -> Result<HttpResponse> {
    let header_end = bytes
        .windows(4)
        .position(|window| window == b"\r\n\r\n")
        .ok_or_else(|| anyhow!("image adapter response is missing headers"))?;
    let head = std::str::from_utf8(&bytes[..header_end]).context("decode image adapter headers")?;
    let status = head
        .lines()
        .next()
        .and_then(|line| line.split_whitespace().nth(1))
        .ok_or_else(|| anyhow!("image adapter response is missing a status"))?
        .parse::<u16>()
        .context("parse image adapter status")?;
    let headers = head
        .lines()
        .skip(1)
        .filter_map(|line| line.split_once(':'))
        .map(|(name, value)| (name.trim().to_ascii_lowercase(), value.trim().to_string()))
        .collect::<BTreeMap<_, _>>();
    let body = bytes[header_end + 4..].to_vec();
    let body = if headers
        .get("transfer-encoding")
        .is_some_and(|value| value.eq_ignore_ascii_case("chunked"))
    {
        decode_chunked_body(&body)?
    } else {
        body
    };
    Ok(HttpResponse {
        status,
        headers,
        body,
    })
}

fn decode_chunked_body(body: &[u8]) -> Result<Vec<u8>> {
    let mut cursor = 0;
    let mut decoded = Vec::new();
    loop {
        let line_end = body
            .get(cursor..)
            .and_then(|remaining| remaining.windows(2).position(|window| window == b"\r\n"))
            .map(|offset| cursor + offset)
            .ok_or_else(|| anyhow!("chunked image response is missing a chunk size"))?;
        let size = std::str::from_utf8(&body[cursor..line_end])
            .context("decode image response chunk size")?
            .split(';')
            .next()
            .unwrap_or_default()
            .trim();
        let size = usize::from_str_radix(size, 16).context("parse image response chunk size")?;
        cursor = line_end + 2;
        if size == 0 {
            return Ok(decoded);
        }
        let end = cursor
            .checked_add(size)
            .ok_or_else(|| anyhow!("chunked image response overflow"))?;
        if end + 2 > body.len() || &body[end..end + 2] != b"\r\n" {
            bail!("chunked image response ended early");
        }
        decoded.extend_from_slice(&body[cursor..end]);
        cursor = end + 2;
    }
}

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
          AND (
              (request.status = 'queued' AND request.available_at <= now())
              OR (request.status = 'processing' AND request.claim_expires_at <= now())
          )
          AND ($4::uuid IS NULL OR request.id = $4)
        ORDER BY request.priority DESC, request.available_at ASC, request.created_at ASC
        LIMIT 1
        "#,
    )
    .bind(CAPABILITY_KEY)
    .bind(WORK_ITEM_TYPE)
    .bind(SPECIFICATION)
    .bind(work_item_id)
    .fetch_optional(pool)
    .await
    .context("load approved Huabaosi image generation request")?;
    row.map(work_item_from_row).transpose()
}

async fn claim_work_item(
    pool: &PgPool,
    work_item_id: Option<Uuid>,
) -> Result<Option<ImageGenerationWorkItem>> {
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
              AND (
                  (request.status = 'queued' AND request.available_at <= now())
                  OR (request.status = 'processing' AND request.claim_expires_at <= now())
              )
              AND ($4::uuid IS NULL OR request.id = $4)
            ORDER BY request.priority DESC, request.available_at ASC, request.created_at ASC
            LIMIT 1
            FOR UPDATE OF request SKIP LOCKED
        )
        UPDATE qintopia_agent_os.work_items request
        SET
            status = 'processing',
            claimed_by = $5,
            locked_at = now(),
            claim_expires_at = now() + interval '10 minutes',
            attempts = attempts + 1,
            updated_at = now()
        FROM claimable
        WHERE request.id = claimable.id
        RETURNING
            request.id,
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
    .bind(work_item_id)
    .bind(WORKER_ID)
    .fetch_optional(pool)
    .await
    .context("claim Huabaosi image generation request")?;
    row.map(work_item_from_row).transpose()
}

fn work_item_from_row(row: sqlx::postgres::PgRow) -> Result<ImageGenerationWorkItem> {
    Ok(ImageGenerationWorkItem {
        id: row.try_get("id")?,
        approved_brief_artifact_id: row.try_get("approved_brief_artifact_id")?,
        approved_brief_content_hash: row.try_get("approved_brief_content_hash")?,
        approved_brief_text: row.try_get("approved_brief_text")?,
        image_specification: row.try_get("image_specification")?,
        prompt_hash: row.try_get("prompt_hash")?,
    })
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

    let row = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.artifacts
            (
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
                'generated_image',
                'pending',
                'huabaosi',
                '活动海报图片（待审核）',
                '由已审核 poster_brief 生成，等待人工审核后才能被下游引用。',
                $2,
                $3,
                $4,
                ARRAY['external_use_review_required','generated_media']::text[],
                'internal_ops',
                $5,
                now()
            )
        ON CONFLICT (work_item_id, content_hash) WHERE content_hash IS NOT NULL AND content_hash <> ''
        DO UPDATE SET
            artifact_uri = EXCLUDED.artifact_uri,
            metadata = qintopia_agent_os.artifacts.metadata || EXCLUDED.metadata,
            updated_at = now()
        RETURNING id
        "#,
    )
    .bind(work_item.id)
    .bind(&generated.artifact_uri)
    .bind(&generated.content_hash)
    .bind(json!([{
        "approved_brief_artifact_id": work_item.approved_brief_artifact_id,
        "approved_brief_content_hash": work_item.approved_brief_content_hash,
    }]))
    .bind(json!({
        "generated_by": WORKER_ID,
        "provider": "openai-compatible",
        "model": "gpt-image-2",
        "mime_type": PNG_MIME_TYPE,
        "width": generated.width,
        "height": generated.height,
        "byte_size": generated.bytes.len(),
        "approved_brief_artifact_id": work_item.approved_brief_artifact_id,
        "approved_brief_content_hash": work_item.approved_brief_content_hash,
        "prompt_hash": work_item.prompt_hash,
    }))
    .fetch_one(&mut *tx)
    .await
    .context("upsert generated image artifact")?;
    let artifact_id: Uuid = row.get("id");

    sqlx::query(
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
        "#,
    )
    .bind(work_item.id)
    .bind(WORKER_ID)
    .execute(&mut *tx)
    .await
    .context("mark image generation request awaiting review")?;
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
    .bind(json!({
        "content_hash": generated.content_hash,
        "mime_type": PNG_MIME_TYPE,
        "width": generated.width,
        "height": generated.height,
        "byte_size": generated.bytes.len(),
        "external_publish_executed": false,
    }))
    .execute(&mut *tx)
    .await
    .context("append generated image event")?;
    tx.commit()
        .await
        .context("commit generated image transaction")?;
    Ok(artifact_id)
}

async fn mark_work_item_failed(pool: &PgPool, work_item_id: Uuid) -> Result<()> {
    let mut tx = pool
        .begin()
        .await
        .context("begin image generation failure transaction")?;
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.work_items
        SET
            status = 'failed',
            locked_at = NULL,
            claim_expires_at = NULL,
            last_error = 'image generation failed; inspect sanitized worker metrics',
            updated_at = now()
        WHERE id = $1
          AND status = 'processing'
          AND claimed_by = $2
        "#,
    )
    .bind(work_item_id)
    .bind(WORKER_ID)
    .execute(&mut *tx)
    .await
    .context("mark image generation request failed")?;
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_item_events
            (work_item_id, event_type, actor_type, actor_id, message, data)
        VALUES ($1, 'failed', 'worker', $2, 'image generation failed before pending artifact creation', $3)
        "#,
    )
    .bind(work_item_id)
    .bind(WORKER_ID)
    .bind(json!({"external_publish_executed": false, "sensitive_fields_redacted": true}))
    .execute(&mut *tx)
    .await
    .context("append image generation failure event")?;
    tx.commit()
        .await
        .context("commit image generation failure transaction")?;
    Ok(())
}

fn image_preview(work_item: &ImageGenerationWorkItem) -> GeneratedImagePreview {
    let content_hash = content_hash_text(&format!(
        "{}|{}|{}|{}",
        work_item.approved_brief_artifact_id,
        work_item.approved_brief_content_hash,
        work_item.image_specification,
        work_item.prompt_hash
    ));
    GeneratedImagePreview {
        artifact_type: "generated_image",
        review_status: "pending",
        content_hash,
        mime_type: PNG_MIME_TYPE,
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
            "provider responses, prompts, credentials, Feishu identifiers, and QiWe payloads are not emitted".to_string(),
        ],
        guardrails: vec![
            "only approved poster_brief artifacts are eligible".to_string(),
            "production endpoints must use HTTPS and media hosts must be allowlisted".to_string(),
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

#[cfg(test)]
fn generate_and_store_with_client(
    config: &AdapterConfig,
    work_item: &ImageGenerationWorkItem,
    client: HttpClient,
) -> Result<GeneratedImage> {
    generate_and_store_with(config, work_item, &client)
}

fn generate_and_store(
    config: &AdapterConfig,
    work_item: &ImageGenerationWorkItem,
) -> Result<GeneratedImage> {
    generate_and_store_with(config, work_item, &HttpClient::production())
}

fn generate_and_store_with(
    config: &AdapterConfig,
    work_item: &ImageGenerationWorkItem,
    client: &HttpClient,
) -> Result<GeneratedImage> {
    let prompt = build_prompt(work_item)?;
    let provider_body = serde_json::to_vec(&json!({
        "model": config.model,
        "prompt": prompt,
        "size": IMAGE_SIZE,
        "response_format": "b64_json",
    }))
    .context("serialize image provider request")?;
    let provider_response = client.request(
        "POST",
        &config.provider_endpoint,
        &[
            ("Authorization", format!("Bearer {}", config.api_key)),
            ("Content-Type", "application/json".to_string()),
            ("Accept", "application/json".to_string()),
        ],
        &provider_body,
    )?;
    let provider_response = parse_provider_response(&provider_response)?;
    let bytes = Base64::decode_vec(&provider_response)
        .map_err(|_| anyhow!("decode image provider b64_json response"))?;
    let metadata = inspect_png(&bytes, config.max_media_bytes)?;
    let content_hash = content_hash_bytes(&bytes);

    let upload_response = client.request(
        "POST",
        &config.media_upload_endpoint,
        &[
            ("Content-Type", PNG_MIME_TYPE.to_string()),
            ("Accept", "application/json".to_string()),
            ("X-Qintopia-Content-Hash", content_hash.clone()),
            ("X-Qintopia-Byte-Size", bytes.len().to_string()),
            ("X-Qintopia-Width", metadata.width.to_string()),
            ("X-Qintopia-Height", metadata.height.to_string()),
            ("X-Qintopia-Work-Item-Id", work_item.id.to_string()),
            ("X-Qintopia-Idempotency-Key", work_item.prompt_hash.clone()),
        ],
        &bytes,
    )?;
    let media = parse_media_upload_response(&upload_response)?;
    let media_url = validate_media_response(
        config,
        &media,
        &content_hash,
        &metadata,
        bytes.len(),
        client.allow_insecure_http,
    )?;

    let readback = client.request("GET", &media_url, &[], &[])?;
    ensure_success(&readback, "media readback")?;
    let content_type = readback
        .headers
        .get("content-type")
        .map(|value| value.split(';').next().unwrap_or_default().trim())
        .unwrap_or_default();
    if content_type != PNG_MIME_TYPE {
        bail!("media readback returned an unexpected MIME type");
    }
    let readback_metadata = inspect_png(&readback.body, config.max_media_bytes)?;
    if readback.body != bytes
        || content_hash_bytes(&readback.body) != content_hash
        || readback_metadata.width != metadata.width
        || readback_metadata.height != metadata.height
    {
        bail!("media readback did not match uploaded image");
    }
    Ok(GeneratedImage {
        bytes,
        content_hash,
        width: metadata.width,
        height: metadata.height,
        artifact_uri: media.uri,
    })
}

#[cfg(test)]
mod tests {
    use std::{
        io::{Read, Write},
        net::TcpListener,
        thread,
    };

    use super::*;

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
    fn production_endpoints_reject_http() {
        assert!(https_url("http://127.0.0.1:8080/images", "provider").is_err());
    }

    #[test]
    fn production_endpoints_reject_query_parameters() {
        assert!(https_url("https://media.example.test/upload?token=secret", "media").is_err());
    }

    #[test]
    fn media_response_must_stay_within_public_base_and_allowlist() {
        let config = test_config("https://media.example.test/public");
        let metadata = PngMetadata {
            width: 1024,
            height: 1024,
        };
        let media = MediaUploadResponse {
            uri: "https://other.example.test/public/image.png".to_string(),
            content_hash: "sha256:abc".to_string(),
            mime_type: PNG_MIME_TYPE.to_string(),
            byte_size: 12,
            width: 1024,
            height: 1024,
        };
        assert!(
            validate_media_response(&config, &media, "sha256:abc", &metadata, 12, false).is_err()
        );
    }

    #[test]
    fn media_response_cannot_escape_public_path_prefix() {
        let config = test_config("https://media.example.test/public");
        let metadata = PngMetadata {
            width: 1024,
            height: 1024,
        };
        let media = MediaUploadResponse {
            uri: "https://media.example.test/publicity/image.png".to_string(),
            content_hash: "sha256:abc".to_string(),
            mime_type: PNG_MIME_TYPE.to_string(),
            byte_size: 12,
            width: 1024,
            height: 1024,
        };
        assert!(
            validate_media_response(&config, &media, "sha256:abc", &metadata, 12, false).is_err()
        );
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
        let metadata = PngMetadata {
            width: 1024,
            height: 1024,
        };
        let media = MediaUploadResponse {
            uri: "https://media.example.test/public/image.png".to_string(),
            content_hash: "sha256:unexpected".to_string(),
            mime_type: PNG_MIME_TYPE.to_string(),
            byte_size: 12,
            width: 1024,
            height: 1024,
        };
        let error =
            validate_media_response(&config, &media, "sha256:expected", &metadata, 12, false)
                .expect_err("upload metadata must match generated image");

        assert!(error.to_string().contains("metadata did not match"));
    }

    #[test]
    fn fake_provider_and_media_server_round_trip_png_bytes() {
        let image = fixture_png();
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
        let port = listener.local_addr().expect("fake server address").port();
        let image_for_server = image.clone();
        let handle = thread::spawn(move || {
            for expected_path in ["/v1/images/generations", "/upload", "/public/image.png"] {
                let (mut stream, _) = listener.accept().expect("accept fake request");
                let request = read_request(&mut stream);
                assert!(request.headers.starts_with(&format!(
                    "{} ",
                    if expected_path == "/public/image.png" {
                        "GET"
                    } else {
                        "POST"
                    }
                )));
                assert!(request.headers.contains(expected_path));
                let response = match expected_path {
                    "/v1/images/generations" => {
                        let encoded = Base64::encode_string(&image_for_server);
                        json_response(&json!({"data":[{"b64_json": encoded}]}).to_string())
                    }
                    "/upload" => {
                        assert!(request
                            .headers
                            .to_ascii_lowercase()
                            .contains("x-qintopia-content-hash: sha256:"));
                        assert_eq!(request.body, image_for_server);
                        let uri = format!("http://127.0.0.1:{port}/public/image.png");
                        json_response(
                            &json!({
                                "uri": uri,
                                "content_hash": content_hash_bytes(&image_for_server),
                                "mime_type": PNG_MIME_TYPE,
                                "byte_size": image_for_server.len(),
                                "width": 1024,
                                "height": 1024,
                            })
                            .to_string(),
                        )
                    }
                    _ => binary_response(&image_for_server),
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
            media_upload_endpoint: Url::parse(&format!("http://127.0.0.1:{port}/upload"))
                .expect("fake upload URL"),
            media_public_base_url: Url::parse(&format!("http://127.0.0.1:{port}/public"))
                .expect("fake public URL"),
            media_allowed_hosts: BTreeSet::from(["127.0.0.1".to_string()]),
            max_media_bytes: DEFAULT_MAX_MEDIA_BYTES,
        };
        let generated =
            generate_and_store_with_client(&config, &fixture_work_item(), HttpClient::test_only())
                .expect("fake image generation succeeds");
        handle.join().expect("fake server joins");

        assert_eq!(generated.bytes, image);
        assert_eq!(generated.width, 1024);
        assert_eq!(generated.height, 1024);
        assert!(generated.artifact_uri.contains("/public/image.png"));
    }

    #[test]
    fn fake_media_readback_must_match_uploaded_bytes() {
        let image = fixture_png();
        let mut different_image = image.clone();
        different_image[24] ^= 1;
        let listener = TcpListener::bind("127.0.0.1:0").expect("bind fake server");
        let port = listener.local_addr().expect("fake server address").port();
        let image_for_server = image.clone();
        let handle = thread::spawn(move || {
            for expected_path in ["/v1/images/generations", "/upload", "/public/image.png"] {
                let (mut stream, _) = listener.accept().expect("accept fake request");
                let request = read_request(&mut stream);
                let response = match expected_path {
                    "/v1/images/generations" => json_response(
                        &json!({"data":[{"b64_json": Base64::encode_string(&image_for_server)}]})
                            .to_string(),
                    ),
                    "/upload" => json_response(
                        &json!({
                            "uri": format!("http://127.0.0.1:{port}/public/image.png"),
                            "content_hash": content_hash_bytes(&image_for_server),
                            "mime_type": PNG_MIME_TYPE,
                            "byte_size": image_for_server.len(),
                            "width": 1024,
                            "height": 1024,
                        })
                        .to_string(),
                    ),
                    _ => binary_response(&different_image),
                };
                if expected_path == "/upload" {
                    assert_eq!(request.body, image_for_server);
                }
                stream.write_all(&response).expect("write fake response");
            }
        });

        let config = test_http_config(port);
        let error =
            generate_and_store_with_client(&config, &fixture_work_item(), HttpClient::test_only())
                .expect_err("readback with different bytes must fail");
        handle.join().expect("fake server joins");

        assert!(error.to_string().contains("did not match uploaded image"));
    }

    fn fixture_work_item() -> ImageGenerationWorkItem {
        ImageGenerationWorkItem {
            id: Uuid::new_v4(),
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
            media_upload_endpoint: Url::parse("https://media.example.test/upload").unwrap(),
            media_public_base_url: Url::parse(public_base).unwrap(),
            media_allowed_hosts: BTreeSet::from(["media.example.test".to_string()]),
            max_media_bytes: DEFAULT_MAX_MEDIA_BYTES,
        }
    }

    fn test_http_config(port: u16) -> AdapterConfig {
        AdapterConfig {
            model: "gpt-image-2".to_string(),
            provider_endpoint: Url::parse(&format!(
                "http://127.0.0.1:{port}/v1/images/generations"
            ))
            .expect("fake provider endpoint"),
            api_key: "test-key".to_string(),
            media_upload_endpoint: Url::parse(&format!("http://127.0.0.1:{port}/upload"))
                .expect("fake upload endpoint"),
            media_public_base_url: Url::parse(&format!("http://127.0.0.1:{port}/public"))
                .expect("fake public base"),
            media_allowed_hosts: BTreeSet::from(["127.0.0.1".to_string()]),
            max_media_bytes: DEFAULT_MAX_MEDIA_BYTES,
        }
    }

    fn fixture_png() -> Vec<u8> {
        let mut bytes = b"\x89PNG\r\n\x1a\n\x00\x00\x00\rIHDR".to_vec();
        bytes.extend_from_slice(&1024_u32.to_be_bytes());
        bytes.extend_from_slice(&1024_u32.to_be_bytes());
        bytes.extend_from_slice(&[8, 6, 0, 0, 0]);
        bytes
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

    fn binary_response(body: &[u8]) -> Vec<u8> {
        let mut response = format!(
            "HTTP/1.1 200 OK\r\nContent-Type: image/png\r\nContent-Length: {}\r\nConnection: close\r\n\r\n",
            body.len()
        )
        .into_bytes();
        response.extend_from_slice(body);
        response
    }
}
