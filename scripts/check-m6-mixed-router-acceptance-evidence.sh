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
JAVA_RAW_HELPER_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceRawDestination.java"
JAVA_STREAM_HELPER_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceStreamingService.java"
CROSSFAMILY_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-m6-mixed-router.sh"
FINAL_CLOSURE_CHECK="${REPO_ROOT}/scripts/check-m6-final-closure-evidence.sh"

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
  "${JAVA_RAW_HELPER_SRC}" "${JAVA_STREAM_HELPER_SRC}" \
  "${CROSSFAMILY_HARNESS}" "${FINAL_CLOSURE_CHECK}"; do
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
  for helper in "${JAVA_RAW_HELPER_SRC}" "${JAVA_STREAM_HELPER_SRC}"; do
    if ! grep -q 'I2PClientFactory\|I2PSocketManagerFactory' "${helper}"; then
      echo "m6 mixed-router evidence check failed: Plan 199 helper lacks public Java client API use: ${helper}" >&2
      failures=$((failures + 1))
    fi
    if grep -n -E 'net\.i2p\.router|I2CPMessage|SAMBridge|Garlic|StreamingPacket' "${helper}" >/dev/null 2>&1; then
      echo "m6 mixed-router evidence check failed: Plan 199 helper calls private/router or wire-level APIs: ${helper}" >&2
      failures=$((failures + 1))
    fi
  done
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
    echo "m6 mixed-router evidence check failed: external workflow never references the Java pin ${JAVA_PIN}" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'workflow_dispatch' "${WORKFLOW}"; then
    echo "m6 mixed-router evidence check failed: external workflow must be workflow_dispatch only (routine CI must never start Java)" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 9. Plan 197 PQ SSU2 option tolerance invariants. -------------------
# The exact-pinned Java I2P 2.13.0 reference publishes `pq=4,3` for
# every SSU2 RouterAddress via `UDPTransport.addSSU2Options`. Plan 197
# adds typed parser tolerance so the M6 second-family lane stops at
# §10.B and flips the external-session-established-java row from
# failed to passed. These guards ensure the parser tolerance is
# permanent and the i2pr publication path remains pq-free.
SSU2_ADDRESS="${REPO_ROOT}/crates/i2pr-transport-ssu2/src/address.rs"
SSU2_PUBLICATION="${REPO_ROOT}/crates/i2pr-transport-ssu2/src/publication.rs"
SSU2_LIB="${REPO_ROOT}/crates/i2pr-transport-ssu2/src/lib.rs"
if [[ ! -f "${SSU2_ADDRESS}" ]]; then
  echo "m6 mixed-router evidence check failed: ${SSU2_ADDRESS} missing (Plan 197 §8)" >&2
  failures=$((failures + 1))
elif [[ ! -f "${SSU2_PUBLICATION}" ]]; then
  echo "m6 mixed-router evidence check failed: ${SSU2_PUBLICATION} missing (Plan 197 §8)" >&2
  failures=$((failures + 1))
elif [[ ! -f "${SSU2_LIB}" ]]; then
  echo "m6 mixed-router evidence check failed: ${SSU2_LIB} missing (Plan 197 §8)" >&2
  failures=$((failures + 1))
else
  # 9a. The PQ_OPTION parser arm must be present and must NOT fall
  # through to the unknown-option default; a positive parse of
  # `pq=4,3` is required via an in-tree unit test.
  if ! grep -q 'PQ_OPTION\s*=>\s*store' "${SSU2_ADDRESS}"; then
    echo "m6 mixed-router evidence check failed: ${SSU2_ADDRESS} lacks the PQ_OPTION parser arm (Plan 197 §5.2)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'parses_java_high_mtu_pq_options\|parses_java_low_mtu_pq_option' "${SSU2_ADDRESS}"; then
    echo "m6 mixed-router evidence check failed: ${SSU2_ADDRESS} lacks the positive pq=4,3 unit-test row (Plan 197 §5.5)" >&2
    failures=$((failures + 1))
  fi
  # 9b. The Ssu2RouterAddress must surface a typed `pq_capabilities()`
  # accessor; the field is required for evidence logging.
  if ! grep -q 'fn pq_capabilities' "${SSU2_ADDRESS}"; then
    echo "m6 mixed-router evidence check failed: ${SSU2_ADDRESS} lacks the pq_capabilities() accessor (Plan 197 §5.3)" >&2
    failures=$((failures + 1))
  fi
  # 9c. The publication path must remain pq-free. We reject only the
  # wire-emit form (setProperty("pq",...) or key == "pq" branch).
  # Comments referencing the policy are allowed; the literal
  # `publication_never_emits_pq` regression test name is allowed.
  if grep -n -E '^[[:space:]]*([^/].*setProperty\([^)]*"pq"[^)]*|key[[:space:]]*==[[:space:]]*"pq")' "${SSU2_PUBLICATION}" >/dev/null 2>&1; then
    echo "m6 mixed-router evidence check failed: ${SSU2_PUBLICATION} emits a pq key (Plan 197 §5.4 / §11 ruled-out work)" >&2
    failures=$((failures + 1))
  fi
  # 9d. The lib.rs re-export must include Ssu2PqKem and PqCapabilities
  # (and the typed bound MAX_SSU2_PQ_SCHEMES).
  for reexport in "Ssu2PqKem" "PqCapabilities" "MAX_SSU2_PQ_SCHEMES"; do
    if ! grep -q "${reexport}" "${SSU2_LIB}"; then
      echo "m6 mixed-router evidence check failed: ${SSU2_LIB} does not re-export ${reexport} (Plan 197 §5.1)" >&2
      failures=$((failures + 1))
    fi
  done
fi

# ---- 10. Plan 200 Java public-client publication observability --------
# Plan 200 is a diagnostic/evidence corrective: the lane must
# (a) stop equating helper readiness with LeaseSet2 publication,
# (b) prove Router A/B main-NetDB bootstrap through an ordinary
#     post-store DatabaseLookup round-trip in both directions,
# (c) observe sanitized Java client LeaseSet2 lifecycle facts,
# (d) emit exactly one terminal `P200-{A..H}` classification per run,
# (e) reject a publication row that lacks the post-store RouterInfo
#     lookup proofs.
if [[ -f "${JAVA_RAW_HELPER_SRC}" && -f "${JAVA_STREAM_HELPER_SRC}" ]]; then
  # 10a. The helper `READY` line must NOT include `leaseset=published`.
  # The new format must carry the bounded Plan 200 §A.1 facts
  # (`PUBLIC_CLIENT_SESSION_CONNECTED`, `PUBLIC_CLIENT_DESTINATION_LEN`,
  # `PUBLIC_CLIENT_CONTROL_READY`) and must NOT claim publication.
  for helper in "${JAVA_RAW_HELPER_SRC}" "${JAVA_STREAM_HELPER_SRC}"; do
    if ! grep -q 'PUBLIC_CLIENT_SESSION_CONNECTED' "${helper}"; then
      echo "m6 mixed-router evidence check failed: ${helper} lacks the Plan 200 §A.1 PUBLIC_CLIENT_SESSION_CONNECTED fact" >&2
      failures=$((failures + 1))
    fi
    if ! grep -q 'PUBLIC_CLIENT_DESTINATION_LEN' "${helper}"; then
      echo "m6 mixed-router evidence check failed: ${helper} lacks the Plan 200 §A.1 PUBLIC_CLIENT_DESTINATION_LEN fact" >&2
      failures=$((failures + 1))
    fi
    if ! grep -q 'PUBLIC_CLIENT_CONTROL_READY' "${helper}"; then
      echo "m6 mixed-router evidence check failed: ${helper} lacks the Plan 200 §A.1 PUBLIC_CLIENT_CONTROL_READY fact" >&2
      failures=$((failures + 1))
    fi
    # The bounded REPORT_STATUS command must be present so the harness
    # can request the explicit helper-local fact set.
    if ! grep -q 'REPORT_STATUS' "${helper}"; then
      echo "m6 mixed-router evidence check failed: ${helper} lacks the Plan 200 §A.2 REPORT_STATUS command" >&2
      failures=$((failures + 1))
    fi
    # Plan 200 §A.1 — reject any claim of helper-side
    # `leaseset=published`; the Rust driver must no longer mix
    # publication into readiness.
    if grep -q 'leaseset=published' "${helper}"; then
      echo "m6 mixed-router evidence check failed: ${helper} retains helper-side leaseset=published claim (Plan 200 §A.1 forbidden)" >&2
      failures=$((failures + 1))
    fi
  done
  # 10b. The Rust driver must NOT mix `leaseset=published` into any
  # helper-readiness evidence row.
  DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
  if [[ -f "${DRIVER_TEST}" ]]; then
    if grep -nE 'session=connected leaseset=published' "${DRIVER_TEST}" >/dev/null 2>&1; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST} still emits helper-side leaseset=published (Plan 200 §A.2 forbidden)" >&2
      failures=$((failures + 1))
    fi
  fi
  # 10c. The driver must support post-bootstrap RouterInfo lookup
  # probes in both directions (`p200-routerinfo-lookup-a-knows-b`
  # and `p200-routerinfo-lookup-b-knows-a`) and emit one terminal
  # `P200-*` classification per run.
  if [[ -f "${DRIVER_TEST}" ]]; then
    if ! grep -q 'probe_routerinfo_lookup' "${DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST} lacks the Plan 200 §B post-bootstrap RouterInfo lookup probe (probe_routerinfo_lookup)" >&2
      failures=$((failures + 1))
    fi
    if ! grep -q 'record_p200_classification' "${DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST} lacks the Plan 200 §11 P200-* terminal classification helper" >&2
      failures=$((failures + 1))
    fi
    if ! grep -q '"a-knows-b"' "${DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST} lacks the Plan 200 §B a-knows-b probe label" >&2
      failures=$((failures + 1))
    fi
    if ! grep -q '"b-knows-a"' "${DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST} lacks the Plan 200 §B b-knows-a probe label" >&2
      failures=$((failures + 1))
    fi
    if ! grep -q 'database_lookup_router_info_wire' "${DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST} lacks the Plan 200 §B database_lookup_router_info_wire helper" >&2
      failures=$((failures + 1))
    fi
    # Plan 217 §6.A — the destination driver must record the
    # transfer-once invariant. The Plan 216 panic was caused by a
    # duplicate post-lookup block that re-asserted
    # `outbound_len() == 1` and re-ran `remove_outbound` after the
    # slot had already been transferred into a
    # `DestinationOutboundRole`. The driver MUST emit
    # `destination-outbound-transferred` and MUST NOT contain a
    # second `remove_outbound(outbound_slot)` call after the
    # initial transfer in `destination_message_plane_against_java`.
    if ! grep -q 'destination-outbound-transferred' "${DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST} lacks the Plan 217 §6.A destination-outbound-transferred evidence key" >&2
      failures=$((failures + 1))
    fi
    # Plan 217 §6.D — disjoint streaming namespace. The streaming
    # driver MUST use its own `STREAM_*` constants for build/tunnel
    # identifiers and MUST NOT collide with the destination driver's
    # `0x51A7_5xxx` / `0x51A7_6xxx` message-id namespace.
    if ! grep -q 'STREAM_OUTBOUND_CREATOR' "${DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST} lacks the Plan 217 §6.D STREAM_OUTBOUND_CREATOR constant (disjoint streaming namespace)" >&2
      failures=$((failures + 1))
    fi
    if ! grep -q 'STREAM_IBGW_RECEIVE' "${DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST} lacks the Plan 217 §6.D STREAM_IBGW_RECEIVE constant" >&2
      failures=$((failures + 1))
    fi
    # Plan 217 §9 — the harness MUST expose a bounded
    # `I2PR_M6_JAVA_DRIVER` selector so the destination and
    # Streaming sub-runs can be invoked independently during
    # diagnosis without duplicating the Java-router topology.
    if ! grep -q 'I2PR_M6_JAVA_DRIVER' "${REPO_ROOT}/tests/integration/m6-interop/run-java.sh"; then
      echo "m6 mixed-router evidence check failed: run-java.sh lacks the Plan 217 §9 I2PR_M6_JAVA_DRIVER selector" >&2
      failures=$((failures + 1))
    fi
    # Plan 217 §6.B.6 — final-snapshot classification. The shell
    # must read the LAST `p200-classification` occurrence (after
    # helper readiness), not the first emission.
    if grep -qE 'p200-classification.*\bexit\b' "${REPO_ROOT}/tests/integration/m6-interop/run-java.sh"; then
      echo "m6 mixed-router evidence check failed: run-java.sh still uses first-occurrence awk for P200 classification (Plan 217 §6.B.6)" >&2
      failures=$((failures + 1))
    fi
    # Plan 217 §6.C — the controlled launcher must declare a
    # relative `router.networkDatabase.dbDir`, never an absolute
    # path that triggers the documented doubled-path bug.
    if grep -qE 'router\.networkDatabase\.dbDir[^\n]*getAbsolutePath' "${REPO_ROOT}/tests/integration/m6-interop/java/ControlledRouter.java"; then
      echo "m6 mixed-router evidence check failed: ControlledRouter.java still uses absolute router.networkDatabase.dbDir (Plan 217 §6.C)" >&2
      failures=$((failures + 1))
    fi
  fi
  # 10d. The harness must wire the Plan 200 row set and the
  # bootstrap-classification aggregator.
  if [[ -f "${JAVA_HARNESS}" ]]; then
    for row in \
      external-routerinfo-lookup-a-knows-b \
      external-routerinfo-lookup-b-knows-a \
      external-java-client-subdb-created \
      external-java-create-leaseset2-received \
      external-java-client-leaseset-stored-current \
      external-java-client-leaseset-publish-scheduled \
      external-java-client-leaseset-republish-job-ran \
      external-java-client-inbound-tunnel-eligible \
      external-java-client-outbound-tunnel-eligible \
      external-java-floodfill-candidate-available \
      external-java-store-emitted \
      external-java-store-ack-observed \
      external-p200-classification; do
      if ! grep -q "${row}" "${JAVA_HARNESS}"; then
        echo "m6 mixed-router evidence check failed: run-java.sh missing Plan 200 row ${row}" >&2
        failures=$((failures + 1))
      fi
    done
    # The diagnostic Java log keys must be greppable from the
    # helper-side counts (Plan 200 §C/D).
    for key in \
      java-client-subdb-created \
      java-create-leaseset2-received \
      java-client-leaseset-stored-current \
      java-client-leaseset-publish-scheduled \
      java-client-leaseset-republish-job-ran \
      java-client-inbound-tunnel-selectable \
      java-client-outbound-tunnel-selectable \
      java-floodfill-candidate-non-empty \
      java-store-emitted \
      java-store-ack-observed \
      java-store-failure-reason; do
      if ! grep -q "${key}" "${JAVA_HARNESS}"; then
        echo "m6 mixed-router evidence check failed: run-java.sh missing Plan 200 diagnostic key ${key}" >&2
        failures=$((failures + 1))
      fi
    done
    # Plan 217 §6.B — the harness MUST also emit the diagnostic-only
    # negative observation keys (each `No …` pattern) so a future
    # pass cannot silently re-fold a negative string into a positive
    # row. These keys are NEVER satisfied by the "No …" pattern
    # itself; they exist so the static check can reject the mixed
    # grep that the Plan 216 diagnostic flagged.
    for key in \
      java-client-inbound-tunnel-unavailable \
      java-client-outbound-tunnel-unavailable \
      java-floodfill-candidate-empty; do
      if ! grep -q "${key}" "${JAVA_HARNESS}"; then
        echo "m6 mixed-router evidence check failed: run-java.sh missing Plan 217 §6.B negative-observation key ${key}" >&2
        failures=$((failures + 1))
      fi
    done
    # Plan 217 §6.B — the positive-only greps MUST NOT contain the
    # "No …" failure pattern. The static check rejects mixed greps
    # so a future rewrite cannot silently satisfy a positive row
    # with a negative diagnostic.
    if grep -nE 'java-(client-inbound-tunnel-selectable|client-outbound-tunnel-selectable|floodfill-candidate-non-empty)' "${JAVA_HARNESS}" \
       | grep -qE 'No (inbound|outbound|floodfill|peers|more peers)'; then
      echo "m6 mixed-router evidence check failed: run-java.sh positive row still consumes 'No …' pattern (Plan 217 §6.B step 3)" >&2
      failures=$((failures + 1))
    fi
  fi
fi

# ---- 11. Plan 201 Branch G diagnostic observation surface. ----------
# Plan 201 is the bounded corrective/closure pass for the Java I2P
# 2.13.0 second-family M6 lane. It may only run after Plan 200 records
# exactly one terminal `P200-*` classification. The most likely
# classification given Plan 198/199 evidence (LS2 stored but
# network-invisible) is `P200-G-store-acked-remote-lookup-fails`.
#
# Until Plan 200 actually runs end-to-end against the exact-pinned
# Java cache, the Plan 201 implementation lands as a defensive
# observation surface: the runtime exposes a typed
# `note_lookup_boundary` helper so the external driver can attribute a
# stuck Branch G lookup to a specific documented boundary without
# silently advancing a counter. The static checker below rejects
# unknown boundary labels so the documented set stays exhaustive.
DESTINATION_TUNNELS_SRC="${REPO_ROOT}/crates/i2pr-daemon/src/destination_tunnels.rs"
if [[ -f "${DESTINATION_TUNNELS_SRC}" ]]; then
  # 11a. `note_lookup_boundary` is the public Branch G observation
  # surface and must exist in the daemon-owned destination coordinator.
  if ! grep -q 'fn note_lookup_boundary' "${DESTINATION_TUNNELS_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${DESTINATION_TUNNELS_SRC} lacks the Plan 201 §G note_lookup_boundary helper" >&2
    failures=$((failures + 1))
  fi
  # 11b. The Branch G observation surface must cover every documented
  # boundary label the external driver is allowed to report. Adding a
  # new documented label requires both the helper implementation and
  # a parallel grep row in this checker.
  for label in \
    floodfill-selection \
    reply-gateway \
    ls2-key-match \
    database-store-ls2-decode \
    inbound-tunnel-reassembly; do
    if ! grep -q "\"${label}\"" "${DESTINATION_TUNNELS_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${DESTINATION_TUNNELS_SRC} lacks the Plan 201 §G documented label '${label}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 11c. The Branch G counter surface must include every typed
  # observation pair. A future expansion of the documented set must
  # update both this checker and the helper.
  for counter in \
    floodfill_candidates_present \
    floodfill_candidates_absent \
    reply_paths_derived \
    reply_paths_unresolved \
    lookup_key_matches \
    lookup_key_mismatches \
    ls2_records_decoded \
    ls2_records_decode_rejected \
    ls2_records_signature_rejected \
    inbound_cells_garlic_completed \
    inbound_cells_garlic_incomplete; do
    if ! grep -q "pub ${counter}:" "${DESTINATION_TUNNELS_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${DESTINATION_TUNNELS_SRC} lacks the Plan 201 §G counter '${counter}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

# ---- 12. Plan 201 Branch G unit coverage. -----------------------------
# The Branch G observation surface must be covered by unit tests in
# the destination tunnel unit suite so a future regression cannot
# silently advance a counter. Each row maps to a documented
# boundary label; the test body uses the public
# `note_lookup_boundary` helper.
DESTINATION_TUNNEL_UNIT_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/destination_tunnel_unit.rs"
if [[ -f "${DESTINATION_TUNNEL_UNIT_TEST}" ]]; then
  for row in \
    plan201_g_floodfill_candidates_absent_counter_increments \
    plan201_g_floodfill_candidates_present_counter_increments \
    plan201_g_lookup_key_match_counter_advances_on_happy_path \
    plan201_g_lookup_key_mismatch_counter_advances \
    plan201_g_signature_rejected_counter_advances_on_tampered_ls2 \
    plan201_g_decode_rejected_counter_advances_on_non_ls2_body \
    plan201_g_note_lookup_boundary_recognises_documented_set \
    plan201_g_note_lookup_boundary_rejects_unknown_labels; do
    if ! grep -q "fn ${row}" "${DESTINATION_TUNNEL_UNIT_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${DESTINATION_TUNNEL_UNIT_TEST} lacks the Plan 201 §G unit row '${row}'" >&2
      failures=$((failures + 1))
    fi
  done
  # Plan 217 §6.A — the transfer-once invariant for an installed
  # outbound role MUST be locked by a unit row so the Plan 216
  # duplicate-block regression cannot return without an immediate
  # local test failure (the external Java run is not required to
  # catch this).
  if ! grep -q 'fn plan217_outbound_role_transfer_once_invariant' "${DESTINATION_TUNNEL_UNIT_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${DESTINATION_TUNNEL_UNIT_TEST} lacks the Plan 217 §6.A transfer-once unit row 'plan217_outbound_role_transfer_once_invariant'" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 14. Plan 220 corrected-diagnostic observation surface ---------------
# Plan 219's J219-{A..J} attribution is superseded (D220-1..D220-9).
# The corrected path observes tri-state P220 facts inside the
# destination driver at its authoritative post-bootstrap epoch,
# with protocol-correct hex RouterHash identity, exact
# stored-RouterInfo evidence, a read-only same-package selector
# probe distinct from PeerManager membership, and directionally
# correct dispatch evidence. The static checker enforces that:
#   - no Plan 219-only classification surface remains in the
#     daemon-owned production coordinator (D220-9);
#   - the driver owns the P220 tri-state classifier with explicit
#     Unknown/observability-gap behavior and consumes no
#     pre-bootstrap shell fact (D220-1);
#   - no raw RouterInfo byte slicing or standard-Base64 RouterHash
#     construction remains on the diagnostic path (D220-2);
#   - stored-B-`f` proves exact identity plus `f`, never
#     presence alone (D220-4);
#   - selector output is never synthesized from PeerManager
#     membership (D220-5);
#   - client-NetDB/OCMOSJ facts are observed or Unknown, never
#     hard-coded/defaulted booleans (D220-6);
#   - forward `reference-received` never satisfies reverse
#     dispatch (D220-7).
# The J219 diagnostic commands stay frozen in the launcher for
# history; only the P220 hex-hash commands are authoritative.
DESTINATION_TUNNELS_SRC_220="${DESTINATION_TUNNELS_SRC}"
DRIVER_TEST_220="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
LAUNCHER_SRC_220="${JAVA_LAUNCHER_SRC}"
SELECTOR_PROBE_SRC_220="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P220SelectorProbe.java"
HARNESS_220="${JAVA_HARNESS}"

if [[ -f "${DESTINATION_TUNNELS_SRC_220}" ]]; then
  # 14a. The Plan 219-only production surface MUST be gone
  # (D220-9): no per-stage typed-fact counters, no aggregator,
  # no terminal enum, no typed-fact writer, no J219 classifier.
  for forbidden in \
    'j219_' \
    'J219TypedFacts' \
    'J219Terminal' \
    'note_j219_typed_fact' \
    'derive_j219_terminal_classification'; do
    if grep -q "${forbidden}" "${DESTINATION_TUNNELS_SRC_220}"; then
      echo "m6 mixed-router evidence check failed: ${DESTINATION_TUNNELS_SRC_220} retains Plan 219-only surface '${forbidden}' (Plan 220 D220-9)" >&2
      failures=$((failures + 1))
    fi
  done
fi

if [[ -f "${DRIVER_TEST_220}" ]]; then
  # 14b. The destination driver MUST own the P220 tri-state
  # classifier with explicit Unknown/observability-gap behavior
  # (D220-1..D220-9 structural guards).
  for required in \
    'enum P220Observed' \
    'struct P220Facts' \
    'enum P220Terminal' \
    'fn p220_collect_authoritative' \
    'fn record_p220_classification' \
    'fn p220_query_diagnostic' \
    'P220_EPOCH_AUTHORITATIVE' \
    'P220_EPOCH_DISPATCH' \
    'P220-OBSERVABILITY-GAP-' \
    'P220-REVERSE-DELIVERY-PASSED' \
    'p220-classification'; do
    if ! grep -q "${required}" "${DRIVER_TEST_220}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_220} lacks the Plan 220 P220 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 14c. The driver MUST take its authoritative snapshot after
  # its own bootstrap and before the reverse send (D220-1): the
  # diagnostic-port env readers and the post-bootstrap collection
  # call site are both required.
  for diag_port in JAVA_DIAGNOSTIC_A_PORT JAVA_DIAGNOSTIC_B_PORT; do
    if ! grep -q "${diag_port}" "${DRIVER_TEST_220}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_220} never reads ${diag_port} (Plan 220 D220-1 authoritative epoch)" >&2
      failures=$((failures + 1))
    fi
  done
  if ! grep -q 'p220_collect_authoritative(' "${DRIVER_TEST_220}"; then
    echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_220} never invokes p220_collect_authoritative (Plan 220 D220-1)" >&2
    failures=$((failures + 1))
  fi
  # 14c2. The driver MUST document which hash is Router B: the
  # service/publication naming has confused the two before (the
  # b5b049c self-test caught it via the HASH cross-check), so the
  # authoritative collection MUST derive Router B from `java_hash`
  # (the publication router), never `service_hash`.
  if ! grep -q 'p220_bytes_to_hex(java_hash.as_bytes())' "${DRIVER_TEST_220}"; then
    echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_220} does not derive the Router B hash from java_hash (Plan 220 D220-2 identity)" >&2
    failures=$((failures + 1))
  fi
  if grep -q 'p220_bytes_to_hex(service_hash.as_bytes())' "${DRIVER_TEST_220}"; then
    echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_220} derives a P220 RouterHash from service_hash (Router A) (Plan 220 D220-2 identity)" >&2
    failures=$((failures + 1))
  fi
  # 14d. The driver MUST cross-check the protocol-derived B hash
  # against the Java self snapshot (D220-2): the cross-check row
  # and the hex-hash helper are both required.
  if ! grep -q 'p220-routerhash-crosscheck' "${DRIVER_TEST_220}"; then
    echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_220} lacks the p220-routerhash-crosscheck evidence row (Plan 220 D220-2)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'p220_bytes_to_hex' "${DRIVER_TEST_220}"; then
    echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_220} lacks the p220_bytes_to_hex helper (Plan 220 D220-2 hex identity)" >&2
    failures=$((failures + 1))
  fi
  # 14e. Directionally correct dispatch evidence (D220-7): the
  # forward receipt and the reverse observations are separate
  # facts, and forward receipt MUST NOT satisfy reverse dispatch.
  for direction_key in \
    'forward_i2pr_to_java_received' \
    'reverse_java_send_admitted' \
    'reverse_i2pr_payload_recovered' \
    'forward_receipt_satisfies_reverse=false'; do
    if ! grep -q "${direction_key}" "${DRIVER_TEST_220}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_220} lacks the Plan 220 directional evidence '${direction_key}' (D220-7)" >&2
      failures=$((failures + 1))
    fi
  done
  # 14f. The superseded classifier MUST be gone from the driver:
  # no J219 aggregator, no J219 terminal derivation, no
  # pre-bootstrap typed-facts file consumption, no
  # `j219-classification` emission.
  for removed in \
    'J219TypedFacts' \
    'derive_j219_terminal_classification' \
    'J219_TYPED_FACTS_PATH' \
    'j219-classification'; do
    if grep -q "${removed}" "${DRIVER_TEST_220}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_220} retains superseded Plan 219 surface '${removed}' (Plan 220 D220)" >&2
      failures=$((failures + 1))
    fi
  done
  # 14g. The terminal `p220-classification` row MUST be emitted
  # only through the P220 tokens, never a literal
  # `record "<P220-X>" passed` line.
  for guarded in P220-CORRECTED-ATTRIBUTION P220-OBSERVABILITY-GAP P220-REVERSE-DELIVERY-PASSED; do
    if rg -n "^[[:space:]]*record[[:space:]]+[\"']${guarded}" "${DRIVER_TEST_220}" >/dev/null; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_220} hard-codes a p220-classification literal for guarded label '${guarded}' (Plan 220 §11)" >&2
      failures=$((failures + 1))
    fi
  done
  # 14h. The P220 unit rows MUST lock the tri-state semantics in
  # the ordinary workspace floor (Plan 220 §19).
  for unit_row in \
    p220_reverse_delivery_passed_when_every_stage_known_pass \
    p220_hash_mismatch_yields_diagnostic_gap_not_a_missing_b \
    p220_stored_presence_and_f_are_independent \
    p220_stored_live_sha_and_published_are_explicit \
    p220_peermanager_and_selector_are_independent \
    p220_unknown_stage_yields_its_gap \
    p220_earliest_known_fail_wins_after_earlier_known_pass \
    p220_missing_fact_cannot_yield_root_cause \
    p220_forward_receipt_cannot_satisfy_reverse_dispatch \
    p220_terminal_tokens_are_canonical \
    p220_kv_parser_handles_quoted_capabilities; do
    if ! grep -q "fn ${unit_row}" "${DRIVER_TEST_220}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_220} lacks the Plan 220 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

if [[ -f "${LAUNCHER_SRC_220}" ]]; then
  # 14i. The controlled-launcher MUST expose the authoritative
  # P220 hex-hash command set (D220-2/D220-4/D220-5). The frozen
  # J219 commands are history and are not re-checked here.
  for bounded_command in P220-SNAPSHOT P220-STORED-RI P220-CAPABILITIES P220-PEERS-FLOODFILL P220-MAIN-ROUTER-COUNT P220-SELECTOR; do
    if ! grep -q "\"${bounded_command}\"" "${LAUNCHER_SRC_220}"; then
      echo "m6 mixed-router evidence check failed: ${LAUNCHER_SRC_220} lacks the Plan 220 read-only command '${bounded_command}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 14j. RouterHash values MUST travel as lowercase hex on the
  # P220 path; I2P `Hash.toBase64()` is echoed for human
  # correlation only.
  if ! grep -q 'self_router_hash_hex' "${LAUNCHER_SRC_220}"; then
    echo "m6 mixed-router evidence check failed: ${LAUNCHER_SRC_220} lacks hex RouterHash identity on the P220 path (Plan 220 D220-2)" >&2
    failures=$((failures + 1))
  fi
fi

if [[ -f "${SELECTOR_PROBE_SRC_220}" ]]; then
  # 14k. The same-package selector probe MUST call the
  # package-visible selector on the live k-buckets read-only
  # (D220-5), and MUST NOT patch any Java I2P class.
  for required in \
    'package net.i2p.router.networkdb.kademlia' \
    'selectFloodfillParticipants' \
    'getKBuckets' \
    'getPeerSelector'; do
    if ! grep -q "${required}" "${SELECTOR_PROBE_SRC_220}"; then
      echo "m6 mixed-router evidence check failed: ${SELECTOR_PROBE_SRC_220} lacks the Plan 220 probe surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
else
  echo "m6 mixed-router evidence check failed: missing Plan 220 same-package selector probe ${SELECTOR_PROBE_SRC_220}" >&2
  failures=$((failures + 1))
fi

if [[ -f "${HARNESS_220}" ]]; then
  # 14l. The harness MUST reserve three diagnostic TCP ports and
  # bind them via the controlled-launcher arg (unchanged
  # reservation; the P220 commands flow over them).
  for diag_port_var in JAVA_DIAGNOSTIC_A_PORT JAVA_DIAGNOSTIC_B_PORT JAVA_DIAGNOSTIC_C_PORT; do
    if ! grep -q "${diag_port_var}" "${HARNESS_220}"; then
      echo "m6 mixed-router evidence check failed: ${HARNESS_220} lacks the ${diag_port_var} reservation" >&2
      failures=$((failures + 1))
    fi
  done
  # 14m. D220-1: no pre-bootstrap shell fact may feed the
  # terminal classifier. The harness MUST NOT derive classifier
  # input before bootstrap and MUST NOT consume the superseded
  # `j219-classification` key.
  if grep -q 'j219-classification' "${HARNESS_220}"; then
    echo "m6 mixed-router evidence check failed: ${HARNESS_220} consumes the superseded j219-classification key (Plan 220 D220-1)" >&2
    failures=$((failures + 1))
  fi
  if grep -q 'J219_TYPED_FACTS_PATH' "${HARNESS_220}"; then
    echo "m6 mixed-router evidence check failed: ${HARNESS_220} exports the superseded J219_TYPED_FACTS_PATH (Plan 220 D220-1)" >&2
    failures=$((failures + 1))
  fi
  # 14n. D220-2: no raw RouterInfo byte slicing and no standard
  # Base64 RouterHash construction on the diagnostic path.
  if grep -q '\[16:386\]' "${HARNESS_220}"; then
    echo "m6 mixed-router evidence check failed: ${HARNESS_220} slices raw RouterInfo bytes for RouterHash (Plan 220 D220-2)" >&2
    failures=$((failures + 1))
  fi
  if grep -q 'base64.b64encode' "${HARNESS_220}"; then
    echo "m6 mixed-router evidence check failed: ${HARNESS_220} builds a RouterHash with standard Base64 (Plan 220 D220-2)" >&2
    failures=$((failures + 1))
  fi
  # 14o. D220-5/D220-6: the harness MUST NOT synthesize selector
  # output from PeerManager membership and MUST NOT hard-code
  # client-NetDB/OCMOSJ terminal facts.
  if grep -q 'j219-a-selector-result-count' "${HARNESS_220}"; then
    echo "m6 mixed-router evidence check failed: ${HARNESS_220} synthesizes selector output (Plan 220 D220-5)" >&2
    failures=$((failures + 1))
  fi
  if grep -q 'emit("j219-client-db-lookup-started", "true")' "${HARNESS_220}"; then
    echo "m6 mixed-router evidence check failed: ${HARNESS_220} hard-codes client-NetDB lookup facts (Plan 220 D220-6)" >&2
    failures=$((failures + 1))
  fi
  for defaulted in \
    'emit("j219-ocmosj-lease-selected", "false")' \
    'emit("j219-ocmosj-dispatch-submitted", "false")'; do
    if grep -q "${defaulted}" "${HARNESS_220}"; then
      echo "m6 mixed-router evidence check failed: ${HARNESS_220} defaults OCMOSJ facts false (Plan 220 D220-6)" >&2
      failures=$((failures + 1))
    fi
  done
  # 14p. The harness MUST read the LAST `p220-classification`
  # occurrence (final-snapshot rule, mirroring Plan 217 §6.B.6)
  # and MUST record every epoch-labeled timed-snapshot moment.
  if ! grep -q 'p220-classification' "${HARNESS_220}"; then
    echo "m6 mixed-router evidence check failed: ${HARNESS_220} never references the Plan 220 p220-classification evidence key" >&2
    failures=$((failures + 1))
  fi
  for moment in first-routerinfo-appearance immediately-before-bootstrap immediately-after-bootstrap immediately-before-reverse-helper-send after-reverse-send-wait-expires; do
    if ! grep -q "${moment}" "${HARNESS_220}"; then
      echo "m6 mixed-router evidence check failed: ${HARNESS_220} lacks the timed-snapshot moment '${moment}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

# ---- 13. Plan 201 Branch C/D corrective — three-router topology. -----
# Plan 201 §3.3 step 3 added Router C as the independent client-tunnel
# participant when stock Java I2P 2.13.0 cannot build 1-hop client
# tunnels in 2-router + zero-hop configurations. The 1-hop helper
# profile (`inbound.length=1 outbound.length=1`) was tested in this
# run but stock Java's peer-profile scoring under the controlled
# loopback topology does not promote any peer to "fast" within the
# 5-minute I2PSession.connect() timeout (ProfileOrganizer line 161 +
# 913). The retained-partial finding is that the helper tunnel profile
# must stay zero-hop to keep the I2CP CreateLeaseSet reply emitting
# READY; full LS2 publication requires either real-network peers or a
# helper-side tunnel profile override that the controlled loopback
# topology cannot satisfy without breaking Plan 198 §4 / Plan 201 §4
# (no public I2P, no patching Java). The Branch C/D corrective is
# retained as the three-router topology invariant below; the helper
# tunnel-profile invariant allows both zero-hop (current) and 1-hop
# (attempted) profiles so either path can be exercised.
if [[ -f "${JAVA_RAW_HELPER_SRC}" && -f "${JAVA_STREAM_HELPER_SRC}" ]]; then
  # 13a. Helpers must declare some bounded client tunnel profile
  # (zero-hop retained as the working configuration; 1-hop is the
  # Plan 201 §3.3 attempted corrective that proved blocked by Java's
  # loopback peer profile scoring).
  for helper in "${JAVA_RAW_HELPER_SRC}" "${JAVA_STREAM_HELPER_SRC}"; do
    if ! grep -qE 'inbound\.length".*(0|1)' "${helper}"; then
      echo "m6 mixed-router evidence check failed: ${helper} lacks inbound.length=0 or inbound.length=1 (Plan 201 Branch C/D)" >&2
      failures=$((failures + 1))
    fi
    if ! grep -qE 'outbound\.length".*(0|1)' "${helper}"; then
      echo "m6 mixed-router evidence check failed: ${helper} lacks outbound.length=0 or outbound.length=1 (Plan 201 Branch C/D)" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${JAVA_HARNESS}" ]]; then
  # 13b. run-java.sh must provision three Java routers (A, B, C).
  if ! grep -q 'JAVA_TUNNEL_PARTICIPANT_SSU2_PORT' "${JAVA_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: run-java.sh lacks JAVA_TUNNEL_PARTICIPANT_SSU2_PORT (Plan 201 Branch C/D three-router topology)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'datadir-tunnel-participant' "${JAVA_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: run-java.sh lacks datadir-tunnel-participant (Plan 201 Branch C/D three-router topology)" >&2
    failures=$((failures + 1))
  fi
fi
# 13c. The bootstrap driver must handle Router C (tunnel participant).
DRIVER_TEST_13="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
if [[ -f "${DRIVER_TEST_13}" ]]; then
  if ! grep -q 'tunnel_participant' "${DRIVER_TEST_13}"; then
    echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_13} lacks Router C / tunnel-participant bootstrap path (Plan 201 Branch C/D)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'c-knows-a' "${DRIVER_TEST_13}"; then
    echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_13} lacks c-knows-a lookup probe (Plan 201 Branch C/D)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'c-knows-b' "${DRIVER_TEST_13}"; then
    echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_13} lacks c-knows-b lookup probe (Plan 201 Branch C/D)" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 15. Plan 222 corrected client-NetDB/OCMOSJ narrowing ---------------
# Plan 220's selector row (raw target hash + hard-coded N=3 + main facade
# + empty exclude set) is historical only. Plan 222 must reproduce the
# exact client lookup (helper client DBID + Java routing key + effective
# `netdb.searchLimit` + EXTRA_PEERS width through the production-equivalent
# overload) and correlate one reverse send through the public
# nonce-bearing `SendMessageStatusListener` path. The 45-second payload
# window stays frozen; status-only polling is bounded to 70 seconds.
P222_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P222SelectorProbe.java"
P220_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P220SelectorProbe.java"
DRIVER_TEST_222="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
LAUNCHER_SRC_222="${JAVA_LAUNCHER_SRC}"
HELPER_SRC_222="${JAVA_RAW_HELPER_SRC}"
HARNESS_222="${JAVA_HARNESS}"

if [[ ! -f "${P222_PROBE_SRC}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 222 exact preflight probe ${P222_PROBE_SRC}" >&2
  failures=$((failures + 1))
else
  # 15a. Selector-equivalence guards: the P222 probe must use the exact
  # production inputs and must not repeat the P220 overclaim.
  for required in \
    'routingKeyGenerator().getRoutingKey' \
    'netdb.searchLimit' \
    'EXTRA_PEERS' \
    'clientNetDb' \
    'selectFloodfillParticipants(routingKey' \
    'selectorWidth' \
    'targetLsPresent' \
    'isClientDb'; do
    if ! grep -q "${required}" "${P222_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P222_PROBE_SRC} lacks the Plan 222 exact-selector surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # The authoritative P222 probe must not hard-code PROBE_FANOUT=3 and
  # must not pass Collections.emptySet() as the selector input.
  if grep -q 'PROBE_FANOUT' "${P222_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P222_PROBE_SRC} retains PROBE_FANOUT=3 as authoritative selector width (Plan 222 §15 I1)" >&2
    failures=$((failures + 1))
  fi
  if grep -q 'Collections.emptySet()' "${P222_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P222_PROBE_SRC} passes Collections.emptySet() as authoritative selector input (Plan 222 §15 I1)" >&2
    failures=$((failures + 1))
  fi
  # Raw target hash must not feed the selector directly on the P222 path.
  if grep -q 'selectFloodfillParticipants(target' "${P222_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P222_PROBE_SRC} feeds raw target hash to selectFloodfillParticipants (Plan 222 §15 I1)" >&2
    failures=$((failures + 1))
  fi
  # The historical P220 probe stays frozen for traceability.
  if [[ -f "${P220_PROBE_SRC}" ]] && ! grep -q 'PROBE_FANOUT' "${P220_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P220_PROBE_SRC} lost its historical PROBE_FANOUT marker (Plan 222 §7 A)" >&2
    failures=$((failures + 1))
  fi
fi

if [[ -f "${LAUNCHER_SRC_222}" ]]; then
  # 15b. The launcher must expose the exact preflight command.
  if ! grep -q '"P222-CLIENT-LOOKUP-PREFLIGHT"' "${LAUNCHER_SRC_222}"; then
    echo "m6 mixed-router evidence check failed: ${LAUNCHER_SRC_222} lacks the Plan 222 P222-CLIENT-LOOKUP-PREFLIGHT command" >&2
    failures=$((failures + 1))
  fi
fi

if [[ -f "${HELPER_SRC_222}" ]]; then
  # 15c. Tracked-send guards: public listener API, bounded nonce map,
  # legacy SEND remains; no reliability/timeout/tunnel mutation.
  for required in \
    'SendMessageStatusListener' \
    'SEND_TRACKED' \
    'SEND_STATUS' \
    'TRACKED_SENT' \
    'TRACKED_STATUS' \
    'MAX_TRACKED_MESSAGES' \
    'MAX_EVENTS_PER_MESSAGE'; do
    if ! grep -q "${required}" "${HELPER_SRC_222}"; then
      echo "m6 mixed-router evidence check failed: ${HELPER_SRC_222} lacks the Plan 222 tracked-send surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if ! grep -q 'case "SEND":' "${HELPER_SRC_222}"; then
    echo "m6 mixed-router evidence check failed: ${HELPER_SRC_222} lost legacy SEND compatibility (Plan 222 §15 I2)" >&2
    failures=$((failures + 1))
  fi
  for forbidden in 'setReliability' 'setExpiration' 'OVERALL_TIMEOUT'; do
    if grep -q "${forbidden}" "${HELPER_SRC_222}"; then
      echo "m6 mixed-router evidence check failed: ${HELPER_SRC_222} mutates send lifetime via '${forbidden}' (Plan 222 §15 I2)" >&2
      failures=$((failures + 1))
    fi
  done
fi

if [[ -f "${DRIVER_TEST_222}" ]]; then
  # 15d. The driver must own the P222 classifier, tracked-send surface,
  # and frozen-window timing; it must not invent terminals.
  for required in \
    'fn p222_parse_tracked_sent' \
    'fn p222_parse_tracked_status' \
    'fn p222_collect_preflight' \
    'fn p222_selector_equivalence' \
    'fn p222_status_facts' \
    'fn record_p222_classification' \
    'enum P222Terminal' \
    'P222-OBSERVABILITY-GAP-SELECTOR-EQUIVALENCE' \
    'P222-CORRECTED-ATTRIBUTION CLIENT-NETDB-NO-USABLE-LEASESET' \
    'P222-CORRECTED-ATTRIBUTION OCMOSJ-NO-USABLE-TUNNEL-OR-GARLIC-PATH' \
    'P222-CORRECTED-ATTRIBUTION JAVA-DISPATCH-PATH-REACHED-I2PR-TUNNELDATA-NOT-OBSERVED' \
    'P222-OBSERVABILITY-GAP-OCMOSJ-POST-ACCEPT' \
    'P222-EVIDENCE-CONTRADICTION-ACK-SUCCESS-WITHOUT-I2PR-PAYLOAD' \
    'P222-REVERSE-DELIVERY-PASSED' \
    'p222-classification' \
    'send_raw_tracked' \
    'P222-CLIENT-LOOKUP-PREFLIGHT' \
    'P222_STATUS_OBSERVATION_DEADLINE' \
    'frozen_tunneldata_45s' \
    'frozen_payload_45s'; do
    if ! grep -q "${required}" "${DRIVER_TEST_222}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_222} lacks the Plan 222 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # The P222 path must not derive a selector boundary from B presence
  # alone and must not reuse the later deadline for the payload row.
  if grep -q 'P222.*SELECTOR-EXCLUDES-B' "${DRIVER_TEST_222}"; then
    echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_222} derives a P222 selector pass from Router B presence (Plan 222 §15 I1)" >&2
    failures=$((failures + 1))
  fi
  # Timing guards: 45-second window frozen; diagnostic deadline 70 s.
  if ! grep -q 'from_secs(70)' "${DRIVER_TEST_222}"; then
    echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_222} lacks the 70-second P222 status-only deadline (Plan 222 §15 I3)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'MUST NOT retroactively' "${DRIVER_TEST_222}"; then
    echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_222} lacks the frozen-45s invariant comment (Plan 222 §15 I3)" >&2
    failures=$((failures + 1))
  fi
  # P222 unit rows must lock the classifier in the ordinary floor.
  for unit_row in \
    p222_old_p220_selector_alone_yields_equivalence_gap \
    p222_routing_key_vs_raw_key_distinction \
    p222_dynamic_selector_width_computation \
    p222_client_dbid_required \
    p222_selector_empty_maps_to_no_lookup_peer \
    p222_selector_nonempty_b_absent_is_not_root_cause \
    p222_tracked_send_parser_accepts_valid \
    p222_tracked_send_parser_rejects_malformed \
    p222_ordered_status_sequence_retained \
    p222_accepted_alone_does_not_pass_client_netdb \
    p222_no_leaseset_maps_to_client_netdb_failure \
    p222_no_tunnels_maps_to_combined_boundary \
    p222_best_effort_failure_without_tunneldata_maps_to_dispatch_boundary \
    p222_guaranteed_success_without_payload_is_contradiction \
    p222_no_terminal_callback_maps_to_gap \
    p222_later_success_overrides_probable_failure \
    p222_status_deadline_cannot_alter_payload_outcome; do
    if ! grep -q "fn ${unit_row}" "${DRIVER_TEST_222}"; then
      echo "m6 mixed-router evidence check failed: ${DRIVER_TEST_222} lacks the Plan 222 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

if [[ -f "${HARNESS_222}" ]]; then
  # 15e. The harness must compile the P222 probe and record the LAST
  # p222-classification occurrence.
  if ! grep -q 'P222SelectorProbe.java' "${HARNESS_222}"; then
    echo "m6 mixed-router evidence check failed: ${HARNESS_222} never compiles the Plan 222 P222SelectorProbe" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'p222-classification' "${HARNESS_222}"; then
    echo "m6 mixed-router evidence check failed: ${HARNESS_222} never references the Plan 222 p222-classification evidence key" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 16. Plan 223 Destination identity / LS2 crypto-separation --------
# Plan 223 separates the legacy Destination identity field (ElGamal/type 0,
# 256-byte slot, public non-secret filler) from the active Standard-LS2
# X25519/type-4 key. The checker verifies behavior-shaping source, not only
# token presence.
P223_IDENTITY_SRC="${REPO_ROOT}/crates/i2pr-client/src/identity.rs"
P223_LEASESET_SRC="${REPO_ROOT}/crates/i2pr-client/src/leaseset.rs"
P223_BRANCH_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P223BranchProbe.java"
P223_HELPER_SRC="${JAVA_RAW_HELPER_SRC}"
P223_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P223_HARNESS="${JAVA_HARNESS}"
if [[ -f "${P223_IDENTITY_SRC}" ]]; then
  # 16a. Explicit separation constants must exist.
  for required in \
    'DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE' \
    'DESTINATION_LS2_CRYPTO_TYPE' \
    'DESTINATION_LEGACY_PUBLIC_LENGTH' \
    'DESTINATION_LEGACY_PADDING_LENGTH' \
    'from_explicit_parts' \
    'from_private_bytes_legacy_x25519'; do
    if ! grep -q "${required}" "${P223_IDENTITY_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P223_IDENTITY_SRC} lacks Plan 223 separation surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 16b. Generated Destination must not reuse ROUTER_CRYPTO_KEY_TYPE
  # directly (old 3-arg reconstruction must be gone; new path uses
  # explicit filler + legacy type).
  if grep -q 'from_private_bytes(\*signing, \*static_secret, Zeroizing::new(padding))' "${P223_IDENTITY_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P223_IDENTITY_SRC} retains pre-223 3-arg X25519 Destination reconstruction (Plan 223 §17.1)" >&2
    failures=$((failures + 1))
  fi
  # 16c. New canonical path must advertise ElGamal/type 0, not X25519/type 4.
  if ! grep -q 'DESTINATION_IDENTITY_LEGACY_CRYPTO_TYPE' "${P223_IDENTITY_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P223_IDENTITY_SRC} lacks legacy ElGamal cert type (Plan 223 §17.2)" >&2
    failures=$((failures + 1))
  fi
  # 16d. Filler must come from caller CSPRNG, never derived from secrets.
  # Reject any filler derivation from static/secret bytes in the new path.
  if grep -n 'legacy_filler.*static_secret\|filler.*signing\|X25519(static_secret).*filler' "${P223_IDENTITY_SRC}" >/dev/null 2>&1; then
    echo "m6 mixed-router evidence check failed: ${P223_IDENTITY_SRC} derives legacy filler from secret material (Plan 223 §17 invariant 11)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P223_LEASESET_SRC}" ]]; then
  # 16e. LS2 must remain X25519/type 4 sourced from the static secret,
  # never from the Destination public field.
  if ! grep -q 'DESTINATION_LS2_CRYPTO_TYPE' "${P223_LEASESET_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P223_LEASESET_SRC} lacks Plan 223 LS2 type separation (Plan 223 §17.3)" >&2
    failures=$((failures + 1))
  fi
  if grep -q 'destination().public_key().as_bytes().*LeaseSet2EncryptionKey\|LeaseSet2EncryptionKey.*destination.*public_key' "${P223_LEASESET_SRC}" >/dev/null 2>&1; then
    echo "m6 mixed-router evidence check failed: ${P223_LEASESET_SRC} sources LS2 key from Destination field (Plan 223 §17.3)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'identity.static_public_bytes()' "${P223_LEASESET_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P223_LEASESET_SRC} lost static-secret LS2 sourcing (Plan 223 D3)" >&2
    failures=$((failures + 1))
  fi
fi
# 16f. DATAGRAM_WAIT must stay 45 seconds (frozen acceptance window).
if [[ -f "${P223_DRIVER_TEST}" ]]; then
  if ! grep -q 'const DATAGRAM_WAIT: Duration = Duration::from_secs(45)' "${P223_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P223_DRIVER_TEST} changed DATAGRAM_WAIT from 45s (Plan 223 §17.4)" >&2
    failures=$((failures + 1))
  fi
  # 16g. Java pin must remain frozen in the Java lane (i2pd pin lives in
  # the per-layer i2pd harnesses + cross-family aggregator, already
  # enforced by §5/§8; the Java lane itself pins Java only).
  if ! grep -q "9134f808337b401e8e53c73734c81fab04280c9d" "${P223_HARNESS}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: run-java.sh lost Java pin (Plan 223 §17.5)" >&2
    failures=$((failures + 1))
  fi
  # 16h. P223 classifier must discriminate status 17 via Destination-type /
  # source-key / target-key facts, never treat status 17 alone as sufficient.
  for required in \
    'p223_classify_preflight' \
    'P223Preflight' \
    'P223Terminal' \
    'P223-PREFLIGHT-DESTINATION-ENC-GUARD-CONFIRMED' \
    'P223-REVERSE-DELIVERY-PASSED' \
    'P223-STATUS17-PERSISTS-SOURCE-KEYS-MISSING' \
    'P223-STATUS17-PERSISTS-SOURCE-KEYS-NO-X25519' \
    'P223-STATUS17-PERSISTS-TARGET-LS2-NO-X25519' \
    'P223-STATUS17-PERSISTS-NO-KEY-INTERSECTION' \
    'source_keys_present' \
    'target_has_x25519' \
    'selected_key_present' \
    'p223-rust-destination' \
    'p223-java-helper-destination' \
    'p223-branch' \
    'p223-classification'; do
    if ! grep -q "${required}" "${P223_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P223_DRIVER_TEST} lacks Plan 223 discriminator surface '${required}' (Plan 223 §17.6)" >&2
      failures=$((failures + 1))
    fi
  done
  # Source-key absence must be explicit absent (observable=false or
  # present=false), never a silent false default without Unknown handling.
  if ! grep -q 'observable=false reason=branch-unreachable\|source_keys_present=' "${P223_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P223_DRIVER_TEST} lacks explicit source-key absence handling (Plan 223 §17.7)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P223_BRANCH_PROBE_SRC}" ]]; then
  # 16i. Branch probe must use read-only public APIs with SET_ELG fallback,
  # and must never mutate KeyManager/client DB/LeaseSet state or use
  # reflection.
  for required in \
    'keyManager().getKeys' \
    'getSupportedEncryption' \
    'lookupLeaseSetLocally' \
    'getEncryptionKey(' \
    'SET_ELG' \
    'branchForClient' \
    'inspectDestination'; do
    if ! grep -q "${required}" "${P223_BRANCH_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P223_BRANCH_PROBE_SRC} lacks Plan 223 read-only surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for forbidden in \
    'registerKeys' \
    'unregisterKeys' \
    '.store(' \
    'setKeys' \
    'getDeclaredField' \
    'setAccessible'; do
    if grep -q "${forbidden}" "${P223_BRANCH_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P223_BRANCH_PROBE_SRC} mutates Java state via '${forbidden}' (Plan 223 §17.8)" >&2
      failures=$((failures + 1))
    fi
  done
else
  echo "m6 mixed-router evidence check failed: missing Plan 223 branch probe ${P223_BRANCH_PROBE_SRC}" >&2
  failures=$((failures + 1))
fi
if [[ -f "${P223_HELPER_SRC}" ]]; then
  if ! grep -q 'INSPECT_DEST' "${P223_HELPER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P223_HELPER_SRC} lacks Plan 223 INSPECT_DEST (WP B)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'DEST_INFO' "${P223_HELPER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P223_HELPER_SRC} lacks Plan 223 DEST_INFO (WP B)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P223_HARNESS}" ]]; then
  if ! grep -q 'P223-DEST-INSPECT' "${REPO_ROOT}/tests/integration/m6-interop/java/ControlledRouter.java"; then
    echo "m6 mixed-router evidence check failed: ControlledRouter.java lacks Plan 223 P223-DEST-INSPECT (WP B/C)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'P223-BRANCH' "${REPO_ROOT}/tests/integration/m6-interop/java/ControlledRouter.java"; then
    echo "m6 mixed-router evidence check failed: ControlledRouter.java lacks Plan 223 P223-BRANCH (WP C)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'p223-classification' "${P223_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P223_HARNESS} never references Plan 223 p223-classification (WP G)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'P223BranchProbe.java' "${P223_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P223_HARNESS} never compiles Plan 223 P223BranchProbe" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 17. Plan 224 NO_LEASESET lookup-path attribution --------------------
# Plan 224 is attribution-only: read-only Router-B main-LS and
# Router-A helper client-subDB snapshots, scratch-only targeted
# logger.config, whitelist-only sanitized trace facts, one bounded
# Rust classifier, and the frozen 45-second payload window. No
# production corrective, no Java mutation, no topology/tunnel/
# selector/SAM change, no second publication, no standalone lookup
# that could prime the helper client DB.
P224_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P224LsProbe.java"
P224_LAUNCHER_SRC="${JAVA_LAUNCHER_SRC}"
P224_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P224_HARNESS="${JAVA_HARNESS}"
if [[ ! -f "${P224_PROBE_SRC}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 224 snapshot probe ${P224_PROBE_SRC}" >&2
  failures=$((failures + 1))
else
  # 17a. Read-only snapshot surface (exact pinned accessors).
  for required in \
    'lookupLocallyWithoutValidation' \
    'lookupLeaseSetLocally' \
    'clientNetDb' \
    'isClientDb' \
    'getReceivedAsPublished' \
    'getReceivedAsReply' \
    'getReceivedBy' \
    'isUnpublished' \
    'getEncryptionKeys' \
    'isCurrent' \
    'snapshotMain' \
    'snapshotClient'; do
    if ! grep -q "${required}" "${P224_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P224_PROBE_SRC} lacks the Plan 224 read-only surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 17b. The probe must never mutate Java state, use reflection, run
  # a network lookup, or publish/store a LeaseSet. `lookupLeaseSet(`
  # with an open paren is the remote/search call; the read-only
  # `lookupLeaseSetLocally(` never matches it.
  for forbidden in \
    'registerKeys' \
    'unregisterKeys' \
    '.store(' \
    '.publish(' \
    'setKeys' \
    'getDeclaredField' \
    'setAccessible' \
    'lookupLeaseSet(' \
    'import java.lang.reflect'; do
    if grep -q "${forbidden}" "${P224_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P224_PROBE_SRC} mutates Java state or searches via '${forbidden}' (Plan 224 §6/§17 out of scope)" >&2
      failures=$((failures + 1))
    fi
  done
  # 17c. Key type codes and counts only: the probe must never render
  # key bytes into evidence (only `getType().getCode()` integers).
  if ! grep -q 'getType().getCode()' "${P224_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P224_PROBE_SRC} lost key-type-code-only exposure (Plan 224 §7)" >&2
    failures=$((failures + 1))
  fi
fi

if [[ -f "${P224_LAUNCHER_SRC}" ]]; then
  # 17d. The launcher must expose exactly the three P224 commands.
  for required in \
    '"P224-MAIN-LS"' \
    '"P224-CLIENT-LS"' \
    '"P224-HASH-B64"' \
    'P224LsProbe' \
    'P224-EV kind=main-ls' \
    'P224-EV kind=client-ls' \
    'P224-EV kind=hash-b64'; do
    if ! grep -q "${required}" "${P224_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P224_LAUNCHER_SRC} lacks the Plan 224 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 17e. No second diagnostic server, no P224 mutation path.
  for forbidden in \
    'P224.*\.store(' \
    'P224.*registerKeys' \
    'P224.*setAccessible'; do
    if grep -q "${forbidden}" "${P224_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P224_LAUNCHER_SRC} carries a P224 mutation path '${forbidden}' (Plan 224 §6/§17)" >&2
      failures=$((failures + 1))
    fi
  done
fi

if [[ -f "${P224_DRIVER_TEST}" ]]; then
  # 17f. The driver must own the P224 snapshots, whitelist-only
  # sanitizer, and single classifier with the frozen window as an
  # input (never derived from the trace).
  for required in \
    'fn p224_parse_main_ls' \
    'fn p224_parse_client_ls' \
    'fn p224_parse_hash_b64' \
    'fn p224_collect_main_ls' \
    'fn p224_collect_client_ls' \
    'fn p224_collect_hash_b64' \
    'fn p224_scan_log_dir' \
    'fn p224_build_trace' \
    'fn p224_logger_config_installed' \
    'fn p224_b_answerable' \
    'fn p224_classify' \
    'enum P224Terminal' \
    'fn record_p224_classification' \
    'fn record_p224_early_stop_gap' \
    'P224-ATTRIBUTION-B-MAIN-LS-ABSENT' \
    'P224-ATTRIBUTION-B-MAIN-LS-INVALID-OR-STALE' \
    'P224-ATTRIBUTION-B-MAIN-LS-NOT-QUERY-ANSWERABLE' \
    'P224-ATTRIBUTION-A-NO-INBOUND-CLIENT-REPLY-TUNNEL' \
    'P224-ATTRIBUTION-A-LOOKUP-REPLY-CRYPTO-UNAVAILABLE' \
    'P224-ATTRIBUTION-A-SEARCH-EXHAUSTED-WITHOUT-QUERYING-B' \
    'P224-ATTRIBUTION-A-TO-B-LOOKUP-NOT-RECEIVED' \
    'P224-EVIDENCE-CONTRADICTION-B-ANSWERABLE-BUT-NOT-ANSWERED' \
    'P224-ATTRIBUTION-B-REPLY-NOT-OBSERVED-ON-A-CLIENT-TUNNEL' \
    'P224-EVIDENCE-CONTRADICTION-CLIENT-TUNNEL-DSM-NOT-IN-CLIENT-DB' \
    'P224-EVIDENCE-CONTRADICTION-NO-LEASESET-WITH-USABLE-CLIENT-LS' \
    'P224-NEXT-BOUNDARY' \
    'P224-REVERSE-DELIVERY-PASSED' \
    'P224-OBSERVABILITY-GAP-LOOKUP-PATH' \
    'p224-b-main-before-send' \
    'p224-a-client-before-send' \
    'p224-a-client-after' \
    'p224-b-main-after' \
    'p224-target-context' \
    'p224-lookup-trace' \
    'p224-classification'; do
    if ! grep -q "${required}" "${P224_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P224_DRIVER_TEST} lacks the Plan 224 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 17g. Whitelist-only fixed-substring patterns (exact pinned log
  # shapes); the sanitizer must match these and nothing else.
  for required in \
    'ISJ try ' \
    'reply via client tunnel? true' \
    'failed, no IB client tunnel to receive reply' \
    'skipped, no ratchet/elg support' \
    'Handling database lookup message for ' \
    'We have the published LS ' \
    'Storing garlic LS down tunnel for: ' \
    'DLM reply encryption error'; do
    if ! grep -q -F "${required}" "${P224_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P224_DRIVER_TEST} lacks the Plan 224 whitelist pattern '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 17h. The P224 unit rows must lock the classifier in the ordinary floor.
  for unit_row in \
    p224_main_snapshot_distinguishes_raw_absent_from_validated_absent \
    p224_main_snapshot_records_received_as_published \
    p224_client_snapshot_rejects_main_db_fallback \
    p224_ls2_snapshot_exposes_type_counts_only_not_key_bytes \
    p224_b_absent_maps_to_absent_terminal \
    p224_b_invalid_or_stale_mapping \
    p224_b_present_but_not_published_maps_correctly \
    p224_answerable_plus_no_inbound_tunnel_maps_correctly \
    p224_answerable_plus_no_reply_crypto_maps_correctly \
    p224_selector_membership_alone_does_not_satisfy_query_to_b \
    p224_query_to_b_without_b_receipt_maps_correctly \
    p224_b_answer_without_a_inbound_maps_to_reply_not_observed \
    p224_a_inbound_but_client_db_absent_is_contradiction \
    p224_client_db_present_with_status21_is_contradiction \
    p224_status_change_maps_to_next_boundary \
    p224_payload_delivery_maps_to_passed \
    p224_missing_log_file_yields_gap_not_false_facts \
    p224_sanitizer_censors_session_key_material \
    p224_client_tunnel_receipt_requires_exact_helper_hash \
    p224_sanitizer_emits_only_bounded_typed_facts \
    p224_frozen_payload_cannot_be_altered_by_trace \
    p224_b_received_but_not_answered_mapping \
    p224_query_never_started_with_status21_is_gap \
    p224_target_mismatch_yields_gap \
    p224_client_pre_snapshot_required_for_deep_terminals \
    p224_parsers_reject_malformed \
    p224_hash_b64_parser_accepts_valid_and_rejects_mismatch \
    p224_terminal_tokens_are_canonical \
    p224_record_emits_exactly_one_classification; do
    if ! grep -q "fn ${unit_row}" "${P224_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P224_DRIVER_TEST} lacks the Plan 224 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 17i. Freeze guards (Plan 224 §10 WP-A): the publication path
  # keeps exactly its pre-224 submissions (destination + streaming +
  # streaming republication), all still targeted at Router B, and the
  # P224 code never touches publication itself.
  if [[ "$(grep -c 'begin_ls2_publication' "${P224_DRIVER_TEST}")" -ne 3 ]]; then
    echo "m6 mixed-router evidence check failed: ${P224_DRIVER_TEST} changed the LS2 publication submission count (Plan 224 §10 freeze: exactly 3 pre-224 sites)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'RouterHash::from_bytes(\*java_hash.as_bytes())' "${P224_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P224_DRIVER_TEST} lost the Router-B publication target (Plan 224 §10 freeze)" >&2
    failures=$((failures + 1))
  fi
  if grep -q 'p224.*begin_ls2_publication\|begin_ls2_publication.*p224' "${P224_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P224_DRIVER_TEST} lets P224 code touch publication (Plan 224 §6 out of scope)" >&2
    failures=$((failures + 1))
  fi
  # 17j. The frozen 45-second window stays the payload authority:
  # P224 consumes `frozen_payload_45s` as a classifier input and the
  # status-only deadline never feeds the payload row.
  if ! grep -q 'frozen_payload_45s' "${P224_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P224_DRIVER_TEST} lost the frozen-45s P224 input (Plan 224 §15.22)" >&2
    failures=$((failures + 1))
  fi
fi

if [[ -f "${P224_HARNESS}" ]]; then
  # 17k. The harness must compile the P224 probe, install the
  # targeted logger.config before router startup, pass both log dirs
  # to the driver, and record the LAST p224-classification.
  for required in \
    'P224LsProbe.java' \
    'write_p224_logger_config' \
    'logger.defaultLevel=ERROR' \
    'logger.record.net.i2p.router.networkdb.kademlia.IterativeSearchJob=INFO' \
    'logger.record.net.i2p.router.networkdb.HandleDatabaseLookupMessageJob=DEBUG' \
    'logger.record.net.i2p.router.tunnel.InboundMessageDistributor=INFO' \
    'JAVA_A_LOG_DIR' \
    'JAVA_B_LOG_DIR' \
    'p224-classification'; do
    if ! grep -q "${required}" "${P224_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P224_HARNESS} lacks the Plan 224 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 17l. The harness must never invent a P224 terminal: only the
  # `external-p224-classification` aggregator row may be recorded.
  if grep -q 'record "P224-' "${P224_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P224_HARNESS} invents a P224 terminal (Plan 224 §13: exactly one classifier terminal)" >&2
    failures=$((failures + 1))
  fi
  # 17m. Raw Java logs stay scratch-only: no P224 evidence row may be
  # fed by copying a router log file into evidence.
  if grep -q 'p224.*log-router\|log-router.*p224' "${P224_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P224_HARNESS} copies raw router logs into P224 evidence (Plan 224 §8.1: scratch-only)" >&2
    failures=$((failures + 1))
  fi
  # 17n. The sanitizer's hostile-input regression test must stay
  # wired (it intentionally contains the pinned AEAD-reply shape as
  # hostile input; the shell/Java side must never emit it).
  if grep -q 'Sending AEAD reply' "${P224_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P224_HARNESS} emits the pinned reply-key log shape (Plan 224 §8.1)" >&2
    failures=$((failures + 1))
  fi
  for probe_file in "${P224_PROBE_SRC}" "${P224_LAUNCHER_SRC}"; do
    if [[ -f "${probe_file}" ]] && grep -q 'Sending AEAD reply' "${probe_file}"; then
      echo "m6 mixed-router evidence check failed: ${probe_file} emits the pinned reply-key log shape (Plan 224 §8.1)" >&2
      failures=$((failures + 1))
    fi
  done
fi

# ---- 18. Plan 225 effective logger activation corrective ------------------
# Plan 225 remains diagnostic-only. It closes the Plan 224 false-gap by
# proving the three lookup logger scopes in the running Java LogManager,
# rendering the exact client Base32 label through Java, and scanning only the
# bounded nested router log layout. No Java state mutation, topology change,
# extra lookup, or production wire change is permitted.
if [[ -f "${P224_LAUNCHER_SRC}" ]]; then
  for required in \
    '"P225-LOGGER-CONFIG"' \
    '"P225-HASH-B32"' \
    'kind=logger-config' \
    'kind=hash-b32' \
    'p225LoggerConfig' \
    'p225HashB32' \
    'getDefaultLimit' \
    'getMinimumPriority' \
    'toBase32'; do
    if ! grep -q -F "${required}" "${P224_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P224_LAUNCHER_SRC} lacks the Plan 225 read-only activation surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for forbidden in \
    'setConfig(' \
    'setLimits(' \
    'rereadConfig(' \
    'getDeclaredField(' \
    'setAccessible(' \
    'registerKeys(' \
    '.store(' \
    '.publish('; do
    if grep -q -F "${forbidden}" "${P224_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P224_LAUNCHER_SRC} carries a Plan 225 mutation/reflection path '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

if [[ -f "${P224_DRIVER_TEST}" ]]; then
  for required in \
    'struct P225LoggerConfig' \
    'fn p225_parse_logger_config' \
    'fn p225_collect_logger_config' \
    'fn p225_parse_hash_b32' \
    'fn p225_collect_hash_b32' \
    'fn record_p225_logger_config' \
    'fn p224_scan_log_dir_with_b32' \
    'fn p224_build_trace_with_b32' \
    'isj_new' \
    'isj_encrypted_to_b' \
    'Encrypted DLM for ' \
    'kind=logger-config' \
    'kind=hash-b32' \
    'enum P225Terminal' \
    'fn p225_classify' \
    'fn record_p225_classification' \
    'fn record_p225_early_stop_gap' \
    'P225-OBSERVABILITY-GAP-LOOKUP-PATH' \
    'p225-logger-config-a' \
    'p225-logger-config-b' \
    'p225-classification'; do
    if ! grep -q -F "${required}" "${P224_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P224_DRIVER_TEST} lacks the Plan 225 corrective surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for unit_row in \
    p225_logger_parser_requires_effective_pinned_levels \
    p225_hash_b32_parser_requires_exact_echo_and_alphabet \
    p225_terminal_namespace_and_mapping_are_canonical \
    p225_record_emits_one_terminal_for_pre_epoch_gap; do
    if ! grep -q "fn ${unit_row}" "${P224_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P224_DRIVER_TEST} lacks the Plan 225 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

if [[ -f "${P224_HARNESS}" ]]; then
  for required in \
    'p225-classification' \
    'external-p225-classification'; do
    if ! grep -q -F "${required}" "${P224_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P224_HARNESS} lacks the Plan 225 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

# ---- 19. Plan 226 loopback peer-diversity corrective --------------------
# Plan 226 is a harness/observability corrective only. It must prove the
# exact target ISJ job and the historical shared-/24 skip before admitting
# a corrected run with three loopback /24s. It must not change the router
# protocol, enable `netDb.alwaysQuery`, patch the Java cache, or move any
# control endpoint away from 127.0.0.1.
P226_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P226_HARNESS="${JAVA_HARNESS}"
if [[ -f "${P226_DRIVER_TEST}" ]]; then
  for required in \
    'struct P226Trace' \
    'P226_MAX_TARGET_JOB_IDS' \
    'fn p226_new_isj_job_id' \
    'fn p226_target_job_line' \
    'fn p226_target_and_peer' \
    'fn p226_build_trace' \
    'fn p226_classify_baseline' \
    'fn p226_classify_distinct' \
    'fn record_p226_trace' \
    'fn record_p226_classification' \
    'fn record_p226_early_stop_gap' \
    'P226-BASELINE-IP-DIVERSITY-CONFIRMED' \
    'P226-EVIDENCE-CONTRADICTION-IP-DIVERSITY-PERSISTS' \
    'P226-NEXT-BOUNDARY-B-STILL-NOT-QUERIED' \
    'P226-OBSERVABILITY-GAP' \
    'p226-target-job-trace' \
    'p226-routerinfo-hosts' \
    'p226-topology-mode' \
    'p226-classification'; do
    if ! grep -q -F "${required}" "${P226_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P226_DRIVER_TEST} lacks the Plan 226 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for unit_row in \
    p226_loopback_mask3_semantics_are_pairwise_exact \
    p226_exact_target_job_requires_exact_job_id_and_router_hash \
    p226_unrelated_skip_cannot_classify_router_b \
    p226_non_ip_baseline_reason_stops_before_topology_correction \
    p226_distinct_topology_requires_query_dispatch_not_selector_membership \
    p226_distinct_topology_maps_post_dispatch_boundaries \
    p226_classification_is_single_and_frozen_payload_wins; do
    if ! grep -q "fn ${unit_row}" "${P226_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P226_DRIVER_TEST} lacks the Plan 226 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
  for forbidden in 'netDb.alwaysQuery' 'new Router(' 'getDeclaredField(' 'setAccessible(' 'registerKeys(' '.store('; do
    if [[ "${forbidden}" == 'new Router(' ]]; then
      continue
    fi
    if grep -q -F "${forbidden}" "${P226_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P226_DRIVER_TEST} carries forbidden Plan 226 mutation/property '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -E "record[[:space:]]+[\"']P226-" "${P226_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P226_DRIVER_TEST} hard-codes a Plan 226 terminal record" >&2
    failures=$((failures + 1))
  fi
fi

if [[ -f "${P226_HARNESS}" ]]; then
  for required in \
    'JAVA_PEER_TOPOLOGY' \
    'I2PR_M6_JAVA_BASELINE_EVIDENCE_DIR' \
    'JAVA_SSU2_HOST_A' \
    'JAVA_SSU2_HOST_B' \
    'JAVA_SSU2_HOST_C' \
    '127.0.1.1' \
    '127.0.2.1' \
    '127.0.3.1' \
    'ipaddress.ip_address' \
    'socket.AF_INET' \
    'p226-topology-preflight' \
    'p226-classification' \
    'external-p226-classification' \
    'external-p226-topology-preflight' \
    'external-p226-routerinfo-hosts' \
    'external-p226-target-job-trace'; do
    if ! grep -q -F "${required}" "${P226_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P226_HARNESS} lacks the Plan 226 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for control in \
    'JAVA_I2CP_ENDPOINT="127.0.0.1:' \
    'JAVA_RAW_CONTROL_ENDPOINT="127.0.0.1:' \
    'JAVA_STREAM_CONTROL_ENDPOINT="127.0.0.1:' \
    '/dev/tcp/127.0.0.1/'; do
    if ! grep -q -F "${control}" "${P226_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P226_HARNESS} moved or omitted control endpoint '${control}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'netDb.alwaysQuery' "${P226_HARNESS}" "${JAVA_LAUNCHER_SRC}"; then
    echo "m6 mixed-router evidence check failed: Plan 226 enables netDb.alwaysQuery" >&2
    failures=$((failures + 1))
  fi
  if grep -q -E '127\.0\.0\.[234][^0-9]' "${P226_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: corrected topology uses same-/24 127.0.0.x peer addresses" >&2
    failures=$((failures + 1))
  fi
  if grep -q -E "record[[:space:]]+[\"']P226-" "${P226_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P226_HARNESS} invents a Plan 226 terminal" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 20. Plan 227 explicit one-hop client-tunnel corrective ------------
# Reference-harness corrective only. The raw helper requests a genuine
# stock-Java one-hop inbound/outbound client tunnel through Router C via
# ordinary I2CP SessionConfig explicitPeers. Installed pool state is
# authoritative; no profile/tier mutation, no NetDB injection, no direct
# install, no VMComm, no alwaysQuery, no distinct topology, no streaming
# change, no production protocol change.
P227_RAW_HELPER_SRC="${JAVA_RAW_HELPER_SRC}"
P227_STREAM_HELPER_SRC="${JAVA_STREAM_HELPER_SRC}"
P227_LAUNCHER_SRC="${JAVA_LAUNCHER_SRC}"
P227_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P227Probe.java"
P227_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P227_HARNESS="${JAVA_HARNESS}"
if [[ ! -f "${P227_PROBE_SRC}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 227 probe ${P227_PROBE_SRC}" >&2
  failures=$((failures + 1))
else
  for required in \
    'snapshotEligibility' \
    'snapshotClientTunnels' \
    'isSelectable' \
    'isEstablished' \
    'isBanlisted' \
    'lookupLocallyWithoutValidation' \
    'lookupRouterInfoLocally' \
    'getInboundPool' \
    'getOutboundPool' \
    'listTunnels' \
    'getLength()' \
    'getPeer('; do
    if ! grep -q -F "${required}" "${P227_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P227_PROBE_SRC} lacks Plan 227 read-only surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for forbidden in \
    'registerKeys' \
    'unregisterKeys' \
    '.store(' \
    '.publish(' \
    'setKeys' \
    'getDeclaredField' \
    'setAccessible' \
    'lookupLeaseSet(' \
    'import java.lang.reflect'; do
    if grep -q -F "${forbidden}" "${P227_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P227_PROBE_SRC} mutates Java state via '${forbidden}' (Plan 227 invariants 8-10)" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P227_RAW_HELPER_SRC}" ]]; then
  # 1. Length 1 inbound/outbound required.
  if ! grep -qE 'inbound\.length".*"1"' "${P227_RAW_HELPER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P227_RAW_HELPER_SRC} lacks inbound.length=1 (Plan 227 §13.1)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -qE 'outbound\.length".*"1"' "${P227_RAW_HELPER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P227_RAW_HELPER_SRC} lacks outbound.length=1 (Plan 227 §13.1)" >&2
    failures=$((failures + 1))
  fi
  # 2. Zero-hop disabled.
  if ! grep -qE 'allowZeroHop".*"false"' "${P227_RAW_HELPER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P227_RAW_HELPER_SRC} lacks allowZeroHop=false (Plan 227 §13.2)" >&2
    failures=$((failures + 1))
  fi
  # 3. Both explicit-peer options scoped to the raw client SessionConfig.
  for scoped in 'inbound.explicitPeers' 'outbound.explicitPeers'; do
    if ! grep -q -F "${scoped}" "${P227_RAW_HELPER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P227_RAW_HELPER_SRC} lacks scoped ${scoped} (Plan 227 §13.3)" >&2
      failures=$((failures + 1))
    fi
  done
  # 4. Explicit peer derives from Router-C identity (validated Base64 shape).
  if ! grep -q 'explicitPeerB64OrNull\|I2PR_M6_JAVA_EXPLICIT_PEER_B64' "${P227_RAW_HELPER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P227_RAW_HELPER_SRC} lacks Router-C explicit-peer derivation (Plan 227 §13.4)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P227_STREAM_HELPER_SRC}" ]]; then
  # 5. Plan 241 supersedes the Plan-227 freeze for the Streaming helper:
  # it carries the OPTIONAL explicit-peer one-hop contract mirrored
  # from the raw helper (scoped to its SessionConfig, validated shape,
  # one-hop only when the explicit 5th argument or env is present) and
  # retains the zero-hop default branch. No silent one-hop-by-default.
  if ! grep -q 'explicitPeerB64OrNull' "${P227_STREAM_HELPER_SRC}"; then
    echo "m6 mixed-router evidence check failed: streaming helper lacks the Plan 241 optional explicit-peer contract (Plan 241 §5 supersedes Plan 227 invariant 17)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'inbound.explicitPeers' "${P227_STREAM_HELPER_SRC}"; then
    echo "m6 mixed-router evidence check failed: streaming helper lacks scoped explicitPeers (Plan 241 §5 supersedes Plan 227 invariant 17)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -qE 'inbound\.length".*"0"' "${P227_STREAM_HELPER_SRC}"; then
    echo "m6 mixed-router evidence check failed: streaming helper lost frozen zero-hop profile (Plan 227 §13.5)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P227_LAUNCHER_SRC}" ]]; then
  # 6. No router-global explicitPeers (only client SessionConfig may carry it).
  if grep -q 'setProperty.*"explicitPeers"' "${P227_LAUNCHER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P227_LAUNCHER_SRC} sets router-global explicitPeers (Plan 227 §13.6)" >&2
    failures=$((failures + 1))
  fi
  for required in \
    '"P227-PEER-ELIGIBILITY"' \
    '"P227-CLIENT-TUNNELS"' \
    'P227Probe' \
    'p227PeerEligibility' \
    'p227ClientTunnels'; do
    if ! grep -q -F "${required}" "${P227_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P227_LAUNCHER_SRC} lacks Plan 227 surface '${required}' (Plan 227 §13.13-14)" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P227_DRIVER_TEST}" ]]; then
  for required in \
    'fn p227_is_hex64' \
    'fn p227_parse_eligibility' \
    'fn p227_parse_tunnels' \
    'fn p227_collect_eligibility' \
    'fn p227_collect_tunnels' \
    'fn p227_tunnel_gate_pass' \
    'fn p227_eligibility_gate_pass' \
    'enum P227Terminal' \
    'fn p227_classify' \
    'fn record_p227_classification' \
    'fn record_p227_eligibility' \
    'fn record_p227_tunnels' \
    'P227-C-NOT-SELECTABLE' \
    'P227-EXPLICIT-ONE-HOP-NOT-BUILT' \
    'P227-OBSERVABILITY-GAP' \
    'P227-EVIDENCE-CONTRADICTION-ONE-HOP-BUT-ZERO-HOP-UNKNOWN' \
    'P227-EVIDENCE-CONTRADICTION-CLIENT-DSM-NOT-STORED' \
    'P227-NEXT-BOUNDARY-B-NOT-QUERIED' \
    'P227-NEXT-BOUNDARY-A-TO-B-LOOKUP-DELIVERY' \
    'P227-NEXT-BOUNDARY-B-REPLY-TO-A-CLIENT-TUNNEL' \
    'P227-REVERSE-DELIVERY-PASSED' \
    'p227-peer-eligibility' \
    'p227-client-tunnels' \
    'p227-explicit-peer-derivation' \
    'p227-classification'; do
    if ! grep -q -F "${required}" "${P227_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P227_DRIVER_TEST} lacks Plan 227 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for unit_row in \
    p227_c_selectable_passes_eligibility_gate \
    p227_c_non_selectable_maps_to_not_selectable \
    p227_exact_c_identity_mismatch_rejected \
    p227_inbound_only_tunnel_is_insufficient \
    p227_outbound_only_tunnel_is_insufficient \
    p227_zero_hop_fallback_is_insufficient \
    p227_exact_one_hop_both_directions_passes_gate \
    p227_one_hop_plus_zero_hop_contradiction \
    p227_one_hop_no_b_query_maps_to_next_boundary \
    p227_b_query_reply_client_store_downstream \
    p227_changed_send_status_maps_to_next_boundary \
    p227_digest_matched_delivery_passes \
    p227_secret_raw_log_rejected \
    p227_classification_is_single_and_frozen_payload_wins; do
    if ! grep -q "fn ${unit_row}" "${P227_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P227_DRIVER_TEST} lacks Plan 227 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -E "record[[:space:]]+[\"']P227-" "${P227_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P227_DRIVER_TEST} hard-codes a Plan 227 terminal record" >&2
    failures=$((failures + 1))
  fi
  # 16. 45-second window frozen.
  if ! grep -q 'const DATAGRAM_WAIT: Duration = Duration::from_secs(45)' "${P227_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P227_DRIVER_TEST} changed DATAGRAM_WAIT from 45s (Plan 227 §13.16)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P227_HARNESS}" ]]; then
  for required in \
    'P227Probe.java' \
    'P227-PEER-ELIGIBILITY' \
    'P227-CLIENT-TUNNELS' \
    'P224-HASH-B64' \
    'p227-peer-eligibility' \
    'p227-client-tunnels' \
    'p227-explicit-peer-derivation' \
    'p227-classification' \
    'external-p227-classification' \
    'external-p227-peer-eligibility' \
    'external-p227-explicit-peer-derivation' \
    'external-p227-client-tunnels' \
    'external-p227-helper-connect' \
    'helper_connect_elapsed_ms' \
    'TunnelPeerSelector' \
    'ClientPeerSelector' \
    'explicit-peer-option-present' \
    'I2PR_M6_JAVA_EXPLICIT_PEER_B64'; do
    if ! grep -q -F "${required}" "${P227_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P227_HARNESS} lacks Plan 227 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 12. No distinct topology in a P227 counted run (baseline only).
  if ! grep -q 'Plan 227 forbids distinct' "${P227_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P227_HARNESS} lacks Plan 227 distinct-topology rejection (Plan 227 §13.12)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'netDb.alwaysQuery' "${P227_HARNESS}" "${P227_LAUNCHER_SRC}" "${P227_PROBE_SRC}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: Plan 227 enables netDb.alwaysQuery (§13.7)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -E "record[[:space:]]+[\"']P227-" "${P227_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P227_HARNESS} invents a Plan 227 terminal" >&2
    failures=$((failures + 1))
  fi
  # 15. Installed one-hop proof required before reverse-send interpretation:
  # the harness must gate the counted driver on the tunnel snapshot.
  if ! grep -q 'P227_TUNNEL_GATE_OK' "${P227_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P227_HARNESS} lacks the installed one-hop gate before reverse send (Plan 227 §13.15)" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 21. Plan 228 client-tunnel build-path attribution ---------------------
# Attribution-only. No production change, no Java patch, no profile/tier
# mutation, no NetDB store, no tunnel install, no exploratory/client policy
# change, no paired-tunnel override, no VMComm, no alwaysQuery, no public
# topology, no timeout change, no Plan-227 helper profile change, no
# distinct topology. Exactly one terminal; raw logs stay scratch-only.
P228_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P228Probe.java"
P228_LAUNCHER_SRC="${JAVA_LAUNCHER_SRC}"
P228_RAW_HELPER_SRC="${JAVA_RAW_HELPER_SRC}"
P228_STREAM_HELPER_SRC="${JAVA_STREAM_HELPER_SRC}"
P228_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P228_HARNESS="${JAVA_HARNESS}"
if [[ ! -f "${P228_PROBE_SRC}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 228 probe ${P228_PROBE_SRC}" >&2
  failures=$((failures + 1))
else
  for required in \
    'snapshotTunnelInfra' \
    'snapshotClientPools' \
    'getFreeTunnelCount' \
    'getOutboundTunnelCount' \
    'getInboundClientTunnelCount' \
    'getOutboundClientTunnelCount' \
    'getInboundExploratoryPool' \
    'getOutboundExploratoryPool' \
    'listTunnels' \
    'getLength()'; do
    if ! grep -q -F "${required}" "${P228_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P228_PROBE_SRC} lacks Plan 228 read-only surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for forbidden in \
    'registerKeys' \
    'unregisterKeys' \
    '.store(' \
    '.publish(' \
    'setKeys' \
    'buildTunnels' \
    'removeTunnels' \
    'getDeclaredField' \
    'setAccessible' \
    'import java.lang.reflect'; do
    if grep -q -F "${forbidden}" "${P228_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P228_PROBE_SRC} mutates Java state via '${forbidden}' (Plan 228 invariants)" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P228_LAUNCHER_SRC}" ]]; then
  for required in \
    '"P228-TUNNEL-INFRA"' \
    '"P228-CLIENT-POOLS"' \
    '"P228-LOGGER-CONFIG"' \
    'P228Probe' \
    'p228TunnelInfra' \
    'p228ClientPools' \
    'p228LoggerConfig'; do
    if ! grep -q -F "${required}" "${P228_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P228_LAUNCHER_SRC} lacks Plan 228 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for forbidden in 'usePairedTunnels' 'netDb.alwaysQuery'; do
    if grep -q -F "${forbidden}" "${P228_LAUNCHER_SRC}" "${P228_PROBE_SRC}" 2>/dev/null; then
      echo "m6 mixed-router evidence check failed: Plan 228 enables forbidden '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
  # VMComm uses the assignment-form guard (bare key appears in the
  # forbidden-list array and comments by design).
  if grep -n -E "^[[:space:]]*([^/#].*setProperty\([^)]*\"i2p\.vmCommSystem\"[^)]*\"(true|1)\"|[^/#].*i2p\.vmCommSystem[[:space:]]*[:=][[:space:]]*(true|1)\b)" "${P228_LAUNCHER_SRC}" "${P228_PROBE_SRC}" "${P228_HARNESS}" 2>/dev/null >/dev/null; then
    echo "m6 mixed-router evidence check failed: Plan 228 enables forbidden 'i2p.vmCommSystem'" >&2
    failures=$((failures + 1))
  fi
  if grep -q 'setProperty.*"explicitPeers"' "${P228_LAUNCHER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P228_LAUNCHER_SRC} sets router-global explicitPeers (Plan 228 forbids policy change)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P228_STREAM_HELPER_SRC}" ]]; then
  # Plan 241 supersedes the Plan-228 freeze for the Streaming helper:
  # optional explicit-peer one-hop contract with retained zero-hop
  # default (see Plan 227 invariant 17 successor above).
  if ! grep -q 'explicitPeerB64OrNull' "${P228_STREAM_HELPER_SRC}"; then
    echo "m6 mixed-router evidence check failed: streaming helper lacks the Plan 241 optional explicit-peer contract (Plan 241 §5 supersedes Plan 228 frozen invariant)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P228_DRIVER_TEST}" ]]; then
  for required in \
    'fn p228_parse_infra' \
    'fn p228_collect_infra' \
    'fn p228_scan_log_dir' \
    'fn p228_logger_config_installed' \
    'fn p228_config_direction' \
    'enum P228Terminal' \
    'fn p228_classify' \
    'fn record_p228_infra' \
    'fn record_p228_trace' \
    'fn record_p228_classification' \
    'p228_build_path_attribution' \
    'P228-ATTRIBUTION-NO-ROUTER-TUNNEL-INFRA' \
    'P228-ATTRIBUTION-NO-CLIENT-CONFIG' \
    'P228-ATTRIBUTION-NO-PAIRED-TUNNEL' \
    'P228-ATTRIBUTION-BUILD-MESSAGE-CREATE-FAILURE' \
    'P228-ATTRIBUTION-BUILD-CREATED-NOT-DISPATCHED' \
    'P228-ATTRIBUTION-FIRST-HOP-DELIVERY-FAILURE' \
    'P228-ATTRIBUTION-A-DISPATCHED-C-NOT-RECEIVED' \
    'P228-ATTRIBUTION-C-DECRYPT-FAILURE' \
    'P228-ATTRIBUTION-C-REJECTED' \
    'P228-ATTRIBUTION-REPLY-NOT-RETURNED' \
    'P228-ATTRIBUTION-BUILD-REPLY-TIMEOUT' \
    'P228-ATTRIBUTION-REPLY-DECRYPT-FAILURE' \
    'P228-ATTRIBUTION-REMOTE-REJECT' \
    'P228-ATTRIBUTION-LOCAL-JOIN-FAILURE' \
    'P228-NEXT-BOUNDARY-CLIENT-TUNNELS-BUILT' \
    'P228-OBSERVABILITY-GAP-BUILD-PATH' \
    'p228-tunnel-infra' \
    'p228-trace' \
    'p228-classification'; do
    if ! grep -q -F "${required}" "${P228_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P228_DRIVER_TEST} lacks Plan 228 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for unit_row in \
    p228_no_router_tunnel_infra_maps_to_infra_terminal \
    p228_selector_without_config_maps_to_no_client_config \
    p228_config_through_c_without_paired_tunnel_maps_correctly \
    p228_message_created_but_not_dispatched_maps_correctly \
    p228_outbound_first_hop_failure_maps_correctly \
    p228_dispatched_but_c_receives_nothing_maps_correctly \
    p228_c_receive_decrypt_failure_maps_correctly \
    p228_c_explicit_reject_code_maps_correctly \
    p228_c_accept_reply_absent_at_a_maps_correctly \
    p228_a_reply_decrypt_failure_maps_correctly \
    p228_a_remote_rejection_maps_correctly \
    p228_local_join_failure_maps_correctly \
    p228_build_reply_timeout_maps_correctly \
    p228_inbound_only_install_is_insufficient \
    p228_outbound_only_install_is_insufficient \
    p228_both_installed_maps_only_to_next_boundary \
    p228_unrelated_exploratory_logs_cannot_classify_client_build \
    p228_unrelated_destinations_cannot_classify_raw_helper \
    p228_secret_raw_log_lines_rejected_from_evidence \
    p228_classification_emits_exactly_one_terminal \
    p228_earliest_stage_precedence_is_deterministic; do
    if ! grep -q "fn ${unit_row}" "${P228_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P228_DRIVER_TEST} lacks Plan 228 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -E "record[[:space:]]+[\"']P228-" "${P228_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P228_DRIVER_TEST} hard-codes a Plan 228 terminal record" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'const DATAGRAM_WAIT: Duration = Duration::from_secs(45)' "${P228_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P228_DRIVER_TEST} changed DATAGRAM_WAIT from 45s (Plan 228 frozen)" >&2
    failures=$((failures + 1))
  fi
  # Production policy guards: the test-only driver must not override
  # paired-tunnel policy, enable alwaysQuery, or touch timeouts.
  for forbidden in 'usePairedTunnels' 'netDb.alwaysQuery' 'BUILD_MSG_TIMEOUT' 'FIRST_HOP_TIMEOUT'; do
    if grep -q -F "${forbidden}" "${P228_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P228_DRIVER_TEST} carries forbidden Plan 228 policy '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P228_HARNESS}" ]]; then
  for required in \
    'P228Probe.java' \
    'P228-TUNNEL-INFRA' \
    'P228-CLIENT-POOLS' \
    'P228-LOGGER-CONFIG' \
    'p228-tunnel-infra' \
    'p228-logger-config-a' \
    'p228-logger-config-c' \
    'p228-client-pools' \
    'p228-trace' \
    'p228-classification' \
    'external-p228-classification' \
    'external-p228-tunnel-infra' \
    'external-p228-trace' \
    'p228_build_path_attribution' \
    'BuildExecutor=DEBUG' \
    'BuildRequestor=DEBUG' \
    'BuildHandler=DEBUG' \
    'BuildMessageProcessor=DEBUG' \
    'BuildReplyHandler=DEBUG'; do
    if ! grep -q -F "${required}" "${P228_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P228_HARNESS} lacks Plan 228 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'netDb.alwaysQuery' "${P228_HARNESS}" "${P228_LAUNCHER_SRC}" "${P228_PROBE_SRC}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: Plan 228 enables netDb.alwaysQuery" >&2
    failures=$((failures + 1))
  fi
  if grep -q -E "record[[:space:]]+[\"']P228-" "${P228_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P228_HARNESS} invents a Plan 228 terminal" >&2
    failures=$((failures + 1))
  fi
  # A Plan-228 terminal without required upstream stage evidence is
  # forbidden: the harness must record infra/trace/logger/pools rows.
  for upstream in 'p228-tunnel-infra' 'p228-trace' 'p228-logger-config-a' 'p228-client-pools'; do
    if ! grep -q -F "${upstream}" "${P228_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P228_HARNESS} emits a Plan 228 terminal without upstream '${upstream}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

# ---- 22. Plan 229 non-zero exploratory paired-tunnel bootstrap corrective -
# Reference-topology corrective only: explicit A/B/C roles with C as the
# non-floodfill transit participant, Java's stock small-router exploratory
# profile on Router A only, ordinary-profile transit gate for Router C,
# genuine non-zero exploratory gate in both directions, then the unchanged
# Plan-227 client profile through the reused Plan-228 attribution. No
# production change, no Java patch, no profile/NetDB/tunnel mutation, no
# exploratory explicitPeers, no VMComm, no alwaysQuery, no public/reseed
# topology, no timeout change, no helper profile change, no Streaming
# execution, no lookup/reverse-delivery qualification.
P229_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P229Probe.java"
P229_LAUNCHER_SRC="${JAVA_LAUNCHER_SRC}"
P229_RAW_HELPER_SRC="${JAVA_RAW_HELPER_SRC}"
P229_STREAM_HELPER_SRC="${JAVA_STREAM_HELPER_SRC}"
P229_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P229_HARNESS="${JAVA_HARNESS}"
if [[ ! -f "${P229_PROBE_SRC}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 229 probe ${P229_PROBE_SRC}" >&2
  failures=$((failures + 1))
else
  # 22a. Read-only probe surface (exact pinned accessors).
  for required in \
    'snapshotTransitPeer' \
    'snapshotExploratorySettings' \
    'snapshotExploratoryTunnels' \
    'lookupLocallyWithoutValidation' \
    'lookupRouterInfoLocally' \
    'selectAllPeers' \
    'getProfileNonblocking' \
    'isSelectable' \
    'countNotFailingPeers' \
    'isFailing' \
    'isBanlisted' \
    'getCapabilities' \
    'getInboundSettings' \
    'getOutboundSettings' \
    'getLength()' \
    'getLengthVariance()' \
    'getQuantity()' \
    'getInboundExploratoryPool' \
    'getOutboundExploratoryPool' \
    'listTunnels' \
    'getPeer('; do
    if ! grep -q -F "${required}" "${P229_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P229_PROBE_SRC} lacks Plan 229 read-only surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 22b. The probe must never create/mutate profiles, NetDB entries,
  # tunnels, or settings, and must never use reflection. `getPeer(`
  # above is the read-only hop check; the dotted-call mutators below
  # are forbidden (bare words appear in the file's own MUST-NOT
  # documentation by design, so only call sites are rejected).
  for forbidden in \
    '.addProfile(' \
    '.getOrCreateProfile' \
    'registerKeys' \
    'unregisterKeys' \
    '.store(' \
    '.publish(' \
    'setKeys' \
    '.setLength(' \
    '.setQuantity(' \
    '.setInboundSettings(' \
    '.setOutboundSettings(' \
    'buildTunnels' \
    'addTunnel' \
    'getDeclaredField' \
    'setAccessible' \
    'import java.lang.reflect'; do
    if grep -q -F "${forbidden}" "${P229_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P229_PROBE_SRC} mutates Java state via '${forbidden}' (Plan 229 invariants 6-9)" >&2
      failures=$((failures + 1))
    fi
  done
  # 22c. Exploratory explicitPeers is not available (pinned
  # shouldSelectExplicit rejects exploratory settings).
  if grep -q -F 'explicitPeers' "${P229_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P229_PROBE_SRC} carries exploratory explicitPeers (Plan 229 §3.3 forbidden)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'vmCommSystem' "${P229_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P229_PROBE_SRC} references VMComm (Plan 229 invariant 10)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'netDb.alwaysQuery' "${P229_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P229_PROBE_SRC} enables netDb.alwaysQuery (Plan 229 invariant 11)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P229_LAUNCHER_SRC}" ]]; then
  # 22d. Role-aware startup configuration (normal public properties only).
  for required in \
    '"service"' \
    '"publication"' \
    '"transit"' \
    'router.floodfillParticipant' \
    'router.inboundPool.length' \
    'router.inboundPool.lengthVariance' \
    'router.outboundPool.length' \
    'router.outboundPool.lengthVariance' \
    '"P229-TRANSIT-PEER"' \
    '"P229-EXPLORATORY-SETTINGS"' \
    '"P229-EXPLORATORY-TUNNELS"' \
    'P229Probe' \
    'p229TransitPeer' \
    'p229ExploratorySettings' \
    'p229ExploratoryTunnels'; do
    if ! grep -q -F "${required}" "${P229_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P229_LAUNCHER_SRC} lacks Plan 229 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # Unknown roles must fail closed; only service receives the four
  # small-router properties; no quantity/backup/allowZeroHop/
  # explicitPeers/timeout/paired-tunnel property may be set (bare words
  # appear in the file's own documentation by design, so only
  # setProperty call sites are rejected).
  if ! grep -q 'unknown role' "${P229_LAUNCHER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P229_LAUNCHER_SRC} lacks the Plan 229 unknown-role fail-closed branch" >&2
    failures=$((failures + 1))
  fi
  for forbidden in \
    'setProperty[^;]*inboundPool\.quantity' \
    'setProperty[^;]*outboundPool\.quantity' \
    'setProperty[^;]*backupQuantity' \
    'setProperty[^;]*allowZeroHop' \
    'setProperty[^;]*usePairedTunnels' \
    'netDb\.alwaysQuery'; do
    if grep -q -E "${forbidden}" "${P229_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P229_LAUNCHER_SRC} carries forbidden Plan 229 property '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q 'setProperty.*"explicitPeers"' "${P229_LAUNCHER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P229_LAUNCHER_SRC} sets router-global explicitPeers (Plan 229 forbids exploratory explicitPeers)" >&2
    failures=$((failures + 1))
  fi
  if grep -n -E "^[[:space:]]*([^/#].*setProperty\([^)]*\"i2p\.vmCommSystem\"[^)]*\"(true|1)\"|[^/#].*i2p\.vmCommSystem[[:space:]]*[:=][[:space:]]*(true|1)\b)" "${P229_LAUNCHER_SRC}" 2>/dev/null >/dev/null; then
    echo "m6 mixed-router evidence check failed: ${P229_LAUNCHER_SRC} enables forbidden 'i2p.vmCommSystem' (Plan 229 invariant 10)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P229_STREAM_HELPER_SRC}" ]]; then
  # 22e. Plan 241 supersedes the Plan-229 freeze for the Streaming
  # helper: optional explicit-peer one-hop contract with retained
  # zero-hop default (see Plan 227 invariant 17 successor above). The
  # Plan 229 destination-only counted lane still never executes it.
  if ! grep -q 'explicitPeerB64OrNull' "${P229_STREAM_HELPER_SRC}"; then
    echo "m6 mixed-router evidence check failed: streaming helper lacks the Plan 241 optional explicit-peer contract (Plan 241 §5 supersedes Plan 229 invariant 17)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P229_DRIVER_TEST}" ]]; then
  # 22f. The driver owns the P229 gates, the §11 next-boundary mapping,
  # and the single terminal; it never enters the lookup lane.
  for required in \
    'fn p229_parse_role' \
    'fn p229_role_floodfill' \
    'fn p229_role_applies_small_exploratory' \
    'fn p229_parse_transit_peer' \
    'fn p229_collect_transit_peer' \
    'fn p229_transit_gate' \
    'fn p229_parse_exploratory_settings' \
    'fn p229_collect_exploratory_settings' \
    'fn p229_settings_match' \
    'fn p229_parse_exploratory' \
    'fn p229_collect_exploratory' \
    'fn p229_nonzero_gate' \
    'fn p229_nonzero_gate_pass' \
    'enum P229Terminal' \
    'fn p229_classify' \
    'fn record_p229_classification' \
    'fn p229_terminal_permits_lookup' \
    'p229_nonzero_exploratory_bootstrap' \
    'P229-C-ROLE-MISMATCH' \
    'P229-EXPLORATORY-SETTINGS-MISMATCH' \
    'P229-C-NOT-EXPLORATORY-ELIGIBLE' \
    'P229-EXPLORATORY-NONZERO-NOT-BUILT' \
    'P229-EVIDENCE-CONTRADICTION-NONZERO-EXPLORATORY-BUT-NO-PAIRED' \
    'P229-NEXT-BOUNDARY-BUILD-MESSAGE-CREATE' \
    'P229-NEXT-BOUNDARY-A-DISPATCH' \
    'P229-NEXT-BOUNDARY-FIRST-HOP-DELIVERY' \
    'P229-NEXT-BOUNDARY-C-DECRYPT' \
    'P229-NEXT-BOUNDARY-C-REJECT' \
    'P229-NEXT-BOUNDARY-REPLY-RETURN' \
    'P229-NEXT-BOUNDARY-REPLY-DECRYPT' \
    'P229-NEXT-BOUNDARY-REMOTE-REJECT' \
    'P229-NEXT-BOUNDARY-LOCAL-JOIN' \
    'P229-NEXT-BOUNDARY-BUILD-REPLY-TIMEOUT' \
    'P229-CLIENT-TUNNELS-BUILT' \
    'P229-OBSERVABILITY-GAP' \
    'p229-transit-peer' \
    'p229-exploratory-settings' \
    'p229-exploratory-tunnels' \
    'p229-classification'; do
    if ! grep -q -F "${required}" "${P229_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P229_DRIVER_TEST} lacks Plan 229 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 22g. The P229 path never consumes the frozen payload window and
  # never invents a terminal literal.
  if grep -q 'p229.*frozen_payload_45s\|frozen_payload_45s.*p229' "${P229_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P229_DRIVER_TEST} lets the P229 path touch the frozen 45s window (Plan 229 invariant 20)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -E "record[[:space:]]+[\"']P229-" "${P229_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P229_DRIVER_TEST} hard-codes a Plan 229 terminal record" >&2
    failures=$((failures + 1))
  fi
  for forbidden in 'usePairedTunnels' 'netDb.alwaysQuery' 'BUILD_MSG_TIMEOUT' 'FIRST_HOP_TIMEOUT'; do
    if grep -q -F "${forbidden}" "${P229_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P229_DRIVER_TEST} carries forbidden Plan 229 policy '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 22h. The 25 focused unit rows lock the gates and the mapping.
  for unit_row in \
    p229_unknown_launcher_role_fails_closed \
    p229_service_role_keeps_floodfill_true \
    p229_publication_role_keeps_floodfill_true \
    p229_transit_role_sets_floodfill_false \
    p229_only_service_role_receives_small_exploratory_profile \
    p229_no_explicit_peers_on_exploratory_settings \
    p229_no_exploratory_quantity_backup_timeout_override \
    p229_c_advertising_f_maps_to_role_mismatch \
    p229_c_no_profile_maps_to_not_eligible \
    p229_c_profile_but_not_selectable_maps_to_not_eligible \
    p229_zero_hop_only_exploratory_is_insufficient \
    p229_inbound_only_nonzero_is_insufficient \
    p229_outbound_only_nonzero_is_insufficient \
    p229_both_nonzero_admits_helper_start \
    p229_nonzero_plus_no_paired_maps_to_contradiction \
    p229_paired_without_create_maps_to_create_boundary \
    p229_create_without_dispatch_maps_to_dispatch_boundary \
    p229_dispatch_without_c_receive_maps_to_first_hop_boundary \
    p229_c_reject_retained_with_bounded_code \
    p229_reply_decrypt_join_failures_retain_earliest_ordering \
    p229_both_client_tunnels_install_maps_only_to_built \
    p229_no_lookup_or_reverse_send_after_terminal \
    p229_exactly_one_terminal_per_attempt \
    p229_unrelated_logs_cannot_satisfy_facts \
    p229_secret_raw_log_lines_rejected; do
    if ! grep -q "fn ${unit_row}" "${P229_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P229_DRIVER_TEST} lacks Plan 229 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P229_HARNESS}" ]]; then
  # 22i. The harness wires roles, gates, the bounded exploratory poll,
  # the P229 driver, and the single-terminal aggregation.
  for required in \
    'P229Probe.java' \
    '"service"' \
    '"publication"' \
    '"transit"' \
    'router.floodfillParticipant=false' \
    'router.inboundPool.length=1' \
    'router.inboundPool.lengthVariance=1' \
    'router.outboundPool.length=1' \
    'router.outboundPool.lengthVariance=1' \
    'P229-TRANSIT-PEER' \
    'P229-EXPLORATORY-SETTINGS' \
    'P229-EXPLORATORY-TUNNELS' \
    'p229-roles' \
    'p229-exploratory-settings' \
    'p229-transit-peer' \
    'p229-exploratory-tunnels' \
    'p229-role-proof' \
    'P229-C-ROLE-MISMATCH' \
    'P229-EXPLORATORY-SETTINGS-MISMATCH' \
    'P229-C-NOT-EXPLORATORY-ELIGIBLE' \
    'P229-EXPLORATORY-NONZERO-NOT-BUILT' \
    'p229_nonzero_exploratory_bootstrap' \
    'p229-classification' \
    'external-p229-classification' \
    'external-p229-roles' \
    'external-p229-exploratory-settings' \
    'external-p229-transit-peer' \
    'external-p229-exploratory-tunnels' \
    'external-p229-lookup-lane' \
    'p229-lookup-lane' \
    'P229_LOOKUP_DRIVER_ENABLED'; do
    if ! grep -q -F "${required}" "${P229_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P229_HARNESS} lacks Plan 229 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # The counted lookup driver stays disabled on the Plan 229 path
  # (default-0 gate); the successor requalification pass owns it.
  if ! grep -q 'P229_LOOKUP_DRIVER_ENABLED:-0' "${P229_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P229_HARNESS} lacks the Plan 229 default-0 lookup-driver gate" >&2
    failures=$((failures + 1))
  fi
  # Bounded exploratory readiness poll only (within the helper ceiling);
  # no helper five-minute ceiling increase, no Java timeout change.
  # The WP C transit gate likewise polls briefly for in-flight
  # DatabaseStore processing (never a direct NetDB store).
  if ! grep -q 'seq 1 60' "${P229_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P229_HARNESS} lacks the bounded Plan 229 exploratory poll" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q 'seq 1 12' "${P229_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P229_HARNESS} lacks the bounded Plan 229 transit-gate wait" >&2
    failures=$((failures + 1))
  fi
  if grep -q -E "record[[:space:]]+[\"']P229-" "${P229_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P229_HARNESS} invents a Plan 229 terminal" >&2
    failures=$((failures + 1))
  fi
  # Raw Java logs stay scratch-only on the P229 path.
  if grep -q 'p229.*log-router\|log-router.*p229' "${P229_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P229_HARNESS} copies raw router logs into P229 evidence (Plan 229 invariant 21)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'netDb.alwaysQuery' "${P229_HARNESS}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: ${P229_HARNESS} enables netDb.alwaysQuery (Plan 229 invariant 11)" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 23. Plan 230 reachability-capability/profile-bootstrap corrective --
# Observation-contract corrective only: capability/predicate evidence for
# Router C as observed by Router A, the exact pinned
# ProfileManagerImpl.shouldCreate(caps) predicate evaluated read-only,
# one baseline run, conditional stock controlled-topology correction
# (i2np.udp.status=ok fixture-only; C-only real bandwidth 128/128; never
# router.forceBandwidthClass), natural profile bootstrap through the
# ordinary authenticated RI path, then direct continuation through the
# retained P229/P228/P201 gates. No production change, no Java patch, no
# profile/NetDB/tunnel mutation, no isFailing reachability authority, no
# VMComm, no alwaysQuery, no public/reseed topology, no timeout change,
# no frozen-window change.
P230_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P230Probe.java"
P230_LAUNCHER_SRC="${JAVA_LAUNCHER_SRC}"
P230_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P230_HARNESS="${JAVA_HARNESS}"
if [[ ! -f "${P230_PROBE_SRC}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 230 probe ${P230_PROBE_SRC}" >&2
  failures=$((failures + 1))
else
  # 23a. The probe exposes the exact capability/predicate surface
  # read-only (never a creation call).
  for required in \
    'snapshotCapability' \
    'snapshotSelf' \
    'heardAboutCreationEligible' \
    'SHARE_BANDWIDTH_FLOOR_BYTES' \
    'lookupLocallyWithoutValidation' \
    'lookupRouterInfoLocally' \
    'selectAllPeers' \
    'getProfileNonblocking' \
    'isSelectable' \
    'countNotFailingPeers' \
    'isBanlisted' \
    'getCapabilities' \
    'getBandwidthTier' \
    'floodfillEnabled' \
    'getMaxShareBandwidth' \
    'getStatus'; do
    if ! grep -q -F "${required}" "${P230_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P230_PROBE_SRC} lacks Plan 230 probe surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 23b. Probe-side profile creation, NetDB mutation, tunnel
  # installation, reflection, and the non-authoritative P229
  # unreachable/isFailing signal are forbidden. `heardAbout(` with an
  # open paren matches real creation calls but not the read-only
  # heardAboutCreationEligible predicate mirror.
  for forbidden in \
    '.addProfile(' \
    '.getOrCreateProfile' \
    'heardAbout(' \
    '.store(' \
    '.publish(' \
    'buildTunnels' \
    'addTunnel' \
    'getDeclaredField' \
    'setAccessible' \
    'import java.lang.reflect' \
    'isFailing'; do
    if grep -q -F "${forbidden}" "${P230_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P230_PROBE_SRC} uses forbidden Plan 230 probe call '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'explicitPeers' "${P230_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P230_PROBE_SRC} mentions explicitPeers (Plan 230 §4)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'vmCommSystem' "${P230_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P230_PROBE_SRC} enables VMComm (Plan 230 §4)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'netDb.alwaysQuery' "${P230_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P230_PROBE_SRC} enables netDb.alwaysQuery (Plan 230 §4)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P230_LAUNCHER_SRC}" ]]; then
  # 23c. The launcher serves the two read-only P230 commands.
  for required in \
    '"P230-CAPABILITY"' \
    '"P230-SELF-VIEW"' \
    'P230Probe' \
    'p230Capability' \
    'p230SelfView' \
    'heard_about_creation_eligible' \
    'caps_has_r='; do
    if ! grep -q -F "${required}" "${P230_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P230_LAUNCHER_SRC} lacks Plan 230 launcher surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # The lying bandwidth-class override is never authorized.
  if grep -q -F 'forceBandwidthClass' "${P230_LAUNCHER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P230_LAUNCHER_SRC} uses router.forceBandwidthClass (Plan 230 §7 forbids it)" >&2
    failures=$((failures + 1))
  fi
  # A reachability override is fixture-only: when present it must be
  # the stock literal `ok` (pinned UDPTransport maps `ok` to
  # Status.OK, which emits the `R` capability).
  if grep -q -F 'i2np.udp.status' "${P230_LAUNCHER_SRC}"; then
    if ! grep -q -F '"i2np.udp.status", "ok"' "${P230_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P230_LAUNCHER_SRC} sets a non-stock i2np.udp.status override" >&2
      failures=$((failures + 1))
    fi
  fi
  # A bandwidth correction applies to transit Router C only: when the
  # stock bandwidth properties are present, a transit-role guard must
  # dominate them within the same startup block.
  if grep -q -F 'i2np.bandwidth.outboundKBytesPerSecond' "${P230_LAUNCHER_SRC}"; then
    p230_bw_line="$(grep -n -F 'i2np.bandwidth.outboundKBytesPerSecond' "${P230_LAUNCHER_SRC}" | head -n 1 | cut -d: -f1)"
    p230_bw_floor=$((p230_bw_line - 15))
    if [[ "${p230_bw_floor}" -lt 1 ]]; then
      p230_bw_floor=1
    fi
    if ! sed -n "${p230_bw_floor},$((p230_bw_line))p" "${P230_LAUNCHER_SRC}" | grep -q -F 'transit'; then
      echo "m6 mixed-router evidence check failed: ${P230_LAUNCHER_SRC} applies the bandwidth correction outside the transit role (Plan 230 §7 C2)" >&2
      failures=$((failures + 1))
    fi
    if ! grep -q -F '"i2np.bandwidth.outboundBurstKBytesPerSecond", "128"' "${P230_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P230_LAUNCHER_SRC} lacks the paired outbound-burst bandwidth property (Plan 230 §7 C2)" >&2
      failures=$((failures + 1))
    fi
  fi
fi
if [[ -f "${P230_DRIVER_TEST}" ]]; then
  # 23d. The Rust driver owns the exact predicate mirror, the bounded
  # reason set, the correction-authorization matrix, and the stage
  # terminals. No isFailing/unreachable authority may satisfy a gate.
  for required in \
    'fn p230_heard_about_eligible' \
    'fn p230_parse_capability' \
    'fn p230_parse_self_view' \
    'fn p230_predicate_agrees' \
    'fn p230_baseline_blockers' \
    'fn p230_classify_baseline' \
    'fn p230_authorized_correction' \
    'fn p230_reachability_override_permitted' \
    'fn p230_authorized_launcher_props' \
    'fn p230_ri_fresh' \
    'enum P230Terminal' \
    'fn p230_classify_profile' \
    'fn p230_classify_tunnel_continuation' \
    'fn p230_terminal_permits_destination' \
    'fn record_p230_classification' \
    'P230-A-PREDICATE-ELIGIBLE' \
    'P230-A-PREDICATE-INELIGIBLE' \
    'P230-A-OBSERVABILITY-GAP' \
    'P230-C-UNEXPECTED-CAPABILITY-EXCLUSION' \
    'P230-D-OBSERVABILITY-GAP' \
    'P230-D-RI-NOT-UPDATED' \
    'P230-D-ELIGIBLE-BUT-NO-PROFILE' \
    'P230-D-PROFILE-BOOTSTRAP-PASSED' \
    'P230-E-EXPLORATORY-NOT-INSTALLED' \
    'P230-E-CLIENT-NOT-BUILT' \
    'P230-E-PAIRED-TUNNEL-CONTRADICTION' \
    'P230-E-TUNNEL-CONTINUATION-PASSED' \
    'p230-capability' \
    'p230-baseline' \
    'p230-profile' \
    'p230-classification' \
    'low-bandwidth-l-on-floodfill-observer' \
    'P230_SHARE_BANDWIDTH_FLOOR_BYTES'; do
    if ! grep -q -F "${required}" "${P230_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P230_DRIVER_TEST} lacks Plan 230 driver surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -E "record[[:space:]]+[\"']P230-" "${P230_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P230_DRIVER_TEST} invents a hard-coded Plan 230 terminal" >&2
    failures=$((failures + 1))
  fi
  # 23e. The 21 required Plan 230 §12 unit rows.
  for unit_row in \
    'p230_isfailing_never_authoritative_for_reachability' \
    'p230_selectable_alone_does_not_imply_creation_eligible' \
    'p230_missing_r_is_ineligible' \
    'p230_u_without_r_is_ineligible' \
    'p230_nonff_l_peer_on_floodfill_observer_is_ineligible' \
    'p230_nonff_r_non_l_non_e_non_g_is_eligible' \
    'p230_floodfill_peer_with_r_is_eligible' \
    'p230_e_is_ineligible_for_nonff_peer' \
    'p230_g_is_ineligible_for_nonff_peer' \
    'p230_baseline_ineligible_authorizes_only_matching_fixture_correction' \
    'p230_reachability_override_is_loopback_fixture_only' \
    'p230_bandwidth_correction_applies_to_transit_c_only' \
    'p230_force_bandwidth_class_is_forbidden' \
    'p230_stale_pre_correction_ri_cannot_pass_post_correction_gate' \
    'p230_eligible_without_profile_maps_to_d_boundary' \
    'p230_profile_gate_requires_natural_organizer_membership' \
    'p230_no_probe_profile_creation_calls' \
    'p230_profile_pass_continues_into_exploratory_gate' \
    'p230_tunnel_pass_continues_into_frozen_destination_lane' \
    'p230_exactly_one_earliest_terminal' \
    'p230_secret_or_unrelated_rows_rejected'; do
    if ! grep -q "fn ${unit_row}" "${P230_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P230_DRIVER_TEST} lacks Plan 230 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P230_HARNESS}" ]]; then
  # 23f. The harness wires the probe compile, the baseline/predicate
  # gates, the bounded D poll, the E continuation, and the single
  # aggregation terminal.
  for required in \
    'P230Probe.java' \
    'P230-CAPABILITY' \
    'P230-SELF-VIEW' \
    'p230-capability' \
    'p230-self-view-c' \
    'p230-baseline' \
    'p230-profile' \
    'p230-classification' \
    'p230_compute_eligible' \
    'p230_compute_blockers' \
    'P230_D_PASS' \
    'P230_F_ENTERED' \
    'P230-A-PREDICATE-ELIGIBLE' \
    'P230-A-PREDICATE-INELIGIBLE' \
    'P230-A-OBSERVABILITY-GAP' \
    'P230-C-UNEXPECTED-CAPABILITY-EXCLUSION' \
    'P230-D-RI-NOT-UPDATED' \
    'P230-D-ELIGIBLE-BUT-NO-PROFILE' \
    'P230-D-PROFILE-BOOTSTRAP-PASSED' \
    'P230-E-EXPLORATORY-NOT-INSTALLED' \
    'P230-E-CLIENT-NOT-BUILT' \
    'P230-E-PAIRED-TUNNEL-CONTRADICTION' \
    'P230-E-TUNNEL-CONTINUATION-PASSED' \
    'external-p230-classification' \
    'external-p230-capability' \
    'external-p230-self-view-c' \
    'external-p230-baseline' \
    'external-p230-profile' \
    'heard_about_creation_eligible' \
    '131072'; do
    if ! grep -q -F "${required}" "${P230_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P230_HARNESS} lacks Plan 230 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # Bounded D readiness poll only (within the helper ceiling); the old
  # multi-minute profile-population exploration stays unauthorized.
  if ! grep -q 'seq 1 6' "${P230_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P230_HARNESS} lacks the bounded Plan 230 profile poll" >&2
    failures=$((failures + 1))
  fi
  if grep -q -E "record[[:space:]]+[\"']P230-" "${P230_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P230_HARNESS} invents a Plan 230 terminal" >&2
    failures=$((failures + 1))
  fi
  # The historical unreachable/isFailing signal must not satisfy any
  # Plan 230 row.
  if grep -q -E 'p230.*unreachable|unreachable.*p230|p230.*isFailing|isFailing.*p230' "${P230_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P230_HARNESS} consumes isFailing/unreachable on the P230 path (Plan 230 §4.11)" >&2
    failures=$((failures + 1))
  fi
  # Raw Java logs stay scratch-only on the P230 path.
  if grep -q 'p230.*log-router\|log-router.*p230' "${P230_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P230_HARNESS} copies raw router logs into P230 evidence (Plan 230 §4)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'netDb.alwaysQuery' "${P230_HARNESS}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: ${P230_HARNESS} enables netDb.alwaysQuery (Plan 230 §4)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'forceBandwidthClass' "${P230_HARNESS}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: ${P230_HARNESS} uses router.forceBandwidthClass (Plan 230 §7 forbids it)" >&2
    failures=$((failures + 1))
  fi
fi

P231_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P231Probe.java"
P231_LAUNCHER_SRC="${JAVA_LAUNCHER_SRC}"
P231_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P231_HARNESS="${JAVA_HARNESS}"
if [[ ! -f "${P231_PROBE_SRC}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 231 probe ${P231_PROBE_SRC}" >&2
  failures=$((failures + 1))
else
  # 24a. The probe exposes the exact post-ACCEPTED attribution surface
  # read-only: StatManager lifetime counts, the installed outbound
  # client-tunnel hop-0 send id, and the single participating HopConfig
  # filtered by the exact receive tunnel id.
  for required in \
    'snapshotGatewayStats' \
    'snapshotClientOutbound' \
    'snapshotParticipating' \
    'statManager().getRate' \
    'getLifetimeEventCount' \
    'tunnelDispatcher().listParticipatingTunnels' \
    'getReceiveTunnelId' \
    'getSendTunnelId' \
    'getReceiveFrom' \
    'getSendTo' \
    'getProcessedMessagesCount' \
    'getOutboundPool' \
    'listTunnels' \
    'client.dispatchTime' \
    'client.dispatchSendTime' \
    'tunnel.dispatchOutboundTunnel' \
    'tunnel.dropGatewayOverflow' \
    'tunnel.dispatchInbound' \
    'tunnel.inboundLookupSuccess'; do
    if ! grep -q -F "${required}" "${P231_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P231_PROBE_SRC} lacks Plan 231 probe surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 24b. Probe-side mutation, NetDB/tunnel/queue/profile writes,
  # reflection, private-field access, and mainline creation calls are
  # forbidden. Tunnel-id setters would let a probe fabricate the very
  # identities it must observe read-only.
  for forbidden in \
    '.addProfile(' \
    '.getOrCreateProfile' \
    'heardAbout(' \
    '.store(' \
    '.publish(' \
    'buildTunnels' \
    'addTunnel' \
    'setSendTunnelId' \
    'setReceiveTunnelId' \
    'getDeclaredField' \
    'setAccessible' \
    'import java.lang.reflect'; do
    if grep -q -F "${forbidden}" "${P231_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P231_PROBE_SRC} uses forbidden Plan 231 probe call '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'explicitPeers' "${P231_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P231_PROBE_SRC} mentions explicitPeers (Plan 231 §4)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'vmCommSystem' "${P231_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P231_PROBE_SRC} enables VMComm (Plan 231 §4)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'netDb.alwaysQuery' "${P231_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P231_PROBE_SRC} enables netDb.alwaysQuery (Plan 231 §4)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P231_LAUNCHER_SRC}" ]]; then
  # 24c. The launcher serves the three read-only P231 commands.
  for required in \
    '"P231-GATEWAY"' \
    '"P231-CLIENT-OUTBOUND"' \
    '"P231-PARTICIPATING"' \
    'P231Probe' \
    'p231Gateway' \
    'p231ClientOutbound' \
    'p231Participating' \
    'kind=gateway' \
    'kind=client-outbound' \
    'kind=participating' \
    'present=false'; do
    if ! grep -q -F "${required}" "${P231_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P231_LAUNCHER_SRC} lacks Plan 231 launcher surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # Plan 231 observability correction: the stock stat.full property
  # must be enabled so the exact-pinned StatManager actually creates
  # the tunnel/client lifetime rates (otherwise ignoreStat drops
  # every createRateStat and the A/D gateway stages stay
  # unobservable). No other stat/observability mutation is authorized.
  if ! grep -q -F '"stat.full", "true"' "${P231_LAUNCHER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P231_LAUNCHER_SRC} lacks the Plan 231 stat.full observability correction" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'forceBandwidthClass' "${P231_LAUNCHER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P231_LAUNCHER_SRC} uses router.forceBandwidthClass (Plan 231 §4 forbids it)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P231_DRIVER_TEST}" ]]; then
  # 24d. The Rust driver owns the reverse-epoch contract, the
  # isolated-epoch deltas, the exact tunnel matches, the i2pr
  # tunnel-id-attributed counters, and the earliest-stage classifier.
  for required in \
    'fn p231_accepted_implies_dispatch_called' \
    'fn p231_parse_gateway' \
    'fn p231_parse_client_outbound' \
    'fn p231_parse_participating' \
    'fn p231_collect_gateway' \
    'fn p231_collect_client_outbound' \
    'fn p231_collect_participating' \
    'fn p231_delta' \
    'fn p231_target_role' \
    'fn p231_count_marker_in_log_dir' \
    'fn p231_count_marker_with_id_in_log_dir' \
    'enum P231Terminal' \
    'fn p231_classify' \
    'fn record_p231_classification' \
    'fn record_p231_early_stop_gap' \
    'P231-A-OUTBOUND-GATEWAY-NOT-FOUND' \
    'P231-A-OUTBOUND-GATEWAY-ENQUEUE-DROP' \
    'P231-A-OBSERVABILITY-GAP' \
    'P231-B-FIRST-HOP-NOT-RECEIVED-BY-C' \
    'P231-B-C-OBEP-REASSEMBLY-FAILED' \
    'P231-B-OBSERVABILITY-GAP' \
    'P231-C-TARGET-IBGW-NOT-INSTALLED' \
    'P231-C-TUNNEL-GATEWAY-NOT-RECEIVED' \
    'P231-C-IBGW-ENQUEUE-OR-PUMP-BOUNDARY' \
    'P231-C-IBGW-NEXT-HOP-LOOKUP-FAILED' \
    'P231-D-I2PR-NO-EXPECTED-TUNNELDATA' \
    'P231-D-I2PR-TUNNEL-RECOVERY-FAILED' \
    'P231-D-I2PR-GARLIC-DECODE-FAILED' \
    'P231-D-I2PR-DESTINATION-DISPATCH-MISSED' \
    'P231-D-I2PR-PAYLOAD-MISMATCH' \
    'P231-D-OBSERVABILITY-GAP' \
    'P231-REVERSE-DELIVERY-PASSED' \
    'p231-reverse-epoch' \
    'p231-a-gateway' \
    'p231-c-obep' \
    'p231-ibgw' \
    'p231-i2pr-reverse' \
    'p231-classification' \
    'ibgw_present_exact' \
    'c_obep_present_exact' \
    'expected_inbound_tunneldata_seen' \
    'p231_expected_tunnel_id'; do
    if ! grep -q -F "${required}" "${P231_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P231_DRIVER_TEST} lacks Plan 231 driver surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -E "record[[:space:]]+[\"']P231-" "${P231_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P231_DRIVER_TEST} invents a hard-coded Plan 231 terminal" >&2
    failures=$((failures + 1))
  fi
  # ACCEPTED must never be treated as delivery proof, and OCMOSJ must
  # never be claimed as not-entered after a nonce-correlated ACCEPTED.
  if grep -qi "never dispatched" "${P231_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P231_DRIVER_TEST} claims OCMOSJ never dispatched after ACCEPTED (Plan 231 §3 source-order lock)" >&2
    failures=$((failures + 1))
  fi
  # The frozen windows stay frozen: 45-second reverse payload
  # acceptance, 70-second status-only observation.
  if ! grep -q -F 'DATAGRAM_WAIT: Duration = Duration::from_secs(45)' "${P231_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P231_DRIVER_TEST} changed the frozen 45-second reverse window (Plan 231 §4)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'P222_STATUS_OBSERVATION_DEADLINE: Duration = Duration::from_secs(70)' "${P231_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P231_DRIVER_TEST} changed the frozen 70-second status-only window (Plan 231 §4)" >&2
    failures=$((failures + 1))
  fi
  # No production Rust behavior change is authorized by the
  # attribution phase: P231 surface stays inside the external test.
  for prod_dir in \
    "${REPO_ROOT}/crates/i2pr-daemon/src" \
    "${REPO_ROOT}/crates/i2pr-client/src" \
    "${REPO_ROOT}/crates/i2pr-tunnel/src" \
    "${REPO_ROOT}/crates/i2pr-runtime/src"; do
    if grep -rq -F 'p231' "${prod_dir}" 2>/dev/null || grep -rq -F 'P231' "${prod_dir}" 2>/dev/null; then
      echo "m6 mixed-router evidence check failed: production dir ${prod_dir} carries Plan 231 surface (Plan 231 §4 forbids production changes)" >&2
      failures=$((failures + 1))
    fi
  done
  # Raw Java logs stay scratch-only: no P231 evidence row carries log
  # text or log paths.
  if grep "append_evidence" "${P231_DRIVER_TEST}" | grep -F "log-router" >/dev/null 2>&1; then
    echo "m6 mixed-router evidence check failed: ${P231_DRIVER_TEST} promotes raw router logs into P231 evidence (Plan 231 §4)" >&2
    failures=$((failures + 1))
  fi
  # 24e. The 18 required Plan 231 §12 unit rows.
  for unit_row in \
    'p231_accepted_is_ordered_after_inline_dispatch_call' \
    'p231_accepted_alone_does_not_prove_gateway_enqueue' \
    'p231_gateway_stat_delta_requires_target_epoch' \
    'p231_gateway_overflow_maps_to_enqueue_drop' \
    'p231_c_obep_requires_exact_installed_tunnel' \
    'p231_c_obep_count_delta_proves_first_hop_processing' \
    'p231_target_gateway_role_comes_from_selected_lease' \
    'p231_ibgw_requires_exact_target_tunnel_id' \
    'p231_ibgw_processed_delta_precedes_tunneldata_emitted' \
    'p231_unrelated_tunneldata_cannot_satisfy_i2pr_stage' \
    'p231_expected_tunnel_id_required_for_recovery_stage' \
    'p231_tunnel_recovery_failure_is_distinct_from_no_wire_receive' \
    'p231_garlic_failure_is_distinct_from_tunnel_recovery_failure' \
    'p231_destination_queue_hit_requires_digest_match_for_pass' \
    'p231_status_only_after_45s_cannot_pass_payload_delivery' \
    'p231_first_unknown_stage_maps_to_observability_gap' \
    'p231_exactly_one_terminal_per_counted_run' \
    'p231_secret_bearing_log_rows_rejected'; do
    if ! grep -q "fn ${unit_row}" "${P231_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P231_DRIVER_TEST} lacks Plan 231 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P231_HARNESS}" ]]; then
  # 24f. The harness wires the probe compile, the C-port/C-log
  # inputs to the destination driver, and the single-terminal guard.
  # (The P231-GATEWAY/P231-CLIENT-OUTBOUND/P231-PARTICIPATING command
  # strings live in the launcher and are pinned by 24c above.)
  for required in \
    'P231Probe.java' \
    'JAVA_DIAGNOSTIC_C_PORT' \
    'JAVA_C_LOG_DIR' \
    'p231-classification'; do
    if ! grep -q -F "${required}" "${P231_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P231_HARNESS} lacks Plan 231 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -E "record[[:space:]]+[\"']P231-" "${P231_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P231_HARNESS} invents a Plan 231 terminal" >&2
    failures=$((failures + 1))
  fi
  # No P231 row may carry topology/profile/tunnel-policy changes,
  # timeout overrides, public-I2P/reseed/VMComm/alwaysQuery escapes,
  # or raw log promotion relative to Plan 230.
  p231_harness_lines="$(grep -n -E 'p231|P231' "${P231_HARNESS}" || true)"
  for forbidden in \
    'explicitPeers' \
    'forceBandwidthClass' \
    'netDb.alwaysQuery' \
    'vmCommSystem' \
    'reseed' \
    'floodfillParticipant' \
    'Pool.length' \
    'DRIVER_TIMEOUT=' \
    'DATAGRAM' \
    'log-router'; do
    if printf '%s\n' "${p231_harness_lines}" | grep -q -F "${forbidden}"; then
      echo "m6 mixed-router evidence check failed: ${P231_HARNESS} P231 row carries forbidden surface '${forbidden}' (Plan 231 §4/§13)" >&2
      failures=$((failures + 1))
    fi
  done
fi

P232_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
if [[ -f "${P232_DRIVER_TEST}" ]]; then
  # 25a. The Rust driver owns the route-derived lease contract, the
  # parity/publication/preflight evidence rows, and the earliest-stage
  # reverse classifier. Every local LS2 lease gateway/tunnel must come
  # from the installed inbound route object, never from a hard-coded
  # Java router role or the publication target.
  for required in \
    'fn p232_route_derived_lease_source' \
    'enum P232LeaseError' \
    'struct P232LeaseParity' \
    'fn p232_lease_parity' \
    'struct P232LeaseRecord' \
    'fn p232_record_lease_route' \
    'fn p232_record_publication_separation' \
    'fn p232_target_ibgw_gate' \
    'fn p232_record_target_ibgw_preflight' \
    'enum P232Terminal' \
    'struct P232ReverseInputs' \
    'fn p232_classify_reverse' \
    'fn record_p232_classification' \
    'fn p232_raw_reverse_permits_streaming' \
    'fn p232_streaming_permits_closure' \
    'inbound_gateway_route' \
    'route.gateway_router' \
    'route.gateway_receive_tunnel' \
    'p232-destination-lease-route' \
    'p232-streaming-lease-route' \
    'p232-publication-separation' \
    'p232-target-ibgw-preflight' \
    'p232-streaming-complete' \
    'p232-classification' \
    'P232-D-REVERSE-DELIVERY-PASSED' \
    'P232-D-JAVA-FORWARDING-BOUNDARY' \
    'P232-D-I2PR-NO-EXPECTED-TUNNELDATA' \
    'P232-D-I2PR-TUNNEL-RECOVERY-FAILED' \
    'P232-D-I2PR-GARLIC-DECODE-FAILED' \
    'P232-D-I2PR-DESTINATION-DISPATCH-MISSED' \
    'P232-D-I2PR-PAYLOAD-MISMATCH' \
    'P232-D-OBSERVABILITY-GAP' \
    'P232-FIXTURE-ROUTE-PARITY-FAILED' \
    'P232-RAW-REVERSE-PASSED-STREAMING-BOUNDARY' \
    'P232-JAVA-SECOND-FAMILY-PASSED'; do
    if ! grep -q -F "${required}" "${P232_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} lacks Plan 232 driver surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 25b. Exactly one direct lease constructor remains: the helper body.
  # Every Java-driver local lease site must call the helper instead, so
  # a future site that bypasses the route-derived contract fails here.
  p232_from_parts_count="$(grep -c -F 'InboundLeaseSource::from_parts' "${P232_DRIVER_TEST}" || true)"
  if [[ "${p232_from_parts_count}" -ne 1 ]]; then
    echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} has ${p232_from_parts_count} InboundLeaseSource::from_parts sites, want exactly 1 (the Plan 232 helper body; Plan 232 §13)" >&2
    failures=$((failures + 1))
  fi
  # 25c. The helper body must derive gateway/tunnel from the route
  # object and must not name either Java router role directly.
  p232_helper_body="$(sed -n '/fn p232_route_derived_lease_source/,/^}/p' "${P232_DRIVER_TEST}")"
  if ! printf '%s\n' "${p232_helper_body}" | grep -q -F 'route.gateway_router'; then
    echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} helper does not derive the lease gateway from the installed route (Plan 232 §6)" >&2
    failures=$((failures + 1))
  fi
  if printf '%s\n' "${p232_helper_body}" | grep -q -F 'java_hash'; then
    echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} helper names java_hash as a lease source (Plan 232 §6 forbids it)" >&2
    failures=$((failures + 1))
  fi
  if printf '%s\n' "${p232_helper_body}" | grep -q -F 'service_hash'; then
    echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} helper names service_hash as a lease source (Plan 232 §6 forbids it)" >&2
    failures=$((failures + 1))
  fi
  # 25d. All three local lease sites must flow through the helper: the
  # destination site, the initial Streaming site, and the refresh site.
  p232_helper_uses="$(grep -c -F 'p232_route_derived_lease_source(' "${P232_DRIVER_TEST}" || true)"
  if [[ "${p232_helper_uses}" -lt 4 ]]; then
    echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} calls the Plan 232 helper ${p232_helper_uses} time(s), want >= 4 (definition + 3 lease sites; Plan 232 §7)" >&2
    failures=$((failures + 1))
  fi
  for site_literal in 'lane: "destination"' 'lane: "streaming"' 'stage: "initial"' 'stage: "refresh"'; do
    if ! grep -q -F "${site_literal}" "${P232_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} lacks Plan 232 lease-route evidence site '${site_literal}' (Plan 232 §7)" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -E "record[[:space:]]+[\"']P232-" "${P232_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} invents a hard-coded Plan 232 terminal" >&2
    failures=$((failures + 1))
  fi
  # 25e. Publication stays on the retained Java floodfill router: the
  # publication target must remain `java_hash` at every LS2 publication.
  if ! grep -q -F 'begin_ls2_publication(store_message, RouterHash::from_bytes(*java_hash.as_bytes()))' "${P232_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} changed the retained Java publication target (Plan 232 §8)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'begin_ls2_publication(fresh_store, RouterHash::from_bytes(*java_hash.as_bytes()))' "${P232_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} changed the retained Java republication target (Plan 232 §8)" >&2
    failures=$((failures + 1))
  fi
  # 25f. Frozen windows stay frozen: 30-second install poll, 45-second
  # reverse payload acceptance, 70-second status-only observation.
  if ! grep -q -F 'ACCEPT_TIMEOUT: Duration = Duration::from_secs(30)' "${P232_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} changed the frozen 30-second install window (Plan 232 §5)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'DATAGRAM_WAIT: Duration = Duration::from_secs(45)' "${P232_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} changed the frozen 45-second reverse window (Plan 232 §5)" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'P222_STATUS_OBSERVATION_DEADLINE: Duration = Duration::from_secs(70)' "${P232_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} changed the frozen 70-second status-only window (Plan 232 §5)" >&2
    failures=$((failures + 1))
  fi
  # 25g. No production Rust behavior change is authorized by the
  # fixture corrective: P232 surface stays inside the external test.
  for prod_dir in \
    "${REPO_ROOT}/crates/i2pr-daemon/src" \
    "${REPO_ROOT}/crates/i2pr-client/src" \
    "${REPO_ROOT}/crates/i2pr-tunnel/src" \
    "${REPO_ROOT}/crates/i2pr-runtime/src"; do
    if grep -rq -F 'p232' "${prod_dir}" 2>/dev/null || grep -rq -F 'P232' "${prod_dir}" 2>/dev/null; then
      echo "m6 mixed-router evidence check failed: production dir ${prod_dir} carries Plan 232 surface (Plan 232 §4 forbids production changes)" >&2
      failures=$((failures + 1))
    fi
  done
  # Raw Java logs stay scratch-only: no P232 evidence row carries log
  # text or log paths.
  if grep "append_evidence" "${P232_DRIVER_TEST}" | grep -F "log-router" >/dev/null 2>&1; then
    echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} promotes raw router logs into P232 evidence (Plan 232 §5)" >&2
    failures=$((failures + 1))
  fi
  # 25h. The 18 required Plan 232 §12 unit rows.
  for unit_row in \
    'p232_destination_lease_gateway_matches_installed_inbound_route' \
    'p232_destination_lease_tunnel_matches_installed_inbound_route' \
    'p232_destination_publication_target_is_not_lease_gateway_source' \
    'p232_streaming_initial_gateway_matches_installed_inbound_route' \
    'p232_streaming_initial_tunnel_matches_installed_inbound_route' \
    'p232_streaming_refresh_revalidates_installed_inbound_route' \
    'p232_streaming_refresh_gateway_matches_installed_inbound_route' \
    'p232_streaming_refresh_tunnel_matches_installed_inbound_route' \
    'p232_route_derived_helper_rejects_missing_inbound_route' \
    'p232_route_derived_helper_rejects_slot_route_mismatch' \
    'p232_java_hash_cannot_be_hardcoded_as_local_lease_gateway' \
    'p232_all_java_local_lease_sites_use_route_derived_contract' \
    'p232_corrected_target_router_must_have_exact_ibgw_before_send' \
    'p232_reverse_pass_requires_exact_tunneldata_and_digest' \
    'p232_raw_reverse_pass_continues_to_streaming' \
    'p232_streaming_pass_can_close_java_second_family' \
    'p232_timeout_windows_remain_frozen' \
    'p232_no_production_surface_change'; do
    if ! grep -q "fn ${unit_row}" "${P232_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P232_DRIVER_TEST} lacks Plan 232 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

# Plan 234 §15 — bounded Streaming SYN epoch attribution and final-authority
# guards.  The surface must stay in the external driver/helper; no production
# crate may acquire Plan-234 tokens before an independently registered
# production corrective proves an i2pr-owned defect.
P234_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P234_HELPER_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceStreamingService.java"
P234_HARNESS="${REPO_ROOT}/tests/integration/m6-interop/run-java.sh"
P234_FINAL_CHECKER="${REPO_ROOT}/scripts/check-m6-final-closure-evidence.sh"
if [[ -f "${P234_DRIVER_TEST}" ]]; then
  for required in \
    'struct P234JavaAcceptState' \
    'fn p234_parse_java_accept_state' \
    'struct P234SynEpoch' \
    'enum P234Terminal' \
    'fn p234_classify_syn_epoch' \
    'fn record_p234_syn_epoch' \
    'p234-syn-epoch' \
    'p234-java-accept-state' \
    'p234-classification' \
    'P234-B-JAVA-ACCEPT-WORKER-NOT-STARTED' \
    'P234-B-JAVA-SYN-NOT-ACCEPTED' \
    'P234-B-JAVA-ACCEPTED-NO-RESPONSE-OBSERVED' \
    'P234-C-I2PR-NO-EXPECTED-TUNNELDATA' \
    'P234-C-I2PR-TUNNEL-RECOVERY-FAILED' \
    'P234-C-I2PR-GARLIC-DECODE-FAILED' \
    'P234-C-I2PR-NO-STREAMING-PAYLOAD' \
    'P234-C-I2PR-STREAMING-ADAPTER-FAILED' \
    'P234-C-DISPATCHED-NOT-ESTABLISHED' \
    'P234-C-STREAMING-DIRECTION-A-ESTABLISHED' \
    'P234-C-OBSERVABILITY-GAP'; do
    if ! grep -q -F "${required}" "${P234_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P234_DRIVER_TEST} lacks Plan 234 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for unit_row in \
    'p234_plan232_route_parity_is_prerequisite' \
    'p234_start_accept_precedes_syn_send' \
    'p234_java_accept_worker_state_is_bounded' \
    'p234_no_expected_tunneldata_is_distinct_from_decode_failure' \
    'p234_tunnel_recovery_failure_is_distinct_from_no_wire' \
    'p234_garlic_failure_is_distinct_from_streaming_adapter_failure' \
    'p234_adapter_dispatch_without_established_is_distinct' \
    'p234_direction_a_pass_does_not_close_java_family' \
    'p234_direction_b_requires_live_refresh_parity' \
    'p234_plan200_c_d_rows_require_pass_or_explicit_supersession_mapping' \
    'p234_superseded_row_requires_stronger_mandatory_external_evidence' \
    'p234_unmapped_legacy_required_row_blocks_closure' \
    'p234_run_java_exit_zero_required_for_family_pass' \
    'p234_final_closure_checker_required' \
    'p234_no_global_required_failed_bypass' \
    'p234_no_production_surface_change_before_owned_defect'; do
    if ! grep -q "fn ${unit_row}" "${P234_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P234_DRIVER_TEST} lacks Plan 234 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -rq -F 'p234' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null ||
     grep -rq -F 'P234' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: production Rust carries Plan 234 surface before an owned production corrective" >&2
    failures=$((failures + 1))
  fi
  if grep -q -E "record[[:space:]]+[\"']P234-" "${P234_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P234_DRIVER_TEST} invents a hard-coded Plan 234 terminal" >&2
    failures=$((failures + 1))
  fi
  if ! grep -q -F 'SYN_ACK_WAIT: Duration = Duration::from_secs(45)' "${P234_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: Plan 234 changed the frozen SYN-ACK window" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P234_HELPER_SRC}" ]]; then
  for required in \
    'REPORT_STREAM_STATE' \
    'ACCEPT_REQUESTED' \
    'ACCEPT_ENTERED' \
    'ACCEPT_RETURNED' \
    'ACCEPT_STORED' \
    'ACCEPT_ERRORS' \
    'STREAM_STATUS accept_requested=' \
    'Plan 234 §8'; do
    if ! grep -q -F "${required}" "${P234_HELPER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P234_HELPER_SRC} lacks Plan 234 helper surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -E 'packet|private key|secret|payload' <(grep -F 'STREAM_STATUS' "${P234_HELPER_SRC}"); then
    echo "m6 mixed-router evidence check failed: Plan 234 helper status exposes non-bounded data" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P234_HARNESS}" ]]; then
  for required in \
    'REQUIRED_FAILED=0' \
    '[[ "${REQUIRED_FAILED}" -ne 0 ]]' \
    'I2PR_M6_JAVA_DRIVER'; do
    if ! grep -q -F "${required}" "${P234_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P234_HARNESS} lacks fail-closed Plan 234 harness invariant '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P234_FINAL_CHECKER}" ]]; then
  for required in 'p234-classification' 'p235-classification' 'run-java.sh' 'm6_final_closure: passed'; do
    if ! grep -q -F "${required}" "${P234_FINAL_CHECKER}"; then
      echo "m6 mixed-router evidence check failed: final closure checker lacks Plan 234/235 invariant '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

# Plan 235 §D — post-accept response-boundary attribution. The corrective is
# intentionally restricted to the external Java helper and black-box driver;
# no production Rust surface may carry Plan-235 tokens before an independently
# registered production corrective proves an i2pr-owned defect.
P235_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P235_HELPER_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceStreamingService.java"
if [[ -f "${P235_DRIVER_TEST}" ]]; then
  for required in \
    'type P235JavaResponseState' \
    'fn p235_parse_java_response_state' \
    'enum P235Terminal' \
    'fn p235_classify_syn_epoch' \
    'fn record_p235_syn_epoch' \
    'p235-plan234-baseline' \
    'p235-syn-epoch' \
    'p235-java-response-state' \
    'p235-classification' \
    'P235-A-PLAN234-BASELINE-REGRESSION' \
    'P235-B-JAVA-SOCKET-SURFACE-READY-NO-I2PR-INBOUND' \
    'P235-C-I2PR-OUTBOUND-ADMISSION-FAILED' \
    'P235-C-I2PR-UNRELATED-TUNNELDATA' \
    'P235-JAVA-STREAMING-PASSED'; do
    if ! grep -q -F "${required}" "${P235_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P235_DRIVER_TEST} lacks Plan 235 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for unit_row in \
    'p235_baseline_regression_wins_before_response_attribution' \
    'p235_socket_surface_ready_without_wire_is_exact_boundary' \
    'p235_outbound_rejection_precedes_no_wire' \
    'p235_unrelated_tunneldata_is_distinct_from_no_wire' \
    'p235_terminal_tokens_are_bounded_and_scoped'; do
    if ! grep -q "fn ${unit_row}" "${P235_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P235_DRIVER_TEST} lacks Plan 235 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -rq -F 'p235' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null ||
     grep -rq -F 'P235' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: production Rust carries Plan 235 surface before an owned production corrective" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P235_HELPER_SRC}" ]]; then
  for required in \
    'SOCKET_SURFACE_ENTERED' \
    'SOCKET_SURFACE_READY' \
    'SOCKET_SURFACE_ERRORS' \
    'socket_surface_entered=' \
    'socket_surface_ready=' \
    'socket_surface_errors=' \
    'Plan 235 §B'; do
    if ! grep -q -F "${required}" "${P235_HELPER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P235_HELPER_SRC} lacks Plan 235 helper surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

# Plan 236 §6–§17 — response-emission attribution is test-only, source-locked,
# and fail-closed. The checker requires the complete typed boundary surface
# while refusing any Plan-236 token in production Rust.
P236_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P236_HELPER_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceStreamingService.java"
P236_SOURCE_LOCK="${REPO_ROOT}/scripts/interop/check-m6-java-response-source-lock.sh"
if [[ ! -x "${P236_SOURCE_LOCK}" ]]; then
  echo "m6 mixed-router evidence check failed: missing executable Plan 236 source-lock checker" >&2
  failures=$((failures + 1))
fi
if [[ -f "${P236_DRIVER_TEST}" ]]; then
  for required in \
    'struct P236JavaResponseState' \
    'fn p236_parse_java_response_state' \
    'enum P236Terminal' \
    'fn p236_classify_syn_epoch' \
    'fn record_p236_response_epoch' \
    'p236-response-epoch' \
    'p236-classification' \
    'P236-C-JAVA-RESPONSE-EMISSION-OBSERVABILITY-GAP' \
    'P236-F-DIRECTION-A-ESTABLISHED'; do
    if ! grep -q -F "${required}" "${P236_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P236_DRIVER_TEST} lacks Plan 236 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for unit_row in \
    'p236_plan235_baseline_precedes_response_attribution' \
    'p236_source_lock_names_exact_pinned_response_path' \
    'p236_scheduler_missing_precedes_packetqueue_missing' \
    'p236_packet_not_constructed_precedes_sendpacket_missing' \
    'p236_sendpacket_missing_precedes_packetqueue_missing' \
    'p236_packetqueue_failure_precedes_router_i2cp_missing' \
    'p236_i2psession_send_failure_precedes_router_i2cp_missing' \
    'p236_ack_only_path_does_not_require_status_listener' \
    'p236_router_i2cp_missing_is_distinct_from_no_target_leaseset' \
    'p236_no_outbound_tunnel_is_distinct_from_gateway_enqueue_failure' \
    'p236_target_ibgw_uses_route_derived_lease_not_publication_target' \
    'p236_no_expected_tunneldata_is_distinct_from_recovery_failure' \
    'p236_i2pr_owned_terminal_requires_expected_tunneldata' \
    'p236_direction_a_established_does_not_by_itself_close_m6' \
    'p236_workspace_sam_hang_cannot_be_recorded_as_pass' \
    'p236_no_production_change_before_owned_defect'; do
    if ! grep -q "fn ${unit_row}" "${P236_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P236_DRIVER_TEST} lacks Plan 236 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -rq -F 'P236' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null ||
     grep -rq -F 'p236' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: production Rust carries Plan 236 surface" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P236_HELPER_SRC}" ]]; then
  for required in \
    'java_source_pin=' \
    'java_response_scheduler_class=' \
    'java_response_send_method=' \
    'java_packetqueue_method=' \
    'java_i2psession_send_method=' \
    'java_response_observation_complete=false' \
    'Plan 236 §6'; do
    if ! grep -q -F "${required}" "${P236_HELPER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P236_HELPER_SRC} lacks Plan 236 helper surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${JAVA_HARNESS}" ]]; then
  for required in \
    'check-m6-java-response-source-lock.sh' \
    'JAVA_SOURCE_ROOT' \
    'java-response-source-lock.tsv'; do
    if ! grep -q -F "${required}" "${JAVA_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${JAVA_HARNESS} lacks Plan 236 source-lock invariant '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi

# ---- 26. Plan 237 stock-response observability corrective -----------------
# Plan 237 replaces the Plan-236 literal response placeholders with real
# bounded stock-Java observations (helper-JVM `stream.con.sendMessageSize`
# lifetime events + exact source-locked DEBUG/WARN log counts through one
# tiny public `REPORT_RESPONSE_STATS` command) and classifies the earliest
# response stage from isolated-epoch deltas. The existing Plan-236 source
# lock remains required and is extended with the pinned log/stat signals.
P237_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P237_HELPER_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceStreamingService.java"
P237_SOURCE_LOCK="${REPO_ROOT}/scripts/interop/check-m6-java-response-source-lock.sh"
if [[ -f "${P237_DRIVER_TEST}" ]]; then
  for required in \
    'struct P237ResponseStats' \
    'fn p237_parse_response_stats' \
    'struct P237Deltas' \
    'fn p237_deltas' \
    'struct P237RouterStages' \
    'enum P237Terminal' \
    'fn p237_classify_response' \
    'fn record_p237_response_epoch' \
    'REPORT_RESPONSE_STATS' \
    'send_message_size_lifetime_events' \
    'p237-response-stats-pre' \
    'p237-response-stats-post' \
    'p237-response-deltas' \
    'p237-router-stages' \
    'p237-classification' \
    'P237-A-RESPONSE-OBSERVATION-NOT-ISOLATABLE' \
    'P237-B-SCHEDULER-NOT-OBSERVED' \
    'P237-B-SCHEDULER-OBSERVED-NO-ACK-CONSTRUCTION' \
    'P237-C-ACK-CONSTRUCTED-NO-SENDMESSAGE' \
    'P237-C-SENDMESSAGE-FAILED' \
    'P237-D-ROUTER-I2CP-NOT-OBSERVED' \
    'P237-E-I2PR-NO-EXPECTED-TUNNELDATA' \
    'P237-F-I2PR-TUNNEL-RECOVERY-FAILED' \
    'P237-F-I2PR-GARLIC-DECODE-FAILED' \
    'P237-F-I2PR-STREAMING-ADAPTER-FAILED' \
    'P237-F-DIRECTION-A-ESTABLISHED'; do
    if ! grep -q -F "${required}" "${P237_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P237_DRIVER_TEST} lacks Plan 237 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  for unit_row in \
    p237_placeholder_false_is_unknown_not_negative_evidence \
    p237_scheduler_requires_enabled_stock_observer \
    p237_scheduler_precedes_ack_construction \
    p237_ack_construction_precedes_sendmessage \
    p237_sendmessage_event_delta_proves_call_returned \
    p237_send_failure_precedes_router_attribution \
    p237_sendmessage_returned_does_not_prove_router_admission \
    p237_router_attribution_requires_sendmessage_returned \
    p237_i2pr_owned_terminal_requires_expected_tunneldata \
    p237_accept_returned_does_not_satisfy_response_stage \
    p237_socket_surface_ready_does_not_satisfy_response_stage \
    p237_no_production_change; do
    if ! grep -q "fn ${unit_row}" "${P237_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P237_DRIVER_TEST} lacks Plan 237 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
  # Response stages must be satisfied by stock deltas, never by the
  # Plan-235 socket surface: the P237 classifier body must not consume
  # `accept_returned` or `socket_surface_ready`.
  if grep -n 'fn p237_classify_response' "${P237_DRIVER_TEST}" >/dev/null 2>&1; then
    p237_body="$(awk '/fn p237_classify_response/,/^}/' "${P237_DRIVER_TEST}")"
    if printf '%s' "${p237_body}" | grep -q 'accept_returned\|socket_surface_ready'; then
      echo "m6 mixed-router evidence check failed: ${P237_DRIVER_TEST} satisfies a P237 response stage from accept_returned/socket_surface_ready (Plan 237 §11.3)" >&2
      failures=$((failures + 1))
    fi
  fi
  # No invented P237 terminals: classification rows flow only through the
  # typed `p237-classification` evidence key.
  if rg -n "^[[:space:]]*record[[:space:]]+[\"']P237-" "${P237_DRIVER_TEST}" >/dev/null; then
    echo "m6 mixed-router evidence check failed: ${P237_DRIVER_TEST} hard-codes a Plan 237 terminal record" >&2
    failures=$((failures + 1))
  fi
  # Frozen acceptance windows stay frozen on the streaming path.
  for frozen in \
    'const DATAGRAM_WAIT: Duration = Duration::from_secs(45)' \
    'const STREAM_WAIT: Duration = Duration::from_secs(45)' \
    'const SYN_ACK_WAIT: Duration = Duration::from_secs(45)'; do
    if ! grep -q -F "${frozen}" "${P237_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P237_DRIVER_TEST} changed frozen window '${frozen}' (Plan 237 §2 retained)" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -rq -F 'P237' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null ||
     grep -rq -F 'p237' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: production Rust carries Plan 237 surface" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P237_HELPER_SRC}" ]]; then
  for required in \
    'REPORT_RESPONSE_STATS' \
    'RESPONSE_STATS scheduler_log_count=' \
    'send_message_size_lifetime_events=' \
    'scheduler_debug_enabled=' \
    'getLifetimeEventCount' \
    'getMostRecentMessages' \
    'setLimits' \
    'setConsoleBufferSize' \
    'SchedulerImpl' \
    'received con... ' \
    'Resend in ' \
    'Send failed for ' \
    'Unable to send the packet' \
    'Plan 237 §'; do
    if ! grep -q -F "${required}" "${P237_HELPER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P237_HELPER_SRC} lacks Plan 237 helper surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # No literal hard-coded positive response observations: counts must be
  # computed from the stock observer, never assigned as literals.
  if grep -n -E 'scheduler_log_count=[1-9]|ack_constructed_log_count=[1-9]|send_message_size_lifetime_events=[1-9]|send_failure_count=[1-9]|send_exception_count=[1-9]' "${P237_HELPER_SRC}" \
     | grep -v 'p237CountBufferSubstring\|p237SendMessageSizeLifetimeEvents\|schedulerCount\|ackCount\|sendEvents\|sendFail\|sendException' \
     >/dev/null 2>&1; then
    echo "m6 mixed-router evidence check failed: ${P237_HELPER_SRC} hard-codes a positive P237 response observation (Plan 237 §11.1)" >&2
    failures=$((failures + 1))
  fi
  # No Java source patching surface: read-only public APIs only, never
  # reflection, state mutation, or router-internals access.
  for forbidden in \
    'getDeclaredField' \
    'setAccessible' \
    'registerKeys' \
    'unregisterKeys' \
    '.store(' \
    '.publish(' \
    'import java.lang.reflect'; do
    if grep -q -F "${forbidden}" "${P237_HELPER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P237_HELPER_SRC} carries forbidden Java mutation/reflection '${forbidden}' (Plan 237 §5)" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P237_SOURCE_LOCK}" ]]; then
  for required in \
    'received con... send a packet' \
    'received con... time till next send: ' \
    'getLog(SchedulerImpl.class)' \
    'Resend in ' \
    'stream.con.sendMessageSize' \
    'Unable to send the packet' \
    'Send failed for ' \
    'ms to sendMessage(...)'; do
    if ! grep -q -F "${required}" "${P237_SOURCE_LOCK}"; then
      echo "m6 mixed-router evidence check failed: ${P237_SOURCE_LOCK} lacks Plan 237 pinned signal '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${JAVA_HARNESS}" ]]; then
  for required in \
    'external-p237-classification' \
    'p237-classification' \
    'p237-response-deltas' \
    'p237-response-stats-post'; do
    if ! grep -q -F "${required}" "${JAVA_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${JAVA_HARNESS} lacks Plan 237 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -E "record[[:space:]]+[\"']P237-" "${JAVA_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${JAVA_HARNESS} invents a Plan 237 terminal" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 27. Plan 238 Router-A admission observer -----------------------------
# Plan 238 observes the first Router-A stage of the Plan-237 proven
# sendMessage epoch with a read-only stock StatManager snapshot
# (`client.distributeTime` admission + `client.dispatchTime` /
# `client.dispatchSendTime` dispatch corroboration +
# `tunnel.dispatchOutboundTunnel` handoff context) through one tiny
# public `P238-ADMISSION` diagnostic command, feeds the isolated-epoch
# deltas into the retained `P237RouterStages` input, and stops at the
# earliest proven D/E stage. No production Rust change, no Java source
# patch, no topology/profile/timing/pin change, no raw-log promotion.
P238_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P238Probe.java"
P238_LAUNCHER_SRC="${JAVA_LAUNCHER_SRC}"
P238_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P238_HARNESS="${JAVA_HARNESS}"
P238_SOURCE_LOCK="${REPO_ROOT}/scripts/interop/check-m6-java-response-source-lock.sh"
if [[ ! -f "${P238_PROBE_SRC}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 238 probe ${P238_PROBE_SRC}" >&2
  failures=$((failures + 1))
else
  # 27a. Read-only admission surface (exact pinned accessors).
  for required in \
    'snapshotAdmission' \
    'statManager().getRate' \
    'getLifetimeEventCount' \
    'client.distributeTime' \
    'client.dispatchTime' \
    'client.dispatchSendTime' \
    'tunnel.dispatchOutboundTunnel' \
    'COUNT_UNKNOWN'; do
    if ! grep -q -F "${required}" "${P238_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P238_PROBE_SRC} lacks Plan 238 probe surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 27b. Probe-side mutation, NetDB/tunnel/queue/profile/stat writes,
  # reflection, private-field access, and mainline creation calls are
  # forbidden. Rate creation would let a probe fabricate the very
  # counts it must observe read-only.
  for forbidden in \
    '.addProfile(' \
    '.getOrCreateProfile' \
    'heardAbout(' \
    'registerKeys' \
    'unregisterKeys' \
    '.store(' \
    '.publish(' \
    'setKeys' \
    'buildTunnels' \
    'addTunnel' \
    'createRateStat' \
    'createRequiredRateStat' \
    'createFrequencyStat' \
    'addRateData' \
    'getDeclaredField' \
    'setAccessible' \
    'import java.lang.reflect'; do
    if grep -q -F "${forbidden}" "${P238_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P238_PROBE_SRC} uses forbidden Plan 238 probe call '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'explicitPeers' "${P238_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P238_PROBE_SRC} mentions explicitPeers (Plan 238 §5 out of scope)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'vmCommSystem' "${P238_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P238_PROBE_SRC} enables VMComm (Plan 238 §5 out of scope)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'netDb.alwaysQuery' "${P238_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P238_PROBE_SRC} enables netDb.alwaysQuery (Plan 238 §5 out of scope)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P238_LAUNCHER_SRC}" ]]; then
  # 27c. The launcher serves the single read-only P238 command.
  for required in \
    '"P238-ADMISSION"' \
    'P238Probe' \
    'p238Admission' \
    'kind=admission' \
    'observable=true' \
    'distribute_time=' \
    'dispatch_time=' \
    'dispatch_send_time=' \
    'dispatch_outbound_tunnel='; do
    if ! grep -q -F "${required}" "${P238_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P238_LAUNCHER_SRC} lacks Plan 238 launcher surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'forceBandwidthClass' "${P238_LAUNCHER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P238_LAUNCHER_SRC} uses router.forceBandwidthClass (Plan 238 §4 forbids it)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P238_DRIVER_TEST}" ]]; then
  # 27d. The Rust driver owns the admission parse/collect/delta/stage
  # mapping plus additive evidence rows. Router-A stages feed the
  # retained P237 classifier; no new terminal is invented.
  for required in \
    'struct P238Admission' \
    'fn p238_parse_admission' \
    'fn p238_collect_admission' \
    'struct P238Deltas' \
    'fn p238_count_delta' \
    'fn p238_deltas' \
    'fn p238_router_stages_from_admission' \
    'fn record_p238_admission_epoch' \
    'P238-ADMISSION' \
    'p238-admission-pre' \
    'p238-admission-post' \
    'p238-admission-deltas' \
    'distribute_time=' \
    'JAVA_DIAGNOSTIC_A_PORT'; do
    if ! grep -q -F "${required}" "${P238_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P238_DRIVER_TEST} lacks Plan 238 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 27e. No Router-A claim before sendMessage: the stage mapper must
  # gate `client_message_admitted` on `router_i2cp_observed`, and the
  # retained P237 classifier must still prove scheduler/ack/sendMessage
  # before any router attribution.
  if ! grep -q -F 'let client_message_admitted = router_i2cp_observed && dispatch_observed' "${P238_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P238_DRIVER_TEST} lets client_message_admitted skip router_i2cp_observed (Plan 238 §13 stop rule)" >&2
    failures=$((failures + 1))
  fi
  if grep -n 'fn p238_router_stages_from_admission' "${P238_DRIVER_TEST}" >/dev/null 2>&1; then
    p238_map_body="$(awk '/fn p238_router_stages_from_admission/,/^}/' "${P238_DRIVER_TEST}")"
    if printf '%s' "${p238_map_body}" | grep -q 'target_leaseset_selected: true\|outbound_tunnel_selected: true\|dispatch_outbound_called: true\|outbound_gateway_enqueued: true\|transit_processed: true\|target_ibgw_present: true\|target_ibgw_dispatched: true'; then
      echo "m6 mixed-router evidence check failed: ${P238_DRIVER_TEST} claims a post-admission D/E stage from the P238 observer (Plan 238 §15: later stages stay Unknown)" >&2
      failures=$((failures + 1))
    fi
  fi
  # No invented P238 terminals: Router-A rows flow only through the
  # retained `p237-classification` plus additive `p238-admission-*` keys.
  if rg -n "^[[:space:]]*record[[:space:]]+[\"']P238-" "${P238_DRIVER_TEST}" >/dev/null; then
    echo "m6 mixed-router evidence check failed: ${P238_DRIVER_TEST} hard-codes a Plan 238 terminal record" >&2
    failures=$((failures + 1))
  fi
  # Frozen acceptance windows stay frozen on the streaming path.
  for frozen in \
    'const DATAGRAM_WAIT: Duration = Duration::from_secs(45)' \
    'const STREAM_WAIT: Duration = Duration::from_secs(45)' \
    'const SYN_ACK_WAIT: Duration = Duration::from_secs(45)'; do
    if ! grep -q -F "${frozen}" "${P238_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P238_DRIVER_TEST} changed frozen window '${frozen}' (Plan 238 §4 retained)" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -rq -F 'P238' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null ||
     grep -rq -F 'p238' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: production Rust carries Plan 238 surface" >&2
    failures=$((failures + 1))
  fi
  # 27f. The 12 required Plan 238 §9 unit rows.
  for unit_row in \
    p238_admission_parser_accepts_valid_and_rejects_malformed \
    p238_unknown_admission_stays_at_router_i2cp \
    p238_zero_delta_with_enabled_stat_is_proven_absence_not_promotion \
    p238_distribute_delta_proves_router_i2cp \
    p238_dispatch_delta_proves_client_admission \
    p238_router_admission_requires_sendmessage_returned \
    p238_later_stage_cannot_skip_earlier_unknown \
    p238_send_failure_precedes_router_admission \
    p238_outbound_tunnel_context_never_satisfies_stage_alone \
    p238_i2pr_owned_terminal_requires_expected_tunneldata \
    p238_accept_socket_surface_satisfy_nothing \
    p238_no_production_change; do
    if ! grep -q "fn ${unit_row}" "${P238_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P238_DRIVER_TEST} lacks Plan 238 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
# 27g. The streaming helper stays frozen: the Router-A observer lives
# in the ControlledRouter diagnostic, never in the client helper.
if grep -q -F 'P238' "${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceStreamingService.java" 2>/dev/null; then
  echo "m6 mixed-router evidence check failed: ReferenceStreamingService.java carries Plan 238 surface (Plan 238 §5: observer lives on Router A)" >&2
  failures=$((failures + 1))
fi
if [[ -f "${P238_SOURCE_LOCK}" ]]; then
  for required in \
    'void handleSendMessage(SendMessageMessage message)' \
    '"client.distributeTime"' \
    'OutboundClientMessageOneShotJob.init' \
    '"client.dispatchTime"' \
    '"client.dispatchSendTime"' \
    'dispatchOutbound(' \
    '"tunnel.dispatchOutboundTunnel"'; do
    if ! grep -q -F "${required}" "${P238_SOURCE_LOCK}"; then
      echo "m6 mixed-router evidence check failed: ${P238_SOURCE_LOCK} lacks Plan 238 pinned signal '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P238_HARNESS}" ]]; then
  for required in \
    'P238Probe.java' \
    'JAVA_DIAGNOSTIC_A_PORT' \
    'p238-admission-deltas' \
    'p238-admission-post' \
    'external-p238-admission-deltas' \
    'external-p238-admission-post'; do
    if ! grep -q -F "${required}" "${P238_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P238_HARNESS} lacks Plan 238 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -E "record[[:space:]]+[\"']P238-" "${P238_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P238_HARNESS} invents a Plan 238 terminal" >&2
    failures=$((failures + 1))
  fi
  # No P238 row may carry topology/profile/tunnel-policy changes,
  # timeout overrides, public-I2P/reseed/VMComm/alwaysQuery escapes,
  # or raw log promotion relative to Plan 237.
  p238_harness_lines="$(grep -n -E 'p238|P238' "${P238_HARNESS}" || true)"
  for forbidden in \
    'explicitPeers' \
    'forceBandwidthClass' \
    'netDb.alwaysQuery' \
    'vmCommSystem' \
    'reseed' \
    'floodfillParticipant' \
    'Pool.length' \
    'DRIVER_TIMEOUT=' \
    'DATAGRAM' \
    'log-router'; do
    if printf '%s\n' "${p238_harness_lines}" | grep -q -F "${forbidden}"; then
      echo "m6 mixed-router evidence check failed: ${P238_HARNESS} P238 row carries forbidden surface '${forbidden}' (Plan 238 §4/§8)" >&2
      failures=$((failures + 1))
    fi
  done
fi

# ---- 28. Plan 239 Router-A pre-dispatch / OCMOSJ attribution ------------
# Plan 239 attributes the first post-admission OCMOSJ stages of the
# Plan-238 proven sendMessage epoch with a read-only stock StatManager
# + client-subDB + tunnel-pool + LogManager-buffer snapshot
# (`client.leaseSetFoundRemoteTime` / `client.leaseSetFailedRemoteTime`
# lookup, `client.dispatchNoTunnels` / `client.dispatchPrepareTime` /
# `client.dispatchTime` / `client.dispatchSendTime` dispatch, local LS
# vs remote lookup, installed inbound/outbound tunnels, exact-source
# OCMOSJ log counts for the two `dispatchNoTunnels` branches and
# pre-dispatch LeaseSet failures) through one tiny public
# `P239-DISPATCH` diagnostic command, feeds the isolated-epoch deltas
# into the ordered D1/D2/D3 classifier with §8 continuation only after
# dispatch is proven, and stops at the earliest proven D/E/F stage. No
# production Rust change, no Java source patch, no
# topology/profile/timing/pin change, no raw-log promotion.
P239_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P239Probe.java"
P239_LAUNCHER_SRC="${JAVA_LAUNCHER_SRC}"
P239_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P239_HARNESS="${JAVA_HARNESS}"
P239_SOURCE_LOCK="${REPO_ROOT}/scripts/interop/check-m6-java-response-source-lock.sh"
if [[ ! -f "${P239_PROBE_SRC}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 239 probe ${P239_PROBE_SRC}" >&2
  failures=$((failures + 1))
else
  # 28a. Read-only pre-dispatch surface (exact pinned accessors).
  for required in \
    'snapshotDispatch' \
    'statManager().getRate' \
    'getLifetimeEventCount' \
    'client.leaseSetFoundRemoteTime' \
    'client.leaseSetFailedRemoteTime' \
    'client.dispatchNoTunnels' \
    'client.dispatchPrepareTime' \
    'client.dispatchTime' \
    'client.dispatchSendTime' \
    'Could not find any outbound tunnels to send the payload through' \
    'Unable to create the garlic message (no tunnels left or too lagged)' \
    'Lookup locally didn' \
    'Only have RAP LS for ' \
    'Got the lease but can' \
    'No leases found from: ' \
    'getOutboundPool' \
    'getInboundPool' \
    'listTunnels' \
    'getSendTunnelId' \
    'getReceiveTunnelId' \
    'getBuffer' \
    'getMostRecentMessages' \
    'COUNT_UNKNOWN'; do
    if ! grep -q -F "${required}" "${P239_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P239_PROBE_SRC} lacks Plan 239 probe surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 28b. Probe-side mutation, NetDB/tunnel/queue/profile/stat writes,
  # reflection, private-field access, and mainline creation calls are
  # forbidden. Rate creation would let a probe fabricate the very
  # counts it must observe read-only.
  for forbidden in \
    '.addProfile(' \
    '.getOrCreateProfile' \
    'heardAbout(' \
    'registerKeys' \
    'unregisterKeys' \
    '.store(' \
    '.publish(' \
    'setKeys' \
    'buildTunnels' \
    'addTunnel' \
    'createRateStat' \
    'createRequiredRateStat' \
    'createFrequencyStat' \
    'addRateData' \
    'getDeclaredField' \
    'setAccessible' \
    'import java.lang.reflect'; do
    if grep -q -F "${forbidden}" "${P239_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P239_PROBE_SRC} uses forbidden Plan 239 probe call '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'explicitPeers' "${P239_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P239_PROBE_SRC} mentions explicitPeers (Plan 239 §4 out of scope)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'vmCommSystem' "${P239_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P239_PROBE_SRC} enables VMComm (Plan 239 §4 out of scope)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'netDb.alwaysQuery' "${P239_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P239_PROBE_SRC} enables netDb.alwaysQuery (Plan 239 §4 out of scope)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P239_LAUNCHER_SRC}" ]]; then
  # 28c. The launcher serves the single read-only P239 command.
  for required in \
    '"P239-DISPATCH"' \
    'P239Probe' \
    'p239Dispatch' \
    'kind=dispatch' \
    'observable=true' \
    'target_ls_local_present=' \
    'lease_lookup_found_remote_events=' \
    'lease_lookup_failed_remote_events=' \
    'client_outbound_tunnel_count=' \
    'client_inbound_tunnel_count=' \
    'dispatch_no_tunnels_events=' \
    'dispatch_prepare_events=' \
    'dispatch_time_events=' \
    'dispatch_send_time_events=' \
    'log_no_outbound_tunnel_count=' \
    'log_garlic_no_tunnel_count=' \
    'log_local_ls_missing_count=' \
    'log_only_rap_ls_count=' \
    'log_bad_or_unsupported_ls_count='; do
    if ! grep -q -F "${required}" "${P239_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P239_LAUNCHER_SRC} lacks Plan 239 launcher surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'forceBandwidthClass' "${P239_LAUNCHER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P239_LAUNCHER_SRC} uses router.forceBandwidthClass (Plan 239 §4 forbids it)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P239_DRIVER_TEST}" ]]; then
  # 28d. The Rust driver owns the dispatch parse/collect/delta/stage
  # mapping plus additive evidence rows. Admission (Plan 238) is a
  # prerequisite; later stages never skip earlier Unknown.
  for required in \
    'struct P239Dispatch' \
    'fn p239_parse_dispatch' \
    'fn p239_collect_dispatch' \
    'struct P239Deltas' \
    'fn p239_count_delta' \
    'fn p239_deltas' \
    'fn p239_admission_is_proven' \
    'fn p239_local_leaseset_path' \
    'fn p239_dispatch_proven' \
    'enum P239Terminal' \
    'fn p239_classify_dispatch' \
    'fn record_p239_dispatch_epoch' \
    'P239-DISPATCH' \
    'p239-dispatch-pre' \
    'p239-dispatch-post' \
    'p239-dispatch-deltas' \
    'p239-classification' \
    'P239-A-ROUTER-A-EPOCH-NOT-ISOLATABLE' \
    'P239-D-TARGET-LEASESET-LOOKUP-FAILED' \
    'P239-D-TARGET-LEASESET-UNUSABLE' \
    'P239-D-TARGET-LEASESET-DECISION-UNKNOWN' \
    'P239-D-NO-OUTBOUND-TUNNEL' \
    'P239-D-GARLIC-TUNNEL-MATERIAL-UNAVAILABLE' \
    'P239-D-NO-TUNNELS-BRANCH-AMBIGUOUS' \
    'P239-D-DISPATCH-OUTBOUND-PROVEN' \
    'P239-D-PRE-DISPATCH-OBSERVABILITY-GAP' \
    'P239-E-OUTBOUND-GATEWAY-NOT-ENQUEUED' \
    'P239-E-I2PR-NO-EXPECTED-TUNNELDATA' \
    'P239-F-I2PR-TUNNEL-RECOVERY-FAILED' \
    'P239-F-I2PR-GARLIC-DECODE-FAILED' \
    'P239-F-I2PR-STREAMING-ADAPTER-FAILED' \
    'P239-F-DIRECTION-A-ESTABLISHED' \
    'JAVA_DIAGNOSTIC_A_PORT'; do
    if ! grep -q -F "${required}" "${P239_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P239_DRIVER_TEST} lacks Plan 239 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 28e. Admission prerequisite: the classifier must gate on
  # `p239_admission_is_proven`, and dispatch must require both
  # dispatch deltas (prepare alone never proves dispatch).
  if ! grep -q -F 'p239_admission_is_proven' "${P239_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P239_DRIVER_TEST} lets P239 claim without Plan 238 admission (Plan 239 §7 prerequisite)" >&2
    failures=$((failures + 1))
  fi
  if grep -n 'fn p239_dispatch_proven' "${P239_DRIVER_TEST}" >/dev/null 2>&1; then
    p239_dispatch_body="$(awk '/fn p239_dispatch_proven/,/^}/' "${P239_DRIVER_TEST}")"
    if ! printf '%s' "${p239_dispatch_body}" | grep -q 'dispatch_time_delta'; then
      echo "m6 mixed-router evidence check failed: ${P239_DRIVER_TEST} proves dispatch without dispatch_time_delta (Plan 239 §7 D3)" >&2
      failures=$((failures + 1))
    fi
    if ! printf '%s' "${p239_dispatch_body}" | grep -q 'dispatch_send_delta'; then
      echo "m6 mixed-router evidence check failed: ${P239_DRIVER_TEST} proves dispatch without dispatch_send_delta (Plan 239 §7 D3)" >&2
      failures=$((failures + 1))
    fi
  fi
  # No Router-A claim before admission: the live mapper must gate
  # dispatch stages on admission, and the classifier must return the
  # A boundary when admission is missing.
  if ! grep -q -F 'P239-A-ROUTER-A-EPOCH-NOT-ISOLATABLE' "${P239_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P239_DRIVER_TEST} lacks the Plan 239 A epoch boundary" >&2
    failures=$((failures + 1))
  fi
  # No invented P239 terminals: Router-A rows flow only through the
  # typed `p239-classification` plus additive `p239-dispatch-*` keys.
  if rg -n "^[[:space:]]*record[[:space:]]+[\"']P239-" "${P239_DRIVER_TEST}" >/dev/null; then
    echo "m6 mixed-router evidence check failed: ${P239_DRIVER_TEST} hard-codes a Plan 239 terminal record" >&2
    failures=$((failures + 1))
  fi
  # Frozen acceptance windows stay frozen on the streaming path.
  for frozen in \
    'const DATAGRAM_WAIT: Duration = Duration::from_secs(45)' \
    'const STREAM_WAIT: Duration = Duration::from_secs(45)' \
    'const SYN_ACK_WAIT: Duration = Duration::from_secs(45)'; do
    if ! grep -q -F "${frozen}" "${P239_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P239_DRIVER_TEST} changed frozen window '${frozen}' (Plan 239 §2 retained)" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -rq -F 'P239' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null ||
     grep -rq -F 'p239' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: production Rust carries Plan 239 surface" >&2
    failures=$((failures + 1))
  fi
  # 28f. The 11 required Plan 239 §10 unit rows.
  for unit_row in \
    p239_plan238_admission_is_prerequisite \
    p239_zero_remote_lookup_delta_does_not_mean_no_leaseset \
    p239_local_leaseset_path_precedes_remote_lookup_interpretation \
    p239_remote_lookup_failure_precedes_tunnel_attribution \
    p239_unknown_rate_is_not_zero \
    p239_dispatch_no_tunnels_requires_branch_discriminator \
    p239_outbound_tunnel_failure_distinct_from_garlic_tunnel_failure \
    p239_dispatch_time_proves_dispatch_outbound_returned \
    p239_dispatch_prepare_is_corroboration_not_primary_dispatch_proof \
    p239_post_dispatch_stages_require_dispatch_outbound \
    p239_no_production_change; do
    if ! grep -q "fn ${unit_row}" "${P239_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P239_DRIVER_TEST} lacks Plan 239 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
# 28g. The streaming helper stays frozen: the Router-A observer lives
# in the ControlledRouter diagnostic, never in the client helper.
if grep -q -F 'P239' "${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceStreamingService.java" 2>/dev/null; then
  echo "m6 mixed-router evidence check failed: ReferenceStreamingService.java carries Plan 239 surface (Plan 239 §4: observer lives on Router A)" >&2
  failures=$((failures + 1))
fi
if [[ -f "${P239_SOURCE_LOCK}" ]]; then
  for required in \
    'ctx.clientNetDb(_from.calculateHash()).lookupLeaseSetLocally(toHash)' \
    '"client.leaseSetFoundRemoteTime"' \
    '"client.leaseSetFailedRemoteTime"' \
    '"client.dispatchNoTunnels"' \
    'Could not find any outbound tunnels to send the payload through' \
    'Unable to create the garlic message (no tunnels left or too lagged)' \
    '"client.dispatchPrepareTime"' \
    'tunnelDispatcher().dispatchOutbound' \
    '"client.dispatchTime"' \
    '"client.dispatchSendTime"'; do
    if ! grep -q -F "${required}" "${P239_SOURCE_LOCK}"; then
      echo "m6 mixed-router evidence check failed: ${P239_SOURCE_LOCK} lacks Plan 239 pinned signal '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P239_HARNESS}" ]]; then
  for required in \
    'P239Probe.java' \
    'JAVA_DIAGNOSTIC_A_PORT' \
    'p239-dispatch-deltas' \
    'p239-dispatch-post' \
    'p239-classification' \
    'external-p239-dispatch-deltas' \
    'external-p239-dispatch-post' \
    'external-p239-classification'; do
    if ! grep -q -F "${required}" "${P239_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P239_HARNESS} lacks Plan 239 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -E "record[[:space:]]+[\"']P239-" "${P239_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P239_HARNESS} invents a Plan 239 terminal" >&2
    failures=$((failures + 1))
  fi
  # No P239 row may carry topology/profile/tunnel-policy changes,
  # timeout overrides, public-I2P/reseed/VMComm/alwaysQuery escapes,
  # or raw log promotion relative to Plan 238.
  p239_harness_lines="$(grep -n -E 'p239|P239' "${P239_HARNESS}" || true)"
  for forbidden in \
    'explicitPeers' \
    'forceBandwidthClass' \
    'netDb.alwaysQuery' \
    'vmCommSystem' \
    'reseed' \
    'floodfillParticipant' \
    'Pool.length' \
    'DRIVER_TIMEOUT=' \
    'DATAGRAM' \
    'log-router'; do
    if printf '%s\n' "${p239_harness_lines}" | grep -q -F "${forbidden}"; then
      echo "m6 mixed-router evidence check failed: ${P239_HARNESS} P239 row carries forbidden surface '${forbidden}' (Plan 239 §4/§8)" >&2
      failures=$((failures + 1))
    fi
  done
fi

# ---- 29. Plan 240 Streaming target-LeaseSet lookup-failure attribution --
# Plan 240 attributes the exact reason the Plan-239 Streaming response
# cannot resolve the i2pr target LeaseSet from Router A's helper client
# NetDB. It reuses the retained P224/P225/P226 surfaces inside the
# proven Streaming response epoch: a read-only Router-B readiness
# snapshot (`P240-ROUTER-B`), the exact streaming target-job correlation
# (helper DBID + target hash + ISJ job ID), the source-locked
# `New ISJ ... toTry:` membership for B, the exact-job pre-query guard
# ordering (IP-close, old-router, tunnels, reply-crypto, zero-hop,
# encrypted-prep), and the retained P225 post-query chain. No Java
# source patch, no production Rust change, no topology/profile/timing/
# pin change, no raw-log promotion, no standalone lookup.
P240_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P240Probe.java"
P240_LAUNCHER_SRC="${JAVA_LAUNCHER_SRC}"
P240_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P240_HARNESS="${JAVA_HARNESS}"
P240_SOURCE_LOCK="${REPO_ROOT}/scripts/interop/check-m6-java-response-source-lock.sh"
if [[ ! -f "${P240_PROBE_SRC}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 240 probe ${P240_PROBE_SRC}" >&2
  failures=$((failures + 1))
else
  # 29a. Read-only Router-B readiness surface (exact pinned accessors).
  for required in \
    'snapshotRouterB' \
    'lookupRouterInfoLocally' \
    'CAPABILITY_FLOODFILL' \
    'getPeersByCapability' \
    'isBanlistedForever' \
    'getProfileNonblocking' \
    'getLastSendFailed' \
    'isEstablished' \
    'getBandwidthTier' \
    'getPublished' \
    'SEND_FAILED_RECENT_MS'; do
    if ! grep -q -F "${required}" "${P240_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P240_PROBE_SRC} lacks Plan 240 probe surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 29b. Probe-side mutation, profile creation, NetDB/tunnel/queue/stat
  # writes, reflection, and private-field access are forbidden. Profile
  # creation would let a probe fabricate the readiness it observes.
  for forbidden in \
    '.addProfile(' \
    '.getOrCreateProfile' \
    'heardAbout(' \
    'registerKeys' \
    'unregisterKeys' \
    '.store(' \
    '.publish(' \
    'setKeys' \
    'buildTunnels' \
    'addTunnel' \
    'createRateStat' \
    'createRequiredRateStat' \
    'createFrequencyStat' \
    'addRateData' \
    'getDeclaredField' \
    'setAccessible' \
    'import java.lang.reflect'; do
    if grep -q -F "${forbidden}" "${P240_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P240_PROBE_SRC} uses forbidden Plan 240 probe call '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'explicitPeers' "${P240_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P240_PROBE_SRC} mentions explicitPeers (Plan 240 §12 out of scope)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'vmCommSystem' "${P240_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P240_PROBE_SRC} enables VMComm (Plan 240 §12 out of scope)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'netDb.alwaysQuery' "${P240_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P240_PROBE_SRC} enables netDb.alwaysQuery (Plan 240 §12 forbidden)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P240_LAUNCHER_SRC}" ]]; then
  # 29c. The launcher serves the single read-only P240 command.
  for required in \
    '"P240-ROUTER-B"' \
    'P240Probe' \
    'p240RouterB' \
    'kind=router-b' \
    'observable=true' \
    'b_ri_present=' \
    'b_floodfill_capability_in_ri=' \
    'b_peer_manager_f_capability_indexed=' \
    'b_banlisted_forever=' \
    'b_ri_age_bucket=' \
    'b_bandwidth_tier=' \
    'b_profile_present=' \
    'b_last_send_failed_recent=' \
    'b_comm_established='; do
    if ! grep -q -F "${required}" "${P240_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P240_LAUNCHER_SRC} lacks Plan 240 launcher surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'forceBandwidthClass' "${P240_LAUNCHER_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P240_LAUNCHER_SRC} uses router.forceBandwidthClass (Plan 240 §12 forbids it)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P240_DRIVER_TEST}" ]]; then
  # 29d. The Rust driver owns the Router-B parse/collect, the exact-job
  # log-scan extension, the ordered A/B/C/D classifier, plus additive
  # evidence rows. Correlation (§11) precedes negative cache (§10),
  # candidate selection (§7), pre-query guards (§8), and the retained
  # post-query chain (§9); membership never equals query; stale
  # destination jobs never classify streaming.
  for required in \
    'struct P240RouterB' \
    'fn p240_parse_router_b' \
    'fn p240_collect_router_b' \
    'fn record_p240_router_b' \
    'enum P240Terminal' \
    'struct P240Inputs' \
    'fn p240_classify' \
    'fn record_p240_classification' \
    'P240-ROUTER-B' \
    'p240-router-b-pre' \
    'p240-router-b-post' \
    'p240-target-context' \
    'p240-target-job-trace' \
    'p240-lookup-trace' \
    'p240-totry' \
    'p240-classification' \
    'p240_target_new_isj_with_b_in_totry' \
    'p240_negative_cached' \
    'p240_zero_hop_self' \
    'P240-A-STREAMING-LOOKUP-JOB-NOT-CORRELATED' \
    'P240-B-TARGET-NEGATIVE-CACHED' \
    'P240-B-B-NOT-FLOODFILL-CANDIDATE' \
    'P240-B-B-ELIGIBLE-NOT-IN-INITIAL-SELECTION' \
    'P240-C-B-IP-DIVERSITY-SKIPPED' \
    'P240-C-B-NOT-REACHED-BEFORE-SEARCH-EXHAUSTION' \
    'P240-C-B-OLD-OR-UNSUPPORTED-ROUTER' \
    'P240-C-B-NO-OUTBOUND-LOOKUP-TUNNEL' \
    'P240-C-B-NO-INBOUND-CLIENT-REPLY-TUNNEL' \
    'P240-C-B-NO-COMPATIBLE-REPLY-ENCRYPTION' \
    'P240-C-B-ZERO-HOP-SELF-LOOKUP' \
    'P240-C-B-ZERO-HOP-UNKNOWN-RI' \
    'P240-C-B-ENCRYPTED-LOOKUP-PREP-FAILED' \
    'P240-C-B-QUERY-DISPATCHED' \
    'P240-C-B-PREQUERY-OBSERVABILITY-GAP' \
    'P240-D-B-LOOKUP-NOT-RECEIVED' \
    'P240-D-B-TARGET-LS-NOT-QUERY-ANSWERABLE' \
    'P240-D-B-ANSWER-NOT-EMITTED' \
    'P240-D-A-CLIENT-TUNNEL-DSM-NOT-RECEIVED' \
    'P240-D-A-CLIENT-SUBDB-NOT-INSTALLED' \
    'P240-D-LOOKUP-SUCCEEDED' \
    'JAVA_DIAGNOSTIC_B_PORT' \
    'JAVA_A_LOG_DIR' \
    'JAVA_B_LOG_DIR'; do
    if ! grep -q -F "${required}" "${P240_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P240_DRIVER_TEST} lacks Plan 240 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 29e. Correlation is mandatory: the classifier must gate every stage
  # on the exact streaming job, negative cache must precede candidate
  # selection, and candidate membership must never equal query dispatch.
  if ! grep -q -F 'streaming_job_correlated' "${P240_DRIVER_TEST}"; then
    echo "m6 mixed-router evidence check failed: ${P240_DRIVER_TEST} classifies without exact streaming-job correlation (Plan 240 §11)" >&2
    failures=$((failures + 1))
  fi
  if rg -n "^[[:space:]]*record[[:space:]]+[\"']P240-" "${P240_DRIVER_TEST}" >/dev/null; then
    echo "m6 mixed-router evidence check failed: ${P240_DRIVER_TEST} hard-codes a Plan 240 terminal record" >&2
    failures=$((failures + 1))
  fi
  # Frozen acceptance windows stay frozen on the streaming path.
  for frozen in \
    'const DATAGRAM_WAIT: Duration = Duration::from_secs(45)' \
    'const STREAM_WAIT: Duration = Duration::from_secs(45)' \
    'const SYN_ACK_WAIT: Duration = Duration::from_secs(45)'; do
    if ! grep -q -F "${frozen}" "${P240_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P240_DRIVER_TEST} changed frozen window '${frozen}' (Plan 240 §3 retained)" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -rq -F 'P240' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null ||
     grep -rq -F 'p240' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: production Rust carries Plan 240 surface" >&2
    failures=$((failures + 1))
  fi
  # 29f. The 16 required Plan 240 §15 unit rows.
  for unit_row in \
    p240_requires_exact_streaming_isj_correlation \
    p240_negative_cache_precedes_candidate_selection \
    p240_candidate_membership_does_not_equal_query \
    p240_b_absent_from_totry_requires_candidate_readiness_facts \
    p240_b_eligible_not_selected_is_distinct_from_not_floodfill \
    p240_ip_close_requires_exact_b_job_correlation \
    p240_old_router_guard_precedes_query_dispatch \
    p240_no_client_reply_tunnel_precedes_query_dispatch \
    p240_reply_encryption_guard_precedes_query_dispatch \
    p240_zero_hop_unknown_guard_precedes_query_dispatch \
    p240_query_dispatch_required_before_b_receipt \
    p240_b_receipt_required_before_b_answer \
    p240_b_answer_required_before_a_dsm \
    p240_a_dsm_required_before_client_subdb_install \
    p240_stale_destination_lookup_job_does_not_classify_streaming \
    p240_no_production_change; do
    if ! grep -q "fn ${unit_row}" "${P240_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P240_DRIVER_TEST} lacks Plan 240 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
# 29g. The streaming helper stays frozen: the Router-B observer lives
# in the ControlledRouter diagnostic, never in the client helper.
if grep -q -F 'P240' "${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceStreamingService.java" 2>/dev/null; then
  echo "m6 mixed-router evidence check failed: ReferenceStreamingService.java carries Plan 240 surface (Plan 240 §13: observer lives on Router A)" >&2
  failures=$((failures + 1))
fi
if [[ -f "${P240_SOURCE_LOCK}" ]]; then
  for required in \
    'isNegativeCached(_key)' \
    'Negative cached, not searching: ' \
    'selectFloodfillParticipants(_rkey, _totalSearchLimit + EXTRA_PEERS, ks)' \
    'New ISJ for ' \
    'toTry: ' \
    'Skipping query w/ router too close to others ' \
    'StoreJob.shouldStoreTo(ri)' \
    'not sending query to old router: ' \
    ' failed, no IB client tunnel to receive reply' \
    ' skipped, no ratchet/elg support' \
    'not doing zero-hop self-lookup of ' \
    'not doing zero-hop lookup to unknown ' \
    'ISJ try ' \
    'Encrypted DLM for ' \
    'getPeersByCapability(FloodfillNetworkDatabaseFacade.CAPABILITY_FLOODFILL)' \
    'isBanlistedForever(h)' \
    'Same /16, family, or port: ' \
    'Good: ' \
    'OK: ' \
    'Bad (DB): ' \
    'Bad (no hist): ' \
    'Bad (no prof): '; do
    if ! grep -q -F "${required}" "${P240_SOURCE_LOCK}"; then
      echo "m6 mixed-router evidence check failed: ${P240_SOURCE_LOCK} lacks Plan 240 pinned signal '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P240_HARNESS}" ]]; then
  for required in \
    'P240Probe.java' \
    'JAVA_DIAGNOSTIC_B_PORT' \
    'JAVA_A_LOG_DIR' \
    'p240-router-b-pre' \
    'p240-target-context' \
    'p240-target-job-trace' \
    'p240-lookup-trace' \
    'p240-classification' \
    'external-p240-router-b' \
    'external-p240-target-context' \
    'external-p240-target-job-trace' \
    'external-p240-lookup-trace' \
    'external-p240-classification'; do
    if ! grep -q -F "${required}" "${P240_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P240_HARNESS} lacks Plan 240 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -E "record[[:space:]]+[\"']P240-" "${P240_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P240_HARNESS} invents a Plan 240 terminal" >&2
    failures=$((failures + 1))
  fi
  # No P240 row may carry topology/profile/tunnel-policy changes,
  # timeout overrides, public-I2P/reseed/VMComm/alwaysQuery escapes,
  # or raw log promotion relative to Plan 239.
  p240_harness_lines="$(grep -n -E 'p240|P240' "${P240_HARNESS}" || true)"
  for forbidden in \
    'explicitPeers' \
    'forceBandwidthClass' \
    'netDb.alwaysQuery' \
    'vmCommSystem' \
    'reseed' \
    'floodfillParticipant' \
    'Pool.length' \
    'DRIVER_TIMEOUT=' \
    'DATAGRAM' \
    'log-router'; do
    if printf '%s\n' "${p240_harness_lines}" | grep -q -F "${forbidden}"; then
      echo "m6 mixed-router evidence check failed: ${P240_HARNESS} P240 row carries forbidden surface '${forbidden}' (Plan 240 §12)" >&2
      failures=$((failures + 1))
    fi
  done
fi

# ---- 30. Plan 241 Streaming one-hop client-tunnel fixture corrective ----
# Plan 241 removes the exact Plan-240 controlled-fixture cause (the
# Streaming helper's forced zero-hop client profile) using only
# ordinary public I2CP SessionConfig options mirrored from the proven
# raw-helper contract, then continues the exact lookup chain. The
# shell proves the Plan-230 transit bootstrap (P241-A) and the
# installed one-hop client pair through C (P241-B) before the driver
# runs; the driver re-proves the helper profile, records main-vs-client
# B-RI visibility (§8), and classifies with P241 tokens (§9/§10/§11).
# No production Rust change, no Java source patch, no raw-helper
# change, no topology/profile/publication/timing change, no zero-hop
# fallback, no raw-log promotion.
P241_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P241Probe.java"
P241_LAUNCHER_SRC="${JAVA_LAUNCHER_SRC}"
P241_STREAM_HELPER_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceStreamingService.java"
P241_RAW_HELPER_SRC="${JAVA_RAW_HELPER_SRC}"
P241_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P241_HARNESS="${JAVA_HARNESS}"
P241_SOURCE_LOCK="${REPO_ROOT}/scripts/interop/check-m6-java-response-source-lock.sh"
if [[ ! -f "${P241_PROBE_SRC}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 241 probe ${P241_PROBE_SRC}" >&2
  failures=$((failures + 1))
else
  # 30a. Read-only main-vs-client B-RI visibility surface (exact pinned
  # accessors; local reads only, never a client-DB store).
  for required in \
    'snapshotBri' \
    'lookupRouterInfoLocally' \
    'lookupLocallyWithoutValidation' \
    'clientNetDb' \
    'isClientDb' \
    'instanceof RouterInfo'; do
    if ! grep -q -F "${required}" "${P241_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P241_PROBE_SRC} lacks Plan 241 probe surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 30b. Probe-side mutation, profile creation, NetDB/tunnel/queue/stat
  # writes, RouterInfo stores, reflection, and private-field access are
  # forbidden. A client-DB store would fabricate the visibility split.
  for forbidden in \
    '.addProfile(' \
    '.getOrCreateProfile' \
    'heardAbout(' \
    'registerKeys' \
    'unregisterKeys' \
    '.store(' \
    '.publish(' \
    'setKeys' \
    'buildTunnels' \
    'addTunnel' \
    'createRateStat' \
    'createRequiredRateStat' \
    'createFrequencyStat' \
    'addRateData' \
    'getDeclaredField' \
    'setAccessible' \
    'import java.lang.reflect'; do
    if grep -q -F "${forbidden}" "${P241_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P241_PROBE_SRC} uses forbidden Plan 241 probe call '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'netDb.alwaysQuery' "${P241_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P241_PROBE_SRC} enables netDb.alwaysQuery (Plan 241 §14 forbidden)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'vmCommSystem' "${P241_PROBE_SRC}"; then
    echo "m6 mixed-router evidence check failed: ${P241_PROBE_SRC} enables VMComm (Plan 241 §14 out of scope)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P241_LAUNCHER_SRC}" ]]; then
  # 30c. The launcher serves the single read-only P241 command.
  for required in \
    '"P241-B-RI"' \
    'P241Probe' \
    'p241Bri' \
    'kind=b-ri' \
    'router_a_main_b_ri_raw_present=' \
    'router_a_main_b_ri_valid_present=' \
    'helper_client_db_resolved=' \
    'helper_client_b_ri_raw_present=' \
    'helper_client_b_ri_valid_present='; do
    if ! grep -q -F "${required}" "${P241_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P241_LAUNCHER_SRC} lacks Plan 241 launcher surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P241_STREAM_HELPER_SRC}" ]]; then
  # 30d. The Streaming helper carries the optional explicit-peer
  # one-hop contract mirrored from the raw helper, plus the bounded
  # tunnel-profile report. Lease-set type, encryption type,
  # publication flags, reliability, Destination generation, Streaming
  # behavior, and response scheduling stay untouched.
  for required in \
    'explicitPeerB64OrNull' \
    'inbound.explicitPeers' \
    'outbound.explicitPeers' \
    'REPORT_TUNNEL_PROFILE' \
    'TUNNEL_PROFILE inbound_length=' \
    'inbound.length", "1"' \
    'outbound.length", "1"' \
    'allowZeroHop", "false"' \
    'allowZeroHop", "true"' \
    'i2cp.leaseSetType", "3"' \
    'i2cp.leaseSetEncType", "4"' \
    'i2cp.dontPublishLeaseSet", "false"' \
    'i2cp.messageReliability", "BestEffort"'; do
    if ! grep -q -F "${required}" "${P241_STREAM_HELPER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P241_STREAM_HELPER_SRC} lacks Plan 241 helper surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'P240' "${P241_STREAM_HELPER_SRC}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: ReferenceStreamingService.java carries Plan 240 surface (Plan 241 §13: observer lives on Router A)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P241_RAW_HELPER_SRC}" ]]; then
  # 30e. The raw helper is frozen: it retains its Plan-227 contract
  # and carries no Plan 241 surface.
  for required in \
    'explicitPeerB64OrNull' \
    'inbound.explicitPeers'; do
    if ! grep -q -F "${required}" "${P241_RAW_HELPER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P241_RAW_HELPER_SRC} lost its Plan-227 contract '${required}' (Plan 241 §14: raw helper frozen)" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'P241' "${P241_RAW_HELPER_SRC}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: ReferenceRawDestination.java carries Plan 241 surface (Plan 241 §14: raw helper frozen)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -F 'REPORT_TUNNEL_PROFILE' "${P241_RAW_HELPER_SRC}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: ReferenceRawDestination.java carries the Streaming-only profile report (Plan 241 §14: raw helper frozen)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P241_DRIVER_TEST}" ]]; then
  # 30f. The Rust driver owns the B-RI parse/collect, the helper-profile
  # re-proof, the authoritative-pool re-query, the ordered P241
  # classifier, plus additive evidence rows. Gates (§6/§7) precede the
  # pool check, which precedes the retained lookup ordering; the
  # zero-hop-unknown guard splits on proven pool state; the D chain
  # keeps P225 order with P241 tokens; OCMOSJ resume needs lookup
  # success; no terminal closes M6 or authorizes production change.
  for required in \
    'struct P241Bri' \
    'fn p241_parse_b_ri' \
    'fn p241_collect_b_ri' \
    'fn record_p241_b_ri' \
    'struct P241TunnelProfile' \
    'fn p241_parse_tunnel_profile' \
    'fn p241_tunnel_profile_is_one_hop' \
    'fn p241_tunnel_profile_is_legacy_zero_hop' \
    'fn p241_explicit_peer_matches_router_c' \
    'fn p241_shell_gates_claimed' \
    'enum P241Terminal' \
    'struct P241Inputs' \
    'fn p241_classify' \
    'fn record_p241_classification' \
    'fn p241_ocmosj_resume_allowed' \
    'fn p241_m6_closure_claimable' \
    'fn p241_authorizes_production_change' \
    'P241-B-RI' \
    'REPORT_TUNNEL_PROFILE' \
    'p241-lane-context' \
    'p241-helper-profile' \
    'p241-b-ri-pre' \
    'p241-b-ri-post' \
    'p241-pool-epoch' \
    'p241-b-query-milestone' \
    'p241-classification' \
    'P241_ROUTER_C_HEX' \
    'P241_CLIENT_DBID_HEX' \
    'P241-A-TRANSIT-BOOTSTRAP-NOT-READY' \
    'P241-B-ONE-HOP-CLIENT-TUNNEL-NOT-BUILT direction=' \
    'P241-C-ZERO-HOP-GUARD-CONTRADICTION' \
    'P241-C-ZERO-HOP-STILL-IN-POOL' \
    'P241-C-B-QUERY-DISPATCHED' \
    'P241-D-B-LOOKUP-NOT-RECEIVED' \
    'P241-D-B-TARGET-LS-NOT-QUERY-ANSWERABLE' \
    'P241-D-B-ANSWER-NOT-EMITTED' \
    'P241-D-A-CLIENT-TUNNEL-DSM-NOT-RECEIVED' \
    'P241-D-A-CLIENT-SUBDB-NOT-INSTALLED' \
    'P241-D-LOOKUP-SUCCEEDED'; do
    if ! grep -q -F "${required}" "${P241_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P241_DRIVER_TEST} lacks Plan 241 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  if rg -n "^[[:space:]]*record[[:space:]]+[\"']P241-" "${P241_DRIVER_TEST}" >/dev/null; then
    echo "m6 mixed-router evidence check failed: ${P241_DRIVER_TEST} hard-codes a Plan 241 terminal record" >&2
    failures=$((failures + 1))
  fi
  if grep -rq -F 'P241' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null ||
     grep -rq -F 'p241' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: production Rust carries Plan 241 surface" >&2
    failures=$((failures + 1))
  fi
  # 30g. The 18 required Plan 241 §16 unit rows (17 named + canonical).
  for unit_row in \
    p241_streaming_helper_requires_one_hop_profile \
    p241_streaming_helper_zero_hop_is_forbidden \
    p241_raw_helper_is_unchanged \
    p241_explicit_peer_must_match_router_c \
    p241_bootstrap_gate_precedes_helper_start \
    p241_client_pair_requires_nonzero_both_directions \
    p241_zero_hop_pool_cannot_continue \
    p241_main_ri_and_client_ri_are_distinct_facts \
    p241_client_ri_absence_does_not_fail_nonzero_lookup \
    p241_zero_hop_guard_requires_selected_zero_hop \
    p241_b_query_requires_exact_streaming_job \
    p241_b_receipt_requires_query \
    p241_client_subdb_install_requires_a_dsm \
    p241_ocmosj_resume_requires_lookup_success \
    p241_i2pr_terminal_requires_expected_tunneldata \
    p241_direction_a_pass_does_not_close_m6 \
    p241_no_production_change \
    p241_terminal_tokens_are_canonical; do
    if ! grep -q "fn ${unit_row}" "${P241_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P241_DRIVER_TEST} lacks Plan 241 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P241_SOURCE_LOCK}" ]]; then
  # 30h. The §15 exact-pinned one-hop selection + sendQuery-split signals.
  for required in \
    'public TunnelInfo selectOutboundTunnel(Hash destination, Hash closestTo)' \
    '_clientOutboundPools.get(destination)' \
    'return pool.selectTunnel(closestTo);' \
    'TunnelInfo selectTunnel(Hash closestTo)' \
    'boolean avoidZeroHop = !_settings.getAllowZeroHop()' \
    'new TunnelInfoComparator(closestTo, avoidZeroHop)' \
    'if true, zero-hop tunnels will be put last' \
    'RouterInfo ri = ctx.netDb().lookupRouterInfoLocally(peer);' \
    '_facade.lookupLocallyWithoutValidation(peer)' \
    'outTunnel.getLength() <= 1' \
    'dispatchOutbound(outMsg, outTunnel.getSendTunnelId(0), peer)'; do
    if ! grep -q -F "${required}" "${P241_SOURCE_LOCK}"; then
      echo "m6 mixed-router evidence check failed: ${P241_SOURCE_LOCK} lacks Plan 241 pinned signal '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P241_HARNESS}" ]]; then
  for required in \
    'P241Probe.java' \
    'P241_ROUTER_C_HEX' \
    'P241_CLIENT_DBID_HEX' \
    'external-p241' ; do
    if ! grep -q -F "${required}" "${P241_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P241_HARNESS} lacks Plan 241 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # Plan 242 §31i supersedes the Plan 241 §30i bootstrap/client-pair
  # gate rows: P242-A-* terminals replace P241-A-TRANSIT-BOOTSTRAP-NOT-READY,
  # P242-B-NONZERO- and P242-B-UNEXPECTED-PEER-IN-CLIENT-TUNNEL replace
  # P241-B-ONE-HOP-CLIENT-TUNNEL-NOT-BUILT, and
  # `start_stream_helper "${P242_C_B64}"` replaces
  # `start_stream_helper "${P241_C_B64}"`. The streaming driver env
  # vars (P241_ROUTER_C_HEX / P241_CLIENT_DBID_HEX) are mirrored from
  # the P242-* derived values inside the §31i block, so the Plan 241
  # driver still runs against the corrected pre-helper state.
  if grep -q -E "P241-A-TRANSIT-BOOTSTRAP-NOT-READY|P241-B-ONE-HOP-CLIENT-TUNNEL-NOT-BUILT|start_stream_helper \"\\\$\\{P241_C_B64\\}\"" "${P241_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P241_HARNESS} still emits pre-Plan-242 gate tokens (Plan 242 §5/§7 supersede Plan 241 §6/§7)" >&2
    failures=$((failures + 1))
  fi
  if grep -q -E "record[[:space:]]+[\"']P241-" "${P241_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P241_HARNESS} invents a Plan 241 terminal" >&2
    failures=$((failures + 1))
  fi
  # No P241 row may carry topology/profile/tunnel-policy changes,
  # timeout overrides, public-I2P/reseed/VMComm/alwaysQuery escapes,
  # or raw log promotion relative to Plan 240. Zero-hop is forbidden
  # on the counted lane: the helper profile mismatch exits 72 (harness
  # defect), never a counted terminal, never a fallback.
  p241_harness_lines="$(grep -n -E 'p241|P241' "${P241_HARNESS}" || true)"
  for forbidden in \
    'netDb.alwaysQuery' \
    'vmCommSystem' \
    'forceBandwidthClass' \
    'floodfillParticipant' \
    'reseed' \
    'DRIVER_TIMEOUT=' \
    'log-router'; do
    if printf '%s\n' "${p241_harness_lines}" | grep -q -F "${forbidden}"; then
      echo "m6 mixed-router evidence check failed: ${P241_HARNESS} P241 row carries forbidden surface '${forbidden}' (Plan 241 §14)" >&2
      failures=$((failures + 1))
    fi
  done
  if ! grep -q -F 'exit 72' "${P241_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P241_HARNESS} lacks the Plan 241 helper-profile hard-fail (fixture defect must exit, never a counted terminal)" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 31. Plan 242 stock one-hop selector semantics corrective --------
# Plan 242 source-locks the 1-in-4 explicit-peer selection semantics,
# removes the Plan-241 exact-via-C hard gate from the counted lane, and
# records the corrected installed-client-pool facts. No production Rust
# change, no Java source patch, no raw-helper change, no topology /
# profile / publication / timing change, no zero-hop fallback, no raw-log
# promotion. The streaming helper retains the Plan-241 one-hop profile
# unchanged (corrected fixture stands).
P242_PROBE_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/net/i2p/router/networkdb/kademlia/P227Probe.java"
P242_LAUNCHER_SRC="${JAVA_LAUNCHER_SRC}"
P242_STREAM_HELPER_SRC="${REPO_ROOT}/tests/integration/m6-interop/java/ReferenceStreamingService.java"
P242_RAW_HELPER_SRC="${JAVA_RAW_HELPER_SRC}"
P242_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P242_HARNESS="${JAVA_HARNESS}"
P242_SOURCE_LOCK="${REPO_ROOT}/scripts/interop/check-m6-java-response-source-lock.sh"
if [[ ! -f "${P242_PROBE_SRC}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 242 extended probe ${P242_PROBE_SRC}" >&2
  failures=$((failures + 1))
else
  # 31a. Extended snapshot must record bounded role/path facts only.
  for required in \
    'snapshotClientTunnelsExtended' \
    'Role.UNKNOWN' \
    'inboundFirstRemoteRole' \
    'outboundLastRemoteRole' \
    'inboundContainsB' \
    'outboundContainsB' \
    'inboundContainsC' \
    'outboundContainsC' \
    'inboundExactOneRemoteHop' \
    'outboundExactOneRemoteHop' \
    'inboundUnexpectedPeerObserved' \
    'outboundUnexpectedPeerObserved' \
    'classifyPeerRole'; do
    if ! grep -q -F "${required}" "${P242_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P242_PROBE_SRC} lacks Plan 242 extended snapshot surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 31b. Probe-side mutation, profile creation, NetDB/tunnel/queue/stat
  # writes, RouterInfo stores, reflection, and private-field access are
  # forbidden. The extended snapshot MUST stay read-only.
  for forbidden in \
    '.addProfile(' \
    '.getOrCreateProfile' \
    'heardAbout(' \
    'registerKeys' \
    'unregisterKeys' \
    '.store(' \
    '.publish(' \
    'setKeys' \
    'buildTunnels' \
    'addTunnel' \
    'createRateStat' \
    'createRequiredRateStat' \
    'createFrequencyStat' \
    'addRateData' \
    'getDeclaredField' \
    'setAccessible' \
    'import java.lang.reflect'; do
    if grep -q -F "${forbidden}" "${P242_PROBE_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P242_PROBE_SRC} uses forbidden Plan 242 probe call '${forbidden}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P242_LAUNCHER_SRC}" ]]; then
  # 31c. The launcher serves the new P242-CLIENT-TUNNELS command and
  # exposes the extended role/path fields in the TSV.
  for required in \
    '"P242-CLIENT-TUNNELS"' \
    'p242ClientTunnels' \
    'kind=extended-client-tunnels' \
    'router_a_hex=' \
    'router_b_hex=' \
    'router_c_hex=' \
    'inbound_tunnel_length_including_local=' \
    'outbound_tunnel_length_including_local=' \
    'inbound_remote_hop_count=' \
    'outbound_remote_hop_count=' \
    'inbound_first_remote_role=' \
    'inbound_last_remote_role=' \
    'outbound_first_remote_role=' \
    'outbound_last_remote_role=' \
    'inbound_contains_b=' \
    'outbound_contains_b=' \
    'inbound_contains_c=' \
    'outbound_contains_c=' \
    'inbound_exact_one_remote_hop=' \
    'outbound_exact_one_remote_hop=' \
    'inbound_nonzero_count=' \
    'outbound_nonzero_count=' \
    'inbound_unexpected_peer_observed=' \
    'outbound_unexpected_peer_observed='; do
    if ! grep -q -F "${required}" "${P242_LAUNCHER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P242_LAUNCHER_SRC} lacks Plan 242 launcher surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P242_STREAM_HELPER_SRC}" ]]; then
  # 31d. The streaming helper retains the Plan-241 corrected one-hop
  # profile unchanged; Plan 242 does NOT introduce a new helper
  # SessionConfig contract or a P242 surface in the helper.
  for required in \
    'explicitPeerB64OrNull' \
    'inbound.length", "1"' \
    'outbound.length", "1"' \
    'allowZeroHop", "false"' \
    'i2cp.leaseSetType", "3"' \
    'i2cp.leaseSetEncType", "4"' \
    'i2cp.messageReliability", "BestEffort"' \
    'REPORT_TUNNEL_PROFILE'; do
    if ! grep -q -F "${required}" "${P242_STREAM_HELPER_SRC}"; then
      echo "m6 mixed-router evidence check failed: ${P242_STREAM_HELPER_SRC} lost its Plan 241 contract '${required}' (Plan 242 §3: helper profile unchanged)" >&2
      failures=$((failures + 1))
    fi
  done
  if grep -q -F 'P242' "${P242_STREAM_HELPER_SRC}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: ReferenceStreamingService.java carries Plan 242 surface (Plan 242 §3: helper stays scoped to its own contract)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P242_RAW_HELPER_SRC}" ]]; then
  # 31e. The raw helper stays frozen; it MUST carry no Plan 242 surface.
  if grep -q -F 'P242' "${P242_RAW_HELPER_SRC}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: ReferenceRawDestination.java carries Plan 242 surface (Plan 242 §3: raw helper frozen)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P242_DRIVER_TEST}" ]]; then
  # 31f. The Rust driver carries the bounded Plan-242 parser +
  # bootstrap gate + continuation gate + ordered classifier, plus the
  # additive P242 evidence keys. `contains_c` and
  # `exact_one_remote_hop_via_c` are recorded but never consumed by
  # the gate (corrected continuation). No terminal closes M6 or
  # authorizes production change.
  for required in \
    'struct P242ExtendedTunnels' \
    'fn p242_parse_extended_tunnels' \
    'fn p242_collect_extended_tunnels' \
    'fn record_p242_extended_tunnels' \
    'fn p242_bootstrap_gate' \
    'fn p242_continuation_gate' \
    'enum P242BootstrapOutcome' \
    'enum P242ContinuationOutcome' \
    'enum P242UnexpectedPeerDirection' \
    'fn p242_role_label_is_valid' \
    'enum P242Terminal' \
    'struct P242Inputs' \
    'fn p242_classify' \
    'fn record_p242_classification' \
    'fn p242_ocmosj_resume_allowed' \
    'fn p242_m6_closure_claimable' \
    'fn p242_authorizes_production_change' \
    'P242-CLIENT-TUNNELS' \
    'p242-classification' \
    'P242-A-NO-STOCK-CLIENT-TUNNEL-CANDIDATE' \
    'P242-A-NONZERO-EXPLORATORY-NOT-READY' \
    'P242-A-TOPOLOGY-INVALID' \
    'P242-A-HELPER-PROFILE-NOT-ONE-HOP' \
    'P242-B-NONZERO-CLIENT-TUNNEL-NOT-BUILT direction=' \
    'P242-B-UNEXPECTED-PEER-IN-CLIENT-TUNNEL direction=' \
    'P242-B-ZERO-HOP-CONTRADICTION' \
    'P242-C-LOOKUP-ZERO-HOP-SELECTION-CONTRADICTION' \
    'P242-C-B-QUERY-DISPATCHED' \
    'P242-D-B-LOOKUP-NOT-RECEIVED' \
    'P242-D-B-TARGET-LS-NOT-QUERY-ANSWERABLE' \
    'P242-D-B-ANSWER-NOT-EMITTED' \
    'P242-D-A-CLIENT-TUNNEL-DSM-NOT-RECEIVED' \
    'P242-D-A-CLIENT-SUBDB-NOT-INSTALLED' \
    'P242-D-LOOKUP-SUCCEEDED'; do
    if ! grep -q -F "${required}" "${P242_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P242_DRIVER_TEST} lacks Plan 242 surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # Reject hard-coded terminal-record calls that fabricate P242 classifications.
  if rg -n "^[[:space:]]*record[[:space:]]+[\"']P242-" "${P242_DRIVER_TEST}" >/dev/null; then
    echo "m6 mixed-router evidence check failed: ${P242_DRIVER_TEST} hard-codes a Plan 242 terminal record" >&2
    failures=$((failures + 1))
  fi
  if grep -rq -F 'P242' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null ||
     grep -rq -F 'p242' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: production Rust carries Plan 242 surface" >&2
    failures=$((failures + 1))
  fi
  # 31g. The Plan 242 §12 unit rows: at least 19 named rows + the
  # extended-tunnels parsing row + the terminal-tokens row.
  for unit_row in \
    p242_explicit_peers_is_probabilistic_not_mandatory \
    p242_exact_via_c_is_diagnostic_not_gate \
    p242_nonzero_pair_is_lookup_prerequisite \
    p242_zero_hop_remains_forbidden \
    p242_c_profile_absence_alone_does_not_block_helper \
    p242_no_candidate_population_stops_before_helper \
    p242_unknown_client_tunnel_peer_fails_closed \
    p242_installed_path_records_remote_hop_count \
    p242_lookup_selected_tunnel_must_be_nonzero \
    p242_plan240_exact_job_correlation_retained \
    p242_b_query_requires_exact_dispatch \
    p242_b_receipt_requires_query \
    p242_b_answer_requires_b_receipt \
    p242_client_dsm_requires_b_answer \
    p242_subdb_install_requires_client_dsm \
    p242_ocmosj_resume_requires_lookup_success \
    p242_i2pr_terminal_requires_expected_tunneldata \
    p242_direction_a_pass_does_not_close_m6 \
    p242_no_production_change \
    p242_terminal_tokens_are_canonical \
    p242_extended_tunnels_row_parses_with_bounded_roles; do
    if ! grep -q "fn ${unit_row}" "${P242_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P242_DRIVER_TEST} lacks Plan 242 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P242_SOURCE_LOCK}" ]]; then
  # 31h. Source-lock the exact-pinned probabilistic explicit-peer
  # selection: `shouldSelectExplicit` returns true only when
  # `ctx.random().nextInt(4) == 0`; `ClientPeerSelector.selectPeers`
  # dispatches between the explicit branch and the stock fast-peer
  # branch.
  for required in \
    'protected boolean shouldSelectExplicit(TunnelPoolSettings settings)' \
    'String peers = opts.getProperty(\"explicitPeers\");' \
    'ctx.random().nextInt(4) == 0' \
    'protected List<Hash> selectExplicit(TunnelPoolSettings settings, int length)' \
    'Collections.shuffle(rv, ctx.random());' \
    'rv.add(ctx.routerHash());' \
    '\"No valid explicit peers found, building zero hop\"' \
    'ctx.profileOrganizer().selectFastPeers(more, exclude, matches);' \
    'public List<Hash> selectPeers(TunnelPoolSettings settings)' \
    'if (shouldSelectExplicit(settings))' \
    'return selectExplicit(settings, length);'; do
    if ! grep -q -F "${required}" "${P242_SOURCE_LOCK}"; then
      echo "m6 mixed-router evidence check failed: ${P242_SOURCE_LOCK} lacks Plan 242 selector source lock '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
if [[ -f "${P242_HARNESS}" ]]; then
  # 31i. Harness shell gate correction: §7 non-zero pair, §6 extended
  # diagnostic, §5 stock-candidate-population gate. Exact-via-C must
  # not be a continuation gate (only diagnostic).
  for required in \
    'p242-bootstrap-gate' \
    'p242-client-tunnels' \
    'p242-helper-profile' \
    'p242-classification' \
    'P242-CLIENT-TUNNELS' \
    'P242_A_HEX' \
    'P242_A_SNAPSHOT' \
    'P242_B_HEX' \
    'P242_B_SNAPSHOT' \
    'P242_C_HEX' \
    'P242_C_B64' \
    'P242_GATE_OK' \
    'P242_STOCK_CANDIDATES_NONEMPTY' \
    'P242_PAIR_OK' \
    'P242_IN_NZ_COUNT' \
    'P242_OUT_NZ_COUNT' \
    'P242_IN_CONTAINS_C' \
    'P242_OUT_CONTAINS_C' \
    'P242_IN_EXACT_C' \
    'P242_OUT_EXACT_C' \
    'start_stream_helper "${P242_C_B64}"' \
    'TUNNEL_PROFILE inbound_length=1' \
    'inbound_nonzero_count=' \
    'outbound_nonzero_count=' \
    'P242-A-NO-STOCK-CLIENT-TUNNEL-CANDIDATE' \
    'P242-A-NONZERO-EXPLORATORY-NOT-READY' \
    'P242-B-NONZERO-CLIENT-TUNNEL-NOT-BUILT direction=' \
    'P242-B-UNEXPECTED-PEER-IN-CLIENT-TUNNEL direction=' \
    'P242-B-ZERO-HOP-CONTRADICTION'; do
    if ! grep -q -F "${required}" "${P242_HARNESS}"; then
      echo "m6 mixed-router evidence check failed: ${P242_HARNESS} lacks Plan 242 harness surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # No P242 row may carry topology/profile/tunnel-policy changes,
  # timeout overrides, public-I2P/reseed/VMComm/alwaysQuery escapes, or
  # raw log promotion relative to Plan 241.
  p242_harness_lines="$(grep -n -E 'p242|P242' "${P242_HARNESS}" || true)"
  for forbidden in \
    'netDb.alwaysQuery' \
    'vmCommSystem' \
    'forceBandwidthClass' \
    'floodfillParticipant' \
    'reseed' \
    'DRIVER_TIMEOUT=' \
    'log-router' \
    'random.nextInt(4) == 0' \
    'random().nextInt(4) = 1' \
    'forceShouldSelectExplicit' \
    'explicitPeers=*[, ]'; do
    if printf '%s\n' "${p242_harness_lines}" | grep -q -F "${forbidden}"; then
      echo "m6 mixed-router evidence check failed: ${P242_HARNESS} P242 row carries forbidden surface '${forbidden}' (Plan 242 §4 + §14)" >&2
      failures=$((failures + 1))
    fi
  done
  if ! grep -q -F 'exit 72' "${P242_HARNESS}"; then
    echo "m6 mixed-router evidence check failed: ${P242_HARNESS} lacks the Plan 242 helper-profile hard-fail (fixture defect must exit, never a counted terminal)" >&2
    failures=$((failures + 1))
  fi
fi

# ---- 32. Plan 243 hosted stock-client-build qualification -----------------
# Plan 243 owns the host qualification gate (scripts/interop/check-p243-host-qualified.sh)
# and the three same-SHA counted executions of the frozen Plan-242
# Streaming lane. The checker enforces the bounded reasons, the
# production-surface guard, the Plan-242 harness reuse, and the no-
# retry-until-C / no-explicit-branch / no-production-change invariants.
P243_HOST_SCRIPT="${REPO_ROOT}/scripts/interop/check-p243-host-qualified.sh"
P243_DRIVER_TEST="${REPO_ROOT}/crates/i2pr-daemon/tests/java_tunnel_external.rs"
P243_PLAN="${REPO_ROOT}/plans/implementation/mixed-router-interop/243-m6-java-streaming-hosted-stock-client-build-qualification.md"
P243_CLOSURE="${REPO_ROOT}/plans/closure/mixed-router-interop/243-status.md"
if [[ ! -f "${P243_HOST_SCRIPT}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 243 host qualification script ${P243_HOST_SCRIPT}" >&2
  failures=$((failures + 1))
else
  # 32a. The host qualification script must emit every bounded reason
  # token from Plan 243 §4 and exit 70 on a host failure.
  for required in \
    'P243-H-HOST-NOT-QUALIFIED' \
    'java-runtime-missing' \
    'javac-missing' \
    'java-reference-cache-missing' \
    'i2pr-daemon-missing' \
    'source-lock-input-missing' \
    'port-preflight-failed' \
    'workspace-sha-mismatch' \
    'filesystem-preflight-failed' \
    'exit 70' \
    '9134f808337b401e8e53c73734c81fab04280c9d' \
    'JAVA_VERSION="2.13.0"'; do
    if ! grep -q -F "${required}" "${P243_HOST_SCRIPT}"; then
      echo "m6 mixed-router evidence check failed: ${P243_HOST_SCRIPT} lacks Plan 243 bounded surface '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 32b. The host script does NOT execute the streaming driver (it
  # only qualifies the host). The Plan 242 source-lock script stays
  # the authoritative input source for the source-lock stage.
  if grep -q 'I2PR_M6_JAVA_DRIVER=streaming' "${P243_HOST_SCRIPT}" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: ${P243_HOST_SCRIPT} executes the streaming driver (Plan 243 §4: host qualification only)" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P243_DRIVER_TEST}" ]]; then
  # 32c. The Plan 243 unit rows: at least the 13 §13 named rows.
  for unit_row in \
    p243_host_not_qualified_does_not_consume_attempt \
    p243_host_gate_requires_exact_workspace_sha \
    p243_host_gate_requires_java_reference_cache \
    p243_host_gate_requires_i2pr_daemon \
    p243_counted_attempt_requires_host_qualified \
    p243_counted_attempt_reuses_plan242_nonzero_pair_gate \
    p243_exact_via_c_not_required \
    p243_explicit_branch_not_required \
    p243_three_attempt_budget_no_retry_until_c \
    p243_lookup_continuation_requires_nonzero_pair \
    p243_production_change_requires_expected_tunneldata \
    p243_direction_a_does_not_close_m6 \
    p243_no_production_change; do
    if ! grep -q "fn ${unit_row}" "${P243_DRIVER_TEST}"; then
      echo "m6 mixed-router evidence check failed: ${P243_DRIVER_TEST} lacks Plan 243 unit row '${unit_row}'" >&2
      failures=$((failures + 1))
    fi
  done
  # 32d. Production Rust stays free of P243 surface (mirrors Plan 242
  # §31f). The checker scans the same production source roots.
  if grep -rq -F 'P243-HOST' "${REPO_ROOT}/crates/i2pr-daemon/src" "${REPO_ROOT}/crates/i2pr-client/src" "${REPO_ROOT}/crates/i2pr-tunnel/src" "${REPO_ROOT}/crates/i2pr-runtime/src" 2>/dev/null; then
    echo "m6 mixed-router evidence check failed: production Rust carries Plan 243 host-qualification surface" >&2
    failures=$((failures + 1))
  fi
fi
if [[ -f "${P243_PLAN}" ]]; then
  # 32e. The implementation plan keeps the §5/§6/§9/§10/§13 invariants
  # in its own text (defense in depth against silent drift).
  for required in \
    'three counted attempts' \
    'no between-attempt tuning' \
    'use one exact implementation SHA for the counted budget' \
    'fresh disposable A/B/C RouterContexts for every attempt' \
    'do not retry merely to obtain the one-in-four explicit-C branch' \
    'No production i2pr corrective is authorized before exact expected TunnelData' \
    'P243-G-DIRECTION-A-ESTABLISHED does not close M6'; do
    if ! grep -q -F "${required}" "${P243_PLAN}"; then
      echo "m6 mixed-router evidence check failed: ${P243_PLAN} lost Plan 243 invariant '${required}'" >&2
      failures=$((failures + 1))
    fi
  done
fi
# The Plan 243 closure record is required for the registry/roadmap
# unblock audit; its presence is asserted by the closure record
# existence below (it is written on the Plan 243 implementation head).
if [[ ! -f "${P243_CLOSURE}" ]]; then
  echo "m6 mixed-router evidence check failed: missing Plan 243 closure record ${P243_CLOSURE}" >&2
  failures=$((failures + 1))
fi

if [[ "${failures}" -ne 0 ]]; then
  echo "m6 mixed-router evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi
echo "m6 mixed-router evidence check passed (${#GUARDED[@]} guarded labels, two-family pins verified, Plan 197 §8 pq parser tolerance invariants, Plan 201 Branch C/D three-router topology, Plan 220 §14 corrected-diagnostic invariants, Plan 222 §15 exact-selector/tracked-send invariants, Plan 223 §16 identity/LS2 separation invariants, Plan 224 §17 NO_LEASESET lookup-path attribution invariants, Plan 225 §18 effective logger activation corrective invariants, Plan 226 §19 loopback peer-diversity corrective invariants, Plan 227 §20 explicit one-hop client-tunnel corrective invariants, Plan 228 §21 build-path attribution invariants, Plan 229 §22 non-zero exploratory paired-tunnel bootstrap corrective invariants, Plan 230 §23 reachability-capability/profile-bootstrap corrective invariants, Plan 231 §24 reverse-delivery tunnel-dispatch attribution invariants, Plan 232 §25 route-derived lease-gateway fixture corrective invariants, Plan 237 §26 stock-response observability corrective invariants, Plan 238 §27 Router-A admission observer invariants, Plan 239 §28 Router-A pre-dispatch OCMOSJ attribution invariants, Plan 240 §29 streaming target-LeaseSet lookup-failure attribution invariants, Plan 241 §30 streaming one-hop client-tunnel fixture corrective invariants, Plan 242 §31 stock one-hop selector semantics corrective invariants, Plan 243 §32 hosted stock-client-build qualification invariants)"
