#!/usr/bin/env bash
# Plan 381 §WP1 — the cheap gate for the ELS2 `tunnels.conf` writer/validator.
#
# This is the part of Plan 381 that needs **no i2pd process at all**. It is
# deliberately first in the work-package order for that reason: it proves the
# configuration grammar that WP2/WP3 will depend on, before any expensive
# external lane exists to be confused by a wrong key spelling.
#
# What it pins:
#
#   1. the closed auth-mode vocabulary and the mode -> key-group pairing that
#      `libi2pd/Destination.cpp:1088-1091` forces;
#   2. that the writer emits a value carrying the `:` separator
#      `libi2pd/Destination.cpp:1612-1620` requires, in every mode that needs a
#      key -- the failure that is accepted by the parser, dropped without a
#      diagnostic, and later mistaken for a crypto defect;
#   3. that the validator accepts the writer's own output;
#   4. that the validator fails closed on each documented violation, including
#      an unknown key, which i2pd itself would silently ignore.
#
# The writer/validator under test are sourced from `els2-tunnels-conf.sh`, the
# same file the lane runner sources, so the two cannot disagree.

set -uo pipefail

HERE="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
# shellcheck source=tests/integration/els2/els2-tunnels-conf.sh
source "${HERE}/els2-tunnels-conf.sh"

FAILURES=0
ok()   { echo "  ok: $1"; }
fail() { echo "  FAIL: $1"; FAILURES=$((FAILURES + 1)); }

expect_pass() {
  local label="$1"; shift
  if validate_els2_tunnels_conf "$@" 2>/dev/null; then
    ok "${label}"
  else
    fail "${label} -- validator rejected a valid config"
    validate_els2_tunnels_conf "$@" 2>&1 | sed 's/^/      /' || true
  fi
}

expect_fail() {
  local label="$1"; shift
  if validate_els2_tunnels_conf "$@" 2>/dev/null; then
    fail "${label} -- validator accepted a broken config"
  else
    ok "${label}"
  fi
}

SCRATCH="$(mktemp -d -t i2pr-plan381-conf.XXXXXX)"
trap 'rm -rf "${SCRATCH}"' EXIT

PSK_HEX="$(printf '%064x' 1)"
DH_HEX="$(printf '%064x' 2)"

echo "== 1. the closed auth-mode vocabulary =="

if [[ "${ELS2_AUTH_NONE}" == "0" && "${ELS2_AUTH_DH}" == "1" && "${ELS2_AUTH_PSK}" == "2" ]]; then
  ok "auth modes are NONE=0, DH=1, PSK=2 (libi2pd/LeaseSet.h:290-292)"
else
  fail "auth mode constants drifted from the reference enum"
fi

if [[ "${ELS2_STORE_TYPE_ENCRYPTED}" == "5" ]]; then
  ok "encrypted LeaseSet2 store type is 5"
else
  fail "encrypted store type must be 5"
fi

# The mode -> group pairing is the reference's choice, not ours. If i2pd ever
# stopped pairing by auth type this test would need revisiting; today it is
# forced by Destination.cpp:1088-1091.
if [[ "$(els2_group_for_auth_type "${ELS2_AUTH_DH}")" == "i2cp.leaseSetClient.dh" ]] &&
   [[ "$(els2_group_for_auth_type "${ELS2_AUTH_PSK}")" == "i2cp.leaseSetClient.psk" ]] &&
   ! els2_group_for_auth_type "${ELS2_AUTH_NONE}" >/dev/null 2>&1; then
  ok "DH reads .dh, PSK reads .psk, and NONE reads no group"
else
  fail "mode -> key-group pairing does not match the reference"
fi

echo "== 2. the writer's output for every mode =="

# NONE needs no credential and must stay working -- this is Plan 380
# invariant 1, asserted on the reference side of the seam.
CONF_NONE="${SCRATCH}/none.conf"
write_els2_tunnels_conf "${CONF_NONE}" "ELS2-None" 18080 "none.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_NONE}" || fail "writer failed for NONE"

if grep -qE '^i2cp\.leaseSetClient\.' "${CONF_NONE}"; then
  fail "NONE mode emitted a client key line"
else
  ok "NONE mode emits no client key line"
fi

for mode in PSK DH; do
  if [[ "${mode}" == "PSK" ]]; then at="${ELS2_AUTH_PSK}"; hex="${PSK_HEX}"; grp=i2cp.leaseSetClient.psk
  else at="${ELS2_AUTH_DH}"; hex="${DH_HEX}"; grp=i2cp.leaseSetClient.dh; fi

  conf="${SCRATCH}/$(echo "${mode}" | tr 'A-Z' 'a-z').conf"
  write_els2_tunnels_conf "${conf}" "ELS2-${mode}" 18080 "$(echo "${mode}" | tr 'A-Z' 'a-z').dat" \
    "${ELS2_STORE_TYPE_ENCRYPTED}" "${at}" "${hex}" || fail "writer failed for ${mode}"

  line="$(grep -E "^${grp}\.0[[:space:]]*=" "${conf}" || true)"
  if [[ -z "${line}" ]]; then
    fail "${mode}: writer did not emit ${grp}.0"
    continue
  fi
  ok "${mode}: writer emits ${grp}.0"

  # The colon. This is the row the whole lane would otherwise fail on.
  value="${line#*=}"
  value="$(printf '%s' "${value}" | sed 's/^[[:space:]]*//;s/[[:space:]]*$//')"
  case "${value}" in
    *:*) ok "${mode}: value carries the ':' separator i2pd requires" ;;
    *)   fail "${mode}: value has no ':' -- i2pd would drop this entry silently" ;;
  esac

  decoded="$(printf '%s' "${value#*:}" | base64 -d 2>/dev/null | wc -c | tr -d ' ')"
  if [[ "${decoded}" == "32" ]]; then
    ok "${mode}: value decodes to a 32-byte auth key"
  else
    fail "${mode}: value decodes to ${decoded} bytes, expected 32"
  fi

  # And the bytes are the ones that went in, not merely 32 of something.
  if [[ "$(printf '%s' "${value#*:}" | base64 -d 2>/dev/null | xxd -p -c 64)" == "${hex}" ]]; then
    ok "${mode}: key material round-trips through the encoding unchanged"
  else
    fail "${mode}: key material was altered by the encoding"
  fi

  # Never both groups in one section.
  other=i2cp.leaseSetClient.dh
  [[ "${grp}" == "${other}" ]] && other=i2cp.leaseSetClient.psk
  if grep -qE "^${other}(\.0)?[[:space:]]*=" "${conf}"; then
    fail "${mode}: writer also emitted ${other}"
  else
    ok "${mode}: writer emits only its own group"
  fi
done

echo "== 3. the validator accepts the writer's own output =="

expect_pass "accepts NONE" "${CONF_NONE}" "ELS2-None" 18080 "none.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_NONE}"
expect_pass "accepts PSK" "${SCRATCH}/psk.conf" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" "${PSK_HEX}"
expect_pass "accepts DH" "${SCRATCH}/dh.conf" "ELS2-DH" 18080 "dh.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_DH}" "${DH_HEX}"

# The standard LeaseSet2 store type backs the `.b32.i2p` authority row, so the
# same writer must cover it.
CONF_B32="${SCRATCH}/b32.conf"
write_els2_tunnels_conf "${CONF_B32}" "ELS2-B32" 18080 "b32.dat" \
  "${ELS2_STORE_TYPE_STANDARD}" "${ELS2_AUTH_NONE}" || fail "writer failed for store type 3"
expect_pass "accepts store type 3 with auth NONE" "${CONF_B32}" "ELS2-B32" 18080 "b32.dat" \
  "${ELS2_STORE_TYPE_STANDARD}" "${ELS2_AUTH_NONE}"

# The reader is a prefix match, so the reference accepts both the bare and the
# indexed spelling of a client key (ClientContext.cpp:465-473). The lane emits
# the indexed form, but a validator that refused the bare form would be
# rejecting something the reference honours, so both must validate.
BAREKEY="${SCRATCH}/barekey.conf"
sed 's|^i2cp\.leaseSetClient\.psk\.0 = 0:|i2cp.leaseSetClient.psk = 0:|' \
  "${SCRATCH}/psk.conf" > "${BAREKEY}"
expect_pass "accepts the bare client-key spelling i2pd also honours" \
  "${BAREKEY}" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" "${PSK_HEX}"

echo "== 4. the validator fails closed =="

expect_fail "rejects a missing file" "${SCRATCH}/absent.conf" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" "${PSK_HEX}"

expect_fail "rejects a wrong store type" "${SCRATCH}/store.conf" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_STANDARD}" "${ELS2_AUTH_PSK}" "${PSK_HEX}"

expect_fail "rejects a wrong auth type" "${SCRATCH}/auth.conf" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_DH}" "${DH_HEX}"

expect_fail "rejects a missing client key for an authenticated mode" \
  "${SCRATCH}/nokey.conf" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}"

# The colon-less value is the reference's silent-drop trap. The validator must
# catch what i2pd does not.
NOCOLON="${SCRATCH}/nocolon.conf"
sed 's|^i2cp\.leaseSetClient\.psk\.0 = 0:|i2cp.leaseSetClient.psk.0 = |' \
  "${SCRATCH}/psk.conf" > "${NOCOLON}"
expect_fail "rejects a colon-less client key value" "${NOCOLON}" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" "${PSK_HEX}"

SHORTKEY="${SCRATCH}/shortkey.conf"
sed 's|^i2cp\.leaseSetClient\.psk\.0 = 0:.*|i2cp.leaseSetClient.psk.0 = 0:YWJjZA==|' \
  "${SCRATCH}/psk.conf" > "${SHORTKEY}"
expect_fail "rejects a short auth key" "${SHORTKEY}" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" "${PSK_HEX}"

BOTH="${SCRATCH}/both.conf"
{ cat "${SCRATCH}/psk.conf"; printf 'i2cp.leaseSetClient.dh.0 = 0:%s\n' \
    "$(printf '%s' "${DH_HEX}" | xxd -r -p | base64 -w0)"; } > "${BOTH}"
expect_fail "rejects both client key groups in one section" "${BOTH}" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" "${PSK_HEX}"

NONEKEY="${SCRATCH}/nonekey.conf"
{ cat "${CONF_NONE}"; printf 'i2cp.leaseSetClient.psk.0 = 0:%s\n' \
    "$(printf '%s' "${PSK_HEX}" | xxd -r -p | base64 -w0)"; } > "${NONEKEY}"
expect_fail "rejects a client key under auth NONE" "${NONEKEY}" "ELS2-None" 18080 "none.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_NONE}"

# An unknown key is what i2pd ignores and a mistyped known key is what it also
# ignores. Both must be refused here.
TYPO="${SCRATCH}/typo.conf"
sed 's|^i2cp\.leaseSetType = |i2cp.leaseSetTypo = |' "${SCRATCH}/psk.conf" > "${TYPO}"
expect_fail "rejects a misspelled i2cp key" "${TYPO}" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" "${PSK_HEX}"

EXTRA="${SCRATCH}/extra.conf"
{ cat "${SCRATCH}/psk.conf"; printf 'inbound.nickname = extra\n'; } >> "${EXTRA}"
expect_fail "rejects an unknown section key" "${EXTRA}" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" "${PSK_HEX}"

SECOND="${SCRATCH}/second.conf"
{ cat "${SCRATCH}/psk.conf"; printf '\n[ELS2-Other]\ntype = server\nport = 19090\n'; } >> "${SECOND}"
expect_fail "rejects a second section" "${SECOND}" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" "${PSK_HEX}"

ZERO="${SCRATCH}/zero.conf"
sed 's|^inbound\.length = 0$|inbound.length = 3|' "${SCRATCH}/psk.conf" > "${ZERO}"
expect_fail "rejects a non-zero inbound length" "${ZERO}" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" "${PSK_HEX}"

PH="${SCRATCH}/placeholder.conf"
sed 's|^port = 18080$|port = ${APP_PORT}|' "${SCRATCH}/psk.conf" > "${PH}"
expect_fail "rejects an unresolved template place-holder" "${PH}" "ELS2-PSK" 18080 "psk.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" "${PSK_HEX}"

echo "== 5. the writer refuses to emit an inconsistent section =="

if write_els2_tunnels_conf "${SCRATCH}/refuse1.conf" "X" 18080 "x.dat" \
     "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" 2>/dev/null; then
  fail "writer accepted an authenticated mode with no key"
else
  ok "writer refuses an authenticated mode with no key"
fi

if write_els2_tunnels_conf "${SCRATCH}/refuse2.conf" "X" 18080 "x.dat" \
     "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_NONE}" "${PSK_HEX}" 2>/dev/null; then
  fail "writer accepted a key under auth NONE"
else
  ok "writer refuses a key under auth NONE"
fi

if write_els2_tunnels_conf "${SCRATCH}/refuse3.conf" "X" 18080 "x.dat" \
     "${ELS2_STORE_TYPE_ENCRYPTED}" "7" "${PSK_HEX}" 2>/dev/null; then
  fail "writer accepted an out-of-vocabulary auth type"
else
  ok "writer refuses an out-of-vocabulary auth type"
fi

if write_els2_tunnels_conf "${SCRATCH}/refuse4.conf" "X" 18080 "x.dat" \
     "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_PSK}" "abcd" 2>/dev/null; then
  fail "writer accepted a short key"
else
  ok "writer refuses a short key"
fi

echo "== 6. the writer/validator refuse argument shapes i2pd cannot use =="

# Plan 381 WP2 found these by executing the lane: the first probe passed
# `ELS2PROBE.dat` as the port and an absolute path as the keys file, and the
# validator returned success. Both comparisons it makes are "the file says X
# and the caller said X", so a wrong caller agrees with itself. These rows
# assert the *shape* of each argument, not its agreement with the file.

for bad_port in "0" "65536" "99999" "-1" "18080x" "" "180 80" "http"; do
  if write_els2_tunnels_conf "${SCRATCH}/bp.conf" "X" "${bad_port}" "x.dat" \
       "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_NONE}" 2>/dev/null; then
    fail "writer accepted port '${bad_port}'"
  else
    ok "writer refuses port '${bad_port}'"
  fi
done

for bad_keys in "/tmp/x/x.dat" "./x.dat" "sub/x.dat" ".." "." "" "a b.dat" "x.dat/" "x\$y.dat"; do
  if write_els2_tunnels_conf "${SCRATCH}/bk.conf" "X" 18080 "${bad_keys}" \
       "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_NONE}" 2>/dev/null; then
    fail "writer accepted keys '${bad_keys}'"
  else
    ok "writer refuses keys '${bad_keys}'"
  fi
done

# The same two shapes, through the validator, including the self-agreeing case
# where the caller and the file carry the *same* wrong value.
write_els2_tunnels_conf "${SCRATCH}/shape-ok.conf" "X" 18080 "x.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_NONE}" || fail "writer failed for the shape baseline"
expect_pass "accepts a bare keys filename and a numeric port" \
  "${SCRATCH}/shape-ok.conf" "X" 18080 "x.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_NONE}"

ABSPATH="${SCRATCH}/abspath.conf"
sed 's|^keys = x\.dat$|keys = /tmp/scratch/x.dat|' "${SCRATCH}/shape-ok.conf" > "${ABSPATH}"
expect_fail "rejects an absolute keys path even when the caller repeats it" \
  "${ABSPATH}" "X" 18080 "/tmp/scratch/x.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_NONE}"

BADPORT="${SCRATCH}/badport.conf"
sed 's|^port = 18080$|port = ELS2PROBE.dat|' "${SCRATCH}/shape-ok.conf" > "${BADPORT}"
expect_fail "rejects a non-numeric port even when the caller repeats it" \
  "${BADPORT}" "X" "ELS2PROBE.dat" "x.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_NONE}"

OUTOFRANGE="${SCRATCH}/outofrange.conf"
sed 's|^port = 18080$|port = 70000|' "${SCRATCH}/shape-ok.conf" > "${OUTOFRANGE}"
expect_fail "rejects an out-of-range port" \
  "${OUTOFRANGE}" "X" 70000 "x.dat" \
  "${ELS2_STORE_TYPE_ENCRYPTED}" "${ELS2_AUTH_NONE}"

if [[ "${FAILURES}" -ne 0 ]]; then
  echo "FAIL: ${FAILURES} Plan 381 tunnels.conf contract violation(s)" >&2
  exit 1
fi
echo "ok: Plan 381 tunnels.conf writer/validator contract holds"