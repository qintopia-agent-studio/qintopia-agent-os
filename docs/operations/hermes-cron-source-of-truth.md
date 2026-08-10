# Hermes Cron as Source of Truth

Updated: 2026-08-10 Scope: all recurring Agent tasks ("定时任务") on the production
Hermes host

## Rule

Recurring Agent tasks live in Hermes cron, not in release-managed systemd timers. The
live declarations in `/home/ubuntu/.hermes/profiles/<profile>/cron/jobs.json` are the
source of truth, so anyone can adjust a schedule, pause a task, or rewrite a prompt by
talking to the Agent. The gateway ticks every minute and picks up changes without a
restart.

This reverses the 2026-08-09 direction that moved the Xiaoman/Erhua timers into
release-managed systemd units. Those units are being retired task by task; see
`docs/plans/active/hermes-cron-migration/`.

## What Can and Cannot Be a Symlink

`jobs.json` can **never** be a symlink or hardlink into git: the daemon rewrites it
after every run with `tempfile + atomic_replace`, which replaces the link with a fresh
regular file on the first run. Version history is copy-based instead:

- `deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh` mirrors every live `jobs.json`
  and every file under `/home/ubuntu/.hermes/scripts/` into a server-local git repo at
  `/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot` (no remote, mode
  `0700`), committing whenever content changes.
- The `hermes-cron-snapshot.timer` systemd **user** timer runs it every 5 minutes, and
  every cron apply script runs it after writing. Conversational edits therefore show up
  in git history within minutes, with full diff and rollback.
- The snapshot repo holds real chat ids and prompts. It never leaves the server; only
  sanitized counts may be quoted elsewhere.

## Repository Boundary

Git-managed (sanitized, reviewed through PR):

- `runtime/hermes/cron/reviewed-cron-jobs.json` - the allowlist registry. Observation
  smokes fail on any live declaration that does not match an entry exactly (profile +
  name + schedule expr + script + no_agent). Adding or changing a recurring task means
  appending an entry here.
- `runtime/hermes/cron/<profile>/<task>.job.json` - declaration templates with
  `{{PLACEHOLDER}}` chat ids.
- `runtime/hermes/scripts/` - wrapper templates that bridge Hermes script jobs to the
  release-managed workers. Business logic stays in the reviewed workers under
  `release/current`; Hermes owns schedule, enablement, and delivery target.

Runtime-local (never in git): live `jobs.json`, scripts deployed under
`/home/ubuntu/.hermes/scripts/`, and the snapshot repo.

## Adding a New Recurring Task

1. Copy `runtime/hermes/scripts/qintopia-hermes-cron-wrapper.template.sh` and fill in
   the task name and worker script.
2. Write the declaration template under `runtime/hermes/cron/<profile>/` with the job
   `enabled: false` and placeholder chat ids.
3. Append the matching entry to `runtime/hermes/cron/reviewed-cron-jobs.json`.
4. Add a reviewed apply script (approval-gated, backup + atomic write, `--enable` second
   pass) and an operations runbook, following
   `docs/plans/active/hermes-cron-migration/README.md` "Per-Task Shape".
5. After merge and release, run the apply script on the server, verify once, then
   enable.

## Governance Notes

- `xiaoman-legacy-cron-observation-smoke.sh` (and its Erhua sibling once migrated)
  verifies that live declarations match the registry. It is the health signal for
  conversational edits: an unreviewed schedule or script drift fails loudly, while
  daemon runtime fields (`last_run_at`, `next_run_at`, `state`) are ignored.
- Conversational edits that only flip `enabled` or touch runtime fields do not trip the
  smoke. Conversational edits to `schedule.expr` or `script` intentionally do: they
  require a registry PR so schedule changes stay reviewed.
