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
    RegisterAccount {
        gateway: String,
        source_link: Uuid,
        label: String,
    },
    DisableAccount {
        account: Uuid,
        expected_account_version: i64,
    },
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
    GrantAccountOperation {
        binding: Uuid,
        account: Uuid,
        role: String,
        operation: String,
        valid_until: Option<DateTime<Utc>>,
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
            let grants = sqlx::query("SELECT o.id,o.authority_grant_id,o.work_account_id,o.account_role,o.operation_key,o.valid_until,o.revoked_at FROM qintopia_agent_os.business_operation_grants o WHERE o.tenant_key=$1 AND o.binding_id=$2 ORDER BY o.created_at,o.id LIMIT 257")
                .bind(&self.tenant).bind(binding).fetch_all(&mut *tx).await?;
            ensure!(grants.len() <= 256, "business_configuration_too_large");
            bindings.push(json!({
                "id":binding,"scope":scope,"source":row.get::<String, _>("source_instance"),
                "property":row.get::<String, _>("property_id"),"active":row.get::<bool, _>("active"),
                "version":row.get::<i64, _>("version"),
                "grants":grants.iter().filter(|g| {
                    match business::operation(&g.get::<String,_>("operation_key")).ok().and_then(|v|v["action"].as_str().map(str::to_owned)).as_deref() {
                        Some("read_business") => can_read,
                        Some("execute_business") => can_execute,
                        _ => false,
                    }
                }).map(|g| json!({
                    "id":g.get::<Uuid, _>("id"),"authority_grant":g.get::<Uuid, _>("authority_grant_id"),
                    "operation":g.get::<String, _>("operation_key"),
                    "work_account":g.get::<Option<Uuid>, _>("work_account_id"),
                    "role":g.get::<Option<String>, _>("account_role"),
                    "valid_until":g.get::<Option<DateTime<Utc>>, _>("valid_until"),
                    "revoked":g.get::<Option<DateTime<Utc>>, _>("revoked_at").is_some()
                })).collect::<Vec<_>>()
            }));
        }
        let accounts=sqlx::query("SELECT w.id,w.label,w.active,w.version,w.gateway_key,g.scope_id FROM qintopia_identity.work_accounts w JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=w.tenant_key AND g.gateway_key=w.gateway_key WHERE w.tenant_key=$1 ORDER BY w.label,w.id LIMIT 257")
            .bind(&self.tenant).fetch_all(&mut *tx).await?;
        ensure!(accounts.len() <= 256, "business_configuration_too_large");
        let accounts:Vec<_>=accounts.iter().filter(|r| policy.manager(actor.person,r.get("scope_id"),"anan","hospitality","read_business").is_some() || policy.manager(actor.person,r.get("scope_id"),"anan","hospitality","execute_business").is_some())
            .map(|r|json!({"id":r.get::<Uuid,_>("id"),"label":r.get::<String,_>("label"),"active":r.get::<bool,_>("active"),"version":r.get::<i64,_>("version"),"gateway":r.get::<String,_>("gateway_key"),"scope":r.get::<Uuid,_>("scope_id")})).collect();
        let observed=sqlx::query("SELECT l.id,g.gateway_key,g.scope_id,coalesce(nullif(l.adapter_metadata->>'display_name',''),'待核对工作账号') AS label FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type WHERE g.tenant_key=$1 AND g.active AND l.person_id IS NULL AND l.status='pending' AND l.adapter_metadata ? 'first_observation_ref' AND NOT EXISTS(SELECT 1 FROM qintopia_identity.work_accounts w WHERE w.tenant_key=$1 AND w.source_link_id=l.id AND w.active) ORDER BY g.gateway_key,l.id LIMIT 257")
            .bind(&self.tenant).fetch_all(&mut *tx).await?;
        ensure!(observed.len() <= 256, "business_configuration_too_large");
        let observed:Vec<_>=observed.iter().filter(|r| policy.manager(actor.person,r.get("scope_id"),"anan","hospitality","read_business").is_some() || policy.manager(actor.person,r.get("scope_id"),"anan","hospitality","execute_business").is_some())
            .map(|r|json!({"source_link":r.get::<Uuid,_>("id"),"gateway":r.get::<String,_>("gateway_key"),"scope":r.get::<Uuid,_>("scope_id"),"label":r.get::<String,_>("label")})).collect();
        let can_manage = policy.scopes.iter().any(|s| {
            s.active
                && (policy
                    .manager(actor.person, s.id, "anan", "hospitality", "read_business")
                    .is_some()
                    || policy
                        .manager(
                            actor.person,
                            s.id,
                            "anan",
                            "hospitality",
                            "execute_business",
                        )
                        .is_some())
        });
        let catalog: Value = serde_json::from_str(include_str!(
            "../../../../../skills/pms-operations/operations.json"
        ))?;
        Ok(
            json!({"version":version,"can_manage":can_manage,"bindings":bindings,"accounts":accounts,"observed":observed,"operations":catalog["operations"]}),
        )
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
            BusinessConfigChange::RegisterAccount {
                gateway,
                source_link,
                label,
            } => {
                ensure!(
                    !label.trim().is_empty() && label.chars().count() <= 100,
                    "invalid_label"
                );
                let row=sqlx::query("SELECT g.scope_id,g.version AS gateway_version,l.version,l.adapter_metadata FROM qintopia_identity.person_identity_gateways g JOIN qintopia_identity.source_identity_links l ON l.namespace=g.namespace AND l.subject_type=g.subject_type JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key WHERE g.tenant_key=$1 AND g.gateway_key=$2 AND l.id=$3 AND g.active AND s.status='active' AND l.person_id IS NULL AND l.status='pending' AND l.adapter_metadata ? 'first_observation_ref' AND (SELECT count(*) FROM qintopia_identity.person_identity_gateways x WHERE x.namespace=g.namespace AND x.subject_type=g.subject_type AND x.active)=1 FOR SHARE OF g,l,s")
                    .bind(&self.tenant).bind(gateway).bind(source_link).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("observed_work_account_required"))?;
                let scope: Uuid = row.get("scope_id");
                ensure!(
                    policy
                        .manager(actor.person, scope, "anan", "hospitality", "read_business")
                        .is_some()
                        || policy
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
                let metadata: Value = row.get("adapter_metadata");
                let evidence = metadata["first_observation_ref"]
                    .as_str()
                    .and_then(|v| Uuid::parse_str(v).ok())
                    .ok_or_else(|| anyhow::anyhow!("trusted_observation_required"))?;
                let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.work_accounts(tenant_key,source_link_id,source_version,gateway_key,gateway_version,label,verified_by,evidence_ref) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(tenant_key,source_link_id) DO UPDATE SET active=true,version=work_accounts.version+1,source_version=EXCLUDED.source_version,gateway_version=EXCLUDED.gateway_version,label=EXCLUDED.label,verified_by=EXCLUDED.verified_by,evidence_ref=EXCLUDED.evidence_ref RETURNING id")
                    .bind(&self.tenant).bind(source_link).bind(row.get::<i64,_>("version")).bind(gateway).bind(row.get::<i64,_>("gateway_version")).bind(label).bind(actor.person).bind(evidence).fetch_one(&mut *tx).await?;
                sqlx::query("UPDATE qintopia_agent_os.business_operation_grants SET revoked_at=$3 WHERE tenant_key=$1 AND work_account_id=$2 AND revoked_at IS NULL")
                    .bind(&self.tenant).bind(id).bind(now).execute(&mut *tx).await?;
                json!({"kind":"work_account","account":id,"active":true,"authority_granted":false})
            }
            BusinessConfigChange::DisableAccount {
                account,
                expected_account_version,
            } => {
                let row=sqlx::query("SELECT w.version,w.active,g.scope_id FROM qintopia_identity.work_accounts w JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=w.tenant_key AND g.gateway_key=w.gateway_key WHERE w.tenant_key=$1 AND w.id=$2 FOR UPDATE OF w")
                    .bind(&self.tenant).bind(account).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("work_account_unavailable"))?;
                let scope: Uuid = row.get("scope_id");
                ensure!(
                    policy
                        .manager(actor.person, scope, "anan", "hospitality", "read_business")
                        .is_some()
                        || policy
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
                let operations:Vec<String>=sqlx::query_scalar("SELECT DISTINCT operation_key FROM qintopia_agent_os.business_operation_grants WHERE tenant_key=$1 AND work_account_id=$2 AND revoked_at IS NULL")
                    .bind(&self.tenant).bind(account).fetch_all(&mut *tx).await?;
                for operation in operations {
                    let spec = business::operation(&operation)?;
                    let action = spec["action"]
                        .as_str()
                        .ok_or_else(|| anyhow::anyhow!("unsupported_business_operation"))?;
                    ensure!(
                        policy
                            .manager(actor.person, scope, "anan", "hospitality", action)
                            .is_some(),
                        "management_denied"
                    );
                }
                ensure!(
                    row.get::<bool, _>("active")
                        && row.get::<i64, _>("version") == *expected_account_version,
                    "work_account_changed_or_revoked"
                );
                sqlx::query("UPDATE qintopia_identity.work_accounts SET active=false,version=version+1 WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(account).execute(&mut *tx).await?;
                sqlx::query("UPDATE qintopia_agent_os.business_operation_grants SET revoked_at=$3 WHERE tenant_key=$1 AND work_account_id=$2 AND revoked_at IS NULL")
                    .bind(&self.tenant).bind(account).bind(now).execute(&mut *tx).await?;
                json!({"kind":"work_account","account":account,"active":false})
            }
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
            BusinessConfigChange::GrantAccountOperation {
                binding,
                account,
                role,
                operation,
                valid_until,
            } => {
                ensure!(
                    matches!(role.as_str(), "operator" | "admin"),
                    "invalid_business_role"
                );
                let spec = business::operation(operation)?;
                let action = spec["action"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("unsupported_business_operation"))?;
                ensure!(
                    valid_until.is_none_or(|until| until > now),
                    "invalid_effective_window"
                );
                let binding_row=sqlx::query("SELECT scope_id FROM qintopia_agent_os.business_property_bindings WHERE tenant_key=$1 AND id=$2 AND active FOR SHARE")
                    .bind(&self.tenant).bind(binding).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("business_binding_unavailable"))?;
                let scope: Uuid = binding_row.get("scope_id");
                let manager = policy
                    .manager(actor.person, scope, "anan", "hospitality", action)
                    .ok_or_else(|| anyhow::anyhow!("management_denied"))?;
                let account_row=sqlx::query("SELECT w.version,w.gateway_version,w.source_version,g.scope_id,g.version AS current_gateway_version,l.version AS current_source_version FROM qintopia_identity.work_accounts w JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=w.tenant_key AND g.gateway_key=w.gateway_key JOIN qintopia_identity.source_identity_links l ON l.id=w.source_link_id WHERE w.tenant_key=$1 AND w.id=$2 AND w.active AND g.active AND l.person_id IS NULL AND l.status<>'revoked' AND l.adapter_metadata ? 'first_observation_ref' FOR SHARE OF w,g,l")
                    .bind(&self.tenant).bind(account).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("work_account_unavailable"))?;
                ensure!(
                    account_row.get::<Uuid, _>("scope_id") == scope
                        && account_row.get::<i64, _>("gateway_version")
                            == account_row.get::<i64, _>("current_gateway_version")
                        && account_row.get::<i64, _>("source_version")
                            == account_row.get::<i64, _>("current_source_version"),
                    "gateway_scope_mismatch"
                );
                let existing:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_operation_grants WHERE tenant_key=$1 AND binding_id=$2 AND work_account_id=$3 AND operation_key=$4 AND revoked_at IS NULL)")
                    .bind(&self.tenant).bind(binding).bind(account).bind(operation).fetch_one(&mut *tx).await?;
                ensure!(!existing, "business_operation_grant_exists");
                let role_conflict:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_operation_grants WHERE tenant_key=$1 AND binding_id=$2 AND work_account_id=$3 AND revoked_at IS NULL AND account_role<>$4)")
                    .bind(&self.tenant).bind(binding).bind(account).bind(role).fetch_one(&mut *tx).await?;
                ensure!(!role_conflict, "business_account_role_conflict");
                let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_operation_grants(tenant_key,authority_grant_id,binding_id,operation_key,issued_by,valid_until,work_account_id,work_account_version,account_role) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9) RETURNING id")
                    .bind(&self.tenant).bind(manager.id).bind(binding).bind(operation).bind(actor.person).bind(valid_until).bind(account).bind(account_row.get::<i64,_>("version")).bind(role).fetch_one(&mut *tx).await?;
                json!({"kind":"operation_grant","grant":id,"binding":binding,"account":account,"role":role,"operation":operation,"active":true})
            }
            BusinessConfigChange::RevokeOperation { grant } => {
                let row = sqlx::query("SELECT b.scope_id,o.operation_key FROM qintopia_agent_os.business_operation_grants o JOIN qintopia_agent_os.business_property_bindings b ON b.tenant_key=o.tenant_key AND b.id=o.binding_id WHERE o.tenant_key=$1 AND o.id=$2 AND o.revoked_at IS NULL FOR UPDATE OF o")
                    .bind(&self.tenant).bind(grant).fetch_optional(&mut *tx).await?
                    .ok_or_else(|| anyhow::anyhow!("business_operation_revoked"))?;
                let action = business::operation(&row.get::<String, _>("operation_key"))?["action"]
                    .as_str()
                    .ok_or_else(|| anyhow::anyhow!("unsupported_business_operation"))?
                    .to_string();
                ensure!(
                    policy
                        .manager(
                            actor.person,
                            row.get("scope_id"),
                            "anan",
                            "hospitality",
                            &action
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
