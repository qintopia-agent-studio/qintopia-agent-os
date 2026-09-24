//! Real shared services; only source observations and channel ingress are synthetic.
#![cfg(feature = "postgres-integration-tests")]
use super::{
    Actor, MemoryChange, MemoryCommand, MemoryEvidence, ReplyCondition, ReplyStyle, Store,
};
use anyhow::Result;
use chrono::{Duration, Utc};
use serde_json::{json, Value};
use uuid::Uuid;

async fn fixture() -> Result<(Store, Actor, Value)> {
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let store = Store::local(
        &database,
        &format!("synthetic-collaboration-memory-{}", Uuid::new_v4()),
    )
    .await?;
    crate::db::run_migrations(&store.pool).await?;
    store.bootstrap_fixture().await?;
    let seeded = store.bootstrap_identity_memory_fixture().await?;
    let actor = store
        .gateway_actor("synthetic-qiwe-one", "synthetic-resident")
        .await?;
    Ok((store, actor, seeded))
}
fn command(version: i64, change: MemoryChange) -> MemoryCommand {
    MemoryCommand {
        operation_id: Uuid::new_v4(),
        expected_version: version,
        change,
    }
}
fn set(version: i64, condition: ReplyCondition, style: ReplyStyle) -> MemoryCommand {
    command(version, MemoryChange::Set { condition, style })
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn memory_self_revision_stop_replay_and_real_answer_consumer() -> Result<()> {
    let (store, actor, seed) = fixture().await?;
    let person: Uuid = serde_json::from_value(seed["person_ref"].clone())?;
    let before: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.work_items WHERE metadata::text LIKE $1",
    )
    .bind(format!("%{person}%"))
    .fetch_one(&store.pool)
    .await?;
    let at = Utc::now() - Duration::minutes(2);
    let initial = set(0, ReplyCondition::General, ReplyStyle::Brief);
    let source = MemoryEvidence::trusted(Uuid::new_v4(), at);
    assert_eq!(
        store.remember(&actor, &initial, &source).await?["version"],
        1
    );
    // A new Gateway/conversation sees the same current Person and preference.
    let other = store
        .gateway_actor("synthetic-wecom-two", "synthetic-resident")
        .await?;
    assert_eq!(
        store.memory_context(&other, "general").await?["reply_style"],
        "brief"
    );
    let conditional = set(1, ReplyCondition::Fees, ReplyStyle::Detailed);
    store
        .remember(
            &other,
            &conditional,
            &MemoryEvidence::trusted(Uuid::new_v4(), at + Duration::seconds(1)),
        )
        .await?;
    assert_eq!(
        store.memory_context(&actor, "fees").await?["reply_style"],
        "detailed"
    );
    assert_eq!(
        store.memory_context(&actor, "general").await?["reply_style"],
        "brief"
    );
    assert!(store
        .remember(
            &actor,
            &initial,
            &MemoryEvidence::trusted(Uuid::new_v4(), at)
        )
        .await
        .is_err());
    let stop = command(2, MemoryChange::Stop);
    store
        .remember(
            &actor,
            &stop,
            &MemoryEvidence::trusted(Uuid::new_v4(), at + Duration::seconds(2)),
        )
        .await?;
    assert_eq!(
        store.remember(&actor, &initial, &source).await?["replayed"],
        true
    );
    assert_eq!(
        store.memory_context(&other, "fees").await?["reply_style"],
        Value::Null
    );
    let stale = set(3, ReplyCondition::General, ReplyStyle::Brief);
    assert!(store
        .remember(&other, &stale, &MemoryEvidence::trusted(Uuid::new_v4(), at))
        .await
        .is_err());
    // Rebuilding an old worker snapshot must not revive withdrawn reply habits.
    sqlx::query("INSERT INTO qintopia_identity.member_profile_snapshots(person_id,profile_kind,profile_version,status,summary,communication_style,generated_by,input_hash) VALUES($1,'reply_context','synthetic-old','active','OLD WITHDRAWN STYLE', '{\"reply_style\":\"brief\"}','synthetic-old-worker',$2)")
        .bind(person).bind(Uuid::new_v4().to_string()).execute(&store.pool).await?;
    let chat = Uuid::new_v4().to_string();
    let sender = Uuid::new_v4().to_string();
    let channel_identity:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.channel_identities(person_id,platform,channel_user_id,chat_id,display_name,metadata) VALUES($1,'qiwe',$2,$3,'合成住客','{\"current_qiwe_room_member\":true}') RETURNING id")
        .bind(person).bind(&sender).bind(&chat).fetch_one(&store.pool).await?;
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET channel_identity_id=$2 WHERE namespace=$1 AND subject_type='qiwe_sender'")
        .bind(format!("{}/synthetic-qiwe-one",store.tenant)).bind(channel_identity).execute(&store.pool).await?;
    let config = crate::context_tools::ContextConfig {
        search: crate::message_search::SearchConfig {
            database_url: String::new(),
            db_max_connections: 1,
            embedding_endpoint: String::new(),
            embedding_api_key: String::new(),
            embedding_model: String::new(),
            allowed_caller: "erhua".into(),
        },
        allowed_callers: ["erhua".into()].into(),
        erhua_trainer_user_ids: Default::default(),
    };
    let actual=crate::context_tools::call_tool(&store.pool,&config,"qintopia_answer_context_prepare",json!({"caller_profile":"erhua","platform":"qiwe","chat_id":chat,"sender_id":sender,"message_text":"合成住客查询","purpose":"reply"})).await?;
    assert!(!actual.to_string().contains("OLD WITHDRAWN STYLE"));
    assert_eq!(
        actual["speaker"]["communication_style"]["self_reply_preference"]["status"],
        "stopped"
    );
    assert_eq!(
        actual["speaker"]["communication_style"]["self_reply_preference"]["reply_style"],
        Value::Null
    );
    // A new explicit choice may resume use; revoking the legacy channel link
    // must still prevent its old channel identity row from bypassing authority.
    store
        .remember(
            &other,
            &set(3, ReplyCondition::General, ReplyStyle::Brief),
            &MemoryEvidence::trusted(Uuid::new_v4(), at + Duration::seconds(3)),
        )
        .await?;
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked',version=version+1 WHERE namespace=$1 AND subject_type='qiwe_sender'")
        .bind(format!("{}/synthetic-qiwe-one",store.tenant)).execute(&store.pool).await?;
    let after_revocation=crate::context_tools::call_tool(&store.pool,&config,"qintopia_answer_context_prepare",json!({"caller_profile":"erhua","platform":"qiwe","chat_id":chat,"sender_id":sender,"message_text":"再次查询","purpose":"reply"})).await?;
    assert!(after_revocation["speaker"]["communication_style"]["self_reply_preference"].is_null());
    assert!(store.memory_context(&actor, "general").await.is_err());
    assert_eq!(
        store.memory_context(&other, "general").await?["reply_style"],
        "brief"
    );
    let after: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_agent_os.work_items WHERE metadata::text LIKE $1",
    )
    .bind(format!("%{person}%"))
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(before, after, "optional memory must not create staff tasks");
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn gateway_identity_same_person_scope_shared_unknown_and_revocation() -> Result<()> {
    let (store, one, _) = fixture().await?;
    let two = store
        .gateway_actor("synthetic-wecom-two", "synthetic-resident")
        .await?;
    let a = store.identity_context(&one).await?;
    let b = store.identity_context(&two).await?;
    assert_eq!(a["person_ref"], b["person_ref"]);
    assert_eq!(a["scope"]["label"], "一栋");
    assert_eq!(b["scope"]["label"], "二栋");
    assert!(store
        .gateway_actor("synthetic-qiwe-one", "人员甲")
        .await
        .is_err());
    sqlx::query("UPDATE qintopia_identity.person_identity_gateways SET account_kind='shared',version=version+1 WHERE tenant_key=$1 AND gateway_key='synthetic-wecom-two'")
        .bind(&store.tenant).execute(&store.pool).await?;
    assert!(store
        .gateway_actor("synthetic-wecom-two", "synthetic-resident")
        .await
        .is_err());
    assert!(store.memory_context(&two, "general").await.is_err());
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked',version=version+1 WHERE namespace=$1 AND subject_type='qiwe_sender'")
        .bind(format!("{}/synthetic-qiwe-one",store.tenant)).execute(&store.pool).await?;
    assert!(store
        .remember(
            &one,
            &set(0, ReplyCondition::General, ReplyStyle::Brief),
            &MemoryEvidence::trusted(Uuid::new_v4(), Utc::now())
        )
        .await
        .is_err());
    let facts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_identity.member_facts WHERE person_id=$1",
    )
    .bind(serde_json::from_value::<Uuid>(a["person_ref"].clone())?)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(facts, 0);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn memory_stay_history_return_checkout_reorder_and_unknown() -> Result<()> {
    use crate::resident_welcome::state::{Occupant, Snapshot, StayState};
    let (store, actor, seed) = fixture().await?;
    let source = format!("synthetic-memory-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_sources(source_instance,property_id,mode,rebuilding,enabled) VALUES($1,'fixture-property','synthetic',false,false)")
        .bind(&source).execute(&store.pool).await?;
    let database = crate::foundation_test_support::database_url("QINTOPIA_COLLABORATION_TEST")?;
    let welcome = crate::resident_welcome::store::Store::local(&database).await?;
    let mut snapshot = Snapshot {
        source_hash: None,
        source: source.clone(),
        property: "fixture-property".into(),
        order: "first-order".into(),
        revision: "1".into(),
        stay: "first-stay".into(),
        state: StayState::InHouse,
        inventory_reserved: true,
        current_arrangement: true,
        building: "一栋".into(),
        occupants: vec![Occupant {
            id: "actual-person".into(),
            active: true,
        }],
        related_revisions: Default::default(),
        observed_at: Utc::now() - Duration::minutes(30),
        business_date: Utc::now().date_naive(),
    };
    welcome.apply_snapshot(None, &snapshot).await?;
    let person: Uuid = serde_json::from_value(seed["person_ref"].clone())?;
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET person_id=$2,status='confirmed',evidence_ref=$3,confirmed_by=$2 WHERE namespace=$1 AND source_ref='actual-person'")
        .bind(format!("pms/{source}/fixture-property/occupant")).bind(person).bind(Uuid::new_v4()).execute(&store.pool).await?;
    assert_eq!(
        store.person_history(&actor).await?["current_state"],
        "in_community"
    );
    welcome.apply_snapshot(None, &snapshot).await?;
    assert_eq!(
        store.person_history(&actor).await?["stays"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    let earlier = snapshot.clone();
    snapshot.revision = "2".into();
    snapshot.state = StayState::Terminated;
    snapshot.observed_at += Duration::minutes(1);
    welcome.apply_snapshot(None, &snapshot).await?;
    welcome.apply_snapshot(None, &earlier).await?;
    let away = store.person_history(&actor).await?;
    assert_eq!(away["current_state"], "not_in_community");
    assert_eq!(away["long_term_member"], true);
    snapshot.order = "return-order".into();
    snapshot.stay = "return-stay".into();
    snapshot.revision = "1".into();
    snapshot.state = StayState::InHouse;
    snapshot.observed_at += Duration::minutes(1);
    welcome.apply_snapshot(None, &snapshot).await?;
    let returned = store.person_history(&actor).await?;
    assert_eq!(returned["current_state"], "in_community");
    assert_eq!(returned["stays"].as_array().unwrap().len(), 2);
    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET invalidated=true,conflicted=true WHERE source_instance=$1")
        .bind(&source).execute(&store.pool).await?;
    assert_eq!(
        store.person_history(&actor).await?["current_state"],
        "unknown"
    );
    let other = store
        .gateway_actor("synthetic-wecom-two", "synthetic-resident")
        .await?;
    assert!(store.person_history(&other).await?["stays"]
        .as_array()
        .unwrap()
        .is_empty());
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn identity_source_pair_requires_scoped_confirmation_and_retains_revoke_history() -> Result<()>
{
    let (store, resident, seed) = fixture().await?;
    let pair = super::QiweConversion {
        external_gateway: "synthetic-wecom-two".into(),
        user_id: "new-account".into(),
        open_user_id: "external-account".into(),
        evidence_ref: Uuid::new_v4(),
    };
    let conversion = store
        .observe_qiwe_conversion("synthetic-qiwe-one", &pair)
        .await?;
    assert_eq!(conversion["person_confirmed"], false);
    assert!(store
        .gateway_actor("synthetic-qiwe-one", "new-account")
        .await
        .is_err());
    let observation = store
        .observe_gateway_subject("synthetic-qiwe-one", "new-account", Uuid::new_v4())
        .await?;
    let person = serde_json::from_value(seed["person_ref"].clone())?;
    let mut change = super::IdentityCommand {
        operation_id: Uuid::new_v4(),
        link_ref: serde_json::from_value(observation["link_ref"].clone())?,
        person_ref: person,
        expected_version: 1,
        evidence_ref: Uuid::new_v4(),
        revoke: false,
    };
    assert!(store.identity_change(&resident, &change).await.is_err());
    let owner = store.actor(store.fixture_operator().await?).await?;
    assert_eq!(
        store.identity_change(&owner, &change).await?["status"],
        "confirmed"
    );
    let newly_linked = store
        .gateway_actor("synthetic-qiwe-one", "new-account")
        .await?;
    assert_eq!(
        store.identity_context(&newly_linked).await?["person_ref"],
        seed["person_ref"]
    );
    assert_eq!(
        store.identity_change(&owner, &change).await?["replayed"],
        true
    );
    change.operation_id = Uuid::new_v4();
    change.expected_version = 2;
    change.revoke = true;
    store.identity_change(&owner, &change).await?;
    assert!(store.identity_context(&newly_linked).await.is_err());
    let retained: Value = sqlx::query_scalar(
        "SELECT adapter_metadata FROM qintopia_identity.source_identity_links WHERE id=$1",
    )
    .bind(change.link_ref)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!(
        retained["account_conversion_evidence"]
            .as_array()
            .unwrap()
            .len(),
        1
    );
    assert_eq!(retained["identity_history"].as_array().unwrap().len(), 2);
    Ok(())
}

#[tokio::test]
#[ignore = "explicit task-isolated local database required"]
async fn legacy_trainer_allowlist_cannot_bypass_adopted_or_revoked_foundation() -> Result<()> {
    let (store, _, seed) = fixture().await?;
    let person: Uuid = serde_json::from_value(seed["person_ref"].clone())?;
    let chat = format!("legacy-context-{}", Uuid::new_v4());
    let marker = Uuid::new_v4().to_string();
    let channel = format!("legacy-channel-{}", Uuid::new_v4());
    sqlx::query("INSERT INTO qintopia_identity.channel_identities(person_id,platform,channel_user_id,chat_id,display_name,metadata) VALUES($1,'qiwe',$2,$3,'合成受控住客','{\"current_qiwe_room_member\":true}')")
        .bind(person).bind(&channel).bind(&chat).execute(&store.pool).await?;
    let config = crate::context_tools::ContextConfig {
        search: crate::message_search::SearchConfig {
            database_url: String::new(),
            db_max_connections: 1,
            embedding_endpoint: String::new(),
            embedding_api_key: String::new(),
            embedding_model: String::new(),
            allowed_caller: "erhua".into(),
        },
        allowed_callers: ["erhua".into()].into(),
        erhua_trainer_user_ids: [
            "synthetic-resident".into(),
            "synthetic-unmanaged-trainer".into(),
        ]
        .into(),
    };
    let request = json!({"caller_profile":"erhua","platform":"qiwe","chat_id":chat,"source_conversation_type":"direct","trainer_user_id":"synthetic-unmanaged-trainer","target_channel_user_id":channel,"training_type":"member_preference","training_text":"每次回答前先读一遍旧训练约定","purpose":"synthetic legacy boundary regression","source_platform_message_id":marker});
    let denied = crate::context_tools::call_tool(
        &store.pool,
        &config,
        "qintopia_erhua_training_note_submit",
        request.clone(),
    )
    .await?;
    assert_eq!(denied["reason"], "foundation_shared_authority_required");
    let notes:i64=sqlx::query_scalar("SELECT count(*) FROM qintopia_identity.erhua_training_notes WHERE source_platform_message_id=$1").bind(&marker).fetch_one(&store.pool).await?;
    let facts: i64 = sqlx::query_scalar(
        "SELECT count(*) FROM qintopia_identity.member_facts WHERE person_id=$1",
    )
    .bind(person)
    .fetch_one(&store.pool)
    .await?;
    assert_eq!((notes, facts), (0, 0));
    // Removing rights and bindings does not reopen the old allowlist writer.
    sqlx::query("UPDATE qintopia_identity.person_identity_gateways SET active=false,version=version+1 WHERE tenant_key=$1").bind(&store.tenant).execute(&store.pool).await?;
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET status='revoked',version=version+1 WHERE person_id=$1").bind(person).execute(&store.pool).await?;
    assert_eq!(
        crate::context_tools::call_tool(
            &store.pool,
            &config,
            "qintopia_erhua_training_note_submit",
            request.clone()
        )
        .await?["reason"],
        "foundation_shared_authority_required"
    );
    // An unrelated unmanaged legacy scope retains the existing capability.
    // Its global old persona overlay must not flow into the adopted context.
    let legacy=crate::context_tools::call_tool(&store.pool,&config,"qintopia_erhua_training_note_submit",json!({"caller_profile":"erhua","platform":"qiwe","chat_id":format!("unmanaged-{marker}"),"source_conversation_type":"direct","trainer_user_id":"synthetic-unmanaged-trainer","training_type":"persona_rule","training_text":"LEGACY PERSONA OVERLAY SHOULD STAY OUT","purpose":"synthetic unmanaged compatibility","source_platform_message_id":format!("unmanaged-{marker}")})).await?;
    assert_eq!(legacy["accepted"], true);
    sqlx::query("INSERT INTO qintopia_identity.member_profile_snapshots(person_id,profile_kind,profile_version,status,summary,generated_by,input_hash) VALUES($1,'reply_context','synthetic-legacy-training','active','LEGACY PERSON SNAPSHOT SHOULD STAY OUT','synthetic-old-worker',$2)")
        .bind(person).bind(&marker).execute(&store.pool).await?;
    let actual=crate::context_tools::call_tool(&store.pool,&config,"qintopia_answer_context_prepare",json!({"caller_profile":"erhua","platform":"qiwe","chat_id":chat,"sender_id":channel,"message_text":"查询当前规则","purpose":"reply"})).await?;
    assert!(!actual.to_string().contains("SHOULD STAY OUT"));
    assert_eq!(
        actual["training_guidance"]["rules"]["legacy_guidance_suppressed"],
        true
    );
    let overlay: Uuid = serde_json::from_value(legacy["applied"]["persona_overlay_id"].clone())?;
    sqlx::query("UPDATE qintopia_identity.erhua_persona_overlays SET status='revoked',revoked_at=clock_timestamp() WHERE id=$1").bind(overlay).execute(&store.pool).await?;
    // A managed group rejects legacy persona writes even for an unknown target.
    let group:String=sqlx::query_scalar("UPDATE qintopia_messages.conversations c SET platform='qiwe' FROM qintopia_agent_os.collaboration_scope_bindings b WHERE b.tenant_key=$1 AND c.id=b.conversation_id AND c.display_name LIKE '一栋%' RETURNING c.chat_id").bind(&store.tenant).fetch_one(&store.pool).await?;
    let outsider:Uuid=sqlx::query_scalar("INSERT INTO qintopia_identity.persons(display_name) VALUES('合成尚未关联住客') RETURNING id").fetch_one(&store.pool).await?;
    sqlx::query("INSERT INTO qintopia_identity.channel_identities(person_id,platform,channel_user_id,chat_id,display_name,metadata) VALUES($1,'qiwe','synthetic-outsider',$2,'合成尚未关联住客','{\"current_qiwe_room_member\":true}')")
        .bind(outsider).bind(&group).execute(&store.pool).await?;
    sqlx::query("INSERT INTO qintopia_identity.member_profile_snapshots(person_id,profile_kind,profile_version,status,summary,generated_by,input_hash) VALUES($1,'reply_context','synthetic-legacy-group','active','GROUP LEGACY SHOULD STAY OUT','synthetic-old-worker',$2)")
        .bind(outsider).bind(format!("group-{marker}")).execute(&store.pool).await?;
    let group_context=crate::context_tools::call_tool(&store.pool,&config,"qintopia_answer_context_prepare",json!({"caller_profile":"erhua","platform":"qiwe","chat_id":group,"sender_id":"synthetic-outsider","message_text":"我是谁","purpose":"reply"})).await?;
    assert!(!group_context
        .to_string()
        .contains("GROUP LEGACY SHOULD STAY OUT"));
    let group_request = json!({"caller_profile":"erhua","platform":"qiwe","chat_id":group,"source_conversation_type":"group","trainer_user_id":"synthetic-unmanaged-trainer","training_type":"persona_rule","training_text":"这栋使用新的旧训练规则","purpose":"synthetic scoped gate"});
    assert_eq!(
        crate::context_tools::call_tool(
            &store.pool,
            &config,
            "qintopia_erhua_training_note_submit",
            group_request.clone()
        )
        .await?["reason"],
        "foundation_shared_authority_required"
    );
    sqlx::query("UPDATE qintopia_agent_os.collaboration_scope_bindings SET revoked_at=clock_timestamp() WHERE tenant_key=$1").bind(&store.tenant).execute(&store.pool).await?;
    assert_eq!(
        crate::context_tools::call_tool(
            &store.pool,
            &config,
            "qintopia_erhua_training_note_submit",
            group_request
        )
        .await?["reason"],
        "foundation_shared_authority_required"
    );
    Ok(())
}
