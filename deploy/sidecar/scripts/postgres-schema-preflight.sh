#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  deploy/sidecar/scripts/postgres-schema-preflight.sh [--database-url-env <name>]

Read-only Postgres schema gate for M9 sidecar cutover.

Required:
  QINTOPIA_SIDECAR_DATABASE_URL, or another env var named by --database-url-env.

This script checks schema objects and schema_change_log versions required by the
monorepo sidecar service family. It does not run migrations or read business rows.
Do not pass connection strings as command arguments; they can appear in process lists.
USAGE
}

database_url_env="QINTOPIA_SIDECAR_DATABASE_URL"

while [[ $# -gt 0 ]]; do
  case "$1" in
    --database-url-env)
      database_url_env="${2:-}"
      shift 2
      ;;
    -h | --help)
      usage
      exit 0
      ;;
    *)
      echo "unknown argument: $1" >&2
      usage >&2
      exit 2
      ;;
  esac
done

database_url="${!database_url_env:-}"

if [[ -z "$database_url" ]]; then
  echo "${database_url_env} is required" >&2
  exit 2
fi

if ! command -v psql >/dev/null 2>&1; then
  echo "psql is required for Postgres schema preflight" >&2
  exit 2
fi

if ! command -v python3 >/dev/null 2>&1; then
  echo "python3 is required for Postgres URL parsing" >&2
  exit 2
fi

tmp_dir="$(mktemp -d)"
trap 'rm -rf "$tmp_dir"' EXIT
chmod 700 "$tmp_dir"

pg_env_file="${tmp_dir}/pg.env"
DATABASE_URL="$database_url" python3 - "$pg_env_file" <<'PY'
import os
import shlex
import sys
from urllib.parse import unquote, urlparse

target = sys.argv[1]
url = os.environ["DATABASE_URL"]
parsed = urlparse(url)

if parsed.scheme not in {"postgres", "postgresql"}:
    raise SystemExit("database URL must use postgres:// or postgresql://")
if not parsed.hostname:
    raise SystemExit("database URL must include a host")
if not parsed.path or parsed.path == "/":
    raise SystemExit("database URL must include a database name")

values = {
    "PGHOST": parsed.hostname,
    "PGDATABASE": unquote(parsed.path.lstrip("/")),
}
if parsed.port:
    values["PGPORT"] = str(parsed.port)
if parsed.username is not None:
    values["PGUSER"] = unquote(parsed.username)
if parsed.password is not None:
    values["PGPASSWORD"] = unquote(parsed.password)

with open(target, "w", encoding="utf-8") as fh:
    for key, value in values.items():
        fh.write(f"export {key}={shlex.quote(value)}\n")
PY
chmod 600 "$pg_env_file"
# shellcheck disable=SC1090
. "$pg_env_file"
unset database_url

PSQL_CMD=(psql -X -q -t -A -v ON_ERROR_STOP=1)
failures=()

psql_value() {
  PGCONNECT_TIMEOUT="${PGCONNECT_TIMEOUT:-10}" "${PSQL_CMD[@]}" -c "$1" \
    2>"${tmp_dir}/psql.stderr" | sed '/^$/d'
}

check_query() {
  local label="$1"
  local query="$2"
  local expected="${3:-t}"
  local actual
  set +e
  actual="$(psql_value "$query")"
  local status=$?
  set -e
  if [[ "$status" -ne 0 ]]; then
    failures+=("${label}: query failed: $(tail -n 1 "${tmp_dir}/psql.stderr")")
    return
  fi
  if [[ "$actual" != "$expected" ]]; then
    failures+=("$label")
  fi
}

check_count_at_least() {
  local label="$1"
  local query="$2"
  local minimum="$3"
  local actual
  set +e
  actual="$(psql_value "$query")"
  local status=$?
  set -e
  if [[ "$status" -ne 0 ]]; then
    failures+=("${label}: query failed: $(tail -n 1 "${tmp_dir}/psql.stderr")")
    return
  fi
  if ! [[ "$actual" =~ ^[0-9]+$ ]] || (( actual < minimum )); then
    failures+=("${label}: expected >= ${minimum}, got ${actual:-empty}")
  fi
}

required_schemas=(
  qintopia_messages
  qintopia_knowledge
  qintopia_identity
  qintopia_graph
  qintopia_agent_os
)

required_tables=(
  qintopia_messages.raw_events
  qintopia_messages.messages
  qintopia_messages.message_mentions
  qintopia_messages.message_embeddings
  qintopia_messages.message_processing_jobs
  qintopia_messages.dead_letter_events
  qintopia_messages.entities
  qintopia_messages.message_entities
  qintopia_messages.entity_edges
  qintopia_messages.conversations
  qintopia_knowledge.knowledge_sources
  qintopia_knowledge.knowledge_documents
  qintopia_knowledge.knowledge_chunks
  qintopia_knowledge.knowledge_embeddings
  qintopia_knowledge.knowledge_sync_jobs
  qintopia_knowledge.knowledge_access_audit
  qintopia_identity.persons
  qintopia_identity.person_aliases
  qintopia_identity.channel_identities
  qintopia_identity.person_memberships
  qintopia_identity.member_facts
  qintopia_identity.person_interaction_summaries
  qintopia_identity.member_profile_snapshots
  qintopia_identity.member_context_audit
  qintopia_identity.channel_identity_observations
  qintopia_identity.erhua_training_notes
  qintopia_identity.erhua_persona_overlays
  qintopia_graph.graph_entities
  qintopia_graph.graph_entity_observations
  qintopia_graph.graph_edges
  qintopia_graph.graph_projections
  qintopia_agent_os.schema_change_log
  qintopia_agent_os.embedding_models
  qintopia_agent_os.agent_context_requests
  qintopia_agent_os.agent_context_results
  qintopia_agent_os.tool_invocation_audit
  qintopia_agent_os.daily_digests
  qintopia_agent_os.daily_digest_publish_audit
  qintopia_agent_os.raw_message_archives
  qintopia_agent_os.event_signal_candidates
  qintopia_agent_os.event_signals
  qintopia_agent_os.event_signal_mutations
  qintopia_agent_os.capabilities
  qintopia_agent_os.work_items
  qintopia_agent_os.artifacts
  qintopia_agent_os.work_item_events
  qintopia_agent_os.human_workbench_refs
)

required_columns=(
  "qintopia_agent_os.event_signals|gap_summary"
  "qintopia_agent_os.event_signals|activity_phase"
)

required_functions=(
  "qintopia_identity.identity_source_rank(text)"
  "qintopia_agent_os.is_human_actor_id(text)"
)

required_versions=(
  "2026-06-18.001|202606180001_init.sql"
  "2026-06-24.002|202606240002_agent_os_data_layer.sql"
  "2026-06-26.003|202606260003_identity_observations.sql"
  "2026-06-26.004|202606260004_profile_digest_archive_v1.sql"
  "2026-06-27.005|202606270005_event_signals_v2.sql"
  "2026-06-29.006|202606290006_erhua_training_memory.sql"
  "2026-06-30.007|202606300007_operations_control_plane.sql"
  "2026-07-02.001|202607020001_operations_human_actor_guards.sql"
  "2026-07-13.002|202607130002_huabaosi_image_generation.sql"
  "2026-07-14.001|202607140001_xiaoman_event_signal_mutations.sql"
  "2026-07-15.001|202607150001_xiaoman_activity_phases.sql"
)

required_capabilities=(
  huabaosi.create_visual_asset
  huabaosi.generate_image_asset
  erhua.send_group_message
  wenyuange.retrieve_evidence
  xiaoman.create_activity_request
)

for schema in "${required_schemas[@]}"; do
  check_query "missing_schema|${schema}" "SELECT to_regnamespace('${schema}') IS NOT NULL;"
done

for table in "${required_tables[@]}"; do
  check_query "missing_table|${table}" "SELECT to_regclass('${table}') IS NOT NULL;"
done

for column_record in "${required_columns[@]}"; do
  table_name="${column_record%%|*}"
  column_name="${column_record#*|}"
  schema_name="${table_name%%.*}"
  relation_name="${table_name#*.}"
  check_query \
    "missing_column|${table_name}|${column_name}" \
    "SELECT EXISTS (SELECT 1 FROM information_schema.columns WHERE table_schema = '${schema_name}' AND table_name = '${relation_name}' AND column_name = '${column_name}');"
done

schema_change_log_exists="$(psql_value "SELECT to_regclass('qintopia_agent_os.schema_change_log') IS NOT NULL;" || true)"
if [[ "$schema_change_log_exists" == "t" ]]; then
  for version_record in "${required_versions[@]}"; do
    version="${version_record%%|*}"
    migration="${version_record#*|}"
    check_query \
      "missing_or_non_applied_schema_version|${version}|${migration}" \
      "SELECT EXISTS (SELECT 1 FROM qintopia_agent_os.schema_change_log WHERE schema_version = '${version}' AND migration_name = '${migration}' AND status = 'applied');"
  done
fi

for function_name in "${required_functions[@]}"; do
  check_query \
    "missing_function|${function_name}" \
    "SELECT to_regprocedure('${function_name}') IS NOT NULL;"
done

capabilities_table_exists="$(psql_value "SELECT to_regclass('qintopia_agent_os.capabilities') IS NOT NULL;" || true)"
if [[ "$capabilities_table_exists" == "t" ]]; then
  for capability_key in "${required_capabilities[@]}"; do
    check_query \
      "missing_capability|${capability_key}" \
      "SELECT EXISTS (SELECT 1 FROM qintopia_agent_os.capabilities WHERE capability_key = '${capability_key}' AND enabled IS TRUE);"
  done
  check_count_at_least \
    "capability_seed_count" \
    "SELECT count(*) FROM qintopia_agent_os.capabilities WHERE capability_key IN ('huabaosi.create_visual_asset','huabaosi.generate_image_asset','erhua.send_group_message','wenyuange.retrieve_evidence','xiaoman.create_activity_request') AND enabled IS TRUE;" \
    5
fi

if (( ${#failures[@]} > 0 )); then
  echo "Postgres schema preflight failed:"
  for failure in "${failures[@]}"; do
    echo "- ${failure}"
  done
  exit 1
fi

echo "Postgres schema preflight passed."
