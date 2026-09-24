"""Artifact/version tests independent of fonts, Pillow or a live image provider."""
import hashlib
import importlib.util
from pathlib import Path
import struct
import unittest
import zlib

spec = importlib.util.spec_from_file_location("test_card", Path(__file__).parents[1] / "scripts/test_card_artifact.py")
renderer = importlib.util.module_from_spec(spec)
spec.loader.exec_module(renderer)

def png_chunks(raw):
    assert raw[:8] == b"\x89PNG\r\n\x1a\n"
    offset = 8
    chunks = {}
    while offset < len(raw):
        length = struct.unpack(">I", raw[offset:offset+4])[0]
        kind, data = raw[offset+4:offset+8], raw[offset+8:offset+8+length]
        crc = struct.unpack(">I", raw[offset+8+length:offset+12+length])[0]
        assert crc == zlib.crc32(kind + data)
        chunks[kind] = data
        offset += 12 + length
    assert offset == len(raw) and b"IEND" in chunks
    return chunks

class SyntheticRendererTests(unittest.TestCase):
    def test_valid_png_without_visual_dependencies(self):
        chunks = png_chunks(renderer.render({"synthetic": True, "display_name": "合成访客" * 10}))
        self.assertEqual(struct.unpack(">IIBBBBB", chunks[b"IHDR"]), (1, 1, 8, 2, 0, 0, 0))
        self.assertEqual(zlib.decompress(chunks[b"IDAT"]), b"\x00\x19\x20\x1c")

    def test_source_change_invalidates_identical_pixels_and_is_deterministic(self):
        first = {"synthetic": True, "display_name": "合成", "description": "喜欢散步"}
        a = renderer.render(first)
        b = renderer.render({**first, "description": "喜欢阅读"})
        self.assertEqual(a, renderer.render(dict(reversed(list(first.items())))))
        self.assertEqual(png_chunks(a)[b"IDAT"], png_chunks(b)[b"IDAT"])
        self.assertNotEqual(hashlib.sha256(a).digest(), hashlib.sha256(b).digest())

    def test_rejects_real_or_unbounded_material(self):
        for value in [{"display_name": "真实"}, {"synthetic": True, "display_name": "长" * 41},
                      {"synthetic": True, "display_name": "合成", "description": "长" * 321},
                      {"synthetic": True, "display_name": "合成\x00"}]:
            with self.assertRaises(ValueError):
                renderer.render(value)
