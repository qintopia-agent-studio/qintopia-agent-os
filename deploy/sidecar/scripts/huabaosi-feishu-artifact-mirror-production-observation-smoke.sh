#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "Huabaosi Feishu mirror production observation skipped: set QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_OBSERVATION_ENABLE=1 to inspect runtime state" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-/etc/qintopia/message-sidecar.env}"
RELEASE_CURRENT_DIR="${QINTOPIA_RELEASE_CURRENT_DIR:-/home/ubuntu/qintopia-agent-os-releases/current}"
PREFLIGHT_SERVICE="qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service"
WORKER_SERVICE="qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.service"
WORKER_TIMER="qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer"
SYSTEMCTL="${SYSTEMCTL:-systemctl}"
EXPECTED_STATE="${QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_EXPECTED_STATE:-auto}"

cd "$MONOREPO_ROOT"

if [[ -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
  SIDECAR_BIN="$QINTOPIA_SIDECAR_BIN"
else
  SIDECAR_BIN="${RELEASE_CURRENT_DIR}/sidecar/qintopia-message-sidecar"
fi

if ! python3 - "$SIDECAR_BIN" "$RELEASE_CURRENT_DIR" <<'PY'
import json
import os
import re
import stat
import sys

bin_path, current_path = sys.argv[1:3]
if not os.path.isabs(bin_path) or not os.path.exists(current_path):
    raise SystemExit(1)

current_real = os.path.realpath(current_path)
release_sha = os.path.basename(current_real)
if not re.fullmatch(r"[0-9a-f]{40}", release_sha):
    raise SystemExit(1)

expected_bin = os.path.join(current_real, "sidecar", "qintopia-message-sidecar")
if os.path.realpath(bin_path) != expected_bin:
    raise SystemExit(1)
if os.path.islink(bin_path) or not os.path.isfile(bin_path) or not os.access(bin_path, os.X_OK):
    raise SystemExit(1)

for path in (current_real, os.path.dirname(expected_bin), expected_bin):
    mode = os.stat(path).st_mode
    if mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise SystemExit(1)

manifest_path = os.path.join(current_real, "sidecar", "artifact-manifest.json")
with open(manifest_path, encoding="utf-8") as fh:
    manifest = json.load(fh)
if manifest.get("validation", {}).get("cargo_features") != [
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
]:
    raise SystemExit(1)
if manifest.get("commit_sha") != release_sha:
    raise SystemExit(1)
PY
then
  echo "Huabaosi Feishu mirror production observation requires the immutable release/current sidecar binary with approved features" >&2
  exit 1
fi

parse_observation_enablement() {
  local path="$1"
  if [[ ! -f "$path" ]]; then
    printf '0\n'
    return 0
  fi
  python3 - "$path" <<'PY'
import re
import sys

path = sys.argv[1]
key_name = "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED"
value = None
assignment = re.compile(r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=[ \t]*(.*?)[ \t]*(?:#[^\"']*)?$")

with open(path, encoding="utf-8") as fh:
    for lineno, raw in enumerate(fh, 1):
        line = raw.rstrip("\r\n")
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = assignment.fullmatch(line)
        if not match:
            raise SystemExit(f"invalid observation env line {lineno}")
        key, candidate = match.groups()
        if key != key_name:
            continue
        if value is not None:
            raise SystemExit("duplicate observation enable key")
        if any(token in candidate for token in ("$(", "`", "\\", ";", "|", "&", "<", ">", "(", ")")):
            raise SystemExit("unsafe observation enable value")
        if (candidate.startswith('"') and candidate.endswith('"')) or (
            candidate.startswith("'") and candidate.endswith("'")
        ):
            candidate = candidate[1:-1]
        if candidate not in {"0", "1"}:
            raise SystemExit("invalid observation enable value")
        value = candidate

print(value or "0")
PY
}

MIRROR_ENABLED="$(parse_observation_enablement "$ENV_FILE")" || {
  echo "Huabaosi Feishu mirror observation env is invalid" >&2
  exit 1
}

mirror_flag="$MIRROR_ENABLED"
mirror_flag="${mirror_flag//[[:space:]]/}"
if [[ "$EXPECTED_STATE" == "auto" ]]; then
  if [[ "$mirror_flag" == "1" ]]; then
    EXPECTED_STATE="enabled"
  else
    EXPECTED_STATE="disabled"
  fi
fi
if [[ "$EXPECTED_STATE" != "enabled" && "$EXPECTED_STATE" != "disabled" ]]; then
  echo "Huabaosi Feishu mirror expected state must be enabled, disabled, or auto" >&2
  exit 1
fi
if [[ "$EXPECTED_STATE" == "enabled" && "$mirror_flag" != "1" ]]; then
  echo "Huabaosi Feishu mirror enablement does not match expected state" >&2
  exit 1
fi
if [[ "$EXPECTED_STATE" == "disabled" && "$mirror_flag" == "1" ]]; then
  echo "Huabaosi Feishu mirror disablement does not match expected state" >&2
  exit 1
fi

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for Huabaosi Feishu mirror production observation" >&2
  exit 1
fi

if [[ "$EXPECTED_STATE" == "enabled" ]]; then
  for unit in "$PREFLIGHT_SERVICE" "$WORKER_SERVICE" "$WORKER_TIMER"; do
    if ! "$SYSTEMCTL" cat "$unit" >/dev/null 2>&1; then
      echo "Huabaosi Feishu mirror production unit is missing" >&2
      exit 1
    fi
  done
  "$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"
  "$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"
else
  if "$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER" >/dev/null 2>&1; then
    echo "Huabaosi Feishu mirror timer must not be enabled" >&2
    exit 1
  fi
  if "$SYSTEMCTL" is-active --quiet "$WORKER_TIMER" >/dev/null 2>&1; then
    echo "Huabaosi Feishu mirror timer must not be active" >&2
    exit 1
  fi
fi

run_sidecar_with_observation_env() {
  python3 - "$ENV_FILE" "$SIDECAR_BIN" "$RELEASE_CURRENT_DIR" "$@" <<'PY'
import os
import re
import sys

env_path, bin_path, current_path, *args = sys.argv[1:]
allowed = {
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_SIDECAR_DB_MAX_CONNECTIONS",
    "QINTOPIA_DEPLOYED_COMMIT_SHA",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED",
    "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL",
    "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA",
    "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256",
    "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS",
    "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID",
    "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS",
    "QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH",
    "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION",
    "QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS",
    "QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES",
}
assignment = re.compile(r"^(?:export[ \t]+)?([A-Z][A-Z0-9_]*)[ \t]*=[ \t]*(.*)$")
values = {}

if os.path.exists(env_path):
    with open(env_path, encoding="utf-8") as fh:
        for lineno, raw in enumerate(fh, 1):
            line = raw.rstrip("\r\n")
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            match = assignment.fullmatch(line)
            if not match:
                raise SystemExit(f"invalid observation env line {lineno}")
            key, value = match.groups()
            if key not in allowed:
                continue
            if key in values:
                raise SystemExit(f"duplicate observation env key {key}")
            value = value.strip()
            if len(value) >= 2 and value[0] == value[-1] and value[0] in {'"', "'"}:
                value = value[1:-1]
            values[key] = value

child_env = {}
for key in ("PATH", "HOME", "LANG", "LC_ALL", "TZ", "SSL_CERT_FILE", "SSL_CERT_DIR"):
    if key in os.environ:
        child_env[key] = os.environ[key]
child_env.update(values)
child_env["QINTOPIA_DEPLOYED_COMMIT_SHA"] = os.path.basename(
    os.path.realpath(current_path)
)
os.execve(bin_path, [bin_path, *args], child_env)
PY
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

assert_no_sensitive_output() {
  local label="$1"
  local file="$2"
  local forbidden=(
    "tenant_access_token"
    "file_token"
    "artifact_uri"
    "base_token"
    "table_id"
    "external_calls_executed\": true"
    "database_writes_executed\": true"
  )
  local value_name
  for value_name in \
    QINTOPIA_SIDECAR_DATABASE_URL \
    QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN \
    QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID \
    QINTOPIA_HUABAOSI_FEISHU_APP_ID \
    QINTOPIA_HUABAOSI_FEISHU_APP_SECRET; do
    if [[ -n "${!value_name:-}" ]]; then
      forbidden+=("${!value_name}")
    fi
  done
  local token
  for token in "${forbidden[@]}"; do
    if [[ -n "$token" ]] && grep -Fq -- "$token" "$file"; then
      echo "${label} contains forbidden sensitive output" >&2
      exit 1
    fi
  done
}

preflight="$tmp_dir/preflight.json"
preflight_stderr="$tmp_dir/preflight.stderr"
set +e
run_sidecar_with_observation_env huabaosi-feishu-artifact-mirror-preflight >"$preflight" 2>"$preflight_stderr"
preflight_status=$?
set -e
assert_no_sensitive_output "Huabaosi Feishu mirror preflight" "$preflight"
assert_no_sensitive_output "Huabaosi Feishu mirror preflight stderr" "$preflight_stderr"
python3 - "$preflight" "$preflight_status" "$EXPECTED_STATE" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)
status = int(sys.argv[2])
expected_state = sys.argv[3]

assert payload["worker"] == "huabaosi-feishu-artifact-mirror-worker"
assert payload["schema_version"] == "huabaosi-generated-image-v1"
assert payload["mirror_enabled"] is (expected_state == "enabled")
assert payload["external_calls_executed"] is False
assert payload["database_writes_executed"] is False
assert payload["sensitive_fields_redacted"] is True
if expected_state == "enabled":
    assert payload["success"] is True
    assert payload["adapter_compiled"] is True
    assert payload["config_valid"] is True
    assert payload["action_status"] == "adapter_config_ready"
    assert payload["missing_configuration"] == []
    assert status == 0
else:
    assert payload["success"] is False
    assert payload["action_status"] in {
        "mirror_disabled",
        "adapter_not_configured",
        "adapter_not_compiled",
    }
    assert status != 0
PY

preview="$tmp_dir/preview.json"
preview_stderr="$tmp_dir/preview.stderr"
set +e
run_sidecar_with_observation_env run-huabaosi-feishu-artifact-mirror-worker --once --dry-run >"$preview" 2>"$preview_stderr"
preview_status=$?
set -e
assert_no_sensitive_output "Huabaosi Feishu mirror dry-run" "$preview"
assert_no_sensitive_output "Huabaosi Feishu mirror dry-run stderr" "$preview_stderr"
if [[ "$preview_status" != "0" ]]; then
  echo "Huabaosi Feishu mirror dry-run failed" >&2
  exit 1
fi
python3 - "$preview" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

assert payload["success"] is True
assert payload["worker"] == "huabaosi-feishu-artifact-mirror-worker"
assert payload["dry_run"] is True
assert payload["apply_requested"] is False
assert payload["fixture_mode"] is False
assert payload["schema_version"] == "huabaosi-generated-image-v1"
assert payload["external_calls_executed"] is False
assert payload["database_writes_executed"] is False
assert payload["sensitive_fields_redacted"] is True
assert payload["action_status"] in {
    "no_mirrorable_generated_images",
    "already_synced",
    "mirror_preview_ready",
}
PY

echo "Huabaosi Feishu artifact mirror production observation passed"
