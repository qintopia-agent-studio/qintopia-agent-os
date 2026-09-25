//! Real service/PG tests; PMS and channel readbacks below are explicitly simulated.
use super::*;
use crate::person_collaboration::KnowledgeWrite;

struct ReminderFixture {
    f: Fixture,
    work: Uuid,
    scope: Uuid,
    group: Uuid,
    collaboration: Uuid,
    rule: Value,
    owner: Actor,
}
impl ReminderFixture {
    async fn new() -> Result<Self> {
        let f = Fixture::new().await?;
        f.allow("RECORD_COLLECTION").await?;
        f.capture("reminder", "请登记模拟收款").await?;
        let started=f.call("reminder","pms_start",json!({"binding":f.binding,"operation":"pms.command.RECORD_COLLECTION","input":{"orderId":"synthetic_order","method":"CASH","amountMinor":12000,"note":"模拟客服"},"reason":{"code":"CUSTOMER_PAYMENT","note":"模拟提醒"}})).await?;
        let work = serde_json::from_value(started["work_item"].clone())?;
        let row=sqlx::query("SELECT g.collaboration_id,c.appointment_id,g.parent_grant_id,a.scope_id FROM qintopia_agent_os.collaboration_grants g JOIN qintopia_agent_os.agent_collaborations c ON c.id=g.collaboration_id JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE g.id=$1").bind(f.grant).fetch_one(&f.store.pool).await?;
        let collaboration = row.get("collaboration_id");
        let scope = row.get("scope_id");
        let read:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id,decision_mode) VALUES($1,$2,'read_business',$3,'autonomous') RETURNING id").bind(&f.store.tenant).bind(collaboration).bind(row.get::<Uuid,_>("parent_grant_id")).fetch_one(&f.store.pool).await?;
        let person = f.store.verified_person(&f.actor).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.business_operation_grants(tenant_key,authority_grant_id,binding_id,operation_key,issued_by) VALUES($1,$2,$3,'pms.read.order',$4)").bind(&f.store.tenant).bind(read).bind(f.binding).bind(person).execute(&f.store.pool).await?;
        let rules:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key,responsibility_text) VALUES($1,$2,'erhua','community_service','模拟工作约定') RETURNING id").bind(&f.store.tenant).bind(row.get::<Uuid,_>("appointment_id")).fetch_one(&f.store.pool).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id,decision_mode) VALUES($1,$2,'change_rules',$3,'autonomous')").bind(&f.store.tenant).bind(rules).bind(row.get::<Uuid,_>("parent_grant_id")).execute(&f.store.pool).await?;
        for (connection, domain, label) in [
            (rules, "community_service", "模拟规则职责"),
            (collaboration, "hospitality", "模拟客房职责"),
        ] {
            let duty:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_duties(tenant_key,label,description,domain_key,available_actions) VALUES($1,$2,'模拟',$3,ARRAY['change_rules','read_business','execute_business']) RETURNING id").bind(&f.store.tenant).bind(label).bind(domain).fetch_one(&f.store.pool).await?;
            sqlx::query("UPDATE qintopia_agent_os.agent_collaborations SET duty_id=$2 WHERE id=$1")
                .bind(connection)
                .bind(duty)
                .execute(&f.store.pool)
                .await?;
        }
        let group:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type) VALUES($1,'wecom',$2,'group') RETURNING id").bind(&f.store.tenant).bind(format!("synthetic-{}",Uuid::new_v4())).fetch_one(&f.store.pool).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3)").bind(&f.store.tenant).bind(scope).bind(group).execute(&f.store.pool).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_audiences(tenant_key,collaboration_id,configuration) VALUES($1,$2,$3)").bind(&f.store.tenant).bind(collaboration).bind(json!({"groups":[group],"proactive":"autonomous"})).execute(&f.store.pool).await?;
        let owner = f.store.gateway_actor(&f.gateway, &f.sender).await?;
        let rule = json!({"collaboration":collaboration,"group":group,"kinds":["missing_information","collection","arrival"],"required_fields":["arrival"],"weekdays":[1,2,3,4,5,6,7],"start_minute":0,"end_minute":1440,"utc_offset_minutes":480,"arrival_due_minute":0,"initial_delay_seconds":0,"repeat_seconds":3600,"escalation":null});
        Ok(Self {
            f,
            work,
            scope,
            group,
            collaboration,
            rule,
            owner,
        })
    }
    async fn save_rule(&self, version: i64, content: Value) -> Result<Value> {
        self.f
            .store
            .knowledge_save(
                &self.owner,
                &KnowledgeWrite {
                    operation_id: Uuid::new_v4(),
                    expected_version: version,
                    scope: self.scope,
                    key: "anan.pms.reminders".into(),
                    kind: "rule".into(),
                    shared: false,
                    case_ref: None,
                    content,
                    effective_at: None,
                    effective_until: None,
                },
                false,
            )
            .await
    }
    async fn call(&self, action: &str, extra: Value) -> Result<Value> {
        let mut a = json!({"action":action,"work":self.work});
        a.as_object_mut()
            .unwrap()
            .extend(extra.as_object().unwrap().clone());
        self.f
            .store
            .business_reminder(&self.f.gateway, self.f.binding, &a)
            .await
    }
    fn observed() -> Value {
        json!({"applications":[],"order":{"order":{"id":"synthetic_order","property_id":"property_a","version":1,"status":"RESERVED","arrival_date":"2026-01-01"},"amounts":{"currentContractAmount":{"currency":"CNY","minorUnits":12000},"netRecordedCollection":{"currency":"CNY","minorUnits":0}}}})
    }
    async fn claim(&self, observed: &Value) -> Result<Value> {
        let plan = self
            .call("observe", json!({"observation":observed}))
            .await?;
        self.call(
            "claim",
            json!({"observation":observed,"signature":plan["signature"]}),
        )
        .await
    }
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn reminder_rules_merge_stop_snooze_and_unknown_are_durable() -> Result<()> {
    let r = ReminderFixture::new().await?;
    let observed = ReminderFixture::observed();
    assert_eq!(
        r.call("observe", json!({"observation":observed})).await?["status"],
        "waiting_rule"
    );
    r.save_rule(0, r.rule.clone()).await?;
    let plan = r.call("observe", json!({"observation":observed})).await?;
    assert_eq!(plan["pending"], json!(["collection", "arrival"]));
    let (a, b) = tokio::join!(
        r.call(
            "claim",
            json!({"observation":observed,"signature":plan["signature"]})
        ),
        r.call(
            "claim",
            json!({"observation":observed,"signature":plan["signature"]})
        )
    );
    let a = a?;
    let b = b?;
    assert_ne!(a["status"], b["status"]);
    let claim = if a["status"] == "claimed" {
        a["claim"].clone()
    } else {
        b["claim"].clone()
    };
    let send = r
        .call("validate", json!({"observation":observed,"claim":claim}))
        .await?;
    assert_eq!(send["status"], "send");
    assert!(r
        .call("validate", json!({"observation":observed,"claim":claim}))
        .await
        .is_err());
    let reopened = Store::local(
        &crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?,
        &r.f.store.tenant,
    )
    .await?;
    assert_eq!(reopened.business_reminder(&r.f.gateway,r.f.binding,&json!({"action":"claim","work":r.work,"observation":observed,"signature":plan["signature"]})).await?["status"],"unknown");
    let mut receipt = json!({"claim":claim,"profile":"anan","simulated":true,"destination":send["plan"]["destination"],"signature":send["plan"]["signature"],"message_id":"simulated-one"});
    receipt["profile"] = json!("erhua");
    assert!(r
        .call("settle", json!({"claim":claim,"receipt":receipt}))
        .await
        .is_err());
    receipt["profile"] = json!("anan");
    assert_eq!(
        r.call("settle", json!({"claim":claim,"receipt":receipt}))
            .await?["status"],
        "sent"
    );
    assert_eq!(
        r.call("observe", json!({"observation":observed})).await?["status"],
        "waiting"
    );
    let until = (Utc::now() + Duration::hours(2)).to_rfc3339();
    r.f.capture("snooze", "将本事项提醒暂缓两小时").await?;
    r.f.call(
        "snooze",
        "pms_reminder_snooze",
        json!({"binding":r.f.binding,"work_item":r.work,"until":until}),
    )
    .await?;
    assert_eq!(
        r.call("context", json!({})).await?["state"]["snooze_until"],
        until
    );
    sqlx::query("UPDATE qintopia_agent_os.work_items SET metadata=jsonb_set(metadata,'{pms_reminder,last_sent}',to_jsonb((clock_timestamp()-interval '2 hours')::text)) WHERE id=$1").bind(r.work).execute(&r.f.store.pool).await?;
    assert_eq!(
        r.call("observe", json!({"observation":observed})).await?["status"],
        "waiting"
    );
    sqlx::query("UPDATE qintopia_agent_os.work_items SET metadata=jsonb_set(metadata,'{pms_reminder,snooze_until}',to_jsonb((clock_timestamp()-interval '1 hour')::text)) WHERE id=$1").bind(r.work).execute(&r.f.store.pool).await?;
    assert_eq!(
        r.call("observe", json!({"observation":observed})).await?["status"],
        "due"
    );
    let mut done = observed.clone();
    done["order"]["order"]["status"] = json!("CHECKED_IN");
    done["order"]["amounts"]["netRecordedCollection"]["minorUnits"] = json!(12000);
    assert_eq!(
        r.call("observe", json!({"observation":done})).await?["status"],
        "stopped"
    );
    assert_eq!(
        r.call("context", json!({})).await?["state"]["current_status"],
        "stopped"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn reminder_rechecks_rule_audience_scope_and_current_facts() -> Result<()> {
    let r = ReminderFixture::new().await?;
    let observed = ReminderFixture::observed();
    r.save_rule(0, r.rule.clone()).await?;
    let claim = r.claim(&observed).await?;
    let mut changed = observed.clone();
    changed["order"]["order"]["version"] = json!(2);
    assert_eq!(
        r.call(
            "validate",
            json!({"claim":claim["claim"],"observation":changed})
        )
        .await?["status"],
        "suppressed"
    );
    let claim = r.claim(&observed).await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_audiences SET configuration=jsonb_set(configuration,'{proactive}','\"denied\"') WHERE collaboration_id=$1").bind(r.collaboration).execute(&r.f.store.pool).await?;
    assert!(r
        .call(
            "validate",
            json!({"claim":claim["claim"],"observation":observed})
        )
        .await
        .is_err());
    assert!(r
        .f
        .store
        .business_reminder(
            "wrong-gateway",
            r.f.binding,
            &json!({"action":"context","work":r.work})
        )
        .await
        .is_err());
    let other = ReminderFixture::new().await?;
    assert!(r
        .f
        .store
        .business_reminder(
            &r.f.gateway,
            r.f.binding,
            &json!({"action":"context","work":other.work})
        )
        .await
        .is_err());
    sqlx::query("UPDATE qintopia_agent_os.collaboration_audiences SET configuration=jsonb_set(configuration,'{proactive}','\"autonomous\"') WHERE collaboration_id=$1").bind(r.collaboration).execute(&r.f.store.pool).await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_scope_bindings SET revoked_at=clock_timestamp() WHERE conversation_id=$1").bind(r.group).execute(&r.f.store.pool).await?;
    assert!(r
        .call(
            "validate",
            json!({"claim":claim["claim"],"observation":observed})
        )
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn reminder_application_completion_merge_and_rule_windows() -> Result<()> {
    let mut r = ReminderFixture::new().await?;
    r.save_rule(0, r.rule.clone()).await?;
    let original = r.work;
    let record = format!("rec{}", Uuid::new_v4().simple());
    let mut fields = json!({"name":"模拟住客","nickname":"模拟昵称","phone":"synthetic-phone","consent":true,"arrival":null});
    let mut observation = crate::person_collaboration::store::applications::Observation {
        identity_hash: crate::person_collaboration::digest(&serde_json::to_vec(
            &json!({"name":fields["name"],"nickname":fields["nickname"],"phone":fields["phone"]}),
        )?),
        field_hash: crate::person_collaboration::digest(&serde_json::to_vec(
            &json!({"fields":fields,"valid":true,"consent_active":true}),
        )?),
        valid: true,
        consent_active: true,
        source_version: Some("100".into()),
    };
    let opened =
        r.f.store
            .application_read_open(r.f.binding, "resident-application", &record)
            .await?;
    r.f.store
        .application_read_save(
            r.f.binding,
            "resident-application",
            &record,
            serde_json::from_value(opened["read_token"].clone())?,
            &observation,
        )
        .await?;
    let application_work:Uuid=sqlx::query_scalar("SELECT anan_work_id FROM qintopia_agent_os.application_intake_states WHERE tenant_key=$1 AND record_ref=$2").bind(&r.f.store.tenant).bind(&record).fetch_one(&r.f.store.pool).await?;
    r.work = application_work;
    let mut observed = json!({"order":null,"applications":[{"record":record,"resource_alias":"resident-application","source_version":"100","fields":fields,"valid":true,"consent_active":true}]});
    assert_eq!(
        r.call("observe", json!({"observation":observed})).await?["pending"],
        json!(["missing_information"])
    );
    let claim = r.claim(&observed).await?;
    fields["arrival"] = json!("2026-09-30");
    observation.source_version = Some("101".into());
    observation.field_hash = crate::person_collaboration::digest(&serde_json::to_vec(
        &json!({"fields":fields,"valid":true,"consent_active":true}),
    )?);
    let opened =
        r.f.store
            .application_read_open(r.f.binding, "resident-application", &record)
            .await?;
    r.f.store
        .application_read_save(
            r.f.binding,
            "resident-application",
            &record,
            serde_json::from_value(opened["read_token"].clone())?,
            &observation,
        )
        .await?;
    assert!(r
        .call(
            "validate",
            json!({"claim":claim["claim"],"observation":observed})
        )
        .await
        .is_err());
    observed["applications"][0]["fields"] = fields;
    observed["applications"][0]["source_version"] = json!("101");
    assert_eq!(
        r.call(
            "validate",
            json!({"claim":claim["claim"],"observation":observed})
        )
        .await?["status"],
        "suppressed"
    );
    assert_eq!(
        r.call("observe", json!({"observation":observed})).await?["status"],
        "stopped"
    );
    // Fixture supplies a previously confirmed source relation. The source-link command
    // itself is covered by the separate source-link acceptance tests.
    sqlx::query("UPDATE qintopia_agent_os.work_items SET metadata=metadata || jsonb_build_object('business_work_ref',$2::text,'business_order_ref','synthetic_order') WHERE id=$1").bind(application_work).bind(original).execute(&r.f.store.pool).await?;
    assert_eq!(
        r.call("observe", json!({"observation":observed})).await?["status"],
        "stopped"
    );
    r.work = original;
    observed["order"] = ReminderFixture::observed()["order"].clone();
    let plan = r.call("observe", json!({"observation":observed})).await?;
    assert_eq!(plan["pending"], json!(["collection", "arrival"]));
    let mut delayed = r.rule.clone();
    delayed["initial_delay_seconds"] = json!(3600);
    r.save_rule(1, delayed.clone()).await?;
    assert_eq!(
        r.call("observe", json!({"observation":observed})).await?["status"],
        "waiting"
    );
    delayed["initial_delay_seconds"] = json!(0);
    let tomorrow = (Utc::now() + Duration::minutes(480)).date_naive() + Duration::days(1);
    use chrono::Datelike;
    delayed["weekdays"] = json!([tomorrow.weekday().number_from_monday()]);
    r.save_rule(2, delayed.clone()).await?;
    assert_eq!(
        r.call("observe", json!({"observation":observed})).await?["status"],
        "waiting"
    );
    delayed["weekdays"] = json!([1, 2, 3, 4, 5, 6, 7]);
    delayed["escalation"] = json!({"after_seconds":1,"group":Uuid::new_v4()});
    r.save_rule(3, delayed).await?;
    sqlx::query("UPDATE qintopia_agent_os.work_items SET metadata=jsonb_set(metadata,'{pms_reminder,pending_since}',to_jsonb((clock_timestamp()-interval '1 hour')::text)) WHERE id=$1").bind(original).execute(&r.f.store.pool).await?;
    assert!(r
        .call(
            "claim",
            json!({"observation":observed,"signature":plan["signature"]})
        )
        .await
        .is_err());
    Ok(())
}

#[tokio::test(flavor = "multi_thread", worker_threads = 2)]
#[ignore = "run this broker integration alone with explicit task-isolated PG"]
async fn reminder_real_broker_python_host_and_simulated_channel() -> Result<()> {
    // Native PG runs include this module. Keep broker environment changes in a
    // child so both serial and parallel callers retain their original configuration.
    if std::env::var("QINTOPIA_REMINDER_TEST_CHILD").as_deref() != Ok("1") {
        let status=tokio::task::spawn_blocking(||std::process::Command::new(std::env::current_exe()?)
            .arg("person_collaboration::business_tests::business_reminder_tests::reminder_real_broker_python_host_and_simulated_channel")
            .args(["--exact","--ignored","--nocapture"])
            .env("QINTOPIA_REMINDER_TEST_CHILD","1").status()).await??;
        anyhow::ensure!(status.success(), "isolated_reminder_broker_failed");
        return Ok(());
    }
    let r = ReminderFixture::new().await?;
    r.save_rule(0, r.rule.clone()).await?;
    let dir = tempfile::tempdir()?;
    let socket = dir.path().join("foundation.sock");
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
        ("QINTOPIA_FOUNDATION_GATEWAY_ID", r.f.gateway.clone()),
        ("QINTOPIA_FOUNDATION_PROFILE", "anan".into()),
        ("QINTOPIA_FOUNDATION_LOCAL_ENABLE", "1".into()),
        ("QINTOPIA_PMS_LOCAL_ENABLE", "1".into()),
        ("QINTOPIA_PMS_REMINDERS_LOCAL_ENABLE", "1".into()),
        ("QINTOPIA_PMS_REMINDER_BINDING", r.f.binding.to_string()),
        ("ANAN_REMINDER_WORK", r.work.to_string()),
    ] {
        std::env::set_var(key, value);
    }
    let store = Store::local(
        &crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?,
        &r.f.store.tenant,
    )
    .await?;
    let broker = tokio::spawn(crate::person_collaboration::foundation_server::broker(
        store,
    ));
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
                "/../../skills/pms-operations/tests/reminder_broker_journey.py"
            ))
            .status()
    })
    .await??;
    broker.abort();
    anyhow::ensure!(status.success(), "reminder_broker_journey_failed");
    let context = r.call("context", json!({})).await?;
    assert_eq!(context["state"]["current_status"], "stopped");
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn reminder_bounded_scans_do_not_starve_later_work() -> Result<()> {
    let f = Fixture::new().await?;
    let observation = crate::person_collaboration::store::applications::Observation {
        identity_hash: "a".repeat(64),
        field_hash: "b".repeat(64),
        valid: true,
        consent_active: true,
        source_version: Some("1".into()),
    };
    for _ in 0..102 {
        let record = format!("rec{}", Uuid::new_v4().simple());
        let opened = f
            .store
            .application_read_open(f.binding, "resident-application", &record)
            .await?;
        f.store
            .application_read_save(
                f.binding,
                "resident-application",
                &record,
                serde_json::from_value(opened["read_token"].clone())?,
                &observation,
            )
            .await?;
    }
    let first = f
        .store
        .business_reminder(&f.gateway, f.binding, &json!({"action":"list"}))
        .await?;
    let second = f
        .store
        .business_reminder(&f.gateway, f.binding, &json!({"action":"list"}))
        .await?;
    let a = first["works"].as_array().unwrap();
    let b = second["works"].as_array().unwrap();
    assert_eq!(a.len(), 100);
    assert_eq!(b.len(), 100);
    assert!(!a.contains(&b[0]) && !a.contains(&b[1]));
    let union: std::collections::BTreeSet<_> =
        a.iter().chain(b).map(|v| v.as_str().unwrap()).collect();
    assert_eq!(union.len(), 102);
    let claimed:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.work_items WHERE id IN(SELECT anan_work_id FROM qintopia_agent_os.application_intake_states WHERE tenant_key=$1) AND metadata->'pms_reminder'->>'claim' IS NOT NULL").bind(&f.store.tenant).fetch_one(&f.store.pool).await?;
    assert_eq!(claimed, 0);
    Ok(())
}
