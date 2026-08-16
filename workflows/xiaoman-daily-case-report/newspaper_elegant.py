"""Elegant broadsheet-style newspaper renderer for the Xiaoman daily report.

Inspired by the wx-cli "秦托邦时报" roast-newspaper layout, but deliberately
uses only deterministic text metadata. It does not embed chat images, read
message payloads, or make network requests, in line with the project privacy
boundary.
"""
from __future__ import annotations

import html
from typing import Any


def _section_card(kicker: str, title: str, body: str, extra_class: str = "") -> str:
    return f"""
    <section class="ns-card {extra_class}">
      <div class="ns-kicker">{html.escape(kicker)}</div>
      <h3 class="ns-section-title">{html.escape(title)}</h3>
      <div class="ns-card-body">{body}</div>
    </section>"""


def _li_item(text: str) -> str:
    return f"<li>{html.escape(text)}</li>"


def _build_topics_section(topic_cards: list[dict[str, Any]]) -> str:
    if not topic_cards:
        return ""
    rows = ""
    for topic in topic_cards:
        title = topic.get("title", "")
        summary = topic.get("summary", "")
        participants = topic.get("participants", 0)
        rows += f"""
        <div class="ns-topic-row">
          <strong>{html.escape(title)}</strong>
          <span>{html.escape(summary)}</span>
          <small>{participants} 人参与</small>
        </div>"""
    return _section_card("COMMUNITY DESK", "主要话题", f'<div class="ns-topic-list">{rows}</div>')


def _build_characters_section(characters: list[dict[str, Any]]) -> str:
    if not characters:
        return ""
    rows = ""
    for character in characters[:6]:
        name = character.get("name", "")
        role = character.get("role", "")
        evidence = character.get("evidence", "")
        rank = character.get("rank", 0)
        rows += f"""
        <div class="ns-cast-row">
          <div class="ns-cast-rank">{rank}</div>
          <div class="ns-cast-copy">
            <strong>{html.escape(name)}</strong>
            <span>{html.escape(role)}</span>
            <small>{html.escape(evidence)}</small>
          </div>
        </div>"""
    return _section_card("CAST NOTES", "人物出场表", f'<div class="ns-cast-list">{rows}</div>')


def _build_highlight_section(highlight: str) -> str:
    if not highlight:
        return ""
    return _section_card(
        "QUOTE ANCHOR",
        "今日台词",
        f'<blockquote class="ns-quote">{html.escape(highlight)}</blockquote>',
    )


def _build_list_section(kicker: str, title: str, items: list[str]) -> str:
    if not items:
        return ""
    body = '<ul class="ns-list">' + "".join(_li_item(item) for item in items[:6]) + "</ul>"
    return _section_card(kicker, title, body)


def _build_cases_section(cases: list[dict[str, Any]]) -> str:
    if not cases:
        return ""
    rows = ""
    for case in cases[:4]:
        case_no = case.get("case_no", "").replace("CASE ", "")
        title = case.get("title", "")
        summary = case.get("summary", "")
        rows += f"""
        <div class="ns-case-row">
          <span class="ns-case-no">{html.escape(case_no)}</span>
          <strong>{html.escape(title)}</strong>
          <small>{html.escape(summary)}</small>
        </div>"""
    return _section_card("STORYLINE FILES", "故事线候选", f'<div class="ns-case-list">{rows}</div>')


def render(input_data: dict[str, Any]) -> str:
    """Render an elegant single-page newspaper from pre-computed report data.

    ``input_data`` must contain the keys produced by
    ``_build_newspaper_elegant_input`` in ``daily_case_report.py``.
    """
    width = input_data["width"]
    group_name = input_data["group_name"]
    report_title = input_data["report_title"]
    report_date = input_data["report_date"]
    time_range = input_data["time_range"]
    message_count = input_data["message_count"]
    participant_count = input_data["participant_count"]
    case_count = input_data["case_count"]
    character_count = input_data["character_count"]
    main_storyline = input_data["main_storyline"]
    opening_line = input_data["opening_line"]
    highlight = input_data.get("highlight")
    topic_cards = input_data.get("topic_cards") or []
    characters = input_data.get("characters") or []
    callbacks = input_data.get("callbacks") or []
    relationships = input_data.get("relationships") or []
    local_life_notes = input_data.get("local_life_notes") or []
    open_questions = input_data.get("open_questions") or []
    cases = input_data.get("cases") or []
    hourly_svg = input_data["hourly_svg"]

    masthead_sub = group_name if group_name else "QINTOPIA REVIEW"
    masthead_title = report_title if report_title else "秦托邦时报"
    edition = f"{report_date} 版"
    page_meta = f"{time_range} · {participant_count} 人出场"

    topics_html = _build_topics_section(topic_cards)
    characters_html = _build_characters_section(characters)
    highlight_html = _build_highlight_section(highlight or "")
    callbacks_html = _build_list_section("MEME MAP", "梗和回调候选", callbacks)
    relationships_html = _build_list_section("ENSEMBLE LINKS", "同场关系", relationships)
    local_life_html = _build_list_section(
        "LOCAL THREADS",
        "地点 / 本地生活线索",
        [f"{item.get('label', '')}（{item.get('source', '')}）" for item in local_life_notes],
    )
    questions_html = _build_list_section("OPEN LOOPS", "待解决问题", open_questions)
    cases_html = _build_cases_section(cases)

    stats_items = [
        ("消息", message_count, "当日素材"),
        ("出场", participant_count, "活跃成员"),
        ("主线", case_count, "可归档"),
        ("人物", character_count, "群像卡"),
    ]
    stats_html = "".join(
        f"""
        <div class="ns-stat">
          <span class="ns-stat-value">{value}</span>
          <span class="ns-stat-label">{html.escape(label)}</span>
          <span class="ns-stat-caption">{html.escape(caption)}</span>
        </div>"""
        for label, value, caption in stats_items
    )

    return f"""<!DOCTYPE html>
<html lang="zh-CN">
<head>
<meta charset="utf-8">
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  html, body {{
    margin: 0;
    background: #d5d4ce;
    color: #171717;
    font-family: "PingFang SC", "Noto Sans CJK SC", "Heiti SC", sans-serif;
  }}
  .ns-page {{
    width: {width}px;
    margin: 0 auto;
    padding: 38px 32px 32px;
    background:
      linear-gradient(90deg, rgba(23, 23, 23, 0.018) 1px, transparent 1px) 0 0 / 60px 60px,
      linear-gradient(#fbfaf6, #fbfaf6);
    position: relative;
  }}
  .ns-page::after {{
    content: "";
    position: absolute;
    inset: 18px;
    border: 1px solid #d6d2c8;
    pointer-events: none;
  }}
  .ns-header {{
    position: relative;
    z-index: 1;
    border-bottom: 2px solid #242424;
    margin-bottom: 22px;
  }}
  .ns-nameplate {{
    display: grid;
    grid-template-columns: 1fr auto 1fr;
    align-items: end;
    gap: 18px;
    padding-bottom: 10px;
    text-transform: uppercase;
    color: #63625d;
    font-size: 13px;
    letter-spacing: 0.08em;
  }}
  .ns-nameplate strong {{
    color: #171717;
    font-family: "Songti SC", "STSong", "Noto Serif CJK SC", serif;
    font-size: 44px;
    line-height: 0.95;
    font-weight: 900;
    letter-spacing: 0;
    white-space: nowrap;
  }}
  .ns-nameplate span:last-child {{ text-align: right; }}
  .ns-meta {{
    display: grid;
    grid-template-columns: 1fr 1fr 1fr;
    gap: 14px;
    padding: 8px 0 9px;
    border-top: 1px solid #d6d2c8;
    color: #63625d;
    font-size: 13px;
  }}
  .ns-meta span:first-child {{ color: #171717; font-weight: 800; }}
  .ns-meta span:nth-child(2) {{ text-align: center; }}
  .ns-meta span:last-child {{ text-align: right; color: #171717; font-weight: 800; }}
  .ns-hero {{
    position: relative;
    z-index: 1;
    padding: 18px 20px 20px;
    margin-bottom: 22px;
    background: #f1f0ea;
    border-left: 6px solid #8b1f2f;
  }}
  .ns-hero-kicker {{
    color: #8b1f2f;
    font-size: 12px;
    font-weight: 900;
    letter-spacing: 0.12em;
    text-transform: uppercase;
  }}
  .ns-hero h2 {{
    margin-top: 6px;
    font-family: "Songti SC", "STSong", "Noto Serif CJK SC", serif;
    font-size: 36px;
    line-height: 1.05;
    font-weight: 900;
  }}
  .ns-deck {{
    margin-top: 12px;
    color: #373530;
    font-size: 16px;
    line-height: 1.45;
    font-weight: 700;
  }}
  .ns-hero-meta {{
    margin-top: 12px;
    padding-top: 8px;
    border-top: 1px solid #d6d2c8;
    font-size: 11px;
    color: #555;
  }}
  .ns-grid {{
    position: relative;
    z-index: 1;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
  }}
  .ns-card {{
    padding: 14px 14px 16px;
    border: 1px solid #d6d2c8;
    background: #ffffff;
  }}
  .ns-kicker {{
    color: #8b1f2f;
    font-size: 11px;
    font-weight: 900;
    letter-spacing: 0.1em;
    text-transform: uppercase;
  }}
  .ns-section-title {{
    margin-top: 6px;
    padding-bottom: 6px;
    border-bottom: 1px solid #242424;
    font-family: "Songti SC", "STSong", "Noto Serif CJK SC", serif;
    font-size: 20px;
    line-height: 1.15;
    font-weight: 900;
  }}
  .ns-card-body {{
    margin-top: 10px;
    font-size: 13px;
    line-height: 1.55;
  }}
  .ns-list {{
    list-style: none;
    padding: 0;
  }}
  .ns-list li {{
    margin: 0 0 7px;
    padding-left: 14px;
    position: relative;
  }}
  .ns-list li::before {{
    content: "";
    position: absolute;
    left: 0;
    top: 0.65em;
    width: 5px;
    height: 5px;
    background: #8b1f2f;
  }}
  .ns-quote {{
    margin: 0;
    padding: 12px 14px;
    background: #fff;
    border-top: 3px solid #8b1f2f;
    font-size: 15px;
    line-height: 1.5;
    font-weight: 700;
  }}
  .ns-topic-row {{
    display: grid;
    gap: 2px;
    padding: 7px 0;
    border-bottom: 1px solid #f1f0ea;
  }}
  .ns-topic-row:last-child {{ border-bottom: 0; }}
  .ns-topic-row strong {{ font-size: 14px; }}
  .ns-topic-row span {{ color: #555; font-size: 12px; }}
  .ns-topic-row small {{ color: #8b1f2f; font-size: 10px; font-weight: 800; }}
  .ns-cast-row {{
    display: grid;
    grid-template-columns: 28px 1fr;
    gap: 10px;
    padding: 7px 0;
    border-bottom: 1px solid #f1f0ea;
  }}
  .ns-cast-row:last-child {{ border-bottom: 0; }}
  .ns-cast-rank {{
    width: 28px;
    height: 28px;
    display: grid;
    place-items: center;
    border: 1px solid #171717;
    background: #f1f0ea;
    font-size: 12px;
    font-weight: 900;
  }}
  .ns-cast-copy strong {{ font-size: 14px; }}
  .ns-cast-copy span {{ display: block; color: #8b1f2f; font-size: 11px; font-weight: 700; }}
  .ns-cast-copy small {{ display: block; color: #555; font-size: 11px; margin-top: 2px; }}
  .ns-case-row {{
    display: grid;
    gap: 2px;
    padding: 7px 0;
    border-bottom: 1px solid #f1f0ea;
  }}
  .ns-case-row:last-child {{ border-bottom: 0; }}
  .ns-case-no {{
    width: 22px;
    height: 22px;
    display: grid;
    place-items: center;
    border: 1px solid #171717;
    border-radius: 50%;
    background: #f1f0ea;
    font-size: 10px;
    font-weight: 900;
  }}
  .ns-case-row strong {{ font-size: 13px; }}
  .ns-case-row small {{ color: #555; font-size: 11px; }}
  .ns-bottom {{
    position: relative;
    z-index: 1;
    margin-top: 22px;
    display: grid;
    grid-template-columns: 1fr 1fr;
    gap: 18px;
  }}
  .ns-timeline {{
    padding: 14px;
    border: 1px solid #d6d2c8;
    background: #fff;
  }}
  .ns-timeline h4 {{
    font-family: "Songti SC", "STSong", "Noto Serif CJK SC", serif;
    font-size: 16px;
    margin-bottom: 8px;
  }}
  .ns-timeline svg {{ display: block; width: 100%; height: 90px; }}
  .ns-stats {{
    display: grid;
    grid-template-columns: repeat(4, 1fr);
    gap: 1px;
    background: #d6d2c8;
    border: 1px solid #d6d2c8;
  }}
  .ns-stat {{
    background: #fff;
    padding: 12px 8px;
    text-align: center;
  }}
  .ns-stat-value {{ font-size: 26px; font-weight: 900; color: #171717; }}
  .ns-stat-label {{ display: block; font-size: 11px; color: #8b1f2f; font-weight: 800; margin-top: 4px; }}
  .ns-stat-caption {{ display: block; font-size: 9px; color: #63625d; margin-top: 2px; }}
  .ns-footer {{
    position: relative;
    z-index: 1;
    margin-top: 22px;
    padding-top: 10px;
    border-top: 2px solid #242424;
    color: #63625d;
    font-size: 11px;
    display: flex;
    justify-content: space-between;
  }}
  @media screen and (max-width: 780px) {{
    .ns-grid {{ grid-template-columns: 1fr; }}
    .ns-bottom {{ grid-template-columns: 1fr; }}
    .ns-nameplate strong {{ font-size: 32px; }}
    .ns-hero h2 {{ font-size: 28px; }}
  }}
</style>
</head>
<body>
<div class="ns-page">
  <header class="ns-header">
    <div class="ns-nameplate">
      <span>{html.escape(masthead_sub)}</span>
      <strong>{html.escape(masthead_title)}</strong>
      <span>{html.escape(report_date)}</span>
    </div>
    <div class="ns-meta">
      <span>COMMUNITY DAILY</span>
      <span>{html.escape(page_meta)}</span>
      <span>{html.escape(edition)}</span>
    </div>
  </header>
  <section class="ns-hero">
    <div class="ns-hero-kicker">COVER STORY</div>
    <h2>{html.escape(main_storyline)}</h2>
    <p class="ns-deck">{html.escape(opening_line)}</p>
    <div class="ns-hero-meta">{message_count} 条素材 · {case_count} 条主线 · {character_count} 位剧中人</div>
  </section>
  <div class="ns-grid">
    {topics_html}
    {characters_html}
    {highlight_html}
    {callbacks_html}
    {relationships_html}
    {local_life_html}
    {questions_html}
    {cases_html}
  </div>
  <div class="ns-bottom">
    <section class="ns-timeline">
      <h4>24H 活跃节奏</h4>
      <svg viewBox="0 0 {width - 120} 90" aria-label="24小时活跃节奏">{hourly_svg}</svg>
    </section>
    <div class="ns-stats">{stats_html}</div>
  </div>
  <footer class="ns-footer">
    <span>本报告由小满根据最新群聊窗口自动整理 · 长期画像只以公开安全的角色复现计数参与</span>
    <span>{html.escape(report_title)}</span>
  </footer>
</div>
</body>
</html>"""
