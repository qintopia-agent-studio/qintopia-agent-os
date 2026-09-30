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
    let relation = appoint(&store, &owner, &state, person, scope).await?;
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
