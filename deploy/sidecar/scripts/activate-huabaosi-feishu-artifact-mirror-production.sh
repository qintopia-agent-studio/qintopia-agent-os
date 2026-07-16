#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ACTIVATION:-}" != "approved-production-huabaosi-feishu-artifact-mirror" ]]; then
  echo "Huabaosi Feishu mirror production activation requires explicit owner approval" >&2
  exit 1
fi

ENV_FILE="/etc/qintopia/message-sidecar.env"
PREFLIGHT_SERVICE="qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service"
WORKER_TIMER="qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer"
SYSTEMCTL="${SYSTEMCTL:-systemctl}"

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for Huabaosi Feishu mirror production activation" >&2
  exit 1
fi

if [[ ! -f "$ENV_FILE" ]]; then
  echo "Huabaosi Feishu mirror production activation requires the persistent sidecar env file" >&2
  exit 1
fi

enablement_values=()
enablement_assignment_count=0
enablement_assignment_invalid=0
while IFS= read -r line; do
  if [[ "$line" =~ ^[[:space:]]*(export[[:space:]]+)?QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED[[:space:]]*= ]]; then
    enablement_assignment_count=$((enablement_assignment_count + 1))
    if [[ "$line" =~ ^[[:space:]]*(export[[:space:]]+)?QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED[[:space:]]*=[[:space:]]*([^#[:space:]]+)[[:space:]]*(#.*)?$ ]]; then
      value="${BASH_REMATCH[2]}"
      value="${value%\"}"
      value="${value#\"}"
      value="${value%\'}"
      value="${value#\'}"
      enablement_values+=("$value")
    else
      enablement_assignment_invalid=1
    fi
  fi
done <"$ENV_FILE"

if [[ "$enablement_assignment_invalid" == "1" || "$enablement_assignment_count" -ne 1 ]]; then
  echo "Huabaosi Feishu mirror production activation requires exactly one persistent enablement flag" >&2
  exit 1
fi
if [[ "${#enablement_values[@]}" -ne 1 || "${enablement_values[0]}" != "1" ]]; then
  echo "Huabaosi Feishu mirror production activation requires QINTOPIA_HUABAOSI_FEISHU_MIRROR_ENABLED=1" >&2
  exit 1
fi

"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"
"$SYSTEMCTL" enable --now "$WORKER_TIMER"
"$SYSTEMCTL" is-enabled --quiet "$WORKER_TIMER"
"$SYSTEMCTL" is-active --quiet "$WORKER_TIMER"

echo "Huabaosi Feishu artifact mirror production timer activated"
