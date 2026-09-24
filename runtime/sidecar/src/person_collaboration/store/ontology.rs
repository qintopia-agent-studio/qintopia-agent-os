//! Human-readable, read-only explanations of the same governed configuration.
use super::*;

impl Store {
    pub async fn ontology(&self, actor: &Actor, scope: Uuid) -> Result<Value> {
        let (mut tx, version, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let policy = self.policy(&mut tx, now).await?;
        ensure!(
            policy.scopes.iter().any(|s| s.id == scope && s.active),
            "scope_access_denied"
        );
        if let Some((_, _, bound)) = actor.gateway.as_ref() {
            ensure!(policy.in_scope(scope, *bound, true), "scope_access_denied");
        }
        ensure!(
            policy
                .manager(actor.person, scope, "default", "organization", "manage")
                .is_some()
                || policy.can_inspect(actor.person, scope, "erhua", "community_service")
                || policy.grants.iter().any(|g| g.person == actor.person
                    && g.agent == "erhua"
                    && g.domain == "community_service"
                    && policy.effective(g)
                    && policy.in_scope(scope, g.scope, g.descendants)),
            "scope_access_denied"
        );
        let label: String = sqlx::query_scalar("SELECT label FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND id=$2")
            .bind(&self.tenant).bind(scope).fetch_one(&mut *tx).await?;
        let ancestors: Vec<Uuid> = policy
            .scopes
            .iter()
            .filter(|s| s.active && policy.in_scope(scope, s.id, true))
            .map(|s| s.id)
            .collect();
        // Only current general rules and shared ancestor constraints. Per-case
        // business settings stay in the conversation rather than this admin view.
        let rows = sqlx::query("SELECT DISTINCT ON (i.id) r.id,i.knowledge_key,i.kind,i.scope_id,s.label AS scope_label,d.version,d.definition->'content' AS content,r.author_person_id,r.authority_grant_id,coalesce(p.preferred_name,p.display_name) AS author_label,r.effective_at,r.effective_until FROM qintopia_agent_os.collaboration_knowledge_items i JOIN qintopia_agent_os.collaboration_knowledge_revisions r ON r.item_id=i.id AND r.tenant_key=i.tenant_key JOIN qintopia_agent_os.business_definition_versions d ON d.id=r.id JOIN qintopia_agent_os.collaboration_scopes s ON s.id=i.scope_id AND s.tenant_key=i.tenant_key JOIN qintopia_identity.persons p ON p.id=r.author_person_id WHERE i.tenant_key=$1 AND i.scope_id=ANY($2) AND (i.scope_id=$3 OR i.shared) AND i.case_ref IS NULL AND i.kind IN ('culture','principle','rule') AND i.knowledge_key<>'resident_welcome' AND r.withdrawn_at IS NULL AND d.status='shadow' AND r.effective_at<=$4 AND (r.effective_until IS NULL OR r.effective_until>$4) ORDER BY i.id,r.effective_at DESC,d.version DESC")
            .bind(&self.tenant).bind(&ancestors).bind(scope).bind(now).fetch_all(&mut *tx).await?;
        let mut constraints = Vec::new();
        for r in rows {
            let kind: String = r.get("kind");
            if kind == "rule" {
                // Match the consuming runtime: a replacement appointment must not
                // revive an executable rule authored under a revoked grant.
                let authority = foundation::authorize_current(
                    &mut tx,
                    &self.tenant,
                    r.get("author_person_id"),
                    r.get("scope_id"),
                    "erhua",
                    "community_service",
                    "change_rules",
                )
                .await?;
                if authority.status == "denied"
                    || authority.grant_id != Some(r.get("authority_grant_id"))
                {
                    continue;
                }
            }
            let content: Value = r.get("content");
            let title =
                content
                    .get("title")
                    .and_then(Value::as_str)
                    .unwrap_or(match kind.as_str() {
                        "culture" => "社区共同约定",
                        "principle" => "共同责任边界",
                        _ => "本范围有效规则",
                    });
            let source_scope: Uuid = r.get("scope_id");
            constraints.push(json!({"id":r.get::<Uuid,_>("id"),"key":r.get::<String,_>("knowledge_key"),"title":title,"content":content,"version":r.get::<i32,_>("version"),"scope":{"id":source_scope,"label":r.get::<String,_>("scope_label")},"inherited":source_scope!=scope,"effective_at":r.get::<DateTime<Utc>,_>("effective_at"),"effective_until":r.get::<Option<DateTime<Utc>>,_>("effective_until"),"source":{"kind":"confirmed_rule","label":"有权负责人确认的规则记录","ref":r.get::<Uuid,_>("id")},"author":{"id":r.get::<Uuid,_>("author_person_id"),"label":r.get::<String,_>("author_label")}}));
        }
        let registered = agents();
        let executors: Vec<String> = sqlx::query_scalar("SELECT agent_key FROM qintopia_agent_os.collaboration_local_executors WHERE tenant_key=$1 AND available")
            .bind(&self.tenant).fetch_all(&mut *tx).await?;
        let ledger: Vec<(String,String)> = sqlx::query_as("SELECT object_ref,status FROM qintopia_agent_os.collaboration_ledger WHERE tenant_key=$1 AND kind='agent'")
            .bind(&self.tenant).fetch_all(&mut *tx).await?;
        let configured: Vec<String> = sqlx::query_scalar("SELECT DISTINCT c.agent_key FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE c.tenant_key=$1 AND a.scope_id=$2 AND c.status='active' AND a.status='active' AND (a.valid_until IS NULL OR a.valid_until>$3)")
            .bind(&self.tenant).bind(scope).bind(now).fetch_all(&mut *tx).await?;
        let enabled = std::env::var("QINTOPIA_FOUNDATION_LOCAL_ENABLE").as_deref() == Ok("1");
        let capabilities: Vec<Value> = registered.iter().map(|agent| {
            let registration = ledger.iter().find(|(key,_)|key==agent).map(|(_,status)|status.as_str()).unwrap_or("active");
            let supported = matches!(*agent,"erhua"|"anan"|"huabaosi");
            let ready = supported && enabled && executors.iter().any(|key|key==agent) && registration=="active";
            let functions: &[(&str,&str)] = match *agent {"erhua"=>&[("foundation_context","读取职责与有效规则"),("foundation_rule","按授权保存本栋规则"),("welcome_review_forward","传递审核与转发任务")],"anan"=>&[("welcome_orchestration","协调已授权事项")],"huabaosi"=>&[("welcome_card","制作已授权卡片")],_=>&[]};
            let status=if ready{"local_ready"}else if supported{"unavailable"}else{"not_connected"};
            let reason=if ready{"已接入本地受控工具；真实模型和渠道尚未启用"}else if supported{"本地执行器或入口尚未就绪，任务会等待恢复"}else{"已登记合作对象，本页尚未接通对应业务执行入口"};
            json!({"agent_key":agent,"registration_status":registration,"configured":configured.iter().any(|key|key==agent),"local_status":status,"real_channel_enabled":false,"reason":reason,"capabilities":functions.iter().map(|(key,label)|json!({"key":key,"label":label,"status":status,"reason":reason})).collect::<Vec<_>>()})
        }).collect();
        Ok(
            json!({"scope":{"id":scope,"label":label},"configuration_version":version,"constraints":constraints,"capabilities":capabilities,"production_enabled":false}),
        )
    }

    pub(super) async fn audit_snapshot(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        change: &Change,
        created: Option<Uuid>,
    ) -> Result<Option<Value>> {
        match change {
            Change::Assign(a) => Ok(Some(self.work_snapshot(tx,created.or(a.collaboration)).await?)),
            Change::EndCollaboration{collaboration} => Ok(Some(self.work_snapshot(tx,Some(*collaboration)).await?)),
            Change::RevokeGrant{grant} => Ok(sqlx::query_scalar("SELECT to_jsonb(g)-'tenant_key' FROM qintopia_agent_os.collaboration_grants g WHERE tenant_key=$1 AND id=$2").bind(&self.tenant).bind(grant).fetch_optional(&mut **tx).await?),
            Change::EndAppointment{appointment} => Ok(sqlx::query_scalar("SELECT jsonb_build_object('appointment',to_jsonb(a)-'tenant_key','connections',(SELECT coalesce(jsonb_agg(to_jsonb(c)-'tenant_key'),'[]') FROM qintopia_agent_os.agent_collaborations c WHERE c.appointment_id=a.id)) FROM qintopia_agent_os.collaboration_appointments a WHERE tenant_key=$1 AND id=$2").bind(&self.tenant).bind(appointment).fetch_optional(&mut **tx).await?),
            Change::SaveRole{id,..} => self.catalog_snapshot(tx,"collaboration_roles",id.or(created)).await,
            Change::SaveDuty{id,..} => self.catalog_snapshot(tx,"collaboration_duties",id.or(created)).await,
            Change::UpdateScope{id,..} => self.catalog_snapshot(tx,"collaboration_scopes",Some(*id)).await,
            Change::SetGroups{scope,..} => Ok(Some(sqlx::query_scalar("SELECT jsonb_build_object('scope_id',$2::uuid,'groups',coalesce(jsonb_agg(conversation_id ORDER BY conversation_id),'[]')) FROM qintopia_agent_os.collaboration_scope_bindings WHERE tenant_key=$1 AND scope_id=$2 AND revoked_at IS NULL").bind(&self.tenant).bind(scope).fetch_one(&mut **tx).await?)),
            _ => self.configuration_snapshot(tx,change,created).await,
        }
    }
    async fn catalog_snapshot(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        table: &str,
        id: Option<Uuid>,
    ) -> Result<Option<Value>> {
        let Some(id) = id else { return Ok(None) };
        // table is chosen exclusively by the fixed match above, never an HTTP parameter.
        Ok(sqlx::query_scalar(&format!("SELECT to_jsonb(x)-'tenant_key' FROM qintopia_agent_os.{table} x WHERE tenant_key=$1 AND id=$2")).bind(&self.tenant).bind(id).fetch_optional(&mut **tx).await?)
    }
}
