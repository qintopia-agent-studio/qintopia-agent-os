#!/usr/bin/env python3
"""Card-style poster rendering for the Erhua morning brief.

Self-contained: builds a styled HTML card (greeting / weather / activities / AI
news / highlight) and screenshots it with Playwright, falling back to a Pillow
drawing when Playwright is unavailable. No dependency on the Xiaoman daily
case-report renderer so the morning brief stays independently reviewable.
"""
from __future__ import annotations

import html
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

from weather import WeatherInfo

DEFAULT_WIDTH = 720
DEFAULT_IMAGE_FORMAT = "png"

_INK = "#1a1a1a"
_YELLOW = "#ffd92e"
_ORANGE = "#f25a18"
_BLUE = "#88d7ff"
_CREAM = "#fff8df"
_PALE = "#fff0a6"
_PAPER = "#f4efe2"


@dataclass(frozen=True)
class MorningBriefCard:
    greeting: str
    date_label: str
    weather: Optional[WeatherInfo] = None
    activity_title: str = "今日活动"
    activity_body: str = ""
    ai_news_title: str = "AI 新闻"
    ai_news_items: list[str] = field(default_factory=list)
    highlight: Optional[str] = None


def _esc(value: str) -> str:
    return html.escape(value or "")


def _weather_line(weather: Optional[WeatherInfo]) -> str:
    if weather is None:
        return "今日天气稍后补充"
    return weather.summary


def _render_html(card: MorningBriefCard, width: int) -> str:
    weather_text = _weather_line(card.weather)
    activity_html = "".join(
        f"<p>{_esc(line)}</p>" for line in (card.activity_body or "今天暂时没有安排好的活动。").splitlines() if line.strip()
    )
    news_html = "".join(
        f'<li><span class="num">{idx}</span><div>{_esc(item)}</div></li>'
        for idx, item in enumerate(card.ai_news_items, start=1)
    ) or '<li class="empty">今天暂时没有读到 AI 新闻。</li>'
    highlight_html = (
        f'<section class="highlight"><span class="kicker">今日亮点</span>'
        f'<p>{_esc(card.highlight)}</p></section>'
        if card.highlight
        else ""
    )
    return f"""<!DOCTYPE html>
<html>
<head>
<meta charset="utf-8">
<style>
  * {{ box-sizing: border-box; margin: 0; padding: 0; }}
  body {{ background: {_PAPER}; font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Noto Sans CJK SC", sans-serif; }}
  .card {{ width: {width}px; margin: 20px auto; background: {_CREAM}; border: 8px solid {_INK}; border-radius: 22px; overflow: hidden; }}
  .header {{ background: {_INK}; color: {_YELLOW}; padding: 22px 26px 18px; }}
  .header h1 {{ font-size: 34px; font-weight: 900; letter-spacing: 1px; }}
  .header .date {{ margin-top: 6px; font-size: 14px; color: {_PALE}; font-weight: 600; }}
  .weather {{ display: flex; align-items: center; gap: 14px; margin: 18px 22px 0; padding: 14px 18px; background: {_BLUE}; border: 3px solid {_INK}; border-radius: 16px; }}
  .weather .cond {{ font-size: 22px; font-weight: 900; color: {_INK}; }}
  .weather .temp {{ margin-left: auto; font-size: 16px; font-weight: 700; color: {_INK}; text-align: right; }}
  .section {{ margin: 18px 22px 0; padding: 16px 18px; background: #fff; border: 3px solid {_INK}; border-radius: 16px; }}
  .section .kicker {{ display: inline-block; font-size: 12px; font-weight: 900; letter-spacing: 1px; color: #fff; background: {_ORANGE}; padding: 3px 10px; border-radius: 8px; margin-bottom: 10px; }}
  .section.activity .kicker {{ background: {_ORANGE}; }}
  .section.news .kicker {{ background: #1f6f54; }}
  .section p {{ font-size: 15px; line-height: 1.7; color: #222; }}
  .section.news ul {{ list-style: none; }}
  .section.news li {{ display: flex; gap: 10px; align-items: flex-start; font-size: 14px; line-height: 1.6; color: #222; padding: 8px 0; border-bottom: 1px dashed #d8c7a2; }}
  .section.news li:last-child {{ border-bottom: 0; }}
  .section.news li.empty {{ color: #777; }}
  .section.news .num {{ flex: 0 0 22px; height: 22px; display: grid; place-items: center; background: {_PALE}; border: 2px solid {_INK}; border-radius: 50%; font-size: 12px; font-weight: 900; }}
  .highlight {{ margin: 18px 22px 0; padding: 16px 18px; background: {_ORANGE}; border: 3px solid {_INK}; border-radius: 16px; }}
  .highlight .kicker {{ font-size: 12px; font-weight: 900; letter-spacing: 1px; color: {_YELLOW}; }}
  .highlight p {{ margin-top: 6px; font-size: 16px; font-weight: 700; line-height: 1.6; color: {_CREAM}; }}
  .footer {{ margin-top: 18px; padding: 12px 26px; background: {_INK}; color: {_PALE}; font-size: 11px; text-align: center; }}
</style>
</head>
<body>
<main class="card">
  <header class="header">
    <h1>{_esc(card.greeting)}</h1>
    <div class="date">{_esc(card.date_label)}</div>
  </header>
  <div class="weather">
    <span class="cond">{_esc(weather_text)}</span>
  </div>
  <section class="section activity">
    <span class="kicker">{_esc(card.activity_title)}</span>
    {activity_html}
  </section>
  <section class="section news">
    <span class="kicker">{_esc(card.ai_news_title)}</span>
    <ul>{news_html}</ul>
  </section>
  {highlight_html}
  <div class="footer">二花早报 · 由小满自动整理，仅供社区群内参考</div>
</main>
</body>
</html>"""


def _file_url(path: Path) -> str:
    return path.resolve().as_uri()


def _render_with_playwright(html_path: Path, output_path: Path, width: int, image_format: str) -> None:
    from playwright.sync_api import sync_playwright

    screenshot_options: dict[str, Any] = {
        "path": str(output_path),
        "full_page": True,
        "type": image_format,
    }
    if image_format == "jpeg":
        screenshot_options["quality"] = 88
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


def _font_candidates(*, bold: bool = False) -> list[str]:
    """Ordered CJK-capable font paths to try, honoring the deploy override.

    Existence is checked by the caller; separating resolution from loading lets
    tests stub the list without touching the real filesystem.
    """
    candidates = [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc" if bold else "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    configured = os.environ.get("QINTOPIA_ERHUA_MORNING_BRIEF_FONT")
    if configured:
        candidates.insert(0, configured)
    return candidates


def _pil_font(size: int, *, bold: bool = False) -> Any:
    """Load a CJK-capable TrueType font, or fail closed.

    Pillow's ``ImageFont.load_default()`` is a bitmap font with no ``.size``
    attribute and no CJK glyphs: silently falling back to it either raised
    AttributeError while measuring lines or produced garbled posters. When no
    usable font exists we raise so the caller can drop the image instead of
    shipping mojibake.
    """
    from PIL import ImageFont

    for candidate in _font_candidates(bold=bold):
        if candidate and Path(candidate).exists():
            return ImageFont.truetype(candidate, size=size)
    raise RuntimeError(
        "No CJK-capable font available for the morning-brief Pillow fallback. "
        "Install Noto Sans CJK / PingFang or set QINTOPIA_ERHUA_MORNING_BRIEF_FONT "
        "to a .ttf/.ttc path. Refusing to render garbled text."
    )


def _wrap(draw: Any, text: str, font: Any, max_width: int) -> list[str]:
    lines: list[str] = []
    line = ""
    for ch in text:
        if line and draw.textlength(line + ch, font=font) > max_width:
            lines.append(line)
            line = ch
        else:
            line += ch
    if line:
        lines.append(line)
    return lines or [""]


def _draw_lines(d: Any, xy: tuple[int, int], lines: list[str], font: Any, fill: str, line_h: int) -> int:
    x, y = xy
    for line in lines:
        d.text((x, y), line, font=font, fill=fill)
        y += line_h
    return y


def _render_with_pillow(card: MorningBriefCard, output_path: Path, width: int, image_format: str) -> None:
    from PIL import Image, ImageDraw

    scale = 2
    W = width * scale
    pad = 28 * scale
    cw = W - pad * 2
    inner_w = cw - 32 * scale

    title = _pil_font(34 * scale, bold=True)
    sub = _pil_font(14 * scale)
    kicker = _pil_font(13 * scale, bold=True)
    body = _pil_font(16 * scale)
    body_sm = _pil_font(14 * scale)
    hi = _pil_font(17 * scale, bold=True)

    body_lh = int(body.size * 1.4) + 2 * scale
    news_lh = body_sm.size * 2 + 10 * scale
    hi_lh = int(hi.size * 1.5)

    # Measure pass: wrap every section up front so the canvas can grow to the
    # real content height instead of a fixed cap. A long activity plus several
    # bilingual news items can exceed the old 4000px floor and were previously
    # silently cropped by `min(y, img.height)`.
    measure = ImageDraw.Draw(Image.new("RGB", (1, 1)))
    weather_lines = _wrap(measure, _weather_line(card.weather), body, inner_w)
    activity_src = [ln for ln in (card.activity_body or "今天暂时没有安排好的活动。").splitlines() if ln.strip()]
    activity_lines = [ln for src in activity_src for ln in _wrap(measure, src, body, inner_w)]
    news_src = card.ai_news_items or ["今天暂时没有读到 AI 新闻。"]
    news_lines = [
        ln
        for idx, item in enumerate(news_src, start=1)
        for ln in _wrap(measure, f"{idx}. {item}", body_sm, inner_w)
    ]
    highlight_lines = _wrap(measure, card.highlight, hi, inner_w) if card.highlight else []

    def section_height(top_pad: int, lines: list[str], line_h: int, *, min_h: int = 0) -> int:
        h = top_pad + len(lines) * line_h
        return max(h, min_h) if min_h else h

    wh = section_height(18 * scale, weather_lines, body_lh, min_h=54 * scale)
    ah = section_height(44 * scale, activity_lines, body_lh, min_h=54 * scale)
    nh = section_height(44 * scale, news_lines, news_lh, min_h=54 * scale)
    hh = section_height(38 * scale, highlight_lines, hi_lh, min_h=50 * scale) if highlight_lines else 0

    gap_after = 18 * scale
    H = (
        20 * scale
        + 86 * scale + 18 * scale
        + wh + gap_after
        + ah + gap_after
        + nh + gap_after
        + (hh + gap_after if highlight_lines else 0)
        + 40 * scale + 20 * scale
    )
    H = max(int(H), 1)

    img = Image.new("RGB", (W, H), _PAPER)
    d = ImageDraw.Draw(img)

    def rect(x: int, y: int, w: int, h: int, fill: str, outline: str = _INK, ow: int = 3 * scale, r: int = 16 * scale) -> None:
        d.rounded_rectangle((x, y, x + w, y + h), radius=r, fill=fill, outline=outline, width=ow)

    y = 20 * scale
    rect(0, 0, W, 0, _INK)  # noop guard
    d.rectangle((0, y, W, y + 86 * scale), fill=_INK)
    d.text((pad, y + 18 * scale), card.greeting, font=title, fill=_YELLOW)
    d.text((pad, y + 60 * scale), card.date_label, font=sub, fill=_PALE)
    y += 86 * scale + 18 * scale

    # weather chip
    rect(pad, y, cw, wh, _BLUE)
    _draw_lines(d, (pad + 16 * scale, y + 18 * scale), weather_lines, body, _INK, body_lh)
    y += wh + gap_after

    # activity
    rect(pad, y, cw, ah, "#ffffff")
    d.text((pad + 16 * scale, y + 14 * scale), card.activity_title, font=kicker, fill=_ORANGE)
    _draw_lines(d, (pad + 16 * scale, y + 44 * scale), activity_lines, body, "#222222", body_lh)
    y += ah + gap_after

    # ai news
    rect(pad, y, cw, nh, "#ffffff")
    d.text((pad + 16 * scale, y + 14 * scale), card.ai_news_title, font=kicker, fill="#1f6f54")
    _draw_lines(d, (pad + 16 * scale, y + 44 * scale), news_lines, body_sm, "#222222", news_lh)
    y += nh + gap_after

    # highlight
    if highlight_lines:
        rect(pad, y, cw, hh, _ORANGE)
        d.text((pad + 16 * scale, y + 12 * scale), "今日亮点", font=kicker, fill=_YELLOW)
        _draw_lines(d, (pad + 16 * scale, y + 38 * scale), highlight_lines, hi, _CREAM, hi_lh)
        y += hh + gap_after

    d.rectangle((0, y, W, y + 40 * scale), fill=_INK)
    d.text((pad, y + 12 * scale), "二花早报 · 由小满自动整理，仅供社区群内参考", font=_pil_font(11 * scale), fill=_PALE)
    y += 40 * scale + 20 * scale

    # H is derived from the measured wrapped-line heights, so the drawn content
    # fits exactly (final y == img.height). Crop to the real drawn height rather
    # than clamping to the canvas, so a measurement drift can never silently drop
    # content the way the old fixed-4000px path did.
    cropped = img.crop((0, 0, W, y))
    save_kwargs: dict[str, Any] = {}
    if image_format == "jpeg":
        save_kwargs["quality"] = 88
        save_kwargs["optimize"] = True
        fmt = "JPEG"
    else:
        fmt = "PNG"
    cropped.save(output_path, format=fmt, **save_kwargs)


def render(
    card: MorningBriefCard,
    output_path: Path,
    *,
    width: int = DEFAULT_WIDTH,
    image_format: str = DEFAULT_IMAGE_FORMAT,
) -> None:
    """Render the morning brief card to an image file (PNG/JPEG).

    Playwright (HTML screenshot) is preferred; a self-contained Pillow drawing
    is the fallback. The card image is a derived artifact and the morning-brief
    text stays the canonical deliverable, so any rendering failure is non-fatal:
    this returns leaving no file rather than raising and taking down the brief.
    """
    output_path = Path(output_path)
    output_path.parent.mkdir(parents=True, exist_ok=True)
    try:
        import tempfile

        with tempfile.TemporaryDirectory(prefix="erhua-morning-brief-") as tmp:
            html_path = Path(tmp) / "card.html"
            html_path.write_text(_render_html(card, width), encoding="utf-8")
            _render_with_playwright(html_path, output_path, width, image_format)
            return
    except Exception:
        pass
    try:
        _render_with_pillow(card, output_path, width, image_format)
    except Exception:
        # Fail closed: no garbled or half-drawn image rather than a crash that
        # would abort the whole morning brief.
        return
