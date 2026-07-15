#!/usr/bin/env python3
"""Create or verify the exact reference cache selected by current-cache.json."""

from __future__ import annotations

import argparse
import hashlib
import json
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tests/integration/ntcp2"))
from harness.build_gate import BuildGateError, build_cache_manifest, validate_cache_manifest  # noqa: E402


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--verify", action="store_true")
    args = parser.parse_args()
    repo_root = Path(__file__).resolve().parents[2]
    manifest = repo_root / "target/interop/build/reference-cache-manifest.json"
    try:
        if args.verify:
            validate_cache_manifest(repo_root, manifest)
            print("verified selected reference cache manifest")
        else:
            build_cache_manifest(repo_root, manifest)
            summary_path = repo_root / "target/interop/build/reference-build-summary.json"
            if summary_path.is_file():
                summary = json.loads(summary_path.read_text(encoding="utf-8"))
                summary["cache_manifest_sha256"] = hashlib.sha256(manifest.read_bytes()).hexdigest()
                summary_path.write_text(
                    json.dumps(summary, sort_keys=True, separators=(",", ":")) + "\n",
                    encoding="utf-8",
                )
                summary_path.chmod(0o600)
            print(f"wrote {manifest.relative_to(repo_root)}")
    except (BuildGateError, OSError) as exc:
        print(f"cache manifest error: {exc}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
