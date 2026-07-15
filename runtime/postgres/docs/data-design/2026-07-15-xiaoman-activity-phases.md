# Xiaoman Activity Phases

- Schema version: `2026-07-15.001`
- Migration: `migrations/202607150001_xiaoman_activity_phases.sql`
- Status: proposed
- Date: 2026-07-15

## Purpose

Store Xiaoman's current activity lifecycle phase in Postgres and make phase changes
auditable. The phase determines which existing AgentOS capabilities may receive child
work; it does not authorize external execution.

## Schema

`qintopia_agent_os.event_signals.activity_phase` is nullable because non-activity
signals do not have an activity lifecycle. Its only non-null values are `pre_event`,
`in_event`, and `post_event`.

New `活动/聚会` signals created by the event-signal worker start at `pre_event`.
Existing Xiaoman activity signals are backfilled to `pre_event`; other signal types
remain null.

The migration extends `event_signal_mutations.operation` with `phase-update` and updates
`xiaoman.create_activity_request` to allow the three phase-specific root work item
types. No new capability is registered.

## Mutation Contract

`phase-update` requires:

- `actor_agent=xiaoman`;
- a Xiaoman-owned `活动/聚会` event signal UUID;
- a caller-supplied mutation UUID; and
- one allowed phase value.

The sidecar locks the event signal, validates forward-only transition rules, updates
`activity_phase`, and appends one `event_signal_mutations` row in the same transaction.
Exact replay returns the existing mutation. Conflicting mutation-id reuse fails without
state changes.

Status, gap, and phase remain separate one-field mutations. `phase-update` rejects a
status or gap value in the same payload.

## Routing Facts

Signal intake copies the current phase and its derived route into the root work item's
`payload` and `metadata`. Route values are fixed by code:

- `pre_event -> promotion_preparation`;
- `in_event -> live_support`;
- `post_event -> activity_recap`.

The route is derived from the phase and is never accepted as caller-controlled input.
Changing the event phase does not rewrite historical work items. The signal worker may
create one new idempotent root for the new phase.

## Privacy And Production Boundary

- Postgres writes: one nullable phase field, one mutation audit row, and ordinary
  capability-governed work items.
- Feishu reads/writes: none.
- Image provider/media calls: none.
- QiWe sends: none.
- Hermes profile state: unchanged.

The migration is additive. Rollback keeps the column and audit rows in place while the
previous sidecar ignores them.

## Validation

- Rust unit tests cover phase parsing, transitions, derived routes, phase-specific root
  ids, and child capability allowlists.
- The disposable PostgreSQL apply smoke covers a forward phase mutation, audit replay, a
  new phase root, and an evidence-only `in_event` route.
- Schema preflight requires the phase column and schema-change version.
