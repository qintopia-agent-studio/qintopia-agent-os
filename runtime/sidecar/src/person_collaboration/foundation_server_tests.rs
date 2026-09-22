//! Exercise the exact broker parser and service ingress, without real channels.
use super::foundation_server::parse_broker_request;
use serde_json::{json, Value};

fn request(tool: &str, chat_type: &str, chat: &str, message: &str, arguments: Value) -> Value {
    json!({"operation":"person_foundation_tool","schema_version":1,"agent":"erhua","tool":tool,"trusted_context":{"platform":"qiwe","chat_type":chat_type,"chat_id":chat,"sender_id":"synthetic-resident","message_id":message,"gateway_id":"synthetic-qiwe-one"},"arguments":arguments,"token":"synthetic-test-token-never-a-real-credential"})
}

#[test]
fn foundation_broker_parser_rejects_duplicate_keys_before_value_deserialization() {
    let mut valid=serde_json::to_string(&request("remember","direct","synthetic-chat","synthetic-message",json!({"operation_id":"00000000-0000-0000-0000-000000000001","expected_version":0,"change":{"action":"stop"}}))).unwrap();
    assert!(parse_broker_request(valid.as_bytes()).is_ok());
    valid = valid.replace(
        "\"action\":\"stop\"",
        "\"action\":\"stop\",\"action\":\"set\"",
    );
    assert!(parse_broker_request(valid.as_bytes()).is_err());
    let top = serde_json::to_string(&request(
        "context",
        "direct",
        "synthetic-chat",
        "synthetic-message",
        json!({}),
    ))
    .unwrap()
    .replace(
        "\"agent\":\"erhua\"",
        "\"agent\":\"erhua\",\"agent\":\"default\"",
    );
    assert!(parse_broker_request(top.as_bytes()).is_err());
}

#[cfg(feature = "postgres-integration-tests")]
mod database {
    use super::*;
    use crate::person_collaboration::{
        foundation_server::{broker_invoke, dispatch},
        model::{Assignment, Change, Command, PermissionMode, PermissionSetting},
        Actor, Store,
    };
    use anyhow::Result;
    use uuid::Uuid;

    async fn fixture() -> Result<Store> {
        let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
        let store = Store::local(
            &database,
            &format!("synthetic-collaboration-broker-{}", Uuid::new_v4()),
        )
        .await?;
        crate::db::run_migrations(&store.pool).await?;
        store.bootstrap_fixture().await?;
        store.bootstrap_identity_memory_fixture().await?;
        Ok(store)
    }

    async fn message(
        store: &Store,
        chat_type: &str,
        chat: &str,
        verified: bool,
    ) -> Result<(String, Uuid)> {
        let event = Uuid::new_v4().to_string();
        let raw:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.raw_events(event_id,source,subject,received_at,payload,ingress_auth_verified) VALUES($1,'qiwe','qintopia.qiwe.raw.authenticated',clock_timestamp(),'{}',$2) RETURNING id")
            .bind(&event).bind(verified).fetch_one(&store.pool).await?;
        sqlx::query("INSERT INTO qintopia_messages.messages(tenant_id,platform,message_id,event_id,chat_id,chat_type,sender_id,message_kind,sent_at,received_at,raw_event_id) VALUES($1,'qiwe',$2,$2,$3,$4,'synthetic-resident','text',clock_timestamp()-interval '1 minute',clock_timestamp(),$5)")
            .bind(&store.tenant).bind(&event).bind(chat).bind(chat_type).bind(raw).execute(&store.pool).await?;
        Ok((event, raw))
    }

    async fn invoke(store: &Store, value: Value) -> Result<Value> {
        broker_invoke(
            store,
            "synthetic-qiwe-one",
            "erhua",
            parse_broker_request(&serde_json::to_vec(&value)?)?,
        )
        .await
    }

    async fn talk(
        store: &Store,
        actor: &Actor,
        scope: Uuid,
        operation: Uuid,
        text: &str,
    ) -> Result<Value> {
        dispatch(
            store,
            actor,
            "/api/foundation/talk",
            &serde_json::to_vec(&json!({"scope":scope,"operation_id":operation,"text":text}))?,
        )
        .await
    }

    async fn welcome_fixture() -> Result<(Store, Actor, Uuid, Uuid)> {
        let store = fixture().await?;
        let owner = store.actor(store.fixture_operator().await?).await?;
        let actor = store
            .gateway_actor("synthetic-qiwe-one", "synthetic-resident")
            .await?;
        let scope = store.gateway_scope(&actor).await?;
        let state = store.state(&owner).await?;
        let named = |collection: &str, label: &str| -> Uuid {
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
        };
        let assignment = Assignment {
            collaboration: None,
            person: store.verified_person(&actor).await?,
            role: named("roles", "舍长"),
            duty: Some(named("duties", "居民服务")),
            scope,
            agent: "erhua".into(),
            domain: "community_service".into(),
            responsibility: "合成居民服务对话验收".into(),
            valid_until: None,
            proxy_for: None,
            actions: vec![],
            permissions: ["change_rules", "review", "publish"]
                .into_iter()
                .map(|action| PermissionSetting {
                    action: action.into(),
                    mode: PermissionMode::Autonomous,
                    reviewer: None,
                })
                .collect(),
            delegation: None,
        };
        store
            .command(
                &owner,
                &Command {
                    operation_id: Uuid::new_v4(),
                    expected_version: state["version"].as_i64().unwrap(),
                    change: Change::Assign(Box::new(assignment)),
                },
                true,
            )
            .await?;
        let seed = crate::resident_welcome::store::Store {
            pool: store.pool.clone(),
        }
        .bootstrap_foundation_fixture(&store.tenant)
        .await?;
        let case = serde_json::from_value(
            seed["targets"]
                .as_array()
                .unwrap()
                .iter()
                .find(|t| t["scope_ref"] == json!(scope))
                .unwrap()["case_ref"]
                .clone(),
        )?;
        Ok((store, actor, scope, case))
    }

    async fn welcome_counts(store: &Store, case: Uuid) -> Result<Value> {
        Ok(sqlx::query_scalar("SELECT jsonb_build_array((SELECT count(*) FROM qintopia_agent_os.welcome_synthetic_effects e JOIN qintopia_agent_os.welcome_actions a ON a.id=e.action_id WHERE a.case_id=$1),(SELECT count(*) FROM qintopia_agent_os.welcome_approvals p JOIN qintopia_agent_os.welcome_artifact_bindings b ON b.artifact_id=p.artifact_id WHERE b.case_id=$1))").bind(case).fetch_one(&store.pool).await?)
    }

    #[tokio::test]
    #[ignore = "explicit task-isolated local database, foundation enable and synthetic runtime required"]
    async fn foundation_http_welcome_conversation_direct_and_replay_preserve_exact_effects(
    ) -> Result<()> {
        let (store, actor, scope, case) = welcome_fixture().await?;
        let operation = Uuid::new_v4();
        let saved = talk(
            &store,
            &actor,
            scope,
            operation,
            "以后本栋欢迎直接发，卡片和文案",
        )
        .await?;
        assert_eq!(saved["tool_result"]["result"]["status"], "saved");
        let repeated = talk(
            &store,
            &actor,
            scope,
            operation,
            "以后本栋欢迎直接发，卡片和文案",
        )
        .await?;
        assert_eq!(repeated["tool_result"]["replayed"], true);
        assert_eq!(repeated["tool_result"]["welcome"]["latest_rule_version"], 1);
        assert_eq!(
            talk(
                &store,
                &actor,
                scope,
                operation,
                "以后本栋欢迎先审，卡片和文案"
            )
            .await
            .unwrap_err()
            .to_string(),
            "idempotency_conflict"
        );
        let render = Uuid::new_v4();
        let card = talk(&store, &actor, scope, render, "为本次欢迎制卡").await?;
        assert_eq!(
            card["tool_result"]["result"]["status"], "rendered",
            "{card}"
        );
        let image = card["attachments"]
            .as_array()
            .unwrap()
            .iter()
            .find(|a| a["kind"] == "image")
            .unwrap();
        assert!(image["label"].as_str().unwrap().starts_with("卡片版本 A"));
        let repeated = talk(&store, &actor, scope, render, "为本次欢迎制卡").await?;
        assert_eq!(repeated["tool_result"]["replayed"], true);
        assert_eq!(
            repeated["tool_result"]["result"]["image_ref"],
            card["tool_result"]["result"]["image_ref"]
        );
        assert_eq!(welcome_counts(&store, case).await?, json!([0, 0]));
        let advance = Uuid::new_v4();
        let result = talk(&store, &actor, scope, advance, "推进本次欢迎").await?;
        assert_ne!(
            result["tool_result"]["result"]["status"], "blocked",
            "{result}"
        );
        assert_eq!(
            welcome_counts(&store, case).await?,
            json!([2, 0]),
            "{result}"
        );
        let repeated = talk(&store, &actor, scope, advance, "推进本次欢迎").await?;
        assert_eq!(repeated["tool_result"]["replayed"], true);
        assert_eq!(welcome_counts(&store, case).await?, json!([2, 0]));
        Ok(())
    }

    #[tokio::test]
    #[ignore = "explicit task-isolated local database, foundation enable and synthetic runtime required"]
    async fn foundation_http_welcome_conversation_requires_exact_reviews_and_preserves_unknown(
    ) -> Result<()> {
        let (store, actor, scope, case) = welcome_fixture().await?;
        let setting = talk(
            &store,
            &actor,
            scope,
            Uuid::new_v4(),
            "本次欢迎先审，卡片和文案",
        )
        .await?;
        assert_eq!(setting["tool_result"]["result"]["status"], "saved");
        assert_eq!(setting["tool_result"]["welcome"]["latest_rule_version"], 0);
        let malformed = talk(&store, &actor, scope, Uuid::new_v4(), "欢迎都发吧").await?;
        assert_eq!(
            malformed["tool_result"]["status"],
            "needs_explicit_welcome_instruction"
        );
        let card = talk(&store, &actor, scope, Uuid::new_v4(), "为本次欢迎制卡").await?;
        assert_eq!(
            card["tool_result"]["result"]["status"], "rendered",
            "{card}"
        );
        let waiting = talk(&store, &actor, scope, Uuid::new_v4(), "推进本次欢迎").await?;
        assert_eq!(welcome_counts(&store, case).await?, json!([0, 0]));
        assert_eq!(
            waiting["tool_result"]["result"]["prepared"]["waiting"]
                .as_array()
                .unwrap()
                .len(),
            2,
            "{waiting}"
        );
        let reviews: Vec<String> = waiting["suggestions"]
            .as_array()
            .unwrap()
            .iter()
            .filter_map(Value::as_str)
            .filter(|s| s.starts_with("批准"))
            .map(str::to_owned)
            .collect();
        assert_eq!(reviews.len(), 2);
        for sentence in reviews {
            let operation = Uuid::new_v4();
            let result = talk(&store, &actor, scope, operation, &sentence).await?;
            assert!(
                result["tool_result"]["result"]["approval_ref"].is_string(),
                "{result}"
            );
            assert_eq!(
                talk(&store, &actor, scope, operation, &sentence).await?["tool_result"]["replayed"],
                true
            );
        }
        assert_eq!(welcome_counts(&store, case).await?, json!([0, 2]));
        let operation = Uuid::new_v4();
        let sentence = "推进本次欢迎";
        let hash = crate::person_collaboration::digest(&serde_json::to_vec(
            &json!({"scope":scope,"text":sentence}),
        )?);
        store
            .foundation_turn_input(
                &actor,
                operation,
                &hash,
                &json!({"welcome_request":{"action":"advance"},"welcome_started":true}),
            )
            .await?;
        let uncertain = talk(&store, &actor, scope, operation, sentence).await?;
        assert_eq!(
            uncertain["tool_result"]["result"]["status"],
            "outcome_unknown"
        );
        assert_eq!(welcome_counts(&store, case).await?, json!([0, 2]));
        let advanced = talk(&store, &actor, scope, Uuid::new_v4(), sentence).await?;
        assert_eq!(
            welcome_counts(&store, case).await?,
            json!([2, 2]),
            "{advanced}"
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore = "explicit task-isolated local database and foundation local enable required"]
    async fn foundation_http_scripted_memory_replay_cannot_revive_stopped_preference() -> Result<()>
    {
        let store = fixture().await?;
        let actor = store
            .gateway_actor("synthetic-qiwe-one", "synthetic-resident")
            .await?;
        let scope = store.gateway_scope(&actor).await?;
        let save = Uuid::new_v4();
        let first = talk(&store, &actor, scope, save, "以后回答我简短一点").await?;
        assert_eq!(first["tool_result"]["version"], 1);
        let repeated = talk(&store, &actor, scope, save, "以后回答我简短一点").await?;
        assert_eq!(repeated["tool_result"]["replayed"], true);
        assert_eq!(store.memory_context(&actor, "general").await?["version"], 1);
        let stop = talk(
            &store,
            &actor,
            scope,
            Uuid::new_v4(),
            "以后不用记我的回复习惯",
        )
        .await?;
        assert_eq!(stop["tool_result"]["status"], "stopped");
        assert_eq!(stop["tool_result"]["version"], 2);
        let old = talk(&store, &actor, scope, save, "以后回答我简短一点").await?;
        assert_eq!(old["tool_result"]["replayed"], true);
        assert_eq!(old["tool_result"]["current_memory"]["status"], "stopped");
        assert_eq!(old["tool_result"]["current_memory"]["version"], 2);
        // A historical save receipt must never be presented as current consent.
        assert!(old["reply"].as_str().unwrap().contains("停止"));
        let current = store.memory_context(&actor, "general").await?;
        assert_eq!(current["status"], "stopped");
        assert_eq!(current["version"], 2);
        assert!(current["reply_style"].is_null());
        let conflict = talk(&store, &actor, scope, save, "涉及费用还是详细说清楚")
            .await
            .unwrap_err();
        assert_eq!(conflict.to_string(), "idempotency_conflict");
        let person = store.verified_person(&actor).await?;
        let facts:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_identity.member_facts WHERE person_id=$1 AND fact_type='reply_preference' AND fact_status='active'")
            .bind(person).fetch_one(&store.pool).await?;
        assert_eq!(facts, 0);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "explicit task-isolated local database and foundation local enable required"]
    async fn foundation_http_scripted_rule_replay_keeps_original_revision() -> Result<()> {
        let store = fixture().await?;
        let actor = store.actor(store.fixture_operator().await?).await?;
        let state = store.state(&actor).await?;
        let find = |collection: &str, label: &str| -> Uuid {
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
        };
        let scope = find("scopes", "一栋");
        let assignment = Assignment {
            collaboration: None,
            person: store.verified_person(&actor).await?,
            role: find("roles", "舍长"),
            duty: Some(find("duties", "居民服务")),
            scope,
            agent: "erhua".into(),
            domain: "community_service".into(),
            responsibility: "合成脚本规则回归".into(),
            valid_until: None,
            proxy_for: None,
            actions: vec![],
            permissions: vec![PermissionSetting {
                action: "change_rules".into(),
                mode: PermissionMode::Autonomous,
                reviewer: None,
            }],
            delegation: None,
        };
        store
            .command(
                &actor,
                &Command {
                    operation_id: Uuid::new_v4(),
                    expected_version: state["version"].as_i64().unwrap(),
                    change: Change::Assign(Box::new(assignment)),
                },
                true,
            )
            .await?;
        let operation = Uuid::new_v4();
        let first = talk(
            &store,
            &actor,
            scope,
            operation,
            "把本栋厨房关闭时间改成晚上十点",
        )
        .await?;
        assert_eq!(first["tool_result"]["status"], "saved");
        let original = first["tool_result"]["knowledge"]["id"].clone();
        let repeat = talk(
            &store,
            &actor,
            scope,
            operation,
            "把本栋厨房关闭时间改成晚上十点",
        )
        .await?;
        assert_eq!(repeat["tool_result"]["knowledge"]["id"], original);
        assert_eq!(repeat["tool_result"]["knowledge"]["version"], 1);
        assert_eq!(repeat["tool_result"]["replayed"], true);
        let versions:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_knowledge_revisions WHERE tenant_key=$1")
            .bind(&store.tenant).fetch_one(&store.pool).await?;
        assert_eq!(versions, 1);
        assert_eq!(
            talk(&store, &actor, scope, operation, "以后回答我简短一点")
                .await
                .unwrap_err()
                .to_string(),
            "idempotency_conflict"
        );
        assert_eq!(store.memory_context(&actor, "general").await?["version"], 0);
        Ok(())
    }

    #[tokio::test]
    #[ignore = "explicit task-isolated local database and foundation local enable required"]
    async fn foundation_http_duplicate_value_keys_have_no_write_side_effects() -> Result<()> {
        let store = fixture().await?;
        let actor = store.actor(store.fixture_operator().await?).await?;
        for (path, body) in [
            (
                "/api/foundation/welcome",
                r#"{"action":"refresh_fixture","action":"execute"}"#,
            ),
            (
                "/api/foundation/rule",
                r#"{"content":{"text":"original","text":"changed"}}"#,
            ),
        ] {
            let error = dispatch(&store, &actor, path, body.as_bytes())
                .await
                .unwrap_err();
            assert!(format!("{error:#}").contains("duplicate JSON object key"));
        }
        let counts:Value=sqlx::query_scalar("SELECT jsonb_build_array((SELECT count(*) FROM qintopia_agent_os.collaboration_knowledge_items WHERE tenant_key=$1),(SELECT count(*) FROM qintopia_agent_os.collaboration_work_requests WHERE tenant_key=$1),(SELECT count(*) FROM qintopia_agent_os.collaboration_turn_sources WHERE tenant_key=$1))")
            .bind(&store.tenant).fetch_one(&store.pool).await?;
        assert_eq!(counts, json!([0, 0, 0]));
        Ok(())
    }

    #[tokio::test]
    #[ignore = "explicit task-isolated local database required"]
    async fn foundation_broker_missing_or_unverified_message_has_no_write_side_effects(
    ) -> Result<()> {
        let store = fixture().await?;
        for (tool, args) in [
            (
                "remember",
                json!({"operation_id":Uuid::new_v4(),"expected_version":0,"change":{"action":"set","condition":"general","style":"brief"}}),
            ),
            (
                "save_rule",
                json!({"operation_id":Uuid::new_v4(),"expected_version":0,"key":"kitchen","content":"合成规则"}),
            ),
            ("context", json!({"purpose":"reply"})),
            ("history", json!({"purpose":"self_history"})),
            ("task_status", json!({"work_item_id":Uuid::new_v4()})),
        ] {
            assert_eq!(
                invoke(
                    &store,
                    request(
                        tool,
                        "direct",
                        "synthetic-chat",
                        "nonexistent",
                        args.clone()
                    )
                )
                .await
                .unwrap_err()
                .to_string(),
                "trusted_message_evidence_required"
            );
            let (unverified, _) = message(&store, "direct", "synthetic-chat", false).await?;
            assert_eq!(
                invoke(
                    &store,
                    request(tool, "direct", "synthetic-chat", &unverified, args)
                )
                .await
                .unwrap_err()
                .to_string(),
                "trusted_message_evidence_required"
            );
        }
        let counts:Value=sqlx::query_scalar("SELECT jsonb_build_array((SELECT count(*) FROM qintopia_identity.person_memory_commands m JOIN qintopia_identity.source_identity_links l ON l.id=m.source_link_id WHERE l.namespace LIKE $1),(SELECT count(*) FROM qintopia_agent_os.collaboration_knowledge_items WHERE tenant_key=$2),(SELECT count(*) FROM qintopia_agent_os.collaboration_work_requests WHERE tenant_key=$2))")
            .bind(format!("{}/%",store.tenant)).bind(&store.tenant).fetch_one(&store.pool).await?;
        assert_eq!(counts, json!([0, 0, 0]));
        Ok(())
    }

    #[tokio::test]
    #[ignore = "explicit task-isolated local database required"]
    async fn foundation_broker_self_context_needs_no_job_but_history_is_private() -> Result<()> {
        let store = fixture().await?;
        let (direct, _) = message(&store, "direct", "synthetic-chat", true).await?;
        let context = invoke(
            &store,
            request(
                "context",
                "direct",
                "synthetic-chat",
                &direct,
                json!({"purpose":"reply","topic":"general"}),
            ),
        )
        .await?;
        assert_eq!(context["identity"]["identity_status"], "confirmed");
        assert_eq!(context["memory"]["version"], 0);
        assert_eq!(context["knowledge"], json!([]));
        assert_eq!(context["permissions"], json!([]));
        let history = invoke(
            &store,
            request(
                "history",
                "direct",
                "synthetic-chat",
                &direct,
                json!({"purpose":"self_history"}),
            ),
        )
        .await?;
        assert_eq!(history["purpose"], "self_history");
        let group:String=sqlx::query_scalar("SELECT c.chat_id FROM qintopia_messages.conversations c JOIN qintopia_agent_os.collaboration_scope_bindings b ON b.conversation_id=c.id JOIN qintopia_identity.person_identity_gateways g ON g.scope_id=b.scope_id AND g.tenant_key=b.tenant_key WHERE g.tenant_key=$1 AND g.gateway_key='synthetic-qiwe-one' AND b.revoked_at IS NULL")
            .bind(&store.tenant).fetch_one(&store.pool).await?;
        let (group_message, _) = message(&store, "group", &group, true).await?;
        let rejected = invoke(
            &store,
            request(
                "history",
                "group",
                &group,
                &group_message,
                json!({"purpose":"self_history"}),
            ),
        )
        .await
        .unwrap_err();
        assert_eq!(rejected.to_string(), "private_history_only");
        let public = invoke(
            &store,
            request(
                "context",
                "group",
                &group,
                &group_message,
                json!({"purpose":"reply"}),
            ),
        )
        .await?;
        assert!(public["identity"]["person_ref"].is_null());
        assert_eq!(public["memory"]["disclosure_allowed"], false);
        let forged_type = invoke(
            &store,
            request(
                "history",
                "direct",
                &group,
                &group_message,
                json!({"purpose":"self_history"}),
            ),
        )
        .await
        .unwrap_err();
        assert_eq!(forged_type.to_string(), "trusted_message_evidence_required");
        sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked',version=version+1 WHERE namespace=$1")
            .bind(format!("{}/synthetic-qiwe-one",store.tenant)).execute(&store.pool).await?;
        assert!(invoke(
            &store,
            request(
                "context",
                "direct",
                "synthetic-chat",
                &direct,
                json!({"purpose":"reply"})
            )
        )
        .await
        .is_err());
        Ok(())
    }
}
