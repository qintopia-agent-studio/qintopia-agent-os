#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_ROLLBACK:-}" != "approved-production-huabaosi-feishu-artifact-mirror-rollback" ]]; then
  echo "Huabaosi Feishu mirror production rollback requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
ENV_FILE="/etc/qintopia/message-sidecar.env"
WORKER_SERVICE="qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.service"
WORKER_TIMER="qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer"

if ! command -v "$SYSTEMCTL" >/dev/null 2>&1; then
  echo "systemctl is required for Huabaosi Feishu mirror production rollback" >&2
  exit 1
fi

"$SYSTEMCTL" disable --now "$WORKER_TIMER"
"$SYSTEMCTL" stop "$WORKER_SERVICE" >/dev/null 2>&1 || true
"$SYSTEMCTL" reset-failed "$WORKER_SERVICE" >/dev/null 2>&1 || true

if [[ ! -f "$ENV_FILE" ]]; then
  echo "Huabaosi Feishu mirror timer stopped, but persistent disablement cannot be confirmed" >&2
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
  echo "Huabaosi Feishu mirror timer stopped, but persistent enablement is missing or ambiguous" >&2
  exit 1
fi
if [[ "${#enablement_values[@]}" -ne 1 || "${enablement_values[0]}" != "0" ]]; then
  echo "Huabaosi Feishu mirror timer stopped; disable the persistent mirror flag through the controlled configuration channel and rerun rollback" >&2
  exit 1
fi

echo "Huabaosi Feishu artifact mirror production timer disabled"
