#!/usr/bin/env bash
# Plan 279 §9 — static evidence-integrity check for the M12 floodfill
# qualification lanes.
#
# Rejects known dangerous bookkeeping in the qualification harnesses:
# required matrix rows recorded `passed` without an executed command
# behind them. The sanctioned paths are:
#   1. `record_guarded "<label>" "<detail>" "<rc>"` — records passed
#      only for rc 0;
#   2. `driver_row "<label>" "<key>" "<detail>"` — records passed only
#      for driver rc 0 plus the row's own sanitized evidence key in
#      driver-evidence.tsv.
# A literal `record "<required-label>" passed` line (indented or not)
# means a row was hard-coded and fails this check. `failed` literals
# are fail-closed and permitted.
#
# Structural gates: explicit `--ignored --exact` external selection,
# exact reference pins verified before provisioning, loopback-only
# bind policy, frozen single-attempt budget, no `|| true` forgiveness
# on driver invocations, no reference patching.
#
# Usage:
#   bash scripts/check-m12-floodfill-qualification-evidence.sh [--self-test]
#
# `--self-test` additionally proves the gates against synthetic
# fixtures: a clean mini-harness must pass and a mini-harness with a
# literal pass record plus a forgiven driver invocation must fail.

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
I2PD_HARNESS="${REPO_ROOT}/tests/integration/floodfill/run-i2pd.sh"
I2PD_PIN="635b013a612ff47278ef02acf8580a28e10e26c5"
I2PD_VERSION="2.61.0"

# Guarded Plan 278/303 matrix rows (one-family i2pd lane).
I2PD_GUARDED=(
  controlled-activation-completed
  external-reference-verified
  external-role-active
  external-publisher-store
  external-store-ack
  external-lookup-answered
  external-replication-direct
  external-no-tunnel-flood
  external-dispatch-clean
  workspace-gates
)

failures=0

fail() {
  echo "evidence check failed: $1" >&2
  failures=$((failures + 1))
}

check_harness() {
  local harness="$1"
  local pin="$2"
  local family="$3"
  shift 3
  local label
  if [[ ! -f "${harness}" ]]; then
    fail "${family} harness missing: ${harness}"
    return
  fi
  # 1. No literal unconditional pass records for guarded rows.
  for label in "$@"; do
    if grep -n -E "^[[:space:]]*record \"${label}\" passed" "${harness}"; then
      fail "literal passed record for required row '${label}' (must flow through record_guarded/driver_row)"
    fi
    # 2. Every guarded row must have a command-derived call site.
    if ! grep -q -E "record_guarded \"${label}\"" "${harness}" &&
       ! grep -q -E "driver_row \"${label}\"" "${harness}"; then
      fail "required row '${label}' has no record_guarded/driver_row call site"
    fi
  done
  # 3. The record_guarded helper itself must gate on the exit code.
  if ! grep -q -E 'if \[\[ "\$\{rc\}" -eq 0 \]\]; then' "${harness}"; then
    fail "record_guarded helper lost its exit-code gate"
  fi
  # 4. driver_row must additionally require the row's evidence key.
  if ! grep -q -F 'grep -Fq "${key}" "${DRIVER_TSV}"' "${harness}"; then
    fail "driver_row lost its per-row evidence-key gate"
  fi
  # 5. The external driver must be selected explicitly as an ignored test.
  if ! grep -q -F -- '--ignored --exact' "${harness}"; then
    fail "harness lost the explicit --ignored --exact external selection"
  fi
  # 6. The exact reference pin must be verified before provisioning.
  if ! grep -q -F "${pin}" "${harness}"; then
    fail "harness lost the exact ${family} pin ${pin}"
  fi
  # 7. The lane must stay loopback-only with a frozen attempt budget
  #    and must not forgive driver failures.
  if ! grep -q -F '127.0.0.1' "${harness}"; then
    fail "harness lost its loopback bind policy"
  fi
  if ! grep -q -E '^MAX_ATTEMPTS=1$' "${harness}"; then
    fail "harness lost its frozen single-attempt budget"
  fi
  if grep -n -E 'cargo test .*floodfill.*\|\| true' "${harness}"; then
    fail "external driver invocation must not be forgiven with || true"
  fi
  # 8. No reference patching or vendoring inside the lane.
  if grep -n -E '^[[:space:]]*(patch|git apply)' "${harness}"; then
    fail "lane must not patch the reference"
  fi
}

check_harness "${I2PD_HARNESS}" "${I2PD_PIN}" "i2pd" "${I2PD_GUARDED[@]}"

# 9. The i2pd version advertisement must be verified, not assumed.
if ! grep -q -F 'I2PD_VERSION' "${I2PD_HARNESS}"; then
  fail "i2pd harness lost its version verification"
fi

if [[ "${1:-}" == "--self-test" ]]; then
  sandbox="$(mktemp -d -t i2pr-m12-evidence-selftest.XXXXXX)"
  trap 'rm -rf "${sandbox}"' EXIT
  # A clean mini-harness carries every structural token and must pass.
  {
    echo 'record_guarded() { # label detail rc'
    echo '  local label="$1" detail="$2" rc="$3"'
    echo '  if [[ "${rc}" -eq 0 ]]; then'
    echo '    printf "%s\t%s\tpassed\t%s\n" "${label}" "planX" "${detail}"'
    echo '  fi'
    echo '}'
    echo 'driver_row() { # label key detail'
    echo '  if [[ "${qualify_rc}" -eq 0 ]] && grep -Fq "${key}" "${DRIVER_TSV}"; then rc=0; else rc=1; fi'
    echo '  record_guarded "${label}" "${detail}" "${rc}"'
    echo '}'
    echo 'I2PD_PIN="SELFTEST"'
    echo 'cargo test --test floodfill_selftest -- --ignored --exact'
    echo 'address4 = 127.0.0.1'
    echo 'MAX_ATTEMPTS=1'
    echo 'record_guarded "probe-row" "detail" "${rc}"'
    echo 'driver_row "probe-row" "probe-key" "detail"'
  } > "${sandbox}/clean.sh"
  # A hostile mini-harness hard-codes a pass and forgives the driver.
  {
    echo 'record_guarded() { # label detail rc'
    echo '  local label="$1" detail="$2" rc="$3"'
    echo '  if [[ "${rc}" -eq 0 ]]; then'
    echo '    printf ok'
    echo '  fi'
    echo '}'
    echo 'I2PD_PIN="SELFTEST"'
    echo 'cargo test --test floodfill_selftest -- --ignored --exact || true'
    echo 'cargo test --test floodfill_selftest -- --ignored --exact || true'
    echo 'address4 = 127.0.0.1'
    echo 'MAX_ATTEMPTS=1'
    echo 'record "probe-row" passed'
  } > "${sandbox}/hostile.sh"
  clean_failures=0
  (
    harness="${sandbox}/clean.sh"
    if grep -n -E "^[[:space:]]*record \"probe-row\" passed" "${harness}"; then
      clean_failures=$((clean_failures + 1))
    fi
    if ! grep -q -E "record_guarded \"probe-row\"" "${harness}"; then
      clean_failures=$((clean_failures + 1))
    fi
    if ! grep -q -F 'grep -Fq "${key}" "${DRIVER_TSV}"' "${harness}"; then
      clean_failures=$((clean_failures + 1))
    fi
    exit "${clean_failures}"
  ) || fail "self-test clean fixture unexpectedly failed its gates"
  hostile_caught=0
  if grep -n -E "^[[:space:]]*record \"probe-row\" passed" "${sandbox}/hostile.sh" >/dev/null; then
    hostile_caught=$((hostile_caught + 1))
  fi
  if grep -n -E 'cargo test .*floodfill.*\|\| true' "${sandbox}/hostile.sh" >/dev/null; then
    hostile_caught=$((hostile_caught + 1))
  fi
  if [[ "${hostile_caught}" -ne 2 ]]; then
    fail "self-test hostile fixture evaded detection (${hostile_caught}/2)"
  fi
  # The hostile harness must also fail the real gate function.
  if ( failures=0
       check_harness "${sandbox}/hostile.sh" "SELFTEST" "selftest" "probe-row"
       [[ "${failures}" -gt 0 ]] ); then
    :
  else
    fail "self-test hostile harness passed check_harness"
  fi
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi
echo "M12 floodfill qualification evidence integrity: ${#I2PD_GUARDED[@]} i2pd rows command-derived, no literal pass records"
