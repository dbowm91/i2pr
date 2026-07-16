#!/usr/bin/env bash
# Static fail-closed checks for the Plan 049 Multipass lifecycle boundary.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
dir="$root/scripts/interop/multipass"

fail() {
  echo "Multipass lifecycle boundary violation: $1" >&2
  exit 1
}

required=(common.sh create.sh destroy.sh run-evidence-lane.sh status.sh snapshot.sh restore.sh lifecycle.py records.py aggregate.py export.py)
for name in "${required[@]}"; do
  [[ -f "$dir/$name" ]] || fail "missing lifecycle file: $name"
done

for name in common.sh create.sh destroy.sh run-evidence-lane.sh status.sh snapshot.sh restore.sh; do
  rg -n '^set -euo pipefail$' "$dir/$name" >/dev/null || fail "$name is not strict"
done

for name in common.sh create.sh destroy.sh run-evidence-lane.sh snapshot.sh restore.sh; do
  rg -n 'multipass[[:space:]]+purge|multipass[[:space:]]+delete[[:space:]]+--all' "$dir/$name" >/dev/null && fail "$name can globally purge or delete"
done

rg -n 'flock|acquire_lifecycle_lock|instance_lock' "$dir/common.sh" >/dev/null || fail "lifecycle lock is missing"
rg -n 'atomic|write_json_atomic|os\.replace' "$dir/lifecycle.py" >/dev/null || fail "atomic lifecycle write is missing"
rg -n 'schema_version|LIFECYCLE_STATES|transition' "$dir/lifecycle.py" >/dev/null || fail "validated lifecycle schema is missing"
rg -n 'owner_token_sha256|ownership_proof' "$dir/create.sh" "$dir/destroy.sh" "$dir/lifecycle.py" >/dev/null || fail "ownership proof is missing"
for name in probe.sh transfer-source.sh transfer-cache.sh prepare-offline.sh run-matrix.sh run-direction.sh export-evidence.sh; do
  rg -n 'require_owned_instance' "$dir/$name" >/dev/null || fail "$name lacks an ownership gate"
  rg -n 'acquire_lifecycle_lock' "$dir/$name" >/dev/null || fail "$name lacks a lifecycle lock"
done
rg -n -- '--adopt-owned' "$dir/create.sh" "$dir/run-evidence-lane.sh" >/dev/null || fail "explicit adoption is missing"
rg -n -- '--recreate-owned' "$dir/destroy.sh" "$dir/run-evidence-lane.sh" >/dev/null || fail "explicit recreation is missing"
rg -n -- '--resume-owned' "$dir/run-evidence-lane.sh" >/dev/null || fail "explicit resume is missing"
rg -n 'blocked_deleted_instance_requires_purge|blocked_instance_without_host_state|blocked_ownership_token_mismatch' "$dir" >/dev/null || fail "precise collision outcomes are missing"
rg -n 'environment_evidence_sha256|instance_generation|ownership_record_sha256' "$dir/records.py" "$dir/aggregate.py" "$dir/export.py" >/dev/null || fail "evidence attribution is missing"
rg -n 'environment-id=.*snapshot=|blocked_snapshot_contract_mismatch' "$dir/snapshot.sh" "$dir/restore.sh" >/dev/null || fail "snapshot contract binding is missing"
rg -n 'host_baseline_probe_outcome|guest_rootless_probe_outcome' "$dir/run-evidence-lane.sh" "$dir/records.py" >/dev/null || fail "host/guest probe separation is missing"

# The legacy fixed identifier remains only as a manifest value for read-only
# inspection compatibility; it may not be the authoritative default context.
if rg -n 'i2pr-interop-rootless' "$dir/common.sh" "$dir/create.sh" "$dir/run-evidence-lane.sh" "$dir/status.sh"; then
  fail "legacy fixed name is authoritative in lifecycle code"
fi

echo "Multipass interop boundary checks passed"
