# Server Change Policy

The server is a deployment target, not an editing workspace.

## Allowed Server Activity

- read-only inventory
- service status checks
- log inspection
- smoke checks
- deploying an approved commit SHA through a runbook
- emergency rollback with a follow-up patch and owner record

## Disallowed Server Activity

- editing docs directly
- editing code directly
- editing `.hermes` runtime files directly
- scp overwrites of individual source files
- committing unreviewed experiments on the server and treating them as product direction
- changing deployment scripts or runbooks on the server without a matching git commit

## Deployment Rule

Deployments should identify:

- repository
- branch
- commit SHA
- package or runtime area affected
- validation already run in CI or locally
- smoke check to run after deploy
- rollback command or previous SHA

If an emergency requires a server-side rollback, the follow-up PR must capture what
changed, why it changed, and how the canonical repository should be updated.

## Documentation Rule

Server documents may be read and summarized as evidence. Canonical documentation must be
edited in this monorepo and reviewed through git.

## Observation Records

Production deployment, preflight, smoke, and CI integration failures must be recorded in
the same remediation PR under `docs/reports/` and indexed by `docs/reports/README.md`.
Each record must identify the observed evidence, root cause, resolution, validation,
remaining safety boundary, and next owner action. Update the affected runbook, package
README, manifest, or automated check in that same PR so the operating contract changes
with the code.
