use super::*;
use crate::person_collaboration::{
    model::{Assignment, Change, Command, PermissionMode, PermissionSetting},
    store::business_config::{BusinessConfigChange, BusinessConfigCommand},
};

async fn version(store: &Store) -> Result<i64> {
    Ok(sqlx::query_scalar(
        "SELECT version FROM qintopia_agent_os.collaboration_tenants WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .fetch_one(&store.pool)
    .await?)
}

fn config(expected_version: i64, change: BusinessConfigChange) -> BusinessConfigCommand {
    BusinessConfigCommand {
        operation_id: Uuid::new_v4(),
        expected_version,
        change,
    }
}

#[tokio::test]
#[ignore = "explicit task-isolated live tenant database required"]
async fn live_admin_lifecycle_and_broker_scope_fail_closed() -> Result<()> {
    let base = Fixture::new().await?;
    let pool = base.store.pool.clone();
    let tenant = format!("live-anan-backend-{}", Uuid::new_v4());
    let namespace = format!("live-anan-identities-{}", Uuid::new_v4());
    let admin_person = base.store.verified_person(&base.actor).await?;
    let staff_person: Uuid = sqlx::query_scalar(
        "INSERT INTO qintopia_identity.persons(display_name) VALUES('模拟客房员工') RETURNING id",
    )
    .fetch_one(&pool)
    .await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_tenants(tenant_key,identity_namespace,mode,initialized) VALUES($1,$2,'live',true)")
        .bind(&tenant).bind(&namespace).execute(&pool).await?;
    let root:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,label,kind) VALUES($1,'模拟社区','community') RETURNING id")
        .bind(&tenant).fetch_one(&pool).await?;
    let building:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,parent_scope_id,label,kind) VALUES($1,$2,'一栋','building') RETURNING id")
        .bind(&tenant).bind(root).fetch_one(&pool).await?;
    let other:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,parent_scope_id,label,kind) VALUES($1,$2,'二栋','building') RETURNING id")
        .bind(&tenant).bind(root).fetch_one(&pool).await?;
    let manager_role:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_roles(tenant_key,label,available_actions) VALUES($1,'模拟主管',ARRAY['manage']::text[]) RETURNING id")
        .bind(&tenant).fetch_one(&pool).await?;
    let staff_role:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_roles(tenant_key,label,available_actions) VALUES($1,'模拟客服',ARRAY['read_business','execute_business']::text[]) RETURNING id")
        .bind(&tenant).fetch_one(&pool).await?;
    let appointment:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_appointments(tenant_key,person_id,role_id,scope_id) VALUES($1,$2,$3,$4) RETURNING id")
        .bind(&tenant).bind(admin_person).bind(manager_role).bind(root).fetch_one(&pool).await?;
    let collaboration:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key,responsibility_text) VALUES($1,$2,'default','organization','模拟已有主管任职') RETURNING id")
        .bind(&tenant).bind(appointment).fetch_one(&pool).await?;
    let manager_grant:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,include_descendants,managed_agents,managed_domains,managed_actions,delegation_depth) VALUES($1,$2,'manage',true,ARRAY['anan']::text[],ARRAY['hospitality']::text[],ARRAY['read_business','execute_business']::text[],1) RETURNING id")
        .bind(&tenant).bind(collaboration).fetch_one(&pool).await?;
    for (source, person) in [
        ("simulated-admin", admin_person),
        ("simulated-staff", staff_person),
    ] {
        sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by) VALUES($1,'wecom_internal',$2,$3,'confirmed',$4,$5)")
            .bind(&namespace).bind(source).bind(person).bind(Uuid::new_v4()).bind(admin_person).execute(&pool).await?;
    }
    let admin_gateway = format!("live-admin-{}", Uuid::new_v4());
    let staff_gateway = format!("live-staff-{}", Uuid::new_v4());
    for (gateway, scope) in [(&admin_gateway, root), (&staff_gateway, building)] {
        sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'wecom_internal',$4,'employee',true)")
            .bind(&tenant).bind(gateway).bind(&namespace).bind(scope).execute(&pool).await?;
    }
    let store = Store::live(pool.clone(), &tenant, &namespace).await?;
    assert!(
        Store::live(pool.clone(), &base.store.tenant, &base.store.tenant)
            .await
            .is_err()
    );
    let admin = store
        .production_admin_actor(&admin_gateway, "simulated-admin")
        .await?;
    let staff = store
        .gateway_actor(&staff_gateway, "simulated-staff")
        .await?;
    assert!(store
        .production_admin_actor(&staff_gateway, "simulated-staff")
        .await
        .is_err());
    assert!(store
        .preflight_business_gateway(&staff_gateway)
        .await
        .is_err());

    let assignment = Assignment {
        collaboration: None,
        person: staff_person,
        role: staff_role,
        duty: None,
        scope: building,
        agent: "anan".into(),
        domain: "hospitality".into(),
        responsibility: "模拟本栋客房办理".into(),
        valid_from: None,
        valid_until: None,
        proxy_for: None,
        actions: vec![],
        permissions: ["read_business", "execute_business"]
            .into_iter()
            .map(|action| PermissionSetting {
                action: action.into(),
                mode: PermissionMode::Autonomous,
                reviewer: None,
            })
            .collect(),
        delegation: None,
    };
    let add = Command {
        operation_id: Uuid::new_v4(),
        expected_version: version(&store).await?,
        change: Change::Assign(Box::new(assignment.clone())),
    };
    assert_eq!(
        store.command(&admin, &add, false).await?["persisted"],
        false
    );
    assert_eq!(store.command(&admin, &add, true).await?["persisted"], true);
    assert_eq!(store.command(&admin, &add, true).await?["replayed"], true);
    let mut self_assignment = assignment.clone();
    self_assignment.person = admin_person;
    assert!(store
        .command(
            &admin,
            &Command {
                operation_id: Uuid::new_v4(),
                expected_version: version(&store).await?,
                change: Change::Assign(Box::new(self_assignment))
            },
            false
        )
        .await
        .is_err());
    let execute_grant:Uuid=sqlx::query_scalar("SELECT g.id FROM qintopia_agent_os.collaboration_grants g JOIN qintopia_agent_os.agent_collaborations c ON c.id=g.collaboration_id JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE g.tenant_key=$1 AND a.person_id=$2 AND g.action_key='execute_business' AND g.status='active'")
        .bind(&tenant).bind(staff_person).fetch_one(&pool).await?;

    let create = config(
        version(&store).await?,
        BusinessConfigChange::CreateBinding {
            scope: building,
            source: "simulated-pms".into(),
            property: "property_a".into(),
        },
    );
    assert_eq!(
        store.business_configure(&admin, &create, false).await?["persisted"],
        false
    );
    let saved = store.business_configure(&admin, &create, true).await?;
    assert_eq!(
        store.business_configure(&admin, &create, true).await?["replayed"],
        true
    );
    let binding: Uuid = serde_json::from_value(saved["change"]["binding"].clone())?;
    assert!(store
        .business_configure(
            &staff,
            &config(
                version(&store).await?,
                BusinessConfigChange::DisableBinding {
                    binding,
                    expected_binding_version: 1
                }
            ),
            true
        )
        .await
        .is_err());
    assert!(store
        .preflight_business_gateway(&staff_gateway)
        .await
        .is_err());
    let conversation:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type) VALUES($1,'wecom',$2,'group') RETURNING id")
        .bind(&tenant).bind(format!("simulated-workgroup-{tenant}")).fetch_one(&pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3)")
        .bind(&tenant).bind(building).bind(conversation).execute(&pool).await?;
    store.preflight_business_gateway(&staff_gateway).await?;
    assert!(store
        .business_context(&staff, "wecom", "direct", "staff-private")
        .await?["bindings"]
        .as_array()
        .unwrap()
        .is_empty());

    let grant = config(
        version(&store).await?,
        BusinessConfigChange::GrantOperation {
            binding,
            authority_grant: execute_grant,
            operation: "pms.command.RECORD_COLLECTION".into(),
            valid_until: None,
        },
    );
    assert_eq!(
        store.business_configure(&admin, &grant, true).await?["persisted"],
        true
    );
    for (tool, arguments) in [
        (
            "pms_authorize",
            json!({"binding":binding,"operation":"pms.command.RECORD_COLLECTION"}),
        ),
        (
            "pms_start",
            json!({"binding":binding,"operation":"pms.command.RECORD_COLLECTION","input":{},"reason":{"code":"OPERATOR_REQUEST","note":"模拟测试"}}),
        ),
    ] {
        assert_eq!(
            store
                .business_invoke(
                    &staff,
                    "uncaptured-message",
                    "staff-private",
                    tool,
                    &arguments
                )
                .await
                .unwrap_err()
                .to_string(),
            "trusted_message_evidence_required"
        );
    }
    let context = store
        .business_context(&staff, "wecom", "direct", "staff-private")
        .await?;
    assert_eq!(context["bindings"][0]["property"], "property_a");
    assert_eq!(
        context["bindings"][0]["operations"][0],
        "pms.command.RECORD_COLLECTION"
    );
    assert!(store
        .business_context(&staff, "wecom", "group", "another-group")
        .await
        .is_err());
    let mut head = payment_head("42");
    head.source_instance = "simulated-pms".into();
    store.business_feed_open(binding, Some(&head)).await?;
    store
        .business_accept_payment(
            binding,
            "simulated-pms",
            "property_a",
            &payment(43, "live_bill", "COLLECTION", "DISCOVERED"),
        )
        .await?;
    let payment_work: Uuid = sqlx::query_scalar("SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND binding_id=$2 AND subject_ref='live_bill'")
        .bind(&tenant).bind(binding).fetch_one(&pool).await?;
    store
        .business_capture_turn(
            &staff_gateway,
            &HostTurn {
                platform: "wecom".into(),
                chat_type: "direct".into(),
                chat_id: "staff-private".into(),
                sender_id: "simulated-staff".into(),
                message_id: "live-payment-review".into(),
                text: "请核对这笔收款".into(),
            },
        )
        .await?;
    let event = store
        .business_invoke(
            &staff,
            "live-payment-review",
            "staff-private",
            "pms_event_context",
            &json!({"binding":binding,"operation":"pms.command.RECORD_COLLECTION","work_item":payment_work}),
        )
        .await?;
    assert_eq!(event["bill_id"], "live_bill");
    assert_eq!(event["property"], "property_a");
    let other_binding: Uuid = serde_json::from_value(
        store
            .business_configure(
                &admin,
                &config(
                    version(&store).await?,
                    BusinessConfigChange::CreateBinding {
                        scope: other,
                        source: "simulated-pms".into(),
                        property: "property_b".into(),
                    },
                ),
                true,
            )
            .await?["change"]["binding"]
            .clone(),
    )?;
    assert!(store
        .business_configure(
            &admin,
            &config(
                version(&store).await?,
                BusinessConfigChange::GrantOperation {
                    binding: other_binding,
                    authority_grant: execute_grant,
                    operation: "pms.command.RECORD_COLLECTION".into(),
                    valid_until: None
                }
            ),
            true
        )
        .await
        .is_err());

    let operation_grant:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.business_operation_grants WHERE tenant_key=$1 AND binding_id=$2 AND revoked_at IS NULL")
        .bind(&tenant).bind(binding).fetch_one(&pool).await?;
    store
        .business_configure(
            &admin,
            &config(
                version(&store).await?,
                BusinessConfigChange::RevokeOperation {
                    grant: operation_grant,
                },
            ),
            true,
        )
        .await?;
    assert!(store
        .business_context(&staff, "wecom", "direct", "staff-private")
        .await?["bindings"]
        .as_array()
        .unwrap()
        .is_empty());
    store
        .business_configure(
            &admin,
            &config(
                version(&store).await?,
                BusinessConfigChange::GrantOperation {
                    binding,
                    authority_grant: execute_grant,
                    operation: "pms.command.RECORD_COLLECTION".into(),
                    valid_until: None,
                },
            ),
            true,
        )
        .await?;
    store
        .business_configure(
            &admin,
            &config(
                version(&store).await?,
                BusinessConfigChange::DisableBinding {
                    binding,
                    expected_binding_version: 1,
                },
            ),
            true,
        )
        .await?;
    assert!(store
        .business_authorize(&staff, binding, "pms.command.RECORD_COLLECTION")
        .await
        .is_err());
    let revoked:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.business_operation_grants WHERE tenant_key=$1 AND binding_id=$2 AND revoked_at IS NULL")
        .bind(&tenant).bind(binding).fetch_one(&pool).await?;
    assert_eq!(revoked, 0);
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked',revoked_at=clock_timestamp() WHERE id=$1")
        .bind(manager_grant).execute(&pool).await?;
    assert!(store
        .production_admin_actor(&admin_gateway, "simulated-admin")
        .await
        .is_err());
    assert!(store
        .business_configure(
            &admin,
            &config(
                version(&store).await?,
                BusinessConfigChange::CreateBinding {
                    scope: building,
                    source: "simulated-pms".into(),
                    property: "property_c".into()
                }
            ),
            true
        )
        .await
        .is_err());
    Ok(())
}

async fn payment_workitem_socket_request(
    socket: &std::path::Path,
    request: Value,
) -> Result<Value> {
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let stream = tokio::net::UnixStream::connect(socket).await?;
    let (read, mut write) = stream.into_split();
    write.write_all(&serde_json::to_vec(&request)?).await?;
    write.write_all(b"\n").await?;
    let mut line = String::new();
    BufReader::new(read).read_line(&mut line).await?;
    Ok(serde_json::from_str(&line)?)
}

#[tokio::test]
#[ignore = "explicit task-isolated live tenant database required"]
async fn live_payment_workitem_host_read_is_scoped_and_pure() -> Result<()> {
    if std::env::var("QINTOPIA_PAYMENT_READ_CHILD").as_deref() != Ok("1") {
        let output = tokio::task::spawn_blocking(|| {
            std::process::Command::new(std::env::current_exe()?)
                .arg("person_collaboration::business_tests::business_live_tests::live_payment_workitem_host_read_is_scoped_and_pure")
                .args(["--exact", "--ignored", "--nocapture"])
                .env("QINTOPIA_PAYMENT_READ_CHILD", "1")
                .output()
        }).await??;
        anyhow::ensure!(
            output.status.success(),
            "payment_read_child_failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }
    let base = Fixture::new().await?;
    let pool = base.store.pool.clone();
    let tenant = format!("live-anan-payment-read-{}", Uuid::new_v4());
    let namespace = format!("live-anan-payment-identity-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_tenants(tenant_key,identity_namespace,mode,initialized) VALUES($1,$2,'live',true)")
        .bind(&tenant).bind(&namespace).execute(&pool).await?;
    let scope:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,label,kind) VALUES($1,'模拟客房','community') RETURNING id")
        .bind(&tenant).fetch_one(&pool).await?;
    let gateway = format!("payment-read-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'wecom_internal',$4,'employee',true)")
        .bind(&tenant).bind(&gateway).bind(&namespace).bind(scope).execute(&pool).await?;
    let binding:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) VALUES($1,$2,'synthetic-pms','property_a') RETURNING id")
        .bind(&tenant).bind(scope).fetch_one(&pool).await?;
    let other_binding:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) VALUES($1,$2,'synthetic-pms','property_b') RETURNING id")
        .bind(&tenant).bind(scope).fetch_one(&pool).await?;
    let chat = format!("payment-read-group-{tenant}");
    let conversation:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type) VALUES($1,'wecom',$2,'group') RETURNING id")
        .bind(&tenant).bind(&chat).fetch_one(&pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3)")
        .bind(&tenant).bind(scope).bind(conversation).execute(&pool).await?;
    let store = Store::live(pool.clone(), &tenant, &namespace).await?;
    store
        .business_feed_open(binding, Some(&payment_head("40")))
        .await?;
    store
        .business_accept_payment(
            binding,
            "synthetic-pms",
            "property_a",
            &payment(41, "read_bill", "COLLECTION", "DISCOVERED"),
        )
        .await?;
    let work:Uuid=sqlx::query_scalar("SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND binding_id=$2 AND subject_ref='read_bill'")
        .bind(&tenant).bind(binding).fetch_one(&pool).await?;
    let foreign_tenant = format!("live-anan-payment-foreign-{}", Uuid::new_v4());
    let foreign_namespace = format!("live-anan-payment-foreign-identity-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_tenants(tenant_key,identity_namespace,mode,initialized) VALUES($1,$2,'live',true)")
        .bind(&foreign_tenant).bind(&foreign_namespace).execute(&pool).await?;
    let foreign_scope:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,label,kind) VALUES($1,'模拟其他物业','community') RETURNING id")
        .bind(&foreign_tenant).fetch_one(&pool).await?;
    let foreign_binding:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) VALUES($1,$2,'synthetic-pms','property_a') RETURNING id")
        .bind(&foreign_tenant).bind(foreign_scope).fetch_one(&pool).await?;
    let foreign_store = Store::live(pool.clone(), &foreign_tenant, &foreign_namespace).await?;
    foreign_store
        .business_feed_open(foreign_binding, Some(&payment_head("40")))
        .await?;
    foreign_store
        .business_accept_payment(
            foreign_binding,
            "synthetic-pms",
            "property_a",
            &payment(41, "foreign_bill", "COLLECTION", "DISCOVERED"),
        )
        .await?;
    let foreign_work:Uuid=sqlx::query_scalar("SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND binding_id=$2 AND subject_ref='foreign_bill'")
        .bind(&foreign_tenant).bind(foreign_binding).fetch_one(&pool).await?;

    let directory = tempfile::tempdir()?;
    let socket = directory.path().join("foundation.sock");
    let host_token = format!("host-{}", Uuid::new_v4().simple());
    let model_token = format!("model-{}", Uuid::new_v4().simple());
    for (key, value) in [
        (
            "QINTOPIA_FOUNDATION_SOCKET",
            socket.to_string_lossy().into_owned(),
        ),
        ("QINTOPIA_FOUNDATION_TOKEN", model_token.clone()),
        ("QINTOPIA_FOUNDATION_HOST_TOKEN", host_token.clone()),
        ("QINTOPIA_FOUNDATION_GATEWAY_ID", gateway.clone()),
        ("QINTOPIA_FOUNDATION_PROFILE", "anan".into()),
        ("QINTOPIA_FOUNDATION_PRODUCTION_ENABLE", "1".into()),
        ("QINTOPIA_PMS_EVENTS_PRODUCTION_ENABLE", "1".into()),
        ("QINTOPIA_PMS_EVENT_BINDING", binding.to_string()),
    ] {
        std::env::set_var(key, value);
    }
    std::env::remove_var("QINTOPIA_FOUNDATION_LOCAL_ENABLE");
    std::env::remove_var("QINTOPIA_PMS_EVENTS_LOCAL_ENABLE");
    let broker = tokio::spawn(super::super::foundation_server::broker_live(store));
    let result:Result<()> = async {
        tokio::time::timeout(std::time::Duration::from_secs(5),async {
            loop {
                if socket.exists() { break; }
                tokio::task::yield_now().await;
            }
        }).await?;
        let original_updated:chrono::DateTime<Utc>=sqlx::query_scalar("SELECT updated_at FROM qintopia_agent_os.work_items WHERE id=$1")
            .bind(work).fetch_one(&pool).await?;
        let original_counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1),(SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id=$2),(SELECT count(*) FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1)")
            .bind(&tenant).bind(work).fetch_one(&pool).await?;
        let request=|token:&str,tool:&str,operation:&str,work_item:Uuid,context:Value,binding:Uuid| json!({
            "operation":operation,"schema_version":1,"agent":"anan","tool":tool,
            "trusted_context":context,"arguments":{"binding":binding,"work_item":work_item},"token":token
        });
        let context=json!({"platform":"host","chat_type":"","chat_id":"","sender_id":"","message_id":"","gateway_id":gateway});
        let valid=request(&host_token,"pms_workitem_read","person_foundation_ingress",work,context.clone(),binding);
        let result=payment_workitem_socket_request(&socket,valid.clone()).await?;
        assert_eq!(result["ok"],true);
        assert_eq!(result["result"],json!({"work_item_id":work,"kind":"payment","status":"awaiting_review","readback_required":true,"contact_status":"pending_verified_channel","requires_human_confirmation":true}));
        for denied in [
            request(&model_token,"pms_workitem_read","person_foundation_ingress",work,context.clone(),binding),
            request(&host_token,"pms_workitem_read","person_foundation_tool",work,context.clone(),binding),
            request(&host_token,"pms_workitem_read","person_foundation_ingress",work,json!({"platform":"wecom","chat_type":"","chat_id":"","sender_id":"","message_id":"","gateway_id":gateway}),binding),
            request(&host_token,"pms_workitem_read","person_foundation_ingress",work,json!({"platform":"host","chat_type":"direct","chat_id":"","sender_id":"","message_id":"","gateway_id":gateway}),binding),
            request(&host_token,"pms_workitem_read","person_foundation_ingress",work,json!({"platform":"host","chat_type":"","chat_id":"","sender_id":"someone","message_id":"","gateway_id":gateway}),binding),
            request(&host_token,"pms_workitem_read","person_foundation_ingress",work,json!({"platform":"host","chat_type":"","chat_id":"","sender_id":"","message_id":"","gateway_id":"wrong-gateway"}),binding),
            request(&host_token,"pms_workitem_read","person_foundation_ingress",work,context.clone(),other_binding),
            request(&host_token,"pms_workitem_read","person_foundation_ingress",foreign_work,context.clone(),binding),
        ] { assert_eq!(payment_workitem_socket_request(&socket,denied).await?["ok"],false); }
        let invalid=valid.as_object().unwrap().clone();
        let mut extra=Value::Object(invalid);
        extra["arguments"]["bill_id"]=json!("read_bill");
        assert_eq!(payment_workitem_socket_request(&socket,extra).await?["ok"],false);
        let after_updated:chrono::DateTime<Utc>=sqlx::query_scalar("SELECT updated_at FROM qintopia_agent_os.work_items WHERE id=$1")
            .bind(work).fetch_one(&pool).await?;
        let after_counts:(i64,i64,i64)=sqlx::query_as("SELECT (SELECT count(*) FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1),(SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id=$2),(SELECT count(*) FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1)")
            .bind(&tenant).bind(work).fetch_one(&pool).await?;
        assert_eq!(original_updated,after_updated);
        assert_eq!(original_counts,after_counts);
        sqlx::query("UPDATE qintopia_agent_os.collaboration_tenants SET initialized=false WHERE tenant_key=$1")
            .bind(&tenant).execute(&pool).await?;
        assert_eq!(payment_workitem_socket_request(&socket,valid.clone()).await?["ok"],false);
        sqlx::query("UPDATE qintopia_agent_os.collaboration_tenants SET initialized=true,mode='synthetic' WHERE tenant_key=$1")
            .bind(&tenant).execute(&pool).await?;
        assert_eq!(payment_workitem_socket_request(&socket,valid.clone()).await?["ok"],false);
        sqlx::query("UPDATE qintopia_agent_os.collaboration_tenants SET mode='live',identity_namespace=$2 WHERE tenant_key=$1")
            .bind(&tenant).bind(format!("changed-{namespace}")).execute(&pool).await?;
        assert_eq!(payment_workitem_socket_request(&socket,valid.clone()).await?["ok"],false);
        sqlx::query("UPDATE qintopia_agent_os.collaboration_tenants SET identity_namespace=$2 WHERE tenant_key=$1")
            .bind(&tenant).bind(&namespace).execute(&pool).await?;
        assert_eq!(original_updated,sqlx::query_scalar::<_,chrono::DateTime<Utc>>("SELECT updated_at FROM qintopia_agent_os.work_items WHERE id=$1").bind(work).fetch_one(&pool).await?);
        assert_eq!(original_counts,sqlx::query_as::<_,(i64,i64,i64)>("SELECT (SELECT count(*) FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1),(SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id=$2),(SELECT count(*) FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1)").bind(&tenant).bind(work).fetch_one(&pool).await?);
        sqlx::query("UPDATE qintopia_agent_os.business_property_bindings SET active=false WHERE id=$1")
            .bind(binding).execute(&pool).await?;
        assert_eq!(payment_workitem_socket_request(&socket,valid.clone()).await?["ok"],false);
        sqlx::query("UPDATE qintopia_agent_os.business_property_bindings SET active=true WHERE id=$1")
            .bind(binding).execute(&pool).await?;
        foreign_store.business_feed_open(foreign_binding,None).await?;
        let read=payment_workitem_socket_request(&socket,valid.clone()).await?;
        assert_eq!(read["result"]["requires_human_confirmation"],true);
        // A source match suppresses follow-up even if the work item remains open.
        let live=Store::live(pool.clone(),&tenant,&namespace).await?;
        live.business_accept_payment(binding,"synthetic-pms","property_a",&payment(42,"read_bill","COLLECTION","MATCHED")).await?;
        let matched=payment_workitem_socket_request(&socket,valid.clone()).await?;
        assert_eq!(matched["result"]["requires_human_confirmation"],false);
        assert_eq!(matched["result"]["contact_status"],"suppressed_pending_readback");
        sqlx::query("UPDATE qintopia_agent_os.work_items SET status='completed' WHERE id=$1")
            .bind(work).execute(&pool).await?;
        let terminal=payment_workitem_socket_request(&socket,valid).await?;
        assert_eq!(terminal["result"]["status"],"completed");
        assert_eq!(terminal["result"]["requires_human_confirmation"],false);
        Ok(())
    }.await;
    broker.abort();
    let _ = broker.await;
    result
}

#[tokio::test]
#[ignore = "explicit task-isolated live tenant database required"]
async fn live_host_observation_and_person_cli_do_not_grant_business_authority() -> Result<()> {
    if std::env::var("QINTOPIA_PERSON_CLI_CHILD").as_deref() != Ok("1") {
        let output=tokio::task::spawn_blocking(|| std::process::Command::new(std::env::current_exe()?)
            .arg("person_collaboration::business_tests::business_live_tests::live_host_observation_and_person_cli_do_not_grant_business_authority")
            .args(["--exact","--ignored","--nocapture"])
            .env("QINTOPIA_PERSON_CLI_CHILD","1").output()).await??;
        anyhow::ensure!(
            output.status.success(),
            "person_cli_child_failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        return Ok(());
    }
    let base = Fixture::new().await?;
    let pool = base.store.pool.clone();
    let tenant = format!("live-person-cli-{}", Uuid::new_v4());
    let namespace = format!("live-person-cli-identity-{}", Uuid::new_v4());
    let admin_person = base.store.verified_person(&base.actor).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_tenants(tenant_key,identity_namespace,mode,initialized) VALUES($1,$2,'live',true)")
        .bind(&tenant).bind(&namespace).execute(&pool).await?;
    let scope:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,label,kind) VALUES($1,'模拟社区','community') RETURNING id")
        .bind(&tenant).fetch_one(&pool).await?;
    let other_scope:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,parent_scope_id,label,kind) VALUES($1,$2,'模拟其他楼栋','building') RETURNING id")
        .bind(&tenant).bind(scope).fetch_one(&pool).await?;
    let role:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_roles(tenant_key,label,available_actions) VALUES($1,'模拟管理员',ARRAY['manage']::text[]) RETURNING id")
        .bind(&tenant).fetch_one(&pool).await?;
    let appointment:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_appointments(tenant_key,person_id,role_id,scope_id) VALUES($1,$2,$3,$4) RETURNING id")
        .bind(&tenant).bind(admin_person).bind(role).bind(scope).fetch_one(&pool).await?;
    let collaboration:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key,responsibility_text) VALUES($1,$2,'default','organization','模拟身份管理') RETURNING id")
        .bind(&tenant).bind(appointment).fetch_one(&pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,include_descendants,managed_agents,managed_domains,managed_actions,delegation_depth) VALUES($1,$2,'manage',true,ARRAY['default']::text[],ARRAY['organization']::text[],ARRAY['manage','identity']::text[],1)")
        .bind(&tenant).bind(collaboration).execute(&pool).await?;
    let admin_sender = "simulated-person-admin";
    sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by) VALUES($1,'wecom_internal',$2,$3,'confirmed',$4,$3)")
        .bind(&namespace).bind(admin_sender).bind(admin_person).bind(Uuid::new_v4()).execute(&pool).await?;
    let gateway = format!("person-cli-gateway-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'wecom_internal',$4,'employee',true)")
        .bind(&tenant).bind(&gateway).bind(&namespace).bind(scope).execute(&pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) VALUES($1,$2,'simulated-pms','property_a')")
        .bind(&tenant).bind(scope).execute(&pool).await?;
    let chat = format!("person-cli-group-{tenant}");
    let conversation:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type) VALUES($1,'wecom',$2,'group') RETURNING id")
        .bind(&tenant).bind(&chat).fetch_one(&pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3)")
        .bind(&tenant).bind(scope).bind(conversation).execute(&pool).await?;
    let store = Store::live(pool.clone(), &tenant, &namespace).await?;
    let admin = store.production_admin_actor(&gateway, admin_sender).await?;
    let socket_dir = tempfile::tempdir()?;
    let socket = socket_dir.path().join("foundation.sock");
    let host_token = format!("host-{}", Uuid::new_v4().simple());
    let model_token = format!("model-{}", Uuid::new_v4().simple());
    for (key, value) in [
        (
            "QINTOPIA_FOUNDATION_SOCKET",
            socket.to_string_lossy().into_owned(),
        ),
        ("QINTOPIA_FOUNDATION_TOKEN", model_token.clone()),
        ("QINTOPIA_FOUNDATION_HOST_TOKEN", host_token.clone()),
        ("QINTOPIA_FOUNDATION_GATEWAY_ID", gateway.clone()),
        ("QINTOPIA_FOUNDATION_PROFILE", "anan".into()),
        ("QINTOPIA_FOUNDATION_PRODUCTION_ENABLE", "1".into()),
    ] {
        std::env::set_var(key, value);
    }
    std::env::remove_var("QINTOPIA_FOUNDATION_LOCAL_ENABLE");
    let broker_store = Store::live(pool.clone(), &tenant, &namespace).await?;
    let broker = tokio::spawn(super::super::foundation_server::broker_live(broker_store));
    let result:Result<()> = async {
        tokio::time::timeout(std::time::Duration::from_secs(5),async {
            loop { if socket.exists() { break; } tokio::task::yield_now().await; }
        }).await?;
        let sender="simulated-unconfirmed-staff";
        let context=|message:&str,chat_id:&str,sender_id:&str| json!({"platform":"wecom","chat_type":"group","chat_id":chat_id,"sender_id":sender_id,"message_id":message,"gateway_id":gateway});
        let capture=|token:&str,message:&str,chat_id:&str,sender_id:&str| json!({"operation":"person_foundation_ingress","schema_version":1,"agent":"anan","tool":"pms_capture","trusted_context":context(message,chat_id,sender_id),"arguments":{"text":"模拟客房询问"},"token":token});
        let original_people:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_identity.persons")
            .fetch_one(&pool).await?;
        let original_grants:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_grants WHERE tenant_key=$1")
            .bind(&tenant).fetch_one(&pool).await?;
        for denied in [
            capture(&model_token,"first",&chat,sender),
            capture(&host_token,"wrong-group","unbound-group",sender),
        ] { assert_eq!(payment_workitem_socket_request(&socket,denied).await?["ok"],false); }
        let prior_events:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_messages.raw_events WHERE source='wecom-host' AND payload->>'tenant_key'=$1")
            .bind(&tenant).fetch_one(&pool).await?;
        assert_eq!(prior_events,0);
        let valid=capture(&host_token,"first",&chat,sender);
        let first=payment_workitem_socket_request(&socket,valid.clone()).await?;
        assert_eq!(first["result"]["status"],"identity_pending");
        assert_eq!(payment_workitem_socket_request(&socket,valid).await?["result"]["status"],"identity_pending");
        let pending=store.production_identities(&admin).await?;
        assert_eq!(pending["can_manage"],true);
        let candidate=pending["candidates"].as_array().unwrap().iter()
            .find(|entry| entry["account_label"]==sender).unwrap();
        assert_eq!(candidate["selectable"],true);
        let link:Uuid=serde_json::from_value(candidate["id"].clone())?;
        let link_version=candidate["version"].as_i64().unwrap();
        let gateway_version=candidate["gateway_version"].as_i64().unwrap();
        let after_events:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_messages.raw_events WHERE source='wecom-host' AND payload->>'tenant_key'=$1")
            .bind(&tenant).fetch_one(&pool).await?;
        assert_eq!(after_events,1);
        assert_eq!(payment_workitem_socket_request(&socket,capture(&host_token,"first",&chat,"different-sender")).await?["ok"],false);
        assert_eq!(original_people,sqlx::query_scalar::<_,i64>("SELECT count(*) FROM qintopia_identity.persons").fetch_one(&pool).await?);
        assert_eq!(original_grants,sqlx::query_scalar::<_,i64>("SELECT count(*) FROM qintopia_agent_os.collaboration_grants WHERE tenant_key=$1").bind(&tenant).fetch_one(&pool).await?);
        let draft_version=version(&store).await?;
        let draft=|scope_ref:Uuid| Command { operation_id:link, expected_version:draft_version,
            change:Change::SaveLedger { id:None,object:"person".into(),reference:None,label:"模拟新员工".into(),nickname:"小陈".into(),description:"待来源核验".into(),scope:Some(scope_ref),owner:None,draft:true }};
        assert!(store.production_person_draft(&admin,link,&draft(other_scope),true).await.is_err());
        let command=draft(scope);
        assert_eq!(store.production_person_draft(&admin,link,&command,false).await?["persisted"],false);
        assert_eq!(original_people,sqlx::query_scalar::<_,i64>("SELECT count(*) FROM qintopia_identity.persons").fetch_one(&pool).await?);
        let saved=store.production_person_draft(&admin,link,&command,true).await?;
        assert_eq!(saved["change"]["verified"],false);
        assert_eq!(store.production_person_draft(&admin,link,&command,true).await?["replayed"],true);
        let ledger:Uuid=serde_json::from_value(saved["change"]["id"].clone())?;
        let person_text:String=sqlx::query_scalar("SELECT object_ref FROM qintopia_agent_os.collaboration_ledger WHERE id=$1")
            .bind(ledger).fetch_one(&pool).await?;
        let person=Uuid::parse_str(&person_text)?;
        assert_eq!(original_grants,sqlx::query_scalar::<_,i64>("SELECT count(*) FROM qintopia_agent_os.collaboration_grants WHERE tenant_key=$1").bind(&tenant).fetch_one(&pool).await?);
        let confirm=super::super::store::IdentityUiCommand { operation_id:Uuid::new_v4(),link_ref:link,person_ref:person,
            expected_version:link_version,expected_configuration_version:version(&store).await?,expected_gateway_version:gateway_version,revoke:false };
        assert_eq!(store.production_identity_change(&admin,&confirm,false).await?["saved"],false);
        assert_eq!(store.production_identity_change(&admin,&confirm,true).await?["saved"],true);
        assert_eq!(store.production_identity_change(&admin,&confirm,true).await?["replayed"],true);
        assert_eq!(original_grants,sqlx::query_scalar::<_,i64>("SELECT count(*) FROM qintopia_agent_os.collaboration_grants WHERE tenant_key=$1").bind(&tenant).fetch_one(&pool).await?);
        let revoke=super::super::store::IdentityUiCommand { operation_id:Uuid::new_v4(),link_ref:link,person_ref:person,
            expected_version:link_version+1,expected_configuration_version:version(&store).await?,expected_gateway_version:gateway_version,revoke:true };
        assert_eq!(store.production_identity_change(&admin,&revoke,true).await?["saved"],true);
        assert_eq!(payment_workitem_socket_request(&socket,capture(&host_token,"after-revoke",&chat,sender)).await?["ok"],false);
        let status:String=sqlx::query_scalar("SELECT status FROM qintopia_identity.source_identity_links WHERE id=$1")
            .bind(link).fetch_one(&pool).await?;
        assert_eq!(status,"revoked");
        let final_events:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_messages.raw_events WHERE source='wecom-host' AND payload->>'tenant_key'=$1")
            .bind(&tenant).fetch_one(&pool).await?;
        assert_eq!(final_events,1);
        Ok(())
    }.await;
    broker.abort();
    let _ = broker.await;
    result
}
