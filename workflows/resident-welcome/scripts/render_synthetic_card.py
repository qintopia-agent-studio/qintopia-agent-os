#!/usr/bin/env python3
"""Bounded, network-free synthetic renderer; not the missing legacy v10 migration."""
import io
import json
import os
import sys

from PIL import Image, ImageDraw, ImageFont


def render(material):
    if not isinstance(material, dict) or set(material) - {"synthetic", "display_name", "description"}:
        raise ValueError("unsupported_card_fields")
    if material.get("synthetic") is not True:
        raise ValueError("synthetic_material_required")
    name = material.get("display_name", "")
    description = material.get("description", "")
    if not isinstance(name, str) or not 1 <= len(name) <= 40:
        raise ValueError("invalid_name")
    if not isinstance(description, str) or len(description) > 320:
        raise ValueError("invalid_description")
    if any(ord(c) < 32 and c != "\n" for c in name + description):
        raise ValueError("invalid_controls")
    candidates = [
        os.environ.get("QINTOPIA_WELCOME_FONT", ""),
        "/System/Library/Fonts/PingFang.ttc",
        "/System/Library/Fonts/STHeiti Light.ttc",
        "/usr/share/fonts/opentype/noto/NotoSansCJK-Regular.ttc",
        "/usr/share/fonts/truetype/dejavu/DejaVuSans.ttf",
    ]
    font_path = next((p for p in candidates if p and os.path.isfile(p)), None)
    if font_path is None:
        raise ValueError("local_font_required")
    fonts = {size: ImageFont.truetype(font_path, size) for size in [22, 27, 34, 62]}
    im = Image.new("RGB", (1080, 720), "#19201c")
    draw = ImageDraw.Draw(im)
    draw.rounded_rectangle((36, 36, 1044, 684), radius=28, fill="#242c27")
    draw.line((84, 112, 996, 112), fill="#3b4c3f", width=2)
    draw.text((84, 65), "QINTOPIA  /  欢迎来到秦托邦", font=fonts[22], fill="#a0d8b6")
    draw.text((84, 148), "很高兴遇见你", font=fonts[34], fill="#e9efe9")
    # Long names shrink rather than escaping the safe card region.
    size = 62
    name_font = fonts[62]
    while draw.textlength(name, font=name_font) > 910 and size > 22:
        size -= 2
        name_font = ImageFont.truetype(font_path, size)
    draw.text((84, 211), name, font=name_font, fill="#a0d8b6")
    lines, line = [], ""
    for character in description:
        if character == "\n" or draw.textlength(line + character, font=fonts[27]) > 904:
            lines.append(line)
            line = "" if character == "\n" else character
        else:
            line += character
    lines.append(line)
    if len(lines) > 7:
        raise ValueError("description_layout_overflow")
    for index, line in enumerate(lines):
        draw.text((84, 315 + index * 39), line, font=fonts[27], fill="#e9efe9")
    draw.text((84, 622), "此卡仅含虚构资料 · 本地合成验收", font=fonts[22], fill="#a9bbae")
    output = io.BytesIO()
    im.save(output, format="PNG", optimize=True)
    return output.getvalue()


if __name__ == "__main__":
    raw = sys.stdin.buffer.read(16385)
    if len(raw) > 16384:
        raise ValueError("input_too_large")
    sys.stdout.buffer.write(render(json.loads(raw)))
