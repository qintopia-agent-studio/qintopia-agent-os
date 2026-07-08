use std::collections::BTreeSet;

use anyhow::{bail, Context, Result};
use serde::Deserialize;
use serde_json::{json, Value};
use sqlx::{postgres::PgPool, Row};
use uuid::Uuid;

use crate::{
    config::Cli,
    db,
    knowledge::{self, KnowledgeSearchRequest},
    message_search::{self, SearchConfig, SearchMode, SearchRequest},
};

pub(crate) const WENYUANGE_LOOKUP_TOOL: &str = "qintopia_wenyuange_lookup";
pub(crate) const GIS_LOCATION_LOOKUP_TOOL: &str = "qintopia_gis_location_lookup";
pub(crate) const EXTERNAL_DISCLOSURE_FILTER_TOOL: &str = "qintopia_external_disclosure_filter";
pub(crate) const MEMBER_CONTEXT_LOOKUP_TOOL: &str = "qintopia_member_context_lookup";
pub(crate) const ANSWER_CONTEXT_PREPARE_TOOL: &str = "qintopia_answer_context_prepare";
pub(crate) const ERHUA_TRAINING_NOTE_SUBMIT_TOOL: &str = "qintopia_erhua_training_note_submit";

#[derive(Debug, Clone)]
pub(crate) struct ContextConfig {
    pub search: SearchConfig,
    pub allowed_callers: BTreeSet<String>,
    pub erhua_trainer_user_ids: BTreeSet<String>,
}

impl ContextConfig {
    pub(crate) fn from_cli(cli: &Cli, search: SearchConfig) -> Self {
        let allowed_callers = parse_allowed_callers(
            cli.context_mcp_allowed_callers
                .as_deref()
                .unwrap_or(&search.allowed_caller),
        );
        Self {
            search,
            allowed_callers,
            erhua_trainer_user_ids: parse_allowed_callers(&cli.erhua_trainer_user_ids),
        }
    }
}

pub(crate) async fn call_tool(
    pool: &PgPool,
    config: &ContextConfig,
    name: &str,
    arguments: Value,
) -> Result<Value> {
    match name {
        WENYUANGE_LOOKUP_TOOL => {
            let request: WenyuangeLookupRequest = serde_json::from_value(arguments)?;
            qintopia_wenyuange_lookup(pool, config, request).await
        }
        GIS_LOCATION_LOOKUP_TOOL => {
            let request: GisLocationLookupRequest = serde_json::from_value(arguments)?;
            Ok(qintopia_gis_location_lookup(request))
        }
        EXTERNAL_DISCLOSURE_FILTER_TOOL => {
            let request: ExternalDisclosureFilterRequest = serde_json::from_value(arguments)?;
            Ok(qintopia_external_disclosure_filter(request))
        }
        MEMBER_CONTEXT_LOOKUP_TOOL => {
            let request: MemberContextLookupRequest = serde_json::from_value(arguments)?;
            qintopia_member_context_lookup(pool, config, request).await
        }
        ANSWER_CONTEXT_PREPARE_TOOL => {
            let request: AnswerContextPrepareRequest = serde_json::from_value(arguments)?;
            qintopia_answer_context_prepare(pool, config, request).await
        }
        ERHUA_TRAINING_NOTE_SUBMIT_TOOL => {
            let request: ErhuaTrainingNoteSubmitRequest = serde_json::from_value(arguments)?;
            qintopia_erhua_training_note_submit(pool, config, request).await
        }
        _ => bail!("unknown context tool: {name}"),
    }
}

pub(crate) fn tool_definitions() -> Value {
    json!([
        {
            "name": WENYUANGE_LOOKUP_TOOL,
            "description": "Return filtered Qintopia context for Agent use. Authoritative public facts use qintopia_knowledge; group messages are used only for discussion/history evidence.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Question or context lookup query."},
                    "caller": {"type": "string", "description": "Calling profile id. Defaults to wenyuange for v1."},
                    "purpose": {"type": "string", "description": "Why this context is needed."},
                    "audience": {"type": "string", "description": "Intended audience, e.g. internal_agent, erhua, external_customer."},
                    "chat_id": {"type": "string", "description": "Optional QiWe chat/group id filter."},
                    "sender_id": {"type": "string", "description": "Optional QiWe sender id filter."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 10, "description": "Maximum evidence messages. Defaults to 5."}
                },
                "required": ["query", "purpose"],
                "additionalProperties": false
            }
        },
        {
            "name": GIS_LOCATION_LOOKUP_TOOL,
            "description": "Look up a small set of Public Qintopia GIS locations and return structured coordinates for channel adapters.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "query": {"type": "string", "description": "Location query, e.g. 1 栋 or 秦托邦1栋."},
                    "limit": {"type": "integer", "minimum": 1, "maximum": 5, "description": "Maximum candidate count."},
                    "caller": {"type": "string", "description": "Calling profile id."}
                },
                "required": ["query"],
                "additionalProperties": false
            }
        },
        {
            "name": EXTERNAL_DISCLOSURE_FILTER_TOOL,
            "description": "Filter an external-facing draft and mark whether Human Owner approval is required before sending.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "draft_answer": {"type": "string", "description": "Draft answer to check before external sending."},
                    "recipient": {"type": "string", "description": "Recipient category, e.g. external_customer."},
                    "purpose": {"type": "string", "description": "Disclosure purpose."}
                },
                "required": ["draft_answer"],
                "additionalProperties": false
            }
        },
        {
            "name": MEMBER_CONTEXT_LOOKUP_TOOL,
            "description": "Return safe member reply context for Erhua. This tool returns active safe profile snapshots only and audits every read.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "caller_profile": {"type": "string", "description": "Calling profile id, e.g. erhua."},
                    "platform": {"type": "string", "description": "Channel platform. Defaults to qiwe."},
                    "chat_id": {"type": "string", "description": "QiWe group/chat id."},
                    "channel_user_id": {"type": "string", "description": "QiWe sender/user id."},
                    "person_id": {"type": "string", "description": "Optional resolved person id."},
                    "member_name": {"type": "string", "description": "Optional member display name or alias mentioned in the current message, e.g. Cici."},
                    "purpose": {"type": "string", "description": "Why member context is needed."},
                    "current_message_summary": {"type": "string", "description": "Short summary of current message."}
                },
                "required": ["caller_profile", "purpose"],
                "additionalProperties": false
            }
        },
        {
            "name": ANSWER_CONTEXT_PREPARE_TOOL,
            "description": "Prepare deterministic safe answer context for Erhua before replying. Resolves speaker and mentioned members, returning safe profile summaries only.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "caller_profile": {"type": "string", "description": "Calling profile id, e.g. erhua."},
                    "platform": {"type": "string", "description": "Channel platform. Defaults to qiwe."},
                    "chat_id": {"type": "string", "description": "QiWe group/chat id."},
                    "sender_id": {"type": "string", "description": "Current QiWe sender/user id."},
                    "message_text": {"type": "string", "description": "Current inbound message text."},
                    "mentioned_member_names": {"type": "array", "items": {"type": "string"}, "description": "Optional high-confidence member display names or aliases extracted by the channel adapter, including QiWe atList display text."},
                    "purpose": {"type": "string", "description": "Why answer context is needed."}
                },
                "required": ["caller_profile", "purpose", "message_text"],
                "additionalProperties": false
            }
        },
        {
            "name": ERHUA_TRAINING_NOTE_SUBMIT_TOOL,
            "description": "Submit an audited Erhua trainer memory note. Only allowlisted trainer QiWe sender ids may write controlled training memory.",
            "inputSchema": {
                "type": "object",
                "properties": {
                    "caller_profile": {"type": "string", "description": "Calling profile id. Must be erhua."},
                    "platform": {"type": "string", "description": "Channel platform. Defaults to qiwe."},
                    "chat_id": {"type": "string", "description": "QiWe chat or direct conversation id."},
                    "source_conversation_type": {"type": "string", "enum": ["group", "direct"], "description": "QiWe source conversation type. Direct trainer chats may auto-activate safe persona rules."},
                    "trainer_user_id": {"type": "string", "description": "Real QiWe sender id of the trainer."},
                    "target_channel_user_id": {"type": "string", "description": "Optional QiWe user id this memory is about."},
                    "target_member_name": {"type": "string", "description": "Optional member display name this memory is about."},
                    "training_type": {"type": "string", "enum": ["member_preference", "member_fact", "reply_example", "persona_rule"]},
                    "training_text": {"type": "string", "description": "Trainer-provided memory text."},
                    "purpose": {"type": "string", "description": "Why the training note is being submitted."},
                    "source_platform_message_id": {"type": "string", "description": "Optional QiWe message id for audit."}
                },
                "required": ["caller_profile", "chat_id", "source_conversation_type", "trainer_user_id", "training_type", "training_text", "purpose"],
                "additionalProperties": false
            }
        }
    ])
}

async fn qintopia_member_context_lookup(
    pool: &PgPool,
    config: &ContextConfig,
    request: MemberContextLookupRequest,
) -> Result<Value> {
    let caller = clean_text(&request.caller_profile, 80);
    if caller.is_empty() {
        bail!("caller_profile is required");
    }
    validate_context_caller(config, &caller)?;
    let purpose = clean_text(&request.purpose, 500);
    if purpose.is_empty() {
        bail!("purpose is required");
    }
    let platform = clean_text(
        if request.platform.is_empty() {
            "qiwe"
        } else {
            &request.platform
        },
        40,
    );
    let chat_id = clean_text(&request.chat_id, 120);
    let channel_user_id = clean_text(&request.channel_user_id, 120);
    let person_id_text = clean_text(&request.person_id, 80);
    let member_name = clean_text(&request.member_name, 120);
    let member_resolution = if !person_id_text.is_empty() {
        MemberNameResolution::resolved(person_id_text.parse::<uuid::Uuid>()?, 1)
    } else if !member_name.is_empty() {
        resolve_member_by_name(pool, &platform, &chat_id, &member_name).await?
    } else {
        match resolve_person_id_by_channel_exact(pool, &platform, &chat_id, &channel_user_id)
            .await?
        {
            Some(person_id) => MemberNameResolution::resolved(person_id, 1),
            None => MemberNameResolution::unresolved(),
        }
    };
    if !member_resolution.status.is_resolved() {
        let reason = if member_resolution.status == MemberNameResolutionStatus::Ambiguous {
            "member_ambiguous"
        } else {
            "member_not_resolved"
        };
        write_member_context_audit(
            pool,
            &caller,
            &platform,
            &channel_user_id,
            &chat_id,
            &purpose,
            None,
            json!([]),
            json!([reason]),
            "qintopia_member_context_lookup",
        )
        .await?;
        return Ok(json!({
            "success": true,
            "can_use_context": false,
            "reason": reason,
            "resolution_status": member_resolution.status.as_str(),
            "match_count": member_resolution.match_count,
            "safe_summary": "",
            "do_not_disclose": ["raw_messages", "hidden_profile_details", "sensitive_facts", "daily_digest_full_text"]
        }));
    }
    let Some(context) = member_safe_context(
        pool,
        &platform,
        &chat_id,
        if channel_user_id.is_empty() {
            None
        } else {
            Some(&channel_user_id)
        },
        member_resolution.person_id,
        if channel_user_id.is_empty() {
            MemberSafeIdentityRowScope::ChatContext
        } else {
            MemberSafeIdentityRowScope::ExactChat
        },
    )
    .await?
    else {
        write_member_context_audit(
            pool,
            &caller,
            &platform,
            &channel_user_id,
            &chat_id,
            &purpose,
            None,
            json!([]),
            json!(["member_not_resolved"]),
            "qintopia_member_context_lookup",
        )
        .await?;
        return Ok(json!({
            "success": true,
            "can_use_context": false,
            "reason": "member_not_resolved",
            "safe_summary": "",
            "do_not_disclose": ["raw_messages", "hidden_profile_details", "sensitive_facts", "daily_digest_full_text"]
        }));
    };

    let fields_returned = json!([
        "person_id",
        "display_name",
        "identity_confidence",
        "safe_summary",
        "communication_style",
        "safe_reply_hints",
        "do_not_disclose",
        "sources_used"
    ]);
    let redactions = json!([
        "raw_messages",
        "hidden_profile_details",
        "sensitive_facts",
        "internal_labels",
        "daily_digest_full_text"
    ]);
    write_member_context_audit(
        pool,
        &caller,
        &platform,
        &channel_user_id,
        &chat_id,
        &purpose,
        context.person_id,
        fields_returned,
        redactions.clone(),
        "qintopia_member_context_lookup",
    )
    .await?;
    Ok(json!({
        "success": true,
        "can_use_context": !context.safe_summary.is_empty(),
        "resolution_status": member_resolution.status.as_str(),
        "match_count": member_resolution.match_count,
        "person_id": context.person_id,
        "display_name": context.display_name,
        "identity_confidence": context.identity_confidence,
        "safe_summary": context.safe_summary,
        "communication_style": context.communication_style,
        "safe_reply_hints": context.safe_reply_hints,
        "relevant_recent_context": clean_text(&request.current_message_summary, 300),
        "do_not_disclose": context.do_not_disclose,
        "risk_flags": [],
        "redactions": redactions,
        "sources_used": {
            "source_fact_ids": context.source_fact_ids,
            "source_summary_ids": context.source_summary_ids,
            "snapshot_generated_at": context.snapshot_generated_at
        }
    }))
}

async fn qintopia_answer_context_prepare(
    pool: &PgPool,
    config: &ContextConfig,
    request: AnswerContextPrepareRequest,
) -> Result<Value> {
    let caller = clean_text(&request.caller_profile, 80);
    if caller.is_empty() {
        bail!("caller_profile is required");
    }
    validate_context_caller(config, &caller)?;
    let purpose = clean_text(&request.purpose, 500);
    if purpose.is_empty() {
        bail!("purpose is required");
    }
    let platform = clean_text(
        if request.platform.is_empty() {
            "qiwe"
        } else {
            &request.platform
        },
        40,
    );
    let chat_id = clean_text(&request.chat_id, 120);
    let sender_id = clean_text(&request.sender_id, 120);
    let message_text = clean_text(&request.message_text, 1000);
    let speaker_identity =
        resolve_answer_context_person_id_by_channel(pool, &platform, &chat_id, &sender_id).await?;
    let speaker_context = member_safe_context(
        pool,
        &platform,
        &chat_id,
        if sender_id.is_empty() {
            None
        } else {
            Some(&sender_id)
        },
        speaker_identity.person_id,
        speaker_identity.member_safe_identity_row_scope(),
    )
    .await?;
    let mut mention_texts = request.mentioned_member_names();
    mention_texts.extend(mentioned_member_names(
        &message_text,
        speaker_context.as_ref(),
    ));
    mention_texts = unique_member_mentions(mention_texts);
    let mut mentioned_members = Vec::new();
    for mention_text in mention_texts {
        let resolution = resolve_member_by_name(pool, &platform, &chat_id, &mention_text)
            .await
            .with_context(|| format!("resolve mentioned member {mention_text}"))?;
        let context = if resolution.status.is_resolved() {
            member_safe_context(
                pool,
                &platform,
                &chat_id,
                None,
                resolution.person_id,
                MemberSafeIdentityRowScope::ChatContext,
            )
            .await?
        } else {
            None
        };
        write_mentioned_member_context_audit(
            pool,
            &caller,
            &platform,
            &chat_id,
            &purpose,
            &mention_text,
            &resolution,
        )
        .await?;
        mentioned_members.push(member_context_json(&mention_text, resolution, context));
    }
    let training_guidance = active_training_guidance(
        pool,
        &platform,
        &chat_id,
        &sender_id,
        speaker_identity.person_id,
    )
    .await?;
    let fields_returned = json!([
        "speaker",
        "mentioned_members",
        "training_guidance",
        "answer_rules"
    ]);
    let redactions = json!([
        "raw_messages",
        "hidden_profile_details",
        "sensitive_facts",
        "internal_labels",
        "daily_digest_full_text"
    ]);
    write_member_context_audit(
        pool,
        &caller,
        &platform,
        &sender_id,
        &chat_id,
        &purpose,
        speaker_identity.person_id,
        fields_returned,
        redactions.clone(),
        "qintopia_answer_context_prepare",
    )
    .await?;
    Ok(json!({
        "success": true,
        "speaker": speaker_context_json(speaker_context, &speaker_identity),
        "mentioned_members": mentioned_members,
        "answer_route": answer_route_json(&message_text),
        "training_guidance": training_guidance,
        "answer_rules": {
            "do_not_disclose_profile_source": true,
            "do_not_claim_monitoring": true,
            "ask_clarification_when_ambiguous": true,
            "do_not_guess_member_state": true,
            "do_not_expose_raw_history": true,
            "do_not_use_vector_search_to_guess_member_identity": true
        },
        "redactions": redactions
    }))
}

async fn qintopia_erhua_training_note_submit(
    pool: &PgPool,
    config: &ContextConfig,
    request: ErhuaTrainingNoteSubmitRequest,
) -> Result<Value> {
    let caller = clean_text(&request.caller_profile, 80);
    if caller != "erhua" {
        bail!("training notes may only be submitted by caller_profile=erhua");
    }
    validate_context_caller(config, &caller)?;
    let trainer_user_id = clean_text(&request.trainer_user_id, 120);
    if !is_erhua_trainer(config, &trainer_user_id) {
        return Ok(json!({
            "success": false,
            "accepted": false,
            "status": "rejected",
            "reason": "trainer_not_allowed",
            "training_id": null
        }));
    }
    let training_type = clean_text(&request.training_type, 80);
    validate_training_type(&training_type)?;
    let training_text = clean_text(&request.training_text, 1200);
    if training_text.is_empty() {
        bail!("training_text is required");
    }
    let purpose = clean_text(&request.purpose, 500);
    if purpose.is_empty() {
        bail!("purpose is required");
    }
    let platform = clean_text(
        if request.platform.is_empty() {
            "qiwe"
        } else {
            &request.platform
        },
        40,
    );
    let chat_id = clean_text(&request.chat_id, 120);
    let source_conversation_type = clean_text(&request.source_conversation_type, 40);
    validate_source_conversation_type(&source_conversation_type)?;
    let target_channel_user_id = clean_text(&request.target_channel_user_id, 120);
    let target_member_name = clean_text(&request.target_member_name, 120);
    let target_person_id = resolve_training_target_person_id(
        pool,
        &platform,
        &chat_id,
        &target_channel_user_id,
        &target_member_name,
    )
    .await?;
    let source_kind = training_source_kind(
        &training_type,
        &chat_id,
        &trainer_user_id,
        &source_conversation_type,
    );
    let decision = classify_training_note(&training_type, &training_text, source_kind);
    let source_platform_message_id = clean_text(&request.source_platform_message_id, 160);
    let training_id = insert_training_note(
        pool,
        TrainingNoteInsert {
            caller_profile: &caller,
            platform: &platform,
            chat_id: &chat_id,
            trainer_user_id: &trainer_user_id,
            target_channel_user_id: &target_channel_user_id,
            target_member_name: &target_member_name,
            target_person_id,
            training_type: &training_type,
            training_text: &training_text,
            sanitized_summary: &decision.sanitized_summary,
            status: decision.status,
            risk_level: decision.risk_level,
            reason: &decision.reason,
            source_platform_message_id: &source_platform_message_id,
            source_conversation_type: &source_conversation_type,
            purpose: &purpose,
        },
    )
    .await?;

    let mut applied_member_fact_id = None;
    let mut applied_profile_snapshot_id = None;
    let mut applied_persona_overlay_id = None;
    if decision.status == "active"
        && matches!(training_type.as_str(), "member_preference" | "member_fact")
    {
        if let Some(person_id) = target_person_id {
            let applied = apply_training_member_memory(
                pool,
                training_id,
                person_id,
                &platform,
                &chat_id,
                &target_channel_user_id,
                &training_type,
                &decision.sanitized_summary,
            )
            .await?;
            applied_member_fact_id = applied.member_fact_id;
            applied_profile_snapshot_id = applied.profile_snapshot_id;
            update_training_note_applied_artifacts(pool, training_id, &applied).await?;
        }
    } else if training_type == "persona_rule" {
        let overlay_id = insert_persona_overlay(
            pool,
            training_id,
            &decision.sanitized_summary,
            decision.status,
            decision.risk_level,
        )
        .await?;
        applied_persona_overlay_id = Some(overlay_id);
        update_training_note_persona_overlay(pool, training_id, overlay_id).await?;
    }

    Ok(json!({
        "success": true,
        "accepted": decision.status != "rejected",
        "training_id": training_id,
        "status": decision.status,
        "risk_level": decision.risk_level,
        "reason": decision.reason,
        "training_type": training_type,
        "applied": {
            "member_fact_id": applied_member_fact_id,
            "profile_snapshot_id": applied_profile_snapshot_id,
            "persona_overlay_id": applied_persona_overlay_id
        },
        "safe_summary": decision.sanitized_summary
    }))
}

async fn resolve_answer_context_person_id_by_channel(
    pool: &PgPool,
    platform: &str,
    chat_id: &str,
    channel_user_id: &str,
) -> Result<AnswerContextIdentityResolution> {
    if channel_user_id.is_empty() {
        return Ok(AnswerContextIdentityResolution::unresolved());
    }
    let exact_row = sqlx::query(
        r#"
        SELECT ci.person_id
        FROM qintopia_identity.channel_identities ci
        WHERE ci.platform = $1
          AND $2 <> ''
          AND ci.chat_id = $2
          AND ci.channel_user_id = $3
          AND ci.person_id IS NOT NULL
        LIMIT 1
        "#,
    )
    .bind(platform)
    .bind(chat_id)
    .bind(channel_user_id)
    .fetch_optional(pool)
    .await
    .context("resolve exact answer context person id by channel identity")?;
    if let Some(row) = exact_row {
        return Ok(AnswerContextIdentityResolution::resolved(
            row.try_get("person_id")?,
            IdentityResolutionScope::ExactChat,
        ));
    }

    if !platform.eq_ignore_ascii_case("qiwe") {
        return Ok(AnswerContextIdentityResolution::unresolved());
    }

    let rows = sqlx::query(
        r#"
        SELECT
            ci.person_id,
            bool_or(ci.chat_id = '') AS has_platform_identity
        FROM qintopia_identity.channel_identities ci
        WHERE ci.platform = 'qiwe'
          AND ci.channel_user_id = $1
          AND ci.person_id IS NOT NULL
        GROUP BY ci.person_id
        ORDER BY max(ci.updated_at) DESC
        LIMIT 2
        "#,
    )
    .bind(channel_user_id)
    .fetch_all(pool)
    .await
    .context("resolve QiWe answer context person id by user identity")?;

    match rows.as_slice() {
        [] => Ok(AnswerContextIdentityResolution::unresolved()),
        [row] => {
            let has_platform_identity: bool = row.try_get("has_platform_identity")?;
            if !has_platform_identity {
                return Ok(AnswerContextIdentityResolution::unresolved());
            }
            Ok(AnswerContextIdentityResolution::resolved(
                row.try_get("person_id")?,
                IdentityResolutionScope::QiwePlatformUser,
            ))
        }
        _ => Ok(AnswerContextIdentityResolution::conflict()),
    }
}

#[cfg(test)]
#[derive(Debug, Clone)]
struct ChannelIdentityCandidate {
    platform: String,
    chat_id: String,
    channel_user_id: String,
    person_id: uuid::Uuid,
    updated_at: chrono::DateTime<chrono::Utc>,
}

#[cfg(test)]
fn select_answer_context_identity_candidate(
    platform: &str,
    chat_id: &str,
    channel_user_id: &str,
    candidates: &[ChannelIdentityCandidate],
) -> AnswerContextIdentityResolution {
    if channel_user_id.is_empty() {
        return AnswerContextIdentityResolution::unresolved();
    }
    if let Some(candidate) = candidates.iter().find(|candidate| {
        candidate.platform == platform
            && !chat_id.is_empty()
            && candidate.chat_id == chat_id
            && candidate.channel_user_id == channel_user_id
    }) {
        return AnswerContextIdentityResolution::resolved(
            candidate.person_id,
            IdentityResolutionScope::ExactChat,
        );
    }

    if !platform.eq_ignore_ascii_case("qiwe") {
        return AnswerContextIdentityResolution::unresolved();
    }

    let mut person_candidates = candidates
        .iter()
        .filter(|candidate| candidate.platform == "qiwe")
        .filter(|candidate| candidate.channel_user_id == channel_user_id)
        .fold(
            Vec::<(uuid::Uuid, bool, chrono::DateTime<chrono::Utc>)>::new(),
            |mut acc, candidate| {
                if let Some((_, has_platform_identity, updated_at)) = acc
                    .iter_mut()
                    .find(|(person_id, _, _)| *person_id == candidate.person_id)
                {
                    *has_platform_identity |= candidate.chat_id.is_empty();
                    if candidate.updated_at > *updated_at {
                        *updated_at = candidate.updated_at;
                    }
                } else {
                    acc.push((
                        candidate.person_id,
                        candidate.chat_id.is_empty(),
                        candidate.updated_at,
                    ));
                }
                acc
            },
        )
        .into_iter()
        .collect::<Vec<_>>();
    person_candidates.sort_by(|left, right| right.2.cmp(&left.2));

    match person_candidates.as_slice() {
        [] => AnswerContextIdentityResolution::unresolved(),
        [(_, false, _)] => AnswerContextIdentityResolution::unresolved(),
        [(person_id, true, _)] => AnswerContextIdentityResolution::resolved(
            *person_id,
            IdentityResolutionScope::QiwePlatformUser,
        ),
        _ => AnswerContextIdentityResolution::conflict(),
    }
}

#[cfg(test)]
fn select_member_safe_identity_candidate<'a>(
    platform: &str,
    chat_id: &str,
    channel_user_id: &str,
    person_id: uuid::Uuid,
    scope: MemberSafeIdentityRowScope,
    candidates: &'a [ChannelIdentityCandidate],
) -> Option<&'a ChannelIdentityCandidate> {
    let mut matches = candidates
        .iter()
        .filter(|candidate| candidate.person_id == person_id)
        .filter(|candidate| candidate.platform == platform)
        .filter(|candidate| match scope {
            MemberSafeIdentityRowScope::ExactChat => {
                !chat_id.is_empty()
                    && candidate.chat_id == chat_id
                    && (channel_user_id.is_empty() || candidate.channel_user_id == channel_user_id)
            }
            MemberSafeIdentityRowScope::QiwePlatformUser => {
                candidate.chat_id.is_empty()
                    && (channel_user_id.is_empty() || candidate.channel_user_id == channel_user_id)
            }
            MemberSafeIdentityRowScope::ChatContext => {
                channel_user_id.is_empty() && (chat_id.is_empty() || candidate.chat_id == chat_id)
            }
        })
        .collect::<Vec<_>>();
    matches.sort_by(|left, right| {
        let left_rank = member_safe_identity_candidate_rank(left, chat_id, scope);
        let right_rank = member_safe_identity_candidate_rank(right, chat_id, scope);
        left_rank
            .cmp(&right_rank)
            .then_with(|| right.updated_at.cmp(&left.updated_at))
    });
    matches.into_iter().next()
}

#[cfg(test)]
fn member_safe_identity_candidate_rank(
    candidate: &ChannelIdentityCandidate,
    chat_id: &str,
    scope: MemberSafeIdentityRowScope,
) -> i32 {
    match scope {
        MemberSafeIdentityRowScope::ExactChat
            if !chat_id.is_empty() && candidate.chat_id == chat_id =>
        {
            0
        }
        MemberSafeIdentityRowScope::QiwePlatformUser if candidate.chat_id.is_empty() => 0,
        MemberSafeIdentityRowScope::ChatContext
            if !chat_id.is_empty() && candidate.chat_id == chat_id =>
        {
            0
        }
        _ => 1,
    }
}

async fn resolve_person_id_by_channel_exact(
    pool: &PgPool,
    platform: &str,
    chat_id: &str,
    channel_user_id: &str,
) -> Result<Option<uuid::Uuid>> {
    if chat_id.is_empty() || channel_user_id.is_empty() {
        return Ok(None);
    }
    let row = sqlx::query(
        r#"
        SELECT ci.person_id
        FROM qintopia_identity.channel_identities ci
        WHERE ci.platform = $1
          AND ci.chat_id = $2
          AND ci.channel_user_id = $3
          AND ci.person_id IS NOT NULL
        ORDER BY ci.updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(platform)
    .bind(chat_id)
    .bind(channel_user_id)
    .fetch_optional(pool)
    .await
    .context("resolve exact member context person id by channel identity")?;
    row.map(|row| row.try_get("person_id"))
        .transpose()
        .map_err(Into::into)
}

#[derive(Debug, Clone)]
struct AnswerContextIdentityResolution {
    person_id: Option<uuid::Uuid>,
    resolution_scope: IdentityResolutionScope,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum IdentityResolutionScope {
    ExactChat,
    QiwePlatformUser,
    Conflict,
    Unresolved,
}

impl AnswerContextIdentityResolution {
    fn resolved(person_id: uuid::Uuid, resolution_scope: IdentityResolutionScope) -> Self {
        Self {
            person_id: Some(person_id),
            resolution_scope,
        }
    }

    fn unresolved() -> Self {
        Self {
            person_id: None,
            resolution_scope: IdentityResolutionScope::Unresolved,
        }
    }

    fn conflict() -> Self {
        Self {
            person_id: None,
            resolution_scope: IdentityResolutionScope::Conflict,
        }
    }

    fn member_safe_identity_row_scope(&self) -> MemberSafeIdentityRowScope {
        match self.resolution_scope {
            IdentityResolutionScope::QiwePlatformUser => {
                MemberSafeIdentityRowScope::QiwePlatformUser
            }
            _ => MemberSafeIdentityRowScope::ExactChat,
        }
    }
}

impl IdentityResolutionScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::ExactChat => "exact_chat",
            Self::QiwePlatformUser => "qiwe_platform_user",
            Self::Conflict => "conflict",
            Self::Unresolved => "unresolved",
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberSafeIdentityRowScope {
    ExactChat,
    QiwePlatformUser,
    ChatContext,
}

impl MemberSafeIdentityRowScope {
    fn as_str(self) -> &'static str {
        match self {
            Self::ExactChat => "exact_chat",
            Self::QiwePlatformUser => "qiwe_platform_user",
            Self::ChatContext => "chat_context",
        }
    }
}

#[derive(Debug, Clone)]
struct MemberSafeContext {
    person_id: Option<uuid::Uuid>,
    display_name: Option<String>,
    identity_confidence: Option<f64>,
    safe_summary: String,
    communication_style: Value,
    safe_reply_hints: Value,
    do_not_disclose: Value,
    source_fact_ids: Vec<uuid::Uuid>,
    source_summary_ids: Vec<uuid::Uuid>,
    snapshot_generated_at: Option<chrono::DateTime<chrono::Utc>>,
}

async fn member_safe_context(
    pool: &PgPool,
    platform: &str,
    chat_id: &str,
    channel_user_id: Option<&str>,
    person_id: Option<uuid::Uuid>,
    identity_row_scope: MemberSafeIdentityRowScope,
) -> Result<Option<MemberSafeContext>> {
    let Some(person_id) = person_id else {
        return Ok(None);
    };
    let channel_user_id = channel_user_id.unwrap_or("");
    let row = sqlx::query(
        r#"
        SELECT
            p.id AS person_id,
            COALESCE(NULLIF(ci.display_name, ''), p.display_name) AS display_name,
            ci.confidence AS identity_confidence,
            s.summary,
            s.communication_style,
            s.safe_reply_hints,
            s.do_not_disclose,
            s.source_fact_ids,
            s.source_summary_ids,
            s.generated_at
        FROM qintopia_identity.persons p
        LEFT JOIN LATERAL (
            SELECT *
            FROM qintopia_identity.channel_identities ci
            WHERE ci.person_id = p.id
              AND ci.platform = $2
              AND (
                (
                  $5 = 'exact_chat'
                  AND $3 <> ''
                  AND ci.chat_id = $3
                  AND ($4 = '' OR ci.channel_user_id = $4)
                )
                OR (
                  $5 = 'qiwe_platform_user'
                  AND ci.chat_id = ''
                  AND ($4 = '' OR ci.channel_user_id = $4)
                )
                OR (
                  $5 = 'chat_context'
                  AND $4 = ''
                  AND ($3 = '' OR ci.chat_id = $3)
                )
              )
            ORDER BY
              CASE
                WHEN $5 = 'exact_chat' AND $3 <> '' AND ci.chat_id = $3 THEN 0
                WHEN $5 = 'qiwe_platform_user' AND ci.chat_id = '' THEN 0
                WHEN $5 = 'chat_context' AND $3 <> '' AND ci.chat_id = $3 THEN 0
                ELSE 1
              END,
              ci.updated_at DESC
            LIMIT 1
        ) ci ON true
        LEFT JOIN LATERAL (
            SELECT *
            FROM qintopia_identity.member_profile_snapshots s
            WHERE s.person_id = p.id
              AND s.profile_kind = 'reply_context'
              AND s.status = 'active'
              AND (s.valid_until IS NULL OR s.valid_until > now())
            ORDER BY s.generated_at DESC
            LIMIT 1
        ) s ON true
        WHERE p.id = $1
        LIMIT 1
        "#,
    )
    .bind(person_id)
    .bind(platform)
    .bind(chat_id)
    .bind(channel_user_id)
    .bind(identity_row_scope.as_str())
    .fetch_optional(pool)
    .await
    .context("load member safe context")?;
    let Some(row) = row else {
        return Ok(None);
    };
    Ok(Some(MemberSafeContext {
        person_id: Some(row.try_get("person_id")?),
        display_name: row.try_get("display_name")?,
        identity_confidence: row.try_get("identity_confidence")?,
        safe_summary: row
            .try_get::<Option<String>, _>("summary")?
            .unwrap_or_default(),
        communication_style: row
            .try_get::<Option<Value>, _>("communication_style")?
            .unwrap_or_else(|| json!({})),
        safe_reply_hints: row
            .try_get::<Option<Value>, _>("safe_reply_hints")?
            .unwrap_or_else(|| json!({})),
        do_not_disclose: row
            .try_get::<Option<Value>, _>("do_not_disclose")?
            .unwrap_or_else(|| json!({})),
        source_fact_ids: row
            .try_get::<Option<Vec<uuid::Uuid>>, _>("source_fact_ids")?
            .unwrap_or_default(),
        source_summary_ids: row
            .try_get::<Option<Vec<uuid::Uuid>>, _>("source_summary_ids")?
            .unwrap_or_default(),
        snapshot_generated_at: row.try_get("generated_at")?,
    }))
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemberNameResolution {
    status: MemberNameResolutionStatus,
    person_id: Option<uuid::Uuid>,
    match_count: usize,
}

impl MemberNameResolution {
    fn resolved(person_id: uuid::Uuid, match_count: usize) -> Self {
        Self {
            status: MemberNameResolutionStatus::Resolved,
            person_id: Some(person_id),
            match_count,
        }
    }

    fn ambiguous(match_count: usize) -> Self {
        Self {
            status: MemberNameResolutionStatus::Ambiguous,
            person_id: None,
            match_count,
        }
    }

    fn unresolved() -> Self {
        Self {
            status: MemberNameResolutionStatus::Unresolved,
            person_id: None,
            match_count: 0,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberNameResolutionStatus {
    Resolved,
    Ambiguous,
    Unresolved,
}

impl MemberNameResolutionStatus {
    fn as_str(self) -> &'static str {
        match self {
            Self::Resolved => "resolved",
            Self::Ambiguous => "ambiguous",
            Self::Unresolved => "unresolved",
        }
    }

    fn is_resolved(self) -> bool {
        self == Self::Resolved
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct MemberNameCandidate {
    person_id: uuid::Uuid,
    rank: i32,
}

fn select_member_name_resolution(
    candidates: &[MemberNameCandidate],
    ambiguity_rank_gap: i32,
) -> MemberNameResolution {
    let Some(top) = candidates.first() else {
        return MemberNameResolution::unresolved();
    };
    let close_matches = candidates
        .iter()
        .filter(|candidate| top.rank - candidate.rank <= ambiguity_rank_gap)
        .map(|candidate| candidate.person_id)
        .collect::<BTreeSet<_>>();
    if close_matches.len() > 1 {
        return MemberNameResolution::ambiguous(close_matches.len());
    }
    MemberNameResolution::resolved(top.person_id, close_matches.len())
}

fn select_scoped_member_name_resolution(
    current_chat_candidates: &[MemberNameCandidate],
    platform_candidates: &[MemberNameCandidate],
) -> MemberNameResolution {
    let current_chat_resolution = select_member_name_resolution(current_chat_candidates, 4);
    if current_chat_resolution.status != MemberNameResolutionStatus::Unresolved {
        return current_chat_resolution;
    }
    select_member_name_resolution(platform_candidates, 4)
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum MemberNameLookupScope {
    CurrentChat,
    Platform,
}

async fn resolve_member_by_name(
    pool: &PgPool,
    platform: &str,
    chat_id: &str,
    member_name: &str,
) -> Result<MemberNameResolution> {
    let normalized = db::normalize_display_name(member_name);
    if normalized.is_empty() {
        return Ok(MemberNameResolution::unresolved());
    }
    let current_chat_candidates = member_name_candidates(
        pool,
        platform,
        chat_id,
        member_name,
        &normalized,
        MemberNameLookupScope::CurrentChat,
    )
    .await?;
    let platform_candidates = member_name_candidates(
        pool,
        platform,
        chat_id,
        member_name,
        &normalized,
        MemberNameLookupScope::Platform,
    )
    .await?;
    Ok(select_scoped_member_name_resolution(
        &current_chat_candidates,
        &platform_candidates,
    ))
}

async fn member_name_candidates(
    pool: &PgPool,
    platform: &str,
    chat_id: &str,
    member_name: &str,
    normalized: &str,
    scope: MemberNameLookupScope,
) -> Result<Vec<MemberNameCandidate>> {
    let like_pattern = format!("%{normalized}%");
    let rows = sqlx::query(
        r#"
        SELECT person_id, rank
        FROM (
            SELECT person_id, max(rank) AS rank, max(observed_at) AS observed_at
            FROM (
            SELECT
                ci.person_id,
                ci.updated_at AS observed_at,
                CASE
                    WHEN ci.normalized_display_name = $3 THEN 100
                    WHEN regexp_replace(btrim(coalesce(ci.display_name, '')), '[[:space:]]+', ' ', 'g') = $3 THEN 98
                    WHEN ci.display_name = $4 THEN 95
                    WHEN regexp_replace(btrim(coalesce(p.display_name, '')), '[[:space:]]+', ' ', 'g') = $3
                      OR regexp_replace(btrim(coalesce(p.preferred_name, '')), '[[:space:]]+', ' ', 'g') = $3
                      OR regexp_replace(btrim(coalesce(p.primary_name, '')), '[[:space:]]+', ' ', 'g') = $3 THEN 90
                    WHEN regexp_replace(btrim(coalesce(a.alias, '')), '[[:space:]]+', ' ', 'g') = $3 THEN 85
                    WHEN ci.normalized_display_name ILIKE $5 THEN 70
                    WHEN ci.display_name ILIKE $5 THEN 65
                    WHEN p.display_name ILIKE $5 OR p.preferred_name ILIKE $5 OR p.primary_name ILIKE $5 THEN 60
                    WHEN a.alias ILIKE $5 THEN 55
                    ELSE 0
                END AS rank
            FROM qintopia_identity.channel_identities ci
            JOIN qintopia_identity.persons p ON p.id = ci.person_id
            LEFT JOIN qintopia_identity.person_aliases a ON a.person_id = p.id
            WHERE ci.platform = $1
              AND (
                ($6 = 'current_chat' AND ($2 = '' OR ci.chat_id = $2))
                OR (
                  $6 = 'platform'
                  AND (
                    ci.chat_id = ''
                    OR ci.normalized_display_name = $3
                    OR regexp_replace(btrim(coalesce(ci.display_name, '')), '[[:space:]]+', ' ', 'g') = $3
                    OR regexp_replace(btrim(coalesce(p.display_name, '')), '[[:space:]]+', ' ', 'g') = $3
                    OR regexp_replace(btrim(coalesce(p.preferred_name, '')), '[[:space:]]+', ' ', 'g') = $3
                    OR regexp_replace(btrim(coalesce(p.primary_name, '')), '[[:space:]]+', ' ', 'g') = $3
                    OR regexp_replace(btrim(coalesce(a.alias, '')), '[[:space:]]+', ' ', 'g') = $3
                    OR ci.normalized_display_name ILIKE $5
                    OR ci.display_name ILIKE $5
                    OR p.display_name ILIKE $5
                    OR p.preferred_name ILIKE $5
                    OR p.primary_name ILIKE $5
                    OR a.alias ILIKE $5
                  )
                )
              )
              AND ci.person_id IS NOT NULL
            ) row_candidates
            WHERE rank > 0
            GROUP BY person_id
        ) candidates
        ORDER BY rank DESC, observed_at DESC
        LIMIT 5
        "#,
    )
    .bind(platform)
    .bind(chat_id)
    .bind(&normalized)
    .bind(member_name)
    .bind(&like_pattern)
    .bind(match scope {
        MemberNameLookupScope::CurrentChat => "current_chat",
        MemberNameLookupScope::Platform => "platform",
    })
    .fetch_all(pool)
    .await
    .context("resolve member context by member name")?;
    let mut candidates = Vec::new();
    for row in rows {
        candidates.push(MemberNameCandidate {
            person_id: row.try_get("person_id")?,
            rank: row.try_get("rank")?,
        });
    }
    Ok(candidates)
}

async fn write_member_context_audit(
    pool: &PgPool,
    caller: &str,
    platform: &str,
    channel_user_id: &str,
    chat_id: &str,
    purpose: &str,
    person_id: Option<uuid::Uuid>,
    fields_returned: Value,
    redactions: Value,
    tool: &str,
) -> Result<()> {
    sqlx::query(
        r#"
        INSERT INTO qintopia_identity.member_context_audit
            (caller_profile, platform, channel_user_id, person_id, chat_id, purpose, fields_returned, redactions, metadata)
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, jsonb_build_object('tool', $9::text))
        "#,
    )
    .bind(caller)
    .bind(platform)
    .bind(channel_user_id)
    .bind(person_id)
    .bind(chat_id)
    .bind(purpose)
    .bind(fields_returned)
    .bind(redactions)
    .bind(tool)
    .execute(pool)
    .await?;
    Ok(())
}

async fn write_mentioned_member_context_audit(
    pool: &PgPool,
    caller: &str,
    platform: &str,
    chat_id: &str,
    purpose: &str,
    mention_text: &str,
    resolution: &MemberNameResolution,
) -> Result<()> {
    let fields_returned = if resolution.status.is_resolved() {
        json!([
            "mention_text",
            "person_id",
            "display_name",
            "identity_confidence",
            "safe_summary",
            "safe_reply_hints",
            "communication_style"
        ])
    } else {
        json!(["mention_text", "resolution_status", "match_count"])
    };
    let redactions = if resolution.status == MemberNameResolutionStatus::Ambiguous {
        json!([
            "raw_messages",
            "hidden_profile_details",
            "sensitive_facts",
            "member_ambiguous"
        ])
    } else if resolution.status == MemberNameResolutionStatus::Unresolved {
        json!([
            "raw_messages",
            "hidden_profile_details",
            "sensitive_facts",
            "member_not_resolved"
        ])
    } else {
        json!([
            "raw_messages",
            "hidden_profile_details",
            "sensitive_facts",
            "internal_labels",
            "daily_digest_full_text"
        ])
    };
    let audit_purpose = format!(
        "{}; mentioned_member={}; resolution_status={}; match_count={}",
        clean_text(purpose, 360),
        clean_text(mention_text, 80),
        resolution.status.as_str(),
        resolution.match_count
    );
    write_member_context_audit(
        pool,
        caller,
        platform,
        "",
        chat_id,
        &audit_purpose,
        resolution.person_id,
        fields_returned,
        redactions,
        "qintopia_answer_context_prepare.mentioned_member",
    )
    .await
}

async fn active_training_guidance(
    pool: &PgPool,
    platform: &str,
    chat_id: &str,
    sender_id: &str,
    person_id: Option<Uuid>,
) -> Result<Value> {
    let persona_rows = sqlx::query(
        r#"
        SELECT overlay_text
        FROM qintopia_identity.erhua_persona_overlays
        WHERE status = 'active'
          AND revoked_at IS NULL
          AND valid_from <= now()
          AND (valid_until IS NULL OR valid_until > now())
        ORDER BY priority ASC, created_at DESC
        LIMIT 3
        "#,
    )
    .fetch_all(pool)
    .await
    .context("load active Erhua persona overlays")?;
    let persona_overlays = persona_rows
        .iter()
        .filter_map(|row| row.try_get::<String, _>("overlay_text").ok())
        .map(|text| clean_text(&text, 260))
        .filter(|text| !text.is_empty())
        .collect::<Vec<_>>();

    let member_rows = sqlx::query(
        r#"
        SELECT sanitized_summary, training_type
        FROM qintopia_identity.erhua_training_notes
        WHERE status = 'active'
          AND revoked_at IS NULL
          AND platform = $1
          AND (
            ($2::uuid IS NOT NULL AND target_person_id = $2)
            OR ($3 <> '' AND target_channel_user_id = $3)
          )
        ORDER BY created_at DESC
        LIMIT 5
        "#,
    )
    .bind(platform)
    .bind(person_id)
    .bind(sender_id)
    .fetch_all(pool)
    .await
    .context("load active Erhua member training guidance")?;
    let member_guidance = member_rows
        .iter()
        .filter_map(|row| {
            let summary = row.try_get::<String, _>("sanitized_summary").ok()?;
            let training_type = row
                .try_get::<String, _>("training_type")
                .unwrap_or_default();
            Some(json!({
                "training_type": training_type,
                "summary": clean_text(&summary, 260)
            }))
        })
        .collect::<Vec<_>>();

    let reply_rows = sqlx::query(
        r#"
        SELECT sanitized_summary, chat_id
        FROM qintopia_identity.erhua_training_notes
        WHERE status = 'active'
          AND revoked_at IS NULL
          AND platform = $1
          AND training_type = 'reply_example'
          AND ($2 = '' OR chat_id = '' OR chat_id = $2)
        ORDER BY created_at DESC
        LIMIT 3
        "#,
    )
    .bind(platform)
    .bind(chat_id)
    .fetch_all(pool)
    .await
    .context("load active Erhua reply-example training guidance")?;
    let reply_examples = reply_rows
        .iter()
        .filter_map(|row| {
            let summary = row.try_get::<String, _>("sanitized_summary").ok()?;
            Some(clean_text(&summary, 260))
        })
        .filter(|summary| !summary.is_empty())
        .collect::<Vec<_>>();

    Ok(json!({
        "persona_overlays": persona_overlays,
        "member_guidance": member_guidance,
        "reply_examples": reply_examples,
        "rules": {
            "use_only_active_training": true,
            "do_not_expose_training_source": true,
            "do_not_override_safety_boundaries": true,
            "chat_id": chat_id
        }
    }))
}

async fn resolve_training_target_person_id(
    pool: &PgPool,
    platform: &str,
    chat_id: &str,
    target_channel_user_id: &str,
    target_member_name: &str,
) -> Result<Option<Uuid>> {
    if !target_channel_user_id.is_empty() {
        return resolve_person_id_by_channel_exact(pool, platform, chat_id, target_channel_user_id)
            .await;
    }
    if !target_member_name.is_empty() {
        let resolution =
            resolve_member_by_name(pool, platform, chat_id, target_member_name).await?;
        return Ok(if resolution.status.is_resolved() {
            resolution.person_id
        } else {
            None
        });
    }
    Ok(None)
}

struct TrainingNoteInsert<'a> {
    caller_profile: &'a str,
    platform: &'a str,
    chat_id: &'a str,
    trainer_user_id: &'a str,
    target_channel_user_id: &'a str,
    target_member_name: &'a str,
    target_person_id: Option<Uuid>,
    training_type: &'a str,
    training_text: &'a str,
    sanitized_summary: &'a str,
    status: &'static str,
    risk_level: &'static str,
    reason: &'a str,
    source_platform_message_id: &'a str,
    source_conversation_type: &'a str,
    purpose: &'a str,
}

async fn insert_training_note(pool: &PgPool, note: TrainingNoteInsert<'_>) -> Result<Uuid> {
    let row = sqlx::query(
        r#"
        INSERT INTO qintopia_identity.erhua_training_notes
            (
                caller_profile,
                platform,
                chat_id,
                trainer_user_id,
                target_channel_user_id,
                target_member_name,
                target_person_id,
                training_type,
                training_text,
                sanitized_summary,
                status,
                risk_level,
                reason,
                source_platform_message_id,
                metadata
            )
        VALUES ($1, $2, $3, $4, $5, $6, $7, $8, $9, $10, $11, $12, $13, $14, jsonb_build_object('purpose', $15::text, 'source_conversation_type', $16::text, 'submitted_via', 'mcp_context_v1'))
        RETURNING id
        "#,
    )
    .bind(note.caller_profile)
    .bind(note.platform)
    .bind(note.chat_id)
    .bind(note.trainer_user_id)
    .bind(note.target_channel_user_id)
    .bind(note.target_member_name)
    .bind(note.target_person_id)
    .bind(note.training_type)
    .bind(note.training_text)
    .bind(note.sanitized_summary)
    .bind(note.status)
    .bind(note.risk_level)
    .bind(note.reason)
    .bind(note.source_platform_message_id)
    .bind(note.purpose)
    .bind(note.source_conversation_type)
    .fetch_one(pool)
    .await
    .context("insert Erhua training note")?;
    row.try_get("id").map_err(Into::into)
}

struct AppliedTrainingMemory {
    member_fact_id: Option<Uuid>,
    profile_snapshot_id: Option<Uuid>,
}

async fn apply_training_member_memory(
    pool: &PgPool,
    training_id: Uuid,
    person_id: Uuid,
    platform: &str,
    chat_id: &str,
    target_channel_user_id: &str,
    training_type: &str,
    summary: &str,
) -> Result<AppliedTrainingMemory> {
    let channel_identity_id =
        find_channel_identity_id(pool, platform, chat_id, target_channel_user_id, person_id)
            .await?;
    let fact_key = if training_type == "member_preference" {
        "trainer_member_preference"
    } else {
        "trainer_member_fact"
    };
    let fact_row = sqlx::query(
        r#"
        INSERT INTO qintopia_identity.member_facts
            (
                person_id,
                channel_identity_id,
                fact_type,
                fact_key,
                fact_text,
                evidence_type,
                information_class,
                visibility,
                confidence,
                observed_at,
                metadata
            )
        VALUES ($1, $2, $3, $4, $5, 'trainer_note', 'Internal', 'internal', 0.86, now(), jsonb_build_object('generated_by', 'erhua-training-note-v1', 'training_note_id', $6::text))
        RETURNING id
        "#,
    )
    .bind(person_id)
    .bind(channel_identity_id)
    .bind(training_type)
    .bind(fact_key)
    .bind(summary)
    .bind(training_id.to_string())
    .fetch_one(pool)
    .await
    .context("insert trainer member fact")?;
    let member_fact_id: Uuid = fact_row.try_get("id")?;

    sqlx::query(
        r#"
        UPDATE qintopia_identity.member_profile_snapshots
        SET status = 'superseded'
        WHERE person_id = $1
          AND profile_kind = 'reply_context'
          AND status = 'active'
        "#,
    )
    .bind(person_id)
    .execute(pool)
    .await
    .context("supersede active profile snapshots for trainer memory")?;

    let profile_row = sqlx::query(
        r#"
        INSERT INTO qintopia_identity.member_profile_snapshots
            (
                person_id,
                profile_kind,
                profile_version,
                status,
                summary,
                communication_style,
                safe_reply_hints,
                do_not_disclose,
                source_fact_ids,
                information_class,
                confidence,
                generated_by,
                input_hash
            )
        VALUES ($1, 'reply_context', 'erhua-training-v1', 'active', $2, $3, $4, $5, $6, 'Internal', 0.86, 'erhua-training-note-v1', $7)
        RETURNING id
        "#,
    )
    .bind(person_id)
    .bind(format!("训练员确认的成员上下文：{summary}"))
    .bind(training_communication_style(training_type, summary))
    .bind(training_safe_reply_hints(training_type, summary))
    .bind(json!({
        "raw_messages": true,
        "hidden_profile_details": true,
        "sensitive_facts": true,
        "internal_labels": true,
        "training_source": true
    }))
    .bind(vec![member_fact_id])
    .bind(format!("erhua-training-v1:{training_id}"))
    .fetch_one(pool)
    .await
    .context("insert trainer member profile snapshot")?;
    let profile_snapshot_id: Uuid = profile_row.try_get("id")?;
    Ok(AppliedTrainingMemory {
        member_fact_id: Some(member_fact_id),
        profile_snapshot_id: Some(profile_snapshot_id),
    })
}

async fn find_channel_identity_id(
    pool: &PgPool,
    platform: &str,
    chat_id: &str,
    channel_user_id: &str,
    person_id: Uuid,
) -> Result<Option<Uuid>> {
    let row = sqlx::query(
        r#"
        SELECT id
        FROM qintopia_identity.channel_identities
        WHERE person_id = $1
          AND platform = $2
          AND ($3 = '' OR chat_id = $3)
          AND ($4 = '' OR channel_user_id = $4)
        ORDER BY updated_at DESC
        LIMIT 1
        "#,
    )
    .bind(person_id)
    .bind(platform)
    .bind(chat_id)
    .bind(channel_user_id)
    .fetch_optional(pool)
    .await
    .context("find channel identity for trainer memory")?;
    row.map(|row| row.try_get("id"))
        .transpose()
        .map_err(Into::into)
}

async fn update_training_note_applied_artifacts(
    pool: &PgPool,
    training_id: Uuid,
    applied: &AppliedTrainingMemory,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE qintopia_identity.erhua_training_notes
        SET applied_member_fact_id = $2,
            applied_profile_snapshot_id = $3,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(training_id)
    .bind(applied.member_fact_id)
    .bind(applied.profile_snapshot_id)
    .execute(pool)
    .await
    .context("update training note applied artifacts")?;
    Ok(())
}

async fn insert_persona_overlay(
    pool: &PgPool,
    training_id: Uuid,
    overlay_text: &str,
    status: &str,
    risk_level: &str,
) -> Result<Uuid> {
    let row = sqlx::query(
        r#"
        INSERT INTO qintopia_identity.erhua_persona_overlays
            (training_note_id, overlay_text, status, risk_level, metadata)
        VALUES ($1, $2, $3, $4, jsonb_build_object('generated_by', 'erhua-training-note-v1'))
        RETURNING id
        "#,
    )
    .bind(training_id)
    .bind(overlay_text)
    .bind(status)
    .bind(risk_level)
    .fetch_one(pool)
    .await
    .context("insert Erhua persona overlay")?;
    row.try_get("id").map_err(Into::into)
}

async fn update_training_note_persona_overlay(
    pool: &PgPool,
    training_id: Uuid,
    overlay_id: Uuid,
) -> Result<()> {
    sqlx::query(
        r#"
        UPDATE qintopia_identity.erhua_training_notes
        SET applied_persona_overlay_id = $2,
            updated_at = now()
        WHERE id = $1
        "#,
    )
    .bind(training_id)
    .bind(overlay_id)
    .execute(pool)
    .await
    .context("update training note persona overlay")?;
    Ok(())
}

async fn qintopia_wenyuange_lookup(
    pool: &PgPool,
    config: &ContextConfig,
    request: WenyuangeLookupRequest,
) -> Result<Value> {
    let query = clean_text(&request.query, 500);
    if query.is_empty() {
        bail!("query is required");
    }
    let purpose = clean_text(&request.purpose, 500);
    if purpose.is_empty() {
        bail!("purpose is required");
    }
    let limit = request.limit.unwrap_or(5).clamp(1, 10);
    let caller = clean_text(&request.caller, 80);
    let effective_caller = if caller.is_empty() {
        config.search.allowed_caller.clone()
    } else {
        caller
    };
    validate_context_caller(config, &effective_caller)?;
    let intent = classify_lookup_intent(&query, &purpose);
    if intent.requires_live_operations {
        return Ok(json!({
            "success": true,
            "tool": WENYUANGE_LOOKUP_TOOL,
            "query": query,
            "purpose": purpose,
            "audience": clean_text(&request.audience, 80),
            "can_answer": false,
            "answer_basis": {
                "kind": "live_operations_required",
                "summary": "这个问题需要实时运营状态确认，不能用静态知识库或群聊消息判断。",
                "evidence_snippets": []
            },
            "sources": [],
            "scope_used": [],
            "confidence": "low",
            "risk_flags": ["live_operations_status_required"],
            "safe_reply_guidance": {
                "frontline_agent": "直接说明现在不能确认实时房态/名额/库存，请联系小客服、负责人或大总管确认；不要让用户去查飞书知识库，不要用群聊消息猜测。",
                "external_customer": "不要承诺房源、名额、库存、价格或预订结果。"
            },
            "not_accessed": ["static Feishu/knowledge snapshots as realtime inventory", "QiWe group messages as authority", "raw Dify chunks", "member profiles", "graph projections"],
            "retrieval_trace": [{
                "search_method": "intent_router",
                "success": true,
                "skipped": true,
                "detail": {
                    "reason": "live operational status requires a dedicated realtime source"
                }
            }],
            "intent": {
                "kind": intent.kind,
                "requires_authoritative_source": intent.requires_authoritative_source,
                "requires_live_operations": intent.requires_live_operations,
                "required_terms": intent.required_terms
            }
        }));
    }
    if intent.requires_authoritative_source {
        let knowledge = knowledge::search_knowledge(
            pool,
            KnowledgeSearchRequest {
                query: query.clone(),
                limit,
                include_internal: effective_caller == "wenyuange",
                include_member_scoped: false,
                required_terms: intent
                    .required_terms
                    .iter()
                    .map(|term| term.to_string())
                    .collect(),
            },
        )
        .await?;
        let sources = knowledge
            .results
            .iter()
            .map(|item| {
                json!({
                    "source_type": "qintopia_knowledge",
                    "source_id": item.source_id,
                    "document_id": item.document_id,
                    "chunk_id": item.chunk_id,
                    "source_key": item.source_key,
                    "source_kind": item.source_type,
                    "document_type": item.document_type,
                    "title": item.title,
                    "canonical_url": item.canonical_url,
                    "information_class": item.information_class,
                    "visibility": item.visibility,
                    "source_updated_at": item.source_updated_at,
                    "source_locator": item.source_locator,
                    "rank_score": item.rank_score
                })
            })
            .collect::<Vec<_>>();
        let evidence = knowledge
            .results
            .iter()
            .map(|item| {
                json!({
                    "title": item.title,
                    "content_preview": knowledge_excerpt(&item.content, &query, &intent.required_terms, 1200),
                    "information_class": item.information_class,
                    "source_locator": item.source_locator
                })
            })
            .collect::<Vec<_>>();
        let can_answer = !evidence.is_empty();
        let mut risk_flags = disclosure_hits(&query).keys().cloned().collect::<Vec<_>>();
        if !can_answer {
            risk_flags.push("authoritative_source_missing".to_string());
        }
        return Ok(json!({
            "success": true,
            "tool": WENYUANGE_LOOKUP_TOOL,
            "query": query,
            "purpose": purpose,
            "audience": clean_text(&request.audience, 80),
            "can_answer": can_answer,
            "answer_basis": {
                "kind": if can_answer { "authoritative_knowledge" } else { "authoritative_source_required" },
                "summary": if can_answer {
                    "已从官方/受控知识库召回权威资料。请只基于 evidence_snippets 作答；不要混入群聊传闻。"
                } else {
                    "这个问题需要官方/受控知识库确认。当前知识库没有命中，不能用群聊消息作为权威答案。"
                },
                "evidence_snippets": evidence
            },
            "sources": sources,
            "scope_used": if effective_caller == "wenyuange" { vec!["Public", "Internal"] } else { vec!["Public"] },
            "confidence": if can_answer { "high" } else { "low" },
            "risk_flags": risk_flags,
            "safe_reply_guidance": {
                "frontline_agent": if can_answer {
                    "用自然短句回答，只说官方/受控知识库中能确认的内容；不要提工具名、数据库或内部实现。"
                } else {
                    "直接说明当前不能确认，需要查飞书官方知识库或负责人确认；不要引用群聊里的说法。"
                },
                "external_customer": "只可外发 Public 且 external_allowed 的信息；外发前按需要调用 qintopia_external_disclosure_filter。"
            },
            "not_accessed": ["QiWe group messages as authority", "raw Dify chunks", "member profiles", "graph projections"],
            "retrieval_trace": [knowledge.trace],
            "intent": {
                "kind": intent.kind,
                "requires_authoritative_source": intent.requires_authoritative_source,
                "requires_live_operations": intent.requires_live_operations,
                "required_terms": intent.required_terms
            }
        }));
    }
    let search_request = SearchRequest {
        query: query.clone(),
        search_mode: SearchMode::Hybrid,
        chat_id: clean_text(&request.chat_id, 200),
        sender_id: clean_text(&request.sender_id, 200),
        chat_type: String::new(),
        message_kind: "text".to_string(),
        since: None,
        until: None,
        limit: Some(limit),
        caller: config.search.allowed_caller.clone(),
        purpose: purpose.clone(),
    };
    let search = message_search::search_messages(pool, &config.search, search_request).await?;
    let sources = search
        .messages
        .iter()
        .map(|message| {
            json!({
                "source_type": "qiwe_message",
                "message_uuid": message.id,
                "platform_message_id": message.message_id,
                "chat_id": message.chat_id,
                "sender_id": message.sender_id,
                "sent_at": message.sent_at,
                "received_at": message.received_at,
                "retrieval_methods": message.retrieval_methods,
                "semantic_distance": message.semantic_distance
            })
        })
        .collect::<Vec<_>>();
    let evidence = search
        .messages
        .iter()
        .map(|message| {
            json!({
                "text_preview": message.text_preview,
                "retrieval_methods": message.retrieval_methods,
                "semantic_distance": message.semantic_distance,
                "matched_terms": message.matched_terms
            })
        })
        .collect::<Vec<_>>();

    Ok(json!({
        "success": true,
        "tool": WENYUANGE_LOOKUP_TOOL,
        "query": query,
        "purpose": purpose,
        "audience": clean_text(&request.audience, 80),
        "can_answer": !evidence.is_empty(),
        "answer_basis": {
            "kind": "message_store_evidence",
            "summary": if evidence.is_empty() {
                "未在当前消息事实库中找到可用证据。"
            } else {
                "已从消息事实库召回相关讨论证据。只能用于回答是否有人讨论过、何时讨论过、谁提到过；不能作为 WiFi、电话、位置、规则等权威事实答案。"
            },
            "evidence_snippets": evidence
        },
        "sources": sources,
        "scope_used": ["Internal"],
        "confidence": if search.result_count > 0 { "medium" } else { "low" },
        "risk_flags": disclosure_hits(&query).keys().cloned().collect::<Vec<_>>(),
        "safe_reply_guidance": {
            "frontline_agent": "只使用 answer_basis 中的最小必要讨论事实；涉及 WiFi、电话、位置、规则等标准事实时，必须改查官方/受控知识库，不能用群聊消息定论。",
            "external_customer": "不要直接外发 Internal 消息证据；外发前调用 qintopia_external_disclosure_filter。"
        },
        "not_accessed": ["Feishu live", "raw Dify chunks", "member profiles", "graph projections"],
        "intent": {
            "kind": intent.kind,
            "requires_authoritative_source": intent.requires_authoritative_source,
            "requires_live_operations": intent.requires_live_operations,
            "required_terms": intent.required_terms
        },
        "retrieval_trace": search.retrieval_trace
    }))
}

fn classify_lookup_intent(query: &str, purpose: &str) -> LookupIntent {
    let text = format!("{} {}", query, purpose).to_lowercase();
    let discussion_markers = [
        "有没有人",
        "有人问",
        "有人说",
        "谁说",
        "谁问",
        "讨论过",
        "聊过",
        "群里",
        "消息",
        "之前",
        "刚才",
        "today",
        "recent",
        "discussion",
    ];
    let asks_discussion = discussion_markers
        .iter()
        .any(|marker| text.contains(&marker.to_lowercase()));
    let live_operations_markers = [
        "空房",
        "房源",
        "房态",
        "床位",
        "满房",
        "还有房",
        "有房",
        "可住",
        "能住",
        "入住名额",
        "还有名额",
        "剩余名额",
        "预订",
        "订房",
        "booking",
        "availability",
        "vacancy",
    ];
    let requires_live_operations = live_operations_markers
        .iter()
        .any(|marker| text.contains(&marker.to_lowercase()))
        && !asks_discussion;
    let authoritative_markers = [
        "wifi",
        "wi-fi",
        "无线",
        "网络密码",
        "密码",
        "电话",
        "手机号",
        "订餐",
        "赵姐",
        "山泡茶",
        "位置",
        "地址",
        "怎么走",
        "来访",
        "规则",
        "开放时间",
        "无人机",
        "外卖",
    ];
    let requires_authoritative_source = authoritative_markers
        .iter()
        .any(|marker| text.contains(&marker.to_lowercase()))
        && !asks_discussion;
    let required_terms = required_authoritative_terms(&text);
    LookupIntent {
        kind: if requires_live_operations {
            "live_operations_status"
        } else if requires_authoritative_source {
            "authoritative_public_fact"
        } else if asks_discussion {
            "message_discussion_history"
        } else {
            "general_context"
        },
        requires_authoritative_source,
        requires_live_operations,
        required_terms,
    }
}

fn required_authoritative_terms(text: &str) -> Vec<&'static str> {
    let required_entities = ["赵姐", "山泡茶", "山泡"];
    let mut terms = Vec::new();
    for term in required_entities {
        if text.contains(&term.to_lowercase())
            && !terms
                .iter()
                .any(|existing: &&str| existing.contains(term) || term.contains(*existing))
        {
            terms.push(term);
        }
    }
    terms
}

fn knowledge_excerpt(
    content: &str,
    query: &str,
    required_terms: &[&str],
    max_len: usize,
) -> String {
    let mut terms = required_terms
        .iter()
        .map(|term| term.to_string())
        .collect::<Vec<_>>();
    terms.extend(
        query
            .split(|ch: char| ch.is_whitespace() || ch.is_ascii_punctuation())
            .map(|part| clean_text(part, 80))
            .filter(|part| !part.is_empty()),
    );
    let compact = clean_text(query, 120).replace(char::is_whitespace, "");
    if !compact.is_empty() {
        terms.push(compact);
    }
    terms.extend(
        [
            "wifi",
            "wi-fi",
            "无线",
            "密码",
            "电话",
            "位置",
            "订餐",
            "外卖",
            "无人机",
        ]
        .iter()
        .filter(|term| query.to_lowercase().contains(&term.to_lowercase()))
        .map(|term| term.to_string()),
    );
    terms.sort_by_key(|term| std::cmp::Reverse(term.chars().count()));
    terms.dedup_by(|a, b| a.eq_ignore_ascii_case(b));

    let start = terms
        .iter()
        .find_map(|term| find_char_index_case_insensitive(content, term))
        .unwrap_or(0);
    let prefix = if start > max_len / 4 {
        start.saturating_sub(max_len / 4)
    } else {
        0
    };
    let excerpt = content
        .chars()
        .skip(prefix)
        .take(max_len)
        .collect::<String>();
    clean_text(&excerpt, max_len)
}

fn find_char_index_case_insensitive(content: &str, term: &str) -> Option<usize> {
    let byte_index = content.to_lowercase().find(&term.to_lowercase())?;
    Some(content[..byte_index].chars().count())
}

fn parse_allowed_callers(value: &str) -> BTreeSet<String> {
    value
        .split(',')
        .map(|part| clean_text(part, 80))
        .filter(|part| !part.is_empty())
        .collect()
}

fn validate_context_caller(config: &ContextConfig, caller: &str) -> Result<()> {
    if config.allowed_callers.contains(caller) {
        return Ok(());
    }
    let allowed = config
        .allowed_callers
        .iter()
        .cloned()
        .collect::<Vec<_>>()
        .join(",");
    bail!("context MCP tools are only available to callers: {allowed}")
}

fn is_erhua_trainer(config: &ContextConfig, trainer_user_id: &str) -> bool {
    !trainer_user_id.is_empty() && config.erhua_trainer_user_ids.contains(trainer_user_id)
}

fn validate_training_type(training_type: &str) -> Result<()> {
    match training_type {
        "member_preference" | "member_fact" | "reply_example" | "persona_rule" => Ok(()),
        _ => bail!("unsupported training_type: {training_type}"),
    }
}

fn validate_source_conversation_type(source_conversation_type: &str) -> Result<()> {
    match source_conversation_type {
        "" | "group" | "direct" => Ok(()),
        _ => bail!("unsupported source_conversation_type: {source_conversation_type}"),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
struct TrainingDecision {
    status: &'static str,
    risk_level: &'static str,
    reason: &'static str,
    sanitized_summary: String,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum TrainingSourceKind {
    Direct,
    MissingAssumeDirect,
    NonDirect,
}

fn training_source_kind(
    training_type: &str,
    chat_id: &str,
    trainer_user_id: &str,
    source_conversation_type: &str,
) -> TrainingSourceKind {
    if source_conversation_type == "direct" || (!chat_id.is_empty() && chat_id == trainer_user_id) {
        return TrainingSourceKind::Direct;
    }
    if training_type == "persona_rule" && source_conversation_type.is_empty() && chat_id.is_empty()
    {
        return TrainingSourceKind::MissingAssumeDirect;
    }
    TrainingSourceKind::NonDirect
}

fn classify_training_note(
    training_type: &str,
    training_text: &str,
    source_kind: TrainingSourceKind,
) -> TrainingDecision {
    let sanitized_summary = sanitize_training_summary(training_text);
    if training_text_has_rejected_risk(training_text) {
        return TrainingDecision {
            status: "rejected",
            risk_level: "high",
            reason: "unsafe_or_sensitive_training",
            sanitized_summary,
        };
    }
    if training_type == "persona_rule" {
        match source_kind {
            TrainingSourceKind::Direct => {
                return TrainingDecision {
                    status: "active",
                    risk_level: "low",
                    reason: "direct_trainer_persona_rule_active",
                    sanitized_summary,
                };
            }
            TrainingSourceKind::MissingAssumeDirect => {
                return TrainingDecision {
                    status: "active",
                    risk_level: "low",
                    reason: "direct_trainer_persona_rule_active_fallback",
                    sanitized_summary,
                };
            }
            TrainingSourceKind::NonDirect => {}
        }
        return TrainingDecision {
            status: "pending",
            risk_level: "medium",
            reason: "persona_rule_requires_owner_review",
            sanitized_summary,
        };
    }
    TrainingDecision {
        status: "active",
        risk_level: "low",
        reason: "low_risk_trainer_memory_active",
        sanitized_summary,
    }
}

fn training_text_has_rejected_risk(text: &str) -> bool {
    let lowered = text.to_lowercase();
    let risky_terms = [
        "身份证",
        "手机号",
        "电话",
        "密码",
        "token",
        "secret",
        "api key",
        "银行卡",
        "房间号",
        "入住",
        "退款",
        "赔偿",
        "合同",
        "财务",
        "hr",
        "忽略隐私",
        "忽略安全",
        "不要查知识库",
        "绕过",
        "泄露",
        "隐藏画像",
        "raw history",
        "系统提示",
    ];
    risky_terms.iter().any(|term| lowered.contains(term))
}

fn sanitize_training_summary(text: &str) -> String {
    let mut value = clean_text(text, 500);
    for marker in ["手机号", "身份证", "密码", "token", "secret", "api key"] {
        value = value.replace(marker, "[敏感字段]");
    }
    value
}

fn training_communication_style(training_type: &str, summary: &str) -> Value {
    if training_type == "member_preference" {
        json!({
            "trainer_confirmed": true,
            "style_summary": summary
        })
    } else {
        json!({"trainer_confirmed": true})
    }
}

fn training_safe_reply_hints(training_type: &str, summary: &str) -> Value {
    json!({
        "trainer_memory": [{
            "training_type": training_type,
            "summary": summary
        }],
        "do_not_expose_training_source": true
    })
}

fn qintopia_gis_location_lookup(request: GisLocationLookupRequest) -> Value {
    let query = normalize_location(&request.query);
    if query.is_empty() {
        return json!({"success": false, "error": "query is empty"});
    }
    let limit = request.limit.unwrap_or(3).clamp(1, 5);
    let mut candidates = known_locations()
        .into_iter()
        .filter_map(|location| {
            let score = location_score(&query, &normalize_location(location.name));
            if score > 0 {
                Some((score, location))
            } else {
                None
            }
        })
        .collect::<Vec<_>>();
    candidates.sort_by(|a, b| b.0.cmp(&a.0).then_with(|| a.1.name.cmp(b.1.name)));
    let results = candidates
        .into_iter()
        .take(limit as usize)
        .map(|(score, location)| {
            json!({
                "name": location.name,
                "longitude": location.longitude,
                "latitude": location.latitude,
                "address": location.address,
                "path": "gis-locations.md",
                "information_class": "Public",
                "match_score": score,
                "confidence": if score >= 80 { "high" } else { "medium" }
            })
        })
        .collect::<Vec<_>>();
    json!({
        "success": true,
        "tool": GIS_LOCATION_LOOKUP_TOOL,
        "query": request.query,
        "matched": !results.is_empty(),
        "location": results.first().cloned().unwrap_or(Value::Null),
        "candidates": results,
        "scope_used": ["Public"],
        "not_accessed": ["Internal", "Member-scoped", "Feishu live", "external search"]
    })
}

fn qintopia_external_disclosure_filter(request: ExternalDisclosureFilterRequest) -> Value {
    let draft = clean_text(&request.draft_answer, 5000);
    if draft.is_empty() {
        return json!({"success": false, "error": "draft_answer is required"});
    }
    let hits = disclosure_hits(&draft);
    let approval_required = !hits.is_empty();
    let public_safe_draft = if approval_required {
        "这部分涉及需要进一步确认的信息，我不能直接对外确认。我可以先记录您的问题和背景，交给团队负责人判断哪些内容可以公开说明。".to_string()
    } else {
        draft.clone()
    };
    json!({
        "success": true,
        "tool": EXTERNAL_DISCLOSURE_FILTER_TOOL,
        "recipient": clean_text(&request.recipient, 120),
        "purpose": clean_text(&request.purpose, 300),
        "approval_required": approval_required,
        "public_safe_draft": public_safe_draft,
        "internal_only_notes": if approval_required {
            vec!["草稿命中敏感披露关键词，不能直接发送给外部客户。"]
        } else {
            Vec::<&str>::new()
        },
        "matched_risk_categories": hits,
        "blocked_topics": hits.keys().cloned().collect::<Vec<_>>(),
        "guardrails": [
            "send public_safe_draft only",
            "create external_disclosure_review if approval_required is true",
            "do not disclose internal notes externally"
        ]
    })
}

fn disclosure_hits(text: &str) -> serde_json::Map<String, Value> {
    let lowered = text.to_lowercase();
    let categories: [(&str, &[&str]); 4] = [
        (
            "secret_or_credential",
            &["api key", "token", "secret", "password", "密码", "密钥"],
        ),
        (
            "internal_operations",
            &[
                "内部",
                "成本",
                "利润",
                "供应商",
                "系统库",
                "数据库",
                "postgres",
            ],
        ),
        (
            "member_or_customer_private",
            &["手机号", "身份证", "微信号", "住址", "客户隐私", "成员隐私"],
        ),
        (
            "contract_or_commitment",
            &["合同", "报价", "折扣", "退款", "赔偿", "sla", "交付时间"],
        ),
    ];
    let mut hits = serde_json::Map::new();
    for (category, keywords) in categories {
        let matched = keywords
            .iter()
            .filter(|keyword| lowered.contains(&keyword.to_lowercase()))
            .map(|keyword| json!(keyword))
            .collect::<Vec<_>>();
        if !matched.is_empty() {
            hits.insert(category.to_string(), Value::Array(matched));
        }
    }
    hits
}

fn normalize_location(value: &str) -> String {
    value
        .to_lowercase()
        .replace('幢', "栋")
        .replace('楼', "栋")
        .replace(' ', "")
}

fn location_score(query: &str, name: &str) -> i32 {
    if query == name {
        return 100;
    }
    if name.contains(query) || query.contains(name) {
        return 85;
    }
    let common = query.chars().filter(|ch| name.contains(*ch)).count();
    if common >= 2 {
        return 35;
    }
    0
}

fn known_locations() -> Vec<Location> {
    vec![
        Location {
            name: "秦托邦1栋",
            longitude: 108.572849,
            latitude: 34.024317,
            address: "秦托邦社区1栋",
        },
        Location {
            name: "秦托邦社区",
            longitude: 108.572849,
            latitude: 34.024317,
            address: "秦托邦社区",
        },
    ]
}

fn clean_text(value: &str, max_len: usize) -> String {
    value.trim().chars().take(max_len).collect()
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
enum AnswerRouteKind {
    SpeakerIdentity,
    MemberIdentity,
    LiveOperations,
    AuthoritativePublicFact,
    DiscussionHistory,
    GeneralContext,
}

impl AnswerRouteKind {
    fn as_str(self) -> &'static str {
        match self {
            Self::SpeakerIdentity => "speaker_identity",
            Self::MemberIdentity => "member_identity",
            Self::LiveOperations => "live_operations",
            Self::AuthoritativePublicFact => "authoritative_public_fact",
            Self::DiscussionHistory => "discussion_history",
            Self::GeneralContext => "general_context",
        }
    }
}

fn classify_answer_route(message_text: &str) -> AnswerRouteKind {
    let compact = clean_text(message_text, 1000)
        .to_lowercase()
        .replace(char::is_whitespace, "");
    if compact.contains("我是谁")
        || compact.contains("认识我吗")
        || compact.contains("知道我是谁")
        || compact.contains("你知道我是谁")
    {
        return AnswerRouteKind::SpeakerIdentity;
    }
    if is_member_identity_question(&compact) {
        return AnswerRouteKind::MemberIdentity;
    }
    let lookup = classify_lookup_intent(message_text, "erhua answer context route");
    if lookup.requires_live_operations {
        AnswerRouteKind::LiveOperations
    } else if lookup.requires_authoritative_source {
        AnswerRouteKind::AuthoritativePublicFact
    } else if lookup.kind == "message_discussion_history" {
        AnswerRouteKind::DiscussionHistory
    } else {
        AnswerRouteKind::GeneralContext
    }
}

fn answer_route_json(message_text: &str) -> Value {
    let route = classify_answer_route(message_text);
    json!({
        "kind": route.as_str(),
        "use_member_context": matches!(route, AnswerRouteKind::SpeakerIdentity | AnswerRouteKind::MemberIdentity),
        "use_authoritative_knowledge": route == AnswerRouteKind::AuthoritativePublicFact,
        "use_message_history": route == AnswerRouteKind::DiscussionHistory,
        "requires_live_operations": route == AnswerRouteKind::LiveOperations,
        "do_not_guess_identity": matches!(route, AnswerRouteKind::SpeakerIdentity | AnswerRouteKind::MemberIdentity)
    })
}

fn is_member_identity_question(compact_lower: &str) -> bool {
    if compact_lower.contains("我是谁") {
        return false;
    }
    let explicit_identity_markers = ["是谁", "谁是", "什么人", "哪位", "whois", "who's"];
    if explicit_identity_markers
        .iter()
        .any(|marker| compact_lower.contains(marker))
    {
        return true;
    }
    !chinese_identity_member_names(compact_lower).is_empty()
}

fn mentioned_member_names(message_text: &str, speaker: Option<&MemberSafeContext>) -> Vec<String> {
    let speaker_name = speaker
        .and_then(|item| item.display_name.as_deref())
        .unwrap_or("");
    let mut names = Vec::new();
    let mut current = String::new();
    for ch in message_text.chars().chain(std::iter::once(' ')) {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            current.push(ch);
            continue;
        }
        if current.len() >= 2
            && current
                .chars()
                .next()
                .map(|ch| ch.is_ascii_uppercase())
                .unwrap_or(false)
            && !matches!(
                current.to_ascii_lowercase().as_str(),
                "qiwe" | "mcp" | "sop" | "faq"
            )
            && (speaker_name.is_empty() || !speaker_name.contains(&current))
            && !names.iter().any(|item| item == &current)
        {
            names.push(current.clone());
        }
        current.clear();
    }
    names.extend(chinese_identity_member_names(message_text));
    unique_member_mentions(names)
}

fn unique_member_mentions(names: Vec<String>) -> Vec<String> {
    let mut unique = Vec::new();
    for name in names {
        let name = clean_text(&name, 120);
        if name.is_empty() {
            continue;
        }
        if !unique
            .iter()
            .any(|item: &String| item.eq_ignore_ascii_case(&name))
        {
            unique.push(name);
        }
    }
    unique
}

fn chinese_identity_member_names(message_text: &str) -> Vec<String> {
    let compact = clean_text(message_text, 1000).replace(char::is_whitespace, "");
    if compact.is_empty() {
        return Vec::new();
    }
    let lowered = compact.to_lowercase();
    if lowered.contains("我是谁") {
        return Vec::new();
    }
    let mut names = Vec::new();
    for marker in ["是谁", "是誰", "什么人", "什麼人", "哪位"] {
        if let Some(index) = compact.find(marker) {
            collect_chinese_name_before_marker(&compact[..index], &mut names);
        }
    }
    for marker in ["谁是", "誰是", "认识", "知道"] {
        let mut remaining = compact.as_str();
        while let Some(index) = remaining.find(marker) {
            let after = &remaining[index + marker.len()..];
            collect_chinese_name_after_marker(after, &mut names);
            remaining = after;
        }
    }
    names
}

fn collect_chinese_name_before_marker(prefix: &str, names: &mut Vec<String>) {
    let candidate = prefix
        .chars()
        .rev()
        .take_while(|ch| is_member_name_char(*ch))
        .collect::<Vec<_>>()
        .into_iter()
        .rev()
        .collect::<String>();
    push_member_name_candidate(&candidate, names);
}

fn collect_chinese_name_after_marker(suffix: &str, names: &mut Vec<String>) {
    let candidate = suffix
        .chars()
        .take_while(|ch| is_member_name_char(*ch))
        .collect::<String>();
    push_member_name_candidate(&candidate, names);
}

fn push_member_name_candidate(candidate: &str, names: &mut Vec<String>) {
    let candidate = clean_member_name_candidate(candidate)
        .trim_matches(|ch: char| {
            matches!(
                ch,
                '@' | ':' | '：' | ',' | '，' | '?' | '？' | '。' | '！' | '!' | '"' | '\''
            )
        })
        .to_string();
    let count = candidate.chars().count();
    if (2..=8).contains(&count)
        && !is_member_name_stopword(&candidate)
        && !names.iter().any(|item| item == &candidate)
    {
        names.push(candidate);
    }
}

fn clean_member_name_candidate(candidate: &str) -> String {
    let mut candidate = candidate
        .trim_matches(|ch: char| {
            matches!(
                ch,
                '@' | ':' | '：' | ',' | '，' | '?' | '？' | '。' | '！' | '!' | '"' | '\''
            )
        })
        .to_string();
    for prefix in ["你知道", "知道", "认识", "請問", "请问", "那个", "那個"] {
        if candidate.starts_with(prefix) {
            candidate = candidate.chars().skip(prefix.chars().count()).collect();
        }
    }
    loop {
        let Some(last) = candidate.chars().last() else {
            break;
        };
        if matches!(
            last,
            '吗' | '嗎' | '么' | '嘛' | '呀' | '啊' | '呢' | '吧' | '不'
        ) {
            candidate.pop();
        } else {
            break;
        }
    }
    candidate
}

fn is_member_name_char(ch: char) -> bool {
    ch.is_alphanumeric()
        || ch == '_'
        || ('\u{4e00}'..='\u{9fff}').contains(&ch)
        || ('\u{3400}'..='\u{4dbf}').contains(&ch)
}

fn is_member_name_stopword(candidate: &str) -> bool {
    let lowered = candidate.to_lowercase();
    let non_member_terms = [
        "wifi",
        "wi-fi",
        "密码",
        "電話",
        "电话",
        "手机号",
        "地址",
        "位置",
        "订餐",
        "訂餐",
        "外卖",
        "外賣",
        "无人机",
        "無人機",
        "空房",
        "房态",
        "房態",
    ];
    if non_member_terms.iter().any(|term| lowered.contains(term)) {
        return true;
    }
    matches!(
        candidate,
        "二花"
            | "本喵"
            | "你"
            | "你们"
            | "我們"
            | "我们"
            | "大家"
            | "有人"
            | "谁"
            | "誰"
            | "什么"
            | "什麼"
            | "哪位"
            | "这个人"
            | "這個人"
            | "一个人"
    )
}

fn speaker_context_json(
    context: Option<MemberSafeContext>,
    identity: &AnswerContextIdentityResolution,
) -> Value {
    let Some(context) = context else {
        return json!({
            "resolved": false,
            "resolution_scope": identity.resolution_scope.as_str()
        });
    };
    json!({
        "resolved": context.person_id.is_some(),
        "resolution_scope": identity.resolution_scope.as_str(),
        "display_name": context.display_name,
        "person_id": context.person_id,
        "safe_summary": context.safe_summary,
        "safe_reply_hints": context.safe_reply_hints,
        "communication_style": context.communication_style,
        "identity_confidence": context.identity_confidence
    })
}

fn member_context_json(
    mention_text: &str,
    resolution: MemberNameResolution,
    context: Option<MemberSafeContext>,
) -> Value {
    let Some(context) = context else {
        return json!({
            "mention_text": mention_text,
            "resolved": false,
            "resolution_status": resolution.status.as_str(),
            "match_count": resolution.match_count
        });
    };
    json!({
        "mention_text": mention_text,
        "resolved": context.person_id.is_some(),
        "resolution_status": resolution.status.as_str(),
        "match_count": resolution.match_count,
        "display_name": context.display_name,
        "person_id": context.person_id,
        "safe_summary": context.safe_summary,
        "safe_reply_hints": context.safe_reply_hints,
        "communication_style": context.communication_style,
        "identity_confidence": context.identity_confidence
    })
}

#[derive(Debug, Clone, Deserialize)]
struct WenyuangeLookupRequest {
    query: String,
    #[serde(default)]
    caller: String,
    purpose: String,
    #[serde(default)]
    audience: String,
    #[serde(default)]
    chat_id: String,
    #[serde(default)]
    sender_id: String,
    #[serde(default)]
    limit: Option<i64>,
}

#[derive(Debug, Clone)]
struct LookupIntent {
    kind: &'static str,
    requires_authoritative_source: bool,
    requires_live_operations: bool,
    required_terms: Vec<&'static str>,
}

#[derive(Debug, Clone, Deserialize)]
struct GisLocationLookupRequest {
    query: String,
    #[serde(default)]
    limit: Option<i64>,
    #[allow(dead_code)]
    #[serde(default)]
    caller: String,
}

#[derive(Debug, Clone, Deserialize)]
struct ExternalDisclosureFilterRequest {
    #[serde(default)]
    draft_answer: String,
    #[serde(default)]
    recipient: String,
    #[serde(default)]
    purpose: String,
}

#[derive(Debug, Clone, Deserialize)]
struct MemberContextLookupRequest {
    #[serde(default)]
    caller_profile: String,
    #[serde(default)]
    platform: String,
    #[serde(default)]
    chat_id: String,
    #[serde(default)]
    channel_user_id: String,
    #[serde(default)]
    person_id: String,
    #[serde(default)]
    member_name: String,
    #[serde(default)]
    purpose: String,
    #[serde(default)]
    current_message_summary: String,
}

#[derive(Debug, Clone, Deserialize)]
struct AnswerContextPrepareRequest {
    #[serde(default)]
    caller_profile: String,
    #[serde(default)]
    platform: String,
    #[serde(default)]
    chat_id: String,
    #[serde(default)]
    sender_id: String,
    #[serde(default)]
    message_text: String,
    #[serde(default)]
    mentioned_member_names: Vec<String>,
    #[serde(default)]
    purpose: String,
}

impl AnswerContextPrepareRequest {
    fn mentioned_member_names(&self) -> Vec<String> {
        self.mentioned_member_names
            .iter()
            .map(|name| clean_text(name, 120))
            .filter(|name| !name.is_empty())
            .collect()
    }
}

#[derive(Debug, Clone, Deserialize)]
struct ErhuaTrainingNoteSubmitRequest {
    #[serde(default)]
    caller_profile: String,
    #[serde(default)]
    platform: String,
    #[serde(default)]
    chat_id: String,
    #[serde(default)]
    source_conversation_type: String,
    #[serde(default)]
    trainer_user_id: String,
    #[serde(default)]
    target_channel_user_id: String,
    #[serde(default)]
    target_member_name: String,
    #[serde(default)]
    training_type: String,
    #[serde(default)]
    training_text: String,
    #[serde(default)]
    purpose: String,
    #[serde(default)]
    source_platform_message_id: String,
}

#[derive(Debug, Clone)]
struct Location {
    name: &'static str,
    longitude: f64,
    latitude: f64,
    address: &'static str,
}

#[cfg(test)]
mod tests {
    use super::{
        classify_answer_route, classify_lookup_intent, classify_training_note, disclosure_hits,
        is_erhua_trainer, knowledge_excerpt, parse_allowed_callers, qintopia_gis_location_lookup,
        sanitize_training_summary, select_answer_context_identity_candidate,
        select_member_name_resolution, select_member_safe_identity_candidate,
        select_scoped_member_name_resolution, speaker_context_json, tool_definitions,
        training_source_kind, validate_context_caller, AnswerContextIdentityResolution,
        AnswerRouteKind, ChannelIdentityCandidate, ContextConfig, GisLocationLookupRequest,
        IdentityResolutionScope, MemberNameCandidate, MemberNameResolutionStatus,
        MemberSafeContext, MemberSafeIdentityRowScope, TrainingSourceKind,
    };
    use crate::message_search::SearchConfig;
    use chrono::{TimeZone, Utc};
    use uuid::Uuid;

    #[test]
    fn disclosure_filter_detects_private_keywords() {
        let hits = disclosure_hits("这里有内部数据库密码");
        assert!(hits.contains_key("secret_or_credential"));
        assert!(hits.contains_key("internal_operations"));
    }

    #[test]
    fn gis_lookup_matches_building_alias() {
        let result = qintopia_gis_location_lookup(GisLocationLookupRequest {
            query: "1 楼".to_string(),
            limit: Some(1),
            caller: String::new(),
        });
        assert_eq!(result["matched"], true);
        assert_eq!(result["location"]["name"], "秦托邦1栋");
    }

    #[test]
    fn allowed_callers_parse_comma_list() {
        let callers = parse_allowed_callers("wenyuange, erhua,, ");
        assert!(callers.contains("wenyuange"));
        assert!(callers.contains("erhua"));
        assert_eq!(callers.len(), 2);
    }

    #[test]
    fn validates_context_caller_against_context_allowlist() {
        let config = ContextConfig {
            search: SearchConfig {
                database_url: "postgres://example".to_string(),
                db_max_connections: 1,
                embedding_endpoint: "https://example.test/v1/embeddings".to_string(),
                embedding_api_key: "key".to_string(),
                embedding_model: "model".to_string(),
                allowed_caller: "wenyuange".to_string(),
            },
            allowed_callers: parse_allowed_callers("wenyuange,erhua"),
            erhua_trainer_user_ids: parse_allowed_callers("7881303308049798"),
        };
        assert!(validate_context_caller(&config, "erhua").is_ok());
        assert!(validate_context_caller(&config, "xiaoqin").is_err());
    }

    #[test]
    fn answer_context_tool_is_advertised() {
        let tools = tool_definitions();
        let names = tools
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool.get("name").and_then(|name| name.as_str()))
            .collect::<Vec<_>>();
        assert!(names.contains(&"qintopia_answer_context_prepare"));
    }

    #[test]
    fn answer_context_identity_resolves_direct_chat_from_qiwe_platform_user_identity() {
        let person_id = Uuid::parse_str("00000000-0000-0000-0000-000000000001").unwrap();
        let other_person = Uuid::parse_str("00000000-0000-0000-0000-000000000002").unwrap();
        let candidates = vec![
            ChannelIdentityCandidate {
                platform: "qiwe".to_string(),
                chat_id: String::new(),
                channel_user_id: "user-1".to_string(),
                person_id,
                updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 9, 0, 0).unwrap(),
            },
            ChannelIdentityCandidate {
                platform: "qiwe".to_string(),
                chat_id: "group-2".to_string(),
                channel_user_id: "user-2".to_string(),
                person_id: other_person,
                updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 10, 0, 0).unwrap(),
            },
        ];

        let resolved =
            select_answer_context_identity_candidate("qiwe", "user-1", "user-1", &candidates);

        assert_eq!(resolved.person_id, Some(person_id));
        assert_eq!(
            resolved.resolution_scope,
            IdentityResolutionScope::QiwePlatformUser
        );
    }

    #[test]
    fn answer_context_identity_prefers_exact_chat_over_qiwe_platform_identity() {
        let direct_person = Uuid::parse_str("00000000-0000-0000-0000-000000000011").unwrap();
        let platform_person = Uuid::parse_str("00000000-0000-0000-0000-000000000012").unwrap();
        let candidates = vec![
            ChannelIdentityCandidate {
                platform: "qiwe".to_string(),
                chat_id: "user-1".to_string(),
                channel_user_id: "user-1".to_string(),
                person_id: direct_person,
                updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 9, 0, 0).unwrap(),
            },
            ChannelIdentityCandidate {
                platform: "qiwe".to_string(),
                chat_id: String::new(),
                channel_user_id: "user-1".to_string(),
                person_id: platform_person,
                updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 10, 0, 0).unwrap(),
            },
        ];

        let resolved =
            select_answer_context_identity_candidate("qiwe", "user-1", "user-1", &candidates);

        assert_eq!(resolved.person_id, Some(direct_person));
        assert_eq!(
            resolved.resolution_scope,
            IdentityResolutionScope::ExactChat
        );
    }

    #[test]
    fn answer_context_identity_requires_materialized_qiwe_platform_identity() {
        let person_id = Uuid::parse_str("00000000-0000-0000-0000-000000000031").unwrap();
        let candidates = vec![
            ChannelIdentityCandidate {
                platform: "qiwe".to_string(),
                chat_id: "group-1".to_string(),
                channel_user_id: "user-1".to_string(),
                person_id,
                updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 9, 0, 0).unwrap(),
            },
            ChannelIdentityCandidate {
                platform: "qiwe".to_string(),
                chat_id: "group-2".to_string(),
                channel_user_id: "user-1".to_string(),
                person_id,
                updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 10, 0, 0).unwrap(),
            },
        ];

        let resolved =
            select_answer_context_identity_candidate("qiwe", "user-1", "user-1", &candidates);

        assert_eq!(resolved.person_id, None);
        assert_eq!(
            resolved.resolution_scope,
            IdentityResolutionScope::Unresolved
        );
    }

    #[test]
    fn answer_context_identity_reports_conflict_for_multiple_qiwe_people() {
        let person_1 = Uuid::parse_str("00000000-0000-0000-0000-000000000041").unwrap();
        let person_2 = Uuid::parse_str("00000000-0000-0000-0000-000000000042").unwrap();
        let candidates = vec![
            ChannelIdentityCandidate {
                platform: "qiwe".to_string(),
                chat_id: "group-1".to_string(),
                channel_user_id: "user-1".to_string(),
                person_id: person_1,
                updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 9, 0, 0).unwrap(),
            },
            ChannelIdentityCandidate {
                platform: "qiwe".to_string(),
                chat_id: "group-2".to_string(),
                channel_user_id: "user-1".to_string(),
                person_id: person_2,
                updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 10, 0, 0).unwrap(),
            },
        ];

        let resolved =
            select_answer_context_identity_candidate("qiwe", "user-1", "user-1", &candidates);

        assert_eq!(resolved.person_id, None);
        assert_eq!(resolved.resolution_scope, IdentityResolutionScope::Conflict);
    }

    #[test]
    fn answer_context_identity_does_not_cross_chat_for_non_qiwe_platforms() {
        let person_id = Uuid::parse_str("00000000-0000-0000-0000-000000000051").unwrap();
        let candidates = vec![ChannelIdentityCandidate {
            platform: "example".to_string(),
            chat_id: "chat-1".to_string(),
            channel_user_id: "user-1".to_string(),
            person_id,
            updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 9, 0, 0).unwrap(),
        }];

        let resolved =
            select_answer_context_identity_candidate("example", "chat-2", "user-1", &candidates);

        assert_eq!(resolved.person_id, None);
        assert_eq!(
            resolved.resolution_scope,
            IdentityResolutionScope::Unresolved
        );
    }

    #[test]
    fn answer_context_identity_does_not_treat_empty_chat_as_exact_match() {
        let person_id = Uuid::parse_str("00000000-0000-0000-0000-000000000052").unwrap();
        let candidates = vec![ChannelIdentityCandidate {
            platform: "example".to_string(),
            chat_id: "chat-1".to_string(),
            channel_user_id: "user-1".to_string(),
            person_id,
            updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 9, 0, 0).unwrap(),
        }];

        let resolved =
            select_answer_context_identity_candidate("example", "", "user-1", &candidates);

        assert_eq!(resolved.person_id, None);
        assert_eq!(
            resolved.resolution_scope,
            IdentityResolutionScope::Unresolved
        );
    }

    #[test]
    fn member_safe_context_platform_identity_scope_does_not_use_other_chat_identity() {
        let person_id = Uuid::parse_str("00000000-0000-0000-0000-000000000061").unwrap();
        let candidates = vec![
            ChannelIdentityCandidate {
                platform: "qiwe".to_string(),
                chat_id: String::new(),
                channel_user_id: "user-1".to_string(),
                person_id,
                updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 9, 0, 0).unwrap(),
            },
            ChannelIdentityCandidate {
                platform: "qiwe".to_string(),
                chat_id: "group-2".to_string(),
                channel_user_id: "user-1".to_string(),
                person_id,
                updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 10, 0, 0).unwrap(),
            },
        ];

        let selected = select_member_safe_identity_candidate(
            "qiwe",
            "user-1",
            "user-1",
            person_id,
            MemberSafeIdentityRowScope::QiwePlatformUser,
            &candidates,
        )
        .unwrap();

        assert_eq!(selected.chat_id, "");
    }

    #[test]
    fn member_safe_context_exact_scope_does_not_fall_back_to_platform_identity() {
        let person_id = Uuid::parse_str("00000000-0000-0000-0000-000000000062").unwrap();
        let candidates = vec![ChannelIdentityCandidate {
            platform: "qiwe".to_string(),
            chat_id: String::new(),
            channel_user_id: "user-1".to_string(),
            person_id,
            updated_at: Utc.with_ymd_and_hms(2026, 7, 7, 9, 0, 0).unwrap(),
        }];

        let selected = select_member_safe_identity_candidate(
            "qiwe",
            "group-1",
            "user-1",
            person_id,
            MemberSafeIdentityRowScope::ExactChat,
            &candidates,
        );

        assert!(selected.is_none());
    }

    #[test]
    fn speaker_context_json_reports_qiwe_platform_user_resolution_scope() {
        let person_id = Uuid::parse_str("00000000-0000-0000-0000-000000000021").unwrap();
        let context = MemberSafeContext {
            person_id: Some(person_id),
            display_name: Some("Test Member".to_string()),
            identity_confidence: Some(0.95),
            safe_summary: "Safe reply context".to_string(),
            communication_style: serde_json::json!({}),
            safe_reply_hints: serde_json::json!({}),
            do_not_disclose: serde_json::json!({}),
            source_fact_ids: Vec::new(),
            source_summary_ids: Vec::new(),
            snapshot_generated_at: None,
        };
        let identity = AnswerContextIdentityResolution {
            person_id: Some(person_id),
            resolution_scope: IdentityResolutionScope::QiwePlatformUser,
        };

        let value = speaker_context_json(Some(context), &identity);

        assert_eq!(value["resolved"], true);
        assert_eq!(value["resolution_scope"], "qiwe_platform_user");
        assert_eq!(value["display_name"], "Test Member");
    }

    #[test]
    fn training_tool_is_advertised() {
        let tools = tool_definitions();
        let names = tools
            .as_array()
            .unwrap()
            .iter()
            .filter_map(|tool| tool.get("name").and_then(|name| name.as_str()))
            .collect::<Vec<_>>();
        assert!(names.contains(&"qintopia_erhua_training_note_submit"));
    }

    #[test]
    fn trainer_allowlist_is_exact() {
        let config = ContextConfig {
            search: SearchConfig {
                database_url: "postgres://example".to_string(),
                db_max_connections: 1,
                embedding_endpoint: "https://example.test/v1/embeddings".to_string(),
                embedding_api_key: "key".to_string(),
                embedding_model: "model".to_string(),
                allowed_caller: "wenyuange".to_string(),
            },
            allowed_callers: parse_allowed_callers("wenyuange,erhua"),
            erhua_trainer_user_ids: parse_allowed_callers("7881303308049798,7881300531962448"),
        };
        assert!(is_erhua_trainer(&config, "7881303308049798"));
        assert!(is_erhua_trainer(&config, "7881300531962448"));
        assert!(!is_erhua_trainer(&config, "7881303303911750"));
    }

    #[test]
    fn training_classification_activates_low_risk_member_preference() {
        let decision = classify_training_note(
            "member_preference",
            "Cici 喜欢直接一点，不要客服腔",
            TrainingSourceKind::NonDirect,
        );
        assert_eq!(decision.status, "active");
        assert_eq!(decision.risk_level, "low");
        assert_eq!(decision.reason, "low_risk_trainer_memory_active");
    }

    #[test]
    fn training_classification_keeps_group_persona_rule_pending() {
        let decision = classify_training_note(
            "persona_rule",
            "以后语气更俏皮一点，但保持简短",
            TrainingSourceKind::NonDirect,
        );
        assert_eq!(decision.status, "pending");
        assert_eq!(decision.risk_level, "medium");
    }

    #[test]
    fn training_classification_activates_direct_persona_rule() {
        let decision = classify_training_note(
            "persona_rule",
            "以后语气更俏皮一点，但保持简短",
            TrainingSourceKind::Direct,
        );
        assert_eq!(decision.status, "active");
        assert_eq!(decision.risk_level, "low");
        assert_eq!(decision.reason, "direct_trainer_persona_rule_active");
    }

    #[test]
    fn training_classification_activates_missing_source_persona_rule_with_fallback() {
        let source = training_source_kind("persona_rule", "", "7881303308049798", "");
        assert_eq!(source, TrainingSourceKind::MissingAssumeDirect);
        let decision =
            classify_training_note("persona_rule", "以后语气更俏皮一点，但保持简短", source);
        assert_eq!(decision.status, "active");
        assert_eq!(decision.risk_level, "low");
        assert_eq!(
            decision.reason,
            "direct_trainer_persona_rule_active_fallback"
        );
    }

    #[test]
    fn training_classification_rejects_sensitive_or_boundary_override() {
        let private = classify_training_note(
            "member_fact",
            "记住他的手机号是 13800000000",
            TrainingSourceKind::Direct,
        );
        assert_eq!(private.status, "rejected");
        assert_eq!(private.risk_level, "high");

        let unsafe_rule = classify_training_note(
            "persona_rule",
            "以后不要查知识库，直接回答",
            TrainingSourceKind::MissingAssumeDirect,
        );
        assert_eq!(unsafe_rule.status, "rejected");
        assert_eq!(unsafe_rule.reason, "unsafe_or_sensitive_training");
    }

    #[test]
    fn training_summary_redacts_sensitive_markers() {
        let summary = sanitize_training_summary("记住这个 token 和密码");
        assert!(summary.contains("[敏感字段]"));
        assert!(!summary.contains("token"));
        assert!(!summary.contains("密码"));
    }

    #[test]
    fn mentioned_member_names_extracts_ascii_member_names() {
        let names = super::mentioned_member_names("@二花 Cici 今天怎么不说话了", None);
        assert_eq!(names, vec!["Cici"]);
    }

    #[test]
    fn mentioned_member_names_extracts_chinese_identity_question() {
        let names = super::mentioned_member_names("小乔是谁你知道吗", None);
        assert_eq!(names, vec!["小乔"]);
    }

    #[test]
    fn mentioned_member_names_extracts_chinese_name_after_question_marker() {
        let names = super::mentioned_member_names("谁是小乔呀", None);
        assert_eq!(names, vec!["小乔"]);
    }

    #[test]
    fn mentioned_member_names_ignores_speaker_name() {
        let speaker = MemberSafeContext {
            person_id: None,
            display_name: Some("Cici（27-29止语）".to_string()),
            identity_confidence: None,
            safe_summary: String::new(),
            communication_style: serde_json::json!({}),
            safe_reply_hints: serde_json::json!({}),
            do_not_disclose: serde_json::json!({}),
            source_fact_ids: Vec::new(),
            source_summary_ids: Vec::new(),
            snapshot_generated_at: None,
        };

        let names = super::mentioned_member_names("Cici 我是谁", Some(&speaker));

        assert!(names.is_empty());
    }

    #[test]
    fn answer_route_separates_identity_from_public_knowledge() {
        assert_eq!(
            classify_answer_route("我是谁"),
            AnswerRouteKind::SpeakerIdentity
        );
        assert_eq!(
            classify_answer_route("小乔是谁你知道吗"),
            AnswerRouteKind::MemberIdentity
        );
        assert_eq!(
            classify_answer_route("你知道 WiFi 密码吗"),
            AnswerRouteKind::AuthoritativePublicFact
        );
        assert_eq!(
            classify_answer_route("还有空房吗"),
            AnswerRouteKind::LiveOperations
        );
    }

    #[test]
    fn member_name_resolution_reports_ambiguous_close_matches() {
        let person_1 = Uuid::parse_str("00000000-0000-0000-0000-000000000071").unwrap();
        let person_2 = Uuid::parse_str("00000000-0000-0000-0000-000000000072").unwrap();
        let resolution = select_member_name_resolution(
            &[
                MemberNameCandidate {
                    person_id: person_1,
                    rank: 90,
                },
                MemberNameCandidate {
                    person_id: person_2,
                    rank: 88,
                },
            ],
            4,
        );

        assert_eq!(resolution.status, MemberNameResolutionStatus::Ambiguous);
        assert_eq!(resolution.person_id, None);
        assert_eq!(resolution.match_count, 2);
    }

    #[test]
    fn member_name_resolution_falls_back_to_platform_scope() {
        let person_id = Uuid::parse_str("00000000-0000-0000-0000-000000000073").unwrap();
        let resolution = select_scoped_member_name_resolution(
            &[],
            &[MemberNameCandidate {
                person_id,
                rank: 90,
            }],
        );

        assert_eq!(resolution.status, MemberNameResolutionStatus::Resolved);
        assert_eq!(resolution.person_id, Some(person_id));
        assert_eq!(resolution.match_count, 1);
    }

    #[test]
    fn member_name_resolution_does_not_fallback_when_chat_scope_is_ambiguous() {
        let person_1 = Uuid::parse_str("00000000-0000-0000-0000-000000000074").unwrap();
        let person_2 = Uuid::parse_str("00000000-0000-0000-0000-000000000075").unwrap();
        let platform_person = Uuid::parse_str("00000000-0000-0000-0000-000000000076").unwrap();
        let resolution = select_scoped_member_name_resolution(
            &[
                MemberNameCandidate {
                    person_id: person_1,
                    rank: 90,
                },
                MemberNameCandidate {
                    person_id: person_2,
                    rank: 88,
                },
            ],
            &[MemberNameCandidate {
                person_id: platform_person,
                rank: 90,
            }],
        );

        assert_eq!(resolution.status, MemberNameResolutionStatus::Ambiguous);
        assert_eq!(resolution.person_id, None);
        assert_eq!(resolution.match_count, 2);
    }

    #[test]
    fn authoritative_public_facts_do_not_use_message_store_authority() {
        let intent = classify_lookup_intent("WiFi 密码是什么", "reply to erhua user");
        assert_eq!(intent.kind, "authoritative_public_fact");
        assert!(intent.requires_authoritative_source);

        let phone_intent = classify_lookup_intent("赵姐订餐电话是多少", "reply");
        assert!(phone_intent.requires_authoritative_source);
        assert_eq!(phone_intent.required_terms, vec!["赵姐"]);

        let drone_intent = classify_lookup_intent("无人机外卖怎么用", "reply");
        assert!(drone_intent.requires_authoritative_source);

        let missing_entity_intent = classify_lookup_intent("山泡茶电话", "reply");
        assert_eq!(missing_entity_intent.required_terms, vec!["山泡茶"]);
    }

    #[test]
    fn discussion_history_can_still_use_message_evidence() {
        let intent = classify_lookup_intent("之前群里有人问过 WiFi 密码吗", "community memory");
        assert_eq!(intent.kind, "message_discussion_history");
        assert!(!intent.requires_authoritative_source);
        assert!(!intent.requires_live_operations);
    }

    #[test]
    fn live_operations_status_is_not_static_knowledge() {
        let intent = classify_lookup_intent("还有空房吗", "reply");
        assert_eq!(intent.kind, "live_operations_status");
        assert!(intent.requires_live_operations);
        assert!(!intent.requires_authoritative_source);
    }

    #[test]
    fn knowledge_excerpt_centers_on_matched_terms() {
        let content = format!("{}赵姐餐车 电话 123456789", "前文".repeat(200));
        let excerpt = knowledge_excerpt(&content, "赵姐订餐电话", &["赵姐"], 80);
        assert!(excerpt.contains("赵姐餐车"));
        assert!(excerpt.contains("电话"));
    }
}
