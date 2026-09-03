#!/usr/bin/env bash
# Plan 150 — fetch + build the libsam3 + i2psam external-client
# assets into the interop cache. The script pins exact upstream
# revisions and verifies them against git rev-parse; a wrong
# commit is a hard error.
#
# Usage: bash scripts/interop/fetch-sam-clients.sh [--rebuild]
#
# No network is strictly required when the cache is populated; the
# script only reaches the network when a cache entry is missing.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
CACHE_ROOT="${REPO_ROOT}/target/interop/cache/sam"
WORK_ROOT="${REPO_ROOT}/target/interop/sam-sources"

LIBSAM3_REPO="https://github.com/i2p/libsam3.git"
LIBSAM3_PIN="7d6e658798baec31394c5685f9583343cc00900b"

I2PSAM_REPO="https://github.com/i2p/i2psam.git"
I2PSAM_PIN="b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac"

I2PLIB_REPO="https://github.com/l-n-s/i2plib.git"
I2PLIB_PIN="6edf51cd5d21cc745aa7e23cb98c582144884fa8"

mkdir -p "${CACHE_ROOT}" "${WORK_ROOT}"

# ---- libsam3 ----------------------------------------------------------------
LIBSAM3_CACHE="${CACHE_ROOT}/libsam3/${LIBSAM3_PIN}"
if [[ -f "${LIBSAM3_CACHE}/libsam3.a" ]] && [[ -f "${LIBSAM3_CACHE}/libsam3.h" ]] \
   && [[ "${1:-}" != "--rebuild" ]]; then
  echo "libsam3 cache present at ${LIBSAM3_CACHE}"
else
  LIBSAM3_SRC="${WORK_ROOT}/libsam3-${LIBSAM3_PIN}"
  if [[ ! -d "${LIBSAM3_SRC}/.git" ]]; then
    git clone "${LIBSAM3_REPO}" "${LIBSAM3_SRC}"
  fi
  git -C "${LIBSAM3_SRC}" fetch --tags --force origin
  git -C "${LIBSAM3_SRC}" checkout --detach "${LIBSAM3_PIN}"
  actual="$(git -C "${LIBSAM3_SRC}" rev-parse HEAD)"
  if [[ "${actual}" != "${LIBSAM3_PIN}" ]]; then
    echo "libsam3 pin mismatch: expected ${LIBSAM3_PIN}, got ${actual}" >&2
    exit 1
  fi
  mkdir -p "${LIBSAM3_CACHE}"
  make -C "${LIBSAM3_SRC}" build >/dev/null
  cp "${LIBSAM3_SRC}/libsam3.a" "${LIBSAM3_CACHE}/"
  cp "${LIBSAM3_SRC}/src/libsam3/libsam3.h" "${LIBSAM3_CACHE}/"
  sha256sum "${LIBSAM3_CACHE}/libsam3.a" "${LIBSAM3_CACHE}/libsam3.h" \
    > "${LIBSAM3_CACHE}/build-sha256.txt"
fi

# ---- i2psam -----------------------------------------------------------------
I2PSAM_CACHE="${CACHE_ROOT}/i2psam/${I2PSAM_PIN}"
if [[ -f "${I2PSAM_CACHE}/libi2psam.a" ]] && [[ -f "${I2PSAM_CACHE}/i2psam.h" ]] \
   && [[ "${1:-}" != "--rebuild" ]]; then
  echo "i2psam cache present at ${I2PSAM_CACHE}"
else
  I2PSAM_SRC="${WORK_ROOT}/i2psam-${I2PSAM_PIN}"
  if [[ ! -d "${I2PSAM_SRC}/.git" ]]; then
    git clone "${I2PSAM_REPO}" "${I2PSAM_SRC}"
  fi
  git -C "${I2PSAM_SRC}" fetch --tags --force origin
  git -C "${I2PSAM_SRC}" checkout --detach "${I2PSAM_PIN}"
  actual="$(git -C "${I2PSAM_SRC}" rev-parse HEAD)"
  if [[ "${actual}" != "${I2PSAM_PIN}" ]]; then
    echo "i2psam pin mismatch: expected ${I2PSAM_PIN}, got ${actual}" >&2
    exit 1
  fi
  mkdir -p "${I2PSAM_CACHE}"
  make -C "${I2PSAM_SRC}" >/dev/null
  cp "${I2PSAM_SRC}/libi2psam.a" "${I2PSAM_CACHE}/"
  cp "${I2PSAM_SRC}/i2psam.h" "${I2PSAM_CACHE}/"
  cp "${I2PSAM_SRC}/i2psam-c.h" "${I2PSAM_CACHE}/"
  cp "${I2PSAM_SRC}/compat.h" "${I2PSAM_CACHE}/"
  sha256sum "${I2PSAM_CACHE}/libi2psam.a" \
              "${I2PSAM_CACHE}/i2psam.h" \
              "${I2PSAM_CACHE}/i2psam-c.h" \
              "${I2PSAM_CACHE}/compat.h" \
    > "${I2PSAM_CACHE}/build-sha256.txt"
fi

# ---- i2plib -----------------------------------------------------------------
# i2plib is the Plan 150 §6 substitute for libsam3 (which is out-of-spec
# for SAM 3.1 Ed25519: SAM3_PRIVKEY_MIN_SIZE=884 rejects i2pr's 608-char
# PRIV). i2plib's `sam.py` is a pure-Python SAM 3.1 message/Base64 surface
# from a separate codebase; the runner owns the socket lifecycle.
I2PLIB_CACHE="${CACHE_ROOT}/i2plib/${I2PLIB_PIN}"
if [[ -f "${I2PLIB_CACHE}/i2plib/sam.py" ]] \
   && [[ "${1:-}" != "--rebuild" ]]; then
  echo "i2plib cache present at ${I2PLIB_CACHE}"
else
  I2PLIB_SRC="${WORK_ROOT}/i2plib-${I2PLIB_PIN}"
  if [[ ! -d "${I2PLIB_SRC}/.git" ]]; then
    git clone "${I2PLIB_REPO}" "${I2PLIB_SRC}"
  fi
  git -C "${I2PLIB_SRC}" fetch --tags --force origin
  git -C "${I2PLIB_SRC}" checkout --detach "${I2PLIB_PIN}"
  actual="$(git -C "${I2PLIB_SRC}" rev-parse HEAD)"
  if [[ "${actual}" != "${I2PLIB_PIN}" ]]; then
    echo "i2plib pin mismatch: expected ${I2PLIB_PIN}, got ${actual}" >&2
    exit 1
  fi
  mkdir -p "${I2PLIB_CACHE}"
  cp -r "${I2PLIB_SRC}/i2plib" "${I2PLIB_CACHE}/i2plib"
  (cd "${I2PLIB_CACHE}/i2plib" && \
     find . -name __pycache__ -type d -exec rm -rf {} + 2>/dev/null || true)
  sha256sum "${I2PLIB_CACHE}/i2plib/sam.py" \
              "${I2PLIB_CACHE}/i2plib/__init__.py" \
    > "${I2PLIB_CACHE}/build-sha256.txt"
fi

cat <<EOF
==> SAM external-client cache ready
   libsam3 pin: ${LIBSAM3_PIN}
   i2psam pin:  ${I2PSAM_PIN}
   i2plib pin:  ${I2PLIB_PIN}
   cache root:  ${CACHE_ROOT}
EOF
