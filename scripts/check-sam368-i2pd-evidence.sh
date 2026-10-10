#!/usr/bin/env bash
# Verifies sanitized, exact-pinned Plan 368 i2pd diagnostic evidence.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EVIDENCE_DIR="${I2PR_SAM_368_I2PD_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/sam-368-i2pd-evidence}"
EVIDENCE="${EVIDENCE_DIR}/evidence.tsv"

if [[ ! -f "${EVIDENCE}" ]]; then
  echo "Plan 368 i2pd evidence is missing: ${EVIDENCE}" >&2
  exit 1
fi
grep -Fxq $'i2pd_version\t2.61.0' "${EVIDENCE}" || {
  echo "Plan 368 i2pd evidence has the wrong version" >&2
  exit 1
}
grep -Fxq $'i2pd_revision\t635b013a612ff47278ef02acf8580a28e10e26c5' "${EVIDENCE}" || {
  echo "Plan 368 i2pd evidence has the wrong source revision" >&2
  exit 1
}
grep -Fxq $'bind_scope\tloopback_ntcp2_no_reseed_no_addressbook_peers_empty' "${EVIDENCE}" || {
  echo "Plan 368 i2pd evidence does not prove the constrained loopback profile" >&2
  exit 1
}
for row in \
  $'hello\tversion_3_3' \
  $'primary_create\tunsupported' \
  $'primary_master_spelling\tmaster_only_source' \
  $'session_add\tstream_only_source' \
  $'session_remove\tmaster_only_source' \
  $'datagram_styles\tordinary_17_19_20_source' \
  $'raw_style\tordinary_raw_source'; do
  grep -Fxq "${row}" "${EVIDENCE}" || {
    echo "Plan 368 i2pd evidence is missing expected diagnostic row: ${row//$'\t'/ }" >&2
    exit 1
  }
done
if grep -Eq 'DESTINATION=|PRIV=|PUB=|payload|[A-Za-z0-9_-]{300,}' "${EVIDENCE}"; then
  echo "Plan 368 i2pd evidence may contain secret or payload material" >&2
  exit 1
fi

echo "Plan 368 i2pd diagnostic evidence: exact 2.61.0 pin and bounded loopback rows passed"
