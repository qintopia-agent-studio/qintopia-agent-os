//! Application source tests share the existing explicit isolated PG fixture.
use super::*;
use crate::person_collaboration::store::applications::Observation;

fn observation() -> Observation {
    Observation {
        identity_hash: "a".repeat(64),
        field_hash: "b".repeat(64),
        valid: true,
        consent_active: true,
        source_version: Some("1234".into()),
    }
}
fn record() -> String {
    format!("rec{}", Uuid::new_v4().simple())
}
async fn save(f: &Fixture, record: &str, observation: &Observation) -> Result<Value> {
    let opened = f
        .store
        .application_read_open(f.binding, "resident-application", record)
        .await?;
    f.store
        .application_read_save(
            f.binding,
            "resident-application",
            record,
            serde_json::from_value(opened["read_token"].clone())?,
            observation,
        )
        .await
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn application_repeated_readback_reuses_projection_and_two_work_items() -> Result<()> {
    let f = Fixture::new().await?;
    let record = record();
    let first = save(&f, &record, &observation()).await?;
    let mut next = observation();
    next.source_version = Some("9999".into());
    let duplicate = save(&f, &record, &next).await?;
    assert_eq!(first["revision"], 1);
    assert_eq!(duplicate["status"], "duplicate");
    assert_eq!(duplicate["revision"], 1);
    assert_eq!(first["application"], duplicate["application"]);
    assert_eq!(first["work_items"], duplicate["work_items"]);
    let application: Uuid = serde_json::from_value(first["application"].clone())?;
    let rows=sqlx::query("SELECT target_agent,work_item_type,payload,status FROM qintopia_agent_os.work_items WHERE payload->>'application_ref'=$1 ORDER BY target_agent")
        .bind(application.to_string()).fetch_all(&f.store.pool).await?;
    assert_eq!(rows.len(), 2);
    assert_eq!(rows[0].get::<String, _>("target_agent"), "anan");
    assert_eq!(rows[1].get::<String, _>("target_agent"), "silaoshi");
    assert_eq!(
        rows[1].get::<String, _>("work_item_type"),
        "application_review"
    );
    assert!(rows
        .iter()
        .all(|r| r.get::<String, _>("status") == "awaiting_review"));
    let actions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.business_actions WHERE tenant_key=$1",
    )
    .bind(&f.store.tenant)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(actions, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn application_read_fence_rejects_stale_response_and_recovers_lost_ack() -> Result<()> {
    let f = Fixture::new().await?;
    let record = record();
    let first = f
        .store
        .application_read_open(f.binding, "resident-application", &record)
        .await?;
    let second = f
        .store
        .application_read_open(f.binding, "resident-application", &record)
        .await?;
    let old_token = serde_json::from_value(first["read_token"].clone())?;
    let token = serde_json::from_value(second["read_token"].clone())?;
    assert!(f
        .store
        .application_read_save(
            f.binding,
            "resident-application",
            &record,
            old_token,
            &observation()
        )
        .await
        .is_err());
    let accepted = f
        .store
        .application_read_save(
            f.binding,
            "resident-application",
            &record,
            token,
            &observation(),
        )
        .await?;
    let replay = f
        .store
        .application_read_save(
            f.binding,
            "resident-application",
            &record,
            token,
            &observation(),
        )
        .await?;
    assert_eq!(accepted["application"], replay["application"]);
    assert_eq!(replay["status"], "duplicate");
    let mut conflict = observation();
    conflict.field_hash = "c".repeat(64);
    assert!(f
        .store
        .application_read_save(f.binding, "resident-application", &record, token, &conflict)
        .await
        .is_err());
    sqlx::query(
        "UPDATE qintopia_agent_os.business_property_bindings SET version=version+1 WHERE id=$1",
    )
    .bind(f.binding)
    .execute(&f.store.pool)
    .await?;
    assert!(f
        .store
        .application_read_open(f.binding, "resident-application", &record)
        .await
        .is_err());
    assert!(f
        .store
        .application_read_save(
            f.binding,
            "resident-application",
            &record,
            token,
            &observation()
        )
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn application_content_preserves_person_but_identity_change_unlinks_only_application(
) -> Result<()> {
    let f = Fixture::new().await?;
    let record = record();
    let first = save(&f, &record, &observation()).await?;
    let application: Uuid = serde_json::from_value(first["application"].clone())?;
    let person = f.store.verified_person(&f.actor).await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_applications SET person_id=$2 WHERE id=$1")
        .bind(application)
        .bind(person)
        .execute(&f.store.pool)
        .await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_sources(source_instance,property_id,mode) VALUES('synthetic-pms','property_a','synthetic') ON CONFLICT DO NOTHING").execute(&f.store.pool).await?;
    let case:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_cases(source_instance,property_id,order_id,stay_id,occupant_id,person_id,application_id) VALUES('synthetic-pms','property_a',$1,$1,'synthetic-occupant',$2,$3) RETURNING id")
        .bind(Uuid::new_v4().to_string()).bind(person).bind(application).fetch_one(&f.store.pool).await?;
    let mut content = observation();
    content.field_hash = "c".repeat(64);
    assert_eq!(save(&f, &record, &content).await?["revision"], 2);
    let kept: Option<Uuid> = sqlx::query_scalar(
        "SELECT person_id FROM qintopia_agent_os.welcome_applications WHERE id=$1",
    )
    .bind(application)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(kept, Some(person));
    let kept_case: Option<Uuid> = sqlx::query_scalar(
        "SELECT application_id FROM qintopia_agent_os.welcome_cases WHERE id=$1",
    )
    .bind(case)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(kept_case, Some(application));
    content.identity_hash = "d".repeat(64);
    content.field_hash = "e".repeat(64);
    assert_eq!(save(&f, &record, &content).await?["revision"], 3);
    let cleared: Option<Uuid> = sqlx::query_scalar(
        "SELECT person_id FROM qintopia_agent_os.welcome_applications WHERE id=$1",
    )
    .bind(application)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(cleared, None);
    let row = sqlx::query(
        "SELECT application_id,person_id FROM qintopia_agent_os.welcome_cases WHERE id=$1",
    )
    .bind(case)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(row.get::<Option<Uuid>, _>("application_id"), None);
    assert_eq!(row.get::<Option<Uuid>, _>("person_id"), Some(person));
    assert_eq!(f.store.verified_person(&f.actor).await?, person);
    let audits:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type='application_identity_basis_changed' AND data->>'application_ref'=$1 AND data->>'previous_person'=$2")
        .bind(application.to_string()).bind(person.to_string()).fetch_one(&f.store.pool).await?;
    assert_eq!(audits, 1);
    // A -> B -> A never recreates the application/person or application/stay links.
    content = observation();
    assert_eq!(save(&f, &record, &content).await?["revision"], 4);
    let restored: Option<Uuid> = sqlx::query_scalar(
        "SELECT person_id FROM qintopia_agent_os.welcome_applications WHERE id=$1",
    )
    .bind(application)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(restored, None);
    let restored_case: Option<Uuid> = sqlx::query_scalar(
        "SELECT application_id FROM qintopia_agent_os.welcome_cases WHERE id=$1",
    )
    .bind(case)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(restored_case, None);
    let scope: Uuid = sqlx::query_scalar(
        "SELECT scope_id FROM qintopia_agent_os.business_property_bindings WHERE id=$1",
    )
    .bind(f.binding)
    .fetch_one(&f.store.pool)
    .await?;
    let mut tx = f.store.pool.begin().await?;
    assert_eq!(
        f.store
            .application_identity_basis(&mut tx, scope, application)
            .await?,
        Some(content.identity_hash.clone())
    );
    assert_eq!(
        f.store
            .application_identity_basis(&mut tx, Uuid::new_v4(), application)
            .await?,
        None
    );
    tx.commit().await?;
    // A different application writer cannot reuse the last accepted identity digest.
    for sql in [
        "UPDATE qintopia_agent_os.welcome_applications SET field_hash=repeat('f',64),revision=revision+1 WHERE id=$1",
        "UPDATE qintopia_agent_os.welcome_applications SET valid=NOT valid WHERE id=$1",
        "UPDATE qintopia_agent_os.welcome_applications SET consent_active=NOT consent_active WHERE id=$1",
    ] {
        let mut tx=f.store.pool.begin().await?;
        sqlx::query(sql).bind(application).execute(&mut *tx).await?;
        assert_eq!(f.store.application_identity_basis(&mut tx,scope,application).await?,None);
        tx.rollback().await?;
    }
    for sql in [
        "UPDATE qintopia_agent_os.business_property_bindings SET active=false WHERE id=$1",
        "UPDATE qintopia_agent_os.business_property_bindings SET version=version+1 WHERE id=$1",
    ] {
        let mut tx = f.store.pool.begin().await?;
        sqlx::query(sql).bind(f.binding).execute(&mut *tx).await?;
        assert_eq!(
            f.store
                .application_identity_basis(&mut tx, scope, application)
                .await?,
            None
        );
        tx.rollback().await?;
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn application_unreadable_source_does_not_withdraw_and_completed_work_is_not_replayed(
) -> Result<()> {
    let f = Fixture::new().await?;
    let record = record();
    let first = save(&f, &record, &observation()).await?;
    let app: Uuid = serde_json::from_value(first["application"].clone())?;
    let work: Uuid = serde_json::from_value(first["work_items"][0].clone())?;
    // A failed GET only leaves an opened read fence; no save means no source change.
    f.store
        .application_read_open(f.binding, "resident-application", &record)
        .await?;
    let valid: bool =
        sqlx::query_scalar("SELECT valid FROM qintopia_agent_os.welcome_applications WHERE id=$1")
            .bind(app)
            .fetch_one(&f.store.pool)
            .await?;
    assert!(valid);
    sqlx::query("UPDATE qintopia_agent_os.work_items SET status='completed' WHERE id=$1")
        .bind(work)
        .execute(&f.store.pool)
        .await?;
    let mut withdrawn = observation();
    withdrawn.valid = false;
    withdrawn.field_hash = "c".repeat(64);
    let result = save(&f, &record, &withdrawn).await?;
    assert_eq!(result["revision"], 2);
    assert_eq!(result["work_items"], first["work_items"]);
    let rows=sqlx::query("SELECT target_agent,status FROM qintopia_agent_os.work_items WHERE payload->>'application_ref'=$1 ORDER BY target_agent").bind(app.to_string()).fetch_all(&f.store.pool).await?;
    assert_eq!(rows[0].get::<String, _>("status"), "completed");
    assert_eq!(rows[1].get::<String, _>("status"), "cancelled");
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn application_model_ingress_and_unowned_projection_are_rejected() -> Result<()> {
    let f = Fixture::new().await?;
    let record = record();
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_applications(source_instance,resource_ref,record_ref,revision,field_hash) VALUES('synthetic-pms','resident-application',$1,1,$2)")
        .bind(&record).bind("f".repeat(64)).execute(&f.store.pool).await?;
    assert!(save(&f, &record, &observation()).await.is_err());
    let request = super::super::foundation_server::parse_broker_request(&serde_json::to_vec(
        &json!({
        "operation":"person_foundation_tool","schema_version":1,"agent":"anan","tool":"pms_application_intake",
        "trusted_context":{"gateway_id":f.gateway,"platform":"host","chat_type":"","chat_id":"","sender_id":"","message_id":""},
        "arguments":{"action":"open","record":record,"resource_alias":"resident-application"},"token":"not-used-direct-unit"}),
    )?)?;
    assert!(
        super::super::foundation_server::broker_invoke(&f.store, &f.gateway, "anan", request)
            .await
            .is_err()
    );
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "explicit task-isolated local database required"]
async fn application_signed_bridge_http_and_broker_persist_actual_readbacks() -> Result<()> {
    // The broker reads process-wide configuration. Run this journey alone in a
    // child test process so its local enablement cannot alter sibling snapshots.
    const CHILD: &str = "ANAN_APPLICATION_BRIDGE_TEST_CHILD";
    if std::env::var_os(CHILD).is_none() {
        let status = std::process::Command::new(std::env::current_exe()?)
            .args([
                "--exact",
                "person_collaboration::business_tests::application_tests::application_signed_bridge_http_and_broker_persist_actual_readbacks",
                "--ignored",
                "--nocapture",
                "--test-threads=1",
            ])
            .env(CHILD, "1")
            .status()?;
        anyhow::ensure!(status.success(), "application_bridge_child_failed");
        return Ok(());
    }
    let f = Fixture::new().await?;
    let record = record();
    let dir = tempfile::Builder::new()
        .prefix("anan-app-")
        .tempdir_in("/tmp")?;
    let socket = dir.path().join("foundation.sock");
    struct Restore(Vec<(String, Option<std::ffi::OsString>)>);
    impl Drop for Restore {
        fn drop(&mut self) {
            for (key, value) in &self.0 {
                if let Some(value) = value {
                    std::env::set_var(key, value)
                } else {
                    std::env::remove_var(key)
                }
            }
        }
    }
    let mut restore = Restore(Vec::new());
    for (key, value) in [
        (
            "QINTOPIA_FOUNDATION_SOCKET",
            socket.to_str().unwrap().into(),
        ),
        (
            "QINTOPIA_FOUNDATION_TOKEN",
            Uuid::new_v4().simple().to_string(),
        ),
        (
            "QINTOPIA_FOUNDATION_HOST_TOKEN",
            Uuid::new_v4().simple().to_string(),
        ),
        ("QINTOPIA_FOUNDATION_GATEWAY_ID", f.gateway.clone()),
        ("QINTOPIA_FOUNDATION_PROFILE", "anan".into()),
        ("QINTOPIA_FOUNDATION_LOCAL_ENABLE", "1".into()),
        ("QINTOPIA_PMS_LOCAL_ENABLE", "1".into()),
        ("QINTOPIA_APPLICATION_LOCAL_ENABLE", "1".into()),
        ("QINTOPIA_APPLICATION_BINDING", f.binding.to_string()),
        (
            "QINTOPIA_APPLICATION_RESOURCE_ALIAS",
            "resident-application".into(),
        ),
        ("ANAN_APPLICATION_TEST_RECORD", record.clone()),
    ] {
        restore.0.push((key.into(), std::env::var_os(key)));
        std::env::set_var(key, value);
    }
    let store = Store::local(
        &crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?,
        &f.store.tenant,
    )
    .await?;
    let broker = tokio::spawn(super::super::foundation_server::broker(store));
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    anyhow::ensure!(socket.exists(), "application_broker_not_ready");
    let result = tokio::task::spawn_blocking(|| {
        std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../skills/pms-operations/tests/application_bridge_journey.py"
            ))
            .status()
    })
    .await;
    broker.abort();
    anyhow::ensure!(result??.success(), "application_bridge_journey_failed");
    let row=sqlx::query("SELECT a.id,a.revision,a.valid FROM qintopia_agent_os.application_intake_states s JOIN qintopia_agent_os.welcome_applications a ON a.id=s.application_id WHERE s.tenant_key=$1 AND s.record_ref=$2")
        .bind(&f.store.tenant).bind(&record).fetch_one(&f.store.pool).await?;
    assert_eq!(row.get::<i64, _>("revision"), 4);
    assert!(!row.get::<bool, _>("valid"));
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.work_items WHERE payload->>'application_ref'=$1 AND status='cancelled'")
        .bind(row.get::<Uuid,_>("id").to_string()).fetch_one(&f.store.pool).await?;
    assert_eq!(count, 2);
    Ok(())
}
