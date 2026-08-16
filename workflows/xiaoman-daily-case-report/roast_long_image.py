"""Roast long-image renderer for the Xiaoman daily report.

Takes the narrative markdown (from narrative_generator) and report metadata,
parses it into structured sections, and renders a long-form article HTML
suitable for Playwright JPEG screenshot.

Privacy boundary: same as newspaper_elegant — no chat images, no network,
only text metadata from the deterministic report + LLM narrative.
"""

from __future__ import annotations

import html
import re
from typing import Any


def _split_title_line(line: str) -> tuple[str, str, str]:
    """Parse '# kicker | date | subtitle' into (kicker, date, subtitle)."""
    text = line.lstrip("#").strip()
    parts = [p.strip() for p in text.split("|")]
    if len(parts) >= 3:
        return parts[0], parts[1], parts[2]
    if len(parts) == 2:
        return parts[0], parts[1], ""
    return text, "", ""


def _parse_narrative(md: str) -> dict[str, Any]:
    """Parse roast narrative markdown into structured sections."""
    lines = md.strip().split("\n")

    # Remove --- separators and footer line
    cleaned: list[str] = []
    for line in lines:
        stripped = line.strip()
        if stripped == "---":
            continue
        if stripped.startswith("*") and stripped.endswith("*") and "吐槽" in stripped:
            continue
        cleaned.append(line)

    text = "\n".join(cleaned)

    # Extract # title line
    kicker = ""
    date_line = ""
    title = ""
    title_match = re.match(r"^#\s+(.+)$", text, re.MULTILINE)
    if title_match:
        kicker, date_line, title = _split_title_line(title_match.group(1))
        text = text[title_match.end():].lstrip("\n")

    # Extract war report
    war_report = ""
    war_match = re.match(r"^\*\*战报\*\*[：:]\s*(.+)$", text, re.MULTILINE)
    if war_match:
        war_report = war_match.group(1).strip()
        text = text[:war_match.start()] + text[war_match.end():]
        text = text.lstrip("\n")

    # Split by ## sections
    raw_sections = re.split(r"^##\s+", text, flags=re.MULTILINE)

    chapters: list[dict[str, Any]] = []
    tomorrow = ""
    characters: list[dict[str, str]] = []
    final_quote: dict[str, str] = {"text": "", "author": ""}

    for section in raw_sections[1:]:  # Skip content before first ##
        if not section.strip():
            continue

        nl_idx = section.find("\n")
        if nl_idx == -1:
            section_title = section.strip()
            section_body = ""
        else:
            section_title = section[:nl_idx].strip()
            section_body = section[nl_idx + 1:].strip()

        if re.match(r"第.{1,3}章", section_title):
            chapters.append(_parse_chapter(section_title, section_body))
        elif "明日" in section_title or "前瞻" in section_title:
            tomorrow = section_body.strip()
        elif "人物速写" in section_title or "人物" in section_title:
            characters = _parse_characters(section_body)
        elif "金句" in section_title or "最佳" in section_title:
            final_quote = _parse_final_quote(section_body)

    return {
        "kicker": kicker,
        "date_line": date_line,
        "title": title,
        "war_report": war_report,
        "chapters": chapters,
        "tomorrow": tomorrow,
        "characters": characters,
        "final_quote": final_quote,
    }


def _parse_chapter(title: str, body: str) -> dict[str, Any]:
    """Parse a chapter section into title, paragraphs, and golden quote."""
    paragraphs: list[str] = []
    golden_quote = ""

    blocks = re.split(r"\n\s*\n", body)
    for block in blocks:
        block = block.strip()
        if not block:
            continue
        # Filter out image references like ![[undefined]]
        if block.startswith("![["):
            continue
        # Match golden quote in various formats:
        #   **金句：text**  /  金句：**text**  /  金句：text
        q_match = (
            re.match(r"^\*\*金句[：:]\s*(.+)\*\*$", block)
            or re.match(r"^金句[：:]\s*\*\*(.+)\*\*$", block)
            or re.match(r"^金句[：:]\s*(.+)$", block)
        )
        if q_match:
            golden_quote = q_match.group(1).strip().rstrip("*")
        else:
            # Filter out standalone image caption lines like _sender time — caption_
            if block.startswith("_") and block.endswith("_") and "—" in block:
                continue
            paragraphs.append(block)

    return {
        "title": title,
        "paragraphs": paragraphs,
        "golden_quote": golden_quote,
    }


def _parse_characters(body: str) -> list[dict[str, str]]:
    """Parse character cards from blockquotes."""
    characters: list[dict[str, str]] = []
    blocks = re.split(r"\n\s*\n", body)
    for block in blocks:
        block = block.strip()
        if not block.startswith(">"):
            continue
        lines = [line.lstrip(">").strip() for line in block.split("\n")]
        first = lines[0]
        name_match = re.match(r"\*\*(.+?)\*\*", first)
        if name_match:
            name = name_match.group(1)
            desc = " ".join(lines[1:]).strip()
            characters.append({"name": name, "desc": desc})
    return characters


def _parse_final_quote(body: str) -> dict[str, str]:
    """Parse the final quote section."""
    body = body.strip()
    match = re.match(r'^\*\*"(.+?)"\*\*\s*[—\-]{1,3}\s*(.+)$', body)
    if match:
        return {
            "text": match.group(1).strip(),
            "author": match.group(2).strip(),
        }
    return {"text": body, "author": ""}


_CSS = """\
  * { margin: 0; padding: 0; box-sizing: border-box; }

  body {
    width: {width}px;
    background: #fbfaf6;
    font-family: "Songti SC", "Noto Serif SC", "STSong", "SimSun", serif;
    color: #2a2a2a;
    line-height: 2.0;
    padding: 70px 110px 60px;
  }

  .kicker {
    text-align: center;
    font-size: 18px;
    letter-spacing: 10px;
    color: #8b1f2f;
    font-weight: 600;
    margin-bottom: 18px;
  }

  .title {
    text-align: center;
    font-size: 38px;
    font-weight: 700;
    color: #1a1a1a;
    line-height: 1.5;
    margin-bottom: 14px;
  }

  .date-line {
    text-align: center;
    font-size: 17px;
    color: #999;
    margin-bottom: 36px;
    letter-spacing: 1px;
  }

  .divider-top {
    border: none;
    border-top: 4px solid #8b1f2f;
    margin: 0 0 36px;
  }
  .divider-top::after {
    content: "";
    display: block;
    border-top: 1px solid #8b1f2f;
    margin-top: 4px;
  }

  .war-report {
    background: #f5f0e8;
    border-left: 5px solid #8b1f2f;
    padding: 24px 28px;
    font-size: 20px;
    line-height: 2.0;
    margin-bottom: 42px;
    border-radius: 0 8px 8px 0;
  }
  .war-report .label {
    font-size: 15px;
    color: #8b1f2f;
    font-weight: 600;
    letter-spacing: 3px;
    margin-bottom: 8px;
  }

  .chapter {
    margin-bottom: 40px;
  }

  .chapter h2 {
    font-size: 26px;
    font-weight: 700;
    color: #1a1a1a;
    margin-bottom: 16px;
    padding-left: 16px;
    border-left: 5px solid #8b1f2f;
    line-height: 1.5;
  }

  .chapter p {
    font-size: 21px;
    line-height: 2.05;
    margin-bottom: 14px;
    text-align: justify;
  }

  .golden-quote {
    font-size: 20px;
    font-weight: 600;
    color: #8b1f2f;
    margin-top: 18px;
    padding: 12px 0;
    border-top: 1px solid #e0d5c5;
    border-bottom: 1px solid #e0d5c5;
    text-align: center;
  }

  .tomorrow {
    background: #f0ede5;
    padding: 24px 28px;
    border-radius: 10px;
    margin-bottom: 42px;
  }
  .tomorrow h2 {
    font-size: 23px;
    color: #8b1f2f;
    margin-bottom: 10px;
  }
  .tomorrow p {
    font-size: 20px;
    line-height: 2.0;
  }

  .characters {
    margin-bottom: 42px;
  }
  .characters h2 {
    font-size: 26px;
    font-weight: 700;
    margin-bottom: 20px;
    padding-left: 16px;
    border-left: 5px solid #8b1f2f;
  }
  .char-card {
    background: #fff;
    border: 1px solid #e8e0d0;
    border-radius: 10px;
    padding: 18px 24px;
    margin-bottom: 14px;
  }
  .char-card .name {
    font-size: 20px;
    font-weight: 700;
    color: #8b1f2f;
    margin-bottom: 6px;
  }
  .char-card .desc {
    font-size: 19px;
    line-height: 1.9;
    color: #444;
  }

  .final-quote {
    text-align: center;
    margin-bottom: 42px;
    padding: 28px;
    background: linear-gradient(135deg, #8b1f2f 0%, #6b1722 100%);
    border-radius: 12px;
    color: #fff;
  }
  .final-quote .label {
    font-size: 15px;
    letter-spacing: 5px;
    opacity: 0.85;
    margin-bottom: 10px;
  }
  .final-quote .text {
    font-size: 24px;
    font-weight: 600;
    line-height: 1.6;
    margin-bottom: 10px;
  }
  .final-quote .author {
    font-size: 17px;
    opacity: 0.85;
  }

  .divider-bottom {
    border: none;
    border-top: 1px solid #d0c8b8;
    margin: 24px 0;
  }
  .footer {
    text-align: center;
    font-size: 15px;
    color: #aaa;
    line-height: 1.8;
  }"""


def render(input_data: dict[str, Any]) -> str:
    """Render the roast long-image HTML.

    Required keys:
        - narrative_md: str (roast markdown from narrative_generator)
    Optional keys:
        - report_date: str (fallback for date display)
        - time_range: str (fallback for time display)
        - group_name: str
        - message_count: int
        - participant_count: int
        - width: int (default 1080)
    """
    md = input_data.get("narrative_md", "")
    if not md:
        raise RuntimeError("roast_long_image.render requires narrative_md")

    parsed = _parse_narrative(md)

    width = input_data.get("width", 1080)
    kicker = parsed["kicker"] or "秦托邦吐槽日报"
    kicker_spaced = " ".join(kicker)

    title = parsed["title"] or "今日群聊观察"
    date_line = parsed["date_line"] or input_data.get("report_date", "")

    # Build war report
    war_html = ""
    if parsed["war_report"]:
        war_html = (
            '<div class="war-report">\n'
            '  <div class="label">战 报</div>\n'
            f"  {html.escape(parsed['war_report'])}\n"
            "</div>\n"
        )

    # Build chapters
    chapters_html = ""
    for ch in parsed["chapters"]:
        paras = "".join(
            f"  <p>{html.escape(p)}</p>\n" for p in ch["paragraphs"]
        )
        gq = ""
        if ch["golden_quote"]:
            gq = f'  <div class="golden-quote">{html.escape(ch["golden_quote"])}</div>\n'
        chapters_html += (
            '<div class="chapter">\n'
            f"  <h2>{html.escape(ch['title'])}</h2>\n"
            f"{paras}"
            f"{gq}"
            "</div>\n"
        )

    # Build tomorrow section
    tomorrow_html = ""
    if parsed["tomorrow"]:
        tomorrow_html = (
            '<div class="tomorrow">\n'
            "  <h2>明日线索</h2>\n"
            f"  <p>{html.escape(parsed['tomorrow'])}</p>\n"
            "</div>\n"
        )

    # Build characters section
    characters_html = ""
    if parsed["characters"]:
        cards = ""
        for c in parsed["characters"]:
            cards += (
                '    <div class="char-card">\n'
                f'      <div class="name">{html.escape(c["name"])}</div>\n'
                f'      <div class="desc">{html.escape(c["desc"])}</div>\n'
                "    </div>\n"
            )
        characters_html = (
            '<div class="characters">\n'
            "  <h2>今日人物速写</h2>\n"
            f"{cards}"
            "</div>\n"
        )

    # Build final quote
    final_quote_html = ""
    fq = parsed["final_quote"]
    if fq["text"]:
        author = f"—— {html.escape(fq['author'])}" if fq["author"] else ""
        final_quote_html = (
            '<div class="final-quote">\n'
            '  <div class="label">今 日 金 句</div>\n'
            f'  <div class="text">"{html.escape(fq["text"])}"</div>\n'
            f'  <div class="author">{author}</div>\n'
            "</div>\n"
        )

    css = _CSS.replace("{width}", str(width))

    return (
        "<!DOCTYPE html>\n"
        '<html lang="zh-CN">\n'
        "<head>\n"
        '<meta charset="UTF-8">\n'
        '<meta name="viewport" content="width=device-width, initial-scale=1.0">\n'
        f"<style>\n{css}\n</style>\n"
        "</head>\n"
        "<body>\n"
        f'  <div class="kicker">{html.escape(kicker_spaced)}</div>\n'
        f'  <h1 class="title">{html.escape(title)}</h1>\n'
        f'  <div class="date-line">{html.escape(date_line)}</div>\n'
        '  <hr class="divider-top">\n'
        f"  {war_html}"
        f"  {chapters_html}"
        f"  {tomorrow_html}"
        f"  {characters_html}"
        f"  {final_quote_html}"
        '  <hr class="divider-bottom">\n'
        '  <div class="footer">\n'
        "    秦托邦 · 小满吐槽日报<br>\n"
        "    所有引用可回溯至当天 quote-map\n"
        "  </div>\n"
        "</body>\n"
        "</html>"
    )
