#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_FEISHU_POSTER_PRODUCTION_ACTIVATION:-}" != "approved-production-xiaoman-feishu-poster-return" ]]; then
  echo "Xiaoman Feishu poster production activation requires explicit owner approval" >&2
  exit 1
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
ENV_FILE="/etc/qintopia/message-sidecar.env"
HERMES_ENV_FILE="/home/ubuntu/.hermes/profiles/xiaoman/.env"
HERMES_PLUGIN_PATH="/home/ubuntu/.hermes/profiles/xiaoman/plugins/qintopia-tools"
RELEASE_PLUGIN_PATH="/home/ubuntu/qintopia-agent-os-releases/current/skills/qintopia-tools/variants/xiaoman"
RUNUSER_BIN="/usr/sbin/runuser"
PYTHON_BIN="/usr/bin/python3"
HERMES_SYSTEMD_USER="ubuntu"
HERMES_SERVICE="hermes-gateway-xiaoman.service"
PREFLIGHT_SERVICE="qintopia-agentos-xiaoman-feishu-poster-preflight.service"
INTAKE_SERVICE="qintopia-agentos-operations-intake.service"
CALLBACK_SERVICE="qintopia-agentos-xiaoman-poster-review-callback.service"
STARTER_TIMER="qintopia-agentos-xiaoman-poster-notification-starter.timer"
DELIVERY_TIMER="qintopia-agentos-xiaoman-feishu-poster-delivery.timer"
GROUP_DELIVERY_TIMER="qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer"

if [[ ! -x "$SYSTEMCTL" || ! -x "$RUNUSER_BIN" || ! -x "$PYTHON_BIN" || ! -f "$ENV_FILE" || ! -f "$HERMES_ENV_FILE" ]]; then
  echo "Xiaoman Feishu poster production activation prerequisites are missing" >&2
  exit 1
fi

if [[ ! -L "$HERMES_PLUGIN_PATH" || ! -d "$RELEASE_PLUGIN_PATH" || "$(readlink -f "$HERMES_PLUGIN_PATH")" != "$(readlink -f "$RELEASE_PLUGIN_PATH")" ]]; then
  echo "Xiaoman Feishu poster production activation requires the immutable release plugin" >&2
  exit 1
fi

if ! "$PYTHON_BIN" - "$ENV_FILE" "$HERMES_ENV_FILE" <<'PY'
import os
import re
import shlex
import stat
import sys

sidecar_path, hermes_path = sys.argv[1:3]
callback_key = "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY"
group_key = "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"


def parse(path, wanted):
    metadata = os.lstat(path)
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise SystemExit(1)
    assignment = re.compile(r"^(?:export[ \t]+)?([A-Za-z_][A-Za-z0-9_]*)[ \t]*=(.*)$")
    values = {}
    with open(path, encoding="utf-8") as handle:
        for raw in handle:
            line = raw.rstrip("\r\n")
            stripped = line.strip()
            if not stripped or stripped.startswith("#"):
                continue
            match = assignment.fullmatch(line)
            if not match:
                if any(stripped.startswith(name) for name in wanted):
                    raise SystemExit(1)
                continue
            name, raw_value = match.groups()
            if name not in wanted:
                continue
            if name in values:
                raise SystemExit(1)
            parts = shlex.split(raw_value, comments=True, posix=True)
            if len(parts) != 1:
                raise SystemExit(1)
            values[name] = parts[0]
    if set(values) != set(wanted):
        raise SystemExit(1)
    return values


sidecar = parse(
    sidecar_path,
    {"QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED", callback_key, group_key},
)
hermes = parse(
    hermes_path,
    {"QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE", callback_key, group_key},
)
key = sidecar[callback_key]
if (
    sidecar["QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED"] != "1"
    or hermes["QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE"] != "1"
    or sidecar[group_key] != "0"
    or hermes[group_key] != "0"
    or not key
    or len(key) > 512
    or key != hermes[callback_key]
):
    raise SystemExit(1)
PY
then
  echo "Xiaoman Feishu poster production activation env binding is invalid" >&2
  exit 1
fi

restart_xiaoman() {
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
}

"$SYSTEMCTL" disable --now "$GROUP_DELIVERY_TIMER"
"$SYSTEMCTL" start "$PREFLIGHT_SERVICE"
restart_xiaoman
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
