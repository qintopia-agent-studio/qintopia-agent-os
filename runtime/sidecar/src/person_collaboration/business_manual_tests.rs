//! Manual completion uses independently observed PMS history, never status/amount guesses.
use super::*;

async fn manual_plan(f: &Fixture, command: &str, effect: Value) -> Result<Value> {
    f.allow(command).await?;
    f.capture(command, "请准备方案并交人工接手").await?;
    let a=f.call(command,"pms_start",json!({"binding":f.binding,"operation":format!("pms.command.{command}"),"input":{"orderId":"order_manual"},"reason":{"code":"MANUAL","note":""}})).await?;
    let c = f
        .call(command, "pms_claim_preview", json!({"action":a["action"]}))
        .await?;
    f.call(command,"pms_save_preview",json!({"action":a["action"],"claim":c["claim"],"preview":{"previewId":"preview_manual","propertyId":"property_a","commandType":command,"effectHash":"a".repeat(64),"effect":effect,"expiresAt":(Utc::now()+Duration::minutes(10)).to_rfc3339()}})).await?;
    Ok(a)
}
fn before() -> Value {
    json!({"order":{"id":"order_manual","property_id":"property_a","version":4,"status":"RESERVED"},"collectionFacts":[]})
}
#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn manual_amendments_reject_old_racing_changed_and_ambiguous_effects() -> Result<()> {
    for (command, status) in [
        ("CHECK_IN", "CHECKED_IN"),
        ("CHECK_OUT", "CHECKED_OUT"),
        ("CANCEL_ORDER", "CANCELLED"),
        ("RESCHEDULE_STAY", "RESERVED"),
        ("EXTEND_STAY", "CHECKED_IN"),
        ("SHORTEN_STAY", "CHECKED_IN"),
        ("MOVE_UNIT", "CHECKED_IN"),
    ] {
        let f = Fixture::new().await?;
        let effect = json!({"orderId":"order_manual","newDepartureDate":"2026-10-02","nested":{"inventoryUnitId":"unit_a","amountMinor":12000}});
        let a = manual_plan(&f, command, effect.clone()).await?;
        assert!(f
            .call(command, "pms_handoff", json!({"action":a["action"]}))
            .await
            .is_err());
        f.call(
            command,
            "pms_handoff",
            json!({"action":a["action"],"readback":before()}),
        )
        .await?;
        let baseline = f
            .call(command, "pms_status", json!({"action":a["action"]}))
            .await?["readback"]
            .clone();
        let mut later = before();
        later["order"]["version"] = json!(99);
        let repeat = f
            .call(
                command,
                "pms_handoff",
                json!({"action":a["action"],"readback":later}),
            )
            .await?;
        assert_eq!(repeat["replayed"], true);
        assert_eq!(
            f.call(command, "pms_status", json!({"action":a["action"]}))
                .await?["readback"],
            baseline
        );
        for tool in ["pms_pause", "pms_cancel", "pms_claim_execute"] {
            assert!(f
                .call(command, tool, json!({"action":a["action"]}))
                .await
                .is_err());
        }
        f.capture("manual_done", "已在PMS办理这个方案，订单号order_manual")
            .await?;
        let observed_at: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&f.store.pool)
            .await?;
        let record = json!({"id":"amend_new","order_id":"order_manual","sequence":5,"amendment_type":command,"prior_version":4,"new_version":5,"payload":effect,"command_id":"command_staff","created_at":observed_at});
        let good = json!({"order":{"id":"order_manual","property_id":"property_a","version":5,"status":status},"amendments":[record]});
        let mut bad = Vec::new();
        let mut old = good.clone();
        old["amendments"][0]["prior_version"] = json!(3);
        bad.push(old);
        let mut race = good.clone();
        race["amendments"][0]["created_at"] = json!(Utc::now() - Duration::minutes(1));
        bad.push(race);
        let mut changed = good.clone();
        changed["amendments"][0]["payload"]["nested"]["inventoryUnitId"] = json!("other");
        bad.push(changed);
        let mut superseded = good.clone();
        superseded["order"]["version"] = json!(6);
        bad.push(superseded);
        let mut ambiguous = good.clone();
        ambiguous["amendments"]
            .as_array_mut()
            .unwrap()
            .push(record.clone());
        bad.push(ambiguous);
        let mut cross = good.clone();
        cross["order"]["property_id"] = json!("other");
        bad.push(cross);
        for read in bad {
            assert!(
                f.call(
                    "manual_done",
                    "pms_save_manual",
                    json!({"action":a["action"],"readback":read})
                )
                .await
                .is_err(),
                "{command}"
            );
            assert_eq!(
                f.call("manual_done", "pms_status", json!({"action":a["action"]}))
                    .await?["phase"],
                "manual_handoff"
            );
        }
        let restarted = Store::local(
            &crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?,
            &f.store.tenant,
        )
        .await?;
        let args = json!({"action":a["action"],"readback":good});
        let (one, two) = tokio::join!(
            restarted.business_invoke(
                &f.actor,
                "manual_done",
                "synthetic_chat",
                "pms_save_manual",
                &args
            ),
            f.call("manual_done", "pms_save_manual", args.clone())
        );
        assert_eq!(usize::from(one.is_ok()) + usize::from(two.is_ok()), 1);
        let saved = f
            .call("manual_done", "pms_status", json!({"action":a["action"]}))
            .await?;
        assert_eq!(saved["phase"], "manual_completed");
        assert_eq!(
            saved["readback"]["manual_evidence"]["amendment_id"],
            "amend_new"
        );
        assert!(saved["readback"].get("amendments").is_none());
        let count:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id=$1 AND event_type='human_pms_completion_observed'").bind(Uuid::parse_str(a["work_item"].as_str().unwrap())?).fetch_one(&f.store.pool).await?;
        assert_eq!(count, 1);
    }
    Ok(())
}
#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn manual_collection_requires_unique_new_unreversed_reference() -> Result<()> {
    for (reference, precise) in [(Some("transfer_1"), false), (None, false), (None, true)] {
        let f = Fixture::new().await?;
        let effect = json!({"orderId":"order_manual","method":if reference.is_some() {"BANK_TRANSFER"} else {"CASH"},"amountMinor":12000,"currency":"CNY","transactionReference":reference});
        let a = manual_plan(&f, "RECORD_COLLECTION", effect).await?;
        let mut base = before();
        base["collectionFacts"] = json!([{"fact_id":"fact_old","order_id":"order_manual"}]);
        f.call(
            "RECORD_COLLECTION",
            "pms_handoff",
            json!({"action":a["action"],"readback":base}),
        )
        .await?;
        f.capture("manual_done", "已在PMS办理这个方案，订单号order_manual")
            .await?;
        let observed_at: chrono::DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&f.store.pool)
            .await?;
        let fact = json!({"fact_id":"fact_new","command_id":"command_staff","order_id":"order_manual","fact_type":"COLLECTION","amount_minor":12000,"net_effect_minor":12000,"currency":"CNY","method":if reference.is_some() {"BANK_TRANSFER"} else {"CASH"},"transaction_reference":reference,"created_at":observed_at,"transfer":null});
        let mut good = before();
        good["collectionFacts"] = json!([fact]);
        let mut bad = Vec::new();
        let mut old = good.clone();
        old["collectionFacts"][0]["fact_id"] = json!("fact_old");
        bad.push(old);
        let mut wrong = good.clone();
        wrong["collectionFacts"][0]["transaction_reference"] = json!("unrelated_same_amount");
        bad.push(wrong);
        let mut transferred = good.clone();
        transferred["collectionFacts"][0]["transfer"] = json!({"id":"transfer"});
        bad.push(transferred);
        let mut reversed = good.clone();
        reversed["collectionFacts"]
            .as_array_mut()
            .unwrap()
            .push(json!({"fact_id":"reversal","reverses_fact_id":"fact_new"}));
        bad.push(reversed);
        let mut duplicate = good.clone();
        duplicate["collectionFacts"]
            .as_array_mut()
            .unwrap()
            .push(fact.clone());
        bad.push(duplicate);
        for read in bad {
            assert!(f
                .call(
                    "manual_done",
                    "pms_save_manual",
                    json!({"action":a["action"],"readback":read})
                )
                .await
                .is_err());
        }
        if precise {
            f.capture(
                "manual_precise",
                "已在PMS办理这个方案，订单号order_manual，收款记录fact_new",
            )
            .await?;
        }
        let result = f
            .call(
                if precise {
                    "manual_precise"
                } else {
                    "manual_done"
                },
                "pms_save_manual",
                json!({"action":a["action"],"readback":good}),
            )
            .await;
        if reference.is_some() || precise {
            assert_eq!(result?["phase"], "manual_completed");
        } else {
            let proposal = result?;
            assert_eq!(proposal["phase"], "manual_handoff");
            assert_eq!(proposal["manual_fact"]["id"], "fact_new");
            f.capture("cash_confirm", "确认这笔人工收款是本事项的办理结果")
                .await?;
            let mut newer = good.clone();
            newer["order"]["version"] = json!(5);
            let stale = f
                .call(
                    "cash_confirm",
                    "pms_save_manual",
                    json!({"action":a["action"],"readback":newer}),
                )
                .await?;
            assert_eq!(stale["phase"], "manual_handoff");
            // Confirmation of the previous read must not approve the new proposal.
            assert_eq!(
                f.call(
                    "cash_confirm",
                    "pms_save_manual",
                    json!({"action":a["action"],"readback":newer})
                )
                .await?["phase"],
                "manual_handoff"
            );
            f.capture("cash_current", "确认这笔人工收款是本事项的办理结果")
                .await?;
            let mut reversed = newer.clone();
            reversed["collectionFacts"]
                .as_array_mut()
                .unwrap()
                .push(json!({"reverses_fact_id":"fact_new"}));
            assert!(f
                .call(
                    "cash_current",
                    "pms_save_manual",
                    json!({"action":a["action"],"readback":reversed})
                )
                .await
                .is_err());
            let done = f
                .call(
                    "cash_current",
                    "pms_save_manual",
                    json!({"action":a["action"],"readback":newer}),
                )
                .await?;
            assert_eq!(done["phase"], "manual_completed");
            assert!(f
                .call(
                    "cash_current",
                    "pms_claim_execute",
                    json!({"action":a["action"]})
                )
                .await
                .is_err());
        }
    }
    Ok(())
}

fn adoption_read(order: &str) -> Value {
    json!({"order":{"id":order,"property_id":"property_a","version":1,"status":"RESERVED"},"booking":{"quoteId":"staff_quote","id":order,"propertyId":"property_a","version":1,"status":"RESERVED","primaryGuest":{"fullName":"模拟人工住客","nickname":"小住"},"inventoryUnitId":"unit_manual","unit_code":"102","arrivalDate":"2026-09-26","departureDate":"2026-09-27","bookingChannelCode":"WECOM","amountMinor":12000,"currency":"CNY","stayType":"TRANSIENT","memberId":null,"occupants":[{"fullName":"模拟人工住客","nickname":"小住"}],"segments":[{"inventoryUnitId":"unit_manual","arrivalDate":"2026-09-26","departureDate":"2026-09-27"}]}})
}
#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn manual_booking_adoption_distinguishes_decision_and_rejects_stale_or_conflicting_orders(
) -> Result<()> {
    let f = Fixture::new().await?;
    let (a, _) = f.prepared().await?;
    f.call("start", "pms_handoff", json!({"action":a["action"]}))
        .await?;
    let mut read = adoption_read("order_adopted");
    let first = f
        .call(
            "start",
            "pms_save_manual",
            json!({"action":a["action"],"readback":read}),
        )
        .await?;
    assert_eq!(first["phase"], "manual_handoff");
    assert!(!first["differences"].as_array().unwrap().is_empty());
    f.capture("accept_old", "确认此订单承接原订房事项").await?;
    read["order"]["version"] = json!(2);
    read["booking"]["version"] = json!(2);
    assert_eq!(
        f.call(
            "accept_old",
            "pms_save_manual",
            json!({"action":a["action"],"readback":read})
        )
        .await?["phase"],
        "manual_handoff"
    );
    let mut absent = read.clone();
    absent["booking"]["primaryGuest"] = Value::Null;
    assert!(f
        .call(
            "accept_old",
            "pms_save_manual",
            json!({"action":a["action"],"readback":absent})
        )
        .await
        .is_err());
    let mut cross = read.clone();
    cross["booking"]["propertyId"] = json!("other");
    assert!(f
        .call(
            "accept_old",
            "pms_save_manual",
            json!({"action":a["action"],"readback":cross})
        )
        .await
        .is_err());
    f.capture("accept_current", "确认此订单承接原订房事项")
        .await?;
    let done = f
        .call(
            "accept_current",
            "pms_save_manual",
            json!({"action":a["action"],"readback":read}),
        )
        .await?;
    assert_eq!(done["completion_basis"], "human_order_adoption");
    let saved = f
        .call(
            "accept_current",
            "pms_status",
            json!({"action":a["action"]}),
        )
        .await?;
    assert!(saved["result"].is_null());
    assert_eq!(
        saved["readback"]["manual_evidence"]["kind"],
        "human_order_adoption"
    );
    assert!(f
        .call(
            "accept_current",
            "pms_claim_execute",
            json!({"action":a["action"]})
        )
        .await
        .is_err());
    f.store
        .business_feed_open(f.binding, Some(&payment_head("0")))
        .await?;
    f.store
        .business_accept_payment(
            f.binding,
            "synthetic-pms",
            "property_a",
            &payment(1, "manual-link-bill", "COLLECTION", "DISCOVERED"),
        )
        .await?;
    let source: Uuid = sqlx::query_scalar(
        "SELECT work_item_id FROM qintopia_agent_os.business_event_inbox WHERE tenant_key=$1",
    )
    .bind(&f.store.tenant)
    .fetch_one(&f.store.pool)
    .await?;
    let link = json!({"action":a["action"],"work_item":source,"readback":read});
    let action_id = Uuid::parse_str(a["action"].as_str().unwrap())?;
    sqlx::query("UPDATE qintopia_agent_os.business_actions SET readback=readback-'manual_evidence' WHERE id=$1").bind(action_id).execute(&f.store.pool).await?;
    assert!(f
        .call("accept_current", "pms_link", link.clone())
        .await
        .is_err());
    sqlx::query("UPDATE qintopia_agent_os.business_actions SET readback=$2 WHERE id=$1")
        .bind(action_id)
        .bind(&saved["readback"])
        .execute(&f.store.pool)
        .await?;
    let mut wrong = link.clone();
    wrong["readback"]["order"]["id"] = json!("unrelated_order");
    assert!(f.call("accept_current", "pms_link", wrong).await.is_err());
    assert_eq!(
        f.call("accept_current", "pms_link", link.clone()).await?["phase"],
        "awaiting_confirmation"
    );
    f.capture("link_manual", "确认关联").await?;
    assert_eq!(
        f.call("link_manual", "pms_link", link.clone()).await?["linked"],
        true
    );
    assert!(f.call("link_manual","pms_start",json!({"binding":f.binding,"operation":"pms.command.CREATE_ORDER","work_item":source,"input":{"quoteId":"do_not_rebook"},"reason":{"code":"CREATE_STANDARD_ORDER","note":""}})).await.is_err());
    f.allow("RECORD_COLLECTION").await?;
    let funds=f.call("link_manual","pms_start",json!({"binding":f.binding,"operation":"pms.command.RECORD_COLLECTION","work_item":source,"input":{"orderId":"order_adopted","method":"WECOM","amountMinor":100,"transactionReference":"manual-linked-reference"},"source_payment":{"id":"manual-link-bill","status":"AVAILABLE","kind":"COLLECTION","reference":"manual-linked-reference","amountMinor":100},"reason":{"code":"CUSTOMER_PAYMENT","note":""}})).await?;
    assert_eq!(funds["work_item"], a["work_item"]);
    let funds_claim = f
        .call(
            "link_manual",
            "pms_claim_preview",
            json!({"action":funds["action"]}),
        )
        .await?;
    let pending=f.call("link_manual","pms_save_preview",json!({"action":funds["action"],"claim":funds_claim["claim"],"preview":{"previewId":"funds_preview","propertyId":"property_a","commandType":"RECORD_COLLECTION","effectHash":"c".repeat(64),"effect":{"orderId":"order_adopted","method":"WECOM","amountMinor":100,"transactionReference":"manual-linked-reference","currency":"CNY"},"expiresAt":(Utc::now()+Duration::minutes(10)).to_rfc3339()}})).await?;
    assert_eq!(pending["confirmation_reused"], false);
    assert!(f
        .call(
            "link_manual",
            "pms_claim_execute",
            json!({"action":funds["action"]})
        )
        .await
        .is_err());
    f.capture("other_booking", "另一次订房").await?;
    let other=f.call("other_booking","pms_start",json!({"binding":f.binding,"operation":"pms.command.CREATE_ORDER","input":{"quoteId":"quote_other"},"reason":{"code":"CREATE_STANDARD_ORDER","note":""}})).await?;
    let claim = f
        .call(
            "other_booking",
            "pms_claim_preview",
            json!({"action":other["action"]}),
        )
        .await?;
    assert!(f
        .call(
            "other_booking",
            "pms_handoff",
            json!({"action":other["action"]})
        )
        .await
        .is_err());
    f.call("other_booking","pms_save_preview",json!({"action":other["action"],"claim":claim["claim"],"preview":{"previewId":"other_preview","propertyId":"property_a","commandType":"CREATE_ORDER","effectHash":"b".repeat(64),"effect":{"amountMinor":14000},"expiresAt":(Utc::now()+Duration::minutes(10)).to_rfc3339()}})).await?;
    f.call(
        "other_booking",
        "pms_handoff",
        json!({"action":other["action"]}),
    )
    .await?;
    assert!(f
        .call(
            "other_booking",
            "pms_save_manual",
            json!({"action":other["action"],"readback":read})
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("manual_order_conflict"));
    let second = adoption_read("order_second");
    f.capture("full_report","采用订单「order_second」承接原订房事项：住客「模拟人工住客」（昵称「小住」），房间「102」，2026-09-26入住，2026-09-27离店，渠道「WECOM」，合同金额120.00元；已核对并采纳与原方案的差异。").await?;
    let direct = f
        .call(
            "full_report",
            "pms_save_manual",
            json!({"action":other["action"],"readback":second}),
        )
        .await?;
    assert_eq!(direct["phase"], "manual_completed");
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked' WHERE id=$1")
        .bind(f.grant)
        .execute(&f.store.pool)
        .await?;
    assert!(f.call("link_manual", "pms_link", link).await.is_err());
    Ok(())
}
