#!/usr/bin/env python3
"""Create and validate the narrow sanitized Plan 043 run manifest."""

from __future__ import annotations

import argparse
import hashlib
import json
import os
import re
import subprocess
import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tests/integration/ntcp2"))
from harness.build_gate import (  # noqa: E402
    BuildGateError,
    _record_paths,
    gates_for_profile,
    scenarios_for_gate,
    validate_aggregate_manifest,
)


def _sha256(path: Path) -> str:
    return hashlib.sha256(path.read_bytes()).hexdigest()


def _commit(repo_root: Path) -> str:
    result = subprocess.run(
        ["git", "-C", str(repo_root), "rev-parse", "HEAD"],
        capture_output=True,
        text=True,
        check=False,
    )
    value = result.stdout.strip()
    if result.returncode != 0 or not re.fullmatch(r"[0-9a-f]{40}", value):
        raise BuildGateError("current checkout has no exact commit")
    return value


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--profile", required=True)
    args = parser.parse_args()
    repo_root = Path(__file__).resolve().parents[2]
    evidence_root = repo_root / "target/interop/evidence"
    summary_path = repo_root / "target/interop/build/reference-build-summary.json"
    host_metadata = repo_root / "target/interop/build/host-metadata.json"
    clean_marker = repo_root / "target/interop/build/clean-host-verification.json"
    manifest_path = evidence_root / "run-manifest.json"
    try:
        gates = gates_for_profile(args.profile)
        summary = json.loads(summary_path.read_text(encoding="utf-8"))
        clean = json.loads(clean_marker.read_text(encoding="utf-8"))
        if summary.get("schema") != 1 or clean.get("schema") != 1 or clean.get("result") != "clean":
            raise BuildGateError("build summary or cleanup verification is not valid")
        records = _record_paths(evidence_root)
        entries = [
            {
                "gate": gate,
                "scenario_id": json.loads(path.read_text(encoding="utf-8"))["scenario_id"],
                "filename": path.name,
                "sha256": _sha256(path),
            }
            for gate, path in records
        ]
        expected = {gate: list(scenarios_for_gate(gate)) for gate in gates}
        manifest = {
            "schema": 1,
            "profile": args.profile,
            "i2pr_commit": _commit(repo_root),
            "workflow_run_id": os.environ.get("GITHUB_RUN_ID", "local"),
            "workflow_run_attempt": os.environ.get("GITHUB_RUN_ATTEMPT", "0"),
            "host_contract": "ubuntu-24.04-amd64",
            "host_contract_digest": _sha256(host_metadata),
            "lock_sha256": summary["lock_sha256"],
            "reference_cache": summary["references"],
            "expected_scenarios": expected,
            "records": entries,
            "per_gate_disposition": {gate: "passed" for gate in gates},
            "cleanup_verification": "clean",
            "aggregate_manifest_sha256": "",
        }
        manifest["aggregate_manifest_sha256"] = hashlib.sha256(
            json.dumps(manifest, sort_keys=True, separators=(",", ":")).encode()
        ).hexdigest()
        evidence_root.mkdir(mode=0o700, parents=True, exist_ok=True)
        manifest_path.write_text(json.dumps(manifest, sort_keys=True, separators=(",", ":")) + "\n", encoding="utf-8")
        manifest_path.chmod(0o600)
        validate_aggregate_manifest(repo_root, manifest_path, args.profile)
    except (BuildGateError, OSError, KeyError, TypeError, ValueError, json.JSONDecodeError) as exc:
        print(f"aggregate evidence error: {exc}", file=sys.stderr)
        return 1
    print(f"validated aggregate manifest for {args.profile}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
