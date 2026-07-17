# Erhua Weather Broadcast Current-only Output

Date: 2026-07-17

## Observed Evidence

- Erhua's scheduled morning message was delivered, but the member-facing content
  contained only current conditions instead of the expected local-day forecast.
- The canonical weather package generates a forecast-first `morning_broadcast` with
  fixed midday, afternoon, and evening periods. Its package tests passed before this
  remediation.
- The release bundle contained `skills/qintopia-weather` but did not contain the
  observed runtime broadcast script.
- Repository inventory named `qintopia-erhua-weather-broadcast.py`,
  `qintopia-erhua-weather-context.py`, and `cron/jobs.json`, but their live contents
  were not available in git. Read-only SSH inspection was unavailable, so no claim is
  made about the exact current server script line.

## Root Cause

The confirmed root-cause class is a release-ownership and consumption-contract gap: the
package that generates `morning_broadcast` is reviewed and deployed, while the scheduled
script that chooses the final output is server-only and outside the deploy bundle. That
allows the last mile to remain on a current-only implementation even when the canonical
forecast code is present.

The exact live implementation defect remains unconfirmed until the scheduled job and
script hashes are inventoried read-only.

## Resolution

- Add a release-owned, no-send CLI that calls the canonical weather handler and emits
  only its top-level `morning_broadcast`.
- Reject unsuccessful, malformed, missing-period, overlong, or current-first payloads
  instead of falling back to `current`.
- Add regression tests for fixed handler arguments, exact stdout, current-only
  rejection, and silent stdout on failure.
- Include the CLI in the deploy bundle and require it in deploy contract checks.
- Record the CLI source in Erhua's non-secret profile template without inventing or
  activating a `cron/jobs.json` schema.

## Validation

The remediation is accepted locally only after the weather package, Erhua profile
contracts, Hermes runtime contracts, deploy bundle checks, and complete repository
checks pass. The built artifact manifest must contain
`skills/qintopia-weather/scripts/qintopia-erhua-weather-broadcast.py`.

## Remaining Boundary

This remediation does not send a message, change the target group, edit the live Hermes
profile, or activate a cron job. The current production job remains unchanged until a
separate reviewed cutover. A normal Release deployment is not impact-free: restart
routing selects `hermes-erhua` and `qintopia-system-services`, so the existing Erhua
gateway and the fixed system-service allowlist can be briefly interrupted during
promotion and must pass the runner's post-promotion smoke checks. The approved
`release/current` to `previous` rollback remains available even before cron activation.

## Next Owner Action

Collect a sanitized read-only inventory of the 07:00 job and both weather scripts,
record their hashes and command shape, then review an activation and rollback change
that points the existing Hermes cron delivery path at the release-owned CLI.
