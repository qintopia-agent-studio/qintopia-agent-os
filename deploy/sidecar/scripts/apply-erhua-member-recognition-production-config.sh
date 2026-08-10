#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_ERHUA_MEMBER_RECOGNITION_PRODUCTION_CONFIG:-}" != "approved-production-erhua-member-recognition-config" ]]; then
  echo "Erhua member recognition production config requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
PYTHON_BIN="/usr/bin/python3"
ENV_FILE="/etc/qintopia/message-sidecar.env"

fail() {
  echo "Erhua member recognition production config failed: $1" >&2
  exit 1
}

if [[ "$#" != "1" || "${1:-}" != "--apply" ]]; then
  fail "usage: apply-erhua-member-recognition-production-config.sh --apply"
fi
if [[ ! -x "$PYTHON_BIN" ]]; then
  fail "fixed python3 is required"
fi
if [[ ! -f "$ENV_FILE" ]]; then
  fail "persistent sidecar env file is required"
fi

"$PYTHON_BIN" - "$ENV_FILE" <<'PY'
from __future__ import annotations

import os
import re
import stat
import sys
import tempfile
from pathlib import Path


path = Path(sys.argv[1])
assignment = re.compile(r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=")
managed_keys = {
    "QINTOPIA_PROFILE_TARGET_CHAT_IDS",
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_CHAT_ID",
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_SENDER_ID",
}


def clean_secret_label(value: str, label: str) -> str:
    value = value.strip()
    if not value:
        raise SystemExit(f"{label} is required")
    if any(ch in value for ch in "\r\n\t "):
        raise SystemExit(f"{label} must not contain whitespace")
    if any(ch in value for ch in "'\"`$\\"):
        raise SystemExit(f"{label} must not contain shell metacharacters")
    return value


def quote_env(value: str) -> str:
    return "'" + value.replace("'", "'\\''") + "'"


try:
    st = path.lstat()
except FileNotFoundError:
    raise SystemExit("persistent sidecar env file is required")
if stat.S_ISLNK(st.st_mode) or not stat.S_ISREG(st.st_mode):
    raise SystemExit("persistent sidecar env file must be a regular non-symlink file")
if st.st_nlink != 1:
    raise SystemExit("persistent sidecar env file must not have hard links")
if stat.S_IMODE(st.st_mode) & (stat.S_IWGRP | stat.S_IWOTH):
    raise SystemExit("persistent sidecar env file must not be group/world writable")

original = path.read_text(encoding="utf-8").splitlines()
counts = {"QINTOPIA_SIDECAR_DATABASE_URL": 0}
existing: dict[str, list[str]] = {}
kept: list[str] = []

for line in original:
    match = assignment.match(line.strip())
    if not match:
        kept.append(line)
        continue
    key = match.group(1)
    if key in counts:
        counts[key] += 1
    if key in managed_keys:
        value = line.split("=", 1)[1].strip().strip("'\"")
        existing.setdefault(key, []).append(value)
        continue
    kept.append(line)

if counts["QINTOPIA_SIDECAR_DATABASE_URL"] != 1:
    raise SystemExit("requires exactly one QINTOPIA_SIDECAR_DATABASE_URL")

requested_chat_id = os.environ.get("QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CHAT_ID", "")
profile_targets = existing.get("QINTOPIA_PROFILE_TARGET_CHAT_IDS", [])
if requested_chat_id:
    chat_id = clean_secret_label(requested_chat_id, "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CHAT_ID")
elif len(profile_targets) == 1:
    target_items = [item.strip() for item in profile_targets[0].split(",") if item.strip()]
    if len(target_items) != 1:
        raise SystemExit("existing QINTOPIA_PROFILE_TARGET_CHAT_IDS must contain exactly one reviewed group")
    chat_id = clean_secret_label(target_items[0], "QINTOPIA_PROFILE_TARGET_CHAT_IDS")
else:
    raise SystemExit("QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CHAT_ID is required when no single profile target exists")

requested_sender_id = os.environ.get("QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID", "")
existing_sender = existing.get("QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_SENDER_ID", [])
if requested_sender_id:
    sender_id = clean_secret_label(
        requested_sender_id,
        "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID",
    )
elif len(existing_sender) == 1:
    sender_id = clean_secret_label(existing_sender[0], "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_SENDER_ID")
else:
    raise SystemExit("QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID is required")
if sender_id == chat_id:
    raise SystemExit("QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID must differ from the reviewed group id")

target_values = {
    "QINTOPIA_PROFILE_TARGET_CHAT_IDS": chat_id,
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_CHAT_ID": chat_id,
    "QINTOPIA_ERHUA_MEMBER_RECOGNITION_CANARY_SENDER_ID": sender_id,
}

while kept and not kept[-1].strip():
    kept.pop()
if kept:
    kept.append("")
kept.append("# Managed by apply-erhua-member-recognition-production-config.sh")
for key, value in target_values.items():
    kept.append(f"{key}={quote_env(value)}")

env_dir = str(path.parent)
fd, temp_name = tempfile.mkstemp(prefix=f".{path.name}.", dir=env_dir, text=True)
try:
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        fh.write("\n".join(kept))
        fh.write("\n")
        fh.flush()
        os.fsync(fh.fileno())
    os.chown(temp_name, st.st_uid, st.st_gid)
    os.chmod(temp_name, stat.S_IMODE(st.st_mode))
    os.replace(temp_name, path)
except Exception:
    try:
        os.unlink(temp_name)
    except FileNotFoundError:
        pass
    raise

print("Erhua member recognition production config applied: chat_id=reviewed, canary_sender_id=reviewed")
PY
