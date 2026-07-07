#!/usr/bin/env bash
set -euo pipefail

STATE_DIR="${QINTOPIA_DEPLOY_RUNNER_STATE_DIR:-/var/lib/qintopia-agent-os-deploy}"
ENV_FILE="${QINTOPIA_COS_ENV_FILE:-/etc/qintopia/cos-artifacts.env}"
RUNNER="${QINTOPIA_DEPLOY_RUNNER_BIN:-/home/ubuntu/qintopia-agent-os-releases/current/deploy/runner/qintopia-agent-os-deploy-runner}"

usage() {
  cat <<'USAGE'
Usage:
  deploy/runner/poll-deploy-requests.sh

Fetches the fixed production deploy request pointer from Tencent COS, runs the
referenced deploy request once, uploads the deploy result, and records local
idempotency state.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  source "$ENV_FILE"
fi

require_env() {
  if [[ -z "${!1:-}" ]]; then
    echo "$1 is required" >&2
    exit 2
  fi
}

require_env TENCENT_COS_BUCKET
require_env TENCENT_COS_REGION
require_env DEPLOY_REQUEST_SIGNING_KEY
require_env DEPLOY_REQUEST_SIGNING_KEY_ID

auth_mode="${TENCENT_COS_AUTH_MODE:-SecretKey}"
if [[ "$auth_mode" == "CvmRole" ]]; then
  require_env TENCENT_COS_CVM_ROLE_NAME
else
  require_env TENCENT_COS_SECRET_ID
  require_env TENCENT_COS_SECRET_KEY
fi

mkdir -p "${STATE_DIR}/requests/pending" "${STATE_DIR}/requests/processed" \
  "${STATE_DIR}/requests/failed" "${STATE_DIR}/results"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT
chmod 700 "$tmp_dir"

coscli_path="${COSCLI_PATH:-}"
if [[ -z "$coscli_path" ]]; then
  if command -v coscli >/dev/null 2>&1; then
    coscli_path="$(command -v coscli)"
  else
    coscli_path="$(/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/install-coscli.sh --output "${tmp_dir}/coscli")"
  fi
fi

config_path="${tmp_dir}/cos.yaml"
touch "$config_path"
if [[ "$auth_mode" == "CvmRole" ]]; then
  "$coscli_path" config set \
    --mode CvmRole \
    --cvm_role_name "$TENCENT_COS_CVM_ROLE_NAME" \
    -c "$config_path" \
    --init-skip \
    --disable-log >/dev/null
else
  auth_args=(--mode SecretKey --secret_id "$TENCENT_COS_SECRET_ID" --secret_key "$TENCENT_COS_SECRET_KEY")
  if [[ -n "${TENCENT_COS_SESSION_TOKEN:-}" ]]; then
    auth_args+=(--session_token "$TENCENT_COS_SESSION_TOKEN")
  fi
  "$coscli_path" config set \
    -c "$config_path" \
    --init-skip \
    --disable-log \
    "${auth_args[@]}" >/dev/null
fi

bucket_alias="${TENCENT_COS_BUCKET_ALIAS:-qintopia-agent-os-artifacts}"
bucket_config_args=(
  -b "$TENCENT_COS_BUCKET"
  -r "$TENCENT_COS_REGION"
  -a "$bucket_alias"
  -c "$config_path"
  --init-skip
  --disable-log
)
if [[ -n "${TENCENT_COS_ENDPOINT:-}" ]]; then
  bucket_config_args+=(-e "$TENCENT_COS_ENDPOINT")
fi
"$coscli_path" config add "${bucket_config_args[@]}" >/dev/null

prefix="qintopia-agent-os"
pointer_key="${prefix}/deploy-requests/production/current.json"
pointer_file="${STATE_DIR}/requests/current.json"

set +e
"$coscli_path" cp "cos://${bucket_alias}/${pointer_key}" "$pointer_file" \
  -c "$config_path" \
  --disable-log
pointer_status=$?
set -e
if [[ "$pointer_status" -ne 0 ]]; then
  echo "No deploy request pointer found or pointer download failed: ${pointer_key}" >&2
  exit "$pointer_status"
fi

pointer_identity="$(python3 - "$pointer_file" "$prefix" <<'PY'
import json
import re
import sys

pointer_file, prefix = sys.argv[1:3]
request_id_pattern = re.compile(r"^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$")
try:
    with open(pointer_file, encoding="utf-8") as fh:
        pointer = json.load(fh)
    request_id = pointer.get("request_id", "")
    request_key = pointer.get("request_key", "")
    result_key = pointer.get("result_key", "")
    expected_request_key = f"{prefix}/deploy-requests/production/requests/{request_id}.json"
    expected_result_key = f"{prefix}/deploy-results/production/{request_id}.json"
    if (
        pointer.get("schema_version") == 1
        and pointer.get("environment") == "production"
        and pointer.get("repository") == "qintopia-agent-studio/qintopia-agent-os"
        and request_id_pattern.fullmatch(request_id)
        and request_key == expected_request_key
        and result_key == expected_result_key
    ):
        print(f"{request_id}\t{request_key}\t{result_key}")
except Exception:
    pass
PY
)"

if [[ -z "$pointer_identity" ]]; then
  echo "deploy request pointer is invalid" >&2
  exit 2
fi

request_id="${pointer_identity%%$'\t'*}"
remaining_identity="${pointer_identity#*$'\t'}"
request_key="${remaining_identity%%$'\t'*}"
result_key="${remaining_identity#*$'\t'}"
request_name="${request_id}.json"
request_file="${STATE_DIR}/requests/pending/${request_name}"
"$coscli_path" cp "cos://${bucket_alias}/${request_key}" "$request_file" \
  -c "$config_path" \
  --disable-log

parsed_identity="$(python3 - "$request_file" "$request_id" "$prefix" "$request_key" <<'PY'
import json
import re
import sys

request_file, expected_id, prefix, actual_request_key = sys.argv[1:5]
request_id_pattern = re.compile(r"^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$")
try:
    with open(request_file, encoding="utf-8") as fh:
        data = json.load(fh)
    request_id = data.get("request_id", "")
    request_key = data.get("cos", {}).get("request_key", "")
    result_key = data.get("cos", {}).get("result_key", "")
    expected_request_key = f"{prefix}/deploy-requests/production/requests/{request_id}.json"
    expected_result_key = f"{prefix}/deploy-results/production/{request_id}.json"
    if (
        request_id == expected_id
        and request_id_pattern.fullmatch(request_id)
        and request_key == actual_request_key
        and request_key == expected_request_key
        and result_key == expected_result_key
    ):
        print(f"{request_id}\t{result_key}")
except Exception:
    pass
PY
)"
if [[ -n "$parsed_identity" ]]; then
  request_id="${parsed_identity%%$'\t'*}"
  result_key="${parsed_identity#*$'\t'}"
fi
result_file="${STATE_DIR}/results/${request_id}.json"

runner_status=0
fallback_error="deploy request failed before promotion result was written"
if [[ -z "$parsed_identity" ]]; then
  runner_status=2
  fallback_error="deploy request key or identity is invalid"
elif [[ -e "${STATE_DIR}/requests/processed/${request_name}" || -e "${STATE_DIR}/requests/failed/${request_name}" ]]; then
  runner_status=2
  fallback_error="deploy request was already consumed"
else
  set +e
  "$RUNNER" --request-file "$request_file"
  runner_status=$?
  set -e
fi

if [[ "$runner_status" -ne 0 && ! -f "$result_file" ]]; then
  python3 - "$result_file" "$request_id" "$fallback_error" <<'PY'
import json
import sys
from datetime import datetime, timezone

path, request_id, error = sys.argv[1:4]
now = datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
result = {
    "schema_version": 1,
    "request_id": request_id,
    "environment": "production",
    "status": "failed",
    "started_at": now,
    "finished_at": now,
    "release_sha": "0000000000000000000000000000000000000000",
    "previous_sha": "",
    "current_target": "",
    "restart_targets": [],
    "checks": [{"name": "deploy-request-validation", "status": "failed"}],
    "rollback": {"attempted": False, "status": "not_needed"},
    "error": error,
}
with open(path, "w", encoding="utf-8") as fh:
    json.dump(result, fh, ensure_ascii=False, indent=2)
    fh.write("\n")
PY
fi

if [[ -f "$result_file" ]]; then
  "$coscli_path" cp "$result_file" "cos://${bucket_alias}/${result_key}" \
    -c "$config_path" \
    --disable-log
fi

if [[ "$runner_status" -eq 0 ]]; then
  archive_dir="${STATE_DIR}/requests/processed"
else
  archive_dir="${STATE_DIR}/requests/failed"
fi

mv "$request_file" "${archive_dir}/${request_name}"

exit "$runner_status"
