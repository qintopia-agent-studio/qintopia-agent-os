# Xiaoman Weekly Loop Production Cutover Runbook

Updated: 2026-08-09

> **Status: rollback-only.** Since 2026-08-10 both weekly recurrences live in Xiaoman
> Hermes cron jobs: Saturday recruitment is governed by
> `docs/operations/xiaoman-weekly-recruitment-hermes-cron-runbook.md` and Sunday plan
> confirmation by
> [`xiaoman-weekly-plan-confirmation-hermes-cron-runbook.md`](xiaoman-weekly-plan-confirmation-hermes-cron-runbook.md).
> Use this document only to restore the systemd timers during a rollback.

This runbook activated the Saturday recruitment and Sunday plan-confirmation systemd
timers for the Xiaoman weekly activity loop. Both systemd activations below are now
rollback paths; the Hermes cron jobs are the live paths. Both timers produced
operations-review drafts only; they did not send, publish, write Feishu, call Erhua, or
call QiWe.

## Production Boundary

On 2026-08-09 the owner approved adding `xiaoman-weekly-recruitment` and
`xiaoman-weekly-plan-confirmation` to the fixed `production-activation` target allowlist
so the weekly minimum loop can be enabled through the reviewed GitHub Actions plus
deploy-runner path. This expands only the selectable timer targets; it does not allow
caller-provided shell, runtime values, automatic legacy cron retirement, automatic
rollback, direct sending, Feishu writes, Erhua handoff, or QiWe calls.

## Timers

| Step                 | Unit                                                | Schedule       |
| -------------------- | --------------------------------------------------- | -------------- |
| Resident recruitment | `qintopia-agentos-xiaoman-weekly-recruitment.timer` | Saturday 10:00 |
| Plan confirmation    | Hermes cron (see migration runbook)                 | Sunday 20:00   |

The Monday confirmed preview remains covered by
`xiaoman-weekly-preview-cutover-runbook.md`.

Both weekly-loop timers use the Xiaoman activity wrapper boundary. Production config,
activation, observation, and workers must require:

```text
QINTOPIA_XIAOMAN_ACTIVITY_WRAPPERS_ENABLE=1
QINTOPIA_XIAOMAN_ACTIVITY_USE_FEISHU_BASE=1
QINTOPIA_XIAOMAN_ACTIVITY_READ_THROUGH_ENABLE=1
```

## Preconditions

- A reviewed Release containing `workflows/xiaoman-weekly-loop` and the scripts below is
  published and deployed to production.
- `release/current` points at the reviewed production release SHA.
- `deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh` passes with only
  reviewed declarations. Do not hand-edit Hermes `cron/jobs.json`; use the reviewed
  apply scripts.
- Live form URLs, Feishu table IDs, chat IDs, and secrets remain in runtime state; do
  not commit or paste them into git.

## Apply Config

```bash
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_CONFIG=approved-production-xiaoman-weekly-recruitment-config \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-production-config.sh --enable
```

Plan confirmation config now follows the Hermes cron runbook, not this section.

## Activate

Prefer the `Activate Production Timers` GitHub workflow with the recruitment target:

```text
xiaoman-weekly-recruitment
```

Manual release-local activation, if needed after owner review:

```bash
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_ACTIVATION=approved-production-xiaoman-weekly-recruitment \
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_RELEASE_SHA=<published-production-release-sha> \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/activate-xiaoman-weekly-recruitment-production.sh
```

## Observe

```bash
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_OBSERVATION_ENABLE=1 \
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_EXPECTED_STATE=enabled \
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_RELEASE_SHA=<published-production-release-sha> \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-weekly-recruitment-production-observation-smoke.sh
```

The worker output is local operator state:

```text
/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment/latest-operator-review-message.txt
/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-recruitment/latest-summary.json
/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-plan-confirmation/latest-operator-review-message.txt
/home/ubuntu/.local/state/qintopia-agentos/xiaoman-weekly-plan-confirmation/latest-summary.json
```

## Rollback

Disable persistent config first, then stop the timer:

```bash
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_CONFIG=approved-production-xiaoman-weekly-recruitment-config \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-production-config.sh --disable

QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_ROLLBACK=approved-production-xiaoman-weekly-recruitment-rollback \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/rollback-xiaoman-weekly-recruitment-production.sh
```

Plan confirmation rollback now follows the Hermes cron runbook.
