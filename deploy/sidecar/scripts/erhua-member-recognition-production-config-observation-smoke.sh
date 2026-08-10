#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "Erhua member recognition production config observation skipped: set QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_ENABLE=1 to inspect fixed production config" >&2
  exit 0
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
PYTHON_BIN="/usr/bin/python3"
DEFAULT_ENV_FILE="/etc/qintopia/message-sidecar.env"
ENV_FILE="$DEFAULT_ENV_FILE"
TEST_MODE="${QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_TEST_MODE:-0}"
TEST_ROOT="${QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_TEST_ROOT:-}"

fail() {
  echo "Erhua member recognition production config observation failed: $1" >&2
  exit 1
}

if [[ "$TEST_MODE" != "0" && "$TEST_MODE" != "1" ]]; then
  fail "test mode must be 0 or 1"
fi
if [[ "$TEST_MODE" == "1" ]]; then
  ENV_FILE="${QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_ENV_FILE:-$DEFAULT_ENV_FILE}"
  if [[ "$TEST_ROOT" != /tmp/* && "$TEST_ROOT" != /private/tmp/* ]]; then
    fail "test mode requires a /tmp test root"
  fi
  case "$ENV_FILE" in
    "$TEST_ROOT"/*) ;;
    *) fail "test env file must stay under the test root" ;;
  esac
elif [[ "$ENV_FILE" != "$DEFAULT_ENV_FILE" ]]; then
  fail "production observation requires the fixed production env file"
fi
if [[ ! -x "$PYTHON_BIN" ]]; then
  fail "fixed python3 is required"
fi

"$PYTHON_BIN" - "$ENV_FILE" "$TEST_MODE" <<'PY'
from __future__ import annotations

import hashlib
import json
import re
import shlex
import stat
import sys
from pathlib import Path


path = Path(sys.argv[1])
test_mode = sys.argv[2] == "1"
assignment = re.compile(r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=(.*)$")
tracked_keys = {
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_PROFILE_TARGET_CHAT_IDS",
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_CHAT_ID",
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_SENDER_ID",
}


def parse_value(raw: str) -> str:
    lexer = shlex.shlex(raw.strip(), posix=True)
    lexer.whitespace_split = True
    lexer.commenters = ""
    parts = list(lexer)
    if len(parts) != 1:
        raise ValueError("unsafe tracked value")
    value = parts[0].strip()
    if not value or any(ch in value for ch in "\r\n\t "):
        raise ValueError("empty or whitespace tracked value")
    if any(ch in value for ch in "'\"`$\\"):
        raise ValueError("unsafe tracked value")
    return value


def scope_fingerprint(chat_id: str) -> str:
    digest = hashlib.sha256()
    digest.update(b"qintopia-erhua-member-recognition-scope-v1\0")
    digest.update(chat_id.encode("utf-8"))
    return "sha256:" + digest.hexdigest()


report = {
    "success": False,
    "worker": "erhua-member-recognition-production-config-observation",
    "action_status": "not_ready",
    "safe_for_chat": True,
    "test_mode": test_mode,
    "env_file_present": False,
    "env_file_secure": False,
    "database_url_count": 0,
    "profile_target_count": 0,
    "has_canary_chat_id": False,
    "has_canary_sender_id": False,
    "profile_target_matches_canary_chat": False,
    "canary_sender_differs_from_chat": False,
    "scope_fingerprint": "",
    "limitations": [],
    "guardrails": [
        "read-only persistent env inspection",
        "does not print group id or sender id",
        "does not print database URL",
        "does not call QiWe, Postgres, MCP, systemctl, or network",
    ],
}

try:
    st = path.lstat()
except FileNotFoundError:
    report["limitations"].append("env_file_missing")
    print("erhua_member_recognition_production_config_observation=" + json.dumps(report, sort_keys=True, separators=(",", ":")))
    raise SystemExit(1)

report["env_file_present"] = True
if stat.S_ISLNK(st.st_mode) or not stat.S_ISREG(st.st_mode):
    report["limitations"].append("env_file_not_regular")
elif st.st_nlink != 1:
    report["limitations"].append("env_file_has_hard_links")
elif stat.S_IMODE(st.st_mode) & (stat.S_IWGRP | stat.S_IWOTH):
    report["limitations"].append("env_file_group_or_world_writable")
else:
    report["env_file_secure"] = True

values: dict[str, list[str]] = {key: [] for key in tracked_keys}
try:
    for lineno, raw in enumerate(path.read_text(encoding="utf-8").splitlines(), 1):
        stripped = raw.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = assignment.match(stripped)
        if not match:
            continue
        key, raw_value = match.groups()
        if key not in tracked_keys:
            continue
        if key == "QINTOPIA_SIDECAR_DATABASE_URL":
            values[key].append("present")
            continue
        try:
            values[key].append(parse_value(raw_value))
        except ValueError:
            report["limitations"].append(f"{key}_invalid_tracked_value")
except UnicodeDecodeError:
    report["limitations"].append("env_file_not_utf8")

report["database_url_count"] = len(values["QINTOPIA_SIDECAR_DATABASE_URL"])
profile_targets = values["QINTOPIA_PROFILE_TARGET_CHAT_IDS"]
canary_chats = values["QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_CHAT_ID"]
canary_senders = values["QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_SENDER_ID"]

if len(profile_targets) == 1:
    targets = [item.strip() for item in profile_targets[0].split(",") if item.strip()]
else:
    targets = []
report["profile_target_count"] = len(targets)
report["has_canary_chat_id"] = len(canary_chats) == 1
report["has_canary_sender_id"] = len(canary_senders) == 1

if len(profile_targets) != 1:
    report["limitations"].append("profile_target_key_count_not_one")
elif len(targets) != 1:
    report["limitations"].append("profile_target_value_count_not_one")

if len(canary_chats) != 1:
    report["limitations"].append("canary_chat_key_count_not_one")
if len(canary_senders) != 1:
    report["limitations"].append("canary_sender_key_count_not_one")
if report["database_url_count"] != 1:
    report["limitations"].append("database_url_key_count_not_one")

if len(targets) == 1 and len(canary_chats) == 1:
    report["profile_target_matches_canary_chat"] = targets[0] == canary_chats[0]
    if report["profile_target_matches_canary_chat"]:
        report["scope_fingerprint"] = scope_fingerprint(canary_chats[0])
    else:
        report["limitations"].append("profile_target_canary_chat_mismatch")
if len(canary_chats) == 1 and len(canary_senders) == 1:
    report["canary_sender_differs_from_chat"] = canary_senders[0] != canary_chats[0]
    if not report["canary_sender_differs_from_chat"]:
        report["limitations"].append("canary_sender_equals_chat")

ready = (
    report["env_file_secure"]
    and report["database_url_count"] == 1
    and report["profile_target_count"] == 1
    and report["has_canary_chat_id"]
    and report["has_canary_sender_id"]
    and report["profile_target_matches_canary_chat"]
    and report["canary_sender_differs_from_chat"]
)
if ready:
    report["success"] = True
    report["action_status"] = "ready_for_member_recognition_runbook"

print("erhua_member_recognition_production_config_observation=" + json.dumps(report, sort_keys=True, separators=(",", ":")))
raise SystemExit(0 if ready else 1)
PY
