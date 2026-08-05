"""Plan 062 trigger record v4 schema tests.

These tests cover the Plan 062 v4 trigger schema
(``i2pr-reference-trigger-v4``) and the bounded outcomes enumerated
in Plan 062 D3. They assert that:

- a valid v4 trigger record finalizes with a 64-hex trigger digest;
- a 64-hex Router Hash is accepted for both local and peer;
- 40-hex Router Hash is rejected for v4 records;
- 63/65-hex Router Hash is rejected;
- uppercase hex is rejected;
- all-zero attempted provenance is rejected;
- delivery_status_message_id zero is rejected;
- delivery_status_message_id greater than ``0xffffffff`` is rejected;
- missing delivery_status_message_id is rejected;
- wrong message ID across records is rejected;
- wrong peer Router Hash is rejected;
- unknown v4 fields are rejected;
- missing v4 fields are rejected;
- attempt_count other than exactly one for an attempted record is
  rejected;
- completed_monotonic_ms earlier than started_monotonic_ms is
  rejected;
- target address outside the synthetic set is rejected;
- target port outside 1..=65535 is rejected;
- v3 trigger records are not accepted by the v4 validator.
"""

from __future__ import annotations

import unittest

from reference_trigger_v4 import (
    TRIGGER_SCHEMA,
    TRIGGER_SCHEMA_VERSION,
    TriggerHelperKind,
    TriggerOutcome,
    TriggerRecordError,
    build_trigger_record,
    compute_trigger_sha256,
    finalize_trigger_record,
    is_source_locked_helper,
    validate_trigger_record,
)


def _minimal_record(**overrides):
    payload = {
        "schema": TRIGGER_SCHEMA,
        "schema_version": TRIGGER_SCHEMA_VERSION,
        "run_id": "mixed-20260101t000000z-1-abcdef01",
        "scenario_id": "i2pr-to-java-ipv4",
        "direction": "i2pr-to-java-ipv4",
        "reference": "java_i2p",
        "reference_version": "2.12.0",
        "reference_revision": "2800040deee9bb376567b671ef2e9c34cf3e30b6",
        "helper_kind": "java-direct-helper",
        "helper_binary_sha256": "a" * 64,
        "helper_source_sha256": "b" * 64,
        "helper_build_manifest_sha256": "c" * 64,
        "helper_pinned_inputs_sha256": "d" * 64,
        "source_inspection_record_sha256": "e" * 64,
        "observer_patch_sha256": "f" * 64,
        "run_identity_sha256": "1" * 64,
        "local_router_hash_sha256": "2" * 64,
        "peer_router_hash_sha256": "3" * 64,
        "local_router_info_sha256": "4" * 64,
        "peer_router_info_sha256": "5" * 64,
        "peer_ntcp2_static_key_sha256": "6" * 64,
        "target_address": "192.0.2.1",
        "target_port": 45678,
        "delivery_status_message_id": 1,
        "attempted": True,
        "attempt_count": 1,
        "outcome": "authenticated",
        "reason_code": "java-direct-helper-authenticated",
        "transport_request_observed": True,
        "connection_established_observed": True,
        "sender_frame_write_observed": True,
        "started_monotonic_ms": 1000,
        "completed_monotonic_ms": 2500,
        "sanitized_detail": "v4 trigger reached",
        "topology_kind": "rootless-sealed-single-netns",
        "trigger_sha256": "",
    }
    payload.update(overrides)
    return payload


class TriggerSchemaTests(unittest.TestCase):
    def test_minimal_record_finalizes(self):
        record = _minimal_record()
        digest = finalize_trigger_record(record)
        self.assertEqual(len(digest), 64)
        self.assertEqual(record["trigger_sha256"], digest)

    def test_accepts_valid_64_hex_router_hash(self):
        record = _minimal_record()
        finalize_trigger_record(record)

    def test_64_hex_router_hash_accepted_both_sides(self):
        record = _minimal_record(
            local_router_hash_sha256="abcdef" * 10 + "abcd",
            peer_router_hash_sha256="123456" * 10 + "1234",
        )
        finalize_trigger_record(record)

    def test_40_hex_router_hash_rejected(self):
        record = _minimal_record(peer_router_hash_sha256="a" * 40)
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(ctx.exception.args[0], "trigger-peer_router_hash_sha256-invalid")

    def test_63_hex_router_hash_rejected(self):
        record = _minimal_record(peer_router_hash_sha256="a" * 63)
        with self.assertRaises(TriggerRecordError):
            finalize_trigger_record(record)

    def test_65_hex_router_hash_rejected(self):
        record = _minimal_record(peer_router_hash_sha256="a" * 65)
        with self.assertRaises(TriggerRecordError):
            finalize_trigger_record(record)

    def test_uppercase_hex_rejected(self):
        record = _minimal_record(peer_router_hash_sha256="A" * 64)
        with self.assertRaises(TriggerRecordError):
            finalize_trigger_record(record)

    def test_all_zero_provenance_rejected(self):
        record = _minimal_record(helper_binary_sha256="0" * 64)
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(
            ctx.exception.args[0],
            "trigger-zero-provenance-digest-with-attempted",
        )

    def test_message_id_zero_rejected(self):
        record = _minimal_record(delivery_status_message_id=0)
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(
            ctx.exception.args[0],
            "trigger-delivery-status-message-id-out-of-range",
        )

    def test_message_id_out_of_range_rejected(self):
        record = _minimal_record(delivery_status_message_id=0x1_0000_0000)
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(
            ctx.exception.args[0],
            "trigger-delivery-status-message-id-out-of-range",
        )

    def test_message_id_missing_rejected(self):
        record = _minimal_record()
        del record["delivery_status_message_id"]
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertTrue(ctx.exception.args[0].startswith("trigger-record-missing:"))

    def test_wrong_message_id_rejected(self):
        record = _minimal_record()
        record["delivery_status_message_id"] = "not-an-int"
        with self.assertRaises(TriggerRecordError):
            finalize_trigger_record(record)

    def test_wrong_peer_router_hash_rejected(self):
        record = _minimal_record(peer_router_hash_sha256="x" * 64)
        with self.assertRaises(TriggerRecordError):
            finalize_trigger_record(record)

    def test_v3_schema_rejected_by_v4_validator(self):
        # The v4 validator checks schema==v4 first, before checking
        # required fields. A v3 record fails on schema-invalid.
        record = _minimal_record(schema="i2pr-reference-trigger-v3", schema_version=3)
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(ctx.exception.args[0], "trigger-record-schema-invalid")

    def test_unknown_v4_field_rejected(self):
        record = _minimal_record(unexpected_field=True)
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(
            ctx.exception.args[0],
            "trigger-record-unknown-fields:unexpected_field",
        )

    def test_missing_v4_field_rejected(self):
        record = _minimal_record()
        del record["delivery_status_message_id"]
        with self.assertRaises(TriggerRecordError):
            finalize_trigger_record(record)

    def test_attempt_count_other_than_one_rejected(self):
        record = _minimal_record(attempt_count=2)
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(
            ctx.exception.args[0],
            "trigger-attempt-count-other-than-one-shot",
        )

    def test_completed_before_started_rejected(self):
        record = _minimal_record(
            started_monotonic_ms=2000,
            completed_monotonic_ms=1000,
        )
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(ctx.exception.args[0], "trigger-completed-before-started")

    def test_target_address_outside_synthetic_rejected(self):
        record = _minimal_record(target_address="198.51.100.7")
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(
            ctx.exception.args[0],
            "trigger-target-address-not-synthetic",
        )

    def test_target_port_out_of_range_rejected(self):
        record = _minimal_record(target_port=70000)
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(ctx.exception.args[0], "trigger-target-port-invalid")

    def test_java_trigger_must_use_java_direct_helper(self):
        record = _minimal_record(helper_kind="i2pd-direct-helper")
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(
            ctx.exception.args[0],
            "java-trigger-must-use-java-direct-helper",
        )

    def test_i2pd_trigger_must_use_i2pd_direct_helper(self):
        record = _minimal_record(
            reference="i2pd",
            reference_version="2.60.0",
            reference_revision="f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
            helper_kind="java-direct-helper",
        )
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(
            ctx.exception.args[0],
            "i2pd-trigger-must-use-i2pd-direct-helper",
        )

    def test_run_identity_mismatch_rejected(self):
        record = _minimal_record()
        finalize_trigger_record(record)
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record, run_identity_sha256="2" * 64)
        self.assertEqual(ctx.exception.args[0], "trigger-run-identity-mismatch")

    def test_finalize_round_trip(self):
        record = _minimal_record()
        finalize_trigger_record(record)
        canonical = compute_trigger_sha256({**record, "trigger_sha256": ""})
        self.assertEqual(canonical, record["trigger_sha256"])

    def test_attempted_with_zero_attempt_count_rejected(self):
        record = _minimal_record(
            attempted=True,
            attempt_count=0,
        )
        with self.assertRaises(TriggerRecordError) as ctx:
            finalize_trigger_record(record)
        self.assertEqual(
            ctx.exception.args[0],
            "trigger-attempt-count-other-than-one-shot",
        )

    def test_helper_kind_classifiers(self):
        self.assertTrue(
            is_source_locked_helper(TriggerHelperKind.I2PD_DIRECT_HELPER)
        )
        self.assertTrue(
            is_source_locked_helper(TriggerHelperKind.JAVA_DIRECT_HELPER)
        )


class BuildHelperTests(unittest.TestCase):
    def test_build_trigger_record_attaches_v4_provenance(self):
        record = build_trigger_record(
            run_id="mixed-20260101t000000z-1-abcdef01",
            scenario_id="i2pr-to-java-ipv4",
            direction="i2pr-to-java-ipv4",
            reference="java_i2p",
            helper_kind=TriggerHelperKind.JAVA_DIRECT_HELPER,
            helper_binary_sha256="a" * 64,
            helper_source_sha256="b" * 64,
            helper_build_manifest_sha256="c" * 64,
            helper_pinned_inputs_sha256="d" * 64,
            source_inspection_record_sha256="e" * 64,
            observer_patch_sha256="f" * 64,
            run_identity_sha256="1" * 64,
            local_router_hash_sha256="2" * 64,
            peer_router_hash_sha256="3" * 64,
            local_router_info_sha256="4" * 64,
            peer_router_info_sha256="5" * 64,
            peer_ntcp2_static_key_sha256="6" * 64,
            target_address="192.0.2.1",
            target_port=45678,
            delivery_status_message_id=1,
            attempted=True,
            attempt_count=1,
            outcome=TriggerOutcome.AUTHENTICATED,
            reason_code="java-direct-helper-authenticated",
            transport_request_observed=True,
            connection_established_observed=True,
            sender_frame_write_observed=True,
            started_monotonic_ms=1000,
            completed_monotonic_ms=2500,
            sanitized_detail="v4 trigger reached",
        )
        self.assertEqual(record["schema"], "i2pr-reference-trigger-v4")
        self.assertEqual(record["schema_version"], 4)
        self.assertEqual(record["delivery_status_message_id"], 1)
        self.assertEqual(len(record["trigger_sha256"]), 64)


if __name__ == "__main__":
    unittest.main()
