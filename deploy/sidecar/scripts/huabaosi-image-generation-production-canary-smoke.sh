#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENABLE:-}" != "1" ]]; then
  echo "Huabaosi production canary skipped: set QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENABLE=1 to run one approved image" >&2
  exit 0
fi

if [[ "${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_APPROVAL:-}" != "approved-production-image-generation-canary" ]]; then
  echo "Huabaosi production canary requires explicit owner approval" >&2
  exit 1
fi

BRIEF_ARTIFACT_ID="${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_BRIEF_ARTIFACT_ID:-}"
EXPECTED_DATABASE_HASH="${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_DATABASE_URL_SHA256:-}"
EXPECTED_RELEASE_SHA="${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_RELEASE_SHA:-}"
EXPECTED_SIDECAR_HASH="${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_SIDECAR_SHA256:-}"
TEST_MODE="${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_TEST_MODE:-0}"
PRODUCTION_RELEASE_PARENT="/home/ubuntu/qintopia-agent-os-releases"
PRODUCTION_ENV_FILE="/etc/qintopia/message-sidecar.env"
PROVIDER_TIMER="qintopia-agentos-huabaosi-image-generation-worker.timer"
REVIEWER_ID="trainer"

if [[ ! "$EXPECTED_DATABASE_HASH" =~ ^[0-9a-f]{64}$ ]]; then
  echo "production canary database URL hash must be canonical SHA-256" >&2
  exit 1
fi
if [[ ! "$EXPECTED_RELEASE_SHA" =~ ^[0-9a-f]{40}$ ]]; then
  echo "production canary release SHA must be 40-character lowercase hex" >&2
  exit 1
fi
if [[ ! "$EXPECTED_SIDECAR_HASH" =~ ^[0-9a-f]{64}$ ]]; then
  echo "production canary sidecar hash must be canonical SHA-256" >&2
  exit 1
fi
if [[ "$TEST_MODE" != "0" && "$TEST_MODE" != "1" ]]; then
  echo "production canary test mode must be 0 or 1" >&2
  exit 1
fi
if ! python3 - "$BRIEF_ARTIFACT_ID" <<'PY'
import sys
import uuid

uuid.UUID(sys.argv[1])
PY
then
  echo "production canary brief artifact id must be a UUID" >&2
  exit 1
fi

SCRIPT_DIR="$(cd -P "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
RELEASE_ROOT="$(cd -P "${SCRIPT_DIR}/../../.." && pwd)"

if [[ "$TEST_MODE" == "0" ]]; then
  if [[ "$(id -u)" != "0" ]]; then
    echo "Huabaosi production canary must run as root" >&2
    exit 1
  fi
  if [[ "$RELEASE_ROOT" != "${PRODUCTION_RELEASE_PARENT}/${EXPECTED_RELEASE_SHA}" ]]; then
    echo "Huabaosi production canary must run from the approved immutable release" >&2
    exit 1
  fi
  if [[ -n "${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENV_FILE:-}" || -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
    echo "production canary path overrides are test-only" >&2
    exit 1
  fi
  ENV_FILE="$PRODUCTION_ENV_FILE"
  SIDECAR_BIN="${RELEASE_ROOT}/sidecar/qintopia-message-sidecar"
  SYSTEMCTL="systemctl"
else
  ENV_FILE="${QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_CANARY_ENV_FILE:-}"
  SIDECAR_BIN="${QINTOPIA_SIDECAR_BIN:-}"
  SYSTEMCTL="${SYSTEMCTL:-systemctl}"
  case "$ENV_FILE" in
    /private/tmp/*|/tmp/*|/private/var/folders/*|/var/folders/*) ;;
    *)
      echo "production canary test mode may read only a temporary fake env file" >&2
      exit 1
      ;;
  esac
fi

if [[ ! -f "$ENV_FILE" || ! -r "$ENV_FILE" ]]; then
  echo "production canary environment file is unavailable" >&2
  exit 1
fi
if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for Huabaosi production canary" >&2
  exit 1
fi
if ! "$SYSTEMCTL" cat "$PROVIDER_TIMER" >/dev/null 2>&1; then
  echo "Huabaosi production provider timer is not installed" >&2
  exit 1
fi
timer_enabled_state="$("$SYSTEMCTL" is-enabled "$PROVIDER_TIMER" 2>/dev/null || true)"
if [[ "$timer_enabled_state" != "disabled" ]]; then
  echo "Huabaosi production provider timer must be disabled during one-shot canary" >&2
  exit 1
fi
if "$SYSTEMCTL" is-active --quiet "$PROVIDER_TIMER" >/dev/null 2>&1; then
  echo "Huabaosi production provider timer must be inactive during one-shot canary" >&2
  exit 1
fi

verify_release_boundary() {
  readarray -t release_facts < <(python3 - "$RELEASE_ROOT" "$SIDECAR_BIN" "$ENV_FILE" "$EXPECTED_RELEASE_SHA" "$TEST_MODE" <<'PY'
import hashlib
import os
import stat
import sys

root, binary, env_file, expected_sha, test_mode = sys.argv[1:6]
if not all(os.path.isabs(path) for path in (root, binary, env_file)):
    print("path_not_absolute")
    raise SystemExit(0)
if test_mode == "0":
    if os.path.dirname(root) != "/home/ubuntu/qintopia-agent-os-releases":
        print("release_parent_mismatch")
        raise SystemExit(0)
    if os.path.basename(root) != expected_sha:
        print("release_sha_mismatch")
        raise SystemExit(0)
    if env_file != "/etc/qintopia/message-sidecar.env":
        print("env_path_mismatch")
        raise SystemExit(0)
    if os.path.commonpath([root, binary]) != root:
        print("binary_outside_release")
        raise SystemExit(0)

checks = ((root, "directory", 0o755), (os.path.dirname(binary), "directory", 0o755), (binary, "regular", 0o755))
for path, kind, required_mode in checks:
    try:
        metadata = os.lstat(path)
    except FileNotFoundError:
        print("release_path_missing")
        raise SystemExit(0)
    if stat.S_ISLNK(metadata.st_mode):
        print("release_path_symlink")
        raise SystemExit(0)
    if kind == "directory" and not stat.S_ISDIR(metadata.st_mode):
        print("release_path_type")
        raise SystemExit(0)
    if kind == "regular" and not stat.S_ISREG(metadata.st_mode):
        print("release_path_type")
        raise SystemExit(0)
    if test_mode == "0" and metadata.st_uid != 0:
        print("release_owner_mismatch")
        raise SystemExit(0)
    if stat.S_IMODE(metadata.st_mode) != required_mode:
        print("release_mode_mismatch")
        raise SystemExit(0)

env_metadata = os.lstat(env_file)
if stat.S_ISLNK(env_metadata.st_mode) or not stat.S_ISREG(env_metadata.st_mode):
    print("env_file_type")
    raise SystemExit(0)
if test_mode == "0" and env_metadata.st_uid != 0:
    print("env_owner_mismatch")
    raise SystemExit(0)
if env_metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
    print("env_file_writable")
    raise SystemExit(0)

digest = hashlib.sha256()
with open(binary, "rb") as handle:
    for chunk in iter(lambda: handle.read(1024 * 1024), b""):
        digest.update(chunk)
print(digest.hexdigest())
PY
)
  case "${release_facts[0]:-}" in
    "$EXPECTED_SIDECAR_HASH") ;;
    path_not_absolute|release_parent_mismatch|release_sha_mismatch|env_path_mismatch|binary_outside_release|release_path_missing|release_path_symlink|release_path_type|release_owner_mismatch|release_mode_mismatch|env_file_type|env_owner_mismatch|env_file_writable)
      echo "Huabaosi production canary release boundary is invalid" >&2
      exit 1
      ;;
    *)
      echo "Huabaosi production canary sidecar hash does not match" >&2
      exit 1
      ;;
  esac
}

verify_release_boundary

PRODUCTION_ENV_KEYS=(
  QINTOPIA_SIDECAR_DATABASE_URL
  QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS
  QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED
  QINTOPIA_HUABAOSI_IMAGE_PROVIDER
  QINTOPIA_HUABAOSI_IMAGE_MODEL
  QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL
  QINTOPIA_HUABAOSI_IMAGE_API_KEY
  QINTOPIA_HUABAOSI_IMAGE_HTTP_TIMEOUT_SECONDS
  QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND
  QINTOPIA_HUABAOSI_MEDIA_ALLOWED_HOSTS
  QINTOPIA_HUABAOSI_MEDIA_MAX_BYTES
  QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED
  QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL
  QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN
  QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS
  QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID
  QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS
  QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH
  QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION
)

for key in "${PRODUCTION_ENV_KEYS[@]}"; do
  unset "$key"
done

load_production_env() {
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
      echo "production environment contains an invalid assignment" >&2
      return 1
    fi
    key="${BASH_REMATCH[1]}"
    value="${BASH_REMATCH[2]}"
    case " ${PRODUCTION_ENV_KEYS[*]} " in
      *" ${key} "*) ;;
      *) continue ;;
    esac
    case "$loaded_keys" in
      *"|${key}|"*)
        echo "production environment contains a duplicate canary key" >&2
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

load_production_env

if [[ "${QINTOPIA_HUABAOSI_IMAGE_GENERATION_ENABLED:-}" != "1" ]]; then
  echo "Huabaosi production image generation must be enabled for canary" >&2
  exit 1
fi
if [[ "${QINTOPIA_HUABAOSI_IMAGE_STORAGE_BACKEND:-}" != "feishu-base" ]]; then
  echo "Huabaosi production canary requires Feishu Base primary storage" >&2
  exit 1
fi
if [[ "${QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED:-}" != "1" || "${QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL:-}" != "approved-huabaosi-feishu-artifact-mirror" ]]; then
  echo "Huabaosi production canary Feishu boundary is not approved" >&2
  exit 1
fi
if [[ -z "${QINTOPIA_SIDECAR_DATABASE_URL:-}" ]]; then
  echo "Huabaosi production canary database is not configured" >&2
  exit 1
fi

actual_database_hash="$(printf '%s' "$QINTOPIA_SIDECAR_DATABASE_URL" | python3 -c 'import hashlib,sys; print(hashlib.sha256(sys.stdin.read().encode()).hexdigest())')"
if [[ "$actual_database_hash" != "$EXPECTED_DATABASE_HASH" ]]; then
  echo "Huabaosi production canary database hash does not match" >&2
  exit 1
fi
if ! printf '%s' "${QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS:-}" | python3 -c '
import sys

reviewers = {value.strip() for value in sys.stdin.read().split(",") if value.strip()}
raise SystemExit(0 if sys.argv[1] in reviewers else 1)
' "$REVIEWER_ID"; then
  echo "trainer is not in the production reviewer allowlist" >&2
  exit 1
fi

CHILD_ENV=(
  "PATH=${PATH:-/usr/bin:/bin:/usr/sbin:/sbin}"
  "HOME=${HOME:-/root}"
  "TMPDIR=${TMPDIR:-/tmp}"
)
for key in SSL_CERT_FILE SSL_CERT_DIR; do
  if [[ -n "${!key:-}" ]]; then
    CHILD_ENV+=("${key}=${!key}")
  fi
done
for key in "${PRODUCTION_ENV_KEYS[@]}"; do
  CHILD_ENV+=("${key}=${!key:-}")
done
CHILD_ENV+=(
  "QINTOPIA_DEPLOYED_COMMIT_SHA=${EXPECTED_RELEASE_SHA}"
  "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_APPROVAL=approved-production-image-generation"
  "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_RELEASE_SHA=${EXPECTED_RELEASE_SHA}"
  "QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_DATABASE_URL_SHA256=${EXPECTED_DATABASE_HASH}"
  "QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA=${EXPECTED_RELEASE_SHA}"
  "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256=${EXPECTED_DATABASE_HASH}"
)

assert_no_sensitive_text() {
  local label="$1"
  local text="$2"
  local value_name=""
  local forbidden=(
    "tenant_access_token"
    "postgres://"
    "postgresql://"
  )
  for value_name in \
    QINTOPIA_SIDECAR_DATABASE_URL \
    QINTOPIA_HUABAOSI_IMAGE_API_BASE_URL \
    QINTOPIA_HUABAOSI_IMAGE_API_KEY \
    QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN \
    QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS \
    QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID \
    QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS \
    QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH; do
    if [[ -n "${!value_name:-}" ]]; then
      forbidden+=("${!value_name}")
    fi
  done
  local value=""
  for value in "${forbidden[@]}"; do
    if [[ -n "$value" && "$text" == *"$value"* ]]; then
      echo "${label} contains sensitive output" >&2
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
  verify_release_boundary
  set +e
  output="$(env -i "${CHILD_ENV[@]}" "$@" </dev/null 2>&1)"
  status=$?
  set -e
  assert_no_sensitive_text "$label" "$output"
  if [[ $status -ne 0 ]]; then
    echo "${label} failed; inspect protected production logs" >&2
    exit 1
  fi
  SANITIZED_OUTPUT="$output"
}

emit_evidence() {
  local phase="$1"
  shift
  python3 - "$phase" "$EXPECTED_RELEASE_SHA" "$EXPECTED_SIDECAR_HASH" "$EXPECTED_DATABASE_HASH" "$@" <<'PY'
import json
import sys

phase, release_sha, binary_hash, database_hash, *values = sys.argv[1:]
evidence = {
    "approved_database_url_sha256_matched": True,
    "approved_sidecar_sha256_matched": True,
    "database_url_sha256": database_hash,
    "phase": phase,
    "release_binary_verified": True,
    "release_sha": release_sha,
    "sidecar_binary_sha256": binary_hash,
    "success": True,
}
if phase == "preflight":
    evidence.update({"action_status": values[0], "timer_active": False})
elif phase == "brief_review":
    evidence.update({
        "action_status": values[0],
        "brief_artifact_id": values[1],
        "brief_work_item_id": values[2],
        "review_status": values[3],
        "reviewer_id": "trainer",
    })
elif phase == "request_intake":
    evidence.update({
        "action_status": values[0],
        "brief_artifact_id": values[1],
        "brief_work_item_id": values[2],
        "image_generation_work_item_id": values[3],
        "request_created": True,
    })
elif phase == "generation":
    evidence.update({
        "action_status": values[0],
        "artifact_id": values[1],
        "byte_size": int(values[3]),
        "content_hash": values[2],
        "height": int(values[5]),
        "image_generation_work_item_id": values[6],
        "mime_type": values[7],
        "review_status": values[8],
        "storage_backend": "feishu-base",
        "width": int(values[4]),
    })
elif phase == "revalidation":
    evidence.update({
        "action_status": values[0],
        "artifact_id": values[1],
        "byte_size": int(values[3]),
        "content_hash": values[2],
        "database_writes_executed": False,
        "external_calls_executed": True,
        "height": int(values[5]),
        "width": int(values[4]),
    })
else:
    raise AssertionError("unsupported canary evidence phase")
print("huabaosi_image_generation_production_canary_evidence=" + json.dumps(evidence, sort_keys=True, separators=(",", ":")))
PY
}

run_sanitized "image generation preflight" "$SIDECAR_BIN" huabaosi-image-generation-preflight
if ! preflight_status="$(printf '%s' "$SANITIZED_OUTPUT" | python3 -c '
import json,sys
data=json.load(sys.stdin)
assert data["success"] is True
assert data["action_status"] == "adapter_config_ready"
assert data["generation_enabled"] is True
assert data["adapter_compiled"] is True
assert data["adapter_mode"] == "production"
assert data["config_valid"] is True
assert data["missing_configuration"] == []
assert data["safe_for_chat"] is False
print(data["action_status"])
' 2>/dev/null)"; then
  echo "image generation preflight returned an invalid report" >&2
  exit 1
fi
emit_evidence preflight "$preflight_status"

review_payload="$(python3 - "$BRIEF_ARTIFACT_ID" "$REVIEWER_ID" <<'PY'
import json
import sys

print(json.dumps({
    "artifact_id": sys.argv[1],
    "reviewer_id": sys.argv[2],
    "decision": "approved",
    "expected_artifact_type": "poster_brief",
    "expected_review_status": "pending",
    "reason": "owner-approved Aliang production canary brief",
    "source": "huabaosi_production_canary",
}, separators=(",", ":")))
PY
)"
run_sanitized "poster brief review" "$SIDECAR_BIN" operations-artifact-review-decision --apply --payload-json "$review_payload"
if ! parsed_facts="$(printf '%s' "$SANITIZED_OUTPUT" | python3 -c '
import json,sys,uuid
data=json.load(sys.stdin)
assert data["success"] is True
assert data["dry_run"] is False
assert data["apply_requested"] is True
assert data["action_status"] == "review_recorded"
assert data["artifact_id"] == sys.argv[1]
uuid.UUID(data["work_item_id"])
assert data["artifact_type"] == "poster_brief"
assert data["previous_review_status"] == "pending"
assert data["review_status"] == "approved"
assert data["reviewer_id"] == sys.argv[2]
print(data["action_status"])
print(data["artifact_id"])
print(data["work_item_id"])
print(data["review_status"])
' "$BRIEF_ARTIFACT_ID" "$REVIEWER_ID" 2>/dev/null)"; then
  echo "poster brief review returned an invalid report" >&2
  exit 1
fi
mapfile -t review_facts <<<"$parsed_facts"
BRIEF_WORK_ITEM_ID="${review_facts[2]}"
emit_evidence brief_review "${review_facts[@]}"

run_sanitized "image request intake" "$SIDECAR_BIN" run-xiaoman-activity-image-generation-starter-worker --once --apply --work-item-id "$BRIEF_WORK_ITEM_ID"
if ! parsed_facts="$(printf '%s' "$SANITIZED_OUTPUT" | python3 -c '
import json,sys,uuid
data=json.load(sys.stdin)
assert data["success"] is True
assert data["action_status"] == "image_generation_requests_created"
assert data["requested_work_item_id"] == sys.argv[1]
assert data["created_count"] == 1
assert data["existing_count"] == 0
assert len(data["work_items"]) == 1
item=data["work_items"][0]
assert item["existing"] is False
assert item["work_item_type"] == "image_generation_request"
assert item["capability_key"] == "huabaosi.generate_image_asset"
assert item["parent_work_item_id"] == sys.argv[1]
uuid.UUID(item["work_item_id"])
print(data["action_status"])
print(item["work_item_id"])
' "$BRIEF_WORK_ITEM_ID" 2>/dev/null)"; then
  echo "image request intake returned an invalid report" >&2
  exit 1
fi
mapfile -t intake_facts <<<"$parsed_facts"
IMAGE_WORK_ITEM_ID="${intake_facts[1]}"
emit_evidence request_intake "${intake_facts[0]}" "$BRIEF_ARTIFACT_ID" "$BRIEF_WORK_ITEM_ID" "$IMAGE_WORK_ITEM_ID"

run_sanitized "image generation worker" "$SIDECAR_BIN" run-huabaosi-image-generation-worker --once --work-item-id "$IMAGE_WORK_ITEM_ID" --apply
if ! parsed_facts="$(printf '%s' "$SANITIZED_OUTPUT" | python3 -c '
import json,sys,uuid
data=json.load(sys.stdin)
assert data["success"] is True
assert data["action_status"] == "generated_image_created"
assert data["dry_run"] is False
assert data["apply_requested"] is True
assert data["work_item_id"] == sys.argv[1]
assert len(data["artifact_ids"]) == 1
artifact=data["artifact_preview"]
artifact_id=data["artifact_ids"][0]
uuid.UUID(artifact_id)
assert artifact["artifact_type"] == "generated_image"
assert artifact["review_status"] == "pending"
assert artifact["mime_type"] == "image/jpeg"
assert artifact["artifact_uri"] == "feishu-base://huabaosi-generated-image/" + artifact_id
assert artifact["width"] == 1024 and artifact["height"] == 1024
assert 0 < artifact["byte_size"] <= 10485760
assert artifact["content_hash"].startswith("sha256:") and len(artifact["content_hash"]) == 71
print(data["action_status"])
print(artifact_id)
print(artifact["content_hash"])
print(artifact["byte_size"])
print(artifact["width"])
print(artifact["height"])
print(artifact["mime_type"])
print(artifact["review_status"])
' "$IMAGE_WORK_ITEM_ID" 2>/dev/null)"; then
  echo "image generation worker returned an invalid report" >&2
  exit 1
fi
mapfile -t generation_facts <<<"$parsed_facts"
GENERATED_ARTIFACT_ID="${generation_facts[1]}"
emit_evidence generation "${generation_facts[0]}" "$GENERATED_ARTIFACT_ID" "${generation_facts[2]}" "${generation_facts[3]}" "${generation_facts[4]}" "${generation_facts[5]}" "$IMAGE_WORK_ITEM_ID" "${generation_facts[6]}" "${generation_facts[7]}"

run_sanitized "Feishu primary storage revalidation" "$SIDECAR_BIN" huabaosi-feishu-primary-storage-revalidate --artifact-id "$GENERATED_ARTIFACT_ID"
if ! parsed_facts="$(printf '%s' "$SANITIZED_OUTPUT" | python3 -c '
import json,sys
data=json.load(sys.stdin)
assert data["success"] is True
assert data["action_status"] == "feishu_primary_storage_revalidated"
assert data["artifact_id"] == sys.argv[1]
assert data["content_hash"] == sys.argv[2]
assert data["byte_size"] == int(sys.argv[3])
assert data["width"] == int(sys.argv[4]) and data["height"] == int(sys.argv[5])
assert data["external_calls_executed"] is True
assert data["database_writes_executed"] is False
assert data["sensitive_fields_redacted"] is True
print(data["action_status"])
print(data["artifact_id"])
print(data["content_hash"])
print(data["byte_size"])
print(data["width"])
print(data["height"])
' "$GENERATED_ARTIFACT_ID" "${generation_facts[2]}" "${generation_facts[3]}" "${generation_facts[4]}" "${generation_facts[5]}" 2>/dev/null)"; then
  echo "Feishu primary storage revalidation returned an invalid report" >&2
  exit 1
fi
mapfile -t revalidation_facts <<<"$parsed_facts"
emit_evidence revalidation "${revalidation_facts[@]}"

echo "Huabaosi production canary passed: one Feishu-backed JPEG remains pending human review; no generated-image approval, mirror, publish, QiWe, or send was executed"
