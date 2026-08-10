# Xiaoman Weekly Recruitment Hermes Cron Runbook

Updated: 2026-08-10

This runbook moves the Saturday 10:00 Xiaoman weekly recruitment timer from the
release-managed systemd timer back to a Xiaoman Hermes cron job. Behavior does not
change: the job reads the resident-recruitment form window, writes the operator-review
draft work items to server-local state, and sends nothing to a group.

Read `docs/operations/hermes-cron-source-of-truth.md` first. Hermes `jobs.json` becomes
the source of truth for the schedule, the enablement, and the delivery target; the
business logic stays release-managed and reviewed.

## Reviewed Assets

- `runtime/hermes/scripts/qintopia_xiaoman_weekly_recruitment.sh` - the `no_agent`
  wrapper
- `runtime/hermes/cron/xiaoman/weekly-recruitment.job.json` - sanitized declaration
  template
- `runtime/hermes/cron/reviewed-cron-jobs.json` - reviewed allowlist entry
- `deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-hermes-cron.sh` - the only
  writer
- `deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh` - copy-based version management
- `deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh` - allowlist verdict
- `deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-production-config.sh` - env
  gate
- `deploy/sidecar/scripts/rollback-xiaoman-weekly-recruitment-production.sh` - timer
  stop

All of them must be present under `/home/ubuntu/qintopia-agent-os-releases/current`
before starting. Never hand-edit `/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json`
and never create a symlink or hardlink on it: the Hermes daemon rewrites the file with
`tempfile + atomic_replace` after every run, which breaks links on the first run.

## Enablement Ownership (read before executing)

The worker `xiaoman-weekly-recruitment-worker.sh` skips with exit 0 unless
`QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_ENABLED=1`, while
`rollback-xiaoman-weekly-recruitment-production.sh` refuses to stop the systemd timer
unless the persistent sidecar env file carries
`QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_ENABLED=0`. Those two requirements cannot both be
met from the env file after cutover, so the Hermes wrapper exports
`QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_ENABLED=1` itself.

End state after cutover:

- `/etc/qintopia/message-sidecar.env` keeps
  `QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_ENABLED=0`, which keeps the retired systemd path
  inert.
- The Hermes job's `enabled` field is the only enablement switch that matters.
- The remaining worker gates (`..._PRODUCTION_APPROVAL`,
  `QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE`,
  `QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE`,
  `QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE`) still come from the reviewed sidecar
  env file and must stay set.

## Preconditions

1. The reviewed release containing all assets above is promoted and `release/current`
   resolves to that 40-character release SHA.
2. The Xiaoman profile env defines exactly one reviewed origin channel. Confirm the key
   exists without printing its value:

   ```bash
   grep -c '^WECOM_HOME_CHANNEL=' /home/ubuntu/.hermes/profiles/xiaoman/.env
   ```

   The answer must be exactly `1`. If it is not, stop and ask the owner for the reviewed
   source of the origin chat id; do not paste a real group id into a command.

3. The current systemd timer is still the live producer. Record the last systemd run for
   the comparison in step 2:

   ```bash
   sudo -n stat -c '%n %s %y' \
     /home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment/latest-summary.json
   ```

   The activation script's `xiaoman-legacy-cron-observation-smoke.sh` precondition
   passes only while the Hermes recruitment job is absent or already reviewed; after
   this task lands, the `reviewed-cron-jobs.json` registry entry covers it, so the
   allowlist smoke in step 5 is the ongoing health signal.

## Step 1 - Install the Hermes job disabled

```bash
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_HERMES_CRON=approved-production-xiaoman-weekly-recruitment-hermes-cron \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-hermes-cron.sh --install
```

The script resolves the origin chat id from the Xiaoman profile env without printing it,
installs the wrapper into `/home/ubuntu/.hermes/scripts/` at mode `0700`, backs up
`jobs.json`, and inserts the job with `enabled: false` through an atomic replace that
keeps mode `0600 ubuntu:ubuntu`.

Expect `"status":"weekly_recruitment_hermes_cron_installed"`, `"job_enabled":false`,
`"backup_created":true`, and
`xiaoman_weekly_recruitment_hermes_cron_snapshot_sync_ok=true`. A disabled job never
fires, so this step is safe at any time.

## Step 2 - Run the wrapper once and compare artifacts

```bash
/home/ubuntu/.hermes/scripts/qintopia_xiaoman_weekly_recruitment.sh
echo "exit=$?"
```

The wrapper must print nothing and exit 0. Any stdout here would be delivered verbatim
to the origin chat by Hermes, so silence is the contract.

Compare the produced artifacts with the last systemd run recorded in the preconditions:

```bash
sudo -n cat \
  /home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment/latest-summary.json
sudo -n tail -n 5 \
  /home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment/hermes-cron.log
```

The summary must keep the same shape as the systemd run: `requires_human_confirmation`
true, `external_send_executed` false, `safe_for_member_chat` false, and a plausible
recruitment work-item summary. The worker emits an operations-review draft of work items
(form fields, audience, operator), not a deliverable message, and must never perform
external send. Compare structure and work-item types, not exact text. The log line must
read `run=ok`.

The config apply and activation scripts
(`apply-xiaoman-weekly-recruitment-production-config.sh`,
`activate-xiaoman-weekly-recruitment-production.sh`) stay valid for the persistent env
values the worker still consumes; the wrapper sources the same env file, so no config
re-apply is needed during cutover. Note in the runbook that the activation script's
`xiaoman-legacy-cron-observation-smoke.sh` precondition now passes only while the Hermes
job is absent or reviewed - after this task lands, the registry entry covers it.

Stop and investigate if the wrapper printed anything, exited non-zero, or the summary
lost a field. Nothing is live yet, so there is nothing to roll back.

## Step 3 - Retire the systemd timer

```bash
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_CONFIG=approved-production-xiaoman-weekly-recruitment-config \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-production-config.sh --disable
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_ROLLBACK=approved-production-xiaoman-weekly-recruitment-rollback \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/rollback-xiaoman-weekly-recruitment-production.sh
```

The config script sets `QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_ENABLED=0` and keeps the
three other reviewed values, then the rollback script disables and stops only
`qintopia-agentos-xiaoman-weekly-recruitment.timer` and its service.

Never leave both paths enabled. The systemd timer must be disabled before step 4.

## Step 4 - Enable the Hermes job

```bash
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_HERMES_CRON=approved-production-xiaoman-weekly-recruitment-hermes-cron \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-hermes-cron.sh --enable
```

`--enable` re-verifies that the installed wrapper still matches the release-local source
and that the live declaration still matches the reviewed name, schedule expression, and
script before flipping `enabled` to `true`. It is idempotent: a second run reports
`weekly_recruitment_hermes_cron_already_enabled` and writes nothing.

The gateway ticks every minute and reloads `jobs.json`, so no restart is needed.

## Step 5 - Observation

```bash
QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1 \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_OBSERVATION_ENABLE=1 \
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_EXPECTED_STATE=disabled \
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_RELEASE_SHA=<published-production-release-sha> \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-weekly-recruitment-production-observation-smoke.sh
```

The legacy cron smoke must report `status: "reviewed_declarations_only"` with
`reviewed_decl_count` including this job. The weekly recruitment smoke must report the
systemd timer as disabled: after cutover that is the expected healthy end state, not a
regression. The allowlist smoke plus the snapshot log are the health signal from now on.

## Step 6 - Snapshot verification

```bash
QINTOPIA_HERMES_CRON_SNAPSHOT=approved-production-hermes-cron-snapshot \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh
git -C /home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot log --oneline -3
git -C /home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot status --short
```

The snapshot repo must contain a commit covering the new job and the installed wrapper,
and must have no remote. Do not print file contents from the snapshot repo.

## Watch the first natural run

The next natural fire is Saturday 2026-08-15 10:00 Asia/Shanghai. Complete the cutover
before then and watch that run:

```bash
sudo -n tail -n 20 \
  /home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment/hermes-cron.log
```

A healthy run appends one `run=ok` line plus the sanitized worker summary and delivers
nothing to the group. Any group message from this job means the wrapper leaked stdout;
disable the job immediately with the rollback below.

## Rollback

Disable the Hermes job first, then restore the systemd timer only if the recruitment is
genuinely needed before the problem is fixed.

Ask the owner to set the Hermes job to disabled through Xiaoman conversation, or run the
reviewed retirement path for the observed `jobs.json`. To restore the systemd timer:

```bash
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_CONFIG=approved-production-xiaoman-weekly-recruitment-config \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-production-config.sh --enable
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_ACTIVATION=approved-production-xiaoman-weekly-recruitment \
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_RELEASE_SHA=<published-production-release-sha> \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/activate-xiaoman-weekly-recruitment-production.sh
```

The Hermes job must be disabled before the systemd timer is re-enabled, otherwise both
paths would run the same worker on Saturday morning.

## Acceptance

- Hermes `jobs.json` owns the schedule, the enablement, and the delivery target.
- Exactly one path is enabled at any time.
- The wrapper stays silent on success and prints one sanitized line on failure.
- The job still produces an operations-review draft of recruitment work items, and
  performs no external send.
- The allowlist observation smoke passes with the reviewed declaration.
- The server-local snapshot repo recorded the change.
- No real group id, chat id, or secret appears in any command output or evidence.
