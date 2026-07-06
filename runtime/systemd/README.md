# Runtime: systemd

`runtime/systemd` owns the monorepo-native systemd template boundary for Agent OS
services and workers.

## Responsibility

- Keep service unit templates and dry-run render checks versioned.
- Render units against immutable release directories and the `current` symlink model.
- Avoid server-local build paths or legacy standalone checkout references.
- Keep environment files and secrets out of git.

## Production Boundary

- Rendering templates is safe and non-mutating.
- Installing or restarting systemd units requires an owner-approved runbook and smoke
  evidence.

## Validation

```bash
pnpm runtime:systemd:check
pnpm deploy:systemd:check
```
