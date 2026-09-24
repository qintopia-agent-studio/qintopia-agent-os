"""Erhua binds review and forwarding instructions to the host's exact artifact."""
from local_agent_runtime import EMPTY_ARGUMENTS, HASH, PHASE, REFERENCE, object_schema

PLUGIN_ID = "qintopia-welcome-erhua"
PLUGIN_VERSION = "0.1.0"


def register(ctx):
    def forward_review(_arguments):
        value = ctx.trusted_input(agent="erhua", operation="forward_review", schema=object_schema({
            "case_ref": REFERENCE, "target_ref": REFERENCE, "phase": PHASE,
            "artifact_ref": REFERENCE, "content_hash": HASH,
            "approval_kind": {"type": "string", "enum": ["content_review", "publish_confirmation"]},
            "reviewer": REFERENCE,
        }))
        return {"command": "forward_review", **value}

    def forward(_arguments):
        value = ctx.trusted_input(agent="erhua", operation="forward", schema=object_schema({
            "action_ref": REFERENCE, "artifact_ref": REFERENCE, "content_hash": HASH,
            "target_ref": REFERENCE, "phase": PHASE,
            "part": {"type": "string", "enum": ["image", "text"]}, "attempt_ref": REFERENCE,
        }))
        # The host verifies this instruction again before its synthetic adapter.
        # The plugin cannot rewrite content, select a new target, or send directly.
        return {"command": "forward", **value}

    ctx.register_tool(name="qintopia_welcome_forward_review", schema=EMPTY_ARGUMENTS, handler=forward_review)
    ctx.register_tool(name="qintopia_welcome_forward", schema=EMPTY_ARGUMENTS, handler=forward)
