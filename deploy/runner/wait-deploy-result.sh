#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  deploy/runner/wait-deploy-result.sh --request-file <file>

Polls Tencent COS for the deploy result JSON referenced by a deploy request.

Required environment:
  TENCENT_COS_BUCKET
  TENCENT_COS_REGION
  TENCENT_COS_SECRET_ID
  TENCENT_COS_SECRET_KEY

Optional environment:
  TENCENT_COS_BUCKET_ALIAS
  TENCENT_COS_ENDPOINT
  TENCENT_COS_SESSION_TOKEN
  COSCLI_PATH
  DEPLOY_RESULT_TIMEOUT_SECONDS  Defaults to 900.
  DEPLOY_RESULT_POLL_SECONDS     Defaults to 15.
USAGE
}

request_file=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --request-file)
      request_file="${2:-}"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$request_file" || ! -f "$request_file" ]]; then
  echo "--request-file must point to an existing JSON file" >&2
  exit 2
fi

require_env() {
  if [[ -z "${!1:-}" ]]; then
    echo "$1 is required" >&2
    exit 2
  fi
}

positive_int_env() {
  local name="$1"
  local default_value="$2"
  local value="${!name:-$default_value}"
  if ! [[ "$value" =~ ^[1-9][0-9]*$ ]]; then
    echo "${name} must be a positive integer" >&2
    exit 2
  fi
  printf '%s\n' "$value"
}

print_sanitized_coscli_output() {
  python3 - "$1" <<'PY'
import os
import sys

path = sys.argv[1]
with open(path, encoding="utf-8", errors="replace") as fh:
    output = fh.read()

for name in (
    "TENCENT_COS_SECRET_ID",
    "TENCENT_COS_SECRET_KEY",
    "TENCENT_COS_SESSION_TOKEN",
):
    value = os.environ.get(name, "")
    if value:
        output = output.replace(value, "***")

if output.strip():
    sys.stderr.write("COSCLI output:\n")
    sys.stderr.write(output)
    if not output.endswith("\n"):
        sys.stderr.write("\n")
PY
}

require_env TENCENT_COS_BUCKET
require_env TENCENT_COS_REGION
require_env TENCENT_COS_SECRET_ID
require_env TENCENT_COS_SECRET_KEY

timeout_seconds="$(positive_int_env DEPLOY_RESULT_TIMEOUT_SECONDS 900)"
poll_seconds="$(positive_int_env DEPLOY_RESULT_POLL_SECONDS 15)"
bucket_alias="${TENCENT_COS_BUCKET_ALIAS:-qintopia-agent-os-artifacts}"

result_identity="$(python3 - "$request_file" <<'PY'
import json
import re
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    request = json.load(fh)

request_id = request.get("request_id", "")
result_key = request.get("cos", {}).get("result_key", "")
if not re.fullmatch(r"deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}", request_id):
    raise SystemExit("deploy request id is invalid")
expected = f"qintopia-agent-os/deploy-results/production/{request_id}.json"
if result_key != expected:
    raise SystemExit("deploy result key is invalid")

print(f"{request_id}\t{result_key}")
PY
)"
request_id="${result_identity%%$'\t'*}"
result_key="${result_identity#*$'\t'}"

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
    coscli_path="$(deploy/sidecar/scripts/install-coscli.sh --output "${tmp_dir}/coscli")"
  fi
fi

if [[ ! -x "$coscli_path" ]]; then
  echo "coscli is not executable: $coscli_path" >&2
  exit 2
fi

config_path="${tmp_dir}/cos.yaml"
touch "$config_path"
auth_args=(
  --mode SecretKey
  --secret_id "$TENCENT_COS_SECRET_ID"
  --secret_key "$TENCENT_COS_SECRET_KEY"
)
if [[ -n "${TENCENT_COS_SESSION_TOKEN:-}" ]]; then
  auth_args+=(--session_token "$TENCENT_COS_SESSION_TOKEN")
fi
"$coscli_path" config set \
  -c "$config_path" \
  "${auth_args[@]}" \
  --init-skip \
  --disable-log >/dev/null

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

deadline=$((SECONDS + timeout_seconds))
result_file="${tmp_dir}/deploy-result.json"
last_error="${tmp_dir}/last-coscli-error.log"

echo "Waiting for deploy result ${request_id} at cos://${bucket_alias}/${result_key}"

while (( SECONDS < deadline )); do
  rm -f "$result_file" "$last_error"
  set +e
  "$coscli_path" cp "cos://${bucket_alias}/${result_key}" "$result_file" \
    -c "$config_path" \
    --disable-log \
    2>"$last_error" \
    1>>"$last_error"
  status=$?
  set -e

  if [[ "$status" -eq 0 ]]; then
    result_status="$(python3 - "$result_file" "$request_file" "$request_id" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    result = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    request = json.load(fh)

if result.get("schema_version") != 1:
    raise SystemExit("deploy result schema_version is invalid")
if result.get("request_id") != sys.argv[3]:
    raise SystemExit("deploy result request_id mismatch")
if result.get("environment") != "production":
    raise SystemExit("deploy result environment mismatch")
for key in (
    "release_sha",
    "commit_sha",
    "runtime_sha",
    "runtime_artifact_profile",
    "deploy_bundle_sha",
):
    if result.get(key) != request.get(key):
        raise SystemExit(f"deploy result {key} mismatch")
if result.get("release_scope") != request.get("release_scope"):
    raise SystemExit("deploy result release_scope mismatch")
if result.get("restart_targets") != request.get("restart_targets"):
    raise SystemExit("deploy result restart_targets mismatch")

print(result.get("status", ""))
PY
)"
    case "$result_status" in
      succeeded|dry_run_succeeded)
        echo "Deploy result succeeded: ${result_status}"
        python3 -m json.tool "$result_file"
        exit 0
        ;;
      failed|rolled_back)
        echo "Deploy result failed: ${result_status}" >&2
        python3 -m json.tool "$result_file" >&2
        exit 1
        ;;
      *)
        echo "Unsupported deploy result status: ${result_status}" >&2
        python3 -m json.tool "$result_file" >&2
        exit 1
        ;;
    esac
  fi

  sleep "$poll_seconds"
done

echo "Timed out after ${timeout_seconds}s waiting for deploy result: ${request_id}" >&2
if [[ -f "$last_error" ]]; then
  print_sanitized_coscli_output "$last_error"
fi
exit 124
