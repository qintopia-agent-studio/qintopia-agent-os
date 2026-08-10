# Task 0: Hermes Cron Infrastructure

Updated: 2026-08-10 Status: PR phase implemented 2026-08-10 (uncommitted); server phase
pending release + owner approval Blocks: tasks 1-5 (they reuse everything built here)

## Goal

Build the shared infrastructure every per-timer migration task depends on: the reviewed
job registry, the wrapper script template, the server-local snapshot sync, and the
governance redesign that lets observation smokes accept reviewed Hermes cron
declarations instead of failing on any declaration.

Read `README.md` in this directory first. All server contract facts live there.

## Deliverables (PR phase, repo only)

### 1. `runtime/hermes/cron/reviewed-cron-jobs.json`

The reviewed allowlist registry. Observation smokes load it from
`/home/ubuntu/qintopia-agent-os-releases/current/runtime/hermes/cron/reviewed-cron-jobs.json`.
One entry per migrated timer:

```json
{
  "schema_version": 1,
  "reviewed_jobs": [
    {
      "profile": "xiaoman",
      "name": "<exact job name, matches jobs.json>",
      "schedule_expr": "<exact 5-field cron expr>",
      "script": "<exact script filename under /home/ubuntu/.hermes/scripts/>",
      "no_agent": true,
      "deliver": "origin",
      "approved_at": "<YYYY-MM-DD>",
      "plan": "docs/plans/active/hermes-cron-migration/task-<n>-*.md"
    }
  ]
}
```

Task 0 ships this file with an empty `reviewed_jobs` array. Matching rule for smokes: a
live declaration is allowed when `name` + `schedule.expr` + `script` + `no_agent` match
an entry exactly. `enabled` and runtime fields are daemon-owned and never matched.

### 2. `runtime/hermes/scripts/qintopia-hermes-cron-wrapper.template.sh`

The canonical wrapper every task instantiates (copy + fill two variables). Behavior:
source the sidecar env, run the release worker, append all output to a server-local log,
stay silent on success, emit one sanitized line and exit non-zero on failure (that line
becomes the WeCom error alert; keep it free of env values and secrets).

```bash
#!/usr/bin/env bash
set -euo pipefail

TASK_NAME="__TASK_NAME__"
WORKER="/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/__WORKER_SCRIPT__"
ENV_FILE="/etc/qintopia/message-sidecar.env"
STATE_DIR="/home/ubuntu/.local/state/qintopia-agentos/${TASK_NAME}"
LOG_FILE="${STATE_DIR}/hermes-cron.log"

umask 077
mkdir -p "$STATE_DIR"

set -a
# shellcheck disable=SC1090
. "$ENV_FILE"
set +a

if output="$("$WORKER" 2>&1)"; then
  printf '%s\n' "$output" >>"$LOG_FILE"
else
  rc=$?
  printf '%s\n' "$output" >>"$LOG_FILE"
  echo "${TASK_NAME} worker failed (exit=${rc}); evidence in server-local log"
  exit "$rc"
fi
```

Notes for executors:

- Workers self-check their enablement env flags and exit 0 with a skip message when
  disabled; that message lands in the log and the run stays silent. Do not re-implement
  flag checks in the wrapper.
- Some workers need extra `Environment=` bindings their systemd unit provides today
  (example: the daily case-report unit binds
  `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA`). Each per-timer task must diff its
  unit's `Environment=` lines against the worker's `require_env` checks and bind any
  missing values in its own wrapper. Never copy secrets into the wrapper.
- `bash` is guaranteed by the Hermes runner for `.sh` scripts; keep wrappers simple bash
  without arrays or process substitution.

### 3. `deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh`

Copy-based version management (the snapshot mechanism). Requirements:

- Snapshot root: `/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot` (git
  repo, `git init` if missing, no remote, dir mode `0700`).
- Mirrors every `/home/ubuntu/.hermes/profiles/*/cron/jobs.json` into
  `profiles/<name>/cron/jobs.json` and every file under `/home/ubuntu/.hermes/scripts/`
  into `scripts/`, excluding `__pycache__`, `*.bak*`, `.tick.lock`, `output/`.
- Copies only files whose content hash changed since the last snapshot (cheap compare).
- `git add -A && git commit` with message `snapshot <utc-iso8601>` only when the tree is
  dirty. Never pushes, never prints file contents.
- Fixed minimal `PATH`, `umask 077`, `set -euo pipefail`.
- Owner-approval gate on first install:
  `QINTOPIA_HERMES_CRON_SNAPSHOT=approved-production-hermes-cron-snapshot`. Subsequent
  timer runs are unattended and need no gate.

### 4. `deploy/sidecar/scripts/install-hermes-cron-snapshot-timer.sh`

Installs a systemd **user** timer `hermes-cron-snapshot.timer` (every 5 minutes,
`OnUnitActiveSec=5min`) that runs the sync script. This is infrastructure, not an Agent
task, so a user-level unit is acceptable; it is still installed only through this
reviewed script with the same approval gate. It must also run the sync script once
immediately after install so the baseline snapshot exists.

### 5. Governance redesign: `xiaoman-legacy-cron-observation-smoke.sh`

Change the verdict logic, keep every guardrail (fixed paths, test mode, size limits,
permission checks):

- Load the reviewed registry from the fixed release path
  (`$RELEASE_DIR/runtime/hermes/cron/reviewed-cron-jobs.json`,
  `RELEASE_DIR=/home/ubuntu/qintopia-agent-os-releases/current`, test-mode overridable
  alongside the existing test-root contract).
- For each live declaration in the Xiaoman `jobs.json`: allowed when it matches a
  registry entry for profile `xiaoman` (see matching rule above); otherwise fail with a
  sanitized mismatch report (name, expr, script only - never prompt or chat id).
- Success output gains `reviewed_decl_count` and reports
  `status: "reviewed_declarations_only"`, keeping `cron_decl_count` and
  `cron_file_sha256`. A file with zero declarations stays valid
  (`reviewed_decl_count: 0`).
- Update `tools/deploy/test-xiaoman-legacy-cron-observation.mjs`: keep existing cases
  (unknown declaration fails, empty file passes) and add reviewed-declaration passes,
  wrong-expr fails, wrong-script fails, daemon-runtime-fields ignored.
- Leave the script filename unchanged; activation/rollback scripts reference it. The
  Erhua smoke (`erhua-legacy-cron-observation-smoke.sh`) is task 5's job, not this one.

### 6. Contract doc: `docs/operations/hermes-cron-source-of-truth.md`

Short operator-facing doc: Hermes cron is the source of truth for recurring Agent tasks;
how conversational edits work; why `jobs.json` can never be a symlink (atomic replace);
how the snapshot sync provides version history; where sanitized declaration templates
live; how to add a new reviewed timer (registry entry + declaration template + apply
script + runbook).

### 7. Docs updates

- `runtime/hermes/README.md`: add the `cron/` and `scripts/` areas to the allowed
  git-managed inputs list (cron declarations are already listed; point at the new
  locations).
- `AGENTS.md`: add a new section describing the source-of-truth reversal (Hermes cron
  owns recurring Agent timers; systemd release-managed timers for the five migrated
  targets are being retired task-by-task; new recurring Agent tasks must be declared
  through the registry flow). Do not yet delete the per-timer systemd rules; tasks 1-5
  rewrite those as they land.
- `docs/plans/active/current-roadmap.md`: add an Active Direction entry linking here.

## Acceptance

- `pnpm lint:md` clean.
- `node tools/deploy/test-xiaoman-legacy-cron-observation.mjs` passes with the new
  cases.
- `pnpm deploy:contracts:check` (or the narrower
  `node tools/deploy/check-deploy-contracts.mjs`) passes; register the two new scripts
  wherever that checker requires registration.
- `git diff --check` clean; no real ids or secrets anywhere (scan the diff for WeCom
  id/token patterns before opening the PR).

## Forbidden

- No server writes in the PR phase.
- No symlink or hardlink anywhere in the design.
- Do not weaken the smoke's path/permission/size guards while adding the allowlist.
- Do not commit the snapshot repo to this monorepo; it is server-local runtime state.
