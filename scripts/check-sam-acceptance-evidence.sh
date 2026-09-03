#!/usr/bin/env bash
# Plan 151 §5.2 — static evidence-integrity check for the SAM lane.
#
# Rejects known dangerous bookkeeping in tests/integration/sam/run-independent.sh:
# required acceptance rows recorded `passed` without an executed command behind
# them. The sanctioned paths are:
#   1. `record_guarded "<label>" "<detail>" "<rc>"` — records passed only for rc 0;
#   2. `plan151_row "<label>" "<test>" "<detail>"` — records passed only for
#      suite rc 0 plus the row's own captured `test <name> ... ok` line;
#   3. the per-pair dynamic helper `record "${label}" passed ...` inside
#      run_stream_pair, gated on both client exit codes.
# A literal `record "<required-label>" passed` line (indented or not) means a
# row was hard-coded and fails this check. `failed` literals are fail-closed
# and permitted.
#
# Guarded labels (Plan 151 §5.2 plus the §12 final-row set):
#   multiple-stream-lifecycle, slow-reader, slow-writer, fault-data-drop,
#   fault-ack-drop, fault-duplicate, fault-reorder, fault-corruption,
#   fault-retransmit-ceiling, forward-lifecycle, plan127-134-regressions,
#   sibling-stream-isolation, close-reset-lifecycle, workspace-gates,
#   binary-matrix, stream-forward, plan149-self-composed, silent-transcript,
#   naming-transcript, negative-matrix, private-destination-i2psam,
#   private-destination-i2plib-substitute.
#
# Usage: bash scripts/check-sam-acceptance-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS="${REPO_ROOT}/tests/integration/sam/run-independent.sh"

GUARDED=(
  multiple-stream-lifecycle
  slow-reader
  slow-writer
  fault-data-drop
  fault-ack-drop
  fault-duplicate
  fault-reorder
  fault-corruption
  fault-retransmit-ceiling
  forward-lifecycle
  plan127-134-regressions
  sibling-stream-isolation
  close-reset-lifecycle
  workspace-gates
  binary-matrix
  stream-forward
  plan149-self-composed
  silent-transcript
  naming-transcript
  negative-matrix
  private-destination-i2psam
  private-destination-i2plib-substitute
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
  # direct (record_guarded) or per-test (plan151_row, which wraps
  # record_guarded with the suite rc plus the row's own ok-line).
  if ! grep -q -E "record_guarded \"${label}\"" "${HARNESS}" &&
     ! grep -q -E "plan151_row \"${label}\"" "${HARNESS}"; then
    echo "evidence check failed: required row '${label}' has no record_guarded/plan151_row call site" >&2
    failures=$((failures + 1))
  fi
done

# 3. The record_guarded helper itself must gate on the exit code.
if ! grep -q -E 'if \[\[ "\$\{rc\}" -eq 0 \]\]; then' "${HARNESS}"; then
  echo "evidence check failed: record_guarded helper lost its exit-code gate" >&2
  failures=$((failures + 1))
fi

# 4. The dynamic per-pair helper must stay gated on both client exits.
if ! grep -q -E 'if \[\[ "\$\{connect_rc\}" -eq 0 && "\$\{accept_rc\}" -eq 0 \]\]; then' "${HARNESS}"; then
  echo "evidence check failed: run_stream_pair lost its dual-exit-code gate" >&2
  failures=$((failures + 1))
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi
echo "SAM acceptance evidence integrity: ${#GUARDED[@]} rows command-derived, no literal pass records"
