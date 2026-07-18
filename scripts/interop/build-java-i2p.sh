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
for command in git java ant gettext curl sha256sum python3; do require_command "$command"; done

command_version="java_i2p-ant-pkg5-v1"
cache_key=$(cache_key_for "$JAVA_REFERENCE" "$JAVA_REVISION" "$command_version")
cache_dir="$CACHE_ROOT/$JAVA_REFERENCE/$cache_key"
metadata="$cache_dir/build-metadata.txt"
if [[ "$force" == "0" && -f "$metadata" ]]; then
  validate_cache_metadata "$metadata" "$JAVA_REFERENCE"
  printf 'reference=%s\ncache_key=%s\nmetadata=%s\ndisposition=cache-reused\n' \
    "$JAVA_REFERENCE" "$cache_key" "$metadata"
  exit 0
fi
if [[ "$force" == "1" && -d "$cache_dir" ]]; then
  chmod -R u+w "$cache_dir"
  rm -rf "$cache_dir"
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
verify_git_remote "$source_dir" "$JAVA_REPOSITORY"
cleanup_generated_source() {
  rm -f "$source_dir/override.properties" "$source_dir/install.jar"
  rm -rf "$source_dir/com"
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
cat >"$override" <<EOF
build.built-by=Plan 038 harness
noExe=true
izpack5.home=$BUILD_ROOT/tools/izpack/$cache_key
EOF
izpack_root="$BUILD_ROOT/tools/izpack/$cache_key"
mkdir -p "$izpack_root"
izpack_options="$log_dir/izpack-install.properties"
printf '# Verified IzPack 5.2.4\nINSTALL_PATH=%s\n' "$izpack_root" >"$izpack_options"
java -jar "$izpack" -options-auto "$izpack_options" >"$log_dir/izpack-install.log" 2>&1
export IZPACK_HOME="$izpack_root"
# IzPack 5.2.4's installer always extracts a com/izforge/izpack tree into
# its working directory regardless of INSTALL_PATH; remove it so the source
# checkout is not dirtied before the ant build's git status check.
rm -rf "$source_dir/com"
auto_options="$log_dir/auto-install.properties"
cat >"$auto_options" <<EOF
sys.installationDir=$install_dir
INSTALL_PATH=$install_dir
sys.language.selected=eng
sys.pack.selected.Base=true
sys.pack.selected.Windows Service=false
EOF
printf 'reference=%s\nrevision=%s\ncommand=ant distclean pkg5\n' "$JAVA_REFERENCE" "$JAVA_REVISION" \
  >"$log_dir/build-command.txt"
grep -Fq '<target name="pkg5"' "$source_dir/build.xml" \
  || die "pinned Java I2P source does not contain the reviewed pkg5 target"
(cd "$source_dir" && ant distclean pkg5) >"$log_dir/ant.log" 2>&1
[[ -f "$source_dir/install.jar" ]] || die "Java I2P build did not produce install.jar"
install -m 0644 "$source_dir/install.jar" "$log_dir/install.jar"
java -jar "$source_dir/install.jar" -options-auto "$auto_options" >"$log_dir/install.log" 2>&1

launcher=""
for candidate in "$install_dir/runplain.sh" "$install_dir/i2prouter"; do
  if [[ -x "$candidate" ]] && head -n 1 "$candidate" | grep -q '^#!'; then
    launcher="$candidate"
    break
  fi
done
[[ -n "$launcher" ]] || die "Java I2P staged runtime has no reviewed headless shell launcher"
mkdir -p "$cache_dir"
cp -a "$install_dir/." "$cache_dir/"
mkdir -p "$cache_dir/artifacts"
install -m 0644 "$source_dir/install.jar" "$cache_dir/artifacts/install.jar"
if head -n 1 "$launcher" | grep -q '^#!'; then
  bash -n "$launcher" >"$log_dir/launcher-inspection.txt"
else
  die "Java I2P launcher is not a reviewed shell launcher"
fi
artifact_sha256=$(sha256sum "$cache_dir/artifacts/install.jar" | awk '{print $1}')
installed_tree_sha256=$(hash_tree "$cache_dir")
write_metadata_header "$metadata" "$JAVA_REFERENCE" "$JAVA_REVISION" "$command_version"
{
  printf 'source_repository=%s\n' "$JAVA_REPOSITORY"
  printf 'artifact_sha256=%s\n' "$artifact_sha256"
  printf 'artifact_path=artifacts/install.jar\n'
  printf 'installed_tree_sha256=%s\n' "$installed_tree_sha256"
  printf 'launcher=%s\n' "${launcher#"$install_dir/"}"
  printf 'execution_network=forbidden\n'
  printf 'toolchain=java:%s;ant:%s;gettext:%s;izpack:5.2.4\n' \
    "$(java -version 2>&1 | head -n 1)" "$(ant -version 2>&1 | head -n 1)" \
    "$(gettext --version | head -n 1)"
  printf 'launcher_probe=static-bash-syntax-only\n'
  printf 'version_check=install-jar-sha256-verified\n'
  printf 'test_disposition=not-applicable\n'
} >>"$metadata"
validate_cache_metadata "$metadata" "$JAVA_REFERENCE"
chmod -R a-w "$cache_dir"
printf 'reference=%s\ncache_key=%s\nmetadata=%s\nartifact_sha256=%s\ninstalled_tree_sha256=%s\n' \
  "$JAVA_REFERENCE" \
  "$cache_key" "$metadata" "$artifact_sha256" "$installed_tree_sha256"
