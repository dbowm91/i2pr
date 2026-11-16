#!/usr/bin/env bash
# Plan 411 — one-attempt stock Java SAM/I2CP session-create diagnostic.
#
# The diagnostic uses one fresh loopback-only Java router. The failure under
# investigation occurs while SAM creates the first DATAGRAM listener, before
# I2CP session construction, so a peer mesh is unnecessary. This runner is
# independent of Plan 279 and never invokes its qualification runner.
set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_CACHE="${REPO_ROOT}/target/interop/cache/m6-java/${JAVA_PIN}"
JAVA_SOURCE="${REPO_ROOT}/target/interop/m6-java-sources/i2p.i2p-${JAVA_PIN}"
JAVA21_HOME="${JAVA21_HOME:-/usr/lib/jvm/java-21-openjdk-amd64}"
JAVA21="${JAVA21_HOME}/bin/java"
JAVAC="${JAVAC:-$(command -v javac)}"
EVIDENCE_DIR="${I2PR_JAVA_SAM_DIAGNOSTIC_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/java-sam-diagnostic-plan411}"

[[ -f "${JAVA_CACHE}/source-revision.txt" && "$(<"${JAVA_CACHE}/source-revision.txt")" == "${JAVA_PIN}" ]] || {
  echo "Plan 411 requires the exact cached Java I2P pin ${JAVA_PIN}" >&2; exit 1;
}
[[ -f "${JAVA_CACHE}/build-metadata.txt" ]] || { echo "missing Java build metadata" >&2; exit 1; }
[[ -x "${JAVA21}" && -x "${JAVAC}" ]] || { echo "JDK 21 runtime or javac unavailable" >&2; exit 1; }
[[ -d "${JAVA_SOURCE}/.git" ]] || { echo "pinned Java source checkout unavailable" >&2; exit 1; }
[[ "$(git -C "${JAVA_SOURCE}" rev-parse HEAD)" == "${JAVA_PIN}" ]] || { echo "Java source pin mismatch" >&2; exit 1; }
[[ -z "$(git -C "${JAVA_SOURCE}" status --porcelain --untracked-files=no)" ]] || {
  echo "refusing modified Java reference source" >&2; exit 1;
}

mkdir -p "${EVIDENCE_DIR}"
if find "${EVIDENCE_DIR}" -mindepth 1 -print -quit | rg -q .; then
  echo "Plan 411 evidence directory must be empty: ${EVIDENCE_DIR}" >&2
  exit 1
fi
SCRATCH="$(mktemp -d -t i2pr-plan411-java-sam.XXXXXX)"
PIDS=()
trap 'cleanup' EXIT INT TERM

cleanup() {
  local pid
  for pid in "${PIDS[@]:-}"; do
    if ! kill -TERM -- "-${pid}" 2>/dev/null; then
      kill -TERM "${pid}" 2>/dev/null || cleanup_status=$?
    fi
  done
  for pid in "${PIDS[@]:-}"; do
    for _ in $(seq 1 20); do
      if ! kill -0 "${pid}" 2>/dev/null; then break; fi
      sleep 0.25
    done
    if kill -0 "${pid}" 2>/dev/null; then
      if ! kill -KILL -- "-${pid}" 2>/dev/null; then
        kill -KILL "${pid}" 2>/dev/null || cleanup_status=$?
      fi
    fi
    if ! wait "${pid}" 2>/dev/null; then cleanup_status=$?; fi
  done
  rm -rf "${SCRATCH}"
}

reserve_port() {
  python3 -c 'import socket; s=socket.socket(); s.bind(("127.0.0.1",0)); print(s.getsockname()[1]); s.close()'
}

UDP_PORT="$(reserve_port)"
SAM_PORT="$(reserve_port)"
I2CP_PORT="$(reserve_port)"

udp_7655_available=1
set +e
python3 - <<'PY'
import errno, socket
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
try:
    s.bind(("127.0.0.1", 7655))
except OSError as error:
    raise SystemExit(1 if error.errno == errno.EADDRINUSE else 2)
s.close()
PY
preflight_rc=$?
set -e
if [[ "${preflight_rc}" == 1 ]]; then
  udp_7655_available=0
elif [[ "${preflight_rc}" != 0 ]]; then
  echo "could not classify UDP 127.0.0.1:7655 preflight" >&2
  exit 1
fi

JAVA_CP=""
for jar in "${JAVA_CACHE}"/*.jar "${JAVA_CACHE}"/lib/*.jar; do
  [[ -f "${jar}" ]] || continue
  if [[ -z "${JAVA_CP}" ]]; then JAVA_CP="${jar}"; else JAVA_CP="${JAVA_CP}:${jar}"; fi
done
[[ -n "${JAVA_CP}" ]] || { echo "Java pin cache has no jars" >&2; exit 1; }

LAUNCHER_BUILD="${SCRATCH}/launcher"
mkdir -p "${LAUNCHER_BUILD}" "${SCRATCH}/data/logs"
JAVA_TEST_DIR="${REPO_ROOT}/tests/integration/m6-interop/java"
"${JAVAC}" --release 17 -d "${LAUNCHER_BUILD}" -cp "${JAVA_CP}" \
  "${JAVA_TEST_DIR}/ControlledRouter.java" \
  "${JAVA_TEST_DIR}/ReferenceRawDestination.java" \
  "${JAVA_TEST_DIR}/ReferenceStreamingService.java" \
  "${JAVA_TEST_DIR}"/net/i2p/router/networkdb/kademlia/*.java \
  >"${SCRATCH}/launcher-build.log" 2>&1
LAUNCHER_CP="${LAUNCHER_BUILD}:${JAVA_CP}"

setsid "${JAVA21}" -Djava.net.preferIPv4Stack=true -Djava.awt.headless=true \
  -Djava.library.path="${JAVA_CACHE}:${JAVA_CACHE}/lib" \
  -Di2p.dir.base="${JAVA_CACHE}" -DloggerFilenameOverride=logs/log-router-0.txt \
  -Drouterconsole.enable=false -cp "${LAUNCHER_CP}" \
  ControlledRouter "${SCRATCH}/data" 127.0.0.1 "${UDP_PORT}" "${SAM_PORT}" "${I2CP_PORT}" 0 transit \
  >"${SCRATCH}/router-stdout.log" 2>&1 </dev/null &
JAVA_PID="$!"
PIDS+=("${JAVA_PID}")

router_ready=0
for _ in $(seq 1 240); do
  if [[ -f "${SCRATCH}/data/router/router.info" ]] && python3 - "${UDP_PORT}" <<'PY'
import socket, sys
s = socket.socket(socket.AF_INET, socket.SOCK_DGRAM)
try:
    s.bind(("127.0.0.1", int(sys.argv[1])))
except OSError:
    raise SystemExit(0)
raise SystemExit(1)
PY
  then router_ready=1; break; fi
  if ! kill -0 "${JAVA_PID}" 2>/dev/null; then break; fi
  sleep 0.5
done
[[ "${router_ready}" == 1 ]] || { echo "Java router did not reach loopback-ready state" >&2; exit 1; }

sam_ready=0
for _ in $(seq 1 300); do
  if python3 - "${SAM_PORT}" <<'PY'
import socket, sys
try:
    s = socket.create_connection(("127.0.0.1", int(sys.argv[1])), timeout=1)
    s.close()
except OSError:
    raise SystemExit(1)
PY
  then sam_ready=1; break; fi
  sleep 1
done
[[ "${sam_ready}" == 1 ]] || { echo "Java SAM bridge did not bind loopback" >&2; exit 1; }

# One HELLO followed by exactly one SAM CREATE, matching the failed setup
# request. The raw response is kept only in memory and is never written out.
set +e
python3 - "${SAM_PORT}" "${EVIDENCE_DIR}/result.json" "${udp_7655_available}" <<'PY'
import hashlib, json, socket, sys, time, uuid
port, output = int(sys.argv[1]), sys.argv[2]
udp_7655_available = sys.argv[3] == "1"
stage, outcome, protocol_result = "sam-connect", "transport-error", "unavailable"
response_sha256 = hashlib.sha256(b"").hexdigest()
started = time.monotonic()
try:
    with socket.create_connection(("127.0.0.1", port), timeout=10) as sock:
        sock.settimeout(180)
        stream = sock.makefile("rwb")
        stream.write(b"HELLO VERSION\n")
        stream.flush()
        hello = stream.readline()
        stage = "sam-hello"
        if not hello.startswith(b"HELLO REPLY RESULT=OK"):
            outcome, protocol_result = "invalid-response", "HELLO-not-OK"
            response_sha256 = hashlib.sha256(hello).hexdigest()
        else:
            stage = "session-create"
            request = (f"SESSION CREATE STYLE=DATAGRAM ID=plan411-{uuid.uuid4().hex} "
                       "DESTINATION=TRANSIENT SIGNATURE_TYPE=7\n").encode("ascii")
            sock.sendall(request)
            response = stream.readline()
            response_sha256 = hashlib.sha256(response).hexdigest()
            if response.startswith(b"SESSION STATUS RESULT=OK"):
                outcome, protocol_result = "established", "OK"
            elif response.startswith(b"SESSION STATUS RESULT=I2P_ERROR") and b"Address already in use" in response:
                outcome, protocol_result = "address-in-use", "I2P_ERROR"
            elif response.startswith(b"SESSION STATUS RESULT=I2P_ERROR"):
                outcome, protocol_result = "sam-error", "I2P_ERROR"
            elif response:
                outcome, protocol_result = "invalid-response", "other"
            else:
                outcome, protocol_result = "timeout", "no-response"
except socket.timeout:
    outcome, protocol_result = "timeout", "read-deadline"
except ConnectionRefusedError:
    outcome, protocol_result = "transport-error", "connection-refused"
except OSError:
    outcome, protocol_result = "transport-error", "socket-error"
record = {
    "schema": "i2pr-java-sam-diagnostic-v1",
    "java_pin": "9134f808337b401e8e53c73734c81fab04280c9d",
    "topology": "single-fresh-java-router-loopback-only",
    "attempts": 1,
    "stage": stage,
    "outcome": outcome,
    "protocol_result": protocol_result,
    "udp_127_0_0_1_7655_available_before_start": udp_7655_available,
    "response_sha256": response_sha256,
    "elapsed_ms": min(180000, int((time.monotonic() - started) * 1000)),
}
with open(output, "w", encoding="utf-8") as handle:
    json.dump(record, handle, sort_keys=True, indent=2)
    handle.write("\n")
PY
diag_rc=$?
set -e
[[ "${diag_rc}" == 0 && -s "${EVIDENCE_DIR}/result.json" ]] || exit 1

# Exact-pin source trace: default SAM datagram bind, failure call order, and
# conversion of IOException.getMessage() into the SESSION STATUS response.
python3 "${REPO_ROOT}/scripts/trace-java-sam-source.py" \
  "${JAVA_SOURCE}" "${EVIDENCE_DIR}/source-trace.json"

python3 - "${EVIDENCE_DIR}" <<'PY'
import hashlib, json, pathlib, sys
root = pathlib.Path(sys.argv[1])
result = (root / "result.json").read_bytes()
trace = (root / "source-trace.json").read_bytes()
manifest = {
    "schema": "i2pr-java-sam-diagnostic-manifest-v1",
    "java_pin": "9134f808337b401e8e53c73734c81fab04280c9d",
    "source_revision": "9134f808337b401e8e53c73734c81fab04280c9d",
    "bind_policy": "127.0.0.1 only",
    "reseed": "disabled",
    "attempts": 1,
    "result_sha256": hashlib.sha256(result).hexdigest(),
    "source_trace_sha256": hashlib.sha256(trace).hexdigest(),
}
(root / "manifest.json").write_text(json.dumps(manifest, sort_keys=True, indent=2) + "\n")
PY

echo "Plan 411 diagnostic artifact: ${EVIDENCE_DIR}"
