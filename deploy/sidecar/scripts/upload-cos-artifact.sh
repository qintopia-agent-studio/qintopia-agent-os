#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  deploy/sidecar/scripts/upload-cos-artifact.sh --artifact-dir <dir> [--sha <commit-sha>] [--artifact-type <type>]

Uploads an Agent OS artifact directory to Tencent Cloud COS.

Required environment:
  TENCENT_COS_BUCKET      Full bucket name, including APPID.
  TENCENT_COS_REGION      COS region, for example ap-shanghai.
  TENCENT_COS_SECRET_ID   CAM SecretId for CI upload.
  TENCENT_COS_SECRET_KEY  CAM SecretKey for CI upload.

Optional environment:
  TENCENT_COS_PREFIX         Object prefix. Defaults to qintopia-agent-os.
  TENCENT_COS_BUCKET_ALIAS   COSCLI bucket alias. Defaults to qintopia-agent-os-artifacts.
  TENCENT_COS_ENDPOINT       Optional COS endpoint, for example cos.accelerate.myqcloud.com.
  TENCENT_COS_SESSION_TOKEN  Temporary key token.
  COSCLI_PATH                Existing coscli binary path.
  COSCLI_CONFIG_TIMEOUT_SECONDS    Per config command timeout. Defaults to 60.
  COSCLI_TRANSFER_TIMEOUT_SECONDS  Per upload command timeout. Defaults to 300.
  COSCLI_PART_SIZE_MB              Per-part upload size. Defaults to 4.
  COSCLI_THREAD_NUM                Concurrent transfer threads. Defaults to 8.
  COSCLI_ERR_RETRY_NUM             Transfer error retry count. Defaults to 3.
  COSCLI_ERR_RETRY_INTERVAL_SECONDS  Transfer retry interval. Defaults to 3.
  TENCENT_COS_ARTIFACT_PAYLOAD        Object payload mode: bundle or raw. Defaults to bundle.
  QINTOPIA_COS_ARTIFACT_TYPE          Artifact type: sidecar or deploy-bundle. Defaults to sidecar.
USAGE
}

artifact_dir=""
sha=""
artifact_type="${QINTOPIA_COS_ARTIFACT_TYPE:-sidecar}"

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
    --artifact-type)
      artifact_type="${2:-}"
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
sidecar_binary_path="${artifact_dir}/qintopia-message-sidecar"
sidecar_bundle_path="${artifact_dir}/qintopia-message-sidecar.tar.gz"
deploy_bundle_path="${artifact_dir}/qintopia-agent-os-deploy-bundle.tar.gz"

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

print_sanitized_coscli_details() {
  python3 - "${PWD}/coscli_output" <<'PY'
import os
import sys
from pathlib import Path

root = Path(sys.argv[1])
if not root.exists():
    return_code = 0
    raise SystemExit(return_code)

secrets = [
    os.environ.get(name, "")
    for name in (
        "TENCENT_COS_SECRET_ID",
        "TENCENT_COS_SECRET_KEY",
        "TENCENT_COS_SESSION_TOKEN",
    )
]
secrets = [value for value in secrets if value]

files = [path for path in root.rglob("*") if path.is_file()]
files.sort(key=lambda path: path.stat().st_mtime, reverse=True)
if not files:
    raise SystemExit(0)

sys.stderr.write("COSCLI detail files:\n")
for path in files[:8]:
    relative = path.relative_to(root)
    sys.stderr.write(f"--- coscli_output/{relative} ---\n")
    try:
        output = path.read_text(encoding="utf-8", errors="replace")
    except OSError as exc:
        sys.stderr.write(f"failed to read detail file: {exc}\n")
        continue

    for value in secrets:
        output = output.replace(value, "***")

    max_chars = 12000
    if len(output) > max_chars:
        output = output[:max_chars] + "\n... truncated ...\n"
    sys.stderr.write(output)
    if output and not output.endswith("\n"):
        sys.stderr.write("\n")
PY
}

for file_path in "$manifest_path" "$checksum_path"; do
  if [[ ! -f "$file_path" ]]; then
    echo "artifact file not found: $file_path" >&2
    exit 1
  fi
done

case "$artifact_type" in
  sidecar)
    if [[ ! -f "$sidecar_binary_path" ]]; then
      echo "artifact file not found: $sidecar_binary_path" >&2
      exit 1
    fi
    ;;
  deploy-bundle)
    if [[ ! -f "$deploy_bundle_path" ]]; then
      echo "artifact file not found: $deploy_bundle_path" >&2
      exit 1
    fi
    ;;
  *)
    echo "--artifact-type must be sidecar or deploy-bundle" >&2
    exit 2
    ;;
esac

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
    print_sanitized_coscli_details
    exit "$status"
  fi
}

config_path="${tmp_dir}/cos.yaml"
touch "$config_path"
bucket_alias="${TENCENT_COS_BUCKET_ALIAS:-qintopia-agent-os-artifacts}"
prefix="${TENCENT_COS_PREFIX:-qintopia-agent-os}"
prefix="${prefix#/}"
prefix="${prefix%/}"
remote_base="${prefix}/${artifact_type}/${sha}/${artifact_name}"
payload_mode="${TENCENT_COS_ARTIFACT_PAYLOAD:-bundle}"
payload_files=(artifact-manifest.json SHA256SUMS)
case "$payload_mode" in
  bundle)
    if [[ "$artifact_type" == "sidecar" ]]; then
      if [[ ! -f "$sidecar_bundle_path" ]]; then
        echo "artifact bundle not found: $sidecar_bundle_path" >&2
        exit 1
      fi
      payload_files+=(qintopia-message-sidecar.tar.gz)
    else
      payload_files+=(qintopia-agent-os-deploy-bundle.tar.gz)
    fi
    ;;
  raw)
    if [[ "$artifact_type" != "sidecar" ]]; then
      echo "raw payload mode is supported only for sidecar artifacts" >&2
      exit 2
    fi
    payload_files+=(qintopia-message-sidecar)
    ;;
  *)
    echo "TENCENT_COS_ARTIFACT_PAYLOAD must be bundle or raw" >&2
    exit 2
    ;;
esac
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

run_coscli "configure COS bucket ${TENCENT_COS_BUCKET}" config add \
  "${bucket_config_args[@]}"

for file_name in "${payload_files[@]}"; do
  log "Uploading ${file_name} to cos://${bucket_alias}/${remote_base}/${file_name}"
  run_coscli "upload ${file_name}" cp \
    "${artifact_dir}/${file_name}" \
    "cos://${bucket_alias}/${remote_base}/${file_name}" \
    -c "$config_path" \
    --disable-log \
    "${transfer_args[@]}"
done

echo "Uploaded ${artifact_type} artifact to cos://${bucket_alias}/${remote_base}/"
