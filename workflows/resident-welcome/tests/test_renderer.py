import importlib.util
import io
from pathlib import Path
import unittest
from unittest.mock import patch

from PIL import Image


spec = importlib.util.spec_from_file_location(
    "welcome_renderer", Path(__file__).parents[1] / "scripts/render_synthetic_card.py"
)
renderer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(renderer)


class SyntheticRendererTests(unittest.TestCase):
    def test_real_png_without_photo_and_long_name(self):
        raw = renderer.render({
            "synthetic": True,
            "display_name": "合成访客" * 10,
            "description": "来自虚构城市，喜欢阅读与散步。\n期待在这里认识新朋友。",
        })
        with Image.open(io.BytesIO(raw)) as image:
            image.load()
            self.assertEqual(image.size, (1080, 720))
            self.assertEqual(image.format, "PNG")
            self.assertGreater(len(image.getcolors(maxcolors=1080 * 720)), 20)

    def test_source_change_invalidates_identical_pixels_and_is_deterministic(self):
        first = {"synthetic": True, "display_name": "合成", "description": "喜欢散步"}
        revised = {**first, "description": "喜欢阅读"}
        # Force identical visible pixels independently of host fonts.
        with patch.object(renderer.ImageDraw.ImageDraw, "text", return_value=None):
            a = renderer.render(first)
            b = renderer.render(revised)
            self.assertEqual(a, renderer.render(dict(reversed(list(first.items())))))
        with Image.open(io.BytesIO(a)) as old, Image.open(io.BytesIO(b)) as new:
            self.assertEqual(old.tobytes(), new.tobytes())
            self.assertNotEqual(old.info["qintopia_material_sha256"], new.info["qintopia_material_sha256"])
        self.assertNotEqual(a, b)

    def test_rejects_real_or_unbounded_material(self):
        for value in [
            {"display_name": "不可渲染"},
            {"synthetic": True, "display_name": "长" * 41},
            {"synthetic": True, "display_name": "合成", "description": "长" * 321},
            {"synthetic": True, "display_name": "合成\x00"},
        ]:
            with self.assertRaises(ValueError):
                renderer.render(value)


if __name__ == "__main__":
    unittest.main()
