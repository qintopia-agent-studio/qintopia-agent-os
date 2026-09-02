#!/usr/bin/env bash
set -euo pipefail

APPLY_APPROVAL="approved-production-qiwe-webhook-ingress-apply"
ROLLBACK_APPROVAL="approved-production-qiwe-webhook-ingress-rollback"
PUBLIC_HOST="qintopia.cn"
INCLUDE_LINE="include /etc/nginx/snippets/qintopia-qiwe-webhook.conf;"
AUTH_HEADER="X-Qintopia-Qiwe-Ingress-Auth"
ADAPTER_URL="http://127.0.0.1:18661/qiwe/webhook"

safe_failure() {
  printf 'qintopia_runtime_one_shot_safe_failure=%s\n' "$1" >&2
  exit "${2:-1}"
}

if [[ $# -ne 1 || ( "$1" != "--apply" && "$1" != "--rollback" ) ]]; then
  safe_failure "Qiwe webhook ingress action must be apply or rollback" 2
fi
action="${1#--}"

test_mode="${QINTOPIA_QIWE_WEBHOOK_INGRESS_TEST_MODE:-0}"
if [[ "$test_mode" != "0" && "$test_mode" != "1" ]]; then
  safe_failure "Qiwe webhook ingress test mode must be zero or one" 2
fi

if [[ "$test_mode" == "1" ]]; then
  release_current="${QINTOPIA_QIWE_WEBHOOK_INGRESS_RELEASE_CURRENT:?test release current is required}"
  config_file="${QINTOPIA_QIWE_WEBHOOK_INGRESS_CONFIG_FILE:?test config file is required}"
  site_file="${QINTOPIA_QIWE_WEBHOOK_INGRESS_SITE_FILE:?test site file is required}"
  snippet_file="${QINTOPIA_QIWE_WEBHOOK_INGRESS_SNIPPET_FILE:?test snippet file is required}"
  state_dir="${QINTOPIA_QIWE_WEBHOOK_INGRESS_STATE_DIR:?test state directory is required}"
  nginx_bin="${QINTOPIA_QIWE_WEBHOOK_INGRESS_NGINX_BIN:?test nginx binary is required}"
  systemctl_bin="${QINTOPIA_QIWE_WEBHOOK_INGRESS_SYSTEMCTL_BIN:?test systemctl binary is required}"
  curl_bin="${QINTOPIA_QIWE_WEBHOOK_INGRESS_CURL_BIN:?test curl binary is required}"
else
  for override in \
    QINTOPIA_QIWE_WEBHOOK_INGRESS_RELEASE_CURRENT \
    QINTOPIA_QIWE_WEBHOOK_INGRESS_CONFIG_FILE \
    QINTOPIA_QIWE_WEBHOOK_INGRESS_SITE_FILE \
    QINTOPIA_QIWE_WEBHOOK_INGRESS_SNIPPET_FILE \
    QINTOPIA_QIWE_WEBHOOK_INGRESS_STATE_DIR \
    QINTOPIA_QIWE_WEBHOOK_INGRESS_NGINX_BIN \
    QINTOPIA_QIWE_WEBHOOK_INGRESS_SYSTEMCTL_BIN \
    QINTOPIA_QIWE_WEBHOOK_INGRESS_CURL_BIN; do
    if [[ -n "${!override:-}" ]]; then
      safe_failure "Qiwe webhook ingress production paths are fixed" 2
    fi
  done
  if [[ "$(id -u)" -ne 0 ]]; then
    safe_failure "Qiwe webhook ingress production action requires root" 2
  fi
  release_current="/home/ubuntu/qintopia-agent-os-releases/current"
  config_file="/etc/qintopia/qiwe-webhook-ingress.env"
  site_file="/etc/nginx/sites-available/qintopia.cn"
  snippet_file="/etc/nginx/snippets/qintopia-qiwe-webhook.conf"
  state_dir="/var/lib/qintopia-agent-os-deploy/qiwe-webhook-ingress"
  nginx_bin="/usr/sbin/nginx"
  systemctl_bin="/usr/bin/systemctl"
  curl_bin="/usr/bin/curl"
fi

approval="${QINTOPIA_QIWE_WEBHOOK_INGRESS_APPROVAL:-}"
expected_release_sha="${QINTOPIA_QIWE_WEBHOOK_INGRESS_RELEASE_SHA:-}"
if [[ ! "$expected_release_sha" =~ ^[0-9a-f]{40}$ ]]; then
  safe_failure "Qiwe webhook ingress release SHA is required" 2
fi
if [[ "$action" == "apply" && "$approval" != "$APPLY_APPROVAL" ]]; then
  safe_failure "Qiwe webhook ingress apply requires explicit owner approval" 2
fi
if [[ "$action" == "rollback" && "$approval" != "$ROLLBACK_APPROVAL" ]]; then
  safe_failure "Qiwe webhook ingress rollback requires explicit owner approval" 2
fi

release_dir="$(/usr/bin/python3 - "$release_current" <<'PY'
from pathlib import Path
import sys

print(Path(sys.argv[1]).resolve())
PY
)"
if [[ -z "$release_dir" || ! -d "$release_dir" || "${release_dir##*/}" != "$expected_release_sha" ]]; then
  safe_failure "Qiwe webhook ingress release current binding failed" 2
fi
template_file="${release_dir}/runtime/nginx/templates/qiwe-webhook.location.conf.template"
disabled_template="${release_dir}/runtime/nginx/templates/qiwe-webhook.disabled.conf"
if [[ ! -r "$template_file" || ! -r "$disabled_template" ]]; then
  safe_failure "Qiwe webhook ingress release templates are unavailable" 2
fi

validate_file() {
  local path="$1"
  local purpose="$2"
  local secret="$3"
  /usr/bin/python3 - "$path" "$purpose" "$secret" "$test_mode" <<'PY'
import os
import stat
import sys

path, purpose, secret, test_mode = sys.argv[1:5]
try:
    metadata = os.lstat(path)
except FileNotFoundError:
    raise SystemExit(f"{purpose} is unavailable")
if not stat.S_ISREG(metadata.st_mode) or stat.S_ISLNK(metadata.st_mode):
    raise SystemExit(f"{purpose} must be a regular non symlink file")
if metadata.st_mode & 0o022:
    raise SystemExit(f"{purpose} must not be group or world writable")
if secret == "1" and metadata.st_mode & 0o077:
    raise SystemExit(f"{purpose} must be mode 0600")
if test_mode == "0" and metadata.st_uid != 0:
    raise SystemExit(f"{purpose} must be root owned")
if metadata.st_size > 1024 * 1024:
    raise SystemExit(f"{purpose} exceeds the fixed size bound")
PY
}

validate_file "$config_file" "Qiwe webhook ingress secret config" 1 || \
  safe_failure "Qiwe webhook ingress secret config validation failed" 2
validate_file "$site_file" "Qiwe webhook ingress site config" 0 || \
  safe_failure "Qiwe webhook ingress site config validation failed" 2
validate_file "$snippet_file" "Qiwe webhook ingress include" 0 || \
  safe_failure "Qiwe webhook ingress include validation failed" 2

if ! /usr/bin/python3 - "$site_file" "$INCLUDE_LINE" <<'PY'
import sys
from pathlib import Path

path = Path(sys.argv[1])
expected = sys.argv[2]
matches = [line for line in path.read_text(encoding="utf-8").splitlines() if line.strip() == expected]
if len(matches) != 1:
    raise SystemExit("fixed Qiwe webhook include must occur exactly once")
PY
then
  safe_failure "Qiwe webhook ingress fixed include is not bootstrapped" 2
fi

config_output="$(/usr/bin/python3 - "$config_file" <<'PY'
import re
import sys
from pathlib import Path

allowed = {
    "QINTOPIA_QIWE_WEBHOOK_PUBLIC_PATH_TOKEN",
    "QIWE_WEBHOOK_AUTH_TOKEN",
}
values = {}
line_re = re.compile(r"^([A-Za-z_][A-Za-z0-9_]*)=(.*)$")
for raw_line in Path(sys.argv[1]).read_text(encoding="utf-8").splitlines():
    line = raw_line.strip()
    if not line or line.startswith("#"):
        continue
    match = line_re.fullmatch(line)
    if not match:
        raise SystemExit("invalid ingress config line")
    key, value = match.groups()
    if key not in allowed or key in values:
        raise SystemExit("unsupported or duplicate ingress config key")
    if len(value) >= 2 and value[0] == value[-1] and value[0] in {"'", '"'}:
        value = value[1:-1]
    values[key] = value
if set(values) != allowed:
    raise SystemExit("ingress config keys are incomplete")
public_token = values["QINTOPIA_QIWE_WEBHOOK_PUBLIC_PATH_TOKEN"]
internal_token = values["QIWE_WEBHOOK_AUTH_TOKEN"]
if not re.fullmatch(r"[A-Za-z0-9_-]{48,128}", public_token):
    raise SystemExit("public path token must be 48 to 128 URL safe characters")
if not re.fullmatch(r"[A-Za-z0-9._~-]{43,256}", internal_token):
    raise SystemExit("internal auth token must be 43 to 256 header safe characters")
if public_token == internal_token:
    raise SystemExit("public and internal ingress tokens must differ")

def reject_obvious_placeholder(value, label):
    lowered = value.lower()
    if len(set(value)) < 10:
        raise SystemExit(f"{label} is an obvious low-complexity value")
    if re.search(r"(.)\1{4,}", value):
        raise SystemExit(f"{label} contains an obvious repeated run")
    if any(word in lowered for word in ("changeme", "example", "placeholder", "replace", "testtoken")):
        raise SystemExit(f"{label} contains an obvious placeholder")
    for width in range(1, min(8, len(value) // 2) + 1):
        if len(value) % width == 0 and value == value[:width] * (len(value) // width):
            raise SystemExit(f"{label} is an obvious repeated pattern")

reject_obvious_placeholder(public_token, "public path token")
reject_obvious_placeholder(internal_token, "internal auth token")
print(public_token)
print(internal_token)
PY
)" || safe_failure "Qiwe webhook ingress secret config content is invalid" 2
if [[ "$config_output" != *$'\n'* || "${config_output#*$'\n'}" == *$'\n'* ]]; then
  safe_failure "Qiwe webhook ingress secret config content is invalid" 2
fi
public_token="${config_output%%$'\n'*}"
internal_token="${config_output#*$'\n'}"
unset config_output

mkdir -p "$state_dir"
chmod 0700 "$state_dir"
work_dir="$(mktemp -d "${state_dir}/action.XXXXXX")"
chmod 0700 "$work_dir"
cleanup() {
  rm -rf "$work_dir"
}
trap cleanup EXIT

atomic_copy() {
  local source="$1"
  local target="$2"
  local mode="$3"
  /usr/bin/python3 - "$source" "$target" "$mode" <<'PY'
import os
import secrets
import sys
from pathlib import Path

source = Path(sys.argv[1])
target = Path(sys.argv[2])
mode = int(sys.argv[3], 8)
temporary = target.parent / f".{target.name}.{os.getpid()}.{secrets.token_hex(8)}"
descriptor = os.open(temporary, os.O_WRONLY | os.O_CREAT | os.O_EXCL, mode)
try:
    with source.open("rb") as reader, os.fdopen(descriptor, "wb", closefd=False) as writer:
        while chunk := reader.read(65536):
            writer.write(chunk)
        writer.flush()
        os.fsync(writer.fileno())
    os.fchmod(descriptor, mode)
finally:
    os.close(descriptor)
try:
    os.replace(temporary, target)
    directory = os.open(target.parent, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(directory)
    finally:
        os.close(directory)
finally:
    try:
        temporary.unlink()
    except FileNotFoundError:
        pass
PY
}

nginx_test() {
  "$nginx_bin" -t >/dev/null 2>&1
}

reload_nginx() {
  "$systemctl_bin" reload nginx.service >/dev/null 2>&1
}

curl_status() {
  local probe="$1"
  local probe_public_token="${2:-}"
  local probe_internal_token="${3:-}"
  local status=""
  case "$probe" in
    adapter-unauthorized)
      status="$({
        printf 'silent\nshow-error\noutput = "/dev/null"\nwrite-out = "%%{http_code}"\n'
        printf 'request = "POST"\nheader = "Content-Type: application/json"\ndata = "{}"\n'
        printf 'connect-timeout = 1\nmax-time = 2\nurl = "%s"\n' "$ADAPTER_URL"
      } | "$curl_bin" --config - 2>/dev/null)" || return 1
      ;;
    adapter-authorized)
      status="$({
        printf 'silent\nshow-error\noutput = "/dev/null"\nwrite-out = "%%{http_code}"\n'
        printf 'request = "POST"\nheader = "Content-Type: application/json"\n'
        printf 'header = "%s: %s"\n' "$AUTH_HEADER" "$probe_internal_token"
        printf 'data = "{}"\nconnect-timeout = 1\nmax-time = 2\nurl = "%s"\n' "$ADAPTER_URL"
      } | "$curl_bin" --config - 2>/dev/null)" || return 1
      ;;
    public-exact)
      status="$({
        printf 'silent\nshow-error\noutput = "/dev/null"\nwrite-out = "%%{http_code}"\n'
        printf 'request = "POST"\nheader = "Content-Type: application/json"\ndata = "{}"\n'
        printf 'connect-timeout = 1\nmax-time = 2\nresolve = "%s:443:127.0.0.1"\n' "$PUBLIC_HOST"
        printf 'url = "https://%s/qiwe/webhook/%s"\n' "$PUBLIC_HOST" "$probe_public_token"
      } | "$curl_bin" --config - 2>/dev/null)" || return 1
      ;;
    public-wrong-path)
      status="$({
        printf 'silent\nshow-error\noutput = "/dev/null"\nwrite-out = "%%{http_code}"\n'
        printf 'request = "POST"\nheader = "Content-Type: application/json"\ndata = "{}"\n'
        printf 'connect-timeout = 1\nmax-time = 2\nresolve = "%s:443:127.0.0.1"\n' "$PUBLIC_HOST"
        printf 'url = "https://%s/qiwe/webhook/qintopia-invalid-ingress-probe"\n' "$PUBLIC_HOST"
      } | "$curl_bin" --config - 2>/dev/null)" || return 1
      ;;
    *)
      return 2
      ;;
  esac
  [[ "$status" =~ ^[0-9]{3}$ ]] || return 1
  printf '%s' "$status"
}

run_adapter_smoke() {
  local token="$1"
  local unauthorized authorized
  unauthorized="$(curl_status adapter-unauthorized)" || return 1
  [[ "$unauthorized" == "401" ]] || return 1
  authorized="$(curl_status adapter-authorized "" "$token")" || return 1
  [[ "$authorized" =~ ^2[0-9]{2}$ ]]
}

run_active_smoke() {
  local route_token="$1"
  local auth_token="$2"
  local exact wrong
  run_adapter_smoke "$auth_token" || return 1
  exact="$(curl_status public-exact "$route_token")" || return 1
  [[ "$exact" =~ ^2[0-9]{2}$ ]] || return 1
  wrong="$(curl_status public-wrong-path)" || return 1
  [[ "$wrong" == "404" ]]
}

run_disabled_smoke() {
  local route_token="$1"
  local exact wrong unauthorized
  unauthorized="$(curl_status adapter-unauthorized)" || return 1
  [[ "$unauthorized" == "401" ]] || return 1
  exact="$(curl_status public-exact "$route_token")" || return 1
  [[ "$exact" == "404" ]] || return 1
  wrong="$(curl_status public-wrong-path)" || return 1
  [[ "$wrong" == "404" ]]
}

canonical_values() {
  /usr/bin/python3 - "$1" "$disabled_template" "$template_file" <<'PY'
import re
import sys
from pathlib import Path

actual = Path(sys.argv[1]).read_text(encoding="utf-8")
disabled = Path(sys.argv[2]).read_text(encoding="utf-8")
if actual == disabled:
    print("disabled")
    raise SystemExit(0)

template = Path(sys.argv[3]).read_text(encoding="utf-8")
if template.count("__QIWE_PUBLIC_PATH_TOKEN__") != 1 or template.count("__QIWE_INTERNAL_AUTH_TOKEN__") != 1:
    raise SystemExit("ingress template placeholders are invalid")
route = re.findall(r"location\s*=\s*/qiwe/webhook/([A-Za-z0-9_-]{48,128})\s*\{", actual)
header = re.findall(r"proxy_set_header\s+X-Qintopia-Qiwe-Ingress-Auth\s+([A-Za-z0-9._~-]{43,256});", actual)
if len(route) != 1 or len(header) != 1:
    raise SystemExit("rendered ingress values are invalid")
if route[0] == header[0]:
    raise SystemExit("rendered public and internal ingress tokens must differ")

def reject_obvious_placeholder(value):
    lowered = value.lower()
    if len(set(value)) < 10 or re.search(r"(.)\1{4,}", value):
        raise SystemExit("rendered ingress contains an obvious low-complexity value")
    if any(word in lowered for word in ("changeme", "example", "placeholder", "replace", "testtoken")):
        raise SystemExit("rendered ingress contains an obvious placeholder")
    for width in range(1, min(8, len(value) // 2) + 1):
        if len(value) % width == 0 and value == value[:width] * (len(value) // width):
            raise SystemExit("rendered ingress contains an obvious repeated pattern")

reject_obvious_placeholder(route[0])
reject_obvious_placeholder(header[0])
expected = template.replace("__QIWE_PUBLIC_PATH_TOKEN__", route[0]).replace(
    "__QIWE_INTERNAL_AUTH_TOKEN__", header[0]
)
if actual != expected:
    raise SystemExit("ingress include is not a canonical reviewed template")
print("active")
print(route[0])
print(header[0])
PY
}

smoke_file() {
  local file="$1"
  local canonical kind active_values previous_public previous_internal
  canonical="$(canonical_values "$file")" || return 1
  kind="${canonical%%$'\n'*}"
  if [[ "$kind" == "disabled" ]]; then
    run_disabled_smoke "$public_token"
    return
  fi
  [[ "$kind" == "active" && "$canonical" == *$'\n'* ]] || return 1
  active_values="${canonical#*$'\n'}"
  [[ "$active_values" == *$'\n'* && "${active_values#*$'\n'}" != *$'\n'* ]] || return 1
  previous_public="${active_values%%$'\n'*}"
  previous_internal="${active_values#*$'\n'}"
  run_active_smoke "$previous_public" "$previous_internal"
}

restore_and_reload() {
  local restore_file="$1"
  atomic_copy "$restore_file" "$snippet_file" 0600 || return 1
  nginx_test || return 1
  reload_nginx || return 1
  smoke_file "$restore_file"
}

canonical_values "$snippet_file" >/dev/null || \
  safe_failure "Qiwe webhook ingress current include is not canonical" 2

if [[ "$action" == "apply" ]]; then
  run_adapter_smoke "$internal_token" || \
    safe_failure "Qiwe webhook ingress adapter authentication smoke failed" 1

  candidate="${work_dir}/candidate.conf"
  if ! printf '%s\n%s\n' "$public_token" "$internal_token" | \
    /usr/bin/python3 -c '
import os
import sys
from pathlib import Path

template_path, output_path = sys.argv[1:3]
tokens = sys.stdin.read().splitlines()
if len(tokens) != 2:
    raise SystemExit("ingress render token input is invalid")
public_token, internal_token = tokens
text = Path(template_path).read_text(encoding="utf-8")
if text.count("__QIWE_PUBLIC_PATH_TOKEN__") != 1 or text.count("__QIWE_INTERNAL_AUTH_TOKEN__") != 1:
    raise SystemExit("ingress template placeholders are invalid")
rendered = text.replace("__QIWE_PUBLIC_PATH_TOKEN__", public_token).replace(
    "__QIWE_INTERNAL_AUTH_TOKEN__", internal_token
)
descriptor = os.open(output_path, os.O_WRONLY | os.O_CREAT | os.O_EXCL, 0o600)
with os.fdopen(descriptor, "w", encoding="utf-8") as fh:
    fh.write(rendered)
    fh.flush()
    os.fsync(fh.fileno())
' "$template_file" "$candidate"
  then
    safe_failure "Qiwe webhook ingress template rendering failed" 1
  fi

  if cmp -s "$candidate" "$snippet_file"; then
    nginx_test && reload_nginx && run_active_smoke "$public_token" "$internal_token" || \
      safe_failure "Qiwe webhook ingress idempotent smoke failed" 1
    printf 'qiwe_webhook_ingress_action=apply\nqiwe_webhook_ingress_state=enabled\n'
    exit 0
  fi

  previous="${work_dir}/previous.conf"
  atomic_copy "$snippet_file" "$previous" 0600 || \
    safe_failure "Qiwe webhook ingress previous config backup failed" 1
  atomic_copy "$previous" "${state_dir}/rollback.conf" 0600 || \
    safe_failure "Qiwe webhook ingress durable rollback preparation failed" 1

  if ! atomic_copy "$candidate" "$snippet_file" 0600; then
    safe_failure "Qiwe webhook ingress atomic apply failed" 1
  fi
  if ! nginx_test; then
    restore_and_reload "$previous" >/dev/null 2>&1 || true
    safe_failure "Qiwe webhook ingress nginx validation failed and previous config was restored" 1
  fi
  if ! reload_nginx || ! run_active_smoke "$public_token" "$internal_token"; then
    if restore_and_reload "$previous" >/dev/null 2>&1; then
      safe_failure "Qiwe webhook ingress activation smoke failed and previous config was restored" 1
    fi
    safe_failure "Qiwe webhook ingress activation smoke failed and automatic restore failed" 1
  fi
  printf 'qiwe_webhook_ingress_action=apply\nqiwe_webhook_ingress_state=enabled\n'
  exit 0
fi

rollback_file="${state_dir}/rollback.conf"
validate_file "$rollback_file" "Qiwe webhook ingress rollback config" 1 || \
  safe_failure "Qiwe webhook ingress rollback state is unavailable" 2
canonical_values "$rollback_file" >/dev/null || \
  safe_failure "Qiwe webhook ingress rollback state is not canonical" 2
if cmp -s "$rollback_file" "$snippet_file"; then
  nginx_test && reload_nginx && smoke_file "$rollback_file" || \
    safe_failure "Qiwe webhook ingress idempotent rollback smoke failed" 1
  printf 'qiwe_webhook_ingress_action=rollback\nqiwe_webhook_ingress_state=restored\n'
  exit 0
fi

current="${work_dir}/current.conf"
atomic_copy "$snippet_file" "$current" 0600 || \
  safe_failure "Qiwe webhook ingress current config recovery backup failed" 1
if ! atomic_copy "$rollback_file" "$snippet_file" 0600; then
  safe_failure "Qiwe webhook ingress atomic rollback failed" 1
fi
if ! nginx_test || ! reload_nginx || ! smoke_file "$rollback_file"; then
  if restore_and_reload "$current" >/dev/null 2>&1; then
    safe_failure "Qiwe webhook ingress rollback smoke failed and current config was restored" 1
  fi
  safe_failure "Qiwe webhook ingress rollback smoke failed and recovery failed" 1
fi
printf 'qiwe_webhook_ingress_action=rollback\nqiwe_webhook_ingress_state=restored\n'
