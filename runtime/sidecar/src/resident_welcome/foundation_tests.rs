#![cfg(feature = "postgres-integration-tests")]
use super::{
    foundation::{CardMaterial, SyntheticOutcome},
    review::ReviewRequest,
    store::Store,
};
use crate::person_collaboration::{
    Actor, Assignment, Change, Command, KnowledgeWrite, Store as People,
};
use anyhow::Result;
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

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
async fn assign(
    people: &People,
    owner: &Actor,
    person: Uuid,
    scope: Uuid,
    confirm: Option<Uuid>,
) -> Result<Value> {
    let state = people.state(owner).await?;
    let prior = state["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["person"] == json!(person)
                && r["scope"] == json!(scope)
                && r["agent"] == "erhua"
                && r["status"] == "active"
        })
        .map(|r| r["id"].clone());
    let assignment: Assignment = serde_json::from_value(
        json!({"collaboration":prior,"person":person,"role":find(&state,"roles","舍长"),"duty":find(&state,"duties","居民服务"),"scope":scope,"agent":"erhua","domain":"community_service","responsibility":"合成欢迎居民服务","valid_until":null,"proxy_for":null,"permissions":[{"action":"designate","mode":"autonomous","reviewer":null},{"action":"confirm_knowledge","mode":"autonomous","reviewer":null},{"action":"change_rules","mode":"autonomous","reviewer":null},{"action":"review","mode":"autonomous","reviewer":null},{"action":"publish","mode":if confirm.is_some(){"confirmation"}else{"autonomous"},"reviewer":confirm}],"delegation":null}),
    )?;
    people
        .command(
            owner,
            &Command {
                operation_id: Uuid::new_v4(),
                expected_version: state["version"].as_i64().unwrap(),
                change: Change::Assign(Box::new(assignment)),
            },
            true,
        )
        .await
}
struct Fixture {
    welcome: Store,
    people: People,
    owner: Actor,
    tenant: String,
    seed: Value,
    stewards: [Uuid; 2],
}
async fn fixture() -> Result<Fixture> {
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let tenant = format!("synthetic-collaboration-welcome-{}", Uuid::new_v4());
    let people = People::local(&database, &tenant).await?;
    let welcome = Store::local(&database).await?;
    crate::db::run_migrations(&welcome.pool).await?;
    let owner = people.actor(people.bootstrap_fixture().await?).await?;
    let state = people.state(&owner).await?;
    let stewards = [
        find(&state, "people", "人员甲 · 合成样例 A"),
        find(&state, "people", "人员甲 · 合成样例 B"),
    ];
    for (person, building) in stewards.iter().zip(["一栋", "二栋"]) {
        assign(
            &people,
            &owner,
            *person,
            find(&state, "scopes", building),
            None,
        )
        .await?;
    }
    let seed = welcome.bootstrap_foundation_fixture(&tenant).await?;
    Ok(Fixture {
        welcome,
        people,
        owner,
        tenant,
        seed,
        stewards,
    })
}
fn target(f: &Fixture, n: usize) -> Uuid {
    id(&f.seed["targets"][n]["target_ref"])
}
fn case(f: &Fixture, n: usize) -> Uuid {
    id(&f.seed["targets"][n]["case_ref"])
}
fn scope(f: &Fixture, n: usize) -> Uuid {
    id(&f.seed["targets"][n]["scope_ref"])
}
async fn setting(
    f: &Fixture,
    n: usize,
    mode: &str,
    parts: &[&str],
    case_ref: Option<Uuid>,
    expected: i64,
) -> Result<Value> {
    f.welcome.foundation_configure(&f.tenant,f.stewards[n],target(f,n),&KnowledgeWrite{operation_id:Uuid::new_v4(),expected_version:expected,scope:scope(f,n),key:"resident_welcome".into(),kind:"rule".into(),shared:false,case_ref,content:json!({"mode":mode,"parts":parts,"phase":"formal","text_template":"欢迎 {name} 来到社区，一起从容生活。"}),effective_at:None,effective_until:None}).await
}
async fn render(f: &Fixture, n: usize) -> Result<Value> {
    f.welcome
        .foundation_render(
            case(f, n),
            &CardMaterial {
                display_name: "合成小林".into(),
                description: "喜欢阅读与散步，期待认识新朋友。".into(),
                welcome_text: "合成资料，不代表任何实际住客。".into(),
            },
        )
        .await
}
async fn prepare(f: &Fixture, n: usize) -> Result<Value> {
    f.welcome
        .foundation_prepare(case(f, n), target(f, n), "formal")
        .await
}
async fn artifacts(f: &Fixture, n: usize) -> Result<Vec<Value>> {
    let state = f.welcome.foundation_state(&f.tenant, f.stewards[n]).await?;
    Ok(state["targets"][0]["artifacts"]
        .as_array()
        .unwrap()
        .iter()
        .filter(|v| v["case_ref"] == json!(case(f, n)))
        .cloned()
        .collect())
}
async fn approve(f: &Fixture, n: usize, artifact: &Value, reviewer: Uuid) -> Result<Value> {
    f.welcome
        .foundation_approve(
            &f.tenant,
            reviewer,
            &ReviewRequest {
                operation: Uuid::new_v4(),
                artifact: id(&artifact["artifact_ref"]),
                target: target(f, n),
                target_version: 1,
                phase: "formal".into(),
                content_hash: artifact["content_hash"].as_str().unwrap().into(),
            },
        )
        .await
}
async fn effects(f: &Fixture) -> Result<i64> {
    Ok(sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.welcome_synthetic_effects e JOIN qintopia_agent_os.welcome_actions a ON a.id=e.action_id JOIN qintopia_agent_os.welcome_cases c ON c.id=a.case_id WHERE c.source_instance=$1").bind(f.seed["source"].as_str().unwrap()).fetch_one(&f.welcome.pool).await?)
}

#[tokio::test]
#[ignore = "explicit isolated database and Pillow renderer required"]
async fn foundation_welcome_direct_review_real_agents_and_partial_unknown_recovery() -> Result<()> {
    let f = fixture().await?;
    setting(&f, 0, "direct", &["text", "image"], None, 0).await?;
    setting(&f, 1, "review", &["text", "image"], None, 0).await?;
    render(&f, 0).await?;
    render(&f, 1).await?;
    let direct = prepare(&f, 0).await?;
    assert_eq!(direct["actions"].as_array().unwrap().len(), 2);
    let fake:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.welcome_approvals a JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=a.artifact_id WHERE b.case_id=$1").bind(case(&f,0)).fetch_one(&f.welcome.pool).await?;
    assert_eq!(fake, 0, "direct never fabricates approval");
    let waiting = prepare(&f, 1).await?;
    assert!(waiting["actions"].as_array().unwrap().is_empty());
    assert_eq!(waiting["waiting"].as_array().unwrap().len(), 2);
    let a = id(&direct["actions"][0]);
    let b = id(&direct["actions"][1]);
    assert_eq!(
        f.welcome
            .foundation_execute(a, SyntheticOutcome::Success)
            .await?["status"],
        "succeeded"
    );
    assert_eq!(
        f.welcome
            .foundation_execute(b, SyntheticOutcome::DefiniteNoSend)
            .await?["status"],
        "retryable"
    );
    assert_eq!(effects(&f).await?, 1);
    let again = prepare(&f, 0).await?;
    assert_eq!(direct["actions"], again["actions"]);
    f.welcome
        .foundation_execute(a, SyntheticOutcome::Success)
        .await?;
    assert_eq!(
        f.welcome
            .foundation_execute(b, SyntheticOutcome::UnknownAfterSend)
            .await?["status"],
        "unknown"
    );
    f.welcome
        .foundation_execute(b, SyntheticOutcome::Success)
        .await?;
    assert_eq!(effects(&f).await?, 2, "unknown must not blindly resend");
    let reopened = Store::local(&crate::foundation_test_support::database_url(
        "QINTOPIA_COLLABORATION_TEST",
    )?)
    .await?;
    assert_eq!(
        reopened.foundation_reconcile_unknown(b).await?["status"],
        "succeeded"
    );
    for artifact in artifacts(&f, 1).await? {
        approve(&f, 1, &artifact, f.stewards[1]).await?;
    }
    let reviewed = prepare(&f, 1).await?;
    assert_eq!(reviewed["actions"].as_array().unwrap().len(), 2);
    for action in reviewed["actions"].as_array().unwrap() {
        f.welcome
            .foundation_execute(id(action), SyntheticOutcome::Success)
            .await?;
    }
    assert_eq!(effects(&f).await?, 4);
    let agents:Vec<String>=sqlx::query_scalar("SELECT DISTINCT e.actor_id FROM qintopia_agent_os.work_item_events e JOIN qintopia_agent_os.work_items w ON w.id=e.work_item_id WHERE e.event_type='welcome_agent_result' AND (w.id IN (SELECT work_item_id FROM qintopia_agent_os.welcome_actions WHERE case_id=ANY($1)) OR e.data->'result'->>'case_ref'=ANY($2)) ORDER BY e.actor_id")
        .bind(vec![case(&f,0),case(&f,1)]).bind(vec![case(&f,0).to_string(),case(&f,1).to_string()]).fetch_all(&f.welcome.pool).await?;
    assert_eq!(agents, vec!["anan", "erhua", "huabaosi"]);
    let stored:Vec<u8>=sqlx::query_scalar("SELECT d.content FROM qintopia_agent_os.welcome_local_artifact_data d JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=d.artifact_id WHERE b.case_id=$1 AND d.media_type='image/png' LIMIT 1").bind(case(&f,0)).fetch_one(&f.welcome.pool).await?;
    assert_eq!(image::load_from_memory(&stored)?.width(), 1080);
    let runtime:Vec<Value>=sqlx::query_scalar("SELECT e.data FROM qintopia_agent_os.work_item_events e JOIN qintopia_agent_os.work_items w ON w.id=e.work_item_id WHERE e.event_type='welcome_runtime_result' AND w.idempotency_key LIKE ANY($1)")
        .bind(vec![format!("%{}%",case(&f,0)),format!("%{}%",case(&f,1))]).fetch_all(&f.welcome.pool).await?;
    let operations: std::collections::BTreeSet<String> = runtime
        .iter()
        .map(|e| {
            format!(
                "{}.{}",
                e["agent"].as_str().unwrap(),
                e["operation"].as_str().unwrap()
            )
        })
        .collect();
    assert_eq!(
        operations,
        [
            "anan.prepare",
            "anan.request_card",
            "erhua.forward",
            "erhua.forward_review",
            "huabaosi.render_card"
        ]
        .into_iter()
        .map(str::to_owned)
        .collect()
    );
    let mut call_ids = std::collections::HashSet::new();
    for event in &runtime {
        assert_eq!(event["runtime"], "local_scripted_agent_runtime");
        assert_eq!(event["plugin_version"], "0.1.0");
        assert!(event["process_id"].as_u64().unwrap() > 0);
        assert_ne!(
            event["process_id"].as_u64().unwrap(),
            u64::from(std::process::id())
        );
        assert!(call_ids.insert(event["call_id"].clone().to_string()));
        for hash in ["request_sha256", "output_sha256", "plugin_source_sha256"] {
            assert_eq!(event[hash].as_str().unwrap().len(), 64);
        }
        let invocation:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.work_item_events WHERE event_type='welcome_runtime_invoked' AND data->>'call_id'=$1 AND data->>'request_sha256'=$2")
            .bind(event["call_id"].as_str().unwrap()).bind(event["request_sha256"].as_str().unwrap()).fetch_one(&f.welcome.pool).await?;
        assert_eq!(invocation, 1);
    }
    let render = runtime
        .iter()
        .find(|e| {
            e["operation"] == "render_card" && e["output_binding"]["case_ref"] == json!(case(&f, 0))
        })
        .unwrap();
    assert_eq!(
        render["output_binding"]["content_hash"],
        super::digest(&stored),
        "the actual Agent PNG became the stored Artifact"
    );
    let forwards: Vec<_> = runtime
        .iter()
        .filter(|e| e["operation"] == "forward")
        .collect();
    assert_eq!(
        forwards
            .iter()
            .filter(|e| e["output_binding"]["action_ref"] == json!(a))
            .count(),
        1,
        "success does not repeat the Agent send tool"
    );
    assert_eq!(
        forwards
            .iter()
            .filter(|e| e["output_binding"]["action_ref"] == json!(b))
            .count(),
        2,
        "only explicit no-send retries; unknown readback never runs the send tool again"
    );
    let bound_effects:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.welcome_synthetic_effects effect JOIN qintopia_agent_os.welcome_actions action ON action.id=effect.action_id JOIN qintopia_agent_os.work_item_events e ON e.work_item_id=action.work_item_id AND e.event_type='welcome_runtime_result' AND e.data->>'operation'='forward' WHERE action.case_id=ANY($1) AND e.data->'output_binding'->>'action_ref'=effect.action_id::text AND e.data->'output_binding'->>'attempt_ref'=effect.attempt_id::text AND e.data->'output_binding'->>'artifact_ref'=effect.artifact_id::text AND e.data->'output_binding'->>'content_hash'=effect.content_hash")
        .bind(vec![case(&f,0),case(&f,1)]).fetch_one(&f.welcome.pool).await?;
    assert_eq!(
        bound_effects, 4,
        "every actual synthetic provider effect consumed the bound Agent instruction"
    );

    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database and Pillow renderer required"]
async fn foundation_welcome_single_overrides_future_expiry_and_content_versions() -> Result<()> {
    let f = fixture().await?;
    setting(&f, 0, "direct", &["text", "image"], None, 0).await?;
    render(&f, 0).await?;
    let old = prepare(&f, 0).await?;
    setting(&f, 0, "review", &["text", "image"], Some(case(&f, 0)), 0).await?;
    assert!(f
        .welcome
        .foundation_execute(id(&old["actions"][0]), SyntheticOutcome::Success)
        .await
        .is_err());
    assert_eq!(effects(&f).await?, 0);
    let one = prepare(&f, 0).await?;
    assert_eq!(one["waiting"][0]["reason"], "content_approval_required");
    // One-time review does not overwrite the persistent direct default.
    let state = f.welcome.foundation_state(&f.tenant, f.stewards[0]).await?;
    assert_eq!(state["targets"][0]["rule"]["content"]["mode"], "direct");
    let art = artifacts(&f, 0).await?;
    let granted = approve(&f, 0, &art[0], f.stewards[0]).await?;
    f.welcome
        .foundation_revoke_approval(&f.tenant, f.stewards[0], id(&granted["approval_ref"]), 1)
        .await?;
    assert!(prepare(&f, 0).await?["waiting"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["reason"] == "content_approval_required"));
    // W10: direct only for the current case leaves the next default untouched.
    setting(&f, 0, "review", &["text", "image"], None, 1).await?;
    setting(&f, 0, "direct", &["text", "image"], Some(case(&f, 0)), 1).await?;
    assert_eq!(
        prepare(&f, 0).await?["actions"].as_array().unwrap().len(),
        2
    );
    assert_eq!(
        f.welcome.foundation_state(&f.tenant, f.stewards[0]).await?["targets"][0]["rule"]
            ["content"]["mode"],
        "review"
    );
    // Expired case setting resolves to the then-current default, with no sleeps.
    sqlx::query("UPDATE qintopia_agent_os.collaboration_knowledge_revisions r SET effective_at=clock_timestamp()-interval '2 hours',effective_until=clock_timestamp()-interval '1 hour' FROM qintopia_agent_os.collaboration_knowledge_items i WHERE i.id=r.item_id AND i.tenant_key=$1 AND i.case_ref=$2").bind(&f.tenant).bind(case(&f,0)).execute(&f.welcome.pool).await?;
    assert_eq!(
        prepare(&f, 0).await?["waiting"][0]["reason"],
        "content_approval_required"
    );
    let future = KnowledgeWrite {
        operation_id: Uuid::new_v4(),
        expected_version: 2,
        scope: scope(&f, 0),
        key: "resident_welcome".into(),
        kind: "rule".into(),
        shared: false,
        case_ref: None,
        content: json!({"mode":"direct","parts":["text","image"],"phase":"formal","text_template":"未来版本欢迎"}),
        effective_at: Some(Utc::now() + Duration::hours(1)),
        effective_until: None,
    };
    f.welcome
        .foundation_configure(&f.tenant, f.stewards[0], target(&f, 0), &future)
        .await?;
    assert_eq!(
        prepare(&f, 0).await?["waiting"][0]["reason"],
        "content_approval_required"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database and Pillow renderer required"]
async fn foundation_welcome_timing_permission_change_and_scope_fail_closed() -> Result<()> {
    let f = fixture().await?;
    setting(&f, 0, "direct", &["text"], None, 0).await?;
    render(&f, 0).await?;
    let text_only = prepare(&f, 0).await?;
    assert_eq!(text_only["actions"].as_array().unwrap().len(), 1);
    assert_eq!(text_only["waiting"][0]["part"], "image");
    assert!(f.welcome.foundation_configure(&f.tenant,f.stewards[1],target(&f,0),&KnowledgeWrite{operation_id:Uuid::new_v4(),expected_version:1,scope:scope(&f,0),key:"resident_welcome".into(),kind:"rule".into(),shared:false,case_ref:None,content:json!({"mode":"direct","parts":["text","image"],"phase":"formal","text_template":"越权"}),effective_at:None,effective_until:None}).await.is_err());
    setting(&f, 0, "direct", &["text", "image"], None, 1).await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET projection=jsonb_set(projection,'{state}','\"Reserved\"') WHERE source_instance=$1 AND aggregate_id='synthetic-order-0'").bind(f.seed["source"].as_str().unwrap()).execute(&f.welcome.pool).await?;
    assert_eq!(
        prepare(&f, 0).await?["waiting"][0]["reason"],
        "actual_check_in_required"
    );
    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET projection=jsonb_set(projection,'{state}','\"InHouse\"') WHERE source_instance=$1 AND aggregate_id='synthetic-order-0'").bind(f.seed["source"].as_str().unwrap()).execute(&f.welcome.pool).await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_members SET current=false WHERE target_id=$1")
        .bind(target(&f, 0))
        .execute(&f.welcome.pool)
        .await?;
    assert_eq!(
        prepare(&f, 0).await?["waiting"][0]["reason"],
        "current_target_membership_required"
    );
    sqlx::query("UPDATE qintopia_agent_os.welcome_members SET current=true WHERE target_id=$1")
        .bind(target(&f, 0))
        .execute(&f.welcome.pool)
        .await?;
    let ready = prepare(&f, 0).await?;
    let action = id(&ready["actions"][0]);
    let claim = f.welcome.claim_delivery(action, Uuid::new_v4()).await?;
    let grant:Uuid=sqlx::query_scalar("SELECT g.id FROM qintopia_agent_os.collaboration_grants g JOIN qintopia_agent_os.agent_collaborations c ON c.id=g.collaboration_id JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE g.tenant_key=$1 AND a.person_id=$2 AND g.action_key='publish' AND g.status='active'").bind(&f.tenant).bind(f.stewards[0]).fetch_one(&f.welcome.pool).await?;
    let state = f.people.state(&f.owner).await?;
    f.people
        .command(
            &f.owner,
            &Command {
                operation_id: Uuid::new_v4(),
                expected_version: state["version"].as_i64().unwrap(),
                change: Change::RevokeGrant { grant },
            },
            true,
        )
        .await?;
    assert!(f.welcome.begin_synthetic_attempt(&claim).await.is_err());
    assert_eq!(effects(&f).await?, 0);
    // Historical rebuild may keep artifacts, but never re-admit or execute backlog.
    f.welcome
        .start_rebuild(
            f.seed["source"].as_str().unwrap(),
            "fixture-property",
            "synthetic-head",
        )
        .await?;
    assert!(f
        .welcome
        .foundation_render(
            case(&f, 0),
            &CardMaterial {
                display_name: "合成".into(),
                description: "".into(),
                welcome_text: "欢迎".into()
            }
        )
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database and Pillow renderer required"]
async fn foundation_welcome_upper_confirmation_and_executor_retention() -> Result<()> {
    let f = fixture().await?;
    assign(&f.people, &f.owner, f.stewards[1], scope(&f, 0), None).await?;
    assign(
        &f.people,
        &f.owner,
        f.stewards[0],
        scope(&f, 0),
        Some(f.stewards[1]),
    )
    .await?;
    setting(&f, 0, "direct", &["text", "image"], None, 0).await?;
    for agent in ["anan", "huabaosi"] {
        sqlx::query("UPDATE qintopia_agent_os.collaboration_local_executors SET available=false WHERE tenant_key=$1 AND agent_key=$2").bind(&f.tenant).bind(agent).execute(&f.welcome.pool).await?;
        assert!(render(&f, 0).await.is_err());
        sqlx::query("UPDATE qintopia_agent_os.collaboration_local_executors SET available=true WHERE tenant_key=$1 AND agent_key=$2").bind(&f.tenant).bind(agent).execute(&f.welcome.pool).await?;
    }
    render(&f, 0).await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_local_executors SET available=false WHERE tenant_key=$1 AND agent_key='erhua'").bind(&f.tenant).execute(&f.welcome.pool).await?;
    assert!(prepare(&f, 0).await.is_err());
    sqlx::query("UPDATE qintopia_agent_os.collaboration_local_executors SET available=true WHERE tenant_key=$1 AND agent_key='erhua'").bind(&f.tenant).execute(&f.welcome.pool).await?;
    let pending = prepare(&f, 0).await?;
    assert_eq!(
        pending["waiting"][0]["reason"],
        "upper_confirmation_required"
    );
    let materials = artifacts(&f, 0).await?;
    assert!(
        approve(&f, 0, &materials[0], f.stewards[0]).await.is_err(),
        "own rule cannot bypass upper confirmer"
    );
    for artifact in materials {
        approve(&f, 0, &artifact, f.stewards[1]).await?;
    }
    let ready = prepare(&f, 0).await?;
    assert_eq!(ready["actions"].as_array().unwrap().len(), 2);
    let action = id(&ready["actions"][0]);
    sqlx::query("UPDATE qintopia_agent_os.collaboration_local_executors SET available=false WHERE tenant_key=$1 AND agent_key='erhua'").bind(&f.tenant).execute(&f.welcome.pool).await?;
    assert_eq!(
        f.welcome
            .foundation_execute(action, SyntheticOutcome::Success)
            .await?["reason"],
        "executor_unavailable"
    );
    assert_eq!(effects(&f).await?, 0);
    sqlx::query("UPDATE qintopia_agent_os.collaboration_local_executors SET available=true WHERE tenant_key=$1 AND agent_key='erhua'").bind(&f.tenant).execute(&f.welcome.pool).await?;
    assert_eq!(
        f.welcome
            .foundation_execute(action, SyntheticOutcome::Success)
            .await?["status"],
        "succeeded"
    );
    assert_eq!(effects(&f).await?, 1);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database and Pillow renderer required"]
async fn foundation_welcome_month_review_never_reuses_previous_case_approval() -> Result<()> {
    let f = fixture().await?;
    let setting = KnowledgeWrite {
        operation_id: Uuid::new_v4(),
        expected_version: 0,
        scope: scope(&f, 0),
        key: "resident_welcome".into(),
        kind: "rule".into(),
        shared: false,
        case_ref: None,
        content: json!({"mode":"review","parts":["text","image"],"phase":"formal","text_template":"欢迎 {name}，慢慢熟悉这里。"}),
        effective_at: None,
        effective_until: Some(Utc::now() + Duration::days(30)),
    };
    f.welcome
        .foundation_configure(&f.tenant, f.stewards[0], target(&f, 0), &setting)
        .await?;
    render(&f, 0).await?;
    prepare(&f, 0).await?;
    for artifact in artifacts(&f, 0).await? {
        approve(&f, 0, &artifact, f.stewards[0]).await?;
    }
    assert_eq!(
        prepare(&f, 0).await?["actions"].as_array().unwrap().len(),
        2
    );
    let raw:Value=sqlx::query_scalar("SELECT projection FROM qintopia_agent_os.welcome_source_versions WHERE source_instance=$1 AND aggregate_id='synthetic-order-1'").bind(f.seed["source"].as_str().unwrap()).fetch_one(&f.welcome.pool).await?;
    let mut snapshot: super::state::Snapshot = serde_json::from_value(raw)?;
    snapshot.revision = "2".into();
    snapshot.building = "一栋".into();
    f.welcome.apply_snapshot(None, &snapshot).await?;
    render(&f, 1).await?;
    let second = f
        .welcome
        .foundation_prepare(case(&f, 1), target(&f, 0), "formal")
        .await?;
    assert!(second["actions"].as_array().unwrap().is_empty());
    assert_eq!(second["waiting"][0]["reason"], "content_approval_required");
    assert_eq!(effects(&f).await?, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database and Pillow renderer required"]
async fn foundation_welcome_content_revision_consent_and_legacy_bypass_rejected() -> Result<()> {
    let f = fixture().await?;
    setting(&f, 0, "direct", &["text", "image"], None, 0).await?;
    render(&f, 0).await?;
    let before = prepare(&f, 0).await?;
    let image_action: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_agent_os.welcome_actions WHERE case_id=$1 AND part='image'",
    )
    .bind(case(&f, 0))
    .fetch_one(&f.welcome.pool)
    .await?;
    let previous: Uuid =
        sqlx::query_scalar("SELECT artifact_id FROM qintopia_agent_os.welcome_actions WHERE id=$1")
            .bind(image_action)
            .fetch_one(&f.welcome.pool)
            .await?;
    f.welcome
        .foundation_render(
            case(&f, 0),
            &CardMaterial {
                display_name: "合成小林".into(),
                description: "本人更正：喜欢徒步，也喜欢阅读。".into(),
                welcome_text: "旧文案不能代替目标规则".into(),
            },
        )
        .await?;
    assert!(f
        .welcome
        .foundation_execute(image_action, SyntheticOutcome::Success)
        .await
        .is_err());
    let after = prepare(&f, 0).await?;
    assert_eq!(before["actions"], after["actions"]);
    let current: Uuid =
        sqlx::query_scalar("SELECT artifact_id FROM qintopia_agent_os.welcome_actions WHERE id=$1")
            .bind(image_action)
            .fetch_one(&f.welcome.pool)
            .await?;
    assert_ne!(previous, current);
    // A legacy welcome grant cannot authorize this target after foundation binding.
    let grant:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_grants(person_id,source_instance,property_id,target_id,action,expires_at,appointed_by) VALUES($1,$2,'fixture-property',$3,'publish',now()+interval '1 day',$1) RETURNING id")
        .bind(f.stewards[0]).bind(f.seed["source"].as_str().unwrap()).bind(target(&f,0)).fetch_one(&f.welcome.pool).await?;
    let hash: String =
        sqlx::query_scalar("SELECT content_hash FROM qintopia_agent_os.artifacts WHERE id=$1")
            .bind(current)
            .fetch_one(&f.welcome.pool)
            .await?;
    let approval:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_approvals(artifact_id,target_id,target_version,phase,content_hash,grant_id,grant_version,approved_by) VALUES($1,$2,1,'formal',$3,$4,1,$5) RETURNING id")
        .bind(current).bind(target(&f,0)).bind(hash).bind(grant).bind(f.stewards[0]).fetch_one(&f.welcome.pool).await?;
    assert!(f
        .welcome
        .prepare_delivery(case(&f, 0), approval, grant)
        .await
        .is_err());
    let app:Value=sqlx::query_scalar("SELECT jsonb_build_object('source',a.source_instance,'resource',a.resource_ref,'record',a.record_ref,'person',a.person_id) FROM qintopia_agent_os.welcome_applications a JOIN qintopia_agent_os.welcome_cases c ON c.application_id=a.id WHERE c.id=$1").bind(case(&f,0)).fetch_one(&f.welcome.pool).await?;
    f.welcome
        .application_from_readback(
            &super::channels::ApplicationRevision {
                source: app["source"].as_str().unwrap().into(),
                resource: app["resource"].as_str().unwrap().into(),
                record: app["record"].as_str().unwrap().into(),
                person: Some(id(&app["person"])),
                revision: 2,
                valid: true,
                consent_version: 2,
                consent_active: false,
                field_hash: super::digest(b"synthetic consent withdrawn"),
            },
            "synthetic-application-base",
        )
        .await?;
    assert!(f
        .welcome
        .foundation_execute(image_action, SyntheticOutcome::Success)
        .await
        .is_err());
    assert_eq!(effects(&f).await?, 0);
    Ok(())
}

async fn publish_only_confirmer(f: &Fixture) -> Result<()> {
    let state = f.people.state(&f.owner).await?;
    let assignment: Assignment = serde_json::from_value(json!({
        "collaboration":null,"person":f.stewards[1],"role":find(&state,"roles","舍长"),
        "duty":find(&state,"duties","居民服务"),"scope":scope(f,0),"agent":"erhua",
        "domain":"community_service","responsibility":"合成上层发布确认人",
        "valid_until":null,"proxy_for":null,"permissions":[{"action":"publish","mode":"autonomous","reviewer":null}],"delegation":null
    }))?;
    f.people
        .command(
            &f.owner,
            &Command {
                operation_id: Uuid::new_v4(),
                expected_version: state["version"].as_i64().unwrap(),
                change: Change::Assign(Box::new(assignment)),
            },
            true,
        )
        .await?;
    assign(
        &f.people,
        &f.owner,
        f.stewards[0],
        scope(f, 0),
        Some(f.stewards[1]),
    )
    .await?;
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database and Pillow renderer required"]
async fn foundation_welcome_direct_upper_confirmation_needs_publish_without_review() -> Result<()> {
    let f = fixture().await?;
    publish_only_confirmer(&f).await?;
    setting(&f, 0, "direct", &["text", "image"], None, 0).await?;
    render(&f, 0).await?;
    let pending = prepare(&f, 0).await?;
    assert_eq!(
        pending["waiting"][0]["approval_kind"],
        "publish_confirmation"
    );
    assert_eq!(pending["waiting"][0]["reviewer"], json!(f.stewards[1]));
    let upper_state = f.welcome.foundation_state(&f.tenant, f.stewards[1]).await?;
    let first = upper_state["targets"]
        .as_array()
        .unwrap()
        .iter()
        .find(|t| t["target_ref"] == json!(target(&f, 0)))
        .unwrap();
    assert_eq!(first["review"], "denied");
    assert_eq!(first["publish"], "autonomous");
    for material in artifacts(&f, 0).await? {
        let accepted = approve(&f, 0, &material, f.stewards[1]).await?;
        assert_eq!(accepted["approval_kind"], "publish_confirmation");
    }
    let ready = prepare(&f, 0).await?;
    assert_eq!(ready["actions"].as_array().unwrap().len(), 2);
    let content:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.welcome_approvals a JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=a.artifact_id WHERE b.case_id=$1 AND a.foundation_basis->>'approval_kind'='content_review'").bind(case(&f,0)).fetch_one(&f.welcome.pool).await?;
    assert_eq!(content, 0, "direct does not invent a content review");
    for action in ready["actions"].as_array().unwrap() {
        f.welcome
            .foundation_execute(id(action), SyntheticOutcome::Success)
            .await?;
    }
    assert_eq!(effects(&f).await?, 2);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database and Pillow renderer required"]
async fn foundation_welcome_review_and_upper_confirmation_remain_separate() -> Result<()> {
    let f = fixture().await?;
    publish_only_confirmer(&f).await?;
    setting(&f, 0, "review", &["text", "image"], None, 0).await?;
    render(&f, 0).await?;
    let pending = prepare(&f, 0).await?;
    assert_eq!(pending["waiting"][0]["approval_kind"], "content_review");
    assert_eq!(pending["waiting"][0]["reviewer"], json!(f.stewards[0]));
    let material = artifacts(&f, 0).await?;
    // A valid upper confirmation for the first part cannot replace its content review.
    let upper_first = approve(&f, 0, &material[0], f.stewards[1]).await?;
    assert_eq!(upper_first["approval_kind"], "publish_confirmation");
    assert!(prepare(&f, 0).await?["actions"]
        .as_array()
        .unwrap()
        .is_empty());
    let own_first = approve(&f, 0, &material[0], f.stewards[0]).await?;
    assert_eq!(own_first["approval_kind"], "content_review");
    // A content review for the other part likewise cannot replace upper confirmation.
    let own_second = approve(&f, 0, &material[1], f.stewards[0]).await?;
    assert_eq!(own_second["approval_kind"], "content_review");
    let midway = prepare(&f, 0).await?;
    assert_eq!(midway["actions"].as_array().unwrap().len(), 1);
    assert_eq!(
        midway["waiting"][0]["approval_kind"],
        "publish_confirmation"
    );
    let upper_second = approve(&f, 0, &material[1], f.stewards[1]).await?;
    let ready = prepare(&f, 0).await?;
    assert_eq!(ready["actions"].as_array().unwrap().len(), 2);
    // Either independently revocable decision invalidates a prepared send.
    f.welcome
        .foundation_revoke_approval(
            &f.tenant,
            f.stewards[1],
            id(&upper_second["approval_ref"]),
            1,
        )
        .await?;
    f.welcome
        .foundation_revoke_approval(&f.tenant, f.stewards[0], id(&own_first["approval_ref"]), 1)
        .await?;
    for action in ready["actions"].as_array().unwrap() {
        assert!(f
            .welcome
            .foundation_execute(id(action), SyntheticOutcome::Success)
            .await
            .is_err());
    }
    assert_eq!(effects(&f).await?, 0);
    let blocked = prepare(&f, 0).await?;
    assert!(blocked["waiting"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["approval_kind"] == "content_review"));
    assert!(blocked["waiting"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["approval_kind"] == "publish_confirmation"));
    Ok(())
}

#[path = "steward_tests.rs"]
mod steward_tests;
