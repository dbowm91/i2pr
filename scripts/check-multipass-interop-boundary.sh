#!/usr/bin/env bash
# Static fail-closed checks for the Plan 048/049/050/051 Multipass lifecycle boundary.
set -euo pipefail

root=$(cd "$(dirname "${BASH_SOURCE[0]}")/.." && pwd)
dir="$root/scripts/interop/multipass"

fail() {
  echo "Multipass lifecycle boundary violation: $1" >&2
  exit 1
}

required=(common.sh create.sh destroy.sh run-evidence-lane.sh status.sh snapshot.sh restore.sh lifecycle.py records.py aggregate.py export.py cloud_init_status.py)
for name in "${required[@]}"; do
  [[ -f "$dir/$name" ]] || fail "missing lifecycle file: $name"
done

for name in common.sh create.sh destroy.sh run-evidence-lane.sh status.sh snapshot.sh restore.sh cloud-init-status.sh verify-base.sh selective-purge.sh dispatch-gate.sh; do
  rg -n '^set -euo pipefail$' "$dir/$name" >/dev/null || fail "$name is not strict"
done

for name in common.sh create.sh destroy.sh run-evidence-lane.sh snapshot.sh restore.sh; do
  rg -n 'multipass[[:space:]]+purge|multipass[[:space:]]+delete[[:space:]]+--all' "$dir/$name" >/dev/null && fail "$name can globally purge or delete"
done

rg -n 'flock|acquire_lifecycle_lock|instance_lock' "$dir/common.sh" >/dev/null || fail "lifecycle lock is missing"
rg -n 'atomic|write_json_atomic|os\.replace' "$dir/lifecycle.py" >/dev/null || fail "atomic lifecycle write is missing"
rg -n 'schema_version|LIFECYCLE_STATES|transition' "$dir/lifecycle.py" >/dev/null || fail "validated lifecycle schema is missing"
rg -n 'owner_token_sha256|ownership_proof' "$dir/create.sh" "$dir/destroy.sh" "$dir/lifecycle.py" >/dev/null || fail "ownership proof is missing"
for name in probe.sh transfer-source.sh transfer-cache.sh prepare-offline.sh run-matrix.sh run-direction.sh export-evidence.sh dispatch-gate.sh; do
  rg -n 'require_owned_instance|acquire_lifecycle_lock' "$dir/$name" >/dev/null || fail "$name lacks an ownership gate or lifecycle lock"
done
rg -n -- '--adopt-owned' "$dir/create.sh" "$dir/run-evidence-lane.sh" >/dev/null || fail "explicit adoption is missing"
rg -n -- '--recreate-owned' "$dir/destroy.sh" "$dir/run-evidence-lane.sh" >/dev/null || fail "explicit recreation is missing"
rg -n -- '--resume-owned' "$dir/run-evidence-lane.sh" >/dev/null || fail "explicit resume is missing"
rg -n 'blocked_deleted_instance_requires_purge|blocked_instance_without_host_state|blocked_ownership_token_mismatch' "$dir" >/dev/null || fail "precise collision outcomes are missing"
rg -n 'environment_evidence_sha256|instance_generation|ownership_record_sha256' "$dir/records.py" "$dir/aggregate.py" "$dir/export.py" >/dev/null || fail "evidence attribution is missing"
rg -n 'environment-id=.*snapshot=|blocked_snapshot_contract_mismatch' "$dir/snapshot.sh" "$dir/restore.sh" >/dev/null || fail "snapshot contract binding is missing"
rg -n 'host_baseline_probe_outcome|guest_rootless_probe_outcome' "$dir/run-evidence-lane.sh" "$dir/records.py" >/dev/null || fail "host/guest probe separation is missing"

# Plan 050: precise cloud-init classification, minimal cloud-init, guest-probe-only,
# selective purge, and post-verify base-environment contract.
rg -n 'cloud_init_state|cloud_init_stage|failure_class|recommended_action' "$dir/cloud_init_status.py" >/dev/null || fail "sanitized cloud-init status parser is missing"
rg -n 'cloud-init-status\.sh|verify-base\.sh' "$dir/create.sh" "$dir/run-evidence-lane.sh" >/dev/null || fail "plan 050 provisioning gates are missing"
rg -n 'guest-probe-only' "$dir/run-evidence-lane.sh" >/dev/null || fail "guest-probe-only operation is missing"
rg -n 'selective-purge\.sh|selective_purge_supported|selective_purge_not_supported' "$dir/selective-purge.sh" >/dev/null || fail "selective-purge remediation is missing"
rg -n 'rustup toolchain install' "$dir/cloud-init.yaml" >/dev/null && fail "cloud-init still contains long-running toolchain work"
rg -n 'i2pr-multipass-verify-base|base-packages\.complete' "$dir/cloud-init.yaml" >/dev/null || fail "phase markers are missing from cloud-init"
rg -n 'eval[[:space:]]' "$dir/selective-purge.sh" "$dir/cloud-init-status.sh" "$dir/verify-base.sh" >/dev/null && fail "plan 050 files must not use eval"
rg -n 'cloud_init_sha256|environment_manifest_sha256' "$dir/cloud_init_status.py" "$dir/verify-base.sh" >/dev/null || fail "sanitized records must bind environment and cloud-init digests"
rg -n 'multipass purge[[:space:]]*$|multipass purge[[:space:]]+--' "$dir/selective-purge.sh" "$dir/create.sh" "$dir/destroy.sh" "$dir/run-evidence-lane.sh" "$dir/dispatch-gate.sh" >/dev/null && fail "selective-purge must never use global multipass purge"

# The legacy fixed identifier remains only as a manifest value for read-only
# inspection compatibility; it may not be the authoritative default context.
if rg -n 'i2pr-interop-rootless' "$dir/common.sh" "$dir/create.sh" "$dir/run-evidence-lane.sh" "$dir/status.sh"; then
  fail "legacy fixed name is authoritative in lifecycle code"
fi

echo "Multipass interop boundary checks passed"
