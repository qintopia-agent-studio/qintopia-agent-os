# Xiaoman Preflight QiWe Companion Routing

Date: 2026-07-30

## Summary

The release-local Xiaoman aggregate production preflight passes one discovered sidecar
binary to every child observation. That binary is the primary `huabaosi-production`
artifact under `sidecar/`, while the QiWe production observations require the separate
`qiwe-production` companion under `sidecar-profiles/qiwe-production/`.

As a result, the aggregate preflight can fail at the QiWe image-send observation even
when the independently executed QiWe send and callback observations pass against the
reviewed companion. This is a read-only composition defect; it does not indicate that a
worker ran, a timer changed, or an external send failed.

## Root Cause

The aggregate preflight's isolated child environment was introduced before production
releases installed QiWe as a separate companion artifact. The composition continued to
discover only `sidecar/qintopia-message-sidecar` and injected that primary path into all
children through `QINTOPIA_SIDECAR_BIN`.

The later dual-artifact release layout correctly kept Huabaosi and QiWe production
features separate, but the aggregate caller was not updated to route observations by
artifact profile.

## Resolution

- Discover the primary Huabaosi and QiWe companion binaries separately from the fixed
  release-local layout.
- Pass the selected binary to `run_step` explicitly.
- Keep the first nine Xiaoman, operations, and Huabaosi observations on the primary
  binary.
- Route the QiWe image-send and callback-bridge observations to the QiWe companion.
- Preserve `env -i`; each child still receives only the fixed system `PATH`, its enable
  flag, and its selected release-local sidecar path.

The change does not alter workers, timers, systemd units, database state, Feishu,
provider calls, callback processing, QiWe requests, or external send behavior.

## Validation

The production observation contract test constructs a temporary release layout with both
binary profiles, replaces all child observations with recorders, executes the real
aggregate script, and asserts the exact route for every child. Repository deploy,
preflight-readiness, production-evidence, PR-tier, and PR-doctor checks must also pass.

## v0.2.57 Live Read-Only Verification

The following production facts were rechecked on 2026-07-30 without changing runtime
state:

- GitHub Release `v0.2.57` was published for commit
  `9255cddeec726cd4e47a48e62e1ce8ac6eeba2cb`. Its CI, artifact, and production deploy
  workflows completed successfully.
- Production `release/current` resolved to that exact commit.
- The Huabaosi image-generation production observation passed against the primary
  artifact.
- The QiWe image-send observation passed with `state=disabled`,
  `artifact_profile=qiwe-production`, and the same release SHA.
- The Erhua QiWe callback-bridge observation passed with `state=enabled`,
  `artifact_profile=qiwe-production`, and the same release SHA. The Erhua gateway was
  active on the post-deploy process started at 12:12:31 CST.
- The unmodified `v0.2.57` aggregate preflight passed its first nine Xiaoman,
  operations, and Huabaosi observations, then reproduced the expected QiWe rejection:
  `requires the immutable release/current sidecar binary with reviewed production adapter features`.

These results accept the immutable `v0.2.57` deployment, artifact separation, and
current read-only runtime state. The aggregate failure is fully explained by this
composition defect rather than by a failed QiWe companion or callback-bridge binding.

## Remaining End-To-End Hold

No image callback processor event was recorded after the `v0.2.57` deployment, and the
release-local Xiaoman activity signal worker reported `no_eligible_signals` with zero
scanned work. The callback report fix in `v0.2.57` therefore still needs one fresh real
activity callback before the release can be treated as newly end-to-end accepted.

The current production signal ingress extracts `event_signals` from human-authored QiWe
messages. The configured event-signal target and Erhua's `QIWE_HOME_GROUP` resolve to
the same group, whose runtime metadata display name is `秦托邦的小伙伴（新）`. A fresh
acceptance input must therefore be posted by a human in that group; bot-authored
messages are excluded by the event-signal classifier. Writing the Feishu activity-plan
table alone does not create that AgentOS fact. Do not insert a synthetic event signal
through SQL, reuse the terminal `v0.2.56` request, or claim a form-origin acceptance
until a reviewed form-to-event-signal ingress exists. That bridge would be a separate
feature and is outside this minimal repair.

## Release And Acceptance Boundary

Published release `v0.2.57` is immutable and does not contain this repair. Its runtime
state must be accepted with the independent release-local observations that already
select the reviewed binary profile directly. The corrected aggregate composition becomes
available only after this change is merged, released, and deployed.

Do not hot-edit the `v0.2.57` release tree or reinterpret this repair as evidence that a
new real activity completed. A fresh real-activity callback/send remains a separate
production-completion gate.
