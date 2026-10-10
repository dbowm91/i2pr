#!/usr/bin/env bash
# Verifies the sanitized result record emitted by the Plan 368 Java lane.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EVIDENCE_DIR="${I2PR_SAM_368_JAVA_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/sam-368-java-evidence}"
EVIDENCE="${EVIDENCE_DIR}/evidence.tsv"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
TEST_NAME="sam::tests::pinned_java_213_client_uses_sam33_primary_and_receives_datagram"
CHILD_TEST_NAME="sam::tests::pinned_java_213_primary_datagrams_and_stream_share_one_destination"

if [[ ! -f "${EVIDENCE}" ]]; then
  echo "Plan 368 Java evidence is missing: ${EVIDENCE}" >&2
  exit 1
fi
grep -Fxq $'java_version\t2.13.0' "${EVIDENCE}" || {
  echo "Plan 368 Java evidence has the wrong Java version" >&2
  exit 1
}
grep -Fxq $'java_revision\t'"${JAVA_PIN}" "${EVIDENCE}" || {
  echo "Plan 368 Java evidence has the wrong source revision" >&2
  exit 1
}
grep -Fxq $'java_client\tSAMStreamSink plus Java SAM wire probe' "${EVIDENCE}" || {
  echo "Plan 368 Java evidence is missing the reference SAM client" >&2
  exit 1
}
grep -Fxq $'java_test\t'"${TEST_NAME}" "${EVIDENCE}" || {
  echo "Plan 368 Java evidence names the wrong test" >&2
  exit 1
}
grep -Fxq $'java_test\t'"${CHILD_TEST_NAME}" "${EVIDENCE}" || {
  echo "Plan 368 Java evidence is missing the full child-style matrix test" >&2
  exit 1
}
grep -Fxq $'java_sam33_primary\tpassed' "${EVIDENCE}" || {
  echo "Plan 368 Java SAM 3.3 primary row did not pass" >&2
  exit 1
}
grep -Fxq $'java_primary_child_matrix\tpassed' "${EVIDENCE}" || {
  echo "Plan 368 Java PRIMARY child-style matrix did not pass" >&2
  exit 1
}
grep -Fxq $'java_primary_datagram_17_19_20_receive\tpassed' "${EVIDENCE}" || {
  echo "Plan 368 Java DATAGRAM1/2/3 receive traffic did not pass" >&2
  exit 1
}
grep -Fxq $'java_primary_stream_roundtrip\tpassed' "${EVIDENCE}" || {
  echo "Plan 368 Java PRIMARY STREAM roundtrip did not pass" >&2
  exit 1
}

echo "Plan 368 Java evidence: pinned Java 2.13.0 SAMStreamSink and PRIMARY child matrix passed"
