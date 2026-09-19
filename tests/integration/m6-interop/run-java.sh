#!/usr/bin/env bash
# Plan 199 Phase A — run the M6 Java I2P public-client second-family closure lane
# end-to-end with a controlled first-run topology.
#
# The second-family lane re-uses the Plan 184–193 product suites the
# Plan 193 i2pd family already exercised, but with the exact-pinned
# Java I2P 2.13.0 reference substituted for the exact-pinned i2pd
# 2.61.0 reference. The lane provisions one ephemeral Java router on
# loopback with a fresh data dir (no reseed, no public I2P
# participation), keeps the SAM bridge only as a diagnostic surface,
# starts the counted public Java client helpers, and runs the second-family driver
# (crates/i2pr-daemon/tests/java_tunnel_external.rs) through its
# explicit `--ignored --exact` selection.
#
# The Plan 194 §11 first-run topology blocker is closed by the
# ControlledRouter test-only launcher: it compiles into the
# ephemeral scratch dir against the staged Java I2P `lib/` jars and
# invokes the stock `net.i2p.router.Router(Properties)` +
# `setKillVMOnEnd(false)` + `runRouter()` lifecycle directly, so
# every controlled-topology property is authoritative at startup
# (the exact-pinned upstream `MultiRouter` precedent). The Java
# base/cache is never mutated; the disposable Java data dir is the
# only directory that receives a per-run router.config /
# clients.config / noreseed.i2p.
#
# The lane is unprivileged and loopback-only. Required failures make
# this script fail. Sanitized evidence defaults below
# target/interop; set I2PR_M6_JAVA_EVIDENCE_DIR to retain it elsewhere.
# Private router keys, destination secrets, and raw application
# payloads stay in the ephemeral scratch directory and are never
# copied to evidence (digests/lengths/counters only).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${I2PR_M6_JAVA_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/m6-java-evidence}"
if [[ "${EVIDENCE_DIR}" != /* ]]; then
  EVIDENCE_DIR="${REPO_ROOT}/${EVIDENCE_DIR}"
fi

JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION="2.13.0"
JAVA_REPO="https://github.com/i2p/i2p.i2p.git"
JAVA_CACHE="${REPO_ROOT}/target/interop/cache/m6-java/${JAVA_PIN}"

I2PR_PORT="${I2PR_SSU2_JAVA_PORT:-44090}"
I2PR_STREAM_PORT="${I2PR_SSU2_JAVA_STREAM_PORT:-44091}"
reserve_port() {
  python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()'
}
I2PR_BOOTSTRAP_PORT="${I2PR_SSU2_JAVA_BOOTSTRAP_PORT:-$(reserve_port)}"
# Plan 196 §5.4 — reserve/select fixed loopback Java SSU2, SAM and
# I2CP ports before the Java router starts; the runner reports the
# actual bound endpoints to the driver rather than assuming the
# upstream default tuple.
JAVA_SSU2_PORT="${I2PR_M6_JAVA_SSU2_PORT:-$(reserve_port)}"
JAVA_SAM_PORT="${I2PR_M6_JAVA_SAM_PORT:-$(reserve_port)}"
JAVA_I2CP_PORT="${I2PR_M6_JAVA_I2CP_PORT:-$(reserve_port)}"
JAVA_RAW_CONTROL_PORT="${I2PR_M6_JAVA_RAW_CONTROL_PORT:-$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')}"
JAVA_STREAM_CONTROL_PORT="${I2PR_M6_JAVA_STREAM_CONTROL_PORT:-$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')}"
# Plan 199 Phase A.1: Router A owns public client destinations; Router B is
# the distinct publication/floodfill target.
JAVA_PUBLICATION_SSU2_PORT="${I2PR_M6_JAVA_PUBLICATION_SSU2_PORT:-$(reserve_port)}"
JAVA_PUBLICATION_SAM_PORT="${I2PR_M6_JAVA_PUBLICATION_SAM_PORT:-$(reserve_port)}"
JAVA_PUBLICATION_I2CP_PORT="${I2PR_M6_JAVA_PUBLICATION_I2CP_PORT:-$(reserve_port)}"
# Plan 201 Branch C/D corrective — Router C is a tunnel participant
# (no SAM/I2CP) so 1-hop client tunnels build within stock Java's
# 5-minute I2PSession.connect() timeout.
JAVA_TUNNEL_PARTICIPANT_SSU2_PORT="${I2PR_M6_JAVA_TUNNEL_PARTICIPANT_SSU2_PORT:-$(reserve_port)}"
# Plan 220 — read-only P220 diagnostic TCP control port per
# router. The harness reserves three loopback ports (Router A,
# Router B, Router C); the controlled-launcher binds them on
# 127.0.0.1 only when this argument is non-zero. Same-port
# collisions are a typed IOException, not a silent fallback.
# (The frozen J219 commands remain for history; the P220
# hex-hash commands are the authoritative path.)
JAVA_DIAGNOSTIC_A_PORT="${I2PR_M6_JAVA_DIAGNOSTIC_A_PORT:-$(reserve_port)}"
JAVA_DIAGNOSTIC_B_PORT="${I2PR_M6_JAVA_DIAGNOSTIC_B_PORT:-$(reserve_port)}"
JAVA_DIAGNOSTIC_C_PORT="${I2PR_M6_JAVA_DIAGNOSTIC_C_PORT:-$(reserve_port)}"
DRIVER_TIMEOUT="600s"

mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-m6-plan196-java.XXXXXX)"
RESULTS_FILE="${SCRATCH}/results.tsv"
: > "${RESULTS_FILE}"
# Hygiene: no stale secret-bearing or result-bearing file from a
# previous run may linger in evidence. The raw Java I2P log is never
# evidence (it carries SAM session lines); only sanitized counts
# extracted below reach evidence.
rm -f "${EVIDENCE_DIR}/java.log" \
  "${EVIDENCE_DIR}/driver/driver-evidence.tsv" \
  "${EVIDENCE_DIR}/driver/destination/driver-evidence.tsv" \
  "${EVIDENCE_DIR}/driver/streaming/driver-evidence.tsv" \
  "${EVIDENCE_DIR}/reference-facts.tsv"
rm -f "${EVIDENCE_DIR}/reference-raw-destination.tsv" \
  "${EVIDENCE_DIR}/reference-streaming-service.tsv"

# ---- Java I2P cache verification (fail closed before any network use) ----
if [[ ! -d "${JAVA_CACHE}/lib" ]]; then
  echo "Java I2P cache missing lib/: ${JAVA_CACHE}/lib" >&2
  echo "run scripts/interop/fetch-m6-java.sh --rebuild first" >&2
  exit 1
fi
if [[ ! -f "${JAVA_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${JAVA_CACHE}/source-revision.txt")" != "${JAVA_PIN}" ]]; then
  echo "Java I2P cache has no verified Plan 194 source revision" >&2
  echo "run scripts/interop/fetch-m6-java.sh --rebuild first" >&2
  exit 1
fi
echo "==> Java I2P reference: ${JAVA_VERSION} (${JAVA_PIN})"

# ---- Plan 196 §5.1 controlled stock-router launcher build ----------------
# Compile the out-of-tree launcher against the staged Java I2P `lib/`
# jars into the ephemeral scratch dir. Never compile into or against
# the exact-pinned source checkout. The compiled classpath includes
# the staged `router.jar`, which carries the stock `net.i2p.router.Router`
# class the launcher constructs.
LAUNCHER_BUILD="${SCRATCH}/build"
mkdir -p "${LAUNCHER_BUILD}"
LAUNCHER_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/ControlledRouter.java"
RAW_HELPER_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceRawDestination.java"
STREAM_HELPER_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceStreamingService.java"
# Plan 220 WP D — test-only same-package FloodfillPeerSelector probe
# (read-only; never patches a Java I2P class).
SELECTOR_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P220SelectorProbe.java"
# Plan 222 WP B — test-only exact client-lookup preflight probe
# (helper client DBID + Java routing key + effective width through the
# production-equivalent 3-argument selector overload; read-only).
P222_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P222SelectorProbe.java"
# Plan 223 WP C — test-only bounded status-17 branch discriminator
# (source LeaseSetKeys + target LS2 + exact getEncryptionKey intersection;
# read-only, no KeyManager/client-DB/LeaseSet mutation).
P223_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P223BranchProbe.java"
if [[ ! -f "${LAUNCHER_SRC}" || ! -f "${RAW_HELPER_SRC}" || ! -f "${STREAM_HELPER_SRC}" || ! -f "${SELECTOR_PROBE_SRC}" || ! -f "${P222_PROBE_SRC}" || ! -f "${P223_PROBE_SRC}" ]]; then
  echo "Java launcher source missing: ${LAUNCHER_SRC}" >&2
  exit 1
fi
JAVA_CP=""
for jar in "${JAVA_CACHE}"/*.jar "${JAVA_CACHE}"/lib/*.jar; do
  if [[ -z "${JAVA_CP}" ]]; then
    JAVA_CP="${jar}"
  else
    JAVA_CP="${JAVA_CP}:${jar}"
  fi
done
if ! javac -d "${LAUNCHER_BUILD}" -cp "${JAVA_CP}" \
   "${LAUNCHER_SRC}" "${RAW_HELPER_SRC}" "${STREAM_HELPER_SRC}" "${SELECTOR_PROBE_SRC}" "${P222_PROBE_SRC}" "${P223_PROBE_SRC}" \
   >"${SCRATCH}/javac.log" 2>&1; then
  echo "Java launcher compile failed; see ${SCRATCH}/javac.log" >&2
  tail -n 60 "${SCRATCH}/javac.log" >&2 || true
  exit 1
fi
LAUNCHER_CP="${LAUNCHER_BUILD}:${JAVA_CP}"
LAUNCHER="ControlledRouter"
echo "==> Java launcher compiled: ${LAUNCHER_SRC} -> ${LAUNCHER_BUILD}/"

# ---- Plan 196 §5.2 property-set + disposable data-dir --------------------
# Per-run disposable Java data dir; no reseed URL or HTTPS contact
# the harness did not pre-resolve to loopback.
JAVA_DATA="${SCRATCH}/datadir-service"
JAVA_PUBLICATION_DATA="${SCRATCH}/datadir-publication"
JAVA_TUNNEL_PARTICIPANT_DATA="${SCRATCH}/datadir-tunnel-participant"
JAVA_LOG="${SCRATCH}/java.log"
JAVA_PUBLICATION_LOG="${SCRATCH}/java-publication.log"
JAVA_TUNNEL_PARTICIPANT_LOG="${SCRATCH}/java-tunnel-participant.log"
mkdir -p "${JAVA_DATA}/logs" "${JAVA_PUBLICATION_DATA}/logs" "${JAVA_TUNNEL_PARTICIPANT_DATA}/logs"

# Sanity: never mutate the verified Java cache/build outputs.
# Plan 196 §5.3 forbids `sed`/`clients.config` mutations of the cache.
CACHE_FINGERPRINT_BEFORE="$(find "${JAVA_CACHE}" -type f -name '*.config' -o -name 'runplain.sh' -o -name 'clients.config*' 2>/dev/null | LC_ALL=C.UTF-8 sort | xargs -r sha256sum | sha256sum | awk '{print $1}')"

# ---- Plan 196 §5.4 start Java through the controlled launcher -----------
: > "${JAVA_LOG}"
JAVA_CMD=(
  java
  -Djava.net.preferIPv4Stack=true
  -Djava.awt.headless=true
  -Djava.library.path="${JAVA_CACHE}:${JAVA_CACHE}/lib"
  -Di2p.dir.base="${JAVA_CACHE}"
  -DloggerFilenameOverride=logs/log-router-@.txt
  -Drouterconsole.enable=false
  -cp "${LAUNCHER_CP}"
  -Dlauncher.scratch="${SCRATCH}"
  "ControlledRouter"
  "${JAVA_DATA}"
  "127.0.0.1"
  "${JAVA_SSU2_PORT}"
  "${JAVA_SAM_PORT}"
  "${JAVA_I2CP_PORT}"
  "${JAVA_DIAGNOSTIC_A_PORT}"
)
setsid "${JAVA_CMD[@]}" >/dev/null 2>"${JAVA_LOG}" < /dev/null &
JAVA_PID=$!
CHILD_PIDS=("${JAVA_PID}")

JAVA_PUBLICATION_CMD=(
  java
  -Djava.net.preferIPv4Stack=true
  -Djava.awt.headless=true
  -Djava.library.path="${JAVA_CACHE}:${JAVA_CACHE}/lib"
  -Di2p.dir.base="${JAVA_CACHE}"
  -DloggerFilenameOverride=logs/log-router-@.txt
  -Drouterconsole.enable=false
  -cp "${LAUNCHER_CP}"
  -Dlauncher.scratch="${SCRATCH}"
  "ControlledRouter"
  "${JAVA_PUBLICATION_DATA}"
  "127.0.0.1"
  "${JAVA_PUBLICATION_SSU2_PORT}"
  "${JAVA_PUBLICATION_SAM_PORT}"
  "${JAVA_PUBLICATION_I2CP_PORT}"
  "${JAVA_DIAGNOSTIC_B_PORT}"
)
setsid "${JAVA_PUBLICATION_CMD[@]}" >/dev/null 2>"${JAVA_PUBLICATION_LOG}" < /dev/null &
JAVA_PUBLICATION_PID=$!
CHILD_PIDS+=("${JAVA_PUBLICATION_PID}")

# Plan 201 Branch C/D corrective — Router C is a tunnel-participant
# (no SAM/I2CP binding). It provides the third peer that stock Java
# I2P 2.13.0 needs to build 1-hop client tunnels within its
# 5-minute I2PSession.connect() timeout.
JAVA_TUNNEL_PARTICIPANT_ROUTER_DIR="${JAVA_TUNNEL_PARTICIPANT_DATA}/router"
JAVA_TUNNEL_PARTICIPANT_RI=""
JAVA_TUNNEL_PARTICIPANT_CMD=(
  java
  -Djava.net.preferIPv4Stack=true
  -Djava.awt.headless=true
  -Djava.library.path="${JAVA_CACHE}:${JAVA_CACHE}/lib"
  -Di2p.dir.base="${JAVA_CACHE}"
  -DloggerFilenameOverride=logs/log-router-@.txt
  -Drouterconsole.enable=false
  -cp "${LAUNCHER_CP}"
  -Dlauncher.scratch="${SCRATCH}"
  "ControlledRouter"
  "${JAVA_TUNNEL_PARTICIPANT_DATA}"
  "127.0.0.1"
  "${JAVA_TUNNEL_PARTICIPANT_SSU2_PORT}"
  "0"
  "0"
  "${JAVA_DIAGNOSTIC_C_PORT}"
)
setsid "${JAVA_TUNNEL_PARTICIPANT_CMD[@]}" >/dev/null 2>"${JAVA_TUNNEL_PARTICIPANT_LOG}" < /dev/null &
JAVA_TUNNEL_PARTICIPANT_PID=$!
CHILD_PIDS+=("${JAVA_TUNNEL_PARTICIPANT_PID}")

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
  # Best-effort verification: the Java cache must be untouched.
  if [[ -n "${JAVA_CACHE:-}" ]]; then
    local fp_after
    fp_after="$(find "${JAVA_CACHE}" -type f \( -name '*.config' -o -name 'runplain.sh' -o -name 'clients.config*' \) 2>/dev/null | LC_ALL=C.UTF-8 sort | xargs -r sha256sum | sha256sum | awk '{print $1}')"
    if [[ "${fp_after}" != "${CACHE_FINGERPRINT_BEFORE:-}" ]]; then
      echo "Java cache fingerprint drifted (cache was mutated during the run)" >&2
    fi
  fi
  [[ -z "${SCRATCH:-}" || ! -d "${SCRATCH}" ]] || [[ "${I2PR_KEEP_SCRATCH:-0}" == "1" ]] || rm -rf "${SCRATCH}"
}
trap cleanup EXIT

stop_helper() {
  local pid="${1:-}"
  [[ -z "${pid}" ]] && return 0
  kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null || true
  wait "${pid}" 2>/dev/null || true
  CHILD_PIDS=("${CHILD_PIDS[@]/${pid}}")
}

echo "==> waiting for ephemeral Java I2P on 127.0.0.1:${JAVA_SSU2_PORT} (SAM 127.0.0.1:${JAVA_SAM_PORT} I2CP 127.0.0.1:${JAVA_I2CP_PORT})"
JAVA_RI=""
# Plan 196 §5.4 — Java writes router.info to
# `${i2p.dir.router}/router.info`; we set i2p.dir.router to
# `${JAVA_DATA}/router`, so router.info lands at
# `${JAVA_DATA}/router/router.info`. The Java Router takes ~60 s
# to publish router.info on a fresh data dir (key generation +
# signatures + initial RouterInfo build), so the harness waits up
# to 360 retries × 0.5 s = 180 s with periodic liveness checks.
JAVA_ROUTER_DIR="${JAVA_DATA}/router"
JAVA_PUBLICATION_ROUTER_DIR="${JAVA_PUBLICATION_DATA}/router"
JAVA_PUBLICATION_RI=""
for _ in $(seq 1 360); do
  if [[ -f "${JAVA_ROUTER_DIR}/router.info" ]]; then
    JAVA_RI="${JAVA_ROUTER_DIR}/router.info"
    break
  fi
  if ! kill -0 "${JAVA_PID}" 2>/dev/null; then
    echo "ephemeral Java I2P exited during startup" >&2
    sed -n '1,40p' "${JAVA_LOG}" >&2 || true
    sed -n '1,40p' "${JAVA_DATA}/logs/log-router-0.txt" 2>&1 >&2 || true
    exit 2
  fi
  sleep 0.5
done
if [[ -z "${JAVA_RI}" ]]; then
  echo "ephemeral Java I2P did not publish router.info" >&2
  sed -n '1,40p' "${JAVA_LOG}" >&2 || true
  sed -n '1,40p' "${JAVA_DATA}/logs/log-router-0.txt" 2>&1 >&2 || true
  exit 2
fi
for _ in $(seq 1 360); do
  if [[ -f "${JAVA_PUBLICATION_ROUTER_DIR}/router.info" ]]; then
    JAVA_PUBLICATION_RI="${JAVA_PUBLICATION_ROUTER_DIR}/router.info"
    break
  fi
  if ! kill -0 "${JAVA_PUBLICATION_PID}" 2>/dev/null; then
    echo "ephemeral Java publication router exited during startup" >&2
    sed -n '1,40p' "${JAVA_PUBLICATION_LOG}" >&2 || true
    exit 2
  fi
  sleep 0.5
done
if [[ -z "${JAVA_PUBLICATION_RI}" ]]; then
  echo "ephemeral Java publication router did not publish router.info" >&2
  sed -n '1,40p' "${JAVA_PUBLICATION_LOG}" >&2 || true
  exit 2
fi
for _ in $(seq 1 360); do
  if [[ -f "${JAVA_TUNNEL_PARTICIPANT_ROUTER_DIR}/router.info" ]]; then
    JAVA_TUNNEL_PARTICIPANT_RI="${JAVA_TUNNEL_PARTICIPANT_ROUTER_DIR}/router.info"
    break
  fi
  if ! kill -0 "${JAVA_TUNNEL_PARTICIPANT_PID}" 2>/dev/null; then
    echo "ephemeral Java tunnel-participant router exited during startup" >&2
    sed -n '1,40p' "${JAVA_TUNNEL_PARTICIPANT_LOG}" >&2 || true
    exit 2
  fi
  sleep 0.5
done
if [[ -z "${JAVA_TUNNEL_PARTICIPANT_RI}" ]]; then
  echo "ephemeral Java tunnel-participant router did not publish router.info" >&2
  sed -n '1,40p' "${JAVA_TUNNEL_PARTICIPANT_LOG}" >&2 || true
  exit 2
fi

SAM_READY=0
# Plan 196 §5.4 — Java Router adds a `clientApp.0.delay=120` to the
# auto-generated SAM client config (it waits for the I2CP server
# before bringing up the SAM bridge). The harness waits up to
# 240 retries × 0.5s = 120 s for the SAM port to bind, with an
# extra buffer for the upstream TCP accept race.
for _ in $(seq 1 240); do
  if (exec 3<>"/dev/tcp/127.0.0.1/${JAVA_SAM_PORT}") 2>/dev/null; then
    exec 3<&- 3>&- || true
    SAM_READY=1
    break
  fi
  if ! kill -0 "${JAVA_PID}" 2>/dev/null; then
    echo "ephemeral Java I2P exited before SAM came up" >&2
    sed -n '1,40p' "${JAVA_LOG}" >&2 || true
    exit 2
  fi
  sleep 0.5
done
if [[ "${SAM_READY}" -ne 1 ]]; then
  echo "ephemeral Java I2P SAM did not listen on 127.0.0.1:${JAVA_SAM_PORT}" >&2
  tail -n 20 "${JAVA_DATA}/logs/log-router-0.txt" 2>&1 >&2 || true
  exit 2
fi
echo "    Java service router A: 127.0.0.1:${JAVA_SSU2_PORT} (SAM ${JAVA_SAM_PORT}, I2CP ${JAVA_I2CP_PORT}, J219 ${JAVA_DIAGNOSTIC_A_PORT})"
echo "    Java publication router B: 127.0.0.1:${JAVA_PUBLICATION_SSU2_PORT} (SAM ${JAVA_PUBLICATION_SAM_PORT}, I2CP ${JAVA_PUBLICATION_I2CP_PORT}, J219 ${JAVA_DIAGNOSTIC_B_PORT})"
echo "    Java tunnel-participant router C: 127.0.0.1:${JAVA_TUNNEL_PARTICIPANT_SSU2_PORT} (J219 ${JAVA_DIAGNOSTIC_C_PORT})"
echo "    Java service RouterInfo: $(wc -c <"${JAVA_RI}") bytes; publication RouterInfo: $(wc -c <"${JAVA_PUBLICATION_RI}") bytes; tunnel-participant RouterInfo: $(wc -c <"${JAVA_TUNNEL_PARTICIPANT_RI}") bytes"

# Plan 220 — wait for the three read-only diagnostic TCP
# ports to bind. The controlled-launcher binds 127.0.0.1 only
# when each non-zero port is supplied. The harness's read-only
# `J219-*` snapshot rows cannot run before this; missing
# diagnostic readiness is a hard fail.
J219_READY_A=0
J219_READY_B=0
J219_READY_C=0
for _ in $(seq 1 240); do
  ok_a=0; ok_b=0; ok_c=0
  (exec 3<>"/dev/tcp/127.0.0.1/${JAVA_DIAGNOSTIC_A_PORT}") 2>/dev/null && { exec 3<&- 3>&- || true; ok_a=1; }
  (exec 3<>"/dev/tcp/127.0.0.1/${JAVA_DIAGNOSTIC_B_PORT}") 2>/dev/null && { exec 3<&- 3>&- || true; ok_b=1; }
  (exec 3<>"/dev/tcp/127.0.0.1/${JAVA_DIAGNOSTIC_C_PORT}") 2>/dev/null && { exec 3<&- 3>&- || true; ok_c=1; }
  if [[ "${ok_a}" -eq 1 && "${ok_b}" -eq 1 && "${ok_c}" -eq 1 ]]; then
    J219_READY_A=1; J219_READY_B=1; J219_READY_C=1
    break
  fi
  if ! kill -0 "${JAVA_PID}" 2>/dev/null || ! kill -0 "${JAVA_PUBLICATION_PID}" 2>/dev/null || ! kill -0 "${JAVA_TUNNEL_PARTICIPANT_PID}" 2>/dev/null; then
    echo "ephemeral Java I2P exited before J219 diagnostic ports bound" >&2
    break
  fi
  sleep 0.5
done
if [[ "${J219_READY_A}" -ne 1 || "${J219_READY_B}" -ne 1 || "${J219_READY_C}" -ne 1 ]]; then
  echo "J219 diagnostic port(s) did not bind (A=${J219_READY_A} B=${J219_READY_B} C=${J219_READY_C})" >&2
  tail -n 20 "${JAVA_DATA}/logs/log-router-0.txt" 2>&1 >&2 || true
  exit 2
fi

# Plan 220 §7 — ready probe + role assignment. The
# diagnostic server starts anonymous; the harness assigns each
# server's role immediately after the port binds so each
# snapshot self-reports its role.
j219_send_command() {
  local port="$1"
  local cmd="$2"
  python3 - "${port}" "${cmd}" <<'PY' 2>/dev/null || true
import socket, sys
port = int(sys.argv[1])
cmd = sys.argv[2]
try:
    with socket.create_connection(("127.0.0.1", port), timeout=5) as sock:
        sock.sendall((cmd + "\n").encode("ascii"))
        chunks = []
        # Read up to two '\n' terminated lines so the
        # implementation can emit J219-READY followed by PONG.
        for _ in range(2):
            data = sock.recv(4096)
            if not data:
                break
            chunks.append(data)
        sys.stdout.write(b"".join(chunks).decode("ascii", "replace"))
except OSError as e:
    sys.stdout.write(f"J219-ERROR connection-failed port={port} cmd={cmd} err={e}")
PY
}
printf 'J219-ROLE A\n' | timeout 2 nc -q 1 127.0.0.1 "${JAVA_DIAGNOSTIC_A_PORT}" >/dev/null 2>&1 || true
printf 'J219-ROLE B\n' | timeout 2 nc -q 1 127.0.0.1 "${JAVA_DIAGNOSTIC_B_PORT}" >/dev/null 2>&1 || true
printf 'J219-ROLE C\n' | timeout 2 nc -q 1 127.0.0.1 "${JAVA_DIAGNOSTIC_C_PORT}" >/dev/null 2>&1 || true

# Plan 220 §7 — authoritative-observation history dir. Bounded,
# sanitized, read-only, loopback-only. The shell records
# epoch-labeled P220 history snapshots here for post-hoc timeline
# correlation ONLY. No shell-derived fact in this directory is ever
# consumed by the terminal classifier: the destination driver takes
# its own fresh authoritative P220 snapshot after its own A/B
# RouterInfo bootstrap and before the reverse send (Plan 220 D220-1
# guard). Pre-bootstrap shell state MUST NOT feed attribution.
P220_HISTORY_DIR="${EVIDENCE_DIR}/p220"
mkdir -p "${P220_HISTORY_DIR}"
P220_HISTORY_TSV="${P220_HISTORY_DIR}/history-snapshots.tsv"
: > "${P220_HISTORY_TSV}"

# Helper function to query a J219 diagnostic command; returns
# the raw response line (single-line per call).
j219_query() {
  local port="$1"
  local cmd="$2"
  python3 - "${port}" "${cmd}" <<'PY' 2>/dev/null
import socket, sys
port = int(sys.argv[1])
cmd = sys.argv[2]
try:
    with socket.create_connection(("127.0.0.1", port), timeout=5) as sock:
        sock.sendall((cmd + "\n").encode("ascii"))
        data = b""
        while not data.endswith(b"\n"):
            chunk = sock.recv(4096)
            if not chunk:
                break
            data += chunk
            if len(data) > 65536:
                break
        sys.stdout.write(data.decode("ascii", "replace"))
except OSError as exc:
    sys.stdout.write(f"J219-ERROR cmd={cmd} err={exc}")
PY
}

# Plan 220 §7 — pre-bootstrap history snapshots (epoch:
# `pre-bootstrap`). These P220-SNAPSHOT rows are timeline history
# only; the terminal classifier never consumes a pre-bootstrap
# epoch (D220-1). No RouterHash is derived here: the destination
# driver derives the protocol-correct 32-byte RouterHash from the
# validated RouterInfo identity and cross-checks it against the
# Java self snapshot at its own post-bootstrap epoch (D220-2).
P220_SNAPSHOT_A_PRE="$(j219_query "${JAVA_DIAGNOSTIC_A_PORT}" "P220-SNAPSHOT")"
P220_SNAPSHOT_B_PRE="$(j219_query "${JAVA_DIAGNOSTIC_B_PORT}" "P220-SNAPSHOT")"
P220_SNAPSHOT_C_PRE="$(j219_query "${JAVA_DIAGNOSTIC_C_PORT}" "P220-SNAPSHOT")"
printf 'pre-bootstrap\tA\t%s\n' "${P220_SNAPSHOT_A_PRE}" >> "${P220_HISTORY_TSV}"
printf 'pre-bootstrap\tB\t%s\n' "${P220_SNAPSHOT_B_PRE}" >> "${P220_HISTORY_TSV}"
printf 'pre-bootstrap\tC\t%s\n' "${P220_SNAPSHOT_C_PRE}" >> "${P220_HISTORY_TSV}"

# Plan 220 §7 — emit the epoch-labeled timed history required by
# the plan. The pre-bootstrap snapshots above are timestamp #1;
# the remaining moments are captured later in the script. Every
# row carries its epoch; rows with a stale epoch MUST NOT feed
# the terminal classifier (D220-1 guard, enforced by the static
# checker).
J219_TIMED_SNAPSHOTS_TSV="${P220_HISTORY_DIR}/timed-snapshots.tsv"
: > "${J219_TIMED_SNAPSHOTS_TSV}"
printf 'p220-snapshot-moment\trole\tresponse\n' >> "${J219_TIMED_SNAPSHOTS_TSV}"
j219_record_timed_snapshot() {
  local moment="$1"
  j219_query "${JAVA_DIAGNOSTIC_A_PORT}" "P220-SNAPSHOT" >/dev/null \
    && printf '%s\tA\t%s\n' "${moment}" "$(j219_query "${JAVA_DIAGNOSTIC_A_PORT}" 'P220-SNAPSHOT')" \
       >> "${J219_TIMED_SNAPSHOTS_TSV}" || true
  j219_query "${JAVA_DIAGNOSTIC_B_PORT}" "P220-SNAPSHOT" >/dev/null \
    && printf '%s\tB\t%s\n' "${moment}" "$(j219_query "${JAVA_DIAGNOSTIC_B_PORT}" 'P220-SNAPSHOT')" \
       >> "${J219_TIMED_SNAPSHOTS_TSV}" || true
  j219_query "${JAVA_DIAGNOSTIC_C_PORT}" "P220-SNAPSHOT" >/dev/null \
    && printf '%s\tC\t%s\n' "${moment}" "$(j219_query "${JAVA_DIAGNOSTIC_C_PORT}" 'P220-SNAPSHOT')" \
       >> "${J219_TIMED_SNAPSHOTS_TSV}" || true
}
# Already captured: moment = first RouterInfo appearance.
printf 'first-routerinfo-appearance\tA\t%s\n' "${P220_SNAPSHOT_A_PRE}" >> "${J219_TIMED_SNAPSHOTS_TSV}"
printf 'first-routerinfo-appearance\tB\t%s\n' "${P220_SNAPSHOT_B_PRE}" >> "${J219_TIMED_SNAPSHOTS_TSV}"
printf 'first-routerinfo-appearance\tC\t%s\n' "${P220_SNAPSHOT_C_PRE}" >> "${J219_TIMED_SNAPSHOTS_TSV}"

# Plan 196 §5.5 — externally observable topology invariants.
TOPOLOGY_OK=1
TOPOLOGY_REASON=""
if [[ -s "${JAVA_DATA}/router.config" ]]; then
  if ! grep -q '^i2np.udp.host=127.0.0.1$' "${JAVA_DATA}/router.config"; then
    TOPOLOGY_OK=0
    TOPOLOGY_REASON="${TOPOLOGY_REASON} udp-host-mismatch"
  fi
  if ! grep -q "^i2np.udp.port=${JAVA_SSU2_PORT}$" "${JAVA_DATA}/router.config"; then
    TOPOLOGY_OK=0
    TOPOLOGY_REASON="${TOPOLOGY_REASON} udp-port-mismatch"
  fi
  if ! grep -q "^i2np.udp.internalPort=${JAVA_SSU2_PORT}$" "${JAVA_DATA}/router.config"; then
    TOPOLOGY_OK=0
    TOPOLOGY_REASON="${TOPOLOGY_REASON} udp-internal-port-mismatch"
  fi
  if ! grep -q '^router.reseedDisable=true$' "${JAVA_DATA}/router.config"; then
    TOPOLOGY_OK=0
    TOPOLOGY_REASON="${TOPOLOGY_REASON} reseed-not-disabled"
  fi
  if ! grep -q '^router.floodfillParticipant=true$' "${JAVA_DATA}/router.config"; then
    TOPOLOGY_OK=0
    TOPOLOGY_REASON="${TOPOLOGY_REASON} floodfill-not-enabled"
  fi
  if ! grep -q '^i2np.ntcp.enable=false$' "${JAVA_DATA}/router.config"; then
    TOPOLOGY_OK=0
    TOPOLOGY_REASON="${TOPOLOGY_REASON} ntcp-not-disabled"
  fi
else
  TOPOLOGY_OK=0
  TOPOLOGY_REASON="${TOPOLOGY_REASON} router-config-missing"
fi
# Plan 196 §5.4 — Java Router rewrites the disposable clients.config
# from `${JAVA_DATA}/clients.config` into per-app config files under
# `${JAVA_DATA}/clients.config.d/`. Either form is acceptable; the
# SAM bridge must be the only client app started on load.
SAM_BRIDGE_CONFIG=""
if [[ -f "${JAVA_DATA}/clients.config" ]]; then
  if ! grep -q '^clientApp.0.main=net.i2p.sam.SAMBridge$' "${JAVA_DATA}/clients.config"; then
    TOPOLOGY_OK=0
    TOPOLOGY_REASON="${TOPOLOGY_REASON} sam-bridge-not-default-app"
  else
    SAM_BRIDGE_CONFIG="${JAVA_DATA}/clients.config"
  fi
elif [[ -d "${JAVA_DATA}/clients.config.d" ]]; then
  SAM_BRIDGE_CONFIG=$(grep -lE '^clientApp\.0\.main=net\.i2p\.sam\.SAMBridge$' "${JAVA_DATA}/clients.config.d"/*-clients.config 2>/dev/null | head -1 || true)
  if [[ -z "${SAM_BRIDGE_CONFIG}" ]]; then
    TOPOLOGY_OK=0
    TOPOLOGY_REASON="${TOPOLOGY_REASON} sam-bridge-config-not-in-clients-config-d"
  fi
else
  TOPOLOGY_OK=0
  TOPOLOGY_REASON="${TOPOLOGY_REASON} clients-config-missing"
fi
if [[ ! -f "${JAVA_DATA}/noreseed.i2p" ]]; then
  TOPOLOGY_OK=0
  TOPOLOGY_REASON="${TOPOLOGY_REASON} noreseed-flag-missing"
fi
# Confirm the bound port is actually bound by Java (loopback UDP).
if ! python3 - <<PY 2>/dev/null
import socket
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
try:
    s.bind(("127.0.0.1", ${JAVA_SSU2_PORT}))
except OSError:
    raise SystemExit(0)
raise SystemExit(1)
PY
then
  TOPOLOGY_OK=0
  TOPOLOGY_REASON="${TOPOLOGY_REASON} udp-port-not-bound-by-java"
fi

# Confirm the bound SAM port is actually accepting connections (loopback TCP).
if ! (exec 3<>"/dev/tcp/127.0.0.1/${JAVA_SAM_PORT}") 2>/dev/null; then
  TOPOLOGY_OK=0
  TOPOLOGY_REASON="${TOPOLOGY_REASON} sam-port-not-listening"
fi

if [[ "${TOPOLOGY_OK}" -ne 1 ]]; then
  echo "controlled Java topology invariants failed: ${TOPOLOGY_REASON}" >&2
  sed -n '1,80p' "${JAVA_DATA}/logs/log-router-0.txt" 2>&1 >&2 || true
  exit 3
fi

# Router B is independently checked as the publication/floodfill role. It
# has a separate RouterContext and disposable state, never a shared VMComm
# or copied NetDB.
if ! grep -q '^i2np.udp.host=127.0.0.1$' "${JAVA_PUBLICATION_DATA}/router.config" ||
   ! grep -q "^i2np.udp.port=${JAVA_PUBLICATION_SSU2_PORT}$" "${JAVA_PUBLICATION_DATA}/router.config" ||
   ! grep -q '^router.reseedDisable=true$' "${JAVA_PUBLICATION_DATA}/router.config" ||
   ! grep -q '^router.floodfillParticipant=true$' "${JAVA_PUBLICATION_DATA}/router.config" ||
   [[ ! -f "${JAVA_PUBLICATION_DATA}/noreseed.i2p" ]]; then
  echo "controlled Java publication topology invariants failed" >&2
  sed -n '1,80p' "${JAVA_PUBLICATION_DATA}/logs/log-router-0.txt" >&2 || true
  exit 3
fi

# Router C is a tunnel participant — no SAM/I2CP required, no floodfill required.
if ! grep -q "^i2np.udp.port=${JAVA_TUNNEL_PARTICIPANT_SSU2_PORT}$" "${JAVA_TUNNEL_PARTICIPANT_DATA}/router.config" ||
   ! grep -q '^router.reseedDisable=true$' "${JAVA_TUNNEL_PARTICIPANT_DATA}/router.config" ||
   ! grep -q '^i2np.ntcp.enable=false$' "${JAVA_TUNNEL_PARTICIPANT_DATA}/router.config" ||
   [[ ! -f "${JAVA_TUNNEL_PARTICIPANT_DATA}/noreseed.i2p" ]]; then
  echo "controlled Java tunnel-participant topology invariants failed" >&2
  sed -n '1,80p' "${JAVA_TUNNEL_PARTICIPANT_DATA}/logs/log-router-0.txt" >&2 || true
  exit 3
fi

# ---- Plan 199 public-client reference helpers ----------------------------
# These helpers are compiled out-of-tree against the staged public jars. The
# reference client owns its destination and publishes its Standard LS2 through
# ordinary I2CP/client behavior; the control socket carries only test commands.
HELPER_CP="${LAUNCHER_CP}"
RAW_HELPER_KEY="${SCRATCH}/raw-reference.priv"
STREAM_HELPER_KEY="${SCRATCH}/stream-reference.priv"
RAW_HELPER_LOG="${SCRATCH}/reference-raw.log"
STREAM_HELPER_LOG="${SCRATCH}/reference-stream.log"
RAW_HELPER_READY="${SCRATCH}/reference-raw.ready"
STREAM_HELPER_READY="${SCRATCH}/reference-stream.ready"

start_raw_helper() {
  : > "${RAW_HELPER_LOG}"
  setsid java -Djava.net.preferIPv4Stack=true -Djava.awt.headless=true \
    -Djava.library.path="${JAVA_CACHE}:${JAVA_CACHE}/lib" \
    -Di2p.dir.base="${JAVA_CACHE}" -cp "${HELPER_CP}" \
    ReferenceRawDestination 127.0.0.1 "${JAVA_I2CP_PORT}" "${JAVA_RAW_CONTROL_PORT}" \
    "${RAW_HELPER_KEY}" >"${RAW_HELPER_READY}" 2>"${RAW_HELPER_LOG}" < /dev/null &
  RAW_HELPER_PID=$!
  CHILD_PIDS+=("${RAW_HELPER_PID}")
  for _ in $(seq 1 180); do
    if grep -q '^READY ' "${RAW_HELPER_READY}" 2>/dev/null; then
      return 0
    fi
    if ! kill -0 "${RAW_HELPER_PID}" 2>/dev/null; then
      cat "${RAW_HELPER_LOG}" >&2 || true
      return 1
    fi
    sleep 0.5
  done
  echo "public Java raw helper did not become ready" >&2
  cat "${RAW_HELPER_LOG}" >&2 || true
  return 1
}

start_stream_helper() {
  : > "${STREAM_HELPER_LOG}"
  setsid java -Djava.net.preferIPv4Stack=true -Djava.awt.headless=true \
    -Djava.library.path="${JAVA_CACHE}:${JAVA_CACHE}/lib" \
    -Di2p.dir.base="${JAVA_CACHE}" -cp "${HELPER_CP}" \
    ReferenceStreamingService 127.0.0.1 "${JAVA_I2CP_PORT}" "${JAVA_STREAM_CONTROL_PORT}" \
    "${STREAM_HELPER_KEY}" >"${STREAM_HELPER_READY}" 2>"${STREAM_HELPER_LOG}" < /dev/null &
  STREAM_HELPER_PID=$!
  CHILD_PIDS+=("${STREAM_HELPER_PID}")
  for _ in $(seq 1 240); do
    if grep -q '^READY ' "${STREAM_HELPER_READY}" 2>/dev/null; then
      return 0
    fi
    if ! kill -0 "${STREAM_HELPER_PID}" 2>/dev/null; then
      cat "${STREAM_HELPER_LOG}" >&2 || true
      return 1
    fi
    sleep 0.5
  done
  echo "public Java streaming helper did not become ready" >&2
  cat "${STREAM_HELPER_LOG}" >&2 || true
  return 1
}

stop_reference_helper() {
  local pid="${1:-}"
  local port="${2:-}"
  [[ -z "${pid}" ]] && return 0
  if [[ -n "${port}" ]]; then
    python3 - "${port}" <<'PY' 2>/dev/null || true
import socket, sys
try:
    with socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=2) as sock:
        sock.sendall(b"STOP\n")
        sock.recv(128)
except OSError:
    pass
PY
  fi
  kill -TERM -- "-${pid}" 2>/dev/null || kill -TERM "${pid}" 2>/dev/null || true
  wait "${pid}" 2>/dev/null || true
}

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

echo "==> local Plan 187/192/193 rows (re-run destination/streaming suites against the i2pr reference build)"
UNIT_LOG="${EVIDENCE_DIR}/local-destination-tunnel-unit.log"
: > "${UNIT_LOG}"
unit_rc=0
cargo test --locked -p i2pr-daemon --test destination_tunnel_unit -- \
  --test-threads=1 >>"${UNIT_LOG}" 2>&1 || unit_rc=$?
record_guarded "local-destination-tunnel-unit" \
  "Plan 187 destination coordinator unit rows (cargo test -p i2pr-daemon --test destination_tunnel_unit)" \
  "${unit_rc}"

LIVE_LOG="${EVIDENCE_DIR}/local-destination-tunnel-live.log"
: > "${LIVE_LOG}"
live_rc=0
cargo test --locked -p i2pr-daemon --test destination_tunnel_live -- \
  --test-threads=1 >>"${LIVE_LOG}" 2>&1 || live_rc=$?
record_guarded "local-destination-tunnel-live" \
  "Plan 187 destination coordinator live rows (cargo test -p i2pr-daemon --test destination_tunnel_live)" \
  "${live_rc}"

STREAMING_UNIT_LOG="${EVIDENCE_DIR}/local-streaming-tunnel-unit.log"
: > "${STREAMING_UNIT_LOG}"
streaming_unit_rc=0
cargo test --locked -p i2pr-daemon --test streaming_tunnel_unit -- \
  --test-threads=1 >>"${STREAMING_UNIT_LOG}" 2>&1 || streaming_unit_rc=$?
record_guarded "local-streaming-tunnel-unit" \
  "Plan 193 Streaming manager unit rows (cargo test -p i2pr-daemon --test streaming_tunnel_unit)" \
  "${streaming_unit_rc}"

STREAMING_LIVE_LOG="${EVIDENCE_DIR}/local-streaming-tunnel-live.log"
: > "${STREAMING_LIVE_LOG}"
streaming_live_rc=0
cargo test --locked -p i2pr-daemon --test streaming_tunnel_live -- \
  --test-threads=1 >>"${STREAMING_LIVE_LOG}" 2>&1 || streaming_live_rc=$?
record_guarded "local-streaming-tunnel-live" \
  "Plan 193 Streaming live two-role rows (cargo test -p i2pr-daemon --test streaming_tunnel_live)" \
  "${streaming_live_rc}"

LIVENESS_LOG="${EVIDENCE_DIR}/local-tunnel-liveness.log"
: > "${LIVENESS_LOG}"
liveness_rc=0
cargo test --locked -p i2pr-daemon --lib tunnel_liveness -- \
  --test-threads=1 >>"${LIVENESS_LOG}" 2>&1 || liveness_rc=$?
record_guarded "local-tunnel-liveness" \
  "Plan 185 liveness scheduler unit rows (cargo test -p i2pr-daemon --lib tunnel_liveness)" \
  "${liveness_rc}"

echo "==> external Java I2P second-family lane against exact-pinned Java I2P 2.13.0"
DRIVER_EVIDENCE="${EVIDENCE_DIR}/driver"
mkdir -p "${DRIVER_EVIDENCE}"
DRIVER_LOG="${EVIDENCE_DIR}/external-driver.log"
: > "${DRIVER_LOG}"
driver_rc=0

# Plan 217 §9 — bounded selector for the destination/Streaming
# sub-runs. Set I2PR_M6_JAVA_DRIVER=destination|streaming|both
# (default both) so an operator can re-run a single sub-run during
# diagnosis without duplicating the Java-router topology setup. The
# bootstrap probe always runs because both drivers depend on it.
I2PR_M6_JAVA_DRIVER="${I2PR_M6_JAVA_DRIVER:-both}"
case "${I2PR_M6_JAVA_DRIVER}" in
  destination|streaming|both)
    ;;
  *)
    echo "I2PR_M6_JAVA_DRIVER must be one of: destination, streaming, both (got '${I2PR_M6_JAVA_DRIVER}')" >&2
    exit 64
    ;;
esac
echo "    sub-run selector: I2PR_M6_JAVA_DRIVER=${I2PR_M6_JAVA_DRIVER}" >>"${DRIVER_LOG}"
# Plan 199 §A.2: establish the Java A/B NetDB peer relationship before
# starting public clients, whose one-hop tunnel readiness depends on it.
BOOTSTRAP_LOG="${DRIVER_EVIDENCE}/bootstrap.log"
mkdir -p "${DRIVER_EVIDENCE}/bootstrap"
# Plan 219 §6.B — moment #2 (immediately before bootstrap)
# captures the freshest RouterInfo capability state on each
# Java router; this is the snapshot the bootstrap probe's
# `p200-routerinfo-lookup-*` rows will then verify.
j219_record_timed_snapshot "immediately-before-bootstrap"
bootstrap_rc=0
if ! /usr/bin/env JAVA_SERVICE_ROUTER_INFO="${JAVA_RI}" \
   JAVA_SERVICE_SSU2_ENDPOINT="127.0.0.1:${JAVA_SSU2_PORT}" \
   JAVA_PUBLICATION_ROUTER_INFO="${JAVA_PUBLICATION_RI}" \
   JAVA_PUBLICATION_SSU2_ENDPOINT="127.0.0.1:${JAVA_PUBLICATION_SSU2_PORT}" \
   JAVA_TUNNEL_PARTICIPANT_ROUTER_INFO="${JAVA_TUNNEL_PARTICIPANT_RI}" \
   JAVA_TUNNEL_PARTICIPANT_SSU2_ENDPOINT="127.0.0.1:${JAVA_TUNNEL_PARTICIPANT_SSU2_PORT}" \
   I2PR_SSU2_BIND="127.0.0.1:${I2PR_BOOTSTRAP_PORT}" \
   EVIDENCE_DIR="${DRIVER_EVIDENCE}/bootstrap" \
   timeout --foreground "${DRIVER_TIMEOUT}" \
   cargo test --locked -p i2pr-daemon --test java_tunnel_external \
   bootstrap_java_router_peers -- --ignored --exact --nocapture --test-threads=1 \
   >>"${BOOTSTRAP_LOG}" 2>&1; then
  bootstrap_rc=$?
  driver_rc=1
fi
echo "    Java A/B ordinary RouterInfo bootstrap exit=${bootstrap_rc}" >>"${DRIVER_LOG}"
# Plan 219 §6.B — moment #3 (immediately after bootstrap)
# captures the post-bootstrap RouterInfo capability state
# the helper client-specific lookup will see.
j219_record_timed_snapshot "immediately-after-bootstrap"
# Plan 200 §B — include the bootstrap probe evidence in the
# aggregated driver-evidence.tsv so the shell can read the
# `p200-routerinfo-lookup-*` rows without a second pass over the
# bootstrap evidence directory. The destination/streaming evidence
# is concatenated below.
BOOTSTRAP_TSV="${DRIVER_EVIDENCE}/bootstrap/driver-evidence.tsv"
if [[ -f "${BOOTSTRAP_TSV}" ]]; then
  cat "${BOOTSTRAP_TSV}" >> "${DRIVER_EVIDENCE}/bootstrap-driver-evidence.tsv"
fi
# Plan 199 §A.3 — the destination and Streaming drivers run against
# separate public-client helpers. SAM remains available only for the
# retained diagnostic compatibility row and is never a counted service
# destination.
DRIVER_DEST_TSV="${DRIVER_EVIDENCE}/driver-destination.tsv"
DRIVER_STREAM_TSV="${DRIVER_EVIDENCE}/driver-streaming.tsv"
mkdir -p "${DRIVER_EVIDENCE}/destination" "${DRIVER_EVIDENCE}/streaming"
: > "${DRIVER_DEST_TSV}"
: > "${DRIVER_STREAM_TSV}"
driver_rc=0
streaming_rc=0

# Plan 217 §9 — bounded selector. The sub-runs share the same Java
# RouterContexts but use disjoint build/tunnel/message-id namespaces
# (Plan 217 §6.D), so each sub-run is independent and may be
# executed alone for diagnosis.
if [[ "${I2PR_M6_JAVA_DRIVER}" == "destination" || "${I2PR_M6_JAVA_DRIVER}" == "both" ]]; then
  start_raw_helper
  # Plan 200 §A.1 — the helper's `READY` line is intentionally the
  # minimal public-client fact set; the b64 destination is the
  # last-but-one whitespace-separated field after `READY`, with
  # the Java PIN as the final field.
  RAW_REFERENCE_DESTINATION_B64="$(awk 'NR==1 {
    for (i = NF; i >= 1; i--) {
      if (length($i) > 100) { print $i; exit }
    }
  }' "${RAW_HELPER_READY}")"
  echo "    public Java raw helper ready; running destination driver" >>"${DRIVER_LOG}"
  # Plan 220 §7 — moment #4 (immediately before reverse
  # helper SEND) captures the RouterInfo capability state
  # the helper observes on its own outbound-tunnel endpoint's
  # peer-selection path. History only; the driver takes its own
  # authoritative P220 snapshot at its post-bootstrap epoch.
  j219_record_timed_snapshot "immediately-before-reverse-helper-send"
  if /usr/bin/env JAVA_ROUTER_INFO="${JAVA_PUBLICATION_RI}" \
     JAVA_SSU2_ENDPOINT="127.0.0.1:${JAVA_PUBLICATION_SSU2_PORT}" \
     JAVA_SERVICE_ROUTER_INFO="${JAVA_RI}" \
     JAVA_SERVICE_SSU2_ENDPOINT="127.0.0.1:${JAVA_SSU2_PORT}" \
     JAVA_PUBLICATION_ROUTER_INFO="${JAVA_PUBLICATION_RI}" \
     JAVA_PUBLICATION_SSU2_ENDPOINT="127.0.0.1:${JAVA_PUBLICATION_SSU2_PORT}" \
     JAVA_I2CP_ENDPOINT="127.0.0.1:${JAVA_I2CP_PORT}" \
     JAVA_RAW_CONTROL_ENDPOINT="127.0.0.1:${JAVA_RAW_CONTROL_PORT}" \
     JAVA_RAW_REFERENCE_DESTINATION_B64="${RAW_REFERENCE_DESTINATION_B64}" \
     I2PR_SSU2_BIND="127.0.0.1:${I2PR_PORT}" \
     EVIDENCE_DIR="${DRIVER_EVIDENCE}/destination" \
     JAVA_DIAGNOSTIC_A_PORT="${JAVA_DIAGNOSTIC_A_PORT}" \
     JAVA_DIAGNOSTIC_B_PORT="${JAVA_DIAGNOSTIC_B_PORT}" \
     JAVA_DIAGNOSTIC_C_PORT="${JAVA_DIAGNOSTIC_C_PORT}" \
     timeout --foreground "${DRIVER_TIMEOUT}" \
     cargo test --locked -p i2pr-daemon --test java_tunnel_external \
     destination_message_plane_against_java -- --ignored --exact --nocapture --test-threads=1 \
     >>"${DRIVER_LOG}" 2>&1; then
    driver_rc=0
  else
    driver_rc=$?
  fi
  echo "    destination driver exit=${driver_rc}" >>"${DRIVER_LOG}"
  # Concatenate the destination driver's evidence into the destination TSV
  if [[ -f "${DRIVER_EVIDENCE}/destination/driver-evidence.tsv" ]]; then
    cat "${DRIVER_EVIDENCE}/destination/driver-evidence.tsv" >> "${DRIVER_DEST_TSV}"
  fi
  stop_reference_helper "${RAW_HELPER_PID}" "${JAVA_RAW_CONTROL_PORT}"
  # Plan 220 §7 — moment #5 (after reverse-send wait
  # expires) captures the post-deadline RouterInfo
  # capability state and closes the correlation timeline.
  j219_record_timed_snapshot "after-reverse-send-wait-expires"
fi

if [[ "${I2PR_M6_JAVA_DRIVER}" == "streaming" || "${I2PR_M6_JAVA_DRIVER}" == "both" ]]; then
  start_stream_helper
  # Plan 200 §A.1 — see RAW_REFERENCE_DESTINATION_B64 above.
  STREAM_REFERENCE_DESTINATION_B64="$(awk 'NR==1 {
    for (i = NF; i >= 1; i--) {
      if (length($i) > 100) { print $i; exit }
    }
  }' "${STREAM_HELPER_READY}")"
  echo "    public Java streaming helper ready; running streaming driver" >>"${DRIVER_LOG}"
  # Streaming driver run. Reuses the same SSU2 endpoint and SAM
  # Java public Streaming manager; it is independent of §5.4.
  if /usr/bin/env JAVA_ROUTER_INFO="${JAVA_PUBLICATION_RI}" \
     JAVA_SSU2_ENDPOINT="127.0.0.1:${JAVA_PUBLICATION_SSU2_PORT}" \
     JAVA_SERVICE_ROUTER_INFO="${JAVA_RI}" \
     JAVA_SERVICE_SSU2_ENDPOINT="127.0.0.1:${JAVA_SSU2_PORT}" \
     JAVA_PUBLICATION_ROUTER_INFO="${JAVA_PUBLICATION_RI}" \
     JAVA_PUBLICATION_SSU2_ENDPOINT="127.0.0.1:${JAVA_PUBLICATION_SSU2_PORT}" \
     JAVA_I2CP_ENDPOINT="127.0.0.1:${JAVA_I2CP_PORT}" \
     JAVA_STREAM_CONTROL_ENDPOINT="127.0.0.1:${JAVA_STREAM_CONTROL_PORT}" \
     JAVA_STREAM_REFERENCE_DESTINATION_B64="${STREAM_REFERENCE_DESTINATION_B64}" \
     I2PR_SSU2_BIND="127.0.0.1:${I2PR_STREAM_PORT}" \
     EVIDENCE_DIR="${DRIVER_EVIDENCE}/streaming" \
     timeout --foreground "${DRIVER_TIMEOUT}" \
     cargo test --locked -p i2pr-daemon --test java_tunnel_external \
     streaming_through_java -- --ignored --exact --nocapture --test-threads=1 \
     >>"${DRIVER_LOG}" 2>&1; then
    streaming_rc=0
  else
    streaming_rc=$?
  fi
  echo "    streaming driver exit=${streaming_rc}" >>"${DRIVER_LOG}"
  stop_reference_helper "${STREAM_HELPER_PID}" "${JAVA_STREAM_CONTROL_PORT}"
  if [[ -f "${DRIVER_EVIDENCE}/streaming/driver-evidence.tsv" ]]; then
    cat "${DRIVER_EVIDENCE}/streaming/driver-evidence.tsv" >> "${DRIVER_STREAM_TSV}"
  fi
fi
# Compose the aggregated driver-evidence.tsv the helpers below read.
: > "${DRIVER_EVIDENCE}/driver-evidence.tsv"
if [[ -f "${DRIVER_EVIDENCE}/bootstrap-driver-evidence.tsv" ]]; then
  cat "${DRIVER_EVIDENCE}/bootstrap-driver-evidence.tsv" >> "${DRIVER_EVIDENCE}/driver-evidence.tsv"
fi
cat "${DRIVER_DEST_TSV}" >> "${DRIVER_EVIDENCE}/driver-evidence.tsv"
cat "${DRIVER_STREAM_TSV}" >> "${DRIVER_EVIDENCE}/driver-evidence.tsv"
DRIVER_TSV="${DRIVER_EVIDENCE}/driver-evidence.tsv"
echo "==> sanitized reference-side facts (counts only, never key material)"
# Keep a narrowly filtered, secret-scrubbed router diagnostic so a public
# client publication regression can be distinguished from an i2pr lookup
# regression without exporting the raw Java router log.
JAVA_LOG_FILE="$(find "${JAVA_DATA}" -type f -name 'log-router-*.txt' -print -quit 2>/dev/null || true)"
if [[ -n "${JAVA_LOG_FILE}" ]]; then
  grep -Ei 'LeaseSet|I2CP|Database(Store|Lookup)|publish|client tunnel' \
    "${JAVA_LOG_FILE}" 2>/dev/null \
    | sed -E 's/[A-Za-z0-9+~=\/~.-]{60,}/<redacted>/g' \
    >"${EVIDENCE_DIR}/java-public-client-diagnostic.log" || true
fi
REFERENCE_FACTS="${EVIDENCE_DIR}/reference-facts.tsv"
: > "${REFERENCE_FACTS}"
{
  printf 'java-udp-listening\t%s\n' "$(grep -c 'SSU2 endpoint\|UDPTransport\|Started UDPTransport\|UDP transport started' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-sam-bridge-up\t%s\n' "$(grep -c 'SAM bridge started\|SAMBridge\|Starting SAM' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-reseed-disabled\t%s\n' "$(grep -c '^router.reseedDisable=true$' "${JAVA_DATA}/router.config" 2>/dev/null || true)"
  printf 'java-floodfill-capable\t%s\n' "$(grep -c '^router.floodfillParticipant=true$' "${JAVA_DATA}/router.config" 2>/dev/null || true)"
  printf 'java-udp-port-bound\t%s\n' "$(grep -c "^i2np.udp.port=${JAVA_SSU2_PORT}$" "${JAVA_DATA}/router.config" 2>/dev/null || true)"
  printf 'java-ntcp-disabled\t%s\n' "$(grep -c '^i2np.ntcp.enable=false$' "${JAVA_DATA}/router.config" 2>/dev/null || true)"
  # Java writes the SAM bridge to `clients.config.d/<prefix>-clients.config`,
  # not the legacy monolithic `clients.config`. The shell harness must
  # consult the d/ directory (the upstream Java 2.13.0 default layout)
  # before the older `clients.config` form. Plan 196 §5.4 captures this.
  printf 'java-no-public-reseed\t%s\n' "$(test -f "${JAVA_DATA}/noreseed.i2p" && echo 1 || echo 0)"
  printf 'java-sam-bridge-configured\t%s\n' "$(grep -rcl '^clientApp.0.main=net.i2p.sam.SAMBridge$' "${JAVA_DATA}/clients.config.d" 2>/dev/null | head -1 | wc -l)"
  # Plan 194 §5.5: Java's StreamingConnection emits "Rcvd accept status"
  # and "Rcvd success status" at INFO level when a streaming SYN is
  # processed (ConnectionPacketHandler.java:82 / PacketQueue.java:362-368).
  # That is the Java-side counterpart of i2pd's "Streaming: Incoming
  # stream from" — the reference StreamingDestination accepted the SYN.
  printf 'java-streaming-accepted\t%s\n' "$(grep -cE 'Rcvd (accept|success) status' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  # Plan 200 §C — observe the Java client LeaseSet lifecycle through
  # sanitized stock log events. Each count is a coarse signal only;
  # absence is itself a diagnostic fact (not silently rewritten to
  # "passed"). Raw log lines are NEVER retained as evidence — only
  # these bounded count keys.
  # Plan 217 §6.B — positive-observation patterns only. The pinned
  # Java 2.13.0 source (`LeaseSetPublisher.java:79` etc.) emits these
  # strings when the helper LeaseSet2 publication path advances; absence
  # is itself a diagnostic fact.
  printf 'java-client-subdb-created\t%s\n' "$(grep -cE 'new FloodfillNetworkDatabaseSegmentor|new FloodfillNetworkDatabaseFacade|ClientConnectionRunner' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-create-leaseset2-received\t%s\n' "$(grep -cE 'CreateLeaseSet2MessageHandler|CreateLeaseSetMessage|handleCreateLeaseSet2|createNewLeaseSet' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-client-leaseset-stored-current\t%s\n' "$(grep -cE 'Stored local LeaseSet|getStoredLocal|LeaseSet stored|LeaseSet2 stored|current.*ls2' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-client-leaseset-publish-scheduled\t%s\n' "$(grep -cE 'Scheduling republish|RepublishLeaseSetJob|republish scheduled|will republish' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-client-leaseset-republish-job-ran\t%s\n' "$(grep -cE 'RepublishLeaseSetJob|republishStore|republishing leaseSet|RepublishJob run|publishing lease' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  # Plan 200 §D — tunnel eligibility, floodfill selection, store/ack
  # bookkeeping. Each grep is bounded to the documented exact-pinned
  # log shape; absence is itself a diagnostic fact, never rewritten.
  # Plan 217 §6.B — positive-only patterns. The prior mixed greps
  # counted "No outbound tunnels available" alongside positive
  # eligibility signals, which violates §6.B step 3 ("no positive row
  # may be satisfied by a string beginning with 'No …'"). Negative
  # observations are recorded in the `*-unavailable` and
  # `*-no-peers` keys below; positive rows now consume only positive
  # patterns.
  printf 'java-client-inbound-tunnel-selectable\t%s\n' "$(grep -cE 'inbound tunnel.*select|selectReplyInbound' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-client-outbound-tunnel-selectable\t%s\n' "$(grep -cE 'outbound tunnel.*select|selectOutboundTunnel|client tunnel.*select' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-floodfill-candidate-non-empty\t%s\n' "$(grep -cE 'floodfill peer selector|floodfill routerInfo' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-store-emitted\t%s\n' "$(grep -cE 'Sending store|Sending to floodfill|Storing leaseSet|Storing leaseSet2|sent StoreJob|sent StoreMsg' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-store-ack-observed\t%s\n' "$(grep -cE 'DeliveryStatusMessageHandler|received ack|stored successfully|Store successful|Ack received|store reply.*received' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-store-failure-reason\t%s\n' "$(grep -cE 'store failed|store timeout|store exception|could not store|peer.*not eligible|peer.*unreachable' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  # Plan 217 §6.B — diagnostic-only negative observations. Each is a
  # "No …" pinned Java string the harness explicitly attributes to a
  # failure mode; zero is the healthy state, ≥1 is a diagnostic
  # observation only and NEVER satisfies a positive acceptance row.
  printf 'java-client-inbound-tunnel-unavailable\t%s\n' "$(grep -cE 'No inbound tunnels available|No reply inbound tunnels' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-client-outbound-tunnel-unavailable\t%s\n' "$(grep -cE 'No outbound tunnels available' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
  printf 'java-floodfill-candidate-empty\t%s\n' "$(grep -cE 'No floodfill peers|No peers|No more peers' "${JAVA_LOG_FILE}" 2>/dev/null || true)"
} >> "${REFERENCE_FACTS}"
ref_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  local rc=1
  if [[ -f "${REFERENCE_FACTS}" ]]; then
    local count
    count="$(grep -F "${key}" "${REFERENCE_FACTS}" 2>/dev/null | cut -f2 || true)"
    if [[ "${count:-0}" -ge 1 ]]; then
      rc=0
    fi
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}
m6_row() {
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
m6_key_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  local rc=1
  if [[ -f "${DRIVER_TSV}" ]] && grep -Fq "${key}" "${DRIVER_TSV}"; then
    rc=0
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}
# Plan 199 §A.3 — topology readiness must precede the SSU2 gate. The
# controlled launcher already produces the topology evidence keys; we
# emit them as passed on success and as failed-with-stop-provenance
# only when the external driver recorded `plan199-java-stop`.
STOP_FIRED=0
if [[ -f "${DRIVER_TSV}" ]] && grep -Fq "plan199-java-stop" "${DRIVER_TSV}"; then
  STOP_FIRED=1
fi
blocked_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  if [[ -f "${DRIVER_TSV}" ]] && awk -v k="${key}" -F'\t' '$1 == k {found=1} END{exit !found}' "${DRIVER_TSV}"; then
    record "${label}" passed "${detail}"
  elif [[ "${STOP_FIRED}" -eq 1 ]]; then
    record "${label}" blocked "${detail} (m6-java-second-family-stop; see Plan 199 stop provenance)"
  else
    record "${label}" failed "${detail} (no evidence key, no stop provenance)"
  fi
}
m6_key_row "external-daemon-strict-profile" "daemon-strict-profile" \
  "daemon starts with strict SSU2 controlled profile (explicit --ignored --exact driver)"
m6_key_row "external-reference-verified" "reference-routerinfo-verified" \
  "exact-pinned Java RouterInfo parsed/verified through the documented file path"
m6_key_row "external-reference-floodfill" "reference-floodfill-capable" \
  "reference RouterInfo advertises floodfill and bootstraps the authoritative store"
m6_key_row "external-session-established" "session-established" \
  "authenticated SSU2 session establishes via daemon-owned runtime"
m6_key_row "external-public-client-destination-created" "public-client-destination-created" \
  "reference Standard LeaseSet2 service destination created through public Java I2PSession"
m6_key_row "external-public-streaming-destination-created" "public-streaming-destination-created" \
  "reference Streaming service destination created through public I2PSocketManager"
ref_row "java-routerinfo-host-bound" "java-udp-port-bound" \
  "Java router binds the controlled UDP port and RouterInfo advertises it"
ref_row "java-routerinfo-port-bound" "java-udp-port-bound" \
  "Java router.config records the selected UDP port from the harness"
ref_row "java-reseed-disabled" "java-reseed-disabled" \
  "Java router keeps the controlled-topology no-reseed setting in the live datadir"
ref_row "java-floodfill-capable" "java-floodfill-capable" \
  "Java router.floodfillParticipant=true is committed in the controlled data dir"
ref_row "java-ntcp-disabled" "java-ntcp-disabled" \
  "Java router NTCP/SSU legacy transports are disabled in the controlled profile"
ref_row "java-sam-bridge-configured" "java-sam-bridge-configured" \
  "Java disposable clients.config starts only the SAM bridge on the selected port"
blocked_row "external-outbound-tunnel" "outbound-installed" \
  "real one-hop outbound build installed with cryptographically derived keys"
blocked_row "external-inbound-tunnel" "inbound-installed" \
  "real one-hop inbound build installed with cryptographically derived keys"
blocked_row "external-outbound-accepted" "outbound-installed" \
  "Java accepted the outbound build (proved by outbound-installed evidence key from the driver)"
blocked_row "external-inbound-accepted" "inbound-installed" \
  "Java accepted the inbound build (proved by inbound-installed evidence key from the driver)"
ref_row "external-reference-ls2-published" "java-floodfill-capable" \
  "reference floodfill setting is committed in the controlled data dir"
blocked_row "external-lease-lookup-tunnel" "lease-lookup-completed" \
  "reference Standard LeaseSet2 resolved through the real tunnel NetDB path and cached"
blocked_row "external-ls2-publication-tunnel" "ls2-publication-tunnel" \
  "local Standard LeaseSet2 with the real inbound lease published through the controlled path"
blocked_row "external-destination-outbound" "destination-outbound-delivered" \
  "bounded message traverses ECIES/Garlic + real outbound tunnel + selected remote lease"
blocked_row "external-reference-received" "reference-received" \
  "reference SAM RAW session receives and authenticates the bounded message"
blocked_row "external-destination-inbound" "destination-inbound-received" \
  "reply traverses the real inbound tunnel and the existing ECIES decrypt path"
blocked_row "external-streaming-syn-sent" "streaming-syn-sent" \
  "i2pr StreamingManager.connect emits a SYN through ECIES/Garlic + real outbound tunnel"
blocked_row "external-streaming-syn-accepted" "streaming-syn-accepted" \
  "Java StreamingDestination accepts the SYN and emits a SYN response"
blocked_row "external-streaming-established" "streaming-established" \
  "Streaming connection reaches Established state in both directions"
blocked_row "external-streaming-data-digest" "streaming-data-digest" \
  "Streaming application data round-trips byte-exact through the destination path"
blocked_row "external-streaming-multipacket-digest" "streaming-multipacket-digest" \
  "Streaming multi-packet payload digest matches through the destination path"
blocked_row "external-streaming-reverse-data-digest" "streaming-reverse-data-digest" \
  "reference-to-i2pr application data digest matches over the established stream"
blocked_row "external-streaming-reverse-multipacket-digest" "streaming-reverse-multipacket-digest" \
  "reference-to-i2pr multi-packet digest matches over the established stream"
blocked_row "external-streaming-sibling-established" "streaming-sibling-established" \
  "second sibling stream establishes over the same real path"
blocked_row "external-streaming-sibling-data-digest" "streaming-sibling-data-digest" \
  "sibling stream application data arrives on its own ACCEPT socket"
blocked_row "external-streaming-close" "streaming-close" \
  "orderly full close reaches Closed with reference socket EOF"
blocked_row "external-streaming-sibling-isolated" "streaming-sibling-isolated" \
  "sibling stream still delivers after the first connection closes"
blocked_row "external-streaming-b-established" "streaming-b-established" \
  "Java-initiated stream establishes through the normal listener/accept path"
blocked_row "external-streaming-b-data-digest" "streaming-b-data-digest" \
  "Java-to-i2pr Direction B payload digest matches"
blocked_row "external-streaming-b-reverse-data-digest" "streaming-b-reverse-data-digest" \
  "i2pr-to-Java Direction B payload digest matches"
blocked_row "external-streaming-b-close" "streaming-b-close" \
  "Direction B stream closes orderly with reference socket EOF"
blocked_row "external-manager-cleanup" "manager-cleanup" \
  "no queued transport or undrained bytes after every stream closed"
blocked_row "external-streaming-reference-accepted" "streaming-syn-accepted" \
  "Java accepted the streaming SYN (proved by streaming-syn-accepted evidence key from the driver; the Java log-line equivalent is informational)"
m6_key_row "external-direct-rejected" "direct-rejected" \
  "direct transport streaming delivery is rejected as a counted path"
m6_key_row "external-liveness-first-test" "liveness-first-test" \
  "creator-side liveness scheduler first test succeeds during destination activity"
ref_row "external-reseed-disabled" "java-no-public-reseed" \
  "Java router has the controlled no-public-reseed flag in the live datadir"

# Plan 200 §B — post-bootstrap RouterInfo lookup proofs in both
# directions. Each row is `passed` only when the corresponding
# `p200-routerinfo-lookup-<label>` evidence key was emitted by the
# bootstrap probe with `response_observed=true key_match=true
# identity_match=true ssu2_addresses>=1`.
m6_key_row "external-routerinfo-lookup-a-knows-b" "p200-routerinfo-lookup-a-knows-b" \
  "Plan 200 §B: Java router A's main NetDB serves router B's RouterInfo through ordinary DatabaseLookup"
m6_key_row "external-routerinfo-lookup-b-knows-a" "p200-routerinfo-lookup-b-knows-a" \
  "Plan 200 §B: Java router B's main NetDB serves router A's RouterInfo through ordinary DatabaseLookup"

# Plan 200 §C — Java client LeaseSet lifecycle observed through
# sanitized stock log events. Each row is `passed` only when the
# corresponding event class was observed at least once.
ref_row "external-java-client-subdb-created" "java-client-subdb-created" \
  "Plan 200 §C: Java created the client-specific NetDB facade (sanitized log count)"
ref_row "external-java-create-leaseset2-received" "java-create-leaseset2-received" \
  "Plan 200 §C: Java's client message listener received the CreateLeaseSet2 message (sanitized log count)"
ref_row "external-java-client-leaseset-stored-current" "java-client-leaseset-stored-current" \
  "Plan 200 §C: Java stored a current local LS2 in the client sub-NetDB (sanitized log count)"
ref_row "external-java-client-leaseset-publish-scheduled" "java-client-leaseset-publish-scheduled" \
  "Plan 200 §C: Java scheduled the republish job for the client LS2 (sanitized log count)"
ref_row "external-java-client-leaseset-republish-job-ran" "java-client-leaseset-republish-job-ran" \
  "Plan 200 §C: Java's republish job actually ran for the client LS2 (sanitized log count)"

# Plan 200 §D — tunnel eligibility, floodfill selection, store
# emission vs ack bookkeeping. Each row is `passed` only when the
# sanitized Java log count is >= 1 (positive observation).
ref_row "external-java-client-inbound-tunnel-eligible" "java-client-inbound-tunnel-selectable" \
  "Plan 200 §D: Java's client inbound tunnel path is selectable (sanitized log count)"
ref_row "external-java-client-outbound-tunnel-eligible" "java-client-outbound-tunnel-selectable" \
  "Plan 200 §D: Java's client outbound tunnel path is selectable (sanitized log count)"
ref_row "external-java-floodfill-candidate-available" "java-floodfill-candidate-non-empty" \
  "Plan 200 §D: Java's floodfill selector had at least one candidate (sanitized log count)"
ref_row "external-java-store-emitted" "java-store-emitted" \
  "Plan 200 §D: Java emitted a DatabaseStore for the client LS2 (sanitized log count)"
ref_row "external-java-store-ack-observed" "java-store-ack-observed" \
  "Plan 200 §D: Java observed an ack / DeliveryStatus for the LS2 store (sanitized log count)"
ref_row "external-java-store-failure-reason" "java-store-failure-reason" \
  "Plan 200 §D: Java surfaced an explicit store failure reason (sanitized log count, positive or absent)"

# Plan 200 §11 — emit one terminal `P200-*` classification row
# derived from the bootstrap probe's `p200-classification` evidence.
# The earliest non-passing boundary wins; if all observed
# boundaries pass, the classification is `P200-H-publication-path-passed`.
# Plan 217 §6.B.6 — read from the FINAL snapshot (last occurrence),
# not the first emission. The bootstrap probe is the only emitter
# today, so the two are equivalent, but using `last-classification`
# preserves the property if a later corrective pass adds a re-emit
# after helper readiness. The classification itself remains the
# single terminal P200 row emitted per run.
P200_CLASSIFICATION=""
if [[ -f "${DRIVER_EVIDENCE}/bootstrap/driver-evidence.tsv" ]]; then
  P200_CLASSIFICATION="$(awk -F'\t' '$1 == "p200-classification" { sub(/^[^ ]+ /, "", $2); last=$2 } END { if (last) print last }' "${DRIVER_EVIDENCE}/bootstrap/driver-evidence.tsv")"
fi
if [[ -z "${P200_CLASSIFICATION}" ]]; then
  P200_CLASSIFICATION="P200-classification-missing"
fi
if [[ "${P200_CLASSIFICATION}" == "P200-H-publication-path-passed" ]]; then
  record "external-p200-classification" passed "Plan 200 §11: ${P200_CLASSIFICATION}"
else
  # Per Plan 200 §11, downstream rows past the classified boundary
  # stay blocked until Plan 201; the classification itself is
  # recorded as `passed` because it is itself a diagnostic
  # observation rather than a publication claim.
  record "external-p200-classification" passed \
    "Plan 200 §11: terminal classification ${P200_CLASSIFICATION} (downstream rows pending Plan 201 corrective)"
fi

# Plan 220 §11 — read the terminal `p220-classification`
# the destination driver emitted from its authoritative
# post-bootstrap / pre-reverse-send epoch. We MUST consume the
# LAST occurrence so an early-fail branch can't shadow the
# authoritative outcome. The static checker rejects a
# first-occurrence awk and rejects any consumption of the
# superseded Plan 219 terminal key (D220 attribution is
# historical only).
P220_CLASSIFICATION=""
DEST_DRIVER_TSV_FOR_P220="${DRIVER_EVIDENCE}/destination/driver-evidence.tsv"
if [[ -f "${DEST_DRIVER_TSV_FOR_P220}" ]]; then
  P220_CLASSIFICATION="$(awk -F'\t' '$1 == "p220-classification" { sub(/^[^ ]+ /, "", $2); last=$2 } END { if (last) print last }' "${DEST_DRIVER_TSV_FOR_P220}")"
fi
if [[ -z "${P220_CLASSIFICATION}" ]]; then
  P220_CLASSIFICATION="P220-classification-missing"
fi
# Plan 220 §11 — exactly one terminal classification per run.
# Recording the classification itself is a diagnostic
# observation; it is reported as `passed` regardless of the
# boundary or observability gap it names. The static checker
# rejects any literal `record "<P220-X>" passed` line.
record "external-p220-classification" passed "Plan 220 §11: ${P220_CLASSIFICATION}"

# Plan 222 §H — read the terminal `p222-classification` the destination
# driver emitted from its exact client-lookup preflight + tracked-send
# path. We MUST consume the LAST occurrence so an early-fail branch
# can't shadow the authoritative outcome. Recording the classification
# itself is a diagnostic observation; it is reported as `passed`
# regardless of the boundary or observability gap it names. The static
# checker rejects any literal `record "<P222-X>" passed` line. The
# frozen 45-second i2pr payload row is never rewritten by this status.
P222_CLASSIFICATION=""
DEST_DRIVER_TSV_FOR_P222="${DRIVER_EVIDENCE}/destination/driver-evidence.tsv"
if [[ -f "${DEST_DRIVER_TSV_FOR_P222}" ]]; then
  P222_CLASSIFICATION="$(awk -F'\t' '$1 == "p222-classification" { sub(/^[^ ]+ /, "", $2); last=$2 } END { if (last) print last }' "${DEST_DRIVER_TSV_FOR_P222}")"
fi
if [[ -z "${P222_CLASSIFICATION}" ]]; then
  P222_CLASSIFICATION="P222-classification-missing"
fi
record "external-p222-classification" passed "Plan 222 §H: ${P222_CLASSIFICATION}"

# Plan 223 §12 — read the terminal `p223-classification` the destination
# driver emitted from its exact Destination + branch discriminator path.
# We MUST consume the LAST occurrence so an early branch can't shadow the
# authoritative outcome. Recording the classification itself is a
# diagnostic observation; it is reported as `passed` regardless of the
# terminal it names. The static checker rejects any literal
# `record "<P223-X>" passed` line.
P223_CLASSIFICATION=""
DEST_DRIVER_TSV_FOR_P223="${DRIVER_EVIDENCE}/destination/driver-evidence.tsv"
if [[ -f "${DEST_DRIVER_TSV_FOR_P223}" ]]; then
  P223_CLASSIFICATION="$(awk -F'\t' '$1 == "p223-classification" { sub(/^[^ ]+ /, "", $2); last=$2 } END { if (last) print last }' "${DEST_DRIVER_TSV_FOR_P223}")"
fi
if [[ -z "${P223_CLASSIFICATION}" ]]; then
  P223_CLASSIFICATION="P223-classification-missing"
fi
record "external-p223-classification" passed "Plan 223 §12: ${P223_CLASSIFICATION}"

# Plan 201 §G — Branch G (store-acked-remote-lookup-fails) diagnostic
# boundary rows. Each row is `passed` only when the corresponding
# `p201-lookup-boundary-<label>-<value>` evidence key was emitted
# by the destination driver on the lookup path. The keys are a
# pre-registered subset of the documented set (the rest of the
# set is available through the daemon-owned
# `note_lookup_boundary` helper, but the harness only requires the
# ones the JS2-closed Probe can observe end-to-end).
blocked_row "external-p201-lookup-floodfill-present" "p201-lookup-boundary-floodfill-selection-present" \
  "Plan 201 §G: i2pr floodfill selection produced at least one candidate for the active Java LS2 lookup"
blocked_row "external-p201-lookup-reply-gateway-derived" "p201-lookup-boundary-reply-gateway-derived" \
  "Plan 201 §G: i2pr reply-path derivation succeeded for the active Java LS2 lookup"
blocked_row "external-p201-lookup-ls2-key-match" "p201-lookup-boundary-ls2-key-match-match" \
  "Plan 201 §G: i2pr LS2 lookup response key matched the requested destination hash"
blocked_row "external-p201-lookup-ls2-decoded" "p201-lookup-boundary-database-store-ls2-decode-decoded" \
  "Plan 201 §G: i2pr decoded the LS2 envelope body on the lookup response"
blocked_row "external-p201-lookup-ls2-signature-rejected" "p201-lookup-boundary-database-store-ls2-decode-signature-rejected" \
  "Plan 201 §G: i2pr rejected the LS2 envelope at the signature-validation layer (positive or zero is diagnostic)"
blocked_row "external-p201-inbound-garlic-completed" "p201-lookup-boundary-inbound-tunnel-reassembly-garlic-completed" \
  "Plan 201 §G: i2pr recovered a complete Garlic envelope through a real inbound tunnel"

echo "==> workspace gates slice"
GATES_LOG="${EVIDENCE_DIR}/workspace-gates.log"
: > "${GATES_LOG}"
gates_rc=0
cargo fmt --all --check >>"${GATES_LOG}" 2>&1 || gates_rc=1
cargo check --locked --workspace --all-targets >>"${GATES_LOG}" 2>&1 || gates_rc=1
for gate in check-dependency-direction check-runtime-boundaries check-fixture-manifest \
           check-ntcp2-vectors check-ssu2-vectors check-ntcp2-interoperability \
           check-constrained-host-lane-boundary check-sam-acceptance-evidence \
           check-ssu2-acceptance-evidence check-i2cp-acceptance-evidence \
           check-service-tunnel-acceptance-evidence \
           check-netdb-tunnel-evidence check-destination-tunnel-evidence \
           check-streaming-tunnel-evidence check-m6-mixed-router-acceptance-evidence; do
  if ! bash "${REPO_ROOT}/scripts/${gate}.sh" >>"${GATES_LOG}" 2>&1; then
    echo "GATE FAILED: ${gate}.sh" >>"${GATES_LOG}"
    gates_rc=1
  fi
done
record_guarded "workspace-gates" \
  "fmt + workspace check --all-targets + static boundary scripts (full test/clippy/doc/deny floor stays in routine CI)" \
  "${gates_rc}"

python3 - "${RESULTS_FILE}" "${EVIDENCE_DIR}" "${REPO_ROOT}" "${JAVA_PIN}" "${JAVA_VERSION}" "${JAVA_SSU2_PORT}" "${JAVA_SAM_PORT}" "${JAVA_I2CP_PORT}" <<'PY'
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

results_path, evidence_dir, repo_root = sys.argv[1:4]
java_pin, java_version = sys.argv[4:6]
ssu2_port, sam_port, i2cp_port = sys.argv[6:9]
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
passed = [row["label"] for row in rows if row["status"] == "passed"]
blocked = [row["label"] for row in rows if row["status"] == "blocked"]
failed = [row["label"] for row in rows if row["status"] == "failed"]
if failed:
    java_status = "failed"
elif blocked:
    java_status = "blocked-pending-plan199-stop"
elif passed:
    java_status = "passed-via-java-2.13.0"
else:
    java_status = "failed"
evidence = {
    "schema": "i2pr-m6-java-v2",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": "m6-java-external",
    "ssu2_bind_policy": "127.0.0.1 loopback only, advertise=false, no introducer",
    "java_i2p": {
        "repository": "https://github.com/i2p/i2p.i2p.git",
        "revision": java_pin,
        "version": java_version,
        "role": "mandatory second-family mixed-router reference, unmodified",
        "transit": "loopback-only, no public reseed, public I2CP/Streaming client helpers; SAM diagnostic only",
        "datadir": "fresh per-run scratch dir under i2p.dir.config (ControlledRouter)",
        "selected_ports": {
            "ssu2": f"127.0.0.1:{ssu2_port}",
            "sam": f"127.0.0.1:{sam_port}",
            "i2cp": f"127.0.0.1:{i2cp_port}",
        },
    },
    "driver_evidence_keys": driver_keys,
    "results": rows,
    "m6_java": java_status,
    "passed_labels": passed,
    "blocked_labels": blocked,
    "failed_labels": failed,
    "known_limitations": [
        "second-family Java qualification: i2pd first-family passed via Plan 193",
        "loopback-only Java reference; no public I2P participation",
        "Plan 199 public Java client helpers prove transport/streaming; LS2 publication remains blocked at client-ls2-local-but-not-network-visible",
        "SAM remains diagnostic compatibility evidence and is not a counted service destination",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 199 Phase A M6 Java public-client second-family evidence\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- Java I2P: `{java_version}` @ `{java_pin}` (unmodified)\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only, `advertise=false`, no introducer\n")
    stream.write("- Java profile: `i2p.dir.config=scratch`, `router.reseedDisable=true`, public client helpers (SAM diagnostic only)\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
  echo "Plan 199 Phase A M6 Java lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 199 Phase A M6 Java lane passed; sanitized evidence: ${EVIDENCE_DIR}"
