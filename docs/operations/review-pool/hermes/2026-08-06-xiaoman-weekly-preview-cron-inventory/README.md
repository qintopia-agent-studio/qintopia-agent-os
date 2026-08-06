# Xiaoman Weekly Preview Cron Declaration — Review Inventory

This package is the **prerequisite governance evidence** for eventually turning the
Monday activity-preview cron job into a reviewed, git-managed bundle input. It is not a
deployable Hermes patch and does not change any runtime behavior.

Per `runtime/hermes/README.md` (lines 58–62), a cron/scheduled-job declaration must not
be invented or repointed until a read-only inventory records the job shape and current
script hashes, and a separate reviewed cutover adds activation + rollback. This package
is that inventory.

## Source Evidence

| Evidence                                                              | Path                                                              | SHA-256                                                            |
| --------------------------------------------------------------------- | ----------------------------------------------------------------- | ------------------------------------------------------------------ |
| Deterministic weekly-preview script (the replacement text-first path) | `workflows/xiaoman-weekly-preview/weekly_preview.py`              | `add1b249e695476655d31a1101158136713e046e5571c18211aa1a5b733ddf58` |
| Weekly-preview workflow README                                        | `workflows/xiaoman-weekly-preview/README.md`                      | `3f6effac536983b3ddb306fac533999fb7f68f922c8c4b3d5af4747960c41e9e` |
| Canonical cron-job key contract (what counts as a "job")              | `deploy/sidecar/scripts/xiaoman-legacy-cron-observation-smoke.sh` | `7ed9aae488f7ce9fab0cbc622f202886c670853d18aa0be15af512af1f107dea` |

All hashes are content SHA-256 of the files as committed on
`feat/xiaoman-weekly-preview` (PR #384) / `master` at observation time 2026-08-06.

## Cron Job Shape (derived from the observation contract)

`xiaoman-legacy-cron-observation-smoke.sh` defines `JOB_KEYS`, the key set that makes a
JSON node "look like a job":

```text
active, command, cron, enabled, handler, interval, message, prompt, schedule, target, tool
```

The live Xiaoman `cron/jobs.json` is a runtime-local file under
`/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json`. The historical black-box
behavior was: at schedule time the stored `prompt` was handed to the model, and whatever
the model returned was sent verbatim to `target`.

> **Live read pending.** The exact production JSON envelope (top-level `jobs[]` vs a
> flat map, the precise `schedule`/`cron` field semantics, and the `handler`/`command`
> runner contract) must be confirmed by an owner-triggered read of the live file before
> this proposal is adopted. This inventory records the _contract surface_; it does not
> assume the live envelope.

## Proposed Deterministic Declaration (draft)

See `proposed-xiaoman-weekly-preview-job.json`. The draft replaces the free-form
`prompt` with a fixed `command` that invokes the reviewed `weekly_preview.py` script,
and pins `target` to the home **group** channel (never a private chat). It deliberately
contains no `?table=...` token, so it cannot trip the record-sanitize interceptor that
blocked the old Sunday task.

The script still follows the existing human-confirmation model: it prints an
`operator_review_message` and never auto-sends. The weekly-preview announcement mode
(`announcement_prepare(mode="weekly_preview")`, shipped in PR #384) reads both the plan
and occurrence tables through read-through, deduplicates by `record_ref`, and clearly
reports an empty week instead of sending stale text.

## Black-Box Findings → Disposition

From the 2026-08-04/05 investigation ("the black box is open"):

| Root cause                                                                                                      | Layer             | Fixed by                                                                             | Status                 |
| --------------------------------------------------------------------------------------------------------------- | ----------------- | ------------------------------------------------------------------------------------ | ---------------------- |
| Delivery pointed at a private chat, and `?table=...` tripped sanitize                                           | Delivery          | Proposed declaration pins `target` to the group and drops the token                  | Draft (this package)   |
| Cron session had no Feishu-table read tool; "open the plan table" failed and the failure text was sent verbatim | Capability        | `weekly_preview.py` calls `announcement_prepare`, which has read-through for Xiaoman | Resolved in PR #384    |
| Free-form natural-language prompt → unstable per-run output                                                     | Content           | Deterministic script path, not a model prompt                                        | Resolved in PR #384    |
| Creating a task broadcast to Xiaoman **and** Erhua (no `@` filter) → duplicate jobs                             | Routing/broadcast | Out of repo scope (Hermes runtime dispatch); note below                              | **Not addressed here** |

**Routing note.** The duplicate-job root cause lives in the Hermes runtime dispatch
layer (user → which agent receives "create a cron job"), not in repository Python. QiWe
already filters non-mention group messages by default
(`QIWE_PASSIVE_PIPELINE_ENABLED=false`), so the duplicate is _not_ a group-message-layer
issue — it is the task-creation broadcast. A true fix requires a runtime/orchestration
change and is intentionally out of scope for this repository evidence package.

## Governance Gates Discovered (why this cannot ship yet)

1. `agents/xiaoman/profile-bundle/bundle.json` is `status: observation-only` with
   `production_boundary.live_profile_changes: false`; `cron/jobs.json` is in
   `excluded_runtime_state`. A cron declaration is not yet a bundle input.
2. `xiaoman-legacy-cron-observation-smoke.sh` **hard-fails** if the live jobs.json
   contains any cron declaration (`no_legacy_cron_jobs`). The production posture today
   is "Xiaoman live jobs.json must be empty of cron declarations."
3. `runtime/hermes/README.md` (58–62) forbids inventing the declaration until a
   read-only inventory (this package) records the job shape and current script hashes.

Therefore the proposed declaration in this package is **evidence only**. Activation
requires, in order: (a) owner-triggered live read to confirm the envelope; (b) a
reviewed change to the profile bundle + smoke gate to allow a _specific, signed_ cron
declaration; (c) a separate production cutover PR with rollback.

## Production Boundary

- No server write, restart, profile mutation, database write, or external send.
- This directory is not included in the deploy bundle.
- The proposed `weekly_preview.py` performs no delivery; it only prints a review
  message.
- Rollback is a no-op: nothing here touches runtime state.

## Validation

```bash
pnpm runtime:hermes:check
pnpm agents:profile-bundles:check
git diff --check
```
