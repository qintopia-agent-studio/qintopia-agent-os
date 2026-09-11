#!/usr/bin/env bash
set -euo pipefail

readonly STATE_ROOT=/var/lib/qintopia-agent-os-deploy
readonly INGRESS_ROOT="${STATE_ROOT}/hermes-core-ingress"
readonly DOWNLOAD_ROOT="${STATE_ROOT}/hermes-core-download"
readonly QUARANTINE_ROOT="${STATE_ROOT}/hermes-core-quarantine"
readonly INGRESS_LOCK="${STATE_ROOT}/hermes-core-ingress.lock"
readonly COS_BUCKET_ALIAS=qintopia-agent-os-hermes-core
readonly COS_OBJECT_PREFIX=qintopia-agent-os/hermes-core
readonly SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly REPO_ROOT="$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd -P)"
readonly EXTRACTOR="${REPO_ROOT}/tools/deploy/extract-hermes-core-artifact.py"
readonly VERIFIER="${REPO_ROOT}/tools/deploy/verify-hermes-core-artifact.mjs"
readonly FLOCK_BIN=/usr/bin/flock
readonly TIMEOUT_BIN=/usr/bin/timeout
readonly PRLIMIT_BIN=/usr/bin/prlimit
readonly MAX_ARCHIVE_BYTES=$((512 * 1024 * 1024))
readonly MAX_COSCLI_LOG_BYTES=$((16 * 1024 * 1024))
readonly DOWNLOAD_TIMEOUT_SECONDS=600
readonly MAX_FAILED_QUARANTINES=2

expected_archive_sha256=""
expected_tag=""
expected_commit=""
expected_source_sha256=""
expected_identity_sha256=""
expected_manifest_sha256=""
attempt_dir=""
staging_artifact=""
config_path=""
coscli_log=""
archive_path=""
result_code=hermes_core_artifact_fetch_failed

usage() {
  cat <<'USAGE'
Usage:
  deploy/runner/fetch-hermes-core-artifact.sh \
    --expected-archive-sha256 <sha256> \
    --expected-tag <tag> \
    --expected-commit <40-hex-commit> \
    --expected-source-sha256 <sha256> \
    --expected-identity-sha256 <sha256> \
    --expected-manifest-sha256 <sha256>

Downloads the fixed Hermes core archive for the expected commit into the
root-owned ingress boundary. The COS object key is derived internally from the
fixed qintopia-agent-os/hermes-core prefix and the expected commit.

COS authentication is read from the server environment or the fixed
/etc/qintopia/cos-artifacts.env file. SecretKey values are written only to a
private temporary coscli YAML file; they are never command arguments or output.
USAGE
}

emit_blocked() {
  printf 'hermes_core_artifact_fetch=blocked\n' >&2
  printf 'hermes_core_artifact_error=%s\n' "$result_code" >&2
}

emit_ready() {
  local idempotent="$1"
  printf 'hermes_core_artifact_fetch=ready\n'
  printf 'hermes_core_artifact_tag=%s\n' "$expected_tag"
  printf 'hermes_core_artifact_commit=%s\n' "$expected_commit"
  printf 'hermes_core_artifact_idempotent=%s\n' "$idempotent"
}

cleanup_sensitive() {
  local failed=0
  if [[ -n "$config_path" ]]; then
    if /bin/rm -f -- "$config_path" 2>/dev/null; then
      config_path=""
    else
      failed=1
    fi
  fi
  if [[ -n "$coscli_log" ]]; then
    if /bin/rm -f -- "$coscli_log" 2>/dev/null; then
      coscli_log=""
    else
      failed=1
    fi
  fi
  if [[ -n "$attempt_dir" && ( -e "${attempt_dir}/home" || -L "${attempt_dir}/home" ) ]]; then
    if [[ -L "${attempt_dir}/home" || ! -d "${attempt_dir}/home" ]]; then
      failed=1
    else
      /bin/rm -rf -- "${attempt_dir}/home" 2>/dev/null || failed=1
    fi
  fi
  return "$failed"
}

cleanup_untrusted_archive() {
  if [[ -n "$archive_path" ]]; then
    /bin/rm -f -- "$archive_path" 2>/dev/null || return 1
    archive_path=""
  fi
}

quarantine_attempt() {
  if [[ -n "$staging_artifact" && ( -e "$staging_artifact" || -L "$staging_artifact" ) ]]; then
    [[ -n "$attempt_dir" && -d "$attempt_dir" && ! -L "$attempt_dir" ]] || return 1
    /bin/chmod 0700 "$staging_artifact" 2>/dev/null || return 1
    /bin/mv -- "$staging_artifact" "${attempt_dir}/artifact-staging" || return 1
    staging_artifact=""
  fi
  if [[ -z "$attempt_dir" || ! -e "$attempt_dir" && ! -L "$attempt_dir" ]]; then
    return 0
  fi
  local slot
  slot="$(mktemp -d "${QUARANTINE_ROOT}/failed-${expected_commit}.XXXXXX")" || return 1
  /bin/rmdir "$slot" 2>/dev/null || return 1
  if ! /bin/mv -- "$attempt_dir" "$slot"; then
    return 1
  fi
  attempt_dir=""
  return 0
}

on_exit() {
  local status=$?
  if [[ "$status" -ne 0 ]]; then
    local cleanup_failed=0
    cleanup_sensitive || cleanup_failed=1
    cleanup_untrusted_archive || cleanup_failed=1
    if [[ "$cleanup_failed" -ne 0 ]]; then
      result_code=sensitive_cleanup_failed
    elif ! quarantine_attempt; then
      result_code=quarantine_failed
    fi
    emit_blocked
  else
    cleanup_sensitive
  fi
  exit "$status"
}
trap on_exit EXIT

if [[ "${EUID}" -ne 0 ]]; then
  result_code=root_required
  exit 1
fi

if [[ "${1:-}" == "--help" || "${1:-}" == "-h" ]]; then
  usage
  exit 0
fi

while [[ $# -gt 0 ]]; do
  case "$1" in
    --expected-archive-sha256)
      [[ $# -ge 2 && -z "$expected_archive_sha256" ]] || { result_code=invalid_invocation; exit 2; }
      expected_archive_sha256="$2"
      shift 2
      ;;
    --expected-tag)
      [[ $# -ge 2 && -z "$expected_tag" ]] || { result_code=invalid_invocation; exit 2; }
      expected_tag="$2"
      shift 2
      ;;
    --expected-commit)
      [[ $# -ge 2 && -z "$expected_commit" ]] || { result_code=invalid_invocation; exit 2; }
      expected_commit="$2"
      shift 2
      ;;
    --expected-source-sha256)
      [[ $# -ge 2 && -z "$expected_source_sha256" ]] || { result_code=invalid_invocation; exit 2; }
      expected_source_sha256="$2"
      shift 2
      ;;
    --expected-identity-sha256)
      [[ $# -ge 2 && -z "$expected_identity_sha256" ]] || { result_code=invalid_invocation; exit 2; }
      expected_identity_sha256="$2"
      shift 2
      ;;
    --expected-manifest-sha256)
      [[ $# -ge 2 && -z "$expected_manifest_sha256" ]] || { result_code=invalid_invocation; exit 2; }
      expected_manifest_sha256="$2"
      shift 2
      ;;
    *)
      result_code=invalid_invocation
      exit 2
      ;;
  esac
done

if ! [[ "$expected_archive_sha256" =~ ^[0-9a-f]{64}$ \
  && "$expected_tag" =~ ^[A-Za-z0-9][A-Za-z0-9._-]{0,127}$ \
  && "$expected_commit" =~ ^[0-9a-f]{40}$ \
  && "$expected_source_sha256" =~ ^[0-9a-f]{64}$ \
  && "$expected_identity_sha256" =~ ^[0-9a-f]{64}$ \
  && "$expected_manifest_sha256" =~ ^[0-9a-f]{64}$ ]]; then
  result_code=invalid_invocation
  exit 2
fi

if [[ ! -f "$EXTRACTOR" || ! -f "$VERIFIER" || ! -x "$FLOCK_BIN" \
  || ! -x "$TIMEOUT_BIN" || ! -x "$PRLIMIT_BIN" ]]; then
  result_code=runner_dependency_missing
  exit 1
fi

node_bin=""
for candidate in /usr/bin/node /usr/local/bin/node; do
  if [[ -x "$candidate" ]]; then
    node_bin="$candidate"
    break
  fi
done
python_bin=""
for candidate in /usr/bin/python3 /usr/local/bin/python3; do
  if [[ -x "$candidate" ]]; then
    python_bin="$candidate"
    break
  fi
done
if [[ -z "$node_bin" || -z "$python_bin" ]]; then
  result_code=runner_dependency_missing
  exit 1
fi

ensure_private_dir() {
  local directory="$1"
  if [[ -L "$directory" ]]; then
    return 1
  fi
  if [[ ! -e "$directory" ]]; then
    /bin/mkdir -m 0700 "$directory" || return 1
    /bin/chown root:root "$directory" || return 1
  fi
  [[ -d "$directory" ]] || return 1
  [[ "$(stat -c '%u:%g:%a' "$directory" 2>/dev/null || true)" == "0:0:700" ]]
}

if ! ensure_private_dir "$STATE_ROOT" \
  || ! ensure_private_dir "$DOWNLOAD_ROOT" \
  || ! ensure_private_dir "$INGRESS_ROOT" \
  || ! ensure_private_dir "$QUARANTINE_ROOT"; then
  result_code=state_root_invalid
  exit 1
fi

if [[ -e "$INGRESS_LOCK" || -L "$INGRESS_LOCK" ]]; then
  if [[ -L "$INGRESS_LOCK" || ! -f "$INGRESS_LOCK" \
    || "$(stat -c '%u:%g:%a:%h' "$INGRESS_LOCK" 2>/dev/null || true)" != "0:0:600:1" ]]; then
    result_code=ingress_lock_invalid
    exit 1
  fi
else
  if ! "$python_bin" - "$INGRESS_LOCK" <<'PY'
import os
import sys

descriptor = os.open(sys.argv[1], os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
try:
    os.fchown(descriptor, 0, 0)
    os.fchmod(descriptor, 0o600)
    os.fsync(descriptor)
finally:
    os.close(descriptor)
PY
  then
    result_code=ingress_lock_invalid
    exit 1
  fi
fi
if [[ "$(stat -c '%u:%g:%a:%h' "$INGRESS_LOCK" 2>/dev/null || true)" != "0:0:600:1" ]]; then
  result_code=ingress_lock_invalid
  exit 1
fi

exec 9<>"$INGRESS_LOCK"
if ! "$FLOCK_BIN" -n 9; then
  result_code=ingress_lock_busy
  exit 75
fi

target_path="${INGRESS_ROOT}/${expected_commit}"
if [[ -L "$target_path" || -e "$target_path" ]]; then
  if [[ -L "$target_path" || ! -d "$target_path" ]]; then
    result_code=existing_artifact_invalid
    exit 1
  fi
  if "$node_bin" "$VERIFIER" \
    --artifact-dir "$target_path" \
    --expected-tag "$expected_tag" \
    --expected-commit "$expected_commit" \
    --expected-source-sha256 "$expected_source_sha256" \
    --expected-identity-sha256 "$expected_identity_sha256" \
    --expected-manifest-sha256 "$expected_manifest_sha256" \
    >/dev/null 2>&1; then
    if ! "$python_bin" - "$target_path" "$INGRESS_ROOT" <<'PY'
import os
import sys

for target in sys.argv[1:]:
    descriptor = os.open(target, os.O_RDONLY | getattr(os, "O_NOFOLLOW", 0))
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
PY
    then
      result_code=existing_artifact_commit_uncertain
      exit 1
    fi
    emit_ready true
    exit 0
  fi
  result_code=existing_artifact_invalid
  exit 1
fi

if ! "$python_bin" - "$QUARANTINE_ROOT" "$MAX_FAILED_QUARANTINES" <<'PY'
import os
import re
import stat
import sys

root, maximum_text = sys.argv[1:3]
maximum = int(maximum_text)
entries = os.listdir(root)
pattern = re.compile(r"^failed-[0-9a-f]{40}\.[A-Za-z0-9]{6}$")
if len(entries) > maximum:
    raise SystemExit(1)
for name in entries:
    if not pattern.fullmatch(name):
        raise SystemExit(1)
    metadata = os.lstat(os.path.join(root, name))
    if (
        not stat.S_ISDIR(metadata.st_mode)
        or metadata.st_uid != 0
        or metadata.st_gid != 0
        or stat.S_IMODE(metadata.st_mode) != 0o700
    ):
        raise SystemExit(1)
raise SystemExit(0 if len(entries) < maximum else 1)
PY
then
  result_code=quarantine_capacity_reached
  exit 1
fi

if ! "$python_bin" - "$STATE_ROOT" <<'PY'
import os
import sys

stats = os.statvfs(sys.argv[1])
available = stats.f_bavail * stats.f_frsize
raise SystemExit(0 if available >= 5 * 1024 * 1024 * 1024 else 1)
PY
then
  result_code=state_space_insufficient
  exit 1
fi

if [[ -f /etc/qintopia/cos-artifacts.env ]]; then
  if [[ -L /etc/qintopia/cos-artifacts.env ]]; then
    result_code=cos_auth_config_invalid
    exit 1
  fi
  if [[ "$(stat -c '%u:%g:%a' /etc/qintopia/cos-artifacts.env 2>/dev/null || true)" != "0:0:600" ]]; then
    result_code=cos_auth_config_invalid
    exit 1
  fi
  set +e
  . /etc/qintopia/cos-artifacts.env >/dev/null 2>&1
  source_status=$?
  set -e
  if [[ "$source_status" -ne 0 ]]; then
    result_code=cos_auth_config_invalid
    exit 1
  fi
fi

if [[ -z "${TENCENT_COS_BUCKET:-}" || -z "${TENCENT_COS_REGION:-}" ]]; then
  result_code=cos_config_missing
  exit 1
fi
auth_mode="${TENCENT_COS_AUTH_MODE:-SecretKey}"
case "$auth_mode" in
  CvmRole)
    [[ -n "${TENCENT_COS_CVM_ROLE_NAME:-}" ]] || { result_code=cos_auth_missing; exit 1; }
    ;;
  SecretKey)
    [[ -n "${TENCENT_COS_SECRET_ID:-}" && -n "${TENCENT_COS_SECRET_KEY:-}" ]] || { result_code=cos_auth_missing; exit 1; }
    ;;
  *)
    result_code=cos_auth_invalid
    exit 1
    ;;
esac

if [[ -n "${COSCLI_PATH:-}" ]]; then
  result_code=coscli_override_rejected
  exit 1
fi
coscli_path=""
for candidate in /usr/local/bin/coscli /usr/bin/coscli; do
  if [[ -x "$candidate" ]]; then
    coscli_path="$candidate"
    break
  fi
done
if [[ -z "$coscli_path" || -L "$coscli_path" || ! -f "$coscli_path" || ! -x "$coscli_path" ]]; then
  result_code=coscli_missing
  exit 1
fi
if ! "$python_bin" - "$coscli_path" <<'PY'
import os
import stat
import sys

target = os.path.abspath(sys.argv[1])
current = os.path.sep
for component in target.removeprefix(os.path.sep).split(os.path.sep):
    current = os.path.join(current, component)
    metadata = os.lstat(current)
    if stat.S_ISLNK(metadata.st_mode) or metadata.st_uid != 0 or metadata.st_gid != 0:
        raise SystemExit(1)
    if stat.S_IMODE(metadata.st_mode) & 0o022:
        raise SystemExit(1)
if not stat.S_ISREG(metadata.st_mode) or metadata.st_nlink != 1:
    raise SystemExit(1)
PY
then
  result_code=coscli_invalid
  exit 1
fi

attempt_dir="$(mktemp -d "${DOWNLOAD_ROOT}/.download-${expected_commit}.XXXXXX")" || { result_code=download_temp_failed; exit 1; }
/bin/chmod 0700 "$attempt_dir"
/bin/chown root:root "$attempt_dir"
mkdir -m 0700 "$attempt_dir/home"
/bin/chown root:root "$attempt_dir/home"
archive_path="${attempt_dir}/hermes-core-${expected_commit}.tar.gz"
staging_artifact="$(mktemp -d "${INGRESS_ROOT}/.staging-${expected_commit}.XXXXXX")" || { result_code=download_temp_failed; exit 1; }
/bin/rmdir "$staging_artifact" || { result_code=download_temp_failed; exit 1; }
artifact_path="$staging_artifact"
config_path="${attempt_dir}/cos.yaml"
coscli_log="${attempt_dir}/coscli.log"

if ! "$python_bin" - "$config_path" <<'PY'
import json
import os
import sys

path = sys.argv[1]
mode = os.environ.get("TENCENT_COS_AUTH_MODE", "SecretKey")
base = {
    "secretid": os.environ.get("TENCENT_COS_SECRET_ID", "") if mode == "SecretKey" else "",
    "secretkey": os.environ.get("TENCENT_COS_SECRET_KEY", "") if mode == "SecretKey" else "",
    "sessiontoken": os.environ.get("TENCENT_COS_SESSION_TOKEN", "") if mode == "SecretKey" else "",
    "protocol": "https",
    "mode": mode,
    "cvmrolename": os.environ.get("TENCENT_COS_CVM_ROLE_NAME", "") if mode == "CvmRole" else "",
    "disableencryption": "true",
}
payload = "cos:\n  base:\n"
for key, value in base.items():
    payload += f"    {key}: {json.dumps(value, ensure_ascii=True)}\n"
payload += "  buckets: []\n"
fd = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
try:
    os.write(fd, payload.encode("utf-8"))
    os.fsync(fd)
finally:
    os.close(fd)
PY
then
  result_code=cos_config_failed
  exit 1
fi

run_coscli() {
  local label="$1"
  shift
  if ! HOME="${attempt_dir}/home" "$TIMEOUT_BIN" \
    --signal=TERM --kill-after=10s "${DOWNLOAD_TIMEOUT_SECONDS}s" \
    "$PRLIMIT_BIN" --fsize="${MAX_COSCLI_LOG_BYTES}:${MAX_COSCLI_LOG_BYTES}" -- \
    "$coscli_path" "$@" >"$coscli_log" 2>&1; then
    result_code="${label}_failed"
    exit 1
  fi
}

run_bounded_download() {
  if ! HOME="${attempt_dir}/home" "$TIMEOUT_BIN" \
    --signal=TERM --kill-after=10s "${DOWNLOAD_TIMEOUT_SECONDS}s" \
    "$PRLIMIT_BIN" --fsize="${MAX_ARCHIVE_BYTES}:${MAX_ARCHIVE_BYTES}" -- \
    "$coscli_path" "$@" >"$coscli_log" 2>&1; then
    result_code=download_failed
    exit 1
  fi
  local archive_size
  archive_size="$(stat -c '%s' "$archive_path" 2>/dev/null || true)"
  if ! [[ "$archive_size" =~ ^[0-9]+$ ]] \
    || (( archive_size <= 0 || archive_size > MAX_ARCHIVE_BYTES )); then
    result_code=download_size_invalid
    exit 1
  fi
}

run_coscli config_add config add \
  -b "$TENCENT_COS_BUCKET" \
  -r "$TENCENT_COS_REGION" \
  -a "$COS_BUCKET_ALIAS" \
  -c "$config_path" \
  --init-skip \
  --disable-log

object_key="${COS_OBJECT_PREFIX}/${expected_commit}/hermes-core-${expected_commit}.tar.gz"
run_bounded_download cp \
  "cos://${COS_BUCKET_ALIAS}/${object_key}" \
  "$archive_path" \
  -c "$config_path" \
  --init-skip \
  --disable-log

if ! "$python_bin" "$EXTRACTOR" \
  --extract \
  --archive "$archive_path" \
  --staging "$artifact_path" \
  --expected-archive-sha256 "$expected_archive_sha256" \
  --expected-tag "$expected_tag" \
  --expected-commit "$expected_commit" \
  --expected-source-sha256 "$expected_source_sha256" \
  --expected-identity-sha256 "$expected_identity_sha256" \
  --expected-manifest-sha256 "$expected_manifest_sha256" \
  >"${attempt_dir}/extract.log" 2>&1; then
  result_code=extract_failed
  exit 1
fi

if ! "$node_bin" "$VERIFIER" \
  --artifact-dir "$artifact_path" \
  --expected-tag "$expected_tag" \
  --expected-commit "$expected_commit" \
  --expected-source-sha256 "$expected_source_sha256" \
  --expected-identity-sha256 "$expected_identity_sha256" \
  --expected-manifest-sha256 "$expected_manifest_sha256" \
  >"${attempt_dir}/verify.log" 2>&1; then
  result_code=artifact_verification_failed
  exit 1
fi

if ! "$python_bin" "$EXTRACTOR" \
  --commit-artifact \
  --staging "$artifact_path" \
  --commit "$expected_commit" \
  >"${attempt_dir}/commit.log" 2>&1; then
  result_code=commit_failed
  if /usr/bin/grep -q '^hermes_core_artifact_error=commit_uncertain$' "${attempt_dir}/commit.log" 2>/dev/null; then
    result_code=commit_uncertain
  fi
  exit 1
fi

if ! cleanup_sensitive \
  || ! cleanup_untrusted_archive \
  || ! /bin/rm -f -- "${attempt_dir}/extract.log" "${attempt_dir}/verify.log" "${attempt_dir}/commit.log" \
  || ! /bin/rmdir "$attempt_dir"; then
  result_code=download_cleanup_failed
  exit 1
fi
attempt_dir=""
staging_artifact=""
emit_ready false
