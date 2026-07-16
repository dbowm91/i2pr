#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

scenario=""
while (($#)); do
  case "$1" in
    --scenario)
      [[ -z "$scenario" && $# -ge 2 ]] || die "duplicate or incomplete --scenario"
      scenario=$2
      shift
      ;;
    --help|-h) printf 'usage: run-direction.sh --scenario <direction>\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
[[ -n "$scenario" ]] || die "--scenario is required"
reference=$(scenario_reference "$scenario")
require_instance
require_command python3

status_json=$(bash "$script_dir/status.sh" --json)
if ! python3 - "$status_json" <<'PY'
import json
import sys
value = json.loads(sys.argv[1])
if value.get("execution_user_privileged") is not False:
    raise SystemExit("execution user is privileged")
if value.get("source_manifest") is None:
    raise SystemExit("source manifest is missing")
if not value.get("cache_verified"):
    raise SystemExit("reference cache is not verified")
if value.get("sysctls", {}).get("kernel.apparmor_restrict_unprivileged_userns") != "0":
    raise SystemExit("guest AppArmor userns policy is not permissive")
PY
then
  typed_blocker blocked_guest_policy_mismatch
  exit 2
fi
if ! guest_root_exec nft list table inet i2pr_interop_offline >/dev/null 2>&1; then
  typed_blocker blocked_execution_not_offline
  exit 2
fi

bash "$script_dir/probe.sh" >/dev/null
attestation_path="$guest_evidence_root/$scenario-attestation.json"
record_path="$guest_evidence_root/$scenario.json"
guest_exec python3 "$guest_repo_root/scripts/interop/multipass/collect.py" \
  --root "$guest_repo_root" --clear-scenario "$scenario"
guest_exec rm -f "$attestation_path" "$record_path"
runner_status=0
runner_output=$(guest_exec bash "$guest_repo_root/scripts/interop/rootless-enter.sh" \
  --scenario "$scenario" --reference "$reference" --build-cache "$guest_cache_root" \
  --run-root "$guest_repo_root/target/interop/runs" --attestation-output "$attestation_path") || runner_status=$?
if ! guest_exec test -s "$attestation_path" >/dev/null 2>&1; then
  typed_blocker blocked_isolation_attestation_missing
  exit 2
fi
guest_exec python3 "$guest_repo_root/scripts/interop/multipass/collect.py" \
  --root "$guest_repo_root" --attestation "$attestation_path" >/dev/null
collect_status=0
guest_exec python3 "$guest_repo_root/scripts/interop/multipass/collect.py" \
  --root "$guest_repo_root" --scenario "$scenario" --output "$record_path" || collect_status=$?
if [[ "$collect_status" != 0 ]]; then
  typed_blocker blocked_direction_record_missing
  exit 2
fi
if ! guest_exec find "$guest_repo_root/target/interop/runs" -mindepth 1 -print -quit 2>/dev/null | grep -q .; then
  cleanup_result=clean
else
  typed_blocker blocked_direction_cleanup
  exit 2
fi

record=$(guest_exec cat "$record_path")
receipt=$(python3 - "$scenario" "$reference" "$runner_status" "$record" "$environment_manifest_sha256" <<'PY'
import datetime as dt
import hashlib
import json
import sys
record = json.loads(sys.argv[4])
print(json.dumps({
    "schema": 1,
    "type": "multipass-direction-result",
    "scenario_id": sys.argv[1],
    "reference": sys.argv[2],
    "runner_exit": int(sys.argv[3]),
    "record_result": record.get("actual_typed_result"),
    "record_sha256": hashlib.sha256(sys.argv[4].encode()).hexdigest(),
    "environment_manifest_sha256": sys.argv[5],
    "cleanup_result": "clean",
    "completed_at": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}, sort_keys=True, separators=(",", ":")))
PY
)
write_json "$instance_state_dir/$scenario.json" "$receipt"
printf '%s\n' "$receipt"
if [[ "$runner_status" != 0 ]]; then
  exit 2
fi
