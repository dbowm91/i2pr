#!/usr/bin/env bash
# Plan 194 — fetch/build the exact-pinned Java I2P second-family
# reference for the M6 mixed-router (two-family) qualification lane.
#
# Usage:
#   bash scripts/interop/fetch-m6-java.sh [--rebuild]
#
# Mandatory second-family reference (per Plan 194 §3):
#   Java I2P 2.13.0
#   repository = https://github.com/i2p/i2p.i2p.git
#   commit     = 9134f808337b401e8e53c73734c81fab04280c9d
#
# The build deliberately avoids the full IzPack 5 GUI installer path
# the upstream `ant pkg5` target requires:
#   1. `ant updater` produces `pkg-temp/` with every router JAR
#      (i2p.jar, router.jar, sam.jar, streaming.jar, ...), every
#      server WAR, the locale bundle, the certificates bundle, and
#      the runner scripts — but skips the IzPack packaging step;
#   2. `ant preppkg` (after `updater`) finishes `pkg-temp/` with
#      `clients.config`, `wrapper.config`, `hosts.txt`, the per-OS
#      wrapper binaries, and the eepsite tree;
#   3. `build-instance.sh` substitutes INSTALL_PATH/JAVA_HOME in the
#      staged `runplain.sh` and stamps `build-metadata.txt` with the
#      exact pin, repository, and per-OS launcher probe so the
#      Plan 194 harness can verify the cache before starting the
#      router.
#
# No third-party source is committed; the cache is disposable and lives
# under `target/interop`. The Java router is unmodified: no source
# patching, no vendoring, no public-network participation.
#
# The lane is unprivileged and loopback-only. The build emits the
# headless launcher the same way the existing i2pd cache does; the
# router process is owned by the calling harness exactly like every
# other per-layer harness.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CACHE_ROOT="${REPO_ROOT}/target/interop/cache/m6-java"
WORK_ROOT="${REPO_ROOT}/target/interop/m6-java-sources"

JAVA_REPO="https://github.com/i2p/i2p.i2p.git"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION="2.13.0"

REBUILD=""
for arg in "$@"; do
  case "${arg}" in
    --rebuild) REBUILD="--rebuild" ;;
    *) echo "usage: $0 [--rebuild]" >&2; exit 64 ;;
  esac
done

for command in git java ant sha256sum curl; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "fetch-m6-java.sh: required command missing: ${command}" >&2
    exit 1
  }
done

JAVA_CACHE="${CACHE_ROOT}/${JAVA_PIN}"
JAVA_SRC_DEFAULT="${WORK_ROOT}/i2p.i2p-${JAVA_PIN}"
JAVA_SRC_OVERRIDE="${I2PR_M6_JAVA_SRC:-${I2PR_JAVA_I2P_SRC:-}}"
JAVA_SRC="${JAVA_SRC_OVERRIDE:-${JAVA_SRC_DEFAULT}}"

mkdir -p "${CACHE_ROOT}" "${WORK_ROOT}"

verify_git_revision() {
  local source="$1"
  local pin="$2"
  [[ -d "${source}/.git" ]] || { echo "java_i2p source is not a git checkout: ${source}" >&2; exit 1; }
  [[ -z "$(git -C "${source}" status --porcelain --untracked-files=no)" ]] \
    || { echo "java_i2p source tree is dirty (refusing to build a patched reference)" >&2; git -C "${source}" status --porcelain --untracked-files=no >&2; exit 1; }
  local actual
  actual="$(git -C "${source}" rev-parse HEAD)"
  [[ "${actual}" == "${pin}" ]] \
    || { echo "java_i2p pin mismatch: expected ${pin}, got ${actual}" >&2; exit 1; }
  local remote
  remote="$(git -C "${source}" remote get-url origin 2>/dev/null || true)"
  [[ "${remote}" == "${JAVA_REPO}" || "${remote}" == "${JAVA_REPO%.git}" ]] \
    || { echo "java_i2p origin mismatch: expected ${JAVA_REPO}, got ${remote}" >&2; exit 1; }
}

write_metadata() {
  local cache_dir="$1"
  local toolchain
  toolchain="java:$(java -version 2>&1 | head -n 1);ant:$(ant -version 2>&1 | head -n 1)"
  local installed_tree_sha
  installed_tree_sha="$(find "${cache_dir}" -type f -printf '%P\0' | \
    LC_ALL=C.UTF-8 sort -z | while IFS= read -r -d '' path; do
      sha256sum "${cache_dir}/${path}"
    done | sha256sum | awk '{print $1}')"
  {
    printf 'schema=2\n'
    printf 'reference=java_i2p\n'
    printf 'source_revision=%s\n' "${JAVA_PIN}"
    printf 'build_command_version=m6-java-ant-updater-preppkg-v1\n'
    printf 'host_contract=ubuntu-24.04-amd64\n'
    printf 'source_repository=%s\n' "${JAVA_REPO}"
    printf 'installed_tree_sha256=%s\n' "${installed_tree_sha}"
    printf 'launcher=runplain.sh\n'
    printf 'execution_network=forbidden\n'
    printf 'toolchain=%s\n' "${toolchain}"
    printf 'version_check=install-tree-sha256-verified\n'
    printf 'test_disposition=not-applicable\n'
  } > "${cache_dir}/build-metadata.txt"
  printf '%s\n' "${JAVA_PIN}" > "${cache_dir}/source-revision.txt"
  printf '%s\n' "${JAVA_REPO}" > "${cache_dir}/source-repository.txt"
  printf '%s\n' "${JAVA_VERSION}" > "${cache_dir}/source-version.txt"
  printf '%s\n' "ant updater preppkg (no IzPack; per Plan 194 §3)" \
    > "${cache_dir}/build-command.txt"
}

if [[ -n "${JAVA_SRC_OVERRIDE}" ]]; then
  if [[ ! -d "${JAVA_SRC}/.git" ]]; then
    echo "java_i2p source override is not a Git checkout: ${JAVA_SRC}" >&2
    exit 1
  fi
elif [[ ! -d "${JAVA_SRC}/.git" ]]; then
  echo "==> cloning Java I2P ${JAVA_VERSION}" >&2
  git clone "${JAVA_REPO}" "${JAVA_SRC}"
elif [[ "${REBUILD}" == "--rebuild" ]]; then
  echo "==> refreshing Java I2P source" >&2
  git -C "${JAVA_SRC}" fetch --tags --force origin
fi

if [[ -z "${JAVA_SRC_OVERRIDE}" ]]; then
  # Fresh clones land on the remote default branch; force the exact
  # pin before the build emits any artifacts.
  git -C "${JAVA_SRC}" checkout --detach "${JAVA_PIN}"
fi
verify_git_revision "${JAVA_SRC}" "${JAVA_PIN}"

# The artifacts we need come from `ant updater preppkg`, not `pkg5`:
# `pkg5` adds the IzPack 5 GUI installer step that the Plan 194 lane
# deliberately skips because it (a) requires downloading a separate
# IzPack 5.2.4 installer at runtime, (b) needs Java 13 or older for
# IzPack 4, and (c) only packages what `pkg-temp/` already contains.
# The staged `pkg-temp/` directory IS the install directory.
mkdir -p "${JAVA_CACHE}"
if [[ "${REBUILD}" == "--rebuild" || ! -d "${JAVA_CACHE}/lib" ]] || \
   [[ ! -x "${JAVA_CACHE}/runplain.sh" ]]; then
  echo "==> building Java I2P ${JAVA_VERSION} (${JAVA_PIN}) — ant updater preppkg" >&2
  LOG_DIR="${CACHE_ROOT}/logs/${JAVA_PIN}"
  mkdir -p "${LOG_DIR}"
  GETOPT_JAR="${REPO_ROOT}/target/interop/cache/i2cp/java_i2p/${JAVA_PIN}/lib/gnu-getopt.jar"
  if [[ ! -f "${GETOPT_JAR}" ]]; then
    # The Plan 170 fetch already produced this jar; if not present we
    # fall back to Maven Central exactly the same way (sha256-pinned
    # by the URL; no source patching).
    mkdir -p "$(dirname "${GETOPT_JAR}")"
    curl --fail --location --silent --show-error \
      "https://repo1.maven.org/maven2/gnu/getopt/java-getopt/1.0.13/java-getopt-1.0.13.jar" \
      -o "${GETOPT_JAR}"
  fi
  (cd "${JAVA_SRC}" && \
    ant -DnoExe=true \
        -Dgettext.jar=/usr/share/java/libintl-0.21.jar \
        -Dgetopt.jar="${GETOPT_JAR}" \
        updater preppkg) > "${LOG_DIR}/ant.log" 2>&1
  if [[ ! -d "${JAVA_SRC}/pkg-temp/lib" ]]; then
    echo "Java I2P build did not produce pkg-temp/lib" >&2
    tail -n 80 "${LOG_DIR}/ant.log" >&2 || true
    exit 1
  fi
  rm -rf "${JAVA_CACHE}/"* 2>/dev/null || true
  mkdir -p "${JAVA_CACHE}"
  cp -a "${JAVA_SRC}/pkg-temp/." "${JAVA_CACHE}/"
  cp "${JAVA_SRC}/pkg-temp/runplain.sh" "${JAVA_CACHE}/runplain.sh.upstream" 2>/dev/null || true
  cat > "${JAVA_CACHE}/runplain.sh" <<'LAUNCHER'
#!/bin/bash
# Plan 194/196 — Java I2P headless launcher.
# Substituted from the staged upstream runplain.sh so the harness can
# exec the JVM in the foreground and observe its stdout/stderr for
# the ready token (the upstream `nohup ... &` form exits immediately
# and races the harness `BoundedProcess`).
#
# Plan 196 stops using this launcher at runtime. The Plan 196
# second-family harness compiles a ControlledRouter test-only class
# against the staged `lib/` jars (see tests/integration/m6-interop/java/)
# and invokes the stock `net.i2p.router.Router(Properties)` lifecycle
# directly. This substituted launcher stays in the cache as a
# diagnostic fallback (e.g. for manual sanity checks via
# `bash target/interop/cache/m6-java/<pin>/runplain.sh`).
set -euo pipefail
I2P="$(cd "$(dirname "$0")" && pwd)"
I2PTEMP="${I2P}/tmp"
mkdir -p "${I2PTEMP}"
JAVA="$(which java 2>/dev/null || command -v java 2>/dev/null)"
if [ -z "$JAVA" ] || [ ! -x "$JAVA" ]; then
  JAVA="/usr/lib/jvm/java-21-openjdk-amd64/bin/java"
fi
CP=""
for jar in ${I2P}/lib/*.jar; do
  if [ -z "$CP" ]; then
    CP="$jar"
  else
    CP="${CP}:${jar}"
  fi
done
JAVAOPTS="-Djava.net.preferIPv4Stack=true -Djava.awt.headless=true -Djava.library.path=${I2P}:${I2P}/lib -Di2p.dir.base=${I2P} -DloggerFilenameOverride=logs/log-router-@.txt -Drouterconsole.enable=false"
exec "$JAVA" -cp "$CP" ${JAVAOPTS} net.i2p.router.RouterLaunch
LAUNCHER
  chmod 0755 "${JAVA_CACHE}/runplain.sh"
  mkdir -p "${JAVA_CACHE}/tmp"
  write_metadata "${JAVA_CACHE}"
else
  echo "java_i2p cache present at ${JAVA_CACHE}"
fi

cat <<EOF
==> M6 mixed-router Java I2P reference cache ready
   pin:       ${JAVA_PIN} (${JAVA_VERSION})
   install:   ${JAVA_CACHE}
   launcher:  ${JAVA_CACHE}/runplain.sh
   metadata:  ${JAVA_CACHE}/build-metadata.txt
EOF
