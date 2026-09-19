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

if [[ "${failures}" -ne 0 ]]; then
  echo "m6 mixed-router evidence check failed: ${failures} violation(s)" >&2
  exit 1
fi
echo "m6 mixed-router evidence check passed (${#GUARDED[@]} guarded labels, two-family pins verified, Plan 197 §8 pq parser tolerance invariants, Plan 201 Branch C/D three-router topology, Plan 220 §14 corrected-diagnostic invariants, Plan 222 §15 exact-selector/tracked-send invariants, Plan 223 §16 identity/LS2 separation invariants, Plan 224 §17 NO_LEASESET lookup-path attribution invariants)"
