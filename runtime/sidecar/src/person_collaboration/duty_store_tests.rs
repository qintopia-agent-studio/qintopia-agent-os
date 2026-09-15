//! Destructive checks are confined to a random namespace in the explicitly enabled local test DB.
#![cfg(feature = "postgres-integration-tests")]

use super::{model::*, store::Actor, Store};
use anyhow::Result;
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

async fn fixture() -> Result<(Store, Actor, Value)> {
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let store = Store::local(
        &database,
        &format!("synthetic-collaboration-duty-{}", Uuid::new_v4()),
    )
    .await?;
    crate::db::run_migrations(&store.pool).await?;
    let operator = store.bootstrap_fixture().await?;
    let owner = store.actor(operator).await?;
    let state = store.state(&owner).await?;
    Ok((store, owner, state))
}

fn id(value: &Value) -> Uuid {
    serde_json::from_value(value.clone()).unwrap()
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn group_retirement_preserves_work_but_agent_retirement_revokes_it() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let assignment = assignment(&state, "人员甲 · 合成样例 A");
    let group = find(&state, "groups", "一栋居民群（合成）");
    let work = save(
        &store,
        &owner,
        Change::ConfigureWork {
            assignment: Box::new(assignment.clone()),
            audience: Audience {
                open_reception: false,
                groups: vec![group],
                people: vec![],
                residents: "none".into(),
                reply: PermissionMode::Autonomous,
                proactive: PermissionMode::Denied,
                reviewer: None,
                topics: "合成居民服务".into(),
                visibility: "general".into(),
            },
        },
    )
    .await?;
    let relation = id(&work["change"]["collaboration"]);
    assert_eq!(
        store
            .contact_decision(&owner, relation, "group", group, false)
            .await?["status"],
        "autonomous"
    );
    for (kind, reference, expected_ended) in [
        ("group", group.to_string(), 0),
        ("agent", assignment.agent.clone(), 1),
    ] {
        let ledger = save(
            &store,
            &owner,
            Change::SaveLedger {
                id: None,
                object: kind.into(),
                reference: Some(reference),
                label: format!("合成{kind}"),
                nickname: String::new(),
                description: "生命周期回归案例".into(),
                scope: Some(assignment.scope),
                owner: None,
                draft: false,
            },
        )
        .await?;
        let ledger_id = id(&ledger["change"]["id"]);
        let retire = command(
            &store,
            &owner,
            Change::Lifecycle {
                object: "ledger".into(),
                id: ledger_id,
                operation: "retire".into(),
            },
        )
        .await?;
        let before = store.state(&owner).await?;
        let preview = store.command(&owner, &retire, false).await?;
        assert_eq!(preview["change"]["ended_connections"], expected_ended);
        assert_eq!(
            store.state(&owner).await?,
            before,
            "preview must not mutate state"
        );
        store.command(&owner, &retire, true).await?;
        let expected_train = if kind == "group" {
            "autonomous"
        } else {
            "denied"
        };
        assert_eq!(
            store.decision(&owner, relation, "train").await?["status"],
            expected_train
        );
        assert_eq!(
            store
                .contact_decision(&owner, relation, "group", group, false)
                .await?["status"],
            "denied"
        );
        save(
            &store,
            &owner,
            Change::Lifecycle {
                object: "ledger".into(),
                id: ledger_id,
                operation: "restore".into(),
            },
        )
        .await?;
        assert_eq!(
            store.decision(&owner, relation, "train").await?["status"],
            expected_train
        );
        assert_eq!(
            store
                .contact_decision(&owner, relation, "group", group, false)
                .await?["status"],
            "denied",
            "restoring a ledger must not restore contact authority"
        );
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn confirmed_workbench_saves_assignment_and_contact_atomically() -> Result<()> {
    let (store, owner, initial) = fixture().await?;
    let mut first = assignment(&initial, "人员甲 · 合成样例 A");
    let mut other = first.clone();
    other.scope = find(&initial, "scopes", "二栋");
    let two = save(&store, &owner, Change::Assign(Box::new(other))).await?;
    let two_id = id(&two["change"]["collaboration"]);
    let audience = Audience {
        open_reception: true,
        groups: vec![],
        people: vec![],
        residents: "current".into(),
        reply: PermissionMode::Autonomous,
        proactive: PermissionMode::Denied,
        reviewer: None,
        topics: "仅本栋合成服务咨询".into(),
        visibility: "general".into(),
    };
    let cmd = command(
        &store,
        &owner,
        Change::ConfigureWork {
            assignment: Box::new(first.clone()),
            audience: audience.clone(),
        },
    )
    .await?;
    let before = store.state(&owner).await?;
    let preview = store.command(&owner, &cmd, false).await?;
    assert_eq!(preview["persisted"], false);
    assert_eq!(
        store.state(&owner).await?,
        before,
        "preview must roll back contact and assignment"
    );
    let result = store.command(&owner, &cmd, true).await?;
    assert_eq!(result["persisted"], true);
    let one_id = id(&result["change"]["collaboration"]);
    let replay = store.command(&owner, &cmd, true).await?;
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["version"], result["version"]);
    assert_eq!(
        result["change"]["after"]["audience"]["topics"],
        audience.topics
    );
    assert_eq!(
        store.decision(&owner, one_id, "train").await?["status"],
        "autonomous"
    );
    first.collaboration = Some(one_id);
    first.person = find(&initial, "people", "人员乙 · 合成样例");
    let mut invalid = audience.clone();
    invalid.groups = vec![Uuid::new_v4()];
    let before_failure = store.state(&owner).await?;
    assert!(save(
        &store,
        &owner,
        Change::ConfigureWork {
            assignment: Box::new(first.clone()),
            audience: invalid,
        }
    )
    .await
    .is_err());
    assert_eq!(
        store.state(&owner).await?,
        before_failure,
        "invalid contact must roll back replacement and revocation"
    );
    let replacement = save(
        &store,
        &owner,
        Change::ConfigureWork {
            assignment: Box::new(first),
            audience,
        },
    )
    .await?;
    assert_eq!(replacement["change"]["replaced"], json!(one_id));
    assert_eq!(
        store.decision(&owner, one_id, "train").await?["status"],
        "denied"
    );
    assert_eq!(
        store.decision(&owner, two_id, "train").await?["status"],
        "autonomous"
    );
    let new_id = id(&replacement["change"]["collaboration"]);
    assert_eq!(
        store.decision(&owner, new_id, "train").await?["status"],
        "autonomous"
    );
    let reopened = Store::local(
        &crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?,
        &store.tenant,
    )
    .await?;
    let reread = reopened.state(&owner).await?;
    assert!(reread["organization"]["audiences"]
        .as_array()
        .unwrap()
        .iter()
        .any(|a| a["collaboration"] == json!(new_id)));
    assert!(
        store
            .command(
                &owner,
                &Command {
                    operation_id: Uuid::new_v4(),
                    ..cmd
                },
                true
            )
            .await
            .is_err(),
        "stale version must not overwrite"
    );
    Ok(())
}

fn find(state: &Value, collection: &str, label: &str) -> Uuid {
    id(&state[collection]
        .as_array()
        .unwrap()
        .iter()
        .find(|item| item["label"] == label)
        .unwrap()["id"])
}

fn permission(action: &str, mode: PermissionMode, reviewer: Option<Uuid>) -> PermissionSetting {
    PermissionSetting {
        action: action.into(),
        mode,
        reviewer,
    }
}

fn assignment(state: &Value, person: &str) -> Assignment {
    Assignment {
        collaboration: None,
        person: find(state, "people", person),
        role: find(state, "roles", "舍长"),
        duty: Some(find(state, "duties", "居民服务")),
        scope: find(state, "scopes", "一栋"),
        agent: "erhua".into(),
        domain: "community_service".into(),
        responsibility: "合成测试的居民服务职责".into(),
        valid_until: None,
        proxy_for: None,
        actions: vec![],
        permissions: vec![permission("train", PermissionMode::Autonomous, None)],
        delegation: None,
    }
}

async fn command(store: &Store, owner: &Actor, change: Change) -> Result<Command> {
    Ok(Command {
        operation_id: Uuid::new_v4(),
        expected_version: store.state(owner).await?["version"].as_i64().unwrap(),
        change,
    })
}

async fn save(store: &Store, owner: &Actor, change: Change) -> Result<Value> {
    store
        .command(owner, &command(store, owner, change).await?, true)
        .await
}

async fn rejected_without_change(store: &Store, owner: &Actor, change: Change) -> Result<()> {
    let before = store.state(owner).await?;
    assert!(save(store, owner, change).await.is_err());
    let after = store.state(owner).await?;
    for field in [
        "version",
        "relations",
        "grants",
        "duties",
        "roles",
        "scopes",
    ] {
        assert_eq!(
            before[field], after[field],
            "rejected command mutated {field}"
        );
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn organization_lifecycle_disambiguates_people_and_never_resurrects_grants() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let first = assignment(&state, "人员甲 · 合成样例 A");
    let mut second = first.clone();
    second.scope = find(&state, "scopes", "二栋");
    let one = save(&store, &owner, Change::Assign(Box::new(first.clone()))).await?;
    let two = save(&store, &owner, Change::Assign(Box::new(second))).await?;
    let one_id = id(&one["change"]["collaboration"]);
    let two_id = id(&two["change"]["collaboration"]);
    let position = save(
        &store,
        &owner,
        Change::SavePosition {
            id: None,
            role: first.role,
            scope: first.scope,
            parent: None,
            label: "一栋舍长岗位".into(),
            description: "一栋范围自治".into(),
            draft: false,
        },
    )
    .await?;
    let position_id = id(&position["change"]["id"]);
    let retire = command(
        &store,
        &owner,
        Change::Lifecycle {
            object: "position".into(),
            id: position_id,
            operation: "retire".into(),
        },
    )
    .await?;
    let preview = store.command(&owner, &retire, false).await?;
    assert_eq!(preview["change"]["ended_connections"], 1);
    assert_eq!(
        store.decision(&owner, one_id, "train").await?["status"],
        "autonomous"
    );
    store.command(&owner, &retire, true).await?;
    assert_eq!(
        store.decision(&owner, one_id, "train").await?["status"],
        "denied"
    );
    assert_eq!(
        store.decision(&owner, two_id, "train").await?["status"],
        "autonomous"
    );
    save(
        &store,
        &owner,
        Change::Lifecycle {
            object: "position".into(),
            id: position_id,
            operation: "restore".into(),
        },
    )
    .await?;
    assert_eq!(
        store.decision(&owner, one_id, "train").await?["status"],
        "denied"
    );
    for name in ["同名待确认", "同名待确认"] {
        let result = save(
            &store,
            &owner,
            Change::SaveLedger {
                id: None,
                object: "person".into(),
                reference: None,
                label: name.into(),
                nickname: "".into(),
                description: "本地合成待核验记录".into(),
                scope: Some(first.scope),
                owner: None,
                draft: false,
            },
        )
        .await?;
        assert_eq!(result["change"]["verified"], false);
    }
    let state = store.state(&owner).await?;
    let people: Vec<_> = state["organization"]["ledger"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|e| e["label"] == "同名待确认")
        .collect();
    assert_eq!(people.len(), 2);
    assert_ne!(people[0]["object_ref"], people[1]["object_ref"]);
    let pending_id: Uuid = serde_json::from_value(people[0]["object_ref"].clone())?;
    let ledger_id = id(&people[0]["id"]);
    let mut pending = first.clone();
    pending.person = pending_id;
    rejected_without_change(&store, &owner, Change::Assign(Box::new(pending))).await?;
    save(
        &store,
        &owner,
        Change::Lifecycle {
            object: "ledger".into(),
            id: ledger_id,
            operation: "delete".into(),
        },
    )
    .await?;
    // Established records are kept even when never appointed.
    let known = save(
        &store,
        &owner,
        Change::SaveLedger {
            id: None,
            object: "person".into(),
            reference: Some(first.person.to_string()),
            label: "同名已核验".into(),
            nickname: "A".into(),
            description: "合成负责人 A".into(),
            scope: None,
            owner: None,
            draft: false,
        },
    )
    .await?;
    rejected_without_change(
        &store,
        &owner,
        Change::Lifecycle {
            object: "ledger".into(),
            id: id(&known["change"]["id"]),
            operation: "delete".into(),
        },
    )
    .await?;
    save(
        &store,
        &owner,
        Change::Lifecycle {
            object: "ledger".into(),
            id: id(&known["change"]["id"]),
            operation: "retire".into(),
        },
    )
    .await?;
    assert_eq!(
        store.decision(&owner, two_id, "train").await?["status"],
        "denied"
    );
    save(
        &store,
        &owner,
        Change::Lifecycle {
            object: "ledger".into(),
            id: id(&known["change"]["id"]),
            operation: "restore".into(),
        },
    )
    .await?;
    assert_eq!(
        store.decision(&owner, two_id, "train").await?["status"],
        "denied"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn contact_scope_and_hierarchy_are_explicit_and_revocable() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let a = assignment(&state, "人员甲 · 合成样例 A");
    let relation = save(&store, &owner, Change::Assign(Box::new(a.clone()))).await?;
    let relation = id(&relation["change"]["collaboration"]);
    let target = find(&state, "people", "人员乙 · 合成样例");
    let other = find(&state, "people", "人员丙 · 合成样例");
    let audience = Audience {
        open_reception: true,
        groups: vec![],
        people: vec![target],
        residents: "all".into(),
        reply: PermissionMode::Autonomous,
        proactive: PermissionMode::Autonomous,
        reviewer: None,
        topics: "本栋服务交流".into(),
        visibility: "general".into(),
    };
    save(
        &store,
        &owner,
        Change::SetAudience {
            collaboration: relation,
            audience: audience.clone(),
        },
    )
    .await?;
    let public = store
        .contact_decision(&owner, relation, "public", Uuid::new_v4(), false)
        .await?;
    assert_eq!(public["status"], "autonomous");
    assert_eq!(public["visibility"], "public_only");
    assert_eq!(
        store
            .contact_decision(&owner, relation, "public", Uuid::new_v4(), true)
            .await?["status"],
        "denied"
    );
    assert_eq!(
        store
            .contact_decision(&owner, relation, "person", target, false)
            .await?["status"],
        "autonomous"
    );
    assert_eq!(
        store
            .contact_decision(&owner, relation, "person", target, true)
            .await?["status"],
        "denied"
    );
    assert_eq!(
        store
            .contact_decision(&owner, relation, "person", other, false)
            .await?["reason"],
        "pms_membership_resolution_required"
    );
    let mut manager = a.clone();
    manager.person = target;
    manager.role = find(&state, "roles", "社区负责人");
    manager.duty = Some(find(&state, "duties", "组织管理"));
    manager.agent = "default".into();
    manager.domain = "organization".into();
    manager.permissions = vec![permission("manage", PermissionMode::Autonomous, None)];
    manager.delegation = Some(Delegation {
        agents: vec!["erhua".into()],
        domains: vec!["community_service".into()],
        actions: vec!["publish".into()],
        depth: 0,
    });
    let mg = save(&store, &owner, Change::Assign(Box::new(manager))).await?;
    let manager_actor = person_actor(&store, target).await?;
    save(
        &store,
        &manager_actor,
        Change::SetAudience {
            collaboration: relation,
            audience: audience.clone(),
        },
    )
    .await?;
    save(
        &store,
        &owner,
        Change::RevokeGrant {
            grant: id(&mg["change"]["grants"][0]),
        },
    )
    .await?;
    assert_eq!(
        store
            .contact_decision(&owner, relation, "person", target, false)
            .await?["reason"],
        "contact_authority_revoked"
    );
    let mut bad = audience;
    bad.groups = vec![Uuid::new_v4()];
    rejected_without_change(
        &store,
        &owner,
        Change::SetAudience {
            collaboration: relation,
            audience: bad,
        },
    )
    .await?;
    let parent = save(
        &store,
        &owner,
        Change::SavePosition {
            id: None,
            role: find(&state, "roles", "社区负责人"),
            scope: find(&state, "scopes", "秦托邦"),
            parent: None,
            label: "社区服务负责人岗位".into(),
            description: "明确委托才有管理权限".into(),
            draft: false,
        },
    )
    .await?;
    let parent = id(&parent["change"]["id"]);
    let child = save(
        &store,
        &owner,
        Change::SavePosition {
            id: None,
            role: a.role,
            scope: a.scope,
            parent: Some(parent),
            label: "一栋岗位".into(),
            description: "本栋自治".into(),
            draft: false,
        },
    )
    .await?;
    let child = id(&child["change"]["id"]);
    rejected_without_change(
        &store,
        &owner,
        Change::Lifecycle {
            object: "position".into(),
            id: parent,
            operation: "retire".into(),
        },
    )
    .await?;
    rejected_without_change(
        &store,
        &owner,
        Change::SavePosition {
            id: Some(parent),
            role: find(&state, "roles", "社区负责人"),
            scope: find(&state, "scopes", "秦托邦"),
            parent: Some(child),
            label: "循环上级".into(),
            description: "必须拒绝".into(),
            draft: false,
        },
    )
    .await?;
    save(
        &store,
        &owner,
        Change::EndCollaboration {
            collaboration: relation,
        },
    )
    .await?;
    assert_eq!(
        store
            .contact_decision(&owner, relation, "person", target, false)
            .await?["status"],
        "denied"
    );
    Ok(())
}

async fn person_actor(store: &Store, person: Uuid) -> Result<Actor> {
    let link = sqlx::query_scalar(
        "SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2",
    )
    .bind(&store.tenant)
    .bind(person)
    .fetch_one(&store.pool)
    .await?;
    store.actor(link).await
}

fn grants_by_id(state: &Value) -> std::collections::BTreeMap<Uuid, Value> {
    state["grants"]
        .as_array()
        .unwrap()
        .iter()
        .map(|grant| (id(&grant["id"]), grant.clone()))
        .collect()
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn permission_modes_persist_and_reviewer_loss_never_becomes_autonomy() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let mut reviewer = assignment(&state, "人员乙 · 合成样例");
    reviewer.valid_until = Some(Utc::now() + Duration::hours(1));
    let reviewer_saved = save(&store, &owner, Change::Assign(Box::new(reviewer.clone()))).await?;
    let mut learner = assignment(&state, "人员甲 · 合成样例 A");
    learner.permissions = vec![
        permission("train", PermissionMode::Confirmation, Some(reviewer.person)),
        permission("review", PermissionMode::Autonomous, None),
        permission("publish", PermissionMode::Denied, None),
    ];
    let learner_saved = save(&store, &owner, Change::Assign(Box::new(learner.clone()))).await?;
    let relation = id(&learner_saved["change"]["collaboration"]);
    let learner_actor = person_actor(&store, learner.person).await?;
    for (action, expected) in [
        ("train", "confirmation_required"),
        ("review", "autonomous"),
        ("publish", "denied"),
    ] {
        let decision = store.decision(&learner_actor, relation, action).await?;
        assert_eq!(decision["status"], expected);
        assert_eq!(decision["runtime_connected"], false);
        assert_eq!(decision["configuration_version"], learner_saved["version"]);
    }
    assert!(
        !store
            .allowed(
                &learner_actor,
                learner.scope,
                "erhua",
                "community_service",
                "train"
            )
            .await?
    );
    let persisted = store.state(&owner).await?;
    let modes: Vec<_> = persisted["grants"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|grant| grant["collaboration"] == json!(relation))
        .map(|grant| {
            (
                grant["action"].as_str().unwrap(),
                grant["mode"].as_str().unwrap(),
            )
        })
        .collect();
    assert!(modes.contains(&("train", "confirmation")));
    assert!(modes.contains(&("review", "autonomous")));
    assert!(modes.contains(&("publish", "denied")));

    let outsider = person_actor(&store, find(&state, "people", "人员丙 · 合成样例")).await?;
    assert!(store.decision(&outsider, relation, "review").await.is_err());
    let mut cross_scope = learner.clone();
    cross_scope.scope = find(&state, "scopes", "二栋");
    rejected_without_change(&store, &owner, Change::Assign(Box::new(cross_scope))).await?;
    let mut self_review = learner.clone();
    self_review.permissions[0].reviewer = Some(learner.person);
    rejected_without_change(&store, &owner, Change::Assign(Box::new(self_review))).await?;
    let mut cross_agent = learner;
    cross_agent.agent = "xiaoman".into();
    rejected_without_change(&store, &owner, Change::Assign(Box::new(cross_agent))).await?;

    save(
        &store,
        &owner,
        Change::RevokeGrant {
            grant: id(&reviewer_saved["change"]["grants"][0]),
        },
    )
    .await?;
    assert_eq!(
        store.decision(&owner, relation, "train").await?["status"],
        "denied"
    );
    assert_eq!(
        store.decision(&owner, relation, "review").await?["status"],
        "autonomous"
    );
    reviewer.collaboration = Some(id(&reviewer_saved["change"]["collaboration"]));
    save(&store, &owner, Change::Assign(Box::new(reviewer))).await?;
    assert_eq!(
        store.decision(&owner, relation, "train").await?["status"],
        "confirmation_required"
    );
    // Synthetic time fixture: expiry is read at decision time, without relying on a new UI save.
    sqlx::query("UPDATE qintopia_agent_os.collaboration_appointments SET valid_from=clock_timestamp()-interval '2 hours',valid_until=clock_timestamp()-interval '1 hour' WHERE tenant_key=$1 AND id=$2")
        .bind(&store.tenant).bind(id(&reviewer_saved["change"]["appointment"])).execute(&store.pool).await?;
    assert_eq!(
        store.decision(&owner, relation, "train").await?["status"],
        "denied"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn catalog_edits_do_not_grant_permissions_and_retirement_preserves_history() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let duty_result = save(
        &store,
        &owner,
        Change::SaveDuty {
            id: None,
            label: "合成服务职责".into(),
            description: "用于检查职责方案维护".into(),
            domain: "community_service".into(),
            available_actions: vec!["train".into()],
        },
    )
    .await?;
    let duty = id(&duty_result["change"]["duty"]);
    let role_result = save(
        &store,
        &owner,
        Change::SaveRole {
            id: None,
            label: "合成服务岗位".into(),
            description: "用于检查岗位方案维护".into(),
            duty_ids: vec![duty],
        },
    )
    .await?;
    let role = id(&role_result["change"]["role"]);
    let mut assigned = assignment(&state, "人员甲 · 合成样例 A");
    assigned.duty = Some(duty);
    assigned.role = role;
    let relation_result = save(&store, &owner, Change::Assign(Box::new(assigned))).await?;
    let relation = id(&relation_result["change"]["collaboration"]);
    let before_grants = grants_by_id(&store.state(&owner).await?);
    save(
        &store,
        &owner,
        Change::SaveDuty {
            id: Some(duty),
            label: "合成服务职责（更新）".into(),
            description: "增加可选审核类别".into(),
            domain: "community_service".into(),
            available_actions: vec!["train".into(), "review".into()],
        },
    )
    .await?;
    save(
        &store,
        &owner,
        Change::SaveRole {
            id: Some(role),
            label: "合成服务岗位（更新）".into(),
            description: "增加可选客房协调职责".into(),
            duty_ids: vec![duty, find(&state, "duties", "客房协调")],
        },
    )
    .await?;
    assert_eq!(grants_by_id(&store.state(&owner).await?), before_grants);
    assert_eq!(
        store.decision(&owner, relation, "train").await?["status"],
        "autonomous"
    );
    assert_eq!(
        store.decision(&owner, relation, "review").await?["status"],
        "denied"
    );
    rejected_without_change(
        &store,
        &owner,
        Change::SaveDuty {
            id: Some(duty),
            label: "合成服务职责（更新）".into(),
            description: "移除正在使用的权限类别".into(),
            domain: "community_service".into(),
            available_actions: vec!["review".into()],
        },
    )
    .await?;
    rejected_without_change(
        &store,
        &owner,
        Change::SaveRole {
            id: Some(role),
            label: "合成服务岗位（更新）".into(),
            description: "移除正在使用的职责".into(),
            duty_ids: vec![],
        },
    )
    .await?;
    for (object, object_id) in [("role", role), ("duty", duty)] {
        rejected_without_change(
            &store,
            &owner,
            Change::RetireCatalog {
                object: object.into(),
                id: object_id,
            },
        )
        .await?;
    }
    save(
        &store,
        &owner,
        Change::EndCollaboration {
            collaboration: relation,
        },
    )
    .await?;
    for (object, object_id, collection) in [("role", role, "roles"), ("duty", duty, "duties")] {
        let result = save(
            &store,
            &owner,
            Change::RetireCatalog {
                object: object.into(),
                id: object_id,
            },
        )
        .await?;
        assert_eq!(result["change"]["disposition"], "retired");
        let current = store.state(&owner).await?;
        assert!(current[collection]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == json!(object_id) && item["status"] == "retired"));
    }
    let unused_duty = save(
        &store,
        &owner,
        Change::SaveDuty {
            id: None,
            label: "合成未使用职责".into(),
            description: "尚未关联任何连接".into(),
            domain: "community_service".into(),
            available_actions: vec![],
        },
    )
    .await?;
    let unused_role = save(
        &store,
        &owner,
        Change::SaveRole {
            id: None,
            label: "合成未使用岗位".into(),
            description: "尚未安排人员".into(),
            duty_ids: vec![],
        },
    )
    .await?;
    let unused_scope = save(
        &store,
        &owner,
        Change::CreateScope {
            parent: find(&state, "scopes", "秦托邦"),
            label: "合成未使用范围".into(),
            scope_kind: "business".into(),
        },
    )
    .await?;
    for (object, object_id, collection) in [
        ("duty", id(&unused_duty["change"]["duty"]), "duties"),
        ("role", id(&unused_role["change"]["role"]), "roles"),
        ("scope", id(&unused_scope["change"]["scope"]), "scopes"),
    ] {
        let result = save(
            &store,
            &owner,
            Change::RetireCatalog {
                object: object.into(),
                id: object_id,
            },
        )
        .await?;
        assert_eq!(result["change"]["disposition"], "retired");
        assert!(store.state(&owner).await?[collection]
            .as_array()
            .unwrap()
            .iter()
            .any(|item| item["id"] == json!(object_id)));
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn replacing_and_ending_connections_is_atomic_and_leaves_other_work_intact() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let original = assignment(&state, "人员甲 · 合成样例 A");
    let first = save(&store, &owner, Change::Assign(Box::new(original.clone()))).await?;
    let first_id = id(&first["change"]["collaboration"]);
    let mut other_work = original.clone();
    other_work.agent = "xiaoman".into();
    let second = save(&store, &owner, Change::Assign(Box::new(other_work))).await?;
    let second_id = id(&second["change"]["collaboration"]);
    assert_eq!(
        first["change"]["appointment"],
        second["change"]["appointment"]
    );

    let mut replacement = original.clone();
    replacement.collaboration = Some(first_id);
    replacement.person = find(&state, "people", "人员乙 · 合成样例");
    replacement.scope = find(&state, "scopes", "二栋");
    let mut invalid = replacement.clone();
    invalid.role = Uuid::new_v4();
    rejected_without_change(&store, &owner, Change::Assign(Box::new(invalid))).await?;
    assert_eq!(
        store.decision(&owner, first_id, "train").await?["status"],
        "autonomous"
    );

    let operation = command(&store, &owner, Change::Assign(Box::new(replacement))).await?;
    let before_preview = store.state(&owner).await?;
    let preview = store.command(&owner, &operation, false).await?;
    assert_eq!(preview["persisted"], false);
    assert_eq!(store.state(&owner).await?, before_preview);
    let (saved, replayed) = tokio::join!(
        store.command(&owner, &operation, true),
        store.command(&owner, &operation, true)
    );
    let saved = saved?;
    let replayed = replayed?;
    assert_eq!(saved["change"], replayed["change"]);
    let replacement_id = id(&saved["change"]["collaboration"]);
    assert_ne!(replacement_id, first_id);
    assert_eq!(saved["change"]["replaced"], json!(first_id));
    assert_eq!(
        store.decision(&owner, first_id, "train").await?["status"],
        "denied"
    );
    assert_eq!(
        store.decision(&owner, replacement_id, "train").await?["status"],
        "autonomous"
    );
    assert_eq!(
        store.decision(&owner, second_id, "train").await?["status"],
        "autonomous"
    );
    let after = store.state(&owner).await?;
    assert!(after["relations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == json!(first_id) && r["status"] == "ended"));
    assert!(after["relations"]
        .as_array()
        .unwrap()
        .iter()
        .any(|r| r["id"] == json!(replacement_id) && r["replaces"] == json!(first_id)));
    let writes: i64 = sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1 AND id=$2")
        .bind(&store.tenant).bind(operation.operation_id).fetch_one(&store.pool).await?;
    assert_eq!(writes, 1);

    save(
        &store,
        &owner,
        Change::EndCollaboration {
            collaboration: replacement_id,
        },
    )
    .await?;
    assert_eq!(
        store.decision(&owner, replacement_id, "train").await?["status"],
        "denied"
    );
    assert_eq!(
        store.decision(&owner, second_id, "train").await?["status"],
        "autonomous"
    );
    // A stale edit cannot reopen the old connection or revoke unrelated work.
    let mut stale = original;
    stale.collaboration = Some(first_id);
    rejected_without_change(&store, &owner, Change::Assign(Box::new(stale))).await?;
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn bootstrap_revocation_and_confirmation_management_cannot_bypass_catalog_policy(
) -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let bootstrap_grant = id(&state["grants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|grant| {
            grant["agent"] == "default"
                && grant["domain"] == "organization"
                && grant["action"] == "manage"
        })
        .unwrap()["id"]);
    rejected_without_change(
        &store,
        &owner,
        Change::RevokeGrant {
            grant: bootstrap_grant,
        },
    )
    .await?;

    let mut reviewer = assignment(&state, "人员甲 · 合成样例 A");
    reviewer.role = find(&state, "roles", "社区负责人");
    reviewer.duty = Some(find(&state, "duties", "组织管理"));
    reviewer.scope = find(&state, "scopes", "秦托邦");
    reviewer.agent = "default".into();
    reviewer.domain = "organization".into();
    reviewer.permissions = vec![permission("manage", PermissionMode::Autonomous, None)];
    reviewer.delegation = Some(Delegation {
        agents: vec!["default".into()],
        domains: vec!["organization".into()],
        actions: vec!["manage".into()],
        depth: 0,
    });
    save(&store, &owner, Change::Assign(Box::new(reviewer.clone()))).await?;
    let mut requires_confirmation = reviewer.clone();
    requires_confirmation.person = find(&state, "people", "人员乙 · 合成样例");
    requires_confirmation.permissions = vec![permission(
        "manage",
        PermissionMode::Confirmation,
        Some(reviewer.person),
    )];
    requires_confirmation.delegation = None;
    let configured = save(
        &store,
        &owner,
        Change::Assign(Box::new(requires_confirmation.clone())),
    )
    .await?;
    let restricted_actor = person_actor(&store, requires_confirmation.person).await?;
    assert_eq!(
        store
            .decision(
                &restricted_actor,
                id(&configured["change"]["collaboration"]),
                "manage"
            )
            .await?["status"],
        "confirmation_required"
    );
    assert!(store.state(&restricted_actor).await.is_err());

    for change in [
        Change::CreateRole {
            label: "合成未确认旧入口岗位".into(),
            available_actions: vec!["train".into()],
        },
        Change::SaveRole {
            id: None,
            label: "合成未确认新入口岗位".into(),
            description: "需确认的权限不能自行维护目录".into(),
            duty_ids: vec![reviewer.duty.unwrap()],
        },
    ] {
        let before = store.state(&owner).await?;
        let request = command(&store, &owner, change).await?;
        assert!(store
            .command(&restricted_actor, &request, true)
            .await
            .is_err());
        let after = store.state(&owner).await?;
        for (field, value) in before.as_object().unwrap() {
            assert!(
                after[field] == *value,
                "rejected command changed field {field}"
            );
        }
    }
    Ok(())
}
