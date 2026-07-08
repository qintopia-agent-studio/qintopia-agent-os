# Identity Observations v3

Schema version: `2026-06-26.003`  
Migration: `migrations/202606260003_identity_observations.sql`  
Status: applied by migration when deployed  
Date: 2026-06-26

## Purpose

This migration adds audited display-name observations for channel identities. QiWe
webhook payloads may omit `senderName`, while later identity resolution can derive a
current group member or contact name from QiWe APIs. The observation table records where
that display name came from and when it was resolved.

## Tables

`qintopia_identity.channel_identity_observations` records:

- the resolved `channel_identity_id`;
- stable platform/chat/user identifiers;
- observed display name and normalized form;
- source such as `room_member`, `contact`, `webhook`, or `current_backfill`;
- optional source message id or event id;
- confidence and metadata.

`qintopia_identity.channel_identities` remains the current display-name view. For
historical QiWe backfills, `identity_source` stores the resolver authority that produced
the name, such as `room_member` or `contact`; the fact that the write came from a
backfill is stored in metadata and message `processing_hints`.
`qintopia_messages.messages.sender_channel_identity_id` links captured messages to the
stable channel identity.

`qintopia_identity.identity_source_rank(source text)` prevents lower-confidence updates
from replacing stronger current identity values. The rank order is:

`room_member > contact > webhook > current_backfill > fallback_sender_id`.

## Accuracy Boundary

Historical rows whose original webhook did not include a sender display name can only be
backfilled with the current QiWe-verifiable display name. Person and profile workers
must aggregate by `platform + chat_id + sender_id` or `sender_channel_identity_id`, not
by nickname text.

`identity-backfill --refresh` intentionally bypasses existing `channel_identities` rows
and asks QiWe again. Use it when improving resolver logic or source ranking so
previously captured identities can be reclassified without hard-editing production data.

Full-chat refreshes resolve identities in batches: one room-detail request per chat,
then one contact batch request for any remaining unresolved sender ids. Use
`--request-delay-ms` only as a rate-limit safety valve when scanning many chats. For a
single failed key, use `--chat-id ... --sender-id ... --refresh` instead of repeatedly
scanning the whole chat.

The identity worker keeps an in-process member map cache per `chat_id`, keyed by
`userId`, with `QINTOPIA_IDENTITY_MEMBER_MAP_TTL_SECONDS` defaulting to 1200. The cache
is a performance optimization only; durable identity facts remain in
`channel_identities` and `channel_identity_observations`.

For QiWe, `channel_identities.chat_id = ''` is reserved for a platform-level user
identity keyed by `channel_user_id = userId`. It is materialized only by identity
workers or bootstrap flows after the same `userId` has a single unambiguous linked
`person_id`. Reply-context reads may use this platform identity to recognize the same
speaker across group mentions and direct chats, but they must not scan all chat-scoped
rows and choose the newest match.

QiWe `external_userid` / `openUserId` enrichment is not part of the reply hot path. If
enabled later, it should be fetched in batches from QiWe's `Userid转Openid` API and
stored as metadata on the platform-level identity. Missing or failed enrichment must not
block webhook ACKs or 二花 replies.

`identity-bootstrap-persons` creates one `persons` row per unresolved
`channel_identities.person_id`, links the identity to that person, writes a real
nickname alias, and backfills `messages.sender_person_id`. This is intentionally
one-to-one in v1; cross-chat or cross-platform person merges must be handled by a later
reviewed merge workflow.
