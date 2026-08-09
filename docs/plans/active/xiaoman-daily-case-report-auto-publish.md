# Xiaoman Daily Case-Report Auto Publish

Updated: 2026-08-08

## Goal

Change `workflows/xiaoman-daily-case-report` from a human-reviewed draft generator into
a release-managed daily automatic publisher. The daily run should read the latest
rolling 24 hours of the reviewed QiWe group, render the case-file poster, and publish it
to the target group without per-day human confirmation.

This does not remove reviewed production enablement. It removes the recurring human
"send this one" step after the system is activated.

## Current State

- PR #389 merged the deterministic report generator.
- The script can generate HTML locally and now emits JPEG artifact identity fields when
  image rendering succeeds.
- The workflow manifest is still draft until production evidence is retained.
- `operations-daily-case-report-media-upload` validates the local JPEG identity and
  uploads it to the reviewed HTTPS media boundary before any publish work item is
  created.
- `operations-daily-case-report-auto-publish-create` now binds a durable JPEG URI and
  image identity to an approved `generated_image` artifact plus one automatic send-ready
  `group_message_request`.
- `deploy/sidecar/scripts/xiaoman-daily-case-report-auto-publish-worker.sh` runs the
  immutable release workflow, media upload, and AgentOS auto-publish creation as one
  release-local entrypoint.
- The release systemd renderer installs
  `qintopia-agentos-xiaoman-daily-case-report-auto-publish.{service,timer}` with a daily
  `OnCalendar=*-*-* 07:45:00` timer. Release install does not default-enable the timer;
  production activation/rollback are dedicated reviewed scripts.
- `group_message_send` records send-ready only; the existing QiWe image-send timer
  performs the external delivery.
- The real QiWe image-send adapter now has a `daily_case_report` automatic-publish
  branch. Production still requires owner activation and retained real send evidence.

## Required Design

1. Add a daily-report artifact boundary.
   - Store the rendered JPEG as a durable AgentOS artifact through the reviewed sidecar
     binding after the image has a stable HTTPS or reviewed storage URI.
   - Record content hash, file MD5, byte size, MIME type, render window, source chat id
     hash or allowlisted runtime reference, template version, and dimensions.
   - Keep real group ids, member names, raw excerpts, credentials, and local paths out
     of retained reports.

2. Add an automatic publish work item.
   - Create one idempotent send work item per report window.
   - The work item must target the reviewed group from runtime config, not a committed
     group id.
   - The payload must make `requires_human_final_confirmation=false` explicit for this
     workflow only.

3. Connect to the QiWe image-send production adapter.
   - Reuse the reviewed async upload, callback, and send-image path.
   - Do not add a Python QiWe sender or deprecated synchronous upload shortcut.
   - Do not send from a local image path.

4. Install a release-managed daily timer.
   - Render/install through reviewed deploy/runner code.
   - Use release-local workflow and sidecar companion paths.
   - Include templates, scripts, and workflow code in the deploy bundle.
   - Use `activate-xiaoman-daily-case-report-auto-publish-production.sh` and
     `rollback-xiaoman-daily-case-report-auto-publish-production.sh` for timer state.

5. Validate production behavior.
   - Read-through succeeds for the target group.
   - JPEG rendering succeeds from the immutable release runtime.
   - One daily report window creates one artifact and one send attempt.
   - QiWe callback correlation completes and the send result is retained.
   - Reruns are idempotent and never duplicate a daily send.

## Non-Goals

- No daily human confirmation.
- No conversation-created cron.
- No manual copy into `/etc/systemd/system`.
- No direct Python QiWe publishing path.
- No committed real QiWe group id or secret-bearing report output.

## Acceptance

- The workflow can be marked active only after the automatic publish path is implemented
  in reviewed deploy code and covered by local tests plus production observation.
- A retained production evidence report proves one real daily auto-publish completed
  without per-day human confirmation.
- Rollback disables only the daily report timer and leaves unrelated Xiaoman/QiWe timers
  unchanged.
