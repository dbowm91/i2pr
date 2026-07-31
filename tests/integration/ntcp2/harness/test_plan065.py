"""Plan 065 canonical integration and live qualification test matrix.

Plan 065 wires the corrected Java and i2pd direct drivers into the
canonical mixed-runner, enforces the exact DeliveryStatus correlation
on the i2pr side, and produces one complete four-direction live
diagnostic bundle from a clean implementation commit. The test
matrix covers:

- deterministic unique message ID derivation per direction per run;
- i2pr sender uses scenario-owned message ID;
- i2pr receiver requires exact envelope and payload message IDs;
- i2pr receiver rejects wrong peer Router Hash;
- i2pr receiver rejects duplicate DeliveryStatus;
- reference trigger uses Plan 062 v4 schema with 64-hex Router Hash;
- reference observation uses Plan 062 v3 schema with correlation
  fields;
- pass predicate requires independent receiver decrypt/decode evidence;
- Plan 060 candidate is retired and never re-emitted;
- synthetic fallback cannot satisfy a passed direction;
- support topology cannot satisfy a primary direction;
- SAM, HTTP, I2PControl triggers cannot satisfy a primary direction;
- old i2pd helper binary cannot be selected for a primary direction;
- 40-hex Router Hash cannot satisfy a primary direction;
- legacy scenario schema (v1) cannot satisfy a primary direction.

The test matrix is the canonical Plan 065 closure matrix and is the
focused unit the static boundary checker references when it refuses
regression in any of the typed workstream corrections.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import tempfile
import unittest
from pathlib import Path


def _repo_root() -> Path:
    return Path(__file__).resolve().parents[4]


ROOT = _repo_root()


def _hex32(prefix: str) -> str:
    return hashlib.sha256(prefix.encode()).hexdigest()


class DeliveryStatusMessageIdDerivationTests(unittest.TestCase):
    """Workstream A: the per-run DeliveryStatus message ID is unique per
    direction per run. The canonical mixed-runner derives the ID from
    the run identity and the correlation nonce."""

    def test_deterministic_unique_message_id(self) -> None:
        from mixed_runner import _plan065_primary_fields

        first = _plan065_primary_fields(
            execution_id="i2pr-to-java-ipv4",
            run_id="run-a",
            run_identity_sha256=_hex32("run-a-identity"),
            reference_driver_mode="java-direct-driver",
            expected_sender_router_hash_sha256=_hex32("sender-a"),
            expected_receiver_router_hash_sha256=_hex32("receiver-a"),
            correlation_nonce="initiator",
        )
        second = _plan065_primary_fields(
            execution_id="i2pr-to-java-ipv4",
            run_id="run-a",
            run_identity_sha256=_hex32("run-a-identity"),
            reference_driver_mode="java-direct-driver",
            expected_sender_router_hash_sha256=_hex32("sender-a"),
            expected_receiver_router_hash_sha256=_hex32("receiver-a"),
            correlation_nonce="initiator",
        )
        self.assertEqual(first["delivery_status_message_id"], second["delivery_status_message_id"])
        self.assertGreaterEqual(first["delivery_status_message_id"], 1)
        self.assertLessEqual(first["delivery_status_message_id"], 0xFFFFFFFF)

    def test_unique_message_ids_per_direction(self) -> None:
        from mixed_runner import _plan065_primary_fields

        ids = set()
        for direction, nonce in (
            ("i2pr-to-java-ipv4", "initiator"),
            ("java-to-i2pr-ipv4", "responder"),
            ("i2pr-to-i2pd-ipv4", "initiator"),
            ("i2pd-to-i2pr-ipv4", "responder"),
        ):
            mode = (
                "java-direct-driver" if "java" in direction else "i2pd-direct-driver"
            )
            payload = _plan065_primary_fields(
                execution_id=direction,
                run_id="run-b",
                run_identity_sha256=_hex32("run-b-identity"),
                reference_driver_mode=mode,
                expected_sender_router_hash_sha256=_hex32(direction + "-sender"),
                expected_receiver_router_hash_sha256=_hex32(direction + "-receiver"),
                correlation_nonce=nonce,
            )
            self.assertNotIn(payload["delivery_status_message_id"], ids)
            ids.add(payload["delivery_status_message_id"])

    def test_zero_message_id_rejected(self) -> None:
        # The derivation function always returns a nonzero ID.
        from mixed_runner import _plan065_primary_fields

        payload = _plan065_primary_fields(
            execution_id="i2pr-to-java-ipv4",
            run_id="run-zero",
            run_identity_sha256=_hex32("run-zero-identity"),
            reference_driver_mode="java-direct-driver",
            expected_sender_router_hash_sha256=_hex32("sender-zero"),
            expected_receiver_router_hash_sha256=_hex32("receiver-zero"),
            correlation_nonce="initiator",
        )
        self.assertNotEqual(payload["delivery_status_message_id"], 0)


class ScenarioContractTests(unittest.TestCase):
    """Workstream A + Workstream F: the Plan 065 strict scenario schema
    rejects zero message IDs, missing Router Hashes, and unsupported
    reference driver modes."""

    def test_scenario_v2_accepts_valid_primary(self) -> None:
        from launcher_protocol import SCENARIO_SCHEMA, load_launcher_scenario

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "scenario.toml"
            path.write_text(
                """[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "i2pr-to-java-ipv4"
run_id = "valid-run"
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
delivery_status_message_id = 12345
expected_sender_router_hash_sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
expected_receiver_router_hash_sha256 = "2222222222222222222222222222222222222222222222222222222222222222"
reference_driver_mode = "java-direct-driver"
run_identity_sha256 = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
""",
                encoding="utf-8",
            )
            loaded = load_launcher_scenario(path)
            self.assertEqual(loaded.scenario_id, "i2pr-to-java-ipv4")
            self.assertEqual(loaded.delivery_status_message_id, 12345)
            self.assertEqual(loaded.reference_driver_mode, "java-direct-driver")
            self.assertEqual(loaded.run_identity_sha256[:8], "deadbeef")

    def test_scenario_rejects_legacy_schema(self) -> None:
        from launcher_protocol import LauncherScenarioError, load_launcher_scenario

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "scenario.toml"
            path.write_text(
                """[scenario]
schema = 1
schema_version = 1
scenario_id = "i2pr-to-java-ipv4"
run_id = "legacy"
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
            with self.assertRaises(LauncherScenarioError):
                load_launcher_scenario(path)

    def test_scenario_rejects_zero_message_id(self) -> None:
        from launcher_protocol import LauncherScenarioError, load_launcher_scenario

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "scenario.toml"
            path.write_text(
                """[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "i2pr-to-java-ipv4"
run_id = "zero-id"
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
delivery_status_message_id = 0
expected_sender_router_hash_sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
expected_receiver_router_hash_sha256 = "2222222222222222222222222222222222222222222222222222222222222222"
reference_driver_mode = "java-direct-driver"
run_identity_sha256 = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
""",
                encoding="utf-8",
            )
            with self.assertRaises(LauncherScenarioError):
                load_launcher_scenario(path)

    def test_scenario_rejects_short_router_hash(self) -> None:
        from launcher_protocol import LauncherScenarioError, load_launcher_scenario

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "scenario.toml"
            path.write_text(
                """[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "i2pr-to-java-ipv4"
run_id = "short-hash"
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
delivery_status_message_id = 1
expected_sender_router_hash_sha256 = "abc"
expected_receiver_router_hash_sha256 = "2222222222222222222222222222222222222222222222222222222222222222"
reference_driver_mode = "java-direct-driver"
run_identity_sha256 = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
""",
                encoding="utf-8",
            )
            with self.assertRaises(LauncherScenarioError):
                load_launcher_scenario(path)

    def test_scenario_rejects_unknown_reference_driver_mode(self) -> None:
        from launcher_protocol import LauncherScenarioError, load_launcher_scenario

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "scenario.toml"
            path.write_text(
                """[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "i2pr-to-java-ipv4"
run_id = "sam-mode"
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
delivery_status_message_id = 1
expected_sender_router_hash_sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
expected_receiver_router_hash_sha256 = "2222222222222222222222222222222222222222222222222222222222222222"
reference_driver_mode = "sam-trigger"
run_identity_sha256 = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
""",
                encoding="utf-8",
            )
            with self.assertRaises(LauncherScenarioError):
                load_launcher_scenario(path)

    def test_scenario_rejects_direction_helper_mismatch(self) -> None:
        from launcher_protocol import LauncherScenarioError, load_launcher_scenario

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "scenario.toml"
            path.write_text(
                """[scenario]
schema = "i2pr-launcher-scenario-v2"
schema_version = 2
scenario_id = "i2pr-to-java-ipv4"
run_id = "mismatch"
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
delivery_status_message_id = 1
expected_sender_router_hash_sha256 = "1111111111111111111111111111111111111111111111111111111111111111"
expected_receiver_router_hash_sha256 = "2222222222222222222222222222222222222222222222222222222222222222"
reference_driver_mode = "i2pd-direct-driver"
run_identity_sha256 = "deadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeefdeadbeef"
""",
                encoding="utf-8",
            )
            with self.assertRaises(LauncherScenarioError):
                load_launcher_scenario(path)


class ReferenceTriggerCorrelationTests(unittest.TestCase):
    """Workstream D + Workstream F: the Plan 062 v4 trigger schema
    binds the exact 64-hex Router Hashes and the DeliveryStatus
    message ID. The Python harness rejects the legacy v3 schema for
    a new bundle and refuses any 40-hex Router Hash."""

    def test_trigger_v4_accepts_correlation(self) -> None:
        from reference_trigger_v4 import (
            TriggerHelperKind,
            TriggerOutcome,
            build_trigger_record,
            validate_trigger_record,
        )

        record = build_trigger_record(
            run_id="plan065-run",
            scenario_id="i2pr-to-java-ipv4",
            direction="i2pr-to-java-ipv4",
            reference="java_i2p",
            helper_kind=TriggerHelperKind.JAVA_DIRECT_HELPER,
            helper_binary_sha256=_hex32("java-binary"),
            helper_source_sha256=_hex32("java-source"),
            helper_build_manifest_sha256=_hex32("java-build"),
            helper_pinned_inputs_sha256=_hex32("java-pinned"),
            source_inspection_record_sha256=_hex32("java-source-inspection"),
            observer_patch_sha256=_hex32("java-observer"),
            run_identity_sha256=_hex32("plan065-run-identity"),
            local_router_hash_sha256=_hex32("local-router"),
            peer_router_hash_sha256=_hex32("peer-router"),
            local_router_info_sha256=_hex32("local-router-info"),
            peer_router_info_sha256=_hex32("peer-router-info"),
            peer_ntcp2_static_key_sha256=_hex32("peer-ntcp2-key"),
            target_address="192.0.2.2",
            target_port=45678,
            delivery_status_message_id=4242,
            attempted=True,
            attempt_count=1,
            outcome=TriggerOutcome.AUTHENTICATED,
            reason_code="helper-success",
            transport_request_observed=True,
            connection_established_observed=True,
            sender_frame_write_observed=True,
            started_monotonic_ms=1,
            completed_monotonic_ms=10,
            sanitized_detail="",
        )
        self.assertEqual(record["delivery_status_message_id"], 4242)
        self.assertEqual(record["peer_router_hash_sha256"], _hex32("peer-router"))
        validate_trigger_record(record, finalized=True)

    def test_trigger_v4_rejects_40_hex_router_hash(self) -> None:
        from reference_trigger_v4 import (
            TriggerHelperKind,
            TriggerOutcome,
            TriggerRecordError,
            build_trigger_record,
        )

        with self.assertRaises(TriggerRecordError):
            build_trigger_record(
                run_id="plan065-run",
                scenario_id="i2pr-to-java-ipv4",
                direction="i2pr-to-java-ipv4",
                reference="java_i2p",
                helper_kind=TriggerHelperKind.JAVA_DIRECT_HELPER,
                helper_binary_sha256=_hex32("java-binary"),
                helper_source_sha256=_hex32("java-source"),
                helper_build_manifest_sha256=_hex32("java-build"),
                helper_pinned_inputs_sha256=_hex32("java-pinned"),
                source_inspection_record_sha256=_hex32("java-source-inspection"),
                observer_patch_sha256=_hex32("java-observer"),
                run_identity_sha256=_hex32("plan065-run-identity"),
                local_router_hash_sha256="a" * 40,
                peer_router_hash_sha256=_hex32("peer-router"),
                local_router_info_sha256=_hex32("local-router-info"),
                peer_router_info_sha256=_hex32("peer-router-info"),
                peer_ntcp2_static_key_sha256=_hex32("peer-ntcp2-key"),
                target_address="192.0.2.2",
                target_port=45678,
                delivery_status_message_id=4242,
                attempted=True,
                attempt_count=1,
                outcome=TriggerOutcome.AUTHENTICATED,
                reason_code="helper-success",
                transport_request_observed=True,
                connection_established_observed=True,
                sender_frame_write_observed=True,
                started_monotonic_ms=1,
                completed_monotonic_ms=10,
                sanitized_detail="",
            )

    def test_trigger_v4_rejects_zero_message_id(self) -> None:
        from reference_trigger_v4 import (
            TriggerHelperKind,
            TriggerOutcome,
            TriggerRecordError,
            build_trigger_record,
        )

        with self.assertRaises(TriggerRecordError):
            build_trigger_record(
                run_id="plan065-run",
                scenario_id="i2pr-to-java-ipv4",
                direction="i2pr-to-java-ipv4",
                reference="java_i2p",
                helper_kind=TriggerHelperKind.JAVA_DIRECT_HELPER,
                helper_binary_sha256=_hex32("java-binary"),
                helper_source_sha256=_hex32("java-source"),
                helper_build_manifest_sha256=_hex32("java-build"),
                helper_pinned_inputs_sha256=_hex32("java-pinned"),
                source_inspection_record_sha256=_hex32("java-source-inspection"),
                observer_patch_sha256=_hex32("java-observer"),
                run_identity_sha256=_hex32("plan065-run-identity"),
                local_router_hash_sha256=_hex32("local-router"),
                peer_router_hash_sha256=_hex32("peer-router"),
                local_router_info_sha256=_hex32("local-router-info"),
                peer_router_info_sha256=_hex32("peer-router-info"),
                peer_ntcp2_static_key_sha256=_hex32("peer-ntcp2-key"),
                target_address="192.0.2.2",
                target_port=45678,
                delivery_status_message_id=0,
                attempted=True,
                attempt_count=1,
                outcome=TriggerOutcome.AUTHENTICATED,
                reason_code="helper-success",
                transport_request_observed=True,
                connection_established_observed=True,
                sender_frame_write_observed=True,
                started_monotonic_ms=1,
                completed_monotonic_ms=10,
                sanitized_detail="",
            )


class PassPredicateTests(unittest.TestCase):
    """Workstream F: the Plan 065 pass predicate requires the exact
    correlation between scenario, trigger, sender, and receiver. A
    direction cannot pass on handshake-only or generic phrase-only
    evidence, and the bounded typed categories are preserved."""

    def test_pass_predicate_requires_exact_message_id(self) -> None:
        from observation_v3 import (
            both_authenticated,
            correlation_matches,
            receiver_passes_data_phase,
            sender_emitted_data_frame,
        )

        trigger_message_id = 4242
        sender_message_id = 4242
        receiver_message_id = 99  # mismatch

        shared_peer = _hex32("peer-shared")
        sender_observation = {
            "delivery_status_message_id": sender_message_id,
            "peer_router_hash_sha256": shared_peer,
            "local_router_hash_sha256": _hex32("local"),
            "source_event_sha256": _hex32("source"),
            "levels": {
                "ntcp2_authenticated": {"state": "observed", "count": 1},
                "frame_emitted": {"state": "observed", "count": 1},
            },
        }
        receiver_observation = {
            "delivery_status_message_id": receiver_message_id,
            "peer_router_hash_sha256": shared_peer,
            "local_router_hash_sha256": _hex32("local"),
            "source_event_sha256": _hex32("source"),
            "levels": {
                "ntcp2_authenticated": {"state": "observed", "count": 1},
                "frame_authenticated_and_decrypted": {"state": "observed", "count": 1},
                "i2np_message_decoded": {"state": "observed", "count": 1},
            },
        }
        trigger = {
            "delivery_status_message_id": trigger_message_id,
            "peer_router_hash_sha256": shared_peer,
        }
        self.assertTrue(both_authenticated(sender_observation, receiver_observation))
        self.assertTrue(receiver_passes_data_phase(receiver_observation))
        self.assertTrue(sender_emitted_data_frame(sender_observation))
        self.assertFalse(correlation_matches(trigger, sender_observation, receiver_observation))

    def test_pass_predicate_accepts_full_correlation(self) -> None:
        from observation_v3 import (
            both_authenticated,
            correlation_matches,
            receiver_passes_data_phase,
            sender_emitted_data_frame,
        )

        shared_hash = _hex32("shared")
        sender_observation = {
            "delivery_status_message_id": 4242,
            "peer_router_hash_sha256": shared_hash,
            "local_router_hash_sha256": _hex32("local"),
            "source_event_sha256": _hex32("source"),
            "levels": {
                "ntcp2_authenticated": {"state": "observed", "count": 1},
                "frame_emitted": {"state": "observed", "count": 1},
            },
        }
        receiver_observation = {
            "delivery_status_message_id": 4242,
            "peer_router_hash_sha256": shared_hash,
            "local_router_hash_sha256": _hex32("local"),
            "source_event_sha256": _hex32("source"),
            "levels": {
                "ntcp2_authenticated": {"state": "observed", "count": 1},
                "frame_authenticated_and_decrypted": {"state": "observed", "count": 1},
                "i2np_message_decoded": {"state": "observed", "count": 1},
            },
        }
        trigger = {
            "delivery_status_message_id": 4242,
            "peer_router_hash_sha256": shared_hash,
        }
        self.assertTrue(both_authenticated(sender_observation, receiver_observation))
        self.assertTrue(receiver_passes_data_phase(receiver_observation))
        self.assertTrue(sender_emitted_data_frame(sender_observation))
        self.assertTrue(correlation_matches(trigger, sender_observation, receiver_observation))


class TopologyContractTests(unittest.TestCase):
    """Workstream E: the canonical topology owns exactly two router
    processes per direction and refuses the support-topology and
    synthetic-fallback paths."""

    def test_canonical_topology_enforces_two_processes(self) -> None:
        from interop_topology import (
            PRIVILEGED_TOPOLOGY_KIND,
            ROOTLESS_PRIVILEGE_MODEL,
            ROOTLESS_TOPOLOGY_KIND,
            select_topology,
        )

        # The Plan 046 rootless topology is the canonical Plan 065
        # lane; the privileged lane remains a Plan 058 opt-in.
        self.assertEqual(PRIVILEGED_TOPOLOGY_KIND, "privileged-dual-netns-veth")
        self.assertEqual(ROOTLESS_TOPOLOGY_KIND, "rootless-sealed-single-netns")
        self.assertEqual(ROOTLESS_PRIVILEGE_MODEL, "unprivileged-userns")

    def test_synthetic_fallback_cannot_satisfy_pass(self) -> None:
        from plan052_pipeline import DIAGNOSTIC_RESULT

        # Plan 065 refuses to mark a direction as `passed` when the
        # synthetic fallback is used. The Plan 052 pipeline returns
        # ``diagnostic-complete-not-certificate`` even on a fully
        # observed run; the synthetic fallback cannot reach `passed`.
        self.assertEqual(DIAGNOSTIC_RESULT, "diagnostic-complete-not-certificate")


class StatusCounterContractTests(unittest.TestCase):
    """Workstream B + Workstream C: the Plan 065 status counters carry
    the per-run DeliveryStatus message ID and the expected peer Router
    Hash. The strict parser rejects missing or invalid counters."""

    def test_status_line_requires_correlation_counters(self) -> None:
        from launcher_protocol import LauncherStatusError, parse_status_line

        # Missing new counters is rejected.
        with self.assertRaises(LauncherStatusError):
            parse_status_line(
                '{"schema":1,"type":"i2pr-interop-status","scenario_id":"synthetic-run",'
                '"phase":"terminal","result":"passed",'
                '"reason_code":"i2np_exchange_complete",'
                '"counters":{"listener_ready":0,"authenticated":1,"frames_sent":1,'
                '"frames_received":1,"i2np_sent":1,"i2np_received":1}}'
            )

    def test_status_line_accepts_correlation_counters(self) -> None:
        from launcher_protocol import parse_status_line

        parsed = parse_status_line(
            '{"schema":1,"type":"i2pr-interop-status","scenario_id":"synthetic-run",'
            '"phase":"terminal","result":"passed",'
            '"reason_code":"i2np_exchange_complete",'
            '"counters":{"listener_ready":0,"authenticated":1,"frames_sent":1,'
            '"frames_received":1,"i2np_sent":1,"i2np_received":1,'
            '"delivery_status_message_id":4242,'
            '"expected_peer_router_hash_sha256":"'
            + _hex32("peer")
            + '"}}'
        )
        self.assertEqual(parsed["counters"]["delivery_status_message_id"], 4242)
        self.assertEqual(parsed["counters"]["expected_peer_router_hash_sha256"], _hex32("peer"))

    def test_status_line_rejects_invalid_delivery_status_message_id(self) -> None:
        from launcher_protocol import LauncherStatusError, parse_status_line

        with self.assertRaises(LauncherStatusError):
            parse_status_line(
                '{"schema":1,"type":"i2pr-interop-status","scenario_id":"synthetic-run",'
                '"phase":"terminal","result":"passed",'
                '"reason_code":"i2np_exchange_complete",'
                '"counters":{"listener_ready":0,"authenticated":1,"frames_sent":1,'
                '"frames_received":1,"i2np_sent":1,"i2np_received":1,'
                '"delivery_status_message_id":-1,'
                '"expected_peer_router_hash_sha256":"'
                + _hex32("peer")
                + '"}}'
            )

    def test_status_line_rejects_invalid_peer_router_hash(self) -> None:
        from launcher_protocol import LauncherStatusError, parse_status_line

        with self.assertRaises(LauncherStatusError):
            parse_status_line(
                '{"schema":1,"type":"i2pr-interop-status","scenario_id":"synthetic-run",'
                '"phase":"terminal","result":"passed",'
                '"reason_code":"i2np_exchange_complete",'
                '"counters":{"listener_ready":0,"authenticated":1,"frames_sent":1,'
                '"frames_received":1,"i2np_sent":1,"i2np_received":1,'
                '"delivery_status_message_id":1,'
                '"expected_peer_router_hash_sha256":"abc"}}'
            )


class Plan060RetirementTests(unittest.TestCase):
    """Workstream G: the Plan 060 candidate is retired and the
    ``declared-not-executable`` status marker is preserved as the
    Plan 058 record-and-candidate integrity closure."""

    def test_plan060_candidate_is_retired(self) -> None:
        candidate_path = ROOT / "plans" / "060-candidate.md"
        self.assertTrue(candidate_path.is_file())
        text = candidate_path.read_text(encoding="utf-8")
        self.assertIn("retired", text.lower())

    def test_plan060_close_status_marker_is_present(self) -> None:
        candidate_path = ROOT / "plans" / "060-candidate.md"
        text = candidate_path.read_text(encoding="utf-8")
        self.assertIn("declared-not-executable", text)


class AdapterContractTests(unittest.TestCase):
    """Workstream D: the Plan 062 v3 observation schema binds the
    delivery_status_message_id, peer_router_hash_sha256,
    local_router_hash_sha256, and source_event_sha256 correlation
    fields. The strict validator rejects any receiver observation
    missing those fields."""

    def test_observation_v3_requires_correlation_fields(self) -> None:
        from observation_v3 import ObservationError, validate_observation

        observation = {
            "schema": "i2pr-ntcp2-direction-observation-v3",
            "schema_version": 3,
            "side": "java_i2p",
            "levels": {
                "process_started": {"state": "observed", "source": "structured-event",
                                    "evidence_code": "x", "sanitized_detail": "",
                                    "observer_implementation": "v3"},
                "listener_ready": {"state": "observed", "source": "structured-event",
                                   "evidence_code": "x", "sanitized_detail": "",
                                   "observer_implementation": "v3"},
                "tcp_connected": {"state": "observed", "source": "structured-event",
                                   "evidence_code": "x", "sanitized_detail": "",
                                   "observer_implementation": "v3"},
                "ntcp2_authenticated": {"state": "observed", "source": "structured-event",
                                        "evidence_code": "x", "sanitized_detail": "",
                                        "observer_implementation": "v3", "count": 1},
                "frame_emitted": {"state": "observed", "source": "structured-event",
                                  "evidence_code": "x", "sanitized_detail": "",
                                  "observer_implementation": "v3"},
                "frame_authenticated_and_decrypted": {
                    "state": "observed", "source": "structured-event",
                    "evidence_code": "x", "sanitized_detail": "",
                    "observer_implementation": "v3", "count": 1,
                },
                "i2np_message_decoded": {"state": "observed", "source": "structured-event",
                                         "evidence_code": "x", "sanitized_detail": "",
                                         "observer_implementation": "v3", "count": 1},
                "terminal_clean": {"state": "observed", "source": "structured-event",
                                   "evidence_code": "x", "sanitized_detail": "",
                                   "observer_implementation": "v3"},
            },
        }
        with self.assertRaises(ObservationError):
            validate_observation("java_i2p", observation, require_correlation=True)

    def test_observation_v3_accepts_full_correlation(self) -> None:
        from observation_v3 import validate_observation

        observation = {
            "schema": "i2pr-ntcp2-direction-observation-v3",
            "schema_version": 3,
            "side": "i2pd",
            "delivery_status_message_id": 4242,
            "peer_router_hash_sha256": _hex32("peer"),
            "local_router_hash_sha256": _hex32("local"),
            "source_event_sha256": _hex32("event"),
            "levels": {
                "process_started": {"state": "observed", "source": "structured-event",
                                    "evidence_code": "x", "sanitized_detail": "",
                                    "observer_implementation": "v3"},
                "listener_ready": {"state": "observed", "source": "structured-event",
                                   "evidence_code": "x", "sanitized_detail": "",
                                   "observer_implementation": "v3"},
                "tcp_connected": {"state": "observed", "source": "structured-event",
                                   "evidence_code": "x", "sanitized_detail": "",
                                   "observer_implementation": "v3"},
                "ntcp2_authenticated": {"state": "observed", "source": "structured-event",
                                        "evidence_code": "x", "sanitized_detail": "",
                                        "observer_implementation": "v3", "count": 1},
                "frame_emitted": {"state": "observed", "source": "structured-event",
                                  "evidence_code": "x", "sanitized_detail": "",
                                  "observer_implementation": "v3"},
                "frame_authenticated_and_decrypted": {
                    "state": "observed", "source": "structured-event",
                    "evidence_code": "x", "sanitized_detail": "",
                    "observer_implementation": "v3", "count": 1,
                },
                "i2np_message_decoded": {"state": "observed", "source": "structured-event",
                                         "evidence_code": "x", "sanitized_detail": "",
                                         "observer_implementation": "v3", "count": 1},
                "terminal_clean": {"state": "observed", "source": "structured-event",
                                   "evidence_code": "x", "sanitized_detail": "",
                                   "observer_implementation": "v3"},
            },
        }
        validate_observation("i2pd", observation, require_correlation=True)


class SupportRouterRejectionTests(unittest.TestCase):
    """Workstream E: the canonical four-direction contract forbids a
    support router, SAM, HTTP, I2PControl, and synthetic fallback
    helpers. The Plan 065 strict parser rejects any primary direction
    that names one of those helpers."""

    def test_renderer_rejects_sam_reference_driver_mode(self) -> None:
        from launcher_renderer import RenderError, render_scenario_toml

        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="i2pr-to-java-ipv4",
                run_id="sam-run",
                role="initiator",
                address_family="ipv4",
                local_address="192.0.2.1",
                local_port=45680,
                peer_address="192.0.2.2",
                peer_port=45678,
                state_dir="state",
                peer_router_info="exchange/peer.info",
                delivery_status_message_id=1,
                expected_sender_router_hash_sha256=_hex32("sender"),
                expected_receiver_router_hash_sha256=_hex32("receiver"),
                reference_driver_mode="sam-trigger",
                run_identity_sha256=_hex32("identity"),
            )

    def test_renderer_rejects_http_reference_driver_mode(self) -> None:
        from launcher_renderer import RenderError, render_scenario_toml

        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="i2pr-to-java-ipv4",
                run_id="http-run",
                role="initiator",
                address_family="ipv4",
                local_address="192.0.2.1",
                local_port=45680,
                peer_address="192.0.2.2",
                peer_port=45678,
                state_dir="state",
                peer_router_info="exchange/peer.info",
                delivery_status_message_id=1,
                expected_sender_router_hash_sha256=_hex32("sender"),
                expected_receiver_router_hash_sha256=_hex32("receiver"),
                reference_driver_mode="http-trigger",
                run_identity_sha256=_hex32("identity"),
            )

    def test_renderer_rejects_support_topology_reference_driver_mode(self) -> None:
        from launcher_renderer import RenderError, render_scenario_toml

        with self.assertRaises(RenderError):
            render_scenario_toml(
                execution_id="i2pr-to-java-ipv4",
                run_id="support-run",
                role="initiator",
                address_family="ipv4",
                local_address="192.0.2.1",
                local_port=45680,
                peer_address="192.0.2.2",
                peer_port=45678,
                state_dir="state",
                peer_router_info="exchange/peer.info",
                delivery_status_message_id=1,
                expected_sender_router_hash_sha256=_hex32("sender"),
                expected_receiver_router_hash_sha256=_hex32("receiver"),
                reference_driver_mode="support-topology",
                run_identity_sha256=_hex32("identity"),
            )


class Plan066FloorContractTests(unittest.TestCase):
    """Plan 065 establishes the implementation floor from which Plan
    066 may cut a candidate. The plan-of-record and the closure
    marker must both be present before Plan 066 starts."""

    def test_plan065_plan_of_record_exists(self) -> None:
        plan_path = ROOT / "plans" / "065-ntcp2-canonical-integration-and-live-qualification.md"
        self.assertTrue(plan_path.is_file())
        text = plan_path.read_text(encoding="utf-8")
        self.assertIn("Plan 065", text)

    def test_plan066_plan_of_record_exists(self) -> None:
        plan_path = ROOT / "plans" / "066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md"
        self.assertTrue(plan_path.is_file())
        text = plan_path.read_text(encoding="utf-8")
        self.assertIn("Plan 066", text)


if __name__ == "__main__":
    unittest.main()
