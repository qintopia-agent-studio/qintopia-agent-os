#!/usr/bin/env bash
set -euo pipefail

release_root="/home/ubuntu/qintopia-agent-os-releases"
expected_current_sha=""
expected_previous_sha=""
restore_previous_sha=""
restore_previous_absent=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --release-root)
      release_root="${2:-}"
      shift 2
      ;;
    --expected-current-sha)
      expected_current_sha="${2:-}"
      shift 2
      ;;
    --expected-previous-sha)
      expected_previous_sha="${2:-}"
      shift 2
      ;;
    --restore-previous-sha)
      restore_previous_sha="${2:-}"
      shift 2
      ;;
    --restore-previous-absent)
      restore_previous_absent=true
      shift
      ;;
    -h | --help)
      echo "Usage: deploy/runner/rollback-release.sh [--release-root <dir>] --expected-current-sha <sha> --expected-previous-sha <sha> [--restore-previous-sha <sha> | --restore-previous-absent]"
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      exit 2
      ;;
  esac
done

for required_sha in "$expected_current_sha" "$expected_previous_sha"; do
  if [[ ! "$required_sha" =~ ^[0-9a-f]{40}$ ]]; then
    echo "expected current and previous SHAs are required" >&2
    exit 2
  fi
done
if [[ "$expected_current_sha" == "$expected_previous_sha" ]]; then
  echo "expected current and previous SHAs must differ" >&2
  exit 2
fi
if [[ -n "$restore_previous_sha" && ! "$restore_previous_sha" =~ ^[0-9a-f]{40}$ ]]; then
  echo "restore previous SHA must be a lowercase 40-character SHA" >&2
  exit 2
fi
if [[ -n "$restore_previous_sha" && "$restore_previous_absent" == "true" ]]; then
  echo "restore previous SHA and restore previous absent are mutually exclusive" >&2
  exit 2
fi

previous_target="$(readlink -f "${release_root}/previous" 2>/dev/null || true)"
current_target="$(readlink -f "${release_root}/current" 2>/dev/null || true)"

if [[ -z "$previous_target" || ! -d "$previous_target" ]]; then
  echo "previous release target is missing" >&2
  exit 1
fi
if [[ -z "$current_target" || ! -d "$current_target" ]]; then
  echo "current release target is missing" >&2
  exit 1
fi
if [[ "$previous_target" == "$current_target" ]]; then
  echo "previous release target must differ from current" >&2
  exit 1
fi

release_root_resolved="$(readlink -f "$release_root" 2>/dev/null || true)"
if [[ -z "$release_root_resolved" || ! -d "$release_root_resolved" ]]; then
  echo "release root is missing" >&2
  exit 1
fi

validate_release_target() {
  local label="$1"
  local target="$2"
  python3 - "$release_root_resolved" "$target" "$label" <<'PY'
from pathlib import Path
import re
import sys

root = Path(sys.argv[1]).resolve()
target = Path(sys.argv[2]).resolve()
label = sys.argv[3]
if target.parent != root:
    raise SystemExit(f"{label} release must be a direct child of the release root")
if not re.fullmatch(r"[0-9a-f]{40}", target.name):
    raise SystemExit(f"{label} release directory must be named by a lowercase commit SHA")
print(target.name)
PY
}

previous_sha="$(validate_release_target previous "$previous_target")"
current_sha="$(validate_release_target current "$current_target")"
if [[ "$current_sha" != "$expected_current_sha" ]]; then
  echo "current release does not match expected current SHA" >&2
  exit 1
fi
if [[ "$previous_sha" != "$expected_previous_sha" ]]; then
  echo "previous release does not match expected previous SHA" >&2
  exit 1
fi
python3 - "$current_target/manifest.json" "$previous_target/manifest.json" \
  "$expected_current_sha" "$expected_previous_sha" <<'PY'
import json
import sys

current_path, previous_path, expected_current, expected_previous = sys.argv[1:5]
with open(current_path, encoding="utf-8") as fh:
    current = json.load(fh)
with open(previous_path, encoding="utf-8") as fh:
    previous = json.load(fh)
if current.get("release_sha") != expected_current:
    raise SystemExit("current manifest release SHA mismatch")
if current.get("previous_sha") != expected_previous:
    raise SystemExit("current manifest previous SHA mismatch")
if previous.get("release_sha") != expected_previous:
    raise SystemExit("previous manifest release SHA mismatch")
PY
restore_previous_target=""
if [[ -n "$restore_previous_sha" ]]; then
  restore_previous_target="${release_root_resolved}/${restore_previous_sha}"
  if [[ ! -d "$restore_previous_target" || -L "$restore_previous_target" ]]; then
    echo "restore previous release target is missing or invalid" >&2
    exit 1
  fi
  validate_release_target restore-previous "$restore_previous_target" >/dev/null
fi

previous_installer="${previous_target}/deploy/runner/install-release-systemd-units.sh"
current_installer="${current_target}/deploy/runner/install-release-systemd-units.sh"
if [[ ! -x "$previous_installer" ]]; then
  echo "previous release systemd installer is missing or not executable" >&2
  exit 1
fi
if [[ ! -r "$current_installer" ]]; then
  echo "current release systemd installer is missing" >&2
  exit 1
fi

manifest_dir="$(mktemp -d)"
cleanup() {
  rm -rf "$manifest_dir"
}
trap cleanup EXIT

extract_unit_manifest() {
  local installer="$1"
  local output="$2"
  python3 - "$installer" "$output" <<'PY'
from pathlib import Path
import re
import shlex
import sys

installer = Path(sys.argv[1])
output = Path(sys.argv[2])
lines = installer.read_text(encoding="utf-8").splitlines()
units = []

for array_name in ("unit_files", "runner_unit_files"):
    start = next(
        (index for index, line in enumerate(lines) if line.strip() == f"{array_name}=("),
        None,
    )
    if start is None:
        raise SystemExit(f"{installer}: missing {array_name} manifest")
    closed = False
    for line in lines[start + 1 :]:
        if line.strip() == ")":
            closed = True
            break
        tokens = shlex.split(line, comments=True, posix=True)
        if not tokens:
            continue
        if len(tokens) != 1:
            raise SystemExit(f"{installer}: invalid {array_name} entry")
        unit = tokens[0]
        if not re.fullmatch(r"qintopia-[A-Za-z0-9@_.-]+\.(?:service|timer)", unit):
            raise SystemExit(f"{installer}: unsafe {array_name} entry")
        units.append(unit)
    if not closed:
        raise SystemExit(f"{installer}: unterminated {array_name} manifest")

if len(units) != len(set(units)):
    raise SystemExit(f"{installer}: duplicate managed unit")
output.write_text("".join(f"{unit}\n" for unit in sorted(units)), encoding="utf-8")
PY
}

extract_unit_manifest "$current_installer" "${manifest_dir}/current"
extract_unit_manifest "$previous_installer" "${manifest_dir}/previous"
python3 - "${manifest_dir}/current" "${manifest_dir}/previous" "${manifest_dir}/candidate-only" <<'PY'
from pathlib import Path
import sys

current = set(Path(sys.argv[1]).read_text(encoding="utf-8").splitlines())
previous = set(Path(sys.argv[2]).read_text(encoding="utf-8").splitlines())
Path(sys.argv[3]).write_text(
    "".join(f"{unit}\n" for unit in sorted(current - previous)),
    encoding="utf-8",
)
PY

atomic_symlink() {
  python3 - "$release_root" "$1" "$2" <<'PY'
import os
import secrets
import sys

root, name, target = sys.argv[1:4]
temporary = os.path.join(root, f".{name}.{os.getpid()}.{secrets.token_hex(8)}")
try:
    os.symlink(target, temporary)
    os.replace(temporary, os.path.join(root, name))
    descriptor = os.open(root, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
    try:
        os.fsync(descriptor)
    finally:
        os.close(descriptor)
finally:
    try:
        os.unlink(temporary)
    except FileNotFoundError:
        pass
PY
}

remove_symlink() {
  python3 - "$release_root" "$1" "$2" <<'PY'
import os
import stat
import sys

root, name, expected_target = sys.argv[1:4]
pointer = os.path.join(root, name)
try:
    metadata = os.lstat(pointer)
except FileNotFoundError:
    raise SystemExit(f"{name} release pointer disappeared before removal")
if not stat.S_ISLNK(metadata.st_mode):
    raise SystemExit(f"{name} release pointer must remain a symlink")
if os.path.realpath(pointer) != os.path.realpath(expected_target):
    raise SystemExit(f"{name} release pointer changed before removal")
os.unlink(pointer)
descriptor = os.open(root, os.O_RDONLY | getattr(os, "O_DIRECTORY", 0))
try:
    os.fsync(descriptor)
finally:
    os.close(descriptor)
PY
}

atomic_symlink rollback-from "$current_target"
atomic_symlink current "$previous_target"
if [[ "$restore_previous_absent" == "true" ]]; then
  remove_symlink previous "$previous_target"
elif [[ -n "$restore_previous_target" ]]; then
  atomic_symlink previous "$restore_previous_target"
else
  atomic_symlink previous "$current_target"
fi

if [[ "$(readlink -f "${release_root}/current" 2>/dev/null || true)" != "$previous_target" ]]; then
  echo "rollback current target verification failed" >&2
  exit 1
fi
if [[ "$restore_previous_absent" == "true" ]]; then
  if [[ -e "${release_root}/previous" || -L "${release_root}/previous" ]]; then
    echo "rollback previous absence verification failed" >&2
    exit 1
  fi
else
  expected_previous_pointer="$current_target"
  if [[ -n "$restore_previous_target" ]]; then
    expected_previous_pointer="$restore_previous_target"
  fi
  if [[ "$(readlink -f "${release_root}/previous" 2>/dev/null || true)" != "$expected_previous_pointer" ]]; then
    echo "rollback previous target verification failed" >&2
    exit 1
  fi
fi

"$previous_installer" --release-root "$release_root" --release-sha "$previous_sha"

systemctl_bin="${SYSTEMCTL:-systemctl}"
unit_dir="${QINTOPIA_SYSTEMD_UNIT_DIR:-/etc/systemd/system}"
if [[ "$unit_dir" != /* ]]; then
  echo "systemd unit directory must be absolute" >&2
  exit 1
fi

while IFS= read -r unit_name; do
  if [[ -z "$unit_name" ]]; then
    continue
  fi
  "$systemctl_bin" disable --now "$unit_name"
  rm -f -- "${unit_dir}/${unit_name}"
done <"${manifest_dir}/candidate-only"

"$systemctl_bin" daemon-reload

echo "Rolled back current and restored release-managed systemd units from ${previous_target}"
