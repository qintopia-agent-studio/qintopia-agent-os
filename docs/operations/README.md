# Operations

This directory contains operational evidence, source inventories, and future runbook
inputs. Server documents are summarized here as evidence before they are adopted into
canonical architecture, engineering, package, or deployment docs.

## Documents

- [inventory/README.md](inventory/README.md): M1 migration inventory for local sources,
  server sources, runtime assets, profiles, and services.
- [source-document-inventory.md](source-document-inventory.md): read-only inventory of
  server and local documents reviewed during the documentation organization pass.
- [runtime-baseline.md](runtime-baseline.md): production runtime baseline and migration
  implications.
- [agent-capability-matrix.md](agent-capability-matrix.md): active Agent package
  capabilities, approval boundaries, and runtime-state exclusions.
- [m9-server-cutover-runbook.md](m9-server-cutover-runbook.md): final migration runbook
  for monorepo checkout, sidecar service cutover, deprecated runtime cleanup,
  acceptance, and rollback.

## Checks

- `pnpm agents:check`: validates active Agent package templates and dry-run
  expectations.
- `pnpm deploy:preflight`: validates non-mutating deployment gates before any server
  cutover.

## Rules

- Do not edit server docs or code directly.
- Convert deployment evidence into runbooks through reviewed git changes.
- Treat server-side exploration as `review-pool` until owner review.
- Do not copy live secrets, `.env` files, generated caches, raw member profile text, or
  private chat logs into this repository.
