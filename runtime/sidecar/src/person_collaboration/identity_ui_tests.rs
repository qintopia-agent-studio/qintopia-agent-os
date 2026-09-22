//! Controlled UI identity review over real local transactions and password sessions.
#![cfg(feature = "postgres-integration-tests")]
use super::{
    store::{AccountCommand, Credentials, IdentityUiCommand},
    Actor, Change, Command, Store,
};
use anyhow::Result;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

fn id(value: &Value) -> Uuid {
    serde_json::from_value(value.clone()).unwrap()
}
async fn fixture() -> Result<(Store, Actor, Value)> {
    let db = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let store = Store::local(
        &db,
        &format!("synthetic-collaboration-id-ui-{}", Uuid::new_v4()),
    )
    .await?;
    crate::db::run_migrations(&store.pool).await?;
    let trusted = store.actor(store.bootstrap_fixture().await?).await?;
    let state = store.state(&trusted).await?;
    let owner = id(&state["people"]
        .as_array()
        .unwrap()
        .iter()
        .find(|p| p["label"] == "合成负责人")
        .unwrap()["id"]);
    store
        .bootstrap_account(owner, "owner", "Synthetic-password-owner")
        .await?;
    let token = store
        .login(&Credentials {
            username: "owner".into(),
            password: "Synthetic-password-owner".into(),
        })
        .await?;
    let actor = store.session_actor(&token).await?;
    store.bootstrap_identity_memory_fixture().await?;
    Ok((store, actor, state))
}
async fn draft(store: &Store, actor: &Actor, label: &str) -> Result<Uuid> {
    let version = store.state(actor).await?["version"].as_i64().unwrap();
    let result = store
        .command(
            actor,
            &Command {
                operation_id: Uuid::new_v4(),
                expected_version: version,
                change: Change::SaveLedger {
                    id: None,
                    object: "person".into(),
                    reference: None,
                    label: label.into(),
                    nickname: String::new(),
                    description: "人工登记，等待来源核验".into(),
                    scope: None,
                    owner: None,
                    draft: false,
                },
            },
            true,
        )
        .await?;
    let ledger = id(&result["change"]["id"]);
    let reference: String = sqlx::query_scalar(
        "SELECT object_ref FROM qintopia_agent_os.collaboration_ledger WHERE id=$1",
    )
    .bind(ledger)
    .fetch_one(&store.pool)
    .await?;
    Ok(Uuid::parse_str(&reference)?)
}
async fn review(
    store: &Store,
    actor: &Actor,
    person: Uuid,
    source: &str,
    revoke: bool,
) -> Result<IdentityUiCommand> {
    let list = store.identities(actor, Some(person)).await?;
    let row = list[if revoke { "links" } else { "candidates" }]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["account_label"] == source && v["gateway_key"] == "synthetic-qiwe-one")
        .unwrap();
    Ok(IdentityUiCommand {
        operation_id: Uuid::new_v4(),
        link_ref: id(&row["id"]),
        person_ref: person,
        expected_version: row["version"].as_i64().unwrap(),
        expected_configuration_version: list["version"].as_i64().unwrap(),
        expected_gateway_version: row["gateway_version"].as_i64().unwrap(),
        revoke,
    })
}
async fn snapshot(store: &Store) -> Result<Value> {
    Ok(sqlx::query_scalar("SELECT jsonb_build_object('version',(SELECT version FROM qintopia_agent_os.collaboration_tenants WHERE tenant_key=$1),'links',(SELECT coalesce(jsonb_agg(to_jsonb(l) ORDER BY id),'[]') FROM qintopia_identity.source_identity_links l WHERE namespace=$1 OR namespace LIKE $1||'/%'),'ledger',(SELECT coalesce(jsonb_agg(to_jsonb(l) ORDER BY id),'[]') FROM qintopia_agent_os.collaboration_ledger l WHERE tenant_key=$1),'commands',(SELECT coalesce(jsonb_agg(to_jsonb(c) ORDER BY id),'[]') FROM qintopia_agent_os.collaboration_commands c WHERE tenant_key=$1))")
        .bind(&store.tenant).fetch_one(&store.pool).await?)
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn identity_ui_draft_review_is_explicit_preview_is_pure_and_receipt_is_idempotent(
) -> Result<()> {
    let (store, actor, _) = fixture().await?;
    let person = draft(&store, &actor, "同名人员").await?;
    let other = draft(&store, &actor, "同名人员").await?;
    let command = review(&store, &actor, person, "synthetic-unlinked", false).await?;
    let before = snapshot(&store).await?;
    let preview = store.preview_identity(&actor, &command).await?;
    assert_eq!(preview["saved"], false);
    assert_eq!(preview["impact"]["activates_person"], true);
    assert_eq!(preview["impact"]["authority_granted"], false);
    assert_eq!(snapshot(&store).await?, before);
    let saved = store.save_identity(&actor, &command).await?;
    assert_eq!(saved["saved"], true);
    assert_eq!(saved["after"], preview["after"]);
    let after = snapshot(&store).await?;
    assert_eq!(
        store.save_identity(&actor, &command).await?["replayed"],
        true
    );
    assert_eq!(snapshot(&store).await?, after);
    let mut conflicting = command.clone();
    conflicting.person_ref = other;
    assert!(store
        .save_identity(&actor, &conflicting)
        .await
        .unwrap_err()
        .to_string()
        .contains("identity_operation_conflict"));
    let ledgers=sqlx::query("SELECT object_ref,status,verified FROM qintopia_agent_os.collaboration_ledger WHERE tenant_key=$1 AND kind='person'")
        .bind(&store.tenant).fetch_all(&store.pool).await?;
    let new = ledgers
        .iter()
        .find(|r| r.get::<String, _>("object_ref") == person.to_string())
        .unwrap();
    assert_eq!(new.get::<String, _>("status"), "active");
    assert!(new.get::<bool, _>("verified"));
    let untouched = ledgers
        .iter()
        .find(|r| r.get::<String, _>("object_ref") == other.to_string())
        .unwrap();
    assert_eq!(untouched.get::<String, _>("status"), "draft");
    assert!(!untouched.get::<bool, _>("verified"));
    store
        .account_command(
            &actor,
            &AccountCommand::Create {
                person,
                username: "new_person".into(),
                password: "Synthetic-password-new".into(),
            },
        )
        .await?;
    let token = store
        .login(&Credentials {
            username: "new_person".into(),
            password: "Synthetic-password-new".into(),
        })
        .await?;
    let new_actor = store.session_actor(&token).await?;
    assert_eq!(
        store.identities(&new_actor, None).await?["can_manage"],
        false
    );
    assert_eq!(
        store.identities(&new_actor, None).await?["candidates"],
        json!([])
    );
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn identity_ui_requires_password_session_and_root_identity_management() -> Result<()> {
    let (store, actor, state) = fixture().await?;
    let person = draft(&store, &actor, "待核验人员").await?;
    let command = review(&store, &actor, person, "synthetic-unlinked", false).await?;
    let linked_person:Uuid=sqlx::query_scalar("SELECT person_id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND source_ref='synthetic-resident'")
        .bind(format!("{}/synthetic-qiwe-one",store.tenant)).fetch_one(&store.pool).await?;
    let revoke_command = review(&store, &actor, linked_person, "synthetic-resident", true).await?;
    let no_session = store.actor(store.fixture_operator().await?).await?;
    assert!(store
        .identities(&no_session, None)
        .await
        .unwrap_err()
        .to_string()
        .contains("authentication_required"));
    assert!(store
        .save_identity(&no_session, &command)
        .await
        .unwrap_err()
        .to_string()
        .contains("authentication_required"));
    let scoped = id(&state["people"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["label"] == "人员乙 · 合成样例")
        .unwrap()["id"]);
    let scope = id(&state["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["label"] == "一栋")
        .unwrap()["id"]);
    let role = id(&state["roles"][0]["id"]);
    let appointment:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_appointments(tenant_key,person_id,role_id,scope_id) VALUES($1,$2,$3,$4) RETURNING id")
        .bind(&store.tenant).bind(scoped).bind(role).bind(scope).fetch_one(&store.pool).await?;
    let collaboration:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key,responsibility_text) VALUES($1,$2,'default','organization','仅管理本栋身份') RETURNING id")
        .bind(&store.tenant).bind(appointment).fetch_one(&store.pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,include_descendants,managed_agents,managed_domains,managed_actions,delegation_depth) VALUES($1,$2,'manage',true,ARRAY['default'],ARRAY['organization'],ARRAY['identity'],1)")
        .bind(&store.tenant).bind(collaboration).execute(&store.pool).await?;
    store
        .account_command(
            &actor,
            &AccountCommand::Create {
                person: scoped,
                username: "scoped".into(),
                password: "Synthetic-scoped-password".into(),
            },
        )
        .await?;
    let token = store
        .login(&Credentials {
            username: "scoped".into(),
            password: "Synthetic-scoped-password".into(),
        })
        .await?;
    let scoped_actor = store.session_actor(&token).await?;
    assert_eq!(
        store.identities(&scoped_actor, None).await?["can_manage"],
        false
    );
    assert!(store
        .preview_identity(&scoped_actor, &command)
        .await
        .unwrap_err()
        .to_string()
        .contains("identity_management_denied"));
    let gateway_actor = store
        .gateway_actor("synthetic-qiwe-one", "synthetic-resident")
        .await?;
    assert!(store
        .preview_identity(&gateway_actor, &command)
        .await
        .unwrap_err()
        .to_string()
        .contains("authentication_required"));
    // Root identity management without descendants must not reach a building.
    let root:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_scopes WHERE tenant_key=$1 AND parent_scope_id IS NULL")
        .bind(&store.tenant).fetch_one(&store.pool).await?;
    let root_grant:Uuid=sqlx::query_scalar("SELECT g.id FROM qintopia_agent_os.collaboration_grants g JOIN qintopia_agent_os.agent_collaborations c ON c.id=g.collaboration_id JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE g.tenant_key=$1 AND a.scope_id=$2 AND g.parent_grant_id IS NULL")
        .bind(&store.tenant).bind(root).fetch_one(&store.pool).await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET include_descendants=false,version=version+1 WHERE id=$1")
        .bind(root_grant).execute(&store.pool).await?;
    let restricted = store.identities(&actor, None).await?;
    assert_eq!(restricted["can_manage"], true);
    assert_eq!(restricted["links"], json!([]));
    assert_eq!(restricted["candidates"], json!([]));
    assert_eq!(restricted["people"], json!([]));
    for denied in [&command, &revoke_command] {
        assert_eq!(
            store
                .preview_identity(&actor, denied)
                .await
                .unwrap_err()
                .to_string(),
            "identity_management_denied"
        );
        assert_eq!(
            store
                .save_identity(&actor, denied)
                .await
                .unwrap_err()
                .to_string(),
            "identity_management_denied"
        );
    }
    // A scoped ledger person is visible only inside the manager's actual scope.
    sqlx::query("UPDATE qintopia_agent_os.collaboration_ledger SET scope_id=$3 WHERE tenant_key=$1 AND object_ref=$2 AND kind='person'")
        .bind(&store.tenant).bind(person.to_string()).bind(root).execute(&store.pool).await?;
    let restricted = store.identities(&actor, None).await?;
    assert_eq!(restricted["people"].as_array().unwrap().len(), 1);
    assert_eq!(restricted["people"][0]["id"], json!(person));
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET include_descendants=true,version=version+1 WHERE id=$1")
        .bind(root_grant).execute(&store.pool).await?;
    let remote_person = draft(&store, &actor, "另一根范围人员").await?;
    let remote_root:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scopes(tenant_key,label,kind) VALUES($1,'独立范围','community') RETURNING id")
        .bind(&store.tenant).fetch_one(&store.pool).await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_ledger SET scope_id=$3 WHERE tenant_key=$1 AND object_ref=$2 AND kind='person'")
        .bind(&store.tenant).bind(remote_person.to_string()).bind(remote_root).execute(&store.pool).await?;
    sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,'synthetic-other-root',$2,'qiwe_sender',$3,'personal',true)")
        .bind(&store.tenant).bind(format!("{}/synthetic-other-root",store.tenant)).bind(remote_root).execute(&store.pool).await?;
    let observed = store
        .observe_gateway_subject("synthetic-other-root", "remote-person", Uuid::new_v4())
        .await?;
    let restricted = store.identities(&actor, None).await?;
    assert_eq!(restricted["can_manage"], true);
    assert!(restricted["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|l| l["scope_ref"] != json!(remote_root)));
    assert!(restricted["people"]
        .as_array()
        .unwrap()
        .iter()
        .all(|p| p["id"] != json!(remote_person)));
    let own = review(&store, &actor, person, "synthetic-unlinked", false).await?;
    store.preview_identity(&actor, &own).await?;
    let mut cross_person = own.clone();
    cross_person.person_ref = remote_person;
    assert_eq!(
        store
            .preview_identity(&actor, &cross_person)
            .await
            .unwrap_err()
            .to_string(),
        "identity_management_denied"
    );
    assert_eq!(
        store
            .save_identity(&actor, &cross_person)
            .await
            .unwrap_err()
            .to_string(),
        "identity_management_denied"
    );
    let mut foreign = IdentityUiCommand {
        operation_id: Uuid::new_v4(),
        link_ref: id(&observed["link_ref"]),
        person_ref: remote_person,
        expected_version: 1,
        expected_configuration_version: restricted["version"].as_i64().unwrap(),
        expected_gateway_version: 1,
        revoke: false,
    };
    assert_eq!(
        store
            .preview_identity(&actor, &foreign)
            .await
            .unwrap_err()
            .to_string(),
        "identity_management_denied"
    );
    assert_eq!(
        store
            .save_identity(&actor, &foreign)
            .await
            .unwrap_err()
            .to_string(),
        "identity_management_denied"
    );
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET person_id=$2,status='confirmed',version=version+1,evidence_ref=$3,confirmed_by=$2 WHERE id=$1")
        .bind(foreign.link_ref).bind(remote_person).bind(Uuid::new_v4()).execute(&store.pool).await?;
    foreign.revoke = true;
    foreign.expected_version = 2;
    assert_eq!(
        store
            .preview_identity(&actor, &foreign)
            .await
            .unwrap_err()
            .to_string(),
        "identity_management_denied"
    );
    assert_eq!(
        store
            .save_identity(&actor, &foreign)
            .await
            .unwrap_err()
            .to_string(),
        "identity_management_denied"
    );
    assert!(store.identities(&actor, None).await?["links"]
        .as_array()
        .unwrap()
        .iter()
        .all(|l| l["scope_ref"] != json!(remote_root)));
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn identity_ui_rejects_stale_configuration_link_and_gateway_versions() -> Result<()> {
    let (store, actor, _) = fixture().await?;
    let person = draft(&store, &actor, "版本验证人员").await?;
    let command = review(&store, &actor, person, "synthetic-unlinked", false).await?;
    store.preview_identity(&actor, &command).await?;
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET version=version+1 WHERE id=$1")
        .bind(command.link_ref)
        .execute(&store.pool)
        .await?;
    assert!(store
        .save_identity(&actor, &command)
        .await
        .unwrap_err()
        .to_string()
        .contains("identity_version_conflict"));
    let command = review(&store, &actor, person, "synthetic-unlinked", false).await?;
    store.preview_identity(&actor, &command).await?;
    sqlx::query("UPDATE qintopia_identity.person_identity_gateways SET version=version+1 WHERE tenant_key=$1 AND gateway_key='synthetic-qiwe-one'")
        .bind(&store.tenant).execute(&store.pool).await?;
    assert!(store
        .save_identity(&actor, &command)
        .await
        .unwrap_err()
        .to_string()
        .contains("identity_gateway_version_conflict"));
    let command = review(&store, &actor, person, "synthetic-unlinked", false).await?;
    store.preview_identity(&actor, &command).await?;
    draft(&store, &actor, "其他已保存变更").await?;
    assert!(store
        .save_identity(&actor, &command)
        .await
        .unwrap_err()
        .to_string()
        .contains("configuration_version_conflict"));
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn identity_ui_source_candidates_reject_shared_unobserved_unregistered_and_cross_tenant(
) -> Result<()> {
    let (store, actor, _) = fixture().await?;
    let person = draft(&store, &actor, "来源核对人员").await?;
    let command = review(&store, &actor, person, "synthetic-unlinked", false).await?;
    sqlx::query("UPDATE qintopia_identity.person_identity_gateways SET account_kind='shared' WHERE tenant_key=$1 AND gateway_key='synthetic-qiwe-one'")
        .bind(&store.tenant).execute(&store.pool).await?;
    let list = store.identities(&actor, Some(person)).await?;
    assert!(!list["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| id(&v["id"]) == command.link_ref)
        .unwrap()["selectable"]
        .as_bool()
        .unwrap());
    assert!(store
        .save_identity(&actor, &command)
        .await
        .unwrap_err()
        .to_string()
        .contains("shared_account_person_unknown"));
    sqlx::query("UPDATE qintopia_identity.person_identity_gateways SET account_kind='personal' WHERE tenant_key=$1 AND gateway_key='synthetic-qiwe-one'")
        .bind(&store.tenant).execute(&store.pool).await?;
    sqlx::query(
        "UPDATE qintopia_identity.source_identity_links SET adapter_metadata='{}' WHERE id=$1",
    )
    .bind(command.link_ref)
    .execute(&store.pool)
    .await?;
    assert!(store
        .save_identity(&actor, &command)
        .await
        .unwrap_err()
        .to_string()
        .contains("identity_observation_required"));
    let observed = store
        .observe_gateway_subject("synthetic-qiwe-one", "another-observed", Uuid::new_v4())
        .await?;
    let mut valid = review(&store, &actor, person, "another-observed", false).await?;
    assert_eq!(valid.link_ref, id(&observed["link_ref"]));
    valid.link_ref = Uuid::new_v4();
    assert!(store
        .save_identity(&actor, &valid)
        .await
        .unwrap_err()
        .to_string()
        .contains("identity_scope_unbound"));
    let (foreign, foreign_actor, _) = fixture().await?;
    let foreign_person = draft(&foreign, &foreign_actor, "来源核对人员").await?;
    valid = review(&store, &actor, person, "another-observed", false).await?;
    valid.person_ref = foreign_person;
    assert!(store
        .save_identity(&actor, &valid)
        .await
        .unwrap_err()
        .to_string()
        .contains("person_outside_tenant"));
    let foreign_command = review(
        &foreign,
        &foreign_actor,
        foreign_person,
        "synthetic-unlinked",
        false,
    )
    .await?;
    valid = review(&store, &actor, person, "another-observed", false).await?;
    valid.link_ref = foreign_command.link_ref;
    assert!(store
        .save_identity(&actor, &valid)
        .await
        .unwrap_err()
        .to_string()
        .contains("identity_scope_unbound"));
    // An independently registered gateway using this namespace is ambiguous even
    // when it belongs to another tenant and the source subject name is identical.
    sqlx::query("UPDATE qintopia_identity.person_identity_gateways SET namespace=$1 WHERE tenant_key=$2 AND gateway_key='synthetic-qiwe-one'")
        .bind(format!("{}/synthetic-qiwe-one",store.tenant)).bind(&foreign.tenant).execute(&foreign.pool).await?;
    valid = review(&store, &actor, person, "another-observed", false).await?;
    assert!(store
        .save_identity(&actor, &valid)
        .await
        .unwrap_err()
        .to_string()
        .contains("identity_namespace_conflict"));
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn identity_ui_revocation_ends_sessions_and_old_delegated_authority_cannot_revive(
) -> Result<()> {
    revocation_case(false).await
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn identity_legacy_welcome_revocation_invalidates_reviewed_projection_and_authority(
) -> Result<()> {
    revocation_case(true).await
}

async fn revocation_case(legacy: bool) -> Result<()> {
    let (store, actor, state) = fixture().await?;
    let person = draft(&store, &actor, "身份撤销验证人员").await?;
    let confirm = review(&store, &actor, person, "synthetic-unlinked", false).await?;
    store.save_identity(&actor, &confirm).await?;
    let gateway_actor = store
        .gateway_actor("synthetic-qiwe-one", "synthetic-unlinked")
        .await?;
    store
        .account_command(
            &actor,
            &AccountCommand::Create {
                person,
                username: "revoke_person".into(),
                password: "Synthetic-revoke-password".into(),
            },
        )
        .await?;
    let token = store
        .login(&Credentials {
            username: "revoke_person".into(),
            password: "Synthetic-revoke-password".into(),
        })
        .await?;
    let person_actor = store.session_actor(&token).await?;
    let accounts = store.accounts(&actor).await?;
    let account = accounts["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["person"] == json!(person))
        .unwrap();
    let account_id = id(&account["id"]);
    assert_eq!(account["status"], "active");
    assert_eq!(account["login_state"], "ready");
    assert_eq!(account["login_available"], true);
    let scope = id(&state["scopes"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["label"] == "一栋")
        .unwrap()["id"]);
    let role = id(&state["roles"][0]["id"]);
    let child = id(&state["people"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["label"] == "人员乙 · 合成样例")
        .unwrap()["id"]);
    let duty = id(&state["duties"]
        .as_array()
        .unwrap()
        .iter()
        .find(|v| v["label"] == "居民服务")
        .unwrap()["id"]);
    let root_grant:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_agent_os.collaboration_grants WHERE tenant_key=$1 AND parent_grant_id IS NULL AND action_key='manage'")
        .bind(&store.tenant).fetch_one(&store.pool).await?;
    let mut grant_ids = Vec::new();
    for target in [person, child] {
        let appointment:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_appointments(tenant_key,person_id,role_id,scope_id) VALUES($1,$2,$3,$4) RETURNING id")
            .bind(&store.tenant).bind(target).bind(role).bind(scope).fetch_one(&store.pool).await?;
        let collaboration:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.agent_collaborations(tenant_key,appointment_id,agent_key,domain_key,duty_id,responsibility_text) VALUES($1,$2,'erhua','community_service',$3,'身份撤销测试任职') RETURNING id")
            .bind(&store.tenant).bind(appointment).bind(duty).fetch_one(&store.pool).await?;
        if target == person {
            let manage:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id,managed_agents,managed_domains,managed_actions,delegation_depth) VALUES($1,$2,'manage',$3,ARRAY['erhua'],ARRAY['community_service'],ARRAY['train'],1) RETURNING id")
                .bind(&store.tenant).bind(collaboration).bind(root_grant).fetch_one(&store.pool).await?;
            grant_ids.push(manage);
        }
        let grant:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_grants(tenant_key,collaboration_id,action_key,parent_grant_id) VALUES($1,$2,'train',$3) RETURNING id")
            .bind(&store.tenant).bind(collaboration).bind(grant_ids.first().copied()).fetch_one(&store.pool).await?;
        grant_ids.push(grant);
    }
    assert!(
        store
            .allowed(&person_actor, scope, "erhua", "community_service", "train")
            .await?
    );
    let revoke = review(&store, &actor, person, "synthetic-unlinked", true).await?;
    let before = snapshot(&store).await?;
    let preview = store.preview_identity(&actor, &revoke).await?;
    assert_eq!(preview["impact"]["removes_last_verification"], true);
    assert_eq!(preview["impact"]["revokes_sessions"], 1);
    assert_eq!(preview["impact"]["ends_appointments"], 1);
    assert_eq!(preview["impact"]["revokes_grants"], 3);
    assert_eq!(snapshot(&store).await?, before);
    if legacy {
        let owner = id(&state["people"]
            .as_array()
            .unwrap()
            .iter()
            .find(|p| p["label"] == "合成负责人")
            .unwrap()["id"]);
        let source = format!("synthetic-identity-{}", Uuid::new_v4());
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_sources(source_instance,property_id,mode) VALUES($1,'fixture-property','synthetic')")
            .bind(&source).execute(&store.pool).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_identity_scopes(namespace,source_instance,property_id) VALUES($1,$2,'fixture-property')")
            .bind(format!("{}/synthetic-qiwe-one",store.tenant)).bind(&source).execute(&store.pool).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_grants(person_id,source_instance,property_id,action,expires_at,appointed_by) VALUES($1,$2,'fixture-property','identity',clock_timestamp()+interval '1 hour',$1)")
            .bind(owner).bind(&source).execute(&store.pool).await?;
        let legacy_actor = crate::resident_welcome::store::Actor {
            person: owner,
            source,
            property: "fixture-property".into(),
            expires: chrono::Utc::now() + chrono::Duration::minutes(5),
        };
        let legacy_store = crate::resident_welcome::store::Store {
            pool: store.pool.clone(),
        };
        // Old projections remain attributable even after the Gateway moves.
        sqlx::query("UPDATE qintopia_identity.person_identity_gateways SET namespace=namespace||'/moved',version=version+1 WHERE tenant_key=$1 AND gateway_key='synthetic-qiwe-one'")
            .bind(&store.tenant).execute(&store.pool).await?;
        legacy_store
            .change_identity(
                &legacy_actor,
                Uuid::new_v4(),
                revoke.link_ref,
                person,
                revoke.expected_version,
                Uuid::new_v4(),
                true,
            )
            .await?;
        sqlx::query("UPDATE qintopia_identity.person_identity_gateways SET namespace=$2,version=version+1 WHERE tenant_key=$1 AND gateway_key='synthetic-qiwe-one'")
            .bind(&store.tenant).bind(format!("{}/synthetic-qiwe-one",store.tenant)).execute(&store.pool).await?;
    } else {
        store.save_identity(&actor, &revoke).await?;
    }
    let accounts = store.accounts(&actor).await?;
    let account = accounts["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["person"] == json!(person))
        .unwrap();
    assert_eq!(account["status"], "active");
    assert_eq!(account["login_state"], "identity_invalid");
    assert_eq!(account["login_available"], false);
    assert!(store.session_actor(&token).await.is_err());
    assert!(store.identity_context(&gateway_actor).await.is_err());
    assert!(store
        .login(&Credentials {
            username: "revoke_person".into(),
            password: "Synthetic-revoke-password".into()
        })
        .await
        .is_err());
    // Replaying the previous confirmation is historical; it cannot restore state.
    assert_eq!(
        store.save_identity(&actor, &confirm).await?["replayed"],
        true
    );
    let status: String = sqlx::query_scalar(
        "SELECT status FROM qintopia_identity.source_identity_links WHERE id=$1",
    )
    .bind(confirm.link_ref)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(status, "revoked");
    let reconfirm = review(&store, &actor, person, "synthetic-unlinked", false).await?;
    store.save_identity(&actor, &reconfirm).await?;
    assert!(store.session_actor(&token).await.is_err());
    let fresh_token = store
        .login(&Credentials {
            username: "revoke_person".into(),
            password: "Synthetic-revoke-password".into(),
        })
        .await?;
    let fresh_actor = store.session_actor(&fresh_token).await?;
    let accounts = store.accounts(&actor).await?;
    let account = accounts["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["person"] == json!(person))
        .unwrap();
    assert_eq!(account["login_state"], "ready");
    assert_eq!(account["login_available"], true);
    assert!(
        !store
            .allowed(&fresh_actor, scope, "erhua", "community_service", "train")
            .await?
    );
    let active:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.collaboration_grants WHERE id=ANY($1) AND status='active'").bind(grant_ids).fetch_one(&store.pool).await?;
    assert_eq!(active, 0);
    store
        .account_command(
            &actor,
            &AccountCommand::Disable {
                account: account_id,
            },
        )
        .await?;
    let accounts = store.accounts(&actor).await?;
    let account = accounts["accounts"]
        .as_array()
        .unwrap()
        .iter()
        .find(|a| a["person"] == json!(person))
        .unwrap();
    assert_eq!(account["status"], "disabled");
    assert_eq!(account["login_state"], "disabled");
    assert_eq!(account["login_available"], false);
    Ok(())
}
