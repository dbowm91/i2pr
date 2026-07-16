#!/usr/bin/env python3
"""Validate and aggregate the four sanitized Plan 045 guest records."""

from __future__ import annotations

import argparse
import hashlib
import json
from pathlib import Path

ROOT = Path(__file__).resolve().parents[3]
import sys
sys.path.insert(0, str(ROOT / "tests/integration/ntcp2"))
from harness.evidence import validate_file  # noqa: E402

EXPECTED = {
    "i2pr-to-java-ipv4": "java_i2p",
    "java-to-i2pr-ipv4": "java_i2p",
    "i2pr-to-i2pd-ipv4": "i2pd",
    "i2pd-to-i2pr-ipv4": "i2pd",
}


def sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def aggregate(evidence: Path, environment: Path, probe: Path) -> dict[str, object]:
    environment_value = json.loads(environment.read_text(encoding="utf-8"))
    if environment_value.get("schema") != 1 or environment_value.get("rootless_probe_outcome") != "rootless_sandbox_available":
        raise ValueError("environment record is not a successful recovery guest")
    probe_value = json.loads(probe.read_text(encoding="utf-8"))
    if probe_value.get("outcome") != "rootless_sandbox_available":
        raise ValueError("rootless probe is not successful")
    records = []
    attestation = ""
    for scenario, reference in EXPECTED.items():
        path = evidence / f"{scenario}.json"
        if not path.is_file():
            raise ValueError(f"missing direction record: {scenario}")
        validate_file(path)
        value = json.loads(path.read_text(encoding="utf-8"))
        if value.get("scenario_id") != scenario or value.get("reference") != reference:
            raise ValueError("direction/reference mapping mismatch")
        if value.get("actual_typed_result") != "passed" or value.get("cleanup_result") != "clean":
            raise ValueError("direction did not pass with clean cleanup")
        if value.get("topology_kind") != "rootless-sealed-single-netns" or value.get("privilege_model") != "unprivileged-userns":
            raise ValueError("direction topology contract mismatch")
        if not isinstance(value.get("sandbox_attestation_sha256"), str) or not value["sandbox_attestation_sha256"].strip("0"):
            raise ValueError("direction attestation is missing")
        if value.get("parent_network_state_unchanged") is not True:
            raise ValueError("direction changed parent network state")
        current = value["sandbox_attestation_sha256"]
        if attestation and attestation != current:
            raise ValueError("direction attestations differ")
        attestation = current
        records.append({
            "scenario_id": scenario,
            "reference": reference,
            "sha256": sha256(path),
            "actual_typed_result": value["actual_typed_result"],
            "cleanup_result": value["cleanup_result"],
        })
    return {
        "schema": 1,
        "type": "multipass-interop-aggregate",
        "environment_sha256": sha256(environment),
        "probe_sha256": sha256(probe),
        "source_commit": environment_value["source_commit"],
        "source_tree_sha256": environment_value["source_tree_sha256"],
        "reference_cache_manifest_sha256": environment_value["reference_cache_manifest_sha256"],
        "topology_kind": "rootless-sealed-single-netns",
        "privilege_model": "unprivileged-userns",
        "sandbox_attestation_sha256": attestation,
        "directions": records,
        "matrix_result": "passed",
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--evidence", type=Path, required=True)
    parser.add_argument("--environment", type=Path, required=True)
    parser.add_argument("--probe", type=Path, required=True)
    args = parser.parse_args()
    value = aggregate(args.evidence, args.environment, args.probe)
    aggregate_path = args.evidence / "aggregate.json"
    aggregate_path.write_text(json.dumps(value, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    aggregate_path.chmod(0o600)
    files = [args.environment, args.probe] + [args.evidence / f"{scenario}.json" for scenario in EXPECTED] + [aggregate_path]
    manifest = {
        "schema": 1,
        "type": "multipass-interop-manifest",
        "files": [{"name": path.name, "sha256": sha256(path)} for path in files],
        "aggregate_sha256": sha256(aggregate_path),
    }
    manifest_path = args.evidence / "manifest.json"
    manifest_path.write_text(json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
    manifest_path.chmod(0o600)
    print(json.dumps(value, sort_keys=True, separators=(",", ":")))
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
