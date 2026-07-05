#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  deploy/sidecar/scripts/fetch-cos-artifact.sh --sha <commit-sha> [--output-dir <dir>]

Downloads a sidecar artifact from Tencent Cloud COS and verifies SHA256SUMS.

Required environment:
  TENCENT_COS_BUCKET  Full bucket name, including APPID.
  TENCENT_COS_REGION  COS region, for example ap-shanghai.

Authentication environment, choose one:
  TENCENT_COS_AUTH_MODE=CvmRole
  TENCENT_COS_CVM_ROLE_NAME=<role-name>

  or:

  TENCENT_COS_SECRET_ID=<secret-id>
  TENCENT_COS_SECRET_KEY=<secret-key>

Optional environment:
  TENCENT_COS_PREFIX         Object prefix. Defaults to qintopia-agent-os.
  TENCENT_COS_BUCKET_ALIAS   COSCLI bucket alias. Defaults to qintopia-agent-os-artifacts.
  TENCENT_COS_SESSION_TOKEN  Temporary key token.
  ARTIFACT_NAME              Defaults to qintopia-message-sidecar-linux-x86_64-gnu.
  ARTIFACT_TARGET            Defaults to linux-x86_64-gnu.
  COSCLI_PATH                Existing coscli binary path.
  COSCLI_CONFIG_TIMEOUT_SECONDS    Per config command timeout. Defaults to 60.
  COSCLI_TRANSFER_TIMEOUT_SECONDS  Per download command timeout. Defaults to 300.
USAGE
}

sha=""
output_dir=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --sha)
      sha="${2:-}"
      shift 2
      ;;
    --output-dir)
      output_dir="${2:-}"
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

if [[ -z "$sha" ]]; then
  echo "--sha is required" >&2
  usage >&2
  exit 2
fi

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

auth_mode="${TENCENT_COS_AUTH_MODE:-SecretKey}"
if [[ "$auth_mode" == "CvmRole" ]]; then
  require_env TENCENT_COS_CVM_ROLE_NAME
else
  require_env TENCENT_COS_SECRET_ID
  require_env TENCENT_COS_SECRET_KEY
fi

artifact_name="${ARTIFACT_NAME:-qintopia-message-sidecar-linux-x86_64-gnu}"
artifact_target="${ARTIFACT_TARGET:-linux-x86_64-gnu}"
output_dir="${output_dir:-/tmp/qintopia-agent-os-artifacts/${sha}}"
bucket_alias="${TENCENT_COS_BUCKET_ALIAS:-qintopia-agent-os-artifacts}"
prefix="${TENCENT_COS_PREFIX:-qintopia-agent-os}"
prefix="${prefix#/}"
prefix="${prefix%/}"
remote_base="${prefix}/sidecar/${sha}/${artifact_name}"

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
    echo "Source bucket alias: ${bucket_alias}" >&2
    echo "Source object prefix: ${remote_base}/" >&2
    echo "Credentials were not printed. Check server COS auth and object read permissions." >&2
    print_sanitized_coscli_output "$output_path"
    exit "$status"
  fi
}

config_path="${tmp_dir}/cos.yaml"
touch "$config_path"
config_auth_args=(--init-skip --disable-log)
if [[ "$auth_mode" == "CvmRole" ]]; then
  run_coscli "configure COS CVM role auth" config set \
    --mode CvmRole \
    --cvm_role_name "$TENCENT_COS_CVM_ROLE_NAME" \
    -c "$config_path" \
    --init-skip \
    --disable-log
else
  secret_key_auth_args=(
    --mode SecretKey
    --secret_id "$TENCENT_COS_SECRET_ID"
    --secret_key "$TENCENT_COS_SECRET_KEY"
  )
  if [[ -n "${TENCENT_COS_SESSION_TOKEN:-}" ]]; then
    secret_key_auth_args+=(--session_token "$TENCENT_COS_SESSION_TOKEN")
  fi
  run_coscli "configure COS SecretKey auth" config set \
    -c "$config_path" \
    --init-skip \
    --disable-log \
    "${secret_key_auth_args[@]}"
fi

run_coscli "configure COS bucket ${TENCENT_COS_BUCKET}" config add \
  -b "$TENCENT_COS_BUCKET" \
  -r "$TENCENT_COS_REGION" \
  -a "$bucket_alias" \
  -c "$config_path" \
  --init-skip \
  --disable-log

mkdir -p "$output_dir"
for file_name in artifact-manifest.json SHA256SUMS qintopia-message-sidecar; do
  log "Downloading ${file_name} from cos://${bucket_alias}/${remote_base}/${file_name}"
  run_coscli "download ${file_name}" cp \
    "cos://${bucket_alias}/${remote_base}/${file_name}" \
    "${output_dir}/${file_name}" \
    -c "$config_path" \
    --disable-log
done

(
  cd "$output_dir"
  test -f artifact-manifest.json
  test -f SHA256SUMS
  test -f qintopia-message-sidecar
  python3 - "$sha" "$artifact_name" "$artifact_target" <<'PY'
import json
import sys

expected_sha, expected_artifact, expected_target = sys.argv[1:4]
with open("artifact-manifest.json", encoding="utf-8") as fh:
    manifest = json.load(fh)

manifest_commit = manifest.get("commit_sha", "")
manifest_artifact = manifest.get("artifact_name", "")
manifest_target = manifest.get("target", "")
manifest_binary_sha = ""
for item in manifest.get("files", []):
    if item.get("path") == "qintopia-message-sidecar":
        manifest_binary_sha = item.get("sha256", "")
        break

checksum_binary_sha = ""
with open("SHA256SUMS", encoding="utf-8") as fh:
    for line in fh:
        parts = line.split()
        if len(parts) >= 2 and parts[1] == "qintopia-message-sidecar":
            checksum_binary_sha = parts[0]
            break

if manifest_commit != expected_sha:
    raise SystemExit(
        f"artifact manifest commit mismatch: got {manifest_commit}, expected {expected_sha}"
    )
if manifest_artifact != expected_artifact:
    raise SystemExit(
        f"artifact manifest name mismatch: got {manifest_artifact}, expected {expected_artifact}"
    )
if manifest_target != expected_target:
    raise SystemExit(
        f"artifact manifest target mismatch: got {manifest_target}, expected {expected_target}"
    )
if not manifest_binary_sha or manifest_binary_sha != checksum_binary_sha:
    raise SystemExit("artifact manifest checksum does not match SHA256SUMS")
PY
  sha256sum -c SHA256SUMS
  chmod 0755 qintopia-message-sidecar
)

echo "Downloaded ${artifact_name} from COS"
echo "Output: ${output_dir}"
