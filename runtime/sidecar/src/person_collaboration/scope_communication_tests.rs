use super::*;
use crate::person_collaboration::store::scope_communication::{
    CommunicationContactSelection, CommunicationSelection,
};

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn scope_communication_candidates_and_current_route_fail_closed() -> Result<()> {
    let f = Fixture::new().await?;
    let scope: Uuid = sqlx::query_scalar(
        "SELECT scope_id FROM qintopia_agent_os.business_property_bindings WHERE id=$1",
    )
    .bind(f.binding)
    .fetch_one(&f.store.pool)
    .await?;
    let person = f.store.verified_person(&f.actor).await?;
    let source_link: Uuid = sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND subject_type='wecom_internal' AND person_id=$2")
        .bind(&f.store.tenant).bind(person).fetch_one(&f.store.pool).await?;
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET adapter_metadata=jsonb_build_object('first_observation_ref',$2::text) WHERE id=$1")
        .bind(source_link).bind(Uuid::new_v4()).execute(&f.store.pool).await?;
    let chat = format!("simulated-staff-group-{}", Uuid::new_v4());
    let conversation: Uuid = sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type,display_name) VALUES($1,'wecom',$2,'group','模拟工作群') RETURNING id")
        .bind(&f.store.tenant).bind(&chat).fetch_one(&f.store.pool).await?;
    let group_binding: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3) RETURNING id")
        .bind(&f.store.tenant).bind(scope).bind(conversation).fetch_one(&f.store.pool).await?;
    let other_conversation: Uuid = sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type,display_name) VALUES($1,'wecom',$2,'group','另一模拟工作群') RETURNING id")
        .bind(&f.store.tenant).bind(format!("simulated-staff-group-{}",Uuid::new_v4())).fetch_one(&f.store.pool).await?;
    let other_binding: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3) RETURNING id")
        .bind(&f.store.tenant).bind(scope).bind(other_conversation).fetch_one(&f.store.pool).await?;
    let collaboration: Uuid = sqlx::query_scalar(
        "SELECT collaboration_id FROM qintopia_agent_os.collaboration_grants WHERE id=$1",
    )
    .bind(f.grant)
    .fetch_one(&f.store.pool)
    .await?;
    let parent: Uuid = sqlx::query_scalar(
        "SELECT parent_grant_id FROM qintopia_agent_os.collaboration_grants WHERE id=$1",
    )
    .bind(f.grant)
    .fetch_one(&f.store.pool)
    .await?;
    let duty: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_duties(tenant_key,label,description,domain_key) VALUES($1,$2,'模拟工作群联系职责','hospitality') RETURNING id")
        .bind(&f.store.tenant).bind(format!("模拟联系-{}",Uuid::new_v4())).fetch_one(&f.store.pool).await?;
    sqlx::query("UPDATE qintopia_agent_os.agent_collaborations SET duty_id=$2 WHERE id=$1")
        .bind(collaboration)
        .bind(duty)
        .execute(&f.store.pool)
        .await?;
    let audience_grant: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id,decision_mode) VALUES($1,$2,'read_business',$3,'autonomous') RETURNING id")
        .bind(&f.store.tenant).bind(collaboration).bind(parent).fetch_one(&f.store.pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_audiences(tenant_key,collaboration_id,configuration) VALUES($1,$2,$3)")
        .bind(&f.store.tenant).bind(collaboration).bind(json!({"proactive":"autonomous","groups":[conversation,other_conversation]})).execute(&f.store.pool).await?;

    let groups = f
        .store
        .scope_communication_candidates(&f.actor, scope, "groups", "模拟工作", 50, None)
        .await?;
    assert_eq!(groups["groups"].as_array().map(Vec::len), Some(2));
    assert_eq!(groups["contacts"], json!([]));
    let first_page = f
        .store
        .scope_communication_candidates(&f.actor, scope, "groups", "", 1, None)
        .await?;
    assert_eq!(first_page["has_more"], true);
    let after = first_page["next"].as_str().unwrap();
    let second_page = f
        .store
        .scope_communication_candidates(&f.actor, scope, "groups", "", 1, Some(after))
        .await?;
    assert_eq!(second_page["has_more"], false);
    let page_ids = [
        first_page["groups"][0]["binding_id"].clone(),
        second_page["groups"][0]["binding_id"].clone(),
    ];
    assert!(page_ids.contains(&json!(group_binding)));
    assert!(page_ids.contains(&json!(other_binding)));
    assert!(f
        .store
        .scope_communication_candidates(&f.actor, Uuid::new_v4(), "groups", "", 1, Some(after))
        .await
        .is_err());
    assert!(f
        .store
        .scope_communication_candidates(&f.actor, scope, "groups", "模拟", 1, Some(after))
        .await
        .is_err());
    let contacts = f
        .store
        .scope_communication_candidates(&f.actor, scope, "contacts", "", 50, None)
        .await?;
    assert_eq!(
        contacts["contacts"][0]["channel_source_link_id"],
        json!(source_link)
    );
    let gateway = f.gateway.as_str();
    let searched = f
        .store
        .scope_communication_candidates(&f.actor, scope, "contacts", gateway, 50, None)
        .await?;
    assert_eq!(
        searched["contacts"][0]["channel_source_link_id"],
        json!(source_link)
    );
    let qiwe_gateway = format!("simulated-qiwe-{}", Uuid::new_v4());
    let qiwe_link: Uuid = sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by,adapter_metadata) VALUES($1,'qiwe_sender',$2,$3,'confirmed',$4,$3,jsonb_build_object('first_observation_ref',$4::text)) RETURNING id")
        .bind(&f.store.tenant).bind(format!("simulated-qiwe-user-{}",Uuid::new_v4())).bind(person).bind(Uuid::new_v4()).fetch_one(&f.store.pool).await?;
    sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$1,'qiwe_sender',$3,'personal',true)")
        .bind(&f.store.tenant).bind(&qiwe_gateway).bind(scope).execute(&f.store.pool).await?;
    let selection = CommunicationSelection {
        staff_group_binding_id: group_binding,
        contacts: vec![
            CommunicationContactSelection {
                subject_kind: "person".into(),
                subject_id: person,
                channel_source_link_id: source_link,
            },
            CommunicationContactSelection {
                subject_kind: "person".into(),
                subject_id: person,
                channel_source_link_id: qiwe_link,
            },
        ],
    };
    let mut tx = f.store.pool.begin().await?;
    let saved = f
        .store
        .validate_scope_communication_in(&mut tx, scope, &selection)
        .await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_scopes SET communication_config=$2,version=version+1 WHERE id=$1")
        .bind(scope).bind(saved).execute(&mut *tx).await?;
    tx.commit().await?;
    let current = f
        .store
        .scope_communication_current(&f.gateway, f.binding)
        .await?;
    assert_eq!(current["current"], true);
    assert_eq!(current["staff_group"]["chat_id"], json!(chat));
    assert_eq!(current["contacts"][0]["platform"], "wecom");
    assert_eq!(current["contacts"][1]["platform"], "qiwe");

    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked' WHERE id=$1")
        .bind(audience_grant)
        .execute(&f.store.pool)
        .await?;
    assert_eq!(
        f.store
            .scope_communication_current(&f.gateway, f.binding)
            .await?["current"],
        false
    );
    let revoked = f
        .store
        .scope_communication_candidates(&f.actor, scope, "groups", "", 50, None)
        .await?;
    assert_eq!(revoked["groups"], json!([]));
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='active' WHERE id=$1")
        .bind(audience_grant)
        .execute(&f.store.pool)
        .await?;
    assert_eq!(
        f.store
            .scope_communication_current(&f.gateway, f.binding)
            .await?["current"],
        true
    );
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked' WHERE id=$1")
        .bind(parent)
        .execute(&f.store.pool)
        .await?;
    assert_eq!(
        f.store
            .scope_communication_current(&f.gateway, f.binding)
            .await?["current"],
        false
    );
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='active' WHERE id=$1")
        .bind(parent)
        .execute(&f.store.pool)
        .await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_duties SET status='retired' WHERE id=$1")
        .bind(duty)
        .execute(&f.store.pool)
        .await?;
    assert_eq!(
        f.store
            .scope_communication_current(&f.gateway, f.binding)
            .await?["current"],
        false
    );
    sqlx::query("UPDATE qintopia_agent_os.collaboration_duties SET status='active' WHERE id=$1")
        .bind(duty)
        .execute(&f.store.pool)
        .await?;
    sqlx::query("UPDATE qintopia_agent_os.agent_collaborations SET status='ended' WHERE id=$1")
        .bind(collaboration)
        .execute(&f.store.pool)
        .await?;
    assert_eq!(
        f.store
            .scope_communication_current(&f.gateway, f.binding)
            .await?["current"],
        false
    );
    sqlx::query("UPDATE qintopia_agent_os.agent_collaborations SET status='active' WHERE id=$1")
        .bind(collaboration)
        .execute(&f.store.pool)
        .await?;
    assert_eq!(
        f.store
            .scope_communication_current(&f.gateway, f.binding)
            .await?["current"],
        true
    );
    let role: Uuid = sqlx::query_scalar("SELECT a.role_id FROM qintopia_agent_os.collaboration_appointments a JOIN qintopia_agent_os.agent_collaborations c ON c.appointment_id=a.id WHERE c.id=$1")
        .bind(collaboration).fetch_one(&f.store.pool).await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_roles SET status='retired' WHERE id=$1")
        .bind(role)
        .execute(&f.store.pool)
        .await?;
    assert_eq!(
        f.store
            .scope_communication_current(&f.gateway, f.binding)
            .await?["current"],
        false
    );
    sqlx::query("UPDATE qintopia_agent_os.collaboration_roles SET status='active' WHERE id=$1")
        .bind(role)
        .execute(&f.store.pool)
        .await?;

    sqlx::query(
        "UPDATE qintopia_messages.conversations SET display_name='更新后的模拟工作群' WHERE id=$1",
    )
    .bind(conversation)
    .execute(&f.store.pool)
    .await?;
    let renamed = f
        .store
        .scope_communication_current(&f.gateway, f.binding)
        .await?;
    assert_eq!(renamed["current"], true);
    assert_eq!(renamed["staff_group"]["label"], "更新后的模拟工作群");

    sqlx::query("UPDATE qintopia_agent_os.collaboration_scope_bindings SET revoked_at=clock_timestamp(),version=version+1 WHERE id=$1")
        .bind(group_binding).execute(&f.store.pool).await?;
    let stale = f
        .store
        .scope_communication_current(&f.gateway, f.binding)
        .await?;
    assert_eq!(stale["current"], false);
    assert!(stale["staff_group"].is_null());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn scope_communication_group_pages_recheck_authority() -> Result<()> {
    let f = Fixture::new().await?;
    let scope: Uuid = sqlx::query_scalar(
        "SELECT scope_id FROM qintopia_agent_os.business_property_bindings WHERE id=$1",
    )
    .bind(f.binding)
    .fetch_one(&f.store.pool)
    .await?;
    let row = sqlx::query("SELECT g.collaboration_id,g.parent_grant_id FROM qintopia_agent_os.collaboration_grants g WHERE g.id=$1")
        .bind(f.grant).fetch_one(&f.store.pool).await?;
    let collaboration: Uuid = row.get("collaboration_id");
    let parent: Uuid = row.get("parent_grant_id");
    let duty: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_duties(tenant_key,label,description,domain_key) VALUES($1,$2,'分页候选模拟职责','hospitality') RETURNING id")
        .bind(&f.store.tenant).bind(format!("分页职责-{}",Uuid::new_v4())).fetch_one(&f.store.pool).await?;
    sqlx::query("UPDATE qintopia_agent_os.agent_collaborations SET duty_id=$2 WHERE id=$1")
        .bind(collaboration)
        .bind(duty)
        .execute(&f.store.pool)
        .await?;
    let authority: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id,decision_mode) VALUES($1,$2,'read_business',$3,'autonomous') RETURNING id")
        .bind(&f.store.tenant).bind(collaboration).bind(parent).fetch_one(&f.store.pool).await?;
    let prefix = format!("simulated-pages-{}", Uuid::new_v4());
    let groups: Vec<Uuid> = sqlx::query_scalar("WITH created AS (INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type,display_name) SELECT $1,'wecom',$2||'-'||n::text,'group','分页模拟工作群' FROM generate_series(1,257) AS n RETURNING id), bound AS (INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) SELECT $1,$3,id FROM created RETURNING conversation_id) SELECT array_agg(conversation_id) FROM bound")
        .bind(&f.store.tenant).bind(&prefix).bind(scope).fetch_one(&f.store.pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_audiences(tenant_key,collaboration_id,configuration) VALUES($1,$2,$3)")
        .bind(&f.store.tenant).bind(collaboration).bind(json!({"proactive":"autonomous","groups":groups})).execute(&f.store.pool).await?;
    let mut after: Option<String> = None;
    let mut seen = std::collections::BTreeSet::new();
    let mut pages = 0;
    loop {
        let result = f
            .store
            .scope_communication_candidates(
                &f.actor,
                scope,
                "groups",
                "分页模拟",
                100,
                after.as_deref(),
            )
            .await?;
        pages += 1;
        for group in result["groups"].as_array().unwrap() {
            seen.insert(serde_json::from_value::<Uuid>(group["binding_id"].clone())?);
        }
        after = result["next"].as_str().map(str::to_owned);
        if after.is_none() {
            break;
        }
    }
    assert_eq!(pages, 3);
    assert_eq!(seen.len(), 257);
    let first = f
        .store
        .scope_communication_candidates(&f.actor, scope, "groups", "分页模拟", 100, None)
        .await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked' WHERE id=$1")
        .bind(authority)
        .execute(&f.store.pool)
        .await?;
    let denied = f
        .store
        .scope_communication_candidates(
            &f.actor,
            scope,
            "groups",
            "分页模拟",
            100,
            first["next"].as_str(),
        )
        .await?;
    assert_eq!(denied["groups"], json!([]));
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn scope_communication_account_pages_keep_channels_and_recheck_manager() -> Result<()> {
    let f = Fixture::new().await?;
    let scope: Uuid = sqlx::query_scalar(
        "SELECT scope_id FROM qintopia_agent_os.business_property_bindings WHERE id=$1",
    )
    .bind(f.binding)
    .fetch_one(&f.store.pool)
    .await?;
    let manager_grant: Uuid = sqlx::query_scalar(
        "SELECT parent_grant_id FROM qintopia_agent_os.collaboration_grants WHERE id=$1",
    )
    .bind(f.grant)
    .fetch_one(&f.store.pool)
    .await?;
    let manager = f.store.verified_person(&f.actor).await?;
    let wecom_namespace = format!("simulated-wecom-accounts-{}", Uuid::new_v4());
    let qiwe_namespace = format!("simulated-qiwe-accounts-{}", Uuid::new_v4());
    let wecom_gateway = format!("simulated-wecom-gateway-{}", Uuid::new_v4());
    let qiwe_gateway = format!("simulated-qiwe-gateway-{}", Uuid::new_v4());
    for (namespace, gateway, source_type) in [
        (&wecom_namespace, &wecom_gateway, "wecom_internal"),
        (&qiwe_namespace, &qiwe_gateway, "qiwe_sender"),
    ] {
        sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,$4,$5,'shared',true)")
            .bind(&f.store.tenant).bind(gateway).bind(namespace).bind(source_type).bind(scope)
            .execute(&f.store.pool).await?;
    }

    // One channel supplies 256 rows; the other supplies the 257th with the same label.
    sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,adapter_metadata) SELECT $1,'wecom_internal','observed-'||n::text,jsonb_build_object('first_observation_ref',$2::text,'display_name','模拟同名工作号') FROM generate_series(1,256) n")
        .bind(&wecom_namespace).bind(Uuid::new_v4()).execute(&f.store.pool).await?;
    let qiwe_observed: Uuid = sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,adapter_metadata) VALUES($1,'qiwe_sender','observed-qiwe',jsonb_build_object('first_observation_ref',$2::text,'display_name','模拟同名工作号')) RETURNING id")
        .bind(&qiwe_namespace).bind(Uuid::new_v4()).fetch_one(&f.store.pool).await?;
    sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,adapter_metadata) SELECT $1,'wecom_internal','account-'||n::text,jsonb_build_object('first_observation_ref',$2::text,'display_name','模拟同名工作号') FROM generate_series(1,256) n")
        .bind(&wecom_namespace).bind(Uuid::new_v4()).execute(&f.store.pool).await?;
    sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,adapter_metadata) VALUES($1,'qiwe_sender','account-qiwe',jsonb_build_object('first_observation_ref',$2::text,'display_name','模拟同名工作号'))")
        .bind(&qiwe_namespace).bind(Uuid::new_v4()).execute(&f.store.pool).await?;
    for (namespace, gateway, source_type) in [
        (&wecom_namespace, &wecom_gateway, "wecom_internal"),
        (&qiwe_namespace, &qiwe_gateway, "qiwe_sender"),
    ] {
        sqlx::query("INSERT INTO qintopia_identity.work_accounts(tenant_key,source_link_id,source_version,gateway_key,gateway_version,label,verified_by,evidence_ref) SELECT $1,l.id,l.version,g.gateway_key,g.version,'模拟同名工作号',$4,$5 FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type WHERE l.namespace=$2 AND l.subject_type=$3 AND l.source_ref LIKE 'account-%' AND g.gateway_key=$6")
            .bind(&f.store.tenant).bind(namespace).bind(source_type).bind(manager).bind(Uuid::new_v4()).bind(gateway)
            .execute(&f.store.pool).await?;
    }

    let mut first_cursors = Vec::new();
    for (kind, id_key) in [("observed_accounts", "source_link"), ("accounts", "id")] {
        let mut after: Option<String> = None;
        let mut seen = std::collections::BTreeSet::new();
        let mut pages = 0;
        let mut qiwe_entry = None;
        let mut wecom_entry = None;
        loop {
            let page = f
                .store
                .scope_communication_candidates(
                    &f.actor,
                    scope,
                    kind,
                    "模拟同名工作号",
                    100,
                    after.as_deref(),
                )
                .await?;
            pages += 1;
            let entries = page[kind].as_array().unwrap();
            assert!(!entries.is_empty() && entries.len() <= 100);
            for entry in entries {
                assert_eq!(entry["label"], "模拟同名工作号");
                assert!(seen.insert(serde_json::from_value::<Uuid>(entry[id_key].clone())?));
                match entry["source_type"].as_str().unwrap() {
                    "wecom_internal" => wecom_entry = Some(entry.clone()),
                    "qiwe_sender" => qiwe_entry = Some(entry.clone()),
                    source_type => panic!("unexpected channel: {source_type}"),
                }
            }
            if pages == 1 {
                first_cursors.push(page["next"].as_str().unwrap().to_owned());
            }
            after = page["next"].as_str().map(str::to_owned);
            assert_eq!(page["has_more"], after.is_some());
            if after.is_none() {
                break;
            }
        }
        assert_eq!(pages, 3);
        assert_eq!(seen.len(), 257);
        let qiwe_entry = qiwe_entry.unwrap();
        let wecom_entry = wecom_entry.unwrap();
        assert_eq!(qiwe_entry["gateway"], json!(qiwe_gateway));
        assert_eq!(wecom_entry["gateway"], json!(wecom_gateway));
        assert_ne!(qiwe_entry["source_link"], wecom_entry["source_link"]);
        if kind == "observed_accounts" {
            assert_eq!(qiwe_entry["source_link"], json!(qiwe_observed));
        } else {
            assert_ne!(qiwe_entry["id"], wecom_entry["id"]);
            assert_eq!(qiwe_entry["current"], true);
        }
    }
    assert!(f
        .store
        .scope_communication_candidates(
            &f.actor,
            scope,
            "accounts",
            "模拟同名工作号",
            100,
            Some(&first_cursors[0]),
        )
        .await
        .is_err());

    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked' WHERE id=$1")
        .bind(manager_grant)
        .execute(&f.store.pool)
        .await?;
    for (kind, cursor) in [
        ("observed_accounts", &first_cursors[0]),
        ("accounts", &first_cursors[1]),
    ] {
        let denied = f
            .store
            .scope_communication_candidates(
                &f.actor,
                scope,
                kind,
                "模拟同名工作号",
                100,
                Some(cursor),
            )
            .await
            .unwrap_err();
        assert!(denied.to_string().contains("management_denied"), "{denied}");
    }
    Ok(())
}
