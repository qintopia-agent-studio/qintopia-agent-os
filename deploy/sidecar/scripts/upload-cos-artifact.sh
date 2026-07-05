#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  deploy/sidecar/scripts/upload-cos-artifact.sh --artifact-dir <dir> [--sha <commit-sha>]

Uploads a sidecar artifact directory to Tencent Cloud COS.

Required environment:
  TENCENT_COS_BUCKET      Full bucket name, including APPID.
  TENCENT_COS_REGION      COS region, for example ap-shanghai.
  TENCENT_COS_SECRET_ID   CAM SecretId for CI upload.
  TENCENT_COS_SECRET_KEY  CAM SecretKey for CI upload.

Optional environment:
  TENCENT_COS_PREFIX         Object prefix. Defaults to qintopia-agent-os.
  TENCENT_COS_BUCKET_ALIAS   COSCLI bucket alias. Defaults to qintopia-agent-os-artifacts.
  TENCENT_COS_SESSION_TOKEN  Temporary key token.
  COSCLI_PATH                Existing coscli binary path.
  COSCLI_CONFIG_TIMEOUT_SECONDS    Per config command timeout. Defaults to 60.
  COSCLI_TRANSFER_TIMEOUT_SECONDS  Per upload command timeout. Defaults to 300.
  COSCLI_PART_SIZE_MB              Per-part upload size. Defaults to 4.
  COSCLI_THREAD_NUM                Concurrent transfer threads. Defaults to 8.
  COSCLI_ERR_RETRY_NUM             Transfer error retry count. Defaults to 3.
  COSCLI_ERR_RETRY_INTERVAL_SECONDS  Transfer retry interval. Defaults to 3.
USAGE
}

artifact_dir=""
sha=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact-dir)
      artifact_dir="${2:-}"
      shift 2
      ;;
    --sha)
      sha="${2:-}"
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

if [[ -z "$artifact_dir" ]]; then
  echo "--artifact-dir is required" >&2
  usage >&2
  exit 2
fi

artifact_dir="${artifact_dir%/}"
manifest_path="${artifact_dir}/artifact-manifest.json"
checksum_path="${artifact_dir}/SHA256SUMS"
binary_path="${artifact_dir}/qintopia-message-sidecar"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 2
  fi
}

require_env() {
  if [[ -z "${!1:-}" ]]; then
    echo "$1 is required" >&2
    exit 2
  fi
}

require_command python3
require_command sha256sum

require_env TENCENT_COS_BUCKET
require_env TENCENT_COS_REGION
require_env TENCENT_COS_SECRET_ID
require_env TENCENT_COS_SECRET_KEY

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

log() {
  printf '%s\n' "$*" >&2
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

for file_path in "$manifest_path" "$checksum_path" "$binary_path"; do
  if [[ ! -f "$file_path" ]]; then
    echo "artifact file not found: $file_path" >&2
    exit 1
  fi
done

artifact_name="$(basename "$artifact_dir")"
if [[ -z "$sha" ]]; then
  sha="$(python3 - "$manifest_path" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    print(json.load(fh).get("commit_sha", ""))
PY
)"
fi

if [[ -z "$sha" ]]; then
  echo "commit SHA is required or must exist in artifact-manifest.json" >&2
  exit 1
fi

(
  cd "$artifact_dir"
  sha256sum -c SHA256SUMS
)

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

run_coscli() {
  local label="$1"
  shift
  local command_name="${1:-}"
  local timeout_seconds="${COSCLI_CONFIG_TIMEOUT_SECONDS:-60}"
  if [[ "$command_name" == "cp" ]]; then
    timeout_seconds="${COSCLI_TRANSFER_TIMEOUT_SECONDS:-300}"
  fi
  if ! [[ "$timeout_seconds" =~ ^[1-9][0-9]*$ ]]; then
    echo "invalid COSCLI timeout for ${label}: ${timeout_seconds}" >&2
    exit 2
  fi

  set +e
  local output_path="${tmp_dir}/coscli-output.log"
  python3 - "$output_path" "$timeout_seconds" "$coscli_path" "$@" <<'PY'
import subprocess
import sys

output_path = sys.argv[1]
timeout_seconds = int(sys.argv[2])
command = sys.argv[3:]

with open(output_path, "wb") as output:
    try:
        completed = subprocess.run(
            command,
            stdout=output,
            stderr=subprocess.STDOUT,
            timeout=timeout_seconds,
            check=False,
        )
    except subprocess.TimeoutExpired:
        raise SystemExit(124)

raise SystemExit(completed.returncode)
PY
  local status=$?
  set -e

  if [[ "$status" -ne 0 ]]; then
    if [[ "$status" -eq 124 || "$status" -eq 137 ]]; then
      echo "COSCLI timed out after ${timeout_seconds}s during: ${label}" >&2
    else
      echo "COSCLI failed during: ${label}" >&2
    fi
    echo "Destination bucket alias: ${bucket_alias}" >&2
    echo "Destination object prefix: ${remote_base}/" >&2
    echo "Credentials were not printed. Check CI COS SecretId/SecretKey and CAM upload permissions." >&2
    echo "COSCLI upload commonly requires bucket probe, object write, and multipart upload permissions for this prefix." >&2
    print_sanitized_coscli_output "$output_path"
    exit "$status"
  fi
}

config_path="${tmp_dir}/cos.yaml"
touch "$config_path"
bucket_alias="${TENCENT_COS_BUCKET_ALIAS:-qintopia-agent-os-artifacts}"
prefix="${TENCENT_COS_PREFIX:-qintopia-agent-os}"
prefix="${prefix#/}"
prefix="${prefix%/}"
remote_base="${prefix}/sidecar/${sha}/${artifact_name}"
transfer_args=(
  --part-size "$(positive_int_env COSCLI_PART_SIZE_MB 4)"
  --thread-num "$(positive_int_env COSCLI_THREAD_NUM 8)"
  --err-retry-num "$(positive_int_env COSCLI_ERR_RETRY_NUM 3)"
  --err-retry-interval "$(positive_int_env COSCLI_ERR_RETRY_INTERVAL_SECONDS 3)"
)
config_auth_args=(
  --mode SecretKey
  --secret_id "$TENCENT_COS_SECRET_ID"
  --secret_key "$TENCENT_COS_SECRET_KEY"
)
if [[ -n "${TENCENT_COS_SESSION_TOKEN:-}" ]]; then
  config_auth_args+=(--session_token "$TENCENT_COS_SESSION_TOKEN")
fi

run_coscli "configure COS SecretKey auth" config set \
  -c "$config_path" \
  --init-skip \
  --disable-log \
  "${config_auth_args[@]}"

run_coscli "configure COS bucket ${TENCENT_COS_BUCKET}" config add \
  -b "$TENCENT_COS_BUCKET" \
  -r "$TENCENT_COS_REGION" \
  -a "$bucket_alias" \
  -c "$config_path" \
  --init-skip \
  --disable-log

for file_name in artifact-manifest.json SHA256SUMS qintopia-message-sidecar; do
  log "Uploading ${file_name} to cos://${bucket_alias}/${remote_base}/${file_name}"
  run_coscli "upload ${file_name}" cp \
    "${artifact_dir}/${file_name}" \
    "cos://${bucket_alias}/${remote_base}/${file_name}" \
    -c "$config_path" \
    --disable-log \
    "${transfer_args[@]}"
done

echo "Uploaded sidecar artifact to cos://${bucket_alias}/${remote_base}/"
