"""Fixed PNG fixture with an input digest; no typography or production renderer."""
import hashlib
import json
import struct
import zlib

# A fixed 1x1 RGB PNG. Only provenance metadata changes with the synthetic input.
PIXELS = b"\x00\x19\x20\x1c"

def chunk(kind, payload):
    return struct.pack(">I", len(payload)) + kind + payload + struct.pack(">I", zlib.crc32(kind + payload))

def render(material):
    if not isinstance(material, dict) or set(material) - {"synthetic", "display_name", "description"}:
        raise ValueError("unsupported_card_fields")
    if material.get("synthetic") is not True:
        raise ValueError("synthetic_material_required")
    name, description = material.get("display_name", ""), material.get("description", "")
    if not isinstance(name, str) or not 1 <= len(name) <= 40:
        raise ValueError("invalid_name")
    if not isinstance(description, str) or len(description) > 320:
        raise ValueError("invalid_description")
    if any(ord(c) < 32 and c != "\n" for c in name + description):
        raise ValueError("invalid_controls")
    source = json.dumps(material, ensure_ascii=False, sort_keys=True, separators=(",", ":")).encode()
    metadata = b"qintopia_material_sha256\x00" + hashlib.sha256(source).hexdigest().encode()
    return (b"\x89PNG\r\n\x1a\n" + chunk(b"IHDR", struct.pack(">IIBBBBB", 1, 1, 8, 2, 0, 0, 0))
            + chunk(b"tEXt", metadata) + chunk(b"IDAT", zlib.compress(PIXELS)) + chunk(b"IEND", b""))
