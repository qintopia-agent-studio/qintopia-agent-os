//! Non-default synthetic integration harness, using the actual Store and GET parser.
use super::{client::Client, recovery::ReadSource, store::Store};
use anyhow::{ensure, Result};
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Debug, Clone, clap::ValueEnum)]
pub enum Step {
    Init,
    Pull,
    Consume,
    Scan,
    Rebuild,
    Refresh,
    Recover,
    Status,
}

fn env(name: &str) -> Result<String> {
    std::env::var(name).map_err(|_| anyhow::anyhow!("explicit synthetic configuration required"))
}

pub async fn run(step: Step) -> Result<()> {
    // Never print downstream errors, URLs, cursors, payloads or credentials.
    match execute(step).await {
        Ok(value) => {
            println!("{}", serde_json::to_string(&value)?);
            Ok(())
        }
        Err(_) => anyhow::bail!("synthetic_step_failed_check_configuration_or_persisted_state"),
    }
}

async fn execute(step: Step) -> Result<Value> {
    ensure!(
        env("QINTOPIA_WELCOME_SYNTHETIC_ENABLE")? == "1",
        "synthetic_disabled"
    );
    let source = env("QINTOPIA_WELCOME_LOCAL_SOURCE")?;
    let property = env("QINTOPIA_WELCOME_LOCAL_PROPERTY")?;
    ensure!(
        source.starts_with("synthetic-")
            && super::protocol::reference(&source)
            && super::protocol::reference(&property),
        "synthetic_scope_required"
    );
    let store = Store::local(&env("QINTOPIA_WELCOME_LOCAL_DATABASE_URL")?).await?;
    if matches!(step, Step::Init) {
        crate::db::run_migrations(&store.pool).await?;
        return initialize(&store, &source, &property).await;
    }
    let mode:String=sqlx::query_scalar("SELECT mode FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2")
        .bind(&source).bind(&property).fetch_one(&store.pool).await?;
    ensure!(mode == "synthetic", "synthetic_source_required");
    if matches!(step, Step::Status) {
        return status(&store, &source, &property).await;
    }
    if matches!(step, Step::Recover) {
        store.recover_sending(&source, &property).await?;
        store.reconcile_scope(&source, &property).await?;
        return status(&store, &source, &property).await;
    }
    let reader: Arc<dyn ReadSource> = Arc::new(Client::synthetic(
        &env("QINTOPIA_WELCOME_SYNTHETIC_READ_ROOT")?,
        env("QINTOPIA_WELCOME_SYNTHETIC_READ_TOKEN")?,
        source.clone(),
        property.clone(),
        true,
    )?);
    let result = match step {
        Step::Pull => json!({"has_more":store.pull_once(&source,&property,reader).await?}),
        Step::Consume => json!({"consumed":store.consume_one(&source,&property,reader).await?}),
        Step::Scan => json!({"complete":store.scan_once(&source,&property,reader).await?}),
        Step::Refresh => json!({"refreshed":store.refresh_orders(&source,&property,reader).await?}),
        Step::Rebuild => {
            let page = tokio::task::spawn_blocking(move || reader.events(None)).await??;
            page.verified(&source, &property)?;
            store
                .start_rebuild(&source, &property, &page.head_cursor)
                .await?;
            json!({"rebuild_started":true})
        }
        _ => unreachable!(),
    };
    Ok(json!({"result":result,"state":status(&store,&source,&property).await?}))
}

async fn initialize(store: &Store, source: &str, property: &str) -> Result<Value> {
    let mut tx = store.pool.begin().await?;
    // Re-running init never re-enables a rebuilt/quarantined source or overwrites identities.
    let inserted=sqlx::query("INSERT INTO qintopia_agent_os.welcome_sources(source_instance,property_id,mode,rebuilding,enabled,admission_after,execution_epoch) VALUES($1,$2,'synthetic',false,true,now(),1) ON CONFLICT DO NOTHING")
        .bind(source).bind(property).execute(&mut *tx).await?.rows_affected();
    ensure!(inserted == 1, "source_already_registered");
    let person = Uuid::new_v4();
    let identity = Uuid::new_v4();
    sqlx::query(
        "INSERT INTO qintopia_identity.persons(id,display_name) VALUES($1,'合成联调审核员')",
    )
    .bind(person)
    .execute(&mut *tx)
    .await?;
    let namespace = format!("{source}/{}", super::digest(property.as_bytes()));
    sqlx::query("INSERT INTO qintopia_identity.source_identity_links(id,namespace,subject_type,source_ref,person_id,status,evidence_ref,confirmed_by) VALUES($1,$2,'feishu_open','synthetic-operator',$3,'confirmed',$4,$3)")
        .bind(identity).bind(&namespace).bind(person).bind(Uuid::new_v4()).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_identity_scopes(namespace,source_instance,property_id) VALUES($1,$2,$3)")
        .bind(namespace).bind(source).bind(property).execute(&mut *tx).await?;
    sqlx::query("INSERT INTO qintopia_agent_os.welcome_grants(person_id,source_instance,property_id,action,expires_at,appointed_by) VALUES($1,$2,$3,'identity',now()+interval '1 day',$1)")
        .bind(person).bind(source).bind(property).execute(&mut *tx).await?;
    tx.commit().await?;
    Ok(json!({"initialized":true,"synthetic_operator_link":identity,"external_effects":false}))
}

async fn status(store: &Store, source: &str, property: &str) -> Result<Value> {
    let row=sqlx::query("SELECT rebuilding,enabled,cursor IS NOT NULL AS has_cursor FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2")
        .bind(source).bind(property).fetch_one(&store.pool).await?;
    let counts=sqlx::query("SELECT status,count(*) AS count FROM qintopia_agent_os.welcome_inbox WHERE source_instance=$1 AND property_id=$2 GROUP BY status ORDER BY status")
        .bind(source).bind(property).fetch_all(&store.pool).await?;
    let counts: Vec<Value> = counts
        .iter()
        .map(|r| json!({"status":r.get::<String,_>("status"),"count":r.get::<i64,_>("count")}))
        .collect();
    Ok(
        json!({"rebuilding":row.get::<bool,_>("rebuilding"),"admission_enabled":row.get::<bool,_>("enabled"),"has_cursor":row.get::<bool,_>("has_cursor"),"inbox":counts,"external_effects":false}),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn loopback_transport_does_not_weaken_production_constructor() {
        for root in [
            "http://localhost:18443/",
            "http://example.com/",
            "http://127.0.0.1/path",
            "http://user@127.0.0.1/",
            "http://127.0.0.1/?x=1",
        ] {
            assert!(Client::synthetic(
                root,
                "fixture".into(),
                "synthetic-test".into(),
                "fixture-property".into(),
                true
            )
            .is_err());
        }
        assert!(Client::synthetic(
            "http://127.0.0.1:18443/",
            "fixture".into(),
            "synthetic-test".into(),
            "fixture-property".into(),
            false
        )
        .is_err());
        assert!(Client::new(
            "http://127.0.0.1:18443/",
            "fixture".into(),
            "synthetic-test".into(),
            "fixture-property".into()
        )
        .is_err());
        assert!(Client::synthetic(
            "http://127.0.0.1:18443/",
            "fixture".into(),
            "synthetic-test".into(),
            "fixture-property".into(),
            true
        )
        .is_ok());
    }
}
