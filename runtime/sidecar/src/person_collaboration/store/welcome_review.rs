//! One operations matter for UI and trusted group callers; never a send/PMS ticket.
use super::{Actor, Store};
use crate::person_collaboration::{digest, welcome_model::*};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

pub(crate) enum WelcomeSubject {
    Group {
        subject: Box<WelcomeSubject>,
        scope: Uuid,
        platform: String,
        chat: String,
        configuration_version: i64,
    },
    Person(Actor),
    LinkedPerson {
        link: Uuid,
        person: Uuid,
        version: i64,
        tenant: String,
        gateway: String,
        gateway_version: i64,
    },
    WorkAccount {
        id: Uuid,
        version: i64,
        tenant: String,
        gateway: String,
    },
}
impl WelcomeSubject {
    fn reference(&self) -> SubjectRef {
        match self {
            Self::Group { subject, .. } => subject.reference(),
            Self::Person(a) => SubjectRef::Person(a.person),
            Self::LinkedPerson { person, .. } => SubjectRef::Person(*person),
            Self::WorkAccount { id, .. } => SubjectRef::WorkAccount(*id),
        }
    }
    fn proof(&self) -> Value {
        match self {
            Self::Group { subject, .. } => subject.proof(),
            Self::LinkedPerson {
                link,
                gateway,
                gateway_version,
                ..
            } => {
                json!({"link":link,"linked_person":true,"gateway":gateway,"gateway_version":gateway_version})
            }
            Self::Person(a) => {
                json!({"link":a.link,"namespace":a.identity_namespace,"gateway":a.gateway})
            }
            Self::WorkAccount { gateway, .. } => json!({"gateway":gateway}),
        }
    }
    fn version(&self) -> i64 {
        match self {
            Self::Group { subject, .. } => subject.version(),
            Self::Person(a) => a.identity_version,
            Self::LinkedPerson { version, .. } => *version,
            Self::WorkAccount { version, .. } => *version,
        }
    }
}

impl Store {
    /// Only an already verified personal UI session can become a personal subject.
    pub(crate) async fn welcome_person_subject(&self, actor: &Actor) -> Result<WelcomeSubject> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        Ok(WelcomeSubject::Person(Actor {
            link: actor.link,
            person: actor.person,
            identity_version: actor.identity_version,
            identity_namespace: actor.identity_namespace.clone(),
            gateway: actor.gateway.clone(),
            session_hash: actor.session_hash.clone(),
            tenant: actor.tenant.clone(),
        }))
    }

    /// gateway and sender come exclusively from a trusted host, never model arguments.
    pub(crate) async fn welcome_gateway_subject(
        &self,
        gateway: &str,
        sender: &str,
    ) -> Result<WelcomeSubject> {
        let rows = sqlx::query("SELECT w.id,w.version FROM qintopia_identity.work_accounts w JOIN qintopia_identity.source_identity_links l ON l.id=w.source_link_id JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=w.tenant_key AND g.gateway_key=w.gateway_key WHERE w.tenant_key=$1 AND w.gateway_key=$2 AND l.source_ref=$3 AND l.namespace=g.namespace AND l.subject_type=g.subject_type AND w.active AND g.active AND w.gateway_version=g.version AND l.version=w.source_version AND l.status<>'revoked' AND l.person_id IS NULL")
            .bind(&self.tenant).bind(gateway).bind(sender).fetch_all(&self.pool).await?;
        ensure!(rows.len() <= 1, "welcome_subject_conflict");
        if let Some(row) = rows.first() {
            return Ok(WelcomeSubject::WorkAccount {
                id: row.get("id"),
                version: row.get("version"),
                tenant: self.tenant.clone(),
                gateway: gateway.into(),
            });
        }
        if let Ok(actor) = self.gateway_actor(gateway, sender).await {
            return Ok(WelcomeSubject::Person(actor));
        }
        let row=sqlx::query("SELECT l.id,l.person_id,l.version,g.version AS gateway_version FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type JOIN qintopia_identity.work_accounts w ON w.id=l.confirmed_by_work_account AND w.tenant_key=g.tenant_key JOIN qintopia_agent_os.welcome_review_receipts r ON r.id=l.evidence_ref AND r.tenant_key=g.tenant_key AND r.subject_kind='work_account' AND r.subject_id=w.id JOIN qintopia_identity.persons p ON p.id=l.person_id WHERE g.tenant_key=$1 AND g.gateway_key=$2 AND l.source_ref=$3 AND g.active AND g.account_kind<>'shared' AND l.status='confirmed' AND p.status='active'")
            .bind(&self.tenant).bind(gateway).bind(sender).fetch_optional(&self.pool).await?.ok_or_else(||anyhow::anyhow!("verified_identity_required"))?;
        Ok(WelcomeSubject::LinkedPerson {
            link: row.get("id"),
            person: row.get("person_id"),
            version: row.get("version"),
            tenant: self.tenant.clone(),
            gateway: gateway.into(),
            gateway_version: row.get("gateway_version"),
        })
    }

    async fn welcome_verify(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        subject: &WelcomeSubject,
        scope: Uuid,
    ) -> Result<()> {
        match subject {
            WelcomeSubject::Group {
                subject,
                scope: bound,
                platform,
                chat,
                configuration_version,
            } => {
                ensure!(*bound == scope, "gateway_scope_mismatch");
                let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_review_settings s JOIN qintopia_messages.conversations c ON c.id=s.conversation_id JOIN qintopia_agent_os.collaboration_scope_bindings b ON b.tenant_key=s.tenant_key AND b.scope_id=s.scope_id AND b.conversation_id=c.id WHERE s.tenant_key=$1 AND s.scope_id=$2 AND s.version=$3 AND c.platform=$4 AND c.chat_id=$5 AND c.status='active' AND b.revoked_at IS NULL)")
                    .bind(&self.tenant).bind(scope).bind(configuration_version).bind(platform).bind(chat).fetch_one(&mut **tx).await?;
                ensure!(valid, "configured_operations_group_changed");
                Box::pin(self.welcome_verify(tx, subject, scope)).await?;
            }
            WelcomeSubject::LinkedPerson {
                link,
                person,
                version,
                tenant,
                gateway,
                gateway_version,
            } => {
                ensure!(tenant == &self.tenant, "tenant_mismatch");
                let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type JOIN qintopia_identity.persons p ON p.id=l.person_id JOIN qintopia_agent_os.welcome_review_receipts r ON r.id=l.evidence_ref AND r.tenant_key=g.tenant_key AND r.subject_id=l.confirmed_by_work_account AND r.subject_kind='work_account' WHERE l.id=$1 AND l.person_id=$2 AND l.version=$3 AND l.status='confirmed' AND p.status='active' AND g.tenant_key=$4 AND g.gateway_key=$5 AND g.version=$6 AND g.scope_id=$7 AND g.active AND g.account_kind<>'shared' AND (SELECT count(*) FROM qintopia_identity.person_identity_gateways x WHERE x.namespace=g.namespace AND x.subject_type=g.subject_type)=1)")
                    .bind(link).bind(person).bind(version).bind(&self.tenant).bind(gateway).bind(gateway_version).bind(scope).fetch_one(&mut **tx).await?;
                ensure!(valid, "identity_changed_or_revoked");
            }
            WelcomeSubject::Person(actor) => {
                self.verify(tx, actor).await?;
                if let Some((_, _, bound)) = &actor.gateway {
                    ensure!(*bound == scope, "gateway_scope_mismatch");
                }
            }
            WelcomeSubject::WorkAccount {
                id,
                version,
                tenant,
                gateway,
            } => {
                ensure!(tenant == &self.tenant, "tenant_mismatch");
                let ok:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.work_accounts w JOIN qintopia_identity.source_identity_links l ON l.id=w.source_link_id JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=w.tenant_key AND g.gateway_key=w.gateway_key JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key WHERE w.tenant_key=$1 AND w.id=$2 AND w.version=$3 AND w.gateway_key=$4 AND g.scope_id=$5 AND w.active AND g.active AND w.gateway_version=g.version AND l.version=w.source_version AND l.namespace=g.namespace AND l.subject_type=g.subject_type AND l.person_id IS NULL AND l.status<>'revoked' AND s.status='active')")
                    .bind(&self.tenant).bind(id).bind(version).bind(gateway).bind(scope).fetch_one(&mut **tx).await?;
                ensure!(ok, "welcome_subject_changed_or_revoked");
            }
        }
        Ok(())
    }

    pub(crate) async fn welcome_account_register(
        &self,
        actor: &Actor,
        link: Uuid,
        gateway: &str,
        label: &str,
    ) -> Result<Value> {
        ensure!(
            !label.trim().is_empty() && label.chars().count() <= 100,
            "invalid_label"
        );
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let row=sqlx::query("SELECT g.scope_id,g.version AS gateway_version,l.version,l.adapter_metadata FROM qintopia_identity.person_identity_gateways g JOIN qintopia_identity.source_identity_links l ON l.namespace=g.namespace AND l.subject_type=g.subject_type JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key WHERE g.tenant_key=$1 AND g.gateway_key=$2 AND l.id=$3 AND g.active AND s.status='active' AND l.person_id IS NULL AND l.status='pending' AND g.account_kind='shared' AND (SELECT count(*) FROM qintopia_identity.person_identity_gateways x WHERE x.namespace=g.namespace AND x.subject_type=g.subject_type)=1")
            .bind(&self.tenant).bind(gateway).bind(link).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("observed_work_account_required"))?;
        let scope: Uuid = row.get("scope_id");
        ensure!(
            self.policy(&mut tx, now)
                .await?
                .manager(actor.person, scope, "anan", "hospitality", "identity")
                .is_some(),
            "identity_management_required"
        );
        let metadata: Value = row.get("adapter_metadata");
        let evidence = metadata["first_observation_ref"]
            .as_str()
            .and_then(|v| Uuid::parse_str(v).ok())
            .ok_or_else(|| anyhow::anyhow!("trusted_observation_required"))?;
        let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.work_accounts(tenant_key,source_link_id,source_version,gateway_key,gateway_version,label,verified_by,evidence_ref) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(tenant_key,source_link_id) DO UPDATE SET active=true,version=work_accounts.version+1,source_version=EXCLUDED.source_version,gateway_version=EXCLUDED.gateway_version,label=EXCLUDED.label,verified_by=EXCLUDED.verified_by,evidence_ref=EXCLUDED.evidence_ref RETURNING id")
            .bind(&self.tenant).bind(link).bind(row.get::<i64,_>("version")).bind(gateway).bind(row.get::<i64,_>("gateway_version")).bind(label).bind(actor.person).bind(evidence).fetch_one(&mut *tx).await?;
        // Re-registration must not revive authority previously issued to an old account version.
        sqlx::query("UPDATE qintopia_agent_os.welcome_review_subject_grants SET revoked_at=clock_timestamp() WHERE tenant_key=$1 AND work_account_id=$2 AND revoked_at IS NULL")
            .bind(&self.tenant).bind(id).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"work_account":id,"label":label}))
    }

    pub(crate) async fn welcome_settings_save(
        &self,
        actor: &Actor,
        request: &ReviewSettings,
    ) -> Result<Value> {
        ensure!(request.reviewers.len() <= 20, "too_many_reviewers");
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let policy = self.policy(&mut tx, now).await?;
        let mut authorities = Vec::new();
        for effect in ["identity", "review"] {
            authorities.push(
                policy
                    .manager(actor.person, request.scope, "anan", "hospitality", effect)
                    .ok_or_else(|| anyhow::anyhow!("welcome_management_required"))?
                    .id,
            );
        }
        let bound:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id WHERE b.tenant_key=$1 AND b.scope_id=$2 AND b.conversation_id=$3 AND b.revoked_at IS NULL AND c.status='active' AND c.platform IN ('qiwe','wecom'))")
            .bind(&self.tenant).bind(request.scope).bind(request.conversation).fetch_one(&mut *tx).await?;
        ensure!(bound, "configured_operations_group_required");
        let old:Option<i64>=sqlx::query_scalar("SELECT version FROM qintopia_agent_os.welcome_review_settings WHERE tenant_key=$1 AND scope_id=$2").bind(&self.tenant).bind(request.scope).fetch_optional(&mut *tx).await?;
        ensure!(
            old.unwrap_or(0) == request.expected_version,
            "configuration_version_conflict"
        );
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_review_settings(tenant_key,scope_id,conversation_id,configured_by,authority_refs) VALUES($1,$2,$3,$4,$5) ON CONFLICT(tenant_key,scope_id) DO UPDATE SET conversation_id=EXCLUDED.conversation_id,configured_by=EXCLUDED.configured_by,authority_refs=EXCLUDED.authority_refs,version=welcome_review_settings.version+1")
            .bind(&self.tenant).bind(request.scope).bind(request.conversation).bind(actor.person).bind(authorities).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.welcome_review_subject_grants SET revoked_at=clock_timestamp() WHERE tenant_key=$1 AND scope_id=$2 AND revoked_at IS NULL").bind(&self.tenant).bind(request.scope).execute(&mut *tx).await?;
        for grant in &request.reviewers {
            ensure!(
                !grant.effects.is_empty()
                    && grant
                        .effects
                        .iter()
                        .all(|v| matches!(v.as_str(), "identity" | "review"))
                    && grant.valid_until > now,
                "invalid_review_grant"
            );
            let (person, account) = match grant.subject {
                SubjectRef::Person(id) => {
                    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.persons p ON p.id=l.person_id WHERE l.namespace=$1 AND l.person_id=$2 AND l.status='confirmed' AND l.confirmed_by IS NOT NULL AND l.evidence_ref IS NOT NULL AND p.status='active')").bind(&self.tenant).bind(id).fetch_one(&mut *tx).await?;
                    ensure!(valid, "verified_identity_required");
                    (Some(id), None)
                }
                SubjectRef::WorkAccount(id) => {
                    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.work_accounts w JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=w.tenant_key AND g.gateway_key=w.gateway_key WHERE w.tenant_key=$1 AND w.id=$2 AND w.active AND g.active AND g.scope_id=$3 AND g.version=w.gateway_version)").bind(&self.tenant).bind(id).bind(request.scope).fetch_one(&mut *tx).await?;
                    ensure!(valid, "observed_work_account_required");
                    (None, Some(id))
                }
            };
            sqlx::query("INSERT INTO qintopia_agent_os.welcome_review_subject_grants(tenant_key,scope_id,subject_kind,person_id,work_account_id,effects,valid_until) VALUES($1,$2,$3,$4,$5,$6,$7)")
                .bind(&self.tenant).bind(request.scope).bind(grant.subject.kind()).bind(person).bind(account).bind(&grant.effects).bind(grant.valid_until).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(json!({"version":request.expected_version+1}))
    }

    async fn welcome_authorize(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        subject: &WelcomeSubject,
        scope: Uuid,
        effects: &[&str],
    ) -> Result<Uuid> {
        self.welcome_verify(tx, subject, scope).await?;
        let now = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut **tx)
            .await?;
        let policy = self.policy(tx, now).await?;
        let settings=sqlx::query("SELECT configured_by,authority_refs FROM qintopia_agent_os.welcome_review_settings WHERE tenant_key=$1 AND scope_id=$2")
            .bind(&self.tenant).bind(scope).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("welcome_settings_required"))?;
        let issuer: Uuid = settings.get("configured_by");
        let refs: Vec<Uuid> = settings.get("authority_refs");
        for effect in ["identity", "review"] {
            ensure!(
                policy.grants.iter().any(|g| refs.contains(&g.id)
                    && g.person == issuer
                    && policy.effective(g)
                    && policy
                        .manager(issuer, scope, "anan", "hospitality", effect)
                        .is_some_and(|m| refs.contains(&m.id))),
                "welcome_authority_revoked"
            );
        }
        let reference = subject.reference();
        let id:Option<Uuid>=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.welcome_review_subject_grants WHERE tenant_key=$1 AND scope_id=$2 AND subject_kind=$3 AND coalesce(person_id,work_account_id)=$4 AND revoked_at IS NULL AND valid_until>clock_timestamp() AND effects @> $5::text[] ORDER BY id LIMIT 1")
            .bind(&self.tenant).bind(scope).bind(reference.kind()).bind(reference.id()).bind(effects).fetch_optional(&mut **tx).await?;
        id.ok_or_else(|| anyhow::anyhow!("welcome_effect_not_authorized"))
    }
}

async fn snapshot(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    scope: Uuid,
    case: Uuid,
    application: Uuid,
    artifacts: &[Uuid],
) -> Result<Value> {
    ensure!(artifacts.len() <= 8, "too_many_artifacts");
    let row=sqlx::query("SELECT c.version,c.source_instance,c.property_id,c.order_id,c.stay_id,c.occupant_id,a.revision,a.field_hash,a.consent_version,a.consent_active,a.valid,v.revision::text AS source_revision,v.projection FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_applications a ON a.id=$4 AND a.source_instance=c.source_instance JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id AND NOT v.invalidated WHERE c.id=$3 AND EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_foundation_targets f JOIN qintopia_agent_os.welcome_targets t ON t.id=f.target_id WHERE f.tenant_key=$1 AND f.scope_id=$2 AND t.source_instance=c.source_instance AND t.property_id=c.property_id AND (t.kind<>'building' OR t.building_code=v.projection->>'building')) FOR SHARE OF c,a,v")
        .bind(tenant).bind(scope).bind(case).bind(application).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("welcome_source_scope_mismatch"))?;
    let mut contents = Vec::new();
    for id in artifacts {
        let artifact=sqlx::query("SELECT a.id,a.content_hash,a.artifact_type,a.content_text FROM qintopia_agent_os.artifacts a JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=a.id WHERE a.id=$1 AND b.case_id=$2 AND b.application_id=$3 AND b.application_revision=$4 AND b.consent_version=$5 AND b.revoked_at IS NULL AND a.content_hash IS NOT NULL")
            .bind(id).bind(case).bind(application).bind(row.get::<i64,_>("revision")).bind(row.get::<i64,_>("consent_version")).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("welcome_content_version_conflict"))?;
        contents.push(json!({"id":id,"hash":artifact.get::<String,_>("content_hash"),"kind":artifact.get::<String,_>("artifact_type"),"text":artifact.get::<Option<String>,_>("content_text")}));
    }
    let projection: Value = row.get("projection");
    Ok(
        json!({"case_version":row.get::<i64,_>("version"),"application_revision":row.get::<i64,_>("revision"),"application_hash":row.get::<String,_>("field_hash"),"consent_version":row.get::<i64,_>("consent_version"),"consent_active":row.get::<bool,_>("consent_active"),"valid":row.get::<bool,_>("valid"),"source":row.get::<String,_>("source_instance"),"property":row.get::<String,_>("property_id"),"order":row.get::<String,_>("order_id"),"stay":row.get::<String,_>("stay_id"),"occupant":row.get::<String,_>("occupant_id"),"source_revision":row.get::<String,_>("source_revision"),"building":projection["building"],"artifacts":contents}),
    )
}
fn artifact_ids(value: &Value) -> Result<Vec<Uuid>> {
    value["artifacts"]
        .as_array()
        .ok_or_else(|| anyhow::anyhow!("invalid_snapshot"))?
        .iter()
        .map(|a| serde_json::from_value(a["id"].clone()).map_err(Into::into))
        .collect()
}

impl Store {
    pub(crate) async fn welcome_review_open(
        &self,
        actor: &Actor,
        request: &ReviewOpen,
    ) -> Result<Value> {
        self.welcome_review_open_task(Some(actor), request).await
    }

    /// Internal caller uses a persisted Anan WorkItem; None is never an HTTP/model argument.
    pub(crate) async fn welcome_review_open_task(
        &self,
        actor: Option<&Actor>,
        request: &ReviewOpen,
    ) -> Result<Value> {
        let (mut tx, _, now) = self.begin().await?;
        if let Some(actor) = actor {
            self.verify(&mut tx, actor).await?;
            let policy = self.policy(&mut tx, now).await?;
            ensure!(
                policy.allowed(actor.person, request.scope, "anan", "hospitality", "review")
                    || policy
                        .manager(actor.person, request.scope, "anan", "hospitality", "review")
                        .is_some(),
                "welcome_intake_denied"
            );
        }
        let owned:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.work_items WHERE id=$1 AND requester_agent='anan' AND target_agent='anan' AND work_item_type='welcome_event' AND payload->>'tenant_key'=$2 AND payload->>'case_ref'=$3 AND payload->>'scope_ref'=$4 AND payload->>'application_ref'=$5)")
            .bind(request.work_item).bind(&self.tenant).bind(request.case_ref.to_string()).bind(request.scope.to_string()).bind(request.application.to_string()).fetch_one(&mut *tx).await?;
        ensure!(owned, "welcome_work_item_source_required");
        let version:i64=sqlx::query_scalar("SELECT version FROM qintopia_agent_os.welcome_review_settings WHERE tenant_key=$1 AND scope_id=$2").bind(&self.tenant).bind(request.scope).fetch_one(&mut *tx).await?;
        let current = snapshot(
            &mut tx,
            &self.tenant,
            request.scope,
            request.case_ref,
            request.application,
            &request.artifacts,
        )
        .await?;
        // Candidate references come from this source's existing person links. They never bind automatically.
        let rows=sqlx::query("SELECT DISTINCT p.id,CASE WHEN nullif(p.preferred_name,'') IS NULL OR p.preferred_name=p.display_name THEN p.display_name ELSE p.preferred_name||' · '||p.display_name END AS label FROM qintopia_identity.persons p JOIN qintopia_identity.source_identity_links l ON l.person_id=p.id JOIN qintopia_agent_os.welcome_identity_scopes s ON s.namespace=l.namespace WHERE s.source_instance=$1 AND s.property_id=$2 AND p.status='active' UNION SELECT p.id,CASE WHEN nullif(p.preferred_name,'') IS NULL OR p.preferred_name=p.display_name THEN p.display_name ELSE p.preferred_name||' · '||p.display_name END AS label FROM qintopia_identity.persons p JOIN qintopia_agent_os.welcome_applications a ON a.person_id=p.id WHERE a.id=$3 AND p.status='active' ORDER BY label,id LIMIT 20")
            .bind(current["source"].as_str()).bind(current["property"].as_str()).bind(request.application).fetch_all(&mut *tx).await?;
        let candidates:Vec<Value>=rows.iter().map(|r|json!({"person":r.get::<Uuid,_>("id"),"label":r.get::<String,_>("label"),"basis":"已有来源人员，待核对申请及入住关系","confirmed":false})).collect();
        if let Some(old)=sqlx::query("SELECT tenant_key,scope_id,case_id,application_id,snapshot,version,status,configuration_version FROM qintopia_agent_os.welcome_review_items WHERE work_item_id=$1").bind(request.work_item).fetch_optional(&mut *tx).await? {
            ensure!(old.get::<String,_>("tenant_key")==self.tenant && old.get::<Uuid,_>("scope_id")==request.scope && old.get::<Uuid,_>("case_id")==request.case_ref && old.get::<Uuid,_>("application_id")==request.application,"welcome_work_item_conflict");
            if old.get::<Value,_>("snapshot")==current && old.get::<i64,_>("configuration_version")==version && !matches!(old.get::<String,_>("status").as_str(),"revoked"|"rejected") { return Ok(json!({"work_item":request.work_item,"version":old.get::<i64,_>("version"),"replayed":true})); }
            sqlx::query("UPDATE qintopia_agent_os.welcome_review_items SET snapshot=$2,candidates=$3,configuration_version=$4,version=version+1,status='pending',content_receipt=NULL WHERE work_item_id=$1")
                .bind(request.work_item).bind(&current).bind(json!(candidates)).bind(version).execute(&mut *tx).await?;
        } else {
            sqlx::query("INSERT INTO qintopia_agent_os.welcome_review_items(work_item_id,tenant_key,scope_id,case_id,application_id,configuration_version,snapshot,candidates) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
                .bind(request.work_item).bind(&self.tenant).bind(request.scope).bind(request.case_ref).bind(request.application).bind(version).bind(&current).bind(json!(candidates)).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE qintopia_agent_os.work_items SET status='awaiting_review' WHERE id=$1")
            .bind(request.work_item)
            .execute(&mut *tx)
            .await?;
        sqlx::query("INSERT INTO qintopia_agent_os.work_item_events(work_item_id,event_type,actor_type,actor_id,data,created_at) VALUES($1,'welcome_operations_requested','agent','anan',$2,clock_timestamp())").bind(request.work_item).bind(json!({"configuration_version":version,"external_send":false})).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"work_item":request.work_item,"status":"pending"}))
    }

    pub(crate) async fn welcome_review_list(
        &self,
        subject: &WelcomeSubject,
        scope: Uuid,
    ) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        let grant = self.welcome_authorize(&mut tx, subject, scope, &[]).await?;
        let allowed_effects: Vec<String> = sqlx::query_scalar(
            "SELECT effects FROM qintopia_agent_os.welcome_review_subject_grants WHERE id=$1",
        )
        .bind(grant)
        .fetch_one(&mut *tx)
        .await?;
        let rows=sqlx::query("SELECT i.*,CASE WHEN nullif(p.preferred_name,'') IS NULL OR p.preferred_name=p.display_name THEN p.display_name ELSE p.preferred_name||' · '||p.display_name END AS person_label,c.display_name AS group_label FROM qintopia_agent_os.welcome_review_items i JOIN qintopia_agent_os.welcome_review_settings s ON s.tenant_key=i.tenant_key AND s.scope_id=i.scope_id JOIN qintopia_messages.conversations c ON c.id=s.conversation_id LEFT JOIN qintopia_identity.persons p ON p.id=i.confirmed_person WHERE i.tenant_key=$1 AND i.scope_id=$2 ORDER BY i.work_item_id LIMIT 100")
            .bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
        let channels=sqlx::query("SELECT DISTINCT l.id,l.version,coalesce(nullif(ci.display_name,''),nullif(l.adapter_metadata->>'display_name',''),'待核对渠道账号') AS label,l.person_id FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type LEFT JOIN qintopia_identity.channel_identities ci ON ci.id=l.channel_identity_id WHERE g.tenant_key=$1 AND g.scope_id=$2 AND g.active AND g.account_kind<>'shared' AND (l.status<>'revoked' OR EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_review_receipts r JOIN qintopia_agent_os.welcome_review_items i ON i.work_item_id=r.work_item_id WHERE r.id=l.evidence_ref AND i.tenant_key=$1 AND i.scope_id=$2)) AND l.subject_type IN ('qiwe_sender','wecom_external','wecom_internal') ORDER BY label,l.id LIMIT 100")
            .bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
        Ok(
            json!({"allowed_effects":allowed_effects,"items":rows.iter().map(|r|json!({"work_item":r.get::<Uuid,_>("work_item_id"),"version":r.get::<i64,_>("version"),"status":r.get::<String,_>("status"),"snapshot":r.get::<Value,_>("snapshot"),"candidates":r.get::<Value,_>("candidates"),"person_label":r.get::<Option<String>,_>("person_label"),"group_label":r.get::<String,_>("group_label"),"identity_confirmed":r.get::<Option<Uuid>,_>("identity_receipt").is_some(),"content_confirmed":r.get::<Option<Uuid>,_>("content_receipt").is_some()})).collect::<Vec<_>>(),"channels":channels.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"label":r.get::<String,_>("label"),"person":r.get::<Option<Uuid>,_>("person_id")})).collect::<Vec<_>>()}),
        )
    }

    pub(crate) async fn welcome_review_decide(
        &self,
        subject: &WelcomeSubject,
        r: &ReviewDecision,
    ) -> Result<Value> {
        ensure!(
            matches!(r.decision.as_str(), "confirm" | "reject" | "revoke"),
            "explicit_decision_required"
        );
        let mut effects = Vec::new();
        if r.confirm_application_stay || r.confirm_channel_person {
            effects.push("identity");
        }
        if r.confirm_content {
            effects.push("review");
        }
        if r.decision == "revoke" {
            effects = vec!["identity", "review"];
        }
        if r.decision == "reject" {
            effects = vec!["review"];
        }
        ensure!(!effects.is_empty(), "explicit_effect_required");
        let (mut tx, _, _) = self.begin().await?;
        let row=sqlx::query("SELECT * FROM qintopia_agent_os.welcome_review_items WHERE work_item_id=$1 AND tenant_key=$2 FOR UPDATE").bind(r.work_item).bind(&self.tenant).fetch_one(&mut *tx).await?;
        let scope: Uuid = row.get("scope_id");
        let grant = self
            .welcome_authorize(&mut tx, subject, scope, &effects)
            .await?;
        let reference = subject.reference();
        let hash = digest(&serde_json::to_vec(r)?);
        if let Some(receipt)=sqlx::query("SELECT request_hash,subject_kind,subject_id,result FROM qintopia_agent_os.welcome_review_receipts WHERE id=$1 AND tenant_key=$2").bind(r.operation_id).bind(&self.tenant).fetch_optional(&mut *tx).await? {
            ensure!(receipt.get::<String,_>("request_hash")==hash && receipt.get::<String,_>("subject_kind")==reference.kind() && receipt.get::<Uuid,_>("subject_id")==reference.id(),"idempotency_conflict");
            return Ok(receipt.get("result"));
        }
        ensure!(
            row.get::<i64, _>("version") == r.expected_version,
            "welcome_version_conflict"
        );
        let config:i64=sqlx::query_scalar("SELECT version FROM qintopia_agent_os.welcome_review_settings WHERE tenant_key=$1 AND scope_id=$2").bind(&self.tenant).bind(scope).fetch_one(&mut *tx).await?;
        ensure!(
            config == row.get::<i64, _>("configuration_version"),
            "configuration_version_conflict"
        );
        let case: Uuid = row.get("case_id");
        let app: Uuid = row.get("application_id");
        let saved: Value = row.get("snapshot");
        let artifacts = artifact_ids(&saved)?;
        let current = if r.decision == "confirm" {
            snapshot(&mut tx, &self.tenant, scope, case, app, &artifacts).await?
        } else {
            saved.clone()
        };
        ensure!(
            r.decision == "revoke" || current == saved,
            "welcome_content_version_conflict"
        );
        let mut person: Option<Uuid> = row.get("confirmed_person");
        let mut channel: Option<Uuid> = row.get("confirmed_channel");
        let mut identity_receipt: Option<Uuid> = row.get("identity_receipt");
        let mut content_receipt: Option<Uuid> = row.get("content_receipt");
        let mut status = "pending";
        let mut previous_application_person: Option<Uuid> = None;
        if r.decision == "confirm" {
            ensure!(
                !matches!(
                    row.get::<String, _>("status").as_str(),
                    "revoked" | "rejected"
                ),
                "welcome_matter_closed"
            );
            if effects.contains(&"identity") {
                content_receipt = None;
                let chosen = r
                    .person
                    .ok_or_else(|| anyhow::anyhow!("person_selection_required"))?;
                let candidates: Value = row.get("candidates");
                ensure!(
                    candidates
                        .as_array()
                        .is_some_and(|a| a.iter().any(|c| c["person"] == json!(chosen))),
                    "candidate_selection_required"
                );
                ensure!(
                    person.is_none_or(|p| p == chosen),
                    "identity_conflict_revoke_first"
                );
                let active:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.persons WHERE id=$1 AND status='active')").bind(chosen).fetch_one(&mut *tx).await?;
                ensure!(active, "person_inactive");
                if r.confirm_application_stay {
                    let application=sqlx::query("SELECT person_id FROM qintopia_agent_os.welcome_applications WHERE id=$1 FOR UPDATE").bind(app).fetch_one(&mut *tx).await?;
                    previous_application_person = application.get("person_id");
                    ensure!(
                        application
                            .get::<Option<Uuid>, _>("person_id")
                            .is_none_or(|p| p == chosen),
                        "application_person_conflict"
                    );
                    let link=sqlx::query("SELECT l.id FROM qintopia_identity.source_identity_links l JOIN qintopia_agent_os.welcome_identity_scopes s ON s.namespace=l.namespace WHERE s.source_instance=$1 AND s.property_id=$2 AND l.subject_type='pms_occupant' AND l.source_ref=$3")
                        .bind(current["source"].as_str()).bind(current["property"].as_str()).bind(current["occupant"].as_str()).fetch_one(&mut *tx).await?;
                    let link_id: Uuid = link.get("id");
                    confirm_link(
                        &mut tx,
                        link_id,
                        chosen,
                        &reference,
                        r.operation_id,
                        r.work_item,
                    )
                    .await?;
                    sqlx::query("UPDATE qintopia_agent_os.welcome_applications SET person_id=$2 WHERE id=$1").bind(app).bind(chosen).execute(&mut *tx).await?;
                    let changed=sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET person_id=$2,identity_link_id=$3,identity_version=(SELECT version FROM qintopia_identity.source_identity_links WHERE id=$3),application_id=$4,version=version+1 WHERE id=$1 AND (person_id IS NULL OR person_id=$2)")
                        .bind(case).bind(chosen).bind(link_id).bind(app).execute(&mut *tx).await?.rows_affected();
                    ensure!(changed == 1, "case_person_conflict");
                    person = Some(chosen);
                    identity_receipt = Some(r.operation_id);
                }
                if r.confirm_channel_person {
                    let id = r
                        .channel
                        .ok_or_else(|| anyhow::anyhow!("channel_selection_required"))?;
                    let observed:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type WHERE l.id=$1 AND g.tenant_key=$2 AND g.scope_id=$3 AND g.active AND g.account_kind<>'shared' AND l.subject_type IN ('qiwe_sender','wecom_external','wecom_internal') AND l.adapter_metadata ? 'first_observation_ref')")
                        .bind(id).bind(&self.tenant).bind(scope).fetch_one(&mut *tx).await?;
                    ensure!(observed, "observed_channel_required");
                    confirm_link(&mut tx, id, chosen, &reference, r.operation_id, r.work_item)
                        .await?;
                    channel = Some(id);
                }
            }
            if identity_receipt.is_some() {
                status = "identity_confirmed";
            }
            if r.confirm_content {
                ensure!(
                    person.is_some() && identity_receipt.is_some() && channel.is_some(),
                    "identity_segments_required"
                );
                ensure!(
                    current["valid"] == true
                        && current["consent_active"] == true
                        && !artifacts.is_empty(),
                    "welcome_content_not_ready"
                );
                let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links WHERE id=$1 AND person_id=$2 AND status='confirmed')").bind(channel).bind(person).fetch_one(&mut *tx).await?;
                ensure!(valid, "channel_identity_changed");
                sqlx::query("UPDATE qintopia_agent_os.welcome_artifact_bindings SET case_version=(SELECT version FROM qintopia_agent_os.welcome_cases WHERE id=$1) WHERE artifact_id=ANY($2) AND case_id=$1")
                    .bind(case).bind(&artifacts).execute(&mut *tx).await?;
                content_receipt = Some(r.operation_id);
                status = "confirmed";
            }
        } else {
            status = if r.decision == "reject" {
                "rejected"
            } else {
                "revoked"
            };
            if r.decision == "revoke" {
                if let Some(receipt) = identity_receipt {
                    let previous:Option<Value>=sqlx::query_scalar("SELECT effects FROM qintopia_agent_os.welcome_review_receipts WHERE id=$1 AND work_item_id=$2 AND tenant_key=$3").bind(receipt).bind(r.work_item).bind(&self.tenant).fetch_optional(&mut *tx).await?;
                    if previous.as_ref().is_some_and(|v| {
                        v["application_stay"] == true
                            && v.get("previous_application_person")
                                .is_some_and(Value::is_null)
                    }) {
                        sqlx::query("UPDATE qintopia_agent_os.welcome_applications SET person_id=NULL WHERE id=$1 AND person_id=$2").bind(app).bind(person).execute(&mut *tx).await?;
                    }
                }
                sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET person_id=NULL,identity_link_id=NULL,identity_version=NULL,version=version+1 WHERE id=$1 AND identity_link_id IN (SELECT l.id FROM qintopia_identity.source_identity_links l JOIN qintopia_agent_os.welcome_review_receipts r ON r.id=l.evidence_ref WHERE r.work_item_id=$2 AND r.tenant_key=$3)")
                    .bind(case).bind(r.work_item).bind(&self.tenant).execute(&mut *tx).await?;
                // Only links established by this matter are revoked. Other proofs are not overwritten.
                sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked',version=version+1 WHERE evidence_ref IN (SELECT id FROM qintopia_agent_os.welcome_review_receipts WHERE work_item_id=$1 AND tenant_key=$2)")
                    .bind(r.work_item).bind(&self.tenant).execute(&mut *tx).await?;
            }
            content_receipt = None;
            identity_receipt = None;
            channel = None;
            person = None;
            // Operations status blocks this confirmation independently of a steward's hold.
            // Never clear or overwrite another workflow's manual pause.
        }
        let after = if r.decision == "confirm" {
            snapshot(&mut tx, &self.tenant, scope, case, app, &artifacts).await?
        } else {
            saved
        };
        let result = json!({"work_item":r.work_item,"version":r.expected_version+1,"status":status,"identity_confirmed":identity_receipt.is_some(),"channel_confirmed":channel.is_some(),"content_confirmed":content_receipt.is_some(),"published":false});
        sqlx::query("UPDATE qintopia_agent_os.welcome_review_items SET version=version+1,status=$2,snapshot=$3,confirmed_person=$4,confirmed_channel=$5,identity_receipt=$6,content_receipt=$7 WHERE work_item_id=$1")
            .bind(r.work_item).bind(status).bind(after).bind(person).bind(channel).bind(identity_receipt).bind(content_receipt).execute(&mut *tx).await?;
        let effect_record = json!({"application_stay":r.confirm_application_stay,"channel_person":r.confirm_channel_person,"content":r.confirm_content,"decision":r.decision,"publish":false,"previous_application_person":previous_application_person});
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_review_receipts(id,tenant_key,work_item_id,subject_kind,subject_id,subject_version,grant_id,request_hash,effects,result,subject_proof) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
            .bind(r.operation_id).bind(&self.tenant).bind(r.work_item).bind(reference.kind()).bind(reference.id()).bind(subject.version()).bind(grant).bind(hash).bind(&effect_record).bind(&result).bind(subject.proof()).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.work_item_events(work_item_id,event_type,actor_type,actor_id,data) VALUES($1,'welcome_operations_decision',$2,$3,$4)").bind(r.work_item).bind(reference.kind()).bind(reference.id().to_string()).bind(effect_record).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.work_items SET status=$2,updated_at=clock_timestamp() WHERE id=$1").bind(r.work_item).bind(if status=="confirmed" {"completed"} else if status=="revoked" || status=="rejected" {"cancelled"} else {"awaiting_review"}).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
}
async fn confirm_link(
    tx: &mut Transaction<'_, Postgres>,
    link: Uuid,
    person: Uuid,
    subject: &SubjectRef,
    evidence: Uuid,
    work: Uuid,
) -> Result<()> {
    let row=sqlx::query("SELECT person_id,status,evidence_ref FROM qintopia_identity.source_identity_links WHERE id=$1 FOR UPDATE").bind(link).fetch_one(&mut **tx).await?;
    let ours:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_review_receipts WHERE id=$1 AND work_item_id=$2)")
        .bind(row.get::<Option<Uuid>,_>("evidence_ref")).bind(work).fetch_one(&mut **tx).await?;
    let status = row.get::<String, _>("status");
    ensure!(
        row.get::<Option<Uuid>, _>("person_id")
            .is_none_or(|p| p == person)
            || (status == "revoked" && ours),
        "identity_conflict_revoke_first"
    );
    if status == "confirmed" {
        return Ok(());
    }
    ensure!(
        status == "pending" || (status == "revoked" && ours),
        "identity_revoked"
    );
    let (human, account) = match subject {
        SubjectRef::Person(id) => (Some(*id), None),
        SubjectRef::WorkAccount(id) => (None, Some(*id)),
    };
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET person_id=$2,status='confirmed',version=version+1,evidence_ref=$3,confirmed_by=$4,confirmed_by_work_account=$5,updated_at=clock_timestamp() WHERE id=$1")
        .bind(link).bind(person).bind(evidence).bind(human).bind(account).execute(&mut **tx).await?;
    Ok(())
}

impl Store {
    pub(crate) async fn welcome_review_options(&self, actor: &Actor, scope: Uuid) -> Result<Value> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let p = self.policy(&mut tx, now).await?;
        if !["identity", "review"].iter().all(|a| {
            p.manager(actor.person, scope, "anan", "hospitality", a)
                .is_some()
        }) {
            return Ok(json!({"can_manage":false}));
        }
        let setting=sqlx::query("SELECT version,conversation_id FROM qintopia_agent_os.welcome_review_settings WHERE tenant_key=$1 AND scope_id=$2").bind(&self.tenant).bind(scope).fetch_optional(&mut *tx).await?;
        let groups=sqlx::query("SELECT c.id,c.display_name FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id WHERE b.tenant_key=$1 AND b.scope_id=$2 AND b.revoked_at IS NULL AND c.status='active' AND c.platform IN ('wecom','qiwe') ORDER BY c.display_name,c.id").bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
        let accounts=sqlx::query("SELECT w.id,w.label,w.active FROM qintopia_identity.work_accounts w JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=w.tenant_key AND g.gateway_key=w.gateway_key WHERE w.tenant_key=$1 AND g.scope_id=$2 ORDER BY w.label,w.id").bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
        let observed=sqlx::query("SELECT l.id,g.gateway_key,coalesce(nullif(ci.display_name,''),nullif(l.adapter_metadata->>'display_name',''),'待核对工作账号') AS label FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type LEFT JOIN qintopia_identity.channel_identities ci ON ci.id=l.channel_identity_id WHERE g.tenant_key=$1 AND g.scope_id=$2 AND g.account_kind='shared' AND g.active AND l.status='pending' AND l.person_id IS NULL AND l.adapter_metadata ? 'first_observation_ref' AND NOT EXISTS(SELECT 1 FROM qintopia_identity.work_accounts w WHERE w.source_link_id=l.id AND w.tenant_key=$1 AND w.active)")
            .bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
        let people=sqlx::query("SELECT DISTINCT p.id,CASE WHEN nullif(p.preferred_name,'') IS NULL OR p.preferred_name=p.display_name THEN p.display_name ELSE p.preferred_name||' · '||p.display_name END AS label FROM qintopia_identity.persons p JOIN qintopia_identity.source_identity_links l ON l.person_id=p.id WHERE l.namespace=$1 AND l.status='confirmed' AND l.confirmed_by IS NOT NULL AND p.status='active' AND (p.id=$3 OR EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_appointments a WHERE a.tenant_key=$1 AND a.scope_id=$2 AND a.person_id=p.id AND a.status='active')) ORDER BY label,p.id").bind(&self.tenant).bind(scope).bind(actor.person).fetch_all(&mut *tx).await?;
        let grants=sqlx::query("SELECT subject_kind,coalesce(person_id,work_account_id) AS subject_id,effects,valid_until FROM qintopia_agent_os.welcome_review_subject_grants WHERE tenant_key=$1 AND scope_id=$2 AND revoked_at IS NULL").bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
        Ok(
            json!({"can_manage":true,"version":setting.as_ref().map(|r|r.get::<i64,_>("version")).unwrap_or(0),"conversation":setting.as_ref().map(|r|r.get::<Uuid,_>("conversation_id")),"groups":groups.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"label":r.get::<String,_>("display_name")})).collect::<Vec<_>>(),"accounts":accounts.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"label":r.get::<String,_>("label"),"active":r.get::<bool,_>("active")})).collect::<Vec<_>>(),"observed":observed.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"label":r.get::<String,_>("label"),"gateway":r.get::<String,_>("gateway_key")})).collect::<Vec<_>>(),"people":people.iter().map(|r|json!({"id":r.get::<Uuid,_>("id"),"label":r.get::<String,_>("label")})).collect::<Vec<_>>(),"reviewers":grants.iter().map(|r|json!({"subject":{"kind":r.get::<String,_>("subject_kind"),"id":r.get::<Uuid,_>("subject_id")},"effects":r.get::<Vec<String>,_>("effects"),"valid_until":r.get::<chrono::DateTime<chrono::Utc>,_>("valid_until")})).collect::<Vec<_>>()}),
        )
    }
}

pub(crate) async fn http_dispatch(
    store: &Store,
    actor: &Actor,
    path: &str,
    body: &[u8],
) -> Result<Value> {
    #[derive(serde::Deserialize)]
    #[serde(deny_unknown_fields)]
    struct Scope {
        scope: Uuid,
    }
    match path {
        "/api/foundation/operations/options" => {
            let r: Scope = serde_json::from_slice(body)?;
            store.welcome_review_options(actor, r.scope).await
        }
        "/api/foundation/operations/settings" => {
            store
                .welcome_settings_save(actor, &serde_json::from_slice(body)?)
                .await
        }
        "/api/foundation/operations/account" => {
            #[derive(serde::Deserialize)]
            #[serde(deny_unknown_fields)]
            struct Register {
                link: Uuid,
                gateway: String,
                label: String,
            }
            let r: Register = serde_json::from_slice(body)?;
            store
                .welcome_account_register(actor, r.link, &r.gateway, &r.label)
                .await
        }
        "/api/foundation/operations/open" => {
            store
                .welcome_review_open(actor, &serde_json::from_slice(body)?)
                .await
        }
        "/api/foundation/operations/list" => {
            let r: Scope = serde_json::from_slice(body)?;
            let subject = store.welcome_person_subject(actor).await?;
            store.welcome_review_list(&subject, r.scope).await
        }
        "/api/foundation/operations/decide" => {
            let subject = store.welcome_person_subject(actor).await?;
            store
                .welcome_review_decide(&subject, &serde_json::from_slice(body)?)
                .await
        }
        _ => anyhow::bail!("unknown_operation"),
    }
}

pub(crate) async fn broker_invoke(
    store: &Store,
    gateway: &str,
    profile: &str,
    t: super::super::foundation_server::TrustedContext,
    tool: &str,
    args: Value,
) -> Result<Value> {
    ensure!(
        profile == "anan"
            && t.gateway_id == gateway
            && t.chat_type == "group"
            && matches!(t.platform.as_str(), "qiwe" | "wecom")
            && !t.message_id.is_empty()
            && t.message_id.len() <= 240
            && !t.sender_id.is_empty()
            && t.sender_id.len() <= 240,
        "trusted_context_unavailable"
    );
    ensure!(
        matches!(
            tool,
            "welcome_operations_list" | "welcome_operations_decide"
        ),
        "unknown_welcome_operation"
    );
    let (mut tx, _, _) = store.begin().await?;
    let row=sqlx::query("SELECT g.scope_id,g.subject_type,g.namespace,s.version AS configuration_version FROM qintopia_identity.person_identity_gateways g JOIN qintopia_agent_os.welcome_review_settings s ON s.tenant_key=g.tenant_key AND s.scope_id=g.scope_id JOIN qintopia_messages.conversations c ON c.id=s.conversation_id JOIN qintopia_agent_os.collaboration_scope_bindings b ON b.tenant_key=s.tenant_key AND b.scope_id=s.scope_id AND b.conversation_id=c.id WHERE g.tenant_key=$1 AND g.gateway_key=$2 AND g.active AND c.platform=$3 AND c.chat_id=$4 AND c.status='active' AND b.revoked_at IS NULL AND (SELECT count(*) FROM qintopia_identity.person_identity_gateways x WHERE x.namespace=g.namespace AND x.subject_type=g.subject_type)=1")
        .bind(&store.tenant).bind(gateway).bind(&t.platform).bind(&t.chat_id).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("configured_operations_group_required"))?;
    let kind: String = row.get("subject_type");
    ensure!(
        (t.platform == "qiwe" && kind == "qiwe_sender")
            || (t.platform == "wecom"
                && matches!(kind.as_str(), "wecom_internal" | "wecom_external")),
        "gateway_platform_mismatch"
    );
    ensure!(
        !row.get::<String, _>("namespace").is_empty(),
        "identity_namespace_unbound"
    );
    let scope: Uuid = row.get("scope_id");
    let evidence=sqlx::query("SELECT m.raw FROM qintopia_messages.messages m JOIN qintopia_messages.raw_events r ON r.id=m.raw_event_id WHERE m.tenant_id=$1 AND m.platform=$2 AND m.chat_id=$3 AND m.sender_id=$4 AND m.message_id=$5 AND m.chat_type='group' AND r.ingress_auth_verified AND r.subject=$6 AND m.sent_at IS NOT NULL")
        .bind(&store.tenant).bind(&t.platform).bind(&t.chat_id).bind(&t.sender_id).bind(&t.message_id).bind(format!("qintopia.{}.raw.authenticated",t.platform)).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("trusted_message_evidence_required"))?;
    // Release tenant lock before the common command opens its own transaction.
    tx.commit().await?;
    let subject = WelcomeSubject::Group {
        subject: Box::new(store.welcome_gateway_subject(gateway, &t.sender_id).await?),
        scope,
        platform: t.platform.clone(),
        chat: t.chat_id.clone(),
        configuration_version: row.get("configuration_version"),
    };
    if tool == "welcome_operations_list" {
        ensure!(args == json!({}), "invalid_arguments");
        return store.welcome_review_list(&subject, scope).await;
    }
    let decision: ReviewDecision = serde_json::from_value(args.clone())?;
    let raw: Value = evidence.get("raw");
    // Only an explicit, authenticated interactive decision can authorize effects.
    // A model's interpretation of an ordinary "agree" message is not evidence.
    ensure!(
        raw["welcome_confirmation"] == args,
        "explicit_group_confirmation_required"
    );
    let same_scope:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_review_items WHERE tenant_key=$1 AND scope_id=$2 AND work_item_id=$3)").bind(&store.tenant).bind(scope).bind(decision.work_item).fetch_one(&store.pool).await?;
    ensure!(same_scope, "gateway_scope_mismatch");
    store.welcome_review_decide(&subject, &decision).await
}

/// Operations approval remains a separate gate before the existing building policy.
pub(crate) async fn assert_operations_review(
    pool: &sqlx::PgPool,
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    scope: Uuid,
    case: Uuid,
    artifact: Uuid,
) -> Result<()> {
    let configured:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_review_settings WHERE tenant_key=$1 AND scope_id=$2)").bind(tenant).bind(scope).fetch_one(&mut **tx).await?;
    if !configured {
        return Ok(());
    }
    let row=sqlx::query("SELECT i.*,r.subject_kind,r.subject_id,r.subject_version,r.subject_proof,r.grant_id FROM qintopia_agent_os.welcome_review_items i JOIN qintopia_agent_os.welcome_review_receipts r ON r.id=i.content_receipt JOIN qintopia_agent_os.welcome_review_settings s ON s.tenant_key=i.tenant_key AND s.scope_id=i.scope_id WHERE i.tenant_key=$1 AND i.scope_id=$2 AND i.case_id=$3 AND i.status='confirmed' AND i.configuration_version=s.version AND i.work_item_id=(SELECT newer.work_item_id FROM qintopia_agent_os.welcome_review_items newer JOIN qintopia_agent_os.work_item_events e ON e.work_item_id=newer.work_item_id AND e.event_type='welcome_operations_requested' WHERE newer.tenant_key=$1 AND newer.scope_id=$2 AND newer.case_id=$3 ORDER BY e.created_at DESC,e.id DESC LIMIT 1) ORDER BY r.created_at DESC LIMIT 1")
        .bind(tenant).bind(scope).bind(case).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("operations_confirmation_required"))?;
    let saved: Value = row.get("snapshot");
    let ids = artifact_ids(&saved)?;
    let current = snapshot(tx, tenant, scope, case, row.get("application_id"), &ids).await?;
    ensure!(saved == current, "operations_content_changed");
    let kind: String =
        sqlx::query_scalar("SELECT artifact_type FROM qintopia_agent_os.artifacts WHERE id=$1")
            .bind(artifact)
            .fetch_one(&mut **tx)
            .await?;
    ensure!(
        kind != "welcome_card" || ids.contains(&artifact),
        "operations_card_not_confirmed"
    );
    let proof: Value = row.get("subject_proof");
    let id: Uuid = row.get("subject_id");
    let version: i64 = row.get("subject_version");
    let subject = if row.get::<String, _>("subject_kind") == "work_account" {
        WelcomeSubject::WorkAccount {
            id,
            version,
            tenant: tenant.into(),
            gateway: proof["gateway"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("invalid_subject_proof"))?
                .into(),
        }
    } else if proof["linked_person"] == true {
        WelcomeSubject::LinkedPerson {
            link: serde_json::from_value(proof["link"].clone())?,
            person: id,
            version,
            tenant: tenant.into(),
            gateway: proof["gateway"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("invalid_subject_proof"))?
                .into(),
            gateway_version: proof["gateway_version"]
                .as_i64()
                .ok_or_else(|| anyhow::anyhow!("invalid_subject_proof"))?,
        }
    } else {
        WelcomeSubject::Person(Actor {
            link: serde_json::from_value(proof["link"].clone())?,
            person: id,
            identity_version: version,
            identity_namespace: proof["namespace"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("invalid_subject_proof"))?
                .into(),
            gateway: serde_json::from_value(proof["gateway"].clone())?,
            session_hash: None,
            tenant: tenant.into(),
        })
    };
    let store = Store {
        pool: pool.clone(),
        tenant: tenant.into(),
    };
    let grant = store
        .welcome_authorize(tx, &subject, scope, &["review"])
        .await?;
    ensure!(
        grant == row.get::<Uuid, _>("grant_id"),
        "operations_approval_authority_changed"
    );
    let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.persons p ON p.id=l.person_id WHERE l.id=$1 AND l.person_id=$2 AND l.status='confirmed' AND p.status='active')").bind(row.get::<Option<Uuid>,_>("confirmed_channel")).bind(row.get::<Option<Uuid>,_>("confirmed_person")).fetch_one(&mut **tx).await?;
    ensure!(valid, "operations_identity_changed");
    Ok(())
}
impl Store {
    pub(crate) async fn welcome_review_card(
        &self,
        actor: &Actor,
        artifact: Uuid,
    ) -> Result<Vec<u8>> {
        let subject = self.welcome_person_subject(actor).await?;
        let (mut tx, _, _) = self.begin().await?;
        let rows=sqlx::query("SELECT scope_id FROM qintopia_agent_os.welcome_review_items WHERE tenant_key=$1 AND snapshot->'artifacts' @> $2::jsonb").bind(&self.tenant).bind(json!([{"id":artifact}])).fetch_all(&mut *tx).await?;
        for row in rows {
            if self
                .welcome_authorize(&mut tx, &subject, row.get("scope_id"), &[])
                .await
                .is_ok()
            {
                let content:Vec<u8>=sqlx::query_scalar("SELECT content FROM qintopia_agent_os.welcome_local_artifact_data WHERE artifact_id=$1 AND media_type='image/png'").bind(artifact).fetch_one(&mut *tx).await?;
                return Ok(content);
            }
        }
        anyhow::bail!("scope_access_denied")
    }
}
