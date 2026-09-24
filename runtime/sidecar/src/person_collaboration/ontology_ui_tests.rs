//! Persisted UI contracts: scheduled authority, truthful scope explanations and audit history.
#![cfg(feature = "postgres-integration-tests")]
use super::{model::*, Actor, Store};
use anyhow::{Context, Result};
use chrono::{Duration, Timelike, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

async fn fixture() -> Result<(Store, Actor, Value)> {
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let store = Store::local(
        &database,
        &format!("synthetic-collaboration-ontology-{}", Uuid::new_v4()),
    )
    .await?;
    crate::db::run_migrations(&store.pool).await?;
    let owner = store.actor(store.bootstrap_fixture().await?).await?;
    store.organization_fixture(&owner).await?;
    store
        .bootstrap_foundation_consumers(&owner, "ontology-fixture-password")
        .await?;
    let state = store.state(&owner).await?;
    Ok((store, owner, state))
}
fn find(s: &Value, key: &str, label: &str) -> Uuid {
    serde_json::from_value(
        s[key]
            .as_array()
            .unwrap()
            .iter()
            .find(|v| v["label"] == label)
            .unwrap()["id"]
            .clone(),
    )
    .unwrap()
}
async fn actor(store: &Store, person: Uuid) -> Result<Actor> {
    let link=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed'").bind(&store.tenant).bind(person).fetch_one(&store.pool).await?;
    store.actor(link).await
}
fn scheduled(s: &Value) -> Assignment {
    Assignment {
        collaboration: None,
        person: find(s, "people", "人员丙 · 合成样例"),
        role: find(s, "roles", "舍长"),
        duty: Some(find(s, "duties", "居民服务")),
        scope: find(s, "scopes", "三栋"),
        agent: "erhua".into(),
        domain: "community_service".into(),
        responsibility: "未来任期本地回归".into(),
        valid_from: Some(
            (Utc::now() + Duration::days(1))
                .with_nanosecond(123456789)
                .unwrap(),
        ),
        valid_until: Some(
            (Utc::now() + Duration::days(2))
                .with_nanosecond(987654321)
                .unwrap(),
        ),
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

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn scheduled_appointment_never_grants_early_and_invalid_dates_are_atomic() -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let a = scheduled(&state);
    let member = actor(&store, a.person).await?;
    let command = Command {
        operation_id: Uuid::new_v4(),
        expected_version: state["version"].as_i64().unwrap(),
        change: Change::Assign(Box::new(a.clone())),
    };
    let before = store.state(&owner).await?;
    assert_eq!(
        store.command(&owner, &command, false).await?["persisted"],
        false
    );
    assert_eq!(before, store.state(&owner).await?);
    let saved = store
        .command(&owner, &command, true)
        .await
        .context("save scheduled connection")?;
    let id: Uuid = serde_json::from_value(saved["change"]["collaboration"].clone())?;
    store
        .account_command(
            &owner,
            &super::store::AccountCommand::Create {
                person: a.person,
                username: "upcoming-member".into(),
                password: "ontology-fixture-password".into(),
            },
        )
        .await?;
    let token = store
        .login(&super::store::Credentials {
            username: "upcoming-member".into(),
            password: "ontology-fixture-password".into(),
        })
        .await?;
    let upcoming_member = store.session_actor(&token).await?;
    let personal = store.state(&upcoming_member).await?;
    assert_eq!(personal["actor_person"], json!(a.person));
    assert_eq!(personal["management_available"], false);
    assert_eq!(personal["catalog_admin"], false);
    assert_eq!(personal["scopes"].as_array().unwrap().len(), 1);
    assert_eq!(personal["scopes"][0]["id"], json!(a.scope));
    assert_eq!(personal["relations"].as_array().unwrap().len(), 1);
    assert_eq!(personal["relations"][0]["id"], json!(id));
    assert_eq!(personal["relations"][0]["status"], "scheduled");
    assert_eq!(personal["relations"][0]["can_manage"], false);
    assert!(personal["grants"]
        .as_array()
        .unwrap()
        .iter()
        .all(|g| g["effective"] == false));
    assert!(store.ontology(&upcoming_member, a.scope).await.is_err());
    let forbidden = Command {
        operation_id: Uuid::new_v4(),
        expected_version: personal["version"].as_i64().unwrap(),
        change: Change::EndCollaboration { collaboration: id },
    };
    assert!(store
        .command(&upcoming_member, &forbidden, true)
        .await
        .is_err());
    assert_eq!(personal, store.state(&upcoming_member).await?);
    assert!(
        !store
            .allowed(
                &member,
                a.scope,
                "erhua",
                "community_service",
                "change_rules"
            )
            .await?
    );
    let state = store.state(&owner).await?;
    let relation = state["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == json!(id))
        .unwrap();
    assert_eq!(relation["status"], "scheduled");
    assert!(relation["valid_from"].as_str().is_some());
    let mut edit = a.clone();
    edit.collaboration = Some(id);
    edit.valid_from = None; // unchanged start from the existing UI
    edit.valid_until = Some(Utc::now() + Duration::days(3));
    let edit_command = Command {
        operation_id: Uuid::new_v4(),
        expected_version: state["version"].as_i64().unwrap(),
        change: Change::Assign(Box::new(edit)),
    };
    let edited = store
        .command(&owner, &edit_command, true)
        .await
        .context("replace scheduled term while preserving future start")?;
    assert_eq!(
        edited["change"]["after"]["connection"]["id"],
        edited["change"]["collaboration"]
    );
    assert_ne!(edited["change"]["collaboration"], json!(id));
    assert_eq!(
        edited["change"]["before"]["appointment"]["valid_from"],
        edited["change"]["after"]["appointment"]["valid_from"]
    );
    assert!(
        !store
            .allowed(
                &member,
                a.scope,
                "erhua",
                "community_service",
                "change_rules"
            )
            .await?
    );
    let edited_state = store.state(&owner).await?;
    let relation = edited_state["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| r["id"] == edited["change"]["collaboration"])
        .unwrap();
    assert_eq!(relation["status"], "scheduled");
    let appointment: Uuid = serde_json::from_value(relation["appointment"].clone())?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_appointments SET valid_from=clock_timestamp()-interval '1 second' WHERE tenant_key=$1 AND id=$2").bind(&store.tenant).bind(appointment).execute(&store.pool).await?;
    assert!(
        store
            .allowed(
                &member,
                a.scope,
                "erhua",
                "community_service",
                "change_rules"
            )
            .await?
    );
    let stable = store.state(&owner).await?;
    let mut bad = a;
    bad.valid_until = bad.valid_from;
    let invalid = Command {
        operation_id: Uuid::new_v4(),
        expected_version: stable["version"].as_i64().unwrap(),
        change: Change::Assign(Box::new(bad)),
    };
    assert_eq!(
        store
            .command(&owner, &invalid, true)
            .await
            .unwrap_err()
            .to_string(),
        "invalid_term_range"
    );
    assert_eq!(stable, store.state(&owner).await?);
    apply_change(&store, &owner, Change::EndAppointment { appointment }).await?;
    let revoked = store.state(&upcoming_member).await?;
    for collection in ["relations", "scopes", "roles", "duties", "agents", "grants"] {
        assert_eq!(
            revoked[collection],
            json!([]),
            "{collection} stays scoped after revocation"
        );
    }
    Ok(())
}

async fn apply_change(store: &Store, owner: &Actor, change: Change) -> Result<Value> {
    let state = store.state(owner).await?;
    store
        .command(
            owner,
            &Command {
                operation_id: Uuid::new_v4(),
                expected_version: state["version"].as_i64().unwrap(),
                change,
            },
            true,
        )
        .await
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn scheduled_permissions_remain_editable_and_revocable_without_early_authority() -> Result<()>
{
    let (store, owner, state) = fixture().await?;
    let mut a = scheduled(&state);
    let member = actor(&store, a.person).await?;
    let saved = apply_change(&store, &owner, Change::Assign(Box::new(a.clone())))
        .await
        .context("create scheduled connection")?;
    let id: Uuid = serde_json::from_value(saved["change"]["collaboration"].clone())?;
    let appointment: Uuid = serde_json::from_value(saved["change"]["appointment"].clone())?;
    a.collaboration = Some(id);
    a.valid_from = None;
    a.responsibility = "开始前调整职责与决定方式".into();
    a.permissions[0].mode = PermissionMode::Denied;
    let edited = apply_change(&store, &owner, Change::Assign(Box::new(a.clone())))
        .await
        .context("edit scheduled permissions without replacing the term")?;
    assert_eq!(edited["change"]["collaboration"], json!(id));
    a.permissions[0].mode = PermissionMode::Autonomous;
    let enabled = apply_change(&store, &owner, Change::Assign(Box::new(a.clone())))
        .await
        .context("restore scheduled permission")?;
    let grant: Uuid = serde_json::from_value(enabled["change"]["grants"][0].clone())?;
    let current = store.state(&owner).await?;
    let configured = current["grants"]
        .as_array()
        .unwrap()
        .iter()
        .find(|g| g["id"] == json!(grant))
        .unwrap();
    assert_eq!(configured["effective"], false);
    assert_eq!(configured["revocable"], true);
    assert!(
        !store
            .allowed(
                &member,
                a.scope,
                "erhua",
                "community_service",
                "change_rules"
            )
            .await?
    );

    // A manager covering the scope but only a different action cannot silently
    // remove this not-yet-effective change_rules permission.
    let limited_person = find(&state, "people", "人员乙 · 合成样例");
    let mut manager = a.clone();
    manager.collaboration = None;
    manager.person = limited_person;
    manager.role = find(&state, "roles", "社区负责人");
    manager.duty = Some(find(&state, "duties", "组织管理"));
    manager.agent = "silaoshi".into();
    manager.domain = "organization".into();
    manager.valid_until = None;
    manager.permissions = vec![PermissionSetting {
        action: "manage".into(),
        mode: PermissionMode::Autonomous,
        reviewer: None,
    }];
    manager.delegation = Some(Delegation {
        agents: vec!["erhua".into()],
        domains: vec!["community_service".into()],
        actions: vec!["train".into()],
        depth: 0,
    });
    apply_change(&store, &owner, Change::Assign(Box::new(manager))).await?;
    let limited = actor(&store, limited_person).await?;
    let stable = store.state(&owner).await?;
    let forbidden = Command {
        operation_id: Uuid::new_v4(),
        expected_version: stable["version"].as_i64().unwrap(),
        change: Change::EndCollaboration { collaboration: id },
    };
    assert_eq!(
        store
            .command(&limited, &forbidden, true)
            .await
            .unwrap_err()
            .to_string(),
        "management_denied"
    );
    assert_eq!(stable, store.state(&owner).await?);

    apply_change(&store, &owner, Change::RevokeGrant { grant })
        .await
        .context("revoke scheduled permission")?;
    apply_change(&store, &owner, Change::EndAppointment { appointment })
        .await
        .context("cancel scheduled appointment")?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_appointments SET valid_from=clock_timestamp()-interval '1 second' WHERE tenant_key=$1 AND id=$2").bind(&store.tenant).bind(appointment).execute(&store.pool).await?;
    assert!(
        !store
            .allowed(
                &member,
                a.scope,
                "erhua",
                "community_service",
                "change_rules"
            )
            .await?
    );
    a.collaboration = None;
    a.valid_from = Some(Utc::now() + Duration::days(1));
    let another = apply_change(&store, &owner, Change::Assign(Box::new(a))).await?;
    let another_id = serde_json::from_value(another["change"]["collaboration"].clone())?;
    apply_change(
        &store,
        &owner,
        Change::EndCollaboration {
            collaboration: another_id,
        },
    )
    .await
    .context("cancel scheduled collaboration")?;
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn ontology_explains_current_inherited_rules_without_other_buildings_or_live_claims(
) -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let one = find(&state, "scopes", "一栋");
    let two = find(&state, "scopes", "二栋");
    let house = actor(&store, find(&state, "people", "人员甲 · 合成样例 B")).await?;
    assert!(store.ontology(&house, one).await.is_err());
    let context = store.ontology(&house, two).await?;
    assert!(context["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .any(|v| v["inherited"] == true && v["key"] == "community_culture"));
    assert!(context["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["scope"]["id"] != json!(one) && v["key"] != "resident_welcome"));
    assert!(context["capabilities"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["real_channel_enabled"] == false));
    let write = super::KnowledgeWrite {
        operation_id: Uuid::new_v4(),
        expected_version: 1,
        scope: two,
        key: "kitchen".into(),
        kind: "rule".into(),
        shared: false,
        case_ref: None,
        content: json!({"text":"未来才适用的规则"}),
        effective_at: Some(Utc::now() + Duration::days(1)),
        effective_until: None,
    };
    store.knowledge_save(&house, &write, false).await?;
    let context = store.ontology(&house, two).await?;
    let current = context["constraints"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["key"] == "kitchen")
        .unwrap();
    assert_eq!(current["version"], 1);
    assert_ne!(current["content"]["text"], "未来才适用的规则");
    assert!(current["author"]["label"].as_str().is_some());
    assert!(store.ontology(&owner, Uuid::new_v4()).await.is_err());

    // Actual identity receipts must not bypass the identity endpoint through
    // whole-tenant history, even when organization management is still valid.
    let token = store
        .login(&super::store::Credentials {
            username: "admin".into(),
            password: "ontology-fixture-password".into(),
        })
        .await?;
    let session = store.session_actor(&token).await?;
    let identities = store.identities(&session, None).await?;
    let candidate = identities["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|c| c["account_label"] == "synthetic-unlinked" && c["selectable"] == true)
        .unwrap();
    let identity = super::store::IdentityUiCommand {
        operation_id: Uuid::new_v4(),
        link_ref: serde_json::from_value(candidate["id"].clone())?,
        person_ref: find(&state, "people", "合成负责人"),
        expected_version: candidate["version"].as_i64().unwrap(),
        expected_configuration_version: identities["version"].as_i64().unwrap(),
        expected_gateway_version: candidate["gateway_version"].as_i64().unwrap(),
        revoke: false,
    };
    store.save_identity(&session, &identity).await?;
    store
        .identity_change(
            &owner,
            &super::store::IdentityCommand {
                operation_id: Uuid::new_v4(),
                link_ref: identity.link_ref,
                person_ref: identity.person_ref,
                expected_version: identity.expected_version + 1,
                evidence_ref: Uuid::new_v4(),
                revoke: false,
            },
        )
        .await?;
    let full = store.state(&owner).await?;
    let history = full["organization"]["history"].as_array().unwrap();
    assert!(history
        .iter()
        .any(|r| r["change"]["kind"] == "identity_change"));
    assert!(history
        .iter()
        .any(|r| r["change"]["link_ref"] == json!(identity.link_ref)
            && r["change"]["kind"].is_null()));
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET managed_actions=array_remove(managed_actions,'identity'),version=version+1 WHERE tenant_key=$1 AND parent_grant_id IS NULL")
        .bind(&store.tenant).execute(&store.pool).await?;
    let restricted = store.state(&owner).await?;
    let history = restricted["organization"]["history"].as_array().unwrap();
    assert!(
        !history.is_empty(),
        "ordinary organization history remains visible"
    );
    assert!(
        history
            .iter()
            .all(|r| r["change"]["kind"] != "identity_change"
                && r["change"].get("link_ref").is_none())
    );
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET managed_actions=array_append(managed_actions,'identity'),version=version+1 WHERE tenant_key=$1 AND parent_grant_id IS NULL")
        .bind(&store.tenant).execute(&store.pool).await?;

    // Root management is bounded by its actual scope and descendant setting.
    let other_root: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,label,kind) VALUES($1,'独立社区','community') RETURNING id")
        .bind(&store.tenant).fetch_one(&store.pool).await?;
    assert_eq!(
        store
            .ontology(&owner, other_root)
            .await
            .unwrap_err()
            .to_string(),
        "scope_access_denied"
    );
    let bounded = store.state(&session).await?;
    assert_eq!(bounded["organization"]["history"], json!([]));
    assert_eq!(bounded["catalog_admin"], false);
    assert!(bounded["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .all(|s| s["id"] != json!(other_root)));
    assert_eq!(bounded["organization"]["ledger"], json!([]));
    let rejected_catalog = Command {
        operation_id: Uuid::new_v4(),
        expected_version: bounded["version"].as_i64().unwrap(),
        change: Change::SaveRole {
            id: None,
            label: "不能跨范围新增的岗位".into(),
            description: String::new(),
            duty_ids: vec![],
        },
    };
    assert_eq!(
        store
            .command(&session, &rejected_catalog, true)
            .await
            .unwrap_err()
            .to_string(),
        "catalog_management_required"
    );
    sqlx::query("UPDATE qintopia_agent_os.collaboration_scopes SET status='revoked' WHERE tenant_key=$1 AND id=$2")
        .bind(&store.tenant).bind(other_root).execute(&store.pool).await?;
    assert!(!store.state(&owner).await?["organization"]["history"]
        .as_array()
        .unwrap()
        .is_empty());
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET include_descendants=false,version=version+1 WHERE tenant_key=$1 AND parent_grant_id IS NULL")
        .bind(&store.tenant).execute(&store.pool).await?;
    assert_eq!(
        store.ontology(&owner, two).await.unwrap_err().to_string(),
        "scope_access_denied"
    );
    assert_eq!(
        store.state(&owner).await?["organization"]["history"],
        json!([])
    );
    let bounded = store.state(&session).await?;
    assert_eq!(bounded["catalog_admin"], false);
    assert_eq!(bounded["scopes"].as_array().unwrap().len(), 1);
    assert_eq!(bounded["scopes"][0]["label"], "秦托邦");
    assert_eq!(bounded["groups"], json!([]));
    assert!(store
        .ontology(&owner, find(&state, "scopes", "秦托邦"))
        .await
        .is_ok());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn history_records_actor_change_and_revocation_effect_without_mutating_on_preview(
) -> Result<()> {
    let (store, owner, state) = fixture().await?;
    let relation = state["relations"]
        .as_array()
        .unwrap()
        .iter()
        .find(|r| {
            r["agent"] == "erhua"
                && r["scope"] == json!(find(&state, "scopes", "二栋"))
                && r["status"] == "active"
        })
        .unwrap();
    let change = Change::EndCollaboration {
        collaboration: serde_json::from_value(relation["id"].clone())?,
    };
    let command = Command {
        operation_id: Uuid::new_v4(),
        expected_version: state["version"].as_i64().unwrap(),
        change,
    };
    let preview = store.command(&owner, &command, false).await?;
    assert!(!preview["change"]["impact"].as_array().unwrap().is_empty());
    assert_eq!(state, store.state(&owner).await?);
    store.command(&owner, &command, true).await?;
    let after = store.state(&owner).await?;
    let entry = &after["organization"]["history"][0];
    assert!(entry["actor_label"].as_str().is_some());
    assert_eq!(entry["before"]["connection"]["status"], "active");
    assert_eq!(entry["after"]["connection"]["status"], "ended");
    assert!(!entry["impact"].as_array().unwrap().is_empty());
    let house = actor(&store, find(&state, "people", "人员甲 · 合成样例 B")).await?;
    let body = store.ontology(&house, find(&state, "scopes", "二栋")).await;
    assert!(body.is_err());
    let current = store
        .ontology(&owner, find(&state, "scopes", "二栋"))
        .await?;
    let constraints = current["constraints"].as_array().unwrap();
    assert!(constraints.iter().all(|rule| rule["key"] != "kitchen"));
    assert!(constraints
        .iter()
        .any(|rule| rule["key"] == "community_culture"));
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn ontology_http_reuses_password_scope_and_strict_write_boundaries() -> Result<()> {
    use super::auth_tests::request;
    use super::store::Credentials;
    let (store, _, state) = fixture().await?;
    for (username, label, opposite) in
        [("house-one", "一栋", "二栋"), ("house-two", "二栋", "一栋")]
    {
        let session = store
            .login(&Credentials {
                username: username.into(),
                password: "ontology-fixture-password".into(),
            })
            .await?;
        let (status, personal, _) =
            request(&store, "GET", "/api/state", Some(&session), json!({}), true).await?;
        assert_eq!(status, 200);
        assert_eq!(
            personal["management_available"], false,
            "{username} must not inherit the earlier manager example"
        );
        assert_eq!(personal["catalog_admin"], false);
        assert!(personal["actor_person"].is_string());
        assert!(personal["local_dialogue_available"].is_boolean());
        assert_eq!(personal["scopes"].as_array().unwrap().len(), 1);
        assert_eq!(personal["scopes"][0]["label"], label);
        assert_eq!(personal["relations"].as_array().unwrap().len(), 1);
        assert_eq!(personal["relations"][0]["person"], personal["actor_person"]);
        assert_eq!(personal["relations"][0]["can_manage"], false);
        assert_eq!(personal["roles"].as_array().unwrap().len(), 1);
        assert_eq!(personal["roles"][0]["label"], "舍长");
        assert_eq!(personal["duties"].as_array().unwrap().len(), 1);
        assert_eq!(personal["agents"], json!(["erhua"]));
        assert_eq!(
            personal["organization"]["positions"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        assert_eq!(personal["organization"]["history"], json!([]));
        assert_eq!(personal["organization"]["ledger"], json!([]));
        assert!(!personal.to_string().contains(opposite));
        let cross = format!("/api/ontology?scope={}", find(&state, "scopes", opposite));
        assert_eq!(
            request(&store, "GET", &cross, Some(&session), json!({}), true)
                .await?
                .0,
            403
        );
        let own_connection = personal["relations"][0]["id"].clone();
        let command = json!({"operation_id":Uuid::new_v4(),"expected_version":personal["version"],"change":{"kind":"end_collaboration","collaboration":own_connection}});
        let (status, _, _) =
            request(&store, "POST", "/api/save", Some(&session), command, true).await?;
        assert_eq!(
            status, 403,
            "ordinary work authority never becomes configuration authority"
        );
        let (_, unchanged, _) =
            request(&store, "GET", "/api/state", Some(&session), json!({}), true).await?;
        assert_eq!(personal, unchanged);
    }
    let scope = find(&state, "scopes", "二栋");
    let path = format!("/api/ontology?scope={scope}");
    assert_eq!(
        request(&store, "GET", &path, None, json!({}), true)
            .await?
            .0,
        401
    );
    let token = store
        .login(&Credentials {
            username: "house-two".into(),
            password: "ontology-fixture-password".into(),
        })
        .await?;
    let (status, body, _) = request(&store, "GET", &path, Some(&token), json!({}), true).await?;
    assert_eq!(status, 200);
    assert_eq!(body["scope"]["id"], json!(scope));
    let other = format!("/api/ontology?scope={}", find(&state, "scopes", "一栋"));
    assert_eq!(
        request(&store, "GET", &other, Some(&token), json!({}), true)
            .await?
            .0,
        403
    );
    let (_, identities, _) = request(
        &store,
        "GET",
        "/api/identities",
        Some(&token),
        json!({}),
        true,
    )
    .await?;
    assert_eq!(identities["can_manage"], false);
    assert_eq!(identities["links"], json!([]));
    let (_, body, _) = request(
        &store,
        "POST",
        "/api/audience-preview",
        Some(&token),
        json!({"collaboration":Uuid::new_v4()}),
        false,
    )
    .await?;
    assert_eq!(body["code"], "cross_site_request_denied");
    let (_, body, _) = request(
        &store,
        "POST",
        "/api/audience-preview",
        Some(&token),
        json!({"collaboration":Uuid::new_v4(),"actor":"admin"}),
        true,
    )
    .await?;
    assert_eq!(body["code"], "invalid_command");
    Ok(())
}
