#!/usr/bin/env bash
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
RUN_EVIDENCE_DIR="${I2PR_M6_STREAMING_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/m6-streaming-evidence}"
EVIDENCE_DIR="${I2PR_PLAN312_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/anonymity/plan312-streaming}"

mkdir -p "${EVIDENCE_DIR}"
rm -f "${EVIDENCE_DIR}/fingerprint-i2pr-client.tsv" \
  "${EVIDENCE_DIR}/fingerprint-i2pd-client.tsv" \
  "${EVIDENCE_DIR}/fingerprint-i2pr-server.tsv" \
  "${EVIDENCE_DIR}/fingerprint-i2pd-server.tsv" \
  "${EVIDENCE_DIR}/fingerprint-manifest.tsv" \
  "${EVIDENCE_DIR}/fingerprint-matrix.tsv" \
  "${EVIDENCE_DIR}/evidence.json"

I2PR_M6_STREAMING_EVIDENCE_DIR="${RUN_EVIDENCE_DIR}" \
  bash "${REPO_ROOT}/tests/integration/m6-interop/run-streaming.sh"

DRIVER_EVIDENCE="${RUN_EVIDENCE_DIR}/driver"
for role in i2pr-client i2pd-client i2pr-server i2pd-server; do
  cp "${DRIVER_EVIDENCE}/fingerprint-${role}.tsv" "${EVIDENCE_DIR}/"
done
cp "${DRIVER_EVIDENCE}/fingerprint-manifest.tsv" "${EVIDENCE_DIR}/"

bash "${REPO_ROOT}/scripts/check-streaming-fingerprint-evidence.sh" "${EVIDENCE_DIR}"

python3 - "${EVIDENCE_DIR}" "${REPO_ROOT}" <<'PY'
import hashlib
import json
import subprocess
import sys
from pathlib import Path

evidence_dir = Path(sys.argv[1])
repo_root = Path(sys.argv[2])
commit = subprocess.check_output(
    ["git", "-C", str(repo_root), "rev-parse", "HEAD"], text=True
).strip()
artifacts = {}
for name in (
    "fingerprint-i2pr-client.tsv",
    "fingerprint-i2pd-client.tsv",
    "fingerprint-i2pr-server.tsv",
    "fingerprint-i2pd-server.tsv",
    "fingerprint-matrix.tsv",
    "fingerprint-manifest.tsv",
):
    path = evidence_dir / name
    artifacts[name] = hashlib.sha256(path.read_bytes()).hexdigest()
payload = {
    "schema": "i2pr-plan312-streaming-fingerprint-v1",
    "i2pr_commit": commit,
    "i2pd_version": "2.61.0",
    "i2pd_revision": "635b013a612ff47278ef02acf8580a28e10e26c5",
    "scenario": "clean_handshake_default_port",
    "compared_dimensions": ["flags", "from_included", "max_payload", "payload_length"],
    "artifacts_sha256": artifacts,
    "raw_packet_bytes_retained": False,
    "destination_or_stream_ids_retained": False,
}
(evidence_dir / "evidence.json").write_text(
    json.dumps(payload, indent=2, sort_keys=True) + "\n", encoding="utf-8"
)
print(f"Plan 312 sanitized traces and comparison matrix written to {evidence_dir}")
PY
