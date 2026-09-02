#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "Space automation runtime production observation skipped: set QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_ENABLE=1 to inspect runtime state" >&2
  exit 0
fi

DEFAULT_ENV_FILE="/etc/qintopia/message-sidecar.env"
DEFAULT_RELEASE_CURRENT_DIR="/home/ubuntu/qintopia-agent-os-releases/current"
DEFAULT_UNIT_DIR="/etc/systemd/system"
DEFAULT_SYSTEMCTL="/usr/bin/systemctl"
DEFAULT_PROC_ROOT="/proc"
ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-$DEFAULT_ENV_FILE}"
RELEASE_CURRENT_DIR="${QINTOPIA_RELEASE_CURRENT_DIR:-$DEFAULT_RELEASE_CURRENT_DIR}"
UNIT_DIR="${QINTOPIA_SYSTEMD_UNIT_DIR:-$DEFAULT_UNIT_DIR}"
SYSTEMCTL="${SYSTEMCTL:-$DEFAULT_SYSTEMCTL}"
EXPECTED_STATE="${QINTOPIA_SPACE_AUTOMATION_RUNTIME_EXPECTED_STATE:-auto}"
EXPECTED_RELEASE_SHA="${QINTOPIA_SPACE_AUTOMATION_RUNTIME_RELEASE_SHA:-}"
EXPECTED_COMMIT_SHA="${QINTOPIA_SPACE_AUTOMATION_RUNTIME_COMMIT_SHA:-}"
EXPECTED_RUNTIME_SHA="${QINTOPIA_SPACE_AUTOMATION_RUNTIME_RUNTIME_SHA:-}"
EXPECTED_DEPLOY_BUNDLE_SHA="${QINTOPIA_SPACE_AUTOMATION_RUNTIME_DEPLOY_BUNDLE_SHA:-}"
TEST_MODE="${QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_TEST_MODE:-0}"
TEST_ROOT="${QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_TEST_ROOT:-}"
PROC_ROOT="$DEFAULT_PROC_ROOT"
DISPATCHER_TIMER="qintopia-agentos-automation-dispatcher.timer"
DISPATCHER_SERVICE="qintopia-agentos-automation-dispatcher.service"
EXECUTION_WORKER="qintopia-agentos-space-automation-execution-worker.service"
PATH="/usr/bin:/bin:/usr/sbin:/sbin"

for expected_sha in \
  "$EXPECTED_RELEASE_SHA" \
  "$EXPECTED_COMMIT_SHA" \
  "$EXPECTED_RUNTIME_SHA" \
  "$EXPECTED_DEPLOY_BUNDLE_SHA"; do
  if [[ ! "$expected_sha" =~ ^[0-9a-f]{40}$ ]]; then
    echo "Space automation runtime production observation requires complete release identity" >&2
    exit 1
  fi
done

if [[ "$TEST_MODE" != "1" ]]; then
  if [[ "$ENV_FILE" != "$DEFAULT_ENV_FILE" || "$RELEASE_CURRENT_DIR" != "$DEFAULT_RELEASE_CURRENT_DIR" || "$UNIT_DIR" != "$DEFAULT_UNIT_DIR" || "$SYSTEMCTL" != "$DEFAULT_SYSTEMCTL" ]]; then
    echo "Space automation runtime production observation requires fixed production paths" >&2
    exit 1
  fi
else
  if [[ "$TEST_ROOT" != /tmp/* && "$TEST_ROOT" != /private/tmp/* ]]; then
    echo "Space automation runtime production observation test mode requires a temporary test root" >&2
    exit 1
  fi
  PROC_ROOT="${QINTOPIA_SPACE_AUTOMATION_RUNTIME_OBSERVATION_TEST_PROC_ROOT:-}"
  for candidate in "$ENV_FILE" "$RELEASE_CURRENT_DIR" "$UNIT_DIR" "$SYSTEMCTL" "$PROC_ROOT"; do
    case "$candidate" in
      "$TEST_ROOT"/*) ;;
      *)
        echo "Space automation runtime production observation test paths must stay under the test root" >&2
        exit 1
        ;;
    esac
  done
fi

if [[ ! -x "$SYSTEMCTL" ]]; then
  echo "systemctl is required for Space automation runtime production observation" >&2
  exit 1
fi
if ! python3 - "$ENV_FILE" <<'PY'
import os
import stat
import sys

path = sys.argv[1]
try:
    file_stat = os.lstat(path)
except OSError:
    raise SystemExit(1)
if (
    not stat.S_ISREG(file_stat.st_mode)
    or stat.S_IMODE(file_stat.st_mode) != 0o600
    or file_stat.st_uid != os.geteuid()
):
    raise SystemExit(1)
PY
then
  echo "Space automation runtime production observation requires an owner-only regular env file" >&2
  exit 1
fi

systemd_property_equals() {
  local unit="$1"
  local property="$2"
  local expected="$3"
  local observed
  if ! observed="$("$SYSTEMCTL" show --property="$property" --value "$unit" 2>/dev/null)"; then
    return 1
  fi
  [[ "$observed" == "$expected" ]]
}

if ! RELEASE_FACTS="$(python3 - "$RELEASE_CURRENT_DIR" "$EXPECTED_RELEASE_SHA" "$EXPECTED_COMMIT_SHA" "$EXPECTED_RUNTIME_SHA" "$EXPECTED_DEPLOY_BUNDLE_SHA" <<'PY'
import json
import os
import re
import stat
import sys

current_path, expected_release, expected_commit, expected_runtime, expected_bundle = sys.argv[1:]
if not os.path.isabs(current_path) or not os.path.exists(current_path):
    raise SystemExit(1)
current_real = os.path.realpath(current_path)
release_sha = os.path.basename(current_real)
if release_sha != expected_release or not re.fullmatch(r"[0-9a-f]{40}", release_sha):
    raise SystemExit(1)

release_manifest_path = os.path.join(current_real, "manifest.json")
if os.path.islink(release_manifest_path) or not os.path.isfile(release_manifest_path):
    raise SystemExit(1)
if os.stat(release_manifest_path).st_mode & (stat.S_IWGRP | stat.S_IWOTH):
    raise SystemExit(1)
with open(release_manifest_path, encoding="utf-8") as fh:
    release_manifest = json.load(fh)
for key, expected in (
    ("release_sha", expected_release),
    ("commit_sha", expected_commit),
    ("runtime_sha", expected_runtime),
    ("deploy_bundle_sha", expected_bundle),
):
    if release_manifest.get(key) != expected or not re.fullmatch(
        r"[0-9a-f]{40}", release_manifest.get(key, "")
    ):
        raise SystemExit(1)

primary_bin = os.path.join(current_real, "sidecar", "qintopia-message-sidecar")
companion_dir = os.path.join(current_real, "sidecar-profiles", "qiwe-production")
companion_bin = os.path.join(companion_dir, "qintopia-message-sidecar")
manifest_path = os.path.join(companion_dir, "artifact-manifest.json")
for binary in (primary_bin, companion_bin):
    if os.path.islink(binary) or not os.path.isfile(binary) or not os.access(binary, os.X_OK):
        raise SystemExit(1)
if os.path.islink(manifest_path) or not os.path.isfile(manifest_path):
    raise SystemExit(1)
for candidate in (
    current_real,
    os.path.join(current_real, "sidecar"),
    primary_bin,
    os.path.join(current_real, "sidecar-profiles"),
    companion_dir,
    companion_bin,
    manifest_path,
):
    if os.stat(candidate).st_mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise SystemExit(1)

with open(manifest_path, encoding="utf-8") as fh:
    manifest = json.load(fh)
validation = manifest.get("validation", {})
if manifest.get("commit_sha") != release_manifest["commit_sha"]:
    raise SystemExit(1)
if validation.get("artifact_profile") != "qiwe-production":
    raise SystemExit(1)
if validation.get("cargo_features") != [
    "qiwe-production-adapter",
    "huabaosi-feishu-mirror-adapter",
]:
    raise SystemExit(1)

print(json.dumps({
    "release_dir": current_real,
    "release_sha": release_sha,
    "commit_sha": release_manifest["commit_sha"],
    "runtime_sha": release_manifest["runtime_sha"],
    "deploy_bundle_sha": release_manifest["deploy_bundle_sha"],
}))
PY
)"; then
  echo "Space automation runtime production observation requires the reviewed immutable runtime artifacts" >&2
  exit 1
fi

RELEASE_DIR="$(python3 - "$RELEASE_FACTS" <<'PY'
import json
import sys
print(json.loads(sys.argv[1])["release_dir"])
PY
)"
RELEASE_SHA="$(python3 - "$RELEASE_FACTS" <<'PY'
import json
import sys
print(json.loads(sys.argv[1])["release_sha"])
PY
)"

read_binary_env_flag() {
  local key="$1"
  python3 - "$ENV_FILE" "$key" <<'PY'
import re
import sys

path, expected_key = sys.argv[1:3]
assignment = re.compile(r"^(?:export[ \t]+)?([A-Z0-9_]+)[ \t]*=[ \t]*(.*?)[ \t]*(?:#[^\"']*)?$")
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
            raise SystemExit(f"duplicate binary flag {expected_key}")
        seen = True
        value = value.strip()
        if (value.startswith('"') and value.endswith('"')) or (
            value.startswith("'") and value.endswith("'")
        ):
            value = value[1:-1]
        if value not in {"0", "1"}:
            raise SystemExit(f"invalid binary flag {expected_key}")
        print(value)
if not seen:
    raise SystemExit(f"missing binary flag {expected_key}")
PY
}

if ! EXECUTION_ENABLED="$(read_binary_env_flag "QINTOPIA_SPACE_AUTOMATION_EXECUTION_ENABLED")"; then
  echo "Space automation runtime production observation env is invalid" >&2
  exit 1
fi
if ! AGENT_TURN_RUNTIME_READY="$(read_binary_env_flag "QINTOPIA_SPACE_AGENT_TURN_RUNTIME_READY")"; then
  echo "Space automation runtime production observation agent-turn readiness is invalid" >&2
  exit 1
fi
if [[ "$AGENT_TURN_RUNTIME_READY" != "0" ]]; then
  echo "Space automation runtime production observation requires agent-turn readiness to remain disabled until its dedicated runtime is reviewed" >&2
  exit 1
fi

if [[ "$EXPECTED_STATE" == "auto" ]]; then
  if [[ "$EXECUTION_ENABLED" == "1" ]]; then
    EXPECTED_STATE="enabled"
  else
    EXPECTED_STATE="disabled"
  fi
fi
if [[ "$EXPECTED_STATE" != "enabled" && "$EXPECTED_STATE" != "disabled" ]]; then
  echo "Space automation runtime expected state must be enabled, disabled, or auto" >&2
  exit 1
fi
if [[ "$EXPECTED_STATE" == "enabled" && "$EXECUTION_ENABLED" != "1" ]]; then
  echo "Space automation runtime persistent execution flag does not match enabled state" >&2
  exit 1
fi
if [[ "$EXPECTED_STATE" == "disabled" && "$EXECUTION_ENABLED" != "0" ]]; then
  echo "Space automation runtime persistent execution flag does not match disabled state" >&2
  exit 1
fi

if ! python3 - "$UNIT_DIR" "$ENV_FILE" "$RELEASE_DIR" "$RELEASE_SHA" "$TEST_MODE" <<'PY'
import os
import stat
import sys

unit_dir, env_file, release_dir, release_sha, test_mode = sys.argv[1:6]
primary_bin = os.path.join(release_dir, "sidecar", "qintopia-message-sidecar")
qiwe_bin = os.path.join(
    release_dir, "sidecar-profiles", "qiwe-production", "qintopia-message-sidecar"
)
migrations = os.path.join(release_dir, "runtime", "postgres", "migrations")

expected = {
    "qintopia-agentos-automation-dispatcher.service": f"""[Unit]
Description=Qintopia AgentOS Space automation dispatcher
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory={release_dir}
EnvironmentFile={env_file}
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR={migrations}
ExecStart=/usr/bin/env QINTOPIA_DEPLOYED_COMMIT_SHA={release_sha} QINTOPIA_SIDECAR_MIGRATIONS_DIR={migrations} {primary_bin} run-automation-dispatcher --once --apply
NoNewPrivileges=true
PrivateTmp=true
""",
    "qintopia-agentos-automation-dispatcher.timer": """[Unit]
Description=Run Qintopia AgentOS Space automation dispatcher

[Timer]
OnBootSec=1min
OnUnitActiveSec=1min
AccuracySec=30s
Persistent=true
Unit=qintopia-agentos-automation-dispatcher.service

[Install]
WantedBy=timers.target
""",
    "qintopia-agentos-space-automation-execution-worker.service": f"""[Unit]
Description=Qintopia AgentOS Space automation execution worker
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory={release_dir}
EnvironmentFile={env_file}


Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR={migrations}
# EnvironmentFile values override Environment values. Bind immutable release identity
# and migrations at the final exec boundary so stale persistent values cannot shadow
# this release.
ExecStart=/usr/bin/env QINTOPIA_DEPLOYED_COMMIT_SHA={release_sha} QINTOPIA_SIDECAR_MIGRATIONS_DIR={migrations} {qiwe_bin} run-space-automation-execution-worker --apply
Restart=always
RestartSec=10
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
""",
}

for name, expected_text in expected.items():
    path = os.path.join(unit_dir, name)
    if os.path.islink(path) or not os.path.isfile(path):
        raise SystemExit(1)
    metadata = os.stat(path)
    if metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise SystemExit(1)
    if test_mode != "1" and metadata.st_uid != 0:
        raise SystemExit(1)
    with open(path, encoding="utf-8") as fh:
        if fh.read() != expected_text:
            raise SystemExit(1)
PY
then
  echo "Space automation runtime production observation found unreviewed systemd unit content" >&2
  exit 1
fi

if [[ "$EXPECTED_STATE" == "enabled" ]]; then
  "$SYSTEMCTL" is-enabled --quiet "$DISPATCHER_TIMER" >/dev/null 2>&1 || {
    echo "Space automation dispatcher timer must be enabled" >&2
    exit 1
  }
  "$SYSTEMCTL" is-active --quiet "$DISPATCHER_TIMER" >/dev/null 2>&1 || {
    echo "Space automation dispatcher timer must be active" >&2
    exit 1
  }
  "$SYSTEMCTL" is-enabled --quiet "$EXECUTION_WORKER" >/dev/null 2>&1 || {
    echo "Space automation execution worker must be enabled" >&2
    exit 1
  }
  "$SYSTEMCTL" is-active --quiet "$EXECUTION_WORKER" >/dev/null 2>&1 || {
    echo "Space automation execution worker must be active" >&2
    exit 1
  }
  for unit in "$DISPATCHER_TIMER" "$DISPATCHER_SERVICE" "$EXECUTION_WORKER"; do
    systemd_property_equals "$unit" "LoadState" "loaded" || {
      echo "Space automation runtime units must be loaded" >&2
      exit 1
    }
  done
  for unit in "$DISPATCHER_TIMER" "$EXECUTION_WORKER"; do
    systemd_property_equals "$unit" "UnitFileState" "enabled" || {
      echo "Space automation runtime units must be persistently enabled" >&2
      exit 1
    }
    systemd_property_equals "$unit" "ActiveState" "active" || {
      echo "Space automation runtime units must report active state" >&2
      exit 1
    }
  done
  timer_next_elapse="$("$SYSTEMCTL" show --property=NextElapseUSecMonotonic --value "$DISPATCHER_TIMER")"
  if [[ -z "$timer_next_elapse" || "$timer_next_elapse" == "0" || "$timer_next_elapse" == "infinity" || "$timer_next_elapse" == "n/a" ]]; then
    echo "Space automation dispatcher timer must expose a scheduled trigger" >&2
    exit 1
  fi
  worker_pid="$("$SYSTEMCTL" show --property=MainPID --value "$EXECUTION_WORKER")"
  worker_started_monotonic="$("$SYSTEMCTL" show --property=ExecMainStartTimestampMonotonic --value "$EXECUTION_WORKER")"
  if ! python3 - "$PROC_ROOT" "$worker_pid" "$worker_started_monotonic" "$RELEASE_DIR" <<'PY'
import os
import re
import sys

proc_root, pid, started_monotonic, release_dir = sys.argv[1:5]
if not re.fullmatch(r"[1-9][0-9]*", pid):
    raise SystemExit(1)
if not re.fullmatch(r"[1-9][0-9]*", started_monotonic):
    raise SystemExit(1)
expected = os.path.join(
    release_dir,
    "sidecar-profiles",
    "qiwe-production",
    "qintopia-message-sidecar",
)
process_exe = os.path.join(proc_root, pid, "exe")
if not os.path.islink(process_exe) or os.path.realpath(process_exe) != expected:
    raise SystemExit(1)
PY
  then
    echo "Space automation execution worker must run the current reviewed companion binary" >&2
    exit 1
  fi
  echo "space_automation_runtime_dispatcher_timer_schedule_value_present=true"
  echo "space_automation_runtime_worker_release_identity_verified=true"
else
  if "$SYSTEMCTL" is-enabled --quiet "$DISPATCHER_TIMER" >/dev/null 2>&1; then
    echo "Space automation dispatcher timer must be disabled" >&2
    exit 1
  fi
  if "$SYSTEMCTL" is-active --quiet "$DISPATCHER_TIMER" >/dev/null 2>&1; then
    echo "Space automation dispatcher timer must be inactive" >&2
    exit 1
  fi
  if "$SYSTEMCTL" is-enabled --quiet "$EXECUTION_WORKER" >/dev/null 2>&1; then
    echo "Space automation execution worker must be disabled" >&2
    exit 1
  fi
  if "$SYSTEMCTL" is-active --quiet "$EXECUTION_WORKER" >/dev/null 2>&1; then
    echo "Space automation execution worker must be inactive" >&2
    exit 1
  fi
  if "$SYSTEMCTL" is-active --quiet "$DISPATCHER_SERVICE" >/dev/null 2>&1; then
    echo "Space automation dispatcher service must be inactive" >&2
    exit 1
  fi
  for unit in "$DISPATCHER_TIMER" "$DISPATCHER_SERVICE" "$EXECUTION_WORKER"; do
    systemd_property_equals "$unit" "LoadState" "loaded" || {
      echo "Space automation runtime units must remain loaded while disabled" >&2
      exit 1
    }
  done
  for unit in "$DISPATCHER_TIMER" "$EXECUTION_WORKER"; do
    systemd_property_equals "$unit" "UnitFileState" "disabled" || {
      echo "Space automation runtime units must report disabled unit-file state" >&2
      exit 1
    }
  done
  for unit in "$DISPATCHER_TIMER" "$DISPATCHER_SERVICE" "$EXECUTION_WORKER"; do
    systemd_property_equals "$unit" "ActiveState" "inactive" || {
      echo "Space automation runtime units must report inactive state" >&2
      exit 1
    }
  done
fi

echo "space_automation_runtime_observation_state=${EXPECTED_STATE}"
echo "space_automation_runtime_artifact_profile=qiwe-production"
echo "space_automation_runtime_release_sha=${RELEASE_SHA}"
echo "Space automation runtime production observation passed"
