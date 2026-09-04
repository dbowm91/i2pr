#!/usr/bin/env bash
# Plan 161 §4 — fetch/verify/build the exact-pinned SSU2 independent reference.
#
# Usage:
#   bash scripts/interop/fetch-ssu2-reference.sh [--rebuild] [--with-java]
#
# Mandatory reference (both direct UDP directions):
#   i2pd 2.61.0 at 635b013a612ff47278ef02acf8580a28e10e26c5 (PurpleI2P/i2pd)
#
# Optional secondary reference (nonblocking per Plan 161 §12):
#   Java I2P 2.13.0 at 9134f808337b401e8e53c73734c81fab04280c9d (i2p/i2p.i2p)
#
# The optional source override is useful on a constrained host:
#   I2PR_I2PD_SRC=/path/to/i2pd
#
# Every source directory must be a Git checkout whose HEAD equals the exact
# pin above. A mismatch is a hard error. The cache is disposable and lives
# below target/interop; no third-party source is committed and the external
# source is never patched to accommodate i2pr.
#
# The i2pd build uses the upstream GNU Makefile recipe
# (`make USE_UPNP=no DEBUG=0`) with a bounded job count. The exact build
# command, compiler/toolchain versions, artifact hash, and source revision
# are recorded under the cache directory.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CACHE_ROOT="${REPO_ROOT}/target/interop/cache/ssu2"
WORK_ROOT="${REPO_ROOT}/target/interop/ssu2-sources"
REBUILD=""
WITH_JAVA=""

for arg in "$@"; do
  case "${arg}" in
    --rebuild) REBUILD="--rebuild" ;;
    --with-java) WITH_JAVA="1" ;;
    *) echo "usage: $0 [--rebuild] [--with-java]" >&2; exit 64 ;;
  esac
done

I2PD_REPO="https://github.com/PurpleI2P/i2pd.git"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
JAVA_REPO="https://github.com/i2p/i2p.i2p.git"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION="2.13.0"

mkdir -p "${CACHE_ROOT}" "${WORK_ROOT}"

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

  if [[ -z "${source_override}" && "${REBUILD}" == "--rebuild" ]]; then
    git -C "${source}" checkout --detach --quiet "${pin}"
  fi

  local actual
  actual="$(git -C "${source}" rev-parse HEAD)"
  if [[ "${actual}" != "${pin}" ]]; then
    echo "${name} pin mismatch: expected ${pin}, got ${actual}" >&2
    echo "use --rebuild or provide a correctly pinned source override" >&2
    exit 1
  fi
  local remote
  remote="$(git -C "${source}" remote get-url origin 2>/dev/null || true)"
  if [[ -n "${source_override}" ]]; then
    echo "note: ${name} uses caller-provided checkout at verified pin ${pin}" >&2
  elif [[ "${remote}" != "${repository}" && "${remote}" != "${repository%.git}" ]]; then
    echo "${name} origin mismatch: expected ${repository}, got ${remote}" >&2
    exit 1
  fi
  printf '%s\n' "${source}"
}

write_source_metadata() {
  local cache="$1"
  local repository="$2"
  local pin="$3"
  local version="$4"
  local build_command="$5"
  printf '%s\n' "${pin}" > "${cache}/source-revision.txt"
  printf '%s\n' "${repository}" > "${cache}/source-repository.txt"
  printf '%s\n' "${version}" > "${cache}/source-version.txt"
  printf '%s\n' "${build_command}" > "${cache}/build-command.txt"
}

verify_cache_revision() {
  local name="$1"
  local cache="$2"
  local pin="$3"
  if [[ ! -f "${cache}/source-revision.txt" ]] ||
     [[ "$(<"${cache}/source-revision.txt")" != "${pin}" ]]; then
    echo "${name} cache has no verified source revision: ${cache}" >&2
    echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
    exit 1
  fi
}

# ---- i2pd (mandatory) ------------------------------------------------------
I2PD_CACHE="${CACHE_ROOT}/i2pd/${I2PD_PIN}"
I2PD_BUILD_COMMAND="make -j\$(nproc) USE_UPNP=no DEBUG=0"
if [[ -n "${I2PR_I2PD_SRC:-}" || "${REBUILD}" == "--rebuild" ||
      ! -x "${I2PD_CACHE}/bin/i2pd" ]]; then
  I2PD_SRC="$(prepare_source i2pd "${I2PD_REPO}" "${I2PD_PIN}" \
    "${I2PR_I2PD_SRC:-}" "${WORK_ROOT}/i2pd-${I2PD_PIN}")"
  # Refuse tracked modifications (patching). Untracked in-tree build
  # outputs from a prior `make` do not indicate patching.
  if [[ -n "$(git -C "${I2PD_SRC}" status --porcelain --untracked-files=no)" ]]; then
    echo "i2pd source tree is dirty; refusing to build a patched reference" >&2
    git -C "${I2PD_SRC}" status --porcelain --untracked-files=no >&2
    exit 1
  fi
  mkdir -p "${I2PD_CACHE}/bin" "${I2PD_CACHE}/logs"
  echo "==> building i2pd ${I2PD_VERSION} (${I2PD_PIN})" >&2
  cpus="$(nproc 2>/dev/null || printf '1')"
  memory_kb="$(awk '/MemTotal:/ {print $2}' /proc/meminfo 2>/dev/null || printf '16777216')"
  jobs="${cpus}"
  if [[ "${cpus}" -gt 4 ]]; then jobs=4; fi
  if [[ "${memory_kb}" =~ ^[0-9]+$ && "${memory_kb}" -lt 8388608 ]]; then jobs=1; fi
  if [[ "${jobs}" -lt 1 ]]; then jobs=1; fi
  (cd "${I2PD_SRC}" && make -j"${jobs}" USE_UPNP=no DEBUG=0) >"${I2PD_CACHE}/logs/build.log" 2>&1
  [[ -x "${I2PD_SRC}/i2pd" ]] || { echo "i2pd build did not produce an executable" >&2; exit 1; }
  install -m 0755 "${I2PD_SRC}/i2pd" "${I2PD_CACHE}/bin/i2pd"
  if ! "${I2PD_CACHE}/bin/i2pd" --version >"${I2PD_CACHE}/logs/version.txt" 2>&1; then
    echo "i2pd --version probe failed" >&2
    exit 1
  fi
  grep -Fq "${I2PD_VERSION}" "${I2PD_CACHE}/logs/version.txt" \
    || { echo "i2pd version does not report ${I2PD_VERSION}" >&2; exit 1; }
  write_source_metadata "${I2PD_CACHE}" "${I2PD_REPO}" "${I2PD_PIN}" \
    "${I2PD_VERSION}" "${I2PD_BUILD_COMMAND}"
  {
    printf 'toolchain=compiler:%s;make:%s;boost:%s;openssl:%s;zlib:%s\n' \
      "$(c++ --version | head -n 1)" "$(make --version | head -n 1)" \
      "$(sed -n 's/^#define BOOST_LIB_VERSION "\(.*\)"/\1/p' /usr/include/boost/version.hpp 2>/dev/null | head -n 1)" \
      "$(openssl version | head -n 1)" \
      "$(sed -n 's/^#define ZLIB_VERSION "\(.*\)"/\1/p' /usr/include/zlib.h 2>/dev/null | head -n 1)"
    sha256sum "${I2PD_CACHE}/bin/i2pd" | awk '{print "artifact_sha256=" $1}'
  } > "${I2PD_CACHE}/build-sha256.txt"
else
  verify_cache_revision i2pd "${I2PD_CACHE}" "${I2PD_PIN}"
  echo "i2pd cache present at ${I2PD_CACHE}"
fi

# ---- Java I2P (preferred secondary, nonblocking) ---------------------------
if [[ -n "${WITH_JAVA}" ]]; then
  JAVA_CACHE="${CACHE_ROOT}/java/${JAVA_PIN}"
  mkdir -p "${JAVA_CACHE}"
  JAVA_SRC="$(prepare_source java-i2p "${JAVA_REPO}" "${JAVA_PIN}" \
    "" "${WORK_ROOT}/i2p.i2p-${JAVA_PIN}")"
  write_source_metadata "${JAVA_CACHE}" "${JAVA_REPO}" "${JAVA_PIN}" \
    "${JAVA_VERSION}" "source-verified-only; router build deferred per Plan 161 section 12"
  printf 'java source verified at %s (build deferred; see plans/161-status.md)\n' "${JAVA_PIN}"
fi

cat <<EOF
==> SSU2 independent-reference cache ready
   i2pd pin: ${I2PD_PIN} (${I2PD_VERSION})
   i2pd binary: ${I2PD_CACHE}/bin/i2pd
   java pin: ${JAVA_PIN} (${JAVA_VERSION}) [source only with --with-java]
   cache root: ${CACHE_ROOT}
EOF
