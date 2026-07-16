#!/usr/bin/env python3
"""Write hashes for the fixed sanitized guest evidence files."""

from __future__ import annotations

import argparse
import hashlib
from pathlib import Path

FILES = ("environment.json", "probe.json")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", type=Path, required=True)
    args = parser.parse_args()
    manifest = args.evidence / "manifest.json"
    if manifest.is_file():
        import json
        files = []
        for path in sorted(args.evidence.iterdir()):
            if path.is_file() and path.name not in {"manifest.json", "environment.json.sha256", "probe.json.sha256"}:
                files.append({"name": path.name, "sha256": hashlib.sha256(path.read_bytes()).hexdigest()})
        value = json.loads(manifest.read_text(encoding="utf-8"))
        value["files"] = files
        manifest.write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
        manifest.chmod(0o600)
    for name in FILES:
        path = args.evidence / name
        if path.is_file():
            sidecar = path.with_name(f"{name}.sha256")
            sidecar.write_text(f"{hashlib.sha256(path.read_bytes()).hexdigest()}  {name}\n", encoding="utf-8")
            sidecar.chmod(0o600)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
