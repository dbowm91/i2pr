#!/usr/bin/env bash
# Plan 199 Phase A — evidence-consuming M6 final closure gate.
# This is intentionally separate from the routine structural checker: it is
# run only after a complete manual two-family external workflow has produced
# sanitized evidence for the exact candidate head.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
EVIDENCE_DIR="${I2PR_M6_MIXED_ROUTER_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/m6-mixed-router-evidence}"
EXPECTED_I2PD="635b013a612ff47278ef02acf8580a28e10e26c5"
EXPECTED_JAVA="9134f808337b401e8e53c73734c81fab04280c9d"

python3 - "${EVIDENCE_DIR}" "${REPO_ROOT}" "${EXPECTED_I2PD}" "${EXPECTED_JAVA}" <<'PY'
import json
import subprocess
import sys
from pathlib import Path

evidence_dir = Path(sys.argv[1])
repo_root = Path(sys.argv[2])
expected_i2pd = sys.argv[3]
expected_java = sys.argv[4]
java_harness = repo_root / "tests/integration/m6-interop/run-java.sh"
if not java_harness.is_file():
    raise SystemExit(f"missing authoritative Java harness: {java_harness}")
root_file = evidence_dir / "evidence.json"
if not root_file.is_file():
    raise SystemExit(f"missing cross-family evidence: {root_file}")

root_evidence = json.loads(root_file.read_text(encoding="utf-8"))
head = subprocess.check_output(["git", "-C", str(repo_root), "rev-parse", "HEAD"], text=True).strip()
if root_evidence.get("i2pr_commit") != head:
    raise SystemExit("cross-family evidence does not belong to the exact current i2pr head")
if root_evidence.get("workflow_run_id", "") == "":
    raise SystemExit("missing exact-head manual workflow run provenance")
if root_evidence.get("families", {}).get("i2pd", {}).get("revision") != expected_i2pd:
    raise SystemExit("i2pd pin mismatch")
if root_evidence.get("families", {}).get("java_i2p", {}).get("revision") != expected_java:
    raise SystemExit("Java pin mismatch")

families = {"i2pd": [], "java": []}
generic = [
    "external-daemon-strict-profile",
    "external-reference-verified",
    "external-session-established",
    "external-tunnel-build-accepted",
    "external-netdb-lookup-tunnel",
    "external-destination-ls2-resolved",
    "external-destination-message-roundtrip",
    "external-streaming-established",
    "external-streaming-multipacket-digest",
    "external-clean-resource-baseline",
    "workspace-gates",
]
rows = root_evidence.get("results", [])
for family in families:
    suffix = f"-{family}"
    family_rows = {row.get("label"): row for row in rows if row.get("label", "").endswith(suffix)}
    missing = [f"{label}{suffix}" for label in generic if f"{label}{suffix}" not in family_rows]
    bad = [label for label, row in family_rows.items() if row.get("status") != "passed"]
    if missing or bad:
        raise SystemExit(f"{family} family is not all-pass: missing={missing} bad={bad}")
    families[family] = list(family_rows.values())

java_files = []
for candidate in evidence_dir.rglob("evidence.json"):
    if candidate == root_file:
        continue
    try:
        data = json.loads(candidate.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        continue
    if data.get("execution_lane") == "m6-java-external":
        java_files.append((candidate, data))
if len(java_files) != 1:
    raise SystemExit(f"expected exactly one Java public-client evidence ledger, found {len(java_files)}")

java_path, java_evidence = java_files[0]
if java_evidence.get("i2pr_commit") != head:
    raise SystemExit(f"Java evidence does not belong to exact head: {java_path}")
if java_evidence.get("java_i2p", {}).get("revision") != expected_java:
    raise SystemExit("Java evidence pin mismatch")
java_rows = java_evidence.get("results", [])
mandatory_java = {
    "daemon-strict-profile", "reference-routerinfo-verified", "session-established",
    "outbound-installed", "inbound-installed", "lease-lookup-completed",
    "ls2-publication-tunnel", "destination-outbound-delivered", "reference-received",
    "destination-inbound-received", "streaming-syn-sent", "streaming-syn-accepted",
    "streaming-established", "streaming-data-digest", "streaming-multipacket-digest",
    "streaming-reverse-data-digest", "streaming-reverse-multipacket-digest",
    "streaming-sibling-established", "streaming-sibling-data-digest", "streaming-close",
    "streaming-sibling-isolated", "streaming-b-established", "streaming-b-data-digest",
    "streaming-b-reverse-data-digest", "streaming-b-close", "manager-cleanup",
    "direct-rejected", "liveness-first-test", "shutdown-baseline",
    "p234-classification", "p234-syn-epoch", "p234-java-accept-state",
}
java_keys = set(java_evidence.get("driver_evidence_keys", []))
missing = sorted(mandatory_java - java_keys)
bad = [row.get("label") for row in java_rows if row.get("status") != "passed"]
driver_tsv = java_path.parent / "driver" / "driver-evidence.tsv"
driver_tsv_text = driver_tsv.read_text(encoding="utf-8") if driver_tsv.exists() else ""
p234_passed = any(
    line.startswith("p234-classification\t")
    and "P234-C-STREAMING-DIRECTION-A-ESTABLISHED" in line
    for line in driver_tsv_text.splitlines()
)
if missing or bad or not p234_passed or java_evidence.get("m6_java") != "passed-via-java-2.13.0":
    raise SystemExit(f"Java public-client ledger is not all-pass: missing={missing} bad={bad} status={java_evidence.get('m6_java')}")

print(f"mandatory_i2pd: {len(families['i2pd'])}/{len(generic)} passed, 0 blocked, 0 failed, 0 missing")
print(f"mandatory_java: {len(mandatory_java)}/{len(mandatory_java)} passed, 0 blocked, 0 failed, 0 missing")
print("m6_final_closure: passed")
PY
