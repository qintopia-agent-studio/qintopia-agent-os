#!/usr/bin/env bash
set -euo pipefail

TARGET_SHA="${QINTOPIA_M9_TARGET_SHA:-m9-preview}"
MONOREPO_DIR="${QINTOPIA_MONOREPO_DIR:-/home/ubuntu/qintopia-agent-os-monorepo}"
ARTIFACT_ROOT="${QINTOPIA_ARTIFACT_ROOT:-/home/ubuntu/qintopia-agent-os-artifacts}"
ARTIFACT_DIR="${QINTOPIA_ARTIFACT_DIR:-}"
ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-/etc/qintopia/message-sidecar.env}"
IDENTITY_ENV_FILE="${QINTOPIA_QIWE_ENV_FILE:-/home/ubuntu/.hermes/profiles/erhua/.env}"
MIGRATIONS_DIR="${QINTOPIA_SIDECAR_MIGRATIONS_DIR:-${MONOREPO_DIR}/runtime/postgres/migrations}"
OUTPUT_DIR="${QINTOPIA_SYSTEMD_OUTPUT_DIR:-}"
CHECK_ONLY=0
OUTPUT_DIR_EXPLICIT=0

usage() {
  cat <<'EOF'
Usage: deploy/sidecar/scripts/render-systemd-units.sh [options]

Render the M9 monorepo-native sidecar systemd unit plan into a local output
directory. This script never writes to /etc/systemd/system and never restarts
services.

Options:
  --target-sha <sha>        Approved monorepo commit SHA for artifact path.
  --monorepo-dir <path>     Server monorepo checkout path.
  --artifact-dir <path>     Verified CI artifact directory.
  --artifact-root <path>    Artifact root used with --target-sha.
  --env-file <path>         Server sidecar environment file.
  --identity-env-file <path>
                            Optional QiWe identity environment file.
  --migrations-dir <path>   Sidecar migrations directory passed to services.
  --output-dir <path>       Local render output directory.
  --check                   Render to a temporary directory and validate output.
  -h, --help                Show this help.

Defaults are production-shaped but non-mutating:
  monorepo-dir:  /home/ubuntu/qintopia-agent-os-monorepo
  artifact-root: /home/ubuntu/qintopia-agent-os-artifacts
  env-file:      /etc/qintopia/message-sidecar.env
EOF
}

while [[ $# -gt 0 ]]; do
  case "$1" in
    --target-sha)
      TARGET_SHA="${2:-}"
      shift 2
      ;;
    --monorepo-dir)
      MONOREPO_DIR="${2:-}"
      shift 2
      ;;
    --artifact-dir)
      ARTIFACT_DIR="${2:-}"
      shift 2
      ;;
    --artifact-root)
      ARTIFACT_ROOT="${2:-}"
      shift 2
      ;;
    --env-file)
      ENV_FILE="${2:-}"
      shift 2
      ;;
    --identity-env-file)
      IDENTITY_ENV_FILE="${2:-}"
      shift 2
      ;;
    --migrations-dir)
      MIGRATIONS_DIR="${2:-}"
      shift 2
      ;;
    --output-dir)
      OUTPUT_DIR="${2:-}"
      OUTPUT_DIR_EXPLICIT=1
      shift 2
      ;;
    --check)
      CHECK_ONLY=1
      shift
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "Unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

if [[ -z "$TARGET_SHA" ]]; then
  echo "--target-sha is required" >&2
  exit 2
fi

if [[ -z "$ARTIFACT_DIR" ]]; then
  ARTIFACT_DIR="${ARTIFACT_ROOT}/${TARGET_SHA}"
fi

BIN="${ARTIFACT_DIR}/qintopia-message-sidecar"

if [[ -z "$OUTPUT_DIR" ]]; then
  if [[ "$CHECK_ONLY" == "1" ]]; then
    OUTPUT_DIR="$(mktemp -d)"
  else
    OUTPUT_DIR="dist/sidecar-systemd-units/${TARGET_SHA}"
  fi
fi

case "$OUTPUT_DIR" in
  /etc/systemd/system | /etc/systemd/system/*)
    echo "Refusing to render directly into ${OUTPUT_DIR}; render locally and review first." >&2
    exit 3
    ;;
esac

cleanup() {
  if [[ "$CHECK_ONLY" == "1" && "$OUTPUT_DIR_EXPLICIT" == "0" ]]; then
    rm -rf "$OUTPUT_DIR"
  fi
}
trap cleanup EXIT

write_file() {
  local relative_path="$1"
  local absolute_path="${OUTPUT_DIR}/${relative_path}"
  mkdir -p "$(dirname "$absolute_path")"
  cat >"$absolute_path"
}

render_long_running_service() {
  local service_name="$1"
  local description="$2"
  local after="$3"
  local command="$4"
  local restart_sec="$5"
  local extra_env_file="${6:-}"

  write_file "$service_name" <<EOF
[Unit]
Description=${description}
After=${after}
Wants=network-online.target

[Service]
Type=simple
User=ubuntu
Group=ubuntu
WorkingDirectory=${MONOREPO_DIR}
EnvironmentFile=${ENV_FILE}
${extra_env_file}
Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=${TARGET_SHA}
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=${MIGRATIONS_DIR}
ExecStart=${BIN} ${command}
Restart=always
RestartSec=${restart_sec}
NoNewPrivileges=true
PrivateTmp=true

[Install]
WantedBy=multi-user.target
EOF
}

render_oneshot_service() {
  local service_name="$1"
  local description="$2"
  local command="$3"

  write_file "$service_name" <<EOF
[Unit]
Description=${description}
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=${MONOREPO_DIR}
EnvironmentFile=${ENV_FILE}
Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=${TARGET_SHA}
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=${MIGRATIONS_DIR}
ExecStart=${BIN} ${command}
NoNewPrivileges=true
PrivateTmp=true
EOF
}

render_guarded_oneshot_service() {
  local service_name="$1"
  local description="$2"
  local preflight_command="$3"
  local command="$4"

  write_file "$service_name" <<EOF
[Unit]
Description=${description}
After=network-online.target postgresql.service
Wants=network-online.target

[Service]
Type=oneshot
User=ubuntu
Group=ubuntu
WorkingDirectory=${MONOREPO_DIR}
EnvironmentFile=${ENV_FILE}
Environment=QINTOPIA_DEPLOYED_COMMIT_SHA=${TARGET_SHA}
Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=${MIGRATIONS_DIR}
ExecStartPre=${BIN} ${preflight_command}
ExecStart=${BIN} ${command}
NoNewPrivileges=true
PrivateTmp=true
EOF
}

render_timer() {
  local timer_name="$1"
  local description="$2"
  local service_name="$3"
  local boot_sec="$4"
  local active_sec="$5"

  write_file "$timer_name" <<EOF
[Unit]
Description=${description}

[Timer]
OnBootSec=${boot_sec}
OnUnitActiveSec=${active_sec}
AccuracySec=30s
Persistent=true
Unit=${service_name}

[Install]
WantedBy=timers.target
EOF
}

render_plan() {
  write_file "_M9_SYSTEMD_PLAN.txt" <<EOF
M9 sidecar systemd render plan

Target SHA: ${TARGET_SHA}
Monorepo checkout: ${MONOREPO_DIR}
Artifact directory: ${ARTIFACT_DIR}
Runtime binary: ${BIN}
Environment file: ${ENV_FILE}
Optional identity environment file: ${IDENTITY_ENV_FILE}
Migrations directory: ${MIGRATIONS_DIR}

This output is a review artifact. It is not an installer.

Install scope for the M9 window:
- Install or update only units approved by the owner for the migration window.
- Restart the main sidecar first, then workers that were already active before cutover.
- Do not enable new worker services or timers by default unless this plan names an
  owner-reviewed default-enabled exception.
- Operations timers are rendered for review but should remain disabled unless explicitly approved.
- The Xiaoman activity signal worker timer may be enabled by default after owner
  review because it writes only AgentOS work_items from Xiaoman event_signals.
  It must not write Feishu, send QiWe messages, or create visual assets.
- The Xiaoman activity promotion starter timer may be enabled by default after
  owner review because it only creates missing AgentOS evidence/visual child
  work_items under existing Xiaoman activity request parents. It must not write
  Feishu, send QiWe messages, create visual assets, or run external adapters.
- The Xiaoman activity image-generation starter timer may be enabled by default after
  owner review because it only creates AgentOS image_generation_request work_items from
  approved poster_brief artifacts. It must not call an image provider, upload media,
  create generated images, write Feishu, call QiWe, publish, or send externally.
- The Xiaoman activity send request starter timer may be enabled by default after
  owner review because it only creates awaiting_publish AgentOS group_message_request
  work_items from approved Xiaoman poster_brief artifacts. It must not confirm,
  queue, send, publish, write Feishu, call QiWe, or run external adapters.
- The operations evidence and visual worker timers may be enabled after the Xiaoman
  child intake path is observed healthy. They only process AgentOS work_items into
  internal evidence_summary and poster_brief artifacts. They must not write Feishu,
  send QiWe messages, run live Wenyuange search, call Huabaosi production generation,
  or run external adapters.

Apply shape during the approved window:
1. Copy reviewed unit files into /etc/systemd/system.
2. Run sudo systemctl daemon-reload.
3. Restart approved services one by one.
4. Verify status and journal output after each restart.

Rollback shape:
1. Restore previous unit files that pointed to /home/ubuntu/qintopia-msg-sidecar.
2. Run sudo systemctl daemon-reload.
3. Restart the previous sidecar service family.
4. Verify the old binary and service health.
EOF
}

render_all() {
  mkdir -p "$OUTPUT_DIR"
  render_plan

  render_long_running_service \
    "qintopia-message-sidecar.service" \
    "Qintopia Message Sidecar" \
    "nats-server.service network-online.target" \
    "run" \
    "5"

  render_long_running_service \
    "qintopia-message-embedding-worker.service" \
    "Qintopia Message Embedding Worker" \
    "network-online.target" \
    "run-embedding-worker" \
    "15"

  render_long_running_service \
    "qintopia-message-identity-worker.service" \
    "Qintopia Message Identity Worker" \
    "network-online.target postgresql.service" \
    "run-identity-worker" \
    "30" \
    "EnvironmentFile=-${IDENTITY_ENV_FILE}"

  render_long_running_service \
    "qintopia-agentos-member-profile-worker.service" \
    "Qintopia Agent OS Member Profile Worker" \
    "network-online.target postgresql.service qintopia-message-identity-worker.service" \
    "run-member-profile-worker" \
    "30"

  render_long_running_service \
    "qintopia-agentos-graph-projection-worker.service" \
    "Qintopia Agent OS SQL Graph Projection Worker" \
    "network-online.target postgresql.service qintopia-agentos-member-profile-worker.service" \
    "run-graph-projection-worker" \
    "30"

  render_long_running_service \
    "qintopia-agentos-event-signal-worker.service" \
    "Qintopia Agent OS Event Signal Worker" \
    "network-online.target postgresql.service qintopia-agentos-member-profile-worker.service" \
    "run-event-signal-worker" \
    "30"

  render_long_running_service \
    "qintopia-agentos-daily-digest-worker.service" \
    "Qintopia Agent OS Daily Digest Worker" \
    "network-online.target postgresql.service qintopia-agentos-event-signal-worker.service" \
    "agentos-daily-digest-worker" \
    "30"

  render_long_running_service \
    "qintopia-agentos-daily-digest-publisher.service" \
    "Qintopia Agent OS Daily Digest Feishu Publisher" \
    "network-online.target postgresql.service qintopia-agentos-daily-digest-worker.service" \
    "run-daily-digest-publisher-worker" \
    "30"

  render_long_running_service \
    "qintopia-agentos-raw-archive-worker.service" \
    "Qintopia Agent OS Raw Message Archive Worker" \
    "network-online.target postgresql.service" \
    "run-raw-archive-worker" \
    "300"

  render_oneshot_service \
    "qintopia-agentos-operations-workflow-sync.service" \
    "Qintopia AgentOS Operations Workflow Summary Sync" \
    "run-workflow-sync-worker --once --apply"
  render_timer \
    "qintopia-agentos-operations-workflow-sync.timer" \
    "Run Qintopia AgentOS operations workflow summary sync" \
    "qintopia-agentos-operations-workflow-sync.service" \
    "2min" \
    "${QINTOPIA_OPERATIONS_WORKFLOW_SYNC_TIMER_INTERVAL:-2min}"

  render_oneshot_service \
    "qintopia-agentos-operations-evidence-worker.service" \
    "Qintopia AgentOS Operations Evidence Worker" \
    "run-evidence-worker --once --apply"
  render_timer \
    "qintopia-agentos-operations-evidence-worker.timer" \
    "Run Qintopia AgentOS operations evidence worker" \
    "qintopia-agentos-operations-evidence-worker.service" \
    "7min" \
    "${QINTOPIA_OPERATIONS_EVIDENCE_WORKER_TIMER_INTERVAL:-2min}"

  render_oneshot_service \
    "qintopia-agentos-operations-visual-worker.service" \
    "Qintopia AgentOS Operations Visual Worker" \
    "run-collaboration-worker --work-item-type visual_asset_request --once --apply"
  render_timer \
    "qintopia-agentos-operations-visual-worker.timer" \
    "Run Qintopia AgentOS operations visual worker" \
    "qintopia-agentos-operations-visual-worker.service" \
    "8min" \
    "${QINTOPIA_OPERATIONS_VISUAL_WORKER_TIMER_INTERVAL:-2min}"

  render_oneshot_service \
    "qintopia-agentos-operations-workbench-event.service" \
    "Qintopia AgentOS Operations Workbench Event Processor" \
    "run-workbench-event-worker --once --apply"
  render_timer \
    "qintopia-agentos-operations-workbench-event.timer" \
    "Run Qintopia AgentOS operations workbench event processor" \
    "qintopia-agentos-operations-workbench-event.service" \
    "3min" \
    "${QINTOPIA_OPERATIONS_WORKBENCH_EVENT_TIMER_INTERVAL:-1min}"

  render_oneshot_service \
    "qintopia-agentos-operations-group-send-ready.service" \
    "Qintopia AgentOS Operations Group Send Readiness" \
    "run-group-message-send-worker --once --apply"
  render_timer \
    "qintopia-agentos-operations-group-send-ready.timer" \
    "Run Qintopia AgentOS operations group send-readiness audit" \
    "qintopia-agentos-operations-group-send-ready.service" \
    "4min" \
    "${QINTOPIA_OPERATIONS_GROUP_SEND_READY_TIMER_INTERVAL:-1min}"

  render_oneshot_service \
    "qintopia-agentos-xiaoman-activity-signal-worker.service" \
    "Qintopia AgentOS Xiaoman Activity Signal Worker" \
    "run-xiaoman-activity-signal-worker --once --apply"
  render_timer \
    "qintopia-agentos-xiaoman-activity-signal-worker.timer" \
    "Run Qintopia AgentOS Xiaoman activity signal intake" \
    "qintopia-agentos-xiaoman-activity-signal-worker.service" \
    "5min" \
    "${QINTOPIA_XIAOMAN_ACTIVITY_SIGNAL_TIMER_INTERVAL:-2min}"

  render_oneshot_service \
    "qintopia-agentos-xiaoman-activity-promotion-starter-worker.service" \
    "Qintopia AgentOS Xiaoman Activity Promotion Starter Worker" \
    "run-xiaoman-activity-promotion-starter-worker --once --apply"
  render_timer \
    "qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer" \
    "Run Qintopia AgentOS Xiaoman activity promotion child starter" \
    "qintopia-agentos-xiaoman-activity-promotion-starter-worker.service" \
    "6min" \
    "${QINTOPIA_XIAOMAN_ACTIVITY_PROMOTION_STARTER_TIMER_INTERVAL:-2min}"

  render_oneshot_service \
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.service" \
    "Qintopia AgentOS Xiaoman Activity Image Generation Starter Worker" \
    "run-xiaoman-activity-image-generation-starter-worker --once --apply"
  render_timer \
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer" \
    "Run Qintopia AgentOS Xiaoman activity image-generation request starter" \
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.service" \
    "9min" \
    "${QINTOPIA_XIAOMAN_ACTIVITY_IMAGE_GENERATION_STARTER_TIMER_INTERVAL:-2min}"

  render_oneshot_service \
    "qintopia-agentos-huabaosi-image-generation-preflight.service" \
    "Qintopia AgentOS Huabaosi Image Generation Production Preflight" \
    "huabaosi-image-generation-preflight"
  render_guarded_oneshot_service \
    "qintopia-agentos-huabaosi-image-generation-worker.service" \
    "Qintopia AgentOS Huabaosi Image Generation Worker" \
    "huabaosi-image-generation-preflight" \
    "run-huabaosi-image-generation-worker --once --apply"
  render_timer \
    "qintopia-agentos-huabaosi-image-generation-worker.timer" \
    "Run Qintopia AgentOS Huabaosi image generation worker" \
    "qintopia-agentos-huabaosi-image-generation-worker.service" \
    "11min" \
    "${QINTOPIA_HUABAOSI_IMAGE_GENERATION_TIMER_INTERVAL:-5min}"

  render_oneshot_service \
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service" \
    "Qintopia AgentOS Huabaosi Feishu Artifact Mirror Production Preflight" \
    "huabaosi-feishu-artifact-mirror-preflight"
  render_guarded_oneshot_service \
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.service" \
    "Qintopia AgentOS Huabaosi Feishu Artifact Mirror Worker" \
    "huabaosi-feishu-artifact-mirror-preflight" \
    "run-huabaosi-feishu-artifact-mirror-worker --once --apply"
  render_timer \
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer" \
    "Run Qintopia AgentOS Huabaosi Feishu artifact mirror worker" \
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.service" \
    "12min" \
    "${QINTOPIA_HUABAOSI_FEISHU_MIRROR_TIMER_INTERVAL:-5min}"

  render_oneshot_service \
    "qintopia-agentos-qiwe-image-send-preflight.service" \
    "Qintopia AgentOS QiWe Image Send Production Preflight" \
    "qiwe-image-send-production-preflight"
  render_guarded_oneshot_service \
    "qintopia-agentos-qiwe-image-send-worker.service" \
    "Qintopia AgentOS QiWe Image Send Worker" \
    "qiwe-image-send-production-preflight" \
    "run-qiwe-image-send-worker --once --apply"
  render_timer \
    "qintopia-agentos-qiwe-image-send-worker.timer" \
    "Run Qintopia AgentOS QiWe image send worker" \
    "qintopia-agentos-qiwe-image-send-worker.service" \
    "13min" \
    "${QINTOPIA_QIWE_IMAGE_SEND_TIMER_INTERVAL:-1min}"

  render_oneshot_service \
    "qintopia-agentos-xiaoman-activity-send-request-starter-worker.service" \
    "Qintopia AgentOS Xiaoman Activity Send Request Starter Worker" \
    "run-xiaoman-activity-send-request-starter-worker --once --apply"
  render_timer \
    "qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer" \
    "Run Qintopia AgentOS Xiaoman activity send request starter" \
    "qintopia-agentos-xiaoman-activity-send-request-starter-worker.service" \
    "10min" \
    "${QINTOPIA_XIAOMAN_ACTIVITY_SEND_REQUEST_STARTER_TIMER_INTERVAL:-2min}"
}

validate_output() {
  local required_files=(
    "_M9_SYSTEMD_PLAN.txt"
    "qintopia-message-sidecar.service"
    "qintopia-message-embedding-worker.service"
    "qintopia-message-identity-worker.service"
    "qintopia-agentos-member-profile-worker.service"
    "qintopia-agentos-graph-projection-worker.service"
    "qintopia-agentos-event-signal-worker.service"
    "qintopia-agentos-daily-digest-worker.service"
    "qintopia-agentos-daily-digest-publisher.service"
    "qintopia-agentos-raw-archive-worker.service"
    "qintopia-agentos-operations-workflow-sync.service"
    "qintopia-agentos-operations-workflow-sync.timer"
    "qintopia-agentos-operations-evidence-worker.service"
    "qintopia-agentos-operations-evidence-worker.timer"
    "qintopia-agentos-operations-visual-worker.service"
    "qintopia-agentos-operations-visual-worker.timer"
    "qintopia-agentos-operations-workbench-event.service"
    "qintopia-agentos-operations-workbench-event.timer"
    "qintopia-agentos-operations-group-send-ready.service"
    "qintopia-agentos-operations-group-send-ready.timer"
    "qintopia-agentos-xiaoman-activity-signal-worker.service"
    "qintopia-agentos-xiaoman-activity-signal-worker.timer"
    "qintopia-agentos-xiaoman-activity-promotion-starter-worker.service"
    "qintopia-agentos-xiaoman-activity-promotion-starter-worker.timer"
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.service"
    "qintopia-agentos-xiaoman-activity-image-generation-starter-worker.timer"
    "qintopia-agentos-huabaosi-image-generation-preflight.service"
    "qintopia-agentos-huabaosi-image-generation-worker.service"
    "qintopia-agentos-huabaosi-image-generation-worker.timer"
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-preflight.service"
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.service"
    "qintopia-agentos-huabaosi-feishu-artifact-mirror-worker.timer"
    "qintopia-agentos-qiwe-image-send-preflight.service"
    "qintopia-agentos-qiwe-image-send-worker.service"
    "qintopia-agentos-qiwe-image-send-worker.timer"
    "qintopia-agentos-xiaoman-activity-send-request-starter-worker.service"
    "qintopia-agentos-xiaoman-activity-send-request-starter-worker.timer"
  )

  local file
  for file in "${required_files[@]}"; do
    if [[ ! -f "${OUTPUT_DIR}/${file}" ]]; then
      echo "Missing rendered file: ${file}" >&2
      exit 4
    fi
  done

  for file in "$OUTPUT_DIR"/*.service; do
    grep -F "WorkingDirectory=${MONOREPO_DIR}" "$file" >/dev/null
    grep -F "Environment=QINTOPIA_SIDECAR_MIGRATIONS_DIR=${MIGRATIONS_DIR}" "$file" >/dev/null
    grep -F "ExecStart=${BIN}" "$file" >/dev/null
  done

  if grep -R -F "/home/ubuntu/qintopia-msg-sidecar" "$OUTPUT_DIR"/*.service >/dev/null; then
    echo "Rendered units still reference the legacy standalone checkout." >&2
    exit 4
  fi

  if grep -R "/target/release/qintopia-message-sidecar\\|cargo \\|git " "$OUTPUT_DIR"/*.service >/dev/null; then
    echo "Rendered units must use verified artifacts and must not build or fetch source." >&2
    exit 4
  fi
}

render_all

if [[ "$CHECK_ONLY" == "1" ]]; then
  validate_output
  echo "M9 systemd render check passed."
else
  echo "Rendered M9 systemd unit review files into ${OUTPUT_DIR}"
fi
