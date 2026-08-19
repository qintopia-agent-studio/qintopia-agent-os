#!/usr/bin/env python3
"""Unit tests for rasterize.py output-bounds clamping.

The Rust daily-case-report pipeline rasterizes flowing roast HTML with
Playwright at 2x density; long reports can exceed the reviewed media-upload
bounds (4096x8192, enforced in the sidecar operations module). These tests
pin the scale computation so the JPEG always stays inside the bounds.
"""
from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import rasterize  # noqa: E402


class RasterizeOutputScaleTests(unittest.TestCase):
    def test_prefers_base_two_scale_for_typical_height(self) -> None:
        # 1080 CSS px wide, ~3209 CSS px tall (Python-era canvas) -> 2x.
        self.assertEqual(rasterize._output_scale(1080, 3209), 2.0)

    def test_clamps_scale_when_2x_height_would_exceed_max(self) -> None:
        # 4930 CSS px tall -> 2x would be 9860 > 8192; clamp to 8192/4930.
        scale = rasterize._output_scale(1080, 4930)
        self.assertAlmostEqual(scale, 8192 / 4930, places=6)
        self.assertLess(scale, 2.0)
        self.assertLessEqual(1080 * scale, rasterize.MAX_OUTPUT_WIDTH)
        self.assertLessEqual(4930 * scale, rasterize.MAX_OUTPUT_HEIGHT)

    def test_clamps_scale_when_2x_width_would_exceed_max(self) -> None:
        scale = rasterize._output_scale(3000, 3000)
        self.assertAlmostEqual(scale, 4096 / 3000, places=6)
        self.assertLessEqual(3000 * scale, rasterize.MAX_OUTPUT_WIDTH)
        self.assertLessEqual(3000 * scale, rasterize.MAX_OUTPUT_HEIGHT)

    def test_takes_the_stricter_of_width_and_height_clamps(self) -> None:
        # width 4096 at 2x already at the max; height 1000*2 also fine ->
        # 2x stays legal for the width, so no extra clamping.
        self.assertEqual(rasterize._output_scale(2048, 1000), 2.0)
        # Extremely tall page: height clamp wins over the 2x preference.
        scale = rasterize._output_scale(1080, 20000)
        self.assertAlmostEqual(scale, 8192 / 20000, places=6)
        self.assertLessEqual(20000 * scale, rasterize.MAX_OUTPUT_HEIGHT)

    def test_invalid_dimensions_fall_back_to_base_scale(self) -> None:
        self.assertEqual(rasterize._output_scale(0, 100), 2.0)
        self.assertEqual(rasterize._output_scale(1080, 0), 2.0)


if __name__ == "__main__":
    unittest.main()
