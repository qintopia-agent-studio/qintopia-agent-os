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

validate_release_tree() {
  local candidate="$1"
  local expected_uid
  expected_uid="$(id -u)"
  python3 - "$candidate" "$expected_uid" <<'PY'
import os
import stat
import sys

root = sys.argv[1]
expected_uid = int(sys.argv[2])

if not os.path.isdir(root) or os.path.islink(root):
    raise SystemExit("release tree must be a non-symlink directory")

paths = [root]
for directory, directories, files in os.walk(root, followlinks=False):
    paths.extend(os.path.join(directory, name) for name in directories)
    paths.extend(os.path.join(directory, name) for name in files)

for path in paths:
    metadata = os.lstat(path)
    relative = os.path.relpath(path, root)
    if metadata.st_uid != expected_uid:
        raise SystemExit(f"release tree owner mismatch: {relative}")
    is_link = stat.S_ISLNK(metadata.st_mode)
    is_directory = stat.S_ISDIR(metadata.st_mode)
    is_regular = stat.S_ISREG(metadata.st_mode)
    if not (is_link or is_directory or is_regular):
        raise SystemExit(f"release tree contains unsupported file type: {relative}")
    if not is_link and metadata.st_mode & (stat.S_IWGRP | stat.S_IWOTH):
        raise SystemExit(f"release tree path is group/world writable: {relative}")
    required_directory_access = (
        stat.S_IRGRP | stat.S_IXGRP | stat.S_IROTH | stat.S_IXOTH
    )
    if (
        is_directory
        and metadata.st_mode & required_directory_access != required_directory_access
    ):
        raise SystemExit(f"release tree directory is not group/world accessible: {relative}")

required = {
    "sidecar/qintopia-message-sidecar": 0o755,
    "sidecar/artifact-manifest.json": 0o444,
    "sidecar/SHA256SUMS": 0o444,
}
for relative, expected_mode in required.items():
    path = os.path.join(root, relative)
    try:
        metadata = os.lstat(path)
    except FileNotFoundError:
        raise SystemExit(f"release tree required file is missing: {relative}") from None
    if not stat.S_ISREG(metadata.st_mode):
        raise SystemExit(f"release tree required path is not a regular file: {relative}")
    actual_mode = stat.S_IMODE(metadata.st_mode)
    if actual_mode != expected_mode:
        raise SystemExit(
            f"release tree mode mismatch: {relative} expected {expected_mode:04o} got {actual_mode:04o}"
        )
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
validate_release_tree "$staging_dir"

if [[ "$dry_run" == "true" ]]; then
  echo "Dry run assembled release at ${staging_dir}"
  exit 0
fi

if [[ -e "$release_dir" ]]; then
  echo "release already exists: ${release_dir}; verifying manifest"
  python3 - "$release_dir/manifest.json" "$staging_dir/manifest.json" <<'PY'
import json
import sys

existing_path, requested_path = sys.argv[1:3]
with open(existing_path, encoding="utf-8") as fh:
    manifest = json.load(fh)
with open(requested_path, encoding="utf-8") as fh:
    requested = json.load(fh)

keys = (
    "release_sha",
    "runtime_sha",
    "deploy_bundle_sha",
    "commit_sha",
    "release_scope",
    "restart_targets",
)
for key in keys:
    if manifest.get(key) != requested.get(key):
        raise SystemExit(f"existing release manifest {key} mismatch")
PY
  validate_release_tree "$release_dir"
  rm -rf "$staging_dir"
else
  mv "$staging_dir" "$release_dir"
fi

release_target="$(readlink -f "$release_dir")"
if [[ -n "$current_target" && "$current_target" != "$release_target" ]]; then
  ln -sfn "$current_target" "${release_root}/previous"
fi
ln -sfn "$release_dir" "${release_root}/current"

echo "Promoted ${release_sha} for request ${request_id}"
