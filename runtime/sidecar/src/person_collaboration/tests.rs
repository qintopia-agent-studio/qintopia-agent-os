use super::{model::*, validate_local_database};
use chrono::Utc;
use uuid::Uuid;

fn assignment() -> Assignment {
    Assignment {
        collaboration: None,
        person: Uuid::new_v4(),
        role: Uuid::new_v4(),
        duty: None,
        scope: Uuid::new_v4(),
        agent: "erhua".into(),
        domain: "community_service".into(),
        responsibility: "确认本栋知识".into(),
        valid_from: None,
        valid_until: None,
        proxy_for: None,
        actions: vec!["train".into()],
        permissions: vec![],
        delegation: None,
    }
}

fn policy() -> (Policy, Uuid, Uuid, Uuid, Uuid) {
    let root = Uuid::new_v4();
    let one = Uuid::new_v4();
    let two = Uuid::new_v4();
    let owner = Uuid::new_v4();
    let resident = Uuid::new_v4();
    let parent = Grant {
        id: Uuid::new_v4(),
        collaboration: Uuid::new_v4(),
        person: owner,
        scope: root,
        agent: "default".into(),
        domain: "organization".into(),
        duty: None,
        action: "manage".into(),
        mode: PermissionMode::Autonomous,
        reviewer: None,
        parent: None,
        descendants: true,
        active: true,
        delegation: Delegation {
            agents: vec!["erhua".into()],
            domains: vec!["community_service".into()],
            actions: vec!["train".into(), "manage".into()],
            depth: 8,
        },
    };
    let child = Grant {
        id: Uuid::new_v4(),
        collaboration: Uuid::new_v4(),
        person: resident,
        scope: one,
        agent: "erhua".into(),
        domain: "community_service".into(),
        duty: Some(Uuid::new_v4()),
        action: "train".into(),
        mode: PermissionMode::Autonomous,
        reviewer: None,
        parent: Some(parent.id),
        descendants: false,
        active: true,
        delegation: Delegation {
            agents: vec![],
            domains: vec![],
            actions: vec![],
            depth: 0,
        },
    };
    (
        Policy {
            scopes: vec![
                Scope {
                    id: root,
                    parent: None,
                    active: true,
                },
                Scope {
                    id: one,
                    parent: Some(root),
                    active: true,
                },
                Scope {
                    id: two,
                    parent: Some(root),
                    active: true,
                },
            ],
            grants: vec![parent, child],
        },
        owner,
        resident,
        one,
        two,
    )
}

#[test]
fn management_is_not_execution_and_scope_is_not_a_label() {
    let (p, owner, resident, one, two) = policy();
    assert!(p
        .manager(owner, one, "erhua", "community_service", "train")
        .is_some());
    assert!(!p.allowed(owner, one, "erhua", "community_service", "train"));
    assert!(p.allowed(resident, one, "erhua", "community_service", "train"));
    assert!(!p.allowed(resident, two, "erhua", "community_service", "train"));
    assert!(!p.allowed(resident, one, "xiaoman", "community_service", "train"));
    assert!(!p.allowed(resident, one, "erhua", "technical_support", "train"));
    assert!(!p.allowed(resident, one, "erhua", "community_service", "publish"));
}

#[test]
fn parent_revocation_invalid_scope_and_cycles_fail_closed() {
    let (mut p, _, resident, one, _) = policy();
    p.grants[0].active = false;
    assert!(!p.allowed(resident, one, "erhua", "community_service", "train"));
    p.grants[0].active = true;
    p.scopes[0].active = false;
    assert!(!p.effective(&p.grants[1]));
    p.scopes[0].active = true;
    p.grants[0].parent = Some(p.grants[1].id);
    assert!(!p.effective(&p.grants[1]));
    p.scopes[1].parent = Some(one);
    assert!(!p.in_scope(one, p.scopes[0].id, true));
}

#[test]
fn delegated_envelope_cannot_outgrow_its_source() {
    let (mut p, _, _, _, _) = policy();
    p.grants[1].action = "manage".into();
    p.grants[1].delegation = Delegation {
        agents: vec!["erhua".into()],
        domains: vec!["community_service".into()],
        actions: vec!["train".into()],
        depth: 0,
    };
    assert!(p.effective(&p.grants[1]));
    p.grants[1].delegation.agents.push("xiaoman".into());
    assert!(!p.effective(&p.grants[1]));
    p.grants[1].delegation.agents.pop();
    p.grants[1].delegation.depth = 8;
    assert!(!p.effective(&p.grants[1]));
    p.grants[1].delegation.depth = 0;
    p.grants[1].scope = p.grants[0].scope;
    p.grants[1].descendants = true;
    p.grants[0].descendants = false;
    assert!(!p.effective(&p.grants[1]));
}

#[test]
fn typed_requests_reject_self_asserted_actor_unknown_agent_and_unbounded_proxy() {
    let mut a = assignment();
    assert!(a.validate(Utc::now()).is_ok());
    a.agent = "guestroom-not-registered".into();
    assert!(a.validate(Utc::now()).is_err());
    a.agent = "erhua".into();
    a.proxy_for = Some(Uuid::new_v4());
    assert!(a.validate(Utc::now()).is_err());
    a.proxy_for = None;
    a.actions.push("train".into());
    assert!(a.validate(Utc::now()).is_err());
    let bad = serde_json::json!({"operation_id":Uuid::new_v4(),"expected_version":1,"actor":"admin","change":{"kind":"create_role","label":"舍长"}});
    assert!(serde_json::from_value::<Command>(bad).is_err());
}

#[test]
fn multiline_work_descriptions_do_not_relax_catalog_name_validation() {
    let mut a = assignment();
    a.responsibility =
        "整理本栋服务信息。\n需要确认时，先给负责人看。\r\n\t保留待处理事项。".into();
    assert!(a.validate(Utc::now()).is_ok());
    assert!(plain_text(&a.responsibility, 2000).is_ok());
    for control in ['\n', '\r', '\t', '\0', '\u{1b}', '\u{7f}'] {
        assert!(label(&format!("服务{control}职责"), 80).is_err());
    }
    for control in ['\0', '\u{1b}', '\u{7f}'] {
        a.responsibility = format!("服务{control}说明");
        assert!(a.validate(Utc::now()).is_err());
    }
    a.responsibility = " \n\r\t ".into();
    assert!(a.validate(Utc::now()).is_err());
    a.responsibility = "说".repeat(2001);
    assert!(a.validate(Utc::now()).is_err());
    assert!(plain_text("说明", 2).is_ok());
    assert!(plain_text("说明", 1).is_err());
    assert!(label("一栋服务", 80).is_ok());
}

#[test]
fn connection_gate_never_falls_back_to_live_or_query_overrides() {
    for bad in [
        "postgres://localhost/qintopia_test",
        "postgres://127.0.0.1/production",
        "postgres://192.0.2.1/qintopia_test",
        "postgres://127.0.0.1/qintopia_test?host=192.0.2.1",
        "postgres://127.0.0.1/qintopia_test#override",
    ] {
        assert!(validate_local_database(bad).is_err());
    }
    assert!(validate_local_database("postgres://127.0.0.1:55439/qintopia_test").is_ok());
    assert!(validate_local_database("postgres://[::1]:55439/qintopia_test").is_ok());
}

#[test]
fn permission_settings_reject_implicit_escalation_and_accept_empty_duty_drafts() {
    let mut a = assignment();
    a.duty = Some(Uuid::new_v4());
    a.actions.clear();
    assert!(a.validate(Utc::now()).is_ok());
    a.permissions = vec![PermissionSetting {
        action: "train".into(),
        mode: PermissionMode::Denied,
        reviewer: None,
    }];
    assert!(a.validate(Utc::now()).is_ok());
    assert!(!a.has_autonomous("train"));
    a.permissions[0].mode = PermissionMode::Confirmation;
    assert!(a.validate(Utc::now()).is_err());
    a.permissions[0].reviewer = Some(a.person);
    assert!(a.validate(Utc::now()).is_err());
    a.permissions[0].reviewer = Some(Uuid::new_v4());
    assert!(a.validate(Utc::now()).is_ok());
    a.permissions[0].mode = PermissionMode::Autonomous;
    assert!(a.validate(Utc::now()).is_err());
    a.permissions[0].reviewer = None;
    assert!(a.validate(Utc::now()).is_ok());
    a.actions = vec!["publish".into()];
    assert!(a.validate(Utc::now()).is_err());
    a.actions.clear();
    a.permissions.push(a.permissions[0].clone());
    assert!(a.validate(Utc::now()).is_err());
    a.permissions.pop();
    a.permissions[0].action = "manage".into();
    assert!(a.validate(Utc::now()).is_err());
    a.permissions[0].mode = PermissionMode::Confirmation;
    a.permissions[0].reviewer = Some(Uuid::new_v4());
    assert!(a.validate(Utc::now()).is_ok());
    a.delegation = Some(Delegation {
        agents: vec!["erhua".into()],
        domains: vec!["community_service".into()],
        actions: vec!["train".into()],
        depth: 0,
    });
    assert!(a.validate(Utc::now()).is_err());
    assert!(
        serde_json::from_value::<PermissionSetting>(serde_json::json!({
            "action":"train", "mode":"allow_all", "reviewer":null
        }))
        .is_err()
    );
}

#[test]
fn complete_connection_decisions_distinguish_autonomy_confirmation_and_no_permission() {
    let (mut p, _, person, scope, _) = policy();
    let relation = p.grants[1].collaboration;
    assert_eq!(p.decision(relation, "train")["status"], "autonomous");
    assert_eq!(p.decision(relation, "publish")["status"], "denied");
    assert_eq!(p.decision(Uuid::new_v4(), "train")["status"], "denied");
    p.grants[1].mode = PermissionMode::Denied;
    assert_eq!(p.decision(relation, "train")["status"], "denied");
    assert!(!p.allowed(person, scope, "erhua", "community_service", "train"));

    let reviewer = Uuid::new_v4();
    let mut reviewer_grant = p.grants[1].clone();
    reviewer_grant.id = Uuid::new_v4();
    reviewer_grant.collaboration = Uuid::new_v4();
    reviewer_grant.person = reviewer;
    reviewer_grant.mode = PermissionMode::Autonomous;
    p.grants.push(reviewer_grant);
    p.grants[1].mode = PermissionMode::Confirmation;
    p.grants[1].reviewer = Some(reviewer);
    let decision = p.decision(relation, "train");
    assert_eq!(decision["status"], "confirmation_required");
    assert_eq!(decision["reviewer"], serde_json::json!(reviewer));
    assert!(!p.allowed(person, scope, "erhua", "community_service", "train"));
    p.grants[2].active = false;
    assert_eq!(
        p.decision(relation, "train")["reason"],
        "reviewer_authority_inactive"
    );
    p.grants[2].active = true;
    p.grants[0].active = false;
    assert_eq!(p.decision(relation, "train")["status"], "denied");
}

#[test]
fn reviewer_must_be_another_person_with_the_same_current_duty_agent_domain_and_scope() {
    let (mut p, _, person, _, other_scope) = policy();
    let relation = p.grants[1].collaboration;
    let mut reviewer_grant = p.grants[1].clone();
    reviewer_grant.id = Uuid::new_v4();
    reviewer_grant.collaboration = Uuid::new_v4();
    reviewer_grant.person = Uuid::new_v4();
    p.grants[1].mode = PermissionMode::Confirmation;
    p.grants[1].reviewer = Some(reviewer_grant.person);
    p.grants.push(reviewer_grant.clone());
    assert_eq!(
        p.decision(relation, "train")["status"],
        "confirmation_required"
    );
    let mut variants = Vec::new();
    let mut candidate = reviewer_grant.clone();
    candidate.scope = other_scope;
    variants.push(candidate);
    let mut candidate = reviewer_grant.clone();
    candidate.duty = Some(Uuid::new_v4());
    variants.push(candidate);
    let mut candidate = reviewer_grant.clone();
    candidate.agent = "xiaoman".into();
    variants.push(candidate);
    let mut candidate = reviewer_grant.clone();
    candidate.domain = "activity_operations".into();
    variants.push(candidate);
    let mut candidate = reviewer_grant.clone();
    candidate.mode = PermissionMode::Confirmation;
    candidate.reviewer = Some(Uuid::new_v4());
    variants.push(candidate);
    let mut candidate = reviewer_grant.clone();
    candidate.parent = Some(Uuid::new_v4());
    variants.push(candidate);
    for candidate in variants {
        p.grants[2] = candidate;
        assert_eq!(p.decision(relation, "train")["status"], "denied");
    }
    p.grants[2] = reviewer_grant;
    p.grants[1].reviewer = Some(person);
    assert_eq!(
        p.decision(relation, "train")["reason"],
        "eligible_reviewer_required"
    );
}

#[test]
fn broad_queries_do_not_choose_a_convenient_duty_or_legacy_grant() {
    let (mut p, _, person, scope, _) = policy();
    let relation = p.grants[1].collaboration;
    let mut second = p.grants[1].clone();
    second.id = Uuid::new_v4();
    second.collaboration = Uuid::new_v4();
    second.duty = Some(Uuid::new_v4());
    p.grants.push(second);
    assert_eq!(p.decision(relation, "train")["status"], "autonomous");
    assert!(!p.allowed(person, scope, "erhua", "community_service", "train"));
    p.grants[2].duty = p.grants[1].duty;
    assert!(p.allowed(person, scope, "erhua", "community_service", "train"));
    p.grants[2].mode = PermissionMode::Denied;
    assert!(!p.allowed(person, scope, "erhua", "community_service", "train"));
    p.grants[2].active = false;
    p.grants[1].duty = None;
    assert_eq!(p.decision(relation, "train")["reason"], "duty_required");
    assert!(!p.allowed(person, scope, "erhua", "community_service", "train"));
}

#[test]
fn confirmation_management_cannot_issue_grants_or_inspect_as_a_manager() {
    let (mut p, _, person, scope, _) = policy();
    p.grants[1].action = "manage".into();
    p.grants[1].mode = PermissionMode::Confirmation;
    p.grants[1].reviewer = Some(Uuid::new_v4());
    p.grants[1].delegation = Delegation {
        agents: vec!["erhua".into()],
        domains: vec!["community_service".into()],
        actions: vec!["train".into()],
        depth: 0,
    };
    assert!(p
        .manager(person, scope, "erhua", "community_service", "train")
        .is_none());
    assert!(!p.can_inspect(person, scope, "erhua", "community_service"));
    let mut subordinate = p.grants[1].clone();
    subordinate.id = Uuid::new_v4();
    subordinate.action = "train".into();
    subordinate.mode = PermissionMode::Autonomous;
    subordinate.reviewer = None;
    subordinate.parent = Some(p.grants[1].id);
    assert!(!p.effective(&subordinate));
}

#[cfg(feature = "postgres-integration-tests")]
mod postgres {
    use super::*;
    use crate::person_collaboration::{store::Actor, Store};
    use anyhow::Result;
    use serde_json::{json, Value};

    async fn fixture() -> Result<(Store, Actor)> {
        let db = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
        let store =
            Store::local(&db, &format!("synthetic-collaboration-{}", Uuid::new_v4())).await?;
        crate::db::run_migrations(&store.pool).await?;
        let operator = store.bootstrap_fixture().await?;
        let actor = store.actor(operator).await?;
        Ok((store, actor))
    }
    fn id(v: &Value) -> Uuid {
        Uuid::parse_str(v.as_str().unwrap()).unwrap()
    }
    fn find(s: &Value, key: &str, label: &str) -> Uuid {
        id(&s[key]
            .as_array()
            .unwrap()
            .iter()
            .find(|x| x["label"] == label)
            .unwrap()["id"])
    }
    fn assign_from(s: &Value) -> Assignment {
        Assignment {
            person: find(s, "people", "人员甲 · 合成样例 A"),
            role: find(s, "roles", "舍长"),
            duty: Some(find(s, "duties", "居民服务")),
            scope: find(s, "scopes", "一栋"),
            ..assignment()
        }
    }
    async fn apply(store: &Store, actor: &Actor, change: Change) -> Result<Value> {
        let version = store.state(actor).await?["version"].as_i64().unwrap();
        store
            .command(
                actor,
                &Command {
                    operation_id: Uuid::new_v4(),
                    expected_version: version,
                    change,
                },
                true,
            )
            .await
    }
    async fn member_actor(store: &Store, person: Uuid) -> Result<Actor> {
        let link=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2")
            .bind(&store.tenant).bind(person).fetch_one(&store.pool).await?;
        store.actor(link).await
    }

    #[tokio::test]
    #[ignore = "explicit isolated local database required"]
    async fn preview_save_replay_concurrent_commands_and_restart() -> Result<()> {
        let (store, owner) = fixture().await?;
        // Installation is additive and safe to reapply; bootstrap cannot be replayed into another admin.
        sqlx::raw_sql(include_str!(
            "../../../postgres/migrations/202609100001_person_agent_collaboration.sql"
        ))
        .execute(&store.pool)
        .await?;
        assert!(store.bootstrap_fixture().await.is_err());
        let s = store.state(&owner).await?;
        let a = assign_from(&s);
        // A forged client cannot add technical support to a building steward's duties.
        let mut incompatible = a.clone();
        incompatible.actions = vec!["technical_support".into()];
        let rejected = apply(&store, &owner, Change::Assign(Box::new(incompatible))).await;
        assert!(rejected.is_err());
        assert_eq!(store.state(&owner).await?["version"], s["version"]);
        // Custom positions explicitly choose duties; the label is not an authorization key.
        let steward = s["roles"]
            .as_array()
            .unwrap()
            .iter()
            .find(|r| r["label"] == "舍长")
            .unwrap();
        let resident_duty = s["duties"]
            .as_array()
            .unwrap()
            .iter()
            .find(|d| d["id"] == serde_json::json!(a.duty.unwrap()))
            .unwrap();
        assert!(steward["duty_ids"]
            .as_array()
            .unwrap()
            .contains(&resident_duty["id"]));
        assert!(!resident_duty["available_actions"]
            .as_array()
            .unwrap()
            .iter()
            .any(|a| a == "technical_support"));
        let person = a.person;
        let scope = a.scope;
        let actor = member_actor(&store, person).await?;
        let command = Command {
            operation_id: Uuid::new_v4(),
            expected_version: s["version"].as_i64().unwrap(),
            change: Change::Assign(Box::new(a.clone())),
        };
        let preview = store.command(&owner, &command, false).await?;
        assert_eq!(preview["persisted"], false);
        assert_eq!(store.state(&owner).await?["relations"], s["relations"]);
        assert!(
            !store
                .allowed(&actor, scope, "erhua", "community_service", "train")
                .await?
        );
        let (first, retry) = tokio::join!(
            store.command(&owner, &command, true),
            store.command(&owner, &command, true)
        );
        let first = first?;
        let retry = retry?;
        assert_eq!(first["change"], retry["change"]);
        assert!(
            store
                .allowed(&actor, scope, "erhua", "community_service", "train")
                .await?
        );
        assert!(
            !store
                .allowed(
                    &actor,
                    find(&s, "scopes", "二栋"),
                    "erhua",
                    "community_service",
                    "train"
                )
                .await?
        );
        assert!(
            !store
                .allowed(&owner, scope, "erhua", "community_service", "train")
                .await?
        );
        assert!(
            !store
                .allowed(&actor, scope, "erhua", "community_service", "publish")
                .await?
        );
        let other = member_actor(&store, find(&s, "people", "人员甲 · 合成样例 B")).await?;
        assert!(
            !store
                .allowed(&other, scope, "erhua", "community_service", "train")
                .await?
        );
        assert_eq!(
            store
                .responsible(&owner, scope, "erhua", "community_service", "train")
                .await?["status"],
            "resolved"
        );
        let mut changed = command.clone();
        changed.change = Change::CreateRole {
            label: "不应创建".into(),
            available_actions: vec!["train".into()],
        };
        assert!(store.command(&owner, &changed, true).await.is_err());
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM qintopia_agent_os.collaboration_commands WHERE tenant_key=$1",
        )
        .bind(&store.tenant)
        .fetch_one(&store.pool)
        .await?;
        assert_eq!(count, 1);
        let audits:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_agent_os.tool_invocation_audit WHERE tool_name='collaboration.configure' AND input_summary->>'command_ref'=$1")
            .bind(command.operation_id.to_string()).fetch_one(&store.pool).await?;
        assert_eq!(audits, 1);
        let mut another_agent = a.clone();
        another_agent.agent = "xiaoman".into();
        let parallel_relation =
            apply(&store, &owner, Change::Assign(Box::new(another_agent))).await?;
        assert_eq!(
            parallel_relation["change"]["appointment"],
            first["change"]["appointment"]
        );
        assert_ne!(
            parallel_relation["change"]["collaboration"],
            first["change"]["collaboration"]
        );
        assert!(
            store
                .allowed(&actor, scope, "xiaoman", "community_service", "train")
                .await?
        );
        let version = store.state(&owner).await?["version"].as_i64().unwrap();
        let c1 = Command {
            operation_id: Uuid::new_v4(),
            expected_version: version,
            change: Change::CreateRole {
                label: "并发岗位 A".into(),
                available_actions: vec!["train".into()],
            },
        };
        let c2 = Command {
            operation_id: Uuid::new_v4(),
            expected_version: version,
            change: Change::CreateRole {
                label: "并发岗位 B".into(),
                available_actions: vec!["train".into()],
            },
        };
        let (r1, r2) = tokio::join!(
            store.command(&owner, &c1, true),
            store.command(&owner, &c2, true)
        );
        assert_ne!(r1.is_ok(), r2.is_ok());
        let reconnected = Store::local(
            &crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?,
            &store.tenant,
        )
        .await?;
        assert!(
            reconnected
                .allowed(&actor, scope, "erhua", "community_service", "train")
                .await?
        );
        let mut amended = a;
        amended.collaboration = Some(id(&first["change"]["collaboration"]));
        amended.actions.push("review".into());
        let updated = apply(&store, &owner, Change::Assign(Box::new(amended))).await?;
        assert_eq!(updated["change"]["grants"][0], first["change"]["grants"][0]);
        assert!(
            store
                .allowed(&actor, scope, "erhua", "community_service", "review")
                .await?
        );
        apply(
            &store,
            &owner,
            Change::RevokeGrant {
                grant: id(&first["change"]["grants"][0]),
            },
        )
        .await?;
        assert!(
            !store
                .allowed(&actor, scope, "erhua", "community_service", "train")
                .await?
        );
        assert!(
            store
                .allowed(&actor, scope, "erhua", "community_service", "review")
                .await?
        );
        Ok(())
    }

    #[tokio::test]
    #[ignore = "explicit isolated local database required"]
    async fn delegation_handover_proxy_expiry_and_identity_revocation() -> Result<()> {
        let (store, owner) = fixture().await?;
        let s = store.state(&owner).await?;
        let mut a = assign_from(&s);
        let scope = a.scope;
        // A management responsibility is explicitly configured, never inferred from 舍长.
        let duty = apply(
            &store,
            &owner,
            Change::SaveDuty {
                id: None,
                label: "合成范围授权管理".into(),
                description: "合成测试中受限的职责授予".into(),
                domain: "community_service".into(),
                available_actions: vec!["manage".into()],
            },
        )
        .await?;
        a.duty = Some(id(&duty["change"]["duty"]));
        let role = apply(
            &store,
            &owner,
            Change::SaveRole {
                id: None,
                label: "合成范围授权管理员".into(),
                description: "仅用于验证授权链".into(),
                duty_ids: vec![a.duty.unwrap()],
            },
        )
        .await?;
        a.role = id(&role["change"]["role"]);
        a.actions = vec!["manage".into()];
        a.delegation = Some(Delegation {
            agents: vec!["erhua".into()],
            domains: vec!["community_service".into()],
            actions: vec!["train".into(), "review".into()],
            depth: 0,
        });
        let manager = member_actor(&store, a.person).await?;
        let root_assignment = apply(&store, &owner, Change::Assign(Box::new(a.clone()))).await?;
        let manager_state = store.state(&manager).await?;
        assert_eq!(manager_state["groups"].as_array().unwrap().len(), 1);
        assert!(apply(
            &store,
            &manager,
            Change::SetGroups {
                scope,
                conversations: vec![id(&manager_state["groups"][0]["id"])]
            }
        )
        .await
        .is_err());
        assert!(manager_state["relations"]
            .as_array()
            .unwrap()
            .iter()
            .all(|r| r["agent"] == "erhua" && r["domain"] == "community_service"));
        assert!(
            !store
                .allowed(&manager, scope, "erhua", "community_service", "train")
                .await?
        );
        let mut child = assign_from(&s);
        child.person = find(&s, "people", "人员乙 · 合成样例");
        let member = member_actor(&store, child.person).await?;
        let result = apply(&store, &manager, Change::Assign(Box::new(child.clone()))).await?;
        assert!(
            store
                .allowed(&member, scope, "erhua", "community_service", "train")
                .await?
        );
        let mut denied = child.clone();
        denied.scope = find(&s, "scopes", "二栋");
        assert!(apply(&store, &manager, Change::Assign(Box::new(denied)))
            .await
            .is_err());
        let mut denied = child.clone();
        denied.agent = "xiaoman".into();
        assert!(apply(&store, &manager, Change::Assign(Box::new(denied)))
            .await
            .is_err());
        let mut denied = child.clone();
        denied.actions = vec!["publish".into()];
        assert!(apply(&store, &manager, Change::Assign(Box::new(denied)))
            .await
            .is_err());
        // No majority/name-based selection if two people have matching authority.
        let mut second = child.clone();
        second.person = find(&s, "people", "人员丙 · 合成样例");
        second.proxy_for = Some(id(&result["change"]["appointment"]));
        second.valid_until = Some(Utc::now() + chrono::Duration::hours(1));
        let proxy = apply(&store, &manager, Change::Assign(Box::new(second.clone()))).await?;
        assert_eq!(
            store
                .responsible(&owner, scope, "erhua", "community_service", "train")
                .await?["status"],
            "conflict"
        );
        // Synthetic clock fixture: make the proxy's term expire without any waiting.
        sqlx::query("UPDATE qintopia_agent_os.collaboration_appointments SET valid_from=clock_timestamp()-interval '2 hours',valid_until=clock_timestamp()-interval '1 hour' WHERE id=$1")
            .bind(id(&proxy["change"]["appointment"])).execute(&store.pool).await?;
        let proxy_actor = member_actor(&store, second.person).await?;
        assert!(
            !store
                .allowed(&proxy_actor, scope, "erhua", "community_service", "train")
                .await?
        );
        assert_eq!(
            store
                .responsible(&owner, scope, "erhua", "community_service", "train")
                .await?["status"],
            "resolved"
        );
        apply(
            &store,
            &owner,
            Change::EndAppointment {
                appointment: id(&root_assignment["change"]["appointment"]),
            },
        )
        .await?;
        assert!(
            !store
                .allowed(&member, scope, "erhua", "community_service", "train")
                .await?
        );
        assert!(store.state(&manager).await.is_err());
        let new_term = apply(&store, &owner, Change::Assign(Box::new(a))).await?;
        assert_ne!(
            new_term["change"]["appointment"],
            root_assignment["change"]["appointment"]
        );
        // Reappointment never revives grants chained to the old appointment.
        assert!(
            !store
                .allowed(&member, scope, "erhua", "community_service", "train")
                .await?
        );
        sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked',version=version+1 WHERE namespace=$1 AND person_id=$2")
            .bind(&store.tenant).bind(child.person).execute(&store.pool).await?;
        assert!(store
            .allowed(&member, scope, "erhua", "community_service", "train")
            .await
            .is_err());
        Ok(())
    }

    #[tokio::test]
    #[ignore = "explicit isolated local database required"]
    async fn cross_tenant_group_bounds_and_authenticated_http() -> Result<()> {
        use tokio::{
            io::{AsyncReadExt, AsyncWriteExt},
            net::TcpListener,
        };
        let (store, owner) = fixture().await?;
        let s = store.state(&owner).await?;
        let scope = find(&s, "scopes", "一栋");
        let (foreign, foreign_owner) = fixture().await?;
        let foreign_state = foreign.state(&foreign_owner).await?;
        let foreign_group = id(&foreign_state["groups"][0]["id"]);
        assert!(apply(
            &store,
            &owner,
            Change::SetGroups {
                scope,
                conversations: vec![foreign_group]
            }
        )
        .await
        .is_err());
        let mut wrong_person = assign_from(&s);
        wrong_person.person = find(&foreign_state, "people", "人员甲 · 合成样例 A");
        assert!(
            apply(&store, &owner, Change::Assign(Box::new(wrong_person)))
                .await
                .is_err()
        );
        assert!(foreign.state(&owner).await.is_err());
        let scope_result = apply(
            &store,
            &owner,
            Change::CreateScope {
                parent: find(&s, "scopes", "秦托邦"),
                label: "合成活动区域".into(),
                scope_kind: "business".into(),
            },
        )
        .await?;
        let role_result = apply(
            &store,
            &owner,
            Change::SaveRole {
                id: None,
                label: "合成活动协作员".into(),
                description: "合成活动职责".into(),
                duty_ids: vec![find(&s, "duties", "活动运营")],
            },
        )
        .await?;
        let mut independent = assign_from(&s);
        independent.scope = id(&scope_result["change"]["scope"]);
        independent.role = id(&role_result["change"]["role"]);
        independent.duty = Some(find(&s, "duties", "活动运营"));
        independent.agent = "xiaoman".into();
        independent.domain = "activity_operations".into();
        apply(&store, &owner, Change::Assign(Box::new(independent))).await?;
        let all_groups: Vec<_> = s["groups"]
            .as_array()
            .unwrap()
            .iter()
            .map(|g| id(&g["id"]))
            .collect();
        assert!(apply(
            &store,
            &owner,
            Change::SetGroups {
                scope,
                conversations: vec![Uuid::new_v4()]
            }
        )
        .await
        .is_err());
        apply(
            &store,
            &owner,
            Change::SetGroups {
                scope,
                conversations: all_groups.clone(),
            },
        )
        .await?;
        assert_eq!(
            store.state(&owner).await?["bindings"]
                .as_array()
                .unwrap()
                .iter()
                .filter(|b| b["scope"] == json!(scope))
                .count(),
            3
        );
        let listener = TcpListener::bind("127.0.0.1:0").await?;
        let port = listener.local_addr()?.port();
        for (headers,body,expected) in [
            ("", "{}",403),
            ("Cookie: collaboration-local=fixture-session\r\nOrigin: https://invalid.example\r\nContent-Type: application/json\r\n", "{}",403),
            ("Cookie: collaboration-local=fixture-session\r\nOrigin: LOCAL\r\nContent-Type: application/json\r\n", "{\"actor\":\"admin\"}",409),
        ] {
            let headers=headers.replace("LOCAL",&format!("http://127.0.0.1:{port}"));
            let request=format!("POST /api/save HTTP/1.1\r\nHost: 127.0.0.1:{port}\r\n{headers}Content-Length: {}\r\n\r\n{body}",body.len());
            let client=async { let mut stream=tokio::net::TcpStream::connect((std::net::Ipv4Addr::LOCALHOST,port)).await?;stream.write_all(request.as_bytes()).await?;let mut response=String::new();stream.read_to_string(&mut response).await?;anyhow::Ok(response) };
            let server=async {let(mut stream,_)=listener.accept().await?;super::super::local_server::handle(&mut stream,&store,&owner,port,"fixture-session").await};
            let (response,result)=tokio::join!(client,server);result?;assert!(response?.starts_with(&format!("HTTP/1.1 {expected}")));
        }
        // Revoked or rebound identity cannot inherit the still-running session.
        let operator = store.fixture_operator().await?;
        sqlx::query("UPDATE qintopia_identity.source_identity_links SET person_id=$2,version=version+1 WHERE id=$1")
            .bind(operator).bind(find(&s,"people","人员甲 · 合成样例 A")).execute(&store.pool).await?;
        assert!(store.state(&owner).await.is_err());
        Ok(())
    }
}
