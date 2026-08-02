#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "Xiaoman Feishu internal-group production observation skipped: set QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_PRODUCTION_OBSERVATION_ENABLE=1 to inspect runtime state" >&2
  exit 0
fi

PATH="/usr/bin:/bin:/usr/sbin:/sbin"
SYSTEMCTL="/usr/bin/systemctl"
PYTHON_BIN="/usr/bin/python3"
RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
SIDECAR_ENV_FILE="/etc/qintopia/message-sidecar.env"
HERMES_ENV_FILE="/home/ubuntu/.hermes/profiles/xiaoman/.env"
HERMES_PLUGIN_PATH="/home/ubuntu/.hermes/profiles/xiaoman/plugins/qintopia-tools"
EXPECTED_STATE="${QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_EXPECTED_STATE:-auto}"
GROUP_DELIVERY_EXPECTED_STATE="${QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_DELIVERY_EXPECTED_STATE:-auto}"
INTAKE_SERVICE="qintopia-agentos-operations-intake.service"
CALLBACK_SERVICE="qintopia-agentos-xiaoman-poster-review-callback.service"
STARTER_TIMER="qintopia-agentos-xiaoman-poster-notification-starter.timer"
DIRECT_DELIVERY_TIMER="qintopia-agentos-xiaoman-feishu-poster-delivery.timer"
GROUP_DELIVERY_TIMER="qintopia-agentos-xiaoman-feishu-internal-group-poster-delivery.timer"

if [[ ! -x "$SYSTEMCTL" || ! -x "$PYTHON_BIN" ]]; then
  echo "Xiaoman Feishu internal-group production observation prerequisites are missing" >&2
  exit 1
fi

if ! RELEASE_SHA="$("$PYTHON_BIN" - "$RELEASE_CURRENT_DIR" "$HERMES_PLUGIN_PATH" 2>/dev/null <<'PY'
import json
import os
import re
import stat
import sys

current_path, plugin_path = sys.argv[1:3]
if not os.path.islink(current_path):
    raise SystemExit(1)
current_real = os.path.realpath(current_path)
release_sha = os.path.basename(current_real)
if not re.fullmatch(r"[0-9a-f]{40}", release_sha):
    raise SystemExit(1)

sidecar_dir = os.path.join(current_real, "sidecar")
sidecar_bin = os.path.join(sidecar_dir, "qintopia-message-sidecar")
manifest_path = os.path.join(sidecar_dir, "artifact-manifest.json")
expected_plugin = os.path.join(
    current_real, "skills", "qintopia-tools", "variants", "xiaoman"
)
plugin_entrypoint = os.path.join(expected_plugin, "__init__.py")
if not os.path.islink(plugin_path) or os.path.realpath(plugin_path) != expected_plugin:
    raise SystemExit(1)
if os.path.islink(sidecar_bin) or not os.path.isfile(sidecar_bin) or not os.access(sidecar_bin, os.X_OK):
    raise SystemExit(1)
if os.path.islink(manifest_path) or not os.path.isfile(manifest_path):
    raise SystemExit(1)
if os.path.islink(plugin_entrypoint) or not os.path.isfile(plugin_entrypoint):
    raise SystemExit(1)

for path in (
    current_real,
    sidecar_dir,
    sidecar_bin,
    manifest_path,
    os.path.join(current_real, "skills"),
    os.path.join(current_real, "skills", "qintopia-tools"),
    os.path.join(current_real, "skills", "qintopia-tools", "variants"),
    expected_plugin,
    plugin_entrypoint,
):
    if os.stat(path).st_mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise SystemExit(1)

with open(manifest_path, encoding="utf-8") as handle:
    manifest = json.load(handle)
if manifest.get("commit_sha") != release_sha:
    raise SystemExit(1)
validation = manifest.get("validation", {})
if validation.get("artifact_profile") != "huabaosi-production":
    raise SystemExit(1)
if validation.get("cargo_features") != [
    "huabaosi-production-adapter",
    "huabaosi-feishu-mirror-adapter",
    "xiaoman-feishu-poster-adapter",
]:
    raise SystemExit(1)
print(release_sha)
PY
)"; then
  echo "Xiaoman Feishu internal-group production observation requires an immutable release/current sidecar and Xiaoman plugin" >&2
  exit 1
fi

if ! ENV_FACTS="$("$PYTHON_BIN" - "$SIDECAR_ENV_FILE" "$HERMES_ENV_FILE" 2>/dev/null <<'PY'
import os
import re
import shlex
import stat
import sys

sidecar_path, hermes_path = sys.argv[1:3]
group_key = "QINTOPIA_XIAOMAN_FEISHU_INTERNAL_GROUP_ENABLED"
ingress_key = "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HMAC_KEY"
callback_key = "QINTOPIA_XIAOMAN_FEISHU_CALLBACK_ENCRYPT_KEY"
bot_key = "QINTOPIA_XIAOMAN_FEISHU_BOT_OPEN_ID"


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
    {
        "QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED",
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE",
        group_key,
        ingress_key,
        callback_key,
        bot_key,
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS",
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS",
        "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS",
        "QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS",
        "QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS",
    },
)
hermes = parse(
    hermes_path,
    {
        "QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE",
        "QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE",
        group_key,
        ingress_key,
        callback_key,
        bot_key,
    },
)
if (
    sidecar["QINTOPIA_XIAOMAN_FEISHU_POSTER_ENABLED"] != "1"
    or sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"] != "1"
    or hermes["QINTOPIA_XIAOMAN_FEISHU_INGRESS_HOOK_ENABLE"] != "1"
    or hermes["QINTOPIA_XIAOMAN_POSTER_REVIEW_HOOK_ENABLE"] != "1"
):
    raise SystemExit(1)
if sidecar[group_key] not in {"0", "1"} or sidecar[group_key] != hermes[group_key]:
    raise SystemExit(1)
if not 32 <= len(sidecar[ingress_key]) <= 512:
    raise SystemExit(1)
if not 1 <= len(sidecar[callback_key]) <= 512:
    raise SystemExit(1)
if sidecar[ingress_key] != hermes[ingress_key] or sidecar[callback_key] != hermes[callback_key]:
    raise SystemExit(1)
if sidecar[ingress_key] == sidecar[callback_key]:
    raise SystemExit(1)

identifier = re.compile(r"[A-Za-z0-9_.-]{1,240}")
if not identifier.fullmatch(sidecar[bot_key]) or sidecar[bot_key] != hermes[bot_key]:
    raise SystemExit(1)


def identifiers(value):
    items = {item.strip() for item in value.split(",") if item.strip()}
    if not items or len(items) > 256 or any(not identifier.fullmatch(item) for item in items):
        raise SystemExit(1)
    return items


ingress_chats = identifiers(sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_CHAT_IDS"])
ingress_users = identifiers(sidecar["QINTOPIA_XIAOMAN_FEISHU_INGRESS_ALLOWED_USER_IDS"])
delivery_chats = identifiers(sidecar["QINTOPIA_XIAOMAN_FEISHU_ALLOWED_CHAT_IDS"])
delivery_users = identifiers(sidecar["QINTOPIA_XIAOMAN_FEISHU_ALLOWED_USER_IDS"])
reviewer_users = identifiers(sidecar["QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS"])
if delivery_chats != ingress_chats or delivery_users != ingress_users:
    raise SystemExit(1)
if not delivery_users.issubset(reviewer_users):
    raise SystemExit(1)

state = "enabled" if sidecar[group_key] == "1" else "disabled"
print(f"{state}|{len(ingress_chats)}|{len(ingress_users)}|{len(delivery_chats)}|{len(delivery_users)}|{len(reviewer_users)}")
PY
)"; then
  echo "Xiaoman Feishu internal-group production environment binding is invalid" >&2
  exit 1
fi

IFS='|' read -r OBSERVED_STATE INGRESS_CHAT_COUNT INGRESS_USER_COUNT DELIVERY_CHAT_COUNT DELIVERY_USER_COUNT REVIEWER_USER_COUNT <<<"$ENV_FACTS"
if [[ "$EXPECTED_STATE" == "auto" ]]; then
  EXPECTED_STATE="$OBSERVED_STATE"
fi
if [[ "$EXPECTED_STATE" != "enabled" && "$EXPECTED_STATE" != "disabled" ]]; then
  echo "Xiaoman Feishu internal-group expected state must be disabled, enabled, or auto" >&2
  exit 1
fi
if [[ "$EXPECTED_STATE" != "$OBSERVED_STATE" ]]; then
  echo "Xiaoman Feishu internal-group observed state does not match expected state" >&2
  exit 1
fi
if [[ "$GROUP_DELIVERY_EXPECTED_STATE" == "auto" ]]; then
  if [[ "$OBSERVED_STATE" == "enabled" ]]; then
    GROUP_DELIVERY_EXPECTED_STATE="active"
  else
    GROUP_DELIVERY_EXPECTED_STATE="stopped"
  fi
fi
if [[ "$GROUP_DELIVERY_EXPECTED_STATE" != "active" && "$GROUP_DELIVERY_EXPECTED_STATE" != "stopped" ]]; then
  echo "Xiaoman Feishu internal-group delivery expected state must be active, stopped, or auto" >&2
  exit 1
fi

for unit in "$INTAKE_SERVICE" "$CALLBACK_SERVICE" "$STARTER_TIMER"; do
  env -i PATH="$PATH" "$SYSTEMCTL" is-enabled --quiet "$unit"
  env -i PATH="$PATH" "$SYSTEMCTL" is-active --quiet "$unit"
done
env -i PATH="$PATH" "$SYSTEMCTL" is-enabled --quiet "$DIRECT_DELIVERY_TIMER"
env -i PATH="$PATH" "$SYSTEMCTL" is-active --quiet "$DIRECT_DELIVERY_TIMER"
direct_next_elapse="$(env -i PATH="$PATH" "$SYSTEMCTL" show --property=NextElapseUSecMonotonic --value "$DIRECT_DELIVERY_TIMER")"
if [[ -z "$direct_next_elapse" || "$direct_next_elapse" == "0" || "$direct_next_elapse" == "infinity" ]]; then
  echo "Xiaoman Feishu direct poster delivery timer has no future trigger" >&2
  exit 1
fi

if [[ "$GROUP_DELIVERY_EXPECTED_STATE" == "active" ]]; then
  env -i PATH="$PATH" "$SYSTEMCTL" is-enabled --quiet "$GROUP_DELIVERY_TIMER"
  env -i PATH="$PATH" "$SYSTEMCTL" is-active --quiet "$GROUP_DELIVERY_TIMER"
  next_elapse="$(env -i PATH="$PATH" "$SYSTEMCTL" show --property=NextElapseUSecMonotonic --value "$GROUP_DELIVERY_TIMER")"
  if [[ -z "$next_elapse" || "$next_elapse" == "0" || "$next_elapse" == "infinity" ]]; then
    echo "Xiaoman Feishu internal-group poster delivery timer has no future trigger" >&2
    exit 1
  fi
else
  if env -i PATH="$PATH" "$SYSTEMCTL" is-enabled --quiet "$GROUP_DELIVERY_TIMER"; then
    echo "Xiaoman Feishu internal-group poster delivery timer remains enabled" >&2
    exit 1
  fi
  if env -i PATH="$PATH" "$SYSTEMCTL" is-active --quiet "$GROUP_DELIVERY_TIMER"; then
    echo "Xiaoman Feishu internal-group poster delivery timer remains active" >&2
    exit 1
  fi
fi

echo "xiaoman_feishu_internal_group_production_observation_state=${OBSERVED_STATE}"
echo "xiaoman_feishu_direct_delivery_runtime_state=active"
echo "xiaoman_feishu_internal_group_delivery_runtime_state=${GROUP_DELIVERY_EXPECTED_STATE}"
echo "xiaoman_feishu_internal_group_production_release_sha=${RELEASE_SHA}"
echo "xiaoman_feishu_internal_group_ingress_chat_allowlist_count=${INGRESS_CHAT_COUNT}"
echo "xiaoman_feishu_internal_group_ingress_user_allowlist_count=${INGRESS_USER_COUNT}"
echo "xiaoman_feishu_internal_group_delivery_chat_allowlist_count=${DELIVERY_CHAT_COUNT}"
echo "xiaoman_feishu_internal_group_delivery_user_allowlist_count=${DELIVERY_USER_COUNT}"
echo "xiaoman_feishu_internal_group_reviewer_allowlist_count=${REVIEWER_USER_COUNT}"
echo "Xiaoman Feishu internal-group production observation passed"
