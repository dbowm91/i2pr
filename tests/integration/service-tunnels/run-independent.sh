#!/usr/bin/env bash
# Plan 181 — M10 independent-application-client matrix plus the
# controlled remote independent-I2P qualification attempt.
#
# Provenance: local rows execute the focused Plan 174–180/182 Rust
# suites plus unmodified external application clients (curl, nc, a
# stdlib generic driver, exact-pinned jaraco/irc) against the real
# i2pr M10 service-tunnel manager booted from
# `service_tunnels_loopback_listener`. Remote rows provision one
# ephemeral exact-pinned i2pd 2.61.0 process on loopback, obtain an
# independently generated destination via SAM DEST GENERATE, and
# run the single fail-closed qualification driver
# (crates/i2pr-daemon/tests/service_tunnels_remote_qualification.rs)
# through its explicit `--ignored --exact` selection.
#
# No required row is recorded `passed` except by the exit status of
# its associated command (plus the row's own evidence keys where
# applicable); remote rows are recorded `blocked` (never `passed`)
# with command/log provenance per the §6.3 stop condition. See
# scripts/check-service-tunnel-acceptance-evidence.sh.
#
# The lane is unprivileged and loopback-only (no root, no Docker, no
# namespaces, no public I2P participation). Required failures make
# this script fail. Sanitized evidence defaults below
# target/interop; set I2PR_M10_EVIDENCE_DIR to retain it elsewhere.
# Private keys and raw application payloads are never copied to
# evidence (digests/lengths/counters only).

set -euo pipefail

LANE="full"
if [[ "${1:-}" == "--local-only" ]]; then
  LANE="local"
elif [[ -n "${1:-}" && "${1:-}" != "--full" ]]; then
  echo "usage: $0 [--full|--local-only]" >&2
  exit 64
fi

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
LANE_DIR="${REPO_ROOT}/tests/integration/service-tunnels"
FIXTURES_DIR="${LANE_DIR}/fixtures"
CLIENTS_DIR="${LANE_DIR}/clients"
EVIDENCE_DIR="${I2PR_M10_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/service-tunnels-evidence}"
JARACO_PIN="90e10e690da2c7bf60de21be4e36d24c9ffd7474"
JARACO_CACHE="${REPO_ROOT}/target/interop/cache/service-tunnels/jaraco_irc/${JARACO_PIN}"
JARACO_SRC_FILE="${JARACO_CACHE}/source-path.txt"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
I2PD_CACHE="${REPO_ROOT}/target/interop/cache/ssu2/i2pd/${I2PD_PIN}"
I2PD_BIN="${I2PR_I2PD_BIN:-${I2PD_CACHE}/bin/i2pd}"

mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-m10-plan181.XXXXXX)"
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
  [[ -z "${SCRATCH:-}" || ! -d "${SCRATCH}" ]] || rm -rf "${SCRATCH}"
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

# Plan 181 §8: the only sanctioned path from an executed command to
# a required `passed` row. The caller captures the command's exit
# code in `rc` first; a zero code records passed, anything else
# records failed. Direct literal `record "<required-label>"
# passed` lines are rejected by
# scripts/check-service-tunnel-acceptance-evidence.sh.
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

# Plan 181 §6.3: remote rows that cannot pass for lack of
# mixed-router destination/Streaming interop are recorded
# `blocked` with command/log provenance — never `passed`, never
# silently skipped. Any `blocked` row fails this lane.
record_blocked() {
  local label="$1"
  local detail="$2"
  record "${label}" blocked "${detail}"
}

# Extract a KEY=value fact (single line) from a driver log.
# Never fails: absence is reported as empty so `set -e`/`pipefail`
# cannot trip on an early poll (the caller decides pass/fail).
fact() {
  local key="$1"
  local file="$2"
  if [[ -f "${file}" ]]; then
    grep -E "^${key}=" "${file}" 2>/dev/null | head -n1 | sed "s/^${key}=//" || true
  fi
  return 0
}

# ---- Plan 181 §3 retained source floor ----------------------------------
echo "==> verifying Plans 174-180 prerequisite status"
prereq_rc=0
for plan in 174 175 176 177 178 179 180; do
  if ! grep -Eq "plan_${plan} = passed" "${REPO_ROOT}/plans/180-status.md" 2>/dev/null; then
    echo "prerequisite Plan ${plan} has no passed record in plans/180-status.md" >&2
    prereq_rc=1
  fi
done
if ! grep -Eq "plan_182 = passed" "${REPO_ROOT}/plans/182-status.md" 2>/dev/null; then
  echo "prerequisite Plan 182 has no passed record in plans/182-status.md" >&2
  prereq_rc=1
fi
record_guarded "m10-prerequisite-plans" \
  "Plans 174-180 + 182 carry explicit passed status records" \
  "${prereq_rc}"

# ---- tool + pin verification (fail closed before any network use) -------
echo "==> verifying tools and exact external pins"
tools_rc=0
for command in curl nc python3 git cargo timeout setsid; do
  command -v "${command}" >/dev/null 2>&1 || {
    echo "required command missing: ${command}" >&2
    tools_rc=1
  }
done
CURL_VERSION="$(curl --version 2>/dev/null | head -n1 || true)"
NC_VERSION="$(nc -h 2>&1 | head -n1 || true)"
PYTHON_VERSION="$(python3 --version 2>&1 || true)"
[[ -n "${CURL_VERSION}" ]] || tools_rc=1
if [[ ! -f "${JARACO_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${JARACO_CACHE}/source-revision.txt")" != "${JARACO_PIN}" ]]; then
  echo "jaraco/irc cache has no verified Plan 181 source revision" >&2
  echo "run scripts/interop/fetch-service-tunnel-clients.sh first" >&2
  tools_rc=1
fi
JARACO_SRC=""
if [[ -f "${JARACO_SRC_FILE}" ]]; then
  JARACO_SRC="$(<"${JARACO_SRC_FILE}")"
fi
if [[ -z "${JARACO_SRC}" || ! -d "${JARACO_SRC}/.git" ]] ||
   [[ "$(git -C "${JARACO_SRC}" rev-parse HEAD)" != "${JARACO_PIN}" ]]; then
  echo "jaraco/irc source checkout is not at the exact pin ${JARACO_PIN}" >&2
  tools_rc=1
fi
if [[ -n "$(git -C "${JARACO_SRC}" status --porcelain --untracked-files=no 2>/dev/null)" ]]; then
  echo "jaraco/irc checkout has tracked modifications (patching forbidden)" >&2
  tools_rc=1
fi
{
  echo "curl: ${CURL_VERSION}"
  echo "nc: ${NC_VERSION}"
  echo "python: ${PYTHON_VERSION}"
  echo "jaraco-pin: ${JARACO_PIN}"
} > "${EVIDENCE_DIR}/tool-versions.txt"
record_guarded "m10-tool-pin-verification" \
  "curl/nc/python3 present; jaraco/irc exact pin ${JARACO_PIN} verified clean" \
  "${tools_rc}"

# ---- static boundary checker --------------------------------------------
echo "==> M10 static boundary checks"
boundary_rc=0
bash "${REPO_ROOT}/scripts/check-service-tunnel-boundaries.sh" \
  >"${EVIDENCE_DIR}/boundary-checks.log" 2>&1 || boundary_rc=$?
record_guarded "m10-foundation-boundary-checks" \
  "runtime-neutral + loopback + single-pump static checker" \
  "${boundary_rc}"

# ---- Plan 174-180/182 focused Rust suites --------------------------------
echo "==> local M10 focused suites"
SUITES_LOG="${EVIDENCE_DIR}/local-suites.log"
: > "${SUITES_LOG}"
suites_rc=0
cargo test --locked -p i2pr-daemon --test service_tunnels_final_acceptance -- \
  --test-threads=1 >>"${SUITES_LOG}" 2>&1 || suites_rc=$?
cargo test --locked -p i2pr-daemon --test service_tunnels_adversarial_matrix -- \
  --test-threads=1 >>"${SUITES_LOG}" 2>&1 || suites_rc=$?
record_guarded "m10-local-final-product" \
  "Plan 180 reconcile/adversarial composition suites (cargo test service_tunnels_final_acceptance + adversarial_matrix)" \
  "${suites_rc}"
# Each suite row derives from the suite exit status plus its own
# captured `test <name> ... ok` line: the suites must be green AND
# the row's test must have run to ok. A renamed/deleted test fails
# its row instead of passing silently.
suite_row() {
  local label="$1"
  local test_name="$2"
  local detail="$3"
  local rc=1
  if [[ "${suites_rc}" -eq 0 ]] &&
     grep -q "^test ${test_name} ... ok$" "${SUITES_LOG}"; then
    rc=0
  fi
  record_guarded "${label}" "${detail}" "${rc}"
}
suite_row "reconcile-rollback-local" "staged_destination_failure_rolls_back_resources" \
  "staged failure rolls back to the committed generation"
suite_row "cross-service-resource-bounds" "cross_service_resource_baselines_are_bounded" \
  "unified cross-service resource accounting returns to baseline"

ROUNDTRIP_LOG="${EVIDENCE_DIR}/local-roundtrip.log"
: > "${ROUNDTRIP_LOG}"
roundtrip_rc=0
cargo test --locked -p i2pr-daemon --test service_tunnels_local_roundtrip -- \
  --test-threads=1 >>"${ROUNDTRIP_LOG}" 2>&1 || roundtrip_rc=$?
record_guarded "m10-local-roundtrip-suite" \
  "Plan 182 generic/HTTP/SOCKS/IRC byte round-trip incl. reverse + half-close (cargo test service_tunnels_local_roundtrip)" \
  "${roundtrip_rc}"
WIRESURFACE_LOG="${EVIDENCE_DIR}/wire-surface.log"
: > "${WIRESURFACE_LOG}"
wiresurface_rc=0
cargo test --locked -p i2pr-daemon --test service_tunnels_independent_application_clients -- \
  --test-threads=1 >>"${WIRESURFACE_LOG}" 2>&1 || wiresurface_rc=$?
record_guarded "m10-wire-surface-suite" \
  "application-facing wire surface incl. restart-stable server identity (cargo test service_tunnels_independent_application_clients)" \
  "${wiresurface_rc}"

# ---- build the harness listener ------------------------------------------
echo "==> building i2pr M10 listener example"
cargo build --locked --example service_tunnels_loopback_listener -p i2pr-daemon --quiet
LISTENER="${REPO_ROOT}/target/debug/examples/service_tunnels_loopback_listener"

# ---- fixtures (owned by the harness, on loopback) -------------------------
echo "==> starting loopback fixtures"
python3 "${FIXTURES_DIR}/http_fixture.py" --port 0 \
  --facts "${EVIDENCE_DIR}/http-fixture-facts.jsonl" \
  --max-connections 24 >"${SCRATCH}/http-fixture-port.log" 2>&1 &
CHILD_PIDS+=($!)
python3 "${FIXTURES_DIR}/echo_fixture.py" --port 0 \
  --max-connections 12 >"${SCRATCH}/echo-fixture-port.log" 2>&1 &
CHILD_PIDS+=($!)
python3 "${FIXTURES_DIR}/irc_fixture.py" --port 0 \
  --facts "${EVIDENCE_DIR}/irc-fixture-facts.jsonl" >"${SCRATCH}/irc-fixture-port.log" 2>&1 &
CHILD_PIDS+=($!)
HTTP_TARGET=""
ECHO_TARGET=""
IRC_TARGET=""
for _ in $(seq 1 100); do
  HTTP_TARGET="$(fact PORT "${SCRATCH}/http-fixture-port.log")"
  ECHO_TARGET="$(fact PORT "${SCRATCH}/echo-fixture-port.log")"
  IRC_TARGET="$(fact PORT "${SCRATCH}/irc-fixture-port.log")"
  [[ -n "${HTTP_TARGET}" && -n "${ECHO_TARGET}" && -n "${IRC_TARGET}" ]] && break
  sleep 0.1
done
fixture_rc=0
if [[ -z "${HTTP_TARGET}" || -z "${ECHO_TARGET}" || -z "${IRC_TARGET}" ]]; then
  echo "loopback fixtures did not publish ports" >&2
  fixture_rc=1
fi
record_guarded "m10-fixture-startup" \
  "http/echo/irc fixtures on 127.0.0.1 ephemeral ports" \
  "${fixture_rc}"
# Fixture wiring: the main listener's generic server points at the
# HTTP fixture (it serves the HTTP proxy rows and the SOCKS-opaque
# HTTP row); a second listener's generic server points at the raw
# echo fixture for the generic byte-stream rows; the IRC server
# points at the IRC fixture.
echo "    http fixture: 127.0.0.1:${HTTP_TARGET}  echo: 127.0.0.1:${ECHO_TARGET}  irc: 127.0.0.1:${IRC_TARGET}"

# ---- M10 listener (one generation, explicit test-only config) -------------
echo "==> starting i2pr M10 listener"
DATA_DIR="${SCRATCH}/m10-data"
mkdir -p "${DATA_DIR}"
setsid "${LISTENER}" --data-dir "${DATA_DIR}" \
  --generic-server-target "127.0.0.1:${HTTP_TARGET}" \
  --irc-server-target "127.0.0.1:${IRC_TARGET}" \
  >"${SCRATCH}/listener.json" 2>"${SCRATCH}/listener.stderr" &
LISTENER_PID=$!
CHILD_PIDS+=("${LISTENER_PID}")
LISTENER_JSON=""
for _ in $(seq 1 100); do
  if [[ -s "${SCRATCH}/listener.json" ]]; then
    LISTENER_JSON="$(cat "${SCRATCH}/listener.json")"
    [[ "${LISTENER_JSON}" == \{* ]] && break
  fi
  if ! kill -0 "${LISTENER_PID}" 2>/dev/null; then
    echo "i2pr M10 listener exited during startup" >&2
    cat "${SCRATCH}/listener.stderr" >&2 || true
    exit 2
  fi
  sleep 0.1
done
if [[ "${LISTENER_JSON}" != \{* ]]; then
  echo "i2pr M10 listener did not publish its port document" >&2
  exit 2
fi
GENERIC_PORT="$(python3 -c "import json,sys; print(json.load(open('${SCRATCH}/listener.json'))['services']['alpha-generic-client']['port'])")"
HTTP_PORT="$(python3 -c "import json,sys; print(json.load(open('${SCRATCH}/listener.json'))['services']['alpha-http-client']['port'])")"
SOCKS_PORT="$(python3 -c "import json,sys; print(json.load(open('${SCRATCH}/listener.json'))['services']['alpha-socks5-client']['port'])")"
IRC_PORT="$(python3 -c "import json,sys; print(json.load(open('${SCRATCH}/listener.json'))['services']['alpha-irc-client']['port'])")"
SERVER_B64="$(python3 -c "import json,sys; print(json.load(open('${SCRATCH}/listener.json'))['services']['alpha-generic-server']['destination_b64'])")"
cp "${SCRATCH}/listener.json" "${EVIDENCE_DIR}/listener-ports.json"
# The destination base64 stays out of evidence beyond its length:
# only the public length proves the document parsed.
python3 -c "import json; print(len(json.load(open('${SCRATCH}/listener.json'))['services']['alpha-generic-server']['destination_b64']))" \
  > "${EVIDENCE_DIR}/server-destination-b64-length.txt"
echo "    generic=${GENERIC_PORT} http=${HTTP_PORT} socks=${SOCKS_PORT} irc=${IRC_PORT}"

# ---- generic rows ----------------------------------------------------------
# NOTE: the HTTP fixture backs the generic-server target, so the
# generic rows use the echo fixture through a second listener run
# is unnecessary: point generic traffic at the echo fixture by
# using the SOCKS opaque path? No — keep it simple and honest:
# generic rows run against the echo fixture via a dedicated second
# manager below (see generic-listener block).
echo "==> generic rows via dedicated echo-backed listener"
setsid "${LISTENER}" --data-dir "${SCRATCH}/m10-data-generic" \
  --generic-server-target "127.0.0.1:${ECHO_TARGET}" \
  --irc-server-target "127.0.0.1:${IRC_TARGET}" \
  >"${SCRATCH}/generic-listener.json" 2>>"${SCRATCH}/listener.stderr" &
GENERIC_LISTENER_PID=$!
CHILD_PIDS+=("${GENERIC_LISTENER_PID}")
for _ in $(seq 1 100); do
  [[ -s "${SCRATCH}/generic-listener.json" ]] && break
  sleep 0.1
done
GENERIC_ECHO_PORT="$(python3 -c "import json; print(json.load(open('${SCRATCH}/generic-listener.json'))['services']['alpha-generic-client']['port'])")"

echo "==> generic-small via nc"
NC_SMALL_OUT="${SCRATCH}/nc-small.out"
nc_small_rc=0
printf 'plan181-generic-small-001' | timeout 20s nc -w 10 127.0.0.1 "${GENERIC_ECHO_PORT}" > "${NC_SMALL_OUT}" 2>"${SCRATCH}/nc-small.err" || nc_small_rc=$?
if [[ "${nc_small_rc}" -eq 0 ]] && cmp -s <(printf 'plan181-generic-small-001') "${NC_SMALL_OUT}"; then
  nc_small_rc=0
else
  nc_small_rc=1
fi
record_guarded "generic-small-independent" \
  "nc small payload exact bytes via generic tunnel (nc: ${NC_VERSION})" \
  "${nc_small_rc}"

for mode in large halfclose siblings; do
  echo "==> generic-${mode} via stdlib driver"
  mode_rc=0
  timeout 60s python3 "${CLIENTS_DIR}/generic_driver.py" \
    --port "${GENERIC_ECHO_PORT}" --mode "${mode}" \
    >"${SCRATCH}/generic-${mode}.log" 2>&1 || mode_rc=$?
  case "${mode}" in
    large)
      [[ "$(fact LARGE_DIGEST_MATCH "${SCRATCH}/generic-large.log")" == "1" ]] || mode_rc=1
      ;;
    halfclose)
      # Half-close converges to prompt EOF without hanging in the
      # linger. Byte receipt across the half-close is proven by the
      # Rust round-trip suite (see m10-local-roundtrip-suite); an
      # echo reply racing the CLOSE is not part of the contract.
      [[ "$(fact HALFCLOSE_EOF "${SCRATCH}/generic-halfclose.log")" == "1" ]] || mode_rc=1
      ;;
    siblings)
      [[ "$(fact SIBLING_A_MATCH "${SCRATCH}/generic-siblings.log")" == "1" ]] || mode_rc=1
      [[ "$(fact SIBLING_B_MATCH "${SCRATCH}/generic-siblings.log")" == "1" ]] || mode_rc=1
      ;;
  esac
  cp "${SCRATCH}/generic-${mode}.log" "${EVIDENCE_DIR}/generic-${mode}.log"
  record_guarded "generic-${mode}-independent" \
    "stdlib generic driver ${mode} digest/eof facts (single-purpose byte stream only)" \
    "${mode_rc}"
done

# ---- HTTP rows via unmodified curl ------------------------------------------
echo "==> HTTP rows via curl ${CURL_VERSION}"
curl_get_rc=0
timeout 30s curl -sS --max-time 20 -x "http://127.0.0.1:${HTTP_PORT}" \
  http://alpha-test.i2p/hello -o "${SCRATCH}/curl-get.body" \
  -w "%{http_code}" > "${SCRATCH}/curl-get.code" 2>"${SCRATCH}/curl-get.err" || curl_get_rc=$?
GET_CODE="$(cat "${SCRATCH}/curl-get.code" 2>/dev/null || true)"
GET_SHA="$(sha256sum "${SCRATCH}/curl-get.body" 2>/dev/null | awk '{print $1}' || true)"
[[ "${curl_get_rc}" -eq 0 && "${GET_CODE}" == "200" ]] || curl_get_rc=1
[[ "${GET_SHA}" == "$(printf 'hello-from-loopback-fixture' | sha256sum | awk '{print $1}')" ]] || curl_get_rc=1
record_guarded "curl-http-get" \
  "curl proxy GET 200 + body digest (code=${GET_CODE} sha=${GET_SHA:0:12}...)" \
  "${curl_get_rc}"

curl_post_rc=0
printf 'plan181-post-body-002' > "${SCRATCH}/post-payload.bin"
POST_SHA="$(sha256sum "${SCRATCH}/post-payload.bin" 2>/dev/null | awk '{print $1}' || true)"
timeout 30s curl -sS --max-time 20 -x "http://127.0.0.1:${HTTP_PORT}" \
  --data-binary "@${SCRATCH}/post-payload.bin" http://alpha-test.i2p/post \
  -o "${SCRATCH}/curl-post.body" -w "%{http_code}" > "${SCRATCH}/curl-post.code" \
  2>"${SCRATCH}/curl-post.err" || curl_post_rc=$?
POST_CODE="$(cat "${SCRATCH}/curl-post.code" 2>/dev/null || true)"
[[ "${curl_post_rc}" -eq 0 && "${POST_CODE}" == "200" ]] || curl_post_rc=1
FIXTURE_POST_SHA="$(python3 - "${EVIDENCE_DIR}/http-fixture-facts.jsonl" 2>/dev/null <<'PY' || true
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    fact = json.loads(line)
    if fact.get("method") == "POST":
        print(fact.get("body_sha256", ""))
        break
PY
)"
[[ "${FIXTURE_POST_SHA}" == "${POST_SHA}" ]] || curl_post_rc=1
record_guarded "curl-http-post" \
  "curl proxy POST body digest equality (code=${POST_CODE})" \
  "${curl_post_rc}"

curl_large_rc=0
timeout 40s curl -sS --max-time 30 -x "http://127.0.0.1:${HTTP_PORT}" \
  http://alpha-test.i2p/large -o "${SCRATCH}/curl-large.body" \
  -w "%{http_code}" > "${SCRATCH}/curl-large.code" 2>"${SCRATCH}/curl-large.err" || curl_large_rc=$?
LARGE_CODE="$(cat "${SCRATCH}/curl-large.code" 2>/dev/null || true)"
LARGE_LEN="$(wc -c <"${SCRATCH}/curl-large.body" 2>/dev/null || echo 0)"
[[ "${curl_large_rc}" -eq 0 && "${LARGE_CODE}" == "200" && "${LARGE_LEN}" == "65536" ]] || curl_large_rc=1
record_guarded "curl-http-large" \
  "curl proxy 64KiB response digest path (code=${LARGE_CODE} len=${LARGE_LEN})" \
  "${curl_large_rc}"

curl_connect_rc=0
timeout 40s curl -sS --max-time 30 --proxytunnel -x "http://127.0.0.1:${HTTP_PORT}" \
  http://alpha-test.i2p:443/echo -o "${SCRATCH}/curl-connect.body" \
  -w "%{http_code}" > "${SCRATCH}/curl-connect.code" 2>"${SCRATCH}/curl-connect.err" || curl_connect_rc=$?
CONNECT_CODE="$(cat "${SCRATCH}/curl-connect.code" 2>/dev/null || true)"
CONNECT_SHA="$(sha256sum "${SCRATCH}/curl-connect.body" 2>/dev/null | awk '{print $1}' || true)"
[[ "${curl_connect_rc}" -eq 0 && "${CONNECT_CODE}" == "200" ]] || curl_connect_rc=1
[[ "${CONNECT_SHA}" == "$(printf 'hello-from-loopback-fixture' | sha256sum | awk '{print $1}')" ]] || curl_connect_rc=1
# The daemon terminates CONNECT itself, so the loopback fixture
# only ever sees the inner origin-form GET: the tunneled flag
# cannot observe it. Prove CONNECT method dispatch by contrast
# instead: :443 tunnels to 200 while :80 is rejected 403 with no
# Streaming connect (a plain proxy would treat both alike). curl
# exits 56 on a rejected CONNECT, so only the response code gates
# this probe, never curl's own exit status.
CONNECT80_CODE=""
if [[ "${curl_connect_rc}" -eq 0 ]]; then
  # curl reports %{http_code}=000 for a rejected tunnel even though
  # the proxy answered 403 (exit 56); the 403 surfaces in curl's
  # own error text, so gate on that instead of the code.
  timeout 30s curl -sS --max-time 20 --proxytunnel -x "http://127.0.0.1:${HTTP_PORT}" \
    http://alpha-test.i2p:80/echo -o /dev/null \
    -w "%{http_code}" > "${SCRATCH}/curl-connect80.code" \
    2>"${SCRATCH}/curl-connect80.err" || true
  CONNECT80_CODE="$(cat "${SCRATCH}/curl-connect80.code" 2>/dev/null || true)"
  if grep -q "response 403" "${SCRATCH}/curl-connect80.err"; then
    CONNECT80_CODE="403(proxy)"
  else
    curl_connect_rc=1
  fi
fi
record_guarded "curl-http-connect" \
  "curl CONNECT :443 opaque bytes 200+digest with :80 contrast 403, no TLS interception (codes=${CONNECT_CODE}/${CONNECT80_CODE})" \
  "${curl_connect_rc}"

clearnet_rc=0
timeout 30s curl -sS --max-time 20 -x "http://127.0.0.1:${HTTP_PORT}" \
  http://example.com/ -o /dev/null -w "%{http_code}" > "${SCRATCH}/curl-clearnet.code" \
  2>"${SCRATCH}/curl-clearnet.err" || clearnet_rc=$?
CLEARNET_CODE="$(cat "${SCRATCH}/curl-clearnet.code" 2>/dev/null || true)"
if [[ "${CLEARNET_CODE}" == "403" ]]; then
  clearnet_rc=0
else
  clearnet_rc=1
fi
record_guarded "http-clearnet-rejected" \
  "clearnet target bounded 403, no Streaming connect (code=${CLEARNET_CODE})" \
  "${clearnet_rc}"

unknown_rc=0
timeout 30s curl -sS --max-time 20 -x "http://127.0.0.1:${HTTP_PORT}" \
  "http://unknown-$(head -c4 /dev/urandom | od -An -tx1 | tr -d ' \n').i2p/" \
  -o /dev/null -w "%{http_code}" > "${SCRATCH}/curl-unknown.code" \
  2>"${SCRATCH}/curl-unknown.err" || unknown_rc=$?
UNKNOWN_CODE="$(cat "${SCRATCH}/curl-unknown.code" 2>/dev/null || true)"
if [[ "${UNKNOWN_CODE}" == "400" || "${UNKNOWN_CODE}" == "502" ]]; then
  unknown_rc=0
else
  unknown_rc=1
fi
record_guarded "http-unknown-i2p-bounded" \
  "unknown .i2p bounded 400/502 failure (code=${UNKNOWN_CODE})" \
  "${unknown_rc}"

# Privacy-header facts come from the fixture log (digests only in evidence).
PRIVACY_UA="$(python3 - "${EVIDENCE_DIR}/http-fixture-facts.jsonl" 2>/dev/null <<'PY' || true
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    fact = json.loads(line)
    if fact.get("method") == "GET" and fact.get("tunneled") is False:
        print(fact.get("user_agent", ""))
        break
PY
)"
{
  echo "user-agent-observed=${PRIVACY_UA}"
  echo "curl: ${CURL_VERSION}"
} > "${EVIDENCE_DIR}/http-privacy-facts.txt"

# ---- SOCKS rows via unmodified curl ------------------------------------------
echo "==> SOCKS5 rows via curl --socks5-hostname"
# Port 443: the default M10 SOCKS port policy allows the
# canonical HTTPS port only (port 80 is fail-closed by design;
# see socks-clearnet-ip-rejected and the Rust product suite).
socks_rc=0
timeout 30s curl -sS --max-time 20 --socks5-hostname "127.0.0.1:${SOCKS_PORT}" \
  http://alpha-test.i2p:443/socks -o "${SCRATCH}/curl-socks.body" \
  -w "%{http_code}" > "${SCRATCH}/curl-socks.code" 2>"${SCRATCH}/curl-socks.err" || socks_rc=$?
SOCKS_CODE="$(cat "${SCRATCH}/curl-socks.code" 2>/dev/null || true)"
SOCKS_SHA="$(sha256sum "${SCRATCH}/curl-socks.body" 2>/dev/null | awk '{print $1}' || true)"
[[ "${socks_rc}" -eq 0 && "${SOCKS_CODE}" == "200" ]] || socks_rc=1
[[ "${SOCKS_SHA}" == "$(printf 'hello-from-loopback-fixture' | sha256sum | awk '{print $1}')" ]] || socks_rc=1
# `.i2p` never resolves via clearnet DNS: success with
# --socks5-hostname proves hostname-at-proxy semantics.
record_guarded "curl-socks5-domainname" \
  "curl --socks5-hostname DOMAINNAME CONNECT body digest (code=${SOCKS_CODE})" \
  "${socks_rc}"

socks_ip_rc=0
timeout 20s curl -sS --max-time 10 --socks5-hostname "127.0.0.1:${SOCKS_PORT}" \
  http://127.0.0.1/ -o /dev/null -w "%{http_code}" > "${SCRATCH}/curl-socks-ip.code" \
  2>"${SCRATCH}/curl-socks-ip.err" || socks_ip_rc=$?
SOCKS_IP_CODE="$(cat "${SCRATCH}/curl-socks-ip.code" 2>/dev/null || true)"
if [[ "${socks_ip_rc}" -ne 0 || "${SOCKS_IP_CODE}" == "000" ]] || [[ "${SOCKS_IP_CODE}" =~ ^[45] ]]; then
  socks_ip_rc=0
else
  socks_ip_rc=1
fi
record_guarded "socks-clearnet-ip-rejected" \
  "IPv4-literal target fail-closed, no IPv4/IPv6 fallback (curl rc/code=${SOCKS_IP_CODE})" \
  "${socks_ip_rc}"

# ---- IRC rows via exact-pinned jaraco/irc ------------------------------------
echo "==> IRC rows via jaraco/irc ${JARACO_PIN}"
VENV_DIR="${SCRATCH}/irc-venv"
venv_rc=0
python3 -m venv "${VENV_DIR}" >"${SCRATCH}/irc-venv-install.log" 2>&1 || venv_rc=$?
"${VENV_DIR}/bin/pip" install --quiet "${JARACO_SRC}" >>"${SCRATCH}/irc-venv-install.log" 2>&1 || venv_rc=$?
"${VENV_DIR}/bin/python3" -c "import irc.client; print(irc.client.__name__)" \
  >"${SCRATCH}/irc-import.log" 2>&1 || venv_rc=$?
record_guarded "irc-venv-install" \
  "unmodified jaraco/irc ${JARACO_PIN} installed from verified source into an isolated venv" \
  "${venv_rc}"
irc_rc=0
timeout 90s "${VENV_DIR}/bin/python3" "${CLIENTS_DIR}/irc_driver.py" \
  --port "${IRC_PORT}" --nick alice --channel "#chan" \
  >"${SCRATCH}/irc-driver.log" 2>&1 || irc_rc=$?
cp "${SCRATCH}/irc-driver.log" "${EVIDENCE_DIR}/irc-driver.log"
[[ "$(fact CONNECTED "${SCRATCH}/irc-driver.log")" == "1" ]] || irc_rc=1
[[ "$(fact REGISTER_SENT "${SCRATCH}/irc-driver.log")" == "1" ]] || irc_rc=1
[[ "$(fact WELCOME "${SCRATCH}/irc-driver.log")" == "1" ]] || irc_rc=1
record_guarded "irc-independent-register" \
  "jaraco/irc public API connect/register/welcome through the IRC client profile" \
  "${irc_rc}"
message_rc=0
[[ "$(fact ECHO_RECEIVED "${SCRATCH}/irc-driver.log")" == "1" ]] || message_rc=1
[[ "$(fact PONG_SENT "${SCRATCH}/irc-driver.log")" == "1" ]] || message_rc=1
[[ "${irc_rc}" -eq 0 ]] || message_rc=1
record_guarded "irc-independent-message-roundtrip" \
  "bidirectional PRIVMSG echo + PING/PONG through the tunnel pair" \
  "${message_rc}"
hostname_rc=0
USER_LINE="$(python3 - "${EVIDENCE_DIR}/irc-fixture-facts.jsonl" 2>/dev/null <<'PY' || true
import json, sys
for line in open(sys.argv[1], encoding="utf-8"):
    fact = json.loads(line)
    if fact.get("event") == "user":
        print(fact.get("line", ""))
        break
PY
)"
if [[ "${USER_LINE}" == *".b32.i2p"* ]] && [[ "${USER_LINE}" != *"127.0.0.1"* ]]; then
  hostname_rc=0
else
  hostname_rc=1
fi
[[ "${irc_rc}" -eq 0 ]] || hostname_rc=1
record_guarded "irc-user-hostname-authenticated-destination" \
  "fixture USER hostname is the authenticated Destination projection, not client-supplied" \
  "${hostname_rc}"
ctcp_rc=0
[[ "$(fact ACTION_SENT "${SCRATCH}/irc-driver.log")" == "1" ]] || ctcp_rc=1
if grep -q "ACTION waves hello" "${EVIDENCE_DIR}/irc-fixture-facts.jsonl"; then
  :
else
  ctcp_rc=1
fi
if grep -q "DCC SEND" "${EVIDENCE_DIR}/irc-fixture-facts.jsonl"; then
  ctcp_rc=1
fi
[[ "${irc_rc}" -eq 0 ]] || ctcp_rc=1
record_guarded "irc-ctcp-policy-local" \
  "CTCP ACTION passes, DCC dropped at the client privacy filter" \
  "${ctcp_rc}"

# ---- restart stability (same data dir, fresh process) -------------------------
echo "==> server identity restart stability"
stop_group "${LISTENER_PID}"
wait "${LISTENER_PID}" 2>/dev/null || true
stop_group "${GENERIC_LISTENER_PID}"
wait "${GENERIC_LISTENER_PID}" 2>/dev/null || true
sleep 1
setsid "${LISTENER}" --data-dir "${DATA_DIR}" \
  --generic-server-target "127.0.0.1:${HTTP_TARGET}" \
  --irc-server-target "127.0.0.1:${IRC_TARGET}" \
  >"${SCRATCH}/listener-restart.json" 2>>"${SCRATCH}/listener.stderr" &
LISTENER_PID=$!
CHILD_PIDS+=("${LISTENER_PID}")
for _ in $(seq 1 100); do
  [[ -s "${SCRATCH}/listener-restart.json" ]] && break
  sleep 0.1
done
restart_rc=0
RESTART_B64="$(python3 -c "import json; print(json.load(open('${SCRATCH}/listener-restart.json'))['services']['alpha-generic-server']['destination_b64'])" 2>/dev/null || true)"
if [[ -z "${RESTART_B64}" || "${RESTART_B64}" != "${SERVER_B64}" ]]; then
  restart_rc=1
fi
record_guarded "server-identity-restart-stable" \
  "persistent server Destination identical across process restart" \
  "${restart_rc}"
# The restarted listener binds fresh ephemeral ports; the resource
# baseline below must probe the current generation, not the first.
HTTP_PORT="$(python3 -c "import json; print(json.load(open('${SCRATCH}/listener-restart.json'))['services']['alpha-http-client']['port'])")"
SOCKS_PORT="$(python3 -c "import json; print(json.load(open('${SCRATCH}/listener-restart.json'))['services']['alpha-socks5-client']['port'])")"

# ---- remote independent-I2P qualification (§6.1 attempt, §6.3 verdict) ---------
if [[ "${LANE}" == "full" ]]; then
  echo "==> remote qualification attempt against exact-pinned i2pd"
  if [[ ! -x "${I2PD_BIN}" ]]; then
    echo "i2pd binary missing: ${I2PD_BIN}" >&2
    echo "run scripts/interop/fetch-ssu2-reference.sh --rebuild first" >&2
    record_blocked "remote-independent-http-eepsite" \
      "i2pd reference cache absent; attempt not executable (fail closed)"
    record_blocked "remote-independent-irc-service" \
      "i2pd reference cache absent; attempt not executable (fail closed)"
  elif [[ ! -f "${I2PD_CACHE}/source-revision.txt" ]] ||
       [[ "$(<"${I2PD_CACHE}/source-revision.txt")" != "${I2PD_PIN}" ]]; then
    echo "i2pd cache has no verified Plan 161 source revision" >&2
    record_blocked "remote-independent-http-eepsite" \
      "i2pd pin unverified; attempt not executable (fail closed)"
    record_blocked "remote-independent-irc-service" \
      "i2pd pin unverified; attempt not executable (fail closed)"
  elif ! "${I2PD_BIN}" --version 2>&1 | grep -Fq "${I2PD_VERSION}"; then
    echo "i2pd binary does not report ${I2PD_VERSION}" >&2
    record_blocked "remote-independent-http-eepsite" \
      "i2pd version mismatch; attempt not executable (fail closed)"
    record_blocked "remote-independent-irc-service" \
      "i2pd version mismatch; attempt not executable (fail closed)"
  else
    I2PD_HOME="${SCRATCH}/i2pd"
    I2PD_DATA="${I2PD_HOME}/data"
    I2PD_LOG="${EVIDENCE_DIR}/i2pd.log"
    mkdir -p "${I2PD_DATA}"
    SAM_PORT="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
    I2PD_PORT="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
    cat > "${I2PD_HOME}/i2pd.conf" <<EOF
daemon = false
loglevel = info
netid = 2
host = 127.0.0.1
port = ${I2PD_PORT}
ipv4 = true
ipv6 = false
nat = false
notransit = true
floodfill = false
reservedrange = false
bandwidth = L
[sam]
enabled = true
address = 127.0.0.1
port = ${SAM_PORT}
[i2cp]
enabled = false
[bob]
enabled = false
[http]
enabled = false
[httpproxy]
enabled = false
[socksproxy]
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
    SAM_READY=0
    for _ in $(seq 1 120); do
      if (echo > "/dev/tcp/127.0.0.1/${SAM_PORT}") 2>/dev/null; then
        SAM_READY=1
        break
      fi
      if ! kill -0 "${I2PD_PID}" 2>/dev/null; then
        echo "ephemeral i2pd exited during startup" >&2
        sed -n '1,40p' "${I2PD_LOG}" >&2 || true
        break
      fi
      sleep 0.5
    done
    if [[ "${SAM_READY}" -ne 1 ]]; then
      record_blocked "remote-independent-http-eepsite" \
        "i2pd SAM did not listen on 127.0.0.1:${SAM_PORT} (see i2pd.log)"
      record_blocked "remote-independent-irc-service" \
        "i2pd SAM did not listen on 127.0.0.1:${SAM_PORT} (see i2pd.log)"
    else
      # Independently generated destination via SAM DEST GENERATE.
      # Only the public PUB leaves this block; PRIV never touches
      # disk, logs, or evidence.
      I2PD_PUB_FILE="${SCRATCH}/i2pd-peer.pub"
      sam_rc=0
      SAM_PORT="${SAM_PORT}" PUB_FILE="${I2PD_PUB_FILE}" python3 - <<'PY' \
        >"${SCRATCH}/i2pd-sam.log" 2>&1 || sam_rc=$?
import os
import socket

port = int(os.environ["SAM_PORT"])
pub_file = os.environ["PUB_FILE"]

sock = socket.create_connection(("127.0.0.1", port), timeout=15)
try:
    sock.settimeout(15)
    buffer = b""

    def transact(command):
        global buffer
        sock.sendall(command.encode("ascii"))
        while b"\n" not in buffer:
            chunk = sock.recv(65536)
            if not chunk:
                break
            buffer += chunk
        line, _, buffer = buffer.partition(b"\n")
        return line.decode("latin-1", "replace")

    # One SAM session: HELLO and DEST GENERATE share the
    # connection (a fresh socket without HELLO is rejected).
    hello = transact("HELLO VERSION MIN=3.1 MAX=3.1\n")
    print(f"HELLO_REPLY={hello[:80]}")
    if "RESULT=OK" not in hello:
        raise SystemExit(f"SAM hello failed: {hello[:120]}")
    for variant in ("DEST GENERATE SIGNATURE_TYPE=7\n", "DEST GENERATE\n"):
        dest = transact(variant)
        print(f"DEST_VARIANT={variant.strip()}")
        print(f"DEST_REPLY_PREFIX={dest[:60]}")
        # i2pd deviation (documented): DEST GENERATE success carries
        # no RESULT=OK token; `DEST REPLY PUB=... PRIV=...` alone is
        # the success signal. PUB length gates Ed25519 material.
        if dest.startswith("DEST REPLY") and " PUB=" in dest:
            pub = dest.split(" PUB=", 1)[1].split(" ", 1)[0].strip()
            if len(pub) >= 512:
                with open(pub_file, "w", encoding="ascii") as handle:
                    handle.write(pub + "\n")
                print(f"PUB_LEN={len(pub)}")
                break
    else:
        raise SystemExit("SAM DEST GENERATE yielded no usable PUB")
finally:
    sock.close()
PY
      # The SAM transcript (reply prefixes and lengths only, never
      # key material) is evidence in every outcome.
      cp "${SCRATCH}/i2pd-sam.log" "${EVIDENCE_DIR}/i2pd-sam.log" 2>/dev/null || true
      cp "${I2PD_LOG}" "${EVIDENCE_DIR}/i2pd.log" 2>/dev/null || true
      if [[ "${sam_rc}" -ne 0 || ! -s "${I2PD_PUB_FILE}" ]]; then
        record_blocked "remote-independent-http-eepsite" \
          "i2pd SAM DEST GENERATE yielded no public destination (see i2pd-sam.log)"
        record_blocked "remote-independent-irc-service" \
          "i2pd SAM DEST GENERATE yielded no public destination (see i2pd-sam.log)"
      else
        # The qualification driver attempts one M10 connect to the
        # independent destination and asserts the §6.3 stop
        # condition (valid peer, no route, bounded timeout). The
        # driver exits 0 with REMOTE_QUALIFY_* facts when the
        # blocker is demonstrated; any establishment flips it red
        # on purpose (blocker lifted -> re-count as interop).
        QUALIFY_LOG="${EVIDENCE_DIR}/remote-qualify.log"
        : > "${QUALIFY_LOG}"
        qualify_rc=0
        if I2PD_PEER_PUB_B64="$(cat "${I2PD_PUB_FILE}")" \
           timeout --foreground 120s \
           cargo test --locked -p i2pr-daemon --test service_tunnels_remote_qualification \
           m10_remote_independent_router_unreachable_blocker -- --ignored --exact --nocapture --test-threads=1 \
           >>"${QUALIFY_LOG}" 2>&1; then
          qualify_rc=0
        else
          qualify_rc=$?
        fi
        # libtest prefixes fact lines (`test <name> ... FACT=...`)
        # under --nocapture, so extract with unanchored matches.
        qualify_established="$(grep -o 'REMOTE_QUALIFY_ESTABLISHED=[01]' "${QUALIFY_LOG}" 2>/dev/null | head -n1 | cut -d= -f2 || true)"
        qualify_delivered="$(grep -o 'REMOTE_QUALIFY_DELIVERED=[0-9]*' "${QUALIFY_LOG}" 2>/dev/null | head -n1 | cut -d= -f2 || true)"
        if [[ "${qualify_rc}" -eq 0 ]] &&
           grep -Fq "REMOTE_QUALIFY_UNKNOWN_PEER=" "${QUALIFY_LOG}" &&
           [[ "${qualify_established}" == "0" ]] &&
           [[ "${qualify_delivered}" == "0" ]]; then
          record_blocked "remote-independent-http-eepsite" \
            "m6-mixed-router-streaming-blocker: i2pd PUB valid, unknown_peer>0, delivered=0, no establishment (see remote-qualify.log)"
          record_blocked "remote-independent-irc-service" \
            "m6-mixed-router-streaming-blocker: same qualification attempt covers the IRC service path (see remote-qualify.log)"
        else
          # Any deviation — including unexpected establishment
          # (blocker lifted: re-count as interop evidence) — fails
          # closed here. Remote rows never pass through this lane.
          record "remote-independent-http-eepsite" failed \
            "qualification diverged rc=${qualify_rc} established=${qualify_established:-?} delivered=${qualify_delivered:-?} (see remote-qualify.log)"
          record "remote-independent-irc-service" failed \
            "qualification diverged rc=${qualify_rc} established=${qualify_established:-?} delivered=${qualify_delivered:-?} (see remote-qualify.log)"
        fi
      fi
    fi
  fi
else
  record_blocked "remote-independent-http-eepsite" \
    "not attempted in the local-only lane; run the full lane for the §6 qualification"
  record_blocked "remote-independent-irc-service" \
    "not attempted in the local-only lane; run the full lane for the §6 qualification"
fi

# ---- resource baseline -------------------------------------------------------
echo "==> external resource baseline"
# Keep the listener stderr (INFO/WARN diagnostics, no secrets) for
# post-run forensics before the scratch directory is cleaned.
cp "${SCRATCH}/listener.stderr" "${EVIDENCE_DIR}/listener-stderr.log" 2>/dev/null || true
# The ephemeral i2pd router (if provisioned) shuts down here so the
# baseline covers fixtures, listeners, and router alike.
if [[ -n "${I2PD_PID:-}" ]]; then
  stop_group "${I2PD_PID}"
  wait "${I2PD_PID}" 2>/dev/null || true
fi
for pid in "${CHILD_PIDS[@]:-}"; do
  stop_group "${pid}"
done
for pid in "${CHILD_PIDS[@]:-}"; do
  wait "${pid}" 2>/dev/null || true
done
CHILD_PIDS=()
sleep 1
resource_rc=0
if pgrep -f "service_tunnels_loopback_listener" >/dev/null 2>&1; then
  echo "listener process survived shutdown" >&2
  resource_rc=1
fi
if pgrep -f "i2pd.*--datadir=${SCRATCH}/i2pd" >/dev/null 2>&1; then
  echo "ephemeral i2pd process survived shutdown" >&2
  resource_rc=1
fi
if (echo > "/dev/tcp/127.0.0.1/${HTTP_PORT}") 2>/dev/null; then
  echo "HTTP listener port still accepts after shutdown" >&2
  resource_rc=1
fi
if (echo > "/dev/tcp/127.0.0.1/${SOCKS_PORT}") 2>/dev/null; then
  echo "SOCKS listener port still accepts after shutdown" >&2
  resource_rc=1
fi
record_guarded "external-clean-resource-baseline" \
  "listener shutdown leaves no process and loopback ports refused" \
  "${resource_rc}"

# ---- unsupported-profile ledger ------------------------------------------------
echo "==> unsupported-profile ledger"
ledger_rc=0
for keyword in "clearnet outproxy" "SOCKS UDP" "BIND" "SOCKS4" "transparent proxying" \
               "TLS interception" "DCC" "WEBIRC" "floodfill"; do
  if ! grep -Fqi "${keyword}" "${REPO_ROOT}/specs/CONFORMANCE.md"; then
    echo "CONFORMANCE.md omits unsupported profile: ${keyword}" >&2
    ledger_rc=1
  fi
done
record_guarded "unsupported-profile-ledger" \
  "unsupported/deferred profiles explicitly ledgered in specs/CONFORMANCE.md" \
  "${ledger_rc}"

# ---- evidence.json + evidence.md ----------------------------------------------
python3 - "${RESULTS_FILE}" "${EVIDENCE_DIR}" "${REPO_ROOT}" \
  "${JARACO_PIN}" "${I2PD_PIN}" "${I2PD_VERSION}" "${LANE}" \
  "${CURL_VERSION}" "${NC_VERSION}" "${PYTHON_VERSION}" <<'PY'
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

results_path = Path(sys.argv[1])
evidence_dir = Path(sys.argv[2])
repo_root = sys.argv[3]
jaraco_pin, i2pd_pin, i2pd_version, lane = sys.argv[4:8]
curl_version, nc_version, python_version = sys.argv[8:11]

CLASSIFICATION = {
    "m10-fixture-startup": "local-product",
    "m10-prerequisite-plans": "local-product",
    "m10-tool-pin-verification": "local-product",
    "m10-foundation-boundary-checks": "local-product",
    "m10-fixture-startup": "local-product",
    "m10-local-final-product": "local-product",
    "reconcile-rollback-local": "local-product",
    "cross-service-resource-bounds": "local-product",
    "m10-local-roundtrip-suite": "local-product",
    "m10-wire-surface-suite": "local-product",
    "generic-small-independent": "external-application-client",
    "generic-large-independent": "external-application-client",
    "generic-half-close-independent": "external-application-client",
    "generic-siblings-independent": "external-application-client",
    "server-identity-restart-stable": "local-product",
    "curl-http-get": "external-application-client",
    "curl-http-post": "external-application-client",
    "curl-http-large": "external-application-client",
    "curl-http-connect": "external-application-client",
    "http-clearnet-rejected": "external-application-client",
    "http-unknown-i2p-bounded": "external-application-client",
    "curl-socks5-domainname": "external-application-client",
    "socks-clearnet-ip-rejected": "external-application-client",
    "irc-venv-install": "external-application-client",
    "irc-independent-register": "external-application-client",
    "irc-independent-message-roundtrip": "external-application-client",
    "irc-user-hostname-authenticated-destination": "external-application-client",
    "irc-ctcp-policy-local": "external-application-client",
    "remote-independent-http-eepsite": "external-independent-i2p-service",
    "remote-independent-irc-service": "external-independent-i2p-service",
    "external-clean-resource-baseline": "local-product",
    "unsupported-profile-ledger": "local-product",
}

rows = []
with open(results_path, encoding="utf-8") as stream:
    for line in stream:
        label, status, detail = line.rstrip("\n").split("\t", 2)
        rows.append(
            {
                "label": label,
                "status": status,
                "detail": detail,
                "classification": CLASSIFICATION.get(label, "unclassified"),
            }
        )

commit = subprocess.check_output(
    ["git", "-C", repo_root, "rev-parse", "HEAD"], text=True
).strip()
rustc = subprocess.check_output(["rustc", "--version"], text=True).strip()
remote_rows = [row for row in rows if row["classification"] == "external-independent-i2p-service"]
local_failed = [
    row["label"]
    for row in rows
    if row["classification"] != "external-independent-i2p-service"
    and row["status"] != "passed"
]
if local_failed:
    verdict = "failed"
elif any(row["status"] == "blocked" for row in remote_rows):
    verdict = "blocked-by-m6-mixed-router-streaming-blocker"
elif all(row["status"] == "passed" for row in rows):
    verdict = "passed"
else:
    verdict = "failed"

evidence = {
    "schema": "i2pr-m10-external-v1",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": f"service-tunnels-{lane}",
    "bind_policy": "127.0.0.1 ephemeral loopback only (Plan 181 host contract)",
    "curl": curl_version,
    "netcat": nc_version,
    "python": python_version,
    "jaraco_irc": {
        "repository": "https://github.com/jaraco/irc.git",
        "revision": jaraco_pin,
        "role": "independent ordinary IRC client implementation, unmodified",
    },
    "i2pd": {
        "repository": "https://github.com/PurpleI2P/i2pd.git",
        "revision": i2pd_pin,
        "version": i2pd_version,
        "role": "independent I2P router reference for the §6 qualification attempt, unmodified",
    },
    "verdict": verdict,
    "results": rows,
    "known_limitations": [
        "M10 local product plus independent-application-client evidence only; no public I2P participation",
        "remote independent-I2P service rows are recorded blocked under the §6.3 stop condition (m6-mixed-router-streaming-blocker) with command/log provenance",
        "self-composed i2pr rows are never substituted for the remote rows",
        "no external client/router source is patched; no private keys or raw payloads in evidence",
    ],
}
out = Path(evidence_dir)
out.mkdir(parents=True, exist_ok=True)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 181 M10 independent application/service evidence\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- lane: `{lane}`\n")
    stream.write(f"- verdict: `{verdict}`\n")
    stream.write(f"- jaraco/irc: `{jaraco_pin}` (unmodified)\n")
    stream.write(f"- i2pd: `{i2pd_version}` @ `{i2pd_pin}` (unmodified)\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only\n\n")
    stream.write("| Result | Status | Class | Detail |\n| --- | --- | --- | --- |\n")
    for row in rows:
        stream.write(
            f"| {row['label']} | {row['status']} | {row['classification']} | {row['detail']} |\n"
        )
PY

if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
  echo "Plan 181 M10 lane finished with non-passed rows; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 181 M10 lane passed; sanitized evidence: ${EVIDENCE_DIR}"
