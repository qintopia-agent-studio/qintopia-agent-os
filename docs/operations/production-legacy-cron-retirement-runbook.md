# Production Legacy Cron Retirement Runbook

Updated: 2026-08-09

This runbook retires reviewed legacy Hermes cron files through a signed deploy-runner
request. It exists so cutover from Hermes cron to release-managed systemd timers can be
operated through GitHub Actions plus the production COS pull runner, not through ad-hoc
SSH commands.

## Workflow

Use the `Retire Production Legacy Crons` workflow from `master`.

Required inputs:

```text
release_sha=<current production release SHA>
legacy_cron_retirement_targets=<comma-separated fixed targets>
```

Allowed targets:

```text
erhua-legacy-cron
xiaoman-legacy-cron
```

The workflow creates a signed deploy request with
`release_scope=["production-legacy-cron-retirement"]`. The production runner accepts
only that fixed scope and the fixed target enum above. It does not execute
caller-provided shell.

## Preconditions

- `release_sha` must match the production `release/current` manifest.
- The release must contain the reviewed retirement and observation scripts for each
  selected target.
- Retirement is explicit. Timer activation requests do not retire legacy cron files, and
  observation failure does not trigger retirement as an automatic side effect.
- The request sets `rollback_on_smoke_failure=false`. A retired legacy cron file is
  backed up by the target-specific retirement script; any restoration requires a
  separate reviewed decision.

## Evidence

The server result is written to the normal production deploy-result location and
includes a `production-legacy-cron-retirement` check. Its detail records each requested
target as `passed` or `failed`.

Acceptance requires:

- The workflow run succeeds.
- The server deploy result has `status=succeeded`.
- The `production-legacy-cron-retirement` check is present and passed.
- Each selected target appears in the check detail with `status=passed`.
