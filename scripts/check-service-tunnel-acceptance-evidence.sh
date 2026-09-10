#!/usr/bin/env bash
# Plan 181 §9 — static evidence-integrity check for the M10
# service-tunnel external lane.
#
# Rejects known dangerous bookkeeping in
# tests/integration/service-tunnels/run-independent.sh: required
# acceptance rows recorded `passed` without an executed command
# behind them, in-tree protocol drivers masquerading as independent
# clients, missing pin verification, or a remote-interop claim
# without the qualification attempt. The sanctioned paths are:
#   `record_guarded "<label>" "<detail>" "<rc>"` — records passed
#   only for rc 0 (every §5 local row plus the shared rows);
#   `suite_row "<label>" "<test>" "<detail>"` — records passed only
#   for suite rc 0 plus the row's own captured `test <name> ... ok`
#   line (reconcile/resource rows);
#   `record_blocked "<label>" "<detail>"` — the ONLY sanctioned
#   path for the two remote rows (blocked, never passed).
# A literal `record "<required-label>" passed` line (indented or
# not) means a row was hard-coded and fails this check. `failed`
# literals are fail-closed and permitted only outside required
# rows (required failures must flow through record_guarded).
#
# Guarded labels (Plan 181 §8 minus the two remote rows):
#   m10-prerequisite-plans, m10-tool-pin-verification,
#   m10-foundation-boundary-checks, m10-fixture-startup,
#   m10-local-final-product, reconcile-rollback-local,
#   cross-service-resource-bounds, m10-local-roundtrip-suite,
#   m10-wire-surface-suite, generic-small-independent,
#   generic-large-independent, generic-half-close-independent,
#   generic-siblings-independent, server-identity-restart-stable,
#   curl-http-get, curl-http-post, curl-http-large, curl-http-connect,
#   http-clearnet-rejected, http-unknown-i2p-bounded,
#   curl-socks5-domainname, socks-clearnet-ip-rejected,
#   irc-venv-install, irc-independent-register,
#   irc-independent-message-roundtrip,
#   irc-user-hostname-authenticated-destination, irc-ctcp-policy-local,
#   external-clean-resource-baseline, unsupported-profile-ledger.
#
# Blocked labels (Plan 181 §6.3 stop condition):
#   remote-independent-http-eepsite, remote-independent-irc-service.
#
# Usage: bash scripts/check-service-tunnel-acceptance-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS="${REPO_ROOT}/tests/integration/service-tunnels/run-independent.sh"
FETCH="${REPO_ROOT}/scripts/interop/fetch-service-tunnel-clients.sh"
JARACO_PIN="90e10e690da2c7bf60de21be4e36d24c9ffd7474"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
REMOTE_DRIVER="${REPO_ROOT}/crates/i2pr-daemon/tests/service_tunnels_remote_qualification.rs"

GUARDED=(
  m10-prerequisite-plans
  m10-tool-pin-verification
  m10-foundation-boundary-checks
  m10-fixture-startup
  m10-local-final-product
  reconcile-rollback-local
  cross-service-resource-bounds
  m10-local-roundtrip-suite
  m10-wire-surface-suite
  generic-small-independent
  generic-large-independent
  generic-half-close-independent
  generic-siblings-independent
  server-identity-restart-stable
  curl-http-get
  curl-http-post
  curl-http-large
  curl-http-connect
  http-clearnet-rejected
  http-unknown-i2p-bounded
  curl-socks5-domainname
  socks-clearnet-ip-rejected
  irc-venv-install
  irc-independent-register
  irc-independent-message-roundtrip
  irc-user-hostname-authenticated-destination
  irc-ctcp-policy-local
  external-clean-resource-baseline
  unsupported-profile-ledger
)

BLOCKED=(
  remote-independent-http-eepsite
  remote-independent-irc-service
)

failures=0

if [[ ! -f "${HARNESS}" ]]; then
  echo "evidence check failed: harness missing: ${HARNESS}" >&2
  exit 1
fi
if [[ ! -f "${FETCH}" ]]; then
  echo "evidence check failed: fetch script missing: ${FETCH}" >&2
  exit 1
fi
if [[ ! -f "${REMOTE_DRIVER}" ]]; then
  echo "evidence check failed: remote qualification driver missing: ${REMOTE_DRIVER}" >&2
  exit 1
fi

# 1. No literal unconditional pass records for guarded rows.
for label in "${GUARDED[@]}"; do
  if grep -n -E "^[[:space:]]*record \"${label}\" passed" "${HARNESS}"; then
    echo "evidence check failed: literal passed record for required row '${label}' (must flow through record_guarded/suite_row)" >&2
    failures=$((failures + 1))
  fi
  # 2. Every guarded row must have a command-derived call site,
  # direct (record_guarded) or per-test (suite_row, which wraps
  # record_guarded with the suite rc plus the row's own ok-line).
  # The three stdlib-generic matrix rows compose their label
  # inside the mode loop; see the dedicated composition check
  # below instead of a literal call site.
  case "${label}" in
    generic-large-independent|generic-half-close-independent|generic-siblings-independent)
      continue
      ;;
  esac
  if ! grep -q -E "record_guarded \"${label}\"" "${HARNESS}" &&
     ! grep -q -E "suite_row \"${label}\"" "${HARNESS}"; then
    echo "evidence check failed: required row '${label}' has no record_guarded/suite_row call site" >&2
    failures=$((failures + 1))
  fi
done

# 2b. The generic matrix loop must enumerate all three modes and
# record through the composed record_guarded call site (exit-code
# gate enforced by record_guarded itself).
if ! grep -q -F 'for mode in large halfclose siblings' "${HARNESS}"; then
  echo "evidence check failed: generic matrix loop lost a mode" >&2
  failures=$((failures + 1))
fi
if ! grep -q -E 'record_guarded "generic-\$\{mode\}-independent"' "${HARNESS}"; then
  echo "evidence check failed: generic matrix lost its composed record_guarded call site" >&2
  failures=$((failures + 1))
fi

# 3. Remote rows must be recorded blocked (or failed closed),
# never passed and never through the pass-capable guarded path.
for label in "${BLOCKED[@]}"; do
  if grep -n -E "^[[:space:]]*record(_guarded)? \"${label}\" passed" "${HARNESS}"; then
    echo "evidence check failed: remote row '${label}' claims passed (self-composed must never stand in for independent interop)" >&2
    failures=$((failures + 1))
  fi
  if grep -n -E "record_guarded \"${label}\"" "${HARNESS}"; then
    echo "evidence check failed: remote row '${label}' flows through record_guarded (pass-capable; use record_blocked or explicit failed)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -E "record_blocked \"${label}\"" "${HARNESS}"; then
    echo "evidence check failed: remote row '${label}' has no record_blocked call site" >&2
    failures=$((failures + 1))
  fi
done
# No other row may use the blocked path (no skipping local rows).
if grep -n -E '^[[:space:]]*record_blocked "' "${HARNESS}" |
   grep -v -E 'record_blocked "remote-independent-(http-eepsite|irc-service)"'; then
  echo "evidence check failed: record_blocked used outside the two remote rows" >&2
  failures=$((failures + 1))
fi

# 4. The record_guarded helper itself must gate on the exit code.
if ! grep -q -E 'if \[\[ "\$\{rc\}" -eq 0 \]\]; then' "${HARNESS}"; then
  echo "evidence check failed: record_guarded helper lost its exit-code gate" >&2
  failures=$((failures + 1))
fi

# 5. The suite_row helper must require the suite rc plus the row's
# own ok-line.
if ! grep -q -E 'grep -q "\^test \$\{test_name\} \.\.\. ok\$"' "${HARNESS}"; then
  echo "evidence check failed: suite_row lost its per-test ok-line gate" >&2
  failures=$((failures + 1))
fi

# 6. The exact jaraco/irc pin must be verified before provisioning.
if ! grep -q -F "${JARACO_PIN}" "${HARNESS}"; then
  echo "evidence check failed: harness lost the exact jaraco/irc pin ${JARACO_PIN}" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F "${JARACO_PIN}" "${FETCH}"; then
  echo "evidence check failed: fetch script lost the exact jaraco/irc pin" >&2
  failures=$((failures + 1))
fi
# The venv must install from the verified source checkout, and the
# checkout must be verified clean (no patching).
if ! grep -q -F 'rev-parse HEAD' "${HARNESS}"; then
  echo "evidence check failed: harness lost its jaraco HEAD verification" >&2
  failures=$((failures + 1))
fi
if ! grep -q -E 'status --porcelain' "${HARNESS}"; then
  echo "evidence check failed: harness lost its no-patching cleanliness check" >&2
  failures=$((failures + 1))
fi

# 7. Counted HTTP rows must use unmodified curl (never an in-tree
# raw HTTP driver); counted SOCKS rows must use --socks5-hostname;
# counted IRC rows must use the jaraco public client API.
if ! grep -q -E 'timeout [0-9]+s curl ' "${HARNESS}"; then
  echo "evidence check failed: harness lost its unmodified-curl HTTP invocations" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F -- '--socks5-hostname' "${HARNESS}"; then
  echo "evidence check failed: harness lost hostname-at-proxy SOCKS invocation" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'irc_driver.py' "${HARNESS}"; then
  echo "evidence check failed: harness lost its jaraco/irc driver invocation" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'irc.client' "${REPO_ROOT}/tests/integration/service-tunnels/clients/irc_driver.py"; then
  echo "evidence check failed: IRC driver lost its public irc.client API use" >&2
  failures=$((failures + 1))
fi
# The generic stdlib driver must not speak application protocols:
# protocol keywords would make it a shadow HTTP/SOCKS/IRC client.
if grep -n -E 'GET |POST |CONNECT |NICK|USER |PRIVMSG|SOCKS|220 |EHLO' \
    "${REPO_ROOT}/tests/integration/service-tunnels/clients/generic_driver.py"; then
  echo "evidence check failed: generic driver contains application-protocol bytes (must stay opaque)" >&2
  failures=$((failures + 1))
fi

# 8. The remote qualification must run the ignored driver through
# explicit selection with digest/establishment gating, against the
# exact i2pd pin, with fail-closed missing-environment behavior.
if ! grep -q -F -- '--ignored --exact' "${HARNESS}"; then
  echo "evidence check failed: harness lost the explicit --ignored --exact remote selection" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F "${I2PD_PIN}" "${HARNESS}"; then
  echo "evidence check failed: harness lost the exact i2pd pin ${I2PD_PIN}" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'DEST GENERATE' "${HARNESS}"; then
  echo "evidence check failed: harness lost its independent-destination SAM provenance" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'REMOTE_QUALIFY_UNKNOWN_PEER=' "${HARNESS}"; then
  echo "evidence check failed: harness lost its unknown-peer evidence gate" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'I2PD_PEER_PUB_B64 is absent' "${REMOTE_DRIVER}"; then
  echo "evidence check failed: remote driver lost its fail-closed missing-environment guard" >&2
  failures=$((failures + 1))
fi
# A println!/print!/eprintln! line mentioning the peer PUB
# variable or value would leak key material into logs.
if grep -n -E '(println!|print!|eprintln!)[^;]*(peer_pub_b64|PUB_B64|PUB=)' "${REMOTE_DRIVER}"; then
  echo "evidence check failed: remote driver may log peer key material" >&2
  failures=$((failures + 1))
fi

# 9. The lane must stay loopback-only and must not forgive required
# command failures. The `|| true` forgiveness check ignores the
# kill/wait cleanup helpers and the optional-probe patterns that
# deliberately tolerate absence.
if ! grep -q -F '127.0.0.1' "${HARNESS}"; then
  echo "evidence check failed: harness lost its loopback bind policy" >&2
  failures=$((failures + 1))
fi
if grep -n -E 'cargo test .*(\|\| true)|timeout .*(\|\| true)' "${HARNESS}"; then
  echo "evidence check failed: harness forgives a required command via '|| true'" >&2
  failures=$((failures + 1))
fi

# 10. Evidence must not carry private keys or raw payloads.
if ! grep -q -F 'never copied to' "${HARNESS}"; then
  echo "evidence check failed: harness lost its no-secrets statement" >&2
  failures=$((failures + 1))
fi

# 11. Cleanup/baseline gating must exist.
if ! grep -q -F 'trap cleanup EXIT' "${HARNESS}"; then
  echo "evidence check failed: harness lost its cleanup trap" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'external-clean-resource-baseline' "${HARNESS}"; then
  echo "evidence check failed: harness lost its resource-baseline row" >&2
  failures=$((failures + 1))
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi
echo "service-tunnel acceptance evidence integrity: ${#GUARDED[@]} rows command-derived, ${#BLOCKED[@]} rows blocked, no literal pass records"
