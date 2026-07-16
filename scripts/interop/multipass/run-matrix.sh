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
require_owned_instance
require_command python3
ensure_dirs
acquire_lifecycle_lock
require_owned_instance
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

python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state running \
  --operation matrix-start --outcome guest-probe-verified >/dev/null
instance_name_digest=$(python3 - "$instance_name" <<'PY'
import sys
from pathlib import Path
sys.path.insert(0, str(Path.cwd() / "scripts/interop/multipass"))
from lifecycle import instance_name_digest
print(instance_name_digest(sys.argv[1]))
PY
)
ownership_record_sha256=$(guest_root_exec sha256sum /var/lib/i2pr-interop/environment.json | awk '{print $1}')
adoption_mode=$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("adoption_mode","fresh"))' <"$instance_lifecycle_path")
host_baseline_probe_outcome=${host_baseline_probe_outcome:-not-run}
guest_rootless_probe_outcome=rootless_sandbox_available
export host_baseline_probe_outcome guest_rootless_probe_outcome

environment_local="$instance_state_dir/environment.json"
python3 "$script_dir/records.py" environment --output "$environment_local" \
  --environment-id "$environment_id" --run-id "$run_id" --instance-generation "$instance_generation" \
  --instance-name-digest "$instance_name_digest" --lifecycle-schema-version 1 \
  --ownership-record-sha256 "$ownership_record_sha256" \
  --source-commit "$source_commit" --source-tree-sha256 "$source_tree_sha256" \
  --cache-manifest-sha256 "$cache_manifest_sha256" --cloud-init-sha256 "$cloud_init_sha256" \
  --provisioning-sha256 "$provisioning_sha256" --environment-manifest-sha256 "$environment_manifest_sha256" \
  --host-baseline-probe-outcome "$host_baseline_probe_outcome" \
  --guest-rootless-probe-outcome "$guest_rootless_probe_outcome" --adoption-mode "$adoption_mode"
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
lifecycle_local="$instance_lifecycle_path"
python3 "$lifecycle_py" update --state-file "$lifecycle_local" --state exporting \
  --operation matrix-complete --outcome matrix-passed >/dev/null
sanitized_lifecycle=$(mktemp)
python3 - "$lifecycle_local" "$sanitized_lifecycle" "$ownership_record_sha256" <<'PY'
import hashlib
import json
import sys
from pathlib import Path
value = json.loads(Path(sys.argv[1]).read_text(encoding="utf-8"))
safe = {key: value.get(key) for key in (
    "schema_version", "environment_id", "run_id", "instance_generation", "state",
    "source_commit", "reference_cache_manifest_sha256", "environment_manifest_sha256",
    "cloud_init_sha256", "adoption_mode", "last_operation", "last_typed_outcome",
)}
safe["instance_name_digest"] = hashlib.sha256(value["instance_name"].encode()).hexdigest()
safe["ownership_record_sha256"] = sys.argv[3]
Path(sys.argv[2]).write_text(json.dumps(safe, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
PY
multipass transfer "$sanitized_lifecycle" "$instance_name:/tmp/i2pr-lifecycle.json" >/dev/null
rm -f "$sanitized_lifecycle"
guest_root_exec install -o "$guest_execution_user" -g "$guest_execution_user" -m 0600 /tmp/i2pr-lifecycle.json "$guest_evidence_root/lifecycle.json"
guest_root_exec rm -f /tmp/i2pr-lifecycle.json
guest_exec python3 "$guest_repo_root/scripts/interop/multipass/sidecars.py" --evidence "$guest_evidence_root"
guest_admin_exec cat "$guest_evidence_root/aggregate.json"
