//! Transactional person collaboration persistence.
use super::{digest, model::*};
use anyhow::{bail, ensure, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

mod auth;
pub(super) use auth::{AccountCommand, Credentials};
mod catalog;
pub(crate) mod foundation;
mod rule_lifecycle;
pub(crate) mod steward;
pub(crate) use rule_lifecycle::{RuleCommand, RuleEdit};
mod identity;
mod ontology;
pub(crate) use identity::IdentityUiCommand;
mod organization;
pub use identity::{IdentityCommand, QiweConversion};
mod memory;
pub(crate) use memory::reply_context as shared_reply_context;
pub use memory::{MemoryChange, MemoryCommand, MemoryEvidence, ReplyCondition, ReplyStyle};

pub struct Store {
    pub(super) pool: PgPool,
    pub(super) tenant: String,
}

// Constructed only from the trusted local session, never deserialized from browser input.
pub struct Actor {
    link: Uuid,
    person: Uuid,
    identity_version: i64,
    identity_namespace: String,
    gateway: Option<(String, i64, Uuid)>,
    session_hash: Option<String>,
    tenant: String,
}

impl Store {
    pub async fn local(database: &str, tenant: &str) -> Result<Self> {
        ensure!(
            tenant.starts_with("synthetic-collaboration-") && tenant.len() <= 100,
            "synthetic_tenant_required"
        );
        Ok(Self {
            pool: super::connect_local(database).await?,
            tenant: tenant.into(),
        })
    }

    async fn begin(&self) -> Result<(Transaction<'_, Postgres>, i64, DateTime<Utc>)> {
        let mut tx = self.pool.begin().await?;
        let row=sqlx::query("SELECT version,mode,initialized,identity_namespace FROM qintopia_agent_os.collaboration_tenants WHERE tenant_key=$1 FOR UPDATE")
            .bind(&self.tenant).fetch_optional(&mut *tx).await?
            .ok_or_else(||anyhow::anyhow!("tenant_not_initialized"))?;
        ensure!(
            row.get::<String, _>("mode") == "synthetic"
                && row.get::<bool, _>("initialized")
                && row.get::<String, _>("identity_namespace") == self.tenant,
            "synthetic_tenant_required"
        );
        // After the lock wait: transaction-start now() can predate an expiring grant.
        let now = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        Ok((tx, row.get("version"), now))
    }

    pub async fn actor(&self, link: Uuid) -> Result<Actor> {
        let (mut tx, _, _) = self.begin().await?;
        let row=sqlx::query("SELECT l.person_id,l.version FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.persons p ON p.id=l.person_id WHERE l.id=$1 AND l.namespace=$2 AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL AND p.status='active' FOR SHARE OF l,p")
            .bind(link).bind(&self.tenant).fetch_optional(&mut *tx).await?
            .ok_or_else(||anyhow::anyhow!("verified_identity_required"))?;
        Ok(Actor {
            link,
            session_hash: None,
            person: row.get("person_id"),
            identity_version: row.get("version"),
            identity_namespace: self.tenant.clone(),
            gateway: None,
            tenant: self.tenant.clone(),
        })
    }

    async fn verify(&self, tx: &mut Transaction<'_, Postgres>, actor: &Actor) -> Result<()> {
        ensure!(actor.tenant == self.tenant, "tenant_mismatch");
        if let Some((gateway, version, scope)) = &actor.gateway {
            let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.person_identity_gateways g JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key WHERE g.tenant_key=$1 AND g.gateway_key=$2 AND g.version=$3 AND g.scope_id=$4 AND g.namespace=$5 AND g.active AND g.account_kind<>'shared' AND s.status='active' AND EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l WHERE l.id=$6 AND l.namespace=g.namespace AND l.subject_type=g.subject_type))")
                .bind(&self.tenant).bind(gateway).bind(version).bind(scope).bind(&actor.identity_namespace).bind(actor.link).fetch_one(&mut **tx).await?;
            ensure!(active, "gateway_changed_or_revoked");
        } else {
            ensure!(
                actor.identity_namespace == self.tenant,
                "identity_namespace_unbound"
            );
        }
        if let Some(hash) = &actor.session_hash {
            self.verify_session(tx, hash, actor.person).await?;
        }
        let row=sqlx::query("SELECT l.person_id,l.version,l.status,p.status AS person_status FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.persons p ON p.id=l.person_id WHERE l.id=$1 AND l.namespace=$2 AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL FOR SHARE OF l,p")
            .bind(actor.link).bind(&actor.identity_namespace).fetch_optional(&mut **tx).await?
            .ok_or_else(||anyhow::anyhow!("verified_identity_required"))?;
        ensure!(
            row.get::<Uuid, _>("person_id") == actor.person
                && row.get::<i64, _>("version") == actor.identity_version
                && row.get::<String, _>("status") == "confirmed"
                && row.get::<String, _>("person_status") == "active",
            "identity_changed_or_revoked"
        );
        Ok(())
    }

    async fn policy(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        now: DateTime<Utc>,
    ) -> Result<Policy> {
        foundation::load_policy(tx, &self.tenant, now).await
    }

    // Persisted permissions must remain editable/revocable before their term starts.
    // Grant.active is current execution eligibility, not the stored grant lifecycle.
    async fn configured_grant_ids(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        collaboration: Option<Uuid>,
    ) -> Result<Vec<Uuid>> {
        Ok(sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_grants WHERE tenant_key=$1 AND status='active' AND ($2::uuid IS NULL OR collaboration_id=$2)")
            .bind(&self.tenant).bind(collaboration).fetch_all(&mut **tx).await?)
    }

    pub async fn allowed(
        &self,
        actor: &Actor,
        scope: Uuid,
        agent: &str,
        domain: &str,
        action: &str,
    ) -> Result<bool> {
        ensure!(
            agents().contains(&agent) && DOMAINS.contains(&domain) && ACTIONS.contains(&action),
            "unknown_permission_dimension"
        );
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        Ok(self
            .policy(&mut tx, now)
            .await?
            .allowed(actor.person, scope, agent, domain, action))
    }

    // A query is not a reusable send/execute ticket. F3 must call under its final execution transaction.
    pub async fn decision(
        &self,
        actor: &Actor,
        collaboration: Uuid,
        action: &str,
    ) -> Result<Value> {
        ensure!(ACTIONS.contains(&action), "unknown_permission_dimension");
        let (mut tx, version, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let p = self.policy(&mut tx, now).await?;
        let r = sqlx::query("SELECT a.person_id,a.scope_id,c.agent_key,c.domain_key FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND c.id=$2")
            .bind(&self.tenant).bind(collaboration).fetch_optional(&mut *tx).await?
            .ok_or_else(|| anyhow::anyhow!("collaboration_not_found"))?;
        ensure!(
            (r.get::<Uuid, _>("person_id") == actor.person
                && (actor.session_hash.is_none()
                    || p.grants
                        .iter()
                        .any(|g| g.collaboration == collaboration && p.effective(g))))
                || p.can_inspect(
                    actor.person,
                    r.get("scope_id"),
                    r.get::<String, _>("agent_key").as_str(),
                    r.get::<String, _>("domain_key").as_str()
                ),
            "scope_access_denied"
        );
        let mut decision = p.decision(collaboration, action);
        decision["configuration_version"] = json!(version);
        decision["runtime_connected"] = json!(false);
        Ok(decision)
    }

    // A query is not a reusable send/execute ticket. F3 must call under its final execution transaction.
    pub async fn responsible(
        &self,
        actor: &Actor,
        scope: Uuid,
        agent: &str,
        domain: &str,
        action: &str,
    ) -> Result<Value> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let policy = self.policy(&mut tx, now).await?;
        ensure!(
            policy
                .manager(actor.person, scope, agent, domain, action)
                .is_some()
                || policy.allowed(actor.person, scope, agent, domain, action),
            "scope_access_denied"
        );
        let mut people: Vec<_> = policy
            .grants
            .iter()
            .filter(|g| policy.allowed(g.person, scope, agent, domain, action))
            .map(|g| g.person)
            .collect();
        people.sort();
        people.dedup();
        Ok(
            json!({"status":match people.len(){0=>"unassigned",1=>"resolved",_=>"conflict"},"people":people}),
        )
    }

    pub async fn command(&self, actor: &Actor, command: &Command, apply: bool) -> Result<Value> {
        let (mut tx, version, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let hash = digest(&serde_json::to_vec(command)?);
        if let Some(row)=sqlx::query("SELECT request_hash,result,actor_identity_id FROM qintopia_agent_os.collaboration_commands WHERE id=$1 AND tenant_key=$2")
            .bind(command.operation_id).bind(&self.tenant).fetch_optional(&mut *tx).await? {
            ensure!(row.get::<Uuid,_>("actor_identity_id")==actor.link && row.get::<String,_>("request_hash")==hash,"idempotency_conflict");
            // Password sessions never replay historical payloads after an authority change.
            // Caller must refresh current state; the already committed command remains intact.
            ensure!(actor.session_hash.is_none(), "command_already_processed_refresh_state");
            let mut result:Value=row.get("result");
            result["replayed"]=json!(true);
            return Ok(result);
        }
        ensure!(
            version == command.expected_version,
            "configuration_version_conflict"
        );
        let policy = self.policy(&mut tx, now).await?;
        let before = self.audit_snapshot(&mut tx, &command.change, None).await?;
        let mut result = self
            .change(&mut tx, actor, &policy, &command.change, now)
            .await?;
        let after = self
            .audit_snapshot(
                &mut tx,
                &command.change,
                result
                    .get("id")
                    .or_else(|| result.get("collaboration"))
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
            )
            .await?;
        if before.is_some() || after.is_some() {
            result["before"] = json!(before);
            result["after"] = json!(after);
        }
        let actor_label: String = sqlx::query_scalar("SELECT coalesce(preferred_name,display_name) FROM qintopia_identity.persons WHERE id=$1")
            .bind(actor.person).fetch_one(&mut *tx).await?;
        result["actor_label"] = json!(actor_label);
        result["impact"] = json!(match &command.change {
            Change::EndAppointment { .. } =>
                vec!["结束这项任职及其工作连接，后续操作不再沿用原权限。"],
            Change::EndCollaboration { .. } => vec!["结束所选协作及其授权；其他独立工作保留。"],
            Change::RevokeGrant { .. } =>
                vec!["收回这项权限及依赖它的转授，待执行事项会重新核验。"],
            Change::ConfigureWork { assignment, .. }
                if assignment.valid_from.is_some_and(|t| t > now) =>
                vec![
                    "已安排未来开始；到开始时间前不产生可执行权限。",
                    "真正执行时仍核对届时有效的任职和授权。"
                ],
            Change::ConfigureWork { .. } | Change::Assign(_) =>
                vec!["只变更所选工作安排；没有启用真实消息或上传。"],
            _ => vec!["保存配置与历史记录；登记或恢复对象不自动增加业务权限。"],
        });
        let result = json!({"persisted":apply,"version":if apply{version+1}else{version},"change":result,"replayed":false,"runtime_connected":false});
        if !apply {
            tx.rollback().await?;
            return Ok(result);
        }
        sqlx::query("UPDATE qintopia_agent_os.collaboration_tenants SET version=version+1 WHERE tenant_key=$1")
            .bind(&self.tenant).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_commands(id,tenant_key,actor_identity_id,actor_person_id,request_hash,expected_version,result) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(command.operation_id).bind(&self.tenant).bind(actor.link).bind(actor.person).bind(&hash).bind(version).bind(&result).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.tool_invocation_audit(profile_id,tool_name,purpose,input_summary,output_summary,risk_level) VALUES('collaboration-local','collaboration.configure','synthetic_configuration',$1,$2,'high')")
            .bind(json!({"command_ref":command.operation_id,"actor_ref":actor.person,"identity_version":actor.identity_version,"request_hash":hash}))
            .bind(json!({"version":version+1,"persisted":true,"external_effects":false})).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }

    async fn change(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        p: &Policy,
        change: &Change,
        now: DateTime<Utc>,
    ) -> Result<Value> {
        match change {
            Change::ConfigureWork {
                assignment,
                audience,
            } => {
                ensure!(
                    assignment.duty.is_some() && assignment.actions.is_empty(),
                    "explicit_duty_permissions_required"
                );
                let before = self.work_snapshot(tx, assignment.collaboration).await?;
                let mut result = self.assign(tx, actor, p, assignment, now).await?;
                let id: Uuid = serde_json::from_value(result["collaboration"].clone())?;
                // Re-read policy after replacement/revocation; contact validation shares
                // the same transaction, version lock, idempotency key and audit entry.
                let current_policy = self.policy(tx, now).await?;
                self.organization_change(
                    tx,
                    actor,
                    &current_policy,
                    &Change::SetAudience {
                        collaboration: id,
                        audience: audience.clone(),
                    },
                    now,
                )
                .await?;
                result["kind"] = json!("configure_work");
                result["before"] = before;
                result["after"] = self.work_snapshot(tx, Some(id)).await?;
                Ok(result)
            }
            Change::SavePosition { .. }
            | Change::SaveLedger { .. }
            | Change::Lifecycle { .. }
            | Change::SetAudience { .. } => {
                self.organization_change(tx, actor, p, change, now).await
            }
            Change::Assign(a) => self.assign(tx, actor, p, a, now).await,
            Change::RevokeGrant { grant } => {
                let g = p
                    .grants
                    .iter()
                    .find(|g| g.id == *grant)
                    .ok_or_else(|| anyhow::anyhow!("grant_not_found"))?;
                ensure!(g.parent.is_some(), "bootstrap_relation_cannot_be_rewritten");
                ensure!(
                    p.manager(actor.person, g.scope, &g.agent, &g.domain, &g.action)
                        .is_some(),
                    "management_denied"
                );
                let n=sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked',version=version+1,revoked_at=$3 WHERE tenant_key=$1 AND id=$2 AND status='active'")
                    .bind(&self.tenant).bind(grant).bind(now).execute(&mut **tx).await?.rows_affected();
                ensure!(n == 1, "grant_already_revoked");
                Ok(json!({"kind":"revoke_grant","grant":grant}))
            }
            Change::EndAppointment { appointment } => {
                let r=sqlx::query("SELECT scope_id FROM qintopia_agent_os.collaboration_appointments WHERE tenant_key=$1 AND id=$2 AND status='active'")
                    .bind(&self.tenant).bind(appointment).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("appointment_not_active"))?;
                let scope: Uuid = r.get("scope_id");
                let ids:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_grants WHERE tenant_key=$1 AND collaboration_id IN(SELECT id FROM qintopia_agent_os.agent_collaborations WHERE appointment_id=$2)")
                    .bind(&self.tenant).bind(appointment).fetch_all(&mut **tx).await?;
                let connections=sqlx::query("SELECT agent_key,domain_key FROM qintopia_agent_os.agent_collaborations WHERE tenant_key=$1 AND appointment_id=$2 AND status='active'")
                    .bind(&self.tenant).bind(appointment).fetch_all(&mut **tx).await?;
                ensure!(!connections.is_empty(), "empty_appointment");
                for c in connections {
                    ensure!(
                        p.can_inspect(
                            actor.person,
                            scope,
                            c.get::<String, _>("agent_key").as_str(),
                            c.get::<String, _>("domain_key").as_str()
                        ),
                        "management_denied"
                    );
                }
                for g in p.grants.iter().filter(|g| ids.contains(&g.id)) {
                    ensure!(g.parent.is_some(), "bootstrap_relation_cannot_be_rewritten");
                    ensure!(
                        p.manager(actor.person, scope, &g.agent, &g.domain, &g.action)
                            .is_some(),
                        "management_denied"
                    );
                }
                sqlx::query("UPDATE qintopia_agent_os.collaboration_appointments SET status='ended',version=version+1,ended_at=$3 WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(appointment).bind(now).execute(&mut **tx).await?;
                sqlx::query("UPDATE qintopia_agent_os.agent_collaborations SET status='ended',version=version+1 WHERE tenant_key=$1 AND appointment_id=$2")
                    .bind(&self.tenant).bind(appointment).execute(&mut **tx).await?;
                Ok(json!({"kind":"end_appointment","appointment":appointment}))
            }
            Change::CreateScope {
                parent,
                label: name,
                scope_kind,
            } => {
                label(name, 80)?;
                ensure!(
                    matches!(scope_kind.as_str(), "building" | "business"),
                    "invalid_scope_kind"
                );
                ensure!(
                    p.manager(actor.person, *parent, "default", "organization", "manage")
                        .is_some(),
                    "organization_management_required"
                );
                let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,parent_scope_id,label,kind) VALUES($1,$2,$3,$4) RETURNING id")
                    .bind(&self.tenant).bind(parent).bind(name).bind(scope_kind).fetch_one(&mut **tx).await?;
                Ok(json!({"kind":"create_scope","scope":id}))
            }
            Change::CreateRole {
                label: name,
                available_actions,
            } => {
                label(name, 80)?;
                ensure!(
                    valid_list(available_actions, ACTIONS),
                    "invalid_role_actions"
                );
                ensure!(Self::catalog_admin(p, actor), "catalog_management_required");
                let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_roles(tenant_key,label,available_actions) VALUES($1,$2,$3) RETURNING id")
                    .bind(&self.tenant).bind(name).bind(available_actions).fetch_one(&mut **tx).await?;
                Ok(json!({"kind":"create_role","role":id}))
            }
            Change::SetGroups {
                scope,
                conversations,
            } => {
                ensure!(
                    conversations.len() <= 100
                        && conversations
                            .iter()
                            .collect::<std::collections::BTreeSet<_>>()
                            .len()
                            == conversations.len(),
                    "invalid_groups"
                );
                // Moving/adding a group expands the scope. F1 requires the community-root envelope.
                let root = p
                    .scopes
                    .iter()
                    .find(|s| s.parent.is_none())
                    .ok_or_else(|| anyhow::anyhow!("root_scope_missing"))?;
                ensure!(
                    p.manager(actor.person, root.id, "default", "organization", "manage")
                        .is_some()
                        && p.in_scope(*scope, root.id, true),
                    "group_binding_management_required"
                );
                let count:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_messages.conversations c WHERE tenant_id=$1 AND id=ANY($2) AND status='active' AND chat_type='group' AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_knowledge_items k WHERE k.tenant_key=$1 AND k.space_id=c.id) AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger l WHERE l.tenant_key=$1 AND l.kind='group' AND l.object_ref=c.id::text AND l.status<>'active')")
                    .bind(&self.tenant).bind(conversations).fetch_one(&mut **tx).await?;
                ensure!(count == conversations.len() as i64, "group_outside_tenant");
                sqlx::query("UPDATE qintopia_agent_os.collaboration_scope_bindings SET revoked_at=$3,version=version+1 WHERE tenant_key=$1 AND scope_id=$2 AND revoked_at IS NULL")
                    .bind(&self.tenant).bind(scope).bind(now).execute(&mut **tx).await?;
                for conversation in conversations {
                    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3)")
                        .bind(&self.tenant).bind(scope).bind(conversation).execute(&mut **tx).await?;
                }
                sqlx::query("UPDATE qintopia_agent_os.collaboration_scopes SET version=version+1 WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(scope).execute(&mut **tx).await?;
                Ok(json!({"kind":"set_groups","scope":scope,"count":conversations.len()}))
            }
            other => self.catalog_change(tx, actor, p, other, now).await,
        }
    }

    async fn assign(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        p: &Policy,
        a: &Assignment,
        now: DateTime<Utc>,
    ) -> Result<Value> {
        a.validate(now)?;
        self.check_organization_assignment(tx, a).await?;
        if let Some(id) = a.collaboration {
            let old = sqlx::query("SELECT a.person_id,a.role_id,a.scope_id,a.valid_from,a.valid_until,a.proxy_for_id,c.agent_key,c.domain_key,c.duty_id FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND c.id=$2 AND c.status='active'")
                .bind(&self.tenant).bind(id).fetch_optional(&mut **tx).await?
                .ok_or_else(|| anyhow::anyhow!("collaboration_not_active"))?;
            let unchanged = old.get::<Uuid, _>("person_id") == a.person
                && old.get::<Uuid, _>("role_id") == a.role
                && old.get::<Uuid, _>("scope_id") == a.scope
                && old.get::<String, _>("agent_key") == a.agent
                && old.get::<String, _>("domain_key") == a.domain
                && old.get::<Option<Uuid>, _>("duty_id") == a.duty
                && a.valid_from
                    .is_none_or(|start| start == old.get::<DateTime<Utc>, _>("valid_from"))
                && old.get::<Option<DateTime<Utc>>, _>("valid_until") == a.valid_until
                && old.get::<Option<Uuid>, _>("proxy_for_id") == a.proxy_for;
            if !unchanged {
                self.end_connection(tx, actor, p, id, now).await?;
                let updated_policy = self.policy(tx, now).await?;
                let mut replacement = a.clone();
                replacement.collaboration = None;
                replacement.valid_from = a
                    .valid_from
                    .or(Some(old.get::<DateTime<Utc>, _>("valid_from")))
                    .filter(|start| *start > now);
                let mut result =
                    Box::pin(self.assign(tx, actor, &updated_policy, &replacement, now)).await?;
                let replacement_id: Uuid = serde_json::from_value(result["collaboration"].clone())?;
                sqlx::query("UPDATE qintopia_agent_os.agent_collaborations SET replaces_id=$3 WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(replacement_id).bind(id).execute(&mut **tx).await?;
                result["replaced"] = json!(id);
                return Ok(result);
            }
        }
        ensure!(
            p.can_inspect(actor.person, a.scope, &a.agent, &a.domain),
            "management_denied"
        );
        let settings = a.settings();
        let action_keys: Vec<String> = settings.iter().map(|s| s.action.clone()).collect();
        if let Some(duty) = a.duty {
            let row = sqlx::query("SELECT d.domain_key,d.available_actions FROM qintopia_agent_os.collaboration_duties d JOIN qintopia_agent_os.collaboration_role_duties rd ON rd.duty_id=d.id AND rd.tenant_key=d.tenant_key JOIN qintopia_agent_os.collaboration_roles r ON r.id=rd.role_id WHERE d.tenant_key=$1 AND d.id=$2 AND rd.role_id=$3 AND d.status='active' AND r.status='active'")
                .bind(&self.tenant).bind(duty).bind(a.role).fetch_optional(&mut **tx).await?
                .ok_or_else(||anyhow::anyhow!("duty_not_available_for_role"))?;
            ensure!(
                row.get::<String, _>("domain_key") == a.domain,
                "duty_domain_mismatch"
            );
            ensure!(
                subset(
                    &action_keys,
                    &row.get::<Vec<String>, _>("available_actions")
                ),
                "action_outside_duty"
            );
        }
        let mut parents = Vec::new();
        for setting in &settings {
            let action = &setting.action;
            let parent = p
                .manager(actor.person, a.scope, &a.agent, &a.domain, action)
                .ok_or_else(|| anyhow::anyhow!("management_denied"))?;
            if let Some(d) = a.delegation.as_ref().filter(|_| action == "manage") {
                ensure!(
                    parent.delegation.depth > d.depth
                        && subset(&d.agents, &parent.delegation.agents)
                        && subset(&d.domains, &parent.delegation.domains)
                        && subset(&d.actions, &parent.delegation.actions),
                    "delegation_exceeds_authority"
                );
            }
            parents.push(parent.id);
            if setting.mode == PermissionMode::Confirmation {
                let reviewer = setting
                    .reviewer
                    .ok_or_else(|| anyhow::anyhow!("reviewer_required"))?;
                ensure!(
                    reviewer != a.person
                        && a.duty.is_some()
                        && p.grants.iter().any(|g| g.person == reviewer
                            && g.duty == a.duty
                            && g.agent == a.agent
                            && g.domain == a.domain
                            && g.action == *action
                            && g.scope == a.scope
                            && p.decision(g.collaboration, action)["status"] == "autonomous"),
                    "reviewer_not_authorized"
                );
            }
        }
        let known:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.persons p JOIN qintopia_identity.source_identity_links l ON l.person_id=p.id WHERE p.id=$1 AND p.status='active' AND l.namespace=$2 AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL)")
            .bind(a.person).bind(&self.tenant).fetch_one(&mut **tx).await?;
        ensure!(known, "verified_person_required");
        let available: Option<Vec<String>>=sqlx::query_scalar("SELECT available_actions FROM qintopia_agent_os.collaboration_roles WHERE id=$1 AND tenant_key=$2 AND status='active'")
            .bind(a.role).bind(&self.tenant).fetch_optional(&mut **tx).await?;
        let available = available.ok_or_else(|| anyhow::anyhow!("role_outside_tenant"))?;
        ensure!(
            a.duty.is_some() || subset(&action_keys, &available),
            "action_outside_role"
        );
        if let Some(proxy) = a.proxy_for {
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_appointments WHERE id=$1 AND tenant_key=$2 AND scope_id=$3 AND role_id=$4 AND person_id<>$5 AND status='active' AND valid_from<=$6 AND (valid_until IS NULL OR valid_until>$6))")
                .bind(proxy).bind(&self.tenant).bind(a.scope).bind(a.role).bind(a.person).bind(now).fetch_one(&mut **tx).await?;
            ensure!(valid, "invalid_proxy_appointment");
        }
        sqlx::query("UPDATE qintopia_agent_os.collaboration_appointments SET status='ended',ended_at=$5,version=version+1 WHERE tenant_key=$1 AND person_id=$2 AND role_id=$3 AND scope_id=$4 AND status='active' AND valid_until<=$5")
            .bind(&self.tenant).bind(a.person).bind(a.role).bind(a.scope).bind(now).execute(&mut **tx).await?;
        let existing=sqlx::query("SELECT id,valid_from,valid_until,proxy_for_id FROM qintopia_agent_os.collaboration_appointments WHERE tenant_key=$1 AND person_id=$2 AND role_id=$3 AND scope_id=$4 AND status='active'")
            .bind(&self.tenant).bind(a.person).bind(a.role).bind(a.scope).fetch_optional(&mut **tx).await?;
        let appointment = if let Some(r) = existing {
            ensure!(
                a.valid_from
                    .is_none_or(|start| start == r.get::<DateTime<Utc>, _>("valid_from"))
                    && r.get::<Option<DateTime<Utc>>, _>("valid_until") == a.valid_until
                    && r.get::<Option<Uuid>, _>("proxy_for_id") == a.proxy_for,
                "existing_term_differs"
            );
            r.get::<Uuid, _>("id")
        } else {
            ensure!(
                a.valid_from.is_none_or(|start| start >= now),
                "term_start_in_past"
            );
            sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_appointments(tenant_key,person_id,role_id,scope_id,valid_from,valid_until,proxy_for_id) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
                .bind(&self.tenant).bind(a.person).bind(a.role).bind(a.scope).bind(a.valid_from.unwrap_or(now)).bind(a.valid_until).bind(a.proxy_for).fetch_one(&mut **tx).await?
        };
        let relation = if let Some(id) = a.collaboration {
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.agent_collaborations WHERE tenant_key=$1 AND id=$2 AND appointment_id=$3 AND agent_key=$4 AND domain_key=$5 AND status='active')")
                .bind(&self.tenant).bind(id).bind(appointment).bind(&a.agent).bind(&a.domain).fetch_one(&mut **tx).await?;
            ensure!(valid, "collaboration_dimensions_changed");
            let configured = self.configured_grant_ids(tx, Some(id)).await?;
            for g in p.grants.iter().filter(|g| configured.contains(&g.id)) {
                ensure!(
                    p.manager(actor.person, g.scope, &g.agent, &g.domain, &g.action)
                        .is_some(),
                    "management_denied"
                );
                ensure!(g.parent.is_some(), "bootstrap_relation_cannot_be_rewritten");
            }
            sqlx::query("UPDATE qintopia_agent_os.agent_collaborations SET responsibility_text=$3,version=version+1 WHERE tenant_key=$1 AND id=$2")
                .bind(&self.tenant).bind(id).bind(&a.responsibility).execute(&mut **tx).await?;
            sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked',revoked_at=$4,version=version+1 WHERE tenant_key=$1 AND collaboration_id=$2 AND status='active' AND NOT(action_key=ANY($3))")
                .bind(&self.tenant).bind(id).bind(&action_keys).bind(now).execute(&mut **tx).await?;
            id
        } else {
            sqlx::query_scalar("INSERT INTO qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key,responsibility_text,duty_id) VALUES($1,$2,$3,$4,$5,$6) RETURNING id")
                .bind(&self.tenant).bind(appointment).bind(&a.agent).bind(&a.domain).bind(&a.responsibility).bind(a.duty).fetch_one(&mut **tx).await?
        };
        let configured = self.configured_grant_ids(tx, Some(relation)).await?;
        let mut grants = Vec::new();
        for (setting, parent) in settings.iter().zip(parents) {
            let action = &setting.action;
            let empty = Delegation {
                agents: vec![],
                domains: vec![],
                actions: vec![],
                depth: 0,
            };
            let d = if action == "manage" {
                a.delegation.as_ref().unwrap_or(&empty)
            } else {
                &empty
            };
            if let Some(old) = p
                .grants
                .iter()
                .find(|g| g.action == *action && configured.contains(&g.id))
            {
                if p.effective(old)
                    && old.mode == setting.mode
                    && old.reviewer == setting.reviewer
                    && old.delegation.agents == d.agents
                    && old.delegation.domains == d.domains
                    && old.delegation.actions == d.actions
                    && old.delegation.depth == d.depth
                {
                    grants.push(old.id);
                    continue;
                }
                // Changing the delegation envelope invalidates grants derived from the old envelope.
                sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked',revoked_at=$3,version=version+1 WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(old.id).bind(now).execute(&mut **tx).await?;
            }
            let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id,include_descendants,managed_agents,managed_domains,managed_actions,delegation_depth,decision_mode,reviewer_person_id) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11) RETURNING id")
                .bind(&self.tenant).bind(relation).bind(action).bind(parent).bind(action=="manage" && setting.mode==PermissionMode::Autonomous).bind(&d.agents).bind(&d.domains).bind(&d.actions).bind(d.depth).bind(setting.mode.as_str()).bind(setting.reviewer).fetch_one(&mut **tx).await?;
            grants.push(id);
        }
        sqlx::query("UPDATE qintopia_agent_os.collaboration_positions SET used=true WHERE tenant_key=$1 AND role_id=$2 AND scope_id=$3")
            .bind(&self.tenant).bind(a.role).bind(a.scope).execute(&mut **tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.collaboration_ledger SET used=true WHERE tenant_key=$1 AND ((kind='person' AND object_ref=$2) OR (kind='agent' AND object_ref=$3))")
            .bind(&self.tenant).bind(a.person.to_string()).bind(&a.agent).execute(&mut **tx).await?;
        Ok(
            json!({"kind":"assign","appointment":appointment,"collaboration":relation,"grants":grants,"onboarding":"not_connected"}),
        )
    }

    pub async fn state(&self, actor: &Actor) -> Result<Value> {
        let (mut tx, version, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let p = self.policy(&mut tx, now).await?;
        let root_manager = Self::catalog_admin(&p, actor);
        let configured = self.configured_grant_ids(&mut tx, None).await?;
        // A person may read their own upcoming assignment before it grants any
        // authority. The source delegation must still be valid now; no future
        // grant is inserted into the execution policy.
        let upcoming: Vec<(Uuid, Uuid)> = if actor.session_hash.is_some() {
            sqlx::query_as("SELECT c.id,a.scope_id FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id JOIN qintopia_agent_os.collaboration_roles r ON r.id=a.role_id JOIN qintopia_agent_os.collaboration_duties d ON d.id=c.duty_id WHERE c.tenant_key=$1 AND a.person_id=$2 AND c.status='active' AND a.status='active' AND r.status='active' AND d.status='active' AND a.valid_from>$3 AND (a.valid_until IS NULL OR a.valid_until>a.valid_from)")
                .bind(&self.tenant).bind(actor.person).bind(now).fetch_all(&mut *tx).await?
        } else {
            vec![]
        };
        let upcoming: Vec<(Uuid, Uuid)> = upcoming
            .into_iter()
            .filter(|(id, _)| {
                p.grants.iter().any(|g| {
                    if g.collaboration != *id || !configured.contains(&g.id) {
                        return false;
                    }
                    let mut readable = g.clone();
                    readable.active = true;
                    p.effective(&readable)
                })
            })
            .collect();
        let visible: Vec<Uuid> = p
            .scopes
            .iter()
            .filter(|s| {
                // A global catalog manager also needs archived scopes for
                // lifecycle maintenance. Partial-root managers remain scoped.
                root_manager
                    || upcoming.iter().any(|(_, scope)| *scope == s.id)
                    || p.grants.iter().any(|g| {
                        g.person == actor.person
                            && (actor.session_hash.is_some() || g.action == "manage")
                            && p.effective(g)
                            && (g.action != "manage"
                                || (g.mode == PermissionMode::Autonomous && g.reviewer.is_none()))
                            && p.in_scope(s.id, g.scope, g.descendants)
                    })
            })
            .map(|s| s.id)
            .collect();
        if actor.session_hash.is_none() {
            ensure!(!visible.is_empty(), "management_denied");
        }
        let people:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(x ORDER BY x->>'label'),'[]') FROM (SELECT DISTINCT jsonb_build_object('id',p.id,'label',coalesce(p.preferred_name,p.display_name),'display_name',p.display_name) x FROM qintopia_identity.persons p JOIN qintopia_identity.source_identity_links l ON l.person_id=p.id WHERE l.namespace=$1 AND l.status='confirmed' AND p.status='active') s")
            .bind(&self.tenant).fetch_one(&mut *tx).await?;
        let scopes:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'parent',parent_scope_id,'label',label,'kind',kind,'version',version,'status',status) ORDER BY label),'[]') FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND id=ANY($2)")
            .bind(&self.tenant).bind(&visible).fetch_one(&mut *tx).await?;
        let mut roles:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'label',label,'description',description,'available_actions',available_actions,'status',status,'version',version,'duty_ids',ARRAY(SELECT duty_id FROM qintopia_agent_os.collaboration_role_duties rd WHERE rd.tenant_key=r.tenant_key AND rd.role_id=r.id ORDER BY duty_id)) ORDER BY label,id),'[]') FROM qintopia_agent_os.collaboration_roles r WHERE tenant_key=$1")
            .bind(&self.tenant).fetch_one(&mut *tx).await?;
        let mut duties:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'label',label,'description',description,'domain',domain_key,'available_actions',available_actions,'status',status,'version',version) ORDER BY label),'[]') FROM qintopia_agent_os.collaboration_duties WHERE tenant_key=$1")
            .bind(&self.tenant).fetch_one(&mut *tx).await?;
        let relations:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('id',c.id,'appointment',a.id,'person',a.person_id,'role',a.role_id,'scope',a.scope_id,'agent',c.agent_key,'domain',c.domain_key,'duty',c.duty_id,'version',c.version,'replaces',c.replaces_id,'immutable',EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_grants g WHERE g.collaboration_id=c.id AND g.parent_grant_id IS NULL),'responsibility',c.responsibility_text,'status',CASE WHEN c.status<>'active' THEN c.status WHEN a.status<>'active' THEN a.status WHEN a.valid_until<=$3 THEN 'expired' WHEN a.valid_from>$3 THEN 'scheduled' ELSE 'active' END,'valid_from',a.valid_from,'valid_until',a.valid_until,'proxy_for',a.proxy_for_id) ORDER BY c.created_at),'[]') FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND a.scope_id=ANY($2)")
            .bind(&self.tenant).bind(&visible).bind(now).fetch_one(&mut *tx).await?;
        let groups:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'label',display_name) ORDER BY display_name),'[]') FROM qintopia_messages.conversations c WHERE tenant_id=$1 AND chat_type='group' AND status='active' AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_knowledge_items k WHERE k.tenant_key=$1 AND k.space_id=c.id)")
            .bind(&self.tenant).fetch_one(&mut *tx).await?;
        let bindings:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('scope',scope_id,'conversation',conversation_id) ORDER BY scope_id,conversation_id),'[]') FROM qintopia_agent_os.collaboration_scope_bindings WHERE tenant_key=$1 AND scope_id=ANY($2) AND revoked_at IS NULL")
            .bind(&self.tenant).bind(&visible).fetch_one(&mut *tx).await?;
        let grants:Vec<Value>=p.grants.iter().filter(|g|(g.person==actor.person && (p.effective(g) || (configured.contains(&g.id) && upcoming.iter().any(|(id,_)|*id==g.collaboration)))) || p.can_inspect(actor.person,g.scope,&g.agent,&g.domain)).map(|g|json!({"id":g.id,"collaboration":g.collaboration,"person":g.person,"scope":g.scope,"agent":g.agent,"domain":g.domain,"duty":g.duty,"action":g.action,"mode":g.mode,"reviewer":g.reviewer,"effective":p.effective(g),"revocable":configured.contains(&g.id),"management_envelope":if g.action=="manage" && g.mode==PermissionMode::Autonomous{Some(&g.delegation)}else{None}})).collect();
        let mut relations: Vec<Value> = relations
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| {
                r["scope"]
                    .as_str()
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .is_some_and(|scope| {
                        (r["person"] == json!(actor.person)
                            && (upcoming.iter().any(|(id, _)| json!(id) == r["id"])
                                || p.grants
                                    .iter()
                                    .any(|g| json!(g.collaboration) == r["id"] && p.effective(g))))
                            || p.can_inspect(
                                actor.person,
                                scope,
                                r["agent"].as_str().unwrap_or(""),
                                r["domain"].as_str().unwrap_or(""),
                            )
                    })
            })
            .cloned()
            .collect();
        for relation in &mut relations {
            let scope: Uuid = serde_json::from_value(relation["scope"].clone())?;
            relation["can_manage"] = json!(p.can_inspect(
                actor.person,
                scope,
                relation["agent"].as_str().unwrap_or(""),
                relation["domain"].as_str().unwrap_or("")
            ));
        }
        let management_available = p.grants.iter().any(|g| {
            g.person == actor.person
                && g.action == "manage"
                && g.mode == PermissionMode::Autonomous
                && p.effective(g)
        });
        // History contains whole-tenant snapshots. A manager of one root or of
        // a root without descendants cannot use it to inspect other scopes.
        let history_visible = root_manager;
        let identity_history_visible = history_visible
            && p.scopes.iter().filter(|s| s.active).all(|s| {
                p.manager(actor.person, s.id, "default", "organization", "identity")
                    .is_some()
            });
        let groups: Vec<Value> = groups
            .as_array()
            .into_iter()
            .flatten()
            .filter(|g| {
                root_manager
                    || bindings
                        .as_array()
                        .into_iter()
                        .flatten()
                        .any(|b| b["conversation"] == g["id"])
            })
            .cloned()
            .collect();
        let mut organization = self
            .organization_state(
                &mut tx,
                &visible,
                root_manager,
                history_visible,
                identity_history_visible,
            )
            .await?;
        // Scope visibility alone does not expose another Agent's contact configuration.
        organization["audiences"]
            .as_array_mut()
            .unwrap()
            .retain(|a| relations.iter().any(|r| r["id"] == a["collaboration"]));
        // Login callers only receive people participating in visible authorized connections.
        // Legacy fixture adapters keep their explicitly synthetic provisioning picker.
        let people: Vec<Value> = people
            .as_array()
            .into_iter()
            .flatten()
            .filter(|person| {
                actor.session_hash.is_none()
                    || root_manager
                    || person["id"] == json!(actor.person)
                    || relations.iter().any(|r| r["person"] == person["id"])
                    || grants.iter().any(|g| g["reviewer"] == person["id"])
            })
            .cloned()
            .collect();
        if actor.session_hash.is_some() && !root_manager {
            // Full contact/ledger metadata can contain arbitrary Person references. Only managers see it.
            organization["ledger"] = json!([]);
            organization["audiences"] = json!([]);
            organization["positions"]
                .as_array_mut()
                .unwrap()
                .retain(|pos| {
                    relations
                        .iter()
                        .any(|r| r["role"] == pos["role_id"] && r["scope"] == pos["scope_id"])
                });
        }
        let delegated_reviews =
            steward::delegated_scopes_in(&self.pool, &self.tenant, &mut tx, actor.person).await?;
        let personal = actor.session_hash.is_some() && !management_available;
        if personal {
            roles
                .as_array_mut()
                .unwrap()
                .retain(|role| relations.iter().any(|r| r["role"] == role["id"]));
            duties
                .as_array_mut()
                .unwrap()
                .retain(|duty| relations.iter().any(|r| r["duty"] == duty["id"]));
            for role in roles.as_array_mut().unwrap() {
                role["duty_ids"]
                    .as_array_mut()
                    .unwrap()
                    .retain(|id| duties.as_array().unwrap().iter().any(|d| d["id"] == *id));
            }
        }
        let agents: Vec<_> = agents()
            .into_iter()
            .filter(|key| !personal || relations.iter().any(|r| r["agent"] == *key))
            .collect();
        let domains: Vec<_> = DOMAINS
            .iter()
            .filter(|key| !personal || relations.iter().any(|r| r["domain"] == **key))
            .collect();
        let actions: Vec<_> = ACTIONS
            .iter()
            .filter(|key| !personal || grants.iter().any(|g| g["action"] == **key))
            .collect();
        Ok(
            json!({"version":version,"actor_person":actor.person,"delegated_reviews":delegated_reviews,"people":people,"scopes":scopes,"roles":roles,"duties":duties,"relations":relations,"agents":agents,"domains":domains,"actions":actions,"grants":grants,"groups":groups,"bindings":bindings,"organization":organization,"catalog_admin":root_manager,"management_available":management_available,"contact_configuration_visible":actor.session_hash.is_none() || root_manager,"local_dialogue_available":std::env::var("QINTOPIA_FOUNDATION_LOCAL_ENABLE").as_deref()==Ok("1"),"mode":"synthetic","runtime_connected":false}),
        )
    }

    /// Explicit one-shot synthetic fixture, not an administrator self-enrolment endpoint.
    pub async fn bootstrap_fixture(&self) -> Result<Uuid> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_tenants(tenant_key,identity_namespace,mode) VALUES($1,$1,'synthetic')")
            .bind(&self.tenant).execute(&mut *tx).await?;
        let root:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,label,kind) VALUES($1,'秦托邦','community') RETURNING id")
            .bind(&self.tenant).fetch_one(&mut *tx).await?;
        for name in ["一栋", "二栋", "三栋"] {
            let scope:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,parent_scope_id,label,kind) VALUES($1,$2,$3,'building') RETURNING id")
                .bind(&self.tenant).bind(root).bind(name).fetch_one(&mut *tx).await?;
            let group:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type,display_name) VALUES($1,'synthetic',$2,'group',$3) RETURNING id")
                .bind(&self.tenant).bind(Uuid::new_v4().to_string()).bind(format!("{name}居民群（合成）")).fetch_one(&mut *tx).await?;
            sqlx::query("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3)")
                .bind(&self.tenant).bind(scope).bind(group).execute(&mut *tx).await?;
        }
        let mut roles = Vec::new();
        for name in [
            "公司负责人",
            "社区负责人",
            "舍长",
            "小管家",
            "活动运营",
            "技术负责人",
        ] {
            // Explicit synthetic templates, never infer live authority from a title.
            let available: Vec<&str> = if name == "技术负责人" {
                vec!["technical_support"]
            } else {
                ACTIONS
                    .iter()
                    .copied()
                    .filter(|a| *a != "technical_support")
                    .collect()
            };
            let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_roles(tenant_key,label,available_actions) VALUES($1,$2,$3) RETURNING id")
                .bind(&self.tenant).bind(name).bind(available).fetch_one(&mut *tx).await?;
            roles.push(id);
        }
        let mut people = Vec::new();
        self.seed_duties(&mut tx).await?;
        let mut links = Vec::new();
        for (i, name) in [
            "合成负责人",
            "人员甲 · 合成样例 A",
            "人员甲 · 合成样例 B",
            "人员乙 · 合成样例",
            "人员丙 · 合成样例",
        ]
        .iter()
        .enumerate()
        {
            let person:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.persons(display_name,preferred_name) VALUES($1,$2) RETURNING id")
                .bind(if i==1 || i==2 {"人员甲"}else{name}).bind(name).fetch_one(&mut *tx).await?;
            let link:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by) VALUES($1,'feishu_open',$2,$3,'confirmed',$4,$3) RETURNING id")
                .bind(&self.tenant).bind(format!("fixture-person-{i}")).bind(person).bind(Uuid::new_v4()).fetch_one(&mut *tx).await?;
            people.push(person);
            links.push(link);
        }
        let appointment:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_appointments(tenant_key,person_id,role_id,scope_id) VALUES($1,$2,$3,$4) RETURNING id")
            .bind(&self.tenant).bind(people[0]).bind(roles[0]).bind(root).fetch_one(&mut *tx).await?;
        let relation:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key,responsibility_text) VALUES($1,$2,'default','organization','管理秦托邦的本地测试配置') RETURNING id")
            .bind(&self.tenant).bind(appointment).fetch_one(&mut *tx).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,include_descendants,managed_agents,managed_domains,managed_actions,delegation_depth) VALUES($1,$2,'manage',true,$3,$4,$5,8)")
            .bind(&self.tenant).bind(relation).bind(agents()).bind(DOMAINS).bind(ACTIONS).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.collaboration_tenants SET initialized=true WHERE tenant_key=$1")
            .bind(&self.tenant).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.tool_invocation_audit(profile_id,tool_name,input_summary,output_summary,risk_level) VALUES('collaboration-local','collaboration.synthetic_bootstrap',$1,'{\"synthetic\":true,\"external_effects\":false}','high')")
            .bind(json!({"actor_ref":people[0],"tenant_hash":digest(self.tenant.as_bytes())})).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(links[0])
    }

    pub async fn fixture_operator(&self) -> Result<Uuid> {
        let link=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND source_ref='fixture-person-0'")
            .bind(&self.tenant).fetch_optional(&self.pool).await?;
        match link {
            Some(id) => Ok(id),
            None => bail!("synthetic_fixture_required"),
        }
    }
}
