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
