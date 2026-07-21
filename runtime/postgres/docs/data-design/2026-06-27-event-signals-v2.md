# Event Signals V2

Schema version: `2026-06-27.005`  
Migration: `migrations/202606270005_event_signals_v2.sql`  
Status: applied by migration when deployed  
Date: 2026-06-27

## Purpose

V2 makes structured event signals the source of truth for 小满每日社区事件雷达. The V1
publisher parsed daily-digest Markdown bullets back into Feishu Base event rows. That
was convenient for bootstrapping but too broad: summaries, risk boilerplate, casual
questions, and assignment text could become event records.

The V2 source flow is:

```text
raw messages
-> event_signal_candidates
-> rule/AI judge
-> event_signals
-> daily_digests markdown
-> Feishu Base publisher
```

`daily_digests.markdown` is a presentation artifact. It is no longer the source used to
decide what enters `事件信号表`.

## Tables

### `qintopia_agent_os.event_signal_candidates`

One row per message candidate considered for operations extraction.

The candidate row records:

- stable message identity and sender links;
- local signal date and source message time;
- normalized candidate labels;
- candidate score;
- filter / judge status;
- extraction version and metadata.

This table is intentionally auditable and can contain rejected candidates. It lets
operators compare rule prefilter quality with future AI judge decisions.

### `qintopia_agent_os.event_signals`

One row per accepted actionable operations event.

This table is the source for:

- `事件信号表` rows in Feishu Base;
- daily digest summaries;
- graph projections and Agent OS assignment workflows.

Each event has:

- signal type, title, summary;
- owner agent and owner display name;
- priority, status, confidence;
- evidence candidate IDs and source message IDs;
- source time window;
- stable `dedupe_key`;
- judge model/reason and extraction version;
- Feishu record publication metadata.

Multiple candidate messages can map to one accepted event. For example, several
solitaire updates for the same activity should produce one `event_signals` row with
aggregated source message IDs and related member names, not one Feishu row per
participant update.

## Event Boundary

V2 event rows should be actionable. They should represent something an operator can
triage, assign, close, or use in a retrospective.

V2 allowed event types:

- `活动/聚会`
- `服务/设施`
- `未回答问题`
- `FAQ/SOP`
- `内容线索`

V2 excluded from event rows by default:

- casual jokes or one-off chat;
- short ambiguous questions such as “粉色的吗”;
- summary-only statements;
- risk boilerplate;
- daily assignment text;
- relationship or conflict inference;
- sensitive medical, legal, financial, psychological, or private-life inference.

Risk and assignment summaries may still appear in the daily digest, but they are not
event records unless a separate, evidence-backed event exists.

## Rule And AI Boundary

V2 does not fully hard-code business judgment and does not send every message to AI.

The stable boundary is:

- hard rules filter system noise, trivial messages, disabled categories, and non-target
  groups;
- configurable rules produce candidates and low-cost labels;
- a judge stage decides whether a candidate group should become an event;
- accepted events are written only when confidence passes the type threshold.

The first deployed V2 judge can be rule-based for reliability. The interface must leave
room for a future AI judge that returns strict JSON:

```json
{
  "should_create_event": true,
  "signal_type": "服务/设施",
  "title": "吧台整理求助",
  "summary": "成员询问是否有人擅长整理，可协助整理吧台。",
  "owner_agent": "xiaoguanjia",
  "priority": "中",
  "confidence": 0.86,
  "reason": "涉及社区公共空间整理，需要运营确认是否分派。"
}
```

AI failure must not block daily digest generation. Failed or low-confidence judge
results remain as candidates and do not publish to Feishu event rows.

## Feishu Base Sync

The Feishu Base remains a three-layer operations view:

- `日报总表`: one row per group/day digest;
- `事件信号表`: rows copied from `event_signals`;
- `文档归档表`: publication/archive index.

The publisher must sync accepted `event_signals`; it must not parse Markdown sections
into events.

`event_signals.feishu_record_id` and `last_published_at` record the Feishu sync state.
The database remains the source of truth.

Production uses `run-daily-digest-publisher-worker` as the long-running narrow
publisher. It scans digest outbox rows with pending or failed publish status and
publishes by `digest_id`; it still cannot accept arbitrary Markdown or write to
arbitrary Feishu tables.

Failed publishes are retried conservatively: automatic retries only pick
`publish_failed` rows with fewer than five attempts and at least five minutes of
cooldown since the last update.

## Compatibility

This migration is additive. V1 tables remain in place:

- existing `daily_digests` rows are retained;
- V1 failed publish audit rows are retained;
- Feishu Base can be cleared or overwritten by V2 sync without deleting the database
  audit history.

Runtime behavior changes are implemented in application code:

- `event-signal` commands generate `event_signal_candidates` and `event_signals`;
- daily digest rendering reads `event_signals`;
- `qintopia_daily_digest_publish` reads `event_signals` for event rows.

When the rule judge changes, unpublished stale rule-generated events for the same
chat/date/version can be deleted and already published stale events are closed, so
reports do not keep carrying obsolete automatic detections.

## Mutation Addendum

Schema version `2026-07-14.001` adds `gap_summary` and the append-only
`event_signal_mutations` audit. Xiaoman status/gap applies now target internal event
signal UUIDs with explicit mutation UUIDs; they do not target Feishu record ids. See
`2026-07-14-xiaoman-event-signal-mutations.md` for status transitions, idempotency, and
privacy boundaries.
