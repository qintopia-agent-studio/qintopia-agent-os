use super::*;
use sqlx::Row;

async fn steward(f: &Fixture) -> Result<Actor> {
    let link:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_identity.source_identity_links WHERE namespace=$1 AND person_id=$2 AND status='confirmed'").bind(&f.tenant).bind(f.stewards[0]).fetch_one(&f.welcome.pool).await?;
    f.people.actor(link).await
}
async fn prepare_resident(f: &Fixture) -> Result<Uuid> {
    let rows=sqlx::query("SELECT projection FROM qintopia_agent_os.welcome_source_versions WHERE source_instance=$1 AND aggregate_type='order'").bind(f.seed["source"].as_str().unwrap()).fetch_all(&f.welcome.pool).await?;
    for r in rows {
        let mut s: super::super::state::Snapshot = serde_json::from_value(r.get("projection"))?;
        s.observed_at = Utc::now();
        f.welcome.apply_snapshot(None, &s).await?;
    }
    Ok(id(&f.seed["targets"][0]["person_ref"]))
}
fn command(f: &Fixture, person: Uuid) -> Value {
    json!({"operation_id":Uuid::new_v4(),"scope":scope(f,0),"expected_id":null,"delegate":person,"valid_from":null,"valid_until":Utc::now()+Duration::hours(2)})
}

#[tokio::test]
#[ignore = "explicit isolated database required"]
async fn steward_delegation_routes_exact_review_and_revokes_old_approval() -> Result<()> {
    let f = fixture().await?;
    let actor = steward(&f).await?;
    let person = prepare_resident(&f).await?;
    setting(&f, 0, "review", &["text", "image"], None, 0).await?;
    let cmd = command(&f, person);
    let saved = f
        .people
        .review_delegation_change(&actor, &serde_json::from_value(cmd.clone())?)
        .await?;
    assert_eq!(
        f.people
            .review_delegation_change(&actor, &serde_json::from_value(cmd)?)
            .await?["replayed"],
        true
    );
    f.people.account_command(&f.owner,&serde_json::from_value(json!({"kind":"create","person":person,"username":"resident-proxy","password":"synthetic-proxy-check-only"}))?).await?;
    let token = f
        .people
        .login(&serde_json::from_value(
            json!({"username":"resident-proxy","password":"synthetic-proxy-check-only"}),
        )?)
        .await?;
    let proxy = f.people.session_actor(&token).await?;
    let workspace = f.people.state(&proxy).await?;
    assert_eq!(workspace["management_available"], false);
    assert!(workspace["relations"].as_array().unwrap().is_empty());
    assert_eq!(workspace["delegated_reviews"].as_array().unwrap().len(), 1);
    f.people.welcome_assert_scope(&proxy, scope(&f, 0)).await?;
    assert!(f
        .people
        .foundation_assert_scope(&proxy, scope(&f, 0))
        .await
        .is_err());
    render(&f, 0).await?;
    let pending = prepare(&f, 0).await?;
    assert!(pending["waiting"]
        .as_array()
        .unwrap()
        .iter()
        .all(|w| w["reviewer"] == json!(person)));
    let docs = artifacts(&f, 0).await?;
    for artifact in &docs {
        assert!(approve(&f, 0, artifact, f.stewards[0]).await.is_err());
        approve(&f, 0, artifact, person).await?;
    }
    let ready = prepare(&f, 0).await?;
    assert_eq!(ready["actions"].as_array().unwrap().len(), 2);
    let state = f.welcome.foundation_state(&f.tenant, person).await?;
    assert!(state["targets"]
        .as_array()
        .unwrap()
        .iter()
        .any(|t| t["scope_ref"] == json!(scope(&f, 0))));
    f.people.review_delegation_change(&actor,&serde_json::from_value(json!({"operation_id":Uuid::new_v4(),"scope":scope(&f,0),"expected_id":saved["id"],"delegate":null,"valid_from":null,"valid_until":null}))?).await?;
    assert!(f.people.state(&proxy).await?["delegated_reviews"]
        .as_array()
        .unwrap()
        .is_empty());
    assert!(f
        .people
        .welcome_assert_scope(&proxy, scope(&f, 0))
        .await
        .is_err());
    for action in ready["actions"].as_array().unwrap() {
        let result = f
            .welcome
            .foundation_execute(id(action), SyntheticOutcome::Success)
            .await;
        assert!(result.is_err() || result.as_ref().unwrap()["status"] != "succeeded");
    }
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database required"]
async fn steward_delegation_rejects_nonresident_expiry_and_stale_source() -> Result<()> {
    let f = fixture().await?;
    let actor = steward(&f).await?;
    let person = prepare_resident(&f).await?;
    assert!(f
        .people
        .review_delegation_change(&actor, &serde_json::from_value(command(&f, f.stewards[1]))?)
        .await
        .is_err());
    let mut invalid = command(&f, person);
    invalid["valid_until"] = Value::Null;
    assert!(f
        .people
        .review_delegation_change(&actor, &serde_json::from_value(invalid)?)
        .await
        .is_err());
    let saved = f
        .people
        .review_delegation_change(&actor, &serde_json::from_value(command(&f, person))?)
        .await?;
    setting(&f, 0, "review", &["text", "image"], None, 0).await?;
    render(&f, 0).await?;
    sqlx::query("UPDATE qintopia_agent_os.welcome_source_versions SET projection=jsonb_set(projection,'{observed_at}',to_jsonb(clock_timestamp()-interval '2 minutes')) WHERE source_instance=$1").bind(f.seed["source"].as_str().unwrap()).execute(&f.welcome.pool).await?;
    let state = f
        .people
        .review_delegation_state(&actor, scope(&f, 0))
        .await?;
    assert_eq!(state["reason"], "delegate_not_current_resident");
    f.welcome.refresh_foundation_fixture(&f.tenant).await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_review_delegations SET valid_from=clock_timestamp()-interval '2 hours',valid_until=clock_timestamp()-interval '1 hour' WHERE id=$1").bind(id(&saved["id"])).execute(&f.welcome.pool).await?;
    assert_eq!(
        f.people
            .review_delegation_state(&actor, scope(&f, 0))
            .await?["reason"],
        "review_delegation_expired"
    );
    let pending = prepare(&f, 0).await?;
    assert!(pending["actions"].as_array().unwrap().is_empty());
    assert!(pending["waiting"]
        .as_array()
        .unwrap()
        .iter()
        .any(|w| w["reason"] == "review_delegation_expired"));
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database required"]
async fn steward_direct_setting_does_not_require_expired_delegate() -> Result<()> {
    let f = fixture().await?;
    let actor = steward(&f).await?;
    let person = prepare_resident(&f).await?;
    let saved = f
        .people
        .review_delegation_change(&actor, &serde_json::from_value(command(&f, person))?)
        .await?;
    sqlx::query("UPDATE qintopia_agent_os.collaboration_review_delegations SET valid_from=clock_timestamp()-interval '2 hours',valid_until=clock_timestamp()-interval '1 hour' WHERE id=$1").bind(id(&saved["id"])).execute(&f.welcome.pool).await?;
    setting(&f, 0, "direct", &["text", "image"], None, 0).await?;
    render(&f, 0).await?;
    assert_eq!(
        prepare(&f, 0).await?["actions"].as_array().unwrap().len(),
        2
    );
    // Existing welcome regression separately covers upper publish confirmation.
    Ok(())
}

#[tokio::test]
#[ignore = "explicit isolated database required"]
async fn steward_delegate_checkout_and_owner_revocation_remove_access() -> Result<()> {
    let f = fixture().await?;
    let actor = steward(&f).await?;
    let person = prepare_resident(&f).await?;
    f.people
        .review_delegation_change(&actor, &serde_json::from_value(command(&f, person))?)
        .await?;
    let projection:Value=sqlx::query_scalar("SELECT projection FROM qintopia_agent_os.welcome_source_versions WHERE source_instance=$1 AND aggregate_type='order' AND projection->>'building'='一栋'").bind(f.seed["source"].as_str().unwrap()).fetch_one(&f.welcome.pool).await?;
    let mut snapshot: super::super::state::Snapshot = serde_json::from_value(projection)?;
    snapshot.state = super::super::state::StayState::Terminated;
    snapshot.revision = (snapshot.revision.parse::<u64>()? + 1).to_string();
    snapshot.observed_at = Utc::now();
    f.welcome.apply_snapshot(None, &snapshot).await?;
    assert_eq!(
        f.people
            .review_delegation_state(&actor, scope(&f, 0))
            .await?["reason"],
        "delegate_not_current_resident"
    );
    assert!(
        f.welcome.foundation_state(&f.tenant, person).await?["targets"]
            .as_array()
            .unwrap()
            .is_empty()
    );
    sqlx::query("UPDATE qintopia_agent_os.collaboration_grants g SET status='revoked',revoked_at=clock_timestamp() FROM qintopia_agent_os.agent_collaborations c JOIN qintopia_agent_os.collaboration_appointments a ON a.id=c.appointment_id WHERE g.collaboration_id=c.id AND g.tenant_key=$1 AND a.person_id=$2 AND g.action_key='designate'").bind(&f.tenant).bind(f.stewards[0]).execute(&f.welcome.pool).await?;
    assert!(f
        .people
        .review_delegation_change(&actor, &serde_json::from_value(command(&f, person))?)
        .await
        .is_err());
    Ok(())
}
