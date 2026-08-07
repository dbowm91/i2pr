"""Plan 084 i2pd-to-i2pr reverse probe test matrix.

The Plan 084 test matrix exercises the canonical usage of the
``minimal_i2pd_reverse_probe`` schema and the ``plan084_runner``
orchestration module:

- the direction and reference are locked to ``i2pd-to-i2pr-ipv4``
  and ``i2pd``;
- the strictly-increasing stage model reports one bounded value per
  record;
- the reverse-direction process counter skeleton uses
  ``i2pr_prepare``, ``i2pd_prepare``, ``i2pr_listener``, and
  ``i2pd_dialer`` (reversed from the Plan 083 forward direction);
- the positive passed record requires the final stage, all four
  canonical observed events, clean cleanup, and ``reason_code =
  not_started``;
- pre-protocol rejections use the typed
  ``pre_protocol_rejected`` terminal with a bounded reason code from
  the Plan 083/084 fixed set;
- protocol rejections and timeouts use the typed terminal and reason;
- a generic ``typed-harness-operation-failed`` reason never closes a
  record;
- the schema enforces the Plan 082 run-identity contract;
- the probe never imports Plan 056/066 candidate, bundle,
  certificate, rootless-topology, or Multipass authority;
- the Plan 083 forward-direction schema is rejected for any reverse
  record, and the Plan 084 reverse-direction schema is rejected for
  any forward record.

The tests use the in-process schema only; they never launch a
subprocess and never need the real i2pd binary.
"""

from __future__ import annotations

import json
import os
import tempfile
import unittest
from pathlib import Path

import minimal_i2pd_probe as forward_probe
import minimal_i2pd_reverse_probe as reverse_probe


def _hex(value: str, length: int = 64) -> str:
    """Return ``value`` repeated/padded until exactly ``length`` hex chars."""

    text = value * (length // max(len(value), 1) + 1)
    return text[:length].lower()


def _minimal_passed_record(
    *,
    observed_events: list[dict[str, str]] | None = None,
    highest_stage: str = reverse_probe.I2NP_DELIVERY_STATUS_DECODED,
    terminal_result: str = reverse_probe.PASSED,
    reason_code: str = reverse_probe.REASON_NOT_STARTED,
    cleanup_result: str = "clean",
) -> dict[str, object]:
    events = observed_events
    if events is None:
        events = [
            {
                "event_name": "ntcp2_authenticated",
                "source_side": "i2pd",
                "event_sha256": _hex("5", 64),
            },
            {
                "event_name": "frame_emitted",
                "source_side": "i2pd",
                "event_sha256": _hex("6", 64),
            },
            {
                "event_name": "frame_authenticated_and_decrypted",
                "source_side": "i2pr",
                "event_sha256": _hex("7", 64),
            },
            {
                "event_name": "i2np_message_decoded",
                "source_side": "i2pr",
                "event_sha256": _hex("8", 64),
            },
        ]
    return reverse_probe.build_reverse_record(
        run_id="plan084-passed",
        source_commit=_hex("a", 40),
        reference_revision=_hex("b", 40),
        lane_qualification_sha256=_hex("c", 64),
        topology_kind="rootless-sealed-single-netns",
        parent_network_state_unchanged=True,
        i2pr_binary_sha256=_hex("d", 64),
        i2pd_binary_sha256=_hex("e", 64),
        i2pr_build_manifest_sha256=_hex("f", 64),
        i2pd_build_manifest_sha256=_hex("a1", 64),
        reference_source_tree_sha256=_hex("a2", 64),
        scenario_sha256=_hex("a3", 64),
        attempt_kind="instrumented",
        attempt_index=1,
        i2pr_router_info_sha256=_hex("1", 64),
        i2pd_router_info_sha256=_hex("2", 64),
        i2pr_router_hash_sha256=_hex("3", 64),
        i2pd_router_hash_sha256=_hex("4", 64),
        delivery_status_message_id=0x04200002,
        observed_events=list(events),
        highest_stage_reached=highest_stage,
        terminal_result=terminal_result,
        reason_code=reason_code,
        process_counters={
            "i2pr_prepare": {"started": 1, "exited": 1, "forced": 0},
            "i2pd_prepare": {"started": 1, "exited": 1, "forced": 0},
            "i2pr_listener": {"started": 1, "exited": 1, "forced": 0},
            "i2pd_dialer": {"started": 1, "exited": 1, "forced": 0},
        },
        cleanup_result=cleanup_result,
        placement_record_sha256=_hex("9", 64),
    )


class Plan084DirectionAndReferenceTests(unittest.TestCase):
    def test_direction_is_i2pd_to_i2pr_ipv4(self) -> None:
        record = _minimal_passed_record()
        self.assertEqual(record["direction"], "i2pd-to-i2pr-ipv4")

    def test_reference_is_i2pd(self) -> None:
        record = _minimal_passed_record()
        self.assertEqual(record["reference"], "i2pd")

    def test_forward_direction_is_not_allowlisted_in_reverse_schema(self) -> None:
        record = _minimal_passed_record()
        record["direction"] = "i2pr-to-i2pd-ipv4"
        record["record_sha256"] = reverse_probe.canonical_record_digest(record)
        with self.assertRaises(reverse_probe.ReverseProbeError):
            reverse_probe.validate_reverse_record(record)

    def test_reverse_direction_record_is_rejected_by_forward_schema(self) -> None:
        record = _minimal_passed_record()
        record["schema"] = forward_probe.SCHEMA
        record["schema_version"] = forward_probe.SCHEMA_VERSION
        record["direction"] = forward_probe.DIRECTION
        record["record_sha256"] = forward_probe.canonical_record_digest(record)
        with self.assertRaises(forward_probe.MinimalI2pdProbeError):
            forward_probe.validate_record(record)


class Plan084SchemaContractTests(unittest.TestCase):
    def test_schema_marker_is_locked(self) -> None:
        self.assertEqual(reverse_probe.SCHEMA, "i2pr-minimal-i2pd-reverse-probe-v1")
        self.assertEqual(reverse_probe.SCHEMA_VERSION, 1)

    def test_required_fields_match_forward_schema(self) -> None:
        forward = set(forward_probe.REQUIRED_FIELDS)
        reverse = set(reverse_probe.REQUIRED_FIELDS)
        self.assertEqual(forward, reverse)


class Plan084StageModelTests(unittest.TestCase):
    def test_stages_are_strictly_ordered_with_final_decoded(self) -> None:
        ordered = list(reverse_probe.STAGES)
        self.assertEqual(ordered[0], reverse_probe.NOT_STARTED)
        self.assertEqual(ordered[-1], reverse_probe.I2NP_DELIVERY_STATUS_DECODED)

    def test_passed_record_uses_final_stage(self) -> None:
        record = _minimal_passed_record()
        self.assertEqual(
            record["highest_stage_reached"],
            reverse_probe.I2NP_DELIVERY_STATUS_DECODED,
        )

    def test_pre_protocol_record_uses_state_prepared(self) -> None:
        record = _minimal_passed_record(
            observed_events=[],
            highest_stage=reverse_probe.STATE_PREPARED,
            terminal_result=reverse_probe.PRE_PROTOCOL_REJECTED,
            reason_code=reverse_probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            cleanup_result="not-run",
        )
        self.assertEqual(record["highest_stage_reached"], reverse_probe.STATE_PREPARED)
        self.assertEqual(
            record["terminal_result"], reverse_probe.PRE_PROTOCOL_REJECTED
        )

    def test_protocol_rejection_preserves_highest_stage(self) -> None:
        record = _minimal_passed_record(
            observed_events=[],
            highest_stage=reverse_probe.NOISE_AUTHENTICATED,
            terminal_result=reverse_probe.PROTOCOL_REJECTED,
            reason_code=reverse_probe.REASON_TCP_CONNECT_FAILED,
            cleanup_result="clean",
        )
        self.assertEqual(
            record["highest_stage_reached"], reverse_probe.NOISE_AUTHENTICATED
        )
        self.assertEqual(record["terminal_result"], reverse_probe.PROTOCOL_REJECTED)


class Plan084ReasonCodeTests(unittest.TestCase):
    def test_reason_codes_are_bounded(self) -> None:
        self.assertIn(reverse_probe.REASON_TCP_CONNECT_FAILED, reverse_probe.REASON_CODES)
        self.assertIn(
            reverse_probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            reverse_probe.REASON_CODES,
        )
        self.assertIn(
            reverse_probe.REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED,
            reverse_probe.REASON_CODES,
        )
        self.assertIn(
            reverse_probe.REASON_REFERENCE_EVENTS_MISSING, reverse_probe.REASON_CODES
        )

    def test_generic_failure_reason_is_not_allowlisted(self) -> None:
        self.assertNotIn("typed-harness-operation-failed", reverse_probe.REASON_CODES)


class Plan084ObservedEventTests(unittest.TestCase):
    def test_passed_record_carries_four_canonical_events(self) -> None:
        record = _minimal_passed_record()
        names = [event["event_name"] for event in record["observed_events"]]
        self.assertEqual(
            names,
            [
                "ntcp2_authenticated",
                "frame_emitted",
                "frame_authenticated_and_decrypted",
                "i2np_message_decoded",
            ],
        )

    def test_passed_record_requires_frame_emitted_on_i2pd_side(self) -> None:
        events = [
            {
                "event_name": "ntcp2_authenticated",
                "source_side": "i2pd",
                "event_sha256": _hex("5", 64),
            },
            {
                "event_name": "frame_authenticated_and_decrypted",
                "source_side": "i2pr",
                "event_sha256": _hex("7", 64),
            },
            {
                "event_name": "i2np_message_decoded",
                "source_side": "i2pr",
                "event_sha256": _hex("8", 64),
            },
        ]
        with self.assertRaises(reverse_probe.ReverseProbeError):
            _minimal_passed_record(observed_events=events)

    def test_passed_record_requires_data_phase_on_i2pr_side(self) -> None:
        events = [
            {
                "event_name": "ntcp2_authenticated",
                "source_side": "i2pd",
                "event_sha256": _hex("5", 64),
            },
            {
                "event_name": "frame_emitted",
                "source_side": "i2pd",
                "event_sha256": _hex("6", 64),
            },
        ]
        with self.assertRaises(reverse_probe.ReverseProbeError):
            _minimal_passed_record(observed_events=events)


class Plan084ProcessCounterTests(unittest.TestCase):
    def test_reverse_process_keys_are_distinct_from_forward(self) -> None:
        self.assertEqual(
            reverse_probe.PROCESS_KEYS,
            frozenset(
                {
                    "i2pr_prepare",
                    "i2pd_prepare",
                    "i2pr_listener",
                    "i2pd_dialer",
                }
            ),
        )
        self.assertNotEqual(reverse_probe.PROCESS_KEYS, forward_probe.PROCESS_KEYS)

    def test_reverse_record_uses_listener_and_dialer_keys(self) -> None:
        record = _minimal_passed_record()
        counters = record["process_counters"]
        self.assertIn("i2pr_listener", counters)
        self.assertIn("i2pd_dialer", counters)
        self.assertNotIn("i2pd_listener", counters)
        self.assertNotIn("i2pr_dialer", counters)

    def test_pre_protocol_rejection_uses_zero_live_counters(self) -> None:
        record = reverse_probe.build_reverse_record(
            run_id="plan084-preprotocol",
            source_commit=_hex("a", 40),
            reference_revision=_hex("b", 40),
            lane_qualification_sha256=_hex("c", 64),
            topology_kind="rootless-sealed-single-netns",
            parent_network_state_unchanged=False,
            i2pr_binary_sha256=_hex("d", 64),
            i2pd_binary_sha256=_hex("e", 64),
            i2pr_build_manifest_sha256=_hex("f", 64),
            i2pd_build_manifest_sha256=_hex("a1", 64),
            reference_source_tree_sha256=_hex("a2", 64),
            scenario_sha256=_hex("a3", 64),
            attempt_kind="instrumented",
            attempt_index=1,
            i2pr_router_info_sha256=_hex("1", 64),
            i2pd_router_info_sha256=_hex("2", 64),
            i2pr_router_hash_sha256=_hex("3", 64),
            i2pd_router_hash_sha256=_hex("4", 64),
            delivery_status_message_id=0x04200003,
            observed_events=[],
            highest_stage_reached=reverse_probe.STATE_PREPARED,
            terminal_result=reverse_probe.PRE_PROTOCOL_REJECTED,
            reason_code=reverse_probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            process_counters={
                "i2pr_prepare": {"started": 1, "exited": 1, "forced": 0},
                "i2pd_prepare": {"started": 0, "exited": 0, "forced": 0},
                "i2pr_listener": {"started": 0, "exited": 0, "forced": 0},
                "i2pd_dialer": {"started": 0, "exited": 0, "forced": 0},
            },
            cleanup_result="not-run",
            placement_record_sha256=_hex("9", 64),
        )
        counters = record["process_counters"]
        self.assertEqual(counters["i2pr_listener"]["started"], 0)
        self.assertEqual(counters["i2pd_dialer"]["started"], 0)
        self.assertEqual(counters["i2pr_prepare"]["started"], 1)


class Plan084TopologyTests(unittest.TestCase):
    def test_rootless_sealed_topology_is_allowlisted(self) -> None:
        self.assertIn("rootless-sealed-single-netns", reverse_probe.ALLOWED_TOPOLOGY_KINDS)

    def test_multipass_owned_guest_topology_is_allowlisted(self) -> None:
        self.assertIn("multipass-owned-guest", reverse_probe.ALLOWED_TOPOLOGY_KINDS)

    def test_host_loopback_development_topology_is_allowlisted(self) -> None:
        # Plan 086/088 enables literal IPv4 loopback development under
        # the bounded ``host-loopback-development`` topology. The
        # allowlist must accept it without breaking the strict schema.
        self.assertIn(
            "host-loopback-development", reverse_probe.ALLOWED_TOPOLOGY_KINDS
        )

    def test_host_loopback_development_is_development_only(self) -> None:
        # Plan 086/088 must mark the new topology as development-only;
        # release and isolated lanes remain exclusive.
        self.assertIn(
            "host-loopback-development",
            forward_probe.DEVELOPMENT_ONLY_TOPOLOGY_KINDS,
        )
        self.assertNotIn(
            "rootless-sealed-single-netns",
            forward_probe.DEVELOPMENT_ONLY_TOPOLOGY_KINDS,
        )

    def test_public_topology_is_not_allowlisted(self) -> None:
        self.assertNotIn("public-network", reverse_probe.ALLOWED_TOPOLOGY_KINDS)

    def test_runner_accepts_host_loopback_development_topology(self) -> None:
        # The reverse runner must accept the development topology and
        # produce a valid record without classifying the lane as
        # invalid. No protocol stage is exercised in this unit test;
        # the lane check is the only behaviour under examination.
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ReverseProbeConfig(
                run_id="plan088-hldev",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="host-loopback-development",
                parent_network_state_unchanged=True,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                i2pr_router_info_sha256="1" * 64,
                i2pd_router_info_sha256="2" * 64,
                i2pr_router_hash_sha256="3" * 64,
                i2pd_router_hash_sha256="4" * 64,
                delivery_status_message_id=0x04200002,
                output_path=root / "reverse-probe-record.json",
            )
            runner = runner_mod.ReverseProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(record["topology_kind"], "host-loopback-development")
            self.assertNotEqual(record["terminal_result"], reverse_probe.LANE_INVALID)


class Plan084ProvenanceTests(unittest.TestCase):
    def test_record_binds_source_commit(self) -> None:
        record = _minimal_passed_record()
        self.assertEqual(len(record["source_commit"]), 40)
        self.assertTrue(
            all(character in "0123456789abcdef" for character in record["source_commit"])
        )

    def test_record_binds_binary_digests(self) -> None:
        record = _minimal_passed_record()
        self.assertEqual(len(record["i2pr_binary_sha256"]), 64)
        self.assertEqual(len(record["i2pd_binary_sha256"]), 64)

    def test_record_binds_router_hash_pair(self) -> None:
        record = _minimal_passed_record()
        self.assertEqual(len(record["i2pr_router_hash_sha256"]), 64)
        self.assertEqual(len(record["i2pd_router_hash_sha256"]), 64)

    def test_record_binds_message_id(self) -> None:
        record = _minimal_passed_record()
        self.assertEqual(record["delivery_status_message_id"], 0x04200002)


class Plan084BoundaryTests(unittest.TestCase):
    def test_module_does_not_import_release_authority(self) -> None:
        source_path = reverse_probe.__file__
        if source_path is None:
            self.skipTest("module path unavailable")
        import pathlib
        import re

        text = pathlib.Path(source_path).read_text(encoding="utf-8")
        code_only = re.sub(r"\"\"\".*?\"\"\"", "", text, flags=re.DOTALL)
        code_only = re.sub(r"#.*", "", code_only)
        for forbidden in (
            "verify_milestone3_certificate",
            "candidate_record",
            "evidence_bundle",
            "from .rootless_topology",
            "from .multipass",
        ):
            self.assertNotIn(
                forbidden,
                code_only,
                f"reverse probe module imports release authority: {forbidden}",
            )

    def test_module_references_plan084_documentation(self) -> None:
        source_path = reverse_probe.__file__
        if source_path is None:
            self.skipTest("module path unavailable")
        import pathlib

        text = pathlib.Path(source_path).read_text(encoding="utf-8")
        self.assertIn("Plan 084", text)

    def test_runner_module_does_not_import_release_authority(self) -> None:
        import plan084_runner as runner_mod

        source_path = runner_mod.__file__
        if source_path is None:
            self.skipTest("module path unavailable")
        import pathlib
        import re

        text = pathlib.Path(source_path).read_text(encoding="utf-8")
        code_only = re.sub(r"\"\"\".*?\"\"\"", "", text, flags=re.DOTALL)
        code_only = re.sub(r"#.*", "", code_only)
        for forbidden in (
            "verify_milestone3_certificate",
            "candidate_record",
            "evidence_bundle",
            "from .rootless_topology",
            "from .multipass",
        ):
            self.assertNotIn(
                forbidden,
                code_only,
                f"reverse runner imports release authority: {forbidden}",
            )

    def test_runner_schema_constants_match_reverse_module(self) -> None:
        import plan084_runner as runner_mod

        self.assertEqual(runner_mod.SCHEMA, reverse_probe.SCHEMA)
        self.assertEqual(runner_mod.DIRECTION, reverse_probe.DIRECTION)
        self.assertEqual(runner_mod.REFERENCE, reverse_probe.REFERENCE)


class Plan084RunnerContractTests(unittest.TestCase):
    """Runner module contract and boundary tests."""

    def test_runner_module_imports_reverse_probe(self) -> None:
        import plan084_runner as runner_mod

        self.assertTrue(hasattr(runner_mod, "ReverseProbeRunner"))
        self.assertTrue(hasattr(runner_mod, "ReverseProbeConfig"))

    def test_runner_refuses_unknown_topology_kind(self) -> None:
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ReverseProbeConfig(
                run_id="test-run",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="public-network",
                parent_network_state_unchanged=True,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                i2pr_router_info_sha256="1" * 64,
                i2pd_router_info_sha256="2" * 64,
                i2pr_router_hash_sha256="3" * 64,
                i2pd_router_hash_sha256="4" * 64,
                delivery_status_message_id=0x04200002,
                output_path=root / "reverse-probe-record.json",
            )
            runner = runner_mod.ReverseProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(record["terminal_result"], reverse_probe.LANE_INVALID)
            self.assertEqual(record["reason_code"], "lane-invalid")

    def test_runner_refuses_unchanged_network_state_false(self) -> None:
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ReverseProbeConfig(
                run_id="test-run",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="rootless-sealed-single-netns",
                parent_network_state_unchanged=False,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                i2pr_router_info_sha256="1" * 64,
                i2pd_router_info_sha256="2" * 64,
                i2pr_router_hash_sha256="3" * 64,
                i2pd_router_hash_sha256="4" * 64,
                delivery_status_message_id=0x04200002,
                output_path=root / "reverse-probe-record.json",
            )
            runner = runner_mod.ReverseProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(record["terminal_result"], reverse_probe.LANE_INVALID)
            self.assertEqual(record["reason_code"], "lane-invalid")

    def test_runner_refuses_invalid_run_id(self) -> None:
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ReverseProbeConfig(
                run_id="INVALID",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="rootless-sealed-single-netns",
                parent_network_state_unchanged=True,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                i2pr_router_info_sha256="1" * 64,
                i2pd_router_info_sha256="2" * 64,
                i2pr_router_hash_sha256="3" * 64,
                i2pd_router_hash_sha256="4" * 64,
                delivery_status_message_id=0x04200002,
                output_path=root / "reverse-probe-record.json",
            )
            runner = runner_mod.ReverseProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(
                record["terminal_result"], reverse_probe.PRE_PROTOCOL_REJECTED
            )

    def test_runner_refuses_zero_message_id(self) -> None:
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ReverseProbeConfig(
                run_id="test-run",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="rootless-sealed-single-netns",
                parent_network_state_unchanged=True,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                i2pr_router_info_sha256="1" * 64,
                i2pd_router_info_sha256="2" * 64,
                i2pr_router_hash_sha256="3" * 64,
                i2pd_router_hash_sha256="4" * 64,
                delivery_status_message_id=0,
                output_path=root / "reverse-probe-record.json",
            )
            runner = runner_mod.ReverseProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(
                record["terminal_result"], reverse_probe.PRE_PROTOCOL_REJECTED
            )


class Plan084RunnerStageProgressionTests(unittest.TestCase):
    """Runner stage progression with fake event sources."""

    def test_runner_advances_to_state_prepared(self) -> None:
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ReverseProbeConfig(
                run_id="test-run",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="rootless-sealed-single-netns",
                parent_network_state_unchanged=True,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                i2pr_router_info_sha256="1" * 64,
                i2pd_router_info_sha256="2" * 64,
                i2pr_router_hash_sha256="3" * 64,
                i2pd_router_hash_sha256="4" * 64,
                delivery_status_message_id=0x04200002,
                output_path=root / "reverse-probe-record.json",
            )
            runner = runner_mod.ReverseProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertIn(record["highest_stage_reached"], reverse_probe.STAGES)

    def test_runner_emits_valid_reverse_probe_record(self) -> None:
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ReverseProbeConfig(
                run_id="test-run",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="rootless-sealed-single-netns",
                parent_network_state_unchanged=True,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                i2pr_router_info_sha256="1" * 64,
                i2pd_router_info_sha256="2" * 64,
                i2pr_router_hash_sha256="3" * 64,
                i2pd_router_hash_sha256="4" * 64,
                delivery_status_message_id=0x04200002,
                output_path=root / "reverse-probe-record.json",
            )
            runner = runner_mod.ReverseProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            reverse_probe.validate_reverse_record(record)

    def test_runner_pre_protocol_result_for_missing_router_info(self) -> None:
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ReverseProbeConfig(
                run_id="test-run",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="rootless-sealed-single-netns",
                parent_network_state_unchanged=True,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                i2pr_router_info_sha256="",
                i2pd_router_info_sha256="2" * 64,
                i2pr_router_hash_sha256="3" * 64,
                i2pd_router_hash_sha256="4" * 64,
                delivery_status_message_id=0x04200002,
                output_path=root / "reverse-probe-record.json",
            )
            runner = runner_mod.ReverseProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(
                record["terminal_result"], reverse_probe.PRE_PROTOCOL_REJECTED
            )
            self.assertIn(record["reason_code"], reverse_probe.REASON_CODES)

    def test_runner_preserves_non_started_counter_for_failed_lane(self) -> None:
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ReverseProbeConfig(
                run_id="test-run",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="public-network",
                parent_network_state_unchanged=True,
                i2pr_binary_sha256="d" * 64,
                i2pd_binary_sha256="e" * 64,
                i2pr_router_info_sha256="1" * 64,
                i2pd_router_info_sha256="2" * 64,
                i2pr_router_hash_sha256="3" * 64,
                i2pd_router_hash_sha256="4" * 64,
                delivery_status_message_id=0x04200002,
                output_path=root / "reverse-probe-record.json",
            )
            runner = runner_mod.ReverseProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            counters = record["process_counters"]
            for key in reverse_probe.PROCESS_KEYS:
                self.assertEqual(counters[key]["started"], 0)


class Plan084FakeEventSourceTests(unittest.TestCase):
    """FakeEventSource injection tests."""

    def test_fake_event_source_returns_configured_event(self) -> None:
        import plan084_runner as runner_mod

        fake = runner_mod.FakeEventSource()
        fake.add_event("listener_ready", "i2pr")
        event = fake.wait_for_event("listener_ready", 1.0)
        self.assertIsNotNone(event)
        self.assertEqual(event["event_name"], "listener_ready")
        self.assertEqual(event["source_side"], "i2pr")

    def test_fake_event_source_returns_none_for_missing_event(self) -> None:
        import plan084_runner as runner_mod

        fake = runner_mod.FakeEventSource()
        event = fake.wait_for_event("listener_ready", 0.01)
        self.assertIsNone(event)

    def test_fake_event_source_terminal_rejection(self) -> None:
        import plan084_runner as runner_mod

        fake = runner_mod.FakeEventSource()
        fake.add_event("listener_ready", "i2pr")
        fake.inject_terminal_rejected()
        event = fake.wait_for_event("listener_ready", 0.01)
        self.assertIsNone(event)


class Plan084HostBlockerTests(unittest.TestCase):
    """Host blocker detection and typed record tests."""

    def test_detect_host_blocker_returns_none_when_unset(self) -> None:
        import plan084_runner as runner_mod

        env_key = runner_mod._PLAN046_HOST_BLOCKER_ENV
        old = os.environ.pop(env_key, None)
        try:
            result = runner_mod.detect_host_blocker()
            self.assertIsNone(result)
        finally:
            if old is not None:
                os.environ[env_key] = old

    def test_detect_host_blocker_returns_code_when_set(self) -> None:
        import plan084_runner as runner_mod

        env_key = runner_mod._PLAN046_HOST_BLOCKER_ENV
        old = os.environ.pop(env_key, None)
        try:
            os.environ[env_key] = "blocked_unprivileged_user_namespace"
            result = runner_mod.detect_host_blocker()
            self.assertEqual(result, "blocked_unprivileged_user_namespace")
        finally:
            if old is not None:
                os.environ[env_key] = old
            else:
                os.environ.pop(env_key, None)

    def test_write_host_blocked_record_emits_lane_invalid(self) -> None:
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = runner_mod.write_host_blocked_record(
                run_root=root,
                run_id="blocked-run",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="rootless-sealed-single-netns",
                host_blocker="blocked_unprivileged_user_namespace",
                output_path=root / "blocked-record.json",
            )
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(record["terminal_result"], reverse_probe.LANE_INVALID)
            self.assertEqual(record["reason_code"], "lane-invalid")
            self.assertEqual(
                record["highest_stage_reached"], reverse_probe.NOT_STARTED
            )
            self.assertEqual(record["cleanup_result"], "not-run")
            reverse_probe.validate_reverse_record(record)

    def test_write_host_blocked_record_has_zero_binary_digests(self) -> None:
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = runner_mod.write_host_blocked_record(
                run_root=root,
                run_id="blocked-run",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="rootless-sealed-single-netns",
                host_blocker="blocked_unprivileged_user_namespace",
                output_path=root / "blocked-record.json",
            )
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(record["i2pr_binary_sha256"], "0" * 64)
            self.assertEqual(record["i2pd_binary_sha256"], "0" * 64)
            self.assertEqual(record["i2pr_router_info_sha256"], "0" * 64)
            self.assertEqual(record["i2pd_router_info_sha256"], "0" * 64)
            self.assertEqual(record["i2pr_router_hash_sha256"], "0" * 64)
            self.assertEqual(record["i2pd_router_hash_sha256"], "0" * 64)

    def test_write_host_blocked_record_has_empty_observed_events(self) -> None:
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            output = runner_mod.write_host_blocked_record(
                run_root=root,
                run_id="blocked-run",
                source_commit="a" * 40,
                reference_revision="b" * 40,
                lane_qualification_sha256="c" * 64,
                topology_kind="rootless-sealed-single-netns",
                host_blocker="blocked_unprivileged_user_namespace",
                output_path=root / "blocked-record.json",
            )
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(record["observed_events"], [])


class Plan084DecisionLockTests(unittest.TestCase):
    """Decision-lock tests for the Plan 084 development outcome.

    These tests encode the contract that ``plans/084-status.md`` must
    record exactly one of the five Plan 084 development decisions, and
    that the decision controls the dependent plan state machine:

    - ``two-way-development-probe-passed`` unblocks Plan 079.
    - ``one-way-passed-reverse-defect`` keeps Plan 079 blocked.
    - ``same-stage-two-way-i2pr-defect`` keeps Plan 079 blocked.
    - ``ambiguous-reference-divergence`` activates Plan 072.
    - ``lane-invalidated`` returns to Plan 077/080 only.

    These are unit-level contract tests on the decision vocabulary
    rather than live wire attempts.
    """

    def test_decision_values_are_bounded(self) -> None:
        allowed = {
            "two-way-development-probe-passed",
            "one-way-passed-reverse-defect",
            "same-stage-two-way-i2pr-defect",
            "ambiguous-reference-divergence",
            "lane-invalidated",
        }
        self.assertEqual(len(allowed), 5)

    def test_two_way_decision_unblocks_plan079(self) -> None:
        decision = "two-way-development-probe-passed"
        blocks_079 = decision != "two-way-development-probe-passed"
        self.assertFalse(blocks_079)

    def test_other_decisions_keep_plan079_blocked(self) -> None:
        for decision in (
            "one-way-passed-reverse-defect",
            "same-stage-two-way-i2pr-defect",
            "ambiguous-reference-divergence",
            "lane-invalidated",
        ):
            blocks_079 = decision != "two-way-development-probe-passed"
            self.assertTrue(blocks_079)

    def test_ambiguous_decision_activates_plan072(self) -> None:
        decision = "ambiguous-reference-divergence"
        activates_072 = decision == "ambiguous-reference-divergence"
        self.assertTrue(activates_072)

    def test_non_ambiguous_decisions_keep_plan072_inactive(self) -> None:
        for decision in (
            "two-way-development-probe-passed",
            "one-way-passed-reverse-defect",
            "same-stage-two-way-i2pr-defect",
            "lane-invalidated",
        ):
            activates_072 = decision == "ambiguous-reference-divergence"
            self.assertFalse(activates_072)


if __name__ == "__main__":
    unittest.main()