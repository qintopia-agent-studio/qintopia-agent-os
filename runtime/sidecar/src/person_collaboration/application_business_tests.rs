//! Real transaction/authority tests; PMS receipts below are explicit boundary fixtures.
use super::*;

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn application_booking_stage_survives_revision_parallel_and_unknown() -> Result<()> {
    let f = Fixture::new().await?;
    f.allow("CREATE_ORDER").await?;
    let rec = record();
    let saved = save(&f, &rec, &observation()).await?;
    let work:Uuid=sqlx::query_scalar("SELECT anan_work_id FROM qintopia_agent_os.application_intake_states WHERE tenant_key=$1 AND record_ref=$2")
        .bind(&f.store.tenant).bind(&rec).fetch_one(&f.store.pool).await?;
    f.capture("book", "准备该申请的订房").await?;
    let args = json!({"binding":f.binding,"operation":"pms.command.CREATE_ORDER","work_item":work,"input":{"quoteId":"source-quote"},"reason":{"code":"CREATE_STANDARD_ORDER","note":""}});
    let (a, b) = tokio::join!(
        f.call("book", "pms_start", args.clone()),
        f.call("book", "pms_start", args.clone())
    );
    let a = a?;
    assert_eq!(a["action"], b?["action"]);
    let mut updated = observation();
    updated.field_hash = "c".repeat(64);
    save(&f, &rec, &updated).await?;
    f.capture("new-message", "重新核对这份申请").await?;
    assert_eq!(
        f.call("new-message", "pms_start", args.clone()).await?["action"],
        a["action"]
    );
    let claim = f
        .call("book", "pms_claim_preview", json!({"action":a["action"]}))
        .await?;
    f.call("book","pms_save_result",json!({"action":a["action"],"claim":claim["claim"],"result":{"executionStatus":"UNKNOWN","businessCommitted":false}})).await?;
    let mut changed = args.clone();
    changed["input"]["quoteId"] = json!("replacement-quote");
    assert!(f
        .call("new-message", "pms_start", changed.clone())
        .await
        .unwrap_err()
        .to_string()
        .contains("booking_stage_already_exists"));
    assert!(f
        .call("new-message", "pms_cancel", json!({"action":a["action"]}))
        .await
        .is_err());
    let reopened = Store::local(
        &crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?,
        &f.store.tenant,
    )
    .await?;
    let actor = reopened.gateway_actor(&f.gateway, &f.sender).await?;
    assert_eq!(
        reopened
            .business_invoke(&actor, "new-message", "synthetic_chat", "pms_start", &args)
            .await?["action"],
        a["action"]
    );
    // An authoritative NOT_EXECUTED permits a revised independent action.
    f.call("book","pms_save_result",json!({"action":a["action"],"claim":claim["claim"],"result":{"executionStatus":"NOT_EXECUTED","businessCommitted":false,"receiptId":"synthetic_not_executed"}})).await?;
    let replacement = f.call("new-message", "pms_start", changed).await?;
    assert_ne!(replacement["action"], a["action"]);
    assert_eq!(replacement["work_item"], json!(work));
    assert!(saved["application"].is_string());
    sqlx::query("UPDATE qintopia_agent_os.business_operation_grants SET revoked_at=clock_timestamp() WHERE tenant_key=$1")
        .bind(&f.store.tenant).execute(&f.store.pool).await?;
    assert!(f.call("new-message", "pms_start", args).await.is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn application_link_requires_current_human_proposal_and_preserves_stages() -> Result<()> {
    let f = Fixture::new().await?;
    let (a, preview) = f.prepared().await?;
    f.capture(
        "confirm",
        &format!(
            "确认方案 {}",
            preview["confirmation_code"].as_str().unwrap()
        ),
    )
    .await?;
    let claim = f
        .call(
            "confirm",
            "pms_claim_execute",
            json!({"action":a["action"]}),
        )
        .await?;
    let order = json!({"order":{"id":"synthetic-order","property_id":"property_a","version":1,"status":"CONFIRMED"}});
    f.call("confirm","pms_save_result",json!({"action":a["action"],"claim":claim["claim"],"result":{"executionStatus":"EXECUTED","businessCommitted":true,"receiptId":"synthetic-receipt","commandId":"synthetic-command"},"readback":order})).await?;
    let rec = record();
    save(&f, &rec, &observation()).await?;
    let source:Uuid=sqlx::query_scalar("SELECT anan_work_id FROM qintopia_agent_os.application_intake_states WHERE tenant_key=$1 AND record_ref=$2")
        .bind(&f.store.tenant).bind(&rec).fetch_one(&f.store.pool).await?;
    f.capture("proposal", "请核对这份申请与已订房订单的关系")
        .await?;
    let mut link = json!({"action":a["action"],"work_item":source,"readback":order});
    assert_eq!(
        f.call("proposal", "pms_link", link.clone()).await?["phase"],
        "awaiting_confirmation"
    );
    f.capture("agree-old", "确认关联").await?;
    link["readback"]["order"]["version"] = json!(2);
    assert_eq!(
        f.call("agree-old", "pms_link", link.clone()).await?["phase"],
        "awaiting_confirmation"
    );
    f.capture("agree-new", "确认关联").await?;
    let linked = f.call("agree-new", "pms_link", link.clone()).await?;
    assert_eq!(linked["work_item"], a["work_item"]);
    assert_eq!(linked["linked"], true);
    assert_eq!(
        f.call("agree-new", "pms_link", link).await?["replayed"],
        true
    );
    let audits:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id=$1 AND event_type='business_source_linked'").bind(source).fetch_one(&f.store.pool).await?;
    assert_eq!(audits, 1);
    f.store
        .business_feed_open(f.binding, Some(&payment_head("0")))
        .await?;
    let event = f
        .store
        .business_accept_payment(
            f.binding,
            "synthetic-pms",
            "property_a",
            &payment(1, "proxy_bill", "COLLECTION", "DISCOVERED"),
        )
        .await?;
    let payment_work: Uuid = sqlx::query_scalar(
        "SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE id=$1",
    )
    .bind(serde_json::from_value::<Uuid>(event["receipt_id"].clone())?)
    .fetch_one(&f.store.pool)
    .await?;
    f.capture("payment-link-proposal", "这笔代付款对应已核对的订单")
        .await?;
    let payment_link = json!({"action":a["action"],"work_item":payment_work,"readback":{"order":{"id":"synthetic-order","property_id":"property_a","version":2,"status":"CONFIRMED"}}});
    assert_eq!(
        f.call("payment-link-proposal", "pms_link", payment_link.clone())
            .await?["phase"],
        "awaiting_confirmation"
    );
    f.capture("payment-link-confirm", "确认关联").await?;
    assert_eq!(
        f.call("payment-link-confirm", "pms_link", payment_link)
            .await?["work_item"],
        a["work_item"]
    );
    f.allow("RECORD_COLLECTION").await?;
    f.capture("payment", "核对代付款后准备收款").await?;
    let collect = json!({"binding":f.binding,"work_item":source,"operation":"pms.command.RECORD_COLLECTION","input":{"orderId":"synthetic-order","amountMinor":12000,"method":"BANK_TRANSFER","transactionReference":"synthetic-proxy-payment"},"reason":{"code":"COLLECT","note":""}});
    let next = f.call("payment", "pms_start", collect.clone()).await?;
    assert_eq!(next["work_item"], a["work_item"]);
    assert!(f
        .call(
            "payment",
            "pms_claim_execute",
            json!({"action":next["action"]})
        )
        .await
        .is_err());
    let mut wrong = collect;
    wrong["input"]["orderId"] = json!("another-stay");
    assert!(f.call("payment", "pms_start", wrong).await.is_err());
    assert_eq!(
        f.call("payment", "pms_status", json!({"action":a["action"]}))
            .await?["phase"],
        "completed"
    );
    Ok(())
}
