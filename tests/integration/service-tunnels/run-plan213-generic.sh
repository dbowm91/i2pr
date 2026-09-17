#!/usr/bin/env bash
# Plan 213 — M10 router-backed generic Direction A + Direction B
# external qualification runner.
#
# Standalone fail-closed lane (Plan 213 §10): does not grow
# run-independent.sh. Reuses the exact-pinned i2pd cache/fetch
# scripts, the Plan 193 controlled i2pd profile, and the bounded
# fixture helpers instead of copying M6 orchestration wholesale.
#
# Flow:
#   1. resolve repo root + exact source head;
#   2. delete/create fresh Plan 213 evidence dir (no stale rows);
#   3. verify exact i2pd pin/version/clean cache (command-derived);
#   4. run service-tunnel static evidence checker;
#   5. run focused Plan 212 unit/source floors;
#   6. start exact-pinned i2pd with the proven Plan 193 controlled
#      profile (loopback-only, notransit=false, floodfill=true,
#      SSU2 published on loopback, SAM on loopback, NTCP2 off,
#      reseed empty);
#   7. wait for router.info + SSU2 + SAM readiness;
#   8. start the SAM STREAM server fixture (Direction A target);
#   9. capture only public destination facts (hash/b32/lengths —
#      private material never leaves scratch);
#   10. start the harness-owned loopback echo target (Direction B
#      target, with target-side digest facts);
#   11. run the ignored Rust Plan 213 generic driver with explicit
#      env (the driver performs real TCP I/O both directions with
#      concurrent production inbound pumping);
#   12. sanitize reference-side counts (counts only, raw i2pd log
#      never enters evidence);
#   13. validate every mandatory evidence key/value, including
#      exactly one terminal P213-* classification;
#   14. write results.tsv / evidence.json without secrets;
#   15. clean all children/scratch; exit nonzero unless every
#      mandatory row passes.
#
# The lane is unprivileged and loopback-only after the existing
# build-dependency installation step. No public I2P, no reseed, no
# i2pd patching.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${I2PR_PLAN213_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/plan213-generic-evidence}"
LANE_DIR="${REPO_ROOT}/tests/integration/service-tunnels"
CLIENTS_DIR="${LANE_DIR}/clients"
FIXTURES_DIR="${LANE_DIR}/fixtures"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
I2PD_CACHE="${REPO_ROOT}/target/interop/cache/ssu2/i2pd/${I2PD_PIN}"
I2PD_BIN="${I2PR_I2PD_BIN:-${I2PD_CACHE}/bin/i2pd}"
I2PD_PORT="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
I2PD_SAM_PORT="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
I2PR_PORT="${I2PR_SSU2_PLAN213_PORT:-44107}"
DRIVER_TIMEOUT="600s"
PYTHON_BIN="${PLAN213_PYTHON_BIN:-python3}"

# ---- fresh evidence (no stale rows) ------------------------------------
rm -rf "${EVIDENCE_DIR}"
mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-m10-plan213-generic.XXXXXX)"
RESULTS_FILE="${SCRATCH}/results.tsv"
: > "${RESULTS_FILE}"

CHILD_PIDS=()

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
  if [[ "${PLAN213_KEEP_SCRATCH:-0}" == "1" && -n "${SCRATCH:-}" && -d "${SCRATCH}" ]]; then
    echo "scratch kept at ${SCRATCH}" >&2
  else
    [[ -z "${SCRATCH:-}" || ! -d "${SCRATCH}" ]] || rm -rf "${SCRATCH}"
  fi
}
trap cleanup EXIT

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

SOURCE_HEAD="$(git -C "${REPO_ROOT}" rev-parse HEAD)"
echo "${SOURCE_HEAD}" > "${EVIDENCE_DIR}/source-head.txt"
echo "==> Plan 213 generic qualification at ${SOURCE_HEAD}"

# ---- exact-pin verification (command-derived, Plan 213 §E1) -------------
echo "==> verifying exact i2pd pin/version/clean cache"
pin_rc=0
if [[ ! -x "${I2PD_BIN}" ]]; then
  echo "i2pd binary missing: ${I2PD_BIN}" >&2
  echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
  pin_rc=1
fi
if [[ ! -f "${I2PD_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${I2PD_CACHE}/source-revision.txt")" != "${I2PD_PIN}" ]]; then
  echo "i2pd cache has no verified source revision" >&2
  pin_rc=1
fi
if [[ -d "${I2PD_CACHE}/source/.git" ]]; then
  if [[ "$(git -C "${I2PD_CACHE}/source" rev-parse HEAD)" != "${I2PD_PIN}" ]]; then
    echo "i2pd source checkout is not at the exact pin" >&2
    pin_rc=1
  fi
  if [[ -n "$(git -C "${I2PD_CACHE}/source" status --porcelain --untracked-files=no 2>/dev/null)" ]]; then
    echo "i2pd source checkout has tracked modifications (patching forbidden)" >&2
    pin_rc=1
  fi
fi
I2PD_REPORTED=""
if [[ "${pin_rc}" -eq 0 ]]; then
  I2PD_REPORTED="$("${I2PD_BIN}" --version 2>&1 | head -n1 || true)"
  if ! printf '%s' "${I2PD_REPORTED}" | grep -Fq "${I2PD_VERSION}"; then
    echo "i2pd binary does not report ${I2PD_VERSION}: ${I2PD_REPORTED}" >&2
    pin_rc=1
  fi
fi
{
  echo "source_head=${SOURCE_HEAD}"
  echo "i2pd_pin=${I2PD_PIN}"
  echo "i2pd_version=${I2PD_VERSION}"
  echo "i2pd_reported=${I2PD_REPORTED}"
} > "${EVIDENCE_DIR}/pin-facts.txt"
record_guarded "plan213-i2pd-pin-sha" \
  "exact i2pd pin ${I2PD_PIN} verified clean" "${pin_rc}"
record_guarded "plan213-i2pd-version" \
  "i2pd reports ${I2PD_VERSION}" "${pin_rc}"
record_guarded "plan213-i2pd-cache-clean" \
  "reference cache verified, no tracked modifications" "${pin_rc}"
record_guarded "plan213-source-head" \
  "qualification source head ${SOURCE_HEAD}" 0
if [[ "${pin_rc}" -ne 0 ]]; then
  echo "pin verification failed; refusing to start the reference" >&2
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi

# ---- static checker ------------------------------------------------------
echo "==> service-tunnel static evidence checker"
checker_rc=0
bash "${REPO_ROOT}/scripts/check-service-tunnel-acceptance-evidence.sh" \
  >"${EVIDENCE_DIR}/static-checker.log" 2>&1 || checker_rc=$?
record_guarded "plan213-static-checker" \
  "check-service-tunnel-acceptance-evidence.sh incl. Plan 213 §H invariants" \
  "${checker_rc}"

# ---- focused Plan 212 unit/source floors ---------------------------------
echo "==> focused Plan 212 unit/source floors"
floor_rc=0
cargo test --locked -p i2pr-daemon --lib plan212 -- --test-threads=1 \
  >>"${EVIDENCE_DIR}/unit-floor.log" 2>&1 || floor_rc=$?
cargo test --locked -p i2pr-daemon --lib plan213 -- --test-threads=1 \
  >>"${EVIDENCE_DIR}/unit-floor.log" 2>&1 || floor_rc=$?
cargo test --locked -p i2pr-daemon --test service_tunnels_plan212_router_backed_product -- \
  --test-threads=1 >>"${EVIDENCE_DIR}/unit-floor.log" 2>&1 || floor_rc=$?
record_guarded "plan213-unit-floor" \
  "plan212/plan213 lib rows + driver unit rows (cargo test, ignored external driver skipped)" \
  "${floor_rc}"

# ---- ephemeral i2pd (Plan 193 controlled profile) -------------------------
I2PD_HOME="${SCRATCH}/i2pd"
I2PD_DATA="${I2PD_HOME}/data"
# The raw i2pd log stays in scratch: it carries SAM session lines
# that may embed reference key material. Only sanitized counts
# extracted below reach evidence.
I2PD_LOG="${SCRATCH}/i2pd.log"
mkdir -p "${I2PD_DATA}"
cat > "${I2PD_HOME}/i2pd.conf" <<EOF
daemon = false
loglevel = debug
netid = 2
address4 = 127.0.0.1
host = 127.0.0.1
port = ${I2PD_PORT}
ipv4 = true
ipv6 = false
nat = false
notransit = false
floodfill = true
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
enabled = true
address = 127.0.0.1
port = ${I2PD_SAM_PORT}
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
CHILD_PIDS+=("${I2PD_PID}")

echo "==> waiting for ephemeral i2pd on 127.0.0.1:${I2PD_PORT} (SAM ${I2PD_SAM_PORT})"
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
  record "plan213-reference-router-ready" failed "no router.info / SSU2 listener"
  record "plan213-terminal-classification" failed "P213-A-reference-startup"
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi
record_guarded "plan213-reference-router-ready" \
  "router.info + SSU2 listener on 127.0.0.1:${I2PD_PORT}" 0
SAM_READY=0
for _ in $(seq 1 60); do
  if (exec 3<>"/dev/tcp/127.0.0.1/${I2PD_SAM_PORT}") 2>/dev/null; then
    exec 3<&- 3>&- || true
    SAM_READY=1
    break
  fi
  if ! kill -0 "${I2PD_PID}" 2>/dev/null; then
    echo "ephemeral i2pd exited before SAM came up" >&2
    exit 2
  fi
  sleep 0.5
done
if [[ "${SAM_READY}" -ne 1 ]]; then
  echo "ephemeral i2pd SAM did not listen on 127.0.0.1:${I2PD_SAM_PORT}" >&2
  record "plan213-reference-sam-ready" failed "no SAM listener"
  record "plan213-terminal-classification" failed "P213-B-reference-sam-session"
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi
record_guarded "plan213-reference-sam-ready" \
  "SAM bridge on 127.0.0.1:${I2PD_SAM_PORT}" 0

# ---- Direction A target: SAM STREAM server fixture ------------------------
echo "==> starting SAM STREAM server fixture (Direction A target)"
FIXTURE_PUB="${SCRATCH}/fixture-server.pub"
FIXTURE_FACTS="${EVIDENCE_DIR}/fixture-server-facts.txt"
ACCEPT_TRIGGER="${SCRATCH}/accept-trigger"
: > "${EVIDENCE_DIR}/fixture-server.log"
setsid "${PYTHON_BIN}" "${CLIENTS_DIR}/sam_stream_fixture.py" \
  --mode server --sam "127.0.0.1:${I2PD_SAM_PORT}" \
  --session-id "plan213-a-${I2PD_PORT}" \
  --pub-file "${FIXTURE_PUB}" --facts "${FIXTURE_FACTS}" \
  --accept-trigger "${ACCEPT_TRIGGER}" \
  --expected-connections 2 \
  >>"${EVIDENCE_DIR}/fixture-server.log" 2>&1 < /dev/null &
FIXTURE_PID=$!
CHILD_PIDS+=("${FIXTURE_PID}")
READY=""
for _ in $(seq 1 120); do
  if grep -Fq "READY=1" "${EVIDENCE_DIR}/fixture-server.log" 2>/dev/null; then
    READY="1"
    break
  fi
  if ! kill -0 "${FIXTURE_PID}" 2>/dev/null; then
    echo "fixture server exited during setup" >&2
    cat "${EVIDENCE_DIR}/fixture-server.log" >&2 || true
    break
  fi
  sleep 0.5
done
fixture_rc=0
if [[ "${READY}" != "1" || ! -s "${FIXTURE_PUB}" ]]; then
  echo "SAM STREAM fixture never became ready" >&2
  fixture_rc=1
fi
# Recompute the public destination facts from the scratch PUB file
# and require equality with the fixture-reported facts: only the
# recomputed public values enter the driver environment, and only
# digests/lengths enter evidence.
DEST_B64=""; DEST_HASH=""; DEST_B32=""
if [[ "${fixture_rc}" -eq 0 ]]; then
  DEST_B64="$(tr -d ' \n' < "${FIXTURE_PUB}")"
  read -r DEST_HASH DEST_B32 <<<"$("${PYTHON_BIN}" - "${DEST_B64}" <<'PY'
import base64, hashlib, sys
pub = sys.argv[1].strip().replace("-", "+").replace("~", "/")
pub += "=" * ((-len(pub)) % 4)
raw = base64.b64decode(pub)
digest = hashlib.sha256(raw).digest()
b32 = base64.b32encode(digest).decode("ascii").rstrip("=").lower() + ".b32.i2p"
print(f"{digest.hex()} {b32}")
PY
)"
  if [[ "$(grep -E '^dest_hash=' "${FIXTURE_FACTS}" | cut -d= -f2)" != "${DEST_HASH}" ]] ||
     [[ "$(grep -E '^dest_b32=' "${FIXTURE_FACTS}" | cut -d= -f2)" != "${DEST_B32}" ]]; then
    echo "fixture-reported destination facts diverge from recomputation" >&2
    fixture_rc=1
  fi
fi
record_guarded "plan213-fixture-server-ready" \
  "SAM STREAM fixture READY with recomputed public destination (hash=${DEST_HASH:0:12}...)" \
  "${fixture_rc}"
if [[ "${fixture_rc}" -ne 0 ]]; then
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi
# Bounded stabilization: i2pd's SAM bridge publishes the fresh
# STREAM destination LeaseSet2 into its own NetDB asynchronously.
echo "    fixture destination hash=${DEST_HASH:0:16}... b32=${DEST_B32:0:16}... (stabilizing)"
sleep 15

# ---- Direction B target: harness-owned loopback echo target ---------------
echo "==> starting loopback echo target (Direction B target)"
ECHO_FACTS="${EVIDENCE_DIR}/echo-target-facts.txt"
"${PYTHON_BIN}" "${FIXTURES_DIR}/echo_fixture.py" --port 0 \
  --max-connections 8 --facts "${ECHO_FACTS}" \
  >"${SCRATCH}/echo-target-port.log" 2>&1 &
ECHO_PID=$!
CHILD_PIDS+=("${ECHO_PID}")
ECHO_TARGET=""
for _ in $(seq 1 100); do
  ECHO_TARGET="$(grep -E '^PORT=' "${SCRATCH}/echo-target-port.log" 2>/dev/null | head -n1 | cut -d= -f2 || true)"
  [[ -n "${ECHO_TARGET}" ]] && break
  sleep 0.1
done
echo_rc=0
[[ -n "${ECHO_TARGET}" ]] || echo_rc=1
record_guarded "plan213-echo-target-ready" \
  "harness-owned echo target on 127.0.0.1:${ECHO_TARGET:-unset} with digest facts" \
  "${echo_rc}"
if [[ "${echo_rc}" -ne 0 ]]; then
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi

# ---- counted Rust driver ---------------------------------------------------
echo "==> counted Plan 213 generic driver (explicit ignored selection)"
DRIVER_LOG="${EVIDENCE_DIR}/external-driver.log"
: > "${DRIVER_LOG}"
driver_rc=0
if I2PD_ROUTER_INFO="${I2PD_RI}" \
   I2PD_SSU2_ENDPOINT="127.0.0.1:${I2PD_PORT}" \
   I2PR_SSU2_BIND="127.0.0.1:${I2PR_PORT}" \
   I2PD_SAM_ENDPOINT="127.0.0.1:${I2PD_SAM_PORT}" \
   EVIDENCE_DIR="${EVIDENCE_DIR}/driver" \
   PLAN213_GENERIC_DEST_B64="${DEST_B64}" \
   PLAN213_GENERIC_DEST_HASH="${DEST_HASH}" \
   PLAN213_GENERIC_DEST_B32="${DEST_B32}" \
   PLAN213_SERVER_TARGET_PORT="${ECHO_TARGET}" \
   PLAN213_A_TARGET_FACTS="${FIXTURE_FACTS}" \
   PLAN213_A_TRIGGER="${ACCEPT_TRIGGER}" \
   PLAN213_B_TARGET_FACTS="${ECHO_FACTS}" \
   PLAN213_SAM_FIXTURE="${CLIENTS_DIR}/sam_stream_fixture.py" \
   PLAN213_PYTHON_BIN="${PYTHON_BIN}" \
   timeout --foreground "${DRIVER_TIMEOUT}" \
   cargo test --locked -p i2pr-daemon --test service_tunnels_plan212_router_backed_product \
   plan212_router_backed_generic_directions -- --ignored --exact --nocapture --test-threads=1 \
   >>"${DRIVER_LOG}" 2>&1; then
  driver_rc=0
else
  driver_rc=$?
fi
DRIVER_TSV="${EVIDENCE_DIR}/driver/driver-evidence.tsv"

# ---- sanitized reference-side counts (never key material) ------------------
# The raw i2pd log stays in scratch (it carries SAM session
# lines). Only per-line-prefixed transit/tunnel/NetDB/Streaming
# counts — plus a bounded excerpt of those same sanitized
# prefixes for timing diagnosis — reach evidence. The excerpt
# filter admits no SAM/SESSION/DEST/PRIV line.
REFERENCE_FACTS="${EVIDENCE_DIR}/reference-facts.tsv"
: > "${REFERENCE_FACTS}"
{
  printf 'transit-endpoint-created\t%s\n' "$(grep -c 'TransitTunnel: endpoint .* created' "${I2PD_LOG}" 2>/dev/null || true)"
  printf 'transit-gateway-created\t%s\n' "$(grep -c 'TransitTunnel: gateway .* created' "${I2PD_LOG}" 2>/dev/null || true)"
  printf 'reference-leaseset-updated\t%s\n' "$(grep -c 'NetDb: LeaseSet2 updated' "${I2PD_LOG}" 2>/dev/null || true)"
  printf 'reference-sam-bridge-up\t%s\n' "$(grep -c 'Starting SAM bridge' "${I2PD_LOG}" 2>/dev/null || true)"
  printf 'reference-streaming-accepted\t%s\n' "$(grep -c 'Streaming: Incoming stream from ' "${I2PD_LOG}" 2>/dev/null || true)"
} >> "${REFERENCE_FACTS}"
grep -E 'TransitTunnel: (endpoint|gateway).*created|Tunnels: Test of .* successful|NetDb: LeaseSet2 updated|Streaming: Incoming stream from |Starting SAM bridge|Start listening on ' "${I2PD_LOG}" 2>/dev/null \
  | grep -v -iE 'sam|session|dest |priv|naming|streaming destination' \
  | tail -n 60 > "${EVIDENCE_DIR}/reference-transit-excerpt.log" || true
# Bounded secret-filtered debug excerpt for delivery diagnosis
# (Plan 213 failure provenance only): admits Garlic / TunnelData /
# ECIES / LeaseSet / DatabaseLookup delivery lines, rejects any
# line that could carry SAM session material, destinations, or
# keys. Never the raw log.
grep -iE 'garlic|tunneldata|tunnel data|ecies|aead|decrypt|crypto|leaseset|databaselookup|databasestore|streaming|tunnel build|tunnel reply|delivery status|short request|record .* ours|ssu2.*(session|established|closed|destroy|timeout|peer)|invalid|drop|reject|fail|error|mismatch|unknown|expired|stale|unrecognized|unhandled|ignore' "${I2PD_LOG}" 2>/dev/null \
  | grep -v -iE 'sam|session create|session status|dest |priv|pub=|hello|naming|password|user|nick|ping|pong|payload:|bytes \[|key ' \
  | tail -n 300 > "${EVIDENCE_DIR}/i2pd-debug-excerpt.log" || true

# ---- mandatory evidence validation ------------------------------------------
# Key-gated on the fresh per-run driver TSV (stale evidence was
# deleted at startup): the key proves the driver executed that
# step in this run. Exact-value rows additionally require the
# documented value; delta rows require a positive integer.
DRIVER_FILES=("${DRIVER_TSV}")
if [[ -f "${DRIVER_LOG}" ]]; then
  DRIVER_FILES+=("${DRIVER_LOG}")
fi

tsv_value() {
  local key="$1"
  local file value
  for file in "${DRIVER_FILES[@]}"; do
    [[ -f "${file}" ]] || continue
    value="$(awk -F'\t' -v want="${key}" '$1 == want {print $2; exit}' "${file}" 2>/dev/null || true)"
    if [[ -n "${value}" ]]; then
      printf '%s' "${value}"
      return 0
    fi
  done
  return 1
}

# Duplicate conflicting keys are never valid evidence (Plan 213
# §15 item 17): the same label with two different values fails
# the row even if one value looks right.
duplicate_conflict() {
  local key="$1"
  [[ -f "${DRIVER_TSV}" ]] || return 1
  local distinct
  distinct="$(awk -F'\t' -v want="${key}" '$1 == want {print $2}' "${DRIVER_TSV}" 2>/dev/null | sort -u | wc -l || true)"
  [[ "${distinct}" -gt 1 ]]
}

m213_key_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  local rc=1
  if [[ "${driver_rc}" -eq 0 ]] && tsv_value "${key}" >/dev/null; then
    if ! duplicate_conflict "${key}"; then
      rc=0
    fi
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}

m213_exact_row() {
  local label="$1"
  local key="$2"
  local expected="$3"
  local detail="$4"
  local rc=1
  local value
  if value="$(tsv_value "${key}")" && [[ "${value}" == "${expected}" ]]; then
    if ! duplicate_conflict "${key}"; then
      rc=0
    fi
  fi
  record_guarded "${label}" "${detail} (value=${value:-missing})" "${rc}"
}

m213_positive_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  local rc=1
  local value
  if value="$(tsv_value "${key}")" && [[ "${value}" =~ ^[0-9]+$ ]] && (( value >= 1 )); then
    if ! duplicate_conflict "${key}"; then
      rc=0
    fi
  fi
  record_guarded "${label}" "${detail} (value=${value:-missing})" "${rc}"
}

m213_key_row "plan212-service-destination-hash" "plan212-service-destination-hash" \
  "generic target destination hash recorded"
m213_exact_row "plan212-router-bootstrap-ok" "plan212-router-bootstrap-ok" "1" \
  "production composition started (dial + bootstrap + provisioning)"
m213_positive_row "plan212-real-outbound-installed" "plan212-real-outbound-installed" \
  "real outbound material expiry derived from typed summary"
m213_positive_row "plan212-real-inbound-installed" "plan212-real-inbound-installed" \
  "real inbound receive count derived from typed summary"
m213_positive_row "plan212-local-ls2-real-lease-count" "plan212-local-ls2-real-lease-count" \
  "real local LS2 lease count derived from typed summary"
m213_exact_row "plan212-inbound-owner-registered" "plan212-inbound-owner-registered" "1" \
  "inbound receive ids resolve to registered owners"
m213_positive_row "plan212-remote-ls2-lookup-started" "plan212-remote-ls2-lookup-started" \
  "remote LS2 lookups started through the coordinator"
m213_positive_row "plan212-remote-ls2-lookup-succeeded" "plan212-remote-ls2-lookup-succeeded" \
  "remote LS2 lookups reached validated cache records"
m213_exact_row "plan212-direction-a-stream-established" "plan212-direction-a-stream-established" "1" \
  "Direction A byte round trip through the GenericClient listener"
m213_exact_row "plan212-direction-a-small-digest-match" "plan212-direction-a-small-digest-match" "1" \
  "Direction A small payload digest equality"
m213_exact_row "plan212-direction-a-large-digest-match" "plan212-direction-a-large-digest-match" "1" \
  "Direction A multi-packet payload digest equality"
m213_exact_row "plan212-direction-a-inbound-streaming-accepted" "plan212-direction-a-inbound-streaming-accepted" "1" \
  "Direction A positive remote inbound dispatched delta"
m213_exact_row "plan213-direction-a-client-exit" "plan213-direction-a-client-exit" "0" \
  "Direction A local TCP exchange exit code"
m213_exact_row "plan213-direction-a-target-observed-small" "plan213-direction-a-target-observed-small" "1" \
  "Direction A target observed the small digest"
m213_exact_row "plan213-direction-a-target-observed-large" "plan213-direction-a-target-observed-large" "1" \
  "Direction A target observed the large digest"
m213_exact_row "plan212-direction-b-local-ls2-published" "plan212-direction-b-local-ls2-published" "1" \
  "Direction B server LS2 publication proven by reference connect"
m213_exact_row "plan212-direction-b-inbound-owner-hit" "plan212-direction-b-inbound-owner-hit" "1" \
  "Direction B inbound dispatch through the registered owner"
m213_exact_row "plan212-direction-b-stream-established" "plan212-direction-b-stream-established" "1" \
  "Direction B bidirectional digest equality via independent initiator"
m213_exact_row "plan212-direction-b-small-digest-match" "plan212-direction-b-small-digest-match" "1" \
  "Direction B small payload digest equality"
m213_exact_row "plan212-direction-b-large-digest-match" "plan212-direction-b-large-digest-match" "1" \
  "Direction B multi-packet payload digest equality"
m213_exact_row "plan213-direction-b-reference-connect-exit" "plan213-direction-b-reference-connect-exit" "0" \
  "Direction B independent i2pd CONNECT helper exit code"
m213_exact_row "plan213-direction-b-target-observed-small" "plan213-direction-b-target-observed-small" "1" \
  "Direction B target observed the small digest"
m213_exact_row "plan213-direction-b-target-observed-large" "plan213-direction-b-target-observed-large" "1" \
  "Direction B target observed the large digest"
m213_positive_row "plan213-direction-a-remote-outbound-delta" "plan213-direction-a-remote-outbound-delta" \
  "Direction A remote outbound composed delta"
m213_positive_row "plan213-direction-a-remote-inbound-delta" "plan213-direction-a-remote-inbound-delta" \
  "Direction A remote inbound dispatched delta"
m213_positive_row "plan213-direction-b-remote-outbound-delta" "plan213-direction-b-remote-outbound-delta" \
  "Direction B response-path remote outbound composed delta"
m213_positive_row "plan213-direction-b-remote-inbound-delta" "plan213-direction-b-remote-inbound-delta" \
  "Direction B remote inbound dispatched delta"
m213_exact_row "plan212-orphan-receive-delta-zero" "plan212-orphan-receive-delta-zero" "1" \
  "zero orphan receives over the run"
m213_exact_row "plan212-local-coowned-delta-zero" "plan212-local-coowned-delta-zero" "1" \
  "zero local co-owned substitution over the run"
m213_exact_row "plan212-unknown-peer-delta-zero" "plan212-unknown-peer-delta-zero" "1" \
  "zero unknown-peer outcomes over the run"
m213_exact_row "plan212-resource-baseline-clean" "plan212-resource-baseline-clean" "1" \
  "checked product shutdown"

# Exactly one terminal classification per run, and it must be the
# pass class (Plan 213 §14).
terminal_count=0
terminal_value=""
if [[ -f "${DRIVER_TSV}" ]]; then
  terminal_count="$(awk -F'\t' '$1 == "plan213-terminal-classification" {count++} END{print count+0}' "${DRIVER_TSV}")"
  terminal_value="$(awk -F'\t' '$1 == "plan213-terminal-classification" {print $2; exit}' "${DRIVER_TSV}")"
fi
terminal_rc=1
if [[ "${terminal_count}" -eq 1 && "${terminal_value}" == "P213-N-passed" ]]; then
  terminal_rc=0
fi
record_guarded "plan213-terminal-classification" \
  "exactly one terminal class P213-N-passed (count=${terminal_count} value=${terminal_value:-missing})" \
  "${terminal_rc}"

# Reference-side counts: the controlled reference must show tunnel
# acceptance both directions, at least one stored LS2, and at
# least one accepted incoming stream.
ref_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  local rc=1
  local count
  count="$(awk -F'\t' -v want="${key}" '$1 == want {print $2; exit}' "${REFERENCE_FACTS}" 2>/dev/null || true)"
  if [[ "${count:-0}" =~ ^[0-9]+$ ]] && (( count >= 1 )); then
    rc=0
  fi
  record_guarded "${label}" "${detail} (count=${count:-missing})" "${rc}"
}
ref_row "plan213-reference-outbound-accepted" "transit-endpoint-created" \
  "reference transit log proves the outbound build was accepted"
ref_row "plan213-reference-inbound-accepted" "transit-gateway-created" \
  "reference transit log proves the inbound build was accepted"
ref_row "plan213-reference-ls2-stored" "reference-leaseset-updated" \
  "reference floodfill stored a LeaseSet2"
ref_row "plan213-reference-streaming-accepted" "reference-streaming-accepted" \
  "reference Streaming log proves a SYN was accepted"

# ---- cleanup + resource baseline -------------------------------------------
echo "==> resource baseline"
for pid in "${CHILD_PIDS[@]:-}"; do
  stop_group "${pid}"
done
for pid in "${CHILD_PIDS[@]:-}"; do
  wait "${pid}" 2>/dev/null || true
done
CHILD_PIDS=()
sleep 1
resource_rc=0
if pgrep -f "sam_stream_fixture" >/dev/null 2>&1; then
  echo "fixture process survived shutdown" >&2
  resource_rc=1
fi
if pgrep -f "echo_fixture" >/dev/null 2>&1; then
  echo "echo target process survived shutdown" >&2
  resource_rc=1
fi
if pgrep -f "i2pd.*--datadir=${SCRATCH}/i2pd" >/dev/null 2>&1; then
  echo "ephemeral i2pd process survived shutdown" >&2
  resource_rc=1
fi
if (echo > "/dev/tcp/127.0.0.1/${ECHO_TARGET}") 2>/dev/null; then
  echo "echo target port still accepts after shutdown" >&2
  resource_rc=1
fi
record_guarded "plan213-clean-resource-baseline" \
  "fixture/echo/i2pd shutdown leaves no process and loopback ports refused" \
  "${resource_rc}"

# ---- no-secret audit ---------------------------------------------------------
# Evidence must carry digests/lengths/counters only: reject any
# private-destination marker or any long opaque token that could
# be key material (a PUB base64 is 500+ chars; no evidence value
# may be that long).
secret_rc=0
if grep -rEi 'PRIV|private.?key' "${EVIDENCE_DIR}" >/dev/null 2>&1; then
  echo "evidence may carry private material markers" >&2
  grep -rEi 'PRIV|private.?key' "${EVIDENCE_DIR}" | head -5 >&2 || true
  secret_rc=1
fi
# `seed` alone also matches i2pd's own `Reseed: ...` log lines, so
# exclude that word before failing.
if grep -rEi 'seed' "${EVIDENCE_DIR}" 2>/dev/null | grep -vi 'reseed' | head -1 | grep -q .; then
  echo "evidence may carry private material markers" >&2
  grep -rEi 'seed' "${EVIDENCE_DIR}" 2>/dev/null | grep -vi 'reseed' | head -5 >&2 || true
  secret_rc=1
fi
if [[ ! -f "${EVIDENCE_DIR}/driver/driver-evidence.tsv" ]]; then
  echo "counted driver produced no evidence TSV" >&2
  secret_rc=1
elif awk -F'\t' '{for (i=1; i<=NF; i++) if (length($i) > 400) exit 1}' \
    "${EVIDENCE_DIR}/driver/driver-evidence.tsv" 2>/dev/null; then
  :
else
  echo "evidence carries an overlong token (possible key material)" >&2
  secret_rc=1
fi
record_guarded "plan213-no-secret-leak" \
  "evidence carries no private markers and no overlong tokens" \
  "${secret_rc}"

# ---- evidence.json -------------------------------------------------------------
python3 - "${RESULTS_FILE}" "${EVIDENCE_DIR}" "${REPO_ROOT}" "${I2PD_PIN}" "${I2PD_VERSION}" "${SOURCE_HEAD}" <<'PY'
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

results_path, evidence_dir, repo_root = sys.argv[1:4]
i2pd_pin, i2pd_version, source_head = sys.argv[4:7]
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
        driver_keys.append(line.split("\t", 1)[0])
terminal = [key for key in driver_keys if key == "plan213-terminal-classification"]
evidence = {
    "schema": "i2pr-plan213-generic-v1",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "source_head": source_head,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": "plan213-generic-external",
    "ssu2_bind_policy": "127.0.0.1 loopback only, advertise=false, no introducer",
    "i2pd": {
        "repository": "https://github.com/PurpleI2P/i2pd.git",
        "revision": i2pd_pin,
        "version": i2pd_version,
        "role": "mandatory independent generic STREAM reference, unmodified",
        "transit": "notransit=false, floodfill=true, SAM loopback (Plan 213 generic qualification only)",
    },
    "driver_evidence_keys": driver_keys,
    "terminal_classification": terminal[0] if len(terminal) == 1 else f"invalid-count-{len(terminal)}",
    "results": rows,
    "plan213_generic": "passed" if all(
        row["status"] == "passed" for row in rows
    ) else "failed",
    "known_limitations": [
        "generic STREAM qualification only; HTTP/IRC application rows belong to Plan 214",
        "one-hop service tunnels only; no multi-hop tunnel build",
        "loopback-only i2pd reference; no public I2P participation",
        "digest evidence only; no payload bytes or key material in evidence",
    ],
}
out = Path(evidence_dir)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 213 M10 router-backed generic external qualification evidence\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- i2pd: `{i2pd_version}` @ `{i2pd_pin}` (unmodified)\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only, `advertise=false`, no introducer\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
  echo "Plan 213 generic qualification lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 213 generic qualification lane passed; sanitized evidence: ${EVIDENCE_DIR}"
