//! Shared self-service memory: the model never selects a Person or provenance.
use super::{Actor, Store};
use anyhow::{ensure, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{PgPool, Row};
use uuid::Uuid;

#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplyCondition {
    General,
    Fees,
}
impl ReplyCondition {
    fn key(self) -> &'static str {
        match self {
            Self::General => "general",
            Self::Fees => "fees",
        }
    }
}
#[derive(Clone, Copy, Debug, Deserialize, Serialize, PartialEq, Eq)]
#[serde(rename_all = "snake_case")]
pub enum ReplyStyle {
    Brief,
    Detailed,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub enum MemoryChange {
    Set {
        condition: ReplyCondition,
        style: ReplyStyle,
    },
    Stop,
}
#[derive(Clone, Debug, Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub struct MemoryCommand {
    pub operation_id: Uuid,
    pub expected_version: i64,
    pub change: MemoryChange,
}
/// Built by a verified persisted-message or local session adapter, never JSON.
pub struct MemoryEvidence {
    message_ref: Uuid,
    observed_at: DateTime<Utc>,
}
impl MemoryEvidence {
    pub(crate) fn trusted(message_ref: Uuid, observed_at: DateTime<Utc>) -> Self {
        Self {
            message_ref,
            observed_at,
        }
    }
}

impl Store {
    pub async fn remember(
        &self,
        actor: &Actor,
        command: &MemoryCommand,
        evidence: &MemoryEvidence,
    ) -> Result<Value> {
        let (mut tx, _, now) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        ensure!(
            evidence.observed_at <= now + Duration::seconds(5),
            "memory_source_time_invalid"
        );
        let request_hash = super::super::digest(&serde_json::to_vec(
            &json!({"change":command.change,"expected_version":command.expected_version,"message_ref":evidence.message_ref,"observed_at":evidence.observed_at}),
        )?);
        // Every Gateway for a Person serializes through the same durable authority.
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!("person-memory/{}", actor.person))
            .execute(&mut *tx)
            .await?;
        let previous=sqlx::query("SELECT person_id,request_hash,result FROM qintopia_identity.person_memory_commands WHERE operation_id=$1 OR (person_id=$2 AND message_ref=$3)")
            .bind(command.operation_id).bind(actor.person).bind(evidence.message_ref).fetch_optional(&mut *tx).await?;
        if let Some(previous) = previous {
            ensure!(
                previous.get::<Uuid, _>("person_id") == actor.person
                    && previous.get::<String, _>("request_hash") == request_hash,
                "memory_idempotency_conflict"
            );
            let mut result: Value = previous.get("result");
            result["replayed"] = json!(true);
            // The original receipt is historical, never a current-use permit.
            result["current_state_requires_read"] = json!(true);
            return Ok(result);
        }
        sqlx::query("INSERT INTO qintopia_identity.person_memory_state(person_id) VALUES($1) ON CONFLICT DO NOTHING")
            .bind(actor.person).execute(&mut *tx).await?;
        let state=sqlx::query("SELECT version,status,observed_through FROM qintopia_identity.person_memory_state WHERE person_id=$1 FOR UPDATE")
            .bind(actor.person).fetch_one(&mut *tx).await?;
        let version: i64 = state.get("version");
        ensure!(
            version == command.expected_version,
            "memory_version_conflict"
        );
        let observed: Option<DateTime<Utc>> = state.get("observed_through");
        ensure!(
            observed.is_none_or(|old| evidence.observed_at > old),
            "memory_stale_source"
        );
        let next = version + 1;
        let stopped = matches!(command.change, MemoryChange::Stop);
        match &command.change {
            MemoryChange::Set { condition, style } => {
                sqlx::query("UPDATE qintopia_identity.member_facts SET fact_status='superseded',updated_at=$3 WHERE person_id=$1 AND fact_type='reply_preference' AND fact_condition=$2 AND fact_status='active'")
                    .bind(actor.person).bind(condition.key()).bind(now).execute(&mut *tx).await?;
                sqlx::query("INSERT INTO qintopia_identity.member_facts(person_id,fact_type,fact_key,fact_text,evidence_type,evidence_ref_id,evidence_ref_table,observed_at,valid_from,fact_value,fact_condition,fact_version,fact_status,use_purpose,use_audience,metadata) VALUES($1,'reply_preference','reply_style',$2,'self_statement',$3,'trusted_memory_message',$4,$5,$6,$7,$8,'active','personal_reply','self',jsonb_build_object('source_link_id',$9::text,'authority','person_memory_v1'))")
                    .bind(actor.person).bind(match style {ReplyStyle::Brief=>"本人偏好简短回复",ReplyStyle::Detailed=>"本人偏好详细回复"})
                    .bind(evidence.message_ref).bind(evidence.observed_at).bind(now).bind(json!({"style":style})).bind(condition.key()).bind(next).bind(actor.link.to_string())
                    .execute(&mut *tx).await?;
            }
            MemoryChange::Stop => {
                sqlx::query("UPDATE qintopia_identity.member_facts SET fact_status='stopped',revoked_at=$2,updated_at=$2 WHERE person_id=$1 AND fact_type='reply_preference' AND fact_status='active'")
                    .bind(actor.person).bind(now).execute(&mut *tx).await?;
            }
        }
        sqlx::query("UPDATE qintopia_identity.person_memory_state SET version=$2,status=$3,observed_through=$4,updated_at=$5 WHERE person_id=$1")
            .bind(actor.person).bind(next).bind(if stopped {"stopped"} else {"active"}).bind(evidence.observed_at).bind(now).execute(&mut *tx).await?;
        // Derived old snapshots are not a second authority and cannot restore use.
        sqlx::query("UPDATE qintopia_identity.member_profile_snapshots SET status='superseded' WHERE person_id=$1 AND profile_kind='reply_context' AND status='active'")
            .bind(actor.person).execute(&mut *tx).await?;
        let result = json!({"saved":true,"version":next,"status":if stopped {"stopped"} else {"active"},"replayed":false,"human_task_created":false});
        sqlx::query("INSERT INTO qintopia_identity.person_memory_commands(operation_id,person_id,source_link_id,message_ref,observed_at,request_hash,result) VALUES($1,$2,$3,$4,$5,$6,$7)")
            .bind(command.operation_id).bind(actor.person).bind(actor.link).bind(evidence.message_ref).bind(evidence.observed_at).bind(request_hash).bind(&result).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(result)
    }

    pub async fn memory_context(&self, actor: &Actor, topic: &str) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        self.verify(&mut tx, actor).await?;
        let result = reply_context_connection(&mut tx, actor.person, topic).await?;
        tx.commit().await?;
        Ok(result)
    }
}

/// Only the shared service's explicit, typed self-reply facts can shape replies.
pub(crate) async fn reply_context(
    pool: &PgPool,
    person: Uuid,
    platform: &str,
    chat: &str,
    channel_user: &str,
) -> Result<Value> {
    let mut tx = pool.begin().await?;
    let bound:Option<Uuid>=sqlx::query_scalar("SELECT l.id FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.channel_identities ci ON ci.id=l.channel_identity_id AND ci.person_id=l.person_id JOIN qintopia_identity.persons p ON p.id=l.person_id JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type JOIN qintopia_agent_os.collaboration_scopes s ON s.id=g.scope_id AND s.tenant_key=g.tenant_key WHERE l.person_id=$1 AND l.status='confirmed' AND l.evidence_ref IS NOT NULL AND l.confirmed_by IS NOT NULL AND p.status='active' AND ci.platform=$2 AND ci.chat_id=$3 AND ci.channel_user_id=$4 AND g.active AND g.account_kind<>'shared' AND s.status='active' LIMIT 1 FOR SHARE OF l,ci,p,g,s")
        .bind(person).bind(platform).bind(chat).bind(channel_user).fetch_optional(&mut *tx).await?;
    if bound.is_none() {
        return Ok(json!({"version":0,"status":"identity_unconfirmed","reply_style":null}));
    }
    let value = reply_context_connection(&mut tx, person, "general").await?;
    tx.commit().await?;
    Ok(value)
}
async fn reply_context_connection(
    tx: &mut sqlx::Transaction<'_, sqlx::Postgres>,
    person: Uuid,
    topic: &str,
) -> Result<Value> {
    let row=sqlx::query("SELECT version,status FROM qintopia_identity.person_memory_state WHERE person_id=$1 FOR SHARE")
        .bind(person).fetch_optional(&mut **tx).await?;
    let Some(row) = row else {
        return Ok(
            json!({"version":0,"status":"unknown","reply_style":null,"current_conditions":{}}),
        );
    };
    let status: String = row.get("status");
    let mut conditions = serde_json::Map::new();
    if status == "active" {
        let facts=sqlx::query("SELECT fact_condition,fact_value FROM qintopia_identity.member_facts WHERE person_id=$1 AND fact_type='reply_preference' AND fact_status='active' AND revoked_at IS NULL AND use_purpose='personal_reply' AND use_audience='self' AND (expires_at IS NULL OR expires_at>clock_timestamp())")
            .bind(person).fetch_all(&mut **tx).await?;
        for fact in facts {
            let value: Value = fact.get("fact_value");
            conditions.insert(fact.get("fact_condition"), value["style"].clone());
        }
    }
    let style = conditions
        .get(if topic == "fees" { "fees" } else { "general" })
        .or_else(|| conditions.get("general"))
        .cloned()
        .unwrap_or(Value::Null);
    Ok(
        json!({"version":row.get::<i64,_>("version"),"status":status,"reply_style":style,"current_conditions":conditions,"purpose":"personal_reply","audience":"self"}),
    )
}
