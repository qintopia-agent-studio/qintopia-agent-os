# Hermes Cron Normalization Rollout Notes

Date: 2026-08-12

## Current State

- Branch: `codex/xiaoman-hermes-cron-idempotent-install`
- Built from current `origin/master` after `v0.2.122`.
- Scope: Xiaoman reviewed Hermes cron install idempotency and schema normalization.

## Why This Blocks Xiaoman Daily Report Production

`v0.2.122` deployed the live parity schema-size and Erhua schema normalization fixes,
but rerunning the production Hermes cron install stopped on the first already installed
Xiaoman job:

```text
Hermes cron file already declares the daily case report job
```

The remediation in this branch keeps the reviewed production boundary intact while
letting Xiaoman reviewed jobs be installed again against a newer release without
appending duplicates. Existing jobs must still match the reviewed name, schedule,
script, delivery mode, origin platform, routing fields, and profile-derived chat id.

## After PR Merge And Release Deployment

Run the signed production workflow sequence from GitHub Actions:

1. Apply Hermes cron install for the deployed release SHA.
2. Observe `hermes-cron-snapshot` and `hermes-cron-live-parity`.
3. Apply Hermes cron enable for the five reviewed targets.
4. Observe:
   - `xiaoman-daily-case-report-auto-publish`
   - `hermes-cron-snapshot`
   - `hermes-cron-live-parity`
   - `xiaoman-daily-case-report-worker-run`
5. Build/check Xiaoman production completion evidence only after worker-run evidence is
   `success` and the character-universe flags prove:
   - `raw_messages_included=false`
   - `profile_fact_text_included=false`

Do not paste live `jobs.json`, group ids, profile env values, prompts, raw messages, or
raw worker logs into PRs, reports, or chat.
