#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_ACTIVATION:-}" != "approved-production-xiaoman-daily-case-report-auto-publish" ]]; then
  echo "xiaoman daily case report auto-publish production activation requires explicit owner approval" >&2
  exit 1
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
OBSERVATION_SCRIPT="${SCRIPT_DIR}/xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh"
ENV_FILE="/etc/qintopia/message-sidecar.env"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
SHA256SUM="/usr/bin/sha256sum"
PYTHON_BIN="/usr/bin/python3"
PSQL_BIN="/usr/bin/psql"
SERVICE_NAME="qintopia-agentos-xiaoman-daily-case-report-auto-publish.service"
TIMER_NAME="qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer"

if [[ ! -x "$OBSERVATION_SCRIPT" ]]; then
  echo "xiaoman daily case report activation requires the release-local observation script" >&2
  exit 1
fi

if [[ ! -f "$ENV_FILE" ]]; then
  echo "xiaoman daily case report activation requires the persistent sidecar env file" >&2
  exit 1
fi

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for xiaoman daily case report activation" >&2
  exit 1
fi

if [[ ! -x "$SHA256SUM" ]]; then
  echo "sha256sum is required for xiaoman daily case report activation" >&2
  exit 1
fi
if [[ ! -x "$PYTHON_BIN" ]]; then
  echo "python3 is required for xiaoman daily case report activation" >&2
  exit 1
fi
if [[ ! -x "$PSQL_BIN" ]]; then
  echo "psql is required for xiaoman daily case report activation" >&2
  exit 1
fi
if ! "$PYTHON_BIN" - <<'PY' >/dev/null 2>&1; then
from PIL import Image, ImageDraw, ImageFont
PY
  echo "Pillow is required for xiaoman daily case report activation" >&2
  exit 1
fi

require_env_line() {
  local key="$1"
  local expected="$2"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "xiaoman daily case report activation requires exactly one ${key}" >&2
    exit 1
  fi
  if ! grep -Fxq "${key}=${expected}" "$ENV_FILE"; then
    echo "xiaoman daily case report activation requires ${key}=${expected}" >&2
    exit 1
  fi
}

require_present_env_line() {
  local key="$1"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "xiaoman daily case report activation requires exactly one ${key}" >&2
    exit 1
  fi
}

env_value() {
  local key="$1"
  grep -E "^${key}=" "$ENV_FILE" | cut -d= -f2-
}

env_line_value() {
  local key="$1"
  local value
  if ! value="$(python3 - "$ENV_FILE" "$key" <<'PY'
import re
import sys

path, expected_key = sys.argv[1:3]
assignment = re.compile(
    r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=[ \t]*(.*?)[ \t]*(?:#[^\"']*)?$"
)
seen = False

with open(path, encoding="utf-8") as fh:
    for lineno, raw in enumerate(fh, 1):
        line = raw.rstrip("\r\n")
        stripped = line.strip()
        if not stripped or stripped.startswith("#"):
            continue
        match = assignment.fullmatch(line)
        if not match:
            raise SystemExit(f"invalid env line {lineno}")
        key, value = match.groups()
        if key != expected_key:
            continue
        if seen:
            raise SystemExit(f"duplicate env key {expected_key}")
        seen = True
        value = value.strip()
        if (value.startswith('"') and value.endswith('"')) or (
            value.startswith("'") and value.endswith("'")
        ):
            value = value[1:-1]
        has_control = any(ord(ch) < 32 or ord(ch) == 127 for ch in value)
        if "$(" in value or "`" in value or has_control:
            raise SystemExit(f"unsafe env value for {expected_key}")
        print(value)

if not seen:
    raise SystemExit(f"missing env key {expected_key}")
PY
)"; then
    echo "xiaoman daily case report activation requires exactly one ${key}" >&2
    exit 1
  fi
  printf '%s' "$value"
}

require_env_value() {
  local key="$1"
  local expected="$2"
  local value
  value="$(env_line_value "$key")"
  if [[ "$value" != "$expected" ]]; then
    echo "xiaoman daily case report activation requires ${key}=${expected}" >&2
    exit 1
  fi
}

require_sha256_env_value() {
  local key="$1"
  local value
  value="$(env_line_value "$key")"
  if [[ ! "$value" =~ ^[0-9a-f]{64}$ ]]; then
    echo "xiaoman daily case report activation requires canonical ${key}" >&2
    exit 1
  fi
}

require_exact_allowlist() {
  local allowlist_key="$1"
  local value_key="$2"
  local label="$3"
  local allowlist
  local expected
  allowlist="$(env_line_value "$allowlist_key")"
  expected="$(env_line_value "$value_key")"
  if [[ "$allowlist" != "$expected" ]]; then
    echo "xiaoman daily case report activation requires exact ${label} allowlist" >&2
    exit 1
  fi
}

require_feishu_database_hash_match() {
  local database_url
  local actual_hash
  local qiwe_hash
  local feishu_hash
  database_url="$(env_line_value "QINTOPIA_SIDECAR_DATABASE_URL")"
  qiwe_hash="$(env_line_value "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256")"
  feishu_hash="$(env_line_value "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256")"
  actual_hash="$(printf '%s' "$database_url" | "$SHA256SUM")"
  actual_hash="${actual_hash%% *}"
  if [[ "$actual_hash" != "$qiwe_hash" || "$feishu_hash" != "$qiwe_hash" ]]; then
    echo "xiaoman daily case report activation database hash does not match the reviewed Feishu/QiWe boundary" >&2
    exit 1
  fi
}

cleanup_failed_activation() {
  "$SYSTEMCTL" disable --now "$TIMER_NAME" >/dev/null 2>&1 || true
  "$SYSTEMCTL" stop "$SERVICE_NAME" >/dev/null 2>&1 || true
  "$SYSTEMCTL" reset-failed "$SERVICE_NAME" >/dev/null 2>&1 || true
}

require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED" "1"
require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_APPROVAL" "approved-production-xiaoman-daily-case-report-auto-publish"
require_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_READ_THROUGH_ENABLE" "1"
require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_CHAT_ID"
require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_TARGET_GROUP_ID"
require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND"
case "$(env_value "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_STORAGE_BACKEND")" in
  feishu-base)
    require_env_value "QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED" "1"
    require_env_value "QINTOPIA_HUABAOSI_FEISHU_MIRROR_APPROVAL" "approved-huabaosi-feishu-artifact-mirror"
    require_env_value "QINTOPIA_HUABAOSI_FEISHU_PROFILE_ENV_PATH" "/home/ubuntu/.hermes/profiles/huabaosi/.env"
    require_env_value "QINTOPIA_HUABAOSI_FEISHU_SCHEMA_VERSION" "huabaosi-generated-image-v1"
    require_sha256_env_value "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256"
    require_sha256_env_value "QINTOPIA_HUABAOSI_FEISHU_DATABASE_URL_SHA256"
    require_present_env_line "QINTOPIA_SIDECAR_DATABASE_URL"
    require_present_env_line "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN"
    require_present_env_line "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS"
    require_present_env_line "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID"
    require_present_env_line "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS"
    require_feishu_database_hash_match
    require_exact_allowlist "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_BASE_TOKENS" "QINTOPIA_HUABAOSI_FEISHU_BASE_TOKEN" "Feishu Base token"
    require_exact_allowlist "QINTOPIA_HUABAOSI_FEISHU_ALLOWED_ARTIFACT_TABLE_IDS" "QINTOPIA_HUABAOSI_FEISHU_ARTIFACT_TABLE_ID" "Feishu artifact table"
    ;;
  https-public)
    require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_UPLOAD_ENDPOINT"
    require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_PUBLIC_BASE_URL"
    require_present_env_line "QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_MEDIA_ALLOWED_HOSTS"
    ;;
  *)
    echo "xiaoman daily case report activation requires a reviewed storage backend" >&2
    exit 1
    ;;
esac

"$SYSTEMCTL" enable "$TIMER_NAME"
"$SYSTEMCTL" restart "$TIMER_NAME"
if ! "$SYSTEMCTL" is-enabled --quiet "$TIMER_NAME"; then
  cleanup_failed_activation
  exit 1
fi
if ! "$SYSTEMCTL" is-active --quiet "$TIMER_NAME"; then
  cleanup_failed_activation
  exit 1
fi
if ! env -i \
  PATH="$PATH" \
  QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_OBSERVATION_ENABLE=1 \
  QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE=enabled \
  "$OBSERVATION_SCRIPT" >/dev/null; then
  cleanup_failed_activation
  exit 1
fi

echo "xiaoman daily case report auto-publish production timer enabled"
