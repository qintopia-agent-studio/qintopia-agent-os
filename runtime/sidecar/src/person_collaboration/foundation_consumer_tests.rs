//! Execute actual shared-rule and durable dispatch consumers against isolated fixtures.
#![cfg(feature = "postgres-integration-tests")]
use super::{model::*, Actor, KnowledgeWrite, Store};
use anyhow::Result;
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

fn id(value: &Value) -> Uuid {
    serde_json::from_value(value.clone()).unwrap()
}
fn find(state: &Value, collection: &str, label: &str) -> Uuid {
    id(&state[collection]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["label"] == label)
        .unwrap()["id"])
}
async fn fixture() -> Result<(Store, Actor, Value)> {
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let store = Store::local(
        &database,
        &format!("synthetic-collaboration-consumer-{}", Uuid::new_v4()),
    )
    .await?;
    crate::db::run_migrations(&store.pool).await?;
    let owner = store.actor(store.bootstrap_fixture().await?).await?;
    store.bootstrap_identity_memory_fixture().await?;
    let state = store.state(&owner).await?;
    Ok((store, owner, state))
}
async fn actor(store: &Store, person: Uuid) -> Result<Actor> {
    let link:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed'").bind(&store.tenant).bind(person).fetch_one(&store.pool).await?;
    store.actor(link).await
}
async fn command(store: &Store, owner: &Actor, change: Change) -> Result<Value> {
    store
        .command(
            owner,
            &Command {
                operation_id: Uuid::new_v4(),
                expected_version: store.state(owner).await?["version"].as_i64().unwrap(),
                change,
            },
            true,
        )
        .await
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
        responsibility: "实际消费者合成验证".into(),
        valid_until: None,
        proxy_for: None,
        actions: vec![],
        permissions: vec![PermissionSetting {
            action: "change_rules".into(),
            mode: PermissionMode::Autonomous,
            reviewer: None,
        }],
        delegation: None,
    }
}
async fn assign(store: &Store, owner: &Actor, assignment: Assignment) -> Result<Uuid> {
    Ok(id(&command(
        store,
        owner,
        Change::Assign(Box::new(assignment)),
    )
    .await?["change"]["collaboration"]))
}
fn write(scope: Uuid, key: &str, text: &str) -> KnowledgeWrite {
    KnowledgeWrite {
        operation_id: Uuid::new_v4(),
        expected_version: 0,
        scope,
        key: key.into(),
        kind: "rule".into(),
        shared: false,
        case_ref: None,
        content: json!({"text":text}),
        effective_at: None,
        effective_until: None,
    }
}
fn knowledge<'a>(context: &'a Value, key: &str) -> &'a Value {
    context["knowledge"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == key)
        .unwrap()
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn foundation_consumer_same_names_scopes_rules_refresh_and_versions() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let one = find(&state, "scopes", "一栋");
    let two = find(&state, "scopes", "二栋");
    let pa = find(&state, "people", "人员甲 · 合成样例 A");
    let pb = find(&state, "people", "人员甲 · 合成样例 B");
    assert_ne!(pa, pb);
    assign(&store, &owner, assignment(&state, pa, one)).await?;
    assign(&store, &owner, assignment(&state, pb, two)).await?;
    let a = actor(&store, pa).await?;
    let b = actor(&store, pb).await?;
    let first = write(one, "kitchen", "一栋厨房九点关闭");
    let saved = store.knowledge_save(&a, &first, false).await?;
    assert_eq!(saved["status"], "saved");
    let same = store.knowledge_save(&a, &first, false).await?;
    assert_eq!(
        saved["knowledge"], same["knowledge"],
        "exact operation repeats the persisted receipt"
    );
    assert_eq!(same["status"], "saved");
    assert_eq!(same["replayed"], true);
    assert_eq!(same["current_state_requires_read"], true);
    let mut conflict = first.clone();
    conflict.content = json!({"text":"同一操作不能改写正文"});
    assert!(store
        .knowledge_save(&a, &conflict, false)
        .await
        .unwrap_err()
        .to_string()
        .contains("idempotency_conflict"));
    conflict.operation_id = Uuid::new_v4();
    assert!(store
        .knowledge_save(&a, &conflict, false)
        .await
        .unwrap_err()
        .to_string()
        .contains("knowledge_version_conflict"));
    assert!(store
        .knowledge_save(&a, &write(two, "kitchen", "跨栋"), false)
        .await
        .is_err());
    assert!(store.foundation_context(&a, two, "").await.is_err());
    store
        .knowledge_save(&b, &write(two, "kitchen", "二栋厨房十点关闭"), false)
        .await?;
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let reopened = Store::local(&database, &store.tenant).await?;
    let ca = reopened.foundation_context(&a, one, "厨房").await?;
    let cb = reopened.foundation_context(&b, two, "厨房").await?;
    assert_eq!(ca["identity"]["person_ref"], json!(pa));
    assert_eq!(cb["identity"]["person_ref"], json!(pb));
    assert_eq!(
        knowledge(&ca, "kitchen")["content"]["text"],
        "一栋厨房九点关闭"
    );
    assert_eq!(
        knowledge(&cb, "kitchen")["content"]["text"],
        "二栋厨房十点关闭"
    );
    let mut future = write(one, "kitchen", "明天改为八点关闭");
    future.expected_version = 1;
    future.effective_at = Some(Utc::now() + Duration::days(1));
    assert_eq!(
        store.knowledge_save(&a, &future, false).await?["knowledge"]["version"],
        2
    );
    let before = store.foundation_context(&a, one, "厨房").await?;
    assert_eq!(knowledge(&before, "kitchen")["version"], 1);
    assert_eq!(knowledge(&before, "kitchen")["latest_version"], 2);
    assert_eq!(
        knowledge(&before, "kitchen")["content"]["text"],
        "一栋厨房九点关闭"
    );
    let versions:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_knowledge_revisions r JOIN qintopia_agent_os.collaboration_knowledge_items i ON i.id=r.item_id WHERE i.tenant_key=$1 AND i.scope_id=$2 AND i.knowledge_key='kitchen'").bind(&store.tenant).bind(one).fetch_one(&store.pool).await?;
    assert_eq!(
        versions, 2,
        "failed and replayed requests do not create new versions"
    );
    let internal_space:Uuid=sqlx::query_scalar("SELECT space_id FROM qintopia_agent_os.collaboration_knowledge_items WHERE tenant_key=$1 AND scope_id=$2 AND knowledge_key='kitchen'")
        .bind(&store.tenant).bind(one).fetch_one(&store.pool).await?;
    let state = store.state(&owner).await?;
    assert!(
        !state["groups"]
            .as_array()
            .unwrap()
            .iter()
            .any(|group| group["id"] == json!(internal_space)),
        "inert knowledge storage must not appear as a contact group"
    );
    assert!(
        command(
            &store,
            &owner,
            Change::SetGroups {
                scope: one,
                conversations: vec![internal_space]
            }
        )
        .await
        .is_err(),
        "internal storage cannot become a message target"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn foundation_consumer_shared_community_principle_cannot_be_overridden() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let root = find(&state, "scopes", "秦托邦");
    let one = find(&state, "scopes", "一栋");
    let two = find(&state, "scopes", "二栋");
    let owner_person = store.verified_person(&owner).await?;
    let pa = find(&state, "people", "人员甲 · 合成样例 A");
    let pb = find(&state, "people", "人员甲 · 合成样例 B");
    // Management alone cannot write the rule: acquire a distinct business grant.
    assert!(store
        .knowledge_save(
            &owner,
            &write(root, "community_culture", "管理不等于执行"),
            false
        )
        .await
        .is_err());
    assign(&store, &owner, assignment(&state, owner_person, root)).await?;
    assign(&store, &owner, assignment(&state, pa, one)).await?;
    assign(&store, &owner, assignment(&state, pb, two)).await?;
    let a = actor(&store, pa).await?;
    let b = actor(&store, pb).await?;
    let mut shared = write(root, "community_culture", "尊重所有社区成员");
    shared.kind = "culture".into();
    shared.shared = true;
    store.knowledge_save(&owner, &shared, false).await?;
    let mut principle = write(root, "safety_principle", "公共通道保持畅通");
    principle.kind = "principle".into();
    principle.shared = true;
    let canonical = store.knowledge_save(&owner, &principle, false).await?;
    for (person, scope) in [(&a, one), (&b, two)] {
        let context = store.foundation_context(person, scope, "").await?;
        assert_eq!(
            knowledge(&context, "community_culture")["content"]["text"],
            "尊重所有社区成员"
        );
        assert_eq!(
            knowledge(&context, "safety_principle")["id"],
            canonical["knowledge"]["id"]
        );
        assert!(store
            .knowledge_save(
                person,
                &write(scope, "safety_principle", "本栋试图覆盖原则"),
                false
            )
            .await
            .unwrap_err()
            .to_string()
            .contains("inherited_principle_cannot_be_overridden"));
        assert!(store
            .knowledge_save(
                person,
                &write(root, "community_culture", "越权修改社区"),
                false
            )
            .await
            .is_err());
    }
    let mut leak = write(one, "local_culture", "本栋不能擅自标记社区共享");
    leak.shared = true;
    assert!(store
        .knowledge_save(&a, &leak, false)
        .await
        .unwrap_err()
        .to_string()
        .contains("community_sharing_requires_community_scope"));
    let local:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_knowledge_items WHERE tenant_key=$1 AND scope_id=ANY($2)").bind(&store.tenant).bind(vec![one,two]).fetch_one(&store.pool).await?;
    assert_eq!(local, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn foundation_consumer_deferred_rule_rechecks_revocation_before_any_write() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let one = find(&state, "scopes", "一栋");
    let person = find(&state, "people", "人员甲 · 合成样例 A");
    let relation = assign(&store, &owner, assignment(&state, person, one)).await?;
    let a = actor(&store, person).await?;
    let request = write(one, "deferred_rule", "撤权前排队的规则");
    let queued = store.knowledge_save(&a, &request, true).await?;
    let work = id(&queued["work_item_id"]);
    assert_eq!(queued["status"], "queued");
    let replay = store.knowledge_save(&a, &request, true).await?;
    assert_eq!(replay["work_item_id"], queued["work_item_id"]);
    assert_eq!(replay["replayed"], true);
    command(
        &store,
        &owner,
        Change::EndCollaboration {
            collaboration: relation,
        },
    )
    .await?;
    let result = store.foundation_execute_work(work).await?;
    assert_eq!(result["status"], "failed");
    assert_eq!(result["reason"], "authority_changed_or_revoked");
    let items:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_knowledge_items WHERE tenant_key=$1 AND scope_id=$2 AND knowledge_key='deferred_rule'").bind(&store.tenant).bind(one).fetch_one(&store.pool).await?;
    let versions:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_knowledge_revisions WHERE tenant_key=$1").bind(&store.tenant).fetch_one(&store.pool).await?;
    assert_eq!((items, versions), (0, 0));
    assert_eq!(
        store.foundation_work_status(&owner, work).await?["reason"],
        "authority_changed_or_revoked"
    );
    assert_eq!(store.foundation_execute_work(work).await?["replayed"], true);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn foundation_consumer_management_dispatch_persists_recovers_and_rechecks_authority(
) -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let one = find(&state, "scopes", "一栋");
    let two = find(&state, "scopes", "二栋");
    let person = find(&state, "people", "人员甲 · 合成样例 A");
    let mut management = assignment(&state, person, one);
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
        actions: vec!["train".into()],
        depth: 0,
    });
    let relation = assign(&store, &owner, management).await?;
    let manager = actor(&store, person).await?;
    assert!(store
        .knowledge_save(
            &manager,
            &write(one, "management_not_business", "管理不能当业务权"),
            false
        )
        .await
        .is_err());
    let version = store.state(&owner).await?["version"].as_i64().unwrap();
    let operation = Uuid::new_v4();
    assert!(store
        .dispatch_context(&manager, one, operation, version - 1, "版本冲突")
        .await
        .is_err());
    assert!(store
        .dispatch_context(&manager, two, operation, version, "跨栋不合法")
        .await
        .is_err());
    let queued = store
        .dispatch_context(&manager, one, operation, version, "整理本栋现行规则")
        .await?;
    let work = id(&queued["work_item_id"]);
    assert_eq!(queued["status"], "queued");
    assert_eq!(
        store
            .dispatch_context(&manager, one, operation, version, "整理本栋现行规则")
            .await?["work_item_id"],
        json!(work)
    );
    assert!(store
        .dispatch_context(&manager, one, operation, version, "同一操作替换任务")
        .await
        .unwrap_err()
        .to_string()
        .contains("idempotency_conflict"));
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_local_executors(tenant_key,agent_key,available) VALUES($1,'erhua',false) ON CONFLICT(tenant_key,agent_key) DO UPDATE SET available=false").bind(&store.tenant).execute(&store.pool).await?;
    assert_eq!(
        store.foundation_execute_work(work).await?["reason"],
        "target_unavailable"
    );
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let reopened = Store::local(&database, &store.tenant).await?;
    assert_eq!(
        reopened.foundation_work_status(&manager, work).await?["status"],
        "queued"
    );
    sqlx::query("UPDATE qintopia_agent_os.collaboration_local_executors SET available=true WHERE tenant_key=$1 AND agent_key='erhua'").bind(&store.tenant).execute(&store.pool).await?;
    assert_eq!(
        reopened.foundation_execute_work(work).await?["status"],
        "completed"
    );
    let done = reopened.foundation_work_status(&manager, work).await?;
    assert_eq!(done["result"]["consumer"], "erhua");
    assert!(done["result"]["reply"]
        .as_str()
        .unwrap()
        .contains("完成整理"));
    assert_eq!(
        reopened.foundation_execute_work(work).await?["replayed"],
        true
    );
    let next = store
        .dispatch_context(&manager, one, Uuid::new_v4(), version, "撤权前第二个任务")
        .await?;
    command(
        &store,
        &owner,
        Change::EndCollaboration {
            collaboration: relation,
        },
    )
    .await?;
    let rejected = reopened
        .foundation_execute_work(id(&next["work_item_id"]))
        .await?;
    assert_eq!(rejected["status"], "failed");
    assert_eq!(rejected["reason"], "authority_changed_or_revoked");
    assert!(store
        .dispatch_context(
            &manager,
            one,
            Uuid::new_v4(),
            store.state(&owner).await?["version"].as_i64().unwrap(),
            "撤权后新任务"
        )
        .await
        .is_err());
    let successes:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE work_item_id=$1 AND event_type='completed'").bind(work).fetch_one(&store.pool).await?;
    assert_eq!(successes, 1);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn foundation_consumer_future_and_current_withdrawal_preserve_history_and_receipts(
) -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let one = find(&state, "scopes", "一栋");
    let person = find(&state, "people", "人员甲 · 合成样例 A");
    assign(&store, &owner, assignment(&state, person, one)).await?;
    let a = actor(&store, person).await?;
    let v1 = store
        .knowledge_save(&a, &write(one, "kitchen", "当前九点关闭"), false)
        .await?;
    let mut future = write(one, "kitchen", "未来八点关闭");
    future.expected_version = 1;
    future.effective_at = Some(Utc::now() + Duration::days(1));
    let v2 = store.knowledge_save(&a, &future, false).await?;
    let before = store.foundation_context(&a, one, "").await?;
    assert_eq!(knowledge(&before, "kitchen")["id"], v1["knowledge"]["id"]);
    assert_eq!(before["later_revisions"][0]["id"], v2["knowledge"]["id"]);
    let operation = Uuid::new_v4();
    let withdrawn = store
        .knowledge_withdraw(&a, operation, one, id(&v2["knowledge"]["id"]), 2)
        .await?;
    assert_eq!(withdrawn["latest_version"], 3);
    assert_eq!(
        store
            .knowledge_withdraw(&a, operation, one, id(&v2["knowledge"]["id"]), 2)
            .await?,
        withdrawn
    );
    assert!(store
        .knowledge_withdraw(&a, operation, one, id(&v1["knowledge"]["id"]), 3)
        .await
        .unwrap_err()
        .to_string()
        .contains("idempotency_conflict"));
    assert!(store
        .knowledge_withdraw(&a, Uuid::new_v4(), one, id(&v1["knowledge"]["id"]), 2)
        .await
        .unwrap_err()
        .to_string()
        .contains("knowledge_version_conflict"));
    let after = store.foundation_context(&a, one, "").await?;
    assert!(after["later_revisions"].as_array().unwrap().is_empty());
    assert_eq!(knowledge(&after, "kitchen")["id"], v1["knowledge"]["id"]);
    assert_eq!(knowledge(&after, "kitchen")["latest_version"], 3);
    future.operation_id = Uuid::new_v4();
    future.expected_version = 3;
    let v4 = store.knowledge_save(&a, &future, false).await?;
    assert_eq!(v4["knowledge"]["version"], 4);
    // Advance only this synthetic fixture's effective time; no wall-clock sleeps.
    sqlx::query("UPDATE qintopia_agent_os.collaboration_knowledge_revisions SET effective_at=clock_timestamp() WHERE id=$1").bind(id(&v4["knowledge"]["id"])).execute(&store.pool).await?;
    let effective = store.foundation_context(&a, one, "").await?;
    assert!(effective["later_revisions"].as_array().unwrap().is_empty());
    assert_eq!(
        knowledge(&effective, "kitchen")["id"],
        v4["knowledge"]["id"]
    );
    store
        .knowledge_withdraw(&a, Uuid::new_v4(), one, id(&v4["knowledge"]["id"]), 4)
        .await?;
    assert_eq!(
        knowledge(&store.foundation_context(&a, one, "").await?, "kitchen")["id"],
        v1["knowledge"]["id"]
    );
    store
        .knowledge_withdraw(&a, Uuid::new_v4(), one, id(&v1["knowledge"]["id"]), 5)
        .await?;
    let empty = store.foundation_context(&a, one, "").await?;
    assert!(empty["knowledge"].as_array().unwrap().is_empty());
    assert_eq!(empty["knowledge_items"][0]["latest_version"], 6);
    let mut new = write(one, "kitchen", "重新确定晚上七点关闭");
    new.expected_version = 6;
    assert_eq!(
        store.knowledge_save(&a, &new, false).await?["knowledge"]["version"],
        7
    );
    let revisions:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_knowledge_revisions WHERE tenant_key=$1").bind(&store.tenant).fetch_one(&store.pool).await?;
    assert_eq!(
        revisions, 4,
        "withdrawal retains history and consumes edit versions without creating content"
    );
    Ok(())
}
