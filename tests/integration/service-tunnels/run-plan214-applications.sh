#!/usr/bin/env bash
# Plan 214 — M10 HTTP/IRC product-only external requalification
# runner (evidence hardening over the retained Plan 211 harness,
# final M10 application closure authority).
#
# Standalone fail-closed lane (Plan 214 §14): the retained local
# matrix stays in run-independent.sh; this runner owns the final
# remote application qualification and is the ONLY runner that
# performs it (no duplicate qualification elsewhere).
# run-independent.sh (full lane) delegates to this script and maps
# its aggregate rows onto the `remote-independent-*` rows.
#
# Flow (Plan 214 §14):
#   1. require Plan 213 exact-head status prerequisite;
#   2. verify exact source SHA;
#   3. verify i2pd and jaraco pins from commands;
#   4. fresh evidence dir; remove stale result files;
#   5. start HTTP/IRC loopback fixtures with fresh fact logs;
#   6. start exact-pinned i2pd controlled router;
#   7. provision independent i2pd HTTP + IRC server tunnels;
#   8. wait for public destination files;
#   9. parse only public destination material;
#   10. invoke corrected Plan 214 product-only driver (concurrent
#      inbound pumping around every external client operation);
#   11. independently evaluate fixture facts (via the driver's
#      fresh-seq target rows cross-checked against expected
#      values captured here);
#   12. independently evaluate production counter deltas;
#   13. write HTTP aggregate row;
#   14. write IRC aggregate row;
#   15. record exactly one P214-* terminal classification;
#   16. verify cleanup/resource baseline;
#   17. exit nonzero unless both remote rows pass.
#
# The lane is unprivileged and loopback-only after the existing
# build-dependency installation step. No public I2P, no reseed, no
# i2pd/jaraco patching. Private keys and raw payloads never enter
# evidence (digests/lengths/counters/booleans only).

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"
EVIDENCE_DIR="${I2PR_PLAN214_EVIDENCE_DIR:-${REPO_ROOT}/target/interop/plan214-applications-evidence}"
LANE_DIR="${REPO_ROOT}/tests/integration/service-tunnels"
CLIENTS_DIR="${LANE_DIR}/clients"
FIXTURES_DIR="${LANE_DIR}/fixtures"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
JARACO_PIN="90e10e690da2c7bf60de21be4e36d24c9ffd7474"
I2PD_CACHE="${REPO_ROOT}/target/interop/cache/ssu2/i2pd/${I2PD_PIN}"
I2PD_BIN="${I2PR_I2PD_BIN:-${I2PD_CACHE}/bin/i2pd}"
JARACO_CACHE="${REPO_ROOT}/target/interop/cache/service-tunnels/jaraco_irc/${JARACO_PIN}"
JARACO_SRC_FILE="${JARACO_CACHE}/source-path.txt"
I2PD_PORT="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
I2PD_SAM_PORT="$(python3 -c 'import socket; s = socket.socket(); s.bind(("127.0.0.1", 0)); print(s.getsockname()[1]); s.close()')"
I2PR_PORT="${I2PR_SSU2_PLAN214_PORT:-44109}"
DRIVER_TIMEOUT="600s"
PYTHON_BIN="${PLAN214_PYTHON_BIN:-python3}"
CURL_BIN="${PLAN214_CURL_BIN:-curl}"
IRC_NICK="plan214alice"
IRC_CHANNEL="#chan214"
IRC_TOKEN="plan214-token"

# ---- fresh evidence (no stale rows, Plan 214 §19.18) ----------------------
rm -rf "${EVIDENCE_DIR}"
mkdir -p "${EVIDENCE_DIR}"
SCRATCH="$(mktemp -d -t i2pr-m10-plan214-applications.XXXXXX)"
RESULTS_FILE="${SCRATCH}/results.tsv"
: > "${RESULTS_FILE}"

CHILD_PIDS=()

stop_group() {
  local pid="${1:-}"
  [[ -z "${pid}" ]] && return 0
  # Plan 215 §5 — KILL the process group so CPython's accept()
  # auto-restart on EINTR cannot wedge the runner. SIGTERM would
  # be the polite signal, but the Python default SIGTERM handler
  # is "exit at the next bytecode boundary", which never comes
  # inside an accept() syscall. The kernel can interrupt the
  # syscall but CPython's socket wrapper restarts it, so the
  # fixture ignores SIGTERM until it returns from accept() —
  # which never happens in this lane. SIGKILL is the only signal
  # the kernel will deliver unconditionally; using it here keeps
  # the §16 cleanup bounded and fail-closed. The SCRATCH data
  # directory is throwaway; the loss of any in-flight i2pd
  # bytestream bookkeeping is irrelevant.
  kill -KILL -- "-${pid}" 2>/dev/null || kill -KILL "${pid}" 2>/dev/null || true
}

cleanup() {
  local pid
  for pid in "${CHILD_PIDS[@]:-}"; do
    stop_group "${pid}"
  done
  for pid in "${CHILD_PIDS[@]:-}"; do
    wait "${pid}" 2>/dev/null || true
  done
  if [[ "${PLAN214_KEEP_SCRATCH:-0}" == "1" && -n "${SCRATCH:-}" && -d "${SCRATCH}" ]]; then
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
echo "==> Plan 214 application requalification at ${SOURCE_HEAD}"
record_guarded "plan214-source-head" \
  "qualification source head ${SOURCE_HEAD}" 0

# ---- 1. Plan 213 prerequisite (P214-A) ------------------------------------
echo "==> Plan 213 prerequisite gate"
prereq_rc=0
if ! grep -Fq "passed-m10-router-backed-generic-external-qualification" \
    "${REPO_ROOT}/plans/213-status.md" 2>/dev/null; then
  echo "Plan 213 has no passed status record; refusing Plan 214 closure" >&2
  prereq_rc=1
fi
record_guarded "plan214-prerequisite-plan213" \
  "Plan 213 generic router-backed Direction A+B passed before Plan 214" \
  "${prereq_rc}"

# ---- 2/3. exact-pin verification (command-derived, Plan 214 §6) -----------
echo "==> verifying exact i2pd/jaraco pins from commands"
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
JARACO_SRC=""
if [[ ! -f "${JARACO_CACHE}/source-revision.txt" ]] ||
   [[ "$(<"${JARACO_CACHE}/source-revision.txt")" != "${JARACO_PIN}" ]]; then
  echo "jaraco/irc cache has no verified source revision" >&2
  pin_rc=1
fi
if [[ -f "${JARACO_SRC_FILE}" ]]; then
  JARACO_SRC="$(<"${JARACO_SRC_FILE}")"
fi
if [[ -z "${JARACO_SRC}" || ! -d "${JARACO_SRC}/.git" ]] ||
   [[ "$(git -C "${JARACO_SRC}" rev-parse HEAD)" != "${JARACO_PIN}" ]]; then
  echo "jaraco/irc source checkout is not at the exact pin ${JARACO_PIN}" >&2
  pin_rc=1
fi
if [[ -n "${JARACO_SRC}" && -d "${JARACO_SRC}/.git" ]] &&
   [[ -n "$(git -C "${JARACO_SRC}" status --porcelain --untracked-files=no 2>/dev/null)" ]]; then
  echo "jaraco/irc checkout has tracked modifications (patching forbidden)" >&2
  pin_rc=1
fi
CURL_VERSION="$("${CURL_BIN}" --version 2>/dev/null | head -n1 || true)"
PYTHON_VERSION="$("${PYTHON_BIN}" --version 2>&1 || true)"
[[ -n "${CURL_VERSION}" ]] || pin_rc=1
[[ -n "${PYTHON_VERSION}" ]] || pin_rc=1
{
  echo "source_head=${SOURCE_HEAD}"
  echo "i2pd_pin=${I2PD_PIN}"
  echo "i2pd_version=${I2PD_VERSION}"
  echo "i2pd_reported=${I2PD_REPORTED}"
  echo "jaraco_pin=${JARACO_PIN}"
  echo "curl=${CURL_VERSION}"
  echo "python=${PYTHON_VERSION}"
} > "${EVIDENCE_DIR}/pin-facts.txt"
I2PD_PIN_OK=0; JARACO_PIN_OK=0
[[ "${pin_rc}" -eq 0 ]] && I2PD_PIN_OK=1
[[ "${pin_rc}" -eq 0 ]] && JARACO_PIN_OK=1
record_guarded "plan214-i2pd-pin-sha" \
  "exact i2pd pin ${I2PD_PIN} verified clean" "${pin_rc}"
record_guarded "plan214-i2pd-version" \
  "i2pd reports ${I2PD_VERSION}" "${pin_rc}"
record_guarded "plan214-i2pd-cache-clean" \
  "reference cache verified, no tracked modifications" "${pin_rc}"
record_guarded "plan214-jaraco-pin-sha" \
  "exact jaraco/irc pin ${JARACO_PIN} verified clean" "${pin_rc}"
record_guarded "plan214-jaraco-cache-clean" \
  "jaraco cache verified, no tracked modifications" "${pin_rc}"
record_guarded "plan214-curl-version" \
  "system curl under test: ${CURL_VERSION}" "${pin_rc}"
record_guarded "plan214-python-version" \
  "python under test: ${PYTHON_VERSION}" "${pin_rc}"
if [[ "${pin_rc}" -ne 0 ]]; then
  echo "pin verification failed; refusing to start the reference (P214-B)" >&2
  record "plan214-terminal-classification" failed "P214-B-reference-startup-or-pin"
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi

# ---- static checker --------------------------------------------------------
echo "==> service-tunnel static evidence checker"
checker_rc=0
bash "${REPO_ROOT}/scripts/check-service-tunnel-acceptance-evidence.sh" \
  >"${EVIDENCE_DIR}/static-checker.log" 2>&1 || checker_rc=$?
record_guarded "plan214-static-checker" \
  "check-service-tunnel-acceptance-evidence.sh incl. Plan 214 §28 invariants" \
  "${checker_rc}"

# ---- focused Plan 214 unit floor -------------------------------------------
echo "==> focused Plan 214 unit floor"
floor_rc=0
cargo test --locked -p i2pr-daemon --test service_tunnels_application_product_only_remote_qualification -- \
  --test-threads=1 >>"${EVIDENCE_DIR}/unit-floor.log" 2>&1 || floor_rc=$?
record_guarded "plan214-unit-floor" \
  "plan214 driver evidence-helper unit rows (cargo test, ignored external driver skipped)" \
  "${floor_rc}"

# ---- 5. HTTP/IRC loopback fixtures with fresh fact logs --------------------
echo "==> starting Plan 214 HTTP/IRC loopback fixtures"
HTTP_FACTS="${EVIDENCE_DIR}/http-fixture-facts.jsonl"
IRC_FACTS="${EVIDENCE_DIR}/irc-fixture-facts.jsonl"
: > "${HTTP_FACTS}"
: > "${IRC_FACTS}"
# Plan 215 §5 — the loopback fixtures run in their own session and
# process group so the §16 cleanup's `kill -TERM -- -${pid}` only
# signals the fixture subtree. Without `setsid`, the fixture shares
# the runner's process group, the cleanup TERM would also signal
# the runner (and any unrelated siblings that inherited that pgid),
# and the §16 pgrep / port-baseline checks would race against
# grandchildren that have not yet reaped.
setsid "${PYTHON_BIN}" "${FIXTURES_DIR}/http_fixture.py" --port 0 \
  --facts "${HTTP_FACTS}" --max-connections 24 \
  >"${SCRATCH}/http-fixture-port.log" 2>&1 &
CHILD_PIDS+=($!)
setsid "${PYTHON_BIN}" "${FIXTURES_DIR}/irc_fixture.py" --port 0 \
  --facts "${IRC_FACTS}" \
  >"${SCRATCH}/irc-fixture-port.log" 2>&1 &
CHILD_PIDS+=($!)
HTTP_TARGET=""; IRC_TARGET=""; LARGE_LEN=""; LARGE_SHA=""
for _ in $(seq 1 100); do
  HTTP_TARGET="$(grep -E '^PORT=' "${SCRATCH}/http-fixture-port.log" 2>/dev/null | head -n1 | cut -d= -f2 || true)"
  IRC_TARGET="$(grep -E '^PORT=' "${SCRATCH}/irc-fixture-port.log" 2>/dev/null | head -n1 | cut -d= -f2 || true)"
  LARGE_LEN="$(grep -Eo 'LARGE_LEN=[0-9]+' "${SCRATCH}/http-fixture-port.log" 2>/dev/null | head -n1 | cut -d= -f2 || true)"
  LARGE_SHA="$(grep -Eo 'LARGE_SHA256=[0-9a-f]+' "${SCRATCH}/http-fixture-port.log" 2>/dev/null | head -n1 | cut -d= -f2 || true)"
  if [[ -n "${HTTP_TARGET}" && -n "${IRC_TARGET}" && -n "${LARGE_LEN}" && -n "${LARGE_SHA}" ]]; then
    break
  fi
  sleep 0.1
done
fixture_rc=0
if [[ -z "${HTTP_TARGET}" || -z "${IRC_TARGET}" ]]; then
  echo "Plan 214 fixtures did not publish ports" >&2
  fixture_rc=1
fi
if [[ -z "${LARGE_LEN}" || "${#LARGE_SHA}" -ne 64 ]]; then
  echo "Plan 214 HTTP fixture did not publish its large-response contract" >&2
  fixture_rc=1
fi
{
  echo "http_target_port=${HTTP_TARGET}"
  echo "irc_target_port=${IRC_TARGET}"
  echo "large_expected_len=${LARGE_LEN}"
  echo "large_expected_sha256=${LARGE_SHA}"
} > "${EVIDENCE_DIR}/fixture-contract.txt"
record_guarded "plan214-fixture-startup" \
  "http/irc fixtures on 127.0.0.1:${HTTP_TARGET:-unset}/${IRC_TARGET:-unset} with large contract len=${LARGE_LEN:-unset}" \
  "${fixture_rc}"
if [[ "${fixture_rc}" -ne 0 ]]; then
  record "plan214-terminal-classification" failed "P214-H-http-policy-or-fixture-integrity"
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi
echo "    http fixture: 127.0.0.1:${HTTP_TARGET}  irc: 127.0.0.1:${IRC_TARGET}  large: len=${LARGE_LEN} sha=${LARGE_SHA:0:16}..."

# Plan 215 §4 — deterministic, shell-inert writer for the Plan 214
# i2pd tunnels.conf. The previous unquoted heredoc containing
# Markdown backticks inside explanatory comments was interpreted by
# the shell as command substitution while generating the config; the
# hosted lane therefore stripped `type = server` from the IRC server
# tunnel and i2pd never produced the destination .dat files.
#
# The shell is never asked to interpret template prose: every line
# of generated config is emitted either as a literal argument to
# printf or as a single %s interpolation of an explicitly-passed
# port number. No backtick, $(), or ${} expansion can reach the
# file body.
#
# Plan 214 local investigation established why the external transport
# endpoint uses an i2pd transparent server tunnel rather than i2pd's
# IRC-transforming tunnel type: i2pd's `type = irc`
# (I2PTunnelConnectionIRC::Write) re-emits every received chunk
# line-by-line with bare `\n` and drops trailing partials, which
# corrupts any chunked byte stream at the tunnel layer (proven by
# packet-tap A/B: identical i2pr bytes arrive byte-exact through
# `type = server` and mangled with stray `\n` + duplicated QUIT
# through `type = irc`). i2pd is the encrypted-transport endpoint
# here, not the IRC application counterpart (that is the harness
# fixture); the IRC application semantics under test — registration,
# neutral USER privacy rewrite, PING/PONG, token PRIVMSG echo,
# ACTION pass, DCC block — are all enforced by the i2pr IRC client
# profile and observed at the fixture either way.
write_plan214_tunnels_conf() {
  local path="$1"
  local http_port="$2"
  local irc_port="$3"

  {
    printf '%s\n' \
      '[HTTP-Server]' \
      'type = http' \
      'host = 127.0.0.1'
    printf 'port = %s\n' "${http_port}"
    printf '%s\n' \
      'keys = plan214-http-server.dat' \
      'inbound.length = 0' \
      'outbound.length = 0' \
      '' \
      '[IRC-Server]' \
      'type = server' \
      'host = 127.0.0.1'
    printf 'port = %s\n' "${irc_port}"
    printf '%s\n' \
      'keys = plan214-irc-server.dat' \
      'inbound.length = 0' \
      'outbound.length = 0'
  } > "${path}"
}

# Plan 215 §5 — pre-launch generated-config sanity gate. Fail closed
# unless all 12 invariants from Plan 215 §5 hold. Reports only bounded
# public facts (section count, profile names, configured port equality,
# key filenames). No private key bytes or raw application payloads.
# The helper is a pure file/argument inspection — it does not launch
# i2pd and it does not read the raw i2pd log.
plan215_section_body() {
  # Emit the body of `[<section>]` from `<path>`, stopping at the next
  # `[…]` header or EOF. Comment and blank lines are preserved so a
  # future regression that inserts prose inside the section is still
  # caught by the typed-key checks below.
  local section="$1"
  local path="$2"
  awk -v sec="[${section}]" '
    $0 == sec { flag = 1; next }
    /^\[/     { flag = 0; next }
    flag      { print }
  ' "${path}"
}

validate_plan214_tunnels_conf() {
  local path="$1"
  local http_port="$2"
  local irc_port="$3"
  local rc=0
  local http_sections irc_sections total_sections
  local http_body irc_body

  # 1. file exists and is non-empty
  if [[ ! -s "${path}" ]]; then
    echo "tunnels.conf is missing or empty: ${path}" >&2
    rc=1
  fi

  if [[ "${rc}" -eq 0 ]]; then
    http_sections="$(grep -cE '^\[HTTP-Server\]$' "${path}" 2>/dev/null || true)"
    irc_sections="$(grep -cE '^\[IRC-Server\]$' "${path}" 2>/dev/null || true)"
    total_sections="$(grep -cE '^\[' "${path}" 2>/dev/null || true)"

    # 2. exactly one [HTTP-Server] section
    if [[ "${http_sections}" != "1" ]]; then
      echo "tunnels.conf [HTTP-Server] section count is ${http_sections:-0}, expected 1" >&2
      rc=1
    fi
    # 3. exactly one [IRC-Server] section
    if [[ "${irc_sections}" != "1" ]]; then
      echo "tunnels.conf [IRC-Server] section count is ${irc_sections:-0}, expected 1" >&2
      rc=1
    fi
    # 11. no unintended third tunnel section exists
    if [[ "${total_sections:-0}" -ne 2 ]]; then
      echo "tunnels.conf total section count is ${total_sections:-0}, expected 2" >&2
      rc=1
    fi
  fi

  if [[ "${rc}" -eq 0 ]]; then
    http_body="$(plan215_section_body 'HTTP-Server' "${path}")"
    irc_body="$(plan215_section_body 'IRC-Server' "${path}")"

    # 4. HTTP section contains `type = http`
    if ! grep -qxF 'type = http' <<<"${http_body}"; then
      echo "tunnels.conf [HTTP-Server] section missing 'type = http'" >&2
      rc=1
    fi
    # 5. IRC section contains `type = server`
    if ! grep -qxF 'type = server' <<<"${irc_body}"; then
      echo "tunnels.conf [IRC-Server] section missing 'type = server'" >&2
      rc=1
    fi
    # 6. configured HTTP port equals ${HTTP_TARGET}
    if ! grep -qxF "port = ${http_port}" <<<"${http_body}"; then
      echo "tunnels.conf [HTTP-Server] port does not match expected ${http_port}" >&2
      rc=1
    fi
    # 7. configured IRC port equals ${IRC_TARGET}
    if ! grep -qxF "port = ${irc_port}" <<<"${irc_body}"; then
      echo "tunnels.conf [IRC-Server] port does not match expected ${irc_port}" >&2
      rc=1
    fi
    # 8. HTTP key file is `plan214-http-server.dat`
    if ! grep -qxF 'keys = plan214-http-server.dat' <<<"${http_body}"; then
      echo "tunnels.conf [HTTP-Server] keys file is not plan214-http-server.dat" >&2
      rc=1
    fi
    # 9. IRC key file is `plan214-irc-server.dat`
    if ! grep -qxF 'keys = plan214-irc-server.dat' <<<"${irc_body}"; then
      echo "tunnels.conf [IRC-Server] keys file is not plan214-irc-server.dat" >&2
      rc=1
    fi
    # 10. both profiles retain zero-hop inbound/outbound lengths
    if ! grep -qxF 'inbound.length = 0' <<<"${http_body}" ||
       ! grep -qxF 'outbound.length = 0' <<<"${http_body}"; then
      echo "tunnels.conf [HTTP-Server] zero-hop inbound/outbound length missing" >&2
      rc=1
    fi
    if ! grep -qxF 'inbound.length = 0' <<<"${irc_body}" ||
       ! grep -qxF 'outbound.length = 0' <<<"${irc_body}"; then
      echo "tunnels.conf [IRC-Server] zero-hop inbound/outbound length missing" >&2
      rc=1
    fi
    # 12. no unresolved template token/place-holder in the generated file
    if grep -nE '\$\{[A-Za-z_][A-Za-z0-9_]*\(`|\$\{[A-Za-z_][A-Za-z0-9_]*(:[^}]*)?\}`|\{\{[A-Za-z_][A-Za-z0-9_]*\}\}|<%[A-Za-z_][A-Za-z0-9_]*%>|__[A-Za-z_][A-Za-z0-9_]*__' "${path}" >/dev/null 2>&1; then
      echo "tunnels.conf contains unresolved template placeholders" >&2
      rc=1
    fi
  fi

  return "${rc}"
}

# ---- 6/7. ephemeral i2pd with HTTP + IRC server tunnels --------------------
I2PD_HOME="${SCRATCH}/i2pd"
I2PD_DATA="${I2PD_HOME}/data"
# The raw i2pd log stays in scratch: it carries SAM session lines
# that may embed reference key material. Only sanitized counts
# extracted below reach evidence; the per-tunnel .dat files never
# leave scratch except through the public-only parse helper.
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
# Plan 215 §4 — the i2pd tunnel configuration is generated through a
# deterministic, shell-inert printf writer (see write_plan214_tunnels_conf
# above). It MUST NOT be written via an unquoted heredoc: command
# substitution inside the heredoc body would strip literal text and
# silently break the tunnels i2pd is supposed to provision.
write_plan214_tunnels_conf "${I2PD_HOME}/tunnels.conf" "${HTTP_TARGET}" "${IRC_TARGET}"
# Plan 215 §5 — pre-launch sanity gate. Fail closed before launching
# i2pd if the generated tunnels.conf drifts from the documented
# contract (section count, profile names, configured port equality,
# key filenames, zero-hop lengths, no unresolved placeholders).
tunnels_conf_rc=0
validate_plan214_tunnels_conf "${I2PD_HOME}/tunnels.conf" "${HTTP_TARGET}" "${IRC_TARGET}" \
  || tunnels_conf_rc=$?
record_guarded "plan214-reference-tunnel-config-sanity" \
  "tunnels.conf sections, profile names, ports, keys, lengths, placeholders all match Plan 215 §5 contract" \
  "${tunnels_conf_rc}"
if [[ "${tunnels_conf_rc}" -ne 0 ]]; then
  cp "${I2PD_HOME}/tunnels.conf" "${EVIDENCE_DIR}/tunnels.conf" 2>/dev/null || true
  record "plan214-terminal-classification" failed "P214-B-reference-startup-or-pin"
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi
cp "${I2PD_HOME}/tunnels.conf" "${EVIDENCE_DIR}/tunnels.conf"
: > "${I2PD_LOG}"
setsid "${I2PD_BIN}" "--conf=${I2PD_HOME}/i2pd.conf" "--tunconf=${I2PD_HOME}/tunnels.conf" "--datadir=${I2PD_DATA}" \
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
    break
  fi
  sleep 0.5
done
SAM_READY=0
for _ in $(seq 1 60); do
  if (exec 3<>"/dev/tcp/127.0.0.1/${I2PD_SAM_PORT}") 2>/dev/null; then
    exec 3<&- 3>&- || true
    SAM_READY=1
    break
  fi
  if ! kill -0 "${I2PD_PID}" 2>/dev/null; then
    echo "ephemeral i2pd exited before SAM came up" >&2
    break
  fi
  sleep 0.5
done
if [[ -z "${I2PD_RI}" || "${SAM_READY}" -ne 1 ]]; then
  echo "ephemeral i2pd did not publish router.info / SSU2 / SAM" >&2
  record "plan214-reference-router-ready" failed "no router.info / SSU2 / SAM listener"
  record "plan214-terminal-classification" failed "P214-B-reference-startup-or-pin"
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi
record_guarded "plan214-reference-router-ready" \
  "router.info + SSU2 on 127.0.0.1:${I2PD_PORT} + SAM on 127.0.0.1:${I2PD_SAM_PORT}" 0

# ---- 8/9. public destination files (public material only) ------------------
HTTP_DEST_DAT="${I2PD_DATA}/plan214-http-server.dat"
IRC_DEST_DAT="${I2PD_DATA}/plan214-irc-server.dat"
HTTP_DEST_READY=0; IRC_DEST_READY=0
for _ in $(seq 1 120); do
  [[ -s "${HTTP_DEST_DAT}" ]] && HTTP_DEST_READY=1
  [[ -s "${IRC_DEST_DAT}" ]] && IRC_DEST_READY=1
  if [[ "${HTTP_DEST_READY}" -eq 1 && "${IRC_DEST_READY}" -eq 1 ]]; then
    break
  fi
  if ! kill -0 "${I2PD_PID}" 2>/dev/null; then
    break
  fi
  sleep 0.5
done
extract_rc=0
if [[ "${HTTP_DEST_READY}" -ne 1 || "${IRC_DEST_READY}" -ne 1 ]]; then
  echo "i2pd server tunnel .dat files were not produced within the bounded wait" >&2
  extract_rc=1
fi
HTTP_DEST_B64=""; HTTP_DEST_HASH=""; HTTP_DEST_B32=""
IRC_DEST_B64=""; IRC_DEST_HASH=""; IRC_DEST_B32=""
if [[ "${extract_rc}" -eq 0 ]]; then
  # Plan 214 §12 — the helper reads only the public Destination
  # prefix; private suffix bytes never cross into evidence or env
  # beyond the configured public base64 the driver needs.
  if ! HTTP_DEST_B64="$(python3 "${CLIENTS_DIR}/parse_i2pd_destination.py" "${HTTP_DEST_DAT}" 2>"${SCRATCH}/parse-http.log" | awk -F: '$1 == "dest_b64" {print $2; exit}')" ||
     ! HTTP_DEST_HASH="$(python3 "${CLIENTS_DIR}/parse_i2pd_destination.py" "${HTTP_DEST_DAT}" 2>>"${SCRATCH}/parse-http.log" | awk -F: '$1 == "dest_hash" {print $2; exit}')" ||
     ! HTTP_DEST_B32="$(python3 "${CLIENTS_DIR}/parse_i2pd_destination.py" "${HTTP_DEST_DAT}" 2>>"${SCRATCH}/parse-http.log" | awk -F: '$1 == "dest_b32" {print $2; exit}')" ||
     ! IRC_DEST_B64="$(python3 "${CLIENTS_DIR}/parse_i2pd_destination.py" "${IRC_DEST_DAT}" 2>"${SCRATCH}/parse-irc.log" | awk -F: '$1 == "dest_b64" {print $2; exit}')" ||
     ! IRC_DEST_HASH="$(python3 "${CLIENTS_DIR}/parse_i2pd_destination.py" "${IRC_DEST_DAT}" 2>>"${SCRATCH}/parse-irc.log" | awk -F: '$1 == "dest_hash" {print $2; exit}')" ||
     ! IRC_DEST_B32="$(python3 "${CLIENTS_DIR}/parse_i2pd_destination.py" "${IRC_DEST_DAT}" 2>>"${SCRATCH}/parse-irc.log" | awk -F: '$1 == "dest_b32" {print $2; exit}')"; then
    echo "destination extraction helper failed" >&2
    extract_rc=1
  fi
fi
if [[ "${extract_rc}" -eq 0 ]]; then
  if [[ "${#HTTP_DEST_HASH}" -ne 64 || "${#IRC_DEST_HASH}" -ne 64 ]]; then
    echo "extracted destination hash has wrong shape" >&2
    extract_rc=1
  elif [[ "${HTTP_DEST_HASH}" == "0000000000000000000000000000000000000000000000000000000000000000" ]] ||
       [[ "${IRC_DEST_HASH}" == "0000000000000000000000000000000000000000000000000000000000000000" ]]; then
    echo "extracted destination hash is zero" >&2
    extract_rc=1
  elif [[ "${HTTP_DEST_HASH}" == "${IRC_DEST_HASH}" ]]; then
    echo "HTTP and IRC destinations are not independently owned" >&2
    extract_rc=1
  fi
fi
if [[ "${extract_rc}" -eq 0 ]]; then
  # The computed hash must match the b32 label (integrity of the
  # public material before the driver consumes it).
  for pair in "${HTTP_DEST_HASH} ${HTTP_DEST_B32}" "${IRC_DEST_HASH} ${IRC_DEST_B32}"; do
    set -- ${pair}
    recomputed="$(printf '%s' "$1" | xxd -r -p 2>/dev/null | python3 -c 'import base64,sys; print(base64.b32encode(sys.stdin.buffer.read()).decode().rstrip("=").lower() + ".b32.i2p")' || true)"
    if [[ "${recomputed}" != "$2" ]]; then
      echo "destination hash/b32 mismatch for $2" >&2
      extract_rc=1
    fi
  done
fi
record_guarded "plan214-public-destination-extraction" \
  "HTTP + IRC public destinations extracted, nonzero, distinct, hash/b32 consistent" \
  "${extract_rc}"
if [[ "${extract_rc}" -ne 0 ]]; then
  record "plan214-terminal-classification" failed "P214-C-public-destination-extraction"
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi
{
  echo "http_dest_hash=${HTTP_DEST_HASH}"
  echo "http_dest_b32=${HTTP_DEST_B32}"
  echo "irc_dest_hash=${IRC_DEST_HASH}"
  echo "irc_dest_b32=${IRC_DEST_B32}"
} > "${EVIDENCE_DIR}/destinations.txt"
echo "    http=${HTTP_DEST_B32:0:16}... irc=${IRC_DEST_B32:0:16}... (waiting for LS2 publication)"
# Bounded publication gate: the zero-hop server tunnels need no
# peers, but i2pd still builds the zero-hop inbound and publishes
# each LeaseSet2 into its own NetDB asynchronously. The lane
# proceeds only after both publications land (`LeaseSet2
# updated` x2 in the controlled log); a mere `.dat` file proves
# key generation, not reachability.
published_rc=0
for _ in $(seq 1 180); do
  if [[ "$(grep -c 'NetDb: LeaseSet2 updated' "${I2PD_LOG}" 2>/dev/null || true)" -ge 2 ]]; then
    published_rc=0
    break
  fi
  if ! kill -0 "${I2PD_PID}" 2>/dev/null; then
    published_rc=1
    break
  fi
  sleep 0.5
done
if [[ "$(grep -c 'NetDb: LeaseSet2 updated' "${I2PD_LOG}" 2>/dev/null || true)" -lt 2 ]]; then
  published_rc=1
fi
record_guarded "plan214-server-ls2-published" \
  "both zero-hop server-tunnel LeaseSet2s published into the reference NetDB" \
  "${published_rc}"
if [[ "${published_rc}" -ne 0 ]]; then
  record "plan214-terminal-classification" failed "P214-C-public-destination-extraction"
  cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
  exit 1
fi
# Short stabilization so the published records are lookup-stable.
sleep 5

# ---- 10. counted Plan 214 product-only driver ------------------------------
echo "==> counted Plan 214 product-only driver (explicit ignored selection)"
DRIVER_LOG="${EVIDENCE_DIR}/external-driver.log"
: > "${DRIVER_LOG}"
driver_rc=0
if I2PD_ROUTER_INFO="${I2PD_RI}" \
   I2PD_SSU2_ENDPOINT="127.0.0.1:${I2PD_PORT}" \
   I2PR_SSU2_BIND="127.0.0.1:${I2PR_PORT}" \
   EVIDENCE_DIR="${EVIDENCE_DIR}" \
   PLAN214_HTTP_FIXTURE_FACTS="${HTTP_FACTS}" \
   PLAN214_IRC_FIXTURE_FACTS="${IRC_FACTS}" \
   PLAN214_HTTP_LARGE_EXPECTED_LEN="${LARGE_LEN}" \
   PLAN214_HTTP_LARGE_EXPECTED_SHA256="${LARGE_SHA}" \
   PLAN214_JARACO_SRC="${JARACO_SRC}" \
   PLAN214_HARNESS_DIR="${LANE_DIR}" \
   PLAN214_CURL_BIN="${CURL_BIN}" \
   PLAN214_PYTHON_BIN="${PYTHON_BIN}" \
   PLAN214_I2PD_PIN_OK="${I2PD_PIN_OK}" \
   PLAN214_JARACO_PIN_OK="${JARACO_PIN_OK}" \
   PLAN214_IRC_NICK="${IRC_NICK}" \
   PLAN214_IRC_CHANNEL="${IRC_CHANNEL}" \
   PLAN214_IRC_TOKEN="${IRC_TOKEN}" \
   PLAN214_HTTP_DEST_B64="${HTTP_DEST_B64}" \
   PLAN214_HTTP_DEST_HASH="${HTTP_DEST_HASH}" \
   PLAN214_HTTP_DEST_B32="${HTTP_DEST_B32}" \
   PLAN214_IRC_DEST_B64="${IRC_DEST_B64}" \
   PLAN214_IRC_DEST_HASH="${IRC_DEST_HASH}" \
   PLAN214_IRC_DEST_B32="${IRC_DEST_B32}" \
   timeout --foreground "${DRIVER_TIMEOUT}" \
   cargo test --locked -p i2pr-daemon --test service_tunnels_application_product_only_remote_qualification \
   m10_product_only_remote_http_and_irc_application_interop_v214 -- --ignored --exact --nocapture --test-threads=1 \
   >>"${DRIVER_LOG}" 2>&1; then
  driver_rc=0
else
  driver_rc=$?
fi
DRIVER_TSV="${EVIDENCE_DIR}/plan214-driver/driver-evidence.tsv"

# ---- sanitized reference-side counts (never key material) ------------------
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

# ---- 11/12. mandatory evidence validation ----------------------------------
# Key-gated on the fresh per-run driver TSV (stale evidence was
# deleted at startup): the key proves the driver executed that
# step in this run. Exact-value rows additionally require the
# documented value; delta rows require a positive integer; digest
# rows require equality with the independently known expected
# value captured by this runner (never shape alone).
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

# Duplicate conflicting keys are never valid evidence (Plan 214
# §19.17): the same label twice fails the row even if one value
# looks right. Duplicates with identical values are also rejected
# (the driver must emit each fact exactly once).
duplicate_key() {
  local key="$1"
  [[ -f "${DRIVER_TSV}" ]] || return 1
  local count
  count="$(awk -F'\t' -v want="${key}" '$1 == want {count++} END{print count+0}' "${DRIVER_TSV}" 2>/dev/null || true)"
  [[ "${count}" -gt 1 ]]
}

m214_exact_row() {
  local label="$1"
  local key="$2"
  local expected="$3"
  local detail="$4"
  local rc=1
  local value
  if value="$(tsv_value "${key}")" && [[ "${value}" == "${expected}" ]]; then
    if ! duplicate_key "${key}"; then
      rc=0
    fi
  fi
  record_guarded "${label}" "${detail} (value=${value:-missing})" "${rc}"
}

m214_positive_row() {
  local label="$1"
  local key="$2"
  local detail="$3"
  local rc=1
  local value
  if value="$(tsv_value "${key}")" && [[ "${value}" =~ ^[0-9]+$ ]] && (( value >= 1 )); then
    if ! duplicate_key "${key}"; then
      rc=0
    fi
  fi
  record_guarded "${label}" "${detail} (value=${value:-missing})" "${rc}"
}

m214_digest_row() {
  local label="$1"
  local key="$2"
  local expected="$3"
  local detail="$4"
  local rc=1
  local value
  if value="$(tsv_value "${key}")" &&
     [[ "${#value}" -eq 64 && "${value}" =~ ^[0-9a-f]+$ ]] &&
     [[ "${value}" == "${expected}" ]]; then
    if ! duplicate_key "${key}"; then
      rc=0
    fi
  fi
  record_guarded "${label}" "${detail} (match=$([[ "${value:-}" == "${expected}" ]] && echo yes || echo no))" "${rc}"
}

# The counted driver must have run in this lane: without its TSV
# every row below fails closed.
if [[ ! -f "${DRIVER_TSV}" ]]; then
  echo "counted driver produced no evidence TSV (driver_rc=${driver_rc})" >&2
  tail -n 20 "${DRIVER_LOG}" >&2 || true
fi

# Plan 214 §16 — HTTP facts.
SMALL_SHA="$(printf 'hello-from-loopback-fixture' | sha256sum | awk '{print $1}')"
m214_exact_row "plan214-http-get-command-exit" "http-get-command-exit" "0" \
  "curl GET exit code"
m214_exact_row "plan214-http-get-status" "http-get-status" "200" \
  "actual curl %{http_code} status (never inferred)"
m214_digest_row "plan214-http-get-response" "http-get-response-sha256" "${SMALL_SHA}" \
  "GET body digest equals the fixture contract"
m214_exact_row "plan214-http-get-fixture-method" "http-get-fixture-method" "GET" \
  "fresh target record method after the GET baseline"
m214_key_exact_path() {
  local value
  value="$(tsv_value "http-get-fixture-path" || true)"
  local rc=1
  if [[ "${value}" == "/hello" || "${value}" == *"/hello" ]]; then
    duplicate_key "http-get-fixture-path" || rc=0
  fi
  record_guarded "plan214-http-get-fixture-path" \
    "fresh target record path is /hello (value=${value:-missing})" "${rc}"
}
m214_key_exact_path
POST_SENT_SHA="$(tsv_value "http-post-request-sha256" || true)"
POST_FIXTURE_SHA="$(tsv_value "http-post-fixture-body-sha256" || true)"
post_rc=1
if [[ -n "${POST_SENT_SHA}" && "${POST_SENT_SHA}" == "${POST_FIXTURE_SHA}" ]] &&
   [[ "${#POST_SENT_SHA}" -eq 64 ]] &&
   ! duplicate_key "http-post-fixture-body-sha256" &&
   ! duplicate_key "http-post-request-sha256"; then
  post_rc=0
fi
record_guarded "plan214-http-post-digest-equality" \
  "sent POST digest equals target-observed digest" "${post_rc}"
m214_exact_row "plan214-http-post-response" "http-post-response-ok" "1" \
  "POST response matches the fixture posted=<len> contract"
m214_exact_row "plan214-http-post-command-exit" "http-post-command-exit" "0" \
  "curl POST exit code"
m214_digest_row "plan214-http-large-response" "http-large-response-sha256" "${LARGE_SHA}" \
  "large response digest equals the fixture-published contract"
m214_exact_row "plan214-http-large-expected-len" "http-large-expected-len" "${LARGE_LEN}" \
  "driver consumed the runner-published large length"
m214_exact_row "plan214-http-clearnet-rejected" "http-clearnet-rejected" "1" \
  "clearnet rejected locally"
m214_exact_row "plan214-http-clearnet-target-zero" "http-clearnet-target-observation-delta-zero" "1" \
  "clearnet never reached the target fixture"
m214_exact_row "plan214-http-ip-rejected" "http-ip-literal-rejected" "1" \
  "IP literal rejected locally"
m214_exact_row "plan214-http-ip-target-zero" "http-ip-target-observation-delta-zero" "1" \
  "IP literal never reached the target fixture"
m214_exact_row "plan214-http-startup-hash-match" "http-startup-destination-hash-match" "1" \
  "startup summary addresses the exact HTTP destination"
m214_positive_row "plan214-http-startup-lease-count" "http-startup-lease-count" \
  "startup installed real leases for the HTTP service"
m214_exact_row "plan214-http-startup-owner" "http-startup-inbound-owner-registered" "1" \
  "startup registered the HTTP inbound owner"
m214_positive_row "plan214-http-remote-outbound-delta" "http-remote-outbound-composed-delta" \
  "HTTP window remote outbound composed delta"
m214_positive_row "plan214-http-router-delivery-delta" "http-router-delivery-delta" \
  "HTTP window router delivery delta"
m214_positive_row "plan214-http-remote-inbound-delta" "http-remote-inbound-dispatched-delta" \
  "HTTP window remote inbound dispatched delta"
m214_exact_row "plan214-http-local-coowned-zero" "http-local-coowned-delta-zero" "1" \
  "HTTP window zero local co-owned substitution"
m214_exact_row "plan214-http-unknown-peer-zero" "http-unknown-peer-delta-zero" "1" \
  "HTTP window zero unknown-peer outcomes"
m214_exact_row "plan214-http-orphan-zero" "http-orphan-receive-delta-zero" "1" \
  "HTTP window zero orphan receives"

# Plan 214 §17 — IRC facts.
m214_exact_row "plan214-irc-command-exit" "irc-command-exit" "0" \
  "exact-pinned jaraco client exit code"
m214_exact_row "plan214-irc-welcome" "irc-registration-client-welcome" "1" \
  "client observed the welcome numeric"
m214_exact_row "plan214-irc-target-nick" "irc-registration-target-nick-observed" "1" \
  "target observed NICK ${IRC_NICK}"
m214_exact_row "plan214-irc-target-user" "irc-registration-target-user-observed" "1" \
  "target observed the USER registration"
m214_exact_row "plan214-irc-privacy-rewrite" "irc-privacy-rewrite-derived-from-target" "1" \
  "privacy rewrite derived from target USER booleans"
m214_exact_row "plan214-irc-ping-issued" "irc-ping-issued-by-target" "1" \
  "target issued the PING challenge"
m214_exact_row "plan214-irc-pong-observed" "irc-pong-observed-by-target" "1" \
  "target observed the matching PONG"
m214_exact_row "plan214-irc-privmsg-target" "irc-outbound-privmsg-target-observed" "1" \
  "target observed the token PRIVMSG"
m214_exact_row "plan214-irc-privmsg-client" "irc-inbound-privmsg-client-observed" "1" \
  "client observed the echo reply"
m214_exact_row "plan214-irc-privmsg-token" "irc-privmsg-token-match" "1" \
  "session token matched on both sides"
m214_exact_row "plan214-irc-action" "irc-action-result" "allowed-observed" \
  "target observed the allowed ACTION"
m214_exact_row "plan214-irc-dcc-attempted" "irc-dcc-attempted" "1" \
  "DCC policy case explicitly attempted"
m214_exact_row "plan214-irc-dcc-policy" "irc-dcc-policy-result" "blocked" \
  "DCC policy blocked the forbidden payload"
m214_exact_row "plan214-irc-dcc-target-zero" "irc-dcc-target-observation-delta-zero" "1" \
  "forbidden DCC payload never reached the target"
m214_exact_row "plan214-irc-destination-distinct" "irc-destination-distinct-from-http" "1" \
  "IRC destination independently owned from HTTP"
m214_exact_row "plan214-irc-startup-hash-match" "irc-startup-destination-hash-match" "1" \
  "startup summary addresses the exact IRC destination"
m214_positive_row "plan214-irc-startup-lease-count" "irc-startup-lease-count" \
  "startup installed real leases for the IRC service"
m214_exact_row "plan214-irc-startup-owner" "irc-startup-inbound-owner-registered" "1" \
  "startup registered the IRC inbound owner"
m214_positive_row "plan214-irc-remote-outbound-delta" "irc-remote-outbound-composed-delta" \
  "IRC window remote outbound composed delta"
m214_positive_row "plan214-irc-router-delivery-delta" "irc-router-delivery-delta" \
  "IRC window router delivery delta"
m214_positive_row "plan214-irc-remote-inbound-delta" "irc-remote-inbound-dispatched-delta" \
  "IRC window remote inbound dispatched delta"
m214_exact_row "plan214-irc-local-coowned-zero" "irc-local-coowned-delta-zero" "1" \
  "IRC window zero local co-owned substitution"
m214_exact_row "plan214-irc-unknown-peer-zero" "irc-unknown-peer-delta-zero" "1" \
  "IRC window zero unknown-peer outcomes"
m214_exact_row "plan214-irc-orphan-zero" "irc-orphan-receive-delta-zero" "1" \
  "IRC window zero orphan receives"

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
ref_row "plan214-reference-outbound-accepted" "transit-endpoint-created" \
  "reference transit log proves the outbound build was accepted"
ref_row "plan214-reference-inbound-accepted" "transit-gateway-created" \
  "reference transit log proves the inbound build was accepted"
ref_row "plan214-reference-ls2-stored" "reference-leaseset-updated" \
  "reference floodfill stored a LeaseSet2"
ref_row "plan214-reference-streaming-accepted" "reference-streaming-accepted" \
  "reference Streaming log proves a SYN was accepted"

# ---- 13/14. aggregate rows (Plan 214 §16/§17) -------------------------------
http_agg_rc=0
for label in plan214-http-get-command-exit plan214-http-get-status \
    plan214-http-get-response plan214-http-get-fixture-method \
    plan214-http-get-fixture-path plan214-http-post-digest-equality \
    plan214-http-post-response plan214-http-post-command-exit \
    plan214-http-large-response plan214-http-large-expected-len \
    plan214-http-clearnet-rejected plan214-http-clearnet-target-zero \
    plan214-http-ip-rejected plan214-http-ip-target-zero \
    plan214-http-startup-hash-match plan214-http-startup-lease-count \
    plan214-http-startup-owner plan214-http-remote-outbound-delta \
    plan214-http-router-delivery-delta plan214-http-remote-inbound-delta \
    plan214-http-local-coowned-zero plan214-http-unknown-peer-zero \
    plan214-http-orphan-zero; do
  if ! grep -q "^${label}	passed" "${RESULTS_FILE}"; then
    http_agg_rc=1
    break
  fi
done
record_guarded "plan214-remote-http-eepsite" \
  "aggregate HTTP eepsite row from command + target + counter evidence" \
  "${http_agg_rc}"

irc_agg_rc=0
for label in plan214-irc-command-exit plan214-irc-welcome \
    plan214-irc-target-nick plan214-irc-target-user \
    plan214-irc-privacy-rewrite plan214-irc-ping-issued \
    plan214-irc-pong-observed plan214-irc-privmsg-target \
    plan214-irc-privmsg-client plan214-irc-privmsg-token \
    plan214-irc-action plan214-irc-dcc-attempted plan214-irc-dcc-policy \
    plan214-irc-dcc-target-zero plan214-irc-destination-distinct \
    plan214-irc-startup-hash-match plan214-irc-startup-lease-count \
    plan214-irc-startup-owner plan214-irc-remote-outbound-delta \
    plan214-irc-router-delivery-delta plan214-irc-remote-inbound-delta \
    plan214-irc-local-coowned-zero plan214-irc-unknown-peer-zero \
    plan214-irc-orphan-zero; do
  if ! grep -q "^${label}	passed" "${RESULTS_FILE}"; then
    irc_agg_rc=1
    break
  fi
done
record_guarded "plan214-remote-irc-service" \
  "aggregate IRC service row from command + target + counter evidence" \
  "${irc_agg_rc}"

# ---- 16. cleanup + resource baseline ----------------------------------------
echo "==> resource baseline"
# Plan 215 §5 — every supervised child is signalled exactly once
# with TERM (process group, falling back to direct PID); the
# Python fixtures (started under setsid above) and i2pd get a
# bounded grace window before the KILL fallback. The previous
# `wait $pid` block was racy because CPython's accept() syscall
# auto-restarts on EINTR for the default SIGTERM handler, which
# kept the runner wedged inside wait() long after the fixture
# ignored the signal. The bounded grace + KILL fallback replaces
# that indefinite wait with a deterministic, fail-closed reap.
KILL_GRACE_ITERS=40
REMAINING_PIDS=()
for pid in "${CHILD_PIDS[@]:-}"; do
  [[ -z "${pid}" ]] && continue
  stop_group "${pid}"
  REMAINING_PIDS+=("${pid}")
done
CHILD_PIDS=()
for pid in "${REMAINING_PIDS[@]:-}"; do
  alive=1
  for _ in $(seq 1 "${KILL_GRACE_ITERS}"); do
    if ! kill -0 "${pid}" 2>/dev/null; then
      alive=0
      break
    fi
    sleep 0.1
  done
  if [[ "${alive}" -eq 1 ]]; then
    kill -KILL -- "-${pid}" 2>/dev/null || kill -KILL "${pid}" 2>/dev/null || true
    # Bounded reap of the KILL'd child; SIGKILL is uncatchable so
    # the kernel will deliver the exit promptly.
    for _ in $(seq 1 20); do
      kill -0 "${pid}" 2>/dev/null || break
      sleep 0.1
    done
  fi
done
# Bounded settle: any session that briefly held the loopback
# port (TIME_WAIT, SO_REUSEADDR rebind by an unrelated checker)
# must drain before the port probes.
for _ in $(seq 1 20); do
  if ! pgrep -f "http_fixture.*--facts[= ]${HTTP_FACTS}" >/dev/null 2>&1 \
     && ! pgrep -f "irc_fixture.*--facts[= ]${IRC_FACTS}" >/dev/null 2>&1 \
     && ! pgrep -f "i2pd.*--datadir=${I2PD_DATA}" >/dev/null 2>&1; then
    break
  fi
  sleep 0.1
done
resource_rc=0
# Plan 215 §5 — narrow the pgrep patterns to this run's scratch
# fixtures (matched by `--facts ${HTTP_FACTS}`) and the ephemeral
# i2pd (matched by `--datadir ${I2PD_DATA}`). A broad `pgrep -f
# http_fixture` would also match the run-independent.sh's local
# lane fixtures when this runner is invoked via delegation, since
# those fixtures are started by the harness without setsid and
# live until the harness's own CHILD_PIDS sweep.
if pgrep -f "http_fixture.*--facts[= ]${HTTP_FACTS}" >/dev/null 2>&1; then
  echo "http fixture process survived shutdown:" >&2
  pgrep -af "http_fixture.*--facts[= ]${HTTP_FACTS}" >&2 || true
  resource_rc=1
fi
if pgrep -f "irc_fixture.*--facts[= ]${IRC_FACTS}" >/dev/null 2>&1; then
  echo "irc fixture process survived shutdown:" >&2
  pgrep -af "irc_fixture.*--facts[= ]${IRC_FACTS}" >&2 || true
  resource_rc=1
fi
# Plan 215 §5 — i2pd is started with `--datadir=${I2PD_DATA}` (with
# the trailing `/data` segment) so the pgrep pattern must match the
# actual command line, not the `${SCRATCH}/i2pd` directory.
if pgrep -f "i2pd.*--datadir=${I2PD_DATA}" >/dev/null 2>&1; then
  echo "ephemeral i2pd process survived shutdown:" >&2
  pgrep -af "i2pd.*--datadir=${I2PD_DATA}" >&2 || true
  resource_rc=1
fi
if (echo > "/dev/tcp/127.0.0.1/${HTTP_TARGET}") 2>/dev/null; then
  echo "http fixture port still accepts after shutdown" >&2
  resource_rc=1
fi
if (echo > "/dev/tcp/127.0.0.1/${IRC_TARGET}") 2>/dev/null; then
  echo "irc fixture port still accepts after shutdown" >&2
  resource_rc=1
fi
record_guarded "plan214-clean-resource-baseline" \
  "fixtures/i2pd shutdown leaves no process and loopback ports refused" \
  "${resource_rc}"
# Cleanup must kill children even when a case timed out (Plan 214
# §19.20): the trap already stopped every CHILD_PID, and the two
# fixture ports above prove the kill landed.
record_guarded "plan214-cleanup-kills-on-timeout" \
  "trap stopped all children; fixture ports refuse after shutdown" \
  "${resource_rc}"

# ---- no-secret audit ---------------------------------------------------------
secret_rc=0
# `PRIV` alone also matches the documented `irc-privacy-*` row
# labels, so the marker check targets actual key-material shapes
# (`private key` / `PRIV=`); overlong opaque tokens and `.dat`
# files are rejected separately below.
if grep -rEi 'private.?key|PRIV=' "${EVIDENCE_DIR}" >/dev/null 2>&1; then
  echo "evidence may carry private material markers" >&2
  grep -rEi 'private.?key|PRIV=' "${EVIDENCE_DIR}" | head -5 >&2 || true
  secret_rc=1
fi
if grep -rEi 'seed' "${EVIDENCE_DIR}" 2>/dev/null | grep -vi 'reseed' | head -1 | grep -q .; then
  echo "evidence may carry private material markers" >&2
  grep -rEi 'seed' "${EVIDENCE_DIR}" 2>/dev/null | grep -vi 'reseed' | head -5 >&2 || true
  secret_rc=1
fi
if [[ -f "${DRIVER_TSV}" ]]; then
  if awk -F'\t' '{for (i=1; i<=NF; i++) if (length($i) > 400) exit 1}' \
      "${DRIVER_TSV}" 2>/dev/null; then
    :
  else
    echo "evidence carries an overlong token (possible key material)" >&2
    secret_rc=1
  fi
else
  echo "counted driver produced no evidence TSV" >&2
  secret_rc=1
fi
# Private .dat files must never be copied into evidence.
if find "${EVIDENCE_DIR}" -name '*.dat' 2>/dev/null | grep -q .; then
  echo "evidence contains a private .dat file" >&2
  secret_rc=1
fi
record_guarded "plan214-no-secret-leak" \
  "evidence carries no private markers, no overlong tokens, no .dat files" \
  "${secret_rc}"

# ---- 15. exactly one terminal classification (Plan 214 §18) -----------------
row_failed() {
  grep -q "^$1	failed" "${RESULTS_FILE}"
}
terminal="P214-N-passed"
if row_failed "plan214-prerequisite-plan213"; then terminal="P214-A-prerequisite-plan213-not-green";
elif row_failed "plan214-i2pd-pin-sha" || row_failed "plan214-i2pd-version" ||
     row_failed "plan214-i2pd-cache-clean" || row_failed "plan214-jaraco-pin-sha" ||
     row_failed "plan214-jaraco-cache-clean" || row_failed "plan214-reference-tunnel-config-sanity" ||
     row_failed "plan214-reference-router-ready"; then terminal="P214-B-reference-startup-or-pin";
elif row_failed "plan214-public-destination-extraction" || row_failed "plan214-server-ls2-published"; then terminal="P214-C-public-destination-extraction";
elif row_failed "plan214-remote-http-eepsite" && row_failed "plan214-remote-irc-service"; then
  if grep -q "remote-stop" "${DRIVER_TSV}" 2>/dev/null &&
     grep -q "service-product-start" "${DRIVER_TSV}" 2>/dev/null; then terminal="P214-D-service-product-start";
  else terminal="P214-G-http-return-path"; fi
elif row_failed "plan214-http-startup-hash-match" || row_failed "plan214-http-startup-lease-count" ||
     row_failed "plan214-http-startup-owner"; then terminal="P214-E-http-target-resolution";
elif row_failed "plan214-http-remote-outbound-delta" || row_failed "plan214-http-router-delivery-delta"; then terminal="P214-F-http-request-outbound";
elif row_failed "plan214-http-clearnet-rejected" || row_failed "plan214-http-clearnet-target-zero" ||
     row_failed "plan214-http-ip-rejected" || row_failed "plan214-http-ip-target-zero" ||
     row_failed "plan214-fixture-startup"; then terminal="P214-H-http-policy-or-fixture-integrity";
elif row_failed "plan214-remote-http-eepsite"; then terminal="P214-G-http-return-path";
elif row_failed "plan214-irc-startup-hash-match" || row_failed "plan214-irc-startup-lease-count" ||
     row_failed "plan214-irc-startup-owner"; then terminal="P214-I-irc-target-resolution";
elif row_failed "plan214-irc-welcome" || row_failed "plan214-irc-target-nick" ||
     row_failed "plan214-irc-target-user" || row_failed "plan214-irc-command-exit"; then terminal="P214-J-irc-registration";
elif row_failed "plan214-irc-privacy-rewrite" || row_failed "plan214-irc-dcc-attempted" ||
     row_failed "plan214-irc-dcc-policy" || row_failed "plan214-irc-dcc-target-zero" ||
     row_failed "plan214-irc-action"; then terminal="P214-L-irc-privacy-policy";
elif row_failed "plan214-remote-irc-service"; then terminal="P214-K-irc-return-path";
elif row_failed "plan214-clean-resource-baseline" || row_failed "plan214-cleanup-kills-on-timeout" ||
     row_failed "plan214-no-secret-leak" || row_failed "plan214-static-checker" ||
     row_failed "plan214-unit-floor"; then terminal="P214-M-resource-or-evidence-integrity";
fi
if [[ "${terminal}" == "P214-N-passed" ]]; then
  # A pass classification additionally requires every recorded row
  # to be passed (no silent failed row outside the mapping).
  if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
    terminal="P214-M-resource-or-evidence-integrity"
  fi
fi
terminal_rc=0
[[ "${terminal}" == "P214-N-passed" ]] || terminal_rc=1
record_guarded "plan214-terminal-classification" \
  "exactly one terminal class ${terminal}" \
  "${terminal_rc}"

# ---- evidence.json -------------------------------------------------------------
"${PYTHON_BIN}" - "${RESULTS_FILE}" "${EVIDENCE_DIR}" "${REPO_ROOT}" "${I2PD_PIN}" "${I2PD_VERSION}" "${JARACO_PIN}" "${SOURCE_HEAD}" <<'PY'
import json
import platform
import subprocess
import sys
import time
from pathlib import Path

results_path, evidence_dir, repo_root = sys.argv[1:4]
i2pd_pin, i2pd_version, jaraco_pin, source_head = sys.argv[4:8]
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
driver_tsv = Path(evidence_dir) / "plan214-driver" / "driver-evidence.tsv"
if driver_tsv.exists():
    for line in driver_tsv.read_text(encoding="utf-8").splitlines():
        driver_keys.append(line.split("\t", 1)[0])
terminal = [row for row in rows if row["label"] == "plan214-terminal-classification"]
evidence = {
    "schema": "i2pr-plan214-applications-v1",
    "timestamp_utc": time.strftime("%Y-%m-%dT%H:%M:%SZ", time.gmtime()),
    "i2pr_commit": commit,
    "source_head": source_head,
    "os_image": platform.platform(),
    "rust_toolchain": rustc,
    "execution_lane": "plan214-applications-external",
    "ssu2_bind_policy": "127.0.0.1 loopback only, advertise=false, no introducer",
    "i2pd": {
        "repository": "https://github.com/PurpleI2P/i2pd.git",
        "revision": i2pd_pin,
        "version": i2pd_version,
        "role": "mandatory independent HTTP/IRC reference, unmodified",
        "transit": "notransit=false, floodfill=true, SAM loopback (Plan 214 application qualification only)",
    },
    "jaraco_irc": {
        "repository": "https://github.com/jaraco/irc.git",
        "revision": jaraco_pin,
        "role": "independent ordinary IRC client implementation, unmodified",
    },
    "driver_evidence_keys": driver_keys,
    "terminal_classification": terminal[0]["detail"] if len(terminal) == 1 else f"invalid-count-{len(terminal)}",
    "results": rows,
    "plan214_applications": "passed" if all(
        row["status"] == "passed" for row in rows
    ) else "failed",
    "known_limitations": [
        "HTTP/IRC application qualification only; generic transport rows belong to Plan 213",
        "one-hop service tunnels only; no multi-hop tunnel build",
        "loopback-only i2pd reference; no public I2P participation",
        "digest evidence only; no payload bytes or key material in evidence",
    ],
}
out = Path(evidence_dir)
(out / "evidence.json").write_text(json.dumps(evidence, indent=2, sort_keys=True) + "\n")
with (out / "evidence.md").open("w", encoding="utf-8") as stream:
    stream.write("# Plan 214 M10 HTTP/IRC product-only external requalification evidence\n\n")
    stream.write(f"- i2pr commit: `{commit}`\n")
    stream.write(f"- i2pd: `{i2pd_version}` @ `{i2pd_pin}` (unmodified)\n")
    stream.write(f"- jaraco/irc: `{jaraco_pin}` (unmodified)\n")
    stream.write(f"- OS/image: `{platform.platform()}`\n")
    stream.write(f"- Rust: `{rustc}`\n")
    stream.write("- Bind policy: `127.0.0.1` only, `advertise=false`, no introducer\n\n")
    stream.write("| Result | Status | Detail |\n| --- | --- | --- |\n")
    for row in rows:
        stream.write(f"| {row['label']} | {row['status']} | {row['detail']} |\n")
PY

cp "${RESULTS_FILE}" "${EVIDENCE_DIR}/results.tsv"
if [[ "${REQUIRED_FAILED}" -ne 0 ]]; then
  echo "Plan 214 application requalification lane failed; sanitized evidence: ${EVIDENCE_DIR}" >&2
  exit 1
fi
echo "Plan 214 application requalification lane passed; sanitized evidence: ${EVIDENCE_DIR}"
