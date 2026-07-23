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

## Xiaoman Profile Bundle Observation

`deploy/sidecar/scripts/xiaoman-profile-bundle-observation-smoke.sh` renders the
observation-only Xiaoman bundle into a temporary directory, verifies the reviewed live
source hashes, and requires byte-for-byte `SOUL.md` and `profile.yaml` parity.

After an owner-approved Release deploy, prepare the fixed values JSON once:

```bash
sudo env \
  QINTOPIA_XIAOMAN_PROFILE_VALUES_MIGRATION_APPROVAL=approved-xiaoman-profile-values-migration \
  /home/ubuntu/qintopia-agent-os-releases/current/agents/xiaoman/profile-bundle/migrate_values.py \
  --apply
```

The command fails before reading the live profile without the exact approval, verifies
both reviewed source hashes and complete rendered parity, and never replaces an existing
values file. It does not edit the live profile or activate the bundle.

```bash
sudo env QINTOPIA_XIAOMAN_PROFILE_BUNDLE_OBSERVATION_ENABLE=1 \
  /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-profile-bundle-observation-smoke.sh
```

The default production values JSON must be root-owned and inaccessible to group/world,
and the smoke must run as root. A custom fixture path used by local tests must instead
be owned by that test process. The smoke does not print values, create symlinks, edit
the live profile, restart Hermes, write a database, use the network, or send externally.
It is not a cutover approval.

## Huabaosi WeCom Gateway Observation

`deploy/sidecar/scripts/huabaosi-wecom-gateway-observation-smoke.sh` is the phase-2
read-only observation smoke for the 阿亮画报师 WeCom migration. It inspects the active
Hermes user gateway service through `systemctl --user`, its fixed command shape, public
`busy_input_mode`, release/current presence, and sanitized user-journal marker counts
for internal filtering, send fallback, and API timeouts. The journal scan is fixed to
the most recent 30 minutes and at most 160 lines so failures from an earlier Release do
not make every later observation fail. Commands, production paths, and the journal
window are fixed; callers cannot replace them with test doubles or alternate state.

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
`--once --dry-run`, verifies the configured Huabaosi production timer state, and does
not run the send-ready worker. It is not a release approval and does not authorize
Feishu writes, QiWe sends, poster publishing, or external adapters.

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
