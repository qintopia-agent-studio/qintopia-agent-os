#!/usr/bin/env bash
set -euo pipefail

APPROVAL="approved-production-qiwe-image-send-intro-text-v1"
ENV_FILE="/etc/qintopia/message-sidecar.env"
RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"
KEY="QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED"
VALUE="1"

fail() {
  local reason="$1"
  echo "qintopia_runtime_one_shot_safe_failure=qiwe image send intro-text enable: ${reason}" >&2
  echo "qiwe image send intro-text enable failed: ${reason}" >&2
  exit 1
}

if [[ "${QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLE:-}" != "$APPROVAL" ]]; then
  fail "approval missing"
fi

expected_release_sha="${QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLE_RELEASE_SHA:-}"
if [[ ! "$expected_release_sha" =~ ^[0-9a-f]{40}$ ]]; then
  fail "release sha invalid"
fi

if [[ ! -L "$RELEASE_CURRENT" ]]; then
  fail "release current missing"
fi

release_target="$(readlink -f "$RELEASE_CURRENT")"
if [[ "${release_target##*/}" != "$expected_release_sha" ]]; then
  fail "release sha drift"
fi

if [[ ! -f "$ENV_FILE" || -L "$ENV_FILE" ]]; then
  fail "persistent env shape invalid"
fi

python3 - "$ENV_FILE" "$KEY" "$VALUE" <<'PY' || fail "persistent env update failed"
from __future__ import annotations

import os
import stat
import sys
from pathlib import Path

env_path = Path(sys.argv[1])
key = sys.argv[2]
expected = sys.argv[3]
line = f"{key}={expected}"

metadata = env_path.lstat()
if stat.S_ISLNK(metadata.st_mode) or not stat.S_ISREG(metadata.st_mode):
    raise SystemExit("persistent env is not a regular file")
if metadata.st_nlink != 1:
    raise SystemExit("persistent env has unsafe links")
if stat.S_IMODE(metadata.st_mode) & (stat.S_IWGRP | stat.S_IWOTH):
    raise SystemExit("persistent env is group/world writable")
if metadata.st_size <= 0 or metadata.st_size > 1024 * 1024:
    raise SystemExit("persistent env size invalid")

text = env_path.read_text(encoding="utf-8")
matches = [
    raw
    for raw in text.splitlines()
    if raw.startswith(f"{key}=") or raw.startswith(f"export {key}=")
]
if len(matches) > 1:
    raise SystemExit("intro-text key is duplicated")
if len(matches) == 1:
    if matches[0] != line:
        raise SystemExit("intro-text key value invalid")
    print("qiwe_image_send_intro_text_enable=deduped")
    raise SystemExit(0)

suffix = "" if text.endswith("\n") else "\n"
with env_path.open("a", encoding="utf-8") as handle:
    handle.write(f"{suffix}{line}\n")
    handle.flush()
    os.fsync(handle.fileno())

print("qiwe_image_send_intro_text_enable=applied")
PY

echo "qiwe image send intro-text enable completed"
