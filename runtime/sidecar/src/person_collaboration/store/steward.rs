//! Steward-owned temporary business review, separate from organization management.
use super::{foundation::*, Actor, Audience, Store};
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct ReviewDelegationCommand {
    pub operation_id: Uuid,
    pub scope: Uuid,
    pub expected_id: Option<Uuid>,
    pub delegate: Option<Uuid>,
    pub valid_from: Option<DateTime<Utc>>,
    pub valid_until: Option<DateTime<Utc>>,
}

impl Store {
    pub(crate) async fn welcome_assert_scope(&self, actor: &Actor, scope: Uuid) -> Result<()> {
        if self.foundation_assert_scope(actor, scope).await.is_ok() {
            return Ok(());
        }
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        if let Some((_, _, bound)) = actor.gateway {
            ensure!(scope == bound, "gateway_scope_mismatch");
        }
        ensure!(
            delegated_scopes_in(&self.pool, &self.tenant, &mut tx, actor.person)
                .await?
                .iter()
                .any(|s| s["scope"] == json!(scope)),
            "scope_access_denied"
        );
        Ok(())
    }

    pub(super) async fn resident_candidates_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: Uuid,
    ) -> Result<Vec<Value>> {
        let now = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut **tx)
            .await?;
        let policy = load_policy(tx, &self.tenant, now).await?;
        let audience: Audience = serde_json::from_value(
            json!({"groups":[],"people":[],"residents":"current","reply":"autonomous","proactive":"denied","reviewer":null,"visibility":"public","topics":""}),
        )?;
        let resolved = self
            .resolve_audience(tx, &policy, scope, &audience, now)
            .await?;
        Ok(resolved
            .people
            .into_iter()
            .filter(|p| p["status"] == "current")
            .collect())
    }

    pub(crate) async fn review_delegation_state(
        &self,
        actor: &Actor,
        scope: Uuid,
    ) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        if let Some((_, _, bound)) = actor.gateway {
            ensure!(bound == scope, "gateway_scope_mismatch");
        }
        let designate = authorize_current(
            &mut tx,
            &self.tenant,
            actor.person,
            scope,
            "erhua",
            "community_service",
            "designate",
        )
        .await?;
        ensure!(designate.status != "denied", "scope_access_denied");
        let current: Option<Value> = sqlx::query_scalar("SELECT jsonb_build_object('id',d.id,'delegate',d.delegate_person_id,'label',coalesce(p.preferred_name,p.display_name),'valid_from',d.valid_from,'valid_until',d.valid_until) FROM qintopia_agent_os.collaboration_review_delegations d JOIN qintopia_identity.persons p ON p.id=d.delegate_person_id WHERE d.tenant_key=$1 AND d.scope_id=$2 AND d.owner_person_id=$3 AND d.revoked_at IS NULL")
            .bind(&self.tenant).bind(scope).bind(actor.person).fetch_optional(&mut *tx).await?;
        let candidates = self
            .resident_candidates_in(&mut tx, scope)
            .await?
            .into_iter()
            .filter(|p| p["person_ref"] != json!(actor.person))
            .collect::<Vec<_>>();
        let valid = content_reviewer(&self.pool, &self.tenant, &mut tx, scope, actor.person).await;
        let reason = valid
            .as_ref()
            .err()
            .map(super::super::foundation_server::error_code);
        Ok(
            json!({"current":current,"candidates":candidates,"can_designate":designate.status=="autonomous","reason":reason,"purpose":"resident_welcome_content_review"}),
        )
    }

    pub(crate) async fn review_delegation_change(
        &self,
        actor: &Actor,
        cmd: &ReviewDelegationCommand,
    ) -> Result<Value> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        if let Some((_, _, scope)) = actor.gateway {
            ensure!(scope == cmd.scope, "gateway_scope_mismatch");
        }
        let designation = authorize_current(
            &mut tx,
            &self.tenant,
            actor.person,
            cmd.scope,
            "erhua",
            "community_service",
            "designate",
        )
        .await?;
        ensure!(
            designation.status == "autonomous",
            "designation_authority_required"
        );
        let hash = super::super::digest(&serde_json::to_vec(cmd)?);
        if let Some(result) =
            receipt(&mut tx, &self.tenant, actor.person, cmd.operation_id, &hash).await?
        {
            return Ok(json!({"replayed":true,"result":result}));
        }
        let current:Option<Uuid>=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_review_delegations WHERE tenant_key=$1 AND scope_id=$2 AND owner_person_id=$3 AND revoked_at IS NULL")
            .bind(&self.tenant).bind(cmd.scope).bind(actor.person).fetch_optional(&mut *tx).await?;
        ensure!(current == cmd.expected_id, "delegation_version_conflict");
        let mut id = None;
        if let Some(person) = cmd.delegate {
            ensure!(person != actor.person, "delegate_must_be_another_resident");
            let candidates = self.resident_candidates_in(&mut tx, cmd.scope).await?;
            ensure!(
                candidates.iter().any(|p| p["person_ref"] == json!(person)),
                "current_resident_required"
            );
            let review = authorize_current(
                &mut tx,
                &self.tenant,
                actor.person,
                cmd.scope,
                "erhua",
                "community_service",
                "review",
            )
            .await?;
            ensure!(review.status == "autonomous", "review_authority_required");
            let start = cmd.valid_from.unwrap_or(now).max(now);
            let end = cmd
                .valid_until
                .ok_or_else(|| anyhow::anyhow!("proxy_requires_expiry"))?;
            ensure!(end > start, "invalid_effective_interval");
            sqlx::query("UPDATE qintopia_agent_os.collaboration_review_delegations SET revoked_at=clock_timestamp() WHERE id=$1").bind(current).execute(&mut *tx).await?;
            let new_id = Uuid::new_v4();
            sqlx::query("INSERT INTO qintopia_agent_os.collaboration_review_delegations(id,tenant_key,scope_id,owner_person_id,delegate_person_id,designate_grant_id,review_grant_id,valid_from,valid_until) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)")
                .bind(new_id).bind(&self.tenant).bind(cmd.scope).bind(actor.person).bind(person).bind(designation.grant_id).bind(review.grant_id).bind(start).bind(end).execute(&mut *tx).await?;
            id = Some(new_id);
        } else {
            ensure!(current.is_some(), "delegation_not_found");
            sqlx::query("UPDATE qintopia_agent_os.collaboration_review_delegations SET revoked_at=clock_timestamp() WHERE id=$1").bind(current).execute(&mut *tx).await?;
        }
        let result =
            json!({"status":if id.is_some(){"saved"}else{"revoked"},"id":id,"previous_id":current});
        record_receipt(
            &mut tx,
            &self.tenant,
            actor.person,
            cmd.operation_id,
            &hash,
            &result,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }
}

pub(crate) async fn delegated_scopes_in(
    pool: &sqlx::PgPool,
    tenant: &str,
    tx: &mut Transaction<'_, Postgres>,
    person: Uuid,
) -> Result<Vec<Value>> {
    let rows=sqlx::query("SELECT d.scope_id,d.owner_person_id,s.label,d.valid_until FROM qintopia_agent_os.collaboration_review_delegations d JOIN qintopia_agent_os.collaboration_scopes s ON s.id=d.scope_id AND s.tenant_key=d.tenant_key WHERE d.tenant_key=$1 AND d.delegate_person_id=$2 AND d.revoked_at IS NULL AND s.status='active'")
        .bind(tenant).bind(person).fetch_all(&mut **tx).await?;
    let mut result = vec![];
    for row in rows {
        let scope = row.get("scope_id");
        if content_reviewer(pool, tenant, tx, scope, row.get("owner_person_id"))
            .await
            .is_ok_and(|(p, _)| p == person)
        {
            result.push(json!({"scope":scope,"label":row.get::<String,_>("label"),"valid_until":row.get::<DateTime<Utc>,_>("valid_until")}));
        }
    }
    Ok(result)
}

/// The returned grant/collaboration fingerprint binds approvals to this exact delegation.
/// A lapsed delegation blocks review until the owner explicitly cancels or replaces it.
pub(crate) async fn content_reviewer(
    pool: &sqlx::PgPool,
    tenant: &str,
    tx: &mut Transaction<'_, Postgres>,
    scope: Uuid,
    owner: Uuid,
) -> Result<(Uuid, Authority)> {
    let store = Store {
        pool: pool.clone(),
        tenant: tenant.into(),
    };
    let mut review = authorize_current(
        tx,
        &store.tenant,
        owner,
        scope,
        "erhua",
        "community_service",
        "review",
    )
    .await?;
    ensure!(review.status == "autonomous", "review_authority_required");
    let row=sqlx::query("SELECT * FROM qintopia_agent_os.collaboration_review_delegations WHERE tenant_key=$1 AND scope_id=$2 AND owner_person_id=$3 AND revoked_at IS NULL")
        .bind(&store.tenant).bind(scope).bind(owner).fetch_optional(&mut **tx).await?;
    if let Some(row) = row {
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut **tx)
            .await?;
        if row.get::<DateTime<Utc>, _>("valid_from") > now {
            return Ok((owner, review));
        }
        ensure!(
            row.get::<DateTime<Utc>, _>("valid_until") > now,
            "review_delegation_expired"
        );
        let designate = authorize_current(
            tx,
            &store.tenant,
            owner,
            scope,
            "erhua",
            "community_service",
            "designate",
        )
        .await?;
        ensure!(
            designate.status == "autonomous"
                && designate.grant_id == Some(row.get("designate_grant_id"))
                && review.grant_id == Some(row.get("review_grant_id")),
            "review_delegation_authority_revoked"
        );
        let person: Uuid = row.get("delegate_person_id");
        ensure!(
            store
                .resident_candidates_in(tx, scope)
                .await?
                .iter()
                .any(|p| p["person_ref"] == json!(person)),
            "delegate_not_current_resident"
        );
        review.collaboration_id = Some(row.get("id"));
        return Ok((person, review));
    }
    Ok((owner, review))
}
