//! Real shared-service persistence; only PMS HTTP and channel source are synthetic boundaries.
#![cfg(feature = "postgres-integration-tests")]
use super::store::business_config::{BusinessConfigChange, BusinessConfigCommand};
use super::{store::business::HostTurn, Actor, Store};
use anyhow::Result;
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

#[path = "application_tests.rs"]
mod application_tests;
#[path = "business_live_tests.rs"]
mod business_live_tests;
#[path = "business_manual_tests.rs"]
mod business_manual_tests;
#[path = "business_reminder_tests.rs"]
mod business_reminder_tests;
#[path = "business_scope_tests.rs"]
mod business_scope_tests;

struct Fixture {
    store: Store,
    actor: Actor,
    binding: Uuid,
    grant: Uuid,
    gateway: String,
    sender: String,
}
impl Fixture {
    async fn new() -> Result<Self> {
        let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
        let store = Store::local(
            &database,
            &format!("synthetic-collaboration-business-{}", Uuid::new_v4()),
        )
        .await?;
        static MIGRATED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
        MIGRATED
            .get_or_try_init(|| async {
                // Separate opt-in test processes can share this owned instance too.
                let mut setup = store.pool.begin().await?;
                sqlx::query("SELECT pg_advisory_xact_lock(724320260924)")
                    .execute(&mut *setup)
                    .await?;
                crate::db::run_migrations(&store.pool).await?;
                setup.commit().await?;
                Ok::<(), anyhow::Error>(())
            })
            .await?;
        let owner = store.actor(store.bootstrap_fixture().await?).await?;
        let person = store.verified_person(&owner).await?;
        let row=sqlx::query("SELECT a.id,a.scope_id,g.id AS grant FROM qintopia_agent_os.collaboration_appointments a JOIN qintopia_agent_os.agent_collaborations c ON c.appointment_id=a.id JOIN qintopia_agent_os.collaboration_grants g ON g.collaboration_id=c.id WHERE a.tenant_key=$1 AND a.person_id=$2 AND g.parent_grant_id IS NULL")
            .bind(&store.tenant).bind(person).fetch_one(&store.pool).await?;
        let scope: Uuid = row.get("scope_id");
        let parent: Uuid = row.get("grant");
        let collaboration:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key,responsibility_text) VALUES($1,$2,'anan','hospitality','模拟客房职责任命') RETURNING id")
            .bind(&store.tenant).bind(row.get::<Uuid,_>("id")).fetch_one(&store.pool).await?;
        let grant:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id,decision_mode) VALUES($1,$2,'execute_business',$3,'autonomous') RETURNING id")
            .bind(&store.tenant).bind(collaboration).bind(parent).fetch_one(&store.pool).await?;
        let binding:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) VALUES($1,$2,'synthetic-pms','property_a') RETURNING id")
            .bind(&store.tenant).bind(scope).fetch_one(&store.pool).await?;
        let gateway = format!("synthetic-business-{}", Uuid::new_v4());
        let sender = "synthetic_staff".to_string();
        sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by) VALUES($1,'wecom_internal',$2,$3,'confirmed',$4,$3)")
            .bind(&store.tenant).bind(&sender).bind(person).bind(Uuid::new_v4()).execute(&store.pool).await?;
        sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$1,'wecom_internal',$3,'employee',true)")
            .bind(&store.tenant).bind(&gateway).bind(scope).execute(&store.pool).await?;
        let actor = store.gateway_actor(&gateway, &sender).await?;
        Ok(Self {
            store,
            actor,
            binding,
            grant,
            gateway,
            sender,
        })
    }
    async fn allow(&self, command: &str) -> Result<Uuid> {
        // Explicit fixture provisioning; no runtime role-to-operation default exists.
        Ok(sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_operation_grants(tenant_key,authority_grant_id,binding_id,operation_key,issued_by) VALUES($1,$2,$3,$4,$5) RETURNING id")
            .bind(&self.store.tenant).bind(self.grant).bind(self.binding).bind(format!("pms.command.{command}")).bind(self.store.verified_person(&self.actor).await?).fetch_one(&self.store.pool).await?)
    }
    async fn capture(&self, message: &str, text: &str) -> Result<Value> {
        self.store
            .business_capture_turn(
                &self.gateway,
                &HostTurn {
                    platform: "wecom".into(),
                    chat_type: "direct".into(),
                    chat_id: "synthetic_chat".into(),
                    sender_id: self.sender.clone(),
                    message_id: message.into(),
                    text: text.into(),
                },
            )
            .await
    }
    async fn call(&self, message: &str, tool: &str, a: Value) -> Result<Value> {
        self.store
            .business_invoke(&self.actor, message, "synthetic_chat", tool, &a)
            .await
    }
    async fn prepared(&self) -> Result<(Value, Value)> {
        self.allow("CREATE_ORDER").await?;
        self.capture("start", "请预订").await?;
        let a=self.call("start","pms_start",json!({"binding":self.binding,"operation":"pms.command.CREATE_ORDER","input":{"quoteId":"quote_1"},"reason":{"code":"CREATE_STANDARD_ORDER","note":""}})).await?;
        let claim = self
            .call("start", "pms_claim_preview", json!({"action":a["action"]}))
            .await?;
        let saved=self.call("start","pms_save_preview",json!({"action":a["action"],"claim":claim["claim"],"preview":{"previewId":"preview_1","propertyId":"property_a","commandType":"CREATE_ORDER","effectHash":"a".repeat(64),"effect":{"amountMinor":12000},"expiresAt":(Utc::now()+Duration::minutes(10)).to_rfc3339()}})).await?;
        Ok((a, saved))
    }
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn shared_work_account_confirms_collection_and_revocation_stops_recovery() -> Result<()> {
    let f = Fixture::new().await?;
    let scope: Uuid = sqlx::query_scalar(
        "SELECT scope_id FROM qintopia_agent_os.business_property_bindings WHERE id=$1",
    )
    .bind(f.binding)
    .fetch_one(&f.store.pool)
    .await?;
    let namespace = format!("synthetic-shared-{}", Uuid::new_v4());
    let gateway = format!("synthetic-shared-gateway-{}", Uuid::new_v4());
    let sender = "stable_shared_account";
    sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'wecom_internal',$4,'shared',true)")
        .bind(&f.store.tenant).bind(&gateway).bind(&namespace).bind(scope).execute(&f.store.pool).await?;
    let link:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,adapter_metadata) VALUES($1,'wecom_internal',$2,jsonb_build_object('first_observation_ref',$3::text,'display_name','模拟共用账号')) RETURNING id")
        .bind(&namespace).bind(sender).bind(Uuid::new_v4()).fetch_one(&f.store.pool).await?;
    let command = |version, change| BusinessConfigCommand {
        operation_id: Uuid::new_v4(),
        expected_version: version,
        change,
    };
    let version = || async {
        sqlx::query_scalar::<_, i64>(
            "SELECT version FROM qintopia_agent_os.collaboration_tenants WHERE tenant_key=$1",
        )
        .bind(&f.store.tenant)
        .fetch_one(&f.store.pool)
        .await
    };
    assert!(f
        .store
        .business_gateway_actor(&gateway, sender)
        .await
        .is_err());
    let registered = f
        .store
        .business_configure(
            &f.actor,
            &command(
                version().await?,
                BusinessConfigChange::RegisterAccount {
                    gateway: gateway.clone(),
                    source_link: link,
                    label: "模拟共用账号".into(),
                },
            ),
            true,
        )
        .await?;
    let account: Uuid = serde_json::from_value(registered["change"]["account"].clone())?;
    assert!(f.store.gateway_actor(&gateway, sender).await.is_err());
    let actor = f.store.business_gateway_actor(&gateway, sender).await?;
    assert!(f
        .store
        .gateway_actor(&gateway, "模拟共用账号")
        .await
        .is_err());
    assert!(f
        .store
        .business_authorize(&actor, f.binding, "pms.command.RECORD_COLLECTION")
        .await
        .is_err());
    let grant = f
        .store
        .business_configure(
            &f.actor,
            &command(
                version().await?,
                BusinessConfigChange::GrantAccountOperation {
                    binding: f.binding,
                    account,
                    role: "operator".into(),
                    operation: "pms.command.RECORD_COLLECTION".into(),
                    valid_until: None,
                },
            ),
            true,
        )
        .await?;
    let grant: Uuid = serde_json::from_value(grant["change"]["grant"].clone())?;
    assert_eq!(
        f.store
            .business_authorize(&actor, f.binding, "pms.command.RECORD_COLLECTION")
            .await?
            .operation_grant,
        grant
    );
    let other_scope:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,parent_scope_id,label,kind) VALUES($1,$2,'模拟其他物业','business') RETURNING id")
        .bind(&f.store.tenant).bind(scope).fetch_one(&f.store.pool).await?;
    let other_binding:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) VALUES($1,$2,'simulated-other','other_property') RETURNING id")
        .bind(&f.store.tenant).bind(other_scope).fetch_one(&f.store.pool).await?;
    assert!(f
        .store
        .business_authorize(&actor, other_binding, "pms.command.RECORD_COLLECTION")
        .await
        .is_err());
    let turn = HostTurn {
        platform: "wecom".into(),
        chat_type: "direct".into(),
        chat_id: "shared_chat".into(),
        sender_id: sender.into(),
        message_id: "shared-instruction".into(),
        text: "请为订单「order_1」登记企微收款12.00元，流水号「pay_1」。".into(),
    };
    f.store.business_capture_turn(&gateway, &turn).await?;
    let input = json!({"orderId":"order_1","method":"WECOM","amountMinor":1200,"transactionReference":"pay_1"});
    let action=f.store.business_invoke(&actor,&turn.message_id,&turn.chat_id,"pms_start",&json!({"binding":f.binding,"operation":"pms.command.RECORD_COLLECTION","input":input,"reason":{"code":"OPERATOR_REQUEST","note":"模拟交办"}})).await?;
    let claim = f
        .store
        .business_invoke(
            &actor,
            &turn.message_id,
            &turn.chat_id,
            "pms_claim_preview",
            &json!({"action":action["action"]}),
        )
        .await?;
    let preview = json!({"previewId":"preview_shared","propertyId":"property_a","commandType":"RECORD_COLLECTION","effectHash":"a".repeat(64),"effect":{"orderId":"order_1","method":"WECOM","amountMinor":1200,"transactionReference":"pay_1","currency":"CNY","note":""},"expiresAt":(Utc::now()+Duration::minutes(10)).to_rfc3339()});
    let saved = f
        .store
        .business_invoke(
            &actor,
            &turn.message_id,
            &turn.chat_id,
            "pms_save_preview",
            &json!({"action":action["action"],"claim":claim["claim"],"preview":preview}),
        )
        .await?;
    assert_eq!(saved["confirmation_reused"], true);
    let executing = f
        .store
        .business_invoke(
            &actor,
            &turn.message_id,
            &turn.chat_id,
            "pms_claim_execute",
            &json!({"action":action["action"]}),
        )
        .await?;
    assert_eq!(
        executing["execution_key"],
        format!("anan_execute_{}", action["action"].as_str().unwrap())
    );
    assert!(f
        .store
        .business_invoke(
            &actor,
            &turn.message_id,
            &turn.chat_id,
            "pms_claim_execute",
            &json!({"action":action["action"]})
        )
        .await
        .is_err());
    let unknown=f.store.business_invoke(&actor,&turn.message_id,&turn.chat_id,"pms_save_result",&json!({"action":action["action"],"claim":executing["claim"],"result":{"executionStatus":"UNKNOWN"},"readback":null})).await?;
    assert_eq!(unknown["phase"], "unknown");
    let recovery = f
        .store
        .business_invoke(
            &actor,
            &turn.message_id,
            &turn.chat_id,
            "pms_recovery",
            &json!({"action":action["action"]}),
        )
        .await?;
    assert_eq!(recovery["execution_key"], executing["execution_key"]);
    f.store
        .business_configure(
            &f.actor,
            &command(
                version().await?,
                BusinessConfigChange::RevokeOperation { grant },
            ),
            true,
        )
        .await?;
    assert!(f
        .store
        .business_invoke(
            &actor,
            &turn.message_id,
            &turn.chat_id,
            "pms_recovery",
            &json!({"action":action["action"]})
        )
        .await
        .is_err());
    assert!(f
        .store
        .business_authorize(&actor, f.binding, "pms.command.RECORD_COLLECTION")
        .await
        .is_err());
    let expiring = f
        .store
        .business_configure(
            &f.actor,
            &command(
                version().await?,
                BusinessConfigChange::GrantAccountOperation {
                    binding: f.binding,
                    account,
                    role: "operator".into(),
                    operation: "pms.command.RECORD_COLLECTION".into(),
                    valid_until: Some(Utc::now() + Duration::minutes(1)),
                },
            ),
            true,
        )
        .await?;
    let expiring: Uuid = serde_json::from_value(expiring["change"]["grant"].clone())?;
    sqlx::query("UPDATE qintopia_agent_os.business_operation_grants SET valid_until=clock_timestamp()-interval '1 second' WHERE id=$1")
        .bind(expiring).execute(&f.store.pool).await?;
    assert!(f
        .store
        .business_authorize(&actor, f.binding, "pms.command.RECORD_COLLECTION")
        .await
        .is_err());
    let account_version: i64 =
        sqlx::query_scalar("SELECT version FROM qintopia_identity.work_accounts WHERE id=$1")
            .bind(account)
            .fetch_one(&f.store.pool)
            .await?;
    f.store
        .business_configure(
            &f.actor,
            &command(
                version().await?,
                BusinessConfigChange::DisableAccount {
                    account,
                    expected_account_version: account_version,
                },
            ),
            true,
        )
        .await?;
    assert!(f
        .store
        .business_gateway_actor(&gateway, sender)
        .await
        .is_err());
    assert!(f
        .store
        .business_authorize(&actor, f.binding, "pms.command.RECORD_COLLECTION")
        .await
        .is_err());
    f.store
        .business_configure(
            &f.actor,
            &command(
                version().await?,
                BusinessConfigChange::RegisterAccount {
                    gateway: gateway.clone(),
                    source_link: link,
                    label: "模拟共用账号".into(),
                },
            ),
            true,
        )
        .await?;
    let fresh = f.store.business_gateway_actor(&gateway, sender).await?;
    assert!(f
        .store
        .business_authorize(&fresh, f.binding, "pms.command.RECORD_COLLECTION")
        .await
        .is_err());
    assert!(f
        .store
        .business_authorize(&actor, f.binding, "pms.command.RECORD_COLLECTION")
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn business_empty_exact_scope_and_cross_property_fail_closed() -> Result<()> {
    let f = Fixture::new().await?;
    assert!(f
        .store
        .business_authorize(&f.actor, f.binding, "pms.command.CREATE_ORDER")
        .await
        .is_err());
    f.allow("CREATE_ORDER").await?;
    assert_eq!(
        f.store
            .business_authorize(&f.actor, f.binding, "pms.command.CREATE_ORDER")
            .await?
            .property,
        "property_a"
    );
    assert!(f
        .store
        .business_authorize(&f.actor, f.binding, "pms.command.RECORD_COLLECTION")
        .await
        .is_err());
    assert!(f
        .store
        .business_authorize(&f.actor, Uuid::new_v4(), "pms.command.CREATE_ORDER")
        .await
        .is_err());
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked' WHERE id=$1")
        .bind(f.grant)
        .execute(&f.store.pool)
        .await?;
    assert!(f
        .store
        .business_authorize(&f.actor, f.binding, "pms.command.CREATE_ORDER")
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn business_host_evidence_confirmation_and_concurrent_claim() -> Result<()> {
    let f = Fixture::new().await?;
    let (a, p) = f.prepared().await?;
    assert!(f
        .call(
            "start",
            "pms_claim_execute",
            json!({"action":a["action"],"approved":true})
        )
        .await
        .is_err());
    f.capture(
        "yes",
        &format!("确认方案 {}", p["confirmation_code"].as_str().unwrap()),
    )
    .await?;
    let args = json!({"action":a["action"]});
    let (left, right) = tokio::join!(
        f.call("yes", "pms_claim_execute", args.clone()),
        f.call("yes", "pms_claim_execute", args)
    );
    assert_ne!(left.is_ok(), right.is_ok());
    assert!(f
        .call("yes", "pms_cancel", json!({"action":a["action"]}))
        .await
        .is_err());
    let recovery = f
        .call("yes", "pms_recovery", json!({"action":a["action"]}))
        .await?;
    let first = left.or(right)?;
    assert_eq!(first["execution_key"], recovery["execution_key"]);
    assert!(f.capture("yes", "不同内容").await.is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn business_restart_unknown_reuses_key_and_preserves_independent_results() -> Result<()> {
    let f = Fixture::new().await?;
    let (a, p) = f.prepared().await?;
    f.capture(
        "confirm",
        &format!("确认方案 {}", p["confirmation_code"].as_str().unwrap()),
    )
    .await?;
    let claim = f
        .call(
            "confirm",
            "pms_claim_execute",
            json!({"action":a["action"]}),
        )
        .await?;
    f.call("confirm","pms_save_result",json!({"action":a["action"],"claim":claim["claim"],"result":{"executionStatus":"UNKNOWN","businessCommitted":false}})).await?;
    let reopened = Store::local(
        &crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?,
        &f.store.tenant,
    )
    .await?;
    let actor = reopened.gateway_actor(&f.gateway, &f.sender).await?;
    let recovered = reopened
        .business_invoke(
            &actor,
            "confirm",
            "synthetic_chat",
            "pms_recovery",
            &json!({"action":a["action"]}),
        )
        .await?;
    assert_eq!(recovered["execution_key"], claim["execution_key"]);
    let completed=f.call("confirm","pms_save_result",json!({"action":a["action"],"claim":claim["claim"],"result":{"executionStatus":"EXECUTED","businessCommitted":true,"receiptId":"receipt_1","commandId":"command_1"},"readback":{"order":{"id":"order_1","property_id":"property_a","version":1}}})).await?;
    assert_eq!(completed["phase"], "completed");
    assert!(f
        .call(
            "confirm",
            "pms_claim_execute",
            json!({"action":a["action"]})
        )
        .await
        .is_err());
    f.allow("RECORD_COLLECTION").await?;
    f.capture("collect", "登记这笔收款").await?;
    let second=f.call("collect","pms_start",json!({"binding":f.binding,"operation":"pms.command.RECORD_COLLECTION","work_item":a["work_item"],"input":{"orderId":"order_1","amountMinor":10000},"reason":{"code":"OPERATOR_REQUEST","note":"模拟测试"}})).await?;
    assert_ne!(second["action"], a["action"]);
    assert_eq!(
        f.call("collect", "pms_status", json!({"action":a["action"]}))
            .await?["phase"],
        "completed"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn business_confirmation_cannot_survive_grant_expiry() -> Result<()> {
    let f = Fixture::new().await?;
    let (a, p) = f.prepared().await?;
    f.capture(
        "confirm",
        &format!("确认方案 {}", p["confirmation_code"].as_str().unwrap()),
    )
    .await?;
    sqlx::query("UPDATE qintopia_agent_os.business_operation_grants SET valid_until=clock_timestamp()-interval '1 second' WHERE tenant_key=$1").bind(&f.store.tenant).execute(&f.store.pool).await?;
    assert!(f
        .call(
            "confirm",
            "pms_claim_execute",
            json!({"action":a["action"]})
        )
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn business_natural_confirmation_pause_resume_cancel_and_ambiguity() -> Result<()> {
    let f = Fixture::new().await?;
    let (a, p) = f.prepared().await?;
    let other=f.call("start","pms_start",json!({"binding":f.binding,"operation":"pms.command.CREATE_ORDER","input":{"quoteId":"other"},"reason":{"code":"CREATE_STANDARD_ORDER","note":""}})).await?;
    let oc = f
        .call(
            "start",
            "pms_claim_preview",
            json!({"action":other["action"]}),
        )
        .await?;
    f.call(
        "start",
        "pms_save_preview",
        json!({"action":other["action"],"claim":oc["claim"],"preview":p["preview"]}),
    )
    .await?;
    f.capture("ambiguous", "确认预订").await?;
    assert!(f
        .call(
            "ambiguous",
            "pms_claim_execute",
            json!({"action":a["action"]})
        )
        .await
        .is_err());
    f.call("ambiguous", "pms_cancel", json!({"action":other["action"]}))
        .await?;
    f.capture("natural", "确认预订").await?;
    f.call("natural", "pms_pause", json!({"action":a["action"]}))
        .await?;
    assert!(f
        .call(
            "natural",
            "pms_claim_execute",
            json!({"action":a["action"]})
        )
        .await
        .is_err());
    f.call("natural", "pms_resume", json!({"action":a["action"]}))
        .await?;
    let claim = f
        .call(
            "natural",
            "pms_claim_preview",
            json!({"action":a["action"]}),
        )
        .await?;
    let next = f
        .call(
            "natural",
            "pms_save_preview",
            json!({"action":a["action"],"claim":claim["claim"],"preview":p["preview"]}),
        )
        .await?;
    assert_ne!(p["confirmation_code"], next["confirmation_code"]);
    assert!(f
        .call(
            "natural",
            "pms_claim_execute",
            json!({"action":a["action"]})
        )
        .await
        .is_err());
    f.capture("fresh", "确认预订").await?;
    let executed = f
        .call("fresh", "pms_claim_execute", json!({"action":a["action"]}))
        .await?;
    assert!(f
        .call("fresh", "pms_cancel", json!({"action":a["action"]}))
        .await
        .is_err());
    f.call("fresh","pms_save_result",json!({"action":a["action"],"claim":executed["claim"],"result":{"executionStatus":"NOT_EXECUTED","businessCommitted":false,"receiptId":"not_executed"}})).await?;
    // A separate pending action may be cancelled without touching the first PMS receipt.
    f.capture("second", "另一个预订").await?;
    let second=f.call("second","pms_start",json!({"binding":f.binding,"operation":"pms.command.CREATE_ORDER","input":{"quoteId":"q2"},"reason":{"code":"CREATE_STANDARD_ORDER","note":""}})).await?;
    f.call("second", "pms_cancel", json!({"action":second["action"]}))
        .await?;
    assert!(f
        .call(
            "second",
            "pms_claim_preview",
            json!({"action":second["action"]})
        )
        .await
        .is_err());
    assert_eq!(
        f.call("second", "pms_status", json!({"action":a["action"]}))
            .await?["phase"],
        "not_executed"
    );
    Ok(())
}

/// Opt-in complete local chain: real broker + real PMS service + official Hermes ContextVars.
pub(crate) async fn business_real_broker_pms_journey() -> Result<()> {
    anyhow::ensure!(
        std::env::var("ANAN_FULL_CHAIN").as_deref() == Ok("1"),
        "explicit_full_chain_required"
    );
    let f = Fixture::new().await?;
    let demo: Value = serde_json::from_str(&std::env::var("ANAN_PMS_TEST_DEMO")?)?;
    sqlx::query(
        "UPDATE qintopia_agent_os.business_property_bindings SET property_id=$2 WHERE id=$1",
    )
    .bind(f.binding)
    .bind(demo["propertyId"].as_str().unwrap())
    .execute(&f.store.pool)
    .await?;
    for command in [
        "CREATE_ORDER",
        "RECORD_COLLECTION",
        "CHECK_IN",
        "CHECK_OUT",
        "RESCHEDULE_STAY",
        "MOVE_UNIT",
        "EXTEND_STAY",
        "SHORTEN_STAY",
        "CANCEL_ORDER",
    ] {
        f.allow(command).await?;
    }
    let read:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id,decision_mode) SELECT tenant_key,collaboration_id,'read_business',parent_grant_id,'autonomous' FROM qintopia_agent_os.collaboration_grants WHERE id=$1 RETURNING id")
        .bind(f.grant).fetch_one(&f.store.pool).await?;
    for key in [
        "pms.quote",
        "pms.read.availability",
        "pms.read.order",
        "pms.read.orders",
    ] {
        sqlx::query("INSERT INTO qintopia_agent_os.business_operation_grants(tenant_key,authority_grant_id,binding_id,operation_key,issued_by) VALUES($1,$2,$3,$4,$5)")
            .bind(&f.store.tenant).bind(read).bind(f.binding).bind(key).bind(f.store.verified_person(&f.actor).await?).execute(&f.store.pool).await?;
    }
    let dir = tempfile::tempdir()?;
    let application_record = format!("rec{}", Uuid::new_v4().simple());
    let opened = f
        .store
        .application_read_open(f.binding, "resident-application", &application_record)
        .await?;
    f.store
        .application_read_save(
            f.binding,
            "resident-application",
            &application_record,
            serde_json::from_value(opened["read_token"].clone())?,
            &super::store::applications::Observation {
                identity_hash: "a".repeat(64),
                field_hash: "b".repeat(64),
                valid: true,
                consent_active: true,
                source_version: Some("1".into()),
            },
        )
        .await?;
    let application_work:Uuid=sqlx::query_scalar("SELECT anan_work_id FROM qintopia_agent_os.application_intake_states WHERE tenant_key=$1 AND record_ref=$2")
        .bind(&f.store.tenant).bind(&application_record).fetch_one(&f.store.pool).await?;
    let socket = dir.path().join("foundation.sock");
    // This ignored test is launched alone; never mutates a running service's environment.
    for (key, value) in [
        (
            "QINTOPIA_FOUNDATION_SOCKET",
            socket.to_str().unwrap().to_owned(),
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
        ("ANAN_PMS_TEST_BINDING", f.binding.to_string()),
        ("ANAN_PMS_TEST_SENDER", f.sender.clone()),
        (
            "ANAN_PMS_TEST_APPLICATION_WORK",
            application_work.to_string(),
        ),
    ] {
        std::env::set_var(key, value);
    }
    let store = Store::local(
        &crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?,
        &f.store.tenant,
    )
    .await?;
    let broker = tokio::spawn(super::foundation_server::broker(store));
    for _ in 0..100 {
        if socket.exists() {
            break;
        }
        tokio::time::sleep(std::time::Duration::from_millis(10)).await;
    }
    anyhow::ensure!(socket.exists(), "broker_not_ready");
    let status = tokio::task::spawn_blocking(|| {
        std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../skills/pms-operations/tests/full_broker_journey.py"
            ))
            .status()
    })
    .await??;
    broker.abort();
    anyhow::ensure!(status.success(), "full_broker_journey_failed");
    let rows:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.business_actions WHERE tenant_key=$1 AND phase='completed' AND readback IS NOT NULL")
        .bind(&f.store.tenant).fetch_one(&f.store.pool).await?;
    anyhow::ensure!(rows >= 2, "actual_pms_readback_required");
    println!("Full chain persisted results and actual PMS readbacks verified.");
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn business_delegation_cannot_expand_and_parent_revocation_propagates() -> Result<()> {
    let f = Fixture::new().await?;
    let parent = f.allow("CREATE_ORDER").await?;
    let link:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND source_ref='fixture-person-1'")
        .bind(&f.store.tenant).fetch_one(&f.store.pool).await?;
    let colleague = f.store.actor(link).await?;
    let person = f.store.verified_person(&colleague).await?;
    let appointment:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_appointments(tenant_key,person_id,role_id,scope_id) SELECT a.tenant_key,$2,a.role_id,a.scope_id FROM qintopia_agent_os.collaboration_appointments a JOIN qintopia_agent_os.agent_collaborations c ON c.appointment_id=a.id JOIN qintopia_agent_os.collaboration_grants g ON g.collaboration_id=c.id WHERE g.id=$1 RETURNING id")
        .bind(f.grant).bind(person).fetch_one(&f.store.pool).await?;
    let relation:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key,responsibility_text) VALUES($1,$2,'anan','hospitality','Explicit synthetic delegate') RETURNING id")
        .bind(&f.store.tenant).bind(appointment).fetch_one(&f.store.pool).await?;
    let target:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id,decision_mode) SELECT tenant_key,$2,action_key,parent_grant_id,'autonomous' FROM qintopia_agent_os.collaboration_grants WHERE id=$1 RETURNING id")
        .bind(f.grant).bind(relation).fetch_one(&f.store.pool).await?;
    assert!(f
        .store
        .business_delegate(
            &f.actor,
            target,
            f.binding,
            "pms.command.RECORD_COLLECTION",
            Utc::now() + Duration::hours(1)
        )
        .await
        .is_err());
    f.store
        .business_delegate(
            &f.actor,
            target,
            f.binding,
            "pms.command.CREATE_ORDER",
            Utc::now() + Duration::hours(1),
        )
        .await?;
    f.store
        .business_authorize(&colleague, f.binding, "pms.command.CREATE_ORDER")
        .await?;
    sqlx::query("UPDATE qintopia_agent_os.business_operation_grants SET revoked_at=clock_timestamp() WHERE id=$1").bind(parent).execute(&f.store.pool).await?;
    assert!(f
        .store
        .business_authorize(&colleague, f.binding, "pms.command.CREATE_ORDER")
        .await
        .is_err());
    Ok(())
}

// Payment inbox tests share only the existing isolated tenant fixture, not PMS decisions.
fn payment(
    sequence: i64,
    bill: &str,
    kind: &str,
    event_type: &str,
) -> super::store::business_events::PaymentEvent {
    super::store::business_events::PaymentEvent {
        event_id: format!("payment:{bill}:{event_type}"),
        bill_id: bill.into(),
        kind: kind.into(),
        event_type: event_type.into(),
        occurred_at: Utc::now(),
        sequence: sequence.to_string(),
    }
}
fn payment_head(value: &str) -> super::store::business_events::PaymentHead {
    super::store::business_events::PaymentHead {
        schema_version: "pms.payments.v1".into(),
        source_instance: "synthetic-pms".into(),
        property_id: "property_a".into(),
        binding_version: 1,
        head_cursor: value.into(),
    }
}
fn payment_page(
    events: Vec<super::store::business_events::PaymentEvent>,
    end: &str,
) -> super::store::business_events::PaymentPage {
    super::store::business_events::PaymentPage {
        schema_version: "pms.payments.v1".into(),
        property_id: "property_a".into(),
        events,
        next_cursor: end.into(),
    }
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn payment_nonzero_baseline_is_atomic_concurrent_and_restart_safe() -> Result<()> {
    let f = Fixture::new().await?;
    assert!(f.store.business_feed_open(f.binding, None).await.is_err());
    let h = payment_head("42");
    let (a, b) = tokio::join!(
        f.store.business_feed_open(f.binding, Some(&h)),
        f.store.business_feed_open(f.binding, Some(&h))
    );
    assert_eq!(a?["cursor"], "42");
    assert_eq!(b?["baselineCursor"], "42");
    assert!(f
        .store
        .business_feed_open(f.binding, Some(&payment_head("43")))
        .await
        .is_err());
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let restarted = Store::local(&database, &f.store.tenant).await?;
    assert_eq!(
        restarted.business_feed_open(f.binding, None).await?["cursor"],
        "42"
    );
    restarted
        .business_accept_payment(
            f.binding,
            "synthetic-pms",
            "property_a",
            &payment(41, "old", "COLLECTION", "DISCOVERED"),
        )
        .await?;
    let n:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND work_item_id IS NOT NULL")
        .bind(&f.store.tenant).fetch_one(&f.store.pool).await?;
    assert_eq!(n, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn payment_push_feed_deduplicate_and_gaps_roll_back_atomically() -> Result<()> {
    let f = Fixture::new().await?;
    f.store
        .business_feed_open(f.binding, Some(&payment_head("40")))
        .await?;
    let a = payment(41, "bill_a", "COLLECTION", "DISCOVERED");
    let b = payment(42, "bill_b", "COLLECTION", "DISCOVERED");
    let ack = f
        .store
        .business_accept_payment(f.binding, "synthetic-pms", "property_a", &b)
        .await?;
    assert_eq!(
        f.store.business_feed_open(f.binding, None).await?["cursor"],
        "40"
    );
    assert!(f
        .store
        .business_feed_page(
            f.binding,
            "synthetic-pms",
            "40",
            payment_page(
                vec![a.clone(), payment(43, "gap", "COLLECTION", "DISCOVERED")],
                "43"
            )
        )
        .await
        .is_err());
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1",
    )
    .bind(&f.store.tenant)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(n, 1);
    let page = f
        .store
        .business_feed_page(
            f.binding,
            "synthetic-pms",
            "40",
            payment_page(vec![a.clone(), b.clone()], "42"),
        )
        .await?;
    assert_eq!(page["receipts"][1]["receipt_id"], ack["receipt_id"]);
    let duplicate = f
        .store
        .business_accept_payment(f.binding, "synthetic-pms", "property_a", &a)
        .await?;
    assert_eq!(duplicate["receipt_id"], page["receipts"][0]["receipt_id"]);
    assert_eq!(duplicate["status"], "duplicate");
    let mut conflict = a.clone();
    conflict.kind = "REFUND".into();
    assert!(f
        .store
        .business_accept_payment(f.binding, "synthetic-pms", "property_a", &conflict)
        .await
        .is_err());
    assert!(f
        .store
        .business_accept_payment(
            f.binding,
            "synthetic-pms",
            "property_a",
            &payment(42, "other", "COLLECTION", "DISCOVERED")
        )
        .await
        .is_err());
    let n:i64=sqlx::query_scalar("SELECT count(DISTINCT work_item_id) FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1").bind(&f.store.tenant).fetch_one(&f.store.pool).await?;
    assert_eq!(n, 2);
    let actions: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.business_actions WHERE tenant_key=$1",
    )
    .bind(&f.store.tenant)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(actions, 0); // Receipt/event never manufactures a financial command or human approval.
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn payment_matched_first_refund_and_scope_do_not_create_collection_authority() -> Result<()> {
    let f = Fixture::new().await?;
    f.store
        .business_feed_open(f.binding, Some(&payment_head("9")))
        .await?;
    for e in [
        payment(11, "already", "COLLECTION", "MATCHED"),
        payment(10, "already", "COLLECTION", "DISCOVERED"),
        payment(12, "refund", "REFUND", "DISCOVERED"),
    ] {
        f.store
            .business_accept_payment(f.binding, "synthetic-pms", "property_a", &e)
            .await?;
    }
    let e = payment(13, "new", "COLLECTION", "DISCOVERED");
    assert!(f
        .store
        .business_accept_payment(f.binding, "other-source", "property_a", &e)
        .await
        .is_err());
    assert!(f
        .store
        .business_accept_payment(f.binding, "synthetic-pms", "other-property", &e)
        .await
        .is_err());
    let n:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1 AND work_item_id IS NOT NULL").bind(&f.store.tenant).fetch_one(&f.store.pool).await?;
    assert_eq!(n, 0);
    sqlx::query(
        "UPDATE qintopia_agent_os.business_property_bindings SET version=version+1 WHERE id=$1",
    )
    .bind(f.binding)
    .execute(&f.store.pool)
    .await?;
    assert!(f.store.business_feed_context(f.binding).await.is_err());
    assert!(f
        .store
        .business_accept_payment(f.binding, "synthetic-pms", "property_a", &e)
        .await
        .is_err());
    Ok(())
}

fn signed_payment_request(raw: Vec<u8>, delivery: &str) -> crate::local_http::Request {
    use hmac::{Hmac, Mac};
    use sha2::Sha256;
    let mut mac = Hmac::<Sha256>::new_from_slice(b"synthetic-payment-signing-secret-32").unwrap();
    mac.update(
        format!(
            "POST\n/api/v1/ingress/pms/events\n1000\n{delivery}\n{}",
            super::digest(&raw)
        )
        .as_bytes(),
    );
    let signature = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|v| format!("{v:02x}"))
        .collect::<String>();
    crate::local_http::Request {
        method: "POST".into(),
        path: crate::resident_welcome::protocol::PATH.into(),
        body: raw,
        headers: [
            ("content-type", "application/json".into()),
            ("x-qt-key-id", "test".into()),
            ("x-qt-sent-at", "1000".into()),
            ("x-qt-delivery-id", delivery.into()),
            ("x-qt-signature", signature),
        ]
        .into_iter()
        .map(|(k, v)| (k.into(), v))
        .collect(),
    }
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn payment_signed_ingress_ack_is_durable_and_strict() -> Result<()> {
    use super::business_ingress::{receive, Config};
    use crate::resident_welcome::protocol::SigningKey;
    let f = Fixture::new().await?;
    let config = Config {
        binding: f.binding,
        key: SigningKey {
            key_id: "test".into(),
            secret: zeroize::Zeroizing::new(b"synthetic-payment-signing-secret-32".to_vec()),
            source_instance: "synthetic-pms".into(),
            properties: ["property_a".into()].into_iter().collect(),
        },
    };
    let mut envelope = serde_json::to_value(payment(43, "bill", "COLLECTION", "DISCOVERED"))?;
    envelope.as_object_mut().unwrap().extend(serde_json::from_value::<serde_json::Map<String,Value>>(json!({"schemaVersion":"pms.payments.v1","sourceInstance":"synthetic-pms","propertyId":"property_a"}))?);
    let raw = serde_json::to_vec(&envelope)?;
    assert_eq!(
        receive(
            &f.store,
            &config,
            &signed_payment_request(raw.clone(), "one"),
            1000
        )
        .await
        .status,
        503
    );
    f.store
        .business_feed_open(f.binding, Some(&payment_head("42")))
        .await?;
    let accepted = receive(
        &f.store,
        &config,
        &signed_payment_request(raw.clone(), "one"),
        1000,
    )
    .await;
    assert_eq!(accepted.status, 202);
    let receipt: Uuid = serde_json::from_value(accepted.body["receipt_id"].clone())?;
    let exists: bool = sqlx::query_scalar(
        "SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.business_event_inbox WHERE id=$1)",
    )
    .bind(receipt)
    .fetch_one(&f.store.pool)
    .await?;
    assert!(exists);
    let repeated = receive(
        &f.store,
        &config,
        &signed_payment_request(raw.clone(), "new-delivery"),
        1000,
    )
    .await;
    assert_eq!(repeated.status, 200);
    assert_eq!(repeated.body["receipt_id"], accepted.body["receipt_id"]);
    assert_eq!(
        receive(
            &f.store,
            &config,
            &signed_payment_request(raw.clone(), "old"),
            1301
        )
        .await
        .status,
        401
    );
    let mut bad = signed_payment_request(raw.clone(), "bad");
    bad.body.push(b' ');
    assert_eq!(receive(&f.store, &config, &bad, 1000).await.status, 401);
    envelope["kind"] = json!("REFUND");
    assert_eq!(
        receive(
            &f.store,
            &config,
            &signed_payment_request(serde_json::to_vec(&envelope)?, "conflict"),
            1000
        )
        .await
        .status,
        409
    );
    envelope["propertyId"] = json!("other");
    assert_eq!(
        receive(
            &f.store,
            &config,
            &signed_payment_request(serde_json::to_vec(&envelope)?, "scope"),
            1000
        )
        .await
        .status,
        403
    );
    let duplicate = b"{\"extra\":{\"a\":1,\"a\":2}}".to_vec();
    assert_eq!(
        receive(
            &f.store,
            &config,
            &signed_payment_request(duplicate, "duplicate-json"),
            1000
        )
        .await
        .status,
        400
    );
    let mut wrong_method = signed_payment_request(raw, "method");
    wrong_method.method = "GET".into();
    assert_eq!(
        receive(&f.store, &config, &wrong_method, 1000).await.status,
        404
    );
    Ok(())
}

pub(crate) async fn payment_joint_setup() -> Result<()> {
    anyhow::ensure!(
        std::env::var("QINTOPIA_PAYMENT_JOINT_ENABLE").as_deref() == Ok("1"),
        "joint_disabled"
    );
    let output = std::path::PathBuf::from(std::env::var("QINTOPIA_PAYMENT_JOINT_CONFIG")?);
    anyhow::ensure!(
        output.is_absolute() && !output.exists(),
        "existing_joint_configuration_preserved"
    );
    let source = std::env::var("QINTOPIA_PAYMENT_JOINT_SOURCE")?;
    let property = std::env::var("QINTOPIA_PAYMENT_JOINT_PROPERTY")?;
    anyhow::ensure!(
        source.starts_with("synthetic-") && crate::resident_welcome::protocol::reference(&property),
        "synthetic_source_required"
    );
    let f = Fixture::new().await?;
    sqlx::query("UPDATE qintopia_agent_os.business_property_bindings SET source_instance=$2,property_id=$3 WHERE id=$1")
        .bind(f.binding).bind(&source).bind(&property).execute(&f.store.pool).await?;
    f.allow("RECORD_COLLECTION").await?;
    let read:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id,decision_mode) SELECT tenant_key,collaboration_id,'read_business',parent_grant_id,'autonomous' FROM qintopia_agent_os.collaboration_grants WHERE id=$1 RETURNING id")
        .bind(f.grant).fetch_one(&f.store.pool).await?;
    for key in ["pms.read.payments", "pms.read.order", "pms.read.orders"] {
        sqlx::query("INSERT INTO qintopia_agent_os.business_operation_grants(tenant_key,authority_grant_id,binding_id,operation_key,issued_by) VALUES($1,$2,$3,$4,$5)")
            .bind(&f.store.tenant).bind(read).bind(f.binding).bind(key).bind(f.store.verified_person(&f.actor).await?).execute(&f.store.pool).await?;
    }
    use std::io::Write;
    let mut file = std::fs::OpenOptions::new()
        .write(true)
        .create_new(true)
        .open(output)?;
    file.write_all(&serde_json::to_vec(&json!({"tenant":f.store.tenant,"binding":f.binding,"gateway":f.gateway,"sender":f.sender,"sourceInstance":source,"propertyId":property}))?)?;
    file.sync_all()?;
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn payment_work_attachment_requires_exact_authority_readback_and_human_confirmation(
) -> Result<()> {
    let f = Fixture::new().await?;
    f.store
        .business_feed_open(f.binding, Some(&payment_head("42")))
        .await?;
    f.store
        .business_accept_payment(
            f.binding,
            "synthetic-pms",
            "property_a",
            &payment(43, "bill", "COLLECTION", "DISCOVERED"),
        )
        .await?;
    let work: Uuid = sqlx::query_scalar(
        "SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1",
    )
    .bind(&f.store.tenant)
    .fetch_one(&f.store.pool)
    .await?;
    f.capture("source", "请核对这笔收款").await?;
    let args =
        json!({"binding":f.binding,"operation":"pms.command.RECORD_COLLECTION","work_item":work});
    assert!(f
        .call("source", "pms_event_context", args.clone())
        .await
        .is_err());
    f.allow("RECORD_COLLECTION").await?;
    assert_eq!(
        f.call("source", "pms_event_context", args.clone()).await?["bill_id"],
        "bill"
    );
    let mut start = args;
    start["input"] =
        json!({"orderId":"order","method":"WECOM","amountMinor":100,"transactionReference":"ref"});
    start["reason"] = json!({"code":"CUSTOMER_PAYMENT","note":"模拟核对"});
    assert!(f.call("source", "pms_start", start.clone()).await.is_err());
    start["source_payment"] = json!({"id":"bill","status":"AVAILABLE","kind":"COLLECTION","reference":"other","amountMinor":100});
    assert!(f.call("source", "pms_start", start.clone()).await.is_err());
    start["source_payment"]["reference"] = json!("ref");
    let action = f.call("source", "pms_start", start).await?;
    assert!(f
        .call(
            "source",
            "pms_claim_execute",
            json!({"action":action["action"]})
        )
        .await
        .is_err());
    let request = super::foundation_server::parse_broker_request(&serde_json::to_vec(&json!({
        "operation":"person_foundation_tool","schema_version":1,"agent":"anan","tool":"pms_payment_feed",
        "trusted_context":{"gateway_id":f.gateway,"platform":"host","chat_type":"","chat_id":"","sender_id":"","message_id":""},
        "arguments":{"action":"open","head":{"headCursor":"999"}},"token":"not-used-direct-unit"}))?)?;
    assert!(
        super::foundation_server::broker_invoke(&f.store, &f.gateway, "anan", request)
            .await
            .is_err()
    );
    assert_eq!(
        f.store.business_feed_open(f.binding, None).await?["cursor"],
        "42"
    );
    Ok(())
}
