#!/usr/bin/env bash
set -euo pipefail

MODE="${1:-deploy}"
REPO_DIR="${QINTOPIA_SIDECAR_REPO_DIR:-/home/ubuntu/qintopia-msg-sidecar}"
ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-/etc/qintopia/message-sidecar.env}"
QIWE_ENV_FILE="${QINTOPIA_QIWE_ENV_FILE:-/home/ubuntu/.hermes/profiles/erhua/.env}"
SERVICE_NAME="${QINTOPIA_SIDECAR_SERVICE_NAME:-qintopia-message-sidecar.service}"
SERVICE_FILE="/etc/systemd/system/${SERVICE_NAME}"
EMBEDDING_SERVICE_NAME="${QINTOPIA_MESSAGE_EMBEDDING_SERVICE_NAME:-qintopia-message-embedding-worker.service}"
EMBEDDING_SERVICE_FILE="/etc/systemd/system/${EMBEDDING_SERVICE_NAME}"
IDENTITY_SERVICE_NAME="${QINTOPIA_MESSAGE_IDENTITY_SERVICE_NAME:-qintopia-message-identity-worker.service}"
IDENTITY_SERVICE_FILE="/etc/systemd/system/${IDENTITY_SERVICE_NAME}"
PROFILE_SERVICE_NAME="${QINTOPIA_MEMBER_PROFILE_SERVICE_NAME:-qintopia-agentos-member-profile-worker.service}"
PROFILE_SERVICE_FILE="/etc/systemd/system/${PROFILE_SERVICE_NAME}"
GRAPH_SERVICE_NAME="${QINTOPIA_GRAPH_PROJECTION_SERVICE_NAME:-qintopia-agentos-graph-projection-worker.service}"
GRAPH_SERVICE_FILE="/etc/systemd/system/${GRAPH_SERVICE_NAME}"
EVENT_SIGNAL_SERVICE_NAME="${QINTOPIA_EVENT_SIGNAL_SERVICE_NAME:-qintopia-agentos-event-signal-worker.service}"
EVENT_SIGNAL_SERVICE_FILE="/etc/systemd/system/${EVENT_SIGNAL_SERVICE_NAME}"
DIGEST_SERVICE_NAME="${QINTOPIA_DAILY_DIGEST_SERVICE_NAME:-qintopia-agentos-daily-digest-worker.service}"
DIGEST_SERVICE_FILE="/etc/systemd/system/${DIGEST_SERVICE_NAME}"
PUBLISHER_SERVICE_NAME="${QINTOPIA_DAILY_DIGEST_PUBLISHER_SERVICE_NAME:-qintopia-agentos-daily-digest-publisher.service}"
PUBLISHER_SERVICE_FILE="/etc/systemd/system/${PUBLISHER_SERVICE_NAME}"
ARCHIVE_SERVICE_NAME="${QINTOPIA_RAW_ARCHIVE_SERVICE_NAME:-qintopia-agentos-raw-archive-worker.service}"
ARCHIVE_SERVICE_FILE="/etc/systemd/system/${ARCHIVE_SERVICE_NAME}"
WORKFLOW_SYNC_SERVICE_NAME="${QINTOPIA_OPERATIONS_WORKFLOW_SYNC_SERVICE_NAME:-qintopia-agentos-operations-workflow-sync.service}"
WORKFLOW_SYNC_SERVICE_FILE="/etc/systemd/system/${WORKFLOW_SYNC_SERVICE_NAME}"
WORKFLOW_SYNC_TIMER_NAME="${QINTOPIA_OPERATIONS_WORKFLOW_SYNC_TIMER_NAME:-qintopia-agentos-operations-workflow-sync.timer}"
WORKFLOW_SYNC_TIMER_FILE="/etc/systemd/system/${WORKFLOW_SYNC_TIMER_NAME}"
WORKFLOW_SYNC_TIMER_INTERVAL="${QINTOPIA_OPERATIONS_WORKFLOW_SYNC_TIMER_INTERVAL:-2min}"
EVIDENCE_WORKER_SERVICE_NAME="${QINTOPIA_OPERATIONS_EVIDENCE_WORKER_SERVICE_NAME:-qintopia-agentos-operations-evidence-worker.service}"
EVIDENCE_WORKER_SERVICE_FILE="/etc/systemd/system/${EVIDENCE_WORKER_SERVICE_NAME}"
EVIDENCE_WORKER_TIMER_NAME="${QINTOPIA_OPERATIONS_EVIDENCE_WORKER_TIMER_NAME:-qintopia-agentos-operations-evidence-worker.timer}"
EVIDENCE_WORKER_TIMER_FILE="/etc/systemd/system/${EVIDENCE_WORKER_TIMER_NAME}"
EVIDENCE_WORKER_TIMER_INTERVAL="${QINTOPIA_OPERATIONS_EVIDENCE_WORKER_TIMER_INTERVAL:-2min}"
VISUAL_WORKER_SERVICE_NAME="${QINTOPIA_OPERATIONS_VISUAL_WORKER_SERVICE_NAME:-qintopia-agentos-operations-visual-worker.service}"
VISUAL_WORKER_SERVICE_FILE="/etc/systemd/system/${VISUAL_WORKER_SERVICE_NAME}"
VISUAL_WORKER_TIMER_NAME="${QINTOPIA_OPERATIONS_VISUAL_WORKER_TIMER_NAME:-qintopia-agentos-operations-visual-worker.timer}"
VISUAL_WORKER_TIMER_FILE="/etc/systemd/system/${VISUAL_WORKER_TIMER_NAME}"
VISUAL_WORKER_TIMER_INTERVAL="${QINTOPIA_OPERATIONS_VISUAL_WORKER_TIMER_INTERVAL:-2min}"
WORKBENCH_EVENT_SERVICE_NAME="${QINTOPIA_OPERATIONS_WORKBENCH_EVENT_SERVICE_NAME:-qintopia-agentos-operations-workbench-event.service}"
WORKBENCH_EVENT_SERVICE_FILE="/etc/systemd/system/${WORKBENCH_EVENT_SERVICE_NAME}"
WORKBENCH_EVENT_TIMER_NAME="${QINTOPIA_OPERATIONS_WORKBENCH_EVENT_TIMER_NAME:-qintopia-agentos-operations-workbench-event.timer}"
WORKBENCH_EVENT_TIMER_FILE="/etc/systemd/system/${WORKBENCH_EVENT_TIMER_NAME}"
WORKBENCH_EVENT_TIMER_INTERVAL="${QINTOPIA_OPERATIONS_WORKBENCH_EVENT_TIMER_INTERVAL:-1min}"
GROUP_SEND_READY_SERVICE_NAME="${QINTOPIA_OPERATIONS_GROUP_SEND_READY_SERVICE_NAME:-qintopia-agentos-operations-group-send-ready.service}"
GROUP_SEND_READY_SERVICE_FILE="/etc/systemd/system/${GROUP_SEND_READY_SERVICE_NAME}"
GROUP_SEND_READY_TIMER_NAME="${QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_NAME:-qintopia-agentos-operations-group-send-ready.timer}"
GROUP_SEND_READY_TIMER_FILE="/etc/systemd/system/${GROUP_SEND_READY_TIMER_NAME}"
GROUP_SEND_READY_TIMER_INTERVAL="${QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_INTERVAL:-1min}"
XIAOMAN_ACTIVITY_SIGNAL_SERVICE_NAME="${QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_SERVICE_NAME:-qintopia-agentos-xiaoman-activity-signal-worker.service}"
XIAOMAN_ACTIVITY_SIGNAL_SERVICE_FILE="/etc/systemd/system/${XIAOMAN_ACTIVITY_SIGNAL_SERVICE_NAME}"
XIAOMAN_ACTIVITY_SIGNAL_TIMER_NAME="${QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_NAME:-qintopia-agentos-xiaoman-activity-signal-worker.timer}"
XIAOMAN_ACTIVITY_SIGNAL_TIMER_FILE="/etc/systemd/system/${XIAOMAN_ACTIVITY_SIGNAL_TIMER_NAME}"
XIAOMAN_ACTIVITY_SIGNAL_TIMER_INTERVAL="${QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_INTERVAL:-2min}"
XIAOMAN_ACTIVITY_PROMOTION_STARTER_SERVICE_NAME="${QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_SERVICE_NAME:-qintopia-agentos-xiaoman-activity-promotion-starter-worker.service}"
XIAOMAN_ACTIVITY_PROMOTION_STARTER_SERVICE_FILE="/etc/systemd/system/${XIAOMAN_ACTIVITY_PROMOTION_STARTER_SERVICE_NAME}"
XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_NAME="${QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_NAME:-qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer}"
XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_FILE="/etc/systemd/system/${XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_NAME}"
XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_INTERVAL="${QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_INTERVAL:-2min}"
XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_SERVICE_NAME="${QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_SERVICE_NAME:-qintopia-agentos-xiaoman-activity-image-generation-starter-worker.service}"
XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_SERVICE_FILE="/etc/systemd/system/${XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_SERVICE_NAME}"
XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_NAME="${QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_NAME:-qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer}"
XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_FILE="/etc/systemd/system/${XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_NAME}"
XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_INTERVAL="${QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_INTERVAL:-2min}"
XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_SERVICE_NAME="${QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_SERVICE_NAME:-qintopia-agentos-xiaoman-activity-send-request-starter-worker.service}"
XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_SERVICE_FILE="/etc/systemd/system/${XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_SERVICE_NAME}"
XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_NAME="${QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_NAME:-qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer}"
XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_FILE="/etc/systemd/system/${XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_NAME}"
XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_INTERVAL="${QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_INTERVAL:-2min}"
REMOTE="${QINTOPIA_SIDECAR_GIT_REMOTE:-origin}"
BRANCH="${QINTOPIA_SIDECAR_GIT_BRANCH:-main}"
BIN="${REPO_DIR}/target/release/qintopia-message-sidecar"
CARGO_OFFLINE="${QINTOPIA_SIDECAR_CARGO_OFFLINE:-0}"

usage() {
  cat <<'EOF'
Usage: scripts/server-deploy.sh [prepare|deploy|verify]

prepare  Install the systemd unit and a non-secret env template.
deploy   Fetch/ff-only merge, build, migrate, start sidecar service, and run smoke.
verify   Check service state and run readiness/smoke checks.

Required before deploy/verify:
  - Server git deploy key can fetch origin/main.
  - /etc/qintopia/message-sidecar.env contains QINTOPIA_SIDECAR_DATABASE_URL.

Worker units are installed for message embedding, identity resolution, member
profiles, SQL graph projection, event signals, daily digests, Feishu publishing,
raw-message archive, AgentOS operations workflow summary sync, AgentOS evidence and
visual artifact workers, AgentOS workbench event processing, AgentOS group-send
readiness audit, and Xiaoman activity signal intake, activity promotion child starter,
image-generation request starter, and send request starter.

AgentOS operations control-plane dry smoke runs during deploy/verify. The
Postgres apply smoke runs only when QINTOPIA_OPERATIONS_APPLY_SMOKE_ENABLE=1.
EOF
}

log() {
  printf '[sidecar-deploy] %s\n' "$*"
}

repo_cd() {
  cd "$REPO_DIR"
  git rev-parse --is-inside-work-tree >/dev/null
}

install_unit() {
  log "installing systemd unit at ${SERVICE_FILE}"
  sudo tee "$SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia Message Sidecar
After=nats-server.service network-online.target
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run
Restart=always
RestartSec=5
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
  log "installing systemd unit at ${EMBEDDING_SERVICE_FILE}"
  sudo tee "$EMBEDDING_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia Message Embedding Worker
After=network-online.target
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-embedding-worker
Restart=always
RestartSec=15
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
  log "installing systemd unit at ${IDENTITY_SERVICE_FILE}"
  sudo tee "$IDENTITY_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia Message Identity Worker
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
EnvironmentFile=-/home/ubuntu/.hermes/profiles/erhua/.env
ExecStart=${BIN} run-identity-worker
Restart=always
RestartSec=30
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
  log "installing systemd unit at ${PROFILE_SERVICE_FILE}"
  sudo tee "$PROFILE_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia Agent OS Member Profile Worker
After=network-online.target postgresql.service ${IDENTITY_SERVICE_NAME}
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-member-profile-worker
Restart=always
RestartSec=30
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
  log "installing systemd unit at ${GRAPH_SERVICE_FILE}"
  sudo tee "$GRAPH_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia Agent OS SQL Graph Projection Worker
After=network-online.target postgresql.service ${PROFILE_SERVICE_NAME}
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-graph-projection-worker
Restart=always
RestartSec=30
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
  log "installing systemd unit at ${EVENT_SIGNAL_SERVICE_FILE}"
  sudo tee "$EVENT_SIGNAL_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia Agent OS Event Signal Worker
After=network-online.target postgresql.service ${PROFILE_SERVICE_NAME}
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-event-signal-worker
Restart=always
RestartSec=30
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
  log "installing systemd unit at ${DIGEST_SERVICE_FILE}"
  sudo tee "$DIGEST_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia Agent OS Daily Digest Worker
After=network-online.target postgresql.service ${EVENT_SIGNAL_SERVICE_NAME}
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} agentos-daily-digest-worker
Restart=always
RestartSec=30
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
  log "installing systemd unit at ${PUBLISHER_SERVICE_FILE}"
  sudo tee "$PUBLISHER_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia Agent OS Daily Digest Feishu Publisher
After=network-online.target postgresql.service ${DIGEST_SERVICE_NAME}
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-daily-digest-publisher-worker
Restart=always
RestartSec=30
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
  log "installing systemd unit at ${ARCHIVE_SERVICE_FILE}"
  sudo tee "$ARCHIVE_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia Agent OS Raw Message Archive Worker
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-raw-archive-worker
Restart=always
RestartSec=300
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
  log "installing systemd unit at ${WORKFLOW_SYNC_SERVICE_FILE}"
  sudo tee "$WORKFLOW_SYNC_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia AgentOS Operations Workflow Summary Sync
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-workflow-sync-worker --once --apply
NoNewPrivileges=true
PrivateTmp=true
EOF
  log "installing systemd timer at ${WORKFLOW_SYNC_TIMER_FILE}"
  sudo tee "$WORKFLOW_SYNC_TIMER_FILE" >/dev/null <<EOF
[Unit]
Description=Run Qintopia AgentOS operations workflow summary sync

[Timer]
OnBootSec=2min
OnUnitActiveSec=${WORKFLOW_SYNC_TIMER_INTERVAL}
AccuracySec=30s
Persistent=true
Unit=${WORKFLOW_SYNC_SERVICE_NAME}

[Install]
WantedBy=timers.target
EOF
  log "installing systemd unit at ${EVIDENCE_WORKER_SERVICE_FILE}"
  sudo tee "$EVIDENCE_WORKER_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia AgentOS Operations Evidence Worker
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-evidence-worker --once --apply
NoNewPrivileges=true
PrivateTmp=true
EOF
  log "installing systemd timer at ${EVIDENCE_WORKER_TIMER_FILE}"
  sudo tee "$EVIDENCE_WORKER_TIMER_FILE" >/dev/null <<EOF
[Unit]
Description=Run Qintopia AgentOS operations evidence worker

[Timer]
OnBootSec=7min
OnUnitActiveSec=${EVIDENCE_WORKER_TIMER_INTERVAL}
AccuracySec=30s
Persistent=true
Unit=${EVIDENCE_WORKER_SERVICE_NAME}

[Install]
WantedBy=timers.target
EOF
  log "installing systemd unit at ${VISUAL_WORKER_SERVICE_FILE}"
  sudo tee "$VISUAL_WORKER_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia AgentOS Operations Visual Worker
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-collaboration-worker --work-item-type visual_asset_request --once --apply
NoNewPrivileges=true
PrivateTmp=true
EOF
  log "installing systemd timer at ${VISUAL_WORKER_TIMER_FILE}"
  sudo tee "$VISUAL_WORKER_TIMER_FILE" >/dev/null <<EOF
[Unit]
Description=Run Qintopia AgentOS operations visual worker

[Timer]
OnBootSec=8min
OnUnitActiveSec=${VISUAL_WORKER_TIMER_INTERVAL}
AccuracySec=30s
Persistent=true
Unit=${VISUAL_WORKER_SERVICE_NAME}

[Install]
WantedBy=timers.target
EOF
  log "installing systemd unit at ${WORKBENCH_EVENT_SERVICE_FILE}"
  sudo tee "$WORKBENCH_EVENT_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia AgentOS Operations Workbench Event Processor
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-workbench-event-worker --once --apply
NoNewPrivileges=true
PrivateTmp=true
EOF
  log "installing systemd timer at ${WORKBENCH_EVENT_TIMER_FILE}"
  sudo tee "$WORKBENCH_EVENT_TIMER_FILE" >/dev/null <<EOF
[Unit]
Description=Run Qintopia AgentOS operations workbench event processor

[Timer]
OnBootSec=3min
OnUnitActiveSec=${WORKBENCH_EVENT_TIMER_INTERVAL}
AccuracySec=30s
Persistent=true
Unit=${WORKBENCH_EVENT_SERVICE_NAME}

[Install]
WantedBy=timers.target
EOF
  log "installing systemd unit at ${GROUP_SEND_READY_SERVICE_FILE}"
  sudo tee "$GROUP_SEND_READY_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia AgentOS Operations Group Send Readiness
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-group-message-send-worker --once --apply
NoNewPrivileges=true
PrivateTmp=true
EOF
  log "installing systemd timer at ${GROUP_SEND_READY_TIMER_FILE}"
  sudo tee "$GROUP_SEND_READY_TIMER_FILE" >/dev/null <<EOF
[Unit]
Description=Run Qintopia AgentOS operations group send-readiness audit

[Timer]
OnBootSec=4min
OnUnitActiveSec=${GROUP_SEND_READY_TIMER_INTERVAL}
AccuracySec=30s
Persistent=true
Unit=${GROUP_SEND_READY_SERVICE_NAME}

[Install]
WantedBy=timers.target
EOF
  log "installing systemd unit at ${XIAOMAN_ACTIVITY_SIGNAL_SERVICE_FILE}"
  sudo tee "$XIAOMAN_ACTIVITY_SIGNAL_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia AgentOS Xiaoman Activity Signal Worker
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-xiaoman-activity-signal-worker --once --apply
NoNewPrivileges=true
PrivateTmp=true
EOF
  log "installing systemd timer at ${XIAOMAN_ACTIVITY_SIGNAL_TIMER_FILE}"
  sudo tee "$XIAOMAN_ACTIVITY_SIGNAL_TIMER_FILE" >/dev/null <<EOF
[Unit]
Description=Run Qintopia AgentOS Xiaoman activity signal intake

[Timer]
OnBootSec=5min
OnUnitActiveSec=${XIAOMAN_ACTIVITY_SIGNAL_TIMER_INTERVAL}
AccuracySec=30s
Persistent=true
Unit=${XIAOMAN_ACTIVITY_SIGNAL_SERVICE_NAME}

[Install]
WantedBy=timers.target
EOF
  log "installing systemd unit at ${XIAOMAN_ACTIVITY_PROMOTION_STARTER_SERVICE_FILE}"
  sudo tee "$XIAOMAN_ACTIVITY_PROMOTION_STARTER_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia AgentOS Xiaoman Activity Promotion Starter Worker
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-xiaoman-activity-promotion-starter-worker --once --apply
NoNewPrivileges=true
PrivateTmp=true
EOF
  log "installing systemd timer at ${XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_FILE}"
  sudo tee "$XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_FILE" >/dev/null <<EOF
[Unit]
Description=Run Qintopia AgentOS Xiaoman activity promotion child starter

[Timer]
OnBootSec=6min
OnUnitActiveSec=${XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_INTERVAL}
AccuracySec=30s
Persistent=true
Unit=${XIAOMAN_ACTIVITY_PROMOTION_STARTER_SERVICE_NAME}

[Install]
WantedBy=timers.target
EOF
  log "installing systemd unit at ${XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_SERVICE_FILE}"
  sudo tee "$XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia AgentOS Xiaoman Activity Image Generation Starter Worker
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-xiaoman-activity-image-generation-starter-worker --once --apply
NoNewPrivileges=true
PrivateTmp=true
EOF
  log "installing systemd timer at ${XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_FILE}"
  sudo tee "$XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_FILE" >/dev/null <<EOF
[Unit]
Description=Run Qintopia AgentOS Xiaoman activity image-generation request starter

[Timer]
OnBootSec=9min
OnUnitActiveSec=${XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_INTERVAL}
AccuracySec=30s
Persistent=true
Unit=${XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_SERVICE_NAME}

[Install]
WantedBy=timers.target
EOF
  log "installing systemd unit at ${XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_SERVICE_FILE}"
  sudo tee "$XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_SERVICE_FILE" >/dev/null <<EOF
[Unit]
Description=Qintopia AgentOS Xiaoman Activity Send Request Starter Worker
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=${REPO_DIR}
EnvironmentFile=${ENV_FILE}
ExecStart=${BIN} run-xiaoman-activity-send-request-starter-worker --once --apply
NoNewPrivileges=true
PrivateTmp=true
EOF
  log "installing systemd timer at ${XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_FILE}"
  sudo tee "$XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_FILE" >/dev/null <<EOF
[Unit]
Description=Run Qintopia AgentOS Xiaoman activity send request starter

[Timer]
OnBootSec=10min
OnUnitActiveSec=${XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_INTERVAL}
AccuracySec=30s
Persistent=true
Unit=${XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_SERVICE_NAME}

[Install]
WantedBy=timers.target
EOF
  sudo systemctl daemon-reload
}

install_env_template() {
  sudo install -d -m 0750 -o ubuntu -g ubuntu "$(dirname "$ENV_FILE")"
  if [[ -f "$ENV_FILE" ]]; then
    log "env file already exists at ${ENV_FILE}; leaving it unchanged"
    return
  fi

  log "creating non-secret env template at ${ENV_FILE}"
  sudo tee "$ENV_FILE" >/dev/null <<'EOF'
QINTOPIA_SIDECAR_NATS_URL=nats://127.0.0.1:4222
QINTOPIA_SIDECAR_NATS_STREAM=QINTOPIA_QIWE_MESSAGES
QINTOPIA_SIDECAR_RAW_SUBJECT=qintopia.qiwe.raw
QINTOPIA_SIDECAR_MESSAGE_SUBJECT=qintopia.qiwe.message
QINTOPIA_SIDECAR_CONSUMER=qintopia-message-sidecar
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
QINTOPIA_PROFILE_TARGET_CHAT_IDS=10859791146538059
QINTOPIA_PROFILE_EXCLUDED_DISPLAY_NAMES=秦托邦小客服
QINTOPIA_CHAT_METADATA_JSON='{"10859791146538059":{"display_name":"秦托邦的小伙伴（新）","source":"manual_config"}}'
QINTOPIA_MEMBER_PROFILE_WORKER_BATCH_SIZE=500
QINTOPIA_MEMBER_PROFILE_WORKER_POLL_SECONDS=300
QINTOPIA_GRAPH_PROJECTION_WORKER_BATCH_SIZE=500
QINTOPIA_GRAPH_PROJECTION_WORKER_POLL_SECONDS=300
QINTOPIA_EVENT_SIGNAL_WORKER_BATCH_SIZE=2000
QINTOPIA_EVENT_SIGNAL_WORKER_POLL_SECONDS=300
QINTOPIA_DAILY_DIGEST_TIME=03:00
QINTOPIA_DAILY_DIGEST_TIMEZONE=Asia/Shanghai
QINTOPIA_DAILY_DIGEST_OWNER_AGENT=xiaoman
QINTOPIA_DAILY_DIGEST_WORKER_POLL_SECONDS=60
QINTOPIA_DAILY_DIGEST_PUBLISHER_BATCH_SIZE=10
QINTOPIA_DAILY_DIGEST_PUBLISHER_POLL_SECONDS=120
QINTOPIA_DAILY_DIGEST_PUBLISHER_AGENT=xiaoman
QINTOPIA_DAILY_DIGEST_FEISHU_BASE_TOKEN=replace-with-base-token
QINTOPIA_DAILY_DIGEST_ALLOWED_FEISHU_BASE_TOKENS=replace-with-base-token
QINTOPIA_DAILY_DIGEST_FEISHU_DAILY_TABLE_ID=replace-with-daily-table-id
# Fill these after scripts/setup-daily-digest-base.py --apply creates the tables.
# QINTOPIA_DAILY_DIGEST_FEISHU_SIGNAL_TABLE_ID=
# QINTOPIA_DAILY_DIGEST_FEISHU_ARCHIVE_TABLE_ID=
QINTOPIA_DAILY_DIGEST_FEISHU_PROFILE_ENV_PATH=/home/ubuntu/.hermes/profiles/xiaoman/.env
# Agent assignment rules are versioned in the repository by default.
QINTOPIA_DAILY_DIGEST_DISPATCH_RULES_PATH=config/agentos/daily-digest-dispatch-rules.json
# Optional JSON override for mapping digest signal types to Agent assignments.
# QINTOPIA_DAILY_DIGEST_DISPATCH_RULES_JSON=[{"signal":"activity_events","agent":"小满","template":"补录和复盘今日活动事件。"}]
QINTOPIA_GRAPH_BACKEND=sql
QINTOPIA_AGE_ENABLED=false
QINTOPIA_RAW_MESSAGE_HOT_RETENTION_DAYS=30
QINTOPIA_RAW_ARCHIVE_FORMAT=jsonl.zst
# QINTOPIA_RAW_ARCHIVE_DIR=/var/lib/qintopia/raw-message-archive
# AgentOS operations workflow summary sync is installed as a systemd timer.
# The timer interval is controlled at deploy-time by
# QINTOPIA_OPERATIONS_WORKFLOW_SYNC_TIMER_INTERVAL in scripts/server-deploy.sh.
# AgentOS evidence and visual workers are installed as timers. They only process
# AgentOS evidence_request / visual_asset_request work_items into internal
# evidence_summary / poster_brief artifacts; they do not call Feishu, QiWe,
# Wenyuange live search, Huabaosi production generation, or external adapters.
# QINTOPIA_OPERATIONS_EVIDENCE_WORKER_TIMER_INTERVAL is read at deploy-time.
# QINTOPIA_OPERATIONS_VISUAL_WORKER_TIMER_INTERVAL is read at deploy-time.
# AgentOS workbench event processing is also installed as a timer. It only
# processes already-recorded human_workbench_event_recorded events.
# QINTOPIA_OPERATIONS_WORKBENCH_EVENT_TIMER_INTERVAL is read at deploy-time.
# AgentOS group-send readiness audit is installed as a timer. It only records
# send-ready events and never calls QiWe/Erhua production send adapters.
# QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_INTERVAL is read at deploy-time.
# Xiaoman activity signal intake is installed as a timer. It only scans
# owner_agent=xiaoman event_signals and writes AgentOS work_items; it does not
# read/write Feishu, call QiWe, create visual assets, or send externally.
# QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_INTERVAL is read at deploy-time.
# Xiaoman activity promotion starter is installed as a timer. It only creates
# missing AgentOS evidence/visual child work_items under existing Xiaoman
# activity request parents; it does not read/write Feishu, call QiWe, execute
# evidence retrieval, create visual assets, or send externally.
# QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_INTERVAL is read at deploy-time.
# Xiaoman activity image-generation starter is installed as a timer. It only creates
# AgentOS image_generation_request work_items from approved poster_brief artifacts; it
# does not call the image provider worker, upload media, create generated images,
# read/write Feishu, call QiWe, publish, or send externally.
# QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_INTERVAL is read at deploy-time.
# Xiaoman activity send request starter is installed as a timer. It only creates
# awaiting_publish AgentOS group_message_request work_items from approved Xiaoman
# poster_brief artifacts; it does not record final confirmation, queue, send,
# publish, read/write Feishu, call QiWe, or run external adapters.
# QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_INTERVAL is read at deploy-time.
RUST_LOG=info,qintopia_message_sidecar=debug
EOF
  sudo chown root:ubuntu "$ENV_FILE"
  sudo chmod 0640 "$ENV_FILE"
}

embedding_key_is_placeholder() {
  if ! sudo grep -Eq '^QINTOPIA_EMBEDDING_API_KEY=' "$ENV_FILE"; then
    return 0
  fi
  sudo grep -Eq '^QINTOPIA_EMBEDDING_API_KEY=(replace-with-server-secret|change-me|placeholder|)$' "$ENV_FILE"
}

require_database_url() {
  if ! sudo grep -Eq '^QINTOPIA_SIDECAR_DATABASE_URL=postgres://' "$ENV_FILE"; then
    cat >&2 <<EOF
Missing QINTOPIA_SIDECAR_DATABASE_URL in ${ENV_FILE}.
Add a server-local URL such as:
  QINTOPIA_SIDECAR_DATABASE_URL=postgres://USER:PASSWORD@127.0.0.1:55432/DBNAME
EOF
    exit 2
  fi
}

source_env() {
  set -a
  # shellcheck disable=SC1090
  source "$ENV_FILE"
  if [[ -f "$QIWE_ENV_FILE" ]]; then
    # shellcheck disable=SC1090
    source "$QIWE_ENV_FILE"
  fi
  set +a
}

git_update() {
  repo_cd
  log "fetching ${REMOTE}/${BRANCH}"
  git fetch "$REMOTE"
  log "merging ${REMOTE}/${BRANCH} with ff-only"
  git merge --ff-only "${REMOTE}/${BRANCH}"
  if ! git diff --quiet || ! git diff --cached --quiet; then
    git status --short
    echo "Tracked working tree changes remain after merge." >&2
    exit 3
  fi
}

build_release() {
  repo_cd
  log "building with server Rust toolchain"
  local cargo_flags=(--locked)
  if [[ "$CARGO_OFFLINE" == "1" ]]; then
    cargo_flags=(--offline --locked)
  fi
  cargo check "${cargo_flags[@]}"
  cargo test "${cargo_flags[@]}"
  cargo build --release "${cargo_flags[@]}"
}

run_migrations_and_checks() {
  repo_cd
  require_database_url
  source_env
  log "running migrations"
  "$BIN" migrate
  log "running readiness check"
  "$BIN" check
  log "running embedding worker config/database check"
  "$BIN" run-embedding-worker --check-only
  log "running identity worker one-cycle check"
  "$BIN" run-identity-worker --check-only
  log "running member profile worker one-cycle check"
  "$BIN" run-member-profile-worker --check-only --quiet
  log "running graph projection worker one-cycle check"
  "$BIN" run-graph-projection-worker --check-only
  log "running event signal worker one-cycle check"
  "$BIN" run-event-signal-worker --check-only
  log "running daily digest worker dry-run check"
  "$BIN" agentos-daily-digest-worker --dry-run --quiet
  log "running daily digest publisher dry-run check"
  "$BIN" run-daily-digest-publisher-worker --check-only
  log "running raw archive worker one-cycle check"
  "$BIN" run-raw-archive-worker --check-only
  log "running operations workflow sync worker dry-run check"
  "$BIN" run-workflow-sync-worker --once --dry-run
  log "running operations evidence worker dry-run check"
  "$BIN" run-evidence-worker --once --dry-run
  log "running operations visual worker dry-run check"
  "$BIN" run-collaboration-worker --work-item-type visual_asset_request --once --dry-run
  log "running operations workbench event worker dry-run check"
  "$BIN" run-workbench-event-worker --once --dry-run
  log "running operations group send-ready worker dry-run check"
  "$BIN" run-group-message-send-worker --once --dry-run
  log "running Xiaoman activity signal worker dry-run check"
  "$BIN" run-xiaoman-activity-signal-worker --check-only
  log "running Xiaoman activity promotion starter worker dry-run check"
  "$BIN" run-xiaoman-activity-promotion-starter-worker --check-only
  log "running Xiaoman activity image-generation starter worker dry-run check"
  "$BIN" run-xiaoman-activity-image-generation-starter-worker --check-only
  log "running Xiaoman activity send request starter worker dry-run check"
  "$BIN" run-xiaoman-activity-send-request-starter-worker --check-only
}

start_service() {
  log "starting ${SERVICE_NAME}"
  sudo systemctl enable --now "$SERVICE_NAME"
  systemctl is-active "$SERVICE_NAME"
  log "starting worker services"
  sudo systemctl enable --now "$IDENTITY_SERVICE_NAME"
  sudo systemctl enable --now "$PROFILE_SERVICE_NAME"
  sudo systemctl enable --now "$GRAPH_SERVICE_NAME"
  sudo systemctl enable --now "$EVENT_SIGNAL_SERVICE_NAME"
  sudo systemctl enable --now "$DIGEST_SERVICE_NAME"
  sudo systemctl enable --now "$PUBLISHER_SERVICE_NAME"
  sudo systemctl enable --now "$ARCHIVE_SERVICE_NAME"
  sudo systemctl enable --now "$WORKFLOW_SYNC_TIMER_NAME"
  sudo systemctl enable --now "$EVIDENCE_WORKER_TIMER_NAME"
  sudo systemctl enable --now "$VISUAL_WORKER_TIMER_NAME"
  sudo systemctl enable --now "$WORKBENCH_EVENT_TIMER_NAME"
  sudo systemctl enable --now "$GROUP_SEND_READY_TIMER_NAME"
  sudo systemctl enable --now "$XIAOMAN_ACTIVITY_SIGNAL_TIMER_NAME"
  sudo systemctl enable --now "$XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_NAME"
  sudo systemctl enable --now "$XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_NAME"
  sudo systemctl enable --now "$XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_NAME"
  systemctl is-active "$IDENTITY_SERVICE_NAME"
  systemctl is-active "$PROFILE_SERVICE_NAME"
  systemctl is-active "$GRAPH_SERVICE_NAME"
  systemctl is-active "$EVENT_SIGNAL_SERVICE_NAME"
  systemctl is-active "$DIGEST_SERVICE_NAME"
  systemctl is-active "$PUBLISHER_SERVICE_NAME"
  systemctl is-active "$ARCHIVE_SERVICE_NAME"
  systemctl is-active "$WORKFLOW_SYNC_TIMER_NAME"
  systemctl is-active "$EVIDENCE_WORKER_TIMER_NAME"
  systemctl is-active "$VISUAL_WORKER_TIMER_NAME"
  systemctl is-active "$WORKBENCH_EVENT_TIMER_NAME"
  systemctl is-active "$GROUP_SEND_READY_TIMER_NAME"
  systemctl is-active "$XIAOMAN_ACTIVITY_SIGNAL_TIMER_NAME"
  systemctl is-active "$XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_NAME"
  systemctl is-active "$XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_NAME"
  systemctl is-active "$XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_NAME"
}

run_smoke() {
  repo_cd
  require_database_url
  source_env
  log "running smoke check"
  "$BIN" smoke
}

run_operations_control_plane_smoke() {
  repo_cd
  log "running AgentOS operations control-plane dry smoke"
  QINTOPIA_SIDECAR_BIN="$BIN" scripts/operations-control-plane-smoke.sh
}

run_operations_control_plane_apply_smoke_if_enabled() {
  repo_cd
  require_database_url
  source_env
  log "running AgentOS operations control-plane guarded apply smoke"
  QINTOPIA_SIDECAR_BIN="$BIN" scripts/operations-control-plane-apply-smoke.sh
}

verify_services() {
  log "checking dependent services"
  systemctl is-active nats-server.service
  systemctl is-active "$SERVICE_NAME"
  if embedding_key_is_placeholder; then
    log "${EMBEDDING_SERVICE_NAME} remains disabled because QINTOPIA_EMBEDDING_API_KEY is a placeholder"
    systemctl is-enabled "$EMBEDDING_SERVICE_NAME" || true
  else
    systemctl is-active "$EMBEDDING_SERVICE_NAME"
  fi
  systemctl is-active "$IDENTITY_SERVICE_NAME"
  systemctl is-active "$PROFILE_SERVICE_NAME"
  systemctl is-active "$GRAPH_SERVICE_NAME"
  systemctl is-active "$EVENT_SIGNAL_SERVICE_NAME"
  systemctl is-active "$DIGEST_SERVICE_NAME"
  systemctl is-active "$PUBLISHER_SERVICE_NAME"
  systemctl is-active "$ARCHIVE_SERVICE_NAME"
  systemctl is-active "$WORKFLOW_SYNC_TIMER_NAME"
  systemctl is-active "$EVIDENCE_WORKER_TIMER_NAME"
  systemctl is-active "$VISUAL_WORKER_TIMER_NAME"
  systemctl is-active "$WORKBENCH_EVENT_TIMER_NAME"
  systemctl is-active "$GROUP_SEND_READY_TIMER_NAME"
  systemctl is-active "$XIAOMAN_ACTIVITY_SIGNAL_TIMER_NAME"
  systemctl is-active "$XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_NAME"
  systemctl is-active "$XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_NAME"
  systemctl is-active "$XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_NAME"
  systemctl --user is-active hermes-gateway-erhua.service
}

case "$MODE" in
  -h|--help|help)
    usage
    ;;
  prepare)
    install_env_template
    install_unit
    ;;
  deploy)
    install_env_template
    git_update
    install_unit
    build_release
    run_migrations_and_checks
    start_service
    run_smoke
    run_operations_control_plane_smoke
    run_operations_control_plane_apply_smoke_if_enabled
    log "reading embedding queue counts"
    "$BIN" run-embedding-worker --check-only
    verify_services
    ;;
  verify)
    run_migrations_and_checks
    verify_services
    run_smoke
    run_operations_control_plane_smoke
    run_operations_control_plane_apply_smoke_if_enabled
    log "reading embedding queue counts"
    "$BIN" run-embedding-worker --check-only
    ;;
  *)
    usage >&2
    exit 64
    ;;
esac
