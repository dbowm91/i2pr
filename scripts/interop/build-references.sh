#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/lib/common.sh"
offline=0
force=0
for arg in "$@"; do
  case "$arg" in
    --offline) offline=1 ;;
    --force-rebuild) force=1 ;;
    *) printf 'usage: build-references.sh [--offline] [--force-rebuild]\n' >&2; exit 2 ;;
  esac
done
"$script_dir/ubuntu/check-host.sh" --post-install
ensure_target_dirs
args=()
[[ "$offline" == "1" ]] && args+=(--offline)
[[ "$force" == "1" ]] && args+=(--force-rebuild)
"$script_dir/build-java-i2p.sh" "${args[@]}" >"$BUILD_ROOT/java-i2p-summary.txt"
"$script_dir/build-i2pd.sh" "${args[@]}" >"$BUILD_ROOT/i2pd-summary.txt"
printf '{"schema":1,"java_i2p":"%s","i2pd":"%s"}\n' \
  "$(sed -n 's/^cache_key=//p' "$BUILD_ROOT/java-i2p-summary.txt" | tail -n 1)" \
  "$(sed -n 's/^cache_key=//p' "$BUILD_ROOT/i2pd-summary.txt" | tail -n 1)"
