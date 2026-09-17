#!/usr/bin/env bash
# Plan 199 / Plan 181 / Plan 211 — static evidence-integrity check
# for the M10 service-tunnel external lane.
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
#   `record_blocked "<label>" "<detail>"` — the fail-closed path
#   for the two remote rows while the real remote transport is absent.
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
# Blocked labels (Plan 211 execution stop condition):
#   remote-independent-http-eepsite, remote-independent-irc-service.
#
# Plan 211 invariant: the positive M10 remote HTTP + IRC application
# interop driver must exist, must fail closed when the exact-pinned
# i2pd environment is absent, must never log peer key material,
# must not auto-pass via a literal record call site, and must
# build real enabled HttpClient + IrcClient specs (not an empty
# ServiceTunnelSet). The aggregate pass row must derive from the
# documented Plan 211 §10 subfact rows the driver writes to its
# evidence file (real system curl + real exact-pinned jaraco/irc
# public API invocations); synthetic label injection through
# `record_remote_application_observation` or literal aggregate-row
# pass assignments are insufficient and fail the check.
#
# Usage: bash scripts/check-service-tunnel-acceptance-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS="${REPO_ROOT}/tests/integration/service-tunnels/run-independent.sh"
FETCH="${REPO_ROOT}/scripts/interop/fetch-service-tunnel-clients.sh"
JARACO_PIN="90e10e690da2c7bf60de21be4e36d24c9ffd7474"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
REMOTE_DRIVER="${REPO_ROOT}/crates/i2pr-daemon/tests/service_tunnels_remote_qualification.rs"
PLAN202_DRIVER="${REPO_ROOT}/crates/i2pr-daemon/tests/service_tunnels_remote_transport_qualification.rs"
PLAN207_DRIVER="${REPO_ROOT}/crates/i2pr-daemon/tests/service_tunnels_application_genuine_remote_qualification.rs"
PLAN208_DRIVER="${REPO_ROOT}/crates/i2pr-daemon/tests/service_tunnels_remote_route_integration_qualification.rs"
PLAN211_DRIVER="${REPO_ROOT}/crates/i2pr-daemon/tests/service_tunnels_application_product_only_remote_qualification.rs"

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

# Plan 207 — positive remote HTTP + IRC application interop rows.
# The full lane must flow through `record_guarded` and produce the
# documented Plan 207 §9 subfact rows in
# `${EVIDENCE_DIR}/plan207-driver/driver-evidence.tsv`. The static
# checker rejects literal `record "... passed"` lines; the positive
# rows must flow through `record_guarded` and reference the
# command-derived subfacts the runner reads from the driver
# evidence file (real system curl exit codes / body digests /
# status codes + real exact-pinned jaraco/irc public API exit codes
# / PRIVMSG round-trip / CTCP+DCC policy + Plan 206 backend
# counter facts). The Plan 207 driver replaces the synthetic Plan
# 203 `record_remote_application_observation` label-injection
# pattern; the manager's free observation helper is no longer
# sufficient as a counted proof.
PLAN207_POSITIVE=(
  remote-independent-http-eepsite
  remote-independent-irc-service
)

# Plan 202 — positive Direction A row. The lane records `blocked`
# when the full SSU2 endpoint/bind tuple is absent (this M10 lane)
# and `passed` through `record_guarded` when the exact-pinned M6
# interop lane provisions the environment. The static checker
# must allow either path; literal `record "... passed"` lines
# are still rejected (the positive row must flow through
# `record_guarded`).
PLAN202_TRANSITIONAL=(
  m10-remote-destination-streaming-composition
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

# 3. Remote rows must flow through `record_guarded` (positive) or
# `record_blocked` (fail-closed) — never literal `record "...passed"`
# and never a pass-capable literal record without `record_guarded`.
# Plan 207 promotes the two remote rows from Plan 181's
# `blocked-only` shape to a positive-command-derived shape, so
# both `record_blocked` and `record_guarded` call sites are
# required (the row chooses one path based on whether the exact-
# pinned i2pd environment is provisioned).
for label in "${BLOCKED[@]}"; do
  if grep -n -E "^[[:space:]]*record \"${label}\" passed" "${HARNESS}"; then
    echo "evidence check failed: remote row '${label}' claims passed (self-composed must never stand in for independent interop)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -E "record_blocked \"${label}\"" "${HARNESS}"; then
    echo "evidence check failed: remote row '${label}' has no record_blocked call site" >&2
    failures=$((failures + 1))
  fi
done
# Plan 202 — positive Direction A row may use either the blocked
# path (when the SSU2 lane env is absent) or the guarded path (when
# the exact-pinned i2pd cache provisions the SSU2 endpoint + bind
# tuple). Literal `record "...passed"` lines remain forbidden so
# the positive row must flow through `record_guarded`.
for label in "${PLAN202_TRANSITIONAL[@]}"; do
  if grep -n -E "^[[:space:]]*record \"${label}\" passed" "${HARNESS}"; then
    echo "evidence check failed: Plan 202 row '${label}' claims passed literally (must flow through record_guarded)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -E "record_blocked \"${label}\"" "${HARNESS}"; then
    echo "evidence check failed: Plan 202 row '${label}' has no record_blocked call site" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -E "record_guarded \"${label}\"" "${HARNESS}"; then
    echo "evidence check failed: Plan 202 row '${label}' has no record_guarded call site" >&2
    failures=$((failures + 1))
  fi
done
# Plan 207 — positive remote HTTP + IRC application interop rows.
# Both rows must flow through `record_guarded` AND `record_blocked`
# (one path for the missing-env case, one for the positive case).
# Literal `record "...passed"` lines remain forbidden; the positive
# rows must reference the documented Plan 207 §9 evidence keys.
for label in "${PLAN207_POSITIVE[@]}"; do
  if grep -n -E "^[[:space:]]*record \"${label}\" passed" "${HARNESS}"; then
    echo "evidence check failed: Plan 207 row '${label}' claims passed literally (must flow through record_guarded)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -E "record_blocked \"${label}\"" "${HARNESS}"; then
    echo "evidence check failed: Plan 207 row '${label}' has no record_blocked call site" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -E "record_guarded \"${label}\"|^[[:space:]]*record \"${label}\" passed" "${HARNESS}"; then
    echo "evidence check failed: Plan 207 row '${label}' has no record_guarded or post-`record` call site" >&2
    failures=$((failures + 1))
  fi
done
# No other row may use the blocked path (no skipping local rows).
if grep -n -E '^[[:space:]]*record_blocked "' "${HARNESS}" |
   grep -v -E 'record_blocked "remote-independent-(http-eepsite|irc-service)"|record_blocked "m10-remote-destination-streaming-composition"'; then
  echo "evidence check failed: record_blocked used outside the remote rows" >&2
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

# 8. The remote qualification runs its ignored drivers through
# explicit selection with digest/establishment gating, against the
# exact i2pd pin, with fail-closed missing-environment behavior.
# Plan 214 §14 owns the single counted remote application path:
# the explicit `--ignored --exact` selection lives in the Plan 214
# runner (run-independent.sh delegates to it and maps its
# aggregate rows).
PLAN214_RUNNER="${REPO_ROOT}/tests/integration/service-tunnels/run-plan214-applications.sh"
if ! grep -q -F -- '--ignored --exact' "${PLAN214_RUNNER}"; then
  echo "evidence check failed: Plan 214 runner lost the explicit --ignored --exact remote selection" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'run-plan214-applications.sh' "${HARNESS}"; then
  echo "evidence check failed: harness does not delegate remote qualification to the Plan 214 runner (Plan 214 §14 forbids duplicate qualification)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F "${I2PD_PIN}" "${HARNESS}"; then
  echo "evidence check failed: harness lost the exact i2pd pin ${I2PD_PIN}" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F "${I2PD_PIN}" "${PLAN214_RUNNER}"; then
  echo "evidence check failed: Plan 214 runner lost the exact i2pd pin ${I2PD_PIN}" >&2
  failures=$((failures + 1))
fi
# The historical §6.3 blocker probe (SAM DEST GENERATE +
# REMOTE_QUALIFY_UNKNOWN_PEER) is superseded by the Plan 213
# generic proof and the Plan 214 application proof (Plan 214
# §14): the delegated lane provisions real i2pd server tunnels
# with published LeaseSet2s instead of an unpublished DEST
# GENERATE identity, so neither marker is required in the
# delegating harness anymore.
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

# 12. Plan 202 — positive M10 remote destination/Streaming composition
# driver exists, is gated as `#[ignore]`, fails closed when the
# exact-pinned i2pd environment is absent, and never logs peer key
# material. The driver is the structural foundation for the M10
# production remote composition path; it executes through the
# explicit `--ignored --exact` selection in the external lane and
# advances the typed `RemoteDeliveryCounters` for positive
# observations (Plan 202 §13).
if [[ ! -f "${PLAN202_DRIVER}" ]]; then
  echo "evidence check failed: Plan 202 remote composition driver missing: ${PLAN202_DRIVER}" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F '#[ignore = "Plan 202' "${PLAN202_DRIVER}"; then
  echo "evidence check failed: Plan 202 driver lost its #[ignore] gate" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'm10_remote_destination_streaming_composition_through_manager' "${PLAN202_DRIVER}"; then
  echo "evidence check failed: Plan 202 driver lost its Direction A test name" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'RemoteDeliveryCounters' "${PLAN202_DRIVER}"; then
  echo "evidence check failed: Plan 202 driver lost its typed remote delivery counters" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'install_router_delivery_handle' "${PLAN202_DRIVER}"; then
  echo "evidence check failed: Plan 202 driver lost its install_router_delivery_handle wiring" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'routing_decision_for' "${PLAN202_DRIVER}"; then
  echo "evidence check failed: Plan 202 driver lost its routing_decision_for assertions" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'has_backend' "${PLAN202_DRIVER}"; then
  echo "evidence check failed: Plan 202 driver lost its has_backend assertion (Plan 206 §5)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'RoutingDecision::RemoteRouter' "${PLAN202_DRIVER}"; then
  echo "evidence check failed: Plan 202 driver lost its RemoteRouter classification assertion" >&2
  failures=$((failures + 1))
fi
if grep -n -E '(println!|print!|eprintln!)[^;]*(peer_pub_b64|PUB_B64|PUB=)' "${PLAN202_DRIVER}"; then
  echo "evidence check failed: Plan 202 driver may log peer key material" >&2
  failures=$((failures + 1))
fi
# Plan 206 §13 — the manager must expose the typed
# `RemoteDestinationBackend` shape and the typed seam that routes a
# non-co-owned peer through it. Counters advance at operation
# boundaries, not via external record_observation calls.
if ! grep -q -F 'RemoteDestinationBackend' "${REPO_ROOT}/crates/i2pr-daemon/src/service_delivery.rs"; then
  echo "evidence check failed: Plan 206 backend struct missing from service_delivery.rs" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'with_backend' "${REPO_ROOT}/crates/i2pr-daemon/src/service_delivery.rs"; then
  echo "evidence check failed: Plan 206 ServiceDestinationDelivery::with_backend constructor missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'has_backend' "${REPO_ROOT}/crates/i2pr-daemon/src/service_delivery.rs"; then
  echo "evidence check failed: Plan 206 ServiceDestinationDelivery::has_backend accessor missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'note_lookup_cache_hit' "${REPO_ROOT}/crates/i2pr-daemon/src/service_delivery.rs"; then
  echo "evidence check failed: Plan 206 typed lookup-cache-hit seam missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'note_outbound_composed' "${REPO_ROOT}/crates/i2pr-daemon/src/service_delivery.rs"; then
  echo "evidence check failed: Plan 206 typed outbound-composed seam missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'note_inbound_dispatched' "${REPO_ROOT}/crates/i2pr-daemon/src/service_delivery.rs"; then
  echo "evidence check failed: Plan 206 typed inbound-dispatched seam missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'remote_lookup_cache_hit' "${REPO_ROOT}/crates/i2pr-daemon/src/service_delivery.rs"; then
  echo "evidence check failed: Plan 206 typed lookup-cache-hit counter missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'remote_outbound_composed' "${REPO_ROOT}/crates/i2pr-daemon/src/service_delivery.rs"; then
  echo "evidence check failed: Plan 206 typed outbound-composed counter missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'remote_inbound_dispatched' "${REPO_ROOT}/crates/i2pr-daemon/src/service_delivery.rs"; then
  echo "evidence check failed: Plan 206 typed inbound-dispatched counter missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'plan206_remote_composition_tests' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: Plan 206 manager-level test module missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'route_outbound_remote_request' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: Plan 206 manager route_outbound_remote_request method missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'dispatch_inbound_to_owned_destination' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: Plan 206 manager dispatch_inbound_to_owned_destination method missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'register_inbound_destination_owner' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: Plan 206 inbound owner registration method missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'resolve_remote_lease_set2' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: Plan 206 manager resolve_remote_lease_set2 method missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'inbound_owners' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: Plan 206 inbound_owners field missing from ServiceTunnelManager" >&2
  failures=$((failures + 1))
fi

# 13. Plan 208 — M10 production delivery-driver remote-route integration
# corrective. The production service delivery sweep
# (`ServiceTunnelManager::deliver_outbound`) must invoke the typed
# remote-routing seam (`route_outbound_remote_request`) so a
# reachable remote peer no longer dies at the legacy pre-Plan-208
# `unknown_peer` branch. The integration lives in production code,
# not only in unit tests; a real second-row sweep with an installed
# backend must advance `remote_outbound_composed` (or, when no LS2
# is cached, surface a typed `RemoteDeliveryError`) without
# incrementing the per-destination `unknown_peer` counter. The
# counted Plan 208 driver is `#[ignore]`-gated and exercises the
# production sweep against the exact-pinned i2pd 2.61.0 cache.
if [[ ! -f "${PLAN208_DRIVER}" ]]; then
  echo "evidence check failed: Plan 208 remote-route integration driver missing: ${PLAN208_DRIVER}" >&2
  failures=$((failures + 1))
else
  if ! grep -q -F '#[ignore = "Plan 208' "${PLAN208_DRIVER}"; then
    echo "evidence check failed: Plan 208 driver lost its #[ignore] gate" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'm10_remote_route_integration_through_deliver_outbound' "${PLAN208_DRIVER}"; then
    echo "evidence check failed: Plan 208 driver lost its Direction A test name" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'RemoteDestinationBackend' "${PLAN208_DRIVER}"; then
    echo "evidence check failed: Plan 208 driver lost its RemoteDestinationBackend wiring" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'RoutingDecision::RemoteRouter' "${PLAN208_DRIVER}"; then
    echo "evidence check failed: Plan 208 driver lost its RemoteRouter classification assertion" >&2
    failures=$((failures + 1))
  fi
  # Plan 208 §15 — the counted driver must not construct a parallel
  # StreamingManager / StreamingDestinationAdapter shadow stack. The
  # only sanctioned path is to drive the production sweep and let
  # the manager compose through the shared backend.
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN208_DRIVER}" |
     grep -q 'StreamingManager::new'; then
    echo "evidence check failed: Plan 208 driver must not construct a parallel StreamingManager (Plan 208 §14 anti-shadow rule)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN208_DRIVER}" |
     grep -q 'StreamingDestinationAdapter::new'; then
    echo "evidence check failed: Plan 208 driver must not construct a parallel StreamingDestinationAdapter (Plan 208 §14 anti-shadow rule)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN208_DRIVER}" |
     grep -q 'record_remote_application_observation'; then
    echo "evidence check failed: Plan 208 driver must not call record_remote_application_observation" >&2
    failures=$((failures + 1))
  fi
  if grep -n -E '(println!|print!|eprintln!)[^;]*(peer_pub_b64|PUB_B64|PUB=)' "${PLAN208_DRIVER}"; then
    echo "evidence check failed: Plan 208 driver may log peer key material" >&2
    failures=$((failures + 1))
  fi
fi
# Plan 208 §5 source-level invariants — the production service
# delivery driver must call the remote-routing seam. The grep is
# scoped to `service_tunnels.rs` and matches a call site inside
# `deliver_outbound`. A pure method-definition grep would
# silently satisfy the rule; the static checker requires the call
# site to appear in the production code path.
if ! grep -q -F 'route_outbound_remote_request(' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: production deliver_outbound does not call route_outbound_remote_request (Plan 208 §5)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'plan208_remote_route_integration_tests' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: plan208_remote_route_integration_tests module missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'compose_remote_cells' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: Plan 208 compose_remote_cells helper missing" >&2
  failures=$((failures + 1))
fi
# corrective driver exists, is `#[ignore]`-gated, declares the
# `m10_genuine_remote_http_and_irc_application_interop` Direction
# A test name, exercises `RemoteDestinationBackend` +
# `install_router_delivery_handle` + `routing_decision_for`, and
# asserts `RoutingDecision::RemoteRouter`. The driver spawns real
# system `curl` subprocess invocations against the i2pr HTTP client
# listener and real exact-pinned jaraco/irc public API subprocess
# invocations against the i2pr IRC client listener. The driver
# never logs peer key material, never calls
# `record_remote_application_observation`, and never substitutes
# an in-tree HTTP/IRC shadow client for `curl`/jaraco.
#
# Plan 207 replaces Plan 203 — the synthetic
# `record_remote_application_observation` label-injection pattern
# is no longer sufficient as a counted proof; the aggregate pass
# row must derive from the documented Plan 207 §9 subfact rows.
if [[ ! -f "${PLAN207_DRIVER}" ]]; then
  echo "evidence check failed: Plan 207 genuine remote HTTP/IRC driver missing: ${PLAN207_DRIVER}" >&2
  failures=$((failures + 1))
else
  if ! grep -q -F '#[ignore = "Plan 207' "${PLAN207_DRIVER}"; then
    echo "evidence check failed: Plan 207 driver lost its #[ignore] gate" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'm10_genuine_remote_http_and_irc_application_interop' "${PLAN207_DRIVER}"; then
    echo "evidence check failed: Plan 207 driver lost its test name" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'RemoteDestinationBackend' "${PLAN207_DRIVER}"; then
    echo "evidence check failed: Plan 207 driver lost its RemoteDestinationBackend wiring" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'RoutingDecision::RemoteRouter' "${PLAN207_DRIVER}"; then
    echo "evidence check failed: Plan 207 driver lost its RemoteRouter classification assertion" >&2
    failures=$((failures + 1))
  fi
  # Plan 207 §7 — real subprocess invocations only. The driver must
  # spawn `curl` as a subprocess and the exact-pinned jaraco/irc
  # Python driver as a subprocess; an in-tree `reqwest`-equivalent
  # HTTP shadow client or a hand-coded IRC protocol shadow client
  # are explicitly forbidden.
  if ! grep -q -E 'Command::new\(\s*curl_bin\s*\)|std::process::Command::new\(\s*curl_bin\s*\)|std::process::Command::new\(\s*curl\s*\)' "${PLAN207_DRIVER}" &&
     ! grep -q -E 'Command::new\("curl"\)|std::process::Command::new\("curl"\)' "${PLAN207_DRIVER}"; then
    echo "evidence check failed: Plan 207 driver does not spawn the system curl binary as a subprocess" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'irc.client' "${PLAN207_DRIVER}" &&
     ! grep -q -F 'irc_driver.py' "${PLAN207_DRIVER}"; then
    echo "evidence check failed: Plan 207 driver does not invoke the jaraco/irc public API subprocess" >&2
    failures=$((failures + 1))
  fi
  # Plan 207 §9 — the driver must write the documented subfact rows
  # to the TSV evidence file via `append_evidence` / `write_subfact`
  # / equivalent. The aggregate pass rows derive from these rows
  # only; manual label injection through
  # `record_remote_application_observation` is insufficient.
  if ! grep -q -F 'http-command-exit' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'http-status' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'http-body-digest' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'http-multipacket-digest' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'http-fixture-observed' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'http-no-clearnet-fallback' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'http-policy-retained' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'http-remote-stream-established' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'http-local-coowned-not-used' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'http-clean-resource-baseline' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'irc-driver-exit' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'irc-registration-welcome' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'irc-ping-pong-roundtrip' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'irc-privmsg-outbound-observed' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'irc-privmsg-inbound-observed' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'irc-ctcp-action-allowed' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'irc-dcc-blocked' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'irc-privacy-hostname-rewrite' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'irc-remote-stream-established' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'irc-local-coowned-not-used' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'irc-clean-resource-baseline' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'plan206-backend-counters' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'http-remote-application-established' "${PLAN207_DRIVER}" ||
     ! grep -q -F 'irc-remote-application-established' "${PLAN207_DRIVER}"; then
    echo "evidence check failed: Plan 207 driver omits at least one documented Plan 207 §9 subfact row" >&2
    failures=$((failures + 1))
  fi
  # Plan 207 §9 — forbid manual label injection through the manager's
  # free `record_remote_application_observation` helper. The Plan
  # 207 driver must not call this helper; the aggregate pass row
  # must derive from the command-derived subfact rows only.
  # Comment lines (`//!` / `///` / `//`) and whitespace are excluded
  # so the check fires only on actual call sites.
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN207_DRIVER}" |
     grep -q 'record_remote_application_observation'; then
    echo "evidence check failed: Plan 207 driver must not call record_remote_application_observation (Plan 207 §9 forbids manual label injection)" >&2
    failures=$((failures + 1))
  fi
  if grep -n -E '(println!|print!|eprintln!)[^;]*(peer_pub_b64|PUB_B64|PUB=)' "${PLAN207_DRIVER}"; then
    echo "evidence check failed: Plan 207 driver may log peer key material" >&2
    failures=$((failures + 1))
  fi
fi
# Plan 203 legacy — the in-tree shadow driver is retained as a
# historical scaffold (its `record_remote_application_observation`
# label injection is no longer accepted as a positive proof by the
# Plan 207 lane). The static checker no longer requires the
# Plan 203 driver to exist on disk; the harness records the two
# remote rows `blocked` when the Plan 207 driver is not present.
# Plan 203 §11 typed observation surface is retained as a
# backwards-compatible manager API; new code must not depend on it
# for counted evidence.
#
# Plan 214 — product-only remote HTTP + IRC application
# requalification driver (evidence hardening over the retained
# Plan 211 harness, final M10 application closure authority). The
# counted driver is a black-box product harness: it validates the
# bounded config surface (no second manager), starts the
# production composition through the single `ServiceProduct`
# helper, builds real enabled HttpClient + IrcClient specs whose
# destinations are the i2pd server destinations extracted from
# the per-tunnel .dat files, pumps inbound concurrently around
# every external subprocess, proves target observations from
# fresh fixture records, reads operation-derived Plan 208
# counters through the helper's typed accessor, and stops the
# product. Pin facts are runner-derived preconditions, never
# driver literals. The aggregate pass rows and the terminal
# P214-* classification are runner-owned; the driver never writes
# them.
PLAN214_DRIVER="${PLAN211_DRIVER}"
if [[ ! -f "${PLAN214_DRIVER}" ]]; then
  echo "evidence check failed: Plan 214 product-only driver missing: ${PLAN214_DRIVER}" >&2
  failures=$((failures + 1))
else
  if ! grep -q -F '#[ignore = "Plan 214' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver lost its #[ignore] gate" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'm10_product_only_remote_http_and_irc_application_interop_v214' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver lost its v214 test name" >&2
    failures=$((failures + 1))
  fi
  # Plan 214 §5 — anti-shadow rule (carried from Plan 211 §5 /
  # Plan 209 §5). The counted driver must not construct or
  # directly drive `StreamingManager`,
  # `StreamingDestinationAdapter`, `DestinationRouting`,
  # `EciesSessionManager`, `DestinationTunnelCoordinator`,
  # `ExploratoryBuildCoordinator`, `Ssu2DaemonService`,
  # `RouterDeliveryService`, `RouterDeliveryRequest`, or any
  # `record_observation` / `.record_observation(` helper. The
  # grep is scoped to non-comment / non-doc lines so a documented
  # reference in module docs is allowed.
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q 'StreamingManager::new\|StreamingManager\b' ; then
    echo "evidence check failed: Plan 214 driver must not construct or directly drive StreamingManager (Plan 214 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q 'StreamingDestinationAdapter'; then
    echo "evidence check failed: Plan 214 driver must not construct or directly drive StreamingDestinationAdapter (Plan 214 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q 'DestinationRouting::new\|EciesSessionManager::new'; then
    echo "evidence check failed: Plan 214 driver must not construct or directly drive DestinationRouting / EciesSessionManager (Plan 214 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q 'DestinationTunnelCoordinator'; then
    echo "evidence check failed: Plan 214 driver must not construct DestinationTunnelCoordinator (Plan 214 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q 'ExploratoryBuildCoordinator'; then
    echo "evidence check failed: Plan 214 driver must not construct ExploratoryBuildCoordinator (Plan 214 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q 'Ssu2DaemonService'; then
    echo "evidence check failed: Plan 214 driver must not construct Ssu2DaemonService (Plan 214 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q 'RouterDeliveryService\|RouterDeliveryRequest'; then
    echo "evidence check failed: Plan 214 driver must not construct RouterDeliveryService / RouterDeliveryRequest (Plan 214 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q '\.record_observation(\|record_remote_application_observation'; then
    echo "evidence check failed: Plan 214 driver must not call record_observation / record_remote_application_observation (Plan 214 §5)" >&2
    failures=$((failures + 1))
  fi
  # Plan 214 §5 — the counted driver must not construct a second
  # daemon manager as a placeholder; the runtime-neutral
  # configuration validator is the only sanctioned preflight.
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q 'ServiceTunnelManager::new'; then
    echo "evidence check failed: Plan 214 driver must not construct ServiceTunnelManager (black-box rule, Plan 214 §5)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F '.validate()' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver lost its runtime-neutral config validation preflight (Plan 214 §5)" >&2
    failures=$((failures + 1))
  fi
  # Plan 214 §3 / §7 — the driver must use the production
  # composition helper. The only sanctioned path is the
  # `ServiceProduct::start` / `ServiceProduct::poll_inbound` /
  # `ServiceProduct::remote_counters` typed accessor surface.
  if ! grep -q -F 'ServiceProduct::start' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver must use ServiceProduct::start (Plan 214 §3)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'poll_inbound' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver must use ServiceProduct::poll_inbound (Plan 214 §7)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'remote_counters' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver must read remote_counters through the typed accessor (Plan 214 §9)" >&2
    failures=$((failures + 1))
  fi
  # Plan 214 §6 — pin facts are runner-derived. The driver must
  # consume the runner-verified booleans only as preconditions
  # and must never stamp literal pin success rows.
  if grep -q -F 'http-i2pd-pin-ok' "${PLAN214_DRIVER}" ||
     grep -q -F 'irc-i2pd-pin-ok' "${PLAN214_DRIVER}" ||
     grep -q -F 'irc-jaraco-pin-ok' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver must not stamp literal pin success rows (Plan 214 §6)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'PLAN214_I2PD_PIN_OK' "${PLAN214_DRIVER}" ||
     ! grep -q -F 'PLAN214_JARACO_PIN_OK' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver must consume the runner-verified pin preconditions (Plan 214 §6)" >&2
    failures=$((failures + 1))
  fi
  # Plan 214 §7 — real curl subprocess invocation only, with
  # concurrent inbound pumping around every external operation.
  if ! grep -q -E 'Command::new\(\s*curl_bin\(\s*\)\s*\)|Command::new\(\s*curl_bin\s*\)|std::process::Command::new\(\s*curl_bin\(\s*\)\s*\)|std::process::Command::new\(\s*curl_bin\s*\)|std::process::Command::new\(\s*curl\s*\)' "${PLAN214_DRIVER}" &&
     ! grep -q -E 'Command::new\("curl"\)|std::process::Command::new\("curl"\)' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver does not spawn the system curl binary as a subprocess" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'try_wait' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver has no non-blocking child wait (Plan 214 §7 concurrent pump)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q 'wait_with_output' && ! grep -q -F 'try_wait' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver blocks on subprocess output without pumping inbound (Plan 214 §7)" >&2
    failures=$((failures + 1))
  fi
  # Plan 214 §8 — HTTP status must be captured from curl's actual
  # status frame, never approximated from body length.
  if ! grep -q -F 'parse_curl_status_code' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver lost its actual-status parser (Plan 214 §8)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q 'get_body_len == 22'; then
    echo "evidence check failed: Plan 214 driver approximates HTTP status from body length (Plan 214 §8)" >&2
    failures=$((failures + 1))
  fi
  # Plan 214 §8 — exact-pinned jaraco/irc subprocess only.
  if ! grep -q -F 'irc_driver.py' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver does not invoke the jaraco/irc public API subprocess" >&2
    failures=$((failures + 1))
  fi
  # Plan 214 §5 — the driver must build real enabled HttpClient
  # and IrcClient specs, not an empty ServiceTunnelSet.
  if ! grep -q -F 'HttpClient' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver does not build a real HttpClient spec (Plan 214 §5)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'IrcClient' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver does not build a real IrcClient spec (Plan 214 §5)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'PLAN214_HTTP_SPEC_ID' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver does not declare PLAN214_HTTP_SPEC_ID" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'PLAN214_IRC_SPEC_ID' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver does not declare PLAN214_IRC_SPEC_ID" >&2
    failures=$((failures + 1))
  fi
  # Plan 214 §13 — harness-owned fixture-facts paths are the only
  # sanctioned target inputs; the discarded target-port env reads
  # must not come back.
  if ! grep -q -F 'PLAN214_HTTP_FIXTURE_FACTS' "${PLAN214_DRIVER}" ||
     ! grep -q -F 'PLAN214_IRC_FIXTURE_FACTS' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver must consume harness-owned fixture-facts paths (Plan 214 §13)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'PLAN211_HTTP_TARGET_PORT' "${PLAN214_DRIVER}" ||
     grep -q -F 'PLAN211_IRC_TARGET_PORT' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver must not read unused target-port inputs (Plan 214 §15.9)" >&2
    failures=$((failures + 1))
  fi
  # Plan 214 §9/§11 — independent per-application counter windows
  # with orphan deltas (HTTP and IRC never share one window).
  for window in http_before irc_before http_orphans_before irc_orphans_before; do
    if ! grep -q -F "${window}" "${PLAN214_DRIVER}"; then
      echo "evidence check failed: Plan 214 driver lost its independent counter window ${window} (Plan 214 §9/§11)" >&2
      failures=$((failures + 1))
    fi
  done
  # Plan 214 §10 — privacy rewrite and DCC blocking must be
  # fixture-derived, never literal success rows or absence-only
  # inference.
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q '"irc-privacy-rewrite-derived", "1"'; then
    echo "evidence check failed: Plan 214 driver stamps a literal privacy-rewrite success row (Plan 214 §10)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'irc-privacy-rewrite-derived-from-target' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver lost its target-derived privacy row (Plan 214 §10)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN214_DRIVER}" |
     grep -q 'if dcc_sent'; then
    echo "evidence check failed: Plan 214 driver derives DCC blocking from DCC_SENT absence (Plan 214 §10)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'irc-dcc-attempted' "${PLAN214_DRIVER}" ||
     ! grep -q -F 'irc-dcc-target-observation-delta-zero' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver lost its explicit DCC attempt + target-non-observation rows (Plan 214 §10)" >&2
    failures=$((failures + 1))
  fi
  # Plan 214 §16 — the driver must write the documented HTTP fact
  # rows to the TSV evidence file.
  for label in http-public-destination-hash http-product-listener-bound \
http-get-command-exit http-get-status http-get-response-len \
http-get-response-sha256 http-get-fixture-method http-get-fixture-path \
http-post-request-len http-post-request-sha256 http-post-command-exit \
http-post-fixture-body-len http-post-fixture-body-sha256 \
http-large-command-exit http-large-response-len http-large-response-sha256 \
http-large-expected-len http-large-expected-sha256 \
http-clearnet-rejected http-clearnet-target-observation-delta-zero \
http-ip-literal-rejected http-ip-target-observation-delta-zero \
http-remote-outbound-composed-delta http-router-delivery-delta \
http-remote-inbound-dispatched-delta http-local-coowned-delta-zero \
http-unknown-peer-delta-zero http-orphan-receive-delta-zero \
http-clean-resource-baseline; do
    if ! grep -q -F "${label}" "${PLAN214_DRIVER}"; then
      echo "evidence check failed: Plan 214 driver omits documented §16 HTTP fact row '${label}'" >&2
      failures=$((failures + 1))
    fi
  done
  # Startup LeaseSet2/installation rows are emitted per service
  # through one typed helper (`{http,irc}-startup-*`); the source
  # proves the row shape once and the runner gates both prefixed
  # keys in the TSV (see the plan214-http-startup-* /
  # plan214-irc-startup-* runner rows).
  for suffix in -startup-destination-hash-match -startup-lease-count -startup-inbound-owner-registered -startup-remote-lookup-started; do
    if ! grep -q -F -- "${suffix}" "${PLAN214_DRIVER}"; then
      echo "evidence check failed: Plan 214 driver omits startup row shape '*${suffix}' (Plan 214 §9)" >&2
      failures=$((failures + 1))
    fi
  done
  # Plan 214 §17 — the driver must write the documented IRC fact
  # rows to the TSV evidence file.
  for label in irc-public-destination-hash irc-product-listener-bound \
irc-command-exit irc-registration-client-welcome \
irc-registration-target-nick-observed irc-registration-target-user-observed \
irc-privacy-rewrite-derived-from-target irc-ping-issued-by-target \
irc-pong-observed-by-target irc-outbound-privmsg-target-observed \
irc-inbound-privmsg-client-observed irc-privmsg-token-match \
irc-action-result irc-dcc-attempted irc-dcc-policy-result \
irc-dcc-target-observation-delta-zero irc-destination-distinct-from-http \
irc-remote-outbound-composed-delta irc-router-delivery-delta \
irc-remote-inbound-dispatched-delta irc-local-coowned-delta-zero \
irc-unknown-peer-delta-zero irc-orphan-receive-delta-zero \
irc-clean-resource-baseline; do
    if ! grep -q -F "${label}" "${PLAN214_DRIVER}"; then
      echo "evidence check failed: Plan 214 driver omits documented §17 IRC fact row '${label}'" >&2
      failures=$((failures + 1))
    fi
  done
  # Plan 214 §13 — the counted driver must never log peer key
  # material.
  if grep -n -E '(println!|print!|eprintln!)[^;]*(peer_pub_b64|PUB_B64|PUB=)' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver may log peer key material" >&2
    failures=$((failures + 1))
  fi
  # Aggregate rows and the terminal classification are
  # runner-owned: the driver must never write them.
  if grep -q -F 'plan214-remote-http-eepsite' "${PLAN214_DRIVER}" ||
     grep -q -F 'plan214-remote-irc-service' "${PLAN214_DRIVER}" ||
     grep -q -F 'plan214-terminal-classification' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver must not write aggregate rows or the terminal classification (runner-owned, Plan 214 §13)" >&2
    failures=$((failures + 1))
  fi
  if grep -n -E 'record\s+"remote-independent-(http-eepsite|irc-service)"\s+passed' "${PLAN214_DRIVER}"; then
    echo "evidence check failed: Plan 214 driver must not contain literal aggregate-row pass assignments" >&2
    failures=$((failures + 1))
  fi
  # Plan 214 §19 — the counted driver's evidence helpers must stay
  # locked by focused unit rows in routine CI.
  PLAN214_UNIT_COUNT=$(grep -E -c 'fn plan214_' "${PLAN214_DRIVER}" || true)
  if (( PLAN214_UNIT_COUNT < 20 )); then
    echo "evidence check failed: Plan 214 §19 — at least 20 plan214_… unit rows are required (found ${PLAN214_UNIT_COUNT})" >&2
    failures=$((failures + 1))
  fi
fi
if [[ ! -f "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs" ]]; then
  echo "evidence check failed: service_tunnels.rs source missing" >&2
  failures=$((failures + 1))
fi
if ! grep -q 'fn record_remote_application_observation' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: ServiceTunnelManager lost record_remote_application_observation" >&2
  failures=$((failures + 1))
fi
if ! grep -q 'REMOTE_APPLICATION_DOCUMENTED_LABELS' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: ServiceTunnelManager lost REMOTE_APPLICATION_DOCUMENTED_LABELS" >&2
  failures=$((failures + 1))
fi

# The manager-level routing-decision / router-delivery seams must
# also be exercised through the daemon's own unit tests so the
# structural shape is verified independent of any external peer.
if ! grep -q -F 'install_router_delivery' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: ServiceTunnelManager lost install_router_delivery" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'routing_decision_for' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: ServiceTunnelManager lost routing_decision_for" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'co_owned_destination_hashes' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: ServiceTunnelManager lost co_owned_destination_hashes" >&2
  failures=$((failures + 1))
fi

# Plan 214 §C — non-local client targets resolve through the
# requesting runtime's router-backed remote LeaseSet2 mirror (no
# second lookup, no local fallback). The typed seam must exist on
# the manager, all three client executors (HTTP/SOCKS/IRC) must
# fall through to it on `LookupRequired`, and the daemon's own
# unit suite must lock the miss/hit/unknown-service rows.
if ! grep -q -F 'resolve_remote_client_target' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: Plan 214 §C — ServiceTunnelManager lost resolve_remote_client_target" >&2
  failures=$((failures + 1))
fi
for executor in service_tunnels_http.rs service_tunnels_socks5.rs service_tunnels_irc_client.rs; do
  if ! grep -q -F 'resolve_remote_client_target' "${REPO_ROOT}/crates/i2pr-daemon/src/${executor}"; then
    echo "evidence check failed: Plan 214 §C — ${executor} must fall through to resolve_remote_client_target on LookupRequired" >&2
    failures=$((failures + 1))
  fi
done
PLAN214_RESOLVE_COUNT=$(grep -E -c 'fn plan214_resolve_remote_client_target_' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs" || true)
if (( PLAN214_RESOLVE_COUNT < 3 )); then
  echo "evidence check failed: Plan 214 §C — at least 3 plan214_resolve_remote_client_target_… unit rows are required (found ${PLAN214_RESOLVE_COUNT})" >&2
  failures=$((failures + 1))
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi

# Plan 210 §16 — evidence-integrity extensions for the M10 real
# service-destination network-material and inbound-streaming
# corrective. The structural source-level invariants enforce:
#
#   - remote service LeaseSet lookup is keyed by the actual remote
#     Destination hash; `SHA256(router_info_bytes)` is forbidden as
#     a service-lookup key;
#   - the counted remote compose path does not construct a
#     synthetic `dummy_outbound_tunnel()` placeholder;
#   - the inbound tunnel owner registry must exist, must reject
#     duplicate registrations, and must remove a registered owner
#     on unregistration;
#   - recovered inbound Garlic envelopes dispatch through the
#     canonical destination dispatcher + ECIES session manager
#     instead of being silently dropped.
SERVICE_TUNNELS_RS="${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"
SERVICE_DELIVERY_RS="${REPO_ROOT}/crates/i2pr-daemon/src/service_delivery.rs"
SERVICE_PRODUCT_RS="${REPO_ROOT}/crates/i2pr-daemon/src/service_product.rs"
SAM_STREAMS_RS="${REPO_ROOT}/crates/i2pr-daemon/src/sam/streams.rs"

# 14. Plan 210 §C — the production code path must not derive a
# service LeaseSet lookup key from `SHA256(router_info_bytes)`.
# The literal `DestinationHash::from_hash(i2pr_crypto::sha256(` is
# rejected everywhere in the daemon's service-tunnel composition
# code so a future regression cannot re-introduce the legacy
# router-identity-keyed lookup.
if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${SERVICE_PRODUCT_RS}" |
   grep -q 'DestinationHash::from_hash(i2pr_crypto::sha256('; then
  echo "evidence check failed: Plan 210 §C forbids SHA256(router_info_bytes) as a service lookup key (service_product.rs)" >&2
  failures=$((failures + 1))
fi

# 15. Plan 212 §8 (supersedes Plan 210 §C single-hash shape) —
# `ReferencePeer` is router transport/bootstrap metadata only and
# must NOT carry an application `destination_hash`. Per-service
# remote target hashes derive from the service specs'
# `DestinationRef` via `remote_target_hash_for_reference` +
# `resolve_remote_destination_for_service` (HTTP and IRC resolve
# independently; no single hash applies to all services).
if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${SERVICE_PRODUCT_RS}" |
   grep -q 'destination_hash: Option<\[u8; 32\]>\|reference\.destination_hash'; then
  echo "evidence check failed: Plan 212 §8 — ReferencePeer must not carry an application destination_hash (per-service DestinationRef resolution only)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'remote_target_hash_for_reference' "${SERVICE_TUNNELS_RS}"; then
  echo "evidence check failed: Plan 212 §8 — ServiceTunnelManager must own remote_target_hash_for_reference" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'resolve_remote_destination_for_service' "${SERVICE_PRODUCT_RS}"; then
  echo "evidence check failed: Plan 212 §8 — service_product must own resolve_remote_destination_for_service" >&2
  failures=$((failures + 1))
fi

# 16. Plan 210 §E — the counted remote compose path (`compose_remote_cells`)
# must not call `dummy_outbound_tunnel()` as a placeholder.
if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${SERVICE_TUNNELS_RS}" |
   grep -q 'dummy_outbound_tunnel('; then
  echo "evidence check failed: Plan 210 §E — compose_remote_cells must not call dummy_outbound_tunnel()" >&2
  failures=$((failures + 1))
fi
# The bridge helper that compose_remote_cells delegates to must
# exist and must NOT use a swap-and-restore placeholder pattern.
if ! grep -q -F 'compose_adapter_send_owned_fields' "${SAM_STREAMS_RS}"; then
  echo "evidence check failed: Plan 210 §E — SamDestinationBridge must own compose_adapter_send_owned_fields helper" >&2
  failures=$((failures + 1))
fi

# 17. Plan 210 §F — inbound tunnel owner reverse map exists, the
# helper rejects duplicate registrations, and the
# `note_inbound_orphan_receive` counter exists for stale / unknown
# receive ids.
if ! grep -q -F 'register_inbound_tunnel_owner' "${SERVICE_TUNNELS_RS}"; then
  echo "evidence check failed: Plan 210 §F — ServiceTunnelManager must own register_inbound_tunnel_owner" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'unregister_inbound_tunnel_owner' "${SERVICE_TUNNELS_RS}"; then
  echo "evidence check failed: Plan 210 §F — ServiceTunnelManager must own unregister_inbound_tunnel_owner" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'inbound_tunnel_owner' "${SERVICE_TUNNELS_RS}"; then
  echo "evidence check failed: Plan 210 §F — ServiceTunnelManager must own inbound_tunnel_owner accessor" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'note_inbound_orphan_receive' "${SERVICE_TUNNELS_RS}"; then
  echo "evidence check failed: Plan 210 §F — ServiceTunnelManager must own note_inbound_orphan_receive" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'inbound_tunnel_owners' "${SERVICE_TUNNELS_RS}"; then
  echo "evidence check failed: Plan 210 §F — ServiceTunnelManager must own inbound_tunnel_owners field" >&2
  failures=$((failures + 1))
fi

# 18. Plan 210 §G — recovered Garlic envelopes dispatch through
# the canonical destination dispatcher + ECIES session manager,
# not by being silently dropped.
if ! grep -q -F 'dispatch_inbound_garlic_owned' "${SAM_STREAMS_RS}"; then
  echo "evidence check failed: Plan 210 §G — SamDestinationBridge must own dispatch_inbound_garlic_owned" >&2
  failures=$((failures + 1))
fi
# The composition helper must wire `inbound_tunnel_owner` into
# the Garlic dispatch — a silent drop of Garlic is forbidden.
if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${SERVICE_PRODUCT_RS}" |
   grep -q 'I2npBody::Garlic(_) => {}'; then
  echo "evidence check failed: Plan 210 §G — process_inbound must not silently drop Garlic envelopes" >&2
  failures=$((failures + 1))
fi

# 19. Plan 210 §C/E/F/G — the existing manager-level test modules
# must continue to lock the typed seams. Plan 210 §14 conditions
# 1-24 must remain test-discoverable through at least one
# `plan210_…` test function in the daemon's own unit suite.
PLAN210_TEST_COUNT=$(grep -E -c 'plan210[_a-z]*\(' "${SERVICE_TUNNELS_RS}" || true)
if (( PLAN210_TEST_COUNT < 1 )); then
  echo "evidence check failed: Plan 210 §14 — at least one plan210_… test row is required" >&2
  failures=$((failures + 1))
fi

# 20. Plan 212 §4/§6 — router-backed network state type/field
# exists on the service bridge/runtime (distinct from the local
# `SamLocalProductFabric` seam; no silent relabel of localhost
# material as router-backed material).
if ! grep -q -F 'RouterDestinationNetworkState' "${SAM_STREAMS_RS}"; then
  echo "evidence check failed: Plan 212 §4 — RouterDestinationNetworkState type missing from sam/streams.rs" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'router_network:' "${SAM_STREAMS_RS}"; then
  echo "evidence check failed: Plan 212 §4 — SamDestinationBridge must own the router_network field" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'install_router_network_state' "${SAM_STREAMS_RS}"; then
  echo "evidence check failed: Plan 212 §6 — SamDestinationBridge must own install_router_network_state" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'has_router_network_state' "${SAM_STREAMS_RS}"; then
  echo "evidence check failed: Plan 212 §6 — SamDestinationBridge must own has_router_network_state" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'router_network_summary' "${SAM_STREAMS_RS}"; then
  echo "evidence check failed: Plan 212 §6 — SamDestinationBridge must own router_network_summary" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'install_service_router_material' "${SERVICE_TUNNELS_RS}"; then
  echo "evidence check failed: Plan 212 §6 — ServiceTunnelManager must own install_service_router_material" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'clear_service_router_material' "${SERVICE_TUNNELS_RS}"; then
  echo "evidence check failed: Plan 212 §6 — ServiceTunnelManager must own clear_service_router_material" >&2
  failures=$((failures + 1))
fi

# 21. Plan 212 §7/§9 — production provisioning builds real
# per-service tunnel material through the shared coordinator and
# registers inbound ownership (no second SSU2/NetDB/tunnel stack
# per service).
if ! grep -q -F 'provision_all_service_router_material' "${SERVICE_PRODUCT_RS}"; then
  echo "evidence check failed: Plan 212 §7 — service_product must own provision_all_service_router_material" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'Plan212TunnelIdAllocator' "${SERVICE_PRODUCT_RS}"; then
  echo "evidence check failed: Plan 212 §9 — service_product must own the Plan212TunnelIdAllocator" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'register_inbound_tunnel_owner' "${SERVICE_PRODUCT_RS}"; then
  echo "evidence check failed: Plan 212 §7 — production provisioning must call register_inbound_tunnel_owner" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'ExploratoryBuildCoordinator' "${SERVICE_PRODUCT_RS}"; then
  echo "evidence check failed: Plan 212 §9 — production provisioning must build through the shared ExploratoryBuildCoordinator" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'build_signed_lease_set2' "${SERVICE_PRODUCT_RS}"; then
  echo "evidence check failed: Plan 212 §9 — production provisioning must derive the service LS2 via build_signed_lease_set2" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'InboundLeaseSource::from_parts' "${SERVICE_PRODUCT_RS}"; then
  echo "evidence check failed: Plan 212 §9 — production provisioning must build InboundLeaseSource::from_parts from installed route metadata" >&2
  failures=$((failures + 1))
fi

# 22. Plan 212 §11 — remote compose runs explicitly against
# router-backed state (never `SamLocalProductFabric` fields).
if ! grep -q -F 'compose_router_send' "${SAM_STREAMS_RS}"; then
  echo "evidence check failed: Plan 212 §11 — SamDestinationBridge must own compose_router_send" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'compose_router_send' "${SERVICE_TUNNELS_RS}"; then
  echo "evidence check failed: Plan 212 §11 — remote compose must call the router-backed compose_router_send method" >&2
  failures=$((failures + 1))
fi
# Scoped negative: the router-backed compose seam must not read
# fabric material or placeholder tunnels. The grep is scoped to
# the compose_router_send function body so local-path helpers
# elsewhere in the file do not trip the rule.
COMPOSE_ROUTER_BODY=$(awk '/fn compose_router_send/,/^    \}/' "${SAM_STREAMS_RS}")
if echo "${COMPOSE_ROUTER_BODY}" | grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' | grep -q 'SamLocalProductFabric'; then
  echo "evidence check failed: Plan 212 §11 — compose_router_send must not call SamLocalProductFabric" >&2
  failures=$((failures + 1))
fi
if echo "${COMPOSE_ROUTER_BODY}" | grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' | grep -q 'dummy_outbound_tunnel('; then
  echo "evidence check failed: Plan 212 §11 — compose_router_send must not call dummy_outbound_tunnel" >&2
  failures=$((failures + 1))
fi
if echo "${COMPOSE_ROUTER_BODY}" | grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' | grep -q 'random_outbound_tunnel'; then
  echo "evidence check failed: Plan 212 §11 — compose_router_send must not use random_outbound_tunnel" >&2
  failures=$((failures + 1))
fi

# 23. Plan 212 §14 — inbound router dispatch drains `pop_payload`
# into `StreamingDestinationAdapter::receive` against the SAME
# canonical service StreamingManager; the inbound counter
# advances only after adapter receive (never immediately after
# `dispatch_garlic_envelope`).
if ! grep -q -F 'dispatch_router_garlic_to_canonical_streaming' "${SAM_STREAMS_RS}"; then
  echo "evidence check failed: Plan 212 §14 — SamDestinationBridge must own dispatch_router_garlic_to_canonical_streaming" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'pop_payload' "${SAM_STREAMS_RS}"; then
  echo "evidence check failed: Plan 212 §14 — inbound router dispatch must contain pop_payload" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'StreamingDestinationAdapter::receive' "${SAM_STREAMS_RS}"; then
  echo "evidence check failed: Plan 212 §14 — inbound router dispatch must contain StreamingDestinationAdapter::receive" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'dispatch_router_inbound_to_canonical_streaming' "${SERVICE_TUNNELS_RS}"; then
  echo "evidence check failed: Plan 212 §14 — ServiceTunnelManager must own dispatch_router_inbound_to_canonical_streaming" >&2
  failures=$((failures + 1))
fi
# The production inbound seam must gate `note_inbound_dispatched`
# on `streaming_packets_accepted` (Plan 212 §14 step 8), not on
# bare Garlic authentication.
if ! grep -q -F 'streaming_packets_accepted' "${SERVICE_PRODUCT_RS}"; then
  echo "evidence check failed: Plan 212 §14 — production inbound must gate note_inbound_dispatched on streaming_packets_accepted" >&2
  failures=$((failures + 1))
fi

# 24. Plan 212 §15 — I2NP decode parity: the outer SSU2 decode
# mirrors standard-first / short-fallback (no third decoder).
if ! grep -q -F 'decode_inbound_ssu2_i2np' "${SERVICE_PRODUCT_RS}"; then
  echo "evidence check failed: Plan 212 §15 — service_product must own the decode_inbound_ssu2_i2np parity helper" >&2
  failures=$((failures + 1))
fi

# 25. Plan 212 §17 — manager-level unit rows lock the typed path.
PLAN212_TEST_COUNT=$(grep -E -c 'plan212[_a-z0-9]*\(' "${SERVICE_TUNNELS_RS}" || true)
if (( PLAN212_TEST_COUNT < 25 )); then
  echo "evidence check failed: Plan 212 §17 — at least 25 plan212_… test rows are required (found ${PLAN212_TEST_COUNT})" >&2
  failures=$((failures + 1))
fi

# 26. Plan 212 §L — the ignored generic Direction A/B driver
# exists, is `#[ignore]`-gated, consumes the production
# composition API, and never constructs the forbidden lower
# stack.
PLAN212_DRIVER="${REPO_ROOT}/crates/i2pr-daemon/tests/service_tunnels_plan212_router_backed_product.rs"
if [[ ! -f "${PLAN212_DRIVER}" ]]; then
  echo "evidence check failed: Plan 212 generic driver missing: ${PLAN212_DRIVER}" >&2
  failures=$((failures + 1))
else
  if ! grep -q -F '#[ignore = "Plan 212' "${PLAN212_DRIVER}"; then
    echo "evidence check failed: Plan 212 driver lost its #[ignore] gate" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'plan212_router_backed_generic_directions' "${PLAN212_DRIVER}"; then
    echo "evidence check failed: Plan 212 driver lost its test name" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'ServiceProduct::start' "${PLAN212_DRIVER}"; then
    echo "evidence check failed: Plan 212 driver must use ServiceProduct::start (Plan 212 §L)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'poll_inbound' "${PLAN212_DRIVER}"; then
    echo "evidence check failed: Plan 212 driver must use ServiceProduct::poll_inbound (Plan 212 §L)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'remote_counters' "${PLAN212_DRIVER}"; then
    echo "evidence check failed: Plan 212 driver must read remote_counters (Plan 212 §L)" >&2
    failures=$((failures + 1))
  fi
  for forbidden in 'StreamingManager::new' 'StreamingManager\b' 'StreamingDestinationAdapter' 'DestinationTunnelCoordinator' 'ExploratoryBuildCoordinator' 'Ssu2DaemonService' 'RouterDeliveryService' 'RouterDeliveryRequest'; do
    if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN212_DRIVER}" | grep -q "${forbidden}"; then
      echo "evidence check failed: Plan 212 driver must not construct ${forbidden} (Plan 212 §L anti-shadow rule)" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN212_DRIVER}" |
     grep -q '\.record_observation(\|record_remote_application_observation'; then
    echo "evidence check failed: Plan 212 driver must not call record_observation / record_remote_application_observation (Plan 212 §L)" >&2
    failures=$((failures + 1))
  fi
  if grep -n -E '(println!|print!|eprintln!)[^;]*(peer_pub_b64|PUB_B64|PUB=)' "${PLAN212_DRIVER}"; then
    echo "evidence check failed: Plan 212 driver may log peer key material" >&2
    failures=$((failures + 1))
  fi
  for label in plan212-router-bootstrap-ok plan212-service-destination-hash plan212-real-outbound-installed plan212-real-inbound-installed plan212-local-ls2-real-lease-count plan212-inbound-owner-registered plan212-remote-ls2-lookup-started plan212-remote-ls2-lookup-succeeded plan212-direction-a-stream-established plan212-direction-a-small-digest-match plan212-direction-a-large-digest-match plan212-direction-a-inbound-streaming-accepted plan212-direction-b-local-ls2-published plan212-direction-b-inbound-owner-hit plan212-direction-b-stream-established plan212-direction-b-small-digest-match plan212-direction-b-large-digest-match plan212-orphan-receive-delta-zero plan212-local-coowned-delta-zero plan212-unknown-peer-delta-zero plan212-resource-baseline-clean; do
    if ! grep -q -F "${label}" "${PLAN212_DRIVER}"; then
      echo "evidence check failed: Plan 212 driver omits documented §21 evidence key '${label}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

# 27. Plan 213 §12 — generic external qualification harness
# completion and evidence corrective. The counted driver must be
# a real application-level proof (local TCP I/O both directions
# with concurrent production inbound pumping), mandatory facts
# must derive from executed I/O / typed summaries / counter
# deltas / subprocess exit codes, and the standalone runner must
# verify the exact pin by command and validate every mandatory
# row including exactly one terminal P213-* classification.
PLAN213_RUNNER="${REPO_ROOT}/tests/integration/service-tunnels/run-plan213-generic.sh"
PLAN213_FIXTURE="${REPO_ROOT}/tests/integration/service-tunnels/clients/sam_stream_fixture.py"
PLAN213_ECHO_FIXTURE="${REPO_ROOT}/tests/integration/service-tunnels/fixtures/echo_fixture.py"
if [[ ! -f "${PLAN213_RUNNER}" ]]; then
  echo "evidence check failed: Plan 213 standalone runner missing: ${PLAN213_RUNNER}" >&2
  failures=$((failures + 1))
fi
if [[ ! -f "${PLAN213_FIXTURE}" ]]; then
  echo "evidence check failed: Plan 213 SAM STREAM fixture missing: ${PLAN213_FIXTURE}" >&2
  failures=$((failures + 1))
fi
# 27.1 — the immutable-false Direction A/B scaffold is gone from
# the counted driver (a loop that only polls inbound and sleeps
# is not a qualification proof).
if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN212_DRIVER}" |
   grep -q -F 'direction_a_established = false'; then
  echo "evidence check failed: Plan 213 driver retains the immutable direction_a_established scaffold (Plan 213 §5)" >&2
  failures=$((failures + 1))
fi
if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN212_DRIVER}" |
   grep -q -F 'direction_b_established = false'; then
  echo "evidence check failed: Plan 213 driver retains the immutable direction_b_established scaffold (Plan 213 §5)" >&2
  failures=$((failures + 1))
fi
# 27.2 — no mandatory success row is a same-line literal pass.
# Conditioned `if cond { "1" } else { "0" }` rows are allowed;
# bare `, "1")` value literals are not.
if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN212_DRIVER}" |
   grep -q -E '(write_subfact|append_evidence)\(&evidence_dir, "plan2[12][^"]*", "1"\)'; then
  echo "evidence check failed: Plan 213 driver writes a literal success row (Plan 213 §9 forbids synthetic evidence)" >&2
  failures=$((failures + 1))
fi
# 27.3 — pin verification belongs in the shell runner; the Rust
# driver must not manufacture a pin-ok row.
if grep -q -F 'plan212-i2pd-pin-ok' "${PLAN212_DRIVER}" ||
   grep -q -F 'plan213-i2pd-pin-ok' "${PLAN212_DRIVER}"; then
  echo "evidence check failed: Plan 213 driver must not manufacture a pin-ok row (Plan 213 §E1)" >&2
  failures=$((failures + 1))
fi
# 27.4 — extended anti-shadow rule: the counted driver must not
# construct routing/session/fabric objects either (comment and
# doc lines excluded so the documented prohibition may be named).
for forbidden in 'DestinationRouting::new' 'EciesSessionManager::new' 'SamLocalProductFabric::' 'SamLocalProductFabric {'; do
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN212_DRIVER}" | grep -q -F "${forbidden}"; then
    echo "evidence check failed: Plan 213 driver must not construct ${forbidden} (Plan 213 §3)" >&2
    failures=$((failures + 1))
  fi
done
# 27.5 — Direction A must perform real local TCP application I/O
# through the GenericClient listener. (`grep -c`, not `grep -q`,
# after the pipe: under `pipefail` an early-exiting `grep -q`
# consumer would SIGPIPE the producer and flip the test.)
for required in 'TcpStream' 'write_all' 'read_exact'; do
  if [[ $(grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN212_DRIVER}" | grep -c -F "${required}") -eq 0 ]]; then
    echo "evidence check failed: Plan 213 driver has no real local TCP application I/O (${required} missing; Plan 213 §A1)" >&2
    failures=$((failures + 1))
  fi
done
# 27.6 — Direction B must initiate through the independent i2pd
# SAM STREAM fixture as a subprocess, never an in-tree client.
if ! grep -q -F 'sam_stream_fixture' "${PLAN212_DRIVER}"; then
  echo "evidence check failed: Plan 213 driver does not drive the independent SAM STREAM fixture (Plan 213 §C2)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'Command::new' "${PLAN212_DRIVER}"; then
  echo "evidence check failed: Plan 213 driver does not spawn the fixture as a subprocess (Plan 213 §C2)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'STREAM CONNECT' "${PLAN213_FIXTURE}"; then
  echo "evidence check failed: SAM STREAM fixture has no STREAM CONNECT initiator path (Plan 213 §C2)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'STREAM ACCEPT' "${PLAN213_FIXTURE}"; then
  echo "evidence check failed: SAM STREAM fixture has no STREAM ACCEPT server path (Plan 213 §B1)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'expected-connections' "${PLAN213_FIXTURE}"; then
  echo "evidence check failed: SAM STREAM fixture lost its bounded connection count (Plan 213 §B1)" >&2
  failures=$((failures + 1))
fi
# 27.7 — target-side fixture digest validation for Direction B
# (the pass must not rely only on the initiator echo).
if ! grep -q -F 'target-observed' "${PLAN212_DRIVER}"; then
  echo "evidence check failed: Plan 213 driver has no target-observed rows (Plan 213 §D)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F -- '--facts' "${PLAN213_ECHO_FIXTURE}"; then
  echo "evidence check failed: echo fixture lost its Plan 213 §D target-digest facts surface" >&2
  failures=$((failures + 1))
fi
# 27.8 — Direction A and B must have separate counter baseline
# windows (no single aggregate snapshot attributed to both).
for window in 'before_a' 'after_a' 'after_b'; do
  if ! grep -q -F "${window}" "${PLAN212_DRIVER}"; then
    echo "evidence check failed: Plan 213 driver lost its per-direction counter window ${window} (Plan 213 §E3)" >&2
    failures=$((failures + 1))
  fi
done
COUNTER_SNAPSHOTS=$(grep -E -c 'remote_counters\(\)\.await' "${PLAN212_DRIVER}" || true)
if (( COUNTER_SNAPSHOTS < 3 )); then
  echo "evidence check failed: Plan 213 driver needs at least 3 counter snapshots for two independent windows (found ${COUNTER_SNAPSHOTS})" >&2
  failures=$((failures + 1))
fi
# 27.9 — exact-pin command verification lives in the runner.
for required in 'source-revision.txt' 'rev-parse HEAD' '--version' 'status --porcelain'; do
  if ! grep -q -F -- "${required}" "${PLAN213_RUNNER}"; then
    echo "evidence check failed: Plan 213 runner lost its exact-pin command verification (${required}; Plan 213 §E1)" >&2
    failures=$((failures + 1))
  fi
done
# 27.10 — a skip outcome is never success in the Plan 213 lane.
if grep -q -F 'skip-generic-destination-not-provisioned' "${PLAN213_RUNNER}"; then
  echo "evidence check failed: Plan 213 runner treats a skipped generic gate as an outcome (Plan 213 §12 item 10)" >&2
  failures=$((failures + 1))
fi
# 27.11 — no private destination material enters uploaded
# evidence: the raw i2pd log stays in scratch and the runner
# audits evidence for private markers / overlong tokens.
if ! grep -q -F 'I2PD_LOG="${SCRATCH}' "${PLAN213_RUNNER}"; then
  echo "evidence check failed: Plan 213 runner must keep the raw i2pd log in scratch (Plan 213 §E1/§13)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'plan213-no-secret-leak' "${PLAN213_RUNNER}"; then
  echo "evidence check failed: Plan 213 runner lost its no-secret evidence audit (Plan 213 §12 item 11)" >&2
  failures=$((failures + 1))
fi
# 27.12 — the runner validates exactly one terminal P213-*
# classification per run.
if ! grep -q -F 'plan213-terminal-classification' "${PLAN213_RUNNER}"; then
  echo "evidence check failed: Plan 213 runner does not validate the terminal classification (Plan 213 §14)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'P213-N-passed' "${PLAN213_RUNNER}"; then
  echo "evidence check failed: Plan 213 runner does not gate on P213-N-passed (Plan 213 §14)" >&2
  failures=$((failures + 1))
fi
# 27.13 — the driver emits the documented Plan 213 §13 rows.
for label in plan213-direction-a-client-exit plan213-direction-a-target-observed-small plan213-direction-a-target-observed-large plan213-direction-b-reference-connect-exit plan213-direction-b-target-observed-small plan213-direction-b-target-observed-large plan213-direction-a-remote-outbound-delta plan213-direction-a-remote-inbound-delta plan213-direction-b-remote-outbound-delta plan213-direction-b-remote-inbound-delta plan213-terminal-classification plan213-product-listener-bound; do
  if ! grep -q -F "${label}" "${PLAN212_DRIVER}"; then
    echo "evidence check failed: Plan 213 driver omits documented §13 row '${label}'" >&2
    failures=$((failures + 1))
  fi
done
# 27.14 — the runner owns the command-derived pin/source rows.
for label in plan213-source-head plan213-i2pd-pin-sha plan213-i2pd-version plan213-i2pd-cache-clean plan213-reference-router-ready plan213-reference-sam-ready; do
  if ! grep -q -F "${label}" "${PLAN213_RUNNER}"; then
    echo "evidence check failed: Plan 213 runner omits command-derived row '${label}'" >&2
    failures=$((failures + 1))
  fi
done
# 27.15 — Plan 213 P213-C corrective lock: the production
# composition must address build replies to OUR controlled router
# hash (derived from the signing bundle), never to a hash derived
# from the reference RouterInfo (that addresses the reference
# itself, so installs never arrive while the reference still
# creates the endpoint).
if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${SERVICE_PRODUCT_RS}" |
   grep -q -F 'router_info.router_identity().hash()'; then
  echo "evidence check failed: service_product must not derive the local router hash from the reference RouterInfo (Plan 213 P213-C)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'local_router_hash' "${SERVICE_PRODUCT_RS}"; then
  echo "evidence check failed: service_product lost its bundle-derived local_router_hash (Plan 213 P213-C)" >&2
  failures=$((failures + 1))
fi
# 27.16 — accept ordering: the driver paces fixture ACCEPTs with
# trigger files (the proven Plan 193 accept-after-initiation
# ordering, never a minute-stale pending ACCEPT), and the runner
# wires the trigger prefix into both sides.
if ! grep -q -F 'accept_trigger' "${PLAN212_DRIVER}"; then
  echo "evidence check failed: Plan 213 driver lost its ACCEPT trigger pacing (Plan 213 §B1)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F -- '--accept-trigger' "${PLAN213_RUNNER}"; then
  echo "evidence check failed: Plan 213 runner does not wire --accept-trigger into the fixture (Plan 213 §B1)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'PLAN213_A_TRIGGER' "${PLAN213_RUNNER}"; then
  echo "evidence check failed: Plan 213 runner does not export PLAN213_A_TRIGGER to the driver (Plan 213 §B1)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'accept_trigger' "${PLAN213_FIXTURE}"; then
  echo "evidence check failed: SAM STREAM fixture lost its trigger-paced ACCEPT path (Plan 213 §B1)" >&2
  failures=$((failures + 1))
fi

# 28. Plan 214 §15 — HTTP/IRC product-only external
# requalification, evidence hardening, and final closure. The
# standalone runner owns the single counted remote application
# path; the delegating harness maps its aggregate rows; the
# hosted workflow orders Plan 213 before Plan 214.
if [[ ! -f "${PLAN214_RUNNER}" ]]; then
  echo "evidence check failed: Plan 214 standalone runner missing: ${PLAN214_RUNNER}" >&2
  failures=$((failures + 1))
fi
# 28.1 — exact pins verified by command in the runner (source
# revision file + checkout HEAD + clean tree + --version), plus
# curl/python presence.
for required in 'source-revision.txt' 'rev-parse HEAD' '--version' 'status --porcelain'; do
  if ! grep -q -F -- "${required}" "${PLAN214_RUNNER}"; then
    echo "evidence check failed: Plan 214 runner lost its exact-pin command verification (${required}; Plan 214 §6)" >&2
    failures=$((failures + 1))
  fi
done
if ! grep -q -F 'JARACO_PIN=' "${PLAN214_RUNNER}" &&
   ! grep -q -F "${JARACO_PIN}" "${PLAN214_RUNNER}"; then
  echo "evidence check failed: Plan 214 runner lost the exact jaraco/irc pin (Plan 214 §6)" >&2
  failures=$((failures + 1))
fi
# 28.2 — Plan 213 prerequisite gate (P214-A): no application
# qualification without the generic proof.
if ! grep -q -F 'plan214-prerequisite-plan213' "${PLAN214_RUNNER}"; then
  echo "evidence check failed: Plan 214 runner has no Plan 213 prerequisite gate (Plan 214 §18 P214-A)" >&2
  failures=$((failures + 1))
fi
# 28.3 — stale evidence cleared at runner start (Plan 214
# §19.18): the same directory must never carry a previous run's
# rows into aggregation.
if ! grep -q -F 'rm -rf "${EVIDENCE_DIR}"' "${PLAN214_RUNNER}"; then
  echo "evidence check failed: Plan 214 runner does not clear stale evidence at start (Plan 214 §19.18)" >&2
  failures=$((failures + 1))
fi
# 28.4 — fixture contract: the HTTP fixture publishes its
# deterministic large-response length/digest at startup and the
# runner captures it for equality gating (Plan 214 §8 D.4).
# The i2pd server tunnels are zero-hop (no peer hops exist in
# the isolated lane, so one-hop pools could never publish;
# zero-hop pools publish verifiable LS2s locally).
if ! grep -q -F 'LARGE_SHA256' "${PLAN214_RUNNER}"; then
  echo "evidence check failed: Plan 214 runner does not capture the fixture large-response contract (Plan 214 §8)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'inbound.length = 0' "${PLAN214_RUNNER}"; then
  echo "evidence check failed: Plan 214 runner lost its zero-hop server-tunnel provisioning (isolated lane has no peers for hop selection)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'LARGE_LEN=' "${REPO_ROOT}/tests/integration/service-tunnels/fixtures/http_fixture.py" ||
   ! grep -q -F 'LARGE_SHA256=' "${REPO_ROOT}/tests/integration/service-tunnels/fixtures/http_fixture.py"; then
  echo "evidence check failed: HTTP fixture does not publish its large-response contract (Plan 214 §8 D.4)" >&2
  failures=$((failures + 1))
fi
# 28.5 — fresh-seq target observation: the HTTP fixture emits a
# monotonic seq per record and the IRC fixture streams facts
# mid-run (Plan 214 §8 D.2 / §10).
if ! grep -q -F '"seq"' "${REPO_ROOT}/tests/integration/service-tunnels/fixtures/http_fixture.py"; then
  echo "evidence check failed: HTTP fixture lost its per-record sequence numbers (Plan 214 §8 D.2)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'flush()' "${REPO_ROOT}/tests/integration/service-tunnels/fixtures/irc_fixture.py"; then
  echo "evidence check failed: IRC fixture does not stream facts mid-run (Plan 214 §10)" >&2
  failures=$((failures + 1))
fi
# 28.6 — the IRC client driver carries the deterministic session
# token and records the explicit DCC attempt (Plan 214 §10 F.3/F.5).
if ! grep -q -F -- '--token' "${REPO_ROOT}/tests/integration/service-tunnels/clients/irc_driver.py"; then
  echo "evidence check failed: IRC driver lost its deterministic session token (Plan 214 §10 F.3)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'DCC_ATTEMPTED' "${REPO_ROOT}/tests/integration/service-tunnels/clients/irc_driver.py"; then
  echo "evidence check failed: IRC driver lost its explicit DCC attempt fact (Plan 214 §10 F.5)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'ECHO_TOKEN_MATCH' "${REPO_ROOT}/tests/integration/service-tunnels/clients/irc_driver.py"; then
  echo "evidence check failed: IRC driver lost its echo token-match fact (Plan 214 §10 F.3)" >&2
  failures=$((failures + 1))
fi
# 28.7 — per-application fixture-facts plumbing: the runner owns
# the facts paths and passes them to the driver; the driver never
# reads the discarded target-port inputs (Plan 214 §13/§15.9).
for required in 'PLAN214_HTTP_FIXTURE_FACTS' 'PLAN214_IRC_FIXTURE_FACTS'; do
  if ! grep -q -F "${required}" "${PLAN214_RUNNER}"; then
    echo "evidence check failed: Plan 214 runner does not plumb ${required} (Plan 214 §13)" >&2
    failures=$((failures + 1))
  fi
done
# 28.8 — the runner validates every §16/§17 aggregate input and
# writes exactly the two aggregate rows plus one terminal
# classification (Plan 214 §16/§17/§18). The runner also gates
# both server-tunnel LS2 publications before invoking the driver
# (a `.dat` file proves key generation, not reachability).
for label in plan214-remote-http-eepsite plan214-remote-irc-service plan214-terminal-classification plan214-server-ls2-published; do
  if ! grep -q -F "${label}" "${PLAN214_RUNNER}"; then
    echo "evidence check failed: Plan 214 runner omits aggregate row '${label}' (Plan 214 §16/§17/§18)" >&2
    failures=$((failures + 1))
  fi
done
if ! grep -q -F 'P214-N-passed' "${PLAN214_RUNNER}"; then
  echo "evidence check failed: Plan 214 runner does not gate on P214-N-passed (Plan 214 §18)" >&2
  failures=$((failures + 1))
fi
for class in P214-A-prerequisite-plan213-not-green P214-B-reference-startup-or-pin P214-C-public-destination-extraction P214-D-service-product-start P214-E-http-target-resolution P214-F-http-request-outbound P214-G-http-return-path P214-H-http-policy-or-fixture-integrity P214-I-irc-target-resolution P214-J-irc-registration P214-K-irc-return-path P214-L-irc-privacy-policy P214-M-resource-or-evidence-integrity; do
  if ! grep -q -F "${class}" "${PLAN214_RUNNER}"; then
    echo "evidence check failed: Plan 214 runner omits terminal class ${class} (Plan 214 §18)" >&2
    failures=$((failures + 1))
  fi
done
# 28.9 — duplicate evidence keys fail aggregation in the runner
# (Plan 214 §19.17), not just in driver unit tests.
if ! grep -q -F 'duplicate' "${PLAN214_RUNNER}"; then
  echo "evidence check failed: Plan 214 runner has no duplicate-key aggregation guard (Plan 214 §19.17)" >&2
  failures=$((failures + 1))
fi
# 28.10 — private .dat files never enter evidence (Plan 214
# §12): the runner audits for them.
if ! grep -q -F "*.dat" "${PLAN214_RUNNER}"; then
  echo "evidence check failed: Plan 214 runner has no .dat exclusion audit (Plan 214 §12)" >&2
  failures=$((failures + 1))
fi
# 28.11 — the counted driver file keeps no stale v211 test name
# or placeholder-manager construction beside the v214 path.
if grep -q -F 'm10_product_only_remote_http_and_irc_application_interop_v211' "${PLAN214_DRIVER}"; then
  echo "evidence check failed: counted driver retains the superseded v211 test name (Plan 214 §5)" >&2
  failures=$((failures + 1))
fi
# 28.12 — hosted workflow orders the Plan 213 generic gate before
# the M10 application matrix that now delegates to Plan 214, and
# uploads the Plan 214 evidence (Plan 214 §21).
WORKFLOW="${REPO_ROOT}/.github/workflows/service-tunnels-external.yml"
if ! grep -q -F 'run-plan213-generic.sh' "${WORKFLOW}"; then
  echo "evidence check failed: hosted workflow lost its Plan 213 generic gate (Plan 214 §21)" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'plan214' "${WORKFLOW}"; then
  echo "evidence check failed: hosted workflow has no Plan 214 evidence path (Plan 214 §21)" >&2
  failures=$((failures + 1))
fi
# 28.13 — extraction-boundary self-test: a synthetic i2pd
# PrivateKeys shape proves the helper output stops exactly at
# the public Destination boundary and never emits private
# suffix bytes (Plan 214 §12).
SYNTH_DAT="$(mktemp -t plan214-parse-boundary.XXXXXX)"
SYNTH_OUT="$(REPO_ROOT="${REPO_ROOT}" python3 - "${SYNTH_DAT}" <<'PY' 2>/dev/null || true
import base64 as stdlib_b64
import hashlib
import importlib.util
import os
import sys
helper = os.path.join(
    os.environ.get("REPO_ROOT", "."),
    "tests/integration/service-tunnels/clients/parse_i2pd_destination.py",
)
spec = importlib.util.spec_from_file_location("plan214_parse", helper)
module = importlib.util.module_from_spec(spec)
spec.loader.exec_module(module)
# Synthetic shape mirroring the helper's documented layout:
# 387-byte standard identity (with a 4-byte extended cert, the
# ECIES_X25519_AEAD + Ed25519 shape i2pd 2.61.0 writes) followed
# by 64 fake private suffix bytes that must never be emitted.
pub = bytearray(387 + 4)
for index in range(len(pub)):
    pub[index] = index % 251
pub[385:387] = (4).to_bytes(2, "big")
pub = bytes(pub)
pub_len = 387 + 4
secret = bytes([0xA5]) * 64
with open(sys.argv[1], "wb") as handle:
    handle.write(pub + secret)
info = module.parse(sys.argv[1])
# The helper must stop exactly at the public boundary: hash over
# the public prefix only, base64 decodes back to pub_len bytes
# (no private suffix byte survives the round trip), and the b32
# label matches the hash.
decoded = stdlib_b64.b64decode(
    info["dest_b64"].replace("-", "+").replace("~", "/")
)
assert len(decoded) == pub_len, (len(decoded), pub_len)
assert decoded == pub
assert info["dest_hash"] == hashlib.sha256(pub).hexdigest()
assert info["pub_len"] == pub_len
print("boundary-ok")
PY
)"
if [[ "${SYNTH_OUT}" != "boundary-ok" ]]; then
  echo "evidence check failed: parse helper leaks past the public Destination boundary (Plan 214 §12)" >&2
  failures=$((failures + 1))
fi
rm -f "${SYNTH_DAT}"

if [[ "${failures}" -ne 0 ]]; then
  echo "evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi
echo "service-tunnel acceptance evidence integrity: ${#GUARDED[@]} rows command-derived, ${#BLOCKED[@]} rows blocked, no literal pass records, Plan 202 driver present and gated, Plan 210 structural invariants green, Plan 212 router-backed invariants green, Plan 213 generic qualification invariants green, Plan 214 application requalification invariants green"
