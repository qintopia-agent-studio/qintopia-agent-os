//! Source handoff requires a persisted relation, never a candidate match.
use super::*;
use crate::person_collaboration::welcome_model::{ReviewGrant, ReviewSettings, SubjectRef};

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn application_welcome_reuses_exact_task_and_requires_current_relation() -> Result<()> {
    let f = Fixture::new().await?;
    let service = crate::resident_welcome::store::Store {
        pool: f.store.pool.clone(),
    };
    let seed = service
        .bootstrap_foundation_fixture(&f.store.tenant)
        .await?;
    let scope: Uuid = serde_json::from_value(seed["targets"][0]["scope_ref"].clone())?;
    let other_scope: Uuid = serde_json::from_value(seed["targets"][1]["scope_ref"].clone())?;
    let case: Uuid = serde_json::from_value(seed["targets"][0]["case_ref"].clone())?;
    sqlx::query("UPDATE qintopia_agent_os.business_property_bindings SET source_instance=$2,property_id='fixture-property',scope_id=$3 WHERE id=$1")
        .bind(f.binding).bind(seed["source"].as_str()).bind(scope).execute(&f.store.pool).await?;
    let record = record();
    let saved = save(&f, &record, &observation()).await?;
    let application: Uuid = serde_json::from_value(saved["application"].clone())?;
    let reconcile = || {
        f.store
            .application_reconcile_welcome(f.binding, "resident-application", &record)
    };
    assert_eq!(reconcile().await?["status"], "awaiting_reliable_stay_link");
    // Explicit fixture evidence models a relationship already confirmed by the
    // common service. Intake itself must never create this association.
    sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET application_id=$2 WHERE id=$1")
        .bind(case)
        .bind(application)
        .execute(&f.store.pool)
        .await?;
    assert_eq!(reconcile().await?["status"], "awaiting_reliable_stay_link");
    sqlx::query("UPDATE qintopia_agent_os.welcome_applications SET person_id=(SELECT person_id FROM qintopia_agent_os.welcome_cases WHERE id=$2) WHERE id=$1")
        .bind(application).bind(case).execute(&f.store.pool).await?;
    assert_eq!(
        reconcile().await?["status"],
        "awaiting_operations_configuration"
    );
    let owner_link: Uuid = sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND source_ref='fixture-person-0'")
        .bind(&f.store.tenant).fetch_one(&f.store.pool).await?;
    let owner = f.store.actor(owner_link).await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET managed_agents=array_append(managed_agents,'anan'),managed_domains=array_append(managed_domains,'hospitality') WHERE tenant_key=$1 AND parent_grant_id IS NULL")
        .bind(&f.store.tenant).execute(&f.store.pool).await?;
    let group: Uuid = sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type,display_name) VALUES($1,'wecom',$2,'group','模拟运营群') RETURNING id")
        .bind(&f.store.tenant).bind(Uuid::new_v4().to_string()).fetch_one(&f.store.pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3)")
        .bind(&f.store.tenant).bind(scope).bind(group).execute(&f.store.pool).await?;
    f.store
        .welcome_settings_save(
            &owner,
            &ReviewSettings {
                scope,
                conversation: group,
                expected_version: 0,
                reviewers: vec![ReviewGrant {
                    subject: SubjectRef::Person(f.store.verified_person(&owner).await?),
                    effects: vec!["identity".into(), "review".into()],
                    valid_until: Utc::now() + Duration::hours(1),
                }],
            },
        )
        .await?;
    let first = reconcile().await?;
    assert_eq!(first["status"], "awaiting_operations_confirmation");
    let work: Uuid = serde_json::from_value(first["work_items"][0]["work_item"].clone())?;
    let row = sqlx::query(
        "SELECT work_item_type,target_agent,payload FROM qintopia_agent_os.work_items WHERE id=$1",
    )
    .bind(work)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(row.get::<String, _>("work_item_type"), "welcome_event");
    assert_eq!(row.get::<String, _>("target_agent"), "anan");
    assert_eq!(
        row.get::<Value, _>("payload"),
        json!({"tenant_key":f.store.tenant,"scope_ref":scope,"case_ref":case,"application_ref":application})
    );
    sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET manual_hold=true WHERE id=$1")
        .bind(case)
        .execute(&f.store.pool)
        .await?;
    let again = reconcile().await?;
    assert_eq!(again["work_items"][0]["work_item"], json!(work));
    assert!(
        sqlx::query_scalar::<_, bool>(
            "SELECT manual_hold FROM qintopia_agent_os.welcome_cases WHERE id=$1"
        )
        .bind(case)
        .fetch_one(&f.store.pool)
        .await?
    );
    let count: i64 = sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.work_items WHERE payload->>'application_ref'=$1 AND work_item_type='welcome_event'")
        .bind(application.to_string()).fetch_one(&f.store.pool).await?;
    assert_eq!(count, 1);
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked' WHERE id=(SELECT identity_link_id FROM qintopia_agent_os.welcome_cases WHERE id=$1)")
        .bind(case).execute(&f.store.pool).await?;
    assert_eq!(reconcile().await?["status"], "awaiting_reliable_stay_link");
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='confirmed' WHERE id=(SELECT identity_link_id FROM qintopia_agent_os.welcome_cases WHERE id=$1)")
        .bind(case).execute(&f.store.pool).await?;
    // A conflicting persisted task cannot be repurposed by a later source wake.
    sqlx::query("UPDATE qintopia_agent_os.work_items SET payload=jsonb_set(payload,'{scope_ref}',to_jsonb($2::text)) WHERE id=$1")
        .bind(work).bind(other_scope.to_string()).execute(&f.store.pool).await?;
    assert!(reconcile()
        .await
        .unwrap_err()
        .to_string()
        .contains("application_welcome_work_conflict"));
    let persisted: String = sqlx::query_scalar(
        "SELECT payload->>'scope_ref' FROM qintopia_agent_os.work_items WHERE id=$1",
    )
    .bind(work)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(persisted, other_scope.to_string());
    sqlx::query("UPDATE qintopia_agent_os.work_items SET payload=jsonb_set(payload,'{scope_ref}',to_jsonb($2::text)) WHERE id=$1")
        .bind(work).bind(scope.to_string()).execute(&f.store.pool).await?;
    sqlx::query("UPDATE qintopia_agent_os.business_property_bindings SET scope_id=$2 WHERE id=$1")
        .bind(f.binding)
        .bind(other_scope)
        .execute(&f.store.pool)
        .await?;
    assert_eq!(reconcile().await?["status"], "awaiting_reliable_stay_link");
    sqlx::query("UPDATE qintopia_agent_os.business_property_bindings SET scope_id=$2 WHERE id=$1")
        .bind(f.binding)
        .bind(scope)
        .execute(&f.store.pool)
        .await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET invalidated=true WHERE source_instance=$1")
        .bind(seed["source"].as_str()).execute(&f.store.pool).await?;
    assert_eq!(reconcile().await?["status"], "awaiting_reliable_stay_link");
    let mut withdrawn = observation();
    withdrawn.valid = false;
    save(&f, &record, &withdrawn).await?;
    assert_eq!(reconcile().await?["status"], "source_withdrawn");
    Ok(())
}
