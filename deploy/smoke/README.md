# Smoke Checks

This package is the future common entrypoint for cross-profile, MCP, worker, and
deployment smoke checks.

Existing sidecar-specific smoke scripts remain in `deploy/sidecar/scripts/` until they
are wrapped here without changing behavior.

## Scope

- release manifest validation
- profile bundle dry-run validation
- MCP command resolution checks
- worker binary availability checks
- read-only external adapter preflight

Smoke checks must be safe by default. Real external sends require separate owner review,
allowlists, and explicit runtime configuration.

## Validation

```bash
pnpm deploy:smoke:check
pnpm check:light
```
