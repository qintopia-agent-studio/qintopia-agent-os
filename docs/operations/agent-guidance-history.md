# Compatibility and historical acceptance

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../plans/active/agents-guidance/README.md) records the baseline
and source locations.

## root-242

<!-- preserved-rule: root-242 -->

- As of 2026-07-15, 阿靓/Huabaosi real image production has not completed final
  activation. Do not treat it as live until the same reviewed release has follow-up
  deploy evidence, the Huabaosi timer is activated, and the first real pending
  `generated_image` has review evidence. The `v0.2.10` follow-up deploy and systemd
  installation evidence now exist, but the no-network preflight remains fail-closed
  because provider/media configuration is not provisioned; the timer must remain
  disabled until that gate passes.

<!-- /preserved-rule: root-242 -->

## root-267

<!-- preserved-rule: root-267 -->

- As of 2026-07-15, real end-to-end acceptance is not complete. Do not claim the Xiaoman
  activity flow is accepted until one real activity is observed from Xiaoman signal
  intake through image generation, human approval, and QiWe group-send arrival.

<!-- /preserved-rule: root-267 -->

## root-273

<!-- preserved-rule: root-273 -->

- As of `v0.2.30`, an existing release first assembled by `v0.2.29` may have a
  `manifest.json` that omits `runtime_artifact_profile` even though the immutable
  sidecar artifact manifest already records the reviewed profile. The same-SHA repair
  path must adopt that profile from `sidecar/artifact-manifest.json`, then persist it
  back into the release manifest before exact identity comparison. Do not hot-edit the
  server manifest by hand.

<!-- /preserved-rule: root-273 -->

## root-276

<!-- preserved-rule: root-276 -->

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

<!-- /preserved-rule: root-276 -->

## root-277

<!-- preserved-rule: root-277 -->

- PR #140 and PR #141 completed the Xiaoman profile bundle and values migration, but the
  live profile symlink cutover remains a separate PR. Do not repoint the live Xiaoman
  profile symlink without that reviewed cutover, smoke evidence, and rollback note.

<!-- /preserved-rule: root-277 -->
