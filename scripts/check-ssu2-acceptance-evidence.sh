#!/usr/bin/env bash
# Plan 161 §13 — static evidence-integrity check for the SSU2 lane.
#
# Rejects known dangerous bookkeeping in tests/integration/ssu2/run-independent.sh:
# required acceptance rows recorded `passed` without an executed command behind
# them. The sanctioned paths are:
#   1. `record_guarded "<label>" "<detail>" "<rc>"` — records passed only for rc 0;
#   2. `ssu2_row "<label>" "<key>" "<detail>"` — records passed only for
#      driver rc 0 plus the row's own sanitized evidence keys in
#      driver-evidence.tsv.
# A literal `record "<required-label>" passed` line (indented or not) means a
# row was hard-coded and fails this check. `failed` literals are fail-closed
# and permitted.
#
# Guarded labels (Plan 161 §13 final-row set):
#   local-ssu2-real-udp, local-loss-reorder, path-validation,
#   transport-selection, peer-test, relay, external-i2pd-i2pr-to-i2pd,
#   external-i2pd-i2pd-to-i2pr, external-i2np-small,
#   external-i2np-fragmented, token-retry, malformed-cheap-drop,
#   resource-baseline, plan155-160-focused-regressions, workspace-gates.
#
# Usage: bash scripts/check-ssu2-acceptance-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS="${REPO_ROOT}/tests/integration/ssu2/run-independent.sh"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"

GUARDED=(
  local-ssu2-real-udp
  local-loss-reorder
  path-validation
  transport-selection
  peer-test
  relay
  external-i2pd-i2pr-to-i2pd
  external-i2pd-i2pd-to-i2pr
  external-i2np-small
  external-i2np-fragmented
  token-retry
  malformed-cheap-drop
  resource-baseline
  plan155-160-focused-regressions
  workspace-gates
)

failures=0

if [[ ! -f "${HARNESS}" ]]; then
  echo "evidence check failed: harness missing: ${HARNESS}" >&2
  exit 1
fi

# 1. No literal unconditional pass records for guarded rows.
for label in "${GUARDED[@]}"; do
  if grep -n -E "^[[:space:]]*record \"${label}\" passed" "${HARNESS}"; then
    echo "evidence check failed: literal passed record for required row '${label}' (must flow through record_guarded)" >&2
    failures=$((failures + 1))
  fi
  # 2. Every guarded row must have a command-derived call site, either
  # direct (record_guarded) or per-evidence-key (ssu2_row, which wraps
  # record_guarded with the driver rc plus the row's own evidence keys).
  if ! grep -q -E "record_guarded \"${label}\"" "${HARNESS}" &&
     ! grep -q -E "ssu2_row \"${label}\"" "${HARNESS}"; then
    echo "evidence check failed: required row '${label}' has no record_guarded/ssu2_row call site" >&2
    failures=$((failures + 1))
  fi
done

# 3. The record_guarded helper itself must gate on the exit code.
if ! grep -q -E 'if \[\[ "\$\{rc\}" -eq 0 \]\]; then' "${HARNESS}"; then
  echo "evidence check failed: record_guarded helper lost its exit-code gate" >&2
  failures=$((failures + 1))
fi

# 4. External rows must additionally require their own evidence keys.
if ! grep -q -E 'grep -Fq "\$\{key\}" "\$\{DRIVER_TSV\}"' "${HARNESS}"; then
  echo "evidence check failed: ssu2_row lost its per-row evidence-key gate" >&2
  failures=$((failures + 1))
fi

# 5. The external driver must be selected explicitly as an ignored test.
if ! grep -q -F -- '--ignored --exact' "${HARNESS}"; then
  echo "evidence check failed: harness lost the explicit --ignored --exact external selection" >&2
  failures=$((failures + 1))
fi

# 6. The exact mandatory i2pd pin must be verified before provisioning.
if ! grep -q -F "${I2PD_PIN}" "${HARNESS}"; then
  echo "evidence check failed: harness lost the exact i2pd pin ${I2PD_PIN}" >&2
  failures=$((failures + 1))
fi

# 7. The lane must stay loopback-only and must not forgive failures.
if ! grep -q -F '127.0.0.1' "${HARNESS}"; then
  echo "evidence check failed: harness lost its loopback bind policy" >&2
  failures=$((failures + 1))
fi
if grep -n -E 'cargo test .*ssu2_independent.*\|\| true' "${HARNESS}"; then
  echo "evidence check failed: external driver invocation must not be forgiven with || true" >&2
  failures=$((failures + 1))
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi
echo "SSU2 acceptance evidence integrity: ${#GUARDED[@]} rows command-derived, no literal pass records"
