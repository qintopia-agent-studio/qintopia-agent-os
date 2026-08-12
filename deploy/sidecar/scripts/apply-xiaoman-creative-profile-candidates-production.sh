#!/usr/bin/env bash
set -euo pipefail

APPROVAL="approved-production-xiaoman-creative-profile-candidates"
ENV_FILE="/etc/qintopia/message-sidecar.env"
RELEASE_CURRENT="/home/ubuntu/qintopia-agent-os-releases/current"
PAYLOAD_FILE="/home/ubuntu/.local/state/qintopia-agentos/xiaoman-creative-profile-candidates/reviewed-payload.json"
PYTHON_BIN="/usr/bin/python3"

if [[ "${QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_APPLY:-}" != "$APPROVAL" ]]; then
  echo "Xiaoman creative-profile candidates apply requires explicit owner approval" >&2
  exit 2
fi

expected_sha256="${QINTOPIA_XIAOMAN_CREATIVE_PROFILE_CANDIDATES_PAYLOAD_SHA256:-}"
if [[ ! "$expected_sha256" =~ ^[0-9a-f]{64}$ ]]; then
  echo "Xiaoman creative-profile candidates payload SHA-256 is required" >&2
  exit 2
fi

if [[ ! -r "$PAYLOAD_FILE" ]]; then
  echo "fixed reviewed payload file is unavailable" >&2
  exit 2
fi

actual_sha256="$(sha256sum "$PAYLOAD_FILE" | cut -d ' ' -f 1)"
if [[ "$actual_sha256" != "$expected_sha256" ]]; then
  echo "reviewed payload SHA-256 mismatch" >&2
  exit 2
fi

apply_script="${RELEASE_CURRENT}/workflows/xiaoman-daily-case-report/apply_creative_profile_candidates.py"
if [[ ! -r "$apply_script" ]]; then
  echo "release-local creative-profile apply script is unavailable" >&2
  exit 2
fi

db_env_json="$("$PYTHON_BIN" - "$ENV_FILE" <<'PY'
import json
import re
import sys
from pathlib import Path

path = Path(sys.argv[1])
if not path.is_file():
    raise SystemExit("fixed sidecar env file is unavailable")

allowed = {
    "QINTOPIA_SIDECAR_DATABASE_URL",
    "QINTOPIA_MESSAGE_STORE_DATABASE_URL",
}
env = {key: "" for key in allowed}
line_re = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)=(.*)$")
for raw_line in path.read_text(encoding="utf-8").splitlines():
    line = raw_line.strip()
    if not line or line.startswith("#"):
        continue
    match = line_re.fullmatch(line)
    if not match:
        continue
    key, value = match.groups()
    if key not in allowed:
        continue
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        value = value[1:-1]
    env[key] = value

print(json.dumps(env, separators=(",", ":")))
PY
)"
QINTOPIA_SIDECAR_DATABASE_URL="$("$PYTHON_BIN" -c 'import json,sys; print(json.loads(sys.argv[1]).get("QINTOPIA_SIDECAR_DATABASE_URL",""))' "$db_env_json")"
QINTOPIA_MESSAGE_STORE_DATABASE_URL="$("$PYTHON_BIN" -c 'import json,sys; print(json.loads(sys.argv[1]).get("QINTOPIA_MESSAGE_STORE_DATABASE_URL",""))' "$db_env_json")"

exec env -i \
  PATH="/usr/bin:/bin" \
  QINTOPIA_SIDECAR_DATABASE_URL="${QINTOPIA_SIDECAR_DATABASE_URL:-}" \
  QINTOPIA_MESSAGE_STORE_DATABASE_URL="${QINTOPIA_MESSAGE_STORE_DATABASE_URL:-}" \
  "$PYTHON_BIN" "$apply_script" \
    --payload-json "$PAYLOAD_FILE" \
    --apply \
    --approval "$APPROVAL"
