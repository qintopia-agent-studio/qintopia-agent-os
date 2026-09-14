use super::{
    digest,
    state::evaluate_tx,
    store::{audit, create_work, Store},
};
use anyhow::{ensure, Result};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;

pub struct ArtifactInput<'a> {
    pub case: Uuid,
    pub expected_version: i64,
    pub application_revision: i64,
    pub template: &'a str,
    pub kind: &'a str,
    pub bytes: &'a [u8],
}
impl Store {
    /// Persist the generation request before any renderer can run. Only approved
    /// adapters may later satisfy it; missing renderer/source fields stay visible.
    pub async fn request_card(&self, case: Uuid) -> Result<Uuid> {
        let mut tx = self.pool.begin().await?;
        ensure!(
            evaluate_tx(&mut tx, case).await?.card_ready,
            "card_not_eligible"
        );
        let row=sqlx::query("SELECT c.version,c.application_id,a.revision,a.consent_version,s.mode FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_applications a ON a.id=c.application_id JOIN qintopia_agent_os.welcome_sources s ON s.source_instance=c.source_instance AND s.property_id=c.property_id WHERE c.id=$1 FOR UPDATE OF c,a")
            .bind(case).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<String, _>("mode") == "synthetic",
            "card_request_mode_disabled"
        );
        let version: i64 = row.get("version");
        let revision: i64 = row.get("revision");
        let id = create_work(
            &mut tx,
            &format!("welcome-card-request/{case}/{version}/{revision}"),
            "welcome_card",
            "huabaosi",
            "awaiting_review",
        )
        .await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items SET payload=$2 WHERE id=$1 AND payload='{}'::jsonb")
            .bind(id).bind(json!({"case_ref":case,"case_version":version,"application_ref":row.get::<Uuid,_>("application_id"),"application_revision":revision,"consent_version":row.get::<i64,_>("consent_version"),"boundary":"renderer_and_field_adapter_required","external_execution":false})).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(id)
    }
    /// Register already rendered, bounded output. It neither invokes the legacy
    /// renderer nor grants approval. Live generation is deliberately unavailable.
    pub async fn register_artifact(&self, input: &ArtifactInput<'_>) -> Result<Uuid> {
        ensure!(
            matches!(input.kind, "welcome_card" | "welcome_text")
                && !input.bytes.is_empty()
                && input.bytes.len() <= 10 * 1024 * 1024,
            "invalid_artifact"
        );
        ensure!(
            !input.template.is_empty() && input.template.len() <= 80,
            "invalid_template"
        );
        if input.kind == "welcome_card" {
            let mut reader =
                image::ImageReader::new(std::io::Cursor::new(input.bytes)).with_guessed_format()?;
            let mut limits = image::Limits::default();
            limits.max_image_width = Some(4096);
            limits.max_image_height = Some(4096);
            limits.max_alloc = Some(64 * 1024 * 1024);
            reader.limits(limits);
            reader
                .decode()
                .map_err(|_| anyhow::anyhow!("invalid_bounded_image"))?;
        } else {
            ensure!(
                input.bytes.len() <= 8192 && std::str::from_utf8(input.bytes).is_ok(),
                "invalid_text"
            )
        }
        let mut tx = self.pool.begin().await?;
        ensure!(
            evaluate_tx(&mut tx, input.case).await?.card_ready,
            "card_not_eligible"
        );
        let row=sqlx::query("SELECT c.version,c.application_id,a.revision,a.consent_version,s.mode FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_applications a ON a.id=c.application_id JOIN qintopia_agent_os.welcome_sources s ON s.source_instance=c.source_instance AND s.property_id=c.property_id WHERE c.id=$1 FOR UPDATE OF c,a")
            .bind(input.case).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<String, _>("mode") == "synthetic",
            "artifact_generation_disabled"
        );
        ensure!(
            row.get::<i64, _>("version") == input.expected_version
                && row.get::<i64, _>("revision") == input.application_revision,
            "artifact_input_stale"
        );
        let hash = digest(input.bytes);
        let work = create_work(
            &mut tx,
            &format!(
                "welcome-card/{}/{}/{}/{}/{}",
                input.case, input.expected_version, input.application_revision, input.kind, hash
            ),
            "welcome_card",
            "huabaosi",
            "awaiting_review",
        )
        .await?;
        let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.artifacts(work_item_id,artifact_type,created_by_agent,content_hash,review_status) VALUES ($1,$2,'huabaosi',$3,'pending') ON CONFLICT (work_item_id,content_hash) WHERE content_hash IS NOT NULL AND content_hash<>'' DO UPDATE SET content_hash=EXCLUDED.content_hash RETURNING id")
            .bind(work).bind(input.kind).bind(&hash).fetch_one(&mut *tx).await?;
        if input.kind == "welcome_text" {
            sqlx::query("UPDATE qintopia_agent_os.artifacts SET content_text=$2 WHERE id=$1")
                .bind(id)
                .bind(std::str::from_utf8(input.bytes)?)
                .execute(&mut *tx)
                .await?;
        }
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_artifact_bindings(artifact_id,case_id,application_id,application_revision,case_version,consent_version,template_version) VALUES ($1,$2,$3,$4,$5,$6,$7) ON CONFLICT (artifact_id) DO NOTHING")
            .bind(id).bind(input.case).bind(row.get::<Uuid,_>("application_id")).bind(input.application_revision).bind(input.expected_version).bind(row.get::<i64,_>("consent_version")).bind(input.template).execute(&mut *tx).await?;
        audit(
            &mut tx,
            Some(work),
            "welcome_artifact_registered",
            None,
            json!({"artifact_ref":id}),
        )
        .await?;
        tx.commit().await?;
        Ok(id)
    }

    pub async fn upload_intent(&self, artifact: Uuid) -> Result<Uuid> {
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT a.content_hash,b.case_id FROM qintopia_agent_os.artifacts a JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=a.id WHERE a.id=$1 AND a.artifact_type='welcome_card' AND b.revoked_at IS NULL FOR UPDATE OF a,b")
            .bind(artifact).fetch_one(&mut *tx).await?;
        ensure!(
            evaluate_tx(&mut tx, row.get("case_id")).await?.card_ready,
            "upload_not_eligible"
        );
        let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_upload_intents(artifact_id,content_hash) VALUES ($1,$2) ON CONFLICT (artifact_id) DO UPDATE SET artifact_id=EXCLUDED.artifact_id RETURNING id")
            .bind(artifact).bind(row.get::<String,_>("content_hash")).fetch_one(&mut *tx).await?;
        tx.commit().await?;
        Ok(id)
    }

    /// Readback must come from the fixed application resource adapter. Unknown
    /// upload is reconciled by intent/hash, never by uploading another attachment.
    pub async fn register_upload_readback(
        &self,
        intent: Uuid,
        application: Uuid,
        content_hash: &str,
        storage_ref: &str,
    ) -> Result<()> {
        ensure!(
            storage_ref.starts_with("feishu-base://resident-welcome/")
                && !storage_ref.contains(['?', '#', '\r', '\n']),
            "permanent_application_storage_required"
        );
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT i.content_hash,i.status,i.storage_ref,b.application_id,i.artifact_id FROM qintopia_agent_os.welcome_upload_intents i JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=i.artifact_id WHERE i.id=$1 FOR UPDATE OF i")
            .bind(intent).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<Uuid, _>("application_id") == application
                && row.get::<String, _>("content_hash") == content_hash,
            "upload_readback_mismatch"
        );
        if row.get::<String, _>("status") == "registered" {
            ensure!(
                row.get::<Option<String>, _>("storage_ref").as_deref() == Some(storage_ref),
                "storage_conflict"
            );
            return Ok(());
        }
        sqlx::query("UPDATE qintopia_agent_os.welcome_upload_intents SET status='registered',storage_ref=$2,updated_at=now() WHERE id=$1").bind(intent).bind(storage_ref).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.artifacts SET artifact_uri=$2 WHERE id=$1")
            .bind(row.get::<Uuid, _>("artifact_id"))
            .bind(storage_ref)
            .execute(&mut *tx)
            .await?;
        audit(
            &mut tx,
            None,
            "welcome_upload_reconciled",
            None,
            json!({"intent_ref":intent}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }
}

/// Pure append plan for application Base 欢迎卡片; preserves every unrelated
/// attachment. Provider IDs are transient adapter material, never audit data.
pub fn append_attachment(existing: &[String], uploaded: &str) -> Vec<String> {
    let mut result = existing.to_vec();
    if !result.iter().any(|s| s == uploaded) {
        result.push(uploaded.into());
    }
    result
}
