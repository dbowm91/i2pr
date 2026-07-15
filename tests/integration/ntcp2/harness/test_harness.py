from __future__ import annotations

import json
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import tomllib

from evidence import EvidenceError, validate_record


ROOT = Path(__file__).resolve().parents[4]


class HarnessContractTests(unittest.TestCase):
    def test_lock_manifest_has_exact_pins_and_verified_izpack(self) -> None:
        lock = tomllib.loads((ROOT / "tests/integration/ntcp2/references.lock.toml").read_text())
        self.assertEqual(lock["reference"]["java_i2p"]["source_revision"], "2800040")
        self.assertEqual(lock["reference"]["i2pd"]["source_revision"], "f618e41")
        self.assertEqual(len(lock["izpack"]["sha256"]), 64)

    def test_all_manifest_scenarios_have_schema_files(self) -> None:
        manifest = (ROOT / "tests/integration/ntcp2/manifest.toml").read_text()
        ids = {line.split('"')[1] for line in manifest.splitlines() if line.startswith('id = "')}
        files = sorted((ROOT / "tests/integration/ntcp2/scenarios").glob("*.toml"))
        scenario_ids = {tomllib.loads(path.read_text())["scenario"]["id"] for path in files}
        self.assertEqual(ids, scenario_ids)
        self.assertEqual(len(files), 8)

    def test_evidence_rejects_endpoint_and_secret_material(self) -> None:
        base = {
            "schema": 1,
            "scenario_id": "synthetic",
            "date_utc": "2026-01-01T00:00:00Z",
            "i2pr_commit": "record-at-execution",
            "reference": "i2pd",
            "reference_version": "2.60.0",
            "reference_revision": "f618e41",
            "artifact_sha256": "0" * 64,
            "installed_tree_sha256": "0" * 64,
            "configuration_sha256": "0" * 64,
            "namespace_topology_sha256": "0" * 64,
            "direction": "both",
            "address_family": "ipv4",
            "deterministic_parameters": "seed=1",
            "expected": "bounded",
            "actual_typed_result": "blocked_missing_driver",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 0},
            "cleanup_result": "not-started",
            "evidence_sha256": "",
            "known_deviation": "driver absent",
            "reproduction": "bash scripts/interop/run-scenario.sh",
        }
        validate_record(base)
        base["known_deviation"] = "10.0.0.1:45678"
        with self.assertRaises(EvidenceError):
            validate_record(base)
        base["known_deviation"] = "-----BEGIN PRIVATE KEY-----"
        with self.assertRaises(EvidenceError):
            validate_record(base)

    def test_runner_emits_typed_block_without_raw_error(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            command = [
                sys.executable,
                str(ROOT / "tests/integration/ntcp2/harness/runner.py"),
                "--scenario",
                "smoke-i2pd-ipv4",
                "--reference",
                "i2pd",
                "--build-cache",
                directory,
                "--run-root",
                directory,
            ]
            completed = subprocess.run(command, capture_output=True, text=True, check=False)
        self.assertNotEqual(completed.returncode, 0)
        value = json.loads(completed.stdout.strip())
        self.assertEqual(value["actual_typed_result"], "blocked_host_contract")
        self.assertNotIn("Traceback", completed.stdout)


if __name__ == "__main__":
    unittest.main()
