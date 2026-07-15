#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/../lib/common.sh"

mode=${1:-}
[[ "$mode" == "--pre-install" || "$mode" == "--post-install" ]] \
  || die "usage: check-host.sh --pre-install|--post-install"

os_id=unknown
os_version=unknown
if [[ -r /etc/os-release ]]; then
  source /etc/os-release
  os_id=${ID:-unknown}
  os_version=${VERSION_ID:-unknown}
fi
[[ "$os_id" == "ubuntu" ]] || die "Plan 038 requires Ubuntu; detected $os_id"
[[ "$(uname -m)" == "x86_64" || "$(uname -m)" == "amd64" ]] \
  || die "Plan 038 requires amd64/x86_64"

require_command bash
if [[ "$mode" == "--post-install" ]]; then
  require_command python3
  require_command ip
  for command in ant cmake g++ git java nft sha256sum; do
    require_command "$command"
  done
  java_version=$(java -version 2>&1 | head -n 1)
  ant_version=$(ant -version 2>&1 | head -n 1)
  cmake_version=$(cmake --version | head -n 1)
  compiler_version=$(g++ --version | head -n 1)
else
  java_version=not-installed
  ant_version=not-installed
  cmake_version=not-installed
  compiler_version=not-installed
fi

require_command locale
locale_output=$(locale charmap 2>/dev/null || true)
[[ "$locale_output" == "UTF-8" || "$locale_output" == "UTF-8"* ]] \
  || die "a UTF-8 locale is required"

target_parent="$REPO_ROOT/target"
[[ -d "$target_parent" ]] || target_parent="$REPO_ROOT"
if [[ -e "$INTEROP_TARGET" ]]; then
  [[ -d "$INTEROP_TARGET" && -w "$INTEROP_TARGET" ]] \
    || die "target/interop must be a writable directory"
else
  [[ -d "$target_parent" && -w "$target_parent" ]] \
    || die "target or target/interop must be writable"
fi
free_kb=$(df -Pk "$target_parent" | awk 'NR==2 {print $4}')
[[ "$free_kb" =~ ^[0-9]+$ && "$free_kb" -ge 4194304 ]] \
  || die "at least 4 GiB of free space is required under target"

if [[ "$EUID" -ne 0 ]]; then require_command sudo; fi
if [[ "$mode" == "--post-install" ]]; then
  probe="i2pr-host-probe-$$"
  cleanup_probe() { root_run ip netns del "$probe" >/dev/null 2>&1 || true; }
  trap cleanup_probe EXIT
  root_run ip netns add "$probe"
  root_run ip netns exec "$probe" ip link set lo up
  root_run ip netns del "$probe"
  trap - EXIT

  stale_namespaces=$(root_run ip netns list | awk '$1 ~ /^i2pr-/ || $1 ~ /^ref-/ {print $1}' || true)
  [[ -z "$stale_namespaces" ]] || die "stale Plan 038 namespace exists"
  if pgrep -af '(^|/)(i2pd|i2pr-interop|i2prouter)( |$)' >/dev/null 2>&1; then
    die "a reference-router or i2pr interoperability process is already running"
  fi
fi

printf 'host.os_id=%s\n' "$os_id"
printf 'host.os_version=%s\n' "$os_version"
printf 'host.architecture=%s\n' "$(uname -m)"
printf 'host.kernel=%s\n' "$(uname -r)"
printf 'host.python=%s\n' "$(python3 --version 2>&1)"
printf 'host.java=%s\n' "$java_version"
printf 'host.ant=%s\n' "$ant_version"
printf 'host.cmake=%s\n' "$cmake_version"
printf 'host.compiler=%s\n' "$compiler_version"
printf 'host.locale=%s\n' "$locale_output"
printf 'host.mode=%s\n' "$mode"
