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


# ---------------------------------------------------------------------------
# Pillow (no-browser) roast rendering + deterministic fallback
# ---------------------------------------------------------------------------
#
# The roast long-image was previously rendered only as HTML -> Playwright JPEG,
# which needs a browser (chromium). When the browser is missing the renderer
# silently fell back to a Pillow routine that drew a completely different
# character-poster layout. This module now renders the SAME roast layout
# directly with Pillow, so no browser is required and the output is always the
# roast daily report.


def build_fallback_parsed(report: Any) -> dict[str, Any]:
    """Build the roast parsed-section structure from the deterministic report.

    Used when every LLM narrative model fails: we still emit the roast daily
    report layout, but the chapter text is assembled from the deterministic
    report data (cases, characters, war report) instead of LLM prose. The
    layout, colors and structure are identical to the AI roast version — only
    the wording is plainer. It never degrades to the character poster.
    """
    group = getattr(report, "group_name", "") or "秦托邦"
    date = getattr(report, "report_date", "") or ""
    war = (
        f"{getattr(report, 'message_count', 0)}条消息 · "
        f"{getattr(report, 'participant_count', 0)}人开口"
    )

    chapters: list[dict[str, Any]] = []
    for case in (getattr(report, "cases", None) or [])[:5]:
        paragraphs = [getattr(case, "summary", "") or ""]
        paragraphs.extend([b for b in (getattr(case, "bullets", None) or []) if b])
        paragraphs = [p for p in paragraphs if p]
        if not paragraphs:
            continue
        chapters.append({
            "title": getattr(case, "title", "") or "当日话题",
            "paragraphs": paragraphs,
            "golden_quote": "",
        })

    characters: list[dict[str, str]] = []
    for c in (getattr(report, "characters", None) or [])[:6]:
        desc = getattr(c, "one_liner", "") or getattr(c, "story_function", "") or ""
        if getattr(c, "role_label", ""):
            desc = f"{getattr(c, 'role_label')}｜{desc}" if desc else getattr(c, "role_label")
        characters.append({"name": getattr(c, "name", ""), "desc": desc})

    final_quote = {"text": "", "author": ""}
    if getattr(report, "highlight", None):
        final_quote = {"text": str(report.highlight), "author": ""}

    return {
        "kicker": f"{group}吐槽日报",
        "date_line": date,
        "title": "今日群聊观察",
        "war_report": war,
        "chapters": chapters,
        "tomorrow": "",
        "characters": characters,
        "final_quote": final_quote,
    }


def render_pillow(input_data: dict[str, Any], output_path: str) -> str:
    """Render the roast long-image directly with Pillow (no browser).

    Accepts either `narrative_md` (AI roast text) or a pre-parsed structure via
    `parsed`. Produces the same roast layout as the HTML/Playwright version.
    Returns the output path.
    """
    from PIL import Image, ImageDraw

    parsed = input_data.get("parsed")
    if parsed is None:
        md = input_data.get("narrative_md", "")
        if not md:
            raise RuntimeError("roast render_pillow requires narrative_md or parsed")
        parsed = _parse_narrative(md)

    width = int(input_data.get("width", 1080))
    scale = 2
    canvas_w = width * scale
    padding = 110 * scale
    content_w = canvas_w - padding * 2

    # Palette mirrors the roast HTML CSS.
    bg = "#fbfaf6"
    ink = "#1a1a1a"
    body_ink = "#2a2a2a"
    accent = "#8b1f2f"
    muted = "#999999"
    card_bg = "#f5f0e8"
    char_bg = "#ffffff"
    char_border = "#e8e0d0"
    quote_bg = "#8b1f2f"

    # Fonts (CJK-capable, reuse the shared helper for cross-platform lookup).
    from renderer import _pil_font  # sibling import; workflow dir is on sys.path

    kicker_f = _pil_font(17 * scale)
    title_f = _pil_font(38 * scale, bold=True)
    date_f = _pil_font(17 * scale)
    h2_f = _pil_font(24 * scale, bold=True)
    body_f = _pil_font(18 * scale)
    war_f = _pil_font(22 * scale, bold=True)
    quote_f = _pil_font(24 * scale, bold=True)
    char_name_f = _pil_font(20 * scale, bold=True)
    small_f = _pil_font(14 * scale)
    footer_f = _pil_font(14 * scale)

    line_gap = 10 * scale
    para_gap = 22 * scale
    section_gap = 36 * scale

    def wrap(draw: ImageDraw.ImageDraw, text: str, font: Any, max_w: int) -> list[str]:
        """Greedy CJK-aware wrap: break on width, treating each char as a unit."""
        lines: list[str] = []
        for raw in text.split("\n"):
            raw = raw.strip()
            if not raw:
                continue
            cur = ""
            for ch in raw:
                trial = cur + ch
                if draw.textlength(trial, font=font) <= max_w:
                    cur = trial
                else:
                    if cur:
                        lines.append(cur)
                    cur = ch
            if cur:
                lines.append(cur)
        return lines or [""]

    # --- measure pass: compute total height on a throwaway canvas ---
    def layout(draw: ImageDraw.ImageDraw, render: bool, image: Any = None) -> int:
        y = 70 * scale

        def text(x: int, yy: int, s: str, font: Any, fill: str) -> None:
            if render:
                draw.text((x, yy), s, font=font, fill=fill)

        def center(yy: int, s: str, font: Any, fill: str) -> None:
            if render:
                w = draw.textlength(s, font=font)
                draw.text(((canvas_w - w) / 2, yy), s, font=font, fill=fill)

        # kicker / title / date
        kicker = " ".join(parsed.get("kicker") or "秦托邦吐槽日报")
        center(y, kicker, kicker_f, accent)
        y += 40 * scale
        for ln in wrap(draw, parsed.get("title") or "今日群聊观察", title_f, content_w):
            center(y, ln, title_f, ink)
            y += 52 * scale
        if parsed.get("date_line"):
            center(y, parsed["date_line"], date_f, muted)
            y += 34 * scale
        # divider
        if render:
            draw.rectangle([padding, y, canvas_w - padding, y + 4 * scale], fill=accent)
        y += section_gap

        # war report card
        if parsed.get("war_report"):
            war_lines = wrap(draw, parsed["war_report"], war_f, content_w - 60 * scale)
            card_h = 24 * scale + len(war_lines) * 40 * scale + 24 * scale
            if render:
                draw.rectangle([padding, y, canvas_w - padding, y + card_h], fill=card_bg)
                draw.rectangle([padding, y, padding + 5 * scale, y + card_h], fill=accent)
            yy = y + 24 * scale
            for ln in war_lines:
                center(yy, ln, war_f, accent)
                yy += 40 * scale
            y += card_h + section_gap

        # chapters
        for ch in parsed.get("chapters", []):
            for ln in wrap(draw, ch.get("title", ""), h2_f, content_w - 20 * scale):
                if render:
                    draw.rectangle([padding, y + 4 * scale, padding + 5 * scale, y + 30 * scale], fill=accent)
                text(padding + 20 * scale, y, ln, h2_f, accent)
                y += 40 * scale
            for para in ch.get("paragraphs", []):
                for ln in wrap(draw, para, body_f, content_w):
                    text(padding, y, ln, body_f, body_ink)
                    y += 30 * scale + line_gap // 2
                y += para_gap // 2
            if ch.get("golden_quote"):
                q = f"金句：{ch['golden_quote']}"
                for ln in wrap(draw, q, body_f, content_w - 20 * scale):
                    text(padding + 20 * scale, y, ln, body_f, accent)
                    y += 30 * scale
            y += section_gap

        # tomorrow
        if parsed.get("tomorrow"):
            for ln in wrap(draw, "明日线索", h2_f, content_w - 20 * scale):
                text(padding, y, ln, h2_f, accent)
                y += 40 * scale
            for ln in wrap(draw, parsed["tomorrow"], body_f, content_w):
                text(padding, y, ln, body_f, body_ink)
                y += 30 * scale + line_gap // 2
            y += section_gap

        # characters
        chars = parsed.get("characters") or []
        if chars:
            for ln in wrap(draw, "今日人物速写", h2_f, content_w):
                text(padding, y, ln, h2_f, accent)
                y += 44 * scale
            cols = 2
            col_w = (content_w - 16 * scale) // cols
            row_h = 0
            x_col = 0
            for c in chars:
                name = c.get("name", "")
                desc = c.get("desc", "")
                name_lines = wrap(draw, name, char_name_f, col_w - 32 * scale)
                desc_lines = wrap(draw, desc, small_f, col_w - 32 * scale)
                card_h = 20 * scale + len(name_lines) * 30 * scale + len(desc_lines) * 22 * scale + 20 * scale
                row_h = max(row_h, card_h)
                if render:
                    cx = padding + x_col * (col_w + 16 * scale)
                    draw.rectangle([cx, y, cx + col_w, y + card_h], fill=char_bg, outline=char_border, width=scale)
                    draw.rectangle([cx, y, cx + 5 * scale, y + card_h], fill=accent)
                    yy = y + 16 * scale
                    for ln in name_lines:
                        text(cx + 18 * scale, yy, ln, char_name_f, accent)
                        yy += 30 * scale
                    for ln in desc_lines:
                        text(cx + 18 * scale, yy, ln, small_f, "#444444")
                        yy += 22 * scale
                x_col += 1
                if x_col >= cols:
                    x_col = 0
                    y += row_h + 16 * scale
                    row_h = 0
            if x_col != 0:
                y += row_h + 16 * scale
            y += section_gap

        # final quote band
        fq = parsed.get("final_quote") or {}
        if fq.get("text"):
            quote_text = f"“{fq['text']}”"
            q_lines = wrap(draw, quote_text, quote_f, content_w - 60 * scale)
            band_h = 40 * scale + 40 * scale + len(q_lines) * 44 * scale + 40 * scale
            if render:
                draw.rectangle([padding, y, canvas_w - padding, y + band_h], fill=quote_bg)
            yy = y + 40 * scale
            center(yy, "今 日 金 句", small_f, "#ffd92e")
            yy += 44 * scale
            for ln in q_lines:
                center(yy, ln, quote_f, "#ffffff")
                yy += 44 * scale
            if fq.get("author"):
                center(yy, f"—— {fq['author']}", small_f, "#ffd92e")
                yy += 30 * scale
            y += band_h + section_gap

        # footer divider + footer
        if render:
            draw.rectangle([padding, y, canvas_w - padding, y + scale], fill=accent)
        y += 28 * scale
        center(y, "秦托邦 · 小满吐槽日报", footer_f, muted)
        y += 28 * scale
        center(y, "所有引用可回溯至当天 quote-map", footer_f, muted)
        y += 60 * scale
        return y

    measure_img = Image.new("RGB", (canvas_w, 100), bg)
    measure_draw = ImageDraw.Draw(measure_img)
    total_h = layout(measure_draw, False)

    image = Image.new("RGB", (canvas_w, total_h), bg)
    draw = ImageDraw.Draw(image)
    layout(draw, True, image)

    image.save(output_path, "JPEG", quality=90)
    return output_path
