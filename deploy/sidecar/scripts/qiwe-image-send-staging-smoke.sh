#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE:-}" != "1" ]]; then
  echo "QiWe image-send staging smoke skipped: set QINTOPIA_QIWE_IMAGE_STAGING_SMOKE_ENABLE=1 for one approved staging exercise" >&2
  exit 0
fi

if [[ "${QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL:-}" != "approved-staging-qiwe-image-send" ]]; then
  echo "QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send is required" >&2
  exit 1
fi

ENV_FILE="${QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE:-}"
PHASE="${QINTOPIA_QIWE_IMAGE_STAGING_PHASE:-}"
WORK_ITEM_ID="${QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID:-}"
EXPECTED_DATABASE_HASH="${QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256:-}"

if [[ -z "$ENV_FILE" || ! -f "$ENV_FILE" || "$ENV_FILE" != /* || "$ENV_FILE" != *staging* ]]; then
  echo "QINTOPIA_QIWE_IMAGE_STAGING_ENV_FILE must be an existing absolute path containing staging" >&2
  exit 1
fi

if [[ "$PHASE" != "upload" && "$PHASE" != "callback" ]]; then
  echo "QINTOPIA_QIWE_IMAGE_STAGING_PHASE must be upload or callback" >&2
  exit 1
fi

if [[ ! "$EXPECTED_DATABASE_HASH" =~ ^[0-9a-f]{64}$ ]]; then
  echo "QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256 must be a canonical SHA-256" >&2
  exit 1
fi

if ! python3 - "$WORK_ITEM_ID" <<'PY'
import sys
import uuid

uuid.UUID(sys.argv[1])
PY
then
  echo "QINTOPIA_QIWE_IMAGE_STAGING_WORK_ITEM_ID must be a UUID" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_DIR="${QINTOPIA_SIDECAR_SOURCE_DIR:-${MONOREPO_ROOT}/runtime/sidecar}"

cd "$MONOREPO_ROOT"

STAGING_ENV_KEYS=(
  QINTOPIA_QIWE_IMAGE_SEND_ENABLED
  QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY
  QINTOPIA_SIDECAR_DATABASE_URL
  QIWE_API_URL
  QIWE_TOKEN
  QIWE_GUID
  QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS
  QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS
  QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS
)

for key in "${STAGING_ENV_KEYS[@]}"; do
  unset "$key"
done

load_staging_env() {
  local line=""
  local line_number=0
  local key=""
  local value=""
  local loaded_keys="|"

  while IFS= read -r line || [[ -n "$line" ]]; do
    ((line_number += 1))
    if [[ "$line" =~ ^[[:space:]]*$ || "$line" =~ ^[[:space:]]*# ]]; then
      continue
    fi
    if [[ ! "$line" =~ ^([A-Z][A-Z0-9_]*)=(.*)$ ]]; then
      echo "staging env contains an invalid assignment at line ${line_number}" >&2
      return 1
    fi
    key="${BASH_REMATCH[1]}"
    value="${BASH_REMATCH[2]}"
    case " ${STAGING_ENV_KEYS[*]} " in
      *" ${key} "*) ;;
      *)
        echo "staging env contains an unsupported key at line ${line_number}" >&2
        return 1
        ;;
    esac
    case "$loaded_keys" in
      *"|${key}|"*)
        echo "staging env contains a duplicate key at line ${line_number}" >&2
        return 1
        ;;
    esac
    if [[ ${#value} -ge 2 ]]; then
      if [[ "${value:0:1}" == '"' && "${value: -1}" == '"' ]]; then
        value="${value:1:${#value}-2}"
      elif [[ "${value:0:1}" == "'" && "${value: -1}" == "'" ]]; then
        value="${value:1:${#value}-2}"
      fi
    fi
    export "${key}=${value}"
    loaded_keys+="${key}|"
  done <"$ENV_FILE"
}

load_staging_env

export QINTOPIA_QIWE_IMAGE_SEND_STAGING_APPROVAL=approved-staging-qiwe-image-send
export QINTOPIA_QIWE_IMAGE_STAGING_DATABASE_URL_SHA256="$EXPECTED_DATABASE_HASH"

if [[ "${QINTOPIA_QIWE_IMAGE_SEND_ENABLED:-}" != "1" ]]; then
  echo "QINTOPIA_QIWE_IMAGE_SEND_ENABLED=1 is required for a reviewed staging smoke" >&2
  exit 1
fi

if [[ -z "${QINTOPIA_SIDECAR_DATABASE_URL:-}" ]]; then
  echo "QINTOPIA_SIDECAR_DATABASE_URL is required in the staging env file" >&2
  exit 1
fi

readarray -t database_facts < <(printf '%s' "$QINTOPIA_SIDECAR_DATABASE_URL" | python3 -c '
import hashlib
import sys
from urllib.parse import unquote, urlparse

value = sys.stdin.read()
print(hashlib.sha256(value.encode("utf-8")).hexdigest())
print(unquote(urlparse(value).path).lstrip("/").lower())
')
if [[ "${database_facts[0]:-}" != "$EXPECTED_DATABASE_HASH" ]]; then
  echo "staging database URL hash does not match the approved command" >&2
  exit 1
fi
if [[ "${database_facts[1]:-}" != *staging* ]]; then
  echo "staging database name must contain staging" >&2
  exit 1
fi

if [[ -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
  BIN_CMD=("$QINTOPIA_SIDECAR_BIN")
elif [[ -x "${MONOREPO_ROOT}/sidecar/qintopia-message-sidecar" ]]; then
  BIN_CMD=("${MONOREPO_ROOT}/sidecar/qintopia-message-sidecar")
else
  BIN_CMD=(
    "${CARGO:-cargo}" run --quiet
    --manifest-path "$SIDECAR_DIR/Cargo.toml"
    --features qiwe-staging-adapter
    --
  )
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

assert_no_sensitive_output() {
  local label="$1"
  local file="$2"
  local forbidden=(
    '"request_id"'
    '"requestId"'
    '"file_aes_key"'
    '"fileAesKey"'
    '"fileAeskey"'
    '"file_id"'
    '"fileId"'
    '"file_md5"'
    '"fileMd5"'
    '"file_size"'
    '"fileSize"'
    '"filename"'
    '"fileName"'
    '"message_identifier"'
    '"target_group_id"'
    '"artifact_uri"'
  )

  local value_name
  for value_name in \
    QINTOPIA_SIDECAR_DATABASE_URL \
    QIWE_API_URL \
    QIWE_TOKEN \
    QIWE_GUID \
    QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS \
    QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS \
    QINTOPIA_OPERATIONS_ALLOWED_GROUP_IDS; do
    if [[ -n "${!value_name:-}" ]]; then
      forbidden+=("${!value_name}")
    fi
  done

  local value
  for value in "${forbidden[@]}"; do
    if [[ -n "$value" ]] && grep -Fq -- "$value" "$file"; then
      echo "${label} contains forbidden sensitive output" >&2
      exit 1
    fi
  done
}

run_sanitized() {
  local label="$1"
  local stdout_file="$2"
  local stderr_file="$3"
  shift 3
  if ! "$@" >"$stdout_file" 2>"$stderr_file"; then
    assert_no_sensitive_output "$label stdout" "$stdout_file"
    assert_no_sensitive_output "$label stderr" "$stderr_file"
    echo "${label} failed; inspect the protected staging runner locally" >&2
    exit 1
  fi
  assert_no_sensitive_output "$label stdout" "$stdout_file"
  assert_no_sensitive_output "$label stderr" "$stderr_file"
}

preflight_output="$tmp_dir/preflight.json"
preflight_stderr="$tmp_dir/preflight.stderr"
run_sanitized \
  "QiWe staging preflight" \
  "$preflight_output" \
  "$preflight_stderr" \
  "${BIN_CMD[@]}" qiwe-image-send-staging-preflight </dev/null
python3 - "$preflight_output" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

assert set(payload) == {
    "success", "worker", "action_status", "adapter_compiled", "send_enabled",
    "owner_approval_valid", "config_valid", "database_boundary_valid",
    "webhook_ready", "allowed_host_count", "media_allowed_host_count",
    "allowed_group_count", "missing_configuration", "protocol", "safe_for_chat",
    "limitations", "guardrails",
}
assert payload["success"] is True
assert payload["worker"] == "qiwe-image-send-adapter"
assert payload["action_status"] == "staging_adapter_ready"
assert payload["adapter_compiled"] is True
assert payload["send_enabled"] is True
assert payload["owner_approval_valid"] is True
assert payload["config_valid"] is True
assert payload["database_boundary_valid"] is True
assert payload["webhook_ready"] is True
assert payload["allowed_host_count"] > 0
assert payload["media_allowed_host_count"] > 0
assert payload["allowed_group_count"] == 1
assert payload["missing_configuration"] == []
assert payload["safe_for_chat"] is False
PY

phase_output="$tmp_dir/${PHASE}.json"
phase_stderr="$tmp_dir/${PHASE}.stderr"
if [[ "$PHASE" == "upload" ]]; then
  run_sanitized \
    "QiWe staging upload" \
    "$phase_output" \
    "$phase_stderr" \
    "${BIN_CMD[@]}" run-qiwe-image-send-worker --once --work-item-id "$WORK_ITEM_ID" --apply </dev/null
  python3 - "$phase_output" "$WORK_ITEM_ID" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

assert payload["success"] is True
assert payload["worker"] == "qiwe-image-send-adapter"
assert payload["phase"] == "upload"
assert payload["dry_run"] is False
assert payload["apply_requested"] is True
assert payload["action_status"] == "image_upload_accepted"
assert payload["work_item_id"] == sys.argv[2]
assert payload["external_upload_requested"] is True
assert payload["callback_received"] is False
assert payload["external_send_executed"] is False
assert payload["safe_for_chat"] is False
PY
  echo "QiWe image-send staging upload passed: awaiting one bounded owner-approved callback; no image send was executed"
  exit 0
fi

run_sanitized \
  "QiWe staging callback" \
  "$phase_output" \
  "$phase_stderr" \
  "${BIN_CMD[@]}" process-qiwe-image-send-callback --apply
python3 - "$phase_output" "$WORK_ITEM_ID" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

assert set(payload) == {
    "success", "dry_run", "apply_requested", "worker", "phase", "action_status",
    "work_item_id", "external_upload_requested", "callback_received",
    "callback_credential_schema", "callback_additional_field_count",
    "external_send_executed", "safe_for_chat", "limitations", "guardrails",
}
assert payload["success"] is True
assert payload["worker"] == "qiwe-image-send-adapter"
assert payload["phase"] == "callback"
assert payload["dry_run"] is False
assert payload["apply_requested"] is True
assert payload["action_status"] == "image_send_completed"
assert payload["work_item_id"] == sys.argv[2]
assert payload["external_upload_requested"] is False
assert payload["callback_received"] is True
assert payload["callback_credential_schema"] in {
    "fileAesKey+fileId+fileMd5+fileSize+filename",
    "fileAeskey+fileId+fileMd5+fileSize+filename",
    "fileAesKey+fileId+fileMd5+fileSize+fileName",
    "fileAeskey+fileId+fileMd5+fileSize+fileName",
}
assert isinstance(payload["callback_additional_field_count"], int)
assert payload["callback_additional_field_count"] >= 0
assert payload["external_send_executed"] is True
assert payload["safe_for_chat"] is False
PY

echo "QiWe image-send staging callback passed: one reviewed image send completed for the isolated allowlisted group"
