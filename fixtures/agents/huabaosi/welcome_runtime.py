"""Huabaosi's local registered tool produces the actual bounded synthetic PNG."""
import base64
import hashlib
import importlib.util
import os
from pathlib import Path

from local_agent_runtime import EMPTY_ARGUMENTS, REFERENCE, object_schema, text_schema

PLUGIN_ID = "qintopia-welcome-huabaosi"
PLUGIN_VERSION = "0.1.0"


def register(ctx):
    def render_card(_arguments):
        value = ctx.trusted_input(agent="huabaosi", operation="render_card", schema=object_schema({
            "case_ref": REFERENCE, "case_version": {"type": "integer", "minimum": 0},
            "material": object_schema({"display_name": text_schema(40),
                                       "description": {"type": "string", "minLength": 0, "maxLength": 320}}),
        }))
        script = "test_card_artifact.py" if os.environ.get("QINTOPIA_WELCOME_TEST_ARTIFACT") == "1" else "render_synthetic_card.py"
        renderer_path = Path(__file__).resolve().parents[3] / "workflows/resident-welcome/scripts" / script
        spec = importlib.util.spec_from_file_location("qintopia_welcome_synthetic_renderer", renderer_path)
        if spec is None or spec.loader is None:
            raise ValueError("renderer_unavailable")
        renderer = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(renderer)
        content = renderer.render({"synthetic": True, **value["material"]})
        if not content.startswith(b"\x89PNG\r\n\x1a\n") or len(content) > 10 * 1024 * 1024:
            raise ValueError("invalid_rendered_card")
        return {"command": "register_card", "case_ref": value["case_ref"],
                "case_version": value["case_version"], "media_type": "image/png",
                "content_base64": base64.b64encode(content).decode("ascii"),
                "content_hash": hashlib.sha256(content).hexdigest()}

    ctx.register_tool(name="qintopia_welcome_render_card", schema=EMPTY_ARGUMENTS, handler=render_card)
