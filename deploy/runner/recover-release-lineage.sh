#!/usr/bin/env bash
set -euo pipefail
umask 077

state=/var/lib/qintopia-agent-os-deploy
release_root=/home/ubuntu/qintopia-agent-os-releases
unit=qintopia-agent-os-deploy-runner.service
timer=qintopia-agent-os-deploy-runner.timer
anan_unit=qintopia-agent-os-anan-drain-restart.service
fixed_unit=qintopia-agent-os-fixed-takeover.service
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
export TENCENT_COS_AUTH_MODE TENCENT_COS_SECRET_ID TENCENT_COS_SECRET_KEY TENCENT_COS_SESSION_TOKEN
export TENCENT_COS_CVM_ROLE_NAME TENCENT_COS_ENDPOINT
journal="${state}/recovery/${request_id}.json"
hold="${state}/recovery/hold"
dropin=/etc/systemd/system/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf
[[ -f "$journal" && ! -L "$journal" && ! -L "$hold" && ! -L "$dropin" ]] || {
  echo "recovery journal or isolation path is invalid" >&2
  exit 75
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

systemctl disable --now "$timer"
for ((i=0; i<120; i++)); do
  if unit_stopped "$unit"; then break; fi
  sleep 1
done
unit_stopped "$unit" || { echo "ordinary poller active or state unknown" >&2; exit 75; }
systemctl daemon-reload
if [[ -f "$dropin" ]]; then
  cmp -s "$dropin" "$verified_release/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf" || exit 75
fi
unit_stopped "$timer" || exit 75
unit_stopped "$anan_unit" true || {
  echo "Anan drain helper is still active or unknown" >&2
  exit 75
}

exec 8>"${state}/poller.lock"
flock -n 8 || { echo "poller lock is held" >&2; exit 75; }
exec 9>"${state}/deploy.lock"
flock -n 9 || { echo "deploy lock is held" >&2; exit 75; }
unit_stopped "$unit" || { echo "ordinary poller restarted during recovery" >&2; exit 75; }
unit_stopped "$fixed_unit" true || { echo "fixed takeover consumer remains active" >&2; exit 75; }
unit_stopped "$anan_unit" true || { echo "Anan helper remains active" >&2; exit 75; }

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

claim_file="${state}/requests/claimed/${request_id}.json"
claim_state="$(python3 - "$journal" "$claim_file" "$request_file" "$release_root" \
  "$direction" "$original_current" "$request_id" "$hold" "${state}/recovery/takeover.json" <<'PY'
import hashlib
import json
import os
import re
import stat
import sys
from pathlib import Path

journal_path, claim_path, request_path, release_root, direction, current, request_id, hold_path, takeover_path = sys.argv[1:10]
with open(journal_path, encoding="utf-8") as fh:
    journal = json.load(fh)
with open(request_path, "rb") as fh:
    request_digest = hashlib.sha256(fh.read()).hexdigest()
hold = Path(hold_path)
for other in Path(claim_path).parent.glob("*.json"):
    if other.name != Path(claim_path).name:
        raise SystemExit("another deploy request claim is present")
for other in Path(journal_path).parent.glob("deploy-*.json"):
    if other.name > Path(journal_path).name:
        raise SystemExit("a later recovery journal owns the isolation state")
if not os.path.isfile(claim_path):
    token = journal.get("hold_token", "")
    if direction == "O→T":
        with open(takeover_path, encoding="utf-8") as fh:
            takeover = json.load(fh)
        if (not isinstance(token, str) or not re.fullmatch(r"[0-9a-f]{32}", token) or
                takeover.get("request_id") != request_id or takeover.get("hold_token") != token):
            raise SystemExit("legacy takeover hold identity conflicts")
    elif token != request_id:
        raise SystemExit("legacy recovery hold request identity conflicts")
    if not hold.is_file() or hold.is_symlink():
        raise SystemExit("legacy recovery has no authenticated hold")
    metadata = hold.lstat()
    if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or
            stat.S_IMODE(metadata.st_mode) != 0o600 or
            hold.read_text(encoding="ascii") != token + "\n"):
        raise SystemExit("legacy recovery hold belongs to another transaction")
    print("legacy\t" + token + "\t")
    raise SystemExit(0)
with open(claim_path, encoding="utf-8") as fh:
    claim = json.load(fh)
if (claim.get("request_id") != request_id or claim.get("request_sha256") != request_digest or
        claim.get("phase") != "possibly_executing" or claim.get("recovery_eligible") is not True or
        journal.get("request_sha256") != request_digest or
        journal.get("execution") != claim.get("execution") or
        journal.get("result_upload") != claim.get("result_upload") or
        journal.get("hold_token") != claim.get("hold_token")):
    raise SystemExit("recovery claim, journal or upload stage conflicts")
execution = claim.get("execution")
if not isinstance(execution, dict):
    raise SystemExit("recovery execution identity is absent")
expected_path = Path(release_root) / current / "deploy/runner/qintopia-agent-os-deploy-runner"
actual_path = Path(execution.get("path", ""))
if actual_path != expected_path or not expected_path.is_file() or expected_path.is_symlink():
    raise SystemExit("recovery execution release path is invalid")
metadata = expected_path.stat()
if metadata.st_uid != 0 or stat.S_IMODE(metadata.st_mode) != 0o755:
    raise SystemExit("recovery execution file owner or mode drifted")
if hashlib.sha256(expected_path.read_bytes()).hexdigest() != execution.get("sha256"):
    raise SystemExit("recovery execution file digest drifted")
expected_unit = ("qintopia-agent-os-fixed-takeover.service" if direction == "O→T" else
                 "qintopia-agent-os-deploy-runner.service")
if execution.get("unit") != expected_unit:
    raise SystemExit("recovery execution unit differs from direction")
invocation = execution.get("invocation_id", "")
if not isinstance(invocation, str) or (invocation and not re.fullmatch(r"[0-9a-f]{32}", invocation)):
    raise SystemExit("recovery invocation identity is invalid")
if invocation and execution.get("unit_invocation_verified") is not True:
    raise SystemExit("recovery unit start evidence is absent")
token = claim.get("hold_token", "")
if direction == "O→T":
    with open(takeover_path, encoding="utf-8") as fh:
        takeover = json.load(fh)
    if (takeover.get("request_id") != request_id or takeover.get("hold_token") != token or
            not isinstance(token, str) or not re.fullmatch(r"[0-9a-f]{32}", token)):
        raise SystemExit("first takeover hold identity conflicts")
elif token != request_id:
    raise SystemExit("recovery hold request identity conflicts")
if hold.exists() or hold.is_symlink():
    metadata = hold.lstat()
    if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or
            stat.S_IMODE(metadata.st_mode) != 0o600 or
            hold.read_text(encoding="ascii") != token + "\n"):
        raise SystemExit("current recovery hold belongs to another transaction")
print("\t".join((claim["result_upload"].get("phase", ""), token, invocation)))
PY
)" || exit 75
IFS=$'\t' read -r upload_phase hold_token invocation_id <<<"$claim_state"

if [[ ! -f "$hold" ]]; then
  [[ "$direction" != "O→T" && -n "$hold_token" ]] || exit 75
  python3 - "$state/recovery" "$hold_token" <<'PY' || exit 75
import os
import sys
directory = os.open(sys.argv[1], os.O_RDONLY | os.O_DIRECTORY | os.O_NOFOLLOW)
try:
    descriptor = os.open("hold", os.O_WRONLY | os.O_CREAT | os.O_EXCL | os.O_NOFOLLOW,
                         0o600, dir_fd=directory)
    try:
        os.write(descriptor, (sys.argv[2] + "\n").encode("ascii"))
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
    os.fsync(directory)
finally:
    os.close(directory)
PY
fi
mkdir -p "$(dirname "$dropin")"
install -m 0644 "$verified_release/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf" "$dropin"
systemctl daemon-reload
cmp -s "$dropin" "$verified_release/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf" || exit 75
unit_stopped "$timer" && unit_stopped "$unit" || exit 75

result_file="${state}/results/${request_id}.json"
remote_dir="$(mktemp -d)" || exit 75
chmod 0700 "$remote_dir"
trap 'rm -rf -- "$remote_dir"' EXIT
remote_result="${remote_dir}/result.json"
remote_state_file="${remote_dir}/result.state"

fetch_remote_evidence() {
  local coscli="${COSCLI_PATH:-}" config="" alias="${TENCENT_COS_BUCKET_ALIAS:-qintopia-agent-os-artifacts}"
  [[ -n "${TENCENT_COS_BUCKET:-}" && -n "${TENCENT_COS_REGION:-}" ]] || return 1
  if [[ -z "$coscli" ]]; then
    coscli="$(command -v coscli)" || return 1
  fi
  [[ -x "$coscli" ]] || return 1
  config="$remote_dir/cos.yaml"
  touch "$config" || return 1
  local status=0
  (
    cd "$remote_dir" || exit 1
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
      "$remote_dir/request.json" -c "$config" --disable-log >/dev/null 2>&1 || exit 1
    cmp -s "$request_file" "$remote_dir/request.json" || exit 1
  ) || status=$?
  return "$status"
}

fetch_remote_evidence || {
  echo "COS original request is absent, unreadable, or conflicts with local signed evidence" >&2
  exit 75
}

if ! python3 - "$request_id" "$remote_result" "$remote_state_file" <<'PY'
import hashlib
import hmac
import json
import os
import re
import sys
import time
import urllib.error
import urllib.parse
import urllib.request
import xml.etree.ElementTree as ET
from pathlib import Path

request_id, result_path, state_path = sys.argv[1:4]
key = f"qintopia-agent-os/deploy-results/production/{request_id}.json"
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
        Path(result_path).write_bytes(payload)
        os.chmod(result_path, 0o600)
        state = "present"
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
    state = "absent"
Path(state_path).write_text(state + "\n", encoding="ascii")
PY
then
  echo "COS result state is unreadable or unknown" >&2
  exit 75
fi
remote_state="$(cat "$remote_state_file")"

result_status() {
  "${verified_release}/deploy/runner/wait-deploy-result.sh" --request-file "$request_file" \
    --result-file "$1" --verify-archived-request >/dev/null || return 1
  python3 - "$1" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    print(json.load(fh).get("status", ""))
PY
}

local_status=missing
if [[ -e "$result_file" || -L "$result_file" ]]; then
  [[ -f "$result_file" && ! -L "$result_file" ]] || exit 75
  local_status="$(result_status "$result_file")" || exit 75
  [[ "$local_status" == succeeded || "$local_status" == failed || "$local_status" == rolled_back ]] || exit 75
fi
remote_status=missing
if [[ "$remote_state" == present ]]; then
  remote_status="$(result_status "$remote_result")" || exit 75
  [[ "$remote_status" == succeeded || "$remote_status" == failed || "$remote_status" == rolled_back ]] || exit 75
  if [[ "$local_status" != missing ]]; then
    cmp -s "$result_file" "$remote_result" || { echo "local and COS signed results conflict" >&2; exit 75; }
  fi
elif [[ "$remote_state" != absent ]]; then
  exit 75
fi

if [[ "$remote_state" == absent ]]; then
  [[ "$local_status" != succeeded ]] || {
    echo "local signed success exists; missing COS upload cannot authorize reversal" >&2
    exit 75
  }
  [[ "$upload_phase" == not_started && -n "$invocation_id" ]] || {
    echo "result upload may have started or execution identity is unknown" >&2
    exit 75
  }
elif [[ "$remote_status" == succeeded ]]; then
  :
elif [[ "$local_status" != "$remote_status" ||
        ( "$remote_status" != failed && "$remote_status" != rolled_back ) ]]; then
  echo "signed remote failure lacks matching local evidence" >&2
  exit 75
fi

verify_execution_ended() {
  [[ -n "$invocation_id" ]] || return 1
  local consumer=""
  if [[ "$direction" == "O→T" ]]; then
    consumer="$fixed_unit"
    [[ -f "${state}/recovery/takeover-consumed" &&
      "$(cat "${state}/recovery/takeover-consumed")" == "$request_id" ]] || return 1
  else
    consumer="$unit"
  fi
  local properties="" load_state="" shown_invocation=""
  properties="$(systemctl show "$consumer" --property=LoadState --property=InvocationID 2>/dev/null)" || return 1
  load_state="$(printf '%s\n' "$properties" | sed -n 's/^LoadState=//p')"
  shown_invocation="$(printf '%s\n' "$properties" | sed -n 's/^InvocationID=//p')"
  [[ -z "$shown_invocation" || "$shown_invocation" == "$invocation_id" ]] || return 1
  if [[ "$load_state" == not-found ]]; then
    [[ "$direction" == "O→T" ]] || return 1
  fi
  unit_stopped "$consumer" "$([[ "$direction" == 'O→T' ]] && echo true || echo false)" || return 1
  unit_stopped "$unit" && unit_stopped "$fixed_unit" true && unit_stopped "$anan_unit" true
}

if [[ "$remote_state" == absent ]]; then
  verify_execution_ended || { echo "original consumer or uploader end is unproven" >&2; exit 75; }
fi

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

python3 - "$journal" "$request_file" "$result_file" "$remote_result" \
  "$remote_state" "$local_status" "$remote_status" "$upload_phase" "$invocation_id" <<'PY' || exit 75
import hashlib
import json
import os
import sys
import tempfile
from datetime import datetime, timezone
from pathlib import Path

path, request_path, local_path, remote_path, remote_state, local_status, remote_status, upload_phase, invocation = sys.argv[1:10]
with open(path, encoding="utf-8") as fh:
    record = json.load(fh)
request_digest = hashlib.sha256(Path(request_path).read_bytes()).hexdigest()
if record.get("request_sha256") != request_digest:
    raise SystemExit("maintenance request identity changed")
evidence = {
    "request_sha256": request_digest,
    "remote_state": remote_state,
    "remote_status": remote_status,
    "remote_sha256": hashlib.sha256(Path(remote_path).read_bytes()).hexdigest() if remote_state == "present" else None,
    "local_status": local_status,
    "local_sha256": hashlib.sha256(Path(local_path).read_bytes()).hexdigest() if local_status != "missing" else None,
    "upload_phase": upload_phase,
    "consumer_invocation_id": invocation,
}
prior = record.get("maintenance_evidence")
if prior is not None:
    stable_keys = ("request_sha256", "remote_state", "remote_status", "remote_sha256",
                   "upload_phase", "consumer_invocation_id")
    if any(prior.get(key) != evidence[key] for key in stable_keys):
        raise SystemExit("maintenance evidence changed between attempts")
    if (prior.get("local_status") != evidence["local_status"] or
            prior.get("local_sha256") != evidence["local_sha256"]):
        if (remote_status != "succeeded" or prior.get("local_status") != "missing" or
                evidence["local_status"] != "succeeded" or
                evidence["local_sha256"] != evidence["remote_sha256"]):
            raise SystemExit("maintenance local result changed between attempts")
else:
    evidence["recorded_at"] = datetime.now(timezone.utc).isoformat()
    record["maintenance_evidence"] = evidence
    fd, temporary = tempfile.mkstemp(prefix=".maintenance-", dir=os.path.dirname(path))
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

if [[ "$remote_status" == succeeded ]]; then
  python3 - "$release_root" "$request_release" "$original_current" "$original_previous" \
    "$direction" "$request_file" "$remote_result" "$result_file" "$journal" "$claim_file" \
    "$verified_release" <<'PY' || exit 75
import hashlib
import json
import os
import re
import stat
import subprocess
import sys
import tempfile
from pathlib import Path

release_root, target_sha, old_current, old_previous, direction, request_path, remote_path, local_path, journal_path, claim_path, verified_release = sys.argv[1:12]
root = Path(release_root).resolve(strict=True)
with open(request_path, encoding="utf-8") as fh:
    request = json.load(fh)
with open(remote_path, encoding="utf-8") as fh:
    result = json.load(fh)
with open(journal_path, encoding="utf-8") as fh:
    journal = json.load(fh)
target = root / target_sha
with (target / "manifest.json").open(encoding="utf-8") as fh:
    manifest = json.load(fh)
expected_previous = old_current if direction != "R→T" else manifest.get("previous_sha")
for name, sha in (("current", target_sha), ("previous", expected_previous)):
    link = root / name
    if not link.is_symlink() or link.resolve(strict=True) != root / sha:
        raise SystemExit("signed success pointer state conflicts")
if (manifest.get("release_sha") != target_sha or manifest.get("previous_sha") != expected_previous or
        result.get("current_target") != str(target) or result.get("previous_sha") != expected_previous or
        request.get("release_sha") != target_sha or result.get("request_id") != request.get("request_id") or
        journal.get("request_id") != request.get("request_id")):
    raise SystemExit("signed success release identity conflicts")
for key in ("commit_sha", "runtime_sha", "deploy_bundle_sha", "runtime_artifact_profile"):
    if manifest.get(key) != request.get(key) or result.get(key) != request.get(key):
        raise SystemExit(f"signed success {key} identity conflicts")
for key in ("release_scope", "restart_targets"):
    if result.get(key) != request.get(key) or (direction != "R→T" and manifest.get(key) != request.get(key)):
        raise SystemExit(f"signed success {key} action conflicts")
if direction == "R→T":
    original_id = manifest.get("request_id", "")
    if (not isinstance(original_id, str) or
            not re.fullmatch(r"deploy-[0-9]{8}T[0-9]{6}Z-[0-9a-f]{7,40}", original_id) or
            original_id == request.get("request_id")):
        raise SystemExit("reverse target original request identity is invalid")
    original_request_path = Path(request_path).parent.parent / "processed" / (original_id + ".json")
    original_result_path = Path(local_path).parent / (original_id + ".json")
    for original_path in (original_request_path, original_result_path):
        if not original_path.is_file() or original_path.is_symlink():
            raise SystemExit("reverse target original signed evidence is unavailable")
    subprocess.run([str(Path(verified_release) / "deploy/runner/wait-deploy-result.sh"),
                    "--request-file", str(original_request_path), "--result-file",
                    str(original_result_path), "--verify-archived-request"],
                   check=True, stdout=subprocess.DEVNULL)
    original_request = json.loads(original_request_path.read_bytes())
    original_result = json.loads(original_result_path.read_bytes())
    for key in ("release_sha", "commit_sha", "runtime_sha", "deploy_bundle_sha",
                "runtime_artifact_profile", "release_scope", "restart_targets"):
        if original_request.get(key) != manifest.get(key) or original_result.get(key) != manifest.get(key):
            raise SystemExit(f"reverse target original {key} evidence conflicts")
    if (manifest.get("dry_run") is not False or original_request.get("dry_run") is not False or
            original_request.get("request_id") != original_id or
            original_result.get("request_id") != original_id or
            original_result.get("status") != "succeeded" or
            original_result.get("previous_sha") != expected_previous or
            original_result.get("current_target") != str(target)):
        raise SystemExit("reverse target original success evidence conflicts")
for relative, mode in (("manifest.json", 0o444),
                       ("deploy/runner/install-release-systemd-units.sh", 0o755),
                       ("deploy/runner/smoke-release.sh", 0o755)):
    path = target / relative
    metadata = path.lstat()
    if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or stat.S_IMODE(metadata.st_mode) != mode:
        raise SystemExit("signed success immutable target tree is invalid")
remote_bytes = Path(remote_path).read_bytes()
local = Path(local_path)
if local.exists() or local.is_symlink():
    if not local.is_file() or local.is_symlink() or local.read_bytes() != remote_bytes:
        raise SystemExit("signed success local result conflicts")
source = Path(request_path)
processed = source.parent.parent / "processed" / source.name
if source.parent.name == "failed":
    raise SystemExit("signed success request is archived as failed")
if source != processed:
    if processed.exists() or processed.is_symlink():
        raise SystemExit("signed success processed archive conflicts")
if os.path.isfile(claim_path):
    with open(claim_path, encoding="utf-8") as fh:
        claim = json.load(fh)
    if (claim.get("request_id") != request.get("request_id") or
            claim.get("request_sha256") != hashlib.sha256(source.read_bytes()).hexdigest()):
        raise SystemExit("signed success claim conflicts")
if not local.exists():
    descriptor, temporary = tempfile.mkstemp(prefix=".signed-result-", dir=local.parent)
    try:
        with os.fdopen(descriptor, "wb") as fh:
            fh.write(remote_bytes)
            fh.flush()
            os.fsync(fh.fileno())
        try:
            os.link(temporary, local, follow_symlinks=False)
        except FileExistsError:
            if local.is_symlink() or not local.is_file() or local.read_bytes() != remote_bytes:
                raise SystemExit("signed success local result conflicts")
        directory = os.open(local.parent, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
    finally:
        os.unlink(temporary)
if source != processed:
    os.replace(source, processed)
    for directory_path in (source.parent, processed.parent):
        directory = os.open(directory_path, os.O_RDONLY | os.O_DIRECTORY)
        try:
            os.fsync(directory)
        finally:
            os.close(directory)
PY
  echo "signed COS success reconciled locally; hold and claim remain for separate isolation review"
  exit 0
fi

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

recovery_phase="$(python3 - "$journal" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    print(json.load(fh).get("recovery_phase", ""))
PY
)"
if [[ "$recovery_phase" == installer-started || "$recovery_phase" == smoke-started ||
      "$recovery_phase" == rollback-*-started ]]; then
  echo "previous installer or smoke attempt has an unknown outcome" >&2
  exit 75
fi
if [[ "$recovery_phase" == smoke-completed ]]; then
  [[ "$current" == "$original_current" && "$previous" == "$original_previous" ]] || exit 75
  "${verified_release}/deploy/runner/management-ui-lifecycle.sh" verify-closed || exit 75
  echo "limited recovery was already completed; hold remains"
  exit 0
fi
resume_installer=false
resume_smoke=false
if [[ "$current" == "$original_current" && "$previous" == "$original_previous" ]]; then
  case "$recovery_phase" in
    ""|read-only-original-pointers)
      record_recovery_phase "read-only-original-pointers"
      "${verified_release}/deploy/runner/management-ui-lifecycle.sh" quiesce || exit 75
      echo "original pointers unchanged; no installer, smoke or request replay; hold remains"
      exit 0 ;;
    cas-*-started|cas-*-completed)
      resume_installer=true ;;
    installer-completed)
      resume_smoke=true ;;
    *)
      echo "prior recovery work has an unknown outcome" >&2
      exit 75 ;;
  esac
fi
python3 - "$release_root" "$direction" "$original_current" "$original_previous" \
  "$request_release" "$request_file" "$verified_release" <<'PY' || exit 75
import json
import os
import stat
import sys
from pathlib import Path

release_root, direction, current, previous, target, request_path, helper_release = sys.argv[1:8]
root = Path(release_root).resolve(strict=True)
with open(request_path, encoding="utf-8") as fh:
    request = json.load(fh)
target_manifest = root / target / "manifest.json"
with target_manifest.open(encoding="utf-8") as fh:
    manifest = json.load(fh)
if manifest.get("release_sha") != target:
    raise SystemExit("recovery target manifest identity is invalid")
if direction != "R→T":
    if (manifest.get("request_id") != request.get("request_id") or
            manifest.get("previous_sha") != current or
            any(manifest.get(key) != request.get(key) for key in (
                "commit_sha", "runtime_sha", "deploy_bundle_sha", "runtime_artifact_profile",
                "release_scope", "restart_targets"))):
        raise SystemExit("recovery target manifest does not bind original request")
elif target != previous or manifest.get("previous_sha") == current:
    raise SystemExit("reverse recovery target lineage is invalid")
for sha in {current, target}:
    tree = root / sha
    if not tree.is_dir() or tree.is_symlink():
        raise SystemExit("recovery immutable tree is unavailable")
    for relative, mode in (("manifest.json", 0o444),
                           ("deploy/runner/install-release-systemd-units.sh", 0o755),
                           ("deploy/runner/smoke-release.sh", 0o755)):
        path = tree / relative
        metadata = path.lstat()
        if (not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or
                stat.S_IMODE(metadata.st_mode) != mode):
            raise SystemExit("recovery immutable tree file drifted")
rollback = Path(helper_release) / "deploy/runner/rollback-release.sh"
metadata = rollback.lstat()
if not stat.S_ISREG(metadata.st_mode) or metadata.st_uid != 0 or stat.S_IMODE(metadata.st_mode) != 0o755:
    raise SystemExit("recovery rollback primitive drifted")
PY

"${verified_release}/deploy/runner/management-ui-lifecycle.sh" quiesce || exit 75

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
if [[ "$resume_installer" == true || "$resume_smoke" == true ]]; then
  restore_sha="$original_current"
else
case "$direction:$current:$previous" in
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
fi

if [[ "$resume_smoke" != true ]]; then
  record_recovery_phase "installer-started"
  "${release_root}/${restore_sha}/deploy/runner/install-release-systemd-units.sh" \
    --release-root "$release_root" --release-sha "$restore_sha"
  record_recovery_phase "installer-completed"
fi
systemctl daemon-reload
cmp -s "$dropin" "$verified_release/deploy/runner/qintopia-agent-os-deploy-runner.service.d/10-recovery-hold.conf"
unit_stopped "$timer"
record_recovery_phase "smoke-started"
"${release_root}/${restore_sha}/deploy/runner/smoke-release.sh" \
  --release-root "$release_root" --restart-targets "$restart_targets"
"${verified_release}/deploy/runner/management-ui-lifecycle.sh" verify-closed || exit 75
record_recovery_phase "smoke-completed"
echo "limited recovery reached ${restore_sha}; hold remains until signed COS and archive reconciliation"
