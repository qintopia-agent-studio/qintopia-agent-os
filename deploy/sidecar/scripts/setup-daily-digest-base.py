#!/usr/bin/env python3
"""Initialize the Xiaoman daily community event radar Feishu Base.

The script is idempotent: it reuses existing tables/fields and only creates
missing structure. It reads the Xiaoman Feishu app credentials from the profile
env file; secrets are never printed.
"""

from __future__ import annotations

import argparse
import json
import os
import sys
import time
import urllib.error
import urllib.request
from pathlib import Path
from typing import Any, NoReturn


DEFAULT_BASE_TOKEN = "QIlqblr9oamGlWsx3frcl5WGnvh"
DEFAULT_DAILY_TABLE_ID = "tblcDfEdBTke4voG"
DEFAULT_PROFILE_ENV = "/home/ubuntu/.hermes/profiles/xiaoman/.env"


def load_profile_env(path: str) -> tuple[str, str]:
    env_path = Path(path)
    if not env_path.exists():
        raise SystemExit(f"profile env not found: {path}")
    values: dict[str, str] = {}
    for line in env_path.read_text(encoding="utf-8").splitlines():
        line = line.strip()
        if not line or line.startswith("#") or "=" not in line:
            continue
        key, value = line.split("=", 1)
        values[key.strip()] = value.strip().strip("\"'")
    app_id = values.get("FEISHU_APP_ID") or values.get("LARK_APP_ID") or values.get("APP_ID")
    app_secret = (
        values.get("FEISHU_APP_SECRET")
        or values.get("LARK_APP_SECRET")
        or values.get("APP_SECRET")
    )
    if not app_id or not app_secret:
        raise SystemExit("missing FEISHU_APP_ID/FEISHU_APP_SECRET in profile env")
    return app_id, app_secret


def request(method: str, url: str, token: str | None = None, body: Any | None = None) -> Any:
    headers = {"Content-Type": "application/json; charset=utf-8"}
    if token:
        headers["Authorization"] = "Bearer " + token
    data = json.dumps(body, ensure_ascii=False).encode("utf-8") if body is not None else None
    req = urllib.request.Request(url, data=data, headers=headers, method=method)
    try:
        with urllib.request.urlopen(req, timeout=30) as resp:
            text = resp.read().decode("utf-8")
            return json.loads(text) if text else {}
    except urllib.error.HTTPError as exc:
        text = exc.read().decode("utf-8", errors="replace")
        try:
            payload = json.loads(text)
        except Exception:
            payload = text
        raise RuntimeError(
            json.dumps(
                {"status": exc.code, "method": method, "url": url, "payload": payload},
                ensure_ascii=False,
                indent=2,
            )
        ) from exc


def fail_with_api_error(error: RuntimeError) -> NoReturn:
    text = str(error)
    try:
        details = json.loads(text)
    except Exception:
        raise error
    payload = details.get("payload")
    if not isinstance(payload, dict):
        raise error
    permission_violations = (
        payload.get("error", {}).get("permission_violations", [])
        if isinstance(payload.get("error"), dict)
        else []
    )
    required_scopes = sorted(
        {
            item.get("subject")
            for item in permission_violations
            if isinstance(item, dict) and item.get("type") == "action_scope_required"
        }
    )
    summary = {
        "ok": False,
        "status": details.get("status"),
        "method": details.get("method"),
        "url": details.get("url"),
        "code": payload.get("code"),
        "msg": payload.get("msg"),
        "required_scopes": required_scopes,
        "log_id": payload.get("error", {}).get("log_id")
        if isinstance(payload.get("error"), dict)
        else None,
    }
    print(json.dumps(summary, ensure_ascii=False, indent=2), file=sys.stderr)
    raise SystemExit(1)


class BaseSetup:
    def __init__(self, base_token: str, token: str, dry_run: bool):
        self.base_token = base_token
        self.token = token
        self.dry_run = dry_run
        self.api = f"https://open.feishu.cn/open-apis/bitable/v1/apps/{base_token}"
        self.changes: list[dict[str, Any]] = []

    def call(self, method: str, path: str, body: Any | None = None) -> Any:
        if self.dry_run and method != "GET":
            self.changes.append({"dry_run": method, "path": path, "body": body})
            return {}
        response = request(method, self.api + path, self.token, body)
        if response.get("code") != 0:
            raise RuntimeError(json.dumps(response, ensure_ascii=False, indent=2))
        return response

    def tables(self) -> list[dict[str, Any]]:
        return self.call("GET", "/tables?page_size=100").get("data", {}).get("items", [])

    def fields(self, table_id: str) -> list[dict[str, Any]]:
        if self.dry_run and table_id.startswith("<dry-run:"):
            return []
        return self.call("GET", f"/tables/{table_id}/fields?page_size=100").get("data", {}).get(
            "items", []
        )

    def views(self, table_id: str) -> list[dict[str, Any]]:
        if self.dry_run and table_id.startswith("<dry-run:"):
            return []
        return self.call("GET", f"/tables/{table_id}/views?page_size=200").get("data", {}).get(
            "items", []
        )

    def ensure_table(self, name: str, primary_field: str, fixed_table_id: str | None = None) -> str:
        for table in self.tables():
            if fixed_table_id and table.get("table_id") == fixed_table_id:
                if table.get("name") != name:
                    self.call("PATCH", f"/tables/{fixed_table_id}", {"name": name})
                    self.changes.append({"rename_table": fixed_table_id, "name": name})
                return fixed_table_id
            if table.get("name") == name:
                return table["table_id"]
        body = {
            "table": {
                "name": name,
                "default_view_name": "全部",
                "fields": [{"field_name": primary_field, "type": 1}],
            }
        }
        response = self.call("POST", "/tables", body)
        table_id = (
            response.get("data", {}).get("table_id")
            or response.get("data", {}).get("table", {}).get("table_id")
            or f"<dry-run:{name}>"
        )
        self.changes.append({"create_table": name, "table_id": table_id})
        return table_id

    def ensure_field(self, table_id: str, spec: dict[str, Any]) -> None:
        existing = self.fields(table_id)
        if any(field.get("field_name") == spec["field_name"] for field in existing):
            return
        self.call("POST", f"/tables/{table_id}/fields", spec)
        self.changes.append({"create_field": spec["field_name"], "table_id": table_id})
        time.sleep(0.08)

    def rename_primary_text(self, table_id: str) -> None:
        for field in self.fields(table_id):
            if field.get("field_name") == "文本" and field.get("is_primary"):
                self.call(
                    "PUT",
                    f"/tables/{table_id}/fields/{field['field_id']}",
                    {"field_name": "标题", "type": 1},
                )
                self.changes.append({"rename_field": "文本->标题", "table_id": table_id})

    def ensure_view(self, table_id: str, name: str) -> None:
        if any(view.get("view_name") == name for view in self.views(table_id)):
            return
        self.call("POST", f"/tables/{table_id}/views", {"view_name": name, "view_type": "grid"})
        self.changes.append({"create_view": name, "table_id": table_id})


def text(name: str) -> dict[str, Any]:
    return {"field_name": name, "type": 1}


def number(name: str) -> dict[str, Any]:
    return {"field_name": name, "type": 2, "property": {"formatter": "0"}}


def date(name: str) -> dict[str, Any]:
    return {"field_name": name, "type": 5, "property": {"date_formatter": "yyyy-MM-dd"}}


def datetime(name: str) -> dict[str, Any]:
    return {"field_name": name, "type": 5, "property": {"date_formatter": "yyyy-MM-dd HH:mm"}}


def url(name: str) -> dict[str, Any]:
    return {"field_name": name, "type": 15}


def checkbox(name: str) -> dict[str, Any]:
    return {"field_name": name, "type": 7}


def select(name: str, options: list[str]) -> dict[str, Any]:
    return {
        "field_name": name,
        "type": 3,
        "property": {
            "options": [{"name": option, "color": index % 7} for index, option in enumerate(options)]
        },
    }


def link(name: str, target_table_id: str, _target_table_name: str) -> dict[str, Any]:
    return {
        "field_name": name,
        "type": 18,
        "property": {
            "table_id": target_table_id,
            "multiple": True,
        },
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--base-token", default=DEFAULT_BASE_TOKEN)
    parser.add_argument("--daily-table-id", default=DEFAULT_DAILY_TABLE_ID)
    parser.add_argument("--profile-env", default=DEFAULT_PROFILE_ENV)
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--dry-run", action="store_true")
    args = parser.parse_args()
    if args.apply and args.dry_run:
        parser.error("use either --apply or --dry-run")
    dry_run = not args.apply

    app_id, app_secret = load_profile_env(args.profile_env)
    token_response = request(
        "POST",
        "https://open.feishu.cn/open-apis/auth/v3/tenant_access_token/internal",
        body={"app_id": app_id, "app_secret": app_secret},
    )
    if token_response.get("code") != 0:
        raise SystemExit(json.dumps(token_response, ensure_ascii=False, indent=2))
    setup = BaseSetup(args.base_token, token_response["tenant_access_token"], dry_run)

    try:
        daily = setup.ensure_table("日报总表", "标题", args.daily_table_id)
        setup.rename_primary_text(daily)
        archive = setup.ensure_table("文档归档表", "文档标题")
        signal = setup.ensure_table("事件信号表", "事件标题")

        for spec in [
            text("digest_id"),
            date("日报日期"),
            text("群名称"),
            text("群ID"),
            url("飞书文档链接"),
            text("文档 token"),
            text("Markdown 摘要"),
            text("Markdown SHA256"),
            select("发布状态", ["待发布", "已发布", "发布失败"]),
            text("发布错误"),
            select("发布人/Agent", ["xiaoman"]),
            datetime("发布时间"),
            datetime("创建时间"),
        ]:
            setup.ensure_field(archive, spec)

        for spec in [
            date("日报日期"),
            text("群名称"),
            text("群ID"),
            select("Owner Agent", ["xiaoman"]),
            text("今日摘要"),
            number("消息数"),
            number("有效信号数"),
            link("日报文档", archive, "文档归档表"),
            select("发布状态", ["待发布", "已发布", "发布失败"]),
            select("生成状态", ["已生成", "生成失败"]),
            text("digest_id"),
            datetime("生成时间"),
            datetime("更新时间"),
            text("备注"),
        ]:
            setup.ensure_field(daily, spec)

        for spec in [
            link("关联日报", daily, "日报总表"),
            date("事件日期"),
            text("群名称"),
            text("群ID"),
            select(
                "信号类型",
                [
                    "活动/聚会",
                    "服务/设施",
                    "未回答问题",
                    "高频问题",
                    "内容线索",
                    "成员故事",
                    "FAQ/SOP",
                    "风险提示",
                ],
            ),
            text("事件摘要"),
            text("相关成员"),
            select("建议负责人", ["小满", "小管家", "四老师", "画报司", "关二爷", "大总管", "文渊阁"]),
            select(
                "建议 Agent",
                ["xiaoman", "xiaoguanjia", "silaoshi", "huabaosi", "guanerye", "default", "wenyuange"],
            ),
            select("优先级", ["高", "中", "低"]),
            select("处理状态", ["待处理", "跟进中", "已完成", "已关闭", "暂不处理"]),
            datetime("截止时间"),
            text("证据摘要"),
            text("source_message_ids"),
            select("风险级别", ["无", "低", "中", "高"]),
            checkbox("是否适合对外发布"),
            select("外部发布状态", ["未评估", "可规划", "已发布", "不发布"]),
            datetime("创建时间"),
            datetime("更新时间"),
        ]:
            setup.ensure_field(signal, spec)

        for name in ["最近 7 天", "待发布", "发布失败", "按群分组"]:
            setup.ensure_view(daily, name)
        for name in [
            "待小满处理",
            "待小管家处理",
            "待画报司处理",
            "高优先级未处理",
            "活动和聚会",
            "服务/设施问题",
            "内容线索",
            "FAQ/SOP 候选",
            "已完成归档",
        ]:
            setup.ensure_view(signal, name)
        for name in ["最近 30 天", "发布失败", "按群分组"]:
            setup.ensure_view(archive, name)
    except RuntimeError as error:
        fail_with_api_error(error)

    print(
        json.dumps(
            {
                "dry_run": dry_run,
                "base_token": args.base_token,
                "daily_table_id": daily,
                "signal_table_id": signal,
                "archive_table_id": archive,
                "changes": setup.changes,
            },
            ensure_ascii=False,
            indent=2,
        )
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
