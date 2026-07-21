#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

while (($#)); do
  case "$1" in
    --help|-h) printf 'usage: prepare-offline.sh\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
require_owned_instance
require_command python3
ensure_dirs
acquire_lifecycle_lock
require_owned_instance

if ! guest_exec python3 "$guest_repo_root/scripts/interop/multipass/source_tree.py" \
    --root "$guest_repo_root" --commit "$(python3 - "$instance_state_dir/source-transfer.json" <<'PY'
import json
import sys
print(json.load(open(sys.argv[1], encoding="utf-8"))["source_commit"])
PY
)" --archive-sha256 "$(python3 - "$instance_state_dir/source-transfer.json" <<'PY'
import json
import sys
print(json.load(open(sys.argv[1], encoding="utf-8"))["source_archive_sha256"])
PY
)" --verify --archive-listing "$guest_repo_root/.source-listing.txt" >/dev/null 2>&1; then
  typed_blocker blocked_source_tree_hash_mismatch
  exit 2
fi
if ! guest_exec python3 "$guest_repo_root/scripts/interop/cache-manifest.py" --verify >/dev/null 2>&1; then
  typed_blocker blocked_reference_cache_offline_reuse_failed
  exit 2
fi

guest_root_exec nft delete table inet i2pr_interop_offline >/dev/null 2>&1 || true
guest_root_exec nft add table inet i2pr_interop_offline
guest_root_exec nft add chain inet i2pr_interop_offline output '{ type filter hook output priority -100; policy accept; }'
guest_root_exec nft add rule inet i2pr_interop_offline output oifname lo accept
guest_root_exec nft add rule inet i2pr_interop_offline output ip daddr 127.0.0.0/8 accept
guest_root_exec nft add rule inet i2pr_interop_offline output ip6 daddr ::1 accept
if ! guest_root_exec nft list table inet i2pr_interop_offline >/dev/null 2>&1; then
  typed_blocker blocked_offline_enforcement_unavailable
  exit 2
fi
if ! guest_exec test -x "$guest_repo_root/target/debug/i2pr-interop" >/dev/null 2>&1; then
  typed_blocker blocked_reference_cache_offline_reuse_failed
  exit 2
fi

guest_root_exec install -d -o root -g root -m 0700 /var/lib/i2pr-interop
guest_root_exec install -d -o "$guest_execution_user" -g "$guest_execution_user" -m 0700 "$guest_evidence_root"
guest_root_exec chown -R "$guest_execution_user:$guest_execution_user" "$guest_repo_root/target"
offline_file=/tmp/i2pr-offline-enforcement.json
multipass transfer "$script_dir/offline-enforcement.json" "$instance_name:$offline_file" >/dev/null
guest_root_exec install -o root -g root -m 0600 "$offline_file" /var/lib/i2pr-interop/offline-enforcement.json
guest_root_exec rm -f "$offline_file"
receipt=$(python3 - "$environment_manifest_sha256" <<'PY'
import datetime as dt
import json
import sys
print(json.dumps({
    "schema": 1,
    "type": "multipass-offline-transition",
    "offline_enforcement": "namespace-only",
    "guest_nft_role": "marker",
    "execution_network": "forbidden-inside-namespace",
    "environment_manifest_sha256": sys.argv[1],
    "completed_at": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}, sort_keys=True, separators=(",", ":")))
PY
)
write_json "$instance_state_dir/offline-transition.json" "$receipt"
python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state offline_ready \
  --operation offline-enforcement --outcome namespace-only-marker >/dev/null
printf '%s\n' "$receipt"
