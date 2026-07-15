# Server Deployment Runbook

This is the adopted standalone sidecar runbook from before the Agent OS monorepo
cutover. Keep it as historical deployment evidence and rollback reference. Current
production deployment uses `../../../docs/operations/release-current-model.md`,
`../../../docs/operations/m9-server-cutover-runbook.md`, and COS-verified release
payloads instead of rebuilding from this standalone checkout.

This sidecar runs on the Hermes server and persists QiWe/Hermes message events from
local NATS JetStream into the Postgres tunnel exposed on the server.

## Server Paths

- Checkout: `/home/ubuntu/qintopia-msg-sidecar`
- Systemd unit: `/etc/systemd/system/qintopia-message-sidecar.service`
- Embedding worker unit: `/etc/systemd/system/qintopia-message-embedding-worker.service`
- Identity worker unit: `/etc/systemd/system/qintopia-message-identity-worker.service`
- AgentOS operations workflow sync timer:
  `/etc/systemd/system/qintopia-agentos-operations-workflow-sync.timer`
- AgentOS operations evidence worker timer:
  `/etc/systemd/system/qintopia-agentos-operations-evidence-worker.timer`
- AgentOS operations visual worker timer:
  `/etc/systemd/system/qintopia-agentos-operations-visual-worker.timer`
- AgentOS operations workbench event timer:
  `/etc/systemd/system/qintopia-agentos-operations-workbench-event.timer`
- AgentOS operations group send-readiness timer:
  `/etc/systemd/system/qintopia-agentos-operations-group-send-ready.timer`
- AgentOS Xiaoman activity signal worker timer:
  `/etc/systemd/system/qintopia-agentos-xiaoman-activity-signal-worker.timer`
- AgentOS Xiaoman activity promotion starter worker timer:
  `/etc/systemd/system/qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer`
- Environment file: `/etc/qintopia/message-sidecar.env`
- NATS URL: `nats://127.0.0.1:4222`
- Expected Postgres endpoint: `127.0.0.1:55432`

## Git Access

The server checkout should use a dedicated deploy key for this repository:

```text
Host github-qintopia-msg-sidecar
  HostName github.com
  User git
  IdentityFile ~/.ssh/qintopia_msg_sidecar
  IdentitiesOnly yes
  StrictHostKeyChecking accept-new
```

The remote URL should be:

```bash
git remote set-url origin github-qintopia-msg-sidecar:PatrickLiveCool/qintopia-msg-sidecar.git
```

If `git fetch origin` fails with `Permission denied (publickey)`, add
`~/.ssh/qintopia_msg_sidecar.pub` on the server as a read-only deploy key for
`PatrickLiveCool/qintopia-msg-sidecar`.

Current server deploy public key:

```text
ssh-ed25519 AAAAC3NzaC1lZDI1NTE5AAAAIH1fnbaWX9q/g+ovbo9sK0LtvcJolUzFQhmaGMzYgjqq qintopia-msg-sidecar-deploy
```

## Environment

Do not commit the real database URL. Store it only in
`/etc/qintopia/message-sidecar.env`:

```env
QINTOPIA_SIDECAR_NATS_URL=nats://127.0.0.1:4222
QINTOPIA_SIDECAR_NATS_STREAM=QINTOPIA_QIWE_MESSAGES
QINTOPIA_SIDECAR_RAW_SUBJECT=qintopia.qiwe.raw
QINTOPIA_SIDECAR_MESSAGE_SUBJECT=qintopia.qiwe.message
QINTOPIA_SIDECAR_CONSUMER=qintopia-message-sidecar
QINTOPIA_SIDECAR_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:55432/DBNAME
QINTOPIA_SIDECAR_BATCH_SIZE=25
QINTOPIA_SIDECAR_NAK_DELAY_SECONDS=30
QINTOPIA_SIDECAR_DB_MAX_CONNECTIONS=5
QINTOPIA_EMBEDDING_BASE_URL=https://livecool.net
QINTOPIA_EMBEDDING_API_KEY=replace-with-server-secret
# Optional full endpoint override for providers that do not use BASE_URL + /v1/embeddings.
# QINTOPIA_MESSAGE_EMBEDDING_ENDPOINT=https://ark.cn-beijing.volces.com/api/plan/v3/embeddings
QINTOPIA_MESSAGE_EMBEDDING_MODEL=text-embedding-3-small
QINTOPIA_MESSAGE_EMBEDDING_BATCH_SIZE=10
QINTOPIA_MESSAGE_EMBEDDING_POLL_SECONDS=10
QINTOPIA_MESSAGE_EMBEDDING_REQUEST_DELAY_MS=0
QINTOPIA_MESSAGE_EMBEDDING_MAX_ATTEMPTS=5
QINTOPIA_MESSAGE_STORE_MCP_ALLOWED_CALLER=wenyuange
# Context MCP caller allowlist. Defaults to QINTOPIA_MESSAGE_STORE_MCP_ALLOWED_CALLER.
# QINTOPIA_CONTEXT_MCP_ALLOWED_CALLERS=wenyuange,erhua
# QiWe sender identity worker. Token and guid may also be sourced from the
# Hermes erhua profile env in production.
QIWE_API_URL=http://manager.qiweapi.com/qiwe/api/qw/doApi
# QIWE_TOKEN=replace-with-server-secret
# QIWE_GUID=replace-with-server-guid
QINTOPIA_IDENTITY_WORKER_BATCH_SIZE=10
QINTOPIA_IDENTITY_WORKER_POLL_SECONDS=60
# QINTOPIA_IDENTITY_WORKER_CHAT_ID=10859791146538059
RUST_LOG=info,qintopia_message_sidecar=debug
```

The deployment template contains placeholders for Feishu Base tokens and table ids. Fill
those values only in the server-local environment file through the approved secret
process; the deploy script leaves an existing environment file unchanged. If a runtime
value is found in git, revoke or rotate it in the provider, replace the server-local
value, deploy an approved SHA, and record the verification. Do not hot-edit source files
on the server.

By default the worker calls `QINTOPIA_EMBEDDING_BASE_URL` plus `/v1/embeddings`. Set
`QINTOPIA_MESSAGE_EMBEDDING_ENDPOINT` to a full endpoint when a provider uses a
different path.

Do not enable or start `qintopia-message-embedding-worker.service` while
`QINTOPIA_EMBEDDING_API_KEY` is still a placeholder. The worker has a `--check-only`
mode for validating config and database connectivity without calling the embedding API.

## Build And Validate

Historical standalone path after the deploy key and database URL are configured:

```bash
cd /home/ubuntu/qintopia-msg-sidecar
scripts/server-deploy.sh deploy
```

Historical manual path:

The production server is an artifact deployment target. Its apt Rust version is not a
supported build toolchain; any exceptional manual build must first install Rust 1.96.0
and use the checked-in lockfile:

```bash
cd /home/ubuntu/qintopia-msg-sidecar
cargo check --offline --locked
cargo test --offline --locked
cargo build --release --offline --locked
```

Before starting the service, run migrations and readiness checks:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
./target/release/qintopia-message-sidecar migrate
./target/release/qintopia-message-sidecar check
./target/release/qintopia-message-sidecar run-embedding-worker --check-only
./target/release/qintopia-message-sidecar run-identity-worker --check-only --batch-size 5
```

## AgentOS Operations Control Plane Smoke

The operations control plane is the source-of-truth layer for governed cross-Agent
workflows such as 小满 -> 画报司 visual requests and approved artifact -> 二花 send
requests. Its detailed runbook is `docs/operations/agentos-operations-control-plane.md`.

Run the no-credential smoke locally or on the server after build:

```bash
scripts/operations-control-plane-smoke.sh
```

`scripts/server-deploy.sh deploy` and `scripts/server-deploy.sh verify` run this dry
smoke automatically with the release binary after the service smoke.

Run the guarded Postgres apply smoke only when it is acceptable to write AgentOS test
audit rows:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1
export QINTOPIA_OPERATIONS_ALLOWED_GROUP_ALIASES="${QINTOPIA_OPERATIONS_ALLOWED_GROUP_ALIASES:-community_activity_group}"
export QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS="${QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS:-operations-apply-smoke-reviewer,operations-apply-smoke-reviewer-2}"
export QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS="${QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS:-operations-apply-smoke-confirmer,operations-apply-smoke-reviewer}"
scripts/operations-control-plane-apply-smoke.sh
```

The deploy script also invokes this apply smoke entrypoint, but it remains a no-op
unless `QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1` is exported in the server shell before
running deploy or verify.

This smoke must not call Feishu Task APIs, QiWe/企微, Huabaosi production image
generation, or any external send/publish adapter. It should only run migrations, write
AgentOS control-plane rows, record audit events, and write
`human_workbench_refs(provider=feishu_task_dry_run)`.

For production use, set the reviewer and confirmer allowlists to real operator
identities instead of the smoke defaults. `QINTOPIA_OPERATIONS_ALLOWED_REVIEWER_IDS`
controls who can approve or request changes on artifacts, and
`QINTOPIA_OPERATIONS_ALLOWED_CONFIRMER_IDS` controls who can give final approval for
high-risk group-message sends.

## Systemd

The service should be installed as a root system service that runs as `ubuntu`:

```ini
[Unit]
Description=Qintopia Message Sidecar
After=nats-server.service network-online.target
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=/home/ubuntu/qintopia-msg-sidecar
EnvironmentFile=/etc/qintopia/message-sidecar.env
ExecStart=/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar run
Restart=always
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

The message embedding worker is a separate systemd service using the same release binary
and env file:

```ini
[Unit]
Description=Qintopia Message Embedding Worker
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=/home/ubuntu/qintopia-msg-sidecar
EnvironmentFile=/etc/qintopia/message-sidecar.env
ExecStart=/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar run-embedding-worker
Restart=always
RestartSec=15
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

The QiWe sender identity worker is a separate systemd service using the same release
binary and env file. It continuously scans messages missing sender identity links,
reuses existing `qintopia_identity.channel_identities` rows, and only calls QiWe when a
chat/sender pair has no trusted display name yet:

```ini
[Unit]
Description=Qintopia Message Identity Worker
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=/home/ubuntu/qintopia-msg-sidecar
EnvironmentFile=/etc/qintopia/message-sidecar.env
EnvironmentFile=-/home/ubuntu/.hermes/profiles/erhua/.env
ExecStart=/home/ubuntu/qintopia-msg-sidecar/target/release/qintopia-message-sidecar run-identity-worker
Restart=always
RestartSec=30
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
```

Install the worker unit and inspect it, but leave it disabled until the real API key is
present:

```bash
sudo systemctl daemon-reload
systemctl is-enabled qintopia-message-embedding-worker.service || true
systemctl status qintopia-message-embedding-worker.service --no-pager || true
systemctl is-enabled qintopia-message-identity-worker.service || true
systemctl status qintopia-message-identity-worker.service --no-pager || true
```

Start and inspect:

```bash
sudo systemctl daemon-reload
sudo systemctl enable --now qintopia-message-sidecar.service
systemctl status qintopia-message-sidecar.service --no-pager
journalctl -u qintopia-message-sidecar.service -n 100 --no-pager
```

After a real embedding API key is configured, enable the worker explicitly:

```bash
sudo systemctl enable --now qintopia-message-embedding-worker.service
systemctl status qintopia-message-embedding-worker.service --no-pager
journalctl -u qintopia-message-embedding-worker.service -n 100 --no-pager
```

After QiWe token/guid are available, enable the identity worker explicitly:

```bash
sudo systemctl enable --now qintopia-message-identity-worker.service
systemctl status qintopia-message-identity-worker.service --no-pager
journalctl -u qintopia-message-identity-worker.service -n 100 --no-pager
```

The AgentOS operations workflow summary sync runs as a systemd timer. It calls the same
release binary with `run-workflow-sync-worker --once --apply`, updates only Postgres
parent workflow summaries and `workflow_status_synced` audit events, and does not run
child workers, create Feishu Tasks, publish artifacts, or send messages.
`scripts/server-deploy.sh prepare|deploy` installs the oneshot service and timer; the
deploy script enables the timer with the other non-external AgentOS worker services.

```bash
sudo systemctl enable --now qintopia-agentos-operations-workflow-sync.timer
systemctl status qintopia-agentos-operations-workflow-sync.timer --no-pager
systemctl list-timers qintopia-agentos-operations-workflow-sync.timer --no-pager
journalctl -u qintopia-agentos-operations-workflow-sync.service -n 100 --no-pager
```

The default timer interval is `2min`. Override it during unit installation by running
the deploy script with `QINTOPIA_OPERATIONS_WORKFLOW_SYNC_TIMER_INTERVAL=5min` or
another systemd time span.

The AgentOS operations evidence worker runs as a systemd timer. It calls
`run-evidence-worker --once --apply`, claims one queued `evidence_request`, creates an
internal `evidence_summary` artifact, and records the audit trail in Postgres. It does
not call live Wenyuange search, export raw messages, read or write Feishu, call QiWe, or
send externally.

```bash
sudo systemctl enable --now qintopia-agentos-operations-evidence-worker.timer
systemctl status qintopia-agentos-operations-evidence-worker.timer --no-pager
systemctl list-timers qintopia-agentos-operations-evidence-worker.timer --no-pager
journalctl -u qintopia-agentos-operations-evidence-worker.service -n 100 --no-pager
```

The default timer interval is `2min`. Override it during unit installation with
`QINTOPIA_OPERATIONS_EVIDENCE_WORKER_TIMER_INTERVAL=5min` or another systemd time span.

The AgentOS operations visual worker also runs as a systemd timer. It calls
`run-collaboration-worker --work-item-type visual_asset_request --once --apply`, claims
one queued `visual_asset_request`, creates a pending `poster_brief` artifact, and
records the audit trail in Postgres. It does not call Huabaosi production generation,
read or write Feishu, call QiWe, publish posters, or send externally.

```bash
sudo systemctl enable --now qintopia-agentos-operations-visual-worker.timer
systemctl status qintopia-agentos-operations-visual-worker.timer --no-pager
systemctl list-timers qintopia-agentos-operations-visual-worker.timer --no-pager
journalctl -u qintopia-agentos-operations-visual-worker.service -n 100 --no-pager
```

The default timer interval is `2min`. Override it during unit installation with
`QINTOPIA_OPERATIONS_VISUAL_WORKER_TIMER_INTERVAL=5min` or another systemd time span.

After an owner-approved deploy, run the guarded downstream timer observation smoke to
inspect both timer units, fixed service commands, recent journal output, and read-only
worker previews:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_OPERATIONS_DOWNSTREAM_TIMERS_OBSERVATION_ENABLE=1
scripts/operations-downstream-timers-observation-smoke.sh
```

The observation smoke does not write Postgres, read or write Feishu, call QiWe, create
production visual assets, or send externally. It fails if the services are not fixed to
`run-evidence-worker --once --apply` and
`run-collaboration-worker --work-item-type visual_asset_request --once --apply`, or if
inspected output includes known secret/external-send markers.

The AgentOS operations workbench event processor also runs as a systemd timer. It calls
`run-workbench-event-worker --once --apply`, processes only already-recorded
`human_workbench_event_recorded` rows that represent review or final-confirmation
requests, delegates to the existing policy-checked commands, and records
`human_workbench_event_processed` for idempotency. It does not poll Feishu directly and
does not call QiWe or any external publish adapter.

```bash
sudo systemctl enable --now qintopia-agentos-operations-workbench-event.timer
systemctl status qintopia-agentos-operations-workbench-event.timer --no-pager
systemctl list-timers qintopia-agentos-operations-workbench-event.timer --no-pager
journalctl -u qintopia-agentos-operations-workbench-event.service -n 100 --no-pager
```

The default timer interval is `1min`. Override it during unit installation with
`QINTOPIA_OPERATIONS_WORKBENCH_EVENT_TIMER_INTERVAL=2min` or another systemd time span.

The AgentOS group send-readiness worker also runs as a systemd timer. It calls
`run-group-message-send-worker --once --apply`, validates queued
`erhua.send_group_message` work items, confirms the artifact is approved and the target
group is allowlisted, and records `group_message_send_ready_recorded` with
`send_executed=false`. It does not call QiWe, Erhua, or any production send adapter.

```bash
sudo systemctl enable --now qintopia-agentos-operations-group-send-ready.timer
systemctl status qintopia-agentos-operations-group-send-ready.timer --no-pager
systemctl list-timers qintopia-agentos-operations-group-send-ready.timer --no-pager
journalctl -u qintopia-agentos-operations-group-send-ready.service -n 100 --no-pager
```

The default timer interval is `1min`. Override it during unit installation with
`QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_INTERVAL=2min` or another systemd time span.

After an owner-approved deploy, inspect this timer with the read-only observation smoke:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_OBSERVATION_ENABLE=1
scripts/operations-group-send-ready-timer-observation-smoke.sh
```

This observation checks only systemd state, rendered unit commands, and sanitized
journal output. It does not run the worker, confirm a group message, write Postgres,
call QiWe, or send externally.

The Xiaoman activity signal worker runs as a systemd timer. It calls
`run-xiaoman-activity-signal-worker --once --apply`, scans only Xiaoman activity
`event_signals`, and creates missing AgentOS `xiaoman.create_activity_request`
`work_items` through the existing `signal-ingest` contract. It does not read or write
Feishu, call QiWe, create visual assets, or send externally.

```bash
sudo systemctl enable --now qintopia-agentos-xiaoman-activity-signal-worker.timer
systemctl status qintopia-agentos-xiaoman-activity-signal-worker.timer --no-pager
systemctl list-timers qintopia-agentos-xiaoman-activity-signal-worker.timer --no-pager
journalctl -u qintopia-agentos-xiaoman-activity-signal-worker.service -n 100 --no-pager
```

The default timer interval is `2min`. Override it during unit installation with
`QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_INTERVAL=5min` or another systemd time span.

After an owner-approved deploy, run the guarded observation smoke to inspect the timer,
service command, recent journal output, and a read-only worker preview:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_OBSERVATION_ENABLE=1
scripts/xiaoman-activity-signal-timer-observation-smoke.sh
```

The observation smoke does not write Postgres, read or write Feishu, call QiWe, create
visual assets, or send externally. It fails if the service command is not
`run-xiaoman-activity-signal-worker --once --apply` or if inspected output includes
known secret/external-send markers.

The Xiaoman activity promotion starter worker also runs as a systemd timer. It calls
`run-xiaoman-activity-promotion-starter-worker --once --apply`, scans existing Xiaoman
activity request `work_items`, and creates only missing AgentOS evidence/visual child
`work_items`. It does not execute evidence retrieval, create visual assets, read or
write Feishu, call QiWe, or send externally.

```bash
sudo systemctl enable --now qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer
systemctl status qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer --no-pager
systemctl list-timers qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer --no-pager
journalctl -u qintopia-agentos-xiaoman-activity-promotion-starter-worker.service -n 100 --no-pager
```

The default timer interval is `2min`. Override it during unit installation with
`QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_INTERVAL=5min` or another systemd
time span.

After an owner-approved deploy, run the guarded observation smoke to inspect the timer,
service command, recent journal output, and a read-only worker preview:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_OBSERVATION_ENABLE=1
scripts/xiaoman-activity-promotion-starter-timer-observation-smoke.sh
```

The observation smoke does not write Postgres, read or write Feishu, call QiWe, create
visual assets, or send externally. It fails if the service command is not
`run-xiaoman-activity-promotion-starter-worker --once --apply` or if inspected output
includes known secret/external-send markers.

The Xiaoman image-generation starter runs as a separate internal systemd timer. It calls
`run-xiaoman-activity-image-generation-starter-worker --once --apply` and only creates
missing AgentOS `image_generation_request` work items from approved `poster_brief`
artifacts. It does not run the Huabaosi image worker, contact a provider, upload media,
create generated images, write Feishu, call QiWe, publish, or send externally.

```bash
sudo systemctl enable --now qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer
systemctl status qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer --no-pager
journalctl -u qintopia-agentos-xiaoman-activity-image-generation-starter-worker.service -n 100 --no-pager
```

The default interval is `2min`. Override it during unit installation with
`QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_INTERVAL=5min` or another
systemd time span. After an owner-approved deploy, run the read-only observation:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_OBSERVATION_ENABLE=1
scripts/xiaoman-activity-image-generation-starter-observation-smoke.sh
```

The release installer installs the Huabaosi production preflight, worker, and timer
units but leaves the timer disabled. Before activation, apply the reviewed non-secret
release SHA and database URL hash plus provider/media secrets through the production
configuration channel. Do not edit the server checkout or commit those values.

Run the read-only observation before activation. It runs configuration preflight and a
queue preview only; it does not claim requests or contact provider/media endpoints.

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_OBSERVATION_ENABLE=1
scripts/huabaosi-image-generation-production-observation-smoke.sh
```

After the owner manually publishes the Release and confirms the exact release SHA,
confirm the new units were installed. The first release containing this installer change
is processed by the previous deploy runner, so run the reviewed same-SHA follow-up
deployment with the original release scope and `qintopia-system-services` restart target
before activation. Do not repair a missing unit with a server edit.

Then activate the canary timer. The activation command first starts the fixed no-network
preflight service and stops without enabling the timer if any production gate is
invalid.

```bash
export QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ACTIVATION=approved-production-image-generation
scripts/activate-huabaosi-image-generation-production.sh
unset QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ACTIVATION
```

Observe the first pending `generated_image`; do not approve it automatically. Confirm
the timer and sanitized worker outcome with the same observation smoke. Immediate
rollback stops scheduling before runtime configuration is changed through the reviewed
configuration channel:

```bash
export QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ROLLBACK=approved-production-image-generation-rollback
scripts/rollback-huabaosi-image-generation-production.sh
unset QINTOPIA_HUABAOSI_IMAGE_PRODUCTION_ROLLBACK
```

Rollback does not delete generated artifacts or audit rows and does not enable QiWe.

For the Huabaosi WeCom migration, run the separate gateway observation smoke from the
release directory. This smoke does not source `.env`. It inspects only the active Hermes
gateway service, fixed command shape, public `busy_input_mode`, release/current
presence, and sanitized journal marker counts for internal filtering, send fallback, and
API timeouts.

```bash
export QINTOPIA_HUABAOSI_WECOM_OBSERVATION_ENABLE=1
scripts/huabaosi-wecom-gateway-observation-smoke.sh
```

It must not restart services, print raw journal lines, print user messages, send WeCom
messages, run image generation, write Postgres or Feishu, call QiWe/provider/media
endpoints, or modify live Hermes profile state.

The Xiaoman activity send request starter worker also runs as a systemd timer. It calls
`run-xiaoman-activity-send-request-starter-worker --once --apply`, scans Xiaoman
activity promotion parents with completed visual children and approved `poster_brief`
artifacts, and creates only missing AgentOS `group_message_request` child `work_items`
in `awaiting_publish`. It does not record final confirmation, move work items to
`queued`, run send-ready, publish, read or write Feishu, call QiWe, or send externally.

```bash
sudo systemctl enable --now qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer
systemctl status qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer --no-pager
systemctl list-timers qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer --no-pager
journalctl -u qintopia-agentos-xiaoman-activity-send-request-starter-worker.service -n 100 --no-pager
```

The default timer interval is `2min`. Override it during unit installation with
`QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_INTERVAL=5min` or another systemd
time span.

After an owner-approved deploy, run the guarded observation smoke to inspect the timer,
service command, recent journal output, and a read-only worker preview:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_OBSERVATION_ENABLE=1
scripts/xiaoman-activity-send-request-starter-observation-smoke.sh
```

The observation smoke does not write Postgres, read or write Feishu, call QiWe, record
final confirmation, queue group messages, run send-ready, or send externally. It fails
if the service command is not
`run-xiaoman-activity-send-request-starter-worker --once --apply` or if inspected output
includes known secret/external-send markers.

After the Xiaoman intake and starter timers are healthy, run the downstream observation
smoke to inspect the evidence and visual artifact timers:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE=1
scripts/xiaoman-activity-downstream-observation-smoke.sh
```

This smoke runs `run-evidence-worker --once --dry-run` and
`run-collaboration-worker --work-item-type visual_asset_request --once --dry-run`. It
does not write Postgres, read or write Feishu, call QiWe, generate posters, or send
externally. It fails if the preview output includes known secret/external-send markers.

For the production preflight, run the Xiaoman aggregate observation smoke after an
owner-approved deploy. It composes the Xiaoman signal timer observation, Xiaoman
promotion starter timer observation, shared evidence/visual timer observation, Xiaoman
downstream evidence/visual preview, Xiaoman image-generation starter observation,
Huabaosi provider disabled-state observation, Xiaoman send request starter timer
observation, and the group send-ready timer observation. It runs the provider worker
only as `--once --dry-run`, does not run the send-ready worker, and does not deploy,
write Postgres or Feishu, call provider/media endpoints or QiWe, publish, or send
externally.

The group send-ready timer observation only inspects the fixed command, timer state, and
sanitized journal; it does not run the send-ready worker.

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
export QINTOPIA_XIAOMAN_ACTIVITY_PRODUCTION_PREFLIGHT_ENABLE=1
scripts/xiaoman-activity-production-preflight-smoke.sh
```

## Read-Only Database Checks

Before and after deployment, confirm pending embedding jobs and embedding rows with the
worker check mode:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
./target/release/qintopia-message-sidecar run-embedding-worker --check-only
```

The JSON output includes `pending_embedding_jobs` and `message_embeddings`.

## Message Store MCP Smoke

The message store MCP server is started by an MCP client over stdio. It is not an HTTP
service and does not need a separate systemd unit for v1.

Before wiring a client, validate the same search path through the CLI:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
./target/release/qintopia-message-sidecar search-message-store \
  --query "wifi 密码" \
  --caller wenyuange \
  --purpose "server smoke" \
  --limit 5
```

Then validate basic MCP discovery and call handling:

```bash
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0.1.0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"qintopia_message_store_search","arguments":{"caller":"wenyuange","purpose":"server smoke","query":"wifi 密码","limit":5}}}' \
  | ./target/release/qintopia-message-sidecar mcp-message-store
```

## Context MCP Smoke

The context MCP server is also stdio-only. It exposes Agent-facing context tools and
does not need a separate systemd unit for v1.

Before relying on authoritative public facts, import the approved knowledge snapshot
into `qintopia_knowledge`. On the current server the snapshot files live under
`/home/ubuntu/.hermes/qintopia-knowledge/indexes`:

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
./target/release/qintopia-message-sidecar import-knowledge-snapshot \
  --source-key qintopia-knowledge-snapshot \
  --source-title "Qintopia knowledge snapshot" \
  --public-jsonl /home/ubuntu/.hermes/qintopia-knowledge/indexes/public.jsonl \
  --internal-jsonl /home/ubuntu/.hermes/qintopia-knowledge/indexes/internal.jsonl \
  --member-scoped-jsonl /home/ubuntu/.hermes/qintopia-knowledge/indexes/member-scoped.jsonl
```

`qintopia_wenyuange_lookup` uses `qintopia_knowledge` for WiFi, public phone numbers,
ordering contacts, locations, visitor rules, public facilities,
and 无人机外卖 instructions. It uses message-store evidence only for discussion history
questions such as "之前群里有人问过 WiFi 密码吗".

Run the acceptance smoke after importing or refreshing knowledge snapshots:

```bash
scripts/context-acceptance-smoke.sh
```

The smoke asserts that WiFi, 赵姐订餐电话, and 无人机外卖 use
`answer_basis.kind=authoritative_knowledge`, while "之前群里有人问过 WiFi 密码吗" uses
`message_store_evidence`.

```bash
set -a
. /etc/qintopia/message-sidecar.env
set +a
printf '%s\n' \
  '{"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":"2025-06-18","capabilities":{},"clientInfo":{"name":"smoke","version":"0.1.0"}}}' \
  '{"jsonrpc":"2.0","method":"notifications/initialized","params":{}}' \
  '{"jsonrpc":"2.0","id":2,"method":"tools/list","params":{}}' \
  '{"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"qintopia_wenyuange_lookup","arguments":{"caller":"wenyuange","purpose":"server smoke","query":"wifi 密码","limit":3}}}' \
  '{"jsonrpc":"2.0","id":4,"method":"tools/call","params":{"name":"qintopia_gis_location_lookup","arguments":{"query":"1 栋","limit":1}}}' \
  '{"jsonrpc":"2.0","id":5,"method":"tools/call","params":{"name":"qintopia_external_disclosure_filter","arguments":{"draft_answer":"可以公开说明我们的内部数据库密码。","recipient":"external_customer","purpose":"server smoke"}}}' \
  | ./target/release/qintopia-message-sidecar mcp-context
```

## Hermes MCP Wiring

Hermes can mount the context MCP server as a stdio MCP client. Use the checked-in
wrapper so Hermes config does not contain database URLs or embedding API keys:

```bash
/home/ubuntu/qintopia-msg-sidecar/scripts/hermes/qintopia-context-mcp
```

For the first rollout, add only this block to
`/home/ubuntu/.hermes/profiles/wenyuange/config.yaml`:

```yaml
mcp_servers:
  qintopia-context:
    command: /home/ubuntu/qintopia-msg-sidecar/scripts/hermes/qintopia-context-mcp
    connect_timeout: 60
    timeout: 120
```

Then restart or reload only the WenYuanGe user service:

```bash
systemctl --user restart hermes-gateway-wenyuange.service
systemctl --user is-active hermes-gateway-wenyuange.service
```

Do not add this MCP server to `/home/ubuntu/.hermes/profiles/erhua/config.yaml` until
the WenYuanGe profile has confirmed tool discovery and successful
`qintopia_wenyuange_lookup` calls.

## Rollout Order

1. Build the sidecar and pass `check` against NATS and Postgres.
2. Run migrations.
3. Start `qintopia-message-sidecar.service`.
4. Run `./target/release/qintopia-message-sidecar smoke` to publish a synthetic
   raw/message event and confirm rows in Postgres.
5. Only then enable `QIWE_NATS_CAPTURE_ENABLED=1` for the Hermes QiWe plugin.
6. Configure a real `QINTOPIA_EMBEDDING_API_KEY`, run
   `run-embedding-worker --check-only`, then explicitly enable
   `qintopia-message-embedding-worker.service`.
7. Confirm `qintopia-agentos-operations-workflow-sync.timer` is active so parent
   workflow summaries refresh without relying on manual CLI runs.
8. Confirm `qintopia-agentos-operations-workbench-event.timer` is active so recorded
   review/final-confirmation workbench events are processed without manual CLI runs.
9. Confirm `qintopia-agentos-operations-group-send-ready.timer` is active so
   final-confirmed group-send requests record send-ready audit events without manual CLI
   runs. This does not send messages.
10. Confirm `qintopia-agentos-xiaoman-activity-signal-worker.timer` is active so Xiaoman
    activity `event_signals` create AgentOS intake work items without manual CLI runs.
    This does not read or write Feishu and does not send messages.
11. Confirm `qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer` is active
    so Xiaoman activity requests create evidence/visual child work items without manual
    CLI runs. This does not execute evidence retrieval, create visual assets, read or
    write Feishu, or send messages.
12. Mount `qintopia-context` MCP into the WenYuanGe Hermes profile first. Keep
    the 二花 profile unchanged until the WenYuanGe MCP path is validated.

Hermes publisher failures must remain best-effort only. NATS, sidecar, or Postgres
failures must not affect QiWe webhook ACK or 二花 replies.
