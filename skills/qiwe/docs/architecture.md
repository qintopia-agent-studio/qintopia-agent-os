# Architecture

## Desired Data Flow

```text
QiWe third-party webhook
  -> Hermes QiWe platform plugin HTTP route
  -> parse wrapped payload
  -> apply channel-only gates
  -> resolve sender display name for triggered messages
  -> MessageEvent for triggered group/direct messages
  -> Hermes Gateway Runner
  -> Hermes AIAgent
  -> adapter.send()
  -> QiWe /msg/sendHyperText | /msg/sendText | /msg/sendLocation
```

## Why Hermes Platform Plugin

Hermes officially recommends the plugin path for community or third-party messaging
platforms. A platform plugin can be dropped into `~/.hermes/plugins/` and registered
with `ctx.register_platform()` without modifying Hermes core.

This is a better fit than a generic webhook route because QiWe requires:

- stable group id extraction from a third-party wrapped payload;
- group session identity;
- sender mention in replies;
- outbound delivery through QiWe `/msg/sendHyperText`;
- direct/private replies through QiWe `/msg/sendText`;
- location-card delivery through QiWe `/msg/sendLocation`;
- sender display-name resolution without exposing contact APIs to the agent;
- redacted channel audit records for operations and debugging;
- later support for cron/daily summaries and active delivery to groups.

## Stable QiWe Group ID

Use only the inner event field:

```js
const rawEvent = JSON.parse(payload.data);
const groupId = String(rawEvent.fromRoomId);
```

Field responsibilities:

- `sourceGroupId = String(rawEvent.fromRoomId)`
- `deliveryGroupId = String(rawEvent.fromRoomId)`
- Hermes session/group key should include `fromRoomId`
- QiWe send `toId = String(rawEvent.fromRoomId)`

Outer fields:

- `payload.fromGroup` is diagnostic only.
- If `payload.fromGroup !== rawEvent.fromRoomId`, log a mismatch and continue with
  `rawEvent.fromRoomId`.

## Session Strategy

For group mention/cue-triggered replies:

```text
session = qiwe:<fromRoomId>:user:<senderId>
```

This keeps different users in the same group from blocking or polluting each other's
direct conversation with the agent, while still allowing future group shared context to
be injected separately.

For direct/private replies:

```text
session = qiwe:dm:<senderId>
```

Direct messages are not a privilege escalation path. Hermes receives them as
`chat_type=dm`; member context must remain scoped to the current sender only.

## Identity Strategy

The adapter treats `senderId` as the stable answer to "who addressed 二花". Webhook
`senderName` is only a fallback because QiWe payloads often leave it empty.
`msgData.atList` identifies the mentioned users, such as 二花 herself; it is not the
sender identity.

For triggered messages only, the adapter may resolve a display name for Hermes source
metadata. This is an internal channel function, not an agent-visible contact lookup
tool. Display names use this precedence:

```text
group member name -> contact nickname -> contact realName -> webhook senderName -> senderId
```

The adapter must not inject phone numbers, raw contact profiles, tokens, or other
private contact fields into the Hermes prompt. Member/profile access, permissions, and
privacy decisions belong to Agent OS skills and orchestration.

Resolved display names are cached in memory by `(chat_id, senderId)`. When
`QIWE_STATE_DIR` is configured, the same safe mapping is persisted under
`cache/identity.json` and loaded on service startup. The persisted file uses the same
`QIWE_IDENTITY_CACHE_TTL_SECONDS` expiry as memory cache and stores only chat id, user
id, display name, source, and update time; it is not a raw contact database.

## Mention Strategy

Bot name:

```text
二花
```

The current QiWe payload includes:

```json
"msgData": {
  "atList": [
    {"nickname": "二花", "userId": "1688857683805864"}
  ],
  "content": "@二花 hi"
}
```

Mention detection should support:

- `msgData.atList` contains bot user id or nickname;
- text starts with or contains `@二花`;
- text clearly cues the configured bot name for help/search/location;
- configurable bot names and bot user id.

## Reply Strategy

Group text replies use:

```text
/msg/sendHyperText
```

Send to:

```text
params.toId = String(rawEvent.fromRoomId)
```

Mention sender:

```text
subtype=1, text=<senderId>
subtype=0, text=<AI reply>
```

If mention send fails, do not guess alternate group ids. Log the error and surface it
clearly.

Direct text replies use:

```text
/msg/sendText
```

The send target is the direct sender id. Direct mode can be disabled with
`QIWE_DIRECT_ENABLED=false`, constrained with `QIWE_DIRECT_ALLOWED_USERS`, or explicitly
opened with `QIWE_DIRECT_ALLOW_ALL=true`. The default direct/private gate is
allowlist-only, so a missing allowlist does not silently expose private chat handling.
Before sending a direct/private outbound message, the adapter checks QiWe external
contacts through `/contact/getWxContactList` and requires the recipient to be a normal
friend (`contactType=2057`). QiWe documents other contact states such as double-delete,
"I deleted customer", and "customer deleted me"; those states are blocked before
`/msg/sendText` is called. Markdown is not stripped by the adapter; text is sent through
as channel text.

Location replies use:

```text
/msg/sendLocation
```

Hermes business logic or skills must first choose an approved concrete location with
title/name, address, latitude, and longitude. The normal final-response path is for
text. Native location cards should be sent through the controlled Hermes tool:

```text
qiwe_send_location_card
```

That tool is still channel-only: it does not perform GIS search, privacy checks, or
business judgment. It only maps an already-approved structured location result to
`/msg/sendLocation`, optionally sends a short text bundle, and falls back to concise
text if the location-card delivery fails.

Approved private follow-ups use:

```text
/msg/sendText
```

The controlled Hermes tool is:

```text
qiwe_send_direct_message
```

This tool is a channel executor, not a contact center or generic private-send API. The
orchestrating Agent OS workflow must choose the recipient and message first. The adapter
requires an approved `purpose` and stable `idempotency_key` for each call, sends only to
one concrete QiWe user id, and performs short-term idempotency to avoid duplicate
private follow-ups. The same normal-friend contact guard runs before this tool sends, so
Agent OS receives a clear channel error instead of a later QiWe send failure when the
recipient is not sendable. For a group complaint follow-up, that guard error includes a
safe `suggestedNextTool` pointing to `qiwe_request_direct_contact` with the current
group id and sender id; it is a hint for approved orchestration, not automatic
friend-request behavior.

Approved direct-contact requests use:

```text
/contact/addRoomContact
/contact/addDeletedContact
```

The controlled Hermes tool is:

```text
qiwe_request_direct_contact
```

This tool is also a channel executor. It does not search contacts, expose member-center
APIs, or decide whether 二花 should add someone. Agent OS / 大主管 must approve the
complaint follow-up first, choose the exact user id, provide the verification text, and
pass a stable `idempotency_key`. For a group complaint initiator, the workflow should
call `mode=room_member` with the verified `fromRoomId` as `room_id`, which maps to
`/contact/addRoomContact`. If the current Hermes session already contains distinct group
and sender ids, the tool can resolve `room_id` from the current group and `user_id` from
the current sender when those fields are omitted. For a deleted-contact re-request, the
workflow should call `mode=deleted_contact`, which maps to `/contact/addDeletedContact`.
The adapter returns only safe execution fields and QiWe code/message status, never phone
numbers, raw contact profiles, search results, or private member details.

Approved rich/media/card sends use a single whitelisted Hermes tool:

```text
qiwe_send_rich_message
```

It is not a generic QiWe API passthrough. `message_type` maps only to the QiWe message
endpoints currently wrapped by the plugin:

```text
image         -> /msg/sendImage
gif           -> /msg/sendGif
file          -> /msg/sendFile
voice         -> /msg/sendVoice
link          -> /msg/sendLink
weapp         -> /msg/sendWeapp
personal_card -> /msg/sendPersonalCard
```

The tool accepts only the documented fields for the selected type, requires a stable
`idempotency_key` and `purpose`, and returns safe send status fields such as QiWe
code/message, `msgServerId`, `msgUniqueIdentifier`, sequence, and timestamp. It does not
echo file AES keys, file ids, or raw media credentials back to the Agent.

Approved message correction/moderation uses:

```text
qiwe_revoke_message -> /msg/revokeMsg
```

The tool requires `chat_id`, `msg_server_id`, `purpose`, and `idempotency_key`. It
should only be used when an upstream workflow has decided that revocation is allowed and
QiWe's revoke window is still likely valid.

Controlled voice transcription uses:

```text
qiwe_voice_to_text -> /msg/voiceToTextApply -> /msg/voiceToTextQuery
```

`QIWE_VOICE_TO_TEXT_ENABLED` still defaults to false. When enabled, this tool returns
only the voice id and final text result for an explicitly selected `msg_server_id`;
voice messages do not automatically enter model processing.

## Normalized Message Pipeline

The adapter keeps QiWe protocol parsing separate from business processors:

```text
QiWe webhook
  -> QiWe protocol parser
  -> NormalizedMessageEvent
  -> active mention dispatch | passive pipeline
```

`NormalizedMessageEvent` is the extension boundary for future message kinds:

```text
text | solitaire | voice | image | quote | link | location | file | mixed | system | unsupported
```

The active path is for explicit `@二花` or allowed direct messages. The passive path is
for non-mention processors that are explicitly enabled by environment flags. Passive
processors must not call the Hermes Agent or send chat replies just because a group
message arrived.

The first passive processor is group-solitaire activity collection. It uses QiWe
protocol fields (`commonMsgType=SOLITAIRE`, `msgType=213`, or `msgData.solitaireInfo`)
instead of localized title text. The body comes from `msgData.title`; the first line may
be `#Group Note`, `#接龙`, or another localized header and is stripped as presentation
text.

Solitaire body interpretation is delegated to an LLM-only `SolitaireContentParser`.
There is no regex fallback for activity subject, time, detail, or participants: if the
parser is disabled or returns invalid JSON, the message is not turned into an activity
record. This keeps heuristic parsing out of the activity fact source while preserving
deterministic protocol gating.

Feishu activity writes use a configurable field mapping. Code emits stable internal
fields such as `activity_id`, `activity_subject`, `participant_count`, and
`participant_names`; the mapping file chooses the current Feishu table headers. Header
changes should update mapping configuration, not parser or processor code.

## Activity Layer

Group-solitaire activity collection uses a three-layer boundary:

```text
QiWe Channel Layer
  -> ActivityService / ActivityStore
  -> Feishu sink | ReminderWorker | optional semantic enhancement
```

The QiWe adapter owns webhook parsing, normalized event creation, sender/group ids, and
outbound group sends. The activity layer owns activity facts: upserts, latest
participant snapshots, message audit, Feishu sync queue, and reminder jobs. Feishu
remains a human operations view and is never the reminder fact source.

`ActivityStore` v1 is file-based under `QIWE_STATE_DIR/solitaire/`:

```text
activities.json
messages.jsonl
feishu_sync_jobs.jsonl
feishu_writes.jsonl
feishu_retry.jsonl
reminders.json
reminder_sends.jsonl
feishu_record_ids.json
```

`ReminderWorker` reads due jobs from ActivityStore. Before sending it verifies that the
activity is still active, the activity start time still matches the job snapshot, and
the target group is allowed for live sends. Dry-run mode marks jobs sent with a recorded
payload but does not call QiWe. Live mode sends only to `source_group_id` and does not
call Hermes Agent, Feishu, or WenYuanGe.

WenYuanGe is an optional enhancement point for fuzzy classification, promo-copy
improvement, or historical semantic dedupe. It is not required for normal activity
writes or reminder execution.

## Media Strategy

Text remains the only message kind that triggers the existing Hermes reply path by
default. Non-text QiWe messages are accepted and normalized as deterministic channel
metadata:

```text
message_kind = image | voice | file | location | link | video | mixed | card | system | unsupported
attachments = safe structured fields for the detected kind
```

Media normalization is not interpretation. The adapter does not call the model, perform
OCR, download media, or expose raw file credentials because of a media message.
Sensitive file transport fields such as AES keys are omitted from attachments.

QiWe voice-to-text endpoints may be used later as a controlled channel helper, but
`QIWE_VOICE_TO_TEXT_ENABLED` defaults to false. Even when enabled, the helper only
returns transcription output to an explicitly approved code path; voice messages still
do not auto-trigger Hermes just by arriving in a group. When
`QIWE_ACTIVE_ATTACHMENT_PREPROCESS_ENABLED=true`, explicit `@二花` attachment messages
can be converted into safe summaries or controlled fallback text before Hermes dispatch.

## Non-Mention Messages

Initial version:

- accept and optionally record non-mention messages;
- do not trigger agent replies;
- do not call the model.
- reject group slash commands at the channel gate.

Future version:

- process approved message kinds through passive processors;
- run daily summaries by group;
- identify activities, group sign-ups, tasks, and decisions;
- write stable summaries to Feishu.

## Audit Strategy

When `QIWE_AUDIT_ENABLED=true` and `QIWE_STATE_DIR` is configured, the adapter writes
private JSONL channel audit records under that state directory. Audit is for operations
and debugging, not a group memory store.

Audit records include inbound id, conversation id, trigger/decision, resolved
display-name source, and hashed sender id. They must not include QiWe tokens, headers,
raw credentials, phone numbers, or raw contact profiles.

## Performance Constraints

Avoid calling the agent for every group message. The target design is:

- all messages: deterministic parse and storage, zero model tokens;
- mention messages: Hermes agent reply;
- daily summaries: batched model calls;
- long-term memory: curated facts only.
