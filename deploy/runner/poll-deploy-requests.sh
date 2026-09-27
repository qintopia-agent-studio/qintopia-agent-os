#!/usr/bin/env bash
set -euo pipefail

STATE_DIR="${QINTOPIA_DEPLOY_RUNNER_STATE_DIR:-/var/lib/qintopia-agent-os-deploy}"
ENV_FILE="${QINTOPIA_COS_ENV_FILE:-/etc/qintopia/cos-artifacts.env}"
RUNNER="${QINTOPIA_DEPLOY_RUNNER_BIN:-/home/ubuntu/qintopia-agent-os-releases/current/deploy/runner/qintopia-agent-os-deploy-runner}"

usage() {
  cat <<'USAGE'
Usage:
  deploy/runner/poll-deploy-requests.sh

Fetches the fixed production deploy request pointer from Tencent COS, runs the
referenced deploy request once, uploads the deploy result, and records local
idempotency state.
USAGE
}

if [[ "${1:-}" == "-h" || "${1:-}" == "--help" ]]; then
  usage
  exit 0
fi

if [[ -f "$ENV_FILE" ]]; then
  # shellcheck disable=SC1090
  source "$ENV_FILE"
fi

require_env() {
  if [[ -z "${!1:-}" ]]; then
    echo "$1 is required" >&2
    exit 2
  fi
}

require_env TENCENT_COS_BUCKET
require_env TENCENT_COS_REGION
require_env DEPLOY_REQUEST_SIGNING_KEY
require_env DEPLOY_REQUEST_SIGNING_KEY_ID

auth_mode="${TENCENT_COS_AUTH_MODE:-SecretKey}"
if [[ "$auth_mode" == "CvmRole" ]]; then
  require_env TENCENT_COS_CVM_ROLE_NAME
else
  require_env TENCENT_COS_SECRET_ID
  require_env TENCENT_COS_SECRET_KEY
fi

mkdir -p "${STATE_DIR}/requests/pending" "${STATE_DIR}/requests/processed" \
  "${STATE_DIR}/requests/failed" "${STATE_DIR}/requests/claimed" "${STATE_DIR}/results"

# The runner takes deploy.lock after this lock; recovery uses the same order.
exec 9>"${STATE_DIR}/poller.lock"
if command -v flock >/dev/null 2>&1; then
  flock -n 9 || { echo "another deploy request consumer is running" >&2; exit 75; }
elif command -v lockf >/dev/null 2>&1; then
  lockf -t 0 9 || { echo "another deploy request consumer is running" >&2; exit 75; }
else
  echo "no supported file locking utility is available" >&2
  exit 75
fi
if find "${STATE_DIR}/requests/claimed" -maxdepth 1 -name '*.json' -print -quit | grep -q .; then
  echo "an unfinished deploy request claim requires recovery" >&2
  exit 75
fi

tmp_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$tmp_dir"
}
trap cleanup EXIT
chmod 700 "$tmp_dir"
coscli_work_dir="${tmp_dir}/coscli-work"
mkdir -m 0700 "$coscli_work_dir"

coscli_path="${COSCLI_PATH:-}"
if [[ -z "$coscli_path" ]]; then
  if command -v coscli >/dev/null 2>&1; then
    coscli_path="$(command -v coscli)"
  else
    coscli_path="$(/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/install-coscli.sh --output "${tmp_dir}/coscli")"
  fi
fi

run_coscli() {
  (
    cd "$coscli_work_dir"
    "$coscli_path" "$@"
  )
}

config_path="${tmp_dir}/cos.yaml"
touch "$config_path"
if [[ "$auth_mode" == "CvmRole" ]]; then
  run_coscli config set \
    --mode CvmRole \
    --cvm_role_name "$TENCENT_COS_CVM_ROLE_NAME" \
    -c "$config_path" \
    --init-skip \
    --disable-log >/dev/null
else
  auth_args=(--mode SecretKey --secret_id "$TENCENT_COS_SECRET_ID" --secret_key "$TENCENT_COS_SECRET_KEY")
  if [[ -n "${TENCENT_COS_SESSION_TOKEN:-}" ]]; then
    auth_args+=(--session_token "$TENCENT_COS_SESSION_TOKEN")
  fi
  run_coscli config set \
    -c "$config_path" \
    --init-skip \
    --disable-log \
    "${auth_args[@]}" >/dev/null
fi

bucket_alias="${TENCENT_COS_BUCKET_ALIAS:-qintopia-agent-os-artifacts}"
bucket_config_args=(
  -b "$TENCENT_COS_BUCKET"
  -r "$TENCENT_COS_REGION"
  -a "$bucket_alias"
  -c "$config_path"
  --init-skip
  --disable-log
)
if [[ -n "${TENCENT_COS_ENDPOINT:-}" ]]; then
  bucket_config_args+=(-e "$TENCENT_COS_ENDPOINT")
fi
run_coscli config add "${bucket_config_args[@]}" >/dev/null

prefix="qintopia-agent-os"
pointer_key="${prefix}/deploy-requests/production/current.json"
pointer_file="${STATE_DIR}/requests/current.json"

is_object_missing_error() {
  [[ "$1" =~ (NoSuchKey|not[[:space:]]+found|not[[:space:]]+exist|does[[:space:]]+not[[:space:]]+exist|404|No[[:space:]]+such[[:space:]]+object|对象不存在) ]]
}

cos_cp_probe() {
  local source="$1"
  local destination="$2"
  local output_file="$3"
  run_coscli cp "$source" "$destination" \
    -c "$config_path" \
    2>"$output_file" \
    1>>"$output_file"
}

pointer_error="${tmp_dir}/pointer-cp.err"
set +e
cos_cp_probe "cos://${bucket_alias}/${pointer_key}" "$pointer_file" "$pointer_error"
pointer_status=$?
set -e
if [[ "$pointer_status" -ne 0 ]]; then
  pointer_error_text="$(tr '\n' ' ' <"$pointer_error")"
  if is_object_missing_error "$pointer_error_text"; then
    echo "No deploy request pointer found; idle: ${pointer_key}"
    exit 0
  fi
  echo "Deploy request pointer download failed: ${pointer_key}" >&2
  if [[ -n "$pointer_error_text" ]]; then
    echo "$pointer_error_text" >&2
  fi
  exit "$pointer_status"
fi

pointer_identity="$(python3 - "$pointer_file" "$prefix" <<'PY'
import json
import re
import sys

pointer_file, prefix = sys.argv[1:3]
request_id_pattern = re.compile(r"^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$")
try:
    with open(pointer_file, encoding="utf-8") as fh:
        pointer = json.load(fh)
    request_id = pointer.get("request_id", "")
    request_key = pointer.get("request_key", "")
    result_key = pointer.get("result_key", "")
    expected_request_key = f"{prefix}/deploy-requests/production/requests/{request_id}.json"
    expected_result_key = f"{prefix}/deploy-results/production/{request_id}.json"
    if (
        pointer.get("schema_version") == 1
        and pointer.get("environment") == "production"
        and pointer.get("repository") == "qintopia-agent-studio/qintopia-agent-os"
        and request_id_pattern.fullmatch(request_id)
        and request_key == expected_request_key
        and result_key == expected_result_key
    ):
        print(f"{request_id}\t{request_key}\t{result_key}")
except Exception:
    pass
PY
)"

if [[ -z "$pointer_identity" ]]; then
  echo "deploy request pointer is invalid" >&2
  exit 2
fi

request_id="${pointer_identity%%$'\t'*}"
expected_request_id="${QINTOPIA_EXPECTED_DEPLOY_REQUEST_ID:-}"
if [[ -n "$expected_request_id" && "$request_id" != "$expected_request_id" ]]; then
  echo "deploy request pointer does not match expected request ID" >&2
  exit 75
fi
remaining_identity="${pointer_identity#*$'\t'}"
request_key="${remaining_identity%%$'\t'*}"
result_key="${remaining_identity#*$'\t'}"
request_name="${request_id}.json"
request_file="${STATE_DIR}/requests/pending/${request_name}"
remote_result_probe="${tmp_dir}/${request_id}-existing-result.json"
remote_result_error="${tmp_dir}/${request_id}-result-cp.err"
set +e
cos_cp_probe "cos://${bucket_alias}/${result_key}" "$remote_result_probe" "$remote_result_error"
remote_result_status=$?
set -e
if [[ "$remote_result_status" -eq 0 ]]; then
  echo "Deploy request result already exists; idle: ${request_id}"
  exit 0
fi
remote_result_error_text="$(tr '\n' ' ' <"$remote_result_error")"
if ! is_object_missing_error "$remote_result_error_text"; then
  echo "Deploy request result probe failed: ${result_key}" >&2
  if [[ -n "$remote_result_error_text" ]]; then
    echo "$remote_result_error_text" >&2
  fi
  exit "$remote_result_status"
fi
if [[ -e "${STATE_DIR}/requests/processed/${request_name}" ]]; then
  echo "Deploy request already processed; idle: ${request_id}"
  exit 0
fi
if [[ -e "${STATE_DIR}/requests/failed/${request_name}" ]]; then
  echo "Deploy request already failed; idle until current.json changes: ${request_id}" >&2
  exit 0
fi
run_coscli cp "cos://${bucket_alias}/${request_key}" "$request_file" \
  -c "$config_path" \
  --disable-log

parsed_identity="$(python3 - "$request_file" "$request_id" "$prefix" "$request_key" <<'PY'
import json
import re
import sys

request_file, expected_id, prefix, actual_request_key = sys.argv[1:5]
request_id_pattern = re.compile(r"^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$")
try:
    with open(request_file, encoding="utf-8") as fh:
        data = json.load(fh)
    request_id = data.get("request_id", "")
    request_key = data.get("cos", {}).get("request_key", "")
    result_key = data.get("cos", {}).get("result_key", "")
    expected_request_key = f"{prefix}/deploy-requests/production/requests/{request_id}.json"
    expected_result_key = f"{prefix}/deploy-results/production/{request_id}.json"
    if (
        request_id == expected_id
        and request_id_pattern.fullmatch(request_id)
        and request_key == actual_request_key
        and request_key == expected_request_key
        and result_key == expected_result_key
    ):
        print(f"{request_id}\t{result_key}")
except Exception:
    pass
PY
)"
if [[ -n "$parsed_identity" ]]; then
  request_id="${parsed_identity%%$'\t'*}"
  result_key="${parsed_identity#*$'\t'}"
fi
if [[ -n "$expected_request_id" && "$request_id" != "$expected_request_id" ]]; then
  echo "downloaded deploy request does not match expected request ID" >&2
  exit 75
fi
result_file="${STATE_DIR}/results/${request_id}.json"

python3 - "$request_file" "$pointer_file" "${STATE_DIR}/requests/claimed/${request_name}" \
    "$RUNNER" "${QINTOPIA_EXPECTED_DEPLOY_REQUEST_ID:-}" \
    "${STATE_DIR}/recovery/takeover.json" "$request_id" "$parsed_identity" <<'PY'
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

request_path, pointer_path, claim_path, runner_path, expected_id, takeover_path, request_id, parsed = sys.argv[1:9]
with open(request_path, "rb") as fh:
    request_bytes = fh.read()
with open(pointer_path, "rb") as fh:
    pointer_bytes = fh.read()
claim = {
    "request_id": request_id,
    "request_sha256": hashlib.sha256(request_bytes).hexdigest(),
    "pointer_sha256": hashlib.sha256(pointer_bytes).hexdigest(),
    "phase": "possibly_executing",
    "recovery_eligible": bool(parsed),
    "result_upload": {"phase": "not_started"},
    "hold_token": request_id,
}
if parsed:
    request = json.loads(request_bytes)
    if request.get("request_id") != request_id:
        raise SystemExit("deploy claim request identity mismatch")
    runner = Path(runner_path).resolve(strict=True)
    if not runner.is_file():
        raise SystemExit("deploy runner execution file is absent")
    with runner.open("rb") as fh:
        execution_sha256 = hashlib.sha256(fh.read()).hexdigest()
    unit = ("qintopia-agent-os-fixed-takeover.service" if expected_id else
            "qintopia-agent-os-deploy-runner.service")
    invocation = os.environ.get("INVOCATION_ID", "")
    if invocation and not re.fullmatch(r"[0-9a-f]{32}", invocation):
        raise SystemExit("consumer invocation identity is invalid")
    if invocation:
        shown = subprocess.run(
            ["systemctl", "show", unit, "--property=InvocationID", "--value"],
            capture_output=True, text=True, timeout=5, check=False)
        if shown.returncode != 0 or shown.stdout.strip() != invocation:
            raise SystemExit("consumer systemd invocation identity is not verified")
    claim["execution"] = {"path": str(runner), "sha256": execution_sha256,
                          "unit": unit, "invocation_id": invocation,
                          "unit_invocation_verified": bool(invocation)}
if expected_id and parsed:
    if expected_id != request_id:
        raise SystemExit("fixed takeover claim request mismatch")
    with open(takeover_path, encoding="utf-8") as fh:
        takeover = json.load(fh)
    token = takeover.get("hold_token", "")
    if (takeover.get("request_id") != expected_id or not isinstance(token, str) or
            not re.fullmatch(r"[0-9a-f]{32}", token)):
        raise SystemExit("fixed takeover claim hold identity mismatch")
    claim["hold_token"] = token
descriptor = os.open(claim_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(descriptor, "w", encoding="utf-8") as fh:
    json.dump(claim, fh, sort_keys=True)
    fh.write("\n")
    fh.flush()
    os.fsync(fh.fileno())
directory = os.open(os.path.dirname(claim_path), os.O_RDONLY | os.O_DIRECTORY)
try:
    os.fsync(directory)
finally:
    os.close(directory)
PY

if [[ -n "$expected_request_id" && -n "$parsed_identity" ]]; then
  # The fixed takeover runs an old runner. Hold its deploy lock across the
  # journal snapshot, execution, result upload, and archive finalization.
  fixed_runner="/home/ubuntu/qintopia-agent-os-releases/16e8d56b98001579c6288ba13199b80d6d3dfc74/deploy/runner/qintopia-agent-os-deploy-runner"
  fixed_runner_sha256="04b27ea6900dec7078b3dfb56a28e0f5af3f9b58413b2df85ecaac54784b4dcf"
  [[ "$RUNNER" == "$fixed_runner" && ! -L "$RUNNER" &&
    "$(readlink -f "$RUNNER")" == "$fixed_runner" &&
    "$(sha256sum "$RUNNER" | awk '{print $1}')" == "$fixed_runner_sha256" ]] || {
    echo "fixed takeover old runner identity mismatch" >&2
    exit 75
  }
  exec 7>"${STATE_DIR}/deploy.lock"
  flock -n 7 || { echo "deploy lock is held before fixed takeover" >&2; exit 75; }
  fixed_fd_inode="$(stat -Lc '%d:%i' /proc/self/fd/7)" || exit 75
  fixed_lock_inode="$(stat -Lc '%d:%i' "${STATE_DIR}/deploy.lock")" || exit 75
  [[ -n "$fixed_fd_inode" && "$fixed_fd_inode" == "$fixed_lock_inode" ]] || {
    echo "fixed takeover inherited lock descriptor mismatch" >&2
    exit 75
  }
  mkdir -p -m 0700 "${STATE_DIR}/recovery"
  python3 - "$request_file" "${STATE_DIR}/recovery/${request_id}.json" \
    "${STATE_DIR}/requests/claimed/${request_name}" <<'PY'
import hashlib
import hmac
import json
import os
import re
import sys
import tempfile
from pathlib import Path

request_path, journal_path, claim_path = sys.argv[1:4]
root = Path("/home/ubuntu/qintopia-agent-os-releases").resolve(strict=True)
with open(request_path, "rb") as fh:
    request_bytes = fh.read()
request = json.loads(request_bytes)
with open(claim_path, encoding="utf-8") as fh:
    claim = json.load(fh)
if (claim.get("request_id") != request["request_id"] or
        claim.get("request_sha256") != hashlib.sha256(request_bytes).hexdigest() or
        claim.get("result_upload") != {"phase": "not_started"}):
    raise SystemExit("fixed takeover claim identity or upload phase changed")
signature = request.get("signature", {})
metadata = {key: signature.get(key) for key in ("algorithm", "issuer", "key_id", "signed_at")}
if (metadata["algorithm"] != "hmac-sha256" or metadata["issuer"] != "github-actions" or
        metadata["key_id"] != os.environ.get("DEPLOY_REQUEST_SIGNING_KEY_ID")):
    raise SystemExit("fixed takeover request signature metadata is invalid")
def canonical(value):
    if isinstance(value, list):
        return "[" + ",".join(canonical(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(k, separators=(",", ":")) + ":" + canonical(value[k]) for k in sorted(value)) + "}"
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
unsigned = dict(request)
unsigned.pop("signature", None)
expected = hmac.new(os.environ["DEPLOY_REQUEST_SIGNING_KEY"].encode(),
                    canonical({"request": unsigned, "signature": metadata}).encode(), hashlib.sha256).hexdigest()
if not hmac.compare_digest(signature.get("value", ""), expected):
    raise SystemExit("fixed takeover request signature is invalid")
old = "16e8d56b98001579c6288ba13199b80d6d3dfc74"
prior = "83d694f2c3bc21fd78a73d25da3197379e2a14d5"
target = "70e7984fab92ddab956009585212d0e9729767b5"
if (request.get("release_sha") != target or request.get("commit_sha") != old or
        request.get("runtime_sha") != prior or request.get("release_scope") != ["deploy-bundle"] or
        request.get("restart_targets") != ["qintopia-system-services"] or request.get("dry_run") is not False or
        not re.fullmatch(r"[0-9a-f]{40}", request.get("deploy_bundle_sha", ""))):
    raise SystemExit("fixed takeover request tuple is invalid")
digests = {}
for name, sha in (("current", old), ("previous", prior)):
    link = root / name
    if not link.is_symlink() or link.resolve(strict=True) != root / sha:
        raise SystemExit("fixed takeover lineage differs from O/P")
    with open(root / sha / "manifest.json", "rb") as fh:
        digests[name] = hashlib.sha256(fh.read()).hexdigest()
record = {"schema_version": 1, "request_id": request["request_id"],
          "request_sha256": hashlib.sha256(request_bytes).hexdigest(), "direction": "O→T",
          "original_current_sha": old, "original_previous_sha": prior,
          "manifest_sha256": digests, "phase": "intent",
          "execution": claim["execution"], "hold_token": claim["hold_token"],
          "result_upload": {"phase": "not_started"}}
if os.path.exists(journal_path):
    raise SystemExit("fixed takeover journal already exists")
fd, temporary = tempfile.mkstemp(prefix=".journal-", dir=os.path.dirname(journal_path))
try:
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        json.dump(record, fh, sort_keys=True)
        fh.write("\n")
        fh.flush()
        os.fsync(fh.fileno())
    os.chmod(temporary, 0o600)
    os.replace(temporary, journal_path)
    directory = os.open(os.path.dirname(journal_path), os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)
finally:
    if os.path.exists(temporary):
        os.unlink(temporary)
PY
fi

runner_status=0
fallback_written=false
fallback_error="deploy request failed before promotion result was written"
if [[ -z "$parsed_identity" ]]; then
  runner_status=2
  fallback_error="deploy request key or identity is invalid"
else
  set +e
  if [[ -n "$expected_request_id" ]]; then
    QINTOPIA_FIXED_TAKEOVER_LOCK=1
    export QINTOPIA_FIXED_TAKEOVER_LOCK
    flock() {
      local fd_inode="" lock_inode=""
      if [[ "${QINTOPIA_FIXED_TAKEOVER_LOCK:-}" == 1 && "$#" -eq 2 &&
            "$1" == -n && "$2" == 9 && "$0" == "${QINTOPIA_DEPLOY_RUNNER_BIN:-}" ]]; then
        [[ "$0" == "$fixed_runner" && "$(sha256sum "$0" | awk '{print $1}')" == "$fixed_runner_sha256" ]] || return 75
        fd_inode="$(stat -Lc '%d:%i' /proc/self/fd/7)" || return 75
        lock_inode="$(stat -Lc '%d:%i' "${QINTOPIA_DEPLOY_RUNNER_STATE_DIR}/deploy.lock")" || return 75
        [[ -n "$fd_inode" && "$fd_inode" == "$lock_inode" ]] || return 75
        command flock -n 7
      else
        command flock "$@"
      fi
    }
    export fixed_runner fixed_runner_sha256
    export -f flock
  fi
  "$RUNNER" --request-file "$request_file"
  runner_status=$?
  if [[ -n "$expected_request_id" ]]; then
    unset -f flock
    unset QINTOPIA_FIXED_TAKEOVER_LOCK
  fi
  set -e
fi

if [[ "$runner_status" -ne 0 && ! -f "$result_file" ]]; then
  fallback_written=true
  python3 - "$result_file" "$request_file" "$request_id" "$fallback_error" <<'PY'
import hashlib
import hmac
import json
import os
import re
import sys
from datetime import datetime, timezone

path, request_file, request_id, error = sys.argv[1:5]
now = datetime.now(timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")
sha_pattern = re.compile(r"^[0-9a-f]{40}$")
sha256_pattern = re.compile(r"^[0-9a-f]{64}$")
approved_profiles = {"huabaosi-production", "qiwe-production"}

try:
    with open(request_file, encoding="utf-8") as fh:
        request = json.load(fh)
except Exception:
    request = {}


def normalized_sha(value):
    value = str(value or "")
    return value if sha_pattern.fullmatch(value) else "0" * 40


def normalized_sha256(value):
    value = str(value or "")
    return value if sha256_pattern.fullmatch(value) else "0" * 64


runtime_artifact_profile = str(request.get("runtime_artifact_profile") or "")
if runtime_artifact_profile not in approved_profiles:
    runtime_artifact_profile = "huabaosi-production"

release_scope = request.get("release_scope")
if not isinstance(release_scope, list) or not release_scope:
    release_scope = ["sidecar-runtime"]

restart_targets = request.get("restart_targets")
if not isinstance(restart_targets, list) or not restart_targets:
    restart_targets = ["qintopia-system-services"]

is_hermes_core_release = release_scope == ["hermes-core-release"]
core_keys = {
    "repository", "tag", "commit_sha", "source_archive_sha256",
    "artifact_identity_sha256", "artifact_manifest_sha256",
    "previous_version", "previous_commit_sha", "archive_sha256",
}
hermes_core_request = request.get("hermes_core_release")
core_request_valid = (
    is_hermes_core_release
    and isinstance(hermes_core_request, dict)
    and set(hermes_core_request) == core_keys
    and hermes_core_request.get("repository") == "https://github.com/NousResearch/hermes-agent.git"
    and isinstance(hermes_core_request.get("tag"), str)
    and re.fullmatch(r"^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$", hermes_core_request.get("tag", ""))
    and all(re.fullmatch(r"^[0-9a-f]{40}$", hermes_core_request.get(key, "")) for key in ("commit_sha", "previous_commit_sha"))
    and all(re.fullmatch(r"^[0-9a-f]{64}$", hermes_core_request.get(key, "")) for key in ("source_archive_sha256", "artifact_identity_sha256", "artifact_manifest_sha256", "archive_sha256"))
    and isinstance(hermes_core_request.get("previous_version"), str)
    and re.fullmatch(r"^[A-Za-z0-9][A-Za-z0-9._+-]{0,127}$", hermes_core_request.get("previous_version", ""))
    and hermes_core_request.get("commit_sha") != hermes_core_request.get("previous_commit_sha")
)
if core_request_valid:
    result = {
        "schema_version": 1,
        "request_id": request_id,
        "environment": "production",
        "status": "failed",
        "started_at": now,
        "finished_at": now,
        "release_scope": ["hermes-core-release"],
        "previous_sha": "",
        "current_target": "",
        "restart_targets": ["hermes-core"],
        "checks": [{"name": "deploy-request-validation", "status": "failed"}],
        "rollback": {"attempted": False, "status": "not_needed"},
        "error": error,
        "hermes_core": {
            "tag": hermes_core_request["tag"],
            "commit_sha": hermes_core_request["commit_sha"],
            "source_archive_sha256": hermes_core_request["source_archive_sha256"],
            "artifact_identity_sha256": hermes_core_request["artifact_identity_sha256"],
            "artifact_manifest_sha256": hermes_core_request["artifact_manifest_sha256"],
            "previous_commit_sha": hermes_core_request["previous_commit_sha"],
            "new_version": None,
            "transaction_status": "failed",
        },
    }
elif is_hermes_core_release:
    result = {
        "schema_version": 1,
        "request_id": request_id,
        "environment": "production",
        "status": "failed",
        "started_at": now,
        "finished_at": now,
        "release_scope": ["production-observation"],
        "previous_sha": "",
        "current_target": "",
        "restart_targets": ["qintopia-system-services"],
        "checks": [{"name": "deploy-request-validation", "status": "failed"}],
        "rollback": {"attempted": False, "status": "not_needed"},
        "validation_failure": True,
        "error": error,
    }
else:
    result = {
    "schema_version": 1,
    "request_id": request_id,
    "environment": "production",
    "status": "failed",
    "started_at": now,
    "finished_at": now,
    "release_sha": normalized_sha(request.get("release_sha")),
    "commit_sha": normalized_sha(request.get("commit_sha")),
    "runtime_sha": normalized_sha(request.get("runtime_sha")),
    "runtime_artifact_profile": runtime_artifact_profile,
    "deploy_bundle_sha": normalized_sha(request.get("deploy_bundle_sha")),
    "release_scope": release_scope,
    "previous_sha": "",
    "current_target": "",
    "restart_targets": restart_targets,
    "checks": [{"name": "deploy-request-validation", "status": "failed"}],
    "rollback": {"attempted": False, "status": "not_needed"},
    "error": error,
    }
signing_key = os.environ.get("DEPLOY_REQUEST_SIGNING_KEY", "")
signing_key_id = os.environ.get("DEPLOY_REQUEST_SIGNING_KEY_ID", "")
if not signing_key or not signing_key_id:
    raise SystemExit("deploy result signing key and key id are required")
signature_metadata = {
    "algorithm": "hmac-sha256",
    "issuer": "qintopia-deploy-runner",
    "key_id": signing_key_id,
    "signed_at": now,
}

def canonical_json(value):
    if isinstance(value, list):
        return "[" + ",".join(canonical_json(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{" + ",".join(
            json.dumps(key, separators=(",", ":")) + ":" + canonical_json(value[key])
            for key in sorted(value)
        ) + "}"
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))

result["signature"] = {
    **signature_metadata,
    "value": hmac.new(
        signing_key.encode("utf-8"),
        canonical_json({"result": result, "signature": signature_metadata}).encode("utf-8"),
        hashlib.sha256,
    ).hexdigest(),
}
with open(path, "w", encoding="utf-8") as fh:
    json.dump(result, fh, ensure_ascii=False, indent=2)
    fh.write("\n")
PY
fi

if [[ -f "$result_file" ]]; then
  "$(dirname "${BASH_SOURCE[0]}")/wait-deploy-result.sh" \
    --request-file "$request_file" --result-file "$result_file" >/dev/null
  python3 - "${STATE_DIR}/requests/claimed/${request_name}" \
    "${STATE_DIR}/recovery/${request_id}.json" "$result_file" "$request_id" <<'PY'
import hashlib
import json
import os
import sys
import tempfile
from datetime import datetime, timezone

claim_path, journal_path, result_path, request_id = sys.argv[1:5]
with open(result_path, "rb") as fh:
    digest = hashlib.sha256(fh.read()).hexdigest()
intent = {"phase": "upload_intent", "payload_sha256": digest,
          "recorded_at": datetime.now(timezone.utc).isoformat()}
def replace(path):
    with open(path, encoding="utf-8") as fh:
        record = json.load(fh)
    if (record.get("request_id") != request_id or
            record.get("result_upload") != {"phase": "not_started"}):
        raise SystemExit("result upload evidence is missing or already started")
    record["result_upload"] = intent
    fd, temporary = tempfile.mkstemp(prefix=".upload-intent-", dir=os.path.dirname(path))
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            json.dump(record, fh, sort_keys=True)
            fh.write("\n")
            fh.flush()
            os.fsync(fh.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
        directory = os.open(os.path.dirname(path), os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
if os.path.isfile(journal_path):
    replace(journal_path)
replace(claim_path)
PY
  run_coscli cp "$result_file" "cos://${bucket_alias}/${result_key}" \
    -c "$config_path" \
    --disable-log
  verified_result="${tmp_dir}/${request_id}-uploaded-result.json"
  run_coscli cp "cos://${bucket_alias}/${result_key}" "$verified_result" \
    -c "$config_path" --disable-log
  cmp -s "$result_file" "$verified_result" || {
    echo "uploaded deploy result differs from local signed result" >&2
    exit 75
  }
fi

if [[ "$runner_status" -eq 0 ]]; then
  archive_dir="${STATE_DIR}/requests/processed"
else
  archive_dir="${STATE_DIR}/requests/failed"
fi

mv "$request_file" "${archive_dir}/${request_name}"

if [[ -n "$parsed_identity" && -f "$result_file" && "$fallback_written" != true ]]; then
  archived_pointer="${tmp_dir}/${request_id}-archived-pointer.json"
  run_coscli cp "cos://${bucket_alias}/${pointer_key}" "$archived_pointer" \
    -c "$config_path" --disable-log
  cmp -s "$pointer_file" "$archived_pointer" || {
    echo "deploy pointer changed before claim finalization" >&2
    exit 75
  }
  python3 - "${STATE_DIR}/requests/claimed/${request_name}" <<'PY'
import os
import sys

path = sys.argv[1]
os.unlink(path)
directory = os.open(os.path.dirname(path), os.O_RDONLY | os.O_DIRECTORY)
try:
    os.fsync(directory)
finally:
    os.close(directory)
PY
fi

if [[ -z "$parsed_identity" ]]; then
  python3 - "${STATE_DIR}/requests/claimed/${request_name}" <<'PY'
import os
import sys
path = sys.argv[1]
os.unlink(path)
directory = os.open(os.path.dirname(path), os.O_RDONLY | os.O_DIRECTORY)
try:
    os.fsync(directory)
finally:
    os.close(directory)
PY
fi

exit "$runner_status"
