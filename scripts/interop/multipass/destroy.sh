#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

discard=0
purge=0
while (($#)); do
  case "$1" in
    --discard-unexported) [[ "$discard" == 0 ]] || die "duplicate --discard-unexported"; discard=1 ;;
    --purge) [[ "$purge" == 0 ]] || die "duplicate --purge"; purge=1 ;;
    --help|-h) printf 'usage: destroy.sh [--discard-unexported] [--purge]\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
require_multipass
ensure_dirs
if instance_exists; then
  if [[ "$discard" != 1 ]] && ! find "$host_evidence_root" -mindepth 1 -maxdepth 1 -type d -print -quit | grep -q .; then
    typed_blocker blocked_unexported_evidence
    exit 2
  fi
  multipass stop "$instance_name" >/dev/null 2>&1 || true
  multipass delete "$instance_name" >/dev/null || {
    typed_blocker blocked_destroy_failed
    exit 2
  }
  if [[ "$purge" == 1 ]]; then
    multipass purge >/dev/null || { typed_blocker blocked_destroy_failed; exit 2; }
  fi
fi
if instance_exists; then
  typed_blocker blocked_destroy_failed "canonical instance still present"
  exit 2
fi
receipt=$(python3 - "$discard" "$purge" <<'PY'
import datetime as dt
import json
import sys
print(json.dumps({
    "schema": 1,
    "type": "multipass-destruction",
    "instance_name": "i2pr-interop-rootless",
    "discard_unexported": sys.argv[1] == "1",
    "purge_requested": sys.argv[2] == "1",
    "instance_present_after": False,
    "completed_at": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}, sort_keys=True, separators=(",", ":")))
PY
)
write_json "$instance_state_dir/destruction.json" "$receipt"
printf '%s\n' "$receipt"
