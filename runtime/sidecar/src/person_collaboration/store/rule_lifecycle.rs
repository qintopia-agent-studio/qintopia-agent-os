//! Text-rule lifecycle on the existing knowledge versions and WorkItems.
use super::{foundation::*, Actor, Store};
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct RuleCommand {
    pub operation_id: Uuid,
    pub scope: Uuid,
    pub key: String,
    #[serde(default = "rule_kind", skip_serializing_if = "is_rule")]
    pub kind: String,
    pub expected_version: i64,
    pub change: RuleEdit,
}
#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum RuleEdit {
    Save {
        content: Value,
        effective_at: Option<DateTime<Utc>>,
        effective_until: Option<DateTime<Utc>>,
        #[serde(default)]
        replace_revision: Option<Uuid>,
        #[serde(default)]
        reactivate: bool,
    },
    Stop,
    CancelScheduled {
        revision_id: Uuid,
    },
}
fn is_rule(kind: &str) -> bool {
    kind == "rule"
}
fn rule_kind() -> String {
    "rule".into()
}
pub(super) fn input_action(input: &Value) -> &'static str {
    match input.get("lifecycle").unwrap_or(input)["kind"]
        .as_str()
        .unwrap_or("rule")
    {
        "rule" | "principle" => "change_rules",
        _ => "confirm_knowledge",
    }
}
impl RuleCommand {
    pub(super) fn permission(&self) -> &'static str {
        if self.kind == "rule" {
            "change_rules"
        } else {
            "confirm_knowledge"
        }
    }
    fn action(&self) -> &'static str {
        match self.change {
            RuleEdit::Save { .. } => "save",
            RuleEdit::Stop => "stop",
            RuleEdit::CancelScheduled { .. } => "cancel_scheduled",
        }
    }
}

async fn reviewer_name(
    tx: &mut Transaction<'_, Postgres>,
    person: Option<Uuid>,
) -> Result<Option<String>> {
    Ok(sqlx::query_scalar(
        "SELECT coalesce(preferred_name,display_name) FROM qintopia_identity.persons WHERE id=$1",
    )
    .bind(person)
    .fetch_optional(&mut **tx)
    .await?)
}

async fn snapshot(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    scope: Uuid,
    key: &str,
) -> Result<Value> {
    Ok(sqlx::query_scalar::<_, Option<Value>>("SELECT jsonb_build_object('key',i.knowledge_key,'kind',i.kind,'version',i.version,'stopped_at',i.stopped_at,'revisions',COALESCE((SELECT jsonb_agg(jsonb_build_object('id',r.id,'version',d.version,'content',d.definition->'content','effective_at',r.effective_at,'effective_until',r.effective_until,'withdrawn_at',r.withdrawn_at,'author',coalesce(p.preferred_name,p.display_name)) ORDER BY d.version DESC) FROM qintopia_agent_os.collaboration_knowledge_revisions r JOIN qintopia_agent_os.business_definition_versions d ON d.id=r.id LEFT JOIN qintopia_identity.persons p ON p.id=r.author_person_id WHERE r.item_id=i.id),'[]'::jsonb)) FROM qintopia_agent_os.collaboration_knowledge_items i WHERE i.tenant_key=$1 AND i.scope_id=$2 AND i.knowledge_key=$3 AND i.case_ref IS NULL")
        .bind(tenant).bind(scope).bind(key).fetch_optional(&mut **tx).await?.flatten().unwrap_or(Value::Null))
}
async fn validate(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    cmd: &RuleCommand,
) -> Result<Value> {
    ensure!(
        cmd.key.len() <= 80
            && cmd.key.starts_with(|c: char| c.is_ascii_lowercase())
            && cmd
                .key
                .bytes()
                .all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || b"_.-".contains(&c)),
        "invalid_knowledge_key"
    );
    ensure!(
        ["rule", "fact", "culture", "experience"].contains(&cmd.kind.as_str()),
        "invalid_knowledge_kind"
    );
    let inherited:bool=sqlx::query_scalar("WITH RECURSIVE ancestors AS (SELECT parent_scope_id FROM qintopia_agent_os.collaboration_scopes WHERE id=$2 AND tenant_key=$1 UNION ALL SELECT s.parent_scope_id FROM qintopia_agent_os.collaboration_scopes s JOIN ancestors a ON a.parent_scope_id=s.id WHERE s.tenant_key=$1) SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.collaboration_knowledge_items i JOIN ancestors a ON a.parent_scope_id=i.scope_id WHERE i.tenant_key=$1 AND i.knowledge_key=$3 AND i.kind='principle' AND i.shared)")
        .bind(tenant).bind(cmd.scope).bind(&cmd.key).fetch_one(&mut **tx).await?;
    ensure!(!inherited, "inherited_principle_cannot_be_overridden");
    let before = snapshot(tx, tenant, cmd.scope, &cmd.key).await?;
    ensure!(
        before["version"].as_i64().unwrap_or(0) == cmd.expected_version,
        "knowledge_version_conflict"
    );
    let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
        .fetch_one(&mut **tx)
        .await?;
    if !before.is_null() {
        let supported: bool = sqlx::query_scalar("SELECT kind=$4 AND NOT shared FROM qintopia_agent_os.collaboration_knowledge_items WHERE tenant_key=$1 AND scope_id=$2 AND knowledge_key=$3 AND case_ref IS NULL").bind(tenant).bind(cmd.scope).bind(&cmd.key).bind(&cmd.kind).fetch_one(&mut **tx).await?;
        let content = &before["revisions"][0]["content"];
        ensure!(
            supported
                && content.as_object().is_some_and(|o| o.contains_key("text")
                    && o.keys().all(|k| ["text", "title"].contains(&k.as_str()))),
            "text_rule_required"
        );
    }
    let scheduled = |revision: Uuid| -> bool {
        before["revisions"].as_array().is_some_and(|rs| {
            rs.iter().any(|r| {
                r["id"] == json!(revision)
                    && r["withdrawn_at"].is_null()
                    && r["effective_at"]
                        .as_str()
                        .and_then(|v| v.parse::<DateTime<Utc>>().ok())
                        .is_some_and(|v| v > now)
            })
        })
    };
    match &cmd.change {
        RuleEdit::Save {
            content,
            effective_at,
            effective_until,
            replace_revision,
            reactivate,
        } => {
            ensure!(
                content
                    .as_object()
                    .is_some_and(|o| o.keys().all(|k| ["text", "title"].contains(&k.as_str())))
                    && content["text"]
                        .as_str()
                        .is_some_and(|v| !v.trim().is_empty() && v.len() <= 12000)
                    && content["title"]
                        .as_str()
                        .is_some_and(|v| !v.trim().is_empty() && v.chars().count() <= 80),
                "invalid_knowledge_content"
            );
            ensure!(
                before["stopped_at"].is_null() || *reactivate,
                "knowledge_stopped"
            );
            ensure!(
                effective_until.is_none_or(|until| until > effective_at.unwrap_or(now).max(now)),
                "invalid_effective_interval"
            );
            if let Some(revision) = replace_revision {
                ensure!(scheduled(*revision), "scheduled_revision_changed");
            }
        }
        RuleEdit::Stop => ensure!(
            !before.is_null() && before["stopped_at"].is_null(),
            "knowledge_stopped"
        ),
        RuleEdit::CancelScheduled { revision_id } => {
            ensure!(scheduled(*revision_id), "scheduled_revision_changed")
        }
    }
    Ok(before)
}
impl Store {
    pub(crate) async fn rule_command(&self, actor: &Actor, cmd: &RuleCommand) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        if let Some((_, _, scope)) = actor.gateway {
            ensure!(scope == cmd.scope, "gateway_scope_mismatch");
        }
        let auth = authorize_current(
            &mut tx,
            &self.tenant,
            actor.person,
            cmd.scope,
            "erhua",
            "community_service",
            cmd.permission(),
        )
        .await?;
        ensure!(auth.status != "denied", "scope_access_denied");
        let hash = super::super::digest(&serde_json::to_vec(cmd)?);
        if let Some(mut result) =
            receipt(&mut tx, &self.tenant, actor.person, cmd.operation_id, &hash).await?
        {
            result["replayed"] = json!(true);
            return Ok(result);
        }
        // Replay a queued command before validating its possibly superseded version.
        let queued: Option<Value> = sqlx::query_scalar(
            "SELECT payload->'input' FROM qintopia_agent_os.work_items WHERE idempotency_key=$1",
        )
        .bind(format!("foundation/{}/{}", self.tenant, cmd.operation_id))
        .fetch_optional(&mut *tx)
        .await?;
        let input = if let Some(input) = &queued {
            ensure!(
                input["lifecycle"] == serde_json::to_value(cmd)?,
                "idempotency_conflict"
            );
            input.clone()
        } else {
            let before = validate(&mut tx, &self.tenant, cmd).await?;
            json!({"lifecycle":cmd,"before":before})
        };
        if auth.status == "confirmation_required" || queued.is_some() {
            let result = self
                .queue_work(
                    &mut tx,
                    actor,
                    cmd.operation_id,
                    cmd.scope,
                    "erhua.foundation_rule",
                    &input,
                    auth.grant_id.unwrap(),
                    "awaiting_review",
                )
                .await?;
            tx.commit().await?;
            return Ok(result);
        }
        let result = self
            .apply_rule_command(&mut tx, actor, cmd, auth.grant_id.unwrap())
            .await?;
        tx.commit().await?;
        Ok(result)
    }

    pub(super) async fn apply_rule_command(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        actor: &Actor,
        cmd: &RuleCommand,
        grant: Uuid,
    ) -> Result<Value> {
        let hash = super::super::digest(&serde_json::to_vec(cmd)?);
        if let Some(result) =
            receipt(tx, &self.tenant, actor.person, cmd.operation_id, &hash).await?
        {
            return Ok(result);
        }
        let before = validate(tx, &self.tenant, cmd).await?;
        let result = match &cmd.change {
            RuleEdit::Save {
                content,
                effective_at,
                effective_until,
                replace_revision,
                ..
            } => {
                if let Some(revision) = replace_revision {
                    sqlx::query("UPDATE qintopia_agent_os.collaboration_knowledge_revisions SET withdrawn_at=clock_timestamp() WHERE id=$1").bind(revision).execute(&mut **tx).await?;
                }
                sqlx::query("UPDATE qintopia_agent_os.collaboration_knowledge_items SET stopped_at=NULL WHERE tenant_key=$1 AND scope_id=$2 AND knowledge_key=$3 AND case_ref IS NULL").bind(&self.tenant).bind(cmd.scope).bind(&cmd.key).execute(&mut **tx).await?;
                let write = KnowledgeWrite {
                    operation_id: Uuid::new_v4(),
                    expected_version: cmd.expected_version,
                    scope: cmd.scope,
                    key: cmd.key.clone(),
                    kind: cmd.kind.clone(),
                    shared: false,
                    case_ref: None,
                    content: content.clone(),
                    effective_at: *effective_at,
                    effective_until: *effective_until,
                };
                let saved =
                    put_authorized_knowledge(tx, &self.tenant, actor.person, &write, grant).await?;
                sqlx::query("UPDATE qintopia_agent_os.collaboration_knowledge_items SET lifecycle_managed=true WHERE tenant_key=$1 AND scope_id=$2 AND knowledge_key=$3 AND case_ref IS NULL").bind(&self.tenant).bind(cmd.scope).bind(&cmd.key).execute(&mut **tx).await?;
                json!({"status":"saved","knowledge":saved,"notification_status":"not_configured"})
            }
            RuleEdit::Stop | RuleEdit::CancelScheduled { .. } => {
                let revision = match cmd.change {
                    RuleEdit::CancelScheduled { revision_id } => Some(revision_id),
                    _ => None,
                };
                sqlx::query("UPDATE qintopia_agent_os.collaboration_knowledge_revisions r SET withdrawn_at=clock_timestamp() FROM qintopia_agent_os.collaboration_knowledge_items i WHERE r.item_id=i.id AND i.tenant_key=$1 AND i.scope_id=$2 AND i.knowledge_key=$3 AND i.case_ref IS NULL AND r.withdrawn_at IS NULL AND ($4::uuid IS NULL OR r.id=$4)").bind(&self.tenant).bind(cmd.scope).bind(&cmd.key).bind(revision).execute(&mut **tx).await?;
                sqlx::query("UPDATE qintopia_agent_os.collaboration_knowledge_items SET version=version+1,lifecycle_managed=true,stopped_at=CASE WHEN $4 THEN clock_timestamp() ELSE stopped_at END WHERE tenant_key=$1 AND scope_id=$2 AND knowledge_key=$3 AND case_ref IS NULL").bind(&self.tenant).bind(cmd.scope).bind(&cmd.key).bind(revision.is_none()).execute(&mut **tx).await?;
                json!({"status":if revision.is_none(){"stopped"}else{"schedule_cancelled"},"latest_version":cmd.expected_version+1})
            }
        };
        let after = snapshot(tx, &self.tenant, cmd.scope, &cmd.key).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_rule_events(id,tenant_key,scope_id,knowledge_key,operation_id,actor_person_id,action,before_state,after_state) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9)").bind(Uuid::new_v4()).bind(&self.tenant).bind(cmd.scope).bind(&cmd.key).bind(cmd.operation_id).bind(actor.person).bind(cmd.action()).bind(&before).bind(&after).execute(&mut **tx).await?;
        record_receipt(
            tx,
            &self.tenant,
            actor.person,
            cmd.operation_id,
            &hash,
            &result,
        )
        .await?;
        Ok(result)
    }

    pub(crate) async fn rule_state(&self, actor: &Actor, scope: Uuid) -> Result<Value> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let policy = self.policy(&mut tx, now).await?;
        self.read_scope(&policy, actor, scope)?;
        let rules = knowledge_context(&mut tx, &self.tenant, scope).await?;
        let auth = authorize_current(
            &mut tx,
            &self.tenant,
            actor.person,
            scope,
            "erhua",
            "community_service",
            "change_rules",
        )
        .await?;
        let rule_reviewer_name = reviewer_name(&mut tx, auth.reviewer).await?;
        let knowledge_auth = authorize_current(
            &mut tx,
            &self.tenant,
            actor.person,
            scope,
            "erhua",
            "community_service",
            "confirm_knowledge",
        )
        .await?;
        let knowledge_reviewer = reviewer_name(&mut tx, knowledge_auth.reviewer).await?;
        let context = json!({"scope":scope,"knowledge":rules,"permissions":[{"action":"change_rules","decision":{"status":auth.status,"reviewer":auth.reviewer,"reviewer_name":rule_reviewer_name}},{"action":"confirm_knowledge","decision":{"status":knowledge_auth.status,"reviewer":knowledge_auth.reviewer,"reviewer_name":knowledge_reviewer}}]});
        let keys:Vec<String>=sqlx::query_scalar("SELECT knowledge_key FROM qintopia_agent_os.collaboration_knowledge_items WHERE tenant_key=$1 AND scope_id=$2 AND case_ref IS NULL AND kind IN ('rule','fact','culture','experience') AND NOT shared ORDER BY knowledge_key").bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
        let mut items = vec![];
        for key in keys {
            let mut item = snapshot(&mut tx, &self.tenant, scope, &key).await?;
            if !item["revisions"][0]["content"]
                .as_object()
                .is_some_and(|v| {
                    v.contains_key("text")
                        && v.keys().all(|k| ["text", "title"].contains(&k.as_str()))
                })
            {
                continue;
            }
            item["current"] = context["knowledge"]
                .as_array()
                .unwrap()
                .iter()
                .find(|r| r["key"] == key && r["scope"] == json!(scope))
                .cloned()
                .unwrap_or(Value::Null);
            item["scheduled"] = json!(item["revisions"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|r| r["withdrawn_at"].is_null()
                    && r["effective_at"]
                        .as_str()
                        .and_then(|v| v.parse::<DateTime<Utc>>().ok())
                        .is_some_and(|v| v > now))
                .collect::<Vec<_>>());
            items.push(item);
        }
        let events:Vec<Value>=sqlx::query_scalar("SELECT jsonb_build_object('key',e.knowledge_key,'action',e.action,'at',e.created_at,'actor',p.preferred_name,'before',e.before_state,'after',e.after_state) FROM qintopia_agent_os.collaboration_rule_events e JOIN qintopia_identity.persons p ON p.id=e.actor_person_id WHERE e.tenant_key=$1 AND e.scope_id=$2 ORDER BY e.created_at DESC LIMIT 100").bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
        tx.commit().await?;
        Ok(
            json!({"context":context,"items":items,"events":events,"tasks":self.rule_tasks(actor,scope).await?}),
        )
    }

    pub(crate) async fn rule_tasks(&self, actor: &Actor, scope: Uuid) -> Result<Vec<Value>> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        self.read_scope(&self.policy(&mut tx, now).await?, actor, scope)?;
        let rows=sqlx::query("SELECT w.id,w.status,w.payload,w.last_error,r.person_id,r.authority_grant_id,r.result,coalesce(p.preferred_name,p.display_name) AS preferred_name FROM qintopia_agent_os.collaboration_work_requests r JOIN qintopia_agent_os.work_items w ON w.id=r.work_item_id JOIN qintopia_identity.persons p ON p.id=r.person_id WHERE r.tenant_key=$1 AND r.scope_id=$2 AND w.capability_key='erhua.foundation_rule' ORDER BY w.created_at DESC LIMIT 100").bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
        let mut tasks = vec![];
        for row in rows {
            let payload: Value = row.get("payload");
            let permission = input_action(&payload["input"]);
            let person: Uuid = row.get("person_id");
            let auth = authorize_current(
                &mut tx,
                &self.tenant,
                person,
                scope,
                "erhua",
                "community_service",
                permission,
            )
            .await?;
            let own = person == actor.person;
            let mine = authorize_current(
                &mut tx,
                &self.tenant,
                actor.person,
                scope,
                "erhua",
                "community_service",
                permission,
            )
            .await?;
            let review = auth.status == "confirmation_required"
                && auth.reviewer == Some(actor.person)
                && auth.grant_id == Some(row.get("authority_grant_id"))
                && mine.status == "autonomous";
            if !own && !review {
                continue;
            }
            let status: String = row.get("status");
            let payload: Value = row.get("payload");
            let reviewer_name = reviewer_name(&mut tx, auth.reviewer).await?;
            tasks.push(json!({"id":row.get::<Uuid,_>("id"),"status":status,"reviewer":auth.reviewer,"reviewer_name":reviewer_name,"requester":row.get::<String,_>("preferred_name"),"input":payload["input"],"result":row.get::<Option<Value>,_>("result"),"reason":row.get::<Option<String>,_>("last_error"),"can_review":review && status=="awaiting_review","can_execute":review&&status=="queued","can_cancel":own&&["awaiting_review","queued"].contains(&status.as_str())}));
        }
        Ok(tasks)
    }

    pub(crate) async fn rule_work_decide(
        &self,
        actor: &Actor,
        scope: Uuid,
        work: Uuid,
        action: &str,
    ) -> Result<Value> {
        let tasks = self.rule_tasks(actor, scope).await?;
        let task = tasks
            .iter()
            .find(|r| r["id"] == json!(work))
            .ok_or_else(|| anyhow::anyhow!("scope_access_denied"))?;
        ensure!(
            ["approve", "reject", "cancel"].contains(&action),
            "unknown_action"
        );
        let permission = input_action(&task["input"]);
        if ["completed", "failed", "cancelled"]
            .iter()
            .any(|s| task["status"] == *s)
        {
            return Ok(
                json!({"work_item_id":work,"status":task["status"],"result":task["result"],"reason":task["reason"],"replayed":true}),
            );
        }
        if action == "approve" {
            ensure!(
                task["can_review"] == true || task["can_execute"] == true,
                "designated_confirmation_required"
            );
            if task["can_review"] == true {
                self.foundation_approve_rule(actor, work).await?;
            }
            return self.foundation_execute_work(work).await;
        }
        ensure!(
            (action == "reject" && task["can_review"] == true)
                || (action == "cancel" && task["can_cancel"] == true),
            "scope_access_denied"
        );
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        self.read_scope(&self.policy(&mut tx, now).await?, actor, scope)?;
        // Recheck the decision under the same lock used by execution and revocation.
        let row=sqlx::query("SELECT r.person_id,r.authority_grant_id,w.status FROM qintopia_agent_os.collaboration_work_requests r JOIN qintopia_agent_os.work_items w ON w.id=r.work_item_id WHERE r.tenant_key=$1 AND r.scope_id=$2 AND w.id=$3 FOR UPDATE OF w").bind(&self.tenant).bind(scope).bind(work).fetch_one(&mut *tx).await?;
        ensure!(
            ["awaiting_review", "queued"].contains(&row.get::<String, _>("status").as_str()),
            "review_not_pending"
        );
        if action == "cancel" {
            ensure!(
                row.get::<Uuid, _>("person_id") == actor.person,
                "scope_access_denied"
            );
        } else {
            let auth = authorize_current(
                &mut tx,
                &self.tenant,
                row.get("person_id"),
                scope,
                "erhua",
                "community_service",
                permission,
            )
            .await?;
            let mine = authorize_current(
                &mut tx,
                &self.tenant,
                actor.person,
                scope,
                "erhua",
                "community_service",
                permission,
            )
            .await?;
            ensure!(
                row.get::<String, _>("status") == "awaiting_review"
                    && auth.reviewer == Some(actor.person)
                    && auth.status == "confirmation_required"
                    && auth.grant_id == Some(row.get("authority_grant_id"))
                    && mine.status == "autonomous",
                "designated_confirmation_required"
            );
        }
        sqlx::query("UPDATE qintopia_agent_os.work_items SET status='cancelled',last_error=$2,updated_at=clock_timestamp() WHERE id=$1").bind(work).bind(if action=="reject"{"rejected_by_reviewer"}else{"cancelled_by_requester"}).execute(&mut *tx).await?;
        work_event(
            &mut tx,
            work,
            action,
            "human",
            &json!({"person":actor.person}),
        )
        .await?;
        tx.commit().await?;
        Ok(json!({"status":"cancelled","action":action,"work_item_id":work}))
    }
}
