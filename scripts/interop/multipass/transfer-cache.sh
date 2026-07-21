#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

while (($#)); do
  case "$1" in
    --help|-h) printf 'usage: transfer-cache.sh\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
require_owned_instance
require_command python3
require_command gzip
require_command tar
ensure_dirs
acquire_lifecycle_lock
require_owned_instance

host_cache="$repo_root/target/interop/cache"
host_build="$repo_root/target/interop/build"
if ! python3 "$repo_root/scripts/interop/cache-manifest.py" --verify >/dev/null 2>&1; then
  typed_blocker blocked_reference_cache_manifest_invalid
  exit 2
fi
for path in "$host_cache/current-cache.json" "$host_build/reference-cache-manifest.json" "$host_build/reference-build-summary.json"; do
  [[ -f "$path" ]] || { typed_blocker blocked_reference_cache_missing; exit 2; }
done
if find "$host_cache" "$host_build/reference-cache-manifest.json" "$host_build/reference-build-summary.json" -type l -print -quit | grep -q .; then
  typed_blocker blocked_reference_cache_hash_mismatch "symbolic links are not transferable"
  exit 2
fi

archive="$instance_state_dir/cache/reference-cache.tar.gz"
mkdir -p "$(dirname "$archive")"
(
  cd "$host_target"
  tar --format=ustar --sort=name --mtime='UTC 1970-01-01' --owner=0 --group=0 --numeric-owner \
    -cf - cache build/reference-cache-manifest.json build/reference-build-summary.json
) | gzip -n >"$archive"
archive_sha256=$(sha256_file "$archive")
cache_manifest_sha256=$(sha256_file "$host_build/reference-cache-manifest.json")
summary_sha256=$(sha256_file "$host_build/reference-build-summary.json")

guest_archive="/tmp/i2pr-reference-cache.tar.gz"
multipass transfer "$archive" "$instance_name:$guest_archive" >/dev/null
guest_root_exec install -d -o "$guest_execution_user" -g "$guest_execution_user" -m 0700 "$guest_repo_root/target/interop"
guest_root_exec rm -rf "$guest_repo_root/target/interop/cache" \
  "$guest_repo_root/target/interop/build/reference-cache-manifest.json" \
  "$guest_repo_root/target/interop/build/reference-build-summary.json"
guest_root_exec install -d -o "$guest_execution_user" -g "$guest_execution_user" -m 0700 "$guest_repo_root/target/interop/build"
guest_exec tar -xzf "$guest_archive" -C "$guest_repo_root/target/interop"
guest_root_exec rm -f "$guest_archive"
guest_root_exec chown -R "$guest_execution_user:$guest_execution_user" "$guest_repo_root/target"
if ! guest_exec python3 "$guest_repo_root/scripts/interop/cache-manifest.py" --verify >/dev/null 2>&1; then
  typed_blocker blocked_reference_cache_offline_reuse_failed
  exit 2
fi

receipt=$(python3 - "$archive_sha256" "$cache_manifest_sha256" "$summary_sha256" "$environment_manifest_sha256" <<'PY'
import datetime as dt
import json
import sys
print(json.dumps({
    "schema": 1,
    "type": "multipass-reference-cache-import",
    "cache_root": "target/interop/cache",
    "archive_sha256": sys.argv[1],
    "cache_manifest_sha256": sys.argv[2],
    "reference_build_summary_sha256": sys.argv[3],
    "environment_manifest_sha256": sys.argv[4],
    "offline_reuse": "verified-cache-only",
    "completed_at": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}, sort_keys=True, separators=(",", ":")))
PY
)
write_json "$instance_state_dir/cache-import.json" "$receipt"
current_state=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["state"])' <"$instance_lifecycle_path")
if [[ "$current_state" == source_ready ]]; then
  next_state=source_and_cache_ready
elif [[ "$current_state" == provisioned ]]; then
  next_state=cache_ready
else
  next_state=""
fi
if [[ -n "$next_state" ]]; then
  python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state "$next_state" \
    --operation cache-transfer --outcome "$next_state" --updates-json "$(python3 - "$cache_manifest_sha256" <<'PY'
import json, sys
print(json.dumps({"reference_cache_manifest_sha256": sys.argv[1]}))
PY
  )" >/dev/null
fi
guest_root_exec install -d -o "$guest_execution_user" -g "$guest_execution_user" -m 0700 "$guest_evidence_root/environment-probe"
printf '%s\n' "$receipt"
