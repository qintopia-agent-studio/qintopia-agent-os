# 2026-07-31 Xiaoman Poster Review Hook Boundary

## Scope

The Xiaoman asynchronous poster return path needs to consume Feishu review-card actions
without sending the synthetic `/card` command through model dispatch. This remediation
is repository-only. It does not configure production secrets, activate the poster
services, call Feishu, approve an artifact, or create a group-send request.

## Evidence

Production-adjacent inspection found four boundary gaps:

- Hermes derives `chat_type=group` for the generic card event even when the Feishu SDK
  callback originated in a P2P conversation, so that derived field cannot establish the
  direct-message trust boundary.
- The default Hermes card logging path can include raw actor, chat, token, and dispatch
  identifiers before the plugin handles the event.
- The initial activation sequence enabled the callback path without restarting the
  Xiaoman gateway, so the newly installed hook was not guaranteed to be loaded.
- The first hook remediation enabled intake, callback, and starter units before the
  gateway restart completed, so a restart failure could leave a partial activation.
- A card callback could reach idempotency handling before all notification, artifact,
  conversation, actor, delivered-state, and decision bindings were revalidated.

No raw Feishu identifiers, tokens, callback keys, or card payloads are retained in this
report.

## Resolution

- Register a deterministic `pre_gateway_dispatch` hook in the versioned Xiaoman
  `qintopia-tools` plugin and accept only authentic SDK card objects with the fixed
  `xiaoman_poster_review` callback kind. Typed `/card` text cannot enter the path.
- Cross-check the SDK chat and operator fields, then forward a bounded signed envelope
  over the fixed local Unix socket. The sidecar matches the same conversation and actor
  against the restricted `poster_return_targets` row.
- Validate notification, artifact, conversation, actor, delivered state, and the
  expected decision before idempotency can return an existing review result.
- Install a narrow logging filter for matching card events so raw actor, chat, token,
  and dispatch identifiers are not emitted by the Hermes log path.
- Require both persistent enable flags, one matching callback key, and an immutable
  `release/current` plugin link before any activation side effect. After the preflight,
  restart and verify the Xiaoman gateway before enabling any workflow unit or timer.
- Stop delivery first during rollback, require both persistent flags to be exactly
  disabled, and restart Xiaoman so the hook is unloaded.

## Validation

The repository harness covers the shared Python/Rust callback fixture, a real temporary
Unix socket exchange, forged and mismatched callback rejection, actor and conversation
binding, duplicate decision binding, log redaction, typed-command rejection, and fake
systemd/runuser activation and rollback ordering. The activation fixture also proves a
mismatched plugin link fails before any systemd or gateway command is issued.

The Xiaoman plugin suite passed 68 tests, the default sidecar suite passed 453 tests,
and the all-features suite passed 459 tests with 16 guarded PostgreSQL tests ignored.
The PostgreSQL integration target compiled, and the deploy-contract checks passed. The
disposable PostgreSQL smoke and real Feishu direct-message exercise remain separate
evidence gates.

The activation fixture also forces the Xiaoman gateway restart to fail after preflight
and verifies that no intake, callback, starter, or delivery unit is enabled or
restarted.

## Remaining Boundary

A Release may install the reviewed units but must leave the poster path disabled. A
separate owner decision is required for production deployment, secret and allowlist
configuration, service activation, and the real Feishu acceptance. Image approval still
does not authorize publication or group delivery.
