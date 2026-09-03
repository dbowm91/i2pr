#!/usr/bin/env bash
# Plan 150 — build the libsam3 + i2psam runners against the cached
# libraries. The runners are harness code; they MUST NOT link any
# i2pr crate.
#
# Usage: bash tests/integration/sam/clients/build.sh
# Outputs land in tests/integration/sam/build/.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../../.." && pwd)"
CACHE_ROOT="${REPO_ROOT}/target/interop/cache/sam"
BUILD_ROOT="${REPO_ROOT}/tests/integration/sam/build"

mkdir -p "${BUILD_ROOT}"

LIBSAM3_PIN="7d6e658798baec31394c5685f9583343cc00900b"
I2PSAM_PIN="b80ecd487f7b8d1a743a1f40337b2eb0caaae6ac"

LIBSAM3_CACHE="${CACHE_ROOT}/libsam3/${LIBSAM3_PIN}"
I2PSAM_CACHE="${CACHE_ROOT}/i2psam/${I2PSAM_PIN}"

if [[ ! -f "${LIBSAM3_CACHE}/libsam3.a" ]] || [[ ! -f "${LIBSAM3_CACHE}/libsam3.h" ]]; then
  echo "missing libsam3 cache: ${LIBSAM3_CACHE}" >&2
  echo "run scripts/interop/fetch-sam-clients.sh first" >&2
  exit 1
fi

if [[ ! -f "${I2PSAM_CACHE}/libi2psam.a" ]] || [[ ! -f "${I2PSAM_CACHE}/i2psam.h" ]]; then
  echo "missing i2psam cache: ${I2PSAM_CACHE}" >&2
  echo "run scripts/interop/fetch-sam-clients.sh first" >&2
  exit 1
fi

CC="${CC:-cc}"
CXX="${CXX:-c++}"
CFLAGS="${CFLAGS:--Wall -O2 -std=gnu11}"
CXXFLAGS="${CXXFLAGS:--Wall -Wextra -Wno-unused-parameter -std=c++11}"

echo "==> compiling libsam3_runner"
${CC} ${CFLAGS} -I"${LIBSAM3_CACHE}" \
  "${REPO_ROOT}/tests/integration/sam/clients/libsam3_runner.c" \
  "${LIBSAM3_CACHE}/libsam3.a" -lpthread \
  -o "${BUILD_ROOT}/libsam3_runner"

echo "==> compiling libsam3_forward_runner"
${CC} ${CFLAGS} -I"${LIBSAM3_CACHE}" \
  "${REPO_ROOT}/tests/integration/sam/clients/libsam3_forward_runner.c" \
  "${LIBSAM3_CACHE}/libsam3.a" -lpthread \
  -o "${BUILD_ROOT}/libsam3_forward_runner"

echo "==> compiling i2psam_runner"
${CXX} ${CXXFLAGS} -I"${I2PSAM_CACHE}" \
  "${REPO_ROOT}/tests/integration/sam/clients/i2psam_runner.cpp" \
  "${I2PSAM_CACHE}/libi2psam.a" -lpthread \
  -o "${BUILD_ROOT}/i2psam_runner"

echo "==> compiling i2psam_forward_runner"
${CXX} ${CXXFLAGS} -I"${I2PSAM_CACHE}" \
  "${REPO_ROOT}/tests/integration/sam/clients/i2psam_forward_runner.cpp" \
  "${I2PSAM_CACHE}/libi2psam.a" -lpthread \
  -o "${BUILD_ROOT}/i2psam_forward_runner"

echo "==> transcript.py and echo_target.py are pure-python; nothing to link"
cp "${REPO_ROOT}/tests/integration/sam/clients/transcript.py" "${BUILD_ROOT}/transcript.py"
cp "${REPO_ROOT}/tests/integration/sam/clients/echo_target.py" "${BUILD_ROOT}/echo_target.py"
chmod +x "${BUILD_ROOT}/transcript.py" "${BUILD_ROOT}/echo_target.py"

echo "==> build complete"
ls -la "${BUILD_ROOT}"
