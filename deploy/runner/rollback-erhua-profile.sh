#!/usr/bin/env bash
set -euo pipefail
umask 077

release_dir=""
state_dir=""
request_id=""
evidence_output=""
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release-dir) release_dir="${2:-}"; shift 2 ;;
    --state-dir) state_dir="${2:-}"; shift 2 ;;
    --request-id) request_id="${2:-}"; shift 2 ;;
    --evidence-output) evidence_output="${2:-}"; shift 2 ;;
    *) echo "unsupported Erhua rollback argument" >&2; exit 2 ;;
  esac
done
if [[ ! "$request_id" =~ ^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$ ]]; then
  echo "invalid request id for Erhua rollback" >&2
  exit 2
fi

profile_dir="${QINTOPIA_ERHUA_PROFILE_DIR:-/home/ubuntu/.hermes/profiles/erhua}"
backup_dir="${state_dir}/profile-backups/${request_id}"
transaction="${release_dir}/runtime/hermes/profile_transaction.py"
metadata="${backup_dir}/metadata.json"
python3 "$transaction" restore \
  --config "${profile_dir}/config.yaml" \
  --env "${profile_dir}/.env" \
  --backup-dir "$backup_dir" \
  --metadata "$metadata"

if [[ -n "$evidence_output" ]]; then
  python3 - "$metadata" "$evidence_output" <<'PY'
import json
import os
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    metadata = json.load(fh)
files = metadata.get("files", {})
files.get("env", {}).pop("sha256", None)
evidence = {
    "schema_version": 1,
    "agent_id": "erhua",
    "phase": "restored",
    "files": files,
}
temporary = sys.argv[2] + ".tmp"
with open(temporary, "w", encoding="utf-8") as fh:
    json.dump(evidence, fh, indent=2)
    fh.write("\n")
    fh.flush()
    os.fsync(fh.fileno())
os.chmod(temporary, 0o600)
os.replace(temporary, sys.argv[2])
PY
fi
