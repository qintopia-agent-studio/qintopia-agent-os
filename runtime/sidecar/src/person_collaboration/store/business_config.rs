//! Shared PMS scope configuration, guarded by current organizational management rights.
use super::{business, Actor, Store};
use crate::person_collaboration::{digest, model::PermissionMode};
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

#[derive(Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct BusinessConfigCommand {
    pub operation_id: Uuid,
    pub expected_version: i64,
    pub change: BusinessConfigChange,
}

#[derive(Debug, Serialize, Deserialize)]
#[serde(tag = "kind", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum BusinessConfigChange {
    CreateBinding {
        scope: Uuid,
        source: String,
        property: String,
    },
    DisableBinding {
        binding: Uuid,
        expected_binding_version: i64,
    },
    GrantOperation {
        binding: Uuid,
        authority_grant: Uuid,
        operation: String,
        valid_until: Option<DateTime<Utc>>,
    },
    RevokeOperation {
        grant: Uuid,
    },
}

fn valid_source_key(value: &str) -> bool {
    !value.is_empty()
        && value.len() <= 160
        && value.trim() == value
        && !value.chars().any(char::is_control)
}

impl Store {
    pub(crate) async fn business_configuration_state(&self, actor: &Actor) -> Result<Value> {
        let (mut tx, version, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let policy = self.policy(&mut tx, now).await?;
        let rows = sqlx::query("SELECT id,scope_id,source_instance,property_id,active,version FROM qintopia_agent_os.business_property_bindings WHERE tenant_key=$1 ORDER BY scope_id,property_id LIMIT 257")
            .bind(&self.tenant).fetch_all(&mut *tx).await?;
        ensure!(rows.len() <= 256, "business_configuration_too_large");
        let mut bindings = Vec::new();
        for row in rows {
            let scope: Uuid = row.get("scope_id");
            let can_read = policy
                .manager(actor.person, scope, "anan", "hospitality", "read_business")
                .is_some();
            let can_execute = policy
                .manager(
                    actor.person,
                    scope,
                    "anan",
                    "hospitality",
                    "execute_business",
                )
                .is_some();
            if !(can_read || can_execute) {
                continue;
            }
            let binding: Uuid = row.get("id");
            let grants = sqlx::query("SELECT o.id,o.authority_grant_id,o.operation_key,o.valid_until,o.revoked_at,g.action_key FROM qintopia_agent_os.business_operation_grants o JOIN qintopia_agent_os.collaboration_grants g ON g.tenant_key=o.tenant_key AND g.id=o.authority_grant_id WHERE o.tenant_key=$1 AND o.binding_id=$2 ORDER BY o.created_at,o.id LIMIT 257")
                .bind(&self.tenant).bind(binding).fetch_all(&mut *tx).await?;
            ensure!(grants.len() <= 256, "business_configuration_too_large");
            bindings.push(json!({
                "id":binding,"scope":scope,"source":row.get::<String, _>("source_instance"),
                "property":row.get::<String, _>("property_id"),"active":row.get::<bool, _>("active"),
                "version":row.get::<i64, _>("version"),
                "grants":grants.iter().filter(|g| {
                    match g.get::<String, _>("action_key").as_str() {
                        "read_business" => can_read,
                        "execute_business" => can_execute,
                        _ => false,
                    }
                }).map(|g| json!({
                    "id":g.get::<Uuid, _>("id"),"authority_grant":g.get::<Uuid, _>("authority_grant_id"),
                    "operation":g.get::<String, _>("operation_key"),
                    "valid_until":g.get::<Option<DateTime<Utc>>, _>("valid_until"),
                    "revoked":g.get::<Option<DateTime<Utc>>, _>("revoked_at").is_some()
                })).collect::<Vec<_>>()
            }));
        }
        Ok(json!({"version":version,"bindings":bindings}))
    }

    /// Invoked by an authenticated management surface; never from an Agent PMS tool.
    pub(crate) async fn business_configure(
        &self,
        actor: &Actor,
        command: &BusinessConfigCommand,
        apply: bool,
    ) -> Result<Value> {
        let (mut tx, version, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let hash = digest(&serde_json::to_vec(command)?);
        if let Some(row) = sqlx::query("SELECT actor_identity_id,request_hash,result FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1 AND id=$2")
            .bind(&self.tenant).bind(command.operation_id).fetch_optional(&mut *tx).await? {
            ensure!(
                row.get::<Uuid, _>("actor_identity_id") == actor.link
                    && row.get::<String, _>("request_hash") == hash,
                "idempotency_conflict"
            );
            ensure!(actor.session_hash.is_none(), "command_already_processed_refresh_state");
            let mut result: Value = row.get("result");
            result["replayed"] = json!(true);
            return Ok(result);
        }
        ensure!(
            version == command.expected_version,
            "configuration_version_conflict"
        );
        let policy = self.policy(&mut tx, now).await?;
        let change = match &command.change {
            BusinessConfigChange::CreateBinding {
                scope,
                source,
                property,
            } => {
                ensure!(
                    valid_source_key(source) && valid_source_key(property),
                    "invalid_business_binding"
                );
                ensure!(
                    policy
                        .manager(actor.person, *scope, "anan", "hospitality", "read_business")
                        .is_some()
                        || policy
                            .manager(
                                actor.person,
                                *scope,
                                "anan",
                                "hospitality",
                                "execute_business"
                            )
                            .is_some(),
                    "management_denied"
                );
                let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND id=$2 AND status='active')")
                    .bind(&self.tenant).bind(scope).fetch_one(&mut *tx).await?;
                ensure!(active, "business_scope_unavailable");
                let existing = sqlx::query("SELECT id,active,version FROM qintopia_agent_os.business_property_bindings WHERE tenant_key=$1 AND scope_id=$2 AND source_instance=$3 AND property_id=$4 FOR UPDATE")
                    .bind(&self.tenant).bind(scope).bind(source).bind(property).fetch_optional(&mut *tx).await?;
                let id = if let Some(row) = existing {
                    ensure!(!row.get::<bool, _>("active"), "business_binding_exists");
                    let id: Uuid = row.get("id");
                    sqlx::query("UPDATE qintopia_agent_os.business_property_bindings SET active=true,version=version+1 WHERE tenant_key=$1 AND id=$2")
                        .bind(&self.tenant).bind(id).execute(&mut *tx).await?;
                    id
                } else {
                    sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) VALUES($1,$2,$3,$4) RETURNING id")
                        .bind(&self.tenant).bind(scope).bind(source).bind(property).fetch_one(&mut *tx).await?
                };
                json!({"kind":"binding","binding":id,"active":true})
            }
            BusinessConfigChange::DisableBinding {
                binding,
                expected_binding_version,
            } => {
                let row = sqlx::query("SELECT scope_id,version,active FROM qintopia_agent_os.business_property_bindings WHERE tenant_key=$1 AND id=$2 FOR UPDATE")
                    .bind(&self.tenant).bind(binding).fetch_optional(&mut *tx).await?
                    .ok_or_else(|| anyhow::anyhow!("business_binding_unavailable"))?;
                let scope: Uuid = row.get("scope_id");
                ensure!(
                    policy
                        .manager(actor.person, scope, "anan", "hospitality", "read_business")
                        .is_some()
                        && policy
                            .manager(
                                actor.person,
                                scope,
                                "anan",
                                "hospitality",
                                "execute_business"
                            )
                            .is_some(),
                    "management_denied"
                );
                ensure!(
                    row.get::<bool, _>("active")
                        && row.get::<i64, _>("version") == *expected_binding_version,
                    "business_binding_changed"
                );
                sqlx::query("UPDATE qintopia_agent_os.business_property_bindings SET active=false,version=version+1 WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(binding).execute(&mut *tx).await?;
                sqlx::query("UPDATE qintopia_agent_os.business_operation_grants SET revoked_at=$3 WHERE tenant_key=$1 AND binding_id=$2 AND revoked_at IS NULL")
                    .bind(&self.tenant).bind(binding).bind(now).execute(&mut *tx).await?;
                json!({"kind":"binding","binding":binding,"active":false,"version":expected_binding_version+1})
            }
            BusinessConfigChange::GrantOperation {
                binding,
                authority_grant,
                operation,
                valid_until,
            } => {
                let spec = business::operation(operation)?;
                let action = spec["action"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("unsupported_business_operation"))?;
                ensure!(
                    valid_until.is_none_or(|until| until > now),
                    "invalid_effective_window"
                );
                let scope: Uuid = sqlx::query_scalar("SELECT scope_id FROM qintopia_agent_os.business_property_bindings WHERE tenant_key=$1 AND id=$2 AND active FOR SHARE")
                    .bind(&self.tenant).bind(binding).fetch_optional(&mut *tx).await?
                    .ok_or_else(|| anyhow::anyhow!("business_binding_unavailable"))?;
                ensure!(
                    policy
                        .manager(actor.person, scope, "anan", "hospitality", action)
                        .is_some(),
                    "management_denied"
                );
                let target = policy
                    .grants
                    .iter()
                    .find(|g| g.id == *authority_grant)
                    .ok_or_else(|| anyhow::anyhow!("business_authority_denied"))?;
                ensure!(
                    target.person != actor.person
                        && target.agent == "anan"
                        && target.domain == "hospitality"
                        && target.action == action
                        && target.mode == PermissionMode::Autonomous
                        && policy.effective(target)
                        && policy.in_scope(scope, target.scope, target.descendants),
                    "business_authority_denied"
                );
                let existing: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_operation_grants WHERE tenant_key=$1 AND authority_grant_id=$2 AND binding_id=$3 AND operation_key=$4 AND revoked_at IS NULL)")
                    .bind(&self.tenant).bind(authority_grant).bind(binding).bind(operation).fetch_one(&mut *tx).await?;
                ensure!(!existing, "business_operation_grant_exists");
                let id: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_operation_grants(tenant_key,authority_grant_id,binding_id,operation_key,issued_by,valid_until) VALUES($1,$2,$3,$4,$5,$6) RETURNING id")
                    .bind(&self.tenant).bind(authority_grant).bind(binding).bind(operation).bind(actor.person).bind(valid_until).fetch_one(&mut *tx).await?;
                json!({"kind":"operation_grant","grant":id,"binding":binding,"operation":operation,"active":true})
            }
            BusinessConfigChange::RevokeOperation { grant } => {
                let row = sqlx::query("SELECT b.scope_id,g.action_key FROM qintopia_agent_os.business_operation_grants o JOIN qintopia_agent_os.business_property_bindings b ON b.tenant_key=o.tenant_key AND b.id=o.binding_id JOIN qintopia_agent_os.collaboration_grants g ON g.tenant_key=o.tenant_key AND g.id=o.authority_grant_id WHERE o.tenant_key=$1 AND o.id=$2 AND o.revoked_at IS NULL FOR UPDATE OF o")
                    .bind(&self.tenant).bind(grant).fetch_optional(&mut *tx).await?
                    .ok_or_else(|| anyhow::anyhow!("business_operation_revoked"))?;
                ensure!(
                    policy
                        .manager(
                            actor.person,
                            row.get("scope_id"),
                            "anan",
                            "hospitality",
                            row.get::<String, _>("action_key").as_str()
                        )
                        .is_some(),
                    "management_denied"
                );
                let count = sqlx::query("WITH RECURSIVE affected(id) AS (SELECT id FROM qintopia_agent_os.business_operation_grants WHERE tenant_key=$1 AND id=$2 UNION SELECT child.id FROM qintopia_agent_os.business_operation_grants child JOIN affected parent ON child.parent_id=parent.id WHERE child.tenant_key=$1) UPDATE qintopia_agent_os.business_operation_grants SET revoked_at=$3 WHERE tenant_key=$1 AND id IN(SELECT id FROM affected) AND revoked_at IS NULL")
                    .bind(&self.tenant).bind(grant).bind(now).execute(&mut *tx).await?.rows_affected();
                json!({"kind":"operation_grant","grant":grant,"active":false,"revoked_count":count})
            }
        };
        let result = json!({"persisted":apply,"version":if apply {version+1} else {version},"change":change,"replayed":false});
        if !apply {
            tx.rollback().await?;
            return Ok(result);
        }
        sqlx::query("UPDATE qintopia_agent_os.collaboration_tenants SET version=version+1 WHERE tenant_key=$1")
            .bind(&self.tenant).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_commands(id,tenant_key,actor_identity_id,actor_person_id,request_hash,expected_version,result) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(command.operation_id).bind(&self.tenant).bind(actor.link).bind(actor.person).bind(&hash).bind(version).bind(&result).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.tool_invocation_audit(profile_id,tool_name,purpose,input_summary,output_summary,risk_level) VALUES('foundation-admin','collaboration.business.configure',$1,$2,$3,'high')")
            .bind(if self.is_live() {"live_configuration"} else {"synthetic_configuration"})
            .bind(json!({"command_ref":command.operation_id,"actor_ref":actor.person,"identity_version":actor.identity_version,"request_hash":hash}))
            .bind(json!({"version":version+1,"persisted":true,"external_effects":false})).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
}
