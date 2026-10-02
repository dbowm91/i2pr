#!/usr/bin/env bash
# Plan 279 — exact-pinned Java I2P 2.13.0 controlled floodfill second-family lane.
#
# Topology (all unprivileged, loopback-only, no reseed, no public network):
#
#   J1  stock Java floodfillParticipant=true  (reference replication target)
#   J2  stock Java floodfillParticipant=true  (reference replication target)
#   JC  stock Java floodfillParticipant=false (reference publisher + requester;
#       the only floodfill in its netDb is i2pr, so its own NetDB selection
#       is deterministic; hosts one TRANSIENT SAM destination so a
#       LeaseSet-family publication exercises matrix B)
#   P   i2pr controlled floodfill             (the subject of the qualification)
#
# The Java routers run unmodified through the out-of-tree test-only
# ControlledRouter launcher (stock Router + injected loopback properties,
# same precedent as the M6 lane). No Java source is patched, vendored, or
# rebuilt: the cache compiled by scripts/interop/fetch-m6-java.sh is used
# as-is and verified by pin before any process starts.
#
# The controlled identity and its `caps=f` RouterInfo are produced by the
# driver's prepare phase on a fixed loopback port and then seeded into the
# Java netDb file layout (<datadir>/netDb/r<ch>/routerInfo-<b64>.dat)
# before each Java router starts. The qualify phase reloads the same
# identity, rebinds the same port, and re-runs controlled activation
# against the live Java peers before driving the matrix; the withdraw
# phase reloads once more and proves health withdrawal.
#
# Required failures make this script fail. Sanitized evidence (counts,
# digests, categorical outcomes) defaults to
# target/interop/m12-floodfill-java-evidence. Reference private keys stay
# in the ephemeral scratch directory and are never copied to evidence.
# The Java DESTINATION reply of the lane-local SAM session contains key
# material and is never logged; only its RESULT is recorded.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${I2PR_M12_JAVA_FLOODFILL_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/m12-floodfill-java-evidence}"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION="2.13.0"
JAVA_CACHE="${REPO_ROOT}/target/interop/cache/m6-java/${JAVA_PIN}"
JAVA_J1_PORT="${I2PR_JAVA_FLOODFILL_A_PORT:-44841}"
JAVA_J2_PORT="${I2PR_JAVA_FLOODFILL_B_PORT:-44842}"
JAVA_JC_PORT="${I2PR_JAVA_FLOODFILL_C_PORT:-44843}"
JAVA_JC_SAM_PORT="${I2PR_JAVA_FLOODFILL_C_SAM_PORT:-44943}"
I2PR_PORT="${I2PR_FLOODFILL_PORT:-44831}"
DRIVER_TIMEOUT="${I2PR_M12_FLOODFILL_TIMEOUT:-900s}"
# Frozen before execution: three bounded attempts for a first-of-kind
# lane. A re-attempt requires a recorded lane delta (fix or finding);
# blind re-runs are forbidden.
MAX_ATTEMPTS=3

rm -rf "${EVIDENCE_DIR}"
mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-m12-plan279-java.XXXXXX)"
STATE_DIR="${SCRATCH}/state"
RESULTS_FILE="${SCRATCH}/results.tsv"
: > "${RESULTS_FILE}"
PIDS=()

# ---- Java cache verification (fail closed before any network use) -------
if [[ ! -f "${JAVA_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${JAVA_CACHE}/source-revision.txt")" != "${JAVA_PIN}" ]]; then
  echo "Java I2P cache has no verified Plan 279 source revision" >&2
  echo "run scripts/interop/fetch-m6-java.sh first" >&2
  exit 1
fi
if [[ ! -f "${JAVA_CACHE}/build-metadata.txt" ]]; then
  echo "Java I2P cache has no build metadata" >&2
  exit 1
fi
echo "==> Java I2P reference: ${JAVA_VERSION} (${JAVA_PIN})"
echo "==> attempt budget: ${MAX_ATTEMPTS} (frozen)"

cleanup() {
  local pid
  for pid in "${PIDS[@]:-}"; do
    kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null || true
  done
  for pid in "${PIDS[@]:-}"; do
    wait "${pid}" 2>/dev/null || true
  done
  [[ -z "${SCRATCH:-}" || ! -d "${SCRATCH}" ]] || rm -rf "${SCRATCH}"
}
trap cleanup EXIT

record_guarded() { # label detail rc
  local label="$1" detail="$2" rc="$3"
  detail="${detail//$'\t'/ }"
  detail="${detail//$'\n'/ }"
  if [[ "${rc}" -eq 0 ]]; then
    printf '%s\t%s\tpassed\t%s\n' "${label}" "plan279-java" "${detail}" >> "${RESULTS_FILE}"
  else
    printf '%s\t%s\tfailed\t%s (exit %s)\n' "${label}" "plan279-java" "${detail}" "${rc}" >> "${RESULTS_FILE}"
  fi
}

# ---- Plan 196 §5.1 controlled stock-router launcher build ----------------
LAUNCHER_BUILD="${SCRATCH}/build"
mkdir -p "${LAUNCHER_BUILD}"
JAVA_SRC_DIR="${REPO_ROOT}/tests/integration/m6-interop/java"
JAVA_CP=""
for jar in "${JAVA_CACHE}"/*.jar "${JAVA_CACHE}"/lib/*.jar; do
  if [[ -z "${JAVA_CP}" ]]; then
    JAVA_CP="${jar}"
  else
    JAVA_CP="${JAVA_CP}:${jar}"
  fi
done
if ! javac -d "${LAUNCHER_BUILD}" -cp "${JAVA_CP}" \
   "${JAVA_SRC_DIR}/ControlledRouter.java" \
   "${JAVA_SRC_DIR}/ReferenceRawDestination.java" \
   "${JAVA_SRC_DIR}/ReferenceStreamingService.java" \
   "${JAVA_SRC_DIR}"/net/i2p/router/networkdb/kademlia/*.java \
   >"${EVIDENCE_DIR}/javac.log" 2>&1; then
  echo "Java launcher compile failed; see ${EVIDENCE_DIR}/javac.log" >&2
  exit 1
fi
LAUNCHER_CP="${LAUNCHER_BUILD}:${JAVA_CP}"
echo "==> Java launcher compiled"

start_java() { # name datadir ssu2_port sam_port role logname
  local name="$1" datadir="$2" port="$3" sam="$4" role="$5" logname="$6"
  mkdir -p "${datadir}/logs"
  setsid java -Djava.net.preferIPv4Stack=true -Djava.awt.headless=true \
    -Djava.library.path="${JAVA_CACHE}:${JAVA_CACHE}/lib" \
    -Di2p.dir.base="${JAVA_CACHE}" \
    -DloggerFilenameOverride="logs/log-router-0.txt" \
    -Drouterconsole.enable=false \
    -cp "${LAUNCHER_CP}" -Dlauncher.scratch="${SCRATCH}" \
    ControlledRouter "${datadir}" 127.0.0.1 "${port}" "${sam}" 0 0 "${role}" \
    >"${EVIDENCE_DIR}/${logname}-stdout.log" 2>&1 < /dev/null &
  PIDS+=("$!")
  printf '%s' "$!" > "${SCRATCH}/${name}.pid"
}

wait_java_ready() { # name datadir ssu2_port attempts
  local name="$1" datadir="$2" port="$3" n="$4" i
  for i in $(seq 1 "${n}"); do
    if [[ -f "${datadir}/router/router.info" ]] &&
       python3 - "${port}" <<'PY' 2>/dev/null; then
import socket, sys
probe = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
try:
    probe.bind(("127.0.0.1", int(sys.argv[1])))
except OSError:
    sys.exit(0)
sys.exit(1)
PY
      return 0
    fi
    sleep 0.5
  done
  echo "ephemeral Java ${name} never published router.info / SSU2 listener" >&2
  return 1
}

# Seeds one RouterInfo into Java's PersistentDataStore file layout:
# <datadir>/netDb/r<first-b64-char>/routerInfo-<b64>.dat, where the
# ident is the i2p base64 router hash the prepare phase recorded.
seed_java_netdb() { # datadir src_file ident_b64
  local datadir="$1" src="$2" ident="$3"
  local bucket="r${ident:0:1}"
  mkdir -p "${datadir}/netDb/${bucket}"
  cp "${src}" "${datadir}/netDb/${bucket}/routerInfo-${ident}.dat"
}

# ---- phase 0: prepare the stable controlled identity + caps=f RouterInfo --
mkdir -p "${STATE_DIR}"
echo "==> phase 0: controlled identity + controlled activation"
prepare_rc=0
I2PR_FLOODFILL_STATE_DIR="${STATE_DIR}" \
I2PR_FLOODFILL_BIND_PORT="${I2PR_PORT}" \
EVIDENCE_DIR="${EVIDENCE_DIR}" \
timeout --foreground "${DRIVER_TIMEOUT}" cargo test --locked -p i2pr-daemon \
  --test floodfill_i2pd_external floodfill_prepare_against_i2pd \
  -- --ignored --exact --nocapture --test-threads=1 \
  > "${EVIDENCE_DIR}/prepare-driver.log" 2>&1 || prepare_rc=$?
record_guarded "controlled-activation-completed" \
  "peer-test-confirmed activation installed the caps=f RouterInfo (explicit --ignored --exact driver)" \
  "${prepare_rc}"
if [[ "${prepare_rc}" -ne 0 ]]; then
  sed -n '1,60p' "${EVIDENCE_DIR}/prepare-driver.log" >&2 || true
  echo "Plan 279 Java lane stopped at controlled activation" >&2
  exit 1
fi
PUBLISHED="${STATE_DIR}/published.routerinfo"
if [[ ! -f "${PUBLISHED}" ]]; then
  echo "controlled activation did not publish a RouterInfo" >&2
  exit 1
fi
echo "    published $(wc -c < "${PUBLISHED}") bytes"

# ---- phase 1: seed the Java mesh and start the reference routers ---------
# JC knows only i2pr as a floodfill, so its own NetDB selection is
# deterministic. J1/J2 start unseeded like the i2pd lane's F1/F2.
CONTROLLED_IDENT="$(<"${STATE_DIR}/ident.b64")"
if [[ -z "${CONTROLLED_IDENT}" ]]; then
  echo "prepare phase did not record the controlled router ident" >&2
  exit 2
fi
seed_java_netdb "${SCRATCH}/jcdata" "${PUBLISHED}" "${CONTROLLED_IDENT}"
echo "==> seeded controlled RouterInfo into the Java client's netDb layout"
start_java j1 "${SCRATCH}/j1data" "${JAVA_J1_PORT}" 0 publication j1
start_java j2 "${SCRATCH}/j2data" "${JAVA_J2_PORT}" 0 publication j2
start_java jc "${SCRATCH}/jcdata" "${JAVA_JC_PORT}" "${JAVA_JC_SAM_PORT}" transit jc
wait_java_ready j1 "${SCRATCH}/j1data" "${JAVA_J1_PORT}" 480
wait_java_ready j2 "${SCRATCH}/j2data" "${JAVA_J2_PORT}" 480
wait_java_ready jc "${SCRATCH}/jcdata" "${JAVA_JC_PORT}" 480
echo "==> Java routers up on 127.0.0.1:${JAVA_J1_PORT},127.0.0.1:${JAVA_J2_PORT},127.0.0.1:${JAVA_JC_PORT}"
# Java startup jobs (exploration scheduling, first publication) need a
# settle window after the listeners bind; the matrix waits absorb the rest.
sleep 60

# ---- phase 1b: lane-local TRANSIENT SAM destination on JC ---------------
# Gives the Java client a LeaseSet-family record to publish toward its
# only known floodfill (i2pr, matrix B). The DESTINATION reply carries
# key material and is never logged; only its RESULT is recorded.
python3 - "${JAVA_JC_SAM_PORT}" <<'PY' >"${EVIDENCE_DIR}/sam-destination.result" 2>&1 &
import socket, sys, time
port = int(sys.argv[1])
deadline = time.time() + 120
session = None
while time.time() < deadline:
    try:
        session = socket.create_connection(("127.0.0.1", port), timeout=5)
        break
    except OSError:
        time.sleep(1)
if session is None:
    print("RESULT=FAIL sam-unreachable")
    sys.exit(3)
session.settimeout(30)
stream = session.makefile("rwb")
def command(line):
    stream.write(line.encode() + b"\n")
    stream.flush()
    return stream.readline().decode(errors="replace").strip()
reply = command("HELLO VERSION")
assert "RESULT=OK" in reply, reply
reply = command("SESSION CREATE STYLE=DATAGRAM ID=lane-dest DESTINATION=TRANSIENT SIGNATURE_TYPE=7")
assert "RESULT=OK" in reply, "SESSION CREATE rejected"
print("RESULT=OK destination-established")
sys.stdout.flush()
time.sleep(1500)
PY
SAM_PID=$!
PIDS+=("${SAM_PID}")
# The holder sleeps for the lane duration; poll for establishment
# without joining it (the EXIT trap owns its teardown).
sam_ok=0
for i in $(seq 1 150); do
  if grep -Fq "RESULT=OK" "${EVIDENCE_DIR}/sam-destination.result" 2>/dev/null; then
    sam_ok=1
    break
  fi
  if ! kill -0 "${SAM_PID}" 2>/dev/null; then
    break
  fi
  sleep 2
done
if [[ "${sam_ok}" -eq 1 ]]; then
  echo "==> Java SAM destination established on JC"
else
  echo "Java SAM destination failed; see ${EVIDENCE_DIR}/sam-destination.result" >&2
  cat "${EVIDENCE_DIR}/sam-destination.result" >&2 || true
  exit 1
fi

# ---- phase 2: run the qualification matrix --------------------------------
echo "==> phase 2: qualification matrix"
qualify_rc=0
I2PR_FLOODFILL_STATE_DIR="${STATE_DIR}" \
I2PR_FLOODFILL_BIND_PORT="${I2PR_PORT}" \
EVIDENCE_DIR="${EVIDENCE_DIR}" \
FLOODFILL_REF_A_ROUTER_INFO="${SCRATCH}/j1data/router/router.info" \
FLOODFILL_REF_A_ENDPOINT="127.0.0.1:${JAVA_J1_PORT}" \
FLOODFILL_REF_B_ROUTER_INFO="${SCRATCH}/j2data/router/router.info" \
FLOODFILL_REF_B_ENDPOINT="127.0.0.1:${JAVA_J2_PORT}" \
FLOODFILL_REF_C_ROUTER_INFO="${SCRATCH}/jcdata/router/router.info" \
FLOODFILL_REF_C_ENDPOINT="127.0.0.1:${JAVA_JC_PORT}" \
timeout --foreground "${DRIVER_TIMEOUT}" cargo test --locked -p i2pr-daemon \
  --test floodfill_i2pd_external floodfill_qualify_against_java \
  -- --ignored --exact --nocapture --test-threads=1 \
  > "${EVIDENCE_DIR}/qualify-driver.log" 2>&1 || qualify_rc=$?
if [[ "${qualify_rc}" -ne 0 ]]; then
  sed -n '1,80p' "${EVIDENCE_DIR}/qualify-driver.log" >&2 || true
fi

DRIVER_TSV="${EVIDENCE_DIR}/driver-evidence.tsv"
driver_row() { # label key detail
  local label="$1" key="$2" detail="$3" rc=1
  if [[ "${qualify_rc}" -eq 0 && -f "${DRIVER_TSV}" ]] &&
     grep -Fq "${key}" "${DRIVER_TSV}"; then
    rc=0
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}

driver_row "external-reference-verified" "reference-routerinfo-verified" \
  "every Java RouterInfo parsed and verified through the documented file path"
driver_row "external-role-active" "role-active-after-activation" \
  "controlled activation reached the Active role with the caps=f RouterInfo installed"
driver_row "external-publisher-store" "publisher-store-accepted" \
  "Java publisher store validated and accepted (matrix A)"
driver_row "external-store-ack" "publisher-store-ack-delivered" \
  "DatabaseStore DeliveryStatus acknowledgement delivered to Java (matrix A)"
driver_row "external-lookup-answered" "lookup-replies-classified" \
  "Java DatabaseLookup answered with classified replies (matrix C/E)"
driver_row "external-replication-direct" "replication-direct-store-delivered" \
  "direct zero-token SSU2 DatabaseStore reached the Java floodfill (matrix F)"
driver_row "external-no-tunnel-flood" "replication-tunnel-fallback-absent" \
  "replication stayed on the direct authenticated path with no tunnel fallback"
driver_row "external-dispatch-clean" "client-peer-identified" \
  "no dispatch errors and no unclassified effects during the matrix"

# ---- matrix B: LeaseSet-family store gate ---------------------------------
# The census row lists stored-record type discriminants (0 RouterInfo,
# 1 LeaseSet, 3 LeaseSet2, 7 MetaLeaseSet). Matrix B passes only when a
# LeaseSet-family discriminant (1, 3, or 7) was stored and acked through
# the same production path as matrix A. Fail-closed: an absent family
# is a lane failure with exact provenance, never a skip.
leaseset_rc=1
if [[ "${qualify_rc}" -eq 0 && -f "${DRIVER_TSV}" ]] &&
   grep -Fq "store-record-families-final" "${DRIVER_TSV}"; then
  if python3 - "${DRIVER_TSV}" <<'PY'; then
import sys
census = [line for line in open(sys.argv[1], encoding="utf-8")
          if line.startswith("store-record-families-final")]
kinds = set()
for line in census:
    for token in line.replace("[", " ").replace("]", " ").replace(",", " ").split():
        if token in ("1", "3", "7"):
            kinds.add(token)
sys.exit(0 if kinds else 1)
PY
    leaseset_rc=0
  fi
fi
record_guarded "external-leaseset-store" \
  "LeaseSet-family store accepted and acked through the production path (matrix B)" \
  "${leaseset_rc}"

# ---- phase 3: health withdrawal -------------------------------------------
echo "==> phase 3: health withdrawal"
withdraw_rc=0
I2PR_FLOODFILL_STATE_DIR="${STATE_DIR}" \
I2PR_FLOODFILL_BIND_PORT="${I2PR_PORT}" \
EVIDENCE_DIR="${EVIDENCE_DIR}" \
FLOODFILL_REF_A_ROUTER_INFO="${SCRATCH}/j1data/router/router.info" \
FLOODFILL_REF_A_ENDPOINT="127.0.0.1:${JAVA_J1_PORT}" \
FLOODFILL_REF_B_ROUTER_INFO="${SCRATCH}/j2data/router/router.info" \
FLOODFILL_REF_B_ENDPOINT="127.0.0.1:${JAVA_J2_PORT}" \
timeout --foreground "${DRIVER_TIMEOUT}" cargo test --locked -p i2pr-daemon \
  --test floodfill_i2pd_external floodfill_withdraw_against_java \
  -- --ignored --exact --nocapture --test-threads=1 \
  > "${EVIDENCE_DIR}/withdraw-driver.log" 2>&1 || withdraw_rc=$?
if [[ "${withdraw_rc}" -ne 0 ]]; then
  sed -n '1,80p' "${EVIDENCE_DIR}/withdraw-driver.log" >&2 || true
fi
withdraw_key_rc=1
if [[ "${withdraw_rc}" -eq 0 && -f "${DRIVER_TSV}" ]] &&
   grep -Fq "withdrawal-nonf-republished" "${DRIVER_TSV}" &&
   grep -Fq "role-disabled-after-withdraw" "${DRIVER_TSV}"; then
  withdraw_key_rc=0
fi
record_guarded "external-withdrawal" \
  "loss snapshot stopped admission and republished without f; role Disabled (matrix I)" \
  "${withdraw_key_rc}"

# ---- local gates slice ----------------------------------------------------
GATES_LOG="${EVIDENCE_DIR}/workspace-gates.log"
: > "${GATES_LOG}"
gates_rc=0
cargo fmt --all --check >>"${GATES_LOG}" 2>&1 || gates_rc=1
cargo check --locked --workspace --all-targets >>"${GATES_LOG}" 2>&1 || gates_rc=1
for gate in check-dependency-direction check-runtime-boundaries check-m12-floodfill-boundaries; do
  if ! bash "${REPO_ROOT}/scripts/${gate}.sh" >>"${GATES_LOG}" 2>&1; then
    echo "GATE FAILED: ${gate}.sh" >>"${GATES_LOG}"
    gates_rc=1
  fi
done
record_guarded "workspace-gates" \
  "fmt + workspace check --all-targets + static boundary scripts" "${gates_rc}"

python3 - "${RESULTS_FILE}" "${EVIDENCE_DIR}" "${REPO_ROOT}" "${JAVA_PIN}" "${JAVA_VERSION}" \
  "${MAX_ATTEMPTS}" <<'PY'
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

results_path, evidence_dir, repo_root, java_pin, java_version, budget = sys.argv[1:7]
rows = []
with open(results_path, encoding="utf-8") as stream:
    for line in stream:
        label, plan, status, detail = line.rstrip("\n").split("\t", 3)
        rows.append({"label": label, "plan": plan, "status": status, "detail": detail})

commit = subprocess.check_output(
    ["git", "-C", repo_root, "rev-parse", "HEAD"], text=True
).strip()
rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
driver_keys = []
driver_tsv = Path(evidence_dir) / "driver-evidence.tsv"
if driver_tsv.exists():
    for line in driver_tsv.read_text(encoding="utf-8").splitlines():
        driver_keys.append(line.split("\t", 1)[0])
passed = all(row["status"] == "passed" for row in rows)
evidence = {
    "schema": "i2pr-m12-floodfill-qualification-v1",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": "m12-floodfill-java-external",
    "attempt_budget": int(budget),
    "attempts_used": 1,
    "bind_policy": "127.0.0.1 loopback only; controlled fixed port; no introducer",
    "java": {
        "repository": "https://github.com/i2p/i2p.i2p.git",
        "revision": java_pin,
        "version": java_version,
        "role": "independent second-family floodfill reference, unmodified",
        "profile": "two floodfillParticipant=true reference peers, one transit client, SSU2 only, reseed disabled",
    },
    "driver_evidence_keys": driver_keys,
    "results": rows,
    "m12_floodfill_second_family": "passed-via-java-2.13.0" if passed else "failed",
    "known_limitations": [
        "loopback-only controlled mesh; no public I2P participation",
        "second-family (Java) qualification only; no normal-daemon caps=f advertisement",
        "reference logs are diagnostic only; retained evidence is sanitized counts, digests, and categorical outcomes",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 279 M12 controlled floodfill second-family qualification (exact-pinned Java)\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- Java I2P: `{java_version}` @ `{java_pin}` (unmodified)\n")
    stream.write(f"- Attempt budget: `{budget}` frozen before execution\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only, fixed controlled port, no introducer\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

if awk -F'\t' '$3 != "passed" { found = 1 } END { exit found ? 0 : 1 }' "${RESULTS_FILE}"; then
  echo "Plan 279 Java lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 279 Java lane passed; sanitized evidence: ${EVIDENCE_DIR}"
