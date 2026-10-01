//! Trusted, purpose-scoped directory selection. Read-only; selection never grants authority.
use super::{
    audience_contacts::CONTACT_GATEWAY_OWNERSHIP_SQL, foundation::authorize_current,
    organization::AudienceResolution, Actor, Audience, Store,
};
use crate::person_collaboration::{digest, model::*};
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

#[derive(Default, Deserialize, Serialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Query {
    pub scope: Uuid,
    pub kind: String,
    pub purpose: String,
    pub search: String,
    pub limit: Option<i64>,
    pub after: Option<String>,
    pub subject_kind: Option<String>,
    pub subject_ref: Option<Uuid>,
    pub collaboration: Option<Uuid>,
}
impl Query {
    pub(crate) fn from_path(path: &str) -> Result<Self> {
        let raw = path
            .strip_prefix("/api/workspace/candidates?")
            .ok_or_else(|| anyhow::anyhow!("invalid_candidate_query"))?;
        ensure!(raw.len() <= 4096, "invalid_candidate_query");
        let mut fields = serde_json::Map::new();
        for (k, v) in url::form_urlencoded::parse(raw.as_bytes()) {
            ensure!(!fields.contains_key(k.as_ref()), "invalid_candidate_query");
            let value = if k == "limit" {
                json!(v
                    .parse::<i64>()
                    .map_err(|_| anyhow::anyhow!("invalid_candidate_query"))?)
            } else {
                json!(v)
            };
            fields.insert(k.into_owned(), value);
        }
        serde_json::from_value(Value::Object(fields))
            .map_err(|_| anyhow::anyhow!("invalid_candidate_query"))
    }
    fn validate(&self) -> Result<i64> {
        let limit = self.limit.unwrap_or(50);
        ensure!(
            !self.scope.is_nil()
                && (1..=50).contains(&limit)
                && self.search.chars().count() <= 80
                && !self.search.contains('\0'),
            "invalid_candidate_query"
        );
        ensure!(
            matches!(
                (self.purpose.as_str(), self.kind.as_str()),
                ("assign" | "delegate_review", "people")
                    | ("contact", "people" | "accounts" | "channels")
                    | ("set_groups", "groups")
            ),
            "invalid_candidate_query"
        );
        ensure!(
            if self.kind == "channels" {
                matches!(
                    self.subject_kind.as_deref(),
                    Some("person" | "work_account")
                ) && self.subject_ref.is_some_and(|id| !id.is_nil())
            } else {
                self.subject_kind.is_none() && self.subject_ref.is_none()
            },
            "invalid_candidate_query"
        );
        ensure!(
            self.collaboration
                .is_none_or(|id| self.purpose == "contact" && !id.is_nil()),
            "invalid_candidate_query"
        );
        Ok(limit)
    }
    fn fingerprint(&self, tenant: &str, actor: Uuid) -> String {
        digest(
            &serde_json::to_vec(&json!([
                tenant,
                actor,
                self.scope,
                self.kind,
                self.purpose,
                self.search,
                self.subject_kind,
                self.subject_ref,
                self.collaboration
            ]))
            .expect("JSON query"),
        )
    }
}
#[derive(Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
struct Cursor {
    query: String,
    label: String,
    reference: Uuid,
}
fn encode_cursor(cursor: &Cursor) -> Result<String> {
    Ok(url::form_urlencoded::byte_serialize(&serde_json::to_vec(cursor)?).collect())
}
fn decode_cursor(raw: &str, query: &str) -> Result<Cursor> {
    ensure!(raw.len() <= 2048, "invalid_candidate_query");
    let decoded = url::form_urlencoded::parse(format!("c={raw}").as_bytes())
        .next()
        .ok_or_else(|| anyhow::anyhow!("invalid_candidate_query"))?
        .1
        .into_owned();
    let cursor: Cursor =
        serde_json::from_str(&decoded).map_err(|_| anyhow::anyhow!("invalid_candidate_query"))?;
    ensure!(
        cursor.query == query && cursor.label.len() <= 1000 && !cursor.reference.is_nil(),
        "invalid_candidate_query"
    );
    Ok(cursor)
}

// Resolver diagnostics occur both globally and on individual resident rows.
fn residence_directory_available(resolved: &AudienceResolution) -> bool {
    let unavailable = |reason: &str| reason.starts_with("pms_");
    resolved.sources_available
        && !resolved
            .unresolved
            .iter()
            .any(|v| v["reason"].as_str().is_some_and(unavailable))
        && !resolved.people.iter().any(|p| {
            p["reasons"]
                .as_array()
                .is_some_and(|reasons| reasons.iter().any(|v| v.as_str().is_some_and(unavailable)))
        })
}

impl Store {
    pub(crate) async fn workspace_candidates(&self, actor: &Actor, q: &Query) -> Result<Value> {
        let limit = q.validate()?;
        let fingerprint = q.fingerprint(&self.tenant, actor.person);
        let cursor = q
            .after
            .as_deref()
            .map(|raw| decode_cursor(raw, &fingerprint))
            .transpose()?;
        let (mut tx, version, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        if let Some(scope) = actor.foundation_scope() {
            ensure!(scope == q.scope, "scope_access_denied");
        }
        let policy = self.policy(&mut tx, now).await?;
        ensure!(
            policy.scopes.iter().any(|s| s.id == q.scope && s.active),
            "scope_access_denied"
        );
        let manager = policy.grants.iter().any(|g| {
            g.person == actor.person
                && g.action == "manage"
                && g.mode == PermissionMode::Autonomous
                && g.reviewer.is_none()
                && policy.effective(g)
                && policy.in_scope(q.scope, g.scope, g.descendants)
                && g.delegation.actions.iter().any(|a| a == "designate")
        });
        let mut eligible: Option<Vec<Uuid>> = None;
        match q.purpose.as_str() {
            "assign" => ensure!(manager, "scope_access_denied"),
            "set_groups" => {
                let root = policy
                    .scopes
                    .iter()
                    .find(|s| s.parent.is_none() && s.active)
                    .ok_or_else(|| anyhow::anyhow!("scope_access_denied"))?;
                ensure!(
                    policy
                        .manager(actor.person, root.id, "default", "organization", "manage")
                        .is_some()
                        && policy.in_scope(q.scope, root.id, true),
                    "scope_access_denied"
                );
            }
            "delegate_review" => {
                let auth = authorize_current(
                    &mut tx,
                    &self.tenant,
                    actor.person,
                    q.scope,
                    "erhua",
                    "community_service",
                    "designate",
                )
                .await?;
                ensure!(auth.status != "denied", "scope_access_denied");
                let audience: Audience = serde_json::from_value(
                    json!({"groups":[],"people":[],"residents":"current","reply":"autonomous","proactive":"denied","reviewer":null,"visibility":"general","topics":""}),
                )?;
                let resolved = self
                    .resolve_audience(&mut tx, &policy, q.scope, &audience, now)
                    .await?;
                ensure!(
                    residence_directory_available(&resolved),
                    "candidate_source_unavailable"
                );
                eligible = Some(
                    resolved
                        .people
                        .iter()
                        .filter(|p| {
                            p["status"] == "current" && p["person_ref"] != json!(actor.person)
                        })
                        .filter_map(|p| serde_json::from_value(p["person_ref"].clone()).ok())
                        .collect(),
                );
            }
            "contact" => {
                let connections: Vec<Uuid> = if let Some(target) = q.collaboration {
                    // Bind directory access to the exact saved connection being configured.
                    // Use the same policy checks as set_audience, not the assign directory gate.
                    let row = sqlx::query("SELECT a.scope_id FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND c.id=$2 AND c.agent_key='erhua' AND c.domain_key='community_service' AND c.status='active' AND a.status='active' AND (a.valid_until IS NULL OR a.valid_until>$3)")
                        .bind(&self.tenant).bind(target).bind(now).fetch_optional(&mut *tx).await?
                        .ok_or_else(|| anyhow::anyhow!("scope_access_denied"))?;
                    ensure!(
                        row.get::<Uuid, _>("scope_id") == q.scope,
                        "scope_access_denied"
                    );
                    let owns_connection = policy.grants.iter().any(|g| {
                        g.collaboration == target && g.person == actor.person && policy.effective(g)
                    });
                    let can_configure =
                        policy.can_inspect(actor.person, q.scope, "erhua", "community_service")
                            && policy
                                .manager(
                                    actor.person,
                                    q.scope,
                                    "erhua",
                                    "community_service",
                                    "publish",
                                )
                                .is_some();
                    ensure!(owns_connection || can_configure, "scope_access_denied");
                    ensure!(
                        policy
                            .grants
                            .iter()
                            .any(|g| g.collaboration == target && policy.effective(g)),
                        "scope_access_denied"
                    );
                    vec![target]
                } else {
                    policy
                        .grants
                        .iter()
                        .filter(|g| {
                            g.person == actor.person
                                && g.agent == "erhua"
                                && g.domain == "community_service"
                                && policy.effective(g)
                                && policy.in_scope(q.scope, g.scope, g.descendants)
                        })
                        .map(|g| g.collaboration)
                        .collect()
                };
                ensure!(!connections.is_empty(), "scope_access_denied");
                let audiences: Vec<Value> = sqlx::query_scalar("SELECT configuration FROM qintopia_agent_os.collaboration_audiences WHERE tenant_key=$1 AND collaboration_id=ANY($2)").bind(&self.tenant).bind(&connections).fetch_all(&mut *tx).await?;
                ensure!(!audiences.is_empty(), "scope_access_denied");
                let mut people = std::collections::BTreeSet::new();
                for mut value in audiences {
                    let authority = value
                        .as_object_mut()
                        .and_then(|o| o.remove("authority_grant"))
                        .and_then(|v| serde_json::from_value::<Uuid>(v).ok());
                    ensure!(
                        authority.is_some_and(|id| policy
                            .grants
                            .iter()
                            .any(|g| g.id == id && policy.effective(g))),
                        "scope_access_denied"
                    );
                    let audience = Audience::from_configuration(value)?;
                    if q.kind == "accounts" || q.subject_kind.as_deref() == Some("work_account") {
                        continue;
                    }
                    let resolved = self
                        .resolve_audience(&mut tx, &policy, q.scope, &audience, now)
                        .await?;
                    ensure!(
                        audience.residents == "none" || residence_directory_available(&resolved),
                        "candidate_source_unavailable"
                    );
                    for person in resolved.people.iter().filter(|p| p["selected"] == true) {
                        if let Ok(id) = serde_json::from_value(person["person_ref"].clone()) {
                            people.insert(id);
                        }
                    }
                }
                eligible = Some(people.into_iter().collect());
                if q.kind == "channels" && q.subject_kind.as_deref() == Some("person") {
                    ensure!(
                        eligible
                            .as_ref()
                            .is_some_and(|ids| ids.contains(&q.subject_ref.unwrap())),
                        "scope_access_denied"
                    );
                }
            }
            _ => unreachable!(),
        }
        // Gateway directory entries belong to this tenant and the selected scope/its ancestors.
        let gateways: Vec<String> = sqlx::query(&format!("SELECT g.gateway_key,g.scope_id FROM qintopia_identity.person_identity_gateways g WHERE g.tenant_key=$1 AND g.active AND {CONTACT_GATEWAY_OWNERSHIP_SQL}")).bind(&self.tenant).fetch_all(&mut *tx).await?.iter().filter(|r| policy.in_scope(q.scope, r.get("scope_id"), true)).map(|r| r.get("gateway_key")).collect();
        let mut channel_refs = Vec::<Uuid>::new();
        if q.kind == "channels" && q.subject_kind.as_deref() == Some("person") {
            let person = q.subject_ref.unwrap();
            let links = sqlx::query("SELECT DISTINCT l.id,l.confirmed_by FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type WHERE g.tenant_key=$1 AND g.gateway_key=ANY($2) AND g.active AND g.account_kind<>'shared' AND l.person_id=$3 AND l.status='confirmed' AND l.evidence_ref IS NOT NULL").bind(&self.tenant).bind(&gateways).bind(person).fetch_all(&mut *tx).await?;
            for link in links {
                let reference = link.get("id");
                if link.get::<Option<Uuid>, _>("confirmed_by").is_some()
                    || self
                        .work_account_person_proof(&mut tx, reference, person)
                        .await?
                {
                    channel_refs.push(reference);
                }
            }
        }
        let directory = match q.kind.as_str() {
            "people" => {
                r#"SELECT p.id AS ref, CASE WHEN nullif(p.preferred_name,'') IS NOT NULL AND p.preferred_name<>coalesce(nullif(p.primary_name,''),p.display_name) THEN coalesce(nullif(p.primary_name,''),p.display_name)||'（昵称：'||p.preferred_name||'）' ELSE coalesce(nullif(p.primary_name,''),p.display_name) END AS label, jsonb_build_object('description','已关联人员') AS extra FROM qintopia_identity.persons p CROSS JOIN args a WHERE p.status='active' AND (a.eligible IS NULL OR p.id=ANY(a.eligible)) AND EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l WHERE l.namespace=a.namespace AND l.person_id=p.id AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL AND coalesce(l.adapter_metadata->>'account_kind','personal')<>'shared') AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger x WHERE x.tenant_key=a.tenant AND x.kind='person' AND x.object_ref=p.id::text AND x.status<>'active')"#
            }
            "accounts" => {
                r#"SELECT w.id AS ref,w.label,jsonb_build_object('description','已验证工作账号') AS extra FROM qintopia_identity.work_accounts w CROSS JOIN args a JOIN qintopia_identity.source_identity_links l ON l.id=w.source_link_id JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=w.tenant_key AND g.gateway_key=w.gateway_key WHERE w.tenant_key=a.tenant AND w.active AND w.gateway_key=ANY(a.gateways) AND g.active AND g.account_kind='shared' AND g.version=w.gateway_version AND l.version=w.source_version AND l.namespace=g.namespace AND l.subject_type=g.subject_type AND l.person_id IS NULL AND l.status<>'revoked' AND l.adapter_metadata ? 'first_observation_ref'"#
            }
            "channels" => {
                r#"SELECT l.id AS ref,coalesce(nullif(l.adapter_metadata->>'display_name',''),nullif(l.adapter_metadata->>'nickname',''),p.preferred_name,p.display_name,w.label,'已关联账号') AS label,jsonb_build_object('description','已验证渠道账号','subject_kind',a.subject_kind,'subject_ref',a.subject_ref,'subject_id',a.subject_ref,'channel_source_link_id',l.id,'platform',CASE WHEN l.subject_type='qiwe_sender' THEN 'qiwe' WHEN l.subject_type IN ('wecom_internal','wecom_external') THEN 'wecom' ELSE l.subject_type END,'gateway_label',s.label||' · '||CASE WHEN g.subject_type='qiwe_sender' THEN '微信' ELSE '企业微信' END) AS extra FROM qintopia_identity.source_identity_links l CROSS JOIN args a JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=a.tenant AND g.namespace=l.namespace AND g.subject_type=l.subject_type JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key LEFT JOIN qintopia_identity.persons p ON p.id=l.person_id LEFT JOIN qintopia_identity.work_accounts w ON w.tenant_key=a.tenant AND w.source_link_id=l.id WHERE g.gateway_key=ANY(a.gateways) AND g.active AND s.status='active' AND ((a.subject_kind='person' AND l.person_id=a.subject_ref AND p.status='active' AND g.account_kind<>'shared' AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.id=ANY(a.channel_refs)) OR (a.subject_kind='work_account' AND w.id=a.subject_ref AND w.active AND g.account_kind='shared' AND w.gateway_version=g.version AND w.source_version=l.version AND l.person_id IS NULL AND l.status<>'revoked' AND l.adapter_metadata ? 'first_observation_ref'))"#
            }
            "groups" => {
                r#"SELECT c.id AS ref,c.display_name AS label,jsonb_build_object('description','已登记群','platform',c.platform,'gateway_label',CASE WHEN c.platform='qiwe' THEN '微信' WHEN c.platform='wecom' THEN '企业微信' ELSE c.platform END) AS extra FROM qintopia_messages.conversations c CROSS JOIN args a WHERE c.tenant_id=a.tenant AND c.chat_type='group' AND c.status='active' AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_knowledge_items k WHERE k.tenant_key=a.tenant AND k.space_id=c.id) AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger x WHERE x.tenant_key=a.tenant AND x.kind='group' AND x.object_ref=c.id::text AND x.status<>'active')"#
            }
            _ => unreachable!(),
        };
        // Recheck at the row-producing statement too: another tenant may change
        // its source ownership after the earlier gateway selection.
        let source_ownership = if matches!(q.kind.as_str(), "accounts" | "channels") {
            format!(" AND ({CONTACT_GATEWAY_OWNERSHIP_SQL})")
        } else {
            String::new()
        };
        let sql = format!("WITH args AS (SELECT $1::text tenant,$2::uuid scope,$3::text search,$4::text last_label,$5::uuid last_ref,$6::bigint page_limit,$7::text namespace,$8::uuid[] eligible,$9::uuid subject_ref,$10::text subject_kind,$11::text[] gateways,$12::uuid[] channel_refs), directory AS ({directory}{source_ownership}), unique_directory AS (SELECT DISTINCT ON (ref) ref,label,extra FROM directory ORDER BY ref,label) SELECT d.ref,d.label,d.extra,lower(d.label) AS sort_label FROM unique_directory d CROSS JOIN args a WHERE strpos(lower(d.label),lower(a.search))>0 AND (a.last_label IS NULL OR (lower(d.label) COLLATE \"C\",d.ref)>(a.last_label COLLATE \"C\",a.last_ref)) ORDER BY lower(d.label) COLLATE \"C\",d.ref LIMIT $6");
        let rows = sqlx::query(&sql)
            .bind(&self.tenant)
            .bind(q.scope)
            .bind(&q.search)
            .bind(cursor.as_ref().map(|c| &c.label))
            .bind(cursor.as_ref().map(|c| c.reference))
            .bind(limit + 1)
            .bind(&self.identity_namespace)
            .bind(eligible)
            .bind(q.subject_ref)
            .bind(&q.subject_kind)
            .bind(gateways)
            .bind(channel_refs)
            .fetch_all(&mut *tx)
            .await
            .map_err(|_| anyhow::anyhow!("candidate_source_unavailable"))?;
        let items: Vec<Value> = rows
            .iter()
            .take(limit as usize)
            .map(|r| {
                let mut value = r.get::<Value, _>("extra");
                value["ref"] = json!(r.get::<Uuid, _>("ref"));
                value["label"] = json!(r.get::<String, _>("label"));
                value
            })
            .collect();
        let next = if rows.len() > limit as usize {
            let last = &rows[limit as usize - 1];
            Some(encode_cursor(&Cursor {
                query: fingerprint,
                label: last.get("sort_label"),
                reference: last.get("ref"),
            })?)
        } else {
            None
        };
        tx.commit().await?;
        Ok(
            json!({"scope":q.scope,"kind":q.kind,"purpose":q.purpose,"configuration_version":version,"items":items,"next_cursor":next}),
        )
    }
}

pub(crate) fn http_error(error: &anyhow::Error) -> (u16, &'static str) {
    match error.to_string().as_str() {
        "invalid_candidate_query" => (400, "invalid_candidate_query"),
        "authentication_required" => (401, "authentication_required"),
        "scope_access_denied"
        | "gateway_scope_mismatch"
        | "management_denied"
        | "verified_identity_required"
        | "identity_changed_or_revoked"
        | "gateway_changed_or_revoked" => (403, "scope_access_denied"),
        _ => (503, "candidate_source_unavailable"),
    }
}
