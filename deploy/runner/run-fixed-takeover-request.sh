#!/usr/bin/env bash
set -euo pipefail
umask 077

state=/var/lib/qintopia-agent-os-deploy
recovery="${state}/recovery"
staged="${recovery}/staged"
release_root=/home/ubuntu/qintopia-agent-os-releases
unit=qintopia-agent-os-deploy-runner.service
timer=qintopia-agent-os-deploy-runner.timer
dropin_dir=/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d
dropin_name=10-recovery-hold.conf

[[ "$(id -u)" -eq 0 ]] || { echo "root is required" >&2; exit 2; }
[[ $# -ge 1 ]] || { echo "prepare, consume or finalize is required" >&2; exit 2; }
mode="$1"
shift

verify_staged_bundle() {
  [[ -d "$staged/payload/deploy/runner" && ! -L "$staged" ]] || return 1
  (cd "$staged" && sha256sum -c SHA256SUMS >/dev/null) || return 1
  python3 - "$staged" <<'PY'
import hashlib
import json
import os
import stat
import sys
from pathlib import Path

root = Path(sys.argv[1])
with open(root / "artifact-manifest.json", encoding="utf-8") as fh:
    manifest = json.load(fh)
if manifest.get("schema_version") != 1 or manifest.get("target") != "server-operator-files":
    raise SystemExit("staged recovery bundle manifest is invalid")
expected = set()
for item in manifest.get("files", []):
    relative = Path(item["path"])
    if relative.is_absolute() or ".." in relative.parts:
        raise SystemExit("staged recovery bundle path escaped root")
    target = root / relative
    metadata = target.lstat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or metadata.st_mode & 0o022:
        raise SystemExit("staged recovery bundle owner, type or mode is invalid")
    digest = hashlib.sha256(target.read_bytes()).hexdigest()
    if digest != item["sha256"] or metadata.st_size != item["size_bytes"]:
        raise SystemExit("staged recovery bundle content mismatch")
    expected.add(str(relative))
for relative in ("payload/deploy/runner/poll-deploy-requests.sh",
                 "payload/deploy/runner/recover-release-lineage.sh",
                 "payload/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"):
    if relative not in expected:
        raise SystemExit(f"staged recovery bundle misses {relative}")
PY
}

verify_hold_guard() {
  cmp -s "$staged/payload/deploy/runner/qintopia-agent-os-deploy-runner.service.d/$dropin_name" \
    "$dropin_dir/$dropin_name" || return 1
}

verify_hold() {
  [[ -f "${recovery}/hold" ]] || return 1
  verify_hold_guard || return 1
  python3 - "${recovery}/takeover.json" "${recovery}/hold" <<'PY' || return 1
import json
import re
import stat
import sys
from pathlib import Path

record_path, hold_path = map(Path, sys.argv[1:3])
with record_path.open(encoding="utf-8") as fh:
    token = json.load(fh).get("hold_token", "")
if not isinstance(token, str) or not re.fullmatch(r"[0-9a-f]{32}", token):
    raise SystemExit("takeover hold identity is invalid")
metadata = hold_path.lstat()
if not stat.S_ISREG(metadata.st_mode) or stat.S_IMODE(metadata.st_mode) != 0o600:
    raise SystemExit("takeover hold file is invalid")
if hold_path.read_text(encoding="ascii") != token + "\n":
    raise SystemExit("takeover hold belongs to another transaction")
PY
  [[ "$(timer_file_state)" == disabled ]] || return 1
  unit_stopped "$timer" && unit_stopped "$unit"
}

verify_finalization_state() {
  python3 - "$recovery" "${state}/requests/claimed" "$request_id" "$mode" <<'PY'
import json
import re
import stat
import sys
from pathlib import Path

recovery, claimed, request_id, mode = sys.argv[1:5]
recovery = Path(recovery)
with (recovery / "takeover.json").open(encoding="utf-8") as fh:
    record = json.load(fh)
token = record.get("hold_token", "")
if not isinstance(token, str) or not re.fullmatch(r"[0-9a-f]{32}", token):
    raise SystemExit("takeover hold identity is invalid")
if record.get("request_id") != request_id:
    raise SystemExit("takeover hold is not bound to this request")
if any(Path(claimed).glob("*.json")):
    raise SystemExit("another deploy claim is unfinished")
takeover_sha = "70e7984fab92ddab956009585212d0e9729767b5"
for path in recovery.glob("deploy-*.json"):
    if path.name == request_id + ".json":
        continue
    with path.open(encoding="utf-8") as fh:
        later = json.load(fh)
    if (path.name > request_id + ".json" or
            takeover_sha in (later.get("original_current_sha"), later.get("original_previous_sha"))):
        raise SystemExit("another recovery journal owns the current isolation")
hold = recovery / "hold"
if hold.exists() or hold.is_symlink():
    metadata = hold.lstat()
    if (not stat.S_ISREG(metadata.st_mode) or stat.S_IMODE(metadata.st_mode) != 0o600 or
            hold.read_text(encoding="ascii") != token + "\n"):
        raise SystemExit("current hold belongs to another transaction")
elif mode != "finalize":
    raise SystemExit("takeover hold disappeared before finalization")
PY
}

unit_stopped() {
  local properties="" load_state="" active_state=""
  properties="$(systemctl show "$1" --property=LoadState --property=ActiveState 2>/dev/null)" || return 1
  load_state="$(printf '%s\n' "$properties" | sed -n 's/^LoadState=//p')"
  active_state="$(printf '%s\n' "$properties" | sed -n 's/^ActiveState=//p')"
  [[ "$load_state" == loaded && ( "$active_state" == inactive || "$active_state" == failed ) ]]
}

timer_file_state() {
  local value=""
  value="$(systemctl show "$timer" --property=UnitFileState --value 2>/dev/null)" || return 1
  [[ "$value" == enabled || "$value" == disabled ]] || return 1
  printf '%s\n' "$value"
}

case "$mode" in
  prepare)
    [[ $# -eq 0 ]] || exit 2
    verify_staged_bundle
    mkdir -p -m 0700 "$recovery"
    [[ ! -e "${recovery}/hold" ]] || { echo "recovery hold already exists" >&2; exit 75; }
    [[ ! -e "${recovery}/takeover-consumed" ]] || { echo "takeover was already consumed" >&2; exit 75; }
    timer_state="$(timer_file_state)" || exit 75
    timer_was_enabled=false
    [[ "$timer_state" != enabled ]] || timer_was_enabled=true
    systemctl disable --now "$timer"
    for ((i=0; i<120; i++)); do
      if unit_stopped "$unit"; then break; fi
      sleep 1
    done
    unit_stopped "$unit" || { echo "old poller active or state unknown" >&2; exit 75; }
    python3 - "$recovery" "$timer_was_enabled" <<'PY'
import json
import os
import secrets
import sys
import tempfile
directory, enabled = sys.argv[1:3]
token = secrets.token_hex(16)
fd, temporary = tempfile.mkstemp(prefix=".prepare-", dir=directory)
with os.fdopen(fd, "w", encoding="utf-8") as fh:
    json.dump({"timer_was_enabled": enabled == "true", "hold_token": token}, fh)
    fh.write("\n")
    fh.flush()
    os.fsync(fh.fileno())
os.replace(temporary, os.path.join(directory, "takeover.json"))
hold = os.open(os.path.join(directory, "hold"), os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
os.write(hold, (token + "\n").encode("ascii"))
os.fsync(hold)
os.close(hold)
descriptor = os.open(directory, os.O_RDONLY | os.O_DIRECTORY)
os.fsync(descriptor)
os.close(descriptor)
PY
    mkdir -p "$dropin_dir"
    install -m 0644 "$staged/payload/deploy/runner/qintopia-agent-os-deploy-runner.service.d/$dropin_name" \
      "$dropin_dir/$dropin_name"
    systemctl daemon-reload
    verify_hold
    ;;
  consume|finalize)
    [[ $# -eq 1 && "$1" =~ ^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$ ]] || exit 2
    request_id="$1"
    verify_staged_bundle
    old_sha=16e8d56b98001579c6288ba13199b80d6d3dfc74
    if [[ "$mode" == consume ]]; then
      verify_hold
      [[ "$(readlink -f "$release_root/current")" == "$release_root/$old_sha" &&
        -x "${release_root}/${old_sha}/deploy/runner/qintopia-agent-os-deploy-runner" &&
        "$(sha256sum "${release_root}/${old_sha}/deploy/runner/qintopia-agent-os-deploy-runner" | awk '{print $1}')" == \
          04b27ea6900dec7078b3dfb56a28e0f5af3f9b58413b2df85ecaac54784b4dcf ]] || {
        echo "fixed takeover old runner identity mismatch" >&2
        exit 75
      }
      python3 - "$recovery" "$request_id" <<'PY'
import json
import os
import re
import sys
import tempfile
directory = os.open(sys.argv[1], os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
try:
    with open(os.path.join(sys.argv[1], "takeover.json"), encoding="utf-8") as fh:
        record = json.load(fh)
    token = record.get("hold_token", "")
    if (not isinstance(token, str) or not re.fullmatch(r"[0-9a-f]{32}", token) or
            record.get("request_id") not in (None, sys.argv[2])):
        raise SystemExit("takeover request binding is invalid")
    with open(os.path.join(sys.argv[1], "hold"), encoding="ascii") as fh:
        if fh.read() != token + "\n":
            raise SystemExit("takeover hold identity changed")
    record["request_id"] = sys.argv[2]
    fd, temporary = tempfile.mkstemp(prefix=".takeover-binding-", dir=sys.argv[1])
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as fh:
            json.dump(record, fh, sort_keys=True)
            fh.write("\n")
            fh.flush()
            os.fsync(fh.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, os.path.join(sys.argv[1], "takeover.json"))
        os.fsync(directory)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
    marker = os.open("takeover-consumed", os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                     0o600, dir_fd=directory)
    with os.fdopen(marker, "w", encoding="utf-8") as fh:
        fh.write(sys.argv[2] + "\n")
        fh.flush()
        os.fsync(fh.fileno())
    os.fsync(directory)
finally:
    os.close(directory)
PY
      run_output="$(mktemp)"
      if ! systemd-run --unit=qintopia-agent-os-fixed-takeover.service --service-type=oneshot --wait \
      --uid=root --gid=root --property=StateDirectory=qintopia-agent-os-deploy \
      --property=StateDirectoryMode=0700 --property=WorkingDirectory=/var/lib/qintopia-agent-os-deploy \
      --property=NoNewPrivileges=yes --property=PrivateTmp=yes \
      --property=ProtectHome=read-only --property=ProtectSystem=strict \
      --property="ReadWritePaths=/etc/nginx/snippets /etc/qintopia/message-sidecar.env /etc/systemd/system /home/ubuntu/.hermes/profiles/erhua /home/ubuntu/.hermes/profiles/erhua/scripts /home/ubuntu/.hermes/profiles/xiaoman/cron /home/ubuntu/.hermes/profiles/xiaoman/scripts /home/ubuntu/.hermes/scripts /home/ubuntu/.config/systemd/user /home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot /home/ubuntu/.local/state/qintopia-agentos/xiaoman-creative-profile-candidates /home/ubuntu/.local/state/qintopia-agentos/xiaoman-daily-case-report /home/ubuntu/qintopia-agent-os-releases /var/lib/qintopia-agent-os-deploy /tmp" \
      --property=Environment=QINTOPIA_COS_ENV_FILE=/etc/qintopia/cos-artifacts.env \
      --property=Environment=QINTOPIA_DEPLOY_RUNNER_STATE_DIR=/var/lib/qintopia-agent-os-deploy \
      --property="Environment=QINTOPIA_EXPECTED_DEPLOY_REQUEST_ID=${request_id}" \
      --property="Environment=QINTOPIA_DEPLOY_RUNNER_BIN=${release_root}/${old_sha}/deploy/runner/qintopia-agent-os-deploy-runner" \
      "$staged/payload/deploy/runner/poll-deploy-requests.sh" >"$run_output" 2>&1; then
        rm -f "$run_output"
        echo "fixed takeover result is uncertain; hold retained" >&2
        exit 75
      fi
      rm -f "$run_output"
    fi
    exec 8>"${state}/poller.lock"
    flock -n 8 || { echo "poller lock is held during takeover finalization" >&2; exit 75; }
    exec 9>"${state}/deploy.lock"
    flock -n 9 || { echo "deploy lock is held during takeover finalization" >&2; exit 75; }
    [[ -f "${recovery}/takeover-consumed" && ! -L "${recovery}/takeover-consumed" &&
      "$(cat "${recovery}/takeover-consumed")" == "$request_id" ]] || exit 75
    verify_finalization_state || exit 75
    [[ -f "${state}/requests/processed/${request_id}.json" &&
      ! -e "${state}/requests/claimed/${request_id}.json" ]] || exit 75
    "$staged/payload/deploy/runner/wait-deploy-result.sh" \
      --request-file "${state}/requests/processed/${request_id}.json" \
      --result-file "${state}/results/${request_id}.json" --verify-archived-request >/dev/null
    [[ "$(python3 - "${state}/results/${request_id}.json" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    print(json.load(fh).get("status", ""))
PY
)" == succeeded ]] || exit 75
    new_release="$(readlink -f "$release_root/current")"
    [[ "$new_release" == "$release_root/"* && "$(readlink -f "$release_root/previous")" == "$release_root/$old_sha" ]] || exit 75
    python3 - "$new_release/manifest.json" "$request_id" \
      "${state}/requests/processed/${request_id}.json" \
      "${state}/results/${request_id}.json" \
      "${recovery}/${request_id}.json" "$old_sha" "$new_release" <<'PY' || exit 75
import hashlib
import json
import sys
from pathlib import Path

manifest_path, request_id, request_path, result_path, journal_path, old_sha, new_release = sys.argv[1:8]
with open(manifest_path, encoding="utf-8") as fh:
    manifest = json.load(fh)
with open(request_path, "rb") as fh:
    request_bytes = fh.read()
request = json.loads(request_bytes)
with open(result_path, encoding="utf-8") as fh:
    result = json.load(fh)
with open(journal_path, encoding="utf-8") as fh:
    journal = json.load(fh)
with open(Path(journal_path).parent / "takeover.json", encoding="utf-8") as fh:
    takeover = json.load(fh)
with open(Path(new_release).parent / old_sha / "manifest.json", "rb") as fh:
    old_manifest_bytes = fh.read()
old_manifest = json.loads(old_manifest_bytes)
if (journal.get("direction") != "O→T" or journal.get("phase") != "intent" or
        journal.get("hold_token") != takeover.get("hold_token") or
        journal.get("request_id") != request_id or
        journal.get("request_sha256") != hashlib.sha256(request_bytes).hexdigest() or
        journal.get("original_current_sha") != old_sha or
        journal.get("original_previous_sha") != old_manifest.get("previous_sha") or
        journal.get("manifest_sha256", {}).get("current") != hashlib.sha256(old_manifest_bytes).hexdigest() or
        request.get("request_id") != request_id or request.get("release_sha") != Path(new_release).name or
        manifest.get("request_id") != request_id or
        manifest.get("previous_sha") != old_sha or
        manifest.get("release_scope") != ["deploy-bundle"] or
        manifest.get("restart_targets") != ["qintopia-system-services"] or
        result.get("request_id") != request_id or result.get("status") != "succeeded" or
        result.get("previous_sha") != old_sha or result.get("current_target") != new_release or
        result.get("rollback", {}).get("attempted") is not False or
        not any(check.get("name") == "deploy-runner" and check.get("status") == "passed"
                for check in result.get("checks", []))):
    raise SystemExit("takeover journal, request, smoke result, and pointers disagree")
for key in ("commit_sha", "runtime_sha", "deploy_bundle_sha", "runtime_artifact_profile"):
    if manifest.get(key) != request.get(key) or result.get(key) != request.get(key):
        raise SystemExit(f"takeover {key} identity mismatch")
PY
    verify_hold_guard
    timer_was_enabled="$(python3 - "${recovery}/takeover.json" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    value = json.load(fh)["timer_was_enabled"]
if type(value) is not bool:
    raise SystemExit("takeover timer state is invalid")
print(str(value).lower())
PY
)"
    if [[ ! -e "${recovery}/hold" ]]; then
      if [[ "$timer_was_enabled" == true ]]; then
        [[ "$(timer_file_state)" == enabled ]] && systemctl is-active --quiet "$timer" || exit 75
      else
        [[ "$(timer_file_state)" == disabled ]] && unit_stopped "$timer" || exit 75
      fi
      exit 0
    fi
    if [[ "$timer_was_enabled" == true ]]; then
      systemctl enable --now "$timer" || { echo "timer restore failed; hold retained" >&2; exit 75; }
      [[ "$(timer_file_state)" == enabled ]] && systemctl is-active --quiet "$timer" || {
        echo "timer readiness failed; hold retained" >&2
        exit 75
      }
    else
      [[ "$(timer_file_state)" == disabled ]] && unit_stopped "$timer" || exit 75
    fi
    if [[ -f "${recovery}/hold" ]]; then
      rm "${recovery}/hold"
    fi
    ;;
  *) echo "prepare, consume or finalize is required" >&2; exit 2 ;;
esac
