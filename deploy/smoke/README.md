# Smoke Checks

This package is the future common entrypoint for cross-profile, MCP, worker, and
deployment smoke checks.

Existing sidecar-specific smoke scripts remain in `deploy/sidecar/scripts/` until they
are wrapped here without changing behavior.

## Scope

- release manifest validation
- profile bundle dry-run validation
- MCP command resolution checks
- worker binary availability checks
- read-only external adapter preflight

Smoke checks must be safe by default. Real external sends require separate owner review,
allowlists, and explicit runtime configuration.

## Huabaosi WeCom Gateway Observation

`deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh` is the phase-2
read-only observation smoke for the 阿亮画报师 WeCom migration. It inspects the active
Hermes user gateway service through `systemctl --user`, its fixed command shape, public
`busy_input_mode`, release/current presence, and sanitized user-journal marker counts
for internal filtering, send fallback, and API timeouts.

Run it only after an owner-approved deploy from the immutable release directory:

```bash
QINTOPIA_HUABAOSI_WECOM_OBSERVATION_ENABLE=1 \
  deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh
```

It does not source `.env`, restart services, run generation, print raw journal lines,
send WeCom messages, write Postgres or Feishu, call QiWe/provider/media endpoints, or
modify live Hermes profile state.

## Huabaosi WeCom Canary Observation

`deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh` is the phase-5
disabled-state observation for the canary gateway. It verifies that no canary gateway
service/timer is installed, active, or enabled, then runs
`huabaosi-wecom-canary-preflight` and checks only sanitized JSON fields.

```bash
QINTOPIA_HUABAOSI_WECOM_CANARY_OBSERVATION_ENABLE=1 \
  deploy/sidecar/scripts/huabaosi-wecom-canary-observation-smoke.sh
```

It does not source `.env`, read stdin, run `--apply`, send WeCom messages, write
Postgres or Feishu, call QiWe/provider/media endpoints, install units, run image
generation, or modify live Hermes profile state. A real canary send requires a separate
owner-reviewed staging command with exact allowlists and the non-default Cargo feature.
When invoked from an immutable release, it automatically uses
`release/current/sidecar/qintopia-message-sidecar`; source checkouts continue to use the
local Cargo fallback unless `QINTOPIA_SIDECAR_BIN` is set explicitly.

## Xiaoman Production Preflight

After an owner-approved deploy, run the aggregate Xiaoman production preflight from the
sidecar scripts and record the sanitized result in
[`docs/xiaoman-production-preflight-record.md`](docs/xiaoman-production-preflight-record.md).
The record captures timer health, fixed service commands, read-only preview counts,
secret-scan results, and the pass/hold decision. This includes read-only observations of
the image-generation request starter, the disabled Huabaosi provider worker, and the
group send-ready timer. The preflight runs the provider worker only with
`--once --dry-run` and does not run the send-ready worker. It is not a release approval
and does not authorize Feishu writes, QiWe sends, poster publishing, or external
adapters.

An explicit `no_claimable_*` worker result is a valid empty-queue observation only when
it reports `dry_run=true`, `apply_requested=false`, and empty artifact ids/previews.
When a worker returns an actual preview, its limitations or guardrails must still state
the external-adapter boundary. A release that changes the deploy runner's systemd unit
allowlist needs one owner-approved follow-up deployment for the same SHA because its
first promotion is processed by the previous release runner.

## Validation

```bash
pnpm deploy:smoke:check
pnpm check:light
```
