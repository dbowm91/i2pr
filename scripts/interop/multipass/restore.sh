#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

snapshot_name=""
while (($#)); do
  case "$1" in
    --name)
      [[ -z "$snapshot_name" && $# -ge 2 ]] || die "duplicate or incomplete --name"
      snapshot_name=$2
      shift
      ;;
    --help|-h) printf 'usage: restore.sh --name <provisioned|source-and-cache-ready>\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
case "$snapshot_name" in provisioned|source-and-cache-ready) ;; *) die "unknown snapshot name" ;; esac
require_instance
ensure_dirs
acquire_lifecycle_lock
bash "$script_dir/create.sh" --run-id "$run_id" --instance-name "$instance_name" --generation "$instance_generation" --adopt-owned >/dev/null
snapshot_info=$(multipass info "$instance_name" --format json 2>/dev/null || true)
source_commit=$(python3 - "$instance_lifecycle_path" <<'PY'
import json
import sys
print(json.load(open(sys.argv[1], encoding="utf-8")).get("source_commit", "unknown"))
PY
)
cache_manifest=$(python3 - "$instance_lifecycle_path" <<'PY'
import json
import sys
print(json.load(open(sys.argv[1], encoding="utf-8")).get("reference_cache_manifest_sha256", "pending"))
PY
)
if ! python3 - "$snapshot_info" "$instance_name" "$snapshot_name" "$environment_id" "$instance_generation" "$environment_manifest_sha256" "$cloud_init_sha256" "$source_commit" "$cache_manifest" <<'PY'
import json
import sys
info, instance_name, snapshot_name, environment_id, generation, manifest, cloud_init, source_commit, cache_manifest = sys.argv[1:]
try:
    value = json.loads(info)
    entry = value.get("info", {}).get(instance_name, {})
    snapshots = entry.get("snapshots", [])
    target = next(item for item in snapshots if isinstance(item, dict) and item.get("name") == snapshot_name)
    comment = target.get("comment", "")
    required = (
        f"environment-id={environment_id}", f"generation={generation}",
        f"environment-manifest={manifest}", f"cloud-init={cloud_init}",
        f"snapshot={snapshot_name}", f"source-commit={source_commit}",
        f"cache-manifest={cache_manifest}",
    )
    if not isinstance(comment, str) or any(token not in comment for token in required):
        raise ValueError
except (ValueError, KeyError, TypeError, json.JSONDecodeError):
    raise SystemExit(2)
PY
then
  typed_blocker blocked_snapshot_contract_mismatch
  exit 2
fi
if multipass info "$instance_name" --format json | grep -Fq '"state": "Running"'; then
  multipass stop "$instance_name" >/dev/null || { typed_blocker blocked_snapshot_restore_failed; exit 2; }
fi
multipass restore "$instance_name.$snapshot_name" --destructive >/dev/null || {
  typed_blocker blocked_snapshot_restore_failed
  exit 2
}
multipass start "$instance_name" >/dev/null || { typed_blocker blocked_snapshot_restore_failed; exit 2; }
bash "$script_dir/status.sh" --json >/dev/null || { typed_blocker blocked_guest_policy_mismatch; exit 2; }
if [[ "$snapshot_name" == source-and-cache-ready ]]; then
  status_json=$(bash "$script_dir/status.sh" --json)
  if ! python3 - "$status_json" <<'PY'
import json
import sys
value = json.loads(sys.argv[1])
if value.get("source_manifest") is None or not value.get("cache_verified"):
    raise SystemExit("restored source/cache state differs")
PY
  then
    typed_blocker blocked_snapshot_content_mismatch
    exit 2
  fi
fi
bash "$script_dir/probe.sh" >/dev/null
printf '%s\n' "restored $snapshot_name"
