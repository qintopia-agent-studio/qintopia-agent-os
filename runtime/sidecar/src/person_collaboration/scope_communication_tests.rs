use super::*;
use crate::person_collaboration::store::scope_communication::{
    CommunicationContactSelection, CommunicationSelection,
};

#[tokio::test]
#[ignore = "explicit isolated PostgreSQL required"]
async fn scope_communication_candidates_and_current_route_fail_closed() -> Result<()> {
    let f = Fixture::new().await?;
    let scope: Uuid = sqlx::query_scalar(
        "SELECT scope_id FROM qintopia_agent_os.business_property_bindings WHERE id=$1",
    )
    .bind(f.binding)
    .fetch_one(&f.store.pool)
    .await?;
    let person = f.store.verified_person(&f.actor).await?;
    let source_link: Uuid = sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND subject_type='wecom_internal' AND person_id=$2")
        .bind(&f.store.tenant).bind(person).fetch_one(&f.store.pool).await?;
    sqlx::query("UPDATE qintopia_identity.source_identity_links SET adapter_metadata=jsonb_build_object('first_observation_ref',$2::text) WHERE id=$1")
        .bind(source_link).bind(Uuid::new_v4()).execute(&f.store.pool).await?;
    let chat = format!("simulated-staff-group-{}", Uuid::new_v4());
    let conversation: Uuid = sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type,display_name) VALUES($1,'wecom',$2,'group','模拟工作群') RETURNING id")
        .bind(&f.store.tenant).bind(&chat).fetch_one(&f.store.pool).await?;
    let group_binding: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3) RETURNING id")
        .bind(&f.store.tenant).bind(scope).bind(conversation).fetch_one(&f.store.pool).await?;
    let other_conversation: Uuid = sqlx::query_scalar("INSERT INTO qintopia_messages.conversations(tenant_id,platform,chat_id,chat_type,display_name) VALUES($1,'wecom',$2,'group','另一模拟工作群') RETURNING id")
        .bind(&f.store.tenant).bind(format!("simulated-staff-group-{}",Uuid::new_v4())).fetch_one(&f.store.pool).await?;
    let other_binding: Uuid = sqlx::query_scalar("INSERT INTO qintopia_agent_os.collaboration_scope_bindings(tenant_key,scope_id,conversation_id) VALUES($1,$2,$3) RETURNING id")
        .bind(&f.store.tenant).bind(scope).bind(other_conversation).fetch_one(&f.store.pool).await?;
    let collaboration: Uuid = sqlx::query_scalar(
        "SELECT collaboration_id FROM qintopia_agent_os.collaboration_grants WHERE id=$1",
    )
    .bind(f.grant)
    .fetch_one(&f.store.pool)
    .await?;
    sqlx::query("INSERT INTO qintopia_agent_os.collaboration_audiences(tenant_key,collaboration_id,configuration) VALUES($1,$2,$3)")
        .bind(&f.store.tenant).bind(collaboration).bind(json!({"proactive":"autonomous","groups":[conversation,other_conversation]})).execute(&f.store.pool).await?;

    let groups = f
        .store
        .scope_communication_candidates(&f.actor, scope, "groups", "模拟工作", 50, None)
        .await?;
    assert_eq!(groups["groups"].as_array().map(Vec::len), Some(2));
    assert_eq!(groups["contacts"], json!([]));
    let first_page = f
        .store
        .scope_communication_candidates(&f.actor, scope, "groups", "", 1, None)
        .await?;
    assert_eq!(first_page["has_more"], true);
    let after = first_page["next"].as_str().unwrap();
    let second_page = f
        .store
        .scope_communication_candidates(&f.actor, scope, "groups", "", 1, Some(after))
        .await?;
    assert_eq!(second_page["has_more"], false);
    let page_ids = [
        first_page["groups"][0]["binding_id"].clone(),
        second_page["groups"][0]["binding_id"].clone(),
    ];
    assert!(page_ids.contains(&json!(group_binding)));
    assert!(page_ids.contains(&json!(other_binding)));
    assert!(f
        .store
        .scope_communication_candidates(&f.actor, Uuid::new_v4(), "groups", "", 1, Some(after))
        .await
        .is_err());
    assert!(f
        .store
        .scope_communication_candidates(&f.actor, scope, "groups", "模拟", 1, Some(after))
        .await
        .is_err());
    let contacts = f
        .store
        .scope_communication_candidates(&f.actor, scope, "contacts", "", 50, None)
        .await?;
    assert_eq!(
        contacts["contacts"][0]["channel_source_link_id"],
        json!(source_link)
    );
    let gateway = f.gateway.as_str();
    let searched = f
        .store
        .scope_communication_candidates(&f.actor, scope, "contacts", gateway, 50, None)
        .await?;
    assert_eq!(
        searched["contacts"][0]["channel_source_link_id"],
        json!(source_link)
    );
    let selection = CommunicationSelection {
        staff_group_binding_id: group_binding,
        contacts: vec![CommunicationContactSelection {
            subject_kind: "person".into(),
            subject_id: person,
            channel_source_link_id: source_link,
        }],
    };
    let mut tx = f.store.pool.begin().await?;
    let saved = f
        .store
        .validate_scope_communication_in(&mut tx, scope, &selection)
        .await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_scopes SET communication_config=$2,version=version+1 WHERE id=$1")
        .bind(scope).bind(saved).execute(&mut *tx).await?;
    tx.commit().await?;
    let current = f
        .store
        .scope_communication_current(&f.gateway, f.binding)
        .await?;
    assert_eq!(current["current"], true);
    assert_eq!(current["staff_group"]["chat_id"], json!(chat));

    sqlx::query(
        "UPDATE qintopia_messages.conversations SET display_name='更新后的模拟工作群' WHERE id=$1",
    )
    .bind(conversation)
    .execute(&f.store.pool)
    .await?;
    let renamed = f
        .store
        .scope_communication_current(&f.gateway, f.binding)
        .await?;
    assert_eq!(renamed["current"], true);
    assert_eq!(renamed["staff_group"]["label"], "更新后的模拟工作群");

    sqlx::query("UPDATE qintopia_agent_os.collaboration_scope_bindings SET revoked_at=clock_timestamp(),version=version+1 WHERE id=$1")
        .bind(group_binding).execute(&f.store.pool).await?;
    let stale = f
        .store
        .scope_communication_current(&f.gateway, f.binding)
        .await?;
    assert_eq!(stale["current"], false);
    assert!(stale["staff_group"].is_null());
    Ok(())
}
