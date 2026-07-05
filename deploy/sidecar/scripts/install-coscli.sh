#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  deploy/sidecar/scripts/install-coscli.sh [--output <path>]

Downloads Tencent Cloud COSCLI for Linux amd64 and verifies its SHA256 checksum.

Optional environment:
  COSCLI_DOWNLOAD_URL  Download URL for COSCLI.
  COSCLI_SHA256        Expected SHA256 checksum.
USAGE
}

output_path=""

while [[ $# -gt 0 ]]; do
  case "$1" in
    --output)
      output_path="${2:-}"
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

if [[ "$(uname -s)" != "Linux" || "$(uname -m)" != "x86_64" ]]; then
  echo "install-coscli.sh currently supports Linux x86_64 only" >&2
  exit 2
fi

download_url="${COSCLI_DOWNLOAD_URL:-https://cosbrowser.cloud.tencent.com/software/coscli/coscli-linux-amd64}"
expected_sha256="${COSCLI_SHA256:-7165f2ae16c5f7ac495864c963ca574a76e04ec72680d7bc8a8eee3234d8cf91}"

if [[ -z "$output_path" ]]; then
  output_dir="${RUNNER_TEMP:-/tmp}/qintopia-coscli"
  output_path="${output_dir}/coscli"
fi

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 2
  fi
}

require_command curl
require_command sha256sum

mkdir -p "$(dirname "$output_path")"
tmp_path="${output_path}.tmp"

curl -fsSL \
  --connect-timeout 20 \
  --max-time 240 \
  --retry 3 \
  --retry-delay 2 \
  "$download_url" \
  -o "$tmp_path"

printf '%s  %s\n' "$expected_sha256" "$tmp_path" | sha256sum -c -
chmod 0755 "$tmp_path"
mv "$tmp_path" "$output_path"

printf '%s\n' "$output_path"
