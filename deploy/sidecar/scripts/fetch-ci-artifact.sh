#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  deploy/sidecar/scripts/fetch-ci-artifact.sh --sha <commit-sha> [--output-dir <dir>]

Downloads the sidecar CI artifact for an approved commit SHA and verifies SHA256SUMS.

Required environment:
  GITHUB_TOKEN  GitHub token with read access to Actions artifacts for this private repo.

Optional environment:
  GITHUB_REPOSITORY  Defaults to qintopia-agent-studio/qintopia-agent-os.
  GITHUB_WORKFLOW    Defaults to ci.yml.
  ARTIFACT_NAME      Defaults to qintopia-message-sidecar-linux-x86_64-gnu.
  ARTIFACT_TARGET    Defaults to linux-x86_64-gnu.
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

if [[ -z "${GITHUB_TOKEN:-}" ]]; then
  echo "GITHUB_TOKEN is required for private repository artifact download" >&2
  exit 2
fi

repo="${GITHUB_REPOSITORY:-qintopia-agent-studio/qintopia-agent-os}"
workflow="${GITHUB_WORKFLOW:-ci.yml}"
artifact_name="${ARTIFACT_NAME:-qintopia-message-sidecar-linux-x86_64-gnu}"
artifact_target="${ARTIFACT_TARGET:-linux-x86_64-gnu}"
output_dir="${output_dir:-/tmp/qintopia-agent-os-artifacts/${sha}}"

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 2
  fi
}

require_command curl
require_command jq
require_command unzip
require_command sha256sum

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT
chmod 700 "$tmp_dir"

curl_config="${tmp_dir}/github-curl.conf"
{
  printf '%s\n' 'connect-timeout = 20'
  printf '%s\n' 'max-time = 240'
  printf '%s\n' 'retry = 2'
  printf '%s\n' 'retry-delay = 2'
  printf '%s\n' 'fail'
  printf '%s\n' 'silent'
  printf '%s\n' 'show-error'
  printf '%s\n' 'location'
  printf '%s\n' 'http1.1'
  printf '%s\n' 'header = "Accept: application/vnd.github+json"'
  printf 'header = "Authorization: Bearer %s"\n' "$GITHUB_TOKEN"
  printf '%s\n' 'header = "X-GitHub-Api-Version: 2022-11-28"'
} >"$curl_config"
chmod 600 "$curl_config"
unset GITHUB_TOKEN

runs_json="${tmp_dir}/runs.json"
curl --config "$curl_config" \
  "https://api.github.com/repos/${repo}/actions/workflows/${workflow}/runs?head_sha=${sha}&status=success&per_page=20" \
  -o "$runs_json"

run_id="$(
  jq -r '.workflow_runs | sort_by(.created_at) | reverse | .[0].id // empty' "$runs_json"
)"

if [[ -z "$run_id" ]]; then
  echo "no successful ${workflow} run found for ${sha}" >&2
  exit 1
fi

artifacts_json="${tmp_dir}/artifacts.json"
curl --config "$curl_config" \
  "https://api.github.com/repos/${repo}/actions/runs/${run_id}/artifacts?per_page=100" \
  -o "$artifacts_json"

download_url="$(
  jq -r --arg name "$artifact_name" \
    '.artifacts[] | select(.name == $name and .expired == false) | .archive_download_url' \
    "$artifacts_json" | head -n 1
)"

if [[ -z "$download_url" ]]; then
  echo "artifact ${artifact_name} not found for run ${run_id}" >&2
  exit 1
fi

mkdir -p "$output_dir"
zip_path="${tmp_dir}/${artifact_name}.zip"
curl --config "$curl_config" "$download_url" -o "$zip_path"
unzip -o -q "$zip_path" -d "$output_dir"

(
  cd "$output_dir"
  test -f artifact-manifest.json
  test -f SHA256SUMS
  test -f qintopia-message-sidecar
  manifest_commit="$(jq -r '.commit_sha // empty' artifact-manifest.json)"
  manifest_artifact="$(jq -r '.artifact_name // empty' artifact-manifest.json)"
  manifest_target="$(jq -r '.target // empty' artifact-manifest.json)"
  manifest_binary_sha="$(
    jq -r \
      '.files[]? | select(.path == "qintopia-message-sidecar") | .sha256 // empty' \
      artifact-manifest.json | head -n 1
  )"
  checksum_binary_sha="$(awk '$2 == "qintopia-message-sidecar" {print $1}' SHA256SUMS)"
  if [[ "$manifest_commit" != "$sha" ]]; then
    echo "artifact manifest commit mismatch: got ${manifest_commit}, expected ${sha}" >&2
    exit 1
  fi
  if [[ "$manifest_artifact" != "$artifact_name" ]]; then
    echo "artifact manifest name mismatch: got ${manifest_artifact}, expected ${artifact_name}" >&2
    exit 1
  fi
  if [[ "$manifest_target" != "$artifact_target" ]]; then
    echo "artifact manifest target mismatch: got ${manifest_target}, expected ${artifact_target}" >&2
    exit 1
  fi
  if [[ -z "$manifest_binary_sha" || "$manifest_binary_sha" != "$checksum_binary_sha" ]]; then
    echo "artifact manifest checksum does not match SHA256SUMS" >&2
    exit 1
  fi
  sha256sum -c SHA256SUMS
  chmod 0755 qintopia-message-sidecar
)

echo "Downloaded ${artifact_name}"
echo "Run id: ${run_id}"
echo "Run URL: https://github.com/${repo}/actions/runs/${run_id}"
echo "Output: ${output_dir}"
