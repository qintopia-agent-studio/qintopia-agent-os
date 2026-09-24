"""Anan's registered local task tools coordinate governed welcome work."""
from local_agent_runtime import EMPTY_ARGUMENTS, PHASE, REFERENCE, object_schema, text_schema

PLUGIN_ID = "qintopia-welcome-anan"
PLUGIN_VERSION = "0.1.0"
SETTING = object_schema({
    "mode": {"type": "string", "enum": ["direct", "review"]},
    "parts": {"type": "array", "items": {"type": "string", "enum": ["image", "text"]},
              "minItems": 1, "maxItems": 2, "uniqueItems": True},
    "phase": PHASE,
    "text_template": text_schema(2048),
})


def register(ctx):
    def request_card(_arguments):
        value = ctx.trusted_input(agent="anan", operation="request_card", schema=object_schema({
            "case_ref": REFERENCE, "case_version": {"type": "integer", "minimum": 0},
        }))
        return {"command": "request_card", "target_agent": "huabaosi", **value}

    def prepare(_arguments):
        value = ctx.trusted_input(agent="anan", operation="prepare", schema=object_schema({
            "case_ref": REFERENCE, "target_ref": REFERENCE, "phase": PHASE, "setting": SETTING,
        }))
        setting = value["setting"]
        if setting["phase"] != value["phase"]:
            raise ValueError("welcome_phase_mismatch")
        # The welcome remains card + text. A text-only direct setting leaves the
        # card for content review instead of silently authorizing it or omitting it.
        return {"command": "prepare_parts", "case_ref": value["case_ref"],
                "target_ref": value["target_ref"], "phase": value["phase"],
                "parts": [{"part": part, "requires_content_review": setting["mode"] == "review" or part not in setting["parts"]}
                          for part in ("image", "text")]}

    ctx.register_tool(name="qintopia_welcome_request_card", schema=EMPTY_ARGUMENTS, handler=request_card)
    ctx.register_tool(name="qintopia_welcome_prepare", schema=EMPTY_ARGUMENTS, handler=prepare)
