#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

run_id=""
while (($#)); do
  case "$1" in
    --run-id)
      [[ -z "$run_id" && $# -ge 2 ]] || die "duplicate or incomplete --run-id"
      run_id=$2
      shift
      ;;
    --help|-h) printf 'usage: export-evidence.sh --run-id <safe-id>\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
[[ -n "$run_id" ]] || die "--run-id is required"
validate_run_id "$run_id"
require_instance
require_command python3
ensure_dirs

source_dir="$instance_state_dir/export-$run_id"
rm -rf "$source_dir"
install -d -m 0700 "$source_dir"
guest_transfer_root="/tmp/i2pr-export-$run_id"
guest_root_exec rm -rf "$guest_transfer_root"
guest_root_exec install -d -o "$guest_admin_user" -g "$guest_admin_user" -m 0700 "$guest_transfer_root"
cleanup_guest_transfer() {
  guest_root_exec rm -rf "$guest_transfer_root" >/dev/null 2>&1 || true
}
trap cleanup_guest_transfer EXIT
for name in environment.json environment.json.sha256 probe.json probe.json.sha256 \
    i2pr-to-java-ipv4.json java-to-i2pr-ipv4.json i2pr-to-i2pd-ipv4.json \
    i2pd-to-i2pr-ipv4.json aggregate.json manifest.json lifecycle.json; do
  guest_root_exec install -o "$guest_admin_user" -g "$guest_admin_user" -m 0600 \
    "$guest_evidence_root/$name" "$guest_transfer_root/$name" || {
    rm -rf "$source_dir"
    typed_blocker blocked_evidence_transfer_failed "$name"
    exit 2
  }
  multipass transfer "$instance_name:$guest_transfer_root/$name" "$source_dir/$name" >/dev/null || {
    rm -rf "$source_dir"
    typed_blocker blocked_evidence_transfer_failed "$name"
    exit 2
  }
done
destination="$host_evidence_root/$run_id"
python3 "$script_dir/export.py" --source "$source_dir" --destination "$destination"
printf '%s\n' "$destination"
