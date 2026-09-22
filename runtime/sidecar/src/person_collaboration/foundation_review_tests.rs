//! Independent regression cases for actual consumer scope and transaction boundaries.
#![cfg(feature = "postgres-integration-tests")]
use super::{model::*, Actor, Store};
use anyhow::Result;
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

async fn fixture() -> Result<(Store, Actor, Value)> {
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let store = Store::local(
        &database,
        &format!("synthetic-collaboration-review-{}", Uuid::new_v4()),
    )
    .await?;
    crate::db::run_migrations(&store.pool).await?;
    let owner = store.actor(store.bootstrap_fixture().await?).await?;
    store.bootstrap_identity_memory_fixture().await?;
    let state = store.state(&owner).await?;
    Ok((store, owner, state))
}
fn find(state: &Value, collection: &str, label: &str) -> Uuid {
    serde_json::from_value(
        state[collection]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["label"] == label)
            .unwrap()["id"]
            .clone(),
    )
    .unwrap()
}
async fn assign(
    store: &Store,
    owner: &Actor,
    state: &Value,
    person: Uuid,
    scope: Uuid,
    reviewer: Option<Uuid>,
) -> Result<Uuid> {
    let assignment = Assignment {
        collaboration: None,
        person,
        role: find(state, "roles", "舍长"),
        duty: Some(find(state, "duties", "居民服务")),
        scope,
        agent: "erhua".into(),
        domain: "community_service".into(),
        responsibility: "独立回归：本栋规则维护".into(),
        valid_from: None,
        valid_until: None,
        proxy_for: None,
        actions: vec![],
        permissions: vec![PermissionSetting {
            action: "change_rules".into(),
            mode: if reviewer.is_some() {
                PermissionMode::Confirmation
            } else {
                PermissionMode::Autonomous
            },
            reviewer,
        }],
        delegation: None,
    };
    let result = store
        .command(
            owner,
            &Command {
                operation_id: Uuid::new_v4(),
                expected_version: store.state(owner).await?["version"].as_i64().unwrap(),
                change: Change::Assign(Box::new(assignment)),
            },
            true,
        )
        .await?;
    Ok(serde_json::from_value(
        result["change"]["collaboration"].clone(),
    )?)
}
fn write(scope: Uuid) -> super::KnowledgeWrite {
    super::KnowledgeWrite {
        operation_id: Uuid::new_v4(),
        expected_version: 0,
        scope,
        key: "review.kitchen".into(),
        kind: "rule".into(),
        shared: false,
        case_ref: None,
        content: json!({"text":"合成厨房规则"}),
        effective_at: None,
        effective_until: None,
    }
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn foundation_review_failed_write_rolls_back_new_item_and_space() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let person = store.verified_person(&owner).await?;
    let scope = find(&state, "scopes", "一栋");
    assign(&store, &owner, &state, person, scope, None).await?;
    let before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_messages.conversations WHERE tenant_id=$1",
    )
    .bind(&store.tenant)
    .fetch_one(&store.pool)
    .await?;
    let mut request = write(scope);
    request.effective_until = Some(Utc::now() - Duration::minutes(1));
    let queued = store.knowledge_save(&owner, &request, true).await?;
    let work = serde_json::from_value(queued["work_item_id"].clone())?;
    let result = store.foundation_execute_work(work).await?;
    assert_eq!(result["status"], "failed");
    assert_eq!(result["reason"], "invalid_effective_interval");
    let items: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.collaboration_knowledge_items WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .fetch_one(&store.pool)
    .await?;
    let spaces: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_messages.conversations WHERE tenant_id=$1",
    )
    .bind(&store.tenant)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(items, 0);
    assert_eq!(spaces, before);
    assert_eq!(store.foundation_execute_work(work).await?["replayed"], true);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn foundation_review_gateway_scope_limits_management_and_approval() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let owner_person = store.verified_person(&owner).await?;
    let scope_two = find(&state, "scopes", "二栋");
    // A globally authorized manager entering through building one keeps that
    // Gateway boundary. This is adapter fixture setup, not a name-based merge.
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET person_id=$2,version=version+1 WHERE namespace=$1 AND source_ref='synthetic-resident'")
        .bind(format!("{}/synthetic-qiwe-one",store.tenant)).bind(owner_person).execute(&store.pool).await?;
    let scoped_owner = store
        .gateway_actor("synthetic-qiwe-one", "synthetic-resident")
        .await?;
    let version = store.state(&owner).await?["version"].as_i64().unwrap();
    assert!(store
        .dispatch_context(
            &scoped_owner,
            scope_two,
            Uuid::new_v4(),
            version,
            "合成跨栋分派"
        )
        .await
        .is_err());
    let works: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.collaboration_work_requests WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(works, 0);
    assign(&store, &owner, &state, owner_person, scope_two, None).await?;
    let requester = find(&state, "people", "人员甲 · 合成样例 A");
    assign(
        &store,
        &owner,
        &state,
        requester,
        scope_two,
        Some(owner_person),
    )
    .await?;
    let requester = store
        .gateway_actor("synthetic-wecom-two", "synthetic-resident")
        .await?;
    let pending = store
        .knowledge_save(&requester, &write(scope_two), false)
        .await?;
    assert_eq!(pending["status"], "awaiting_review");
    let work = serde_json::from_value(pending["work_item_id"].clone())?;
    assert!(store
        .foundation_approve_rule(&scoped_owner, work)
        .await
        .is_err());
    assert_eq!(
        store.foundation_work_status(&owner, work).await?["status"],
        "awaiting_review"
    );
    assert_eq!(
        store.foundation_approve_rule(&owner, work).await?["approved"],
        true
    );
    assert_eq!(
        store.foundation_execute_work(work).await?["status"],
        "completed"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn foundation_rule_approval_does_not_revive_when_same_reviewer_is_reappointed() -> Result<()>
{
    let (store, owner, state) = fixture().await?;
    let reviewer = store.verified_person(&owner).await?;
    let scope = find(&state, "scopes", "二栋");
    let original = assign(&store, &owner, &state, reviewer, scope, None).await?;
    let requester_person = find(&state, "people", "人员甲 · 合成样例 A");
    assign(
        &store,
        &owner,
        &state,
        requester_person,
        scope,
        Some(reviewer),
    )
    .await?;
    let requester = store
        .gateway_actor("synthetic-wecom-two", "synthetic-resident")
        .await?;
    let queued = store
        .knowledge_save(&requester, &write(scope), false)
        .await?;
    let work = serde_json::from_value(queued["work_item_id"].clone())?;
    store.foundation_approve_rule(&owner, work).await?;
    store
        .command(
            &owner,
            &Command {
                operation_id: Uuid::new_v4(),
                expected_version: store.state(&owner).await?["version"].as_i64().unwrap(),
                change: Change::EndCollaboration {
                    collaboration: original,
                },
            },
            true,
        )
        .await?;
    let replacement = assign(&store, &owner, &state, reviewer, scope, None).await?;
    assert_ne!(replacement, original);
    let executed = store.foundation_execute_work(work).await?;
    assert_eq!(executed["status"], "failed");
    assert_eq!(executed["reason"], "approval_authority_changed_or_revoked");
    let knowledge: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.collaboration_knowledge_items WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(knowledge, 0);
    Ok(())
}
