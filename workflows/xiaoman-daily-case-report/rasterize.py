#!/usr/bin/env python3
"""Rasterize a daily case-report HTML file to JPEG/PNG via Playwright.

This is a bounded subprocess entrypoint: it reads a JSON request from stdin,
renders the HTML with Chromium, and writes a JSON response to stdout.
All errors are caught and reported as JSON with ``success: false`` so the
Rust caller can fail closed instead of inheriting a Python traceback.
"""
from __future__ import annotations

import json
import sys
from pathlib import Path
from typing import Any

DEFAULT_JPEG_QUALITY = 92

# Reviewed output bounds enforced by the sidecar media upload
# (runtime/sidecar/src/operations.rs `daily_case_report_image_identity`):
# the rasterized JPEG must stay within these maxima or upload fails closed.
# The base density matches the pre-cutover Python renderer's 2x output.
MAX_OUTPUT_WIDTH = 4096
MAX_OUTPUT_HEIGHT = 8192
BASE_DEVICE_SCALE_FACTOR = 2

# Cross-platform font files that indicate CJK text is likely to render
# correctly in Chromium. If none are present, we fail closed rather than
# producing a screenshot full of tofu.
CJK_FONT_PATHS = [
    # Linux
    "/usr/share/fonts/truetype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/noto-cjk/NotoSansCJK-Regular.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-zenhei.ttc",
    "/usr/share/fonts/truetype/wqy/wqy-microhei.ttc",
    # macOS
    "/System/Library/Fonts/PingFang.ttc",
    "/System/Library/Fonts/STHeiti Light.ttc",
    "/Library/Fonts/Arial Unicode.ttf",
    "/System/Library/Fonts/Hiragino Sans GB.ttc",
]


def _fail(message: str) -> None:
    json.dump({"success": False, "error": message}, sys.stdout, ensure_ascii=False)
    sys.stdout.write("\n")


def _has_cjk_font() -> bool:
    return any(Path(path).exists() for path in CJK_FONT_PATHS)


def _output_scale(width: int, page_height: int) -> float:
    """Pick the device scale factor for the screenshot.

    Prefer the base 2x density but clamp so the output stays within the
    reviewed media-upload bounds (width <= MAX_OUTPUT_WIDTH and
    height <= MAX_OUTPUT_HEIGHT). Flowing roast HTML is taller than the old
    fixed canvas renderer, so an unbounded 2x would exceed the height limit
    and fail the upload for long reports.
    """
    if width <= 0 or page_height <= 0:
        return BASE_DEVICE_SCALE_FACTOR
    scale = BASE_DEVICE_SCALE_FACTOR
    if width * scale > MAX_OUTPUT_WIDTH:
        scale = MAX_OUTPUT_WIDTH / width
    if page_height * scale > MAX_OUTPUT_HEIGHT:
        scale = min(scale, MAX_OUTPUT_HEIGHT / page_height)
    return scale


def _route_handler(route: Any) -> None:
    """Block external network requests; allow file:// data."""
    if route.request.url.startswith(("http://", "https://")):
        route.abort()
    else:
        route.continue_()


def _image_dimensions(path: Path, fallback_width: int, fallback_height: int) -> tuple[int, int]:
    try:
        from PIL import Image as PILImage  # noqa: N814

        with PILImage.open(path) as image:
            return image.size
    except Exception:
        return fallback_width, fallback_height


def main() -> int:
    try:
        request = json.load(sys.stdin)
    except json.JSONDecodeError as exc:
        _fail(f"invalid JSON request: {exc}")
        return 1

    html_path = Path(request.get("html_path", ""))
    output_path = Path(request.get("output_path", ""))
    width = int(request.get("width", 0))
    image_format = request.get("image_format", "jpeg")
    quality = int(request.get("quality", DEFAULT_JPEG_QUALITY))

    if not html_path.is_file():
        _fail(f"HTML file not found: {html_path}")
        return 1
    if width <= 0:
        _fail("width must be positive")
        return 1
    if image_format not in {"jpeg", "png"}:
        _fail(f"unsupported image format: {image_format!r}")
        return 1

    if not _has_cjk_font():
        _fail("no CJK fonts available; aborting rasterization to avoid tofu output")
        return 1

    try:
        from playwright.sync_api import sync_playwright
    except ImportError as exc:
        _fail(f"Playwright is not available: {exc}")
        return 1

    try:
        screenshot_options: dict[str, Any] = {
            "path": str(output_path),
            "full_page": True,
            "type": image_format,
        }
        if image_format == "jpeg":
            screenshot_options["quality"] = quality

        with sync_playwright() as playwright:
            browser = playwright.chromium.launch()
            # Measure the laid-out page height first (CSS pixels, independent
            # of device scale) so the screenshot scale can be clamped to the
            # reviewed output bounds instead of failing media upload later.
            measure = browser.new_page(
                viewport={"width": width, "height": 100},
                device_scale_factor=1,
            )
            measure.route("**/*", _route_handler)
            measure.goto(html_path.as_uri(), wait_until="load")
            page_height = measure.evaluate("document.body.scrollHeight")
            measure.close()

            scale = _output_scale(width, page_height)
            page = browser.new_page(
                viewport={"width": width, "height": page_height},
                device_scale_factor=scale,
            )
            page.route("**/*", _route_handler)
            page.goto(html_path.as_uri(), wait_until="load")
            page.screenshot(**screenshot_options)
            browser.close()

        if not output_path.is_file():
            _fail("screenshot did not create output file")
            return 1

        img_width, img_height = _image_dimensions(output_path, width, page_height)
        mime_type = "image/jpeg" if image_format == "jpeg" else "image/png"
        byte_size = output_path.stat().st_size

        json.dump(
            {
                "success": True,
                "image_path": str(output_path),
                "mime_type": mime_type,
                "byte_size": byte_size,
                "width": img_width,
                "height": img_height,
                "image_format": image_format,
            },
            sys.stdout,
            ensure_ascii=False,
        )
        sys.stdout.write("\n")
        return 0
    except Exception as exc:  # noqa: BLE001 - catch-all subprocess boundary
        _fail(f"{type(exc).__name__}: {exc}")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
