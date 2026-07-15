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
from config_contract import ConfigurationContractError, assert_i2pd_private_configuration, assert_java_private_configuration
from i2pd import I2pdAdapter, I2pdError
from i2pr import I2prAdapter
from launcher_protocol import LauncherScenarioError, LauncherStatusError, load_launcher_scenario, parse_status_line
from java_i2p import JavaI2pAdapter, JavaI2pError
from metadata import MetadataError, hash_runtime_tree, parse_metadata
from reference_scenario import load_reference_scenario
from reference_topology import ReferencePairTopology
from topology import EndpointDescription, NamespaceTopology, topology_token
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
            "actual_typed_result": "blocked",
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

    def test_launcher_scenario_and_status_contracts_are_strict(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            scenario_path = root / "scenario.toml"
            scenario_path.write_text(
                """[scenario]
schema = 1
scenario_id = "synthetic-run"
role = "initiator"
address_family = "ipv4"
local_address = "192.0.2.1"
local_port = 45680
peer_address = "192.0.2.2"
peer_port = 45678
network_id = 99
state_dir = "state"
peer_router_info = "exchange/peer.info"
handshake_deadline_ms = 30000
read_deadline_ms = 1000
write_deadline_ms = 1000
queue_deadline_ms = 1000
drain_deadline_ms = 1000
padding_profile = "representative"
smoke_message_profile = "delivery-status"
deterministic_seed = 1
expected_result_class = "authenticated-handshake-and-bounded-i2np-exchange"
status_path = "status.jsonl"
""",
                encoding="utf-8",
            )
            loaded = load_launcher_scenario(scenario_path)
            self.assertEqual(loaded.network_id, 99)
            self.assertEqual(loaded.peer_port, 45678)
            with self.assertRaises(LauncherScenarioError):
                load_launcher_scenario(root / "missing.toml")
            status = parse_status_line(
                '{"schema":1,"type":"i2pr-interop-status","scenario_id":"synthetic-run",'
                '"phase":"terminal","result":"rejected",'
                '"reason_code":"state_invalid",'
                '"counters":{"listener_ready":0,"authenticated":0,"frames_sent":0,'
                '"frames_received":0,"i2np_sent":0,"i2np_received":0}}'
            )
            self.assertEqual(status["result"], "rejected")
            with self.assertRaises(LauncherStatusError):
                parse_status_line(
                    '{"schema":1,"type":"i2pr-interop-status","scenario_id":"synthetic-run",'
                    '"phase":"listener_ready","result":"rejected",'
                    '"reason_code":"state_invalid",'
                    '"counters":{"listener_ready":0,"authenticated":0,"frames_sent":0,'
                    '"frames_received":0,"i2np_sent":0,"i2np_received":0}}'
                )

    def test_plan_041_reference_pair_scenarios_are_strict_and_directional(self) -> None:
        scenario_root = ROOT / "tests/integration/ntcp2/reference-scenarios"
        java_first = load_reference_scenario(scenario_root / "reference-java-i2pd-ipv4.toml")
        i2pd_first = load_reference_scenario(scenario_root / "reference-i2pd-java-ipv4.toml")
        self.assertEqual(java_first.dial_initiator, "java_i2p")
        self.assertEqual(i2pd_first.dial_initiator, "i2pd")
        self.assertEqual(java_first.reference_revisions["i2pd"], "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e")
        self.assertEqual(java_first.java.address, "192.0.2.1")
        self.assertEqual(java_first.i2pd.address, "192.0.2.2")

    def test_plan_041_configuration_contract_requires_identical_private_network_id(self) -> None:
        java = (ROOT / "tests/integration/ntcp2/config/java-i2p/router.config.template").read_text()
        java = java.replace("@CONFIG_DIR@", "/run/root/config").replace("@NTCP2_ADDRESS@", "192.0.2.1").replace("@NTCP2_PORT@", "45678").replace("@ADDRESS_FAMILY_IPV6@", "false")
        assert_java_private_configuration(java, address="192.0.2.1", port=45678, network_id=99)
        with self.assertRaises(ConfigurationContractError):
            assert_java_private_configuration(java.replace("router.networkID=99", "router.networkID=2"), address="192.0.2.1", port=45678, network_id=99)
        i2pd = (ROOT / "tests/integration/ntcp2/config/i2pd/i2pd.conf.template").read_text()
        i2pd = i2pd.replace("@ADDRESS4@", "192.0.2.2").replace("@ADDRESS6@", "").replace("@LOCAL_PORT@", "45679").replace("@IPV4_ENABLED@", "true").replace("@IPV6_ENABLED@", "false")
        assert_i2pd_private_configuration(i2pd, address="192.0.2.2", port=45679, network_id=99)
        with self.assertRaises(ConfigurationContractError):
            assert_i2pd_private_configuration(i2pd.replace("netid = 99", "netid = 2"), address="192.0.2.2", port=45679, network_id=99)

    def test_plan_041_topology_uses_reference_names_and_directional_firewall(self) -> None:
        scenario = load_reference_scenario(ROOT / "tests/integration/ntcp2/reference-scenarios/reference-java-i2pd-ipv4.toml")
        topology = ReferencePairTopology(scenario, "run-041-test")
        self.assertRegex(topology.java_namespace, r"^java-[0-9a-f]{8}$")
        self.assertRegex(topology.i2pd_namespace, r"^i2pd-[0-9a-f]{8}$")
        self.assertIn("ip daddr 192.0.2.2 tcp dport 45679 ct state new accept", topology.java_rules)
        self.assertNotIn("ip daddr 192.0.2.1 tcp dport 45678 ct state new accept", topology.i2pd_rules)
        self.assertNotIn("default", topology.java_rules)

    def test_plan_041_router_info_import_is_confined_to_the_run_root(self) -> None:
        scenario = load_reference_scenario(ROOT / "tests/integration/ntcp2/reference-scenarios/reference-java-i2pd-ipv4.toml")
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            run_root = root / "run"
            outside = root / "outside.info"
            outside.write_bytes(b"not-a-router-info")
            java_endpoint = EndpointDescription(
                local_address=scenario.java.address,
                peer_address=scenario.i2pd.address,
                local_port=scenario.java.port,
                peer_port=scenario.i2pd.port,
                address_family="ipv4",
                namespace="java-test",
                network_id="99",
            )
            i2pd_endpoint = EndpointDescription(
                local_address=scenario.i2pd.address,
                peer_address=scenario.java.address,
                local_port=scenario.i2pd.port,
                peer_port=scenario.java.port,
                address_family="ipv4",
                namespace="i2pd-test",
                network_id="99",
            )
            java = JavaI2pAdapter(root / "java-cache", run_root / "java", java_endpoint, ROOT)
            i2pd = I2pdAdapter(root / "i2pd-cache", run_root / "i2pd", i2pd_endpoint, ROOT)
            with self.assertRaises(JavaI2pError) as java_error:
                java.import_peer_router_info(outside)
            with self.assertRaises(I2pdError) as i2pd_error:
                i2pd.import_peer_router_info(outside)
            self.assertEqual(java_error.exception.code, "peer-router-info-outside-run-root")
            self.assertEqual(i2pd_error.exception.code, "peer-router-info-outside-run-root")

    def test_plan_041_pair_evidence_requires_dual_authentication_and_cleanup(self) -> None:
        pair = {
            "schema": 2, "scenario_id": "reference-java-i2pd-ipv4", "date_utc": "2026-01-01T00:00:00Z",
            "i2pr_commit": "a" * 40, "java_reference": "java_i2p", "java_version": "2.12.0",
            "java_revision": "2800040deee9bb376567b671ef2e9c34cf3e30b6", "java_artifact_sha256": "1" * 64,
            "java_installed_tree_sha256": "2" * 64, "java_configuration_sha256": "3" * 64,
            "i2pd_reference": "i2pd", "i2pd_version": "2.60.0", "i2pd_revision": "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
            "i2pd_artifact_sha256": "4" * 64, "i2pd_installed_tree_sha256": "5" * 64, "i2pd_configuration_sha256": "6" * 64,
            "namespace_topology_sha256": "7" * 64, "private_network_id": "explicit-non-public", "direction_policy": "java_i2p",
            "router_info_validation": {"java_i2p": "validated-and-bound", "i2pd": "validated-and-bound"},
            "authenticated_link_observations": {"java_i2p": "authenticated", "i2pd": "authenticated"},
            "connection_counters": {"java_i2p": {"attempts": 1, "authenticated": 1}, "i2pd": {"attempts": 0, "authenticated": 1}},
            "process_counters": {"java_i2p": {"started": 2, "exited": 2, "forced": 0}, "i2pd": {"started": 2, "exited": 2, "forced": 0}},
            "expected_authenticated_link_count": 1, "actual_typed_result": "passed", "cleanup_result": "clean",
            "evidence_sha256": "", "known_deviation": "reference-only-control", "reproduction": "reference-crosscheck-ipv4",
        }
        validate_record(pair)
        pair["authenticated_link_observations"]["i2pd"] = "not-observed"
        with self.assertRaises(EvidenceError):
            validate_record(pair)
        pair["authenticated_link_observations"]["i2pd"] = "authenticated"
        pair["cleanup_result"] = "failed"
        with self.assertRaises(EvidenceError):
            validate_record(pair)


if __name__ == "__main__":
    unittest.main()
