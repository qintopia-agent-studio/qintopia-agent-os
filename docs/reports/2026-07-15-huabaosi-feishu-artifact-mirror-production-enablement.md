# Huabaosi Feishu Artifact Mirror Production Enablement

Date: 2026-07-15

## Current State

The guarded mirror adapter is merged separately. It revalidates an immutable Huabaosi
final JPEG, mirrors one artifact version into the fixed Feishu schema, and records only
sanitized sync state in Postgres. Production artifacts previously omitted the adapter
feature and no release-installed timer could execute the worker, so the path could not
run in production.

The owner approved production enablement. Release publication remains a manual owner
action, and ordinary release deployment must not activate the external write timer.

## Resolution

- Build production sidecar artifacts with exactly `huabaosi-production-adapter` and
  `huabaosi-feishu-mirror-adapter`; continue rejecting staging and QiWe features.
- Bind mirror preflight and apply to the exact deployed 40-character commit SHA in
  addition to the existing owner phrase, database hash, Base/table exact allowlists,
  fixed schema, profile path, and media host policy.
- Render and install fixed mirror preflight, worker, and timer units from the immutable
  release while leaving the timer disabled. Ship the read-only observation script and
  non-secret observation preflight command in the same release.
- Add explicit owner-approved activation and rollback commands. Activation confirms the
  persistent mirror flag is present exactly once and set to `1`, then runs preflight
  before enabling the timer.
- Add a read-only production observation that verifies timer state and runs a non-secret
  mirror observation preflight only. It discovers the immutable
  `release/current/sidecar/qintopia-message-sidecar` binary, accepts an explicit binary
  only when it resolves to that same release-local file with the approved production
  features, and cannot fall back to a mutable source checkout.
- Parse only the enable flag as literal text in the observation shell, then pass only
  that flag and the non-secret release SHA to the immutable binary through a child
  launcher. The script does not source or eval the env file, execute command
  substitution, import Feishu/Postgres secrets, or create a secret-bearing temporary
  file. Non-allowlisted env values are ignored before mirror-flag value validation, so
  punctuation in unrelated credentials cannot break observation.
- Reviewer Guide disposition: the observation no longer runs the full Feishu mirror
  preflight or worker dry-run because those paths require database/Base/table/Feishu
  runtime configuration. Full configuration validation remains in activation and apply;
  observation proves the disabled/enabled boundary, immutable artifact contract, adapter
  compilation, deployed SHA shape, and redaction without passing secret env.
- Stop the timer and worker first during rollback, then fail closed until the persistent
  mirror enable flag is confirmed present exactly once and exactly `0` in the reviewed
  sidecar env.

Postgres remains the fact source. The worker cannot approve an image, update the legacy
poster task summary, call QiWe, send, publish, or change image-generation state.

## Production Configuration

The runtime configuration channel must provide the production release SHA, database URL
hash, Base token and exact allowlist, artifact table id and exact allowlist, fixed
schema version, Huabaosi profile env path, and media host allowlist. Feishu app
credentials stay in the existing server-side Huabaosi profile. No value, table id,
token, URL, or secret is committed to git.

## Validation

The PR validates focused Rust mirror tests, production/all-feature builds and Clippy,
RustSec advisories, systemd rendering and installation, artifact feature manifests,
activation/rollback, read-only observation, deploy bundle contracts, repository checks,
and PR body policy. Local validation does not contact production Postgres, media,
Feishu, QiWe, or a production server.

## Remaining Owner Action

1. Merge the ordinary production enablement PR after its latest Reviewer Guide, reviews,
   inline threads, and CI are clean.
2. Manually merge the resulting Release Please PR only when ready, then manually publish
   its draft GitHub Release.
3. Deploy the published immutable SHA and provision the reviewed production values
   through the server configuration channel.
4. Run the release-local observation, activate the timer explicitly, and verify the
   first mirrored record against the source artifact and sanitized Postgres audit.
5. On any unexpected write, run rollback to stop the timer, disable the persistent
   mirror flag through the controlled configuration channel, and rerun rollback until it
   confirms the disabled state before investigating further.
