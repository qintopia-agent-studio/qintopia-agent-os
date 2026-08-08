# Secret Rotation Review Inventory

Date: 2026-08-08 Type: review-evidence (read-only governance package) Owner: qiaopengjun
Risk level: low

## Scope

This package inventories the current state of **secret/credential rotation** for the
production runtime. It is the governance prerequisite for any future recurring-rotation
work, and carries **no runtime change**. It is one of the three originally-deferred
("不急") follow-ups from the weekly-preview cron black-box cleanup (the other two: qiwe
mention guard, PR #387; and `.env` drift, sibling inventory in this directory).

Key finding up front: **secret rotation is not an unsolved problem.** A vetted,
Release-bundled rollover state machine already exists for the shared database password.
The open work is (a) confirming that gap-closure landed in production, and (b) covering
the remaining non-database secret classes.

## Current State

- **`deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py`** — a
  production-grade, immutable-Release-bundled rollover state machine. Notable controls:
  - Root-only durable escrow under `/var/lib/qintopia-xiaoman-db-password-rollover` (a
    volatile `/run` escrow was explicitly rejected per the gap report).
  - Exact dual-target routing `qintopia-system-services,hermes-erhua` matching the
    canonical resolver order.
  - Exact-SHA dry-run gate: the rollover entrypoint validates the processed request id
    and a root-owned successful, non-rollback result before it creates state or touches
    PostgreSQL.
  - Owner approval phrase `approved-production-xiaoman-shared-db-password-rollover-v1`,
    bound to a reviewed Release artifact and database URL SHA-256.
- **`docs/reports/2026-08-03-xiaoman-shared-database-rollover-gap.md`** — documents the
  gap that motivated the state machine: the shared DB credential also lived in the Erhua
  Hermes gateway + a child process + `/home/ubuntu/.hermes/profiles/erhua/.env`; the
  protected config transaction did not cover every holder; and a host restart
  mid-rotation could have made the new credential unrecoverable. Resolution mandates
  extending the protected transaction to include the Erhua profile env, shipping the
  state machine in the immutable Release, dual-target routing, and owner review _before_
  `prepare` invalidates the previous credential.
- **`tools/security/check-secrets.mjs`** — secret-presence/format gate (does not perform
  rotation; it is a static check).
- **`runtime/sidecar/.env.example`** — all live secrets are placeholders
  (`replace-with-server-secret`, `replace-with-xiaoman-feishu-app-secret`,
  `QIWE_TOKEN=replace-with-server-secret`, `QINTOPIA_EMBEDDING_API_KEY=...`, ingress
  HMAC keys, etc.), confirming non-DB secrets are provisioned host-side and rotated
  manually.

## Evidence (pinned)

| File                                                                       | SHA-256                                                            |
| -------------------------------------------------------------------------- | ------------------------------------------------------------------ |
| `deploy/sidecar/scripts/xiaoman-shared-db-password-rollover-production.py` | `d26b43c2037b4cab4133d2ca3b8b74b68dc113fd3cf4976d3771720789933d0a` |
| `docs/reports/2026-08-03-xiaoman-shared-database-rollover-gap.md`          | `5c3ba23863cfd95196168cc24d1b6f905baffccca9fc1ea82ee0d1d5ede2ffe5` |
| `tools/security/check-secrets.mjs`                                         | `3ca794e433dd434a814fd811bd62c842ddc267e8cdf748196a5f290dc4ab4b70` |
| `tools/security/README.md`                                                 | `47e373f659afc50f381397aa2bc877c5eba9140e5e6d81eea120e33ad669da81` |
| `runtime/sidecar/.env.example`                                             | `4ff397ad0a67bd7f6d56e182679d9339502deb26a5da5a55f346dbe64b87bbe0` |

## Residual Risks

- **R1 — Gap closure unconfirmed in production.** The `2026-08-03` gap report captured
  the ordering mismatch at `v0.2.69`; whether the corrected dual-target rollover
  subsequently executed cleanly against the live database on the host is not asserted by
  this evidence package. Requires an owner read of current Release state.
- **R2 — Non-DB secrets have no automated rollover.** Feishu app secret, QiWe token,
  embedding API key, and ingress/callback HMAC keys are `replace-with-server-secret`
  placeholders with no equivalent state machine. Rotation today is manual host editing.
- **R3 — Escrow durability is implementation-bound.** The rollover's safety depends on
  the root-only persistent state dir; any future refactor that moves escrow to a
  volatile path re-opens the unrecoverable-credential risk from the gap report.

## Governance Proposal

- **P1 (owner):** read current Release state on the host to confirm the shared-DB
  rollover gap is closed (no process retains the previous DB URL; rollover receipt
  retained).
- **P2 (owner-reviewed PR):** extend rotation coverage to the non-DB secret classes, or
  document a manual rotation runbook with the same owner-approval + exact-SHA dry-run
  discipline as the DB rollover.
- **P3 (owner decision):** decide whether recurring/scheduled rotation is in scope; if
  so, it must ride the same immutable-Release + dual-target + owner-review pattern,
  never a hot host edit.

## Gates

- `db-rollover-state-machine-exists`: immutable-Release script with root-only escrow,
  dual-target routing, exact-SHA dry-run gate, owner approval phrase.
- `gap-documented`: `2026-08-03` report names root cause and resolution.
- `template-only-secrets`: `.env.example` placeholders; live secrets are host-side.

## Pending Before Activation

- P1 owner read of current Release state to confirm gap closure.
- P2 owner-reviewed PR covering non-DB secret rotation (or manual runbook).
- P3 separate owner decision on scheduled rotation scope.

## Validation

```bash
pnpm security:check
pnpm deploy:contracts:check
```

## Rollback

No runtime state is touched by this evidence package; rollback is a no-op. The DB
rollover itself already retains a durable receipt and rollback path per the gap report.
