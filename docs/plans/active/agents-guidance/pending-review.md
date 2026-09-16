# Pending policy decisions

All rules below remain in force with their original conditions. This migration does not
interpret age, a merged PR, or a new release as permission to retire them.

| Question                                                                 | Existing evidence                                             | Disposition                                                                                               |
| ------------------------------------------------------------------------ | ------------------------------------------------------------- | --------------------------------------------------------------------------------------------------------- |
| July Huabaosi/Xiaoman acceptance statements                              | Dated rules and referenced production evidence gates          | Preserve the historical conditions below; verify actual acceptance separately.                            |
| v0.2.10/v0.2.29/v0.2.30 release reuse exceptions                         | Original release/manifest repair clauses                      | Preserve until supported rollback releases are inventoried.                                               |
| Xiaoman profile distribution versus live cutover                         | Original PR #140/#141 restrictions                            | Preserve; package migration is not live cutover authorization.                                            |
| General draft sequencing versus the owner-approved v0.3.0 reconciliation | Collaboration model records the historical v0.2.177 exception | Preserve the general rule and the explicit exception; do not generalize it.                               |
| Server runtime edits versus owner-directed credential rotation           | Server change policy and session-specific operations          | Preserve the repository policy; this documentation task grants no new exception.                          |
| Repeated Sidecar/QiWe feature, transport and send clauses                | Root and Sidecar source blocks                                | Retain both detailed clauses when wording/conditions differ; short entries link to their canonical topic. |

The historical conditions below remain binding. The questions above do not authorize
weakening any existing rule.

## Operating rules

These constraints supplement the scoped AGENTS.md summaries. Conditions and historical
exceptions remain binding. Backtick paths from root rules are repository-relative;
Sidecar rules retain their original `runtime/sidecar/` path base.

### Core Rules

- As of 2026-07-15, 阿靓/Huabaosi real image production has not completed final
  activation. Do not treat it as live until the same reviewed release has follow-up
  deploy evidence, the Huabaosi timer is activated, and the first real pending
  `generated_image` has review evidence. The `v0.2.10` follow-up deploy and systemd
  installation evidence now exist, but the no-network preflight remains fail-closed
  because provider/media configuration is not provisioned; the timer must remain
  disabled until that gate passes.

- As of 2026-07-15, real end-to-end acceptance is not complete. Do not claim the Xiaoman
  activity flow is accepted until one real activity is observed from Xiaoman signal
  intake through image generation, human approval, and QiWe group-send arrival.

- As of `v0.2.30`, an existing release first assembled by `v0.2.29` may have a
  `manifest.json` that omits `runtime_artifact_profile` even though the immutable
  sidecar artifact manifest already records the reviewed profile. The same-SHA repair
  path must adopt that profile from `sidecar/artifact-manifest.json`, then persist it
  back into the release manifest before exact identity comparison. Do not hot-edit the
  server manifest by hand.

- As of 2026-07-15, the corrected `v0.2.10` same-SHA follow-up deploy installed the new
  systemd units. A same-SHA request for an existing release must reuse the immutable
  manifest's exact runtime, runtime artifact profile, bundle, commit, scope, and
  restart-target fields. The only content exception is installing a complete missing
  QiWe companion into a legacy Huabaosi-only release without changing the primary
  binary. Narrowing `restart_targets` is rejected before promotion and does not trigger
  rollback. Content, path, type, or symlink drift must fail before mutation. After the
  bounded metadata or companion repair allowed above, the existing tree must satisfy the
  same deploy-runner owner, non-writable, directory accessibility, regular/symlink type,
  sidecar `0755`, and metadata `0444` checks as a new staging tree. Same-SHA reuse must
  preserve a distinct `previous` target. Production release and staging roots must be
  created explicitly as `0755` so the validation contract does not depend on ambient
  `umask`.

- PR #140 and PR #141 completed the Xiaoman profile bundle and values migration, but the
  live profile symlink cutover remains a separate PR. Do not repoint the live Xiaoman
  profile symlink without that reviewed cutover, smoke evidence, and rollback note.
