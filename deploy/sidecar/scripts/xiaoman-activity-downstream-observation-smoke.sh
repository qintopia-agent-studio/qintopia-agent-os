#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE:-}" != "1" ]]; then
  echo "xiaoman activity downstream observation skipped: set QINTOPIA_XIAOMAN_ACTIVITY_DOWNSTREAM_OBSERVATION_ENABLE=1 to inspect evidence/visual worker previews" >&2
  exit 0
fi

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
MONOREPO_ROOT="$(cd "${SCRIPT_DIR}/../../.." && pwd)"
SIDECAR_DIR="${QINTOPIA_SIDECAR_SOURCE_DIR:-${MONOREPO_ROOT}/runtime/sidecar}"
ENV_FILE="${QINTOPIA_SIDECAR_ENV_FILE:-/etc/qintopia/message-sidecar.env}"

cd "$MONOREPO_ROOT"

if [[ -n "${QINTOPIA_SIDECAR_BIN:-}" ]]; then
  BIN_CMD=("$QINTOPIA_SIDECAR_BIN")
else
  BIN_CMD=("${CARGO:-cargo}" run --quiet --manifest-path "$SIDECAR_DIR/Cargo.toml" --)
fi

source_env_if_present() {
  if [[ -f "$ENV_FILE" ]]; then
    set -a
    # shellcheck disable=SC1090
    source "$ENV_FILE"
    set +a
  fi
}

assert_no_sensitive_output() {
  local label="$1"
  local file="$2"
  local forbidden=(
    "tenant_access_token"
    "QINTOPIA_SIDECAR_DATABASE_URL=postgres://"
    "--use-feishu-base"
    "send_executed=true"
    "message_id"
    "raw_chat"
    "base_token"
  )

  local value_name
  for value_name in \
    QINTOPIA_SIDECAR_DATABASE_URL \
    QINTOPIA_XIAOMAN_ACTIVITY_FEISHU_BASE_TOKEN \
    QINTOPIA_DAILY_DIGEST_FEISHU_BASE_TOKEN \
    QIWE_TOKEN \
    QIWE_GUID; do
    if [[ -n "${!value_name:-}" ]]; then
      forbidden+=("${!value_name}")
    fi
  done

  local token
  for token in "${forbidden[@]}"; do
    if [[ -n "$token" ]] && grep -Fq -- "$token" "$file"; then
      echo "${label} contains forbidden sensitive output" >&2
      exit 1
    fi
  done
}

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT

source_env_if_present

evidence_check="$tmp_dir/evidence-worker-check.json"
"${BIN_CMD[@]}" run-evidence-worker --once --dry-run >"$evidence_check"
python3 - "$evidence_check" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

assert payload["success"] is True
assert payload["worker"] == "evidence-worker"
assert payload["dry_run"] is True
assert payload["apply_requested"] is False
assert payload["fixture_mode"] is False
assert payload["action_status"] in {
    "dry_run_ok",
    "no_claimable_evidence_request",
}
assert isinstance(payload["artifact_ids"], list)
assert isinstance(payload["artifact_previews"], list)
assert isinstance(payload["limitations"], list)
assert isinstance(payload["guardrails"], list)
joined = "\n".join(payload["limitations"] + payload["guardrails"])
assert "external" in joined.lower() or "外部" in joined
PY
assert_no_sensitive_output "evidence worker check" "$evidence_check"

visual_check="$tmp_dir/visual-worker-check.json"
"${BIN_CMD[@]}" run-collaboration-worker --work-item-type visual_asset_request --once --dry-run >"$visual_check"
python3 - "$visual_check" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as fh:
    payload = json.load(fh)

assert payload["success"] is True
assert payload["worker"] == "collaboration-worker"
assert payload["dry_run"] is True
assert payload["apply_requested"] is False
assert payload["fixture_mode"] is False
assert payload["action_status"] in {
    "dry_run_ok",
    "no_claimable_work_item",
}
assert isinstance(payload["artifact_ids"], list)
assert isinstance(payload["artifact_previews"], list)
assert isinstance(payload["limitations"], list)
assert isinstance(payload["guardrails"], list)
joined = "\n".join(payload["limitations"] + payload["guardrails"])
assert "external" in joined.lower() or "外部" in joined
PY
assert_no_sensitive_output "visual worker check" "$visual_check"

echo "xiaoman activity downstream observation passed"
