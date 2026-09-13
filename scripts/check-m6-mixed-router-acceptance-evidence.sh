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
# Plan 194 extends the Plan 189 §8 scaffolding with:
#   tests/integration/m6-interop/run-streaming.sh (Plan 193 first-family)
#   tests/integration/m6-interop/run-java.sh (Plan 194 second-family)
#   scripts/interop/fetch-m6-java.sh (Plan 194 second-family fetch)
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

# Plan 196 §7 — exhaustive list of forbidden short-cuts in the
# controlled-topology lane. The list is intentionally duplicated here
# so a regression that re-introduces any of these fails the static
# check before the runner even starts.
FORBIDDEN_VMCOMM_KEYS=(
  "i2p.vmCommSystem"
)
# Plan 196 §3.3 — obsolete Plan 194 keys that are NOT canonical Java
# I2P property names. The run-java.sh + ControlledRouter.java must
# use exact-pinned upstream names; i2pd-flavored aliases are
# forbidden. Each entry is the bare key; assignments of any value
# (including `=false`, `=true`) are caught by the regex.
OBSOLETE_TOPOLOGY_KEYS=(
  "i2np.reseed.enable"
  "router.isFloodfill"
  "i2np.ntcp2.enabled"
)
# Plan 196 §3.4 — the exact-pinned Java cache/build is reference
# material and must remain immutable across a counted run.
FORBIDDEN_CACHE_FILES=(
  "clients.config"
)

PREFLIGHT_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-preflight.sh"
PREFLIGHT_CHECK="${REPO_ROOT}/scripts/check-exploratory-tunnel-evidence.sh"
TUNNELS_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-tunnels.sh"
TUNNELS_CHECK="${REPO_ROOT}/scripts/check-exploratory-tunnel-evidence.sh"
NETDB_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-netdb.sh"
NETDB_CHECK="${REPO_ROOT}/scripts/check-netdb-tunnel-evidence.sh"
DESTINATION_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-destination.sh"
DESTINATION_CHECK="${REPO_ROOT}/scripts/check-destination-tunnel-evidence.sh"
STREAMING_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-streaming.sh"
STREAMING_CHECK="${REPO_ROOT}/scripts/check-streaming-tunnel-evidence.sh"
JAVA_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-java.sh"
JAVA_FETCH="${REPO_ROOT}/scripts/interop/fetch-m6-java.sh"
JAVA_LAUNCHER_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/ControlledRouter.java"
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
  "${STREAMING_HARNESS}" "${STREAMING_CHECK}" \
  "${JAVA_HARNESS}" "${JAVA_FETCH}" \
  "${JAVA_LAUNCHER_SRC}" \
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
HARNESSES=("${PREFLIGHT_HARNESS}" "${TUNNELS_HARNESS}" "${NETDB_HARNESS}" "${DESTINATION_HARNESS}" "${STREAMING_HARNESS}" "${JAVA_HARNESS}" "${CROSSFAMILY_HARNESS}")
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

# ---- 7. Plan 196 controlled-topology invariants. -------------------------
# The Java second-family lane must not silently forgive failures, must
# never enable VMCommSystem, must never use obsolete i2pd-flavored
# property aliases, must never mutate the verified Java cache, and
# must drive the router through the public stock `Router(Properties)`
# lifecycle via the test-only launcher.
if [[ -f "${JAVA_HARNESS}" ]]; then
  # 7a. No `|| true` / `||:` forgiveness for required commands.
  if grep -n -E '^[^#]*(cargo test|cargo fmt|cargo check|bash \[\[ .* \]\]|javac -cp|java -D).*\|\| true' "${JAVA_HARNESS}" >/dev/null 2>&1; then
    echo "m6 mixed-router evidence check failed: run-java.sh forgives a required command via '|| true' (Plan 196 §7)" >&2
    failures=$((failures + 1))
  fi
  # 7b. The launcher must invoke the stock public Router(Properties)
  # lifecycle; an absent or off-pattern launcher re-introduces the
  # Plan 194 §11 first-run topology race.
  if ! grep -q 'new Router(props)\|new Router(.*)' "${JAVA_LAUNCHER_SRC}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: ControlledRouter.java does not construct net.i2p.router.Router(Properties) (Plan 196 §5.1)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'setKillVMOnEnd' "${JAVA_LAUNCHER_SRC}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: ControlledRouter.java does not call Router.setKillVMOnEnd (Plan 196 §5.1)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'runRouter()' "${JAVA_LAUNCHER_SRC}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: ControlledRouter.java does not call Router.runRouter() (Plan 196 §5.1)" >&2
    failures=$((failures + 1))
  fi
  # 7c. Forbidden JVM property keys. We reject only the assignment
  # form `<key>=<value>` where the value resolves to true/1. The
  # list of keys is permitted (e.g. as comments or in the
  # forbidden-list array) so the runner can document them.
  for forbidden in "${FORBIDDEN_VMCOMM_KEYS[@]}"; do
    # Match Java setProperty("key", "true") and properties-file
    # style `key=true`. Skip pure Java comments (lines starting with
    # `//`) and pure shell comments (lines starting with `#`).
    if grep -n -E "^[[:space:]]*([^/#].*setProperty\([^)]*\"${forbidden}\"[^)]*\"(true|1)\"|[^/#].*${forbidden}[[:space:]]*[:=][[:space:]]*(true|1)\b)" "${JAVA_LAUNCHER_SRC}" "${JAVA_HARNESS}" 2>/dev/null >/dev/null; then
      echo "m6 mixed-router evidence check failed: VMCommSystem property ${forbidden} is enabled (Plan 196 §4 forbidden)" >&2
      failures=$((failures + 1))
    fi
  done
  # 7d. Obsolete Plan 194 topology keys. We reject only the
  # assignment form `<key>=<value>` (or `<key>:<space> value`); the
  # bare key as a string is allowed in explanatory comments and in
  # the forbidden-list array.
  for obsolete in "${OBSOLETE_TOPOLOGY_KEYS[@]}"; do
    # Strip the `=<value>` suffix so we match the bare key form.
    local_key="${obsolete%%=*}"
    # Look for the bare key followed by an `=` and a non-blank
    # value (real assignment, not a comment), skipping Java/shell
    # comments and the forbidden-list array itself. The pattern
    # explicitly excludes the `="value"` form when the line is the
    # forbidden-list declaration (`OBSOLETE_TOPOLOGY_KEYS=` block).
    if grep -n -E "^[[:space:]]*([^/#].*setProperty\([^)]*\"${local_key}\"[^)]*\"|[^/#].*${local_key}[[:space:]]*[:=][[:space:]]*[^[:space:]]+)" "${JAVA_LAUNCHER_SRC}" "${JAVA_HARNESS}" 2>/dev/null \
       | grep -v "OBSOLETE_TOPOLOGY_KEYS=" \
       | grep -v "FORBIDDEN_VMCOMM_KEYS=" \
       | grep -v "^[[:space:]]*//" \
       >/dev/null; then
      echo "m6 mixed-router evidence check failed: obsolete Plan 194 topology key '${obsolete}' assigned (Plan 196 §3.3)" >&2
      failures=$((failures + 1))
    fi
  done
  # 7e. The harness must never `sed` the exact-pinned Java cache's
  # clients.config / clients.config.d. The precomputed patterns
  # reject every common mutation path.
  if grep -n -E '^[^#]*sed[[:space:]]+-[iIn]?[[:space:]].*JAVA_CACHE.*clients\.config' "${JAVA_HARNESS}" 2>/dev/null >/dev/null; then
    echo "m6 mixed-router evidence check failed: run-java.sh mutates the exact-pinned Java cache clients.config (Plan 196 §3.4)" >&2
    failures=$((failures + 1))
  fi
  if grep -n -E '^[^#]*sed[[:space:]]+-[iIn]?[[:space:]].*clients\.config\.d' "${JAVA_HARNESS}" 2>/dev/null >/dev/null; then
    echo "m6 mixed-router evidence check failed: run-java.sh mutates clients.config.d (Plan 196 §3.4)" >&2
    failures=$((failures + 1))
  fi
  # 7f. Reseed URLs must remain loopback-only or absent.
  if grep -n -E 'i2p\.reseedURL[[:space:]]*=[[:space:]]*https?://(?!127\.|localhost)' "${JAVA_LAUNCHER_SRC}" "${JAVA_HARNESS}" 2>/dev/null >/dev/null; then
    echo "m6 mixed-router evidence check failed: non-loopback reseed URL present (Plan 196 §3.2)" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 8. The manual external lane must reference both pins too. ----------
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