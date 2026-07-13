use anyhow::{bail, Context, Result};
use serde::Serialize;
use sha2::{Digest, Sha256};
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::{config::Cli, db};

const WORKER_ID: &str = "huabaosi-image-generation-worker";
const CAPABILITY_KEY: &str = "huabaosi.generate_image_asset";
const WORK_ITEM_TYPE: &str = "image_generation_request";
const SPECIFICATION: &str = "community_poster_1024x1024";

#[derive(Debug, Serialize)]
pub struct ImageGenerationWorkerReport {
    pub success: bool,
    pub dry_run: bool,
    pub apply_requested: bool,
    pub fixture_mode: bool,
    pub worker: &'static str,
    pub action_status: String,
    pub work_item_id: Option<Uuid>,
    pub artifact_preview: Option<GeneratedImagePreview>,
    pub safe_for_chat: bool,
    pub limitations: Vec<String>,
    pub guardrails: Vec<String>,
}

#[derive(Debug, Serialize)]
pub struct GeneratedImagePreview {
    pub artifact_type: &'static str,
    pub review_status: &'static str,
    pub content_hash: String,
    pub mime_type: &'static str,
    pub width: u32,
    pub height: u32,
    pub image_specification: String,
}

#[derive(Debug)]
struct ImageGenerationWorkItem {
    id: Uuid,
    approved_brief_artifact_id: Uuid,
    approved_brief_content_hash: String,
    image_specification: String,
    prompt_hash: String,
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
    let work_item = ImageGenerationWorkItem {
        id: Uuid::nil(),
        approved_brief_artifact_id: Uuid::nil(),
        approved_brief_content_hash: "sha256:fixture-approved-brief".to_string(),
        image_specification: SPECIFICATION.to_string(),
        prompt_hash: "sha256:fixture-prompt".to_string(),
    };
    report(
        false,
        true,
        "fixture_image_generation_preview",
        Some(work_item.id),
        Some(image_preview(&work_item)),
    )
}

async fn run_once(
    pool: &PgPool,
    apply_requested: bool,
    work_item_id: Option<Uuid>,
) -> Result<ImageGenerationWorkerReport> {
    let Some(work_item) = load_work_item(pool, work_item_id).await? else {
        return Ok(report(
            apply_requested,
            false,
            "no_claimable_image_request",
            None,
            None,
        ));
    };
    let preview = image_preview(&work_item);
    if !apply_requested {
        return Ok(report(
            false,
            false,
            "image_generation_preview",
            Some(work_item.id),
            Some(preview),
        ));
    }

    if !image_generation_enabled() {
        return Ok(report(
            true,
            false,
            "image_generation_disabled",
            Some(work_item.id),
            Some(preview),
        ));
    }

    Ok(report(
        true,
        false,
        "adapter_not_configured",
        Some(work_item.id),
        Some(preview),
    ))
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

    row.map(|row| {
        Ok(ImageGenerationWorkItem {
            id: row.try_get("id")?,
            approved_brief_artifact_id: row.try_get("approved_brief_artifact_id")?,
            approved_brief_content_hash: row.try_get("approved_brief_content_hash")?,
            image_specification: row.try_get("image_specification")?,
            prompt_hash: row.try_get("prompt_hash")?,
        })
    })
    .transpose()
}

fn image_preview(work_item: &ImageGenerationWorkItem) -> GeneratedImagePreview {
    let content_hash = content_hash(&format!(
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
        mime_type: "image/png",
        width: 1024,
        height: 1024,
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
    apply_requested: bool,
    fixture_mode: bool,
    action_status: &str,
    work_item_id: Option<Uuid>,
    artifact_preview: Option<GeneratedImagePreview>,
) -> ImageGenerationWorkerReport {
    ImageGenerationWorkerReport {
        success: true,
        dry_run: !apply_requested,
        apply_requested,
        fixture_mode,
        worker: WORKER_ID,
        action_status: action_status.to_string(),
        work_item_id,
        artifact_preview,
        safe_for_chat: false,
        limitations: vec![
            "this worker currently validates and previews approved image-generation requests".to_string(),
            "a reviewed provider and isolated media storage adapter are required before an image can be generated".to_string(),
            "no image provider, media upload, Feishu write, QiWe send, or external publication is called by the current implementation".to_string(),
        ],
        guardrails: vec![
            "only approved poster_brief artifacts are eligible".to_string(),
            "generated_image artifacts must remain pending human review".to_string(),
            "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED defaults to disabled".to_string(),
        ],
    }
}

fn content_hash(value: &str) -> String {
    format!("sha256:{:x}", Sha256::digest(value.as_bytes()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fixture_preview_is_pending_and_safe_for_chat_is_false() {
        let report = fixture_report();
        let raw = serde_json::to_string(&report).expect("report serializes");

        assert_eq!(report.action_status, "fixture_image_generation_preview");
        assert!(report.dry_run);
        assert!(!report.safe_for_chat);
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
        let work_item = ImageGenerationWorkItem {
            id: Uuid::new_v4(),
            approved_brief_artifact_id: Uuid::new_v4(),
            approved_brief_content_hash: "sha256:approved-brief".to_string(),
            image_specification: SPECIFICATION.to_string(),
            prompt_hash: "sha256:prompt".to_string(),
        };

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
}
