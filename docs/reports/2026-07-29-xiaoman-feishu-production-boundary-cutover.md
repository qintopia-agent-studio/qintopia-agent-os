# Xiaoman Feishu Production Boundary Cutover

Date: 2026-07-29

## Summary

Release `v0.2.51` deployed successfully and corrected the systemd release identity
binding. The Huabaosi image-generation production preflight then passed, its queue
preview reported no claimable request, and the reviewed activation enabled the image
generation timer successfully.

Two later read-only queue previews failed before external I/O. The Feishu mirror
selected a Feishu-primary-storage artifact even though that worker owns only
HTTPS-backed artifacts. The QiWe production companion reported its Feishu delivery
bridge as compiled in the top-level preflight, but the send-state boundary still treated
the same production feature pair as unsupported.

The enabled Huabaosi image-generation timer also had no future trigger. Systemd reported
it as `active (elapsed)`, `NextElapseUSecMonotonic=infinity`, with its previous trigger
still dated 2026-07-19. No image worker execution followed the 2026-07-29 activation.

No failed preview claimed work, wrote Postgres or Feishu, called the image provider or
QiWe, processed a callback, or sent a message. The Feishu mirror and QiWe image-send
timers remained disabled.

## Root Causes

1. The mirror candidate query did not enforce the documented HTTPS URI eligibility
   before selecting a generated image. A Feishu-primary-storage artifact became stale
   relative to its workbench reference after review and was selected, then correctly
   rejected by the HTTPS validator.
2. The QiWe bridge compile predicate was duplicated. The top-level adapter recognized
   both the staging pair and the production pair, while the send-state module recognized
   only the staging pair. The production artifact manifest was correct, but a real
   `feishu-base://` queue preview reached the stale predicate and failed.
3. All-feature tests masked the second defect because the all-feature build also enables
   the staging pair. Artifact-profile behavior needs to remain correct for the exact
   production feature pair, not only for the all-feature CI build.
4. The manually activated external timers used `OnBootSec` for their first trigger.
   Enabling one after the server boot boundary left no future monotonic trigger when the
   worker had no recent successful activation. Enabled/active-only checks accepted that
   elapsed timer as healthy.

## Remediation

- Restrict the Feishu mirror candidate query to lowercase `https://` artifacts. The
  existing structured URL and media identity validation remains authoritative after
  selection. Feishu-primary-storage artifacts continue to use their dedicated storage,
  authenticated revalidation, approval, and QiWe delivery paths.
- Make the send-state bridge predicate the single source used by the top-level QiWe
  adapter and include both reviewed staging and production feature pairs.
- Add a compile-time assertion for the exact QiWe production feature pair and a focused
  mirror candidate query test. No CI job, feature, protocol, database schema, or timer
  is added.
- Render only the three owner-activated external timers with `OnActiveSec` for the first
  trigger, re-arm them during approved activation, and require a finite future trigger
  in activation and read-only observation checks. Internal AgentOS timer behavior is
  unchanged.

## Validation

Run the focused Rust tests with the exact production feature pair, then the repository
deployment and PR checks:

```bash
RUST_MIN_STACK=33554432 cargo test \
  --manifest-path runtime/sidecar/Cargo.toml \
  --features qiwe-production-adapter,huabaosi-feishu-mirror-adapter \
  qiwe_image_send
RUST_MIN_STACK=33554432 cargo test \
  --manifest-path runtime/sidecar/Cargo.toml \
  --features huabaosi-production-adapter,huabaosi-feishu-mirror-adapter \
  huabaosi_feishu_artifact_mirror
pnpm deploy:contracts:check
pnpm check:pr:auto
```

After the reviewed fix is released, rerun both production queue previews. The mirror
must report no mirrorable generated images or one valid HTTPS preview; the QiWe
companion must report one image upload preview or no claimable send request without
rejecting the Feishu bridge. The image-generation timer must report a finite next
monotonic trigger. Keep the mirror and QiWe external timers disabled until their checks
pass.
