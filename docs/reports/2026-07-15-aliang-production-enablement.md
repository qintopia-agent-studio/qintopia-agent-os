# Aliang Image Generation Production Enablement

Date: 2026-07-15

## Current State

Xiaoman production already creates internal `image_generation_request` work items from
approved poster briefs. The Huabaosi worker implementation could call the reviewed
provider, convert bounded PNG output to the deterministic final JPEG, upload it to the
isolated media boundary, verify same-byte readback, and create a pending
`generated_image`. Production artifacts intentionally omitted that live code and no
provider worker timer existed, so queued requests stopped before real generation.

The owner approved production enablement on 2026-07-15. Release publishing remains a
manual owner action.

Release `v0.2.10` was subsequently published and deployed. The required same-SHA
follow-up deploy installed the three Huabaosi production units from the immutable
release. The external worker timer remains disabled because the no-network production
preflight reports that provider and media configuration has not yet been provisioned.
The failed first follow-up request and corrected deployment are recorded in the
[follow-up deploy report](2026-07-15-v0210-follow-up-deploy.md).

## Resolution

- Add the production-only `huabaosi-production-adapter` Cargo feature and build the
  production artifact with exactly that feature. Staging features remain forbidden.
- Require the Rust command entry to validate the exact production approval phrase,
  deployed release SHA binding, production database URL SHA-256 binding, and existing
  provider/media policy before Postgres or network access.
- Render and install fixed preflight, worker, and timer units from the immutable
  release. The ordinary release installer leaves the external worker timer disabled.
- Add an explicit activation command that runs the no-network preflight service before
  enabling the timer, plus an immediate timer rollback command.
- Extend the production observation to validate both disabled and enabled states while
  retaining a dry-run-only queue preview.

The worker still creates only pending artifacts. It does not approve images, write
Feishu, call QiWe, or publish.

## Validation

The first PR CI run failed in `check-xiaoman-preflight-readiness.mjs` because the
pre-production contract forbade the Huabaosi `--apply` command anywhere in the systemd
renderer. That assertion correctly represented the previous disabled-only policy but
became stale once the owner approved a dedicated production service. The checker now
requires the fixed guarded service command while continuing to forbid that command in
the Xiaoman aggregate preflight. The preflight record now accepts either a consistent
disabled or enabled provider runtime state; observation remains dry-run-only.

- Default Rust suite: 371 passed, 0 failed.
- All-feature Rust suite: 367 passed, 0 failed, 9 guarded PostgreSQL tests ignored by
  design.
- Warning-denied Clippy passed with no default features, with
  `huabaosi-production-adapter`, and with all features.
- `cargo deny check advisories bans sources` passed with only the existing duplicate
  dependency warnings.
- systemd rendering, release installation, production activation, rollback, disabled and
  enabled observation, deploy contracts, deploy runner, and CI preflight tests passed.

No production database, provider, media service, Feishu, QiWe, or server was contacted
during these validations.

## Remaining Owner Action

Completed: the enablement PR was merged, `v0.2.10` was published, and the corrected
same-SHA follow-up deployment installed all three Huabaosi units. Same-SHA follow-up
requests must reuse the existing immutable release manifest's exact release scope and
restart targets; for `v0.2.10`, those targets are
`qintopia-system-services,hermes-erhua`.

1. Apply the production provider/media configuration, exact published release SHA, and
   database URL hash through the reviewed configuration channel.
2. Run the release-local activation command and inspect the first pending generated
   image before broadening the timer window.
3. On any unexpected cost, provider, storage, integrity, or claim outcome, run the timer
   rollback command first and then turn generation off through configuration.
