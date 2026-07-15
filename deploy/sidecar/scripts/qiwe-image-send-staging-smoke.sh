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

assert_no_sensitive_text() {
  local label="$1"
  local text="$2"
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
    if [[ -n "$value" && "$text" == *"$value"* ]]; then
      echo "${label} contains forbidden sensitive output" >&2
      exit 1
    fi
  done
}

SANITIZED_OUTPUT=""

run_sanitized() {
  local label="$1"
  local output=""
  local status=0
  shift

  set +e
  output="$("$@" 2>&1)"
  status=$?
  set -e
  assert_no_sensitive_text "$label output" "$output"
  if [[ $status -ne 0 ]]; then
    echo "${label} failed; inspect the protected staging runner locally" >&2
    exit 1
  fi
  SANITIZED_OUTPUT="$output"
}

emit_sanitized_evidence() {
  local evidence_kind="$1"

  SANITIZED_EVIDENCE_PAYLOAD="$SANITIZED_OUTPUT" python3 - "$evidence_kind" <<'PY'
import json
import os
import sys

payload = json.loads(os.environ["SANITIZED_EVIDENCE_PAYLOAD"])
evidence_kind = sys.argv[1]

evidence = {
    "action_status": payload["action_status"],
    "safe_for_chat": payload["safe_for_chat"],
    "success": payload["success"],
    "worker": payload["worker"],
}

if evidence_kind == "preflight":
    evidence.update({
        "adapter_compiled": payload["adapter_compiled"],
        "allowed_group_count": payload["allowed_group_count"],
        "allowed_host_count": payload["allowed_host_count"],
        "config_valid": payload["config_valid"],
        "database_boundary_valid": payload["database_boundary_valid"],
        "media_allowed_host_count": payload["media_allowed_host_count"],
        "send_enabled": payload["send_enabled"],
        "webhook_ready": payload["webhook_ready"],
    })
else:
    evidence.update({
        "apply_requested": payload["apply_requested"],
        "callback_received": payload["callback_received"],
        "dry_run": payload["dry_run"],
        "external_send_executed": payload["external_send_executed"],
        "external_upload_requested": payload["external_upload_requested"],
        "phase": payload["phase"],
        "work_item_id": payload["work_item_id"],
    })
    if evidence_kind == "callback":
        evidence.update({
            "callback_additional_field_count": payload["callback_additional_field_count"],
            "callback_credential_schema": payload["callback_credential_schema"],
        })

print(
    "qiwe_image_send_staging_evidence="
    + json.dumps(evidence, ensure_ascii=True, separators=(",", ":"), sort_keys=True)
)
PY
}

run_sanitized \
  "QiWe staging preflight" \
  "${BIN_CMD[@]}" qiwe-image-send-staging-preflight </dev/null
printf '%s\n' "$SANITIZED_OUTPUT" | python3 -c '
import json
import sys

payload = json.load(sys.stdin)

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
'
emit_sanitized_evidence "preflight"

if [[ "$PHASE" == "upload" ]]; then
  run_sanitized \
    "QiWe staging upload" \
    "${BIN_CMD[@]}" run-qiwe-image-send-worker --once --work-item-id "$WORK_ITEM_ID" --apply </dev/null
  printf '%s\n' "$SANITIZED_OUTPUT" | python3 -c '
import json
import sys

payload = json.load(sys.stdin)

assert payload["success"] is True
assert payload["worker"] == "qiwe-image-send-adapter"
assert payload["phase"] == "upload"
assert payload["dry_run"] is False
assert payload["apply_requested"] is True
assert payload["action_status"] == "image_upload_accepted"
assert payload["work_item_id"] == sys.argv[1]
assert payload["external_upload_requested"] is True
assert payload["callback_received"] is False
assert payload["external_send_executed"] is False
assert payload["safe_for_chat"] is False
' "$WORK_ITEM_ID"
  emit_sanitized_evidence "upload"
  echo "QiWe image-send staging upload passed: awaiting one bounded owner-approved callback; no image send was executed"
  exit 0
fi

run_sanitized \
  "QiWe staging callback" \
  "${BIN_CMD[@]}" process-qiwe-image-send-callback --apply
printf '%s\n' "$SANITIZED_OUTPUT" | python3 -c '
import json
import sys

payload = json.load(sys.stdin)

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
assert payload["work_item_id"] == sys.argv[1]
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
' "$WORK_ITEM_ID"

emit_sanitized_evidence "callback"
echo "QiWe image-send staging callback passed: one reviewed image send completed for the isolated allowlisted group"
