#!/usr/bin/env bash
set -euo pipefail
umask 077

unit=qintopia-agentos-management-ui.service
state=/var/lib/qintopia-agent-os-deploy
release_root=/home/ubuntu/qintopia-agent-os-releases
env_file=/etc/qintopia/collaboration-management-ui.env
site=/etc/nginx/sites-available/qintopia-management-ui.conf
enabled=/etc/nginx/sites-enabled/qintopia-management-ui.conf
renewal=/etc/letsencrypt/renewal/qintopia-management-ui.conf
certificate=/etc/letsencrypt/live/qintopia-management-ui/fullchain.pem

[[ $# -ge 1 && $# -le 2 ]] || exit 2
mode="$1"
case "$mode" in
  begin|finish) [[ $# -eq 2 && "$2" =~ ^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$ ]] || exit 2 ;;
  prepare|install-http|issue-cert|install-https|activate|quiesce|verify-closed|retire-site)
    [[ $# -eq 1 ]] || exit 2 ;;
  *) exit 2 ;;
esac
[[ "$(id -u)" -eq 0 ]] || { echo "management UI lifecycle requires root" >&2; exit 75; }
script_path="$(readlink -f -- "${BASH_SOURCE[0]}")"
staged="$state/recovery/staged"
staged_closure=false
if [[ "$script_path" == "$staged/payload/deploy/runner/management-ui-lifecycle.sh" ]]; then
  [[ "$mode" == quiesce || "$mode" == verify-closed ]] || exit 75
  authority="$("$staged/payload/deploy/runner/run-fixed-takeover-request.sh" verify-staged-closure)" || exit 75
  [[ "$authority" != verify-only || "$mode" == verify-closed ]] || exit 75
  staged_closure=true
  release="$staged/payload"
  release_sha=""
elif [[ "$script_path" =~ ^/home/ubuntu/qintopia-agent-os-releases/([0-9a-f]{40})/deploy/runner/management-ui-lifecycle\.sh$ ]]; then
  release_sha="${BASH_REMATCH[1]}"
  release="${release_root}/${release_sha}"
else
  echo "management UI lifecycle requires a fixed immutable release" >&2
  exit 75
fi

if [[ "$staged_closure" != true ]]; then
python3 - "$release" "$release_sha" <<'PY'
import hashlib
import json
import os
import stat
import sys
from pathlib import Path

root, sha = Path(sys.argv[1]), sys.argv[2]
for relative in ("manifest.json", "deploy-bundle/artifact-manifest.json"):
    info = (root / relative).lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or stat.S_IMODE(info.st_mode) != 0o444:
        raise SystemExit("management UI release metadata drifted")
manifest = json.loads((root / "manifest.json").read_text())
artifact = json.loads((root / "deploy-bundle/artifact-manifest.json").read_text())
if (manifest.get("release_sha") != sha or
        artifact.get("commit_sha") != manifest.get("deploy_bundle_sha")):
    raise SystemExit("management UI release identity is invalid")
for relative, mode in (("deploy/runner/management-ui-lifecycle.sh", 0o755),
                       ("runtime/nginx/templates/management-ui-http.conf.template", 0o644),
                       ("runtime/nginx/templates/management-ui-https.conf.template", 0o644)):
    file = root / relative
    info = file.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or stat.S_IMODE(info.st_mode) != mode:
        raise SystemExit("management UI immutable file metadata drifted")
    matches = [item for item in artifact.get("files", [])
               if item.get("path") == "payload/" + relative]
    if len(matches) != 1 or hashlib.sha256(file.read_bytes()).hexdigest() != matches[0].get("sha256"):
        raise SystemExit("management UI deploy bundle file digest drifted")
PY
fi

check_inherited_lock() {
  python3 - "$state/deploy.lock" <<'PY'
import fcntl
import os
import stat
import sys

lock_path = sys.argv[1]
path = os.lstat(lock_path)
held = os.fstat(9)
if (not stat.S_ISREG(path.st_mode) or not stat.S_ISREG(held.st_mode) or
        path.st_uid != 0 or path.st_nlink != 1 or
        (path.st_dev, path.st_ino) != (held.st_dev, held.st_ino)):
    raise SystemExit("inherited deploy lock identity changed")
probe = os.open(lock_path, os.O_RDONLY | os.O_NOFOLLOW | os.O_CLOEXEC)
try:
    independent = os.fstat(probe)
    if (independent.st_dev, independent.st_ino) != (held.st_dev, held.st_ino):
        raise SystemExit("deploy lock path changed during verification")
    try:
        fcntl.flock(probe, fcntl.LOCK_EX | fcntl.LOCK_NB)
    except BlockingIOError:
        pass
    else:
        fcntl.flock(probe, fcntl.LOCK_UN)
        raise SystemExit("deploy lock was not held before the helper")
finally:
    os.close(probe)
try:
    fcntl.flock(9, fcntl.LOCK_EX | fcntl.LOCK_NB)
except BlockingIOError:
    raise SystemExit("deploy lock FD 9 is not the inherited owner") from None
PY
}

check_maintenance_hold() {
  python3 - "$state" <<'PY'
import json
import os
import re
import stat
import sys
from pathlib import Path

state = Path(sys.argv[1])
hold = state / "recovery/hold"
info = hold.lstat()
if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or
        info.st_nlink != 1 or stat.S_IMODE(info.st_mode) != 0o600):
    raise SystemExit("management UI hold metadata is invalid")
token = hold.read_text(encoding="ascii").strip()
takeover = state / "recovery/takeover.json"
request_id = ""
if re.fullmatch(r"[0-9a-f]{32}", token) and takeover.is_file():
    record = json.loads(takeover.read_text())
    if record.get("hold_token") == token:
        request_id = record.get("request_id", "")
elif re.fullmatch(r"deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}", token):
    journal = state / "recovery" / (token + ".json")
    if journal.is_file() and json.loads(journal.read_text()).get("request_id") == token:
        request_id = token
if not re.fullmatch(r"deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}", request_id):
    raise SystemExit("management UI hold is not request-bound")
for claim in (state / "requests/claimed").glob("*.json"):
    if claim.stem != request_id:
        raise SystemExit("another deploy request claim is present")
PY
}

if [[ "$mode" == quiesce || "$mode" == verify-closed ]]; then
  check_inherited_lock
else
  exec 8>"$state/poller.lock"
  flock -n 8 || { echo "poller lock is held" >&2; exit 75; }
  exec 9>"$state/deploy.lock"
  flock -n 9 || { echo "deploy lock is held" >&2; exit 75; }
  check_inherited_lock
  [[ "$(readlink -f "$release_root/current")" == "$release" ]] || {
    echo "management UI maintenance release is not current" >&2
    exit 75
  }
  python3 - "$state/poller.lock" <<'PY'
import os, stat, sys
path, held = os.lstat(sys.argv[1]), os.fstat(8)
if (not stat.S_ISREG(path.st_mode) or path.st_uid != 0 or path.st_nlink != 1 or
        (path.st_dev, path.st_ino) != (held.st_dev, held.st_ino)):
    raise SystemExit("poller lock identity changed")
PY
  if [[ "$mode" == activate ]]; then
    [[ ! -e "$state/recovery/hold" && ! -L "$state/recovery/hold" ]] || exit 75
    if compgen -G "$state/requests/claimed/*.json" >/dev/null; then exit 75; fi
  elif [[ "$mode" == begin || "$mode" == finish ]]; then
    :
  else
    check_maintenance_hold
  fi
fi

snapshot() {
  python3 - "$unit" "$release_root" <<'PY'
import json
import os
import re
import subprocess
import sys
import hashlib
import stat
from pathlib import Path

unit, root = sys.argv[1:3]
keys = ("LoadState", "ActiveState", "SubState", "UnitFileState", "InvocationID",
        "Result", "ExecMainCode", "ExecMainStatus", "MainPID", "ControlPID",
        "ControlGroup", "NRestarts")
result = subprocess.run(["/usr/bin/systemctl", "show", unit] +
                        ["--property=" + key for key in keys],
                        capture_output=True, text=True, check=True)
data = dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
if any(key not in data for key in keys):
    raise SystemExit("management UI systemd status is incomplete")
if data["LoadState"] not in ("loaded", "not-found"):
    raise SystemExit("management UI load state is unknown")
pid = int(data["MainPID"])
if pid:
    executable = os.readlink(f"/proc/{pid}/exe")
    if not re.fullmatch(re.escape(root) + r"/[0-9a-f]{40}/sidecar/qintopia-message-sidecar", executable):
        raise SystemExit("management UI executable is outside an immutable release")
    metadata = os.stat(f"/proc/{pid}/exe")
    installed = Path(executable).parent.parent
    manifest_path = installed / "manifest.json"
    artifact_path = installed / "sidecar/artifact-manifest.json"
    for path in (manifest_path, artifact_path, Path(executable)):
        info = path.lstat()
        if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
            raise SystemExit("management UI running release metadata is unsafe")
    manifest = json.loads(manifest_path.read_bytes())
    artifact = json.loads(artifact_path.read_bytes())
    matches = [f for f in artifact.get("files", []) if f.get("path") == "qintopia-message-sidecar"]
    if (manifest.get("release_sha") != installed.name or
            artifact.get("commit_sha") != manifest.get("runtime_sha") or len(matches) != 1 or
            hashlib.sha256(Path(executable).read_bytes()).hexdigest() != matches[0].get("sha256") or
            (Path(executable).stat().st_dev, Path(executable).stat().st_ino) !=
                (metadata.st_dev, metadata.st_ino)):
        raise SystemExit("management UI running release identity drifted")
    data["ExecutableIdentity"] = [metadata.st_dev, metadata.st_ino, executable]
else:
    data["ExecutableIdentity"] = None
invocation = data["InvocationID"]
data["JournalStopSuccess"] = False
if not invocation and data["LoadState"] == "loaded" and data["ActiveState"] == "inactive":
    # systemd clears the invocation and exit code on stop; manager entries retain them.
    stopped = subprocess.run(["/usr/bin/journalctl", "--no-pager", "-n", "1",
                              "-o", "json", "UNIT=" + unit],
                             capture_output=True, text=True, check=True)
    if stopped.stdout.strip():
        event = json.loads(stopped.stdout.strip().splitlines()[-1])
        if (event.get("UNIT") != unit or event.get("JOB_TYPE") != "stop" or
                event.get("JOB_RESULT") != "done" or
                event.get("MESSAGE_ID") != "9d1aaa27d60140bd96365438aad20286"):
            raise SystemExit("management UI final manager event is not a successful stop")
        invocation = event.get("INVOCATION_ID", "")
        if not re.fullmatch(r"[0-9a-f]{32}", invocation):
            raise SystemExit("management UI stop invocation identity is invalid")
        deactivated = subprocess.run(
            ["/usr/bin/journalctl", "--no-pager", "-n", "1", "-o", "json",
             "UNIT=" + unit, "INVOCATION_ID=" + invocation,
             "MESSAGE_ID=7ad2d189f7e94e70a38c781354912448"],
            capture_output=True, text=True, check=True)
        if not deactivated.stdout.strip():
            raise SystemExit("management UI successful deactivation journal is missing")
        success_event = json.loads(deactivated.stdout.strip().splitlines()[-1])
        if (success_event.get("UNIT") != unit or
                success_event.get("INVOCATION_ID") != invocation or
                success_event.get("MESSAGE_ID") != "7ad2d189f7e94e70a38c781354912448"):
            raise SystemExit("management UI deactivation journal identity changed")
        data["JournalStopSuccess"] = True
    elif data["ExecMainCode"] not in ("0", ""):
        raise SystemExit("management UI stop invocation journal is missing")
    data["InvocationID"] = invocation
data["Unknown"] = False
if invocation:
    if not re.fullmatch(r"[0-9a-f]{32}", invocation):
        raise SystemExit("management UI invocation identity is invalid")
    journal = subprocess.run(["/usr/bin/journalctl", "--no-pager", "-o", "cat",
                              "_SYSTEMD_INVOCATION_ID=" + invocation],
                             capture_output=True, text=True, check=True)
    data["Unknown"] = any(code in journal.stdout for code in (
        "production_ui_request_outcome_unknown", "production_ui_stop_outcome_unknown",
        "configuration_commit_outcome_unknown"))
print(json.dumps(data, separators=(",", ":")))
PY
}

expect_route_code() {
  local expected="$1" route="$2" resolution="$3" observed
  for _ in 1 2 3 4 5 6 7 8; do
    observed="$(curl --noproxy '*' --silent --show-error --output /dev/null \
      --write-out '%{http_code}' --resolve "$resolution" --max-time 2 "$route")" || observed=""
    [[ "$observed" == "$expected" ]] && return 0
    sleep 0.25
  done
  return 75
}

check_closed() {
  python3 - "$1" "${2:-}" "$release_root" <<'PY'
import json
import os
import socket
import sys
from pathlib import Path

after = json.loads(sys.argv[1])
before = json.loads(sys.argv[2]) if sys.argv[2] else None
if before and (before["InvocationID"] != after["InvocationID"] or
               before["NRestarts"] != after["NRestarts"] or
               (before["Unknown"] or after["Unknown"])):
    raise SystemExit("management UI invocation changed or outcome is unknown")
if after["Unknown"]:
    raise SystemExit("management UI outcome is unknown")
if after["MainPID"] != "0" or after["ControlPID"] != "0":
    raise SystemExit("management UI process remains")
if after["NRestarts"] != "0":
    raise SystemExit("management UI restart history is unexpected")
if after["LoadState"] == "loaded":
    if (after["UnitFileState"] != "disabled" or after["ActiveState"] != "inactive" or
            after["MainPID"] != "0" or after["ControlPID"] != "0"):
        raise SystemExit("management UI unit is not disabled and inactive")
    if (after["Result"] != "success" or after["ExecMainStatus"] != "0" or
            (after["InvocationID"] and not after["JournalStopSuccess"] and
             after["ExecMainCode"] not in ("1", "exited")) or
            (after["JournalStopSuccess"] and
             after["ExecMainCode"] not in ("0", "1", "exited"))):
        raise SystemExit("management UI exit was abnormal")
elif after["ActiveState"] != "inactive":
    raise SystemExit("absent management UI unit is not inactive")
group = after["ControlGroup"]
if group:
    if not group.startswith("/") or ".." in group:
        raise SystemExit("management UI cgroup path is invalid")
    events = Path("/sys/fs/cgroup" + group + "/cgroup.events")
    if not events.is_file() or "populated 0" not in events.read_text().splitlines():
        raise SystemExit("management UI cgroup is populated or unreadable")
with socket.socket(socket.AF_INET, socket.SOCK_STREAM) as listener:
    listener.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
    try:
        listener.bind(("127.0.0.1", 18780))
    except OSError:
        raise SystemExit("management UI loopback port is occupied") from None
for proc in Path("/proc").iterdir():
    if not proc.name.isdecimal():
        continue
    try:
        parts = (proc / "cmdline").read_bytes().split(b"\0")
        if b"run-collaboration-production-ui" in parts and b"--port" in parts and b"18780" in parts:
            raise SystemExit("management UI executable process remains")
    except (FileNotFoundError, PermissionError, ProcessLookupError):
        continue
PY
  if [[ -e "$site" || -L "$site" || -e "$enabled" || -L "$enabled" ]]; then
    [[ -f "$site" && ! -L "$site" && -L "$enabled" && "$(readlink "$enabled")" == "$site" ]] || {
      echo "management UI site linkage is incomplete" >&2
      return 75
    }
    if cmp -s "$site" "$release/runtime/nginx/templates/management-ui-https.conf.template"; then
      expect_route_code 503 https://agentos.qintopia.cn/ agentos.qintopia.cn:443:127.0.0.1 || {
        echo "management UI HTTPS route did not return 503" >&2
        return 75
      }
    elif cmp -s "$site" "$release/runtime/nginx/templates/management-ui-http.conf.template"; then
      expect_route_code 404 http://agentos.qintopia.cn/ agentos.qintopia.cn:80:127.0.0.1 || {
        echo "management UI HTTP bootstrap route did not return 404" >&2
        return 75
      }
    else
      echo "management UI site content drifted" >&2
      return 75
    fi
  fi
}

check_env() {
  python3 - "${1:-$env_file}" <<'PY'
import os
import re
import stat
import sys
from pathlib import Path
from urllib.parse import unquote, urlsplit

file = Path(sys.argv[1])
info = file.lstat()
if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_gid != 0 or
        info.st_nlink != 1 or stat.S_IMODE(info.st_mode) != 0o600):
    raise SystemExit("management UI environment metadata is invalid")
raw = file.read_bytes()
if not raw or len(raw) > 8192 or not raw.endswith(b"\n") or b"\r" in raw:
    raise SystemExit("management UI environment format is invalid")
expected = {"QINTOPIA_FOUNDATION_PRODUCTION_ENABLE", "QINTOPIA_FOUNDATION_TENANT",
            "QINTOPIA_FOUNDATION_IDENTITY_NAMESPACE", "QINTOPIA_FOUNDATION_DATABASE_URL",
            "QINTOPIA_COLLABORATION_PUBLIC_ORIGIN"}
values = {}
for line in raw.decode("ascii").splitlines():
    if not re.fullmatch(r"[A-Z_]+=[!-~]+", line) or line.count("=") < 1:
        raise SystemExit("management UI environment line is invalid")
    key, value = line.split("=", 1)
    if key not in expected or key in values or any(char in value for char in "'\"\\`$#;"):
        raise SystemExit("management UI environment keys or syntax are invalid")
    values[key] = value
if set(values) != expected or values["QINTOPIA_FOUNDATION_PRODUCTION_ENABLE"] != "1" or \
        values["QINTOPIA_COLLABORATION_PUBLIC_ORIGIN"] != "https://agentos.qintopia.cn":
    raise SystemExit("management UI environment contract is invalid")
if not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", values["QINTOPIA_FOUNDATION_TENANT"]) or \
        not re.fullmatch(r"[A-Za-z0-9_.:-]{1,128}", values["QINTOPIA_FOUNDATION_IDENTITY_NAMESPACE"]):
    raise SystemExit("management UI tenant or namespace is invalid")
database = urlsplit(values["QINTOPIA_FOUNDATION_DATABASE_URL"])
if (database.scheme not in ("postgres", "postgresql") or not database.hostname or
        unquote(database.username or "") != "qintopia_management_ui" or
        not database.password or
        not database.path.strip("/") or "test" in database.path.lower() or
        database.fragment):
    raise SystemExit("management UI database URL is not the restricted live role")
PY
}

check_account() {
  python3 <<'PY'
import grp
import pwd

user = pwd.getpwnam("qintopia-management-ui")
group = grp.getgrnam("qintopia-management-ui")
if (user.pw_gid != group.gr_gid or user.pw_dir != "/nonexistent" or
        user.pw_shell != "/usr/sbin/nologin"):
    raise SystemExit("management UI account attributes are invalid")
for entry in grp.getgrall():
    if user.pw_name in entry.gr_mem:
        raise SystemExit("management UI account has supplementary group membership")
PY
}

check_certificate() {
  [[ -f "$certificate" && -f "$renewal" ]] || return 75
  /usr/bin/openssl x509 -in "$certificate" -noout -checkend 604800 >/dev/null
  python3 - "$certificate" <<'PY'
import ssl
import sys

certificate = ssl._ssl._test_decode_cert(sys.argv[1])
if certificate.get("subjectAltName") != (("DNS", "agentos.qintopia.cn"),):
    raise SystemExit("management UI certificate SAN is not exact")
PY
}

check_https_site() {
  [[ -L "$enabled" && "$(readlink "$enabled")" == "$site" ]] || return 75
  cmp -s "$site" "$release/runtime/nginx/templates/management-ui-https.conf.template"
  check_certificate
}

maintenance_record="$state/recovery/management-ui-maintenance.json"
maintenance_timer=qintopia-agent-os-deploy-runner.timer
maintenance_dropin=/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf
maintenance_source="$release/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"

maintenance_timer_state() {
  local file_state active_state
  file_state="$(/usr/bin/systemctl show "$maintenance_timer" --property=UnitFileState --value)"
  active_state="$(/usr/bin/systemctl show "$maintenance_timer" --property=ActiveState --value)"
  [[ "$file_state" == enabled || "$file_state" == disabled ]] || return 75
  if [[ "${1:-}" == stopped ]]; then
    [[ "$file_state" == disabled && "$active_state" == inactive ]] || return 75
  fi
  printf '%s\n' "$file_state"
}

maintenance_service_stopped() {
  python3 - "$1" "${2:-false}" <<'PY'
import subprocess
import sys

unit, allow_missing = sys.argv[1:3]
result = subprocess.run(["/usr/bin/systemctl", "show", unit,
                         "--property=LoadState,ActiveState,MainPID,ControlPID,ControlGroup"],
                        capture_output=True, text=True, check=True)
data = dict(line.split("=", 1) for line in result.stdout.splitlines() if "=" in line)
if (data.get("LoadState") not in (("loaded", "not-found") if allow_missing == "true" else ("loaded",)) or
        data.get("ActiveState") != "inactive" or data.get("MainPID") != "0" or
        data.get("ControlPID") != "0"):
    raise SystemExit("management UI maintenance consumer is active or unknown")
group = data.get("ControlGroup", "")
if group:
    from pathlib import Path
    if (not group.startswith("/") or ".." in group or
            "populated 0" not in Path("/sys/fs/cgroup" + group + "/cgroup.events").read_text().splitlines()):
        raise SystemExit("management UI maintenance consumer cgroup is populated")
PY
}

maintenance_evidence() {
  local request_id="$1" request_file result_file journal_file signing_env
  request_file="$state/requests/processed/$request_id.json"
  result_file="$state/results/$request_id.json"
  journal_file="$state/recovery/$request_id.json"
  signing_env=/etc/qintopia/cos-artifacts.env
  [[ -f "$request_file" && ! -L "$request_file" && -f "$result_file" && ! -L "$result_file" &&
    -f "$journal_file" && ! -L "$journal_file" && -f "$signing_env" && ! -L "$signing_env" &&
    "$(stat -c '%u:%g:%a:%h' "$signing_env")" == 0:0:600:1 ]] || return 75
  (
    set -a
    # The existing result verifier needs the same private signing environment as recovery.
    # shellcheck disable=SC1090
    source "$signing_env"
    set +a
    "$release/deploy/runner/wait-deploy-result.sh" --request-file "$request_file" \
      --result-file "$result_file" --verify-archived-request >/dev/null
  ) || return 75
  python3 - "$state" "$release_root" "$release_sha" "$request_id" <<'PY'
import hashlib
import json
import os
import re
import stat
import sys
from pathlib import Path

state, root, sha, request_id = Path(sys.argv[1]), Path(sys.argv[2]), sys.argv[3], sys.argv[4]
state_info = state.lstat()
if (not stat.S_ISDIR(state_info.st_mode) or state_info.st_uid != 0 or
        stat.S_IMODE(state_info.st_mode) != 0o700):
    raise SystemExit("management UI maintenance state directory is not private")
request_path = state / "requests/processed" / (request_id + ".json")
result_path = state / "results" / (request_id + ".json")
journal_path = state / "recovery" / (request_id + ".json")
for path, allowed_modes in ((request_path, (0o600, 0o644)),
                            (result_path, (0o600, 0o644)),
                            (journal_path, (0o600,))):
    info = path.lstat()
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_nlink != 1 or
            stat.S_IMODE(info.st_mode) not in allowed_modes):
        raise SystemExit("management UI maintenance evidence metadata is invalid")
request_bytes, result_bytes = request_path.read_bytes(), result_path.read_bytes()
request, result = json.loads(request_bytes), json.loads(result_bytes)
journal = json.loads(journal_path.read_bytes())
target = root / sha
if (not (root / "current").is_symlink() or (root / "current").resolve(strict=True) != target or
        not (root / "previous").is_symlink()):
    raise SystemExit("management UI maintenance pointers are invalid")
previous = (root / "previous").resolve(strict=True)
if previous.parent != root or not re.fullmatch(r"[0-9a-f]{40}", previous.name):
    raise SystemExit("management UI maintenance previous pointer is invalid")
for path in (target / "manifest.json", previous / "manifest.json"):
    info = path.lstat()
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_nlink != 1 or
            stat.S_IMODE(info.st_mode) != 0o444):
        raise SystemExit("management UI maintenance manifest metadata is invalid")
with (target / "manifest.json").open() as file:
    manifest = json.load(file)
with (previous / "manifest.json").open() as file:
    previous_manifest = json.load(file)
ancestor = previous_manifest.get("previous_sha", "")
if not re.fullmatch(r"[0-9a-f]{40}", ancestor):
    raise SystemExit("management UI maintenance ancestor is invalid")
ancestor_manifest = root / ancestor / "manifest.json"
info = ancestor_manifest.lstat()
if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_nlink != 1 or
        stat.S_IMODE(info.st_mode) != 0o444):
    raise SystemExit("management UI maintenance ancestor metadata is invalid")
targets = {"qintopia-system-services", "hermes-erhua", "hermes-xiaoman",
           "hermes-silaoshi", "hermes-huabaosi", "hermes-anan"}
scope = ["sidecar-runtime", "deploy-bundle", "hermes-plugins"]
if (request.get("request_id") != request_id or request.get("environment") != "production" or
        request.get("dry_run") is not False or request.get("release_sha") != sha or
        request.get("release_scope") != scope or
        set(request.get("restart_targets", [])) != targets or
        len(request.get("restart_targets", [])) != len(targets) or
        any(request.get(key) != sha for key in ("commit_sha", "runtime_sha", "deploy_bundle_sha")) or
        manifest.get("release_sha") != sha or manifest.get("previous_sha") != previous.name or
        previous_manifest.get("release_sha") != previous.name or
        manifest.get("release_scope") != scope or
        manifest.get("restart_targets") != request.get("restart_targets") or
        any(manifest.get(key) != request.get(key) for key in
            ("commit_sha", "runtime_sha", "deploy_bundle_sha", "runtime_artifact_profile"))):
    raise SystemExit("management UI maintenance release identity is invalid")
if (result.get("request_id") != request_id or result.get("status") != "succeeded" or
        result.get("previous_sha") != previous.name or result.get("current_target") != str(target) or
        result.get("rollback", {}).get("attempted") is not False or
        any(result.get(key) != request.get(key) for key in
            ("commit_sha", "runtime_sha", "deploy_bundle_sha", "runtime_artifact_profile",
             "release_scope", "restart_targets")) or
        not any(check.get("name") == "deploy-runner" and check.get("status") == "passed"
                for check in result.get("checks", []))):
    raise SystemExit("management UI maintenance signed success is invalid")
upload = journal.get("result_upload", {})
if (journal.get("schema_version") != 1 or journal.get("request_id") != request_id or
        journal.get("request_sha256") != hashlib.sha256(request_bytes).hexdigest() or
        journal.get("direction") != "T→R" or journal.get("phase") != "smoke-passed" or
        journal.get("hold_token") != request_id or
        journal.get("original_current_sha") != previous.name or
        journal.get("original_previous_sha") != ancestor or
        journal.get("manifest_sha256", {}).get("current") !=
            hashlib.sha256((previous / "manifest.json").read_bytes()).hexdigest() or
        journal.get("manifest_sha256", {}).get("previous") !=
            hashlib.sha256(ancestor_manifest.read_bytes()).hexdigest() or
        upload.get("phase") != "upload_intent" or
        upload.get("payload_sha256") != hashlib.sha256(result_bytes).hexdigest()):
    raise SystemExit("management UI maintenance journal is invalid")
if any((state / "requests/claimed").glob("*.json")):
    raise SystemExit("management UI maintenance has an unfinished claim")
for path in (state / "recovery").glob("deploy-*.json"):
    if path.name > journal_path.name:
        raise SystemExit("a later recovery journal owns the maintenance state")
artifact = json.loads((target / "deploy-bundle/artifact-manifest.json").read_bytes())
for name, expected_mode in (("deploy/runner/wait-deploy-result.sh", 0o755),
                            ("deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf", 0o644)):
    path = target / name
    info = path.lstat()
    matches = [item for item in artifact.get("files", []) if item.get("path") == "payload/" + name]
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or
            stat.S_IMODE(info.st_mode) != expected_mode or len(matches) != 1 or
            hashlib.sha256(path.read_bytes()).hexdigest() != matches[0].get("sha256")):
        raise SystemExit("management UI maintenance helper identity drifted")
PY
}

maintenance_state() {
  python3 - "$maintenance_record" "$state" "$release_sha" "$1" "$2" "${3:-}" <<'PY'
import hashlib
import json
import os
import stat
import sys
import tempfile
from pathlib import Path

path, state, sha, action, request_id, timer_state = Path(sys.argv[1]), Path(sys.argv[2]), *sys.argv[3:]
request = state / "requests/processed" / (request_id + ".json")
result = state / "results" / (request_id + ".json")
def digest(file):
    return hashlib.sha256(file.read_bytes()).hexdigest()
if action == "create" and not path.exists() and not path.is_symlink():
    if timer_state not in ("enabled", "disabled"):
        raise SystemExit("maintenance timer state is invalid")
    record = {"schema_version": 1, "request_id": request_id, "release_sha": sha,
              "timer_was_enabled": timer_state == "enabled", "phase": "preparing",
              "request_sha256": digest(request), "result_sha256": digest(result)}
    descriptor = os.open(path, os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW, 0o600)
    with os.fdopen(descriptor, "w", encoding="ascii") as file:
        json.dump(record, file, sort_keys=True)
        file.write("\n")
        file.flush()
        os.fsync(file.fileno())
    directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    os.fsync(directory)
    os.close(directory)
info = path.lstat()
if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_nlink != 1 or
        stat.S_IMODE(info.st_mode) != 0o600):
    raise SystemExit("maintenance record metadata is invalid")
record = json.loads(path.read_bytes())
if (record.get("schema_version") != 1 or record.get("request_id") != request_id or
        record.get("release_sha") != sha or type(record.get("timer_was_enabled")) is not bool or
        record.get("phase") not in ("preparing", "isolated", "finishing") or
        record.get("request_sha256") != digest(request) or
        record.get("result_sha256") != digest(result)):
    raise SystemExit("maintenance record identity changed")
if action == "create" and timer_state and record["timer_was_enabled"] != (timer_state == "enabled"):
    raise SystemExit("maintenance timer history conflicts")
if action == "isolated" and record["phase"] == "preparing":
    record["phase"] = "isolated"
elif action == "finishing" and record["phase"] == "isolated":
    record["phase"] = "finishing"
elif action not in ("create", "read", record["phase"]):
    raise SystemExit("maintenance phase transition is invalid")
if action in ("isolated", "finishing"):
    descriptor, temporary = tempfile.mkstemp(prefix=".management-ui-maintenance-", dir=path.parent)
    try:
        with os.fdopen(descriptor, "w", encoding="ascii") as file:
            json.dump(record, file, sort_keys=True)
            file.write("\n")
            file.flush()
            os.fsync(file.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
        directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
        os.fsync(directory)
        os.close(directory)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
elif action == "read":
    descriptor = os.open(path, os.O_RDONLY | os.O_NOFOLLOW)
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    directory = os.open(path.parent, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
    finally:
        os.close(directory)
print(record["phase"] + "\t" + ("enabled" if record["timer_was_enabled"] else "disabled"))
PY
}

maintenance_hold() {
  python3 - "$state/recovery" "$1" "$2" <<'PY'
import os
import stat
import sys

root, request_id, action = sys.argv[1:4]
if action not in ("create", "verify", "remove", "finalize"):
    raise SystemExit("maintenance hold action is invalid")
directory = os.open(root, os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
try:
    if action == "finalize":
        if os.path.lexists(os.path.join(root, "hold")):
            raise SystemExit("maintenance hold still exists")
        os.fsync(directory)
        raise SystemExit(0)
    if action == "create":
        try:
            descriptor = os.open("hold", os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                                 0o600, dir_fd=directory)
        except FileExistsError:
            descriptor = None
        if descriptor is not None:
            with os.fdopen(descriptor, "w", encoding="ascii") as file:
                file.write(request_id + "\n")
                file.flush()
                os.fsync(file.fileno())
            os.fsync(directory)
    descriptor = os.open("hold", os.O_RDONLY | os.O_NOFOLLOW, dir_fd=directory)
    try:
        info = os.fstat(descriptor)
        if (not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_nlink != 1 or
                stat.S_IMODE(info.st_mode) != 0o600 or
                os.read(descriptor, 256) != (request_id + "\n").encode("ascii")):
            raise SystemExit("maintenance hold belongs to another request")
        if action == "create":
            os.fsync(descriptor)
    finally:
        os.close(descriptor)
    if action == "create":
        os.fsync(directory)
    if action == "remove":
        os.unlink("hold", dir_fd=directory)
        os.fsync(directory)
finally:
    os.close(directory)
PY
}

case "$mode" in
  begin)
    request_id="$2"
    check_closed "$(snapshot)"
    maintenance_evidence "$request_id"
    if [[ -e "$maintenance_record" || -L "$maintenance_record" ]]; then
      maintenance_status="$(maintenance_state read "$request_id")" || exit 75
      IFS=$'\t' read -r phase timer_original <<<"$maintenance_status"
      [[ "$phase" == preparing || "$phase" == isolated ]] || exit 75
    else
      [[ ! -e "$state/recovery/hold" && ! -L "$state/recovery/hold" ]] || exit 75
      timer_original="$(maintenance_timer_state)"
      maintenance_status="$(maintenance_state create "$request_id" "$timer_original")" || exit 75
      IFS=$'\t' read -r phase timer_original <<<"$maintenance_status"
    fi
    /usr/bin/systemctl disable --now "$maintenance_timer"
    maintenance_timer_state stopped >/dev/null
    maintenance_service_stopped qintopia-agent-os-deploy-runner.service
    maintenance_service_stopped qintopia-agent-os-fixed-takeover.service true
    maintenance_service_stopped qintopia-agent-os-anan-drain-restart.service true
    maintenance_evidence "$request_id"
    check_closed "$(snapshot)"
    maintenance_hold "$request_id" create
    [[ -f "$maintenance_source" && ! -L "$maintenance_source" ]] || exit 75
    if [[ -e "$maintenance_dropin" || -L "$maintenance_dropin" ]]; then
      [[ -f "$maintenance_dropin" && ! -L "$maintenance_dropin" ]] || exit 75
      cmp -s "$maintenance_source" "$maintenance_dropin" || exit 75
    else
      install -d -m 0755 "$(dirname "$maintenance_dropin")"
      install -m 0644 "$maintenance_source" "$maintenance_dropin"
    fi
    /usr/bin/systemctl daemon-reload
    cmp -s "$maintenance_source" "$maintenance_dropin"
    maintenance_timer_state stopped >/dev/null
    maintenance_service_stopped qintopia-agent-os-deploy-runner.service
    check_maintenance_hold
    maintenance_state isolated "$request_id" >/dev/null
    ;;
  finish)
    request_id="$2"
    maintenance_evidence "$request_id"
    maintenance_status="$(maintenance_state read "$request_id")" || exit 75
    IFS=$'\t' read -r phase timer_original <<<"$maintenance_status"
    [[ "$phase" == isolated || "$phase" == finishing ]] || exit 75
    [[ -f "$maintenance_dropin" && ! -L "$maintenance_dropin" ]] || exit 75
    cmp -s "$maintenance_source" "$maintenance_dropin" || exit 75
    check_https_site
    check_closed "$(snapshot)"
    if [[ ! -e "$state/recovery/hold" && ! -L "$state/recovery/hold" ]]; then
      [[ "$phase" == finishing ]] || exit 75
      if [[ "$timer_original" == enabled ]]; then
        [[ "$(maintenance_timer_state)" == enabled ]]
        /usr/bin/systemctl is-active --quiet "$maintenance_timer"
      else
        maintenance_timer_state stopped >/dev/null
      fi
      maintenance_hold "$request_id" finalize
    else
      maintenance_hold "$request_id" verify
      maintenance_state finishing "$request_id" >/dev/null
      if [[ "$timer_original" == enabled ]]; then
        /usr/bin/systemctl enable --now "$maintenance_timer"
        [[ "$(maintenance_timer_state)" == enabled ]]
        /usr/bin/systemctl is-active --quiet "$maintenance_timer"
      else
        maintenance_timer_state stopped >/dev/null
      fi
      maintenance_hold "$request_id" remove
    fi
    ;;
  prepare)
    if ! getent passwd qintopia-management-ui >/dev/null; then
      /usr/sbin/useradd --system --user-group --no-create-home \
        --home-dir /nonexistent --shell /usr/sbin/nologin qintopia-management-ui
    fi
    check_account
    [[ ! -e "$env_file" && ! -L "$env_file" ]] || {
      echo "management UI environment already exists; rotation needs separate review" >&2
      exit 75
    }
    [[ -d /etc/qintopia && ! -L /etc/qintopia ]] || exit 75
    temporary="$(mktemp /etc/qintopia/.collaboration-management-ui.XXXXXX)"
    trap 'rm -f -- "$temporary"' EXIT
    python3 - "$temporary" 3<&0 <<'PY'
import os
import sys

target = sys.argv[1]
chunks = []
total = 0
while True:
    chunk = os.read(3, 8193 - total)
    if not chunk:
        break
    total += len(chunk)
    if total > 8192:
        raise SystemExit("management UI environment input is too large")
    chunks.append(chunk)
payload = b"".join(chunks)
fd = os.open(target, os.O_WRONLY | os.O_TRUNC | os.O_NOFOLLOW)
try:
    os.write(fd, payload)
    os.fsync(fd)
finally:
    os.close(fd)
PY
    if ! check_env "$temporary"; then
      rm -f "$temporary"
      exit 75
    fi
    ln "$temporary" "$env_file"
    rm "$temporary"
    check_env
    trap - EXIT
    ;;
  install-http)
    [[ ! -e "$site" && ! -L "$site" && ! -e "$enabled" && ! -L "$enabled" ]] || exit 75
    existing_config="$(/usr/sbin/nginx -T 2>/dev/null)"
    if grep -Fq 'server_name agentos.qintopia.cn' <<<"$existing_config"; then
      echo "management UI host is already configured" >&2
      exit 75
    fi
    temporary="$(mktemp /etc/nginx/sites-available/.qintopia-management-ui.XXXXXX)"
    if ! { install -m 0644 "$release/runtime/nginx/templates/management-ui-http.conf.template" "$temporary" &&
      mv "$temporary" "$site" && ln -s "$site" "$enabled" &&
      /usr/sbin/nginx -t && /usr/bin/systemctl reload nginx &&
      check_closed "$(snapshot)"; }; then
      [[ ! -L "$enabled" || "$(readlink "$enabled")" != "$site" ]] || rm "$enabled"
      if [[ -f "$site" ]] && cmp -s "$site" "$release/runtime/nginx/templates/management-ui-http.conf.template"; then
        rm "$site"
      fi
      rm -f "$temporary"
      /usr/sbin/nginx -t
      /usr/bin/systemctl reload nginx
      exit 75
    fi
    ;;
  issue-cert)
    [[ -L "$enabled" && "$(readlink "$enabled")" == "$site" ]] || exit 75
    cmp -s "$site" "$release/runtime/nginx/templates/management-ui-http.conf.template" || exit 75
    [[ ! -e "$renewal" && ! -L "$renewal" && ! -e "$certificate" ]] || {
      echo "management UI certificate state already exists; inspect before retry" >&2
      exit 75
    }
    [[ -d /etc/letsencrypt/accounts ]] || exit 75
    /usr/bin/certbot certonly --nginx --non-interactive \
      --cert-name qintopia-management-ui -d agentos.qintopia.cn || {
      echo "certificate result requires inspection; do not retry blindly" >&2
      exit 75
    }
    check_certificate
    cmp -s "$site" "$release/runtime/nginx/templates/management-ui-http.conf.template" || {
      echo "certificate issuance changed the reviewed HTTP site" >&2
      exit 75
    }
    ;;
  install-https)
    [[ -L "$enabled" && "$(readlink "$enabled")" == "$site" ]] || exit 75
    cmp -s "$site" "$release/runtime/nginx/templates/management-ui-http.conf.template" || exit 75
    check_certificate
    /usr/bin/certbot renew --cert-name qintopia-management-ui --dry-run
    prior="$(mktemp /etc/nginx/sites-available/.qintopia-management-ui-prior.XXXXXX)"
    cp -p "$site" "$prior"
    temporary="$(mktemp /etc/nginx/sites-available/.qintopia-management-ui.XXXXXX)"
    if ! { install -m 0644 "$release/runtime/nginx/templates/management-ui-https.conf.template" "$temporary" &&
      mv "$temporary" "$site" && /usr/sbin/nginx -t &&
      /usr/bin/systemctl reload nginx && check_closed "$(snapshot)"; }; then
      mv "$prior" "$site"
      rm -f "$temporary"
      /usr/sbin/nginx -t
      /usr/bin/systemctl reload nginx
      exit 75
    fi
    rm -f "$prior"
    check_https_site
    ;;
  activate)
    [[ "$(readlink -f "$release_root/current")" == "$release" ]] || exit 75
    python3 - "$release/manifest.json" <<'PY'
import json, sys
with open(sys.argv[1], encoding="utf-8") as file:
    manifest = json.load(file)
if manifest.get("release_scope") != ["sidecar-runtime", "deploy-bundle", "hermes-plugins"]:
    raise SystemExit("management UI activation requires the approved R release")
PY
    check_account
    check_env
    check_https_site
    /usr/sbin/runuser -u qintopia-management-ui -- test -x "$release/sidecar/qintopia-message-sidecar"
    check_closed "$(snapshot)"
    if ! /usr/bin/systemctl enable --now "$unit"; then
      /usr/bin/systemctl disable --now "$unit" || true
      check_closed "$(snapshot)" || true
      exit 75
    fi
    [[ "$(/usr/bin/systemctl show --property=ActiveState --value "$unit")" == active ]]
    ;;
  retire-site)
    check_closed "$(snapshot)"
    check_https_site
    archive="$state/certbot-renewal-archive/qintopia-management-ui.conf"
    [[ ! -e "$archive" && ! -L "$archive" ]] || exit 75
    install -d -m 0700 "$(dirname "$archive")"
    /usr/bin/systemctl is-active --quiet certbot.timer
    backup="$(mktemp /etc/nginx/sites-available/.qintopia-management-ui-retired.XXXXXX)"
    cp -p "$site" "$backup"
    if ! { rm "$enabled" "$site" && /usr/sbin/nginx -t && /usr/bin/systemctl reload nginx &&
      mv "$renewal" "$archive" && chmod 0600 "$archive"; }; then
      if [[ -e "$archive" && ! -e "$renewal" ]]; then mv "$archive" "$renewal"; fi
      mv "$backup" "$site"
      [[ -e "$enabled" || -L "$enabled" ]] || ln -s "$site" "$enabled"
      /usr/sbin/nginx -t
      /usr/bin/systemctl reload nginx
      exit 75
    fi
    rm -f "$backup"
    ;;
esac

foundation_broker_lifecycle() {
  local trusted_release="$release" closing_helper="$release/deploy/runner/foundation-broker-lifecycle.sh"
  if [[ "$staged_closure" == true ]]; then
    # The broker helper independently verifies the same signed staged closure authority.
    "$closing_helper" "$1" || return 75
    return 0
  fi
  python3 - "$trusted_release" "$closing_helper" <<'PY' || return 75
import hashlib
import json
import os
from pathlib import Path
import stat
import sys
root, helper = map(Path, sys.argv[1:])
owner = root.stat().st_uid
if str(root.parent) == '/home/ubuntu/qintopia-agent-os-releases' and owner != 0:
    raise SystemExit('closing release ownership drifted')
for path, mode in ((helper,0o755),(root/'manifest.json',0o444),(root/'deploy-bundle/artifact-manifest.json',0o444)):
    item = path.lstat()
    if not stat.S_ISREG(item.st_mode) or item.st_uid != owner or item.st_nlink != 1 or stat.S_IMODE(item.st_mode)!=mode:
        raise SystemExit('closing helper metadata drifted')
manifest = json.loads((root/'manifest.json').read_text())
artifact = json.loads((root/'deploy-bundle/artifact-manifest.json').read_text())
relative = helper.relative_to(root).as_posix()
entries = [item for item in artifact.get('files',[]) if item.get('path')=='payload/'+relative]
if (manifest.get('release_sha')!=root.name or artifact.get('commit_sha')!=manifest.get('deploy_bundle_sha') or
        len(entries)!=1 or hashlib.sha256(helper.read_bytes()).hexdigest()!=entries[0].get('sha256')):
    raise SystemExit('closing helper identity/digest drifted')
PY
  "$closing_helper" "$1" || return 75
}

case "$mode" in
  quiesce)
    before="$(snapshot)"
    if [[ "$(python3 - "$before" <<'PY'
import json, sys
print(json.loads(sys.argv[1])["LoadState"])
PY
)" == loaded ]]; then
      /usr/bin/systemctl disable --now "$unit"
    fi
    after="$(snapshot)"
    check_closed "$after" "$before"
    foundation_broker_lifecycle quiesce || exit 75
    ;;
  verify-closed)
    check_closed "$(snapshot)"
    foundation_broker_lifecycle verify-closed || exit 75
    ;;
  *) : ;;
esac
