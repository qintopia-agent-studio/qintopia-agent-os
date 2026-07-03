# Registry

The registry is the machine-readable index for Agent OS packages. It does not replace
package README files; it gives CI and programming agents a consistent way to discover
packages and validate their manifests.

## Files

- `schemas/package-manifest.schema.json`: shared package manifest schema.
- `schemas/registry-index.schema.json`: registry index schema.
- `agents.yaml`, `skills.yaml`, `workflows.yaml`, `mcp.yaml`, `runtime.yaml`,
  `deploy.yaml`, `deprecated.yaml`: domain indexes.

## Rules

- A registry entry points to a package path and its manifest file.
- Empty indexes are valid while the repo is still in migration.
- Template manifests under `_template/` are validated to keep future packages
  consistent.
- Adopted packages must be added to the matching registry index before they are treated
  as part of the monorepo contract.

Run:

```bash
pnpm registry:check
```
