# Erhua Morning Brief Hermes Cron Runbook

Updated: 2026-08-10

This runbook migrates the daily 08:10 Erhua morning brief from the release-managed
systemd timer back to an Erhua Hermes cron job. The worker and the reviewed auto-publish
chain stay unchanged; Hermes now owns schedule, enablement, and delivery target.

## Reviewed Assets

- `runtime/hermes/scripts/qintopia_erhua_morning_brief.sh`
- `runtime/hermes/cron/erhua/morning-brief.job.json`
- `runtime/hermes/cron/reviewed-cron-jobs.json` (registry entry for this job)
- `deploy/sidecar/scripts/apply-erhua-morning-brief-hermes-cron.sh`
- `deploy/sidecar/scripts/sync-hermes-cron-snapshot.sh`
- `deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh`

The reviewed declaration is fixed:

```text
name: 二花·每日早报
schedule expr: 10 8 * * *
script: qintopia_erhua_morning_brief.sh
no_agent: true
deliver: origin
```

The apply script resolves the real WeCom home channel from
`/home/ubuntu/.hermes/profiles/erhua/.env` (`WECOM_HOME_CHANNEL`), never prints it, and
emits only its SHA-256 in evidence. It also preserves the Erhua `jobs.json` envelope's
`updated_at` field.

## Preconditions

- A reviewed Release containing the assets above is deployed and `release/current`
  points at it.
- The reviewed Erhua profile overlay has been activated through the
  `hermes-profile-erhua` deploy-runner path so
  `/home/ubuntu/.hermes/profiles/erhua/config.yaml` has `channel.wecom.enabled=true`.
  The profile evidence may report only changed paths and hashes, never channel
  credentials or runtime field values.
- The existing auto-publish chain boundaries are present in
  `/etc/qintopia/message-sidecar.env`:
  `QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED=1`,
  `QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_APPROVAL=approved-production-erhua-morning-brief-auto-publish`,
  `QINTOPIA_QIWE_TEXT_SEND_ENABLED=1`, and
  `QINTOPIA_QIWE_TEXT_SEND_PRODUCTION_APPROVAL=approved-production-qiwe-text-send`.
- After Hermes cutover, the wrapper exports `QINTOPIA_ERHUA_MORNING_BRIEF_ENABLED=1` and
  `QINTOPIA_ERHUA_MORNING_BRIEF_AUTO_PUBLISH_ENABLED=1` only for that worker process;
  the retired systemd path may stay disabled in persistent env.
- The systemd timer is still the active scheduler until step 4.

## Cutover Steps

The whole cutover must happen inside one day after the 08:10 run and before midnight, so
the next morning has exactly one scheduler enabled.

1. Install the wrapper and insert the disabled Hermes job:

   ```bash
   QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON=approved-production-erhua-morning-brief-hermes-cron \
     /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-erhua-morning-brief-hermes-cron.sh \
     --install
   ```

2. Manually run the wrapper once:

   ```bash
   /home/ubuntu/.hermes/profiles/erhua/scripts/qintopia_erhua_morning_brief.sh
   ```

   Compare the produced brief artifact and the send-ready evidence against the most
   recent systemd run (the 2026-08-10 09:31 one-shot is the reference). The manual run
   must not create a second send: confirm the send-ready evidence before step 4 so the
   auto-publish chain does not send a duplicate brief.

3. Disable the release-managed timer with the reviewed rollback path:

   ```bash
   QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_CONFIG=approved-production-erhua-morning-brief-config \
     /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-erhua-morning-brief-production-config.sh \
     --disable
   QINTOPIA_ERHUA_MORNING_BRIEF_PRODUCTION_ROLLBACK=approved-production-erhua-morning-brief-rollback \
     /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/rollback-erhua-morning-brief-production.sh
   ```

4. Enable the Hermes job:

   ```bash
   QINTOPIA_ERHUA_MORNING_BRIEF_HERMES_CRON=approved-production-erhua-morning-brief-hermes-cron \
     /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-erhua-morning-brief-hermes-cron.sh \
     --enable
   ```

5. Verify the reviewed-declaration health signal:

   ```bash
   QINTOPIA_ERHUA_LEGACY_CRON_OBSERVATION_ENABLE=1 \
     /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/erhua-legacy-cron-observation-smoke.sh
   ```

   It must report `reviewed_declarations_only` with the morning-brief job counted. Then
   check the server-local snapshot git repo recorded the change:

   ```bash
   git -C /home/ubuntu/.local/state/qintopia-agentos/hermes-cron-snapshot log --oneline -3
   ```

6. Watch the next natural fire at 08:10 and the group arrival of the brief. The worker
   log is
   `/home/ubuntu/.local/state/qintopia-agentos/erhua-morning-brief/hermes-cron.log`.

## Rollback to Systemd Timer

Disable the Hermes job through a future reviewed disable mode or a conversation edit,
then re-enable the systemd timer using the
`erhua-morning-brief-production-activation-runbook.md` activation path. Do not enable
both schedulers at any point.

## Forbidden

- Never enable the Hermes job and the systemd timer at the same time.
- Never hand-edit `/home/ubuntu/.hermes/profiles/erhua/cron/jobs.json`; only the apply
  script writes it.
- Never resurrect the retired prompt-style jobs; this migration is deterministic
  `no_agent` script form only.
- Never make the Hermes job call QiWe directly or generalize the
  `run-qiwe-text-send-worker` path; external sending stays on the reviewed auto-publish
  chain with exact content-hash binding.
