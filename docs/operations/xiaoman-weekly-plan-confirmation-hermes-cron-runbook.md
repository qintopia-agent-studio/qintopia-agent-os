# Xiaoman Weekly Plan Confirmation Hermes Cron Runbook

Updated: 2026-08-11

This runbook migrates the Sunday 20:00 plan-confirmation timer from the release-managed
systemd timer back to a Xiaoman Hermes cron job. The worker and its persistent runtime
values stay unchanged; Hermes now owns schedule, enablement, and delivery target. The
job produces the same operations-review draft only and never sends, publishes, writes
Feishu, calls Erhua, or calls QiWe.

## Reviewed Assets

- `runtime/hermes/scripts/qintopia_xiaoman_weekly_plan_confirmation.sh`
- `runtime/hermes/cron/xiaoman/weekly-plan-confirmation.job.json`
- `runtime/hermes/cron/reviewed-cron-jobs.json` (registry entry for this job)
- `deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh`
- `deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh`
- `deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh`

The reviewed declaration is fixed:

```text
name: 小满·周日活动计划确认
schedule expr: 0 20 * * 0
script: qintopia_xiaoman_weekly_plan_confirmation.sh
no_agent: true
deliver: origin
```

The apply script resolves the real WeCom home channel from
`/home/ubuntu/.hermes/profiles/xiaoman/.env` (`WECOM_HOME_CHANNEL`), never prints it,
and emits only its SHA-256 in evidence.

## Preconditions

- A reviewed Release containing the assets above is deployed and `release/current`
  points at it.
- Before rollback, the existing persistent config is enabled:
  `QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_ENABLED=1` plus
  `QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_APPROVAL=approved-production-xiaoman-weekly-plan-confirmation`
  and the three Xiaoman activity flags in `/etc/qintopia/message-sidecar.env`.
- The Sunday systemd timer is still the active scheduler until step 4.

After step 3, `/etc/qintopia/message-sidecar.env` must keep
`QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_ENABLED=0` so the retired systemd path stays
inert. The Hermes wrapper exports `QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_ENABLED=1`
itself, clears the worker's runtime path override group, and rebinds
`QINTOPIA_DEPLOYED_COMMIT_SHA` from `release/current` after sourcing the persistent env.
It also executes the worker from that same resolved release directory, not from the
mutable `release/current` symlink path.

## Cutover Steps

1. Install the wrapper and insert the disabled Hermes job:

   ```bash
   QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON=approved-production-xiaoman-weekly-plan-confirmation-hermes-cron \
     /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh \
     --install
   ```

2. Manually run the wrapper once:

   ```bash
   /home/ubuntu/.hermes/scripts/qintopia_xiaoman_weekly_plan_confirmation.sh
   ```

   Compare `latest-operator-review-message.txt` and `latest-summary.json` under
   `/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-plan-confirmation/`
   against the 2026-08-09 systemd run. Compare structure and work-item types, not exact
   text (the plan table may have shifted).

3. Disable the release-managed timer with the reviewed rollback script:

   ```bash
   QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_CONFIG=approved-production-xiaoman-weekly-plan-confirmation-config \
     /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-production-config.sh \
     --disable
   QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_ROLLBACK=approved-production-xiaoman-weekly-plan-confirmation-rollback \
     /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/rollback-xiaoman-weekly-plan-confirmation-production.sh
   ```

4. Enable the Hermes job:

   ```bash
   QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_HERMES_CRON=approved-production-xiaoman-weekly-plan-confirmation-hermes-cron \
     /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-hermes-cron.sh \
     --enable
   ```

5. Verify the reviewed-declaration health signal:

   ```bash
   QINTOPIA_XIAOMAN_LEGACY_CRON_OBSERVATION_ENABLE=1 \
     /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh
   ```

   It must report `reviewed_declarations_only` with the plan-confirmation job counted.
   Then check the server-local snapshot git repo recorded the change:

   ```bash
   git -C /home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot log --oneline -3
   ```

6. Watch the next natural fire at Sunday 20:00. The worker log is
   `/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-plan-confirmation/hermes-cron.log`.

## Rollback to Systemd Timer

Disable the Hermes job by editing the job through Xiaoman conversation or by running the
apply script's reviewed replacement with `enabled: false` through a future reviewed
disable mode. Then re-enable the systemd timer using the
`xiaoman-weekly-loop-cutover-runbook.md` activation path.

## Forbidden

- Never enable the Hermes job and the systemd timer at the same time.
- Never hand-edit `/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json`; only the apply
  script writes it.
- Never resurrect the retired prompt-style jobs; this migration is deterministic
  `no_agent` script form only.
