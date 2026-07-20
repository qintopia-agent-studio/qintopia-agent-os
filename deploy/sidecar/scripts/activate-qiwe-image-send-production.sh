#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_ACTIVATION:-}" != "approved-production-qiwe-image-send" ]]; then
  echo "QiWe image-send production activation requires explicit owner approval" >&2
  exit 1
fi

ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-/etc/qintopia/message-sidecar.env}"
SYSTEMCTL="${SYSTEMCTL:-systemctl}"
PREFLIGHT_SERVICE="qintopia-agentos-qiwe-image-send-preflight.service"
WORKER_TIMER="qintopia-agentos-qiwe-image-send-worker.timer"

if [[ ! -f "$ENV_FILE" ]]; then
  echo "QiWe image-send production activation requires the persistent sidecar env file" >&2
  exit 1
fi

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for QiWe image-send production activation" >&2
  exit 1
fi

require_env_line() {
  local key="$1"
  local expected="$2"
  local count
  count="$(grep -Ec "^${key}=" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "QiWe image-send production activation requires exactly one ${key}" >&2
    exit 1
  fi
  if ! grep -Fxq "${key}=${expected}" "$ENV_FILE"; then
    echo "QiWe image-send production activation requires ${key}=${expected}" >&2
    exit 1
  fi
}

require_sha256_env_line() {
  local key="$1"
  local count
  count="$(grep -Ec "^${key}=[0-9a-f]{64}$" "$ENV_FILE" || true)"
  if [[ "$count" != "1" ]]; then
    echo "QiWe image-send production activation requires exactly one canonical ${key}" >&2
    exit 1
  fi
}

require_env_line "QINTOPIA_QIWE_IMAGE_SEND_ENABLED" "1"
require_env_line "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_APPROVAL" "approved-production-qiwe-image-send"
require_sha256_env_line "QINTOPIA_QIWE_IMAGE_SEND_PRODUCTION_DATABASE_URL_SHA256"

"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"
"$SYSTEMCTL" enable --now "$WORKER_TIMER"
"$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"
"$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"

echo "QiWe image-send production timer activated"
