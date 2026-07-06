# Rollback

Rollback restores production to the previous approved release without editing server
source files.

The standard model is:

1. keep immutable release directories under `/home/ubuntu/qintopia-agent-os-releases/`;
2. keep `current` and `previous` symlinks;
3. promote or roll back by repointing symlinks through an approved runbook;
4. restart only affected services;
5. run smoke checks and record evidence.

## Boundaries

- Do not edit files directly under `.hermes`.
- Do not fetch or build source on the production server for routine rollback.
- Do not delete rollback material without an owner-approved retention plan.

## Validation

```bash
pnpm deploy:rollback:check
pnpm check:light
```
