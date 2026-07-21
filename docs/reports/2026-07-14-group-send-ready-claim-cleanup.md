# Group Send-Ready Claim Cleanup

Date: 2026-07-14

## Current State

The Rust `group_message_send` worker claims a queued, final-confirmed group message by
setting `claimed_by`, `locked_at`, and `claim_expires_at` in one transaction. Its
send-ready success and policy-denied paths clear the lock timestamps before committing
their audit events, but leave `claimed_by=group-message-send-worker` behind.

The later QiWe staging adapter can replace that value when it claims an eligible item,
so this does not prove that an external send failed. It does leave Postgres with a
contradictory internal state: the work item has no active lease but still reports a
claim owner. The existing disposable PostgreSQL test asserted status, attempts,
idempotent events, and no external send, but did not inspect complete claim cleanup.

## Root Cause

The first send-readiness implementation treated the two lease timestamps as the entire
claim state. The shared work-item model uses all three fields as one claim tuple, and
other recovery paths clear `claimed_by` together with the timestamps.

The transition updates also ignored `rows_affected`, so an unexpected zero-row update
could proceed to audit insertion instead of failing the transaction closed.

## Resolution

Clear `claimed_by`, `locked_at`, and `claim_expires_at` together on send-ready success
and policy denial. Require exactly one affected work-item row before appending the
corresponding audit event. Extend the guarded PostgreSQL integration test to assert the
complete claim tuple is null for both states.

## Validation

Focused `group_message_send` tests passed `5/5` in both default and all-feature builds.
The complete default suite passed `363/363`; the all-feature suite passed `359/359`,
with eight guarded PostgreSQL integration tests skipped by design. Warning-denied Clippy
passed for both `--no-default-features` and `--all-features`.

Repository pre-commit formatting, Markdown, workflow, deploy, and Xiaoman readiness
checks passed, as did anti-drift policy, secret, runtime-contract, CI-contract, and
Cargo advisory, ban, and source checks. Cargo deny reported only the existing
duplicate-version warnings.

The feature-gated PostgreSQL test compiled locally. Its database execution remains
restricted to the disposable loopback database named exactly `qintopia_test`; a green
non-database suite does not substitute for that gate. GitHub Actions run `29332192397`
executed the test against that service on the prior rebased head and passed. The current
head still requires its own replacement database gate before merge.

## Production Boundary

This change updates internal AgentOS Postgres state only when the existing send-ready
worker runs. It adds no migration, external adapter, listener, service, timer, runtime
configuration, Feishu write, image generation, media upload, QiWe call, or message send.
The worker continues to record `send_executed=false`; rollback is the previous sidecar
release because the schema is unchanged.
