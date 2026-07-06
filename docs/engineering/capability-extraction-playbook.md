# Capability Extraction Playbook

Use this playbook when moving an Agent OS capability out of a broad package such as
`skills/qintopia-tools`.

## Target Shape

```text
legacy Hermes entrypoint
  keeps tool registration and stable tool name only

skills/<capability>
  owns Agent-facing behavior, policy, manifest, docs, fixtures, and tests

mcp/<adapter>
  owns provider/database/external-system adapter contract when the capability depends on
  shared infrastructure

deploy bundle and checks
  include every runtime path required by the new package boundary
```

## Migration Steps

1. Update the target package README or manifest before implementation.
2. Move the real implementation into `skills/<capability>`.
3. Keep the old package as a thin registration shell only when Hermes still loads that
   entrypoint.
4. Move behavior tests to the new capability package.
5. Keep only registration/delegation tests in the legacy package.
6. Add or update MCP adapter contracts when provider or database behavior is shared.
7. Update registry entries and the change routing index.
8. Update deploy bundle packaging and preflight checks for any runtime path the new
   package needs.
9. Run package checks, deploy bundle build, and `pnpm check:light`.

## Risk Strategy

- Low-risk capability: direct migration is acceptable when there are no external sends,
  no database writes, one stable tool name, and package tests cover the behavior.
- Medium-risk capability: keep a thin forwarding shell and add fixture replay before
  changing runtime packaging.
- High-risk capability: add contract, fixtures, dry-run, shadow or read-only checks, and
  rollback notes before switching runtime entrypoints.

Do not keep two active implementations for the same capability. If a fallback is needed
for rollout safety, document the expiry condition and remove it in the same migration
series.
