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
    if environment.get("guest_rootless_probe_outcome") != "rootless_sandbox_available" or environment.get("execution_user_privileged") is not False:
        raise ValueError("environment contract is not successful")
    lifecycle = json.loads((source / "lifecycle.json").read_text(encoding="utf-8"))
    for field in ("schema_version", "environment_id", "run_id", "instance_generation", "instance_name_digest", "state", "environment_manifest_sha256", "cloud_init_sha256"):
        if field not in lifecycle:
            raise ValueError(f"lifecycle attribution is missing: {field}")
    if "instance_name" in lifecycle or "owner_token_sha256" in lifecycle:
        raise ValueError("lifecycle export contains host-only identity material")
    if lifecycle["run_id"] != environment.get("run_id") or lifecycle["instance_generation"] != environment.get("instance_generation"):
        raise ValueError("lifecycle and environment identity differ")
    attribution = None
    for name in ("i2pr-to-java-ipv4.json", "java-to-i2pr-ipv4.json", "i2pr-to-i2pd-ipv4.json", "i2pd-to-i2pr-ipv4.json"):
        path = source / name
        validate_file(path)
        value = json.loads(path.read_text(encoding="utf-8"))
        if value.get("actual_typed_result") != "passed" or value.get("cleanup_result") != "clean":
            raise ValueError(f"direction is not a passing clean record: {name}")
        current = tuple(value.get(field) for field in (
            "environment_id", "run_id", "instance_generation", "environment_evidence_sha256",
            "instance_name_digest", "lifecycle_schema_version", "ownership_record_sha256",
            "environment_manifest_sha256", "cloud_init_sha256", "host_baseline_probe_outcome",
            "guest_rootless_probe_outcome", "adoption_mode",
        ))
        if any(item in (None, "") for item in current):
            raise ValueError(f"direction attribution is missing: {name}")
        if attribution is not None and current != attribution:
            raise ValueError("direction attribution differs")
        attribution = current
    if attribution is None or attribution[0] != environment.get("environment_id") or attribution[1] != environment.get("run_id"):
        raise ValueError("environment attribution differs")
    if attribution[2] != environment.get("instance_generation") or attribution[4] != environment.get("instance_name_digest"):
        raise ValueError("environment generation/name attribution differs")
    if attribution[7] != environment.get("environment_manifest_sha256") or attribution[8] != environment.get("cloud_init_sha256"):
        raise ValueError("environment contract attribution differs")


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
