//! Minimal human UI, behind a caller-supplied authenticated Actor. Never accept
//! actor/person authority or arbitrary routes from browser JSON.
use super::store::{authorize, Actor, Store};
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

pub const HTML: &str = include_str!("../../../../workflows/resident-welcome/workbench.html");
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct BindRequest {
    pub operation_id: Uuid,
    pub case_ref: Uuid,
    pub expected_version: i64,
    pub person_ref: Uuid,
    pub link_ref: Uuid,
    pub application_ref: Uuid,
}
#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct ApproveRequest {
    pub operation_id: Uuid,
    pub artifact_ref: Uuid,
    pub content_hash: String,
    pub target_ref: Uuid,
    pub target_version: i64,
    pub phase: String,
}

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityRequest {
    pub operation_id: Uuid,
    pub link_ref: Uuid,
    pub expected_version: i64,
    pub person_ref: Option<Uuid>,
    pub new_person_label: Option<String>,
    pub revoke: bool,
}

/// Shared persistence primitive. Callers must authorize and bind the source scope first.
pub(crate) async fn create_person_for_pending_source(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    link: Uuid,
    version: i64,
    label: &str,
    nickname: Option<&str>,
) -> Result<Uuid> {
    ensure!(
        !label.trim().is_empty()
            && label.chars().count() <= 80
            && !label.chars().any(char::is_control),
        "invalid_person_label"
    );
    let row=sqlx::query("SELECT status,version,person_id FROM qintopia_identity.source_identity_links WHERE id=$1 FOR UPDATE").bind(link).fetch_one(&mut **tx).await?;
    ensure!(
        row.get::<String, _>("status") == "pending"
            && row.get::<i64, _>("version") == version
            && row.get::<Option<Uuid>, _>("person_id").is_none(),
        "pending_identity_required"
    );
    Ok(sqlx::query_scalar("INSERT INTO qintopia_identity.persons(display_name,preferred_name) VALUES($1,$2) RETURNING id").bind(label).bind(nickname).fetch_one(&mut **tx).await?)
}

impl Store {
    pub async fn workbench_identity(&self, actor: &Actor, r: &IdentityRequest) -> Result<Value> {
        if let Some(person) = r.person_ref {
            ensure!(r.new_person_label.is_none(), "choose_existing_or_new");
            self.change_identity(
                actor,
                r.operation_id,
                r.link_ref,
                person,
                r.expected_version,
                r.operation_id,
                r.revoke,
            )
            .await?;
        } else {
            let label = r.new_person_label.as_deref().unwrap_or("").trim();
            ensure!(
                !r.revoke
                    && !label.is_empty()
                    && label.chars().count() <= 80
                    && !label.chars().any(char::is_control),
                "invalid_person_label"
            );
            let mut tx = self.pool.begin().await?;
            authorize(&mut tx, actor, "identity", None).await?;
            let input = json!([
                "new_person_from_identity",
                r.link_ref,
                r.expected_version,
                label
            ]);
            if let Some(result) =
                super::store::operation_start(&mut tx, actor, r.operation_id, &input).await?
            {
                return Ok(result);
            }
            let link=sqlx::query("SELECT l.version,l.status FROM qintopia_identity.source_identity_links l WHERE l.id=$1 AND EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_identity_scopes s WHERE s.namespace=l.namespace AND s.source_instance=$2 AND s.property_id=$3) FOR UPDATE OF l")
                .bind(r.link_ref).bind(&actor.source).bind(&actor.property).fetch_one(&mut *tx).await?;
            ensure!(
                link.get::<i64, _>("version") == r.expected_version
                    && link.get::<String, _>("status") == "pending",
                "pending_identity_required"
            );
            let person = create_person_for_pending_source(
                &mut tx,
                r.link_ref,
                r.expected_version,
                label,
                None,
            )
            .await?;
            sqlx::query("UPDATE qintopia_identity.source_identity_links SET person_id=$2,status='confirmed',version=version+1,evidence_ref=$3,confirmed_by=$4,updated_at=now() WHERE id=$1")
                .bind(r.link_ref).bind(person).bind(r.operation_id).bind(actor.person).execute(&mut *tx).await?;
            let result = json!({"message":"已创建人员并确认所选来源；本次住宿仍需关联有效申请。"});
            super::store::operation_finish(&mut tx, actor, r.operation_id, &input, &result).await?;
            tx.commit().await?;
            return Ok(result);
        }
        self.reconcile_scope(&actor.source, &actor.property).await?;
        Ok(
            json!({"message":if r.revoke {"已撤销来源确认；相关未发任务已失效。"} else {"已确认来源人员；本次住宿仍需关联有效申请。"}}),
        )
    }
    pub async fn workbench_state(&self, actor: &Actor) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        authorize(&mut tx, actor, "identity", None).await?;
        let mode:String=sqlx::query_scalar("SELECT mode FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2").bind(&actor.source).bind(&actor.property).fetch_one(&mut *tx).await?;
        ensure!(
            mode == "synthetic",
            "local_workbench_requires_synthetic_scope"
        );
        let rows=sqlx::query("SELECT c.id,c.version,c.readiness_reasons,coalesce(p.preferred_name,p.display_name,'待确认入住人') AS label FROM qintopia_agent_os.welcome_cases c LEFT JOIN qintopia_identity.persons p ON p.id=c.person_id WHERE c.source_instance=$1 AND c.property_id=$2 ORDER BY c.id LIMIT 100")
            .bind(&actor.source).bind(&actor.property).fetch_all(&mut *tx).await?;
        let cases=rows.iter().enumerate().map(|(i,r)|json!({"ref":r.get::<Uuid,_>("id"),"version":r.get::<i64,_>("version"),"label":format!("{} · 本次住宿 {}",r.get::<String,_>("label"),i+1),"readiness":if r.get::<Value,_>("readiness_reasons").as_array().is_some_and(|r|r.is_empty()) {"基本资格已齐备；发布按各目标另行判断"}else{"仍需核对申请、身份、住宿或授权"}})).collect::<Vec<_>>();
        let rows=sqlx::query("SELECT DISTINCT l.id,l.version,l.status,l.person_id,l.subject_type,coalesce(p.preferred_name,p.display_name,'待确认人员') AS label FROM qintopia_identity.source_identity_links l JOIN qintopia_agent_os.welcome_identity_scopes s ON s.namespace=l.namespace LEFT JOIN qintopia_identity.persons p ON p.id=l.person_id WHERE s.source_instance=$1 AND s.property_id=$2 ORDER BY l.id LIMIT 100")
            .bind(&actor.source).bind(&actor.property).fetch_all(&mut *tx).await?;
        let identities=rows.iter().enumerate().map(|(i,r)|json!({"ref":r.get::<Uuid,_>("id"),"version":r.get::<i64,_>("version"),"person_ref":r.get::<Option<Uuid>,_>("person_id"),"status":r.get::<String,_>("status"),"label":format!("{} · {} · 来源 {}",r.get::<String,_>("label"),r.get::<String,_>("subject_type"),i+1)})).collect::<Vec<_>>();
        let rows=sqlx::query("SELECT DISTINCT l.id,l.subject_type,coalesce(p.preferred_name,p.display_name,'待确认人员') AS label FROM qintopia_identity.source_identity_links l JOIN qintopia_agent_os.welcome_identity_scopes s ON s.namespace=l.namespace LEFT JOIN qintopia_identity.persons p ON p.id=l.person_id WHERE s.source_instance=$1 AND s.property_id=$2 AND l.subject_type='pms_occupant' AND l.status='confirmed' ORDER BY l.id LIMIT 100")
            .bind(&actor.source).bind(&actor.property).fetch_all(&mut *tx).await?;
        let links=rows.iter().enumerate().map(|(i,r)|json!({"ref":r.get::<Uuid,_>("id"),"label":format!("{} · 已核对来源 {}",r.get::<String,_>("label"),i+1)})).collect::<Vec<_>>();
        let rows=sqlx::query("SELECT DISTINCT a.id,coalesce(p.preferred_name,p.display_name,'申请人') AS label FROM qintopia_agent_os.welcome_applications a JOIN qintopia_identity.persons p ON p.id=a.person_id JOIN qintopia_identity.source_identity_links l ON l.person_id=p.id JOIN qintopia_agent_os.welcome_identity_scopes s ON s.namespace=l.namespace WHERE s.source_instance=$1 AND s.property_id=$2 AND a.source_instance=$1 AND a.valid ORDER BY a.id LIMIT 100")
            .bind(&actor.source).bind(&actor.property).fetch_all(&mut *tx).await?;
        let applications=rows.iter().enumerate().map(|(i,r)|json!({"ref":r.get::<Uuid,_>("id"),"label":format!("{} · 有效申请 {}",r.get::<String,_>("label"),i+1)})).collect::<Vec<_>>();
        let rows=sqlx::query("SELECT a.id,a.artifact_type,a.content_hash,a.content_text FROM qintopia_agent_os.artifacts a JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=a.id JOIN qintopia_agent_os.welcome_cases c ON c.id=b.case_id WHERE c.source_instance=$1 AND c.property_id=$2 AND b.revoked_at IS NULL ORDER BY a.created_at DESC LIMIT 100")
            .bind(&actor.source).bind(&actor.property).fetch_all(&mut *tx).await?;
        let artifacts=rows.iter().enumerate().map(|(i,r)|json!({"ref":r.get::<Uuid,_>("id"),"content_hash":r.get::<String,_>("content_hash"),"preview":r.get::<Option<String>,_>("content_text"),"label":format!("{} · 版本 {}",if r.get::<String,_>("artifact_type")=="welcome_card" {"欢迎卡片"}else{"欢迎文案"},i+1)})).collect::<Vec<_>>();
        let rows=sqlx::query("SELECT id,version,display_name FROM qintopia_agent_os.welcome_targets WHERE source_instance=$1 AND property_id=$2 AND enabled ORDER BY display_name LIMIT 100").bind(&actor.source).bind(&actor.property).fetch_all(&mut *tx).await?;
        let targets=rows.iter().map(|r|json!({"ref":r.get::<Uuid,_>("id"),"version":r.get::<i64,_>("version"),"label":r.get::<String,_>("display_name")})).collect::<Vec<_>>();
        tx.commit().await?;
        let people = self
            .search_people(actor, "", 20)
            .await?
            .iter()
            .map(|p| json!({"ref":p["person_ref"],"label":p["preferred_name"]}))
            .collect::<Vec<_>>();
        Ok(
            json!({"cases":cases,"people":people,"links":links,"applications":applications,"artifacts":artifacts,"targets":targets,"identities":identities}),
        )
    }

    pub async fn workbench_bind(&self, actor: &Actor, request: &BindRequest) -> Result<Value> {
        let person:Option<Uuid>=sqlx::query_scalar("SELECT person_id FROM qintopia_identity.source_identity_links WHERE id=$1 AND status='confirmed'").bind(request.link_ref).fetch_optional(&self.pool).await?.flatten();
        ensure!(
            person == Some(request.person_ref),
            "所选人员与已核对来源不一致"
        );
        self.bind_case(
            actor,
            request.operation_id,
            request.case_ref,
            request.link_ref,
            request.application_ref,
            request.expected_version,
        )
        .await?;
        Ok(json!({"message":"已保存本次关联；条件将按每个目标重新检查。"}))
    }

    pub async fn workbench_approve(
        &self,
        actor: &Actor,
        request: &ApproveRequest,
    ) -> Result<Value> {
        self.approve(
            actor,
            &super::review::ReviewRequest {
                operation: request.operation_id,
                artifact: request.artifact_ref,
                target: request.target_ref,
                target_version: request.target_version,
                phase: request.phase.clone(),
                content_hash: request.content_hash.clone(),
            },
        )
        .await?;
        Ok(json!({"message":"已保存所选版本与目标的审核；发布授权和实时条件仍会再次检查。"}))
    }
}
