//! Two valid peers in one tenant: isolation is not tested with a broken second actor.
use super::*;

async fn peer(base: &Fixture, label: &str, person_ref: &str, suffix: &str) -> Result<Fixture> {
    let store = Store::local(
        &crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?,
        &base.store.tenant,
    )
    .await?;
    let scope: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND label=$2",
    )
    .bind(&store.tenant)
    .bind(label)
    .fetch_one(&store.pool)
    .await?;
    let person:Uuid=sqlx::query_scalar("SELECT person_id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND source_ref=$2").bind(&store.tenant).bind(person_ref).fetch_one(&store.pool).await?;
    let root=sqlx::query("SELECT a.role_id,g.id FROM qintopia_agent_os.collaboration_grants g JOIN qintopia_agent_os.agent_collaborations c ON c.id=g.collaboration_id JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE g.tenant_key=$1 AND g.parent_grant_id IS NULL AND g.action_key='manage'").bind(&store.tenant).fetch_one(&store.pool).await?;
    let appointment:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_appointments(tenant_key,person_id,role_id,scope_id) VALUES($1,$2,$3,$4) RETURNING id").bind(&store.tenant).bind(person).bind(root.get::<Uuid,_>("role_id")).bind(scope).fetch_one(&store.pool).await?;
    let collaboration:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key,responsibility_text) VALUES($1,$2,'anan','hospitality','模拟独立栋客房办理') RETURNING id").bind(&store.tenant).bind(appointment).fetch_one(&store.pool).await?;
    let grant:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id,decision_mode) VALUES($1,$2,'execute_business',$3,'autonomous') RETURNING id").bind(&store.tenant).bind(collaboration).bind(root.get::<Uuid,_>("id")).fetch_one(&store.pool).await?;
    let binding:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) VALUES($1,$2,$3,$4) RETURNING id").bind(&store.tenant).bind(scope).bind(format!("synthetic-peer-{suffix}")).bind(format!("property_peer_{suffix}")).fetch_one(&store.pool).await?;
    let gateway = format!("peer-{suffix}-{}", Uuid::new_v4());
    let sender = format!("peer_staff_{suffix}");
    sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by) VALUES($1,'wecom_internal',$2,$3,'confirmed',$4,$3)").bind(&store.tenant).bind(&sender).bind(person).bind(Uuid::new_v4()).execute(&store.pool).await?;
    sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$1,'wecom_internal',$3,'employee',true)").bind(&store.tenant).bind(&gateway).bind(scope).execute(&store.pool).await?;
    let actor = store.gateway_actor(&gateway, &sender).await?;
    let f = Fixture {
        store,
        actor,
        binding,
        grant,
        gateway,
        sender,
    };
    f.allow("RECORD_COLLECTION").await?;
    Ok(f)
}

async fn ledger(f: &Fixture) -> Result<Value> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('binding',to_jsonb(b),'inbox',(SELECT jsonb_agg(to_jsonb(i) ORDER BY i.id) FROM qintopia_agent_os.business_event_inbox i WHERE i.binding_id=b.id),'checkpoint',(SELECT jsonb_agg(to_jsonb(c)) FROM qintopia_agent_os.business_feed_checkpoints c WHERE c.binding_id=b.id),'actions',(SELECT jsonb_agg(to_jsonb(a) ORDER BY a.id) FROM qintopia_agent_os.business_actions a WHERE a.binding_id=b.id),'works',(SELECT jsonb_agg(to_jsonb(w) ORDER BY w.id) FROM qintopia_agent_os.work_items w WHERE w.id IN(SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE binding_id=b.id)),'events',(SELECT jsonb_agg(to_jsonb(e) ORDER BY e.id) FROM qintopia_agent_os.work_item_events e WHERE e.work_item_id IN(SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE binding_id=b.id))) FROM qintopia_agent_os.business_property_bindings b WHERE b.id=$1").bind(f.binding).fetch_one(&f.store.pool).await?)
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn two_valid_scopes_accept_same_external_ids_and_reject_crossed_authority() -> Result<()> {
    let base = Fixture::new().await?;
    let a = peer(&base, "一栋", "fixture-person-1", "a").await?;
    let b = peer(&base, "二栋", "fixture-person-2", "b").await?;
    let aa = a
        .store
        .business_authorize(&a.actor, a.binding, "pms.command.RECORD_COLLECTION")
        .await?;
    let bb = b
        .store
        .business_authorize(&b.actor, b.binding, "pms.command.RECORD_COLLECTION")
        .await?;
    assert_ne!(aa.scope, bb.scope);
    assert_ne!(
        a.store.verified_person(&a.actor).await?,
        b.store.verified_person(&b.actor).await?
    );
    for (f, auth) in [(&a, &aa), (&b, &bb)] {
        let mut head = payment_head("0");
        head.source_instance = auth.source.clone();
        head.property_id = auth.property.clone();
        f.store.business_feed_open(f.binding, Some(&head)).await?;
        f.capture("same_message", "请核对同额收款").await?;
    }
    let event = payment(1, "same_bill", "COLLECTION", "DISCOVERED");
    let (ar, br) = tokio::join!(
        a.store
            .business_accept_payment(a.binding, &aa.source, &aa.property, &event),
        b.store
            .business_accept_payment(b.binding, &bb.source, &bb.property, &event)
    );
    let (ar, br) = (ar?, br?);
    assert_eq!(ar["status"], "accepted");
    assert_eq!(br["status"], "accepted");
    assert_ne!(ar["receipt_id"], br["receipt_id"]);
    let mut plans = Vec::new();
    for (f, auth) in [(&a, &aa), (&b, &bb)] {
        let work: Uuid = sqlx::query_scalar(
            "SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE binding_id=$1",
        )
        .bind(f.binding)
        .fetch_one(&f.store.pool)
        .await?;
        let context=f.call("same_message","pms_event_context",json!({"binding":f.binding,"operation":"pms.command.RECORD_COLLECTION","work_item":work})).await?;
        assert_eq!(context["bill_id"], "same_bill");
        assert_eq!(context["property"], auth.property);
        let args = json!({"binding":f.binding,"operation":"pms.command.RECORD_COLLECTION","work_item":work,"input":{"orderId":"same_order","method":"WECOM","amountMinor":12000,"transactionReference":"same_reference"},"source_payment":{"id":"same_bill","status":"AVAILABLE","kind":"COLLECTION","reference":"same_reference","amountMinor":12000},"reason":{"code":"CUSTOMER_PAYMENT","note":"模拟收款"}});
        let action = f.call("same_message", "pms_start", args.clone()).await?;
        let claim = f
            .call(
                "same_message",
                "pms_claim_preview",
                json!({"action":action["action"]}),
            )
            .await?;
        let pending=f.call("same_message","pms_save_preview",json!({"action":action["action"],"claim":claim["claim"],"preview":{"previewId":"same_preview","propertyId":auth.property,"commandType":"RECORD_COLLECTION","effectHash":"a".repeat(64),"effect":{"orderId":"same_order","amountMinor":12000,"currency":"CNY","method":"WECOM","transactionReference":"same_reference"},"expiresAt":(Utc::now()+Duration::minutes(10)).to_rfc3339()}})).await?;
        assert_eq!(pending["phase"], "awaiting_confirmation");
        assert_eq!(pending["confirmation_reused"], false);
        plans.push((work, action, args));
    }
    assert_ne!(plans[0].0, plans[1].0);
    assert_ne!(plans[0].1["action"], plans[1].1["action"]);
    let before = [ledger(&a).await?, ledger(&b).await?];
    for (f, auth, other, other_auth, i, j) in [(&a, &aa, &b, &bb, 0, 1), (&b, &bb, &a, &aa, 1, 0)] {
        assert!(f
            .store
            .business_accept_payment(f.binding, &other_auth.source, &auth.property, &event)
            .await
            .is_err());
        assert!(f
            .store
            .business_accept_payment(f.binding, &auth.source, &other_auth.property, &event)
            .await
            .is_err());
        let mut switched = plans[i].2.clone();
        switched["binding"] = json!(other.binding);
        assert!(f.call("same_message", "pms_start", switched).await.is_err());
        let mut switched = plans[i].2.clone();
        switched["work_item"] = json!(plans[j].0);
        // Query must not reveal the foreign event; a mismatching source cannot attach to a new request.
        assert!(f.call("same_message","pms_event_context",json!({"binding":f.binding,"operation":"pms.command.RECORD_COLLECTION","work_item":plans[j].0})).await?["bill_id"].is_null());
        switched["input"]["transactionReference"] = json!("new_reference");
        switched["source_payment"]["reference"] = json!("new_reference");
        assert!(f.call("same_message", "pms_start", switched).await.is_err());
        assert!(f
            .store
            .business_invoke(
                &other.actor,
                "same_message",
                "synthetic_chat",
                "pms_start",
                &plans[i].2
            )
            .await
            .is_err());
        assert!(f
            .call(
                "same_message",
                "pms_status",
                json!({"action":plans[j].1["action"]})
            )
            .await
            .is_err());
        let substituted = f.store.gateway_actor(&f.gateway, &other.sender).await?;
        assert!(f
            .store
            .business_authorize(&substituted, f.binding, "pms.command.RECORD_COLLECTION")
            .await
            .is_err());
        assert_eq!(ledger(other).await?, before[j]);
        assert_eq!(ledger(f).await?, before[i]);
    }
    // A legitimate next event mutates only A; B remains a functioning positive peer.
    a.store
        .business_accept_payment(
            a.binding,
            &aa.source,
            &aa.property,
            &payment(2, "same_bill", "COLLECTION", "MATCHED"),
        )
        .await?;
    assert_eq!(ledger(&b).await?, before[1]);
    assert_eq!(
        b.call(
            "same_message",
            "pms_status",
            json!({"action":plans[1].1["action"]})
        )
        .await?["phase"],
        "awaiting_confirmation"
    );
    let a_after = ledger(&a).await?;
    b.store
        .business_accept_payment(
            b.binding,
            &bb.source,
            &bb.property,
            &payment(2, "same_bill", "COLLECTION", "MATCHED"),
        )
        .await?;
    assert_eq!(ledger(&a).await?, a_after);
    println!(
        "{}",
        json!({"two_valid_scopes":true,"same_external_ids":true,"same_amount_minor":12000,"accepted_receipts":2,"independent_work_items":2,"independent_actions":2,"crossed_authority_rejected":true,"peer_ledger_unchanged":true,"pms_http_simulated":true})
    );
    Ok(())
}
