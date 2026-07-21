# Erhua Livecool Profile Overlay Runbook

This runbook governs the first production Hermes profile overlay. It repairs Erhua's
missing `Livecool.net` provider registration without replacing the profile directory or
copying live runtime state into a release.

## Managed Contract

The release carries only `agents/erhua/config.template.yaml`. The renderer changes these
fields and no others:

- `model.default: gpt-5.5`
- `model.provider: custom:livecool.net`
- `model.base_url: ""`
- the single custom provider named `Livecool.net`, with `https://livecool.net/v1`,
  `gpt-5.5`, `key_env: LIVECOOL_API_KEY`, and `api_mode: chat_completions`

The live Erhua `config.yaml` remains runtime-local. Rendering starts from that file,
preserves unrelated keys and providers, and writes a replacement atomically with mode
`0600` while preserving the original owner. A duplicate provider, YAML alias, malformed
document, unexpected overlay field, or aliased profile path fails closed.

## Credential Boundary

`LIVECOOL_API_KEY` is server-only. It must never enter Git, a release artifact, a deploy
request, a diff, or a log. The fixed migration checks Erhua's existing `.env` first. If
the binding is absent, it reads the one existing `Livecool.net` inline credential from
the default Hermes profile and writes only the environment binding into Erhua's staged
`.env`. It does not print the value, its length, or a hash.

The migration fails before activation when the source is missing, duplicated, empty, or
conflicting. Sourcing a new credential is outside this runbook and requires owner
approval.

## Two-Stage Bootstrap

The installed runner cannot recognize `hermes-profile-erhua`, so the first rollout uses
three separately approved requests:

1. Publish and verify the reviewed deploy bundle. Submit an existing `deploy-bundle`
   scope request that upgrades `release/current` and therefore the runner used by the
   next poll. Do not include `hermes-profile-erhua` in this request.
2. At the owner checkpoint, install the runner unit from that verified immutable release
   into `/etc/systemd/system`, after backing up the installed unit under
   `/var/lib/qintopia-agent-os-deploy/runner-unit-backups/<release-sha>/`. Run
   `systemd-analyze verify`, `systemctl daemon-reload`, and confirm root `python3`
   imports PyYAML and `systemctl show` reports
   `ReadWritePaths=/home/ubuntu/.hermes/profiles/erhua`. Restore the backup and reload
   systemd if any verification fails. This is deployment of an approved release file,
   not a server-side edit.
3. Submit `release_scope: [hermes-profile-erhua]`, `restart_targets: [hermes-erhua]`,
   and `dry_run: true`. Review the redacted changed paths, prerequisite checks, and
   candidate hashes. The dry run must also resolve the candidate through the installed
   Hermes interpreter. This request does not change `release/current`, `config.yaml`,
   `.env`, or the service. Record its deploy request ID.
4. After explicit owner approval, submit the same fixed scope and target with
   `dry_run: false`, `rollback_on_smoke_failure: true`, and `profile_dry_run_request_id`
   set to that reviewed request. Activation must occur within 24 hours. The request SHAs
   and current config/env hashes must match the exact reviewed dry-run marker.
   Publishing the draft GitHub Release remains a separate owner action.

The runner rejects any request that combines `hermes-profile-erhua` with another scope
or restart target, disables rollback, or targets a release other than `current`.
Profile-only requests do not promote a release or modify `current`/`previous`. Request
data cannot provide profile paths.

The runner-unit bootstrap is required because the installed unit is a static root-owned
file, not a symlink into `release/current`. Switching the release alone updates the
scripts but does not change the active systemd sandbox.

## Activation And Evidence

Before replacing anything, the runner records the current release target and creates a
request-specific `0700` backup directory containing `0600` copies of Erhua `config.yaml`
and `.env`. Metadata records only paths, ownership, modes, and SHA-256 hashes. The
candidate environment is installed before the candidate config so interruption cannot
activate a provider before its binding exists; each replacement is atomic.

Successful evidence contains:

- request, release, and previous release identifiers;
- the fixed scope and restart target;
- redacted changed paths, config hashes, file modes, and ownership;
- environment binding status without a credential-derived hash;
- static provider and environment-binding validation;
- active state for `hermes-gateway-erhua.service`;
- affirmative resolution by the installed Hermes runtime's own provider resolver;
- exact activated config and environment hashes, modes, and ownership revalidated before
  success;
- Hermes doctor success; and
- confirmation that new service logs do not contain
  `Unknown provider 'custom:livecool.net'`.

Smoke does not call a model and does not send a QiWe message. The change touches Hermes
profile runtime, a server-local secret binding, and Erhua's user systemd service. It
does not send externally, write the business database, or alter Feishu.

The uploaded result records `release_scope`, `restart_targets`, the authorizing dry-run
request ID, activation or rollback smoke phase, and separate restore evidence.
Server-only environment hashes remain in private dry-run/transaction state and are not
exported.

## Rollback

Any activation or smoke failure restores both profile files from the request backup,
verifies their original hashes, modes, and ownership, verifies that the approved current
release remains unchanged, restarts only `hermes-gateway-erhua.service`, and runs the
rollback-safe non-sending service smoke. Rollback is successful only if every restore
and check succeeds.

Keep the backup and deploy result as operational evidence until the release is accepted.
Do not manually edit `.hermes` to repair a failed rollout.

## Validation

```bash
pnpm runtime:hermes:check
pnpm agents:profile-bundles:check
pnpm deploy:runner:check
pnpm artifact:deploy-bundle
pnpm secrets:check
pnpm check
pnpm pr:doctor
```
