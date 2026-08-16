"""Shared data models and constants for the Xiaoman daily case-report pipeline."""
from __future__ import annotations

import re
from dataclasses import dataclass, field
from datetime import datetime
from typing import Any


# ---------------------------------------------------------------------------
# Defaults
# ---------------------------------------------------------------------------

DEFAULT_GROUP_NAME = "秦托邦的小伙伴（新）"
DEFAULT_REPORT_TITLE = "小满群聊日报"
CHAT_ID_ENV = "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID"
DEFAULT_TIMEZONE = "Asia/Shanghai"
DEFAULT_OUTPUT_WIDTH = 750
DEFAULT_CASE_LIMIT = 6
PRODUCTION_PSQL_BIN = "/usr/bin/psql"
PRODUCTION_PSQL_PATH = "/usr/bin:/bin"
DEFAULT_SUSPECT_LIMIT = 5
DEFAULT_CHARACTER_LIMIT = 4
DEFAULT_HOURLY_BUCKETS = 24
DEFAULT_WINDOW_HOURS = 24
DEFAULT_MIN_CASE_MESSAGES = 3
DEFAULT_TOP_KEYWORDS = 18
DEFAULT_HOT_TOPIC_LIMIT = 4
DEFAULT_HOT_TOPIC_MIN_MESSAGES = 2
DEFAULT_HOT_TOPIC_MIN_CHARS = 3
DEFAULT_HOT_TOPIC_MAX_CHARS = 8
DEFAULT_IMAGE_FORMAT = "jpeg"
DEFAULT_JPEG_QUALITY = 92
DEFAULT_TEMPLATE = "roast-long-image"
NEWSPAPER_ELEGANT_TEMPLATE = "newspaper-elegant"
ROAST_LONG_IMAGE_TEMPLATE = "roast-long-image"
TEMPLATE_VERSION = "xiaoman-daily-case-report-v5-roast-long-image"
MEMORY_LOOKBACK_DAYS = 90
REVIEW_DRAFT_REVIEWED_BY = "xiaoman-daily-case-report-review-draft"

# ---------------------------------------------------------------------------
# Text-analysis sets
# ---------------------------------------------------------------------------

STOP_WORDS: set[str] = {
    "这个", "那个", "然后", "就是", "什么", "怎么", "还是", "可以", "今天",
    "明天", "现在", "已经", "没有", "但是", "因为", "所以", "一下", "大家",
    "我们", "你们", "他们", "自己", "这里", "那里", "这样", "那样", "一个",
    "不是", "不用", "不要", "应该", "可能", "需要", "觉得", "看看", "一下",
    "哈哈", "嘿嘿", "嗯嗯", "好的", "收到", "谢谢", "请问", "知道", "真的",
    "一下", "一直", "一下", "时候", "过来", "过去", "为了", "作为", "关于",
    "还是", "或者", "以及", "并且", "虽然", "尽管", "不过", "只是", "而且",
    "国家", "规定", "词元", "哇喔", "名字", "好帅", "很帅",
    "呲牙", "哈哈", "哈哈哈", "哈哈哈哈", "啧啧", "啧啧啧", "欢迎欢迎",
}

PROMOTIONAL_NOISE_PHRASES: tuple[str, ...] = (
    "复制此条消息",
    "长按复制",
    "快帮我付个款",
    "帮我付款",
    "订单在",
    "分钟内有效",
    "打开抖音",
    "打开淘宝",
    "打开京东",
    "打开拼多多",
    "喜欢的宝贝",
    "查看详情",
)

HIGHLIGHT_SIGNAL_WORDS: tuple[str, ...] = (
    "建议",
    "经验",
    "分享",
    "讨论",
    "问题",
    "风险",
    "策略",
    "学习",
    "可以",
    "觉得",
    "复盘",
    "总结",
)

TOPIC_MARKER_HINTS: tuple[str, ...] = (
    "话题",
    "主题",
    "讨论",
    "复盘",
    "分享",
    "求助",
    "建议",
    "活动",
    "预告",
    "提醒",
    "计划",
    "安排",
)

CHARACTER_ROLE_RULES: tuple[tuple[str, str, str, tuple[str, ...]], ...] = (
    (
        "activity_organizer",
        "活动推进者",
        "把松散聊天推成下一步行动",
        ("活动", "报名", "接龙", "安排", "预告", "提醒", "收集", "表单"),
    ),
    (
        "resource_scout",
        "资料投喂员",
        "把有用线索递到群友手边",
        ("分享", "资料", "链接", "推荐", "文章", "工具", "教程", "收藏"),
    ),
    (
        "question_raiser",
        "问题发射台",
        "把模糊卡点抛到台面上",
        ("求助", "请问", "怎么", "有没有", "为什么", "吗", "？", "?"),
    ),
    (
        "answerer",
        "现场解法师",
        "把经验拆成群里能接住的话",
        ("建议", "可以", "试试", "检查", "经验", "我觉得", "先", "注意"),
    ),
    (
        "atmosphere",
        "气氛承包人",
        "负责让一天的聊天不只是信息流",
        ("欢迎", "哈哈", "加油", "稳住", "笑死", "太好", "厉害"),
    ),
)

MEMORY_FACT_ROLE_LABELS: dict[str, str] = {
    "activity_organizer": "活动推进者",
    "activity_participation": "活动在场者",
    "content_story_lead": "故事线雷达",
    "operation_signal": "规则观察员",
    "resource_scout": "资料投喂员",
    "service_need": "需求提醒人",
    "unresolved_question": "问题发射台",
}

MEMORY_FACT_TYPES: tuple[str, ...] = tuple(MEMORY_FACT_ROLE_LABELS)

CASE_CARD_COLORS = [
    ("#fef3c7", "#92400e"),  # amber
    ("#fee2e2", "#991b1b"),  # red
    ("#dbeafe", "#1e40af"),  # blue
    ("#dcfce7", "#166534"),  # green
    ("#f3e8ff", "#6b21a8"),  # purple
    ("#ffedd5", "#9a3412"),  # orange
]


# ---------------------------------------------------------------------------
# Data classes
# ---------------------------------------------------------------------------

@dataclass
class ReportMessage:
    id: str
    sender_id: str
    sender_name: str
    text: str
    sent_at: datetime | None
    message_kind: str
    person_id: str | None = None


@dataclass
class CaseCard:
    case_no: str
    title: str
    time_label: str
    summary: str
    bullets: list[str]
    message_count: int
    participant_count: int
    color_bg: str
    color_text: str
    top_speaker: str


@dataclass
class Suspect:
    rank: int
    name: str
    message_count: int
    word_count: int
    avatar_emoji: str


@dataclass
class CharacterCard:
    rank: int
    name: str
    role_label: str
    one_liner: str
    evidence: str
    message_count: int
    topic_count: int
    node_key: str = ""
    memory_label: str = ""
    member_fact_memory_used: bool = False
    story_function: str = ""
    callback_hint: str = ""
    arc_label: str = ""
    relationship_hint: str = ""
    relationship_target_key: str = ""
    relationship_topic: str = ""
    meme_seed: str = ""
    memory_weight_label: str = ""
    evidence_anchor: str = ""
    expressive_label: str = ""
    profile_evidence_count: int = 0
    profile_upgrade_status: str = ""
    profile_upgrade_reason: str = ""
    creative_profile_label: str = ""
    creative_profile_status: str = ""
    color_bg: str = ""
    color_text: str = ""


@dataclass
class CharacterMemory:
    person_id: str
    recent_fact_count: int
    lifetime_fact_count: int
    dominant_role_label: str
    recurrence_label: str = ""
    depth_label: str = ""
    memory_weight_label: str = ""
    callback_seed: str = ""


@dataclass
class CreativeProfileMemory:
    person_id: str
    role_label: str
    story_function: str = ""
    daily_arc: str = ""
    memory_weight_label: str = ""
    meme_seed: str = ""
    callback_hint: str = ""
    expressive_label: str = ""
    evidence_anchor: str = ""
    recurrence_evidence_count: int = 0


@dataclass
class HotTopic:
    rank: int
    keyword: str
    message_count: int
    participant_count: int


@dataclass
class ReportData:
    group_name: str
    report_title: str
    report_date: str
    time_range: str
    member_count: int
    message_count: int
    participant_count: int
    case_count: int
    suspect_count: int
    hourly_counts: list[int]
    cases: list[CaseCard]
    suspects: list[Suspect]
    highlight: str | None
    hot_topics: list[HotTopic] = field(default_factory=list)
    character_count: int = 0
    characters: list[CharacterCard] = field(default_factory=list)
    character_universe: dict[str, Any] = field(default_factory=dict)
    window_start: str = ""
    window_end: str = ""
    timezone: str = DEFAULT_TIMEZONE
    messages: list = field(default_factory=list)


# ---------------------------------------------------------------------------
# Shared text utilities
# ---------------------------------------------------------------------------

def clean_text(text: str) -> str:
    """Strip URLs, @mentions, and collapse whitespace."""
    text = text or ""
    text = re.sub(r"https?://\S+", "", text)
    text = re.sub(r"(?<!\S)@(?:[A-Za-z0-9_.-]{1,64}|[\u4e00-\u9fff]{1,6})(?=\s|$)", "", text)
    text = re.sub(r"\s+", " ", text)
    return text.strip()


def memory_recurrence_label(recent_count: int) -> str:
    if recent_count >= 10:
        return "近90天高频复现"
    if recent_count >= 4:
        return "近90天稳定复现"
    if recent_count >= 1:
        return "近90天偶发复现"
    return "今日新鲜出场"


def memory_depth_label(lifetime_count: int) -> str:
    if lifetime_count >= 24:
        return "长期角色锚点"
    if lifetime_count >= 8:
        return "长期线索可用"
    if lifetime_count >= 1:
        return "历史线索较轻"
    return "暂无长期画像"


def memory_weight_label(recent_count: int, lifetime_count: int) -> str:
    if lifetime_count <= 0:
        return "只按今日表现呈现"
    return f"{memory_recurrence_label(recent_count)} · {memory_depth_label(lifetime_count)}"


def memory_callback_seed(role_label: str, recent_count: int) -> str:
    if recent_count >= 4:
        return f"可作为「{role_label}」连续出场回调"
    if recent_count >= 1:
        return f"保留为「{role_label}」轻量回看点"
    return f"先记今日「{role_label}」一笔"
