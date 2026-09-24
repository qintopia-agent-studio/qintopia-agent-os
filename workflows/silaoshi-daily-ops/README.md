# Workflow: Silaoshi Daily Ops

`workflows/silaoshi-daily-ops` defines the daily operations loop for Si Laoshi. It
covers SOP follow-up, activity review, service follow-up, and operating summaries.

## Responsibility

- Read approved operational state and evidence.
- Produce daily review or follow-up tasks for human operators.
- Keep service follow-up within approved workbench boundaries.
- Avoid direct production sends until the external adapter allowlist is reviewed.

## Hermes Script Action Bridge

`bin/script_action_bridge.py` replaces the local Hermes core `script_action` patch for
the single `silaoshi-base-notify` route. The official Hermes `script` transform invokes
`bin/hermes_script_transform.py`, which submits a signed bounded payload to a local Unix
socket and returns `[SILENT]` after the runner durably accepts it.

The runner stores one row per delivery ID in a mode-0600 SQLite database. It executes
only the server-local action script whose SHA-256 is pinned in the root-owned config,
uses a maximum 900-second timeout, retries failures at most twice, and never replays a
successful action when its final status notification fails. Logs contain only the
delivery hash, attempt, return code, status, and notification status.

Real script paths, destination IDs, templates, and the HMAC key remain in
`/etc/qintopia`. `bin/migrate_script_action.py` derives those bindings from the current
server-local route and scripts without printing their values. Dry-run is the default;
`--apply` writes an atomic subscription update and a mode-0600 `.pre-bridge` backup;
`--rollback` restores that backup byte for byte.

The production service template is `systemd/qintopia-silaoshi-script-action.service`.
Obtain server access details from an administrator. Before applying the subscription
migration, install the reviewed service unit, start the runner, and confirm its Unix
socket is mode 0600.

## Production Boundary

### Local application readback branch

The optional `application_intake` config uses the existing signed Unix ingress. It
requires `local_only: true`, a fixed `resource_alias`, an exact `record_path` array and
`QINTOPIA_APPLICATION_LOCAL_ENABLE=1`. Without this config the old route is unchanged.
With it, callbacks durably wake readback by resource/record; a repeated record-only
delivery is not swallowed by the legacy jobs table. The branch never runs the legacy
action or notification command, including previously queued legacy jobs.

The fixed `skills/pms-operations/application_intake.py` host adapter reads a local
allowlisted source and commits through the independently authenticated foundation host
broker. Failed readback keeps the wake pending with bounded backoff; a newer wake during
processing remains pending. Only references and counters enter this queue. Content
dedupe and revision fencing belong to the shared PostgreSQL service.

This branch is local-only, not an installed replacement for the real Feishu Workflow.
Source callbacks do not authorize booking, payment, identity linking or welcome sends.
Silaoshi's `application_review` is an operating follow-up, not mandatory order approval.
Keep queued state when disabling; do not replay it through the old card/send script.

See the
[source/version design](../../runtime/postgres/docs/data-design/2026-09-24-application-event-intake.md).

### Existing production route

- The bridge may invoke only the pinned server-local resident action and notification
  adapter.
- The service runs as the unprivileged Hermes owner and writes only its socket, job
  database, and existing Silaoshi profile runtime paths.
- Hermes profile state changes only through the reviewed migration or rollback command.

## Acceptance Scenarios

- Daily operating state can be summarized from approved sources.
- Missing source evidence is reported as a review gap.
- Service follow-up creates a controlled work item instead of direct external action.
- Activity review links back to source records and owner.

## Validation

```bash
pnpm workflows:check
PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover \
  -s workflows/silaoshi-daily-ops/tests -v
```
