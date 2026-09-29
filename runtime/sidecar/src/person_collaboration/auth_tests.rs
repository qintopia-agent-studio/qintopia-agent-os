//! Real password/HTTP/Store checks in unique synthetic tenants; no external clients.
#![cfg(feature = "postgres-integration-tests")]
use super::{
    model::*,
    store::{AccountCommand, Credentials},
    Actor, Store,
};
use anyhow::Result;
use serde_json::{json, Value};
use tokio::{
    io::{AsyncReadExt, AsyncWriteExt},
    net::{TcpListener, TcpStream},
};
use uuid::Uuid;

fn id(v: &Value) -> Uuid {
    serde_json::from_value(v.clone()).unwrap()
}
fn find(s: &Value, key: &str, label: &str) -> Uuid {
    id(&s[key]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["label"] == label)
        .unwrap()["id"])
}
fn password() -> String {
    format!("Synthetic-{}", Uuid::new_v4())
}
async fn fixture() -> Result<(Store, Actor, Value, String)> {
    // Share the HTTP fixtures' once-only gate before taking the baseline snapshot.
    // Account lifecycle assertions still compare the entire authorization state.
    super::foundation_server::enable_test_http();
    let db = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let store = Store::local(
        &db,
        &format!("synthetic-collaboration-login-{}", Uuid::new_v4()),
    )
    .await?;
    crate::db::run_migrations(&store.pool).await?;
    let owner = store.actor(store.bootstrap_fixture().await?).await?;
    let state = store.state(&owner).await?;
    let pass = password();
    store
        .bootstrap_account(find(&state, "people", "合成负责人"), "owner", &pass)
        .await?;
    Ok((store, owner, state, pass))
}
async fn login(store: &Store, name: &str, pass: &str) -> Result<String> {
    store
        .login(&Credentials {
            username: name.into(),
            password: pass.into(),
        })
        .await
}
async fn create(
    store: &Store,
    owner: &Actor,
    person: Uuid,
    name: &str,
    pass: &str,
) -> Result<Uuid> {
    Ok(id(&store
        .account_command(
            owner,
            &AccountCommand::Create {
                person,
                username: name.into(),
                password: pass.into(),
            },
        )
        .await?["id"]))
}
pub(super) async fn request(
    store: &Store,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
    origin: bool,
) -> Result<(u16, Value, String)> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let port = listener.local_addr()?.port();
    let payload = if method == "POST" {
        body.to_string()
    } else {
        String::new()
    };
    let headers=format!("{method} {path} HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\nContent-Type: application/json\r\nContent-Length: {}\r\n{}{}\r\n{payload}",payload.len(),
        if origin {format!("Origin: http://127.0.0.1:{port}\r\n")} else {"Origin: https://untrusted.invalid\r\n".into()},
        token.map(|t|format!("Cookie: collaboration-session={t}\r\n")).unwrap_or_default());
    let server = async {
        let (mut stream, _) = listener.accept().await?;
        super::auth_server::handle(&mut stream, store, port).await
    };
    let client = async {
        let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
        stream.write_all(headers.as_bytes()).await?;
        let mut response = String::new();
        stream.read_to_string(&mut response).await?;
        let (head, data) = response.split_once("\r\n\r\n").unwrap();
        let status = head.split_whitespace().nth(1).unwrap().parse::<u16>()?;
        let cookie = head
            .lines()
            .find_map(|s| s.strip_prefix("Set-Cookie: collaboration-session="))
            .map(|s| s.split(';').next().unwrap().to_string())
            .unwrap_or_default();
        if !cookie.is_empty() {
            assert!(head.contains("HttpOnly; SameSite=Strict"));
        }
        Ok::<_, anyhow::Error>((
            status,
            serde_json::from_str(data).unwrap_or(json!(data)),
            cookie,
        ))
    };
    let (server, client) = tokio::join!(server, client);
    server?;
    client
}

struct HttpsRequestHeaders<'a> {
    host: &'a str,
    origin: Option<&'a str>,
    content_type: &'a str,
    extra: &'a str,
}

async fn https_request(
    store: &Store,
    method: &str,
    path: &str,
    token: Option<&str>,
    body: Value,
    request_headers: HttpsRequestHeaders<'_>,
) -> Result<(u16, Value, String)> {
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let port = listener.local_addr()?.port();
    let payload = if method == "POST" && !body.is_null() {
        body.to_string()
    } else {
        String::new()
    };
    let headers = format!(
        "{method} {path} HTTP/1.1\r\nHost: {}\r\nContent-Type: {}\r\nContent-Length: {}\r\n{}{}{}\r\n{payload}",
        request_headers.host,
        request_headers.content_type,
        payload.len(),
        request_headers
            .origin
            .map(|o| format!("Origin: {o}\r\n"))
            .unwrap_or_default(),
        token.map(|t| format!("Cookie: collaboration-session={t}\r\n")).unwrap_or_default(),
        request_headers.extra,
    );
    let configured = super::auth_server::UiOrigin::production("https://admin.example.test")?;
    let server = async {
        let (mut stream, _) = listener.accept().await?;
        super::auth_server::handle_with_origin(&mut stream, store, &configured).await
    };
    let client = async {
        let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
        stream.write_all(headers.as_bytes()).await?;
        stream.shutdown().await?;
        let mut response = String::new();
        stream
            .read_to_string(&mut response)
            .await
            .map_err(|error| {
                anyhow::anyhow!(
                    "https_request_failed: {method} {path} {}: {error}",
                    request_headers.host
                )
            })?;
        let (head, data) = response.split_once("\r\n\r\n").unwrap();
        let status = head.split_whitespace().nth(1).unwrap().parse::<u16>()?;
        Ok::<_, anyhow::Error>((
            status,
            serde_json::from_str(data).unwrap_or(json!(data)),
            head.to_string(),
        ))
    };
    let (server, client) = tokio::join!(server, client);
    server?;
    client
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn production_https_origin_and_cookie_contract_preserves_live_authorization() -> Result<()> {
    let (store, owner, state, pass) = fixture().await?;
    let host = "admin.example.test";
    let origin = Some("https://admin.example.test");
    let login_body = json!({"username":"owner","password":pass});
    let send = |method, path, token, body, host, origin, content_type, extra_headers| {
        https_request(
            &store,
            method,
            path,
            token,
            body,
            HttpsRequestHeaders {
                host,
                origin,
                content_type,
                extra: extra_headers,
            },
        )
    };
    let before = business_snapshot(&store).await?;
    assert_eq!(
        send(
            "GET",
            "/api/state",
            None,
            json!({}),
            host,
            origin,
            "application/json",
            ""
        )
        .await?
        .0,
        401
    );
    let (status, page, _) = send(
        "GET",
        "/",
        None,
        json!({}),
        host,
        origin,
        "application/json",
        "",
    )
    .await?;
    assert_eq!(status, 200);
    assert!(page.as_str().unwrap().contains("/auth.js"));
    assert_eq!(
        send(
            "GET",
            "/workbench.js",
            None,
            json!({}),
            host,
            origin,
            "application/json",
            ""
        )
        .await?
        .0,
        200
    );
    for (bad_host, bad_origin, content_type, extra, expected) in [
        ("127.0.0.1", origin, "application/json", "X-Forwarded-Host: admin.example.test\r\nForwarded: host=admin.example.test;proto=https\r\n", 400),
        (host, Some("https://other.example.test"), "application/json", "X-Forwarded-Host: admin.example.test\r\nX-Forwarded-Proto: https\r\n", 403),
        (host, None, "application/json", "", 403),
        (host, origin, "text/plain", "", 403),
        (host, origin, "application/json", "Origin: https://admin.example.test\r\n", 400),
        (host, origin, "application/json", "Host: admin.example.test\r\n", 400),
    ] {
        let body = if expected == 400 {
            Value::Null
        } else {
            login_body.clone()
        };
        assert_eq!(
            send("POST", "/api/login", None, body, bad_host, bad_origin, content_type, extra)
                .await?
                .0,
            expected
        );
    }
    assert_eq!(business_snapshot(&store).await?, before);
    let (status, _, headers) = send(
        "POST",
        "/api/login",
        None,
        login_body,
        host,
        origin,
        "application/json",
        "",
    )
    .await?;
    assert_eq!(status, 200);
    let cookie = headers
        .lines()
        .find_map(|line| line.strip_prefix("Set-Cookie: collaboration-session="))
        .unwrap();
    assert!(cookie.contains("; HttpOnly; SameSite=Strict; Path=/; Secure"));
    assert!(!cookie.contains("Domain="));
    let token = cookie.split(';').next().unwrap();
    assert_eq!(
        send(
            "GET",
            "/api/state",
            Some(token),
            json!({}),
            host,
            origin,
            "application/json",
            ""
        )
        .await?
        .0,
        200
    );
    let member = find(&state, "people", "人员甲 · 合成样例 A");
    let member_pass = password();
    create(&store, &owner, member, "member", &member_pass).await?;
    let member_token = login(&store, "member", &member_pass).await?;
    let before_denied = business_snapshot(&store).await?;
    for (method, body) in [
        ("GET", json!({})),
        ("POST", json!({"kind":"disable","account":Uuid::new_v4()})),
    ] {
        let (status, _, _) = send(
            method,
            "/api/accounts",
            Some(&member_token),
            body,
            host,
            origin,
            "application/json",
            "",
        )
        .await?;
        assert_eq!(status, 403);
    }
    assert_eq!(business_snapshot(&store).await?, before_denied);
    let (status, _, headers) = send(
        "POST",
        "/api/logout",
        Some(token),
        json!({}),
        host,
        origin,
        "application/json",
        "",
    )
    .await?;
    assert_eq!(status, 200);
    assert!(headers.contains(
        "Set-Cookie: collaboration-session=; HttpOnly; SameSite=Strict; Path=/; Secure; Max-Age=0"
    ));
    assert!(store.session_actor(token).await.is_err());
    let token = login(&store, "owner", &pass).await?;
    let (status, _, headers) = send(
        "POST",
        "/api/password",
        Some(&token),
        json!({"current_password":pass,"new_password":password()}),
        host,
        origin,
        "application/json",
        "",
    )
    .await?;
    assert_eq!(status, 200);
    assert!(headers.contains(
        "Set-Cookie: collaboration-session=; HttpOnly; SameSite=Strict; Path=/; Secure; Max-Age=0"
    ));
    assert!(store.session_actor(&token).await.is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn live_https_routes_keep_drafts_unverified_and_require_current_management() -> Result<()> {
    let (mut store, owner, state, pass) = fixture().await?;
    sqlx::query(
        "UPDATE qintopia_agent_os.collaboration_tenants SET mode='live' WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .execute(&store.pool)
    .await?;
    store.mode = super::store::StoreMode::Live;
    let token = login(&store, "owner", &pass).await?;
    let host = "admin.example.test";
    let origin = Some("https://admin.example.test");
    let send = |method, path, token, body| {
        https_request(
            &store,
            method,
            path,
            token,
            body,
            HttpsRequestHeaders {
                host,
                origin,
                content_type: "application/json",
                extra: "",
            },
        )
    };
    let root: Uuid = sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND parent_scope_id IS NULL")
        .bind(&store.tenant).fetch_one(&store.pool).await?;
    for path in [
        "/api/state".to_string(),
        "/api/business".to_string(),
        "/api/identities".to_string(),
        format!("/api/ontology?scope={root}"),
        format!(
            "/api/identities?person={}",
            find(&state, "people", "合成负责人")
        ),
    ] {
        assert_eq!(
            https_request(
                &store,
                "GET",
                &path,
                Some(&token),
                json!({}),
                HttpsRequestHeaders {
                    host,
                    origin,
                    content_type: "application/json",
                    extra: "",
                },
            )
            .await?
            .0,
            200,
            "{path}"
        );
    }
    for path in ["/api/contact-decision", "/api/unknown"] {
        assert_eq!(
            send("POST", path, Some(&token), json!({})).await?.1["code"],
            "production_configuration_change_denied"
        );
    }
    let version = store.state(&owner).await?["version"].as_i64().unwrap();
    let command = Command {
        operation_id: Uuid::new_v4(),
        expected_version: version,
        change: Change::SaveLedger {
            id: None,
            object: "person".into(),
            reference: None,
            label: "模拟待核验人员".into(),
            nickname: String::new(),
            description: "模拟待核验来源记录".into(),
            scope: None,
            owner: None,
            draft: false,
        },
    };
    let (status, preview, _) = send("POST", "/api/preview", Some(&token), json!(command)).await?;
    assert_eq!(status, 200, "{preview}");
    let (status, saved, _) = send("POST", "/api/save", Some(&token), json!(command)).await?;
    assert_eq!(status, 200, "{saved}");
    let ledger = id(&saved["change"]["id"]);
    let (draft_person, status, verified): (String, String, bool) = sqlx::query_as(
        "SELECT object_ref,status,verified FROM qintopia_agent_os.collaboration_ledger WHERE id=$1",
    )
    .bind(ledger)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(status, "draft");
    assert!(!verified);
    let draft_person = Uuid::parse_str(&draft_person)?;
    let links: i64 = sqlx::query_scalar("SELECT count(*) FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2")
        .bind(&store.identity_namespace).bind(draft_person).fetch_one(&store.pool).await?;
    assert_eq!(links, 0);
    assert!(store
        .account_command(
            &owner,
            &AccountCommand::Create {
                person: draft_person,
                username: "unverified".into(),
                password: password(),
            }
        )
        .await
        .is_err());
    let draft_assignment = assignment(&state, "人员甲 · 合成样例 A", "一栋");
    let mut draft_assignment = draft_assignment;
    draft_assignment.person = draft_person;
    let assign = Command {
        operation_id: Uuid::new_v4(),
        expected_version: store.state(&owner).await?["version"].as_i64().unwrap(),
        change: Change::Assign(Box::new(draft_assignment)),
    };
    assert_eq!(
        send("POST", "/api/preview", Some(&token), json!(assign))
            .await?
            .1["code"],
        "catalog_not_active"
    );
    let member = find(&state, "people", "人员甲 · 合成样例 A");
    let member_pass = password();
    create(&store, &owner, member, "member", &member_pass).await?;
    let member_token = login(&store, "member", &member_pass).await?;
    let denied = Command {
        operation_id: Uuid::new_v4(),
        expected_version: store.state(&owner).await?["version"].as_i64().unwrap(),
        change: Change::CreateScope {
            parent: root,
            label: "模拟越权范围".into(),
            scope_kind: "building".into(),
        },
    };
    assert_ne!(
        send("POST", "/api/save", Some(&member_token), json!(denied))
            .await?
            .0,
        200
    );
    assert_eq!(
        send(
            "POST",
            "/api/save",
            Some(&token),
            json!(Command {
                operation_id: Uuid::new_v4(),
                expected_version: version,
                change: Change::CreateRole {
                    label: "旧入口".into(),
                    available_actions: vec!["manage".into()],
                },
            })
        )
        .await?
        .1["code"],
        "production_configuration_change_denied"
    );
    for path in [
        "/foundation",
        "/api/foundation/state",
        "/api/foundation/card?id=00000000-0000-0000-0000-000000000000",
    ] {
        assert_eq!(send("GET", path, Some(&token), json!({})).await?.0, 403);
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database and restricted UI role required"]
async fn live_https_restricted_role_runs_nonempty_account_and_read_paths() -> Result<()> {
    let role_url = match std::env::var("QINTOPIA_MANAGEMENT_UI_TEST_DATABASE_URL") {
        Ok(value) => value,
        Err(std::env::VarError::NotPresent) => {
            eprintln!("restricted_management_ui_role_test_not_configured");
            return Ok(());
        }
        Err(error) => return Err(error.into()),
    };
    let (mut admin, _owner, state, pass) = fixture().await?;
    sqlx::query(
        "UPDATE qintopia_agent_os.collaboration_tenants SET mode='live' WHERE tenant_key=$1",
    )
    .bind(&admin.tenant)
    .execute(&admin.pool)
    .await?;
    admin.mode = super::store::StoreMode::Live;
    sqlx::query("DELETE FROM qintopia_agent_os.collaboration_accounts WHERE tenant_key=$1")
        .bind(&admin.tenant)
        .execute(&admin.pool)
        .await?;
    let restricted = Store {
        pool: crate::db::connect(&role_url, 3).await?,
        tenant: admin.tenant.clone(),
        identity_namespace: admin.identity_namespace.clone(),
        mode: super::store::StoreMode::Live,
    };
    let owner = find(&state, "people", "合成负责人");
    restricted.bootstrap_account(owner, "owner", &pass).await?;
    let token = login(&restricted, "owner", &pass).await?;
    let send = |method, path, body| {
        https_request(
            &restricted,
            method,
            path,
            Some(&token),
            body,
            HttpsRequestHeaders {
                host: "admin.example.test",
                origin: Some("https://admin.example.test"),
                content_type: "application/json",
                extra: "",
            },
        )
    };
    for path in [
        "/api/state",
        "/api/identities",
        "/api/business",
        "/api/accounts",
    ] {
        let (status, body, _) = send("GET", path, json!({})).await?;
        assert_eq!(status, 200, "{path}: {body}");
    }
    let root: Uuid = sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND parent_scope_id IS NULL")
        .bind(&admin.tenant).fetch_one(&admin.pool).await?;
    let ontology_path = format!("/api/ontology?scope={root}");
    let (status, body, _) = send("GET", &ontology_path, json!({})).await?;
    assert_eq!(status, 200, "{body}");
    let person = find(&state, "people", "人员甲 · 合成样例 A");
    let (status, body, _) = send(
        "POST",
        "/api/accounts",
        json!({
            "kind":"create", "person":person, "username":"restricted-member", "password":password()
        }),
    )
    .await?;
    assert_eq!(status, 200, "{body}");
    let account = id(&body["id"]);
    let (status, body, _) = send(
        "POST",
        "/api/accounts",
        json!({"kind":"disable","account":account}),
    )
    .await?;
    assert_eq!(status, 200, "{body}");

    let appointment: Uuid = sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_appointments WHERE tenant_key=$1 AND person_id=$2 AND scope_id=$3")
        .bind(&admin.tenant).bind(owner).bind(root).fetch_one(&admin.pool).await?;
    let collaboration: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key,responsibility_text) VALUES($1,$2,'anan','hospitality','模拟业务管理') RETURNING id")
        .bind(&admin.tenant).bind(appointment).fetch_one(&admin.pool).await?;
    let management_grant: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,include_descendants,managed_agents,managed_domains,managed_actions,delegation_depth) VALUES($1,$2,'manage',true,ARRAY['anan']::text[],ARRAY['hospitality']::text[],ARRAY['read_business','execute_business']::text[],1) RETURNING id")
        .bind(&admin.tenant).bind(collaboration).fetch_one(&admin.pool).await?;
    let (status, business, _) = send("GET", "/api/business", json!({})).await?;
    assert_eq!(status, 200, "{business}");
    assert!(business["manageable_scopes"]
        .as_array()
        .is_some_and(|scopes| !scopes.is_empty()));
    let business_command = |version: i64, change: Value| {
        json!({
            "operation_id":Uuid::new_v4(),"expected_version":version,"change":change
        })
    };
    let version = send("GET", "/api/state", json!({})).await?.1["version"]
        .as_i64()
        .unwrap();
    let binding_command = business_command(
        version,
        json!({
            "kind":"create_binding","scope":root,"source":"simulated-pms","property":"property_a"
        }),
    );
    let (status, preview, _) =
        send("POST", "/api/business/preview", binding_command.clone()).await?;
    assert_eq!(status, 200, "{preview}");
    let (status, saved, _) = send("POST", "/api/business/save", binding_command).await?;
    assert_eq!(status, 200, "{saved}");
    let binding = id(&saved["change"]["binding"]);

    let shared_namespace = format!("{}/shared-{}", admin.tenant, Uuid::new_v4());
    let gateway = format!("simulated-shared-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'wecom_internal',$4,'shared',true)")
        .bind(&admin.tenant).bind(&gateway).bind(&shared_namespace).bind(root).execute(&admin.pool).await?;
    let evidence = Uuid::new_v4();
    let source = format!("simulated-shared-source-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_messages.raw_events(id,event_id,source,subject,received_at,payload,ingress_auth_verified) VALUES($1,$2,'wecom-host','qintopia.wecom.host.observed',clock_timestamp(),$3,false)")
        .bind(evidence).bind(format!("simulated-{evidence}")).bind(json!({"tenant_key":admin.tenant,"gateway_key":gateway,"namespace":shared_namespace,"scope_ref":root,"sender_hash":super::digest(source.as_bytes())})).execute(&admin.pool).await?;
    let source_link: Uuid = sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,adapter_metadata) VALUES($1,'wecom_internal',$2,jsonb_build_object('first_observation_ref',$3::text)) RETURNING id")
        .bind(&shared_namespace).bind(&source).bind(evidence).fetch_one(&admin.pool).await?;
    let version = send("GET", "/api/state", json!({})).await?.1["version"]
        .as_i64()
        .unwrap();
    let command = business_command(
        version,
        json!({"kind":"register_account","gateway":gateway,"source_link":source_link,"label":"模拟共用账号"}),
    );
    let (status, saved, _) = send("POST", "/api/business/save", command).await?;
    assert_eq!(status, 200, "{saved}");
    let work_account = id(&saved["change"]["account"]);
    let version = send("GET", "/api/state", json!({})).await?.1["version"]
        .as_i64()
        .unwrap();
    let command = business_command(
        version,
        json!({"kind":"grant_account_operation","binding":binding,"account":work_account,"role":"operator","operation":"pms.command.RECORD_COLLECTION","valid_until":null}),
    );
    let (status, saved, _) = send("POST", "/api/business/save", command).await?;
    assert_eq!(status, 200, "{saved}");
    let operation_grant = id(&saved["change"]["grant"]);
    let version = send("GET", "/api/state", json!({})).await?.1["version"]
        .as_i64()
        .unwrap();
    let command = business_command(
        version,
        json!({"kind":"revoke_operation","grant":operation_grant}),
    );
    let (status, saved, _) = send("POST", "/api/business/save", command).await?;
    assert_eq!(status, 200, "{saved}");
    let account_version: i64 =
        sqlx::query_scalar("SELECT version FROM qintopia_identity.work_accounts WHERE id=$1")
            .bind(work_account)
            .fetch_one(&admin.pool)
            .await?;
    let version = send("GET", "/api/state", json!({})).await?.1["version"]
        .as_i64()
        .unwrap();
    let command = business_command(
        version,
        json!({"kind":"disable_account","account":work_account,"expected_account_version":account_version}),
    );
    let (status, saved, _) = send("POST", "/api/business/save", command).await?;
    assert_eq!(status, 200, "{saved}");

    let version = send("GET", "/api/state", json!({})).await?.1["version"]
        .as_i64()
        .unwrap();
    let draft = json!({"operation_id":Uuid::new_v4(),"expected_version":version,
        "change":{"kind":"save_ledger","id":null,"object":"person","reference":null,
        "label":"模拟待核验员工","nickname":"","description":"模拟企微来源核验","scope":null,"owner":null,"draft":false}});
    let (status, saved, _) = send("POST", "/api/save", draft).await?;
    assert_eq!(status, 200, "{saved}");
    let ledger = id(&saved["change"]["id"]);
    let person_ref: String = sqlx::query_scalar(
        "SELECT object_ref FROM qintopia_agent_os.collaboration_ledger WHERE id=$1",
    )
    .bind(ledger)
    .fetch_one(&admin.pool)
    .await?;
    let person_ref = Uuid::parse_str(&person_ref)?;
    let employee_gateway = format!("simulated-employee-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'wecom_internal',$4,'employee',true)")
        .bind(&admin.tenant).bind(&employee_gateway).bind(&admin.identity_namespace).bind(root).execute(&admin.pool).await?;
    let employee_source = format!("simulated-employee-{}", Uuid::new_v4());
    let employee_evidence = Uuid::new_v4();
    sqlx::query("INSERT INTO qintopia_messages.raw_events(id,event_id,source,subject,received_at,payload,ingress_auth_verified) VALUES($1,$2,'wecom-host','qintopia.wecom.host.observed',clock_timestamp(),$3,false)")
        .bind(employee_evidence).bind(format!("simulated-{employee_evidence}"))
        .bind(json!({"tenant_key":admin.tenant,"gateway_key":employee_gateway,"namespace":admin.identity_namespace,"scope_ref":root,"sender_hash":super::digest(employee_source.as_bytes())})).execute(&admin.pool).await?;
    let employee_link: Uuid = sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,adapter_metadata) VALUES($1,'wecom_internal',$2,jsonb_build_object('first_observation_ref',$3::text)) RETURNING id")
        .bind(&admin.identity_namespace).bind(&employee_source).bind(employee_evidence).fetch_one(&admin.pool).await?;
    let mut registry_lock = restricted.pool.begin().await?;
    sqlx::query("SELECT qintopia_identity.management_ui_lock_gateway_registry()")
        .execute(&mut *registry_lock)
        .await?;
    let mut competing = admin.pool.begin().await?;
    sqlx::query("SET LOCAL lock_timeout='100ms'")
        .execute(&mut *competing)
        .await?;
    let conflict = sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'wecom_internal',$4,'employee',true)")
        .bind(&admin.tenant).bind(format!("competing-{}",Uuid::new_v4()))
        .bind(&admin.identity_namespace).bind(root).execute(&mut *competing).await.unwrap_err();
    let sqlstate = conflict
        .as_database_error()
        .and_then(|error| error.code())
        .map(|code| code.to_string());
    assert_eq!(sqlstate.as_deref(), Some("55P03"));
    drop(competing);
    registry_lock.rollback().await?;
    let identity_path = format!("/api/identities?person={person_ref}");
    let (status, identities, _) = send("GET", &identity_path, json!({})).await?;
    assert_eq!(status, 200, "{identities}");
    let candidate = identities["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|candidate| candidate["id"] == json!(employee_link))
        .unwrap();
    assert_eq!(candidate["selectable"], true);
    let mut identity_command = json!({"operation_id":Uuid::new_v4(),"link_ref":employee_link,
        "person_ref":person_ref,"expected_version":candidate["version"],
        "expected_configuration_version":identities["version"],
        "expected_gateway_version":candidate["gateway_version"],"revoke":false});
    let (status, preview, _) =
        send("POST", "/api/identities/preview", identity_command.clone()).await?;
    assert_eq!(status, 200, "{preview}");
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET version=version+1 WHERE id=$1")
        .bind(employee_link)
        .execute(&admin.pool)
        .await?;
    let (status, stale, _) = send("POST", "/api/identities/save", identity_command.clone()).await?;
    assert_eq!(status, 409, "{stale}");
    assert_eq!(stale["code"], "identity_version_conflict");
    identity_command["expected_version"] = json!(candidate["version"].as_i64().unwrap() + 1);
    let (status, saved, _) = send("POST", "/api/identities/save", identity_command.clone()).await?;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["saved"], true);
    let (status, replay, _) =
        send("POST", "/api/identities/save", identity_command.clone()).await?;
    assert_eq!(status, 200, "{replay}");
    assert_eq!(replay["replayed"], true);
    assert_eq!(replay["current_state_requires_read"], true);
    assert_eq!(replay["historical_receipt"], saved);

    let group: Uuid = sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type,display_name) VALUES($1,'wecom',$2,'group','模拟运营工作群') RETURNING id")
        .bind(&admin.tenant).bind(format!("simulated-staff-group-{}",Uuid::new_v4()))
        .fetch_one(&admin.pool).await?;
    let group_binding: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3) RETURNING id")
        .bind(&admin.tenant).bind(root).bind(group).fetch_one(&admin.pool).await?;
    let duty: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_duties(tenant_key,label,description,domain_key) VALUES($1,$2,'模拟运营群联系职责','hospitality') RETURNING id")
        .bind(&admin.tenant).bind(format!("模拟联系-{}",Uuid::new_v4()))
        .fetch_one(&admin.pool).await?;
    sqlx::query("UPDATE qintopia_agent_os.agent_collaborations SET duty_id=$2 WHERE id=$1")
        .bind(collaboration)
        .bind(duty)
        .execute(&admin.pool)
        .await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id,decision_mode) VALUES($1,$2,'read_business',$3,'autonomous')")
        .bind(&admin.tenant).bind(collaboration).bind(management_grant)
        .execute(&admin.pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_audiences(tenant_key,collaboration_id,configuration) VALUES($1,$2,$3)")
        .bind(&admin.tenant).bind(collaboration)
        .bind(json!({"proactive":"autonomous","groups":[group]}))
        .execute(&admin.pool).await?;
    let qiwe_namespace = format!("{}/qiwe-{}", admin.tenant, Uuid::new_v4());
    let qiwe_gateway = format!("simulated-qiwe-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'qiwe_sender',$4,'personal',true)")
        .bind(&admin.tenant).bind(&qiwe_gateway).bind(&qiwe_namespace).bind(root)
        .execute(&admin.pool).await?;
    let qiwe_link: Uuid = sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by,adapter_metadata) VALUES($1,'qiwe_sender',$2,$3,'confirmed',$4,$5,jsonb_build_object('first_observation_ref',$6::text)) RETURNING id")
        .bind(&qiwe_namespace).bind(format!("simulated-qiwe-source-{}",Uuid::new_v4()))
        .bind(person_ref).bind(Uuid::new_v4()).bind(owner).bind(Uuid::new_v4())
        .fetch_one(&admin.pool).await?;
    let groups_path = format!("/api/business/candidates?scope={root}&kind=groups&limit=50");
    let (status, groups, _) = send("GET", &groups_path, json!({})).await?;
    assert_eq!(status, 200, "{groups}");
    assert!(groups["groups"].as_array().is_some_and(|items| items
        .iter()
        .any(|item| item["binding_id"] == json!(group_binding))));
    let contacts_path = format!("/api/business/candidates?scope={root}&kind=contacts&limit=50");
    let (status, contacts, _) = send("GET", &contacts_path, json!({})).await?;
    assert_eq!(status, 200, "{contacts}");
    for link in [employee_link, qiwe_link] {
        assert!(contacts["contacts"].as_array().is_some_and(|items| items
            .iter()
            .any(|item| item["channel_source_link_id"] == json!(link))));
    }
    let version = send("GET", "/api/state", json!({})).await?.1["version"]
        .as_i64()
        .unwrap();
    let communication = business_command(
        version,
        json!({
            "kind":"set_scope_communication","scope":root,
            "staff_group_binding_id":group_binding,
            "contacts":[
                {"subject_kind":"person","subject_id":person_ref,"channel_source_link_id":employee_link},
                {"subject_kind":"person","subject_id":person_ref,"channel_source_link_id":qiwe_link}
            ]
        }),
    );
    let (status, preview, _) = send("POST", "/api/business/preview", communication.clone()).await?;
    assert_eq!(status, 200, "{preview}");
    assert_eq!(preview["persisted"], false);
    let (status, saved, _) = send("POST", "/api/business/save", communication).await?;
    assert_eq!(status, 200, "{saved}");
    assert_eq!(saved["persisted"], true);
    assert_eq!(
        saved["change"]["communication"]["contacts"]
            .as_array()
            .map(Vec::len),
        Some(2)
    );
    let current = restricted
        .scope_communication_current(&employee_gateway, binding)
        .await?;
    assert_eq!(current["current"], true);
    assert_eq!(current["contacts"].as_array().map(Vec::len), Some(2));
    assert_eq!(current["contacts"][0]["platform"], "wecom");
    assert_eq!(current["contacts"][1]["platform"], "qiwe");

    let person_password = password();
    let (status, created, _) = send("POST", "/api/accounts", json!({
        "kind":"create","person":person_ref,"username":"reviewed-employee","password":person_password
    })).await?;
    assert_eq!(status, 200, "{created}");
    let person_token = login(&restricted, "reviewed-employee", &person_password).await?;
    let welcome_source = format!("simulated-welcome-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_sources(source_instance,property_id,mode) VALUES($1,'property_a','synthetic')")
        .bind(&welcome_source).execute(&admin.pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_identity_scopes(namespace,source_instance,property_id) VALUES($1,$2,'property_a')")
        .bind(&admin.identity_namespace).bind(&welcome_source).execute(&admin.pool).await?;
    let application: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_applications(source_instance,resource_ref,record_ref,person_id,revision,field_hash) VALUES($1,$2,$3,$4,1,$5) RETURNING id")
        .bind(&welcome_source).bind(format!("resource-{}",Uuid::new_v4()))
        .bind(format!("record-{}",Uuid::new_v4())).bind(person_ref).bind("simulated-hash")
        .fetch_one(&admin.pool).await?;
    let reviewed_version: i64 = sqlx::query_scalar(
        "SELECT version FROM qintopia_identity.source_identity_links WHERE id=$1",
    )
    .bind(employee_link)
    .fetch_one(&admin.pool)
    .await?;
    let welcome_case: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_cases(source_instance,property_id,order_id,stay_id,occupant_id,person_id,identity_link_id,identity_version,application_id) VALUES($1,'property_a',$2,$3,$4,$5,$6,$7,$8) RETURNING id")
        .bind(&welcome_source).bind(format!("order-{}",Uuid::new_v4()))
        .bind(format!("stay-{}",Uuid::new_v4())).bind(format!("occupant-{}",Uuid::new_v4()))
        .bind(person_ref).bind(employee_link).bind(reviewed_version).bind(application)
        .fetch_one(&admin.pool).await?;
    let unrelated_case: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_cases(source_instance,property_id,order_id,stay_id,occupant_id,person_id) VALUES($1,'property_a',$2,$3,$4,$5) RETURNING id")
        .bind(&welcome_source).bind(format!("order-{}",Uuid::new_v4()))
        .bind(format!("stay-{}",Uuid::new_v4())).bind(format!("occupant-{}",Uuid::new_v4()))
        .bind(owner).fetch_one(&admin.pool).await?;
    let work_item: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.work_items(work_item_type,requester_agent,target_agent,capability_key,brief_summary,dedupe_key,idempotency_key) VALUES('welcome_publish','erhua','erhua','resident_welcome.coordinate','模拟欢迎待执行',$1,$1) RETURNING id")
        .bind(format!("simulated-{}",Uuid::new_v4())).fetch_one(&admin.pool).await?;
    let artifact: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.artifacts(work_item_id,artifact_type,created_by_agent) VALUES($1,'welcome_card','erhua') RETURNING id")
        .bind(work_item).fetch_one(&admin.pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_artifact_bindings(artifact_id,case_id,application_id,application_revision,case_version,consent_version,template_version) VALUES($1,$2,$3,1,1,0,'simulated-v1')")
        .bind(artifact).bind(welcome_case).bind(application).execute(&admin.pool).await?;
    let target: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_targets(source_instance,property_id,kind,display_name,namespace,conversation_ref) VALUES($1,'property_a','building','模拟群',$2,$3) RETURNING id")
        .bind(&welcome_source).bind(&admin.identity_namespace)
        .bind(format!("conversation-{}",Uuid::new_v4())).fetch_one(&admin.pool).await?;
    let publish_grant: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_grants(person_id,source_instance,property_id,target_id,action,expires_at,appointed_by) VALUES($1,$2,'property_a',$3,'publish',clock_timestamp()+interval '1 hour',$1) RETURNING id")
        .bind(owner).bind(&welcome_source).bind(target).fetch_one(&admin.pool).await?;
    let approval: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_approvals(artifact_id,target_id,target_version,phase,content_hash,grant_id,grant_version,approved_by) VALUES($1,$2,1,'formal','simulated-hash',$3,1,$4) RETURNING id")
        .bind(artifact).bind(target).bind(publish_grant).bind(owner).fetch_one(&admin.pool).await?;
    let action: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.welcome_actions(case_id,target_id,phase,part,work_item_id,approval_id,publish_grant_id,publish_grant_version,target_version,execution_epoch) VALUES($1,$2,'formal','text',$3,$4,$5,1,1,0) RETURNING id")
        .bind(welcome_case).bind(target).bind(work_item).bind(approval).bind(publish_grant)
        .fetch_one(&admin.pool).await?;
    let (status, identities, _) = send("GET", &identity_path, json!({})).await?;
    assert_eq!(status, 200, "{identities}");
    let reviewed = identities["links"]
        .as_array()
        .unwrap()
        .iter()
        .find(|link| link["id"] == json!(employee_link))
        .unwrap();
    identity_command["operation_id"] = json!(Uuid::new_v4());
    identity_command["expected_version"] = reviewed["version"].clone();
    identity_command["expected_configuration_version"] = identities["version"].clone();
    identity_command["revoke"] = json!(true);
    let revoke_operation = id(&identity_command["operation_id"]);
    let (status, saved, _) = send("POST", "/api/identities/save", identity_command).await?;
    assert_eq!(status, 200, "{saved}");
    assert!(restricted.session_actor(&person_token).await.is_err());
    assert_eq!(
        restricted
            .scope_communication_current(&employee_gateway, binding)
            .await?["current"],
        false
    );
    let case_state: (i64, bool) = sqlx::query_as(
        "SELECT version,manual_hold FROM qintopia_agent_os.welcome_cases WHERE id=$1",
    )
    .bind(welcome_case)
    .fetch_one(&admin.pool)
    .await?;
    assert_eq!(case_state, (3, true));
    let unrelated_state: (i64, bool) = sqlx::query_as(
        "SELECT version,manual_hold FROM qintopia_agent_os.welcome_cases WHERE id=$1",
    )
    .bind(unrelated_case)
    .fetch_one(&admin.pool)
    .await?;
    assert_eq!(unrelated_state, (1, false));
    let artifact_revoked: bool = sqlx::query_scalar("SELECT revoked_at IS NOT NULL FROM qintopia_agent_os.welcome_artifact_bindings WHERE artifact_id=$1")
        .bind(artifact).fetch_one(&admin.pool).await?;
    assert!(artifact_revoked);
    let action_state: (String, i64) =
        sqlx::query_as("SELECT status,version FROM qintopia_agent_os.welcome_actions WHERE id=$1")
            .bind(action)
            .fetch_one(&admin.pool)
            .await?;
    assert_eq!(action_state, ("cancelled".into(), 2));
    let work_status: String =
        sqlx::query_scalar("SELECT status FROM qintopia_agent_os.work_items WHERE id=$1")
            .bind(work_item)
            .fetch_one(&admin.pool)
            .await?;
    assert_eq!(work_status, "cancelled");
    let receipt: Value = sqlx::query_scalar(
        "SELECT result FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1 AND id=$2",
    )
    .bind(&admin.tenant)
    .bind(revoke_operation)
    .fetch_one(&admin.pool)
    .await?;
    assert_eq!(receipt, saved);

    for sql in [
        "UPDATE qintopia_identity.persons SET preferred_name=preferred_name WHERE false",
        "UPDATE qintopia_identity.person_identity_gateways SET version=version WHERE false",
    ] {
        assert!(
            sqlx::query(sql).execute(&restricted.pool).await.is_err(),
            "{sql}"
        );
    }
    let can_create: bool = sqlx::query_scalar(
        "SELECT has_schema_privilege(current_user,'qintopia_identity','CREATE')",
    )
    .fetch_one(&restricted.pool)
    .await?;
    assert!(!can_create);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn live_configuration_commit_failure_keeps_original_receipt_key() -> Result<()> {
    let (mut store, _, _, pass) = fixture().await?;
    sqlx::query(
        "UPDATE qintopia_agent_os.collaboration_tenants SET mode='live' WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .execute(&store.pool)
    .await?;
    store.mode = super::store::StoreMode::Live;
    let token = login(&store, "owner", &pass).await?;
    let actor = store.session_actor(&token).await?;
    let original_version = store.state(&actor).await?["version"].as_i64().unwrap();
    let operation_id = Uuid::new_v4();
    let trigger_name = format!("pr723_commit_failure_{}", Uuid::new_v4().simple());
    let function_name = format!("{trigger_name}_fn");
    let function_sql = format!(
        "CREATE FUNCTION qintopia_agent_os.{function_name}() RETURNS trigger LANGUAGE plpgsql AS $$ BEGIN IF NEW.tenant_key = '{}' THEN RAISE EXCEPTION 'simulated_commit_failure'; END IF; RETURN NEW; END $$",
        store.tenant
    );
    sqlx::query(&function_sql).execute(&store.pool).await?;
    let trigger_sql = format!(
        "CREATE CONSTRAINT TRIGGER {trigger_name} AFTER INSERT ON qintopia_agent_os.collaboration_commands DEFERRABLE INITIALLY DEFERRED FOR EACH ROW EXECUTE FUNCTION qintopia_agent_os.{function_name}()"
    );
    sqlx::query(&trigger_sql).execute(&store.pool).await?;
    let command = Command {
        operation_id,
        expected_version: original_version,
        change: Change::SaveLedger {
            id: None,
            object: "person".into(),
            reference: None,
            label: "模拟提交故障人员".into(),
            nickname: String::new(),
            description: "模拟提交阶段结果不明".into(),
            scope: None,
            owner: None,
            draft: false,
        },
    };
    let result = store.command(&actor, &command, true).await;
    let drop_trigger =
        format!("DROP TRIGGER {trigger_name} ON qintopia_agent_os.collaboration_commands");
    sqlx::query(&drop_trigger).execute(&store.pool).await?;
    let drop_function = format!("DROP FUNCTION qintopia_agent_os.{function_name}()");
    sqlx::query(&drop_function).execute(&store.pool).await?;
    assert_eq!(
        result.unwrap_err().to_string(),
        "configuration_commit_outcome_unknown"
    );
    let receipt: Option<Uuid> = sqlx::query_scalar(
        "SELECT id FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1 AND id=$2",
    )
    .bind(&store.tenant)
    .bind(operation_id)
    .fetch_optional(&store.pool)
    .await?;
    assert!(receipt.is_none());
    assert_eq!(store.state(&actor).await?["version"], original_version);
    let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.tool_invocation_audit WHERE tool_name='collaboration.configure' AND input_summary->>'command_ref'=$1")
        .bind(operation_id.to_string()).fetch_one(&store.pool).await?;
    assert_eq!(audit_count, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated PostgreSQL and loopback ack-loss proxy required"]
async fn live_configuration_postcommit_ack_loss_reads_original_receipt_once() -> Result<()> {
    if std::env::var("QINTOPIA_COLLABORATION_ACK_LOSS_TEST_ENABLE").as_deref() != Ok("1") {
        eprintln!("postcommit_ack_loss_test_not_configured");
        return Ok(());
    }
    let (mut store, _, _, pass) = fixture().await?;
    sqlx::query(
        "UPDATE qintopia_agent_os.collaboration_tenants SET mode='live' WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .execute(&store.pool)
    .await?;
    store.mode = super::store::StoreMode::Live;
    let token = login(&store, "owner", &pass).await?;
    let actor = store.session_actor(&token).await?;
    let original_version = store.state(&actor).await?["version"].as_i64().unwrap();
    let original_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .fetch_one(&store.pool)
    .await?;
    let operation_id = Uuid::new_v4();
    let command = Command {
        operation_id,
        expected_version: original_version,
        change: Change::SaveLedger {
            id: None,
            object: "person".into(),
            reference: None,
            label: "模拟已提交但确认丢失人员".into(),
            nickname: String::new(),
            description: "模拟提交确认丢失".into(),
            scope: None,
            owner: None,
            draft: false,
        },
    };

    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let mut proxy_url = url::Url::parse(&database)?;
    let database_port = proxy_url
        .port()
        .filter(|port| *port != 5432)
        .ok_or_else(|| anyhow::anyhow!("dedicated_postgres_port_required"))?;
    let proxy_listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    proxy_url
        .set_port(Some(proxy_listener.local_addr()?.port()))
        .map_err(|_| anyhow::anyhow!("invalid_ack_loss_proxy_port"))?;
    proxy_url.set_query(Some("sslmode=disable"));
    let (ack_tx, ack_rx) = tokio::sync::oneshot::channel::<()>();
    let (close_tx, close_rx) = tokio::sync::oneshot::channel::<()>();
    let proxy = tokio::spawn(async move {
        let (client, _) = proxy_listener.accept().await?;
        let server = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, database_port)).await?;
        let (mut client_read, mut client_write) = client.into_split();
        let (mut server_read, mut server_write) = server.into_split();
        let (mut client_bytes, mut server_bytes) = ([0_u8; 8192], [0_u8; 8192]);
        let (mut request_tail, mut withheld) = (Vec::new(), Vec::new());
        let mut commit_seen = false;
        loop {
            tokio::select! {
                biased;
                read = client_read.read(&mut client_bytes) => {
                    let count = read?;
                    anyhow::ensure!(count > 0, "client_closed_before_commit_ack");
                    if !commit_seen {
                        request_tail.extend_from_slice(&client_bytes[..count]);
                        commit_seen = request_tail.windows(7).any(|bytes| bytes == b"COMMIT\0");
                        if request_tail.len() > 32 {
                            request_tail.drain(..request_tail.len() - 32);
                        }
                    }
                    server_write.write_all(&client_bytes[..count]).await?;
                }
                read = server_read.read(&mut server_bytes) => {
                    let count = read?;
                    anyhow::ensure!(count > 0, "server_closed_before_commit_ack");
                    if !commit_seen {
                        client_write.write_all(&server_bytes[..count]).await?;
                        continue;
                    }
                    withheld.extend_from_slice(&server_bytes[..count]);
                    anyhow::ensure!(withheld.len() <= 1024, "unexpected_commit_response");
                    let committed = withheld.windows(12).any(|bytes| bytes == b"C\0\0\0\x0bCOMMIT\0");
                    let ready = withheld.windows(6).any(|bytes| bytes == b"Z\0\0\0\x05I");
                    if committed && ready {
                        ack_tx.send(()).map_err(|_| anyhow::anyhow!("ack_observer_gone"))?;
                        let _ = close_rx.await;
                        return Ok::<_, anyhow::Error>(());
                    }
                }
            }
        }
    });
    let proxied = Store {
        pool: crate::db::connect(proxy_url.as_str(), 1).await?,
        tenant: store.tenant.clone(),
        identity_namespace: store.identity_namespace.clone(),
        mode: super::store::StoreMode::Live,
    };
    let origin = super::auth_server::UiOrigin::production("https://admin.example.test")?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let port = listener.local_addr()?.port();
    let application = tokio::spawn(async move {
        super::serve_production_ui_until(listener, &proxied, &origin, std::future::pending()).await
    });
    let body = json!(command).to_string();
    let request = format!("POST /api/save HTTP/1.1\r\nHost: admin.example.test\r\nOrigin: https://admin.example.test\r\nContent-Type: application/json\r\nCookie: collaboration-session={token}\r\nContent-Length: {}\r\n\r\n{body}", body.len());
    let browser = tokio::spawn(async move {
        let mut stream = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
        stream.write_all(request.as_bytes()).await?;
        stream.shutdown().await?;
        let mut response = String::new();
        stream.read_to_string(&mut response).await?;
        Ok::<_, anyhow::Error>(response)
    });

    tokio::time::timeout(std::time::Duration::from_secs(5), ack_rx).await??;
    let receipt: Value = sqlx::query_scalar(
        "SELECT result FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1 AND id=$2",
    )
    .bind(&store.tenant)
    .bind(operation_id)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(receipt["version"], original_version + 1);
    assert_eq!(receipt["replayed"], false);
    let audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.tool_invocation_audit WHERE tool_name='collaboration.configure' AND input_summary->>'command_ref'=$1")
        .bind(operation_id.to_string()).fetch_one(&store.pool).await?;
    assert_eq!(audit_count, 1);
    let version: i64 = sqlx::query_scalar(
        "SELECT version FROM qintopia_agent_os.collaboration_tenants WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(version, original_version + 1);
    eprintln!("postcommit_ack_loss_readback operation_id={operation_id} receipt=1 audit=1 version={version}");

    close_tx
        .send(())
        .map_err(|_| anyhow::anyhow!("proxy_closed_before_readback"))?;
    tokio::time::timeout(std::time::Duration::from_secs(5), proxy).await???;
    let response = tokio::time::timeout(std::time::Duration::from_secs(5), browser).await???;
    assert!(response.is_empty(), "{response}");
    let outcome = tokio::time::timeout(std::time::Duration::from_secs(5), application).await??;
    assert_eq!(
        outcome.unwrap_err().to_string(),
        "production_ui_request_outcome_unknown"
    );
    assert!(TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .is_err());
    let final_count: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(final_count, original_count + 1);
    let final_audit_count: i64 = sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.tool_invocation_audit WHERE tool_name='collaboration.configure' AND input_summary->>'command_ref'=$1")
        .bind(operation_id.to_string()).fetch_one(&store.pool).await?;
    assert_eq!(final_audit_count, 1);
    assert_eq!(store.state(&actor).await?["version"], original_version + 1);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn production_listener_recovers_from_slow_reads_and_closes_on_shutdown() -> Result<()> {
    let (mut store, _, _, pass) = fixture().await?;
    sqlx::query(
        "UPDATE qintopia_agent_os.collaboration_tenants SET mode='live' WHERE tenant_key=$1",
    )
    .bind(&store.tenant)
    .execute(&store.pool)
    .await?;
    store.mode = super::store::StoreMode::Live;
    let token = login(&store, "owner", &pass).await?;
    let origin = super::auth_server::UiOrigin::production("https://admin.example.test")?;
    let listener = TcpListener::bind((std::net::Ipv4Addr::LOCALHOST, 0)).await?;
    let port = listener.local_addr()?.port();
    let (stop, stopped) = tokio::sync::oneshot::channel::<()>();
    let server = tokio::spawn(async move {
        super::serve_production_ui_until(listener, &store, &origin, async move {
            let _ = stopped.await;
        })
        .await
    });
    let mut slow = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    slow.write_all(b"GET /api/me HTTP/1.1\r\nHost: admin.example.test\r\n")
        .await?;
    let mut client = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    client
        .write_all(b"GET /api/me HTTP/1.1\r\nHost: admin.example.test\r\n\r\n")
        .await?;
    let mut response = String::new();
    tokio::time::timeout(
        std::time::Duration::from_secs(7),
        client.read_to_string(&mut response),
    )
    .await??;
    assert!(response.starts_with("HTTP/1.1 401"), "{response}");
    let stale = json!(Command {
        operation_id: Uuid::new_v4(),
        expected_version: -1,
        change: Change::SaveLedger {
            id: None,
            object: "person".into(),
            reference: None,
            label: "模拟过期版本".into(),
            nickname: String::new(),
            description: "模拟明确拒绝".into(),
            scope: None,
            owner: None,
            draft: false,
        },
    })
    .to_string();
    let mut rejected = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    rejected.write_all(format!("POST /api/save HTTP/1.1\r\nHost: admin.example.test\r\nOrigin: https://admin.example.test\r\nContent-Type: application/json\r\nCookie: collaboration-session={token}\r\nContent-Length: {}\r\n\r\n{stale}", stale.len()).as_bytes()).await?;
    let mut rejected_response = String::new();
    rejected.read_to_string(&mut rejected_response).await?;
    assert!(
        rejected_response.starts_with("HTTP/1.1 409"),
        "{rejected_response}"
    );
    let mut healthy = TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port)).await?;
    healthy.write_all(format!("GET /api/me HTTP/1.1\r\nHost: admin.example.test\r\nCookie: collaboration-session={token}\r\n\r\n").as_bytes()).await?;
    let mut healthy_response = String::new();
    healthy.read_to_string(&mut healthy_response).await?;
    assert!(
        healthy_response.starts_with("HTTP/1.1 200"),
        "{healthy_response}"
    );
    let _ = stop.send(());
    tokio::time::timeout(std::time::Duration::from_secs(3), server).await???;
    assert!(TcpStream::connect((std::net::Ipv4Addr::LOCALHOST, port))
        .await
        .is_err());
    Ok(())
}
async fn save(store: &Store, owner: &Actor, change: Change) -> Result<Value> {
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
fn assignment(s: &Value, person: &str, scope: &str) -> Assignment {
    Assignment {
        collaboration: None,
        person: find(s, "people", person),
        role: find(s, "roles", "舍长"),
        duty: Some(find(s, "duties", "居民服务")),
        scope: find(s, "scopes", scope),
        agent: "erhua".into(),
        domain: "community_service".into(),
        responsibility: format!("{scope}合成管理"),
        valid_from: None,
        valid_until: None,
        proxy_for: None,
        actions: vec![],
        permissions: vec![PermissionSetting {
            action: "train".into(),
            mode: PermissionMode::Autonomous,
            reviewer: None,
        }],
        delegation: None,
    }
}
async fn business_snapshot(store: &Store) -> Result<Value> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('version',(SELECT version FROM qintopia_agent_os.collaboration_tenants WHERE tenant_key=$1),'commands',(SELECT count(*) FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1),'accounts',(SELECT coalesce(jsonb_agg(to_jsonb(a) ORDER BY id),'[]') FROM qintopia_agent_os.collaboration_accounts a WHERE tenant_key=$1),'grants',(SELECT coalesce(jsonb_agg(to_jsonb(g) ORDER BY id),'[]') FROM qintopia_agent_os.collaboration_grants g WHERE tenant_key=$1))")
        .bind(&store.tenant).fetch_one(&store.pool).await?)
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn password_http_requires_credentials_and_rejects_csrf_and_actor_claims() -> Result<()> {
    let (store, owner, state, pass) = fixture().await?;
    // Exercise the actual password server asset route, not the older fixture-only handler.
    let (asset_status, asset, _) = request(
        &store,
        "GET",
        "/workbench-steward.js",
        None,
        json!({}),
        true,
    )
    .await?;
    assert_eq!(asset_status, 200);
    assert_eq!(asset, json!(include_str!("workbench-steward.js")));
    let before = business_snapshot(&store).await?;
    let (status, data, _) = request(&store, "GET", "/api/state", None, json!({}), true).await?;
    assert_eq!(status, 401);
    assert_eq!(data, json!({"code":"authentication_required"}));
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/login",
            None,
            json!({"username":"owner","password":"wrong"}),
            true
        )
        .await?
        .0,
        401
    );
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/login",
            None,
            json!({"username":"unknown","password":"wrong"}),
            true
        )
        .await?
        .1,
        json!({"code":"invalid_credentials"})
    );
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/login",
            None,
            json!({"username":"owner","password":pass}),
            false
        )
        .await?
        .0,
        403
    );
    let (status, _, token) = request(
        &store,
        "POST",
        "/api/login",
        None,
        json!({"username":"OWNER","password":pass}),
        true,
    )
    .await?;
    assert_eq!(status, 200);
    assert_eq!(token.len(), 64);
    assert_eq!(
        request(&store, "GET", "/api/state", Some(&token), json!({}), true)
            .await?
            .0,
        200
    );
    let actor = store.session_actor(&token).await?;
    assert_eq!(
        store.me(&actor).await?["person"],
        json!(find(&state, "people", "合成负责人"))
    );
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/login",
            None,
            json!({"username":"owner","password":pass,"person_id":Uuid::new_v4()}),
            true
        )
        .await?
        .0,
        400
    );
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/accounts",
            Some(&token),
            json!({"kind":"disable","account":Uuid::new_v4(),"role":"admin"}),
            true
        )
        .await?
        .0,
        400
    );
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/logout",
            Some(&token),
            json!({}),
            false
        )
        .await?
        .0,
        403
    );
    assert!(store.session_actor(&token).await.is_ok());
    assert_eq!(
        request(&store, "POST", "/api/logout", Some(&token), json!({}), true)
            .await?
            .0,
        200
    );
    assert!(store.session_actor(&token).await.is_err());
    // A previously resolved actor cannot continue after logout either.
    assert!(store.state(&actor).await.is_err());
    assert_eq!(business_snapshot(&store).await?, before);
    assert!(store
        .bootstrap_account(find(&state, "people", "合成负责人"), "other", &pass)
        .await
        .is_err());
    let accounts = store.accounts(&owner).await?.to_string();
    assert!(
        !accounts.contains(&pass)
            && !accounts.contains("password_hash")
            && !accounts.contains(&token)
    );
    let audit:String=sqlx::query_scalar("SELECT coalesce(string_agg(input_summary::text||output_summary::text,''),'') FROM qintopia_agent_os.tool_invocation_audit WHERE tool_name='collaboration.account'").fetch_one(&store.pool).await?;
    assert!(!audit.contains(&pass) && !audit.contains(&token));
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn account_lifecycle_revokes_sessions_and_never_grants_business_authority() -> Result<()> {
    let (store, owner, state, owner_pass) = fixture().await?;
    let owner_token = login(&store, "owner", &owner_pass).await?;
    let person = find(&state, "people", "人员甲 · 合成样例 A");
    let pass = password();
    let account = create(&store, &owner, person, "member", &pass).await?;
    let token = login(&store, "member", &pass).await?;
    let actor = store.session_actor(&token).await?;
    assert_eq!(store.state(&actor).await?["relations"], json!([]));
    assert!(
        !store
            .allowed(
                &actor,
                find(&state, "scopes", "一栋"),
                "erhua",
                "community_service",
                "train"
            )
            .await?
    );
    let before = business_snapshot(&store).await?;
    let (code, data, _) = request(
        &store,
        "GET",
        "/api/accounts",
        Some(&token),
        json!({}),
        true,
    )
    .await?;
    assert_eq!(code, 403);
    assert_eq!(data, json!({"code":"account_management_denied"}));
    for body in [
        json!({"kind":"disable","account":account}),
        json!({"kind":"reset","account":account,"password":password()}),
        json!({"kind":"create","person":find(&state,"people","人员乙 · 合成样例"),"username":"intruder","password":password()}),
    ] {
        assert_eq!(
            request(&store, "POST", "/api/accounts", Some(&token), body, true)
                .await?
                .0,
            403
        );
    }
    assert_eq!(business_snapshot(&store).await?, before);
    let new = password();
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/password",
            Some(&token),
            json!({"current_password":"wrong","new_password":new}),
            true
        )
        .await?
        .0,
        401
    );
    assert!(store.session_actor(&token).await.is_ok());
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/password",
            Some(&token),
            json!({"current_password":pass,"new_password":new}),
            true
        )
        .await?
        .0,
        200
    );
    assert!(store.state(&actor).await.is_err());
    assert!(login(&store, "member", &pass).await.is_err());
    let token = login(&store, "member", &new).await?;
    let second = login(&store, "member", &new).await?;
    let reset = password();
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/accounts",
            Some(&owner_token),
            json!({"kind":"reset","account":account,"password":reset}),
            true
        )
        .await?
        .0,
        200
    );
    assert!(store.session_actor(&token).await.is_err());
    assert!(store.session_actor(&second).await.is_err());
    let token = login(&store, "member", &reset).await?;
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/accounts",
            Some(&owner_token),
            json!({"kind":"disable","account":account}),
            true
        )
        .await?
        .0,
        200
    );
    assert_eq!(
        request(&store, "GET", "/api/state", Some(&token), json!({}), true)
            .await?
            .0,
        401
    );
    // Reset and disable don't mutate appointments/grants.
    assert_eq!(store.state(&owner).await?, state);
    sqlx::query("UPDATE qintopia_agent_os.collaboration_login_limits SET window_start=clock_timestamp()-interval '16 minutes' WHERE tenant_key=$1").bind(&store.tenant).execute(&store.pool).await?;
    assert_eq!(
        login(&store, "member", &reset)
            .await
            .unwrap_err()
            .to_string(),
        "invalid_credentials"
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn password_session_scope_and_live_revocation_deny_reads_and_writes() -> Result<()> {
    let (store, owner, state, _) = fixture().await?;
    let a = assignment(&state, "人员甲 · 合成样例 A", "一栋");
    let b = assignment(&state, "人员乙 · 合成样例", "二栋");
    let ar = save(&store, &owner, Change::Assign(Box::new(a.clone()))).await?;
    let br = save(&store, &owner, Change::Assign(Box::new(b.clone()))).await?;
    let pass = password();
    create(&store, &owner, a.person, "steward", &pass).await?;
    let token = login(&store, "steward", &pass).await?;
    let actor = store.session_actor(&token).await?;
    let visible = store.state(&actor).await?;
    assert_eq!(visible["relations"].as_array().unwrap().len(), 1);
    assert_eq!(visible["scopes"].as_array().unwrap().len(), 1);
    assert!(!visible.to_string().contains(&b.person.to_string()));
    assert!(!visible.to_string().contains("二栋"));
    assert!(
        store
            .allowed(&actor, a.scope, "erhua", "community_service", "train")
            .await?
    );
    assert!(
        !store
            .allowed(&actor, b.scope, "erhua", "community_service", "train")
            .await?
    );
    let before = business_snapshot(&store).await?;
    let (status, data, _) = request(
        &store,
        "POST",
        "/api/decision",
        Some(&token),
        json!({"collaboration":br["change"]["collaboration"],"action":"train"}),
        true,
    )
    .await?;
    assert_eq!(status, 403);
    assert_eq!(data, json!({"code":"scope_access_denied"}));
    let forbidden = Command {
        operation_id: Uuid::new_v4(),
        expected_version: visible["version"].as_i64().unwrap(),
        change: Change::EndAppointment {
            appointment: id(&br["change"]["appointment"]),
        },
    };
    assert_ne!(
        request(
            &store,
            "POST",
            "/api/save",
            Some(&token),
            serde_json::to_value(&forbidden)?,
            true
        )
        .await?
        .0,
        200
    );
    assert_eq!(business_snapshot(&store).await?, before);
    let grant = store.state(&owner).await?["grants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["collaboration"] == ar["change"]["collaboration"] && g["action"] == "train")
        .unwrap()["id"]
        .clone();
    save(&store, &owner, Change::RevokeGrant { grant: id(&grant) }).await?;
    assert!(
        !store
            .allowed(&actor, a.scope, "erhua", "community_service", "train")
            .await?
    );
    assert_eq!(store.state(&actor).await?["relations"], json!([]));
    let mut renewed = a.clone();
    renewed.collaboration = Some(id(&ar["change"]["collaboration"]));
    save(&store, &owner, Change::Assign(Box::new(renewed))).await?;
    assert!(
        store
            .allowed(&actor, a.scope, "erhua", "community_service", "train")
            .await?
    );
    save(
        &store,
        &owner,
        Change::EndAppointment {
            appointment: id(&ar["change"]["appointment"]),
        },
    )
    .await?;
    assert!(
        !store
            .allowed(&actor, a.scope, "erhua", "community_service", "train")
            .await?
    );
    assert_eq!(store.state(&actor).await?["relations"], json!([]));
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/decision",
            Some(&token),
            json!({"collaboration":ar["change"]["collaboration"],"action":"train"}),
            true
        )
        .await?
        .0,
        403
    );
    // Business revocation doesn't need logout to take effect.
    assert_eq!(
        request(&store, "GET", "/api/me", Some(&token), json!({}), true)
            .await?
            .0,
        200
    );
    let before = business_snapshot(&store).await?;
    assert_ne!(
        request(
            &store,
            "POST",
            "/api/save",
            Some(&token),
            serde_json::to_value(&forbidden)?,
            true
        )
        .await?
        .0,
        200
    );
    assert_eq!(business_snapshot(&store).await?, before);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn persistent_limits_expiry_and_identity_revocation_fail_closed() -> Result<()> {
    let (store, _owner, _state, pass) = fixture().await?;
    for _ in 0..5 {
        assert_eq!(
            request(
                &store,
                "POST",
                "/api/login",
                None,
                json!({"username":"owner","password":"wrong"}),
                true
            )
            .await?
            .0,
            401
        );
    }
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/login",
            None,
            json!({"username":"owner","password":pass}),
            true
        )
        .await?
        .0,
        429
    );
    // New Store/process doesn't reset the PostgreSQL limiter.
    let db = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let restarted = Store::local(&db, &store.tenant).await?;
    assert!(login(&restarted, "owner", &pass)
        .await
        .unwrap_err()
        .to_string()
        .contains("rate_limited"));
    sqlx::query("UPDATE qintopia_agent_os.collaboration_login_limits SET window_start=clock_timestamp()-interval '16 minutes' WHERE tenant_key=$1").bind(&store.tenant).execute(&store.pool).await?;
    let token = login(&restarted, "owner", &pass).await?;
    let actor = store.session_actor(&token).await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_sessions SET expires_at=clock_timestamp()-interval '1 second' WHERE tenant_key=$1").bind(&store.tenant).execute(&store.pool).await?;
    assert!(store.state(&actor).await.is_err());
    let token = login(&store, "owner", &pass).await?;
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET version=version+1 WHERE namespace=$1 AND source_ref='fixture-person-0'").bind(&store.tenant).execute(&store.pool).await?;
    assert!(store.session_actor(&token).await.is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn account_management_requires_current_explicit_identity_authority() -> Result<()> {
    let (store, owner, state, _) = fixture().await?;
    let person = find(&state, "people", "人员甲 · 合成样例 A");
    let pass = password();
    create(&store, &owner, person, "delegated", &pass).await?;
    let token = login(&store, "delegated", &pass).await?;
    let actor = store.session_actor(&token).await?;
    let mut a = Assignment {
        collaboration: None,
        person,
        role: find(&state, "roles", "公司负责人"),
        duty: Some(find(&state, "duties", "组织管理")),
        scope: find(&state, "scopes", "秦托邦"),
        agent: "default".into(),
        domain: "organization".into(),
        responsibility: "合成组织台账管理员，尚无账号权".into(),
        valid_from: None,
        valid_until: None,
        proxy_for: None,
        actions: vec![],
        permissions: vec![PermissionSetting {
            action: "manage".into(),
            mode: PermissionMode::Autonomous,
            reviewer: None,
        }],
        delegation: Some(Delegation {
            agents: vec!["default".into()],
            domains: vec!["organization".into()],
            actions: vec!["manage".into()],
            depth: 1,
        }),
    };
    let initial = save(&store, &owner, Change::Assign(Box::new(a.clone()))).await?;
    assert!(store.state(&actor).await?["catalog_admin"]
        .as_bool()
        .unwrap());
    assert!(
        store.accounts(&actor).await.is_err(),
        "catalog management isn't identity administration"
    );
    a.collaboration = Some(id(&initial["change"]["collaboration"]));
    a.delegation
        .as_mut()
        .unwrap()
        .actions
        .push("identity".into());
    save(&store, &owner, Change::Assign(Box::new(a))).await?;
    assert!(store.me(&actor).await?["account_admin"].as_bool().unwrap());
    let next = find(&state, "people", "人员乙 · 合成样例");
    let account = create(&store, &actor, next, "managed", &password()).await?;
    save(
        &store,
        &owner,
        Change::EndAppointment {
            appointment: id(&initial["change"]["appointment"]),
        },
    )
    .await?;
    let before = business_snapshot(&store).await?;
    assert!(!store.me(&actor).await?["account_admin"].as_bool().unwrap());
    assert_eq!(
        request(
            &store,
            "GET",
            "/api/accounts",
            Some(&token),
            json!({}),
            true
        )
        .await?
        .0,
        403
    );
    assert_eq!(
        request(
            &store,
            "POST",
            "/api/accounts",
            Some(&token),
            json!({"kind":"disable","account":account}),
            true
        )
        .await?
        .0,
        403
    );
    assert_eq!(business_snapshot(&store).await?, before);
    Ok(())
}
