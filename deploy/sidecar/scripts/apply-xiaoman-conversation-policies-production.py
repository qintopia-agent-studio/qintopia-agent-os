#!/usr/bin/env python3
from __future__ import annotations

import argparse
import importlib.util
import json
import os
import re
import stat
import subprocess
import sys
from pathlib import Path
from typing import Any


SCRIPT_DIR = Path(__file__).resolve().parent
CONFIG_HELPER_PATH = SCRIPT_DIR / "apply-xiaoman-feishu-poster-production-config.py"
SIDECAR_ENV_PATH = Path("/etc/qintopia/message-sidecar.env")
RELEASE_CURRENT_PATH = Path("/home/ubuntu/qintopia-agent-os-releases/current")
APPLY_APPROVAL = "approved-production-xiaoman-conversation-policy-v3"
MAX_INPUT_BYTES = 64 * 1024
SHA256_RE = re.compile(r"[0-9a-f]{64}")
OPAQUE_REF_RE = re.compile(r"sha256:[0-9a-f]{64}")


class PolicyApplyError(ValueError):
    pass


def load_config_helper():
    spec = importlib.util.spec_from_file_location("xiaoman_production_config_helper", CONFIG_HELPER_PATH)
    if spec is None or spec.loader is None:
        raise PolicyApplyError("production configuration helper is unavailable")
    module = importlib.util.module_from_spec(spec)
    sys.modules[spec.name] = module
    spec.loader.exec_module(module)
    return module


def reject_duplicate_keys(pairs: list[tuple[str, Any]]) -> dict[str, Any]:
    result: dict[str, Any] = {}
    for key, value in pairs:
        if key in result:
            raise PolicyApplyError("policy input contains a duplicate key")
        result[key] = value
    return result


def load_policy_input(data: bytes) -> tuple[dict[str, Any], set[str]]:
    if not data or len(data) > MAX_INPUT_BYTES:
        raise PolicyApplyError("policy input length is invalid")
    try:
        value = json.loads(data, object_pairs_hook=reject_duplicate_keys)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PolicyApplyError("policy input is not valid JSON") from exc
    if not isinstance(value, dict) or set(value) != {"schema_version", "policies"}:
        raise PolicyApplyError("policy input must contain only schema_version and policies")
    policies = value.get("policies")
    if value.get("schema_version") != 3 or not isinstance(policies, list):
        raise PolicyApplyError("policy input schema is invalid")
    if not 1 <= len(policies) <= 100:
        raise PolicyApplyError("policy input count is invalid")
    sensitive_ids: set[str] = set()
    for policy in policies:
        if not isinstance(policy, dict):
            raise PolicyApplyError("each policy must be one JSON object")
        chat_id = policy.get("chat_id")
        reviewers = policy.get("reviewer_user_ids", [])
        if isinstance(chat_id, str) and chat_id:
            sensitive_ids.add(chat_id)
        if not isinstance(reviewers, list):
            raise PolicyApplyError("reviewer_user_ids must be an array")
        sensitive_ids.update(item for item in reviewers if isinstance(item, str) and item)
    return value, sensitive_ids


def resolve_sidecar_binary(release_current: Path, release_sha: str) -> Path:
    release_root = release_current.resolve(strict=True)
    sidecar_dir = release_root / "sidecar"
    expected_uid = os.geteuid()
    try:
        sidecar_metadata = os.lstat(sidecar_dir)
        if (
            stat.S_ISLNK(sidecar_metadata.st_mode)
            or not stat.S_ISDIR(sidecar_metadata.st_mode)
            or sidecar_metadata.st_uid != expected_uid
            or sidecar_metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH)
        ):
            raise PolicyApplyError("release sidecar directory boundary is invalid")
        expected = sidecar_dir / "qintopia-message-sidecar"
        metadata = os.lstat(expected)
        resolved_expected = expected.resolve(strict=True)
    except OSError as exc:
        raise PolicyApplyError("release sidecar binary is unavailable") from exc
    if (
        stat.S_ISLNK(metadata.st_mode)
        or not stat.S_ISREG(metadata.st_mode)
        or metadata.st_uid != expected_uid
        or metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH)
        or not os.access(expected, os.X_OK)
        or release_root.name != release_sha
        or resolved_expected != expected
    ):
        raise PolicyApplyError("release sidecar binary boundary is invalid")
    return expected


def validate_policy_report(
    report: Any, *, expected_count: int, database_hash: str
) -> dict[str, Any]:
    expected_keys = {
        "success",
        "action_status",
        "input_count",
        "created_version_count",
        "deduped_count",
        "policies",
        "database_url_sha256",
        "approved_database_url_sha256_matched",
        "external_calls_executed",
        "sensitive_fields_redacted",
    }
    if not isinstance(report, dict) or set(report) != expected_keys:
        raise PolicyApplyError("conversation policy command returned invalid evidence")
    integer_fields = ["input_count", "created_version_count", "deduped_count"]
    if any(type(report[name]) is not int or report[name] < 0 for name in integer_fields):
        raise PolicyApplyError("conversation policy command returned invalid evidence")
    if (
        report["success"] is not True
        or report["action_status"] != "conversation_policies_applied"
        or report["input_count"] != expected_count
        or report["created_version_count"] + report["deduped_count"] != expected_count
        or report["database_url_sha256"] != database_hash
        or not SHA256_RE.fullmatch(report["database_url_sha256"])
        or report["approved_database_url_sha256_matched"] is not True
        or report["external_calls_executed"] is not False
        or report["sensitive_fields_redacted"] is not True
        or not isinstance(report["policies"], list)
        or len(report["policies"]) != expected_count
    ):
        raise PolicyApplyError("conversation policy command returned unsafe evidence")

    policy_keys = {
        "conversation_ref",
        "policy_digest",
        "policy_version",
        "enabled",
        "deduped",
        "reviewer_count",
    }
    for policy in report["policies"]:
        if (
            not isinstance(policy, dict)
            or set(policy) != policy_keys
            or not isinstance(policy["conversation_ref"], str)
            or not OPAQUE_REF_RE.fullmatch(policy["conversation_ref"])
            or not isinstance(policy["policy_digest"], str)
            or not OPAQUE_REF_RE.fullmatch(policy["policy_digest"])
            or type(policy["policy_version"]) is not int
            or policy["policy_version"] < 1
            or type(policy["enabled"]) is not bool
            or type(policy["deduped"]) is not bool
            or type(policy["reviewer_count"]) is not int
            or not 0 <= policy["reviewer_count"] <= 100
        ):
            raise PolicyApplyError("conversation policy command returned invalid policy evidence")
    return report


def run_policy_apply(
    *,
    body: bytes,
    env_path: Path,
    release_current_path: Path,
    approval: str,
    effective_uid: int,
    timeout_seconds: int = 60,
) -> str:
    if effective_uid != 0:
        raise PolicyApplyError("Xiaoman conversation policy apply requires root")
    if approval != APPLY_APPROVAL:
        raise PolicyApplyError("exact owner approval is required for policy apply")
    policy_input, input_sensitive_ids = load_policy_input(body)
    helper = load_config_helper()
    helper.reject_symlinked_parents(env_path)
    helper.reject_symlinked_parents(release_current_path)
    release_sha = helper.resolve_release_sha(release_current_path)
    binary = resolve_sidecar_binary(release_current_path, release_sha)
    env_document = helper.read_env(env_path)

    database_url = helper.require_value(
        env_document.values, "QINTOPIA_SIDECAR_DATABASE_URL"
    )
    database_hash = helper.require_value(
        env_document.values,
        "QINTOPIA_XIAOMAN_CONVERSATION_POLICY_DATABASE_URL_SHA256",
    )
    if helper.sha256_hex(database_url.encode("utf-8")) != database_hash:
        raise PolicyApplyError("conversation policy database binding is invalid")
    chat_ceiling = helper.require_value(
        env_document.values, "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS"
    )
    user_ceiling = helper.require_value(
        env_document.values, "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS"
    )
    ceiling_ids = set(helper.parse_identifier_csv("chat ceiling", chat_ceiling))
    ceiling_ids.update(helper.parse_identifier_csv("user ceiling", user_ceiling))

    child_env = {
        "PATH": "/usr/bin:/bin",
        "QINTOPIA_SIDECAR_DATABASE_URL": database_url,
        "QINTOPIA_XIAOMAN_CONVERSATION_POLICY_APPROVAL": APPLY_APPROVAL,
        "QINTOPIA_XIAOMAN_CONVERSATION_POLICY_DATABASE_URL_SHA256": database_hash,
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS": chat_ceiling,
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS": user_ceiling,
    }
    try:
        result = subprocess.run(
            [str(binary), "conversation-policy-apply", "--stdin"],
            input=body,
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            env=child_env,
            timeout=timeout_seconds,
            check=False,
        )
    except (OSError, subprocess.TimeoutExpired) as exc:
        raise PolicyApplyError("conversation policy command did not complete") from exc

    combined = result.stdout + result.stderr
    sensitive_values = {database_url, chat_ceiling, user_ceiling}
    sensitive_values.update(input_sensitive_ids)
    sensitive_values.update(ceiling_ids)
    if any(value.encode("utf-8") in combined for value in sensitive_values if value):
        raise PolicyApplyError("conversation policy command output failed redaction")
    if result.returncode != 0:
        raise PolicyApplyError("conversation policy command rejected the request")
    try:
        output = result.stdout.decode("utf-8").strip()
        report = json.loads(output)
    except (UnicodeDecodeError, json.JSONDecodeError) as exc:
        raise PolicyApplyError("conversation policy command returned invalid evidence") from exc
    validated = validate_policy_report(
        report,
        expected_count=len(policy_input["policies"]),
        database_hash=database_hash,
    )
    return json.dumps(validated, sort_keys=True, separators=(",", ":"))


def parse_args() -> argparse.Namespace:
    parser = argparse.ArgumentParser(
        description="Apply bounded Xiaoman direct and internal-group policies"
    )
    parser.add_argument("--stdin", action="store_true")
    parser.add_argument("--apply", action="store_true")
    parser.add_argument("--approval", default="")
    return parser.parse_args()


def emit_failure(message: str) -> None:
    print(
        "xiaoman_conversation_policy_apply="
        + json.dumps(
            {
                "success": False,
                "action_status": "policy_apply_failed",
                "error": message,
                "external_calls_executed": False,
                "sensitive_values_redacted": True,
            },
            sort_keys=True,
            separators=(",", ":"),
        )
    )


def main() -> int:
    args = parse_args()
    if not args.stdin or not args.apply:
        emit_failure("--stdin and --apply are required")
        return 1
    try:
        if os.geteuid() != 0:
            raise PolicyApplyError("Xiaoman conversation policy apply requires root")
        if args.approval != APPLY_APPROVAL:
            raise PolicyApplyError("exact owner approval is required for policy apply")
        body = sys.stdin.buffer.read(MAX_INPUT_BYTES + 1)
        output = run_policy_apply(
            body=body,
            env_path=SIDECAR_ENV_PATH,
            release_current_path=RELEASE_CURRENT_PATH,
            approval=args.approval,
            effective_uid=os.geteuid(),
        )
        print(output)
        return 0
    except PolicyApplyError as exc:
        emit_failure(str(exc))
        return 1
    except Exception:
        emit_failure("unexpected policy apply failure")
        return 1


if __name__ == "__main__":
    raise SystemExit(main())
