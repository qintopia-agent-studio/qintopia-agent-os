# Xiaoman Production Preflight Record

Use this record after an owner-approved deploy and before declaring the Xiaoman activity
path ready for production observation. Do not paste secrets, raw chat logs, Feishu Base
ids, QiWe tokens, message ids, or private member data into this file.

## Run Metadata

- Commit SHA: `f8b02d7eedd6835ad90e84403ef34b8980a23159`
- Release or artifact id: `v0.2.6`
- Operator: `qiaopengjun5162` release publish; Codex read-only preflight
- Run time: `2026-07-13T01:53:02Z`
- Server: `paxon-server`
- Environment file loaded: `/etc/qintopia/message-sidecar.env`
- Command:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1
deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh
```

## Required Evidence

| Check                               | Expected evidence                                                                                                                                                                          | Result                                                        |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------------------------------------------------------------- |
| Xiaoman signal timer                | `qintopia-agentos-xiaoman-activity-signal-worker.timer` is active and enabled; service command is fixed to `run-xiaoman-activity-signal-worker --once --apply`                             | Pass                                                          |
| Xiaoman promotion starter timer     | `qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer` is active and enabled; service command is fixed to `run-xiaoman-activity-promotion-starter-worker --once --apply`       | Pass                                                          |
| Operations evidence timer           | `qintopia-agentos-operations-evidence-worker.timer` is active and enabled; service command is fixed to `run-evidence-worker --once --apply`                                                | Pass                                                          |
| Operations visual timer             | `qintopia-agentos-operations-visual-worker.timer` is active and enabled; service command is fixed to `run-collaboration-worker --work-item-type visual_asset_request --once --apply`       | Pass                                                          |
| Xiaoman image request starter timer | `qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer` is active and enabled; command is fixed to `run-xiaoman-activity-image-generation-starter-worker --once --apply` | Not present in observed `v0.2.6`; required on the next deploy |
| Xiaoman send request starter timer  | `qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer` is active and enabled; service command is fixed to `run-xiaoman-activity-send-request-starter-worker --once --apply` | Pass                                                          |
| Operations group send-ready timer   | `qintopia-agentos-operations-group-send-ready.timer` is active and enabled; service command is fixed to `run-group-message-send-worker --once --apply`                                     | Pass                                                          |
| Read-only worker previews           | Aggregate smoke finishes with `xiaoman activity production preflight passed`; preview reports are JSON-valid and `safe_for_chat=false` where present                                       | Pass                                                          |
| Secret and external-send scan       | Journal/unit/preview output contains no token, database URL, Feishu Base id, message id, raw chat, `send_executed=true`, or external-send command                                          | Pass                                                          |
| Production boundary                 | No deploy command, Release publish, Feishu write, QiWe call, poster publish, final confirmation, queueing, send-ready, or external send happened during this smoke                         | Pass                                                          |

## Queue Snapshot

Record counts only. Do not paste row payloads.

- Eligible Xiaoman `event_signals` preview count: `0`
- Eligible activity request parent count: `0`
- Eligible evidence request count: `70`
- Eligible visual request count: `70`
- Eligible approved poster brief count: `0`
- Eligible image-generation request preview count: not observed in `v0.2.6`
- Eligible awaiting publish group message request count: `0`
- Eligible queued group message request count: `0`

## Decision

- [x] Pass: production observation can continue without enabling external adapters.
- [ ] Hold: one or more timers, commands, previews, or boundary checks failed.

Reason: `v0.2.6` deployed successfully, the deployed sidecar unit SHA matched the
release, and all checks in the then-current aggregate read-only preflight passed. The
image request starter timer was added later and still requires a new observed run. The
previously observed downstream `dry_run` report mismatch did not recur.

Follow-up owner action: Keep external adapters disabled. Observe the existing internal
evidence and visual queues. After the next approved deploy, rerun this preflight and
replace the image request starter result with observed evidence. Do not treat the old
`v0.2.6` pass as approval for Feishu writeback, QiWe sending, publishing, or real
external retrieval/generation.

## Boundary Reminder

Passing this preflight does not approve publishing, QiWe sends, Feishu writeback, real
Wenyuange retrieval, or Huabaosi production generation. Those require separate
owner-reviewed adapters, allowlists, staged smoke tests, and rollback notes.
