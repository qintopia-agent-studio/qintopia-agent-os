# Erhua identity contract

These are retained operating constraints, not a new permission grant. Read the sections
relevant to the change before editing. Original conditions and exceptions remain
binding; summaries in AGENTS.md do not replace them.

Backtick paths in root-source rules are repository-relative; paths in Sidecar-source
rules retain their original `runtime/sidecar/` base. New navigation links are relative
to this document.

[Migration inventory](../../../docs/plans/active/agents-guidance/README.md) records the
baseline and source locations.

<a id="root-045"></a>

## Commands — root-045

<!-- preserved-rule: root-045 -->

- Erhua member recognition local release-current readiness check:
  `node tools/deploy/check-erhua-member-recognition-local.mjs`. This proves the
  release-current runbook, deploy bundle files, focused Rust tests, fixture checkers,
  and completion finalizers are present; it does not prove production DB completion.

<!-- /preserved-rule: root-045 -->

<a id="root-046"></a>

## Commands — root-046

<!-- preserved-rule: root-046 -->

- Erhua member recognition reviewed production config apply:

  ```bash
  QINTOPIA_ERHUA_MEMBER_RECOGNITION_PRODUCTION_CONFIG=approved-production-erhua-member-recognition-config \
    deploy/sidecar/scripts/apply-erhua-member-recognition-production-config.sh --apply
  ```

  Pass `QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CHAT_ID` and
  `QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID` only from reviewed
  server-local values. Do not print, retain, paste, or commit the real group id or
  sender id.

<!-- /preserved-rule: root-046 -->

<a id="root-047"></a>

## Commands — root-047

<!-- preserved-rule: root-047 -->

- Erhua member recognition production config observation:
  `QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/erhua-member-recognition-production-config-observation-smoke.sh`.
  Continue only when it reports `action_status=ready_for_member_recognition_runbook`.

<!-- /preserved-rule: root-047 -->

<a id="root-048"></a>

## Commands — root-048

<!-- preserved-rule: root-048 -->

- Erhua member recognition room roster sync evidence:
  `qintopia-message-sidecar identity-backfill --sync-room-members --chat-id <reviewed-erhua-qiwe-group-id> --apply`
  then
  `node tools/deploy/check-erhua-room-member-sync.mjs <identity-backfill-room-member-sync-output.json>`.

<!-- /preserved-rule: root-048 -->

<a id="root-049"></a>

## Commands — root-049

<!-- preserved-rule: root-049 -->

- Erhua member recognition coverage and completion evidence:
  `node tools/deploy/finalize-erhua-member-recognition-coverage.mjs` and
  `node tools/deploy/finalize-erhua-member-recognition-completion.mjs`. Retained
  evidence must keep only sanitized counts, route-level hint coverage, and
  `scope_fingerprint`; never retain real group ids, QiWe user ids, sender ids, person
  ids, DB URLs, tokens, raw messages, or raw profile text. Final completion must retain
  `unsafe_display_unlinked = 0` so numeric or otherwise unsafe current-room display
  names cannot disappear into `excluded`. It must also retain
  `qiwe_speaker_identities.platform_identities_missing = 0` and
  `qiwe_speaker_identities.ambiguous_users = 0`, proving current-room QiWe users are
  speaker-ready for "我是谁" lookup. Profile repair evidence must also retain
  `profile_repair.current_room_linked_people = linked_people.total`; linked current-room
  people without useful profile signals should receive an active no-stable-profile
  `reply_context` snapshot with `do_not_infer_missing_profile=true`, not remain
  identity-only.

<!-- /preserved-rule: root-049 -->

<a id="root-050"></a>

## Commands — root-050

<!-- preserved-rule: root-050 -->

- Erhua member recognition roster audit evidence:
  `node tools/deploy/build-erhua-member-recognition-roster-audit.mjs`. It must derive
  only from sanitized coverage, canary, and completion-summary evidence and may retain
  safe names, canonical keys, `person_ref` hashes, profile status, required-term
  matches, and route canary booleans; never rebuild it from raw DB rows, raw group
  messages, real QiWe ids, person UUIDs, or raw profile text.

<!-- /preserved-rule: root-050 -->

<a id="root-172"></a>

## Core Rules — root-172

<!-- preserved-rule: root-172 -->

- Erhua member recognition depends on `qintopia_identity.channel_identities.person_id`
  and active `member_profile_snapshots`, not display-name guessing. The identity worker
  must not skip QiWe messages that already have `sender_channel_identity_id` and
  `sender_name` but still have `sender_person_id IS NULL`; otherwise Erhua will call
  `qintopia_answer_context_prepare` and correctly return `speaker_unresolved` even when
  the display name uniquely matches an existing person/profile. The member-profile
  repair must seed active no-stable-profile `reply_context` snapshots for linked
  current-room people that have no useful profile facts yet, so "known member but no
  stable profile" is a database-backed state rather than an identity-only fallback.
  Running profile hints must cover community event language such as `跑步局`, `约跑`,
  route, pace, or `km` context, while avoiding object-only chatter such as running
  shoes.

<!-- /preserved-rule: root-172 -->

<a id="sidecar-007"></a>

## Commands — sidecar-007

<!-- preserved-rule: sidecar-007 -->

- Erhua current room roster sync:
  `cargo run -- identity-backfill --sync-room-members --chat-id <reviewed-erhua-qiwe-group-id> --dry-run`
  Retained evidence may keep `scope_fingerprint`; never retain the raw group id or QiWe
  user ids.

<!-- /preserved-rule: sidecar-007 -->

<a id="sidecar-008"></a>

## Commands — sidecar-008

<!-- preserved-rule: sidecar-008 -->

- Erhua scoped member profile refresh:
  `cargo run -- member-profile --chat-id <reviewed-erhua-qiwe-group-id> --apply --quiet`
  Retained evidence may keep aggregate counts and `scope_fingerprints`; never retain raw
  chat ids or candidate facts.

<!-- /preserved-rule: sidecar-008 -->

<a id="sidecar-009"></a>

## Commands — sidecar-009

<!-- preserved-rule: sidecar-009 -->

- Erhua speaker self-canary private sender map:
  `cargo run -- erhua-member-speaker-canary-sender-map --chat-id <reviewed-erhua-qiwe-group-id>`.
  Its output contains raw QiWe sender ids; keep it as a server-local temporary file only
  and never retain it as evidence.

<!-- /preserved-rule: sidecar-009 -->
