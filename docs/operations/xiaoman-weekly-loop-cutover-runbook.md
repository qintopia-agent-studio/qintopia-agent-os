# Xiaoman Weekly Loop Production Cutover Runbook

Updated: 2026-08-09

> **Status: recruitment half is rollback-only.** Since 2026-08-10 the Saturday 10:00
> recruitment recurrence lives in the Xiaoman Hermes cron and is governed by
> `docs/operations/xiaoman-weekly-recruitment-hermes-cron-runbook.md`. Use this document
> only to restore the recruitment systemd timer during a rollback. The Sunday 20:00 plan
> confirmation remains a release-managed systemd timer (a later task migrates it).

This runbook activates the Saturday recruitment and Sunday plan-confirmation timers for
the Xiaoman weekly activity loop. The recruitment systemd activation below is the
rollback path; the recruitment Hermes cron is the live path. Both timers produce
operations-review drafts only; they do not send, publish, write Feishu, call Erhua, or
call QiWe.

## Production Boundary

On 2026-08-09 the owner approved adding `xiaoman-weekly-recruitment` and
`xiaoman-weekly-plan-confirmation` to the fixed `production-activation` target allowlist
so the weekly minimum loop can be enabled through the reviewed GitHub Actions plus
deploy-runner path. This expands only the selectable timer targets; it does not allow
caller-provided shell, runtime values, automatic legacy cron retirement, automatic
rollback, direct sending, Feishu writes, Erhua handoff, or QiWe calls.

## Timers

| Step                 | Unit                                                      | Schedule       |
| -------------------- | --------------------------------------------------------- | -------------- |
| Resident recruitment | `qintopia-agentos-xiaoman-weekly-recruitment.timer`       | Saturday 10:00 |
| Plan confirmation    | `qintopia-agentos-xiaoman-weekly-plan-confirmation.timer` | Sunday 20:00   |

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
- `deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh` passes. Do not
  recreate Hermes `cron/jobs.json` timers after cutover.
- Live form URLs, Feishu table IDs, chat IDs, and secrets remain in runtime state; do
  not commit or paste them into git.

## Apply Config

```bash
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_CONFIG=approved-production-xiaoman-weekly-recruitment-config \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-recruitment-production-config.sh --enable

QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_CONFIG=approved-production-xiaoman-weekly-plan-confirmation-config \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-production-config.sh --enable
```

## Activate

Prefer the `Activate Production Timers` GitHub workflow with targets:

```text
xiaoman-weekly-recruitment,xiaoman-weekly-plan-confirmation,xiaoman-weekly-preview
```

Manual release-local activation, if needed after owner review:

```bash
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_ACTIVATION=approved-production-xiaoman-weekly-recruitment \
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_RELEASE_SHA=<published-production-release-sha> \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/activate-xiaoman-weekly-recruitment-production.sh

QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_ACTIVATION=approved-production-xiaoman-weekly-plan-confirmation \
QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_RELEASE_SHA=<published-production-release-sha> \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/activate-xiaoman-weekly-plan-confirmation-production.sh
```

## Observe

```bash
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_OBSERVATION_ENABLE=1 \
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_EXPECTED_STATE=enabled \
QINTOPIA_XIAOMAN_WEEKLY_RECRUITMENT_PRODUCTION_RELEASE_SHA=<published-production-release-sha> \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-weekly-recruitment-production-observation-smoke.sh

QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_OBSERVATION_ENABLE=1 \
QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_EXPECTED_STATE=enabled \
QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_RELEASE_SHA=<published-production-release-sha> \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-weekly-plan-confirmation-production-observation-smoke.sh
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

QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_CONFIG=approved-production-xiaoman-weekly-plan-confirmation-config \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/apply-xiaoman-weekly-plan-confirmation-production-config.sh --disable

QINTOPIA_XIAOMAN_WEEKLY_PLAN_CONFIRMATION_PRODUCTION_ROLLBACK=approved-production-xiaoman-weekly-plan-confirmation-rollback \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/rollback-xiaoman-weekly-plan-confirmation-production.sh
```
