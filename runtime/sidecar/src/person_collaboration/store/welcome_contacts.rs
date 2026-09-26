//! Private host read of per-occupant contacts. Phones never leave this comparison.
use super::Store;
use crate::person_collaboration::{digest, foundation_server::TrustedContext};
use anyhow::{ensure, Result};
use chrono::{DateTime, Duration, Utc};
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use sqlx::{Postgres, Row, Transaction};
use std::collections::{BTreeMap, BTreeSet};
use uuid::Uuid;
const KEY: &str = "welcome_contact_read_v1";

#[derive(Deserialize)]
#[serde(tag = "action", rename_all = "snake_case", deny_unknown_fields)]
pub(crate) enum Request {
    Open {
        work_item: Uuid,
        #[serde(default)]
        refresh: bool,
        presentation: Option<Uuid>,
    },
    Save {
        work_item: Uuid,
        read_token: Uuid,
        order: Order,
    },
    Failed {
        work_item: Uuid,
        read_token: Uuid,
        reason: Failure,
    },
    Status {
        work_item: Uuid,
    },
}
#[derive(Deserialize)]
#[serde(rename_all = "snake_case")]
pub(crate) enum Failure {
    ReadFailed,
    ReadUnavailable,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct Order {
    id: String,
    property_id: String,
    version: u64,
    occupants: Vec<Occupant>,
}
#[derive(Deserialize, Serialize)]
#[serde(deny_unknown_fields)]
struct Occupant {
    id: String,
    #[serde(deserialize_with = "required_phone")]
    phone: Option<String>,
}
fn required_phone<'de, D: serde::Deserializer<'de>>(
    d: D,
) -> std::result::Result<Option<String>, D::Error> {
    Option::<String>::deserialize(d)
}
#[derive(Clone, Deserialize, Serialize)]
struct Read {
    token: Uuid,
    order: String,
    property: String,
    revision: String,
    occupants: Vec<String>,
    cases: BTreeMap<String, Uuid>,
    status: String,
    hash: Option<String>,
    comparisons: BTreeMap<Uuid, String>,
}
#[derive(Deserialize, Serialize)]
struct State {
    basis: String,
    salt: Uuid,
    generation: Uuid,
    expires: DateTime<Utc>,
    message: Option<Uuid>,
    presentation: Option<Uuid>,
    reads: Vec<Read>,
}
struct Context {
    work: Uuid,
    application: Uuid,
    basis: String,
    phone: Option<String>,
    reads: Vec<Read>,
    state: Option<State>,
}
fn phone(raw: Option<&str>) -> std::result::Result<String, &'static str> {
    let Some(raw) = raw.filter(|s| !s.trim().is_empty()) else {
        return Err("missing");
    };
    let s: String = raw
        .chars()
        .filter(|c| !c.is_whitespace() && !matches!(c, '(' | ')' | '-' | '（' | '）'))
        .collect();
    let s = s
        .strip_prefix("+86")
        .or_else(|| s.strip_prefix("0086"))
        .unwrap_or(&s);
    if s.len() != 11
        || !s.bytes().all(|b| b.is_ascii_digit())
        || !s.starts_with('1')
        || !(b'3'..=b'9').contains(&s.as_bytes()[1])
    {
        return Err("unusable");
    }
    Ok(s.into())
}
fn comparison(a: Option<&str>, b: Option<&str>) -> String {
    match (phone(a), phone(b)) {
        (Err("missing"), _) => "application_missing",
        (Err(_), _) => "application_unusable",
        (_, Err("missing")) => "missing",
        (_, Err(_)) => "unusable",
        (Ok(a), Ok(b)) if a == b => "match",
        _ => "different",
    }
    .into()
}
fn state_status(c: &Context, now: DateTime<Utc>) -> &'static str {
    let Some(s) = &c.state else {
        return "pending";
    };
    if s.basis != c.basis || s.expires <= now {
        return "stale";
    }
    if s.reads.iter().any(|r| r.status == "awaiting_source_sync") {
        return "awaiting_source_sync";
    }
    if s.reads.iter().any(|r| r.status == "failed") {
        return "incomplete";
    }
    if s.reads.iter().all(|r| r.status == "complete") {
        "complete"
    } else {
        "pending"
    }
}
fn summary(c: &Context, now: DateTime<Utc>, local_only: bool) -> Value {
    let status = state_status(c, now);
    let reads = c.state.as_ref().map_or(&c.reads, |s| &s.reads);
    json!({"work_item":c.work,"application":c.application,"status":status,"pool_count":c.reads.iter().map(|r|r.cases.len()).sum::<usize>(),"orders_total":c.reads.len(),"orders_done":reads.iter().filter(|r|r.status=="complete").count(),"failed_count":reads.iter().filter(|r|r.status=="failed").count(),"scan_complete":status=="complete","local_only":local_only})
}
fn evidence(c: &Context, now: DateTime<Utc>) -> Result<Value> {
    if state_status(c, now) != "complete" {
        return Ok(Value::Null);
    }
    let s = c.state.as_ref().unwrap();
    let comparisons: BTreeMap<Uuid, String> =
        s.reads.iter().flat_map(|r| r.comparisons.clone()).collect();
    let stable = json!({"basis":s.basis,"reads":s.reads.iter().map(|r|json!({"order":r.order,"hash":r.hash,"comparisons":r.comparisons})).collect::<Vec<_>>()});
    Ok(
        json!({"basis":digest(&serde_json::to_vec(&stable)?),"work_item":c.work,"comparisons":comparisons,"ambiguous":comparisons.values().filter(|v|v.as_str()=="match").count()>1}),
    )
}
impl Store {
    async fn contact_context(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: Uuid,
        application: Uuid,
    ) -> Result<Context> {
        let source = self
            .welcome_candidate_source(tx, scope, application)
            .await?;
        let anchor=sqlx::query("SELECT i.anan_work_id,i.binding_id,i.binding_version,a.revision,a.field_hash,i.identity_hash,w.metadata FROM qintopia_agent_os.application_intake_states i JOIN qintopia_agent_os.work_items w ON w.id=i.anan_work_id AND w.work_item_type='business_operation' AND w.capability_key='anan.pms' AND w.target_agent='anan' JOIN qintopia_agent_os.welcome_applications a ON a.id=i.application_id WHERE i.tenant_key=$1 AND i.application_id=$2 FOR UPDATE OF w")
            .bind(&self.tenant).bind(application).fetch_one(&mut **tx).await?;
        let rows = self
            .welcome_candidate_pool(
                tx,
                scope,
                &source.get::<String, _>("source_instance"),
                &source.get::<String, _>("property_id"),
            )
            .await?;
        let mut reads: BTreeMap<String, Read> = BTreeMap::new();
        let mut cases = Vec::new();
        for row in rows {
            let projection: Value = row.get("projection");
            let revision = projection["revision"]
                .as_str()
                .ok_or_else(|| anyhow::anyhow!("order_revision_required"))?
                .to_owned();
            ensure!(revision.parse::<u64>().is_ok(), "order_revision_required");
            let mut occupants: Vec<String> = projection["occupants"]
                .as_array()
                .ok_or_else(|| anyhow::anyhow!("active_occupants_required"))?
                .iter()
                .filter(|o| o["active"] == true)
                .map(|o| o["id"].as_str().unwrap_or("").to_owned())
                .collect();
            occupants.sort();
            ensure!(
                occupants.iter().all(|s| !s.is_empty())
                    && occupants.iter().collect::<BTreeSet<_>>().len() == occupants.len(),
                "active_occupants_required"
            );
            let order: String = row.get("order_id");
            let case: Uuid = row.get("case_id");
            let read = reads.entry(order.clone()).or_insert(Read {
                token: Uuid::new_v4(),
                order,
                property: source.get("property_id"),
                revision,
                occupants,
                cases: BTreeMap::new(),
                status: "pending".into(),
                hash: None,
                comparisons: BTreeMap::new(),
            });
            read.cases.insert(row.get("occupant_id"), case);
            cases.push(json!({"case":case,"version":row.get::<i64,_>("version"),"projection":projection,"person":row.get::<Option<Uuid>,_>("person"),"display":row.get::<Option<String>,_>("display_name"),"preferred":row.get::<Option<String>,_>("preferred_name"),"aliases":row.get::<Vec<String>,_>("aliases")}));
        }
        let basis = digest(&serde_json::to_vec(
            &json!({"application":application,"revision":anchor.get::<i64,_>("revision"),"field_hash":anchor.get::<String,_>("field_hash"),"identity_hash":anchor.get::<String,_>("identity_hash"),"binding":anchor.get::<Uuid,_>("binding_id"),"binding_version":anchor.get::<i64,_>("binding_version"),"cases":cases}),
        )?);
        let metadata: Value = anchor.get("metadata");
        let state = metadata
            .get(KEY)
            .map(|v| serde_json::from_value(v.clone()))
            .transpose()?;
        let hints: Value = source.get("hints");
        Ok(Context {
            work: anchor.get("anan_work_id"),
            application,
            basis,
            phone: hints["phone"].as_str().map(str::to_owned),
            reads: reads.into_values().collect(),
            state,
        })
    }
    pub(super) async fn welcome_contacts_for_application(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: Uuid,
        application: Uuid,
    ) -> Result<Value> {
        let available:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.application_intake_states i JOIN qintopia_agent_os.work_items w ON w.id=i.anan_work_id WHERE i.tenant_key=$1 AND i.application_id=$2 AND w.metadata ? $3)").bind(&self.tenant).bind(application).bind(KEY).fetch_one(&mut **tx).await?;
        if !available {
            return Ok(json!({"status":"not_read","scan_complete":false,"evidence":null}));
        }
        let c = self.contact_context(tx, scope, application).await?;
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut **tx)
            .await?;
        let mut result = summary(&c, now, !self.is_live());
        result["evidence"] = evidence(&c, now)?;
        Ok(result)
    }
    pub(in crate::person_collaboration) async fn welcome_stay_contacts(
        &self,
        gateway: &str,
        binding: Uuid,
        alias: &str,
        t: &TrustedContext,
        r: Request,
    ) -> Result<Value> {
        ensure!(t.gateway_id == gateway, "gateway_scope_mismatch");
        let work = match &r {
            Request::Open { work_item, .. }
            | Request::Save { work_item, .. }
            | Request::Failed { work_item, .. }
            | Request::Status { work_item } => *work_item,
        };
        let row=sqlx::query("SELECT i.application_id,b.scope_id FROM qintopia_agent_os.application_intake_states i JOIN qintopia_agent_os.business_property_bindings b ON b.id=i.binding_id AND b.tenant_key=i.tenant_key AND b.version=i.binding_version AND b.active JOIN qintopia_identity.person_identity_gateways g ON g.tenant_key=b.tenant_key AND g.scope_id=b.scope_id AND g.gateway_key=$2 AND g.active WHERE i.tenant_key=$1 AND i.binding_id=$3 AND i.resource_alias=$4 AND i.anan_work_id=$5")
            .bind(&self.tenant).bind(gateway).bind(binding).bind(alias).bind(work).fetch_one(&self.pool).await?;
        let scope: Uuid = row.get("scope_id");
        let app: Uuid = row.get("application_id");
        // Authenticate the exact persisted human message before taking the tenant lock.
        let callback = if let Request::Open {
            refresh: true,
            presentation: Some(id),
            ..
        } = &r
        {
            let context = self
                .welcome_confirmation_context(gateway, t.clone(), scope)
                .await?;
            ensure!(
                context["requires_contacts"] == true
                    && context["presentation"] == json!(id)
                    && context["work_item"] == json!(work),
                "contact_confirmation_context_mismatch"
            );
            let message:Uuid=sqlx::query_scalar("SELECT id FROM qintopia_messages.messages WHERE tenant_id=$1 AND platform=$2 AND chat_id=$3 AND sender_id=$4 AND message_id=$5").bind(&self.tenant).bind(&t.platform).bind(&t.chat_id).bind(&t.sender_id).bind(&t.message_id).fetch_one(&self.pool).await?;
            Some((message, *id))
        } else {
            None
        };
        let (mut tx, _, _) = self.begin().await?;
        let mut c = self.contact_context(&mut tx, scope, app).await?;
        ensure!(c.work == work, "application_source_mismatch");
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut *tx)
            .await?;
        if matches!(r, Request::Save { .. } | Request::Failed { .. }) {
            if let Some(message) = c.state.as_ref().and_then(|s| s.message) {
                let same:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_messages.messages WHERE id=$1 AND tenant_id=$2 AND platform=$3 AND chat_id=$4 AND sender_id=$5 AND message_id=$6)").bind(message).bind(&self.tenant).bind(&t.platform).bind(&t.chat_id).bind(&t.sender_id).bind(&t.message_id).fetch_one(&mut *tx).await?;
                ensure!(same, "contact_confirmation_context_mismatch");
            }
        }
        match r {
            Request::Open {
                refresh,
                presentation,
                ..
            } => {
                ensure!(
                    presentation.is_none() || callback.is_some(),
                    "contact_confirmation_context_mismatch"
                );
                if refresh || state_status(&c, now) == "stale" || c.state.is_none() {
                    let salt = c.state.as_ref().map_or_else(Uuid::new_v4, |s| s.salt);
                    c.state = Some(State {
                        basis: c.basis.clone(),
                        salt,
                        generation: Uuid::new_v4(),
                        expires: now + Duration::minutes(5),
                        message: callback.map(|c| c.0),
                        presentation: callback.map(|c| c.1),
                        reads: c.reads.clone(),
                    });
                    self.save_contacts(&mut tx, &c).await?;
                }
                let mut result = summary(&c, now, !self.is_live());
                result["reads"]=json!(c.state.as_ref().unwrap().reads.iter().filter(|r|r.status=="pending").map(|r|json!({"read_token":r.token,"order_id":r.order,"property_id":r.property,"order_revision":r.revision})).collect::<Vec<_>>());
                tx.commit().await?;
                Ok(result)
            }
            Request::Save {
                read_token,
                mut order,
                ..
            } => {
                ensure!(state_status(&c, now) != "stale", "contact_read_stale");
                ensure!(
                    order.occupants.len() <= 200
                        && order.id.len() <= 200
                        && order.property_id.len() <= 200
                        && order.occupants.iter().all(|o| !o.id.is_empty()
                            && o.id.len() <= 200
                            && o.phone
                                .as_ref()
                                .is_none_or(|p| p.chars().count() <= 40 && !p.contains('\0'))),
                    "invalid_contact_read"
                );
                order.occupants.sort_by(|a, b| a.id.cmp(&b.id));
                ensure!(
                    order.occupants.windows(2).all(|p| p[0].id != p[1].id),
                    "invalid_contact_read"
                );
                let s = c
                    .state
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("contact_read_required"))?;
                // Private per-work salt avoids a plain, unsalted phone digest in metadata.
                let hash = digest(&serde_json::to_vec(&json!([s.salt, order]))?);
                let read = s
                    .reads
                    .iter_mut()
                    .find(|r| r.token == read_token)
                    .ok_or_else(|| anyhow::anyhow!("contact_read_token_stale"))?;
                ensure!(
                    order.id == read.order && order.property_id == read.property,
                    "contact_order_mismatch"
                );
                let replayed = read.hash.is_some();
                if let Some(old) = &read.hash {
                    ensure!(old == &hash, "contact_read_conflict");
                } else {
                    ensure!(read.status == "pending", "contact_read_conflict");
                    read.hash = Some(hash);
                    if order.version.to_string() != read.revision
                        || order
                            .occupants
                            .iter()
                            .map(|o| o.id.clone())
                            .collect::<Vec<_>>()
                            != read.occupants
                    {
                        read.status = "awaiting_source_sync".into();
                    } else {
                        read.status = "complete".into();
                        for o in order.occupants {
                            if let Some(case) = read.cases.get(&o.id) {
                                read.comparisons.insert(
                                    *case,
                                    comparison(c.phone.as_deref(), o.phone.as_deref()),
                                );
                            }
                        }
                    }
                }
                let read_status = read.status.clone();
                if !replayed {
                    self.save_contacts(&mut tx, &c).await?;
                }
                let mut result = summary(&c, now, !self.is_live());
                result["stored"] = json!(true);
                result["replayed"] = json!(replayed);
                result["read_status"] = json!(read_status);
                tx.commit().await?;
                Ok(result)
            }
            Request::Failed {
                read_token, reason, ..
            } => {
                let _ = reason; // Deliberately never retain upstream exception text.
                ensure!(state_status(&c, now) != "stale", "contact_read_stale");
                let s = c
                    .state
                    .as_mut()
                    .ok_or_else(|| anyhow::anyhow!("contact_read_required"))?;
                let read = s
                    .reads
                    .iter_mut()
                    .find(|r| r.token == read_token)
                    .ok_or_else(|| anyhow::anyhow!("contact_read_token_stale"))?;
                ensure!(
                    matches!(read.status.as_str(), "pending" | "failed"),
                    "contact_read_conflict"
                );
                read.status = "failed".into();
                self.save_contacts(&mut tx, &c).await?;
                let mut result = summary(&c, now, !self.is_live());
                result["stored"] = json!(true);
                result["read_status"] = json!("failed");
                tx.commit().await?;
                Ok(result)
            }
            Request::Status { .. } => Ok(summary(&c, now, !self.is_live())),
        }
    }
    async fn save_contacts(&self, tx: &mut Transaction<'_, Postgres>, c: &Context) -> Result<()> {
        sqlx::query("UPDATE qintopia_agent_os.work_items SET metadata=jsonb_set(metadata,ARRAY[$2],$3) WHERE id=$1").bind(c.work).bind(KEY).bind(serde_json::to_value(&c.state)?).execute(&mut **tx).await?;
        Ok(())
    }
    pub(super) async fn verify_contact_confirmation(
        &self,
        tx: &mut Transaction<'_, Postgres>,
        scope: Uuid,
        application: Uuid,
        presentation: Uuid,
        message: Uuid,
        saved: &Value,
    ) -> Result<()> {
        let c = self.contact_context(tx, scope, application).await?;
        let now: DateTime<Utc> = sqlx::query_scalar("SELECT clock_timestamp()")
            .fetch_one(&mut **tx)
            .await?;
        let s = c
            .state
            .as_ref()
            .ok_or_else(|| anyhow::anyhow!("contact_confirmation_refresh_required"))?;
        ensure!(
            s.message == Some(message)
                && s.presentation == Some(presentation)
                && state_status(&c, now) == "complete",
            "contact_confirmation_refresh_required"
        );
        ensure!(
            &evidence(&c, now)? == saved,
            "contact_confirmation_basis_changed"
        );
        Ok(())
    }
}
