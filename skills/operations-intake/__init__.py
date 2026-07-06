"""Controlled operations intake tools for Qintopia Agent OS."""

from __future__ import annotations

import hashlib
import json
import os
import re
import textwrap
from typing import Any, Callable


QINTOPIA_TENANT = "qintopia"
COMPLAINT_TASK_TYPE = "complaint_intake"
COMPLAINT_OWNER_PROFILE = "default"
COMPLAINT_BOARD = "default"
SALES_OWNER_PROFILE = "xiaoqin"
SALES_BOARD = "default"
SALES_TASK_TYPES = {
    "sales_lead": {"status": "triage", "label": "商务线索"},
    "demo_request": {"status": "triage", "label": "演示请求"},
    "proposal": {"status": "todo", "label": "方案提案"},
    "external_disclosure_review": {"status": "review", "label": "对外披露复核"},
}
SALES_SOURCE_CHANNELS = ["wechat_external", "wecom_external", "feishu_external", "manual"]
PUBLIC_AGENT_OS_BASELINES = [
    "Qintopia Agent OS 是面向组织协作场景的 Agent 工作系统。",
    "系统可以把客户需求、知识检索、方案草拟、演示准备、任务流转和交接沉淀到可追踪流程中。",
    "系统强调可控披露、任务可追踪、人工审批和多角色协作。",
    "报价、合同、正式交付范围、排期、SLA 和客户案例细节需要团队负责人确认。",
]
XIAOQIN_SAFE_FOLLOWUP_MESSAGE = "我先帮您记录下来，稍后由团队同事继续跟进确认。"
DEFAULT_KB_LIMIT = 5
MAX_KB_LIMIT = 10

_KANBAN_CREATE_COMPLAINT: Callable[[str, str, int, str], tuple[str | None, str]] | None = None
_KANBAN_ADD_COMPLAINT_COMMENT: Callable[[str, str], tuple[int | None, str]] | None = None
_KANBAN_CREATE_SALES_TASK: Callable[[str, str, str, int, str], tuple[str | None, str]] | None = None
_KB_SEARCH_HANDLER: Callable[[dict[str, Any]], str] | None = None


def configure_runtime(
    *,
    kanban_create_complaint: Callable[[str, str, int, str], tuple[str | None, str]] | None = None,
    kanban_add_complaint_comment: Callable[[str, str], tuple[int | None, str]] | None = None,
    kanban_create_sales_task: Callable[[str, str, str, int, str], tuple[str | None, str]] | None = None,
    kb_search_handler: Callable[[dict[str, Any]], str] | None = None,
) -> None:
    global _KANBAN_CREATE_COMPLAINT
    global _KANBAN_ADD_COMPLAINT_COMMENT
    global _KANBAN_CREATE_SALES_TASK
    global _KB_SEARCH_HANDLER
    _KANBAN_CREATE_COMPLAINT = kanban_create_complaint
    _KANBAN_ADD_COMPLAINT_COMMENT = kanban_add_complaint_comment
    _KANBAN_CREATE_SALES_TASK = kanban_create_sales_task
    _KB_SEARCH_HANDLER = kb_search_handler


def _json(data: dict[str, Any]) -> str:
    return json.dumps(data, ensure_ascii=False, separators=(",", ":"))


def _clean_text(value: Any, *, max_len: int = 1200) -> str:
    cleaned = re.sub(r"\s+", " ", str(value or "")).strip()
    return cleaned[:max_len]


def _body_text(value: Any, *, max_len: int = 4000) -> str:
    cleaned = str(value or "").replace("\r\n", "\n").replace("\r", "\n").strip()
    return cleaned[:max_len]


def _session_env(name: str) -> str:
    try:
        from gateway.session_context import get_session_env

        return _clean_text(get_session_env(name, ""), max_len=4000)
    except Exception:
        return _clean_text(os.getenv(name, ""), max_len=4000)


def _kanban_create_complaint(title: str, body: str, priority: int, idempotency_key: str) -> tuple[str | None, str]:
    if _KANBAN_CREATE_COMPLAINT is None:
        return None, "dry_run_no_hermes_kanban_runtime"
    return _KANBAN_CREATE_COMPLAINT(title, body, priority, idempotency_key)


def _kanban_add_complaint_comment(task_id: str, body: str) -> tuple[int | None, str]:
    if _KANBAN_ADD_COMPLAINT_COMMENT is None:
        return None, "dry_run_no_hermes_kanban_runtime"
    return _KANBAN_ADD_COMPLAINT_COMMENT(task_id, body)


def _kanban_create_sales_task(title: str, body: str, task_type: str, priority: int, idempotency_key: str) -> tuple[str | None, str]:
    if _KANBAN_CREATE_SALES_TASK is None:
        return None, "dry_run_no_hermes_kanban_runtime"
    return _KANBAN_CREATE_SALES_TASK(title, body, task_type, priority, idempotency_key)


def _public_kb_search(args: dict[str, Any]) -> dict[str, Any]:
    if _KB_SEARCH_HANDLER is None:
        return {
            "success": False,
            "results": [],
            "result_count": 0,
            "error": "Public KB search is not configured",
        }
    try:
        result = _KB_SEARCH_HANDLER(args)
        if isinstance(result, dict):
            return result
        return json.loads(result)
    except Exception as exc:
        return {
            "success": False,
            "results": [],
            "result_count": 0,
            "error": "Public KB search failed",
            "detail": _clean_text(exc, max_len=500),
        }

QINTOPIA_COMPLAINT_INTAKE_CREATE_SCHEMA = {
    "description": (
        "Create a controlled Qintopia complaint/service-recovery intake card "
        "for 大总管 dispatch. This is the only Kanban-create path available "
        "to Erhua and it always creates task_type=complaint_intake."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "requester_display_name": {
                "type": "string",
                "description": "How the requester is known in the current channel.",
            },
            "requester_channel_user_id": {
                "type": "string",
                "description": (
                    "Channel-scoped requester id. For QiWe this is the webhook "
                    "senderId used as /msg/sendText toId; omitted calls fall "
                    "back to the current Hermes session user id."
                ),
            },
            "source_channel": {
                "type": "string",
                "enum": ["qiwe_group_internal", "qiwe_direct", "wechat_work_internal", "feishu_internal"],
                "description": "Where the complaint was received.",
            },
            "source_conversation_id": {
                "type": "string",
                "description": "Channel conversation id for audit and follow-up.",
            },
            "source_message_id": {
                "type": "string",
                "description": "Original message id, if available.",
            },
            "source_message_url": {
                "type": "string",
                "description": "Original message URL, if available.",
            },
            "original_message": {
                "type": "string",
                "description": "Requester complaint/feedback text.",
            },
            "complaint_summary": {
                "type": "string",
                "description": "Short Chinese summary. Defaults to a safe summary of original_message.",
            },
            "priority": {
                "type": "integer",
                "minimum": 0,
                "maximum": 3,
                "description": "Hermes priority. Defaults to 1 for service recovery.",
            },
            "idempotency_key": {
                "type": "string",
                "description": "Optional webhook idempotency key.",
            },
        },
        "required": ["source_channel", "source_conversation_id", "original_message"],
        "additionalProperties": False,
    },
}


QINTOPIA_COMPLAINT_INTAKE_UPDATE_SCHEMA = {
    "description": (
        "Append requester-provided details to an existing complaint_intake card. "
        "Does not change owner or assign an executor."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "task_id": {
                "type": "string",
                "description": "Existing complaint_intake Kanban task id.",
            },
            "requester_display_name": {
                "type": "string",
                "description": "Requester display name, if available.",
            },
            "details": {
                "type": "string",
                "description": "New details provided by the complainant.",
            },
            "occurred_at": {
                "type": "string",
                "description": "When the issue happened, if provided.",
            },
            "location_or_area": {
                "type": "string",
                "description": "Where the issue happened, if provided.",
            },
            "already_contacted": {
                "type": "string",
                "description": "Whether the requester contacted staff/front desk.",
            },
            "expected_resolution": {
                "type": "string",
                "description": "Requested resolution or reply expectation.",
            },
        },
        "required": ["task_id", "details"],
        "additionalProperties": False,
    },
}


QINTOPIA_COMPLAINT_FOLLOWUP_SEND_SCHEMA = {
    "description": (
        "Prepare an approved private follow-up message to a complainant after "
        "a complaint_intake card is completed/reviewed. The caller must provide "
        "the approved resolution text."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "task_id": {
                "type": "string",
                "description": "Completed complaint_intake Kanban task id.",
            },
            "requester_channel_user_id": {
                "type": "string",
                "description": "Channel-scoped requester id for private follow-up.",
            },
            "requester_display_name": {
                "type": "string",
                "description": "Requester display name, if available.",
            },
            "approved_resolution": {
                "type": "string",
                "description": "大总管/负责人批准可对外发送的处理结果。",
            },
            "followup_question": {
                "type": "string",
                "description": "Optional short follow-up question.",
            },
        },
        "required": ["task_id", "requester_channel_user_id", "approved_resolution"],
        "additionalProperties": False,
    },
}


QINTOPIA_EXTERNAL_PRODUCT_KB_SEARCH_SCHEMA = {
    "description": (
        "Search only Public Qintopia Agent OS product knowledge for Xiaoqin. "
        "Returns approved baseline statements when the Public KB has no useful match."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "query": {"type": "string", "description": "Customer-facing product question."},
            "limit": {"type": "integer", "minimum": 1, "maximum": MAX_KB_LIMIT},
            "purpose": {"type": "string", "description": "Why Xiaoqin needs this answer."},
        },
        "required": ["query"],
        "additionalProperties": False,
    },
}


QINTOPIA_PUBLIC_CASE_SEARCH_SCHEMA = {
    "description": (
        "Search approved Public case/demo references for Xiaoqin. If no approved "
        "case exists, returns a safe escalation result instead of inventing one."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "query": {"type": "string", "description": "Case, customer, demo, or pilot query."},
            "limit": {"type": "integer", "minimum": 1, "maximum": MAX_KB_LIMIT},
        },
        "required": ["query"],
        "additionalProperties": False,
    },
}


QINTOPIA_CUSTOMER_CONTEXT_LOOKUP_SCHEMA = {
    "description": (
        "Return only current-channel customer context for Xiaoqin. This is not a "
        "CRM lookup and does not expose other customers or internal records."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "customer_display_name": {"type": "string"},
            "source_channel": {"type": "string"},
            "source_conversation_id": {"type": "string"},
            "source_message_id": {"type": "string"},
            "customer_provided_context": {"type": "string"},
        },
        "additionalProperties": False,
    },
}


QINTOPIA_LEAD_CAPTURE_SCHEMA = {
    "description": (
        "Create a controlled Xiaoqin sales/customer Kanban card. It can create "
        "only sales_lead, demo_request, proposal, or external_disclosure_review."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "task_type": {
                "type": "string",
                "enum": list(SALES_TASK_TYPES.keys()),
                "description": "Controlled sales task type. Defaults to sales_lead.",
            },
            "customer_display_name": {"type": "string"},
            "source_channel": {
                "type": "string",
                "enum": SALES_SOURCE_CHANNELS,
            },
            "source_conversation_id": {"type": "string"},
            "source_message_id": {"type": "string"},
            "source_message_url": {"type": "string"},
            "customer_request": {"type": "string"},
            "business_scenario": {"type": "string"},
            "budget_range": {"type": "string"},
            "urgency": {"type": "string"},
            "current_system": {"type": "string"},
            "pilot_expectation": {"type": "string"},
            "next_step": {"type": "string"},
            "public_answer_given": {"type": "string"},
            "boundary_trigger": {"type": "string"},
            "idempotency_key": {"type": "string"},
            "priority": {"type": "integer", "minimum": 0, "maximum": 3},
        },
        "required": ["source_channel", "source_conversation_id", "customer_request"],
        "additionalProperties": False,
    },
}


QINTOPIA_PROPOSAL_OUTLINE_GENERATE_SCHEMA = {
    "description": (
        "Generate a safe proposal outline draft for Xiaoqin. The output is a draft "
        "and does not include binding price, schedule, scope, SLA, or contract terms."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "customer_display_name": {"type": "string"},
            "business_scenario": {"type": "string"},
            "goals": {"type": "string"},
            "current_system": {"type": "string"},
            "constraints": {"type": "string"},
            "pilot_expectation": {"type": "string"},
            "public_knowledge_summary": {"type": "string"},
        },
        "required": ["business_scenario"],
        "additionalProperties": False,
    },
}


QINTOPIA_DEMO_SCRIPT_GENERATE_SCHEMA = {
    "description": (
        "Generate a low-risk demo script for public samples, redacted materials, "
        "or customer-provided materials explicitly allowed for demo use."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "customer_display_name": {"type": "string"},
            "demo_goal": {"type": "string"},
            "business_scenario": {"type": "string"},
            "allowed_materials": {"type": "string"},
            "timebox_minutes": {"type": "integer", "minimum": 5, "maximum": 120},
        },
        "required": ["demo_goal", "business_scenario"],
        "additionalProperties": False,
    },
}


QINTOPIA_EXTERNAL_DISCLOSURE_FILTER_SCHEMA = {
    "description": (
        "Filter a Xiaoqin draft for external disclosure. Returns public-safe text, "
        "internal-only notes, and whether team review is required."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "draft_answer": {"type": "string"},
            "recipient": {"type": "string"},
            "purpose": {"type": "string"},
        },
        "required": ["draft_answer", "purpose"],
        "additionalProperties": False,
    },
}


QINTOPIA_CONVERSATION_SUMMARY_SCHEMA = {
    "description": (
        "Summarize a Xiaoqin customer conversation into a safe handoff format."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "conversation_text": {"type": "string"},
            "customer_display_name": {"type": "string"},
            "source_channel": {"type": "string"},
            "public_answer_given": {"type": "string"},
            "boundary_trigger": {"type": "string"},
        },
        "required": ["conversation_text"],
        "additionalProperties": False,
    },
}



def _resolve_requester_channel_user_id(args: dict[str, Any], source_channel: str) -> str:
    explicit = _clean_text(args.get("requester_channel_user_id"), max_len=160)
    if explicit:
        return explicit
    if source_channel in {"qiwe_group_internal", "qiwe_direct"}:
        return _clean_text(_session_env("HERMES_SESSION_USER_ID"), max_len=160)
    return ""


def _complaint_idempotency_key(args: dict[str, Any]) -> str:
    explicit = _clean_text(args.get("idempotency_key"), max_len=200)
    if explicit:
        return explicit
    source = "|".join(
        [
            _clean_text(args.get("source_channel"), max_len=80),
            _clean_text(args.get("source_conversation_id"), max_len=160),
            _clean_text(args.get("source_message_id"), max_len=160),
            _clean_text(args.get("original_message"), max_len=600),
        ]
    )
    return "qintopia-complaint-" + hashlib.sha256(source.encode("utf-8")).hexdigest()[:24]


def _complaint_followup_idempotency_key(task_id: str, requester_id: str, approved_resolution: str) -> str:
    source = "|".join([task_id, requester_id, approved_resolution])
    return "qintopia-complaint-followup-" + hashlib.sha256(source.encode("utf-8")).hexdigest()[:24]


def _private_detail_request_message(name: str) -> str:
    prefix = f"{name}，" if name else ""
    return (
        f"{prefix}我已经把这件事作为投诉/反馈提交给大总管分诊了。\n\n"
        "为了避免在群里公开你的细节，我在这里补充确认几项就好：\n"
        "1. 希望怎么称呼你？\n"
        "2. 具体发生了什么？\n"
        "3. 大概时间、地点或涉及区域是哪里？\n"
        "4. 是否已经联系过前台、小客服或工作人员？\n"
        "5. 你期望怎么处理，或者希望什么时候收到回复？"
    )


def _complaint_task_title(summary: str) -> str:
    base = summary or "投诉/反馈待分诊"
    return f"投诉受理：{base[:48]}"


def _complaint_task_body(args: dict[str, Any], summary: str) -> str:
    requester = _clean_text(args.get("requester_display_name"), max_len=80) or "未提供"
    source_channel = _clean_text(args.get("source_channel"), max_len=80)
    requester_id = _resolve_requester_channel_user_id(args, source_channel) or "未提供"
    conversation_id = _clean_text(args.get("source_conversation_id"), max_len=160)
    message_id = _clean_text(args.get("source_message_id"), max_len=160) or "未提供"
    source_url = _clean_text(args.get("source_message_url"), max_len=300) or "未提供"
    original = _body_text(args.get("original_message"))
    return textwrap.dedent(
        f"""
        ## 投诉/反馈摘要

        {summary}

        ## 来源

        - 发起人：{requester}
        - 渠道用户 ID：{requester_id}
        - 来源渠道：{source_channel}
        - 会话 ID：{conversation_id}
        - 消息 ID：{message_id}
        - 来源链接：{source_url}

        ## 原始反馈

        {original}

        ## 待补充信息

        - 希望如何称呼投诉人
        - 具体内容
        - 发生时间、地点或涉及区域
        - 是否已联系前台、小客服或工作人员
        - 期望的处理方式或回复时间

        ## 首次私聊状态

        二花入口会通过受控 `qiwe_send_direct_message` 动作向投诉人发送一次私聊收集请求。
        大总管不要再创建或派发“二花私聊补充受理”子任务，避免投诉人收到重复私聊。
        投诉人回复后，二花应通过 `qintopia_complaint_intake_update` 补充到同一张投诉卡。

        ## 受理边界

        二花只负责受理、私聊收集最小必要信息、补充到同一张卡，并在处理结果批准后回访。
        大总管负责分诊、派单、确认处理结果和对外回复口径。
        """
    ).strip()


def _sales_idempotency_key(args: dict[str, Any]) -> str:
    explicit = _clean_text(args.get("idempotency_key"), max_len=200)
    if explicit:
        return explicit
    source = "|".join(
        [
            _clean_text(args.get("task_type") or "sales_lead", max_len=80),
            _clean_text(args.get("source_channel"), max_len=80),
            _clean_text(args.get("source_conversation_id"), max_len=160),
            _clean_text(args.get("source_message_id"), max_len=160),
            _clean_text(args.get("customer_request"), max_len=600),
        ]
    )
    return "qintopia-sales-" + hashlib.sha256(source.encode("utf-8")).hexdigest()[:24]


def _sales_task_title(task_type: str, customer: str, request: str) -> str:
    label = SALES_TASK_TYPES.get(task_type, SALES_TASK_TYPES["sales_lead"])["label"]
    who = customer or "外部客户"
    summary = _clean_text(request, max_len=42) or "待补充需求"
    return f"{label}：{who} - {summary}"


def _sales_task_body(args: dict[str, Any], task_type: str) -> str:
    customer = _clean_text(args.get("customer_display_name"), max_len=120) or "未提供"
    source_channel = _clean_text(args.get("source_channel"), max_len=80)
    conversation_id = _clean_text(args.get("source_conversation_id"), max_len=160)
    message_id = _clean_text(args.get("source_message_id"), max_len=160) or "未提供"
    source_url = _clean_text(args.get("source_message_url"), max_len=300) or "未提供"
    request = _body_text(args.get("customer_request"))
    business_scenario = _body_text(args.get("business_scenario"), max_len=1200) or "待补充"
    public_answer = _body_text(args.get("public_answer_given"), max_len=1600) or "尚未形成公开回答"
    boundary_trigger = _body_text(args.get("boundary_trigger"), max_len=800) or "未触发特殊边界"
    fields = [
        ("预算范围", _clean_text(args.get("budget_range"), max_len=180)),
        ("紧急程度", _clean_text(args.get("urgency"), max_len=180)),
        ("现有系统", _clean_text(args.get("current_system"), max_len=260)),
        ("试点期望", _clean_text(args.get("pilot_expectation"), max_len=500)),
        ("建议下一步", _clean_text(args.get("next_step"), max_len=500) or "由小秦或团队负责人继续确认"),
    ]
    field_lines = [f"- {label}：{value}" for label, value in fields if value]
    return textwrap.dedent(
        f"""
        ## 客户与来源

        - 客户：{customer}
        - 来源渠道：{source_channel}
        - 会话 ID：{conversation_id}
        - 消息 ID：{message_id}
        - 来源链接：{source_url}
        - 任务类型：{task_type}

        ## 客户诉求

        {request}

        ## 场景与约束

        {business_scenario}

        {chr(10).join(field_lines) if field_lines else "- 待补充预算、紧急程度、现有系统和试点期望。"}

        ## 已公开回答

        {public_answer}

        ## 触发边界

        {boundary_trigger}

        ## 需要团队负责人决策

        - 报价、合同、交付范围、排期、SLA 或客户案例细节需要团队负责人确认。
        - 若涉及内部资料、未公开案例、客户数据或基础设施细节，需要先做披露审核。

        ## 建议下一步

        {_clean_text(args.get("next_step"), max_len=500) or "确认客户场景、预算范围、演示时间和可使用的公开/脱敏材料。"}
        """
    ).strip()


def _safe_list(value: str) -> list[str]:
    return [line.strip(" -\t") for line in str(value or "").splitlines() if line.strip()]


SENSITIVE_DISCLOSURE_PATTERNS = {
    "internal_information": ["内部", "未公开", "私有", "prompt", "提示词", "服务器", "日志", "数据库", "路由", "成本", "模型供应商"],
    "commercial_commitment": ["报价", "价格", "折扣", "合同", "交付", "排期", "sla", "验收", "付款", "采购"],
    "customer_or_member_data": ["客户资料", "客户案例", "成员资料", "联系方式", "聊天记录", "入住信息", "村民档案"],
    "credentials": ["secret", "token", "credential", "password", "密钥", "凭证", "私钥"],
}


def _disclosure_hits(text: str) -> dict[str, list[str]]:
    lowered = text.lower()
    hits: dict[str, list[str]] = {}
    for category, patterns in SENSITIVE_DISCLOSURE_PATTERNS.items():
        matched = [pattern for pattern in patterns if pattern.lower() in lowered]
        if matched:
            hits[category] = matched[:5]
    return hits



def handle_qintopia_complaint_intake_create(args: dict[str, Any], **_: Any) -> str:
    source_channel = _clean_text(args.get("source_channel"), max_len=80)
    conversation_id = _clean_text(args.get("source_conversation_id"), max_len=160)
    original_message = _body_text(args.get("original_message"))
    if source_channel not in {"qiwe_group_internal", "qiwe_direct", "wechat_work_internal", "feishu_internal"}:
        return _json({"success": False, "error": "source_channel is not allowed for complaint intake"})
    if not conversation_id:
        return _json({"success": False, "error": "source_conversation_id is required"})
    if not original_message:
        return _json({"success": False, "error": "original_message is required"})

    summary = _clean_text(args.get("complaint_summary"), max_len=160)
    if not summary:
        summary = _clean_text(original_message, max_len=80) or "投诉/反馈待分诊"
    priority = args.get("priority")
    try:
        priority_value = int(priority) if priority is not None else 1
    except (TypeError, ValueError):
        return _json({"success": False, "error": "priority must be an integer"})
    priority_value = min(max(priority_value, 0), 3)

    title = _complaint_task_title(summary)
    body = _complaint_task_body(args, summary)
    idempotency_key = _complaint_idempotency_key(args)
    task_id, status = _kanban_create_complaint(title, body, priority_value, idempotency_key)
    requester_name = _clean_text(args.get("requester_display_name"), max_len=80)
    requester_id = _resolve_requester_channel_user_id(args, source_channel)

    kanban_action = {
        "action": "kanban_task_create_request",
        "board": COMPLAINT_BOARD,
        "tenant": QINTOPIA_TENANT,
        "title": title,
        "body": body,
        "assignee": COMPLAINT_OWNER_PROFILE,
        "owner_profile": COMPLAINT_OWNER_PROFILE,
        "task_type": COMPLAINT_TASK_TYPE,
        "status": "triage",
        "priority": priority_value,
        "information_class": "Member-scoped",
        "risk_level": "P1",
        "human_approval_requirement": "大总管确认处理结果后才能对投诉人回访",
        "idempotency_key": idempotency_key,
    }
    actions: list[dict[str, Any]] = [kanban_action]
    if requester_id:
        actions.append(
            {
                "tool": "qiwe_send_direct_message",
                "recipient_user_id": requester_id,
                "recipient_display_name": requester_name,
                "conversation_scope": "private",
                "message": _private_detail_request_message(requester_name),
                "purpose": "complaint_intake_detail_collection",
                "idempotency_key": f"{idempotency_key}:direct:intake",
            }
        )

    return _json(
        {
            "success": True,
            "skill": "qintopia_complaint_intake_create",
            "mode": status,
            "task_id": task_id,
            "task_type": COMPLAINT_TASK_TYPE,
            "owner_profile": COMPLAINT_OWNER_PROFILE,
            "board": COMPLAINT_BOARD,
            "tenant": QINTOPIA_TENANT,
            "followup_status": "need_private_details",
            "requester_channel_user_id_resolved": bool(requester_id),
            "actions": actions,
            "guardrails": [
                "only complaint_intake may be created",
                "dispatch owner remains default/大总管",
                "do not assign executor from Erhua",
                "collect details in private conversation",
                "do not promise compensation, punishment, refund, deadline, or final result",
            ],
        }
    )


def handle_qintopia_complaint_intake_update(args: dict[str, Any], **_: Any) -> str:
    task_id = _clean_text(args.get("task_id"), max_len=80)
    details = _body_text(args.get("details"))
    if not task_id:
        return _json({"success": False, "error": "task_id is required"})
    if not details:
        return _json({"success": False, "error": "details is required"})

    requester = _clean_text(args.get("requester_display_name"), max_len=80) or "投诉人"
    fields = [
        ("补充人", requester),
        ("发生时间", _clean_text(args.get("occurred_at"), max_len=120)),
        ("地点/区域", _clean_text(args.get("location_or_area"), max_len=120)),
        ("已联系人员", _clean_text(args.get("already_contacted"), max_len=240)),
        ("期望处理", _clean_text(args.get("expected_resolution"), max_len=300)),
    ]
    field_lines = [f"- {label}：{value}" for label, value in fields if value]
    comment = "\n".join(
        [
            "## 二花补充的投诉详情",
            "",
            *field_lines,
            "",
            "### 新增说明",
            "",
            details,
        ]
    ).strip()
    comment_id, status = _kanban_add_complaint_comment(task_id, comment)

    return _json(
        {
            "success": True,
            "skill": "qintopia_complaint_intake_update",
            "mode": status,
            "task_id": task_id,
            "comment_id": comment_id,
            "task_type": COMPLAINT_TASK_TYPE,
            "actions": [
                {
                    "action": "kanban_comment_add_request",
                    "board": COMPLAINT_BOARD,
                    "task_id": task_id,
                    "author": "erhua",
                    "body": comment,
                    "does_not_assign_executor": True,
                }
            ],
            "guardrails": [
                "append details to existing complaint card only",
                "do not change owner or assignee",
                "do not publish private details to group chat",
            ],
        }
    )


def handle_qintopia_complaint_followup_send(args: dict[str, Any], **_: Any) -> str:
    task_id = _clean_text(args.get("task_id"), max_len=80)
    requester_id = _clean_text(args.get("requester_channel_user_id"), max_len=160)
    approved_resolution = _body_text(args.get("approved_resolution"), max_len=2000)
    if not task_id:
        return _json({"success": False, "error": "task_id is required"})
    if not requester_id:
        return _json({"success": False, "error": "requester_channel_user_id is required"})
    if not approved_resolution:
        return _json({"success": False, "error": "approved_resolution is required"})

    requester_name = _clean_text(args.get("requester_display_name"), max_len=80)
    prefix = f"{requester_name}，" if requester_name else ""
    followup_question = (
        _clean_text(args.get("followup_question"), max_len=200)
        or "你看这个处理结果是否解决了你的问题？如果还需要继续跟进，可以直接告诉我。"
    )
    message = f"{prefix}这件投诉/反馈已经有处理结果了：\n\n{approved_resolution}\n\n{followup_question}"
    comment_id, status = _kanban_add_complaint_comment(
        task_id,
        "## 二花回访记录\n\n已根据批准口径私聊投诉人同步处理结果，并询问是否需要继续跟进。",
    )
    idempotency_key = _complaint_followup_idempotency_key(task_id, requester_id, approved_resolution)

    return _json(
        {
            "success": True,
            "skill": "qintopia_complaint_followup_send",
            "mode": status,
            "task_id": task_id,
            "comment_id": comment_id,
            "actions": [
                {
                    "tool": "qiwe_send_direct_message",
                    "recipient_user_id": requester_id,
                    "recipient_display_name": requester_name,
                    "conversation_scope": "private",
                    "message": message,
                    "purpose": "complaint_resolution_followup",
                    "idempotency_key": idempotency_key,
                    "requires_approved_resolution": True,
                }
            ],
            "guardrails": [
                "send only approved resolution text",
                "private follow-up only",
                "do not invent handling result",
                "continue follow-up only by appending to the complaint card",
            ],
        }
    )


def handle_qintopia_external_product_kb_search(args: dict[str, Any], **_: Any) -> str:
    query = _clean_text(args.get("query"), max_len=300)
    if not query:
        return _json({"success": False, "error": "query is required"})
    raw = _public_kb_search(
            {
                "query": query,
                "information_classes": ["Public"],
                "limit": args.get("limit") or DEFAULT_KB_LIMIT,
                "caller": "xiaoqin",
                "purpose": args.get("purpose") or "external_product_answer",
            }
        )
    if raw.get("success") is False:
        return _json(
            {
                "success": False,
                "skill": "qintopia_external_product_kb_search",
                "query": query,
                "scope_used": ["Public"],
                "result_count": 0,
                "results": [],
                "error": raw.get("error") or "Public KB search failed",
                "detail": raw.get("detail") or "",
                "needs_human_review": True,
                "safe_answer_mode": "kb_lookup_failed",
                "safe_customer_message": "公开知识库检索暂时不可用，先不要直接对外确认产品事实或案例细节，请交给团队同事复核后再回复。",
                "not_accessed": ["Internal", "Member-scoped", "Restricted", "Feishu live", "other customers"],
            }
        )
    results = raw.get("results", [])
    return _json(
        {
            "success": True,
            "skill": "qintopia_external_product_kb_search",
            "query": query,
            "scope_used": ["Public"],
            "result_count": len(results),
            "results": results,
            "approved_public_baselines": PUBLIC_AGENT_OS_BASELINES,
            "needs_human_review": len(results) == 0,
            "safe_answer_mode": (
                "use_public_results_with_disclosure_filter"
                if results
                else "use_baseline_only_and_collect_customer_context"
            ),
            "not_accessed": ["Internal", "Member-scoped", "Restricted", "Feishu live", "other customers"],
            "guardrails": [
                "do not invent product facts beyond Public results or approved baselines",
                "do not discuss price, contract, delivery schedule, SLA, internal architecture, prompts, logs, or customer cases without approval",
            ],
        }
    )


def handle_qintopia_public_case_search(args: dict[str, Any], **_: Any) -> str:
    query = _clean_text(args.get("query"), max_len=300)
    if not query:
        return _json({"success": False, "error": "query is required"})
    raw = _public_kb_search(
            {
                "query": f"{query} 案例 客户 成功案例 试点 公开",
                "information_classes": ["Public"],
                "limit": args.get("limit") or DEFAULT_KB_LIMIT,
                "caller": "xiaoqin",
                "purpose": "public_case_search",
            }
        )
    if raw.get("success") is False:
        return _json(
            {
                "success": False,
                "skill": "qintopia_public_case_search",
                "query": query,
                "scope_used": ["Public"],
                "result_count": 0,
                "results": [],
                "error": raw.get("error") or "Public KB search failed",
                "detail": raw.get("detail") or "",
                "approved_public_cases_available": False,
                "needs_human_review": True,
                "safe_answer_mode": "kb_lookup_failed",
                "safe_customer_message": "公开案例检索暂时不可用，先不要对外确认是否有可公开案例，请交给团队同事复核后再回复。",
                "not_accessed": ["Internal cases", "other customer data", "contracts", "private pilots"],
            }
        )
    approved = []
    for item in raw.get("results", []):
        title_path = f"{item.get('title', '')} {item.get('path', '')}".lower()
        snippet = str(item.get("snippet", ""))
        if any(word in title_path for word in ["案例", "case", "成功案例", "public-case", "approved-case"]):
            approved.append(item)
        elif "已批准公开案例" in snippet or "approved public case" in snippet.lower():
            approved.append(item)
    return _json(
        {
            "success": True,
            "skill": "qintopia_public_case_search",
            "query": query,
            "scope_used": ["Public"],
            "result_count": len(approved),
            "results": approved,
            "approved_public_cases_available": bool(approved),
            "needs_human_review": not approved,
            "safe_customer_message": (
                "当前没有检索到已批准公开的客户案例。我可以先记录您想了解的案例方向，交给团队负责人判断哪些材料可以对外分享。"
                if not approved
                else ""
            ),
            "not_accessed": ["Internal cases", "other customer data", "contracts", "private pilots"],
        }
    )


def handle_qintopia_customer_context_lookup(args: dict[str, Any], **_: Any) -> str:
    provided_context = _body_text(args.get("customer_provided_context"), max_len=1600)
    return _json(
        {
            "success": True,
            "skill": "qintopia_customer_context_lookup",
            "mode": "current_channel_context_only",
            "customer": {
                "display_name": _clean_text(args.get("customer_display_name"), max_len=120) or "未提供",
                "source_channel": _clean_text(args.get("source_channel"), max_len=80) or "未提供",
                "source_conversation_id": _clean_text(args.get("source_conversation_id"), max_len=160) or "未提供",
                "source_message_id": _clean_text(args.get("source_message_id"), max_len=160) or "未提供",
            },
            "customer_provided_context": provided_context,
            "stored_context_found": False,
            "not_accessed": ["CRM", "other customer records", "member profiles", "private chat history"],
            "guardrails": [
                "use only current conversation and customer-provided context",
                "ask for permission before using customer-provided material in a demo",
            ],
        }
    )


def handle_qintopia_lead_capture(args: dict[str, Any], **_: Any) -> str:
    task_type = _clean_text(args.get("task_type") or "sales_lead", max_len=80)
    if task_type not in SALES_TASK_TYPES:
        return _json({"success": False, "error": "task_type is not allowed for Xiaoqin lead capture"})
    source_channel = _clean_text(args.get("source_channel"), max_len=80)
    conversation_id = _clean_text(args.get("source_conversation_id"), max_len=160)
    customer_request = _body_text(args.get("customer_request"), max_len=2400)
    if source_channel not in SALES_SOURCE_CHANNELS:
        return _json({"success": False, "error": "source_channel is not allowed for Xiaoqin lead capture"})
    if not conversation_id:
        return _json({"success": False, "error": "source_conversation_id is required"})
    if not customer_request:
        return _json({"success": False, "error": "customer_request is required"})
    try:
        priority = int(args.get("priority") if args.get("priority") is not None else 1)
    except (TypeError, ValueError):
        return _json({"success": False, "error": "priority must be an integer"})
    priority = min(max(priority, 0), 3)
    customer = _clean_text(args.get("customer_display_name"), max_len=120)
    title = _sales_task_title(task_type, customer, customer_request)
    body = _sales_task_body(args, task_type)
    idempotency_key = _sales_idempotency_key(args)
    task_id, status = _kanban_create_sales_task(title, body, task_type, priority, idempotency_key)
    assignee = "default" if task_type == "external_disclosure_review" else SALES_OWNER_PROFILE

    return _json(
        {
            "success": True,
            "skill": "qintopia_lead_capture",
            "mode": status,
            "task_id": task_id,
            "task_type": task_type,
            "board": SALES_BOARD,
            "tenant": QINTOPIA_TENANT,
            "owner_profile": assignee,
            "safe_customer_message": XIAOQIN_SAFE_FOLLOWUP_MESSAGE,
            "customer_response_policy": [
                "Use safe_customer_message for the customer-facing reply after this tool call.",
                "Do not add internal execution details to the customer-facing reply.",
            ],
            "actions": [
                {
                    "action": "kanban_task_create_request",
                    "board": SALES_BOARD,
                    "tenant": QINTOPIA_TENANT,
                    "title": title,
                    "body": body,
                    "assignee": assignee,
                    "owner_profile": assignee,
                    "task_type": task_type,
                    "status": SALES_TASK_TYPES[task_type]["status"],
                    "priority": priority,
                    "information_class": "Public" if task_type in {"sales_lead", "demo_request"} else "Internal",
                    "risk_level": "P1" if task_type == "external_disclosure_review" else "P2",
                    "human_approval_requirement": "团队负责人需确认报价、合同、交付、SLA、内部披露或客户案例细节",
                    "idempotency_key": idempotency_key,
                }
            ],
            "guardrails": [
                "only controlled sales task types may be created",
                "do not create binding price, contract, delivery, schedule, or SLA commitments",
                "external_disclosure_review is routed to default/team review",
            ],
        }
    )


def handle_qintopia_proposal_outline_generate(args: dict[str, Any], **_: Any) -> str:
    scenario = _body_text(args.get("business_scenario"), max_len=1200)
    if not scenario:
        return _json({"success": False, "error": "business_scenario is required"})
    customer = _clean_text(args.get("customer_display_name"), max_len=120) or "客户"
    outline = textwrap.dedent(
        f"""
        # Qintopia Agent OS 方案草案

        ## 客户与场景

        - 客户：{customer}
        - 场景：{scenario}

        ## 目标

        {_body_text(args.get("goals"), max_len=1000) or "待客户进一步确认业务目标、成功指标和试点范围。"}

        ## 初步思路

        - 用 Agent OS 承接需求收集、公开知识问答、方案草拟、任务交接和人工审批。
        - 先使用公开样例、脱敏材料或客户明确允许用于演示的材料做低风险试点。
        - 将需要人工判断的报价、合同、交付范围、排期和 SLA 交给团队负责人。

        ## 当前系统与约束

        - 现有系统：{_body_text(args.get("current_system"), max_len=800) or "待补充"}
        - 约束：{_body_text(args.get("constraints"), max_len=800) or "待补充"}
        - 试点期望：{_body_text(args.get("pilot_expectation"), max_len=800) or "待补充"}

        ## 需要确认

        - 可用于演示或试点的数据范围。
        - 是否涉及内部资料、客户案例、报价、合同或交付承诺。
        - 下一步会议时间和团队负责人参与方式。
        """
    ).strip()
    return _json(
        {
            "success": True,
            "skill": "qintopia_proposal_outline_generate",
            "draft": outline,
            "requires_disclosure_filter": True,
            "requires_human_review_before_external_send": True,
            "guardrails": [
                "draft only",
                "no binding price, contract, delivery schedule, scope, SLA, or customer case claims",
                "run qintopia_external_disclosure_filter before sending externally",
            ],
        }
    )


def handle_qintopia_demo_script_generate(args: dict[str, Any], **_: Any) -> str:
    demo_goal = _body_text(args.get("demo_goal"), max_len=800)
    scenario = _body_text(args.get("business_scenario"), max_len=1000)
    if not demo_goal:
        return _json({"success": False, "error": "demo_goal is required"})
    if not scenario:
        return _json({"success": False, "error": "business_scenario is required"})
    timebox = args.get("timebox_minutes")
    try:
        minutes = int(timebox) if timebox is not None else 30
    except (TypeError, ValueError):
        return _json({"success": False, "error": "timebox_minutes must be an integer"})
    minutes = min(max(minutes, 5), 120)
    allowed_materials = _body_text(args.get("allowed_materials"), max_len=1000) or "公开样例或客户明确允许用于演示的材料"
    script = textwrap.dedent(
        f"""
        # 小秦演示脚本草案

        ## 演示目标

        {demo_goal}

        ## 场景设定

        {scenario}

        ## 可用材料

        {allowed_materials}

        ## {minutes} 分钟流程

        1. 开场确认：说明本次演示只使用公开样例、脱敏材料或客户授权材料。
        2. 需求收集：让客户描述目标、现有流程、预算范围、紧急程度和试点期望。
        3. 公开知识问答：只回答 Public-safe 产品能力和流程边界。
        4. 方案草拟：生成一份标注“草案/需审核”的方案结构。
        5. 任务交接：展示如何形成商机、演示准备、方案草案或披露审核任务。
        6. 收尾升级：报价、合同、交付范围、排期、SLA、客户案例交给团队负责人。
        """
    ).strip()
    return _json(
        {
            "success": True,
            "skill": "qintopia_demo_script_generate",
            "script": script,
            "requires_human_review_before_external_send": True,
            "guardrails": [
                "do not use real member records, other customer data, internal logs, prompts, or server details",
                "mark demo output as draft/needs review",
                "do not promise price, schedule, delivery result, or SLA",
            ],
        }
    )


def handle_qintopia_external_disclosure_filter(args: dict[str, Any], **_: Any) -> str:
    draft = _body_text(args.get("draft_answer"), max_len=5000)
    if not draft:
        return _json({"success": False, "error": "draft_answer is required"})
    purpose = _clean_text(args.get("purpose"), max_len=300)
    hits = _disclosure_hits(draft)
    approval_required = bool(hits)
    public_safe = draft
    internal_notes: list[str] = []
    if approval_required:
        internal_notes.append("草稿命中敏感披露关键词，不能直接发送给外部客户。")
        public_safe = (
            "这部分涉及需要进一步确认的信息，我不能直接对外确认。"
            "我可以先记录您的问题和背景，交给团队负责人判断哪些内容可以公开说明。"
        )
    return _json(
        {
            "success": True,
            "skill": "qintopia_external_disclosure_filter",
            "recipient": _clean_text(args.get("recipient"), max_len=120) or "external_customer",
            "purpose": purpose,
            "approval_required": approval_required,
            "public_safe_draft": public_safe,
            "internal_only_notes": internal_notes,
            "matched_risk_categories": hits,
            "blocked_topics": list(hits.keys()),
            "guardrails": [
                "send public_safe_draft only",
                "create external_disclosure_review if approval_required is true",
                "do not disclose internal notes externally",
            ],
        }
    )


def handle_qintopia_conversation_summary(args: dict[str, Any], **_: Any) -> str:
    conversation = _body_text(args.get("conversation_text"), max_len=6000)
    if not conversation:
        return _json({"success": False, "error": "conversation_text is required"})
    customer = _clean_text(args.get("customer_display_name"), max_len=120) or "未提供"
    source_channel = _clean_text(args.get("source_channel"), max_len=80) or "未提供"
    hits = _disclosure_hits(conversation)
    summary = textwrap.dedent(
        f"""
        ## 客户与来源

        - 客户：{customer}
        - 来源渠道：{source_channel}

        ## 客户诉求

        {_clean_text(conversation, max_len=800)}

        ## 已公开回答

        {_body_text(args.get("public_answer_given"), max_len=1200) or "待补充"}

        ## 触发边界

        {_body_text(args.get("boundary_trigger"), max_len=1000) or ("命中需审核主题：" + ", ".join(hits.keys()) if hits else "未发现明确敏感边界")}

        ## 需要团队负责人决策

        {"需要审核后再对外确认。" if hits else "如客户继续追问报价、合同、交付、SLA、客户案例或内部信息，需要团队负责人确认。"}

        ## 建议下一步

        继续确认业务场景、预算范围、紧急程度、现有系统、试点期望和下一步会议安排。
        """
    ).strip()
    return _json(
        {
            "success": True,
            "skill": "qintopia_conversation_summary",
            "summary": summary,
            "matched_risk_categories": hits,
            "suggested_task_type": "external_disclosure_review" if hits else "sales_lead",
            "guardrails": [
                "summary is for handoff and Kanban only",
                "do not include private data not provided in the current conversation",
            ],
        }
    )



def check_complaint_requirements() -> bool:
    return True


def check_sales_requirements() -> bool:
    return True
