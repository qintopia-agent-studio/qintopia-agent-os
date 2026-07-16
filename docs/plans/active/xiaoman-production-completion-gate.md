# Xiaoman Production Completion Gate

Updated: 2026-07-16

## Goal

Prevent an infrastructure-only Release from being treated as a usable Xiaoman production
completion. A Release may still ship reviewed staging, deploy, or activation tooling,
but it must not be described as "Xiaoman production complete" until the real external
workflow has passed the gates below.

## Current Boundary

The AgentOS-only Xiaoman orchestration path is production schedulable through internal
send-ready state. It is not a complete business workflow while Huabaosi real image
generation, Feishu mirror activation, QiWe final delivery, and one real end-to-end
activity remain unproven.

Release `v0.2.12` is currently an infrastructure/provisioning candidate unless later
changes add and prove the missing external send boundary. Publishing it may be useful
for staging or deployment prerequisites, but it must not be used as the completion claim
for Xiaoman production.

## Release Classification

Classify each Xiaoman-adjacent Release before merging its Release Please PR:

- `infrastructure`: release, deploy, staging, provisioning, smoke, or guardrail changes
  that do not make the full production workflow usable.
- `activation-ready`: production artifacts and activation scripts are present, but the
  external runtime remains disabled until owner-provided configuration and evidence
  pass.
- `production-complete`: all gates in [Completion Gates](#completion-gates) are
  satisfied and the Release notes identify the retained evidence.

Only `production-complete` may be described as Xiaoman fully usable in production.

## Completion Gates

All gates are required before a Release is called Xiaoman production complete:

1. Release Please validation passes on the exact release PR head, including the manual
   CI dispatch required for bot-authored release PRs.
2. The staging sidecar artifact is built and provisioned under the fixed immutable
   staging release root with reviewed release SHA, sidecar SHA-256, and rollback owner.
3. Huabaosi staging generation produces one reviewed final JPEG and retains only the
   sanitized evidence allowed by the staging template.
4. QiWe staging preflight, upload, and callback/send phases pass against one isolated
   send-ready work item and one trusted memory-only callback stream.
5. The cross-flow evidence checker proves the Huabaosi final JPEG `content_hash` equals
   the QiWe `artifact_content_hash`.
6. A separate QiWe production enablement PR adds reviewed listener/service/timer,
   observation, rollback, exact allowlists, and production feature boundaries.
7. Huabaosi production generation and Feishu mirror activation pass release-local
   observation, explicit activation, and first-record evidence.
8. One real Xiaoman activity is observed from signal intake through image generation,
   human approval, send-ready, QiWe group-send arrival, and sanitized production
   evidence retention.

## Non-Completion Cases

These are useful but not completion:

- local fake-server, fake-sidecar, disposable PostgreSQL, or CI-only validation;
- staging artifact builder or staging provisioner changes without a server-side
  owner-approved staging run;
- Huabaosi production units installed but timer disabled;
- Feishu mirror units installed but timer disabled or first-record evidence missing;
- QiWe staging evidence without a reviewed production enablement PR;
- a Release that deploys internal timers but still leaves QiWe delivery disabled.

## Next Work

1. Keep Release `v0.2.12` classified as infrastructure unless its PR body and release
   notes explicitly remain within that boundary.
2. Use the staging artifact builder and provisioner to prepare the fixed staging
   runtime.
3. Run Huabaosi and QiWe staging evidence smokes, then the cross-flow checker.
4. Add QiWe production enablement in a separate reviewed PR.
5. Process one real production Xiaoman activity and retain sanitized evidence before
   changing the completion classification.

## Production Boundary

This plan changes release decision criteria only. It does not publish a Release, deploy
to production, install or enable timers, write Postgres or Feishu, call providers or
QiWe, process callbacks, or send externally.
