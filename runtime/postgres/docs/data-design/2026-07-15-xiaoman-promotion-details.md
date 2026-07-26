# Xiaoman Promotion Details Mutation

Schema version: `2026-07-15.002`  
Migration: `migrations/202607150002_xiaoman_promotion_details.sql`

## Purpose

Record the human reply required to turn an incomplete activity record into a
visual-asset brief. The mutation is one logical promotion-details update and uses the
existing `event_signals.owner_name` and `event_signals.metadata` columns. No new
business table or Feishu write path is introduced.

The structured value contains the public activity `location`, the pre-announcement
decision, the allowlisted channels (`朋友圈` or `小红书`), and the human reviewer. The
activity owner is stored in `event_signals.owner_name`.

## Contract

The sidecar requires `actor_agent=xiaoman`, an internal event-signal UUID, and a
caller-supplied mutation UUID. It accepts only Xiaoman-owned `活动/聚会` signals and
rejects Feishu record identifiers. `需要` requires at least one channel; `不需要`
requires no channels.

The mutation locks the signal, checks the idempotency key, updates the owner and
`metadata.promotion_review`, and appends one audit row in the same transaction. Exact
replay returns the existing mutation without another update or audit row.

## Boundary

- Postgres writes: one event-signal logical promotion-details value and one audit row.
- Feishu reads/writes: none.
- Huabaosi/provider calls: none.
- QiWe sends: none.
- Image generation: none.

After the mutation, Xiaoman may pass the sanitized returned details into the existing
promotion draft tool. A human-confirmed complete brief may then create one internal
`visual_asset_request`; the collaboration worker creates a pending `poster_brief`.
