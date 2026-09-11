#!/usr/bin/env bash
# Plan 185 — static evidence-integrity check for the exploratory
# tunnel lane.
#
# Rejects known dangerous bookkeeping in
# tests/integration/m6-interop/run-tunnels.sh: required acceptance
# rows recorded `passed` without an executed command behind them.
# The sanctioned paths are:
#   1. `record_guarded "<label>" "<detail>" "<rc>"` — records passed
#      only for rc 0;
#   2. `m6_row "<label>" "<key>" "<detail>"` — records passed only
#      for driver rc 0 plus the row's own sanitized evidence keys
#      in driver-evidence.tsv.
# A literal `record "<required-label>" passed` line (indented or not)
# means a row was hard-coded and fails this check. `failed` literals
# are fail-closed and permitted.
#
# Guarded labels (Plan 185 final-row set):
#   local-exploratory-build-unit, local-exploratory-build-live,
#   local-tunnel-liveness, external-daemon-strict-profile,
#   external-reference-verified, external-session-established,
#   external-outbound-build-emitted, external-outbound-installed,
#   external-inbound-build-emitted, external-inbound-installed,
#   external-liveness-first-test, workspace-gates.
#
# Usage: bash scripts/check-exploratory-tunnel-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-tunnels.sh"

GUARDED=(
  local-exploratory-build-unit
  local-exploratory-build-live
  local-tunnel-liveness
  external-daemon-strict-profile
  external-reference-verified
  external-session-established
  external-outbound-build-emitted
  external-outbound-installed
  external-inbound-build-emitted
  external-inbound-installed
  external-liveness-first-test
  workspace-gates
)

failures=0

if [[ ! -f "${HARNESS}" ]]; then
  echo "exploratory tunnel harness missing: ${HARNESS}" >&2
  exit 1
fi

for label in "${GUARDED[@]}"; do
  # The harness must use `record_guarded` (with an explicit rc) or
  # `m6_row` (with an explicit driver-evidence key) for every
  # required label. A direct `record "<label>" passed` line fails.
  if rg -n "^[[:space:]]*record[[:space:]]+[\"']${label}[\"'][[:space:]]+passed[[:space:]]*$" "${HARNESS}" >/dev/null; then
    echo "FAIL: harness hard-codes a 'passed' row for guarded label '${label}'" >&2
    failures=$((failures + 1))
  fi
  if ! rg -n "record_guarded[\"']?[[:space:]]*[\"']?${label}[\"']?|m6_row[\"']?[[:space:]]*[\"']?${label}[\"']?" "${HARNESS}" >/dev/null; then
    echo "FAIL: harness never references guarded label '${label}' through record_guarded or m6_row" >&2
    failures=$((failures + 1))
  fi
done

if [[ "${failures}" -ne 0 ]]; then
  echo "exploratory tunnel evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi

echo "exploratory tunnel evidence check passed (${#GUARDED[@]} guarded labels)"
