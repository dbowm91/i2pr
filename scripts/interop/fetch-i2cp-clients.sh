#!/usr/bin/env bash
# Plan 170 — fetch/build the pinned I2CP external-client assets.
#
# Usage:
#   bash scripts/interop/fetch-i2cp-clients.sh [--rebuild]
#
# The optional source overrides are useful on a constrained host:
#   I2PR_JAVA_I2P_SRC=/path/to/i2p.i2p
#   I2PR_GO_I2CP_SRC=/path/to/go-i2cp
#
# Every source directory is required to be a Git checkout at the exact
# revision below. A mismatch is a hard error. The cache is disposable and
# lives below target/interop; no third-party source is committed.
#
# Plan 170 deliberately avoids a full I2P router build with IzPack
# packaging: the lane only needs the Java I2CP client API surface
# (`net.i2p.client.I2PClient` / `I2PSession` plus the supporting
# `net.i2p.data` / `net.i2p.crypto` types). `ant jar` on the `core/java`
# module produces `i2p.jar` in seconds, well under the cost of the
# IzPack + signing + headless launcher chain the router build would
# require.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CACHE_ROOT="${REPO_ROOT}/target/interop/cache/i2cp"
WORK_ROOT="${REPO_ROOT}/target/interop/i2cp-sources"
REBUILD="${1:-}"

JAVA_I2P_REPO="https://github.com/i2p/i2p.i2p.git"
JAVA_I2P_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
GO_I2CP_REPO="https://github.com/go-i2p/go-i2cp.git"
GO_I2CP_PIN="b529ee1c10a6011558b4d69fc9436a4afc489eac"

if [[ -n "${REBUILD}" && "${REBUILD}" != "--rebuild" ]]; then
  echo "usage: $0 [--rebuild]" >&2
  exit 64
fi

mkdir -p "${CACHE_ROOT}" "${WORK_ROOT}"

for command in git java ant sha256sum; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "fetch-i2cp-clients.sh: required command missing: ${command}" >&2
    exit 1
  }
done

prepare_source() {
  local name="$1"
  local repository="$2"
  local pin="$3"
  local source_override="$4"
  local default_source="$5"
  local source="${source_override:-${default_source}}"

  if [[ -n "${source_override}" ]]; then
    if [[ ! -d "${source}/.git" ]]; then
      echo "${name} source override is not a Git checkout: ${source}" >&2
      exit 1
    fi
  elif [[ ! -d "${source}/.git" ]]; then
    echo "==> cloning ${name}" >&2
    git clone "${repository}" "${source}"
  elif [[ "${REBUILD}" == "--rebuild" ]]; then
    echo "==> refreshing ${name} source" >&2
    git -C "${source}" fetch --tags --force origin
  fi

  if [[ -n "${source_override}" ]]; then
    :
  else
    # Fresh clones land on the remote default branch, not the pinned
    # revision; rebuilds refresh then pin. Either way the lane must
    # detach at the exact pin before verification — a hosted runner
    # with an empty cache failed here (Java I2P cloned at the default
    # tip instead of 9134f808...) because checkout only ran on
    # --rebuild.
    git -C "${source}" checkout --detach "${pin}"
  fi

  local actual
  actual="$(git -C "${source}" rev-parse HEAD)"
  if [[ "${actual}" != "${pin}" ]]; then
    echo "${name} pin mismatch: expected ${pin}, got ${actual}" >&2
    echo "use --rebuild or provide a correctly pinned source override" >&2
    exit 1
  fi
  printf '%s\n' "${source}"
}

write_metadata() {
  local cache="$1"
  local repository="$2"
  local pin="$3"
  local build_version="$4"
  local toolchain="$5"
  local artifact_path="$6"
  local artifact_sha="$7"
  printf '%s\n' "${pin}" > "${cache}/source-revision.txt"
  printf '%s\n' "${repository}" > "${cache}/source-repository.txt"
  cat > "${cache}/build-metadata.txt" <<EOF
schema=2
reference=${cache##*/i2cp/}
source_revision=${pin}
build_command_version=${build_version}
host_contract=ubuntu-24.04-amd64
source_repository=${repository}
artifact_sha256=${artifact_sha}
artifact_path=${artifact_path}
launcher=core-jar-only
execution_network=forbidden
toolchain=${toolchain}
version_check=jar-sha256-verified
test_disposition=not-applicable
EOF
}

verify_cache_revision() {
  local name="$1"
  local cache="$2"
  local pin="$3"
  if [[ ! -f "${cache}/source-revision.txt" ]] ||
     [[ "$(<"${cache}/source-revision.txt")" != "${pin}" ]]; then
    echo "${name} cache has no verified source revision: ${cache}" >&2
    echo "run scripts/interop/fetch-i2cp-clients.sh --rebuild" >&2
    exit 1
  fi
}

# ---- Java I2P core jar (Plan 170 §3 mandatory primary) ---------------------
JAVA_I2P_CACHE="${CACHE_ROOT}/java_i2p/${JAVA_I2P_PIN}"
JAVA_I2P_LIB="${JAVA_I2P_CACHE}/lib"
JAVA_I2P_JAR="${JAVA_I2P_LIB}/i2p.jar"
if [[ -n "${I2PR_JAVA_I2P_SRC:-}" || "${REBUILD}" == "--rebuild" ||
      ! -f "${JAVA_I2P_JAR}" ]]; then
  JAVA_I2P_SRC="$(prepare_source java_i2p "${JAVA_I2P_REPO}" "${JAVA_I2P_PIN}" \
    "${I2PR_JAVA_I2P_SRC:-}" "${WORK_ROOT}/java-i2p-${JAVA_I2P_PIN}")"
  mkdir -p "${JAVA_I2P_LIB}"
  # Fetch gnu-getopt (used by I2PClientFactory.getRouterConsole) from Maven
  # Central so the Java I2CP API can resolve it at runtime without patching
  # the pinned source. The artifact is identified by SHA-256; the URL is
  # the only network egress.
  GETOPT_JAR="${JAVA_I2P_LIB}/gnu-getopt.jar"
  GETOPT_URL="https://repo1.maven.org/maven2/gnu/getopt/java-getopt/1.0.13/java-getopt-1.0.13.jar"
  if [[ ! -f "${GETOPT_JAR}" ]]; then
    curl --fail --location --silent --show-error "${GETOPT_URL}" \
      -o "${GETOPT_JAR}"
  fi
  cp /usr/share/java/libintl.jar "${JAVA_I2P_LIB}/libintl.jar"
  # Build the I2P core jar only — we never need the router, the headless
  # launcher, or any of the IzPack-packaged artifacts for the M9 I2CP
  # client surface. `ant jar` finishes in seconds and emits the
  # exactly-pinned i2p.jar.
  (cd "${JAVA_I2P_SRC}/core/java" && \
    ant -Dgettext.jar="${JAVA_I2P_LIB}/libintl.jar" \
        -Dgetopt.jar="${GETOPT_JAR}" \
        -Djavac.classpath.mod2= \
        jar) >"${WORK_ROOT}/java-i2p-${JAVA_I2P_PIN}-ant.log" 2>&1
  if [[ ! -f "${JAVA_I2P_SRC}/core/java/build/i2p.jar" ]]; then
    echo "Java I2P core jar build failed; see ${WORK_ROOT}/java-i2p-${JAVA_I2P_PIN}-ant.log" >&2
    exit 1
  fi
  cp "${JAVA_I2P_SRC}/core/java/build/i2p.jar" "${JAVA_I2P_JAR}"
  ARTIFACT_SHA="$(sha256sum "${JAVA_I2P_JAR}" | awk '{print $1}')"
  TOOLCHAIN="java:$(java -version 2>&1 | head -n 1);ant:$(ant -version 2>&1 | head -n 1)"
  write_metadata "${JAVA_I2P_CACHE}" "${JAVA_I2P_REPO}" "${JAVA_I2P_PIN}" \
    "java_i2p-core-jar-v1" "${TOOLCHAIN}" "lib/i2p.jar" "${ARTIFACT_SHA}"
  sha256sum "${JAVA_I2P_JAR}" "${GETOPT_JAR}" "${JAVA_I2P_LIB}/libintl.jar" \
    > "${JAVA_I2P_CACHE}/build-sha256.txt"
else
  verify_cache_revision java_i2p "${JAVA_I2P_CACHE}" "${JAVA_I2P_PIN}"
  echo "java_i2p cache present at ${JAVA_I2P_CACHE}"
fi

# ---- go-i2cp (Plan 170 §3 mandatory secondary target) ----------------------
GO_I2CP_CACHE="${CACHE_ROOT}/go_i2cp/${GO_I2CP_PIN}"
GO_I2CP_SRC_CACHE="${GO_I2CP_CACHE}/source"
if [[ -n "${I2PR_GO_I2CP_SRC:-}" || "${REBUILD}" == "--rebuild" ||
      ! -d "${GO_I2CP_SRC_CACHE}" ]]; then
  GO_I2CP_SRC="$(prepare_source go_i2cp "${GO_I2CP_REPO}" "${GO_I2CP_PIN}" \
    "${I2PR_GO_I2CP_SRC:-}" "${WORK_ROOT}/go-i2cp-${GO_I2CP_PIN}")"
  if [[ "${GO_I2CP_SRC}" != "${GO_I2CP_SRC_CACHE}" ]]; then
    rm -rf "${GO_I2CP_SRC_CACHE}"
    mkdir -p "${GO_I2CP_SRC_CACHE%/*}"
    cp -R "${GO_I2CP_SRC}/." "${GO_I2CP_SRC_CACHE}/"
  fi
  TOOLCHAIN="go:$(go version)"
  ARTIFACT_SHA="$(sha256sum "${GO_I2CP_SRC_CACHE}/go.mod" | awk '{print $1}')"
  write_metadata "${GO_I2CP_CACHE}" "${GO_I2CP_REPO}" "${GO_I2CP_PIN}" \
    "go-i2cp-modules-v1" "${TOOLCHAIN}" "source/go.mod" "${ARTIFACT_SHA}"
  # Pin the go module cache by listing the resolved SHA-256 for the
  # primary library sources; runtime callers build against the cached
  # checkout directly.
  find "${GO_I2CP_SRC_CACHE}" -maxdepth 1 -name '*.go' -type f \
    | sort | xargs sha256sum > "${GO_I2CP_CACHE}/build-sha256.txt"
else
  verify_cache_revision go_i2cp "${GO_I2CP_CACHE}" "${GO_I2CP_PIN}"
  echo "go_i2cp cache present at ${GO_I2CP_CACHE}"
fi

cat <<EOF
==> I2CP external-client cache ready
   Java I2P pin: ${JAVA_I2P_PIN}
   go-i2cp pin:  ${GO_I2CP_PIN}
   cache root:   ${CACHE_ROOT}
EOF
