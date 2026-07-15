# Huabaosi Stale Image-Generation Claim Ambiguity

Date: 2026-07-15

## Current State

The Huabaosi image-generation worker claims an eligible request as `processing` before
calling the image provider and media service. A later worker run currently treats an
expired `processing` lease as claimable, replaces its claim token, increments the
attempt count, and starts the external path again.

## Root Cause

Lease expiry proves only that local ownership ended. It does not prove that a provider
generation request or media upload stayed local. A crash after remote acceptance but
before artifact persistence therefore turns the current automatic reclaim into a
duplicate-generation and duplicate-upload risk.

The retry policy is otherwise bounded to explicit recoverable provider errors, but the
claim query bypasses that classification by treating process loss as an ordinary retry.

## Resolution

Before selecting queued work, reconcile one expired or structurally incomplete
`processing` claim in the same database transaction. Mark the request `failed`, clear
the complete claim tuple, and append one sanitized `image_generation_outcome_ambiguous`
event with automatic retry disabled. Select new work from `queued` requests only;
dry-run preview must not present stale processing work as claimable.

## Validation

The feature-gated Rust/PostgreSQL integration test targets only the disposable loopback
database named exactly `qintopia_test`. It asserts that an expired processing request is
terminalized once, cannot be reclaimed, keeps its attempt count, creates no artifact,
releases all claim fields, and stores no prior claim token or external data in the
event.

Focused image-generation tests passed `40/40` with default features and `40/40` with all
features. The complete default suite passed `366/366`; the all-feature suite passed
`363/363`, with nine guarded PostgreSQL tests skipped by design. Warning-denied Clippy
passed for both no-default-feature and all-feature builds. Repository pre-commit,
anti-drift, secret, runtime-contract, and CI-contract checks passed. Cargo deny passed
advisories, bans, and sources after retrying the advisory fetch through the configured
proxy; it reported only existing duplicate-version warnings.

All-feature `cargo llvm-cov nextest` passed with `43.70%` total line coverage and
`53.86%` line coverage for `image_generation.rs`. The ignored PostgreSQL branch is not
included in that local line coverage and still requires the CI database execution.

The first local execution connected to loopback `qintopia_test` but stopped before the
test fixture because the Homebrew PostgreSQL server does not provide the required
`vector` extension. The database had no AgentOS tables, so repository code and the new
state transition did not execute. The extension requirement is unchanged; the
authoritative database execution remains the CI job's disposable
`pgvector/pgvector:pg16` service.

PR review found that the initial integration guard also accepted `localhost`. That still
depended on host-name resolution for a migration-and-write test. The guard now accepts
only literal `127.0.0.1` or `[::1]` plus the exact `qintopia_test` database name; a
feature-gated Rust unit test rejects `localhost`, non-loopback hosts, and other database
names before any connection.

The first IPv6 positive assertion exposed that the pinned `url` crate serializes an IPv6
`host_str()` with brackets. The validator now matches the crate's documented literal
forms, `127.0.0.1` and `[::1]`, while the connection URL remains standard
`postgres://...@[::1]/qintopia_test` syntax.

## Production Boundary

This change writes only internal AgentOS work-item and audit state when a future enabled
Huabaosi worker encounters an abandoned claim. It adds no migration, timer, provider or
media request, generated artifact, Feishu write, QiWe send, production configuration, or
server change. Rollback is the previous sidecar release; the schema is unchanged.
