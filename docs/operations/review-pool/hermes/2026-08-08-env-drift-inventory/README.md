# Environment Drift Review Inventory

Date: 2026-08-08 Type: review-evidence (read-only governance package) Owner: qiaopengjun
Risk level: low

## Scope

This package inventories the current state of **`.env` / runtime-environment drift**
between the repository template (`runtime/sidecar/.env.example`), the rendered staging
env, and the live host secret files under `/etc/qintopia/*.env` and
`/home/ubuntu/.hermes/profiles/*/.env`. It is the governance prerequisite for any future
"detect or reconcile env drift" work, and carries **no runtime change**.

This is one of the three originally-deferred ("不急") follow-ups from the weekly-preview
cron black-box cleanup. The other two were the qiwe mention guard (PR #387) and secret
rotation (see sibling inventory in this directory).

## Current State

The repository already has a formal drift-governance layer, so "env drift" is not an
ungoverned area:

- **`docs/engineering/anti-drift-policy.md`** — explicitly forbids committing live
  profile state (`.env`, auth files, secrets, sessions, caches, logs, state databases)
  into `agents/*`. `pnpm policy:check` enforces this via
  `tools/policy/check-anti-drift.mjs`.
- **`docs/operations/runtime-baseline.md`** — Server Handling Rules: _read-only
  inventory is allowed; direct server edits are not allowed_; runtime dirs under
  `.hermes/profiles/*` are live state and cannot be copied wholesale; deployment must
  happen from reviewed commit SHAs through runbooks.
- **`runtime/sidecar/.env.example`** — the canonical template. All live values are
  placeholders (`replace-with-server-secret`, `replace-with-xiaoman-feishu-app-secret`,
  `replace-with-production-database-url-sha256`, etc.). The real values live only on the
  host in `/etc/qintopia/message-sidecar.env`,
  `/home/ubuntu/.hermes/profiles/xiaoman/.env`, and
  `/home/ubuntu/.hermes/profiles/erhua/.env` (per `runtime-baseline.md` and the rollover
  script's path constants).
- **`deploy/sidecar/scripts/render-staging-runtime-env.py`** — renders the _staging_ env
  from templates; production env is managed via the immutable Release + `/etc/qintopia`
  drop-ins, not from this repo file.
- **`docs/reports/2026-07-29-systemd-release-env-precedence.md`** — documents the
  precedence order of env sources for the release-managed services.

## Evidence (pinned)

| File                                    | SHA-256                                                            |
| --------------------------------------- | ------------------------------------------------------------------ |
| `docs/engineering/anti-drift-policy.md` | `48b47721997e2276ffc9644a2eba60f8f830a3b558be84c82c10f447500eda1a` |
| `tools/policy/check-anti-drift.mjs`     | `a5d041e6c2ed8c9e6fde0fb1237043f8da62b5b94a2435cb67adc3813a506287` |
| `docs/operations/runtime-baseline.md`   | `16da3aef6243c9300e9be48a35a8679e02475069deb8cc3d227e9ceab6a66af1` |
| `runtime/sidecar/.env.example`          | `4ff397ad0a67bd7f6d56e182679d9339502deb26a5da5a55f346dbe64b87bbe0` |

## Residual Risks

- **R1 — No value-parity check.** `check-anti-drift.mjs` guards _direction_ (toolchain,
  review-pool classification, secret-in-agent-package exclusion). It does **not**
  compare live host `.env` values against `.env.example` keys. A host secret
  added/removed/renamed relative to the template would not be flagged by CI.
- **R2 — Staging vs production parity.** `render-staging-runtime-env.py` renders staging
  only. There is no automated check that production `/etc/qintopia/*.env` stays aligned
  with the template's key set after a release.
- **R3 — Stale secret values.** When a secret is rotated (see sibling rotation
  inventory), the old value may linger in a host `.env` that the rotation path did not
  cover, with no drift detector to surface it.

## Governance Proposal

- **P1 (repo):** add a read-only `tools/security/check-env-parity.mjs` (or extend
  `tools/security/check-secrets.mjs`) that compares the key set of live host env files
  against `runtime/sidecar/.env.example`, reporting added/removed/renamed keys. Must run
  only on the host under owner control; never reads or emits secret values.
- **P2 (owner):** one-time owner read of the live `/etc/qintopia/*.env` and
  `.hermes/profiles/*/.env` key sets, diffed against the current template, to seed the
  baseline.
- **P3 (owner-reviewed change):** if drift is found, a separate owner-approved PR
  reconciles the template or the host file — never a direct host edit outside a runbook.

## Gates

- `anti-drift-policy-exists`: `pnpm policy:check` already blocks live `.env` from
  entering `agents/*`.
- `runtime-baseline-rules`: read-only inventory allowed, direct server edits forbidden.
- `template-only-secrets`: `.env.example` carries only placeholders; live values are
  host-side.

## Pending Before Activation

- P1 owner-reviewed PR adding the read-only env-parity check.
- P2 owner read of live host env key sets.
- P3 separate owner decision if reconciliation is required.

## Validation

```bash
pnpm policy:check
pnpm security:check
```

## Rollback

No runtime state is touched by this evidence package; rollback is a no-op.
