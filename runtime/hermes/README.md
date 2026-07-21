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

## Erhua Model Overlay

`render_profile_overlay.py` applies the reviewed Erhua model overlay to a sanitized or
runtime-local base config. It rejects aliases, duplicate keys/providers, forbidden
overlay fields, and path aliasing. Reports contain changed field paths and file hashes,
not values. `migrate_erhua_livecool_env.py` creates or checks the server-local
`LIVECOOL_API_KEY` binding without printing credential material.
`verify_runtime_provider.py` runs inside the installed Hermes interpreter during both
dry-run and activation smoke. It requires Hermes's own provider resolver to return the
approved named provider and base URL.

The deploy runner is the only production caller. It supplies fixed profile paths; deploy
requests cannot supply paths. See
`docs/operations/profile-bundles/erhua-livecool-profile-overlay-runbook.md`.

## Validation

```bash
pnpm agents:profile-bundles:check
pnpm runtime:hermes:check
pnpm check:light
```
