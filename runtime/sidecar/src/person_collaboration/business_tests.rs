//! Real shared-service persistence; only PMS HTTP and channel source are synthetic boundaries.
#![cfg(feature = "postgres-integration-tests")]
use super::{store::business::HostTurn, Actor, Store};
use anyhow::Result;
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

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
    let completed=f.call("confirm","pms_save_result",json!({"action":a["action"],"claim":claim["claim"],"result":{"executionStatus":"EXECUTED","businessCommitted":true,"receiptId":"receipt_1","commandId":"command_1"}})).await?;
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
