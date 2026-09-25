//! Trusted Gateway identities converge on the existing stable Person.
use super::{Actor, Store};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use sqlx::{postgres::PgRow, Postgres, Row, Transaction};
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

/// Browser-selected, already observed identity. Evidence and namespaces are never
/// accepted from the browser; both are read back under the tenant transaction.
#[derive(Clone, serde::Serialize, serde::Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct IdentityUiCommand {
    pub operation_id: Uuid,
    pub link_ref: Uuid,
    pub person_ref: Uuid,
    pub expected_version: i64,
    pub expected_configuration_version: i64,
    pub expected_gateway_version: i64,
    pub revoke: bool,
}

fn observed_evidence(row: &PgRow) -> Option<Uuid> {
    let metadata: Value = row.get("adapter_metadata");
    let link: Uuid = row.get("id");
    metadata["first_observation_ref"]
        .as_str()
        .and_then(|v| Uuid::parse_str(v).ok())
        .or_else(|| {
            metadata["account_conversion_evidence"]
                .as_array()?
                .iter()
                .filter(|v| {
                    v["adapter"] == "qiwe/contact/openid"
                        && v["link_refs"]
                            .as_array()
                            .is_some_and(|ids| ids.contains(&json!(link)))
                })
                .find_map(|v| {
                    v["evidence_ref"]
                        .as_str()
                        .and_then(|v| Uuid::parse_str(v).ok())
                })
        })
        .or_else(|| {
            // Previously confirmed identities retain their original trusted proof.
            (row.get::<String, _>("status") != "pending"
                && row.get::<Option<Uuid>, _>("confirmed_by").is_some())
            .then(|| row.get::<Option<Uuid>, _>("evidence_ref"))
            .flatten()
        })
}

fn source_label(row: &PgRow) -> String {
    let kind = row.get::<String, _>("subject_type");
    let source = match kind.as_str() {
        "qiwe_sender" => "企微消息账号",
        "wecom_external" => "企业微信外部联系人",
        "wecom_internal" => "企业微信员工账号",
        "pms_member" => "PMS 成员",
        "pms_occupant" => "PMS 入住人",
        "feishu_open" => "飞书账号",
        _ => "已登记来源",
    };
    format!("{} · {}", source, row.get::<String, _>("scope_label"))
}

/// Exact documented positive response pair; only an authenticated adapter builds it.
pub struct QiweConversion {
    pub external_gateway: String,
    pub user_id: String,
    pub open_user_id: String,
    pub evidence_ref: Uuid,
}

impl Store {
    async fn identity_ui_access(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        now: chrono::DateTime<chrono::Utc>,
    ) -> Result<Option<(Vec<Uuid>, bool)>> {
        ensure!(
            actor.session_hash.is_some() && actor.gateway.is_none(),
            "authentication_required"
        );
        self.verify(tx, actor).await?;
        let policy = self.policy(tx, now).await?;
        let root_access = policy
            .scopes
            .iter()
            .filter(|s| s.parent.is_none())
            .any(|s| {
                policy
                    .manager(actor.person, s.id, "default", "organization", "identity")
                    .is_some()
            });
        if !root_access {
            return Ok(None);
        }
        let scopes: Vec<Uuid> = policy
            .scopes
            .iter()
            .filter(|s| {
                policy
                    .manager(actor.person, s.id, "default", "organization", "identity")
                    .is_some()
            })
            .map(|s| s.id)
            .collect();
        // An unscoped person belongs to the tenant-wide directory; a manager of
        // one root, or of a root without descendants, cannot inspect that directory.
        let tenant_wide = policy
            .scopes
            .iter()
            .filter(|s| s.active)
            .all(|s| scopes.contains(&s.id));
        Ok(Some((scopes, tenant_wide)))
    }

    async fn identity_ui_people(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scopes: &[Uuid],
        tenant_wide: bool,
    ) -> Result<Value> {
        Ok(sqlx::query_scalar("SELECT coalesce(jsonb_agg(x ORDER BY x->>'label',x->>'id'),'[]') FROM (SELECT jsonb_build_object('id',p.id,'label',coalesce(nullif(e.nickname,''),e.label,p.preferred_name,p.display_name),'description',coalesce(e.description,''),'status',coalesce(e.status,'active'),'verified',EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l WHERE l.namespace=$1 AND l.person_id=p.id AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL)) x FROM qintopia_identity.persons p LEFT JOIN qintopia_agent_os.collaboration_ledger e ON e.tenant_key=$1 AND e.kind='person' AND e.object_ref=p.id::text WHERE p.status='active' AND (e.status IN('draft','active') OR (e.id IS NULL AND EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l WHERE l.namespace=$1 AND l.person_id=p.id AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL))) AND ($3 OR e.scope_id=ANY($2) OR EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type WHERE g.tenant_key=$1 AND g.scope_id=ANY($2) AND g.active AND g.account_kind<>'shared' AND l.person_id=p.id AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL))) entries")
            .bind(&self.tenant).bind(scopes).bind(tenant_wide).fetch_one(&mut **tx).await?)
    }

    async fn identity_ui_rows(&self, tx: &mut Transaction<'_, Postgres>) -> Result<Vec<PgRow>> {
        // Ambiguous source registries are visible as unavailable, never guessed.
        Ok(sqlx::query("SELECT l.*,g.gateway_key,g.version AS gateway_version,g.scope_id,g.account_kind,g.active AS gateway_active,s.label AS scope_label,s.status AS scope_status,coalesce(p.preferred_name,p.display_name) AS person_label,(SELECT count(*) FROM qintopia_identity.person_identity_gateways x WHERE x.namespace=g.namespace AND x.subject_type=g.subject_type) AS namespace_owners,EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_tenants t WHERE t.identity_namespace=g.namespace) AS reserved_namespace FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key LEFT JOIN qintopia_identity.persons p ON p.id=l.person_id WHERE g.tenant_key=$1 ORDER BY s.label,g.gateway_key,l.source_ref,l.id")
            .bind(&self.tenant).fetch_all(&mut **tx).await?)
    }

    pub(crate) async fn identities(&self, actor: &Actor, person: Option<Uuid>) -> Result<Value> {
        let (mut tx, version, now) = self.begin().await?;
        let Some((scopes, tenant_wide)) = self.identity_ui_access(&mut tx, actor, now).await?
        else {
            return Ok(
                json!({"version":version,"can_manage":false,"links":[],"candidates":[],"people":[]}),
            );
        };
        let people = self
            .identity_ui_people(&mut tx, &scopes, tenant_wide)
            .await?;
        let mut links = Vec::new();
        let mut candidates = Vec::new();
        for row in self.identity_ui_rows(&mut tx).await? {
            if !scopes.contains(&row.get::<Uuid, _>("scope_id")) {
                continue;
            }
            let evidence = observed_evidence(&row);
            let reason = if row.get::<String, _>("account_kind") == "shared" {
                Some("共享账号不能直接确认成自然人，请使用可核验的个人账号。")
            } else if row.get::<i64, _>("namespace_owners") != 1
                || row.get::<bool, _>("reserved_namespace")
            {
                Some("来源登记存在冲突，需先由技术负责人核对。")
            } else if !row.get::<bool, _>("gateway_active")
                || row.get::<String, _>("scope_status") != "active"
            {
                Some("该来源或所属范围已停用。")
            } else if evidence.is_none() {
                Some("尚无可信来源记录，暂不能核验。")
            } else {
                None
            };
            let linked: Option<Uuid> = row.get("person_id");
            let status: String = row.get("status");
            let entry = json!({"id":row.get::<Uuid,_>("id"),"version":row.get::<i64,_>("version"),"status":status,"source_label":source_label(&row),"account_label":row.get::<String,_>("source_ref"),"account_kind":row.get::<String,_>("account_kind"),"person_ref":linked,"person_label":row.get::<Option<String>,_>("person_label"),"scope_ref":row.get::<Uuid,_>("scope_id"),"scope_label":row.get::<String,_>("scope_label"),"evidence_ref":evidence,"gateway_key":row.get::<String,_>("gateway_key"),"gateway_version":row.get::<i64,_>("gateway_version"),"subject_type":row.get::<String,_>("subject_type"),"selectable":reason.is_none(),"unavailable_reason":reason});
            if linked.is_some() && person.is_none_or(|p| Some(p) == linked) {
                links.push(entry.clone());
            }
            if status != "confirmed" {
                candidates.push(entry);
            }
        }
        Ok(
            json!({"version":version,"can_manage":true,"links":links,"candidates":candidates,"people":people}),
        )
    }

    pub(crate) async fn preview_identity(
        &self,
        actor: &Actor,
        command: &IdentityUiCommand,
    ) -> Result<Value> {
        self.identity_ui_command(actor, command, false).await
    }

    pub(crate) async fn save_identity(
        &self,
        actor: &Actor,
        command: &IdentityUiCommand,
    ) -> Result<Value> {
        self.identity_ui_command(actor, command, true).await
    }

    async fn identity_ui_command(
        &self,
        actor: &Actor,
        command: &IdentityUiCommand,
        save: bool,
    ) -> Result<Value> {
        let (mut tx, version, now) = self.begin().await?;
        let (scopes, tenant_wide) = self
            .identity_ui_access(&mut tx, actor, now)
            .await?
            .ok_or_else(|| anyhow::anyhow!("identity_management_denied"))?;
        // Older source consumers do not all take the collaboration tenant lock.
        // Freeze registry membership (including ambiguous namespace insertions),
        // then lock this exact link and Gateway until validation and write finish.
        sqlx::query("LOCK TABLE qintopia_identity.person_identity_gateways IN SHARE MODE")
            .execute(&mut *tx)
            .await?;
        let locked=sqlx::query("SELECT l.id FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key WHERE g.tenant_key=$1 AND l.id=$2 FOR UPDATE OF l FOR SHARE OF g,s")
            .bind(&self.tenant).bind(command.link_ref).fetch_all(&mut *tx).await?;
        ensure!(!locked.is_empty(), "identity_scope_unbound");
        let rows = self.identity_ui_rows(&mut tx).await?;
        let row = rows
            .iter()
            .find(|r| r.get::<Uuid, _>("id") == command.link_ref)
            .ok_or_else(|| anyhow::anyhow!("identity_scope_unbound"))?;
        ensure!(
            scopes.contains(&row.get::<Uuid, _>("scope_id")),
            "identity_management_denied"
        );
        let hash = super::super::digest(&serde_json::to_vec(&json!({"identity_ui":command}))?);
        if let Some(previous)=sqlx::query("SELECT actor_identity_id,request_hash,result FROM qintopia_agent_os.collaboration_commands WHERE id=$1 AND tenant_key=$2")
            .bind(command.operation_id).bind(&self.tenant).fetch_optional(&mut *tx).await? {
            ensure!(previous.get::<Uuid,_>("actor_identity_id")==actor.link && previous.get::<String,_>("request_hash")==hash,"identity_operation_conflict");
            return Ok(json!({"replayed":true,"historical_receipt":previous.get::<Value,_>("result"),"current_state_requires_read":true}));
        }
        ensure!(
            version == command.expected_configuration_version,
            "configuration_version_conflict"
        );
        ensure!(
            row.get::<i64, _>("namespace_owners") == 1 && !row.get::<bool, _>("reserved_namespace"),
            "identity_namespace_conflict"
        );
        ensure!(
            row.get::<bool, _>("gateway_active")
                && row.get::<String, _>("scope_status") == "active",
            "gateway_not_active"
        );
        ensure!(
            row.get::<String, _>("account_kind") != "shared",
            "shared_account_person_unknown"
        );
        ensure!(
            row.get::<i64, _>("gateway_version") == command.expected_gateway_version,
            "identity_gateway_version_conflict"
        );
        ensure!(
            row.get::<i64, _>("version") == command.expected_version,
            "identity_version_conflict"
        );
        let evidence = observed_evidence(row)
            .ok_or_else(|| anyhow::anyhow!("identity_observation_required"))?;
        let current: Option<Uuid> = row.get("person_id");
        let status: String = row.get("status");
        ensure!(
            !command.revoke || (status == "confirmed" && current == Some(command.person_ref)),
            "revoke_person_mismatch"
        );
        ensure!(
            command.revoke || status != "confirmed",
            "revoke_conflicting_link_first"
        );
        // A new ledger person is eligible for explicit review of an existing observed
        // account. Names never select or merge people, and retired people stay retired.
        let person=sqlx::query("SELECT coalesce(nullif(e.nickname,''),e.label,p.preferred_name,p.display_name) AS label,e.status AS ledger_status FROM qintopia_identity.persons p LEFT JOIN qintopia_agent_os.collaboration_ledger e ON e.tenant_key=$2 AND e.kind='person' AND e.object_ref=p.id::text WHERE p.id=$1 AND p.status='active' AND (e.status IN('draft','active') OR (e.id IS NULL AND EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l WHERE l.namespace=$2 AND l.person_id=p.id AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL)))")
            .bind(command.person_ref).bind(&self.tenant).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("person_outside_tenant"))?;
        let people = self
            .identity_ui_people(&mut tx, &scopes, tenant_wide)
            .await?;
        ensure!(
            people
                .as_array()
                .is_some_and(|people| people.iter().any(|p| p["id"] == json!(command.person_ref))),
            "identity_management_denied"
        );
        let anchor_ref = format!("reviewed-link:{}", command.link_ref);
        let other_verification:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed' AND evidence_ref IS NOT NULL AND confirmed_by IS NOT NULL AND NOT(subject_type=$3 AND source_ref=$4))")
            .bind(&self.tenant).bind(command.person_ref).bind(row.get::<String,_>("subject_type")).bind(&anchor_ref).fetch_one(&mut *tx).await?;
        let removes_last = command.revoke && !other_verification;
        let (appointments, grants) = if removes_last {
            Self::identity_authority_impact(&mut tx, &self.tenant, command.person_ref).await?
        } else {
            (0, Vec::new())
        };
        let sessions: i64 = if command.revoke {
            sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_sessions s JOIN qintopia_agent_os.collaboration_accounts a ON a.id=s.account_id AND a.tenant_key=s.tenant_key WHERE s.tenant_key=$1 AND a.person_id=$2 AND s.revoked_at IS NULL AND s.expires_at>clock_timestamp()")
            .bind(&self.tenant).bind(command.person_ref).fetch_one(&mut *tx).await?
        } else {
            0
        };
        let activates = !command.revoke
            && person.get::<Option<String>, _>("ledger_status").as_deref() == Some("draft");
        let description = if removes_last {
            "撤销此账号关联及其人员核验依据，结束当前登录会话、原任职和由其派生的授权。再次核验不会恢复旧权限，需另行配置任职。"
        } else if command.revoke {
            "撤销此账号关联并结束该人员当前登录会话。该人员还有独立核验依据，其他有效任职保留；旧会话和旧身份版本不会恢复。"
        } else if activates {
            "确认此来源账号属于所选人员，将其台账由待核验改为已核验。之后可单独开设登录账号和配置任职；本次不授予业务权限。"
        } else {
            "确认此来源账号属于所选人员，保留来源核验记录；本次不新增任职或业务权限。"
        };
        let mut result = json!({"kind":"identity_change","person_ref":command.person_ref,"person_label":person.get::<String,_>("label"),"source_label":source_label(row),"account_label":row.get::<String,_>("source_ref"),"link_ref":command.link_ref,"before":{"status":status,"person_ref":current,"version":command.expected_version},"after":{"status":if command.revoke {"revoked"}else{"confirmed"},"person_ref":command.person_ref,"version":command.expected_version+1},"impact":{"description":description,"revokes_sessions":sessions,"removes_last_verification":removes_last,"ends_appointments":appointments,"revokes_grants":grants.len(),"activates_person":activates,"authority_granted":false},"configuration_version":version,"saved":false});
        if !save {
            return Ok(result);
        }
        crate::resident_welcome::store::invalidate_link(&mut tx, command.link_ref).await?;
        let changed=sqlx::query("UPDATE qintopia_identity.source_identity_links SET person_id=$2,status=$3,version=version+1,evidence_ref=$4,confirmed_by=$5,updated_at=clock_timestamp(),adapter_metadata=jsonb_set(adapter_metadata,'{identity_history}',coalesce(adapter_metadata->'identity_history','[]'::jsonb)||jsonb_build_array(jsonb_build_object('person_ref',person_id,'status',status,'version',version,'evidence_ref',evidence_ref,'confirmed_by',confirmed_by,'ended_at',clock_timestamp()))) WHERE id=$1 AND version=$6")
            .bind(command.link_ref).bind(command.person_ref).bind(if command.revoke {"revoked"}else{"confirmed"}).bind(evidence).bind(actor.person).bind(command.expected_version).execute(&mut *tx).await?.rows_affected();
        ensure!(changed == 1, "identity_version_conflict");
        if command.revoke {
            Self::revoke_reviewed_identity(
                &mut tx,
                &self.tenant,
                command.link_ref,
                command.person_ref,
            )
            .await?;
        } else {
            // This is a projection of a reviewed source identity in this tenant,
            // never a fabricated external account or a name-based Person match.
            let metadata = json!({"identity_review":{"source_link_ref":command.link_ref,"gateway_key":row.get::<String,_>("gateway_key"),"gateway_version":command.expected_gateway_version,"namespace":row.get::<String,_>("namespace"),"evidence_ref":evidence}});
            sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by,adapter_metadata) VALUES($1,$2,$3,$4,'confirmed',$5,$6,$7) ON CONFLICT(namespace,subject_type,source_ref) DO UPDATE SET person_id=EXCLUDED.person_id,status='confirmed',version=source_identity_links.version+1,evidence_ref=EXCLUDED.evidence_ref,confirmed_by=EXCLUDED.confirmed_by,adapter_metadata=EXCLUDED.adapter_metadata,updated_at=clock_timestamp()")
                .bind(&self.tenant).bind(row.get::<String,_>("subject_type")).bind(anchor_ref).bind(command.person_ref).bind(evidence).bind(actor.person).bind(metadata).execute(&mut *tx).await?;
            sqlx::query("UPDATE qintopia_agent_os.collaboration_ledger SET verified=true,status='active',version=version+1 WHERE tenant_key=$1 AND kind='person' AND object_ref=$2 AND status IN('draft','active')")
                .bind(&self.tenant).bind(command.person_ref.to_string()).execute(&mut *tx).await?;
        }
        result["saved"] = json!(true);
        result["configuration_version"] = json!(version + 1);
        result["actor_ref"] = json!(actor.person);
        result["recorded_at"] = json!(now);
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_commands(id,tenant_key,actor_identity_id,actor_person_id,request_hash,expected_version,result) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(command.operation_id).bind(&self.tenant).bind(actor.link).bind(actor.person).bind(hash).bind(version).bind(&result).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.collaboration_tenants SET version=version+1 WHERE tenant_key=$1").bind(&self.tenant).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }

    /// Acquire tenant locks before the source row, including tenants whose old
    /// projection outlived a Gateway move. All identity entry points share this
    /// order, so a legacy welcome command cannot race a new review into stale trust.
    pub(crate) async fn lock_reviewed_source_identity(
        tx: &mut Transaction<'_, Postgres>,
        link: Uuid,
    ) -> Result<()> {
        sqlx::query("LOCK TABLE qintopia_identity.person_identity_gateways IN SHARE MODE")
            .execute(&mut **tx)
            .await?;
        sqlx::query("SELECT t.tenant_key FROM qintopia_agent_os.collaboration_tenants t WHERE EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type WHERE l.id=$1 AND g.tenant_key=t.tenant_key) OR EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links a WHERE a.namespace=t.identity_namespace AND a.adapter_metadata->'identity_review'->>'source_link_ref'=$2) ORDER BY t.tenant_key FOR UPDATE OF t")
            .bind(link).bind(link.to_string()).fetch_all(&mut **tx).await?;
        Ok(())
    }

    /// Legacy source mutations invalidate every derived tenant verification in
    /// the same transaction; a rebind never leaves the old Person's projection.
    pub(crate) async fn invalidate_reviewed_source_identity(
        tx: &mut Transaction<'_, Postgres>,
        link: Uuid,
    ) -> Result<()> {
        let owners=sqlx::query("SELECT DISTINCT t.tenant_key,a.person_id FROM qintopia_identity.source_identity_links a JOIN qintopia_agent_os.collaboration_tenants t ON t.identity_namespace=a.namespace WHERE a.status='confirmed' AND a.adapter_metadata->'identity_review'->>'source_link_ref'=$1 ORDER BY t.tenant_key,a.person_id")
            .bind(link.to_string()).fetch_all(&mut **tx).await?;
        for owner in owners {
            let tenant: String = owner.get("tenant_key");
            Self::revoke_reviewed_identity(tx, &tenant, link, owner.get("person_id")).await?;
            sqlx::query("UPDATE qintopia_agent_os.collaboration_tenants SET version=version+1 WHERE tenant_key=$1")
                .bind(tenant).execute(&mut **tx).await?;
        }
        Ok(())
    }

    async fn identity_authority_impact(
        tx: &mut Transaction<'_, Postgres>,
        tenant: &str,
        person: Uuid,
    ) -> Result<(i64, Vec<Uuid>)> {
        let appointments:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_appointments WHERE tenant_key=$1 AND person_id=$2 AND status='active'")
            .bind(tenant).bind(person).fetch_one(&mut **tx).await?;
        let grants:Vec<Uuid>=sqlx::query_scalar("WITH RECURSIVE affected(id) AS (SELECT g.id FROM qintopia_agent_os.collaboration_grants g JOIN qintopia_agent_os.agent_collaborations c ON c.id=g.collaboration_id AND c.tenant_key=g.tenant_key JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id AND a.tenant_key=c.tenant_key WHERE g.tenant_key=$1 AND a.person_id=$2 UNION SELECT g.id FROM qintopia_agent_os.collaboration_grants g JOIN affected p ON g.parent_grant_id=p.id WHERE g.tenant_key=$1) SELECT g.id FROM qintopia_agent_os.collaboration_grants g JOIN affected x ON x.id=g.id WHERE g.status='active' ORDER BY g.id")
            .bind(tenant).bind(person).fetch_all(&mut **tx).await?;
        Ok((appointments, grants))
    }

    async fn revoke_reviewed_identity(
        tx: &mut Transaction<'_, Postgres>,
        tenant: &str,
        link: Uuid,
        person: Uuid,
    ) -> Result<()> {
        let anchors:Vec<Uuid>=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed' AND adapter_metadata->'identity_review'->>'source_link_ref'=$3")
            .bind(tenant).bind(person).bind(link.to_string()).fetch_all(&mut **tx).await?;
        for anchor in &anchors {
            crate::resident_welcome::store::invalidate_link(tx, *anchor).await?;
        }
        sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked',version=version+1,updated_at=clock_timestamp() WHERE id=ANY($1)")
            .bind(&anchors).execute(&mut **tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.collaboration_sessions SET revoked_at=clock_timestamp() WHERE tenant_key=$1 AND account_id IN(SELECT id FROM qintopia_agent_os.collaboration_accounts WHERE tenant_key=$1 AND person_id=$2) AND revoked_at IS NULL")
            .bind(tenant).bind(person).execute(&mut **tx).await?;
        let verified:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed' AND evidence_ref IS NOT NULL AND confirmed_by IS NOT NULL)")
            .bind(tenant).bind(person).fetch_one(&mut **tx).await?;
        if !verified {
            let (_, grants) = Self::identity_authority_impact(tx, tenant, person).await?;
            sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked',version=version+1,revoked_at=clock_timestamp() WHERE tenant_key=$1 AND id=ANY($2) AND status='active'")
                .bind(tenant).bind(grants).execute(&mut **tx).await?;
            sqlx::query("UPDATE qintopia_agent_os.agent_collaborations SET status='revoked',version=version+1 WHERE tenant_key=$1 AND appointment_id IN(SELECT id FROM qintopia_agent_os.collaboration_appointments WHERE tenant_key=$1 AND person_id=$2) AND status='active'")
                .bind(tenant).bind(person).execute(&mut **tx).await?;
            sqlx::query("UPDATE qintopia_agent_os.collaboration_appointments SET status='revoked',version=version+1,ended_at=clock_timestamp() WHERE tenant_key=$1 AND person_id=$2 AND status='active'")
                .bind(tenant).bind(person).execute(&mut **tx).await?;
            sqlx::query("UPDATE qintopia_agent_os.collaboration_ledger SET verified=false,status='draft',version=version+1 WHERE tenant_key=$1 AND kind='person' AND object_ref=$2 AND status='active'")
                .bind(tenant).bind(person.to_string()).execute(&mut **tx).await?;
        }
        Ok(())
    }

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
        if command.revoke {
            Self::revoke_reviewed_identity(
                &mut tx,
                &self.tenant,
                command.link_ref,
                command.person_ref,
            )
            .await?;
        }
        let result = json!({"link_ref":command.link_ref,"version":command.expected_version+1,"status":if command.revoke {"revoked"}else{"confirmed"}});
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_commands(id,tenant_key,actor_identity_id,actor_person_id,request_hash,expected_version,result) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(command.operation_id).bind(&self.tenant).bind(actor.link).bind(actor.person).bind(hash).bind(configuration_version).bind(&result).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.collaboration_tenants SET version=version+1 WHERE tenant_key=$1").bind(&self.tenant).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }
    /// A personal channel can be confirmed by an explicitly authorized work account.
    /// This validates the recorded effect, not merely the presence of an account UUID.
    pub(super) async fn work_account_person_proof(
        &self,
        tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
        link: Uuid,
        person: Uuid,
    ) -> Result<bool> {
        Ok(sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.work_accounts w ON w.id=l.confirmed_by_work_account AND w.tenant_key=$1 JOIN qintopia_agent_os.welcome_review_receipts r ON r.id=l.evidence_ref AND r.tenant_key=w.tenant_key AND r.subject_kind='work_account' AND r.subject_id=w.id JOIN qintopia_agent_os.welcome_review_items i ON i.work_item_id=r.work_item_id AND i.tenant_key=r.tenant_key JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=w.tenant_key AND g.scope_id=i.scope_id AND g.namespace=l.namespace AND g.subject_type=l.subject_type WHERE l.id=$2 AND l.person_id=$3 AND l.status='confirmed' AND g.active AND g.account_kind<>'shared' AND r.effects->>'decision'='confirm' AND r.effects->>'channel_person'='true' AND r.effects->>'person'=$3::text AND r.effects->>'channel'=$2::text AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_review_receipts revoked WHERE revoked.work_item_id=r.work_item_id AND revoked.effects->>'decision'='revoke' AND revoked.created_at>r.created_at))")
            .bind(&self.tenant).bind(link).bind(person).fetch_one(&mut **tx).await?)
    }

    /// `gateway` is deployment/session context, never a model supplied namespace.
    pub(crate) async fn gateway_actor(&self, gateway: &str, subject_ref: &str) -> Result<Actor> {
        self.resolve_gateway_actor(gateway, subject_ref, false)
            .await
    }

    pub(crate) async fn conversation_actor(
        &self,
        gateway: &str,
        subject_ref: &str,
    ) -> Result<Actor> {
        self.resolve_gateway_actor(gateway, subject_ref, true).await
    }

    async fn resolve_gateway_actor(
        &self,
        gateway: &str,
        subject_ref: &str,
        work_proof: bool,
    ) -> Result<Actor> {
        let (mut tx, _, _) = self.begin().await?;
        let row=sqlx::query("SELECT g.namespace,g.version AS gateway_version,g.scope_id,g.account_kind,l.id,l.person_id,l.version FROM qintopia_identity.person_identity_gateways g JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key JOIN qintopia_identity.source_identity_links l ON l.namespace=g.namespace AND l.subject_type=g.subject_type JOIN qintopia_identity.persons p ON p.id=l.person_id WHERE g.tenant_key=$1 AND g.gateway_key=$2 AND g.active AND s.status='active' AND l.source_ref=$3 AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND (l.confirmed_by IS NOT NULL OR $4) AND p.status='active' FOR SHARE OF g,s,l,p")
            .bind(&self.tenant).bind(gateway).bind(subject_ref).bind(work_proof).fetch_optional(&mut *tx).await?
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
