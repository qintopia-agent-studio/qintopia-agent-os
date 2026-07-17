# Erhua Weather Broadcast Runtime Adoption

Updated: 2026-07-17

## Goal

Move Erhua's 07:00 weather output selection into a reviewed release asset so the
scheduled path emits the canonical `morning_broadcast` produced by
`skills/qintopia-weather`. The scheduled path must not rebuild the message from
`current` conditions.

## Current Evidence

- `skills/qintopia-weather` already returns `daily_forecast`, `morning_reference`, and
  forecast-first `morning_broadcast` output for both QWeather and Open-Meteo fallback.
- Hermes QiWe delivery already owns cron-to-group sending through `QIWE_HOME_GROUP` and
  the QiWe standalone sender.
- The deploy bundle includes the weather skill implementation but not the observed
  runtime script `qintopia-erhua-weather-broadcast.py`.
- The repository inventory names the live `cron/jobs.json` and weather scripts, but it
  does not contain their bodies, source hashes, command, target chat, or Hermes cron
  schema.

This evidence confirms a release-ownership gap in the last mile. It does not prove the
exact line or field used by the current server-only script.

## This Change

1. Add a no-send weather CLI under `skills/qintopia-weather/scripts/`.
2. Call the existing weather handler with fixed arguments
   `{"intent": "general", "hours": 24}`.
3. Parse the handler JSON and write only the top-level `morning_broadcast` to stdout.
4. Fail closed with empty stdout if the payload is unsuccessful, malformed, missing
   forecast periods, or has regressed to current-only wording.
5. Package the CLI in the immutable deploy bundle and add checks that prevent it from
   being dropped from future releases.

The CLI does not import the QiWe adapter, read QiWe credentials, choose a target group,
or call a send endpoint. Hermes and `skills/qiwe` keep those responsibilities.

## Activation Gate

This change does not render or replace the live `cron/jobs.json`. Before activation, a
read-only production inventory must record a sanitized copy or structural summary of:

- the 07:00 job schedule, timezone, mode, command, and delivery target source;
- the current broadcast script path and SHA-256;
- the current context script path and SHA-256;
- whether the job can switch only its executable path or needs a reviewed profile bundle
  renderer.

After that evidence exists, use a separate reviewed cutover change to point the job at
the release-owned CLI, smoke the rendered output without external delivery, then
activate the existing Hermes delivery path. Do not hot-edit the profile.

## Production Boundary

- External sends: contract-adjacent, but this change executes no send.
- Database writes: none.
- Hermes profile runtime: packages a future cron executable; no live profile or cron
  reference changes. The Release restart resolver still selects `hermes-erhua` because
  the Erhua/weather packages changed, so deployment can briefly interrupt the gateway
  even though the new executable is not activated.
- Secrets: none added or read by the CLI beyond the existing weather provider runtime.
- Feishu: untouched.
- QiWe: existing delivery ownership is documented; adapter behavior is unchanged.
- systemd: no unit definition changes. Because the deploy contract tooling changed, the
  Release restart resolver also selects `qintopia-system-services`; deployment can
  briefly restart the fixed system-service allowlist.

## Validation

```bash
CI=true pnpm skills:qintopia-weather:check
CI=true pnpm skills:qintopia-tools:check
CI=true pnpm mcp:adapters:check
CI=true pnpm agents:check
CI=true pnpm agents:profile-bundles:check
CI=true pnpm runtime:hermes:check
CI=true pnpm deploy:contracts:check
CI=true pnpm deploy:preflight:ci
CI=true pnpm deploy:runner:check
CI=true pnpm artifact:deploy-bundle
node tools/deploy/resolve-restart-targets.mjs --base-ref origin/master --head-ref HEAD
CI=true pnpm check
```

## Rollback

Before activation there is no cron-specific rollback because the live cron remains
unchanged. Release deployment rollback still applies: if promotion smoke fails, the
runner must restore `release/current` from `previous`; if an issue is found after a
successful deployment, use the approved release rollback path and smoke both expected
restart targets, `hermes-erhua` and `qintopia-system-services`.

After a future cron cutover, rollback must restore the previous reviewed profile or
executable reference through the release workflow, then restart and smoke the targets
resolved for that separate cutover. Do not hot-edit the live profile.
