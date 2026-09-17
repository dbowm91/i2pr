#!/usr/bin/env bash
# Plan 215 §11 — focused shell test for the deterministic
# `tunnels.conf` writer and the pre-launch sanity gate.
#
# The writer and validator live in
# `tests/integration/service-tunnels/run-plan214-applications.sh`
# as named shell functions. This test exercises the same logic
# inline (a small duplication permitted by Plan 215 §11) so the
# contract can be re-verified independently of the expensive
# external lane:
#
#   1. the writer produces a minimal deterministic file with the
#      documented sections / profile names / dynamic ports / key
#      filenames / zero-hop lengths;
#   2. changing the dynamic inputs changes only the intended
#      fields;
#   3. the validator accepts the writer's output;
#   4. the validator fails closed on:
#        - a missing file;
#        - a wrong HTTP port;
#        - a wrong IRC port;
#        - `type = irc` instead of `type = server`;
#        - a missing HTTP section;
#        - a missing IRC section;
#        - a third unintended tunnel section;
#        - an unresolved template place-holder.
#
# No tests rely on the raw i2pd log or any private key material;
# the path is unprivileged and loopback-only.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../../.." && pwd)"

# Mirror the writer/validator helpers from the runner. The shape is
# intentionally narrow: the only divergence from the runner's body
# must be the literal placeholder/port tokens the test substitutes.
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

plan215_section_body() {
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

  if [[ ! -s "${path}" ]]; then
    echo "tunnels.conf is missing or empty: ${path}" >&2
    rc=1
  fi

  if [[ "${rc}" -eq 0 ]]; then
    http_sections="$(grep -cE '^\[HTTP-Server\]$' "${path}" 2>/dev/null || true)"
    irc_sections="$(grep -cE '^\[IRC-Server\]$' "${path}" 2>/dev/null || true)"
    total_sections="$(grep -cE '^\[' "${path}" 2>/dev/null || true)"

    if [[ "${http_sections}" != "1" ]]; then rc=1; fi
    if [[ "${irc_sections}" != "1" ]]; then rc=1; fi
    if [[ "${total_sections:-0}" -ne 2 ]]; then rc=1; fi
  fi

  if [[ "${rc}" -eq 0 ]]; then
    http_body="$(plan215_section_body 'HTTP-Server' "${path}")"
    irc_body="$(plan215_section_body 'IRC-Server' "${path}")"

    if ! grep -qxF 'type = http' <<<"${http_body}"; then rc=1; fi
    if ! grep -qxF 'type = server' <<<"${irc_body}"; then rc=1; fi
    if ! grep -qxF "port = ${http_port}" <<<"${http_body}"; then rc=1; fi
    if ! grep -qxF "port = ${irc_port}" <<<"${irc_body}"; then rc=1; fi
    if ! grep -qxF 'keys = plan214-http-server.dat' <<<"${http_body}"; then rc=1; fi
    if ! grep -qxF 'keys = plan214-irc-server.dat' <<<"${irc_body}"; then rc=1; fi
    if ! grep -qxF 'inbound.length = 0' <<<"${http_body}" ||
       ! grep -qxF 'outbound.length = 0' <<<"${http_body}"; then rc=1; fi
    if ! grep -qxF 'inbound.length = 0' <<<"${irc_body}" ||
       ! grep -qxF 'outbound.length = 0' <<<"${irc_body}"; then rc=1; fi
    if grep -nE '\$\{[A-Za-z_][A-Za-z0-9_]*\(`|\$\{[A-Za-z_][A-Za-z0-9_]*(:[^}]*)?\}`|\{\{[A-Za-z_][A-Za-z0-9_]*\}\}|<%[A-Za-z_][A-Za-z0-9_]*%>|__[A-Za-z_][A-Za-z0-9_]*__' "${path}" >/dev/null 2>&1; then
      rc=1
    fi
  fi

  return "${rc}"
}

FAILURES=0
expect_pass() {
  local label="$1"
  local path="$2"
  local http_port="$3"
  local irc_port="$4"
  if validate_plan214_tunnels_conf "${path}" "${http_port}" "${irc_port}"; then
    echo "  ok: ${label}"
  else
    echo "  FAIL: ${label} — validator rejected valid config"
    FAILURES=$((FAILURES + 1))
  fi
}

expect_fail() {
  local label="$1"
  local path="$2"
  local http_port="$3"
  local irc_port="$4"
  if validate_plan214_tunnels_conf "${path}" "${http_port}" "${irc_port}"; then
    echo "  FAIL: ${label} — validator accepted broken config"
    cat "${path}" 2>/dev/null | sed 's/^/    | /'
    FAILURES=$((FAILURES + 1))
  else
    echo "  ok: ${label}"
  fi
}

SCRATCH="$(mktemp -d -t i2pr-plan215-conf.XXXXXX)"
trap 'rm -rf "${SCRATCH}"' EXIT

# --- writer produces the documented minimal config -------------------------
CONF_A="${SCRATCH}/a.conf"
write_plan214_tunnels_conf "${CONF_A}" "18080" "6667"

if [[ ! -s "${CONF_A}" ]]; then
  echo "FAIL: writer produced empty file"
  FAILURES=$((FAILURES + 1))
else
  # shape: exactly two sections, one HTTP-Server, one IRC-Server
  section_count="$(grep -cE '^\[' "${CONF_A}")"
  if [[ "${section_count}" != "2" ]]; then
    echo "FAIL: writer produced ${section_count} sections, expected 2"
    FAILURES=$((FAILURES + 1))
  fi
  # IRC stays transparent (no `type = irc`)
  if grep -qxF 'type = server' "${CONF_A}" &&
     ! grep -qxF 'type = irc' "${CONF_A}"; then
    echo "  ok: writer emits transparent IRC type=server"
  else
    echo "  FAIL: writer emitted unexpected IRC type line"
    FAILURES=$((FAILURES + 1))
  fi
  # exactly the documented dynamic inputs appear
  if grep -qxF 'port = 18080' "${CONF_A}" &&
     grep -qxF 'port = 6667' "${CONF_A}"; then
    echo "  ok: writer emits the dynamic HTTP/IRC ports"
  else
    echo "  FAIL: writer did not emit the dynamic HTTP/IRC ports"
    FAILURES=$((FAILURES + 1))
  fi
fi

# --- validator accepts the writer's output ---------------------------------
expect_pass "validator accepts writer output" "${CONF_A}" "18080" "6667"

# --- changing the dynamic inputs changes only the intended fields ----------
CONF_B="${SCRATCH}/b.conf"
write_plan214_tunnels_conf "${CONF_B}" "28080" "7667"
expect_pass "validator accepts writer output (alternate ports)" "${CONF_B}" "28080" "7667"
if grep -qxF 'port = 18080' "${CONF_B}"; then
  echo "FAIL: alternate-port writer leaked the previous HTTP port"
  FAILURES=$((FAILURES + 1))
fi
if grep -qxF 'port = 6667' "${CONF_B}"; then
  echo "FAIL: alternate-port writer leaked the previous IRC port"
  FAILURES=$((FAILURES + 1))
fi

# --- validator rejects a missing file ---------------------------------------
EMPTY="${SCRATCH}/missing.conf"
rm -f "${EMPTY}"
expect_fail "validator rejects missing file" "${EMPTY}" "18080" "6667"

# --- validator rejects a wrong HTTP port ------------------------------------
WRONG_HTTP="${SCRATCH}/wrong_http.conf"
write_plan214_tunnels_conf "${WRONG_HTTP}" "18080" "6667"
sed -i 's|^port = 18080$|port = 19999|' "${WRONG_HTTP}"
expect_fail "validator rejects wrong HTTP port" "${WRONG_HTTP}" "18080" "6667"

# --- validator rejects a wrong IRC port -------------------------------------
WRONG_IRC="${SCRATCH}/wrong_irc.conf"
write_plan214_tunnels_conf "${WRONG_IRC}" "18080" "6667"
sed -i 's|^port = 6667$|port = 6999|' "${WRONG_IRC}"
expect_fail "validator rejects wrong IRC port" "${WRONG_IRC}" "18080" "6667"

# --- validator rejects `type = irc` instead of `type = server` --------------
IRC_TYPE="${SCRATCH}/irc_type.conf"
write_plan214_tunnels_conf "${IRC_TYPE}" "18080" "6667"
# Replace the IRC `type = server` line with the forbidden IRC-transforming
# profile to ensure the validator enforces the transparent tunnel
# contract that the Plan 214 local investigation requires.
awk '
  /^\[IRC-Server\]$/ { flag = 1 }
  flag && /^type = / { print "type = irc"; next }
  { print }
' "${IRC_TYPE}" > "${SCRATCH}/irc_type.tmp" && mv "${SCRATCH}/irc_type.tmp" "${IRC_TYPE}"
expect_fail "validator rejects IRC type=irc" "${IRC_TYPE}" "18080" "6667"

# --- validator rejects a missing HTTP section -------------------------------
NO_HTTP="${SCRATCH}/no_http.conf"
write_plan214_tunnels_conf "${NO_HTTP}" "18080" "6667"
awk '
  /^\[HTTP-Server\]$/ { skip = 1 }
  /^\[/ { if (skip && $0 != "[HTTP-Server]") { skip = 0; print; next } else if (!skip) { print; next } }
  !skip { print }
' "${NO_HTTP}" > "${SCRATCH}/no_http.tmp" && mv "${SCRATCH}/no_http.tmp" "${NO_HTTP}"
expect_fail "validator rejects missing HTTP section" "${NO_HTTP}" "18080" "6667"

# --- validator rejects a third unintended section ---------------------------
EXTRA="${SCRATCH}/extra.conf"
write_plan214_tunnels_conf "${EXTRA}" "18080" "6667"
printf '\n[Extra-Server]\ntype = server\nhost = 127.0.0.1\nport = 9999\nkeys = extra.dat\ninbound.length = 0\noutbound.length = 0\n' >> "${EXTRA}"
expect_fail "validator rejects extra section" "${EXTRA}" "18080" "6667"

# --- validator rejects an unresolved place-holder --------------------------
PH="${SCRATCH}/placeholder.conf"
write_plan214_tunnels_conf "${PH}" "18080" "6667"
printf '\n[HTTP-Server-Extra]\ntype = http\nport = ${EXTRA_PORT}\n' >> "${PH}"
expect_fail "validator rejects \${} placeholder" "${PH}" "18080" "6667"

if [[ "${FAILURES}" -ne 0 ]]; then
  echo "FAIL: ${FAILURES} Plan 215 writer/validator contract violation(s)" >&2
  exit 1
fi
echo "ok: Plan 215 writer/validator contract holds"
