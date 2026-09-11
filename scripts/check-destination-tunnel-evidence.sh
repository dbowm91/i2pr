#!/usr/bin/env bash
# Plan 187 — static evidence-integrity check for the destination lane.
#
# Rejects known dangerous bookkeeping in
# tests/integration/m6-interop/run-destination.sh: required acceptance
# rows recorded `passed` without an executed command behind them.
# The sanctioned paths are:
#   1. `record_guarded "<label>" "<detail>" "<rc>"` — records passed
#      only for rc 0;
#   2. `m6_row "<label>" "<key>" "<detail>"` — records passed only
#      for driver rc 0 plus the row's own sanitized evidence keys
#      in driver-evidence.tsv;
#   3. `m6_key_row "<label>" "<key>" "<detail>"` — records passed
#      only for the row's own key in the fresh per-run
#      driver-evidence.tsv (hygiene deletes it at startup, so the
#      key proves this run executed the step); used only for rows
#      whose evidence the driver records before the §11 stop;
#   4. `ref_row "<label>" "<key>" "<detail>"` — records passed only
#      for the row's own count in the fresh per-run
#      reference-facts.tsv (reference-side evidence);
#   5. `blocked_row "<label>" "<key>" "<detail>"` — records passed
#      only for the row's own key in the fresh driver TSV (success
#      path); records `blocked` (never passed) with stop provenance
#      when the driver recorded `build-reply-gap-stop`; otherwise
#      records failed.
# A literal `record "<required-label>" passed` line (indented or not)
# means a row was hard-coded and fails this check. `failed`/`blocked`
# literals are fail-closed and permitted.
# A literal `record "<required-label>" passed` line (indented or not)
# means a row was hard-coded and fails this check. `failed` literals
# are fail-closed and permitted.
#
# Guarded labels (Plan 187 final-row set):
#   local-destination-tunnel-unit, local-destination-tunnel-live,
#   local-tunnel-liveness, external-daemon-strict-profile,
#   external-reference-verified, external-reference-floodfill,
#   external-session-established, external-sam-destination-created,
#   external-outbound-tunnel, external-inbound-tunnel,
#   external-outbound-accepted, external-inbound-accepted,
#   external-reference-ls2-published, external-lease-lookup-tunnel,
#   external-ls2-publication-tunnel, external-destination-outbound,
#   external-reference-received, external-destination-inbound,
#   external-direct-rejected, external-liveness-first-test,
#   workspace-gates.
#
# Driver-derived rows must be referenced through `m6_row` (with an
# explicit driver-evidence key); reference-log rows through
# `ref_row` (with an explicit reference-facts key); local/gate rows
# through `record_guarded` (with an explicit rc).
#
# Usage: bash scripts/check-destination-tunnel-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-destination.sh"

GUARDED=(
  local-destination-tunnel-unit
  local-destination-tunnel-live
  local-tunnel-liveness
  external-daemon-strict-profile
  external-reference-verified
  external-reference-floodfill
  external-session-established
  external-sam-destination-created
  external-outbound-tunnel
  external-inbound-tunnel
  external-outbound-accepted
  external-inbound-accepted
  external-reference-ls2-published
  external-lease-lookup-tunnel
  external-ls2-publication-tunnel
  external-destination-outbound
  external-reference-received
  external-destination-inbound
  external-direct-rejected
  external-liveness-first-test
  workspace-gates
)

failures=0

if [[ ! -f "${HARNESS}" ]]; then
  echo "destination harness missing: ${HARNESS}" >&2
  exit 1
fi

for label in "${GUARDED[@]}"; do
  # The harness must use `record_guarded` (with an explicit rc),
  # `m6_row` (with an explicit driver-evidence key), `m6_key_row`
  # (fresh-TSV key), `ref_row` (fresh facts key), or `blocked_row`
  # (key plus stop provenance) for every required label. A direct
  # `record "<label>" passed` line fails.
  if rg -n "^[[:space:]]*record[[:space:]]+[\"']${label}[\"'][[:space:]]+passed[[:space:]]*$" "${HARNESS}" >/dev/null; then
    echo "FAIL: harness hard-codes a 'passed' row for guarded label '${label}'" >&2
    failures=$((failures + 1))
  fi
  if ! rg -n "record_guarded[\"']?[[:space:]]*[\"']?${label}[\"']?|m6_row[\"']?[[:space:]]*[\"']?${label}[\"']?|m6_key_row[\"']?[[:space:]]*[\"']?${label}[\"']?|ref_row[\"']?[[:space:]]*[\"']?${label}[\"']?|blocked_row[\"']?[[:space:]]*[\"']?${label}[\"']?" "${HARNESS}" >/dev/null; then
    echo "FAIL: harness never references guarded label '${label}' through record_guarded, m6_row, m6_key_row, ref_row, or blocked_row" >&2
    failures=$((failures + 1))
  fi
done

if [[ "${failures}" -ne 0 ]]; then
  echo "destination evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi

echo "destination evidence check passed (${#GUARDED[@]} guarded labels)"
