from __future__ import annotations

import json
import hashlib
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import tomllib

from evidence import EvidenceError, validate_file, validate_record, write_record
from firewall import canonical_firewall_rules
from metadata import MetadataError, hash_runtime_tree, parse_metadata
from topology import NamespaceTopology, topology_token
from router_info import netdb_filename


ROOT = Path(__file__).resolve().parents[4]


class HarnessContractTests(unittest.TestCase):
    def test_lock_manifest_has_exact_pins_and_verified_izpack(self) -> None:
        lock = tomllib.loads((ROOT / "tests/integration/ntcp2/references.lock.toml").read_text())
        self.assertRegex(lock["reference"]["java_i2p"]["source_revision"], r"^[0-9a-f]{40}$")
        self.assertRegex(lock["reference"]["i2pd"]["source_revision"], r"^[0-9a-f]{40}$")
        self.assertNotIn("java-i2p", lock["reference"])
        self.assertEqual(len(lock["izpack"]["sha256"]), 64)

    def test_metadata_parser_rejects_duplicates_and_confined_paths(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            cache = Path(directory)
            (cache / "bin").mkdir()
            artifact = cache / "bin/i2pd"
            artifact.write_bytes(b"i2pd")
            artifact.chmod(0o755)
            tree_hash = hash_runtime_tree(cache)
            digest = hashlib.sha256(b"i2pd").hexdigest()
            lines = [
                "schema=2", "reference=i2pd", "source_revision=" + "a" * 40,
                "source_repository=https://example.invalid/i2pd.git", "lock_sha256=" + "b" * 64,
                "build_command_version=test", "host_contract=ubuntu-24.04-amd64",
                "artifact_sha256=" + digest, "artifact_path=bin/i2pd",
                "installed_tree_sha256=" + tree_hash, "launcher=bin/i2pd",
                "execution_network=forbidden", "toolchain=test", "launcher_probe=test",
                "version_check=test", "test_disposition=not-available",
            ]
            metadata = cache / "build-metadata.txt"
            metadata.write_text("\n".join(lines) + "\n")
            parsed = parse_metadata(metadata, selected_reference="i2pd", cache_root=cache)
            self.assertEqual(parsed.reference, "i2pd")
            metadata.write_text("\n".join(lines + ["reference=i2pd"]) + "\n")
            with self.assertRaises(MetadataError):
                parse_metadata(metadata, selected_reference="i2pd", cache_root=cache)
            metadata.write_text("\n".join(line.replace("a" * 40, "a" * 7) for line in lines) + "\n")
            with self.assertRaises(MetadataError):
                parse_metadata(metadata, selected_reference="i2pd", cache_root=cache)

    def test_topology_token_and_interface_bounds(self) -> None:
        self.assertEqual(topology_token("same"), topology_token("same"))
        self.assertNotEqual(topology_token("one"), topology_token("two"))
        topology = NamespaceTopology(ROOT, "run-20260715T000000Z-1-abcd", False)
        self.assertLessEqual(len(topology.i2pr_if.encode()), 15)
        self.assertLessEqual(len(topology.reference_if.encode()), 15)

    def test_firewall_uses_destination_ports_and_narrow_ipv6(self) -> None:
        rules = canonical_firewall_rules(
            local_ipv4="192.0.2.2", peer_ipv4="192.0.2.1", local_port=45679, peer_port=45680,
            local_ipv6="2001:db8:36::2", peer_ipv6="2001:db8:36::1",
        )
        self.assertIn("ip saddr 192.0.2.1 tcp dport 45679 accept", rules)
        self.assertNotIn("tcp sport", rules)
        self.assertIn("ip6 daddr 2001:db8:36::1 tcp dport 45680 accept", rules)
        self.assertIn("ip6 saddr 2001:db8:36::1 tcp dport 45679 accept", rules)
        self.assertNotIn("ip6 daddr 2001:db8:36::/64 accept", rules)

    def test_router_info_import_name_is_derived_and_bounded(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "peer.info"
            identity = b"i" * 384 + b"\x00\x00\x03" + b"cert"
            path.write_bytes(identity + b"payload")
            name = netdb_filename(path)
            self.assertRegex(name, r"^routerInfo-[A-Za-z0-9~-]+\.dat$")
            with self.assertRaises(ValueError):
                netdb_filename(Path(directory) / "missing.info")

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
            "i2pr_commit": "a" * 40 + ";clean",
            "reference": "i2pd",
            "reference_version": "2.60.0",
            "reference_revision": "f" * 40,
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

    def test_passed_evidence_is_atomic_and_digest_validated(self) -> None:
        base = {
            "schema": 1,
            "scenario_id": "synthetic",
            "date_utc": "2026-01-01T00:00:00Z",
            "i2pr_commit": "a" * 40 + ";clean",
            "reference": "i2pd",
            "reference_version": "2.60.0",
            "reference_revision": "f" * 40,
            "artifact_sha256": "1" * 64,
            "installed_tree_sha256": "2" * 64,
            "configuration_sha256": "3" * 64,
            "namespace_topology_sha256": "4" * 64,
            "direction": "both",
            "address_family": "ipv4",
            "deterministic_parameters": "seed=1;timeouts=bounded",
            "expected": "bounded",
            "actual_typed_result": "passed",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 1, "exited": 1, "forced": 0},
            "cleanup_result": "clean",
            "evidence_sha256": "",
            "known_deviation": "environment-smoke-only",
            "reproduction": "bash scripts/interop/run-scenario.sh --scenario synthetic --reference i2pd",
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "record.json"
            write_record(path, base)
            validate_file(path)
            self.assertTrue(path.is_file())
            self.assertFalse((Path(directory) / "secret.log").exists())

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
