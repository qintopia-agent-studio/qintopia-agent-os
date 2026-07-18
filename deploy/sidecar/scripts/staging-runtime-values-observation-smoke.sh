#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "staging runtime values observation skipped: set QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENABLE=1 to inspect fixed staging values inputs" >&2
  exit 0
fi

TEST_MODE="${QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_TEST_MODE:-0}"
if [[ "$TEST_MODE" != "0" && "$TEST_MODE" != "1" ]]; then
  echo "QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_TEST_MODE must be 0 or 1" >&2
  exit 1
fi

VALUES_FILE="/etc/qintopia/message-sidecar-staging-values.json"
ENV_FILE="/etc/qintopia/message-sidecar-staging.env"
RENDERER="deploy/sidecar/scripts/render-staging-runtime-env.py"
if [[ "$TEST_MODE" == "1" ]]; then
  VALUES_FILE="${QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_VALUES_FILE:-$VALUES_FILE}"
  ENV_FILE="${QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_ENV_FILE:-$ENV_FILE}"
  RENDERER="${QINTOPIA_STAGING_RUNTIME_VALUES_OBSERVATION_RENDERER:-$RENDERER}"
fi

OBSERVATION_VALUES_FILE="$VALUES_FILE" \
OBSERVATION_ENV_FILE="$ENV_FILE" \
OBSERVATION_RENDERER="$RENDERER" \
OBSERVATION_TEST_MODE="$TEST_MODE" \
python3 - <<'PY'
import json
import os
import stat


def add_limitation(report, value):
    if value not in report["limitations"]:
        report["limitations"].append(value)


def path_present(reason):
    return reason not in ("path_missing", "path_parent_missing")


def path_is_secure(
    path,
    *,
    require_regular=False,
    require_executable=False,
    allow_missing=False,
    reject_group_or_world_readable=False,
):
    if not os.path.isabs(path) and not path.startswith("deploy/"):
        return False, "path_not_absolute"
    if "staging" not in path:
        return False, "path_missing_staging_marker"
    if os.path.isabs(path):
        current = os.path.sep
        parts = path.strip(os.path.sep).split(os.path.sep)
    else:
        current = "."
        parts = path.split(os.path.sep)
    for part in parts:
        current = os.path.join(current, part)
        try:
            component_stat = os.lstat(current)
        except FileNotFoundError:
            if current == path or os.path.abspath(current) == os.path.abspath(path):
                if allow_missing:
                    return True, "path_missing"
                return False, "path_missing"
            return False, "path_parent_missing"
        is_final = os.path.abspath(current) == os.path.abspath(path)
        if stat.S_ISLNK(component_stat.st_mode):
            if is_final:
                return False, "path_is_symlink"
            return False, "path_parent_is_symlink"
        if not is_final:
            if not stat.S_ISDIR(component_stat.st_mode):
                return False, "path_parent_not_directory"
            if component_stat.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
                return False, "path_parent_group_or_world_writable"
            if component_stat.st_uid not in (0, os.geteuid()):
                return False, "path_parent_unexpected_owner"
    try:
        path_stat = os.lstat(path)
    except FileNotFoundError:
        if allow_missing:
            return True, "path_missing"
        return False, "path_missing"
    if stat.S_ISLNK(path_stat.st_mode):
        return False, "path_is_symlink"
    if require_regular and not stat.S_ISREG(path_stat.st_mode):
        return False, "path_not_regular_file"
    if require_executable and not os.access(path, os.X_OK):
        return False, "path_not_executable"
    if path_stat.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
        return False, "path_group_or_world_writable"
    if reject_group_or_world_readable and path_stat.st_mode & (
        stat.S_IRGRP | stat.S_IROTH
    ):
        return False, "path_group_or_world_readable"
    if path_stat.st_uid not in (0, os.geteuid()):
        return False, "path_unexpected_owner"
    return True, "ok"


values_file = os.environ["OBSERVATION_VALUES_FILE"]
env_file = os.environ["OBSERVATION_ENV_FILE"]
renderer = os.environ["OBSERVATION_RENDERER"]

report = {
    "success": True,
    "worker": "staging-runtime-values-observation",
    "action_status": "not_ready",
    "ready_for_render_validation": False,
    "test_mode": os.environ["OBSERVATION_TEST_MODE"] == "1",
    "values_file_present": False,
    "values_file_secure": False,
    "env_file_present": False,
    "env_file_secure": False,
    "renderer_present": False,
    "renderer_executable": False,
    "safe_for_chat": True,
    "limitations": [],
    "guardrails": [
        "read-only path and metadata check",
        "server-local values file contents are not read",
        "staging env file contents are not read",
        "renderer is not executed",
        "no Postgres, Huabaosi, Feishu, QiWe, provider, media, service, timer, release, or network action",
    ],
}

values_ok, values_reason = path_is_secure(
    values_file, require_regular=True, reject_group_or_world_readable=True
)
if path_present(values_reason):
    report["values_file_present"] = True
report["values_file_secure"] = values_ok
if not values_ok:
    add_limitation(report, f"values_file_{values_reason}")

env_ok, env_reason = path_is_secure(
    env_file,
    require_regular=True,
    allow_missing=True,
    reject_group_or_world_readable=True,
)
if path_present(env_reason):
    report["env_file_present"] = True
    report["env_file_secure"] = env_ok
    add_limitation(report, "env_file_already_present")
    if not env_ok:
        add_limitation(report, f"env_file_{env_reason}")
elif not env_ok:
    add_limitation(report, f"env_file_{env_reason}")

renderer_ok, renderer_reason = path_is_secure(
    renderer,
    require_regular=True,
    require_executable=True,
)
if path_present(renderer_reason):
    report["renderer_present"] = True
report["renderer_executable"] = renderer_ok
if not renderer_ok:
    add_limitation(report, f"renderer_{renderer_reason}")

if values_ok and renderer_ok and env_reason == "path_missing":
    report["ready_for_render_validation"] = True
    report["action_status"] = "ready_for_render_validation"
elif values_ok and renderer_ok and env_ok:
    report["action_status"] = "rendered_env_already_present"

print(
    "staging_runtime_values_observation="
    + json.dumps(report, sort_keys=True, separators=(",", ":"))
)
PY
