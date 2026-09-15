#!/usr/bin/env bash
# Plan 199 / Plan 181 / Plan 202 — static evidence-integrity check
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
# Blocked labels (Plan 199 / Plan 202 execution stop condition):
#   remote-independent-http-eepsite, remote-independent-irc-service,
#   m10-remote-destination-streaming-composition.
#
# Plan 202 invariant: the positive M10 remote destination/Streaming
# composition driver must exist, must fail closed when the exact-
# pinned i2pd environment is absent, must never log peer key
# material, and must not auto-pass via a literal record call site.
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
PLAN203_DRIVER="${REPO_ROOT}/crates/i2pr-daemon/tests/service_tunnels_application_remote_qualification.rs"

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

# Plan 203 — positive remote HTTP + IRC application interop rows.
# The full lane must flow through `record_guarded` and produce the
# Plan 203 §5/§6 evidence keys (`http-remote-application-established`
# and `irc-remote-application-established`). The static checker
# rejects literal `record "... passed"` lines; the positive rows
# must flow through `record_guarded` and reference the documented
# Plan 203 evidence keys.
PLAN203_POSITIVE=(
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
# Plan 203 promotes the two remote rows from Plan 181's
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
# Plan 203 — positive remote HTTP + IRC application interop rows.
# Both rows must flow through `record_guarded` AND `record_blocked`
# (one path for the missing-env case, one for the positive case).
# Literal `record "...passed"` lines remain forbidden; the positive
# rows must reference the documented Plan 203 §5/§6 evidence keys.
for label in "${PLAN203_POSITIVE[@]}"; do
  if grep -n -E "^[[:space:]]*record \"${label}\" passed" "${HARNESS}"; then
    echo "evidence check failed: Plan 203 row '${label}' claims passed literally (must flow through record_guarded)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -E "record_blocked \"${label}\"" "${HARNESS}"; then
    echo "evidence check failed: Plan 203 row '${label}' has no record_blocked call site" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -E "record_guarded \"${label}\"|^[[:space:]]*record \"${label}\" passed" "${HARNESS}"; then
    echo "evidence check failed: Plan 203 row '${label}' has no record_guarded or post-`record` call site" >&2
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

# 13. Plan 203 — M10 positive remote HTTP + IRC application interop
# driver exists, is `#[ignore]`-gated, declares the
# `m10_positive_remote_http_and_irc_application_interop` Direction
# A test name, exercises `RemoteDeliveryCounters` +
# `install_router_delivery_handle` + `routing_decision_for`, and
# asserts `RoutingDecision::RemoteRouter`. The driver never logs
# peer key material. The manager exposes
# `record_remote_application_observation` and the documented
# 18-label observation set the static checker reads.
if [[ ! -f "${PLAN203_DRIVER}" ]]; then
  echo "evidence check failed: Plan 203 positive remote HTTP/IRC driver missing: ${PLAN203_DRIVER}" >&2
  failures=$((failures + 1))
else
  if ! grep -q -F '#[ignore = "Plan 203' "${PLAN203_DRIVER}"; then
    echo "evidence check failed: Plan 203 driver lost its #[ignore] gate" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'm10_positive_remote_http_and_irc_application_interop' "${PLAN203_DRIVER}"; then
    echo "evidence check failed: Plan 203 driver lost its Direction A test name" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'RemoteDeliveryCounters' "${PLAN203_DRIVER}" && ! grep -q -F 'install_router_delivery_handle' "${PLAN203_DRIVER}"; then
    echo "evidence check failed: Plan 203 driver lost its RemoteDeliveryCounters / install_router_delivery_handle wiring" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'RoutingDecision::RemoteRouter' "${PLAN203_DRIVER}"; then
    echo "evidence check failed: Plan 203 driver lost its RemoteRouter classification assertion" >&2
    failures=$((failures + 1))
  fi
  if grep -n -E '(println!|print!|eprintln!)[^;]*(peer_pub_b64|PUB_B64|PUB=)' "${PLAN203_DRIVER}"; then
    echo "evidence check failed: Plan 203 driver may log peer key material" >&2
    failures=$((failures + 1))
  fi
  # The Plan 203 §5/§6 documented evidence keys the static checker
  # gates on must appear in the driver output.
  for key in \
    http-remote-application-established \
    irc-remote-application-established \
    manager-routing-decision; do
    if ! grep -q "\"${key}\"" "${PLAN203_DRIVER}"; then
      echo "evidence check failed: Plan 203 driver missing append_evidence for ${key}" >&2
      failures=$((failures + 1))
    fi
  done
fi
# The manager must expose the Plan 203 §11 typed observation
# surface and the documented 18-label observation set.
if ! grep -q 'fn record_remote_application_observation' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: ServiceTunnelManager lost record_remote_application_observation" >&2
  failures=$((failures + 1))
fi
if ! grep -q 'REMOTE_APPLICATION_DOCUMENTED_LABELS' "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
  echo "evidence check failed: ServiceTunnelManager lost REMOTE_APPLICATION_DOCUMENTED_LABELS" >&2
  failures=$((failures + 1))
fi
for label in \
  http-get-status \
  http-get-body-digest \
  http-multipacket-digest \
  http-no-clearnet-fallback \
  http-policy-retained \
  http-remote-stream-established-counter \
  http-local-coowned-not-used \
  http-clean-resource-baseline \
  irc-connection-established \
  irc-registration-welcome \
  irc-ping-pong-roundtrip \
  irc-privmsg-roundtrip \
  irc-ctcp-action-allowed \
  irc-dcc-blocked \
  irc-privacy-hostname-rewrite \
  irc-remote-stream-established-counter \
  irc-local-coowned-not-used \
  irc-clean-resource-baseline; do
  if ! grep -q "\"${label}\"" "${REPO_ROOT}/crates/i2pr-daemon/src/service_tunnels.rs"; then
    echo "evidence check failed: ServiceTunnelManager missing REMOTE_APPLICATION_DOCUMENTED_LABELS entry '${label}'" >&2
    failures=$((failures + 1))
  fi
done

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
echo "service-tunnel acceptance evidence integrity: ${#GUARDED[@]} rows command-derived, ${#BLOCKED[@]} rows blocked, no literal pass records, Plan 202 driver present and gated"
