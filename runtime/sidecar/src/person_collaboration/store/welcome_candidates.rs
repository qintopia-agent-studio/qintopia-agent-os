//! Scope-bound source hints and suggestions. No match is an identity confirmation.
use super::{welcome_review::WelcomeSubject, Store};
use crate::person_collaboration::{digest, welcome_model::ReviewOpen};
use anyhow::{ensure, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::Row;
use uuid::Uuid;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
pub(crate) struct SourceProjection {
    pub binding: Uuid,
    pub application: Uuid,
    pub fields: Value,
}
fn field(value: &Value) -> String {
    match value {
        Value::String(v) => v.trim().into(),
        Value::Number(v) => v.to_string(),
        _ => String::new(),
    }
}
fn normalize(value: &str) -> String {
    value
        .chars()
        .filter(|c| !c.is_whitespace() && !c.is_ascii_punctuation())
        .flat_map(char::to_lowercase)
        .collect()
}
fn similarity(a: &str, b: &str) -> bool {
    let a: Vec<char> = a.chars().collect();
    let b: Vec<char> = b.chars().collect();
    if a.len() < 2 || b.len() < 2 || a.len().abs_diff(b.len()) > 1 {
        return false;
    }
    let mut previous: Vec<usize> = (0..=b.len()).collect();
    for (i, x) in a.iter().enumerate() {
        let mut next = vec![i + 1];
        for (j, y) in b.iter().enumerate() {
            next.push(
                (next[j] + 1)
                    .min(previous[j + 1] + 1)
                    .min(previous[j] + usize::from(x != y)),
            );
        }
        previous = next;
    }
    previous[b.len()] <= 1
}
impl Store {
    pub(crate) async fn welcome_project_source(&self, r: &SourceProjection) -> Result<Value> {
        let fields = r
            .fields
            .as_object()
            .ok_or_else(|| anyhow::anyhow!("invalid_source_projection"))?;
        ensure!(
            fields.len() <= 10
                && fields.keys().all(|k| [
                    "name",
                    "nickname",
                    "phone",
                    "arrival",
                    "nights",
                    "room_type",
                    "occupation",
                    "interests",
                    "consent",
                    "status"
                ]
                .contains(&k.as_str()))
                && ["name", "nickname", "phone", "consent"]
                    .iter()
                    .all(|k| fields.contains_key(*k)),
            "invalid_source_projection"
        );
        ensure!(
            fields.values().all(|v| v.is_null()
                || v.is_boolean()
                || v.is_i64()
                || v.is_u64()
                || v.as_str()
                    .is_some_and(|s| s.chars().count() <= 2048 && !s.contains('\0'))),
            "invalid_source_projection"
        );
        let (mut tx, _, _) = self.begin().await?;
        let row=sqlx::query("SELECT b.scope_id,b.version,a.revision,a.valid,a.consent_active,a.field_hash FROM qintopia_agent_os.application_intake_states i JOIN qintopia_agent_os.business_property_bindings b ON b.tenant_key=i.tenant_key AND b.id=i.binding_id AND b.version=i.binding_version AND b.source_instance=i.source_instance AND b.property_id=i.property_id JOIN qintopia_agent_os.welcome_applications a ON a.id=i.application_id AND a.source_instance=i.source_instance AND a.resource_ref=i.resource_alias AND a.record_ref=i.record_ref WHERE i.tenant_key=$1 AND b.id=$2 AND a.id=$3 AND b.active FOR SHARE OF i,b,a")
            .bind(&self.tenant).bind(r.binding).bind(r.application).fetch_one(&mut *tx).await?;
        ensure!(
            row.get::<bool, _>("valid") && row.get::<bool, _>("consent_active"),
            "source_not_usable"
        );
        let scope: Uuid = row.get("scope_id");
        let basis = self
            .application_identity_basis(&mut tx, scope, r.application)
            .await?
            .ok_or_else(|| anyhow::anyhow!("trusted_readback_required"))?;
        let identity =
            json!({"name":fields["name"],"nickname":fields["nickname"],"phone":fields["phone"]});
        let full = json!({"fields":r.fields,"valid":row.get::<bool,_>("valid"),"consent_active":row.get::<bool,_>("consent_active")});
        ensure!(
            digest(&serde_json::to_vec(&identity)?) == basis
                && digest(&serde_json::to_vec(&full)?) == row.get::<String, _>("field_hash"),
            "source_projection_mismatch"
        );
        let hints = json!({"name":field(&identity["name"]),"nickname":field(&identity["nickname"]),"phone":field(&identity["phone"]),"arrival":field(&r.fields["arrival"])});
        ensure!(
            hints["name"].as_str().unwrap().chars().count() <= 120
                && hints["nickname"].as_str().unwrap().chars().count() <= 120
                && hints["phone"].as_str().unwrap().chars().count() <= 40
                && hints["arrival"].as_str().unwrap().chars().count() <= 40,
            "invalid_source_projection"
        );
        let changed=sqlx::query("INSERT INTO qintopia_agent_os.welcome_source_projections(tenant_key,scope_id,application_id,binding_id,binding_version,application_revision,identity_hash,field_hash,hints,expires_at) VALUES($1,$2,$3,$4,$5,$6,$7,$8,$9,clock_timestamp()+interval '24 hours') ON CONFLICT(application_id) DO UPDATE SET binding_version=EXCLUDED.binding_version,application_revision=EXCLUDED.application_revision,identity_hash=EXCLUDED.identity_hash,field_hash=EXCLUDED.field_hash,hints=EXCLUDED.hints,expires_at=EXCLUDED.expires_at,updated_at=clock_timestamp() WHERE welcome_source_projections.tenant_key=EXCLUDED.tenant_key AND welcome_source_projections.binding_id=EXCLUDED.binding_id AND welcome_source_projections.scope_id=EXCLUDED.scope_id")
            .bind(&self.tenant).bind(scope).bind(r.application).bind(r.binding).bind(row.get::<i64,_>("version")).bind(row.get::<i64,_>("revision")).bind(basis).bind(row.get::<String,_>("field_hash")).bind(hints).execute(&mut *tx).await?.rows_affected();
        ensure!(changed == 1, "source_projection_conflict");
        tx.commit().await?;
        Ok(json!({"stored":true,"identity_confirmed":false}))
    }

    pub(crate) async fn welcome_candidates(
        &self,
        subject: &WelcomeSubject,
        scope: Uuid,
        application: Uuid,
    ) -> Result<Value> {
        let (mut tx, _, _) = self.begin().await?;
        self.welcome_authorize(&mut tx, subject, scope, &["identity"])
            .await?;
        let basis = self
            .application_identity_basis(&mut tx, scope, application)
            .await?
            .ok_or_else(|| anyhow::anyhow!("trusted_readback_required"))?;
        let source=sqlx::query("SELECT p.hints,a.source_instance,b.property_id FROM qintopia_agent_os.welcome_source_projections p JOIN qintopia_agent_os.welcome_applications a ON a.id=p.application_id AND a.revision=p.application_revision AND a.field_hash=p.field_hash JOIN qintopia_agent_os.business_property_bindings b ON b.id=p.binding_id AND b.tenant_key=p.tenant_key AND b.version=p.binding_version AND b.active WHERE p.tenant_key=$1 AND p.scope_id=$2 AND a.id=$3 AND p.identity_hash=$4 AND p.expires_at>clock_timestamp() AND a.valid AND a.consent_active FOR SHARE OF p,a,b")
            .bind(&self.tenant).bind(scope).bind(application).bind(basis).fetch_optional(&mut *tx).await?.ok_or_else(||anyhow::anyhow!("current_source_projection_required"))?;
        let hints: Value = source.get("hints");
        let name = normalize(&field(&hints["name"]));
        let nickname = normalize(&field(&hints["nickname"]));
        // The authoritative stay projection may lack contact details; never invent them.
        let rows=sqlx::query("SELECT c.id AS case_id,c.occupant_id,c.version,c.stay_id,p.id AS person,p.display_name,p.preferred_name,ARRAY(SELECT alias FROM qintopia_identity.person_aliases WHERE person_id=p.id ORDER BY alias LIMIT 20) AS aliases,v.projection FROM qintopia_agent_os.welcome_cases c JOIN qintopia_agent_os.welcome_source_versions v ON v.source_instance=c.source_instance AND v.property_id=c.property_id AND v.aggregate_type='order' AND v.aggregate_id=c.order_id AND NOT v.invalidated AND NOT v.conflicted JOIN qintopia_agent_os.welcome_identity_scopes s ON s.source_instance=c.source_instance AND s.property_id=c.property_id LEFT JOIN qintopia_identity.source_identity_links l ON l.namespace=s.namespace AND l.subject_type='pms_occupant' AND l.source_ref=c.occupant_id AND l.status='confirmed' LEFT JOIN qintopia_identity.persons p ON p.id=l.person_id AND p.status='active' WHERE c.source_instance=$1 AND c.property_id=$2 AND c.admitted AND v.projection->>'state' IN ('Reserved','InHouse') AND v.projection->>'current_arrangement'='true' AND v.projection->>'inventory_reserved'='true' AND v.projection->>'stay'=c.stay_id AND EXISTS(SELECT 1 FROM jsonb_array_elements(v.projection->'occupants') o WHERE o->>'id'=c.occupant_id AND o->>'active'='true') AND EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_sources ws WHERE ws.source_instance=c.source_instance AND ws.property_id=c.property_id AND ws.enabled AND NOT ws.rebuilding) AND EXISTS(SELECT 1 FROM qintopia_agent_os.welcome_foundation_targets f JOIN qintopia_agent_os.welcome_targets t ON t.id=f.target_id WHERE f.tenant_key=$3 AND f.scope_id=$4 AND t.source_instance=c.source_instance AND t.property_id=c.property_id AND t.enabled AND (t.kind<>'building' OR t.building_code=v.projection->>'building')) ORDER BY c.id LIMIT 201")
            .bind(source.get::<String,_>("source_instance")).bind(source.get::<String,_>("property_id")).bind(&self.tenant).bind(scope).fetch_all(&mut *tx).await?;
        ensure!(rows.len() <= 200, "candidate_scope_too_large");
        let mut candidates = Vec::new();
        for row in rows {
            let display: String = row
                .get::<Option<String>, _>("display_name")
                .unwrap_or_else(|| "待核对入住人".into());
            let preferred: Option<String> = row.get("preferred_name");
            let mut names = vec![
                normalize(&display),
                normalize(preferred.as_deref().unwrap_or("")),
            ];
            names.extend(
                row.get::<Vec<String>, _>("aliases")
                    .iter()
                    .map(|s| normalize(s)),
            );
            let mut reasons = Vec::new();
            let mut score = 0;
            if !name.is_empty() && names.contains(&name) {
                score += 40;
                reasons.push("姓名一致，仅供核对");
            }
            if !nickname.is_empty() && names.contains(&nickname) {
                score += 25;
                reasons.push("昵称一致，昵称可重复或变更");
            }
            if score == 0
                && names
                    .iter()
                    .any(|n| similarity(&name, n) || similarity(&nickname, n))
            {
                score = 10;
                reasons.push("姓名或昵称近似，需核对错字或同名");
            }
            if score == 0 {
                reasons.push("住宿缺少可匹配的逐人姓名，需客服按实际入住记录核对");
            }
            let projection: Value = row.get("projection");
            let arrival = field(&hints["arrival"]);
            let arrival_match =
                !arrival.is_empty() && projection["arrival"].as_str() == Some(&arrival);
            if arrival_match {
                score += 10;
                reasons.push("申请与住宿开始日期一致");
            }
            candidates.push(json!({"case_ref":row.get::<Uuid,_>("case_id"),"person":row.get::<Option<Uuid>,_>("person"),"case_version":row.get::<i64,_>("version"),"label":format!("{} · {}",preferred.unwrap_or_else(||display.clone()),display),"building":projection["building"],"stay_ref":row.get::<String,_>("stay_id"),"occupant_ref":row.get::<String,_>("occupant_id"),"basis":reasons.join("；"),"score":score,"confirmed":false,"missing":["入住来源未提供可核验手机号；未用手机号认定本人"],"arrival_match":arrival_match}));
        }
        candidates.sort_by_key(|v| std::cmp::Reverse(v["score"].as_i64().unwrap_or(0)));
        let ambiguous = candidates.len() > 1;
        candidates.truncate(20);
        let phone = field(&hints["phone"]);
        let suffix: String = phone
            .chars()
            .rev()
            .take(4)
            .collect::<String>()
            .chars()
            .rev()
            .collect();
        Ok(
            json!({"application":application,"label":format!("{} · {}",field(&hints["nickname"]),field(&hints["name"])),"phone_hint":if phone.is_empty(){String::new()}else{format!("尾号{suffix}")},"candidates":candidates,"ambiguous":ambiguous,"automatic_binding":false}),
        )
    }

    pub(crate) async fn welcome_candidate_open(
        &self,
        subject: &WelcomeSubject,
        scope: Uuid,
        application: Uuid,
        case: Uuid,
    ) -> Result<Value> {
        let found = self.welcome_candidates(subject, scope, application).await?;
        ensure!(
            found["candidates"]
                .as_array()
                .is_some_and(|v| v.iter().any(|v| v["case_ref"] == json!(case))),
            "candidate_selection_required"
        );
        let (mut tx, _, _) = self.begin().await?;
        self.welcome_authorize(&mut tx, subject, scope, &["identity"])
            .await?;
        let work = crate::resident_welcome::store::create_work(
            &mut tx,
            &format!("welcome-candidate/{}/{case}/{application}", self.tenant),
            "welcome_event",
            "anan",
            "awaiting_review",
        )
        .await?;
        let payload = json!({"tenant_key":self.tenant,"scope_ref":scope,"case_ref":case,"application_ref":application});
        let changed=sqlx::query("UPDATE qintopia_agent_os.work_items SET payload=$2 WHERE id=$1 AND (payload='{}'::jsonb OR payload=$2)").bind(work).bind(payload).execute(&mut *tx).await?.rows_affected();
        ensure!(changed == 1, "welcome_work_item_conflict");
        tx.commit().await?;
        self.welcome_review_open_task(
            None,
            &ReviewOpen {
                work_item: work,
                scope,
                case_ref: case,
                application,
                artifacts: vec![],
            },
        )
        .await
    }
}
