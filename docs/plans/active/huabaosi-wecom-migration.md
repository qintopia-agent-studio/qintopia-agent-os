# Huabaosi WeCom Migration Plan

Status: phase 3 Rust shadow capture in progress; no production behavior changed

Scope: 阿亮画报师 / Huabaosi WeCom conversation gateway migration into this
monorepo-managed Agent OS release flow.

## Why This Exists

阿亮画报师 currently has an active production WeCom Bot path that is still owned by live
Hermes runtime state rather than by a reviewed package in this repository. That keeps
real user behavior split from the Rust sidecar, release/current deployment model, and
CI-reviewed migration process.

This plan starts the migration without replacing the production Bot. Each phase must be
a separate PR with its own validation and rollback notes.

## Current Production Path

Read-only production diagnosis on 2026-07-14 found the user-facing path:

```text
WeCom user
  -> hermes-gateway-huabaosi.service
  -> /home/ubuntu/.hermes/hermes-agent
  -> /home/ubuntu/.hermes/profiles/huabaosi
  -> Hermes gateway/platforms/wecom.py
  -> Huabaosi profile tools and image workspace
```

The current service runs:

```text
/home/ubuntu/.hermes/hermes-agent/venv/bin/python -m hermes_cli.main --profile huabaosi gateway run --replace
```

This path is production live state. It may be observed read-only, but it must not be
hot-edited.

## Target Direction

The target is not to rewrite everything at once. The target is to move the production
contract into this repository in layers:

```text
WeCom user
  -> reviewed Huabaosi WeCom ingress contract
  -> Rust sidecar shadow/audit path
  -> reviewed gateway policy preview
  -> allowlisted canary sender
  -> production sender with rollback to Hermes
```

Hermes may remain the Agent runtime during the transition. The migration goal is that
the user-visible channel boundary, safety filters, audit facts, release artifacts,
smokes, and rollback process are owned by this repository before production cutover.

## Phase PRs

### PR 1: Document The Migration Boundary

Deliverables:

- this plan;
- a production incident/evidence report for the busy-ack leak;
- change-routing and roadmap updates.

Production boundary:

- no server writes;
- no code behavior changes;
- no WeCom, Feishu, QiWe, provider, or database writes.

Validation:

- Markdown lint for touched docs;
- `git diff --check`.

### PR 2: Add Read-Only Huabaosi WeCom Observation Smoke

Deliverables:

- `deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh`, which only
  inspects:
  - `hermes-gateway-huabaosi.service` active state;
  - fixed service command shape;
  - public busy mode keys;
  - sanitized journal markers for internal-process filtering, send fallback, and API
    timeout counts;
  - release/current presence;
- docs describing the smoke and the exact forbidden actions.
- deploy-bundle and contract checks so the smoke reaches release/current without
  weakening the boundary.

Forbidden:

- do not restart services;
- do not read `.env` values;
- do not print tokens, WeCom IDs, user messages, image paths, prompt content, or raw
  stack traces;
- do not send WeCom messages or run generation.

Validation:

- shell syntax check;
- fixture or local dry-run test where possible;
- deploy contract and runner checks;
- one owner-approved read-only production run before using results in later PRs.

### PR 3: Capture Huabaosi WeCom Events Into A Rust Shadow Path

Deliverables:

- `huabaosi-wecom-shadow-capture`, a Rust sidecar preview command that accepts one
  supplied WeCom event from bounded stdin;
- sanitized shadow report containing only payload hash/byte count, event/message
  classification, selected field-presence flags, and hashes/byte counts for private
  identifiers or text;
- fixture replay inputs under `runtime/sidecar/fixtures/` for WeCom text, attachment
  placeholder, busy/fallback text, and unsupported event shapes;
- shadow mode that never replies, sends, generates, uploads, opens network or database
  connections, writes Feishu, creates artifacts, or mutates work-item state.

Validation:

- Rust unit tests for sanitization;
- fixture replay for WeCom text, attachment placeholder, busy/fallback text, and
  unsupported event shapes;
- no production apply mode.

### PR 4: Implement Rust Gateway Policy Preview

Deliverables:

- Rust policy preview for:
  - message classification;
  - busy-session handling;
  - internal process text filtering;
  - formatting fallback classification;
  - user-safe fallback copy;
  - idempotency and duplicate suppression;
- preview reports stored as internal artifacts or logs only.

Forbidden:

- no WeCom sends;
- no image generation;
- no provider calls.

Validation:

- fixture replay proves the screenshot text is classified as internal status;
- normal user requests containing words like "plain text" are not suppressed;
- bounded output and no raw private chat export.

### PR 5: Add Allowlisted Canary Gateway

Deliverables:

- canary-only sender path controlled by explicit configuration;
- allowlist for one test Bot, test chat, or test user;
- rollback command and observation script.

Forbidden:

- no production Bot route change;
- no broad group sends;
- no real user traffic outside the allowlist.

Validation:

- staging smoke with known test input;
- timeout and fallback behavior verified;
- rollback tested before any owner approval request.

### PR 6: Migrate Production Routing

Prerequisites:

- PR 2 through PR 5 merged;
- canary evidence reviewed;
- owner approval recorded;
- rollback target verified;
- release/current artifact includes the exact code to run.

Deliverables:

- reviewed production routing change;
- production smoke;
- rollback notes;
- follow-up monitor window.

Rollback:

- restore the WeCom production route to the prior Hermes service;
- keep captured artifacts/audit records read-only;
- do not delete live Hermes profile state in the same PR.

## Open Design Decisions

- Whether Rust owns the WeCom socket/webhook directly or first owns only a policy/safety
  sidecar while Hermes still owns the socket.
- Whether busy follow-ups should queue by default for Huabaosi instead of interrupting
  active image work.
- Where to store sanitized shadow decisions before a database migration is justified.
- Which test Bot or test chat is acceptable for canary.

These decisions should be resolved in the PR that first needs them, not bundled into
this planning PR.

## Success Criteria

The migration is complete only when:

- the production Huabaosi WeCom user-visible boundary is reviewed in this repository;
- the production route runs from release/current or another reviewed immutable artifact;
- internal Hermes status text cannot be sent to users as normal copy;
- WeCom send fallback is bounded, classified, and tested;
- image generation and media send remain behind the existing human gates;
- rollback to the previous Hermes route is documented and tested.

Until then, the current Hermes path remains the production fallback and must be kept
observable.
