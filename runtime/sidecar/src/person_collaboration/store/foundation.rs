//! Governed local consumers. All mutations share the collaboration tenant lock.
use super::{Actor, Store};
use crate::person_collaboration::{digest, model::*};
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub(super) async fn load_policy(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    identity_namespace: &str,
    now: DateTime<Utc>,
) -> Result<Policy> {
    let rows=sqlx::query("SELECT id,parent_scope_id,status FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1")
            .bind(tenant).fetch_all(&mut **tx).await?;
    let scopes = rows
        .iter()
        .map(|r| Scope {
            id: r.get("id"),
            parent: r.get("parent_scope_id"),
            active: r.get::<String, _>("status") == "active",
        })
        .collect();
    let rows=sqlx::query("SELECT g.*,a.person_id,a.scope_id,c.agent_key,c.domain_key,c.duty_id,(g.status='active' AND c.status='active' AND a.status='active' AND p.status='active' AND role.status='active' AND (c.duty_id IS NULL OR duty.status='active') AND a.valid_from<=$2 AND (a.valid_until IS NULL OR a.valid_until>$2) AND EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l WHERE l.person_id=a.person_id AND l.namespace=$3 AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL)) AS effective_now FROM qintopia_agent_os.collaboration_grants g JOIN qintopia_agent_os.agent_collaborations c ON c.id=g.collaboration_id JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id JOIN qintopia_identity.persons p ON p.id=a.person_id JOIN qintopia_agent_os.collaboration_roles role ON role.id=a.role_id LEFT JOIN qintopia_agent_os.collaboration_duties duty ON duty.id=c.duty_id WHERE g.tenant_key=$1")
            .bind(tenant).bind(now).bind(identity_namespace).fetch_all(&mut **tx).await?;
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

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Authority {
    pub status: String,
    pub reviewer: Option<Uuid>,
    pub grant_id: Option<Uuid>,
    pub collaboration_id: Option<Uuid>,
    pub configuration_version: i64,
}

pub async fn authorize_current(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    person: Uuid,
    scope: Uuid,
    agent: &str,
    domain: &str,
    action: &str,
) -> Result<Authority> {
    ensure!(
        agents().contains(&agent) && DOMAINS.contains(&domain) && ACTIONS.contains(&action),
        "unknown_permission_dimension"
    );
    let row = sqlx::query("SELECT version,mode,initialized FROM qintopia_agent_os.collaboration_tenants WHERE tenant_key=$1 FOR UPDATE")
        .bind(tenant).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("tenant_not_initialized"))?;
    ensure!(
        tenant.starts_with("synthetic-collaboration-")
            && row.get::<String, _>("mode") == "synthetic"
            && row.get::<bool, _>("initialized"),
        "synthetic_tenant_required"
    );
    let now = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let policy = load_policy(tx, tenant, tenant, now).await?;
    let matching: Vec<_> = policy
        .grants
        .iter()
        .filter(|g| {
            g.person == person
                && g.agent == agent
                && g.domain == domain
                && g.action == action
                && g.active
                && policy.in_scope(scope, g.scope, g.descendants)
        })
        .collect();
    let mut result = Authority {
        status: "denied".into(),
        reviewer: None,
        grant_id: None,
        collaboration_id: None,
        configuration_version: row.get("version"),
    };
    if let [g] = matching.as_slice() {
        let d = policy.decision(g.collaboration, action);
        result.status = d["status"].as_str().unwrap_or("denied").into();
        result.reviewer = serde_json::from_value(d["reviewer"].clone()).unwrap_or(None);
        if result.status != "denied" {
            result.grant_id = Some(g.id);
            result.collaboration_id = Some(g.collaboration);
        }
    }
    Ok(result)
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub struct KnowledgeWrite {
    pub operation_id: Uuid,
    pub expected_version: i64,
    pub scope: Uuid,
    pub key: String,
    pub kind: String,
    #[serde(default)]
    pub shared: bool,
    #[serde(default)]
    pub case_ref: Option<Uuid>,
    pub content: Value,
    #[serde(default)]
    pub effective_at: Option<DateTime<Utc>>,
    #[serde(default)]
    pub effective_until: Option<DateTime<Utc>>,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct KnowledgeVersion {
    pub id: Uuid,
    pub version: i32,
    pub scope: Uuid,
    pub key: String,
    pub content: Value,
    pub author: Uuid,
    pub effective_at: DateTime<Utc>,
    pub effective_until: Option<DateTime<Utc>>,
    pub authority_grant: Uuid,
}

pub(super) async fn receipt(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    person: Uuid,
    operation: Uuid,
    hash: &str,
) -> Result<Option<Value>> {
    let r=sqlx::query("SELECT person_id,request_hash,result FROM qintopia_agent_os.collaboration_tool_receipts WHERE tenant_key=$1 AND operation_id=$2")
        .bind(tenant).bind(operation).fetch_optional(&mut **tx).await?;
    r.map(|r| {
        ensure!(
            r.get::<Uuid, _>("person_id") == person && r.get::<String, _>("request_hash") == hash,
            "idempotency_conflict"
        );
        Ok(r.get("result"))
    })
    .transpose()
}
pub(super) async fn record_receipt(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    person: Uuid,
    operation: Uuid,
    hash: &str,
    result: &Value,
) -> Result<()> {
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_tool_receipts(tenant_key,operation_id,person_id,request_hash,result) VALUES($1,$2,$3,$4,$5)")
        .bind(tenant).bind(operation).bind(person).bind(hash).bind(result).execute(&mut **tx).await?;
    Ok(())
}

pub async fn put_knowledge_in(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    person: Uuid,
    write: &KnowledgeWrite,
) -> Result<KnowledgeVersion> {
    let authority = authorize_current(
        tx,
        tenant,
        person,
        write.scope,
        "erhua",
        "community_service",
        if write.kind == "rule" || write.kind == "principle" {
            "change_rules"
        } else {
            "confirm_knowledge"
        },
    )
    .await?;
    ensure!(
        authority.status == "autonomous",
        if authority.status == "confirmation_required" {
            "confirmation_required"
        } else {
            "scope_access_denied"
        }
    );
    put_authorized_knowledge(tx, tenant, person, write, authority.grant_id.unwrap()).await
}

pub(super) async fn put_authorized_knowledge(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    person: Uuid,
    write: &KnowledgeWrite,
    grant: Uuid,
) -> Result<KnowledgeVersion> {
    ensure!(
        write.key.len() <= 80
            && write.key.starts_with(|c: char| c.is_ascii_lowercase())
            && write
                .key
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c)),
        "invalid_knowledge_key"
    );
    ensure!(
        ["rule", "fact", "culture", "experience", "principle"].contains(&write.kind.as_str())
            && write.content.is_object()
            && serde_json::to_vec(&write.content)?.len() <= 16000,
        "invalid_knowledge_content"
    );
    let hash = digest(&serde_json::to_vec(write)?);
    if let Some(result) = receipt(tx, tenant, person, write.operation_id, &hash).await? {
        return Ok(serde_json::from_value(result)?);
    }
    let scope=sqlx::query("SELECT kind FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND id=$2 AND status='active'")
        .bind(tenant).bind(write.scope).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("scope_access_denied"))?;
    ensure!(
        !write.shared || scope.get::<String, _>("kind") == "community",
        "community_sharing_requires_community_scope"
    );
    // A local item cannot silently override an inherited principle with the same key.
    let inherited:bool=sqlx::query_scalar("WITH RECURSIVE ancestors AS (SELECT parent_scope_id FROM qintopia_agent_os.collaboration_scopes WHERE id=$2 AND tenant_key=$1 UNION ALL SELECT s.parent_scope_id FROM qintopia_agent_os.collaboration_scopes s JOIN ancestors a ON a.parent_scope_id=s.id WHERE s.tenant_key=$1) SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_knowledge_items i JOIN ancestors a ON a.parent_scope_id=i.scope_id WHERE i.tenant_key=$1 AND i.knowledge_key=$3 AND i.kind='principle' AND i.shared)")
        .bind(tenant).bind(write.scope).bind(&write.key).fetch_one(&mut **tx).await?;
    ensure!(!inherited, "inherited_principle_cannot_be_overridden");
    let mut item=sqlx::query("SELECT id,space_id,definition_key,version,kind,shared,stopped_at FROM qintopia_agent_os.collaboration_knowledge_items WHERE tenant_key=$1 AND scope_id=$2 AND knowledge_key=$3 AND case_ref IS NOT DISTINCT FROM $4 FOR UPDATE")
        .bind(tenant).bind(write.scope).bind(&write.key).bind(write.case_ref).fetch_optional(&mut **tx).await?;
    if item.is_none() {
        ensure!(write.expected_version == 0, "knowledge_version_conflict");
        let space:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type,display_name) VALUES($1,'synthetic',$2,'group','受控知识存储（无消息通道）') RETURNING id")
            .bind(tenant).bind(format!("foundation-knowledge-{}",Uuid::new_v4())).fetch_one(&mut **tx).await?;
        item=Some(sqlx::query("INSERT INTO qintopia_agent_os.collaboration_knowledge_items(tenant_key,scope_id,knowledge_key,case_ref,space_id,definition_key,kind,shared) VALUES($1,$2,$3,$4,$5,$6,$7,$8) RETURNING id,space_id,definition_key,version,kind,shared,stopped_at")
            .bind(tenant).bind(write.scope).bind(&write.key).bind(write.case_ref).bind(space).bind(format!("foundation.{}",write.key)).bind(&write.kind).bind(write.shared).fetch_one(&mut **tx).await?);
    }
    let item = item.unwrap();
    ensure!(
        item.get::<Option<DateTime<Utc>>, _>("stopped_at").is_none(),
        "knowledge_stopped"
    );
    ensure!(
        i64::from(item.get::<i32, _>("version")) == write.expected_version,
        "knowledge_version_conflict"
    );
    ensure!(
        item.get::<String, _>("kind") == write.kind
            && item.get::<bool, _>("shared") == write.shared,
        "knowledge_ownership_immutable"
    );
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    let effective = write.effective_at.unwrap_or(now).max(now);
    ensure!(
        write.effective_until.is_none_or(|u| u > effective),
        "invalid_effective_interval"
    );
    let version = item.get::<i32, _>("version") + 1;
    let body = json!({"foundation_knowledge":true,"content":write.content});
    let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_definition_versions(space_id,definition_key,version,execution_mode,definition,allowed_capabilities,approval_policy,status,definition_digest,created_by_person_id) VALUES($1,$2,$3,'deterministic',$4,'{}','none','shadow',$5,$6) RETURNING id")
        .bind(item.get::<Uuid,_>("space_id")).bind(item.get::<String,_>("definition_key")).bind(version).bind(&body).bind(digest(&serde_json::to_vec(&body)?)).bind(person).fetch_one(&mut **tx).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_knowledge_revisions(id,tenant_key,item_id,author_person_id,authority_grant_id,effective_at,effective_until) VALUES($1,$2,$3,$4,$5,$6,$7)")
        .bind(id).bind(tenant).bind(item.get::<Uuid,_>("id")).bind(person).bind(grant).bind(effective).bind(write.effective_until).execute(&mut **tx).await?;
    sqlx::query(
        "UPDATE qintopia_agent_os.collaboration_knowledge_items SET version=$2 WHERE id=$1",
    )
    .bind(item.get::<Uuid, _>("id"))
    .bind(version)
    .execute(&mut **tx)
    .await?;
    let result = KnowledgeVersion {
        id,
        version,
        scope: write.scope,
        key: write.key.clone(),
        content: write.content.clone(),
        author: person,
        effective_at: effective,
        effective_until: write.effective_until,
        authority_grant: grant,
    };
    record_receipt(
        tx,
        tenant,
        person,
        write.operation_id,
        &hash,
        &serde_json::to_value(&result)?,
    )
    .await?;
    Ok(result)
}

pub async fn effective_knowledge_in(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    scope: Uuid,
    key: &str,
    case_ref: Option<Uuid>,
) -> Result<Option<KnowledgeVersion>> {
    let row=sqlx::query("SELECT r.id,d.version,i.scope_id,i.knowledge_key,d.definition,r.author_person_id,r.authority_grant_id,r.effective_at,r.effective_until,(r.effective_until IS NULL OR r.effective_until>clock_timestamp()) AS still_effective FROM qintopia_agent_os.collaboration_knowledge_items i JOIN qintopia_agent_os.collaboration_knowledge_revisions r ON r.item_id=i.id JOIN qintopia_agent_os.business_definition_versions d ON d.id=r.id WHERE i.tenant_key=$1 AND i.scope_id=$2 AND i.knowledge_key=$3 AND (i.case_ref IS NULL OR i.case_ref=$4) AND i.stopped_at IS NULL AND r.withdrawn_at IS NULL AND d.status='shadow' AND r.effective_at<=clock_timestamp() AND (i.lifecycle_managed OR r.effective_until IS NULL OR r.effective_until>clock_timestamp()) ORDER BY (i.case_ref IS NOT NULL) DESC,r.effective_at DESC,d.version DESC LIMIT 1")
        .bind(tenant).bind(scope).bind(key).bind(case_ref).fetch_optional(&mut **tx).await?;
    Ok(row
        .filter(|r| r.get::<bool, _>("still_effective"))
        .map(|r| KnowledgeVersion {
            id: r.get("id"),
            version: r.get("version"),
            scope: r.get("scope_id"),
            key: r.get("knowledge_key"),
            content: r.get::<Value, _>("definition")["content"].clone(),
            author: r.get("author_person_id"),
            effective_at: r.get("effective_at"),
            effective_until: r.get("effective_until"),
            authority_grant: r.get("authority_grant_id"),
        }))
}

impl Store {
    pub(crate) async fn foundation_existing_turn_input(
        &self,
        actor: &Actor,
        operation: Uuid,
        hash: &str,
    ) -> Result<Option<Value>> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let row=sqlx::query("SELECT person_id,command_hash,command FROM qintopia_agent_os.collaboration_turn_sources WHERE tenant_key=$1 AND message_ref=$2 AND command IS NOT NULL")
            .bind(&self.tenant).bind(operation).fetch_optional(&mut *tx).await?;
        row.map(|r| {
            ensure!(
                r.get::<Uuid, _>("person_id") == actor.person
                    && r.get::<Option<String>, _>("command_hash").as_deref() == Some(hash),
                "idempotency_conflict"
            );
            Ok(r.get("command"))
        })
        .transpose()
    }

    pub(crate) async fn foundation_turn_input(
        &self,
        actor: &Actor,
        operation: Uuid,
        hash: &str,
        input: &Value,
    ) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let row=sqlx::query("INSERT INTO qintopia_agent_os.collaboration_turn_sources(tenant_key,message_ref,person_id,command_hash,command) VALUES($1,$2,$3,$4,$5) ON CONFLICT(tenant_key,message_ref) DO UPDATE SET command_hash=COALESCE(collaboration_turn_sources.command_hash,EXCLUDED.command_hash),command=COALESCE(collaboration_turn_sources.command,EXCLUDED.command) RETURNING person_id,command_hash,command")
            .bind(&self.tenant).bind(operation).bind(actor.person).bind(hash).bind(input).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<Uuid, _>("person_id") == actor.person
                && row.get::<Option<String>, _>("command_hash").as_deref() == Some(hash),
            "idempotency_conflict"
        );
        let result = row.get("command");
        tx.commit().await?;
        Ok(result)
    }

    pub(crate) async fn knowledge_withdraw(
        &self,
        actor: &Actor,
        operation: Uuid,
        scope: Uuid,
        revision: Uuid,
        expected_version: i64,
    ) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        if let Some((_, _, gateway_scope)) = actor.gateway {
            ensure!(scope == gateway_scope, "gateway_scope_mismatch");
        }
        let kind:String=sqlx::query_scalar("SELECT i.kind FROM qintopia_agent_os.collaboration_knowledge_items i JOIN qintopia_agent_os.collaboration_knowledge_revisions r ON r.item_id=i.id WHERE r.id=$1 AND i.tenant_key=$2 AND i.scope_id=$3").bind(revision).bind(&self.tenant).bind(scope).fetch_one(&mut *tx).await?;
        let auth = authorize_current(
            &mut tx,
            &self.tenant,
            actor.person,
            scope,
            "erhua",
            "community_service",
            if kind == "rule" || kind == "principle" {
                "change_rules"
            } else {
                "confirm_knowledge"
            },
        )
        .await?;
        ensure!(auth.status == "autonomous", "scope_access_denied");
        let hash = digest(&serde_json::to_vec(
            &json!({"action":"withdraw","scope":scope,"revision":revision,"expected_version":expected_version}),
        )?);
        if let Some(result) = receipt(&mut tx, &self.tenant, actor.person, operation, &hash).await?
        {
            return Ok(result);
        }
        let row=sqlx::query("SELECT i.id,i.version,i.lifecycle_managed,r.withdrawn_at FROM qintopia_agent_os.collaboration_knowledge_revisions r JOIN qintopia_agent_os.collaboration_knowledge_items i ON i.id=r.item_id WHERE r.id=$1 AND i.tenant_key=$2 AND i.scope_id=$3 FOR UPDATE OF i,r")
            .bind(revision).bind(&self.tenant).bind(scope).fetch_one(&mut *tx).await?;
        ensure!(
            !row.get::<bool, _>("lifecycle_managed"),
            "use_rule_lifecycle"
        );
        ensure!(
            i64::from(row.get::<i32, _>("version")) == expected_version,
            "knowledge_version_conflict"
        );
        ensure!(
            row.get::<Option<DateTime<Utc>>, _>("withdrawn_at")
                .is_none(),
            "knowledge_already_withdrawn"
        );
        sqlx::query("UPDATE qintopia_agent_os.collaboration_knowledge_revisions SET withdrawn_at=clock_timestamp() WHERE id=$1").bind(revision).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.collaboration_knowledge_items SET version=version+1 WHERE id=$1").bind(row.get::<Uuid,_>("id")).execute(&mut *tx).await?;
        let result = json!({"withdrawn":true,"revision_id":revision,"latest_version":expected_version+1,"external_effects":false});
        record_receipt(
            &mut tx,
            &self.tenant,
            actor.person,
            operation,
            &hash,
            &result,
        )
        .await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn foundation_context(
        &self,
        actor: &Actor,
        scope: Uuid,
        topic: &str,
    ) -> Result<Value> {
        let started = std::time::Instant::now();
        let (mut tx, version, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let policy = self.policy(&mut tx, now).await?;
        self.read_scope(&policy, actor, scope)?;
        let rules = knowledge_context(&mut tx, &self.tenant, scope).await?;
        let knowledge_items:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('key',knowledge_key,'scope',scope_id,'latest_version',version,'kind',kind,'shared',shared) FROM qintopia_agent_os.collaboration_knowledge_items WHERE tenant_key=$1 AND scope_id=$2 AND case_ref IS NULL ORDER BY knowledge_key")
            .bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
        let later_revisions:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('id',r.id,'key',i.knowledge_key,'scope',i.scope_id,'version',d.version,'latest_version',i.version,'content',d.definition->'content','effective_at',r.effective_at,'effective_until',r.effective_until,'kind',i.kind,'shared',i.shared) FROM qintopia_agent_os.collaboration_knowledge_items i JOIN qintopia_agent_os.collaboration_knowledge_revisions r ON r.item_id=i.id JOIN qintopia_agent_os.business_definition_versions d ON d.id=r.id WHERE i.tenant_key=$1 AND i.scope_id=$2 AND i.case_ref IS NULL AND r.withdrawn_at IS NULL AND r.effective_at>clock_timestamp() ORDER BY r.effective_at")
            .bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
        let permissions:Vec<_>=policy.grants.iter().filter(|g|g.person==actor.person&&g.agent=="erhua"&&g.domain=="community_service"&&policy.in_scope(scope,g.scope,g.descendants)&&policy.effective(g)).map(|g|json!({"action":g.action,"decision":policy.decision(g.collaboration,&g.action)})).collect();
        tx.commit().await?;
        let identity = self.identity_context(actor).await?;
        let memory = self.memory_context(actor, topic).await?;
        Ok(
            json!({"identity":identity,"scope":scope,"configuration_version":version,"knowledge":rules,"knowledge_items":knowledge_items,"later_revisions":later_revisions,"permissions":permissions,"memory":memory,"consumer":"erhua","runtime":"local_controlled_tools","query_ms":started.elapsed().as_millis(),"notification":"未配置群通知；保存回执仅返回当前对话"}),
        )
    }

    pub(super) fn read_scope(&self, policy: &Policy, actor: &Actor, scope: Uuid) -> Result<()> {
        if let Some((_, _, gateway_scope)) = actor.gateway {
            ensure!(scope == gateway_scope, "gateway_scope_mismatch");
        }
        ensure!(
            policy.can_inspect(actor.person, scope, "erhua", "community_service")
                || policy.grants.iter().any(|g| g.person == actor.person
                    && g.agent == "erhua"
                    && g.domain == "community_service"
                    && policy.in_scope(scope, g.scope, g.descendants)
                    && policy.effective(g)),
            "scope_access_denied"
        );
        Ok(())
    }

    pub async fn knowledge_save(
        &self,
        actor: &Actor,
        write: &KnowledgeWrite,
        defer: bool,
    ) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        if let Some((_, _, scope)) = actor.gateway {
            ensure!(write.scope == scope, "gateway_scope_mismatch");
        }
        let auth = authorize_current(
            &mut tx,
            &self.tenant,
            actor.person,
            write.scope,
            "erhua",
            "community_service",
            if write.kind == "rule" || write.kind == "principle" {
                "change_rules"
            } else {
                "confirm_knowledge"
            },
        )
        .await?;
        ensure!(auth.status != "denied", "scope_access_denied");
        if auth.status == "confirmation_required" || defer {
            let result = self.queue_rule(&mut tx, actor, write, &auth).await?;
            tx.commit().await?;
            return Ok(result);
        }
        let hash = digest(&serde_json::to_vec(write)?);
        if let Some(result) = receipt(
            &mut tx,
            &self.tenant,
            actor.person,
            write.operation_id,
            &hash,
        )
        .await?
        {
            return Ok(
                json!({"status":"saved","persisted":true,"knowledge":result,"replayed":true,"current_state_requires_read":true,"notification_status":"not_configured"}),
            );
        }
        let result = put_authorized_knowledge(
            &mut tx,
            &self.tenant,
            actor.person,
            write,
            auth.grant_id.unwrap(),
        )
        .await?;
        tx.commit().await?;
        Ok(
            json!({"status":"saved","persisted":true,"knowledge":result,"notification_status":"not_configured"}),
        )
    }

    async fn queue_rule(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        write: &KnowledgeWrite,
        auth: &Authority,
    ) -> Result<Value> {
        let body = serde_json::to_value(write)?;
        self.queue_work(
            tx,
            actor,
            write.operation_id,
            write.scope,
            "erhua.foundation_rule",
            &body,
            auth.grant_id.unwrap(),
            if auth.status == "confirmation_required" {
                "awaiting_review"
            } else {
                "queued"
            },
        )
        .await
    }

    #[allow(
        clippy::too_many_arguments,
        reason = "exact work identity and authority are retained in one transaction"
    )]
    pub(super) async fn queue_work(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        operation: Uuid,
        scope: Uuid,
        capability: &str,
        input: &Value,
        grant: Uuid,
        status: &str,
    ) -> Result<Value> {
        let hash = digest(&serde_json::to_vec(
            &json!({"capability":capability,"scope":scope,"input":input}),
        )?);
        let idempotency = format!("foundation/{}/{operation}", self.tenant);
        if let Some(row)=sqlx::query("SELECT w.id,w.status,r.person_id,r.request_hash FROM qintopia_agent_os.work_items w JOIN qintopia_agent_os.collaboration_work_requests r ON r.work_item_id=w.id WHERE w.idempotency_key=$1")
            .bind(&idempotency).fetch_optional(&mut **tx).await? {
            ensure!(row.get::<Uuid,_>("person_id")==actor.person&&row.get::<String,_>("request_hash")==hash,"idempotency_conflict");
            return Ok(json!({"work_item_id":row.get::<Uuid,_>("id"),"status":row.get::<String,_>("status"),"persisted":true,"replayed":true}));
        }
        let work_type = if capability == "erhua.foundation_rule" {
            "foundation_rule_request"
        } else {
            "foundation_context_request"
        };
        let payload = json!({"input":input,"identity_namespace":actor.identity_namespace,"gateway":actor.gateway,"session_hash":actor.session_hash});
        let work:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.work_items(work_item_type,status,requester_agent,target_agent,capability_key,brief_summary,purpose,dedupe_key,idempotency_key,payload,metadata) VALUES($1,$2,$3,'erhua',$4,'本栋受控工作','synthetic_foundation',$5,$5,$6,'{\"local_only\":true}') RETURNING id")
            .bind(work_type).bind(status).bind(if capability=="erhua.foundation_rule"{"erhua"}else{"default"}).bind(capability).bind(&idempotency).bind(payload).fetch_one(&mut **tx).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_work_requests(work_item_id,tenant_key,scope_id,actor_identity_id,identity_version,person_id,authority_grant_id,request_hash) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
            .bind(work).bind(&self.tenant).bind(scope).bind(actor.link).bind(actor.identity_version).bind(actor.person).bind(grant).bind(hash).execute(&mut **tx).await?;
        work_event(
            tx,
            work,
            "accepted",
            "erhua",
            &json!({"person":actor.person,"status":status}),
        )
        .await?;
        Ok(json!({"work_item_id":work,"status":status,"persisted":true,"replayed":false}))
    }

    pub async fn dispatch_context(
        &self,
        actor: &Actor,
        scope: Uuid,
        operation: Uuid,
        expected_version: i64,
        brief: &str,
    ) -> Result<Value> {
        plain_text(brief, 1000)?;
        if let Some((_, _, bound)) = actor.gateway {
            ensure!(scope == bound, "gateway_scope_mismatch");
        }
        let (mut tx, version, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        ensure!(
            version == expected_version,
            "configuration_version_conflict"
        );
        let policy = self.policy(&mut tx, now).await?;
        let manager = policy
            .manager(actor.person, scope, "erhua", "community_service", "train")
            .ok_or_else(|| anyhow::anyhow!("management_delegation_required"))?;
        let result = self
            .queue_work(
                &mut tx,
                actor,
                operation,
                scope,
                "erhua.foundation_context",
                &json!({"brief":brief}),
                manager.id,
                "queued",
            )
            .await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn foundation_work_status(&self, actor: &Actor, work: Uuid) -> Result<Value> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let row=sqlx::query("SELECT w.status,w.target_agent,w.last_error,r.scope_id,r.person_id,r.result FROM qintopia_agent_os.work_items w JOIN qintopia_agent_os.collaboration_work_requests r ON r.work_item_id=w.id WHERE r.tenant_key=$1 AND w.id=$2")
            .bind(&self.tenant).bind(work).fetch_one(&mut *tx).await?;
        let policy = self.policy(&mut tx, now).await?;
        self.read_scope(&policy, actor, row.get("scope_id"))?;
        Ok(
            json!({"work_item_id":work,"status":row.get::<String,_>("status"),"target_agent":row.get::<String,_>("target_agent"),"reason":row.get::<Option<String>,_>("last_error"),"result":row.get::<Option<Value>,_>("result")}),
        )
    }

    pub async fn foundation_approve_rule(&self, actor: &Actor, work: Uuid) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let row=sqlx::query("SELECT r.*,w.status,w.capability_key,w.payload FROM qintopia_agent_os.collaboration_work_requests r JOIN qintopia_agent_os.work_items w ON w.id=r.work_item_id WHERE r.tenant_key=$1 AND r.work_item_id=$2 FOR UPDATE OF r,w")
            .bind(&self.tenant).bind(work).fetch_one(&mut *tx).await?;
        if let Some((_, _, scope)) = actor.gateway {
            ensure!(
                scope == row.get::<Uuid, _>("scope_id"),
                "gateway_scope_mismatch"
            );
        }
        ensure!(
            row.get::<String, _>("status") == "awaiting_review"
                && row.get::<String, _>("capability_key") == "erhua.foundation_rule",
            "review_not_pending"
        );
        let payload: Value = row.get("payload");
        let permission = super::rule_lifecycle::input_action(&payload["input"]);
        let auth = authorize_current(
            &mut tx,
            &self.tenant,
            row.get("person_id"),
            row.get("scope_id"),
            "erhua",
            "community_service",
            permission,
        )
        .await?;
        ensure!(
            auth.status == "confirmation_required"
                && auth.reviewer == Some(actor.person)
                && auth.grant_id == Some(row.get("authority_grant_id")),
            "designated_confirmation_required"
        );
        let reviewer = authorize_current(
            &mut tx,
            &self.tenant,
            actor.person,
            row.get("scope_id"),
            "erhua",
            "community_service",
            permission,
        )
        .await?;
        ensure!(
            reviewer.status == "autonomous" && reviewer.grant_id.is_some(),
            "designated_confirmation_required"
        );
        // A later appointment for the same person is new authority, never a
        // resurrection of this approval. Preserve exact identity and grant proof.
        let proof = json!({"person_id":actor.person,"identity_link_id":actor.link,"identity_version":actor.identity_version,"identity_namespace":actor.identity_namespace,"gateway":actor.gateway,"session_hash":actor.session_hash,"grant_id":reviewer.grant_id});
        sqlx::query("UPDATE qintopia_agent_os.collaboration_work_requests SET approval_person_id=$2,approved_input_hash=request_hash WHERE work_item_id=$1").bind(work).bind(actor.person).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items SET status='queued',payload=jsonb_set(payload,'{approval_authority}',$2) WHERE id=$1").bind(work).bind(proof).execute(&mut *tx).await?;
        work_event(
            &mut tx,
            work,
            "exact_rule_approved",
            "human",
            &json!({"person":actor.person}),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"work_item_id":work,"status":"queued","approved":true}))
    }

    pub async fn foundation_execute_work(&self, work: Uuid) -> Result<Value> {
        let (mut tx, _, now) = self.begin().await?;
        let row=sqlx::query("SELECT r.*,w.payload,w.status,w.capability_key,w.target_agent FROM qintopia_agent_os.collaboration_work_requests r JOIN qintopia_agent_os.work_items w ON w.id=r.work_item_id WHERE r.tenant_key=$1 AND r.work_item_id=$2 FOR UPDATE OF r,w")
            .bind(&self.tenant).bind(work).fetch_one(&mut *tx).await?;
        let status: String = row.get("status");
        if ["completed", "failed", "cancelled"].contains(&status.as_str()) {
            return Ok(json!({"work_item_id":work,"status":status,"replayed":true}));
        }
        ensure!(status == "queued", "review_required");
        let payload: Value = row.get("payload");
        let mut actor = Actor {
            link: row.get("actor_identity_id"),
            person: row.get("person_id"),
            identity_version: row.get("identity_version"),
            identity_namespace: serde_json::from_value(payload["identity_namespace"].clone())?,
            gateway: serde_json::from_value(payload["gateway"].clone())?,
            session_hash: serde_json::from_value(payload["session_hash"].clone())?,
            tenant: self.tenant.clone(),
        };
        let scope: Uuid = row.get("scope_id");
        let capability: String = row.get("capability_key");
        sqlx::query("SAVEPOINT foundation_execution")
            .execute(&mut *tx)
            .await?;
        let result:Result<Value>=async {
            let lifecycle=capability=="erhua.foundation_rule" && payload["input"].get("lifecycle").is_some();
            if lifecycle {
                if let Some(hash)=actor.session_hash.take() {
                    self.verify_rule_request_account(&mut tx,&hash,actor.person).await?;
                }
            }
            self.verify(&mut tx,&actor).await?;
            ensure!(row.get::<String,_>("target_agent")=="erhua","executor_identity_mismatch");
            if capability=="erhua.foundation_rule" {
                let permission=super::rule_lifecycle::input_action(&payload["input"]);
                let auth=authorize_current(&mut tx,&self.tenant,actor.person,scope,"erhua","community_service",permission).await?;
                ensure!(auth.status!="denied"&&auth.grant_id==Some(row.get("authority_grant_id")),"authority_changed_or_revoked");
                if auth.status=="confirmation_required" {
                    ensure!(auth.reviewer==row.get::<Option<Uuid>,_>("approval_person_id") && row.get::<Option<String>,_>("approved_input_hash")==Some(row.get("request_hash")),"designated_confirmation_required");
                    let proof=&payload["approval_authority"];
                    let reviewer=Actor{link:serde_json::from_value(proof["identity_link_id"].clone())?,person:serde_json::from_value(proof["person_id"].clone())?,identity_version:serde_json::from_value(proof["identity_version"].clone())?,identity_namespace:serde_json::from_value(proof["identity_namespace"].clone())?,gateway:serde_json::from_value(proof["gateway"].clone())?,session_hash:None,tenant:self.tenant.clone()};
                    self.verify(&mut tx,&reviewer).await?;
                    if lifecycle {
                        if let Some(hash)=proof["session_hash"].as_str() {
                            self.verify_rule_request_account(&mut tx,hash,reviewer.person).await?;
                        }
                    }
                    let current=authorize_current(&mut tx,&self.tenant,reviewer.person,scope,"erhua","community_service",permission).await?;
                    ensure!(Some(reviewer.person)==auth.reviewer && current.status=="autonomous" && current.grant_id==serde_json::from_value::<Option<Uuid>>(proof["grant_id"].clone())?,"approval_authority_changed_or_revoked");
                }
                if payload["input"].get("lifecycle").is_some() {
                    let command = serde_json::from_value(payload["input"]["lifecycle"].clone())?;
                    self.apply_rule_command(&mut tx, &actor, &command, auth.grant_id.unwrap()).await
                } else {
                    let write:KnowledgeWrite=serde_json::from_value(payload["input"].clone())?;
                    Ok(json!({"knowledge":put_authorized_knowledge(&mut tx,&self.tenant,actor.person,&write,auth.grant_id.unwrap()).await?,"consumer":"erhua"}))
                }
            }else{
                ensure!(capability=="erhua.foundation_context","unknown_capability");
                let policy=self.policy(&mut tx,now).await?;
                let manager=policy.manager(actor.person,scope,"erhua","community_service","train").ok_or_else(||anyhow::anyhow!("authority_changed_or_revoked"))?;
                ensure!(manager.id==row.get::<Uuid,_>("authority_grant_id"),"authority_changed_or_revoked");
                let available:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_local_executors WHERE tenant_key=$1 AND agent_key='erhua' AND available)").bind(&self.tenant).fetch_one(&mut *tx).await?;
                if !available {anyhow::bail!("target_unavailable");}
                Ok(json!({"consumer":"erhua","knowledge":knowledge_context(&mut tx,&self.tenant,scope).await?,"reply":"已根据本栋有效知识完成整理。"}))
            }
        }.await;
        if result.is_err() {
            sqlx::query("ROLLBACK TO SAVEPOINT foundation_execution")
                .execute(&mut *tx)
                .await?;
        }
        let (status, result, reason) = match result {
            Ok(v) => ("completed", Some(v), None),
            Err(e) => {
                let s = e.to_string();
                if s == "target_unavailable" {
                    ("queued", None, Some(s))
                } else {
                    ("failed", None, Some(s))
                }
            }
        };
        sqlx::query("UPDATE qintopia_agent_os.work_items SET status=$2,last_error=$3,updated_at=clock_timestamp() WHERE id=$1").bind(work).bind(status).bind(&reason).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.collaboration_work_requests SET result=$2 WHERE work_item_id=$1").bind(work).bind(&result).execute(&mut *tx).await?;
        work_event(
            &mut tx,
            work,
            status,
            "erhua",
            &json!({"reason":reason,"external_effects":false}),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"work_item_id":work,"status":status,"result":result,"reason":reason}))
    }
}

pub(super) async fn work_event(
    tx: &mut Transaction<'_, Postgres>,
    work: Uuid,
    kind: &str,
    actor: &str,
    data: &Value,
) -> Result<()> {
    sqlx::query("INSERT INTO qintopia_agent_os.work_item_events(work_item_id,event_type,actor_type,actor_id,data) VALUES($1,$2,CASE WHEN $3='human' THEN 'human' ELSE 'agent' END,$3,$4)")
        .bind(work).bind(kind).bind(actor).bind(data).execute(&mut **tx).await?;
    Ok(())
}

pub(super) async fn knowledge_context(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    scope: Uuid,
) -> Result<Vec<Value>> {
    let rows=sqlx::query("WITH RECURSIVE ancestors AS (SELECT id,parent_scope_id FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND id=$2 AND status='active' UNION ALL SELECT s.id,s.parent_scope_id FROM qintopia_agent_os.collaboration_scopes s JOIN ancestors a ON a.parent_scope_id=s.id WHERE s.tenant_key=$1 AND s.status='active') SELECT i.scope_id,i.knowledge_key,i.kind,i.version,i.shared FROM qintopia_agent_os.collaboration_knowledge_items i JOIN ancestors a ON a.id=i.scope_id WHERE i.tenant_key=$1 AND i.case_ref IS NULL AND (i.scope_id=$2 OR i.shared) ORDER BY i.scope_id,i.knowledge_key")
        .bind(tenant).bind(scope).fetch_all(&mut **tx).await?;
    let mut knowledge = Vec::new();
    for row in rows {
        if let Some(v) = effective_knowledge_in(
            tx,
            tenant,
            row.get("scope_id"),
            &row.get::<String, _>("knowledge_key"),
            None,
        )
        .await?
        {
            // Reading facts need not perpetuate an author's job. Executable rules do.
            if row.get::<String, _>("kind") == "rule" {
                let auth = authorize_current(
                    tx,
                    tenant,
                    v.author,
                    v.scope,
                    "erhua",
                    "community_service",
                    "change_rules",
                )
                .await?;
                if auth.status == "denied" || auth.grant_id != Some(v.authority_grant) {
                    continue;
                }
            }
            let mut json = serde_json::to_value(v)?;
            json["latest_version"] = json!(row.get::<i32, _>("version"));
            json["kind"] = json!(row.get::<String, _>("kind"));
            json["shared"] = json!(row.get::<bool, _>("shared"));
            knowledge.push(json);
        }
    }
    Ok(knowledge)
}

impl Store {
    pub(crate) async fn verified_person(&self, actor: &Actor) -> Result<Uuid> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        Ok(actor.person)
    }
    pub(crate) async fn turn_evidence(
        &self,
        actor: &Actor,
        message: Uuid,
    ) -> Result<super::MemoryEvidence> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let row=sqlx::query("INSERT INTO qintopia_agent_os.collaboration_turn_sources(tenant_key,message_ref,person_id) VALUES($1,$2,$3) ON CONFLICT(tenant_key,message_ref) DO UPDATE SET message_ref=EXCLUDED.message_ref RETURNING person_id,observed_at")
            .bind(&self.tenant).bind(message).bind(actor.person).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<Uuid, _>("person_id") == actor.person,
            "source_identity_conflict"
        );
        let evidence = super::MemoryEvidence::trusted(message, row.get("observed_at"));
        tx.commit().await?;
        Ok(evidence)
    }
    pub(crate) async fn foundation_assert_scope(&self, actor: &Actor, scope: Uuid) -> Result<()> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let policy = self.policy(&mut tx, now).await?;
        self.read_scope(&policy, actor, scope)
    }
    pub(crate) async fn gateway_scope(&self, actor: &Actor) -> Result<Uuid> {
        self.verified_person(actor).await?;
        actor
            .gateway
            .as_ref()
            .map(|(_, _, scope)| *scope)
            .ok_or_else(|| anyhow::anyhow!("trusted_gateway_required"))
    }
}

pub async fn can_inspect_current(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    person: Uuid,
    scope: Uuid,
    agent: &str,
    domain: &str,
) -> Result<bool> {
    // Reuse the same lock and synthetic gate as mutation authorization.
    authorize_current(tx, tenant, person, scope, agent, domain, "change_rules").await?;
    let now = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    Ok(load_policy(tx, tenant, tenant, now)
        .await?
        .can_inspect(person, scope, agent, domain))
}

impl Store {
    pub(crate) async fn foundation_work_list(&self, actor: &Actor) -> Result<Vec<Value>> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let p = self.policy(&mut tx, now).await?;
        let rows=sqlx::query("SELECT w.id,w.status,w.last_error,w.target_agent,r.scope_id,r.result FROM qintopia_agent_os.collaboration_work_requests r JOIN qintopia_agent_os.work_items w ON w.id=r.work_item_id WHERE r.tenant_key=$1 ORDER BY w.created_at DESC LIMIT 100")
            .bind(&self.tenant).fetch_all(&mut *tx).await?;
        Ok(rows.into_iter().filter(|r|self.read_scope(&p,actor,r.get("scope_id")).is_ok()).map(|r|json!({"work_item_id":r.get::<Uuid,_>("id"),"scope":r.get::<Uuid,_>("scope_id"),"status":r.get::<String,_>("status"),"reason":r.get::<Option<String>,_>("last_error"),"target_agent":r.get::<String,_>("target_agent"),"result":r.get::<Option<Value>,_>("result")})).collect())
    }

    pub(crate) async fn bootstrap_foundation_consumers(
        &self,
        actor: &Actor,
        password: &str,
    ) -> Result<()> {
        let state = self.state(actor).await?;
        let get = |collection: &str, label: &str| -> Result<Uuid> {
            let v = state[collection]
                .as_array()
                .and_then(|items| items.iter().find(|v| v["label"] == label))
                .ok_or_else(|| anyhow::anyhow!("fixture_missing"))?;
            Ok(serde_json::from_value(v["id"].clone())?)
        };
        let root = get("scopes", "秦托邦")?;
        // This initializer runs once on the explicit empty synthetic fixture.
        // End every replaceable demonstration connection, including the root
        // community-management example held by the future house-one person.
        // Keep the immutable bootstrap owner's authority and preserve history.
        for relation in state["relations"]
            .as_array()
            .into_iter()
            .flatten()
            .filter(|r| r["immutable"] == false && r["status"] == "active")
        {
            let version = self.state(actor).await?["version"].as_i64().unwrap();
            self.command(
                actor,
                &Command {
                    operation_id: Uuid::new_v4(),
                    expected_version: version,
                    change: Change::EndCollaboration {
                        collaboration: serde_json::from_value(relation["id"].clone())?,
                    },
                },
                true,
            )
            .await?;
        }
        let mut heads = Vec::new();
        for (person_label, scope_label) in [
            ("人员甲 · 合成样例 A", "一栋"),
            ("人员甲 · 合成样例 B", "二栋"),
            ("合成负责人", "秦托邦"),
        ] {
            let person = get("people", person_label)?;
            let scope = get("scopes", scope_label)?;
            let version = self.state(actor).await?["version"].as_i64().unwrap();
            self.command(
                actor,
                &Command {
                    operation_id: Uuid::new_v4(),
                    expected_version: version,
                    change: Change::Assign(Box::new(Assignment {
                        collaboration: None,
                        person,
                        role: get(
                            "roles",
                            if scope == root {
                                "公司负责人"
                            } else {
                                "舍长"
                            },
                        )?,
                        duty: Some(get("duties", "居民服务")?),
                        scope,
                        agent: "erhua".into(),
                        domain: "community_service".into(),
                        responsibility: "第一批合成验收职责".into(),
                        valid_from: None,
                        valid_until: None,
                        proxy_for: None,
                        actions: vec![],
                        permissions: [
                            "train",
                            "change_rules",
                            "confirm_knowledge",
                            "designate",
                            "review",
                            "publish",
                        ]
                        .iter()
                        .map(|a| PermissionSetting {
                            action: a.to_string(),
                            mode: PermissionMode::Autonomous,
                            reviewer: None,
                        })
                        .collect(),
                        delegation: None,
                    })),
                },
                true,
            )
            .await?;
            let link:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed'").bind(&self.tenant).bind(person).fetch_one(&self.pool).await?;
            heads.push((self.actor(link).await?, scope));
        }
        for (head, scope) in &heads {
            self.knowledge_save(head,&KnowledgeWrite{operation_id:Uuid::new_v4(),expected_version:0,scope:*scope,key:if *scope==root{"community_culture"}else{"kitchen"}.into(),kind:if *scope==root{"culture"}else{"rule"}.into(),shared:*scope==root,case_ref:None,content:json!({"text":if *scope==root{"尊重彼此，维护共享空间，欢迎每一位社区成员。"}else{"本栋厨房每天晚上九点关闭。"}}),effective_at:None,effective_until:None},false).await?;
        }
        self.bootstrap_identity_memory_fixture().await?;
        let welcome = crate::resident_welcome::store::Store {
            pool: self.pool.clone(),
        };
        let fixture = welcome.bootstrap_foundation_fixture(&self.tenant).await?;
        for target in fixture["targets"].as_array().into_iter().flatten() {
            let scope: Uuid = serde_json::from_value(target["scope_ref"].clone())?;
            let (head, _) = heads
                .iter()
                .find(|(_, s)| *s == scope)
                .ok_or_else(|| anyhow::anyhow!("fixture_missing"))?;
            welcome.foundation_configure(&self.tenant,head.person,serde_json::from_value(target["target_ref"].clone())?,&KnowledgeWrite{operation_id:Uuid::new_v4(),expected_version:0,scope,key:"resident_welcome".into(),kind:"rule".into(),shared:false,case_ref:None,content:json!({"mode":if scope==heads[0].1{"direct"}else{"review"},"parts":["text","image"],"phase":"formal","text_template":"欢迎 {name} 来到秦托邦！愿你在这里自在生活，结识有趣的伙伴。"}),effective_at:None,effective_until:None}).await?;
        }
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_local_executors(tenant_key,agent_key,available) VALUES($1,'erhua',true) ON CONFLICT(tenant_key,agent_key) DO UPDATE SET available=true").bind(&self.tenant).execute(&self.pool).await?;
        self.bootstrap_account(actor.person, "admin", password)
            .await?;
        for (username, head) in [("house-one", &heads[0].0), ("house-two", &heads[1].0)] {
            self.account_command(
                actor,
                &super::AccountCommand::Create {
                    person: head.person,
                    username: username.into(),
                    password: password.to_string(),
                },
            )
            .await?;
        }
        Ok(())
    }
}
