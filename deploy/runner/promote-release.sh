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
    "sidecar/qintopia-message-sidecar.tar.gz": 0o444,
    "deploy-bundle/artifact-manifest.json": 0o444,
    "deploy-bundle/SHA256SUMS": 0o444,
    "deploy-bundle/qintopia-agent-os-deploy-bundle.tar.gz": 0o444,
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
runtime_artifact_profile="$(json_get runtime_artifact_profile)"
deploy_bundle_sha="$(json_get deploy_bundle_sha)"
request_id="$(json_get request_id)"
release_dir="${release_root}/${release_sha}"
staging_dir="${release_root}/.staging-${release_sha}"
current_target="$(readlink -f "${release_root}/current" 2>/dev/null || true)"
adopted_manifest_tmp=""
profile_switch_backup_dir=""
profile_switch_active=false

cleanup() {
  if [[ "$profile_switch_active" == "true" ]]; then
    rm -rf "${release_dir}/sidecar" 2>/dev/null || true
    if [[ -n "$profile_switch_backup_dir" && -d "$profile_switch_backup_dir/sidecar" ]]; then
      mv "$profile_switch_backup_dir/sidecar" "${release_dir}/sidecar" 2>/dev/null || true
    fi
    if [[ -n "$profile_switch_backup_dir" && -f "$profile_switch_backup_dir/manifest.json" ]]; then
      cp -a "$profile_switch_backup_dir/manifest.json" "${release_dir}/manifest.json" 2>/dev/null || true
    fi
  fi
  if [[ -n "$adopted_manifest_tmp" && -f "$adopted_manifest_tmp" ]]; then
    rm -f "$adopted_manifest_tmp"
  fi
  if [[ -n "$profile_switch_backup_dir" ]]; then
    rm -rf "$profile_switch_backup_dir"
  fi
}
trap cleanup EXIT

build_adopted_existing_manifest() {
  local existing_dir="$1"
  local output_path="$2"
  python3 - "$existing_dir/manifest.json" "$existing_dir/sidecar/artifact-manifest.json" "$output_path" <<'PY'
import json
import sys

manifest_path, sidecar_manifest_path, output_path = sys.argv[1:4]
with open(manifest_path, encoding="utf-8") as fh:
    manifest = json.load(fh)

if not manifest.get("runtime_artifact_profile"):
    with open(sidecar_manifest_path, encoding="utf-8") as fh:
        sidecar_manifest = json.load(fh)

    artifact_profile = sidecar_manifest.get("validation", {}).get("artifact_profile")
    if artifact_profile not in {"huabaosi-production", "qiwe-production"}:
        raise SystemExit("existing release sidecar artifact manifest profile is unavailable")
    manifest["runtime_artifact_profile"] = artifact_profile

with open(output_path, "w", encoding="utf-8") as fh:
    json.dump(manifest, fh, ensure_ascii=False, indent=2)
    fh.write("\n")
PY
}

repair_existing_release_metadata() {
  local existing_dir="$1"
  local requested_dir="$2"

  if [[ ! -d "$existing_dir" || -L "$existing_dir" ]]; then
    echo "existing release path must be a non-symlink directory: ${existing_dir}" >&2
    return 1
  fi
  if [[ ! -f "${existing_dir}/manifest.json" || -L "${existing_dir}/manifest.json" ]]; then
    echo "existing release manifest must be a non-symlink regular file" >&2
    return 1
  fi

  python3 - "$existing_dir" "$requested_dir" <<'PY'
import hashlib
import os
import stat
import sys

existing_root, requested_root = sys.argv[1:3]


def digest(path):
    value = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def inventory(root):
    entries = {}
    for directory, dirnames, filenames in os.walk(root, topdown=True, followlinks=False):
        relative_directory = os.path.relpath(directory, root)
        if relative_directory == ".":
            relative_directory = ""

        retained_directories = []
        for name in sorted(dirnames):
            path = os.path.join(directory, name)
            relative = os.path.join(relative_directory, name)
            metadata = os.lstat(path)
            if stat.S_ISLNK(metadata.st_mode):
                entries[relative] = ("symlink", os.readlink(path))
            elif stat.S_ISDIR(metadata.st_mode):
                entries[relative] = ("directory",)
                retained_directories.append(name)
            else:
                raise SystemExit(f"release tree contains unsupported path type: {relative}")
        dirnames[:] = retained_directories

        for name in sorted(filenames):
            path = os.path.join(directory, name)
            relative = os.path.join(relative_directory, name)
            metadata = os.lstat(path)
            if stat.S_ISLNK(metadata.st_mode):
                entries[relative] = ("symlink", os.readlink(path))
            elif stat.S_ISREG(metadata.st_mode):
                if relative == "manifest.json":
                    entries[relative] = ("request-manifest",)
                else:
                    entries[relative] = ("file", digest(path))
            else:
                raise SystemExit(f"release tree contains unsupported path type: {relative}")
    return entries


existing = inventory(existing_root)
requested = inventory(requested_root)
if existing != requested:
    missing = sorted(set(requested) - set(existing))
    extra = sorted(set(existing) - set(requested))
    changed = sorted(
        path for path in set(existing) & set(requested) if existing[path] != requested[path]
    )
    details = []
    if missing:
        details.append(f"missing={','.join(missing[:5])}")
    if extra:
        details.append(f"extra={','.join(extra[:5])}")
    if changed:
        details.append(f"changed={','.join(changed[:5])}")
    raise SystemExit(
        "existing release content differs from freshly verified artifacts"
        + (f": {'; '.join(details)}" if details else "")
    )
PY

  (
    cd "${existing_dir}/sidecar"
    sha256sum -c SHA256SUMS
  )
  (
    cd "${existing_dir}/deploy-bundle"
    sha256sum -c SHA256SUMS
  )

  chown -hR root:root "$existing_dir"
  python3 - "$existing_dir" "$requested_dir" <<'PY'
import os
import stat
import sys

existing_root, requested_root = sys.argv[1:3]
directories = []

for directory, dirnames, filenames in os.walk(requested_root, topdown=True, followlinks=False):
    relative_directory = os.path.relpath(directory, requested_root)
    if relative_directory == ".":
        relative_directory = ""
    directories.append((relative_directory, stat.S_IMODE(os.lstat(directory).st_mode)))

    retained_directories = []
    for name in dirnames:
        path = os.path.join(directory, name)
        if not stat.S_ISLNK(os.lstat(path).st_mode):
            retained_directories.append(name)
    dirnames[:] = retained_directories

    for name in filenames:
        source = os.path.join(directory, name)
        metadata = os.lstat(source)
        if stat.S_ISREG(metadata.st_mode):
            relative = os.path.join(relative_directory, name)
            os.chmod(
                os.path.join(existing_root, relative),
                stat.S_IMODE(metadata.st_mode),
                follow_symlinks=False,
            )

for relative, mode in sorted(directories, key=lambda item: item[0].count(os.sep), reverse=True):
    target = existing_root if not relative else os.path.join(existing_root, relative)
    os.chmod(target, mode, follow_symlinks=False)
PY
}

install -d -m 0755 "$release_root"
rm -rf "$staging_dir"
install -d -m 0755 \
  "$staging_dir" \
  "$staging_dir/sidecar" \
  "$staging_dir/deploy-bundle"

QINTOPIA_SIDECAR_ARTIFACT_PROFILE="$runtime_artifact_profile" \
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
    "runtime_artifact_profile": request["runtime_artifact_profile"],
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
  if [[ ! -d "$release_dir" || -L "$release_dir" ]]; then
    echo "existing release path must be a non-symlink directory: ${release_dir}" >&2
    exit 1
  fi
  if [[ ! -f "${release_dir}/manifest.json" || -L "${release_dir}/manifest.json" ]]; then
    echo "existing release manifest must be a non-symlink regular file" >&2
    exit 1
  fi
  adopted_manifest_tmp="$(mktemp "${release_root}/.existing-manifest-${release_sha}.XXXXXX.json")"
  build_adopted_existing_manifest "$release_dir" "$adopted_manifest_tmp"
  profile_mode="$(python3 - "$adopted_manifest_tmp" "$staging_dir/manifest.json" <<'PY'
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
existing_profile = manifest.get("runtime_artifact_profile")
requested_profile = requested.get("runtime_artifact_profile")
if existing_profile == requested_profile:
    print("same")
elif {existing_profile, requested_profile} == {
    "huabaosi-production",
    "qiwe-production",
}:
    print("profile-switch")
else:
    raise SystemExit("existing release manifest runtime_artifact_profile mismatch")
PY
  )"
  if [[ "$profile_mode" == "profile-switch" ]]; then
    python3 - "$release_dir" "$staging_dir" <<'PY'
import hashlib
import os
import stat
import sys

existing_root, requested_root = sys.argv[1:3]

def inventory(root):
    entries = {}
    for directory, dirnames, filenames in os.walk(root, topdown=True, followlinks=False):
        relative_directory = os.path.relpath(directory, root)
        if relative_directory == ".":
            relative_directory = ""
        if relative_directory == "sidecar" or relative_directory.startswith("sidecar/"):
            dirnames[:] = []
            continue

        retained_directories = []
        for name in sorted(dirnames):
            relative = os.path.join(relative_directory, name)
            if relative == "sidecar":
                continue
            path = os.path.join(directory, name)
            metadata = os.lstat(path)
            if stat.S_ISLNK(metadata.st_mode):
                entries[relative] = ("symlink", os.readlink(path))
            elif stat.S_ISDIR(metadata.st_mode):
                entries[relative] = ("directory",)
                retained_directories.append(name)
            else:
                raise SystemExit(f"unsupported existing path: {relative}")
        dirnames[:] = retained_directories

        for name in sorted(filenames):
            relative = os.path.join(relative_directory, name)
            if relative == "manifest.json":
                continue
            path = os.path.join(directory, name)
            metadata = os.lstat(path)
            if stat.S_ISLNK(metadata.st_mode):
                entries[relative] = ("symlink", os.readlink(path))
            elif stat.S_ISREG(metadata.st_mode):
                digest = hashlib.sha256()
                with open(path, "rb") as handle:
                    for chunk in iter(lambda: handle.read(1024 * 1024), b""):
                        digest.update(chunk)
                entries[relative] = ("file", digest.hexdigest())
            else:
                raise SystemExit(f"unsupported existing path: {relative}")
    return entries

if inventory(existing_root) != inventory(requested_root):
    raise SystemExit(
        "existing release content outside sidecar differs from freshly verified artifacts"
    )
PY
    profile_switch_backup_dir="$(mktemp -d "${release_root}/.profile-switch-${release_sha}.XXXXXX")"
    mv "${release_dir}/sidecar" "$profile_switch_backup_dir/sidecar"
    profile_switch_active=true
    cp -a "${release_dir}/manifest.json" "$profile_switch_backup_dir/manifest.json"
    mv "${staging_dir}/sidecar" "${release_dir}/sidecar"
    mkdir -p "${staging_dir}/sidecar"
    cp -a "${release_dir}/sidecar/." "${staging_dir}/sidecar/"
    python3 - "$adopted_manifest_tmp" "$staging_dir/manifest.json" <<'PY'
import json
import sys

adopted_path, requested_path = sys.argv[1:3]
with open(requested_path, encoding="utf-8") as fh:
    requested = json.load(fh)
with open(adopted_path, encoding="utf-8") as fh:
    adopted = json.load(fh)
adopted["runtime_artifact_profile"] = requested["runtime_artifact_profile"]
with open(adopted_path, "w", encoding="utf-8") as fh:
    json.dump(adopted, fh, ensure_ascii=False, indent=2)
    fh.write("\n")
PY
  fi
  repair_existing_release_metadata "$release_dir" "$staging_dir"
  mv "$adopted_manifest_tmp" "${release_dir}/manifest.json"
  adopted_manifest_tmp=""
  validate_release_tree "$release_dir"
  rm -rf "$staging_dir"
  if [[ "$profile_switch_active" == "true" ]]; then
    rm -rf "$profile_switch_backup_dir"
    profile_switch_backup_dir=""
    profile_switch_active=false
  fi
else
  mv "$staging_dir" "$release_dir"
fi

release_target="$(readlink -f "$release_dir")"
if [[ -n "$current_target" && "$current_target" != "$release_target" ]]; then
  ln -sfn "$current_target" "${release_root}/previous"
fi
ln -sfn "$release_dir" "${release_root}/current"

echo "Promoted ${release_sha} for request ${request_id}"
