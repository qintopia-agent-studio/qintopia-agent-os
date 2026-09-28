#!/usr/bin/env bash
set -euo pipefail
umask 077
export PYTHONDONTWRITEBYTECODE=1

restart_targets=""
release_root="${QINTOPIA_RELEASE_ROOT:-/home/ubuntu/qintopia-agent-os-releases}"
profile_release_dir=""
profile_metadata=""
log_since=""
skip_erhua_provider_check=false
evidence_output=""
evidence_phase="activation"
erhua_service_active=false
erhua_provider_checked=false
erhua_activated_files_verified=false
emit_safe_failure() {
  local target="$1"
  local phase="$2"
  local subject="$3"
  echo "qintopia_smoke_release_safe_failure=target=${target};phase=${phase};subject=${subject}" >&2
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --restart-targets)
      restart_targets="${2:-}"
      shift 2
      ;;
    --release-root)
      release_root="${2:-}"
      shift 2
      ;;
    --profile-release-dir)
      profile_release_dir="${2:-}"
      shift 2
      ;;
    --profile-metadata)
      profile_metadata="${2:-}"
      shift 2
      ;;
    --log-since)
      log_since="${2:-}"
      shift 2
      ;;
    --skip-erhua-provider-check)
      skip_erhua_provider_check=true
      shift
      ;;
    --evidence-output)
      evidence_output="${2:-}"
      shift 2
      ;;
    --evidence-phase)
      evidence_phase="${2:-}"
      shift 2
      ;;
    -h | --help)
      echo "Usage: deploy/runner/smoke-release.sh --restart-targets <comma-separated-targets>"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

IFS=',' read -r -a targets <<<"$restart_targets"
hermes_systemd_user="${QINTOPIA_HERMES_SYSTEMD_USER:-ubuntu}"

system_services=(
  qintopia-message-sidecar.service
  qintopia-message-embedding-worker.service
  qintopia-message-identity-worker.service
  qintopia-agentos-raw-archive-worker.service
  qintopia-agentos-event-signal-worker.service
  qintopia-agentos-graph-projection-worker.service
  qintopia-agentos-member-profile-worker.service
  qintopia-agentos-daily-digest-worker.service
  qintopia-agentos-daily-digest-publisher.service
)

restart_system_services() {
  if ! systemctl daemon-reload; then
    emit_safe_failure qintopia-system-services daemon-reload systemd
    return 1
  fi
  for service in "${system_services[@]}"; do
    if ! systemctl restart "$service"; then
      emit_safe_failure qintopia-system-services restart "$service"
      return 1
    fi
    if ! systemctl is-active --quiet "$service"; then
      emit_safe_failure qintopia-system-services is-active "$service"
      return 1
    fi
  done
}

restart_hermes_service() {
  local target="$1"
  local service="$2"
  if ! runuser -l "$hermes_systemd_user" -c \
    "XDG_RUNTIME_DIR=/run/user/\$(id -u) systemctl --user restart ${service}"; then
    emit_safe_failure "$target" restart "$service"
    return 1
  fi
  if ! runuser -l "$hermes_systemd_user" -c \
    "XDG_RUNTIME_DIR=/run/user/\$(id -u) systemctl --user is-active --quiet ${service}"; then
    emit_safe_failure "$target" is-active "$service"
    return 1
  fi
}

smoke_erhua_profile() {
  local profile_dir="${QINTOPIA_ERHUA_PROFILE_DIR:-/home/ubuntu/.hermes/profiles/erhua}"
  local hermes_bin="${QINTOPIA_HERMES_BIN:-/home/ubuntu/.local/bin/hermes}"
  local hermes_python="${QINTOPIA_HERMES_PYTHON:-/home/ubuntu/.hermes/hermes-agent/venv/bin/python}"
  local approved_release="${profile_release_dir:-${release_root}/current}"
  local renderer="${approved_release}/runtime/hermes/render_profile_overlay.py"
  local migrator="${approved_release}/runtime/hermes/migrate_erhua_livecool_env.py"
  local overlay="${approved_release}/agents/erhua/config.template.yaml"
  local transaction="${approved_release}/runtime/hermes/profile_transaction.py"
  local runtime_verifier="${approved_release}/runtime/hermes/verify_runtime_provider.py"
  local doctor_output
  if [[ -n "$profile_metadata" ]]; then
    python3 "$transaction" verify-activated \
      --config "${profile_dir}/config.yaml" \
      --env "${profile_dir}/.env" \
      --backup-dir "$(dirname "$profile_metadata")" \
      --metadata "$profile_metadata"
    python3 "$renderer" verify --config "${profile_dir}/config.yaml" --overlay "$overlay"
    python3 "$migrator" check --env "${profile_dir}/.env"
  fi
  if ! runuser -l "$hermes_systemd_user" -c \
    "${hermes_python} ${runtime_verifier} --config ${profile_dir}/config.yaml" >/dev/null 2>&1; then
    echo "Erhua Hermes runtime did not resolve the Livecool provider" >&2
    return 1
  fi
  doctor_output="$(mktemp)"
  if ! runuser -l "$hermes_systemd_user" -c \
    "${hermes_bin} --profile erhua doctor" >"$doctor_output" 2>&1; then
    rm -f "$doctor_output"
    echo "Erhua Hermes provider resolution check failed" >&2
    return 1
  fi
  if grep -Fi "custom:livecool.net" "$doctor_output" | \
    grep -Eqi "not a recognised provider|not a recognized provider|unknown provider|unsupported provider|invalid provider|provider.*(error|failed)"; then
    rm -f "$doctor_output"
    echo "Erhua Hermes doctor did not recognise the Livecool provider" >&2
    return 1
  fi
  rm -f "$doctor_output"
  if [[ -n "$log_since" ]]; then
    local journal_output
    journal_output="$(mktemp)"
    if ! runuser -l "$hermes_systemd_user" -c \
      "XDG_RUNTIME_DIR=/run/user/\$(id -u) journalctl --user -u hermes-gateway-erhua.service --since '${log_since}' --no-pager" \
      >"$journal_output" 2>/dev/null; then
      rm -f "$journal_output"
      echo "Erhua journal check failed" >&2
      return 1
    fi
    if grep -Fi "custom:livecool.net" "$journal_output" | \
      grep -Eqi "unknown provider|not a recogni[sz]ed provider|unsupported provider|invalid provider|provider.*(error|failed)"; then
      rm -f "$journal_output"
      echo "Erhua logs contain the unknown Livecool provider error" >&2
      return 1
    fi
    rm -f "$journal_output"
  fi
  if [[ -n "$profile_metadata" ]]; then
    python3 "$transaction" verify-activated \
      --config "${profile_dir}/config.yaml" \
      --env "${profile_dir}/.env" \
      --backup-dir "$(dirname "$profile_metadata")" \
      --metadata "$profile_metadata"
    erhua_activated_files_verified=true
  fi
  erhua_provider_checked=true
}

for target in "${targets[@]}"; do
  case "$target" in
    qintopia-system-services)
      restart_system_services
      ;;
    hermes-erhua)
      restart_hermes_service hermes-erhua hermes-gateway-erhua.service
      erhua_service_active=true
      if [[ "$skip_erhua_provider_check" != "true" && -n "$profile_metadata" ]]; then
        smoke_erhua_profile
      fi
      ;;
    hermes-wenyuange)
      restart_hermes_service hermes-wenyuange hermes-gateway-wenyuange.service
      ;;
    hermes-xiaoman)
      restart_hermes_service hermes-xiaoman hermes-gateway-xiaoman.service
      ;;
    hermes-silaoshi)
      restart_hermes_service hermes-silaoshi hermes-gateway-silaoshi.service
      ;;
    hermes-huabaosi)
      restart_hermes_service hermes-huabaosi hermes-gateway-huabaosi.service
      ;;
    hermes-anan)
      anan_unit=qintopia-agent-os-anan-drain-restart.service
      anan_release="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")/../.." && pwd -P)"
      if [[ "$anan_release" != "${release_root}/"* || "$anan_release" == "${release_root}/current" ]]; then
        emit_safe_failure hermes-anan release-identity "$anan_unit"
        exit 1
      fi
      anan_helper="${anan_release}/runtime/hermes/restart_anan.py"
      if [[ ! -f "$anan_helper" || "$hermes_systemd_user" != ubuntu ]]; then
        emit_safe_failure hermes-anan helper-identity "$anan_unit"
        exit 1
      fi
      anan_load_state="$(systemctl show "$anan_unit" --property=LoadState --value 2>/dev/null)" || {
        emit_safe_failure hermes-anan unit-state "$anan_unit"
        exit 1
      }
      if [[ "$anan_load_state" != not-found ]]; then
        emit_safe_failure hermes-anan unit-busy "$anan_unit"
        exit 1
      fi
      anan_uid="$(id -u ubuntu)"
      anan_python=/home/ubuntu/.local/share/hermes-releases/v2026.9.21/venv/bin/python
      anan_output="$(mktemp)"
      if ! systemd-run --unit="$anan_unit" --service-type=oneshot --wait \
        --uid=ubuntu --gid=ubuntu \
        --property=TimeoutStartSec=infinity \
        --property=NoNewPrivileges=yes \
        --property=PrivateTmp=yes \
        --property=ProtectSystem=strict \
        --property=ProtectHome=read-only \
        --property=ReadWritePaths=/home/ubuntu/.hermes/profiles/anan \
        --property="Environment=XDG_RUNTIME_DIR=/run/user/${anan_uid}" \
        --property="Environment=DBUS_SESSION_BUS_ADDRESS=unix:path=/run/user/${anan_uid}/bus" \
        --property=Environment=XDG_CONFIG_HOME=/home/ubuntu/.config \
        --property=Environment=XDG_DATA_HOME=/home/ubuntu/.local/share \
        --property=Environment=XDG_STATE_HOME=/home/ubuntu/.local/state \
        --property=Environment=PYTHONDONTWRITEBYTECODE=1 \
        "$anan_python" "$anan_helper" >"$anan_output" 2>&1; then
        rm -f "$anan_output"
        emit_safe_failure hermes-anan drain-or-restart hermes-gateway-anan.service
        exit 1
      fi
      anan_invocation="$(sed -n "s/^Running as unit: ${anan_unit}; invocation ID: \([0-9a-f]\{32\}\)$/\1/p" "$anan_output")"
      anan_result="$(sed -n 's/^Finished with result: \([a-z-]*\)$/\1/p' "$anan_output")"
      anan_exit="$(sed -n 's/^Main processes terminated with: code=exited\/status=\([0-9]*\)$/\1/p' "$anan_output")"
      rm -f "$anan_output"
      if [[ ! "$anan_invocation" =~ ^[0-9a-f]{32}$ || "$anan_result" != success || "$anan_exit" != 0 ]]; then
        emit_safe_failure hermes-anan unit-result "$anan_unit"
        exit 1
      fi
      if [[ -n "${QINTOPIA_DEPLOY_REQUEST_ID:-}" ]]; then
        python3 - "${QINTOPIA_DEPLOY_RUNNER_STATE_DIR:-/var/lib/qintopia-agent-os-deploy}/results" \
          "$QINTOPIA_DEPLOY_REQUEST_ID" "$anan_invocation" "$anan_unit" \
          "$anan_python" "$anan_helper" "$anan_result" "$anan_exit" <<'PY'
import json
import os
import re
import sys
import tempfile

directory, request_id, invocation, unit, python, helper, result, status = sys.argv[1:9]
if not re.fullmatch(r"deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}", request_id):
    raise SystemExit("Anan drain request ID is invalid")
record = {
    "unit": unit, "invocation_id": invocation,
    "exec_start": [python, helper], "result": result,
    "exec_main_status": int(status),
}
fd, temporary = tempfile.mkstemp(prefix=".anan-drain-", dir=directory)
path = os.path.join(directory, request_id + ".anan-drain.json")
try:
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        json.dump(record, fh, sort_keys=True)
        fh.write("\n")
        fh.flush()
        os.fsync(fh.fileno())
    os.chmod(temporary, 0o600)
    os.replace(temporary, path)
    descriptor = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
finally:
    if os.path.exists(temporary):
        os.unlink(temporary)
PY
      fi
      echo "Anan drain unit: ${anan_unit} invocation=${anan_invocation} result=${anan_result} exit=${anan_exit}"
      ;;
    hermes-guanerye)
      restart_hermes_service hermes-guanerye hermes-gateway-guanerye.service
      ;;
    "")
      ;;
    *)
      echo "unsupported restart target: ${target}" >&2
      exit 2
      ;;
  esac
done

if [[ -n "$evidence_output" ]]; then
  python3 - "$evidence_output" "$erhua_service_active" "$erhua_provider_checked" \
    "$skip_erhua_provider_check" "$log_since" "$evidence_phase" \
    "$erhua_activated_files_verified" <<'PY'
import json
import os
import sys

(
    path,
    service_active,
    provider_checked,
    provider_skipped,
    log_since,
    phase,
    activated_files_verified,
) = sys.argv[1:8]
evidence = {
    "schema_version": 1,
    "agent_id": "erhua",
    "phase": phase,
    "service_active": service_active == "true",
    "provider_check": "skipped" if provider_skipped == "true" else "passed",
    "doctor_succeeded": provider_checked == "true",
    "runtime_provider_resolved": provider_checked == "true",
    "activated_files_verified": activated_files_verified == "true",
    "unknown_provider_absent": provider_checked == "true",
    "journal_checked": provider_checked == "true" and bool(log_since),
    "inference_called": False,
    "external_delivery": False,
}
temporary = path + ".tmp"
with open(temporary, "w", encoding="utf-8") as fh:
    json.dump(evidence, fh, indent=2)
    fh.write("\n")
    fh.flush()
    os.fsync(fh.fileno())
os.chmod(temporary, 0o600)
os.replace(temporary, path)
PY
fi

echo "Smoke checks passed for restart targets: ${restart_targets}"
