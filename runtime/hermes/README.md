# Hermes Runtime Templates

This package defines the reviewed distribution boundary for Hermes profile bundles.

Hermes remains the live Agent runtime under `/home/ubuntu/.hermes`. This package is not
a copy of live Hermes state. It defines which reviewed files may be distributed from the
monorepo release and which files must stay runtime-local.

## Bundle Inputs

Allowed git-managed profile bundle inputs:

- `SOUL.md` templates
- `config.yaml` templates
- skills and plugin declarations
- MCP command declarations
- cron or scheduled-job declarations
- non-secret channel directory templates

Runtime-local files that must not enter git:

- `.env`
- sessions, logs, cache, pairing, auth, and locks
- generated memory and state databases
- private chat logs and raw member profile data
- server-local config overrides

## Release Model

Profile bundles should render into immutable release directories under
`/home/ubuntu/qintopia-agent-os-releases/<sha>` and become active through the stable
`current` symlink only after dry-run render checks, smoke checks, and owner review.

Do not replace a live Hermes profile `SOUL.md` or `config.yaml` directly from a feature
branch.

## Validation

```bash
pnpm agents:profile-bundles:check
pnpm runtime:hermes:check
pnpm check:light
```
