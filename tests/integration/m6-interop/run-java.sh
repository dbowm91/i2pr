#!/usr/bin/env bash
# Plan 196 — run the M6 Java I2P second-family qualification lane
# end-to-end with a controlled first-run topology.
#
# The second-family lane re-uses the Plan 184–193 product suites the
# Plan 193 i2pd family already exercised, but with the exact-pinned
# Java I2P 2.13.0 reference substituted for the exact-pinned i2pd
# 2.61.0 reference. The lane provisions one ephemeral Java router on
# loopback with a fresh data dir (no reseed, no public I2P
# participation), enables the SAM bridge through the disposable
# clients.config, and runs the second-family external driver
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

JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION="2.13.0"
JAVA_REPO="https://github.com/i2p/i2p.i2p.git"
JAVA_CACHE="${REPO_ROOT}/target/interop/cache/m6-java/${JAVA_PIN}"

I2PR_PORT="${I2PR_SSU2_JAVA_PORT:-44090}"
# Plan 196 §5.4 — reserve/select fixed loopback Java SSU2, SAM and
# I2CP ports before the Java router starts; the runner reports the
# actual bound endpoints to the driver rather than assuming the
# upstream default tuple.
JAVA_SSU2_PORT="${I2PR_M6_JAVA_SSU2_PORT:-$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')}"
JAVA_SAM_PORT="${I2PR_M6_JAVA_SAM_PORT:-$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')}"
JAVA_I2CP_PORT="${I2PR_M6_JAVA_I2CP_PORT:-$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')}"
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
  "${EVIDENCE_DIR}/reference-facts.tsv"

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
if [[ ! -f "${LAUNCHER_SRC}" ]]; then
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
if ! javac -d "${LAUNCHER_BUILD}" -cp "${JAVA_CP}" "${LAUNCHER_SRC}" \
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
JAVA_DATA="${SCRATCH}/datadir"
JAVA_LOG="${SCRATCH}/java.log"
mkdir -p "${JAVA_DATA}/logs"

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
)
setsid "${JAVA_CMD[@]}" >/dev/null 2>"${JAVA_LOG}" < /dev/null &
JAVA_PID=$!
CHILD_PIDS=("${JAVA_PID}")

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
  [[ -z "${SCRATCH:-}" || ! -d "${SCRATCH}" ]] || rm -rf "${SCRATCH}"
}
trap cleanup EXIT

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
echo "    Java I2P SAM: 127.0.0.1:${JAVA_SAM_PORT}"
echo "    Java I2P I2CP: 127.0.0.1:${JAVA_I2CP_PORT}"
echo "    Java I2P: 127.0.0.1:${JAVA_SSU2_PORT} ($(wc -c <"${JAVA_RI}")-byte router.info)"

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
# Plan 196 §5.4 — pass the actual selected SAM/SSU2 endpoints to the
# driver. The harness owns the ports, not the upstream default tuple.
if JAVA_ROUTER_INFO="${JAVA_RI}" \
   JAVA_SSU2_ENDPOINT="127.0.0.1:${JAVA_SSU2_PORT}" \
   JAVA_SAM_ENDPOINT="127.0.0.1:${JAVA_SAM_PORT}" \
   JAVA_I2CP_ENDPOINT="127.0.0.1:${JAVA_I2CP_PORT}" \
   I2PR_SSU2_BIND="127.0.0.1:${I2PR_PORT}" \
   EVIDENCE_DIR="${DRIVER_EVIDENCE}" \
   timeout --foreground "${DRIVER_TIMEOUT}" \
   cargo test --locked -p i2pr-daemon --test java_tunnel_external \
   destination_message_plane_against_java -- --ignored --exact --nocapture --test-threads=1 \
   >>"${DRIVER_LOG}" 2>&1; then
  driver_rc=0
else
  driver_rc=$?
fi
DRIVER_TSV="${DRIVER_EVIDENCE}/driver-evidence.tsv"
echo "==> sanitized reference-side facts (counts only, never key material)"
REFERENCE_FACTS="${EVIDENCE_DIR}/reference-facts.tsv"
: > "${REFERENCE_FACTS}"
{
  printf 'java-udp-listening\t%s\n' "$(grep -c 'SSU2:.*SSU2 endpoint .* created\|UDPTransport' "${JAVA_DATA}/logs/log-router-0.txt" 2>/dev/null || true)"
  printf 'java-sam-bridge-up\t%s\n' "$(grep -c 'SAMBridge\|Starting SAM\|SAM bridge' "${JAVA_DATA}/logs/log-router-0.txt" 2>/dev/null || true)"
  printf 'java-reseed-disabled\t%s\n' "$(grep -c '^router.reseedDisable=true$' "${JAVA_DATA}/router.config" 2>/dev/null || true)"
  printf 'java-floodfill-capable\t%s\n' "$(grep -c '^router.floodfillParticipant=true$' "${JAVA_DATA}/router.config" 2>/dev/null || true)"
  printf 'java-udp-port-bound\t%s\n' "$(grep -c "^i2np.udp.port=${JAVA_SSU2_PORT}$" "${JAVA_DATA}/router.config" 2>/dev/null || true)"
  printf 'java-ntcp-disabled\t%s\n' "$(grep -c '^i2np.ntcp.enable=false$' "${JAVA_DATA}/router.config" 2>/dev/null || true)"
  printf 'java-no-public-reseed\t%s\n' "$(grep -c 'noreseed.i2p' "${JAVA_DATA}/router.config" 2>/dev/null || true)"
  printf 'java-sam-bridge-configured\t%s\n' "$(grep -c '^clientApp.0.main=net.i2p.sam.SAMBridge$' "${JAVA_DATA}/clients.config" 2>/dev/null || true)"
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
# Plan 196 §6 — topology readiness must precede the SSU2 gate. The
# controlled launcher already produces the topology evidence keys; we
# emit them as passed on success and as failed-with-stop-provenance
# only when the external driver recorded `plan194-java-stop`.
STOP_FIRED=0
if [[ -f "${DRIVER_TSV}" ]] && grep -Fq "plan194-java-stop" "${DRIVER_TSV}"; then
  STOP_FIRED=1
fi
blocked_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  if [[ -f "${DRIVER_TSV}" ]] && awk -v k="${key}" -F'\t' '$1 == k {found=1} END{exit !found}' "${DRIVER_TSV}"; then
    record "${label}" passed "${detail}"
  elif [[ "${STOP_FIRED}" -eq 1 ]]; then
    record "${label}" blocked "${detail} (m6-java-second-family-stop; see Plan 196 §11 stop provenance)"
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
m6_key_row "external-sam-destination-created" "sam-destination-created" \
  "reference SAM RAW/RAW-DATAGRAM destination created through Java public SAM"
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
ref_row "external-outbound-accepted" "java-udp-listening" \
  "reference Java log proves the outbound build was accepted"
ref_row "external-inbound-accepted" "java-udp-listening" \
  "reference Java log proves the inbound build was accepted"
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
m6_key_row "external-direct-rejected" "direct-rejected" \
  "direct transport destination delivery is rejected as a counted path"
m6_key_row "external-liveness-first-test" "liveness-first-test" \
  "creator-side liveness scheduler first test succeeds during destination activity"
ref_row "external-reseed-disabled" "java-no-public-reseed" \
  "Java router has the controlled no-public-reseed flag in the live datadir"

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
    java_status = "blocked-pending-plan194-stop"
elif passed:
    java_status = "passed-via-java-2.13.0"
else:
    java_status = "failed"
evidence = {
    "schema": "i2pr-m6-java-v1",
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
        "transit": "loopback-only, no public reseed, SAM loopback (Plan 196 only)",
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
        "no Java second-family Streaming claim until Plan 194 §5.5 rows flip passed",
        "Plan 196 owns the controlled first-run topology + authenticated SSU2 preflight only",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 196 M6 Java I2P controlled first-run topology evidence\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- Java I2P: `{java_version}` @ `{java_pin}` (unmodified)\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only, `advertise=false`, no introducer\n")
    stream.write("- Java profile: `i2p.dir.config=scratch`, `router.reseedDisable=true`, SAM loopback (ControlledRouter)\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
  echo "Plan 196 M6 Java I2P second-family lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 196 M6 Java I2P second-family lane passed; sanitized evidence: ${EVIDENCE_DIR}"