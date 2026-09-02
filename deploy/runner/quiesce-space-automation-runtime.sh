#!/usr/bin/env bash
set -euo pipefail

SYSTEMCTL="/usr/bin/systemctl"
DISPATCHER_TIMER="qintopia-agentos-automation-dispatcher.timer"
DISPATCHER_SERVICE="qintopia-agentos-automation-dispatcher.service"
EXECUTION_WORKER="qintopia-agentos-space-automation-execution-worker.service"

shutdown_status=0
loaded_count=0

for unit in "$DISPATCHER_TIMER" "$DISPATCHER_SERVICE" "$EXECUTION_WORKER"; do
  if ! observed="$("$SYSTEMCTL" show --property=LoadState --value "$unit" 2>/dev/null)"; then
    shutdown_status=1
    continue
  fi
  case "$observed" in
    loaded)
      loaded_count=$((loaded_count + 1))
      ;;
    not-found)
      if ! observed="$("$SYSTEMCTL" show --property=ActiveState --value "$unit" 2>/dev/null)" || [[ "$observed" != "inactive" ]]; then
        shutdown_status=1
      fi
      ;;
    *)
      shutdown_status=1
      ;;
  esac
done

# The first release containing these units may legitimately find none of them.
if [[ "$loaded_count" == "0" ]]; then
  if [[ "$shutdown_status" != "0" ]]; then
    echo "could not prove the pre-promotion Space automation runtime is absent" >&2
    exit "$shutdown_status"
  fi
  exit 0
fi
if [[ "$loaded_count" != "3" ]]; then
  shutdown_status=1
fi

if ! "$SYSTEMCTL" disable --now "$DISPATCHER_TIMER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if ! "$SYSTEMCTL" disable --now "$EXECUTION_WORKER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if ! "$SYSTEMCTL" stop "$DISPATCHER_SERVICE" >/dev/null 2>&1; then
  shutdown_status=1
fi
if ! "$SYSTEMCTL" stop "$EXECUTION_WORKER" >/dev/null 2>&1; then
  shutdown_status=1
fi
for unit in "$DISPATCHER_SERVICE" "$EXECUTION_WORKER" "$DISPATCHER_TIMER"; do
  if ! "$SYSTEMCTL" reset-failed "$unit" >/dev/null 2>&1; then
    shutdown_status=1
  fi
done

if "$SYSTEMCTL" is-enabled --quiet "$DISPATCHER_TIMER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if "$SYSTEMCTL" is-active --quiet "$DISPATCHER_TIMER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if "$SYSTEMCTL" is-enabled --quiet "$EXECUTION_WORKER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if "$SYSTEMCTL" is-active --quiet "$EXECUTION_WORKER" >/dev/null 2>&1; then
  shutdown_status=1
fi
if "$SYSTEMCTL" is-active --quiet "$DISPATCHER_SERVICE" >/dev/null 2>&1; then
  shutdown_status=1
fi

for unit in "$DISPATCHER_TIMER" "$DISPATCHER_SERVICE" "$EXECUTION_WORKER"; do
  if ! observed="$("$SYSTEMCTL" show --property=LoadState --value "$unit" 2>/dev/null)" || [[ "$observed" != "loaded" ]]; then
    shutdown_status=1
  fi
  if ! observed="$("$SYSTEMCTL" show --property=ActiveState --value "$unit" 2>/dev/null)" || [[ "$observed" != "inactive" ]]; then
    shutdown_status=1
  fi
done
for unit in "$DISPATCHER_TIMER" "$EXECUTION_WORKER"; do
  if ! observed="$("$SYSTEMCTL" show --property=UnitFileState --value "$unit" 2>/dev/null)" || [[ "$observed" != "disabled" ]]; then
    shutdown_status=1
  fi
done

if [[ "$shutdown_status" != "0" ]]; then
  echo "could not prove the pre-promotion Space automation runtime is disabled" >&2
  exit "$shutdown_status"
fi
