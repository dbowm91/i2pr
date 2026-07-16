#!/usr/bin/env bash
# Static boundary checks for the rootless sealed-namespace interoperability lane.
#
# The rootless evidence lane must remain free of host-global namespace mutation,
# passwordless sudo escalation, and automatic fallback to the privileged topology.
# These checks fail closed whenever rootless-owned files contain prohibited
# patterns or omit required rootless contracts.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)

fail() {
  echo "rootless interop boundary violation: $1" >&2
  exit 1
}

require_string() {
  local file="$1"
  local needle="$2"
  local label="$3"
  if [[ ! -f "$file" ]]; then
    fail "$label: file not found ($file)"
  fi
  if ! grep -Fq "$needle" "$file"; then
    fail "$label: missing required entry ($needle)"
  fi
}

# --- Files owned by the rootless lane ---
rootless_python=(
  "$root/tests/integration/ntcp2/harness/rootless_supervisor.py"
  "$root/tests/integration/ntcp2/harness/rootless_topology.py"
  "$root/tests/integration/ntcp2/harness/rootless_inner_runner.py"
  "$root/tests/integration/ntcp2/harness/interop_topology.py"
)

rootless_shell=(
  "$root/scripts/interop/rootless-enter.sh"
  "$root/scripts/interop/probe-rootless-sandbox.sh"
)

rootless_workflow=(
  "$root/.github/workflows/ntcp2-interop-rootless.yml"
)

rootless_adrs=(
  "$root/docs/adr/0017-rootless-sealed-namespace-interop-evidence.md"
)

# All rootless-owned files must exist so the lane cannot be silently absent.
for file in "${rootless_python[@]}" "${rootless_shell[@]}" "${rootless_workflow[@]}" "${rootless_adrs[@]}"; do
  if [[ ! -f "$file" ]]; then
    fail "rootless-owned file missing: $file"
  fi
done

# --- Prohibited patterns inside rootless-owned files ---
forbidden_patterns=(
  "sudo[ '\"]"                  # any sudo invocation (word or quoted)
  "ip[ ]+netns[ ]+add"          # host-global named network namespace creation
  "ip[ ]+netns[ ]+exec"
  "ip[ ]+link[ ]+add[ ]+type[ ]+veth"
  "ip[ ]+link[ ]+del"
  "/run/netns"
  "setcap"
  "getcap[ ]+-r[ ]+"            # getcap-based authorization
  "getcap[ ]+"                  # bare getcap authorization
  "--privileged"
  "--network[ ]+host"
  "/var/run/docker.sock"
  "CAP_NET_ADMIN"
  "nft[ ]+-f"                   # namespace-local nftables not part of rootless lane
  "nftables"
  "unshare[ ]+--net"            # net-only unshare requires privilege
  "iptables"                    # rootless lane relies on structural isolation
)

for file in "${rootless_python[@]}" "${rootless_shell[@]}" "${rootless_workflow[@]}"; do
  for pattern in "${forbidden_patterns[@]}"; do
    if grep -En "$pattern" "$file" >/dev/null 2>&1; then
      fail "$file contains prohibited pattern: $pattern"
    fi
  done
done

# --- Disallow arbitrary shell execution paths inside the rootless Python lane ---
for file in "${rootless_python[@]}"; do
  if grep -En "\beval\b|\bexec\(\s*['\"]" "$file" >/dev/null 2>&1; then
    fail "$file uses eval or string exec to execute commands"
  fi
done

# --- Disallow automatic privileged fallback from the rootless lane ---
for file in "${rootless_shell[@]}" "${rootless_workflow[@]}"; do
  if grep -En "privileged[-_]dual[-_]netns" "$file" >/dev/null 2>&1; then
    fail "$file references automatic fallback to privileged-dual-netns"
  fi
  if grep -En "fall[- ]?back" "$file" >/dev/null 2>&1; then
    fail "$file mentions fallback (any kind) - rootless must fail closed"
  fi
done

# --- Static contracts required for the rootless lane ---
require_string \
  "$root/tests/integration/ntcp2/harness/interop_topology.py" \
  "rootless-sealed-single-netns" \
  "interop_topology.py"

require_string \
  "$root/tests/integration/ntcp2/harness/interop_topology.py" \
  "privileged-dual-netns-veth" \
  "interop_topology.py"

require_string \
  "$root/tests/integration/ntcp2/harness/interop_topology.py" \
  "ProcessPlacement" \
  "interop_topology.py"

require_string \
  "$root/scripts/interop/probe-rootless-sandbox.sh" \
  "rootless_sandbox_available" \
  "probe-rootless-sandbox.sh"

require_string \
  "$root/scripts/interop/probe-rootless-sandbox.sh" \
  "blocked_unprivileged_user_namespace" \
  "probe-rootless-sandbox.sh"

require_string \
  "$root/.github/workflows/ntcp2-interop-rootless.yml" \
  "permissions:" \
  "rootless workflow"

require_string \
  "$root/.github/workflows/ntcp2-interop-rootless.yml" \
  "contents: read" \
  "rootless workflow"

if grep -En '^\s*sudo' "$root/.github/workflows/ntcp2-interop-rootless.yml" >/dev/null 2>&1; then
  fail "rootless workflow must not invoke sudo"
fi

# --- Reusable shell scripts must be executable ---
for file in "${rootless_shell[@]}"; do
  if [[ ! -x "$file" ]]; then
    fail "$file must be executable (chmod +x)"
  fi
done

# --- Topology identifier matches the gate catalog entry ---
gate_catalog="$root/tests/integration/ntcp2/harness/build_gate.py"
if [[ -f "$gate_catalog" ]]; then
  if ! grep -Fq "handshake-smoke-rootless" "$gate_catalog"; then
    fail "build_gate.py must declare handshake-smoke-rootless gate"
  fi
  if ! grep -Fq "rootless-sealed-single-netns" "$gate_catalog"; then
    fail "build_gate.py must declare rootless-sealed-single-netns topology"
  fi
  if ! grep -Fq "unprivileged-userns" "$gate_catalog"; then
    fail "build_gate.py must declare unprivileged-userns privilege model"
  fi
fi

# --- Evidence validators must require the sandbox attestation ---
validator="$root/tests/integration/ntcp2/harness/evidence.py"
if [[ -f "$validator" ]]; then
  if ! grep -Fq "sandbox_attestation_sha256" "$validator"; then
    fail "evidence.py must require sandbox_attestation_sha256"
  fi
  if ! grep -Fq "parent_network_state_unchanged" "$validator"; then
    fail "evidence.py must require parent_network_state_unchanged"
  fi
fi

# --- Privileged backend isolation: privileged code must remain in reviewed files only ---
privileged_files=(
  "$root/tests/integration/ntcp2/harness/topology.py"
  "$root/tests/integration/ntcp2/harness/reference_topology.py"
)
for file in "${privileged_files[@]}"; do
  if [[ ! -f "$file" ]]; then
    fail "privileged backend file missing: $file"
  fi
  if ! grep -Fq "privileged-dual-netns-veth" "$file"; then
    fail "$file must declare privileged-dual-netns-veth topology identifier"
  fi
done

echo "rootless interop boundary checks passed"
