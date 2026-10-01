//! Scope-bound staff-group choices; display labels never authorize a route.
use super::{Actor, Store};
use crate::person_collaboration::{
    digest,
    model::{PermissionMode, Policy},
};
use anyhow::{ensure, Result};
use base64ct::{Base64UrlUnpadded, Encoding};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommunicationSelection {
    pub staff_group_binding_id: Uuid,
    pub contacts: Vec<CommunicationContactSelection>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct CommunicationContactSelection {
    pub subject_kind: String,
    pub subject_id: Uuid,
    pub channel_source_link_id: Uuid,
}

fn cursor_context(tenant: &str, scope: Uuid, kind: &str, search: &str) -> String {
    digest(format!("{tenant}:{scope}:{kind}:{search}").as_bytes())
}

fn cursor_decode(after: Option<&str>, context: &str) -> Result<(i32, Uuid)> {
    let Some(after) = after else {
        return Ok((-1, Uuid::nil()));
    };
    ensure!(after.len() <= 256, "invalid_candidate_cursor");
    let raw = Base64UrlUnpadded::decode_vec(after)
        .map_err(|_| anyhow::anyhow!("invalid_candidate_cursor"))?;
    let value = std::str::from_utf8(&raw)?;
    let mut fields = value.split(':');
    ensure!(fields.next() == Some(context), "invalid_candidate_cursor");
    let kind = fields
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid_candidate_cursor"))?;
    let id = fields
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid_candidate_cursor"))?;
    ensure!(fields.next().is_none(), "invalid_candidate_cursor");
    let kind: i32 = kind.parse()?;
    ensure!((0..=4).contains(&kind), "invalid_candidate_cursor");
    Ok((kind, Uuid::parse_str(id)?))
}

fn cursor_encode(context: &str, kind: i32, id: Uuid) -> String {
    Base64UrlUnpadded::encode_string(format!("{context}:{kind}:{id}").as_bytes())
}

#[allow(dead_code)] // The host-only current route is wired by a separate change.
fn authority_basis(value: &Value) -> Value {
    let group = &value["staff_group"];
    json!({
        "staff_group": {"binding_id":group["binding_id"],"binding_version":group["binding_version"],
            "conversation_id":group["conversation_id"],"platform":group["platform"]},
        "contacts":value["contacts"].as_array().map(|contacts| contacts.iter().map(|c|json!({
            "subject_kind":c["subject_kind"],"subject_id":c["subject_id"],
            "channel_source_link_id":c["channel_source_link_id"],"source_version":c["source_version"],
            "gateway":c["gateway"],"gateway_version":c["gateway_version"],
            "platform":c["platform"],"account_version":c["account_version"]
        })).collect::<Vec<_>>())
    })
}

impl Store {
    async fn communication_allowed_groups_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: Uuid,
        policy: &Policy,
    ) -> Result<Vec<Uuid>> {
        let collaborations = policy
            .grants
            .iter()
            .filter(|grant| {
                grant.agent == "anan"
                    && grant.domain == "hospitality"
                    && grant.action == "read_business"
                    && grant.mode == PermissionMode::Autonomous
                    && policy.effective(grant)
                    && policy.decision(grant.collaboration, "read_business")["status"]
                        == "autonomous"
                    && policy.in_scope(scope, grant.scope, grant.descendants)
            })
            .map(|grant| grant.collaboration)
            .collect::<Vec<_>>();
        if collaborations.is_empty() {
            return Ok(Vec::new());
        }
        let rows = sqlx::query("SELECT configuration FROM qintopia_agent_os.collaboration_audiences WHERE tenant_key=$1 AND collaboration_id=ANY($2)")
            .bind(&self.tenant).bind(&collaborations).fetch_all(&mut **tx).await?;
        let mut groups = std::collections::BTreeSet::new();
        for row in rows {
            let configuration: Value = row.get("configuration");
            if configuration["proactive"] != "autonomous" {
                continue;
            }
            for group in configuration["groups"].as_array().into_iter().flatten() {
                if let Some(id) = group.as_str().and_then(|value| Uuid::parse_str(value).ok()) {
                    groups.insert(id);
                }
            }
        }
        Ok(groups.into_iter().collect())
    }

    pub(crate) async fn scope_communication_candidates(
        &self,
        actor: &Actor,
        scope: Uuid,
        kind: &str,
        search: &str,
        limit: i64,
        after: Option<&str>,
    ) -> Result<Value> {
        ensure!(
            (1..=100).contains(&limit)
                && matches!(
                    kind,
                    "groups" | "contacts" | "observed_accounts" | "accounts"
                )
                && search.chars().count() <= 120
                && !search.chars().any(char::is_control),
            "invalid_candidate_query"
        );
        let context = cursor_context(&self.tenant, scope, kind, search);
        let (after_kind, after_id) = cursor_decode(after, &context)?;
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let policy = self.policy(&mut tx, now).await?;
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
        let active: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND id=$2 AND status='active')")
            .bind(&self.tenant).bind(scope).fetch_one(&mut *tx).await?;
        ensure!(active, "business_scope_unavailable");
        let allowed_groups = self
            .communication_allowed_groups_in(&mut tx, scope, &policy)
            .await?;
        let rows = sqlx::query(
            r#"
            WITH candidates AS (
                SELECT 0::integer AS kind, b.id,
                  jsonb_build_object('binding_id',b.id,'binding_version',b.version,
                    'conversation_id',c.id,'label',c.display_name,'platform',c.platform,
                    'scope',$2::uuid) AS value
                FROM qintopia_agent_os.collaboration_scope_bindings b
                JOIN qintopia_messages.conversations c ON c.id=b.conversation_id AND c.tenant_id=b.tenant_key
                WHERE b.tenant_key=$1 AND b.scope_id=$2 AND b.revoked_at IS NULL
                  AND c.status='active' AND c.chat_type='group' AND c.platform IN ('wecom','qiwe')
                  AND NOT EXISTS (SELECT 1 FROM qintopia_agent_os.collaboration_ledger l
                    WHERE l.tenant_key=$1 AND l.kind='group' AND l.object_ref=c.id::text AND l.status<>'active')
                  AND c.id=ANY($8::uuid[])
                UNION ALL
                SELECT 1, l.id,
                  jsonb_build_object('subject_kind','person','subject_id',p.id,
                    'subject_label',coalesce(p.preferred_name,p.display_name),
                    'channel_source_link_id',l.id,
                    'channel_label',coalesce(nullif(ci.display_name,''),nullif(l.adapter_metadata->>'display_name',''),'已观测账号'),
                    'platform',CASE WHEN g.subject_type='qiwe_sender' THEN 'qiwe' ELSE 'wecom' END,
                    'gateway',g.gateway_key,'source_version',l.version,'gateway_version',g.version,
                    'scope',$2::uuid,'label',coalesce(p.preferred_name,p.display_name)||' · '||g.subject_type) AS value
                FROM qintopia_identity.source_identity_links l
                JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type
                JOIN qintopia_identity.persons p ON p.id=l.person_id AND p.status='active'
                LEFT JOIN qintopia_identity.channel_identities ci ON ci.id=l.channel_identity_id
                WHERE g.tenant_key=$1 AND g.scope_id=$2 AND g.active AND g.account_kind IN ('personal','employee')
                  AND g.subject_type IN ('wecom_internal','qiwe_sender')
                  AND l.status='confirmed' AND l.evidence_ref IS NOT NULL
                  AND (l.confirmed_by IS NOT NULL OR l.confirmed_by_work_account IS NOT NULL)
                  AND l.adapter_metadata ? 'first_observation_ref'
                  AND (SELECT count(*) FROM qintopia_identity.person_identity_gateways x
                    WHERE x.namespace=g.namespace AND x.subject_type=g.subject_type AND x.active)=1
                UNION ALL
                SELECT 2, l.id,
                  jsonb_build_object('subject_kind','work_account','subject_id',w.id,
                    'subject_label',w.label,'channel_source_link_id',l.id,
                    'channel_label',coalesce(nullif(ci.display_name,''),nullif(l.adapter_metadata->>'display_name',''),w.label),
                    'platform',CASE WHEN g.subject_type='qiwe_sender' THEN 'qiwe' ELSE 'wecom' END,
                    'gateway',g.gateway_key,'source_version',l.version,'gateway_version',g.version,
                    'account_version',w.version,'scope',$2::uuid,'label',w.label||' · 工作账号') AS value
                FROM qintopia_identity.work_accounts w
                JOIN qintopia_identity.source_identity_links l ON l.id=w.source_link_id
                JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=w.tenant_key AND g.gateway_key=w.gateway_key
                  AND g.namespace=l.namespace AND g.subject_type=l.subject_type
                LEFT JOIN qintopia_identity.channel_identities ci ON ci.id=l.channel_identity_id
                WHERE w.tenant_key=$1 AND g.scope_id=$2 AND w.active AND g.active AND g.account_kind='shared'
                  AND g.subject_type IN ('wecom_internal','qiwe_sender') AND l.person_id IS NULL
                  AND l.status<>'revoked' AND l.adapter_metadata ? 'first_observation_ref'
                  AND w.source_version=l.version AND w.gateway_version=g.version
                  AND (SELECT count(*) FROM qintopia_identity.person_identity_gateways x
                    WHERE x.namespace=g.namespace AND x.subject_type=g.subject_type AND x.active)=1
                UNION ALL
                SELECT 3, l.id,
                  jsonb_build_object('source_link',l.id,'gateway',g.gateway_key,'scope',$2::uuid,
                    'label',coalesce(nullif(ci.display_name,''),nullif(l.adapter_metadata->>'display_name',''),'待登记工作账号'),
                    'source_type',g.subject_type,'source_version',l.version,'gateway_version',g.version,
                    'account_kind','shared','registered',false) AS value
                FROM qintopia_identity.source_identity_links l
                JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type
                LEFT JOIN qintopia_identity.channel_identities ci ON ci.id=l.channel_identity_id
                WHERE g.tenant_key=$1 AND g.scope_id=$2 AND g.active AND g.account_kind='shared'
                  AND g.subject_type IN ('wecom_internal','qiwe_sender')
                  AND l.person_id IS NULL AND l.status='pending' AND l.adapter_metadata ? 'first_observation_ref'
                  AND NOT EXISTS(SELECT 1 FROM qintopia_identity.work_accounts w
                    WHERE w.tenant_key=$1 AND w.source_link_id=l.id AND w.active)
                  AND (SELECT count(*) FROM qintopia_identity.person_identity_gateways x
                    WHERE x.namespace=g.namespace AND x.subject_type=g.subject_type AND x.active)=1
                UNION ALL
                SELECT 4, w.id,
                  jsonb_build_object('id',w.id,'source_link',l.id,'gateway',g.gateway_key,
                    'scope',$2::uuid,'label',w.label,'active',w.active,'version',w.version,
                    'source_version',l.version,'gateway_version',g.version,'source_type',g.subject_type,
                    'current',w.active AND g.active AND w.source_version=l.version AND w.gateway_version=g.version
                      AND l.status<>'revoked' AND l.adapter_metadata ? 'first_observation_ref') AS value
                FROM qintopia_identity.work_accounts w
                JOIN qintopia_identity.source_identity_links l ON l.id=w.source_link_id
                JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=w.tenant_key AND g.gateway_key=w.gateway_key
                  AND g.namespace=l.namespace AND g.subject_type=l.subject_type
                WHERE w.tenant_key=$1 AND g.scope_id=$2 AND g.account_kind='shared'
            )
            SELECT kind,id,value FROM candidates
            WHERE (kind,id)>($4::integer,$5::uuid)
              AND ($7='groups' AND kind=0 OR $7='contacts' AND kind IN (1,2)
                   OR $7='observed_accounts' AND kind=3 OR $7='accounts' AND kind=4)
              AND ($3='' OR strpos(lower(concat_ws(' ',value->>'label',value->>'channel_label',
                    value->>'gateway',value->>'source_type',value->>'platform')),lower($3))>0)
            ORDER BY kind,id LIMIT $6
            "#,
        )
        .bind(&self.tenant)
        .bind(scope)
        .bind(search)
        .bind(after_kind)
        .bind(after_id)
        .bind(limit + 1)
        .bind(kind)
        .bind(&allowed_groups)
        .fetch_all(&mut *tx)
        .await?;
        let has_more = rows.len() as i64 > limit;
        let shown = rows.iter().take(limit as usize).collect::<Vec<_>>();
        let next = if has_more {
            shown
                .last()
                .map(|row| cursor_encode(&context, row.get("kind"), row.get("id")))
        } else {
            None
        };
        let mut accounts = Vec::new();
        for row in &shown {
            if row.get::<i32, _>("kind") != 4 {
                continue;
            }
            let grants = sqlx::query("SELECT o.id,o.binding_id,b.property_id,o.account_role,o.operation_key,o.valid_until,b.scope_id FROM qintopia_agent_os.business_operation_grants o JOIN qintopia_agent_os.business_property_bindings b ON b.tenant_key=o.tenant_key AND b.id=o.binding_id WHERE o.tenant_key=$1 AND o.work_account_id=$2 AND o.revoked_at IS NULL AND b.scope_id=$3 ORDER BY o.id LIMIT 257")
                .bind(&self.tenant).bind(row.get::<Uuid,_>("id")).bind(scope).fetch_all(&mut *tx).await?;
            let can_disable = grants.len() <= 256
                && grants.iter().all(|grant| {
                    super::business::operation(&grant.get::<String, _>("operation_key"))
                        .ok()
                        .and_then(|spec| spec["action"].as_str().map(str::to_owned))
                        .is_some_and(|action| {
                            policy
                                .manager(
                                    actor.person,
                                    grant.get("scope_id"),
                                    "anan",
                                    "hospitality",
                                    &action,
                                )
                                .is_some()
                        })
                });
            let mut value: Value = row.get("value");
            value["can_disable"] = json!(can_disable);
            value["grants"]=json!(grants.iter().take(256).map(|g|json!({
                "id":g.get::<Uuid,_>("id"),"binding":g.get::<Uuid,_>("binding_id"),
                "property":g.get::<String,_>("property_id"),"role":g.get::<Option<String>,_>("account_role"),
                "operation":g.get::<String,_>("operation_key"),
                "valid_until":g.get::<Option<chrono::DateTime<chrono::Utc>>,_>("valid_until")
            })).collect::<Vec<_>>());
            value["grants_has_more"] = json!(grants.len() > 256);
            accounts.push(value);
        }
        Ok(json!({
            "scope":scope,
            "groups":shown.iter().filter(|row|row.get::<i32,_>("kind")==0).map(|row|row.get::<Value,_>("value")).collect::<Vec<_>>(),
            "contacts":shown.iter().filter(|row|matches!(row.get::<i32,_>("kind"),1|2)).map(|row|row.get::<Value,_>("value")).collect::<Vec<_>>(),
            "observed_accounts":shown.iter().filter(|row|row.get::<i32,_>("kind")==3).map(|row|row.get::<Value,_>("value")).collect::<Vec<_>>(),
            "accounts":accounts,
            "has_more":has_more,"next":next
        }))
    }

    pub(crate) async fn validate_scope_communication_in(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: Uuid,
        selection: &CommunicationSelection,
    ) -> Result<Value> {
        ensure!(selection.contacts.len() <= 20, "too_many_contacts");
        let policy = self.policy(tx, chrono::Utc::now()).await?;
        let allowed_groups = self
            .communication_allowed_groups_in(tx, scope, &policy)
            .await?;
        if self.is_live() {
            sqlx::query("SELECT qintopia_identity.management_ui_lock_gateway_registry()")
                .execute(&mut **tx)
                .await?;
        } else {
            sqlx::query("LOCK TABLE qintopia_identity.person_identity_gateways IN SHARE MODE")
                .execute(&mut **tx)
                .await?;
        }
        let group = sqlx::query(
            "SELECT b.id,b.version,c.id AS conversation,c.platform,c.display_name FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id AND c.tenant_id=b.tenant_key JOIN qintopia_agent_os.collaboration_scopes s ON s.id=b.scope_id AND s.tenant_key=b.tenant_key WHERE b.tenant_key=$1 AND b.scope_id=$2 AND b.id=$3 AND c.id=ANY($4::uuid[]) AND b.revoked_at IS NULL AND s.status='active' AND c.status='active' AND c.chat_type='group' AND c.platform IN ('wecom','qiwe') AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger l WHERE l.tenant_key=$1 AND l.kind='group' AND l.object_ref=c.id::text AND l.status<>'active') FOR SHARE OF b,c,s"
        ).bind(&self.tenant).bind(scope).bind(selection.staff_group_binding_id).bind(&allowed_groups).fetch_optional(&mut **tx).await?
            .ok_or_else(||anyhow::anyhow!("communication_group_unavailable"))?;
        let platform: String = group.get("platform");
        let mut contacts = Vec::new();
        let mut seen = std::collections::BTreeSet::new();
        for contact in &selection.contacts {
            ensure!(
                seen.insert(contact.channel_source_link_id),
                "duplicate_communication_contact"
            );
            if self.is_live() {
                let locked: bool = match contact.subject_kind.as_str() {
                    "person" => sqlx::query_scalar("SELECT qintopia_identity.management_ui_lock_communication_person($1,$2,$3,$4)")
                        .bind(&self.tenant).bind(scope).bind(contact.channel_source_link_id).bind(contact.subject_id)
                        .fetch_one(&mut **tx).await?,
                    "work_account" => sqlx::query_scalar("SELECT qintopia_identity.management_ui_lock_business_account($1,$2)")
                        .bind(&self.tenant).bind(contact.subject_id).fetch_one(&mut **tx).await?,
                    _ => anyhow::bail!("invalid_communication_contact"),
                };
                ensure!(locked, "communication_contact_unavailable");
            }
            let value: Option<Value> = match contact.subject_kind.as_str() {
                "person" => {
                    let query = "SELECT jsonb_build_object('subject_kind','person','subject_id',p.id,'subject_label',coalesce(p.preferred_name,p.display_name),'channel_source_link_id',l.id,'channel_label',coalesce(nullif(ci.display_name,''),nullif(l.adapter_metadata->>'display_name',''),'已观测账号'),'platform',CASE WHEN g.subject_type='qiwe_sender' THEN 'qiwe' ELSE 'wecom' END,'source_version',l.version,'gateway',g.gateway_key,'gateway_version',g.version) FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type JOIN qintopia_identity.persons p ON p.id=l.person_id LEFT JOIN qintopia_identity.channel_identities ci ON ci.id=l.channel_identity_id WHERE g.tenant_key=$1 AND g.scope_id=$2 AND g.active AND g.account_kind IN ('personal','employee') AND l.id=$3 AND p.id=$4 AND p.status='active' AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND (l.confirmed_by IS NOT NULL OR l.confirmed_by_work_account IS NOT NULL) AND l.adapter_metadata ? 'first_observation_ref' AND g.subject_type IN ('wecom_internal','qiwe_sender') AND (SELECT count(*) FROM qintopia_identity.person_identity_gateways x WHERE x.namespace=g.namespace AND x.subject_type=g.subject_type AND x.active)=1";
                    let query = if self.is_live() {
                        query.to_owned()
                    } else {
                        format!("{query} FOR SHARE OF l,g,p")
                    };
                    sqlx::query_scalar(&query)
                        .bind(&self.tenant)
                        .bind(scope)
                        .bind(contact.channel_source_link_id)
                        .bind(contact.subject_id)
                        .fetch_optional(&mut **tx)
                        .await?
                }
                "work_account" => {
                    let query = "SELECT jsonb_build_object('subject_kind','work_account','subject_id',w.id,'subject_label',w.label,'channel_source_link_id',l.id,'channel_label',coalesce(nullif(ci.display_name,''),nullif(l.adapter_metadata->>'display_name',''),w.label),'platform',CASE WHEN g.subject_type='qiwe_sender' THEN 'qiwe' ELSE 'wecom' END,'source_version',l.version,'gateway',g.gateway_key,'gateway_version',g.version,'account_version',w.version) FROM qintopia_identity.work_accounts w JOIN qintopia_identity.source_identity_links l ON l.id=w.source_link_id JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=w.tenant_key AND g.gateway_key=w.gateway_key AND g.namespace=l.namespace AND g.subject_type=l.subject_type LEFT JOIN qintopia_identity.channel_identities ci ON ci.id=l.channel_identity_id WHERE w.tenant_key=$1 AND g.scope_id=$2 AND w.id=$4 AND l.id=$3 AND w.active AND g.active AND g.account_kind='shared' AND l.person_id IS NULL AND l.status<>'revoked' AND l.adapter_metadata ? 'first_observation_ref' AND w.source_version=l.version AND w.gateway_version=g.version AND g.subject_type IN ('wecom_internal','qiwe_sender') AND (SELECT count(*) FROM qintopia_identity.person_identity_gateways x WHERE x.namespace=g.namespace AND x.subject_type=g.subject_type AND x.active)=1";
                    let query = if self.is_live() {
                        query.to_owned()
                    } else {
                        format!("{query} FOR SHARE OF w,l,g")
                    };
                    sqlx::query_scalar(&query)
                        .bind(&self.tenant)
                        .bind(scope)
                        .bind(contact.channel_source_link_id)
                        .bind(contact.subject_id)
                        .fetch_optional(&mut **tx)
                        .await?
                }
                _ => anyhow::bail!("invalid_communication_contact"),
            };
            contacts
                .push(value.ok_or_else(|| anyhow::anyhow!("communication_contact_unavailable"))?);
        }
        Ok(
            json!({"staff_group":{"binding_id":group.get::<Uuid,_>("id"),"binding_version":group.get::<i64,_>("version"),"conversation_id":group.get::<Uuid,_>("conversation"),"platform":platform,"label":group.get::<String,_>("display_name")},"contacts":contacts}),
        )
    }

    #[allow(dead_code)] // Retained for the host-only route after channel selection.
    pub(crate) async fn scope_communication_current(
        &self,
        gateway: &str,
        binding: Uuid,
    ) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        let row=sqlx::query("SELECT s.id AS scope,s.version,s.communication_config FROM qintopia_agent_os.business_property_bindings b JOIN qintopia_agent_os.collaboration_scopes s ON s.id=b.scope_id AND s.tenant_key=b.tenant_key JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=b.tenant_key AND g.scope_id=b.scope_id WHERE b.tenant_key=$1 AND b.id=$2 AND b.active AND s.status='active' AND g.gateway_key=$3 AND g.active")
            .bind(&self.tenant).bind(binding).bind(gateway).fetch_optional(&mut *tx).await?
            .ok_or_else(||anyhow::anyhow!("communication_binding_unavailable"))?;
        let scope: Uuid = row.get("scope");
        let version: i64 = row.get("version");
        let saved: Value = row.get("communication_config");
        if saved == json!({}) {
            return Ok(
                json!({"configured":false,"current":false,"scope":scope,"config_version":version,"staff_group":null,"contacts":[]}),
            );
        }
        let selection = (|| -> Result<CommunicationSelection> {
            Ok(CommunicationSelection {
                staff_group_binding_id: serde_json::from_value(
                    saved["staff_group"]["binding_id"].clone(),
                )?,
                contacts: saved["contacts"]
                    .as_array()
                    .ok_or_else(|| anyhow::anyhow!("invalid_communication_config"))?
                    .iter()
                    .map(|c| {
                        Ok(CommunicationContactSelection {
                            subject_kind: serde_json::from_value(c["subject_kind"].clone())?,
                            subject_id: serde_json::from_value(c["subject_id"].clone())?,
                            channel_source_link_id: serde_json::from_value(
                                c["channel_source_link_id"].clone(),
                            )?,
                        })
                    })
                    .collect::<Result<Vec<_>>>()?,
            })
        })();
        let selection = match selection {
            Ok(selection) => selection,
            Err(_) => {
                return Ok(
                    json!({"configured":true,"current":false,"scope":scope,"config_version":version,"staff_group":null,"contacts":[]}),
                )
            }
        };
        let validated = self
            .validate_scope_communication_in(&mut tx, scope, &selection)
            .await;
        let current = validated
            .as_ref()
            .is_ok_and(|value| authority_basis(value) == authority_basis(&saved));
        if !current {
            return Ok(
                json!({"configured":true,"current":false,"scope":scope,"config_version":version,"staff_group":null,"contacts":[]}),
            );
        }
        let group=sqlx::query("SELECT b.id,b.version,c.id AS conversation,c.platform,c.chat_id,c.display_name FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id AND c.tenant_id=b.tenant_key WHERE b.tenant_key=$1 AND b.scope_id=$2 AND b.id=$3 AND b.revoked_at IS NULL AND c.status='active'")
            .bind(&self.tenant).bind(scope).bind(selection.staff_group_binding_id).fetch_one(&mut *tx).await?;
        Ok(
            json!({"configured":true,"current":true,"scope":scope,"config_version":version,"staff_group":{"binding_id":group.get::<Uuid,_>("id"),"binding_version":group.get::<i64,_>("version"),"conversation_id":group.get::<Uuid,_>("conversation"),"platform":group.get::<String,_>("platform"),"chat_id":group.get::<String,_>("chat_id"),"label":group.get::<String,_>("display_name")},"contacts":validated.unwrap()["contacts"]}),
        )
    }
}
