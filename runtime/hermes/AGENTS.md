# Hermes runtime instructions

<!-- guidance-scope: runtime/hermes -->

These instructions supplement the [root contract](../../AGENTS.md) for Hermes runtime
templates, official-core compatibility and Profile distribution.

## Map and required reading

- [Runtime package](README.md).
- [Complete runtime constraints](docs/runtime-agent-contract.md).
- [Complete cron constraints](docs/cron-agent-contract.md).
- [Production recovery runbook](../../docs/operations/hermes-production-recovery.md).
- [Runtime baseline](../../docs/operations/runtime-baseline.md).
- [Historical compatibility](../../docs/operations/agent-guidance-history.md).
- [Production acceptance](../../docs/operations/release-acceptance-checklist.md).

The root contract and these topic constraints remain binding. Read the applicable cron
or profile topic before implementing; a short summary is not a replacement for its
preserved conditions or exceptions.

Read [deploy rules](../../deploy/AGENTS.md) when changing install/launch behavior and
[QiWe rules](../../skills/qiwe/AGENTS.md) when changing plugin discovery or channel
routing. Neither sibling scope is inherited automatically.

## Validation commands

Run from the repository root:

- `pnpm runtime:hermes:check`
- `pnpm deploy:hermes-core:check`
- `pnpm test:qiwe`
- `pnpm check:pr:auto`

Use the official-core probes and fixtures documented by this package. Keep local probe
homes, profiles and transports isolated from live credentials and recipients.

A real-process discovery test must load plugins through the supported entrypoint;
calling register directly does not prove a Gateway or worker can discover it.

## Runtime boundaries

Hermes remains the Agent runtime, not the business database. Keep business facts in
their governed AgentOS/Postgres owners rather than adding an alternate store.

Preserve the official core boundary. Server review-pool patches and mixed old checkouts
are evidence, not deployable replacements. Do not apply local core hotpatches as a
shortcut around reviewed versions and compatibility checks.

Core, interpreter, launcher and dashboard assets must be assessed together when
upgrading. A clean source tree alone does not prove that a running process or packaged
frontend matches that source.

The production environment is uv-managed. Validate the resolved interpreter and its base
according to the existing runtime rules rather than assuming a system Python layout. Do
not fix path validation by broadening arbitrary-path access.

Keep profile distributions distinct from live .hermes state. Preserve credentials,
history, sessions, memories, jobs and intentional channel enablement. An unused profile
is not authorization to remove it.

Xiaoman profile observation and one-time values migration retain their original scope.
Packaging a bundle does not approve live symlink replacement or cutover.

## Cron and delivery

Read the cron contract before modifying ownership, scheduling, wrappers or task
snapshots. Keep the reviewed source of truth and explicit legacy timer boundaries.

A warning is not proof of a lost job. Reconcile execution and delivery state before
proposing replay. Never clear locks or task state to force progress.

A missing delivery acknowledgement is an ambiguous outcome, not permission to send
again. Preserve the owning queue and worker's idempotency and retry boundaries.

Explicit QiWe group delivery must preserve group semantics through scheduled and
standalone paths. Do not infer a direct-message destination from a bare group id.

## Acceptance and reporting

Check configuration separately from liveness, plugin discovery, model/tool work, channel
delivery and scheduled execution. State which checks actually ran.

A cached connected state, open socket or active service is not a successful user round
trip. Synthetic transport tests cannot certify live subscription ownership.

Follow the recovery runbook's backup, compatibility, component rollback and observation
requirements. Do not restore runtime data merely to roll back code.

Keep unresolved upstream behavior explicit. Do not present a restart loop or a quiet log
window as a fix for an unverified subscription or delivery defect.

Update the package runbook and indexed incident report for escaped compatibility
failures. Include the exact source/version boundary and the smallest regression that
would have caught the failure, without retaining credentials or messages.
