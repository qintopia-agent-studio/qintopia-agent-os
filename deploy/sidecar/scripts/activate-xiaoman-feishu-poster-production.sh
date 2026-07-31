#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_FEISHU_POSTER_PRODUCTION_ACTIVATION:-}" != "approved-production-xiaoman-feishu-poster-return" ]]; then
  echo "Xiaoman Feishu poster production activation requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
ENV_FILE="/etc/qintopia/message-sidecar.env"
PREFLIGHT_SERVICE="qintopia-agentos-xiaoman-feishu-poster-preflight.service"
INTAKE_SERVICE="qintopia-agentos-operations-intake.service"
CALLBACK_SERVICE="qintopia-agentos-xiaoman-poster-review-callback.service"
STARTER_TIMER="qintopia-agentos-xiaoman-poster-notification-starter.timer"
DELIVERY_TIMER="qintopia-agentos-xiaoman-feishu-poster-delivery.timer"

if [[ ! -x "$SYSTEMCTL" || ! -f "$ENV_FILE" ]]; then
  echo "Xiaoman Feishu poster production activation prerequisites are missing" >&2
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

if [[ "$invalid_assignment" == "1" || "$assignment_count" -ne 1 || "${#values[@]}" -ne 1 || "${values[0]}" != "1" ]]; then
  echo "Xiaoman Feishu poster production activation requires exactly one persistent enablement value of 1" >&2
  exit 1
fi

"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"
"$SYSTEMCTL" enable --now "$INTAKE_SERVICE"
"$SYSTEMCTL" enable --now "$CALLBACK_SERVICE"
"$SYSTEMCTL" enable --now "$STARTER_TIMER"
"$SYSTEMCTL" enable "$DELIVERY_TIMER"
"$SYSTEMCTL" restart "$DELIVERY_TIMER"

for unit in "$INTAKE_SERVICE" "$CALLBACK_SERVICE" "$STARTER_TIMER" "$DELIVERY_TIMER"; do
  "$SYSTEMCTL" is-enabled --quiet "$unit"
  "$SYSTEMCTL" is-active --quiet "$unit"
done

next_elapse="$("$SYSTEMCTL" show --property=NextElapseUSecMonotonic --value "$DELIVERY_TIMER")"
if [[ -z "$next_elapse" || "$next_elapse" == "0" || "$next_elapse" == "infinity" ]]; then
  "$SYSTEMCTL" disable --now "$DELIVERY_TIMER" >/dev/null 2>&1 || true
  echo "Xiaoman Feishu poster delivery timer has no future trigger" >&2
  exit 1
fi

echo "Xiaoman Feishu poster intake, callback, starter, and delivery activated"
