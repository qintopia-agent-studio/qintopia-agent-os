//! Real PostgreSQL contact evidence and original-message confirmation tests.
use super::*;
use serde_json::Value;

impl Fixture {
    async fn contacts(&self, r: Value, message: &str) -> Result<Value> {
        let row=sqlx::query("SELECT binding_id,anan_work_id FROM qintopia_agent_os.application_intake_states WHERE application_id=$1").bind(self.application).fetch_one(&self.store.pool).await?;
        let chat =
            sqlx::query_scalar("SELECT chat_id FROM qintopia_messages.conversations WHERE id=$1")
                .bind(self.group)
                .fetch_one(&self.store.pool)
                .await?;
        let mut r = r;
        r["work_item"] = json!(row.get::<Uuid, _>("anan_work_id"));
        let t = super::super::foundation_server::TrustedContext {
            platform: "wecom".into(),
            chat_type: "group".into(),
            chat_id: chat,
            sender_id: "synthetic-service-account".into(),
            message_id: message.into(),
            gateway_id: self.gateway.clone(),
        };
        self.store
            .welcome_stay_contacts(
                &self.gateway,
                row.get("binding_id"),
                "resident-application",
                &t,
                serde_json::from_value(r)?,
            )
            .await
    }
    async fn contact_order(&self, read: &Value, phone: Option<&str>) -> Result<Value> {
        let projection:Value=sqlx::query_scalar("SELECT v.projection FROM qintopia_agent_os.welcome_source_versions v JOIN qintopia_agent_os.business_property_bindings b ON b.source_instance=v.source_instance AND b.property_id=v.property_id WHERE b.tenant_key=$1 AND v.aggregate_type='order' AND v.aggregate_id=$2").bind(&self.store.tenant).bind(read["order_id"].as_str()).fetch_one(&self.store.pool).await?;
        Ok(
            json!({"id":read["order_id"],"property_id":read["property_id"],"version":read["order_revision"].as_str().unwrap().parse::<u64>()?,"occupants":projection["occupants"].as_array().unwrap().iter().filter(|o|o["active"]==true).map(|o|json!({"id":o["id"],"phone":phone})).collect::<Vec<_>>()}),
        )
    }
    async fn scan_contacts(
        &self,
        phone: Option<&str>,
        presentation: Option<&Value>,
        message: &str,
    ) -> Result<Value> {
        let opened = self
            .contacts(
                json!({"action":"open","refresh":true,"presentation":presentation}),
                message,
            )
            .await?;
        for read in opened["reads"].as_array().unwrap() {
            let order = self.contact_order(read, phone).await?;
            self.contacts(
                json!({"action":"save","read_token":read["read_token"],"order":order}),
                message,
            )
            .await?;
        }
        self.contacts(json!({"action":"status"}), message).await
    }
    async fn deliver_contacts(&self) -> Result<Value> {
        self.reopen(vec![]).await?;
        let prepared = self
            .host(json!({"action":"prepare","work_item":self.work}), "")
            .await?;
        let claim = self
            .host(
                json!({"action":"claim","presentation":prepared["presentation"]}),
                "",
            )
            .await?;
        assert_eq!(claim["send"], true);
        self.host(json!({"action":"receipt","presentation":prepared["presentation"],"claim":claim["claim"],"outcome":"delivered","receipt":"simulated-contact"}),"").await?;
        Ok(prepared)
    }
}
async fn contact_fixture() -> Result<Fixture> {
    let f = Fixture::with_intake(true).await?;
    let fields =
        json!({"name":"模拟候选","nickname":"不同昵称","phone":"+86 138-0000-0000","consent":true});
    f.readback(&matching_observation(&fields, true, true))
        .await?;
    // Fixture provisioning: a known person is available for explicit selection.
    sqlx::query("UPDATE qintopia_agent_os.welcome_applications SET person_id=$2 WHERE id=$1")
        .bind(f.application)
        .bind(f.person)
        .execute(&f.store.pool)
        .await?;
    let binding:Uuid=sqlx::query_scalar("SELECT binding_id FROM qintopia_agent_os.application_intake_states WHERE application_id=$1").bind(f.application).fetch_one(&f.store.pool).await?;
    f.store
        .welcome_project_source(&super::super::store::welcome_candidates::SourceProjection {
            binding,
            application: f.application,
            fields,
        })
        .await?;
    f.reopen(vec![]).await?;
    Ok(f)
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_contacts_complete_pool_phone_first_and_private_metadata() -> Result<()> {
    let f = contact_fixture().await?;
    // More than the UI limit; no Person/name exists for these source candidates.
    for i in 0..23 {
        let order = format!("sim-order-{i}");
        let occupant = format!("sim-occupant-{i}");
        let stay = format!("sim-stay-{i}");
        let projection:Value=sqlx::query_scalar("SELECT v.projection FROM qintopia_agent_os.welcome_source_versions v JOIN qintopia_agent_os.welcome_cases c ON c.source_instance=v.source_instance AND c.property_id=v.property_id AND c.order_id=v.aggregate_id WHERE c.id=$1 AND v.aggregate_type='order'").bind(f.case).fetch_one(&f.store.pool).await?;
        let mut projection = projection;
        projection["occupants"] = json!([{"id":occupant,"active":true}]);
        projection["stay"] = json!(stay);
        projection["order"] = json!(order);
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_source_versions SELECT (jsonb_populate_record(NULL::qintopia_agent_os.welcome_source_versions,to_jsonb(v)||jsonb_build_object('aggregate_id',$2::text,'projection',$3::jsonb))).* FROM qintopia_agent_os.welcome_source_versions v JOIN qintopia_agent_os.welcome_cases c ON c.source_instance=v.source_instance AND c.property_id=v.property_id AND c.order_id=v.aggregate_id WHERE c.id=$1 AND v.aggregate_type='order'").bind(f.case).bind(&order).bind(projection).execute(&f.store.pool).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_cases SELECT (jsonb_populate_record(NULL::qintopia_agent_os.welcome_cases,to_jsonb(c)||jsonb_build_object('id',$2::uuid,'order_id',$3::text,'stay_id',$4::text,'occupant_id',$5::text,'application_id',NULL))).* FROM qintopia_agent_os.welcome_cases c WHERE c.id=$1").bind(f.case).bind(Uuid::new_v4()).bind(order).bind(stay).bind(occupant).execute(&f.store.pool).await?;
    }
    let opened = f.contacts(json!({"action":"open"}), "").await?;
    assert!(opened["orders_total"].as_u64().unwrap() > 20);
    let mut matching = None;
    for (i, read) in opened["reads"].as_array().unwrap().iter().enumerate() {
        let number = if i == 22 {
            "13800000000"
        } else {
            "13900000000"
        };
        if i == 22 {
            matching = Some(read["order_id"].clone());
        }
        let order = f.contact_order(read, Some(number)).await?;
        f.contacts(
            json!({"action":"save","read_token":read["read_token"],"order":order}),
            "",
        )
        .await?;
    }
    let subject = f.subject().await?;
    let candidates = f
        .store
        .welcome_candidates(&subject, f.scope, f.application)
        .await?;
    assert_eq!(candidates["phone_scan_complete"], true);
    assert_eq!(candidates["candidates"][0]["phone_status"], "match");
    assert_eq!(candidates["candidates"].as_array().unwrap().len(), 20);
    let case: Uuid = serde_json::from_value(candidates["candidates"][0]["case_ref"].clone())?;
    let order: String =
        sqlx::query_scalar("SELECT order_id FROM qintopia_agent_os.welcome_cases WHERE id=$1")
            .bind(case)
            .fetch_one(&f.store.pool)
            .await?;
    assert_eq!(Some(json!(order)), matching);
    assert_eq!(candidates["automatic_binding"], false);
    let metadata:Value=sqlx::query_scalar("SELECT w.metadata FROM qintopia_agent_os.work_items w JOIN qintopia_agent_os.application_intake_states i ON i.anan_work_id=w.id WHERE i.application_id=$1").bind(f.application).fetch_one(&f.store.pool).await?;
    for number in ["13800000000", "13900000000", "+86 138"] {
        assert!(!metadata.to_string().contains(number));
        assert!(!candidates.to_string().contains(number));
    }
    assert!(metadata.get("welcome_contact_read_v1").is_some());
    let work: Uuid = serde_json::from_value(opened["work_item"].clone())?;
    let public =
        serde_json::to_value(crate::operations::work_item_status_tree(&f.store.pool, work).await?)?;
    for private in [
        "welcome_contact_read_v1",
        "generation",
        "comparisons",
        "salt",
        "token",
    ] {
        assert!(!public.to_string().contains(private));
        assert!(!candidates.to_string().contains(private));
    }
    let status = f.contacts(json!({"action":"status"}), "").await?;
    assert_eq!(status["status"], "complete");
    assert!(status.get("reads").is_none());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_contacts_failures_replay_revision_and_null_removal() -> Result<()> {
    let f = contact_fixture().await?;
    let opened = f.contacts(json!({"action":"open"}), "").await?;
    let read = opened["reads"][0].clone();
    let order = f.contact_order(&read, Some("13800000000")).await?;
    let saved = json!({"action":"save","read_token":read["read_token"],"order":order});
    assert_eq!(f.contacts(saved.clone(), "").await?["replayed"], false);
    let before:Value=sqlx::query_scalar("SELECT w.metadata->'welcome_contact_read_v1' FROM qintopia_agent_os.work_items w JOIN qintopia_agent_os.application_intake_states i ON i.anan_work_id=w.id WHERE i.application_id=$1").bind(f.application).fetch_one(&f.store.pool).await?;
    assert_eq!(f.contacts(saved.clone(), "").await?["replayed"], true);
    let after:Value=sqlx::query_scalar("SELECT w.metadata->'welcome_contact_read_v1' FROM qintopia_agent_os.work_items w JOIN qintopia_agent_os.application_intake_states i ON i.anan_work_id=w.id WHERE i.application_id=$1").bind(f.application).fetch_one(&f.store.pool).await?;
    assert_eq!(before, after);
    let mut changed = saved.clone();
    changed["order"]["occupants"][0]["phone"] = Value::Null;
    assert!(f
        .contacts(changed, "")
        .await
        .unwrap_err()
        .to_string()
        .contains("contact_read_conflict"));
    assert_eq!(
        f.contacts(json!({"action":"open"}), "").await?["reads"]
            .as_array()
            .unwrap()
            .len(),
        opened["reads"].as_array().unwrap().len() - 1
    );
    let fresh = f
        .contacts(json!({"action":"open","refresh":true}), "")
        .await?;
    assert!(f.contacts(saved, "").await.is_err());
    let read = &fresh["reads"][0];
    f.contacts(
        json!({"action":"failed","read_token":read["read_token"],"reason":"read_failed"}),
        "",
    )
    .await?;
    assert_eq!(
        f.contacts(json!({"action":"status"}), "").await?["status"],
        "incomplete"
    );
    let fresh = f
        .contacts(json!({"action":"open","refresh":true}), "")
        .await?;
    let read = &fresh["reads"][0];
    let mut order = f.contact_order(read, None).await?;
    order["version"] = json!(order["version"].as_u64().unwrap() + 1);
    assert_eq!(
        f.contacts(
            json!({"action":"save","read_token":read["read_token"],"order":order}),
            ""
        )
        .await?["status"],
        "awaiting_source_sync"
    );
    assert_eq!(f.scan_contacts(None, None, "").await?["status"], "complete");
    let list = f
        .store
        .welcome_candidates(&f.subject().await?, f.scope, f.application)
        .await?;
    assert!(list["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|c| c["phone_status"] == "missing"));
    sqlx::query("UPDATE qintopia_agent_os.work_items w SET metadata=jsonb_set(metadata,'{welcome_contact_read_v1,expires}',$2::jsonb) FROM qintopia_agent_os.application_intake_states i WHERE i.anan_work_id=w.id AND i.application_id=$1").bind(f.application).bind(json!(Utc::now()-Duration::seconds(1))).execute(&f.store.pool).await?;
    assert_eq!(
        f.contacts(json!({"action":"status"}), "").await?["status"],
        "stale"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_contacts_same_basis_original_confirmation_once() -> Result<()> {
    let f = contact_fixture().await?;
    assert_eq!(
        f.scan_contacts(Some("13800000000"), None, "").await?["status"],
        "complete"
    );
    let prepared = f.deliver_contacts().await?;
    assert!(prepared["text"].as_str().unwrap().contains("手机号一致"));
    let message = f
        .group_text(
            &format!(
                "确认 {} 人员1 关联住宿",
                prepared["reference"].as_str().unwrap()
            ),
            true,
        )
        .await?;
    let context = f
        .host(json!({"action":"confirmation_context"}), &message)
        .await?;
    assert_eq!(context["requires_contacts"], true);
    assert!(f
        .host(json!({"action":"callback"}), &message)
        .await
        .is_err());
    assert_eq!(
        f.scan_contacts(
            Some("13800000000"),
            Some(&prepared["presentation"]),
            &message
        )
        .await?["status"],
        "complete"
    );
    let result = f.host(json!({"action":"callback"}), &message).await?;
    assert_eq!(result["identity_confirmed"], true);
    assert_eq!(
        f.host(json!({"action":"confirmation_context"}), &message)
            .await?["replayed"],
        true
    );
    assert_eq!(
        f.host(json!({"action":"callback"}), &message).await?,
        result
    );
    let count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.welcome_review_receipts WHERE work_item_id=$1",
    )
    .bind(f.work)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(count, 1);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_contacts_changed_basis_rejects_and_manual_basis_remains_available() -> Result<()> {
    for manual in [false, true] {
        let f = contact_fixture().await?;
        f.scan_contacts(Some("13800000000"), None, "").await?;
        let prepared = f.deliver_contacts().await?;
        let message = f
            .group_text(
                &format!(
                    "确认 {} 人员1 关联住宿{}",
                    prepared["reference"].as_str().unwrap(),
                    if manual { " 人工核对" } else { "" }
                ),
                true,
            )
            .await?;
        let context = f
            .host(json!({"action":"confirmation_context"}), &message)
            .await?;
        assert_eq!(context["requires_contacts"], !manual);
        if manual {
            let fresh = f
                .contacts(json!({"action":"open","refresh":true}), "")
                .await?;
            f.contacts(json!({"action":"failed","read_token":fresh["reads"][0]["read_token"],"reason":"read_unavailable"}),"").await?;
            assert_eq!(
                f.host(json!({"action":"callback"}), &message).await?["identity_confirmed"],
                true
            );
            let basis:String=sqlx::query_scalar("SELECT effects->>'identity_basis' FROM qintopia_agent_os.welcome_review_receipts WHERE work_item_id=$1").bind(f.work).fetch_one(&f.store.pool).await?;
            assert_eq!(basis, "manual_source");
        } else {
            f.scan_contacts(
                Some("13900000000"),
                Some(&prepared["presentation"]),
                &message,
            )
            .await?;
            assert!(f
                .host(json!({"action":"callback"}), &message)
                .await
                .unwrap_err()
                .to_string()
                .contains("contact_confirmation_basis_changed"));
            let count:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.welcome_review_receipts WHERE work_item_id=$1").bind(f.work).fetch_one(&f.store.pool).await?;
            assert_eq!(count, 0);
            let updated = f
                .host(json!({"action":"prepare","work_item":f.work}), "")
                .await?;
            assert_ne!(updated["reference"], prepared["reference"]);
            assert!(updated["text"].as_str().unwrap().contains("手机号不同"));
        }
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_contacts_exact_set_strict_dto_and_shared_numbers() -> Result<()> {
    let f = contact_fixture().await?;
    // Another active occupant on the same order has an independent phone.
    let row=sqlx::query("SELECT source_instance,property_id,order_id FROM qintopia_agent_os.welcome_cases WHERE id=$1").bind(f.case).fetch_one(&f.store.pool).await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET projection=jsonb_set(projection,'{occupants}',(projection->'occupants') || '[{\"id\":\"sim-companion\",\"active\":true}]'::jsonb) WHERE source_instance=$1 AND property_id=$2 AND aggregate_id=$3 AND aggregate_type='order'").bind(row.get::<String,_>("source_instance")).bind(row.get::<String,_>("property_id")).bind(row.get::<String,_>("order_id")).execute(&f.store.pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_cases SELECT (jsonb_populate_record(NULL::qintopia_agent_os.welcome_cases,to_jsonb(c)||jsonb_build_object('id',$2::uuid,'occupant_id','sim-companion','application_id',NULL))).* FROM qintopia_agent_os.welcome_cases c WHERE c.id=$1").bind(f.case).bind(Uuid::new_v4()).execute(&f.store.pool).await?;
    let fresh = f.contacts(json!({"action":"open"}), "").await?;
    let read = fresh["reads"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["order_id"] == json!(row.get::<String, _>("order_id")))
        .unwrap();
    let order = f.contact_order(read, Some("13800000000")).await?;
    assert!(order["occupants"].as_array().unwrap().len() >= 2);
    for value in [json!(true), json!(1.5), json!("1"), json!(-1)] {
        let mut bad = order.clone();
        bad["version"] = value;
        assert!(f
            .contacts(
                json!({"action":"save","read_token":read["read_token"],"order":bad}),
                ""
            )
            .await
            .is_err());
    }
    for mutation in ["missing_phone", "duplicate", "wrong_property", "extra"] {
        let mut bad = order.clone();
        match mutation {
            "missing_phone" => {
                bad["occupants"][0].as_object_mut().unwrap().remove("phone");
            }
            "duplicate" => {
                let first = bad["occupants"][0].clone();
                bad["occupants"].as_array_mut().unwrap().push(first);
            }
            "wrong_property" => bad["property_id"] = json!("other-property"),
            _ => bad["metadata"] = json!({"supplied_hash":"fake"}),
        }
        assert!(
            f.contacts(
                json!({"action":"save","read_token":read["read_token"],"order":bad}),
                ""
            )
            .await
            .is_err(),
            "{mutation}"
        );
    }
    let mut removed = order;
    removed["occupants"].as_array_mut().unwrap().pop();
    assert_eq!(
        f.contacts(
            json!({"action":"save","read_token":read["read_token"],"order":removed}),
            ""
        )
        .await?["read_status"],
        "awaiting_source_sync"
    );
    f.scan_contacts(Some("13800000000"), None, "").await?;
    let list = f
        .store
        .welcome_candidates(&f.subject().await?, f.scope, f.application)
        .await?;
    assert_eq!(list["phone_ambiguous"], true);
    assert_eq!(list["automatic_binding"], false);
    let linked: Option<Uuid> =
        sqlx::query_scalar("SELECT person_id FROM qintopia_agent_os.welcome_cases WHERE id=$1")
            .bind(f.case)
            .fetch_one(&f.store.pool)
            .await?;
    assert!(linked.is_none());
    // Every contact belongs to its occupant, never borrow the lead resident's phone.
    let fresh = f
        .contacts(json!({"action":"open","refresh":true}), "")
        .await?;
    for read in fresh["reads"].as_array().unwrap() {
        let mut order = f.contact_order(read, None).await?;
        for o in order["occupants"].as_array_mut().unwrap() {
            if o["id"] == "sim-companion" {
                o["phone"] = json!("13800000000");
            }
        }
        f.contacts(
            json!({"action":"save","read_token":read["read_token"],"order":order}),
            "",
        )
        .await?;
    }
    let list = f
        .store
        .welcome_candidates(&f.subject().await?, f.scope, f.application)
        .await?;
    assert_eq!(list["phone_ambiguous"], false);
    assert_eq!(list["candidates"][0]["occupant_ref"], "sim-companion");
    assert!(list["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|c| c["occupant_ref"] != "sim-companion")
        .all(|c| c["phone_status"] == "missing"));
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_contacts_original_message_binding_and_independent_effects() -> Result<()> {
    let f = contact_fixture().await?;
    f.scan_contacts(Some("13800000000"), None, "").await?;
    let p = f.deliver_contacts().await?;
    let reference = p["reference"].as_str().unwrap();
    let only_channel = f
        .group_text(&format!("确认 {reference} 人员1 账号1 关联账号"), true)
        .await?;
    assert_eq!(
        f.host(json!({"action":"confirmation_context"}), &only_channel)
            .await?["requires_contacts"],
        false
    );
    let message = f
        .group_text(&format!("确认 {reference} 人员1 关联住宿"), true)
        .await?;
    let refresh = f
        .contacts(
            json!({"action":"open","refresh":true,"presentation":p["presentation"]}),
            &message,
        )
        .await?;
    let read = &refresh["reads"][0];
    let order = f.contact_order(read, Some("13800000000")).await?;
    assert!(f
        .contacts(
            json!({"action":"save","read_token":read["read_token"],"order":order}),
            &only_channel
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("contact_confirmation_context_mismatch"));
    // An unrelated normal refresh cannot satisfy this human message's refresh proof.
    f.scan_contacts(Some("13800000000"), None, "").await?;
    assert!(f
        .host(json!({"action":"callback"}), &message)
        .await
        .unwrap_err()
        .to_string()
        .contains("contact_confirmation_refresh_required"));
    f.scan_contacts(Some("13800000000"), Some(&p["presentation"]), &message)
        .await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_review_subject_grants SET valid_until=clock_timestamp()-interval '1 second' WHERE tenant_key=$1").bind(&f.store.tenant).execute(&f.store.pool).await?;
    assert!(f
        .host(json!({"action":"callback"}), &message)
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_contacts_source_changes_and_existing_identity_are_independent() -> Result<()> {
    let f = contact_fixture().await?;
    f.scan_contacts(Some("13800000000"), None, "").await?;
    let p = f.deliver_contacts().await?;
    let message = f
        .group_text(
            &format!("确认 {} 人员1 关联住宿", p["reference"].as_str().unwrap()),
            true,
        )
        .await?;
    f.scan_contacts(Some("13800000000"), Some(&p["presentation"]), &message)
        .await?;
    f.host(json!({"action":"callback"}), &message).await?;
    // The newly confirmed relationship changes the case basis. Expired read evidence
    // cannot revoke it or force future content/account-only decisions to use phones.
    assert_eq!(
        f.contacts(json!({"action":"status"}), "").await?["status"],
        "stale"
    );
    let linked: Option<Uuid> =
        sqlx::query_scalar("SELECT person_id FROM qintopia_agent_os.welcome_cases WHERE id=$1")
            .bind(f.case)
            .fetch_one(&f.store.pool)
            .await?;
    assert_eq!(linked, Some(f.person));
    f.scan_contacts(Some("13800000000"), None, "").await?;
    let p = f.deliver_contacts().await?;
    let message = f
        .group_text(
            &format!("确认 {} 人员1 关联住宿", p["reference"].as_str().unwrap()),
            true,
        )
        .await?;
    assert_eq!(
        f.host(json!({"action":"confirmation_context"}), &message)
            .await?["requires_contacts"],
        false
    );
    let fresh = f
        .contacts(json!({"action":"open","refresh":true}), "")
        .await?;
    let read = &fresh["reads"][0];
    let order = f.contact_order(read, Some("13800000000")).await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions v SET projection=jsonb_set(projection,'{revision}',to_jsonb(((projection->>'revision')::bigint+1)::text)) FROM qintopia_agent_os.welcome_cases c WHERE c.id=$1 AND v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_id=c.order_id AND v.aggregate_type='order'").bind(f.case).execute(&f.store.pool).await?;
    assert_eq!(
        f.contacts(json!({"action":"status"}), "").await?["status"],
        "stale"
    );
    assert!(f
        .contacts(
            json!({"action":"save","read_token":read["read_token"],"order":order}),
            ""
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("contact_read_stale"));
    sqlx::query(
        "UPDATE qintopia_agent_os.welcome_applications SET consent_active=false WHERE id=$1",
    )
    .bind(f.application)
    .execute(&f.store.pool)
    .await?;
    assert!(f
        .contacts(json!({"action":"open","refresh":true}), "")
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_contacts_independent_relation_requires_exact_application_and_source() -> Result<()>
{
    for mutation in [
        "other_application",
        "revoked_source",
        "wrong_occupant",
        "stale_identity_version",
    ] {
        let f = contact_fixture().await?;
        let p = f.deliver_contacts().await?;
        let message = f
            .group_text(
                &format!("确认 {} 人员1 关联住宿", p["reference"].as_str().unwrap()),
                true,
            )
            .await?;
        assert_eq!(
            f.host(json!({"action":"callback"}), &message).await?["identity_confirmed"],
            true
        );
        match mutation {
            "other_application" => {
                let other = Uuid::new_v4();
                sqlx::query("INSERT INTO qintopia_agent_os.welcome_applications SELECT (jsonb_populate_record(NULL::qintopia_agent_os.welcome_applications,to_jsonb(a)||jsonb_build_object('id',$2::uuid,'record_ref',$3::text))).* FROM qintopia_agent_os.welcome_applications a WHERE a.id=$1").bind(f.application).bind(other).bind(format!("sim-other-{other}")).execute(&f.store.pool).await?;
                sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET application_id=$2,version=version+1 WHERE id=$1").bind(f.case).bind(other).execute(&f.store.pool).await?;
            }
            "revoked_source" => {
                sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked',version=version+1 WHERE id=(SELECT identity_link_id FROM qintopia_agent_os.welcome_cases WHERE id=$1)").bind(f.case).execute(&f.store.pool).await?;
            }
            "wrong_occupant" => {
                sqlx::query("UPDATE qintopia_identity.source_identity_links SET source_ref=source_ref||'-different',version=version+1 WHERE id=(SELECT identity_link_id FROM qintopia_agent_os.welcome_cases WHERE id=$1)").bind(f.case).execute(&f.store.pool).await?;
            }
            _ => {
                sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET identity_version=identity_version+1,version=version+1 WHERE id=$1").bind(f.case).execute(&f.store.pool).await?;
            }
        }
        f.scan_contacts(Some("13800000000"), None, "").await?;
        let p = f.deliver_contacts().await?;
        let message = f
            .group_text(
                &format!("确认 {} 人员1 关联住宿", p["reference"].as_str().unwrap()),
                true,
            )
            .await?;
        assert_eq!(
            f.host(json!({"action":"confirmation_context"}), &message)
                .await?["requires_contacts"],
            true,
            "{mutation}"
        );
        assert!(
            f.host(json!({"action":"callback"}), &message)
                .await
                .unwrap_err()
                .to_string()
                .contains("contact_confirmation_refresh_required"),
            "{mutation}"
        );
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_contacts_real_broker_private_host_and_original_confirmation() -> Result<()> {
    // No HOST/PROFILE mutation reaches other native tests, even when run in parallel.
    if std::env::var("QINTOPIA_CONTACTS_JOURNEY_CHILD").as_deref() != Ok("1") {
        let output = tokio::task::spawn_blocking(|| {
            std::process::Command::new(std::env::current_exe()?)
                .arg("person_collaboration::welcome_review_tests::contacts::welcome_contacts_real_broker_private_host_and_original_confirmation")
                .args(["--exact", "--ignored", "--nocapture"])
                .env("QINTOPIA_CONTACTS_JOURNEY_CHILD", "1")
                .output()
        }).await??;
        // Keep libtest's child summary out of the parent runner's count; retain full
        // diagnostics on failure and only the journey's JSON evidence on success.
        anyhow::ensure!(
            output.status.success(),
            "contacts_child_failed:\n{}\n{}",
            String::from_utf8_lossy(&output.stdout),
            String::from_utf8_lossy(&output.stderr)
        );
        for line in String::from_utf8_lossy(&output.stdout)
            .lines()
            .filter(|line| line.starts_with('{'))
        {
            println!("{line}");
        }
        return Ok(());
    }
    for mode in ["same", "changed", "manual", "existing", "cross_application"] {
        let f = contact_fixture().await?;
        if matches!(mode, "existing" | "cross_application") {
            let initial = f.deliver_contacts().await?;
            let message = f
                .group_text(
                    &format!(
                        "确认 {} 人员1 关联住宿",
                        initial["reference"].as_str().unwrap()
                    ),
                    true,
                )
                .await?;
            assert_eq!(
                f.host(json!({"action":"callback"}), &message).await?["identity_confirmed"],
                true
            );
            if mode == "cross_application" {
                let other = Uuid::new_v4();
                sqlx::query("INSERT INTO qintopia_agent_os.welcome_applications SELECT (jsonb_populate_record(NULL::qintopia_agent_os.welcome_applications,to_jsonb(a)||jsonb_build_object('id',$2::uuid,'record_ref',$3::text))).* FROM qintopia_agent_os.welcome_applications a WHERE a.id=$1").bind(f.application).bind(other).bind(format!("sim-other-{other}")).execute(&f.store.pool).await?;
                sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET application_id=$2,version=version+1 WHERE id=$1").bind(f.case).bind(other).execute(&f.store.pool).await?;
            }
        }
        let dir = tempfile::tempdir()?;
        let socket = dir.path().join("foundation.sock");
        let file = dir.path().join("synthetic-contacts.json");
        let binding:Uuid=sqlx::query_scalar("SELECT binding_id FROM qintopia_agent_os.application_intake_states WHERE application_id=$1").bind(f.application).fetch_one(&f.store.pool).await?;
        let opened = f.contacts(json!({"action":"open"}), "").await?;
        let mut orders = Vec::new();
        for read in opened["reads"].as_array().unwrap() {
            orders.push(f.contact_order(read, Some("13800000000")).await?);
        }
        let chat: String =
            sqlx::query_scalar("SELECT chat_id FROM qintopia_messages.conversations WHERE id=$1")
                .bind(f.group)
                .fetch_one(&f.store.pool)
                .await?;
        let root = std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("../..");
        let mut fixture = json!({"application_work":opened["work_item"],"orders":orders,
            "context":{"platform":"wecom","chat_type":"group","chat_id":chat,
                "sender_id":"synthetic-service-account","message_id":"","gateway_id":f.gateway}});
        for (key, value) in [
            (
                "QINTOPIA_FOUNDATION_SOCKET",
                socket.to_string_lossy().into_owned(),
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
            ("QINTOPIA_APPLICATION_BINDING", binding.to_string()),
            (
                "QINTOPIA_APPLICATION_RESOURCE_ALIAS",
                "resident-application".into(),
            ),
            ("ANAN_CONTACTS_REPO", root.to_string_lossy().into_owned()),
            ("ANAN_CONTACTS_FIXTURE", file.to_string_lossy().into_owned()),
        ] {
            std::env::set_var(key, value);
        }
        let store = Store {
            pool: f.store.pool.clone(),
            tenant: f.store.tenant.clone(),
        };
        let broker = tokio::spawn(super::super::foundation_server::broker(store));
        let outcome:Result<()>=async {
            tokio::time::timeout(std::time::Duration::from_secs(5),async {
                loop {
                    if let Ok(stream)=tokio::net::UnixStream::connect(&socket).await {
                        use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
                        let (read,mut write)=stream.into_split();
                        write.write_all(b"{}\n").await?;
                        let mut line=String::new();
                        BufReader::new(read).read_line(&mut line).await?;
                        anyhow::ensure!(serde_json::from_str::<Value>(&line)?["ok"]==false,"broker_readiness_failed");
                        return Ok::<(),anyhow::Error>(());
                    }
                    tokio::task::yield_now().await;
                }
            }).await??;
            std::fs::write(&file,serde_json::to_vec(&fixture)?)?;
            run_contacts_journey("prime").await?;
            let prepared=f.deliver_contacts().await?;
            let message=f.group_text(&format!("确认 {} 人员1 关联住宿{}",prepared["reference"].as_str().unwrap(),if mode=="manual" {" 人工核对"}else{""}),true).await?;
            fixture["context"]["message_id"]=json!(message);
            if matches!(mode,"manual"|"existing") {
                // Expiration is RFC3339 evidence only; it never revokes identity.
                sqlx::query("UPDATE qintopia_agent_os.work_items SET metadata=jsonb_set(metadata,'{welcome_contact_read_v1,expires}',to_jsonb($2::text)) WHERE id=$1")
                    .bind(Uuid::parse_str(opened["work_item"].as_str().unwrap())?)
                    .bind((Utc::now()-Duration::minutes(10)).to_rfc3339()).execute(&f.store.pool).await?;
                assert_ne!(f.contacts(json!({"action":"status"}),"").await?["status"],"complete");
            }
            if matches!(mode,"changed"|"cross_application") {
                for order in fixture["orders"].as_array_mut().unwrap() {
                    for occupant in order["occupants"].as_array_mut().unwrap() {occupant["phone"]=json!("13900000000");}
                }
            }
            std::fs::write(&file,serde_json::to_vec(&fixture)?)?;
            let before:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.welcome_review_receipts WHERE work_item_id=$1").bind(f.work).fetch_one(&f.store.pool).await?;
            run_contacts_journey(mode).await?;
            let after:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.welcome_review_receipts WHERE work_item_id=$1").bind(f.work).fetch_one(&f.store.pool).await?;
            assert_eq!(after-before,if matches!(mode,"changed"|"cross_application"){0}else{1},"{mode}");
            if mode=="manual" {
                let basis:String=sqlx::query_scalar("SELECT effects->>'identity_basis' FROM qintopia_agent_os.welcome_review_receipts WHERE work_item_id=$1 ORDER BY created_at DESC LIMIT 1").bind(f.work).fetch_one(&f.store.pool).await?;
                assert_eq!(basis,"manual_source");
            }
            if mode=="existing" {
                let person:Option<Uuid>=sqlx::query_scalar("SELECT person_id FROM qintopia_agent_os.welcome_cases WHERE id=$1").bind(f.case).fetch_one(&f.store.pool).await?;
                assert_eq!(person,Some(f.person));
            }
            Ok(())
        }.await;
        broker.abort();
        let _ = broker.await;
        outcome?;
    }
    Ok(())
}

async fn run_contacts_journey(mode: &str) -> Result<()> {
    let mode = mode.to_owned();
    let status = tokio::task::spawn_blocking(move || {
        std::process::Command::new("python3")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../skills/pms-operations/tests/stay_contacts_broker_journey.py"
            ))
            .arg(mode)
            .status()
    })
    .await??;
    anyhow::ensure!(status.success(), "contacts_python_journey_failed");
    Ok(())
}
