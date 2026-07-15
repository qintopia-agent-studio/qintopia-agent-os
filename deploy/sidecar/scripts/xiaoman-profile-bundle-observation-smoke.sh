#!/usr/bin/env bash
set -euo pipefail

if [[ "${QINTOPIA_XIAOMAN_PROFILE_BUNDLE_OBSERVATION_ENABLE:-0}" != "1" ]]; then
  echo "Xiaoman profile bundle observation is disabled" >&2
  exit 1
fi

profile_dir="${QINTOPIA_XIAOMAN_PROFILE_DIR:-/home/ubuntu/.hermes/profiles/xiaoman}"
bundle_dir="${QINTOPIA_XIAOMAN_PROFILE_BUNDLE_DIR:-/home/ubuntu/qintopia-agent-os-releases/current/agents/xiaoman/profile-bundle}"
default_values_file="/etc/qintopia/xiaoman-profile-bundle-values.json"
values_file="${QINTOPIA_XIAOMAN_PROFILE_BUNDLE_VALUES_FILE:-$default_values_file}"
renderer="${bundle_dir}/render.py"
bundle_manifest="${bundle_dir}/bundle.json"

for path in "$renderer" "$bundle_manifest" "$values_file" "${profile_dir}/SOUL.md" "${profile_dir}/profile.yaml"; do
  if [[ ! -f "$path" || -L "$path" ]]; then
    echo "Xiaoman profile bundle observation requires regular input files" >&2
    exit 1
  fi
done

file_mode() {
  stat -c '%a' "$1" 2>/dev/null || stat -f '%Lp' "$1"
}

file_owner() {
  stat -c '%u' "$1" 2>/dev/null || stat -f '%u' "$1"
}

values_mode="$(file_mode "$values_file")"
values_owner="$(file_owner "$values_file")"
if ((8#$values_mode & 8#077)); then
  echo "Xiaoman profile bundle values file must not be group or world accessible" >&2
  exit 1
fi
observer_uid="$(id -u)"
if [[ "$values_file" == "$default_values_file" ]]; then
  if [[ "$observer_uid" != "0" || "$values_owner" != "0" ]]; then
    echo "Xiaoman production profile bundle observation requires root and a root-owned values file" >&2
    exit 1
  fi
fi
if [[ "$values_owner" != "$observer_uid" ]]; then
  echo "Xiaoman profile bundle values file must be owned by the observing user" >&2
  exit 1
fi

read -r expected_soul_hash expected_profile_hash < <(
  python3 - "$bundle_manifest" <<'PY'
import json
import sys

with open(sys.argv[1], encoding="utf-8") as handle:
    manifest = json.load(handle)
files = {item["target"]: item for item in manifest.get("files", [])}
if manifest.get("status") != "observation-only":
    raise SystemExit("bundle is not observation-only")
if set(files) != {"SOUL.md", "profile.yaml"}:
    raise SystemExit("bundle file allowlist mismatch")
print(files["SOUL.md"]["production_source_sha256"], files["profile.yaml"]["production_source_sha256"])
PY
)

hash_file() {
  if command -v sha256sum >/dev/null 2>&1; then
    sha256sum "$1" | awk '{print $1}'
  else
    shasum -a 256 "$1" | awk '{print $1}'
  fi
}

live_soul_hash_before="$(hash_file "${profile_dir}/SOUL.md")"
live_profile_hash_before="$(hash_file "${profile_dir}/profile.yaml")"
if [[ "$live_soul_hash_before" != "$expected_soul_hash" || "$live_profile_hash_before" != "$expected_profile_hash" ]]; then
  echo "Xiaoman live profile source hash drifted from the reviewed inventory" >&2
  exit 1
fi

temporary_root="$(mktemp -d "${TMPDIR:-/tmp}/qintopia-xiaoman-profile-observation.XXXXXX")"
trap 'rm -rf "$temporary_root"' EXIT
rendered_dir="${temporary_root}/rendered"
python3 "$renderer" --values-file "$values_file" --output-dir "$rendered_dir" >/dev/null

if ! cmp -s "${profile_dir}/SOUL.md" "${rendered_dir}/SOUL.md"; then
  echo "Xiaoman rendered SOUL.md does not match the reviewed live source" >&2
  exit 1
fi
if ! cmp -s "${profile_dir}/profile.yaml" "${rendered_dir}/profile.yaml"; then
  echo "Xiaoman rendered profile.yaml does not match the reviewed live source" >&2
  exit 1
fi

live_soul_hash_after="$(hash_file "${profile_dir}/SOUL.md")"
live_profile_hash_after="$(hash_file "${profile_dir}/profile.yaml")"
if [[ "$live_soul_hash_before" != "$live_soul_hash_after" || "$live_profile_hash_before" != "$live_profile_hash_after" ]]; then
  echo "Xiaoman live profile changed during observation" >&2
  exit 1
fi

python3 - "$live_soul_hash_after" "$live_profile_hash_after" <<'PY'
import json
import sys

print(json.dumps({
    "schema_version": 1,
    "status": "xiaoman_profile_bundle_observation_passed",
    "observation_only": True,
    "live_profile_modified": False,
    "symlink_created": False,
    "soul_match": True,
    "profile_match": True,
    "source_hashes": {
        "SOUL.md": sys.argv[1],
        "profile.yaml": sys.argv[2],
    },
}, separators=(",", ":")))
PY
