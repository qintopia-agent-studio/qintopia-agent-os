# Hermes Cron Migration (Timers Back to Hermes)

Updated: 2026-08-10 Status: task breakdown approved by owner; implementation not started

## Background

Between 2026-08-09 and 2026-08-10 the recurring Agent timers were migrated from Hermes
conversation cron into release-managed systemd timers. The owner has reversed that
decision: **Hermes `jobs.json` is the source of truth** for recurring Agent tasks, so
people can adjust schedules and prompts by talking to Xiaoman/Erhua directly. The code
repository only keeps sanitized declaration templates and a copy-based snapshot sync for
version management.

This directory breaks the migration into one infrastructure task plus five per-timer
tasks. Tasks 1-5 are designed to run in parallel in separate sessions once Task 0 is
merged.

## Verified Server Contracts (read-only diagnostics, 2026-08-10)

Every executor must read this section before writing code. These facts come from live
inspection of `paxon-server` (`/home/ubuntu/.hermes/hermes-agent/cron/`) and supersede
any older assumptions in cutover runbooks.

1. `jobs.json` lives at `/home/ubuntu/.hermes/profiles/<profile>/cron/jobs.json`, mode
   `0600 ubuntu:ubuntu`. Envelope: `{"schema_version": 1, "jobs": [...], ...}`. The
   gateway ticks every minute (`.tick.lock`) and reloads the file; edits take effect
   without any restart.
2. The daemon rewrites `jobs.json` after every run via `tempfile + atomic_replace`
   (`cron/jobs.py` lines 477-483). **Symlinks and hardlinks on `jobs.json` break on the
   first run.** Version management must be copy-based snapshots.
3. Job fields: `id` is 12 lowercase hex chars (`uuid.uuid4().hex[:12]`, `jobs.py:635`),
   `name`, `schedule: {"kind": "cron", "expr": "<5-field cron>", "display": ...}`,
   `deliver`,
   `origin: {"platform": "wecom", "chat_id": ..., "chat_name": null, "thread_id": null}`,
   `no_agent`, `script`, `enabled`. The daemon owns runtime fields (`last_run_at`,
   `next_run_at`, `state`, `last_status`, `last_error`, `repeat.completed`); apply
   scripts must preserve them for untouched jobs.
4. `no_agent: true` + `script` contract (`cron/scheduler.py` lines 851-976 and
   1256-1360): the script must live inside `/home/ubuntu/.hermes/scripts/` (absolute
   paths are validated to stay inside it); `.sh` runs through bash; env is the gateway
   process env plus `HERMES_HOME` and `HOME=<profile home>`; cwd is the scripts dir;
   non-empty stdout is delivered verbatim to the `deliver` target, empty stdout is a
   silent success, non-zero exit is delivered as an error alert. stdout/stderr pass
   through `redact_sensitive_text`, but wrappers must still never print secrets.
5. Write protocol for `jobs.json`: timestamped backup copy, read-modify-write, atomic
   replace, keep mode `0600` and owner `ubuntu:ubuntu`. Model: the existing
   `deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh` (minus its
   whole-file hash pin, which is impossible for a live file the daemon mutates).
6. Sanitization: no real group ids, chat ids, wiki tokens, or secrets in git.
   Declaration templates use `{{QINTOPIA_XIAOMAN_TECHNICAL_HOME_CHANNEL}}`-style
   placeholders. The server-local snapshot git repo holds real content, has no remote,
   and stays mode `0700`.
7. Server access: only through the reviewed `paxon-server` SSH alias (see AGENTS.md).
   The server is a deployment target, not an editing workspace; all changes go through
   reviewed apply scripts with explicit owner approval env values.
8. Behavior parity: migration never changes what a task does (what it produces, whether
   it sends to a group). Enhancements such as "poster goes straight to the group" are
   separate tasks after the migration settles.

## Target Architecture

- Source of truth: `/home/ubuntu/.hermes/profiles/<profile>/cron/jobs.json` plus wrapper
  scripts in `/home/ubuntu/.hermes/scripts/`.
- Each migrated timer is a `no_agent: true` script job whose wrapper sources
  `/etc/qintopia/message-sidecar.env` and calls the existing release-managed worker
  under `/home/ubuntu/qintopia-agent-os-releases/current`. Business logic stays
  release-managed and reviewed; Hermes owns schedule, enablement, and delivery target.
- Version management: `sync-hermes-cron-snapshot.sh` copies live `jobs.json` files and
  `scripts/` into a server-local git repo (no remote) on a user timer and after every
  apply. Sanitized declaration templates in `runtime/hermes/cron/` are the review
  boundary in this monorepo.
- Governance: `*-legacy-cron-observation-smoke.sh` scripts switch from "fail on any
  declaration" to "allow only declarations listed in the reviewed registry"
  (`runtime/hermes/cron/reviewed-cron-jobs.json`).

## Tasks

| Task                                                                                     | Scope                                                                   | Depends on | Parallelizable   |
| ---------------------------------------------------------------------------------------- | ----------------------------------------------------------------------- | ---------- | ---------------- |
| [task-0-hermes-cron-infra.md](task-0-hermes-cron-infra.md)                               | Contract doc, registry, wrapper template, snapshot sync, smoke redesign | none       | must merge first |
| [task-1-xiaoman-weekly-preview.md](task-1-xiaoman-weekly-preview.md)                     | Mon 09:30 weekly preview (text + poster brief)                          | task 0     | yes              |
| [task-2-xiaoman-weekly-recruitment.md](task-2-xiaoman-weekly-recruitment.md)             | Sat 10:00 weekly recruitment                                            | task 0     | yes              |
| [task-3-xiaoman-weekly-plan-confirmation.md](task-3-xiaoman-weekly-plan-confirmation.md) | Sun 20:00 plan confirmation                                             | task 0     | yes              |
| [task-4-xiaoman-daily-case-report.md](task-4-xiaoman-daily-case-report.md)               | Daily 08:00 case report auto-publish                                    | task 0     | yes              |
| [task-5-erhua-morning-brief.md](task-5-erhua-morning-brief.md)                           | Daily 08:10 Erhua morning brief                                         | task 0     | yes              |

Expected shared-file conflicts between parallel PRs: each appends one entry to
`runtime/hermes/cron/reviewed-cron-jobs.json` and one section to AGENTS.md. Both are
single-block conflicts; rebase and keep both entries.

## Per-Task Shape (identical for tasks 1-5)

PR phase (repo only, no server writes):

1. `runtime/hermes/scripts/qintopia_<name>.sh` - wrapper script (template in task 0).
2. `runtime/hermes/cron/<profile>/<name>.job.json` - sanitized declaration template.
3. `deploy/sidecar/scripts/apply-<name>-hermes-cron.sh` - production apply script with
   owner-approval gate, backup, atomic write, `enabled: false` first, `--enable` second
   pass.
4. One reviewed entry appended to `runtime/hermes/cron/reviewed-cron-jobs.json`.
5. `docs/operations/<name>-hermes-cron-runbook.md` - execution runbook.
6. Focused tests plus updates to the observation smoke tests.
7. AGENTS.md rule updates for that timer.

Server phase (after merge, release, and owner approval):

1. Run the apply script on `paxon-server` with the approval env value; it writes the
   wrapper and inserts the job with `enabled: false`.
2. Manually run the wrapper once; compare output artifacts with the last systemd run.
3. Run the existing `rollback-<name>-production.sh` to disable the systemd timer.
4. Re-run the apply script with `--enable` to flip the Hermes job to `enabled: true`.
5. Run the observation smoke; it must pass with the reviewed-declarations status.
6. Run the snapshot sync; verify the server-local git repo recorded the change.

## Follow-ups (not part of tasks 0-5)

- Remove migrated targets from the `Activate Production Timers` GitHub workflow once
  their Hermes jobs are live.
- Retire or repurpose the legacy cron retirement scripts and the
  `production-legacy-cron-retirement` runner target.
- Poster-to-group enhancement for the Monday preview (builds on the poster brief landed
  2026-08-10) is designed only after task 1 settles.
