#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  deploy/runner/upload-deploy-request.sh --request-file <file>

Uploads a validated deploy request JSON to Tencent COS.

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

require_env TENCENT_COS_BUCKET
require_env TENCENT_COS_REGION
require_env TENCENT_COS_SECRET_ID
require_env TENCENT_COS_SECRET_KEY

request_key="$(python3 - "$request_file" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    print(json.load(fh)["cos"]["request_key"])
PY
)"
pointer_file=""
pointer_key="$(python3 - "$request_file" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    data = json.load(fh)

prefix = data["cos"]["prefix"].strip("/")
print(f"{prefix}/deploy-requests/production/current.json")
PY
)"

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT
chmod 700 "$tmp_dir"
pointer_file="${tmp_dir}/current.json"

python3 - "$request_file" "$pointer_file" "$request_key" <<'PY'
import json
import sys

request_path, pointer_path, request_key = sys.argv[1:4]
with open(request_path, encoding="utf-8") as fh:
    request = json.load(fh)

pointer = {
    "schema_version": 1,
    "environment": request["environment"],
    "repository": request["repository"],
    "request_id": request["request_id"],
    "request_key": request_key,
    "result_key": request["cos"]["result_key"],
    "commit_sha": request["commit_sha"],
    "release_sha": request["release_sha"],
    "created_at": request["created_at"],
    "expires_at": request["expires_at"],
    "dry_run": request["dry_run"],
}

with open(pointer_path, "w", encoding="utf-8") as fh:
    json.dump(pointer, fh, ensure_ascii=False, indent=2)
    fh.write("\n")
PY

coscli_path="${COSCLI_PATH:-}"
if [[ -z "$coscli_path" ]]; then
  if command -v coscli >/dev/null 2>&1; then
    coscli_path="$(command -v coscli)"
  else
    coscli_path="$(deploy/sidecar/scripts/install-coscli.sh --output "${tmp_dir}/coscli")"
  fi
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

"$coscli_path" cp "$request_file" "cos://${bucket_alias}/${request_key}" \
  -c "$config_path" \
  --disable-log

echo "Uploaded deploy request to cos://${bucket_alias}/${request_key}"

"$coscli_path" cp "$pointer_file" "cos://${bucket_alias}/${pointer_key}" \
  -c "$config_path" \
  --disable-log

echo "Uploaded deploy request pointer to cos://${bucket_alias}/${pointer_key}"
