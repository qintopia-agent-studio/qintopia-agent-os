---
title: "Deploy the Erhua Livecool provider through a governed profile overlay"
type: "bugfix"
created: "2026-07-21"
status: "done"
review_loop_iteration: 0
baseline_commit: "87c97f85a7502a38364ab0d55289463b88c61558"
context:
  - "{project-root}/docs/engineering/server-change-policy.md"
  - "{project-root}/docs/operations/profile-bundles/m10f-profile-template-plan.md"
  - "{project-root}/docs/operations/release-current-model.md"
---

<!-- markdownlint-disable MD060 -->
<!-- prettier-ignore-start -->

<frozen-after-approval reason="human-owned intent — do not modify unless human renegotiates">

## Intent

**Problem:** The Erhua profile selects `gpt-5.5` through
`custom:livecool.net`, but its profile-local `custom_providers` list does not register
Livecool. Hermes therefore rejects the provider before making an inference call.

**Approach:** Ship a non-secret Erhua model overlay. Extend the fixed deploy runner to
render it against runtime-local config, migrate the existing credential to a
profile-local environment binding, activate atomically, run non-sending checks, and
restore config and secret state on rollback.

## Boundaries & Constraints

**Always:** Manage only the main model and named `Livecool.net` provider; use
`gpt-5.5`, `custom:livecool.net`, `https://livecool.net/v1`, and
`key_env: LIVECOOL_API_KEY`; keep credentials and rendered config outside Git,
artifacts, requests, diffs, and logs; preserve unrelated Erhua config and `.env` values;
require a redacted dry-run; back up config and `.env` as `0600`; pair the fixed scope
only with `hermes-erhua`; verify without inference or QiWe delivery.

**Ask First:** Owner approval is required before live replacement, the non-dry-run
request, and publishing the draft GitHub Release. Return to the owner before broadening
fields, restarting other services, or sourcing a new credential.

**Never:** Hot-edit `.hermes`, copy a profile wholesale, store or print an API key,
accept request-supplied profile paths, alter another Agent, send a test message, call the
model for smoke, or treat restart alone as provider validation.

## I/O & Edge-Case Matrix

| Scenario | Input / State | Expected Output / Behavior | Error Handling |
|----------|---------------|----------------------------|----------------|
| Render | Sanitized base and overlay | Change only managed fields; preserve Ark and all unrelated values | Reject malformed YAML, duplicates, path aliasing, or forbidden fields |
| Dry run | Erhua lacks Livecool; default has one Livecool credential | Emit redacted diff/report; write nothing | Fail closed if any runtime prerequisite is absent |
| Activation | Fixed scope/target after successful dry run | Atomically update config/env, restart Erhua, record checks | Restore runtime files and release on any failure |
| Repeat/rollback | Exact state already exists, or activation fails | Idempotent render, or exact restore plus smoke | Rollback succeeds only when every restore/check passes |

</frozen-after-approval>

<!-- prettier-ignore-end -->
<!-- markdownlint-enable MD060 -->

## Code Map

- `agents/erhua/config.template.yaml` -- non-secret overlay contract.
- `runtime/hermes/` -- renderer, sanitized fixtures, and tests.
- `deploy/runner/` -- fixed-scope activation, smoke, result, and rollback.
- `tools/deploy/` and `.github/workflows/deploy-production.yml` -- artifact and request
  gates.
- `docs/operations/profile-bundles/` -- two-stage runbook and evidence contract.

## Tasks & Acceptance

**Execution:**

- [x] `docs/operations/profile-bundles/` and package docs -- document overlay lifecycle,
      two-stage bootstrap, secret migration, evidence, and rollback first.
- [x] `agents/erhua/config.template.yaml` -- define the exact managed fields and
      runtime-only credential binding without embedding a secret.
- [x] `runtime/hermes/` -- add a Python/PyYAML renderer with redacted reporting, atomic
      `0600` output, fixtures, and tests.
- [x] `deploy/runner/`, `tools/deploy/`, and the production workflow -- add fixed
      scope/target coupling, server-only credential migration, backup, activation,
      non-sending smoke, complete rollback, packaging, and checks.

**Acceptance Criteria:**

- Given sanitized fixtures, when render runs twice, then output is idempotent, `0600`,
  changes only managed fields, and leaks no secret.
- Given `hermes-profile-erhua`, when request validation runs, then only `[hermes-erhua]`
  is accepted and no arbitrary path can enter the request.
- Given missing, duplicate, or conflicting state, when dry-run runs, then it fails
  before changing release, config, `.env`, or service.
- Given approved activation, when smoke runs, then Hermes resolves the provider, Erhua
  is active, and new logs lack the unknown-provider error without inference or delivery.
- Given activation failure, when rollback runs, then recorded hashes/modes, previous
  release, Erhua service, and smoke are restored before success is reported.
- Given the old runner, when first rollout occurs, then one request upgrades it without
  activation and a second dry-run-reviewed request activates Erhua.

## Spec Change Log

## Design Notes

The release contains an immutable non-secret overlay. The runner renders from current
Erhua config into runtime-local staging, records hashes/backups, then installs
atomically. If `LIVECOOL_API_KEY` is absent, a fixed migration reads only the existing
default-profile Livecool credential and writes the profile env without emitting it.

The old runner does not recognize the new scope. The first request upgrades it using an
existing scope. Profile-only requests then validate against that active release without
moving `current` or `previous`; non-dry-run activation also requires an exact matching
dry-run marker, its request ID, and a maximum marker age of 24 hours. Success requires
Hermes's own provider resolver plus exact activated-file revalidation; activation and
rollback write distinct smoke and restore evidence.

## Verification

**Commands:**

- `pnpm runtime:hermes:check` -- renderer and fixture tests pass.
- `pnpm agents:profile-bundles:check` -- overlay contract excludes live state and
  secrets.
- `pnpm deploy:runner:check` -- schemas, scope/target coupling, activation, smoke, and
  rollback contracts pass.
- `pnpm artifact:deploy-bundle` -- artifact contains reviewed inputs and no rendered
  config or credentials.
- `pnpm secrets:check` -- no secret material enters Git-managed files.
- `pnpm check` -- repository validation passes before PR readiness.
- `pnpm pr:doctor` -- completed PR body and production evidence are ready for review.

## Suggested Review Order

<!-- markdownlint-disable MD036 -->

**Release Intent**

- Start with the governed rollout, approval gates, and rollback boundary.
  [`erhua-livecool-profile-overlay-runbook.md:35`](../../operations/profile-bundles/erhua-livecool-profile-overlay-runbook.md#L35)

**Runner Control Plane**

- Follow the fixed profile-only branch without moving release symlinks.
  [`qintopia-agent-os-deploy-runner:388`](../../../deploy/runner/qintopia-agent-os-deploy-runner#L388)

- Verify request coupling and exact dry-run authorization.
  [`deploy-request.schema.json:125`](../../../deploy/runner/deploy-request.schema.json#L125)

- Check workflow inputs and production gate enforcement.
  [`deploy-production.yml:354`](../../../.github/workflows/deploy-production.yml#L354)

**Profile Transaction**

- Review dry-run markers, drift checks, backups, and atomic activation.
  [`activate-erhua-profile.sh:149`](../../../deploy/runner/activate-erhua-profile.sh#L149)

- Inspect durable backup, ownership preservation, activation, and restore verification.
  [`profile_transaction.py:107`](../../../runtime/hermes/profile_transaction.py#L107)

- Confirm smoke uses Hermes's resolver and revalidates activated files.
  [`smoke-release.sh:92`](../../../deploy/runner/smoke-release.sh#L92)

- Review the affirmative installed-runtime provider contract.
  [`verify_runtime_provider.py:19`](../../../runtime/hermes/verify_runtime_provider.py#L19)

**Contract And Tests**

- Confirm the exact non-secret Erhua model overlay.
  [`config.template.yaml:1`](../../../agents/erhua/config.template.yaml#L1)

- Finish with renderer, migration, transaction, rollback, and smoke coverage.
  [`test_profile_overlay.py:29`](../../../runtime/hermes/tests/test_profile_overlay.py#L29)

<!-- markdownlint-enable MD036 -->
