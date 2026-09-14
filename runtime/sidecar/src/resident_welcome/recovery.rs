//! Durable baseline scanning and bounded, scope-specific read-only recovery.
use super::{
    client::{Client, FeedPage},
    projection,
    protocol::Envelope,
    store::{audit, complete_tx, fence_tx, Claim, Store},
};
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use std::sync::Arc;
use uuid::Uuid;

#[derive(Deserialize)]
pub struct ScanPage {
    pub schema_version: String,
    pub source_instance: String,
    pub property_id: String,
    pub orders: Vec<Value>,
    pub next_after_id: Option<String>,
    pub has_more: bool,
}
impl ScanPage {
    pub fn verified(
        &self,
        source: &str,
        property: &str,
        after: Option<&str>,
    ) -> Result<Vec<Value>> {
        ensure!(
            self.schema_version == "pms.projections.v1"
                && self.source_instance == source
                && self.property_id == property
                && self.orders.len() <= 100,
            "scan_scope_or_size"
        );
        let rows: Vec<Value> = self
            .orders
            .iter()
            .map(|v| projection::order(v, source, property, None))
            .collect::<Result<_>>()?;
        let mut previous = after;
        for row in &rows {
            let id = row["order_id"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("scan_id_required"))?;
            ensure!(previous.is_none_or(|p| id > p), "scan_order_or_position");
            previous = Some(id);
        }
        ensure!(
            previous == self.next_after_id.as_deref(),
            "scan_checkpoint_mismatch"
        );
        ensure!(
            !self.has_more || (!rows.is_empty() && previous != after),
            "scan_no_progress"
        );
        Ok(rows)
    }
}

/// Implementations return only the contract's minimal projections. Tests inject
/// fixtures; no test switches production URLs or credentials.
pub trait ReadSource: Send + Sync + 'static {
    fn events(&self, cursor: Option<&str>) -> Result<FeedPage>;
    fn scan(&self, after: Option<&str>) -> Result<ScanPage>;
    fn projection(&self, kind: &str, id: &str) -> Result<Value>;
}
impl ReadSource for Client {
    fn events(&self, cursor: Option<&str>) -> Result<FeedPage> {
        self.events(cursor)
    }
    fn scan(&self, after: Option<&str>) -> Result<ScanPage> {
        self.scan(after)
    }
    fn projection(&self, kind: &str, id: &str) -> Result<Value> {
        match kind {
            "order" => self.order(id),
            "member" => self.member(id),
            "inventory_unit" => self.inventory_unit(id),
            _ => anyhow::bail!("unknown_projection_kind"),
        }
    }
}

impl Store {
    pub async fn start_rebuild(&self, source: &str, property: &str, head: &str) -> Result<Uuid> {
        ensure!(
            !head.is_empty() && head.len() <= 4096,
            "invalid_rebuild_head"
        );
        let mut tx = self.pool.begin().await?;
        let mode:String = sqlx::query_scalar("SELECT mode FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2 FOR UPDATE")
            .bind(source).bind(property).fetch_one(&mut *tx).await?;
        ensure!(mode != "live", "live_rebuild_not_activated");
        let generation = Uuid::new_v4();
        sqlx::query("UPDATE qintopia_agent_os.welcome_sources SET rebuilding=true,enabled=false,rebuild_head=$3 WHERE source_instance=$1 AND property_id=$2")
            .bind(source).bind(property).bind(head).execute(&mut *tx).await?;
        sqlx::query("INSERT INTO qintopia_agent_os.welcome_rebuilds(source_instance,property_id,generation,head_cursor) VALUES ($1,$2,$3,$4) ON CONFLICT (source_instance,property_id) DO UPDATE SET generation=EXCLUDED.generation,head_cursor=EXCLUDED.head_cursor,after_id=NULL,scan_complete=false")
            .bind(source).bind(property).bind(generation).bind(head).execute(&mut *tx).await?;
        audit(
            &mut tx,
            None,
            "welcome_rebuild_started",
            None,
            json!({"generation":generation}),
        )
        .await?;
        tx.commit().await?;
        Ok(generation)
    }

    pub async fn commit_scan(
        &self,
        source: &str,
        property: &str,
        generation: Uuid,
        after: Option<&str>,
        page: &ScanPage,
    ) -> Result<()> {
        let rows = page.verified(source, property, after)?;
        let mut tx = self.pool.begin().await?;
        let checkpoint=sqlx::query("SELECT generation,after_id,scan_complete FROM qintopia_agent_os.welcome_rebuilds WHERE source_instance=$1 AND property_id=$2 FOR UPDATE")
            .bind(source).bind(property).fetch_one(&mut *tx).await?;
        ensure!(
            checkpoint.get::<Uuid, _>("generation") == generation
                && checkpoint.get::<Option<String>, _>("after_id").as_deref() == after
                && !checkpoint.get::<bool, _>("scan_complete"),
            "scan_checkpoint_conflict"
        );
        for p in rows {
            sqlx::query("INSERT INTO qintopia_agent_os.welcome_scan_items(source_instance,property_id,generation,order_id,projection) VALUES ($1,$2,$3,$4,$5)")
                .bind(source).bind(property).bind(generation).bind(p["order_id"].as_str().unwrap()).bind(&p).execute(&mut *tx).await?;
        }
        sqlx::query("UPDATE qintopia_agent_os.welcome_rebuilds SET after_id=$3,scan_complete=$4 WHERE source_instance=$1 AND property_id=$2")
            .bind(source).bind(property).bind(&page.next_after_id).bind(!page.has_more).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(())
    }

    /// One durable projection at a time. Crashing after apply but before marking
    /// complete safely reapplies the same vector. No false event is constructed.
    pub async fn drain_scan_one(&self, source: &str, property: &str) -> Result<bool> {
        let mut tx = self.pool.begin().await?;
        sqlx::query("SELECT 1 FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2 FOR SHARE")
            .bind(source).bind(property).fetch_one(&mut *tx).await?;
        let checkpoint=sqlx::query("SELECT generation FROM qintopia_agent_os.welcome_rebuilds WHERE source_instance=$1 AND property_id=$2 FOR SHARE")
            .bind(source).bind(property).fetch_one(&mut *tx).await?;
        let generation: Uuid = checkpoint.get("generation");
        let row=sqlx::query("SELECT order_id,projection FROM qintopia_agent_os.welcome_scan_items WHERE source_instance=$1 AND property_id=$2 AND generation=$3 AND NOT completed ORDER BY order_id FOR UPDATE SKIP LOCKED LIMIT 1")
            .bind(source).bind(property).bind(generation).fetch_optional(&mut *tx).await?;
        let Some(row) = row else {
            return Ok(false);
        };
        let p: Value = row.get("projection");
        let s = projection::normalize(&p)?;
        self.apply_snapshot(None, &s).await?;
        sqlx::query("UPDATE qintopia_agent_os.welcome_scan_items SET completed=true WHERE source_instance=$1 AND property_id=$2 AND generation=$3 AND order_id=$4")
            .bind(source).bind(property).bind(generation).bind(row.get::<String,_>("order_id")).execute(&mut *tx).await?;
        tx.commit().await?;
        Ok(true)
    }

    pub async fn finish_scan(&self, source: &str, property: &str, generation: Uuid) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        // Same lock order as start_rebuild.
        sqlx::query("SELECT 1 FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2 FOR UPDATE")
            .bind(source).bind(property).fetch_one(&mut *tx).await?;
        let row=sqlx::query("SELECT generation,head_cursor,scan_complete FROM qintopia_agent_os.welcome_rebuilds WHERE source_instance=$1 AND property_id=$2 FOR UPDATE")
            .bind(source).bind(property).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<Uuid, _>("generation") == generation && row.get::<bool, _>("scan_complete"),
            "scan_incomplete_or_stale"
        );
        let pending:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_scan_items WHERE source_instance=$1 AND property_id=$2 AND generation=$3 AND NOT completed)")
            .bind(source).bind(property).bind(generation).fetch_one(&mut *tx).await?;
        ensure!(!pending, "scan_items_pending");
        // CAS head is used only once; repeated finish must not rewind pull progress.
        sqlx::query("UPDATE qintopia_agent_os.welcome_sources SET cursor=$3,rebuild_head=NULL WHERE source_instance=$1 AND property_id=$2 AND rebuild_head=$3")
            .bind(source).bind(property).bind(row.get::<String,_>("head_cursor")).execute(&mut *tx).await?;
        audit(
            &mut tx,
            None,
            "welcome_scan_completed_publish_disabled",
            None,
            json!({"generation":generation}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    pub async fn retry_inbox(&self, claim: &Claim) -> Result<()> {
        let mut tx = self.pool.begin().await?;
        fence_tx(&mut tx, claim).await?;
        let attempts: i32 =
            sqlx::query_scalar("SELECT attempts FROM qintopia_agent_os.work_items WHERE id=$1")
                .bind(claim.work_item_id)
                .fetch_one(&mut *tx)
                .await?;
        if attempts >= 8 {
            complete_tx(&mut tx, claim, true).await?;
        } else {
            sqlx::query("UPDATE qintopia_agent_os.welcome_inbox SET status='queued' WHERE id=$1")
                .bind(claim.inbox_id)
                .execute(&mut *tx)
                .await?;
            sqlx::query("UPDATE qintopia_agent_os.work_items SET status='queued',available_at=now()+interval '5 seconds'*power(2,least(attempts,6)),claimed_by=NULL,locked_at=NULL,claim_expires_at=NULL WHERE id=$1")
                .bind(claim.work_item_id).execute(&mut *tx).await?;
        }
        audit(
            &mut tx,
            Some(claim.work_item_id),
            "welcome_readback_retry",
            None,
            json!({"attempts":attempts}),
        )
        .await?;
        tx.commit().await?;
        Ok(())
    }

    /// Bounded iteration suitable for a scheduler. No message/render/upload APIs.
    pub async fn consume_one(
        &self,
        source: &str,
        property: &str,
        reader: Arc<dyn ReadSource>,
    ) -> Result<bool> {
        let Some(claim) = self.claim(source, property, Uuid::new_v4()).await? else {
            return Ok(false);
        };
        let value: Value =
            sqlx::query_scalar("SELECT envelope FROM qintopia_agent_os.welcome_inbox WHERE id=$1")
                .bind(claim.inbox_id)
                .fetch_one(&self.pool)
                .await?;
        let e: Envelope = serde_json::from_value(value)?;
        let kind = e.aggregate_type.clone();
        let id = e.aggregate_id.clone();
        let result = tokio::task::spawn_blocking(move || reader.projection(&kind, &id)).await?;
        let result = match result {
            Ok(p) if e.aggregate_type == "order" => {
                match projection::order(&p, source, property, Some(&e.aggregate_id))
                    .and_then(|p| projection::normalize(&p))
                {
                    Ok(s) => self.consume_snapshot(&claim, &s).await.map(|_| ()),
                    Err(e) => Err(e),
                }
            }
            Ok(p) => self.consume_entity(&claim, &p).await,
            Err(e) => Err(e),
        };
        if result.is_err() {
            // A commit may have succeeded before convergence failed; inspect first.
            let active:bool=sqlx::query_scalar("SELECT status='processing' AND fence=$2 FROM qintopia_agent_os.welcome_inbox WHERE id=$1").bind(claim.inbox_id).bind(claim.fence).fetch_one(&self.pool).await?;
            if active {
                self.retry_inbox(&claim).await?;
            }
            return Err(anyhow::anyhow!("welcome_consumption_deferred"));
        }
        Ok(true)
    }

    pub async fn pull_once(
        &self,
        source: &str,
        property: &str,
        reader: Arc<dyn ReadSource>,
    ) -> Result<bool> {
        let cursor:Option<String>=sqlx::query_scalar("SELECT cursor FROM qintopia_agent_os.welcome_sources WHERE source_instance=$1 AND property_id=$2").bind(source).bind(property).fetch_one(&self.pool).await?;
        let c = cursor.clone();
        let page = tokio::task::spawn_blocking(move || reader.events(c.as_deref())).await?;
        super::client::commit_feed(self, source, property, cursor.as_deref(), page).await
    }

    /// Capture the feed head before the first page. Repeated ticks resume the
    /// durable generation; only explicit start_rebuild discards its position.
    pub async fn scan_once(
        &self,
        source: &str,
        property: &str,
        reader: Arc<dyn ReadSource>,
    ) -> Result<bool> {
        let checkpoint=sqlx::query("SELECT generation,after_id,scan_complete FROM qintopia_agent_os.welcome_rebuilds WHERE source_instance=$1 AND property_id=$2")
            .bind(source).bind(property).fetch_optional(&self.pool).await?;
        let (generation, after, complete) = if let Some(row) = checkpoint {
            (
                row.get::<Uuid, _>("generation"),
                row.get::<Option<String>, _>("after_id"),
                row.get::<bool, _>("scan_complete"),
            )
        } else {
            let adapter = Arc::clone(&reader);
            let page = tokio::task::spawn_blocking(move || adapter.events(None)).await??;
            page.verified(source, property)?;
            (
                self.start_rebuild(source, property, &page.head_cursor)
                    .await?,
                None,
                false,
            )
        };
        if !complete {
            let position = after.clone();
            let page =
                tokio::task::spawn_blocking(move || reader.scan(position.as_deref())).await??;
            self.commit_scan(source, property, generation, after.as_deref(), &page)
                .await?;
        }
        // Bounded work per scheduler tick, with the remaining items persisted.
        for _ in 0..100 {
            if !self.drain_scan_one(source, property).await? {
                break;
            }
        }
        let done:bool=sqlx::query_scalar("SELECT r.scan_complete AND NOT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_scan_items i WHERE i.source_instance=r.source_instance AND i.property_id=r.property_id AND i.generation=r.generation AND NOT i.completed) FROM qintopia_agent_os.welcome_rebuilds r WHERE r.source_instance=$1 AND r.property_id=$2")
            .bind(source).bind(property).fetch_one(&self.pool).await?;
        if done {
            self.finish_scan(source, property, generation).await?;
        }
        Ok(done)
    }

    /// Oldest observations first, so bounded cycles do not starve larger scopes.
    pub async fn refresh_orders(
        &self,
        source: &str,
        property: &str,
        reader: Arc<dyn ReadSource>,
    ) -> Result<usize> {
        let orders:Vec<String>=sqlx::query_scalar("SELECT aggregate_id FROM qintopia_agent_os.welcome_source_versions WHERE source_instance=$1 AND property_id=$2 AND aggregate_type='order' ORDER BY (projection->>'observed_at')::timestamptz ASC LIMIT 100")
            .bind(source).bind(property).fetch_all(&self.pool).await?;
        let mut count = 0;
        for id in orders {
            let adapter = Arc::clone(&reader);
            let order = id.clone();
            let raw =
                tokio::task::spawn_blocking(move || adapter.projection("order", &order)).await??;
            let p = projection::order(&raw, source, property, Some(&id))?;
            self.apply_snapshot(None, &projection::normalize(&p)?)
                .await?;
            count += 1;
        }
        self.reconcile_scope(source, property).await?;
        Ok(count)
    }
}
