#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  deploy/sidecar/scripts/github-app-git.sh -- <git arguments...>

Runs a git command against GitHub using a short-lived GitHub App installation token.
The token is passed through a temporary GIT_ASKPASS helper, not through the remote URL
or git config.

Required environment:
  GITHUB_APP_ID                 GitHub App id.
  GITHUB_APP_INSTALLATION_ID    Installation id for qintopia-agent-os.
  GITHUB_APP_PRIVATE_KEY_PATH   Path to the GitHub App private key PEM on the server.

Example:
  GITHUB_APP_ID=4214034 \
  GITHUB_APP_INSTALLATION_ID=144332887 \
  GITHUB_APP_PRIVATE_KEY_PATH=/etc/qintopia/github-app/qintopia-agent-os-deployer.pem \
  deploy/sidecar/scripts/github-app-git.sh -- \
    ls-remote https://github.com/qintopia-agent-studio/qintopia-agent-os.git refs/heads/master
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ "${1:-}" == "--" ]]; then
  shift
fi

if [[ $# -eq 0 ]]; then
  echo "git arguments are required" >&2
  usage >&2
  exit 2
fi

require_command() {
  if ! command -v "$1" >/dev/null 2>&1; then
    echo "missing required command: $1" >&2
    exit 2
  fi
}

require_command curl
require_command git
require_command jq
require_command openssl
require_command python3

if [[ -z "${GITHUB_APP_ID:-}" || -z "${GITHUB_APP_INSTALLATION_ID:-}" || -z "${GITHUB_APP_PRIVATE_KEY_PATH:-}" ]]; then
  echo "GITHUB_APP_ID, GITHUB_APP_INSTALLATION_ID, and GITHUB_APP_PRIVATE_KEY_PATH are required" >&2
  exit 2
fi

if [[ ! -r "$GITHUB_APP_PRIVATE_KEY_PATH" ]]; then
  echo "GitHub App private key is not readable: ${GITHUB_APP_PRIVATE_KEY_PATH}" >&2
  exit 2
fi

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT
chmod 700 "$tmp_dir"

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
payload = {
    "iat": now - 60,
    "exp": now + 540,
    "iss": app_id,
}
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

token_file="${tmp_dir}/installation-token"
jq -e -r '.token // empty' "$token_json" >"$token_file"
chmod 600 "$token_file"

permissions="$(jq -c '.permissions // {}' "$token_json")"
if ! jq -e '.permissions.contents == "read" or .permissions.contents == "write"' "$token_json" >/dev/null; then
  echo "GitHub App installation token is missing repository Contents permission: ${permissions}" >&2
  exit 2
fi

askpass="${tmp_dir}/git-askpass.sh"
cat >"$askpass" <<EOF
#!/usr/bin/env bash
case "\$1" in
  *Username*) printf '%s' 'x-access-token' ;;
  *Password*) cat "$token_file" ;;
  *) exit 1 ;;
esac
EOF
chmod 700 "$askpass"

export GIT_ASKPASS="$askpass"
export GIT_TERMINAL_PROMPT=0

git "$@"
