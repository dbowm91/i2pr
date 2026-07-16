#!/usr/bin/env python3
"""Collect one already-sanitized direction record from a guest run."""

from __future__ import annotations

import argparse
import json
import shutil
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tests/integration/ntcp2"))
from harness.evidence import validate_file  # noqa: E402
from harness.rootless_supervisor import SandboxError, verify_attestation_file  # noqa: E402

SCENARIOS = frozenset({
    "i2pr-to-java-ipv4", "java-to-i2pr-ipv4", "i2pr-to-i2pd-ipv4", "i2pd-to-i2pr-ipv4",
})


def collect(repo_root: Path, scenario: str, output: Path) -> dict[str, object]:
    if scenario not in SCENARIOS:
        raise ValueError("unknown direction")
    evidence = repo_root / "target/interop/evidence"
    candidates: list[Path] = []
    for path in evidence.glob("*.json"):
        if path.name in {output.name, "run-manifest.json"} or not path.is_file():
            continue
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, json.JSONDecodeError):
            continue
        if value.get("scenario_id") == scenario:
            validate_file(path)
            candidates.append(path)
    if not candidates:
        raise ValueError("direction record is missing")
    selected = max(candidates, key=lambda path: path.stat().st_mtime_ns)
    output.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    shutil.copyfile(selected, output)
    output.chmod(0o600)
    return json.loads(output.read_text(encoding="utf-8"))


def clear_scenario(repo_root: Path, scenario: str) -> None:
    """Remove only a previous sanitized record for this direction."""

    if scenario not in SCENARIOS:
        raise ValueError("unknown direction")
    evidence = repo_root / "target/interop/evidence"
    for path in evidence.glob("*.json"):
        if not path.is_file() or path.name == "run-manifest.json":
            continue
        try:
            value = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, UnicodeError, json.JSONDecodeError):
            continue
        if value.get("scenario_id") == scenario:
            path.unlink()


def validate_attestation(path: Path) -> str:
    try:
        verify_attestation_file(path)
    except (OSError, ValueError, SandboxError) as exc:
        raise ValueError("invalid rootless attestation") from exc
    value = json.loads(path.read_text(encoding="utf-8"))
    if value.get("parent_network_state_unchanged") is not True:
        raise ValueError("parent network state changed")
    return str(value["attestation_sha256"])


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--root", type=Path, required=True)
    parser.add_argument("--scenario")
    parser.add_argument("--output", type=Path)
    parser.add_argument("--attestation", type=Path)
    parser.add_argument("--clear-scenario")
    args = parser.parse_args()
    if args.clear_scenario:
        clear_scenario(args.root, args.clear_scenario)
    if args.scenario and args.output:
        collect(args.root, args.scenario, args.output)
    if args.attestation:
        print(validate_attestation(args.attestation))
    if not ((args.scenario and args.output) or args.attestation or args.clear_scenario):
        parser.error("one collection or attestation operation is required")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
