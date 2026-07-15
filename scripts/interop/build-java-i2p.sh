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
    *) die "usage: build-java-i2p.sh [--offline] [--force-rebuild]" ;;
  esac
done
assert_lock_contract
ensure_target_dirs
for command in git java ant gettext curl sha256sum; do require_command "$command"; done

command_version="java-i2p-ant-pkg5-v1"
cache_key=$(cache_key_for java-i2p "$JAVA_REVISION" "$command_version")
cache_dir="$CACHE_ROOT/java-i2p/$cache_key"
metadata="$cache_dir/build-metadata.txt"
if [[ "$force" == "0" && -f "$metadata" ]]; then
  expected_tree=$(sed -n 's/^installed_tree_sha256=//p' "$metadata")
  [[ -n "$expected_tree" && "$(hash_tree "$cache_dir")" == "$expected_tree" ]] \
    || die "cached Java I2P runtime tree hash mismatch"
  printf 'reference=java-i2p\ncache_key=%s\nmetadata=%s\n' "$cache_key" "$metadata"
  exit 0
fi

source_dir="$BUILD_ROOT/sources/java-i2p"
log_dir="$BUILD_ROOT/logs/java-i2p/$cache_key"
install_dir="$BUILD_ROOT/install/java-i2p/$cache_key"
mkdir -p "$log_dir" "$install_dir"
if [[ ! -d "$source_dir/.git" ]]; then
  [[ "$offline" == "0" ]] || die "offline Java I2P build requires cached source"
  git clone "$JAVA_REPOSITORY" "$source_dir"
fi
if [[ "$offline" == "0" ]]; then
  git -C "$source_dir" fetch --quiet origin "$JAVA_REVISION"
fi
git -C "$source_dir" checkout --detach --quiet "$JAVA_REVISION"
verify_git_revision "$source_dir" "$JAVA_REVISION"
cleanup_generated_source() {
  rm -f "$source_dir/override.properties" "$source_dir/install.jar"
}
trap cleanup_generated_source EXIT

izpack="$BUILD_ROOT/downloads/izpack-dist-5.2.4-installer.jar"
mkdir -p "$(dirname "$izpack")"
if [[ ! -f "$izpack" ]]; then
  [[ "$offline" == "0" ]] || die "offline Java I2P build requires verified IzPack cache"
  curl --fail --location --silent --show-error "$IZPACK_URL" -o "$izpack"
fi
verify_sha256 "$izpack" "$IZPACK_SHA256"

override="$source_dir/override.properties"
cat >"$override" <<'EOF'
build.built-by=Plan 038 harness
noExe=true
EOF
izpack_root="$BUILD_ROOT/tools/izpack/$cache_key"
mkdir -p "$izpack_root"
izpack_options="$log_dir/izpack-install.properties"
printf '# Verified IzPack 5.2.4\nINSTALL_PATH=%s\n' "$izpack_root" >"$izpack_options"
java -jar "$izpack" -options-auto "$izpack_options" >"$log_dir/izpack-install.log" 2>&1
export IZPACK_HOME="$izpack_root"
auto_options="$log_dir/auto-install.properties"
cat >"$auto_options" <<EOF
sys.installationDir=$install_dir
EOF
printf 'reference=java-i2p\nrevision=%s\ncommand=ant distclean pkg5\n' "$JAVA_REVISION" \
  >"$log_dir/build-command.txt"
(cd "$source_dir" && ant distclean pkg5) >"$log_dir/ant.log" 2>&1
[[ -f "$source_dir/install.jar" ]] || die "Java I2P build did not produce install.jar"
install -m 0644 "$source_dir/install.jar" "$log_dir/install.jar"
java -jar "$source_dir/install.jar" -options-auto "$auto_options" >"$log_dir/install.log" 2>&1

launcher=""
for candidate in "$install_dir/i2psvc" "$install_dir/i2prouter" "$install_dir/runplain.sh"; do
  if [[ -x "$candidate" ]]; then launcher="$candidate"; break; fi
done
[[ -n "$launcher" ]] || die "Java I2P staged runtime has no approved headless launcher"
"$launcher" --help >"$log_dir/readiness-probe.txt" 2>&1 || true
mkdir -p "$cache_dir"
cp -a "$install_dir/." "$cache_dir/"
artifact_sha256=$(sha256sum "$source_dir/install.jar" | awk '{print $1}')
installed_tree_sha256=$(hash_tree "$cache_dir")
write_metadata_header "$metadata" java-i2p "$JAVA_REVISION" "$command_version"
{
  printf 'source_repository=%s\n' "$JAVA_REPOSITORY"
  printf 'artifact_sha256=%s\n' "$artifact_sha256"
  printf 'installed_tree_sha256=%s\n' "$installed_tree_sha256"
  printf 'launcher=%s\n' "$(basename "$launcher")"
  printf 'execution_network=forbidden\n'
} >>"$metadata"
printf 'reference=java-i2p\ncache_key=%s\nartifact_sha256=%s\ninstalled_tree_sha256=%s\n' \
  "$cache_key" "$artifact_sha256" "$installed_tree_sha256"
