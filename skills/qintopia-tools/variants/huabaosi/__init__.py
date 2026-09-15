"""Huabaosi completion policy for the official Hermes plugin boundary."""

from __future__ import annotations

import json
import os
import re
from typing import Any, Optional


_POSTER_RECORD_RE = re.compile(r"\brec[a-zA-Z0-9]{8,}\b")


def _completion_evidence(*values: Any) -> str:
    chunks: list[str] = []
    for value in values:
        if value is None:
            continue
        if isinstance(value, str):
            chunks.append(value)
            continue
        try:
            chunks.append(json.dumps(value, ensure_ascii=False, sort_keys=True))
        except TypeError:
            chunks.append(str(value))
    return "\n".join(chunks)


def _on_pre_tool_call(
    tool_name: str = "", args: Any = None, **_: Any
) -> Optional[dict[str, str]]:
    if tool_name != "kanban_complete" or os.environ.get("HERMES_PROFILE") != "huabaosi":
        return None
    if not isinstance(args, dict):
        return {
            "action": "block",
            "message": "kanban_complete blocked: completion arguments must be an object.",
        }

    metadata = args.get("metadata")
    evidence = _completion_evidence(args.get("summary"), args.get("result"), metadata)
    if not _POSTER_RECORD_RE.search(evidence):
        return {
            "action": "block",
            "message": (
                "kanban_complete blocked by Qintopia Huabaosi completion policy: "
                "the summary, result, or metadata must include the design output "
                "record_id (rec...). Use kanban_block when the output record cannot be written."
            ),
        }

    artifacts: list[str] = []
    if isinstance(metadata, dict):
        raw_artifacts = metadata.get("artifacts")
        if isinstance(raw_artifacts, str):
            artifacts = [raw_artifacts] if raw_artifacts.strip() else []
        elif isinstance(raw_artifacts, (list, tuple)):
            artifacts = [str(item) for item in raw_artifacts if str(item).strip()]
    if artifacts and "成品图" not in evidence:
        return {
            "action": "block",
            "message": (
                "kanban_complete blocked by Qintopia Huabaosi completion policy: "
                "metadata.artifacts is populated, but the completion evidence does not "
                "state the 成品图 attachment status. Upload or describe the attachment "
                "status first; use kanban_block on failure."
            ),
        }
    return None


def register(ctx) -> None:
    ctx.register_hook("pre_tool_call", _on_pre_tool_call)
