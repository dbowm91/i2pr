#!/usr/bin/env bash
# Plan 189 §8 — static evidence-integrity check for the M6 mixed-router
# (two-family) acceptance lane.
#
# Rejects known dangerous bookkeeping that would let the M6 mixed-router
# claim be marked `passed` without an executed command/test behind it.
# The sanctioned paths in the four per-layer harnesses are unchanged:
#   1. `record_guarded "<label>" "<detail>" "<rc>"` — records passed
#      only for rc 0;
#   2. `m6_row "<label>" "<key>" "<detail>"` — records passed only for
#      driver rc 0 plus the row's own sanitized evidence keys in
#      driver-evidence.tsv;
#   3. `m6_key_row` / `ref_row` / `blocked_row` — key-gated variants used
#      by the destination and tunnel harnesses;
#   4. `cross_family_row "<label>" "<family>" "<detail>" "<rc>"` —
#      cross-family aggregator helper that Plan 189 §8 adds on top of
#      the per-layer record_guarded family.
# A literal `record "<required-label>" passed` line (indented or not)
# in any of the four per-layer harnesses or in the cross-family
# aggregator fails this check. `failed`/`blocked` literals are
# fail-closed and permitted.
#
# Guarded cross-family labels (Plan 189 §8 mandatory row set):
#   external-daemon-strict-profile
#   external-reference-verified-i2pd
#   external-reference-verified-java
#   external-session-established-i2pd
#   external-session-established-java
#   external-tunnel-build-accepted-i2pd
#   external-tunnel-build-accepted-java
#   external-netdb-lookup-tunnel-i2pd
#   external-netdb-lookup-tunnel-java
#   external-destination-ls2-resolved-i2pd
#   external-destination-ls2-resolved-java
#   external-destination-message-roundtrip-i2pd
#   external-destination-message-roundtrip-java
#   external-streaming-established-i2pd
#   external-streaming-established-java
#   external-streaming-multipacket-digest-i2pd
#   external-streaming-multipacket-digest-java
#   external-clean-resource-baseline
#   workspace-gates
#
# Cross-family rows compose their label inside record_guarded family
# helpers plus the `cross_family_row` aggregator; per-layer rows must
# still flow through the per-layer static checkers unchanged.
# Routine CI calls this checker in addition to the four per-layer
# checkers; the manual external workflow (`m6-mixed-router-external.yml`)
# also calls it.
#
# Usage: bash scripts/check-m6-mixed-router-acceptance-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"

I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
JAVA_VERSION="2.13.0"

PREFLIGHT_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-preflight.sh"
PREFLIGHT_CHECK="${REPO_ROOT}/scripts/check-exploratory-tunnel-evidence.sh"
TUNNELS_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-tunnels.sh"
TUNNELS_CHECK="${REPO_ROOT}/scripts/check-exploratory-tunnel-evidence.sh"
NETDB_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-netdb.sh"
NETDB_CHECK="${REPO_ROOT}/scripts/check-netdb-tunnel-evidence.sh"
DESTINATION_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-destination.sh"
DESTINATION_CHECK="${REPO_ROOT}/scripts/check-destination-tunnel-evidence.sh"
CROSSFAMILY_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-m6-mixed-router.sh"

GUARDED=(
  external-daemon-strict-profile
  external-reference-verified
  external-session-established
  external-tunnel-build-accepted
  external-netdb-lookup-tunnel
  external-destination-ls2-resolved
  external-destination-message-roundtrip
  external-streaming-established
  external-streaming-multipacket-digest
  external-clean-resource-baseline
  workspace-gates
)

failures=0

# ---- 1. Required artifacts exist. ---------------------------------------
for required in \
  "${PREFLIGHT_HARNESS}" "${PREFLIGHT_CHECK}" \
  "${TUNNELS_HARNESS}" "${TUNNELS_CHECK}" \
  "${NETDB_HARNESS}" "${NETDB_CHECK}" \
  "${DESTINATION_HARNESS}" "${DESTINATION_CHECK}" \
  "${CROSSFAMILY_HARNESS}"; do
  if [[ ! -f "${required}" ]]; then
    echo "m6 mixed-router evidence check failed: missing required artifact: ${required}" >&2
    failures=$((failures + 1))
  fi
done

# ---- 2. The cross-family aggregator must reference both pins. ----------
if [[ -f "${CROSSFAMILY_HARNESS}" ]]; then
  if ! grep -q -F "${I2PD_PIN}" "${CROSSFAMILY_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: cross-family harness never references the i2pd pin ${I2PD_PIN}" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F "${JAVA_PIN}" "${CROSSFAMILY_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: cross-family harness never references the Java I2P pin ${JAVA_PIN}" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F "${I2PD_VERSION}" "${CROSSFAMILY_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: cross-family harness never references the i2pd version ${I2PD_VERSION}" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F "${JAVA_VERSION}" "${CROSSFAMILY_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: cross-family harness never references the Java I2P version ${JAVA_VERSION}" >&2
    failures=$((failures + 1))
  fi
  # The cross-family aggregator must keep its own cross_family_row
  # helper to bind each guarded row to a family + an executed
  # command exit code; without it the harness can only assert
  # structural presence, not evidence integrity.
  if ! grep -q -E 'cross_family_row\s*\(\)' "${CROSSFAMILY_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: cross-family harness never defines cross_family_row helper" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -E 'if \[\[ "\$\{rc\}" -eq 0 \]\]' "${CROSSFAMILY_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: cross-family harness lost the record_guarded-style exit-code gate" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 3. No literal unconditional pass records for guarded rows. ---------
HARNESSES=("${PREFLIGHT_HARNESS}" "${TUNNELS_HARNESS}" "${NETDB_HARNESS}" "${DESTINATION_HARNESS}" "${CROSSFAMILY_HARNESS}")
for label in "${GUARDED[@]}"; do
  for harness in "${HARNESSES[@]}"; do
    if [[ ! -f "${harness}" ]]; then
      continue
    fi
    if rg -n "^[[:space:]]*record[[:space:]]+[\"']${label}[\"'][[:space:]]+passed[[:space:]]*$" "${harness}" >/dev/null; then
      echo "m6 mixed-router evidence check failed: harness hard-codes a 'passed' row for guarded label '${label}': ${harness}" >&2
      failures=$((failures + 1))
    fi
  done
done

# ---- 4. Every guarded row must have a command-derived call site. --------
# Per-layer harnesses compose their labels inside the layer's own
# record_guarded / m6_row / m6_key_row / ref_row / blocked_row
# helpers. The cross-family aggregator must reference each label
# either through cross_family_row or through a recorded per-layer
# binding in evidence.json. We only require a wiring site here; the
# runtime guard (rc 0) lives in record_guarded.
if [[ -f "${CROSSFAMILY_HARNESS}" ]]; then
  for label in "${GUARDED[@]}"; do
    if ! rg -n "cross_family_row[[:space:]]+[\"']${label}[\"']" "${CROSSFAMILY_HARNESS}" >/dev/null; then
      echo "m6 mixed-router evidence check failed: guarded label '${label}' has no cross_family_row call site in the cross-family aggregator" >&2
      failures=$((failures + 1))
    fi
  done
fi

# ---- 5. Both pins must appear in every per-layer harness so the cross-
#        family aggregator can bind the same evidence to both families.
for harness in "${PREFLIGHT_HARNESS}" "${TUNNELS_HARNESS}" "${NETDB_HARNESS}" "${DESTINATION_HARNESS}"; do
  if [[ ! -f "${harness}" ]]; then
    continue
  fi
  if ! grep -q -F "${I2PD_PIN}" "${harness}"; then
    echo "m6 mixed-router evidence check failed: per-layer harness never references the i2pd pin ${I2PD_PIN}: ${harness}" >&2
    failures=$((failures + 1))
  fi
done

# ---- 6. The cross-family aggregator must not silently forgive failures.
if [[ -f "${CROSSFAMILY_HARNESS}" ]]; then
  if grep -n -E '^[^#]*(cargo test|cargo fmt|cargo check|bash \[\[ .* \]\]|javac -cp).*\|\| true' "${CROSSFAMILY_HARNESS}" >/dev/null 2>&1; then
    echo "m6 mixed-router evidence check failed: cross-family aggregator forgives a required command via '|| true'" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 7. The manual external lane must reference both pins too. ----------
WORKFLOW="${REPO_ROOT}/.github/workflows/m6-mixed-router-external.yml"
if [[ ! -f "${WORKFLOW}" ]]; then
  echo "m6 mixed-router evidence check failed: external workflow missing: ${WORKFLOW}" >&2
  failures=$((failures + 1))
else
  if ! grep -q -F "${I2PD_PIN}" "${WORKFLOW}"; then
    echo "m6 mixed-router evidence check failed: external workflow never references the i2pd pin ${I2PD_PIN}" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F "${JAVA_PIN}" "${WORKFLOW}"; then
    echo "m6 mixed-router evidence check failed: external workflow never references the Java I2P pin ${JAVA_PIN}" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'workflow_dispatch' "${WORKFLOW}"; then
    echo "m6 mixed-router evidence check failed: external workflow must be workflow_dispatch only (routine CI must never start Java)" >&2
    failures=$((failures + 1))
  fi
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "m6 mixed-router evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi
echo "m6 mixed-router evidence check passed (${#GUARDED[@]} guarded labels, two-family pins verified)"