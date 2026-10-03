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
[[ $# -ge 1 ]] || { echo "prepare, consume, finalize or retire-unstarted is required" >&2; exit 2; }
mode="$1"
shift

verify_staged_bundle() {
  [[ -d "$staged/payload/deploy/runner" && ! -L "$staged" ]] || return 1
  (cd "$staged" && sha256sum -c SHA256SUMS >/dev/null) || return 1
  python3 - "$staged" <<'PY'
import hashlib
import json
import os
import re
import stat
import sys
from pathlib import Path

root = Path(sys.argv[1])
with open(root / "artifact-manifest.json", encoding="utf-8") as fh:
    manifest = json.load(fh)
if manifest.get("schema_version") != 1 or manifest.get("target") != "server-operator-files":
    raise SystemExit("staged recovery bundle manifest is invalid")
if not re.fullmatch(r"[0-9a-f]{40}", manifest.get("commit_sha", "")):
    raise SystemExit("staged recovery bundle commit is invalid")
for directory in (root, *root.parents):
    metadata = directory.lstat()
    if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != 0 or metadata.st_mode & 0o022:
        raise SystemExit("staged recovery bundle parent is unsafe")
expected = set()
for item in manifest.get("files", []):
    relative = Path(item["path"])
    if relative.is_absolute() or ".." in relative.parts:
        raise SystemExit("staged recovery bundle path escaped root")
    target = root / relative
    for directory in target.parents:
        if directory == root:
            break
        metadata = directory.lstat()
        if not stat.S_ISDIR(metadata.st_mode) or metadata.st_uid != 0 or metadata.st_mode & 0o022:
            raise SystemExit("staged recovery bundle directory is unsafe")
    metadata = target.lstat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or metadata.st_mode & 0o022:
        raise SystemExit("staged recovery bundle owner, type or mode is invalid")
    digest = hashlib.sha256(target.read_bytes()).hexdigest()
    if digest != item["sha256"] or metadata.st_size != item["size_bytes"]:
        raise SystemExit("staged recovery bundle content mismatch")
    if str(relative) in expected:
        raise SystemExit("staged recovery bundle path is duplicated")
    expected.add(str(relative))
for relative in ("payload/deploy/runner/run-fixed-takeover-request.sh",
                 "payload/deploy/runner/qintopia-agent-os-deploy-runner",
                 "payload/deploy/runner/quiesce-space-automation-runtime.sh",
                 "payload/deploy/runner/wait-deploy-result.sh",
                 "payload/deploy/runner/poll-deploy-requests.sh",
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
  local properties="" load_state="" active_state="" main_pid="" control_pid="" control_group=""
  local allow_not_found="${2:-false}"
  properties="$(systemctl show "$1" --property=LoadState --property=ActiveState \
    --property=MainPID --property=ControlPID --property=ControlGroup 2>/dev/null)" || return 1
  load_state="$(printf '%s\n' "$properties" | sed -n 's/^LoadState=//p')"
  active_state="$(printf '%s\n' "$properties" | sed -n 's/^ActiveState=//p')"
  main_pid="$(printf '%s\n' "$properties" | sed -n 's/^MainPID=//p')"
  control_pid="$(printf '%s\n' "$properties" | sed -n 's/^ControlPID=//p')"
  control_group="$(printf '%s\n' "$properties" | sed -n 's/^ControlGroup=//p')"
  if [[ "$1" == *.service ]]; then
    [[ "$main_pid" == 0 && "$control_pid" == 0 ]] || return 1
  fi
  if [[ "$load_state" == not-found ]]; then
    [[ "$allow_not_found" == true && "$active_state" == inactive ]] || return 1
  else
    [[ "$load_state" == loaded && ( "$active_state" == inactive || "$active_state" == failed ) ]] || return 1
  fi
  if [[ -n "$control_group" ]]; then
    [[ "$control_group" == /* && "$control_group" != *..* ]] || return 1
    [[ -f "/sys/fs/cgroup${control_group}/cgroup.events" ]] || return 1
    [[ "$(sed -n 's/^populated //p' "/sys/fs/cgroup${control_group}/cgroup.events")" == 0 ]] || return 1
  fi
}

timer_file_state() {
  local value=""
  value="$(systemctl show "$timer" --property=UnitFileState --value 2>/dev/null)" || return 1
  [[ "$value" == enabled || "$value" == disabled ]] || return 1
  printf '%s\n' "$value"
}

retire_unstarted() {
  verify_hold || return 75
  exec 8>"${state}/poller.lock"
  flock -n 8 || return 75
  exec 9>"${state}/deploy.lock"
  flock -n 9 || return 75
  unit_stopped "$unit" && unit_stopped "$timer" &&
    unit_stopped qintopia-agent-os-fixed-takeover.service true &&
    unit_stopped qintopia-agent-os-anan-drain-restart.service true || return 75
  # Read only COS evidence. No request is executed, no result is fabricated and
  # no ordinary poller is resumed. Expiry prevents the old request being revived.
  set -a
  source /etc/qintopia/cos-artifacts.env
  set +a
  python3 - "$state" "$release_root" "$request_id" <<'PY'
import hashlib
import hmac
import json
import os
import re
import stat
import sys
import tempfile
import time
import urllib.error
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
from datetime import datetime, timedelta, timezone
from pathlib import Path

state, root = map(Path, sys.argv[1:3])
request_id = sys.argv[3]
recovery = state / "recovery"
old = "16e8d56b98001579c6288ba13199b80d6d3dfc74"
prior = "83d694f2c3bc21fd78a73d25da3197379e2a14d5"
target = "70e7984fab92ddab956009585212d0e9729767b5"
def private_bytes(path, private=True):
    info = path.lstat()
    if (not stat.S_ISREG(info.st_mode) or info.st_uid != os.geteuid() or
            info.st_nlink != 1 or stat.S_IMODE(info.st_mode) not in ((0o600,) if private else (0o600, 0o644))):
        raise SystemExit("unsafe takeover evidence file")
    return path.read_bytes()
record_path = recovery / "takeover.json"
original = private_bytes(record_path)
record = json.loads(original)
if record.get("request_id") != request_id or record.get("phase") not in (None, "preparing", "retired"):
    raise SystemExit("takeover is not an unstarted attempt")
hold_token = record.get("hold_token", "")
if not re.fullmatch(r"[0-9a-f]{32}", hold_token) or private_bytes(recovery / "hold") != (hold_token + "\n").encode():
    raise SystemExit("takeover hold identity mismatch")

def unchanged():
    if private_bytes(record_path) != original or private_bytes(recovery / "hold") != (hold_token + "\n").encode():
        raise SystemExit("takeover state changed")
    for name, sha in (("current", old), ("previous", prior)):
        link = root / name
        if not link.is_symlink() or link.resolve(strict=True) != root / sha:
            raise SystemExit("unstarted takeover pointers changed")
    if (root / target).exists():
        raise SystemExit("takeover release already exists")
    if any((state / "requests/claimed").glob("*.json")):
        raise SystemExit("execution claim exists")
    if any(recovery.glob("deploy-*.json")):
        raise SystemExit("execution journal exists")
    for directory in ("results", "requests/processed", "requests/failed"):
        path = state / directory / (request_id + ".json")
        if path.exists() or path.is_symlink():
            raise SystemExit("request has execution evidence")
    # A detached consumer/uploader must not evade the unit/cgroup checks.
    names = {"poll-deploy-requests.sh", "qintopia-agent-os-deploy-runner", "coscli",
             "run-hermes-core-release.sh", "restart_anan.py"}
    for entry in Path("/proc").iterdir():
        if not entry.name.isdigit() or int(entry.name) == os.getpid():
            continue
        try:
            args = (entry / "cmdline").read_bytes().split(b"\0")
        except FileNotFoundError:
            continue
        if any(os.path.basename(os.fsdecode(arg)) in names for arg in args[:3]):
            raise SystemExit("deployment process remains active")
unchanged()
bucket = os.environ["TENCENT_COS_BUCKET"]
region = os.environ["TENCENT_COS_REGION"]
if not re.fullmatch(r"[A-Za-z0-9-]+", bucket) or not re.fullmatch(r"[a-z0-9-]+", region):
    raise SystemExit("COS bucket or region identity is invalid")
endpoint = os.environ.get("TENCENT_COS_ENDPOINT") or f"https://{bucket}.cos.{region}.myqcloud.com"
if "://" not in endpoint:
    endpoint = "https://" + endpoint
parsed = urllib.parse.urlparse(endpoint)
if (parsed.scheme != "https" and not (parsed.scheme == "http" and
        parsed.hostname in ("127.0.0.1", "localhost"))) or not parsed.netloc or parsed.path not in ("", "/") or parsed.query:
    raise SystemExit("COS endpoint is invalid")

auth_mode = os.environ.get("TENCENT_COS_AUTH_MODE") or "SecretKey"
if auth_mode == "CvmRole":
    role = os.environ.get("TENCENT_COS_CVM_ROLE_NAME", "")
    if not re.fullmatch(r"[A-Za-z0-9_-]{1,128}", role):
        raise SystemExit("COS CVM role identity is invalid")
    metadata_url = "http://metadata.tencentyun.com/latest/meta-data/cam/security-credentials/" + role
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    with opener.open(metadata_url, timeout=5) as response:
        if response.status != 200:
            raise SystemExit("COS CVM role credentials are unavailable")
        credentials = json.loads(response.read(16385))
    secret_id = credentials.get("TmpSecretId", "")
    secret_key = credentials.get("TmpSecretKey", "")
    token = credentials.get("Token", "")
elif auth_mode == "SecretKey":
    secret_id = os.environ.get("TENCENT_COS_SECRET_ID", "")
    secret_key = os.environ.get("TENCENT_COS_SECRET_KEY", "")
    token = os.environ.get("TENCENT_COS_SESSION_TOKEN", "")
else:
    raise SystemExit("COS authentication mode is unsupported")
if not secret_id or not secret_key or (auth_mode == "CvmRole" and not token):
    raise SystemExit("COS read credentials are unavailable")


def cos_get(key, allow_missing=False):
    host = parsed.netloc
    uri = "/" + key
    headers = {"host": host}
    if token:
        headers["x-cos-security-token"] = token
    header_names = sorted(headers)
    header_values = "&".join(f"{name}={urllib.parse.quote(headers[name], safe='~-._')}" for name in header_names)
    start_time = int(time.time())
    key_time = f"{start_time};{start_time + 300}"
    http_string = f"get\n{uri}\n\n{header_values}\n"
    string_to_sign = f"sha1\n{key_time}\n{hashlib.sha1(http_string.encode()).hexdigest()}\n"
    sign_key = hmac.new(secret_key.encode(), key_time.encode(), hashlib.sha1).hexdigest()
    signature = hmac.new(sign_key.encode(), string_to_sign.encode(), hashlib.sha1).hexdigest()
    authorization = (f"q-sign-algorithm=sha1&q-ak={secret_id}&q-sign-time={key_time}&q-key-time={key_time}"
                     f"&q-header-list={';'.join(header_names)}&q-url-param-list=&q-signature={signature}")
    url = endpoint.rstrip("/") + uri
    request = urllib.request.Request(url, method="GET", headers={**headers, "Authorization": authorization})
    opener = urllib.request.build_opener(urllib.request.ProxyHandler({}))
    try:
        with opener.open(request, timeout=15) as response:
            if response.status != 200:
                raise SystemExit("COS result read did not return HTTP 200")
            payload = response.read(1048577)
            if len(payload) > 1048576:
                raise SystemExit("COS result exceeds the fixed size limit")
            return payload
    except urllib.error.HTTPError as error:
        body = error.read(8193)
        if error.code != 404 or len(body) > 8192:
            raise SystemExit("COS result object request failed") from None
        try:
            root = ET.fromstring(body)
            fields = {child.tag.rsplit("}", 1)[-1]: child.text for child in root}
        except ET.ParseError:
            raise SystemExit("COS error response is invalid") from None
        if (root.tag.rsplit("}", 1)[-1] != "Error" or fields.get("Code") != "NoSuchKey" or
                not ("Resource" in fields or "Key" in fields) or
                ("Resource" in fields and fields["Resource"] != "/" + key) or
                ("Key" in fields and fields["Key"] != key)):
            raise SystemExit("COS result did not return fixed-key NoSuchKey")
        if not allow_missing:
            raise SystemExit("required COS evidence is absent")
        return None

prefix = "qintopia-agent-os"
request_key = f"{prefix}/deploy-requests/production/requests/{request_id}.json"
request_bytes = cos_get(request_key)
request = json.loads(request_bytes)
signature = dict(request.get("signature", {}))
actual = signature.pop("value", "")
if (signature.get("algorithm"), signature.get("issuer"), signature.get("key_id")) != (
        "hmac-sha256", "github-actions", os.environ["DEPLOY_REQUEST_SIGNING_KEY_ID"]):
    raise SystemExit("retired request signature identity mismatch")
unsigned = dict(request)
unsigned.pop("signature", None)
canonical = json.dumps({"request": unsigned, "signature": signature}, sort_keys=True,
                       ensure_ascii=False, separators=(",", ":"))
expected = hmac.new(os.environ["DEPLOY_REQUEST_SIGNING_KEY"].encode(), canonical.encode(), hashlib.sha256).hexdigest()
if not hmac.compare_digest(actual, expected):
    raise SystemExit("retired request signature mismatch")
created, expires, signed = [datetime.fromisoformat(v.replace("Z", "+00:00")) for v in
                            (request["created_at"], request["expires_at"], signature["signed_at"])]
if (any(v.tzinfo is None for v in (created, expires, signed)) or expires <= created or
        expires - created > timedelta(minutes=60) or abs(signed - created) > timedelta(minutes=5) or
        datetime.now(timezone.utc) <= expires + timedelta(minutes=5)):
    raise SystemExit("request has not safely expired or original signing time is invalid")
if (request.get("schema_version") != 1 or request.get("request_id") != request_id or
        request.get("environment") != "production" or
        request.get("repository") != "qintopia-agent-studio/qintopia-agent-os" or
        request.get("commit_sha") != old or request.get("runtime_sha") != prior or
        request.get("release_sha") != target or request.get("release_scope") != ["deploy-bundle"] or
        request.get("restart_targets") != ["qintopia-system-services"] or request.get("dry_run") is not False):
    raise SystemExit("retired request is not the fixed takeover")
result_key = f"{prefix}/deploy-results/production/{request_id}.json"
if request.get("cos", {}).get("request_key") != request_key or request.get("cos", {}).get("result_key") != result_key:
    raise SystemExit("retired request COS identity mismatch")
if cos_get(result_key, allow_missing=True) is not None:
    raise SystemExit("remote result exists; use result reconciliation")
pending = state / "requests/pending" / (request_id + ".json")
pending_digest = None
if pending.exists() or pending.is_symlink():
    # A failed download can leave incomplete bytes. No claim means these bytes
    # were never handed to a runner; preserve their digest instead of deleting them.
    pending_digest = hashlib.sha256(private_bytes(pending, private=False)).hexdigest()
marker = recovery / "takeover-consumed"
marker_bytes = (request_id + "\n").encode()
if marker.exists() or marker.is_symlink():
    if record.get("phase") not in (None, "retired") or private_bytes(marker) != marker_bytes:
        raise SystemExit("claimed attempt cannot be retired")
    # Legacy v0.3.5 wrote this marker BEFORE downloading the request. New code
    # creates it AFTER the claim; new-format marked attempts are never retired.
unchanged()
digest = hashlib.sha256(request_bytes).hexdigest()
retired = record.setdefault("retired_requests", {})
if request_id in retired and retired[request_id]["request_sha256"] != digest:
    raise SystemExit("retired request identity changed")
retired.setdefault(request_id, {"request_sha256": digest, "reason": "expired_before_claim",
                               "pending_sha256": pending_digest,
                               "recorded_at": datetime.now(timezone.utc).isoformat()})
record["phase"] = "retired"
fd, tmp = tempfile.mkstemp(prefix=".retire-", dir=recovery)
try:
    with os.fdopen(fd, "w", encoding="utf-8") as fh:
        json.dump(record, fh, sort_keys=True)
        fh.write("\n")
        fh.flush()
        os.fsync(fh.fileno())
    os.replace(tmp, record_path)
    directory = os.open(recovery, os.O_RDONLY | os.O_DIRECTORY)
    try:
        os.fsync(directory)
        if marker.exists():
            archive = recovery / ("retired-" + request_id + ".consumed")
            # Rename is atomic; never discard the legacy marker or overwrite an archive.
            if archive.exists() or archive.is_symlink():
                raise SystemExit("retired marker archive already exists")
            os.rename(marker, archive)
            os.fsync(directory)
    finally:
        os.close(directory)
finally:
    if os.path.exists(tmp):
        os.unlink(tmp)
print("unstarted request retired; hold retained; a new signed request is required")
PY
}

# Read-only verification is shared by the fixed poller and staged recovery helper.
# Do not acquire lifecycle locks here: callers already own their required locks.
if [[ "$mode" == verify-staged || "$mode" == verify-staged-closure ]]; then
  [[ $# -eq 0 ]] || exit 2
  verify_staged_bundle
  if [[ "$mode" == verify-staged-closure ]]; then
    # A helper may close services before the candidate release exists, but may
    # never gain maintenance/activation authority from its staged location.
    source /etc/qintopia/cos-artifacts.env
    export DEPLOY_REQUEST_SIGNING_KEY DEPLOY_REQUEST_SIGNING_KEY_ID
    python3 - "$state" "$staged" <<'PYCLOSURE'
import hashlib
import hmac
import json
import os
import re
import stat
import subprocess
import sys
from datetime import datetime, timedelta
from pathlib import Path

state, staged = map(Path, sys.argv[1:3])
def read(path):
    for parent in (path.parent, *path.parent.parents):
        info = parent.lstat()
        if not stat.S_ISDIR(info.st_mode) or info.st_uid != 0 or info.st_mode & 0o022:
            raise SystemExit("staged closure evidence parent is unsafe")
    info = path.lstat()
    if not stat.S_ISREG(info.st_mode) or info.st_uid != 0 or info.st_nlink != 1 or info.st_mode & 0o022:
        raise SystemExit("staged closure evidence metadata is invalid")
    return path.read_bytes()

artifact = json.loads(read(staged / "artifact-manifest.json"))
listed = {f["path"] for f in artifact["files"]}
for relative in ("payload/deploy/runner/management-ui-lifecycle.sh",
                 "payload/runtime/nginx/templates/management-ui-http.conf.template",
                 "payload/runtime/nginx/templates/management-ui-https.conf.template"):
    if relative not in listed:
        raise SystemExit("staged closure helper or template is not in the reviewed bundle")
record = json.loads(read(state / "recovery/takeover.json"))
rid = record.get("request_id", "")
token = record.get("hold_token", "")
if (not re.fullmatch(r"deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}", rid) or
        not re.fullmatch(r"[0-9a-f]{32}", token) or
        read(state / "recovery/hold") != (token + "\n").encode()):
    raise SystemExit("staged closure request-bound hold is invalid")
paths = [state / "requests" / kind / (rid + ".json") for kind in ("pending", "processed", "failed")]
paths = [p for p in paths if p.exists() or p.is_symlink()]
if len(paths) != 1:
    raise SystemExit("staged closure request archive is ambiguous")
raw = read(paths[0])
request = json.loads(raw)
signature = request.pop("signature", {})
value = signature.pop("value", "")
def canonical(value):
    if isinstance(value, list):
        return "[" + ",".join(canonical(item) for item in value) + "]"
    if isinstance(value, dict):
        return "{" + ",".join(json.dumps(k, separators=(",", ":")) + ":" + canonical(value[k])
                             for k in sorted(value)) + "}"
    return json.dumps(value, ensure_ascii=False, separators=(",", ":"))
payload = canonical({"request": request, "signature": signature}).encode()
key = os.environ.get("DEPLOY_REQUEST_SIGNING_KEY", "")
created = datetime.fromisoformat(request["created_at"].replace("Z", "+00:00"))
expires = datetime.fromisoformat(request["expires_at"].replace("Z", "+00:00"))
signed = datetime.fromisoformat(signature["signed_at"].replace("Z", "+00:00"))
if (created.tzinfo is None or expires.tzinfo is None or signed.tzinfo is None or
        expires <= created or expires - created > timedelta(minutes=60) or
        abs(signed - created) > timedelta(minutes=5)):
    raise SystemExit("staged closure signing time is invalid")
if (not key or signature.get("algorithm") != "hmac-sha256" or
        signature.get("issuer") != "github-actions" or
        signature.get("key_id") != os.environ.get("DEPLOY_REQUEST_SIGNING_KEY_ID") or
        not isinstance(value, str) or not hmac.compare_digest(
            hmac.new(key.encode(), payload, hashlib.sha256).hexdigest(), value)):
    raise SystemExit("staged closure request signature is invalid")
journal = json.loads(read(state / "recovery" / (rid + ".json")))
execution = journal.get("execution", {})
runner = staged / "payload/deploy/runner/qintopia-agent-os-deploy-runner"
if (request.get("request_id") != rid or request.get("environment") != "production" or
        request.get("repository") != "qintopia-agent-studio/qintopia-agent-os" or
        request.get("dry_run") is not False or
        request.get("release_scope") != ["deploy-bundle"] or
        request.get("restart_targets") != ["qintopia-system-services"] or
        request.get("commit_sha") != journal.get("original_current_sha") or
        request.get("runtime_sha") != journal.get("original_previous_sha") or
        request.get("release_sha") in (request.get("commit_sha"), request.get("runtime_sha")) or
        journal.get("request_id") != rid or journal.get("direction") != "O→T" or
        journal.get("request_sha256") != hashlib.sha256(raw).hexdigest() or
        journal.get("hold_token") != token or
        execution.get("unit") != "qintopia-agent-os-fixed-takeover.service" or
        execution.get("unit_invocation_verified") is not True or
        not re.fullmatch(r"[0-9a-f]{32}", execution.get("invocation_id", ""))):
    raise SystemExit("staged closure transaction binding is invalid")
claims = list((state / "requests/claimed").glob("*.json"))
if claims:
    if len(claims) != 1 or claims[0].stem != rid:
        raise SystemExit("staged closure has another consumer")
    claim = json.loads(read(claims[0]))
    artifact = json.loads(read(staged / "artifact-manifest.json"))
    if (record.get("phase") != "preparing" or
            rid in record.get("retired_requests", {}) or
            request.get("deploy_bundle_sha") != artifact.get("commit_sha") or
            execution.get("path") != str(runner) or
            execution.get("sha256") != hashlib.sha256(read(runner)).hexdigest() or
            claim.get("request_id") != rid or claim.get("phase") != "possibly_executing" or
            claim.get("recovery_eligible") is not True or
            claim.get("result_upload", {}).get("phase") != "not_started" or
            claim.get("request_sha256") != journal["request_sha256"] or
            claim.get("execution") != execution or claim.get("hold_token") != token):
        raise SystemExit("staged closure active consumer identity changed")
    shown = subprocess.run(["systemctl", "show", execution["unit"],
                            "--property=InvocationID", "--value"],
                           capture_output=True, text=True, check=True, timeout=5)
    if shown.stdout.strip() != execution["invocation_id"]:
        raise SystemExit("staged closure systemd invocation changed")
else:
    # Recovery may use a newer reviewed complete bundle to close the older
    # definite pre-promotion failure. It cannot use this branch to stop services.
    result = json.loads(read(state / "results" / (rid + ".json")))
    checks = [c for c in result.get("checks", []) if c.get("name") == "deploy-runner"]
    detail = json.loads(checks[0].get("detail", "{}")) if len(checks) == 1 else {}
    if (paths[0].parent.name != "failed" or result.get("status") != "failed" or
            result.get("request_id") != rid or journal.get("phase") != "intent" or
            detail.get("failure_stage") not in ("quiesce-space-automation-runtime", "quiesce-management-ui") or
            detail.get("promoted_current") is not False or
            detail.get("profile_activation_attempted") is not False or
            result.get("rollback", {}).get("attempted") is not False or
            journal.get("result_upload", {}).get("payload_sha256") !=
                hashlib.sha256(read(state / "results" / (rid + ".json"))).hexdigest()):
        raise SystemExit("staged closure has no definite pre-promotion failure")
    print("verify-only")
PYCLOSURE
  fi
  exit 0
fi

# This lock serialises prepare/bind/retire/finalize, including the wait for the child.
# The child alone owns poller.lock -> deploy.lock until it exits.
mkdir -p -m 0700 "$recovery"
exec 6>"${recovery}/takeover.lock"
flock -n 6 || { echo "another takeover lifecycle operation is running" >&2; exit 75; }

case "$mode" in
  retire-unstarted)
    [[ $# -eq 1 && "$1" =~ ^deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}$ ]] || exit 2
    request_id="$1"
    verify_staged_bundle
    retire_unstarted || exit 75
    ;;
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
    json.dump({"timer_was_enabled": enabled == "true", "hold_token": token, "phase": "prepared"}, fh)
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
    # Finalization must verify receipts without relying on the operator shell
    # having exported the production signing environment.
    source /etc/qintopia/cos-artifacts.env
    export DEPLOY_REQUEST_SIGNING_KEY DEPLOY_REQUEST_SIGNING_KEY_ID
    old_sha=16e8d56b98001579c6288ba13199b80d6d3dfc74
    if [[ "$mode" == consume ]]; then
      verify_hold
      [[ ! -e "${recovery}/takeover-consumed" ]] || { echo "takeover already claimed" >&2; exit 75; }
      unit_stopped qintopia-agent-os-fixed-takeover.service true || exit 75
      fixed_state="$(systemctl show qintopia-agent-os-fixed-takeover.service --property=ActiveState --value)" || exit 75
      if [[ "$fixed_state" == failed ]]; then
        systemctl reset-failed qintopia-agent-os-fixed-takeover.service || exit 75
      fi
      [[ "$(readlink -f "$release_root/current")" == "$release_root/$old_sha" &&
        "$(readlink -f "$release_root/previous")" == "$release_root/83d694f2c3bc21fd78a73d25da3197379e2a14d5" &&
        -x "$staged/payload/deploy/runner/qintopia-agent-os-deploy-runner" ]] || {
        echo "fixed takeover lineage or staged runner mismatch" >&2
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
            (record.get("request_id") is not None and record.get("phase") != "retired") or
            sys.argv[2] in record.get("retired_requests", {})):
        raise SystemExit("takeover request binding is invalid")
    with open(os.path.join(sys.argv[1], "hold"), encoding="ascii") as fh:
        if fh.read() != token + "\n":
            raise SystemExit("takeover hold identity changed")
    record["request_id"] = sys.argv[2]
    record["phase"] = "preparing"
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
      --property="Environment=QINTOPIA_DEPLOY_RUNNER_BIN=${staged}/payload/deploy/runner/qintopia-agent-os-deploy-runner" \
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
  *) echo "prepare, consume, finalize or retire-unstarted is required" >&2; exit 2 ;;
esac
