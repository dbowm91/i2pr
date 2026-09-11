#!/usr/bin/env bash
# Plan 186 — static evidence-integrity check for the NetDB lane.
#
# Rejects known dangerous bookkeeping in
# tests/integration/m6-interop/run-netdb.sh: required acceptance
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
# Guarded labels (Plan 186 final-row set):
#   local-netdb-tunnel-unit, local-netdb-tunnel-live,
#   local-tunnel-liveness, external-daemon-strict-profile,
#   external-reference-verified, external-reference-floodfill,
#   external-session-established, external-outbound-lookup-tunnel,
#   external-publication-tunnel, external-direct-rejected,
#   external-liveness-first-test, workspace-gates.
#
# Usage: bash scripts/check-netdb-tunnel-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-netdb.sh"

GUARDED=(
  local-netdb-tunnel-unit
  local-netdb-tunnel-live
  local-tunnel-liveness
  external-daemon-strict-profile
  external-reference-verified
  external-reference-floodfill
  external-session-established
  external-outbound-lookup-tunnel
  external-publication-tunnel
  external-direct-rejected
  external-liveness-first-test
  workspace-gates
)

failures=0

if [[ ! -f "${HARNESS}" ]]; then
  echo "NetDB harness missing: ${HARNESS}" >&2
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
  echo "NetDB evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi

echo "NetDB evidence check passed (${#GUARDED[@]} guarded labels)"
