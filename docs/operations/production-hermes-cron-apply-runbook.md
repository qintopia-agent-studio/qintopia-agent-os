# Production Hermes Cron Apply Runbook

Updated: 2026-08-11

This runbook writes reviewed Hermes cron declarations into the live production
`jobs.json` files through a signed deploy-runner request. It exists so the final
repository-to-live step uses GitHub Actions plus the production COS pull runner instead
of ad-hoc SSH commands.

## Workflow

Use the `Apply Production Hermes Crons` workflow from `master`.

Required inputs:

```text
release_sha=<current production release SHA>
apply_mode=install|enable
hermes_cron_apply_targets=<comma-separated fixed targets>
```

Allowed targets:

```text
erhua-morning-brief
xiaoman-daily-case-report
xiaoman-weekly-recruitment
xiaoman-weekly-plan-confirmation
xiaoman-weekly-preview
```

The workflow creates a signed deploy request with
`release_scope=["production-hermes-cron-apply"]`. The production runner accepts only
that fixed scope, the fixed target enum above, and `apply_mode` of `install` or
`enable`. It does not execute caller-provided shell.

## Boundary

This is a production write to live Hermes state:

- `install` runs the target apply script with its fixed owner approval string. The apply
  script installs the reviewed wrapper into `/home/ubuntu/.hermes/scripts/`, writes or
  preserves the reviewed job in the live profile `cron/jobs.json` as disabled, backs up
  before writing, and runs the server-local snapshot sync.
- `enable` reruns the same apply script with `--enable`. The apply script must first
  prove the live declaration still matches the reviewed name, schedule, script, delivery
  mode, origin platform, and resolved chat id before flipping `enabled`.
- The request never accepts chat ids, cron JSON, script paths, approval strings, env
  files, systemctl commands, or arbitrary shell from workflow inputs.
- The deploy result records only target, mode, status, fixed failure exit code, and a
  bounded failure reason when the apply script emits the explicit
  `qintopia_hermes_cron_apply_safe_failure=` marker. It does not record raw script
  stdout/stderr, live `jobs.json`, group ids, prompts, env values, or snapshot contents.

The approval string authorizes the production action and boundary commitment. It is not
a cryptographic signature over `jobs.json`.

## Recommended Sequence

1. Publish and deploy the release that contains the reviewed templates, wrappers, apply
   scripts, registry, runbooks, and runner support.
2. Run this workflow with `apply_mode=install` for the selected reviewed targets. This
   should leave all newly installed jobs disabled.
3. Use production observation or an owner-reviewed server-local inspection to confirm
   the live declarations exist and no unreviewed drift is present.
4. Run this workflow again with `apply_mode=enable`, preferably one target or one small
   profile group at a time.
5. After the first scheduled run, use `Observe Production Runtime` worker-run targets to
   confirm the reviewed Hermes wrapper wrote `run=ok` evidence.

## Preconditions

- `release_sha` must match production `release/current`.
- The release must contain the target apply scripts and reviewed Hermes cron templates.
- The production deploy-runner unit must keep `ProtectHome=read-only` while granting
  `ReadWritePaths` only to:
  - `/home/ubuntu/.hermes/profiles/erhua`
  - `/home/ubuntu/.hermes/profiles/xiaoman/cron`
  - `/home/ubuntu/.hermes/scripts`
- The snapshot sync path must remain server-local and must not have a remote.

## Evidence

The server result is written to the normal production deploy-result location and
includes a `production-hermes-cron-apply` check. Its detail records each requested
target as `passed` or `failed` with the requested mode.

Acceptance requires:

- The workflow run succeeds.
- The server deploy result has `status=succeeded`.
- The `production-hermes-cron-apply` check is present and passed.
- Each selected target appears in the check detail with the requested mode and
  `status=passed`.
