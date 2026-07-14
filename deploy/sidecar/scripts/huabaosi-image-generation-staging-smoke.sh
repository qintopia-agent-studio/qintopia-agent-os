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

if [[ -z "$ENV_FILE" || ! -f "$ENV_FILE" || "$ENV_FILE" != /* || "$ENV_FILE" != *staging* ]]; then
  echo "QINTOPIA_HUABAOSI_IMAGE_STAGING_ENV_FILE must be an existing absolute path containing staging" >&2
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

set -a
# shellcheck disable=SC1090
source "$ENV_FILE"
set +a

if [[ "${QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED:-}" != "1" ]]; then
  echo "QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED=1 is required for a reviewed staging smoke" >&2
  exit 1
fi

if [[ -z "${QINTOPIA_SIDECAR_DATABASE_URL:-}" ]]; then
  echo "QINTOPIA_SIDECAR_DATABASE_URL is required in the staging env file" >&2
  exit 1
fi

database_name="$(printf '%s' "$QINTOPIA_SIDECAR_DATABASE_URL" | python3 -c '
import sys
from urllib.parse import unquote, urlparse

print(unquote(urlparse(sys.stdin.read()).path).lstrip("/"))
')"
if [[ "$database_name" != *staging* ]]; then
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
    --features huabaosi-staging-adapter
    --
  )
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

assert_no_sensitive_output() {
  local label="$1"
  local file="$2"
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
    QINTOPIA_HUABAOSI_MEDIA_UPLOAD_ENDPOINT \
    QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN \
    QIWE_TOKEN \
    QIWE_GUID; do
    if [[ -n "${!value_name:-}" ]]; then
      forbidden+=("${!value_name}")
    fi
  done

  local value
  for value in "${forbidden[@]}"; do
    if [[ -n "$value" ]] && grep -Fq -- "$value" "$file"; then
      echo "${label} leaked forbidden output" >&2
      exit 1
    fi
  done
}

preflight_output="$tmp_dir/preflight.json"
"${BIN_CMD[@]}" huabaosi-image-generation-preflight >"$preflight_output"
python3 - "$preflight_output" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

assert payload["success"] is True
assert payload["worker"] == "huabaosi-image-generation-worker"
assert payload["action_status"] == "adapter_config_ready"
assert payload["generation_enabled"] is True
assert payload["adapter_compiled"] is True
assert payload["config_valid"] is True
assert payload["missing_configuration"] == []
assert payload["safe_for_chat"] is False
PY
assert_no_sensitive_output "image adapter preflight" "$preflight_output"

worker_output="$tmp_dir/image-worker.json"
"${BIN_CMD[@]}" run-huabaosi-image-generation-worker \
  --once \
  --work-item-id "$WORK_ITEM_ID" \
  --apply >"$worker_output"
python3 - "$worker_output" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

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
assert payload["safe_for_chat"] is False

artifact_uri = payload["artifact_preview"].get("artifact_uri")
if artifact_uri is not None:
    from urllib.parse import urlparse
    import os

    public_base = os.environ["QINTOPIA_HUABAOSI_MEDIA_PUBLIC_BASE_URL"].rstrip("/")
    parsed = urlparse(artifact_uri)
    assert parsed.scheme == "https"
    assert not parsed.username and not parsed.password
    assert not parsed.query and not parsed.fragment
    assert artifact_uri.startswith(f"{public_base}/")
PY
assert_no_sensitive_output "image generation worker" "$worker_output"

echo "Huabaosi image staging smoke passed: one generated_image remains pending human review; no Feishu, QiWe, or publish adapter was called"
