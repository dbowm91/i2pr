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
for command in git cmake c++ sha256sum python3 openssl sed; do require_command "$command"; done

command_version="i2pd-cmake-relwithdebinfo-v1"
cache_key=$(cache_key_for i2pd "$I2PD_REVISION" "$command_version")
cache_dir="$CACHE_ROOT/$I2PD_REFERENCE/$cache_key"
metadata="$cache_dir/build-metadata.txt"
if [[ "$force" == "0" && -f "$metadata" ]]; then
  validate_cache_metadata "$metadata" "$I2PD_REFERENCE"
  printf 'reference=%s\ncache_key=%s\nmetadata=%s\ndisposition=cache-reused\n' \
    "$I2PD_REFERENCE" "$cache_key" "$metadata"
  exit 0
fi
if [[ "$force" == "1" && -d "$cache_dir" ]]; then
  chmod -R u+w "$cache_dir"
  rm -rf "$cache_dir"
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
verify_git_remote "$source_dir" "$I2PD_REPOSITORY"
(cd "$build_dir" && cmake -DWITH_GIT_VERSION=ON -DWITH_UPNP=OFF -DCMAKE_BUILD_TYPE=RelWithDebInfo "$source_dir") \
  >"$log_dir/cmake-configure.log" 2>&1
cpus=$(nproc 2>/dev/null || printf '1')
memory_kb=$(awk '/MemTotal:/ {print $2}' /proc/meminfo)
jobs=$((cpus < 4 ? cpus : 4))
if [[ "$memory_kb" =~ ^[0-9]+$ && "$memory_kb" -lt 8388608 ]]; then jobs=1; fi
(( jobs > 0 )) || jobs=1
(cd "$build_dir" && cmake --build . --parallel "$jobs") >"$log_dir/build.log" 2>&1
test_disposition="not-available"
if cmake --build "$build_dir" --target help 2>/dev/null | grep -Eq '(^|[[:space:]])test([[:space:]]|$)'; then
  (cd "$build_dir" && cmake --build . --target test) >"$log_dir/tests.log" 2>&1
  test_disposition="target-test-ran"
elif [[ -f "$build_dir/CTestTestfile.cmake" ]]; then
  (cd "$build_dir" && ctest --output-on-failure) >"$log_dir/tests.log" 2>&1
  test_disposition="ctest-ran"
fi
binary="$build_dir/i2pd"
[[ -x "$binary" ]] || binary=$(find "$build_dir" -type f -name i2pd -perm -u+x -print -quit)
[[ -n "$binary" && -x "$binary" ]] || die "i2pd build did not produce an executable"
if ! "$binary" --version >"$log_dir/version.txt" 2>&1; then
  die "i2pd --version probe failed"
fi
grep -Fq '2.60.0' "$log_dir/version.txt" \
  || die "i2pd version does not report the locked release"
version_check="release-2.60.0;source-checkout-full-object-id-verified"
reported_hash=$(grep -Eo '[0-9a-f]{7,40}' "$log_dir/version.txt" | head -n 1 || true)
if [[ -n "$reported_hash" && "$I2PD_REVISION" != "$reported_hash"* ]]; then
  die "i2pd version reports a source revision different from the lock"
fi
if [[ -n "$reported_hash" ]]; then
  version_check="release-2.60.0;binary-revision-${I2PD_REVISION}"
fi
mkdir -p "$cache_dir/bin"
install -m 0755 "$binary" "$cache_dir/bin/i2pd"
if [[ -d "$source_dir/contrib/certificates" ]]; then
  cp -a "$source_dir/contrib/certificates" "$cache_dir/"
fi
boost_version=$(sed -n 's/^#define BOOST_LIB_VERSION "\(.*\)"/\1/p' /usr/include/boost/version.hpp 2>/dev/null | head -n 1)
openssl_version=$(openssl version | head -n 1)
zlib_version=$(sed -n 's/^#define ZLIB_VERSION "\(.*\)"/\1/p' /usr/include/zlib.h 2>/dev/null | head -n 1)
[[ -n "$boost_version" && -n "$zlib_version" ]] || die "Boost or zlib version headers are missing"
artifact_sha256=$(sha256sum "$binary" | awk '{print $1}')
installed_tree_sha256=$(hash_tree "$cache_dir")
write_metadata_header "$metadata" "$I2PD_REFERENCE" "$I2PD_REVISION" "$command_version"
{
  printf 'source_repository=%s\n' "$I2PD_REPOSITORY"
  printf 'artifact_sha256=%s\n' "$artifact_sha256"
  printf 'artifact_path=bin/i2pd\n'
  printf 'installed_tree_sha256=%s\n' "$installed_tree_sha256"
  printf 'launcher=bin/i2pd\n'
  printf 'execution_network=forbidden\n'
  printf 'toolchain=compiler:%s;cmake:%s;boost:%s;openssl:%s;zlib:%s\n' \
    "$(c++ --version | head -n 1)" "$(cmake --version | head -n 1)" \
    "$boost_version" "$openssl_version" "$zlib_version"
  printf 'launcher_probe=version-nonpersistent\n'
  printf 'version_check=%s\n' "$version_check"
  printf 'test_disposition=%s\n' "$test_disposition"
} >>"$metadata"
validate_cache_metadata "$metadata" "$I2PD_REFERENCE"
chmod -R a-w "$cache_dir"
printf 'reference=%s\ncache_key=%s\nmetadata=%s\nartifact_sha256=%s\ninstalled_tree_sha256=%s\n' \
  "$I2PD_REFERENCE" \
  "$cache_key" "$metadata" "$artifact_sha256" "$installed_tree_sha256"
