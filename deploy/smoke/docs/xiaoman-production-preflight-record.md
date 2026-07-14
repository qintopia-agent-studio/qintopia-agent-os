# Xiaoman Production Preflight Record

Use this record after an owner-approved deploy and before declaring the Xiaoman activity
path ready for production observation. Do not paste secrets, raw chat logs, Feishu Base
ids, QiWe tokens, message ids, or private member data into this file.

## Run Metadata

- Commit SHA: `9ab54cd938d08188b3ab980c7b84f8737da26e5b`
- Release or artifact id: `v0.2.7`; release deploy run `29299942402`; owner-approved
  same-SHA follow-up run `29302981402`
- Operator: `qiaopengjun5162` release publish; Codex read-only preflight
- Run time: initial observation `2026-07-14T02:09Z` through `2026-07-14T02:20Z`; final
  post-follow-up observation `2026-07-14T03:15Z` through `2026-07-14T03:19Z`
- Server: `paxon-server`
- Environment file loaded: `/etc/qintopia/message-sidecar.env`
- Command:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1
/home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh
```

## Required Evidence

| Check                               | Expected evidence                                                                                                                                                                          | Result                                                                                                                                                                |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | --------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| Xiaoman signal timer                | `qintopia-agentos-xiaoman-activity-signal-worker.timer` is active and enabled; service command is fixed to `run-xiaoman-activity-signal-worker --once --apply`                             | Pass                                                                                                                                                                  |
| Xiaoman promotion starter timer     | `qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer` is active and enabled; service command is fixed to `run-xiaoman-activity-promotion-starter-worker --once --apply`       | Pass                                                                                                                                                                  |
| Operations evidence timer           | `qintopia-agentos-operations-evidence-worker.timer` is active and enabled; service command is fixed to `run-evidence-worker --once --apply`                                                | Pass                                                                                                                                                                  |
| Operations visual timer             | `qintopia-agentos-operations-visual-worker.timer` is active and enabled; service command is fixed to `run-collaboration-worker --work-item-type visual_asset_request --once --apply`       | Pass                                                                                                                                                                  |
| Xiaoman image request starter timer | `qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer` is active and enabled; command is fixed to `run-xiaoman-activity-image-generation-starter-worker --once --apply` | Pass after follow-up run `29302981402`: `LoadState=loaded`, `ActiveState=active`, `SubState=waiting`, `UnitFileState=enabled`; fixed release-SHA `ExecStart` verified |
| Huabaosi provider disabled state    | Image generation is disabled; no provider service/timer is installed; preflight and `run-huabaosi-image-generation-worker --once --dry-run` expose no sensitive configuration              | Pass: generation disabled, both provider units absent, preflight bounded, and dry-run returned `no_claimable_image_request`                                           |
| Xiaoman send request starter timer  | `qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer` is active and enabled; service command is fixed to `run-xiaoman-activity-send-request-starter-worker --once --apply` | Timer pass; observation contract hold because the script rejected the current `no_eligible_approved_generated_images` empty-queue status                              |
| Operations group send-ready timer   | `qintopia-agentos-operations-group-send-ready.timer` is active and enabled; service command is fixed to `run-group-message-send-worker --once --apply`                                     | Pass                                                                                                                                                                  |
| Read-only worker previews           | Aggregate smoke finishes with `xiaoman activity production preflight passed`; preview reports are JSON-valid and `safe_for_chat=false` where present                                       | Hold: aggregate stopped on a state-dependent downstream guardrail-text assertion; independent fixed-field checks remained read-only                                   |
| Secret and external-send scan       | Journal/unit/preview output contains no token, database URL, Feishu Base id, message id, raw chat, `send_executed=true`, or external-send command                                          | Pass for completed observation components; no sensitive values were recorded here                                                                                     |
| Production boundary                 | No deploy command, Release publish, Feishu write, QiWe call, provider/media call, poster publish, final confirmation, queueing, send-ready, or external send happened during this smoke    | Pass                                                                                                                                                                  |

## Queue Snapshot

Record counts only. Do not paste row payloads.

- Eligible Xiaoman `event_signals` preview count: `0`
- Eligible activity request parent count: `0`
- Eligible evidence request count: `0` (`no_claimable_evidence_request`)
- Eligible visual request count: `0` (`no_claimable_work_item`)
- Eligible approved poster brief count: `0`
- Eligible image-generation request preview count: `0` (`no_claimable_image_request`)
- Eligible approved generated image count for send-request intake: `0`
- Eligible awaiting publish group message request count: not observed; the preflight
  does not run send-ready
- Eligible queued group message request count: not observed; the preflight does not run
  send-ready

## Decision

- [ ] Pass: production observation can continue without enabling external adapters.
- [x] Hold: one or more timers, commands, previews, or boundary checks failed.

Reason: `v0.2.7` and its sidecar binary remain current at the expected release SHA. The
owner-approved same-SHA follow-up deploy succeeded and the current runner installed the
image request starter service and timer; its independent production observation passed.
The Huabaosi disabled-state observation also passed with no provider unit installed and
no claimable image request. The aggregate preflight remains Hold only because the
deployed `v0.2.7` scripts contain two observation-contract defects: empty downstream
queues lack the `external` wording required by the smoke, and send-request intake now
reports `no_eligible_approved_generated_images` while the smoke expected the old
visual-artifact status. The worker reports showed `dry_run=true`,
`apply_requested=false`, zero artifact ids, and zero previews for the empty queues.

Follow-up owner action: Keep external adapters disabled. Merge and release the
observation-contract fixes, then rerun the aggregate preflight before changing this
decision to Pass. Do not treat the component passes as approval for Feishu writeback,
QiWe sending, provider/media calls, publishing, or real external retrieval/generation.

## Boundary Reminder

Passing this preflight does not approve publishing, QiWe sends, Feishu writeback, real
Wenyuange retrieval, or Huabaosi production generation. Those require separate
owner-reviewed adapters, allowlists, staged smoke tests, and rollback notes.
