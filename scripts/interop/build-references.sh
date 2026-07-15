#!/usr/bin/env bash
set -euo pipefail

script_dir=$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)
source "$script_dir/lib/common.sh"
offline=0
force=0
for arg in "$@"; do
  case "$arg" in
    --offline) offline=1 ;;
    --force-rebuild) force=1 ;;
    *) printf 'usage: build-references.sh [--offline] [--force-rebuild]\n' >&2; exit 2 ;;
  esac
done
"$script_dir/ubuntu/check-host.sh" --post-install --metadata "$BUILD_ROOT/host-metadata.json"
ensure_target_dirs
require_command python3
args=()
[[ "$offline" == "1" ]] && args+=(--offline)
[[ "$force" == "1" ]] && args+=(--force-rebuild)
"$script_dir/build-java-i2p.sh" "${args[@]}" >"$BUILD_ROOT/java-i2p-summary.txt"
"$script_dir/build-i2pd.sh" "${args[@]}" >"$BUILD_ROOT/i2pd-summary.txt"
python3 - "$REPO_ROOT" "$CACHE_ROOT" "$BUILD_ROOT" <<'PY'
import json
import sys
from pathlib import Path

root, cache_root, build_root = map(Path, sys.argv[1:])
entries = []
for reference, summary_name in (("java_i2p", "java-i2p-summary.txt"), ("i2pd", "i2pd-summary.txt")):
    values = {}
    for line in (build_root / summary_name).read_text(encoding="utf-8").splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            values[key] = value
    metadata = Path(values["metadata"]).resolve()
    entries.append({
        "reference": reference,
        "cache_key": values["cache_key"],
        "metadata": metadata.relative_to(root).as_posix(),
        "artifact_sha256": values["artifact_sha256"],
        "installed_tree_sha256": values["installed_tree_sha256"],
        "disposition": values.get("disposition", "built"),
    })
summary = {
    "schema": 2,
    "host_contract": "ubuntu-24.04-amd64",
    "lock_sha256": __import__("hashlib").sha256(
        (root / "tests/integration/ntcp2/references.lock.toml").read_bytes()
    ).hexdigest(),
    "references": entries,
}
destination = cache_root / "current-cache.json"
destination.write_text(json.dumps(summary, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
destination.chmod(0o600)
print(json.dumps(summary, sort_keys=True, separators=(",", ":")))
PY
