#!/usr/bin/env bash
# Plan 255 §G — fail-closed evidence-integrity checker for the M11 i2pd
# transit qualification lane.
#
# Rejects known dangerous bookkeeping in
# tests/integration/m11-transit/run-i2pd.sh. Every mandatory row must
# flow through `record_guarded` (which gates on the actual command exit
# code) or `m11_row` (which additionally requires the row's own
# sanitized driver evidence keys to be present in the driver-evidence
# TSV). No literal `record "<row>" passed` line for a required row is
# permitted.
#
# The Plan 255 row set is split between Work package A (real SSU2
# inbound ownership gate), Work package B (exact-pin source lock and
# peer placement), Work package C (accepted OBEP / IBGW / Participant
# build matrix), Work package D (role-correct live data plane), Work
# package E (rejection and bandwidth), Work package F (expiry / replay /
# cancellation / restart), and Work package H (repeated exact-head
# stability). Work package G (this file + the runner) is the fail-closed
# scaffolding.
#
# Usage: bash scripts/check-m11-transit-qualification-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS="${REPO_ROOT}/tests/integration/m11-transit/run-i2pd.sh"
DRIVER="${REPO_ROOT}/crates/i2pr-daemon/tests/m11_transit_i2pd_external.rs"
WORKFLOW="${REPO_ROOT}/.github/workflows/m11-transit-external.yml"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"

# Plan 255 §4 + §G mandatory rows. Every label must be reachable
# through `record_guarded` or `m11_row` in the runner.
GUARDED=(
  # Work package A — real SSU2 inbound ownership gate
  m11-i2pd-live-next-inbound-observed
  m11-i2pd-live-owner-enabled
  m11-i2pd-authenticated-peer-bound
  m11-i2pd-no-direct-build-injection
  m11-i2pd-session-close-reconciled
  # Work package B — exact-pin source lock and peer placement
  m11-i2pd-source-pin
  m11-i2pd-source-clean
  m11-i2pd-short-build-source-lock
  m11-i2pd-explicit-peer-source-lock
  m11-i2pd-reference-knows-i2pr-ri
  m11-i2pd-selected-role-proven
  # Work package C — accepted build matrix
  m11-i2pd-obep-build-received
  m11-i2pd-obep-build-accepted
  m11-i2pd-obep-registration-live
  m11-i2pd-ibgw-build-received
  m11-i2pd-ibgw-build-accepted
  m11-i2pd-ibgw-registration-live
  m11-i2pd-participant-build-received
  m11-i2pd-participant-build-accepted
  m11-i2pd-participant-registration-live
  # Work package D — role-correct live data plane
  m11-i2pd-participant-data-forward
  m11-i2pd-participant-data-digest
  m11-i2pd-obep-delivery
  m11-i2pd-obep-fragmented-once
  m11-i2pd-ibgw-gateway-ingress
  m11-i2pd-ibgw-multicell-bounded
  m11-i2pd-replay-no-second-delivery
  # Work package E — rejection and bandwidth
  m11-i2pd-code30-build-rejected
  m11-i2pd-code30-no-registration
  m11-i2pd-code30-pending-baseline
  m11-i2pd-bandwidth-option-disposition
  # Work package F — expiry / replay / cancellation / restart
  m11-i2pd-expiry-drops-live-data
  m11-i2pd-expiry-resource-baseline
  m11-i2pd-cancel-drains
  m11-i2pd-session-close-peer-baseline
  m11-i2pd-restart-clean-baseline
  # Work package G — fail-closed runner / driver / workflow
  m11-i2pd-driver-exists
  m11-i2pd-driver-ignored-gated
  m11-i2pd-driver-missing-env-fails
  m11-i2pd-runner-pinned
  m11-i2pd-runner-loopback
  m11-i2pd-runner-no-public-network
  m11-i2pd-workflow-exists
  m11-i2pd-evidence-no-secret
)

failures=0
fail() {
  echo "check-m11-transit-qualification-evidence: $*" >&2
  failures=$((failures + 1))
}

# ---- Driver + runner + workflow must exist ------------------------------
for path in "${DRIVER}" "${HARNESS}" "${WORKFLOW}"; do
  if [[ ! -f "${path}" ]]; then
    fail "required Plan 255 surface missing: ${path}"
  fi
done

# ---- Runner must verify exact i2pd pin + version ------------------------
if ! grep -qF "${I2PD_PIN}" "${HARNESS}"; then
  fail "runner lost the exact i2pd pin ${I2PD_PIN}"
fi
if ! grep -qF "${I2PD_VERSION}" "${HARNESS}"; then
  fail "runner lost the exact i2pd version ${I2PD_VERSION}"
fi

# ---- Runner must stay loopback-only and reject public network -----------
if ! grep -qF '127.0.0.1' "${HARNESS}"; then
  fail "runner lost its loopback bind policy"
fi
if ! grep -qF 'notransit = false' "${HARNESS}"; then
  fail "runner must enable i2pd transit (notransit = false) so it can build tunnels"
fi
if ! grep -qF '[reseed]' "${HARNESS}"; then
  fail "runner must explicitly disable reseed to fail closed on public network"
fi
# Reseed URLs are explicitly forbidden; only the i2pd.conf keys
# `verify = true` and `urls =` (empty) are allowed in the reseed
# section. We strip the `<line-number>:` prefix from the captured
# hits so the filter matches the body content.
reseed_hits="$(grep -nE '^\[reseed\]|^verify|^urls' "${HARNESS}" || true)"
reseed_body="$(printf '%s\n' "${reseed_hits}" | sed -E 's/^[0-9]+://')"
if printf '%s\n' "${reseed_body}" | grep -v -qE 'verify = true|urls =|^\[reseed\]$'; then
  fail "runner must keep reseed disabled (no public URLs)"
fi

# ---- Driver must be #[ignore]-gated and require exact i2pd env ----------
if ! grep -qE '#\[ignore\s*=.*Plan 255' "${DRIVER}"; then
  fail "driver must be #[ignore]-gated with the Plan 255 explanation"
fi
if ! grep -qE 'env_value|I2PD_ROUTER_INFO|I2PR_SSU2_BIND' "${DRIVER}"; then
  fail "driver must require exact i2pd environment variables"
fi
if grep -qE 'cargo test .*\|\| true' "${HARNESS}"; then
  fail "external driver invocation must not be forgiven with || true"
fi
if ! grep -qF -- '--ignored --exact' "${HARNESS}"; then
  fail "harness must invoke the external driver with --ignored --exact"
fi

# ---- Every guarded row must flow through record_guarded or m11_row ------
for label in "${GUARDED[@]}"; do
  if grep -n -E "^[[:space:]]*record \"${label}\" passed" "${HARNESS}"; then
    fail "literal passed record for required row '${label}' (must flow through record_guarded)"
  fi
  if ! grep -q -E "record_guarded \"${label}\"" "${HARNESS}" &&
     ! grep -q -E "m11_row \"${label}\"" "${HARNESS}"; then
    fail "required row '${label}' has no record_guarded/m11_row call site"
  fi
done

# ---- record_guarded must gate on the exit code ---------------------------
if ! grep -q -E 'if \[\[ "\$\{rc\}" -eq 0 \]\]' "${HARNESS}"; then
  fail "record_guarded helper lost its exit-code gate"
fi

# ---- m11_row must additionally require the row's evidence key ----------
if ! grep -q -E 'grep -Fq "\$\{key\}" "\$\{DRIVER_TSV\}"' "${HARNESS}"; then
  fail "m11_row helper lost its per-row evidence-key gate"
fi

# ---- Driver must consume Ssu2InboundI2np through the live owner ----------
if ! grep -qE 'Ssu2InboundI2np|next_inbound|handle_inbound|TransitLiveOwner' "${DRIVER}"; then
  fail "driver must consume Ssu2InboundI2np through TransitLiveOwner::handle_inbound"
fi

# ---- Driver must forbid hand-built STBMs after runtime startup ----------
# The driver is allowed to mention the historical Plan 253 symbol
# inside a self-test that asserts the production invariant
# (`.contains("plan253_short_build_payload")` style checks); the
# rule forbids defining a helper that *constructs* one or that
# substitutes for runtime startup.
if grep -qE 'fn short_build_payload|fn build_short_payload' "${DRIVER}"; then
  fail "driver must not define a hand-built STBM helper"
fi

# ---- Driver must reject public-network fallback ------------------------
if grep -qE 'reseedFrom|publicreseed' "${DRIVER}"; then
  fail "driver must not introduce reseed or public network references"
fi

# ---- Workflow must build the i2pd cache + run the runner ---------------
if ! grep -qE 'fetch-ssu2-reference.sh' "${WORKFLOW}"; then
  fail "workflow must build the exact-pinned i2pd cache via fetch-ssu2-reference.sh"
fi
if ! grep -qE 'run-i2pd.sh' "${WORKFLOW}"; then
  fail "workflow must invoke tests/integration/m11-transit/run-i2pd.sh"
fi

# ---- Evidence must not retain secrets ----------------------------------
# The forbidden tokens are chosen to flag logging/serialization of
# static secrets or session keys, not local ECIES responder-priv
# variables used inside the test body (those are bounded
# `Zeroizing<[u8; 32]>` wrappers owned by the production crypto
# module). The check greps the runner and driver with comments
# stripped, plus the literal pattern definitions so the checker
# itself doesn't trip on its own text.
HARNESS_CODE="$(mktemp)"
grep -v -E '^[[:space:]]*#' "${HARNESS}" > "${HARNESS_CODE}"
DRIVER_CODE="$(mktemp)"
grep -v -E '^[[:space:]]*#' "${DRIVER}" > "${DRIVER_CODE}"
for forbidden in 'static_secret' 'static_priv_key' 'session_priv' 'router_secret' 'private_key_file' 'privkey_path'; do
  hits="$(grep -nE "${forbidden}" "${HARNESS_CODE}" "${DRIVER_CODE}" || true)"
  # Filter out lines that are themselves the checker's pattern definitions.
  filtered="$(printf '%s\n' "${hits}" | grep -v 'if ! rg -q\|for forbidden in' || true)"
  if [[ -n "${filtered}" ]]; then
    fail "runner/driver must not retain secret material (matched forbidden token ${forbidden})"
    printf '%s\n' "${filtered}" >&2
  fi
done
rm -f "${HARNESS_CODE}" "${DRIVER_CODE}"

if [[ "${failures}" -ne 0 ]]; then
  echo "check-m11-transit-qualification-evidence: ${failures} violation(s)" >&2
  exit 1
fi
echo "check-m11-transit-qualification-evidence: ${#GUARDED[@]} Plan 255 rows command-derived, no literal pass records"
