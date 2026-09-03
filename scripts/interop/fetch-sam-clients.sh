#!/usr/bin/env bash
# Plan 150 — fetch/build the pinned SAM external-client assets.
#
# Usage:
#   bash scripts/interop/fetch-sam-clients.sh [--rebuild]
#
# The optional source overrides are useful on a constrained host:
#   I2PR_LIBSAM3_SRC=/path/to/libsam3
#   I2PR_I2PSAM_SRC=/path/to/i2psam
#   I2PR_I2PLIB_SRC=/path/to/i2plib
#
# Every source directory is required to be a Git checkout at the exact
# revision below. A mismatch is a hard error. The cache is disposable and
# lives below target/interop; no third-party source is committed.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CACHE_ROOT="${REPO_ROOT}/target/interop/cache/sam"
WORK_ROOT="${REPO_ROOT}/target/interop/sam-sources"
REBUILD="${1:-}"

LIBSAM3_REPO="https://github.com/i2p/libsam3.git"
LIBSAM3_PIN="7d6e658798baec31394c5685f9583343cc00900b"
I2PSAM_REPO="https://github.com/i2p/i2psam.git"
I2PSAM_PIN="b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac"
I2PLIB_REPO="https://github.com/l-n-s/i2plib.git"
I2PLIB_PIN="6edf51cd5d21cc745aa7e23cb98c582144884fa8"

if [[ -n "${REBUILD}" && "${REBUILD}" != "--rebuild" ]]; then
  echo "usage: $0 [--rebuild]" >&2
  exit 64
fi

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

  if [[ -n "${source_override}" ]]; then
    : "${source}"
  elif [[ "${REBUILD}" == "--rebuild" ]]; then
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

write_source_metadata() {
  local cache="$1"
  local repository="$2"
  local pin="$3"
  printf '%s\n' "${pin}" > "${cache}/source-revision.txt"
  printf '%s\n' "${repository}" > "${cache}/source-repository.txt"
}

verify_cache_revision() {
  local name="$1"
  local cache="$2"
  local pin="$3"
  if [[ ! -f "${cache}/source-revision.txt" ]] ||
     [[ "$(<"${cache}/source-revision.txt")" != "${pin}" ]]; then
    echo "${name} cache has no verified source revision: ${cache}" >&2
    echo "run scripts/interop/fetch-sam-clients.sh --rebuild" >&2
    exit 1
  fi
}

# ---- libsam3 ---------------------------------------------------------------
LIBSAM3_CACHE="${CACHE_ROOT}/libsam3/${LIBSAM3_PIN}"
if [[ -n "${I2PR_LIBSAM3_SRC:-}" || "${REBUILD}" == "--rebuild" ||
      ! -f "${LIBSAM3_CACHE}/libsam3.a" || ! -f "${LIBSAM3_CACHE}/libsam3.h" ]]; then
  LIBSAM3_SRC="$(prepare_source libsam3 "${LIBSAM3_REPO}" "${LIBSAM3_PIN}" \
    "${I2PR_LIBSAM3_SRC:-}" "${WORK_ROOT}/libsam3-${LIBSAM3_PIN}")"
  mkdir -p "${LIBSAM3_CACHE}"
  make -C "${LIBSAM3_SRC}" build >/dev/null
  cp "${LIBSAM3_SRC}/libsam3.a" "${LIBSAM3_CACHE}/"
  cp "${LIBSAM3_SRC}/src/libsam3/libsam3.h" "${LIBSAM3_CACHE}/"
  write_source_metadata "${LIBSAM3_CACHE}" "${LIBSAM3_REPO}" "${LIBSAM3_PIN}"
  sha256sum "${LIBSAM3_CACHE}/libsam3.a" "${LIBSAM3_CACHE}/libsam3.h" \
    > "${LIBSAM3_CACHE}/build-sha256.txt"
else
  verify_cache_revision libsam3 "${LIBSAM3_CACHE}" "${LIBSAM3_PIN}"
  echo "libsam3 cache present at ${LIBSAM3_CACHE}"
fi

# ---- i2psam ----------------------------------------------------------------
I2PSAM_CACHE="${CACHE_ROOT}/i2psam/${I2PSAM_PIN}"
if [[ -n "${I2PR_I2PSAM_SRC:-}" || "${REBUILD}" == "--rebuild" ||
      ! -f "${I2PSAM_CACHE}/libi2psam.a" || ! -f "${I2PSAM_CACHE}/i2psam.h" ]]; then
  I2PSAM_SRC="$(prepare_source i2psam "${I2PSAM_REPO}" "${I2PSAM_PIN}" \
    "${I2PR_I2PSAM_SRC:-}" "${WORK_ROOT}/i2psam-${I2PSAM_PIN}")"
  mkdir -p "${I2PSAM_CACHE}"
  make -C "${I2PSAM_SRC}" >/dev/null
  cp "${I2PSAM_SRC}/libi2psam.a" "${I2PSAM_CACHE}/"
  cp "${I2PSAM_SRC}/i2psam.h" "${I2PSAM_CACHE}/"
  cp "${I2PSAM_SRC}/i2psam-c.h" "${I2PSAM_CACHE}/"
  cp "${I2PSAM_SRC}/compat.h" "${I2PSAM_CACHE}/"
  write_source_metadata "${I2PSAM_CACHE}" "${I2PSAM_REPO}" "${I2PSAM_PIN}"
  sha256sum "${I2PSAM_CACHE}/libi2psam.a" \
    "${I2PSAM_CACHE}/i2psam.h" "${I2PSAM_CACHE}/i2psam-c.h" \
    "${I2PSAM_CACHE}/compat.h" > "${I2PSAM_CACHE}/build-sha256.txt"
else
  verify_cache_revision i2psam "${I2PSAM_CACHE}" "${I2PSAM_PIN}"
  echo "i2psam cache present at ${I2PSAM_CACHE}"
fi

# ---- i2plib (explicit Plan 150 substitute for libsam3) --------------------
I2PLIB_CACHE="${CACHE_ROOT}/i2plib/${I2PLIB_PIN}"
if [[ -n "${I2PR_I2PLIB_SRC:-}" || "${REBUILD}" == "--rebuild" ||
      ! -f "${I2PLIB_CACHE}/i2plib/sam.py" ]]; then
  I2PLIB_SRC="$(prepare_source i2plib "${I2PLIB_REPO}" "${I2PLIB_PIN}" \
    "${I2PR_I2PLIB_SRC:-}" "${WORK_ROOT}/i2plib-${I2PLIB_PIN}")"
  mkdir -p "${I2PLIB_CACHE}"
  cp -R "${I2PLIB_SRC}/i2plib" "${I2PLIB_CACHE}/"
  write_source_metadata "${I2PLIB_CACHE}" "${I2PLIB_REPO}" "${I2PLIB_PIN}"
  sha256sum "${I2PLIB_CACHE}/i2plib/sam.py" \
    "${I2PLIB_CACHE}/i2plib/__init__.py" > "${I2PLIB_CACHE}/build-sha256.txt"
else
  verify_cache_revision i2plib "${I2PLIB_CACHE}" "${I2PLIB_PIN}"
  echo "i2plib cache present at ${I2PLIB_CACHE}"
fi

cat <<EOF
==> SAM external-client cache ready
   libsam3 pin: ${LIBSAM3_PIN}
   i2psam pin:  ${I2PSAM_PIN}
   i2plib pin:  ${I2PLIB_PIN}
   cache root:  ${CACHE_ROOT}
EOF
