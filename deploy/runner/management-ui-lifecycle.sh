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

[[ $# -eq 1 ]] || exit 2
mode="$1"
case "$mode" in
  prepare|install-http|issue-cert|install-https|activate|quiesce|verify-closed|retire-site) ;;
  *) exit 2 ;;
esac
[[ "$(id -u)" -eq 0 ]] || { echo "management UI lifecycle requires root" >&2; exit 75; }
script_path="$(readlink -f -- "${BASH_SOURCE[0]}")"
[[ "$script_path" =~ ^/home/ubuntu/qintopia-agent-os-releases/([0-9a-f]{40})/deploy/runner/management-ui-lifecycle\.sh$ ]] || {
  echo "management UI lifecycle requires a fixed immutable release" >&2
  exit 75
}
release_sha="${BASH_REMATCH[1]}"
release="${release_root}/${release_sha}"

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
      [[ "$(curl --noproxy '*' --silent --show-error --output /dev/null --write-out '%{http_code}' \
        --resolve agentos.qintopia.cn:443:127.0.0.1 --max-time 5 \
        https://agentos.qintopia.cn/)" == 503 ]] || {
        echo "management UI HTTPS route did not return 503" >&2
        return 75
      }
    elif cmp -s "$site" "$release/runtime/nginx/templates/management-ui-http.conf.template"; then
      [[ "$(curl --noproxy '*' --silent --show-error --output /dev/null --write-out '%{http_code}' \
        --resolve agentos.qintopia.cn:80:127.0.0.1 --max-time 5 \
        http://agentos.qintopia.cn/)" == 404 ]] || {
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

case "$mode" in
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
    ;;
  verify-closed)
    check_closed "$(snapshot)"
    ;;
  *) : ;;
esac
