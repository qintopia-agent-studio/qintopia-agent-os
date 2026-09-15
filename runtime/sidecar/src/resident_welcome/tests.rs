use super::*;
use chrono::Utc;
use hmac::{Hmac, Mac};
use serde_json::{json, Value};
use sha2::Sha256;
use std::collections::BTreeMap;
#[cfg(feature = "postgres-integration-tests")]
use uuid::Uuid;

fn raw_event(source: &str, id: &str, revision: &str, event_type: &str) -> Vec<u8> {
    serde_json::to_vec(&json!({"schema_version":"pms.events.v1","event_id":id,"source_instance":source,"property_id":"fixture-property","event_type":event_type,"aggregate_type":"order","aggregate_id":"fixture-order","aggregate_revision":revision,"recorded_at":Utc::now(),"effective_at":null,"source_fact_ref":"command:fixture","publish_seq":revision,"refs":{"order_id":"fixture-order"},"origin":"live"})).unwrap()
}
fn key(source: &str) -> protocol::SigningKey {
    protocol::SigningKey {
        key_id: "fixture-key".into(),
        secret: zeroize::Zeroizing::new(vec![0x42; 32]),
        source_instance: source.into(),
        properties: ["fixture-property".into()].into_iter().collect(),
    }
}
fn headers(raw: &[u8], now: i64, key: &protocol::SigningKey) -> BTreeMap<String, String> {
    let mut mac = Hmac::<Sha256>::new_from_slice(&key.secret).unwrap();
    mac.update(
        format!(
            "POST\n{}\n{now}\nfixture-delivery\n{}",
            protocol::PATH,
            digest(raw)
        )
        .as_bytes(),
    );
    let signature = mac
        .finalize()
        .into_bytes()
        .iter()
        .map(|b| format!("{b:02x}"))
        .collect::<String>();
    [
        ("content-type".into(), "application/json".into()),
        ("x-qt-key-id".into(), key.key_id.clone()),
        ("x-qt-sent-at".into(), now.to_string()),
        ("x-qt-delivery-id".into(), "fixture-delivery".into()),
        ("x-qt-signature".into(), signature),
    ]
    .into()
}

#[test]
fn t10_signature_scope_clock_and_old_event_replay() {
    let raw = raw_event(
        "fixture-source",
        "fixture-event",
        "1",
        "pms.stay.checked_in",
    );
    let key = key("fixture-source");
    let now = Utc::now().timestamp();
    let h = headers(&raw, now, &key);
    assert!(protocol::verify(&raw, &h, std::slice::from_ref(&key), now).is_ok());
    assert_eq!(
        protocol::verify(&raw, &h, std::slice::from_ref(&key), now + 301)
            .err()
            .unwrap()
            .status,
        401
    );
    let mut forged = h.clone();
    forged.insert("x-qt-signature".into(), "0".repeat(64));
    assert_eq!(
        protocol::verify(&raw, &forged, std::slice::from_ref(&key), now)
            .err()
            .unwrap()
            .status,
        401
    );
    let mut old: Value = serde_json::from_slice(&raw).unwrap();
    old["recorded_at"] = json!("2020-01-01T00:00:00Z");
    let old = serde_json::to_vec(&old).unwrap();
    assert!(protocol::verify(
        &old,
        &headers(&old, now, &key),
        std::slice::from_ref(&key),
        now
    )
    .is_ok());
    let mut wrong: Value = serde_json::from_slice(&raw).unwrap();
    wrong["property_id"] = json!("other-property");
    let wrong = serde_json::to_vec(&wrong).unwrap();
    assert_eq!(
        protocol::verify(
            &wrong,
            &headers(&wrong, now, &key),
            std::slice::from_ref(&key),
            now
        )
        .err()
        .unwrap()
        .status,
        403
    );
}

#[test]
fn duplicate_keys_deep_payload_and_precision_are_rejected() {
    assert!(protocol::parse_unique(br#"{"x":{"a":1,"a":2}}"#).is_err());
    assert!(
        protocol::parse_unique(format!("{}0{}", "[".repeat(18), "]".repeat(18)).as_bytes())
            .is_err()
    );
    assert_eq!(
        protocol::parse_unique(&vec![b' '; 65537])
            .err()
            .unwrap()
            .status,
        413
    );
    assert!(protocol::decimal("90071992547409931234567890"));
    assert!(!protocol::decimal("1e9"));
    assert!(!protocol::decimal("01"));
    assert!(state::compare_revision("100000000000000000000", "99999999999999999999").is_gt());
}

#[test]
fn t09_feed_retains_original_event_bytes() {
    let raw = String::from_utf8(raw_event(
        "fixture-source",
        "fixture-event",
        "1",
        "pms.order.created",
    ))
    .unwrap();
    let noncanonical = raw
        .replace("\"event_id\":", "\"event_id\" : ")
        .replace("fixture-event", r"fixture-\u0065vent");
    let page = format!(
        r#"{{"schema_version":"pms.events.v1","source_instance":"fixture-source","property_id":"fixture-property","events":[{noncanonical}],"next_cursor":"opaque-A","has_more":false,"head_cursor":"opaque-A","retention_floor_cursor":"opaque-0"}}"#
    );
    let page: client::FeedPage = serde_json::from_str(&page).unwrap();
    let events = page.verified("fixture-source", "fixture-property").unwrap();
    assert_eq!(events[0].body_hash, digest(noncanonical.as_bytes()));
    assert_ne!(
        events[0].body_hash,
        digest(&serde_json::to_vec(&events[0].envelope).unwrap())
    );
}

#[test]
fn t14_attachment_append_and_self_callback() {
    assert_eq!(
        artifact::append_attachment(&["other-stay".into(), "original".into()], "new"),
        vec!["other-stay", "original", "new"]
    );
    assert_eq!(
        artifact::append_attachment(&["original".into()], "original"),
        vec!["original"]
    );
    assert!(!channels::application_callback_requires_readback(&[
        "欢迎卡片".into()
    ]));
    assert!(channels::application_callback_requires_readback(&[
        "欢迎卡片".into(),
        "展示同意".into()
    ]));
}

#[test]
fn t16_all_external_modes_fail_closed() {
    for mode in [
        ExecutionMode::Shadow,
        ExecutionMode::Synthetic,
        ExecutionMode::Live,
    ] {
        assert!(external_gate(mode).is_err());
    }
}

fn order_fixture(source: &str) -> serde_json::Value {
    let mut p: serde_json::Value = serde_json::from_str(include_str!(
        "../../../../workflows/resident-welcome/fixtures/order-projection.json"
    ))
    .unwrap();
    p["source_instance"] = json!(source);
    let mut stable = p.clone();
    for key in ["projection_hash", "observed_at", "read_context"] {
        stable.as_object_mut().unwrap().remove(key);
    }
    p["projection_hash"] = json!(digest(projection::canonical(&stable).unwrap().as_bytes()));
    p
}

#[test]
fn scan_rejects_wrong_scope_reordering_and_empty_progress() {
    let p = order_fixture("synthetic-scan");
    let mut page = recovery::ScanPage {
        schema_version: "pms.projections.v1".into(),
        source_instance: "synthetic-scan".into(),
        property_id: "fixture-property".into(),
        orders: vec![p],
        next_after_id: Some("fixture-order".into()),
        has_more: false,
    };
    assert!(page
        .verified("synthetic-scan", "fixture-property", None)
        .is_ok());
    assert!(page.verified("another", "fixture-property", None).is_err());
    assert!(page
        .verified("synthetic-scan", "fixture-property", Some("fixture-order"))
        .is_err());
    page.orders.clear();
    page.has_more = true;
    assert!(page
        .verified("synthetic-scan", "fixture-property", Some("fixture-order"))
        .is_err());
    page.has_more = false;
    assert!(page
        .verified("synthetic-scan", "fixture-property", Some("fixture-order"))
        .is_ok());
}

#[cfg(feature = "postgres-integration-tests")]
#[tokio::test]
#[ignore = "requires explicitly isolated local qintopia_test"]
async fn resident_welcome_postgres_scan_resume_and_scoped_worker() -> anyhow::Result<()> {
    use sqlx::Row;
    use std::sync::Arc;
    let database = crate::foundation_test_support::database_url("QINTOPIA_WELCOME_TEST")?;
    let store = store::Store::local(&database).await?;
    crate::db::run_migrations(&store.pool).await?;
    let source = format!("synthetic-scan-{}", Uuid::new_v4());
    let property = "fixture-property";
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_sources(source_instance,property_id,mode) VALUES ($1,$2,'synthetic')").bind(&source).bind(property).execute(&store.pool).await?;
    let generation = store.start_rebuild(&source, property, "head-1").await?;
    let mut page = recovery::ScanPage {
        schema_version: "pms.projections.v1".into(),
        source_instance: source.clone(),
        property_id: property.into(),
        orders: vec![order_fixture(&source)],
        next_after_id: Some("fixture-order".into()),
        has_more: false,
    };
    page.next_after_id = Some("bad-position".into());
    assert!(store
        .commit_scan(&source, property, generation, None, &page)
        .await
        .is_err());
    let position: Option<String> = sqlx::query_scalar(
        "SELECT after_id FROM qintopia_agent_os.welcome_rebuilds WHERE source_instance=$1",
    )
    .bind(&source)
    .fetch_one(&store.pool)
    .await?;
    assert!(position.is_none());
    page.next_after_id = Some("fixture-order".into());
    store
        .commit_scan(&source, property, generation, None, &page)
        .await?;
    assert!(store
        .finish_scan(&source, property, generation)
        .await
        .is_err());
    // A new Store represents a crashed/restarted consumer, retaining durable page.
    let restarted = store::Store::local(&database).await?;
    assert!(restarted.drain_scan_one(&source, property).await?);
    assert!(!restarted.drain_scan_one(&source, property).await?);
    restarted.finish_scan(&source, property, generation).await?;
    let row=sqlx::query("SELECT cursor,enabled,rebuilding FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1").bind(&source).fetch_one(&store.pool).await?;
    assert_eq!(row.get::<String, _>("cursor"), "head-1");
    assert!(!row.get::<bool, _>("enabled") && row.get::<bool, _>("rebuilding"));
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.welcome_cases WHERE source_instance=$1 AND NOT admitted").bind(&source).fetch_one(&store.pool).await?;
    assert_eq!(count, 2);
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.welcome_inbox WHERE source_instance=$1",
    )
    .bind(&source)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(count, 0); // baseline is not an invented PMS event
    let human: Uuid = sqlx::query_scalar(
        "INSERT INTO qintopia_identity.persons(display_name) VALUES ('合成匹配员') RETURNING id",
    )
    .fetch_one(&store.pool)
    .await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_grants(person_id,source_instance,property_id,action,expires_at,appointed_by) VALUES ($1,$2,$3,'identity',now()+interval '1 hour',$1)").bind(human).bind(&source).bind(property).execute(&store.pool).await?;
    let actor = store::Actor {
        person: human,
        source: source.clone(),
        property: property.into(),
        expires: Utc::now() + chrono::Duration::minutes(5),
    };
    let link:Uuid=sqlx::query_scalar("SELECT l.id FROM qintopia_identity.source_identity_links l JOIN qintopia_agent_os.welcome_identity_scopes s ON s.namespace=l.namespace WHERE s.source_instance=$1 AND l.subject_type='pms_occupant' AND l.source_ref='occupant-A'").bind(&source).fetch_one(&store.pool).await?;
    let request = workbench::IdentityRequest {
        operation_id: Uuid::new_v4(),
        link_ref: link,
        expected_version: 1,
        person_ref: None,
        new_person_label: Some("合成独立人员".into()),
        revoke: false,
    };
    let created = store.workbench_identity(&actor, &request).await?;
    assert_eq!(store.workbench_identity(&actor, &request).await?, created);
    let people = store.search_people(&actor, "合成独立人员", 20).await?;
    assert_eq!(people.len(), 1);
    let ready:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.welcome_cases WHERE source_instance=$1 AND application_id IS NOT NULL").bind(&source).fetch_one(&store.pool).await?;
    assert_eq!(ready, 0); // Person existence cannot imply a valid welcome application
    let wrong_actor = store::Actor {
        person: human,
        source: "another-source".into(),
        property: property.into(),
        expires: Utc::now() + chrono::Duration::minutes(5),
    };
    assert!(store
        .workbench_identity(&wrong_actor, &request)
        .await
        .is_err());
    store
        .accept_page(&source, property, Some("head-1"), "head-2", &[])
        .await?;
    restarted.finish_scan(&source, property, generation).await?;
    let cursor: String = sqlx::query_scalar(
        "SELECT cursor FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1",
    )
    .bind(&source)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(cursor, "head-2");
    let newer = store.start_rebuild(&source, property, "head-3").await?;
    assert!(store
        .commit_scan(&source, property, generation, None, &page)
        .await
        .is_err());
    store
        .commit_scan(&source, property, newer, None, &page)
        .await?;
    assert!(store.drain_scan_one(&source, property).await?);
    store.finish_scan(&source, property, newer).await?;
    struct Fake {
        p: serde_json::Value,
        fail: bool,
    }
    impl recovery::ReadSource for Fake {
        fn events(&self, _: Option<&str>) -> anyhow::Result<client::FeedPage> {
            anyhow::bail!("not used")
        }
        fn scan(&self, _: Option<&str>) -> anyhow::Result<recovery::ScanPage> {
            anyhow::bail!("not used")
        }
        fn projection(&self, _: &str, _: &str) -> anyhow::Result<serde_json::Value> {
            anyhow::ensure!(!self.fail, "synthetic unavailable");
            Ok(self.p.clone())
        }
    }
    let raw = raw_event(&source, "recover-event", "1", "pms.stay.checked_in");
    let event = protocol::VerifiedEvent::from_feed(&raw, &source, property)?;
    let ack = store.accept(&event).await?;
    assert!(store
        .consume_one(
            &source,
            property,
            Arc::new(Fake {
                p: order_fixture(&source),
                fail: true
            })
        )
        .await
        .is_err());
    let row=sqlx::query("SELECT i.status,w.available_at>now() AS delayed FROM qintopia_agent_os.welcome_inbox i JOIN qintopia_agent_os.work_items w ON w.id=i.work_item_id WHERE i.id=$1").bind(ack.receipt_id).fetch_one(&store.pool).await?;
    assert_eq!(row.get::<String, _>("status"), "queued");
    assert!(row.get::<bool, _>("delayed"));
    sqlx::query("UPDATE qintopia_agent_os.work_items SET available_at=now() WHERE id=(SELECT work_item_id FROM qintopia_agent_os.welcome_inbox WHERE id=$1)").bind(ack.receipt_id).execute(&store.pool).await?;
    assert!(
        restarted
            .consume_one(
                &source,
                property,
                Arc::new(Fake {
                    p: order_fixture(&source),
                    fail: false
                })
            )
            .await?
    );
    let count:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.welcome_cases WHERE source_instance=$1 AND admitted").bind(&source).fetch_one(&store.pool).await?;
    assert_eq!(count, 0); // replays during rebuild do not promote baseline backlog
    let mut original = projection::normalize(&order_fixture(&source))?;
    original.revision = "9007199254740994".into();
    original.source_hash = None;
    original.related_revisions =
        BTreeMap::from([("inventory_unit/replacement-room".into(), "1".into())]);
    original.observed_at = Utc::now();
    store.apply_snapshot(None, &original).await?;
    let revision:String=sqlx::query_scalar("SELECT revision::text FROM qintopia_agent_os.welcome_source_versions WHERE source_instance=$1 AND aggregate_type='order'").bind(&source).fetch_one(&store.pool).await?;
    assert_eq!(revision, original.revision); // replaced inventory is not a stale vector
    let mut conflict = original.clone();
    conflict.source_hash = Some(digest(b"conflicting authority"));
    assert!(store.apply_snapshot(None, &conflict).await.is_err());
    assert!(store.apply_snapshot(None, &original).await.is_err());
    let isolated:bool=sqlx::query_scalar("SELECT conflicted AND invalidated FROM qintopia_agent_os.welcome_source_versions WHERE source_instance=$1 AND aggregate_type='order'").bind(&source).fetch_one(&store.pool).await?;
    assert!(isolated);
    Ok(())
}

#[test]
fn pms_projection_hash_and_business_day_converge_without_false_version_conflict(
) -> anyhow::Result<()> {
    let raw: Value = serde_json::from_str(include_str!(
        "../../../../workflows/resident-welcome/fixtures/order-projection.json"
    ))?;
    let accepted = projection::order(
        &raw,
        "synthetic-pms",
        "fixture-property",
        Some("fixture-order"),
    )?;
    let snapshot = projection::normalize(&accepted)?;
    assert!(snapshot.current_arrangement && snapshot.state == state::StayState::InHouse);
    // Observation and business-day fields are deliberately not in stable hash.
    let mut due = raw.clone();
    due["observed_at"] = json!("2026-09-12T04:00:00Z");
    due["read_context"]["business_date"] = json!("2026-09-12");
    due["read_context"]["temporal_state"] = json!("DUE_OUT");
    due["read_context"]["current_interval"] = Value::Null;
    let due = projection::order(
        &due,
        "synthetic-pms",
        "fixture-property",
        Some("fixture-order"),
    )?;
    assert!(!projection::normalize(&due)?.current_arrangement);
    let mut extra = raw.clone();
    extra["ignored_extension"] = json!("discarded");
    assert!(
        projection::order(&extra, "synthetic-pms", "fixture-property", None)?
            .get("ignored_extension")
            .is_none()
    );
    let mut wrong = raw.clone();
    wrong["effective_arrangement"]["intervals"][0]["building_code"] = json!("B");
    assert!(projection::order(&wrong, "synthetic-pms", "fixture-property", None).is_err());
    let mut wrong = raw.clone();
    wrong["read_context"]["current_interval"]["inventory_unit_id"] = json!("different");
    assert!(projection::order(&wrong, "synthetic-pms", "fixture-property", None).is_err());
    assert!(projection::order(&raw, "wrong-source", "fixture-property", None).is_err());
    assert_eq!(
        projection::canonical(&json!({"\u{10000}":"x","\u{e000}":"y"}))?,
        "{\"𐀀\":\"x\",\"\":\"y\"}"
    );
    Ok(())
}

#[cfg(feature = "postgres-integration-tests")]
#[tokio::test]
#[ignore = "requires explicitly isolated local qintopia_test"]
async fn resident_welcome_postgres_identity_inbox_and_delivery_recovery() -> anyhow::Result<()> {
    use sqlx::Row;
    let url = crate::foundation_test_support::database_url("QINTOPIA_WELCOME_TEST")?;
    let store = store::Store::local(&url).await?;
    crate::db::run_migrations(&store.pool).await?;
    // Reapply the additive migration to prove idempotent installation.
    sqlx::raw_sql(include_str!(
        "../../../postgres/migrations/202609090001_resident_welcome_v1.sql"
    ))
    .execute(&store.pool)
    .await?;
    let source = format!("synthetic-{}", Uuid::new_v4());
    let property = "fixture-property";
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_sources(source_instance,property_id,mode,rebuilding,enabled,admission_after,execution_epoch) VALUES ($1,$2,'synthetic',false,true,now()-interval '1 day',1)").bind(&source).bind(property).execute(&store.pool).await?;
    let raw = raw_event(&source, "first", "1", "pms.order.created");
    let event = protocol::VerifiedEvent::from_feed(&raw, &source, property)?;
    let (a, b) = tokio::join!(store.accept(&event), store.accept(&event));
    let a = a?;
    let b = b?;
    assert_eq!(a.receipt_id, b.receipt_id);
    assert_ne!(a.status, b.status);
    let inbox_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.welcome_inbox WHERE source_instance=$1",
    )
    .bind(&source)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(inbox_count, 1);
    let conflicting = protocol::VerifiedEvent::from_feed(
        &raw_event(&source, "first", "2", "pms.order.created"),
        &source,
        property,
    )?;
    assert!(store.accept(&conflicting).await.is_err());
    let page_event = protocol::VerifiedEvent::from_feed(
        &raw_event(&source, "page", "2", "pms.order.created"),
        &source,
        property,
    )?;
    assert!(store
        .accept_page(
            &source,
            property,
            None,
            "cursor-2",
            &[page_event, conflicting]
        )
        .await
        .is_err());
    let cursor:Option<String>=sqlx::query_scalar("SELECT cursor FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2").bind(&source).bind(property).fetch_one(&store.pool).await?;
    assert!(cursor.is_none());
    assert!(store
        .claim("different-source", property, Uuid::new_v4())
        .await?
        .is_none());
    let claim = store
        .claim(&source, property, Uuid::new_v4())
        .await?
        .unwrap();
    sqlx::query("UPDATE qintopia_agent_os.work_items SET claim_expires_at=now()-interval '1 second' WHERE id=$1").bind(claim.work_item_id).execute(&store.pool).await?;
    let new_claim = store
        .claim(&source, property, Uuid::new_v4())
        .await?
        .unwrap();
    assert!(new_claim.fence > claim.fence);
    assert!(store.finish(&claim, false).await.is_err());
    let mut snapshot = state::Snapshot {
        source_hash: None,
        source: source.clone(),
        property: property.into(),
        order: "fixture-order".into(),
        revision: "1".into(),
        stay: "fixture-stay".into(),
        state: state::StayState::Reserved,
        inventory_reserved: true,
        current_arrangement: true,
        building: "A".into(),
        occupants: vec![
            state::Occupant {
                id: "occupant-A".into(),
                active: true,
            },
            state::Occupant {
                id: "occupant-B".into(),
                active: true,
            },
        ],
        related_revisions: BTreeMap::new(),
        observed_at: Utc::now(),
        business_date: Utc::now().date_naive(),
    };
    let cases = store.consume_snapshot(&new_claim, &snapshot).await?;
    assert_eq!(cases.len(), 2);
    assert!(!store.evaluate(cases[0]).await?.card_ready);
    let person: Uuid = sqlx::query_scalar(
        "INSERT INTO qintopia_identity.persons(display_name) VALUES ('合成小林') RETURNING id",
    )
    .fetch_one(&store.pool)
    .await?;
    let human: Uuid = sqlx::query_scalar(
        "INSERT INTO qintopia_identity.persons(display_name) VALUES ('合成审核员') RETURNING id",
    )
    .fetch_one(&store.pool)
    .await?;
    let namespace = format!("{source}/pms");
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_identity_scopes(namespace,source_instance,property_id) VALUES ($1,$2,$3)").bind(&namespace).bind(&source).bind(property).execute(&store.pool).await?;
    let link:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref) VALUES ($1,'pms_occupant','occupant-A') RETURNING id").bind(&namespace).fetch_one(&store.pool).await?;
    for action in ["identity", "appoint"] {
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_grants(person_id,source_instance,property_id,action,expires_at,appointed_by) VALUES ($1,$2,$3,$4,now()+interval '1 day',$1)").bind(human).bind(&source).bind(property).bind(action).execute(&store.pool).await?;
    }
    let actor = store::Actor {
        person: human,
        source: source.clone(),
        property: property.into(),
        expires: Utc::now() + chrono::Duration::minutes(5),
    };
    let op = Uuid::new_v4();
    let evidence = Uuid::new_v4();
    let confirmed = store
        .change_identity(&actor, op, link, person, 1, evidence, false)
        .await?;
    assert_eq!(
        confirmed,
        store
            .change_identity(&actor, op, link, person, 1, evidence, false)
            .await?
    );
    assert!(store
        .change_identity(&actor, op, link, human, 1, evidence, false)
        .await
        .is_err());
    let application = store
        .application_from_readback(
            &channels::ApplicationRevision {
                source: source.clone(),
                resource: "fixture-application-base".into(),
                record: "fixture-record".into(),
                person: Some(person),
                revision: 1,
                valid: true,
                consent_version: 1,
                consent_active: true,
                field_hash: digest(b"synthetic application"),
            },
            "fixture-application-base",
        )
        .await?;
    store
        .bind_case(&actor, Uuid::new_v4(), cases[0], link, application, 1)
        .await?;
    assert!(store.evaluate(cases[0]).await?.card_ready);
    let foreign_application:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_applications(source_instance,resource_ref,record_ref,person_id,revision,valid,consent_version,consent_active,field_hash) SELECT source_instance||'-other',resource_ref,record_ref,person_id,revision,valid,consent_version,consent_active,field_hash FROM qintopia_agent_os.welcome_applications WHERE id=$1 RETURNING id")
        .bind(application).fetch_one(&store.pool).await?;
    assert!(store
        .bind_case(
            &actor,
            Uuid::new_v4(),
            cases[0],
            link,
            foreign_application,
            2
        )
        .await
        .is_err());
    sqlx::query("UPDATE qintopia_identity.persons SET status='inactive' WHERE id=$1")
        .bind(person)
        .execute(&store.pool)
        .await?;
    assert!(store
        .evaluate(cases[0])
        .await?
        .reasons
        .iter()
        .any(|r| r == "person_inactive"));
    sqlx::query("UPDATE qintopia_identity.persons SET status='active' WHERE id=$1")
        .bind(person)
        .execute(&store.pool)
        .await?;
    assert!(store.evaluate(cases[0]).await?.card_ready);
    assert!(!store.evaluate(cases[1]).await?.card_ready);
    let bytes = b"Synthetic welcome copy";
    let artifact = store
        .register_artifact(&artifact::ArtifactInput {
            case: cases[0],
            expected_version: 2,
            application_revision: 1,
            template: "fixture-v1",
            kind: "welcome_text",
            bytes,
        })
        .await?;
    let target:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_targets(source_instance,property_id,kind,display_name,namespace,conversation_ref,enabled) VALUES ($1,$2,'community','合成社区',$3,'synthetic-conversation',true) RETURNING id")
        .bind(&source).bind(property).bind(format!("{source}/qiwe")).fetch_one(&store.pool).await?;
    let mut grant_ids = vec![];
    for action in ["review", "publish"] {
        let r = store
            .grant(
                &actor,
                &review::GrantRequest {
                    operation: Uuid::new_v4(),
                    person: human,
                    target: Some(target),
                    action: action.into(),
                    membership: None,
                    expires: Utc::now() + chrono::Duration::hours(1),
                },
            )
            .await?;
        grant_ids.push(Uuid::parse_str(r["grant_ref"].as_str().unwrap())?);
    }
    let approval = store
        .approve(
            &actor,
            &review::ReviewRequest {
                operation: Uuid::new_v4(),
                artifact,
                target,
                target_version: 1,
                phase: "formal".into(),
                content_hash: digest(bytes),
            },
        )
        .await?;
    let approval = Uuid::parse_str(approval["approval_ref"].as_str().unwrap())?;
    assert!(store
        .prepare_delivery(cases[0], approval, grant_ids[1])
        .await
        .is_err()); // still reserved
    let now = Utc::now();
    assert!(store
        .reconcile_members(
            target,
            &format!("{source}/qiwe"),
            &["member-A".into()],
            false,
            now
        )
        .await
        .is_err());
    store
        .reconcile_members(
            target,
            &format!("{source}/qiwe"),
            &["member-A".into()],
            true,
            now,
        )
        .await?;
    let channel_link: Uuid = sqlx::query_scalar(
        "SELECT identity_link_id FROM qintopia_agent_os.welcome_members WHERE target_id=$1",
    )
    .bind(target)
    .fetch_one(&store.pool)
    .await?;
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET person_id=$2,status='confirmed',confirmed_by=$3,evidence_ref=$4 WHERE id=$1").bind(channel_link).bind(person).bind(human).bind(Uuid::new_v4()).execute(&store.pool).await?;
    let second = protocol::VerifiedEvent::from_feed(
        &raw_event(&source, "checkin", "2", "pms.stay.checked_in"),
        &source,
        property,
    )?;
    store.accept(&second).await?;
    let claim = store
        .claim(&source, property, Uuid::new_v4())
        .await?
        .unwrap();
    snapshot.revision = "2".into();
    snapshot.state = state::StayState::InHouse;
    snapshot.observed_at = Utc::now();
    assert_eq!(store.consume_snapshot(&claim, &snapshot).await?, cases);
    let action = store
        .prepare_delivery(cases[0], approval, grant_ids[1])
        .await?;
    let claim = store.claim_delivery(action, Uuid::new_v4()).await?;
    sqlx::query("UPDATE qintopia_agent_os.work_items w SET claim_expires_at=now()-interval '1 second' FROM qintopia_agent_os.welcome_actions a WHERE a.id=$1 AND a.work_item_id=w.id").bind(action).execute(&store.pool).await?;
    let active_claim = store.claim_delivery(action, Uuid::new_v4()).await?;
    assert!(store.begin_synthetic_attempt(&claim).await.is_err());
    store.begin_synthetic_attempt(&active_claim).await?;
    store
        .record_outcome(
            &active_claim,
            delivery::Outcome::Success,
            Some(&digest(b"receipt")),
        )
        .await?;
    assert_eq!(
        store
            .prepare_delivery(cases[0], approval, grant_ids[1])
            .await?,
        action
    );
    // Independent image part: upload recovery never overwrites another stay's
    // attachment, and a process crash after sending must not retry blindly.
    let mut image_bytes = std::io::Cursor::new(Vec::new());
    image::DynamicImage::new_rgb8(32, 32).write_to(&mut image_bytes, image::ImageFormat::Png)?;
    let image_bytes = image_bytes.into_inner();
    let card = store
        .register_artifact(&artifact::ArtifactInput {
            case: cases[0],
            expected_version: 2,
            application_revision: 1,
            template: "fixture-v1",
            kind: "welcome_card",
            bytes: &image_bytes,
        })
        .await?;
    let intent = store.upload_intent(card).await?;
    assert_eq!(store.upload_intent(card).await?, intent);
    sqlx::query("UPDATE qintopia_agent_os.welcome_upload_intents SET status='unknown' WHERE id=$1")
        .bind(intent)
        .execute(&store.pool)
        .await?;
    store
        .register_upload_readback(
            intent,
            application,
            &digest(&image_bytes),
            "feishu-base://resident-welcome/synthetic-attachment",
        )
        .await?;
    store
        .register_upload_readback(
            intent,
            application,
            &digest(&image_bytes),
            "feishu-base://resident-welcome/synthetic-attachment",
        )
        .await?;
    assert!(store
        .register_upload_readback(
            intent,
            application,
            &digest(b"wrong"),
            "feishu-base://resident-welcome/synthetic-attachment"
        )
        .await
        .is_err());
    let image_approval = store
        .approve(
            &actor,
            &review::ReviewRequest {
                operation: Uuid::new_v4(),
                artifact: card,
                target,
                target_version: 1,
                phase: "formal".into(),
                content_hash: digest(&image_bytes),
            },
        )
        .await?;
    let image_approval = Uuid::parse_str(image_approval["approval_ref"].as_str().unwrap())?;
    let image_action = store
        .prepare_delivery(cases[0], image_approval, grant_ids[1])
        .await?;
    let image_claim = store.claim_delivery(image_action, Uuid::new_v4()).await?;
    store.begin_synthetic_attempt(&image_claim).await?;
    sqlx::query("UPDATE qintopia_agent_os.work_items w SET claim_expires_at=now()-interval '1 second' FROM qintopia_agent_os.welcome_actions a WHERE a.id=$1 AND a.work_item_id=w.id").bind(image_action).execute(&store.pool).await?;
    assert_eq!(store.recover_sending("another-source", property).await?, 0);
    assert_eq!(store.recover_sending(&source, property).await?, 1);
    assert!(store
        .claim_delivery(image_action, Uuid::new_v4())
        .await
        .is_err());
    let version: i64 =
        sqlx::query_scalar("SELECT version FROM qintopia_agent_os.welcome_actions WHERE id=$1")
            .bind(image_action)
            .fetch_one(&store.pool)
            .await?;
    store
        .resolve_unknown(
            &actor,
            Uuid::new_v4(),
            image_action,
            version,
            true,
            &digest(b"manual provider receipt"),
        )
        .await?;
    assert!(store
        .claim_delivery(image_action, Uuid::new_v4())
        .await
        .is_err());
    let succeeded:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.welcome_actions WHERE case_id=$1 AND status='succeeded'").bind(cases[0]).fetch_one(&store.pool).await?;
    assert_eq!(succeeded, 2);
    assert!(store.claim_delivery(action, Uuid::new_v4()).await.is_err());
    // Actual check-in is already known. Exercise approval-last and roster-last;
    // each target prepares independently, without a timing delay.
    let mut extra_actions = Vec::new();
    let mut extra_grants = Vec::new();
    let mut appointments = Vec::new();
    for review_last in [true, false] {
        let next:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_targets(source_instance,property_id,kind,display_name,namespace,conversation_ref,building_code,enabled) VALUES ($1,$2,'building','合成楼栋',$3,$4,'A',true) RETURNING id")
            .bind(&source).bind(property).bind(format!("{source}/qiwe")).bind(format!("synthetic-building-{review_last}")).fetch_one(&store.pool).await?;
        let mut publish = Uuid::nil();
        let membership:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.person_memberships(person_id,community_key,role,status,started_at) VALUES ($1,$2,'warden','active',now()-interval '1 day') RETURNING id")
            .bind(human).bind(format!("{source}-{review_last}")).fetch_one(&store.pool).await?;
        appointments.push(membership);
        for kind in ["review", "publish"] {
            let grant = store
                .grant(
                    &actor,
                    &review::GrantRequest {
                        operation: Uuid::new_v4(),
                        person: human,
                        target: Some(next),
                        action: kind.into(),
                        membership: Some(membership),
                        expires: Utc::now() + chrono::Duration::hours(1),
                    },
                )
                .await?;
            if kind == "publish" {
                publish = Uuid::parse_str(grant["grant_ref"].as_str().unwrap())?;
            }
        }
        if review_last {
            store
                .reconcile_members(
                    next,
                    &format!("{source}/qiwe"),
                    &["member-A".into()],
                    true,
                    Utc::now(),
                )
                .await?;
        }
        let reviewed = store
            .approve(
                &actor,
                &review::ReviewRequest {
                    operation: Uuid::new_v4(),
                    artifact,
                    target: next,
                    target_version: 1,
                    phase: "formal".into(),
                    content_hash: digest(bytes),
                },
            )
            .await?;
        let approved = Uuid::parse_str(reviewed["approval_ref"].as_str().unwrap())?;
        if !review_last {
            assert!(store
                .prepare_delivery(cases[0], approved, publish)
                .await
                .is_err());
            store
                .reconcile_members(
                    next,
                    &format!("{source}/qiwe"),
                    &["member-A".into()],
                    true,
                    Utc::now(),
                )
                .await?;
        }
        let prepared:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.welcome_actions WHERE case_id=$1 AND target_id=$2 AND status='prepared'").bind(cases[0]).bind(next).fetch_one(&store.pool).await?;
        let attempt = store.claim_delivery(prepared, Uuid::new_v4()).await?;
        store.begin_synthetic_attempt(&attempt).await?;
        store
            .record_outcome(
                &attempt,
                delivery::Outcome::DefiniteNoSend,
                Some(&digest(b"definite rejection")),
            )
            .await?;
        extra_actions.push(prepared);
        extra_grants.push(publish);
    }
    let succeeded:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.welcome_actions WHERE case_id=$1 AND status='succeeded'").bind(cases[0]).fetch_one(&store.pool).await?;
    assert_eq!(succeeded, 2); // failed building targets did not resend community parts
    store
        .revoke_grant(&actor, Uuid::new_v4(), extra_grants[0], 1)
        .await?;
    assert!(store
        .claim_delivery(extra_actions[0], Uuid::new_v4())
        .await
        .is_err());
    sqlx::query("UPDATE qintopia_identity.person_memberships SET ended_at=now()-interval '1 second',status='ended' WHERE id=$1").bind(appointments[1]).execute(&store.pool).await?;
    assert!(store
        .claim_delivery(extra_actions[1], Uuid::new_v4())
        .await
        .is_err());
    store.reconcile_scope(&source, property).await?;
    // Same stay leave/rejoin and artifact revision do not grant another welcome.
    store
        .reconcile_members(target, &format!("{source}/qiwe"), &[], true, Utc::now())
        .await?;
    store
        .reconcile_members(
            target,
            &format!("{source}/qiwe"),
            &["member-A".into()],
            true,
            Utc::now(),
        )
        .await?;
    assert_eq!(
        store
            .prepare_delivery(cases[0], approval, grant_ids[1])
            .await?,
        action
    );
    // A changed building invalidates old content and pending targets, while
    // stable successful actions remain terminal. Cancellation prevents new work.
    snapshot.revision = "3".into();
    snapshot.building = "B".into();
    snapshot.observed_at = Utc::now();
    store.apply_snapshot(None, &snapshot).await?;
    for old in extra_actions {
        assert!(store.claim_delivery(old, Uuid::new_v4()).await.is_err());
    }
    let revoked:bool=sqlx::query_scalar("SELECT revoked_at IS NOT NULL FROM qintopia_agent_os.welcome_artifact_bindings WHERE artifact_id=$1").bind(artifact).fetch_one(&store.pool).await?;
    assert!(revoked);
    snapshot.revision = "4".into();
    snapshot.state = state::StayState::Terminated;
    snapshot.observed_at = Utc::now();
    store.apply_snapshot(None, &snapshot).await?;
    assert!(!store.evaluate(cases[0]).await?.card_ready);
    // Identity revocation invalidates the old binding; sent actions remain sent.
    store
        .change_identity(
            &actor,
            Uuid::new_v4(),
            link,
            person,
            2,
            Uuid::new_v4(),
            true,
        )
        .await?;
    assert!(!store.evaluate(cases[0]).await?.card_ready);
    let status: String =
        sqlx::query_scalar("SELECT status FROM qintopia_agent_os.welcome_actions WHERE id=$1")
            .bind(action)
            .fetch_one(&store.pool)
            .await?;
    assert_eq!(status, "succeeded");
    store
        .require_rebuild(&source, property, Some("head-before-scan"))
        .await?;
    let row=sqlx::query("SELECT enabled,rebuilding FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2").bind(&source).bind(property).fetch_one(&store.pool).await?;
    assert!(!row.get::<bool, _>("enabled") && row.get::<bool, _>("rebuilding"));
    Ok(())
}
