#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HUABAOSI_IMAGE_STAGING_SMOKE_ENABLE:-}" != "1" ]]; then
  echo "Huabaosi image staging smoke skipped: set QINTOPIA_HUABAOSI_IMAGE_STAGING_SMOKE_ENABLE=1 to run one approved staging image generation" >&2
  exit 0
fi

if [[ "${QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL:-}" != "approved-staging-image-generation" ]]; then
  echo "QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL=approved-staging-image-generation is required" >&2
  exit 1
fi

ENV_FILE="${QINTOPIA_HUABAOSI_IMAGE_STAGING_ENV_FILE:-}"
WORK_ITEM_ID="${QINTOPIA_HUABAOSI_IMAGE_STAGING_WORK_ITEM_ID:-}"
EXPECTED_DATABASE_HASH="${QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256:-}"

if [[ -z "$ENV_FILE" || ! -f "$ENV_FILE" || "$ENV_FILE" != /* || "$ENV_FILE" != *staging* ]]; then
  echo "QINTOPIA_HUABAOSI_IMAGE_STAGING_ENV_FILE must be an existing absolute path containing staging" >&2
  exit 1
fi

if [[ ! "$EXPECTED_DATABASE_HASH" =~ ^[0-9a-f]{64}$ ]]; then
  echo "QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256 must be a canonical SHA-256" >&2
  exit 1
fi

if ! python3 - "$WORK_ITEM_ID" <<'PY'
import sys
import uuid

uuid.UUID(sys.argv[1])
PY
then
  echo "QINTOPIA_HUABAOSI_IMAGE_STAGING_WORK_ITEM_ID must be a UUID" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_DIR="${QINTOPIA_SIDECAR_SOURCE_DIR:-${MONOREPO_ROOT}/runtime/sidecar}"

cd "$MONOREPO_ROOT"

STAGING_ENV_KEYS=(
  QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED
  QINTOPIA_SIDECAR_DATABASE_URL
  QINTOPIA_HUABAOSI_IMAGE_PROVIDER
  QINTOPIA_HUABAOSI_IMAGE_MODEL
  QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL
  QINTOPIA_HUABAOSI_IMAGE_API_KEY
  QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND
  QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED
  QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL
  QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA
  QINTOPIA_DEPLOYED_COMMIT_SHA
  QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256
  QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN
  QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS
  QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID
  QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS
  QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH
  QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION
  QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS
  QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES
)

IGNORED_STAGING_ENV_KEYS=(
  QINTOPIA_QIWE_IMAGE_SEND_ENABLED
  QINTOPIA_QIWE_IMAGE_SEND_WEBHOOK_READY
  QIWE_API_URL
  QIWE_TOKEN
  QIWE_GUID
  QINTOPIA_QIWE_IMAGE_SEND_ALLOWED_HOSTS
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
    case "$loaded_keys" in
      *"|${key}|"*)
        echo "staging env contains a duplicate key at line ${line_number}" >&2
        return 1
        ;;
    esac
    case " ${STAGING_ENV_KEYS[*]} " in
      *" ${key} "*) ;;
      *)
        case " ${IGNORED_STAGING_ENV_KEYS[*]} " in
          *" ${key} "*)
            loaded_keys+="${key}|"
            continue
            ;;
          *)
            echo "staging env contains an unsupported key at line ${line_number}" >&2
            return 1
            ;;
        esac
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

export QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL=approved-staging-image-generation
export QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256="$EXPECTED_DATABASE_HASH"

if [[ "${QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED:-}" != "1" ]]; then
  echo "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=1 is required for a reviewed staging smoke" >&2
  exit 1
fi

if [[ "${QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND:-}" != "feishu-base" ]]; then
  echo "QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND=feishu-base is required for the reviewed Huabaosi staging smoke" >&2
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
if [[ "${QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256:-}" != "$EXPECTED_DATABASE_HASH" ]]; then
  echo "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256 must match the approved staging database hash" >&2
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
    --features huabaosi-staging-adapter
    --
  )
fi

CHILD_ENV=()

add_child_env() {
  local key="$1"
  local value="$2"
  CHILD_ENV+=("${key}=${value}")
}

add_child_env_if_set() {
  local key="$1"
  if [[ -n "${!key:-}" ]]; then
    CHILD_ENV+=("${key}=${!key}")
  fi
}

add_child_env PATH "${PATH:-/usr/bin:/bin:/usr/sbin:/sbin}"
add_child_env HOME "${HOME:-/tmp}"
add_child_env TMPDIR "${TMPDIR:-/tmp}"
add_child_env_if_set CARGO_HOME
add_child_env_if_set RUSTUP_HOME
add_child_env_if_set SSL_CERT_FILE
add_child_env_if_set SSL_CERT_DIR

for key in "${STAGING_ENV_KEYS[@]}"; do
  add_child_env "$key" "${!key:-}"
done
add_child_env QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL "$QINTOPIA_HUABAOSI_IMAGE_STAGING_APPROVAL"
add_child_env QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256 "$QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256"

assert_no_sensitive_text() {
  local label="$1"
  local text="$2"
  local forbidden=(
    "tenant_access_token"
    "message_id"
    "raw_chat"
    "base_token"
    "send_executed=true"
  )

  local value_name
  for value_name in \
    QINTOPIA_SIDECAR_DATABASE_URL \
    QINTOPIA_HUABAOSI_IMAGE_API_KEY \
    QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL \
    QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN \
    QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS \
    QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID \
    QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS \
    QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN \
    QIWE_TOKEN \
    QIWE_GUID; do
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
  output="$(env -i "${CHILD_ENV[@]}" "$@" 2>&1)"
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

  printf '%s' "$SANITIZED_OUTPUT" | python3 -c '
import json
import os
import sys

payload = json.load(sys.stdin)
evidence_kind = sys.argv[1]

evidence = {
    "action_status": payload["action_status"],
    "database_url_sha256": os.environ["QINTOPIA_HUABAOSI_IMAGE_STAGING_DATABASE_URL_SHA256"],
    "safe_for_chat": payload["safe_for_chat"],
    "success": payload["success"],
    "worker": payload["worker"],
}

if evidence_kind == "preflight":
    evidence.update({
        "adapter_compiled": payload["adapter_compiled"],
        "config_valid": payload["config_valid"],
        "generation_enabled": payload["generation_enabled"],
        "phase": "preflight",
        "storage_backend": os.environ["QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND"],
    })
else:
    artifact = payload["artifact_preview"]
    artifact_uri = artifact.get("artifact_uri")
    if not isinstance(artifact_uri, str) or not artifact_uri.startswith("feishu-base://"):
        raise AssertionError("generated artifact_uri must use the reviewed feishu-base storage boundary")
    evidence.update({
        "apply_requested": payload["apply_requested"],
        "artifact_count": len(payload["artifact_ids"]),
        "byte_size": artifact["byte_size"],
        "content_hash": artifact["content_hash"],
        "dry_run": payload["dry_run"],
        "height": artifact["height"],
        "mime_type": artifact["mime_type"],
        "phase": "generation",
        "review_status": artifact["review_status"],
        "storage_backend": "feishu-base",
        "work_item_id": os.environ["QINTOPIA_HUABAOSI_IMAGE_STAGING_WORK_ITEM_ID"],
        "width": artifact["width"],
    })

print(
    "huabaosi_image_generation_staging_evidence="
    + json.dumps(evidence, ensure_ascii=True, separators=(",", ":"), sort_keys=True)
)
' "$evidence_kind"
}

run_sanitized \
  "image adapter preflight" \
  "${BIN_CMD[@]}" huabaosi-image-generation-preflight </dev/null
printf '%s\n' "$SANITIZED_OUTPUT" | python3 -c '
import json
import sys

payload = json.load(sys.stdin)

assert payload["success"] is True
assert payload["worker"] == "huabaosi-image-generation-worker"
assert payload["action_status"] == "adapter_config_ready"
assert payload["generation_enabled"] is True
assert payload["adapter_compiled"] is True
assert payload["config_valid"] is True
assert payload["missing_configuration"] == []
assert payload["safe_for_chat"] is False
'
emit_sanitized_evidence "preflight"

run_sanitized \
  "image generation worker" \
  "${BIN_CMD[@]}" run-huabaosi-image-generation-worker \
  --once \
  --work-item-id "$WORK_ITEM_ID" \
  --apply </dev/null
printf '%s\n' "$SANITIZED_OUTPUT" | python3 -c '
import json
import sys

payload = json.load(sys.stdin)

assert payload["success"] is True
assert payload["worker"] == "huabaosi-image-generation-worker"
assert payload["dry_run"] is False
assert payload["apply_requested"] is True
assert payload["action_status"] == "generated_image_created"
assert len(payload["artifact_ids"]) == 1
assert payload["artifact_preview"]["artifact_type"] == "generated_image"
assert payload["artifact_preview"]["review_status"] == "pending"
assert payload["artifact_preview"]["mime_type"] == "image/jpeg"
assert payload["artifact_preview"]["width"] == 1024
assert payload["artifact_preview"]["height"] == 1024
assert payload["artifact_preview"]["byte_size"] > 0
content_hash = payload["artifact_preview"]["content_hash"]
assert content_hash.startswith("sha256:") and len(content_hash) == 71
artifact_uri = payload["artifact_preview"].get("artifact_uri")
assert isinstance(artifact_uri, str)
assert artifact_uri.startswith("feishu-base://")
assert payload["safe_for_chat"] is False
'
emit_sanitized_evidence "generation"

echo "Huabaosi image staging smoke passed: one generated_image remains pending human review; Feishu Base stored the final JPEG; no QiWe or publish adapter was called"
