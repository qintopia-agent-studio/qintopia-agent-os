//! User workflows through the real scoped rule service and local HTTP dispatcher.
#![cfg(feature = "postgres-integration-tests")]
use super::{
    foundation_review_tests::{assign, find, fixture},
    store::{RuleCommand, RuleEdit},
    Actor, Store,
};
use anyhow::Result;
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

async fn setup() -> Result<(Store, Actor, Value, Uuid)> {
    let (store, owner, state) = fixture().await?;
    let scope = find(&state, "scopes", "一栋");
    assign(
        &store,
        &owner,
        &state,
        store.verified_person(&owner).await?,
        scope,
        None,
    )
    .await?;
    Ok((store, owner, state, scope))
}
fn save(scope: Uuid, version: i64) -> RuleCommand {
    RuleCommand {
        operation_id: Uuid::new_v4(),
        scope,
        key: "kitchen".into(),
        kind: "rule".into(),
        expected_version: version,
        change: RuleEdit::Save {
            content: json!({"title":"厨房使用约定","text":"每天晚上九点关闭"}),
            effective_at: None,
            effective_until: None,
            replace_revision: None,
            reactivate: false,
        },
    }
}
fn edit(scope: Uuid, version: i64, change: RuleEdit) -> RuleCommand {
    RuleCommand {
        change,
        ..save(scope, version)
    }
}
fn id(value: &Value) -> Uuid {
    serde_json::from_value(value.clone()).unwrap()
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn steward_knowledge_import_requires_knowledge_authority_and_stops_shared_context(
) -> Result<()> {
    let (store, owner, state, scope) = setup().await?;
    let mut doc = save(scope, 0);
    doc.kind = "fact".into();
    doc.key = "kitchen_guide".into();
    if let RuleEdit::Save { content, .. } = &mut doc.change {
        *content = json!({"title":"厨房指南.md","text":"# 厨房指南\n\n共享区域使用后请归位。\n<script>never execute</script>"});
    }
    assert!(
        store.rule_command(&owner, &doc).await.is_err(),
        "rule authority does not confer knowledge authority"
    );
    let legacy = super::KnowledgeWrite {
        operation_id: Uuid::new_v4(),
        expected_version: 0,
        scope,
        key: "legacy_guide".into(),
        kind: "fact".into(),
        shared: false,
        case_ref: None,
        content: json!({"text":"不能通过旧入口绕过知识权限"}),
        effective_at: None,
        effective_until: None,
    };
    assert!(store.knowledge_save(&owner, &legacy, false).await.is_err());
    let current = store.state(&owner).await?;
    let person = store.verified_person(&owner).await?;
    let relation = current["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["person"] == json!(person) && r["scope"] == json!(scope) && r["agent"] == "erhua"
        })
        .unwrap();
    let assignment = serde_json::from_value(
        json!({"collaboration":relation["id"],"person":person,"role":find(&state,"roles","舍长"),"duty":find(&state,"duties","居民服务"),"scope":scope,"agent":"erhua","domain":"community_service","responsibility":"本栋知识维护","valid_until":null,"proxy_for":null,"permissions":[{"action":"confirm_knowledge","mode":"autonomous","reviewer":null},{"action":"change_rules","mode":"autonomous","reviewer":null}],"delegation":null}),
    )?;
    store
        .command(
            &owner,
            &super::Command {
                operation_id: Uuid::new_v4(),
                expected_version: current["version"].as_i64().unwrap(),
                change: super::Change::Assign(Box::new(assignment)),
            },
            true,
        )
        .await?;
    store.rule_command(&owner, &doc).await?;
    let context = store.foundation_context(&owner, scope, "general").await?;
    assert!(context["knowledge"]
        .as_array()
        .unwrap()
        .iter()
        .any(|k| k["key"] == "kitchen_guide" && k["kind"] == "fact"));
    let mut modified = doc.clone();
    modified.operation_id = Uuid::new_v4();
    assert!(
        store.rule_command(&owner, &modified).await.is_err(),
        "stale editor cannot overwrite"
    );
    modified.expected_version = 1;
    modified.change = RuleEdit::Stop;
    store.rule_command(&owner, &modified).await?;
    assert!(
        !store.foundation_context(&owner, scope, "general").await?["knowledge"]
            .as_array()
            .unwrap()
            .iter()
            .any(|k| k["key"] == "kitchen_guide")
    );
    assert_eq!(
        store.rule_state(&owner, scope).await?["items"][0]["revisions"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    Ok(())
}
async fn requester(store: &Store, owner: &Actor, state: &Value, scope: Uuid) -> Result<Actor> {
    let person = find(state, "people", "人员甲 · 合成样例 A");
    assign(
        store,
        owner,
        state,
        person,
        scope,
        Some(store.verified_person(owner).await?),
    )
    .await?;
    let link:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed'").bind(&store.tenant).bind(person).fetch_one(&store.pool).await?;
    store.actor(link).await
}
async fn talk(
    store: &Store,
    actor: &Actor,
    scope: Uuid,
    operation: Uuid,
    text: &str,
) -> Result<Value> {
    super::foundation_server::dispatch(
        store,
        actor,
        "/api/foundation/talk",
        &serde_json::to_vec(&json!({"scope":scope,"operation_id":operation,"text":text}))?,
    )
    .await
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn rule_lifecycle_expiry_never_restores_old_content() -> Result<()> {
    let (store, owner, _, scope) = setup().await?;
    store.rule_command(&owner, &save(scope, 0)).await?;
    let mut new = save(scope, 1);
    if let RuleEdit::Save {
        content,
        effective_until,
        ..
    } = &mut new.change
    {
        *content = json!({"title":"厨房使用约定","text":"临时关闭厨房"});
        *effective_until = Some(Utc::now() + Duration::days(1));
    }
    let result = store.rule_command(&owner, &new).await?;
    let context = store.rule_state(&owner, scope).await?;
    assert_eq!(
        context["items"][0]["current"]["content"]["text"],
        "临时关闭厨房"
    );
    // Move the fixture across its time boundary without depending on host sleeps.
    sqlx::query("UPDATE qintopia_agent_os.collaboration_knowledge_revisions SET effective_at=clock_timestamp()-interval '2 days',effective_until=clock_timestamp()-interval '1 day' WHERE id=$1").bind(id(&result["knowledge"]["id"])).execute(&store.pool).await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_knowledge_revisions SET effective_at=clock_timestamp()-interval '3 days' WHERE tenant_key=$1 AND id<>$2").bind(&store.tenant).bind(id(&result["knowledge"]["id"])).execute(&store.pool).await?;
    let context = store.rule_state(&owner, scope).await?;
    assert!(context["items"][0]["current"].is_null());
    assert_eq!(
        context["items"][0]["revisions"].as_array().unwrap().len(),
        2
    );
    assert!(
        store.foundation_context(&owner, scope, "general").await?["knowledge"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn rule_lifecycle_schedule_replace_cancel_and_stop() -> Result<()> {
    let (store, owner, _, scope) = setup().await?;
    store.rule_command(&owner, &save(scope, 0)).await?;
    let mut future = save(scope, 1);
    if let RuleEdit::Save {
        effective_at,
        effective_until,
        ..
    } = &mut future.change
    {
        *effective_at = Some(Utc::now() + Duration::days(2));
        *effective_until = Some(Utc::now() + Duration::days(3));
    }
    let old = store.rule_command(&owner, &future).await?;
    future.operation_id = Uuid::new_v4();
    future.expected_version = 2;
    if let RuleEdit::Save {
        replace_revision,
        content,
        ..
    } = &mut future.change
    {
        *replace_revision = Some(id(&old["knowledge"]["id"]));
        *content = json!({"title":"新排期","text":"未来十点关闭"});
    }
    let new = store.rule_command(&owner, &future).await?;
    let state = store.rule_state(&owner, scope).await?;
    assert_eq!(state["items"][0]["scheduled"].as_array().unwrap().len(), 1);
    assert_eq!(
        state["items"][0]["current"]["content"]["text"],
        "每天晚上九点关闭"
    );
    store
        .rule_command(
            &owner,
            &edit(
                scope,
                3,
                RuleEdit::CancelScheduled {
                    revision_id: id(&new["knowledge"]["id"]),
                },
            ),
        )
        .await?;
    assert!(
        store.rule_state(&owner, scope).await?["items"][0]["scheduled"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    let stale = future.clone();
    future.operation_id = Uuid::new_v4();
    future.expected_version = 4;
    if let RuleEdit::Save {
        replace_revision, ..
    } = &mut future.change
    {
        *replace_revision = None;
    }
    store.rule_command(&owner, &future).await?;
    let stop = edit(scope, 5, RuleEdit::Stop);
    store.rule_command(&owner, &stop).await?;
    let state = store.rule_state(&owner, scope).await?;
    assert!(state["items"][0]["current"].is_null());
    assert!(state["items"][0]["scheduled"]
        .as_array()
        .unwrap()
        .is_empty());
    assert_eq!(store.rule_command(&owner, &stop).await?["replayed"], true);
    assert_eq!(store.rule_command(&owner, &stale).await?["replayed"], true);
    assert!(store.rule_state(&owner, scope).await?["items"][0]["current"].is_null());
    assert_eq!(
        store
            .rule_command(&owner, &save(scope, 6))
            .await
            .unwrap_err()
            .to_string(),
        "knowledge_stopped"
    );
    let mut restart = save(scope, 6);
    if let RuleEdit::Save { reactivate, .. } = &mut restart.change {
        *reactivate = true;
    }
    store.rule_command(&owner, &restart).await?;
    assert_eq!(
        store.rule_state(&owner, scope).await?["items"][0]["current"]["content"]["text"],
        "每天晚上九点关闭"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn rule_lifecycle_review_approve_reject_cancel_and_replay() -> Result<()> {
    let (store, owner, state, scope) = setup().await?;
    let person = requester(&store, &owner, &state, scope).await?;
    let command = save(scope, 0);
    let result = store.rule_command(&person, &command).await?;
    let work = id(&result["work_item_id"]);
    assert_eq!(result["status"], "awaiting_review");
    let tasks = store.rule_tasks(&owner, scope).await?;
    assert_eq!(
        tasks[0]["input"]["lifecycle"],
        serde_json::to_value(&command)?
    );
    assert_eq!(tasks[0]["can_review"], true);
    assert!(tasks[0]["reviewer_name"]
        .as_str()
        .is_some_and(|s| !s.is_empty()));
    assert!(
        store.rule_state(&person, scope).await?["context"]["permissions"][0]["decision"]
            ["reviewer_name"]
            .is_string()
    );
    assert_eq!(
        store.rule_tasks(&person, scope).await?[0]["can_cancel"],
        true
    );
    assert!(store
        .rule_work_decide(&person, scope, work, "approve")
        .await
        .is_err());
    assert_eq!(
        store
            .rule_work_decide(&owner, scope, work, "approve")
            .await?["status"],
        "completed"
    );
    assert_eq!(
        store
            .rule_work_decide(&owner, scope, work, "approve")
            .await?["replayed"],
        true
    );
    assert_eq!(
        store.rule_state(&person, scope).await?["items"][0]["version"],
        1
    );
    let stop = edit(scope, 1, RuleEdit::Stop);
    let work = id(&store.rule_command(&person, &stop).await?["work_item_id"]);
    assert!(!store.rule_tasks(&owner, scope).await?[0]["input"]["before"].is_null());
    store
        .rule_work_decide(&owner, scope, work, "reject")
        .await?;
    assert_eq!(
        store.rule_tasks(&person, scope).await?[0]["reason"],
        "rejected_by_reviewer"
    );
    let stop = edit(scope, 1, RuleEdit::Stop);
    let work = id(&store.rule_command(&person, &stop).await?["work_item_id"]);
    store
        .rule_work_decide(&person, scope, work, "cancel")
        .await?;
    assert_eq!(
        store.rule_tasks(&person, scope).await?[0]["reason"],
        "cancelled_by_requester"
    );
    assert_eq!(
        store.rule_state(&person, scope).await?["items"][0]["version"],
        1
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn rule_lifecycle_stopped_rule_rejects_older_approved_request() -> Result<()> {
    let (store, owner, state, scope) = setup().await?;
    store.rule_command(&owner, &save(scope, 0)).await?;
    let person = requester(&store, &owner, &state, scope).await?;
    let command = save(scope, 1);
    let work = id(&store.rule_command(&person, &command).await?["work_item_id"]);
    store
        .rule_command(&owner, &edit(scope, 1, RuleEdit::Stop))
        .await?;
    // An unchanged request is replayable, but executing it cannot resurrect data.
    assert_eq!(
        id(&store.rule_command(&person, &command).await?["work_item_id"]),
        work
    );
    let result = store
        .rule_work_decide(&owner, scope, work, "approve")
        .await?;
    assert_eq!(result["status"], "failed");
    assert_eq!(result["reason"], "knowledge_version_conflict");
    assert!(store.rule_state(&person, scope).await?["items"][0]["current"].is_null());
    assert_eq!(
        store.rule_tasks(&person, scope).await?[0]["status"],
        "failed"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn rule_lifecycle_scope_conflicts_and_revocation_fail_closed() -> Result<()> {
    let (store, owner, state, scope) = setup().await?;
    let person = requester(&store, &owner, &state, scope).await?;
    let other = find(&state, "scopes", "二栋");
    assert!(store.rule_state(&person, other).await.is_err());
    assert!(store.rule_command(&person, &save(other, 0)).await.is_err());
    store.rule_command(&owner, &save(scope, 0)).await?;
    assert_eq!(
        store
            .rule_command(&owner, &save(scope, 0))
            .await
            .unwrap_err()
            .to_string(),
        "knowledge_version_conflict"
    );
    let command = save(scope, 1);
    let work = id(&store.rule_command(&person, &command).await?["work_item_id"]);
    store.foundation_approve_rule(&owner, work).await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants g SET status='revoked',version=g.version+1,revoked_at=clock_timestamp() FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE g.collaboration_id=c.id AND g.tenant_key=$1 AND a.person_id=$2 AND g.action_key='change_rules'").bind(&store.tenant).bind(store.verified_person(&person).await?).execute(&store.pool).await?;
    let result = store.foundation_execute_work(work).await?;
    assert_eq!(result["status"], "failed");
    assert_eq!(result["reason"], "authority_changed_or_revoked");
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn rule_lifecycle_dialogue_reports_content_preserves_dates_and_replays() -> Result<()> {
    let (store, owner, _, scope) = setup().await?;
    let mut command = save(scope, 0);
    let until = Utc::now() + Duration::days(7);
    if let RuleEdit::Save {
        effective_until, ..
    } = &mut command.change
    {
        *effective_until = Some(until);
    }
    store.rule_command(&owner, &command).await?;
    let query = talk(&store, &owner, scope, Uuid::new_v4(), "本栋有什么规则").await?;
    assert!(query["reply"]
        .as_str()
        .unwrap()
        .contains("每天晚上九点关闭"));
    assert!(query["reply"].as_str().unwrap().contains("截至"));
    let operation = Uuid::new_v4();
    talk(
        &store,
        &owner,
        scope,
        operation,
        "把本栋厨房关闭时间改成晚上十点",
    )
    .await?;
    let current = store.rule_state(&owner, scope).await?;
    assert_eq!(
        current["items"][0]["current"]["content"]["title"],
        "厨房使用约定"
    );
    let stored: chrono::DateTime<Utc> =
        serde_json::from_value(current["items"][0]["current"]["effective_until"].clone())?;
    assert!((stored - until).num_milliseconds().abs() < 1);
    talk(
        &store,
        &owner,
        scope,
        operation,
        "把本栋厨房关闭时间改成晚上十点",
    )
    .await?;
    assert_eq!(
        store.rule_state(&owner, scope).await?["items"][0]["version"],
        2
    );
    let create = Uuid::new_v4();
    let text = "新增约定：欢迎接待说明｜见面时主动介绍公共空间";
    talk(&store, &owner, scope, create, text).await?;
    talk(&store, &owner, scope, create, text).await?;
    assert_eq!(
        store.rule_state(&owner, scope).await?["items"]
            .as_array()
            .unwrap()
            .len(),
        2
    );
    talk(
        &store,
        &owner,
        scope,
        Uuid::new_v4(),
        "停止约定：欢迎接待说明",
    )
    .await?;
    talk(&store, &owner, scope, create, text).await?;
    let state = store.rule_state(&owner, scope).await?;
    let stopped = state["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|i| i["key"] != "kitchen")
        .unwrap();
    assert!(!stopped["stopped_at"].is_null());
    assert!(stopped["current"].is_null());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn rule_lifecycle_review_survives_logout_but_not_account_reset() -> Result<()> {
    use super::store::{AccountCommand, Credentials};
    let (store, owner, state, scope) = setup().await?;
    let member = requester(&store, &owner, &state, scope).await?;
    let pass = "synthetic-rule-session-test";
    store
        .bootstrap_account(store.verified_person(&owner).await?, "rule-reviewer", pass)
        .await?;
    let account = store
        .account_command(
            &owner,
            &AccountCommand::Create {
                person: store.verified_person(&member).await?,
                username: "rule-requester".into(),
                password: pass.into(),
            },
        )
        .await?;
    let owner_token = store
        .login(&Credentials {
            username: "rule-reviewer".into(),
            password: pass.into(),
        })
        .await?;
    let reviewer = store.session_actor(&owner_token).await?;
    let token = store
        .login(&Credentials {
            username: "rule-requester".into(),
            password: pass.into(),
        })
        .await?;
    let requester = store.session_actor(&token).await?;
    let work = id(&store.rule_command(&requester, &save(scope, 0)).await?["work_item_id"]);
    store.logout(&requester).await?;
    assert_eq!(
        store
            .rule_work_decide(&reviewer, scope, work, "approve")
            .await?["status"],
        "completed"
    );
    let token = store
        .login(&Credentials {
            username: "rule-requester".into(),
            password: pass.into(),
        })
        .await?;
    let requester = store.session_actor(&token).await?;
    let work = id(&store.rule_command(&requester, &save(scope, 1)).await?["work_item_id"]);
    store.foundation_approve_rule(&reviewer, work).await?;
    store
        .account_command(
            &owner,
            &AccountCommand::Reset {
                account: id(&account["id"]),
                password: "synthetic-rule-reset-test".into(),
            },
        )
        .await?;
    let result = store.foundation_execute_work(work).await?;
    assert_eq!(result["status"], "failed");
    assert_eq!(result["reason"], "request_account_changed_or_disabled");
    assert_eq!(
        store.rule_state(&reviewer, scope).await?["items"][0]["version"],
        1
    );
    // Reviewer logout is harmless; resetting the approving account invalidates proof.
    let token = store
        .login(&Credentials {
            username: "rule-requester".into(),
            password: "synthetic-rule-reset-test".into(),
        })
        .await?;
    let requester = store.session_actor(&token).await?;
    let work = id(&store.rule_command(&requester, &save(scope, 1)).await?["work_item_id"]);
    store.foundation_approve_rule(&reviewer, work).await?;
    store.logout(&reviewer).await?;
    assert_eq!(
        store.foundation_execute_work(work).await?["status"],
        "completed"
    );
    let owner_token = store
        .login(&Credentials {
            username: "rule-reviewer".into(),
            password: pass.into(),
        })
        .await?;
    let reviewer = store.session_actor(&owner_token).await?;
    let work = id(&store.rule_command(&requester, &save(scope, 2)).await?["work_item_id"]);
    store.foundation_approve_rule(&reviewer, work).await?;
    let reviewer_account:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_accounts WHERE tenant_key=$1 AND username='rule-reviewer'").bind(&store.tenant).fetch_one(&store.pool).await?;
    store
        .account_command(
            &owner,
            &AccountCommand::Disable {
                account: reviewer_account,
            },
        )
        .await?;
    let result = store.foundation_execute_work(work).await?;
    assert_eq!(result["status"], "failed");
    assert_eq!(result["reason"], "request_account_changed_or_disabled");
    Ok(())
}
