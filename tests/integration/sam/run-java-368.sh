#!/usr/bin/env bash
# Plan 368 live Java SAM 3.3 client qualification against the staged i2pr
# profile. Java I2P is normative for this lane; i2pd is never consulted.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION="2.13.0"
JAVA_CACHE="${REPO_ROOT}/target/interop/cache/m6-java/${JAVA_PIN}"
JAVA_SOURCE_ROOT="${I2PR_SAM_368_JAVA_SOURCE_ROOT:-${REPO_ROOT}/target/interop/m6-java-sources/i2p.i2p-${JAVA_PIN}}"
CLIENT_DIR="${REPO_ROOT}/target/interop/sam-368-java-client/${JAVA_PIN}"
EVIDENCE_DIR="${I2PR_SAM_368_JAVA_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/sam-368-java-evidence}"
LOG="${EVIDENCE_DIR}/java-sam33-primary.log"

mkdir -p "${CLIENT_DIR}" "${EVIDENCE_DIR}"
rm -f "${LOG}" "${EVIDENCE_DIR}/evidence.tsv"

if [[ ! -f "${JAVA_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${JAVA_CACHE}/source-revision.txt")" != "${JAVA_PIN}" ]] ||
   [[ "$(<"${JAVA_CACHE}/source-version.txt")" != "${JAVA_VERSION}" ]]; then
  echo "exact-pinned Java I2P ${JAVA_VERSION} cache missing or mismatched" >&2
  echo "run scripts/interop/fetch-m6-java.sh --rebuild first" >&2
  exit 1
fi
if [[ ! -d "${JAVA_SOURCE_ROOT}/.git" ]] ||
   [[ "$(git -C "${JAVA_SOURCE_ROOT}" rev-parse HEAD)" != "${JAVA_PIN}" ]]; then
  echo "Java source checkout is not the pinned revision ${JAVA_PIN}" >&2
  exit 1
fi

SAM_CLIENT_SRC="${JAVA_SOURCE_ROOT}/apps/sam/java/src/net/i2p/sam/client/SAMStreamSink.java"
PRIMARY_SESSION_SRC="${JAVA_SOURCE_ROOT}/apps/sam/java/src/net/i2p/sam/PrimarySession.java"
if [[ ! -f "${SAM_CLIENT_SRC}" ]]; then
  echo "pinned Java SAM client source missing: ${SAM_CLIENT_SRC}" >&2
  exit 1
fi
if [[ ! -f "${PRIMARY_SESSION_SRC}" ]] ||
   ! grep -Fq 'style.equals("DATAGRAM2")' "${PRIMARY_SESSION_SRC}" ||
   ! grep -Fq 'style.equals("DATAGRAM3")' "${PRIMARY_SESSION_SRC}"; then
  echo "pinned Java PRIMARY handler no longer contains the expected DATAGRAM2/3 profile" >&2
  exit 1
fi

echo "==> compile exact-pinned Java I2P SAM client ${JAVA_VERSION} (${JAVA_PIN})"
javac \
  -classpath "${JAVA_CACHE}/lib/*" \
  -sourcepath "${JAVA_SOURCE_ROOT}/apps/sam/java/src" \
  -d "${CLIENT_DIR}" \
  "${SAM_CLIENT_SRC}" \
  "${REPO_ROOT}/tests/integration/sam/java/Sam368PrimaryProbe.java"

echo "==> build managed-app sibling executables for focused daemon qualification"
cargo build --locked -p i2pr-app-fixture -p i2pr-apphost -p i2pr-appd -p i2pr-appctl

classpath="${CLIENT_DIR}:${JAVA_CACHE}/lib/*"
test_rc=0
I2PR_SAM_368_JAVA_CLIENT_CLASSPATH="${classpath}" \
  cargo test --locked -p i2pr-daemon --lib \
    sam::tests::pinned_java_213 \
    -- --ignored --test-threads=1 >"${LOG}" 2>&1 || test_rc=$?

if [[ "${test_rc}" -eq 0 ]] &&
   grep -Fq 'test sam::tests::pinned_java_213_client_uses_sam33_primary_and_receives_datagram ... ok' "${LOG}" &&
   grep -Fq 'test sam::tests::pinned_java_213_primary_datagrams_and_stream_share_one_destination ... ok' "${LOG}"; then
  printf 'java_version\t%s\njava_revision\t%s\njava_client\tSAMStreamSink plus Java SAM wire probe\njava_test\tsam::tests::pinned_java_213_client_uses_sam33_primary_and_receives_datagram\njava_test\tsam::tests::pinned_java_213_primary_datagrams_and_stream_share_one_destination\njava_sam33_primary\tpassed\njava_primary_child_matrix\tpassed\njava_primary_datagram_17_19_20_receive\tpassed\njava_primary_stream_roundtrip\tpassed\n' \
    "${JAVA_VERSION}" "${JAVA_PIN}" >"${EVIDENCE_DIR}/evidence.tsv"
  cat "${EVIDENCE_DIR}/evidence.tsv"
else
  printf 'java_version\t%s\njava_revision\t%s\njava_client\tSAMStreamSink plus Java SAM wire probe\njava_test\tsam::tests::pinned_java_213_client_uses_sam33_primary_and_receives_datagram\njava_test\tsam::tests::pinned_java_213_primary_datagrams_and_stream_share_one_destination\njava_sam33_primary\tfailed\njava_primary_child_matrix\tfailed\njava_primary_datagram_17_19_20_receive\tfailed\njava_primary_stream_roundtrip\tfailed\n' \
    "${JAVA_VERSION}" "${JAVA_PIN}" >"${EVIDENCE_DIR}/evidence.tsv"
  echo "Plan 368 Java SAM 3.3 qualification failed; see sanitized test output: ${LOG}" >&2
  tail -n 80 "${LOG}" >&2
  if [[ "${test_rc}" -eq 0 ]]; then
    exit 1
  fi
  exit "${test_rc}"
fi
