# 2026-07-31 Xiaoman Poster Systemd Install Gap

## Scope

Release `v0.2.60` contains the disabled-by-default Xiaoman asynchronous poster intake,
notification, Feishu delivery, and review callback runtime. A production-readiness audit
after the guarded Bootstrap dry run found that the release renderer and installer did
not share the same fixed unit allowlist.

This remediation changes repository release assembly only. It does not edit the
production server, switch `release/current`, run migrations, enable a timer, call
Feishu, or send a group message.

## Evidence

`deploy/sidecar/scripts/render-systemd-units.sh` rendered and validated all seven units:

- `qintopia-agentos-operations-intake.service`;
- `qintopia-agentos-xiaoman-poster-notification-starter.service` and `.timer`;
- `qintopia-agentos-xiaoman-feishu-poster-preflight.service`;
- `qintopia-agentos-xiaoman-feishu-poster-delivery.service` and `.timer`; and
- `qintopia-agentos-xiaoman-poster-review-callback.service`.

`deploy/runner/install-release-systemd-units.sh` did not include those names in its
fixed `unit_files` array. A normal Release could therefore render the files successfully
without installing them into the system unit directory. The guarded production
activation script would then fail when it tried to start the missing preflight, intake,
callback, starter, and delivery units.

## Resolution

- Add all seven rendered units to the release install allowlist.
- Extend the deploy-runner static contract check with every new unit name.
- Extend the release systemd installation test to prove every unit is installed.
- Keep all seven outside the installer's internal timer enablement list and assert that
  the installer issues no `systemctl enable` command for them.

External delivery remains disabled after a Release. It still requires persistent
owner-reviewed Feishu configuration, release and database bindings, exact chat and user
allowlists, callback configuration, a successful preflight, and the explicit production
activation command.

## Validation

The focused release installation test, deploy-runner contract suite, shell syntax check,
repository diff check, and risk-tiered PR checks are the acceptance path for this
repair.

## Rollout And Rollback

Publish and deploy a new Release containing this repair; do not reuse `v0.2.60` as the
final target. Release installation places the units but does not activate the Xiaoman
poster path. If later explicit activation fails, use the existing guarded rollback
script, which disables delivery first and retains workflow and audit state.
