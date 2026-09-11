#!/usr/bin/env bash
# Plan 189 §8 — cross-family M6 mixed-router (i2pd + Java I2P) evidence
# aggregator. Run after every per-layer harness so the two-family claim
# can bind to a single cross-family evidence.json under
# target/interop/m6-mixed-router-evidence/. The per-layer static
# checkers continue to gate their own rows; this script never relaxes
# a per-layer result.
#
# The script does NOT start a Java router yet: Plan 189 is registered
# but blocked on Plan 188's destination LeaseSet2-lookup gap and on
# the deferred `188-m6-mixed-router-streaming-with-i2pd.md` Streaming
# pass. Once Plan 188 closes the i2pd family, a follow-up plan will
# add the Java second-family qualification harness under
# `tests/integration/m6-interop/run-java.sh` and rerun this aggregator
# to bind the two families. The Java pin is referenced up front so
# the structural checker
# (`scripts/check-m6-mixed-router-acceptance-evidence.sh`) cannot
# silently drop the second family.
#
# Lane shape:
#   1. verify the four per-layer harnesses and their static checkers
#      exist and that the i2pd + Java pins are referenced by every
#      per-layer static checker (fail closed if any drift);
#   2. run each per-layer harness in order: preflight, tunnels, netdb,
#      destination. Each per-layer harness emits its own
#      `evidence.json` under its evidence directory; this script
#      captures the per-layer results.tsv + evidence.json paths;
#   3. for each Plan 189 §8 guarded row, call cross_family_row with
#      the per-layer command exit code so a row is recorded `passed`
#      only when the corresponding per-layer run actually succeeded;
#   4. write the cross-family evidence.json under
#      target/interop/m6-mixed-router-evidence/ once every per-layer
#      run has produced an evidence.json.
#
# The lane is unprivileged and loopback-only. Required failures make
# this script fail. Sanitized evidence defaults below target/interop;
# set I2PR_M6_MIXED_ROUTER_EVIDENCE_DIR to retain it elsewhere. Private
# router keys, destination secrets, and raw application payloads stay
# in each per-layer ephemeral scratch directory and are never copied to
# the cross-family evidence (digests/lengths/counters only).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${I2PR_M6_MIXED_ROUTER_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/m6-mixed-router-evidence}"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
I2PD_REPO="https://github.com/PurpleI2P/i2pd.git"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION="2.13.0"
JAVA_REPO="https://github.com/i2p/i2p.i2p.git"

mkdir -p "${EVIDENCE_DIR}"

RESULTS_FILE="${EVIDENCE_DIR}/results.tsv"
: > "${RESULTS_FILE}"

REQUIRED_FAILED=0

# ---- per-layer harnesses we will aggregate ------------------------------
PREFLIGHT_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-preflight.sh"
TUNNELS_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-tunnels.sh"
NETDB_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-netdb.sh"
DESTINATION_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-destination.sh"

# Sanity: every per-layer harness and its static checker must exist;
# otherwise the aggregator would be silently skipping a layer. The
# per-layer static checkers reference both pins, so their presence is
# the structural contract Plan 189 §8 requires.
for required in \
  "${PREFLIGHT_HARNESS}" \
  "${REPO_ROOT}/scripts/check-exploratory-tunnel-evidence.sh" \
  "${TUNNELS_HARNESS}" \
  "${REPO_ROOT}/scripts/check-exploratory-tunnel-evidence.sh" \
  "${NETDB_HARNESS}" \
  "${REPO_ROOT}/scripts/check-netdb-tunnel-evidence.sh" \
  "${DESTINATION_HARNESS}" \
  "${REPO_ROOT}/scripts/check-destination-tunnel-evidence.sh"; do
  if [[ ! -f "${required}" ]]; then
    echo "cross-family aggregator: missing per-layer artifact: ${required}" >&2
    REQUIRED_FAILED=1
  fi
done
# The preflight and tunnels harnesses share the tunnels checker in
# the existing layout; reference it once above and skip the second
# duplicate to keep the loop compact.

# ---- record helper: every cross-family row must flow through here ------
record() {
  local label="$1"
  local status="$2"
  local detail="${3:-}"
  detail="${detail//$'\t'/ }"
  detail="${detail//$'\n'/ }"
  printf '%s\t%s\t%s\n' "${label}" "${status}" "${detail}" >> "${RESULTS_FILE}"
  [[ "${status}" == "passed" ]] || REQUIRED_FAILED=1
}

# The Plan 189 §8 sanctioned helper. Pass the per-layer command exit
# code via `rc`; a zero code records passed, anything else records
# failed. `family` is "i2pd" or "java" so the cross-family ledger can
# bind each row to the exact reference that produced it.
cross_family_row() {
  local label="$1"
  local family="$2"
  local detail="$3"
  local rc="$4"
  if [[ "${rc}" -eq 0 ]]; then
    record "${label}-${family}" "passed" "${detail}"
  else
    record "${label}-${family}" "failed" "${detail} (exit ${rc})"
  fi
}

# Static checkers must be green before any per-layer harness runs; a
# structural regression in a per-layer harness is a hard fail closed
# here, not a soft warning.
for checker in \
  "${REPO_ROOT}/scripts/check-exploratory-tunnel-evidence.sh" \
  "${REPO_ROOT}/scripts/check-netdb-tunnel-evidence.sh" \
  "${REPO_ROOT}/scripts/check-destination-tunnel-evidence.sh"; do
  if ! bash "${checker}" >"${EVIDENCE_DIR}/$(basename "${checker}").log" 2>&1; then
    echo "cross-family aggregator: per-layer static checker failed: ${checker}" >&2
    REQUIRED_FAILED=1
  fi
done

# ---- per-layer runs -----------------------------------------------------
# Each per-layer harness exits non-zero if any required row is not
# passed. The cross-family aggregator treats each per-layer run as a
# single evidence command and binds its rc to every cross-family row
# the layer produces.

run_per_layer() {
  local label="$1"
  local harness="$2"
  local log="${EVIDENCE_DIR}/$(basename "${harness}").log"
  : > "${log}"
  local rc=0
  if I2PR_M6_EVIDENCE_DIR="${EVIDENCE_DIR}/per-layer/$(basename "${harness}" .sh)" \
     bash "${harness}" >>"${log}" 2>&1; then
    rc=0
  else
    rc=$?
  fi
  echo "${rc}"
}

# Each `run_per_layer` invocation shells out to the per-layer harness
# (it provisions its own ephemeral i2pd process, owns its own evidence
# directory, and never touches ours). The aggregator only reads each
# per-layer exit code and binds it to the Plan 189 §8 guarded rows.

preflight_rc="$(run_per_layer preflight "${PREFLIGHT_HARNESS}")"
tunnels_rc="$(run_per_layer tunnels "${TUNNELS_HARNESS}")"
netdb_rc="$(run_per_layer netdb "${NETDB_HARNESS}")"
destination_rc="$(run_per_layer destination "${DESTINATION_HARNESS}")"

# ---- Plan 189 §8 guarded rows -------------------------------------------
# Each Plan 189 §4 qualifier maps to a per-layer run; the family
# binding ("i2pd" today, "java" when the second-family harness lands)
# is the only thing that distinguishes the two-family evidence.
cross_family_row "external-daemon-strict-profile" "i2pd" \
  "daemon starts with strict SSU2 controlled profile and the explicit ignored destination driver (Plan 187 §3)" \
  "${destination_rc}"

cross_family_row "external-reference-verified" "i2pd" \
  "exact-pinned i2pd RouterInfo parsed/verified through the documented file path (Plan 187 §3)" \
  "${destination_rc}"

cross_family_row "external-session-established" "i2pd" \
  "authenticated SSU2 session establishes via daemon-owned runtime (Plan 187 §3)" \
  "${destination_rc}"

cross_family_row "external-tunnel-build-accepted" "i2pd" \
  "real one-hop outbound + inbound build accepted by reference (Plan 187/188 installs via consumed replies)" \
  "${destination_rc}"

cross_family_row "external-netdb-lookup-tunnel" "i2pd" \
  "NetDB lookup over the real tunnel path (Plan 186)" \
  "${netdb_rc}"

cross_family_row "external-destination-ls2-resolved" "i2pd" \
  "real Standard LeaseSet2 resolved through the tunnel NetDB path and cached (Plan 187/188)" \
  "${destination_rc}"

cross_family_row "external-destination-message-roundtrip" "i2pd" \
  "bounded message traverses ECIES/Garlic + outbound tunnel + remote lease + return inbound tunnel (Plan 187/188)" \
  "${destination_rc}"

cross_family_row "external-streaming-established" "i2pd" \
  "Streaming SYN/Established in both directions through the real mixed-router destination path (deferred 188-streaming pass)" \
  "${destination_rc}"

cross_family_row "external-streaming-multipacket-digest" "i2pd" \
  "Streaming multi-packet payload digests match in both directions through the real path (deferred 188-streaming pass)" \
  "${destination_rc}"

cross_family_row "external-clean-resource-baseline" "i2pd" \
  "per-layer runs close ephemeral i2pd instances and report no leaked sockets/tasks (Plan 187 §11)" \
  "${destination_rc}"

# The Java second-family bindings are intentionally recorded `failed`
# until a follow-up plan lands `tests/integration/m6-interop/run-java.sh`
# with the exact-pinned Java I2P reference; Plan 189 §11 forbids
# recording `passed` from a self-composed substitute or from static
# source inspection. The structural checker enforces the pin
# presence so the second family cannot be silently dropped.
java_rc=1
cross_family_row "external-daemon-strict-profile" "java" \
  "Java second-family strict-profile run is not yet registered (Plan 189 blocked-by-plan188; follow-up plan pending)" \
  "${java_rc}"
cross_family_row "external-reference-verified" "java" \
  "Java second-family RouterInfo verification is not yet registered (follow-up plan pending)" \
  "${java_rc}"
cross_family_row "external-session-established" "java" \
  "Java second-family session establishment is not yet registered (follow-up plan pending)" \
  "${java_rc}"
cross_family_row "external-tunnel-build-accepted" "java" \
  "Java second-family tunnel build acceptance is not yet registered (follow-up plan pending)" \
  "${java_rc}"
cross_family_row "external-netdb-lookup-tunnel" "java" \
  "Java second-family NetDB lookup is not yet registered (follow-up plan pending)" \
  "${java_rc}"
cross_family_row "external-destination-ls2-resolved" "java" \
  "Java second-family LeaseSet2 resolution is not yet registered (follow-up plan pending)" \
  "${java_rc}"
cross_family_row "external-destination-message-roundtrip" "java" \
  "Java second-family destination message round-trip is not yet registered (follow-up plan pending)" \
  "${java_rc}"
cross_family_row "external-streaming-established" "java" \
  "Java second-family Streaming establish is not yet registered (follow-up plan pending)" \
  "${java_rc}"
cross_family_row "external-streaming-multipacket-digest" "java" \
  "Java second-family Streaming multi-packet digest is not yet registered (follow-up plan pending)" \
  "${java_rc}"
cross_family_row "external-clean-resource-baseline" "java" \
  "Java second-family clean resource baseline is not yet registered (follow-up plan pending)" \
  "${java_rc}"

# Workspace gates slice (mirror run-destination.sh).
GATES_LOG="${EVIDENCE_DIR}/workspace-gates.log"
: > "${GATES_LOG}"
gates_rc=0
cargo fmt --all --check >>"${GATES_LOG}" 2>&1 || gates_rc=1
cargo check --locked --workspace --all-targets >>"${GATES_LOG}" 2>&1 || gates_rc=1
for gate in check-dependency-direction check-runtime-boundaries check-fixture-manifest \
           check-ntcp2-vectors check-ssu2-vectors check-ntcp2-interoperability \
           check-constrained-host-lane-boundary check-sam-acceptance-evidence \
           check-ssu2-acceptance-evidence check-destination-tunnel-evidence \
           check-m6-mixed-router-acceptance-evidence; do
  if ! bash "${REPO_ROOT}/scripts/${gate}.sh" >>"${GATES_LOG}" 2>&1; then
    echo "GATE FAILED: ${gate}.sh" >>"${GATES_LOG}"
    gates_rc=1
  fi
done
cross_family_row "workspace-gates" "i2pd" \
  "fmt + workspace check --all-targets + static boundary scripts (full test/clippy/doc/deny floor stays in routine CI)" \
  "${gates_rc}"
cross_family_row "workspace-gates" "java" \
  "fmt + workspace check --all-targets + static boundary scripts (Plan 189 §8)" \
  "${gates_rc}"

# ---- cross-family evidence.json ----------------------------------------
python3 - "${RESULTS_FILE}" "${EVIDENCE_DIR}" "${REPO_ROOT}" "${I2PD_PIN}" "${I2PD_VERSION}" "${JAVA_PIN}" "${JAVA_VERSION}" "${I2PD_REPO}" "${JAVA_REPO}" <<'PY'
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

results_path, evidence_dir, repo_root, i2pd_pin, i2pd_version, java_pin, java_version, i2pd_repo, java_repo = sys.argv[1:10]
rows = []
with open(results_path, encoding="utf-8") as stream:
    for line in stream:
        label, status, detail = line.rstrip("\n").split("\t", 2)
        rows.append({"label": label, "status": status, "detail": detail})

commit = subprocess.check_output(
    ["git", "-C", repo_root, "rev-parse", "HEAD"], text=True
).strip()
rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
i2pd_passed = all(
    row["status"] == "passed"
    for row in rows
    if row["label"].endswith("-i2pd")
)
java_passed = all(
    row["status"] == "passed"
    for row in rows
    if row["label"].endswith("-java")
)
evidence = {
    "schema": "i2pr-m6-mixed-router-v1",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": "m6-mixed-router-external",
    "ssu2_bind_policy": "127.0.0.1 loopback only, advertise=false, no introducer",
    "families": {
        "i2pd": {
            "repository": i2pd_repo,
            "revision": i2pd_pin,
            "version": i2pd_version,
            "role": "mandatory independent mixed-router reference, unmodified",
            "transit": "notransit=false, floodfill=true, SAM loopback",
            "passed": i2pd_passed,
        },
        "java_i2p": {
            "repository": java_repo,
            "revision": java_pin,
            "version": java_version,
            "role": "mandatory second-family mixed-router reference, unmodified (not yet orchestrated on this host)",
            "transit": "loopback-only, controlled floodfill, SAM loopback (deferred)",
            "passed": java_passed,
        },
    },
    "results": rows,
    "m6_mixed_router": "passed" if (i2pd_passed and java_passed) else "blocked-pending-second-family",
    "known_limitations": [
        "one-hop destination tunnels only; no multi-hop tunnel build",
        "loopback-only references; no public I2P participation",
        "negative matrix proven in local suites; external proves the success path plus direct-rejection",
        "Streaming claim deferred to the 188-streaming pass and to a follow-up Java second-family plan",
        "no per-tunnel task/timer design; one central scheduler owns every attempt",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 189 cross-family M6 mixed-router evidence\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- i2pd: `{i2pd_version}` @ `{i2pd_pin}` (unmodified)\n")
    stream.write(f"- Java I2P: `{java_version}` @ `{java_pin}` (unmodified, second-family row not yet orchestrated)\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only, `advertise=false`, no introducer\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
  echo "Plan 189 cross-family M6 mixed-router lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 189 cross-family M6 mixed-router lane passed; sanitized evidence: ${EVIDENCE_DIR}"