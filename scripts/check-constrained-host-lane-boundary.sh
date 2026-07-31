#!/usr/bin/env bash
# Plan 077 static boundary check.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
fail() { echo "Plan 077 constrained-host lane boundary violation: $1" >&2; exit 1; }

probe="$root/scripts/interop/probe-constrained-host-lanes.sh"
module="$root/tests/integration/ntcp2/harness/execution_lane.py"
tests="$root/tests/integration/ntcp2/harness/test_execution_lane.py"

for path in "$probe" "$module" "$tests" "$root/plans/077-status.md"; do
  [[ -f "$path" ]] || fail "missing required artifact: $path"
done
[[ -x "$probe" ]] || fail "probe must be executable: $probe"

for marker in \
  'docker_daemon_accessible' \
  'qemu_tcg_usable' \
  'seccomp_no_new_privs_supported' \
  'remote_workflow_present' \
  'selected_lane' \
  'full_runtime_lane_unavailable' \
  'inherited-descriptors-seccomp' \
  'docker-network-none' \
  'qemu-tcg-no-nic' \
  'i2pr-ntcp2-execution-manifest-v1' \
  'i2pr-ntcp2-execution-lane-qualification-v1'; do
  grep -Fq -- "$marker" "$module" || fail "missing module marker: $marker"
done

for forbidden in 'sudo' 'ip netns' '--privileged' '--network host' '/var/run/docker.sock'; do
  if grep -Fq -- "$forbidden" "$probe"; then
    fail "probe contains forbidden execution authority: $forbidden"
  fi
done

for marker in LaneSelectionTests ManifestTests QualificationTests ProbeTests; do
  grep -Fq -- "$marker" "$tests" || fail "missing test class: $marker"
done

echo "Plan 077 constrained-host lane boundary checks passed"
