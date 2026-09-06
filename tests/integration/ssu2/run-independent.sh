#!/usr/bin/env bash
# Plan 161 — run the SSU2 independent-IPv4 interop matrix end-to-end,
# deriving every final evidence row from an executed command/test result.
#
# Provenance: local rows execute the focused Plan 155–160 suites; external
# rows provision one ephemeral exact-pinned i2pd 2.61.0 process on
# loopback, run the single fail-closed driver
# (crates/i2pr-runtime/tests/ssu2_independent.rs) through its explicit
# `--ignored --exact` selection, and derive each row from the driver exit
# status plus its own sanitized evidence keys. No required row is recorded
# `passed` except by the exit status of its associated command (plus the
# row's own evidence keys for external rows); see
# scripts/check-ssu2-acceptance-evidence.sh.
#
# The lane is unprivileged and loopback-only. Required failures make this
# script fail. Sanitized evidence defaults below target/interop; set
# I2PR_SSU2_EVIDENCE_DIR to retain it elsewhere. Private router keys stay
# in the ephemeral scratch directory and are never copied to evidence.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${I2PR_SSU2_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/ssu2-evidence}"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
I2PD_REPO="https://github.com/PurpleI2P/i2pd.git"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION="2.13.0"
I2PD_CACHE="${REPO_ROOT}/target/interop/cache/ssu2/i2pd/${I2PD_PIN}"
I2PD_BIN="${I2PR_I2PD_BIN:-${I2PD_CACHE}/bin/i2pd}"
I2PD_PORT="${I2PR_I2PD_PORT:-43823}"
I2PR_PORT="${I2PR_SSU2_PORT:-44001}"
DRIVER_TIMEOUT="600s"

mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-ssu2-plan161.XXXXXX)"
RESULTS_FILE="${SCRATCH}/results.tsv"
: > "${RESULTS_FILE}"

# ---- i2pd cache verification (fail closed before any network use) --------
if [[ ! -x "${I2PD_BIN}" ]]; then
  echo "i2pd binary missing: ${I2PD_BIN}" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  exit 1
fi
if [[ ! -f "${I2PD_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${I2PD_CACHE}/source-revision.txt")" != "${I2PD_PIN}" ]]; then
  echo "i2pd cache has no verified Plan 161 source revision" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  exit 1
fi
if "${I2PD_BIN}" --version 2>&1 | grep -Fq "${I2PD_VERSION}"; then
  echo "==> i2pd reference: ${I2PD_VERSION} (${I2PD_PIN})"
else
  echo "i2pd binary does not report ${I2PD_VERSION}" >&2
  exit 1
fi

# ---- ephemeral i2pd provisioning (fresh datadir: no stale netDb) ---------
I2PD_HOME="${SCRATCH}/i2pd"
I2PD_DATA="${I2PD_HOME}/data"
I2PD_LOG="${EVIDENCE_DIR}/i2pd.log"
mkdir -p "${I2PD_DATA}"
cat > "${I2PD_HOME}/i2pd.conf" <<EOF
daemon = false
loglevel = info
netid = 2
address4 = 127.0.0.1
host = 127.0.0.1
port = ${I2PD_PORT}
ipv4 = true
ipv6 = false
nat = false
notransit = true
floodfill = false
reservedrange = false
bandwidth = L
[ssu2]
enabled = true
published = true
port = ${I2PD_PORT}
[ntcp2]
enabled = false
published = false
[http]
enabled = false
[httpproxy]
enabled = false
[socksproxy]
enabled = false
[sam]
enabled = false
[i2cp]
enabled = false
[i2pcontrol]
enabled = false
[upnp]
enabled = false
[reseed]
verify = true
urls =
threshold = 0
EOF
: > "${I2PD_LOG}"
setsid "${I2PD_BIN}" "--conf=${I2PD_HOME}/i2pd.conf" "--datadir=${I2PD_DATA}" \
  --log=file "--logfile=${I2PD_LOG}" >/dev/null 2>&1 < /dev/null &
I2PD_PID=$!
CHILD_PIDS=("${I2PD_PID}")

stop_group() {
  local pid="${1:-}"
  [[ -z "${pid}" ]] && return 0
  kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null || true
}

cleanup() {
  local pid
  for pid in "${CHILD_PIDS[@]:-}"; do
    stop_group "${pid}"
  done
  for pid in "${CHILD_PIDS[@]:-}"; do
    wait "${pid}" 2>/dev/null || true
  done
  # Scratch holds private router keys (router.keys, ssu2.keys); remove it.
  # Evidence keeps only sanitized lengths/digests/counters plus i2pd.log.
  [[ -z "${SCRATCH:-}" || ! -d "${SCRATCH}" ]] || rm -rf "${SCRATCH}"
}
trap cleanup EXIT

echo "==> waiting for ephemeral i2pd on 127.0.0.1:${I2PD_PORT}"
I2PD_RI=""
for _ in $(seq 1 120); do
  if [[ -f "${I2PD_DATA}/router.info" ]] &&
     grep -Fq "Start listening on 127.0.0.1:${I2PD_PORT}" "${I2PD_LOG}" 2>/dev/null; then
    I2PD_RI="${I2PD_DATA}/router.info"
    break
  fi
  if ! kill -0 "${I2PD_PID}" 2>/dev/null; then
    echo "ephemeral i2pd exited during startup" >&2
    sed -n '1,40p' "${I2PD_LOG}" >&2 || true
    exit 2
  fi
  sleep 0.5
done
if [[ -z "${I2PD_RI}" ]]; then
  echo "ephemeral i2pd did not publish router.info / SSU2 listener" >&2
  sed -n '1,40p' "${I2PD_LOG}" >&2 || true
  exit 2
fi
echo "    i2pd: 127.0.0.1:${I2PD_PORT} ($(wc -c <"${I2PD_RI}")-byte router.info)"

REQUIRED_FAILED=0
record() {
  local label="$1"
  local status="$2"
  local detail="${3:-}"
  detail="${detail//$'\t'/ }"
  detail="${detail//$'\n'/ }"
  printf '%s\t%s\t%s\n' "${label}" "${status}" "${detail}" >> "${RESULTS_FILE}"
  [[ "${status}" == "passed" ]] || REQUIRED_FAILED=1
}

# Plan 161 §13: the only sanctioned path from an executed command to a
# required `passed` row. The caller captures the command's exit code in
# `rc` first; a zero code records passed, anything else records failed.
# Direct literal `record "<required-label>" passed` lines are rejected by
# scripts/check-ssu2-acceptance-evidence.sh.
record_guarded() {
  local label="$1"
  local detail="$2"
  local rc="$3"
  if [[ "${rc}" -eq 0 ]]; then
    record "${label}" passed "${detail}"
  else
    record "${label}" failed "${detail} (exit ${rc})"
  fi
}

echo "==> local SSU2 rows (Plans 155-160 focused suites)"
REALUDP_LOG="${EVIDENCE_DIR}/local-real-udp.log"
: > "${REALUDP_LOG}"
realudp_rc=0
cargo test --locked -p i2pr-runtime --test ssu2_local -- \
  --test-threads=1 >>"${REALUDP_LOG}" 2>&1 || realudp_rc=$?
record_guarded "local-ssu2-real-udp" \
  "real-loopback i2pr<->i2pr session product (cargo test -p i2pr-runtime --test ssu2_local)" \
  "${realudp_rc}"

LOSS_LOG="${EVIDENCE_DIR}/local-loss-reorder.log"
: > "${LOSS_LOG}"
loss_rc=0
cargo test --locked -p i2pr-transport-ssu2 --test data_phase -- \
  --test-threads=1 >>"${LOSS_LOG}" 2>&1 || loss_rc=$?
record_guarded "local-loss-reorder" \
  "data-phase fault trajectories incl. loss/reorder/duplicate (cargo test -p i2pr-transport-ssu2 --test data_phase)" \
  "${loss_rc}"

PATH_LOG="${EVIDENCE_DIR}/local-path-validation.log"
: > "${PATH_LOG}"
path_rc=0
cargo test --locked -p i2pr-transport-ssu2 --test path_validation -- \
  --test-threads=1 >>"${PATH_LOG}" 2>&1 || path_rc=$?
record_guarded "path-validation" \
  "authenticated path validation sealed-packet trajectories (cargo test -p i2pr-transport-ssu2 --test path_validation)" \
  "${path_rc}"

SELECT_LOG="${EVIDENCE_DIR}/local-transport-selection.log"
: > "${SELECT_LOG}"
select_rc=0
cargo test --locked -p i2pr-transport --all-targets \
  >>"${SELECT_LOG}" 2>&1 || select_rc=$?
record_guarded "transport-selection" \
  "deterministic NTCP2/SSU2 selection + reachability policy (cargo test -p i2pr-transport --all-targets)" \
  "${select_rc}"

PEERTEST_LOG="${EVIDENCE_DIR}/local-peer-test.log"
: > "${PEERTEST_LOG}"
peertest_rc=0
cargo test --locked -p i2pr-transport-ssu2 --test peer_relay -- \
  --test-threads=1 >>"${PEERTEST_LOG}" 2>&1 || peertest_rc=$?
record_guarded "peer-test" \
  "PeerTest roles sealed-packet trajectories (cargo test -p i2pr-transport-ssu2 --test peer_relay)" \
  "${peertest_rc}"

RELAY_LOG="${EVIDENCE_DIR}/local-relay.log"
: > "${RELAY_LOG}"
relay_rc=0
cargo test --locked -p i2pr-runtime --test ssu2_peer_relay -- \
  --test-threads=1 >>"${RELAY_LOG}" 2>&1 || relay_rc=$?
record_guarded "relay" \
  "real-UDP NAT-like relay reachability (cargo test -p i2pr-runtime --test ssu2_peer_relay)" \
  "${relay_rc}"

echo "==> Plan 155-160 focused regressions"
REGRESS_LOG="${EVIDENCE_DIR}/plan155-160-regressions.log"
: > "${REGRESS_LOG}"
regress_rc=0
cargo test --locked -p i2pr-transport-ssu2 --test handshake -- \
  --test-threads=1 >>"${REGRESS_LOG}" 2>&1 || regress_rc=$?
if ! cargo test --locked -p i2pr-runtime --lib \
  >>"${REGRESS_LOG}" 2>&1; then
  echo "REGRESSION FAILED: -p i2pr-runtime --lib" >>"${REGRESS_LOG}"
  regress_rc=1
fi
record_guarded "plan155-160-focused-regressions" \
  "handshake/token establishment + runtime lib floor (see plan155-160-regressions.log)" \
  "${regress_rc}"

echo "==> external matrix against exact-pinned i2pd (explicit ignored selection)"
DRIVER_EVIDENCE="${EVIDENCE_DIR}/driver"
mkdir -p "${DRIVER_EVIDENCE}"
DRIVER_LOG="${EVIDENCE_DIR}/external-driver.log"
: > "${DRIVER_LOG}"
driver_rc=0
# The dedicated external lane opts in explicitly; missing environment
# stays fail-closed inside the driver (no early-success path).
if I2PD_ROUTER_INFO="${I2PD_RI}" \
   I2PD_SSU2_ENDPOINT="127.0.0.1:${I2PD_PORT}" \
   I2PR_SSU2_BIND="127.0.0.1:${I2PR_PORT}" \
   I2PR_SSU2_FLOODFILL=1 \
   EVIDENCE_DIR="${DRIVER_EVIDENCE}" \
   timeout --foreground "${DRIVER_TIMEOUT}" \
   cargo test --locked -p i2pr-runtime --test ssu2_independent \
   ssu2_independent_ipv4_interop -- --ignored --exact --nocapture --test-threads=1 \
   >>"${DRIVER_LOG}" 2>&1; then
  driver_rc=0
else
  driver_rc=$?
fi
DRIVER_TSV="${DRIVER_EVIDENCE}/driver-evidence.tsv"
# Each external row derives from the driver exit status plus its own
# sanitized evidence keys: the suite must be green AND the row's keys
# must be present. A passing suite with missing keys fails its row.
ssu2_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  local rc=1
  if [[ "${driver_rc}" -eq 0 && -f "${DRIVER_TSV}" ]] &&
     grep -Fq "${key}" "${DRIVER_TSV}"; then
    rc=0
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}
ssu2_row "external-i2pd-i2pr-to-i2pd" "direction-a-established" \
  "i2pr initiator -> i2pd responder tokenless Retry establishment + mutual auth (explicit --ignored --exact driver)"
ssu2_row "external-i2pd-i2pd-to-i2pr" "direction-b-established" \
  "i2pd initiator -> i2pr responder promotion through the normal token/Retry path"
ssu2_row "external-i2np-small" "small-i2np-digest" \
  "small single-datagram DatabaseStore with direct DeliveryStatus echo per direction"
ssu2_row "external-i2np-fragmented" "large-i2np-digest" \
  "fragmented DatabaseStore with direct DeliveryStatus echo per direction"
ssu2_row "token-retry" "direction-a2-cached-token" \
  "tokenless Retry path plus cached-token second dial (expired/invalid/source rows stay local-evidence-only)"
ssu2_row "malformed-cheap-drop" "malformed-cheap-drops" \
  "short/oversized/random datagrams rejected without session creation"
ssu2_row "resource-baseline" "resource-sessions-closed" \
  "graceful termination returns sessions/tasks/pending state to baseline per direction"

echo "==> workspace gates slice"
GATES_LOG="${EVIDENCE_DIR}/workspace-gates.log"
: > "${GATES_LOG}"
gates_rc=0
cargo fmt --all --check >>"${GATES_LOG}" 2>&1 || gates_rc=1
cargo check --locked --workspace --all-targets >>"${GATES_LOG}" 2>&1 || gates_rc=1
for gate in check-dependency-direction check-runtime-boundaries check-fixture-manifest \
           check-ntcp2-vectors check-ssu2-vectors check-ntcp2-interoperability \
           check-constrained-host-lane-boundary check-sam-acceptance-evidence \
           check-ssu2-acceptance-evidence; do
  if ! bash "${REPO_ROOT}/scripts/${gate}.sh" >>"${GATES_LOG}" 2>&1; then
    echo "GATE FAILED: ${gate}.sh" >>"${GATES_LOG}"
    gates_rc=1
  fi
done
record_guarded "workspace-gates" \
  "fmt + workspace check --all-targets + static boundary scripts (full test/clippy/doc/deny floor stays in routine CI)" \
  "${gates_rc}"

python3 - "${RESULTS_FILE}" "${EVIDENCE_DIR}" "${REPO_ROOT}" "${I2PD_PIN}" "${I2PD_VERSION}" "${JAVA_PIN}" "${JAVA_VERSION}" <<'PY'
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

results_path, evidence_dir, repo_root = sys.argv[1:4]
i2pd_pin, i2pd_version, java_pin, java_version = sys.argv[4:8]
rows = []
with open(results_path, encoding="utf-8") as stream:
    for line in stream:
        label, status, detail = line.rstrip("\n").split("\t", 2)
        rows.append({"label": label, "status": status, "detail": detail})

commit = subprocess.check_output(
    ["git", "-C", repo_root, "rev-parse", "HEAD"], text=True
).strip()
rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
driver_keys = []
driver_tsv = Path(evidence_dir) / "driver" / "driver-evidence.tsv"
if driver_tsv.exists():
    for line in driver_tsv.read_text(encoding="utf-8").splitlines():
        label = line.split("\t", 1)[0]
        driver_keys.append(label)
evidence = {
    "schema": "i2pr-ssu2-external-v1",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": "ssu2-external",
    "ssu2_bind_policy": "127.0.0.1 ephemeral loopback only",
    "i2pd": {
        "repository": "https://github.com/PurpleI2P/i2pd.git",
        "revision": i2pd_pin,
        "version": i2pd_version,
        "build_command": "make USE_UPNP=no DEBUG=0 (via scripts/interop/fetch-ssu2-reference.sh)",
        "role": "mandatory independent SSU2 v2 reference, unmodified",
    },
    "java_i2p": {
        "repository": "https://github.com/i2p/i2p.i2p.git",
        "revision": java_pin,
        "version": java_version,
        "status": "deferred-narrow-orchestration-debt",
        "reason": "nonblocking per Plan 161 section 12: no narrow unprivileged standalone SSU2 driver exists; a JVM/Gradle router orchestration would recreate the harness build-up the plan forbids",
    },
    "driver_evidence_keys": driver_keys,
    "results": rows,
    "ssu2_direct_ipv4": "passed-via-i2pd-2.61.0" if all(
        row["status"] == "passed" for row in rows
    ) else "failed",
    "known_limitations": [
        "direct loopback SSU2 v2 session evidence only; no public I2P participation",
        "no NetDB lookup/publication over external peers; no tunnel/destination/Streaming mixed-router claim",
        "expired/invalid/source-bound token rows stay local-evidence-only in the Plan 156/158 suites",
        "unsupported-version/spoofed-source/tag-corruption rows beyond the compact probe stay local-evidence-only in the Plan 157 suite",
        "IPv6 external interop is infrastructure-limited debt; PQ-hybrid v3/v4 deferred; SSU1 unsupported",
        "Java I2P secondary reference deferred as nonblocking narrow-orchestration debt",
        "workspace-gates in this lane covers fmt, workspace check --all-targets, and static boundary scripts; the full test/clippy/doc/deny floor runs in routine CI",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 161 SSU2 evidence (local suites + independent i2pd matrix)\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- i2pd: `{i2pd_version}` @ `{i2pd_pin}` (unmodified)\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only\n")
    stream.write("- Java I2P: deferred narrow-orchestration debt (nonblocking)\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
  echo "Plan 161 SSU2 lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 161 SSU2 lane passed; sanitized evidence: ${EVIDENCE_DIR}"
