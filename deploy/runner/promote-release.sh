#!/usr/bin/env bash
set -euo pipefail

usage() {
  cat <<'USAGE'
Usage:
  deploy/runner/promote-release.sh --request-file <file> --release-root <dir> [--dry-run]
USAGE
}

request_file=""
release_root=""
dry_run=false
while [[ $# -gt 0 ]]; do
  case "$1" in
    --request-file)
      request_file="${2:-}"
      shift 2
      ;;
    --release-root)
      release_root="${2:-}"
      shift 2
      ;;
    --dry-run)
      dry_run=true
      shift
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

if [[ -z "$request_file" || -z "$release_root" ]]; then
  usage >&2
  exit 2
fi

json_get() {
  python3 - "$request_file" "$1" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    data = json.load(fh)
value = data
for part in sys.argv[2].split("."):
    value = value[part]
if isinstance(value, list):
    print(",".join(value))
else:
    print(value)
PY
}

release_sha="$(json_get release_sha)"
runtime_sha="$(json_get runtime_sha)"
deploy_bundle_sha="$(json_get deploy_bundle_sha)"
request_id="$(json_get request_id)"
release_dir="${release_root}/${release_sha}"
staging_dir="${release_root}/.staging-${release_sha}"
current_target="$(readlink -f "${release_root}/current" 2>/dev/null || true)"

mkdir -p "$release_root"
rm -rf "$staging_dir"
mkdir -p "$staging_dir"

deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --artifact-type sidecar \
  --sha "$runtime_sha" \
  --output-dir "${staging_dir}/sidecar"

deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --artifact-type deploy-bundle \
  --sha "$deploy_bundle_sha" \
  --output-dir "${staging_dir}/deploy-bundle"

cp -a "${staging_dir}/deploy-bundle/payload/." "$staging_dir/"

python3 - "$request_file" "$staging_dir/manifest.json" "$current_target" <<'PY'
import json
import sys
from datetime import datetime, timezone

request_path, manifest_path, previous = sys.argv[1:4]
with open(request_path, encoding="utf-8") as fh:
    request = json.load(fh)

manifest = {
    "schema_version": 2,
    "release_sha": request["release_sha"],
    "runtime_sha": request["runtime_sha"],
    "deploy_bundle_sha": request["deploy_bundle_sha"],
    "commit_sha": request["commit_sha"],
    "previous_sha": previous.rsplit("/", 1)[-1] if previous else "",
    "assembled_at": datetime.now(timezone.utc).isoformat(),
    "request_id": request["request_id"],
    "release_scope": request["release_scope"],
    "restart_targets": request["restart_targets"],
    "dry_run": request["dry_run"],
}
with open(manifest_path, "w", encoding="utf-8") as fh:
    json.dump(manifest, fh, ensure_ascii=False, indent=2)
    fh.write("\n")
PY

test -x "${staging_dir}/sidecar/qintopia-message-sidecar"
test -f "${staging_dir}/manifest.json"
test -d "${staging_dir}/deploy"

if [[ "$dry_run" == "true" ]]; then
  echo "Dry run assembled release at ${staging_dir}"
  exit 0
fi

if [[ -e "$release_dir" ]]; then
  echo "release already exists: ${release_dir}; verifying manifest"
  python3 - "$release_dir/manifest.json" "$release_sha" <<'PY'
import json
import sys
with open(sys.argv[1], encoding="utf-8") as fh:
    manifest = json.load(fh)
if manifest.get("release_sha") != sys.argv[2]:
    raise SystemExit("existing release manifest release_sha mismatch")
PY
  rm -rf "$staging_dir"
else
  mv "$staging_dir" "$release_dir"
fi

if [[ -n "$current_target" ]]; then
  ln -sfn "$current_target" "${release_root}/previous"
fi
ln -sfn "$release_dir" "${release_root}/current"

echo "Promoted ${release_sha} for request ${request_id}"
