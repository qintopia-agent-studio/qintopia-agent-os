#!/usr/bin/env bash
set -euo pipefail

# Read-only preflight for the one-time Hermes core detachment and later updates.
HERMES_CORE=/home/ubuntu/.hermes/hermes-agent
HERMES_PYTHON="$HERMES_CORE/venv/bin/python"
readonly MIN_FREE_KB=5242880
readonly SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd)"
readonly REPO_ROOT="$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd)"
readonly PROFILE_REGISTRY_READER="${REPO_ROOT}/tools/deploy/hermes-profile-registry.mjs"
readonly USER_SYSTEMCTL=(
  /usr/sbin/runuser -u ubuntu -- /usr/bin/env -i
  PATH=/usr/local/bin:/usr/bin:/bin
  XDG_RUNTIME_DIR=/run/user/1000
  DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/1000/bus
  /usr/bin/systemctl --user
)

node_bin=""
for candidate in /usr/bin/node /usr/local/bin/node /opt/homebrew/bin/node; do
  if [[ -x "$candidate" ]]; then
    node_bin="$candidate"
    break
  fi
done

errors=()
fail() {
  errors+=("$1")
}

units=()
if [[ -z "$node_bin" || ! -f "$PROFILE_REGISTRY_READER" ]]; then
  fail "hermes_profile_registry=unavailable"
elif ! mapfile -t units < <(
  /usr/bin/env -i PATH=/usr/local/bin:/usr/bin:/bin \
    "$node_bin" "$PROFILE_REGISTRY_READER" --services
); then
  fail "hermes_profile_registry=invalid"
  units=()
fi
if [[ "${#units[@]}" -ne 7 ]]; then
  fail "hermes_profile_registry=unexpected_service_count"
fi

if [[ ! -d "$HERMES_CORE/.git" ]]; then
  fail "hermes_core_git_checkout=missing"
else
  origin=$(git -C "$HERMES_CORE" config --get remote.origin.url || true)
  case "$origin" in
    https://github.com/NousResearch/hermes-agent|https://github.com/NousResearch/hermes-agent.git|git@github.com:NousResearch/hermes-agent|git@github.com:NousResearch/hermes-agent.git)
      ;;
    *)
      fail "hermes_core_origin=not_official"
      ;;
  esac

  dirty_count=$(git -C "$HERMES_CORE" status --porcelain | wc -l | tr -d ' ')
  if [[ "$dirty_count" != "0" ]]; then
    fail "hermes_core_worktree=dirty entries=$dirty_count"
  fi

  branch=$(git -C "$HERMES_CORE" branch --show-current)
  if [[ "$branch" != "main" ]]; then
    fail "hermes_core_branch=${branch:-detached}"
  fi

  upstream=$(git -C "$HERMES_CORE" rev-parse --abbrev-ref --symbolic-full-name '@{upstream}' 2>/dev/null || true)
  if [[ "$upstream" != "origin/main" ]]; then
    fail "hermes_core_upstream=${upstream:-missing}"
  fi

  if git -C "$HERMES_CORE" rev-parse --verify --quiet origin/main >/dev/null; then
    ahead_count=$(git -C "$HERMES_CORE" rev-list --count origin/main..HEAD)
    if [[ "$ahead_count" != "0" ]]; then
      fail "hermes_core_local_commits=$ahead_count"
    fi
  else
    fail "hermes_core_origin_main=missing"
  fi
fi

free_kb=$(df -Pk / | awk 'NR == 2 { print $4 }')
if [[ -z "$free_kb" || "$free_kb" -lt "$MIN_FREE_KB" ]]; then
  fail "root_free_kb=${free_kb:-unknown} required_kb=$MIN_FREE_KB"
fi

if [[ ! -x "$HERMES_PYTHON" ]]; then
  fail "hermes_python=missing"
else
  python_version=$("$HERMES_PYTHON" --version 2>&1 || true)
  hermes_version=$("$HERMES_PYTHON" -m hermes_cli.main --version 2>&1 | sed -n '1p' || true)
  printf 'hermes_python=%s\n' "$python_version"
  printf 'hermes_version=%s\n' "$hermes_version"
fi

for unit in "${units[@]}"; do
  if ! "${USER_SYSTEMCTL[@]}" is-active --quiet "$unit"; then
    fail "${unit}=not_active"
    continue
  fi
  exec_start=$("${USER_SYSTEMCTL[@]}" show "$unit" -p ExecStart --value 2>/dev/null || true)
  if [[ "$exec_start" != *"$HERMES_PYTHON"* ]]; then
    fail "${unit}=unexpected_execstart"
  fi
done

if ((${#errors[@]} > 0)); then
  printf 'hermes_core_readiness=blocked\n'
  printf '%s\n' "${errors[@]}"
  exit 1
fi

printf 'hermes_core_readiness=ready\n'
