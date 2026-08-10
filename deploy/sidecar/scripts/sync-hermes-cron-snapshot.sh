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
HERMES_HOME="/home/ubuntu/.hermes"
SNAPSHOT_ROOT="/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot"

fail() {
  echo "Hermes cron snapshot failed: $1" >&2
  exit 1
}

if [[ ! -d "$SNAPSHOT_ROOT/.git" ]]; then
  if [[ "${QINTOPIA_HERMES_CRON_SNAPSHOT:-}" != "approved-production-hermes-cron-snapshot" ]]; then
    fail "first snapshot init requires QINTOPIA_HERMES_CRON_SNAPSHOT=approved-production-hermes-cron-snapshot"
  fi
fi

for fixed_path in "$HERMES_HOME" "$SNAPSHOT_ROOT" "$PYTHON_BIN" "$GIT_BIN"; do
  case "$fixed_path" in
    /home/ubuntu/* | /usr/bin/*) ;;
    *) fail "unexpected fixed path $fixed_path" ;;
  esac
done

[[ -x "$PYTHON_BIN" ]] || fail "fixed python3 is required"
[[ -x "$GIT_BIN" ]] || fail "fixed git is required"
[[ -d "$HERMES_HOME/profiles" ]] || fail "Hermes profiles directory is missing"

umask 077

if [[ ! -d "$SNAPSHOT_ROOT/.git" ]]; then
  mkdir -p "$SNAPSHOT_ROOT"
  chmod 0700 "$SNAPSHOT_ROOT"
  "$GIT_BIN" -C "$SNAPSHOT_ROOT" init --quiet >/dev/null
  "$GIT_BIN" -C "$SNAPSHOT_ROOT" config user.name "hermes-cron-snapshot"
  "$GIT_BIN" -C "$SNAPSHOT_ROOT" config user.email "hermes-cron-snapshot@localhost"
fi

"$PYTHON_BIN" - "$HERMES_HOME" "$SNAPSHOT_ROOT" <<'PY'
from __future__ import annotations

import hashlib
import shutil
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


def excluded(path: Path) -> bool:
    name = path.name
    if name == ".tick.lock" or ".bak" in name:
        return True
    return any(part in EXCLUDED_DIR_NAMES for part in path.parts)


def source_files():
    profiles_dir = hermes_home / "profiles"
    for jobs_file in sorted(profiles_dir.glob("*/cron/jobs.json")):
        profile = jobs_file.parent.parent.name
        yield jobs_file, Path("profiles") / profile / "cron" / "jobs.json"
    scripts_dir = hermes_home / "scripts"
    if scripts_dir.is_dir():
        for entry in sorted(scripts_dir.rglob("*")):
            if entry.is_file() and not excluded(entry.relative_to(scripts_dir)):
                yield entry, Path("scripts") / entry.relative_to(scripts_dir)


copied = 0
seen = set()
for src, rel in source_files():
    seen.add(str(rel))
    dest = snapshot_root / rel
    if dest.is_file() and sha256(dest) == sha256(src):
        continue
    dest.parent.mkdir(parents=True, exist_ok=True)
    shutil.copyfile(src, dest)
    dest.chmod(0o600)
    copied += 1

removed = 0
for existing in sorted(snapshot_root.rglob("*")):
    if not existing.is_file() or ".git" in existing.parts:
        continue
    rel = existing.relative_to(snapshot_root)
    if rel.parts[0] in {"profiles", "scripts"} and str(rel) not in seen:
        existing.unlink()
        removed += 1

print(f"snapshot_files_copied={copied} snapshot_files_removed={removed}")
PY

changed_count="$("$GIT_BIN" -C "$SNAPSHOT_ROOT" status --porcelain -- profiles scripts | wc -l | tr -d ' ')"
if [[ "$changed_count" == "0" ]]; then
  echo "snapshot_commit=skipped-no-changes"
  exit 0
fi

"$GIT_BIN" -C "$SNAPSHOT_ROOT" add -A -- profiles scripts
timestamp="$(date -u +%Y-%m-%dT%H:%M:%SZ)"
"$GIT_BIN" -C "$SNAPSHOT_ROOT" commit --quiet -m "snapshot ${timestamp}" >/dev/null
echo "snapshot_commit=created snapshot_entries=${changed_count}"
