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
SYSTEM_java_io_tmpdir=$install_dir/tmp
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
mkdir -p "$install_dir/tmp" "$cache_dir/tmp"
chmod 0777 "$install_dir/tmp" "$cache_dir/tmp"
cp -a "$install_dir/." "$cache_dir/"
mkdir -p "$cache_dir/artifacts"
install -m 0644 "$source_dir/install.jar" "$cache_dir/artifacts/install.jar"
# Replace the headless launcher with a foreground exec so the harness
# ``BoundedProcess`` owns the long-lived Java VM directly and can observe
# its stdout/stderr for the ready token (the upstream ``nohup ... &`` form
# exits immediately, producing spurious ``process-exited-before-ready``
# results). The staged launcher also gets a private tmp dir relative to
# the launcher path so the router pid file lands in the runtime dir, which
# the harness always creates with 0o700.
write_harness_launcher() {
  cat > "$1" <<EOF
#!/bin/bash
# Plan 045 harness-compatible headless launcher. Substituted from upstream
# runplain.sh / i2prouter: JAVAOPTS keeps the I2P defaults so the router
# still honours i2p.dir.base, the logger filename, and headless mode.
# I2PTEMP is per-launcher so the pid file lands inside this dir. The
# router is exec'd in the foreground instead of being nohup-backgrounded
# so the harness BoundedProcess owns the long-lived JVM directly.
I2P="\$(cd "\$(dirname "\$0")" && pwd)"
I2PTEMP="\$(cd "\$(dirname "\$0")" && pwd)/tmp"
mkdir -p "\$I2PTEMP"
JAVA="\$(which java 2>/dev/null || command -v java 2>/dev/null)"
if [ -z "\$JAVA" ] || [ ! -x "\$JAVA" ]; then
  JAVA="/usr/lib/jvm/java-21-openjdk-amd64/bin/java"
fi
CP=""
for jar in \${I2P}/lib/*.jar; do
  if [ -z "\$CP" ]; then
    CP="\$jar"
  else
    CP="\${CP}:\${jar}"
  fi
done
JAVAOPTS="-Djava.net.preferIPv4Stack=false -Djava.awt.headless=true -Djava.library.path=\${I2P}:\${I2P}/lib -Di2p.dir.base=\${I2P} -DloggerFilenameOverride=logs/log-router-@.txt"
exec "\$JAVA" -cp "\$CP" \${JAVAOPTS} net.i2p.router.RouterLaunch
EOF
  chmod 0755 "$1"
}
mkdir -p "$cache_dir"
write_harness_launcher "$install_dir/runplain.sh"
write_harness_launcher "$cache_dir/runplain.sh"
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
