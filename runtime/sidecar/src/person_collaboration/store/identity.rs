//! Trusted Gateway identities converge on the existing stable Person.
use super::{Actor, Store};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub struct IdentityCommand {
    pub operation_id: Uuid,
    pub link_ref: Uuid,
    pub person_ref: Uuid,
    pub expected_version: i64,
    pub evidence_ref: Uuid,
    pub revoke: bool,
}

/// Exact documented positive response pair; only an authenticated adapter builds it.
pub struct QiweConversion {
    pub external_gateway: String,
    pub user_id: String,
    pub open_user_id: String,
    pub evidence_ref: Uuid,
}

impl Store {
    /// Source discovery is separate from natural-person confirmation. No display
    /// name, inviter, event operator or model-selected Person is accepted here.
    pub(crate) async fn observe_gateway_subject(
        &self,
        gateway: &str,
        subject_ref: &str,
        evidence_ref: Uuid,
    ) -> Result<Value> {
        ensure!(
            !subject_ref.is_empty()
                && subject_ref.len() <= 256
                && !subject_ref.chars().any(char::is_control),
            "source_subject_invalid"
        );
        let (mut tx, _, _) = self.begin().await?;
        let gateway=sqlx::query("SELECT g.namespace,g.subject_type FROM qintopia_identity.person_identity_gateways g JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key WHERE g.tenant_key=$1 AND g.gateway_key=$2 AND g.active AND s.status='active' FOR SHARE OF g,s")
            .bind(&self.tenant).bind(gateway).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("gateway_not_active"))?;
        let row=sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,adapter_metadata) VALUES($1,$2,$3,jsonb_build_object('first_observation_ref',$4::text,'first_observed_at',clock_timestamp())) ON CONFLICT(namespace,subject_type,source_ref) DO UPDATE SET source_ref=EXCLUDED.source_ref RETURNING id,status,version")
            .bind(gateway.get::<String,_>("namespace")).bind(gateway.get::<String,_>("subject_type")).bind(subject_ref).bind(evidence_ref.to_string()).fetch_one(&mut *tx).await?;
        let result = json!({"link_ref":row.get::<Uuid,_>("id"),"status":row.get::<String,_>("status"),"version":row.get::<i64,_>("version"),"person_auto_created":false});
        tx.commit().await?;
        Ok(result)
    }

    pub(crate) async fn observe_qiwe_conversion(
        &self,
        gateway: &str,
        pair: &QiweConversion,
    ) -> Result<Value> {
        // Account-type mismatches fail before either observation is created.
        let (mut tx, _, _) = self.begin().await?;
        let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.person_identity_gateways q JOIN qintopia_identity.person_identity_gateways e ON e.tenant_key=q.tenant_key WHERE q.tenant_key=$1 AND q.gateway_key=$2 AND e.gateway_key=$3 AND q.subject_type='qiwe_sender' AND e.subject_type='wecom_external' AND q.account_kind='personal' AND e.account_kind='personal' AND q.active AND e.active)")
            .bind(&self.tenant).bind(gateway).bind(&pair.external_gateway).fetch_one(&mut *tx).await?;
        ensure!(valid, "conversion_account_namespace_mismatch");
        ensure!(
            !pair.user_id.is_empty()
                && pair.user_id.len() <= 256
                && !pair.open_user_id.is_empty()
                && pair.open_user_id.len() <= 256,
            "conversion_pair_invalid"
        );
        let mut links = vec![];
        for (key, subject) in [
            (gateway, pair.user_id.as_str()),
            (pair.external_gateway.as_str(), pair.open_user_id.as_str()),
        ] {
            let link:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref) SELECT namespace,subject_type,$3 FROM qintopia_identity.person_identity_gateways WHERE tenant_key=$1 AND gateway_key=$2 AND active ON CONFLICT(namespace,subject_type,source_ref) DO UPDATE SET source_ref=EXCLUDED.source_ref RETURNING id")
                .bind(&self.tenant).bind(key).bind(subject).fetch_one(&mut *tx).await?;
            links.push(link);
        }
        let proof = json!({"adapter":"qiwe/contact/openid","evidence_ref":pair.evidence_ref,"link_refs":links});
        for link in &links {
            sqlx::query("UPDATE qintopia_identity.source_identity_links SET adapter_metadata=jsonb_set(adapter_metadata,'{account_conversion_evidence}',coalesce(adapter_metadata->'account_conversion_evidence','[]'::jsonb)||$2::jsonb) WHERE id=$1 AND NOT coalesce(adapter_metadata->'account_conversion_evidence','[]'::jsonb) @> $2::jsonb")
                .bind(link).bind(json!([proof])).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(json!({"link_refs":links,"account_pair_observed":true,"person_confirmed":false}))
    }

    pub async fn identity_change(&self, actor: &Actor, command: &IdentityCommand) -> Result<Value> {
        let (mut tx, configuration_version, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let row=sqlx::query("SELECT l.person_id,l.status,l.version,g.scope_id FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key WHERE l.id=$1 AND g.tenant_key=$2 AND g.active AND s.status='active' FOR UPDATE OF l")
            .bind(command.link_ref).bind(&self.tenant).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("identity_scope_unbound"))?;
        let scope: Uuid = row.get("scope_id");
        let policy = self.policy(&mut tx, now).await?;
        if let Some((_, _, gateway_scope)) = &actor.gateway {
            ensure!(
                policy.in_scope(scope, *gateway_scope, true),
                "gateway_scope_denied"
            );
        }
        ensure!(
            policy.allowed(actor.person, scope, "default", "organization", "identity")
                || policy
                    .manager(actor.person, scope, "default", "organization", "identity")
                    .is_some(),
            "identity_management_denied"
        );
        let hash = super::super::digest(&serde_json::to_vec(command)?);
        if let Some(previous)=sqlx::query("SELECT actor_identity_id,request_hash,result FROM qintopia_agent_os.collaboration_commands WHERE id=$1 AND tenant_key=$2")
            .bind(command.operation_id).bind(&self.tenant).fetch_optional(&mut *tx).await? {
            ensure!(previous.get::<Uuid,_>("actor_identity_id")==actor.link && previous.get::<String,_>("request_hash")==hash,"identity_operation_conflict");
            return Ok(json!({"replayed":true,"historical_receipt":previous.get::<Value,_>("result"),"current_state_requires_read":true}));
        }
        ensure!(
            row.get::<i64, _>("version") == command.expected_version,
            "identity_version_conflict"
        );
        let current: Option<Uuid> = row.get("person_id");
        ensure!(
            !command.revoke || current == Some(command.person_ref),
            "revoke_person_mismatch"
        );
        ensure!(
            row.get::<String, _>("status") != "confirmed" || current == Some(command.person_ref),
            "revoke_conflicting_link_first"
        );
        let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.persons p JOIN qintopia_identity.source_identity_links l ON l.person_id=p.id WHERE p.id=$1 AND p.status='active' AND l.namespace=$2 AND l.status='confirmed')")
            .bind(command.person_ref).bind(&self.tenant).fetch_one(&mut *tx).await?;
        ensure!(exists, "person_outside_tenant");
        crate::resident_welcome::store::invalidate_link(&mut tx, command.link_ref).await?;
        sqlx::query("UPDATE qintopia_identity.source_identity_links SET person_id=$2,status=$3,version=version+1,evidence_ref=$4,confirmed_by=$5,updated_at=clock_timestamp(),adapter_metadata=jsonb_set(adapter_metadata,'{identity_history}',coalesce(adapter_metadata->'identity_history','[]'::jsonb)||jsonb_build_array(jsonb_build_object('person_ref',person_id,'status',status,'version',version,'evidence_ref',evidence_ref,'confirmed_by',confirmed_by,'ended_at',clock_timestamp()))) WHERE id=$1")
            .bind(command.link_ref).bind(command.person_ref).bind(if command.revoke {"revoked"}else{"confirmed"}).bind(command.evidence_ref).bind(actor.person).execute(&mut *tx).await?;
        let result = json!({"link_ref":command.link_ref,"version":command.expected_version+1,"status":if command.revoke {"revoked"}else{"confirmed"}});
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_commands(id,tenant_key,actor_identity_id,actor_person_id,request_hash,expected_version,result) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(command.operation_id).bind(&self.tenant).bind(actor.link).bind(actor.person).bind(hash).bind(configuration_version).bind(&result).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.collaboration_tenants SET version=version+1 WHERE tenant_key=$1").bind(&self.tenant).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
    /// `gateway` is deployment/session context, never a model supplied namespace.
    pub(crate) async fn gateway_actor(&self, gateway: &str, subject_ref: &str) -> Result<Actor> {
        let (mut tx, _, _) = self.begin().await?;
        let row=sqlx::query("SELECT g.namespace,g.version AS gateway_version,g.scope_id,g.account_kind,l.id,l.person_id,l.version FROM qintopia_identity.person_identity_gateways g JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key JOIN qintopia_identity.source_identity_links l ON l.namespace=g.namespace AND l.subject_type=g.subject_type JOIN qintopia_identity.persons p ON p.id=l.person_id WHERE g.tenant_key=$1 AND g.gateway_key=$2 AND g.active AND s.status='active' AND l.source_ref=$3 AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL AND p.status='active' FOR SHARE OF g,s,l,p")
            .bind(&self.tenant).bind(gateway).bind(subject_ref).fetch_optional(&mut *tx).await?
            .ok_or_else(||anyhow::anyhow!("gateway_identity_unconfirmed"))?;
        ensure!(
            row.get::<String, _>("account_kind") != "shared",
            "shared_account_person_unknown"
        );
        let actor = Actor {
            link: row.get("id"),
            person: row.get("person_id"),
            identity_version: row.get("version"),
            identity_namespace: row.get("namespace"),
            gateway: Some((
                gateway.into(),
                row.get("gateway_version"),
                row.get("scope_id"),
            )),
            session_hash: None,
            tenant: self.tenant.clone(),
        };
        self.verify(&mut tx, &actor).await?;
        Ok(actor)
    }

    /// Purpose-bounded self view; no source identifiers, raw messages or other
    /// buildings' background are returned to an agent at a scoped Gateway.
    pub async fn identity_context(&self, actor: &Actor) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let label:String=sqlx::query_scalar("SELECT coalesce(preferred_name,display_name) FROM qintopia_identity.persons WHERE id=$1")
            .bind(actor.person).fetch_one(&mut *tx).await?;
        let scope = if let Some((_, _, scope)) = &actor.gateway {
            let label:String=sqlx::query_scalar("SELECT label FROM qintopia_agent_os.collaboration_scopes WHERE id=$1 AND tenant_key=$2 AND status='active'")
                .bind(scope).bind(&self.tenant).fetch_one(&mut *tx).await?;
            json!({"id":scope,"label":label})
        } else {
            Value::Null
        };
        Ok(
            json!({"person_ref":actor.person,"preferred_name":label,"identity_status":"confirmed","scope":scope,"purpose":"personal_reply","shared_account":false}),
        )
    }

    /// Persistent history comes from validated PMS projections, never chat or names.
    /// Current-state freshness is independent from permanent community membership.
    pub async fn person_history(&self, actor: &Actor) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let rows=sqlx::query("SELECT h.stay_id,h.first_in_house_observed_at,h.last_observed_at,h.last_state,h.last_building, v.invalidated,v.conflicted,v.projection, s.rebuilding, CASE WHEN g.scope_id IS NULL THEN NULL ELSE cs.kind END AS gateway_scope_kind, g.building_code AS gateway_building_code FROM qintopia_identity.person_stay_history h JOIN qintopia_identity.source_identity_links l ON l.namespace=('pms/'||h.source_instance||'/'||h.property_id||'/occupant') AND l.subject_type='pms_occupant' AND l.source_ref=h.occupant_id AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=h.source_instance AND v.property_id=h.property_id AND v.aggregate_type='order' AND v.aggregate_id=h.order_id JOIN qintopia_agent_os.welcome_sources s ON s.source_instance=h.source_instance AND s.property_id=h.property_id LEFT JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=$2 AND g.gateway_key=$3 LEFT JOIN qintopia_agent_os.collaboration_scopes cs ON cs.id=g.scope_id WHERE l.person_id=$1 ORDER BY h.first_in_house_observed_at")
            .bind(actor.person).bind(&self.tenant).bind(actor.gateway.as_ref().map(|v|v.0.as_str())).fetch_all(&mut *tx).await?;
        let now: chrono::DateTime<chrono::Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        let mut history = vec![];
        let mut current = false;
        let mut uncertain = false;
        for row in &rows {
            let scope_kind: Option<String> = row.get("gateway_scope_kind");
            let scope_label: Option<String> = row.get("gateway_building_code");
            let building: String = row.get("last_building");
            if scope_kind.as_deref() == Some("building")
                && scope_label.as_deref() != Some(building.as_str())
            {
                continue;
            }
            let projection: Value = row.get("projection");
            let observed = projection["observed_at"]
                .as_str()
                .and_then(|s| chrono::DateTime::parse_from_rfc3339(s).ok())
                .map(|v| v.with_timezone(&chrono::Utc));
            let reliable = !row.get::<bool, _>("invalidated")
                && !row.get::<bool, _>("conflicted")
                && !row.get::<bool, _>("rebuilding")
                && observed.is_some_and(|t| {
                    t > now - chrono::Duration::hours(24) && t <= now + chrono::Duration::seconds(5)
                });
            uncertain |= !reliable;
            current |= reliable
                && row.get::<String, _>("last_state") == "InHouse"
                && projection["state"] == "InHouse"
                && projection["stay"] == row.get::<String, _>("stay_id");
            history.push(json!({"stay_ref":row.get::<String,_>("stay_id"),"first_in_house_observed_at":row.get::<chrono::DateTime<chrono::Utc>,_>("first_in_house_observed_at"),"last_observed_at":row.get::<chrono::DateTime<chrono::Utc>,_>("last_observed_at"),"building":building,"last_observed_state":row.get::<String,_>("last_state"),"current_source_reliable":reliable}));
        }
        let has_history = !rows.is_empty();
        // This role denotes the already evidenced enduring relationship, never a
        // business appointment or permission. Checkout does not delete it.
        if has_history {
            sqlx::query("INSERT INTO qintopia_identity.person_memberships(person_id,community_key,role,status,metadata) VALUES($1,$2,'long_term_member','active','{\"source\":\"verified_pms_stay_history\",\"grants_authority\":false}') ON CONFLICT(person_id,community_key,role) DO NOTHING")
                .bind(actor.person).bind(&self.tenant).execute(&mut *tx).await?;
        }
        tx.commit().await?;
        Ok(
            json!({"person_ref":actor.person,"long_term_member":has_history,"current_state":if current {"in_community"}else if uncertain||history.is_empty(){"unknown"}else{"not_in_community"},"stays":history,"purpose":"self_history","non_stay_visits":"deferred"}),
        )
    }

    /// One-shot synthetic setup from the existing fixture Person, no registration
    /// endpoint and no use of a display name as identity evidence.
    pub async fn bootstrap_identity_memory_fixture(&self) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        ensure!(
            self.tenant.starts_with("synthetic-collaboration-"),
            "synthetic_tenant_required"
        );
        let person:Uuid=sqlx::query_scalar("SELECT person_id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND source_ref='fixture-person-1' AND status='confirmed'")
            .bind(&self.tenant).fetch_one(&mut *tx).await?;
        let mut gateways = vec![];
        for (gateway, label, kind) in [
            ("synthetic-qiwe-one", "一栋", "qiwe_sender"),
            ("synthetic-wecom-two", "二栋", "wecom_external"),
        ] {
            let scope:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND label=$2 AND status='active'")
                .bind(&self.tenant).bind(label).fetch_one(&mut *tx).await?;
            let namespace = format!("{}/{}", self.tenant, gateway);
            sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,building_code,account_kind,active) VALUES($1,$2,$3,$4,$5,$6,'personal',true) ON CONFLICT DO NOTHING")
                .bind(&self.tenant).bind(gateway).bind(&namespace).bind(kind).bind(scope).bind(label).execute(&mut *tx).await?;
            sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by) VALUES($1,$2,'synthetic-resident',$3,'confirmed',$4,$3) ON CONFLICT DO NOTHING")
                .bind(&namespace).bind(kind).bind(person).bind(Uuid::new_v4()).execute(&mut *tx).await?;
            gateways
                .push(json!({"gateway":gateway,"subject_ref":"synthetic-resident","scope":label}));
        }
        tx.commit().await?;
        Ok({
            // The synthetic source adapters demonstrate that finding and
            // converting an account still does not confirm a natural person.
            self.observe_gateway_subject("synthetic-qiwe-one", "synthetic-unlinked", person)
                .await?;
            self.observe_qiwe_conversion(
                "synthetic-qiwe-one",
                &QiweConversion {
                    external_gateway: "synthetic-wecom-two".into(),
                    user_id: "synthetic-unlinked".into(),
                    open_user_id: "synthetic-unlinked-external".into(),
                    evidence_ref: person,
                },
            )
            .await?;
            json!({"person_ref":person,"gateways":gateways,"synthetic":true,"real_channel_connected":false})
        })
    }
}
