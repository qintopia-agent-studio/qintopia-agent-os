//! Transactional person collaboration persistence.
use super::{digest, model::*};
use anyhow::{bail, ensure, Result};
use chrono::{DateTime, Utc};
use serde_json::{json, Value};
use sqlx::{PgPool, Postgres, Row, Transaction};
use uuid::Uuid;

mod catalog;
mod organization;

pub struct Store {
    pub(super) pool: PgPool,
    pub(super) tenant: String,
}

// Constructed only from the trusted local session, never deserialized from browser input.
pub struct Actor {
    link: Uuid,
    person: Uuid,
    identity_version: i64,
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
            person: row.get("person_id"),
            identity_version: row.get("version"),
            tenant: self.tenant.clone(),
        })
    }

    async fn verify(&self, tx: &mut Transaction<'_, Postgres>, actor: &Actor) -> Result<()> {
        ensure!(actor.tenant == self.tenant, "tenant_mismatch");
        let row=sqlx::query("SELECT l.person_id,l.version,l.status,p.status AS person_status FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.persons p ON p.id=l.person_id WHERE l.id=$1 AND l.namespace=$2 AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL FOR SHARE OF l,p")
            .bind(actor.link).bind(&self.tenant).fetch_optional(&mut **tx).await?
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
        let rows=sqlx::query("SELECT id,parent_scope_id,status FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1")
            .bind(&self.tenant).fetch_all(&mut **tx).await?;
        let scopes = rows
            .iter()
            .map(|r| Scope {
                id: r.get("id"),
                parent: r.get("parent_scope_id"),
                active: r.get::<String, _>("status") == "active",
            })
            .collect();
        let rows=sqlx::query("SELECT g.*,a.person_id,a.scope_id,c.agent_key,c.domain_key,c.duty_id,(g.status='active' AND c.status='active' AND a.status='active' AND p.status='active' AND role.status='active' AND (c.duty_id IS NULL OR duty.status='active') AND a.valid_from<=$2 AND (a.valid_until IS NULL OR a.valid_until>$2) AND EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l WHERE l.person_id=a.person_id AND l.namespace=$1 AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL)) AS effective_now FROM qintopia_agent_os.collaboration_grants g JOIN qintopia_agent_os.agent_collaborations c ON c.id=g.collaboration_id JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id JOIN qintopia_identity.persons p ON p.id=a.person_id JOIN qintopia_agent_os.collaboration_roles role ON role.id=a.role_id LEFT JOIN qintopia_agent_os.collaboration_duties duty ON duty.id=c.duty_id WHERE g.tenant_key=$1")
            .bind(&self.tenant).bind(now).fetch_all(&mut **tx).await?;
        let mut grants: Vec<Grant> = rows
            .iter()
            .map(|r| Grant {
                id: r.get("id"),
                collaboration: r.get("collaboration_id"),
                person: r.get("person_id"),
                scope: r.get("scope_id"),
                agent: r.get("agent_key"),
                domain: r.get("domain_key"),
                action: r.get("action_key"),
                duty: r.get("duty_id"),
                mode: r
                    .get::<String, _>("decision_mode")
                    .parse()
                    .unwrap_or(PermissionMode::Denied),
                reviewer: r.get("reviewer_person_id"),
                parent: r.get("parent_grant_id"),
                descendants: r.get("include_descendants"),
                active: r.get("effective_now"),
                delegation: Delegation {
                    agents: r.get("managed_agents"),
                    domains: r.get("managed_domains"),
                    actions: r.get("managed_actions"),
                    depth: r.get("delegation_depth"),
                },
            })
            .collect();
        // Postgres may return the same rows in a different physical order after a rollback.
        // Keep policy selection and serialized configuration stable across readbacks.
        grants.sort_by_key(|grant| grant.id);
        Ok(Policy { scopes, grants })
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
            r.get::<Uuid, _>("person_id") == actor.person
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
            let mut result:Value=row.get("result");
            result["replayed"]=json!(true);
            return Ok(result);
        }
        ensure!(
            version == command.expected_version,
            "configuration_version_conflict"
        );
        let policy = self.policy(&mut tx, now).await?;
        let before = self
            .configuration_snapshot(&mut tx, &command.change, None)
            .await?;
        let mut result = self
            .change(&mut tx, actor, &policy, &command.change, now)
            .await?;
        let after = self
            .configuration_snapshot(
                &mut tx,
                &command.change,
                result
                    .get("id")
                    .and_then(|v| serde_json::from_value(v.clone()).ok()),
            )
            .await?;
        if before.is_some() || after.is_some() {
            result["before"] = json!(before);
            result["after"] = json!(after);
        }
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
                let count:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_messages.conversations c WHERE tenant_id=$1 AND id=ANY($2) AND status='active' AND chat_type='group' AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger l WHERE l.tenant_key=$1 AND l.kind='group' AND l.object_ref=c.id::text AND l.status<>'active')")
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
            let old = sqlx::query("SELECT a.person_id,a.role_id,a.scope_id,a.valid_until,a.proxy_for_id,c.agent_key,c.domain_key,c.duty_id FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND c.id=$2 AND c.status='active'")
                .bind(&self.tenant).bind(id).fetch_optional(&mut **tx).await?
                .ok_or_else(|| anyhow::anyhow!("collaboration_not_active"))?;
            let unchanged = old.get::<Uuid, _>("person_id") == a.person
                && old.get::<Uuid, _>("role_id") == a.role
                && old.get::<Uuid, _>("scope_id") == a.scope
                && old.get::<String, _>("agent_key") == a.agent
                && old.get::<String, _>("domain_key") == a.domain
                && old.get::<Option<Uuid>, _>("duty_id") == a.duty
                && old.get::<Option<DateTime<Utc>>, _>("valid_until") == a.valid_until
                && old.get::<Option<Uuid>, _>("proxy_for_id") == a.proxy_for;
            if !unchanged {
                self.end_connection(tx, actor, p, id, now).await?;
                let updated_policy = self.policy(tx, now).await?;
                let mut replacement = a.clone();
                replacement.collaboration = None;
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
        let existing=sqlx::query("SELECT id,valid_until,proxy_for_id FROM qintopia_agent_os.collaboration_appointments WHERE tenant_key=$1 AND person_id=$2 AND role_id=$3 AND scope_id=$4 AND status='active'")
            .bind(&self.tenant).bind(a.person).bind(a.role).bind(a.scope).fetch_optional(&mut **tx).await?;
        let appointment = if let Some(r) = existing {
            ensure!(
                r.get::<Option<DateTime<Utc>>, _>("valid_until") == a.valid_until
                    && r.get::<Option<Uuid>, _>("proxy_for_id") == a.proxy_for,
                "existing_term_differs"
            );
            r.get::<Uuid, _>("id")
        } else {
            sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_appointments(tenant_key,person_id,role_id,scope_id,valid_from,valid_until,proxy_for_id) VALUES($1,$2,$3,$4,$5,$6,$7) RETURNING id")
                .bind(&self.tenant).bind(a.person).bind(a.role).bind(a.scope).bind(now).bind(a.valid_until).bind(a.proxy_for).fetch_one(&mut **tx).await?
        };
        let relation = if let Some(id) = a.collaboration {
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.agent_collaborations WHERE tenant_key=$1 AND id=$2 AND appointment_id=$3 AND agent_key=$4 AND domain_key=$5 AND status='active')")
                .bind(&self.tenant).bind(id).bind(appointment).bind(&a.agent).bind(&a.domain).fetch_one(&mut **tx).await?;
            ensure!(valid, "collaboration_dimensions_changed");
            for g in p
                .grants
                .iter()
                .filter(|g| g.collaboration == id && g.active)
            {
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
                .find(|g| g.collaboration == relation && g.action == *action && g.active)
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
        let visible: Vec<Uuid> = p
            .scopes
            .iter()
            .filter(|s| {
                Self::catalog_admin(&p, actor)
                    || p.grants.iter().any(|g| {
                        g.person == actor.person
                            && g.action == "manage"
                            && g.mode == PermissionMode::Autonomous
                            && g.reviewer.is_none()
                            && p.effective(g)
                            && p.in_scope(s.id, g.scope, g.descendants)
                    })
            })
            .map(|s| s.id)
            .collect();
        ensure!(!visible.is_empty(), "management_denied");
        let people:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(x ORDER BY x->>'label'),'[]') FROM (SELECT DISTINCT jsonb_build_object('id',p.id,'label',coalesce(p.preferred_name,p.display_name),'display_name',p.display_name) x FROM qintopia_identity.persons p JOIN qintopia_identity.source_identity_links l ON l.person_id=p.id WHERE l.namespace=$1 AND l.status='confirmed' AND p.status='active') s")
            .bind(&self.tenant).fetch_one(&mut *tx).await?;
        let scopes:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'parent',parent_scope_id,'label',label,'kind',kind,'version',version,'status',status) ORDER BY label),'[]') FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND id=ANY($2)")
            .bind(&self.tenant).bind(&visible).fetch_one(&mut *tx).await?;
        let roles:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'label',label,'description',description,'available_actions',available_actions,'status',status,'version',version,'duty_ids',ARRAY(SELECT duty_id FROM qintopia_agent_os.collaboration_role_duties rd WHERE rd.tenant_key=r.tenant_key AND rd.role_id=r.id ORDER BY duty_id)) ORDER BY label,id),'[]') FROM qintopia_agent_os.collaboration_roles r WHERE tenant_key=$1")
            .bind(&self.tenant).fetch_one(&mut *tx).await?;
        let duties:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'label',label,'description',description,'domain',domain_key,'available_actions',available_actions,'status',status,'version',version) ORDER BY label),'[]') FROM qintopia_agent_os.collaboration_duties WHERE tenant_key=$1")
            .bind(&self.tenant).fetch_one(&mut *tx).await?;
        let relations:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('id',c.id,'appointment',a.id,'person',a.person_id,'role',a.role_id,'scope',a.scope_id,'agent',c.agent_key,'domain',c.domain_key,'duty',c.duty_id,'version',c.version,'replaces',c.replaces_id,'immutable',EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_grants g WHERE g.collaboration_id=c.id AND g.parent_grant_id IS NULL),'responsibility',c.responsibility_text,'status',CASE WHEN c.status<>'active' THEN c.status WHEN a.status<>'active' THEN a.status WHEN a.valid_until<=$3 THEN 'expired' ELSE 'active' END,'valid_until',a.valid_until,'proxy_for',a.proxy_for_id) ORDER BY c.created_at),'[]') FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND a.scope_id=ANY($2)")
            .bind(&self.tenant).bind(&visible).bind(now).fetch_one(&mut *tx).await?;
        let groups:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('id',id,'label',display_name) ORDER BY display_name),'[]') FROM qintopia_messages.conversations WHERE tenant_id=$1 AND chat_type='group' AND status='active'")
            .bind(&self.tenant).fetch_one(&mut *tx).await?;
        let bindings:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('scope',scope_id,'conversation',conversation_id) ORDER BY scope_id,conversation_id),'[]') FROM qintopia_agent_os.collaboration_scope_bindings WHERE tenant_key=$1 AND scope_id=ANY($2) AND revoked_at IS NULL")
            .bind(&self.tenant).bind(&visible).fetch_one(&mut *tx).await?;
        let grants:Vec<Value>=p.grants.iter().filter(|g|p.can_inspect(actor.person,g.scope,&g.agent,&g.domain)).map(|g|json!({"id":g.id,"collaboration":g.collaboration,"person":g.person,"scope":g.scope,"agent":g.agent,"domain":g.domain,"duty":g.duty,"action":g.action,"mode":g.mode,"reviewer":g.reviewer,"effective":p.effective(g),"revocable":g.active,"management_envelope":if g.action=="manage" && g.mode==PermissionMode::Autonomous{Some(&g.delegation)}else{None}})).collect();
        let relations: Vec<Value> = relations
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| {
                r["scope"]
                    .as_str()
                    .and_then(|s| Uuid::parse_str(s).ok())
                    .is_some_and(|scope| {
                        p.can_inspect(
                            actor.person,
                            scope,
                            r["agent"].as_str().unwrap_or(""),
                            r["domain"].as_str().unwrap_or(""),
                        )
                    })
            })
            .cloned()
            .collect();
        let root_manager = p.scopes.iter().filter(|s| s.parent.is_none()).any(|s| {
            p.manager(actor.person, s.id, "default", "organization", "manage")
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
            .organization_state(&mut tx, &visible, root_manager)
            .await?;
        // Scope visibility alone does not expose another Agent's contact configuration.
        organization["audiences"]
            .as_array_mut()
            .unwrap()
            .retain(|a| relations.iter().any(|r| r["id"] == a["collaboration"]));
        Ok(
            json!({"version":version,"people":people,"scopes":scopes,"roles":roles,"duties":duties,"relations":relations,"agents":agents(),"domains":DOMAINS,"actions":ACTIONS,"grants":grants,"groups":groups,"bindings":bindings,"organization":organization,"catalog_admin":Self::catalog_admin(&p,actor),"mode":"synthetic","runtime_connected":false}),
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
