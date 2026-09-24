//! PostgreSQL confirmation scenarios using the shared production service methods.
#![cfg(feature = "postgres-integration-tests")]
use super::{store::welcome_review::WelcomeSubject, welcome_model::*, Actor, Store};
use anyhow::Result;
use chrono::{Duration, Utc};
use serde_json::json;
use sqlx::Row;
use uuid::Uuid;
struct Fixture {
    store: Store,
    owner: Actor,
    scope: Uuid,
    account: Uuid,
    gateway: String,
    work: Uuid,
    case: Uuid,
    person: Uuid,
    channel: Uuid,
    artifact: Uuid,
    application: Uuid,
    group: Uuid,
}
impl Fixture {
    async fn new() -> Result<Self> {
        Self::with_intake(false).await
    }
    async fn with_intake(intake: bool) -> Result<Self> {
        let db = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
        let store = Store::local(
            &db,
            &format!("synthetic-collaboration-welcome-subject-{}", Uuid::new_v4()),
        )
        .await?;
        static MIGRATED: tokio::sync::OnceCell<()> = tokio::sync::OnceCell::const_new();
        MIGRATED
            .get_or_try_init(|| async {
                crate::db::run_migrations(&store.pool).await?;
                Ok::<(), anyhow::Error>(())
            })
            .await?;
        let owner = store.actor(store.bootstrap_fixture().await?).await?;
        // Fixture provisioning grants no production authority.
        sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET managed_agents=array_append(managed_agents,'anan'),managed_domains=array_append(managed_domains,'hospitality') WHERE tenant_key=$1 AND parent_grant_id IS NULL")
            .bind(&store.tenant).execute(&store.pool).await?;
        let service = crate::resident_welcome::store::Store {
            pool: store.pool.clone(),
        };
        let refs = service.bootstrap_foundation_fixture(&store.tenant).await?;
        let first = &refs["targets"][0];
        let scope: Uuid = serde_json::from_value(first["scope_ref"].clone())?;
        let case: Uuid = serde_json::from_value(first["case_ref"].clone())?;
        let person: Uuid = serde_json::from_value(first["person_ref"].clone())?;
        let group:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type,display_name) VALUES($1,'wecom',$2,'group','模拟运营群') RETURNING id")
            .bind(&store.tenant).bind(Uuid::new_v4().to_string()).fetch_one(&store.pool).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3)").bind(&store.tenant).bind(scope).bind(group).execute(&store.pool).await?;
        let gateway = format!("synthetic-work-{}", Uuid::new_v4());
        let namespace = format!("{}/enterprise/work", store.tenant);
        sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'wecom_internal',$4,'shared',true)")
            .bind(&store.tenant).bind(&gateway).bind(&namespace).bind(scope).execute(&store.pool).await?;
        let link:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,adapter_metadata) VALUES($1,'wecom_internal','synthetic-service-account',$2) RETURNING id")
            .bind(&namespace).bind(json!({"first_observation_ref":Uuid::new_v4(),"display_name":"模拟小客服"})).fetch_one(&store.pool).await?;
        let result = store
            .welcome_account_register(&owner, link, &gateway, "模拟小客服")
            .await?;
        let account: Uuid = serde_json::from_value(result["work_account"].clone())?;
        store
            .welcome_settings_save(
                &owner,
                &ReviewSettings {
                    scope,
                    conversation: group,
                    expected_version: 0,
                    reviewers: vec![
                        ReviewGrant {
                            subject: SubjectRef::WorkAccount(account),
                            effects: vec!["identity".into(), "review".into()],
                            valid_until: Utc::now() + Duration::hours(1),
                        },
                        ReviewGrant {
                            subject: SubjectRef::Person(store.verified_person(&owner).await?),
                            effects: vec!["identity".into(), "review".into()],
                            valid_until: Utc::now() + Duration::hours(1),
                        },
                    ],
                },
            )
            .await?;
        let personal_namespace = format!("{}/enterprise/resident", store.tenant);
        sqlx::query("INSERT INTO qintopia_identity.person_identity_gateways(tenant_key,gateway_key,namespace,subject_type,scope_id,account_kind,active) VALUES($1,$2,$3,'qiwe_sender',$4,'personal',true)")
            .bind(&store.tenant).bind(format!("synthetic-resident-{}",Uuid::new_v4())).bind(&personal_namespace).bind(scope).execute(&store.pool).await?;
        let channel:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.source_identity_links(namespace,subject_type,source_ref,adapter_metadata) VALUES($1,'qiwe_sender','synthetic-resident',$2) RETURNING id")
            .bind(&personal_namespace).bind(json!({"first_observation_ref":Uuid::new_v4(),"display_name":"模拟居民昵称"})).fetch_one(&store.pool).await?;
        let app: Uuid = sqlx::query_scalar(
            "SELECT application_id FROM qintopia_agent_os.welcome_cases WHERE id=$1",
        )
        .bind(case)
        .fetch_one(&store.pool)
        .await?;
        let app = if intake {
            let binding: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.business_property_bindings(tenant_key,scope_id,source_instance,property_id) SELECT $1,$2,source_instance,property_id FROM qintopia_agent_os.welcome_cases WHERE id=$3 RETURNING id")
                .bind(&store.tenant).bind(scope).bind(case).fetch_one(&store.pool).await?;
            let record = format!("rec{}", Uuid::new_v4().simple());
            let read = store
                .application_read_open(binding, "resident-application", &record)
                .await?;
            let result = store
                .application_read_save(
                    binding,
                    "resident-application",
                    &record,
                    serde_json::from_value(read["read_token"].clone())?,
                    &source_observation(),
                )
                .await?;
            let id: Uuid = serde_json::from_value(result["application"].clone())?;
            sqlx::query(
                "UPDATE qintopia_agent_os.welcome_applications SET person_id=$2 WHERE id=$1",
            )
            .bind(id)
            .bind(person)
            .execute(&store.pool)
            .await?;
            sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET application_id=$2 WHERE id=$1")
                .bind(case)
                .bind(id)
                .execute(&store.pool)
                .await?;
            id
        } else {
            app
        };
        // Known Person remains a candidate while the new stay and channel are pending.
        sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='pending',person_id=NULL,confirmed_by=NULL,evidence_ref=NULL WHERE id=(SELECT identity_link_id FROM qintopia_agent_os.welcome_cases WHERE id=$1)").bind(case).execute(&store.pool).await?;
        sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET person_id=NULL,identity_version=NULL,identity_link_id=NULL WHERE id=$1").bind(case).execute(&store.pool).await?;
        let work:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.work_items(work_item_type,status,requester_agent,target_agent,capability_key,brief_summary,purpose,dedupe_key,idempotency_key,payload) VALUES('welcome_event','awaiting_review','anan','anan','resident_welcome.coordinate','模拟确认','synthetic_business',$1,$1,$2) RETURNING id")
            .bind(Uuid::new_v4().to_string()).bind(json!({"tenant_key":store.tenant,"case_ref":case,"scope_ref":scope,"application_ref":app})).fetch_one(&store.pool).await?;
        let artifact:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.artifacts(work_item_id,artifact_type,created_by_agent,content_text,content_hash) VALUES($1,'welcome_text','anan','模拟介绍',$2) RETURNING id").bind(work).bind(super::digest(b"welcome")).fetch_one(&store.pool).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_artifact_bindings(artifact_id,case_id,application_id,application_revision,case_version,consent_version,template_version) VALUES($1,$2,$3,1,1,1,'synthetic')").bind(artifact).bind(case).bind(app).execute(&store.pool).await?;
        store
            .welcome_review_open(
                &owner,
                &ReviewOpen {
                    work_item: work,
                    scope,
                    case_ref: case,
                    application: app,
                    artifacts: vec![artifact],
                },
            )
            .await?;
        Ok(Self {
            store,
            owner,
            scope,
            account,
            gateway,
            work,
            case,
            person,
            channel,
            artifact,
            application: app,
            group,
        })
    }
    async fn reopen(&self, artifacts: Vec<Uuid>) -> Result<serde_json::Value> {
        self.store
            .welcome_review_open_task(
                None,
                &ReviewOpen {
                    work_item: self.work,
                    scope: self.scope,
                    case_ref: self.case,
                    application: self.application,
                    artifacts,
                },
            )
            .await
    }
    async fn readback(&self, observation: &super::store::applications::Observation) -> Result<()> {
        let row = sqlx::query("SELECT binding_id,resource_alias,record_ref FROM qintopia_agent_os.application_intake_states WHERE application_id=$1").bind(self.application).fetch_one(&self.store.pool).await?;
        let binding: Uuid = row.get("binding_id");
        let alias: String = row.get("resource_alias");
        let record: String = row.get("record_ref");
        let read = self
            .store
            .application_read_open(binding, &alias, &record)
            .await?;
        self.store
            .application_read_save(
                binding,
                &alias,
                &record,
                serde_json::from_value(read["read_token"].clone())?,
                observation,
            )
            .await?;
        Ok(())
    }
    async fn new_content(&self) -> Result<Uuid> {
        let id: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.artifacts(work_item_id,artifact_type,created_by_agent,content_text,content_hash) VALUES($1,'welcome_text','anan','新版模拟介绍',$2) RETURNING id")
            .bind(self.work).bind(super::digest(b"new welcome text")).fetch_one(&self.store.pool).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_artifact_bindings(artifact_id,case_id,application_id,application_revision,case_version,consent_version,template_version) SELECT $1,c.id,a.id,a.revision,c.version,a.consent_version,'simulated' FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_applications a ON a.id=$3 WHERE c.id=$2")
            .bind(id).bind(self.case).bind(self.application).execute(&self.store.pool).await?;
        Ok(id)
    }
    async fn assert_publish(&self, artifact: Uuid, permitted: bool) -> Result<()> {
        let mut tx = self.store.pool.begin().await?;
        let result = super::assert_welcome_operations_review(
            &self.store.pool,
            &mut tx,
            &self.store.tenant,
            self.scope,
            self.case,
            artifact,
        )
        .await;
        assert_eq!(
            result.is_ok(),
            permitted,
            "unexpected operations approval: {result:?}"
        );
        tx.rollback().await?;
        Ok(())
    }
    async fn subject(&self) -> Result<WelcomeSubject> {
        self.store
            .welcome_gateway_subject(&self.gateway, "synthetic-service-account")
            .await
    }
    fn decision(&self) -> ReviewDecision {
        ReviewDecision {
            operation_id: Uuid::new_v4(),
            work_item: self.work,
            expected_version: 1,
            person: Some(self.person),
            channel: Some(self.channel),
            confirm_application_stay: true,
            confirm_channel_person: true,
            confirm_content: true,
            decision: "confirm".into(),
        }
    }
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_work_account_combined_confirmation_and_replay() -> Result<()> {
    let f = Fixture::new().await?;
    let subject = f.subject().await?;
    let request = f.decision();
    assert!(f
        .store
        .gateway_actor(&f.gateway, "synthetic-service-account")
        .await
        .is_err());
    let result = f.store.welcome_review_decide(&subject, &request).await?;
    assert_eq!(result["status"], "confirmed");
    assert_eq!(result["published"], false);
    assert_eq!(
        f.store.welcome_review_decide(&subject, &request).await?,
        result
    );
    let row=sqlx::query("SELECT confirmed_by,confirmed_by_work_account,person_id FROM qintopia_identity.source_identity_links WHERE id=$1").bind(f.channel).fetch_one(&f.store.pool).await?;
    assert_eq!(row.get::<Option<Uuid>, _>("confirmed_by"), None);
    assert_eq!(
        row.get::<Option<Uuid>, _>("confirmed_by_work_account"),
        Some(f.account)
    );
    assert_eq!(row.get::<Option<Uuid>, _>("person_id"), Some(f.person));
    let personal = f.store.welcome_person_subject(&f.owner).await?;
    assert_eq!(
        f.store.welcome_review_list(&personal, f.scope).await?["items"][0]["status"],
        "confirmed"
    );
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.welcome_review_receipts WHERE work_item_id=$1",
    )
    .bind(f.work)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(n, 1);
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_partial_link_then_content_and_revoke() -> Result<()> {
    let f = Fixture::new().await?;
    let subject = f.subject().await?;
    let mut request = f.decision();
    request.confirm_channel_person = false;
    request.confirm_content = false;
    request.channel = None;
    assert_eq!(
        f.store.welcome_review_decide(&subject, &request).await?["status"],
        "identity_confirmed"
    );
    request.operation_id = Uuid::new_v4();
    request.expected_version = 2;
    request.confirm_application_stay = false;
    request.confirm_content = true;
    assert!(f
        .store
        .welcome_review_decide(&subject, &request)
        .await
        .unwrap_err()
        .to_string()
        .contains("identity_segments_required"));
    request.confirm_channel_person = true;
    request.channel = Some(f.channel);
    assert_eq!(
        f.store.welcome_review_decide(&subject, &request).await?["status"],
        "confirmed"
    );
    request.operation_id = Uuid::new_v4();
    request.expected_version = 3;
    request.decision = "revoke".into();
    assert_eq!(
        f.store.welcome_review_decide(&subject, &request).await?["status"],
        "revoked"
    );
    let status: String = sqlx::query_scalar(
        "SELECT status FROM qintopia_identity.source_identity_links WHERE id=$1",
    )
    .bind(f.channel)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(status, "revoked");
    let held: bool =
        sqlx::query_scalar("SELECT manual_hold FROM qintopia_agent_os.welcome_cases WHERE id=$1")
            .bind(f.case)
            .fetch_one(&f.store.pool)
            .await?;
    assert!(
        !held,
        "operations revocation must not alter the steward hold"
    );
    let mut tx = f.store.pool.begin().await?;
    assert!(super::assert_welcome_operations_review(
        &f.store.pool,
        &mut tx,
        &f.store.tenant,
        f.scope,
        f.case,
        f.artifact
    )
    .await
    .unwrap_err()
    .to_string()
    .contains("operations_confirmation_required"));
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_changed_content_rolls_back_and_account_revocation_blocks() -> Result<()> {
    let f = Fixture::new().await?;
    let subject = f.subject().await?;
    sqlx::query("UPDATE qintopia_agent_os.artifacts SET content_hash=$2 WHERE id=$1")
        .bind(f.artifact)
        .bind(super::digest(b"changed"))
        .execute(&f.store.pool)
        .await?;
    assert!(f
        .store
        .welcome_review_decide(&subject, &f.decision())
        .await
        .unwrap_err()
        .to_string()
        .contains("version_conflict"));
    let person: Option<Uuid> = sqlx::query_scalar(
        "SELECT person_id FROM qintopia_identity.source_identity_links WHERE id=$1",
    )
    .bind(f.channel)
    .fetch_one(&f.store.pool)
    .await?;
    assert!(person.is_none());
    sqlx::query(
        "UPDATE qintopia_identity.work_accounts SET active=false,version=version+1 WHERE id=$1",
    )
    .bind(f.account)
    .execute(&f.store.pool)
    .await?;
    assert!(f
        .store
        .welcome_review_list(&subject, f.scope)
        .await
        .is_err());
    assert!(f
        .store
        .gateway_actor(&f.gateway, "synthetic-service-account")
        .await
        .is_err());
    let link: Uuid = sqlx::query_scalar(
        "SELECT source_link_id FROM qintopia_identity.work_accounts WHERE id=$1",
    )
    .bind(f.account)
    .fetch_one(&f.store.pool)
    .await?;
    f.store
        .welcome_account_register(&f.owner, link, &f.gateway, "模拟更换后的客服")
        .await?;
    let renewed = f.subject().await?;
    assert!(
        f.store
            .welcome_review_list(&renewed, f.scope)
            .await
            .is_err(),
        "re-registering an account must not revive old grants"
    );
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_configuration_replacement_requires_new_review() -> Result<()> {
    let f = Fixture::new().await?;
    let subject = f.subject().await?;
    f.store
        .welcome_settings_save(
            &f.owner,
            &ReviewSettings {
                scope: f.scope,
                conversation: f.group,
                expected_version: 1,
                reviewers: vec![],
            },
        )
        .await?;
    assert!(f
        .store
        .welcome_review_decide(&subject, &f.decision())
        .await
        .is_err());
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.welcome_review_receipts WHERE work_item_id=$1",
    )
    .bind(f.work)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(n, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_group_and_http_share_explicit_confirmation() -> Result<()> {
    let f = Fixture::new().await?;
    let chat: String =
        sqlx::query_scalar("SELECT chat_id FROM qintopia_messages.conversations WHERE id=$1")
            .bind(f.group)
            .fetch_one(&f.store.pool)
            .await?;
    let event = Uuid::new_v4().to_string();
    let decision = serde_json::to_value(f.decision())?;
    let raw:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.raw_events(event_id,source,subject,received_at,payload,ingress_auth_verified) VALUES($1,'wecom','qintopia.wecom.raw.authenticated',clock_timestamp(),'{}',true) RETURNING id").bind(&event).fetch_one(&f.store.pool).await?;
    sqlx::query("INSERT INTO qintopia_messages.messages(tenant_id,platform,message_id,event_id,chat_id,chat_type,sender_id,message_kind,sent_at,received_at,raw_event_id,raw) VALUES($1,'wecom',$2,$2,$3,'group','synthetic-service-account','text',clock_timestamp(),clock_timestamp(),$4,$5)")
        .bind(&f.store.tenant).bind(&event).bind(&chat).bind(raw).bind(json!({"welcome_confirmation":decision})).execute(&f.store.pool).await?;
    let mut request = json!({"operation":"person_foundation_tool","schema_version":1,"agent":"anan","tool":"welcome_operations_decide","trusted_context":{"platform":"wecom","chat_type":"group","chat_id":chat,"sender_id":"synthetic-service-account","message_id":event,"gateway_id":f.gateway},"arguments":decision,"token":"synthetic-test"});
    let invoke = |v: &serde_json::Value| {
        super::foundation_server::parse_broker_request(&serde_json::to_vec(v).unwrap()).unwrap()
    };
    request["arguments"]["person"] = json!(Uuid::new_v4());
    assert!(super::foundation_server::broker_invoke(
        &f.store,
        &f.gateway,
        "anan",
        invoke(&request)
    )
    .await
    .unwrap_err()
    .to_string()
    .contains("explicit_group_confirmation_required"));
    request["arguments"] = decision;
    assert_eq!(
        super::foundation_server::broker_invoke(&f.store, &f.gateway, "anan", invoke(&request))
            .await?["status"],
        "confirmed"
    );
    super::foundation_server::enable_test_http();
    let from_ui = super::foundation_server::dispatch(
        &f.store,
        &f.owner,
        "/api/foundation/operations/list",
        &serde_json::to_vec(&json!({"scope":f.scope}))?,
    )
    .await?;
    assert_eq!(from_ui["items"][0]["status"], "confirmed");
    request["trusted_context"]["chat_id"] = json!("another-group");
    assert!(super::foundation_server::broker_invoke(
        &f.store,
        &f.gateway,
        "anan",
        invoke(&request)
    )
    .await
    .is_err());
    request["tool"] = json!("welcome_operations_arbitrary");
    assert!(super::foundation_server::broker_invoke(
        &f.store,
        &f.gateway,
        "anan",
        invoke(&request)
    )
    .await
    .unwrap_err()
    .to_string()
    .contains("unknown_welcome_operation"));
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_operations_gate_rechecks_content_and_authority() -> Result<()> {
    let f = Fixture::new().await?;
    let mut tx = f.store.pool.begin().await?;
    assert!(super::assert_welcome_operations_review(
        &f.store.pool,
        &mut tx,
        &f.store.tenant,
        f.scope,
        f.case,
        f.artifact
    )
    .await
    .is_err());
    tx.rollback().await?;
    f.store
        .welcome_review_decide(&f.subject().await?, &f.decision())
        .await?;
    let mut tx = f.store.pool.begin().await?;
    super::assert_welcome_operations_review(
        &f.store.pool,
        &mut tx,
        &f.store.tenant,
        f.scope,
        f.case,
        f.artifact,
    )
    .await?;
    tx.rollback().await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_review_subject_grants SET valid_until=clock_timestamp()-interval '1 second' WHERE work_account_id=$1").bind(f.account).execute(&f.store.pool).await?;
    let mut tx = f.store.pool.begin().await?;
    assert!(super::assert_welcome_operations_review(
        &f.store.pool,
        &mut tx,
        &f.store.tenant,
        f.scope,
        f.case,
        f.artifact
    )
    .await
    .is_err());
    tx.rollback().await?;
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_combined_invalid_consent_rolls_back_identity() -> Result<()> {
    let f = Fixture::new().await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_applications SET consent_active=false WHERE id=(SELECT application_id FROM qintopia_agent_os.welcome_cases WHERE id=$1)").bind(f.case).execute(&f.store.pool).await?;
    let app: Uuid = sqlx::query_scalar(
        "SELECT application_id FROM qintopia_agent_os.welcome_cases WHERE id=$1",
    )
    .bind(f.case)
    .fetch_one(&f.store.pool)
    .await?;
    f.store
        .welcome_review_open(
            &f.owner,
            &ReviewOpen {
                work_item: f.work,
                scope: f.scope,
                case_ref: f.case,
                application: app,
                artifacts: vec![f.artifact],
            },
        )
        .await?;
    let mut request = f.decision();
    request.expected_version = 2;
    assert!(f
        .store
        .welcome_review_decide(&f.subject().await?, &request)
        .await
        .unwrap_err()
        .to_string()
        .contains("welcome_content_not_ready"));
    let status: String = sqlx::query_scalar(
        "SELECT status FROM qintopia_identity.source_identity_links WHERE id=$1",
    )
    .bind(f.channel)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(status, "pending");
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.welcome_review_receipts WHERE work_item_id=$1",
    )
    .bind(f.work)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(n, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_reopen_after_revoke_requires_explicit_new_confirmation() -> Result<()> {
    let f = Fixture::new().await?;
    let subject = f.subject().await?;
    let mut request = f.decision();
    f.store.welcome_review_decide(&subject, &request).await?;
    request.operation_id = Uuid::new_v4();
    request.expected_version = 2;
    request.decision = "revoke".into();
    f.store.welcome_review_decide(&subject, &request).await?;
    let app: Uuid = sqlx::query_scalar(
        "SELECT application_id FROM qintopia_agent_os.welcome_cases WHERE id=$1",
    )
    .bind(f.case)
    .fetch_one(&f.store.pool)
    .await?;
    f.store
        .welcome_review_open(
            &f.owner,
            &ReviewOpen {
                work_item: f.work,
                scope: f.scope,
                case_ref: f.case,
                application: app,
                artifacts: vec![f.artifact],
            },
        )
        .await?;
    request = f.decision();
    request.expected_version = 4;
    assert_eq!(
        f.store.welcome_review_decide(&subject, &request).await?["status"],
        "confirmed"
    );
    let n: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.welcome_review_receipts WHERE work_item_id=$1",
    )
    .bind(f.work)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(n, 3);
    request.operation_id = Uuid::new_v4();
    request.expected_version = 5;
    request.decision = "revoke".into();
    f.store.welcome_review_decide(&subject, &request).await?;
    let retained: Option<Uuid> = sqlx::query_scalar(
        "SELECT person_id FROM qintopia_agent_os.welcome_applications WHERE id=$1",
    )
    .bind(app)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(
        retained,
        Some(f.person),
        "prior revoke receipts must not claim ownership of an existing application link"
    );
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_concurrent_confirmations_and_ui_scope_isolation() -> Result<()> {
    let f = Fixture::new().await?;
    let subject1 = f.subject().await?;
    let subject2 = f.subject().await?;
    let first = f.decision();
    let second = f.decision();
    let (a, b) = tokio::join!(
        f.store.welcome_review_decide(&subject1, &first),
        f.store.welcome_review_decide(&subject2, &second)
    );
    assert_ne!(a.is_ok(), b.is_ok());
    let other:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND source_ref='fixture-person-1'").bind(&f.store.tenant).fetch_one(&f.store.pool).await?;
    let other = f.store.actor(other).await?;
    let other = f.store.welcome_person_subject(&other).await?;
    assert!(f.store.welcome_review_list(&other, f.scope).await.is_err());
    let mut tx = f.store.pool.begin().await?;
    // Unconfigured historical scopes continue through their own existing checks.
    super::assert_welcome_operations_review(
        &f.store.pool,
        &mut tx,
        &f.store.tenant,
        Uuid::new_v4(),
        f.case,
        f.artifact,
    )
    .await?;
    tx.rollback().await?;
    sqlx::query("UPDATE qintopia_agent_os.artifacts SET content_hash=$2 WHERE id=$1")
        .bind(f.artifact)
        .bind(super::digest(b"changed after confirmation"))
        .execute(&f.store.pool)
        .await?;
    let mut tx = f.store.pool.begin().await?;
    assert!(super::assert_welcome_operations_review(
        &f.store.pool,
        &mut tx,
        &f.store.tenant,
        f.scope,
        f.case,
        f.artifact
    )
    .await
    .unwrap_err()
    .to_string()
    .contains("operations_content_changed"));
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_trusted_task_requires_exact_source_and_preserves_manual_hold() -> Result<()> {
    let f = Fixture::new().await?;
    let app: Uuid = sqlx::query_scalar(
        "SELECT application_id FROM qintopia_agent_os.welcome_cases WHERE id=$1",
    )
    .bind(f.case)
    .fetch_one(&f.store.pool)
    .await?;
    let request = ReviewOpen {
        work_item: f.work,
        scope: f.scope,
        case_ref: f.case,
        application: app,
        artifacts: vec![f.artifact],
    };
    assert_eq!(
        f.store.welcome_review_open_task(None, &request).await?["replayed"],
        true
    );
    for key in ["tenant_key", "case_ref", "scope_ref", "application_ref"] {
        let old: serde_json::Value =
            sqlx::query_scalar("SELECT payload FROM qintopia_agent_os.work_items WHERE id=$1")
                .bind(f.work)
                .fetch_one(&f.store.pool)
                .await?;
        let mut forged = old.clone();
        forged[key] = json!(Uuid::new_v4());
        sqlx::query("UPDATE qintopia_agent_os.work_items SET payload=$2 WHERE id=$1")
            .bind(f.work)
            .bind(forged)
            .execute(&f.store.pool)
            .await?;
        assert!(f
            .store
            .welcome_review_open_task(None, &request)
            .await
            .unwrap_err()
            .to_string()
            .contains("welcome_work_item_source_required"));
        sqlx::query("UPDATE qintopia_agent_os.work_items SET payload=$2 WHERE id=$1")
            .bind(f.work)
            .bind(old)
            .execute(&f.store.pool)
            .await?;
    }
    sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET manual_hold=true WHERE id=$1")
        .bind(f.case)
        .execute(&f.store.pool)
        .await?;
    let subject = f.subject().await?;
    let mut decision = f.decision();
    f.store.welcome_review_decide(&subject, &decision).await?;
    let held: bool =
        sqlx::query_scalar("SELECT manual_hold FROM qintopia_agent_os.welcome_cases WHERE id=$1")
            .bind(f.case)
            .fetch_one(&f.store.pool)
            .await?;
    assert!(
        held,
        "identity confirmation must not resume a steward pause"
    );
    decision.operation_id = Uuid::new_v4();
    decision.expected_version = 2;
    decision.decision = "revoke".into();
    f.store.welcome_review_decide(&subject, &decision).await?;
    let held: bool =
        sqlx::query_scalar("SELECT manual_hold FROM qintopia_agent_os.welcome_cases WHERE id=$1")
            .bind(f.case)
            .fetch_one(&f.store.pool)
            .await?;
    assert!(held);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_new_pending_or_rejected_matter_cannot_reuse_older_approval() -> Result<()> {
    let f = Fixture::new().await?;
    let subject = f.subject().await?;
    f.store
        .welcome_review_decide(&subject, &f.decision())
        .await?;
    let app: Uuid = sqlx::query_scalar(
        "SELECT application_id FROM qintopia_agent_os.welcome_cases WHERE id=$1",
    )
    .bind(f.case)
    .fetch_one(&f.store.pool)
    .await?;
    let next: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.work_items(work_item_type,status,requester_agent,target_agent,capability_key,brief_summary,purpose,dedupe_key,idempotency_key,payload) SELECT work_item_type,'awaiting_review',requester_agent,target_agent,capability_key,brief_summary,purpose,$2,$2,payload FROM qintopia_agent_os.work_items WHERE id=$1 RETURNING id")
        .bind(f.work).bind(Uuid::new_v4().to_string()).fetch_one(&f.store.pool).await?;
    f.store
        .welcome_review_open_task(
            None,
            &ReviewOpen {
                work_item: next,
                scope: f.scope,
                case_ref: f.case,
                application: app,
                artifacts: vec![f.artifact],
            },
        )
        .await?;
    for rejected in [false, true] {
        if rejected {
            let mut r = f.decision();
            r.work_item = next;
            r.decision = "reject".into();
            r.confirm_application_stay = false;
            r.confirm_channel_person = false;
            r.confirm_content = false;
            f.store.welcome_review_decide(&subject, &r).await?;
        }
        let mut tx = f.store.pool.begin().await?;
        assert!(super::assert_welcome_operations_review(
            &f.store.pool,
            &mut tx,
            &f.store.tenant,
            f.scope,
            f.case,
            f.artifact
        )
        .await
        .unwrap_err()
        .to_string()
        .contains("operations_confirmation_required"));
        tx.rollback().await?;
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_source_revision_requires_identity_reconfirmation_and_retains_revoke_ownership(
) -> Result<()> {
    let f = Fixture::new().await?;
    let subject = f.subject().await?;
    let app: Uuid = sqlx::query_scalar(
        "SELECT application_id FROM qintopia_agent_os.welcome_cases WHERE id=$1",
    )
    .bind(f.case)
    .fetch_one(&f.store.pool)
    .await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_applications SET person_id=NULL WHERE id=$1")
        .bind(app)
        .execute(&f.store.pool)
        .await?;
    f.store
        .welcome_review_open_task(
            None,
            &ReviewOpen {
                work_item: f.work,
                scope: f.scope,
                case_ref: f.case,
                application: app,
                artifacts: vec![f.artifact],
            },
        )
        .await?;
    let mut initial = f.decision();
    initial.expected_version = 2;
    f.store.welcome_review_decide(&subject, &initial).await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_applications SET revision=revision+1,field_hash=$2 WHERE id=$1")
        .bind(app).bind(super::digest(b"changed source identity fields")).execute(&f.store.pool).await?;
    f.store
        .welcome_review_open_task(
            None,
            &ReviewOpen {
                work_item: f.work,
                scope: f.scope,
                case_ref: f.case,
                application: app,
                artifacts: vec![],
            },
        )
        .await?;
    let list = f.store.welcome_review_list(&subject, f.scope).await?;
    assert_eq!(list["items"][0]["identity_confirmed"], false);
    let mut r = f.decision();
    r.expected_version = 4;
    r.confirm_application_stay = false;
    r.confirm_channel_person = false;
    assert!(f
        .store
        .welcome_review_decide(&subject, &r)
        .await
        .unwrap_err()
        .to_string()
        .contains("identity_segments_required"));
    // Source changes invalidate this matter's proof, not the global account/person fact.
    let status: String = sqlx::query_scalar(
        "SELECT status FROM qintopia_identity.source_identity_links WHERE id=$1",
    )
    .bind(f.channel)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(status, "confirmed");
    r.operation_id = Uuid::new_v4();
    r.confirm_application_stay = true;
    r.confirm_channel_person = true;
    r.confirm_content = false;
    f.store.welcome_review_decide(&subject, &r).await?;
    r.operation_id = Uuid::new_v4();
    r.expected_version = 5;
    r.decision = "revoke".into();
    f.store.welcome_review_decide(&subject, &r).await?;
    let person: Option<Uuid> = sqlx::query_scalar(
        "SELECT person_id FROM qintopia_agent_os.welcome_applications WHERE id=$1",
    )
    .bind(app)
    .fetch_one(&f.store.pool)
    .await?;
    assert!(
        person.is_none(),
        "explicit revocation still removes the association created by this matter"
    );
    Ok(())
}

fn source_observation() -> super::store::applications::Observation {
    super::store::applications::Observation {
        identity_hash: "a".repeat(64),
        field_hash: "b".repeat(64),
        valid: true,
        consent_active: true,
        source_version: Some("1".into()),
    }
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_trusted_content_change_preserves_identity_but_requires_new_content_approval(
) -> Result<()> {
    let f = Fixture::with_intake(true).await?;
    let subject = f.subject().await?;
    let initial = f.decision();
    f.store.welcome_review_decide(&subject, &initial).await?;
    f.assert_publish(f.artifact, true).await?;
    let mut content = source_observation();
    content.field_hash = "c".repeat(64);
    content.source_version = Some("2".into());
    f.readback(&content).await?;
    f.assert_publish(f.artifact, false).await?;
    let artifact = f.new_content().await?;
    f.reopen(vec![artifact]).await?;
    let row = sqlx::query("SELECT identity_receipt,content_receipt FROM qintopia_agent_os.welcome_review_items WHERE work_item_id=$1").bind(f.work).fetch_one(&f.store.pool).await?;
    assert_eq!(
        row.get::<Option<Uuid>, _>("identity_receipt"),
        Some(initial.operation_id)
    );
    assert_eq!(row.get::<Option<Uuid>, _>("content_receipt"), None);
    f.assert_publish(artifact, false).await?;
    let mut review = f.decision();
    review.expected_version = 3;
    review.confirm_application_stay = false;
    review.confirm_channel_person = false;
    assert_eq!(
        f.store.welcome_review_decide(&subject, &review).await?["status"],
        "confirmed"
    );
    f.assert_publish(artifact, true).await?;
    let revoked: bool = sqlx::query_scalar("SELECT revoked_at IS NOT NULL FROM qintopia_agent_os.welcome_artifact_bindings WHERE artifact_id=$1").bind(f.artifact).fetch_one(&f.store.pool).await?;
    assert!(revoked);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_source_aba_without_reopen_cannot_revive_identity_receipt() -> Result<()> {
    let f = Fixture::with_intake(true).await?;
    let subject = f.subject().await?;
    let initial = f.decision();
    f.store.welcome_review_decide(&subject, &initial).await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET manual_hold=true WHERE id=$1")
        .bind(f.case)
        .execute(&f.store.pool)
        .await?;
    let mut changed = source_observation();
    changed.identity_hash = "d".repeat(64);
    changed.field_hash = "e".repeat(64);
    f.readback(&changed).await?;
    f.readback(&source_observation()).await?;
    let mut tx = f.store.pool.begin().await?;
    assert_eq!(
        f.store
            .application_identity_basis(&mut tx, f.scope, f.application)
            .await?,
        Some("a".repeat(64))
    );
    tx.rollback().await?;
    f.assert_publish(f.artifact, false).await?;
    let mut review = f.decision();
    review.expected_version = 2;
    review.confirm_application_stay = false;
    review.confirm_channel_person = false;
    assert!(f
        .store
        .welcome_review_decide(&subject, &review)
        .await
        .is_err());
    let artifact = f.new_content().await?;
    f.reopen(vec![artifact]).await?;
    let list = f.store.welcome_review_list(&subject, f.scope).await?;
    assert_eq!(list["items"][0]["identity_confirmed"], false);
    review.expected_version = 3;
    assert!(f
        .store
        .welcome_review_decide(&subject, &review)
        .await
        .unwrap_err()
        .to_string()
        .contains("identity_segments_required"));
    let row=sqlx::query("SELECT c.person_id,c.application_id,c.manual_hold,a.person_id AS application_person,l.status FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_applications a ON a.id=$2 JOIN qintopia_identity.source_identity_links l ON l.id=c.identity_link_id WHERE c.id=$1").bind(f.case).bind(f.application).fetch_one(&f.store.pool).await?;
    assert_eq!(row.get::<Option<Uuid>, _>("person_id"), Some(f.person));
    assert_eq!(row.get::<Option<Uuid>, _>("application_id"), None);
    assert_eq!(row.get::<Option<Uuid>, _>("application_person"), None);
    assert_eq!(row.get::<String, _>("status"), "confirmed");
    assert!(row.get::<bool, _>("manual_hold"));
    let receipts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.welcome_review_receipts WHERE work_item_id=$1",
    )
    .bind(f.work)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(receipts, 1);
    let channel: String = sqlx::query_scalar(
        "SELECT status FROM qintopia_identity.source_identity_links WHERE id=$1",
    )
    .bind(f.channel)
    .fetch_one(&f.store.pool)
    .await?;
    assert_eq!(channel, "confirmed");
    // An explicit new identity decision is required; the source hash alone never repairs links.
    review.confirm_application_stay = true;
    review.confirm_channel_person = true;
    assert_eq!(
        f.store.welcome_review_decide(&subject, &review).await?["status"],
        "confirmed"
    );
    let held: bool =
        sqlx::query_scalar("SELECT manual_hold FROM qintopia_agent_os.welcome_cases WHERE id=$1")
            .bind(f.case)
            .fetch_one(&f.store.pool)
            .await?;
    assert!(held);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_unchanged_content_cannot_hide_unlinked_or_replaced_identity_relations(
) -> Result<()> {
    for mutation in 0..7 {
        let f = Fixture::with_intake(true).await?;
        let subject = f.subject().await?;
        f.store
            .welcome_review_decide(&subject, &f.decision())
            .await?;
        match mutation {
            0 => {
                sqlx::query(
                    "UPDATE qintopia_agent_os.welcome_applications SET person_id=NULL WHERE id=$1",
                )
                .bind(f.application)
                .execute(&f.store.pool)
                .await?;
            }
            1 => {
                sqlx::query(
                    "UPDATE qintopia_agent_os.welcome_applications SET person_id=$2 WHERE id=$1",
                )
                .bind(f.application)
                .bind(f.store.verified_person(&f.owner).await?)
                .execute(&f.store.pool)
                .await?;
            }
            2 => {
                sqlx::query(
                    "UPDATE qintopia_agent_os.welcome_cases SET application_id=NULL WHERE id=$1",
                )
                .bind(f.case)
                .execute(&f.store.pool)
                .await?;
            }
            3 => {
                sqlx::query("UPDATE qintopia_agent_os.welcome_cases SET person_id=$2 WHERE id=$1")
                    .bind(f.case)
                    .bind(f.store.verified_person(&f.owner).await?)
                    .execute(&f.store.pool)
                    .await?;
            }
            4 => {
                sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked',version=version+1 WHERE id=$1").bind(f.channel).execute(&f.store.pool).await?;
                sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='confirmed',version=version+1 WHERE id=$1").bind(f.channel).execute(&f.store.pool).await?;
            }
            5 => {
                sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked',version=version+1 WHERE id=(SELECT identity_link_id FROM qintopia_agent_os.welcome_cases WHERE id=$1)").bind(f.case).execute(&f.store.pool).await?;
                sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='confirmed',version=version+1 WHERE id=(SELECT identity_link_id FROM qintopia_agent_os.welcome_cases WHERE id=$1)").bind(f.case).execute(&f.store.pool).await?;
            }
            _ => {
                sqlx::query("UPDATE qintopia_agent_os.business_property_bindings SET active=false WHERE tenant_key=$1").bind(&f.store.tenant).execute(&f.store.pool).await?;
            }
        }
        f.assert_publish(f.artifact, false).await?;
        let mut review = f.decision();
        review.expected_version = 2;
        review.confirm_application_stay = false;
        review.confirm_channel_person = false;
        assert!(
            f.store
                .welcome_review_decide(&subject, &review)
                .await
                .is_err(),
            "mutation {mutation}"
        );
        f.reopen(vec![f.artifact]).await?;
        let list = f.store.welcome_review_list(&subject, f.scope).await?;
        assert_eq!(
            list["items"][0]["identity_confirmed"], false,
            "mutation {mutation}"
        );
        assert_eq!(list["items"][0]["content_confirmed"], false);
        review.expected_version = 3;
        assert!(f
            .store
            .welcome_review_decide(&subject, &review)
            .await
            .unwrap_err()
            .to_string()
            .contains("identity_segments_required"));
        f.assert_publish(f.artifact, false).await?;
    }
    Ok(())
}
