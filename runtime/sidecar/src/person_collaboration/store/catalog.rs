//! Object maintenance uses the same transaction, version lock and audit as connections.
use super::*;

impl Store {
    pub(super) fn catalog_admin(p: &Policy, actor: &Actor) -> bool {
        p.scopes.iter().filter(|s| s.parent.is_none()).any(|s| {
            p.manager(actor.person, s.id, "default", "organization", "manage")
                .is_some()
        })
    }

    pub(super) async fn end_connection(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        p: &Policy,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Value> {
        let row = sqlx::query("SELECT c.appointment_id,c.agent_key,c.domain_key,a.scope_id FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND c.id=$2 AND c.status='active'")
            .bind(&self.tenant).bind(id).fetch_optional(&mut **tx).await?
            .ok_or_else(||anyhow::anyhow!("collaboration_not_active"))?;
        let scope: Uuid = row.get("scope_id");
        let agent: String = row.get("agent_key");
        let domain: String = row.get("domain_key");
        ensure!(
            p.can_inspect(actor.person, scope, &agent, &domain),
            "management_denied"
        );
        for g in p
            .grants
            .iter()
            .filter(|g| g.collaboration == id && g.active)
        {
            ensure!(g.parent.is_some(), "bootstrap_relation_cannot_be_rewritten");
            ensure!(
                p.manager(actor.person, scope, &agent, &domain, &g.action)
                    .is_some(),
                "management_denied"
            );
        }
        sqlx::query("UPDATE qintopia_agent_os.agent_collaborations SET status='ended',version=version+1 WHERE tenant_key=$1 AND id=$2")
            .bind(&self.tenant).bind(id).execute(&mut **tx).await?;
        // Closing the last connection closes its empty appointment, without touching any other work.
        sqlx::query("UPDATE qintopia_agent_os.collaboration_appointments SET status='ended',ended_at=$3,version=version+1 WHERE tenant_key=$1 AND id=$2 AND status='active' AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.agent_collaborations WHERE appointment_id=$2 AND status='active')")
            .bind(&self.tenant).bind(row.get::<Uuid,_>("appointment_id")).bind(now).execute(&mut **tx).await?;
        Ok(json!({"kind":"end_collaboration","collaboration":id}))
    }

    pub(super) async fn catalog_change(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        p: &Policy,
        change: &Change,
        now: DateTime<Utc>,
    ) -> Result<Value> {
        if let Change::EndCollaboration { collaboration } = change {
            return self.end_connection(tx, actor, p, *collaboration, now).await;
        }
        ensure!(Self::catalog_admin(p, actor), "catalog_management_required");
        match change {
            Change::RestoreCatalog { object, id } => {
                let table = match object.as_str() {
                    "role" => "collaboration_roles",
                    "duty" => "collaboration_duties",
                    "scope" => "collaboration_scopes",
                    _ => bail!("invalid_catalog_object"),
                };
                if object == "scope" {
                    let parent:Option<Uuid>=sqlx::query_scalar("SELECT parent_scope_id FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND id=$2")
                        .bind(&self.tenant).bind(id).fetch_one(&mut **tx).await?;
                    ensure!(
                        parent.is_none_or(|id| p.scopes.iter().any(|s| s.id == id && s.active)),
                        "scope_not_active"
                    );
                }
                let n=sqlx::query(&format!("UPDATE qintopia_agent_os.{table} SET status='active',version=version+1 WHERE tenant_key=$1 AND id=$2 AND status<>'active'"))
                    .bind(&self.tenant).bind(id).execute(&mut **tx).await?.rows_affected();
                ensure!(n == 1, "catalog_not_retired");
                Ok(json!({"kind":"restore_catalog","id":id,"restored_authorizations":0}))
            }
            Change::SaveDuty {
                id,
                label: name,
                description,
                domain,
                available_actions,
            } => {
                label(name, 80)?;
                plain_text(description, 2000)?;
                ensure!(
                    DOMAINS.contains(&domain.as_str()),
                    "unknown_agent_or_domain"
                );
                ensure!(
                    available_actions.is_empty() || valid_list(available_actions, ACTIONS),
                    "invalid_role_actions"
                );
                // Technical operations are not silently made resident-service actions by renaming a duty.
                ensure!(
                    !available_actions.iter().any(|a| a == "technical_support")
                        || domain == "technical_support",
                    "technical_duty_domain_required"
                );
                let duty = if let Some(id) = id {
                    let row=sqlx::query("SELECT domain_key,status FROM qintopia_agent_os.collaboration_duties WHERE tenant_key=$1 AND id=$2")
                        .bind(&self.tenant).bind(id).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("duty_not_found"))?;
                    ensure!(
                        row.get::<String, _>("status") == "active",
                        "catalog_retired"
                    );
                    let in_use:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND c.duty_id=$2 AND c.status='active' AND a.status='active' AND (a.valid_until IS NULL OR a.valid_until>$3))")
                        .bind(&self.tenant).bind(id).bind(now).fetch_one(&mut **tx).await?;
                    ensure!(
                        !in_use || row.get::<String, _>("domain_key") == *domain,
                        "catalog_in_use"
                    );
                    let conflicting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_grants g JOIN qintopia_agent_os.agent_collaborations c ON c.id=g.collaboration_id JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND c.duty_id=$2 AND c.status='active' AND a.status='active' AND (a.valid_until IS NULL OR a.valid_until>$4) AND g.status='active' AND g.decision_mode<>'denied' AND NOT(g.action_key=ANY($3)))")
                        .bind(&self.tenant).bind(id).bind(available_actions).bind(now).fetch_one(&mut **tx).await?;
                    ensure!(!conflicting, "duty_actions_in_use");
                    sqlx::query("UPDATE qintopia_agent_os.collaboration_duties SET label=$3,description=$4,domain_key=$5,available_actions=$6,version=version+1 WHERE tenant_key=$1 AND id=$2")
                        .bind(&self.tenant).bind(id).bind(name).bind(description).bind(domain).bind(available_actions).execute(&mut **tx).await?;
                    *id
                } else {
                    sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_duties(tenant_key,label,description,domain_key,available_actions) VALUES($1,$2,$3,$4,$5) RETURNING id")
                        .bind(&self.tenant).bind(name).bind(description).bind(domain).bind(available_actions).fetch_one(&mut **tx).await?
                };
                Ok(json!({"kind":"save_duty","duty":duty,"existing_grants_expanded":false}))
            }
            Change::SaveRole {
                id,
                label: name,
                description,
                duty_ids,
            } => {
                label(name, 80)?;
                plain_text(description, 2000)?;
                ensure!(
                    duty_ids.len() <= 100
                        && duty_ids
                            .iter()
                            .collect::<std::collections::BTreeSet<_>>()
                            .len()
                            == duty_ids.len(),
                    "invalid_duties"
                );
                let count:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_duties WHERE tenant_key=$1 AND id=ANY($2) AND status='active'")
                    .bind(&self.tenant).bind(duty_ids).fetch_one(&mut **tx).await?;
                ensure!(count == duty_ids.len() as i64, "duty_not_found");
                let role = if let Some(id) = id {
                    let conflicting:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND a.role_id=$2 AND c.status='active' AND a.status='active' AND (a.valid_until IS NULL OR a.valid_until>$4) AND c.duty_id IS NOT NULL AND NOT(c.duty_id=ANY($3)))")
                        .bind(&self.tenant).bind(id).bind(duty_ids).bind(now).fetch_one(&mut **tx).await?;
                    ensure!(!conflicting, "role_duties_in_use");
                    let n=sqlx::query("UPDATE qintopia_agent_os.collaboration_roles SET label=$3,description=$4,version=version+1 WHERE tenant_key=$1 AND id=$2 AND status='active'")
                        .bind(&self.tenant).bind(id).bind(name).bind(description).execute(&mut **tx).await?.rows_affected();
                    ensure!(n == 1, "role_not_active");
                    sqlx::query("DELETE FROM qintopia_agent_os.collaboration_role_duties WHERE tenant_key=$1 AND role_id=$2")
                        .bind(&self.tenant).bind(id).execute(&mut **tx).await?;
                    *id
                } else {
                    sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_roles(tenant_key,label,description) VALUES($1,$2,$3) RETURNING id")
                        .bind(&self.tenant).bind(name).bind(description).fetch_one(&mut **tx).await?
                };
                for duty in duty_ids {
                    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_role_duties(tenant_key,role_id,duty_id) VALUES($1,$2,$3)")
                        .bind(&self.tenant).bind(role).bind(duty).execute(&mut **tx).await?;
                }
                // Legacy compatibility only; new connections validate the specific duty association.
                sqlx::query("UPDATE qintopia_agent_os.collaboration_roles SET available_actions=ARRAY(SELECT DISTINCT unnest(d.available_actions) FROM qintopia_agent_os.collaboration_duties d JOIN qintopia_agent_os.collaboration_role_duties rd ON rd.duty_id=d.id WHERE rd.tenant_key=$1 AND rd.role_id=$2) WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(role).execute(&mut **tx).await?;
                Ok(json!({"kind":"save_role","role":role,"existing_grants_expanded":false}))
            }
            Change::UpdateScope { id, label: name } => {
                label(name, 80)?;
                let is_root:Option<bool>=sqlx::query_scalar("SELECT parent_scope_id IS NULL FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND id=$2 AND status='active'")
                    .bind(&self.tenant).bind(id).fetch_optional(&mut **tx).await?;
                let is_root = is_root.ok_or_else(|| anyhow::anyhow!("scope_not_active"))?;
                ensure!(!is_root || name == "秦托邦", "root_scope_fixed");
                sqlx::query("UPDATE qintopia_agent_os.collaboration_scopes SET label=$3,version=version+1 WHERE tenant_key=$1 AND id=$2")
                    .bind(&self.tenant).bind(id).bind(name).execute(&mut **tx).await?;
                Ok(json!({"kind":"update_scope","scope":id}))
            }
            Change::RetireCatalog { object, id } => self.retire_catalog(tx, object, *id, now).await,
            _ => bail!("unknown_command"),
        }
    }

    async fn retire_catalog(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        object: &str,
        id: Uuid,
        now: DateTime<Utc>,
    ) -> Result<Value> {
        if matches!(object, "role" | "scope") {
            let in_positions:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_positions WHERE tenant_key=$1 AND status<>'retired' AND (($3='role' AND role_id=$2) OR ($3='scope' AND scope_id=$2)))")
                .bind(&self.tenant).bind(id).bind(object).fetch_one(&mut **tx).await?;
            ensure!(!in_positions, "catalog_in_use");
        }
        let (table,active_sql,history_sql) = match object {
            "role" => ("collaboration_roles",
                "SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_appointments WHERE tenant_key=$1 AND role_id=$2 AND status='active' AND (valid_until IS NULL OR valid_until>$3))",
                "SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_appointments WHERE tenant_key=$1 AND role_id=$2)"),
            "duty" => ("collaboration_duties",
                "SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND c.duty_id=$2 AND c.status='active' AND a.status='active' AND (a.valid_until IS NULL OR a.valid_until>$3))",
                "SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.agent_collaborations WHERE tenant_key=$1 AND duty_id=$2)"),
            "scope" => ("collaboration_scopes",
                "SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_appointments WHERE tenant_key=$1 AND scope_id=$2 AND status='active' AND (valid_until IS NULL OR valid_until>$3))",
                "SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_appointments WHERE tenant_key=$1 AND scope_id=$2) OR EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND parent_scope_id=$2) OR EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scope_bindings WHERE tenant_key=$1 AND scope_id=$2)"),
            _ => bail!("invalid_catalog_object"),
        };
        let active: bool = sqlx::query_scalar(active_sql)
            .bind(&self.tenant)
            .bind(id)
            .bind(now)
            .fetch_one(&mut **tx)
            .await?;
        ensure!(!active, "catalog_in_use");
        if object == "scope" {
            let root:bool=sqlx::query_scalar("SELECT parent_scope_id IS NULL FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND id=$2")
                .bind(&self.tenant).bind(id).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("scope_not_active"))?;
            ensure!(!root, "root_scope_fixed");
            let dependencies:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND parent_scope_id=$2 AND status='active') OR EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_scope_bindings WHERE tenant_key=$1 AND scope_id=$2 AND revoked_at IS NULL)")
                .bind(&self.tenant).bind(id).fetch_one(&mut **tx).await?;
            ensure!(!dependencies, "scope_has_active_bindings");
        }
        let historical: bool = sqlx::query_scalar(history_sql)
            .bind(&self.tenant)
            .bind(id)
            .fetch_one(&mut **tx)
            .await?;
        // Existing definitions are not explicit unused drafts. Keep their associations/history.
        let query = format!("UPDATE qintopia_agent_os.{table} SET status='{}',version=version+1 WHERE tenant_key=$1 AND id=$2",if object=="scope"{"revoked"}else{"retired"});
        let n = sqlx::query(&query)
            .bind(&self.tenant)
            .bind(id)
            .execute(&mut **tx)
            .await?
            .rows_affected();
        ensure!(n == 1, "catalog_not_found");
        Ok(
            json!({"kind":"retire_catalog","object":object,"id":id,"disposition":"retired","has_history":historical}),
        )
    }

    pub(super) async fn seed_duties(&self, tx: &mut Transaction<'_, Postgres>) -> Result<()> {
        for (name, domain, actions) in [
            (
                "居民服务",
                "community_service",
                vec![
                    "confirm_knowledge",
                    "train",
                    "change_rules",
                    "review",
                    "publish",
                    "designate",
                ],
            ),
            (
                "客房协调",
                "hospitality",
                vec![
                    "confirm_knowledge",
                    "train",
                    "change_rules",
                    "review",
                    "publish",
                    "identity",
                ],
            ),
            (
                "活动运营",
                "activity_operations",
                vec![
                    "confirm_knowledge",
                    "train",
                    "change_rules",
                    "review",
                    "publish",
                    "designate",
                ],
            ),
            (
                "技术支持",
                "technical_support",
                vec!["technical_support", "train"],
            ),
            (
                "组织管理",
                "organization",
                vec!["manage", "identity", "review"],
            ),
        ] {
            let duty:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_duties(tenant_key,label,description,domain_key,available_actions) VALUES($1,$2,$3,$4,$5) RETURNING id")
                .bind(&self.tenant).bind(name).bind(format!("{name}的工作职责；实际权限在具体工作连接中确认。"))
                .bind(domain).bind(actions).fetch_one(&mut **tx).await?;
            let role_names: Vec<&str> = match domain {
                "community_service" => vec!["公司负责人", "社区负责人", "舍长", "小管家"],
                "hospitality" => vec!["公司负责人", "社区负责人", "小管家"],
                "activity_operations" => vec!["公司负责人", "社区负责人", "活动运营"],
                "technical_support" => vec!["技术负责人"],
                _ => vec!["公司负责人", "社区负责人"],
            };
            sqlx::query("INSERT INTO qintopia_agent_os.collaboration_role_duties(tenant_key,role_id,duty_id) SELECT tenant_key,id,$3 FROM qintopia_agent_os.collaboration_roles WHERE tenant_key=$1 AND label=ANY($2)")
                .bind(&self.tenant).bind(role_names).bind(duty).execute(&mut **tx).await?;
        }
        Ok(())
    }
}
