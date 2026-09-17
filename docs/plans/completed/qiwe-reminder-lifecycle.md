# QiWe reminder lifecycle

Scope: QiWe solitaire time resolution, existing activity/reminder ledgers and truthful
acknowledgements. No Hermes core, production configuration, new scheduler or real sends.

- [x] Localize production symptom and current parser/storage/send contracts.
- [x] Resolve source-grounded time facts with explicit confirmation states.
- [x] Persist activity/plan/reminders consistently and preserve uncertain outcomes.
- [x] Cover parse-to-delivery, rescheduling, concurrency and recovery with fake
      fixtures.
- [x] Run QiWe and applicable repository checks; review operations/backward
      compatibility.

Prefer a focused typed resolver plus the existing JSON ledgers over additional pattern
patches or a new database/scheduler. A small write-ahead transaction preserves the
existing file layout while repairing interrupted two-file updates. Rollback needs a
drained process and outcome reconciliation; never replay historical reminders.

Validation: QiWe suite passed (326 tests, one Linux peer-credential test skipped on
macOS); Hermes contracts passed (48 tests, one platform-specific skip). The repository
`check:pr:auto` quick tier passed outside the sandbox after local socket binding was
blocked inside it. Final focused time/lifecycle tests also passed after review.

Operations review: ambiguous/cancelled/expired delivery details keep legacy terminal
status for rollback; pending write-ahead transactions require new-code recovery before a
drained rollback. New callbacks cannot revive unknown outcomes; late callbacks cannot
revert a newer plan. No server writes, production messages or deployment occurred.
