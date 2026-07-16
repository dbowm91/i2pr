#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

while (($#)); do
  case "$1" in
    --help|-h) printf 'usage: run-matrix.sh\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
require_instance
require_command python3
ensure_dirs
if ! guest_root_exec nft list table inet i2pr_interop_offline >/dev/null 2>&1; then
  typed_blocker blocked_execution_not_offline
  exit 2
fi

status_json=$(bash "$script_dir/status.sh" --json)
python3 - "$status_json" <<'PY'
import json
import sys
value = json.loads(sys.argv[1])
if value.get("execution_user_privileged") is not False or value.get("source_manifest") is None or not value.get("cache_verified"):
    raise SystemExit("guest inputs are not ready")
PY
source_commit=$(python3 - "$status_json" <<'PY'
import json
import sys
print(json.loads(sys.argv[1])["source_manifest"]["commit"])
PY
)
source_tree_sha256=$(python3 - "$status_json" <<'PY'
import json
import sys
print(json.loads(sys.argv[1])["source_manifest"]["tree_sha256"])
PY
)
cache_manifest_sha256=$(guest_exec sha256sum "$guest_repo_root/target/interop/build/reference-cache-manifest.json" | awk '{print $1}')
cloud_init_sha256=$(guest_exec sha256sum "$guest_repo_root/scripts/interop/multipass/cloud-init.yaml" | awk '{print $1}')
provisioning_sha256=$(guest_root_exec sha256sum /var/lib/i2pr-interop/provisioning.json | awk '{print $1}')
probe_outcome=$(guest_exec cat "$guest_evidence_root/environment-probe/probe.json" | python3 -c 'import json,sys; print(json.load(sys.stdin)["outcome"])')
[[ "$probe_outcome" == rootless_sandbox_available ]] || { typed_blocker blocked_rootless_probe; exit 2; }

environment_local="$instance_state_dir/environment.json"
python3 "$script_dir/records.py" environment --output "$environment_local" \
  --source-commit "$source_commit" --source-tree-sha256 "$source_tree_sha256" \
  --cache-manifest-sha256 "$cache_manifest_sha256" --cloud-init-sha256 "$cloud_init_sha256" \
  --provisioning-sha256 "$provisioning_sha256" --environment-manifest-sha256 "$environment_manifest_sha256"
environment_guest="$guest_evidence_root/environment.json"
multipass transfer "$environment_local" "$instance_name:/tmp/i2pr-environment.json" >/dev/null
guest_root_exec install -o "$guest_execution_user" -g "$guest_execution_user" -m 0600 /tmp/i2pr-environment.json "$environment_guest"
guest_root_exec rm -f /tmp/i2pr-environment.json

for scenario in i2pr-to-java-ipv4 java-to-i2pr-ipv4 i2pr-to-i2pd-ipv4 i2pd-to-i2pr-ipv4; do
  if ! bash "$script_dir/run-direction.sh" --scenario "$scenario"; then
    typed_blocker blocked_direction_failed "$scenario"
    exit 2
  fi
done

guest_exec python3 "$guest_repo_root/scripts/interop/multipass/aggregate.py" \
  --evidence "$guest_evidence_root" --environment "$environment_guest" \
  --probe "$guest_evidence_root/environment-probe/probe.json" >/dev/null
lifecycle_local="$instance_state_dir/lifecycle.json"
python3 "$script_dir/records.py" lifecycle --output "$lifecycle_local" \
  --environment-manifest-sha256 "$environment_manifest_sha256"
multipass transfer "$lifecycle_local" "$instance_name:/tmp/i2pr-lifecycle.json" >/dev/null
guest_root_exec install -o "$guest_execution_user" -g "$guest_execution_user" -m 0600 /tmp/i2pr-lifecycle.json "$guest_evidence_root/lifecycle.json"
guest_root_exec rm -f /tmp/i2pr-lifecycle.json
guest_exec python3 "$guest_repo_root/scripts/interop/multipass/sidecars.py" --evidence "$guest_evidence_root"
guest_admin_exec cat "$guest_evidence_root/aggregate.json"
