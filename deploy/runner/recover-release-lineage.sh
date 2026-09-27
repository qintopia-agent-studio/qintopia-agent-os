#!/usr/bin/env bash
set -euo pipefail
umask 077

state=/var/lib/qintopia-agent-os-deploy
release_root=/home/ubuntu/qintopia-agent-os-releases
unit=qintopia-agent-os-deploy-runner.service
timer=qintopia-agent-os-deploy-runner.timer
anan_unit=qintopia-agent-os-anan-drain-restart.service
env_file=/etc/qintopia/cos-artifacts.env

[[ "$(id -u)" -eq 0 && $# -eq 2 && "$1" == --request-id &&
  "$2" =~ ^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$ ]] || exit 2
request_id="$2"
script_path="$(readlink -f "${BASH_SOURCE[0]}")"
[[ "$script_path" =~ ^/home/ubuntu/qintopia-agent-os-releases/[0-9a-f]{40}/deploy/runner/recover-release-lineage\.sh$ ]] || {
  echo "recovery helper must run from a fixed immutable release" >&2
  exit 2
}
verified_release="${script_path%/deploy/runner/recover-release-lineage.sh}"
[[ -f "$env_file" ]] || { echo "recovery COS environment is unavailable" >&2; exit 75; }
# shellcheck disable=SC1090
source "$env_file"
export TENCENT_COS_BUCKET TENCENT_COS_REGION DEPLOY_REQUEST_SIGNING_KEY DEPLOY_REQUEST_SIGNING_KEY_ID
journal="${state}/recovery/${request_id}.json"
hold="${state}/recovery/hold"
dropin=/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf
[[ -f "$journal" && -f "$hold" && -f "$dropin" ]] || { echo "recovery journal or hold is absent" >&2; exit 75; }

unit_stopped() {
  local properties="" load_state="" active_state="" allow_not_found="${2:-false}"
  properties="$(systemctl show "$1" --property=LoadState --property=ActiveState 2>/dev/null)" || return 1
  load_state="$(printf '%s\n' "$properties" | sed -n 's/^LoadState=//p')"
  active_state="$(printf '%s\n' "$properties" | sed -n 's/^ActiveState=//p')"
  if [[ "$load_state" == not-found ]]; then
    [[ "$allow_not_found" == true && "$active_state" == inactive ]]
  else
    [[ "$load_state" == loaded && ( "$active_state" == inactive || "$active_state" == failed ) ]]
  fi
}

systemctl disable --now "$timer"
for ((i=0; i<120; i++)); do
  if unit_stopped "$unit"; then break; fi
  sleep 1
done
unit_stopped "$unit" || { echo "ordinary poller active or state unknown" >&2; exit 75; }
systemctl daemon-reload
cmp -s "$dropin" "$verified_release/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf" || exit 75
unit_stopped "$timer" || exit 75
unit_stopped "$anan_unit" true || {
  echo "Anan drain helper is still active or unknown" >&2
  exit 75
}

exec 8>"${state}/poller.lock"
flock -n 8 || { echo "poller lock is held" >&2; exit 75; }
exec 9>"${state}/deploy.lock"
flock -n 9 || { echo "deploy lock is held" >&2; exit 75; }

request_file=""
for candidate in "${state}/requests/pending/${request_id}.json" \
  "${state}/requests/processed/${request_id}.json" \
  "${state}/requests/failed/${request_id}.json"; do
  if [[ -f "$candidate" ]]; then
    [[ -z "$request_file" ]] || { echo "ambiguous local request archive" >&2; exit 75; }
    request_file="$candidate"
  fi
done
[[ -n "$request_file" ]] || { echo "original signed request is unavailable" >&2; exit 75; }

identity="$(python3 - "$journal" "$request_file" "$verified_release" "$release_root" <<'PY'
import hashlib
import hmac
import json
import os
import re
import sys
from datetime import datetime, timedelta, timezone
from pathlib import Path

journal_path, request_path, verified_release, release_root = sys.argv[1:5]
with open(journal_path, encoding="utf-8") as fh:
    journal = json.load(fh)
with open(request_path, "rb") as fh:
    request_bytes = fh.read()
request = json.loads(request_bytes)
if (journal.get("schema_version") != 1 or journal.get("request_id") != request.get("request_id") or
        journal.get("request_sha256") != hashlib.sha256(request_bytes).hexdigest() or
        journal.get("direction") not in ("O→T", "T→R", "R→T")):
    raise SystemExit("recovery journal and request identity mismatch")
signature = request.get("signature")
if not isinstance(signature, dict) or (signature.get("algorithm"), signature.get("issuer"), signature.get("key_id")) != (
        "hmac-sha256", "github-actions", os.environ.get("DEPLOY_REQUEST_SIGNING_KEY_ID")):
    raise SystemExit("original request signature identity mismatch")
created = datetime.fromisoformat(request["created_at"].replace("Z", "+00:00"))
expires = datetime.fromisoformat(request["expires_at"].replace("Z", "+00:00"))
signed = datetime.fromisoformat(signature["signed_at"].replace("Z", "+00:00"))
if (created.tzinfo is None or expires.tzinfo is None or signed.tzinfo is None or
        expires <= created or expires - created > timedelta(minutes=60) or
        abs(signed - created) > timedelta(minutes=5)):
    raise SystemExit("original request signing time is invalid")
def canonical(value):
    if isinstance(value, list):
        return "[" + ",".join(canonical(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(k, separators=(",", ":")) + ":" + canonical(value[k]) for k in sorted(value)) + "}"
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
unsigned = dict(request)
metadata = dict(unsigned.pop("signature"))
actual = metadata.pop("value", "")
expected = hmac.new(os.environ["DEPLOY_REQUEST_SIGNING_KEY"].encode(),
                    canonical({"request": unsigned, "signature": metadata}).encode(), hashlib.sha256).hexdigest()
if not hmac.compare_digest(actual, expected):
    raise SystemExit("original request signature verification failed")
root = Path(release_root).resolve(strict=True)
current_sha = journal["original_current_sha"]
previous_sha = journal["original_previous_sha"]
if not all(re.fullmatch(r"[0-9a-f]{40}", value) for value in (current_sha, previous_sha)):
    raise SystemExit("recovery journal original pointers are invalid")
expected_verified = {
    "O→T": request["release_sha"],
    "T→R": current_sha,
    "R→T": previous_sha,
}[journal["direction"]]
if Path(verified_release).name != expected_verified:
    raise SystemExit("recovery helper release does not match direction-bound journal")
for key, sha in (("current", current_sha), ("previous", previous_sha)):
    manifest_path = root / sha / "manifest.json"
    with open(manifest_path, "rb") as fh:
        digest = hashlib.sha256(fh.read()).hexdigest()
    if journal.get("manifest_sha256", {}).get(key) != digest:
        raise SystemExit(f"recovery {key} manifest drifted")
if request.get("dry_run") is not False or request.get("environment") != "production":
    raise SystemExit("recovery requires an original production live request")
cos = request.get("cos", {})
if (cos.get("bucket") != os.environ.get("TENCENT_COS_BUCKET") or
        cos.get("region") != os.environ.get("TENCENT_COS_REGION") or
        cos.get("prefix") != "qintopia-agent-os" or
        cos.get("request_key") != f"qintopia-agent-os/deploy-requests/production/requests/{request['request_id']}.json" or
        cos.get("result_key") != f"qintopia-agent-os/deploy-results/production/{request['request_id']}.json"):
    raise SystemExit("recovery COS keys are not fixed to the signed request")
if journal["direction"] == "O→T":
    if request.get("release_scope") != ["deploy-bundle"] or request.get("restart_targets") != ["qintopia-system-services"]:
        raise SystemExit("first takeover request is not single-system")
elif journal["direction"] == "T→R":
    targets = request.get("restart_targets")
    if (request.get("release_rollback") is not None or
            request.get("release_scope") != ["sidecar-runtime", "deploy-bundle", "hermes-plugins"] or
            not isinstance(targets, list) or len(targets) != 6 or set(targets) != {
                "qintopia-system-services", "hermes-erhua", "hermes-xiaoman",
                "hermes-silaoshi", "hermes-huabaosi", "hermes-anan"}):
        raise SystemExit("mixed forward request is not the approved six-target live action")
elif request.get("release_rollback") != {
        "expected_current_sha": current_sha, "expected_previous_sha": previous_sha}:
    raise SystemExit("reverse rollback request does not bind R/T")
print("\t".join((journal["direction"], current_sha, previous_sha,
                 request["release_sha"], ",".join(request.get("restart_targets", [])))))
PY
)" || exit 75
IFS=$'\t' read -r direction original_current original_previous request_release restart_targets <<<"$identity"

result_file="${state}/results/${request_id}.json"
if [[ ! -f "$result_file" ]]; then
  echo "local result missing; COS outcome requires separate reconciliation" >&2
  exit 75
fi
"${verified_release}/deploy/runner/wait-deploy-result.sh" --request-file "$request_file" \
  --result-file "$result_file" --verify-archived-request >/dev/null || exit 75
result_status="$(python3 - "$result_file" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    print(json.load(fh).get("status", ""))
PY
)"
if [[ "$result_status" == succeeded ]]; then
  echo "signed success result exists; only read-only archive reconciliation is permitted" >&2
  exit 75
fi
[[ "$result_status" == failed || "$result_status" == rolled_back ]] || exit 75

verify_remote_evidence() {
  local tmp="" coscli="${COSCLI_PATH:-}" config="" alias="${TENCENT_COS_BUCKET_ALIAS:-qintopia-agent-os-artifacts}"
  [[ -n "${TENCENT_COS_BUCKET:-}" && -n "${TENCENT_COS_REGION:-}" ]] || return 1
  if [[ -z "$coscli" ]]; then
    coscli="$(command -v coscli)" || return 1
  fi
  [[ -x "$coscli" ]] || return 1
  tmp="$(mktemp -d)" || return 1
  chmod 0700 "$tmp" || { rm -rf "$tmp"; return 1; }
  config="$tmp/cos.yaml"
  touch "$config" || { rm -rf "$tmp"; return 1; }
  local status=0
  (
    cd "$tmp" || exit 1
    if [[ "${TENCENT_COS_AUTH_MODE:-SecretKey}" == CvmRole ]]; then
      [[ -n "${TENCENT_COS_CVM_ROLE_NAME:-}" ]] || exit 1
      "$coscli" config set --mode CvmRole --cvm_role_name "$TENCENT_COS_CVM_ROLE_NAME" \
        -c "$config" --init-skip --disable-log >/dev/null 2>&1 || exit 1
    else
      [[ -n "${TENCENT_COS_SECRET_ID:-}" && -n "${TENCENT_COS_SECRET_KEY:-}" ]] || exit 1
      local auth_args=(--mode SecretKey --secret_id "$TENCENT_COS_SECRET_ID" --secret_key "$TENCENT_COS_SECRET_KEY")
      if [[ -n "${TENCENT_COS_SESSION_TOKEN:-}" ]]; then
        auth_args+=(--session_token "$TENCENT_COS_SESSION_TOKEN")
      fi
      "$coscli" config set -c "$config" "${auth_args[@]}" --init-skip --disable-log >/dev/null 2>&1 || exit 1
    fi
    local bucket_args=(-b "$TENCENT_COS_BUCKET" -r "$TENCENT_COS_REGION" -a "$alias" -c "$config" --init-skip --disable-log)
    if [[ -n "${TENCENT_COS_ENDPOINT:-}" ]]; then
      bucket_args+=(-e "$TENCENT_COS_ENDPOINT")
    fi
    "$coscli" config add "${bucket_args[@]}" >/dev/null 2>&1 || exit 1
    "$coscli" cp "cos://${alias}/qintopia-agent-os/deploy-requests/production/requests/${request_id}.json" \
      "$tmp/request.json" -c "$config" --disable-log >/dev/null 2>&1 || exit 1
    "$coscli" cp "cos://${alias}/qintopia-agent-os/deploy-results/production/${request_id}.json" \
      "$tmp/result.json" -c "$config" --disable-log >/dev/null 2>&1 || exit 1
    cmp -s "$request_file" "$tmp/request.json" || exit 1
    cmp -s "$result_file" "$tmp/result.json" || exit 1
  ) || status=$?
  rm -rf "$tmp"
  return "$status"
}

verify_remote_evidence || {
  echo "COS request or result is absent, unreadable, or conflicts with local signed evidence" >&2
  exit 75
}

record_recovery_phase() {
  python3 - "$journal" "$1" <<'PY'
import json
import os
import sys
import tempfile
from datetime import datetime, timezone

path, phase = sys.argv[1:3]
with open(path, encoding="utf-8") as fh:
    record = json.load(fh)
if record.get("request_id") != os.environ.get("QINTOPIA_RECOVERY_REQUEST_ID"):
    raise SystemExit("recovery journal identity changed")
record["recovery_phase"] = phase
record["recovery_recorded_at"] = datetime.now(timezone.utc).isoformat()
fd, temporary = tempfile.mkstemp(prefix=".recovery-phase-", dir=os.path.dirname(path))
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
PY
}
export QINTOPIA_RECOVERY_REQUEST_ID="$request_id"

read -r current previous < <(python3 - "$release_root" <<'PY'
from pathlib import Path
import sys
root = Path(sys.argv[1]).resolve(strict=True)
values = []
for name in ("current", "previous"):
    link = root / name
    if not link.is_symlink():
        raise SystemExit("release pointer is not a symlink")
    target = link.resolve(strict=True)
    if target.parent != root:
        raise SystemExit("release pointer escaped root")
    values.append(target.name)
print(" ".join(values))
PY
)

cas_pointer() {
  record_recovery_phase "cas-$1-$2-to-$3-started"
  python3 - "$release_root" "$1" "$2" "$3" <<'PY'
import os
import secrets
import sys
from pathlib import Path
root = Path(sys.argv[1]).resolve(strict=True)
name, expected, target = sys.argv[2:5]
assert name in ("current", "previous")
link = root / name
if not link.is_symlink() or link.resolve(strict=True) != root / expected:
    raise SystemExit("recovery pointer CAS conflict")
if not (root / target).is_dir():
    raise SystemExit("recovery pointer target is absent")
temporary = root / f".{name}.recover.{os.getpid()}.{secrets.token_hex(8)}"
try:
    os.symlink(str(root / target), temporary)
    os.replace(temporary, link)
    descriptor = os.open(root, os.O_RDONLY | os.O_DIRECTORY)
    os.fsync(descriptor)
    os.close(descriptor)
finally:
    if temporary.is_symlink():
        temporary.unlink()
PY
  record_recovery_phase "cas-$1-$2-to-$3-completed"
}

restore_sha=""
case "$direction:$current:$previous" in
  "O→T:${original_current}:${original_previous}") restore_sha="$original_current" ;;
  "O→T:${original_current}:${original_current}")
    cas_pointer previous "$original_current" "$original_previous"
    restore_sha="$original_current" ;;
  "O→T:${request_release}:${original_current}")
    record_recovery_phase "rollback-O-to-T-started"
    "${verified_release}/deploy/runner/rollback-release.sh" \
      --release-root "$release_root" --expected-current-sha "$request_release" \
      --expected-previous-sha "$original_current" \
      --restore-previous-sha "$original_previous"
    record_recovery_phase "rollback-O-to-T-completed"
    restore_sha="$original_current" ;;
  "T→R:${original_current}:${original_previous}") restore_sha="$original_current" ;;
  "T→R:${original_current}:${original_current}")
    cas_pointer previous "$original_current" "$original_previous"
    restore_sha="$original_current" ;;
  "T→R:${request_release}:${original_current}")
    record_recovery_phase "rollback-T-to-R-started"
    "${verified_release}/deploy/runner/rollback-release.sh" \
      --release-root "$release_root" --expected-current-sha "$request_release" \
      --expected-previous-sha "$original_current" \
      --restore-previous-sha "$original_previous"
    record_recovery_phase "rollback-T-to-R-completed"
    restore_sha="$original_current" ;;
  "R→T:${original_current}:${original_previous}") restore_sha="$original_current" ;;
  "R→T:${original_previous}:${original_previous}")
    cas_pointer current "$original_previous" "$original_current"
    restore_sha="$original_current" ;;
  "R→T:${original_previous}:"*)
    target_ancestor="$(python3 - "${release_root}/${original_previous}/manifest.json" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    print(json.load(fh).get("previous_sha", ""))
PY
)"
    [[ "$previous" == "$target_ancestor" ]] || exit 75
    cas_pointer previous "$target_ancestor" "$original_previous"
    cas_pointer current "$original_previous" "$original_current"
    restore_sha="$original_current" ;;
  *) echo "pointer state requires direction-specific manual review" >&2; exit 75 ;;
esac

record_recovery_phase "installer-started"
"${release_root}/${restore_sha}/deploy/runner/install-release-systemd-units.sh" \
  --release-root "$release_root" --release-sha "$restore_sha"
record_recovery_phase "installer-completed"
systemctl daemon-reload
cmp -s "$dropin" "$verified_release/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
unit_stopped "$timer"
record_recovery_phase "smoke-started"
"${release_root}/${restore_sha}/deploy/runner/smoke-release.sh" \
  --release-root "$release_root" --restart-targets "$restart_targets"
record_recovery_phase "smoke-completed"
echo "limited recovery reached ${restore_sha}; hold remains until signed COS and archive reconciliation"
