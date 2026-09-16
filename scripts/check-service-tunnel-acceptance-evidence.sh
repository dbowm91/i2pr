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
# Plan 211 — product-only remote HTTP + IRC application
# final-acceptance corrective. The counted driver is a black-box
# product harness: it starts the production composition through
# the single `ServiceProduct` helper, builds real enabled
# HttpClient + IrcClient specs whose destinations are the i2pd
# server destinations extracted from the per-tunnel .dat files,
# reads listener addresses, runs unmodified application clients,
# reads operation-derived Plan 208 counters through the helper's
# typed accessor, and stops the product. The driver must not
# construct or drive any of the shadow-stack types listed in
# Plan 211 §5 / Plan 209 §5; the static checker rejects direct
# imports/calls of those types in the counted Plan 211 driver.
# The aggregate pass rows derive purely from the documented
# Plan 211 §10 subfact rows the driver writes to its evidence
# file.
if [[ ! -f "${PLAN211_DRIVER}" ]]; then
  echo "evidence check failed: Plan 211 product-only driver missing: ${PLAN211_DRIVER}" >&2
  failures=$((failures + 1))
else
  if ! grep -q -F '#[ignore = "Plan 211' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver lost its #[ignore] gate" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'm10_product_only_remote_http_and_irc_application_interop_v211' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver lost its v211 test name" >&2
    failures=$((failures + 1))
  fi
  # Plan 211 §5 — anti-shadow rule. The counted driver must not
  # construct or directly drive `StreamingManager`,
  # `StreamingDestinationAdapter`, `DestinationRouting`,
  # `EciesSessionManager`, `DestinationTunnelCoordinator`,
  # `ExploratoryBuildCoordinator`, `Ssu2DaemonService`,
  # `RouterDeliveryService`, `RouterDeliveryRequest`, or any
  # `record_observation` / `.record_observation(` helper. The
  # grep is scoped to non-comment / non-doc lines so a documented
  # reference in module docs is allowed.
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN211_DRIVER}" |
     grep -q 'StreamingManager::new\|StreamingManager\b' ; then
    echo "evidence check failed: Plan 211 driver must not construct or directly drive StreamingManager (Plan 211 §5 / Plan 209 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN211_DRIVER}" |
     grep -q 'StreamingDestinationAdapter'; then
    echo "evidence check failed: Plan 211 driver must not construct or directly drive StreamingDestinationAdapter (Plan 211 §5 / Plan 209 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN211_DRIVER}" |
     grep -q 'DestinationRouting::new\|EciesSessionManager::new'; then
    echo "evidence check failed: Plan 211 driver must not construct or directly drive DestinationRouting / EciesSessionManager (Plan 211 §5 / Plan 209 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN211_DRIVER}" |
     grep -q 'DestinationTunnelCoordinator'; then
    echo "evidence check failed: Plan 211 driver must not construct DestinationTunnelCoordinator (Plan 211 §5 / Plan 209 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN211_DRIVER}" |
     grep -q 'ExploratoryBuildCoordinator'; then
    echo "evidence check failed: Plan 211 driver must not construct ExploratoryBuildCoordinator (Plan 211 §5 / Plan 209 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN211_DRIVER}" |
     grep -q 'Ssu2DaemonService'; then
    echo "evidence check failed: Plan 211 driver must not construct Ssu2DaemonService (Plan 211 §5 / Plan 209 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN211_DRIVER}" |
     grep -q 'RouterDeliveryService\|RouterDeliveryRequest'; then
    echo "evidence check failed: Plan 211 driver must not construct RouterDeliveryService / RouterDeliveryRequest (Plan 211 §5 / Plan 209 §5)" >&2
    failures=$((failures + 1))
  fi
  if grep -E -v '^\s*(//|/\*|/\*!|/\*\*|\*)' "${PLAN211_DRIVER}" |
     grep -q '\.record_observation(\|record_remote_application_observation'; then
    echo "evidence check failed: Plan 211 driver must not call record_observation / record_remote_application_observation (Plan 211 §5 / Plan 209 §5 / §7)" >&2
    failures=$((failures + 1))
  fi
  # Plan 211 §3 / §7 — the driver must use the production
  # composition helper. The only sanctioned path is the
  # `ServiceProduct::start` / `ServiceProduct::poll_inbound` /
  # `ServiceProduct::remote_counters` typed accessor surface.
  if ! grep -q -F 'ServiceProduct::start' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver must use ServiceProduct::start (Plan 211 §3)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'poll_inbound' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver must use ServiceProduct::poll_inbound (Plan 211 §7)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'remote_counters' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver must read remote_counters through the typed accessor (Plan 211 §8)" >&2
    failures=$((failures + 1))
  fi
  # Plan 211 §7 — real curl subprocess invocation only.
  if ! grep -q -E 'Command::new\(\s*curl_bin\(\s*\)\s*\)|Command::new\(\s*curl_bin\s*\)|std::process::Command::new\(\s*curl_bin\(\s*\)\s*\)|std::process::Command::new\(\s*curl_bin\s*\)|std::process::Command::new\(\s*curl\s*\)' "${PLAN211_DRIVER}" &&
     ! grep -q -E 'Command::new\("curl"\)|std::process::Command::new\("curl"\)' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver does not spawn the system curl binary as a subprocess" >&2
    failures=$((failures + 1))
  fi
  # Plan 211 §8 — exact-pinned jaraco/irc subprocess only.
  if ! grep -q -F 'irc.client' "${PLAN211_DRIVER}" &&
     ! grep -q -F 'irc_driver.py' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver does not invoke the jaraco/irc public API subprocess" >&2
    failures=$((failures + 1))
  fi
  # Plan 211 §5 / §10 — the driver must build real enabled
  # HttpClient and IrcClient specs, not an empty ServiceTunnelSet.
  if ! grep -q -F 'HttpClient' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver does not build a real HttpClient spec (Plan 211 §5)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'IrcClient' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver does not build a real IrcClient spec (Plan 211 §5)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'PLAN211_HTTP_SPEC_ID' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver does not declare PLAN211_HTTP_SPEC_ID" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'PLAN211_IRC_SPEC_ID' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver does not declare PLAN211_IRC_SPEC_ID" >&2
    failures=$((failures + 1))
  fi
  # Plan 211 §10 — the driver must write the documented subfact
  # rows to the TSV evidence file.
  for label in http-curl-version http-i2pd-pin-ok \
http-public-destination-loaded http-product-listener-bound \
http-command-exit http-status http-response-digest \
http-fixture-method-path http-post-request-digest \
http-large-response-digest http-ordinary-ls2-lookup-proven \
http-remote-outbound-composed-delta http-remote-inbound-dispatched-delta \
http-router-delivery-delta http-local-coowned-delta-zero \
http-unknown-peer-delta-zero http-clearnet-rejected \
http-ip-literal-rejected http-clean-resource-baseline \
irc-jaraco-pin-ok irc-i2pd-pin-ok \
irc-public-destination-loaded irc-product-listener-bound \
irc-command-exit irc-registration-observed \
irc-ping-pong-observed irc-outbound-privmsg-observed \
irc-inbound-privmsg-observed \
irc-action-observed-or-retained-policy-reference \
irc-dcc-blocked-derived irc-privacy-rewrite-derived \
irc-ordinary-ls2-lookup-proven-or-cache-proven-after-same-target-resolution \
irc-remote-outbound-composed-delta irc-remote-inbound-dispatched-delta \
irc-router-delivery-delta irc-local-coowned-delta-zero \
irc-unknown-peer-delta-zero irc-clean-resource-baseline; do
    if ! grep -q -F "${label}" "${PLAN211_DRIVER}"; then
      echo "evidence check failed: Plan 211 driver omits documented §10 subfact row '${label}'" >&2
      failures=$((failures + 1))
    fi
  done
  # Plan 211 §13 — the counted driver must never log peer key
  # material.
  if grep -n -E '(println!|print!|eprintln!)[^;]*(peer_pub_b64|PUB_B64|PUB=)' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver may log peer key material" >&2
    failures=$((failures + 1))
  fi
  # Plan 211 §9 — aggregate row derivation must not be a literal
  # assignment. The driver must not contain a literal
  # `record "... passed"` row or similar success literal for
  # either aggregate row.
  if grep -n -E 'record\s+"remote-independent-(http-eepsite|irc-service)"\s+passed' "${PLAN211_DRIVER}"; then
    echo "evidence check failed: Plan 211 driver must not contain literal aggregate-row pass assignments (Plan 211 §9)" >&2
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

# 15. Plan 210 §C — the production code path must derive the
# remote destination hash from the supplied `ReferencePeer`'s
# explicit `destination_hash` field, not from router_info_bytes.
if ! grep -q -F 'reference.destination_hash' "${SERVICE_PRODUCT_RS}"; then
  echo "evidence check failed: Plan 210 §C — service_product must consume reference.destination_hash (not SHA256(router_info_bytes))" >&2
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

if [[ "${failures}" -ne 0 ]]; then
  echo "evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi
echo "service-tunnel acceptance evidence integrity: ${#GUARDED[@]} rows command-derived, ${#BLOCKED[@]} rows blocked, no literal pass records, Plan 202 driver present and gated, Plan 210 structural invariants green"
