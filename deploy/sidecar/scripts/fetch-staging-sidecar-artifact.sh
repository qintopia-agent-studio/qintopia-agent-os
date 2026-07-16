#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  deploy/sidecar/scripts/fetch-staging-sidecar-artifact.sh --sha <commit-sha> [--release-root <dir>]

Downloads the staging-only sidecar GitHub Actions artifact, verifies its staging
manifest and checksums, and installs it under the fixed immutable staging release root.

Required environment:
  QINTOPIA_STAGING_SIDECAR_PROVISION_APPROVAL=approved-staging-sidecar-provision

Preferred GitHub App environment:
  GITHUB_APP_ID
  GITHUB_APP_INSTALLATION_ID
  GITHUB_APP_PRIVATE_KEY_PATH

Fallback environment:
  GITHUB_TOKEN  Token with read access to Actions artifacts for this private repo.

Optional environment:
  GITHUB_REPOSITORY  Defaults to qintopia-agent-studio/qintopia-agent-os.
  GITHUB_WORKFLOW    Defaults to artifacts.yml.
  GITHUB_API_MAX_TIME       Defaults to 240 seconds.
  GITHUB_DOWNLOAD_MAX_TIME  Defaults to 900 seconds.

Test-only:
  QINTOPIA_STAGING_SIDECAR_PROVISION_TEST_MODE=1
  --artifact-zip <zip>
USAGE
}

sha=""
release_root="/home/ubuntu/qintopia-agent-os-staging-releases"
artifact_zip=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --sha)
      sha="${2:-}"
      shift 2
      ;;
    --release-root)
      release_root="${2:-}"
      shift 2
      ;;
    --artifact-zip)
      artifact_zip="${2:-}"
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

test_mode="${QINTOPIA_STAGING_SIDECAR_PROVISION_TEST_MODE:-0}"
if [[ "$test_mode" != "0" && "$test_mode" != "1" ]]; then
  echo "QINTOPIA_STAGING_SIDECAR_PROVISION_TEST_MODE must be 0 or 1" >&2
  exit 2
fi

if [[ "${QINTOPIA_STAGING_SIDECAR_PROVISION_APPROVAL:-}" != "approved-staging-sidecar-provision" ]]; then
  echo "QINTOPIA_STAGING_SIDECAR_PROVISION_APPROVAL must be approved-staging-sidecar-provision" >&2
  exit 2
fi

if [[ ! "$sha" =~ ^[0-9a-f]{40}$ ]]; then
  echo "--sha must be a 40-character lowercase hex commit SHA" >&2
  exit 2
fi

fixed_release_root="/home/ubuntu/qintopia-agent-os-staging-releases"
if [[ "$test_mode" != "1" && "$release_root" != "$fixed_release_root" ]]; then
  echo "--release-root must be ${fixed_release_root}" >&2
  exit 2
fi
if [[ "$test_mode" != "1" && -n "$artifact_zip" ]]; then
  echo "--artifact-zip is test-only" >&2
  exit 2
fi

repo="${GITHUB_REPOSITORY:-qintopia-agent-studio/qintopia-agent-os}"
workflow="${GITHUB_WORKFLOW:-artifacts.yml}"
artifact_name="qintopia-message-sidecar-staging-linux-x86_64-gnu"
artifact_target="linux-x86_64-gnu"
github_api_max_time="${GITHUB_API_MAX_TIME:-240}"
github_download_max_time="${GITHUB_DOWNLOAD_MAX_TIME:-900}"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 2
  fi
}

require_command jq
require_command python3
require_command sha256sum
require_command tar
require_command unzip
if [[ "$test_mode" != "1" ]]; then
  require_command curl
  require_command openssl
fi

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT
chmod 700 "$tmp_dir"

get_github_token() {
  if [[ -n "${GITHUB_APP_ID:-}" || -n "${GITHUB_APP_INSTALLATION_ID:-}" || -n "${GITHUB_APP_PRIVATE_KEY_PATH:-}" ]]; then
    if [[ -z "${GITHUB_APP_ID:-}" || -z "${GITHUB_APP_INSTALLATION_ID:-}" || -z "${GITHUB_APP_PRIVATE_KEY_PATH:-}" ]]; then
      echo "GITHUB_APP_ID, GITHUB_APP_INSTALLATION_ID, and GITHUB_APP_PRIVATE_KEY_PATH must be set together" >&2
      exit 2
    fi
    if [[ ! -r "$GITHUB_APP_PRIVATE_KEY_PATH" ]]; then
      echo "GitHub App private key is not readable: ${GITHUB_APP_PRIVATE_KEY_PATH}" >&2
      exit 2
    fi
    jwt_path="${tmp_dir}/github-app.jwt"
    GITHUB_APP_ID="$GITHUB_APP_ID" \
      GITHUB_APP_PRIVATE_KEY_PATH="$GITHUB_APP_PRIVATE_KEY_PATH" \
      python3 - "$jwt_path" <<'PY'
import base64
import json
import os
import subprocess
import sys
import time

target = sys.argv[1]
app_id = os.environ["GITHUB_APP_ID"]
private_key_path = os.environ["GITHUB_APP_PRIVATE_KEY_PATH"]

def b64url(data: bytes) -> str:
    return base64.urlsafe_b64encode(data).rstrip(b"=").decode("ascii")

now = int(time.time())
header = {"alg": "RS256", "typ": "JWT"}
payload = {"iat": now - 60, "exp": now + 540, "iss": app_id}
signing_input = ".".join(
    [
        b64url(json.dumps(header, separators=(",", ":")).encode("utf-8")),
        b64url(json.dumps(payload, separators=(",", ":")).encode("utf-8")),
    ]
).encode("ascii")

signature = subprocess.check_output(
    ["openssl", "dgst", "-sha256", "-sign", private_key_path],
    input=signing_input,
)

with open(target, "w", encoding="utf-8") as fh:
    fh.write(signing_input.decode("ascii") + "." + b64url(signature))
PY
    app_curl_config="${tmp_dir}/github-app-curl.conf"
    {
      printf '%s\n' 'connect-timeout = 20'
      printf '%s\n' 'max-time = 120'
      printf '%s\n' 'retry = 2'
      printf '%s\n' 'retry-delay = 2'
      printf '%s\n' 'fail'
      printf '%s\n' 'silent'
      printf '%s\n' 'show-error'
      printf '%s\n' 'http1.1'
      printf '%s\n' 'header = "Accept: application/vnd.github+json"'
      printf 'header = "Authorization: Bearer %s"\n' "$(cat "$jwt_path")"
      printf '%s\n' 'header = "X-GitHub-Api-Version: 2022-11-28"'
    } >"$app_curl_config"
    chmod 600 "$app_curl_config"

    token_json="${tmp_dir}/installation-token.json"
    curl --config "$app_curl_config" \
      --request POST \
      "https://api.github.com/app/installations/${GITHUB_APP_INSTALLATION_ID}/access_tokens" \
      -o "$token_json"
    jq -e -r '.token // empty' "$token_json"
    return
  fi

  if [[ -n "${GITHUB_TOKEN:-}" ]]; then
    printf '%s\n' "$GITHUB_TOKEN"
    return
  fi

  echo "GitHub App credentials or GITHUB_TOKEN are required for private repository artifact download" >&2
  exit 2
}

artifact_dir="${tmp_dir}/artifact"
mkdir -p "$artifact_dir"

if [[ "$test_mode" == "1" ]]; then
  if [[ -z "$artifact_zip" || ! -f "$artifact_zip" ]]; then
    echo "--artifact-zip is required in test mode" >&2
    exit 2
  fi
  unzip -o -q "$artifact_zip" -d "$artifact_dir"
  run_id="local-test"
else
  github_api_token="$(get_github_token)"
  curl_config="${tmp_dir}/github-curl.conf"
  {
    printf '%s\n' 'connect-timeout = 20'
    printf 'max-time = %s\n' "$github_api_max_time"
    printf '%s\n' 'retry = 2'
    printf '%s\n' 'retry-delay = 2'
    printf '%s\n' 'fail'
    printf '%s\n' 'silent'
    printf '%s\n' 'show-error'
    printf '%s\n' 'location'
    printf '%s\n' 'http1.1'
    printf '%s\n' 'header = "Accept: application/vnd.github+json"'
    printf 'header = "Authorization: Bearer %s"\n' "$github_api_token"
    printf '%s\n' 'header = "X-GitHub-Api-Version: 2022-11-28"'
  } >"$curl_config"
  chmod 600 "$curl_config"

  download_curl_config="${tmp_dir}/github-download-curl.conf"
  {
    printf '%s\n' 'connect-timeout = 20'
    printf 'max-time = %s\n' "$github_download_max_time"
    printf '%s\n' 'retry = 5'
    printf '%s\n' 'retry-delay = 5'
    printf '%s\n' 'retry-all-errors'
    printf '%s\n' 'fail'
    printf '%s\n' 'silent'
    printf '%s\n' 'show-error'
    printf '%s\n' 'location'
    printf '%s\n' 'http1.1'
    printf '%s\n' 'continue-at = -'
    printf '%s\n' 'header = "Accept: application/vnd.github+json"'
    printf 'header = "Authorization: Bearer %s"\n' "$github_api_token"
    printf '%s\n' 'header = "X-GitHub-Api-Version: 2022-11-28"'
  } >"$download_curl_config"
  chmod 600 "$download_curl_config"
  unset GITHUB_TOKEN
  unset github_api_token

  runs_json="${tmp_dir}/runs.json"
  curl --config "$curl_config" \
    "https://api.github.com/repos/${repo}/actions/workflows/${workflow}/runs?head_sha=${sha}&status=success&per_page=20" \
    -o "$runs_json"

  run_id=""
  download_url=""
  while IFS= read -r candidate_run_id; do
    [[ -z "$candidate_run_id" ]] && continue
    artifacts_json="${tmp_dir}/artifacts-${candidate_run_id}.json"
    curl --config "$curl_config" \
      "https://api.github.com/repos/${repo}/actions/runs/${candidate_run_id}/artifacts?per_page=100" \
      -o "$artifacts_json"
    candidate_download_url="$(
      jq -r --arg name "$artifact_name" \
        '.artifacts[] | select(.name == $name and .expired == false) | .archive_download_url' \
        "$artifacts_json" | head -n 1
    )"
    if [[ -n "$candidate_download_url" ]]; then
      run_id="$candidate_run_id"
      download_url="$candidate_download_url"
      break
    fi
  done < <(jq -r '.workflow_runs | sort_by(.created_at) | reverse | .[].id' "$runs_json")

  if [[ -z "$download_url" ]]; then
    echo "staging artifact ${artifact_name} not found for successful ${workflow} run at ${sha}" >&2
    exit 1
  fi

  zip_path="${tmp_dir}/${artifact_name}.zip"
  curl --config "$download_curl_config" "$download_url" -o "$zip_path"
  unzip -o -q "$zip_path" -d "$artifact_dir"
fi

(
  cd "$artifact_dir"
  test -f artifact-manifest.json
  test -f SHA256SUMS
  test -f qintopia-message-sidecar
  test -f qintopia-message-sidecar.tar.gz
  sha256sum -c SHA256SUMS
  tar_listing="$(tar -tzf qintopia-message-sidecar.tar.gz)"
  if [[ "$tar_listing" != "qintopia-message-sidecar" ]]; then
    echo "staging sidecar bundle must contain only qintopia-message-sidecar" >&2
    exit 1
  fi
  python3 - "$sha" "$artifact_name" "$artifact_target" <<'PY'
import json
import sys

expected_sha, expected_artifact, expected_target = sys.argv[1:4]
with open("artifact-manifest.json", encoding="utf-8") as fh:
    manifest = json.load(fh)

if manifest.get("commit_sha") != expected_sha:
    raise SystemExit("artifact manifest commit mismatch")
if manifest.get("artifact_name") != expected_artifact:
    raise SystemExit("artifact manifest name mismatch")
if manifest.get("target") != expected_target:
    raise SystemExit("artifact manifest target mismatch")

validation = manifest.get("validation", {})
if validation.get("cargo_features") != [
    "huabaosi-staging-adapter",
    "qiwe-staging-adapter",
]:
    raise SystemExit("artifact manifest Cargo features are not approved for staging")
if validation.get("staging_only") is not True:
    raise SystemExit("artifact manifest staging_only must be true")
if validation.get("production_eligible") is not False:
    raise SystemExit("artifact manifest production_eligible must be false")

required = {
    "qintopia-message-sidecar",
    "qintopia-message-sidecar.tar.gz",
    "artifact-manifest.json",
}
checksums = {}
with open("SHA256SUMS", encoding="utf-8") as fh:
    for line in fh:
        parts = line.split()
        if len(parts) >= 2:
            checksums[parts[1]] = parts[0]
missing = required - set(checksums)
if missing:
    raise SystemExit(f"SHA256SUMS missing entries: {sorted(missing)}")

files = {item.get("path"): item.get("sha256") for item in manifest.get("files", [])}
for path in ("qintopia-message-sidecar", "qintopia-message-sidecar.tar.gz"):
    if files.get(path) != checksums.get(path):
        raise SystemExit(f"manifest checksum does not match SHA256SUMS for {path}")
PY
)

binary_sha="$(awk '$2 == "qintopia-message-sidecar" {print $1}' "${artifact_dir}/SHA256SUMS")"
release_root="${release_root%/}"
release_dir="${release_root}/${sha}"
sidecar_dir="${release_dir}/sidecar"
tmp_sidecar_dir="${release_dir}/.sidecar.$$"

STAGING_RELEASE_ROOT="$release_root" \
STAGING_RELEASE_DIR="$release_dir" \
STAGING_SIDECAR_DIR="$sidecar_dir" \
python3 - <<'PY'
import os
import stat

paths = [
    os.environ["STAGING_RELEASE_ROOT"],
    os.environ["STAGING_RELEASE_DIR"],
    os.environ["STAGING_SIDECAR_DIR"],
]

for path in paths:
    if not os.path.isabs(path):
        raise SystemExit(f"path is not absolute: {path}")
    if "staging" not in path:
        raise SystemExit(f"path is missing staging marker: {path}")

for path in paths:
    current = os.path.sep
    for part in path.strip(os.path.sep).split(os.path.sep):
        current = os.path.join(current, part)
        try:
            st = os.lstat(current)
        except FileNotFoundError:
            break
        if stat.S_ISLNK(st.st_mode):
            raise SystemExit(f"path component is a symlink: {current}")
        if not stat.S_ISDIR(st.st_mode):
            raise SystemExit(f"path component is not a directory: {current}")
        if st.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
            raise SystemExit(f"path component is group/world writable: {current}")
        if st.st_uid not in (0, os.geteuid()):
            raise SystemExit(f"path component has unexpected owner: {current}")
PY

if [[ -e "$sidecar_dir" ]]; then
  echo "staging sidecar directory already exists: ${sidecar_dir}" >&2
  exit 1
fi

mkdir -p "$release_dir"

STAGING_RELEASE_ROOT="$release_root" \
STAGING_RELEASE_DIR="$release_dir" \
python3 - <<'PY'
import os
import stat

for path in (os.environ["STAGING_RELEASE_ROOT"], os.environ["STAGING_RELEASE_DIR"]):
    st = os.lstat(path)
    if stat.S_ISLNK(st.st_mode):
        raise SystemExit(f"path component is a symlink: {path}")
    if not stat.S_ISDIR(st.st_mode):
        raise SystemExit(f"path component is not a directory: {path}")
    if st.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise SystemExit(f"path component is group/world writable: {path}")
    if st.st_uid not in (0, os.geteuid()):
        raise SystemExit(f"path component has unexpected owner: {path}")
PY

mkdir "$tmp_sidecar_dir"
cp "${artifact_dir}/qintopia-message-sidecar" "$tmp_sidecar_dir/qintopia-message-sidecar"
cp "${artifact_dir}/qintopia-message-sidecar.tar.gz" "$tmp_sidecar_dir/qintopia-message-sidecar.tar.gz"
cp "${artifact_dir}/artifact-manifest.json" "$tmp_sidecar_dir/artifact-manifest.json"
cp "${artifact_dir}/SHA256SUMS" "$tmp_sidecar_dir/SHA256SUMS"
chmod 0555 "$tmp_sidecar_dir/qintopia-message-sidecar"
chmod 0444 "$tmp_sidecar_dir/qintopia-message-sidecar.tar.gz" "$tmp_sidecar_dir/artifact-manifest.json" "$tmp_sidecar_dir/SHA256SUMS"
(
  cd "$tmp_sidecar_dir"
  sha256sum -c SHA256SUMS
)
chmod 0555 "$tmp_sidecar_dir"
mv "$tmp_sidecar_dir" "$sidecar_dir"
chmod 0555 "$release_dir"

echo "Provisioned ${artifact_name}"
echo "Run id: ${run_id}"
if [[ "$run_id" != "local-test" ]]; then
  echo "Run URL: https://github.com/${repo}/actions/runs/${run_id}"
fi
echo "Release SHA: ${sha}"
echo "Sidecar SHA256: ${binary_sha}"
echo "Sidecar path: ${sidecar_dir}/qintopia-message-sidecar"
