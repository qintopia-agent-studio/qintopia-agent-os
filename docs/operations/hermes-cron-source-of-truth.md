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
  plus profile-local no-agent wrappers under
  `/home/ubuntu/.hermes/profiles/<profile>/scripts/` into a server-local git repo at
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
  name + schedule expr + script + no_agent + deliver + origin platform). Adding or
  changing a recurring task means appending an entry here.
- `runtime/hermes/cron/<profile>/<task>.job.json` - declaration templates with
  `{{PLACEHOLDER}}` chat ids.
- `runtime/hermes/scripts/` - wrapper templates that bridge Hermes script jobs to the
  release-managed workers. Business logic stays in the reviewed workers under
  `release/current`; Hermes owns schedule, enablement, and delivery target.

Runtime-local (never in git): live `jobs.json`, no-agent scripts deployed under
`/home/ubuntu/.hermes/profiles/<profile>/scripts/`, legacy global helpers under
`/home/ubuntu/.hermes/scripts/`, and the snapshot repo. Reviewed apply scripts must
normalize profile-local no-agent wrapper owner/mode back to the live cron file/profile
owner and `0700`; preserving an old root-owned wrapper would make Hermes unable to
execute the reviewed script even when declaration parity passes.

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

- `xiaoman-legacy-cron-observation-smoke.sh` and
  `erhua-legacy-cron-observation-smoke.sh` verify that live declarations match the
  registry. They are the health signal for conversational edits: an unreviewed schedule
  or script drift fails loudly, while daemon runtime fields (`last_run_at`,
  `next_run_at`, `state`) are ignored.
- Conversational edits that only flip `enabled` or touch runtime fields do not trip the
  smoke. Conversational edits to `schedule.expr` or `script` intentionally do: they
  require a registry PR so schedule changes stay reviewed.

## Operating rules

Paths below are repository-relative; Sidecar subsections use `runtime/sidecar/`.

### Commands

- Erhua legacy Hermes cron observation:
  `QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh`
- Erhua legacy Hermes cron reviewed retirement:
  `QINTOPIA_ERHUA_LEGACY_CRON_RETIREMENT=approved-production-erhua-legacy-cron-retirement deploy/sidecar/scripts/retire-erhua-legacy-cron-production.sh`
- Xiaoman legacy Hermes cron reviewed retirement:
  `QINTOPIA_XIAOMAN_LEGACY_CRON_RETIREMENT=approved-production-xiaoman-legacy-cron-retirement deploy/sidecar/scripts/retire-xiaoman-legacy-cron-production.sh`
- Hermes cron snapshot sync (version history for the conversation-editable source of
  truth): `deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh` mirrors live `jobs.json`
  files plus profile-local no-agent wrappers under
  `/home/ubuntu/.hermes/profiles/<profile>/scripts/` into the server-local git repo at
  `/home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot`; first init requires
  `QINTOPIA_HERMES_CRON_SNAPSHOT=approved-production-hermes-cron-snapshot`. Timer
  install:
  `QINTOPIA_HERMES_CRON_SNAPSHOT=approved-production-hermes-cron-snapshot deploy/sidecar/scripts/install-hermes-cron-snapshot-timer.sh`.
  The snapshot repo holds real chat ids and prompts; it has no remote, stays `0700`, and
  only sanitized counts may leave the server. Snapshot sync may be invoked by the root
  deploy-runner apply path or by the ubuntu user timer; keep the script validating the
  git repo with `rev-parse --show-toplevel` bound exactly to the fixed snapshot root
  plus `rev-parse --git-dir`, rejecting remotes, and normalizing the fixed repo
  owner/modes back to `/home/ubuntu` ownership with `0700` directories and `0600` files.
  Root deploy-runner Git operations should run as the fixed ubuntu user rather than
  broadening Git trust or writing to a parent repository. If the fixed snapshot repo
  exists but has no source-file changes and no `HEAD` commit, create an empty baseline
  commit so `hermes-cron-snapshot` observation can prove the repo is initialized without
  exposing snapshot contents. Snapshot observation must also read Git state through the
  fixed ubuntu user; root direct `git -C` can misreport an existing ubuntu-owned repo as
  `repo_commit_missing`.
- Ordinary production release smoke may restart `hermes-erhua`, but Erhua Livecool
  provider/doctor verification belongs only to the reviewed Erhua profile activation
  path where profile metadata is present. Do not let general sidecar/deploy-bundle
  releases fail on Erhua provider state while unrelated runtime changes are being
  promoted. When ordinary release smoke fails, expose only the bounded
  `qintopia_smoke_release_safe_failure=` marker in deploy-result detail; never retain
  raw systemctl output, journal lines, env, prompts, or live Hermes cron content.
- Production legacy Hermes cron retirement should use the
  `Retire Production Legacy Crons` GitHub workflow after the reviewed release containing
  the runner support is deployed. It creates a signed
  `production-legacy-cron-retirement` deploy-runner request and accepts only these fixed
  targets: `erhua-legacy-cron` and `xiaoman-legacy-cron`. Retirement is explicit and
  must not be triggered as an automatic side effect of timer activation or observation
  failure. Xiaoman retirement depends on the deployed runner unit keeping
  `ProtectHome=read-only` while granting `ReadWritePaths` only to the fixed
  `/home/ubuntu/.hermes/profiles/xiaoman/cron` directory; do not grant write access to
  the whole Xiaoman profile. Erhua and Xiaoman retirement hash mismatches may emit only
  sanitized `actual_sha256`, reviewed `expected_sha256`, and declaration-count evidence;
  use that evidence for a follow-up reviewed expected-hash PR, never to bypass review or
  retire an unreviewed cron file.
- Hermes cron production apply scripts under
  `deploy/sidecar/scripts/apply-*-hermes-cron.sh` must remain executable (`100755`): the
  deploy runner executes the fixed script path directly, so a missing execute bit fails
  production apply with exit 126. Keep `tools/deploy/check-deploy-contracts.mjs`
  enforcing this.
- Recurring Agent timers are moving back to Hermes cron as the source of truth (owner
  decision, 2026-08-10): live `jobs.json` under
  `/home/ubuntu/.hermes/profiles/<profile>/cron/` is conversation-editable and wins; the
  repository keeps sanitized declaration templates under `runtime/hermes/cron/`, wrapper
  templates under `runtime/hermes/scripts/`, and the
  `runtime/hermes/cron/reviewed-cron-jobs.json` allowlist registry. Version history is
  copy-based via `deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh` into a
  server-local git repo; `jobs.json` must never be a symlink or hardlink because the
  daemon atomically replaces it after every run. New recurring Agent tasks must follow
  `docs/operations/hermes-cron-source-of-truth.md` and the migration shape in
  `docs/plans/active/hermes-cron-migration/`. Until each per-timer migration task lands,
  the existing release-managed timer rules below still apply to that timer.
- Xiaoman legacy Hermes cron observation smoke:
  `QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh`

### Core Rules

- The production deploy-runner service uses `ProtectHome=read-only`; any reviewed
  operation that mutates live Hermes profile state must have the smallest target path in
  `deploy/runner/qintopia-agent-os-deploy-runner.service` `ReadWritePaths`. Xiaoman
  legacy cron retirement needs `/home/ubuntu/.hermes/profiles/xiaoman/cron` there;
  without it backups and retired manifests fail with a read-only filesystem error.
- A new `ReadWritePaths` entry under `/home/ubuntu` must have its fixed target directory
  prepared before installing the updated deploy-runner unit. A missing non-optional path
  can prevent the next runner service start before `ExecStart`, so the release systemd
  installer owns the fixed Hermes cron snapshot root and ubuntu user systemd unit
  directory preparation. Root-owned preparation must reject symlinks in every fixed
  parent path component before create/chown, not only check the final directory.
- QiWe cron delivery to an explicit group must preserve group semantics. A bare
  `deliver=qiwe:<id>` without `conversation_type=group`, `chat_type=group`, a
  sender-derived `thread_id`, or a configured group allowlist match can be treated as a
  direct recipient and fail contact-guard lookup. Do not weaken direct-recipient contact
  guard to fix this; teach the delivery path to prove the target is a group through
  typed metadata, a configured group allowlist, or QiWe room-detail proof. Cache
  room-detail proof by `(guid, target)`, and count only response items whose
  `roomId`/`chatId`/`id` matches the target as proof; `memberList` or `roomName` alone
  is not enough to bypass the direct-recipient guard.
- `agents/xiaoman/profile-bundle` is observation-only. It may package the reviewed
  `SOUL.md`/`profile.yaml` templates, strict renderer, fake fixtures, and read-only
  parity smoke, but the deploy runner must not render it, read its server-local values,
  create live profile symlinks, or restart Xiaoman for it until a separate cutover PR
  records production parity and first-cutover rollback. Keep Xiaoman `config.yaml`,
  webhook secrets, channel identifiers, cron state, `.env`, sessions, auth, messages,
  memories, logs, cache, locks, and databases out of the bundle.
- Xiaoman Feishu wiki/Base URL readability checks must use the dedicated
  `qintopia_xiaoman_activity_plan_table_probe` wrapper. Do not infer readability from
  cron output, Kanban state, session history, generic Feishu tools, or whether the URL
  looks like a valid wiki link.
