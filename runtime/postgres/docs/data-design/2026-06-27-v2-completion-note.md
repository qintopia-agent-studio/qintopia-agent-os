# V2 Completion Note

Date: 2026-06-27

This note records the verified V2 state for Xiaoman daily community event radar, member
profiles, graph projection, and raw-message retention.

## Runtime Shape

The deployed V2 flow is:

```text
QiWe messages
-> message sidecar persistence
-> identity worker
-> member profile worker
-> graph projection worker
-> event signal worker
-> daily digest worker
-> daily digest publisher worker
-> Feishu Base
```

The Feishu publisher remains a narrow Agent OS boundary. It scans digest outbox rows and
publishes by `digest_id`; it cannot accept arbitrary Markdown or write to arbitrary
Feishu tables.

## Current Production Evidence

Verified on server checkout `510d6ce`:

- `qintopia-agentos-member-profile-worker.service`: active
- `qintopia-agentos-graph-projection-worker.service`: active
- `qintopia-agentos-event-signal-worker.service`: active
- `qintopia-agentos-daily-digest-worker.service`: active
- `qintopia-agentos-daily-digest-publisher.service`: active and enabled
- `qintopia-agentos-raw-archive-worker.service`: active
- `qintopia-message-sidecar.service`: active

Database evidence:

- member facts: 67
- person interaction summaries: 41
- member profile snapshots: 41, active snapshots: 31
- graph entities: 37
- graph edges: 67
- daily digests: 2, all published
- active event signals: 24, all linked to Feishu records
- raw message archives: 0 because there are currently 0 hot QiWe messages older than 30
  days.

Feishu Base evidence:

- `日报总表`: 2 rows
- `事件信号表`: 24 rows
- `文档归档表`: 2 rows

## Acceptance Boundary

V2 is considered complete for the current target group because:

- target chat selection is configuration-driven;
- event rows come from structured `event_signals`, not Markdown parsing;
- duplicate activity/solitaire messages are aggregated into event-level rows;
- daily digests are generated automatically;
- Feishu Base publication is automatic through the narrow publisher worker;
- member profile and graph workers have produced real database artifacts;
- raw archive worker is active and correctly has no work while no messages exceed the
  30-day retention window;
- 二花 remains outside the digest writing path.

## Known Non-Blocking Follow-Ups

- Replace or augment the rule judge with an AI judge at the
  `event_signal_candidates -> event_signals` boundary.
- Add richer event quality evaluation fixtures from real community days.
- Add optional cleanup tooling for old Feishu rows if a future schema reset is needed.
