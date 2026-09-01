#![cfg(feature = "postgres-integration-tests")]

use chrono::{Duration, TimeZone, Utc};
use serde_json::{json, Value};
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::{
    automation_dispatcher::{self, DispatcherOptions},
    channel_event_mapping, db,
    event::RawQiweEvent,
    space_automation_execution,
    space_configuration::{
        self, ProgrammingExtensionRequest, ProgrammingExtensionResearchEvidence,
        TrustedSpaceSession,
    },
};

fn postgres_integration_database_url() -> String {
    assert_eq!(
        std::env::var("QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE").as_deref(),
        Ok("1"),
        "PostgreSQL integration test requires the explicit apply-smoke guard"
    );
    let database_url = std::env::var("QINTOPIA_SIDECAR_DATABASE_URL")
        .expect("PostgreSQL integration test requires QINTOPIA_SIDECAR_DATABASE_URL");
    let parsed = url::Url::parse(&database_url).expect("integration database URL must parse");
    assert!(
        matches!(parsed.scheme(), "postgres" | "postgresql"),
        "PostgreSQL integration test requires a postgres URL"
    );
    assert!(
        matches!(parsed.host_str(), Some("127.0.0.1" | "::1")),
        "PostgreSQL integration test may only use a literal loopback database"
    );
    assert_eq!(parsed.path().trim_start_matches('/'), "qintopia_test");
    database_url
}

fn session(chat_id: &str, user_id: &str, message_id: &str) -> TrustedSpaceSession {
    TrustedSpaceSession {
        platform: "qiwe".to_string(),
        conversation_type: "group".to_string(),
        conversation_id: chat_id.to_string(),
        requester_user_id: user_id.to_string(),
        source_message_id: message_id.to_string(),
        source_message_text: None,
    }
}

fn confirmation_session(
    mut trusted_session: TrustedSpaceSession,
    confirmation_code: &str,
) -> TrustedSpaceSession {
    trusted_session.source_message_text = Some(format!("确认 {confirmation_code}"));
    trusted_session
}

fn policy_intent(status: &str, marker: &str) -> Value {
    json!({
        "summary": format!("Set the current Space policy to {marker}."),
        "changes": [{
            "resource": "space_policy",
            "definition_key": "default",
            "status": status,
            "policy_config": {
                "capability_grants": [],
                "identity": marker
            }
        }]
    })
}

fn deterministic_schedule_intent(
    status: &str,
    definition_key: &str,
    approval_policy: &str,
) -> Value {
    json!({
        "summary": format!("Configure deterministic schedule {definition_key} as {status}."),
        "changes": [
            {
                "resource": "space_policy",
                "definition_key": "default",
                "status": "active",
                "policy_config": {
                    "capability_grants": ["erhua.qiwe_text_template"]
                }
            },
            {
                "resource": "business_definition",
                "definition_key": definition_key,
                "status": status,
                "execution_mode": "deterministic",
                "definition": {
                    "capability_key": "erhua.qiwe_text_template",
                    "input": {"text_template": "Scheduled integration message"}
                },
                "allowed_capabilities": ["erhua.qiwe_text_template"],
                "approval_policy": approval_policy
            },
            {
                "resource": "automation_definition",
                "definition_key": definition_key,
                "status": status,
                "business_definition_key": definition_key,
                "trigger_kind": "schedule",
                "trigger_config": {"cron": "* * * * *"},
                "timezone": "UTC",
                "misfire_policy": "run_once"
            }
        ]
    })
}

fn schedule_automation_only_intent(
    automation_key: &str,
    business_key: &str,
    status: &str,
) -> Value {
    json!({
        "summary": format!("Configure schedule {automation_key} as {status}."),
        "changes": [{
            "resource": "automation_definition",
            "definition_key": automation_key,
            "status": status,
            "business_definition_key": business_key,
            "trigger_kind": "schedule",
            "trigger_config": {"cron": "* * * * *"},
            "timezone": "UTC",
            "misfire_policy": "run_once"
        }]
    })
}

fn schedule_business_only_intent(status: &str, goal: &str) -> Value {
    json!({
        "summary": format!("Configure the schedule business as {status}."),
        "changes": [{
            "resource": "business_definition",
            "definition_key": "integration_schedule",
            "status": status,
            "execution_mode": "deterministic",
            "definition": {
                "capability_key": "erhua.qiwe_text_template",
                "input": {"text_template": goal}
            },
            "allowed_capabilities": ["erhua.qiwe_text_template"],
            "approval_policy": "space_admin_confirmation"
        }]
    })
}

fn automation_operation_intent(operation: &str, version: Option<i32>) -> Value {
    automation_operation_for_key_intent("integration_schedule", operation, version)
}

fn automation_operation_for_key_intent(
    definition_key: &str,
    operation: &str,
    version: Option<i32>,
) -> Value {
    let mut change = json!({
        "resource": "definition_operation",
        "target_resource": "automation_definition",
        "definition_key": definition_key,
        "operation": operation
    });
    if let Some(version) = version {
        change["version"] = json!(version);
    }
    json!({
        "summary": format!("{operation} the existing integration schedule."),
        "changes": [change]
    })
}

fn group_member_add_mapping(status: &str, definition_key: &str) -> Value {
    json!({
        "resource": "channel_event_mapping",
        "provider": "qiwe",
        "definition_key": definition_key,
        "status": status,
        "selector": {
            "op": "any",
            "rules": [
                {
                    "op": "all",
                    "rules": [
                        {"op": "equals", "pointer": "/newMsgType", "value": "GROUP_MEMBER_ADD"},
                        {"op": "in", "pointer": "/cmd", "values": [15000, 15500]}
                    ]
                },
                {
                    "op": "all",
                    "rules": [
                        {"op": "equals", "pointer": "/msgType", "value": 1002},
                        {"op": "exists", "pointer": "/newMsgType", "value": false},
                        {"op": "in", "pointer": "/cmd", "values": [15000, 15500]}
                    ]
                }
            ]
        },
        "extractor": {
            "event_type": "qiwe.group_member_added",
            "event_id": {
                "pointer": "/msgUniqueIdentifier",
                "transforms": [{"op": "opaque_id"}]
            },
            "space_chat_id": {
                "pointer": "/fromRoomId",
                "transforms": [{"op": "opaque_id"}]
            },
            "subject_user_ids": {
                "pointer": "/msgData/changedMemberList",
                "transforms": [
                    {"op": "base64_utf8"},
                    {"op": "split", "delimiter": ";", "max_parts": 64},
                    {"op": "opaque_id"},
                    {"op": "dedupe"}
                ]
            },
            "occurred_at": {
                "pointer": "/timestamp",
                "transforms": [{"op": "unix_timestamp"}]
            }
        },
        "official_sources": [
            "https://doc.qiweapi.com/doc-7331304",
            "https://doc.qiweapi.com/doc-9079960"
        ],
        "validation_evidence": {}
    })
}

fn mapping_only_intent(status: &str, mapping_key: &str) -> Value {
    json!({
        "summary": format!("Configure provider event mapping {mapping_key} as {status}."),
        "changes": [group_member_add_mapping(status, mapping_key)]
    })
}

fn event_automation_intent(status: &str, mapping_key: &str, automation_key: &str) -> Value {
    json!({
        "summary": format!("Configure the integration event automation as {status}."),
        "changes": [
            {
                "resource": "space_policy",
                "definition_key": "default",
                "status": "active",
                "policy_config": {
                    "capability_grants": ["erhua.qiwe_text_template"]
                }
            },
            group_member_add_mapping(status, mapping_key),
            {
                "resource": "business_definition",
                "definition_key": automation_key,
                "status": status,
                "execution_mode": "deterministic",
                "definition": {
                    "capability_key": "erhua.qiwe_text_template",
                    "input": {"text_template": "Welcome {{subject_names}}"}
                },
                "allowed_capabilities": ["erhua.qiwe_text_template"],
                "approval_policy": "space_admin_confirmation"
            },
            {
                "resource": "automation_definition",
                "definition_key": automation_key,
                "status": status,
                "business_definition_key": automation_key,
                "trigger_kind": "event",
                "trigger_config": {"batch_subjects": true},
                "event_mapping_provider": "qiwe",
                "event_mapping_key": mapping_key
            }
        ]
    })
}

fn shadow_event_automation_with_active_dependencies_intent(
    mapping_key: &str,
    automation_key: &str,
) -> Value {
    json!({
        "summary": "Create an active business with a shadow event automation.",
        "changes": [
            {
                "resource": "space_policy",
                "definition_key": "default",
                "status": "active",
                "policy_config": {
                    "capability_grants": ["erhua.qiwe_text_template"]
                }
            },
            {
                "resource": "business_definition",
                "definition_key": automation_key,
                "status": "active",
                "execution_mode": "deterministic",
                "definition": {
                    "capability_key": "erhua.qiwe_text_template",
                    "input": {"text_template": "Welcome {{subject_names}}"}
                },
                "allowed_capabilities": ["erhua.qiwe_text_template"],
                "approval_policy": "space_admin_confirmation"
            },
            {
                "resource": "automation_definition",
                "definition_key": automation_key,
                "status": "shadow",
                "business_definition_key": automation_key,
                "trigger_kind": "event",
                "trigger_config": {"batch_subjects": true},
                "event_mapping_provider": "qiwe",
                "event_mapping_key": mapping_key
            }
        ]
    })
}

fn event_automation_only_intent(status: &str, mapping_key: &str, automation_key: &str) -> Value {
    json!({
        "summary": format!("Configure event automation {automation_key} as {status}."),
        "changes": [{
            "resource": "automation_definition",
            "definition_key": automation_key,
            "status": status,
            "business_definition_key": automation_key,
            "trigger_kind": "event",
            "trigger_config": {"batch_subjects": true},
            "event_mapping_provider": "qiwe",
            "event_mapping_key": mapping_key
        }]
    })
}

fn response_uuid(value: &Value, key: &str) -> Uuid {
    Uuid::parse_str(value[key].as_str().expect("response UUID string"))
        .expect("response UUID value")
}

fn response_code(value: &Value) -> String {
    value["confirmation_code"]
        .as_str()
        .expect("confirmation code")
        .to_string()
}

async fn seed_actor(pool: &PgPool, person_id: Uuid, name: &str) {
    sqlx::query(
        r#"
        INSERT INTO qintopia_identity.persons (id, display_name, primary_name)
        VALUES ($1, $2, $2)
        "#,
    )
    .bind(person_id)
    .bind(name)
    .execute(pool)
    .await
    .expect("seed integration-test person");
}

async fn seed_channel_identity(pool: &PgPool, person_id: Uuid, user_id: &str, chat_id: &str) {
    sqlx::query(
        r#"
        INSERT INTO qintopia_identity.channel_identities
            (person_id, platform, channel_user_id, chat_id, display_name,
             normalized_display_name, identity_source, confidence)
        VALUES ($1, 'qiwe', $2, $3, $2, lower($2), 'integration_test', 1.0)
        "#,
    )
    .bind(person_id)
    .bind(user_id)
    .bind(chat_id)
    .execute(pool)
    .await
    .expect("seed integration-test channel identity");
}

async fn seed_shadow_observation(
    pool: &PgPool,
    space_id: Uuid,
    raw_event_id: Uuid,
    mapping_id: Uuid,
    scope: &str,
    idempotency_key: &str,
) {
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_items
            (space_id, work_item_type, status, requester_agent, target_agent,
             capability_key, human_owner, priority, available_at, brief_summary,
             purpose, source_type, source_refs, dedupe_key, idempotency_key,
             risk_level, information_class, payload, payload_redaction_policy,
             review_policy, metadata)
        VALUES
            ($1, 'space_event_shadow_observation', 'completed', 'system', 'erhua',
             'erhua.execute_space_business', '', 'low', now(),
             'Retained historical shadow evidence for activation-boundary testing.',
             'space_event_shadow', 'space_event_shadow',
             jsonb_build_object(
                 'raw_event_id', $2::uuid,
                 'mapping_version_id', $3::uuid
             ),
             $5, $5, 'low', 'internal_ops', '{"decode_success":true}'::jsonb,
             'summary_only', 'not_required',
             jsonb_build_object(
                 'external_send_executed', false,
                 'space_bound', true,
                 'scope', $4::text
             ))
        "#,
    )
    .bind(space_id)
    .bind(raw_event_id)
    .bind(mapping_id)
    .bind(scope)
    .bind(idempotency_key)
    .execute(pool)
    .await
    .expect("seed retained shadow observation");
}

async fn prepare_and_confirm(
    pool: &PgPool,
    trusted_session: TrustedSpaceSession,
    intent: Value,
) -> Value {
    let prepared = space_configuration::prepare(pool, trusted_session.clone(), intent)
        .await
        .expect("prepare Space change");
    let code = response_code(&prepared);
    space_configuration::confirm(
        pool,
        confirmation_session(trusted_session, &code),
        response_uuid(&prepared, "proposal_id"),
        code,
    )
    .await
    .expect("confirm Space change")
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
async fn postgres_space_control_plane_is_versioned_authorized_and_isolated() {
    let database_url = postgres_integration_database_url();
    let pool = db::connect(&database_url, 4)
        .await
        .expect("connect disposable PostgreSQL");
    db::run_migrations(&pool)
        .await
        .expect("migrate disposable PostgreSQL");

    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET enabled = true WHERE capability_key = 'erhua.manage_space_configuration'",
    )
    .execute(&pool)
    .await
    .expect("enable integration-test Space configuration capability");

    let suffix = Uuid::new_v4().simple().to_string();
    let chat_a = format!("integration-space-a-{suffix}");
    let chat_b = format!("integration-space-b-{suffix}");
    let owner_user = format!("integration-owner-{suffix}");
    let member_user = format!("integration-member-{suffix}");
    let owner_id = Uuid::new_v4();
    let member_id = Uuid::new_v4();

    seed_actor(&pool, owner_id, "Integration Owner").await;
    seed_actor(&pool, member_id, "Integration Member").await;
    for chat_id in [&chat_a, &chat_b] {
        seed_channel_identity(&pool, owner_id, &owner_user, chat_id).await;
    }
    seed_channel_identity(&pool, member_id, &member_user, &chat_a).await;
    sqlx::query(
        r#"
        INSERT INTO qintopia_identity.person_memberships
            (person_id, community_key, role, status, started_at)
        VALUES ($1, 'qintopia', 'owner', 'active', now())
        "#,
    )
    .bind(owner_id)
    .execute(&pool)
    .await
    .expect("seed integration-test global owner");

    let bootstrap_session = session(&chat_a, &owner_user, &format!("bootstrap-{suffix}"));
    let first_prepared = space_configuration::prepare(
        &pool,
        bootstrap_session.clone(),
        policy_intent("active", "v1"),
    )
    .await
    .expect("prepare bootstrap policy");
    let repeated = space_configuration::prepare(
        &pool,
        bootstrap_session.clone(),
        policy_intent("active", "v1"),
    )
    .await
    .expect("reissue idempotent bootstrap proposal");
    assert_eq!(repeated["deduped"], true);
    assert_eq!(repeated["request_id"], first_prepared["request_id"]);
    assert_eq!(repeated["proposal_id"], first_prepared["proposal_id"]);

    let bootstrap_code = response_code(&repeated);
    for message_text in [
        "好的".to_string(),
        format!("不要确认 {bootstrap_code}"),
        "确认 00000000".to_string(),
    ] {
        let mut non_confirmation_session = bootstrap_session.clone();
        non_confirmation_session.source_message_text = Some(message_text);
        let rejected = space_configuration::confirm(
            &pool,
            non_confirmation_session,
            response_uuid(&repeated, "proposal_id"),
            bootstrap_code.clone(),
        )
        .await
        .expect_err("only the exact current-message confirmation command may confirm");
        assert!(rejected
            .to_string()
            .contains("explicit confirmation command"));
    }
    let bootstrap_result = space_configuration::confirm(
        &pool,
        confirmation_session(bootstrap_session.clone(), &bootstrap_code),
        response_uuid(&repeated, "proposal_id"),
        bootstrap_code,
    )
    .await
    .expect("bootstrap current Space administrator");
    assert_eq!(bootstrap_result["status"], "completed");

    let space_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_messages.conversations WHERE tenant_id = 'qintopia' AND platform = 'qiwe' AND chat_id = $1",
    )
    .bind(&chat_a)
    .fetch_one(&pool)
    .await
    .expect("load integration-test Space");
    let is_bootstrapped_admin: bool = sqlx::query_scalar(
        r#"
        SELECT EXISTS (
            SELECT 1 FROM qintopia_identity.person_memberships
            WHERE person_id = $1 AND community_key = $2
              AND role = 'space_admin' AND status = 'active'
        )
        "#,
    )
    .bind(owner_id)
    .bind(format!("space:{space_id}"))
    .fetch_one(&pool)
    .await
    .expect("verify Space administrator bootstrap");
    assert!(is_bootstrapped_admin);

    let extension_session = session(
        &chat_a,
        &owner_user,
        &format!("programming-extension-{suffix}"),
    );
    let research_evidence = vec![ProgrammingExtensionResearchEvidence {
        url: "https://doc.qiweapi.com/doc-7331304".to_string(),
        text: "QiWe provider documentation for a bounded unknown event.".to_string(),
    }];
    let extension_request = ProgrammingExtensionRequest {
        intent: "Handle a QiWe provider event that is not in the registered catalog.".to_string(),
        provider: "qiwe".to_string(),
        research_query: "unknown QiWe provider event".to_string(),
        official_sources: vec!["https://doc.qiweapi.com/doc-7331304".to_string()],
        research_digest: space_configuration::programming_research_digest(&research_evidence),
        research_evidence,
    };
    let extension = space_configuration::prepare_programming_extension(
        &pool,
        extension_session.clone(),
        extension_request.clone(),
    )
    .await
    .expect("prepare programming extension work item");
    let repeated_extension = space_configuration::prepare_programming_extension(
        &pool,
        extension_session.clone(),
        extension_request,
    )
    .await
    .expect("dedupe programming extension work item");
    assert_eq!(
        extension["allowed_change_class"],
        "low_risk_declarative_mapping_bundle_only"
    );
    assert_eq!(repeated_extension["deduped"], true);
    assert_eq!(repeated_extension["request_id"], extension["request_id"]);
    let extension_request_id = response_uuid(&extension, "request_id");
    let extension_status =
        space_configuration::status(&pool, extension_session, extension_request_id)
            .await
            .expect("read current-Space programming extension status");
    assert_eq!(extension_status["programming_extension_required"], true);
    assert_eq!(
        extension_status["allowed_change_class"],
        "low_risk_declarative_mapping_bundle_only"
    );
    assert_eq!(extension_status["status"], "queued");
    let cross_space_extension = space_configuration::status(
        &pool,
        session(
            &chat_b,
            &owner_user,
            &format!("cross-space-extension-{suffix}"),
        ),
        extension_request_id,
    )
    .await
    .expect_err("programming extension status must not cross Space boundaries");
    assert!(cross_space_extension.to_string().contains("current Space"));

    let member_session = session(&chat_a, &member_user, &format!("member-proposal-{suffix}"));
    let member_proposal = space_configuration::prepare(
        &pool,
        member_session.clone(),
        policy_intent("draft", "member-proposal"),
    )
    .await
    .expect("ordinary current-Space member may propose a change");
    let member_code = response_code(&member_proposal);
    let unauthorized = space_configuration::confirm(
        &pool,
        confirmation_session(member_session, &member_code),
        response_uuid(&member_proposal, "proposal_id"),
        member_code,
    )
    .await
    .expect_err("ordinary member must not confirm a Space change");
    assert!(unauthorized.to_string().contains("not authorized"));

    let other_space_session = session(&chat_b, &owner_user, &format!("cross-space-{suffix}"));
    let cross_space = space_configuration::status(
        &pool,
        other_space_session,
        response_uuid(&member_proposal, "request_id"),
    )
    .await
    .expect_err("Space status must not cross conversation boundaries");
    assert!(cross_space.to_string().contains("current Space"));

    let pause_session = session(&chat_a, &owner_user, &format!("pause-{suffix}"));
    prepare_and_confirm(&pool, pause_session, policy_intent("paused", "paused-v2")).await;
    let restore_session = session(&chat_a, &owner_user, &format!("restore-{suffix}"));
    prepare_and_confirm(
        &pool,
        restore_session,
        policy_intent("active", "restored-v3"),
    )
    .await;

    let versions = sqlx::query(
        r#"
        SELECT version, status
        FROM qintopia_agent_os.space_policy_versions
        WHERE space_id = $1 AND definition_key = 'default'
        ORDER BY version
        "#,
    )
    .bind(space_id)
    .fetch_all(&pool)
    .await
    .expect("load Space policy versions");
    let active_count = versions
        .iter()
        .filter(|row| row.get::<String, _>("status") == "active")
        .count();
    assert_eq!(active_count, 1);
    assert_eq!(
        versions.last().unwrap().get::<String, _>("status"),
        "active"
    );
    assert!(versions.len() >= 3);

    let schedule_session = session(&chat_a, &owner_user, &format!("schedule-{suffix}"));
    prepare_and_confirm(
        &pool,
        schedule_session,
        deterministic_schedule_intent("active", "integration_schedule", "space_admin_confirmation"),
    )
    .await;
    let automation_id: Uuid = sqlx::query_scalar(
        r#"
        SELECT id FROM qintopia_agent_os.automation_definition_versions
        WHERE space_id = $1 AND definition_key = 'integration_schedule'
          AND status = 'active'
        "#,
    )
    .bind(space_id)
    .fetch_one(&pool)
    .await
    .expect("load active integration-test schedule");
    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET enabled = true WHERE capability_key IN ('erhua.execute_space_business', 'erhua.qiwe_text_template')",
    )
    .execute(&pool)
    .await
    .expect("enable integration-test Space execution capabilities");
    let scheduled_for = Utc.with_ymd_and_hms(2026, 8, 14, 1, 2, 0).unwrap();
    let dispatch_now = Utc.with_ymd_and_hms(2026, 8, 14, 1, 5, 0).unwrap();
    let dispatcher_options = DispatcherOptions {
        once: true,
        apply: true,
        dry_run: false,
        batch_size: 100,
        poll_seconds: 60,
    };
    for _ in 0..2 {
        sqlx::query(
            r#"
            UPDATE qintopia_agent_os.automation_definition_versions
            SET next_run_at = $2, last_dispatched_at = NULL
            WHERE id = $1
            "#,
        )
        .bind(automation_id)
        .bind(scheduled_for)
        .execute(&pool)
        .await
        .expect("reset integration-test dispatcher cursor");
        automation_dispatcher::dispatch_once_for_integration_test(
            &pool,
            dispatch_now,
            &dispatcher_options,
        )
        .await
        .expect("dispatch due integration-test schedule");
    }
    let scheduled_runs: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*) FROM qintopia_agent_os.work_items
        WHERE work_item_type = 'space_automation_run'
          AND space_id = $1
          AND payload->>'automation_definition_id' = $2
          AND idempotency_key = $3
        "#,
    )
    .bind(space_id)
    .bind(automation_id.to_string())
    .bind(format!(
        "automation:{automation_id}:{}",
        scheduled_for.format("%Y-%m-%dT%H:%M:00Z")
    ))
    .fetch_one(&pool)
    .await
    .expect("count idempotent scheduled work items");
    assert_eq!(scheduled_runs, 1);

    let stale_dependency_session =
        session(&chat_a, &owner_user, &format!("stale-dependency-{suffix}"));
    let stale_dependency_proposal = space_configuration::prepare(
        &pool,
        stale_dependency_session.clone(),
        schedule_automation_only_intent(
            "integration_schedule_secondary",
            "integration_schedule",
            "active",
        ),
    )
    .await
    .expect("prepare automation against the exact active business head");
    prepare_and_confirm(
        &pool,
        session(&chat_a, &owner_user, &format!("business-draft-{suffix}")),
        schedule_business_only_intent(
            "draft",
            "Produce a changed bounded integration-test result.",
        ),
    )
    .await;
    let stale_code = response_code(&stale_dependency_proposal);
    let stale_dependency = space_configuration::confirm(
        &pool,
        confirmation_session(stale_dependency_session, &stale_code),
        response_uuid(&stale_dependency_proposal, "proposal_id"),
        stale_code,
    )
    .await
    .expect_err("automation proposal must reject a changed dependency stream head");
    assert!(stale_dependency
        .to_string()
        .contains("changed after prepare"));

    let unpaired_business_replacement = space_configuration::prepare(
        &pool,
        session(&chat_a, &owner_user, &format!("unpaired-business-{suffix}")),
        schedule_business_only_intent(
            "active",
            "Produce an unpaired replacement integration-test result.",
        ),
    )
    .await
    .expect_err("active business replacement must migrate or pause dependent automations");
    assert!(unpaired_business_replacement
        .to_string()
        .contains("still references it"));

    let business_id: Uuid = sqlx::query_scalar(
        r#"
        SELECT id FROM qintopia_agent_os.business_definition_versions
        WHERE space_id = $1 AND definition_key = 'integration_schedule'
          AND status = 'active'
        "#,
    )
    .bind(space_id)
    .fetch_one(&pool)
    .await
    .expect("load exact integration schedule business");
    prepare_and_confirm(
        &pool,
        session(
            &chat_b,
            &owner_user,
            &format!("second-space-bootstrap-{suffix}"),
        ),
        policy_intent("active", "second-space-bootstrap"),
    )
    .await;
    let space_b_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_messages.conversations WHERE tenant_id = 'qintopia' AND platform = 'qiwe' AND chat_id = $1",
    )
    .bind(&chat_b)
    .fetch_one(&pool)
    .await
    .expect("load second integration-test Space");
    let cross_space_insert = sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.automation_definition_versions
            (space_id, definition_key, version, business_definition_id,
             trigger_kind, trigger_config, timezone, misfire_policy, status,
             definition_digest, created_by_person_id)
        VALUES ($1, $2, 1, $3, 'schedule', '{"cron":"* * * * *"}'::jsonb,
                'UTC', 'run_once', 'draft', $4, $5)
        "#,
    )
    .bind(space_b_id)
    .bind(format!("cross_space_bad_{suffix}"))
    .bind(business_id)
    .bind("c".repeat(64))
    .bind(owner_id)
    .execute(&pool)
    .await
    .expect_err("database must reject an automation/business cross-Space reference");
    assert!(cross_space_insert
        .to_string()
        .contains("automation_definition_versions_business_space_fk"));

    let policy_id: Uuid = sqlx::query_scalar(
        r#"
        SELECT id FROM qintopia_agent_os.space_policy_versions
        WHERE space_id = $1 AND definition_key = 'default' AND status = 'active'
        "#,
    )
    .bind(space_id)
    .fetch_one(&pool)
    .await
    .expect("load active integration-test policy");
    for (gate_table, gate_id) in [
        ("business_definition_versions", business_id),
        ("space_policy_versions", policy_id),
    ] {
        let pause_sql =
            format!("UPDATE qintopia_agent_os.{gate_table} SET status = 'paused' WHERE id = $1");
        sqlx::query(&pause_sql)
            .bind(gate_id)
            .execute(&pool)
            .await
            .expect("pause dispatcher dependency gate");
        sqlx::query(
            r#"
            UPDATE qintopia_agent_os.automation_definition_versions
            SET next_run_at = $2, last_dispatched_at = NULL
            WHERE id = $1
            "#,
        )
        .bind(automation_id)
        .bind(scheduled_for + Duration::minutes(1))
        .execute(&pool)
        .await
        .expect("make automation due behind an invalid dependency gate");
        automation_dispatcher::dispatch_once_for_integration_test(
            &pool,
            dispatch_now + Duration::minutes(1),
            &dispatcher_options,
        )
        .await
        .expect("dispatcher must skip invalid dependency gate");
        let run_count: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM qintopia_agent_os.work_items
            WHERE work_item_type = 'space_automation_run'
              AND space_id = $1
              AND payload->>'automation_definition_id' = $2
            "#,
        )
        .bind(space_id)
        .bind(automation_id.to_string())
        .fetch_one(&pool)
        .await
        .expect("count runs behind invalid dispatcher dependency gate");
        assert_eq!(run_count, 1);
        let restore_sql =
            format!("UPDATE qintopia_agent_os.{gate_table} SET status = 'active' WHERE id = $1");
        sqlx::query(&restore_sql)
            .bind(gate_id)
            .execute(&pool)
            .await
            .expect("restore dispatcher dependency gate");
    }

    for (disable_sql, restore_sql) in [
        (
            "UPDATE qintopia_agent_os.capabilities SET enabled = false WHERE capability_key = 'erhua.qiwe_text_template'",
            "UPDATE qintopia_agent_os.capabilities SET enabled = true WHERE capability_key = 'erhua.qiwe_text_template'",
        ),
        (
            "UPDATE qintopia_agent_os.capabilities SET allowed_callers = ARRAY[]::text[] WHERE capability_key = 'erhua.qiwe_text_template'",
            "UPDATE qintopia_agent_os.capabilities SET allowed_callers = ARRAY['system']::text[] WHERE capability_key = 'erhua.qiwe_text_template'",
        ),
        (
            "UPDATE qintopia_agent_os.capabilities SET allowed_work_item_types = ARRAY[]::text[] WHERE capability_key = 'erhua.qiwe_text_template'",
            "UPDATE qintopia_agent_os.capabilities SET allowed_work_item_types = ARRAY['space_automation_run']::text[] WHERE capability_key = 'erhua.qiwe_text_template'",
        ),
        (
            "UPDATE qintopia_agent_os.capabilities SET metadata = jsonb_set(metadata, '{space_invocable}', 'false'::jsonb) WHERE capability_key = 'erhua.qiwe_text_template'",
            "UPDATE qintopia_agent_os.capabilities SET metadata = jsonb_set(metadata, '{space_invocable}', 'true'::jsonb) WHERE capability_key = 'erhua.qiwe_text_template'",
        ),
    ] {
        sqlx::query(disable_sql)
            .execute(&pool)
            .await
            .expect("invalidate one selected-capability dispatcher gate");
        sqlx::query(
            r#"
            UPDATE qintopia_agent_os.automation_definition_versions
            SET next_run_at = $2, last_dispatched_at = NULL
            WHERE id = $1
            "#,
        )
        .bind(automation_id)
        .bind(scheduled_for + Duration::minutes(1))
        .execute(&pool)
        .await
        .expect("make automation due behind an invalid selected-capability gate");
        automation_dispatcher::dispatch_once_for_integration_test(
            &pool,
            dispatch_now + Duration::minutes(1),
            &dispatcher_options,
        )
        .await
        .expect("dispatcher must skip an invalid selected-capability gate");
        let invalid_capability_run_count: i64 = sqlx::query_scalar(
            r#"
            SELECT count(*) FROM qintopia_agent_os.work_items
            WHERE work_item_type = 'space_automation_run'
              AND space_id = $1
              AND payload->>'automation_definition_id' = $2
            "#,
        )
        .bind(space_id)
        .bind(automation_id.to_string())
        .fetch_one(&pool)
        .await
        .expect("count runs behind an invalid selected-capability gate");
        assert_eq!(invalid_capability_run_count, 1);
        sqlx::query(restore_sql)
            .execute(&pool)
            .await
            .expect("restore the selected-capability dispatcher gate");
    }

    let stale_pause_session = session(
        &chat_a,
        &owner_user,
        &format!("stale-pause-automation-{suffix}"),
    );
    let stale_pause = space_configuration::prepare(
        &pool,
        stale_pause_session.clone(),
        automation_operation_intent("pause", None),
    )
    .await
    .expect("prepare pause against the exact automation stream head");
    prepare_and_confirm(
        &pool,
        session(
            &chat_a,
            &owner_user,
            &format!("new-automation-head-{suffix}"),
        ),
        schedule_automation_only_intent("integration_schedule", "integration_schedule", "draft"),
    )
    .await;
    let stale_pause_code = response_code(&stale_pause);
    let stale_pause_result = space_configuration::confirm(
        &pool,
        confirmation_session(stale_pause_session, &stale_pause_code),
        response_uuid(&stale_pause, "proposal_id"),
        stale_pause_code,
    )
    .await
    .expect_err("pause proposal must reject a changed automation stream head");
    assert!(stale_pause_result
        .to_string()
        .contains("changed after prepare"));

    prepare_and_confirm(
        &pool,
        session(&chat_a, &owner_user, &format!("pause-automation-{suffix}")),
        automation_operation_intent("pause", None),
    )
    .await;
    let paused_versions: Vec<(i32, String)> = sqlx::query_as(
        r#"
        SELECT version, status
        FROM qintopia_agent_os.automation_definition_versions
        WHERE space_id = $1 AND definition_key = 'integration_schedule'
        ORDER BY version
        "#,
    )
    .bind(space_id)
    .fetch_all(&pool)
    .await
    .expect("load paused automation definition history");
    assert_eq!(
        paused_versions,
        vec![
            (1, "retired".to_string()),
            (2, "draft".to_string()),
            (3, "paused".to_string())
        ]
    );

    prepare_and_confirm(
        &pool,
        session(
            &chat_a,
            &owner_user,
            &format!("rollback-automation-{suffix}"),
        ),
        automation_operation_intent("rollback", Some(1)),
    )
    .await;
    let restored_versions: Vec<(i32, String)> = sqlx::query_as(
        r#"
        SELECT version, status
        FROM qintopia_agent_os.automation_definition_versions
        WHERE space_id = $1 AND definition_key = 'integration_schedule'
        ORDER BY version
        "#,
    )
    .bind(space_id)
    .fetch_all(&pool)
    .await
    .expect("load rolled-back automation definition history");
    assert_eq!(
        restored_versions,
        vec![
            (1, "retired".to_string()),
            (2, "draft".to_string()),
            (3, "paused".to_string()),
            (4, "active".to_string())
        ]
    );

    let expiry_session = session(&chat_a, &owner_user, &format!("expiry-{suffix}"));
    let expiring = space_configuration::prepare(
        &pool,
        expiry_session.clone(),
        policy_intent("draft", "expiry"),
    )
    .await
    .expect("prepare expiring proposal");
    let expiring_proposal_id = response_uuid(&expiring, "proposal_id");
    let mut expiring_metadata: Value =
        sqlx::query_scalar("SELECT metadata FROM qintopia_agent_os.artifacts WHERE id = $1")
            .bind(expiring_proposal_id)
            .fetch_one(&pool)
            .await
            .expect("load expiring proposal metadata");
    expiring_metadata["confirmation"]["expires_at"] = json!(Utc::now() - Duration::minutes(1));
    sqlx::query("UPDATE qintopia_agent_os.artifacts SET metadata = $2 WHERE id = $1")
        .bind(expiring_proposal_id)
        .bind(expiring_metadata)
        .execute(&pool)
        .await
        .expect("expire proposal confirmation binding");
    let expiring_code = response_code(&expiring);
    let expired = space_configuration::confirm(
        &pool,
        confirmation_session(expiry_session, &expiring_code),
        expiring_proposal_id,
        expiring_code,
    )
    .await
    .expect_err("expired confirmation code must fail");
    assert!(expired.to_string().contains("expired"));

    let attempts_session = session(&chat_a, &owner_user, &format!("attempts-{suffix}"));
    let attempts = space_configuration::prepare(
        &pool,
        attempts_session.clone(),
        policy_intent("draft", "attempt-limit"),
    )
    .await
    .expect("prepare attempt-limited proposal");
    let attempts_proposal_id = response_uuid(&attempts, "proposal_id");
    let wrong_code = if response_code(&attempts) == "00000000" {
        "11111111"
    } else {
        "00000000"
    };
    for _ in 0..5 {
        let denied = space_configuration::confirm(
            &pool,
            confirmation_session(attempts_session.clone(), wrong_code),
            attempts_proposal_id,
            wrong_code.to_string(),
        )
        .await
        .expect_err("invalid confirmation code must fail");
        assert!(denied.to_string().contains("invalid"));
    }
    let attempts_code = response_code(&attempts);
    let exhausted = space_configuration::confirm(
        &pool,
        confirmation_session(attempts_session.clone(), &attempts_code),
        attempts_proposal_id,
        attempts_code,
    )
    .await
    .expect_err("exhausted confirmation binding must remain denied");
    assert!(exhausted.to_string().contains("attempt limit"));
    let attempts_status = space_configuration::status(
        &pool,
        attempts_session,
        response_uuid(&attempts, "request_id"),
    )
    .await
    .expect("read exhausted proposal status");
    assert_eq!(attempts_status["confirmation"]["attempts_remaining"], 0);
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
async fn postgres_event_automation_requires_same_space_shadow_before_activation() {
    let database_url = postgres_integration_database_url();
    let pool = db::connect(&database_url, 4)
        .await
        .expect("connect disposable PostgreSQL");
    db::run_migrations(&pool)
        .await
        .expect("migrate disposable PostgreSQL");
    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET enabled = true WHERE capability_key = 'erhua.manage_space_configuration'",
    )
    .execute(&pool)
    .await
    .expect("enable integration-test Space configuration capability");

    let suffix = Uuid::new_v4().simple().to_string();
    let chat_id = format!("integration-event-space-{suffix}");
    let owner_user = format!("integration-event-owner-{suffix}");
    let mapping_key = format!("integration_group_member_add_{suffix}");
    let automation_key = format!("integration_member_greeting_{suffix}");
    let owner_id = Uuid::new_v4();
    seed_actor(&pool, owner_id, "Integration Event Owner").await;
    seed_channel_identity(&pool, owner_id, &owner_user, &chat_id).await;
    sqlx::query(
        r#"
        INSERT INTO qintopia_identity.person_memberships
            (person_id, community_key, role, status, started_at)
        VALUES ($1, 'qintopia', 'owner', 'active', now())
        "#,
    )
    .bind(owner_id)
    .execute(&pool)
    .await
    .expect("seed integration-test global owner");

    let historical_event = RawQiweEvent {
        event_id: format!("integration-historical-event-{suffix}"),
        received_at: Utc::now(),
        source: "qiwe".to_string(),
        ingress_auth_verified: true,
        payload: json!({
            "data": [{
                "cmd": 15500,
                "newMsgType": "GROUP_MEMBER_ADD",
                "msgUniqueIdentifier": format!("integration-historical-provider-event-{suffix}"),
                "fromRoomId": chat_id,
                "timestamp": Utc::now().timestamp(),
                "msgData": {"changedMemberList": "bWVtYmVyLWhpc3RvcmljYWw="}
            }]
        }),
    };
    let historical_raw_event_id =
        db::persist_raw_event(&pool, "qintopia.qiwe.raw", &historical_event)
            .await
            .expect("persist authenticated event before shadow definitions");

    let shadow_session = session(&chat_id, &owner_user, &format!("event-shadow-{suffix}"));
    prepare_and_confirm(
        &pool,
        shadow_session,
        event_automation_intent("shadow", &mapping_key, &automation_key),
    )
    .await;
    let space_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_messages.conversations WHERE tenant_id = 'qintopia' AND platform = 'qiwe' AND chat_id = $1",
    )
    .bind(&chat_id)
    .fetch_one(&pool)
    .await
    .expect("load event-test Space");
    let shadow_mapping_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_agent_os.channel_event_mapping_versions WHERE provider = 'qiwe' AND definition_key = $1 AND status = 'shadow'",
    )
    .bind(&mapping_key)
    .fetch_one(&pool)
    .await
    .expect("load shadow event mapping");
    let shadow_automation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_agent_os.automation_definition_versions WHERE space_id = $1 AND definition_key = $2 AND status = 'shadow'",
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("load shadow event automation");

    let premature_session = session(
        &chat_id,
        &owner_user,
        &format!("event-premature-active-{suffix}"),
    );
    let premature = space_configuration::prepare(
        &pool,
        premature_session,
        automation_operation_for_key_intent(&automation_key, "activate", None),
    )
    .await
    .expect_err("event automation activation must fail without exact shadow evidence");
    assert!(premature
        .to_string()
        .contains("exact current-Space shadow version"));

    let replayed_historical_raw_event_id =
        db::persist_raw_event(&pool, "qintopia.qiwe.raw", &historical_event)
            .await
            .expect("replay authenticated event after shadow definitions");
    assert_eq!(replayed_historical_raw_event_id, historical_raw_event_id);
    let historical_precedes_definitions: bool = sqlx::query_scalar(
        r#"
        SELECT raw_event.created_at <= mapping.created_at
           AND raw_event.created_at <= automation.created_at
        FROM qintopia_messages.raw_events raw_event
        JOIN qintopia_agent_os.channel_event_mapping_versions mapping ON mapping.id = $2
        JOIN qintopia_agent_os.automation_definition_versions automation ON automation.id = $3
        WHERE raw_event.id = $1
        "#,
    )
    .bind(historical_raw_event_id)
    .bind(shadow_mapping_id)
    .bind(shadow_automation_id)
    .fetch_one(&pool)
    .await
    .expect("verify historical event predates shadow definitions");
    assert!(historical_precedes_definitions);
    let replayed_historical_event = db::load_raw_event(&pool, replayed_historical_raw_event_id)
        .await
        .expect("load replayed historical event");
    channel_event_mapping::process_persisted_raw_event(
        &pool,
        replayed_historical_raw_event_id,
        &replayed_historical_event,
    )
    .await
    .expect("ignore historical event for new shadow definitions");
    let historical_observation_count: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM qintopia_agent_os.work_items
        WHERE space_id = $1
          AND work_item_type = 'space_event_shadow_observation'
          AND source_refs ->> 'mapping_version_id' = $2
        "#,
    )
    .bind(space_id)
    .bind(shadow_mapping_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("count observations after replaying a historical event");
    assert_eq!(
        historical_observation_count, 0,
        "an event persisted before the exact mapping and automation versions cannot satisfy shadow"
    );
    sqlx::query(
        r#"
        INSERT INTO qintopia_agent_os.work_items
            (space_id, work_item_type, status, requester_agent, target_agent,
             capability_key, human_owner, priority, available_at, brief_summary,
             purpose, source_type, source_refs, dedupe_key, idempotency_key,
             risk_level, information_class, payload, payload_redaction_policy,
             review_policy, metadata)
        VALUES
            ($1, 'space_event_shadow_observation', 'completed', 'system', 'erhua',
             'erhua.execute_space_business', '', 'low', now(),
             'Retained historical shadow evidence for activation-boundary testing.',
             'space_event_shadow', 'space_event_shadow',
             jsonb_build_object(
                 'raw_event_id', $2::uuid,
                 'mapping_version_id', $3::uuid
             ),
             $5, $5, 'low', 'internal_ops', '{"decode_success":true}'::jsonb,
             'summary_only', 'not_required',
             jsonb_build_object(
                 'external_send_executed', false,
                 'space_bound', true,
                 'scope', 'automation_shadow:' || $4::uuid::text
             ))
        "#,
    )
    .bind(space_id)
    .bind(historical_raw_event_id)
    .bind(shadow_mapping_id)
    .bind(shadow_automation_id)
    .bind(format!("integration-stale-shadow-evidence-{suffix}"))
    .execute(&pool)
    .await
    .expect("seed retained historical observation for activation-boundary regression");
    let historical_activation = space_configuration::prepare(
        &pool,
        session(
            &chat_id,
            &owner_user,
            &format!("event-historical-active-{suffix}"),
        ),
        automation_operation_for_key_intent(&automation_key, "activate", None),
    )
    .await
    .expect_err("historical raw-event evidence must not authorize activation");
    assert!(historical_activation
        .to_string()
        .contains("exact current-Space shadow version"));

    let event = RawQiweEvent {
        event_id: format!("integration-fresh-event-{suffix}"),
        received_at: Utc::now(),
        source: "qiwe".to_string(),
        ingress_auth_verified: true,
        payload: json!({
            "data": [{
                "cmd": 15500,
                "newMsgType": "GROUP_MEMBER_ADD",
                "msgUniqueIdentifier": format!("integration-fresh-provider-event-{suffix}"),
                "fromRoomId": chat_id,
                "timestamp": Utc::now().timestamp(),
                "msgData": {"changedMemberList": "bWVtYmVyLWE="}
            }]
        }),
    };
    let raw_event_id = db::persist_raw_event(&pool, "qintopia.qiwe.raw", &event)
        .await
        .expect("persist authenticated integration event");
    let persisted_event = db::load_raw_event(&pool, raw_event_id)
        .await
        .expect("load authenticated integration event");
    channel_event_mapping::process_persisted_raw_event(&pool, raw_event_id, &persisted_event)
        .await
        .expect("record mapping and automation shadow observations");
    let observation_scopes: Vec<String> = sqlx::query_scalar(
        r#"
        SELECT metadata ->> 'scope'
        FROM qintopia_agent_os.work_items
        WHERE space_id = $1
          AND work_item_type = 'space_event_shadow_observation'
          AND source_refs ->> 'mapping_version_id' = $2
        ORDER BY metadata ->> 'scope'
        "#,
    )
    .bind(space_id)
    .bind(shadow_mapping_id.to_string())
    .fetch_all(&pool)
    .await
    .expect("load shadow observation scopes");
    assert!(observation_scopes.contains(&"mapping_shadow".to_string()));
    assert!(observation_scopes.contains(&format!("automation_shadow:{shadow_automation_id}")));

    let active_session = session(&chat_id, &owner_user, &format!("event-active-{suffix}"));
    let activated = prepare_and_confirm(
        &pool,
        active_session,
        automation_operation_for_key_intent(&automation_key, "activate", None),
    )
    .await;
    assert_eq!(activated["status"], "completed");
    let source_mapping_status: String = sqlx::query_scalar(
        "SELECT status FROM qintopia_agent_os.channel_event_mapping_versions WHERE id = $1",
    )
    .bind(shadow_mapping_id)
    .fetch_one(&pool)
    .await
    .expect("load source event mapping status after exact activation");
    assert_eq!(
        source_mapping_status, "shadow",
        "exact activation creates a new active mapping and retains the observed source mapping"
    );
    let active_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.automation_definition_versions WHERE space_id = $1 AND definition_key = $2 AND status = 'active'",
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("count active event automation versions");
    assert_eq!(active_count, 1);

    let activated_version: i32 = sqlx::query_scalar(
        "SELECT version FROM qintopia_agent_os.automation_definition_versions WHERE space_id = $1 AND definition_key = $2 AND status = 'active'",
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("load activated event automation version");
    prepare_and_confirm(
        &pool,
        session(&chat_id, &owner_user, &format!("event-pause-{suffix}")),
        automation_operation_for_key_intent(&automation_key, "pause", None),
    )
    .await;
    let paused_active_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.automation_definition_versions WHERE space_id = $1 AND definition_key = $2 AND status = 'active'",
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("count active event automations after pause");
    assert_eq!(paused_active_count, 0);
    let rolled_back = prepare_and_confirm(
        &pool,
        session(&chat_id, &owner_user, &format!("event-rollback-{suffix}")),
        automation_operation_for_key_intent(&automation_key, "rollback", Some(activated_version)),
    )
    .await;
    assert_eq!(rolled_back["status"], "completed");
    let rollback_active_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.automation_definition_versions WHERE space_id = $1 AND definition_key = $2 AND status = 'active'",
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("count active event automations after rollback");
    assert_eq!(rollback_active_count, 1);

    let (first_restored_id, first_restored_version, first_lineage_event_id, first_lineage): (
        Uuid,
        i32,
        Uuid,
        Value,
    ) = sqlx::query_as(
        r#"
        SELECT restored.id, restored.version, lineage.id, lineage.data
        FROM qintopia_agent_os.automation_definition_versions restored
        JOIN qintopia_agent_os.work_item_events lineage
          ON lineage.work_item_id = restored.created_from_work_item_id
         AND lineage.event_type = 'automation_rollback_lineage_recorded'
         AND lineage.data ->> 'automation_definition_id' = restored.id::text
        WHERE restored.space_id = $1
          AND restored.definition_key = $2
          AND restored.status = 'active'
        "#,
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("load first restored automation rollback lineage");
    assert!(first_restored_version > activated_version);
    assert_eq!(
        first_lineage["shadow_automation_definition_id"],
        json!(shadow_automation_id)
    );
    assert_eq!(
        first_lineage["source_mapping_version_id"],
        json!(shadow_mapping_id)
    );
    assert_eq!(first_lineage["raw_event_id"], json!(raw_event_id));

    prepare_and_confirm(
        &pool,
        session(
            &chat_id,
            &owner_user,
            &format!("event-repeat-pause-{suffix}"),
        ),
        automation_operation_for_key_intent(&automation_key, "pause", None),
    )
    .await;
    let repeated_rollback = prepare_and_confirm(
        &pool,
        session(
            &chat_id,
            &owner_user,
            &format!("event-repeat-rollback-{suffix}"),
        ),
        automation_operation_for_key_intent(
            &automation_key,
            "rollback",
            Some(first_restored_version),
        ),
    )
    .await;
    assert_eq!(repeated_rollback["status"], "completed");
    let (
        repeated_restored_id,
        repeated_restored_version,
        repeated_lineage_event_id,
        repeated_lineage,
    ): (Uuid, i32, Uuid, Value) = sqlx::query_as(
        r#"
        SELECT restored.id, restored.version, lineage.id, lineage.data
        FROM qintopia_agent_os.automation_definition_versions restored
        JOIN qintopia_agent_os.work_item_events lineage
          ON lineage.work_item_id = restored.created_from_work_item_id
         AND lineage.event_type = 'automation_rollback_lineage_recorded'
         AND lineage.data ->> 'automation_definition_id' = restored.id::text
        WHERE restored.space_id = $1
          AND restored.definition_key = $2
          AND restored.status = 'active'
        "#,
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("load repeatedly restored automation rollback lineage");
    assert!(repeated_restored_version > first_restored_version);
    assert_ne!(repeated_restored_id, first_restored_id);
    assert_ne!(repeated_lineage_event_id, first_lineage_event_id);
    for evidence_key in [
        "shadow_automation_definition_id",
        "source_mapping_version_id",
        "observation_work_item_id",
        "raw_event_id",
    ] {
        assert_eq!(repeated_lineage[evidence_key], first_lineage[evidence_key]);
    }

    channel_event_mapping::process_persisted_raw_event(&pool, raw_event_id, &persisted_event)
        .await
        .expect("replay activation shadow event without backfilling an active run");
    let shadow_replay_run_count: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM qintopia_agent_os.work_items
        WHERE space_id = $1
          AND work_item_type = 'space_automation_run'
          AND payload ->> 'automation_key' = $2
        "#,
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("count active runs after replaying activation shadow evidence");
    assert_eq!(
        shadow_replay_run_count, 0,
        "an event observed in shadow must never backfill after activation"
    );
    let shadow_replay_send_count: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM qintopia_agent_os.work_item_events work_event
        JOIN qintopia_agent_os.work_items work_item
          ON work_item.id = work_event.work_item_id
        WHERE work_item.space_id = $1
          AND work_item.work_item_type = 'space_automation_run'
          AND work_item.payload ->> 'automation_key' = $2
          AND work_event.event_type = 'space_automation_send_committed'
        "#,
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("count sends after replaying activation shadow evidence");
    assert_eq!(
        shadow_replay_send_count, 0,
        "an event observed in shadow must never reach the send boundary"
    );

    let mapping_version_count_before_space_b: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.channel_event_mapping_versions WHERE provider = 'qiwe' AND definition_key = $1",
    )
    .bind(&mapping_key)
    .fetch_one(&pool)
    .await
    .expect("count reusable provider mapping versions");
    let chat_b = format!("integration-event-space-b-{suffix}");
    let automation_key_b = format!("integration_member_greeting_b_{suffix}");
    seed_channel_identity(&pool, owner_id, &owner_user, &chat_b).await;
    prepare_and_confirm(
        &pool,
        session(&chat_b, &owner_user, &format!("event-shadow-b-{suffix}")),
        event_automation_intent("shadow", &mapping_key, &automation_key_b),
    )
    .await;
    let space_b_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_messages.conversations WHERE tenant_id = 'qintopia' AND platform = 'qiwe' AND chat_id = $1",
    )
    .bind(&chat_b)
    .fetch_one(&pool)
    .await
    .expect("load second event-test Space");
    let shadow_automation_b_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_agent_os.automation_definition_versions WHERE space_id = $1 AND definition_key = $2 AND status = 'shadow'",
    )
    .bind(space_b_id)
    .bind(&automation_key_b)
    .fetch_one(&pool)
    .await
    .expect("load second-Space shadow automation for lineage isolation testing");

    prepare_and_confirm(
        &pool,
        session(
            &chat_id,
            &owner_user,
            &format!("event-lineage-pause-{suffix}"),
        ),
        automation_operation_for_key_intent(&automation_key, "pause", None),
    )
    .await;
    let active_after_lineage_pause: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.automation_definition_versions WHERE space_id = $1 AND definition_key = $2 AND status = 'active'",
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("count active automations after lineage adversarial pause");
    assert_eq!(active_after_lineage_pause, 0);

    let mut forged_raw_event_lineage = repeated_lineage.clone();
    forged_raw_event_lineage["raw_event_id"] = json!(Uuid::new_v4());
    sqlx::query("UPDATE qintopia_agent_os.work_item_events SET data = $2 WHERE id = $1")
        .bind(repeated_lineage_event_id)
        .bind(&forged_raw_event_lineage)
        .execute(&pool)
        .await
        .expect("forge rollback raw-event lineage for rejection testing");
    let forged_raw_session = session(
        &chat_id,
        &owner_user,
        &format!("event-forged-raw-lineage-{suffix}"),
    );
    let forged_raw_proposal = space_configuration::prepare(
        &pool,
        forged_raw_session.clone(),
        automation_operation_for_key_intent(
            &automation_key,
            "rollback",
            Some(repeated_restored_version),
        ),
    )
    .await
    .expect("prepare forged raw-event lineage rollback proposal");
    let forged_raw_code = response_code(&forged_raw_proposal);
    let forged_raw_rejection = space_configuration::confirm(
        &pool,
        confirmation_session(forged_raw_session, &forged_raw_code),
        response_uuid(&forged_raw_proposal, "proposal_id"),
        forged_raw_code,
    )
    .await
    .expect_err("forged raw-event lineage must not authorize rollback");
    assert!(forged_raw_rejection
        .to_string()
        .contains("exact historical active definition"));

    let mut cross_space_lineage = repeated_lineage.clone();
    cross_space_lineage["shadow_automation_definition_id"] = json!(shadow_automation_b_id);
    sqlx::query("UPDATE qintopia_agent_os.work_item_events SET data = $2 WHERE id = $1")
        .bind(repeated_lineage_event_id)
        .bind(&cross_space_lineage)
        .execute(&pool)
        .await
        .expect("forge cross-Space rollback lineage for rejection testing");
    let cross_space_lineage_session = session(
        &chat_id,
        &owner_user,
        &format!("event-cross-space-lineage-{suffix}"),
    );
    let cross_space_lineage_proposal = space_configuration::prepare(
        &pool,
        cross_space_lineage_session.clone(),
        automation_operation_for_key_intent(
            &automation_key,
            "rollback",
            Some(repeated_restored_version),
        ),
    )
    .await
    .expect("prepare cross-Space lineage rollback proposal");
    let cross_space_lineage_code = response_code(&cross_space_lineage_proposal);
    let cross_space_lineage_rejection = space_configuration::confirm(
        &pool,
        confirmation_session(cross_space_lineage_session, &cross_space_lineage_code),
        response_uuid(&cross_space_lineage_proposal, "proposal_id"),
        cross_space_lineage_code,
    )
    .await
    .expect_err("cross-Space lineage must not authorize rollback");
    assert!(cross_space_lineage_rejection
        .to_string()
        .contains("exact historical active definition"));

    sqlx::query("UPDATE qintopia_agent_os.work_item_events SET data = $2 WHERE id = $1")
        .bind(repeated_lineage_event_id)
        .bind(&repeated_lineage)
        .execute(&pool)
        .await
        .expect("restore exact rollback lineage after adversarial checks");
    let recovered = prepare_and_confirm(
        &pool,
        session(
            &chat_id,
            &owner_user,
            &format!("event-lineage-recovered-{suffix}"),
        ),
        automation_operation_for_key_intent(
            &automation_key,
            "rollback",
            Some(repeated_restored_version),
        ),
    )
    .await;
    assert_eq!(recovered["status"], "completed");

    let mapping_version_count_after_space_b: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.channel_event_mapping_versions WHERE provider = 'qiwe' AND definition_key = $1",
    )
    .bind(&mapping_key)
    .fetch_one(&pool)
    .await
    .expect("count provider mapping versions after second-Space shadow");
    assert_eq!(
        mapping_version_count_after_space_b, mapping_version_count_before_space_b,
        "a second Space must reuse the identical active provider mapping"
    );

    let premature_b = space_configuration::prepare(
        &pool,
        session(
            &chat_b,
            &owner_user,
            &format!("event-premature-active-b-{suffix}"),
        ),
        automation_operation_for_key_intent(&automation_key_b, "activate", None),
    )
    .await
    .expect_err("another Space must not reuse the first Space's shadow evidence");
    assert!(premature_b
        .to_string()
        .contains("exact current-Space shadow version"));

    let event_b = RawQiweEvent {
        event_id: format!("integration-event-b-{suffix}"),
        received_at: Utc::now(),
        source: "qiwe".to_string(),
        ingress_auth_verified: true,
        payload: json!({
            "data": [{
                "cmd": 15000,
                "msgType": 1002,
                "msgUniqueIdentifier": format!("integration-provider-event-b-{suffix}"),
                "fromRoomId": chat_b,
                "timestamp": Utc::now().timestamp(),
                "msgData": {"changedMemberList": "bWVtYmVyLWI="}
            }]
        }),
    };
    let raw_event_b_id = db::persist_raw_event(&pool, "qintopia.qiwe.raw", &event_b)
        .await
        .expect("persist authenticated second-Space event");
    let persisted_event_b = db::load_raw_event(&pool, raw_event_b_id)
        .await
        .expect("load authenticated second-Space event");
    channel_event_mapping::process_persisted_raw_event(&pool, raw_event_b_id, &persisted_event_b)
        .await
        .expect("record second-Space automation shadow observation");
    let activated_b = prepare_and_confirm(
        &pool,
        session(&chat_b, &owner_user, &format!("event-active-b-{suffix}")),
        automation_operation_for_key_intent(&automation_key_b, "activate", None),
    )
    .await;
    assert_eq!(activated_b["status"], "completed");
    let active_b_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.automation_definition_versions WHERE space_id = $1 AND definition_key = $2 AND status = 'active'",
    )
    .bind(space_b_id)
    .bind(&automation_key_b)
    .fetch_one(&pool)
    .await
    .expect("count second-Space active event automation versions");
    assert_eq!(active_b_count, 1);

    let cross_space_mapping_key = format!("integration_cross_space_mapping_{suffix}");
    let cross_space_mapping_id: Uuid = sqlx::query_scalar(
        r#"
        INSERT INTO qintopia_agent_os.channel_event_mapping_versions
            (provider, definition_key, version, selector, extractor, official_sources,
             validation_evidence, status, definition_digest, created_by_person_id)
        SELECT provider, $1, 1,
               jsonb_set(
                   selector,
                   '{rules,0,rules,0,value}',
                   to_jsonb('CROSS_SPACE_GROUP_MEMBER_ADD'::text)
               ),
               jsonb_set(extractor, '{space_chat_id,pointer}', to_jsonb('/targetRoomId'::text)),
               official_sources, validation_evidence, 'shadow', repeat('a', 64), $2
        FROM qintopia_agent_os.channel_event_mapping_versions
        WHERE id = $3
        RETURNING id
        "#,
    )
    .bind(&cross_space_mapping_key)
    .bind(owner_id)
    .bind(shadow_mapping_id)
    .fetch_one(&pool)
    .await
    .expect("seed cross-Space adversarial shadow mapping");
    let cross_space_event = RawQiweEvent {
        event_id: format!("integration-cross-space-event-{suffix}"),
        received_at: Utc::now(),
        source: "qiwe".to_string(),
        ingress_auth_verified: true,
        payload: json!({
            "data": [{
                "cmd": 15500,
                "newMsgType": "CROSS_SPACE_GROUP_MEMBER_ADD",
                "msgUniqueIdentifier": format!("integration-cross-space-provider-event-{suffix}"),
                "fromRoomId": chat_id,
                "targetRoomId": chat_b,
                "timestamp": Utc::now().timestamp(),
                "msgData": {"changedMemberList": "bWVtYmVyLXg="}
            }]
        }),
    };
    let cross_space_raw_event_id =
        db::persist_raw_event(&pool, "qintopia.qiwe.raw", &cross_space_event)
            .await
            .expect("persist cross-Space adversarial event");
    let cross_space_persisted_event = db::load_raw_event(&pool, cross_space_raw_event_id)
        .await
        .expect("load cross-Space adversarial event");
    channel_event_mapping::process_persisted_raw_event(
        &pool,
        cross_space_raw_event_id,
        &cross_space_persisted_event,
    )
    .await
    .expect("reject cross-Space mapping route");
    let cross_space_observation_count: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM qintopia_agent_os.work_items
        WHERE work_item_type = 'space_event_shadow_observation'
          AND source_refs ->> 'mapping_version_id' = $1
        "#,
    )
    .bind(cross_space_mapping_id.to_string())
    .fetch_one(&pool)
    .await
    .expect("count cross-Space mapping observations");
    assert_eq!(
        cross_space_observation_count, 0,
        "a mapping cannot route an authenticated raw event into another Space"
    );

    let cross_space_mapping_replacement = space_configuration::prepare(
        &pool,
        session(
            &chat_id,
            &owner_user,
            &format!("cross-space-mapping-replacement-{suffix}"),
        ),
        event_automation_intent("paused", &mapping_key, &automation_key),
    )
    .await
    .expect_err("one Space must not retire a mapping still used by another Space");
    assert!(cross_space_mapping_replacement
        .to_string()
        .contains("another Space still references it"));

    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET enabled = true WHERE capability_key IN ('erhua.execute_space_business', 'erhua.qiwe_text_template')",
    )
    .execute(&pool)
    .await
    .expect("enable integration-test execution capabilities");
    let event_after_activation = RawQiweEvent {
        event_id: format!("integration-event-active-{suffix}"),
        received_at: Utc::now(),
        source: "qiwe".to_string(),
        ingress_auth_verified: true,
        payload: json!({
            "data": [{
                "cmd": 15500,
                "newMsgType": "GROUP_MEMBER_ADD",
                "msgUniqueIdentifier": format!("integration-provider-event-active-{suffix}"),
                "fromRoomId": chat_id,
                "timestamp": Utc::now().timestamp(),
                "msgData": {"changedMemberList": "bWVtYmVyLWM="}
            }]
        }),
    };
    let active_raw_event_id =
        db::persist_raw_event(&pool, "qintopia.qiwe.raw", &event_after_activation)
            .await
            .expect("persist active-automation event");
    let active_persisted_event = db::load_raw_event(&pool, active_raw_event_id)
        .await
        .expect("load active-automation event");
    channel_event_mapping::process_persisted_raw_event(
        &pool,
        active_raw_event_id,
        &active_persisted_event,
    )
    .await
    .expect("create active Space automation run");
    let duplicate_active_raw_event_id =
        db::persist_raw_event(&pool, "qintopia.qiwe.raw", &event_after_activation)
            .await
            .expect("persist duplicate active-automation event");
    assert_eq!(duplicate_active_raw_event_id, active_raw_event_id);
    let duplicate_active_persisted_event = db::load_raw_event(&pool, duplicate_active_raw_event_id)
        .await
        .expect("load duplicate active-automation event");
    channel_event_mapping::process_persisted_raw_event(
        &pool,
        duplicate_active_raw_event_id,
        &duplicate_active_persisted_event,
    )
    .await
    .expect("deduplicate repeated active Space automation callback");
    let duplicate_active_run_count: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM qintopia_agent_os.work_items
        WHERE space_id = $1
          AND work_item_type = 'space_automation_run'
          AND payload ->> 'automation_key' = $2
        "#,
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("count runs after duplicate active callback");
    assert_eq!(
        duplicate_active_run_count, 1,
        "duplicate callbacks must create exactly one automation run"
    );
    let run_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_agent_os.work_items WHERE space_id = $1 AND work_item_type = 'space_automation_run' AND status = 'queued' ORDER BY created_at DESC LIMIT 1",
    )
    .bind(space_id)
    .fetch_one(&pool)
    .await
    .expect("load queued active Space automation run");

    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET enabled = false WHERE capability_key = 'erhua.qiwe_text_template'",
    )
    .execute(&pool)
    .await
    .expect("revoke selected event capability before enqueue");
    let revoked_capability_event = RawQiweEvent {
        event_id: format!("integration-event-revoked-capability-{suffix}"),
        received_at: Utc::now(),
        source: "qiwe".to_string(),
        ingress_auth_verified: true,
        payload: json!({
            "data": [{
                "cmd": 15500,
                "newMsgType": "GROUP_MEMBER_ADD",
                "msgUniqueIdentifier": format!("integration-provider-event-revoked-capability-{suffix}"),
                "fromRoomId": chat_id,
                "timestamp": Utc::now().timestamp(),
                "msgData": {"changedMemberList": "bWVtYmVyLWQ="}
            }]
        }),
    };
    let revoked_raw_event_id =
        db::persist_raw_event(&pool, "qintopia.qiwe.raw", &revoked_capability_event)
            .await
            .expect("persist event behind revoked selected capability");
    let revoked_persisted_event = db::load_raw_event(&pool, revoked_raw_event_id)
        .await
        .expect("load event behind revoked selected capability");
    channel_event_mapping::process_persisted_raw_event(
        &pool,
        revoked_raw_event_id,
        &revoked_persisted_event,
    )
    .await
    .expect("skip event automation whose selected capability is revoked");
    let run_count_after_revocation: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.work_items WHERE space_id = $1 AND work_item_type = 'space_automation_run'",
    )
    .bind(space_id)
    .fetch_one(&pool)
    .await
    .expect("count event runs behind revoked selected capability");
    assert_eq!(run_count_after_revocation, 1);
    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET enabled = true WHERE capability_key = 'erhua.qiwe_text_template'",
    )
    .execute(&pool)
    .await
    .expect("restore selected event capability");

    space_automation_execution::assert_capability_and_policy_revocation_gates_for_integration_test(
        &pool, run_id,
    )
    .await
    .expect("claim and live gates must reject capability or policy revocation");
    let execution_attempts: i32 =
        sqlx::query_scalar("SELECT attempts FROM qintopia_agent_os.work_items WHERE id = $1")
            .bind(run_id)
            .fetch_one(&pool)
            .await
            .expect("load duplicate-callback automation attempt count");
    assert_eq!(
        execution_attempts, 1,
        "one deduplicated automation run must permit exactly one execution attempt"
    );
    let execution_started_events: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM qintopia_agent_os.work_item_events
        WHERE work_item_id = $1
          AND event_type = 'space_automation_execution_started'
        "#,
    )
    .bind(run_id)
    .fetch_one(&pool)
    .await
    .expect("count duplicate-callback automation execution starts");
    assert_eq!(
        execution_started_events, 1,
        "duplicate callbacks must not start a second execution attempt"
    );
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
async fn postgres_unsupported_approval_policies_cannot_create_active_automations() {
    let database_url = postgres_integration_database_url();
    let pool = db::connect(&database_url, 4)
        .await
        .expect("connect disposable PostgreSQL");
    db::run_migrations(&pool)
        .await
        .expect("migrate disposable PostgreSQL");
    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET enabled = true WHERE capability_key = 'erhua.manage_space_configuration'",
    )
    .execute(&pool)
    .await
    .expect("enable integration-test Space configuration capability");

    let suffix = Uuid::new_v4().simple().to_string();
    let owner_user = format!("integration-approval-owner-{suffix}");
    let owner_id = Uuid::new_v4();
    seed_actor(&pool, owner_id, "Integration Approval Owner").await;
    sqlx::query(
        r#"
        INSERT INTO qintopia_identity.person_memberships
            (person_id, community_key, role, status, started_at)
        VALUES ($1, 'qintopia', 'owner', 'active', now())
        "#,
    )
    .bind(owner_id)
    .execute(&pool)
    .await
    .expect("seed integration-test global owner");

    let deterministic_chat = format!("integration-approval-deterministic-{suffix}");
    let deterministic_key = format!("integration_deterministic_{suffix}");
    seed_channel_identity(&pool, owner_id, &owner_user, &deterministic_chat).await;

    let ordinary_active = space_configuration::prepare(
        &pool,
        session(
            &deterministic_chat,
            &owner_user,
            &format!("ordinary-active-{suffix}"),
        ),
        deterministic_schedule_intent("active", &deterministic_key, "before_external_use"),
    )
    .await
    .expect_err("ordinary active QiWe template must reject an unsupported policy");
    assert!(ordinary_active
        .to_string()
        .contains("requires space_admin_confirmation"));
    let ordinary_active_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.automation_definition_versions WHERE definition_key = $1 AND status = 'active'",
    )
    .bind(&deterministic_key)
    .fetch_one(&pool)
    .await
    .expect("count rejected ordinary active automation versions");
    assert_eq!(ordinary_active_count, 0);

    prepare_and_confirm(
        &pool,
        session(
            &deterministic_chat,
            &owner_user,
            &format!("shadow-unsupported-{suffix}"),
        ),
        deterministic_schedule_intent("shadow", &deterministic_key, "before_external_use"),
    )
    .await;
    let exact_activation = space_configuration::prepare(
        &pool,
        session(
            &deterministic_chat,
            &owner_user,
            &format!("activate-unsupported-{suffix}"),
        ),
        automation_operation_for_key_intent(&deterministic_key, "activate", None),
    )
    .await
    .expect_err("exact shadow activation must reject an unsupported policy");
    assert!(exact_activation
        .to_string()
        .contains("requires space_admin_confirmation"));
    let exact_active_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.automation_definition_versions WHERE definition_key = $1 AND status = 'active'",
    )
    .bind(&deterministic_key)
    .fetch_one(&pool)
    .await
    .expect("count rejected exact activation versions");
    assert_eq!(exact_active_count, 0);

    let rollback_chat = format!("integration-approval-rollback-{suffix}");
    seed_channel_identity(&pool, owner_id, &owner_user, &rollback_chat).await;
    prepare_and_confirm(
        &pool,
        session(
            &rollback_chat,
            &owner_user,
            &format!("rollback-baseline-{suffix}"),
        ),
        deterministic_schedule_intent("active", "integration_schedule", "space_admin_confirmation"),
    )
    .await;
    prepare_and_confirm(
        &pool,
        session(
            &rollback_chat,
            &owner_user,
            &format!("rollback-pause-{suffix}"),
        ),
        automation_operation_intent("pause", None),
    )
    .await;
    let rollback_space_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_messages.conversations WHERE tenant_id = 'qintopia' AND platform = 'qiwe' AND chat_id = $1",
    )
    .bind(&rollback_chat)
    .fetch_one(&pool)
    .await
    .expect("load rollback-test Space");
    sqlx::query(
        r#"
        UPDATE qintopia_agent_os.business_definition_versions
        SET approval_policy = 'human_final_confirmation'
        WHERE space_id = $1
          AND definition_key = 'integration_schedule'
          AND status = 'active'
        "#,
    )
    .bind(rollback_space_id)
    .execute(&pool)
    .await
    .expect("simulate a legacy unsupported active business policy");

    let rollback = space_configuration::prepare(
        &pool,
        session(
            &rollback_chat,
            &owner_user,
            &format!("rollback-unsupported-{suffix}"),
        ),
        automation_operation_intent("rollback", Some(1)),
    )
    .await
    .expect_err("rollback must reject an unsupported active business policy");
    assert!(rollback.to_string().contains("unsupported per-run"));
    let rollback_active_count: i64 = sqlx::query_scalar(
        r#"
        SELECT count(*)
        FROM qintopia_agent_os.automation_definition_versions
        WHERE space_id = $1
          AND definition_key = 'integration_schedule'
          AND status = 'active'
        "#,
    )
    .bind(rollback_space_id)
    .fetch_one(&pool)
    .await
    .expect("count rejected rollback active versions");
    assert_eq!(rollback_active_count, 0);
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
async fn postgres_historical_observation_cannot_authorize_direct_active_mapping_promotion() {
    let database_url = postgres_integration_database_url();
    let pool = db::connect(&database_url, 4)
        .await
        .expect("connect disposable PostgreSQL");
    db::run_migrations(&pool)
        .await
        .expect("migrate disposable PostgreSQL");
    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET enabled = true WHERE capability_key = 'erhua.manage_space_configuration'",
    )
    .execute(&pool)
    .await
    .expect("enable integration-test Space configuration capability");

    let suffix = Uuid::new_v4().simple().to_string();
    let chat_id = format!("integration-mapping-freshness-{suffix}");
    let owner_user = format!("integration-mapping-owner-{suffix}");
    let mapping_key = format!("integration_mapping_freshness_{suffix}");
    let owner_id = Uuid::new_v4();
    seed_actor(&pool, owner_id, "Integration Mapping Freshness Owner").await;
    seed_channel_identity(&pool, owner_id, &owner_user, &chat_id).await;
    sqlx::query(
        r#"
        INSERT INTO qintopia_identity.person_memberships
            (person_id, community_key, role, status, started_at)
        VALUES ($1, 'qintopia', 'owner', 'active', now())
        "#,
    )
    .bind(owner_id)
    .execute(&pool)
    .await
    .expect("seed integration-test global owner");
    prepare_and_confirm(
        &pool,
        session(&chat_id, &owner_user, &format!("space-seed-{suffix}")),
        policy_intent("active", "mapping-freshness"),
    )
    .await;
    let space_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_messages.conversations WHERE tenant_id = 'qintopia' AND platform = 'qiwe' AND chat_id = $1",
    )
    .bind(&chat_id)
    .fetch_one(&pool)
    .await
    .expect("load mapping-freshness Space");

    let historical_event = RawQiweEvent {
        event_id: format!("integration-mapping-historical-{suffix}"),
        received_at: Utc::now(),
        source: "qiwe".to_string(),
        ingress_auth_verified: true,
        payload: json!({
            "data": [{
                "cmd": 15500,
                "newMsgType": "GROUP_MEMBER_ADD",
                "msgUniqueIdentifier": format!("integration-mapping-provider-historical-{suffix}"),
                "fromRoomId": chat_id,
                "timestamp": Utc::now().timestamp(),
                "msgData": {"changedMemberList": "bWVtYmVyLW1hcHBpbmc="}
            }]
        }),
    };
    let historical_raw_event_id =
        db::persist_raw_event(&pool, "qintopia.qiwe.raw", &historical_event)
            .await
            .expect("persist event before mapping definition");
    prepare_and_confirm(
        &pool,
        session(&chat_id, &owner_user, &format!("mapping-shadow-{suffix}")),
        mapping_only_intent("shadow", &mapping_key),
    )
    .await;
    let shadow_mapping_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_agent_os.channel_event_mapping_versions WHERE provider = 'qiwe' AND definition_key = $1 AND status = 'shadow'",
    )
    .bind(&mapping_key)
    .fetch_one(&pool)
    .await
    .expect("load mapping-only shadow version");
    let historical_precedes_mapping: bool = sqlx::query_scalar(
        r#"
        SELECT raw_event.space_id = $3
           AND raw_event.created_at <= mapping.created_at
        FROM qintopia_messages.raw_events raw_event
        JOIN qintopia_agent_os.channel_event_mapping_versions mapping ON mapping.id = $2
        WHERE raw_event.id = $1
        "#,
    )
    .bind(historical_raw_event_id)
    .bind(shadow_mapping_id)
    .bind(space_id)
    .fetch_one(&pool)
    .await
    .expect("verify historical event predates mapping shadow");
    assert!(historical_precedes_mapping);
    seed_shadow_observation(
        &pool,
        space_id,
        historical_raw_event_id,
        shadow_mapping_id,
        "mapping_shadow",
        &format!("integration-stale-mapping-observation-{suffix}"),
    )
    .await;

    let promotion = space_configuration::prepare(
        &pool,
        session(&chat_id, &owner_user, &format!("mapping-active-{suffix}")),
        mapping_only_intent("active", &mapping_key),
    )
    .await
    .expect_err("historical observation must not authorize direct active mapping promotion");
    assert!(promotion.to_string().contains("matching shadow version"));
    let active_mapping_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.channel_event_mapping_versions WHERE provider = 'qiwe' AND definition_key = $1 AND status = 'active'",
    )
    .bind(&mapping_key)
    .fetch_one(&pool)
    .await
    .expect("count direct active mapping versions after rejection");
    assert_eq!(active_mapping_count, 0);
}

#[tokio::test]
#[ignore = "requires guarded disposable PostgreSQL qintopia_test"]
async fn postgres_historical_observation_cannot_authorize_direct_active_automation() {
    let database_url = postgres_integration_database_url();
    let pool = db::connect(&database_url, 4)
        .await
        .expect("connect disposable PostgreSQL");
    db::run_migrations(&pool)
        .await
        .expect("migrate disposable PostgreSQL");
    sqlx::query(
        "UPDATE qintopia_agent_os.capabilities SET enabled = true WHERE capability_key = 'erhua.manage_space_configuration'",
    )
    .execute(&pool)
    .await
    .expect("enable integration-test Space configuration capability");

    let suffix = Uuid::new_v4().simple().to_string();
    let chat_id = format!("integration-automation-freshness-{suffix}");
    let owner_user = format!("integration-automation-owner-{suffix}");
    let mapping_key = format!("integration_automation_mapping_{suffix}");
    let automation_key = format!("integration_automation_freshness_{suffix}");
    let owner_id = Uuid::new_v4();
    seed_actor(&pool, owner_id, "Integration Automation Freshness Owner").await;
    seed_channel_identity(&pool, owner_id, &owner_user, &chat_id).await;
    sqlx::query(
        r#"
        INSERT INTO qintopia_identity.person_memberships
            (person_id, community_key, role, status, started_at)
        VALUES ($1, 'qintopia', 'owner', 'active', now())
        "#,
    )
    .bind(owner_id)
    .execute(&pool)
    .await
    .expect("seed integration-test global owner");
    prepare_and_confirm(
        &pool,
        session(&chat_id, &owner_user, &format!("space-seed-{suffix}")),
        policy_intent("active", "automation-freshness"),
    )
    .await;
    let space_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_messages.conversations WHERE tenant_id = 'qintopia' AND platform = 'qiwe' AND chat_id = $1",
    )
    .bind(&chat_id)
    .fetch_one(&pool)
    .await
    .expect("load automation-freshness Space");

    let historical_event = RawQiweEvent {
        event_id: format!("integration-automation-historical-{suffix}"),
        received_at: Utc::now(),
        source: "qiwe".to_string(),
        ingress_auth_verified: true,
        payload: json!({
            "data": [{
                "cmd": 15500,
                "newMsgType": "GROUP_MEMBER_ADD",
                "msgUniqueIdentifier": format!("integration-automation-provider-historical-{suffix}"),
                "fromRoomId": chat_id,
                "timestamp": Utc::now().timestamp(),
                "msgData": {"changedMemberList": "bWVtYmVyLWF1dG9tYXRpb24="}
            }]
        }),
    };
    let historical_raw_event_id =
        db::persist_raw_event(&pool, "qintopia.qiwe.raw", &historical_event)
            .await
            .expect("persist event before mapping and automation definitions");
    prepare_and_confirm(
        &pool,
        session(&chat_id, &owner_user, &format!("mapping-shadow-{suffix}")),
        mapping_only_intent("shadow", &mapping_key),
    )
    .await;
    let shadow_mapping_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_agent_os.channel_event_mapping_versions WHERE provider = 'qiwe' AND definition_key = $1 AND status = 'shadow'",
    )
    .bind(&mapping_key)
    .fetch_one(&pool)
    .await
    .expect("load automation-test shadow mapping");

    let fresh_event = RawQiweEvent {
        event_id: format!("integration-automation-fresh-{suffix}"),
        received_at: Utc::now(),
        source: "qiwe".to_string(),
        ingress_auth_verified: true,
        payload: json!({
            "data": [{
                "cmd": 15000,
                "msgType": 1002,
                "msgUniqueIdentifier": format!("integration-automation-provider-fresh-{suffix}"),
                "fromRoomId": chat_id,
                "timestamp": Utc::now().timestamp(),
                "msgData": {"changedMemberList": "bWVtYmVyLWZyZXNo"}
            }]
        }),
    };
    let fresh_raw_event_id = db::persist_raw_event(&pool, "qintopia.qiwe.raw", &fresh_event)
        .await
        .expect("persist fresh event after mapping shadow");
    let fresh_event_follows_mapping: bool = sqlx::query_scalar(
        r#"
        SELECT raw_event.created_at > mapping.created_at
        FROM qintopia_messages.raw_events raw_event
        JOIN qintopia_agent_os.channel_event_mapping_versions mapping ON mapping.id = $2
        WHERE raw_event.id = $1
        "#,
    )
    .bind(fresh_raw_event_id)
    .bind(shadow_mapping_id)
    .fetch_one(&pool)
    .await
    .expect("verify fresh event follows mapping shadow");
    assert!(fresh_event_follows_mapping);
    seed_shadow_observation(
        &pool,
        space_id,
        fresh_raw_event_id,
        shadow_mapping_id,
        "mapping_shadow",
        &format!("integration-fresh-mapping-observation-{suffix}"),
    )
    .await;
    prepare_and_confirm(
        &pool,
        session(&chat_id, &owner_user, &format!("mapping-active-{suffix}")),
        mapping_only_intent("active", &mapping_key),
    )
    .await;
    let active_mapping_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_agent_os.channel_event_mapping_versions WHERE provider = 'qiwe' AND definition_key = $1 AND status = 'active'",
    )
    .bind(&mapping_key)
    .fetch_one(&pool)
    .await
    .expect("load active mapping for direct automation test");

    prepare_and_confirm(
        &pool,
        session(
            &chat_id,
            &owner_user,
            &format!("automation-shadow-{suffix}"),
        ),
        shadow_event_automation_with_active_dependencies_intent(&mapping_key, &automation_key),
    )
    .await;
    let shadow_automation_id: Uuid = sqlx::query_scalar(
        "SELECT id FROM qintopia_agent_os.automation_definition_versions WHERE space_id = $1 AND definition_key = $2 AND status = 'shadow'",
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("load direct-active automation shadow");
    let historical_precedes_dependencies: bool = sqlx::query_scalar(
        r#"
        SELECT raw_event.space_id = $4
           AND raw_event.created_at <= GREATEST(automation.created_at, mapping.created_at)
        FROM qintopia_messages.raw_events raw_event
        JOIN qintopia_agent_os.automation_definition_versions automation ON automation.id = $2
        JOIN qintopia_agent_os.channel_event_mapping_versions mapping ON mapping.id = $3
        WHERE raw_event.id = $1
        "#,
    )
    .bind(historical_raw_event_id)
    .bind(shadow_automation_id)
    .bind(active_mapping_id)
    .bind(space_id)
    .fetch_one(&pool)
    .await
    .expect("verify historical event predates direct-active dependencies");
    assert!(historical_precedes_dependencies);
    seed_shadow_observation(
        &pool,
        space_id,
        historical_raw_event_id,
        active_mapping_id,
        &format!("automation_shadow:{shadow_automation_id}"),
        &format!("integration-stale-automation-observation-{suffix}"),
    )
    .await;

    let prepared = space_configuration::prepare(
        &pool,
        session(
            &chat_id,
            &owner_user,
            &format!("automation-active-{suffix}"),
        ),
        event_automation_only_intent("active", &mapping_key, &automation_key),
    )
    .await
    .expect("prepare direct active automation proposal");
    let code = response_code(&prepared);
    let activation = space_configuration::confirm(
        &pool,
        confirmation_session(
            session(
                &chat_id,
                &owner_user,
                &format!("automation-active-{suffix}"),
            ),
            &code,
        ),
        response_uuid(&prepared, "proposal_id"),
        code,
    )
    .await
    .expect_err("historical observation must not authorize direct active automation");
    assert!(activation
        .to_string()
        .contains("completed same-Space shadow observation"));
    let active_automation_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.automation_definition_versions WHERE space_id = $1 AND definition_key = $2 AND status = 'active'",
    )
    .bind(space_id)
    .bind(&automation_key)
    .fetch_one(&pool)
    .await
    .expect("count direct active automation versions after rejection");
    assert_eq!(active_automation_count, 0);
}
