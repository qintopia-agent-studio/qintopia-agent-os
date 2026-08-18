#!/usr/bin/env python3
"""Unit tests for the roast long-image renderer Markdown stripping.

The LLM roast narrative writes inline emphasis (`**bold**`, `*italic*`,
`` `code` `` ...). The renderer draws paragraph text verbatim, so leftover
markers used to appear as literal asterisks on the daily-report image. These
tests pin the stripping behaviour so the regression cannot return.
"""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import roast_long_image  # noqa: E402


def _walk_strings(node):
    if isinstance(node, str):
        yield node
    elif isinstance(node, list):
        for item in node:
            yield from _walk_strings(item)
    elif isinstance(node, dict):
        for value in node.values():
            yield from _walk_strings(value)


_SAMPLE_MD = """# 菩语司吐槽日报 | 2026-08-17 | 今日群聊观察

**战报**：148条消息 · 23人开口

## 第一章 晨光里的木工房
今天**二花**早早就在群里吆喝，说要带大家做**榫卯**。
**老张**二话不说扛着刨子就来了，*气氛*一下子热络起来。

## 今日人物速写
> **二花**
> 群里的**开心果**，每天准点出现。

## 今日金句
**"榫卯不差分毫，就像做人。"** —— 老张
"""


class StripMdInlineTest(unittest.TestCase):
    def test_strips_bold_italic_code_strike_underscore(self) -> None:
        self.assertEqual(roast_long_image._strip_md_inline("**二花**"), "二花")
        self.assertEqual(roast_long_image._strip_md_inline("__粗体__"), "粗体")
        self.assertEqual(roast_long_image._strip_md_inline("*斜体*"), "斜体")
        self.assertEqual(roast_long_image._strip_md_inline("_斜体_"), "斜体")
        self.assertEqual(roast_long_image._strip_md_inline("~~删除~~"), "删除")
        self.assertEqual(roast_long_image._strip_md_inline("`代码`"), "代码")

    def test_strips_inline_markers_within_sentence(self) -> None:
        out = roast_long_image._strip_md_inline("今天**二花**带着**榫卯**来了")
        self.assertEqual(out, "今天二花带着榫卯来了")

    def test_collapses_nested_markers(self) -> None:
        self.assertEqual(roast_long_image._strip_md_inline("***重点***"), "重点")

    def test_preserves_intraword_underscore_in_names(self) -> None:
        # snake_case identifiers / nicknames are not Markdown emphasis.
        self.assertEqual(roast_long_image._strip_md_inline("er_hua_test"), "er_hua_test")

    def test_preserves_multiplication_asterisks(self) -> None:
        # Asterisks adjacent to digits are not emphasis markers.
        self.assertEqual(roast_long_image._strip_md_inline("3*4*5"), "3*4*5")
        self.assertEqual(roast_long_image._strip_md_inline("a*b*c"), "a*b*c")

    def test_preserves_ascii_adjacent_single_markers(self) -> None:
        # *_..._* flanked by ASCII word chars is treated as literal text.
        self.assertEqual(roast_long_image._strip_md_inline("word_二花_word"), "word_二花_word")

    def test_empty_and_plain_text_unchanged(self) -> None:
        self.assertEqual(roast_long_image._strip_md_inline(""), "")
        self.assertEqual(roast_long_image._strip_md_inline("没有标记的句子"), "没有标记的句子")


class ParseNarrativeStripsMarkersTest(unittest.TestCase):
    def test_no_literal_markers_survive_in_parsed_structure(self) -> None:
        parsed = roast_long_image._parse_narrative(_SAMPLE_MD)
        for text in _walk_strings(parsed):
            self.assertNotIn("**", text, f"bold marker leaked into parsed output: {text!r}")
            self.assertNotIn("`", text, f"code marker leaked into parsed output: {text!r}")

    def test_paragraph_text_keeps_inner_content(self) -> None:
        parsed = roast_long_image._parse_narrative(_SAMPLE_MD)
        paragraphs = parsed["chapters"][0]["paragraphs"]
        joined = "\n".join(paragraphs)
        self.assertIn("今天二花早早就在群里吆喝", joined)
        self.assertIn("榫卯", joined)


class RenderHtmlStripsMarkersTest(unittest.TestCase):
    def test_rendered_html_has_no_literal_bold_markers(self) -> None:
        html = roast_long_image.render({"narrative_md": _SAMPLE_MD, "width": 1080})
        self.assertEqual(html.count("**"), 0)
        self.assertIn("今天二花早早就在群里吆喝", html)
        self.assertIn("开心果", html)


if __name__ == "__main__":
    unittest.main()
