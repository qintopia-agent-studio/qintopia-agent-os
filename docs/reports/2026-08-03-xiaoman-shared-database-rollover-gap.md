# Xiaoman Shared Database Rollover Gap

Date: 2026-08-03

## Summary

Release `v0.2.67` deployed successfully and remained disabled by default. The first
same-SHA dry run covered only the nine system services. A follow-up read-only process
inventory found that the same production database credential was also held by the Erhua
Hermes gateway and one of its child processes. The fixed Erhua profile environment
contained a second persistent copy of that credential.

An expanded same-SHA dry run requested both `qintopia-system-services` and
`hermes-erhua`, but the immutable existing Release manifest rejected the changed restart
target set before promotion or restart. Production poster ingress, review, direct
delivery, and group delivery remained disabled. No Feishu call, business database write,
service reload, or activation was performed.

Release `v0.2.68` was subsequently published from `c5f9dcc` and its production deploy
succeeded. It advanced `release/current`, but the signed request still contained only
`qintopia-system-services`; it did not include this remediation or reload Erhua. A
post-deploy read-only check confirmed that every Xiaoman poster feature flag remained
`0` and the notification, review-callback, direct-delivery, and group-delivery units
remained inactive and disabled. The successful Release therefore preserves the safe
disabled state but does not make the direct poster flow ready for testing.

## Evidence

- Release deploy run `30780095491` succeeded for
  `9791752068caa3a66ea2f414bf1c30321cca3fe0`.
- System-only same-SHA dry-run `30784624982` succeeded.
- Expanded dry-run `30785598682` failed during `promote-release` with
  `existing release manifest restart_targets mismatch`; `promoted_current=false` and
  rollback was not needed.
- Release deploy run `30787033670` succeeded for `v0.2.68` at `c5f9dcc`; its retained
  request recorded `dry_run=false` and only `qintopia-system-services` in
  `restart_targets`.
- The production `release/current` link resolved to `c5f9dcc`; both Xiaoman and Erhua
  gateways were active, while all poster intake/delivery flags were `0` and all poster
  notification, callback, direct-delivery, and group-delivery units were inactive and
  disabled.
- Two consecutive sanitized process snapshots found nine matching system-service
  processes and two matching processes in `hermes-gateway-erhua.service`.
- A sanitized fixed-path inventory found the live credential in
  `/etc/qintopia/message-sidecar.env` and `/home/ubuntu/.hermes/profiles/erhua/.env`.
- The protected Xiaoman configuration transaction updated only the sidecar and Xiaoman
  environment files, so it could not retire the old credential safely.
- The candidate one-off rollover guardian was rejected by independent review before
  execution because its escrow was under volatile `/run` and its recovery state was
  incomplete.
- The final read-only database gates reported zero direct notifications, attempts,
  return targets, non-terminal workflows, starter backlog, and enabled policies.

All probes emitted only counts, booleans, file paths, modes, Release identities, and
opaque hashes. They did not emit database URLs, passwords, chat or user ids, Feishu
payloads, or application credentials.

## Root Cause

The closeout contract treated the production database URL as a sidecar and Xiaoman
configuration value, but the same credential also crossed the Erhua profile boundary.
The protected configuration transaction therefore did not cover every persistent
credential holder. The Release restart manifest was correctly immutable, but the Release
had been created with only the system-service target, so a later same-SHA request could
not broaden the restart set to include Erhua.

The attempted operational workaround also lacked a durable recovery escrow. A host
restart after PostgreSQL accepted the new password but before configuration persisted it
could have made the replacement credential unrecoverable.

## Resolution

- Extend the fixed protected configuration transaction to include the Erhua profile
  environment when and only when the database URL changes or needs reconciliation.
  Accept only the approved previous or successor binding, reverse-restore ordinary
  replacement failures, and converge an interrupted mixed state on exact retry.
- Ship the password rollover state machine in the immutable Release. Keep its secret
  escrow under a root-only persistent state directory, bind it to owner-approved opaque
  targets and Release artifacts, distinguish authentication rejection from transport
  failure, and retain a sanitized terminal receipt.
- Route changes to the shared configuration and rollover entrypoints to both
  `qintopia-system-services` and `hermes-erhua`, so the first deploy and every same-SHA
  reload use the same immutable restart target set.
- Require the exact published-Release scope and dual-target same-SHA dry run to pass and
  receive owner review before `prepare` invalidates the previous credential. The
  rollover entrypoint must validate the processed request id and root-owned successful,
  non-rollback result before it creates state or accesses PostgreSQL. The live dispatch
  then differs only by `dry_run=false` and starts immediately after prepare.
- Preserve a tested exact-SHA recovery promotion: if a failed live smoke restores
  `current` to `previous`, re-promote the same immutable identity before invoking the
  rollover status or rollback command.
- Reconcile and remove bounded staged secret files under the protected configuration
  lock and rollover lock before a terminal receipt can claim secret cleanup.
- Keep all Xiaoman poster units stopped until rollover, reload, private-policy apply,
  and the no-network preflight have all passed.

## Validation

Run focused transaction, rollover, and restart-routing tests first. Then run:

```bash
pnpm deploy:contracts:check
pnpm deploy:runner:check
pnpm check:pr:auto
```

Production acceptance requires a pre-rotation dry run and live same-SHA reload with the
identical published scope and dual target set, proof that no process retains the
previous database URL, one exact private policy, a successful no-network preflight, and
a separate owner-approved direct activation. Group delivery and every publication fact
must remain zero.

## Rollback

Before direct activation, rollback keeps every Xiaoman poster unit disabled. If the
password has changed, retain the replacement credential, apply the disabled
configuration through the protected transaction, reload both fixed target families, and
verify that no process retains the prior URL. Preserve the durable rollover receipt and
all database audit facts; do not restore profile files manually or reroute any result to
another conversation.
