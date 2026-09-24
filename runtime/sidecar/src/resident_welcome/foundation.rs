//! Local welcome consumers of the shared identity, authority and rule services.
//! Runtime identities are resolved from persisted tasks, never from model JSON.
use super::{
    artifact::ArtifactInput,
    delivery::Outcome,
    digest,
    review::ReviewRequest,
    state::{evaluate_tx, Snapshot, StayState},
    store::{create_work, Store},
};
use crate::person_collaboration::{
    authorize_current, can_inspect_current, content_reviewer, delegated_scopes_in,
    effective_knowledge_in, put_knowledge_in, Authority, KnowledgeVersion, KnowledgeWrite,
};
use anyhow::{ensure, Result};
use chrono::{Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct WelcomeSetting {
    pub mode: String,
    /// Parts covered by this standing review/direct arrangement.
    /// Other parts still require content review; this is not an output filter.
    pub parts: Vec<String>,
    pub phase: String,
    pub text_template: String,
}
impl WelcomeSetting {
    fn validate(&self) -> Result<()> {
        ensure!(
            matches!(self.mode.as_str(), "direct" | "review")
                && matches!(self.phase.as_str(), "formal" | "preview")
                && !self.parts.is_empty()
                && self.parts.len() <= 2
                && self
                    .parts
                    .iter()
                    .all(|s| matches!(s.as_str(), "text" | "image"))
                && (self.parts.len() != 2 || self.parts[0] != self.parts[1])
                && !self.text_template.is_empty()
                && self.text_template.len() <= 2048,
            "invalid_welcome_setting"
        );
        Ok(())
    }
}
#[derive(Clone, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct CardMaterial {
    pub display_name: String,
    pub description: String,
    pub welcome_text: String,
}
#[derive(Clone, Copy)]
pub enum SyntheticOutcome {
    Success,
    DefiniteNoSend,
    UnknownAfterSend,
    PermanentFailure,
    Unavailable,
}

struct AgentTaskContext {
    work: Uuid,
    agent: String,
}
impl AgentTaskContext {
    async fn from_task(
        tx: &mut Transaction<'_, Postgres>,
        work: Uuid,
        expected: &str,
    ) -> Result<Self> {
        let row = sqlx::query("SELECT target_agent,capability_key,source_type FROM qintopia_agent_os.work_items WHERE id=$1 FOR UPDATE")
            .bind(work).fetch_one(&mut **tx).await?;
        ensure!(
            row.get::<String, _>("target_agent") == expected
                && row.get::<String, _>("capability_key") == "resident_welcome.coordinate"
                && row.get::<String, _>("source_type") == "resident_welcome",
            "trusted_welcome_task_required"
        );
        ensure!(
            include_str!("../../../../fixtures/agents/welcome.yaml")
                .lines()
                .any(|line| line.trim() == format!("- id: agents/{expected}")),
            "registered_local_welcome_agent_required"
        );
        Ok(Self {
            work,
            agent: expected.into(),
        })
    }
    async fn invoke(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        operation: &str,
        input: Value,
    ) -> Result<Value> {
        Self::from_task(tx, self.work, &self.agent).await?;
        let result =
            super::runtime_adapter::invoke(self.work, &self.agent, operation, input).await?;
        let mut invoked = result.evidence.clone();
        let fields = invoked.as_object_mut().unwrap();
        fields.remove("output_binding");
        fields.remove("output_sha256");
        for (kind, evidence) in [
            ("welcome_runtime_invoked", &invoked),
            ("welcome_runtime_result", &result.evidence),
        ] {
            sqlx::query("INSERT INTO qintopia_agent_os.work_item_events(work_item_id,event_type,actor_type,actor_id,data) VALUES ($1,$2,'agent',$3,$4)")
                .bind(self.work).bind(kind).bind(&self.agent).bind(evidence).execute(&mut **tx).await?;
        }
        Ok(result.output)
    }
    async fn record(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        operation: &str,
        result: Value,
    ) -> Result<()> {
        sqlx::query("INSERT INTO qintopia_agent_os.work_item_events(work_item_id,event_type,actor_type,actor_id,data) VALUES ($1,'welcome_agent_result','agent',$2,$3)")
            .bind(self.work).bind(&self.agent).bind(json!({"operation":operation,"result":result,"synthetic":true})).execute(&mut **tx).await?;
        Ok(())
    }
}

async fn target(tx: &mut Transaction<'_, Postgres>, target: Uuid) -> Result<(String, Uuid)> {
    let row=sqlx::query("SELECT f.tenant_key,f.scope_id FROM qintopia_agent_os.welcome_foundation_targets f JOIN qintopia_agent_os.welcome_targets t ON t.id=f.target_id JOIN qintopia_agent_os.collaboration_scopes s ON s.id=f.scope_id AND s.tenant_key=f.tenant_key JOIN qintopia_agent_os.welcome_sources w ON w.source_instance=t.source_instance AND w.property_id=t.property_id WHERE f.target_id=$1 AND t.enabled AND w.mode='synthetic'")
        .bind(target).fetch_one(&mut **tx).await?;
    let tenant: String = row.get("tenant_key");
    sqlx::query(
        "SELECT 1 FROM qintopia_agent_os.collaboration_tenants WHERE tenant_key=$1 FOR UPDATE",
    )
    .bind(&tenant)
    .fetch_one(&mut **tx)
    .await?;
    Ok((tenant, row.get("scope_id")))
}

impl Store {
    /// `person` is supplied only by the authenticated HTTP/runtime caller.
    pub async fn foundation_configure(
        &self,
        tenant: &str,
        person: Uuid,
        target_ref: Uuid,
        write: &KnowledgeWrite,
    ) -> Result<Value> {
        let setting: WelcomeSetting = serde_json::from_value(write.content.clone())?;
        setting.validate()?;
        let mut tx = self.pool.begin().await?;
        let (bound_tenant, scope) = target(&mut tx, target_ref).await?;
        ensure!(
            tenant == bound_tenant
                && write.scope == scope
                && write.key == "resident_welcome"
                && write.kind == "rule"
                && !write.shared,
            "welcome_rule_scope_mismatch"
        );
        if let Some(case) = write.case_ref {
            check_case_target(&mut tx, case, target_ref).await?;
            let matches:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_targets t ON t.source_instance=c.source_instance AND t.property_id=c.property_id WHERE c.id=$1 AND t.id=$2)")
                .bind(case).bind(target_ref).fetch_one(&mut *tx).await?;
            ensure!(matches, "welcome_case_scope_mismatch");
        }
        let saved = put_knowledge_in(&mut tx, tenant, person, write).await?;
        tx.commit().await?;
        Ok(json!({"rule_ref":saved.id,"version":saved.version,"status":"saved"}))
    }

    /// Explicit local field adapter: no images/URLs or applicant records reach the renderer.
    pub async fn foundation_render(&self, case: Uuid, material: &CardMaterial) -> Result<Value> {
        self.foundation_render_bound(case, material, None).await
    }

    pub async fn foundation_render_exact(
        &self,
        case: Uuid,
        material: &CardMaterial,
        expected_case_version: i64,
        expected_application_revision: i64,
    ) -> Result<Value> {
        self.foundation_render_bound(
            case,
            material,
            Some((expected_case_version, expected_application_revision)),
        )
        .await
    }

    async fn foundation_render_bound(
        &self,
        case: Uuid,
        material: &CardMaterial,
        expected: Option<(i64, i64)>,
    ) -> Result<Value> {
        ensure!(
            !material.display_name.is_empty()
                && material.display_name.chars().count() <= 40
                && material.description.chars().count() <= 320
                && !material.welcome_text.is_empty()
                && material.welcome_text.len() <= 8192,
            "invalid_card_material"
        );
        let mut tx = self.pool.begin().await?;
        ensure!(
            evaluate_tx(&mut tx, case).await?.card_ready,
            "card_not_eligible"
        );
        let row=sqlx::query("SELECT c.version,a.revision,s.mode FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_applications a ON a.id=c.application_id JOIN qintopia_agent_os.welcome_sources s ON s.source_instance=c.source_instance AND s.property_id=c.property_id WHERE c.id=$1")
            .bind(case).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<String, _>("mode") == "synthetic",
            "synthetic_render_required"
        );
        let version: i64 = row.get("version");
        let revision: i64 = row.get("revision");
        ensure!(
            expected.is_none_or(|e| e == (version, revision)),
            "welcome_version_conflict"
        );
        let coordinator = create_work(
            &mut tx,
            &format!("welcome-coordinate/{case}/{version}"),
            "welcome_event",
            "anan",
            "processing",
        )
        .await?;
        let tenant:String=sqlx::query_scalar("SELECT f.tenant_key FROM qintopia_agent_os.welcome_foundation_targets f JOIN qintopia_agent_os.welcome_targets t ON t.id=f.target_id JOIN qintopia_agent_os.welcome_cases c ON c.source_instance=t.source_instance AND c.property_id=t.property_id WHERE c.id=$1 LIMIT 1").bind(case).fetch_one(&mut *tx).await?;
        require_executor(&mut tx, &tenant, "anan").await?;
        let coordinator_context = AgentTaskContext::from_task(&mut tx, coordinator, "anan").await?;
        tx.commit().await?;
        let mut tx = self.pool.begin().await?;
        let requested = coordinator_context
            .invoke(
                &mut tx,
                "request_card",
                json!({"case_ref":case,"case_version":version}),
            )
            .await?;
        ensure!(
            requested
                == json!({"command":"request_card","target_agent":"huabaosi","case_ref":case,"case_version":version}),
            "agent_runtime_plan_mismatch"
        );
        coordinator_context
            .record(&mut tx, "request_card", requested.clone())
            .await?;
        tx.commit().await?;
        let requested_case: Uuid = serde_json::from_value(requested["case_ref"].clone())?;
        let work = self.request_card(requested_case).await?;
        let tenant:String=sqlx::query_scalar("SELECT f.tenant_key FROM qintopia_agent_os.welcome_foundation_targets f JOIN qintopia_agent_os.welcome_targets t ON t.id=f.target_id JOIN qintopia_agent_os.welcome_cases c ON c.source_instance=t.source_instance AND c.property_id=t.property_id WHERE c.id=$1 LIMIT 1").bind(case).fetch_one(&self.pool).await?;
        let mut available_tx = self.pool.begin().await?;
        require_executor(&mut available_tx, &tenant, "anan").await?;
        require_executor(&mut available_tx, &tenant, "huabaosi").await?;
        available_tx.commit().await?;
        let mut tx = self.pool.begin().await?;
        let context = AgentTaskContext::from_task(&mut tx, work, "huabaosi").await?;
        let rendered=context.invoke(&mut tx,"render_card",json!({"case_ref":case,"case_version":version,"material":{"display_name":material.display_name,"description":material.description}})).await?;
        ensure!(
            rendered["command"] == "register_card"
                && rendered["case_ref"] == json!(case)
                && rendered["case_version"] == json!(version)
                && rendered["media_type"] == "image/png",
            "agent_runtime_plan_mismatch"
        );
        use base64ct::Encoding;
        let encoded = rendered["content_base64"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("agent_runtime_image_required"))?;
        ensure!(
            encoded.len() <= 14 * 1024 * 1024,
            "agent_runtime_image_too_large"
        );
        let bytes = base64ct::Base64::decode_vec(encoded)
            .map_err(|_| anyhow::anyhow!("agent_runtime_image_invalid"))?;
        ensure!(
            !bytes.is_empty()
                && bytes.len() <= 10 * 1024 * 1024
                && rendered["content_hash"] == digest(&bytes),
            "agent_runtime_image_invalid"
        );
        context.record(&mut tx,"render_started",json!({"case_ref":case,"renderer":"synthetic-pillow-v1","runtime":"local_scripted_agent_runtime","legacy_migration":false})).await?;
        tx.commit().await?;
        let image = self
            .register_artifact(&ArtifactInput {
                case,
                expected_version: version,
                application_revision: revision,
                template: "synthetic-pillow-v1",
                kind: "welcome_card",
                bytes: &bytes,
            })
            .await?;
        let text = self
            .register_artifact(&ArtifactInput {
                case,
                expected_version: version,
                application_revision: revision,
                template: "welcome-text-v1",
                kind: "welcome_text",
                bytes: material.welcome_text.as_bytes(),
            })
            .await?;
        let mut tx = self.pool.begin().await?;
        for (artifact, content, media) in [
            (image, bytes.as_slice(), "image/png"),
            (text, material.welcome_text.as_bytes(), "text/plain"),
        ] {
            sqlx::query("INSERT INTO qintopia_agent_os.welcome_local_artifact_data(artifact_id,content_hash,content,media_type) VALUES ($1,$2,$3,$4) ON CONFLICT (artifact_id) DO NOTHING")
                .bind(artifact).bind(digest(content)).bind(content).bind(media).execute(&mut *tx).await?;
        }
        context
            .record(
                &mut tx,
                "render_completed",
                json!({"image_ref":image,"text_ref":text,"content_hash":digest(&bytes)}),
            )
            .await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items SET status='completed' WHERE id=$1")
            .bind(work)
            .execute(&mut *tx)
            .await?;
        tx.commit().await?;
        let intent = self.upload_intent(image).await?;
        let application:Uuid=sqlx::query_scalar("SELECT application_id FROM qintopia_agent_os.welcome_artifact_bindings WHERE artifact_id=$1").bind(image).fetch_one(&self.pool).await?;
        // Fixed application attachment boundary; never the designer's ordinary Base.
        self.register_upload_readback(
            intent,
            application,
            &digest(&bytes),
            &format!("feishu-base://resident-welcome/{application}/{intent}"),
        )
        .await?;
        Ok(
            json!({"image_ref":image,"text_ref":text,"case_ref":case,"status":"rendered","upload_adapter":"synthetic_application_base","legacy_renderer_migrated":false,"runtime":"local_scripted_agent_runtime"}),
        )
    }

    /// Both routes create real, durable Agent tasks. Only eligible parts are prepared.
    pub async fn foundation_prepare(
        &self,
        case: Uuid,
        target_ref: Uuid,
        phase: &str,
    ) -> Result<Value> {
        self.foundation_prepare_bound(case, target_ref, phase, None)
            .await
    }

    pub async fn foundation_prepare_exact(
        &self,
        case: Uuid,
        target: Uuid,
        phase: &str,
        expected_case_version: i64,
        expected_rule_id: Uuid,
    ) -> Result<Value> {
        self.foundation_prepare_bound(
            case,
            target,
            phase,
            Some((expected_case_version, expected_rule_id)),
        )
        .await
    }

    async fn foundation_prepare_bound(
        &self,
        case: Uuid,
        target_ref: Uuid,
        phase: &str,
        expected: Option<(i64, Uuid)>,
    ) -> Result<Value> {
        ensure!(matches!(phase, "formal" | "preview"), "invalid_phase");
        let mut tx = self.pool.begin().await?;
        let (tenant, scope) = target(&mut tx, target_ref).await?;
        check_case_target(&mut tx, case, target_ref).await?;
        let rule = effective_knowledge_in(&mut tx, &tenant, scope, "resident_welcome", Some(case))
            .await?
            .ok_or_else(|| anyhow::anyhow!("welcome_rule_required"))?;
        let version: i64 = sqlx::query_scalar(
            "SELECT version FROM qintopia_agent_os.welcome_cases WHERE id=$1 FOR UPDATE",
        )
        .bind(case)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(
            expected.is_none_or(|e| e == (version, rule.id)),
            "welcome_version_conflict"
        );
        let setting: WelcomeSetting = serde_json::from_value(rule.content.clone())?;
        setting.validate()?;
        ensure!(setting.phase == phase, "welcome_phase_mismatch");
        require_executor(&mut tx, &tenant, "anan").await?;
        let work = create_work(
            &mut tx,
            &format!("welcome-coordinate/{case}/{target_ref}"),
            "welcome_event",
            "anan",
            "processing",
        )
        .await?;
        let coordinator = AgentTaskContext::from_task(&mut tx, work, "anan").await?;
        let plan = coordinator
            .invoke(
                &mut tx,
                "prepare",
                json!({"case_ref":case,"target_ref":target_ref,"phase":phase,"setting":setting}),
            )
            .await?;
        let expected_parts:Vec<Value>=["image","text"].iter().map(|part|json!({"part":part,"requires_content_review":setting.mode=="review" || !setting.parts.iter().any(|p|p==part)})).collect();
        ensure!(
            plan == json!({"command":"prepare_parts","case_ref":case,"target_ref":target_ref,"phase":phase,"parts":expected_parts}),
            "agent_runtime_plan_mismatch"
        );
        register_template_text(
            &mut tx,
            case,
            target_ref,
            &setting.text_template,
            &rule.id.to_string(),
        )
        .await?;
        let artifacts=sqlx::query("SELECT DISTINCT ON (a.artifact_type) a.id,a.artifact_type FROM qintopia_agent_os.artifacts a JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=a.id JOIN qintopia_agent_os.welcome_cases c ON c.id=b.case_id WHERE b.case_id=$1 AND (b.target_id=$2 OR (b.target_id IS NULL AND a.artifact_type='welcome_card')) AND b.revoked_at IS NULL AND b.case_version=c.version ORDER BY a.artifact_type,a.created_at DESC,a.id DESC")
            .bind(case).bind(target_ref).fetch_all(&mut *tx).await?;
        let mut actions = vec![];
        let mut waiting = vec![];
        for step in plan["parts"]
            .as_array()
            .ok_or_else(|| anyhow::anyhow!("agent_runtime_plan_mismatch"))?
        {
            let part = step["part"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("agent_runtime_plan_mismatch"))?;
            let kind = if part == "image" {
                "welcome_card"
            } else {
                "welcome_text"
            };
            let Some(artifact) = artifacts
                .iter()
                .find(|a| a.get::<String, _>("artifact_type") == kind)
                .map(|a| a.get::<Uuid, _>("id"))
            else {
                waiting.push(json!({"part":part,"reason":"content_required"}));
                continue;
            };
            let decision = decision(&self.pool, &mut tx, case, target_ref, phase, artifact).await;
            let basis = match decision {
                Ok(b) => b,
                Err(e) => {
                    if e.downcast_ref::<sqlx::Error>().is_some() {
                        return Err(e);
                    }
                    let reason = e.to_string();
                    sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status='cancelled',version=version+1 WHERE case_id=$1 AND target_id=$2 AND phase=$3 AND part=$4 AND status IN ('prepared','claimed','retryable')")
                        .bind(case).bind(target_ref).bind(phase).bind(part).execute(&mut *tx).await?;
                    sqlx::query("UPDATE qintopia_agent_os.work_items w SET status='cancelled',claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL FROM qintopia_agent_os.welcome_actions a WHERE a.work_item_id=w.id AND a.case_id=$1 AND a.target_id=$2 AND a.phase=$3 AND a.part=$4 AND a.status='cancelled'")
                        .bind(case).bind(target_ref).bind(phase).bind(part).execute(&mut *tx).await?;
                    let mut pending = json!({"part":part,"reason":reason,"artifact_ref":artifact});
                    if reason == "content_approval_required"
                        || reason == "upper_confirmation_required"
                    {
                        let kind = if reason == "content_approval_required" {
                            "content_review"
                        } else {
                            "publish_confirmation"
                        };
                        let publisher = authorize_current(
                            &mut tx,
                            &tenant,
                            rule.author,
                            scope,
                            "erhua",
                            "community_service",
                            "publish",
                        )
                        .await?;
                        let reviewer = if kind == "content_review" {
                            content_reviewer(&self.pool, &tenant, &mut tx, scope, rule.author)
                                .await?
                                .0
                        } else {
                            publisher
                                .reviewer
                                .ok_or_else(|| anyhow::anyhow!("designated_reviewer_required"))?
                        };
                        pending["approval_kind"] = json!(kind);
                        pending["reviewer"] = json!(reviewer);
                        let review_work = create_work(
                            &mut tx,
                            &format!(
                                "welcome-review/{case}/{target_ref}/{phase}/{artifact}/{}/{kind}",
                                rule.id
                            ),
                            "welcome_review",
                            "erhua",
                            "awaiting_review",
                        )
                        .await?;
                        require_executor(&mut tx, &tenant, "erhua").await?;
                        let review =
                            AgentTaskContext::from_task(&mut tx, review_work, "erhua").await?;
                        let hash: String = sqlx::query_scalar(
                            "SELECT content_hash FROM qintopia_agent_os.artifacts WHERE id=$1",
                        )
                        .bind(artifact)
                        .fetch_one(&mut *tx)
                        .await?;
                        let input = json!({"case_ref":case,"target_ref":target_ref,"phase":phase,"artifact_ref":artifact,"content_hash":hash,"approval_kind":kind,"reviewer":reviewer});
                        let instruction = review
                            .invoke(&mut tx, "forward_review", input.clone())
                            .await?;
                        let mut expected = input;
                        expected["command"] = json!("forward_review");
                        ensure!(instruction == expected, "agent_runtime_plan_mismatch");
                        let mut payload = instruction;
                        payload.as_object_mut().unwrap().remove("command");
                        payload["rule_ref"] = json!(rule.id);
                        payload["reason"] = json!(reason);
                        sqlx::query(
                            "UPDATE qintopia_agent_os.work_items SET payload=$2 WHERE id=$1",
                        )
                        .bind(review_work)
                        .bind(&payload)
                        .execute(&mut *tx)
                        .await?;
                        review.record(&mut tx,"review_forwarded",json!({"artifact_ref":payload["artifact_ref"],"content_hash":payload["content_hash"],"adapter":"synthetic_private_review","reason":reason,"approval_kind":payload["approval_kind"],"reviewer":payload["reviewer"]})).await?;
                    }
                    waiting.push(pending);
                    continue;
                }
            };
            let key = format!("welcome-v1/{case}/{phase}/{target_ref}/{part}");
            let send_work = create_work(
                &mut tx,
                &key,
                "welcome_delivery",
                "erhua",
                "awaiting_publish",
            )
            .await?;
            let row=sqlx::query("SELECT t.version,s.execution_epoch FROM qintopia_agent_os.welcome_targets t JOIN qintopia_agent_os.welcome_sources s ON s.source_instance=t.source_instance AND s.property_id=t.property_id WHERE t.id=$1").bind(target_ref).fetch_one(&mut *tx).await?;
            let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_actions(case_id,target_id,phase,part,work_item_id,approval_id,target_version,execution_epoch,artifact_id,foundation_basis) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9,$10) ON CONFLICT (case_id,phase,target_id,part) DO UPDATE SET foundation_basis=CASE WHEN qintopia_agent_os.welcome_actions.status IN ('prepared','cancelled','retryable') THEN EXCLUDED.foundation_basis ELSE qintopia_agent_os.welcome_actions.foundation_basis END,artifact_id=CASE WHEN qintopia_agent_os.welcome_actions.status IN ('prepared','cancelled','retryable') THEN EXCLUDED.artifact_id ELSE qintopia_agent_os.welcome_actions.artifact_id END,approval_id=CASE WHEN qintopia_agent_os.welcome_actions.status IN ('prepared','cancelled','retryable') THEN EXCLUDED.approval_id ELSE qintopia_agent_os.welcome_actions.approval_id END,target_version=CASE WHEN qintopia_agent_os.welcome_actions.status IN ('prepared','cancelled','retryable') THEN EXCLUDED.target_version ELSE qintopia_agent_os.welcome_actions.target_version END,status=CASE WHEN qintopia_agent_os.welcome_actions.status IN ('prepared','cancelled','retryable') THEN 'prepared' ELSE qintopia_agent_os.welcome_actions.status END RETURNING id")
                .bind(case).bind(target_ref).bind(phase).bind(part).bind(send_work).bind(basis["approval_ref"].as_str().map(Uuid::parse_str).transpose()?).bind(row.get::<i64,_>("version")).bind(row.get::<i64,_>("execution_epoch")).bind(artifact).bind(&basis).fetch_one(&mut *tx).await?;
            sqlx::query("UPDATE qintopia_agent_os.work_items w SET status='awaiting_publish' FROM qintopia_agent_os.welcome_actions a WHERE a.id=$1 AND a.work_item_id=w.id AND a.status='prepared'").bind(id).execute(&mut *tx).await?;
            actions.push(id);
        }
        sqlx::query("UPDATE qintopia_agent_os.work_items SET payload=$2,status=$3 WHERE id=$1")
            .bind(work).bind(json!({"case_ref":case,"target_ref":target_ref,"actions":actions,"waiting":waiting}))
            .bind(if waiting.is_empty(){"completed"}else{"awaiting_review"}).execute(&mut *tx).await?;
        coordinator
            .record(
                &mut tx,
                "conditions_evaluated",
                json!({"target_ref":target_ref,"actions":actions,"waiting":waiting}),
            )
            .await?;
        tx.commit().await?;
        Ok(
            json!({"case_ref":case,"target_ref":target_ref,"actions":actions,"waiting":waiting,"status":if waiting.is_empty(){"ready"}else{"waiting"}}),
        )
    }

    pub async fn foundation_approve(
        &self,
        tenant: &str,
        person: Uuid,
        request: &ReviewRequest,
    ) -> Result<Value> {
        self.foundation_approve_as(tenant, person, request, None)
            .await
    }

    pub async fn foundation_approve_as(
        &self,
        tenant: &str,
        person: Uuid,
        request: &ReviewRequest,
        approval_kind: Option<&str>,
    ) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        ensure!(
            matches!(request.phase.as_str(), "formal" | "preview"),
            "invalid_phase"
        );
        let (bound_tenant, scope) = target(&mut tx, request.target).await?;
        ensure!(tenant == bound_tenant, "review_scope_denied");
        let row=sqlx::query("SELECT a.content_hash,a.artifact_type,b.case_id,b.case_version,c.version,t.version AS target_version FROM qintopia_agent_os.artifacts a JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=a.id JOIN qintopia_agent_os.welcome_cases c ON c.id=b.case_id JOIN qintopia_agent_os.welcome_targets t ON t.id=$2 AND t.source_instance=c.source_instance AND t.property_id=c.property_id WHERE a.id=$1 AND b.revoked_at IS NULL FOR UPDATE OF c,a")
            .bind(request.artifact).bind(request.target).fetch_one(&mut *tx).await?;
        let case: Uuid = row.get("case_id");
        check_case_target(&mut tx, case, request.target).await?;
        ensure!(
            current_artifact(&mut tx, case, request.target, request.artifact).await?,
            "content_version_changed"
        );
        ensure!(
            row.get::<String, _>("content_hash") == request.content_hash
                && row.get::<i64, _>("target_version") == request.target_version
                && row.get::<i64, _>("case_version") == row.get::<i64, _>("version"),
            "review_version_conflict"
        );
        ensure!(
            evaluate_tx(&mut tx, case).await?.card_ready,
            "case_not_ready"
        );
        let rule = effective_knowledge_in(&mut tx, tenant, scope, "resident_welcome", Some(case))
            .await?
            .ok_or_else(|| anyhow::anyhow!("welcome_rule_required"))?;
        let publisher = authorize_current(
            &mut tx,
            tenant,
            rule.author,
            scope,
            "erhua",
            "community_service",
            "publish",
        )
        .await?;
        ensure!(publisher.status != "denied", "publish_authority_required");
        let setting: WelcomeSetting = serde_json::from_value(rule.content.clone())?;
        setting.validate()?;
        let part = if row.get::<String, _>("artifact_type") == "welcome_card" {
            "image"
        } else {
            "text"
        };
        let content_required = setting.mode == "review" || !setting.parts.iter().any(|p| p == part);
        let upper_required = publisher.status == "confirmation_required";
        let content_authority = if content_required {
            Some(content_reviewer(&self.pool, tenant, &mut tx, scope, rule.author).await?)
        } else {
            None
        };
        let kind = approval_kind.unwrap_or(
            if content_authority
                .as_ref()
                .is_some_and(|(p, _)| *p == person)
                && content_required
            {
                "content_review"
            } else {
                "publish_confirmation"
            },
        );
        let (designated, action) = match kind {
            "content_review" => {
                ensure!(content_required, "content_review_not_required");
                (content_authority.as_ref().unwrap().0, "review")
            }
            "publish_confirmation" => {
                ensure!(upper_required, "publish_confirmation_not_required");
                (
                    publisher
                        .reviewer
                        .ok_or_else(|| anyhow::anyhow!("designated_reviewer_required"))?,
                    "publish",
                )
            }
            _ => anyhow::bail!("invalid_approval_kind"),
        };
        ensure!(person == designated, "designated_reviewer_required");
        let auth = if kind == "content_review" {
            content_authority.unwrap().1
        } else {
            authorize_current(
                &mut tx,
                tenant,
                person,
                scope,
                "erhua",
                "community_service",
                action,
            )
            .await?
        };
        ensure!(
            auth.status == "autonomous",
            if kind == "content_review" {
                "review_authority_required"
            } else {
                "upper_confirmation_authority_required"
            }
        );
        let basis = approval_basis(tenant, scope, &rule, &publisher, &auth, kind);
        let existing=sqlx::query("SELECT id,foundation_basis,content_hash,artifact_id,target_id,phase,approved_by,target_version FROM qintopia_agent_os.welcome_approvals WHERE id=$1").bind(request.operation).fetch_optional(&mut *tx).await?;
        if let Some(existing) = existing {
            ensure!(
                existing.get::<Value, _>("foundation_basis") == basis
                    && existing.get::<String, _>("content_hash") == request.content_hash
                    && existing.get::<Uuid, _>("artifact_id") == request.artifact
                    && existing.get::<Uuid, _>("target_id") == request.target
                    && existing.get::<Uuid, _>("approved_by") == person
                    && existing.get::<String, _>("phase") == request.phase
                    && existing.get::<i64, _>("target_version") == request.target_version,
                "approval_operation_conflict"
            );
            tx.commit().await?;
            // Approval may have committed before preparation failed. Replay the
            // idempotent preparation with current authority, never the send.
            self.foundation_prepare(case, request.target, &request.phase)
                .await?;
            return Ok(json!({"approval_ref":request.operation,"version":1,"approval_kind":kind}));
        }
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_approvals(id,artifact_id,target_id,target_version,phase,content_hash,approved_by,foundation_basis) VALUES ($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(request.operation).bind(request.artifact).bind(request.target).bind(request.target_version).bind(&request.phase).bind(&request.content_hash).bind(person).bind(basis).execute(&mut *tx).await?;
        // Artifact-wide approved is deliberately unnecessary: approval is per target/version.
        sqlx::query("INSERT INTO qintopia_agent_os.work_item_events(event_type,actor_type,actor_id,data) VALUES ('welcome_content_approved','human',$1,$2)")
            .bind(person.to_string()).bind(json!({"approval_ref":request.operation,"artifact_ref":request.artifact,"target_ref":request.target,"content_hash":request.content_hash,"approval_kind":kind})).execute(&mut *tx).await?;
        let reviews:Vec<Uuid>=sqlx::query_scalar("UPDATE qintopia_agent_os.work_items SET status='completed' WHERE work_item_type='welcome_review' AND target_agent='erhua' AND payload->>'artifact_ref'=$1 AND payload->>'target_ref'=$2 AND payload->>'rule_ref'=$3 AND payload->>'approval_kind'=$4 RETURNING id")
            .bind(request.artifact.to_string()).bind(request.target.to_string()).bind(rule.id.to_string()).bind(kind).fetch_all(&mut *tx).await?;
        for work in reviews {
            AgentTaskContext::from_task(&mut tx,work,"erhua").await?.record(&mut tx,"review_decision_returned",json!({"approval_ref":request.operation,"reviewer":person,"artifact_ref":request.artifact})).await?;
        }
        tx.commit().await?;
        self.foundation_prepare(case, request.target, &request.phase)
            .await?;
        Ok(json!({"approval_ref":request.operation,"version":1,"approval_kind":kind}))
    }

    /// Fixed synthetic provider adapter. Success records actual adapter invocation;
    /// a lost acknowledgement persists its effect before reporting unknown.
    pub async fn foundation_execute(
        &self,
        action: Uuid,
        outcome: SyntheticOutcome,
    ) -> Result<Value> {
        if matches!(outcome, SyntheticOutcome::Unavailable) {
            return Ok(
                json!({"status":"waiting","reason":"executor_unavailable","action_ref":action}),
            );
        }
        let status:String=sqlx::query_scalar("SELECT status FROM qintopia_agent_os.welcome_actions WHERE id=$1 AND foundation_basis IS NOT NULL").bind(action).fetch_one(&self.pool).await?;
        if status == "succeeded" || status == "unknown" {
            return Ok(json!({"status":status,"action_ref":action}));
        }
        let mut available_tx = self.pool.begin().await?;
        let tenant: String = sqlx::query_scalar(
            "SELECT foundation_basis->>'tenant' FROM qintopia_agent_os.welcome_actions WHERE id=$1",
        )
        .bind(action)
        .fetch_one(&mut *available_tx)
        .await?;
        if require_executor(&mut available_tx, &tenant, "erhua")
            .await
            .is_err()
        {
            return Ok(
                json!({"status":"waiting","reason":"executor_unavailable","action_ref":action}),
            );
        }
        available_tx.commit().await?;
        let claim = self.claim_delivery(action, Uuid::new_v4()).await?;
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT a.work_item_id,a.artifact_id,a.target_id,a.phase,a.part,ar.content_hash FROM qintopia_agent_os.welcome_actions a JOIN qintopia_agent_os.artifacts ar ON ar.id=a.artifact_id WHERE a.id=$1 AND a.attempt_id=$2 AND a.status='claimed' FOR UPDATE OF a").bind(action).bind(claim.attempt).fetch_one(&mut *tx).await?;
        let context =
            AgentTaskContext::from_task(&mut tx, row.get("work_item_id"), "erhua").await?;
        let input = json!({"action_ref":action,"artifact_ref":row.get::<Uuid,_>("artifact_id"),"content_hash":row.get::<String,_>("content_hash"),"target_ref":row.get::<Uuid,_>("target_id"),"phase":row.get::<String,_>("phase"),"part":row.get::<String,_>("part"),"attempt_ref":claim.attempt});
        let invoked = context.invoke(&mut tx, "forward", input.clone()).await;
        let mut expected = input;
        expected["command"] = json!("forward");
        let instruction = match invoked {
            Ok(value) if value == expected => value,
            result => {
                let reason = if result.is_ok() {
                    "agent_runtime_plan_mismatch"
                } else {
                    "agent_runtime_unavailable"
                };
                // The runtime only proposed a send: no provider boundary was crossed.
                sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status='prepared',attempt_id=NULL,version=version+1 WHERE id=$1 AND attempt_id=$2 AND status='claimed'").bind(action).bind(claim.attempt).execute(&mut *tx).await?;
                sqlx::query("UPDATE qintopia_agent_os.work_items SET status='awaiting_publish',claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL,last_error=$2 WHERE id=$1").bind(context.work).bind(reason).execute(&mut *tx).await?;
                context
                    .record(
                        &mut tx,
                        "runtime_waiting",
                        json!({"reason":reason,"external_effects":false}),
                    )
                    .await?;
                tx.commit().await?;
                return Ok(json!({"status":"waiting","reason":reason,"action_ref":action}));
            }
        };
        tx.commit().await?;
        // Policy is checked again after runtime planning, immediately before sending.
        self.begin_synthetic_attempt(&claim).await?;
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT artifact_id FROM qintopia_agent_os.welcome_actions WHERE id=$1 AND attempt_id=$2 AND status='sending' FOR UPDATE").bind(action).bind(claim.attempt).fetch_one(&mut *tx).await?;
        let artifact: Uuid = serde_json::from_value(instruction["artifact_ref"].clone())?;
        let hash = instruction["content_hash"]
            .as_str()
            .ok_or_else(|| anyhow::anyhow!("agent_runtime_plan_mismatch"))?
            .to_string();
        ensure!(
            row.get::<Uuid, _>("artifact_id") == artifact,
            "content_version_changed"
        );
        let receipt = digest(format!("synthetic-receipt/{action}/{artifact}/{hash}").as_bytes());
        if matches!(
            outcome,
            SyntheticOutcome::Success | SyntheticOutcome::UnknownAfterSend
        ) {
            sqlx::query("INSERT INTO qintopia_agent_os.welcome_synthetic_effects(action_id,attempt_id,agent_key,artifact_id,content_hash,receipt_hash) VALUES ($1,$2,'erhua',$3,$4,$5) ON CONFLICT (action_id) DO NOTHING")
                .bind(action).bind(claim.attempt).bind(artifact).bind(&hash).bind(&receipt).execute(&mut *tx).await?;
        }
        context.record(&mut tx,"forward_attempt",json!({"action_ref":action,"artifact_ref":artifact,"content_hash":hash,"adapter":"synthetic_group","attempt_ref":claim.attempt})).await?;
        tx.commit().await?;
        let result = match outcome {
            SyntheticOutcome::Success => Outcome::Success,
            SyntheticOutcome::UnknownAfterSend => Outcome::Unknown,
            SyntheticOutcome::DefiniteNoSend => Outcome::DefiniteNoSend,
            _ => Outcome::PermanentFailure,
        };
        self.record_outcome(
            &claim,
            result,
            if matches!(outcome, SyntheticOutcome::Success) {
                Some(&receipt)
            } else {
                None
            },
        )
        .await?;
        let state = match outcome {
            SyntheticOutcome::Success => "succeeded",
            SyntheticOutcome::UnknownAfterSend => "unknown",
            SyntheticOutcome::DefiniteNoSend => "retryable",
            _ => "failed",
        };
        Ok(json!({"action_ref":action,"status":state,"adapter":"synthetic_group"}))
    }

    /// Provider readback, not resend. Missing receipt leaves the action unknown.
    pub async fn foundation_reconcile_unknown(&self, action: Uuid) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT a.status,a.work_item_id,e.receipt_hash FROM qintopia_agent_os.welcome_actions a LEFT JOIN qintopia_agent_os.welcome_synthetic_effects e ON e.action_id=a.id WHERE a.id=$1 AND a.foundation_basis IS NOT NULL FOR UPDATE OF a").bind(action).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<String, _>("status") == "unknown",
            "unknown_action_required"
        );
        if let Some(receipt) = row.get::<Option<String>, _>("receipt_hash") {
            sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status='succeeded',receipt_hash=$2,version=version+1 WHERE id=$1").bind(action).bind(receipt).execute(&mut *tx).await?;
            sqlx::query("UPDATE qintopia_agent_os.work_items SET status='completed',claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL WHERE id=$1").bind(row.get::<Uuid,_>("work_item_id")).execute(&mut *tx).await?;
            AgentTaskContext::from_task(&mut tx, row.get("work_item_id"), "erhua")
                .await?
                .record(
                    &mut tx,
                    "receipt_reconciled",
                    json!({"action_ref":action,"resent":false}),
                )
                .await?;
            tx.commit().await?;
            return Ok(json!({"action_ref":action,"status":"succeeded","resent":false}));
        }
        Ok(
            json!({"action_ref":action,"status":"unknown","reason":"receipt_check_required","resent":false}),
        )
    }

    pub async fn foundation_state(&self, tenant: &str, person: Uuid) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        let delegated = delegated_scopes_in(&self.pool, tenant, &mut tx, person).await?;
        let targets=sqlx::query("SELECT t.*,f.scope_id FROM qintopia_agent_os.welcome_foundation_targets f JOIN qintopia_agent_os.welcome_targets t ON t.id=f.target_id WHERE f.tenant_key=$1 ORDER BY t.display_name").bind(tenant).fetch_all(&mut *tx).await?;
        let mut result = vec![];
        for row in targets {
            let scope: Uuid = row.get("scope_id");
            let permission = authorize_current(
                &mut tx,
                tenant,
                person,
                scope,
                "erhua",
                "community_service",
                "change_rules",
            )
            .await?;
            let review = authorize_current(
                &mut tx,
                tenant,
                person,
                scope,
                "erhua",
                "community_service",
                "review",
            )
            .await?;
            let publish = authorize_current(
                &mut tx,
                tenant,
                person,
                scope,
                "erhua",
                "community_service",
                "publish",
            )
            .await?;
            if !delegated.iter().any(|d| d["scope"] == json!(scope))
                && permission.status == "denied"
                && publish.status == "denied"
                && review.status == "denied"
                && !can_inspect_current(
                    &mut tx,
                    tenant,
                    person,
                    scope,
                    "erhua",
                    "community_service",
                )
                .await?
            {
                continue;
            }
            let target: Uuid = row.get("id");
            let cases=sqlx::query("SELECT c.id,c.version,c.readiness_reasons,p.display_name FROM qintopia_agent_os.welcome_cases c LEFT JOIN qintopia_identity.persons p ON p.id=c.person_id JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id WHERE c.source_instance=$1 AND c.property_id=$2 AND ($3<>'building' OR v.projection->>'building'=$4)")
                .bind(row.get::<String,_>("source_instance")).bind(row.get::<String,_>("property_id")).bind(row.get::<String,_>("kind")).bind(row.get::<String,_>("building_code")).fetch_all(&mut *tx).await?;
            let actions:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',a.id,'case_ref',a.case_id,'part',a.part,'status',a.status,'artifact_ref',a.artifact_id,'basis',a.foundation_basis) FROM qintopia_agent_os.welcome_actions a WHERE a.target_id=$1 AND a.foundation_basis IS NOT NULL ORDER BY a.part").bind(target).fetch_all(&mut *tx).await?;
            let artifacts:Vec<Value>=sqlx::query_scalar("SELECT DISTINCT ON (b.case_id,a.artifact_type) jsonb_build_object('artifact_ref',a.id,'kind',a.artifact_type,'content_hash',a.content_hash,'text',a.content_text,'case_ref',b.case_id,'target_version',$3::bigint) FROM qintopia_agent_os.artifacts a JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=a.id JOIN qintopia_agent_os.welcome_cases c ON c.id=b.case_id JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id WHERE c.source_instance=$1 AND c.property_id=$2 AND b.revoked_at IS NULL AND (b.target_id=$4 OR (b.target_id IS NULL AND a.artifact_type='welcome_card')) AND ($5<>'building' OR v.projection->>'building'=$6) ORDER BY b.case_id,a.artifact_type,a.created_at DESC,a.id DESC")
                .bind(row.get::<String,_>("source_instance")).bind(row.get::<String,_>("property_id")).bind(row.get::<i64,_>("version")).bind(target).bind(row.get::<String,_>("kind")).bind(row.get::<String,_>("building_code")).fetch_all(&mut *tx).await?;
            let rule =
                effective_knowledge_in(&mut tx, tenant, scope, "resident_welcome", None).await?;
            let mut progress:Vec<Value>=sqlx::query_scalar("SELECT payload FROM qintopia_agent_os.work_items WHERE work_item_type='welcome_event' AND target_agent='anan' AND payload->>'target_ref'=$1").bind(target.to_string()).fetch_all(&mut *tx).await?;
            // Re-resolve the review recipient on read: historical work payloads may name a revoked proxy.
            for entry in &mut progress {
                let case = entry["case_ref"]
                    .as_str()
                    .and_then(|s| Uuid::parse_str(s).ok());
                if let Some(active) =
                    effective_knowledge_in(&mut tx, tenant, scope, "resident_welcome", case).await?
                {
                    if let Some(waiting) = entry["waiting"].as_array_mut() {
                        for pending in waiting {
                            if pending["approval_kind"] == "content_review" {
                                match content_reviewer(
                                    &self.pool,
                                    tenant,
                                    &mut tx,
                                    scope,
                                    active.author,
                                )
                                .await
                                {
                                    Ok((reviewer, _)) => pending["reviewer"] = json!(reviewer),
                                    Err(e) => {
                                        pending["reviewer"] = Value::Null;
                                        pending["reason"] = json!(
                                            crate::person_collaboration::foundation_error_code(&e)
                                        );
                                    }
                                }
                            }
                        }
                    }
                }
            }
            let latest:Option<i64>=sqlx::query_scalar("SELECT version::bigint FROM qintopia_agent_os.collaboration_knowledge_items WHERE tenant_key=$1 AND scope_id=$2 AND knowledge_key='resident_welcome' AND case_ref IS NULL").bind(tenant).bind(scope).fetch_optional(&mut *tx).await?;
            let latest_rule:Option<Value>=sqlx::query_scalar("SELECT jsonb_build_object('version',i.version,'content',v.definition->'content','id',v.id,'effective_at',r.effective_at,'effective_until',r.effective_until) FROM qintopia_agent_os.collaboration_knowledge_items i JOIN qintopia_agent_os.business_definition_versions v ON v.space_id=i.space_id AND v.definition_key=i.definition_key AND v.version=i.version JOIN qintopia_agent_os.collaboration_knowledge_revisions r ON r.id=v.id WHERE i.tenant_key=$1 AND i.scope_id=$2 AND i.knowledge_key='resident_welcome' AND i.case_ref IS NULL").bind(tenant).bind(scope).fetch_optional(&mut *tx).await?;
            let case_rules:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('case_ref',i.case_ref,'version',i.version,'content',v.definition->'content','id',v.id,'effective_at',r.effective_at,'effective_until',r.effective_until) FROM qintopia_agent_os.collaboration_knowledge_items i JOIN qintopia_agent_os.business_definition_versions v ON v.space_id=i.space_id AND v.definition_key=i.definition_key AND v.version=i.version JOIN qintopia_agent_os.collaboration_knowledge_revisions r ON r.id=v.id WHERE i.tenant_key=$1 AND i.scope_id=$2 AND i.knowledge_key='resident_welcome' AND i.case_ref IS NOT NULL").bind(tenant).bind(scope).fetch_all(&mut *tx).await?;
            result.push(json!({"progress":progress,"latest_rule_version":latest.unwrap_or(0),"latest_rule":latest_rule,"case_rules":case_rules,"target_ref":target,"scope_ref":scope,"label":row.get::<String,_>("display_name"),"version":row.get::<i64,_>("version"),"change_rules":permission.status,"review":review.status,"publish":publish.status,"rule":rule.map(|r|json!({"id":r.id,"version":r.version,"content":r.content,"effective_at":r.effective_at,"effective_until":r.effective_until})),"cases":cases.iter().map(|c|json!({"case_ref":c.get::<Uuid,_>("id"),"version":c.get::<i64,_>("version"),"name":c.get::<Option<String>,_>("display_name"),"reasons":c.get::<Value,_>("readiness_reasons")})).collect::<Vec<_>>(),"actions":actions,"artifacts":artifacts}));
        }
        tx.commit().await?;
        Ok(
            json!({"targets":result,"external_adapter":"synthetic","legacy_renderer_migrated":false}),
        )
    }
}

/// Called by claim and immediately before the durable sending boundary.
pub(crate) async fn check_action(
    pool: &sqlx::PgPool,
    tx: &mut Transaction<'_, Postgres>,
    row: &sqlx::postgres::PgRow,
) -> Result<()> {
    let artifact = row
        .get::<Option<Uuid>, _>("artifact_id")
        .ok_or_else(|| anyhow::anyhow!("action_artifact_required"))?;
    let tenant = row
        .get::<Option<Value>, _>("foundation_basis")
        .and_then(|v| v["tenant"].as_str().map(str::to_string))
        .ok_or_else(|| anyhow::anyhow!("action_authority_required"))?;
    require_executor(tx, &tenant, "erhua").await?;
    let current = decision(
        pool,
        tx,
        row.get("case_id"),
        row.get("target_id"),
        &row.get::<String, _>("phase"),
        artifact,
    )
    .await?;
    ensure!(
        row.get::<Option<Value>, _>("foundation_basis") == Some(current),
        "welcome_authority_or_rule_changed"
    );
    Ok(())
}

async fn decision(
    pool: &sqlx::PgPool,
    tx: &mut Transaction<'_, Postgres>,
    case: Uuid,
    target_ref: Uuid,
    phase: &str,
    artifact: Uuid,
) -> Result<Value> {
    let (tenant, scope) = target(tx, target_ref).await?;
    check_case_target(tx, case, target_ref).await?;
    ensure!(
        current_artifact(tx, case, target_ref, artifact).await?,
        "content_version_changed"
    );
    ensure!(evaluate_tx(tx, case).await?.card_ready, "case_not_ready");
    let row=sqlx::query("SELECT c.version,c.person_id,a.artifact_type,a.content_hash,a.artifact_uri,b.case_version,b.application_revision,b.consent_version,app.revision,app.consent_version AS current_consent,t.version AS target_version,t.kind,t.building_code,v.projection FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_applications app ON app.id=c.application_id JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.case_id=c.id AND b.artifact_id=$3 AND b.revoked_at IS NULL JOIN qintopia_agent_os.artifacts a ON a.id=b.artifact_id JOIN qintopia_agent_os.welcome_targets t ON t.id=$2 AND t.source_instance=c.source_instance AND t.property_id=c.property_id JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id WHERE c.id=$1")
        .bind(case).bind(target_ref).bind(artifact).fetch_one(&mut **tx).await?;
    ensure!(
        row.get::<i64, _>("case_version") == row.get::<i64, _>("version")
            && row.get::<i64, _>("application_revision") == row.get::<i64, _>("revision")
            && row.get::<i64, _>("consent_version") == row.get::<i64, _>("current_consent"),
        "artifact_input_stale"
    );
    let image = row.get::<String, _>("artifact_type") == "welcome_card";
    let part = if image { "image" } else { "text" };
    if image {
        ensure!(
            row.get::<Option<String>, _>("artifact_uri")
                .is_some_and(|s| s.starts_with("feishu-base://resident-welcome/")),
            "application_attachment_required"
        );
    }
    let s: Snapshot = serde_json::from_value(row.get("projection"))?;
    ensure!(
        s.observed_at >= Utc::now() - Duration::seconds(60)
            && s.observed_at <= Utc::now() + Duration::seconds(5),
        "pms_refresh_required"
    );
    ensure!(
        row.get::<String, _>("kind") != "building"
            || row.get::<String, _>("building_code") == s.building,
        "building_changed"
    );
    if phase == "formal" {
        ensure!(s.state == StayState::InHouse, "actual_check_in_required");
        let member:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_members m JOIN qintopia_identity.source_identity_links l ON l.id=m.identity_link_id WHERE m.target_id=$1 AND l.person_id=$2 AND l.status='confirmed' AND m.current AND m.observed_at>=clock_timestamp()-interval '60 seconds' AND m.observed_at<=clock_timestamp()+interval '5 seconds')").bind(target_ref).bind(row.get::<Uuid,_>("person_id")).fetch_one(&mut **tx).await?;
        ensure!(member, "current_target_membership_required");
    } else {
        ensure!(
            phase == "preview" && s.state == StayState::Reserved && s.inventory_reserved,
            "reserved_inventory_required"
        );
    }
    let rule = effective_knowledge_in(tx, &tenant, scope, "resident_welcome", Some(case))
        .await?
        .ok_or_else(|| anyhow::anyhow!("welcome_rule_required"))?;
    let setting: WelcomeSetting = serde_json::from_value(rule.content.clone())?;
    setting.validate()?;
    ensure!(setting.phase == phase, "welcome_phase_mismatch");
    let rules = authorize_current(
        tx,
        &tenant,
        rule.author,
        scope,
        "erhua",
        "community_service",
        "change_rules",
    )
    .await?;
    ensure!(
        rules.status == "autonomous" && rules.grant_id == Some(rule.authority_grant),
        "rule_authority_revoked"
    );
    let publish = authorize_current(
        tx,
        &tenant,
        rule.author,
        scope,
        "erhua",
        "community_service",
        "publish",
    )
    .await?;
    ensure!(publish.status != "denied", "publish_authority_required");
    let upper = publish.status == "confirmation_required";
    let needs_content = setting.mode == "review" || !setting.parts.iter().any(|p| p == part);
    let content_authority = if needs_content {
        Some(content_reviewer(pool, &tenant, tx, scope, rule.author).await?)
    } else {
        None
    };
    let mut content_approval = None;
    let mut publish_confirmation = None;
    for (required, kind, reviewer, action, missing) in [
        (
            needs_content,
            "content_review",
            content_authority.as_ref().map_or(rule.author, |a| a.0),
            "review",
            "content_approval_required",
        ),
        (
            upper,
            "publish_confirmation",
            publish.reviewer.unwrap_or(rule.author),
            "publish",
            "upper_confirmation_required",
        ),
    ] {
        if !required {
            continue;
        }
        let auth = if kind == "content_review" {
            content_authority.as_ref().unwrap().1.clone()
        } else {
            authorize_current(
                tx,
                &tenant,
                reviewer,
                scope,
                "erhua",
                "community_service",
                action,
            )
            .await?
        };
        ensure!(
            auth.status == "autonomous",
            if kind == "content_review" {
                "review_authority_required"
            } else {
                "upper_confirmation_authority_required"
            }
        );
        let expected = approval_basis(&tenant, scope, &rule, &publish, &auth, kind);
        let approval:Option<Uuid>=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.welcome_approvals WHERE artifact_id=$1 AND target_id=$2 AND target_version=$3 AND phase=$4 AND content_hash=$5 AND approved_by=$6 AND revoked_at IS NULL AND foundation_basis=$7 ORDER BY created_at DESC LIMIT 1")
            .bind(artifact).bind(target_ref).bind(row.get::<i64,_>("target_version")).bind(phase).bind(row.get::<String,_>("content_hash")).bind(reviewer).bind(expected).fetch_optional(&mut **tx).await?;
        ensure!(approval.is_some(), "{}", missing);
        if kind == "content_review" {
            content_approval = approval;
        } else {
            publish_confirmation = approval;
        }
    }
    let approval = content_approval.or(publish_confirmation);
    let other:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_actions a JOIN qintopia_agent_os.welcome_targets t ON t.id=a.target_id WHERE a.case_id=$1 AND a.phase=$2 AND t.kind='building' AND a.target_id<>$3 AND a.status IN ('succeeded','sending','unknown'))").bind(case).bind(phase).bind(target_ref).fetch_one(&mut **tx).await?;
    ensure!(
        row.get::<String, _>("kind") != "building" || !other,
        "changed_building_requires_manual_resolution"
    );
    Ok(
        json!({"tenant":tenant,"scope":scope,"rule_ref":rule.id,"rule_version":rule.version,"author":rule.author,"publish_grant":publish.grant_id,"publish_collaboration":publish.collaboration_id,"approval_ref":approval,"content_approval_ref":content_approval,"publish_confirmation_ref":publish_confirmation,"artifact_ref":artifact,"content_hash":row.get::<String,_>("content_hash"),"case_version":row.get::<i64,_>("version"),"target_version":row.get::<i64,_>("target_version"),"basis":if needs_content || upper{"human_approval"}else{"effective_direct_authorization"}}),
    )
}

async fn register_template_text(
    tx: &mut Transaction<'_, Postgres>,
    case: Uuid,
    target: Uuid,
    template: &str,
    rule: &str,
) -> Result<Uuid> {
    ensure!(evaluate_tx(tx, case).await?.card_ready, "case_not_ready");
    let row=sqlx::query("SELECT c.version,c.application_id,a.revision,a.consent_version,coalesce(p.preferred_name,p.display_name) AS name FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_applications a ON a.id=c.application_id JOIN qintopia_identity.persons p ON p.id=c.person_id WHERE c.id=$1").bind(case).fetch_one(&mut **tx).await?;
    let text = template.replace("{name}", &row.get::<String, _>("name"));
    ensure!(
        !text.is_empty() && text.len() <= 8192,
        "invalid_welcome_text"
    );
    let hash = digest(text.as_bytes());
    let version: i64 = row.get("version");
    let revision: i64 = row.get("revision");
    let work = create_work(
        tx,
        &format!("welcome-text/{case}/{target}/{version}/{revision}/{hash}"),
        "welcome_card",
        "anan",
        "completed",
    )
    .await?;
    let artifact:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.artifacts(work_item_id,artifact_type,created_by_agent,content_hash,content_text,review_status) VALUES($1,'welcome_text','anan',$2,$3,'pending') ON CONFLICT(work_item_id,content_hash) WHERE content_hash IS NOT NULL AND content_hash<>'' DO UPDATE SET content_hash=EXCLUDED.content_hash RETURNING id")
        .bind(work).bind(&hash).bind(&text).fetch_one(&mut **tx).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_artifact_bindings(artifact_id,case_id,application_id,application_revision,case_version,consent_version,template_version,target_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(artifact_id) DO NOTHING")
        .bind(artifact).bind(case).bind(row.get::<Uuid,_>("application_id")).bind(revision).bind(version).bind(row.get::<i64,_>("consent_version")).bind(rule).bind(target).execute(&mut **tx).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_local_artifact_data(artifact_id,content_hash,content,media_type) VALUES($1,$2,$3,'text/plain') ON CONFLICT DO NOTHING")
        .bind(artifact).bind(&hash).bind(text.as_bytes()).execute(&mut **tx).await?;
    AgentTaskContext::from_task(tx, work, "anan")
        .await?
        .record(
            tx,
            "template_applied",
            json!({"artifact_ref":artifact,"rule_ref":rule,"target_ref":target}),
        )
        .await?;
    Ok(artifact)
}

impl Store {
    pub async fn foundation_revoke_approval(
        &self,
        tenant: &str,
        person: Uuid,
        approval: Uuid,
        expected: i64,
    ) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT target_id,version,approved_by,foundation_basis FROM qintopia_agent_os.welcome_approvals WHERE id=$1 AND foundation_basis IS NOT NULL FOR UPDATE").bind(approval).fetch_one(&mut *tx).await?;
        let (bound, scope) = target(&mut tx, row.get("target_id")).await?;
        ensure!(
            bound == tenant && row.get::<i64, _>("version") == expected,
            "approval_version_or_scope_conflict"
        );
        ensure!(
            person == row.get::<Uuid, _>("approved_by")
                && authorize_current(
                    &mut tx,
                    tenant,
                    person,
                    scope,
                    "erhua",
                    "community_service",
                    if row.get::<Value, _>("foundation_basis")["approval_kind"]
                        == "publish_confirmation"
                    {
                        "publish"
                    } else {
                        "review"
                    }
                )
                .await?
                .status
                    == "autonomous",
            "approval_revocation_denied"
        );
        sqlx::query("UPDATE qintopia_agent_os.welcome_approvals SET revoked_at=clock_timestamp(),version=version+1 WHERE id=$1").bind(approval).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status='cancelled',version=version+1 WHERE (approval_id=$1 OR foundation_basis->>'content_approval_ref'=$1::text OR foundation_basis->>'publish_confirmation_ref'=$1::text) AND status IN ('prepared','claimed','retryable')").bind(approval).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items w SET status='cancelled',claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL FROM qintopia_agent_os.welcome_actions a WHERE a.work_item_id=w.id AND (a.approval_id=$1 OR a.foundation_basis->>'content_approval_ref'=$1::text OR a.foundation_basis->>'publish_confirmation_ref'=$1::text) AND a.status='cancelled'").bind(approval).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"approval_ref":approval,"version":expected+1,"status":"revoked"}))
    }
}

async fn require_executor(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    agent: &str,
) -> Result<()> {
    let available:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_local_executors WHERE tenant_key=$1 AND agent_key=$2 AND available)").bind(tenant).bind(agent).fetch_one(&mut **tx).await?;
    ensure!(available, "executor_unavailable");
    Ok(())
}

async fn check_case_target(
    tx: &mut Transaction<'_, Postgres>,
    case: Uuid,
    target: Uuid,
) -> Result<()> {
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_targets t ON t.source_instance=c.source_instance AND t.property_id=c.property_id JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id WHERE c.id=$1 AND t.id=$2 AND (t.kind='community' OR (t.kind='building' AND t.building_code=v.projection->>'building')))").bind(case).bind(target).fetch_one(&mut **tx).await?;
    ensure!(valid, "case_target_scope_mismatch");
    Ok(())
}
async fn current_artifact(
    tx: &mut Transaction<'_, Postgres>,
    case: Uuid,
    target: Uuid,
    artifact: Uuid,
) -> Result<bool> {
    let current:Option<Uuid>=sqlx::query_scalar("SELECT a.id FROM qintopia_agent_os.artifacts a JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=a.id JOIN qintopia_agent_os.welcome_cases c ON c.id=b.case_id WHERE b.case_id=$1 AND b.revoked_at IS NULL AND b.case_version=c.version AND a.artifact_type=(SELECT artifact_type FROM qintopia_agent_os.artifacts WHERE id=$3) AND (b.target_id=$2 OR (b.target_id IS NULL AND a.artifact_type='welcome_card')) ORDER BY a.created_at DESC,a.id DESC LIMIT 1")
        .bind(case).bind(target).bind(artifact).fetch_optional(&mut **tx).await?;
    Ok(current == Some(artifact))
}

fn approval_basis(
    tenant: &str,
    scope: Uuid,
    rule: &KnowledgeVersion,
    publisher: &Authority,
    approver: &Authority,
    kind: &str,
) -> Value {
    json!({"tenant":tenant,"scope":scope,"rule_ref":rule.id,"approval_kind":kind,"authority_grant":approver.grant_id,"authority_collaboration":approver.collaboration_id,"publish_grant":publisher.grant_id,"publish_collaboration":publisher.collaboration_id})
}
