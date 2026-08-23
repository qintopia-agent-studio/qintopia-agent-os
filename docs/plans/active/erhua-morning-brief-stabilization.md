# 二花早报 / 小满日报发送 stabilization plan

Status: pending production observation evidence. Do not implement runtime changes until
the root cause of the recent text fallback is confirmed.

Scope: stabilize the Erhua morning-brief card send path and decide whether to enable the
QiWe image-send intro-text feature so that both the morning brief and the daily case
report arrive with a short text explanation before the image/card.

## Background

1. The morning brief was changed from plain text to a rendered card image in PR `#648`
   (`feat(sidecar): Erhua morning brief sends a card image instead of plain text`).
2. Recent fixes (`#650`, `#652`, `#655`, `#657`, `#661`, `#662`) hardened the card path,
   news dedup, and Feishu delivery.
3. On 2026-08-22 the morning brief was observed to be sent as plain text instead of a
   card, and both the morning brief and daily report were observed to arrive without a
   leading text explanation.

## Two Separate Problems

### Problem A: morning brief falls back to plain text

The worker script (`deploy/sidecar/scripts/erhua-morning-brief-worker.sh`) is
intentionally fail-soft: if any card prerequisite is missing or the card chain fails, it
degrades to the text-brief path. The code distinguishes three fallback reasons:

1. `card image disabled, falling back to text brief` — a required card env var is
   missing (image-send gate, Huabaosi Feishu mirror gate, etc.).
2. `card image unavailable; falling back to text brief` —
   `morning_brief.py --render-image` did not produce a file.
3. `card delivery failed; falling back to text brief` — the card was rendered and the
   env was ready, but `operations-erhua-morning-brief-media-upload` or
   `operations-erhua-morning-brief-card-publish-create` failed.

The first step is to identify which of these three messages appeared in the 2026-08-22
production Hermes cron log.

### Problem B: no text intro before image/card

`runtime/sidecar/src/qiwe_image_send.rs` supports sending the work item's `message_text`
as a chat intro before the image, but it is gated by:

```text
QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED=1
```

This env var defaults to OFF. When enabled, the image-send worker sends the intro text
first and only proceeds to the image if the text is confirmed delivered. The code is
intentionally fail-closed: a failed intro aborts the entire send with
`intro_text_send_failed` so the group never receives a bare image after a lost intro.

The existing default intro texts are:

- Daily case report: `小满日报已自动生成。`
- Morning brief card: `二花早报 {date} 已生成，完整内容见卡片。`

## Investigation Steps

1. Collect the 2026-08-22 `erhua-morning-brief-worker` Hermes cron log and grep for
   `erhua morning brief worker:`.
2. Classify the failure into one of the three fallback reasons above.
3. Run `Observe Production Runtime` with target `erhua-morning-brief-worker-run` and
   `xiaoman-daily-case-report-worker-run` to confirm current worker health.
4. Confirm the production release SHA matches a release that contains PR #648 and later
   fixes.
5. Check whether `QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED` is set in the production
   env.

## Decision Matrix

| Problem A root cause     | Action                                                                                                                   |
| ------------------------ | ------------------------------------------------------------------------------------------------------------------------ |
| Missing card env var     | Add the missing env var through the reviewed production configuration path; do not change code                           |
| Card render failure      | Inspect `rendered_image_path` and the Python render path; fix the render bug in code                                     |
| Card delivery failure    | Inspect media-upload / card-publish-create sidecar commands and their Feishu/QiWe prerequisites; fix the failing command |
| No fallback reason found | Add structured logging to the worker script so the next occurrence is classifiable                                       |

## Intro Text Enablement Plan

1. Verify the QiWe text-send path is healthy for the target groups (use the existing
   `run-qiwe-text-send-worker` smoke if available).
2. In a non-production environment, set `QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED=1`
   and run a dry-run of the daily report and morning brief send chains.
3. Confirm the intro text is delivered before the image and that a failed intro rejects
   the send rather than silently dropping the text.
4. Update the production env to set `QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED=1`.
5. Observe the next real send for both daily report and morning brief.

## Validation

For code changes:

```text
cd runtime/sidecar
cargo fmt --check
cargo clippy --all-targets --all-features --tests -- -D warnings
RUST_MIN_STACK=33554432 cargo test
bash -n deploy/sidecar/scripts/erhua-morning-brief-worker.sh
bash -n deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh
pnpm check:pr:auto
```

For production env changes:

- Run `Observe Production Runtime` for the affected worker after the change.
- Confirm the next scheduled send produces both the intro text and the image.

## Production Boundary

- Problem A may require production env changes or code changes; code changes are limited
  to the morning-brief worker / sidecard commands.
- Problem B is a production env toggle plus observation; the feature code already exists
  and is unit-tested.
- Neither problem should enable new external-sends or change QiWe/Feishu caller
  contracts beyond the existing image-send/text-send paths.

## Rollback

- For Problem A: if a code fix regresses, revert the change and re-publish the previous
  release; the text fallback remains available.
- For Problem B: unset `QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED` to return to
  bare-image sends.

## Success Criteria

- Morning brief reliably sends as a card for a reviewed observation period, or the root
  cause of text fallback is fixed and verified.
- Both morning brief and daily report include a leading text intro when
  `QINTOPIA_QIWE_IMAGE_SEND_INTRO_TEXT_ENABLED` is enabled.
- No duplicate sends (text + image) occur due to intro-text failures.

## Related Documents

- `docs/plans/active/erhua-morning-brief-card-send.md` (superseded; delete after this
  plan is approved)
- `docs/operations/production-runtime-observation-runbook.md`
- `docs/operations/production-current-status.md`
- PR #648, #650, #652, #655, #657, #661, #662
