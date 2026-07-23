# Xiaoman Production Preflight Record

Use this record after an owner-approved deploy and before declaring the Xiaoman activity
path ready for production observation. Do not paste secrets, raw chat logs, Feishu Base
ids, QiWe tokens, message ids, or private member data into this file.

## Run Metadata

- Commit SHA: `68c9877bbee7590e434602ad59cb3b917e673a30`
- Release: `v0.2.28`; Deploy Production run `30000866695`
- Operator: `qiaopengjun5162` release publish; Codex read-only production preflight
- Run time: `2026-07-23T19:06+08:00`
- Server: `paxon-server`
- Environment file loaded without printing values: `/etc/qintopia/message-sidecar.env`
- Command:

```bash
sudo bash -lc '
  set -a
  . /etc/qintopia/message-sidecar.env
  set +a
  export QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1
  exec /home/ubuntu/qintopia-agent-os-releases/current/deploy/sidecar/scripts/xiaoman-activity-production-preflight-smoke.sh
'
```

## Required Evidence

| Check                               | Expected evidence                                                                                                                                                                          | Result |
| ----------------------------------- | ------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------ | ------ |
| Xiaoman signal timer                | `qintopia-agentos-xiaoman-activity-signal-worker.timer` is active and enabled; service command is fixed to `run-xiaoman-activity-signal-worker --once --apply`                             | Pass   |
| Xiaoman legacy Hermes cron state    | `/home/ubuntu/.hermes/profiles/xiaoman/cron/jobs.json` is absent or contains no legacy job declarations; observation prints only fixed status, counts, and SHA-256                         | Pass   |
| Xiaoman promotion starter timer     | `qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer` is active and enabled; service command is fixed to `run-xiaoman-activity-promotion-starter-worker --once --apply`       | Pass   |
| Operations evidence timer           | `qintopia-agentos-operations-evidence-worker.timer` is active and enabled; service command is fixed to `run-evidence-worker --once --apply`                                                | Pass   |
| Operations visual timer             | `qintopia-agentos-operations-visual-worker.timer` is active and enabled; service command is fixed to `run-collaboration-worker --work-item-type visual_asset_request --once --apply`       | Pass   |
| Xiaoman image request starter timer | `qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer` is active and enabled; command is fixed to `run-xiaoman-activity-image-generation-starter-worker --once --apply` | Pass   |
| Huabaosi provider runtime state     | Generation flag, compiled adapter mode, and timer state agree; preflight and `run-huabaosi-image-generation-worker --once --dry-run` expose no sensitive configuration or external calls   | Pass   |
| Xiaoman send request starter timer  | `qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer` is active and enabled; service command is fixed to `run-xiaoman-activity-send-request-starter-worker --once --apply` | Pass   |
| Operations group send-ready timer   | `qintopia-agentos-operations-group-send-ready.timer` is active and enabled; service command is fixed to `run-group-message-send-worker --once --apply`                                     | Pass   |
| QiWe image-send state               | Observation reports `disabled` and binds the immutable `v0.2.28` release SHA; no QiWe production artifact or external send is accepted                                                     | Pass   |
| QiWe callback bridge state          | Observation reports `disabled` and binds the immutable `v0.2.28` release SHA; no callback processing is accepted                                                                           | Pass   |
| Read-only worker previews           | Aggregate smoke finishes with `xiaoman activity production preflight passed`; preview reports are JSON-valid and `safe_for_chat=false` where present                                       | Pass   |
| Secret and external-send scan       | Journal/unit/preview output contains no token, database URL, Feishu Base id, message id, raw chat, `send_executed=true`, or external-send command                                          | Pass   |
| Production boundary                 | No deploy command, Feishu write, QiWe call, provider/media call, poster publish, final confirmation, queueing, send-ready execution, or external send happens during this smoke            | Pass   |

All seven internal worker services reported `Result=success` and `ExecMainStatus=0`
after their latest timer triggers. Their `ExecStart` paths resolve to the immutable
`v0.2.28` sidecar under `release/current`.

## Queue Snapshot

The aggregate smoke validates only bounded check-only and dry-run report shapes. It does
not export queue counts, row payloads, or create a synthetic production event.

- Eligible Xiaoman `event_signals` preview count: not exported by the aggregate run
- Eligible image-generation request preview count: not exported by the aggregate run
- Eligible awaiting publish group message request count: not exported by the aggregate
  run

This run therefore does not claim that a real activity traversed every stage; it proves
that the deployed timers, commands, queue readers, provider runtime boundary, and
internal send-ready boundary are operational and safe to observe.

## Decision

- [x] Pass: production observation can continue without executing external adapters
- [ ] Hold: one or more timers, commands, previews, or boundary checks failed.

The observation-contract defects recorded for `v0.2.7` were fixed before `v0.2.9`. The
aggregate preflight now completes successfully from the immutable release. This is
production evidence for the AgentOS-only Xiaoman path, not approval for real image
generation or external delivery.

## Boundary Reminder

Passing this preflight does not approve publishing, QiWe sends, Feishu writeback, real
WenYuanGe retrieval, Huabaosi production generation, or synthetic test rows in the
production database. Those require separate owner-reviewed adapters, allowlists, staged
smoke tests, and rollback notes.
