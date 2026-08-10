# Xiaoman Daily Case Report Hermes Cron Runbook

Updated: 2026-08-11

This runbook moves the daily 08:00 Xiaoman daily case-report auto-publish timer from the
release-managed systemd timer back to a Xiaoman Hermes cron job. Behavior does not
change, including the send path: the worker renders the JPEG, uploads it through the
Huabaosi Feishu primary-storage boundary, creates or updates the approved
`generated_image` artifact and one automatic `group_message_request`, and the actual
QiWe delivery keeps riding the separate `operations-group-send-ready` worker chain. That
chain is untouched by this migration.

Read `docs/operations/hermes-cron-source-of-truth.md` first. Hermes `jobs.json` becomes
the source of truth for the schedule, the enablement, and the delivery target; the
business logic stays release-managed and reviewed.

## Reviewed Assets

- `runtime/hermes/scripts/qintopia_xiaoman_daily_case_report.sh` - the `no_agent`
  wrapper
- `runtime/hermes/cron/xiaoman/daily-case-report.job.json` - sanitized declaration
  template
- `runtime/hermes/cron/reviewed-cron-jobs.json` - reviewed allowlist entry
- `deploy/sidecar/scripts/apply-xiaoman-daily-case-report-hermes-cron.sh` - the only
  writer
- `deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh` - copy-based version management
- `deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh` - allowlist verdict
- `deploy/sidecar/scripts/apply-xiaoman-daily-case-report-production-config.py` - env
  gate
- `deploy/sidecar/scripts/rollback-xiaoman-daily-case-report-auto-publish-production.sh` -
  timer stop

All of them must be present under `/home/ubuntu/qintopia-agent-os-releases/current`
before starting. Never hand-edit `/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json`
and never create a symlink or hardlink on it: the Hermes daemon rewrites the file with
`tempfile + atomic_replace` after every run, which breaks links on the first run.

## Enablement Ownership (read before executing)

The worker `xiaoman-daily-case-report-auto-publish-worker.sh` skips with exit 0 unless
`QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=1`, while
`rollback-xiaoman-daily-case-report-auto-publish-production.sh` expects the persistent
sidecar env file to carry `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=0`
before it stops the systemd timer. Those two requirements cannot both be met from the
env file after cutover, so the Hermes wrapper exports
`QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=1` itself.

End state after cutover:

- `/etc/qintopia/message-sidecar.env` keeps
  `QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_ENABLED=0`, which keeps the retired
  systemd path inert.
- The Hermes job's `enabled` field is the only enablement switch that matters.
- The remaining worker gates (production approval, database URL, source chat id, target
  group id, storage backend, read-through switch) still come from the reviewed sidecar
  env file and must stay set.

## Wrapper Release Binding (read before executing)

The retired systemd unit bound both release SHAs at the exec boundary. The wrapper
reproduces that binding: after sourcing the persistent env it derives the 40-character
release SHA from `release/current` and exports it as both `QINTOPIA_DEPLOYED_COMMIT_SHA`
and `QINTOPIA_HUABAOSI_FEISHU_PRODUCTION_RELEASE_SHA`, so a stale persistent env value
cannot override the release binding and the worker's Feishu upload boundary keeps its
release identity. The wrapper also pins `PATH=/usr/bin:/bin` so the system Pillow
renderer (`/usr/bin/python3`) and the fixed `/usr/bin/psql` fallback stay available
exactly as under systemd.

The `Run Production Runtime One-Shot` GitHub workflow target
`xiaoman-daily-case-report-auto-publish-backfill` stays valid after this migration: it
calls the worker boundary directly with its own one-day `--date` override and does not
depend on which scheduler owns the daily fire. Do not "fix" or retire that target as
part of this cutover.

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

3. The current systemd timer is still the live producer. Run this cutover on a day after
   the 08:00 systemd run (the 2026-08-10 08:00 run is the reference) so the manual
   wrapper run in step 2 can be compared against a same-day systemd result. Record the
   last systemd run state for the comparison without printing media URIs or group ids.

   The activation script's `xiaoman-legacy-cron-observation-smoke.sh` precondition
   passes only while the Hermes daily case-report job is absent or already reviewed;
   after this task lands, the `reviewed-cron-jobs.json` registry entry covers it, so the
   allowlist smoke in step 5 is the ongoing health signal.

## Step 1 - Install the Hermes job disabled

```bash
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_HERMES_CRON=approved-production-xiaoman-daily-case-report-hermes-cron \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-daily-case-report-hermes-cron.sh --install
```

The script resolves the origin chat id from the Xiaoman profile env without printing it,
installs the wrapper into `/home/ubuntu/.hermes/scripts/` at mode `0700`, backs up
`jobs.json`, and inserts the job with `enabled: false` through an atomic replace that
keeps mode `0600 ubuntu:ubuntu`.

Expect `"status":"daily_case_report_hermes_cron_installed"`, `"job_enabled":false`,
`"backup_created":true`, and
`xiaoman_daily_case_report_hermes_cron_snapshot_sync_ok=true`. A disabled job never
fires, so this step is safe at any time.

## Step 2 - Run the wrapper once and compare evidence

```bash
/home/ubuntu/.hermes/scripts/qintopia_xiaoman_daily_case_report.sh
echo "exit=$?"
```

The wrapper must print nothing and exit 0. Any stdout here would be delivered verbatim
to the origin chat by Hermes, so silence is the contract.

The worker's Feishu-backed publish is idempotent: it reuses only an existing artifact
whose id matches the reviewed upload evidence and rejects conflicting random-id
artifacts. The manual run must therefore either reproduce the same artifact and
`group_message_request` state as the 08:00 systemd run or no-op; it must not create a
conflicting second artifact for the same day. Compare sanitized evidence only - record
artifact ids and request states, never media URIs, database URLs, or group ids.

```bash
sudo -n tail -n 5 \
  /home/ubuntu/.local/state/qintopia-agentos/xiaoman-daily-case-report/hermes-cron.log
```

The log line must read `run=ok`.

The config apply and activation scripts
(`apply-xiaoman-daily-case-report-production-config.py`,
`activate-xiaoman-daily-case-report-auto-publish-production.sh`) stay valid for the
persistent env values the worker still consumes; the wrapper sources the same env file,
so no config re-apply is needed during cutover.

Stop and investigate if the wrapper printed anything, exited non-zero, or produced a
conflicting artifact. Nothing is live yet, so there is nothing to roll back.

## Step 3 - Retire the systemd timer

Use the production configuration entrypoint with `desired_state: "disabled"` to set the
persistent disabled flag:

```bash
sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-daily-case-report-production-config.py \
  --stdin \
  --apply \
  --approval approved-production-xiaoman-daily-case-report-config-v1 \
  < xiaoman-daily-case-report-production-config-disabled.json
```

The disabled payload carries only `schema_version`, `desired_state: "disabled"`, and the
reviewed release SHA. Then stop the timer:

```bash
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_ROLLBACK=approved-production-xiaoman-daily-case-report-auto-publish-rollback \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/rollback-xiaoman-daily-case-report-auto-publish-production.sh
```

The rollback script disables and stops only
`qintopia-agentos-xiaoman-daily-case-report-auto-publish.timer` and its service.

Never leave both paths enabled. The systemd timer must be disabled before step 4, on the
same day, after the 08:00 systemd run.

## Step 4 - Enable the Hermes job

```bash
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_HERMES_CRON=approved-production-xiaoman-daily-case-report-hermes-cron \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-daily-case-report-hermes-cron.sh --enable
```

`--enable` re-verifies that the installed wrapper still matches the release-local source
and that the live declaration still matches the reviewed name, schedule expression, and
script before flipping `enabled` to `true`. It is idempotent: a second run reports
`daily_case_report_hermes_cron_already_enabled` and writes nothing.

The gateway ticks every minute and reloads `jobs.json`, so no restart is needed.

## Step 5 - Observation

```bash
QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1 \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_OBSERVATION_ENABLE=1 \
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_EXPECTED_STATE=disabled \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-production-observation-smoke.sh
```

The legacy cron smoke must report `status: "reviewed_declarations_only"` with
`reviewed_decl_count` including this job. The daily case-report smoke must report the
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

Complete the cutover before the next 08:00 Asia/Shanghai fire and watch that run:

```bash
sudo -n tail -n 20 \
  /home/ubuntu/.local/state/qintopia-agentos/xiaoman-daily-case-report/hermes-cron.log
```

A healthy run appends one `run=ok` line plus the sanitized worker summary. The worker's
own send evidence (artifact id, request state) is validated through the existing
sanitized evidence chain, not through this log. Any group message authored by this job
itself means the wrapper leaked stdout; disable the job immediately with the rollback
below.

## Rollback

Disable the Hermes job first, then restore the systemd timer only if the daily report is
genuinely needed before the problem is fixed.

Ask the owner to set the Hermes job to disabled through Xiaoman conversation, or run the
reviewed retirement path for the observed `jobs.json`. To restore the systemd timer,
re-apply the enabled production config and re-activate:

```bash
sudo /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-daily-case-report-production-config.py \
  --stdin \
  --apply \
  --approval approved-production-xiaoman-daily-case-report-config-v1 \
  < xiaoman-daily-case-report-production-config.json
QINTOPIA_XIAOMAN_DAILY_CASE_REPORT_AUTO_PUBLISH_PRODUCTION_ACTIVATION=approved-production-xiaoman-daily-case-report-auto-publish \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/activate-xiaoman-daily-case-report-auto-publish-production.sh
```

The Hermes job must be disabled before the systemd timer is re-enabled, otherwise both
paths would run the same worker at 08:00. The worker's artifact idempotency limits the
damage of an accidental overlap, but the one-path rule is absolute.

## Acceptance

- Hermes `jobs.json` owns the schedule, the enablement, and the delivery target.
- Exactly one path is enabled at any time.
- The wrapper stays silent on success and prints one sanitized line on failure.
- The job still renders the daily JPEG, uploads it through the Huabaosi Feishu
  primary-storage boundary, and records the same `generated_image` artifact plus one
  automatic `group_message_request`; the `operations-group-send-ready` chain is
  untouched.
- The wrapper exports both release SHAs derived from `release/current` after sourcing
  the persistent env, and pins `PATH=/usr/bin:/bin`.
- The allowlist observation smoke passes with the reviewed declaration.
- The server-local snapshot repo recorded the change.
- No real group id, chat id, media URI, database URL, or secret appears in any command
  output or evidence.
