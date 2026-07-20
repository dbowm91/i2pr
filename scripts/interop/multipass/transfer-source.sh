#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/common.sh"

commit=""
while (($#)); do
  case "$1" in
    --commit)
      [[ -z "$commit" && $# -ge 2 ]] || die "duplicate or incomplete --commit"
      commit=$2
      shift
      ;;
    --help|-h) printf 'usage: transfer-source.sh --commit <40-char-sha>\n'; exit 0 ;;
    *) die "unknown argument: $1" ;;
  esac
  shift
done
[[ -n "$commit" ]] || die "--commit is required"
validate_commit "$commit"
require_owned_instance
require_command git
require_command gzip
require_command tar
ensure_dirs
acquire_lifecycle_lock
require_owned_instance

[[ "$(git -C "$repo_root" rev-parse HEAD)" == "$commit" ]] || {
  typed_blocker blocked_source_commit_mismatch
  exit 2
}
git -C "$repo_root" diff --quiet || { typed_blocker blocked_source_dirty; exit 2; }
git -C "$repo_root" diff --cached --quiet || { typed_blocker blocked_source_dirty; exit 2; }
if git -C "$repo_root" status --porcelain --untracked-files=all | awk 'length($0) > 3 {print substr($0,4)}' | grep -v '^target/interop/' | grep -q .; then
  typed_blocker blocked_source_dirty
  exit 2
fi

archive="$instance_state_dir/source/$commit.tar.gz"
mkdir -p "$(dirname "$archive")"
git -C "$repo_root" archive --format=tar --mtime='UTC 1970-01-01' "$commit" | gzip -n >"$archive"
archive_sha256=$(sha256_file "$archive")
archive_listing="$instance_state_dir/source/$commit.listing.txt"
tar -tzf "$archive" | LC_ALL=C sort >"$archive_listing"
tree_sha256=$(python3 - "$repo_root" "$archive_listing" <<'PY'
import sys
from pathlib import Path
sys.path.insert(0, str(Path(sys.argv[1]) / "scripts/interop/multipass"))
from source_tree import tree_hash
listing = Path(sys.argv[2]).read_text(encoding="utf-8")
print(tree_hash(Path(sys.argv[1]), listing))
PY
)
guest_archive="/tmp/i2pr-source-$commit.tar.gz"
guest_listing="/tmp/i2pr-source-$commit.listing.txt"
multipass transfer "$archive" "$instance_name:$guest_archive" >/dev/null
multipass transfer "$archive_listing" "$instance_name:$guest_listing" >/dev/null
guest_root_exec rm -rf "$guest_repo_root"
guest_root_exec install -d -o "$guest_execution_user" -g "$guest_execution_user" -m 0700 "$guest_repo_root"
guest_root_exec install -o "$guest_execution_user" -g "$guest_execution_user" -m 0600 "$guest_archive" "$guest_repo_root/.staging-source.tar.gz"
guest_root_exec install -o "$guest_execution_user" -g "$guest_execution_user" -m 0600 "$guest_listing" "$guest_repo_root/.staging-source.listing.txt"
guest_root_exec rm -f "$guest_archive" "$guest_listing"
guest_archive="$guest_repo_root/.staging-source.tar.gz"
guest_listing_local="$guest_repo_root/.staging-source.listing.txt"
guest_exec tar -xzf "$guest_archive" -C "$guest_repo_root"
guest_exec rm -f "$guest_archive"
guest_exec mv "$guest_listing_local" "$guest_repo_root/.source-listing.txt"
guest_exec python3 "$guest_repo_root/scripts/interop/multipass/source_tree.py" \
  --root "$guest_repo_root" --commit "$commit" --archive-sha256 "$archive_sha256" \
  --archive-listing "$guest_repo_root/.source-listing.txt"
guest_exec python3 "$guest_repo_root/scripts/interop/multipass/source_tree.py" \
  --root "$guest_repo_root" --commit "$commit" --archive-sha256 "$archive_sha256" --verify \
  --archive-listing "$guest_repo_root/.source-listing.txt"

receipt=$(python3 - "$commit" "$archive_sha256" "$tree_sha256" "$environment_manifest_sha256" <<'PY'
import datetime as dt
import json
import sys
print(json.dumps({
    "schema": 1,
    "type": "multipass-source-transfer",
    "source_commit": sys.argv[1],
    "source_archive_sha256": sys.argv[2],
    "source_tree_sha256": sys.argv[3],
    "environment_manifest_sha256": sys.argv[4],
    "archive_format": "git-archive-tar-gzip",
    "completed_at": dt.datetime.now(dt.UTC).replace(microsecond=0).isoformat().replace("+00:00", "Z"),
}, sort_keys=True, separators=(",", ":")))
PY
)
write_json "$instance_state_dir/source-transfer.json" "$receipt"
current_state=$(python3 -c 'import json,sys; print(json.load(sys.stdin)["state"])' <"$instance_lifecycle_path")
if [[ "$current_state" == provisioned ]]; then
  next_state=source_ready
elif [[ "$current_state" == cache_ready ]]; then
  next_state=source_and_cache_ready
else
  next_state=""
fi
if [[ -n "$next_state" ]]; then
  python3 "$lifecycle_py" update --state-file "$instance_lifecycle_path" --state "$next_state" \
    --operation source-transfer --outcome "$next_state" --updates-json "$(python3 - "$archive" <<'PY'
import hashlib, json, sys
print(json.dumps({"source_archive_sha256": hashlib.sha256(open(sys.argv[1], "rb").read()).hexdigest()}))
PY
)" >/dev/null
fi
printf '%s\n' "$receipt"
