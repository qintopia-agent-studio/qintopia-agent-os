# Profile, Digest, and Archive V1

Schema version: `2026-06-26.004`  
Migration: `migrations/202606260004_profile_digest_archive_v1.sql`  
Status: applied by migration when deployed  
Date: 2026-06-26

## Purpose

This migration adds the durable outbox, publisher audit, and manifest indexes needed for
the V1 member-profile, SQL graph, group operations digest, and raw-message retention
workflows.

The runtime workers remain separate from the capture sidecar. QiWe webhook ACKs
and 二花 replies must not wait for these workers. Long-term scheduling should use Agent
OS service names such as `qintopia-agentos-daily-digest-worker.service` and
`qintopia-agentos-feishu-publisher.service`; the sidecar binary may host the first
implementation, but the business ownership is Agent OS / 小满, not the message-capture
sidecar.

## Daily Digest Outbox

`qintopia_agent_os.daily_digests` stores one operations digest per
`platform + chat_id + digest_date + owner_agent`.

V1 ownership:

- owner agent: `xiaoman`;
- schedule: configured by `QINTOPIA_DAILY_DIGEST_TIME`, default `03:00`;
- timezone: configured by `QINTOPIA_DAILY_DIGEST_TIMEZONE`, default `Asia/Shanghai`;
- destination: `QINTOPIA_DAILY_DIGEST_FEISHU_PARENT_NODE`.
- chat display name: optionally configured by `QINTOPIA_CHAT_METADATA_JSON`, keyed by
  QiWe `chat_id`. This is presentation metadata for digest titles and publisher output,
  not an identity key. Missing mappings fall back to `chat_id`.
- dispatch rules: optionally configured by `QINTOPIA_DAILY_DIGEST_DISPATCH_RULES_PATH`
  or `QINTOPIA_DAILY_DIGEST_DISPATCH_RULES_JSON`. The default versioned config is
  `config/agentos/daily-digest-dispatch-rules.json`. The value is a JSON array of
  `{signal, agent, template}` rules. `*_JSON` is an emergency/runtime override and takes
  precedence over the file.

The row is an outbox record. It stores the generated markdown, target Feishu parent
node, publish status, publish attempts/errors, and eventual Feishu document token/url. A
Feishu publisher can consume rows where `publish_status = 'pending_feishu_publish'`.

`qintopia_agent_os.daily_digest_publish_audit` records every publish attempt, including
denied attempts, failed attempts, and successful document publication.

The narrow publisher boundary is `qintopia_daily_digest_publish(digest_id)`. It must
only publish rows owned by `xiaoman`, for configured target chats, into allowlisted
Feishu Base tables. It must not accept arbitrary Markdown, arbitrary Feishu URLs, or
generic document update requests.

The Xiaoman event radar Base follows a three-layer structure:

- `日报总表`: one row per `platform + chat_id + digest_date + owner_agent`;
- `事件信号表`: one row per actionable operations signal;
- `文档归档表`: document/archive index and publication audit fields.

The checked-in setup script is idempotent:

```bash
scripts/setup-daily-digest-base.py --dry-run
scripts/setup-daily-digest-base.py --apply
```

It requires Feishu app scopes that can create/update Base tables and fields, including
`base:table:create` or `bitable:app`.

二花 does not generate, write, or publish daily digests. The digest is internal
operations material and must not be posted back to the QiWe group automatically.

## Worker Commands

The V1 implementation exposes both one-shot apply commands and stable worker commands.
Production should use worker commands through systemd; one-shot commands remain useful
for repair, manual inspection, and acceptance checks.

Long-running commands:

```bash
qintopia-message-sidecar run-member-profile-worker
qintopia-message-sidecar run-graph-projection-worker
qintopia-message-sidecar agentos-daily-digest-worker
qintopia-message-sidecar run-daily-digest-publisher-worker
```

Readiness / one-cycle checks:

```bash
qintopia-message-sidecar run-member-profile-worker --check-only
qintopia-message-sidecar run-graph-projection-worker --check-only
qintopia-message-sidecar agentos-daily-digest-worker --dry-run
qintopia-message-sidecar agentos-daily-digest-worker --once
qintopia-message-sidecar run-daily-digest-publisher-worker --check-only
qintopia-message-sidecar run-daily-digest-publisher-worker --once
```

Relevant runtime settings:

- `QINTOPIA_PROFILE_TARGET_CHAT_IDS`: comma-separated allowlist of QiWe groups.
- `QINTOPIA_CHAT_METADATA_JSON`: optional JSON object keyed by chat id, for example
  `{"10859791146538059":{"display_name":"秦托邦的小伙伴（新）"}}`.
- `QINTOPIA_MEMBER_PROFILE_WORKER_BATCH_SIZE`: profile messages scanned per batch;
  default `500`.
- `QINTOPIA_MEMBER_PROFILE_WORKER_POLL_SECONDS`: profile worker interval; default `300`.
- `QINTOPIA_GRAPH_PROJECTION_WORKER_BATCH_SIZE`: graph facts projected per batch;
  default `500`.
- `QINTOPIA_GRAPH_PROJECTION_WORKER_POLL_SECONDS`: graph worker interval; default `300`.
- `QINTOPIA_DAILY_DIGEST_WORKER_POLL_SECONDS`: daily schedule polling interval; default
  `60`.
- `QINTOPIA_DAILY_DIGEST_PUBLISHER_BATCH_SIZE`: pending digest rows published per batch;
  default `10`.
- `QINTOPIA_DAILY_DIGEST_PUBLISHER_POLL_SECONDS`: Feishu publisher polling interval;
  default `120`.
- `QINTOPIA_DAILY_DIGEST_PUBLISHER_AGENT`: actor passed to the narrow publisher; default
  `xiaoman`.
- `QINTOPIA_DAILY_DIGEST_FEISHU_BASE_TOKEN`: target Xiaoman event radar Base.
- `QINTOPIA_DAILY_DIGEST_ALLOWED_FEISHU_BASE_TOKENS`: comma-separated allowlist for the
  narrow publisher.
- `QINTOPIA_DAILY_DIGEST_FEISHU_DAILY_TABLE_ID`: `日报总表` table id.
- `QINTOPIA_DAILY_DIGEST_FEISHU_SIGNAL_TABLE_ID`: `事件信号表` table id.
- `QINTOPIA_DAILY_DIGEST_FEISHU_ARCHIVE_TABLE_ID`: `文档归档表` table id.
- `QINTOPIA_DAILY_DIGEST_FEISHU_PROFILE_ENV_PATH`: path to the Xiaoman Feishu app
  profile env; defaults to `/home/ubuntu/.hermes/profiles/xiaoman/.env`.
- `QINTOPIA_DAILY_DIGEST_DISPATCH_RULES_PATH`: JSON file path for digest signal to Agent
  assignment mapping; defaults to `config/agentos/daily-digest-dispatch-rules.json`.
- `QINTOPIA_DAILY_DIGEST_DISPATCH_RULES_JSON`: optional inline JSON override for the
  same mapping. Supported `signal` values are `activity_events`, `services`,
  `operations`, `content_or_activities`, `questions`, `empty_digest`, and `always`.
  Templates can use `{activity_count}`, `{service_count}`, `{operation_count}`,
  `{question_count}`, and `{content_count}`.

The member profile worker is idempotent for V1 rule-based extraction:

- facts are unique by `source_message_id + fact_type + fact_key` while not revoked;
- active reply-context snapshots are skipped when the same `input_hash` already exists;
- interaction summaries are only inserted when a new snapshot is needed.

The graph projection worker is rebuildable and idempotent through
`graph_edges_unique_evidence`; repeated runs update existing edges instead of creating
duplicate evidence edges.

## Raw Message Archive Index

`qintopia_agent_os.raw_message_archives` records compressed raw-message archive batches.
The V1 archive format is `jsonl.zst` with a separate manifest file.

The archive job marks source message rows with `processing_hints.raw_archived = true`
and records archive metadata in `processing_hints`. V1 does not hard delete message
rows.

Default message-search tools must exclude archived raw messages unless a future audited
restore/search path is explicitly added for an operations or audit purpose.

## Profile and Graph Boundary

Member facts, interaction summaries, safe reply snapshots, and SQL graph projections
continue to use the tables introduced by `202606240002_agent_os_data_layer.sql`.

V1 does not depend on Apache AGE or `tencentdb_ai`. SQL graph tables are the primary
projection and remain the source used by Agent OS components.

## Safety Boundary

Allowed V1 fact categories include communication preferences, reply style preferences,
interests, skills, activity participation/organization, community contribution, service
needs, unresolved/repeated questions, content leads, and operations signals.

Disabled categories include relationship inference, interpersonal conflict, sensitive
service notes, and psychological, medical, financial, legal, or private-life inference.

二花 may only read safe `member_profile_snapshots` through
`qintopia_member_context_lookup`, and every read is recorded in
`qintopia_identity.member_context_audit`.
