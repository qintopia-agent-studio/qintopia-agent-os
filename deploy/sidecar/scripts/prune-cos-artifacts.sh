#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  deploy/sidecar/scripts/prune-cos-artifacts.sh [--artifact-name <name>] [--artifact-type <type>] [--keep <count>] [--dry-run]

Prunes old Agent OS artifact directories from Tencent Cloud COS.

Required environment:
  TENCENT_COS_BUCKET      Full bucket name, including APPID.
  TENCENT_COS_REGION      COS region, for example ap-shanghai.
  TENCENT_COS_SECRET_ID   CAM SecretId for CI upload/prune.
  TENCENT_COS_SECRET_KEY  CAM SecretKey for CI upload/prune.

Optional environment:
  TENCENT_COS_PREFIX         Object prefix. Defaults to qintopia-agent-os.
  TENCENT_COS_BUCKET_ALIAS   COSCLI bucket alias. Defaults to qintopia-agent-os-artifacts.
  TENCENT_COS_ENDPOINT       Optional COS endpoint, for example cos.accelerate.myqcloud.com.
  TENCENT_COS_SESSION_TOKEN  Temporary key token.
  COSCLI_PATH                Existing coscli binary path.
  COSCLI_CONFIG_TIMEOUT_SECONDS     Per config command timeout. Defaults to 60.
  COSCLI_TRANSFER_TIMEOUT_SECONDS   Per list/delete command timeout. Defaults to 300.
  ARTIFACT_NAME                     Defaults to qintopia-message-sidecar-linux-x86_64-gnu.
  QINTOPIA_COS_ARTIFACT_TYPE        Artifact type: sidecar or deploy-bundle. Defaults to sidecar.
  QINTOPIA_COS_ARTIFACT_KEEP_COUNT  Defaults to 2.
USAGE
}

artifact_type="${QINTOPIA_COS_ARTIFACT_TYPE:-sidecar}"
artifact_name="${ARTIFACT_NAME:-}"
keep_count="${QINTOPIA_COS_ARTIFACT_KEEP_COUNT:-2}"
dry_run=0

while [[ $# -gt 0 ]]; do
  case "$1" in
    --artifact-name)
      artifact_name="${2:-}"
      shift 2
      ;;
    --artifact-type)
      artifact_type="${2:-}"
      shift 2
      ;;
    --keep)
      keep_count="${2:-}"
      shift 2
      ;;
    --dry-run)
      dry_run=1
      shift
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

case "$artifact_type" in
  sidecar)
    artifact_name="${artifact_name:-qintopia-message-sidecar-linux-x86_64-gnu}"
    ;;
  deploy-bundle)
    artifact_name="${artifact_name:-qintopia-agent-os-deploy-bundle}"
    ;;
  *)
    echo "--artifact-type must be sidecar or deploy-bundle" >&2
    exit 2
    ;;
esac

if [[ -z "$artifact_name" ]]; then
  echo "artifact name is required" >&2
  exit 2
fi

if ! [[ "$keep_count" =~ ^[1-9][0-9]*$ ]]; then
  echo "--keep must be a positive integer" >&2
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

require_env TENCENT_COS_BUCKET
require_env TENCENT_COS_REGION
require_env TENCENT_COS_SECRET_ID
require_env TENCENT_COS_SECRET_KEY

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

bucket_alias="${TENCENT_COS_BUCKET_ALIAS:-qintopia-agent-os-artifacts}"
prefix="${TENCENT_COS_PREFIX:-qintopia-agent-os}"
prefix="${prefix#/}"
prefix="${prefix%/}"
artifact_prefix="${prefix}/${artifact_type}"

run_coscli_capture() {
  local label="$1"
  shift
  local command_name="${1:-}"
  local timeout_seconds="${COSCLI_CONFIG_TIMEOUT_SECONDS:-60}"
  if [[ "$command_name" == "ls" || "$command_name" == "rm" ]]; then
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
    echo "Bucket alias: ${bucket_alias}" >&2
    echo "Artifact prefix: ${artifact_prefix}/" >&2
    echo "Credentials were not printed. Check CI COS list/delete permissions." >&2
    if [[ "$command_name" == "ls" ]]; then
      echo "COSCLI ls requires HeadBucket and GetBucket permissions on the bucket/prefix." >&2
    fi
    if [[ "$command_name" == "rm" ]]; then
      echo "COSCLI rm requires HeadBucket, HeadObject, GetBucket, DeleteObject, and DeleteMultipleObjects permissions on the bucket/prefix." >&2
    fi
    print_sanitized_coscli_output "$output_path"
    exit "$status"
  fi

  cat "$output_path"
}

config_path="${tmp_dir}/cos.yaml"
touch "$config_path"
config_auth_args=(
  --mode SecretKey
  --secret_id "$TENCENT_COS_SECRET_ID"
  --secret_key "$TENCENT_COS_SECRET_KEY"
)
if [[ -n "${TENCENT_COS_SESSION_TOKEN:-}" ]]; then
  config_auth_args+=(--session_token "$TENCENT_COS_SESSION_TOKEN")
fi

run_coscli_capture "configure COS SecretKey auth" config set \
  -c "$config_path" \
  --init-skip \
  --disable-log \
  "${config_auth_args[@]}" >/dev/null

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

run_coscli_capture "configure COS bucket ${TENCENT_COS_BUCKET}" config add \
  "${bucket_config_args[@]}" >/dev/null

list_output="$(
  run_coscli_capture "list ${artifact_type} artifact manifests" ls \
    "cos://${bucket_alias}/${artifact_prefix}/" \
    -r \
    --limit -1 \
    -c "$config_path" \
    --disable-log
)"

candidate_path="${tmp_dir}/candidates.tsv"
LIST_OUTPUT="$list_output" python3 - "$prefix" "$artifact_type" "$artifact_name" >"$candidate_path" <<'PY'
import re
import os
import sys

prefix, artifact_type, artifact_name = sys.argv[1:4]
escaped_prefix = re.escape(prefix.strip("/"))
escaped_type = re.escape(artifact_type.strip("/"))
escaped_artifact = re.escape(artifact_name)
key_re = re.compile(
    rf"({escaped_prefix}/{escaped_type}/([0-9a-f]{{40}})/{escaped_artifact}/artifact-manifest\.json)"
)

seen = {}
for line in os.environ.get("LIST_OUTPUT", "").splitlines():
    match = key_re.search(line)
    if not match:
        continue
    key = match.group(1)
    sha = match.group(2)
    parts = [part.strip() for part in line.split("|")]
    last_modified = parts[2] if len(parts) >= 3 else ""
    if sha not in seen or last_modified > seen[sha][0]:
        seen[sha] = (last_modified, key)

for sha, (last_modified, key) in seen.items():
    print(f"{last_modified}\t{sha}\t{key}")
PY

if [[ ! -s "$candidate_path" ]]; then
  echo "Found 0 COS ${artifact_type} artifact versions under cos://${bucket_alias}/${artifact_prefix}/; pruning 0."
  exit 0
fi

mapfile -t sorted_candidates < <(sort -r "$candidate_path")
total_count="${#sorted_candidates[@]}"
keep_lines=("${sorted_candidates[@]:0:keep_count}")
prune_lines=("${sorted_candidates[@]:keep_count}")

echo "Found ${total_count} COS ${artifact_type} artifact versions for ${artifact_name}; keeping ${#keep_lines[@]}, pruning ${#prune_lines[@]}."

for line in "${keep_lines[@]}"; do
  IFS=$'\t' read -r last_modified sha key <<<"$line"
  echo "Keep COS artifact sha=${sha} manifest=${key} last_modified=${last_modified:-unknown}"
done

for line in "${prune_lines[@]}"; do
  IFS=$'\t' read -r last_modified sha key <<<"$line"
  remote_dir="${artifact_prefix}/${sha}/${artifact_name}/"
  if [[ "$dry_run" -eq 1 ]]; then
    action_label="Would delete"
  else
    action_label="Deleting"
  fi
  echo "${action_label} COS artifact sha=${sha} prefix=cos://${bucket_alias}/${remote_dir} last_modified=${last_modified:-unknown}"
  if [[ "$dry_run" -eq 0 ]]; then
    run_coscli_capture "delete COS ${artifact_type} artifact ${sha}" rm \
      "cos://${bucket_alias}/${remote_dir}" \
      -r \
      -f \
      -c "$config_path" \
      --disable-log >/dev/null
  fi
done
