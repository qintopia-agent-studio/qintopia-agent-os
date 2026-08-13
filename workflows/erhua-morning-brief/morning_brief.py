#!/usr/bin/env python3
"""Erhua morning community brief generator."""
from __future__ import annotations

import argparse
import hashlib
import html
import importlib.util
import json
import os
import re
import shutil
import shlex
import subprocess
import sys
import tempfile
import urllib.error
import urllib.parse
import urllib.request
import uuid
import xml.etree.ElementTree as ET
from dataclasses import dataclass
from datetime import datetime
from pathlib import Path
from typing import Any
from zoneinfo import ZoneInfo, ZoneInfoNotFoundError


DEFAULT_TIMEZONE = "Asia/Shanghai"
DEFAULT_OPERATOR_NAME = "刘珊"
DEFAULT_AUDIENCE = "社区群成员"
DEFAULT_QUNMIND_TIMEOUT_SECONDS = 180
DEFAULT_NEWS_LIMIT = 5
DEFAULT_NEWS_FEED_TIMEOUT_SECONDS = 12
DEFAULT_NEWS_FEED_URLS = [
    "https://openai.com/news/rss.xml",
    "https://blog.google/technology/ai/rss/",
    "https://deepmind.google/blog/rss.xml",
]
NEWS_FEED_ALLOWED_HOSTS = frozenset({"openai.com", "blog.google", "deepmind.google"})
WORKFLOW_ID = "workflows/erhua-morning-brief"

_THIS = Path(__file__).resolve()
_VARIANT = _THIS.parents[2] / "skills" / "qintopia-tools" / "variants" / "xiaoman" / "__init__.py"


@dataclass(frozen=True)
class AiNewsItem:
    title: str
    summary: str
    title_zh: str = ""
    summary_zh: str = ""


class UnsafeNewsFeedXml(RuntimeError):
    pass


class NoNewsFeedRedirect(urllib.request.HTTPRedirectHandler):
    def redirect_request(self, req, fp, code, msg, headers, newurl):
        raise urllib.error.HTTPError(
            req.full_url,
            code,
            "news feed redirects are not allowed",
            headers,
            fp,
        )


def _parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(description="Erhua morning community brief")
    parser.add_argument("--date", help="Brief date (YYYY-MM-DD). Defaults to today.")
    parser.add_argument("--timezone", default=DEFAULT_TIMEZONE)
    parser.add_argument("--operator-name", default=DEFAULT_OPERATOR_NAME)
    parser.add_argument("--audience", default=DEFAULT_AUDIENCE)
    parser.add_argument("--activity-fixture", help="JSON fixture with Xiaoman announcement output.")
    parser.add_argument("--news-fixture", help="QunMind markdown fixture for tests or demos.")
    parser.add_argument(
        "--qunmind-bin",
        default=os.environ.get("QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_BIN", "qunmind"),
    )
    parser.add_argument(
        "--qunmind-config",
        default=os.environ.get("QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_CONFIG", ""),
    )
    parser.add_argument(
        "--qunmind-report-name",
        default=os.environ.get("QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_REPORT_NAME", ""),
    )
    parser.add_argument("--qunmind-timeout-seconds", type=int, default=DEFAULT_QUNMIND_TIMEOUT_SECONDS)
    parser.add_argument(
        "--news-feed-url",
        action="append",
        default=[],
        help="Public RSS or Atom feed URL used when QunMind is unavailable.",
    )
    parser.add_argument(
        "--news-feed-timeout-seconds",
        type=int,
        default=int(
            os.environ.get(
                "QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_FEED_TIMEOUT_SECONDS",
                DEFAULT_NEWS_FEED_TIMEOUT_SECONDS,
            )
        ),
    )
    parser.add_argument("--news-limit", type=int, default=DEFAULT_NEWS_LIMIT)
    parser.add_argument(
        "--allow-news-unavailable",
        action="store_true",
        help="Return a reviewable brief even when QunMind is unavailable.",
    )
    parser.add_argument(
        "--prepare-send-request",
        action="store_true",
        help="Build an AgentOS group_message_request payload for an approved text artifact.",
    )
    parser.add_argument(
        "--prepare-artifact",
        action="store_true",
        help="Build an AgentOS text_announcement artifact-create payload for the morning brief.",
    )
    parser.add_argument("--source-record-ref", default="")
    parser.add_argument("--artifact-title", default="")
    parser.add_argument("--artifact-summary", default="")
    parser.add_argument(
        "--approved-artifact-id",
        default="",
        help="Approved text_announcement artifact UUID required by --prepare-send-request.",
    )
    parser.add_argument("--target-group-alias", default="community_activity_group")
    parser.add_argument("--target-group-id", default="")
    parser.add_argument("--human-owner", default=DEFAULT_OPERATOR_NAME)
    parser.add_argument("--reviewer-id", default="<human-reviewer-id>")
    parser.add_argument("--confirmer-id", default="<human-confirmer-id>")
    parser.add_argument("--priority", choices=["low", "normal", "high", "urgent"], default="normal")
    parser.add_argument(
        "--sidecar-bin",
        default=os.environ.get("QINTOPIA_ERHUA_MORNING_BRIEF_SIDECAR_BIN", "qintopia-message-sidecar"),
    )
    parser.add_argument(
        "--execute-send-request",
        action="store_true",
        help="Execute operations-work-item-create for the prepared request. Defaults to --dry-run.",
    )
    parser.add_argument(
        "--execute-artifact-create",
        action="store_true",
        help="Execute operations-text-announcement-artifact-create. Defaults to --dry-run.",
    )
    parser.add_argument(
        "--apply-send-request",
        action="store_true",
        help="With --execute-send-request, create the awaiting_publish work item in AgentOS.",
    )
    parser.add_argument(
        "--apply-artifact-create",
        action="store_true",
        help="With --execute-artifact-create, create the pending text_announcement artifact.",
    )
    parser.add_argument(
        "--publish-plan",
        action="store_true",
        help="Include artifact review, send-request, final-confirmation, and send-ready command templates.",
    )
    parser.add_argument("--json", action="store_true", help="Print full JSON instead of the review message.")
    return parser.parse_args()


def _date_for(args: argparse.Namespace) -> str:
    if args.date:
        datetime.strptime(args.date, "%Y-%m-%d")
        return args.date
    try:
        zone = ZoneInfo(args.timezone)
    except ZoneInfoNotFoundError as exc:
        raise RuntimeError(f"unsupported timezone: {args.timezone}") from exc
    return datetime.now(zone).strftime("%Y-%m-%d")


def _load_xiaoman_variant():
    variant = _VARIANT
    if not variant.exists():
        raise RuntimeError(f"cannot locate reviewed xiaoman wrapper at {variant}")
    sys.path.insert(0, str(variant.parent))
    spec = importlib.util.spec_from_file_location("qintopia_xiaoman_wrapper", variant)
    if spec is None or spec.loader is None:
        raise RuntimeError(f"cannot load xiaoman wrapper at {variant}")
    module = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(module)
    return module


def _load_activity_fixture(path: str) -> dict[str, Any]:
    value = json.loads(Path(path).read_text(encoding="utf-8"))
    if not isinstance(value, dict):
        raise RuntimeError("activity fixture must be a JSON object")
    return value


def _prepare_activity(date: str, args: argparse.Namespace) -> dict[str, Any]:
    if args.activity_fixture:
        return _load_activity_fixture(args.activity_fixture)

    os.environ.setdefault("QINTOPIA_PROFILE_ID", "xiaoman")
    for env_key in (
        "QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE",
        "QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE",
        "QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE",
    ):
        if not os.environ.get(env_key):
            raise RuntimeError(f"{env_key} must be set in the runtime environment")

    module = _load_xiaoman_variant()
    result = json.loads(
        module.handle_qintopia_xiaoman_activity_announcement_prepare(
            {
                "date": date,
                "mode": "same_day_preview",
                "operator_name": args.operator_name,
                "community_audience": args.audience,
            }
        )
    )
    if not isinstance(result, dict):
        raise RuntimeError("xiaoman activity wrapper returned a non-object result")
    return result


def _run_qunmind_report(args: argparse.Namespace) -> str:
    if args.news_fixture:
        return Path(args.news_fixture).read_text(encoding="utf-8")
    if not _qunmind_available(args):
        raise RuntimeError("QunMind is not configured or installed")
    if args.qunmind_timeout_seconds <= 0:
        raise RuntimeError("qunmind timeout must be positive")

    with tempfile.TemporaryDirectory(prefix="erhua-morning-brief-") as temp_dir:
        output_path = Path(temp_dir) / "qunmind-public-ai-report.md"
        command = [args.qunmind_bin]
        if args.qunmind_config:
            command.extend(["--config", args.qunmind_config])
        command.extend(["daily-report", "--output", str(output_path), "--public-only"])
        if args.qunmind_report_name:
            command.extend(["--report-name", args.qunmind_report_name])
        completed = subprocess.run(
            command,
            check=False,
            capture_output=True,
            text=True,
            timeout=args.qunmind_timeout_seconds,
        )
        if completed.returncode != 0:
            raise RuntimeError("QunMind public daily report failed")
        if not output_path.exists():
            raise RuntimeError("QunMind public daily report did not create output")
        return output_path.read_text(encoding="utf-8")


def _qunmind_available(args: argparse.Namespace) -> bool:
    if args.news_fixture:
        return True
    configured_bin = os.environ.get("QINTOPIA_ERHUA_MORNING_BRIEF_QUNMIND_BIN", "")
    if configured_bin and Path(args.qunmind_bin).is_absolute():
        return os.access(args.qunmind_bin, os.X_OK)
    return shutil.which(args.qunmind_bin) is not None


def _feed_urls(args: argparse.Namespace) -> list[str]:
    if args.news_feed_url:
        candidates = args.news_feed_url
    else:
        env_value = os.environ.get("QINTOPIA_ERHUA_MORNING_BRIEF_NEWS_FEED_URLS", "")
        candidates = env_value.split(",") if env_value else DEFAULT_NEWS_FEED_URLS

    urls: list[str] = []
    for raw in candidates:
        url = raw.strip()
        if _is_allowed_news_feed_url(url):
            urls.append(url)
    return urls


def _is_allowed_news_feed_url(url: str) -> bool:
    parsed = urllib.parse.urlparse(url)
    return parsed.scheme == "https" and parsed.hostname in NEWS_FEED_ALLOWED_HOSTS


def _plain_feed_text(value: str) -> str:
    text = html.unescape(value)
    text = re.sub(r"<[^>]+>", " ", text)
    text = re.sub(r"\s+", " ", text)
    return _sanitize_public_line(text, max_len=180)


def _child_text(element: ET.Element, names: tuple[str, ...]) -> str:
    for name in names:
        child = element.find(name)
        if child is not None and child.text:
            return child.text
    return ""


def _extract_feed_news_items(xml_text: str, limit: int) -> list[AiNewsItem]:
    if limit <= 0:
        raise RuntimeError("news limit must be positive")
    if re.search(r"<!\s*(?:DOCTYPE|ENTITY)\b", xml_text, flags=re.IGNORECASE):
        raise UnsafeNewsFeedXml(
            "news feed XML must not contain DTD or entity declarations"
        )
    root = ET.fromstring(xml_text)
    feed_items = [
        *root.findall(".//channel/item"),
        *root.findall(".//{http://www.w3.org/2005/Atom}entry"),
    ]
    items: list[AiNewsItem] = []
    seen: set[str] = set()
    for item in feed_items:
        title = _plain_feed_text(
            _child_text(item, ("title", "{http://www.w3.org/2005/Atom}title"))
        )
        summary = _plain_feed_text(
            _child_text(
                item,
                (
                    "description",
                    "summary",
                    "content",
                    "{http://www.w3.org/2005/Atom}summary",
                    "{http://www.w3.org/2005/Atom}content",
                ),
            )
        )
        if not summary:
            summary = title
        key = title.casefold()
        if not title or key in seen:
            continue
        seen.add(key)
        item = _news_item_or_none(
            title=title[:90],
            summary=summary,
            strict_translation=False,
        )
        if item is None:
            continue
        items.append(item)
        if len(items) >= limit:
            break
    return items


def _fetch_feed_news_items(args: argparse.Namespace) -> list[AiNewsItem]:
    if args.news_feed_timeout_seconds <= 0:
        raise RuntimeError("news feed timeout must be positive")
    opener = urllib.request.build_opener(NoNewsFeedRedirect)
    collected: list[AiNewsItem] = []
    seen: set[str] = set()
    for url in _feed_urls(args):
        request = urllib.request.Request(
            url,
            headers={"User-Agent": "qintopia-erhua-morning-brief/1.0"},
        )
        try:
            with opener.open(request, timeout=args.news_feed_timeout_seconds) as response:
                final_url = response.geturl() if hasattr(response, "geturl") else url
                if not _is_allowed_news_feed_url(final_url):
                    continue
                xml_text = response.read(2 * 1024 * 1024).decode("utf-8", errors="replace")
            for item in _extract_feed_news_items(xml_text, args.news_limit):
                key = item.title.casefold()
                if key in seen:
                    continue
                seen.add(key)
                collected.append(item)
                if len(collected) >= args.news_limit:
                    return collected
        except (
            OSError,
            urllib.error.URLError,
            ET.ParseError,
            UnicodeError,
            UnsafeNewsFeedXml,
        ):
            continue
    return collected


def _strip_markdown(value: str) -> str:
    value = re.sub(r"\[([^\]]+)\]\([^)]+\)", r"\1", value)
    value = re.sub(r"https?://\S+", "", value)
    value = re.sub(r"`([^`]+)`", r"\1", value)
    value = value.replace("**", "").replace("__", "").replace("*", "")
    value = re.sub(r"\s+", " ", value)
    return value.strip(" -:：\t")


def _is_internal_marker(value: str) -> bool:
    lower = value.lower()
    return bool(
        re.search(r"\b(?:tbl|rec|vew)[A-Za-z0-9]{6,}\b", value)
        or re.search(r"(?:/home/ubuntu|/Users/[^ \n]+|/tmp|/var/tmp)/\S+", value)
        or "postgres://" in lower
        or "postgresql://" in lower
        or "access_token" in lower
        or "api_key" in lower
        or "client_secret" in lower
        or "bearer " in lower
    )


CHAT_FACING_FORBIDDEN_PHRASES = (
    "需要前置",
    "前置条件",
    "暂无需要宣发",
    "需要宣发",
    "可宣发",
    "宣发判断",
    "宣发状态",
    "计划类活动",
    "活动状态",
    "运营状态",
    "小满运营状态",
    "小满备注",
    "活动前提醒状态",
    "下周排期确认",
    "待确认（居民提交表单后默认）",
    "已确认-排入下周",
    "排入下周",
)

CHAT_FACING_FORBIDDEN_PATTERNS = (
    re.compile(r"(?:^|\n)\s*(?:状态|活动状态|宣发状态|宣发判断|运营状态|小满备注)[：:]", re.MULTILINE),
    re.compile(r"(?:^|\n)\s*(?:前置条件|需要前置|需前置)[：:]", re.MULTILINE),
    re.compile(r"(?:待确认|暂缓|已确认-排入下周)"),
)


def _validate_chat_facing_brief(value: str) -> None:
    for phrase in CHAT_FACING_FORBIDDEN_PHRASES:
        if phrase in value:
            raise RuntimeError("morning brief contains internal planning wording")
    for pattern in CHAT_FACING_FORBIDDEN_PATTERNS:
        if pattern.search(value):
            raise RuntimeError("morning brief contains internal planning wording")


def _sanitize_public_line(value: str, max_len: int = 180) -> str:
    clean = _strip_markdown(value)
    if not clean or _is_internal_marker(clean):
        return ""
    if len(clean) > max_len:
        clean = clean[: max_len - 1].rstrip(" ，,。") + "..."
    return clean


def _sanitize_public_text(value: str) -> str:
    lines = []
    for raw_line in value.splitlines():
        line = _sanitize_public_line(raw_line, max_len=220)
        if line:
            lines.append(line)
    return "\n".join(lines).strip()


def _contains_cjk(value: str) -> bool:
    return bool(re.search(r"[\u3400-\u4dbf\u4e00-\u9fff\uf900-\ufaff]", value))


def _looks_english_public_text(value: str) -> bool:
    if _contains_cjk(value):
        return False
    return len(re.findall(r"[A-Za-z]", value)) >= 8


def _needs_chinese_translation(item: AiNewsItem) -> bool:
    return _looks_english_public_text(item.title) or _looks_english_public_text(item.summary)


def _chinese_title_for(item: AiNewsItem) -> str:
    if item.title_zh:
        return item.title_zh
    return item.title if _contains_cjk(item.title) else ""


def _chinese_summary_for(item: AiNewsItem) -> str:
    if item.summary_zh:
        return item.summary_zh
    return item.summary if _contains_cjk(item.summary) else ""


def _news_item_or_none(
    *,
    title: str,
    summary: str,
    title_zh: str = "",
    summary_zh: str = "",
    strict_translation: bool,
) -> AiNewsItem | None:
    item = AiNewsItem(
        title=title,
        summary=summary,
        title_zh=title_zh,
        summary_zh=summary_zh,
    )
    if not _needs_chinese_translation(item):
        return item
    if _chinese_title_for(item) and _chinese_summary_for(item):
        return item
    if strict_translation:
        raise RuntimeError("English AI news item requires Chinese title and summary translation")
    return None


def _translation_line(raw_line: str) -> tuple[str, str]:
    line = _sanitize_public_line(raw_line, max_len=220)
    match = re.match(
        r"^(中文标题|标题翻译|译后标题|中文摘要|摘要翻译|译后摘要|中文要点|要点翻译|中文译文|译文|翻译|中文)[：:]\s*(.+)$",
        line,
        flags=re.IGNORECASE,
    )
    if not match:
        return "", ""
    return match.group(1), match.group(2).strip()


def _split_translated_title_summary(value: str) -> tuple[str, str]:
    for separator in ("：", ":"):
        if separator not in value:
            continue
        title, summary = value.split(separator, 1)
        title = title.strip()
        summary = summary.strip()
        if title and summary and len(title) <= 90:
            return title, summary
    return "", ""


def _translation_from_block(block: str) -> tuple[str, str]:
    title_zh = ""
    summary_zh = ""
    for raw_line in block.splitlines():
        label, value = _translation_line(raw_line)
        if not label or not value or not _contains_cjk(value):
            continue
        if label in {"中文标题", "标题翻译", "译后标题"}:
            title_zh = title_zh or _sanitize_public_line(value, max_len=90)
            continue
        if label in {"中文摘要", "摘要翻译", "译后摘要", "中文要点", "要点翻译", "中文译文", "译文", "翻译"}:
            summary_zh = summary_zh or _sanitize_public_line(value, max_len=180)
            continue
        if label == "中文":
            translated_title, translated_summary = _split_translated_title_summary(value)
            if translated_title and translated_summary:
                title_zh = title_zh or _sanitize_public_line(translated_title, max_len=90)
                summary_zh = summary_zh or _sanitize_public_line(translated_summary, max_len=180)
            else:
                summary_zh = summary_zh or _sanitize_public_line(value, max_len=180)
    return title_zh, summary_zh


def _extract_ai_section(markdown: str) -> str:
    lines = markdown.splitlines()
    start: int | None = None
    for index, line in enumerate(lines):
        if re.match(r"^##\s+.*AI\s*前沿", line, flags=re.IGNORECASE):
            start = index + 1
            break
    if start is None:
        for index, line in enumerate(lines):
            if re.match(r"^##\s+.*\bAI\b", line, flags=re.IGNORECASE):
                start = index + 1
                break
    if start is None:
        return ""

    end = len(lines)
    for index in range(start, len(lines)):
        if re.match(r"^##\s+", lines[index]):
            end = index
            break
    return "\n".join(lines[start:end]).strip()


def _summary_from_block(block: str) -> str:
    fallback = ""
    for raw_line in block.splitlines():
        stripped = raw_line.strip()
        if not stripped or stripped.startswith("#") or stripped.startswith("---"):
            continue
        if "原文入口" in stripped or "source index" in stripped.lower():
            continue
        if _translation_line(stripped)[0]:
            continue
        line = _sanitize_public_line(stripped)
        if not line:
            continue
        if re.search(r"(值得关注|为什么值得看|摘要|要点|一句话|summary|why it matters|key point)", stripped, flags=re.IGNORECASE):
            return re.sub(
                r"^(值得关注|为什么值得看|摘要|要点|一句话|summary|why it matters|key point)[：:]\s*",
                "",
                line,
                flags=re.IGNORECASE,
            )
        if not fallback:
            fallback = line
    return fallback


def _extract_ai_news_items(markdown: str, limit: int) -> list[AiNewsItem]:
    if limit <= 0:
        raise RuntimeError("news limit must be positive")
    section = _extract_ai_section(markdown)
    if not section:
        return []

    items: list[AiNewsItem] = []
    heading_matches = list(re.finditer(r"^###\s+(.+)$", section, flags=re.MULTILINE))
    for index, match in enumerate(heading_matches):
        title = _sanitize_public_line(match.group(1), max_len=90)
        if "｜" in title:
            title = title.split("｜", 1)[1].strip()
        block_start = match.end()
        block_end = heading_matches[index + 1].start() if index + 1 < len(heading_matches) else len(section)
        block = section[block_start:block_end]
        summary = _summary_from_block(block)
        title_zh, summary_zh = _translation_from_block(block)
        if title and summary:
            item = _news_item_or_none(
                title=title,
                summary=summary,
                title_zh=title_zh,
                summary_zh=summary_zh,
                strict_translation=True,
            )
            if item is not None:
                items.append(item)
        if len(items) >= limit:
            return items

    for raw_line in section.splitlines():
        stripped = raw_line.strip()
        if not stripped.startswith(("-", "*", "1.", "2.", "3.")):
            continue
        line = _sanitize_public_line(stripped, max_len=180)
        if not line:
            continue
        if "：" in line:
            title, summary = line.split("：", 1)
        elif ":" in line:
            title, summary = line.split(":", 1)
        else:
            title, summary = line, line
        title = title.strip(" -0123456789.")
        summary = summary.strip()
        if title and summary:
            item = _news_item_or_none(
                title=title[:90],
                summary=summary,
                strict_translation=True,
            )
            if item is not None:
                items.append(item)
        if len(items) >= limit:
            break
    return items


def _is_sunday(date: str) -> bool:
    return datetime.strptime(date, "%Y-%m-%d").date().weekday() == 6


def _activity_section(date: str, activity_result: dict[str, Any]) -> tuple[str, int, bool]:
    if not activity_result.get("success"):
        raise RuntimeError(f"activity preview failed: {activity_result.get('error', 'unknown error')}")
    count = activity_result.get("publishable_count", 0)
    if isinstance(count, bool) or not isinstance(count, int):
        raise RuntimeError("activity preview returned invalid publishable_count")

    raw_announcement = str(activity_result.get("announcement_text") or "")
    if count > 0:
        _validate_chat_facing_brief(raw_announcement)
    announcement = _sanitize_public_text(raw_announcement)
    if count <= 0:
        if _is_sunday(date):
            return (
                "这周暂时还没有可以直接报名的活动。\n"
                "昨天提醒过一次，今天早上二花再轻轻补提醒一下：如果你这周想发起共读、散步、"
                "吃饭、AI 工具试玩，直接在群里说主题、时间和地点。信息够了，二花就帮你整理成招募文案。",
                0,
                True,
            )
        return (
            "今天群里暂时没有安排好的活动。\n"
            "想组局的话，今天很适合发起一个小活动：共读、散步、吃饭、AI 工具试玩都可以。"
            "你在群里说一声，二花帮你把人喊起来。",
            0,
            False,
        )
    if not announcement:
        raise RuntimeError("activity preview returned empty announcement text")
    return announcement, count, False


def _news_item_lines(index: int, item: AiNewsItem) -> list[str]:
    if not _needs_chinese_translation(item):
        return [f"{index}. {item.title}：{item.summary}"]
    return [
        f"{index}. 英文：{item.title}：{item.summary}",
        f"   中文：{_chinese_title_for(item)}：{_chinese_summary_for(item)}",
    ]


def _compose_brief(
    *,
    date: str,
    activity_text: str,
    activity_count: int,
    news_items: list[AiNewsItem],
    news_unavailable: bool,
) -> str:
    lines = [
        f"早上好，二花早报来啦。今天是 {date}。",
        "",
        "今天活动：",
        activity_text,
        "",
        "AI 新闻：",
    ]
    if news_unavailable:
        lines.append("今天 QunMind 的公开新闻源暂时没读到，二花先不硬编。")
    else:
        for index, item in enumerate(news_items, start=1):
            lines.extend(_news_item_lines(index, item))
    lines.extend(
        [
            "",
            "想办活动的话，直接在群里说主题、时间和地点；信息够了，二花就帮你整理成招募文案。",
        ]
    )
    if activity_count <= 0:
        lines.append("今天没活动也没关系，群里的活动就是从一句“有人一起吗”开始的。")
    return "\n".join(lines).strip()


def _validate_uuid(value: str, field: str) -> str:
    try:
        return str(uuid.UUID(value))
    except ValueError as exc:
        raise RuntimeError(f"{field} must be a uuid") from exc


def _content_hash_for_text(value: str) -> str:
    return f"sha256:{hashlib.sha256(value.encode('utf-8')).hexdigest()}"


def _send_request_payload(
    *,
    date: str,
    message_text: str,
    approved_artifact_id: str,
    args: argparse.Namespace,
) -> dict[str, Any]:
    artifact_id = _validate_uuid(approved_artifact_id, "approved_artifact_id")
    target_group_alias = args.target_group_alias.strip()
    target_group_id = args.target_group_id.strip()
    if not target_group_alias and not target_group_id:
        raise RuntimeError("target_group_alias or target_group_id is required")

    source_record_ref = f"erhua_morning_brief:{date}"
    content_hash = _content_hash_for_text(message_text)
    idempotency_seed = hashlib.sha256(f"{source_record_ref}:{artifact_id}:{content_hash}".encode("utf-8")).hexdigest()[
        :24
    ]
    return {
        "requester_agent": "xiaoman",
        "target_agent": "erhua",
        "capability_key": "erhua.send_group_message",
        "work_item_type": "group_message_request",
        "brief_summary": f"{date} 二花早报发送请求",
        "purpose": "erhua_morning_brief_group_message",
        "human_owner": args.human_owner,
        "priority": args.priority,
        "source_type": "operations_workflow",
        "source_refs": {"source_record_ref": source_record_ref},
        "approved_artifact_id": artifact_id,
        "payload": {
            "workflow_type": "text_activity_announcement",
            "planner_intent": "send_erhua_morning_brief_after_final_confirmation",
            "approved_artifact_id": artifact_id,
            "approved_artifact_type": "text_announcement",
            "approved_artifact_content_hash": content_hash,
            "target_channel": "qiwe",
            "target_group_alias": target_group_alias or None,
            "target_group_id": target_group_id or None,
            "message_text": message_text,
            "send_executed": False,
        },
        "payload_redaction_policy": "summary_only",
        "metadata": {
            "workflow": WORKFLOW_ID,
            "brief_date": date,
            "requires_human_final_confirmation": True,
            "external_send_executed": False,
        },
        "idempotency_key": f"erhua_morning_brief:{idempotency_seed}",
    }


def _operations_create_command(args: argparse.Namespace, payload: dict[str, Any]) -> list[str]:
    payload_json = json.dumps(payload, ensure_ascii=False, separators=(",", ":"))
    return [
        args.sidecar_bin,
        "operations-work-item-create",
        "--payload-json",
        payload_json,
        "--apply" if args.apply_send_request else "--dry-run",
    ]


def _artifact_create_payload(
    *,
    date: str,
    message_text: str,
    args: argparse.Namespace,
) -> dict[str, Any]:
    source_record_ref = args.source_record_ref.strip() or f"erhua_morning_brief:{date}"
    return {
        "date": date,
        "message_text": message_text,
        "title": args.artifact_title.strip() or f"{date} 二花早报文本公告",
        "summary": args.artifact_summary.strip() or "二花早报待审核文本公告",
        "human_owner": args.human_owner,
        "priority": args.priority,
        "source_record_ref": source_record_ref,
        "metadata": {
            "workflow": WORKFLOW_ID,
            "brief_date": date,
            "external_send_executed": False,
        },
    }


def _artifact_create_command(args: argparse.Namespace, payload: dict[str, Any]) -> list[str]:
    payload_json = json.dumps(payload, ensure_ascii=False, separators=(",", ":"))
    return [
        args.sidecar_bin,
        "operations-text-announcement-artifact-create",
        "--payload-json",
        payload_json,
        "--apply" if args.apply_artifact_create else "--dry-run",
    ]


def _artifact_create_action(args: argparse.Namespace, payload: dict[str, Any]) -> dict[str, Any]:
    command = _artifact_create_command(args, payload)
    action: dict[str, Any] = {
        "payload": payload,
        "command": command,
        "shell_preview": " ".join(shlex.quote(part) for part in command),
        "execute_requested": args.execute_artifact_create,
        "apply_requested": args.apply_artifact_create,
        "external_send_executed": False,
    }
    if not args.execute_artifact_create:
        return action

    completed = subprocess.run(command, check=False, capture_output=True, text=True)
    action["returncode"] = completed.returncode
    action["stdout"] = completed.stdout
    action["stderr"] = completed.stderr
    if completed.returncode != 0:
        raise RuntimeError("operations-text-announcement-artifact-create failed")
    return action


def _send_request_action(args: argparse.Namespace, payload: dict[str, Any]) -> dict[str, Any]:
    command = _operations_create_command(args, payload)
    action: dict[str, Any] = {
        "payload": payload,
        "command": command,
        "shell_preview": " ".join(shlex.quote(part) for part in command),
        "execute_requested": args.execute_send_request,
        "apply_requested": args.apply_send_request,
        "external_send_executed": False,
    }
    if not args.execute_send_request:
        return action

    completed = subprocess.run(command, check=False, capture_output=True, text=True)
    action["returncode"] = completed.returncode
    action["stdout"] = completed.stdout
    action["stderr"] = completed.stderr
    if completed.returncode != 0:
        raise RuntimeError("operations-work-item-create failed for Erhua morning brief send request")
    return action


def _stdout_json_field(action: dict[str, Any] | None, field: str) -> str:
    if not action or not action.get("stdout"):
        return ""
    try:
        parsed = json.loads(str(action["stdout"]))
    except json.JSONDecodeError:
        return ""
    value = parsed.get(field)
    return value if isinstance(value, str) and value else ""


def _command_preview(command: list[str]) -> str:
    return " ".join(shlex.quote(part) for part in command)


def _artifact_review_command(args: argparse.Namespace, artifact_id: str) -> list[str]:
    payload = {
        "artifact_id": artifact_id,
        "reviewer_id": args.reviewer_id,
        "decision": "approved",
        "expected_artifact_type": "text_announcement",
        "expected_review_status": "pending",
        "reason": "二花早报文本确认发布",
        "source": "erhua_morning_brief_manual_release",
    }
    return [
        args.sidecar_bin,
        "operations-artifact-review-decision",
        "--payload-json",
        json.dumps(payload, ensure_ascii=False, separators=(",", ":")),
        "--apply",
    ]


def _group_message_confirm_command(args: argparse.Namespace, work_item_id: str) -> list[str]:
    payload = {
        "work_item_id": work_item_id,
        "confirmer_id": args.confirmer_id,
        "decision": "confirmed",
        "reason": "确认发送二花早报",
        "source": "erhua_morning_brief_manual_release",
    }
    return [
        args.sidecar_bin,
        "operations-group-message-confirm",
        "--payload-json",
        json.dumps(payload, ensure_ascii=False, separators=(",", ":")),
        "--apply",
    ]


def _send_ready_command(args: argparse.Namespace, work_item_id: str) -> list[str]:
    return [
        args.sidecar_bin,
        "run-group-message-send-worker",
        "--once",
        "--work-item-id",
        work_item_id,
        "--apply",
    ]


def _publish_plan(args: argparse.Namespace, result: dict[str, Any]) -> dict[str, Any]:
    artifact_id = _stdout_json_field(result.get("artifact_create"), "artifact_id") or "<text-announcement-artifact-uuid>"
    approved_artifact_id = args.approved_artifact_id.strip() or artifact_id
    work_item_id = _stdout_json_field(result.get("send_request"), "work_item_id") or "<group-message-request-work-item-uuid>"
    send_request_command = ""
    if result.get("send_request"):
        send_request_command = str(result["send_request"]["shell_preview"])
    elif approved_artifact_id != "<text-announcement-artifact-uuid>":
        send_payload = _send_request_payload(
            date=result["date"],
            message_text=result["morning_brief_text"],
            approved_artifact_id=approved_artifact_id,
            args=args,
        )
        send_request_command = _command_preview(_operations_create_command(args, send_payload))

    return {
        "manual_post_text": result["morning_brief_text"],
        "steps": [
            {
                "name": "create_or_preview_text_artifact",
                "command": result.get("artifact_create", {}).get(
                    "shell_preview",
                    "rerun with --prepare-artifact --execute-artifact-create --apply-artifact-create",
                ),
            },
            {
                "name": "approve_text_artifact",
                "artifact_id": artifact_id,
                "command": _command_preview(_artifact_review_command(args, artifact_id)),
            },
            {
                "name": "create_awaiting_publish_group_message_request",
                "approved_artifact_id": approved_artifact_id,
                "command": send_request_command
                or "rerun with --prepare-send-request --approved-artifact-id <approved-text-announcement-artifact-uuid>",
            },
            {
                "name": "final_confirm_group_message_request",
                "work_item_id": work_item_id,
                "command": _command_preview(_group_message_confirm_command(args, work_item_id)),
            },
            {
                "name": "record_send_ready",
                "work_item_id": work_item_id,
                "command": _command_preview(_send_ready_command(args, work_item_id)),
            },
            {
                "name": "manual_qiwe_post_if_adapter_unavailable",
                "command": "copy manual_post_text into the approved group channel",
            },
        ],
        "external_send_executed": False,
        "note": "send-ready only records AgentOS readiness; use the manual post text if no reviewed QiWe text sender is active.",
    }


def build_morning_brief(args: argparse.Namespace) -> dict[str, Any]:
    date = _date_for(args)
    activity_result = _prepare_activity(date, args)
    activity_text, activity_count, sunday_no_publishable_activity_followup = _activity_section(date, activity_result)

    news_unavailable = False
    news_items: list[AiNewsItem] = []
    ai_news_source = "qunmind_public_only"
    try:
        markdown = _run_qunmind_report(args)
        news_items = _extract_ai_news_items(markdown, args.news_limit)
        if not news_items:
            raise RuntimeError("QunMind report did not contain AI news items")
    except Exception:
        news_items = _fetch_feed_news_items(args)
        ai_news_source = "public_rss_fallback"
        if not news_items:
            if not args.allow_news_unavailable:
                raise
            news_unavailable = True
            ai_news_source = "unavailable"

    brief = _compose_brief(
        date=date,
        activity_text=activity_text,
        activity_count=activity_count,
        news_items=news_items,
        news_unavailable=news_unavailable,
    )
    _validate_chat_facing_brief(brief)
    result: dict[str, Any] = {
        "success": True,
        "workflow": WORKFLOW_ID,
        "date": date,
        "activity_publishable_count": activity_count,
        "sunday_no_publishable_activity_followup": sunday_no_publishable_activity_followup,
        "ai_news_item_count": len(news_items),
        "ai_news_source": ai_news_source,
        "morning_brief_text": brief,
        "operator_review_message": (
            "二花早报草稿已生成，未发送。\n\n"
            f"{brief}\n\n"
            "确认后才能进入单独审核过的 Erhua/QiWe 发送边界。"
        ),
        "requires_human_confirmation": True,
        "external_send_executed": False,
        "database_writes": False,
        "guardrails": [
            "reads Xiaoman activity preview only",
            "uses QunMind public-only daily report or public RSS fallback for AI news",
            "does not publish, call Erhua, call QiWe, create work items, or send by default",
        ],
    }
    if args.prepare_send_request:
        if not args.approved_artifact_id:
            raise RuntimeError("approved_artifact_id is required with --prepare-send-request")
        send_payload = _send_request_payload(
            date=date,
            message_text=brief,
            approved_artifact_id=args.approved_artifact_id,
            args=args,
        )
        result["send_request"] = _send_request_action(args, send_payload)
        result["database_writes"] = bool(
            result["database_writes"] or (args.execute_send_request and args.apply_send_request)
        )
        result["guardrails"].append(
            "prepared group_message_request remains awaiting_publish and requires final confirmation"
        )
    if args.prepare_artifact:
        artifact_payload = _artifact_create_payload(date=date, message_text=brief, args=args)
        result["artifact_create"] = _artifact_create_action(args, artifact_payload)
        result["database_writes"] = bool(
            result["database_writes"] or (args.execute_artifact_create and args.apply_artifact_create)
        )
        result["guardrails"].append("prepared text_announcement artifact remains pending until reviewed")
    if args.publish_plan:
        result["publish_plan"] = _publish_plan(args, result)
    return result


def main() -> int:
    args = _parse_args()
    try:
        result = build_morning_brief(args)
    except Exception as exc:
        print(f"ERROR: {exc}", file=sys.stderr)
        return 1

    if args.json:
        print(json.dumps(result, ensure_ascii=False, indent=2))
    else:
        print(result["operator_review_message"])
        if "send_request" in result:
            print("\n发送请求预览已生成，未发送：")
            print(result["send_request"]["shell_preview"])
        if "artifact_create" in result:
            print("\n文本 artifact 创建预览已生成，未发送：")
            print(result["artifact_create"]["shell_preview"])
        if "publish_plan" in result:
            print("\n明早发布计划：")
            for step in result["publish_plan"]["steps"]:
                print(f"- {step['name']}: {step['command']}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
