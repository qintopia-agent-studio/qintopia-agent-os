# Xiaoman Group Send-Ready Observation Record

Date: 2026-07-12

## Scope

The Xiaoman internal workflow can reach a human-confirmed, queued
`group_message_request`. The group send-ready timer then records an internal
`send_executed=false` audit event. This change adds only a read-only production
observation for that timer.

## Finding And Resolution

| Finding                                                                                                                                           | Classification             | Resolution                                                                                                | Prevention                                                                                                  |
| ------------------------------------------------------------------------------------------------------------------------------------------------- | -------------------------- | --------------------------------------------------------------------------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| The timer was rendered and deployable, but the aggregate Xiaoman preflight did not observe its active state, fixed command, or sanitized journal. | Production observation gap | Add `operations-group-send-ready-timer-observation-smoke.sh` and compose it into the aggregate preflight. | The Xiaoman readiness check requires the observation script and the preflight record requires its evidence. |

## Safety Boundary

- The observation script reads `systemctl` state, rendered units, timer listing, and
  recent journal output only.
- It does not run `run-group-message-send-worker`, record final confirmation, write
  Postgres, call QiWe, or send externally.
- The deployed timer remains limited to a local AgentOS send-readiness audit with
  `send_executed=false`; it is not a QiWe production send adapter.

## Validation Evidence

- `bash -n deploy/sidecar/scripts/operations-group-send-ready-timer-observation-smoke.sh`
- `deploy/sidecar/scripts/render-systemd-units.sh --check`
- `node tools/deploy/check-xiaoman-preflight-readiness.mjs`
- `pnpm check:light`

Production evidence still requires an owner-approved deployment followed by the
aggregate read-only preflight. Do not treat a rendered systemd unit or local static
check as proof that the production timer is active.
