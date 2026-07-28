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
    "manifest.json": 0o444,
    "sidecar/qintopia-message-sidecar": 0o755,
    "sidecar/artifact-manifest.json": 0o444,
    "sidecar/SHA256SUMS": 0o444,
    "sidecar/qintopia-message-sidecar.tar.gz": 0o444,
    "sidecar-profiles/qiwe-production/qintopia-message-sidecar": 0o755,
    "sidecar-profiles/qiwe-production/artifact-manifest.json": 0o444,
    "sidecar-profiles/qiwe-production/SHA256SUMS": 0o444,
    "sidecar-profiles/qiwe-production/qintopia-message-sidecar.tar.gz": 0o444,
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
if [[ "$runtime_artifact_profile" != "huabaosi-production" ]]; then
  echo "release promotion requires runtime_artifact_profile=huabaosi-production; QiWe is installed as a companion runtime" >&2
  exit 1
fi

companion_runtime_artifact_profile="qiwe-production"
companion_relative_dir="sidecar-profiles/${companion_runtime_artifact_profile}"
release_dir="${release_root}/${release_sha}"
staging_dir="${release_root}/.staging-${release_sha}"
current_target="$(readlink -f "${release_root}/current" 2>/dev/null || true)"
quarantine_root="${QINTOPIA_DEPLOY_RUNNER_QUARANTINE_ROOT:-${QINTOPIA_DEPLOY_RUNNER_STATE_DIR:-/var/lib/qintopia-agent-os-deploy}/quarantine}"
adopted_manifest_tmp=""
existing_manifest_backup_tmp=""
quarantine_container=""
quarantined_coscli_source=""
quarantined_coscli_target=""
quarantine_active=false
companion_install_active=false
companion_parent_created=false
promotion_complete=false

cleanup() {
  if [[ "$promotion_complete" != "true" && "$companion_install_active" == "true" ]]; then
    rm -rf "${release_dir:?}/${companion_relative_dir}" 2>/dev/null || true
    if [[ "$companion_parent_created" == "true" ]]; then
      rmdir "${release_dir}/sidecar-profiles" 2>/dev/null || true
    fi
  fi
  if [[ "$promotion_complete" != "true" && -n "$existing_manifest_backup_tmp" && -f "$existing_manifest_backup_tmp" ]]; then
    if mv "$existing_manifest_backup_tmp" "${release_dir}/manifest.json" 2>/dev/null; then
      existing_manifest_backup_tmp=""
    else
      echo "failed to restore existing release manifest from ${existing_manifest_backup_tmp}" >&2
    fi
  fi
  if [[ "$promotion_complete" != "true" && "$quarantine_active" == "true" ]]; then
    if [[ -d "$quarantined_coscli_target" && ! -e "$quarantined_coscli_source" ]]; then
      mv "$quarantined_coscli_target" "$quarantined_coscli_source" 2>/dev/null || true
    fi
  fi
  if [[ -n "$adopted_manifest_tmp" && -f "$adopted_manifest_tmp" ]]; then
    rm -f "$adopted_manifest_tmp"
  fi
  if [[ -n "$quarantine_container" && -d "$quarantine_container" ]]; then
    rmdir "$quarantine_container" 2>/dev/null || true
  fi
  if [[ -d "$staging_dir" && ! -L "$staging_dir" ]]; then
    rm -rf "${staging_dir:?}"
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
    if artifact_profile != "huabaosi-production":
        raise SystemExit("existing release sidecar artifact manifest profile is unavailable")
    manifest["runtime_artifact_profile"] = artifact_profile

if manifest.get("runtime_artifact_profile") != "huabaosi-production":
    raise SystemExit("existing release primary runtime profile is not huabaosi-production")
manifest["companion_runtime_artifact_profiles"] = ["qiwe-production"]

with open(output_path, "w", encoding="utf-8") as fh:
    json.dump(manifest, fh, ensure_ascii=False, indent=2)
    fh.write("\n")
PY
}

validate_existing_coscli_output() {
  local diagnostic_root="$1"
  local expected_uid
  expected_uid="$(id -u)"
  python3 - "$diagnostic_root" "$expected_uid" <<'PY'
import os
import re
import stat
import sys

root = sys.argv[1]
expected_uid = int(sys.argv[2])
timestamp = re.compile(r"^[0-9]{8}_[0-9]{6}$")

try:
    root_metadata = os.lstat(root)
except FileNotFoundError:
    raise SystemExit("existing COSCLI diagnostic root is missing") from None
if (
    not stat.S_ISDIR(root_metadata.st_mode)
    or stat.S_ISLNK(root_metadata.st_mode)
    or root_metadata.st_uid != expected_uid
    or stat.S_IMODE(root_metadata.st_mode) != 0o755
):
    raise SystemExit("existing COSCLI diagnostic root metadata is invalid")

entries = sorted(os.listdir(root))
if not entries or len(entries) > 32:
    raise SystemExit("existing COSCLI diagnostic entry count is invalid")

for name in entries:
    if not timestamp.fullmatch(name):
        raise SystemExit("existing COSCLI diagnostic directory name is invalid")
    directory = os.path.join(root, name)
    metadata = os.lstat(directory)
    if (
        not stat.S_ISDIR(metadata.st_mode)
        or stat.S_ISLNK(metadata.st_mode)
        or metadata.st_uid != expected_uid
        or stat.S_IMODE(metadata.st_mode) != 0o755
    ):
        raise SystemExit("existing COSCLI diagnostic directory metadata is invalid")
    if os.listdir(directory) != ["process.log"]:
        raise SystemExit("existing COSCLI diagnostic directory contents are invalid")
    process_log = os.path.join(directory, "process.log")
    log_metadata = os.lstat(process_log)
    if (
        not stat.S_ISREG(log_metadata.st_mode)
        or stat.S_ISLNK(log_metadata.st_mode)
        or log_metadata.st_uid != expected_uid
        or stat.S_IMODE(log_metadata.st_mode) != 0o644
        or log_metadata.st_nlink != 1
        or log_metadata.st_size > 1024 * 1024
    ):
        raise SystemExit("existing COSCLI diagnostic process.log metadata is invalid")
PY
}

verify_existing_release_content() {
  local existing_dir="$1"
  local requested_dir="$2"
  python3 - "$existing_dir" "$requested_dir" "$companion_relative_dir" <<'PY'
import hashlib
import os
import stat
import sys

existing_root, requested_root, companion_relative = sys.argv[1:4]


def digest(path):
    value = hashlib.sha256()
    with open(path, "rb") as handle:
        for chunk in iter(lambda: handle.read(1024 * 1024), b""):
            value.update(chunk)
    return value.hexdigest()


def inventory(root, *, existing):
    entries = {}
    for directory, dirnames, filenames in os.walk(root, topdown=True, followlinks=False):
        relative_directory = os.path.relpath(directory, root)
        if relative_directory == ".":
            relative_directory = ""

        retained_directories = []
        for name in sorted(dirnames):
            path = os.path.join(directory, name)
            relative = os.path.join(relative_directory, name)
            if existing and relative == "coscli_output":
                continue
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


companion_path = os.path.join(existing_root, companion_relative)
if os.path.lexists(companion_path):
    companion_metadata = os.lstat(companion_path)
    if stat.S_ISLNK(companion_metadata.st_mode) or not stat.S_ISDIR(companion_metadata.st_mode):
        raise SystemExit("existing QiWe companion path must be a non-symlink directory")
    companion_missing = False
else:
    companion_missing = True

existing = inventory(existing_root, existing=True)
requested = inventory(requested_root, existing=False)
missing = set(requested) - set(existing)
extra = set(existing) - set(requested)
changed = {
    path for path in set(existing) & set(requested) if existing[path] != requested[path]
}

allowed_missing = set()
if companion_missing:
    allowed_missing = {
        path
        for path in requested
        if path == "sidecar-profiles"
        or path == companion_relative
        or path.startswith(f"{companion_relative}/")
    }

unexpected_missing = missing - allowed_missing
if unexpected_missing or extra or changed:
    details = []
    if unexpected_missing:
        details.append(f"missing={','.join(sorted(unexpected_missing)[:5])}")
    if extra:
        details.append(f"extra={','.join(sorted(extra)[:5])}")
    if changed:
        details.append(f"changed={','.join(sorted(changed)[:5])}")
    raise SystemExit(
        "existing release content differs from freshly verified artifacts"
        + (f": {'; '.join(details)}" if details else "")
    )

print("missing" if companion_missing else "complete")
PY
}

verify_existing_release_checksums() {
  local existing_dir="$1"
  local companion_state="$2"
  (
    cd "${existing_dir}/sidecar"
    sha256sum -c SHA256SUMS
  )
  (
    cd "${existing_dir}/deploy-bundle"
    sha256sum -c SHA256SUMS
  )
  if [[ "$companion_state" == "complete" ]]; then
    (
      cd "${existing_dir}/${companion_relative_dir}"
      sha256sum -c SHA256SUMS
    )
  fi
}

quarantine_existing_coscli_output() {
  local existing_dir="$1"
  local source_path="${existing_dir}/coscli_output"
  if [[ ! -d "$source_path" || -L "$source_path" ]]; then
    echo "validated COSCLI diagnostic root changed before quarantine" >&2
    return 1
  fi
  if [[ -e "$quarantine_root" ]]; then
    python3 - "$quarantine_root" "$(id -u)" <<'PY'
import os
import stat
import sys

path = sys.argv[1]
expected_uid = int(sys.argv[2])
metadata = os.lstat(path)
if (
    stat.S_ISLNK(metadata.st_mode)
    or not stat.S_ISDIR(metadata.st_mode)
    or metadata.st_uid != expected_uid
    or stat.S_IMODE(metadata.st_mode) & 0o077
):
    raise SystemExit("deploy quarantine root metadata is invalid")
PY
  else
    install -d -m 0700 "$quarantine_root"
  fi
  quarantine_container="$(mktemp -d "${quarantine_root}/${release_sha}-${request_id}.XXXXXX")"
  chmod 0700 "$quarantine_container"
  quarantined_coscli_source="$source_path"
  quarantined_coscli_target="${quarantine_container}/coscli_output"
  mv "$quarantined_coscli_source" "$quarantined_coscli_target"
  quarantine_active=true
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
  "$staging_dir/sidecar-profiles" \
  "$staging_dir/${companion_relative_dir}" \
  "$staging_dir/deploy-bundle"

QINTOPIA_SIDECAR_ARTIFACT_PROFILE="huabaosi-production" \
deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --artifact-type sidecar \
  --sha "$runtime_sha" \
  --output-dir "${staging_dir}/sidecar"

QINTOPIA_SIDECAR_ARTIFACT_PROFILE="$companion_runtime_artifact_profile" \
deploy/sidecar/scripts/fetch-cos-artifact.sh \
  --artifact-type sidecar \
  --sha "$runtime_sha" \
  --output-dir "${staging_dir}/${companion_relative_dir}"

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
    "companion_runtime_artifact_profiles": ["qiwe-production"],
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
chmod 0444 "$staging_dir/manifest.json"

test -x "${staging_dir}/sidecar/qintopia-message-sidecar"
test -x "${staging_dir}/${companion_relative_dir}/qintopia-message-sidecar"
test -f "${staging_dir}/manifest.json"
test -d "${staging_dir}/deploy"
validate_release_tree "$staging_dir"

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
  python3 - "$adopted_manifest_tmp" "$staging_dir/manifest.json" <<'PY'
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
    "runtime_artifact_profile",
    "deploy_bundle_sha",
    "commit_sha",
    "release_scope",
    "restart_targets",
)
for key in keys:
    if manifest.get(key) != requested.get(key):
        raise SystemExit(f"existing release manifest {key} mismatch")
PY

  coscli_diagnostics_present=false
  if [[ -e "${release_dir}/coscli_output" || -L "${release_dir}/coscli_output" ]]; then
    validate_existing_coscli_output "${release_dir}/coscli_output"
    coscli_diagnostics_present=true
  fi

  companion_state="$(verify_existing_release_content "$release_dir" "$staging_dir")"
  verify_existing_release_checksums "$release_dir" "$companion_state"

  if [[ "$dry_run" == "true" ]]; then
    echo "Dry run validated existing release ${release_sha}; companion_state=${companion_state}; coscli_diagnostics=${coscli_diagnostics_present}"
    exit 0
  fi

  existing_manifest_backup_tmp="$(mktemp "${release_root}/.existing-manifest-backup-${release_sha}.XXXXXX.json")"
  cp -p "${release_dir}/manifest.json" "$existing_manifest_backup_tmp"

  if [[ "$coscli_diagnostics_present" == "true" ]]; then
    quarantine_existing_coscli_output "$release_dir"
  fi

  if [[ "$companion_state" == "missing" ]]; then
    if [[ ! -e "${release_dir}/sidecar-profiles" ]]; then
      install -d -m 0755 "${release_dir}/sidecar-profiles"
      companion_parent_created=true
    fi
    companion_install_active=true
    cp -a \
      "${staging_dir}/${companion_relative_dir}" \
      "${release_dir}/sidecar-profiles/${companion_runtime_artifact_profile}"
  fi

  repair_existing_release_metadata "$release_dir" "$staging_dir"
  chmod 0444 "$adopted_manifest_tmp"
  mv "$adopted_manifest_tmp" "${release_dir}/manifest.json"
  adopted_manifest_tmp=""
  validate_release_tree "$release_dir"
  rm -rf "$staging_dir"
  companion_install_active=false
  quarantine_active=false
else
  if [[ "$dry_run" == "true" ]]; then
    echo "Dry run validated new release ${release_sha} with Huabaosi primary and QiWe companion runtimes"
    exit 0
  fi
  mv "$staging_dir" "$release_dir"
fi

release_target="$(readlink -f "$release_dir")"
if [[ -n "$current_target" && "$current_target" != "$release_target" ]]; then
  ln -sfn "$current_target" "${release_root}/previous"
fi
ln -sfn "$release_dir" "${release_root}/current"

if [[ -n "$existing_manifest_backup_tmp" ]]; then
  rm -f "$existing_manifest_backup_tmp"
  existing_manifest_backup_tmp=""
fi
promotion_complete=true
echo "Promoted ${release_sha} for request ${request_id}"
