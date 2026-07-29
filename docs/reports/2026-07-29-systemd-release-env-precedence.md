# Systemd Release Environment Precedence

Date: 2026-07-29

## Summary

Release `v0.2.50` and its required same-SHA follow-up deployment both completed
successfully for commit `ea01fa5db722da9132eca405635f8d0896b4aba1`. The server promoted
that commit and installed its systemd units, but the Huabaosi image-generation preflight
failed closed with `config_valid=false` and no missing configuration.

Huabaosi image generation, Huabaosi Feishu mirroring, and QiWe image sending remained
disabled and inactive. No image provider, Feishu write, QiWe upload, or group send was
executed during diagnosis.

## Evidence

- GitHub Actions runs `30416859395` and `30417363476` completed successfully for the
  same release commit.
- `release/current` resolved to the released commit and `release/previous` remained a
  distinct rollback target.
- The deploy-runner timer and sidecar service were healthy. Four internal Xiaoman timers
  were enabled; all three external action timers remained disabled.
- The installed image preflight unit declared the current release SHA, but its process
  received older values from `/etc/qintopia/message-sidecar.env`.
- A fixed, no-network preflight under both the installed `ubuntu` unit and an equivalent
  root transient unit failed identically. This excluded user permissions as the cause.
- The Feishu mirror preflight passed only because its stale deployed and Feishu release
  SHA values matched each other. That result did not prove binding to `release/current`.

## Root Cause

The renderer placed immutable release identity in systemd `Environment=` directives
while also loading `/etc/qintopia/message-sidecar.env` with `EnvironmentFile=`. Systemd
gives values from `EnvironmentFile=` precedence over `Environment=` regardless of their
textual order in the unit.

The persistent file still contained historical deployed, image-release, and
Feishu-release SHA values. Image generation compared two different historical SHAs and
failed closed. Feishu mirroring compared two equal historical SHAs and produced a false
ready signal. Other services could also observe a stale deployed SHA even when their
binary and working directory came from the current immutable release.

## Remediation

- Bind `QINTOPIA_DEPLOYED_COMMIT_SHA` at the final `ExecStart` and `ExecStartPre`
  boundary through fixed `/usr/bin/env` assignments.
- Bind Huabaosi image and Feishu release SHAs at the same boundary for only their owned
  units.
- Keep secrets and runtime-local configuration in the existing environment file; do not
  copy them into unit files or command arguments.
- Keep external timers disabled. Do not edit the production environment file or
  installed systemd units directly.
- Add renderer and installation checks that reject release identity expressed through
  vulnerable `Environment=` directives.

## Validation

The focused repository checks are:

```bash
deploy/sidecar/scripts/render-systemd-units.sh \
  --target-sha ea01fa5db722da9132eca405635f8d0896b4aba1 \
  --check
node tools/deploy/test-release-systemd-install.mjs
node tools/deploy/check-release-model.mjs
node tools/deploy/check-deploy-contracts.mjs
node tools/deploy/check-deploy-runner.mjs
```

After the reviewed fix is released, the required production acceptance is:

1. Confirm the new release and any required same-SHA follow-up deployment succeed.
2. Confirm rendered Huabaosi units bind the current SHA at the exec boundary.
3. Run the image-generation and Feishu-mirror systemd preflights and require both to
   pass against the current release.
4. Keep external timers disabled until the existing guarded activation sequence is
   explicitly approved.

## Rollback

Before activation, rollback is to leave all external timers disabled and retain the
previous immutable release pointer. If the corrected unit installation fails, the deploy
runner must fail closed and preserve or restore the prior unit set through its existing
transaction; operators must not hot-edit `/etc/systemd/system` or the persistent
environment file.
