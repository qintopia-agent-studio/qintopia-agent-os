//! Communication selection belongs to a work audience, independent of PMS configuration.
use super::*;
use std::collections::BTreeSet;

impl Store {
    pub(super) async fn validate_audience_contacts(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        p: &Policy,
        scope: Uuid,
        audience: &Audience,
        now: DateTime<Utc>,
    ) -> Result<Value> {
        ensure!(audience.contacts.len() <= 20, "too_many_contacts");
        let mut selected = BTreeSet::new();
        let mut result = Vec::new();
        // Resolve people using the same authorized audience as candidate discovery.
        let resolved = if audience.contacts.iter().any(|c| c.subject_kind == "person") {
            Some(self.resolve_audience(tx, p, scope, audience, now).await?)
        } else {
            None
        };
        for contact in &audience.contacts {
            ensure!(
                selected.insert(contact.channel_source_link_id),
                "duplicate_contact"
            );
            ensure!(
                matches!(contact.subject_kind.as_str(), "person" | "work_account"),
                "invalid_contact"
            );
            if self.is_live() {
                // Existing fixed lock capabilities; no new grants or migration.
                let locked: bool = sqlx::query_scalar(
                    "SELECT qintopia_identity.management_ui_lock_identity_candidate($1,$2)",
                )
                .bind(&self.tenant)
                .bind(contact.channel_source_link_id)
                .fetch_one(&mut **tx)
                .await?;
                ensure!(locked, "contact_source_unavailable");
            }
            if self.is_live() {
                if contact.subject_kind == "person" {
                    let link: Option<Uuid> = sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed' AND evidence_ref IS NOT NULL ORDER BY id LIMIT 1")
                        .bind(&self.identity_namespace).bind(contact.subject_id).fetch_optional(&mut **tx).await?;
                    let link = link.ok_or_else(|| anyhow::anyhow!("contact_source_unavailable"))?;
                    let locked: bool = sqlx::query_scalar(
                        "SELECT qintopia_identity.management_ui_lock_actor($1,$2)",
                    )
                    .bind(&self.tenant)
                    .bind(link)
                    .fetch_one(&mut **tx)
                    .await?;
                    ensure!(locked, "contact_source_unavailable");
                } else {
                    let locked: bool = sqlx::query_scalar(
                        "SELECT qintopia_identity.management_ui_lock_business_account($1,$2)",
                    )
                    .bind(&self.tenant)
                    .bind(contact.subject_id)
                    .fetch_one(&mut **tx)
                    .await?;
                    ensure!(locked, "contact_source_unavailable");
                }
            }
            let rows = sqlx::query("SELECT l.id,l.person_id,l.status,l.evidence_ref,l.confirmed_by,l.adapter_metadata,l.version AS source_version,g.gateway_key,g.scope_id,g.version AS gateway_version,g.account_kind,g.subject_type,p.status AS person_status,coalesce(nullif(p.primary_name,''),p.display_name,w.label) AS subject_label,coalesce(nullif(l.adapter_metadata->>'display_name',''),nullif(l.adapter_metadata->>'nickname',''),p.preferred_name,p.display_name,w.label) AS channel_label,w.id AS account_id,w.version AS account_version,w.active AS account_active,w.source_version AS account_source_version,w.gateway_version AS account_gateway_version FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=$1 AND g.namespace=l.namespace AND g.subject_type=l.subject_type JOIN qintopia_agent_os.collaboration_scopes s ON s.tenant_key=g.tenant_key AND s.id=g.scope_id AND s.status='active' LEFT JOIN qintopia_identity.persons p ON p.id=l.person_id LEFT JOIN qintopia_identity.work_accounts w ON w.tenant_key=g.tenant_key AND w.source_link_id=l.id AND w.gateway_key=g.gateway_key WHERE l.id=$2 AND g.active AND (SELECT count(*) FROM qintopia_identity.person_identity_gateways x WHERE x.namespace=g.namespace AND x.subject_type=g.subject_type AND x.active)=1")
                .bind(&self.tenant).bind(contact.channel_source_link_id).fetch_all(&mut **tx).await?;
            ensure!(rows.len() == 1, "contact_source_unavailable");
            let row = &rows[0];
            ensure!(
                p.in_scope(scope, row.get("scope_id"), true),
                "contact_outside_scope"
            );
            let source_version: i64 = row.get("source_version");
            let gateway_version: i64 = row.get("gateway_version");
            let account_version = if contact.subject_kind == "person" {
                ensure!(
                    row.get::<Option<Uuid>, _>("person_id") == Some(contact.subject_id)
                        && row.get::<Option<String>, _>("person_status").as_deref()
                            == Some("active")
                        && row.get::<String, _>("account_kind") != "shared"
                        && row.get::<String, _>("status") == "confirmed"
                        && row.get::<Option<Uuid>, _>("evidence_ref").is_some(),
                    "contact_source_unavailable"
                );
                ensure!(
                    row.get::<Option<Uuid>, _>("confirmed_by").is_some()
                        || self
                            .work_account_person_proof(
                                tx,
                                contact.channel_source_link_id,
                                contact.subject_id
                            )
                            .await?,
                    "contact_source_unavailable"
                );
                self.known_person(tx, contact.subject_id).await?;
                ensure!(
                    resolved.as_ref().is_some_and(|r| r
                        .people
                        .iter()
                        .any(|person| person["person_ref"] == json!(contact.subject_id)
                            && person["selected"] == true)),
                    "contact_outside_audience"
                );

                None
            } else {
                ensure!(
                    row.get::<Option<Uuid>, _>("account_id") == Some(contact.subject_id)
                        && row.get::<Option<Uuid>, _>("person_id").is_none()
                        && row.get::<String, _>("account_kind") == "shared"
                        && row.get::<Option<bool>, _>("account_active") == Some(true)
                        && row.get::<String, _>("status") != "revoked"
                        && row
                            .get::<Value, _>("adapter_metadata")
                            .get("first_observation_ref")
                            .is_some()
                        && row.get::<Option<i64>, _>("account_source_version")
                            == Some(source_version)
                        && row.get::<Option<i64>, _>("account_gateway_version")
                            == Some(gateway_version),
                    "contact_source_unavailable"
                );

                row.get::<Option<i64>, _>("account_version")
            };
            let platform: String = row.get("subject_type");
            result.push(json!({"subject_kind":contact.subject_kind,"subject_id":contact.subject_id,
                "channel_source_link_id":contact.channel_source_link_id,"subject_label":row.get::<String,_>("subject_label"),
                "channel_label":row.get::<String,_>("channel_label"),"platform":if platform=="qiwe_sender"{"qiwe"}else if platform.starts_with("wecom_"){"wecom"}else{platform.as_str()},
                "source_version":source_version,"gateway_version":gateway_version,"account_version":account_version}));
        }
        Ok(json!(result))
    }

    pub(super) async fn audience_contacts_current(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        p: &Policy,
        scope: Uuid,
        audience: &Audience,
        configuration: &Value,
        now: DateTime<Utc>,
    ) -> Result<bool> {
        if audience.contacts.is_empty() {
            return Ok(true);
        }
        match self
            .validate_audience_contacts(tx, p, scope, audience, now)
            .await
        {
            Ok(current) => Ok(configuration["contact_basis"] == current),
            Err(error) if error.downcast_ref::<sqlx::Error>().is_some() => {
                Err(error.context("contact_source_unavailable"))
            }
            Err(_) => Ok(false),
        }
    }
}
