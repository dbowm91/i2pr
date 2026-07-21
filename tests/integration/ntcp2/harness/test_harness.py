from __future__ import annotations

import json
import hashlib
import os
import re
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
from build_gate import BuildGateError, build_cache_manifest, gates_for_profile, validate_cache_manifest
from reference_scenario import load_reference_scenario
from reference_topology import ReferencePairTopology
from topology import EndpointDescription, NamespaceTopology, topology_token
from router_info import netdb_filename


ROOT = Path(__file__).resolve().parents[4]


class HarnessContractTests(unittest.TestCase):
    def test_plan_043_profiles_have_ordered_distinct_gate_chains(self) -> None:
        self.assertEqual(gates_for_profile("environment-smoke"), ("environment-smoke",))
        self.assertEqual(
            gates_for_profile("full"),
            ("environment-smoke", "reference-crosscheck-ipv4", "handshake-smoke", "full"),
        )
        with self.assertRaises(BuildGateError):
            gates_for_profile("arbitrary-shell-fragment")

    def test_plan_043_cache_manifest_rejects_mutated_selected_artifact(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            lock = root / "tests/integration/ntcp2/references.lock.toml"
            lock.parent.mkdir(parents=True)
            lock.write_text("schema = 2\n", encoding="utf-8")
            cache_root = root / "target/interop/cache"
            references = []
            for reference in ("java_i2p", "i2pd"):
                key = ("a" if reference == "java_i2p" else "b") * 64
                cache = cache_root / reference / key
                (cache / "bin").mkdir(parents=True)
                launcher = cache / "bin/launcher"
                launcher.write_bytes(reference.encode())
                launcher.chmod(0o755)
                artifact_sha256 = hashlib.sha256(launcher.read_bytes()).hexdigest()
                metadata = cache / "build-metadata.txt"
                metadata.write_text(
                    "\n".join(
                        [
                            "schema=2",
                            f"reference={reference}",
                            "source_revision=" + ("c" * 40 if reference == "java_i2p" else "d" * 40),
                            "source_repository=https://example.invalid/source.git",
                            "lock_sha256=" + hashlib.sha256(lock.read_bytes()).hexdigest(),
                            "build_command_version=test",
                            "host_contract=ubuntu-24.04-amd64",
                            f"artifact_sha256={artifact_sha256}",
                            "artifact_path=bin/launcher",
                            "installed_tree_sha256=PLACEHOLDER",
                            "launcher=bin/launcher",
                            "execution_network=forbidden",
                            "toolchain=test",
                            "launcher_probe=test",
                            "version_check=test",
                            "test_disposition=not-available",
                        ]
                    )
                    + "\n",
                    encoding="utf-8",
                )
                tree_hash = hash_runtime_tree(cache)
                metadata.write_text(
                    metadata.read_text(encoding="utf-8").replace("PLACEHOLDER", tree_hash),
                    encoding="utf-8",
                )
                references.append(
                    {
                        "reference": reference,
                        "cache_key": key,
                        "metadata": f"target/interop/cache/{reference}/{key}/build-metadata.txt",
                        "source_revision": "c" * 40 if reference == "java_i2p" else "d" * 40,
                        "build_command_version": "test",
                        "artifact_sha256": artifact_sha256,
                        "installed_tree_sha256": tree_hash,
                    }
                )
            cache_root.mkdir(parents=True, exist_ok=True)
            summary = {
                "schema": 2,
                "host_contract": "ubuntu-24.04-amd64",
                "lock_sha256": hashlib.sha256(lock.read_bytes()).hexdigest(),
                "references": references,
            }
            (cache_root / "current-cache.json").write_text(json.dumps(summary), encoding="utf-8")
            manifest_path = root / "target/interop/build/reference-cache-manifest.json"
            build_cache_manifest(root, manifest_path)
            validate_cache_manifest(root, manifest_path)
            (cache_root / "i2pd" / ("b" * 64) / "bin/launcher").write_bytes(b"mutated")
            with self.assertRaises(BuildGateError):
                validate_cache_manifest(root, manifest_path)

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
            "expected": "bounded-result",
            "actual_typed_result": "blocked",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 0},
            "cleanup_result": "not-started",
            "evidence_sha256": "",
            "known_deviation": "driver-absent",
            "reproduction": "bash scripts/interop/run-scenario.sh --scenario synthetic --reference i2pd",
            "i2pr_router_info_sha256": "0" * 64,
            "reference_router_info_sha256": "0" * 64,
            "data_phase_mode": "round-trip-delivery-status",
            "expected_observation": "i2pr-sent-and-acknowledged",
            "topology_kind": "privileged-dual-netns-veth",
            "privilege_model": "host-capabilities",
            "sandbox_attestation_sha256": "9" * 64,
            "parent_network_state_unchanged": False,
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
            "expected": "authenticated-handshake-and-bounded-i2np-exchange",
            "actual_typed_result": "passed",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 1, "exited": 1, "forced": 0},
            "cleanup_result": "clean",
            "evidence_sha256": "",
            "known_deviation": "environment-smoke-only",
            "reproduction": "bash scripts/interop/run-scenario.sh --scenario synthetic --reference i2pd",
            "i2pr_router_info_sha256": "5" * 64,
            "reference_router_info_sha256": "6" * 64,
            "data_phase_mode": "round-trip-delivery-status",
            "expected_observation": "i2pr-sent-and-acknowledged",
            "topology_kind": "privileged-dual-netns-veth",
            "privilege_model": "host-capabilities",
            "sandbox_attestation_sha256": "9" * 64,
            "parent_network_state_unchanged": False,
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "record.json"
            write_record(path, base)
            validate_file(path)
            self.assertTrue(path.is_file())
            self.assertFalse((Path(directory) / "secret.log").exists())

    def test_evidence_taxonomy_expected_values_are_accepted(self) -> None:
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
            "expected": "authenticated-handshake-and-bounded-i2np-exchange",
            "actual_typed_result": "blocked",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 0},
            "cleanup_result": "not-started",
            "evidence_sha256": "",
            "known_deviation": "driver-absent",
            "reproduction": "bash scripts/interop/run-scenario.sh --scenario synthetic --reference i2pd",
            "i2pr_router_info_sha256": "0" * 64,
            "reference_router_info_sha256": "0" * 64,
            "data_phase_mode": "round-trip-delivery-status",
            "expected_observation": "i2pr-sent-and-acknowledged",
            "topology_kind": "privileged-dual-netns-veth",
            "privilege_model": "host-capabilities",
            "sandbox_attestation_sha256": "0" * 64,
            "parent_network_state_unchanged": False,
        }
        for expected in (
            "authenticated-handshake-and-bounded-i2np-exchange",
            "authenticated-handshake-and-bounded-i2np-exchange-or-explicit-environment-skip",
            "typed-rejection-with-bounded-cleanup",
            "deterministic-winner-and-loser-drain",
            "bounded-result",
        ):
            base["expected"] = expected
            validate_record(base)

    def test_evidence_rejects_unknown_expected_and_deviation(self) -> None:
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
            "expected": "bounded-result",
            "actual_typed_result": "blocked",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 0},
            "cleanup_result": "not-started",
            "evidence_sha256": "",
            "known_deviation": "driver-absent",
            "reproduction": "bash scripts/interop/run-scenario.sh --scenario synthetic --reference i2pd",
            "i2pr_router_info_sha256": "0" * 64,
            "reference_router_info_sha256": "0" * 64,
            "data_phase_mode": "round-trip-delivery-status",
            "expected_observation": "i2pr-sent-and-acknowledged",
            "topology_kind": "privileged-dual-netns-veth",
            "privilege_model": "host-capabilities",
            "sandbox_attestation_sha256": "0" * 64,
            "parent_network_state_unchanged": False,
        }
        base["expected"] = "arbitrary-free-text"
        with self.assertRaises(EvidenceError):
            validate_record(base)
        base["expected"] = "bounded-result"
        base["known_deviation"] = "arbitrary-free-text"
        with self.assertRaises(EvidenceError):
            validate_record(base)

    def test_evidence_rejects_forbidden_material_in_string_fields(self) -> None:
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
            "expected": "bounded-result",
            "actual_typed_result": "blocked",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 0},
            "cleanup_result": "not-started",
            "evidence_sha256": "",
            "known_deviation": "driver-absent",
            "reproduction": "bash scripts/interop/run-scenario.sh --scenario synthetic --reference i2pd",
            "i2pr_router_info_sha256": "0" * 64,
            "reference_router_info_sha256": "0" * 64,
            "data_phase_mode": "round-trip-delivery-status",
            "expected_observation": "i2pr-sent-and-acknowledged",
            "topology_kind": "privileged-dual-netns-veth",
            "privilege_model": "host-capabilities",
            "sandbox_attestation_sha256": "0" * 64,
            "parent_network_state_unchanged": False,
        }
        for pattern, field in (
            ("-----BEGIN PRIVATE KEY-----", "known_deviation"),
            ("router.identity", "known_deviation"),
            ("ntcp2.static.key", "known_deviation"),
            ("192.168.1.1:45678", "known_deviation"),
            ("/home/user/data", "known_deviation"),
            ("/root/.ssh", "known_deviation"),
            ("capture.pcap", "known_deviation"),
            ("capture.pcapng", "known_deviation"),
            ("RouterInfo-data", "known_deviation"),
            ("a" * 80, "known_deviation"),
        ):
            base[field] = pattern
            with self.assertRaises(EvidenceError):
                validate_record(base)
            base[field] = "driver-absent"

    def test_evidence_rejects_forbidden_material_in_parameters(self) -> None:
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
            "expected": "bounded-result",
            "actual_typed_result": "blocked",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 0},
            "cleanup_result": "not-started",
            "evidence_sha256": "",
            "known_deviation": "driver-absent",
            "reproduction": "bash scripts/interop/run-scenario.sh --scenario synthetic --reference i2pd",
            "i2pr_router_info_sha256": "0" * 64,
            "reference_router_info_sha256": "0" * 64,
            "data_phase_mode": "round-trip-delivery-status",
            "expected_observation": "i2pr-sent-and-acknowledged",
            "topology_kind": "privileged-dual-netns-veth",
            "privilege_model": "host-capabilities",
            "sandbox_attestation_sha256": "0" * 64,
            "parent_network_state_unchanged": False,
        }
        base["deterministic_parameters"] = "seed=1;path=/home/user/state"
        with self.assertRaises(EvidenceError):
            validate_record(base)
        base["deterministic_parameters"] = "seed=1"

    def test_evidence_rejects_invalid_reproduction_format(self) -> None:
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
            "expected": "bounded-result",
            "actual_typed_result": "blocked",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 0},
            "cleanup_result": "not-started",
            "evidence_sha256": "",
            "known_deviation": "driver-absent",
            "reproduction": "bash scripts/interop/run-scenario.sh --scenario synthetic --reference i2pd",
            "i2pr_router_info_sha256": "0" * 64,
            "reference_router_info_sha256": "0" * 64,
            "data_phase_mode": "round-trip-delivery-status",
            "expected_observation": "i2pr-sent-and-acknowledged",
            "topology_kind": "privileged-dual-netns-veth",
            "privilege_model": "host-capabilities",
            "sandbox_attestation_sha256": "0" * 64,
            "parent_network_state_unchanged": False,
        }
        base["reproduction"] = "arbitrary-command"
        with self.assertRaises(EvidenceError):
            validate_record(base)
        base["reproduction"] = "bash scripts/interop/run-scenario.sh --scenario synthetic --reference java_i2p"
        validate_record(base)

    def test_evidence_passed_record_rejects_zero_hashes(self) -> None:
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
            "deterministic_parameters": "seed=1",
            "expected": "authenticated-handshake-and-bounded-i2np-exchange",
            "actual_typed_result": "passed",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 1, "exited": 1, "forced": 0},
            "cleanup_result": "clean",
            "evidence_sha256": "",
            "known_deviation": "environment-smoke-only",
            "reproduction": "bash scripts/interop/run-scenario.sh --scenario synthetic --reference i2pd",
            "i2pr_router_info_sha256": "5" * 64,
            "reference_router_info_sha256": "6" * 64,
            "data_phase_mode": "round-trip-delivery-status",
            "expected_observation": "i2pr-sent-and-acknowledged",
            "topology_kind": "privileged-dual-netns-veth",
            "privilege_model": "host-capabilities",
            "sandbox_attestation_sha256": "9" * 64,
            "parent_network_state_unchanged": False,
        }
        base["artifact_sha256"] = "0" * 64
        with self.assertRaises(EvidenceError):
            validate_record(base)
        base["artifact_sha256"] = "1" * 64
        validate_record(base)

    def test_evidence_passed_record_rejects_placeholder_commit(self) -> None:
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
            "deterministic_parameters": "seed=1",
            "expected": "authenticated-handshake-and-bounded-i2np-exchange",
            "actual_typed_result": "passed",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 1, "exited": 1, "forced": 0},
            "cleanup_result": "clean",
            "evidence_sha256": "",
            "known_deviation": "environment-smoke-only",
            "reproduction": "bash scripts/interop/run-scenario.sh --scenario synthetic --reference i2pd",
            "i2pr_router_info_sha256": "5" * 64,
            "reference_router_info_sha256": "6" * 64,
            "data_phase_mode": "round-trip-delivery-status",
            "expected_observation": "i2pr-sent-and-acknowledged",
            "topology_kind": "privileged-dual-netns-veth",
            "privilege_model": "host-capabilities",
            "sandbox_attestation_sha256": "9" * 64,
            "parent_network_state_unchanged": False,
        }
        base["i2pr_commit"] = "record-at-execution"
        with self.assertRaises(EvidenceError):
            validate_record(base)

    def test_evidence_finalization_failure_is_not_mislabeled_as_cleanup(self) -> None:
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
            "deterministic_parameters": "seed=1",
            "expected": "authenticated-handshake-and-bounded-i2np-exchange",
            "actual_typed_result": "passed",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 1, "exited": 1, "forced": 0},
            "cleanup_result": "clean",
            "evidence_sha256": "",
            "known_deviation": "environment-smoke-only",
            "reproduction": "bash scripts/interop/run-scenario.sh --scenario synthetic --reference i2pd",
            "i2pr_router_info_sha256": "5" * 64,
            "reference_router_info_sha256": "6" * 64,
            "data_phase_mode": "round-trip-delivery-status",
            "expected_observation": "i2pr-sent-and-acknowledged",
            "topology_kind": "privileged-dual-netns-veth",
            "privilege_model": "host-capabilities",
            "sandbox_attestation_sha256": "9" * 64,
            "parent_network_state_unchanged": False,
        }
        evidence_only = dict(base)
        evidence_only["actual_typed_result"] = "blocked"
        evidence_only["known_deviation"] = "evidence-finalization-failed"
        evidence_only["cleanup_result"] = "clean"
        validate_record(evidence_only)
        self.assertNotEqual(evidence_only["actual_typed_result"], "failed_cleanup")
        cleanup_failed = dict(base)
        cleanup_failed["actual_typed_result"] = "failed_cleanup"
        cleanup_failed["cleanup_result"] = "failed"
        cleanup_failed["known_deviation"] = "cleanup-verification-failed"
        validate_record(cleanup_failed)
        self.assertEqual(cleanup_failed["actual_typed_result"], "failed_cleanup")
        self.assertEqual(cleanup_failed["cleanup_result"], "failed")

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
            far_outside = Path("/run/lock") / "i2pr-plan-041-outside.info"
            try:
                far_outside.write_bytes(b"not-a-router-info")
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
                    java.import_peer_router_info(far_outside)
                with self.assertRaises(I2pdError) as i2pd_error:
                    i2pd.import_peer_router_info(far_outside)
                self.assertEqual(java_error.exception.code, "peer-router-info-outside-run-root")
                self.assertEqual(i2pd_error.exception.code, "peer-router-info-outside-run-root")
            finally:
                if far_outside.exists():
                    far_outside.unlink()

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


class SequentialGateArchivalTests(unittest.TestCase):
    def _make_record(self, scenario_id: str, reference: str = "i2pd") -> dict:
        return {
            "schema": 1, "scenario_id": scenario_id,
            "date_utc": "2026-01-01T00:00:00Z",
            "i2pr_commit": "a" * 40 + ";clean",
            "reference": reference,
            "reference_version": "2.12.0" if reference == "java_i2p" else "2.60.0",
            "reference_revision": "f" * 40,
            "artifact_sha256": "1" * 64,
            "installed_tree_sha256": "2" * 64,
            "configuration_sha256": "3" * 64,
            "namespace_topology_sha256": "4" * 64,
            "direction": "both",
            "address_family": "ipv4",
            "deterministic_parameters": "seed=1;timeouts=bounded",
            "expected": "authenticated-handshake-and-bounded-i2np-exchange",
            "actual_typed_result": "blocked",
            "resource_counters": {"tasks": 0},
            "process_counters": {"started": 0},
            "cleanup_result": "not-started",
            "evidence_sha256": "",
            "known_deviation": "environment-smoke-only",
            "reproduction": f"bash scripts/interop/run-scenario.sh --scenario {scenario_id} --reference {reference}",
            "i2pr_router_info_sha256": "5" * 64,
            "reference_router_info_sha256": "6" * 64,
            "data_phase_mode": "round-trip-delivery-status",
            "expected_observation": "i2pr-sent-and-acknowledged",
            "topology_kind": "privileged-dual-netns-veth",
            "privilege_model": "host-capabilities",
            "sandbox_attestation_sha256": "0" * 64,
            "parent_network_state_unchanged": False,
        }

    def _write_record(self, evidence_dir: Path, filename: str, record: dict) -> Path:
        path = evidence_dir / filename
        write_record(path, record)
        return path

    def _sha256(self, path: Path) -> str:
        import hashlib
        return hashlib.sha256(path.read_bytes()).hexdigest()

    def test_sequential_gates_preserve_attribution_and_digests(self) -> None:
        from build_gate import PROFILE_GATES
        gates = ("environment-smoke", "reference-crosscheck-ipv4", "handshake-smoke", "full")
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = Path(directory) / "evidence"
            evidence_dir.mkdir()

            gate_files: dict[str, list[Path]] = {}
            gate_digests: dict[str, dict[str, str]] = {}

            env_smoke_scenarios = [
                ("java-ipv4-inbound-outbound", "java_i2p"),
                ("i2pd-ipv4-inbound-outbound", "i2pd"),
            ]
            ref_cross_scenarios = [
                ("reference-java-i2pd-ipv4", "java_i2p"),
                ("reference-i2pd-java-ipv4", "i2pd"),
            ]
            hs_smoke_scenarios = [
                ("java-ipv4-inbound-outbound", "java_i2p"),
                ("i2pd-ipv4-inbound-outbound", "i2pd"),
            ]
            full_scenarios = [
                ("java-ipv4-inbound-outbound", "java_i2p"),
                ("java-ipv6-inbound-outbound", "java_i2p"),
                ("i2pd-ipv4-inbound-outbound", "i2pd"),
                ("i2pd-ipv6-inbound-outbound", "i2pd"),
            ]

            gate_scenario_map = {
                "environment-smoke": env_smoke_scenarios,
                "reference-crosscheck-ipv4": ref_cross_scenarios,
                "handshake-smoke": hs_smoke_scenarios,
                "full": full_scenarios,
            }

            for gate in gates:
                gate_files[gate] = []
                gate_digests[gate] = {}
                for scenario_id, ref in gate_scenario_map[gate]:
                    record = self._make_record(scenario_id, ref)
                    raw_filename = f"run-test-{gate}-{scenario_id}-{ref}.json"
                    staging_dir = evidence_dir / "staging" / gate
                    staging_dir.mkdir(parents=True, exist_ok=True)
                    path = self._write_record(staging_dir, raw_filename, record)
                    dest = evidence_dir / f"{gate}--{raw_filename}"
                    path.rename(dest)
                    gate_files[gate].append(dest)
                    gate_digests[gate][dest.name] = self._sha256(dest)

            for gate in gates:
                for f in gate_files[gate]:
                    self.assertTrue(f.exists(), f"{f.name} should exist after gate {gate}")
                    self.assertEqual(self._sha256(f), gate_digests[gate][f.name],
                                     f"digest of {f.name} changed after gate {gate}")

            self.assertEqual(len(list(evidence_dir.glob("*.json"))),
                             sum(len(v) for v in gate_files.values()))

    def test_gate_rejects_filename_collision(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = Path(directory) / "evidence"
            evidence_dir.mkdir()
            record = self._make_record("java-ipv4-inbound-outbound")
            existing_path = self._write_record(evidence_dir, "handshake-smoke--run-existing.json", record)
            original_digest = self._sha256(existing_path)
            staging_dir = evidence_dir / "staging" / "handshake-smoke"
            staging_dir.mkdir(parents=True)
            self._write_record(staging_dir, "run-existing.json", record)
            dest = evidence_dir / "handshake-smoke--run-existing.json"
            self.assertTrue(dest.exists())
            self.assertEqual(self._sha256(dest), original_digest,
                             "collision destination already exists with correct content")
            self.assertEqual(
                f"handshake-smoke--run-existing.json",
                dest.name,
            )

    def test_gate_cannot_modify_earlier_record(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = Path(directory) / "evidence"
            evidence_dir.mkdir()
            record = self._make_record("java-ipv4-inbound-outbound")
            path = self._write_record(evidence_dir, "environment-smoke--run-test.json", record)
            original_digest = self._sha256(path)
            modified_record = self._make_record("java-ipv4-inbound-outbound")
            modified_record["known_deviation"] = "driver-absent"
            modified_record["evidence_sha256"] = ""
            staging_dir = evidence_dir / "staging" / "handshake-smoke"
            staging_dir.mkdir(parents=True)
            self._write_record(staging_dir, "run-test-modified.json", modified_record)
            dest = evidence_dir / "handshake-smoke--run-test-modified.json"
            (staging_dir / "run-test-modified.json").rename(dest)
            self.assertEqual(self._sha256(path), original_digest)

    def test_gate_preserves_earlier_records_after_archival(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = Path(directory) / "evidence"
            evidence_dir.mkdir()
            env_record = self._make_record("java-ipv4-inbound-outbound")
            env_path = self._write_record(evidence_dir, "environment-smoke--run-env.json", env_record)
            env_digest = self._sha256(env_path)
            hs_record = self._make_record("java-ipv4-inbound-outbound")
            staging_dir = evidence_dir / "staging" / "handshake-smoke"
            staging_dir.mkdir(parents=True)
            self._write_record(staging_dir, "run-hs.json", hs_record)
            hs_dest = evidence_dir / "handshake-smoke--run-hs.json"
            (staging_dir / "run-hs.json").rename(hs_dest)
            self.assertEqual(self._sha256(env_path), env_digest,
                             "earlier gate record was modified during archival")
            self.assertTrue(env_path.exists())

    def test_staging_directory_cleaned_after_successful_gate(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            evidence_dir = Path(directory) / "evidence"
            evidence_dir.mkdir()
            staging_dir = evidence_dir / "staging" / "test-gate"
            staging_dir.mkdir(parents=True)
            record = self._make_record("java-ipv4-inbound-outbound")
            self._write_record(staging_dir, "run-test.json", record)
            self.assertTrue(staging_dir.exists())
            self.assertTrue(any(staging_dir.iterdir()))
            import shutil
            shutil.rmtree(staging_dir)
            self.assertFalse(staging_dir.exists())


class StaticWorkflowContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self._workflow = (ROOT / ".github/workflows/ntcp2-interop-ubuntu.yml").read_text(encoding="utf-8")

    def test_workflow_requires_rustfmt_component(self) -> None:
        self.assertIn("--component rustfmt", self._workflow)

    def test_workflow_requires_clippy_component(self) -> None:
        self.assertIn("--component clippy", self._workflow)

    def test_workflow_rejects_ubuntu_latest(self) -> None:
        self.assertNotIn("ubuntu-latest", self._workflow)

    def test_workflow_rejects_moving_action_references(self) -> None:
        self.assertIsNone(re.search(r"uses:\s+[^\s]+@(master|main|latest)\b", self._workflow))

    def test_workflow_uses_locked_cargo_check(self) -> None:
        self.assertIn("cargo +1.95.0 check --locked", self._workflow)

    def test_workflow_uses_locked_cargo_test(self) -> None:
        self.assertIn("cargo +1.95.0 test --locked", self._workflow)

    def test_workflow_uses_locked_cargo_clippy(self) -> None:
        self.assertIn("cargo +1.95.0 clippy --locked", self._workflow)

    def test_workflow_uses_locked_cargo_doc(self) -> None:
        self.assertIn("cargo +1.95.0 doc --locked", self._workflow)

    def test_workflow_records_toolchain_versions(self) -> None:
        self.assertIn("rustc +1.95.0 --version --verbose", self._workflow)
        self.assertIn("cargo +1.95.0 --version --verbose", self._workflow)

    def test_workflow_rejects_unbounded_inputs(self) -> None:
        if "workflow_dispatch:" in self._workflow and "inputs:" in self._workflow:
            dispatch_block = self._workflow.split("workflow_dispatch:", 1)[1]
            section = dispatch_block.split("\n\n", 1)[0] if "\n\n" in dispatch_block else dispatch_block
            if "inputs:" in section:
                inputs_block = section.split("inputs:", 1)[1]
                input_keys = re.findall(r"^      (\w[\w-]*):", inputs_block, re.MULTILINE)
                approved = {"profile"}
                for key in input_keys:
                    self.assertIn(key, approved, f"unbounded input introduced: {key}")


class JavaRouterInfoExportTests(unittest.TestCase):
    def _make_adapter(self, root: Path) -> JavaI2pAdapter:
        from topology import EndpointDescription
        endpoint = EndpointDescription(
            local_address="192.0.2.1", peer_address="192.0.2.2",
            local_port=45678, peer_port=45679,
            address_family="ipv4", namespace="test", network_id="99",
        )
        return JavaI2pAdapter(root / "cache", root / "run", endpoint, ROOT)

    def test_reference_data_router_info_is_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            adapter = self._make_adapter(root)
            adapter.data_dir.mkdir(parents=True)
            ri = adapter.data_dir / "router.info"
            ri.write_bytes(b"valid-router-info-bytes")
            result = adapter.export_router_info()
            self.assertTrue(result.is_file())
            self.assertIn("exchange", str(result))
            self.assertEqual(result.read_bytes(), b"valid-router-info-bytes")

    def test_reference_data_router_info_su3_is_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            adapter = self._make_adapter(root)
            adapter.data_dir.mkdir(parents=True)
            ri = adapter.data_dir / "router.info.su3"
            ri.write_bytes(b"valid-su3-bytes")
            result = adapter.export_router_info()
            self.assertTrue(result.is_file())
            self.assertEqual(result.read_bytes(), b"valid-su3-bytes")

    def test_file_only_in_reference_runtime_is_not_accepted(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            adapter = self._make_adapter(root)
            adapter.runtime_dir.mkdir(parents=True)
            (adapter.runtime_dir / "router.info").write_bytes(b"wrong-dir")
            with self.assertRaises(JavaI2pError) as ctx:
                adapter.export_router_info()
            self.assertEqual(ctx.exception.code, "router-info-not-produced")

    def test_symlink_candidate_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            adapter = self._make_adapter(root)
            adapter.data_dir.mkdir(parents=True)
            real = adapter.data_dir / "real.info"
            real.write_bytes(b"real")
            link = adapter.data_dir / "router.info"
            link.symlink_to(real)
            with self.assertRaises(JavaI2pError) as ctx:
                adapter.export_router_info()
            self.assertEqual(ctx.exception.code, "router-info-symlink-rejected")

    def test_path_escape_candidate_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            adapter = self._make_adapter(root)
            adapter.data_dir.mkdir(parents=True)
            real = adapter.data_dir / "real.info"
            real.write_bytes(b"real")
            escaped = root / "escaped" / "router.info"
            escaped.parent.mkdir(parents=True)
            escaped.write_bytes(b"escaped")
            adapter.data_dir.joinpath("router.info").symlink_to(escaped)
            with self.assertRaises(JavaI2pError) as ctx:
                adapter.export_router_info()
            self.assertEqual(ctx.exception.code, "router-info-symlink-rejected")

    def test_exported_copy_remains_inside_exchange_directory(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            adapter = self._make_adapter(root)
            adapter.data_dir.mkdir(parents=True)
            (adapter.data_dir / "router.info").write_bytes(b"inside-check")
            result = adapter.export_router_info()
            self.assertTrue(adapter._inside_run_root(result))
            self.assertIn("exchange", result.parts)

    def test_empty_file_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            adapter = self._make_adapter(root)
            adapter.data_dir.mkdir(parents=True)
            (adapter.data_dir / "router.info").write_bytes(b"")
            with self.assertRaises(JavaI2pError) as ctx:
                adapter.export_router_info()
            self.assertEqual(ctx.exception.code, "router-info-empty")

    def test_oversized_file_is_rejected(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            adapter = self._make_adapter(root)
            adapter.data_dir.mkdir(parents=True)
            (adapter.data_dir / "router.info").write_bytes(b"x" * 1_048_577)
            with self.assertRaises(JavaI2pError) as ctx:
                adapter.export_router_info()
            self.assertEqual(ctx.exception.code, "router-info-oversized")


class MixedScenarioTests(unittest.TestCase):
    def test_all_four_directional_scenarios_exist(self) -> None:
        mixed_dir = ROOT / "tests/integration/ntcp2/mixed-scenarios"
        expected_ids = {
            "i2pr-to-java-ipv4",
            "java-to-i2pr-ipv4",
            "i2pr-to-i2pd-ipv4",
            "i2pd-to-i2pr-ipv4",
        }
        files = {path.stem for path in mixed_dir.glob("[!.]*.toml") if path.name != "manifest.toml"}
        self.assertEqual(files, expected_ids)
        manifest = tomllib.loads((mixed_dir / "manifest.toml").read_text(encoding="utf-8"))
        manifest_ids = {item["id"] for item in manifest["scenario"]}
        self.assertEqual(manifest_ids, expected_ids)

    def test_mixed_scenarios_match_manifest(self) -> None:
        from mixed_runner import load_mixed_scenario
        mixed_dir = ROOT / "tests/integration/ntcp2/mixed-scenarios"
        manifest = tomllib.loads((mixed_dir / "manifest.toml").read_text(encoding="utf-8"))
        for item in manifest["scenario"]:
            direction = load_mixed_scenario(ROOT, item["id"])
            self.assertEqual(direction.execution_id, item["id"])
            self.assertEqual(direction.reference, item["reference"])
            self.assertEqual(direction.initiator, item["initiator"])
            self.assertEqual(direction.responder, item["responder"])

    def test_mixed_scenario_direction_invariants(self) -> None:
        from mixed_runner import load_mixed_scenario
        for scenario_id in ("i2pr-to-java-ipv4", "i2pr-to-i2pd-ipv4"):
            d = load_mixed_scenario(ROOT, scenario_id)
            self.assertTrue(d.i2pr_is_initiator)
            self.assertEqual(d.direction, "i2pr-to-reference")
        for scenario_id in ("java-to-i2pr-ipv4", "i2pd-to-i2pr-ipv4"):
            d = load_mixed_scenario(ROOT, scenario_id)
            self.assertFalse(d.i2pr_is_initiator)
            self.assertEqual(d.direction, "reference-to-i2pr")

    def test_mixed_scenario_references_match(self) -> None:
        from mixed_runner import load_mixed_scenario
        d1 = load_mixed_scenario(ROOT, "i2pr-to-java-ipv4")
        self.assertEqual(d1.reference, "java_i2p")
        d2 = load_mixed_scenario(ROOT, "java-to-i2pr-ipv4")
        self.assertEqual(d2.reference, "java_i2p")
        d3 = load_mixed_scenario(ROOT, "i2pr-to-i2pd-ipv4")
        self.assertEqual(d3.reference, "i2pd")
        d4 = load_mixed_scenario(ROOT, "i2pd-to-i2pr-ipv4")
        self.assertEqual(d4.reference, "i2pd")


class LauncherRendererTests(unittest.TestCase):
    def test_render_valid_initiator_scenario(self) -> None:
        from launcher_renderer import render_scenario_toml, RenderError
        content = render_scenario_toml(
            execution_id="i2pr-to-java-ipv4",
            role="initiator",
            address_family="ipv4",
            local_address="192.0.2.1",
            local_port=45680,
            peer_address="192.0.2.2",
            peer_port=45678,
            state_dir="state",
            peer_router_info="exchange/peer.info",
        )
        self.assertIn('role = "initiator"', content)
        self.assertIn('peer_address = "192.0.2.2"', content)
        self.assertIn("peer_port = 45678", content)
        self.assertIn("network_id = 99", content)

    def test_render_valid_responder_scenario(self) -> None:
        from launcher_renderer import render_scenario_toml
        content = render_scenario_toml(
            execution_id="java-to-i2pr-ipv4",
            role="responder",
            address_family="ipv4",
            local_address="192.0.2.1",
            local_port=45680,
            peer_address=None,
            peer_port=None,
            state_dir="state",
            peer_router_info=None,
        )
        self.assertIn('role = "responder"', content)
        self.assertIn('peer_address = ""', content)
        self.assertIn("peer_port = 0", content)
        self.assertIn('peer_router_info = ""', content)

    def test_render_rejects_absolute_path(self) -> None:
        from launcher_renderer import render_scenario_toml, RenderError
        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="test", role="responder", address_family="ipv4",
                local_address="192.0.2.1", local_port=45680,
                peer_address=None, peer_port=None,
                state_dir="/etc/state", peer_router_info=None,
            )

    def test_render_rejects_parent_traversal(self) -> None:
        from launcher_renderer import render_scenario_toml, RenderError
        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="test", role="responder", address_family="ipv4",
                local_address="192.0.2.1", local_port=45680,
                peer_address=None, peer_port=None,
                state_dir="../escape", peer_router_info=None,
            )

    def test_render_rejects_address_outside_synthetic_range(self) -> None:
        from launcher_renderer import render_scenario_toml, RenderError
        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="test", role="responder", address_family="ipv4",
                local_address="10.0.0.1", local_port=45680,
                peer_address=None, peer_port=None,
                state_dir="state", peer_router_info=None,
            )

    def test_render_rejects_mismatched_address_family(self) -> None:
        from launcher_renderer import render_scenario_toml, RenderError
        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="test", role="responder", address_family="ipv4",
                local_address="2001:db8:36::1", local_port=45680,
                peer_address=None, peer_port=None,
                state_dir="state", peer_router_info=None,
            )

    def test_render_rejects_missing_peer_for_initiator(self) -> None:
        from launcher_renderer import render_scenario_toml, RenderError
        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="test", role="initiator", address_family="ipv4",
                local_address="192.0.2.1", local_port=45680,
                peer_address=None, peer_port=None,
                state_dir="state", peer_router_info="exchange/peer.info",
            )

    def test_render_rejects_peer_for_responder(self) -> None:
        from launcher_renderer import render_scenario_toml, RenderError
        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="test", role="responder", address_family="ipv4",
                local_address="192.0.2.1", local_port=45680,
                peer_address="192.0.2.2", peer_port=45678,
                state_dir="state", peer_router_info=None,
            )

    def test_render_rejects_unsupported_padding(self) -> None:
        from launcher_renderer import render_scenario_toml, RenderError
        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="test", role="responder", address_family="ipv4",
                local_address="192.0.2.1", local_port=45680,
                peer_address=None, peer_port=None,
                state_dir="state", peer_router_info=None,
                padding_profile="arbitrary",
            )

    def test_render_rejects_unsupported_smoke_profile(self) -> None:
        from launcher_renderer import render_scenario_toml, RenderError
        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="test", role="responder", address_family="ipv4",
                local_address="192.0.2.1", local_port=45680,
                peer_address=None, peer_port=None,
                state_dir="state", peer_router_info=None,
                smoke_message_profile="arbitrary",
            )

    def test_render_rejects_duplicate_endpoint(self) -> None:
        from launcher_renderer import render_scenario_toml, RenderError
        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="test", role="initiator", address_family="ipv4",
                local_address="192.0.2.1", local_port=45680,
                peer_address="192.0.2.1", peer_port=45680,
                state_dir="state", peer_router_info="exchange/peer.info",
            )

    def test_render_rejects_unsupported_expected_result(self) -> None:
        from launcher_renderer import render_scenario_toml, RenderError
        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="test", role="responder", address_family="ipv4",
                local_address="192.0.2.1", local_port=45680,
                peer_address=None, peer_port=None,
                state_dir="state", peer_router_info=None,
                expected_result_class="arbitrary-result",
            )

    def test_render_and_validate_writes_valid_toml(self) -> None:
        from launcher_renderer import render_and_validate
        with tempfile.TemporaryDirectory() as directory:
            run_root = Path(directory) / "run"
            path = render_and_validate(
                run_root,
                execution_id="i2pr-to-java-ipv4",
                role="responder",
                address_family="ipv4",
                local_address="192.0.2.1",
                local_port=45680,
                peer_address=None,
                peer_port=None,
                state_dir="state",
                peer_router_info=None,
            )
            self.assertTrue(path.is_file())
            loaded = load_launcher_scenario(path)
            self.assertEqual(loaded.scenario_id, "i2pr-to-java-ipv4")
            self.assertEqual(loaded.role, "responder")


class MixedRunnerTerminalStatusTests(unittest.TestCase):
    def test_listener_ready_must_come_before_terminal(self) -> None:
        from i2pr import I2prAdapter
        with tempfile.TemporaryDirectory() as directory:
            run_root = Path(directory)
            adapter = I2prAdapter(ROOT, run_root, "test-ns")
            with self.assertRaises(RuntimeError) as ctx:
                adapter.wait_ready()
            self.assertIn("not-started", str(ctx.exception))

    def test_wait_terminal_requires_started(self) -> None:
        from i2pr import I2prAdapter
        with tempfile.TemporaryDirectory() as directory:
            run_root = Path(directory)
            adapter = I2prAdapter(ROOT, run_root, "test-ns")
            with self.assertRaises(RuntimeError) as ctx:
                adapter.wait_terminal()
            self.assertIn("not-started", str(ctx.exception))

    def test_parse_status_line_rejects_unknown_reason(self) -> None:
        with self.assertRaises(LauncherStatusError):
            parse_status_line(
                '{"schema":1,"type":"i2pr-interop-status","scenario_id":"test",'
                '"phase":"terminal","result":"rejected",'
                '"reason_code":"arbitrary-reason",'
                '"counters":{"listener_ready":0,"authenticated":0,"frames_sent":0,'
                '"frames_received":0,"i2np_sent":0,"i2np_received":0}}'
            )

    def test_parse_status_line_rejects_multi_terminal(self) -> None:
        with self.assertRaises(LauncherStatusError):
            parse_status_line(
                '{"schema":1,"type":"i2pr-interop-status","scenario_id":"test",'
                '"phase":"terminal","result":"ready",'
                '"reason_code":"listener_bound",'
                '"counters":{"listener_ready":0,"authenticated":0,"frames_sent":0,'
                '"frames_received":0,"i2np_sent":0,"i2np_received":0}}'
            )

    def test_parse_status_line_rejects_unknown_counter(self) -> None:
        with self.assertRaises(LauncherStatusError):
            parse_status_line(
                '{"schema":1,"type":"i2pr-interop-status","scenario_id":"test",'
                '"phase":"terminal","result":"rejected",'
                '"reason_code":"handshake_failed",'
                '"counters":{"listener_ready":0,"authenticated":0,"frames_sent":0,'
                '"frames_received":0,"i2np_sent":0,"i2np_received":0,"unknown":1}}'
            )

    def test_parse_status_line_rejects_extra_json_fields(self) -> None:
        with self.assertRaises(LauncherStatusError):
            parse_status_line(
                '{"schema":1,"type":"i2pr-interop-status","scenario_id":"test",'
                '"phase":"terminal","result":"rejected",'
                '"reason_code":"handshake_failed",'
                '"counters":{"listener_ready":0,"authenticated":0,"frames_sent":0,'
                '"frames_received":0,"i2np_sent":0,"i2np_received":0},'
                '"extra_field":"value"}'
            )


class MixedDirectionCoverageTests(unittest.TestCase):
    def test_each_direction_has_distinct_execution_id(self) -> None:
        from mixed_runner import load_mixed_scenario
        ids = set()
        for scenario_id in ("i2pr-to-java-ipv4", "java-to-i2pr-ipv4", "i2pr-to-i2pd-ipv4", "i2pd-to-i2pr-ipv4"):
            d = load_mixed_scenario(ROOT, scenario_id)
            self.assertNotIn(d.execution_id, ids)
            ids.add(d.execution_id)

    def test_each_direction_has_one_initiator_one_responder(self) -> None:
        from mixed_runner import load_mixed_scenario
        for scenario_id in ("i2pr-to-java-ipv4", "java-to-i2pr-ipv4", "i2pr-to-i2pd-ipv4", "i2pd-to-i2pr-ipv4"):
            d = load_mixed_scenario(ROOT, scenario_id)
            self.assertIn(d.initiator, {"i2pr", "java_i2p", "i2pd"})
            self.assertIn(d.responder, {"i2pr", "java_i2p", "i2pd"})
            self.assertNotEqual(d.initiator, d.responder)

    def test_each_direction_has_expected_result(self) -> None:
        from mixed_runner import load_mixed_scenario
        for scenario_id in ("i2pr-to-java-ipv4", "java-to-i2pr-ipv4", "i2pr-to-i2pd-ipv4", "i2pd-to-i2pr-ipv4"):
            d = load_mixed_scenario(ROOT, scenario_id)
            self.assertEqual(d.expected, "authenticated-handshake-and-bounded-i2np-exchange")

    def test_java_directions_cover_both_initiator_and_responder(self) -> None:
        from mixed_runner import load_mixed_scenario
        d1 = load_mixed_scenario(ROOT, "i2pr-to-java-ipv4")
        d2 = load_mixed_scenario(ROOT, "java-to-i2pr-ipv4")
        self.assertEqual(d1.initiator, "i2pr")
        self.assertEqual(d1.responder, "java_i2p")
        self.assertEqual(d2.initiator, "java_i2p")
        self.assertEqual(d2.responder, "i2pr")

    def test_i2pd_directions_cover_both_initiator_and_responder(self) -> None:
        from mixed_runner import load_mixed_scenario
        d1 = load_mixed_scenario(ROOT, "i2pr-to-i2pd-ipv4")
        d2 = load_mixed_scenario(ROOT, "i2pd-to-i2pr-ipv4")
        self.assertEqual(d1.initiator, "i2pr")
        self.assertEqual(d1.responder, "i2pd")
        self.assertEqual(d2.initiator, "i2pd")
        self.assertEqual(d2.responder, "i2pr")


class DataPhaseOracleTests(unittest.TestCase):
    def test_java_oracle_probe_returns_supported(self) -> None:
        from data_oracle import JavaDataPhaseOracle, OracleKind
        oracle = JavaDataPhaseOracle()
        result = oracle.probe()
        self.assertTrue(result.supported)
        self.assertEqual(result.kind, OracleKind.JAVA_SEND_ONLY)

    def test_i2pd_oracle_probe_returns_supported(self) -> None:
        from data_oracle import I2pdDataPhaseOracle, OracleKind
        oracle = I2pdDataPhaseOracle()
        result = oracle.probe()
        self.assertTrue(result.supported)
        self.assertEqual(result.kind, OracleKind.I2PD_SEND_ONLY)

    def test_mixed_oracle_for_java_i2pr_initiator(self) -> None:
        from data_oracle import MixedDataPhaseOracle, OracleKind
        oracle = MixedDataPhaseOracle("java_i2p", True)
        result = oracle.probe()
        self.assertTrue(result.supported)
        self.assertEqual(result.kind, OracleKind.MIXED_SPLIT)
        self.assertIn("i2pr->Java", result.description)

    def test_mixed_oracle_for_java_reference_initiator(self) -> None:
        from data_oracle import MixedDataPhaseOracle, OracleKind
        oracle = MixedDataPhaseOracle("java_i2p", False)
        result = oracle.probe()
        self.assertTrue(result.supported)
        self.assertEqual(result.kind, OracleKind.MIXED_SPLIT)
        self.assertIn("Java->i2pr", result.description)

    def test_mixed_oracle_for_i2pd_i2pr_initiator(self) -> None:
        from data_oracle import MixedDataPhaseOracle, OracleKind
        oracle = MixedDataPhaseOracle("i2pd", True)
        result = oracle.probe()
        self.assertTrue(result.supported)
        self.assertIn("i2pr->i2pd", result.description)

    def test_mixed_oracle_for_i2pd_reference_initiator(self) -> None:
        from data_oracle import MixedDataPhaseOracle, OracleKind
        oracle = MixedDataPhaseOracle("i2pd", False)
        result = oracle.probe()
        self.assertTrue(result.supported)
        self.assertIn("i2pd->i2pr", result.description)

    def test_select_oracle_returns_mixed_for_known_reference(self) -> None:
        from data_oracle import select_oracle, MixedDataPhaseOracle
        oracle = select_oracle("java_i2p", True)
        self.assertIsInstance(oracle, MixedDataPhaseOracle)

    def test_select_oracle_returns_unsupported_for_unknown(self) -> None:
        from data_oracle import select_oracle, _UnsupportedOracle
        oracle = select_oracle("unknown_ref", True)
        self.assertIsInstance(oracle, _UnsupportedOracle)
        self.assertFalse(oracle.probe().supported)

    def test_oracle_observe_returns_pending_state(self) -> None:
        from data_oracle import select_oracle
        oracle = select_oracle("i2pd", False)
        obs = oracle.observe()
        self.assertFalse(obs.sender_observed)
        self.assertFalse(obs.receiver_observed)

    def test_oracle_rejects_echo_assumption(self) -> None:
        from data_oracle import select_oracle
        for ref in ("java_i2p", "i2pd"):
            for initiator in (True, False):
                oracle = select_oracle(ref, initiator)
                obs = oracle.observe()
                self.assertNotIn("echo", obs.sender_evidence.lower())
                self.assertNotIn("echo", obs.receiver_evidence.lower())


class ReferenceTriggerTests(unittest.TestCase):
    def test_java_trigger_kind(self) -> None:
        from reference_trigger import JavaReferenceTrigger, TriggerKind
        trigger = JavaReferenceTrigger()
        self.assertEqual(trigger.trigger_kind, TriggerKind.JAVA_SAM_DIAL)

    def test_i2pd_trigger_kind(self) -> None:
        from reference_trigger import I2pdReferenceTrigger, TriggerKind
        trigger = I2pdReferenceTrigger()
        self.assertEqual(trigger.trigger_kind, TriggerKind.I2PD_HTTP_DIAL)

    def test_select_trigger_returns_java_for_java(self) -> None:
        from reference_trigger import select_trigger, JavaReferenceTrigger
        trigger = select_trigger("java_i2p")
        self.assertIsInstance(trigger, JavaReferenceTrigger)

    def test_select_trigger_returns_i2pd_for_i2pd(self) -> None:
        from reference_trigger import select_trigger, I2pdReferenceTrigger
        trigger = select_trigger("i2pd")
        self.assertIsInstance(trigger, I2pdReferenceTrigger)

    def test_select_trigger_returns_unsupported_for_unknown(self) -> None:
        from reference_trigger import select_trigger, _UnsupportedTrigger
        trigger = select_trigger("unknown_ref")
        self.assertIsInstance(trigger, _UnsupportedTrigger)

    def test_trigger_results_are_pending(self) -> None:
        from reference_trigger import select_trigger
        for ref in ("java_i2p", "i2pd"):
            trigger = select_trigger(ref)
            auto = trigger.verify_auto_dial()
            self.assertFalse(auto.observed)
            manual = trigger.issue_trigger()
            self.assertFalse(manual.observed)


class NegativeScenarioGuardTests(unittest.TestCase):
    def test_known_positive_scenarios_are_allowed(self) -> None:
        from mixed_runner import _reject_negative_before_primary
        for scenario_id in ("i2pr-to-java-ipv4", "java-to-i2pr-ipv4",
                            "i2pr-to-i2pd-ipv4", "i2pd-to-i2pr-ipv4"):
            _reject_negative_before_primary(scenario_id)

    def test_unknown_scenario_is_rejected(self) -> None:
        from mixed_runner import _reject_negative_before_primary, MixedRunError
        with self.assertRaises(MixedRunError) as ctx:
            _reject_negative_before_primary("negative-malformed-handshake")
        self.assertEqual(ctx.exception.code, "negative-scenario-before-primary-directions")

    def test_another_unknown_scenario_is_rejected(self) -> None:
        from mixed_runner import _reject_negative_before_primary, MixedRunError
        with self.assertRaises(MixedRunError):
            _reject_negative_before_primary("adversarial-replay-injection")


class MixedRunnerOracleIntegrationTests(unittest.TestCase):
    def test_mixed_runner_record_includes_data_phase_oracle(self) -> None:
        from mixed_runner import _record_mixed, MixedDirection
        direction = MixedDirection(
            execution_id="i2pr-to-java-ipv4",
            reference="java_i2p",
            profile="handshake-smoke",
            direction="i2pr-to-reference",
            address_family="ipv4",
            padding="minimum-variable-maximum",
            expected="authenticated-handshake-and-bounded-i2np-exchange",
            initiator="i2pr",
            responder="java_i2p",
        )
        record = _record_mixed(
            direction, "passed", "mixed-router-direction-authenticated",
            "clean", "a" * 40 + ";clean", None, None,
            {"i2pr": "validated-and-bound", "java_i2p": "validated-and-bound"},
            {"i2pr": "authenticated", "java_i2p": "authenticated"},
            {"i2pr": {"started": 1, "exited": 1, "forced": 0},
             "java_i2p": {"started": 1, "exited": 1, "forced": 0}},
            oracle_kind="split-send-and-receive",
            runtime_counters={"handshake_attempts": 1},
        )
        self.assertIn("data_phase_oracle=split-send-and-receive",
                       record["deterministic_parameters"])

    def test_mixed_runner_record_rejects_echo_in_oracle_field(self) -> None:
        from mixed_runner import _record_mixed, MixedDirection
        direction = MixedDirection(
            execution_id="test",
            reference="i2pd",
            profile="handshake-smoke",
            direction="i2pr-to-reference",
            address_family="ipv4",
            padding="representative",
            expected="bounded-result",
            initiator="i2pr",
            responder="i2pd",
        )
        record = _record_mixed(
            direction, "blocked", "driver-absent", "not-started",
            "a" * 40 + ";clean", None, None,
            {"i2pr": "not-run", "i2pd": "not-run"},
            {"i2pr": "not-observed", "i2pd": "not-observed"},
            {"i2pr": {"started": 0, "exited": 0, "forced": 0},
             "i2pd": {"started": 0, "exited": 0, "forced": 0}},
            oracle_kind="split-send-and-receive",
        )
        self.assertNotIn("echo", record["deterministic_parameters"].lower())


class BuildGateHandshakeSmokeTests(unittest.TestCase):
    def test_handshake_smoke_scenarios_are_directional_mixed(self) -> None:
        from build_gate import GATE_SCENARIOS
        hs = GATE_SCENARIOS["handshake-smoke"]
        self.assertEqual(len(hs), 4)
        self.assertIn("i2pr-to-java-ipv4", hs)
        self.assertIn("java-to-i2pr-ipv4", hs)
        self.assertIn("i2pr-to-i2pd-ipv4", hs)
        self.assertIn("i2pd-to-i2pr-ipv4", hs)

    def test_full_profile_includes_handshake_smoke(self) -> None:
        from build_gate import gates_for_profile
        gates = gates_for_profile("full")
        self.assertIn("handshake-smoke", gates)

    def test_handshake_smoke_gate_chain_includes_prerequisites(self) -> None:
        from build_gate import gates_for_profile
        gates = gates_for_profile("handshake-smoke")
        self.assertIn("environment-smoke", gates)
        self.assertIn("reference-crosscheck-ipv4", gates)
        self.assertIn("handshake-smoke", gates)


class CleanupFailureInjectionTests(unittest.TestCase):
    def test_stop_adapter_returns_failed_on_runtime_error(self) -> None:
        from mixed_runner import _stop_adapter
        class FailingAdapter:
            def stop(self, timeout: float) -> str:
                raise RuntimeError("refusing-stop")
        result = _stop_adapter(FailingAdapter(), 1.0)
        self.assertEqual(result, "failed")

    def test_stop_adapter_returns_clean_on_success(self) -> None:
        from mixed_runner import _stop_adapter
        class CleanAdapter:
            def stop(self, timeout: float) -> str:
                return "clean"
        result = _stop_adapter(CleanAdapter(), 1.0)
        self.assertEqual(result, "clean")

    def test_no_residual_state_returns_true_for_empty_lists(self) -> None:
        import subprocess
        import os
        from mixed_runner import _no_residual_state
        prefix = [] if os.geteuid() == 0 else ["sudo", "-n"]
        namespaces = subprocess.run(
            prefix + ["ip", "netns", "list"],
            capture_output=True, text=True, check=False,
        )
        if namespaces.returncode != 0:
            self.skipTest("cannot list namespaces")

    def test_evidence_record_schema_unchanged(self) -> None:
        from evidence import RECORD_FIELDS
        self.assertIn("deterministic_parameters", RECORD_FIELDS)
        self.assertIn("resource_counters", RECORD_FIELDS)
        self.assertIn("process_counters", RECORD_FIELDS)


if __name__ == "__main__":
    unittest.main()
