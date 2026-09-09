#!/usr/bin/env bash
# Plan 170 §9 — static evidence-integrity check for the I2CP independent-
# clients lane.
#
# Rejects known dangerous bookkeeping in
# tests/integration/i2cp/run-independent.sh: required acceptance rows
# recorded `passed` without an executed command behind them. The
# sanctioned path is:
#   `record_guarded "<label>" "<detail>" "<rc>"` — records passed only
#   for rc 0;
# Matrix rows compose their label inside run_direction_a/run_direction_b
# as `record_guarded "external-${row}"` with small|large tags; the
# remaining rows use literal record_guarded call sites.
# No literal `record "<required-label>" passed` line is permitted.
#
# Guarded labels (Plan 170 §5/§7/§9 final-row set):
#   external-java-to-go-small, external-java-to-go-large,
#   external-go-to-java-small, external-go-to-java-large,
#   external-message-status-semantics, external-bandwidth-query,
#   plan167-169-focused-regressions, workspace-gates,
#   external-clean-resource-baseline.
#
# Usage: bash scripts/check-i2cp-acceptance-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS="${REPO_ROOT}/tests/integration/i2cp/run-independent.sh"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
GO_PIN="b529ee1c10a6011558b4d69fc9436a4afc489eac"

GUARDED=(
  external-java-to-go-small
  external-java-to-go-large
  external-go-to-java-small
  external-go-to-java-large
  external-message-status-semantics
  external-bandwidth-query
  plan167-169-focused-regressions
  workspace-gates
  external-clean-resource-baseline
)

# Matrix rows are recorded through run_direction_a/run_direction_b with
# a small|large tag composing the label as "external-${row}". The
# remaining rows use literal record_guarded call sites.
MATRIX_TAGS=(
  "run_direction_a small"
  "run_direction_a large"
  "run_direction_b small"
  "run_direction_b large"
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
done

# 2a. Every matrix tag invocation must exist and both direction
# functions must record through record_guarded with the composed
# external label (exit-code gate enforced by record_guarded itself).
for tag in "${MATRIX_TAGS[@]}"; do
  if ! grep -q -F "${tag}" "${HARNESS}"; then
    echo "evidence check failed: matrix invocation '${tag}' missing from harness" >&2
    failures=$((failures + 1))
  fi
done
for fn in run_direction_a run_direction_b; do
  if ! grep -q -E "^${fn}\(\)" "${HARNESS}"; then
    echo "evidence check failed: direction function '${fn}' missing from harness" >&2
    failures=$((failures + 1))
  fi
done
if ! grep -q -E 'record_guarded "external-\$\{row\}"' "${HARNESS}"; then
  echo "evidence check failed: direction functions lost their composed record_guarded call site" >&2
  failures=$((failures + 1))
fi

# 2b. Every non-matrix guarded row must have a literal
# record_guarded call site.
for label in external-message-status-semantics external-bandwidth-query \
             plan167-169-focused-regressions workspace-gates \
             external-clean-resource-baseline; do
  if ! grep -q -E "record_guarded \"${label}\"" "${HARNESS}"; then
    echo "evidence check failed: required row '${label}' has no record_guarded call site" >&2
    failures=$((failures + 1))
  fi
done

# 3. The record_guarded helper itself must gate on the exit code.
if ! grep -q -E 'if \[\[ "\$\{rc\}" -eq 0 \]\]; then' "${HARNESS}"; then
  echo "evidence check failed: record_guarded helper lost its exit-code gate" >&2
  failures=$((failures + 1))
fi

# 4. Plan 170 §5 digest proof: the harness must cross-compare the
# sender outbound sha against the receiver inbound sha for every
# payload row. A lane that records delivery without digest equality
# is not a §5 matrix.
if ! grep -q -E 'out_sha.*==.*in_sha|in_sha.*==.*out_sha' "${HARNESS}"; then
  echo "evidence check failed: harness lost its sender/receiver digest-equality gate" >&2
  failures=$((failures + 1))
fi
# The strong go-i2cp parse path must be required for direction A:
# a fallback-only delivery without client parsing is not §5 proof.
if ! grep -q -F 'client_parsed_digest' "${HARNESS}"; then
  echo "evidence check failed: harness lost its strong go-i2cp parse-path requirement" >&2
  failures=$((failures + 1))
fi

# 5. The exact Java I2P and go-i2cp pins must be verified before
# any external command runs.
if ! grep -q -F "${JAVA_PIN}" "${HARNESS}"; then
  echo "evidence check failed: harness lost the Java I2P pin ${JAVA_PIN}" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F "${GO_PIN}" "${HARNESS}"; then
  echo "evidence check failed: harness lost the go-i2cp pin ${GO_PIN}" >&2
  failures=$((failures + 1))
fi

# 6. The lane must stay loopback-only and must not forgive command
# failures. The `|| true` forgiveness check is scoped to the actual
# command invocations (cargo / timeout / java / go-driver) and ignores
# the generic `kill ... || true` cleanup helpers that stop_pid, the
# listener trap, and the wait/pid shutdown paths use.
if ! grep -q -F '127.0.0.1' "${HARNESS}"; then
  echo "evidence check failed: harness lost its loopback bind policy" >&2
  failures=$((failures + 1))
fi
# Reject `|| true` forgiveness only when applied to a real command, not
# to the kill/wait cleanup helpers. The pattern is `cmd args || true`
# followed by another shell statement, NOT `kill ... || true` followed
# by `|| kill ... || true` etc.
if grep -n -E '^[^#]*\<(cargo test|timeout .* java|timeout .* \$\{GO_DRIVER\}|cargo fmt|cargo check|bash \[\[ .* \]\]|javac -cp)\b.*\|\| true\b' "${HARNESS}"; then
  echo "evidence check failed: harness forgives a required command via '|| true'" >&2
  failures=$((failures + 1))
fi

# 7. The harness must not silently mask missing required environment.
# (Already covered by 5/6 plus the trap-driven cleanup; this guard
# simply asserts the bail-out lines for missing refs.)
if ! grep -q -F 'run scripts/interop/fetch-i2cp-clients.sh first' "${HARNESS}"; then
  echo "evidence check failed: harness lost its fail-closed reference-cache guard" >&2
  failures=$((failures + 1))
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi
echo "I2CP acceptance evidence integrity: ${#GUARDED[@]} rows command-derived, no literal pass records"
