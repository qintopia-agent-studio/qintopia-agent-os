#!/usr/bin/env bash
set -euo pipefail
umask 077

release_dir=""
state_dir=""
request_id=""
release_sha=""
dry_run_request_id=""
dry_run=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release-dir) release_dir="${2:-}"; shift 2 ;;
    --state-dir) state_dir="${2:-}"; shift 2 ;;
    --request-id) request_id="${2:-}"; shift 2 ;;
    --release-sha) release_sha="${2:-}"; shift 2 ;;
    --dry-run-request-id) dry_run_request_id="${2:-}"; shift 2 ;;
    --dry-run) dry_run=true; shift ;;
    *) echo "unsupported Erhua activation argument" >&2; exit 2 ;;
  esac
done

if [[ ! "$request_id" =~ ^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$ ]]; then
  echo "invalid request id for Erhua activation" >&2
  exit 2
fi
if [[ ! "$release_sha" =~ ^[0-9a-f]{40}$ ]]; then
  echo "invalid release sha for Erhua activation" >&2
  exit 2
fi
if [[ "$dry_run" != "true" && ! "$dry_run_request_id" =~ ^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$ ]]; then
  echo "activation requires the reviewed Erhua dry-run request id" >&2
  exit 2
fi

profile_dir="${QINTOPIA_ERHUA_PROFILE_DIR:-/home/ubuntu/.hermes/profiles/erhua}"
default_config="${QINTOPIA_DEFAULT_HERMES_CONFIG:-/home/ubuntu/.hermes/config.yaml}"
config_path="${profile_dir}/config.yaml"
env_path="${profile_dir}/.env"
overlay="${release_dir}/agents/erhua/config.template.yaml"
renderer="${release_dir}/runtime/hermes/render_profile_overlay.py"
migrator="${release_dir}/runtime/hermes/migrate_erhua_livecool_env.py"
transaction="${release_dir}/runtime/hermes/profile_transaction.py"
runtime_verifier="${release_dir}/runtime/hermes/verify_runtime_provider.py"
hermes_python="${QINTOPIA_HERMES_PYTHON:-/home/ubuntu/.hermes/hermes-agent/venv/bin/python}"

if [[ ! -d "$profile_dir" || -L "$profile_dir" || "$(realpath "$profile_dir" 2>/dev/null || true)" != "$profile_dir" ]]; then
  echo "Erhua profile directory must not contain path aliases" >&2
  exit 1
fi

for path in "$config_path" "$env_path" "$default_config" "$overlay" "$renderer" "$migrator" "$transaction" "$runtime_verifier" "$hermes_python"; do
  if [[ ! -f "$path" || -L "$path" ]]; then
    echo "Erhua profile prerequisite is missing or aliased" >&2
    exit 1
  fi
done
if ! python3 -c "import yaml" >/dev/null 2>&1; then
  echo "Erhua profile activation requires PyYAML for the root Python runtime" >&2
  exit 1
fi

work_dir="$(mktemp -d)"
rollback_on_exit=false
cleanup() {
  local status=$?
  if [[ "$status" -ne 0 && "$rollback_on_exit" == "true" ]]; then
    if ! python3 "$transaction" restore --config "$config_path" --env "$env_path" \
      --backup-dir "$backup_dir" --metadata "$metadata"; then
      echo "Erhua activation cleanup restore failed" >&2
      status=70
    fi
  fi
  rm -rf "$work_dir"
  return "$status"
}
trap cleanup EXIT
trap 'exit 129' HUP
trap 'exit 130' INT
trap 'exit 143' TERM
candidate_config="${work_dir}/config.yaml"
candidate_env="${work_dir}/erhua.env"
config_report="${work_dir}/config-report.json"
env_report="${work_dir}/env-report.json"
evidence_dir="${state_dir}/results"
evidence_path="${evidence_dir}/${request_id}.profile.json"
dry_run_dir="${state_dir}/profile-dry-runs"
if [[ "$dry_run" == "true" ]]; then
  dry_run_marker="${dry_run_dir}/erhua-${release_sha}-${request_id}.json"
else
  dry_run_marker="${dry_run_dir}/erhua-${release_sha}-${dry_run_request_id}.json"
fi

python3 "$renderer" render --base "$config_path" --overlay "$overlay" \
  --output "$candidate_config" --report "$config_report"
python3 "$migrator" prepare --env "$env_path" --default-config "$default_config" \
  --output "$candidate_env" --report "$env_report"
python3 "$renderer" verify --config "$candidate_config" --overlay "$overlay"
python3 "$migrator" check --env "$candidate_env"
PYTHONDONTWRITEBYTECODE=1 "$hermes_python" "$runtime_verifier" \
  --config "$candidate_config" >/dev/null

write_evidence() {
  local phase="$1"
  local metadata_path="${2:-}"
  local approved_dry_run="${3:-}"
  mkdir -p "$evidence_dir"
  chmod 700 "$evidence_dir"
  python3 - "$config_report" "$env_report" "$evidence_path" "$phase" \
    "$metadata_path" "$release_sha" "$approved_dry_run" <<'PY'
import json
import os
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    config = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    env = json.load(fh)
env = {key: value for key, value in env.items() if key not in {"before_sha256", "after_sha256"}}
evidence = {
    "schema_version": 1,
    "agent_id": "erhua",
    "release_sha": sys.argv[6],
    "phase": sys.argv[4],
    "profile_overlay": config,
    "secret_binding": env,
    "inference_called": False,
    "external_delivery": False,
}
if sys.argv[7]:
    evidence["approved_dry_run_request_id"] = sys.argv[7]
if sys.argv[5]:
    with open(sys.argv[5], encoding="utf-8") as fh:
        transaction = json.load(fh)
    transaction.get("files", {}).get("env", {}).pop("sha256", None)
    transaction.get("activated", {}).get("env", {}).pop("sha256", None)
    evidence["file_transaction"] = transaction
temporary = sys.argv[3] + ".tmp"
with open(temporary, "w", encoding="utf-8") as fh:
    json.dump(evidence, fh, indent=2)
    fh.write("\n")
    fh.flush()
    os.fsync(fh.fileno())
os.chmod(temporary, 0o600)
os.replace(temporary, sys.argv[3])
print(json.dumps(evidence, separators=(",", ":")))
PY
}

write_dry_run_marker() {
  mkdir -p "$dry_run_dir"
  chmod 700 "$dry_run_dir"
  python3 - "$config_report" "$env_report" "$dry_run_marker" "$release_sha" \
    "$request_id" <<'PY'
import json
import os
import sys
from datetime import datetime, timezone

with open(sys.argv[1], encoding="utf-8") as fh:
    config = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    env = json.load(fh)
marker = {
    "schema_version": 1,
    "agent_id": "erhua",
    "phase": "dry_run",
    "release_sha": sys.argv[4],
    "dry_run_request_id": sys.argv[5],
    "created_at": datetime.now(timezone.utc).isoformat(),
    "profile_overlay": config,
    "secret_binding": env,
}
temporary = sys.argv[3] + ".tmp"
with open(temporary, "w", encoding="utf-8") as fh:
    json.dump(marker, fh, indent=2)
    fh.write("\n")
    fh.flush()
    os.fsync(fh.fileno())
os.chmod(temporary, 0o600)
os.replace(temporary, sys.argv[3])
PY
}

report_get() {
  python3 - "$1" "$2" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    value = json.load(fh).get(sys.argv[2])
if not isinstance(value, str) or not value:
    raise SystemExit("required profile report field is missing")
print(value)
PY
}

if [[ "$dry_run" == "true" ]]; then
  write_evidence dry_run
  write_dry_run_marker
  exit 0
fi

if [[ ! -f "$dry_run_marker" || -L "$dry_run_marker" ]]; then
  echo "matching Erhua dry-run evidence is required before activation" >&2
  exit 1
fi
python3 - "$dry_run_marker" "$config_report" "$env_report" "$release_sha" \
  "$dry_run_request_id" <<'PY'
import json
import sys
from datetime import datetime, timedelta, timezone

with open(sys.argv[1], encoding="utf-8") as fh:
    marker = json.load(fh)
with open(sys.argv[2], encoding="utf-8") as fh:
    config = json.load(fh)
with open(sys.argv[3], encoding="utf-8") as fh:
    env = json.load(fh)
if marker.get("phase") != "dry_run" or marker.get("release_sha") != sys.argv[4]:
    raise SystemExit("Erhua dry-run evidence release mismatch")
if marker.get("dry_run_request_id") != sys.argv[5]:
    raise SystemExit("Erhua dry-run evidence request mismatch")
try:
    created_at = datetime.fromisoformat(marker["created_at"])
except (KeyError, TypeError, ValueError) as exc:
    raise SystemExit("Erhua dry-run evidence timestamp is invalid") from exc
now = datetime.now(timezone.utc)
if created_at.tzinfo is None or created_at > now or now - created_at > timedelta(hours=24):
    raise SystemExit("Erhua dry-run evidence is older than 24 hours")
if marker.get("profile_overlay") != config or marker.get("secret_binding") != env:
    raise SystemExit("Erhua runtime state changed after the reviewed dry run")
PY

backup_dir="${state_dir}/profile-backups/${request_id}"
metadata="${backup_dir}/metadata.json"
expected_config_sha="$(report_get "$config_report" before_sha256)"
expected_env_sha="$(report_get "$env_report" before_sha256)"
python3 "$transaction" backup --config "$config_path" --env "$env_path" \
  --backup-dir "$backup_dir" --metadata "$metadata" \
  --expected-config-sha "$expected_config_sha" --expected-env-sha "$expected_env_sha"
rollback_on_exit=true
python3 "$transaction" activate --config "$config_path" --env "$env_path" \
  --backup-dir "$backup_dir" --metadata "$metadata" \
  --candidate-config "$candidate_config" --candidate-env "$candidate_env"
python3 "$renderer" verify --config "$config_path" --overlay "$overlay"
python3 "$migrator" check --env "$env_path"
write_evidence activated "$metadata" "$dry_run_request_id"
mv "$dry_run_marker" "${dry_run_marker}.used-${request_id}"
rollback_on_exit=false
