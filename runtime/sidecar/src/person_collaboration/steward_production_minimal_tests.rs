//! Actual persistent service and authenticated Erhua requests, using disposable identities only.
use super::{
    foundation_consumer_tests::*,
    foundation_server::{broker_invoke, dispatch, parse_broker_request},
    model::*,
    store::{workspace_candidates::Query, RuleCommand, RuleEdit, StoreMode},
    Actor, Store,
};
use anyhow::Result;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

fn query(scope: Uuid, purpose: &str) -> Query {
    Query {
        scope,
        kind: "people".into(),
        purpose: purpose.into(),
        ..Default::default()
    }
}
async fn appoint(
    store: &Store,
    owner: &Actor,
    state: &Value,
    person: Uuid,
    scope: Uuid,
) -> Result<Uuid> {
    let mut a = assignment(state, person, scope);
    a.permissions.push(PermissionSetting {
        action: "confirm_knowledge".into(),
        mode: PermissionMode::Autonomous,
        reviewer: None,
    });
    for action in ["designate", "review"] {
        a.permissions.push(PermissionSetting {
            action: action.into(),
            mode: PermissionMode::Autonomous,
            reviewer: None,
        });
    }
    Ok(id(
        &command(store, owner, Change::Assign(Box::new(a))).await?["change"]["collaboration"]
    ))
}
async fn source(
    store: &Store,
    sender: &str,
    chat_type: &str,
    chat: &str,
    verified: bool,
) -> Result<(String, Uuid)> {
    let event = Uuid::new_v4().to_string();
    let raw:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.raw_events(event_id,source,subject,received_at,payload,ingress_auth_verified) VALUES($1,'qiwe','qintopia.qiwe.raw.authenticated',clock_timestamp(),'{}',$2) RETURNING id").bind(&event).bind(verified).fetch_one(&store.pool).await?;
    let operation:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.messages(tenant_id,platform,message_id,event_id,chat_id,chat_type,sender_id,message_kind,sent_at,received_at,raw_event_id) VALUES($1,'qiwe',$2,$2,$3,$4,$5,'text',clock_timestamp(),clock_timestamp(),$6) RETURNING id").bind(&store.tenant).bind(&event).bind(chat).bind(chat_type).bind(sender).bind(raw).fetch_one(&store.pool).await?;
    Ok((event, operation))
}
async fn invoke(
    store: &Store,
    sender: &str,
    chat_type: &str,
    chat: &str,
    message: &str,
    tool: &str,
    args: Value,
) -> Result<Value> {
    broker_invoke(store,"synthetic-qiwe-one","erhua",parse_broker_request(&serde_json::to_vec(&json!({"operation":"person_foundation_tool","schema_version":1,"agent":"erhua","tool":tool,"trusted_context":{"platform":"qiwe","chat_type":chat_type,"chat_id":chat,"sender_id":sender,"message_id":message,"gateway_id":"synthetic-qiwe-one"},"arguments":args,"token":"simulated-client-token-xxxxxxxxxxxxxxxx"}))?)?).await
}

#[tokio::test]
#[ignore = "explicit disposable PostgreSQL required"]
async fn candidates_paginate_full_directory_and_recheck_authority() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let scope = find(&state, "scopes", "一栋");
    let person = store.verified_person(&owner).await?;
    for n in 0..305 {
        let p:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.persons(display_name,primary_name,preferred_name,status) VALUES($1,$1,$2,'active') RETURNING id").bind(format!("目录候选{n:03}")).bind(format!("微信{n:03}")).fetch_one(&store.pool).await?;
        sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by) VALUES($1,'qiwe_sender',$2,$3,'confirmed',$4,$5)").bind(&store.identity_namespace).bind(format!("candidate-{n}")).bind(p).bind(Uuid::new_v4()).bind(person).execute(&store.pool).await?;
    }
    let mut q = query(scope, "assign");
    q.search = "目录候选".into();
    let first = store.workspace_candidates(&owner, &q).await?;
    assert_eq!(first["items"].as_array().unwrap().len(), 50);
    assert!(first["items"][0]["label"]
        .as_str()
        .unwrap()
        .contains("昵称：微信"));
    let mut seen = std::collections::BTreeSet::new();
    loop {
        let page = store.workspace_candidates(&owner, &q).await?;
        for item in page["items"].as_array().unwrap() {
            assert!(seen.insert(id(&item["ref"])));
        }
        q.after = page["next_cursor"].as_str().map(str::to_owned);
        if q.after.is_none() {
            break;
        }
    }
    assert_eq!(seen.len(), 305);
    q.limit = Some(51);
    assert_eq!(
        store
            .workspace_candidates(&owner, &q)
            .await
            .unwrap_err()
            .to_string(),
        "invalid_candidate_query"
    );
    q.limit = None;
    q.search = "不存在的人".into();
    assert_eq!(
        store.workspace_candidates(&owner, &q).await?["items"],
        json!([])
    );
    q.after = first["next_cursor"].as_str().map(str::to_owned);
    assert_eq!(
        store
            .workspace_candidates(&owner, &q)
            .await
            .unwrap_err()
            .to_string(),
        "invalid_candidate_query"
    );
    q.search = "目录候选".into();
    sqlx::query(
        "UPDATE qintopia_agent_os.collaboration_grants SET status='revoked' WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .execute(&store.pool)
    .await?;
    assert_eq!(
        store
            .workspace_candidates(&owner, &q)
            .await
            .unwrap_err()
            .to_string(),
        "scope_access_denied"
    );
    assert!(Query::from_path(&format!(
        "/api/workspace/candidates?scope={scope}&kind=people&purpose=assign&limit=10&limit=50"
    ))
    .is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit disposable PostgreSQL required"]
async fn live_rules_share_versions_and_route_one_erhua_to_two_buildings() -> Result<()> {
    let (mut store, owner, state) = fixture().await?;
    let one = find(&state, "scopes", "一栋");
    let two = find(&state, "scopes", "二栋");
    let root = find(&state, "scopes", "秦托邦");
    let resident = store
        .conversation_actor("synthetic-qiwe-one", "synthetic-resident")
        .await?;
    let p1 = store.verified_person(&resident).await?;
    let p2 = state["people"]
        .as_array()
        .unwrap()
        .iter()
        .map(|p| id(&p["id"]))
        .find(|p| *p != p1 && *p != owner.person_ref())
        .unwrap();
    let c1 = appoint(&store, &owner, &state, p1, one).await?;
    let _c2 = appoint(&store, &owner, &state, p2, two).await?;
    let namespace:String=sqlx::query_scalar("SELECT namespace FROM qintopia_identity.person_identity_gateways WHERE tenant_key=$1 AND gateway_key='synthetic-qiwe-one'").bind(&store.tenant).fetch_one(&store.pool).await?;
    sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by) VALUES($1,'qiwe_sender','simulated-house-two',$2,'confirmed',$3,$2)").bind(&namespace).bind(p2).bind(Uuid::new_v4()).execute(&store.pool).await?;
    sqlx::query("UPDATE qintopia_identity.person_identity_gateways SET scope_id=$2,version=version+1 WHERE tenant_key=$1 AND gateway_key='synthetic-qiwe-one'").bind(&store.tenant).bind(root).execute(&store.pool).await?;
    sqlx::query(
        "UPDATE qintopia_agent_os.collaboration_tenants SET mode='live' WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .execute(&store.pool)
    .await?;
    sqlx::query("UPDATE qintopia_messages.conversations SET platform='qiwe' WHERE tenant_id=$1")
        .bind(&store.tenant)
        .execute(&store.pool)
        .await?;
    store.mode = StoreMode::Live;
    store
        .preflight_erhua_gateway("synthetic-qiwe-one")
        .await
        .map_err(|e| e.context("gateway preflight"))?;
    let status = store.state(&owner).await?;
    assert_eq!(status["foundation_available"], true);
    assert_eq!(status["dialogue_available"], false);
    assert_eq!(status["runtime_connected"], false);
    for (sender, scope) in [("synthetic-resident", one), ("simulated-house-two", two)] {
        let (message, op) = source(&store, sender, "direct", "simulated-direct", true).await?;
        let workspace = invoke(
            &store,
            sender,
            "direct",
            "simulated-direct",
            &message,
            "workspace",
            json!({}),
        )
        .await
        .map_err(|e| e.context("live workspace"))?;
        assert_eq!(workspace["operation_id"], json!(op));
        assert_eq!(workspace["knowledge"]["context"]["scope"], json!(scope));
        let args = json!({"operation_id":op,"expected_version":0,"key":"kitchen","change":{"action":"save","content":{"title":"厨房约定","text":format!("{sender} 晚上十点关闭")}}});
        let saved = invoke(
            &store,
            sender,
            "direct",
            "simulated-direct",
            &message,
            "change_knowledge",
            args.clone(),
        )
        .await?;
        assert_eq!(saved["status"], "saved");
        assert_eq!(
            invoke(
                &store,
                sender,
                "direct",
                "simulated-direct",
                &message,
                "change_knowledge",
                args
            )
            .await?["replayed"],
            true
        );
        let context = invoke(
            &store,
            sender,
            "direct",
            "simulated-direct",
            &message,
            "context",
            json!({}),
        )
        .await?;
        assert_eq!(context["knowledge"].as_array().unwrap().len(), 1);
        assert!(invoke(
            &store,
            sender,
            "direct",
            "simulated-direct",
            &message,
            "welcome_setting",
            json!({})
        )
        .await
        .is_err());
        assert!(invoke(&store,sender,"direct","simulated-direct",&message,"change_knowledge",json!({"operation_id":Uuid::new_v4(),"expected_version":1,"key":"kitchen","change":{"action":"stop"}})).await.is_err());
        let ui_actor = actor(&store, if scope == one { p1 } else { p2 }).await?;
        let owner_ui = dispatch(
            &store,
            &ui_actor,
            "/api/foundation/rules",
            &serde_json::to_vec(&json!({"scope":scope}))?,
        )
        .await?;
        assert_eq!(owner_ui["items"][0]["version"], 1);
        let stop = RuleCommand {
            operation_id: Uuid::new_v4(),
            scope,
            key: "kitchen".into(),
            kind: "rule".into(),
            expected_version: 1,
            change: RuleEdit::Stop,
        };
        dispatch(
            &store,
            &ui_actor,
            "/api/foundation/rule/change",
            &serde_json::to_vec(&stop)?,
        )
        .await?;
        assert_eq!(
            invoke(
                &store,
                sender,
                "direct",
                "simulated-direct",
                &message,
                "context",
                json!({})
            )
            .await?["knowledge"],
            json!([])
        );
    }
    let groups=sqlx::query("SELECT b.scope_id,c.chat_id,b.id FROM qintopia_agent_os.collaboration_scope_bindings b JOIN qintopia_messages.conversations c ON c.id=b.conversation_id WHERE b.tenant_key=$1 AND b.revoked_at IS NULL AND b.scope_id=ANY($2)").bind(&store.tenant).bind(vec![one,two]).fetch_all(&store.pool).await?;
    for row in &groups {
        let scope: Uuid = row.get("scope_id");
        let chat: String = row.get("chat_id");
        let sender = if scope == one {
            "synthetic-resident"
        } else {
            "simulated-house-two"
        };
        let (message, _) = source(&store, sender, "group", &chat, true).await?;
        let context = invoke(
            &store,
            sender,
            "group",
            &chat,
            &message,
            "context",
            json!({}),
        )
        .await?;
        assert_eq!(context["scope"], json!(scope));
        assert_eq!(context["identity"], json!({"identity_status":"confirmed"}));
        assert!(invoke(
            &store,
            sender,
            "group",
            &chat,
            &message,
            "history",
            json!({})
        )
        .await
        .is_err());
    }
    let row = groups
        .iter()
        .find(|r| r.get::<Uuid, _>("scope_id") == one)
        .unwrap();
    let route = store
        .route_erhua_actor(
            store
                .conversation_actor("synthetic-qiwe-one", "synthetic-resident")
                .await?,
            "group",
            &row.get::<String, _>("chat_id"),
        )
        .await?;
    assert!(store
        .rule_command(
            &route,
            &RuleCommand {
                operation_id: Uuid::new_v4(),
                scope: two,
                key: "cross".into(),
                kind: "rule".into(),
                expected_version: 0,
                change: RuleEdit::Stop
            }
        )
        .await
        .is_err());
    sqlx::query("UPDATE qintopia_agent_os.collaboration_scope_bindings SET revoked_at=clock_timestamp(),version=version+1 WHERE id=$1").bind(row.get::<Uuid,_>("id")).execute(&store.pool).await?;
    assert!(store
        .foundation_context(&route, one, "general")
        .await
        .is_err());
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked' WHERE collaboration_id=$1").bind(c1).execute(&store.pool).await?;
    let (message, operation) = source(
        &store,
        "synthetic-resident",
        "direct",
        "simulated-direct",
        true,
    )
    .await?;
    assert!(invoke(&store,"synthetic-resident","direct","simulated-direct",&message,"change_knowledge",json!({"operation_id":operation,"expected_version":0,"key":"denied","change":{"action":"stop"}})).await.is_err());
    let (unverified, _) = source(
        &store,
        "simulated-house-two",
        "direct",
        "simulated-direct",
        false,
    )
    .await?;
    assert_eq!(
        invoke(
            &store,
            "simulated-house-two",
            "direct",
            "simulated-direct",
            &unverified,
            "context",
            json!({})
        )
        .await
        .unwrap_err()
        .to_string(),
        "trusted_message_evidence_required"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit disposable PostgreSQL required"]
async fn authenticated_http_keeps_formal_rules_independent_and_errors_distinct() -> Result<()> {
    use super::auth_tests::{fixture as auth_fixture, https_request, login, HttpsRequestHeaders};
    let (mut store, owner, state, password) = auth_fixture().await?;
    let scope = find(&state, "scopes", "一栋");
    let person = owner.person_ref();
    appoint(&store, &owner, &state, person, scope).await?;
    sqlx::query(
        "UPDATE qintopia_agent_os.collaboration_tenants SET mode='live' WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .execute(&store.pool)
    .await?;
    store.mode = StoreMode::Live;
    let token = login(&store, "owner", &password).await?;
    let limit_path =
        format!("/api/workspace/candidates?scope={scope}&kind=people&purpose=assign&limit=51");
    let unknown_path = format!(
        "/api/workspace/candidates?scope={}&kind=people&purpose=assign",
        Uuid::new_v4()
    );
    let delegate_path =
        format!("/api/workspace/candidates?scope={scope}&kind=people&purpose=delegate_review");
    let send = |method, path, token, body| {
        https_request(
            &store,
            method,
            path,
            token,
            body,
            HttpsRequestHeaders {
                host: "admin.example.test",
                origin: Some("https://admin.example.test"),
                content_type: "application/json",
                extra: "",
            },
        )
    };
    let path = format!(
        "/api/workspace/candidates?scope={scope}&kind=people&purpose=assign&search=NoSuchPerson"
    );
    assert_eq!(send("GET", &path, None, Value::Null).await?.0, 401);
    let (status, body, _) = send("GET", &path, Some(token.as_str()), Value::Null).await?;
    assert_eq!(status, 200);
    assert_eq!(body["items"], json!([]));
    assert_eq!(
        send("GET", &limit_path, Some(token.as_str()), Value::Null)
            .await?
            .0,
        400
    );
    let (status, body, _) = send("GET", &unknown_path, Some(token.as_str()), Value::Null).await?;
    assert_eq!(status, 403);
    assert_eq!(body["code"], "scope_access_denied");
    let (status, body, _) = send("GET", &delegate_path, Some(token.as_str()), Value::Null).await?;
    assert_eq!(status, 503);
    assert_eq!(body["code"], "candidate_source_unavailable");
    // A bound but disabled source with no history must not look like an empty roster.
    let source = format!("simulated-steward-source-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_sources(source_instance,property_id,mode,rebuilding,enabled) VALUES($1,'property_a','synthetic',false,false)").bind(&source).execute(&store.pool).await?;
    let target: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_targets(source_instance,property_id,kind,building_code,display_name,namespace,conversation_ref,enabled) VALUES($1,'property_a','building','one','模拟一栋目标','qiwe',$2::text,true) RETURNING id").bind(&source).bind(id(&state["groups"][0]["id"])).fetch_one(&store.pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_foundation_targets(target_id,tenant_key,scope_id) VALUES($1,$2,$3)").bind(target).bind(&store.tenant).bind(scope).execute(&store.pool).await?;
    assert_eq!(
        send("GET", &delegate_path, Some(token.as_str()), Value::Null)
            .await?
            .0,
        503
    );
    sqlx::query(
        "UPDATE qintopia_agent_os.welcome_sources SET enabled=true WHERE source_instance=$1",
    )
    .bind(&source)
    .execute(&store.pool)
    .await?;
    let (status, body, _) = send("GET", &delegate_path, Some(token.as_str()), Value::Null).await?;
    assert_eq!(status, 200);
    assert_eq!(body["items"], json!([]));
    sqlx::query(
        "UPDATE qintopia_agent_os.welcome_sources SET rebuilding=true WHERE source_instance=$1",
    )
    .bind(&source)
    .execute(&store.pool)
    .await?;
    assert_eq!(
        send("GET", &delegate_path, Some(token.as_str()), Value::Null)
            .await?
            .0,
        503
    );
    sqlx::query("UPDATE qintopia_agent_os.welcome_sources SET rebuilding=false,mode='live' WHERE source_instance=$1").bind(&source).execute(&store.pool).await?;
    assert_eq!(
        send("GET", &delegate_path, Some(token.as_str()), Value::Null)
            .await?
            .0,
        503
    );
    // An invalid projection is reported on a person row, not in global diagnostics.
    sqlx::query(
        "UPDATE qintopia_agent_os.welcome_sources SET mode='synthetic' WHERE source_instance=$1",
    )
    .bind(&source)
    .execute(&store.pool)
    .await?;
    sqlx::query("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by) VALUES($1,'pms_occupant','simulated-occupant',$2,'confirmed',$3,$2)").bind(format!("pms/{source}/property_a/occupant")).bind(person).bind(Uuid::new_v4()).execute(&store.pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_source_versions(source_instance,property_id,aggregate_type,aggregate_id,revision,projection_hash,projection,invalidated) VALUES($1,'property_a','order','simulated-order',1,'simulated-hash','{}',true)").bind(&source).execute(&store.pool).await?;
    sqlx::query("INSERT INTO qintopia_identity.person_stay_building_history(source_instance,property_id,stay_id,occupant_id,building_code,order_id,source_revision) VALUES($1,'property_a','simulated-stay','simulated-occupant','one','simulated-order',1)").bind(&source).execute(&store.pool).await?;
    assert_eq!(
        send("GET", &delegate_path, Some(token.as_str()), Value::Null)
            .await?
            .0,
        503
    );
    let (status, body, _) = send(
        "POST",
        "/api/foundation/rules",
        Some(token.as_str()),
        json!({"scope":scope}),
    )
    .await?;
    assert_eq!(status, 200);
    assert!(body["context"]["permissions"].is_array());
    let command = json!({"operation_id":Uuid::new_v4(),"scope":scope,"key":"http_rule","expected_version":0,"change":{"action":"save","content":{"title":"本栋约定","text":"晚十点关闭厨房"}}});
    let (status, body, _) = send(
        "POST",
        "/api/foundation/rule/change",
        Some(token.as_str()),
        command,
    )
    .await?;
    assert_eq!(status, 200);
    assert_eq!(body["status"], "saved");
    let (_, readback, _) = send(
        "POST",
        "/api/foundation/rules",
        Some(token.as_str()),
        json!({"scope":scope}),
    )
    .await?;
    assert_eq!(readback["items"][0]["version"], 1);
    assert_eq!(
        send(
            "POST",
            "/api/foundation/talk",
            Some(token.as_str()),
            json!({})
        )
        .await?
        .0,
        403
    );
    assert_eq!(
        send(
            "POST",
            "/api/foundation/welcome",
            Some(token.as_str()),
            json!({})
        )
        .await?
        .0,
        403
    );
    let groups = store
        .workspace_candidates(
            &owner,
            &Query {
                scope,
                kind: "groups".into(),
                purpose: "set_groups".into(),
                ..Default::default()
            },
        )
        .await?;
    assert!(!groups["items"].as_array().unwrap().is_empty());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit disposable PostgreSQL required"]
async fn contact_candidates_keep_people_work_accounts_and_channels_distinct() -> Result<()> {
    let (mut store, owner, state) = fixture().await?;
    let scope = find(&state, "scopes", "一栋");
    let user = store
        .conversation_actor("synthetic-qiwe-one", "synthetic-resident")
        .await?;
    let person = user.person_ref();
    let mut service = assignment(&state, person, scope);
    service.permissions.push(PermissionSetting {
        action: "publish".into(),
        mode: PermissionMode::Autonomous,
        reviewer: None,
    });
    let relation = id(
        &command(&store, &owner, Change::Assign(Box::new(service))).await?["change"]
            ["collaboration"],
    );
    let mut management = assignment(&state, person, scope);
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
    let management_relation = id(
        &command(&store, &owner, Change::Assign(Box::new(management))).await?["change"]
            ["collaboration"],
    );
    let audience: Audience = serde_json::from_value(
        json!({"groups":[],"people":[person],"residents":"none","reply":"autonomous","proactive":"denied","reviewer":null,"visibility":"general","topics":"本栋日常交流"}),
    )?;
    command(
        &store,
        &owner,
        Change::SetAudience {
            collaboration: relation,
            audience,
        },
    )
    .await?;
    // A second permitted channel and the same person's foreign-scope channel are distinct.
    let permitted_namespace = format!("{}/simulated-local-personal", store.tenant);
    let foreign_namespace = format!("{}/simulated-foreign-personal", store.tenant);
    let foreign_scope = find(&state, "scopes", "二栋");
    let mut forbidden_links = Vec::new();
    for (namespace, gateway, gateway_scope) in [
        (&permitted_namespace, "simulated-local-personal", scope),
        (
            &foreign_namespace,
            "simulated-foreign-personal",
            foreign_scope,
        ),
    ] {
        sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'wecom_internal',$4,'personal',true)").bind(&store.tenant).bind(gateway).bind(namespace).bind(gateway_scope).execute(&store.pool).await?;
        let link:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by,adapter_metadata) VALUES($1,'wecom_internal',$2,$3,'confirmed',$4,$5,jsonb_build_object('first_observation_ref',$4::text)) RETURNING id").bind(namespace).bind(gateway).bind(person).bind(Uuid::new_v4()).bind(owner.person_ref()).fetch_one(&store.pool).await?;
        if gateway_scope == foreign_scope {
            forbidden_links.push(link);
        }
    }
    let source_namespace = format!("{}/simulated-shared", store.tenant);
    sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,'simulated-service',$2,'wecom_internal',$3,'shared',true)").bind(&store.tenant).bind(&source_namespace).bind(scope).execute(&store.pool).await?;
    let link:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,adapter_metadata) VALUES($1,'wecom_internal','simulated-service-account',jsonb_build_object('first_observation_ref',$2::text)) RETURNING id").bind(&source_namespace).bind(Uuid::new_v4().to_string()).fetch_one(&store.pool).await?;
    let account:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.work_accounts(tenant_key,source_link_id,source_version,gateway_key,gateway_version,label,verified_by,evidence_ref) VALUES($1,$2,1,'simulated-service',1,'模拟小客服',$3,$4) RETURNING id").bind(&store.tenant).bind(link).bind(owner.person_ref()).bind(Uuid::new_v4()).fetch_one(&store.pool).await?;
    let canonical_namespace = format!("simulated-canonical/{}", Uuid::new_v4());
    sqlx::query(
        "UPDATE qintopia_identity.source_identity_links SET namespace=$2 WHERE namespace=$1",
    )
    .bind(&store.identity_namespace)
    .bind(&canonical_namespace)
    .execute(&store.pool)
    .await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_tenants SET identity_namespace=$2,mode='live' WHERE tenant_key=$1").bind(&store.tenant).bind(&canonical_namespace).execute(&store.pool).await?;
    store.identity_namespace = canonical_namespace;
    store.mode = StoreMode::Live;
    let user = store
        .conversation_actor("synthetic-qiwe-one", "synthetic-resident")
        .await?;
    let people = store
        .workspace_candidates(&user, &query(scope, "contact"))
        .await?;
    assert_eq!(people["items"].as_array().unwrap().len(), 1);
    assert_eq!(people["items"][0]["ref"], json!(person));
    let accounts = store
        .workspace_candidates(
            &user,
            &Query {
                scope,
                kind: "accounts".into(),
                purpose: "contact".into(),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(accounts["items"][0]["ref"], json!(account));
    assert_ne!(accounts["items"][0]["ref"], json!(person));
    let channels = store
        .workspace_candidates(
            &user,
            &Query {
                scope,
                kind: "channels".into(),
                purpose: "contact".into(),
                subject_kind: Some("person".into()),
                subject_ref: Some(person),
                ..Default::default()
            },
        )
        .await?;
    let platforms: std::collections::BTreeSet<_> = channels["items"]
        .as_array()
        .unwrap()
        .iter()
        .map(|c| c["platform"].as_str().unwrap())
        .collect();
    assert_eq!(
        platforms,
        std::collections::BTreeSet::from(["qiwe", "wecom"])
    );
    for foreign in &forbidden_links {
        assert!(!channels["items"]
            .as_array()
            .unwrap()
            .iter()
            .any(|c| c["ref"] == json!(foreign)));
    }
    assert!(!serde_json::to_string(&channels)?.contains("二栋"));
    for channel in channels["items"].as_array().unwrap() {
        assert_eq!(channel["subject_id"], json!(person));
        assert_eq!(channel["channel_source_link_id"], channel["ref"]);
    }
    let work = store
        .workspace_candidates(
            &user,
            &Query {
                scope,
                kind: "channels".into(),
                purpose: "contact".into(),
                subject_kind: Some("work_account".into()),
                subject_ref: Some(account),
                ..Default::default()
            },
        )
        .await?;
    assert_eq!(work["items"][0]["ref"], json!(link));
    assert_eq!(work["items"][0]["subject_kind"], "work_account");
    {
        let restricted = std::env::var("QINTOPIA_MANAGEMENT_UI_TEST_DATABASE_URL")
            .ok()
            .filter(|v| !v.trim().is_empty());
        let selection_store = Store {
            pool: if let Some(url) = restricted {
                sqlx::PgPool::connect(&url).await?
            } else {
                store.pool.clone()
            },
            tenant: store.tenant.clone(),
            identity_namespace: store.identity_namespace.clone(),
            mode: StoreMode::Live,
        };
        let owner_link: Uuid = sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed' ORDER BY id LIMIT 1").bind(&store.identity_namespace).bind(owner.person_ref()).fetch_one(&store.pool).await?;
        let owner_live = store.actor(owner_link).await?;
        store
            .bootstrap_account(
                owner_live.person_ref(),
                "simulated-owner",
                "simulated-contact-password",
            )
            .await?;
        store
            .account_command(
                &owner_live,
                &super::store::AccountCommand::Create {
                    person,
                    username: "simulated-steward".into(),
                    password: "simulated-contact-password".into(),
                },
            )
            .await?;
        let token = selection_store
            .login(&super::store::Credentials {
                username: "simulated-steward".into(),
                password: "simulated-contact-password".into(),
            })
            .await?;
        let user = selection_store.session_actor(&token).await?;
        // A steward uses organization rights, without any PMS/anan grant or staff group.
        assert_eq!(
            selection_store.business_configuration_state(&user).await?["manageable_scopes"],
            json!([])
        );
        let person_channel = id(&channels["items"][0]["ref"]);
        let mut audience: Audience = serde_json::from_value(
            json!({"groups":[],"people":[person],"residents":"none","reply":"autonomous","proactive":"denied","reviewer":null,"visibility":"general","topics":"本栋生活联系","contacts":[{"subject_kind":"person","subject_id":person,"channel_source_link_id":person_channel},{"subject_kind":"work_account","subject_id":account,"channel_source_link_id":link}]}),
        )?;
        let make = |audience: Audience, version: i64| Command {
            operation_id: Uuid::new_v4(),
            expected_version: version,
            change: Change::SetAudience {
                collaboration: relation,
                audience,
            },
        };
        let cmd = make(
            audience.clone(),
            selection_store.state(&user).await?["version"]
                .as_i64()
                .unwrap(),
        );
        assert_eq!(
            selection_store.command(&user, &cmd, false).await?["persisted"],
            false
        );
        assert_eq!(
            selection_store.audience_preview(&user, relation).await?["contacts"],
            json!([])
        );
        assert_eq!(
            selection_store.command(&user, &cmd, true).await?["persisted"],
            true
        );
        assert_eq!(
            selection_store
                .command(&user, &cmd, true)
                .await
                .unwrap_err()
                .to_string(),
            "command_already_processed_refresh_state"
        );
        let saved = selection_store.audience_preview(&user, relation).await?;
        assert_eq!(saved["contacts"].as_array().unwrap().len(), 2);
        assert_eq!(saved["contacts_current"], true);
        assert_eq!(saved["contacts"][1]["subject_label"], "模拟小客服");
        assert_eq!(
            store
                .contact_decision(&user, relation, "channel", link, false)
                .await?["status"],
            "autonomous"
        );
        let mut forged = audience.clone();
        forged.contacts[0].channel_source_link_id = forbidden_links[0];
        let bad = make(
            forged,
            selection_store.state(&user).await?["version"]
                .as_i64()
                .unwrap(),
        );
        assert_eq!(
            store
                .command(&user, &bad, true)
                .await
                .unwrap_err()
                .to_string(),
            "contact_outside_scope"
        );
        let stale = make(audience.clone(), cmd.expected_version);
        assert!(selection_store.command(&user, &stale, true).await.is_err());
        audience.contacts.remove(0);
        command(
            &selection_store,
            &user,
            Change::SetAudience {
                collaboration: relation,
                audience: audience.clone(),
            },
        )
        .await?;
        assert_eq!(
            selection_store.audience_preview(&user, relation).await?["contacts"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        sqlx::query(
            "UPDATE qintopia_identity.work_accounts SET active=false,version=version+1 WHERE id=$1",
        )
        .bind(account)
        .execute(&selection_store.pool)
        .await?;
        assert_eq!(
            selection_store.audience_preview(&user, relation).await?["contacts_current"],
            false
        );
        assert_eq!(
            store
                .contact_decision(&user, relation, "channel", link, false)
                .await?["reason"],
            "contact_source_changed_or_revoked"
        );
        let unavailable = make(
            audience.clone(),
            selection_store.state(&user).await?["version"]
                .as_i64()
                .unwrap(),
        );
        assert!(selection_store
            .command(&user, &unavailable, true)
            .await
            .is_err());
        audience.contacts.clear();
        command(
            &selection_store,
            &user,
            Change::SetAudience {
                collaboration: relation,
                audience: audience.clone(),
            },
        )
        .await?;
        assert_eq!(
            selection_store
                .command(&user, &cmd, true)
                .await
                .unwrap_err()
                .to_string(),
            "command_already_processed_refresh_state"
        );
        assert_eq!(
            selection_store.audience_preview(&user, relation).await?["contacts"],
            json!([])
        );
        let manager_grant: Uuid = sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_grants WHERE collaboration_id=$1 AND action_key='manage'").bind(management_relation).fetch_one(&selection_store.pool).await?;
        let revoked_version = selection_store.state(&user).await?["version"]
            .as_i64()
            .unwrap();
        sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked',version=version+1 WHERE id=$1").bind(manager_grant).execute(&selection_store.pool).await?;
        let revoked = make(audience, revoked_version);
        assert_eq!(
            store
                .command(&user, &revoked, true)
                .await
                .unwrap_err()
                .to_string(),
            "management_denied"
        );
        sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='active',version=version+1 WHERE id=$1").bind(manager_grant).execute(&selection_store.pool).await?;
        // Restore simulated fixture sources for the remaining independent directory checks.
        sqlx::query(
            "UPDATE qintopia_identity.work_accounts SET active=true,version=version+1 WHERE id=$1",
        )
        .bind(account)
        .execute(&selection_store.pool)
        .await?;
    }
    // A missing resident source affects people, not independently verified work accounts.
    sqlx::query("UPDATE qintopia_agent_os.collaboration_audiences SET configuration=jsonb_set(configuration,'{residents}','\"current\"') WHERE tenant_key=$1 AND collaboration_id=$2").bind(&store.tenant).bind(relation).execute(&store.pool).await?;
    assert_eq!(
        store
            .workspace_candidates(&user, &query(scope, "contact"))
            .await
            .unwrap_err()
            .to_string(),
        "candidate_source_unavailable"
    );
    assert_eq!(
        store
            .workspace_candidates(
                &user,
                &Query {
                    scope,
                    kind: "accounts".into(),
                    purpose: "contact".into(),
                    ..Default::default()
                }
            )
            .await?["items"][0]["ref"],
        json!(account)
    );
    assert_eq!(
        store
            .workspace_candidates(
                &user,
                &Query {
                    scope,
                    kind: "channels".into(),
                    purpose: "contact".into(),
                    subject_kind: Some("work_account".into()),
                    subject_ref: Some(account),
                    ..Default::default()
                }
            )
            .await?["items"][0]["ref"],
        json!(link)
    );
    let json = serde_json::to_string(&channels)?;
    assert!(!json.contains("source_ref"));
    assert!(!json.contains("sender_id"));
    assert!(!json.contains("phone"));
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked',version=version+1 WHERE id=$1").bind(link).execute(&store.pool).await?;
    assert_eq!(
        store
            .workspace_candidates(
                &user,
                &Query {
                    scope,
                    kind: "accounts".into(),
                    purpose: "contact".into(),
                    ..Default::default()
                }
            )
            .await?["items"],
        json!([])
    );
    Ok(())
}

/// The caller is the explicit Linux-only root test, never an automatic PG-tier case.
#[cfg(target_os = "linux")]
pub(crate) async fn linux_production_probe() -> Result<()> {
    use anyhow::ensure;
    use std::{os::unix::fs::PermissionsExt, path::Path};
    ensure!(
        std::env::var("QINTOPIA_FOUNDATION_LINUX_PROBE").as_deref() == Ok("1"),
        "explicit_linux_probe_required"
    );
    let effective_uid = std::process::Command::new("id").arg("-u").output()?;
    ensure!(
        effective_uid.stdout == b"0\n",
        "isolated_linux_root_required"
    );
    let _ = tracing_subscriber::fmt()
        .with_max_level(tracing::Level::INFO)
        .try_init();
    let (mut store, owner, state) = fixture().await?;
    let one = find(&state, "scopes", "一栋");
    let two = find(&state, "scopes", "二栋");
    let user = store
        .conversation_actor("synthetic-qiwe-one", "synthetic-resident")
        .await?;
    let relation = appoint(&store, &owner, &state, user.person_ref(), one).await?;
    let foreign_chat = format!("simulated-linux-foreign-{}", Uuid::new_v4());
    let conversation:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type,display_name) VALUES($1,'qiwe',$2,'group','模拟其他栋') RETURNING id").bind(&store.tenant).bind(&foreign_chat).fetch_one(&store.pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3)").bind(&store.tenant).bind(two).bind(conversation).execute(&store.pool).await?;
    sqlx::query(
        "UPDATE qintopia_agent_os.collaboration_tenants SET mode='live' WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .execute(&store.pool)
    .await?;
    store.mode = StoreMode::Live;
    let mut events = serde_json::Map::new();
    for key in [
        "context",
        "save",
        "update",
        "stale",
        "stop",
        "memory_general",
        "memory_fees",
        "memory_stop",
        "forged",
        "revoked",
        "drain",
    ] {
        let (message, operation) = source(
            &store,
            "synthetic-resident",
            "direct",
            "simulated-linux-direct",
            key != "forged",
        )
        .await?;
        events.insert(key.into(), json!({"message":message,"operation":operation}));
    }
    let root_path = format!("/run/steward-probe-{}", Uuid::new_v4());
    let root = Path::new(&root_path);
    std::fs::create_dir(root)?;
    std::fs::set_permissions(root, std::fs::Permissions::from_mode(0o755))?;
    let socket_dir = root.join("broker");
    std::fs::create_dir(&socket_dir)?;
    ensure!(
        std::process::Command::new("chown")
            .args(["0:2001", socket_dir.to_str().unwrap()])
            .status()?
            .success(),
        "socket_owner_setup_failed"
    );
    std::fs::set_permissions(&socket_dir, std::fs::Permissions::from_mode(0o750))?;
    let endpoint = socket_dir.join("foundation.sock");
    let token = format!("simulated-{}", Uuid::new_v4());
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    // These are local process flags in a network-isolated, task-owned container only.
    for (key, value) in [
        ("QINTOPIA_FOUNDATION_PRODUCTION_ENABLE", "1".into()),
        ("QINTOPIA_FOUNDATION_PROFILE", "erhua".into()),
        (
            "QINTOPIA_FOUNDATION_GATEWAY_ID",
            "synthetic-qiwe-one".into(),
        ),
        (
            "QINTOPIA_FOUNDATION_ERHUA_APPROVAL",
            "steward-foundation-reviewed".into(),
        ),
        ("QINTOPIA_FOUNDATION_DATABASE_URL", database.clone()),
        (
            "QINTOPIA_FOUNDATION_DATABASE_URL_SHA256",
            super::digest(database.as_bytes()),
        ),
        (
            "QINTOPIA_FOUNDATION_TOKEN_SHA256",
            super::digest(token.as_bytes()),
        ),
        ("QINTOPIA_FOUNDATION_RUNNER_UID", "2001".into()),
        ("QINTOPIA_FOUNDATION_RUNNER_GID", "2001".into()),
        (
            "QINTOPIA_FOUNDATION_SOCKET",
            endpoint.to_str().unwrap().into(),
        ),
    ] {
        std::env::set_var(key, value);
    }
    std::env::remove_var("QINTOPIA_FOUNDATION_LOCAL_ENABLE");
    // A non-runner foreign owner with otherwise valid group/mode must fail
    // before creating a lock or listener, without inferring trust from its UID.
    std::os::unix::fs::chown(&socket_dir, Some(2002), None)?;
    let foreign_owner = super::foundation_server::broker_live(Store {
        pool: store.pool.clone(),
        tenant: store.tenant.clone(),
        identity_namespace: store.identity_namespace.clone(),
        mode: StoreMode::Live,
    })
    .await
    .unwrap_err();
    assert_eq!(
        foreign_owner.to_string(),
        "isolated_foundation_runner_required"
    );
    assert!(!endpoint.exists());
    assert!(!endpoint.with_extension("lock").exists());
    std::os::unix::fs::chown(&socket_dir, Some(0), None)?;
    println!("foreign_parent_owner_rejected_before_listener=true");
    let mut broker = tokio::spawn(super::foundation_server::broker_live(Store {
        pool: store.pool.clone(),
        tenant: store.tenant.clone(),
        identity_namespace: store.identity_namespace.clone(),
        mode: StoreMode::Live,
    }));
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !endpoint.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let repo = Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("../..")
        .canonicalize()?;
    let core = std::env::var("QINTOPIA_HERMES_CORE_DIR")?;
    let core_sha = std::env::var("QINTOPIA_HERMES_CORE_SHA")?;
    let input = root.join("input.json");
    std::fs::write(
        &input,
        serde_json::to_vec(
            &json!({"events":events,"foreign_scope":two,"foreign_chat":foreign_chat,"sdk":repo.join("skills/person-foundation/__init__.py"),"core_sha":core_sha,"uids":{"positive":2001,"revoked":2001,"same_uid":0,"wrong_uid":2002,"drain_timeout":2001,"drain_replay":2001}}),
        )?,
    )?;
    let runtime_secret = root.join("broker-only.env");
    std::fs::write(&runtime_secret, "simulated broker-only credential\n")?;
    std::fs::set_permissions(&runtime_secret, std::fs::Permissions::from_mode(0o600))?;
    ensure!(
        !std::process::Command::new("runuser")
            .args([
                "-u",
                "steward-runner",
                "--",
                "test",
                "-r",
                runtime_secret.to_str().unwrap()
            ])
            .status()?
            .success(),
        "client_read_broker_secret"
    );
    let run = |phase: &str| {
        let mut cmd = std::process::Command::new("runuser");
        let user = match phase {
            "same_uid" => "root",
            "wrong_uid" => "steward-foreign",
            _ => "steward-runner",
        };
        let profile = root.join(format!("profile-{phase}"));
        std::fs::create_dir(&profile).unwrap();
        std::fs::create_dir(profile.join("plugins")).unwrap();
        std::os::unix::fs::symlink(
            repo.join("skills/qintopia-tools/variants/erhua"),
            profile.join("plugins/qintopia-tools"),
        )
        .unwrap();
        std::fs::write(
            profile.join("config.yaml"),
            "plugins:\n  enabled: [qintopia-tools]\n",
        )
        .unwrap();
        assert!(std::process::Command::new("chown")
            .args([
                "-R",
                "--no-dereference",
                &format!("{user}:steward-runner"),
                profile.to_str().unwrap(),
            ])
            .status()
            .unwrap()
            .success());
        cmd.args([
            "-u",
            user,
            "--",
            "/opt/official-hermes-venv/bin/python",
            "-I",
            repo.join("skills/person-foundation/tests/linux_production_probe.py")
                .to_str()
                .unwrap(),
            "--input",
            input.to_str().unwrap(),
            "--phase",
            phase,
        ]);
        cmd.env_clear()
            .env(
                "PATH",
                "/opt/official-hermes-venv/bin:/usr/sbin:/usr/bin:/bin",
            )
            .env("PYTHONDONTWRITEBYTECODE", "1")
            .env("HERMES_HOME", profile)
            .env("QINTOPIA_PROFILE_ID", "erhua")
            .env("QINTOPIA_HERMES_CORE_DIR", &core)
            .env("QINTOPIA_FOUNDATION_PRODUCTION_ENABLE", "1")
            .env("QINTOPIA_FOUNDATION_PROFILE", "erhua")
            .env("QINTOPIA_FOUNDATION_SOCKET", &endpoint)
            .env("QINTOPIA_FOUNDATION_TOKEN", &token)
            .env("QINTOPIA_FOUNDATION_BROKER_UID", "0")
            .env("QINTOPIA_FOUNDATION_GATEWAY_ID", "synthetic-qiwe-one");
        cmd
    };
    for phase in ["positive", "same_uid", "wrong_uid"] {
        let mut child = run(phase);
        let output = tokio::time::timeout(
            std::time::Duration::from_secs(90),
            tokio::task::spawn_blocking(move || child.output()),
        )
        .await???;
        ensure!(
            output.status.success(),
            "linux_probe_{phase}: {}",
            String::from_utf8_lossy(&output.stderr)
        );
        println!("{}", String::from_utf8_lossy(&output.stdout));
    }
    let ui = dispatch(
        &store,
        &user,
        "/api/foundation/rules",
        &serde_json::to_vec(&json!({"scope":one}))?,
    )
    .await?;
    let rule = ui["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == "linux-probe")
        .unwrap();
    assert_eq!(rule["version"], 3);
    assert!(!rule["stopped_at"].is_null());
    // Hold the actual tenant row so the accepted broker transaction waits in PostgreSQL.
    let mut held = store.pool.begin().await?;
    sqlx::query("SELECT version FROM qintopia_agent_os.collaboration_tenants WHERE tenant_key=$1 FOR UPDATE").bind(&store.tenant).fetch_one(&mut *held).await?;
    let mut timeout_client = run("drain_timeout");
    let timeout_task = tokio::task::spawn_blocking(move || timeout_client.output());
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        loop {
            let waiting: bool = sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM pg_stat_activity WHERE datname=current_database() AND wait_event_type='Lock' AND query LIKE '%collaboration_tenants%FOR UPDATE%')").fetch_one(&store.pool).await?;
            if waiting { break; }
            tokio::task::yield_now().await;
        }
        Ok::<_, anyhow::Error>(())
    }).await??;
    let timeout_output =
        tokio::time::timeout(std::time::Duration::from_secs(20), timeout_task).await???;
    ensure!(
        timeout_output.status.success(),
        "linux_probe_client_timeout: {}",
        String::from_utf8_lossy(&timeout_output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&timeout_output.stdout));
    ensure!(
        !broker.is_finished(),
        "client_timeout_cancelled_database_work"
    );
    ensure!(
        std::process::Command::new("kill")
            .args(["-TERM", &std::process::id().to_string()])
            .status()?
            .success(),
        "termination_signal_failed"
    );
    tokio::time::timeout(std::time::Duration::from_secs(5), async {
        loop {
            if tokio::net::UnixStream::connect(&endpoint).await.is_err() {
                break;
            }
            tokio::task::yield_now().await;
        }
    })
    .await?;
    // Wait beyond the real 30s deadline: the accepted transaction must stay alive.
    ensure!(
        tokio::time::timeout(std::time::Duration::from_secs(31), &mut broker)
            .await
            .is_err(),
        "drain_deadline_cancelled_work"
    );
    ensure!(!broker.is_finished(), "drain_deadline_killed_broker");
    held.rollback().await?;
    tokio::time::timeout(std::time::Duration::from_secs(10), broker).await???;
    ensure!(!endpoint.exists(), "drained_socket_not_removed");
    let readback = dispatch(
        &store,
        &owner,
        "/api/foundation/rules",
        &serde_json::to_vec(&json!({"scope":one}))?,
    )
    .await?;
    let drain_rule = readback["items"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["key"] == "linux-drain")
        .unwrap();
    assert_eq!(drain_rule["version"], 1);
    println!("actual_database_wait=true client_timeout=outcome_unknown stop_accepting=true deadline_deferred=true accepted_commit_preserved=true");
    broker = tokio::spawn(super::foundation_server::broker_live(Store {
        pool: store.pool.clone(),
        tenant: store.tenant.clone(),
        identity_namespace: store.identity_namespace.clone(),
        mode: StoreMode::Live,
    }));
    tokio::time::timeout(std::time::Duration::from_secs(10), async {
        while !endpoint.exists() {
            tokio::task::yield_now().await;
        }
    })
    .await?;
    let mut replay_client = run("drain_replay");
    let replay = tokio::time::timeout(
        std::time::Duration::from_secs(30),
        tokio::task::spawn_blocking(move || replay_client.output()),
    )
    .await???;
    ensure!(
        replay.status.success(),
        "linux_probe_drain_replay: {}",
        String::from_utf8_lossy(&replay.stderr)
    );
    println!("{}", String::from_utf8_lossy(&replay.stdout));
    let operation: Uuid = serde_json::from_value(events["drain"]["operation"].clone())?;
    let applies: i64 = sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_rule_events WHERE tenant_key=$1 AND operation_id=$2").bind(&store.tenant).bind(operation).fetch_one(&store.pool).await?;
    assert_eq!(applies, 1);
    println!("original_operation_apply_count=1 receipt_recovered=true");
    let grant:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_grants WHERE tenant_key=$1 AND collaboration_id=$2 AND action_key='change_rules' AND status='active'").bind(&store.tenant).bind(relation).fetch_one(&store.pool).await?;
    command(&store, &owner, Change::RevokeGrant { grant }).await?;
    let mut child = run("revoked");
    let output = tokio::time::timeout(
        std::time::Duration::from_secs(90),
        tokio::task::spawn_blocking(move || child.output()),
    )
    .await???;
    ensure!(
        output.status.success(),
        "linux_probe_revoked: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    println!("{}", String::from_utf8_lossy(&output.stdout));
    let after = dispatch(
        &store,
        &owner,
        "/api/foundation/rules",
        &serde_json::to_vec(&json!({"scope":one}))?,
    )
    .await?;
    assert_eq!(
        after["items"]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["key"] == "linux-probe")
            .unwrap()["version"],
        3
    );
    println!("actual_ui_service_readback=passed revoked_and_peer_attempts_changed_state=false");
    ensure!(
        std::process::Command::new("kill")
            .args(["-TERM", &std::process::id().to_string()])
            .status()?
            .success(),
        "termination_signal_failed"
    );
    tokio::time::timeout(std::time::Duration::from_secs(10), broker).await???;
    println!("idle_broker_shutdown=drained");
    Ok(())
}
