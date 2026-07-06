#!/usr/bin/env bash
set -euo pipefail

STATE_DIR="${QINTOPIA_DEPLOY_RUNNER_STATE_DIR:-/var/lib/qintopia-agent-os-deploy}"
ENV_FILE="${QINTOPIA_COS_ENV_FILE:-/etc/qintopia/cos-artifacts.env}"
RUNNER="${QINTOPIA_DEPLOY_RUNNER_BIN:-/home/ubuntu/qintopia-agent-os-releases/current/deploy/runner/qintopia-agent-os-deploy-runner}"

usage() {
  cat <<'USAGE'
Usage:
  deploy/runner/poll-deploy-requests.sh

Polls Tencent COS for one pending production deploy request, runs the fixed deploy
runner, uploads the deploy result, and archives the consumed request locally.
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
pending_prefix="${prefix}/deploy-requests/production/pending/"

request_key="$("$coscli_path" ls "cos://${bucket_alias}/${pending_prefix}" \
  -c "$config_path" \
  --disable-log 2>/dev/null | awk '$NF ~ /\.json$/ {print $NF}' | sort | head -n 1)"

if [[ -z "$request_key" ]]; then
  echo "No pending deploy request."
  exit 0
fi

request_name="$(basename "$request_key")"
request_stem="${request_name%.json}"
if [[ ! "$request_stem" =~ ^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$ ]]; then
  request_stem="invalid-$(printf '%s' "$request_key" | sha256sum | awk '{print $1}' | cut -c1-16)"
fi
request_file="${STATE_DIR}/requests/pending/${request_name}"
"$coscli_path" cp "cos://${bucket_alias}/${request_key}" "$request_file" \
  -c "$config_path" \
  --disable-log

set +e
"$RUNNER" --request-file "$request_file"
runner_status=$?
set -e

request_id="$request_stem"
result_key="${prefix}/deploy-results/production/${request_id}.json"
parsed_identity="$(python3 - "$request_file" "$request_stem" "$prefix" <<'PY'
import json
import re
import sys

request_file, request_stem, prefix = sys.argv[1:4]
request_id_pattern = re.compile(r"^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$")
try:
    with open(request_file, encoding="utf-8") as fh:
        data = json.load(fh)
    request_id = data.get("request_id", "")
    result_key = data.get("cos", {}).get("result_key", "")
    expected_result_key = f"{prefix}/deploy-results/production/{request_id}.json"
    if (
        request_id == request_stem
        and request_id_pattern.fullmatch(request_id)
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

if [[ "$runner_status" -ne 0 && ! -f "$result_file" ]]; then
  python3 - "$result_file" "$request_id" <<'PY'
import json
import sys
from datetime import datetime, timezone

path, request_id = sys.argv[1:3]
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
    "error": "deploy request failed before promotion result was written",
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
  archive_key="${request_key/pending/processed}"
  archive_dir="${STATE_DIR}/requests/processed"
else
  archive_key="${request_key/pending/failed}"
  archive_dir="${STATE_DIR}/requests/failed"
fi

if "$coscli_path" cp "$request_file" "cos://${bucket_alias}/${archive_key}" \
  -c "$config_path" \
  --disable-log; then
  "$coscli_path" rm "cos://${bucket_alias}/${request_key}" \
    -c "$config_path" \
    --disable-log
else
  echo "failed to archive consumed request; leaving pending request in COS" >&2
fi

mv "$request_file" "${archive_dir}/${request_name}"

exit "$runner_status"
