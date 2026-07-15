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
    *) die "usage: build-i2pd.sh [--offline] [--force-rebuild]" ;;
  esac
done
assert_lock_contract
ensure_target_dirs
for command in git cmake c++ sha256sum; do require_command "$command"; done

command_version="i2pd-cmake-relwithdebinfo-v1"
cache_key=$(cache_key_for i2pd "$I2PD_REVISION" "$command_version")
cache_dir="$CACHE_ROOT/i2pd/$cache_key"
metadata="$cache_dir/build-metadata.txt"
if [[ "$force" == "0" && -f "$metadata" ]]; then
  expected_tree=$(sed -n 's/^installed_tree_sha256=//p' "$metadata")
  [[ -n "$expected_tree" && "$(hash_tree "$cache_dir")" == "$expected_tree" ]] \
    || die "cached i2pd runtime tree hash mismatch"
  printf 'reference=i2pd\ncache_key=%s\nmetadata=%s\n' "$cache_key" "$metadata"
  exit 0
fi

source_dir="$BUILD_ROOT/sources/i2pd"
build_dir="$BUILD_ROOT/objects/i2pd/$cache_key"
log_dir="$BUILD_ROOT/logs/i2pd/$cache_key"
mkdir -p "$build_dir" "$log_dir"
if [[ ! -d "$source_dir/.git" ]]; then
  [[ "$offline" == "0" ]] || die "offline i2pd build requires cached source"
  git clone "$I2PD_REPOSITORY" "$source_dir"
fi
if [[ "$offline" == "0" ]]; then
  git -C "$source_dir" fetch --quiet origin "$I2PD_REVISION"
fi
git -C "$source_dir" checkout --detach --quiet "$I2PD_REVISION"
verify_git_revision "$source_dir" "$I2PD_REVISION"
(cd "$build_dir" && cmake -DWITH_GIT_VERSION=ON -DWITH_UPNP=OFF -DCMAKE_BUILD_TYPE=RelWithDebInfo "$source_dir") \
  >"$log_dir/cmake-configure.log" 2>&1
cpus=$(nproc 2>/dev/null || printf '1')
memory_kb=$(awk '/MemTotal:/ {print $2}' /proc/meminfo)
jobs=$((cpus < 4 ? cpus : 4))
if [[ "$memory_kb" =~ ^[0-9]+$ && "$memory_kb" -lt 8388608 ]]; then jobs=1; fi
(( jobs > 0 )) || jobs=1
(cd "$build_dir" && cmake --build . --parallel "$jobs") >"$log_dir/build.log" 2>&1
if cmake --build "$build_dir" --target help 2>/dev/null | grep -qE '(^| )test( |$)'; then
  (cd "$build_dir" && ctest --output-on-failure) >"$log_dir/tests.log" 2>&1
fi
binary="$build_dir/i2pd"
[[ -x "$binary" ]] || binary=$(find "$build_dir" -type f -name i2pd -perm -u+x -print -quit)
[[ -n "$binary" && -x "$binary" ]] || die "i2pd build did not produce an executable"
"$binary" --version >"$log_dir/version.txt" 2>&1 || true
mkdir -p "$cache_dir/bin"
install -m 0755 "$binary" "$cache_dir/bin/i2pd"
if [[ -d "$source_dir/contrib/certificates" ]]; then
  cp -a "$source_dir/contrib/certificates" "$cache_dir/"
fi
artifact_sha256=$(sha256sum "$binary" | awk '{print $1}')
installed_tree_sha256=$(hash_tree "$cache_dir")
write_metadata_header "$metadata" i2pd "$I2PD_REVISION" "$command_version"
{
  printf 'source_repository=%s\n' "$I2PD_REPOSITORY"
  printf 'artifact_sha256=%s\n' "$artifact_sha256"
  printf 'installed_tree_sha256=%s\n' "$installed_tree_sha256"
  printf 'launcher=bin/i2pd\n'
  printf 'execution_network=forbidden\n'
} >>"$metadata"
printf 'reference=i2pd\ncache_key=%s\nartifact_sha256=%s\ninstalled_tree_sha256=%s\n' \
  "$cache_key" "$artifact_sha256" "$installed_tree_sha256"
