//! Trusted adapter boundaries. No external sends or arbitrary Base writes.
use super::store::{audit, Store};
use anyhow::{ensure, Result};
use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

#[derive(Serialize, Deserialize)]
pub struct ApplicationRevision {
    pub source: String,
    pub resource: String,
    pub record: String,
    pub person: Option<Uuid>,
    pub revision: i64,
    pub valid: bool,
    pub consent_version: i64,
    pub consent_active: bool,
    pub field_hash: String,
}
impl Store {
    pub async fn application_from_readback(
        &self,
        app: &ApplicationRevision,
        allowed_resource: &str,
    ) -> Result<Uuid> {
        ensure!(
            app.resource == allowed_resource && app.revision > 0 && app.field_hash.len() == 64,
            "application_scope_or_revision"
        );
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT pg_advisory_xact_lock(hashtextextended($1,0))")
            .bind(format!(
                "application/{}/{}/{}",
                app.source, app.resource, app.record
            ))
            .execute(&mut *tx)
            .await?;
        let existing=sqlx::query("SELECT id,revision,field_hash FROM qintopia_agent_os.welcome_applications WHERE source_instance=$1 AND resource_ref=$2 AND record_ref=$3 FOR UPDATE")
            .bind(&app.source).bind(&app.resource).bind(&app.record).fetch_optional(&mut *tx).await?;
        if let Some(row) = existing {
            let revision = row.get::<i64, _>("revision");
            if app.revision < revision {
                return Ok(row.get("id"));
            }
            if app.revision == revision {
                ensure!(
                    row.get::<String, _>("field_hash") == app.field_hash,
                    "application_revision_conflict"
                );
                return Ok(row.get("id"));
            }
        }
        let id:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_applications(source_instance,resource_ref,record_ref,person_id,revision,valid,consent_version,consent_active,field_hash) VALUES ($1,$2,$3,$4,$5,$6,$7,$8,$9) ON CONFLICT (source_instance,resource_ref,record_ref) DO UPDATE SET person_id=EXCLUDED.person_id,revision=EXCLUDED.revision,valid=EXCLUDED.valid,consent_version=EXCLUDED.consent_version,consent_active=EXCLUDED.consent_active,field_hash=EXCLUDED.field_hash RETURNING id")
            .bind(&app.source).bind(&app.resource).bind(&app.record).bind(app.person).bind(app.revision).bind(app.valid).bind(app.consent_version).bind(app.consent_active).bind(&app.field_hash).fetch_one(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.welcome_artifact_bindings SET revoked_at=now() WHERE application_id=$1 AND application_revision<>$2")
            .bind(id).bind(app.revision).execute(&mut *tx).await?;
        let cases: Vec<Uuid> = sqlx::query_scalar(
            "SELECT id FROM qintopia_agent_os.welcome_cases WHERE application_id=$1",
        )
        .bind(id)
        .fetch_all(&mut *tx)
        .await?;
        for case in &cases {
            super::state::evaluate_tx(&mut tx, *case).await?;
        }
        audit(
            &mut tx,
            None,
            "welcome_application_observed",
            None,
            json!({"application_ref":id,"revision":app.revision}),
        )
        .await?;
        tx.commit().await?;
        for case in cases {
            self.reconcile_case(case).await?;
        }
        Ok(id)
    }

    /// Call only after authenticated complete roster pagination. Incomplete or
    /// failed pages make NO membership changes, including no mass departure.
    /// The callback inviter/sender id is intentionally not an input.
    pub async fn reconcile_members(
        &self,
        target: Uuid,
        namespace: &str,
        members: &[String],
        complete: bool,
        observed: DateTime<Utc>,
    ) -> Result<usize> {
        ensure!(complete, "incomplete_roster");
        ensure!(
            members.len() <= 10000 && observed <= Utc::now() + chrono::Duration::seconds(5),
            "invalid_roster"
        );
        let mut tx = self.pool.begin().await?;
        let expected: String = sqlx::query_scalar(
            "SELECT namespace FROM qintopia_agent_os.welcome_targets WHERE id=$1 FOR UPDATE",
        )
        .bind(target)
        .fetch_one(&mut *tx)
        .await?;
        ensure!(expected == namespace, "roster_namespace_mismatch");
        let last: Option<DateTime<Utc>> = sqlx::query_scalar(
            "SELECT max(observed_at) FROM qintopia_agent_os.welcome_members WHERE target_id=$1",
        )
        .bind(target)
        .fetch_one(&mut *tx)
        .await?;
        if last.is_some_and(|last| observed < last) {
            return Ok(0);
        }
        let mut links = vec![];
        for member in members {
            ensure!(super::protocol::reference(member), "invalid_roster_member");
            let link:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref) VALUES ($1,'qiwe_sender',$2) ON CONFLICT (namespace,subject_type,source_ref) DO UPDATE SET source_ref=EXCLUDED.source_ref RETURNING id")
                .bind(namespace).bind(member).fetch_one(&mut *tx).await?;
            links.push(link);
            sqlx::query("INSERT INTO qintopia_agent_os.welcome_members(target_id,identity_link_id,current,observed_at) VALUES ($1,$2,true,$3) ON CONFLICT (target_id,identity_link_id) DO UPDATE SET episode=qintopia_agent_os.welcome_members.episode+CASE WHEN qintopia_agent_os.welcome_members.current THEN 0 ELSE 1 END,current=true,observed_at=EXCLUDED.observed_at,version=qintopia_agent_os.welcome_members.version+1")
                .bind(target).bind(link).bind(observed).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE qintopia_agent_os.welcome_members SET current=false,observed_at=$3,version=version+1 WHERE target_id=$1 AND NOT identity_link_id=ANY($2) AND observed_at<=$3")
            .bind(target).bind(&links).bind(observed).execute(&mut *tx).await?;
        audit(
            &mut tx,
            None,
            "welcome_roster_reconciled",
            None,
            json!({"target_ref":target,"member_count":links.len()}),
        )
        .await?;
        tx.commit().await?;
        let scope = sqlx::query(
            "SELECT source_instance,property_id FROM qintopia_agent_os.welcome_targets WHERE id=$1",
        )
        .bind(target)
        .fetch_one(&self.pool)
        .await?;
        self.reconcile_scope(
            &scope.get::<String, _>("source_instance"),
            &scope.get::<String, _>("property_id"),
        )
        .await?;
        Ok(links.len())
    }
}

/// Attachment/internal mirror callbacks never start generation. Other changes are
/// readback hints, not trusted application revisions or identity assignments.
pub fn application_callback_requires_readback(changed_fields: &[String]) -> bool {
    changed_fields.is_empty()
        || changed_fields
            .iter()
            .any(|s| !matches!(s.as_str(), "欢迎卡片" | "agent_os_sync_revision"))
}

pub fn candidate_view(
    person: Uuid,
    preferred_name: &str,
    scope_label: &str,
    link_status: &str,
) -> Value {
    json!({"person_ref":person,"preferred_name":preferred_name,"scope_label":scope_label,"link_status":link_status})
}
