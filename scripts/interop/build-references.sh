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
import hashlib
import sys
from pathlib import Path

root, cache_root, build_root = map(Path, sys.argv[1:])
entries = []
existing_summary_path = cache_root / "current-cache.json"
existing_entries = {}
if existing_summary_path.is_file():
    try:
        existing = json.loads(existing_summary_path.read_text(encoding="utf-8"))
    except (OSError, json.JSONDecodeError):
        existing = {}
    if isinstance(existing, dict):
        for prior in existing.get("references", []):
            if isinstance(prior, dict) and isinstance(prior.get("reference"), str):
                existing_entries[prior["reference"]] = prior
for reference, summary_name in (("java_i2p", "java-i2p-summary.txt"), ("i2pd", "i2pd-summary.txt")):
    values = {}
    for line in (build_root / summary_name).read_text(encoding="utf-8").splitlines():
        if "=" in line:
            key, value = line.split("=", 1)
            values[key] = value
    metadata = Path(values["metadata"]).resolve()
    metadata_values = {
        line.split("=", 1)[0]: line.split("=", 1)[1]
        for line in metadata.read_text(encoding="utf-8").splitlines()
        if "=" in line
    }
    prior_entry = existing_entries.get(reference, {})
    entries.append({
        "reference": reference,
        "cache_key": values["cache_key"],
        "metadata": metadata.relative_to(root).as_posix(),
        "source_revision": metadata_values["source_revision"],
        "build_command_version": metadata_values["build_command_version"],
        "artifact_sha256": values.get(
            "artifact_sha256", prior_entry.get("artifact_sha256")
        ),
        "installed_tree_sha256": values.get(
            "installed_tree_sha256", prior_entry.get("installed_tree_sha256")
        ),
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
host_metadata = build_root / "host-metadata.json"
host = json.loads(host_metadata.read_text(encoding="utf-8"))
build_summary = {
    "schema": 1,
    "host_contract": summary["host_contract"],
    "lock_sha256": summary["lock_sha256"],
    "host_metadata_sha256": hashlib.sha256(host_metadata.read_bytes()).hexdigest(),
    "host_tools": {
        key: host[key]
        for key in ("os_id", "os_version", "architecture", "kernel", "python", "java", "ant", "cmake", "compiler", "nft", "iproute2")
    },
    "references": entries,
}
summary_path = build_root / "reference-build-summary.json"
summary_path.write_text(json.dumps(build_summary, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
summary_path.chmod(0o600)
print(json.dumps(summary, sort_keys=True, separators=(",", ":")))
PY
python3 "$script_dir/cache-manifest.py"
