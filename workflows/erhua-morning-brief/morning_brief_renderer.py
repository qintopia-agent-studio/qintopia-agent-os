#!/usr/bin/env python3
"""Card-style poster rendering for the Erhua morning brief.

Self-contained: builds a styled HTML card (greeting / weather / activities / AI
news / highlight) and screenshots it with Playwright, falling back to a Pillow
drawing when Playwright is unavailable. No dependency on the Xiaoman daily
case-report renderer so the morning brief stays independently reviewable.
"""
from __future__ import annotations

import html
import logging
import os
from dataclasses import dataclass, field
from pathlib import Path
from typing import Any, Optional

from weather import WeatherInfo

logger = logging.getLogger(__name__)

DEFAULT_WIDTH = 720
DEFAULT_IMAGE_FORMAT = "png"
# Final-pixel height ceiling, aligned with the Feishu artifact storage
# validation (runtime/sidecar rejects images taller than 8192px). A card
# taller than this would render fine but be refused at upload time, so the
# renderer refuses to emit it: render() then leaves no file and the worker
# degrades to the text brief instead of shipping a rejected upload.
MAX_HEIGHT = 8192


class CardTooTallError(RuntimeError):
    """Raised when the measured card height exceeds MAX_HEIGHT pixels."""

_INK = "#151b22"
_MUTED = "#76828f"
_RULE = "#d6dde2"
_ACCENT = "#2f6fed"
_PAPER = "#f7f4ee"
_SOFT = "#ebe5d8"
FOOTER_TEXT = "资料来源：公开新闻源；内容仅供参考，欢迎在群里补充判断。"


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


def _news_item_html(index: int, item: str) -> str:
    lines = [line.strip() for line in item.splitlines() if line.strip()]
    if not lines:
        return ""
    title = lines[0]
    detail_parts = []
    for line in lines[1:]:
        cls = "source" if line.startswith("来源：") else "detail"
        detail_parts.append(f'<div class="{cls}">{_esc(line)}</div>')
    details = "".join(detail_parts)
    return f"""<li>
  <div class="news-head"><span class="num">{index:02d}</span><strong>{_esc(title)}</strong></div>
  {details}
</li>"""


def _render_html(card: MorningBriefCard, width: int) -> str:
    weather_text = _weather_line(card.weather)
    footer_text = f"{FOOTER_TEXT} 核验时间：{_esc(card.date_label)} 08:10 上海时间。"
    activity_html = "".join(
        f"<p>{_esc(line)}</p>" for line in (card.activity_body or "今天暂时没有安排好的活动。").splitlines() if line.strip()
    )
    news_html_parts = []
    for idx, item in enumerate(card.ai_news_items, start=1):
        rendered = _news_item_html(idx, item)
        if rendered:
            news_html_parts.append(rendered)
    news_html = "".join(news_html_parts) or '<li class="empty">今天暂时没有读到 AI 新闻。</li>'
    highlight_html = (
        f'<section class="section highlight"><h2>一句话总结</h2>'
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
  body {{ background: {_SOFT}; font-family: -apple-system, BlinkMacSystemFont, "PingFang SC", "Hiragino Sans GB", "Microsoft YaHei", "Noto Sans CJK SC", sans-serif; color: {_INK}; }}
  .card {{ width: {width}px; margin: 20px auto; background: {_PAPER}; padding: 34px 34px 28px; }}
  .topbar {{ display: flex; align-items: flex-start; justify-content: space-between; gap: 18px; }}
  .brand {{ display: flex; flex-direction: column; gap: 6px; }}
  .brand-kicker {{ font-size: 12px; color: {_ACCENT}; font-weight: 900; letter-spacing: 0; }}
  .brand-title {{ font-size: 24px; font-weight: 900; line-height: 1.1; }}
  .date {{ margin-top: 4px; font-size: 15px; color: {_INK}; font-weight: 800; }}
  .tag {{ color: {_MUTED}; font-size: 12px; font-weight: 800; padding-top: 5px; white-space: nowrap; }}
  .headline {{ margin-top: 28px; font-size: 30px; font-weight: 900; line-height: 1.26; }}
  .weather {{ margin-top: 26px; padding: 14px 0 0; border-top: 2px solid {_RULE}; }}
  .weather .cond {{ font-size: 17px; line-height: 1.55; font-weight: 800; color: {_INK}; }}
  .section {{ margin-top: 24px; padding-top: 22px; border-top: 2px solid {_RULE}; }}
  .section h2 {{ font-size: 22px; font-weight: 900; line-height: 1.25; margin-bottom: 12px; }}
  .section p {{ font-size: 16px; line-height: 1.7; color: {_INK}; font-weight: 650; }}
  .section.news ul {{ list-style: none; }}
  .section.news li {{ padding: 18px 0 20px; border-bottom: 2px solid {_RULE}; }}
  .section.news li:last-child {{ border-bottom: 0; padding-bottom: 0; }}
  .section.news li.empty {{ color: {_MUTED}; font-weight: 700; }}
  .news-head {{ display: grid; grid-template-columns: 48px 1fr; gap: 14px; align-items: start; }}
  .section.news .num {{ color: {_ACCENT}; font-size: 22px; font-weight: 900; line-height: 1.22; }}
  .section.news strong {{ font-size: 22px; font-weight: 900; line-height: 1.3; }}
  .section.news .detail {{ margin: 10px 0 0 62px; font-size: 16px; line-height: 1.65; font-weight: 650; color: {_INK}; }}
  .section.news .source {{ margin: 8px 0 0 62px; font-size: 13px; line-height: 1.45; color: {_MUTED}; word-break: break-all; }}
  .highlight p {{ font-size: 18px; font-weight: 850; line-height: 1.6; }}
  .footer {{ margin-top: 26px; padding-top: 16px; border-top: 2px solid {_RULE}; color: {_MUTED}; font-size: 13px; line-height: 1.5; }}
</style>
</head>
<body>
<main class="card">
  <header class="topbar">
    <div class="brand">
      <div>
        <div class="brand-kicker">ERHUA DAILY</div>
        <div class="brand-title">二花早报</div>
        <div class="date">{_esc(card.date_label)}</div>
      </div>
    </div>
    <div class="tag">每日社区线索</div>
  </header>
  <h1 class="headline">{_esc(card.greeting)}</h1>
  <div class="weather">
    <span class="cond">{_esc(weather_text)}</span>
  </div>
  <section class="section activity">
    <h2>{_esc(card.activity_title)}</h2>
    {activity_html}
  </section>
  <section class="section news">
    <h2>{_esc(card.ai_news_title)}</h2>
    <ul>{news_html}</ul>
  </section>
  {highlight_html}
  <div class="footer">{footer_text}</div>
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
        # device_scale_factor=2 doubles every CSS pixel into final pixels; the
        # storage-side cap applies to the rendered image, not the CSS layout.
        if height * 2 > MAX_HEIGHT:
            raise CardTooTallError(
                f"card height {height * 2}px exceeds {MAX_HEIGHT}px storage cap; "
                "refusing to render an image that upload validation would reject"
            )
        page.set_viewport_size({"width": width, "height": height})
        page.screenshot(**screenshot_options)
        browser.close()


def _font_candidates(*, bold: bool = False) -> list[str]:
    """Ordered CJK-capable font paths to try, honoring the deploy override.

    Every candidate must carry CJK glyphs: the caller fails closed when none of
    them load, so a Latin-only font (e.g. DejaVu Sans) must never appear here or
    the fallback would silently ship tofu blocks on minimal hosts. Existence is
    checked by the caller; separating resolution from loading lets tests stub the
    list without touching the real filesystem.
    """
    candidates = [
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Bold.ttc" if bold else "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
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
    pad = 34 * scale
    inner_w = W - pad * 2

    title = _pil_font(30 * scale, bold=True)
    brand = _pil_font(24 * scale, bold=True)
    sub = _pil_font(15 * scale, bold=True)
    kicker = _pil_font(22 * scale, bold=True)
    body = _pil_font(16 * scale)
    body_sm = _pil_font(15 * scale)
    num_font = _pil_font(22 * scale, bold=True)
    foot = _pil_font(13 * scale)

    title_lh = int(title.size * 1.32)
    body_lh = int(body.size * 1.6)
    news_lh = int(body_sm.size * 1.55)
    heading_lh = int(kicker.size * 1.35)

    # Measure pass: wrap every section up front so the canvas can grow to the
    # real content height instead of a fixed cap. A long activity plus several
    # bilingual news items can exceed the old 4000px floor and were previously
    # silently cropped by `min(y, img.height)`.
    measure = ImageDraw.Draw(Image.new("RGB", (1, 1)))
    headline_lines = _wrap(measure, card.greeting, title, inner_w)
    weather_lines = _wrap(measure, _weather_line(card.weather), body, inner_w)
    activity_src = [ln for ln in (card.activity_body or "今天暂时没有安排好的活动。").splitlines() if ln.strip()]
    activity_lines = [ln for src in activity_src for ln in _wrap(measure, src, body, inner_w)]
    news_src = card.ai_news_items or ["今天暂时没有读到 AI 新闻。"]
    num_gutter = 62 * scale
    news_blocks: list[list[str]] = []
    for idx, item in enumerate(news_src, start=1):
        block: list[str] = [f"{idx:02d}"]
        for src in item.splitlines():
            if not src.strip():
                continue
            block.extend(_wrap(measure, src, body_sm, inner_w - num_gutter))
        news_blocks.append(block)
    highlight_lines = _wrap(measure, card.highlight, body, inner_w) if card.highlight else []
    footer_text = f"{FOOTER_TEXT} 核验时间：{card.date_label} 08:10 上海时间。"
    footer_lines = _wrap(measure, footer_text, foot, inner_w)

    news_height = sum(max(len(block) - 1, 1) * news_lh + 20 * scale for block in news_blocks)
    H = (
        34 * scale
        + 48 * scale
        + 22 * scale
        + len(headline_lines) * title_lh
        + 26 * scale
        + len(weather_lines) * body_lh
        + 24 * scale
        + heading_lh
        + len(activity_lines) * body_lh
        + 24 * scale
        + heading_lh
        + news_height
        + (22 * scale + heading_lh + len(highlight_lines) * body_lh if highlight_lines else 0)
        + 18 * scale
        + len(footer_lines) * int(foot.size * 1.5)
        + 28 * scale
    )
    H = max(int(H), 1)
    if H > MAX_HEIGHT:
        raise CardTooTallError(
            f"card height {H}px exceeds {MAX_HEIGHT}px storage cap; "
            "refusing to render an image that upload validation would reject"
        )

    canvas_h = min(MAX_HEIGHT, H + 1024 * scale)
    img = Image.new("RGB", (W, canvas_h), _PAPER)
    d = ImageDraw.Draw(img)

    def rule(y_pos: int) -> None:
        d.line((pad, y_pos, W - pad, y_pos), fill=_RULE, width=2 * scale)

    y = 34 * scale
    d.text((pad, y), "ERHUA DAILY", font=foot, fill=_ACCENT)
    d.text((pad, y + 17 * scale), "二花早报", font=brand, fill=_INK)
    d.text((pad, y + 48 * scale), card.date_label, font=sub, fill=_INK)
    right = "每日社区线索"
    right_w = d.textlength(right, font=foot)
    d.text((W - pad - right_w, y + 6 * scale), right, font=foot, fill=_MUTED)
    y += 48 * scale + 22 * scale

    y = _draw_lines(d, (pad, y), headline_lines, title, _INK, title_lh)
    y += 26 * scale

    rule(y)
    y += 14 * scale
    y = _draw_lines(d, (pad, y), weather_lines, body, _INK, body_lh)
    y += 24 * scale

    rule(y)
    y += 18 * scale
    d.text((pad, y), card.activity_title, font=kicker, fill=_INK)
    y += heading_lh
    y = _draw_lines(d, (pad, y), activity_lines, body, _INK, body_lh)
    y += 24 * scale

    rule(y)
    y += 18 * scale
    d.text((pad, y), card.ai_news_title, font=kicker, fill=_INK)
    y += heading_lh
    for block in news_blocks:
        if not block:
            continue
        d.text((pad, y), block[0], font=num_font, fill=_ACCENT)
        line_y = y
        for line_index, line in enumerate(block[1:] or [""]):
            fill = _MUTED if line.startswith("来源：") else _INK
            font = body_sm if line_index > 0 else _pil_font(18 * scale, bold=True)
            line_h = news_lh if line_index > 0 else int(font.size * 1.35)
            d.text((pad + num_gutter, line_y), line, font=font, fill=fill)
            line_y += line_h
        y = max(line_y, y + news_lh) + 20 * scale
        rule(y - 8 * scale)

    if highlight_lines:
        y += 14 * scale
        d.text((pad, y), "一句话总结", font=kicker, fill=_INK)
        y += heading_lh
        y = _draw_lines(d, (pad, y), highlight_lines, body, _INK, body_lh)

    y += 18 * scale
    rule(y)
    y += 14 * scale
    y = _draw_lines(d, (pad, y), footer_lines, foot, _MUTED, int(foot.size * 1.5))
    y += 28 * scale

    if y > MAX_HEIGHT:
        raise CardTooTallError(
            f"card height {y}px exceeds {MAX_HEIGHT}px storage cap; "
            "refusing to render an image that upload validation would reject"
        )
    if y > canvas_h:
        raise CardTooTallError(
            "card height measurement drift exceeded the reserved canvas; "
            "refusing to render a partial image"
        )

    # Crop to the real drawn height instead of the reserved canvas so the image
    # keeps a tight editorial shape while avoiding black out-of-bounds padding
    # when font metrics differ slightly from the measure pass.
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
    # A too-tall card must never leave a file behind: the worker treats an
    # existing image as "send the card", and storage would reject the upload.
    if output_path.exists():
        output_path.unlink()
    try:
        import tempfile

        with tempfile.TemporaryDirectory(prefix="erhua-morning-brief-") as tmp:
            html_path = Path(tmp) / "card.html"
            html_path.write_text(_render_html(card, width), encoding="utf-8")
            _render_with_playwright(html_path, output_path, width, image_format)
            return
    except CardTooTallError:
        # Oversized cards must not fall through to Pillow: the height is a
        # property of the content, so the fallback would hit the same cap.
        logger.warning(
            "Card height exceeds the %dpx storage cap; skipping card image so the "
            "worker degrades to the text brief",
            MAX_HEIGHT,
        )
        return
    except Exception as exc:
        logger.warning("Playwright render failed; falling back to Pillow: %s", exc)
    try:
        _render_with_pillow(card, output_path, width, image_format)
    except Exception as exc:
        # Fail closed: no garbled or half-drawn image rather than a crash that
        # would abort the whole morning brief.
        logger.warning("Pillow fallback also failed; skipping card image: %s", exc)
        return
