//! Source readback projection and work dispatch, not a person/approval service.
use super::Store;
use anyhow::{ensure, Result};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use uuid::Uuid;

#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Observation {
    pub identity_hash: String,
    pub field_hash: String,
    pub valid: bool,
    pub consent_active: bool,
    pub source_version: Option<String>,
}
impl Observation {
    fn validate(&self) -> Result<()> {
        ensure!(
            [&self.identity_hash, &self.field_hash]
                .iter()
                .all(|s| s.len() == 64
                    && s.bytes()
                        .all(|b| b.is_ascii_digit() || (b'a'..=b'f').contains(&b))),
            "invalid_application_hash"
        );
        ensure!(
            self.source_version.as_ref().is_none_or(|s| !s.is_empty()
                && s.len() <= 40
                && s.bytes().all(|b| b.is_ascii_digit())),
            "invalid_application_source_version"
        );
        Ok(())
    }
}

fn references(alias: &str, record: &str) -> Result<()> {
    ensure!(
        !alias.is_empty()
            && alias.len() <= 80
            && alias
                .bytes()
                .all(|b| b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'_' | b'-'))
            && record.starts_with("rec")
            && record.len() > 3
            && record.len() <= 103
            && record.bytes().all(|b| b.is_ascii_alphanumeric()),
        "invalid_application_reference"
    );
    Ok(())
}

async fn binding(
    tx: &mut Transaction<'_, Postgres>,
    tenant: &str,
    id: Uuid,
) -> Result<(String, String, i64, Uuid)> {
    let row=sqlx::query("SELECT b.source_instance,b.property_id,b.version,b.scope_id FROM qintopia_agent_os.business_property_bindings b JOIN qintopia_agent_os.collaboration_scopes s ON s.tenant_key=b.tenant_key AND s.id=b.scope_id WHERE b.tenant_key=$1 AND b.id=$2 AND b.active AND s.status='active' FOR SHARE OF b,s")
        .bind(tenant).bind(id).fetch_optional(&mut **tx).await?.ok_or_else(||anyhow::anyhow!("business_binding_unavailable"))?;
    Ok((
        row.get("source_instance"),
        row.get("property_id"),
        row.get("version"),
        row.get("scope_id"),
    ))
}

impl Store {
    /// Trusted source identity evidence for the shared welcome snapshot. This
    /// joins current ownership and recomputes the saved readback digest, so a
    /// different source writer cannot leave a stale identity hash looking current.
    pub(crate) async fn application_identity_basis(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: Uuid,
        application: Uuid,
    ) -> Result<Option<String>> {
        let row=sqlx::query("SELECT i.identity_hash,i.completed_hash,i.source_version,a.field_hash,a.valid,a.consent_active FROM qintopia_agent_os.application_intake_states i JOIN qintopia_agent_os.welcome_applications a ON a.id=i.application_id AND a.source_instance=i.source_instance AND a.resource_ref=i.resource_alias AND a.record_ref=i.record_ref JOIN qintopia_agent_os.business_property_bindings b ON b.tenant_key=i.tenant_key AND b.id=i.binding_id AND b.version=i.binding_version AND b.source_instance=i.source_instance AND b.property_id=i.property_id JOIN qintopia_agent_os.collaboration_scopes s ON s.tenant_key=b.tenant_key AND s.id=b.scope_id WHERE i.tenant_key=$1 AND b.scope_id=$2 AND i.application_id=$3 AND b.active AND s.status='active' AND i.completed_token IS NOT NULL AND i.identity_hash IS NOT NULL FOR SHARE OF i,a,b,s")
            .bind(&self.tenant).bind(scope).bind(application).fetch_optional(&mut **tx).await?;
        let Some(row) = row else { return Ok(None) };
        let observed = Observation {
            identity_hash: row.get("identity_hash"),
            field_hash: row.get("field_hash"),
            valid: row.get("valid"),
            consent_active: row.get("consent_active"),
            source_version: row.get("source_version"),
        };
        observed.validate()?;
        let hash = crate::person_collaboration::digest(&serde_json::to_vec(&observed)?);
        Ok(
            (row.get::<Option<String>, _>("completed_hash").as_deref() == Some(&hash))
                .then_some(observed.identity_hash),
        )
    }
    pub(crate) async fn application_read_open(
        &self,
        binding_id: Uuid,
        alias: &str,
        record: &str,
    ) -> Result<Value> {
        references(alias, record)?;
        let (mut tx, _, _) = self.begin().await?;
        let (source, property, version, _) = binding(&mut tx, &self.tenant, binding_id).await?;
        let token = Uuid::new_v4();
        sqlx::query("INSERT INTO qintopia_agent_os.application_intake_states(tenant_key,binding_id,binding_version,source_instance,property_id,resource_alias,record_ref,read_token) VALUES($1,$2,$3,$4,$5,$6,$7,$8) ON CONFLICT(tenant_key,resource_alias,record_ref) DO NOTHING")
            .bind(&self.tenant).bind(binding_id).bind(version).bind(&source).bind(&property).bind(alias).bind(record).bind(token).execute(&mut *tx).await?;
        let row=sqlx::query("SELECT s.*,coalesce(a.revision,0) AS revision FROM qintopia_agent_os.application_intake_states s LEFT JOIN qintopia_agent_os.welcome_applications a ON a.id=s.application_id WHERE s.tenant_key=$1 AND s.resource_alias=$2 AND s.record_ref=$3 FOR UPDATE OF s")
            .bind(&self.tenant).bind(alias).bind(record).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<Uuid, _>("binding_id") == binding_id
                && row.get::<i64, _>("binding_version") == version
                && row.get::<String, _>("source_instance") == source
                && row.get::<String, _>("property_id") == property,
            "application_binding_changed"
        );
        sqlx::query("UPDATE qintopia_agent_os.application_intake_states SET read_token=$2,expected_revision=$3,updated_at=clock_timestamp() WHERE id=$1")
            .bind(row.get::<Uuid,_>("id")).bind(token).bind(row.get::<i64,_>("revision")).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(json!({"read_token":token}))
    }

    pub(crate) async fn application_read_save(
        &self,
        binding_id: Uuid,
        alias: &str,
        record: &str,
        token: Uuid,
        observation: &Observation,
    ) -> Result<Value> {
        references(alias, record)?;
        observation.validate()?;
        let hash = crate::person_collaboration::digest(&serde_json::to_vec(observation)?);
        let (mut tx, _, _) = self.begin().await?;
        let (source, property, version, scope) = binding(&mut tx, &self.tenant, binding_id).await?;
        let state=sqlx::query("SELECT * FROM qintopia_agent_os.application_intake_states WHERE tenant_key=$1 AND resource_alias=$2 AND record_ref=$3 FOR UPDATE")
            .bind(&self.tenant).bind(alias).bind(record).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("application_read_token_required"))?;
        ensure!(
            state.get::<Uuid, _>("binding_id") == binding_id
                && state.get::<i64, _>("binding_version") == version
                && state.get::<String, _>("source_instance") == source
                && state.get::<String, _>("property_id") == property,
            "application_binding_changed"
        );
        if state.get::<Option<Uuid>, _>("completed_token") == Some(token) {
            ensure!(
                state.get::<Option<String>, _>("completed_hash").as_deref() == Some(&hash),
                "application_observation_conflict"
            );
            return Ok(
                json!({"status":"duplicate","application":state.get::<Option<Uuid>,_>("application_id")}),
            );
        }
        ensure!(
            state.get::<Uuid, _>("read_token") == token,
            "application_read_conflict"
        );
        let old = if let Some(id) = state.get::<Option<Uuid>, _>("application_id") {
            Some(
                sqlx::query(
                    "SELECT * FROM qintopia_agent_os.welcome_applications WHERE id=$1 FOR UPDATE",
                )
                .bind(id)
                .fetch_one(&mut *tx)
                .await?,
            )
        } else {
            None
        };
        let previous_revision = old.as_ref().map_or(0, |r| r.get::<i64, _>("revision"));
        ensure!(
            previous_revision == state.get::<i64, _>("expected_revision"),
            "application_read_conflict"
        );
        if let Some(row) = &old {
            ensure!(
                row.get::<String, _>("source_instance") == source
                    && row.get::<String, _>("resource_ref") == alias
                    && row.get::<String, _>("record_ref") == record,
                "application_source_mismatch"
            );
        }
        let changed = old.as_ref().is_none_or(|r| {
            r.get::<String, _>("field_hash") != observation.field_hash
                || r.get::<bool, _>("valid") != observation.valid
                || r.get::<bool, _>("consent_active") != observation.consent_active
                || state.get::<Option<String>, _>("identity_hash").as_deref()
                    != Some(&observation.identity_hash)
        });
        let identity_changed = old.is_some()
            && state.get::<Option<String>, _>("identity_hash").as_deref()
                != Some(&observation.identity_hash);
        let application = if let Some(row) = &old {
            row.get::<Uuid, _>("id")
        } else {
            // Existing unowned projections require explicit adoption, never inferred overwrite.
            let exists:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_applications WHERE source_instance=$1 AND resource_ref=$2 AND record_ref=$3)")
                .bind(&source).bind(alias).bind(record).fetch_one(&mut *tx).await?;
            ensure!(!exists, "application_projection_adoption_required");
            sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_applications(source_instance,resource_ref,record_ref,revision,valid,consent_version,consent_active,field_hash) VALUES($1,$2,$3,1,$4,1,$5,$6) RETURNING id")
                .bind(&source).bind(alias).bind(record).bind(observation.valid).bind(observation.consent_active).bind(&observation.field_hash).fetch_one(&mut *tx).await?
        };
        let revision = if changed {
            previous_revision + 1
        } else {
            previous_revision
        };
        if changed && old.is_some() {
            if identity_changed {
                crate::resident_welcome::store::audit(&mut tx,None,"application_identity_basis_changed",None,
                    json!({"application_ref":application,"previous_revision":previous_revision,
                        "previous_person":old.as_ref().and_then(|r|r.get::<Option<Uuid>,_>("person_id")),
                        "previous_identity_hash":state.get::<Option<String>,_>("identity_hash")})).await?;
            }
            sqlx::query("UPDATE qintopia_agent_os.welcome_applications SET revision=$2,valid=$3,consent_version=consent_version+CASE WHEN consent_active<>$4 THEN 1 ELSE 0 END,consent_active=$4,field_hash=$5,person_id=CASE WHEN $6 THEN NULL ELSE person_id END WHERE id=$1")
                .bind(application).bind(revision).bind(observation.valid).bind(observation.consent_active).bind(&observation.field_hash).bind(identity_changed).execute(&mut *tx).await?;
            sqlx::query("UPDATE qintopia_agent_os.welcome_artifact_bindings SET revoked_at=clock_timestamp() WHERE application_id=$1 AND revoked_at IS NULL")
                .bind(application).execute(&mut *tx).await?;
            let cases: Vec<Uuid> = sqlx::query_scalar(
                "SELECT id FROM qintopia_agent_os.welcome_cases WHERE application_id=$1 FOR UPDATE",
            )
            .bind(application)
            .fetch_all(&mut *tx)
            .await?;
            for case in cases {
                if identity_changed {
                    sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET application_id=NULL,version=version+1 WHERE id=$1")
                        .bind(case).execute(&mut *tx).await?;
                    crate::resident_welcome::store::audit(&mut tx,None,"application_stay_basis_invalidated",None,
                        json!({"case_ref":case,"application_ref":application,"previous_revision":previous_revision,
                            "previous_person":old.as_ref().and_then(|r|r.get::<Option<Uuid>,_>("person_id"))})).await?;
                }
                crate::resident_welcome::state::evaluate_tx(&mut tx, case).await?;
            }
        }
        let mut works = Vec::new();
        for (column, agent, kind, capability) in [
            ("anan_work_id", "anan", "business_operation", "anan.pms"),
            (
                "silaoshi_work_id",
                "silaoshi",
                "application_review",
                "silaoshi.application_review",
            ),
        ] {
            let work = if let Some(id) = state.get::<Option<Uuid>, _>(column) {
                id
            } else {
                let key = format!("application/{application}/{agent}");
                sqlx::query_scalar("INSERT INTO qintopia_agent_os.work_items(work_item_type,status,requester_agent,target_agent,capability_key,brief_summary,purpose,source_type,dedupe_key,idempotency_key,payload,metadata) VALUES($1,'awaiting_review','anan',$2,$3,'申请待核对','synthetic_application','application_readback',$4,$4,'{}','{\"local_only\":true,\"event_is_not_authority\":true}') RETURNING id")
                    .bind(kind).bind(agent).bind(capability).bind(key).fetch_one(&mut *tx).await?
            };
            if changed {
                // Terminal work remains terminal. Updates ask for review, never replay PMS actions.
                sqlx::query("UPDATE qintopia_agent_os.work_items SET payload=payload || $2,status=CASE WHEN status IN ('completed','cancelled','processing') THEN status WHEN NOT $3 THEN 'cancelled' ELSE 'awaiting_review' END,updated_at=clock_timestamp() WHERE id=$1")
                    .bind(work).bind(json!({"tenant_key":self.tenant,"binding":binding_id,"application_ref":application,
                        "application_revision":revision,"application_hash":observation.field_hash,"valid":observation.valid,
                        "identity_readback_required":true,"contact_status":"pending_verified_channel"})).bind(observation.valid).execute(&mut *tx).await?;
                super::foundation::work_event(&mut tx,work,"application_source_observed",agent,
                    &json!({"application_ref":application,"revision":revision,"identity_basis_changed":identity_changed,
                        "source_valid":observation.valid,"financial_authority":false})).await?;
            }
            works.push(work);
        }
        sqlx::query("UPDATE qintopia_agent_os.application_intake_states SET application_id=$2,identity_hash=$3,completed_token=$4,completed_hash=$5,source_version=$6,anan_work_id=$7,silaoshi_work_id=$8,updated_at=clock_timestamp() WHERE id=$1")
            .bind(state.get::<Uuid,_>("id")).bind(application).bind(&observation.identity_hash).bind(token).bind(hash)
            .bind(&observation.source_version).bind(works[0]).bind(works[1]).execute(&mut *tx).await?;
        ensure!(
            self.application_identity_basis(&mut tx, scope, application)
                .await?
                .as_deref()
                == Some(&observation.identity_hash),
            "application_source_mismatch"
        );
        tx.commit().await?;
        Ok(
            json!({"status":if changed {"accepted"}else{"duplicate"},"application":application,"revision":revision,"work_items":works}),
        )
    }
}
