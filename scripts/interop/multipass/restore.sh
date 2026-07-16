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
