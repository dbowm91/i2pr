#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

mode=""
baseline="$instance_state_dir/host-baseline.json"
baseline_arg=0
while (($#)); do
  case "$1" in
    --record-baseline) [[ -z "$mode" ]] || die "duplicate host-state mode"; mode=record ;;
    --verify) [[ -z "$mode" ]] || die "duplicate host-state mode"; mode=verify ;;
    --baseline)
      [[ "$baseline_arg" == 0 && $# -ge 2 ]] || die "duplicate or incomplete --baseline"
      baseline=$2
      validate_path "$baseline" baseline
      baseline_arg=1
      shift
      ;;
    --help|-h) printf 'usage: verify-clean-host.sh --record-baseline|--verify [--baseline <path>]\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
[[ -n "$mode" ]] || die "one host-state mode is required"
require_multipass
ensure_dirs

instance_state=$(multipass list --format csv 2>/dev/null | python3 -c 'import csv,json,sys; print(json.dumps(sorted(({"name":r[0],"state":r[1]} for r in csv.reader(sys.stdin) if len(r)>=2 and r[0].lower()!="name"), key=lambda x:x["name"]), separators=(",", ":")))')
route_sha256=$(ip route show table all 2>/dev/null | sha256sum | awk '{print $1}')
link_sha256=$(ip -o link show 2>/dev/null | sha256sum | awk '{print $1}')
firewall_sha256="unreadable"
if command -v nft >/dev/null 2>&1; then
  firewall_sha256=$(nft list ruleset 2>/dev/null | sha256sum | awk '{print $1}')
fi
userns_clone=$(cat /proc/sys/kernel/unprivileged_userns_clone 2>/dev/null || printf 'unreadable')
apparmor_restrict=$(cat /proc/sys/kernel/apparmor_restrict_unprivileged_userns 2>/dev/null || printf 'unreadable')
apparmor_enabled=$(cat /sys/module/apparmor/parameters/enabled 2>/dev/null || printf 'unreadable')
evidence_state=$(find "$host_evidence_root" -mindepth 1 -maxdepth 1 -type d -printf '%f\n' 2>/dev/null | sort | python3 -c 'import json,sys; print(json.dumps([v.strip() for v in sys.stdin if v.strip()], separators=(",", ":")))')
state=$(python3 -c 'import json,sys; print(json.dumps({"schema":1,"instances":json.loads(sys.argv[1]),"route_sha256":sys.argv[2],"link_sha256":sys.argv[3],"firewall_sha256":sys.argv[4],"host_userns_clone":sys.argv[5],"host_apparmor_restrict_unprivileged_userns":sys.argv[6],"host_apparmor_enabled":sys.argv[7],"evidence_directories":json.loads(sys.argv[8])}, sort_keys=True, separators=(",", ":")))' "$instance_state" "$route_sha256" "$link_sha256" "$firewall_sha256" "$userns_clone" "$apparmor_restrict" "$apparmor_enabled" "$evidence_state")
actual="$instance_state_dir/host-state.json"
write_json "$actual" "$state"
if [[ "$mode" == record ]]; then
  write_json "$baseline" "$state"
  printf '%s\n' "$state"
  exit 0
fi
[[ -f "$baseline" ]] || { typed_blocker blocked_host_baseline_missing; exit 2; }
if ! python3 "$script_dir/host_state.py" --baseline "$baseline" --actual "$actual" --canonical "$instance_name"; then
  typed_blocker blocked_host_state_changed
  exit 2
fi
printf '%s\n' "$state"
