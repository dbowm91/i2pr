#!/usr/bin/env bash
# Plan 050: attempt a selective purge of an owned deleted instance.
#
# Most Multipass versions do not expose a per-instance purge; this command
# detects that, refuses to issue a global purge, and reports the typed
# outcome ``selective_purge_supported``, ``selective_purge_not_supported``,
# ``resource_already_absent``, or ``ownership_not_proven``.
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

requested_run_id=""
requested_instance_name=""
purge_owned=0
output_path=""
while (($#)); do
  case "$1" in
    --run-id)
      [[ -z "$requested_run_id" && $# -ge 2 ]] || die "duplicate or incomplete --run-id"
      requested_run_id=$2
      shift
      ;;
    --instance-name)
      [[ -z "$requested_instance_name" && $# -ge 2 ]] || die "duplicate or incomplete --instance-name"
      requested_instance_name=$2
      shift
      ;;
    --purge-owned)
      [[ "$purge_owned" == 0 ]] || die "duplicate --purge-owned"
      purge_owned=1
      ;;
    --output)
      [[ -z "$output_path" && $# -ge 2 ]] || die "duplicate or incomplete --output"
      output_path=$2
      shift
      ;;
    --help|-h) printf 'usage: selective-purge.sh --run-id <safe-id> --instance-name <safe-name> [--purge-owned] [--output <path>]\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done

[[ -n "$requested_run_id" ]] || die "--run-id is required"
[[ -n "$requested_instance_name" ]] || die "--instance-name is required"
validate_run_id "$requested_run_id"
python3 "$lifecycle_py" validate-instance-name "$requested_instance_name" >/dev/null

configure_context "$requested_run_id" "$requested_instance_name" 1
require_multipass
ensure_dirs
acquire_lifecycle_lock

emit() {
  local outcome=$1
  python3 - "$run_id" "$instance_name" "$instance_generation" "$environment_manifest_sha256" "$cloud_init_sha256" "$outcome" "$output_path" <<'PY'
import datetime as dt, json, os, sys
from pathlib import Path
run_id, instance_name, generation, manifest, cloud_init, outcome, output_path = sys.argv[1:8]
value = {
    "schema_version": 1,
    "type": "multipass-selective-purge",
    "run_id": run_id,
    "instance_name": instance_name,
    "instance_generation": int(generation),
    "environment_manifest_sha256": manifest,
    "cloud_init_sha256": cloud_init,
    "outcome": outcome,
    "completed_at_utc": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}
if output_path:
    Path(output_path).parent.mkdir(parents=True, exist_ok=True)
    Path(output_path).write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    os.chmod(output_path, 0o600)
print(json.dumps(value, sort_keys=True, separators=(",", ":")))
PY
}

[[ -f "$instance_lifecycle_path" ]] || { emit ownership_not_proven; exit 2; }
ownership_record_sha256=$(python3 -c 'import json,sys; print(json.load(sys.stdin).get("environment_manifest_sha256",""))' <"$instance_lifecycle_path")
[[ "$ownership_record_sha256" == "$environment_manifest_sha256" ]] || { emit ownership_not_proven; exit 2; }

list_json=$(multipass list --format json 2>/dev/null || true)
state_value=$(python3 - "$list_json" "$instance_name" <<'PY'
import json, sys
from pathlib import Path
sys.path.insert(0, str(Path.cwd() / "scripts/interop/multipass"))
from lifecycle import parse_multipass_list
try:
    for entry in parse_multipass_list(sys.argv[1]):
        if entry["name"] == sys.argv[2]:
            print(entry["state"])
            break
except Exception:
    pass
PY
)
if [[ -z "$state_value" ]]; then
  emit resource_already_absent
  exit 0
fi
if [[ "$state_value" != "deleted-unpurged" ]]; then
  echo "instance is not in deleted-unpurged state: $state_value" >&2
  emit ownership_not_proven
  exit 2
fi

help_output=$(multipass help 2>&1 || true)
if grep -Eq '\bpurge\b' <<<"$help_output" && multipass help purge 2>/dev/null | grep -Eq 'instance|deleted'; then
  if [[ "$purge_owned" != 1 ]]; then
    emit selective_purge_supported
    exit 0
  fi
  if ! multipass purge "$instance_name" >/dev/null 2>&1; then
    emit selective_purge_not_supported
    exit 2
  fi
  emit selective_purge_supported
  exit 0
fi
emit selective_purge_not_supported
exit 2
