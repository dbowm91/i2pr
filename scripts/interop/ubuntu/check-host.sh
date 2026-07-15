#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/../lib/common.sh"

mode=""
metadata_path="$INTEROP_TARGET/host-metadata.json"
while (($#)); do
  case "$1" in
    --pre-install|--post-install) mode=$1 ;;
    --metadata)
      (($# >= 2)) || die "--metadata requires a path"
      metadata_path=$2
      shift
      ;;
    *) die "usage: check-host.sh --pre-install|--post-install [--metadata <path>]" ;;
  esac
  shift
done
[[ -n "$mode" ]] || die "usage: check-host.sh --pre-install|--post-install [--metadata <path>]"

require_file /etc/os-release
source /etc/os-release
[[ "${ID:-}" == "ubuntu" ]] || die "Plan 040 requires Ubuntu; detected ${ID:-unknown}"
[[ "${VERSION_ID:-}" == "24.04" ]] || die "Plan 040 requires Ubuntu 24.04; detected ${VERSION_ID:-unknown}"
[[ "$(uname -m)" == "x86_64" || "$(uname -m)" == "amd64" ]] \
  || die "Plan 040 requires amd64/x86_64"
[[ "${BASH_VERSINFO[0]}" -ge 4 ]] || die "Bash 4 or newer is required"

for command in awk df install locale sha256sum uname; do require_command "$command"; done
if [[ "$EUID" -ne 0 ]]; then
  require_command sudo
  sudo -n true || die "noninteractive sudo is required"
fi

locale_output=$(locale charmap 2>/dev/null || true)
[[ "$locale_output" == UTF-8* ]] || die "a UTF-8 locale is required"

target_parent="$REPO_ROOT/target"
[[ -d "$target_parent" ]] || target_parent="$REPO_ROOT"
[[ -d "$target_parent" && -w "$target_parent" ]] || die "target parent is not writable"
free_kb=$(df -Pk "$target_parent" | awk 'NR==2 {print $4}')
[[ "$free_kb" =~ ^[0-9]+$ && "$free_kb" -ge 4194304 ]] \
  || die "at least 4 GiB of free space is required under target"

if [[ "$mode" == "--pre-install" ]]; then
  require_command apt-get
  for command in bash git curl python3 ip nft java ant cmake g++ gettext openssl; do
    if command -v "$command" >/dev/null 2>&1; then
      printf 'host.available.%s=true\n' "$command"
    else
      printf 'host.available.%s=false\n' "$command"
    fi
  done
else
  for command in bash git curl python3 ip nft java ant cmake g++ gettext openssl; do
    require_command "$command"
  done
fi

java_version=not-installed
ant_version=not-installed
cmake_version=not-installed
compiler_version=not-installed
nft_version=not-installed
iproute2_version=not-installed
python_version=not-installed
if command -v java >/dev/null 2>&1; then java_version=$(java -version 2>&1 | head -n 1); fi
if command -v ant >/dev/null 2>&1; then ant_version=$(ant -version 2>&1 | head -n 1); fi
if command -v cmake >/dev/null 2>&1; then cmake_version=$(cmake --version | head -n 1); fi
if command -v g++ >/dev/null 2>&1; then compiler_version=$(g++ --version | head -n 1); fi
if command -v nft >/dev/null 2>&1; then nft_version=$(nft --version | head -n 1); fi
if command -v ip >/dev/null 2>&1; then iproute2_version=$(ip -V | head -n 1); fi
if command -v python3 >/dev/null 2>&1; then python_version=$(python3 --version 2>&1 | head -n 1); fi

if [[ "$mode" == "--post-install" ]]; then
  ensure_target_dirs
  probe="i2pr-host-probe-$$"
  probe_created=0
  cleanup_probe() {
    if ((probe_created)); then
      root_run ip netns del "$probe" >/dev/null 2>&1 || true
      probe_created=0
    fi
  }
  trap cleanup_probe EXIT
  root_run ip netns add "$probe"
  probe_created=1
  root_run ip -n "$probe" link set lo up
  root_run ip netns exec "$probe" nft -f - <<'EOF'
flush ruleset
table inet i2pr_host_probe {
  chain input { type filter hook input priority 0; policy drop; }
}
EOF
  root_run ip netns exec "$probe" nft list table inet i2pr_host_probe >/dev/null
  cleanup_probe
  root_run ip netns list | awk '{print $1}' | grep -Fxq "$probe" && die "host probe namespace could not be deleted" || true
  trap - EXIT

  stale_namespaces=$(root_run ip netns list | awk '$1 ~ /^(i2pr|ref|java|i2pd)-[A-Za-z0-9-]+$/ {print $1}' || true)
  [[ -z "$stale_namespaces" ]] || die "stale Plan 038/039/040/041 namespace exists"
  stale_veths=$(root_run ip -o link show | awk -F': ' '$2 ~ /^(i2pr-v|ref-v|jv[0-9a-f]{8}a|iv[0-9a-f]{8}b)/ {print $2}' || true)
  [[ -z "$stale_veths" ]] || die "stale Plan 040/041 veth exists"
  if pgrep -af '(^|/)(i2pd|i2pr-interop|i2prouter)( |$)' >/dev/null 2>&1; then
    die "a reference-router or i2pr interoperability process is already running"
  fi
fi

json_escape() {
  local value=$1
  value=${value//\\/\\\\}
  value=${value//\"/\\\"}
  value=${value//$'\n'/ }
  printf '%s' "$value"
}

metadata_parent=$(dirname "$metadata_path")
mkdir -p "$metadata_parent"
[[ -w "$metadata_parent" ]] || die "host metadata parent is not writable"
cat >"$metadata_path" <<EOF
{"schema":1,"mode":"$(json_escape "$mode")","os_id":"$(json_escape "${ID:-unknown}")","os_version":"$(json_escape "${VERSION_ID:-unknown}")","architecture":"$(json_escape "$(uname -m)")","kernel":"$(json_escape "$(uname -r)")","locale":"$(json_escape "$locale_output")","python":"$(json_escape "$python_version")","java":"$(json_escape "$java_version")","ant":"$(json_escape "$ant_version")","cmake":"$(json_escape "$cmake_version")","compiler":"$(json_escape "$compiler_version")","nft":"$(json_escape "$nft_version")","iproute2":"$(json_escape "$iproute2_version")","free_kb":$free_kb,"host_contract":"$HOST_CONTRACT"}
EOF
chmod 0600 "$metadata_path"

printf 'host.os_id=%s\n' "${ID:-unknown}"
printf 'host.os_version=%s\n' "${VERSION_ID:-unknown}"
printf 'host.architecture=%s\n' "$(uname -m)"
printf 'host.kernel=%s\n' "$(uname -r)"
printf 'host.python=%s\n' "$python_version"
printf 'host.java=%s\n' "$java_version"
printf 'host.ant=%s\n' "$ant_version"
printf 'host.cmake=%s\n' "$cmake_version"
printf 'host.compiler=%s\n' "$compiler_version"
printf 'host.nft=%s\n' "$nft_version"
printf 'host.iproute2=%s\n' "$iproute2_version"
printf 'host.locale=%s\n' "$locale_output"
printf 'host.mode=%s\n' "$mode"
printf 'host.metadata=%s\n' "$metadata_path"
