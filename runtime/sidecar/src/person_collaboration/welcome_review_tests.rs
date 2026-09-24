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

impl Fixture {
    async fn host(&self, arguments: serde_json::Value, message: &str) -> Result<serde_json::Value> {
        let chat: String =
            sqlx::query_scalar("SELECT chat_id FROM qintopia_messages.conversations WHERE id=$1")
                .bind(self.group)
                .fetch_one(&self.store.pool)
                .await?;
        let request = json!({"operation":"person_foundation_ingress","schema_version":1,"agent":"anan","tool":"welcome_group_host","trusted_context":{"platform":"wecom","chat_type":"group","chat_id":chat,"sender_id":"synthetic-service-account","message_id":message,"gateway_id":self.gateway},"arguments":arguments,"token":"simulated-host-token"});
        super::foundation_server::broker_invoke(
            &self.store,
            &self.gateway,
            "anan",
            super::foundation_server::parse_broker_request(&serde_json::to_vec(&request)?)?,
        )
        .await
    }
    async fn group_text(&self, text: &str, authenticated: bool) -> Result<String> {
        let event = Uuid::new_v4().to_string();
        let raw:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.raw_events(event_id,source,subject,received_at,payload,ingress_auth_verified) VALUES($1,'wecom','qintopia.wecom.raw.authenticated',clock_timestamp(),'{}',$2) RETURNING id").bind(&event).bind(authenticated).fetch_one(&self.store.pool).await?;
        sqlx::query("INSERT INTO qintopia_messages.messages(tenant_id,platform,message_id,event_id,chat_id,chat_type,sender_id,message_kind,text,sent_at,received_at,raw_event_id,raw) SELECT $1,'wecom',$2,$2,chat_id,'group','synthetic-service-account','text',$3,clock_timestamp(),clock_timestamp(),$4,'{}' FROM qintopia_messages.conversations WHERE id=$5").bind(&self.store.tenant).bind(&event).bind(text).bind(raw).bind(self.group).execute(&self.store.pool).await?;
        Ok(event)
    }
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_host_presentation_authenticated_text_and_ui_receipt() -> Result<()> {
    let f = Fixture::new().await?;
    let prepared = f
        .host(json!({"action":"prepare","work_item":f.work}), "")
        .await?;
    let id = &prepared["presentation"];
    assert_eq!(
        f.host(json!({"action":"prepare","work_item":f.work}), "")
            .await?["presentation"],
        *id
    );
    let claim = f
        .host(json!({"action":"claim","presentation":id}), "")
        .await?;
    assert_eq!(claim["send"], true);
    assert!(!claim["text"]
        .as_str()
        .unwrap()
        .contains(&f.person.to_string()));
    assert_eq!(
        f.host(json!({"action":"claim","presentation":id}), "")
            .await?["send"],
        false
    );
    f.host(json!({"action":"receipt","presentation":id,"claim":claim["claim"],"outcome":"delivered","receipt":"simulated-only-receipt"}),"").await?;
    let command = format!(
        "确认 {} 人员1 账号1 关联住宿 关联账号 内容",
        prepared["reference"].as_str().unwrap()
    );
    let bad = f.group_text(&command, false).await?;
    assert!(f.host(json!({"action":"callback"}), &bad).await.is_err());
    let vague = f.group_text("同意", true).await?;
    assert!(f.host(json!({"action":"callback"}), &vague).await.is_err());
    let event = f.group_text(&command, true).await?;
    let result = f.host(json!({"action":"callback"}), &event).await?;
    assert_eq!(result["status"], "confirmed");
    assert_eq!(f.host(json!({"action":"callback"}), &event).await?, result);
    let s = f.subject().await?;
    assert_eq!(
        f.store.welcome_review_list(&s, f.scope).await?["items"][0]["status"],
        "confirmed"
    );
    let history = f
        .store
        .welcome_receipt_page(&s, f.scope, f.work, &Default::default())
        .await?;
    assert_eq!(history["items"].as_array().unwrap().len(), 1);
    assert_eq!(history["items"][0]["label"], "模拟小客服");
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_host_unknown_never_reclaims_and_settles_original() -> Result<()> {
    let f = Fixture::new().await?;
    let p = f
        .host(json!({"action":"prepare","work_item":f.work}), "")
        .await?;
    let id = &p["presentation"];
    let claim = f
        .host(json!({"action":"claim","presentation":id}), "")
        .await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_group_presentations SET claim_until=clock_timestamp()-interval '1 second' WHERE id=$1").bind(serde_json::from_value::<Uuid>(id.clone())?).execute(&f.store.pool).await?;
    assert_eq!(
        f.host(json!({"action":"status","presentation":id}), "")
            .await?["status"],
        "unknown"
    );
    assert_eq!(
        f.host(json!({"action":"claim","presentation":id}), "")
            .await?["send"],
        false
    );
    assert!(f.host(json!({"action":"receipt","presentation":id,"claim":Uuid::new_v4(),"outcome":"delivered","receipt":"simulated"}),"").await.is_err());
    assert!(f.host(json!({"action":"receipt","presentation":id,"claim":claim["claim"],"outcome":"failed","receipt":null}),"").await.is_err());
    assert_eq!(f.host(json!({"action":"receipt","presentation":id,"claim":claim["claim"],"outcome":"delivered","receipt":"original-simulated-receipt"}),"").await?["status"],"delivered");
    assert_eq!(
        f.host(json!({"action":"claim","presentation":id}), "")
            .await?["send"],
        false
    );
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_host_source_config_and_authority_changes_invalidate() -> Result<()> {
    for change in ["source", "config", "authority", "channel"] {
        let f = Fixture::new().await?;
        let p = f
            .host(json!({"action":"prepare","work_item":f.work}), "")
            .await?;
        match change {
            "source" => {
                sqlx::query("UPDATE qintopia_agent_os.welcome_applications SET revision=revision+1 WHERE id=$1").bind(f.application).execute(&f.store.pool).await?;
            }
            "config" => {
                sqlx::query("UPDATE qintopia_agent_os.welcome_review_settings SET version=version+1 WHERE tenant_key=$1").bind(&f.store.tenant).execute(&f.store.pool).await?;
            }
            "authority" => {
                sqlx::query("UPDATE qintopia_agent_os.collaboration_grants SET status='revoked' WHERE tenant_key=$1").bind(&f.store.tenant).execute(&f.store.pool).await?;
            }
            _ => {
                sqlx::query("UPDATE qintopia_identity.source_identity_links SET version=version+1 WHERE id=$1").bind(f.channel).execute(&f.store.pool).await?;
            }
        }
        let c = f
            .host(
                json!({"action":"claim","presentation":p["presentation"]}),
                "",
            )
            .await?;
        assert_eq!(c["send"], false, "{change}");
        assert_eq!(c["status"], "stale", "{change}");
        if change == "channel" {
            let refreshed = f
                .host(json!({"action":"prepare","work_item":f.work}), "")
                .await?;
            assert_ne!(refreshed["reference"], p["reference"]);
            assert_eq!(
                f.host(
                    json!({"action":"claim","presentation":refreshed["presentation"]}),
                    ""
                )
                .await?["send"],
                true
            );
        }
    }
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_pages_over_100_scope_search_and_receipts() -> Result<()> {
    let f = Fixture::new().await?;
    for n in 0..103 {
        let work:Uuid=sqlx::query_scalar("INSERT INTO qintopia_agent_os.work_items(work_item_type,status,requester_agent,target_agent,capability_key,brief_summary,purpose,dedupe_key,idempotency_key,payload) SELECT work_item_type,status,requester_agent,target_agent,capability_key,brief_summary,purpose,$2,$2,payload FROM qintopia_agent_os.work_items WHERE id=$1 RETURNING id").bind(f.work).bind(Uuid::new_v4().to_string()).fetch_one(&f.store.pool).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_review_items(work_item_id,tenant_key,scope_id,case_id,application_id,configuration_version,snapshot,candidates) SELECT $2,tenant_key,scope_id,case_id,application_id,configuration_version,snapshot,$3 FROM qintopia_agent_os.welcome_review_items WHERE work_item_id=$1").bind(f.work).bind(work).bind(json!([{"person":f.person,"label":format!("模拟分页居民{n}"),"confirmed":false}])).execute(&f.store.pool).await?;
    }
    let s = f.subject().await?;
    let mut q = super::store::welcome_pages::Page::default();
    let mut ids = std::collections::HashSet::new();
    loop {
        let p = f.store.welcome_review_page(&s, f.scope, &q).await?;
        for i in p["items"].as_array().unwrap() {
            assert!(ids.insert(i["work_item"].to_string()));
        }
        if p["has_more"] != true {
            break;
        }
        q.page += 1;
    }
    assert_eq!(ids.len(), 104);
    q.page = 0;
    q.search = "居民102".into();
    assert_eq!(
        f.store.welcome_review_page(&s, f.scope, &q).await?["items"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert!(f
        .store
        .welcome_review_page(&s, Uuid::new_v4(), &q)
        .await
        .is_err());
    assert!(f
        .store
        .welcome_receipt_page(&s, Uuid::new_v4(), f.work, &q)
        .await
        .is_err());
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_person_conversation_compatibility_keeps_pms_separate() -> Result<()> {
    let f = Fixture::new().await?;
    let s = f.subject().await?;
    f.store.welcome_review_decide(&s, &f.decision()).await?;
    let gateway:String=sqlx::query_scalar("SELECT gateway_key FROM qintopia_identity.person_identity_gateways g JOIN qintopia_identity.source_identity_links l ON l.namespace=g.namespace AND l.subject_type=g.subject_type WHERE l.id=$1").bind(f.channel).fetch_one(&f.store.pool).await?;
    let actor = f
        .store
        .conversation_actor(&gateway, "synthetic-resident")
        .await?;
    assert_eq!(f.store.verified_person(&actor).await?, f.person);
    assert!(f
        .store
        .gateway_actor(&gateway, "synthetic-resident")
        .await
        .is_err());
    assert!(f
        .store
        .conversation_actor(&f.gateway, "synthetic-service-account")
        .await
        .is_err());
    // The original confirmer can leave office without erasing an independent identity fact.
    sqlx::query("UPDATE qintopia_identity.work_accounts SET active=false WHERE id=$1")
        .bind(f.account)
        .execute(&f.store.pool)
        .await?;
    assert_eq!(f.store.verified_person(&actor).await?, f.person);
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET adapter_metadata=jsonb_set(adapter_metadata,'{display_name}','\"新昵称\"') WHERE id=$1").bind(f.channel).execute(&f.store.pool).await?;
    assert_eq!(f.store.verified_person(&actor).await?, f.person);
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked',version=version+1 WHERE id=$1").bind(f.channel).execute(&f.store.pool).await?;
    assert!(f.store.verified_person(&actor).await.is_err());
    assert!(f
        .store
        .conversation_actor(&gateway, "synthetic-resident")
        .await
        .is_err());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_person_real_broker_group_and_direct_use_same_link() -> Result<()> {
    let f = Fixture::new().await?;
    f.store
        .welcome_review_decide(&f.subject().await?, &f.decision())
        .await?;
    let gateway:String=sqlx::query_scalar("SELECT gateway_key FROM qintopia_identity.person_identity_gateways g JOIN qintopia_identity.source_identity_links l ON l.namespace=g.namespace AND l.subject_type=g.subject_type WHERE l.id=$1").bind(f.channel).fetch_one(&f.store.pool).await?;
    let chat = format!("simulated-person-group-{}", Uuid::new_v4());
    let group:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type,display_name) VALUES($1,'qiwe',$2,'group','模拟居民群') RETURNING id").bind(&f.store.tenant).bind(&chat).fetch_one(&f.store.pool).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3)").bind(&f.store.tenant).bind(f.scope).bind(group).execute(&f.store.pool).await?;
    for kind in ["direct", "group"] {
        let event = Uuid::new_v4().to_string();
        let raw:Uuid=sqlx::query_scalar("INSERT INTO qintopia_messages.raw_events(event_id,source,subject,received_at,payload,ingress_auth_verified) VALUES($1,'qiwe','qintopia.qiwe.raw.authenticated',clock_timestamp(),'{}',true) RETURNING id").bind(&event).fetch_one(&f.store.pool).await?;
        sqlx::query("INSERT INTO qintopia_messages.messages(tenant_id,platform,message_id,event_id,chat_id,chat_type,sender_id,message_kind,text,sent_at,received_at,raw_event_id,raw) VALUES($1,'qiwe',$2,$2,$3,$4,'synthetic-resident','text','你好',clock_timestamp(),clock_timestamp(),$5,'{}')").bind(&f.store.tenant).bind(&event).bind(&chat).bind(kind).bind(raw).execute(&f.store.pool).await?;
        let request = json!({"operation":"person_foundation_tool","schema_version":1,"agent":"erhua","tool":"context","trusted_context":{"platform":"qiwe","chat_type":kind,"chat_id":chat,"sender_id":"synthetic-resident","message_id":event,"gateway_id":gateway},"arguments":{},"token":"simulated-test"});
        let result = super::foundation_server::broker_invoke(
            &f.store,
            &gateway,
            "erhua",
            super::foundation_server::parse_broker_request(&serde_json::to_vec(&request)?)?,
        )
        .await?;
        if kind == "group" {
            assert_eq!(result["identity"], json!({"identity_status":"confirmed"}));
            assert_eq!(result["memory"]["disclosure_allowed"], false);
        } else {
            assert!(result["identity"].is_object());
        }
    }
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_host_model_token_wrong_profile_and_gateway_are_denied() -> Result<()> {
    use std::os::unix::fs::PermissionsExt;
    use tokio::io::{AsyncBufReadExt, AsyncWriteExt, BufReader};
    let f = Fixture::new().await?;
    let dir = tempfile::tempdir()?;
    std::fs::set_permissions(dir.path(), std::fs::Permissions::from_mode(0o700))?;
    let socket = dir.path().join("host.sock");
    let model_token = "simulated-model-token-0000000000000000";
    let host_token = "simulated-host-token-00000000000000000";
    let vars = [
        (
            "QINTOPIA_FOUNDATION_SOCKET",
            socket.to_string_lossy().into_owned(),
        ),
        ("QINTOPIA_FOUNDATION_TOKEN", model_token.into()),
        ("QINTOPIA_FOUNDATION_HOST_TOKEN", host_token.into()),
        ("QINTOPIA_FOUNDATION_GATEWAY_ID", f.gateway.clone()),
        ("QINTOPIA_FOUNDATION_PROFILE", "anan".into()),
    ];
    let previous: Vec<_> = vars
        .iter()
        .map(|(k, _)| (*k, std::env::var_os(k)))
        .collect();
    for (k, v) in &vars {
        std::env::set_var(k, v);
    }
    let store = Store {
        pool: f.store.pool.clone(),
        tenant: f.store.tenant.clone(),
    };
    let server = tokio::spawn(super::foundation_server::broker(store));
    let result:Result<()>=async{
        let mut connected=None;
        for _ in 0..100 {
            if let Ok(s)=tokio::net::UnixStream::connect(&socket).await {connected=Some(s);break;}
            tokio::task::yield_now().await;
        }
        // Bounded timeout awaits the listener, not a blind sleep.
        if connected.is_none(){connected=Some(tokio::time::timeout(std::time::Duration::from_secs(3),async{loop{if let Ok(s)=tokio::net::UnixStream::connect(&socket).await {return s;}tokio::task::yield_now().await;}}).await?);}

        // Exercise the same host after an empty readiness connection and another
        // client that closes before this current-thread broker can accept it.
        drop(connected.take());
        drop(std::os::unix::net::UnixStream::connect(&socket)?);
        for (token,agent,gateway,denied) in [(model_token,"anan",f.gateway.as_str(),true),(host_token,"erhua",f.gateway.as_str(),true),(host_token,"anan","wrong-gateway",true),(host_token,"anan",f.gateway.as_str(),false)] {
            let request=json!({"operation":"person_foundation_ingress","schema_version":1,"agent":agent,"tool":"welcome_group_host","trusted_context":{"platform":"wecom","chat_type":"group","chat_id":"","sender_id":"","message_id":"","gateway_id":gateway},"arguments":{"action":"pending"},"token":token});
            let stream=if let Some(stream)=connected.take(){stream}else{tokio::net::UnixStream::connect(&socket).await?};let (r,mut w)=stream.into_split();let mut bytes=serde_json::to_vec(&request)?;bytes.push(b'\n');w.write_all(&bytes).await?;
            let mut output=String::new();tokio::time::timeout(std::time::Duration::from_secs(3),BufReader::new(r).read_line(&mut output)).await??;
            let value:serde_json::Value=serde_json::from_str(&output)?;assert_eq!(value["ok"],!denied);
        }
        Ok(())
    }.await;
    server.abort();
    let _ = server.await;
    for (k, v) in previous {
        if let Some(v) = v {
            std::env::set_var(k, v);
        } else {
            std::env::remove_var(k);
        }
    }
    result
}

fn matching_observation(
    fields: &serde_json::Value,
    valid: bool,
    consent: bool,
) -> super::store::applications::Observation {
    super::store::applications::Observation{identity_hash:super::digest(&serde_json::to_vec(&json!({"name":fields["name"],"nickname":fields["nickname"],"phone":fields["phone"]})).unwrap()),field_hash:super::digest(&serde_json::to_vec(&json!({"fields":fields,"valid":valid,"consent_active":consent})).unwrap()),valid,consent_active:consent,source_version:Some("2".into())}
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_source_projection_exact_fields_expiry_and_withdrawal() -> Result<()> {
    let f = Fixture::with_intake(true).await?;
    let binding:Uuid=sqlx::query_scalar("SELECT binding_id FROM qintopia_agent_os.application_intake_states WHERE application_id=$1").bind(f.application).fetch_one(&f.store.pool).await?;
    let fields = json!({"name":" 张晓明 ","nickname":"小明","phone":null,"consent":true,"arrival":"2026-09-25"});
    let observation = matching_observation(&fields, true, true);
    // Golden digest produced independently by Python's sorted compact UTF-8 JSON.
    assert_eq!(
        observation.identity_hash,
        "b77df8e71426736f5cfec391380e2d378d529057ecd719ceafde02fea2d1bdd8"
    );
    f.readback(&observation).await?;
    let mut request = super::store::welcome_candidates::SourceProjection {
        binding,
        application: f.application,
        fields: fields.clone(),
    };
    assert_eq!(
        f.store.welcome_project_source(&request).await?["stored"],
        true
    );
    let before:chrono::DateTime<Utc>=sqlx::query_scalar("SELECT expires_at FROM qintopia_agent_os.welcome_source_projections WHERE application_id=$1").bind(f.application).fetch_one(&f.store.pool).await?;
    request.fields["name"] = json!("张晓明");
    assert!(f.store.welcome_project_source(&request).await.is_err());
    request.fields = fields.clone();
    request.fields.as_object_mut().unwrap().remove("phone");
    assert!(f.store.welcome_project_source(&request).await.is_err());
    request.fields = fields.clone();
    request.binding = Uuid::new_v4();
    assert!(f.store.welcome_project_source(&request).await.is_err());
    request.binding = binding;
    let subject = f.subject().await?;
    let candidates = f
        .store
        .welcome_candidates(&subject, f.scope, f.application)
        .await?;
    assert_eq!(candidates["automatic_binding"], false);
    assert!(candidates["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .all(|v| v["confirmed"] == false));
    assert!(f
        .store
        .welcome_candidates(&subject, Uuid::new_v4(), f.application)
        .await
        .is_err());
    for (valid, consent) in [(true, false), (false, true)] {
        f.readback(&matching_observation(&fields, valid, consent))
            .await?;
        assert!(f
            .store
            .welcome_project_source(&request)
            .await
            .unwrap_err()
            .to_string()
            .contains("source_not_usable"));
        assert!(f
            .store
            .welcome_candidates(&subject, f.scope, f.application)
            .await
            .is_err());
        let after:chrono::DateTime<Utc>=sqlx::query_scalar("SELECT expires_at FROM qintopia_agent_os.welcome_source_projections WHERE application_id=$1").bind(f.application).fetch_one(&f.store.pool).await?;
        assert_eq!(before, after);
    }
    // Character count contract is not a byte limit. Values stay unchanged for hashing.
    request.fields["phone"] = json!("字".repeat(40));
    f.readback(&matching_observation(&request.fields, true, true))
        .await?;
    assert_eq!(
        f.store.welcome_project_source(&request).await?["stored"],
        true
    );
    sqlx::query("UPDATE qintopia_agent_os.welcome_source_projections SET expires_at=clock_timestamp()-interval '1 second' WHERE application_id=$1").bind(f.application).execute(&f.store.pool).await?;
    assert!(f
        .store
        .welcome_candidates(&subject, f.scope, f.application)
        .await
        .is_err());
    request.fields["phone"] = json!("字".repeat(41));
    f.readback(&matching_observation(&request.fields, true, true))
        .await?;
    assert!(f.store.welcome_project_source(&request).await.is_err());
    Ok(())
}

async fn new_person_fixture() -> Result<Fixture> {
    let f = Fixture::with_intake(true).await?;
    let fields =
        json!({"name":"模拟新居民","nickname":"模拟小新","phone":"13800000001","consent":true});
    f.readback(&matching_observation(&fields, true, true))
        .await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_applications SET person_id=NULL WHERE id=$1")
        .bind(f.application)
        .execute(&f.store.pool)
        .await?;
    let binding:Uuid=sqlx::query_scalar("SELECT binding_id FROM qintopia_agent_os.application_intake_states WHERE application_id=$1").bind(f.application).fetch_one(&f.store.pool).await?;
    f.store
        .welcome_project_source(&super::store::welcome_candidates::SourceProjection {
            binding,
            application: f.application,
            fields,
        })
        .await?;
    f.reopen(vec![]).await?;
    Ok(f)
}
fn create_person_request(work: Uuid, version: i64) -> ReviewDecision {
    ReviewDecision {
        operation_id: Uuid::new_v4(),
        work_item: work,
        expected_version: version,
        person: None,
        channel: None,
        confirm_application_stay: false,
        confirm_channel_person: false,
        confirm_content: false,
        decision: "create_person".into(),
    }
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_source_bound_person_creation_replay_concurrency_and_no_implicit_effects(
) -> Result<()> {
    let f = new_person_fixture().await?;
    let subject = f.subject().await?;
    let page = f.store.welcome_review_list(&subject, f.scope).await?;
    let item = &page["items"][0];
    assert_eq!(item["new_person"]["name"], "模拟新居民");
    let request = create_person_request(f.work, item["version"].as_i64().unwrap());
    let other = create_person_request(f.work, request.expected_version);
    let (first, second) = tokio::join!(
        f.store.welcome_review_decide(&subject, &request),
        f.store.welcome_review_decide(&subject, &other)
    );
    assert_ne!(first.is_ok(), second.is_ok());
    let (result, used) = if let Ok(r) = first {
        (r, &request)
    } else {
        (second?, &other)
    };
    let person: Uuid = serde_json::from_value(result["created_person"].clone())?;
    assert_eq!(f.store.welcome_review_decide(&subject, used).await?, result);
    assert_eq!(result["identity_confirmed"], false);
    assert_eq!(result["channel_confirmed"], false);
    assert_eq!(result["content_confirmed"], false);
    let row=sqlx::query("SELECT c.person_id AS case_person,a.person_id AS app_person FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_applications a ON a.id=$2 WHERE c.id=$1").bind(f.case).bind(f.application).fetch_one(&f.store.pool).await?;
    assert!(row.get::<Option<Uuid>, _>("case_person").is_none());
    assert!(row.get::<Option<Uuid>, _>("app_person").is_none());
    let after = f.store.welcome_review_list(&subject, f.scope).await?;
    assert!(after["items"][0]["candidates"]
        .as_array()
        .unwrap()
        .iter()
        .any(|p| p["person"] == json!(person)));
    let repeated = create_person_request(f.work, result["version"].as_i64().unwrap());
    assert!(f
        .store
        .welcome_review_decide(&subject, &repeated)
        .await
        .is_err());
    let mut revoke = repeated;
    revoke.decision = "revoke".into();
    f.store.welcome_review_decide(&subject, &revoke).await?;
    let links:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_identity.source_identity_links WHERE evidence_ref=$1 AND status='revoked'").bind(used.operation_id).fetch_one(&f.store.pool).await?;
    assert_eq!(links, 1);
    let exists: bool =
        sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_identity.persons WHERE id=$1)")
            .bind(person)
            .fetch_one(&f.store.pool)
            .await?;
    assert!(exists);
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_group_explicit_person_creation_and_stale_presentation() -> Result<()> {
    let f = new_person_fixture().await?;
    let p = f
        .host(json!({"action":"prepare","work_item":f.work}), "")
        .await?;
    assert!(p["text"].as_str().unwrap().contains("模拟新居民"));
    let c = f
        .host(
            json!({"action":"claim","presentation":p["presentation"]}),
            "",
        )
        .await?;
    f.host(json!({"action":"receipt","presentation":p["presentation"],"claim":c["claim"],"outcome":"delivered","receipt":"simulated"}),"").await?;
    let event = f
        .group_text(&format!("建档 {}", p["reference"].as_str().unwrap()), true)
        .await?;
    let created = f.host(json!({"action":"callback"}), &event).await?;
    assert!(created["created_person"].is_string());
    assert_eq!(created["status"], "pending");
    assert_eq!(f.host(json!({"action":"callback"}), &event).await?, created);
    let stale = f
        .group_text(
            &format!("确认 {} 人员1 关联住宿", p["reference"].as_str().unwrap()),
            true,
        )
        .await?;
    assert!(f.host(json!({"action":"callback"}), &stale).await.is_err());
    let new = f
        .host(json!({"action":"prepare","work_item":f.work}), "")
        .await?;
    assert_ne!(new["reference"], p["reference"]);
    Ok(())
}
#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn welcome_source_bound_person_creation_rejects_expiry_existing_and_changes() -> Result<()> {
    for change in ["expiry", "source", "existing", "permission"] {
        let f = new_person_fixture().await?;
        let s = f.subject().await?;
        let p = f.store.welcome_review_list(&s, f.scope).await?;
        let request = create_person_request(f.work, p["items"][0]["version"].as_i64().unwrap());
        match change {
            "expiry" => {
                sqlx::query("UPDATE qintopia_agent_os.welcome_source_projections SET expires_at=clock_timestamp()-interval '1 second' WHERE application_id=$1").bind(f.application).execute(&f.store.pool).await?;
            }
            "source" => {
                sqlx::query("UPDATE qintopia_agent_os.welcome_applications SET revision=revision+1 WHERE id=$1").bind(f.application).execute(&f.store.pool).await?;
            }
            "existing" => {
                sqlx::query(
                    "UPDATE qintopia_agent_os.welcome_applications SET person_id=$2 WHERE id=$1",
                )
                .bind(f.application)
                .bind(f.person)
                .execute(&f.store.pool)
                .await?;
            }
            _ => {
                sqlx::query("UPDATE qintopia_agent_os.welcome_review_subject_grants SET revoked_at=clock_timestamp() WHERE tenant_key=$1 AND subject_kind='work_account'").bind(&f.store.tenant).execute(&f.store.pool).await?;
            }
        }
        assert!(
            f.store.welcome_review_decide(&s, &request).await.is_err(),
            "{change}"
        );
        let count: i64 = sqlx::query_scalar(
            "SELECT count(*) FROM qintopia_agent_os.welcome_review_receipts WHERE id=$1",
        )
        .bind(request.operation_id)
        .fetch_one(&f.store.pool)
        .await?;
        assert_eq!(count, 0);
    }
    Ok(())
}

#[path = "welcome_contacts_tests.rs"]
mod contacts;
