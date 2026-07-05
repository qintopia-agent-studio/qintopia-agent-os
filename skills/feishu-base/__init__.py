"""Narrow read-only Feishu Base tools for Qintopia Agent workflows.

The package exposes allowlisted Hermes tools for Huabaosi collaboration. It reads
fixed Base tables only and intentionally has no arbitrary Base browsing or write
operation.
"""

from __future__ import annotations

import json
import os
import re
from datetime import datetime
from typing import Any
from urllib import request as urlrequest
from urllib.parse import quote


XIAOMAN_ACTIVITY_BASE_TOKEN_ENV = "QINTOPIA_BASE_READ_XIAOMAN_ACTIVITY_BASE_TOKEN"
XIAOMAN_ACTIVITY_PLAN_TABLE_ID_ENV = "QINTOPIA_BASE_READ_XIAOMAN_ACTIVITY_PLAN_TABLE_ID"
XIAOMAN_ACTIVITY_FIELD_NAMES = {
    "fldzXqzeG0": "活动内容",
    "fldov46c4K": "活动信息",
    "fld4nWBq8J": "计划地点",
    "fldDC15nKb": "发起人",
    "fldeD4DHTc": "计划状态",
    "fldL0AblkG": "活动前提醒状态",
    "fldqzUUyQF": "小满备注",
    "fldrJD8tOI": "原文摘要",
    "fldTNwQOYV": "宣发判断",
    "fldW0v57Um": "小满运营状态",
    "fldW89gj9f": "活动类型",
    "fldxd5zRiB": "素材照片",
    "fldYfyGR5Q": "计划时间",
    "fldZyFdV0D": "参与人名单",
}

HUABAOSI_DESIGN_BASE_TOKEN_ENV = "QINTOPIA_BASE_READ_HUABAOSI_DESIGN_BASE_TOKEN"
HUABAOSI_POSTER_TABLE_ID_ENV = "QINTOPIA_BASE_READ_HUABAOSI_POSTER_TABLE_ID"
HUABAOSI_POSTER_FIELD_NAMES = {
    "fld8o86g6W": "任务标题",
    "fld0cV1tki": "来源平台",
    "fld0vttg3S": "期望交付时间",
    "fld7Pwh3Pn": "任务状态",
    "fldAaxKcbX": "发送结果",
    "fldCU8EV0n": "创建时间",
    "flde7U9IOS": "成品图",
    "fldEohMSDE": "需求提出人",
    "fldGSHOsMc": "适用平台",
    "fldkZlKqEj": "任务编号",
    "fldLzIigIW": "需求原文",
    "fldmJBb2eC": "实际发送时间",
    "fldNywHYfM": "源素材",
    "fldp8bEu38": "画面规格",
    "fldstZ16tJ": "更新时间",
    "fldsWWVC0Z": "素材摘要",
    "fldws5TmeH": "中文发布文案",
    "fldYVGY1Bp": "生成 Prompt",
    "fldziX1N6t": "审核标记",
    "fldZqTPHQ7": "来源会话/任务ID",
}


QINTOPIA_XIAOMAN_ACTIVITY_RECORD_GET_SCHEMA = {
    "description": (
        "Read one Xiaoman activity-plan record from the allowlisted Feishu Base. "
        "Huabaosi must use this before judging a poster request when "
        "activity_record_id is provided."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "record_id": {
                "type": "string",
                "description": "Activity plan record id, e.g. recxxxx.",
            },
            "purpose": {"type": "string", "description": "Why this read is needed."},
        },
        "required": ["record_id"],
        "additionalProperties": False,
    },
}


QINTOPIA_HUABAOSI_DESIGN_RECORD_GET_SCHEMA = {
    "description": (
        "Read one Huabaosi poster production ledger record from the allowlisted "
        "Feishu Base. Use this to verify task status, source material, finished "
        "images, and blocked reason."
    ),
    "parameters": {
        "type": "object",
        "properties": {
            "record_id": {
                "type": "string",
                "description": "Poster ledger record id, e.g. recxxxx.",
            },
            "purpose": {"type": "string", "description": "Why this read is needed."},
        },
        "required": ["record_id"],
        "additionalProperties": False,
    },
}


def _json(data: dict[str, Any]) -> str:
    return json.dumps(data, ensure_ascii=False, separators=(",", ":"))


def _clean_text(value: Any, *, max_len: int = 1200) -> str:
    cleaned = re.sub(r"\s+", " ", str(value or "")).strip()
    return cleaned[:max_len]


def _session_env(name: str) -> str:
    try:
        from gateway.session_context import get_session_env

        return _clean_text(get_session_env(name, ""), max_len=4000)
    except Exception:
        return _clean_text(os.getenv(name, ""), max_len=4000)


def _required_session_env(name: str) -> str:
    value = _session_env(name)
    if not value:
        raise RuntimeError(f"{name} is required")
    return value


def _base_config(base_token_env: str, table_id_env: str) -> tuple[str, str]:
    return _required_session_env(base_token_env), _required_session_env(table_id_env)


def _feishu_app_credentials() -> tuple[str, str]:
    app_id = _session_env("FEISHU_APP_ID") or _session_env("LARK_APP_ID")
    app_secret = _session_env("FEISHU_APP_SECRET") or _session_env("LARK_APP_SECRET")
    if not app_id or not app_secret:
        raise RuntimeError(
            "FEISHU_APP_ID/FEISHU_APP_SECRET or LARK_APP_ID/LARK_APP_SECRET is required"
        )
    return app_id, app_secret


def _feishu_tenant_access_token() -> str:
    app_id, app_secret = _feishu_app_credentials()
    req = urlrequest.Request(
        "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal",
        data=json.dumps({"app_id": app_id, "app_secret": app_secret}).encode("utf-8"),
        headers={"Content-Type": "application/json; charset=utf-8"},
        method="POST",
    )
    with urlrequest.urlopen(req, timeout=10) as resp:
        data = json.loads(resp.read().decode("utf-8"))
    token = data.get("tenant_access_token")
    if not token:
        raise RuntimeError(f"failed to get tenant_access_token: {data}")
    return token


def _feishu_base_record_get(
    base_token: str, table_id: str, record_id: str
) -> dict[str, Any]:
    token = _feishu_tenant_access_token()
    url = (
        "https://open.feishu.cn/open-apis/bitable/v1/apps/"
        f"{quote(base_token)}/tables/{quote(table_id)}/records/{quote(record_id)}"
    )
    req = urlrequest.Request(url, headers={"Authorization": f"Bearer {token}"}, method="GET")
    with urlrequest.urlopen(req, timeout=10) as resp:
        data = json.loads(resp.read().decode("utf-8"))
    if data.get("code") not in (0, None):
        raise RuntimeError(f"failed to read Base record: {data}")
    return data


def _normalize_base_cell(value: Any) -> Any:
    if isinstance(value, int) and value > 10_000_000_000:
        return datetime.fromtimestamp(value / 1000).strftime("%Y-%m-%d %H:%M:%S")
    if isinstance(value, list):
        normalized: list[Any] = []
        for item in value:
            if isinstance(item, dict):
                if "text" in item:
                    normalized.append(item.get("text"))
                elif "name" in item or "file_token" in item:
                    normalized.append(
                        {
                            "name": item.get("name"),
                            "file_token": item.get("file_token"),
                            "size": item.get("size"),
                        }
                    )
                elif "id" in item:
                    normalized.append(item.get("id"))
                else:
                    normalized.append(item)
            else:
                normalized.append(item)
        return normalized
    return value


def _extract_named_fields(raw: dict[str, Any], field_names: dict[str, str]) -> dict[str, Any]:
    fields_raw = raw.get("data", {}).get("record", {}).get("fields", {})
    fields: dict[str, Any] = {}
    for field_id, name in field_names.items():
        raw_value = fields_raw.get(name, fields_raw.get(field_id))
        if raw_value is not None:
            fields[name] = _normalize_base_cell(raw_value)
    return fields


def handle_qintopia_xiaoman_activity_record_get(args: dict[str, Any], **_: Any) -> str:
    record_id = _clean_text(args.get("record_id"), max_len=80)
    if not re.fullmatch(r"rec[A-Za-z0-9]+", record_id or ""):
        return _json({"success": False, "error": "valid record_id is required"})
    try:
        base_token, table_id = _base_config(
            XIAOMAN_ACTIVITY_BASE_TOKEN_ENV, XIAOMAN_ACTIVITY_PLAN_TABLE_ID_ENV
        )
        raw = _feishu_base_record_get(base_token, table_id, record_id)
    except Exception as exc:
        return _json(
            {
                "success": False,
                "skill": "qintopia_xiaoman_activity_record_get",
                "record_id": record_id,
                "error": str(exc),
                "required_env": [
                    "FEISHU_APP_ID or LARK_APP_ID",
                    "FEISHU_APP_SECRET or LARK_APP_SECRET",
                    XIAOMAN_ACTIVITY_BASE_TOKEN_ENV,
                    XIAOMAN_ACTIVITY_PLAN_TABLE_ID_ENV,
                ],
                "fallback_rule": "读不到活动事实源时，不得声称已核实活动事实，只能请求小满或人补充。",
            }
        )

    fields = _extract_named_fields(raw, XIAOMAN_ACTIVITY_FIELD_NAMES)
    facts = {
        "活动内容": fields.get("活动内容"),
        "活动信息": fields.get("活动信息"),
        "计划地点": fields.get("计划地点"),
        "发起人": fields.get("发起人"),
        "计划时间": fields.get("计划时间"),
        "宣发判断": fields.get("宣发判断"),
        "小满运营状态": fields.get("小满运营状态"),
        "活动类型": fields.get("活动类型"),
        "素材照片": fields.get("素材照片"),
        "原文摘要": fields.get("原文摘要"),
    }
    missing: list[str] = []
    if not facts.get("活动内容"):
        missing.append("还不知道这是什么活动")
    if not facts.get("计划时间"):
        missing.append("还不知道活动具体时间")
    if not (facts.get("活动信息") or facts.get("计划地点")):
        missing.append("还不知道活动地点")
    if not facts.get("发起人"):
        missing.append("还不知道谁负责")
    if not facts.get("素材照片"):
        missing.append("还缺可以使用的照片或素材")

    return _json(
        {
            "success": True,
            "skill": "qintopia_xiaoman_activity_record_get",
            "source": {
                "base_token_env": XIAOMAN_ACTIVITY_BASE_TOKEN_ENV,
                "table_id_env": XIAOMAN_ACTIVITY_PLAN_TABLE_ID_ENV,
                "table_name": "活动计划表",
            },
            "record_id": record_id,
            "purpose": _clean_text(args.get("purpose"), max_len=300),
            "fields": fields,
            "facts": facts,
            "human_summary": (
                f"活动是“{facts.get('活动内容') or '未填写'}”；"
                f"时间是 {facts.get('计划时间') or '未填写'}；"
                f"地点信息是“{facts.get('活动信息') or facts.get('计划地点') or '未填写'}”；"
                f"发起人是“{facts.get('发起人') or '未填写'}”；"
                f"{'有 ' + str(len(facts['素材照片'])) + ' 个素材附件' if facts.get('素材照片') else '没有素材附件'}。"
            ),
            "missing_human_language": missing,
            "must_follow": [
                "后续判断必须以本工具返回的 facts 为准。",
                "不能把宣发判断=需要前宣理解成已授权对外发布。",
                "如果素材附件存在但下载失败，要说“有素材但权限/下载有问题”，不能说“没有素材”。",
            ],
        }
    )


def handle_qintopia_huabaosi_design_record_get(args: dict[str, Any], **_: Any) -> str:
    record_id = _clean_text(args.get("record_id"), max_len=80)
    if not re.fullmatch(r"rec[A-Za-z0-9]+", record_id or ""):
        return _json({"success": False, "error": "valid record_id is required"})
    try:
        base_token, table_id = _base_config(
            HUABAOSI_DESIGN_BASE_TOKEN_ENV, HUABAOSI_POSTER_TABLE_ID_ENV
        )
        raw = _feishu_base_record_get(base_token, table_id, record_id)
    except Exception as exc:
        return _json(
            {
                "success": False,
                "skill": "qintopia_huabaosi_design_record_get",
                "record_id": record_id,
                "error": str(exc),
                "required_env": [
                    "FEISHU_APP_ID or LARK_APP_ID",
                    "FEISHU_APP_SECRET or LARK_APP_SECRET",
                    HUABAOSI_DESIGN_BASE_TOKEN_ENV,
                    HUABAOSI_POSTER_TABLE_ID_ENV,
                ],
                "fallback_rule": "读不到设计产出库时，不得声称已核实生产状态或成品图。",
            }
        )

    fields = _extract_named_fields(raw, HUABAOSI_POSTER_FIELD_NAMES)
    status = fields.get("任务状态")
    finished = fields.get("成品图")
    source_material = fields.get("源素材")
    return _json(
        {
            "success": True,
            "skill": "qintopia_huabaosi_design_record_get",
            "source": {
                "base_token_env": HUABAOSI_DESIGN_BASE_TOKEN_ENV,
                "table_id_env": HUABAOSI_POSTER_TABLE_ID_ENV,
                "table_name": "海报生产任务表",
            },
            "record_id": record_id,
            "purpose": _clean_text(args.get("purpose"), max_len=300),
            "fields": fields,
            "facts": {
                "任务标题": fields.get("任务标题"),
                "任务编号": fields.get("任务编号"),
                "任务状态": status,
                "源素材": source_material,
                "成品图": finished,
                "发送结果": fields.get("发送结果"),
                "素材摘要": fields.get("素材摘要"),
                "生成 Prompt": fields.get("生成 Prompt"),
                "审核标记": fields.get("审核标记"),
                "来源会话/任务ID": fields.get("来源会话/任务ID"),
            },
            "human_summary": (
                f"生产记录“{fields.get('任务标题') or '未命名'}”，"
                f"任务编号 {fields.get('任务编号') or '未生成'}，"
                f"状态 {status or '未填写'}，"
                f"{'有成品图附件' if finished else '没有成品图附件'}，"
                f"{'有源素材附件' if source_material else '没有源素材附件'}。"
            ),
            "must_follow": [
                "只有成品图字段有附件时，才能声称已有可交付图片。",
                "任务状态为需补充信息时，必须把发送结果/素材摘要里的阻塞原因说清楚。",
                "不能用这条记录替代活动事实源；活动事实仍应读小满活动日记本。",
            ],
        }
    )


def check_requirements() -> bool:
    return True


def register(ctx) -> None:
    ctx.register_tool(
        name="qintopia_xiaoman_activity_record_get",
        toolset="qintopia",
        schema=QINTOPIA_XIAOMAN_ACTIVITY_RECORD_GET_SCHEMA,
        handler=handle_qintopia_xiaoman_activity_record_get,
        check_fn=check_requirements,
        description=QINTOPIA_XIAOMAN_ACTIVITY_RECORD_GET_SCHEMA["description"],
        emoji="🌾",
    )
    ctx.register_tool(
        name="qintopia_huabaosi_design_record_get",
        toolset="qintopia",
        schema=QINTOPIA_HUABAOSI_DESIGN_RECORD_GET_SCHEMA,
        handler=handle_qintopia_huabaosi_design_record_get,
        check_fn=check_requirements,
        description=QINTOPIA_HUABAOSI_DESIGN_RECORD_GET_SCHEMA["description"],
        emoji="🧾",
    )
