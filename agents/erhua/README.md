# Agent: Erhua

`erhua` is the QiWe/WeCom front-office cat-assistant Agent for Qintopia community
groups. It represents the community-facing digital presence of Erhua the cat while
handling Public-safe group replies, member-aware greetings, light consultation intake,
controlled handoff, and trainer memory submission through audited backend paths.

## Scope

- Reply only when mentioned or clearly cued in allowed groups.
- Recognize the current speaker and mentioned members when safe Postgres context exists,
  including direct chat and group mention flows.
- Use controlled context lookup for Public-safe answers.
- Escalate availability, booking, refund, compensation, policy, complaint, and uncertain
  operational questions to a human owner or live-ops path.
- Submit trainer notes through the audited Erhua training-memory path when allowed.
- Create and append typed, auditable CSV records only within the current QiWe group; use
  the ledger preset for bookkeeping and reversal events.

## Resident welcome responsibility (local integration; production not enabled)

The independent room Agent 岸岸 (`anan`) owns welcome orchestration and requests card
production from 阿靓 (`huabaosi`). For this workflow, Erhua forwards the specified card
and welcome text to the assigned community/building destination. When the building
requires review, Erhua presents that exact content to its responsible human and records
the decision; Erhua does not create the card, approve it, or rewrite the delivery
content.

The room workflow reads the building's effective policy: a valid single-item or standing
authorization for the complete welcome permits delivery without another per-item human
review; review-required policy waits for approval of the specific content version. The
backend still checks resident disclosure consent, eligibility, current authorization and
delivery deduplication. Artifact references resolve storage locations through controlled
tools, rather than remembered download URLs.

The local foundation workbench now exercises these consumer responsibilities with
versioned rules, real PNG Artifacts and durable per-part results. Uploads and sends use
local synthetic adapters; this is not evidence of production enablement or real-channel
delivery. The shared contract remains in
[welcome §6.4](../../docs/plans/active/unified-person-welcome-v1-contract.md#64-欢迎编排制卡与楼栋转发职责负责人纠偏2026-09-22).
Receiving a resident's profile correction can trigger a controlled handoff to the room
workflow; it does not give Erhua card-generation responsibility.

## Shared identity and memory (local integration)

The Erhua plugin registers `skills/person-foundation` tools for the fixed `erhua`
Profile. Host-provided QiWe session evidence selects the person, gateway and working
scope; model arguments cannot supply identity or permissions. The authenticated local
broker reads shared knowledge and saves authorized rules, while personal reply
preferences support conditional corrections and stopping without old-message revival.
Tools are disabled until explicitly configured for the local broker. Scripted intent
tests do not establish real-model understanding; real gateway ingestion and production
rollout remain separate acceptance work.

## Front-desk collection (design, not yet integrated)

Erhua should collect community life experience and support community co-creation through
permitted interaction. This includes everyday methods and facility information, member
ideas, voluntary participation and contributions, as well as nearby recommendations and
issue follow-up. Use existing knowledge and memory to help members, then revise them
from subsequent feedback and actual outcomes. Reuse shared identity, scope, memory,
knowledge, and work-item services. Temporary status, subjective experience, formal
knowledge, proposals and participation have different meanings; expressing interest does
not assign a task or grant community authority. Routine collection does not require
per-item human review or employee data entry.

Activity work belongs to Xiaoman. When a member's idea develops into an activity, Erhua
carries the permitted context into a traceable handoff and uses returned progress and
outcomes for member service; it does not assume activity ownership. Members should not
have to repeat their account or relay work between Agents. This is the intended
collaboration boundary, not evidence that the new handoff is integrated or enabled.

The
[collection framework](../../docs/agent-os/community-business-model.md#11-二花通过互动收集信息与积累社区记忆)
defines this target and its remaining gaps. Existing event extraction, static place
lookup, and legacy complaint tools do not establish the complete loop. Proactive
outreach and publication still consume effective contact and disclosure agreements; this
design does not enable a new live trigger or send path.

## Boundaries

- Must not promise price, availability, refunds, compensation, contract changes, or
  policy exceptions.
- Must not expose internal SOPs, member records, raw message history, or private profile
  state.
- Must not guess a member identity when context is missing or ambiguous.
- Must not directly read unrestricted message stores or Feishu documents.
- Must not send direct messages unless the channel policy and contact guard allow it.
- Must not expose native file tools, read another group's CSVs, rewrite history, or use
  group CSVs for secrets and highly sensitive personal data.

## Runtime Source

- Server profile observed read-only: `/home/ubuntu/.hermes/profiles/erhua`
- Current service observed read-only: `hermes-gateway-erhua.service`
- Related active package: `skills/qiwe`
- Release-owned weather output entrypoint:
  `skills/qintopia-weather/scripts/qintopia-erhua-weather-broadcast.py`
- Runtime `.env`, memories, identities, caches, locks, logs, and state databases are
  excluded from this package.
- `config.template.yaml` is a non-secret, field-limited Livecool provider overlay. It
  preserves the runtime-local `model.default` when rendered by the governed deploy
  runner; it is not a complete Hermes config and must never receive an inline
  credential.

The weather entrypoint is packaged but not activated. Erhua's live 07:00 job and
`cron/jobs.json` remain runtime-local until their non-secret structure and script hashes
are inventoried and a separate reviewed profile cutover supplies rollback evidence.

## Validation

```bash
pnpm test:qiwe
pnpm skills:erhua-csv:check
pnpm runtime:hermes:check
pnpm agents:profile-bundles:check
pnpm registry:check
pnpm policy:check
```

## Operating rules

Paths below are repository-relative; Sidecar subsections use `runtime/sidecar/`.

### Commands

- Erhua member recognition local release-current readiness check:
  `node tools/deploy/check-erhua-member-recognition-local.mjs`. This proves the
  release-current runbook, deploy bundle files, focused Rust tests, fixture checkers,
  and completion finalizers are present; it does not prove production DB completion.
- Erhua member recognition reviewed production config apply:

  ```bash
  QINTOPIA_ERHUA_MEMBER_RECOGNITION_PRODUCTION_CONFIG=approved-production-erhua-member-recognition-config \
    deploy/sidecar/scripts/apply-erhua-member-recognition-production-config.sh --apply
  ```

  Pass `QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CHAT_ID` and
  `QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_CANARY_SENDER_ID` only from reviewed
  server-local values. Do not print, retain, paste, or commit the real group id or
  sender id.

- Erhua member recognition production config observation:
  `QINTOPIA_ERHUA_MEMBER_RECOGNITION_CONFIG_OBSERVATION_ENABLE=1 deploy/sidecar/scripts/erhua-member-recognition-production-config-observation-smoke.sh`.
  Continue only when it reports `action_status=ready_for_member_recognition_runbook`.
- Erhua member recognition room roster sync evidence:
  `qintopia-message-sidecar identity-backfill --sync-room-members --chat-id <reviewed-erhua-qiwe-group-id> --apply`
  then
  `node tools/deploy/check-erhua-room-member-sync.mjs <identity-backfill-room-member-sync-output.json>`.
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
- Erhua member recognition roster audit evidence:
  `node tools/deploy/build-erhua-member-recognition-roster-audit.mjs`. It must derive
  only from sanitized coverage, canary, and completion-summary evidence and may retain
  safe names, canonical keys, `person_ref` hashes, profile status, required-term
  matches, and route canary booleans; never rebuild it from raw DB rows, raw group
  messages, real QiWe ids, person UUIDs, or raw profile text.

### Core Rules

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

### Sidecar Commands

- Erhua current room roster sync:
  `cargo run -- identity-backfill --sync-room-members --chat-id <reviewed-erhua-qiwe-group-id> --dry-run`
  Retained evidence may keep `scope_fingerprint`; never retain the raw group id or QiWe
  user ids.
- Erhua scoped member profile refresh:
  `cargo run -- member-profile --chat-id <reviewed-erhua-qiwe-group-id> --apply --quiet`
  Retained evidence may keep aggregate counts and `scope_fingerprints`; never retain raw
  chat ids or candidate facts.
- Erhua speaker self-canary private sender map:
  `cargo run -- erhua-member-speaker-canary-sender-map --chat-id <reviewed-erhua-qiwe-group-id>`.
  Its output contains raw QiWe sender ids; keep it as a server-local temporary file only
  and never retain it as evidence.

本地欢迎的脚本测试替身位于
`fixtures/agents/erhua/welcome_runtime.py`，不代表真实 Hermes 接线或正式制卡能力。
