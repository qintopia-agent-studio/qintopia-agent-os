#!/usr/bin/env bash
# Snapshot live Hermes cron state (jobs.json files + scripts/) into a server-local git
# repository for version history. The snapshot repo holds real chat ids and prompts, so
# it never leaves the server: no remote, mode 0700, and nothing is printed beyond
# counts. First run (repo init) requires the owner approval value; later runs are
# unattended.
set -euo pipefail

PATH="/usr/bin:/bin"
PYTHON_BIN="/usr/bin/python3"
GIT_BIN="/usr/bin/git"
STAT_BIN="/usr/bin/stat"
CHMOD_BIN="/usr/bin/chmod"
CHOWN_BIN="/usr/bin/chown"
FIND_BIN="/usr/bin/find"
RUNUSER_BIN="/usr/sbin/runuser"
HERMES_HOME="/home/ubuntu/.hermes"
HOME_DIR="/home/ubuntu"
SNAPSHOT_ROOT="/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot"

fail() {
  echo "Hermes cron snapshot failed: $1" >&2
  exit 1
}

git_snapshot() {
  if [[ "$(id -u)" == "0" ]]; then
    "$RUNUSER_BIN" -u ubuntu -- /usr/bin/env -i HOME="$HOME_DIR" PATH="/usr/bin:/bin" \
      "$GIT_BIN" -C "$SNAPSHOT_ROOT" "$@"
  else
    "$GIT_BIN" -C "$SNAPSHOT_ROOT" "$@"
  fi
}

snapshot_git_valid() {
  local git_top
  local git_dir
  local snapshot_top
  [[ -d "$SNAPSHOT_ROOT" ]] || return 1
  snapshot_top="$(cd "$SNAPSHOT_ROOT" && pwd -P)" || return 1
  git_top="$(git_snapshot rev-parse --show-toplevel 2>/dev/null)" || return 1
  [[ "$git_top" == "$SNAPSHOT_ROOT" || "$git_top" == "$snapshot_top" ]] || return 1
  git_dir="$(git_snapshot rev-parse --git-dir 2>/dev/null)" || return 1
  case "$git_dir" in
    .git | "$SNAPSHOT_ROOT/.git") ;;
    *) return 1 ;;
  esac
  [[ -d "$SNAPSHOT_ROOT/.git" ]] || return 1
}

normalize_snapshot_permissions() {
  if [[ "$(id -u)" == "0" ]]; then
    "$CHOWN_BIN" -R "$SNAPSHOT_UID:$SNAPSHOT_GID" "$SNAPSHOT_ROOT"
  fi
  "$CHMOD_BIN" 0700 "$SNAPSHOT_ROOT"
  for scoped_dir in .git profiles scripts; do
    if [[ -d "$SNAPSHOT_ROOT/$scoped_dir" ]]; then
      "$FIND_BIN" "$SNAPSHOT_ROOT/$scoped_dir" -type d -exec "$CHMOD_BIN" 0700 {} +
      "$FIND_BIN" "$SNAPSHOT_ROOT/$scoped_dir" -type f -exec "$CHMOD_BIN" 0600 {} +
    fi
  done
}

for fixed_path in "$HERMES_HOME" "$SNAPSHOT_ROOT" "$HOME_DIR" "$PYTHON_BIN" "$GIT_BIN" "$STAT_BIN" "$CHMOD_BIN" "$CHOWN_BIN" "$FIND_BIN" "$RUNUSER_BIN"; do
  case "$fixed_path" in
    /home/ubuntu | /home/ubuntu/* | /usr/bin/* | /usr/sbin/*) ;;
    *) fail "unexpected fixed path $fixed_path" ;;
  esac
done

[[ -x "$PYTHON_BIN" ]] || fail "fixed python3 is required"
[[ -x "$GIT_BIN" ]] || fail "fixed git is required"
[[ -x "$STAT_BIN" ]] || fail "fixed stat is required"
[[ -x "$CHMOD_BIN" ]] || fail "fixed chmod is required"
[[ -x "$CHOWN_BIN" ]] || fail "fixed chown is required"
[[ -x "$FIND_BIN" ]] || fail "fixed find is required"
[[ -x "$RUNUSER_BIN" ]] || fail "fixed runuser is required"
[[ -d "$HERMES_HOME/profiles" ]] || fail "Hermes profiles directory is missing"
[[ -d "$HOME_DIR" ]] || fail "ubuntu home directory is missing"

SNAPSHOT_UID="$("$STAT_BIN" -c "%u" "$HOME_DIR")"
SNAPSHOT_GID="$("$STAT_BIN" -c "%g" "$HOME_DIR")"

if ! snapshot_git_valid; then
  if [[ "${QINTOPIA_HERMES_CRON_SNAPSHOT:-}" != "approved-production-hermes-cron-snapshot" ]]; then
    fail "first snapshot init requires QINTOPIA_HERMES_CRON_SNAPSHOT=approved-production-hermes-cron-snapshot"
  fi
fi

umask 077
mkdir -p "$SNAPSHOT_ROOT"
normalize_snapshot_permissions

if ! snapshot_git_valid; then
  git_snapshot init --quiet >/dev/null
  git_snapshot config user.name "hermes-cron-snapshot"
  git_snapshot config user.email "hermes-cron-snapshot@localhost"
  normalize_snapshot_permissions
fi

if [[ -n "$(git_snapshot remote)" ]]; then
  fail "snapshot repo must not have a remote"
fi

"$PYTHON_BIN" - "$HERMES_HOME" "$SNAPSHOT_ROOT" <<'PY'
from __future__ import annotations

import hashlib
import shutil
import stat
import sys
from pathlib import Path

hermes_home = Path(sys.argv[1]).resolve()
snapshot_root = Path(sys.argv[2]).resolve()


def fail(message: str) -> None:
    raise SystemExit(f"Hermes cron snapshot failed: {message}")


for root in (hermes_home, snapshot_root):
    if not root.is_absolute():
        fail("snapshot paths must be absolute")


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


EXCLUDED_DIR_NAMES = {"__pycache__", "output"}
REVIEWED_PROFILE_SCRIPTS = {
    ("erhua", "qintopia_erhua_morning_brief.sh"),
    ("xiaoman", "qintopia_xiaoman_daily_case_report.sh"),
    ("xiaoman", "qintopia_xiaoman_weekly_plan_confirmation.sh"),
    ("xiaoman", "qintopia_xiaoman_weekly_preview.sh"),
    ("xiaoman", "qintopia_xiaoman_weekly_recruitment.sh"),
}


def excluded(path: Path) -> bool:
    name = path.name
    if name == ".tick.lock" or ".bak" in name:
        return True
    return any(part in EXCLUDED_DIR_NAMES for part in path.parts)


def is_regular_file_no_symlink(path: Path) -> bool:
    try:
        return stat.S_ISREG(path.lstat().st_mode)
    except FileNotFoundError:
        return False


def ensure_directory_no_symlink(path: Path) -> None:
    if path == snapshot_root:
        return
    ensure_directory_no_symlink(path.parent)
    if path.exists() or path.is_symlink():
        try:
            mode = path.lstat().st_mode
        except FileNotFoundError:
            return
        if not stat.S_ISDIR(mode):
            rel = path.relative_to(snapshot_root)
            fail(f"snapshot destination is not a directory: {rel}")
        return
    path.mkdir(mode=0o700)


def prepare_dest_file(path: Path) -> None:
    ensure_directory_no_symlink(path.parent)
    if path.exists() or path.is_symlink():
        if not is_regular_file_no_symlink(path):
            path.unlink()


def source_files():
    profiles_dir = hermes_home / "profiles"
    for jobs_file in sorted(profiles_dir.glob("*/cron/jobs.json")):
        if not is_regular_file_no_symlink(jobs_file):
            continue
        profile = jobs_file.parent.parent.name
        yield jobs_file, Path("profiles") / profile / "cron" / "jobs.json"
    for script_file in sorted(profiles_dir.glob("*/scripts/*")):
        if is_regular_file_no_symlink(script_file):
            profile = script_file.parent.parent.name
            if (profile, script_file.name) in REVIEWED_PROFILE_SCRIPTS:
                script_rel = script_file.relative_to(script_file.parent)
                yield script_file, Path("profiles") / profile / "scripts" / script_rel
    scripts_dir = hermes_home / "scripts"
    if scripts_dir.is_dir():
        for entry in sorted(scripts_dir.rglob("*")):
            if is_regular_file_no_symlink(entry) and not excluded(entry.relative_to(scripts_dir)):
                yield entry, Path("scripts") / entry.relative_to(scripts_dir)


copied = 0
seen = set()
for src, rel in source_files():
    seen.add(str(rel))
    dest = snapshot_root / rel
    if is_regular_file_no_symlink(dest) and sha256(dest) == sha256(src):
        continue
    prepare_dest_file(dest)
    shutil.copyfile(src, dest)
    dest.chmod(0o600)
    copied += 1

removed = 0
for existing in sorted(snapshot_root.rglob("*")):
    if ".git" in existing.parts:
        continue
    rel = existing.relative_to(snapshot_root)
    if rel.parts[0] in {"profiles", "scripts"} and str(rel) not in seen:
        if existing.is_symlink() or is_regular_file_no_symlink(existing):
            existing.unlink()
            removed += 1

print(f"snapshot_files_copied={copied} snapshot_files_removed={removed}")
PY

normalize_snapshot_permissions
changed_count="$(git_snapshot status --porcelain -- profiles scripts | wc -l | tr -d ' ')"
if [[ "$changed_count" == "0" ]]; then
  echo "snapshot_commit=skipped-no-changes"
  normalize_snapshot_permissions
  exit 0
fi

git_snapshot add -A -- profiles scripts
timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
git_snapshot commit --quiet -m "snapshot ${timestamp}" >/dev/null
normalize_snapshot_permissions
echo "snapshot_commit=created snapshot_entries=${changed_count}"
