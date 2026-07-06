# Release Manifests

Release manifests record what a reviewed deployment is allowed to promote. They are
audit records, not secrets.

Each manifest should connect:

- git commit SHA
- CI artifact SHA and checksum
- COS object path when used
- profile bundle SHA when profile templates are included
- production services or profile mounts affected
- smoke commands and rollback target

Use `release-manifest.template.yaml` as the starting point for a new reviewed release
record.

## Validation

```bash
pnpm deploy:manifests:check
pnpm check:light
```
