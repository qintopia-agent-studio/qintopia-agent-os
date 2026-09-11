#!/usr/bin/env bash
set -euo pipefail

readonly INSTALL_ROOT=/usr/local/libexec
readonly FIXED_LAUNCHER="${INSTALL_ROOT}/qintopia-hermes-core-launcher"
readonly USER_UNIT_ROOT=/etc/systemd/user
readonly SOURCE_LAUNCHER="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)/qintopia-hermes-core-launcher"
readonly SCRIPT_DIR="$(CDPATH= cd -- "$(dirname -- "${BASH_SOURCE[0]}")" && pwd -P)"
readonly REPO_ROOT="$(CDPATH= cd -- "${SCRIPT_DIR}/../.." && pwd -P)"
readonly REGISTRY_READER="${REPO_ROOT}/tools/deploy/hermes-profile-registry.mjs"
readonly UNIT_RENDERER="${REPO_ROOT}/tools/deploy/render-hermes-core-systemd-unit.mjs"

blocked() {
  printf 'hermes_core_systemd_install=blocked\n' >&2
  printf 'hermes_core_systemd_install_error=%s\n' "$1" >&2
  exit 1
}

if [[ "${EUID}" -ne 0 ]]; then
  blocked runner_must_be_root
fi
if [[ ! -f "$SOURCE_LAUNCHER" || ! -f "$REGISTRY_READER" || ! -f "$UNIT_RENDERER" ]]; then
  blocked runner_dependency_missing
fi
node_bin=""
for candidate in /usr/bin/node /usr/local/bin/node; do
  if [[ -x "$candidate" ]]; then
    node_bin="$candidate"
    break
  fi
done
[[ -n "$node_bin" ]] || blocked runner_dependency_missing

mapfile -t profile_rows < <("$node_bin" "$REGISTRY_READER" --service-map)
[[ "${#profile_rows[@]}" -eq 7 ]] || blocked profile_registry_invalid

install -d -o root -g root -m 0755 "$INSTALL_ROOT"
temp_launcher="${FIXED_LAUNCHER}.pending.$$"
rm -f -- "$temp_launcher"
install -o root -g root -m 0555 "$SOURCE_LAUNCHER" "$temp_launcher"
mv -Tf -- "$temp_launcher" "$FIXED_LAUNCHER"

for row in "${profile_rows[@]}"; do
  IFS=$'\t' read -r profile service <<< "$row"
  case "$profile:$service" in
    default:hermes-gateway.service|erhua:hermes-gateway-erhua.service|guanerye:hermes-gateway-guanerye.service|huabaosi:hermes-gateway-huabaosi.service|silaoshi:hermes-gateway-silaoshi.service|wenyuange:hermes-gateway-wenyuange.service|xiaoman:hermes-gateway-xiaoman.service) ;;
    *) blocked profile_registry_invalid ;;
  esac
  dropin_dir="${USER_UNIT_ROOT}/${service}.d"
  install -d -o root -g root -m 0755 "$dropin_dir"
  dropin_path="${dropin_dir}/20-qintopia-hermes-core.conf"
  temp_dropin="${dropin_path}.pending.$$"
  rm -f -- "$temp_dropin"
  umask 077
  "$node_bin" "$UNIT_RENDERER" "$profile" >"$temp_dropin"
  chown root:root "$temp_dropin"
  chmod 0444 "$temp_dropin"
  mv -Tf -- "$temp_dropin" "$dropin_path"
done

if [[ -x /usr/bin/runuser && -d /run/user/1000 ]]; then
  /usr/bin/runuser -u ubuntu -- env XDG_RUNTIME_DIR=/run/user/1000 /usr/bin/systemctl --user daemon-reload
fi

printf 'hermes_core_systemd_install=ready\n'
printf 'hermes_core_systemd_profile_count=%s\n' "${#profile_rows[@]}"
