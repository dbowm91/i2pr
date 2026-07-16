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
    --help|-h) printf 'usage: snapshot.sh --name <provisioned|source-and-cache-ready>\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
case "$snapshot_name" in provisioned|source-and-cache-ready) ;; *) die "unknown snapshot name" ;; esac
require_instance
ensure_dirs
acquire_lifecycle_lock
bash "$script_dir/create.sh" --run-id "$run_id" --instance-name "$instance_name" --generation "$instance_generation" --adopt-owned >/dev/null
status_json=$(bash "$script_dir/status.sh" --json)
if ! python3 - "$snapshot_name" "$status_json" <<'PY'
import json
import sys

name, raw = sys.argv[1:]
value = json.loads(raw)
if value.get("instance_state") != "Running":
    raise SystemExit("instance is not in a known running state")
if value.get("execution_user_privileged") is not False:
    raise SystemExit("execution user is privileged")
if name == "provisioned" and (value.get("source_manifest") is not None or value.get("cache_verified")):
    raise SystemExit("provisioned snapshot requested after source/cache transfer")
if name == "source-and-cache-ready" and (value.get("source_manifest") is None or not value.get("cache_verified")):
    raise SystemExit("source-and-cache-ready snapshot lacks verified inputs")
PY
then
  typed_blocker blocked_snapshot_state_mismatch
  exit 2
fi
if guest_exec find "$guest_repo_root/target/interop/runs" -mindepth 1 -print -quit 2>/dev/null | grep -q .; then
  typed_blocker blocked_snapshot_secret_state
  exit 2
fi
commit=$(python3 - "$instance_lifecycle_path" <<'PY'
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
comment="plan-049;environment-id=$environment_id;generation=$instance_generation;environment-manifest=$environment_manifest_sha256;cloud-init=$cloud_init_sha256;snapshot=$snapshot_name;source-commit=$commit;cache-manifest=$cache_manifest"
was_running=0
restart_needed=0
restart_if_needed() {
  if [[ "$restart_needed" == 1 ]]; then
    multipass start "$instance_name" >/dev/null 2>&1 || true
  fi
}
trap restart_if_needed EXIT
if multipass info "$instance_name" --format json | grep -Fq '"state": "Running"'; then
  was_running=1
  multipass stop "$instance_name" >/dev/null || { typed_blocker blocked_snapshot_failed "instance could not be stopped"; exit 2; }
  restart_needed=1
fi
multipass snapshot "$instance_name" --name "$snapshot_name" --comment "$comment" >/dev/null || {
  typed_blocker blocked_snapshot_failed
  exit 2
}
if [[ "$was_running" == 1 ]]; then
  multipass start "$instance_name" >/dev/null || { typed_blocker blocked_snapshot_failed "instance could not be restarted"; exit 2; }
  restart_needed=0
fi
if ! multipass info "$instance_name" --format json | grep -Fq "$snapshot_name"; then
  typed_blocker blocked_snapshot_not_recorded
  exit 2
fi
receipt=$(python3 - "$snapshot_name" "$comment" <<'PY'
import datetime as dt
import json
import os
import sys
print(json.dumps({
    "schema": 1,
    "type": "multipass-snapshot",
    "snapshot_name": sys.argv[1],
    "environment_id": os.environ.get("I2PR_MULTIPASS_ENVIRONMENT_ID", ""),
    "instance_generation": int(os.environ.get("I2PR_MULTIPASS_GENERATION", "1")),
    "environment_manifest_sha256": os.environ.get("I2PR_MULTIPASS_ENVIRONMENT_MANIFEST_SHA256", ""),
    "comment": sys.argv[2],
    "created_at": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}, sort_keys=True, separators=(",", ":")))
PY
)
write_json "$instance_state_dir/snapshot-$snapshot_name.json" "$receipt"
printf '%s\n' "$receipt"
