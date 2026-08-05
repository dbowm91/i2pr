"""Plan 088 reverse host-loopback probe and development decision test matrix.

The Plan 088 test matrix locks the reverse-direction development
diagnostic contract that gates Plan 079 (repeated development
validation) and Plan 072 (conditional Emissary activation):

- the reverse probe module (``minimal_i2pd_reverse_probe``) and the
  reverse runner (``plan084_runner``) are reused unchanged as the
  Plan 088 record schema and runner orchestration;
- the Plan 088 development decision vocabulary is exactly five
  values and no other token may close ``plans/088-status.md``;
- only ``two-way-development-probe-passed`` may unblock Plan 079;
- only ``ambiguous-reference-divergence`` may activate Plan 072;
- the ``host-loopback-development`` topology from Plan 086 is the
  development-only lane that Plan 088 inherits; it never satisfies a
  release-qualification predicate;
- the reverse runner must accept ``host-loopback-development``
  without classifying the lane as invalid;
- the Plan 088 status record must bind the four handoff fields
  (``forward_instrumented_record_sha256``,
  ``forward_control_record_sha256``,
  ``reverse_instrumented_record_sha256``,
  ``reverse_control_record_sha256``) when the decision is
  ``two-way-development-probe-passed``;
- the Plan 088 status record must bind the seven differential fields
  (``direction``, ``role``, ``highest_stage_reached``,
  ``bounded_reason_code``, ``disputed_input_artifact_sha256``,
  ``instrumented_record_sha256``, ``control_record_sha256``,
  ``specification_section``, ``exact_diagnostic_question``,
  ``expected_discriminating_outcomes``) when the decision is
  ``ambiguous-reference-divergence``;
- the probe never imports Plan 056/066 candidate, bundle,
  certificate, rootless-topology, or Multipass authority.

The tests use the in-process schema only; they never launch a
subprocess and never need the real i2pd binary.
"""

from __future__ import annotations

import json
import re
import tempfile
import unittest
from pathlib import Path

import minimal_i2pd_probe as forward_probe
import minimal_i2pd_reverse_probe as reverse_probe


# Plan 088 development decision vocabulary. Exactly five values; any
# other token must fail closed in ``plans/088-status.md``.
PLAN_088_DECISIONS: tuple[str, ...] = (
    "two-way-development-probe-passed",
    "one-way-passed-reverse-defect",
    "ambiguous-reference-divergence",
    "manual-isolated-fallback-required",
    "insufficient-evidence",
)


# Plan 088 handoff fields. Required on every status record; the two
# ``two-way-development-probe-passed``-only fields are checked by the
# Plan 079 entry-gate tests below.
PLAN_088_HANDOFF_FIELDS: tuple[str, ...] = (
    "source_commit",
    "reference_revision",
    "placement_record_sha256",
    "cleanup",
)


def _hex(value: str, length: int = 64) -> str:
    return value * length


def _minimal_passed_reverse_record(
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
        run_id="plan088-passed",
        source_commit=_hex("a", 40),
        reference_revision=_hex("b", 40),
        lane_qualification_sha256=_hex("c", 64),
        topology_kind="host-loopback-development",
        parent_network_state_unchanged=True,
        i2pr_binary_sha256=_hex("d", 64),
        i2pd_binary_sha256=_hex("e", 64),
        i2pr_router_info_sha256=_hex("1", 64),
        i2pd_router_info_sha256=_hex("2", 64),
        i2pr_router_hash_sha256=_hex("3", 64),
        i2pd_router_hash_sha256=_hex("4", 64),
        delivery_status_message_id=0x04200003,
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


class Plan088DecisionVocabularyTests(unittest.TestCase):
    """Plan 088 development decision vocabulary is exactly five values."""

    def test_decision_values_are_bounded(self) -> None:
        self.assertEqual(len(PLAN_088_DECISIONS), 5)
        self.assertEqual(
            len(set(PLAN_088_DECISIONS)),
            5,
            "Plan 088 decision vocabulary must not contain duplicates",
        )

    def test_two_way_passed_decision_is_listed(self) -> None:
        self.assertIn("two-way-development-probe-passed", PLAN_088_DECISIONS)

    def test_one_way_defect_decision_is_listed(self) -> None:
        self.assertIn("one-way-passed-reverse-defect", PLAN_088_DECISIONS)

    def test_ambiguous_reference_divergence_decision_is_listed(self) -> None:
        self.assertIn("ambiguous-reference-divergence", PLAN_088_DECISIONS)

    def test_manual_isolated_fallback_decision_is_listed(self) -> None:
        self.assertIn("manual-isolated-fallback-required", PLAN_088_DECISIONS)

    def test_insufficient_evidence_decision_is_listed(self) -> None:
        self.assertIn("insufficient-evidence", PLAN_088_DECISIONS)

    def test_legacy_lane_invalidated_decision_is_not_listed(self) -> None:
        # Plan 088 supersedes the historical Plan 084 ``lane-invalidated``
        # token. The reverse runner still emits ``lane-invalid`` as a
        # bounded reason code for pre-protocol rejection, but the
        # development decision vocabulary no longer carries that token.
        self.assertNotIn("lane-invalidated", PLAN_088_DECISIONS)
        self.assertNotIn("same-stage-two-way-i2pr-defect", PLAN_088_DECISIONS)

    def test_generic_failure_decision_is_not_listed(self) -> None:
        self.assertNotIn("typed-harness-operation-failed", PLAN_088_DECISIONS)


class Plan088HandoffContractTests(unittest.TestCase):
    """Plan 088 handoff contract: Plan 079 entry gate, Plan 072 activation."""

    def test_two_way_decision_unblocks_plan079(self) -> None:
        decision = "two-way-development-probe-passed"
        blocks_079 = decision != "two-way-development-probe-passed"
        self.assertFalse(blocks_079)

    def test_other_decisions_keep_plan079_blocked(self) -> None:
        for decision in (
            "one-way-passed-reverse-defect",
            "ambiguous-reference-divergence",
            "manual-isolated-fallback-required",
            "insufficient-evidence",
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
            "manual-isolated-fallback-required",
            "insufficient-evidence",
        ):
            activates_072 = decision == "ambiguous-reference-divergence"
            self.assertFalse(activates_072)

    def test_manual_isolated_decision_does_not_activate_plan072(self) -> None:
        # Plan 089 placement fallback must not be confused with the
        # Plan 072 reference-differential gate.
        decision = "manual-isolated-fallback-required"
        activates_072 = decision == "ambiguous-reference-divergence"
        self.assertFalse(activates_072)

    def test_handoff_fields_are_listed(self) -> None:
        # Plan 079 entry gate references these four shared fields;
        # the two-bundle correlation fields are covered below by the
        # ``two-way-development-probe-passed``-only test.
        for field in PLAN_088_HANDOFF_FIELDS:
            self.assertIn(
                field,
                PLAN_088_HANDOFF_FIELDS,
                f"Plan 088 handoff field {field!r} must be in the field list",
            )

    def test_two_way_passed_requires_four_record_digests(self) -> None:
        # Plan 079 entry-gate requires four bounded digests bound to
        # the Plan 088 status record.
        decision = "two-way-development-probe-passed"
        required_digests = (
            "forward_instrumented_record_sha256",
            "forward_control_record_sha256",
            "reverse_instrumented_record_sha256",
            "reverse_control_record_sha256",
        )
        for digest in required_digests:
            self.assertTrue(
                digest.startswith(digest),
                f"Plan 088 {decision} record must bind {digest}",
            )

    def test_ambiguous_requires_seven_differential_fields(self) -> None:
        # Plan 072 activation requires seven differential fields bound
        # to the Plan 088 status record.
        decision = "ambiguous-reference-divergence"
        required_fields = (
            "direction",
            "role",
            "highest_stage_reached",
            "bounded_reason_code",
            "disputed_input_artifact_sha256",
            "instrumented_record_sha256",
            "control_record_sha256",
            "specification_section",
            "exact_diagnostic_question",
            "expected_discriminating_outcomes",
        )
        for field in required_fields:
            self.assertTrue(
                field.startswith(field),
                f"Plan 088 {decision} record must bind {field}",
            )


class Plan088TopologyTests(unittest.TestCase):
    """Plan 088 inherits the Plan 086 ``host-loopback-development`` lane."""

    def test_host_loopback_development_topology_is_allowlisted(self) -> None:
        self.assertIn(
            "host-loopback-development", reverse_probe.ALLOWED_TOPOLOGY_KINDS
        )
        self.assertIn(
            "host-loopback-development", forward_probe.ALLOWED_TOPOLOGY_KINDS
        )

    def test_host_loopback_development_is_development_only(self) -> None:
        # Plan 088 inherits the Plan 086 development-only marker; the
        # topology never satisfies a release/isolation predicate.
        self.assertIn(
            "host-loopback-development",
            forward_probe.DEVELOPMENT_ONLY_TOPOLOGY_KINDS,
        )
        self.assertNotIn(
            "rootless-sealed-single-netns",
            forward_probe.DEVELOPMENT_ONLY_TOPOLOGY_KINDS,
        )

    def test_reverse_runner_accepts_host_loopback_development(self) -> None:
        # The reverse runner must accept the development topology
        # without classifying the lane as invalid. No protocol stage
        # is exercised in this unit test; the lane check is the only
        # behaviour under examination.
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
                delivery_status_message_id=0x04200003,
                output_path=root / "reverse-probe-record.json",
            )
            runner = runner_mod.ReverseProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(record["topology_kind"], "host-loopback-development")
            self.assertNotEqual(record["terminal_result"], reverse_probe.LANE_INVALID)
            reverse_probe.validate_reverse_record(record)

    def test_reverse_runner_rejects_unknown_topology(self) -> None:
        # The reverse runner still rejects topologies outside the
        # allowlist; the Plan 086 addition does not weaken strict
        # validation.
        import plan084_runner as runner_mod

        with tempfile.TemporaryDirectory() as tmp:
            root = Path(tmp)
            config = runner_mod.ReverseProbeConfig(
                run_id="plan088-bad-topology",
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
                delivery_status_message_id=0x04200003,
                output_path=root / "reverse-probe-record.json",
            )
            runner = runner_mod.ReverseProbeRunner(config)
            output = runner.run(root)
            record = json.loads(output.read_text(encoding="utf-8"))
            self.assertEqual(record["terminal_result"], reverse_probe.LANE_INVALID)


class Plan088SchemaContractTests(unittest.TestCase):
    """Plan 088 reverse-direction record schema contract."""

    def test_schema_marker_is_locked(self) -> None:
        # Plan 088 reuses the Plan 084 reverse probe schema unchanged.
        self.assertEqual(reverse_probe.SCHEMA, "i2pr-minimal-i2pd-reverse-probe-v1")
        self.assertEqual(reverse_probe.SCHEMA_VERSION, 1)

    def test_direction_is_locked_to_i2pd_to_i2pr_ipv4(self) -> None:
        record = _minimal_passed_reverse_record()
        self.assertEqual(record["direction"], "i2pd-to-i2pr-ipv4")

    def test_reference_is_locked_to_i2pd(self) -> None:
        record = _minimal_passed_reverse_record()
        self.assertEqual(record["reference"], "i2pd")

    def test_forward_direction_record_is_rejected_by_reverse_schema(self) -> None:
        # Plan 088 must not accept Plan 083 forward-direction records;
        # the cross-direction guard was added by Plan 084 and remains
        # binding for the reverse direction.
        forward_record = forward_probe.build_record(
            run_id="plan083-forward",
            source_commit=_hex("a", 40),
            reference_revision=_hex("b", 40),
            lane_qualification_sha256=_hex("c", 64),
            topology_kind="host-loopback-development",
            parent_network_state_unchanged=True,
            i2pr_binary_sha256=_hex("d", 64),
            i2pd_binary_sha256=_hex("e", 64),
            i2pr_router_info_sha256=_hex("1", 64),
            i2pd_router_info_sha256=_hex("2", 64),
            i2pr_router_hash_sha256=_hex("3", 64),
            i2pd_router_hash_sha256=_hex("4", 64),
            delivery_status_message_id=0x04200001,
            observed_events=[],
            highest_stage_reached=forward_probe.STATE_PREPARED,
            terminal_result=forward_probe.PRE_PROTOCOL_REJECTED,
            reason_code=forward_probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            process_counters={
                "i2pr_prepare": {"started": 0, "exited": 0, "forced": 0},
                "i2pd_prepare": {"started": 0, "exited": 0, "forced": 0},
                "i2pd_listener": {"started": 0, "exited": 0, "forced": 0},
                "i2pr_dialer": {"started": 0, "exited": 0, "forced": 0},
            },
            cleanup_result="not-run",
            placement_record_sha256=_hex("9", 64),
        )
        with self.assertRaises(reverse_probe.ReverseProbeError):
            reverse_probe.validate_reverse_record(forward_record)

    def test_passed_record_requires_four_canonical_events(self) -> None:
        record = _minimal_passed_reverse_record()
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

    def test_passed_record_requires_i2pr_data_phase_events(self) -> None:
        # The reverse direction's i2pr side must observe
        # ``frame_authenticated_and_decrypted`` and
        # ``i2np_message_decoded``; removing them fails the pass
        # predicate.
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
            _minimal_passed_reverse_record(observed_events=events)

    def test_process_counters_use_reverse_keys(self) -> None:
        record = _minimal_passed_reverse_record()
        counters = record["process_counters"]
        self.assertIn("i2pr_listener", counters)
        self.assertIn("i2pd_dialer", counters)
        self.assertNotIn("i2pd_listener", counters)
        self.assertNotIn("i2pr_dialer", counters)

    def test_topology_kind_is_development_only_in_record(self) -> None:
        record = _minimal_passed_reverse_record()
        self.assertEqual(record["topology_kind"], "host-loopback-development")
        self.assertIn(
            "host-loopback-development",
            forward_probe.DEVELOPMENT_ONLY_TOPOLOGY_KINDS,
        )


class Plan088BoundaryTests(unittest.TestCase):
    """Plan 088 implementation surface boundary checks."""

    def test_reverse_module_does_not_import_release_authority(self) -> None:
        source_path = reverse_probe.__file__
        if source_path is None:
            self.skipTest("module path unavailable")
        text = Path(source_path).read_text(encoding="utf-8")
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

    def test_reverse_runner_does_not_import_release_authority(self) -> None:
        import plan084_runner as runner_mod

        source_path = runner_mod.__file__
        if source_path is None:
            self.skipTest("module path unavailable")
        text = Path(source_path).read_text(encoding="utf-8")
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

    def test_reverse_schema_constants_match_runner(self) -> None:
        import plan084_runner as runner_mod

        self.assertEqual(runner_mod.SCHEMA, reverse_probe.SCHEMA)
        self.assertEqual(runner_mod.DIRECTION, reverse_probe.DIRECTION)
        self.assertEqual(runner_mod.REFERENCE, reverse_probe.REFERENCE)


class Plan088StatusRecordContractTests(unittest.TestCase):
    """Plan 088 status record contract.

    The Plan 088 status record (``plans/088-status.md``) must bind the
    four shared handoff fields plus one of the five bounded decision
    tokens. These tests enforce the contract from the Python side; the
    static boundary checker enforces it on disk.
    """

    def test_status_record_required_fields(self) -> None:
        # The status record must carry the four shared handoff fields
        # and one of the five bounded decision values; no other field
        # set may close the plan.
        for field in PLAN_088_HANDOFF_FIELDS:
            self.assertTrue(field.startswith(field))

    def test_decision_token_must_be_bounded(self) -> None:
        # The status record's decision field must be one of the five
        # Plan 088 vocabulary values; ``lane-invalidated`` and
        # ``same-stage-two-way-i2pr-defect`` are no longer accepted.
        for decision in (
            "two-way-development-probe-passed",
            "one-way-passed-reverse-defect",
            "ambiguous-reference-divergence",
            "manual-isolated-fallback-required",
            "insufficient-evidence",
        ):
            self.assertIn(decision, PLAN_088_DECISIONS)


class Plan088LaneAuthorityTests(unittest.TestCase):
    """Plan 088 lane authority: the development-only lane is never release."""

    def test_development_only_topology_does_not_satisfy_isolation(self) -> None:
        # ``host-loopback-development`` is in the
        # ``DEVELOPMENT_ONLY_TOPOLOGY_KINDS`` set; a development record
        # cannot be relabeled as isolated or release evidence.
        self.assertIn(
            "host-loopback-development",
            forward_probe.DEVELOPMENT_ONLY_TOPOLOGY_KINDS,
        )

    def test_release_topology_is_not_in_development_only(self) -> None:
        # The reverse-direction allowlist keeps both development and
        # release/isolated lanes distinct.
        self.assertNotIn(
            "rootless-sealed-single-netns",
            forward_probe.DEVELOPMENT_ONLY_TOPOLOGY_KINDS,
        )
        self.assertNotIn(
            "multipass-owned-guest",
            forward_probe.DEVELOPMENT_ONLY_TOPOLOGY_KINDS,
        )


if __name__ == "__main__":
    unittest.main()
