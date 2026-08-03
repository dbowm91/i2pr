"""Plan 083 minimal i2pd probe record schema tests.

The probe record schema is the bounded contract for the Plan 083
minimal ``i2pr -> i2pd`` wire probe. The tests exercise:

- the schema field allowlist and required-field list;
- the bounded stage, terminal-result, and reason-code sets;
- the process-counter skeleton and structure validation;
- the canonical record digest and digest-mismatch rejection;
- the positive passed-record rules (final stage, cleanup, observed
  events, reason code);
- the negative pre-protocol and protocol-rejected rules;
- the forbidden field list (raw payload, private key, Noise state,
  transcripts, RouterInfo bytes);
- the helper constructors for typed event summaries and the empty
  process counter skeleton.

The tests use the in-process schema only; they never launch a
subprocess and never need the real i2pd binary.
"""

from __future__ import annotations

import json
import unittest

import minimal_i2pd_probe as probe


def _hex(value: str, length: int = 64) -> str:
    return value * length


def _valid_record(**overrides: object) -> dict[str, object]:
    payload: dict[str, object] = {
        "schema": probe.SCHEMA,
        "schema_version": probe.SCHEMA_VERSION,
        "run_id": "minimal-probe-run-1",
        "source_commit": _hex("a", 40),
        "direction": probe.DIRECTION,
        "reference": probe.REFERENCE,
        "reference_revision": _hex("b", 40),
        "lane_qualification_sha256": _hex("c", 64),
        "topology_kind": "rootless-sealed-single-netns",
        "parent_network_state_unchanged": True,
        "i2pr_binary_sha256": _hex("d", 64),
        "i2pd_binary_sha256": _hex("e", 64),
        "i2pr_router_info_sha256": _hex("1", 64),
        "i2pd_router_info_sha256": _hex("2", 64),
        "i2pr_router_hash_sha256": _hex("3", 64),
        "i2pd_router_hash_sha256": _hex("4", 64),
        "delivery_status_message_id": 0x04200001,
        "observed_events": [
            {
                "event_name": "ntcp2_authenticated",
                "source_side": "i2pr",
                "event_sha256": _hex("5", 64),
            },
            {
                "event_name": "frame_emitted",
                "source_side": "i2pr",
                "event_sha256": _hex("6", 64),
            },
            {
                "event_name": "frame_authenticated_and_decrypted",
                "source_side": "i2pd",
                "event_sha256": _hex("7", 64),
            },
            {
                "event_name": "i2np_message_decoded",
                "source_side": "i2pd",
                "event_sha256": _hex("8", 64),
            },
        ],
        "highest_stage_reached": probe.I2NP_DELIVERY_STATUS_DECODED,
        "terminal_result": probe.PASSED,
        "reason_code": probe.REASON_NOT_STARTED,
        "process_counters": {
            "i2pr_prepare": {"started": 1, "exited": 1, "forced": 0},
            "i2pd_prepare": {"started": 1, "exited": 1, "forced": 0},
            "i2pd_listener": {"started": 1, "exited": 1, "forced": 0},
            "i2pr_dialer": {"started": 1, "exited": 1, "forced": 0},
        },
        "cleanup_result": "clean",
        "record_sha256": "",
    }
    payload.update(overrides)
    payload["record_sha256"] = probe.canonical_record_digest(payload)
    return payload


class SchemaContractTests(unittest.TestCase):
    def test_schema_marker_is_locked(self) -> None:
        self.assertEqual(probe.SCHEMA, "i2pr-minimal-i2pd-probe-v1")
        self.assertEqual(probe.SCHEMA_VERSION, 1)

    def test_direction_and_reference_are_locked(self) -> None:
        self.assertEqual(probe.DIRECTION, "i2pr-to-i2pd-ipv4")
        self.assertEqual(probe.REFERENCE, "i2pd")

    def test_stages_are_strictly_ordered(self) -> None:
        ordered = list(probe.STAGES)
        self.assertEqual(ordered[0], probe.NOT_STARTED)
        self.assertEqual(ordered[-1], probe.I2NP_DELIVERY_STATUS_DECODED)
        ranks = [probe.STAGE_RANK[stage] for stage in ordered]
        self.assertEqual(ranks, sorted(ranks))
        for stage in ordered:
            self.assertIn(stage, probe.STAGE_RANK)

    def test_terminal_results_include_passed_and_pre_protocol(self) -> None:
        for value in (
            probe.PASSED,
            probe.PROTOCOL_REJECTED,
            probe.PROTOCOL_TIMEOUT,
            probe.PRE_PROTOCOL_REJECTED,
            probe.CLEANUP_FAILED,
            probe.LANE_INVALID,
        ):
            self.assertIn(value, probe.TERMINAL_RESULTS)

    def test_required_fields_match_docstring(self) -> None:
        expected = {
            "schema",
            "schema_version",
            "run_id",
            "source_commit",
            "direction",
            "reference",
            "reference_revision",
            "lane_qualification_sha256",
            "topology_kind",
            "parent_network_state_unchanged",
            "i2pr_binary_sha256",
            "i2pd_binary_sha256",
            "i2pr_router_info_sha256",
            "i2pd_router_info_sha256",
            "i2pr_router_hash_sha256",
            "i2pd_router_hash_sha256",
            "delivery_status_message_id",
            "observed_events",
            "highest_stage_reached",
            "terminal_result",
            "reason_code",
            "process_counters",
            "cleanup_result",
            "record_sha256",
        }
        self.assertEqual(set(probe.REQUIRED_FIELDS), expected)

    def test_forbidden_fields_cover_secret_bearing_data(self) -> None:
        for field in (
            "raw_payload",
            "private_key",
            "noise_state",
            "router_info_bytes",
            "session_state",
            "static_key",
            "router_identity",
            "frame_transcript",
            "transcript_bytes",
        ):
            self.assertIn(field, probe.FORBIDDEN_FIELDS)


class RecordValidationTests(unittest.TestCase):
    def test_valid_passed_record_round_trips(self) -> None:
        record = _valid_record()
        validated = probe.validate_record(record)
        self.assertEqual(validated["direction"], probe.DIRECTION)
        self.assertEqual(validated["reference"], probe.REFERENCE)
        self.assertEqual(validated["highest_stage_reached"], probe.I2NP_DELIVERY_STATUS_DECODED)
        self.assertEqual(validated["terminal_result"], probe.PASSED)
        self.assertEqual(validated["cleanup_result"], "clean")

    def test_schema_mismatch_is_rejected(self) -> None:
        record = _valid_record(schema="i2pr-mismatch-v1")
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_unknown_topology_kind_is_rejected(self) -> None:
        record = _valid_record(topology_kind="public-network")
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_non_hex_router_hash_is_rejected(self) -> None:
        record = _valid_record(i2pr_router_hash_sha256="not-a-digest")
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_short_revision_is_rejected(self) -> None:
        record = _valid_record(reference_revision="cafebabedeadbeef")
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_zero_message_id_is_rejected(self) -> None:
        record = _valid_record(delivery_status_message_id=0)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_oversized_message_id_is_rejected(self) -> None:
        record = _valid_record(delivery_status_message_id=0x1_0000_0000)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_unknown_direction_is_rejected(self) -> None:
        record = _valid_record(direction="java-to-i2pr-ipv4")
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_unknown_reference_is_rejected(self) -> None:
        record = _valid_record(reference="java_i2p")
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_forbidden_secret_field_is_rejected(self) -> None:
        record = _valid_record(raw_payload="secret")
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_unknown_record_field_is_rejected(self) -> None:
        record = _valid_record()
        record["rogue_field"] = "nope"
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_digest_mismatch_is_rejected(self) -> None:
        record = _valid_record()
        record["record_sha256"] = _hex("0", 64)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_passed_record_requires_all_four_observed_events(self) -> None:
        events = [
            {
                "event_name": "ntcp2_authenticated",
                "source_side": "i2pr",
                "event_sha256": _hex("5", 64),
            },
        ]
        record = _valid_record(observed_events=events)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_passed_record_requires_final_stage(self) -> None:
        record = _valid_record(highest_stage_reached=probe.NOISE_AUTHENTICATED)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_passed_record_requires_clean_cleanup(self) -> None:
        record = _valid_record(cleanup_result="forced")
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_passed_record_requires_not_started_reason(self) -> None:
        record = _valid_record(reason_code=probe.REASON_TCP_CONNECT_FAILED)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_pre_protocol_rejected_record_is_accepted(self) -> None:
        record = _valid_record(
            highest_stage_reached=probe.STATE_PREPARED,
            terminal_result=probe.PRE_PROTOCOL_REJECTED,
            reason_code=probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            observed_events=[],
            cleanup_result="not-run",
        )
        validated = probe.validate_record(record)
        self.assertEqual(validated["terminal_result"], probe.PRE_PROTOCOL_REJECTED)

    def test_protocol_rejected_record_keeps_observed_events(self) -> None:
        record = _valid_record(
            highest_stage_reached=probe.TCP_CONNECTED,
            terminal_result=probe.PROTOCOL_REJECTED,
            reason_code=probe.REASON_TCP_CONNECT_FAILED,
            observed_events=[],
            cleanup_result="clean",
        )
        validated = probe.validate_record(record)
        self.assertEqual(validated["terminal_result"], probe.PROTOCOL_REJECTED)

    def test_protocol_timeout_record_uses_typed_reason(self) -> None:
        record = _valid_record(
            highest_stage_reached=probe.NOISE_AUTHENTICATED,
            terminal_result=probe.PROTOCOL_TIMEOUT,
            reason_code=probe.REASON_NOISE_SESSION_REQUEST_REJECTED,
            observed_events=[],
            cleanup_result="forced",
        )
        validated = probe.validate_record(record)
        self.assertEqual(validated["terminal_result"], probe.PROTOCOL_TIMEOUT)

    def test_generic_failure_reason_is_rejected(self) -> None:
        record = _valid_record(reason_code="typed-harness-operation-failed")
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)


class ProcessCounterTests(unittest.TestCase):
    def test_empty_counters_have_four_processes_and_zero_values(self) -> None:
        counters = probe.empty_process_counters()
        self.assertEqual(set(counters), probe.PROCESS_KEYS)
        for entry in counters.values():
            self.assertEqual(entry, {"started": 0, "exited": 0, "forced": 0})

    def test_validator_accepts_zero_counters(self) -> None:
        counters = probe.empty_process_counters()
        validated = probe.validate_process_counters(counters)
        for entry in validated.values():
            self.assertEqual(entry["started"], 0)

    def test_validator_rejects_unknown_process_key(self) -> None:
        counters = probe.empty_process_counters()
        counters["rogue_process"] = {"started": 0, "exited": 0, "forced": 0}
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_process_counters(counters)

    def test_validator_rejects_missing_counter_key(self) -> None:
        counters = probe.empty_process_counters()
        counters["i2pr_dialer"] = {"started": 0, "exited": 0}
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_process_counters(counters)

    def test_validator_rejects_negative_counter(self) -> None:
        counters = probe.empty_process_counters()
        counters["i2pr_dialer"]["started"] = -1
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_process_counters(counters)

    def test_validator_rejects_bool_counter(self) -> None:
        counters = probe.empty_process_counters()
        counters["i2pr_dialer"]["started"] = True
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_process_counters(counters)


class ObservedEventTests(unittest.TestCase):
    def test_validator_accepts_typed_event(self) -> None:
        event = {
            "event_name": "frame_authenticated_and_decrypted",
            "source_side": "i2pd",
            "event_sha256": _hex("9", 64),
        }
        validated = probe.validate_observed_event(event)
        self.assertEqual(validated["event_name"], "frame_authenticated_and_decrypted")

    def test_validator_rejects_unknown_event_name(self) -> None:
        event = {
            "event_name": "made_up_event",
            "source_side": "i2pd",
            "event_sha256": _hex("9", 64),
        }
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_observed_event(event)

    def test_validator_rejects_unknown_source_side(self) -> None:
        event = {
            "event_name": "tcp_connected",
            "source_side": "attacker",
            "event_sha256": _hex("9", 64),
        }
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_observed_event(event)

    def test_validator_rejects_non_hex_event_digest(self) -> None:
        event = {
            "event_name": "tcp_connected",
            "source_side": "i2pd",
            "event_sha256": "not-a-digest",
        }
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_observed_event(event)

    def test_validator_rejects_extra_field(self) -> None:
        event = {
            "event_name": "tcp_connected",
            "source_side": "i2pd",
            "event_sha256": _hex("9", 64),
            "raw_payload": "secret",
        }
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_observed_event(event)


class BuildRecordTests(unittest.TestCase):
    def test_build_record_emits_canonical_digest(self) -> None:
        record = probe.build_record(
            run_id="minimal-probe-run-2",
            source_commit=_hex("a", 40),
            reference_revision=_hex("b", 40),
            lane_qualification_sha256=_hex("c", 64),
            topology_kind="rootless-sealed-single-netns",
            parent_network_state_unchanged=True,
            i2pr_binary_sha256=_hex("d", 64),
            i2pd_binary_sha256=_hex("e", 64),
            i2pr_router_info_sha256=_hex("1", 64),
            i2pd_router_info_sha256=_hex("2", 64),
            i2pr_router_hash_sha256=_hex("3", 64),
            i2pd_router_hash_sha256=_hex("4", 64),
            delivery_status_message_id=0x04200002,
            observed_events=[
                {
                    "event_name": "ntcp2_authenticated",
                    "source_side": "i2pr",
                    "event_sha256": _hex("5", 64),
                },
                {
                    "event_name": "frame_emitted",
                    "source_side": "i2pr",
                    "event_sha256": _hex("6", 64),
                },
                {
                    "event_name": "frame_authenticated_and_decrypted",
                    "source_side": "i2pd",
                    "event_sha256": _hex("7", 64),
                },
                {
                    "event_name": "i2np_message_decoded",
                    "source_side": "i2pd",
                    "event_sha256": _hex("8", 64),
                },
            ],
            highest_stage_reached=probe.I2NP_DELIVERY_STATUS_DECODED,
            terminal_result=probe.PASSED,
            reason_code=probe.REASON_NOT_STARTED,
            process_counters={
                "i2pr_prepare": {"started": 1, "exited": 1, "forced": 0},
                "i2pd_prepare": {"started": 1, "exited": 1, "forced": 0},
                "i2pd_listener": {"started": 1, "exited": 1, "forced": 0},
                "i2pr_dialer": {"started": 1, "exited": 1, "forced": 0},
            },
            cleanup_result="clean",
        )
        self.assertEqual(record["schema"], probe.SCHEMA)
        self.assertEqual(record["record_sha256"], probe.canonical_record_digest(record))
        probe.validate_record(record)

    def test_build_record_rejects_zero_message_id(self) -> None:
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.build_record(
                run_id="x",
                source_commit=_hex("a", 40),
                reference_revision=_hex("b", 40),
                lane_qualification_sha256=_hex("c", 64),
                topology_kind="rootless-sealed-single-netns",
                parent_network_state_unchanged=False,
                i2pr_binary_sha256=_hex("d", 64),
                i2pd_binary_sha256=_hex("e", 64),
                i2pr_router_info_sha256=_hex("1", 64),
                i2pd_router_info_sha256=_hex("2", 64),
                i2pr_router_hash_sha256=_hex("3", 64),
                i2pd_router_hash_sha256=_hex("4", 64),
                delivery_status_message_id=0,
                observed_events=[],
                highest_stage_reached=probe.STATE_PREPARED,
                terminal_result=probe.PRE_PROTOCOL_REJECTED,
                reason_code=probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
                process_counters=probe.empty_process_counters(),
                cleanup_result="not-run",
            )

    def test_build_record_rejects_unknown_topology(self) -> None:
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.build_record(
                run_id="x",
                source_commit=_hex("a", 40),
                reference_revision=_hex("b", 40),
                lane_qualification_sha256=_hex("c", 64),
                topology_kind="public-network",
                parent_network_state_unchanged=False,
                i2pr_binary_sha256=_hex("d", 64),
                i2pd_binary_sha256=_hex("e", 64),
                i2pr_router_info_sha256=_hex("1", 64),
                i2pd_router_info_sha256=_hex("2", 64),
                i2pr_router_hash_sha256=_hex("3", 64),
                i2pd_router_hash_sha256=_hex("4", 64),
                delivery_status_message_id=1,
                observed_events=[],
                highest_stage_reached=probe.STATE_PREPARED,
                terminal_result=probe.PRE_PROTOCOL_REJECTED,
                reason_code=probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
                process_counters=probe.empty_process_counters(),
                cleanup_result="not-run",
            )


class CanonicalDigestTests(unittest.TestCase):
    def test_digest_is_stable_across_key_order(self) -> None:
        record = _valid_record()
        digest_a = probe.canonical_record_digest(record)
        reordered = {key: record[key] for key in reversed(record)}
        digest_b = probe.canonical_record_digest(reordered)
        self.assertEqual(digest_a, digest_b)

    def test_digest_excludes_self_field(self) -> None:
        record = _valid_record()
        expected = probe.canonical_record_digest({key: record[key] for key in record if key != "record_sha256"})
        self.assertEqual(expected, record["record_sha256"])

    def test_digest_round_trips_through_json(self) -> None:
        record = _valid_record()
        encoded = json.dumps(record, sort_keys=True, separators=(",", ":"))
        decoded = json.loads(encoded)
        self.assertEqual(probe.canonical_record_digest(decoded), record["record_sha256"])


if __name__ == "__main__":
    unittest.main()
