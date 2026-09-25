//! Exact live application host scope and readback behavior on a disposable database.
use super::*;
use crate::person_collaboration::{digest, store::welcome_candidates::SourceProjection};
use serde_json::json;
use sqlx::{PgPool, Row};

fn observation(fields: &Value, valid: bool) -> Observation {
    let identity =
        json!({"name":fields["name"],"nickname":fields["nickname"],"phone":fields["phone"]});
    let full = json!({"fields":fields,"valid":valid,"consent_active":true});
    Observation {
        identity_hash: digest(&serde_json::to_vec(&identity).unwrap()),
        field_hash: digest(&serde_json::to_vec(&full).unwrap()),
        valid,
        consent_active: true,
        source_version: Some("1".into()),
    }
}

#[tokio::test]
#[ignore = "explicit task-isolated live tenant database required"]
async fn live_application_host_scope_replay_withdrawal_and_source_change() -> Result<()> {
    if std::env::var("QINTOPIA_APPLICATION_LIVE_CHILD").as_deref() != Ok("1") {
        let output = tokio::task::spawn_blocking(|| {
            std::process::Command::new(std::env::current_exe()?)
                .arg("person_collaboration::application_ingress::live_tests::live_application_host_scope_replay_withdrawal_and_source_change")
                .args(["--exact", "--ignored", "--nocapture"])
                .env("QINTOPIA_APPLICATION_LIVE_CHILD", "1")
                .output()
        })
        .await??;
        anyhow::ensure!(
            output.status.success()
                && String::from_utf8_lossy(&output.stdout).contains("test result: ok. 1 passed;"),
            "application_live_child_failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let pool = PgPool::connect(&database).await?;
    crate::db::run_migrations(&pool).await?;
    let tenant = format!("live-application-{}", Uuid::new_v4());
    let namespace = format!("live-application-identity-{}", Uuid::new_v4());
    let source = format!("simulated-pms-{}", Uuid::new_v4().simple());
    let occupant_namespace = format!("pms/{source}/property_a/occupant");
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_tenants(tenant_key,identity_namespace,mode,initialized) VALUES($1,$2,'live',true)")
        .bind(&tenant).bind(&namespace).execute(&pool).await?;
    let scope:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,label,kind) VALUES($1,'模拟物业','community') RETURNING id")
        .bind(&tenant).fetch_one(&pool).await?;
    let other:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,label,kind) VALUES($1,'另一模拟物业','community') RETURNING id")
        .bind(&tenant).fetch_one(&pool).await?;
    let binding:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) VALUES($1,$2,$3,'property_a') RETURNING id")
        .bind(&tenant).bind(scope).bind(&source).fetch_one(&pool).await?;
    let other_binding:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) VALUES($1,$2,$3,'property_b') RETURNING id")
        .bind(&tenant).bind(other).bind(&source).fetch_one(&pool).await?;
    let gateway = format!("simulated-application-gateway-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'wecom_internal',$4,'employee',true)")
        .bind(&tenant).bind(&gateway).bind(&namespace).bind(scope).execute(&pool).await?;
    let store = Store::live(pool.clone(), &tenant, &namespace).await?;

    std::env::set_var("QINTOPIA_FOUNDATION_PRODUCTION_ENABLE", "1");
    std::env::set_var("QINTOPIA_APPLICATION_PRODUCTION_ENABLE", "1");
    std::env::remove_var("QINTOPIA_FOUNDATION_LOCAL_ENABLE");
    std::env::remove_var("QINTOPIA_APPLICATION_LOCAL_ENABLE");
    std::env::set_var("QINTOPIA_FOUNDATION_GATEWAY_ID", &gateway);
    std::env::set_var("QINTOPIA_APPLICATION_BINDING", binding.to_string());
    std::env::set_var(
        "QINTOPIA_APPLICATION_RESOURCE_ALIAS",
        "resident-application",
    );
    authorize_host(&store, &gateway, binding, "resident-application").await?;
    assert!(
        authorize_host(&store, &gateway, other_binding, "resident-application")
            .await
            .is_err()
    );
    assert!(
        authorize_host(&store, "wrong-gateway", binding, "resident-application")
            .await
            .is_err()
    );
    assert!(authorize_host(&store, &gateway, binding, "other-source")
        .await
        .is_err());
    let model_request = crate::person_collaboration::foundation_server::parse_broker_request(
        &serde_json::to_vec(
            &json!({"operation":"person_foundation_tool","schema_version":1,
            "agent":"anan","tool":"pms_application_intake",
            "trusted_context":{"gateway_id":gateway,"platform":"host","chat_type":"","chat_id":"","sender_id":"","message_id":""},
            "arguments":{"action":"open","resource_alias":"resident-application","record":"recModelDenied"},"token":"model"}),
        )?,
    )?;
    assert!(
        crate::person_collaboration::foundation_server::broker_invoke(
            &store,
            &gateway,
            "anan",
            model_request
        )
        .await
        .is_err()
    );
    std::env::set_var("QINTOPIA_APPLICATION_LOCAL_ENABLE", "1");
    assert!(
        authorize_host(&store, &gateway, binding, "resident-application")
            .await
            .is_err()
    );
    std::env::remove_var("QINTOPIA_APPLICATION_LOCAL_ENABLE");

    let record = format!("rec{}", Uuid::new_v4().simple());
    let fields =
        json!({"name":"模拟新居民","nickname":"模拟小新","phone":"13800000001","consent":true});
    let opened = store
        .application_read_open(binding, "resident-application", &record)
        .await?;
    let token: Uuid = serde_json::from_value(opened["read_token"].clone())?;
    let first = store
        .application_read_save(
            binding,
            "resident-application",
            &record,
            token,
            &observation(&fields, true),
        )
        .await?;
    let application: Uuid = serde_json::from_value(first["application"].clone())?;
    let replay = store
        .application_read_save(
            binding,
            "resident-application",
            &record,
            token,
            &observation(&fields, true),
        )
        .await?;
    assert_eq!(replay["status"], "duplicate");
    let work: Uuid = serde_json::from_value(first["work_items"][0].clone())?;
    let metadata: Value =
        sqlx::query("SELECT metadata FROM qintopia_agent_os.work_items WHERE id=$1")
            .bind(work)
            .fetch_one(&pool)
            .await?
            .get("metadata");
    assert_eq!(metadata["local_only"], false);
    assert_eq!(metadata["event_is_not_authority"], true);
    let projection = SourceProjection {
        binding,
        application,
        fields: fields.clone(),
    };
    assert_eq!(
        store.welcome_project_source(&projection).await?["identity_confirmed"],
        false
    );

    sqlx::query("INSERT INTO qintopia_agent_os.welcome_sources(source_instance,property_id,mode,rebuilding,enabled) VALUES($1,'property_a','live',false,true)")
        .bind(&source).execute(&pool).await?;
    let order = format!("sim-order-{}", Uuid::new_v4().simple());
    let stay = format!("sim-stay-{}", Uuid::new_v4().simple());
    let occupant = format!("sim-occupant-{}", Uuid::new_v4().simple());
    let source_projection = json!({"state":"Reserved","current_arrangement":true,"inventory_reserved":true,
        "order":order,"stay":stay,"revision":"1","building":"A","arrival":"2026-09-25",
        "occupants":[{"id":occupant,"active":true}]});
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_source_versions(source_instance,property_id,aggregate_type,aggregate_id,revision,projection_hash,projection) VALUES($1,'property_a','order',$2,1,$3,$4)")
        .bind(&source).bind(&order).bind("f".repeat(64)).bind(source_projection).execute(&pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_identity_scopes(namespace,source_instance,property_id) VALUES($1,$2,'property_a')")
        .bind(&occupant_namespace).bind(&source).execute(&pool).await?;
    let case:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_cases(source_instance,property_id,order_id,stay_id,occupant_id,admitted) VALUES($1,'property_a',$2,$3,$4,true) RETURNING id")
        .bind(&source).bind(&order).bind(&stay).bind(&occupant).fetch_one(&pool).await?;
    let target:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_targets(source_instance,property_id,kind,display_name,namespace,conversation_ref,enabled) VALUES($1,'property_a','community','模拟社区群','simulated-room','room',true) RETURNING id")
        .bind(&source).fetch_one(&pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_foundation_targets(target_id,tenant_key,scope_id) VALUES($1,$2,$3)")
        .bind(target).bind(&tenant).bind(scope).execute(&pool).await?;
    let context = crate::person_collaboration::foundation_server::TrustedContext {
        platform: "host".into(),
        chat_type: String::new(),
        chat_id: String::new(),
        sender_id: String::new(),
        message_id: String::new(),
        gateway_id: gateway.clone(),
    };
    let open_request = serde_json::from_value(json!({"action":"open","work_item":work}))?;
    let contact_open = store
        .welcome_stay_contacts(
            &gateway,
            binding,
            "resident-application",
            &context,
            open_request,
        )
        .await?;
    assert_eq!(contact_open["local_only"], false);
    assert_eq!(contact_open["orders_total"], 1);
    assert_eq!(contact_open["reads"][0]["order_id"], order);
    assert!(store
        .welcome_stay_contacts(
            &gateway,
            other_binding,
            "resident-application",
            &context,
            serde_json::from_value(json!({"action":"status","work_item":work}))?
        )
        .await
        .is_err());
    let save_request = serde_json::from_value(json!({"action":"save","work_item":work,
        "read_token":contact_open["reads"][0]["read_token"],
        "order":{"id":order,"property_id":"property_a","version":1,
            "occupants":[{"id":occupant,"phone":"13800000001"}]}}))?;
    let saved = store
        .welcome_stay_contacts(
            &gateway,
            binding,
            "resident-application",
            &context,
            save_request,
        )
        .await?;
    assert_eq!(saved["scan_complete"], true);
    assert_eq!(saved["local_only"], false);
    assert!(!saved.to_string().contains("13800000001"));
    let contact_metadata: Value =
        sqlx::query("SELECT metadata FROM qintopia_agent_os.work_items WHERE id=$1")
            .bind(work)
            .fetch_one(&pool)
            .await?
            .get("metadata");
    assert!(!contact_metadata.to_string().contains("13800000001"));
    assert_eq!(
        store
            .application_reconcile_welcome(binding, "resident-application", &record)
            .await?["status"],
        "awaiting_reliable_stay_link"
    );
    let stored_case: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_agent_os.welcome_cases WHERE id=$1 AND application_id IS NULL",
    )
    .bind(case)
    .fetch_one(&pool)
    .await?;
    assert_eq!(stored_case, case);

    let changed =
        json!({"name":"模拟新居民改名","nickname":"模拟小新","phone":"13800000001","consent":true});
    let opened = store
        .application_read_open(binding, "resident-application", &record)
        .await?;
    let token: Uuid = serde_json::from_value(opened["read_token"].clone())?;
    store
        .application_read_save(
            binding,
            "resident-application",
            &record,
            token,
            &observation(&changed, true),
        )
        .await?;
    assert!(store.welcome_project_source(&projection).await.is_err());
    let projection = SourceProjection {
        binding,
        application,
        fields: changed.clone(),
    };
    store.welcome_project_source(&projection).await?;
    let opened = store
        .application_read_open(binding, "resident-application", &record)
        .await?;
    let token: Uuid = serde_json::from_value(opened["read_token"].clone())?;
    store
        .application_read_save(
            binding,
            "resident-application",
            &record,
            token,
            &observation(&changed, false),
        )
        .await?;
    assert!(store.welcome_project_source(&projection).await.is_err());
    assert_eq!(
        store
            .application_reconcile_welcome(binding, "resident-application", &record)
            .await?["status"],
        "source_withdrawn"
    );
    Ok(())
}
