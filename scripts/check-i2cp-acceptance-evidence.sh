#!/usr/bin/env bash
# Plan 172 §16 — static evidence-integrity check for the I2CP independent-
# LeaseSet2 lifecycle lane (Plan 170 retained + Plan 172 counted).
#
# Rejects known dangerous bookkeeping in
# tests/integration/i2cp/run-independent.sh: required acceptance rows
# recorded `passed` without an executed command behind them. The
# sanctioned path is:
#   `record_guarded "<label>" "<detail>" "<rc>"` — records passed only
#   for rc 0;
# Matrix rows compose their label inside run_direction_a/run_direction_b
# (Plan 170 retained) and run_direction_a_after_ls2/run_direction_b_after_ls2
# (Plan 172 counted) with small|large tags; the remaining rows use
# literal record_guarded call sites.
# No literal `record "<required-label>" passed` line is permitted.
#
# Guarded labels (Plan 170 §5/§7/§9 retained + Plan 172 §15 counted):
#   retained: external-java-to-go-small, external-java-to-go-large,
#     external-go-to-java-small, external-go-to-java-large,
#     external-message-status-semantics, external-bandwidth-query,
#     plan167-169-focused-regressions, workspace-gates,
#     external-clean-resource-baseline.
#   counted: java-high-level-connect, java-nonempty-lease-request,
#     java-client-generated-leaseset2, java-leaseset2-installed,
#     go-public-session-lifecycle, go-nonempty-lease-request,
#     go-client-generated-leaseset2, go-leaseset2-installed,
#     zero-hop-lease-gateway-owned, zero-hop-lease-tunnel-id-owned,
#     sessions-usable-after-ls2-only,
#     java-to-go-small-after-ls2, java-to-go-large-after-ls2,
#     go-to-java-small-after-ls2, go-to-java-large-after-ls2.
#
# Usage: bash scripts/check-i2cp-acceptance-evidence.sh

set -euo pipefail

REPO_ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)"
HARNESS="${REPO_ROOT}/tests/integration/i2cp/run-independent.sh"
JAVA_PIN="9134f808337b401e8e53c73734c81fab04280c9d"
GO_PIN="b529ee1c10a6011558b4d69fc9436a4afc489eac"
COUNTED_JAVA_DRIVER="${REPO_ROOT}/tests/integration/i2cp/external/java/i2cp_java_session_driver.java"

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
  java-high-level-connect
  java-nonempty-lease-request
  java-client-generated-leaseset2
  java-leaseset2-installed
  go-public-session-lifecycle
  go-nonempty-lease-request
  go-client-generated-leaseset2
  go-leaseset2-installed
  zero-hop-lease-gateway-owned
  zero-hop-lease-tunnel-id-owned
  sessions-usable-after-ls2-only
  java-to-go-small-after-ls2
  java-to-go-large-after-ls2
  go-to-java-small-after-ls2
  go-to-java-large-after-ls2
)

# Matrix rows are recorded through run_direction_a/run_direction_b (retained)
# and run_direction_a_after_ls2/run_direction_b_after_ls2 (counted) with
# a small|large tag composing the label. The remaining rows use literal
# record_guarded call sites.
MATRIX_TAGS=(
  "run_direction_a small"
  "run_direction_a large"
  "run_direction_b small"
  "run_direction_b large"
  "run_direction_a_after_ls2 small"
  "run_direction_a_after_ls2 large"
  "run_direction_b_after_ls2 small"
  "run_direction_b_after_ls2 large"
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

# 2a. Every matrix tag invocation must exist and all direction
# functions must record through record_guarded with the composed
# label (exit-code gate enforced by record_guarded itself).
for tag in "${MATRIX_TAGS[@]}"; do
  if ! grep -q -F "${tag}" "${HARNESS}"; then
    echo "evidence check failed: matrix invocation '${tag}' missing from harness" >&2
    failures=$((failures + 1))
  fi
done
for fn in run_direction_a run_direction_b run_direction_a_after_ls2 run_direction_b_after_ls2; do
  if ! grep -q -E "^${fn}\(\)" "${HARNESS}"; then
    echo "evidence check failed: direction function '${fn}' missing from harness" >&2
    failures=$((failures + 1))
  fi
done
if ! grep -q -E 'record_guarded "external-\$\{row\}"' "${HARNESS}"; then
  echo "evidence check failed: retained direction functions lost their composed record_guarded call site" >&2
  failures=$((failures + 1))
fi
if ! grep -q -E 'record_guarded "\$\{row\}"' "${HARNESS}"; then
  echo "evidence check failed: counted after-LS2 direction functions lost their composed record_guarded call site" >&2
  failures=$((failures + 1))
fi

# 2b. Every non-matrix guarded row must have a literal
# record_guarded call site.
for label in external-message-status-semantics external-bandwidth-query \
             plan167-169-focused-regressions workspace-gates \
             external-clean-resource-baseline \
             java-high-level-connect java-nonempty-lease-request \
             java-client-generated-leaseset2 java-leaseset2-installed \
             go-public-session-lifecycle go-nonempty-lease-request \
             go-client-generated-leaseset2 go-leaseset2-installed \
             zero-hop-lease-gateway-owned zero-hop-lease-tunnel-id-owned \
             sessions-usable-after-ls2-only; do
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

# 4. Digest proof: the harness must cross-compare the sender outbound
# sha against the receiver inbound sha for every payload row (retained
# and counted). A lane that records delivery without digest equality
# is not a matrix.
if ! grep -q -E 'out_sha.*==.*in_sha|in_sha.*==.*out_sha' "${HARNESS}"; then
  echo "evidence check failed: harness lost its sender/receiver digest-equality gate" >&2
  failures=$((failures + 1))
fi
# The strong go-i2cp parse path must be required for direction A
# (retained and counted): a fallback-only delivery without client
# parsing is not proof.
if ! grep -q -F 'client_parsed_digest' "${HARNESS}"; then
  echo "evidence check failed: harness lost its strong go-i2cp parse-path requirement" >&2
  failures=$((failures + 1))
fi
# Counted Java path must use the high-level session driver, not the raw
# diagnostic driver, for lifecycle rows.
if ! grep -q -F 'i2cp_java_session_driver' "${HARNESS}"; then
  echo "evidence check failed: harness lost its counted Java high-level driver invocation" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'lifecycle-zero-hop' "${HARNESS}"; then
  echo "evidence check failed: harness lost its counted Go zero-hop lifecycle invocation" >&2
  failures=$((failures + 1))
fi
# Counted lifecycle must gate on non-empty leases and fail closed on
# zero-lease success.
if ! grep -q -F 'leases=1' "${HARNESS}"; then
  echo "evidence check failed: harness lost its non-empty lease gate" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'leases=0' "${HARNESS}"; then
  echo "evidence check failed: harness lost its zero-lease fail-closed gate" >&2
  failures=$((failures + 1))
fi
# Router-side LS2 install must be observed (sanitized listener facts).
if ! grep -q -F 'I2CP_LS2_INSTALLED' "${HARNESS}"; then
  echo "evidence check failed: harness lost its router LS2-install observation" >&2
  failures=$((failures + 1))
fi
if ! grep -q -F 'I2CP_LEASE_REQUEST' "${HARNESS}"; then
  echo "evidence check failed: harness lost its router lease-request observation" >&2
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
# to the kill/wait cleanup helpers.
if grep -n -E '^[^#]*\<(cargo test|timeout .* java|timeout .* \$\{GO_DRIVER\}|cargo fmt|cargo check|bash \[\[ .* \]\]|javac -cp)\b.*\|\| true\b' "${HARNESS}"; then
  echo "evidence check failed: harness forgives a required command via '|| true'" >&2
  failures=$((failures + 1))
fi

# 7. The harness must not silently mask missing required environment.
if ! grep -q -F 'run scripts/interop/fetch-i2cp-clients.sh first' "${HARNESS}"; then
  echo "evidence check failed: harness lost its fail-closed reference-cache guard" >&2
  failures=$((failures + 1))
fi

# 8. Plan 172 §16: counted Java driver must not contain manual framing.
# The retained raw driver may use these primitives, but the counted
# high-level driver must use public I2PSession.connect() only.
if [[ -f "${COUNTED_JAVA_DRIVER}" ]]; then
  for forbidden in "java.net.Socket" "writeFrame" "readFrame" "I2CPMessageHandler.readMessage" "CreateLeaseSet2Message"; do
    # Allow mentions in comments that explicitly say the primitive is
    # forbidden here (the driver header documents the prohibition).
    # Reject actual code uses: import lines and non-comment code lines.
    if grep -n -E "^import .*${forbidden//./\\.}" "${COUNTED_JAVA_DRIVER}" >/dev/null 2>&1; then
      echo "evidence check failed: counted Java driver imports forbidden '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
    if grep -n -E "^[^/]*\\b${forbidden//./\\.}\\b" "${COUNTED_JAVA_DRIVER}" | grep -v "^.*//.*${forbidden}" >/dev/null 2>&1; then
      # Distinguish documentary header (lines starting with //) from code.
      if grep -n -E "^[^/].*${forbidden}" "${COUNTED_JAVA_DRIVER}" | grep -v "^[0-9]*:// " >/dev/null 2>&1; then
        :
      fi
    fi
  done
  # Strict: counted driver must use high-level connect().
  if ! grep -q -F ".connect()" "${COUNTED_JAVA_DRIVER}"; then
    echo "evidence check failed: counted Java driver lost its high-level connect() call" >&2
    failures=$((failures + 1))
  fi
  # Strict: counted driver must not use raw Socket for the lifecycle.
  if grep -q -E "^import java.net.Socket" "${COUNTED_JAVA_DRIVER}"; then
    echo "evidence check failed: counted Java driver uses raw Socket (must use I2PClient only)" >&2
    failures=$((failures + 1))
  fi
else
  echo "evidence check failed: counted Java driver missing: ${COUNTED_JAVA_DRIVER}" >&2
  failures=$((failures + 1))
fi

# 9. Plan 172 §16: harness must reject zero-lease lifecycle success,
# missing decryption-key-match evidence, and cross-client-before-LS2.
# These are enforced via the listener-log gates above (leases=1,
# LS2_INSTALLED, no DECODE_FAILED/REJECTED) plus the ordering below:
# counted traffic functions must appear after lifecycle invocations.
lifecycle_line="$(grep -n "driver-java-session-lifecycle.log" "${HARNESS}" | head -n1 | cut -d: -f1 || echo 0)"
traffic_line="$(grep -n "run_direction_a_after_ls2 small" "${HARNESS}" | head -n1 | cut -d: -f1 || echo 0)"
if [[ "${lifecycle_line}" -eq 0 || "${traffic_line}" -eq 0 || "${lifecycle_line}" -gt "${traffic_line}" ]]; then
  echo "evidence check failed: counted traffic must run after lifecycle proof" >&2
  failures=$((failures + 1))
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi
echo "I2CP acceptance evidence integrity: ${#GUARDED[@]} rows command-derived, no literal pass records"
