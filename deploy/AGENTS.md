# Deployment instructions

<!-- guidance-scope: deploy -->

These instructions supplement the [root contract](../AGENTS.md) for release,
installation, observation and rollback work. They grant no production authority.

## Map and required reading

- [Runner and operator guide](runner/README.md).
- [Complete deployment constraints](runner/README.md#operating-rules).
- [Release model](../docs/operations/release-current-model.md).
- [Release acceptance](../docs/operations/release-acceptance-checklist.md).
- [Server change policy](../docs/engineering/server-change-policy.md).
- [Historical compatibility](../docs/plans/active/agents-guidance/pending-review.md#operating-rules).
- [Rollback guide](rollback/README.md).
- [Restart routing](restart-target-rules.yaml).

Read the relevant complete contract before changing a deployment path. Use the owning
business package's runbook for allowlists, approval constants and commands; those
details are not replaced by this summary.

For Hermes changes read [Hermes rules](../runtime/hermes/AGENTS.md). For artifact
features read [Sidecar rules](../runtime/sidecar/AGENTS.md); for QiWe deployment also
read [QiWe rules](../skills/qiwe/AGENTS.md). Sibling rules are not auto-inherited.

## Validation commands

Run from the repository root:

- `pnpm deploy:contracts:check`
- `pnpm deploy:runner:check`
- `pnpm deploy:hermes-core:check`
- `pnpm check:pr:auto`

Select targeted fixtures before broad checks. Do not run an apply or a production
one-shot merely to validate documentation or local code. Use disposable databases and
fake provider transports where the package test calls for them.

## Release identity

Release requests bind reviewed commit, runtime artifact, artifact profile, bundle,
release directory, scope and restart targets. Preserve signed request validation and
bounded error/result schemas.

Same-SHA reuse must satisfy the exact immutable manifest identity, including scope and
restart targets. Only the explicitly documented metadata/companion repairs are
permitted; do not broaden repair into source mutation or rewrite manifests by hand.

Preserve distinct current and previous release pointers. Validate owner, mode, archive
path/type, symlink and checksum boundaries before promotion.

The main production runtime and QiWe companion have separate reviewed Cargo feature
profiles. Derive companion path and checksum from its manifest and SHA256SUMS. Do not
substitute the main binary or a mixed all-features build.

Release installation may install fixed units but cannot implicitly activate external
adapters or timers. Explicit activation paths retain their independent owner,
environment, identity and persistent-enable gates.

The old runner processes the first release that changes runner behavior. Follow the
reviewed follow-up/bootstrap contract; do not edit the running server to bypass it.

## Production operations

Before a switch, identify the immutable artifact, affected processes, existing state and
concrete rollback commands. A successful Actions run may only prove a dry-run; inspect
the actual server result and per-target activation evidence.

Do not use stale release tags, PR descriptions or a shared current pointer as a
substitute for the exact identity required by each production command.

Never source arbitrary writable server checkouts, expand the fixed unit allowlist, add
shell hooks, or pass ambient deployment secrets into observation children.

Observation is read-only unless its documented explicit apply boundary says otherwise.
Do not infer apply authority from an installed executable or timer.

Preserve profile state and channel enablement. Script permission repair is narrow,
source-verified and allowlisted; never recursively relax the Hermes home.

Sanitize retained evidence. Keep credentials, raw payloads, message text, provider
identifiers and secret-bearing output on the server under their existing controls.

## Rollback and handoff

Rollback code does not authorize restoring stale business databases or replaying jobs.
Follow each component's compatibility and ambiguous-outcome rules.

If a deployment fails before promotion, report that explicitly. If rollback only returns
to a known faulty state, label it degraded, not recovered.

Keep historical compatibility exceptions until their supported releases and rollback
artifacts have been reviewed. Do not remove them solely because the current version is
newer.

Update the affected runbook and indexed incident report with observed evidence, root
cause, validation, rollback and remaining owner action. Service liveness alone does not
satisfy production acceptance.

Release publication and PR merge remain explicit decisions. This file adds no automatic
merge, publication, external-send or restart permission.
