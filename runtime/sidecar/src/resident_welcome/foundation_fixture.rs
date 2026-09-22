//! Explicit synthetic setup for the combined local UI; never runs on startup by default.
use super::{
    channels::ApplicationRevision,
    digest,
    state::{Occupant, Snapshot, StayState},
    store::Store,
};
use anyhow::{ensure, Result};
use chrono::Utc;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

impl Store {
    pub async fn bootstrap_foundation_fixture(&self, tenant: &str) -> Result<Value> {
        ensure!(
            tenant.starts_with("synthetic-collaboration-"),
            "synthetic_tenant_required"
        );
        let mode:String=sqlx::query_scalar("SELECT mode FROM qintopia_agent_os.collaboration_tenants WHERE tenant_key=$1 AND initialized")
            .bind(tenant).fetch_one(&self.pool).await?;
        ensure!(mode == "synthetic", "synthetic_tenant_required");
        for agent in ["anan", "huabaosi", "erhua"] {
            sqlx::query("INSERT INTO qintopia_agent_os.collaboration_local_executors(tenant_key,agent_key,available) VALUES($1,$2,true) ON CONFLICT DO NOTHING").bind(tenant).bind(agent).execute(&self.pool).await?;
        }
        let source = format!("synthetic-welcome-{}", &digest(tenant.as_bytes())[..24]);
        let existing:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1)").bind(&source).fetch_one(&self.pool).await?;
        if existing {
            return self.foundation_fixture_refs(&source).await;
        }
        let property = "fixture-property";
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_sources(source_instance,property_id,mode,rebuilding,enabled,admission_after,execution_epoch) VALUES ($1,$2,'synthetic',false,true,now()-interval '1 day',1)")
            .bind(&source).bind(property).execute(&self.pool).await?;
        let resident:Uuid=sqlx::query_scalar("SELECT person_id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND source_ref='fixture-person-3' AND status='confirmed'").bind(tenant).fetch_one(&self.pool).await?;
        let namespace = format!("{source}/qiwe");
        let member:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by) VALUES ($1,'qiwe_sender','synthetic-resident',$2,'confirmed',$3,$2) RETURNING id")
            .bind(&namespace).bind(resident).bind(Uuid::new_v4()).fetch_one(&self.pool).await?;
        for (index, building) in ["一栋", "二栋"].iter().enumerate() {
            let scope:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND label=$2 AND status='active'").bind(tenant).bind(building).fetch_one(&self.pool).await?;
            let target:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_targets(source_instance,property_id,kind,building_code,display_name,namespace,conversation_ref,enabled) VALUES($1,$2,'building',$3,$4,$5,$6,true) RETURNING id")
                .bind(&source).bind(property).bind(building).bind(format!("{building}居民群（合成）")).bind(&namespace).bind(format!("synthetic-building-{index}")).fetch_one(&self.pool).await?;
            sqlx::query("INSERT INTO qintopia_agent_os.welcome_foundation_targets(target_id,tenant_key,scope_id) VALUES($1,$2,$3)").bind(target).bind(tenant).bind(scope).execute(&self.pool).await?;
            sqlx::query("INSERT INTO qintopia_agent_os.welcome_members(target_id,identity_link_id,current,observed_at) VALUES($1,$2,true,clock_timestamp())").bind(target).bind(member).execute(&self.pool).await?;
            let occupant = format!("synthetic-occupant-{index}");
            let snapshot = Snapshot {
                source_hash: None,
                source: source.clone(),
                property: property.into(),
                order: format!("synthetic-order-{index}"),
                revision: "1".into(),
                stay: format!("synthetic-stay-{index}"),
                state: StayState::InHouse,
                inventory_reserved: true,
                current_arrangement: true,
                building: (*building).into(),
                occupants: vec![Occupant {
                    id: occupant.clone(),
                    active: true,
                }],
                related_revisions: Default::default(),
                observed_at: Utc::now(),
                business_date: Utc::now().date_naive(),
            };
            let case = self.apply_snapshot(None, &snapshot).await?[0];
            let link:Uuid=sqlx::query_scalar("UPDATE qintopia_identity.source_identity_links SET person_id=$3,status='confirmed',evidence_ref=$4,confirmed_by=$3 WHERE namespace=$1 AND subject_type='pms_occupant' AND source_ref=$2 RETURNING id")
                .bind(format!("pms/{source}/{property}/occupant")).bind(&occupant).bind(resident).bind(Uuid::new_v4()).fetch_one(&self.pool).await?;
            let application = self
                .application_from_readback(
                    &ApplicationRevision {
                        source: source.clone(),
                        resource: "synthetic-application-base".into(),
                        record: format!("synthetic-application-{index}"),
                        person: Some(resident),
                        revision: 1,
                        valid: true,
                        consent_version: 1,
                        consent_active: true,
                        field_hash: digest(format!("synthetic-fields-{index}").as_bytes()),
                    },
                    "synthetic-application-base",
                )
                .await?;
            sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET person_id=$2,identity_link_id=$3,identity_version=1,application_id=$4,admitted=true WHERE id=$1")
                .bind(case).bind(resident).bind(link).bind(application).execute(&self.pool).await?;
            self.evaluate(case).await?;
        }
        self.foundation_fixture_refs(&source).await
    }

    async fn foundation_fixture_refs(&self, source: &str) -> Result<Value> {
        let rows=sqlx::query("SELECT t.id AS target,f.scope_id,c.id AS case_ref,t.building_code,c.person_id FROM qintopia_agent_os.welcome_foundation_targets f JOIN qintopia_agent_os.welcome_targets t ON t.id=f.target_id JOIN qintopia_agent_os.welcome_cases c ON c.source_instance=t.source_instance AND c.property_id=t.property_id JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id AND v.projection->>'building'=t.building_code WHERE t.source_instance=$1 ORDER BY t.building_code")
            .bind(source).fetch_all(&self.pool).await?;
        Ok(
            json!({"source":source,"targets":rows.iter().map(|r|json!({"target_ref":r.get::<Uuid,_>("target"),"scope_ref":r.get::<Uuid,_>("scope_id"),"case_ref":r.get::<Uuid,_>("case_ref"),"building":r.get::<String,_>("building_code"),"person_ref":r.get::<Uuid,_>("person_id")})).collect::<Vec<_>>()}),
        )
    }

    /// Read-only boundary simulation: refresh timestamps from the fixed synthetic
    /// source, with no admission or new event. Used only by explicit local demo.
    pub async fn refresh_foundation_fixture(&self, tenant: &str) -> Result<()> {
        ensure!(
            tenant.starts_with("synthetic-collaboration-") && tenant.len() <= 100,
            "synthetic_tenant_required"
        );
        let mut tx = self.pool.begin().await?;
        let synthetic:bool=sqlx::query_scalar("SELECT mode='synthetic' AND initialized AND identity_namespace=tenant_key FROM qintopia_agent_os.collaboration_tenants WHERE tenant_key=$1 FOR SHARE")
            .bind(tenant).fetch_optional(&mut *tx).await?.unwrap_or(false);
        ensure!(synthetic, "synthetic_tenant_required");
        let source = format!("synthetic-welcome-{}", &digest(tenant.as_bytes())[..24]);
        let targets = sqlx::query("SELECT t.source_instance,s.mode FROM qintopia_agent_os.welcome_foundation_targets f JOIN qintopia_agent_os.welcome_targets t ON t.id=f.target_id JOIN qintopia_agent_os.welcome_sources s ON s.source_instance=t.source_instance AND s.property_id=t.property_id WHERE f.tenant_key=$1 FOR SHARE OF f,t,s")
            .bind(tenant).fetch_all(&mut *tx).await?;
        ensure!(
            !targets.is_empty()
                && targets
                    .iter()
                    .all(|row| row.get::<String, _>("source_instance") == source
                        && row.get::<String, _>("mode") == "synthetic"),
            "synthetic_fixture_source_required"
        );
        sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions v SET projection=jsonb_set(v.projection,'{observed_at}',to_jsonb(clock_timestamp())) FROM qintopia_agent_os.welcome_sources s WHERE v.source_instance=s.source_instance AND v.property_id=s.property_id AND v.source_instance=$2 AND s.mode='synthetic' AND EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_targets t JOIN qintopia_agent_os.welcome_foundation_targets f ON f.target_id=t.id WHERE t.source_instance=s.source_instance AND t.property_id=s.property_id AND f.tenant_key=$1)")
            .bind(tenant).bind(&source).execute(&mut *tx).await?;
        sqlx::query("UPDATE qintopia_agent_os.welcome_members m SET observed_at=clock_timestamp() FROM qintopia_agent_os.welcome_foundation_targets f JOIN qintopia_agent_os.welcome_targets t ON t.id=f.target_id JOIN qintopia_agent_os.welcome_sources s ON s.source_instance=t.source_instance AND s.property_id=t.property_id WHERE f.target_id=m.target_id AND f.tenant_key=$1 AND t.source_instance=$2 AND s.mode='synthetic'")
            .bind(tenant).bind(&source).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }
}

#[cfg(all(test, feature = "postgres-integration-tests"))]
mod tests {
    use super::*;
    use crate::person_collaboration::Store as People;

    async fn snapshot(store: &Store, source: &str, omit_observed: bool) -> Result<Value> {
        Ok(sqlx::query_scalar("SELECT jsonb_build_object('orders',(SELECT jsonb_agg(CASE WHEN $2 THEN (to_jsonb(v)-'projection')||jsonb_build_object('projection',v.projection-'observed_at') ELSE to_jsonb(v) END ORDER BY v.aggregate_id) FROM qintopia_agent_os.welcome_source_versions v WHERE v.source_instance=$1),'members',(SELECT jsonb_agg(CASE WHEN $2 THEN to_jsonb(m)-'observed_at' ELSE to_jsonb(m) END ORDER BY m.target_id,m.identity_link_id) FROM qintopia_agent_os.welcome_members m JOIN qintopia_agent_os.welcome_targets t ON t.id=m.target_id WHERE t.source_instance=$1),'cases',(SELECT jsonb_agg(to_jsonb(c) ORDER BY c.id) FROM qintopia_agent_os.welcome_cases c WHERE c.source_instance=$1),'events',(SELECT count(*) FROM qintopia_agent_os.welcome_inbox WHERE source_instance=$1),'actions',(SELECT count(*) FROM qintopia_agent_os.welcome_actions a JOIN qintopia_agent_os.welcome_cases c ON c.id=a.case_id WHERE c.source_instance=$1))")
            .bind(source).bind(omit_observed).fetch_one(&store.pool).await?)
    }

    #[tokio::test]
    #[ignore = "explicit isolated database required"]
    async fn foundation_fixture_observation_preserves_state_and_rejects_mixed_sources() -> Result<()>
    {
        let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
        let tenant = format!("synthetic-collaboration-observe-{}", Uuid::new_v4());
        let people = People::local(&database, &tenant).await?;
        let welcome = Store::local(&database).await?;
        crate::db::run_migrations(&welcome.pool).await?;
        assert!(welcome.refresh_foundation_fixture(&tenant).await.is_err());
        people.bootstrap_fixture().await?;
        assert!(welcome.refresh_foundation_fixture(&tenant).await.is_err());
        let seed = welcome.bootstrap_foundation_fixture(&tenant).await?;
        let source = seed["source"].as_str().unwrap();
        sqlx::query("UPDATE qintopia_agent_os.welcome_members m SET current=false,observed_at=clock_timestamp()-interval '2 minutes' FROM qintopia_agent_os.welcome_targets t WHERE t.id=m.target_id AND t.source_instance=$1")
            .bind(source).execute(&welcome.pool).await?;
        sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET projection=jsonb_set(jsonb_set(projection,'{inventory_reserved}','false'),'{observed_at}',to_jsonb(clock_timestamp()-interval '2 minutes')) WHERE source_instance=$1")
            .bind(source).execute(&welcome.pool).await?;
        let before = snapshot(&welcome, source, true).await?;
        welcome.refresh_foundation_fixture(&tenant).await?;
        assert_eq!(snapshot(&welcome, source, true).await?, before);
        let fresh:bool=sqlx::query_scalar("SELECT (SELECT bool_and((projection->>'observed_at')::timestamptz > clock_timestamp()-interval '5 seconds') FROM qintopia_agent_os.welcome_source_versions WHERE source_instance=$1) AND (SELECT bool_and(m.observed_at > clock_timestamp()-interval '5 seconds' AND NOT m.current) FROM qintopia_agent_os.welcome_members m JOIN qintopia_agent_os.welcome_targets t ON t.id=m.target_id WHERE t.source_instance=$1)")
            .bind(source).fetch_one(&welcome.pool).await?;
        assert!(
            fresh,
            "refresh only reobserves, preserving withdrawn members and orders"
        );
        let other_source = format!("synthetic-other-{}", Uuid::new_v4());
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_sources(source_instance,property_id,mode) VALUES($1,'fixture-property','shadow')")
            .bind(&other_source).execute(&welcome.pool).await?;
        let target:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_targets(source_instance,property_id,kind,display_name,namespace,conversation_ref) VALUES($1,'fixture-property','community','Unexpected fixture source','synthetic','unexpected') RETURNING id")
            .bind(&other_source).fetch_one(&welcome.pool).await?;
        let scope: Uuid = serde_json::from_value(seed["targets"][0]["scope_ref"].clone())?;
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_foundation_targets(target_id,tenant_key,scope_id) VALUES($1,$2,$3)")
            .bind(target).bind(&tenant).bind(scope).execute(&welcome.pool).await?;
        let before = snapshot(&welcome, source, false).await?;
        assert!(welcome.refresh_foundation_fixture(&tenant).await.is_err());
        assert_eq!(snapshot(&welcome, source, false).await?, before);
        sqlx::query("UPDATE qintopia_agent_os.welcome_sources SET mode='synthetic' WHERE source_instance=$1")
            .bind(&other_source).execute(&welcome.pool).await?;
        assert!(welcome.refresh_foundation_fixture(&tenant).await.is_err());
        assert_eq!(snapshot(&welcome, source, false).await?, before);
        assert!(welcome
            .refresh_foundation_fixture("real-tenant")
            .await
            .is_err());
        Ok(())
    }
}
