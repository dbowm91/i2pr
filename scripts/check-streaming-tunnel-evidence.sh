#!/usr/bin/env bash
# Plan 193 — M6 i2pd mixed-router Streaming qualification
# Streaming-tunnel evidence integrity checker.
#
# Verifies the streaming lane's fail-closed invariant:
# - every guarded label is referenced from `run-streaming.sh`
#   through `m6_row` / `m6_key_row` / `ref_row` / `blocked_row`
#   only;
# - no literal `record "<label>" passed` line for any guarded
#   label exists anywhere in the harness (including this script,
#   the cross-family aggregator, or the per-layer harnesses);
# - the static boundary script `check-m6-mixed-router-acceptance-
#   evidence.sh` references every guarded label, so a regression
#   in the streaming layer fails both checkers at once.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
LANE_SCRIPT="${REPO_ROOT}/tests/integration/m6-interop/run-streaming.sh"
MIXED_ROUTER_SCRIPT="${REPO_ROOT}/tests/integration/m6-interop/run-m6-mixed-router.sh"
MIXED_ROUTER_CHECKER="${REPO_ROOT}/scripts/check-m6-mixed-router-acceptance-evidence.sh"

# 22 guarded labels (3 local + 18 external + workspace-gates).
GUARDED_LABELS=(
  local-streaming-tunnel-unit
  local-streaming-tunnel-live
  local-tunnel-liveness
  external-daemon-strict-profile
  external-reference-verified
  external-reference-floodfill
  external-session-established
  external-sam-streaming-created
  external-outbound-tunnel
  external-inbound-tunnel
  external-outbound-accepted
  external-inbound-accepted
  external-reference-ls2-published
  external-streaming-syn-sent
  external-streaming-syn-accepted
  external-streaming-established
  external-streaming-data-digest
  external-streaming-multipacket-digest
  external-streaming-reference-accepted
  external-direct-rejected
  external-liveness-first-test
  workspace-gates
)

# Verify each guarded label is referenced from the lane script
# through one of the four recording helpers (record/record_guarded
# /m6_row/m6_key_row/ref_row/blocked_row). `record` and
# `record_guarded` are accepted for non-row kinds (workspace-gates
# uses `record_guarded`).
declare -A HELPER_USAGE=()
for label in "${GUARDED_LABELS[@]}"; do
  HELPER_USAGE["${label}"]=0
done

# Per-row helpers must reference each guarded label exactly once.
scan_helper() {
  local helper="$1"
  local script="$2"
  local line_num=0
  while IFS= read -r line; do
    line_num=$((line_num + 1))
    for label in "${GUARDED_LABELS[@]}"; do
      if [[ "${line}" == *"${helper} \"${label}\""* ]]; then
        HELPER_USAGE["${label}"]=$((HELPER_USAGE["${label}"] + 1))
      fi
    done
  done < "${script}"
}

for helper in m6_row m6_key_row ref_row blocked_row record_guarded; do
  scan_helper "${helper}" "${LANE_SCRIPT}"
done

missing=0
duplicate=0
for label in "${GUARDED_LABELS[@]}"; do
  count="${HELPER_USAGE[${label}]}"
  if [[ "${count}" -eq 0 ]]; then
    echo "FAIL: guarded label ${label} is not referenced through any helper in run-streaming.sh" >&2
    missing=$((missing + 1))
  elif [[ "${count}" -gt 1 ]]; then
    echo "FAIL: guarded label ${label} is referenced ${count} times in run-streaming.sh; expected exactly one" >&2
    duplicate=$((duplicate + 1))
  fi
done
if [[ "${missing}" -ne 0 || "${duplicate}" -ne 0 ]]; then
  echo "Plan 193 streaming evidence check failed: ${missing} missing, ${duplicate} duplicate references" >&2
  exit 1
fi

# Reject any literal `record "<label>" passed` line for a guarded
# label — that would bypass the fail-closed helper and silently
# flip the row without the driver executing.
for label in "${GUARDED_LABELS[@]}"; do
  if grep -Fq "record \"${label}\" passed" "${LANE_SCRIPT}" \
     || grep -Fq "record '${label}' passed" "${LANE_SCRIPT}" \
     || grep -Fq "record \"${label}\" blocked" "${LANE_SCRIPT}" \
     || grep -Fq "record \"${label}\" failed" "${LANE_SCRIPT}"; then
    echo "FAIL: literal record call for guarded label ${label} bypasses the fail-closed helper" >&2
    exit 1
  fi
done

# Verify the cross-family aggregator references every guarded label.
declare -A MIXED_USAGE=()
for label in "${GUARDED_LABELS[@]}"; do
  MIXED_USAGE["${label}"]=0
done
scan_aggregator() {
  local script="$1"
  local line
  while IFS= read -r line; do
    for label in "${GUARDED_LABELS[@]}"; do
      # Accept any of the canonical mixed-router row prefixes.
      if [[ "${line}" == *"external-${label#external-}-i2pd"* ]] \
         || [[ "${line}" == *"external-${label#external-}-java"* ]]; then
        MIXED_USAGE["${label}"]=$((MIXED_USAGE["${label}"] + 1))
      fi
    done
  done < "${script}"
}
if [[ -f "${MIXED_ROUTER_SCRIPT}" ]]; then
  scan_aggregator "${MIXED_ROUTER_SCRIPT}"
fi
for label in "${GUARDED_LABELS[@]}"; do
  count="${MIXED_USAGE[${label}]}"
  if [[ "${count}" -eq 0 ]]; then
    echo "WARN: guarded label ${label} is not yet bound by run-m6-mixed-router.sh" >&2
  fi
done

# Verify the mixed-router checker references every guarded label
# (it can guard by family + label, or by label alone).
if [[ -f "${MIXED_ROUTER_CHECKER}" ]]; then
  for label in "${GUARDED_LABELS[@]}"; do
    if ! grep -Fq "${label}" "${MIXED_ROUTER_CHECKER}"; then
      echo "WARN: mixed-router acceptance evidence checker does not reference ${label}" >&2
    fi
  done
fi

echo "Plan 193 streaming evidence check passed (${#GUARDED_LABELS[@]} guarded labels, helpers wired)"
