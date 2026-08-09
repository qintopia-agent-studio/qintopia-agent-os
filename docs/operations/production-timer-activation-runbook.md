# Production Timer Activation Runbook

Updated: 2026-08-09

This runbook activates reviewed production timers through a signed deploy-runner
request. It exists so timer enablement can be operated and evidenced through GitHub
Actions plus the production COS pull runner, not through ad-hoc SSH commands.

## Workflow

Use the `Activate Production Timers` workflow from `master`.

Required inputs:

```text
release_sha=<current production release SHA>
activation_targets=<comma-separated fixed targets>
```

Allowed targets:

```text
erhua-morning-brief
xiaoman-weekly-preview
xiaoman-daily-case-report-auto-publish
```

The workflow creates a signed deploy request with
`release_scope=["production-activation"]`. The production runner accepts only that fixed
scope and the fixed target enum above. It does not execute caller-provided shell.

## Preconditions

- `release_sha` must match the production `release/current` manifest.
- The release must contain the activation, observation, rollback, and config scripts for
  the selected target.
- `erhua-morning-brief` observes only Erhua legacy Hermes cron state before enabling the
  replacement timer. `xiaoman-weekly-preview` observes only Xiaoman legacy Hermes cron
  state. If legacy-cron observation fails, the activation request fails closed; it does
  not automatically retire legacy cron files.
- The activation request sets `rollback_on_smoke_failure=false`. Use the dedicated
  target rollback runbooks/scripts for a separate reviewed rollback decision.
- Every selected target requires the owner-approved production config to have already
  been applied. The activation request does not carry chat ids, media URLs, database
  URLs, or other runtime values, and it does not write persistent production config.

## Evidence

The server result is written to the normal production deploy-result location and
includes a `production-timer-activation` check. Its detail records each requested target
as `passed` or `failed`.

Acceptance requires:

- The workflow run succeeds.
- The server deploy result has `status=succeeded`.
- The `production-timer-activation` check is present and passed.
- Each selected target appears in the check detail with `status=passed`.
