# Xiaoman QiWe Disabled Observation Artifact Drift

Date: 2026-07-23

## Observed Evidence

Release `v0.2.27` deployed successfully at commit
`2f9ff016bde555e504cb9e0b063f39de6132859a`. The deploy runner promoted that immutable
release without rollback, and the release-local Huabaosi image production observation
passed.

The release-local Xiaoman aggregate production preflight then passed its internal
activity timers, legacy-cron absence, downstream dry runs, Huabaosi image observation,
send-request starter, and group send-ready observation. It failed at the disabled QiWe
image-send observation before any external action:

```text
QiWe image-send production observation requires the immutable release/current sidecar binary with reviewed production adapter features
```

No provider, Postgres write, Feishu write, QiWe call, callback processing, timer
mutation, publication, or send ran during the observation.

## Root Cause

PR #258 correctly restricted the Huabaosi production sidecar artifact to exactly
`huabaosi-production-adapter` and `huabaosi-feishu-mirror-adapter`. The QiWe image-send
and callback-bridge production observation scripts still required that same artifact to
also contain `qiwe-production-adapter`.

That stale assertion rejected the approved Huabaosi artifact and would accept the
forbidden mixed production artifact. It also prevented the Xiaoman aggregate preflight
from proving that QiWe remained disabled.

## Resolution

Both QiWe production observations must accept only the exact Huabaosi production
artifact feature list when inspecting disabled runtime state. Their fixtures and deploy
contract assertions must use the same exact list and explicitly reject
`qiwe-production-adapter`.

The current Huabaosi production artifact cannot support an enabled QiWe worker or
callback bridge. If either persistent enable flag is `1`, the observation must fail
closed instead of reporting production readiness. Enabled QiWe production requires a
separate reviewed artifact and enablement boundary after the required staging evidence;
it must not be restored by mixing the QiWe feature into the Huabaosi artifact.

## Validation Boundary

The repaired aggregate preflight proves only that the release-managed Xiaoman internal
path and Huabaosi observation are healthy while QiWe send and callback processing remain
disabled. It does not prove image generation, human approval, QiWe delivery, or Xiaoman
production completion. Those claims still require the retained evidence in
`docs/plans/active/xiaoman-production-completion-gate.md`.
