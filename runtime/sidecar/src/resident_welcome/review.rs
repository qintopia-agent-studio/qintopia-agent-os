use super::store::{authorize, operation_finish, operation_start, Actor, Store};
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

pub struct GrantRequest {
    pub operation: Uuid,
    pub person: Uuid,
    pub target: Option<Uuid>,
    pub action: String,
    pub membership: Option<Uuid>,
    pub expires: DateTime<Utc>,
}
pub struct ReviewRequest {
    pub operation: Uuid,
    pub artifact: Uuid,
    pub target: Uuid,
    pub target_version: i64,
    pub phase: String,
    pub content_hash: String,
}
impl Store {
    pub async fn search_people(
        &self,
        actor: &Actor,
        query: &str,
        limit: i64,
    ) -> Result<Vec<Value>> {
        ensure!(
            (1..=20).contains(&limit) && query.len() <= 100,
            "invalid_search"
        );
        let mut tx = self.pool.begin().await?;
        authorize(&mut tx, actor, "identity", None).await?;
        let rows=sqlx::query("SELECT DISTINCT p.id,coalesce(p.preferred_name,p.display_name) AS label FROM qintopia_identity.persons p WHERE p.status='active' AND (p.display_name ILIKE $1 OR p.preferred_name ILIKE $1 OR EXISTS(SELECT 1 FROM qintopia_identity.person_aliases a WHERE a.person_id=p.id AND a.alias ILIKE $1)) AND (EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l JOIN qintopia_agent_os.welcome_identity_scopes s ON s.namespace=l.namespace WHERE l.person_id=p.id AND s.source_instance=$2 AND s.property_id=$3) OR EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_cases c WHERE c.person_id=p.id AND c.source_instance=$2 AND c.property_id=$3) OR EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_grants g WHERE g.person_id=p.id AND g.source_instance=$2 AND g.property_id=$3)) ORDER BY label,id LIMIT $4")
            .bind(format!("%{query}%")).bind(&actor.source).bind(&actor.property).bind(limit).fetch_all(&mut *tx).await?;
        Ok(rows
            .into_iter()
            .map(|r| {
                super::channels::candidate_view(
                    r.get("id"),
                    &r.get::<String, _>("label"),
                    "当前授权范围",
                    "candidate_only",
                )
            })
            .collect())
    }

    pub async fn revoke_approval(
        &self,
        actor: &Actor,
        operation: Uuid,
        approval: Uuid,
        expected: i64,
    ) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT ap.target_id,ap.version FROM qintopia_agent_os.welcome_approvals ap JOIN qintopia_agent_os.welcome_targets t ON t.id=ap.target_id WHERE ap.id=$1 AND t.source_instance=$2 AND t.property_id=$3 FOR UPDATE OF ap")
            .bind(approval).bind(&actor.source).bind(&actor.property).fetch_one(&mut *tx).await?;
        authorize(&mut tx, actor, "review", Some(row.get("target_id"))).await?;
        let input = json!(["revoke_approval", approval, expected]);
        if let Some(result) = operation_start(&mut tx, actor, operation, &input).await? {
            return Ok(result);
        }
        ensure!(
            row.get::<i64, _>("version") == expected,
            "approval_version_conflict"
        );
        sqlx::query("UPDATE qintopia_agent_os.welcome_approvals SET revoked_at=now(),version=version+1 WHERE id=$1").bind(approval).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.welcome_actions SET status='cancelled',version=version+1 WHERE approval_id=$1 AND status IN ('prepared','claimed','retryable')").bind(approval).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items w SET status='cancelled',claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL FROM qintopia_agent_os.welcome_actions a WHERE a.work_item_id=w.id AND a.approval_id=$1 AND a.status='cancelled'").bind(approval).execute(&mut *tx).await?;
        let result = json!({"approval_ref":approval,"version":expected+1});
        operation_finish(&mut tx, actor, operation, &input, &result).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn grant(&self, actor: &Actor, request: &GrantRequest) -> Result<Value> {
        ensure!(
            matches!(
                request.action.as_str(),
                "identity" | "appoint" | "review" | "publish"
            ) && request.expires > Utc::now(),
            "invalid_grant"
        );
        let mut tx = self.pool.begin().await?;
        authorize(&mut tx, actor, "appoint", None).await?;
        let input = json!([
            "grant",
            request.person,
            request.target,
            request.action,
            request.membership,
            request.expires
        ]);
        if let Some(r) = operation_start(&mut tx, actor, request.operation, &input).await? {
            return Ok(r);
        }
        if let Some(target) = request.target {
            let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_targets WHERE id=$1 AND source_instance=$2 AND property_id=$3)")
                .bind(target).bind(&actor.source).bind(&actor.property).fetch_one(&mut *tx).await?;
            ensure!(ok, "target_scope_forbidden");
        }
        let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_grants(person_id,membership_id,source_instance,property_id,target_id,action,expires_at,appointed_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id")
            .bind(request.person).bind(request.membership).bind(&actor.source).bind(&actor.property).bind(request.target).bind(&request.action).bind(request.expires).bind(actor.person).fetch_one(&mut *tx).await?;
        let result = json!({"grant_ref":id,"version":1});
        operation_finish(&mut tx, actor, request.operation, &input, &result).await?;
        tx.commit().await?;
        self.reconcile_scope(&actor.source, &actor.property).await?;
        Ok(result)
    }

    pub async fn revoke_grant(
        &self,
        actor: &Actor,
        operation: Uuid,
        grant: Uuid,
        expected: i64,
    ) -> Result<Value> {
        let mut tx = self.pool.begin().await?;
        authorize(&mut tx, actor, "appoint", None).await?;
        let input = json!(["revoke_grant", grant, expected]);
        if let Some(r) = operation_start(&mut tx, actor, operation, &input).await? {
            return Ok(r);
        }
        let n=sqlx::query("UPDATE qintopia_agent_os.welcome_grants SET revoked_at=now(),version=version+1 WHERE id=$1 AND source_instance=$2 AND property_id=$3 AND version=$4")
            .bind(grant).bind(&actor.source).bind(&actor.property).bind(expected).execute(&mut *tx).await?.rows_affected();
        ensure!(n == 1, "grant_version_or_scope_conflict");
        sqlx::query("UPDATE qintopia_agent_os.welcome_actions a SET status='cancelled',version=a.version+1 WHERE (a.publish_grant_id=$1 OR a.approval_id IN (SELECT id FROM qintopia_agent_os.welcome_approvals WHERE grant_id=$1)) AND a.status IN ('prepared','claimed','retryable')")
            .bind(grant).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items w SET status='cancelled',claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL FROM qintopia_agent_os.welcome_actions a WHERE a.work_item_id=w.id AND (a.publish_grant_id=$1 OR a.approval_id IN (SELECT id FROM qintopia_agent_os.welcome_approvals WHERE grant_id=$1)) AND a.status='cancelled' AND w.status<>'cancelled'")
            .bind(grant).execute(&mut *tx).await?;
        let result = json!({"grant_ref":grant,"version":expected+1});
        operation_finish(&mut tx, actor, operation, &input, &result).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn approve(&self, actor: &Actor, request: &ReviewRequest) -> Result<Value> {
        ensure!(
            matches!(request.phase.as_str(), "preview" | "formal" | "internal"),
            "invalid_phase"
        );
        let mut tx = self.pool.begin().await?;
        let (grant, grant_version) =
            authorize(&mut tx, actor, "review", Some(request.target)).await?;
        let input = json!([
            "approve",
            request.artifact,
            request.target,
            request.target_version,
            request.phase,
            request.content_hash
        ]);
        if let Some(r) = operation_start(&mut tx, actor, request.operation, &input).await? {
            return Ok(r);
        }
        let row=sqlx::query("SELECT a.content_hash,b.case_id,c.version,b.case_version,t.version AS target_version FROM qintopia_agent_os.artifacts a JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=a.id JOIN qintopia_agent_os.welcome_cases c ON c.id=b.case_id JOIN qintopia_agent_os.welcome_targets t ON t.id=$2 AND t.source_instance=c.source_instance AND t.property_id=c.property_id WHERE a.id=$1 AND b.revoked_at IS NULL AND c.source_instance=$3 AND c.property_id=$4 FOR UPDATE OF a,b,c,t")
            .bind(request.artifact).bind(request.target).bind(&actor.source).bind(&actor.property).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<i64, _>("target_version") == request.target_version
                && row.get::<i64, _>("case_version") == row.get::<i64, _>("version")
                && row.get::<Option<String>, _>("content_hash").as_deref()
                    == Some(&request.content_hash),
            "review_version_conflict"
        );
        ensure!(
            super::state::evaluate_tx(&mut tx, row.get("case_id"))
                .await?
                .card_ready,
            "case_not_ready"
        );
        let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_approvals(artifact_id,target_id,target_version,phase,content_hash,grant_id,grant_version,approved_by) VALUES ($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id")
            .bind(request.artifact).bind(request.target).bind(request.target_version).bind(&request.phase).bind(&request.content_hash).bind(grant).bind(grant_version).bind(actor.person).fetch_one(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.artifacts SET review_status='approved',reviewed_at=now(),reviewed_by=$2 WHERE id=$1")
            .bind(request.artifact).bind(actor.person.to_string()).execute(&mut *tx).await?;
        let result = json!({"approval_ref":id,"version":1});
        operation_finish(&mut tx, actor, request.operation, &input, &result).await?;
        tx.commit().await?;
        self.reconcile_scope(&actor.source, &actor.property).await?;
        Ok(result)
    }
}
