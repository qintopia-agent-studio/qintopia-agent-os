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

## Server Patch Review Pool

Server-local Hermes patches are extracted under `docs/operations/review-pool/hermes/`
only as source evidence. They are not release inputs and must not be applied to the
production checkout. Each stable behavior needs an owned implementation, focused
validation, and a separate production cutover PR.

The first extraction records the Huabaosi WeCom server patch at
`docs/operations/review-pool/hermes/2026-07-15-huabaosi-wecom-server-patch/`. Its
incident-specific filtering contract is owned by
`runtime/sidecar/src/huabaosi_wecom_policy.rs`; the raw Python patch remains
non-deployable because it mixes generic reliability changes, conflicting tests, and
cross-Agent copy.

## Erhua Weather Broadcast Asset

`skills/qintopia-weather/scripts/qintopia-erhua-weather-broadcast.py` is an allowed
release-owned script input. It renders the canonical forecast-first weather text to
stdout and performs no external delivery. The deploy bundle may carry it before a live
profile cutover.

Erhua's actual `cron/jobs.json` is not yet a reviewed bundle input because the
repository does not contain its schema or sanitized production structure. Do not invent
that declaration or repoint the live job until a read-only inventory records the job
shape and current script hashes. Activation and rollback belong in a separate reviewed
profile cutover.

## Initial Bundle

`agents/xiaoman/profile-bundle` is the first concrete observation-only bundle. It owns a
strict `SOUL.md` and `profile.yaml` renderer with fake fixtures, but it is not installed
or rendered by the deploy runner. Production identities remain in a server-local values
file. The bundle may be used only for read-only parity until a separate cutover PR adds
backup and rollback behavior.

Its one-time values migration command is an owner-triggered root operation, not a deploy
runner step. It may create only the fixed mode-`0600` values JSON after matching both
reviewed source hashes and complete rendered parity. Hermes does not read that file
until a future reviewed activation path exists.

## Validation

```bash
pnpm agents:profile-bundles:check
pnpm runtime:hermes:check
pnpm check:light
```
