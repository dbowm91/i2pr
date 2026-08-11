#!/usr/bin/env bash
# Plan 099 NTCP2 interoperability static boundary check.
#
# This script enforces the durable invariants the Plan 099 active
# development interop surface relies on. It does **not** enforce
# plan-token presence, plan-history status markers, or any other
# plan-specific execution vocabulary. Historical plan documents
# remain in the repository as audit records but are not part of
# this checker's contract.
#
# The active checker enforces:
#
# 1. NTCP2 remains experimental and non-advertised.
# 2. The production daemon never accidentally activates NTCP2.
# 3. The pinned direct i2pd driver and helper artifacts remain
#    test-only and are never linked from production crates.
# 4. The development smoke and reference driver wrapper forbid
#    public-network/reseed/SAM/I2CP/HTTP fallback paths.
# 5. The pinned i2pd reference revision is recorded.
# 6. The bounded functional interop tests exist.
#
# Exit 0 means the durable invariants hold; nonzero means a
# blocking condition.

set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

# 1. NTCP2 remains experimental and non-advertised.
support="$root/specs/support.toml"
test -f "$support"
for entry in \
  'status = "experimental"' \
  'advertised = false'; do
    if ! grep -Fq "$entry" "$support"; then
        echo "NTCP2 support marker missing: $entry" >&2
        exit 1
    fi
done

# 2. Production daemon must not accidentally activate NTCP2.
daemon_lib="$root/crates/i2pr-daemon/src/lib.rs"
daemon_config="$root/crates/i2pr-daemon/src/config.rs"
test -f "$daemon_lib"
test -f "$daemon_config"
# The NTCP2 default must be false.
if ! grep -q 'default_ntcp2_enabled' "$daemon_config" || ! grep -A1 'fn default_ntcp2_enabled' "$daemon_config" | grep -q 'false'; then
    echo "daemon NTCP2 default is not false" >&2
    exit 2
fi
# The composition graph must not register ntcp2-transport.
# The registration pattern is ServiceName::new("ntcp2-transport").
if grep -q 'ServiceName::new("ntcp2-transport")' "$daemon_lib"; then
    echo "daemon registers ntcp2-transport in the service graph" >&2
    exit 2
fi
# The behavioral composition regression test must exist.
if ! grep -q 'daemon_graph_contains_no_ntcp2_transport_service' "$daemon_lib"; then
    echo "daemon composition regression test missing" >&2
    exit 2
fi

# 3. Direct i2pd reference driver and helpers remain test-only.
helper_cmake="$root/tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt"
helper_source="$root/tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp"
test -f "$helper_cmake"
test -f "$helper_source"
if grep -qE 'crate-type.*=.*\[\s*"rlib"' "$helper_cmake" 2>/dev/null; then
    echo "i2pd direct driver must not be a Cargo rlib" >&2
    exit 3
fi
if grep -rlE 'reference-drivers/i2pd' "$root/crates" 2>/dev/null; then
    echo "production crate depends on the test-only i2pd driver" >&2
    exit 4
fi

# 4. The wrapper forbids public-network/reseed/SAM/I2CP/HTTP
# fallback paths. Check actual code lines (skip module docstrings)
# so descriptive prose does not produce false positives.
wrapper="$root/scripts/interop/run-minimal-i2pd-host-loopback-probe.py"
test -f "$wrapper"
for forbidden in reseed sam-trigger http-trigger i2cp support-topology i2p-network; do
    if grep -E "^[^#]*${forbidden}\\b" "$wrapper" >/dev/null 2>&1; then
        echo "wrapper contains forbidden profile marker: $forbidden" >&2
        exit 5
    fi
done
# Wrapper must forbid release/support profile flags.
if ! grep -q 'FORBIDDEN_PROFILE_FLAGS' "$wrapper"; then
    echo "wrapper is missing the FORBIDDEN_PROFILE_FLAGS guard" >&2
    exit 6
fi

# 5. The pinned i2pd reference revision is recorded.
for path in \
  "$root/tests/integration/ntcp2/references.lock.toml" \
  "$root/tests/integration/ntcp2/manifest.toml"; do
    test -f "$path"
    if ! grep -Fq 'f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e' "$path"; then
        echo "pinned i2pd revision missing from $path" >&2
        exit 7
    fi
done

# 6. Bounded functional interop tests exist.
for required in \
  tests/integration/ntcp2/harness/test_minimal_i2pd_probe.py \
  tests/integration/ntcp2/harness/test_i2pd_direct_driver.py \
  tests/integration/ntcp2/harness/test_i2pd_direct_control.py \
  tests/integration/ntcp2/harness/test_execution_lane.py \
  tests/integration/ntcp2/harness/plan083_runner.py \
  tests/integration/ntcp2/harness/plan084_runner.py \
  tests/integration/ntcp2/harness/preflight_runner.py \
  tests/integration/ntcp2/harness/i2pd_direct_driver.py \
  tests/integration/ntcp2/harness/minimal_i2pd_probe.py \
  tests/integration/ntcp2/harness/interop_topology.py; do
    if ! test -f "$root/$required"; then
        echo "required interop module missing: $required" >&2
        exit 8
    fi
done

# Production daemon never calls the i2pd driver.
if grep -rlE 'i2pd_ntcp2_interop_driver' "$root/crates" 2>/dev/null; then
    echo "production crate references the test-only i2pd driver binary" >&2
    exit 9
fi

# Specs surface must still declare NTCP2 experimental.
if ! grep -E 'NTCP2.*[Ee]xperimental|[Nn]tcp2.*experimental' "$root/docs/protocol-support.md" >/dev/null 2>&1; then
    echo "protocol-support doc must mark NTCP2 as experimental" >&2
    exit 10
fi

echo "Plan 099 NTCP2 interoperability static check: OK"