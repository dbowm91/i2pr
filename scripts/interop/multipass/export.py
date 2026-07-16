#!/usr/bin/env python3
"""Validate a guest evidence bundle and atomically install it on the host."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import stat
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
import sys
sys.path.insert(0, str(ROOT / "tests/integration/ntcp2"))
from harness.evidence import validate_file  # noqa: E402

EXPECTED = {
    "environment.json", "environment.json.sha256", "probe.json", "probe.json.sha256",
    "i2pr-to-java-ipv4.json", "java-to-i2pr-ipv4.json", "i2pr-to-i2pd-ipv4.json",
    "i2pd-to-i2pr-ipv4.json", "aggregate.json", "manifest.json", "lifecycle.json",
}


def digest(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate(source: Path) -> None:
    names = {path.name for path in source.iterdir()}
    if names != EXPECTED:
        raise ValueError(f"evidence file set mismatch: {sorted(names ^ EXPECTED)}")
    for path in source.iterdir():
        mode = path.lstat().st_mode
        if path.is_symlink() or not stat.S_ISREG(mode) or path.stat().st_nlink != 1:
            raise ValueError(f"unsafe evidence file: {path.name}")
        if path.stat().st_size > 512 * 1024:
            raise ValueError(f"evidence file is oversized: {path.name}")
    manifest = json.loads((source / "manifest.json").read_text(encoding="utf-8"))
    if manifest.get("schema") != 1 or manifest.get("type") != "multipass-interop-manifest":
        raise ValueError("unsupported evidence manifest")
    if manifest.get("aggregate_sha256") != digest(source / "aggregate.json"):
        raise ValueError("aggregate digest mismatch")
    entries = {entry.get("name"): entry.get("sha256") for entry in manifest.get("files", [])}
    for name in EXPECTED - {"manifest.json", "environment.json.sha256", "probe.json.sha256"}:
        if entries.get(name) != digest(source / name):
            raise ValueError(f"guest manifest digest mismatch: {name}")
    for name in ("environment.json", "probe.json"):
        value = (source / f"{name}.sha256").read_text(encoding="utf-8").strip().split()
        if value != [digest(source / name), name]:
            raise ValueError(f"sidecar digest mismatch: {name}")
    environment = json.loads((source / "environment.json").read_text(encoding="utf-8"))
    if environment.get("rootless_probe_outcome") != "rootless_sandbox_available" or environment.get("execution_user_privileged") is not False:
        raise ValueError("environment contract is not successful")
    for name in ("i2pr-to-java-ipv4.json", "java-to-i2pr-ipv4.json", "i2pr-to-i2pd-ipv4.json", "i2pd-to-i2pr-ipv4.json"):
        path = source / name
        validate_file(path)
        value = json.loads(path.read_text(encoding="utf-8"))
        if value.get("actual_typed_result") != "passed" or value.get("cleanup_result") != "clean":
            raise ValueError(f"direction is not a passing clean record: {name}")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--source", type=Path, required=True)
    parser.add_argument("--destination", type=Path, required=True)
    args = parser.parse_args()
    validate(args.source)
    if args.destination.exists():
        raise ValueError("destination already exists")
    args.destination.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    os.replace(args.source, args.destination)
    print(json.dumps({"schema": 1, "type": "multipass-evidence-export", "result": "validated"}, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
