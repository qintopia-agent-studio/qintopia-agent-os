//! Shared "HTTP public media storage" upload flow.
//!
//! Both image workflows (huabaosi AI image generation and the Xiaoman daily
//! case report) post the final JPEG to a reviewed public media endpoint and
//! validate the JSON metadata the storage service returns. The POST, the
//! success-status check, and the metadata cross-check against the uploaded
//! image identity are identical; only the caller-specific details differ:
//!
//! - extra request headers (e.g. the daily case report's `X-Qintopia-Workflow`),
//! - how the returned URI is validated against the configured media boundary
//!   (huabaosi allows insecure loopback URLs only under its test adapter),
//! - the error message wording each workflow reports,
//! - any caller-specific follow-up (huabaosi additionally GETs the media back
//!   and compares bytes; the daily case report deliberately does not).
//!
//! Those differences stay with the callers. This module owns only the shared
//! POST + response-metadata validation.

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use url::Url;

use crate::bounded_http::{HttpClient, HttpRequestError, HttpResponse};

/// Response cap for the media upload JSON metadata response. Both workflows
/// used the same 64 KiB bound before this module existed.
pub(crate) const MEDIA_UPLOAD_RESPONSE_LIMIT_BYTES: usize = 64 * 1024;

/// Metadata returned by the public media storage service for one upload.
#[derive(Debug, Clone, Deserialize)]
pub(crate) struct MediaUploadMetadata {
    pub(crate) uri: String,
    pub(crate) content_hash: String,
    pub(crate) mime_type: String,
    pub(crate) byte_size: usize,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// Identity of the uploaded image that the storage service must echo back.
pub(crate) struct MediaUploadExpectation<'a> {
    pub(crate) content_hash: &'a str,
    pub(crate) mime_type: &'a str,
    pub(crate) byte_size: usize,
    pub(crate) width: u32,
    pub(crate) height: u32,
}

/// POST `bytes` to the public media upload endpoint and validate the returned
/// metadata against `expectation`. `extra_headers` carries caller-specific
/// request headers on top of the shared `Content-Type`/`Accept` pair.
///
/// This function deliberately stops after metadata validation: it does not
/// read the media back over HTTP. Callers that require a byte-level readback
/// perform it as their own follow-up step.
pub(crate) fn upload_public_media(
    client: &HttpClient,
    endpoint: &Url,
    bytes: &[u8],
    expectation: &MediaUploadExpectation<'_>,
    extra_headers: &[(&str, String)],
    non_success_message: &str,
    metadata_mismatch_message: &str,
) -> std::result::Result<MediaUploadMetadata, HttpRequestError> {
    let mut headers = Vec::with_capacity(extra_headers.len() + 2);
    headers.push(("Content-Type", expectation.mime_type.to_string()));
    headers.push(("Accept", "application/json".to_string()));
    headers.extend(
        extra_headers
            .iter()
            .map(|(name, value)| (*name, value.clone())),
    );
    let response = client.request(
        "POST",
        endpoint,
        &headers,
        bytes,
        MEDIA_UPLOAD_RESPONSE_LIMIT_BYTES,
    )?;
    validate_uploaded_media_metadata(
        &response,
        expectation,
        non_success_message,
        metadata_mismatch_message,
    )
    .map_err(HttpRequestError::after_validation)
}

/// Validate a media upload HTTP response: success status, JSON metadata body,
/// and metadata echoing the uploaded image identity.
pub(crate) fn validate_uploaded_media_metadata(
    response: &HttpResponse,
    expectation: &MediaUploadExpectation<'_>,
    non_success_message: &str,
    metadata_mismatch_message: &str,
) -> Result<MediaUploadMetadata> {
    if !(200..300).contains(&response.status) {
        bail!("{non_success_message}");
    }
    let media: MediaUploadMetadata =
        serde_json::from_slice(&response.body).context("parse media upload response")?;
    validate_media_upload_metadata(&media, expectation, metadata_mismatch_message)?;
    Ok(media)
}

fn validate_media_upload_metadata(
    media: &MediaUploadMetadata,
    expectation: &MediaUploadExpectation<'_>,
    mismatch_message: &str,
) -> Result<()> {
    if media.content_hash != expectation.content_hash
        || media.mime_type != expectation.mime_type
        || media.byte_size != expectation.byte_size
        || media.width != expectation.width
        || media.height != expectation.height
    {
        bail!("{mismatch_message}");
    }
    Ok(())
}
