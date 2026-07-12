# Xiaoman Production Preflight Record

Use this record after an owner-approved deploy and before declaring the Xiaoman activity
path ready for production observation. Do not paste secrets, raw chat logs, Feishu Base
ids, QiWe tokens, message ids, or private member data into this file.

## Run Metadata

- Commit SHA:
- Release or artifact id:
- Operator:
- Run time:
- Server:
- Environment file loaded:
- Command:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1
scripts/xiaoman-activity-production-preflight-smoke.sh
```

## Required Evidence

| Check                              | Expected evidence                                                                                                                                                                          | Result |
| ---------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------ |
| Xiaoman signal timer               | `qintopia-agentos-xiaoman-activity-signal-worker.timer` is active and enabled; service command is fixed to `run-xiaoman-activity-signal-worker --once --apply`                             |        |
| Xiaoman promotion starter timer    | `qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer` is active and enabled; service command is fixed to `run-xiaoman-activity-promotion-starter-worker --once --apply`       |        |
| Operations evidence timer          | `qintopia-agentos-operations-evidence-worker.timer` is active and enabled; service command is fixed to `run-evidence-worker --once --apply`                                                |        |
| Operations visual timer            | `qintopia-agentos-operations-visual-worker.timer` is active and enabled; service command is fixed to `run-collaboration-worker --work-item-type visual_asset_request --once --apply`       |        |
| Xiaoman send request starter timer | `qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer` is active and enabled; service command is fixed to `run-xiaoman-activity-send-request-starter-worker --once --apply` |        |
| Operations group send-ready timer  | `qintopia-agentos-operations-group-send-ready.timer` is active and enabled; service command is fixed to `run-group-message-send-worker --once --apply`                                     |        |
| Read-only worker previews          | Aggregate smoke finishes with `xiaoman activity production preflight passed`; preview reports are JSON-valid and `safe_for_chat=false` where present                                       |        |
| Secret and external-send scan      | Journal/unit/preview output contains no token, database URL, Feishu Base id, message id, raw chat, `send_executed=true`, or external-send command                                          |        |
| Production boundary                | No deploy command, Release publish, Feishu write, QiWe call, poster publish, final confirmation, queueing, send-ready, or external send happened during this smoke                         |        |

## Queue Snapshot

Record counts only. Do not paste row payloads.

- Eligible Xiaoman `event_signals` preview count:
- Eligible activity request parent count:
- Eligible evidence request count:
- Eligible visual request count:
- Eligible approved poster brief count:
- Eligible awaiting publish group message request count:
- Eligible queued group message request count:

## Decision

- [ ] Pass: production observation can continue without enabling external adapters.
- [ ] Hold: one or more timers, commands, previews, or boundary checks failed.

Reason:

Follow-up owner action:

## Boundary Reminder

Passing this preflight does not approve publishing, QiWe sends, Feishu writeback, real
Wenyuange retrieval, or Huabaosi production generation. Those require separate
owner-reviewed adapters, allowlists, staged smoke tests, and rollback notes.
