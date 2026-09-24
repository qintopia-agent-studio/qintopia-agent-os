use super::*;
use crate::resident_welcome::{
    state::{Snapshot, StayState},
    store::Store as Welcome,
};

fn id(v: &Value) -> Uuid {
    serde_json::from_value(v.clone()).unwrap()
}
fn find(state: &Value, collection: &str, label: &str) -> Uuid {
    id(&state[collection]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["label"] == label)
        .unwrap()["id"])
}
async fn change(store: &Store, actor: &Actor, value: Change) -> Result<Value> {
    store
        .command(
            actor,
            &Command {
                operation_id: Uuid::new_v4(),
                expected_version: store.state(actor).await?["version"].as_i64().unwrap(),
                change: value,
            },
            true,
        )
        .await
}
async fn person_actor(store: &Store, person: Uuid) -> Result<Actor> {
    let link:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed'").bind(&store.tenant).bind(person).fetch_one(&store.pool).await?;
    store.actor(link).await
}
fn assignment(state: &Value, person: Uuid, scope: Uuid) -> Assignment {
    Assignment {
        collaboration: None,
        person,
        role: find(state, "roles", "舍长"),
        duty: Some(find(state, "duties", "居民服务")),
        scope,
        agent: "erhua".into(),
        domain: "community_service".into(),
        responsibility: "动态联系对象合成验证".into(),
        valid_from: None,
        valid_until: None,
        proxy_for: None,
        actions: vec![],
        permissions: vec![PermissionSetting {
            action: "publish".into(),
            mode: PermissionMode::Autonomous,
            reviewer: None,
        }],
        delegation: None,
    }
}
fn audience(residents: &str) -> Audience {
    Audience {
        open_reception: false,
        groups: vec![],
        people: vec![],
        residents: residents.into(),
        reply: PermissionMode::Autonomous,
        proactive: PermissionMode::Autonomous,
        reviewer: None,
        topics: "本栋生活服务".into(),
        visibility: "general".into(),
    }
}
async fn fixture() -> Result<(Store, Actor, Value, Welcome, Value)> {
    let db = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let store = Store::local(
        &db,
        &format!("synthetic-collaboration-audience-{}", Uuid::new_v4()),
    )
    .await?;
    crate::db::run_migrations(&store.pool).await?;
    let owner = store.actor(store.bootstrap_fixture().await?).await?;
    store.bootstrap_identity_memory_fixture().await?;
    let state = store.state(&owner).await?;
    let welcome = Welcome::local(&db).await?;
    let seed = welcome.bootstrap_foundation_fixture(&store.tenant).await?;
    Ok((store, owner, state, welcome, seed))
}
async fn relation(store: &Store, owner: &Actor, state: &Value, scope: Uuid) -> Result<Uuid> {
    let person = find(state, "people", "人员甲 · 合成样例 A");
    let r = id(&change(
        store,
        owner,
        Change::Assign(Box::new(assignment(state, person, scope))),
    )
    .await?["change"]["collaboration"]);
    change(
        store,
        owner,
        Change::SetAudience {
            collaboration: r,
            audience: audience("all"),
        },
    )
    .await?;
    Ok(r)
}
async fn snapshot(store: &Store, seed: &Value, index: usize) -> Result<Snapshot> {
    let value:Value=sqlx::query_scalar("SELECT projection FROM qintopia_agent_os.welcome_source_versions WHERE source_instance=$1 AND aggregate_id=$2 AND aggregate_type='order'").bind(seed["source"].as_str().unwrap()).bind(format!("synthetic-order-{index}")).fetch_one(&store.pool).await?;
    Ok(serde_json::from_value(value)?)
}
fn member(preview: &Value, person: Uuid) -> &Value {
    preview["people"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["person_ref"] == json!(person))
        .unwrap()
}

#[tokio::test]
#[ignore = "explicit isolated database required"]
async fn ontology_audience_current_past_move_deduplicate_and_readonly() -> Result<()> {
    let (store, owner, state, welcome, seed) = fixture().await?;
    let one = find(&state, "scopes", "一栋");
    let two = find(&state, "scopes", "二栋");
    let resident = id(&seed["targets"][0]["person_ref"]);
    let first = relation(&store, &owner, &state, one).await?;
    let second = relation(&store, &owner, &state, two).await?;
    let community = relation(&store, &owner, &state, find(&state, "scopes", "秦托邦")).await?;
    let initial = store.audience_preview(&owner, community).await?;
    assert_eq!(
        initial["counts"]["current"], 1,
        "two active stays represent one Person"
    );
    assert_eq!(member(&initial, resident)["status"], "current");
    assert_eq!(
        store
            .contact_decision(&owner, first, "person", resident, true)
            .await?["status"],
        "autonomous"
    );
    let before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .fetch_one(&store.pool)
    .await?;
    store.audience_preview(&owner, first).await?;
    assert_eq!(
        sqlx::query_scalar::<_, i64>(
            "SELECT count(*) FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1"
        )
        .bind(&store.tenant)
        .fetch_one(&store.pool)
        .await?,
        before
    );
    let mut moved = snapshot(&store, &seed, 0).await?;
    welcome.apply_snapshot(None, &moved).await?;
    moved.revision = "2".into();
    moved.building = "二栋".into();
    moved.observed_at = Utc::now();
    welcome.apply_snapshot(None, &moved).await?;
    welcome.apply_snapshot(None, &moved).await?;
    let histories:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_identity.person_stay_building_history WHERE source_instance=$1").bind(seed["source"].as_str().unwrap()).fetch_one(&store.pool).await?;
    assert_eq!(
        histories, 3,
        "same-stay building observations survive a move and duplicate delivery"
    );
    assert_eq!(
        member(&store.audience_preview(&owner, first).await?, resident)["status"],
        "past"
    );
    assert_eq!(
        member(&store.audience_preview(&owner, second).await?, resident)["status"],
        "current"
    );
    change(
        &store,
        &owner,
        Change::SetAudience {
            collaboration: first,
            audience: audience("current"),
        },
    )
    .await?;
    assert_eq!(
        member(&store.audience_preview(&owner, first).await?, resident)["selected"],
        false
    );
    assert_eq!(
        store
            .contact_decision(&owner, first, "person", resident, false)
            .await?["status"],
        "denied"
    );
    change(
        &store,
        &owner,
        Change::SetAudience {
            collaboration: first,
            audience: audience("past"),
        },
    )
    .await?;
    assert_eq!(
        store
            .contact_decision(&owner, first, "person", resident, false)
            .await?["status"],
        "autonomous"
    );
    let mut ended = snapshot(&store, &seed, 1).await?;
    ended.revision = "2".into();
    ended.state = StayState::Terminated;
    ended.current_arrangement = false;
    ended.observed_at = Utc::now();
    welcome.apply_snapshot(None, &ended).await?;
    assert_eq!(
        member(&store.audience_preview(&owner, second).await?, resident)["status"],
        "current",
        "another active stay takes priority over an ended one"
    );
    let mut explicit = audience("all");
    explicit.people = vec![resident];
    change(
        &store,
        &owner,
        Change::SetAudience {
            collaboration: community,
            audience: explicit,
        },
    )
    .await?;
    assert_eq!(
        store.audience_preview(&owner, community).await?["people"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database required"]
async fn ontology_audience_uncertain_identity_and_invalid_projections_fail_closed() -> Result<()> {
    let (store, owner, state, _, seed) = fixture().await?;
    let first = relation(&store, &owner, &state, find(&state, "scopes", "一栋")).await?;
    let resident = id(&seed["targets"][0]["person_ref"]);
    let source = seed["source"].as_str().unwrap();
    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET projection=jsonb_set(projection,'{observed_at}',to_jsonb(clock_timestamp()-interval '2 minutes')) WHERE source_instance=$1").bind(source).execute(&store.pool).await?;
    let stale = store.audience_preview(&owner, first).await?;
    assert_eq!(member(&stale, resident)["status"], "unknown");
    assert_eq!(member(&stale, resident)["selected"], false);
    assert_eq!(stale["completeness"], "partial");
    assert_eq!(
        store
            .contact_decision(&owner, first, "person", resident, false)
            .await?["reason"],
        "pms_membership_resolution_required"
    );
    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET projection=jsonb_set(projection,'{observed_at}',to_jsonb(clock_timestamp())),invalidated=true WHERE source_instance=$1").bind(source).execute(&store.pool).await?;
    let invalid = store.audience_preview(&owner, first).await?;
    assert_eq!(
        member(&invalid, resident)["reasons"][0],
        "pms_projection_invalidated"
    );
    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET invalidated=false,conflicted=true WHERE source_instance=$1").bind(source).execute(&store.pool).await?;
    assert_eq!(
        member(&store.audience_preview(&owner, first).await?, resident)["reasons"][0],
        "pms_projection_conflicted"
    );
    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET conflicted=false,projection=jsonb_set(projection,'{building}','\"\"') WHERE source_instance=$1").bind(source).execute(&store.pool).await?;
    let no_arrangement = store.audience_preview(&owner, first).await?;
    assert_eq!(member(&no_arrangement, resident)["status"], "unknown");
    assert_eq!(member(&no_arrangement, resident)["selected"], false);
    assert_eq!(
        member(&no_arrangement, resident)["reasons"][0],
        "pms_arrangement_unconfirmed"
    );
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='pending' WHERE namespace=$1 AND source_ref='synthetic-occupant-0'").bind(format!("pms/{source}/fixture-property/occupant")).execute(&store.pool).await?;
    let unknown = store.audience_preview(&owner, first).await?;
    assert!(unknown["people"].as_array().unwrap().is_empty());
    assert_eq!(unknown["unresolved"][0]["reason"], "identity_unconfirmed");
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='confirmed',adapter_metadata='{\"account_kind\":\"shared\"}' WHERE namespace=$1 AND source_ref='synthetic-occupant-0'").bind(format!("pms/{source}/fixture-property/occupant")).execute(&store.pool).await?;
    let shared = store.audience_preview(&owner, first).await?;
    assert!(shared["people"].as_array().unwrap().is_empty());
    assert_eq!(
        shared["unresolved"][0]["reason"],
        "shared_account_person_unknown"
    );
    let foreign:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.persons(display_name) VALUES('不属于本租户的合成人') RETURNING id").fetch_one(&store.pool).await?;
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET adapter_metadata='{}',person_id=$2 WHERE namespace=$1 AND source_ref='synthetic-occupant-0'").bind(format!("pms/{source}/fixture-property/occupant")).bind(foreign).execute(&store.pool).await?;
    let foreign_result = store.audience_preview(&owner, first).await?;
    assert!(foreign_result["people"].as_array().unwrap().is_empty());
    assert_eq!(
        foreign_result["unresolved"][0]["reason"],
        "person_outside_tenant"
    );
    assert!(!foreign_result.to_string().contains(&foreign.to_string()));
    assert!(!foreign_result.to_string().contains("synthetic-occupant"));
    let histories:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_identity.person_stay_building_history WHERE source_instance=$1").bind(source).fetch_one(&store.pool).await?;
    assert_eq!(
        histories, 2,
        "invalid or conflicting observations never add residence history"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database required"]
async fn ontology_audience_scope_gateway_and_configuration_authority_are_rechecked() -> Result<()> {
    let (store, owner, state, _, seed) = fixture().await?;
    let one = find(&state, "scopes", "一栋");
    let two = find(&state, "scopes", "二栋");
    let first = relation(&store, &owner, &state, one).await?;
    let assigned = find(&state, "people", "人员甲 · 合成样例 A");
    let steward = person_actor(&store, assigned).await?;
    assert_eq!(
        store.audience_preview(&steward, first).await?["counts"]["current"],
        1
    );
    let stranger = person_actor(&store, find(&state, "people", "人员甲 · 合成样例 B")).await?;
    assert!(store.audience_preview(&stranger, first).await.is_err());
    let gateway = store
        .gateway_actor("synthetic-wecom-two", "synthetic-resident")
        .await?;
    assert!(store
        .audience_preview(&gateway, first)
        .await
        .unwrap_err()
        .to_string()
        .contains("gateway_scope_mismatch"));
    assert!(store
        .contact_decision(
            &gateway,
            first,
            "person",
            id(&seed["targets"][0]["person_ref"]),
            false
        )
        .await
        .is_err());
    let mut management = assignment(&state, find(&state, "people", "人员甲 · 合成样例 B"), one);
    management.role = find(&state, "roles", "社区负责人");
    management.duty = Some(find(&state, "duties", "组织管理"));
    management.agent = "default".into();
    management.domain = "organization".into();
    management.permissions = vec![PermissionSetting {
        action: "manage".into(),
        mode: PermissionMode::Autonomous,
        reviewer: None,
    }];
    management.delegation = Some(Delegation {
        agents: vec!["erhua".into()],
        domains: vec!["community_service".into()],
        actions: vec!["publish".into()],
        depth: 0,
    });
    let manager_relation = id(
        &change(&store, &owner, Change::Assign(Box::new(management))).await?["change"]
            ["collaboration"],
    );
    change(
        &store,
        &stranger,
        Change::SetAudience {
            collaboration: first,
            audience: audience("all"),
        },
    )
    .await?;
    assert_eq!(
        store.audience_preview(&stranger, first).await?["counts"]["current"],
        1
    );
    let other = relation(&store, &owner, &state, two).await?;
    assert!(store.audience_preview(&stranger, other).await.is_err());
    change(
        &store,
        &owner,
        Change::EndCollaboration {
            collaboration: manager_relation,
        },
    )
    .await?;
    assert!(store
        .audience_preview(&owner, first)
        .await
        .unwrap_err()
        .to_string()
        .contains("contact_authority_revoked"));
    assert_eq!(
        store
            .contact_decision(
                &owner,
                first,
                "person",
                id(&seed["targets"][0]["person_ref"]),
                false
            )
            .await?["reason"],
        "contact_authority_revoked"
    );
    change(
        &store,
        &owner,
        Change::SetAudience {
            collaboration: first,
            audience: audience("all"),
        },
    )
    .await?;
    change(
        &store,
        &owner,
        Change::EndCollaboration {
            collaboration: first,
        },
    )
    .await?;
    assert!(store.audience_preview(&steward, first).await.is_err());
    Ok(())
}
