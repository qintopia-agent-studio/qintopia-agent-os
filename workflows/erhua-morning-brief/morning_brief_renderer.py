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


def _pil_font(size: int, *, bold: bool = False) -> Any:
    from PIL import ImageFont

    candidates = [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc" if bold else "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    configured = os.environ.get("QINTOPIA_ERHUA_MORNING_BRIEF_FONT")
    if configured:
        candidates.insert(0, configured)
    for candidate in candidates:
        if candidate and Path(candidate).exists():
            return ImageFont.truetype(candidate, size=size)
    return ImageFont.load_default()


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


def _draw_wrapped(draw: Any, xy: tuple[int, int], text: str, font: Any, fill: str, max_width: int, *, gap: int = 6, max_lines: Optional[int] = None) -> int:
    x, y = xy
    lines = _wrap(draw, text, font, max_width)
    if max_lines is not None and len(lines) > max_lines:
        lines = lines[:max_lines]
        lines[-1] = lines[-1].rstrip("，。；、 ") + "..."
    lh = int(font.size * 1.4)
    for line in lines:
        draw.text((x, y), line, font=font, fill=fill)
        y += lh + gap
    return y


def _render_with_pillow(card: MorningBriefCard, output_path: Path, width: int, image_format: str) -> None:
    from PIL import Image, ImageDraw

    scale = 2
    W = width * scale
    pad = 28 * scale
    cw = W - pad * 2
    img = Image.new("RGB", (W, 4000), _PAPER)
    d = ImageDraw.Draw(img)

    title = _pil_font(34 * scale, bold=True)
    sub = _pil_font(14 * scale)
    kicker = _pil_font(13 * scale, bold=True)
    body = _pil_font(16 * scale)
    body_sm = _pil_font(14 * scale)
    hi = _pil_font(17 * scale, bold=True)

    def rect(x: int, y: int, w: int, h: int, fill: str, outline: str = _INK, ow: int = 3 * scale, r: int = 16 * scale) -> None:
        d.rounded_rectangle((x, y, x + w, y + h), radius=r, fill=fill, outline=outline, width=ow)

    y = 20 * scale
    rect(0, 0, W, 0, _INK)  # noop guard
    d.rectangle((0, y, W, y + 86 * scale), fill=_INK)
    d.text((pad, y + 18 * scale), card.greeting, font=title, fill=_YELLOW)
    d.text((pad, y + 60 * scale), card.date_label, font=sub, fill=_PALE)
    y += 86 * scale + 18 * scale

    # weather chip
    wlines = _wrap(d, _weather_line(card.weather), body, cw - 32 * scale)
    wh = max(54 * scale, 18 * scale + len(wlines) * int(body.size * 1.4))
    rect(pad, y, cw, wh, _BLUE)
    _draw_wrapped(d, (pad + 16 * scale, y + 14 * scale), _weather_line(card.weather), body, _INK, cw - 32 * scale, gap=2 * scale)
    y += wh + 18 * scale

    # activity
    a_lines = [ln for ln in (card.activity_body or "今天暂时没有安排好的活动。").splitlines() if ln.strip()]
    ah = 54 * scale + len(a_lines) * int(body.size * 1.7)
    rect(pad, y, cw, ah, "#ffffff")
    d.text((pad + 16 * scale, y + 14 * scale), card.activity_title, font=kicker, fill=_ORANGE)
    ay = y + 44 * scale
    for ln in a_lines:
        ay = _draw_wrapped(d, (pad + 16 * scale, ay), ln, body, "#222222", cw - 32 * scale, gap=2 * scale)
    y += ah + 18 * scale

    # ai news
    n_lines = card.ai_news_items or ["今天暂时没有读到 AI 新闻。"]
    nh = 54 * scale + len(n_lines) * (body_sm.size * 2 + 10 * scale)
    rect(pad, y, cw, nh, "#ffffff")
    d.text((pad + 16 * scale, y + 14 * scale), card.ai_news_title, font=kicker, fill="#1f6f54")
    ny = y + 44 * scale
    for idx, item in enumerate(n_lines, start=1):
        prefix = f"{idx}. "
        ny = _draw_wrapped(d, (pad + 16 * scale, ny), prefix + item, body_sm, "#222222", cw - 32 * scale, gap=2 * scale)
        ny += 6 * scale
    y += nh + 18 * scale

    # highlight
    if card.highlight:
        hlines = _wrap(d, card.highlight, hi, cw - 32 * scale)
        hh = 50 * scale + len(hlines) * int(hi.size * 1.5)
        rect(pad, y, cw, hh, _ORANGE)
        d.text((pad + 16 * scale, y + 12 * scale), "今日亮点", font=kicker, fill=_YELLOW)
        _draw_wrapped(d, (pad + 16 * scale, y + 38 * scale), card.highlight, hi, _CREAM, cw - 32 * scale, gap=2 * scale)
        y += hh + 18 * scale

    d.rectangle((0, y, W, y + 40 * scale), fill=_INK)
    d.text((pad, y + 12 * scale), "二花早报 · 由小满自动整理，仅供社区群内参考", font=_pil_font(11 * scale), fill=_PALE)
    y += 40 * scale + 20 * scale

    cropped = img.crop((0, 0, W, min(y, img.height)))
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
    """Render the morning brief card to an image file (PNG/JPEG)."""
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
        _render_with_pillow(card, output_path, width, image_format)
