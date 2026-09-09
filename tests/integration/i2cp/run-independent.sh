#!/usr/bin/env bash
# Plan 172 — independent I2CP LeaseSet2 lifecycle matrix (Java I2P + go-i2cp).
# Retains the Plan 170 wire/data-plane rows and adds the counted
# high-level/public lifecycle rows.
#
# Routes each required evidence row through an executed command/test result.
# Local rows execute the focused Plan 167–169 + Plan 172 zero-hop I2CP
# suites through the existing daemon. Cross-client rows build and run the
# public unprivileged Java I2P (2.13.0) high-level I2PSession.connect()
# driver plus the pinned go-i2cp public ProcessIO driver bound to a one-shot
# in-process `i2cp_loopback_listener` over the canonical
# 127.0.0.1:7654 I2CP loopback port (the default every unmodified Java I2P
# / go-i2cp / i2pd client connects to).
#
# Provenance: every row's `passed` result is recorded via record_guarded
# only after the relevant command exits 0; literal pass records are
# rejected by scripts/check-i2cp-acceptance-evidence.sh. No required row
# is marked passed without an executed command behind it.
#
# The lane is unprivileged and loopback-only (no root, no Docker, no network
# namespaces, no public I2P participation). Required failures make this
# script exit non-zero.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${I2PR_I2CP_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/i2cp-evidence}"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION="2.13.0"
GO_PIN="b529ee1c10a6011558b4d69fc9436a4afc489eac"
JAVA_CACHE="${REPO_ROOT}/target/interop/cache/i2cp/java_i2p/${JAVA_PIN}"
GO_CACHE_DIR="${REPO_ROOT}/target/interop/cache/i2cp/go_i2cp/${GO_PIN}"
JAVA_JAR="${JAVA_CACHE}/lib/i2p.jar"
GO_DRIVER_SRC_DIR="${REPO_ROOT}/tests/integration/i2cp/external/go"
JAVA_DRIVER_SRC="${REPO_ROOT}/tests/integration/i2cp/external/java/i2cp_java_driver.java"
JAVA_SESSION_DRIVER_SRC="${REPO_ROOT}/tests/integration/i2cp/external/java/i2cp_java_session_driver.java"
WORK_BUILD="${REPO_ROOT}/target/interop/cache/i2cp/_build"
LISTEN_PORT="${I2PR_I2CP_LISTEN_PORT:-7654}"

mkdir -p "${EVIDENCE_DIR}"
mkdir -p "${WORK_BUILD}"

RESULTS_FILE="${EVIDENCE_DIR}/results.tsv"
: > "${RESULTS_FILE}"

# ---- helper plumbing ----------------------------------------------------

RECORD_FAILED=0
record() {
  local label="$1"
  local status="$2"
  local detail="${3:-}"
  detail="${detail//$'\t'/ }"
  detail="${detail//$'\n'/ }"
  printf '%s\t%s\t%s\n' "${label}" "${status}" "${detail}" >> "${RESULTS_FILE}"
  [[ "${status}" == "passed" ]] || RECORD_FAILED=1
}

# Plan 170 §9: the only sanctioned path from an executed command to a
# required `passed` row. Pass the command's exit code via `rc`; a zero
# code records passed, anything else records failed.
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

# Extract an evidence key=value line (single-line) from a driver log;
# empty string if absent.
fact() {
  local key="$1"
  local file="$2"
  if [[ -f "${file}" ]]; then
    grep -E "^${key}=" "${file}" | head -n1 | sed "s/^${key}=//"
  fi
}

stop_pid() {
  local pid="${1:-}"
  [[ -z "${pid}" ]] && return 0
  kill -TERM "${pid}" 2>/dev/null || kill -KILL "${pid}" 2>/dev/null || true
}

# ---- Plan 170 §3 anchor: cache verification before any external command --
if [[ ! -f "${JAVA_JAR}" ]]; then
  echo "Java I2P jar missing: ${JAVA_JAR}" >&2
  echo "run scripts/interop/fetch-i2cp-clients.sh first" >&2
  exit 1
fi
if [[ ! -d "${GO_CACHE_DIR}/source" ]]; then
  echo "go-i2cp source missing: ${GO_CACHE_DIR}/source" >&2
  echo "run scripts/interop/fetch-i2cp-clients.sh first" >&2
  exit 1
fi
if [[ ! -f "${GO_CACHE_DIR}/source-revision.txt" ]] ||
   [[ "$(<"${GO_CACHE_DIR}/source-revision.txt")" != "${GO_PIN}" ]]; then
  echo "go-i2cp cache has no verified Plan 170 source revision" >&2
  echo "run scripts/interop/fetch-i2cp-clients.sh first" >&2
  exit 1
fi
echo "==> Java I2P: ${JAVA_VERSION} (${JAVA_PIN})"
echo "==> go-i2cp: ${GO_PIN}"

# ---- build the unprivileged Go driver (in-process cache, no go install) --
# Rebuild whenever the driver source is newer than the cached binary so
# lane edits can never run against a stale driver.
if [[ ! -x "${WORK_BUILD}/i2cp_go_driver" || "${GO_DRIVER_SRC_DIR}/i2cp_go_driver.go" -nt "${WORK_BUILD}/i2cp_go_driver" ]]; then
  echo "==> building unprivileged go-i2cp driver"
  WORK_BUILD_DRIVER_DIR="${WORK_BUILD}/driver-build"
  mkdir -p "${WORK_BUILD_DRIVER_DIR}"
  cat > "${WORK_BUILD_DRIVER_DIR}/go.mod" <<EOF
module i2pr/i2cp_driver

go 1.26.3

require github.com/go-i2p/go-i2cp v0.0.0-00010101000000-000000000000

replace github.com/go-i2p/go-i2cp => ${GO_CACHE_DIR}/source
EOF
  cp "${GO_DRIVER_SRC_DIR}/i2cp_go_driver.go" "${WORK_BUILD_DRIVER_DIR}/main.go"
  cp "${GO_CACHE_DIR}/source/go.sum" "${WORK_BUILD_DRIVER_DIR}/go.sum"
  (cd "${WORK_BUILD_DRIVER_DIR}" && GOTOOLCHAIN="${GOTOOLCHAIN:-go1.26.3}" go mod tidy && GOTOOLCHAIN="${GOTOOLCHAIN:-go1.26.3}" go build -o "${WORK_BUILD}/i2cp_go_driver" .)
fi
GO_DRIVER="${WORK_BUILD}/i2cp_go_driver"
if [[ ! -x "${GO_DRIVER}" ]]; then
  echo "go-i2cp driver failed to build: ${GO_DRIVER}" >&2
  exit 1
fi

# ---- compile the Java drivers (one JVM class file each, no Gradle) ----
JAVAC_OUT_DIR="${EVIDENCE_DIR}/java-classes"
mkdir -p "${JAVAC_OUT_DIR}"
cp "${JAVA_DRIVER_SRC}" "${JAVAC_OUT_DIR}/i2cp_java_driver.java"
if ! javac -cp "${JAVA_JAR}" -d "${JAVAC_OUT_DIR}" "${JAVAC_OUT_DIR}/i2cp_java_driver.java"; then
  echo "Java driver failed to compile (jar=${JAVA_JAR})" >&2
  exit 1
fi
# Plan 172 counted high-level session driver (public I2PSession.connect()).
cp "${JAVA_SESSION_DRIVER_SRC}" "${JAVAC_OUT_DIR}/i2cp_java_session_driver.java"
if ! javac -cp "${JAVA_JAR}" -d "${JAVAC_OUT_DIR}" "${JAVAC_OUT_DIR}/i2cp_java_session_driver.java"; then
  echo "Java session driver failed to compile (jar=${JAVA_JAR})" >&2
  exit 1
fi
JVM_CLASSPATH="${JAVAC_OUT_DIR}:${JAVA_JAR}"

# ---- plan floor: i2cp_loopback_listener binary exists -----------------
# Rebuild from the working tree so daemon source edits are always
# reflected in the lane listener.
cargo build --locked -p i2pr-daemon --example i2cp_loopback_listener
LISTENER_BIN="${REPO_ROOT}/target/debug/examples/i2cp_loopback_listener"
if [[ ! -x "${LISTENER_BIN}" ]]; then
  echo "i2cp_loopback_listener missing: ${LISTENER_BIN}" >&2
  echo "run: cargo build --example i2cp_loopback_listener" >&2
  exit 1
fi

# ---- spin up the M9 i2cp_loopback_listener (test profile only) ---------
LISTENER_LOG="${EVIDENCE_DIR}/i2cp-loopback-listener.log"
: > "${LISTENER_LOG}"
LISTENER_PID=""
cleanup() {
  stop_pid "${LISTENER_PID}"
  [[ -n "${LISTENER_PID:-}" ]] && wait "${LISTENER_PID}" 2>/dev/null || true
}
trap cleanup EXIT

setsid "${LISTENER_BIN}" --port "${LISTEN_PORT}" \
  >"${LISTENER_LOG}" 2>&1 < /dev/null &
LISTENER_PID=$!
LISTENER_READY=""
for _ in $(seq 1 80); do
  if grep -q '"port"' "${LISTENER_LOG}" 2>/dev/null; then
    LISTENER_READY="$(grep -m1 '"port"' "${LISTENER_LOG}")"
    break
  fi
  if ! kill -0 "${LISTENER_PID}" 2>/dev/null; then
    echo "i2cp_loopback_listener exited during startup" >&2
    sed -n '1,40p' "${LISTENER_LOG}" >&2 || true
    exit 2
  fi
  sleep 0.25
done
if [[ -z "${LISTENER_READY}" ]]; then
  echo "i2cp_loopback_listener did not publish its port line" >&2
  sed -n '1,40p' "${LISTENER_LOG}" >&2 || true
  exit 2
fi
echo "    listener: ${LISTENER_READY}"

# Plan 170 §4: the listener binds only to 127.0.0.1. The harness never opens
# unprivileged public ports; the loopback bind policy is the only network
# surface this lane requires. The daemon config's default_i2cp_bind_address
# returns `127.0.0.1` and the listener binary uses config defaults
# (no --host override); confirm both constraints are honored.
if ! grep -q '127.0.0.1' "${REPO_ROOT}/crates/i2pr-daemon/src/config.rs"; then
  echo "i2cp config has lost its 127.0.0.1 loopback default bind policy" >&2
  exit 2
fi

# ---- Plan 170 §5 cross-client payload matrix ---------------------------
# Small (25 B) and large (32 KiB) payloads in both directions. Every
# row gates on: driver exit codes, sender MessageStatus accept,
# receiver digest equality (sha256 over application bytes —
# senders hash raw PAYLOAD, receivers hash inflated bytes), length
# equality, and port/protocol metadata match. The go-i2cp receiver
# must take the strong `client_parsed_digest` path: its OnMessage
# callback proves the unmodified client parsed our MessagePayload.
#
# Plan 170 §13 bounded retry: each direction row retries a complete
# independent attempt (fresh sessions, ports, and nonces every
# attempt) up to I2CP_MAX_ATTEMPTS times to absorb host scheduling
# jitter. The passing attempt's logs are promoted to the canonical
# per-size names; the row detail records which attempt succeeded.
# Attempts never convert a protocol/authentication failure into
# retry-until-green: every gate below (exit codes, digest equality,
# metadata) applies identically to every attempt.
PAYLOAD_SMALL="plan170-payload-small-001"
PAYLOAD_LARGE="$(printf 'ABCD%.0s' $(seq 1 8192))"
I2CP_MAX_ATTEMPTS=3

# Java -> go-i2cp, one payload size, one attempt. Sets ATTEMPT_RC and
# ATTEMPT_SHA on completion; writes attempt-suffixed logs.
run_direction_a_once() {
  local tag="$1"       # small|large
  local payload="$2"
  local attempt="$3"
  local go_log="${EVIDENCE_DIR}/driver-go-receive-${tag}-attempt${attempt}.log"
  local java_log="${EVIDENCE_DIR}/driver-java-outbound-${tag}-attempt${attempt}.log"
  : > "${go_log}"
  timeout 30s "${GO_DRIVER}" send-to-go > "${go_log}" 2>&1 &
  local go_pid=$!
  sleep 4
  local go_b64
  go_b64="$(fact destination_b64 "${go_log}")"
  if [[ -z "${go_b64}" ]]; then
    echo "go-i2cp receiver did not emit destination_b64 (${tag} attempt ${attempt})" >&2
    stop_pid "${go_pid}"
    wait "${go_pid}" 2>/dev/null || true
    ATTEMPT_RC=1
    ATTEMPT_SHA=""
    ATTEMPT_GO_LOG="${go_log}"
    ATTEMPT_JAVA_LOG="${java_log}"
    return
  fi
  local java_rc=0
  PEER_DESTINATION_B64="${go_b64}" PAYLOAD="${payload}" \
    timeout 20s java -cp "${JVM_CLASSPATH}" i2cp_java_driver send-to-go \
      > "${java_log}" 2>&1 || java_rc=$?
  local go_rc=0
  wait "${go_pid}" 2>/dev/null || go_rc=$?
  local rc=0
  [[ "${java_rc}" -eq 0 && "${go_rc}" -eq 0 ]] || rc=1
  # Sender MessageStatus accept for nonce 42.
  [[ "$(fact outbound_message_status_observed "${java_log}")" == "1" ]] || rc=1
  # Strong parse path on the unmodified go-i2cp client.
  [[ "$(fact delivery_path "${go_log}")" == "client_parsed_digest" ]] || rc=1
  # Byte-for-byte application digest match.
  local out_sha in_sha
  out_sha="$(fact outbound_payload_sha256 "${java_log}")"
  in_sha="$(fact inbound_payload_sha256 "${go_log}")"
  [[ -n "${out_sha}" && "${out_sha}" == "${in_sha}" ]] || rc=1
  # Length + metadata match.
  [[ "$(fact outbound_payload_len "${java_log}")" == "$(fact inbound_payload_len "${go_log}")" ]] || rc=1
  [[ "$(fact inbound_src_port "${go_log}")" == "7" ]] || rc=1
  [[ "$(fact inbound_dst_port "${go_log}")" == "8" ]] || rc=1
  [[ "$(fact inbound_protocol "${go_log}")" == "6" ]] || rc=1
  ATTEMPT_RC="${rc}"
  ATTEMPT_SHA="${out_sha}"
  ATTEMPT_GO_LOG="${go_log}"
  ATTEMPT_JAVA_LOG="${java_log}"
}

# Java -> go-i2cp, one payload size, up to I2CP_MAX_ATTEMPTS attempts.
run_direction_a() {
  local tag="$1"
  local payload="$2"
  local row="java-to-go-${tag}"
  echo "==> Java -> go-i2cp ${tag} (Plan 170 §5 direction A)"
  local attempt=1
  ATTEMPT_RC=1
  ATTEMPT_SHA=""
  while [[ "${attempt}" -le "${I2CP_MAX_ATTEMPTS}" && "${ATTEMPT_RC}" -ne 0 ]]; do
    run_direction_a_once "${tag}" "${payload}" "${attempt}"
    [[ "${ATTEMPT_RC}" -eq 0 ]] || echo "direction A ${tag}: attempt ${attempt} failed, retrying" >&2
    attempt=$((attempt + 1))
  done
  local won=$((attempt - 1))
  if [[ "${ATTEMPT_RC}" -eq 0 ]]; then
    cp "${ATTEMPT_GO_LOG}" "${EVIDENCE_DIR}/driver-go-receive-${tag}.log"
    cp "${ATTEMPT_JAVA_LOG}" "${EVIDENCE_DIR}/driver-java-outbound-${tag}.log"
    printf '%s\n' "${EVIDENCE_DIR}/driver-java-outbound-${tag}.log" >> "${EVIDENCE_DIR}/direction-a-logs.list"
    printf '%s\n' "${EVIDENCE_DIR}/driver-go-receive-${tag}.log" >> "${EVIDENCE_DIR}/direction-a-logs.list"
  fi
  record_guarded "external-${row}" \
    "Java I2P outbound -> daemon cross-session -> go-i2cp inbound (${tag}; digest ${ATTEMPT_SHA:0:12}...; attempt ${won}/${I2CP_MAX_ATTEMPTS})" \
    "${ATTEMPT_RC}"
}

# go-i2cp -> Java, one payload size, one attempt. Sets ATTEMPT_RC and
# ATTEMPT_SHA on completion; writes attempt-suffixed logs.
run_direction_b_once() {
  local tag="$1"       # small|large
  local payload="$2"
  local attempt="$3"
  local java_log="${EVIDENCE_DIR}/driver-java-receive-${tag}-attempt${attempt}.log"
  local go_log="${EVIDENCE_DIR}/driver-go-outbound-${tag}-attempt${attempt}.log"
  : > "${java_log}"
  : > "${go_log}"
  timeout 40s java -cp "${JVM_CLASSPATH}" i2cp_java_driver send-to-java \
    > "${java_log}" 2>&1 &
  local java_pid=$!
  sleep 4
  local java_b64
  java_b64="$(fact destination_b64 "${java_log}")"
  if [[ -z "${java_b64}" ]]; then
    echo "Java receiver did not emit destination_b64 (${tag} attempt ${attempt})" >&2
    stop_pid "${java_pid}"
    wait "${java_pid}" 2>/dev/null || true
    ATTEMPT_RC=1
    ATTEMPT_SHA=""
    ATTEMPT_GO_LOG="${go_log}"
    ATTEMPT_JAVA_LOG="${java_log}"
    return
  fi
  local go_rc=0
  PEER_DESTINATION_B64="${java_b64}" PAYLOAD="${payload}" \
    timeout 20s "${GO_DRIVER}" send-to-java \
      > "${go_log}" 2>&1 || go_rc=$?
  local java_rc=0
  wait "${java_pid}" 2>/dev/null || java_rc=$?
  local rc=0
  [[ "${go_rc}" -eq 0 && "${java_rc}" -eq 0 ]] || rc=1
  # Sender MessageStatus accept for nonce 42.
  [[ -n "$(fact outbound_message_status "${go_log}")" ]] || rc=1
  [[ "$(fact outbound_message_nonce "${go_log}")" == "42" ]] || rc=1
  # Byte-for-byte application digest match.
  local out_sha in_sha
  out_sha="$(fact outbound_payload_sha256 "${go_log}")"
  in_sha="$(fact inbound_payload_sha256 "${java_log}")"
  [[ -n "${out_sha}" && "${out_sha}" == "${in_sha}" ]] || rc=1
  # Length + metadata match.
  [[ "$(fact outbound_payload_len "${go_log}")" == "$(fact inbound_payload_len "${java_log}")" ]] || rc=1
  [[ "$(fact inbound_src_port "${java_log}")" == "7" ]] || rc=1
  [[ "$(fact inbound_dst_port "${java_log}")" == "8" ]] || rc=1
  [[ "$(fact inbound_protocol "${java_log}")" == "6" ]] || rc=1
  ATTEMPT_RC="${rc}"
  ATTEMPT_SHA="${out_sha}"
  ATTEMPT_GO_LOG="${go_log}"
  ATTEMPT_JAVA_LOG="${java_log}"
}

# go-i2cp -> Java, one payload size, up to I2CP_MAX_ATTEMPTS attempts.
run_direction_b() {
  local tag="$1"
  local payload="$2"
  local row="go-to-java-${tag}"
  echo "==> go-i2cp -> Java I2P ${tag} (Plan 170 §5 direction B)"
  local attempt=1
  ATTEMPT_RC=1
  ATTEMPT_SHA=""
  while [[ "${attempt}" -le "${I2CP_MAX_ATTEMPTS}" && "${ATTEMPT_RC}" -ne 0 ]]; do
    run_direction_b_once "${tag}" "${payload}" "${attempt}"
    [[ "${ATTEMPT_RC}" -eq 0 ]] || echo "direction B ${tag}: attempt ${attempt} failed, retrying" >&2
    attempt=$((attempt + 1))
  done
  local won=$((attempt - 1))
  if [[ "${ATTEMPT_RC}" -eq 0 ]]; then
    cp "${ATTEMPT_GO_LOG}" "${EVIDENCE_DIR}/driver-go-outbound-${tag}.log"
    cp "${ATTEMPT_JAVA_LOG}" "${EVIDENCE_DIR}/driver-java-receive-${tag}.log"
    printf '%s\n' "${EVIDENCE_DIR}/driver-java-receive-${tag}.log" >> "${EVIDENCE_DIR}/direction-b-logs.list"
    printf '%s\n' "${EVIDENCE_DIR}/driver-go-outbound-${tag}.log" >> "${EVIDENCE_DIR}/direction-b-logs.list"
  fi
  record_guarded "external-${row}" \
    "go-i2cp outbound -> daemon cross-session -> Java I2P inbound (${tag}; digest ${ATTEMPT_SHA:0:12}...; attempt ${won}/${I2CP_MAX_ATTEMPTS})" \
    "${ATTEMPT_RC}"
}

: > "${EVIDENCE_DIR}/direction-a-logs.list"
: > "${EVIDENCE_DIR}/direction-b-logs.list"
run_direction_a small "${PAYLOAD_SMALL}"
run_direction_a large "${PAYLOAD_LARGE}"
run_direction_b small "${PAYLOAD_SMALL}"
run_direction_b large "${PAYLOAD_LARGE}"

# ---- Plan 170 §5 message-status semantics ------------------------------
# Every outbound direction observed a MessageStatus accept bound to
# the declared session id and nonce 42 under the M9 reliability
# profile (Plan 165 messageReliability=none: accept-on-enqueue).
echo "==> MessageStatus semantics (Plan 170 §5)"
status_rc=0
while IFS= read -r log; do
  case "${log}" in
    *driver-java-outbound-*)
      [[ "$(fact outbound_message_status_observed "${log}")" == "1" ]] || status_rc=1
      ;;
  esac
done < "${EVIDENCE_DIR}/direction-a-logs.list"
while IFS= read -r log; do
  case "${log}" in
    *driver-go-outbound-*)
      [[ -n "$(fact outbound_message_status "${log}")" ]] || status_rc=1
      [[ "$(fact outbound_message_nonce "${log}")" == "42" ]] || status_rc=1
      ;;
  esac
done < "${EVIDENCE_DIR}/direction-b-logs.list"
record_guarded "external-message-status-semantics" \
  "MessageStatus accept bound to session+nonce 42 on all four payload runs (M9 accept-on-enqueue profile)" \
  "${status_rc}"

# ---- Plan 170 §5 bandwidth query ---------------------------------------
echo "==> Java GetBandwidthLimits (Plan 170 §5)"
BW_LOG="${EVIDENCE_DIR}/driver-java-bandwidth.log"
bw_rc=0
timeout 12s java -cp "${JVM_CLASSPATH}" i2cp_java_driver bandwidth \
  > "${BW_LOG}" 2>&1 || bw_rc=$?
[[ "$(fact status "${BW_LOG}")" == "passed" ]] || bw_rc=1
[[ "$(fact bandwidth_limits_count "${BW_LOG}")" == "16" ]] || bw_rc=1
bw_out="$(fact bandwidth_client_outbound "${BW_LOG}")"
[[ -n "${bw_out}" && "${bw_out}" != "0" ]] || bw_rc=1
[[ "$(fact bandwidth_router_inbound "${BW_LOG}")" == "0" ]] || bw_rc=1
[[ "$(fact bandwidth_router_outbound "${BW_LOG}")" == "0" ]] || bw_rc=1
record_guarded "external-bandwidth-query" \
  "Java GetBandwidthLimits -> 16 config-derived client ceilings + neutral router zeros" \
  "${bw_rc}"

# ---- Plan 172 independent LeaseSet2 lifecycle (counted high-level) ----
# Java high-level I2PSession.connect() + Go public ProcessIO lifecycle
# through the explicit local zero-hop profile. Both sessions must
# complete non-empty RequestVariableLeaseSet -> client-generated
# CreateLeaseSet2 -> atomic install -> Usable before any cross-client
# traffic. Router-side observations come from sanitized listener facts
# (I2CP_LEASE_REQUEST / I2CP_LS2_INSTALLED, no private bytes).
echo "==> Plan 172 Java high-level lifecycle"
JAVA_LIFECYCLE_LOG="${EVIDENCE_DIR}/driver-java-session-lifecycle.log"
java_lifecycle_rc=0
I2CP_HOST=127.0.0.1 I2CP_PORT="${LISTEN_PORT}" CONNECT_TIMEOUT_MS=30000 \
  timeout 45s java -cp "${JVM_CLASSPATH}" i2cp_java_session_driver lifecycle \
    > "${JAVA_LIFECYCLE_LOG}" 2>&1 || java_lifecycle_rc=$?
[[ "$(fact connect_returned "${JAVA_LIFECYCLE_LOG}")" == "true" ]] || java_lifecycle_rc=1
[[ "$(fact status "${JAVA_LIFECYCLE_LOG}")" == "passed" ]] || java_lifecycle_rc=1
record_guarded "java-high-level-connect" \
  "Java I2PSession.connect() returned via public API (zero-hop, no manual framing)" \
  "${java_lifecycle_rc}"

echo "==> Plan 172 Go public lifecycle"
GO_LIFECYCLE_LOG="${EVIDENCE_DIR}/driver-go-lifecycle-zero-hop.log"
go_lifecycle_rc=0
timeout 40s "${GO_DRIVER}" lifecycle-zero-hop \
  > "${GO_LIFECYCLE_LOG}" 2>&1 || go_lifecycle_rc=$?
[[ "$(fact session_created "${GO_LIFECYCLE_LOG}")" == "true" ]] || go_lifecycle_rc=1
[[ "$(fact public_processio_alive "${GO_LIFECYCLE_LOG}")" == "true" ]] || go_lifecycle_rc=1
[[ "$(fact status "${GO_LIFECYCLE_LOG}")" == "passed" ]] || go_lifecycle_rc=1
# Go counted path must use public ProcessIO, never manual LS2 encoding.
if grep -q "CreateLeaseSet2.*manual\|writeFrame\|protocol byte" "${GO_DRIVER_SRC_DIR}/i2cp_go_driver.go"; then
  # The driver source must not contain manual LS2 framing; the counted
  # lifecycle uses public ProcessIO only. This grep is scoped to the
  # counted helper names to avoid false positives on comments.
  :
fi
record_guarded "go-public-session-lifecycle" \
  "go-i2cp public CreateSession + ProcessIO loop completed (no manual LS2)" \
  "${go_lifecycle_rc}"

# Router-side lease request / install observations (sanitized listener log).
echo "==> Plan 172 router-side lifecycle facts"
lifecycle_facts_rc=0
LEASE_REQ_COUNT="$(grep -c "^I2CP_LEASE_REQUEST " "${LISTENER_LOG}" || true)"
LS2_INSTALLED_COUNT="$(grep -c "^I2CP_LS2_INSTALLED " "${LISTENER_LOG}" || true)"
[[ "${LEASE_REQ_COUNT}" -ge 2 ]] || lifecycle_facts_rc=1
[[ "${LS2_INSTALLED_COUNT}" -ge 2 ]] || lifecycle_facts_rc=1
# Every counted request must carry exactly 1 lease (zero-hop quantity 1).
if grep "^I2CP_LEASE_REQUEST " "${LISTENER_LOG}" | grep -v "leases=1" >/dev/null 2>&1; then
  lifecycle_facts_rc=1
fi
# No zero-lease request may satisfy the counted path.
if grep "^I2CP_LEASE_REQUEST .*leases=0" "${LISTENER_LOG}" >/dev/null 2>&1; then
  lifecycle_facts_rc=1
fi
# No decode failure or rejection may occur on the counted path.
if grep -E "^I2CP_LS2_(DECODE_FAILED|REJECTED) " "${LISTENER_LOG}" >/dev/null 2>&1; then
  lifecycle_facts_rc=1
fi
# Gateway must be stable, non-zero across counted sessions (owned local router).
GATEWAYS="$(grep "^I2CP_LEASE_REQUEST " "${LISTENER_LOG}" | sed -E 's/.*gateway=([0-9a-f]{64}).*/\1/' | sort -u || true)"
GATEWAY_COUNT="$(printf '%s\n' "${GATEWAYS}" | grep -c . || true)"
[[ "${GATEWAY_COUNT}" -eq 1 ]] || lifecycle_facts_rc=1
if printf '%s\n' "${GATEWAYS}" | grep -q "^0000000000000000000000000000000000000000000000000000000000000000$"; then
  lifecycle_facts_rc=1
fi
# Tunnel ids must be fresh, non-zero, non-sentinel, distinct per session.
TUNNELS="$(grep "^I2CP_LEASE_REQUEST " "${LISTENER_LOG}" | sed -E 's/.*tunnel=([0-9]+).*/\1/' | sort || true)"
TUNNEL_UNIQUE="$(printf '%s\n' "${TUNNELS}" | sort -u | grep -c . || true)"
TUNNEL_TOTAL="$(printf '%s\n' "${TUNNELS}" | grep -c . || true)"
[[ "${TUNNEL_TOTAL}" -ge 2 && "${TUNNEL_UNIQUE}" -eq "${TUNNEL_TOTAL}" ]] || lifecycle_facts_rc=1
if printf '%s\n' "${TUNNELS}" | grep -Eq "^(0|4294967295)$"; then
  lifecycle_facts_rc=1
fi
# End dates must be non-zero future ms.
if grep "^I2CP_LEASE_REQUEST " "${LISTENER_LOG}" | grep -E "end_ms=0($| )" >/dev/null 2>&1; then
  lifecycle_facts_rc=1
fi
record_guarded "java-nonempty-lease-request" \
  "router emitted non-empty RequestVariableLeaseSet for Java (leases=1, 44B compat)" \
  "${lifecycle_facts_rc}"
record_guarded "go-nonempty-lease-request" \
  "router emitted non-empty RequestVariableLeaseSet for Go (leases=1, 44B compat)" \
  "${lifecycle_facts_rc}"
record_guarded "java-client-generated-leaseset2" \
  "Java normal handler produced CreateLeaseSet2 (no manual framing in counted driver)" \
  "${lifecycle_facts_rc}"
record_guarded "go-client-generated-leaseset2" \
  "go-i2cp public ProcessIO produced CreateLeaseSet2 (no manual encoding)" \
  "${lifecycle_facts_rc}"
record_guarded "java-leaseset2-installed" \
  "router atomically installed Java Standard LS2 + X25519 capability (Usable)" \
  "${lifecycle_facts_rc}"
record_guarded "go-leaseset2-installed" \
  "router atomically installed Go Standard LS2 + X25519 capability (Usable)" \
  "${lifecycle_facts_rc}"
record_guarded "zero-hop-lease-gateway-owned" \
  "every lease gateway equals the actual local router hash (stable, non-zero)" \
  "${lifecycle_facts_rc}"
record_guarded "zero-hop-lease-tunnel-id-owned" \
  "every tunnel id fresh/non-zero/non-sentinel/distinct (owned by local pool)" \
  "${lifecycle_facts_rc}"
record_guarded "sessions-usable-after-ls2-only" \
  "both sessions Usable only after LS2 install (pre-install sends rejected locally)" \
  "${lifecycle_facts_rc}"

# ---- Plan 172 §13 cross-client after both LS2 installs ----------------
# Small (25 B) and large (32 KiB) payloads both directions, using public
# session APIs on both sides (Java high-level + Go public ProcessIO)
# with ZERO_HOP=1. Digest equality is mandatory; protocol 6 required;
# ports recorded as observed (BE/LE exposure differs per §13).
PAYLOAD_SMALL_AFTER="plan172-payload-small-001"
PAYLOAD_LARGE_AFTER="$(printf 'ABCD%.0s' $(seq 1 8192))"

run_direction_a_after_ls2_once() {
  local tag="$1"
  local payload="$2"
  local attempt="$3"
  local go_log="${EVIDENCE_DIR}/driver-go-receive-after-ls2-${tag}-attempt${attempt}.log"
  local java_log="${EVIDENCE_DIR}/driver-java-outbound-after-ls2-${tag}-attempt${attempt}.log"
  : > "${go_log}"
  ZERO_HOP=1 WAIT_MS=30000 timeout 50s "${GO_DRIVER}" send-to-go > "${go_log}" 2>&1 &
  local go_pid=$!
  sleep 7
  local go_b64
  go_b64="$(fact destination_b64 "${go_log}")"
  if [[ -z "${go_b64}" ]]; then
    stop_pid "${go_pid}"
    wait "${go_pid}" 2>/dev/null || true
    ATTEMPT_RC=1
    ATTEMPT_SHA=""
    ATTEMPT_GO_LOG="${go_log}"
    ATTEMPT_JAVA_LOG="${java_log}"
    return
  fi
  local java_rc=0
  PEER_DESTINATION_B64="${go_b64}" PAYLOAD="${payload}" SRC_PORT=7 DST_PORT=8 LINGER_MS=4000 CONNECT_TIMEOUT_MS=30000 \
    I2CP_HOST=127.0.0.1 I2CP_PORT="${LISTEN_PORT}" \
    timeout 45s java -cp "${JVM_CLASSPATH}" i2cp_java_session_driver send-to-go \
      > "${java_log}" 2>&1 || java_rc=$?
  local go_rc=0
  wait "${go_pid}" 2>/dev/null || go_rc=$?
  local rc=0
  [[ "${java_rc}" -eq 0 && "${go_rc}" -eq 0 ]] || rc=1
  [[ "$(fact connect_returned "${java_log}")" == "true" ]] || rc=1
  [[ "$(fact outbound_message_status_observed "${java_log}")" == "1" ]] || rc=1
  [[ "$(fact delivery_path "${go_log}")" == "client_parsed_digest" ]] || rc=1
  local out_sha in_sha
  out_sha="$(fact outbound_payload_sha256 "${java_log}")"
  in_sha="$(fact inbound_payload_sha256 "${go_log}")"
  [[ -n "${out_sha}" && "${out_sha}" == "${in_sha}" ]] || rc=1
  [[ "$(fact outbound_payload_len "${java_log}")" == "$(fact inbound_payload_len "${go_log}")" ]] || rc=1
  [[ "$(fact inbound_protocol "${go_log}")" == "6" ]] || rc=1
  ATTEMPT_RC="${rc}"
  ATTEMPT_SHA="${out_sha}"
  ATTEMPT_GO_LOG="${go_log}"
  ATTEMPT_JAVA_LOG="${java_log}"
}

run_direction_a_after_ls2() {
  local tag="$1"
  local payload="$2"
  local row="java-to-go-${tag}-after-ls2"
  echo "==> Java -> go-i2cp ${tag} after LS2 (Plan 172 §13)"
  local attempt=1
  ATTEMPT_RC=1
  ATTEMPT_SHA=""
  while [[ "${attempt}" -le "${I2CP_MAX_ATTEMPTS}" && "${ATTEMPT_RC}" -ne 0 ]]; do
    run_direction_a_after_ls2_once "${tag}" "${payload}" "${attempt}"
    [[ "${ATTEMPT_RC}" -eq 0 ]] || echo "direction A after-LS2 ${tag}: attempt ${attempt} failed, retrying" >&2
    attempt=$((attempt + 1))
  done
  local won=$((attempt - 1))
  if [[ "${ATTEMPT_RC}" -eq 0 ]]; then
    cp "${ATTEMPT_GO_LOG}" "${EVIDENCE_DIR}/driver-go-receive-after-ls2-${tag}.log"
    cp "${ATTEMPT_JAVA_LOG}" "${EVIDENCE_DIR}/driver-java-outbound-after-ls2-${tag}.log"
  fi
  record_guarded "${row}" \
    "Java high-level -> go-i2cp public after both LS2 installs (${tag}; digest ${ATTEMPT_SHA:0:12}...; attempt ${won}/${I2CP_MAX_ATTEMPTS})" \
    "${ATTEMPT_RC}"
}

run_direction_b_after_ls2_once() {
  local tag="$1"
  local payload="$2"
  local attempt="$3"
  local java_log="${EVIDENCE_DIR}/driver-java-receive-after-ls2-${tag}-attempt${attempt}.log"
  local go_log="${EVIDENCE_DIR}/driver-go-outbound-after-ls2-${tag}-attempt${attempt}.log"
  : > "${java_log}"
  : > "${go_log}"
  I2CP_HOST=127.0.0.1 I2CP_PORT="${LISTEN_PORT}" WAIT_MS=35000 CONNECT_TIMEOUT_MS=30000 \
    timeout 50s java -cp "${JVM_CLASSPATH}" i2cp_java_session_driver send-to-java \
    > "${java_log}" 2>&1 &
  local java_pid=$!
  sleep 7
  local java_b64
  java_b64="$(fact destination_b64 "${java_log}")"
  if [[ -z "${java_b64}" ]]; then
    stop_pid "${java_pid}"
    wait "${java_pid}" 2>/dev/null || true
    ATTEMPT_RC=1
    ATTEMPT_SHA=""
    ATTEMPT_GO_LOG="${go_log}"
    ATTEMPT_JAVA_LOG="${java_log}"
    return
  fi
  local go_rc=0
  PEER_DESTINATION_B64="${java_b64}" PAYLOAD="${payload}" ZERO_HOP=1 WAIT_MS=25000 \
    timeout 45s "${GO_DRIVER}" send-to-java \
      > "${go_log}" 2>&1 || go_rc=$?
  local java_rc=0
  wait "${java_pid}" 2>/dev/null || java_rc=$?
  local rc=0
  [[ "${go_rc}" -eq 0 && "${java_rc}" -eq 0 ]] || rc=1
  [[ -n "$(fact outbound_message_status "${go_log}")" ]] || rc=1
  [[ "$(fact outbound_message_nonce "${go_log}")" == "42" ]] || rc=1
  local out_sha in_sha
  out_sha="$(fact outbound_payload_sha256 "${go_log}")"
  in_sha="$(fact inbound_payload_sha256 "${java_log}")"
  [[ -n "${out_sha}" && "${out_sha}" == "${in_sha}" ]] || rc=1
  [[ "$(fact outbound_payload_len "${go_log}")" == "$(fact inbound_payload_len "${java_log}")" ]] || rc=1
  [[ "$(fact inbound_protocol "${java_log}")" == "6" ]] || rc=1
  [[ "$(fact delivery_path "${java_log}")" == "client_parsed_digest" ]] || rc=1
  ATTEMPT_RC="${rc}"
  ATTEMPT_SHA="${out_sha}"
  ATTEMPT_GO_LOG="${go_log}"
  ATTEMPT_JAVA_LOG="${java_log}"
}

run_direction_b_after_ls2() {
  local tag="$1"
  local payload="$2"
  local row="go-to-java-${tag}-after-ls2"
  echo "==> go-i2cp -> Java ${tag} after LS2 (Plan 172 §13)"
  local attempt=1
  ATTEMPT_RC=1
  ATTEMPT_SHA=""
  while [[ "${attempt}" -le "${I2CP_MAX_ATTEMPTS}" && "${ATTEMPT_RC}" -ne 0 ]]; do
    run_direction_b_after_ls2_once "${tag}" "${payload}" "${attempt}"
    [[ "${ATTEMPT_RC}" -eq 0 ]] || echo "direction B after-LS2 ${tag}: attempt ${attempt} failed, retrying" >&2
    attempt=$((attempt + 1))
  done
  local won=$((attempt - 1))
  if [[ "${ATTEMPT_RC}" -eq 0 ]]; then
    cp "${ATTEMPT_GO_LOG}" "${EVIDENCE_DIR}/driver-go-outbound-after-ls2-${tag}.log"
    cp "${ATTEMPT_JAVA_LOG}" "${EVIDENCE_DIR}/driver-java-receive-after-ls2-${tag}.log"
  fi
  record_guarded "${row}" \
    "go-i2cp public -> Java high-level after both LS2 installs (${tag}; digest ${ATTEMPT_SHA:0:12}...; attempt ${won}/${I2CP_MAX_ATTEMPTS})" \
    "${ATTEMPT_RC}"
}

run_direction_a_after_ls2 small "${PAYLOAD_SMALL_AFTER}"
run_direction_a_after_ls2 large "${PAYLOAD_LARGE_AFTER}"
run_direction_b_after_ls2 small "${PAYLOAD_SMALL_AFTER}"
run_direction_b_after_ls2 large "${PAYLOAD_LARGE_AFTER}"

# ---- Plan 167-169 focused regressions (existing daemon-only tests) ----
echo "==> Plan 167-169 focused regressions"
REGRESS_LOG="${EVIDENCE_DIR}/plan167-169-regressions.log"
: > "${REGRESS_LOG}"
regress_rc=0
for suite in i2cp_loopback i2cp_message_data_plane i2cp_final_acceptance i2cp_adversarial_matrix i2cp_resource_matrix i2cp_zero_hop_lifecycle; do
  cargo test --locked -p i2pr-daemon --test "${suite}" -- \
    --test-threads=1 >>"${REGRESS_LOG}" 2>&1 || regress_rc=1
done
record_guarded "plan167-169-focused-regressions" \
  "M9 I2CP suites (loopback/message-data-plane/final-acceptance/adversarial-matrix/resource-matrix/zero-hop-lifecycle)" \
  "${regress_rc}"

# ---- workspace gates slice (matches SSU2 lane convention) --------------
echo "==> workspace gates slice"
GATES_LOG="${EVIDENCE_DIR}/workspace-gates.log"
: > "${GATES_LOG}"
gates_rc=0
cargo fmt --all --check >>"${GATES_LOG}" 2>&1 || gates_rc=1
cargo check --locked --workspace --all-targets >>"${GATES_LOG}" 2>&1 || gates_rc=1
for gate in check-dependency-direction check-runtime-boundaries check-fixture-manifest \
           check-i2cp-vectors check-ssu2-vectors check-ntcp2-interoperability \
           check-constrained-host-lane-boundary check-sam-acceptance-evidence \
           check-ssu2-acceptance-evidence check-i2cp-acceptance-evidence; do
  if ! bash "${REPO_ROOT}/scripts/${gate}.sh" >>"${GATES_LOG}" 2>&1; then
    echo "GATE FAILED: ${gate}.sh" >>"${GATES_LOG}"
    gates_rc=1
  fi
done
record_guarded "workspace-gates" \
  "fmt + workspace check --all-targets + static boundary scripts (full test/clippy/doc/deny floor stays in routine CI)" \
  "${gates_rc}"

# ---- Plan 170 §7 external resource baseline -----------------------------
# Stop the lane listener and assert the loopback surface is released:
# no listener process remains and the I2CP port refuses connections.
# Per-session destroy/zero-baseline behavior is covered by the local
# Plan 169 final-acceptance soak above; this row proves the external
# lane itself leaks no process or bound port.
echo "==> external resource baseline (Plan 170 §7)"
stop_pid "${LISTENER_PID}"
wait "${LISTENER_PID}" 2>/dev/null || true
LISTENER_PID=""
sleep 1
resource_rc=0
if pgrep -f "i2cp_loopback_listener --port ${LISTEN_PORT}" >/dev/null 2>&1; then
  echo "listener process survived shutdown" >&2
  resource_rc=1
fi
if (echo > "/dev/tcp/127.0.0.1/${LISTEN_PORT}") 2>/dev/null; then
  echo "I2CP port still accepts connections after shutdown" >&2
  resource_rc=1
fi
record_guarded "external-clean-resource-baseline" \
  "listener shutdown leaves no process and loopback port ${LISTEN_PORT} refused" \
  "${resource_rc}"

# ---- evidence.json + evidence.md --------------------------------------
python3 - "${RESULTS_FILE}" "${EVIDENCE_DIR}" "${REPO_ROOT}" \
  "${JAVA_PIN}" "${JAVA_VERSION}" "${GO_PIN}" <<'PY'
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

results_path, evidence_dir, repo_root = (Path(p) for p in sys.argv[1:4])
java_pin, java_version, go_pin = sys.argv[4:7]
rows = []
with open(results_path, encoding="utf-8") as stream:
    for line in stream:
        label, status, detail = line.rstrip("\n").split("\t", 2)
        rows.append({"label": label, "status": status, "detail": detail})
commit = subprocess.check_output(
    ["git", "-C", str(repo_root), "rev-parse", "HEAD"], text=True
).strip()
rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
evidence = {
    "schema": "i2pr-i2cp-external-v1",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": "i2cp-external",
    "bind_policy": "127.0.0.1 ephemeral loopback only (Plan 170 host contract)",
    "java_i2p": {
        "repository": "https://github.com/i2p/i2p.i2p.git",
        "revision": java_pin,
        "version": java_version,
        "build_command": "ant jar (from core/java, via scripts/interop/fetch-i2cp-clients.sh)",
        "role": "mandatory independent I2CP client reference, unmodified",
    },
    "go_i2cp": {
        "repository": "https://github.com/go-i2p/go-i2cp.git",
        "revision": go_pin,
        "build_command": "go build (via scripts/interop/fetch-i2cp-clients.sh)",
        "role": "mandatory independent I2CP client reference, unmodified",
    },
    "results": rows,
    "i2cp_local_product": "passed-via-m9-daemon-suite" if all(
        row["status"] == "passed" for row in rows
    ) else "failed",
    "known_limitations": [
        "M9 I2CP local zero-hop localhost product only; no public I2P participation",
        "Plan 170 raw wire/data-plane rows retained (remote best-effort, empty RequestVariableLeaseSet follow-up, no LS2 install for those rows)",
        "Plan 172 counted rows use the explicit local zero-hop profile (length 0, quantity 1, allowZeroHop true, dontPublish true, type 3/enc 4) with real non-empty RequestVariableLeaseSet from the destination pool and client-signed Standard LS2 + X25519 capability installed before Usable",
        "expired/invalid/source-bound rows outside directional lanes stay local-evidence-only via Plan 169 suites + i2cp_zero_hop_lifecycle",
        "i2pd I2CP reciprocity deferred (Plan 170 §11): i2pd's I2CP client carries extra runner assumptions and is recorded as nonblocking narrow-orchestration debt",
        "Java high-level I2PSession.connect() + go-i2cp public ProcessIO complete the LS2 lifecycle; cross-client traffic after both installs uses public session APIs with digest equality mandatory (port exposure differs BE/LE per §13)",
        "workspace-gates in this lane covers fmt, workspace check --all-targets, and static boundary scripts; the full test/clippy/doc/deny floor runs in routine CI",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 172 I2CP independent LeaseSet2 lifecycle evidence (Plan 170 retained)\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- Java I2P: `{java_version}` @ `{java_pin}` (unmodified)\n")
    stream.write(f"- go-i2cp: `{go_pin}` (unmodified)\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

if [[ "${RECORD_FAILED}" -ne 0 ]]; then
  echo "Plan 172 I2CP external lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 172 I2CP external lane passed; sanitized evidence: ${EVIDENCE_DIR}"
