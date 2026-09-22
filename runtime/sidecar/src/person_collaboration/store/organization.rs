//! Organizational definitions never substitute for effective authorization chains.
use super::*;

impl Store {
    pub(super) async fn work_snapshot(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Option<Uuid>,
    ) -> Result<Value> {
        let Some(id) = id else { return Ok(Value::Null) };
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('connection',to_jsonb(c)-'tenant_key','appointment',to_jsonb(a)-'tenant_key','audience',(SELECT configuration FROM qintopia_agent_os.collaboration_audiences WHERE collaboration_id=c.id),'grants',(SELECT coalesce(jsonb_agg(to_jsonb(g)-'tenant_key' ORDER BY g.id),'[]') FROM qintopia_agent_os.collaboration_grants g WHERE g.collaboration_id=c.id)) FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND c.id=$2")
            .bind(&self.tenant).bind(id).fetch_optional(&mut **tx).await?.unwrap_or(Value::Null))
    }
    pub(super) async fn configuration_snapshot(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        change: &Change,
        created: Option<Uuid>,
    ) -> Result<Option<Value>> {
        let (table, key, id) = match change {
            Change::SaveLedger { id, .. } => ("collaboration_ledger", "id", id.or(created)),
            Change::SavePosition { id, .. } => ("collaboration_positions", "id", id.or(created)),
            Change::SetAudience { collaboration, .. } => (
                "collaboration_audiences",
                "collaboration_id",
                Some(*collaboration),
            ),
            Change::Lifecycle { object, id, .. } if object == "position" => {
                ("collaboration_positions", "id", Some(*id))
            }
            Change::Lifecycle { object, id, .. } if object == "ledger" => {
                ("collaboration_ledger", "id", Some(*id))
            }
            _ => return Ok(None),
        };
        let Some(id) = id else { return Ok(None) };
        Ok(sqlx::query_scalar(&format!("SELECT to_jsonb(x)-'tenant_key' FROM qintopia_agent_os.{table} x WHERE tenant_key=$1 AND {key}=$2"))
            .bind(&self.tenant).bind(id).fetch_optional(&mut **tx).await?)
    }
    /// Only the explicit local --init-fixture path calls this. Never a runtime/default migration seed.
    pub async fn organization_fixture(&self, actor: &Actor) -> Result<()> {
        let state = self.state(actor).await?;
        let find = |collection: &str, name: &str| -> Uuid {
            serde_json::from_value(
                state[collection]
                    .as_array()
                    .unwrap()
                    .iter()
                    .find(|x| x["label"] == name)
                    .unwrap()["id"]
                    .clone(),
            )
            .unwrap()
        };
        let root = find("scopes", "秦托邦");
        let mut parent = None;
        for (name, role, scope) in [
            ("公司负责人", "公司负责人", "秦托邦"),
            ("社区负责人", "社区负责人", "秦托邦"),
            ("一栋舍长", "舍长", "一栋"),
            ("二栋舍长", "舍长", "二栋"),
            ("三栋舍长", "舍长", "三栋"),
            ("小管家", "小管家", "秦托邦"),
            ("技术负责人", "技术负责人", "秦托邦"),
        ] {
            let result = self
                .fixture_change(
                    actor,
                    Change::SavePosition {
                        id: None,
                        role: find("roles", role),
                        scope: find("scopes", scope),
                        parent: if name == "公司负责人" || name == "技术负责人" {
                            None
                        } else {
                            parent
                        },
                        label: name.into(),
                        description: if name == "技术负责人" {
                            "横向技术支撑；不替代业务责任人裁定。"
                        } else {
                            "本地合成组织岗位，实际权限由任职连接单独配置。"
                        }
                        .into(),
                        draft: false,
                    },
                )
                .await?;
            if name == "公司负责人" || name == "社区负责人" {
                parent = Some(serde_json::from_value(result["change"]["id"].clone())?);
            }
        }
        let mut manager = Assignment {
            collaboration: None,
            person: find("people", "人员甲 · 合成样例 A"),
            role: find("roles", "社区负责人"),
            duty: Some(find("duties", "组织管理")),
            scope: root,
            agent: "silaoshi".into(),
            domain: "organization".into(),
            responsibility: "管理社区服务领域的人员安排；管理权不等于亲自执行全部业务。".into(),
            valid_until: None,
            proxy_for: None,
            actions: vec![],
            permissions: vec![PermissionSetting {
                action: "manage".into(),
                mode: PermissionMode::Autonomous,
                reviewer: None,
            }],
            delegation: Some(Delegation {
                agents: agents().iter().map(|a| a.to_string()).collect(),
                domains: vec!["community_service".into()],
                actions: vec![
                    "train".into(),
                    "confirm_knowledge".into(),
                    "review".into(),
                    "publish".into(),
                    "change_rules".into(),
                ],
                depth: 1,
            }),
        };
        self.fixture_change(actor, Change::Assign(Box::new(manager.clone())))
            .await?;
        let link:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed'")
            .bind(&self.tenant).bind(manager.person).fetch_one(&self.pool).await?;
        let supervisor = self.actor(link).await?;
        for (name, scope) in [("人员乙 · 合成样例", "一栋"), ("人员丙 · 合成样例", "二栋")]
        {
            manager.person = find("people", name);
            manager.role = find("roles", "舍长");
            manager.scope = find("scopes", scope);
            manager.duty = Some(find("duties", "居民服务"));
            manager.agent = "erhua".into();
            manager.domain = "community_service".into();
            manager.responsibility = format!(
                "在秦托邦文化与社区原则内服务{scope}居民；工作节点由二花在合作启动时主动引导确认。"
            );
            manager.delegation = None;
            manager.permissions = [
                "train",
                "confirm_knowledge",
                "review",
                "publish",
                "change_rules",
            ]
            .iter()
            .map(|a| PermissionSetting {
                action: a.to_string(),
                mode: PermissionMode::Autonomous,
                reviewer: None,
            })
            .collect();
            self.fixture_change(&supervisor, Change::Assign(Box::new(manager.clone())))
                .await?;
        }
        Ok(())
    }

    async fn fixture_change(&self, actor: &Actor, change: Change) -> Result<Value> {
        let version = self.state(actor).await?["version"].as_i64().unwrap();
        self.command(
            actor,
            &Command {
                operation_id: Uuid::new_v4(),
                expected_version: version,
                change,
            },
            true,
        )
        .await
    }
    pub(super) async fn check_organization_assignment(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        a: &Assignment,
    ) -> Result<()> {
        let blocked:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger WHERE tenant_key=$1 AND status<>'active' AND ((kind='person' AND object_ref=$2) OR (kind='agent' AND object_ref=$3))) OR EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_positions WHERE tenant_key=$1 AND role_id=$4 AND scope_id=$5 AND status<>'active')")
            .bind(&self.tenant).bind(a.person.to_string()).bind(&a.agent).bind(a.role).bind(a.scope).fetch_one(&mut **tx).await?;
        ensure!(!blocked, "catalog_not_active");
        Ok(())
    }
    pub(super) async fn organization_change(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        p: &Policy,
        change: &Change,
        now: DateTime<Utc>,
    ) -> Result<Value> {
        if let Change::SetAudience {
            collaboration,
            audience,
        } = change
        {
            return self
                .set_audience(tx, actor, p, *collaboration, audience, now)
                .await;
        }
        ensure!(Self::catalog_admin(p, actor), "catalog_management_required");
        match change {
            Change::SavePosition {
                id,
                role,
                scope,
                parent,
                label: name,
                description,
                draft,
            } => {
                label(name, 80)?;
                plain_text(description, 2000)?;
                ensure!(
                    p.scopes.iter().any(|s| s.id == *scope && s.active),
                    "scope_not_active"
                );
                let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_roles WHERE tenant_key=$1 AND id=$2 AND status='active')")
                    .bind(&self.tenant).bind(role).fetch_one(&mut **tx).await?;
                ensure!(valid, "role_not_active");
                if let Some(parent) = parent {
                    let rows=sqlx::query("WITH RECURSIVE chain AS (SELECT id,parent_id,status FROM qintopia_agent_os.collaboration_positions WHERE tenant_key=$1 AND id=$2 UNION SELECT x.id,x.parent_id,x.status FROM qintopia_agent_os.collaboration_positions x JOIN chain c ON x.id=c.parent_id WHERE x.tenant_key=$1) SELECT id,status FROM chain")
                        .bind(&self.tenant).bind(parent).fetch_all(&mut **tx).await?;
                    ensure!(
                        !rows.is_empty()
                            && rows.iter().all(|r| r.get::<String, _>("status") == "active"
                                && Some(r.get::<Uuid, _>("id")) != *id),
                        "invalid_position_parent"
                    );
                }
                let position = id.unwrap_or_else(Uuid::new_v4);
                if id.is_some() {
                    let old=sqlx::query("SELECT role_id,scope_id,status,used FROM qintopia_agent_os.collaboration_positions WHERE tenant_key=$1 AND id=$2")
                        .bind(&self.tenant).bind(position).fetch_one(&mut **tx).await?;
                    ensure!(
                        old.get::<String, _>("status") != "retired",
                        "catalog_retired"
                    );
                    let used:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_appointments WHERE tenant_key=$1 AND role_id=$2 AND scope_id=$3)")
                        .bind(&self.tenant).bind(old.get::<Uuid,_>("role_id")).bind(old.get::<Uuid,_>("scope_id")).fetch_one(&mut **tx).await?;
                    ensure!(
                        !(used || old.get::<bool, _>("used"))
                            || (!draft
                                && old.get::<Uuid, _>("role_id") == *role
                                && old.get::<Uuid, _>("scope_id") == *scope),
                        "position_history_dimensions_fixed"
                    );
                    sqlx::query("UPDATE qintopia_agent_os.collaboration_positions SET role_id=$3,scope_id=$4,parent_id=$5,label=$6,description=$7,status=$8,version=version+1 WHERE tenant_key=$1 AND id=$2")
                        .bind(&self.tenant).bind(position).bind(role).bind(scope).bind(parent).bind(name).bind(description).bind(if *draft{"draft"}else{"active"}).execute(&mut **tx).await?;
                } else {
                    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_positions(id,tenant_key,role_id,scope_id,parent_id,label,description,status) VALUES($1,$2,$3,$4,$5,$6,$7,$8)")
                        .bind(position).bind(&self.tenant).bind(role).bind(scope).bind(parent).bind(name).bind(description).bind(if *draft{"draft"}else{"active"}).execute(&mut **tx).await?;
                }
                Ok(json!({"kind":"save_position","id":position,"authority_granted":false}))
            }
            Change::SaveLedger {
                id,
                object,
                reference,
                label: name,
                nickname,
                description,
                scope,
                owner,
                draft,
            } => {
                label(name, 80)?;
                plain_text(description, 2000)?;
                ensure!(
                    nickname.chars().count() <= 80 && !nickname.chars().any(char::is_control),
                    "invalid_label"
                );
                if let Some(scope) = scope {
                    ensure!(
                        p.scopes.iter().any(|s| s.id == *scope && s.active),
                        "scope_not_active"
                    );
                }
                if let Some(owner) = owner {
                    self.known_person(tx, *owner).await?;
                }
                let entry = id.unwrap_or_else(Uuid::new_v4);
                let (reference, verified) = if let Some(id) = id {
                    let old=sqlx::query("SELECT kind,object_ref,verified,status FROM qintopia_agent_os.collaboration_ledger WHERE tenant_key=$1 AND id=$2")
                        .bind(&self.tenant).bind(id).fetch_one(&mut **tx).await?;
                    ensure!(
                        old.get::<String, _>("kind") == *object
                            && old.get::<String, _>("status") != "retired",
                        "ledger_identity_immutable"
                    );
                    ensure!(
                        reference
                            .as_ref()
                            .is_none_or(|r| r == old.get::<&str, _>("object_ref")),
                        "ledger_identity_immutable"
                    );
                    (
                        old.get::<String, _>("object_ref"),
                        old.get::<bool, _>("verified"),
                    )
                } else {
                    match object.as_str() {
                        "person" => {
                            if let Some(reference) = reference {
                                let person = Uuid::parse_str(reference)?;
                                self.known_person(tx, person).await?;
                                (person.to_string(), true)
                            } else {
                                let person = Uuid::new_v4();
                                sqlx::query("INSERT INTO qintopia_identity.persons(id,display_name,preferred_name) VALUES($1,$2,$3)")
                                .bind(person).bind(name).bind(if nickname.is_empty(){name}else{nickname}).execute(&mut **tx).await?;
                                (person.to_string(), false)
                            }
                        }
                        "agent" => {
                            let generated = format!("unconnected-{}", entry.simple());
                            let key = reference.as_deref().unwrap_or(&generated);
                            ensure!(
                                key.len() <= 80
                                    && key.bytes().all(|c| c.is_ascii_lowercase()
                                        || c.is_ascii_digit()
                                        || c == b'-'
                                        || c == b'_')
                                    && !key.is_empty(),
                                "invalid_agent_key"
                            );
                            (key.into(), agents().contains(&key))
                        }
                        "group" => {
                            let group = Uuid::parse_str(reference.as_deref().unwrap_or(""))?;
                            let known:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_messages.conversations WHERE tenant_id=$1 AND id=$2 AND chat_type='group' AND status='active')")
                                .bind(&self.tenant).bind(group).fetch_one(&mut **tx).await?;
                            ensure!(known, "group_not_verified");
                            (group.to_string(), true)
                        }
                        _ => bail!("invalid_catalog_object"),
                    }
                };
                // New people stay pending until a separate verified identity association is available.
                let status = if *draft || (!verified && object == "person") {
                    "draft"
                } else {
                    "active"
                };
                if id.is_some() {
                    let in_use = self
                        .ledger_connections(tx, object, &reference, false)
                        .await?;
                    ensure!(status == "active" || in_use.is_empty(), "catalog_in_use");
                    sqlx::query("UPDATE qintopia_agent_os.collaboration_ledger SET label=$3,nickname=$4,description=$5,scope_id=$6,owner_id=$7,status=$8,version=version+1 WHERE tenant_key=$1 AND id=$2")
                        .bind(&self.tenant).bind(entry).bind(name).bind(nickname).bind(description).bind(scope).bind(owner).bind(status).execute(&mut **tx).await?;
                } else {
                    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_ledger(id,tenant_key,kind,object_ref,label,nickname,description,scope_id,owner_id,status,verified) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,$10,$11)")
                        .bind(entry).bind(&self.tenant).bind(object).bind(reference).bind(name).bind(nickname).bind(description).bind(scope).bind(owner).bind(status).bind(verified).execute(&mut **tx).await?;
                }
                Ok(
                    json!({"kind":"save_ledger","id":entry,"verified":verified,"status":status,"authority_granted":false,"runtime_created":false}),
                )
            }
            Change::Lifecycle {
                object,
                id,
                operation,
            } => {
                self.organization_lifecycle(tx, actor, p, (object, *id, operation), now)
                    .await
            }
            _ => bail!("unknown_command"),
        }
    }

    pub(super) async fn known_person(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        id: Uuid,
    ) -> Result<()> {
        let known:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.persons p JOIN qintopia_identity.source_identity_links l ON l.person_id=p.id WHERE p.id=$1 AND p.status='active' AND l.namespace=$2 AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL) AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger WHERE tenant_key=$2 AND kind='person' AND object_ref=$1::text AND status<>'active')")
            .bind(id).bind(&self.tenant).fetch_one(&mut **tx).await?;
        ensure!(known, "person_not_verified");
        Ok(())
    }

    async fn ledger_connections(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        kind: &str,
        reference: &str,
        history: bool,
    ) -> Result<Vec<Uuid>> {
        Ok(sqlx::query_scalar("SELECT c.id FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND ($4 OR (c.status='active' AND a.status='active')) AND (($2='person' AND a.person_id::text=$3) OR ($2='agent' AND c.agent_key=$3))")
            .bind(&self.tenant).bind(kind).bind(reference).bind(history).fetch_all(&mut **tx).await?)
    }

    async fn organization_lifecycle(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        p: &Policy,
        action: (&str, Uuid, &str),
        now: DateTime<Utc>,
    ) -> Result<Value> {
        let (object, id, operation) = action;
        ensure!(
            matches!(operation, "retire" | "restore" | "delete"),
            "invalid_lifecycle"
        );
        let table = match object {
            "position" => "collaboration_positions",
            "ledger" => "collaboration_ledger",
            _ => bail!("invalid_catalog_object"),
        };
        let row = sqlx::query(&format!(
            "SELECT * FROM qintopia_agent_os.{table} WHERE tenant_key=$1 AND id=$2"
        ))
        .bind(&self.tenant)
        .bind(id)
        .fetch_one(&mut **tx)
        .await?;
        let status: String = row.get("status");
        let (connections, history) = if object == "position" {
            let children:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_positions WHERE tenant_key=$1 AND parent_id=$2)")
                .bind(&self.tenant).bind(id).fetch_one(&mut **tx).await?;
            ensure!(operation == "restore" || !children, "position_has_children");
            let rows=sqlx::query("SELECT c.id,c.status,a.status AS appointment_status FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND a.role_id=$2 AND a.scope_id=$3")
                .bind(&self.tenant).bind(row.get::<Uuid,_>("role_id")).bind(row.get::<Uuid,_>("scope_id")).fetch_all(&mut **tx).await?;
            let history = !rows.is_empty();
            (
                rows.iter()
                    .filter(|r| {
                        r.get::<String, _>("status") == "active"
                            && r.get::<String, _>("appointment_status") == "active"
                    })
                    .map(|r| r.get::<Uuid, _>("id"))
                    .collect::<Vec<_>>(),
                history,
            )
        } else {
            let kind: String = row.get("kind");
            let reference: String = row.get("object_ref");
            let connections = self
                .ledger_connections(tx, &kind, &reference, false)
                .await?;
            let history = !self
                .ledger_connections(tx, &kind, &reference, true)
                .await?
                .is_empty();
            let bound: bool = if kind == "group" {
                sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scope_bindings WHERE tenant_key=$1 AND conversation_id::text=$2)")
                .bind(&self.tenant).bind(&reference).fetch_one(&mut **tx).await?
            } else {
                false
            };
            (connections, history || bound)
        };
        match operation {
            "delete" => {
                ensure!(
                    status == "draft" && !history && !row.get::<bool, _>("used"),
                    "only_unused_draft_deletable"
                );
                sqlx::query(&format!(
                    "DELETE FROM qintopia_agent_os.{table} WHERE tenant_key=$1 AND id=$2"
                ))
                .bind(&self.tenant)
                .bind(id)
                .execute(&mut **tx)
                .await?;
            }
            "retire" => {
                ensure!(status != "retired", "catalog_retired");
                for connection in &connections {
                    self.end_connection(tx, actor, p, *connection, now).await?;
                }
                if object == "ledger" && row.get::<String, _>("kind") == "group" {
                    sqlx::query("UPDATE qintopia_agent_os.collaboration_scope_bindings SET revoked_at=$3,version=version+1 WHERE tenant_key=$1 AND conversation_id::text=$2 AND revoked_at IS NULL")
                        .bind(&self.tenant).bind(row.get::<String,_>("object_ref")).bind(now).execute(&mut **tx).await?;
                }
                sqlx::query(&format!("UPDATE qintopia_agent_os.{table} SET status='retired',used=used OR $3,version=version+1 WHERE tenant_key=$1 AND id=$2"))
                    .bind(&self.tenant).bind(id).bind(history).execute(&mut **tx).await?;
            }
            _ => {
                ensure!(status == "retired", "catalog_not_retired");
                let next = if object == "ledger"
                    && row.get::<String, _>("kind") == "person"
                    && !row.get::<bool, _>("verified")
                {
                    "draft"
                } else {
                    "active"
                };
                sqlx::query(&format!("UPDATE qintopia_agent_os.{table} SET status=$3,version=version+1 WHERE tenant_key=$1 AND id=$2"))
                    .bind(&self.tenant).bind(id).bind(next).execute(&mut **tx).await?;
            }
        }
        Ok(
            json!({"kind":"lifecycle","object":object,"id":id,"operation":operation,"ended_connections":if operation=="retire"{connections.len()}else{0},"restored_authorizations":0,"external_effects":false}),
        )
    }

    async fn set_audience(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        p: &Policy,
        id: Uuid,
        a: &Audience,
        now: DateTime<Utc>,
    ) -> Result<Value> {
        plain_text(&a.topics, 2000)?;
        ensure!(
            matches!(a.residents.as_str(), "none" | "current" | "past" | "all")
                && matches!(a.visibility.as_str(), "general" | "service_private"),
            "invalid_audience"
        );
        ensure!(
            a.groups.len() <= 100 && a.people.len() <= 100,
            "audience_too_large"
        );
        let row=sqlx::query("SELECT a.person_id,a.scope_id,c.agent_key,c.domain_key,c.duty_id FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND c.id=$2 AND c.status='active' AND a.status='active' AND (a.valid_until IS NULL OR a.valid_until>$3)")
            .bind(&self.tenant).bind(id).bind(now).fetch_one(&mut **tx).await?;
        let scope: Uuid = row.get("scope_id");
        let agent: String = row.get("agent_key");
        let domain: String = row.get("domain_key");
        ensure!(
            p.can_inspect(actor.person, scope, &agent, &domain),
            "management_denied"
        );
        let authority = p
            .manager(actor.person, scope, &agent, &domain, "publish")
            .ok_or_else(|| anyhow::anyhow!("management_denied"))?;
        for group in &a.groups {
            let known:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id WHERE b.tenant_key=$1 AND b.scope_id=$2 AND b.conversation_id=$3 AND b.revoked_at IS NULL AND c.status='active') AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger WHERE tenant_key=$1 AND kind='group' AND object_ref=$3::text AND status<>'active')")
                .bind(&self.tenant).bind(scope).bind(group).fetch_one(&mut **tx).await?;
            ensure!(known, "group_outside_scope");
        }
        for person in &a.people {
            self.known_person(tx, *person).await?;
        }
        let confirmation =
            a.reply == PermissionMode::Confirmation || a.proactive == PermissionMode::Confirmation;
        ensure!(confirmation == a.reviewer.is_some(), "reviewer_required");
        if let Some(reviewer) = a.reviewer {
            ensure!(
                reviewer != row.get::<Uuid, _>("person_id")
                    && p.grants.iter().any(|g| g.person == reviewer
                        && g.scope == scope
                        && g.agent == agent
                        && g.domain == domain
                        && g.duty == row.get::<Option<Uuid>, _>("duty_id")
                        && g.action == "review"
                        && g.mode == PermissionMode::Autonomous
                        && p.effective(g)),
                "reviewer_not_authorized"
            );
        }
        let mut configuration = serde_json::to_value(a)?;
        configuration["authority_grant"] = json!(authority.id);
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_audiences(tenant_key,collaboration_id,configuration) VALUES($1,$2,$3) ON CONFLICT(collaboration_id) DO UPDATE SET configuration=EXCLUDED.configuration,version=collaboration_audiences.version+1")
            .bind(&self.tenant).bind(id).bind(configuration).execute(&mut **tx).await?;
        Ok(
            json!({"kind":"set_audience","collaboration":id,"dynamic_membership":"requires_pms_resolution","external_effects":false}),
        )
    }

    pub(super) async fn organization_state(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        visible: &[Uuid],
        admin: bool,
    ) -> Result<Value> {
        let positions:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(to_jsonb(x)-'tenant_key' ORDER BY x.label,x.id),'[]') FROM qintopia_agent_os.collaboration_positions x WHERE tenant_key=$1 AND scope_id=ANY($2)")
            .bind(&self.tenant).bind(visible).fetch_one(&mut **tx).await?;
        let ledger:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(to_jsonb(x)-'tenant_key' ORDER BY x.label,x.id),'[]') FROM qintopia_agent_os.collaboration_ledger x WHERE tenant_key=$1 AND ($3 OR scope_id=ANY($2))")
            .bind(&self.tenant).bind(visible).bind(admin).fetch_one(&mut **tx).await?;
        let audiences:Value=sqlx::query_scalar("SELECT coalesce(jsonb_agg(jsonb_build_object('collaboration',x.collaboration_id,'configuration',x.configuration) ORDER BY x.collaboration_id),'[]') FROM qintopia_agent_os.collaboration_audiences x JOIN qintopia_agent_os.agent_collaborations c ON c.id=x.collaboration_id JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE x.tenant_key=$1 AND a.scope_id=ANY($2)")
            .bind(&self.tenant).bind(visible).fetch_one(&mut **tx).await?;
        let history: Value = if admin {
            sqlx::query_scalar("SELECT coalesce(jsonb_agg(x),'[]') FROM (SELECT created_at,result->'change' AS change FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1 ORDER BY created_at DESC LIMIT 50) x")
            .bind(&self.tenant).fetch_one(&mut **tx).await?
        } else {
            json!([])
        };
        Ok(json!({"positions":positions,"ledger":ledger,"audiences":audiences,"history":history}))
    }

    /// Configuration evaluation only. No provider call, membership assertion or reusable execution ticket.
    pub async fn contact_decision(
        &self,
        actor: &Actor,
        id: Uuid,
        kind: &str,
        target: Uuid,
        proactive: bool,
    ) -> Result<Value> {
        ensure!(
            matches!(kind, "person" | "group" | "public"),
            "invalid_audience"
        );
        let (mut tx, version, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let p = self.policy(&mut tx, now).await?;
        let row=sqlx::query("SELECT c.agent_key,c.domain_key,c.duty_id,a.scope_id,a.person_id,x.configuration FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id JOIN qintopia_agent_os.collaboration_audiences x ON x.collaboration_id=c.id WHERE c.tenant_key=$1 AND c.id=$2 AND c.status='active' AND a.status='active' AND a.valid_from<=$3 AND (a.valid_until IS NULL OR a.valid_until>$3)")
            .bind(&self.tenant).bind(id).bind(now).fetch_optional(&mut *tx).await?;
        let denied = |reason: &str| json!({"status":"denied","reason":reason,"configuration_version":version,"runtime_connected":false});
        let Some(row) = row else {
            return Ok(denied("contact_configuration_not_active"));
        };
        let scope: Uuid = row.get("scope_id");
        let agent: String = row.get("agent_key");
        let domain: String = row.get("domain_key");
        ensure!(
            (row.get::<Uuid, _>("person_id") == actor.person
                && (actor.session_hash.is_none()
                    || p.grants
                        .iter()
                        .any(|g| g.collaboration == id && p.effective(g))))
                || p.can_inspect(actor.person, scope, &agent, &domain),
            "scope_access_denied"
        );
        self.known_person(&mut tx, row.get("person_id")).await?;
        let active_source:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id JOIN qintopia_agent_os.collaboration_roles r ON r.id=a.role_id JOIN qintopia_agent_os.collaboration_duties d ON d.id=c.duty_id WHERE c.tenant_key=$1 AND c.id=$2 AND r.status='active' AND d.status='active') AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger WHERE tenant_key=$1 AND kind='agent' AND object_ref=$3 AND status<>'active')")
            .bind(&self.tenant).bind(id).bind(&agent).fetch_one(&mut *tx).await?;
        if !active_source || !p.scopes.iter().any(|s| s.id == scope && s.active) {
            return Ok(denied("contact_configuration_not_active"));
        }
        let mut configuration: Value = row.get("configuration");
        let authority = configuration
            .as_object_mut()
            .and_then(|v| v.remove("authority_grant"))
            .and_then(|v| serde_json::from_value::<Uuid>(v).ok());
        if !authority.is_some_and(|id| p.grants.iter().any(|g| g.id == id && p.effective(g))) {
            return Ok(denied("contact_authority_revoked"));
        }
        let a: Audience = serde_json::from_value(configuration)?;
        if kind == "public" {
            if proactive || !a.open_reception {
                return Ok(denied("public_reception_not_enabled"));
            }
        } else if kind == "person" {
            self.known_person(&mut tx, target).await?;
            if !a.people.contains(&target) {
                return Ok(denied(if a.residents == "none" {
                    "target_outside_scope"
                } else {
                    "pms_membership_resolution_required"
                }));
            }
        } else {
            let valid:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id WHERE b.tenant_key=$1 AND b.scope_id=$2 AND b.conversation_id=$3 AND b.revoked_at IS NULL AND c.status='active') AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_ledger WHERE tenant_key=$1 AND kind='group' AND object_ref=$3::text AND status<>'active')")
                .bind(&self.tenant).bind(scope).bind(target).fetch_one(&mut *tx).await?;
            if !a.groups.contains(&target) || !valid {
                return Ok(denied("target_outside_scope"));
            }
        }
        let mode = if proactive { a.proactive } else { a.reply };
        if mode == PermissionMode::Denied {
            return Ok(denied("contact_not_granted"));
        }
        if proactive {
            let publish = p.decision(id, "publish");
            if publish["status"] != "autonomous" {
                return Ok(
                    json!({"status":publish["status"],"reason":"publish_authority_required","reviewer":publish["reviewer"],"configuration_version":version,"runtime_connected":false}),
                );
            }
        }
        if mode == PermissionMode::Confirmation
            && !a.reviewer.is_some_and(|reviewer| {
                p.grants.iter().any(|g| {
                    g.person == reviewer
                        && g.scope == scope
                        && g.agent == agent
                        && g.domain == domain
                        && g.duty == row.get::<Option<Uuid>, _>("duty_id")
                        && g.action == "review"
                        && g.mode == PermissionMode::Autonomous
                        && p.effective(g)
                })
            })
        {
            return Ok(denied("eligible_reviewer_required"));
        }
        Ok(
            json!({"status":if mode==PermissionMode::Autonomous{"autonomous"}else{"confirmation_required"},"reviewer":a.reviewer,"visibility":if kind=="public"{"public_only"}else{&a.visibility},"configuration_version":version,"runtime_connected":false,"external_effects":false,"execution_requires":"current_identity_membership_consent_content_visibility_checks"}),
        )
    }
}
