"""LLM narrative layer for the Xiaoman daily report.

This module adds an *optional* storytelling stage on top of the deterministic
report pipeline. It does NOT replace the deterministic clustering/character
logic; instead it uses that structured output (cases, characters, real quotes)
as grounding so the model can only retell what actually happened in the group.

Design boundaries (mirrors the reference wx-cli project):
- The LLM only writes; it never invents facts. Every narrative claim must trace
  back to the grounding block built from real messages.
- This is opt-in via `--narrative roast|normal`. When unset, the pipeline stays
  fully deterministic and zero-LLM (legacy behavior preserved).
- Image grounding is gated behind `--narrative-with-images` and a reviewed image
  source. It is NOT enabled by default because the production privacy boundary
  currently forbids reading attachments. See `_extract_image_grounding`.
"""

from __future__ import annotations

import json
import os
import re
from dataclasses import dataclass
from typing import Any

import httpx

# Environment variable chains follow the repo convention used in
# skills/qintopia-tools: project-specific first, then OPENAI_* fallback.
_LLM_BASE_URL_ENVS = (
    "QINTOPIA_LLM_BASE_URL",
    "QINTOPIA_XIAOMAN_LLM_BASE_URL",
    "OPENAI_BASE_URL",
)
_LLM_API_KEY_ENVS = (
    "QINTOPIA_LLM_API_KEY",
    "QINTOPIA_XIAOMAN_LLM_API_KEY",
    "OPENAI_API_KEY",
)
_LLM_MODEL_ENVS = (
    "QINTOPIA_LLM_MODEL",
    "QINTOPIA_XIAOMAN_LLM_MODEL",
    "OPENAI_MODEL",
)

DEFAULT_MODEL = "gpt-4o-mini"


def _session_env(name: str) -> str:
    value = os.environ.get(name, "")
    return value.strip()


@dataclass
class NarrativeConfig:
    base_url: str
    api_key: str
    model: str
    temperature: float = 0.8
    max_tokens: int = 4000

    @classmethod
    def from_env(cls, *, base_url: str | None = None, api_key: str | None = None,
                 model: str | None = None, temperature: float = 0.8) -> "NarrativeConfig":
        resolved_base = base_url or next((_session_env(v) for v in _LLM_BASE_URL_ENVS if _session_env(v)), "")
        resolved_key = api_key or next((_session_env(v) for v in _LLM_API_KEY_ENVS if _session_env(v)), "")
        resolved_model = model or next((_session_env(v) for v in _LLM_MODEL_ENVS if _session_env(v)), DEFAULT_MODEL)
        if not resolved_base or not resolved_key:
            raise RuntimeError(
                "narrative generation requires an OpenAI-compatible endpoint; "
                "set one of " + "/".join(_LLM_BASE_URL_ENVS) + " and " + "/".join(_LLM_API_KEY_ENVS)
            )
        return cls(
            base_url=resolved_base.rstrip("/"),
            api_key=resolved_key,
            model=resolved_model,
            temperature=temperature,
        )


# ---------------------------------------------------------------------------
# Grounding extraction (deterministic -> facts the model may use)
# ---------------------------------------------------------------------------

_QUOTE_RE = re.compile(r"^[\s>\"'「『]|^\[?引用|转述", re.MULTILINE)


def _clean_quote(text: str, limit: int = 90) -> str:
    text = re.sub(r"\s+", " ", text).strip()
    return text[:limit]


def build_grounding(report: Any) -> dict:
    """Turn the deterministic report into a structured, fact-only grounding block."""
    cases = []
    for case in getattr(report, "cases", []) or []:
        cases.append({
            "case_no": getattr(case, "case_no", ""),
            "title": getattr(case, "title", ""),
            "time_label": getattr(case, "time_label", ""),
            "summary": _clean_quote(getattr(case, "summary", ""), 160),
            "message_count": getattr(case, "message_count", 0),
            "participant_count": getattr(case, "participant_count", 0),
            "top_speaker": getattr(case, "top_speaker", ""),
            "bullets": [b for b in (getattr(case, "bullets", []) or [])][:8],
        })

    characters = []
    for ch in getattr(report, "characters", []) or []:
        characters.append({
            "name": getattr(ch, "name", ""),
            "role_label": getattr(ch, "role_label", ""),
            "one_liner": getattr(ch, "one_liner", ""),
            "story_function": getattr(ch, "story_function", ""),
            "evidence": _clean_quote(getattr(ch, "evidence", ""), 120),
        })

    hot_topics = [
        {"keyword": getattr(t, "keyword", ""), "message_count": getattr(t, "message_count", 0),
         "participant_count": getattr(t, "participant_count", 0)}
        for t in (getattr(report, "hot_topics", []) or [])
    ]

    # Pull short, verbatim quotes from raw messages for traceability.
    quotes = []
    for m in getattr(report, "messages", []) or []:
        text = getattr(m, "text", "") or ""
        if len(text) < 6 or len(text) > 40:
            continue
        if _QUOTE_RE.match(text):
            continue
        name = getattr(m, "sender_name", "") or getattr(m, "sender_id", "") or "?"
        quotes.append({"speaker": name, "text": _clean_quote(text, 40)})
        if len(quotes) >= 40:
            break

    return {
        "group_name": getattr(report, "group_name", ""),
        "report_date": getattr(report, "report_date", ""),
        "time_range": getattr(report, "time_range", ""),
        "message_count": getattr(report, "message_count", 0),
        "participant_count": getattr(report, "participant_count", 0),
        "cases": cases,
        "characters": characters,
        "hot_topics": hot_topics,
        "quotes": quotes,
    }


def format_grounding_markdown(grounding: dict) -> str:
    """Render grounding as a compact, auditable markdown block for the prompt."""
    lines = [
        f"群名：{grounding['group_name']}",
        f"日期：{grounding['report_date']}（{grounding['time_range']}）",
        f"消息总量：{grounding['message_count']} 条，活跃：{grounding['participant_count']} 人",
        "",
        "## 今日主线（确定性聚类结果，事实来源）",
    ]
    for c in grounding["cases"]:
        lines.append(
            f"- {c['case_no']} {c['title']}（{c['time_label']}）：{c['message_count']} 条 / {c['participant_count']} 人参与，牵头 {c['top_speaker']}"
        )
        if c["summary"]:
            lines.append(f"  - 摘要：{c['summary']}")
        for b in c["bullets"][:3]:
            lines.append(f"  - {_clean_quote(b, 80)}")
    lines.append("")
    lines.append("## 今日人物（确定性角色标签，事实来源）")
    for ch in grounding["characters"]:
        lines.append(f"- {ch['name']}（{ch['role_label']}）：{ch['one_liner']}")
    lines.append("")
    lines.append("## 真实语录（可直接引用的原文片段）")
    for q in grounding["quotes"]:
        lines.append(f"- {q['speaker']}：「{q['text']}」")
    return "\n".join(lines)


# ---------------------------------------------------------------------------
# Image grounding (gated; see module docstring on privacy boundary)
# ---------------------------------------------------------------------------

def extract_image_grounding(report: Any, reviewed_image_dir: str | None = None) -> list[dict]:
    """Build `![[...]]` image candidates for the narrative.

    IMPORTANT: this is intentionally NOT wired into the default path. The
    production privacy boundary forbids reading attachments/media, so image
    grounding only activates when:
      1. `--narrative-with-images` is passed, AND
      2. `reviewed_image_dir` points at a reviewed, safe-to-publish image set.
    Until both hold, callers must pass reviewed_image_dir=None and the result
    is empty.
    """
    if not reviewed_image_dir:
        return []
    candidates = []
    try:
        for entry in sorted(os.listdir(reviewed_image_dir)):
            if entry.lower().endswith((".png", ".jpg", ".jpeg", ".webp")):
                candidates.append({
                    "src": f"{reviewed_image_dir.rstrip('/')}/{entry}",
                    "caption": "",
                })
    except OSError:
        return []
    return candidates[:6]


# ---------------------------------------------------------------------------
# LLM call
# ---------------------------------------------------------------------------

ROAST_SYSTEM_PROMPT = """你是一位社区报纸的吐槽版主笔，为微信群「秦托邦的小伙伴（新）」撰写每日吐槽日报。
风格要求（严格遵循，这是产品定位）：
- 像读报纸专栏：有起承转合、有包袱、有金句，不是会议纪要，不是数据摘要。
- 标题/章节标题本身就是观点或玩笑，不是干瘪的话题名。
- 每章至少一句"转折句"（读者以为要夸时拆穿），至少一句可单独提取的金句。
- 用真实群聊的语录和细节，不要编造人名、事件或对话。
- 语气可以毒舌，但只调侃群体行为/语言现象，绝不评价外貌、健康、职业、收入、年龄、性别、地域、身份属性。
- 可以出现发言榜，但如果当天故事性强，优先讲故事，榜单可省略。
- 输出纯 Markdown，结构见用户指令中的模板。"""

ROAST_USER_TEMPLATE = """以下是今日群聊的确定性聚类结果（全部为事实，作为你叙事的锚点，不得脱离）：

{grounding}

请按下面的 roast 模板写出今天的吐槽日报 Markdown：

```
# 秦托邦吐槽日报 | {date} | {{副标题：当天最大荒诞事件的一句话包袱}}

**战报**：{{消息数}}条消息，{{发言人数}}人开口。{{一句话概括当天荒诞指数或群体行为模式}}

---

## 第一章：{{章节标题——观点或包袱，不是话题名}}

{{叙事段落：前两句正经预期，第三句拐弯；中间展开细节，用角色或对话链；至少一句转折句+一句金句}}

![[{{图片路径|可选宽}}]]
_{{发送者} {时间} — {一句话 caption，本身是吐槽或梗}_

---

## 第N章：{{同上}}

{{每篇 4-7 章，按时间或话题组织；最后一章可以是"明日线索"或"今日金句"}}

---

## 今日人物速写

> **{{人物名}}**
> {{一句话角色定位}}。{{当天最能代表这个人的一个梗或行为}}。

## 今日金句

**"{{引用原文}}"** ——{{发言者}}，{{为什么是今日最佳}}

---

*秦托邦 · 小满吐槽日报。所有引用可回溯至当天 quote-map。*
```

只输出 Markdown，不要解释。"""


NORMAL_SYSTEM_PROMPT = """你为微信群「秦托邦的小伙伴（新）」撰写每日内部日报，风格像本地生活故事，
不像会议纪要或摘要报告。用真实群聊细节，不编造。输出纯 Markdown，结构见用户指令。"""

NORMAL_USER_TEMPLATE = """以下是今日群聊的确定性聚类结果（全部为事实，作为叙事锚点，不得脱离）：

{grounding}

请写出今天的内部日报 Markdown：以「今日一句话」开头，下面用 3-5 个有故事性的章节组织，
每章可引用真实语录。可以省略发言榜，优先讲故事。只输出 Markdown。"""


def _chat_completion(config: NarrativeConfig, system: str, user: str) -> str:
    # config.base_url already includes the provider prefix (e.g. .../v1), so the
    # chat completions path is just /chat/completions appended.
    url = f"{config.base_url}/chat/completions"
    headers = {
        "Authorization": f"Bearer {config.api_key}",
        "Content-Type": "application/json",
    }
    payload = {
        "model": config.model,
        "messages": [
            {"role": "system", "content": system},
            {"role": "user", "content": user},
        ],
        "temperature": config.temperature,
        "max_tokens": config.max_tokens,
    }
    with httpx.Client(timeout=120.0) as client:
        resp = client.post(url, json=payload, headers=headers)
        if resp.status_code == 400:
            # Some OpenAI-compatible proxies reject the `system` role. Fold the
            # system instructions into the first user turn and retry once.
            retry_payload = dict(payload)
            retry_payload["messages"] = [
                {"role": "user", "content": f"{system}\n\n{user}"}
            ]
            resp = client.post(url, json=retry_payload, headers=headers)
        resp.raise_for_status()
        data = resp.json()
    if "choices" not in data or not data["choices"]:
        raise RuntimeError(f"no choices in response: {data.keys()}")
    message = data["choices"][0].get("message", {})
    if "content" not in message:
        raise RuntimeError(f"no content in message; keys={list(message.keys())}; response_keys={list(data.keys())}")
    return message["content"].strip()


def generate_narrative(style: str, report: Any, config: NarrativeConfig,
                       reviewed_image_dir: str | None = None) -> str:
    """Generate a narrative markdown from the deterministic report.

    `style` is one of "roast" / "normal". Returns the markdown string.
    """
    grounding = build_grounding(report)
    grounding_md = format_grounding_markdown(grounding)

    if style == "roast":
        system = ROAST_SYSTEM_PROMPT
        # Only swap the two real placeholders; every other {…} in the template is
        # a literal example the model should see, so we must NOT use .format().
        user = (
            ROAST_USER_TEMPLATE
            .replace("{grounding}", grounding_md)
            .replace("{date}", grounding["report_date"])
        )
    else:
        system = NORMAL_SYSTEM_PROMPT
        user = NORMAL_USER_TEMPLATE.replace("{grounding}", grounding_md)

    # Optional image grounding (gated by privacy boundary).
    images = extract_image_grounding(report, reviewed_image_dir)
    if images:
        image_block = "\n\n可选配图（仅当与叙事强相关时使用，caption 本身也要是吐槽或梗）：\n" + "\n".join(
            f"![[{img['src']}|150]]\n_{img['caption'] or '（待补 caption）'}_" for img in images
        )
        user = user + image_block

    return _chat_completion(config, system, user)
