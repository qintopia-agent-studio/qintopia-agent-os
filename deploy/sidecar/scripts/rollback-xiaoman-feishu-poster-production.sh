#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_FEISHU_POSTER_PRODUCTION_ROLLBACK:-}" != "approved-production-xiaoman-feishu-poster-return-rollback" ]]; then
  echo "Xiaoman Feishu poster production rollback requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
ENV_FILE="/etc/qintopia/message-sidecar.env"
HERMES_ENV_FILE="/home/ubuntu/.hermes/profiles/xiaoman/.env"
RUNUSER_BIN="/usr/sbin/runuser"
PYTHON_BIN="/usr/bin/python3"
HERMES_SYSTEMD_USER="ubuntu"
HERMES_SERVICE="hermes-gateway-xiaoman.service"
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

if [[ ! -x "$RUNUSER_BIN" || ! -x "$PYTHON_BIN" || ! -f "$ENV_FILE" || ! -f "$HERMES_ENV_FILE" ]]; then
  echo "Xiaoman poster services stopped, but persistent disablement cannot be confirmed" >&2
  exit 1
fi

if ! "$PYTHON_BIN" - "$ENV_FILE" "$HERMES_ENV_FILE" <<'PY'
import os
import re
import shlex
import stat
import sys


def disabled(path, name):
    metadata = os.lstat(path)
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise SystemExit(1)
    assignment = re.compile(r"^(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_]*)[ \t]*=(.*)$")
    values = []
    with open(path, encoding="utf-8") as handle:
        for raw in handle:
            line = raw.rstrip("\r\n")
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            match = assignment.fullmatch(line)
            if not match:
                if stripped.startswith(name):
                    raise SystemExit(1)
                continue
            key, raw_value = match.groups()
            if key != name:
                continue
            parts = shlex.split(raw_value, comments=True, posix=True)
            if len(parts) != 1:
                raise SystemExit(1)
            values.append(parts[0])
    if values != ["0"]:
        raise SystemExit(1)


disabled(sys.argv[1], "QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED")
disabled(sys.argv[2], "QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE")
PY
then
  echo "Xiaoman poster services stopped; persistent enablement must be exactly 0" >&2
  exit 1
fi

env -i \
  PATH=/usr/bin:/bin:/usr/sbin:/sbin \
  HOME="/home/${HERMES_SYSTEMD_USER}" \
  USER="$HERMES_SYSTEMD_USER" \
  LOGNAME="$HERMES_SYSTEMD_USER" \
  "$RUNUSER_BIN" -l "$HERMES_SYSTEMD_USER" -c \
  "XDG_RUNTIME_DIR=/run/user/\$(id -u) systemctl --user restart ${HERMES_SERVICE}"
env -i \
  PATH=/usr/bin:/bin:/usr/sbin:/sbin \
  HOME="/home/${HERMES_SYSTEMD_USER}" \
  USER="$HERMES_SYSTEMD_USER" \
  LOGNAME="$HERMES_SYSTEMD_USER" \
  "$RUNUSER_BIN" -l "$HERMES_SYSTEMD_USER" -c \
  "XDG_RUNTIME_DIR=/run/user/\$(id -u) systemctl --user is-active --quiet ${HERMES_SERVICE}"

echo "Xiaoman Feishu poster production services disabled; durable workflow state retained"
