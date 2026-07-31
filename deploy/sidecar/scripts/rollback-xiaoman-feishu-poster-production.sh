#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_FEISHU_POSTER_PRODUCTION_ROLLBACK:-}" != "approved-production-xiaoman-feishu-poster-return-rollback" ]]; then
  echo "Xiaoman Feishu poster production rollback requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
ENV_FILE="/etc/qintopia/message-sidecar.env"
DELIVERY_SERVICE="qintopia-agentos-xiaoman-feishu-poster-delivery.service"
DELIVERY_TIMER="qintopia-agentos-xiaoman-feishu-poster-delivery.timer"
STARTER_TIMER="qintopia-agentos-xiaoman-poster-notification-starter.timer"
CALLBACK_SERVICE="qintopia-agentos-xiaoman-poster-review-callback.service"
INTAKE_SERVICE="qintopia-agentos-operations-intake.service"

if [[ ! -x "$SYSTEMCTL" ]]; then
  echo "systemctl is required for Xiaoman Feishu poster rollback" >&2
  exit 1
fi

"$SYSTEMCTL" disable --now "$DELIVERY_TIMER"
"$SYSTEMCTL" stop "$DELIVERY_SERVICE" >/dev/null 2>&1 || true
"$SYSTEMCTL" reset-failed "$DELIVERY_SERVICE" >/dev/null 2>&1 || true
"$SYSTEMCTL" disable --now "$STARTER_TIMER"
"$SYSTEMCTL" disable --now "$CALLBACK_SERVICE"
"$SYSTEMCTL" disable --now "$INTAKE_SERVICE"

if [[ ! -f "$ENV_FILE" ]]; then
  echo "Xiaoman poster services stopped, but persistent disablement cannot be confirmed" >&2
  exit 1
fi

values=()
assignment_count=0
invalid_assignment=0
while IFS= read -r line; do
  if [[ "$line" =~ ^[[:space:]]*(export[[:space:]]+)?QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED[[:space:]]*= ]]; then
    assignment_count=$((assignment_count + 1))
    if [[ "$line" =~ ^[[:space:]]*(export[[:space:]]+)?QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED[[:space:]]*=[[:space:]]*([^#[:space:]]+)[[:space:]]*(#.*)?$ ]]; then
      value="${BASH_REMATCH[2]}"
      value="${value%\"}"
      value="${value#\"}"
      value="${value%\'}"
      value="${value#\'}"
      values+=("$value")
    else
      invalid_assignment=1
    fi
  fi
done <"$ENV_FILE"

if [[ "$invalid_assignment" == "1" || "$assignment_count" -ne 1 || "${#values[@]}" -ne 1 || "${values[0]}" != "0" ]]; then
  echo "Xiaoman poster services stopped; persistent enablement must be exactly 0" >&2
  exit 1
fi

echo "Xiaoman Feishu poster production services disabled; durable workflow state retained"
