# Huabaosi WeCom Observation Journal Window

Date: 2026-07-23

## Observed Evidence

Release `v0.2.28` deployed successfully and the Xiaoman aggregate production preflight
passed from the immutable release. The separate Huabaosi WeCom gateway observation
failed because its last-160-lines journal scan found four Python traceback markers.

Sanitized timestamp and systemd invocation checks showed that all four markers were from
2026-07-14. They predated `v0.2.28` by nine days. No token, URL, message field, or raw
journal line was returned during diagnosis.

## Root Cause

The observation bounded the journal by line count but not by time. A low-volume service
could therefore retain an old error inside its last 160 lines indefinitely, causing
unrelated later Releases to fail the read-only observation.

The line count and production command/path dependencies were also caller-configurable,
so a caller could substitute alternate state and weaken the evidence.

## Resolution

- Scan only the most recent 30 minutes, capped at 160 lines.
- Keep production commands, paths, and both journal limits fixed in the release-local
  script.
- Generate the fake-command variant only inside the Node test fixture.
- Continue to fail closed on recent traceback or sensitive-output markers.
- Retain sanitized counts only; never print matching journal lines.

## Validation

- `node tools/deploy/test-huabaosi-wecom-observation.mjs`
- `node tools/deploy/check-deploy-contracts.mjs`
- `node tools/deploy/check-deploy-runner.mjs`
- `npm run lint:md`
- `npm run format:check`
- `git diff --check`

## Production Boundary

This change does not clear journals, restart or reconfigure Hermes, read profile
secrets, send WeCom or QiWe messages, generate images, write Postgres or Feishu, or
enable a timer. The corrected observation requires a new owner-approved Release before
its production result can be retained as passing evidence.
