# v0.2.59 Legacy Deploy-Runner Bootstrap

Date: 2026-07-31

## Observed Evidence

Published Release `v0.2.59` points to `24035531b27bd2aa317d43b489b59e8fb1b43772`.
Production deploy workflow run `30616180155` reached the existing server runner, which
rejected the new primary Huabaosi artifact before promotion:

```text
artifact manifest Cargo features are not approved for production
```

The result recorded `promoted_current=false`; Hermes activation did not run and no
rollback was needed. Production therefore remained on `v0.2.58` at
`045bd2114114578b61c8be10510768ef2b563adb`.

A follow-up deploy-bundle-only dry run, workflow run `30617406250`, failed earlier in
the GitHub-side COS read validation. The current repository fetcher correctly rejected
the deployed two-feature Huabaosi artifact because its normal contract now requires the
three-feature artifact. No server deploy request was created by that run.

## Root Cause

The production runner and the release artifact contract changed atomically in the same
Release. The old runner cannot install the deploy bundle that teaches it the new
contract because both the server runner and the GitHub-side preflight validate the
runtime artifact before the deploy-bundle-only transition can begin.

Increasing a timeout or rerunning either failed request cannot resolve this contract
deadlock.

## Resolution

Add one default-disabled `legacy_runner_bootstrap` workflow mode. It:

- binds `commit_sha` and `runtime_sha` to the latest trusted successful production
  Release deploy result;
- requires the `huabaosi-production` runtime profile, exactly `deploy-bundle` scope,
  exactly `qintopia-system-services`, and rollback enabled;
- requires distinct runtime, deploy-bundle, and transition release identities;
- permits the historical two-feature Huabaosi artifact only under an explicit bootstrap
  feature contract with an exact runtime SHA binding; and
- leaves ordinary artifact validation on the current three-feature contract.

The transition installs only the reviewed deploy bundle while retaining the currently
deployed runtime. The target Release must then pass its own normal dry run and
deployment.

## Validation

The remediation is covered by focused validator and COS fetcher tests, deploy-runner
contract checks, shell syntax validation, workflow YAML parsing, repository formatting,
Markdown linting, and the risk-tiered local PR gate. The PR evidence records the exact
commands and outcomes.

The negative cases prove that stale runtime selection, broader scope or restart targets,
QiWe/staging profiles, disabled rollback, colliding identities, missing SHA binding, and
ordinary legacy-artifact fetches fail closed.

## Remaining Safety Boundary

This change does not merge, publish, deploy, restart services, modify production data,
activate Hermes, or send Feishu/QiWe messages. Bootstrap remains an explicit production
environment workflow input and must first run with `dry_run=true`.

After this PR is merged, the owner must separately approve the exact dry-run parameters
and review the sanitized result before any live bootstrap. After a successful live
bootstrap, rerun the `v0.2.59` deployment through the normal Release path; do not reuse
the transition release as the new runtime and do not broaden its restart set.
