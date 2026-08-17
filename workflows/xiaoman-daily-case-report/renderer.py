"""HTML / Pillow / Playwright rendering for the Xiaoman daily case-report pipeline.

Deterministic renderers (newspaper, v3, elegant) plus the image pipeline.
Narrative-based roast-long-image rendering delegates to roast_long_image.py.
"""
from __future__ import annotations

import html
import os
from pathlib import Path
from typing import Any

# Workflow directory for sibling imports (roast_long_image, newspaper_elegant).
import sys
_WORKFLOW_DIR = Path(__file__).resolve().parent
if str(_WORKFLOW_DIR) not in sys.path:
    sys.path.insert(0, str(_WORKFLOW_DIR))

from models import (
    CASE_CARD_COLORS,
    DEFAULT_CASE_LIMIT,
    DEFAULT_JPEG_QUALITY,
    DEFAULT_SUSPECT_LIMIT,
    DEFAULT_TEMPLATE,
    NEWSPAPER_ELEGANT_TEMPLATE,
    ROAST_LONG_IMAGE_TEMPLATE,
    ReportData,
)
from analyzer import case_storyline_label
from report_builder import (
    _daily_opening_line,
    _main_storyline_label,
    _meme_callback_candidates,
    _ordinary_digest_candidate_topics,
    _ordinary_digest_local_life_notes,
    _ordinary_digest_open_questions,
    _ordinary_digest_topic_cards,
    _relationship_candidates,
)
import newspaper_elegant
import roast_long_image



def _bar_svg(counts: list[int], max_count: int, width: int, height: int) -> str:
    if not counts or max_count == 0:
        return ""
    bar_width = width // len(counts)
    gap = 2
    effective_width = max(1, bar_width - gap)
    bars = []
    for idx, count in enumerate(counts):
        h = int((count / max_count) * height) if max_count else 0
        x = idx * bar_width + gap // 2
        y = height - h
        bars.append(
            f'<rect x="{x}" y="{y}" width="{effective_width}" height="{h}" rx="2" fill="#1a2744"/>'
        )
    return "\n".join(bars)


def _render_newspaper_html(report: ReportData, width: int) -> str:
    main_storyline = _main_storyline_label(report)
    opening = _daily_opening_line(report)
    highlight = html.escape(report.highlight or "")

    lead_paragraphs: list[str] = []
    if opening:
        lead_paragraphs.append(html.escape(opening))
    for case in report.cases[:3]:
        para = f"【{html.escape(case.title)}】{html.escape(case.summary)}。"
        bullets = "；".join(html.escape(b) for b in case.bullets[:3])
        if bullets:
            para += bullets + "。"
        para += f"（{case.participant_count} 人参与，{case.message_count} 条消息）"
        lead_paragraphs.append(para)
    if report.characters:
        names = "、".join(html.escape(c.name) for c in report.characters[:4])
        lead_paragraphs.append(f"今日活跃的剧中人包括 {names}。")

    lead_article_html = "".join(f"<p>{p}</p>" for p in lead_paragraphs)

    character_html = "".join(
        f'''<div class="profile">
          <div class="profile-avatar" style="background:{html.escape(bg)};color:{html.escape(fg)}">{html.escape(c.name[0])}</div>
          <div class="profile-copy">
            <h4>{html.escape(c.name)}</h4>
            <p>{html.escape(c.role_label)} · {html.escape(c.story_function)}</p>
          </div>
        </div>'''
        for i, c in enumerate(report.characters[:6])
        for bg, fg in [CASE_CARD_COLORS[i % len(CASE_CARD_COLORS)]]
    )

    case_cards_html = "".join(
        f'''<article class="story-card">
          <div class="story-kicker">{html.escape(case_storyline_label(case))}</div>
          <h4>{html.escape(case.title)}</h4>
          <p>{html.escape(case.summary)}</p>
        </article>'''
        for case in report.cases[:4]
    )

    stats = [
        ("消息", str(report.message_count)),
        ("活跃人数", str(report.participant_count)),
        ("主线", str(report.case_count)),
        ("剧中人", str(report.character_count)),
    ]
    stats_html = "".join(
        f'<div class="stat-box"><span>{html.escape(k)}</span><strong>{html.escape(v)}</strong></div>'
        for k, v in stats
    )

    chart_width = width - 84
    peak_count = max(report.hourly_counts or [0])
    max_hourly = peak_count or 1
    peak_idx = report.hourly_counts.index(peak_count) if peak_count else 0
    chart_svg = _bar_svg(report.hourly_counts, max_hourly, chart_width, 80)

    mvp_html = "".join(
        f'<div class="mvp-row"><span>{s.rank}</span><strong>{html.escape(s.name)}</strong><em>{s.message_count} 条 / {s.word_count} 字</em></div>'
        for s in report.suspects[:5]
    )

    hot_html = "".join(
        f'<div class="hot-row"><span>{t.rank}</span><strong>{html.escape(t.keyword)}</strong><small>{t.message_count} 次</small></div>'
        for t in report.hot_topics[:5]
    )

    hot_section = f'<div class="sidebar-section"><h3>热词</h3>{hot_html}</div>' if hot_html else ""

    highlight_block = (
        f'<div class="highlight-box"><blockquote>“{highlight}”</blockquote></div>'
        if highlight
        else ""
    )

    return f"""<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ background: #e8e4dc; color: #1a1a1a; font-family: "Noto Serif CJK SC", "Source Han Serif SC", "STSong", "SimSun", "Songti SC", "AR PL UMing CN", serif; }}
  .paper {{ width: {width}px; margin: 0 auto; background: #f7f5f0; padding: 36px 42px; box-sizing: border-box; }}
  .masthead {{ text-align: center; border-bottom: 3px solid #1a1a1a; padding-bottom: 10px; margin-bottom: 24px; }}
  .masthead-kicker {{ font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif; font-size: 10px; letter-spacing: 4px; text-transform: uppercase; color: #555; }}
  .masthead h1 {{ font-size: 56px; font-weight: 900; letter-spacing: 12px; margin: 6px 0 4px; }}
  .masthead-meta {{ display: flex; justify-content: center; gap: 48px; font-family: sans-serif; font-size: 12px; color: #444; margin-top: 6px; }}
  .headline {{ margin: 28px 0 18px; }}
  .headline h2 {{ font-size: 42px; line-height: 1.25; font-weight: 900; margin-bottom: 10px; }}
  .headline .deck {{ font-size: 18px; line-height: 1.55; color: #444; font-style: italic; }}
  .content {{ display: grid; grid-template-columns: 2fr 1fr; gap: 32px; margin-bottom: 32px; }}
  .lead {{ column-count: 2; column-gap: 28px; font-size: 14px; line-height: 1.85; }}
  .lead p {{ margin-bottom: 14px; text-align: justify; }}
  .lead p:first-child::first-letter {{ font-size: 42px; float: left; line-height: 1; margin-right: 6px; margin-top: 4px; font-weight: 900; }}
  .sidebar {{ border-left: 2px solid #1a1a1a; padding-left: 24px; }}
  .sidebar-section {{ margin-bottom: 22px; }}
  .sidebar-section h3 {{ font-family: sans-serif; font-size: 12px; letter-spacing: 1px; text-transform: uppercase; border-bottom: 2px solid #1a1a1a; padding-bottom: 4px; margin-bottom: 10px; }}
  .stat-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; }}
  .stat-box {{ border: 2px solid #1a1a1a; padding: 8px; text-align: center; }}
  .stat-box span {{ font-family: sans-serif; font-size: 10px; color: #555; display: block; margin-bottom: 2px; }}
  .stat-box strong {{ font-size: 24px; font-weight: 900; }}
  .timeline-chart svg {{ display: block; width: 100%; height: 80px; }}
  .timeline-chart .peak {{ font-family: sans-serif; font-size: 11px; color: #555; margin-top: 6px; }}
  .mvp-row, .hot-row {{ display: flex; align-items: baseline; gap: 8px; font-size: 13px; margin-bottom: 6px; }}
  .mvp-row span, .hot-row span {{ font-family: sans-serif; font-size: 10px; font-weight: 900; min-width: 16px; }}
  .section {{ margin-bottom: 28px; }}
  .section-kicker {{ font-family: sans-serif; font-size: 11px; letter-spacing: 2px; text-transform: uppercase; color: #b91c1c; font-weight: 800; margin-bottom: 6px; }}
  .section h3 {{ font-size: 22px; font-weight: 900; margin-bottom: 14px; }}
  .character-grid {{ display: grid; grid-template-columns: repeat(3, 1fr); gap: 16px; }}
  .profile {{ display: flex; gap: 10px; align-items: flex-start; border: 2px solid #1a1a1a; padding: 10px; background: #fff; }}
  .profile-avatar {{ width: 36px; height: 36px; border-radius: 50%; display: grid; place-items: center; font-family: sans-serif; font-size: 16px; font-weight: 900; flex-shrink: 0; }}
  .profile-copy h4 {{ font-size: 14px; font-weight: 900; margin-bottom: 2px; }}
  .profile-copy p {{ font-size: 11px; color: #444; line-height: 1.4; }}
  .story-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 18px; }}
  .story-card {{ border: 2px solid #1a1a1a; padding: 14px; background: #fff; }}
  .story-card .story-kicker {{ font-family: sans-serif; font-size: 10px; color: #b91c1c; font-weight: 800; letter-spacing: 1px; text-transform: uppercase; margin-bottom: 4px; }}
  .story-card h4 {{ font-size: 16px; font-weight: 900; margin-bottom: 6px; }}
  .story-card p {{ font-size: 12px; line-height: 1.55; color: #333; }}
  .highlight-box {{ border-top: 3px solid #1a1a1a; border-bottom: 3px solid #1a1a1a; padding: 16px 0; margin-bottom: 28px; }}
  .highlight-box blockquote {{ font-size: 22px; font-style: italic; line-height: 1.5; }}
  .footer {{ border-top: 2px solid #1a1a1a; padding-top: 10px; font-family: sans-serif; font-size: 10px; color: #555; display: flex; justify-content: space-between; }}
</style>
</head>
<body>
<main class="paper">
  <header class="masthead">
    <div class="masthead-kicker">QINTOPIA COMMUNITY DAILY</div>
    <h1>小满时报</h1>
    <div class="masthead-meta">
      <span>{html.escape(report.report_date)}</span>
      <span>{html.escape(report.time_range)}</span>
      <span>第 1 版</span>
    </div>
  </header>
  <div class="headline">
    <h2>{html.escape(main_storyline)}</h2>
    <p class="deck">{html.escape(opening)}</p>
  </div>
  <div class="content">
    <article class="lead">
      {lead_article_html}
    </article>
    <aside class="sidebar">
      <div class="sidebar-section">
        <h3>数据</h3>
        <div class="stat-grid">
          {stats_html}
        </div>
      </div>
      <div class="sidebar-section">
        <h3>24H 活跃</h3>
        <div class="timeline-chart">
          <svg viewBox="0 0 {chart_width} 80" preserveAspectRatio="none">{chart_svg}</svg>
          <div class="peak">峰值 {peak_count} 条 / {peak_idx:02d}:00</div>
        </div>
      </div>
      <div class="sidebar-section">
        <h3>发言榜</h3>
        {mvp_html}
      </div>
      {hot_section}
    </aside>
  </div>
  {highlight_block}
  <section class="section">
    <div class="section-kicker">Cast Notes</div>
    <h3>人物出场表</h3>
    <div class="character-grid">
      {character_html}
    </div>
  </section>
  <section class="section">
    <div class="section-kicker">Storylines</div>
    <h3>今日主线</h3>
    <div class="story-grid">
      {case_cards_html}
    </div>
  </section>
  <footer class="footer">
    <span>本报告由小满自动整理，仅反映已审核公开安全的群聊片段。</span>
    <span>{html.escape(report.report_date)}</span>
  </footer>
</main>
</body>
</html>"""


def _render_html(
    report: ReportData,
    width: int,
    template: str = DEFAULT_TEMPLATE,
    narrative_md: str | None = None,
) -> str:
    if template == ROAST_LONG_IMAGE_TEMPLATE:
        if not narrative_md:
            raise RuntimeError(
                "roast-long-image template requires --narrative roast; "
                "the LLM narrative text is the source of the rendered image"
            )
        return roast_long_image.render({
            "narrative_md": narrative_md,
            "report_date": report.report_date,
            "time_range": report.time_range,
            "group_name": report.group_name,
            "message_count": report.message_count,
            "participant_count": report.participant_count,
            "width": width,
        })
    if template == NEWSPAPER_ELEGANT_TEMPLATE:
        return newspaper_elegant.render(_build_newspaper_elegant_input(report, width))
    if template == "newspaper":
        return _render_newspaper_html(report, width)
    return _render_v3_html(report, width)


def _build_newspaper_elegant_input(report: ReportData, width: int) -> dict[str, Any]:
    """Map the deterministic ReportData into the shape newspaper_elegant.render expects.

    Only public-safe, already-computed text metadata is used. No chat images, message
    payloads, or network access — consistent with the project privacy boundary.
    """
    universe = report.character_universe or {}
    hourly = report.hourly_counts or []
    peak_count = max(hourly) if hourly else 0
    max_hourly = peak_count or 1
    chart_w = max(width - 120, 120)
    hourly_svg = _bar_svg(hourly[:24], max_hourly, chart_w, 90)

    topic_cards = _ordinary_digest_topic_cards(report)
    open_questions = _ordinary_digest_open_questions(report)
    local_life_notes = _ordinary_digest_local_life_notes(report)

    characters = [
        {
            "name": c.name,
            "role": c.role_label,
            "evidence": c.evidence,
            "rank": c.rank,
        }
        for c in report.characters[:6]
    ]

    callbacks = [
        cb.get("label", "")
        for cb in (universe.get("callbacks") or [])
        if isinstance(cb, dict) and cb.get("label")
    ]
    relationships = [
        (r.get("label") or f"{r.get('source', '')} 与 {r.get('target', '')}：{r.get('topic', '')}")
        for r in (universe.get("relationships") or [])
        if isinstance(r, dict)
    ]

    cases = [
        {
            "case_no": case.case_no,
            "title": case.title,
            "summary": case.summary,
        }
        for case in report.cases[:4]
    ]

    return {
        "width": width,
        "group_name": report.group_name,
        "report_title": report.report_title,
        "report_date": report.report_date,
        "time_range": report.time_range,
        "message_count": report.message_count,
        "participant_count": report.participant_count,
        "case_count": report.case_count,
        "character_count": report.character_count,
        "main_storyline": _main_storyline_label(report),
        "opening_line": _daily_opening_line(report),
        "highlight": report.highlight,
        "topic_cards": topic_cards,
        "characters": characters,
        "callbacks": callbacks,
        "relationships": relationships,
        "local_life_notes": local_life_notes,
        "open_questions": open_questions,
        "cases": cases,
        "hourly_svg": hourly_svg,
    }


def _render_v3_html(report: ReportData, width: int) -> str:
    chart_width = width - 96
    peak_count = max(report.hourly_counts or [0])
    max_hourly = peak_count or 1
    peak_idx = report.hourly_counts.index(peak_count) if peak_count else 0
    timeline_svg = _bar_svg(report.hourly_counts, max_hourly, chart_width, 68)
    timeline_labels = "".join(
        f'<text x="{int((idx / 24) * chart_width)}" y="94" font-size="9" fill="#4a4a4a" text-anchor="middle">{idx:02d}</text>'
        for idx in range(0, 25, 4)
    )
    peak_x = peak_idx * (chart_width // 24) + (chart_width // 48)
    peak_svg = f'<text x="{peak_x}" y="12" font-size="10" fill="#f25a18" font-weight="700" text-anchor="middle">{peak_count}</text>'
    main_storyline = _main_storyline_label(report)
    opening_line = _daily_opening_line(report)
    callback_candidates = _meme_callback_candidates(report)
    relationship_candidates = _relationship_candidates(report)
    local_life_notes = _ordinary_digest_local_life_notes(report)
    open_questions = _ordinary_digest_open_questions(report)

    story_index_html = "\n".join(
        f"""
      <div class="story-index-item">
        <span>{index:02d}</span>
        <strong>{html.escape(case_storyline_label(case))}</strong>
        <small>{html.escape(case.summary)}</small>
      </div>"""
        for index, case in enumerate(report.cases[:4], start=1)
    )
    story_index_section = f"""
  <section class="story-index">
    <div class="story-index-heading"><span>DAILY WORKSHOP INDEX</span><strong>{report.message_count} 条素材 / {report.participant_count} 位出场 / {report.case_count} 条主线 / {report.character_count} 张人物卡</strong></div>
    <div class="story-index-grid">{story_index_html}</div>
  </section>""" if story_index_html else ""

    stats_html = "\n".join(
        f"""
      <div class="stat">
        <div class="stat-label">{label}</div>
        <div class="stat-value">{value}</div>
        <div class="stat-caption">{caption}</div>
      </div>"""
        for label, value, caption in (
            ("消息", report.message_count, "当日素材"),
            ("出场", report.participant_count, "活跃成员"),
            ("主线", report.case_count, "可归档"),
            ("人物", report.character_count, "群像卡"),
        )
    )

    case_cards = "".join(
        f"""
      <article class="case-card">
        <div class="case-head">
          <span class="case-number">{html.escape(case.case_no.replace("CASE ", ""))}</span>
          <span class="case-time">{html.escape(case.time_label)}</span>
        </div>
        <h3>{html.escape(case_storyline_label(case))}</h3>
        <p class="case-summary">{html.escape(case.summary)}</p>
        <ul class="case-notes">{"".join(f"<li>{html.escape(bullet)}</li>" for bullet in case.bullets[:3])}</ul>
      </article>"""
        for case in report.cases
    )
    cases_html = f"""
  <section class="section cases-section">
    <div class="section-kicker">STORYLINE FILES</div>
    <h2>故事线候选</h2>
    <div class="cases">{case_cards}</div>
  </section>""" if case_cards else ""

    suspects_html = "".join(
        f"""
      <div class="mvp-card">
        <div class="mvp-rank">{suspect.rank}</div>
        <div class="mvp-copy">
          <div class="mvp-name">{html.escape(suspect.name)}</div>
          <div class="mvp-meta">{suspect.message_count} 条 / {suspect.word_count} 字</div>
        </div>
        <div class="mvp-score">{suspect.message_count}</div>
      </div>"""
        for suspect in report.suspects
    )
    mvp_html = f"""
  <section class="section mvp-section">
    <div class="section-kicker">VOICE INDEX</div>
    <h2>发言出场榜</h2>
    <div class="mvp-grid">{suspects_html}</div>
  </section>""" if suspects_html else ""

    highlight_html = ""
    if report.highlight:
        highlight_html = f"""
  <section class="highlight">
    <div class="highlight-kicker">QUOTE ANCHOR</div>
    <div class="highlight-title">今日台词</div>
    <p>“{html.escape(report.highlight)}”</p>
  </section>"""

    callbacks_html = ""
    if callback_candidates:
        callbacks_html = f"""
  <section class="hotlist">
    <div class="hotlist-heading"><span>MEME MAP</span><h2>梗和回调候选</h2></div>
    <div class="hotlist-grid">{"".join(
        f'''<div class="hot-topic"><span class="hot-rank">{index}</span><strong>{html.escape(candidate.split("：", 1)[0])}</strong><small>{html.escape(candidate.split("：", 1)[1] if "：" in candidate else candidate)}</small></div>'''
        for index, candidate in enumerate(callback_candidates, start=1)
    )}</div>
  </section>"""

    relationships_html = ""
    if relationship_candidates:
        relationships_html = f"""
  <section class="relationships">
    <div class="relationships-heading"><span>ENSEMBLE LINKS</span><h2>同场关系</h2></div>
    <div class="relationship-list">{"".join(
        f'''<div class="relationship-row"><span>{index}</span><p>{html.escape(candidate)}</p></div>'''
        for index, candidate in enumerate(relationship_candidates, start=1)
    )}</div>
  </section>"""

    local_life_html = ""
    if local_life_notes:
        local_life_html = f"""
  <section class="reference-notes">
    <div class="reference-heading"><span>LOCAL THREADS</span><h2>地点 / 本地生活线索</h2></div>
    <div class="reference-list">{"".join(
        f'''<div class="reference-row"><span>{index}</span><p>{html.escape(item.get("label", ""))}</p></div>'''
        for index, item in enumerate(local_life_notes, start=1)
    )}</div>
  </section>"""

    open_questions_html = ""
    if open_questions:
        open_questions_html = f"""
  <section class="reference-notes questions">
    <div class="reference-heading"><span>OPEN LOOPS</span><h2>待解决问题</h2></div>
    <div class="reference-list">{"".join(
        f'''<div class="reference-row"><span>{index}</span><p>{html.escape(question)}</p></div>'''
        for index, question in enumerate(open_questions, start=1)
    )}</div>
  </section>"""

    characters_html = ""
    if report.characters:
        characters_html = f"""
  <section class="characters">
    <div class="characters-heading"><span>CAST NOTES</span><h2>人物出场表</h2></div>
    <div class="character-grid">{"".join(
        f'''<article class="character-card"><div class="character-rank">{character.rank}</div><div class="character-copy"><h3>{html.escape(character.name)}</h3><strong>{html.escape(character.role_label)} · {html.escape(character.story_function)}</strong><p>{html.escape(character.arc_label or character.one_liner)}</p><blockquote>{html.escape(character.evidence)}</blockquote><small>{html.escape(character.callback_hint)}{(" · " + html.escape(character.relationship_hint)) if character.relationship_hint else ""}{(" · 已审核标签：" + html.escape(character.expressive_label)) if character.expressive_label else ""}{(" · " + html.escape(character.memory_weight_label)) if character.memory_weight_label else ""}</small></div></article>'''
        for character in report.characters
    )}</div>
  </section>"""

    return f"""<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ background: #ddd8ce; color: #111111; font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", sans-serif; }}
  .daily-paper {{ width: {width}px; margin: 18px auto; background: #fff8df; border: 9px solid #111111; }}
  .topline {{ min-height: 42px; display: flex; align-items: center; justify-content: space-between; padding: 0 24px; background: #111111; color: #ffd92e; font-size: 11px; font-weight: 800; }}
  .hero {{ position: relative; min-height: 196px; padding: 22px 154px 20px 24px; background: #ffd92e; border-bottom: 4px solid #111111; }}
  .hero-group {{ font-size: 25px; font-weight: 800; line-height: 1.25; }}
  .hero-title {{ margin-top: 7px; font-size: 42px; font-weight: 900; line-height: 1; }}
  .hero-mainline {{ margin-top: 14px; font-size: 18px; font-weight: 900; line-height: 1.45; }}
  .hero-opening {{ margin-top: 8px; color: #2b2b2b; font-size: 13px; font-weight: 700; line-height: 1.55; }}
  .hero-time {{ margin-top: 12px; padding-top: 6px; border-top: 4px solid #111111; font-size: 11px; }}
  .hero-badge {{ position: absolute; right: 24px; top: 24px; display: grid; width: 106px; height: 106px; place-items: center; border: 4px solid #111111; border-radius: 12px; background: #88d7ff; font-size: 21px; font-weight: 900; text-align: center; line-height: 1.1; }}
  .story-index {{ padding: 16px 24px 18px; background: #111111; color: #fff8df; }}
  .story-index-heading {{ display: flex; align-items: baseline; justify-content: space-between; gap: 18px; margin-bottom: 11px; }}
  .story-index-heading span {{ color: #ffd92e; font-size: 11px; font-weight: 900; }}
  .story-index-heading strong {{ color: #fff0a6; font-size: 12px; font-weight: 800; text-align: right; }}
  .story-index-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; }}
  .story-index-item {{ display: grid; grid-template-columns: 32px 1fr; gap: 8px; min-height: 54px; padding: 8px; border: 2px solid #fff8df; background: #1c1c1c; }}
  .story-index-item span {{ display: grid; width: 28px; height: 28px; place-items: center; border: 2px solid #ffd92e; border-radius: 50%; color: #ffd92e; font-size: 11px; font-weight: 900; }}
  .story-index-item strong {{ min-width: 0; font-size: 13px; font-weight: 900; line-height: 1.25; }}
  .story-index-item small {{ grid-column: 2; color: #c9c9c9; font-size: 10px; line-height: 1.35; }}
  .stats {{ display: grid; grid-template-columns: repeat(4, 1fr); margin: 22px 24px 0; border: 3px solid #111111; background: #ffffff; color: #111111; }}
  .stat {{ min-height: 70px; padding: 13px 16px; border-right: 2px solid #111111; }}
  .stat:last-child {{ border-right: 0; }}
  .stat-label, .section-kicker, .highlight-kicker, .hotlist-heading span {{ color: #ffd92e; font-size: 11px; font-weight: 800; }}
  .stat .stat-label {{ color: #f25a18; }}
  .stat-value {{ margin-top: 4px; font-size: 26px; font-weight: 900; line-height: 1; }}
  .stat-caption {{ margin-top: 4px; color: #555555; font-size: 10px; }}
  .timeline {{ margin: 22px 24px 0; padding: 18px 18px 12px; border: 4px solid #111111; background: #fff0a6; }}
  .timeline-head {{ display: flex; align-items: baseline; justify-content: space-between; }}
  .timeline h2, .section h2 {{ font-size: 26px; font-weight: 900; line-height: 1.1; }}
  .peak {{ font-size: 12px; font-weight: 700; }}
  .timeline svg {{ display: block; width: 100%; height: 106px; margin-top: 8px; }}
  .highlight {{ display: grid; grid-template-columns: 154px 1fr; gap: 18px; margin: 34px 24px 0; padding: 18px 20px; border: 4px solid #111111; background: #f25a18; color: #fff8df; }}
  .highlight-kicker {{ grid-column: 1; color: #ffd92e; }}
  .highlight-title {{ grid-column: 1; align-self: center; font-size: 25px; font-weight: 900; }}
  .highlight p {{ grid-column: 2; grid-row: 1 / span 2; align-self: center; font-size: 15px; font-weight: 700; line-height: 1.65; }}
  .hotlist {{ margin: 20px 24px 0; padding: 14px 16px 16px; border: 4px solid #111111; background: #fff8df; }}
  .hotlist-heading {{ display: flex; align-items: baseline; gap: 10px; margin-bottom: 12px; }}
  .hotlist-heading span {{ color: #f25a18; }}
  .hotlist-heading h2 {{ font-size: 20px; font-weight: 900; }}
  .hotlist-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 8px; }}
  .hot-topic {{ display: grid; grid-template-columns: 25px 94px 1fr; align-items: center; gap: 7px; min-height: 48px; padding: 6px 8px; border: 2px solid #111111; background: #fff0a6; }}
  .hot-rank {{ display: grid; width: 23px; height: 23px; place-items: center; border: 2px solid #111111; border-radius: 50%; background: #ffd92e; font-size: 11px; font-weight: 900; }}
  .hot-topic strong {{ min-width: 0; font-size: 14px; }}
  .hot-topic small {{ color: #555555; font-size: 10px; line-height: 1.35; }}
  .relationships {{ margin: 20px 24px 0; padding: 14px 16px 16px; border: 4px solid #111111; background: #88d7ff; }}
  .relationships-heading {{ display: flex; align-items: baseline; gap: 10px; margin-bottom: 12px; }}
  .relationships-heading span {{ color: #111111; font-size: 11px; font-weight: 800; }}
  .relationships-heading h2 {{ font-size: 20px; font-weight: 900; }}
  .relationship-list {{ display: grid; gap: 8px; }}
  .relationship-row {{ display: grid; grid-template-columns: 28px 1fr; align-items: center; min-height: 42px; border: 2px solid #111111; background: #ffffff; }}
  .relationship-row span {{ display: grid; height: 100%; place-items: center; border-right: 2px solid #111111; background: #ffd92e; font-size: 11px; font-weight: 900; }}
  .relationship-row p {{ padding: 8px 10px; font-size: 12px; font-weight: 700; line-height: 1.45; }}
  .reference-notes {{ margin: 20px 24px 0; padding: 14px 16px 16px; border: 4px solid #111111; background: #ffffff; }}
  .reference-notes.questions {{ background: #fff0a6; }}
  .reference-heading {{ display: flex; align-items: baseline; gap: 10px; margin-bottom: 12px; }}
  .reference-heading span {{ color: #f25a18; font-size: 11px; font-weight: 800; }}
  .reference-heading h2 {{ font-size: 20px; font-weight: 900; }}
  .reference-list {{ display: grid; gap: 8px; }}
  .reference-row {{ display: grid; grid-template-columns: 28px 1fr; align-items: center; min-height: 42px; border: 2px solid #111111; background: #fff8df; }}
  .reference-row span {{ display: grid; height: 100%; place-items: center; border-right: 2px solid #111111; background: #88d7ff; font-size: 11px; font-weight: 900; }}
  .reference-row p {{ padding: 8px 10px; font-size: 12px; font-weight: 700; line-height: 1.45; }}
  .characters {{ margin: 22px 24px 0; padding: 18px 16px 16px; border: 4px solid #111111; background: #ffffff; }}
  .characters-heading {{ display: flex; align-items: baseline; gap: 10px; margin-bottom: 14px; }}
  .characters-heading span {{ color: #f25a18; font-size: 11px; font-weight: 800; }}
  .characters-heading h2 {{ font-size: 21px; font-weight: 900; }}
  .character-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; }}
  .character-card {{ display: grid; grid-template-columns: 34px 1fr; min-height: 142px; border: 3px solid #111111; background: #fff8df; }}
  .character-rank {{ display: grid; place-items: center; border-right: 2px solid #111111; background: #88d7ff; font-size: 16px; font-weight: 900; }}
  .character-copy {{ min-width: 0; padding: 10px 12px; }}
  .character-copy h3 {{ font-size: 16px; font-weight: 900; line-height: 1.25; }}
  .character-copy strong {{ display: block; margin-top: 4px; color: #f25a18; font-size: 12px; }}
  .character-copy p {{ margin-top: 5px; color: #333333; font-size: 11px; line-height: 1.45; }}
  .character-copy blockquote {{ margin-top: 7px; padding-left: 8px; border-left: 3px solid #111111; font-size: 10px; line-height: 1.45; }}
  .character-copy small {{ display: block; margin-top: 6px; color: #555555; font-size: 10px; }}
  .section {{ margin: 34px 24px 0; }}
  .section-kicker {{ color: #f25a18; }}
  .section h2 {{ margin-top: 6px; }}
  .cases {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 18px; margin-top: 18px; }}
  .case-card {{ min-height: 214px; padding: 16px; border: 4px solid #111111; background: #ffffff; }}
  .case-head {{ display: flex; align-items: center; gap: 12px; }}
  .case-number {{ display: grid; width: 39px; height: 39px; place-items: center; border: 3px solid #111111; border-radius: 50%; background: #ffd92e; font-size: 12px; font-weight: 900; }}
  .case-time {{ color: #f25a18; font-size: 11px; font-weight: 800; }}
  .case-card h3 {{ margin-top: 13px; font-size: 18px; line-height: 1.35; }}
  .case-summary {{ margin-top: 8px; color: #555555; font-size: 12px; line-height: 1.5; }}
  .case-notes {{ margin-top: 14px; padding: 10px 12px 8px 26px; background: #fff0a6; font-size: 11px; line-height: 1.55; }}
  .case-notes li + li {{ margin-top: 4px; }}
  .mvp-grid {{ display: grid; grid-template-columns: repeat(2, 1fr); gap: 10px; margin-top: 18px; }}
  .mvp-card {{ display: grid; grid-template-columns: 34px 1fr 48px; align-items: center; min-height: 70px; border: 3px solid #111111; background: #ffffff; }}
  .mvp-rank {{ display: grid; height: 100%; place-items: center; border-right: 2px solid #111111; font-size: 16px; font-weight: 900; }}
  .mvp-copy {{ padding: 8px 10px; }}
  .mvp-name {{ font-size: 15px; font-weight: 800; }}
  .mvp-meta {{ margin-top: 3px; color: #555555; font-size: 10px; }}
  .mvp-score {{ padding-right: 10px; text-align: right; font-size: 28px; font-weight: 900; }}
  .footer {{ margin-top: 34px; padding: 14px 24px; background: #111111; color: #ffd92e; font-size: 10px; }}
</style>
</head>
<body>
<main class="daily-paper">
  <div class="topline"><span>XIAOMAN CHARACTER DAILY</span><span>{html.escape(report.report_date)}</span></div>
  <header class="hero">
    <div class="hero-group">{html.escape(report.group_name)}</div>
    <div class="hero-title">小满群聊日报</div>
    <div class="hero-mainline">今日主线：{html.escape(main_storyline)}</div>
    <div class="hero-opening">{html.escape(opening_line)}</div>
    <div class="hero-time">{html.escape(report.time_range)} · {report.member_count} 名成员</div>
    <div class="hero-badge">人物<br>主线</div>
  </header>
  {story_index_section}
  {characters_html}
  {highlight_html}
  {callbacks_html}
  {relationships_html}
  {local_life_html}
  {open_questions_html}
  {cases_html}
  <section class="stats">{stats_html}</section>
  <section class="timeline">
    <div class="timeline-head"><h2>24H 活跃节奏</h2><div class="peak">峰值 {peak_count} 条 / {peak_idx:02d}:00</div></div>
    <svg viewBox="0 0 {chart_width} 106" aria-label="24小时活跃节奏">{timeline_svg}{peak_svg}{timeline_labels}</svg>
  </section>
  {mvp_html}
  <footer class="footer">本报告由小满根据最新群聊窗口自动整理 · 长期画像只以公开安全的角色复现计数参与</footer>
</main>
</body>
</html>"""


def _render_daily_markdown(report: ReportData) -> str:
    main_storyline = _main_storyline_label(report)
    callback_candidates = _meme_callback_candidates(report)
    relationship_candidates = _relationship_candidates(report)
    local_life_notes = _ordinary_digest_local_life_notes(report)
    open_questions = _ordinary_digest_open_questions(report)
    lines = [
        f"# 小满群聊日报｜{report.report_date}｜{main_storyline}",
        "",
        "## 今日一句话",
        "",
        _daily_opening_line(report),
        "",
        "## 基本信息",
        "",
        f"- 日期：{report.report_date}",
        f"- 时间范围：{report.time_range}",
        f"- 消息：{report.message_count} 条",
        f"- 活跃：{report.participant_count} 人",
        f"- 可归档主线：{report.case_count} 条",
        f"- 今日剧中人：{report.character_count} 位",
        "",
        "## 天气背景",
        "",
        "今日未接入已审核天气来源，公开日报不硬塞天气。",
        "",
    ]
    topic_cards = _ordinary_digest_topic_cards(report)
    if topic_cards:
        lines.extend(["## 主要话题", ""])
        for topic in topic_cards:
            lines.extend(
                [
                    f"- **{topic['title']}**：{topic['summary']}；参与者 {topic['participants']} 人",
                ]
            )
        lines.append("")
    if report.highlight:
        lines.extend(["## 今日台词", "", f"> {report.highlight}", ""])
    if report.characters:
        lines.extend(["## 今日剧中人", ""])
        for character in report.characters:
            memory = f"（{character.memory_label}）" if character.memory_label else ""
            lines.extend(
                [
                    f"- **{character.name}（{character.role_label}）**："
                    f"{character.story_function}。{character.arc_label or character.one_liner}。"
                    f"{character.callback_hint}{memory}",
                    f"> {character.evidence}",
                    "",
                ]
            )
            if character.relationship_hint:
                lines.extend([f"  同场接力：{character.relationship_hint}", ""])
            if character.expressive_label:
                lines.extend([f"  已审核公开标签：{character.expressive_label}", ""])
    if callback_candidates:
        lines.extend(["## 梗和回调候选", ""])
        lines.extend(f"- {candidate}" for candidate in callback_candidates)
        lines.append("")
    if relationship_candidates:
        lines.extend(["## 同场关系", ""])
        lines.extend(f"- {candidate}" for candidate in relationship_candidates)
        lines.append("")
    if local_life_notes:
        lines.extend(["## 地点 / 本地生活线索", ""])
        lines.extend(
            f"- {item['label']}（{item['source']}）" for item in local_life_notes
        )
        lines.append("")
    if open_questions:
        lines.extend(["## 待解决问题", ""])
        lines.extend(f"- {question}" for question in open_questions)
        lines.append("")
    candidate_topics = _ordinary_digest_candidate_topics(report)
    if candidate_topics:
        lines.extend(["## 候选公众号选题", ""])
        lines.extend(
            f"- {topic['title']}：{topic['reason']}" for topic in candidate_topics
        )
        lines.append("")
    if report.cases:
        lines.extend(["## 今日主线", ""])
        for case in report.cases:
            lines.extend(
                [
                    f"### {case.case_no}｜{case_storyline_label(case)}",
                    "",
                    f"- 时间：{case.time_label}",
                    f"- 规模：{case.summary}",
                    f"- 主讲：{case.top_speaker}",
                    "",
                ]
            )
            lines.extend(f"- {bullet}" for bullet in case.bullets[:3])
            lines.append("")
    if report.suspects:
        lines.extend(["## 发言出场榜", ""])
        for suspect in report.suspects:
            lines.append(
                f"- {suspect.rank}. {suspect.name}：{suspect.message_count} 条 / {suspect.word_count} 字"
            )
        lines.append("")
    universe = report.character_universe or {}
    if universe.get("storyline_candidates"):
        lines.extend(["## 可沉淀故事线", ""])
        for item in universe["storyline_candidates"][:5]:
            lines.append(f"- [[{item['label']}]]：{item['reason']}")
        lines.append("")
    if universe.get("creative_profile_candidates"):
        lines.extend(["## 可审核人物画像候选", ""])
        for item in universe["creative_profile_candidates"][:5]:
            lines.append(
                f"- {item['candidate_role_label']} / {item['story_function']}："
                f"{item['daily_arc']}（{item['profile_upgrade_status']}；"
                f"evidence_count={item['recurrence_evidence_count']}；{item['evidence_policy']}）"
            )
        lines.append("")
    lines.extend(
        [
            "## 公开边界",
            "",
            "- 本日报由小满根据最新群聊窗口自动整理。",
            "- 长期画像只以角色复现计数参与，不展示内部画像原文。",
            "- creative_profile_candidates 仅供内部审核，不写入长期画像表，不允许直接公开展示。",
            "- expressive_label_candidates 只有 owner-reviewed safe_reply_hints 字段可进入公开文案。",
            "- raw_messages_included=false；profile_fact_text_included=false。",
        ]
    )
    return "\n".join(lines)


def _file_url(path: Path) -> str:
    return path.resolve().as_uri()


def _font_candidates() -> list[str]:
    configured = os.environ.get("QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_FONT")
    candidates = [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
    ]
    return ([configured] if configured else []) + candidates


def _pil_font(size: int, *, bold: bool = False) -> Any:
    """Load a CJK-capable TrueType font, or fail closed.

    Pillow's ``ImageFont.load_default()`` is a bitmap font with no ``.size``
    attribute and no CJK glyphs: it either raised AttributeError while measuring
    lines or shipped garbled (tofu) posters. When no usable font exists we raise
    so the caller can drop the image instead of shipping mojibake. This matches
    the erhua morning-brief renderer's policy.
    """
    from PIL import ImageFont

    candidates = _font_candidates()
    if bold:
        candidates = [
            "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc",
            "/System/Library/Fonts/PingFang.ttc",
        ] + candidates
    for candidate in candidates:
        if candidate and Path(candidate).exists():
            return ImageFont.truetype(candidate, size=size)
    raise RuntimeError(
        "No CJK-capable font available for the xiaoman daily-case-report Pillow "
        "fallback. Install Noto Sans CJK / PingFang or set "
        "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_FONT to a .ttf/.ttc path. "
        "Refusing to render garbled text."
    )


def _wrap_for_draw(draw: Any, text: str, font: Any, max_width: int) -> list[str]:
    words = list(text)
    lines: list[str] = []
    line = ""
    for word in words:
        candidate = line + word
        if line and draw.textlength(candidate, font=font) > max_width:
            lines.append(line)
            line = word
        else:
            line = candidate
    if line:
        lines.append(line)
    return lines or [""]


def _draw_wrapped_text(
    draw: Any,
    xy: tuple[int, int],
    text: str,
    font: Any,
    fill: str,
    max_width: int,
    *,
    line_gap: int = 8,
    max_lines: int | None = None,
) -> int:
    x, y = xy
    lines = _wrap_for_draw(draw, text, font, max_width)
    if max_lines is not None and len(lines) > max_lines:
        lines = lines[:max_lines]
        lines[-1] = lines[-1].rstrip("，。；、 ") + "..."
    line_height = int(font.size * 1.35) if hasattr(font, "size") else 24
    for line in lines:
        draw.text((x, y), line, font=font, fill=fill)
        y += line_height + line_gap
    return y


def _draw_card(
    draw: Any,
    box: tuple[int, int, int, int],
    fill: str,
    outline: str = "#d8c7a2",
) -> None:
    draw.rounded_rectangle(box, radius=18, fill=fill, outline=outline, width=2)


def _render_image_with_pillow(
    report: ReportData,
    output_path: Path,
    width: int,
    image_format: str,
) -> None:
    try:
        from PIL import Image, ImageDraw
    except ImportError as exc:
        raise RuntimeError(
            "image rendering requires playwright or Pillow; use --render html only for non-production debugging"
        ) from exc

    scale = 2
    canvas_width = width * scale
    outer = 22 * scale
    padding = 44 * scale
    gutter = 18 * scale
    content_width = canvas_width - padding * 2
    image = Image.new("RGB", (canvas_width, 7600), "#fff8df")
    draw = ImageDraw.Draw(image)

    title_font = _pil_font(44 * scale, bold=True)
    hero_font = _pil_font(26 * scale, bold=True)
    section_font = _pil_font(24 * scale, bold=True)
    card_title_font = _pil_font(18 * scale, bold=True)
    stat_font = _pil_font(31 * scale, bold=True)
    body_font = _pil_font(16 * scale)
    small_font = _pil_font(12 * scale)
    tiny_font = _pil_font(10 * scale)
    mono_font = _pil_font(15 * scale, bold=True)

    ink = "#111111"
    yellow = "#ffd92e"
    orange = "#f25a18"
    cream = "#fff8df"
    pale_yellow = "#fff0a6"
    blue = "#88d7ff"
    main_storyline = _main_storyline_label(report)
    opening_line = _daily_opening_line(report)
    callback_candidates = _meme_callback_candidates(report)
    relationship_candidates = _relationship_candidates(report)
    local_life_notes = _ordinary_digest_local_life_notes(report)
    open_questions = _ordinary_digest_open_questions(report)

    def text_right(x: int, y: int, text: str, font: Any, fill: str) -> None:
        box = draw.textbbox((0, 0), text, font=font)
        draw.text((x - (box[2] - box[0]), y), text, font=font, fill=fill)

    def section_label(y_pos: int, kicker: str, title: str) -> int:
        draw.text((padding, y_pos), kicker, font=tiny_font, fill=orange)
        y_pos += 20 * scale
        draw.text((padding, y_pos), title, font=section_font, fill=ink)
        return y_pos + 42 * scale

    def reference_list(y_pos: int, kicker: str, title: str, rows: list[str], fill: str) -> int:
        if not rows:
            return y_pos
        top = y_pos
        height = (54 + 42 * len(rows)) * scale
        draw.rectangle(
            (outer, top, canvas_width - outer, top + height),
            fill=fill,
            outline=ink,
            width=3 * scale,
        )
        draw.text((padding, top + 14 * scale), kicker, font=tiny_font, fill=orange)
        draw.text((padding + 128 * scale, top + 10 * scale), title, font=body_font, fill=ink)
        for index, row in enumerate(rows):
            row_top = top + (44 + index * 38) * scale
            draw.rectangle(
                (padding, row_top, padding + content_width, row_top + 30 * scale),
                fill=cream,
                outline=ink,
                width=2 * scale,
            )
            draw.rectangle(
                (padding, row_top, padding + 28 * scale, row_top + 30 * scale),
                fill=blue,
                outline=ink,
                width=2 * scale,
            )
            draw.text((padding + 10 * scale, row_top + 8 * scale), str(index + 1), font=tiny_font, fill=ink)
            _draw_wrapped_text(
                draw,
                (padding + 40 * scale, row_top + 7 * scale),
                row,
                tiny_font,
                ink,
                content_width - 52 * scale,
                max_lines=1,
                line_gap=0,
            )
        return top + height + 38 * scale

    y = outer
    draw.rectangle((outer, y, canvas_width - outer, y + 42 * scale), fill=ink)
    draw.text((padding, y + 12 * scale), "XIAOMAN CHARACTER DAILY", font=tiny_font, fill=yellow)
    text_right(canvas_width - padding, y + 12 * scale, report.report_date, tiny_font, yellow)
    y += 42 * scale

    hero_top = y
    hero_height = 206 * scale
    draw.rectangle((outer, hero_top, canvas_width - outer, hero_top + hero_height), fill=yellow, outline=ink, width=3 * scale)
    draw.text((padding, hero_top + 20 * scale), report.group_name, font=hero_font, fill=ink)
    draw.text((padding, hero_top + 60 * scale), "小满群聊日报", font=title_font, fill=ink)
    _draw_wrapped_text(
        draw,
        (padding, hero_top + 112 * scale),
        f"今日主线：{main_storyline}",
        body_font,
        ink,
        content_width - 132 * scale,
        max_lines=1,
    )
    _draw_wrapped_text(
        draw,
        (padding, hero_top + 142 * scale),
        opening_line,
        small_font,
        "#2b2b2b",
        content_width - 132 * scale,
        max_lines=2,
        line_gap=2 * scale,
    )
    draw.rectangle((padding, hero_top + 184 * scale, canvas_width - padding - 126 * scale, hero_top + 188 * scale), fill=ink)
    draw.text((padding, hero_top + 192 * scale), report.time_range, font=tiny_font, fill=ink)
    badge_box = (canvas_width - padding - 106 * scale, hero_top + 24 * scale, canvas_width - padding, hero_top + 130 * scale)
    draw.rounded_rectangle(badge_box, radius=12 * scale, fill=blue, outline=ink, width=3 * scale)
    draw.text((badge_box[0] + 22 * scale, badge_box[1] + 30 * scale), "人物", font=hero_font, fill=ink)
    draw.text((badge_box[0] + 22 * scale, badge_box[1] + 66 * scale), "主线", font=hero_font, fill=ink)
    y = hero_top + hero_height

    story_index_cases = report.cases[:4]
    if story_index_cases:
        story_index_top = y
        story_index_height = 136 * scale
        draw.rectangle((outer, story_index_top, canvas_width - outer, story_index_top + story_index_height), fill=ink)
        draw.text((padding, story_index_top + 16 * scale), "DAILY WORKSHOP INDEX", font=tiny_font, fill=yellow)
        text_right(
            canvas_width - padding,
            story_index_top + 16 * scale,
            (
                f"{report.message_count} 条素材 / {report.participant_count} 位出场 / "
                f"{report.case_count} 条主线 / {report.character_count} 张人物卡"
            ),
            tiny_font,
            pale_yellow,
        )
        index_card_width = (content_width - gutter) // 2
        for index, case in enumerate(story_index_cases):
            column = index % 2
            row = index // 2
            x = padding + column * (index_card_width + gutter)
            row_top = story_index_top + (44 + row * 42) * scale
            draw.rectangle(
                (x, row_top, x + index_card_width, row_top + 34 * scale),
                fill="#1c1c1c",
                outline=cream,
                width=2 * scale,
            )
            draw.ellipse(
                (x + 8 * scale, row_top + 6 * scale, x + 30 * scale, row_top + 28 * scale),
                outline=yellow,
                width=2 * scale,
            )
            draw.text((x + 12 * scale, row_top + 9 * scale), f"{index + 1:02d}", font=tiny_font, fill=yellow)
            draw.text((x + 40 * scale, row_top + 6 * scale), case_storyline_label(case)[:10], font=small_font, fill=cream)
            text_right(x + index_card_width - 10 * scale, row_top + 10 * scale, case.summary, tiny_font, "#c9c9c9")
        y = story_index_top + story_index_height

    if report.characters:
        character_top = y
        character_rows = (len(report.characters) + 1) // 2
        character_height = (58 + 138 * character_rows) * scale
        draw.rectangle((outer, character_top, canvas_width - outer, character_top + character_height), fill="#ffffff", outline=ink, width=3 * scale)
        draw.text((padding, character_top + 16 * scale), "CAST NOTES", font=tiny_font, fill=orange)
        draw.text((padding + 112 * scale, character_top + 12 * scale), "人物出场表", font=body_font, fill=ink)
        card_width = (content_width - gutter) // 2
        card_height = 116 * scale
        for index, character in enumerate(report.characters):
            column = index % 2
            row = index // 2
            x = padding + column * (card_width + gutter)
            card_y = character_top + (48 + row * 128) * scale
            draw.rectangle((x, card_y, x + card_width, card_y + card_height), fill=cream, outline=ink, width=2 * scale)
            draw.rectangle((x, card_y, x + 34 * scale, card_y + card_height), fill=blue, outline=ink, width=2 * scale)
            draw.text((x + 12 * scale, card_y + 42 * scale), str(character.rank), font=tiny_font, fill=ink)
            copy_x = x + 46 * scale
            draw.text((copy_x, card_y + 10 * scale), character.name, font=body_font, fill=ink)
            draw.text((copy_x, card_y + 34 * scale), f"{character.role_label} · {character.story_function}", font=tiny_font, fill=orange)
            _draw_wrapped_text(
                draw,
                (copy_x, card_y + 54 * scale),
                (
                    f"已审核标签：{character.expressive_label}｜"
                    if character.expressive_label
                    else ""
                )
                + f"{character.arc_label or character.one_liner}｜{character.evidence}",
                tiny_font,
                ink,
                card_width - 58 * scale,
                max_lines=3,
                line_gap=2 * scale,
            )
            text_right(
                x + card_width - 10 * scale,
                card_y + 92 * scale,
                f"{character.message_count} 条 · {character.topic_count} 触点",
                tiny_font,
                "#555555",
            )
            if character.memory_label:
                _draw_wrapped_text(
                    draw,
                    (copy_x, card_y + 92 * scale),
                    character.memory_label,
                    tiny_font,
                    "#555555",
                    card_width - 104 * scale,
                    max_lines=1,
                    line_gap=0,
                )
        y = character_top + character_height + 38 * scale

    if report.highlight:
        highlight_top = y
        highlight_height = 138 * scale
        draw.rectangle((outer, highlight_top, canvas_width - outer, highlight_top + highlight_height), fill=orange, outline=ink, width=3 * scale)
        draw.text((padding, highlight_top + 22 * scale), "QUOTE ANCHOR", font=tiny_font, fill=yellow)
        draw.text((padding, highlight_top + 46 * scale), "今日台词", font=section_font, fill=cream)
        _draw_wrapped_text(
            draw,
            (padding + 180 * scale, highlight_top + 30 * scale),
            f"“{report.highlight}”",
            body_font,
            cream,
            content_width - 180 * scale,
            max_lines=3,
        )
        y += highlight_height + 38 * scale

    if callback_candidates:
        hotlist_top = y
        hotlist_rows = (len(callback_candidates) + 1) // 2
        hotlist_height = (54 + 50 * hotlist_rows) * scale
        draw.rectangle((outer, hotlist_top, canvas_width - outer, hotlist_top + hotlist_height), fill=cream, outline=ink, width=3 * scale)
        draw.text((padding, hotlist_top + 14 * scale), "MEME MAP", font=tiny_font, fill=orange)
        draw.text((padding + 102 * scale, hotlist_top + 10 * scale), "梗和回调候选", font=body_font, fill=ink)
        topic_width = (content_width - gutter) // 2
        for index, candidate in enumerate(callback_candidates):
            label, _, detail = candidate.partition("：")
            column = index % 2
            row = index // 2
            x = padding + column * (topic_width + gutter)
            row_top = hotlist_top + (44 + row * 46) * scale
            draw.rectangle((x, row_top, x + topic_width, row_top + 34 * scale), fill=pale_yellow, outline=ink, width=2 * scale)
            draw.ellipse((x + 8 * scale, row_top + 7 * scale, x + 30 * scale, row_top + 29 * scale), fill=yellow, outline=ink, width=2 * scale)
            draw.text((x + 14 * scale, row_top + 10 * scale), str(index + 1), font=tiny_font, fill=ink)
            draw.text((x + 40 * scale, row_top + 8 * scale), label[:8], font=small_font, fill=ink)
            _draw_wrapped_text(
                draw,
                (x + 120 * scale, row_top + 8 * scale),
                detail,
                tiny_font,
                "#555555",
                topic_width - 130 * scale,
                max_lines=1,
                line_gap=0,
            )
        y = hotlist_top + hotlist_height + 38 * scale

    if relationship_candidates:
        relationship_top = y
        relationship_height = (54 + 42 * len(relationship_candidates)) * scale
        draw.rectangle((outer, relationship_top, canvas_width - outer, relationship_top + relationship_height), fill=blue, outline=ink, width=3 * scale)
        draw.text((padding, relationship_top + 14 * scale), "ENSEMBLE LINKS", font=tiny_font, fill=ink)
        draw.text((padding + 132 * scale, relationship_top + 10 * scale), "同场关系", font=body_font, fill=ink)
        row_width = content_width
        for index, candidate in enumerate(relationship_candidates):
            row_top = relationship_top + (44 + index * 38) * scale
            draw.rectangle((padding, row_top, padding + row_width, row_top + 30 * scale), fill="#ffffff", outline=ink, width=2 * scale)
            draw.rectangle((padding, row_top, padding + 28 * scale, row_top + 30 * scale), fill=yellow, outline=ink, width=2 * scale)
            draw.text((padding + 10 * scale, row_top + 8 * scale), str(index + 1), font=tiny_font, fill=ink)
            _draw_wrapped_text(
                draw,
                (padding + 40 * scale, row_top + 7 * scale),
                candidate,
                tiny_font,
                ink,
                row_width - 52 * scale,
                max_lines=1,
                line_gap=0,
            )
        y = relationship_top + relationship_height + 38 * scale

    y = reference_list(
        y,
        "LOCAL THREADS",
        "地点 / 本地生活线索",
        [str(item.get("label", "")) for item in local_life_notes],
        "#ffffff",
    )
    y = reference_list(
        y,
        "OPEN LOOPS",
        "待解决问题",
        open_questions,
        pale_yellow,
    )

    y = section_label(y, "STORYLINE FILES", "故事线候选")
    cases = report.cases[:DEFAULT_CASE_LIMIT]
    if not cases:
        draw.rectangle((padding, y, padding + content_width, y + 92 * scale), fill="#ffffff", outline=ink, width=3 * scale)
        draw.text((padding + 22 * scale, y + 32 * scale), "过去 24 小时暂无可归档话题。", font=body_font, fill="#555555")
        y += 116 * scale
    else:
        card_width = (content_width - gutter) // 2
        card_height = 226 * scale
        for index, case in enumerate(cases):
            col = index % 2
            row = index // 2
            x = padding + col * (card_width + gutter)
            card_y = y + row * (card_height + gutter)
            draw.rectangle((x, card_y, x + card_width, card_y + card_height), fill="#ffffff", outline=ink, width=3 * scale)
            draw.ellipse((x + 18 * scale, card_y + 18 * scale, x + 58 * scale, card_y + 58 * scale), fill=yellow, outline=ink, width=3 * scale)
            draw.text((x + 28 * scale, card_y + 26 * scale), f"{index + 1:02d}", font=tiny_font, fill=ink)
            draw.text((x + 72 * scale, card_y + 22 * scale), case.time_label, font=tiny_font, fill=orange)
            title_y = _draw_wrapped_text(
                draw,
                (x + 18 * scale, card_y + 72 * scale),
                case_storyline_label(case),
                card_title_font,
                ink,
                card_width - 36 * scale,
                max_lines=2,
                line_gap=4 * scale,
            )
            _draw_wrapped_text(
                draw,
                (x + 18 * scale, title_y + 4 * scale),
                case.summary,
                small_font,
                "#444444",
                card_width - 36 * scale,
                max_lines=1,
            )
            note_top = card_y + 150 * scale
            draw.rectangle((x + 18 * scale, note_top, x + card_width - 18 * scale, card_y + card_height - 18 * scale), fill=pale_yellow)
            bullet_text = " / ".join(case.bullets[:2])
            _draw_wrapped_text(
                draw,
                (x + 30 * scale, note_top + 12 * scale),
                bullet_text,
                tiny_font,
                ink,
                card_width - 60 * scale,
                max_lines=3,
                line_gap=3 * scale,
            )
        y += ((len(cases) + 1) // 2) * (card_height + gutter) + 18 * scale

    stats = [
        ("消息", report.message_count, "当日素材"),
        ("出场", report.participant_count, "活跃成员"),
        ("主线", report.case_count, "可归档"),
        ("人物", report.character_count, "群像卡"),
    ]
    stat_height = 70 * scale
    stat_width = content_width // 4
    draw.rectangle((padding, y, padding + content_width, y + stat_height), fill="#ffffff", outline=ink, width=3 * scale)
    for index, (label, value, caption) in enumerate(stats):
        x = padding + index * stat_width
        if index:
            draw.line((x, y, x, y + stat_height), fill=ink, width=2 * scale)
        draw.text((x + 16 * scale, y + 11 * scale), label, font=tiny_font, fill=orange)
        draw.text((x + 16 * scale, y + 31 * scale), str(value), font=section_font, fill=ink)
        draw.text((x + 76 * scale, y + 38 * scale), caption, font=tiny_font, fill="#555555")
    y += stat_height + 24 * scale

    timeline_top = y
    timeline_height = 145 * scale
    draw.rectangle((outer, timeline_top, canvas_width - outer, timeline_top + timeline_height), fill=yellow, outline=ink, width=3 * scale)
    draw.text((padding, timeline_top + 28 * scale), "24H 活跃节奏", font=section_font, fill=ink)
    peak_count = max(report.hourly_counts or [0])
    max_count = peak_count or 1
    peak_idx = report.hourly_counts.index(peak_count) if report.hourly_counts and peak_count else 0
    text_right(canvas_width - padding, timeline_top + 34 * scale, f"峰值 {peak_count} 条 / {peak_idx:02d}:00", small_font, ink)
    bar_left = padding + 200 * scale
    bar_width_area = canvas_width - padding - bar_left
    bar_gap = 5 * scale
    bar_width = max(4 * scale, (bar_width_area - bar_gap * 23) // 24)
    base_y = timeline_top + 112 * scale
    for index, count in enumerate(report.hourly_counts[:24]):
        height = int((count / max_count) * 72 * scale)
        x = bar_left + index * (bar_width + bar_gap)
        fill = orange if index == peak_idx and count else ink if count else "#e6c737"
        draw.rectangle((x, base_y - height, x + bar_width, base_y), fill=fill)
    y = timeline_top + timeline_height + 34 * scale

    y = section_label(y, "VOICE INDEX", "发言出场榜")
    if not report.suspects:
        draw.text((padding, y), "暂无发言榜。", font=body_font, fill="#555555")
        y += 54 * scale
    else:
        row_height = 72 * scale
        for index, suspect in enumerate(report.suspects[:DEFAULT_SUSPECT_LIMIT]):
            x = padding + (index % 2) * ((content_width - gutter) // 2 + gutter)
            row_y = y + (index // 2) * (row_height + 10 * scale)
            row_width = (content_width - gutter) // 2
            draw.rectangle((x, row_y, x + row_width, row_y + row_height), fill="#ffffff", outline=ink, width=2 * scale)
            draw.text((x + 16 * scale, row_y + 20 * scale), str(suspect.rank), font=mono_font, fill=ink)
            draw.text((x + 58 * scale, row_y + 14 * scale), suspect.name, font=body_font, fill=ink)
            draw.text((x + 58 * scale, row_y + 42 * scale), f"{suspect.message_count} 条 / {suspect.word_count} 字", font=tiny_font, fill="#555555")
            text_right(x + row_width - 16 * scale, row_y + 21 * scale, f"{suspect.message_count}", stat_font, ink)
        y += ((min(len(report.suspects), DEFAULT_SUSPECT_LIMIT) + 1) // 2) * (row_height + 10 * scale) + 28 * scale

    draw.rectangle((outer, y, canvas_width - outer, y + 42 * scale), fill=ink)
    draw.text((padding, y + 14 * scale), "本报告由小满根据最新群聊窗口自动整理 · 长期画像仅以角色复现计数参与", font=tiny_font, fill=yellow)
    y += 42 * scale + outer

    cropped = image.crop((0, 0, canvas_width, min(y, image.height)))
    save_kwargs: dict[str, Any] = {}
    if image_format == "jpeg":
        save_kwargs["quality"] = DEFAULT_JPEG_QUALITY
        save_kwargs["optimize"] = True
        save_format = "JPEG"
    else:
        save_format = "PNG"
    cropped.save(output_path, format=save_format, **save_kwargs)


def _render_image(
    html_path: Path,
    output_path: Path,
    width: int,
    image_format: str,
    report: ReportData | None = None,
) -> None:
    try:
        _render_image_with_playwright(html_path, output_path, width, image_format)
    except Exception as exc:
        if report is not None:
            _render_image_with_pillow(report, output_path, width, image_format)
            return
        raise RuntimeError(
            "image rendering requires playwright or a report object for Pillow fallback"
        ) from exc


def _render_image_with_playwright(
    html_path: Path,
    output_path: Path,
    width: int,
    image_format: str,
) -> None:
    from playwright.sync_api import sync_playwright

    screenshot_options: dict[str, Any] = {
        "path": str(output_path),
        "full_page": True,
        "type": image_format,
    }
    if image_format == "jpeg":
        screenshot_options["quality"] = DEFAULT_JPEG_QUALITY

    with sync_playwright() as p:
        browser = p.chromium.launch()
        page = browser.new_page(viewport={"width": width, "height": 100}, device_scale_factor=2)
        page.route(
            "**/*",
            lambda route: route.abort()
            if route.request.url.startswith(("http://", "https://"))
            else route.continue_(),
        )
        page.goto(_file_url(html_path), wait_until="load")
        height = page.evaluate("document.body.scrollHeight")
        page.set_viewport_size({"width": width, "height": height})
        page.screenshot(**screenshot_options)
        browser.close()


def _render_png(html_path: Path, output_path: Path, width: int) -> None:
    _render_image(html_path, output_path, width, "png")


def _image_mime_type(image_format: str) -> str:
    return "image/jpeg" if image_format == "jpeg" else "image/png"


def _image_extension(image_format: str) -> str:
    return "jpg" if image_format == "jpeg" else "png"
