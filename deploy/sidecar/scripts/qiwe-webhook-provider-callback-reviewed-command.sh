#!/usr/bin/env bash
set -euo pipefail

APPROVAL="approved-production-qiwe-webhook-provider-callback-command"
PUBLIC_HOST="qintopia.cn"

fail() {
  printf 'qiwe_provider_callback_command_status=%s\n' "$1" >&2
  exit "${2:-1}"
}

if [[ $# -ne 1 || ( "$1" != "--check" && "$1" != "--execute" ) ]]; then
  fail "invalid_action" 2
fi
action="${1#--}"
test_mode="${QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_TEST_MODE:-0}"
if [[ "$test_mode" != "0" && "$test_mode" != "1" ]]; then
  fail "invalid_test_mode" 2
fi

if [[ "$test_mode" == "1" ]]; then
  command_file="${QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_FILE:?test command file is required}"
  config_file="${QINTOPIA_QIWE_WEBHOOK_PROVIDER_CONFIG_FILE:?test config file is required}"
  runuser_bin="${QINTOPIA_QIWE_WEBHOOK_PROVIDER_RUNUSER_BIN:?test runuser binary is required}"
  timeout_bin="${QINTOPIA_QIWE_WEBHOOK_PROVIDER_TIMEOUT_BIN:?test timeout binary is required}"
  provider_user="${QINTOPIA_QIWE_WEBHOOK_PROVIDER_USER:-$(id -un)}"
else
  for override in \
    QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_FILE \
    QINTOPIA_QIWE_WEBHOOK_PROVIDER_CONFIG_FILE \
    QINTOPIA_QIWE_WEBHOOK_PROVIDER_RUNUSER_BIN \
    QINTOPIA_QIWE_WEBHOOK_PROVIDER_TIMEOUT_BIN \
    QINTOPIA_QIWE_WEBHOOK_PROVIDER_USER; do
    if [[ -n "${!override:-}" ]]; then
      fail "production_paths_are_fixed" 2
    fi
  done
  if [[ "$(id -u)" -ne 0 ]]; then
    fail "root_required" 2
  fi
  command_file="/etc/qintopia/qiwe-webhook-provider-callback-command"
  config_file="/etc/qintopia/qiwe-webhook-ingress.env"
  runuser_bin="/usr/sbin/runuser"
  timeout_bin="/usr/bin/timeout"
  provider_user="ubuntu"
fi

expected_sha256="${QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_SHA256:-}"
if [[ ! "$expected_sha256" =~ ^[0-9a-f]{64}$ ]]; then
  fail "reviewed_sha256_required" 2
fi

if ! /usr/bin/python3 - "$command_file" "$config_file" "$test_mode" <<'PY'
import os
import stat
import sys

command_path, config_path, test_mode = sys.argv[1:4]
for path, label, max_size in (
    (command_path, "command", 16 * 1024),
    (config_path, "config", 8 * 1024),
):
    metadata = os.lstat(path)
    if not stat.S_ISREG(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
        raise SystemExit(f"{label} must be a regular non symlink file")
    if metadata.st_mode & 0o022:
        raise SystemExit(f"{label} must not be group or world writable")
    if metadata.st_size <= 0 or metadata.st_size > max_size:
        raise SystemExit(f"{label} size is invalid")
    if test_mode == "0" and metadata.st_uid != 0:
        raise SystemExit(f"{label} must be root owned")
    if label == "config" and metadata.st_mode & 0o077:
        raise SystemExit("config must be mode 0600")
if os.stat(command_path).st_mode & 0o222:
    raise SystemExit("command must be read only")
PY
then
  fail "server_local_files_invalid" 2
fi

actual_sha256="$(sha256sum "$command_file" | cut -d ' ' -f 1)"
if [[ "$actual_sha256" != "$expected_sha256" ]]; then
  fail "reviewed_sha256_mismatch" 2
fi

public_token="$(/usr/bin/python3 - "$config_file" <<'PY'
import re
import sys
from pathlib import Path

allowed = {
    "QINTOPIA_QIWE_WEBHOOK_PUBLIC_PATH_TOKEN",
    "QIWE_WEBHOOK_AUTH_TOKEN",
}
values = {}
line_re = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)=(.*)$")
for raw_line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines():
    line = raw_line.strip()
    if not line or line.startswith("#"):
        continue
    match = line_re.fullmatch(line)
    if not match:
        raise SystemExit("invalid ingress config line")
    key, value = match.groups()
    if key not in allowed or key in values:
        raise SystemExit("unsupported or duplicate ingress config key")
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        value = value[1:-1]
    values[key] = value
if set(values) != allowed:
    raise SystemExit("ingress config keys are incomplete")
public_token = values["QINTOPIA_QIWE_WEBHOOK_PUBLIC_PATH_TOKEN"]
internal_token = values["QIWE_WEBHOOK_AUTH_TOKEN"]
if not re.fullmatch(r"[A-Za-z0-9_-]{48,128}", public_token):
    raise SystemExit("invalid public token")
if not re.fullmatch(r"[A-Za-z0-9._~-]{43,256}", internal_token):
    raise SystemExit("invalid internal token")
if public_token == internal_token:
    raise SystemExit("public and internal ingress tokens must differ")

def reject_obvious_placeholder(value, label):
    lowered = value.lower()
    if len(set(value)) < 10 or re.search(r"(.)\1{4,}", value):
        raise SystemExit(f"{label} is an obvious low-complexity value")
    if any(word in lowered for word in ("changeme", "example", "placeholder", "replace", "testtoken")):
        raise SystemExit(f"{label} contains an obvious placeholder")
    for width in range(1, min(8, len(value) // 2) + 1):
        if len(value) % width == 0 and value == value[:width] * (len(value) // width):
            raise SystemExit(f"{label} is an obvious repeated pattern")

reject_obvious_placeholder(public_token, "public token")
reject_obvious_placeholder(internal_token, "internal token")
print(public_token)
PY
)" || fail "ingress_config_invalid" 2

if [[ "$action" == "check" ]]; then
  printf 'qiwe_provider_callback_command_status=ready_for_owner_execution\n'
  exit 0
fi
if [[ "${QINTOPIA_QIWE_WEBHOOK_PROVIDER_COMMAND_APPROVAL:-}" != "$APPROVAL" ]]; then
  fail "owner_approval_required" 2
fi

output_file="$(mktemp)"
chmod 0600 "$output_file"
cleanup() {
  rm -f "$output_file"
}
trap cleanup EXIT

if ! printf 'https://%s/qiwe/webhook/%s\n' "$PUBLIC_HOST" "$public_token" | \
  "$timeout_bin" 30s \
    "$runuser_bin" -u "$provider_user" -- \
    /usr/bin/env -i \
    PATH=/usr/bin:/bin \
    QINTOPIA_QIWE_PROVIDER_ENV_FILE=/etc/qintopia/qiwe-provider.env \
    /bin/bash --noprofile --norc "$command_file" \
  >"$output_file" 2>&1; then
  fail "execution_failed" 1
fi

printf 'qiwe_provider_callback_command_status=executed\n'
