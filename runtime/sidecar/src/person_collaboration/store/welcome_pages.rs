//! Bounded operations lists. Every page rechecks the current scope grant.
use super::{welcome_review::WelcomeSubject, Store};
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;
#[derive(Default, Deserialize)]
#[serde(default, deny_unknown_fields)]
pub(crate) struct Page {
    pub page: i64,
    pub page_size: Option<i64>,
    pub search: String,
    pub status: String,
    pub channel_page: i64,
    pub channel_search: String,
}
impl Page {
    pub(super) fn bounds(&self) -> Result<(i64, i64)> {
        let size = self.page_size.unwrap_or(20);
        ensure!(
            (1..=50).contains(&size)
                && (0..=10000).contains(&self.page)
                && (0..=10000).contains(&self.channel_page)
                && self.search.chars().count() <= 120
                && self.channel_search.chars().count() <= 120
                && matches!(
                    self.status.as_str(),
                    "" | "pending" | "history" | "confirmed" | "rejected" | "revoked"
                ),
            "invalid_page"
        );
        Ok((size, self.page * size))
    }
}
impl Store {
    pub(crate) async fn welcome_review_page(
        &self,
        subject: &WelcomeSubject,
        scope: Uuid,
        q: &Page,
    ) -> Result<Value> {
        let (size, offset) = q.bounds()?;
        let (mut tx, _, _) = self.begin().await?;
        let grant = self.welcome_authorize(&mut tx, subject, scope, &[]).await?;
        let effects: Vec<String> = sqlx::query_scalar(
            "SELECT effects FROM qintopia_agent_os.welcome_review_subject_grants WHERE id=$1",
        )
        .bind(grant)
        .fetch_one(&mut *tx)
        .await?;
        let rows=sqlx::query("SELECT i.*,(SELECT jsonb_build_object('name',h.hints->>'name','nickname',h.hints->>'nickname') FROM qintopia_agent_os.welcome_source_projections h JOIN qintopia_agent_os.business_property_bindings b ON b.id=h.binding_id AND b.tenant_key=h.tenant_key AND b.version=h.binding_version AND b.active WHERE h.tenant_key=i.tenant_key AND h.scope_id=i.scope_id AND h.application_id=i.application_id AND h.application_revision=(i.snapshot->>'application_revision')::bigint AND h.field_hash=i.snapshot->>'application_hash' AND h.expires_at>clock_timestamp() AND i.snapshot->'occupant_source'->>'status'='pending' AND i.snapshot->'relations'->'application_person'='null'::jsonb) AS new_person,CASE WHEN nullif(p.preferred_name,'') IS NULL OR p.preferred_name=p.display_name THEN p.display_name ELSE p.preferred_name||' · '||p.display_name END AS person_label,c.display_name AS group_label FROM qintopia_agent_os.welcome_review_items i JOIN qintopia_agent_os.welcome_review_settings s ON s.tenant_key=i.tenant_key AND s.scope_id=i.scope_id JOIN qintopia_messages.conversations c ON c.id=s.conversation_id LEFT JOIN qintopia_identity.persons p ON p.id=i.confirmed_person WHERE i.tenant_key=$1 AND i.scope_id=$2 AND ($3='' OR ($3='pending' AND i.status IN ('pending','identity_confirmed')) OR ($3='history' AND i.status IN ('confirmed','rejected','revoked')) OR i.status=$3) AND ($4='' OR strpos(lower(coalesce(p.display_name,'')||' '||coalesce(p.preferred_name,'')||' '||i.candidates::text),lower($4))>0) ORDER BY i.work_item_id LIMIT $5 OFFSET $6")
            .bind(&self.tenant).bind(scope).bind(&q.status).bind(&q.search).bind(size+1).bind(offset).fetch_all(&mut *tx).await?;
        let channels=sqlx::query("SELECT DISTINCT l.id,l.version,coalesce(nullif(ci.display_name,''),nullif(l.adapter_metadata->>'display_name',''),'待核对渠道账号') AS label,l.person_id FROM qintopia_identity.source_identity_links l JOIN qintopia_identity.person_identity_gateways g ON g.namespace=l.namespace AND g.subject_type=l.subject_type LEFT JOIN qintopia_identity.channel_identities ci ON ci.id=l.channel_identity_id WHERE g.tenant_key=$1 AND g.scope_id=$2 AND g.active AND g.account_kind<>'shared' AND l.status<>'revoked' AND l.subject_type IN ('qiwe_sender','wecom_external','wecom_internal') AND ($3='' OR strpos(lower(coalesce(ci.display_name,'')||' '||coalesce(l.adapter_metadata->>'display_name','')),lower($3))>0) ORDER BY label,l.id LIMIT $4 OFFSET $5")
            .bind(&self.tenant).bind(scope).bind(&q.channel_search).bind(size+1).bind(q.channel_page*size).fetch_all(&mut *tx).await?;
        Ok(
            json!({"allowed_effects":effects,"page":q.page,"page_size":size,"has_more":rows.len()>size as usize,"channel_page":q.channel_page,"channels_has_more":channels.len()>size as usize,"items":rows.iter().take(size as usize).map(|r|json!({"work_item":r.get::<Uuid,_>("work_item_id"),"version":r.get::<i64,_>("version"),"new_person":r.get::<Option<Value>,_>("new_person"),"status":r.get::<String,_>("status"),"snapshot":r.get::<Value,_>("snapshot"),"candidates":r.get::<Value,_>("candidates"),"person_label":r.get::<Option<String>,_>("person_label"),"group_label":r.get::<String,_>("group_label"),"identity_confirmed":r.get::<Option<Uuid>,_>("identity_receipt").is_some(),"content_confirmed":r.get::<Option<Uuid>,_>("content_receipt").is_some()})).collect::<Vec<_>>(),"channels":channels.iter().take(size as usize).map(|r|json!({"id":r.get::<Uuid,_>("id"),"version":r.get::<i64,_>("version"),"label":r.get::<String,_>("label"),"person":r.get::<Option<Uuid>,_>("person_id")})).collect::<Vec<_>>()}),
        )
    }
    pub(crate) async fn welcome_receipt_page(
        &self,
        subject: &WelcomeSubject,
        scope: Uuid,
        work: Uuid,
        q: &Page,
    ) -> Result<Value> {
        let (size, offset) = q.bounds()?;
        let (mut tx, _, _) = self.begin().await?;
        self.welcome_authorize(&mut tx, subject, scope, &[]).await?;
        let belongs:bool=sqlx::query_scalar("SELECT EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_review_items WHERE tenant_key=$1 AND scope_id=$2 AND work_item_id=$3)").bind(&self.tenant).bind(scope).bind(work).fetch_one(&mut *tx).await?;
        ensure!(belongs, "gateway_scope_mismatch");
        let rows=sqlx::query("SELECT r.effects,r.result,r.created_at,coalesce(w.label,p.display_name,'原确认主体') AS label FROM qintopia_agent_os.welcome_review_receipts r LEFT JOIN qintopia_identity.work_accounts w ON r.subject_kind='work_account' AND w.id=r.subject_id LEFT JOIN qintopia_identity.persons p ON r.subject_kind='person' AND p.id=r.subject_id WHERE r.tenant_key=$1 AND r.work_item_id=$2 ORDER BY r.created_at DESC,r.id DESC LIMIT $3 OFFSET $4").bind(&self.tenant).bind(work).bind(size+1).bind(offset).fetch_all(&mut *tx).await?;
        Ok(
            json!({"page":q.page,"has_more":rows.len()>size as usize,"items":rows.iter().take(size as usize).map(|r|{let e:Value=r.get("effects"); json!({"label":r.get::<String,_>("label"),"at":r.get::<chrono::DateTime<chrono::Utc>,_>("created_at"),"decision":e["decision"],"application_stay":e["application_stay"],"channel_person":e["channel_person"],"content":e["content"],"result":r.get::<Value,_>("result")})}).collect::<Vec<_>>()}),
        )
    }
    pub(crate) async fn welcome_source_page(
        &self,
        subject: &WelcomeSubject,
        scope: Uuid,
        q: &Page,
    ) -> Result<Value> {
        let (size, offset) = q.bounds()?;
        let (mut tx, _, _) = self.begin().await?;
        self.welcome_authorize(&mut tx, subject, scope, &["identity"])
            .await?;
        let rows=sqlx::query("SELECT a.id,p.hints FROM qintopia_agent_os.welcome_source_projections p JOIN qintopia_agent_os.welcome_applications a ON a.id=p.application_id AND a.revision=p.application_revision AND a.field_hash=p.field_hash JOIN qintopia_agent_os.business_property_bindings b ON b.id=p.binding_id AND b.tenant_key=p.tenant_key AND b.version=p.binding_version AND b.active WHERE p.tenant_key=$1 AND p.scope_id=$2 AND p.expires_at>clock_timestamp() AND a.valid AND a.consent_active AND ($3='' OR strpos(lower(coalesce(p.hints->>'name','')||' '||coalesce(p.hints->>'nickname','')),lower($3))>0) ORDER BY p.updated_at DESC,a.id LIMIT $4 OFFSET $5").bind(&self.tenant).bind(scope).bind(&q.search).bind(size+1).bind(offset).fetch_all(&mut *tx).await?;
        Ok(
            json!({"has_more":rows.len()>size as usize,"items":rows.iter().take(size as usize).map(|r|{let h:Value=r.get("hints");json!({"application":r.get::<Uuid,_>("id"),"label":format!("{} · {}",h["nickname"].as_str().unwrap_or(""),h["name"].as_str().unwrap_or("")),"arrival":h["arrival"]})}).collect::<Vec<_>>()}),
        )
    }
}
