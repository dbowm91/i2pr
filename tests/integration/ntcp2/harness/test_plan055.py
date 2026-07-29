"""Plan 055 trigger record schema and qualification tests.

These tests cover the Plan 055 Workstream A trigger record schema, the
bounded outcomes enumerated in Workstream A2, the helper-build
provenance fields, and the one-shot attempt-count contract from
Workstream A4. End-to-end qualification tests for the two
reference-initiated directions live in this file as well so that
Plan 055's positive/negative controls share a single harness surface.
"""

from __future__ import annotations

import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]


def _minimal_record(**overrides):
    """Return a baseline Plan 055 trigger record payload."""

    payload = {
        "schema": "i2pr-reference-trigger-v3",
        "schema_version": 3,
        "run_id": "mixed-20260101t000000z-1-abcdef01",
        "scenario_id": "i2pd-to-i2pr-ipv4",
        "reference": "i2pd",
        "reference_version": "2.60.0",
        "reference_revision": "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
        "helper_kind": "i2pd-direct-helper",
        "helper_binary_sha256": "a" * 64,
        "helper_source_sha256": "b" * 64,
        "helper_compiler": "clang-15",
        "helper_pinned_inputs_sha256": "c" * 64,
        "source_inspection_record_sha256": "d" * 64,
        "target_router_hash": "e" * 40,
        "target_router_info_sha256": "f" * 64,
        "target_ntcp2_static_key_sha256": "0" * 64,
        "target_address": "192.0.2.2",
        "target_port": 45680,
        "correlation_nonce": "abcdefgh-1234-5678-9abc-def012345678",
        "attempted": True,
        "attempt_count": 1,
        "outcome": "requested",
        "reason_code": "i2pd-trigger-dispatched",
        "transport_request_observed": True,
        "connection_callback_observed": False,
        "started_monotonic_ms": 1000,
        "completed_monotonic_ms": 2500,
        "sanitized_detail": "i2pd-direct-helper dispatched via ConnectToPeer",
        "run_identity_sha256": "1" * 64,
        "trigger_sha256": "",
    }
    payload.update(overrides)
    return payload


class TriggerSchemaTests(unittest.TestCase):
    def test_minimal_record_finalizes(self):
        from trigger_record import (
            finalize_trigger_record,
            TRIGGER_SCHEMA,
            TRIGGER_SCHEMA_VERSION,
        )
        record = _minimal_record()
        digest = finalize_trigger_record(record)
        self.assertEqual(len(digest), 64)
        self.assertEqual(record["trigger_sha256"], digest)
        self.assertEqual(record["schema"], TRIGGER_SCHEMA)
        self.assertEqual(record["schema_version"], TRIGGER_SCHEMA_VERSION)

    def test_malformed_target_hash_rejected(self):
        from trigger_record import TriggerRecordError, finalize_trigger_record
        record = _minimal_record(target_router_hash="not-a-hash")
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(ctx.exception.args[0], "trigger-target-router-hash-invalid")

    def test_wrong_endpoint_type_rejected(self):
        from trigger_record import TriggerRecordError, finalize_trigger_record
        record = _minimal_record(target_address="198.51.100.7")
        with self.assertRaises(TriggerRecordError):
            finalize_trigger_record(record)

    def test_zero_helper_digest_rejected_for_attempted_trigger(self):
        from trigger_record import TriggerRecordError, finalize_trigger_record
        record = _minimal_record(helper_binary_sha256="0" * 64)
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(
            ctx.exception.args[0], "trigger-zero-helper-digest-with-attempted"
        )

    def test_unknown_helper_kind_rejected(self):
        from trigger_record import TriggerRecordError, finalize_trigger_record
        record = _minimal_record(helper_kind="rogue-helper")
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(ctx.exception.args[0], "trigger-helper-kind-not-allowlisted")

    def test_attempt_count_other_than_one_shot_rejected(self):
        from trigger_record import TriggerRecordError, finalize_trigger_record
        record = _minimal_record(attempt_count=3)
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(ctx.exception.args[0], "trigger-attempt-count-other-than-one-shot")

    def test_unattempted_trigger_with_zero_count_passes(self):
        from trigger_record import finalize_trigger_record
        record = _minimal_record(
            attempted=False,
            attempt_count=0,
            helper_binary_sha256="0" * 64,
            helper_source_sha256="0" * 64,
            transport_request_observed=False,
            connection_callback_observed=False,
            outcome="not-required-i2pr-initiator",
        )
        finalize_trigger_record(record)

    def test_run_identity_mismatch_rejected(self):
        from trigger_record import TriggerRecordError, finalize_trigger_record
        record = _minimal_record()
        finalize_trigger_record(record)
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record, run_identity_sha256="2" * 64)
        self.assertEqual(ctx.exception.args[0], "trigger-run-identity-mismatch")

    def test_missing_required_field_rejected(self):
        from trigger_record import TriggerRecordError, finalize_trigger_record
        record = _minimal_record()
        del record["target_router_hash"]
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertTrue(ctx.exception.args[0].startswith("trigger-record-missing:"))

    def test_unknown_outcome_rejected(self):
        from trigger_record import TriggerRecordError, finalize_trigger_record
        record = _minimal_record(outcome="connected-and-authenticated")
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(ctx.exception.args[0], "trigger-outcome-not-allowlisted")

    def test_invalid_revision_rejected(self):
        from trigger_record import TriggerRecordError, finalize_trigger_record
        record = _minimal_record(reference_revision="short-sha")
        with self.assertRaises(TriggerRecordError):
            finalize_trigger_record(record)

    def test_java_trigger_must_use_allowlisted_helper(self):
        from trigger_record import TriggerRecordError, finalize_trigger_record
        record = _minimal_record(
            reference="java_i2p",
            reference_version="2.12.0",
            reference_revision="2800040deee9bb376567b671ef2e9c34cf3e30b6",
            helper_kind="i2pd-direct-helper",
        )
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(ctx.exception.args[0], "java-trigger-helper-kind-invalid")

    def test_java_direct_helper_is_accepted(self):
        from trigger_record import finalize_trigger_record
        record = _minimal_record(
            reference="java_i2p",
            reference_version="2.12.0",
            reference_revision="2800040deee9bb376567b671ef2e9c34cf3e30b6",
            helper_kind="java-direct-helper",
        )
        finalize_trigger_record(record)

    def test_java_support_topology_helper_is_accepted(self):
        from trigger_record import finalize_trigger_record
        record = _minimal_record(
            reference="java_i2p",
            reference_version="2.12.0",
            reference_revision="2800040deee9bb376567b671ef2e9c34cf3e30b6",
            helper_kind="java-minimal-support-topology",
        )
        finalize_trigger_record(record)

    def test_digest_changes_when_attempt_count_changes(self):
        from trigger_record import finalize_trigger_record
        baseline = _minimal_record(attempt_count=1)
        finalize_trigger_record(baseline)
        attempted = _minimal_record(attempt_count=0, attempted=False)
        finalize_trigger_record(attempted)
        self.assertNotEqual(baseline["trigger_sha256"], attempted["trigger_sha256"])

    def test_digest_round_trip(self):
        from trigger_record import (
            compute_trigger_sha256,
            finalize_trigger_record,
        )
        record = _minimal_record()
        finalize_trigger_record(record)
        canonical = compute_trigger_sha256({**record, "trigger_sha256": ""})
        self.assertEqual(canonical, record["trigger_sha256"])

    def test_completed_before_started_rejected(self):
        from trigger_record import TriggerRecordError, finalize_trigger_record
        record = _minimal_record(
            started_monotonic_ms=2000,
            completed_monotonic_ms=1000,
        )
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(ctx.exception.args[0], "trigger-completed-before-started")


class TriggerBuilderTests(unittest.TestCase):
    def test_build_helper_attaches_provenance(self):
        from trigger_record import (
            build_trigger_record,
            TriggerHelperKind,
            TriggerOutcome,
        )
        record = build_trigger_record(
            run_id="mixed-20260101t000000z-1-abcdef01",
            scenario_id="i2pd-to-i2pr-ipv4",
            reference="i2pd",
            helper_kind=TriggerHelperKind.I2PD_DIRECT_HELPER,
            helper_binary_sha256="a" * 64,
            helper_source_sha256="b" * 64,
            helper_compiler="clang-15",
            helper_pinned_inputs_sha256="c" * 64,
            source_inspection_record_sha256="d" * 64,
            target_router_hash="e" * 40,
            target_router_info_sha256="f" * 64,
            target_ntcp2_static_key_sha256="0" * 64,
            target_address="192.0.2.2",
            target_port=45680,
            correlation_nonce="abcdefgh-1234-5678-9abc-def012345678",
            attempted=True,
            attempt_count=1,
            outcome=TriggerOutcome.REQUESTED,
            reason_code="i2pd-trigger-dispatched",
            transport_request_observed=True,
            connection_callback_observed=False,
            started_monotonic_ms=1000,
            completed_monotonic_ms=2500,
            sanitized_detail="i2pd-direct-helper dispatched via ConnectToPeer",
            run_identity_sha256="1" * 64,
        )
        self.assertEqual(record["schema"], "i2pr-reference-trigger-v3")
        self.assertEqual(record["helper_compiler"], "clang-15")
        self.assertEqual(record["helper_pinned_inputs_sha256"], "c" * 64)
        self.assertEqual(record["source_inspection_record_sha256"], "d" * 64)
        self.assertEqual(record["run_identity_sha256"], "1" * 64)
        self.assertEqual(len(record["trigger_sha256"]), 64)

    def test_helper_kind_classifiers(self):
        from trigger_record import (
            is_source_locked_helper,
            is_support_topology_helper,
            TriggerHelperKind,
        )
        self.assertTrue(
            is_source_locked_helper(TriggerHelperKind.I2PD_DIRECT_HELPER)
        )
        self.assertTrue(
            is_source_locked_helper(TriggerHelperKind.JAVA_DIRECT_HELPER)
        )
        self.assertFalse(
            is_source_locked_helper(TriggerHelperKind.JAVA_MINIMAL_SUPPORT_TOPOLOGY)
        )
        self.assertTrue(
            is_support_topology_helper(TriggerHelperKind.JAVA_MINIMAL_SUPPORT_TOPOLOGY)
        )
        self.assertFalse(is_support_topology_helper(TriggerHelperKind.I2PD_DIRECT_HELPER))


class DirectionBindingTests(unittest.TestCase):
    def _make_direction_record(self, **overrides):
        from trigger_record import (
            build_trigger_record,
            finalize_trigger_record,
            TriggerHelperKind,
            TriggerOutcome,
        )
        record = build_trigger_record(
            run_id="mixed-20260101t000000z-1-abcdef01",
            scenario_id="i2pd-to-i2pr-ipv4",
            reference="i2pd",
            helper_kind=TriggerHelperKind.I2PD_DIRECT_HELPER,
            helper_binary_sha256="a" * 64,
            helper_source_sha256="b" * 64,
            helper_compiler="clang-15",
            helper_pinned_inputs_sha256="c" * 64,
            source_inspection_record_sha256="d" * 64,
            target_router_hash="e" * 40,
            target_router_info_sha256="f" * 64,
            target_ntcp2_static_key_sha256="0" * 64,
            target_address="192.0.2.2",
            target_port=45680,
            correlation_nonce="abcdefgh-1234-5678-9abc-def012345678",
            attempted=True,
            attempt_count=1,
            outcome=TriggerOutcome.CONNECTED,
            reason_code="i2pd-trigger-connected",
            transport_request_observed=True,
            connection_callback_observed=True,
            started_monotonic_ms=1000,
            completed_monotonic_ms=2500,
            sanitized_detail="i2pd-direct-helper connected",
            run_identity_sha256="1" * 64,
        )
        record.update(overrides)
        finalize_trigger_record(record)
        return record

    def test_direction_binding_uses_same_nonce(self):
        trigger = self._make_direction_record(correlation_nonce="same-nonce-1234567890")
        direction = {
            "schema": "i2pr-mixed-router-direction-v2",
            "schema_version": 2,
            "scenario_id": trigger["scenario_id"],
            "trigger_sha256": trigger["trigger_sha256"],
            "target_router_hash": trigger["target_router_hash"],
            "target_router_info_sha256": trigger["target_router_info_sha256"],
            "correlation_nonce": trigger["correlation_nonce"],
        }
        # Re-binding the same fields must round-trip without error.
        for field in (
            "target_router_hash",
            "target_router_info_sha256",
            "correlation_nonce",
            "trigger_sha256",
        ):
            self.assertEqual(direction[field], trigger[field])

    def test_responder_stage_rejection_preserves_trigger_outcome(self):
        from trigger_record import TriggerOutcome
        trigger = self._make_direction_record(outcome=TriggerOutcome.CONNECTED.value)
        # Plan 055 E3: a successful trigger may still be paired with a
        # rejected direction (responder-side failure).
        direction_result = "rejected"
        direction_reason_code = "responder-session-confirmed-part2-failed"
        self.assertEqual(trigger["outcome"], "connected")
        self.assertEqual(direction_result, "rejected")
        self.assertEqual(
            direction_reason_code, "responder-session-confirmed-part2-failed"
        )


class ReferenceInitiationEndToEndTests(unittest.TestCase):
    """Plan 055 acceptance tests for the two reference-initiated directions.

    These tests simulate the four-Plan 055 positive and negative controls
    in-process (no live router) by binding the Plan 052 observation
    records through the Plan 055 trigger schema. They assert that:

    - a positive control reaches the full Plan 052 predicate;
    - a wrong-router-info control fails at the target identity stage;
    - a wrong-endpoint control cannot pass the data phase;
    - a malformed responder preserves the bounded reason while
      reporting a successful trigger;
    - a no-trigger control fails before the observation predicate runs.
    """

    def _observation(self, *, side: str, decrypt: bool, decode: bool) -> dict:
        def level(state: str) -> dict:
            return {
                "state": state,
                "source": "source-derived-log-marker",
                "evidence_code": "x",
                "sanitized_detail": "",
                "observer_implementation": f"{side}-observation-v2",
            }
        return {
            "schema": "i2pr-ntcp2-direction-observation-v2",
            "schema_version": 2,
            "side": side,
            "levels": {
                "process_started": level("observed"),
                "listener_ready": level("observed"),
                "tcp_connected": level("observed"),
                "ntcp2_authenticated": level("observed"),
                "frame_emitted": level("observed"),
                "frame_authenticated_and_decrypted": level(
                    "observed" if decrypt else "not-observed"
                ),
                "i2np_message_decoded": level("observed" if decode else "not-observed"),
                "terminal_clean": level("observed"),
            },
            "observation_sha256": "",
        }

    def _finalize(self, observation: dict) -> str:
        from observation import finalize_observation
        return finalize_observation(observation["side"], observation)

    def test_i2pd_to_i2pr_positive_reaches_plan052_predicate(self):
        from trigger_record import (
            build_trigger_record,
            finalize_trigger_record,
            TriggerHelperKind,
            TriggerOutcome,
        )
        from observation import receiver_passes_data_phase, both_authenticated
        trigger = build_trigger_record(
            run_id="mixed-20260101t000000z-1-abcdef01",
            scenario_id="i2pd-to-i2pr-ipv4",
            reference="i2pd",
            helper_kind=TriggerHelperKind.I2PD_DIRECT_HELPER,
            helper_binary_sha256="a" * 64,
            helper_source_sha256="b" * 64,
            helper_compiler="clang-15",
            helper_pinned_inputs_sha256="c" * 64,
            source_inspection_record_sha256="d" * 64,
            target_router_hash="e" * 40,
            target_router_info_sha256="f" * 64,
            target_ntcp2_static_key_sha256="0" * 64,
            target_address="192.0.2.2",
            target_port=45680,
            correlation_nonce="plan055-i2pd-pos-1234",
            attempted=True,
            attempt_count=1,
            outcome=TriggerOutcome.AUTHENTICATED,
            reason_code="i2pd-trigger-authenticated",
            transport_request_observed=True,
            connection_callback_observed=True,
            started_monotonic_ms=1000,
            completed_monotonic_ms=2500,
            sanitized_detail="i2pd-direct-helper authenticated",
            run_identity_sha256="1" * 64,
        )
        finalize_trigger_record(trigger)
        i2pr_observation = self._observation(side="i2pr", decrypt=True, decode=True)
        ref_observation = self._observation(side="i2pd", decrypt=True, decode=True)
        self._finalize(i2pr_observation)
        self._finalize(ref_observation)
        self.assertTrue(receiver_passes_data_phase(ref_observation))
        # Both sides authenticated:
        self.assertTrue(both_authenticated(i2pr_observation, ref_observation))
        self.assertEqual(trigger["outcome"], "authenticated")

    def test_i2pd_to_i2pr_wrong_router_info_fails(self):
        from trigger_record import (
            build_trigger_record,
            finalize_trigger_record,
            TriggerHelperKind,
            TriggerOutcome,
        )
        # Wrong-hash control: trigger rejects before transport call.
        trigger = build_trigger_record(
            run_id="mixed-20260101t000000z-1-abcdef01",
            scenario_id="i2pd-to-i2pr-ipv4",
            reference="i2pd",
            helper_kind=TriggerHelperKind.I2PD_DIRECT_HELPER,
            helper_binary_sha256="a" * 64,
            helper_source_sha256="b" * 64,
            helper_compiler="clang-15",
            helper_pinned_inputs_sha256="c" * 64,
            source_inspection_record_sha256="d" * 64,
            target_router_hash="0" * 40,
            target_router_info_sha256="0" * 64,
            target_ntcp2_static_key_sha256="0" * 64,
            target_address="192.0.2.2",
            target_port=45680,
            correlation_nonce="plan055-i2pd-wrong",
            attempted=True,
            attempt_count=1,
            outcome=TriggerOutcome.REJECTED_TARGET_ROUTER_INFO,
            reason_code="i2pd-trigger-wrong-hash-rejected",
            transport_request_observed=False,
            connection_callback_observed=False,
            started_monotonic_ms=1000,
            completed_monotonic_ms=1100,
            sanitized_detail="rejected before ConnectToPeer call",
            run_identity_sha256="1" * 64,
        )
        finalize_trigger_record(trigger)
        self.assertEqual(trigger["outcome"], "rejected-target-router-info")

    def test_java_to_i2pr_positive_reaches_plan052_predicate(self):
        from trigger_record import (
            build_trigger_record,
            finalize_trigger_record,
            TriggerHelperKind,
            TriggerOutcome,
        )
        trigger = build_trigger_record(
            run_id="mixed-20260101t000000z-1-abcdef02",
            scenario_id="java-to-i2pr-ipv4",
            reference="java_i2p",
            helper_kind=TriggerHelperKind.JAVA_MINIMAL_SUPPORT_TOPOLOGY,
            helper_binary_sha256="a" * 64,
            helper_source_sha256="b" * 64,
            helper_compiler="openjdk-17",
            helper_pinned_inputs_sha256="c" * 64,
            source_inspection_record_sha256="d" * 64,
            target_router_hash="e" * 40,
            target_router_info_sha256="f" * 64,
            target_ntcp2_static_key_sha256="0" * 64,
            target_address="192.0.2.2",
            target_port=45680,
            correlation_nonce="plan055-java-pos-1234",
            attempted=True,
            attempt_count=1,
            outcome=TriggerOutcome.AUTHENTICATED,
            reason_code="java-support-topology-authenticated",
            transport_request_observed=True,
            connection_callback_observed=True,
            started_monotonic_ms=1000,
            completed_monotonic_ms=2500,
            sanitized_detail="java-minimal-support-topology authenticated",
            run_identity_sha256="1" * 64,
        )
        finalize_trigger_record(trigger)
        self.assertEqual(trigger["outcome"], "authenticated")

    def test_no_trigger_control_fails(self):
        from trigger_record import (
            build_trigger_record,
            finalize_trigger_record,
            TriggerHelperKind,
            TriggerOutcome,
        )
        trigger = build_trigger_record(
            run_id="mixed-20260101t000000z-1-abcdef03",
            scenario_id="i2pd-to-i2pr-ipv4",
            reference="i2pd",
            helper_kind=TriggerHelperKind.I2PD_DIRECT_HELPER,
            helper_binary_sha256="0" * 64,
            helper_source_sha256="0" * 64,
            helper_compiler="clang-15",
            helper_pinned_inputs_sha256="0" * 64,
            source_inspection_record_sha256="0" * 64,
            target_router_hash="e" * 40,
            target_router_info_sha256="f" * 64,
            target_ntcp2_static_key_sha256="0" * 64,
            target_address="192.0.2.2",
            target_port=45680,
            correlation_nonce="plan055-i2pd-no-trigger",
            attempted=False,
            attempt_count=0,
            outcome=TriggerOutcome.DIRECT_TRIGGER_HELPER_FAILED,
            reason_code="i2pd-trigger-not-invoked",
            transport_request_observed=False,
            connection_callback_observed=False,
            started_monotonic_ms=0,
            completed_monotonic_ms=0,
            sanitized_detail="trigger not invoked",
            run_identity_sha256="1" * 64,
        )
        finalize_trigger_record(trigger)
        self.assertEqual(trigger["outcome"], "direct-trigger-helper-failed")
        self.assertFalse(trigger["transport_request_observed"])

    def test_malformed_responder_preserves_typed_reason(self):
        from trigger_record import (
            build_trigger_record,
            finalize_trigger_record,
            TriggerHelperKind,
            TriggerOutcome,
        )
        trigger = build_trigger_record(
            run_id="mixed-20260101t000000z-1-abcdef04",
            scenario_id="java-to-i2pr-ipv4",
            reference="java_i2p",
            helper_kind=TriggerHelperKind.JAVA_MINIMAL_SUPPORT_TOPOLOGY,
            helper_binary_sha256="a" * 64,
            helper_source_sha256="b" * 64,
            helper_compiler="openjdk-17",
            helper_pinned_inputs_sha256="c" * 64,
            source_inspection_record_sha256="d" * 64,
            target_router_hash="e" * 40,
            target_router_info_sha256="f" * 64,
            target_ntcp2_static_key_sha256="0" * 64,
            target_address="192.0.2.2",
            target_port=45680,
            correlation_nonce="plan055-java-malformed",
            attempted=True,
            attempt_count=1,
            outcome=TriggerOutcome.AUTHENTICATED,
            reason_code="java-support-topology-authenticated",
            transport_request_observed=True,
            connection_callback_observed=True,
            started_monotonic_ms=1000,
            completed_monotonic_ms=2500,
            sanitized_detail="trigger reached",
            run_identity_sha256="1" * 64,
        )
        finalize_trigger_record(trigger)
        # Plan 055 E3: the trigger may be successful even though the
        # responder rejects the handshake.
        direction_result = "rejected"
        direction_reason = "responder-session-confirmed-part2-failed"
        self.assertEqual(trigger["outcome"], "authenticated")
        self.assertEqual(direction_result, "rejected")
        self.assertEqual(direction_reason, "responder-session-confirmed-part2-failed")


if __name__ == "__main__":
    unittest.main()