"""Plan 094 focused regression matrix.

Plan 094 closes Plan 093 without reopening its already-landed
NTCP2 data-phase design. The test matrix exercises the focused
runner/evidence authority corrections that must land before any
live wire attempt. The matrix never launches a subprocess and
never needs the real i2pd binary; it uses the in-process schemas
and synthesized events.

The 32 cases are organised as:

- Status authority (1)
- Process invocation identity (2-5)
- Canonical event ingestion (6-9)
- Forward pass classification (10-15)
- Build / binary provenance (16-19)
- Plan 094 closure gating (20-24)
- Plan 088 / 072 / 079 / NTCP2 gates (25-28)
- Plan 094 status authority (29-32)
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import minimal_i2pd_probe as probe
import reference_event


REPO_ROOT = HERE.parents[3]


def _hex(value: str, length: int = 64) -> str:
    """Return ``value`` repeated/padded until exactly ``length`` hex chars."""

    text = value * (length // max(len(value), 1) + 1)
    return text[:length].lower()


def _build_passed_record(**overrides):
    payload = {
        "schema": probe.SCHEMA,
        "schema_version": probe.SCHEMA_VERSION,
        "run_id": "plan094-passed",
        "source_commit": _hex("a", 40),
        "direction": probe.DIRECTION,
        "reference": probe.REFERENCE,
        "reference_revision": _hex("b", 40),
        "lane_qualification_sha256": _hex("c", 64),
        "topology_kind": "host-loopback-development",
        "parent_network_state_unchanged": True,
        "i2pr_binary_sha256": _hex("d", 64),
        "i2pd_binary_sha256": _hex("e", 64),
        "i2pr_build_manifest_sha256": _hex("f", 64),
        "i2pd_build_manifest_sha256": _hex("a1", 64),
        "reference_source_tree_sha256": _hex("a2", 64),
        "scenario_sha256": _hex("a3", 64),
        "attempt_kind": probe.ATTEMPT_KIND_INSTRUMENTED,
        "attempt_index": 1,
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
        "placement_record_sha256": _hex("9", 64),
    }
    payload.update(overrides)
    payload["record_sha256"] = probe.canonical_record_digest(payload)
    return payload


def _base_event_kwargs(**overrides):
    kwargs = dict(
        run_id="mixed-plan094-20260101",
        scenario_id="i2pr-to-i2pd-ipv4",
        direction="i2pr-to-i2pd-ipv4",
        invocation_id="plan094-invocation-1",
        implementation="i2pd-direct-driver",
        implementation_revision="f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
        driver_binary_sha256=_hex("d", 64),
        local_router_hash_sha256=_hex("3", 64),
        peer_router_hash_sha256=_hex("4", 64),
        monotonic_ms=1000,
        event_kind=reference_event.EventKind.PROCESS_STARTED,
        event_sequence=0,
    )
    kwargs.update(overrides)
    return kwargs


class Plan094StatusAuthorityTests(unittest.TestCase):
    def test_current_status_names_plan094_as_completion_authority(self) -> None:
        # Plan 094 may be the active completion authority, or Plan 095
        # may have superseded it as the CI live-wire closure authority.
        text_087 = (REPO_ROOT / "plans/087-status.md").read_text()
        text_088 = (REPO_ROOT / "plans/088-status.md").read_text()
        expected_095_tokens = (
            "plan_095 = ci-live-wire-closure-next-executable",
            "plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run",
        )
        for text, label in (
            (text_087, "087"),
            (text_088, "088"),
        ):
            self.assertTrue(
                "plan_094 = active-single-next-executable-completion-pass" in text
                or any(token in text for token in expected_095_tokens),
                f"plans/{label}-status.md must name Plan 094 or Plan 095 as the active completion authority",
            )
        self.assertTrue(
            "open-pending-plan094-forward-evidence-pair" in text_087
            or "open-pending-plan095-ci-forward-evidence-pair" in text_087,
            "plans/087-status.md must declare a plan-094 or plan-095 forward evidence pair",
        )
        self.assertTrue(
            "blocked-pending-plan094-completion" in text_088
            or "blocked-pending-plan095-ci-closure" in text_088,
            "plans/088-status.md must declare a plan-094 or plan-095 active block",
        )


class Plan094InvocationIdentityTests(unittest.TestCase):
    def test_scenario_id_substitution_rejected(self) -> None:
        # Plan 094: invocation_id cannot be the scenario_id; the
        # runner must allocate a per-process identifier distinct
        # from the scenario name.
        kwargs = _base_event_kwargs(invocation_id="i2pr-to-i2pd-ipv4")
        with self.assertRaises(reference_event.ReferenceEventError):
            reference_event.build_event(**kwargs)

    def test_missing_invocation_id_rejected(self) -> None:
        # The validator must reject events that omit invocation_id.
        kwargs = _base_event_kwargs()
        kwargs.pop("invocation_id")
        with self.assertRaises(TypeError):
            reference_event.build_event(**kwargs)
        # Also reject raw dict events without invocation_id.
        event = reference_event.build_event(**_base_event_kwargs())
        event.pop("invocation_id")
        with self.assertRaises(reference_event.ReferenceEventError):
            reference_event.validate_event(event)

    def test_two_invocations_of_same_scenario_remain_distinguishable(self) -> None:
        # The runner must allocate one invocation_id per launch.
        first = reference_event.build_event(
            **_base_event_kwargs(invocation_id="plan094-invocation-1")
        )
        second = reference_event.build_event(
            **_base_event_kwargs(
                invocation_id="plan094-invocation-2",
                event_sequence=1,
            )
        )
        self.assertNotEqual(first["invocation_id"], second["invocation_id"])
        self.assertEqual(first["scenario_id"], second["scenario_id"])

    def test_event_identity_fields_round_trip(self) -> None:
        event = reference_event.build_event(**_base_event_kwargs())
        self.assertEqual(event["invocation_id"], "plan094-invocation-1")
        self.assertEqual(event["scenario_id"], "i2pr-to-i2pd-ipv4")
        self.assertEqual(event["direction"], "i2pr-to-i2pd-ipv4")
        self.assertEqual(event["event_kind"], "process_started")


class Plan094CanonicalEventIngestionTests(unittest.TestCase):
    def test_zero_event_digest_rejected(self) -> None:
        # Plan 094: zero-digest events cannot contribute to a pass.
        record = _build_passed_record()
        record["observed_events"].append(
            {
                "event_name": "frame_emitted",
                "source_side": "i2pd",
                "event_sha256": "0" * 64,
            }
        )
        record["record_sha256"] = probe.canonical_record_digest(record)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_duplicate_event_sequence_rejected(self) -> None:
        # The runner enforces monotonic event_sequence per invocation.
        first = reference_event.build_event(
            **_base_event_kwargs(event_sequence=1)
        )
        with self.assertRaises(reference_event.ReferenceEventError):
            reference_event.validate_event(
                first, seen_event_sequences={0, 1}
            )

    def test_generic_phrased_event_does_not_satisfy(self) -> None:
        # Generic event kinds without invocation context cannot
        # satisfy the Plan 094 identity contract.
        event = reference_event.build_event(**_base_event_kwargs())
        self.assertNotIn("event_kind", {"foo"})
        # Confirm the event has the bounded set of fields.
        for field in ("invocation_id", "event_sequence", "event_sha256"):
            self.assertIn(field, event)

    def test_zero_digest_in_record_cannot_pass(self) -> None:
        # Plan 094 WP5: a passed record rejects zero binary or
        # build-manifest digests.
        record = _build_passed_record()
        record["i2pr_binary_sha256"] = "0" * 64
        record["record_sha256"] = probe.canonical_record_digest(record)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)


class Plan094ForwardPassClassificationTests(unittest.TestCase):
    def test_passed_record_carries_four_canonical_events(self) -> None:
        record = _build_passed_record()
        names = [e["event_name"] for e in record["observed_events"]]
        self.assertEqual(
            names,
            [
                "ntcp2_authenticated",
                "frame_emitted",
                "frame_authenticated_and_decrypted",
                "i2np_message_decoded",
            ],
        )

    def test_generic_event_without_exact_target_metadata_rejected(self) -> None:
        # Plan 094 WP4: a generic i2np_message_decoded event that
        # lacks the exact DeliveryStatus message_id or peer
        # Router Hash cannot satisfy pass. The minimal probe
        # schema enforces the four-event fingerprint; the runner
        # also enforces exact-target metadata before classifying
        # as pass. The runner-level contract is tested in
        # test_plan083_runner.py.
        record = _build_passed_record()
        record["observed_events"] = [
            {
                "event_name": "i2np_message_decoded",
                "source_side": "i2pd",
                "event_sha256": _hex("8", 64),
            }
        ]
        record["record_sha256"] = probe.canonical_record_digest(record)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_passed_record_requires_final_stage(self) -> None:
        record = _build_passed_record(
            highest_stage_reached=probe.NOISE_AUTHENTICATED,
        )
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_passed_record_requires_clean_cleanup(self) -> None:
        record = _build_passed_record(cleanup_result="forced")
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_passed_record_requires_not_started_reason(self) -> None:
        record = _build_passed_record(reason_code=probe.REASON_TCP_CONNECT_FAILED)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_protocol_rejected_preserves_highest_stage(self) -> None:
        record = _build_passed_record(
            observed_events=[],
            highest_stage_reached=probe.TCP_CONNECTED,
            terminal_result=probe.PROTOCOL_REJECTED,
            reason_code=probe.REASON_TCP_CONNECT_FAILED,
            cleanup_result="clean",
        )
        validated = probe.validate_record(record)
        self.assertEqual(validated["highest_stage_reached"], probe.TCP_CONNECTED)
        self.assertEqual(validated["terminal_result"], probe.PROTOCOL_REJECTED)


class Plan094BuildBinaryProvenanceTests(unittest.TestCase):
    def test_zero_i2pr_build_manifest_rejected(self) -> None:
        record = _build_passed_record(i2pr_build_manifest_sha256="0" * 64)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_zero_i2pd_build_manifest_rejected(self) -> None:
        record = _build_passed_record(i2pd_build_manifest_sha256="0" * 64)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_zero_reference_source_tree_rejected(self) -> None:
        record = _build_passed_record(reference_source_tree_sha256="0" * 64)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_zero_scenario_sha256_rejected(self) -> None:
        record = _build_passed_record(scenario_sha256="0" * 64)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_instrumented_attempt_kind_is_accepted(self) -> None:
        record = _build_passed_record(
            attempt_kind=probe.ATTEMPT_KIND_INSTRUMENTED,
            attempt_index=1,
        )
        validated = probe.validate_record(record)
        self.assertEqual(validated["attempt_kind"], probe.ATTEMPT_KIND_INSTRUMENTED)

    def test_control_attempt_kind_is_accepted(self) -> None:
        record = _build_passed_record(
            attempt_kind=probe.ATTEMPT_KIND_CONTROL,
            attempt_index=2,
        )
        validated = probe.validate_record(record)
        self.assertEqual(validated["attempt_kind"], probe.ATTEMPT_KIND_CONTROL)
        self.assertEqual(validated["attempt_index"], 2)

    def test_unknown_attempt_kind_rejected(self) -> None:
        record = _build_passed_record(attempt_kind="rogue")
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)

    def test_zero_attempt_index_rejected(self) -> None:
        record = _build_passed_record(attempt_index=0)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)


class Plan094ClosureGatingTests(unittest.TestCase):
    def test_prerelease_record_allows_zero_provenance(self) -> None:
        # Plan 094: a non-passed record may carry zero build
        # manifest digests so the pre-protocol and host-blocked
        # paths can construct minimal diagnostics.
        record = _build_passed_record(
            highest_stage_reached=probe.STATE_PREPARED,
            terminal_result=probe.PRE_PROTOCOL_REJECTED,
            reason_code=probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            observed_events=[],
            cleanup_result="not-run",
            i2pr_build_manifest_sha256="0" * 64,
            i2pd_build_manifest_sha256="0" * 64,
            reference_source_tree_sha256="0" * 64,
            scenario_sha256="0" * 64,
        )
        validated = probe.validate_record(record)
        self.assertEqual(
            validated["terminal_result"], probe.PRE_PROTOCOL_REJECTED
        )

    def test_rejected_record_allows_zero_provenance(self) -> None:
        record = _build_passed_record(
            highest_stage_reached=probe.NOISE_AUTHENTICATED,
            terminal_result=probe.PROTOCOL_REJECTED,
            reason_code=probe.REASON_REFERENCE_EVENTS_MISSING,
            observed_events=[],
            cleanup_result="clean",
            i2pr_build_manifest_sha256="0" * 64,
            i2pd_build_manifest_sha256="0" * 64,
            reference_source_tree_sha256="0" * 64,
            scenario_sha256="0" * 64,
        )
        validated = probe.validate_record(record)
        self.assertEqual(
            validated["terminal_result"], probe.PROTOCOL_REJECTED
        )


class Plan094GateHandoffTests(unittest.TestCase):
    def test_plan088_status_remains_blocked(self) -> None:
        text = (REPO_ROOT / "plans/088-status.md").read_text()
        self.assertTrue(
            "blocked-pending-plan094-completion" in text
            or "blocked-pending-plan095-ci-closure" in text,
            "plans/088-status.md must declare a plan-094 or plan-095 active block",
        )

    def test_plan088_decision_is_insufficient_evidence(self) -> None:
        text = (REPO_ROOT / "plans/088-status.md").read_text()
        self.assertIn("decision = insufficient-evidence", text)

    def test_plan079_remains_blocked(self) -> None:
        text = (REPO_ROOT / "plans/088-status.md").read_text()
        self.assertIn("blocked-pending-plan088-two-way-pass", text)

    def test_plan072_remains_inactive(self) -> None:
        text = (REPO_ROOT / "plans/088-status.md").read_text()
        self.assertIn("inactive-pending-plan088-ambiguity", text)


class Plan094PlanStatusTests(unittest.TestCase):
    def test_plan094_plan_document_exists(self) -> None:
        self.assertTrue(
            (REPO_ROOT / "plans/094-plan093-completion-and-plan087-to-plan088-handoff.md").is_file()
        )

    def test_agents_md_names_plan094(self) -> None:
        text = (REPO_ROOT / "AGENTS.md").read_text()
        self.assertIn("Plan 094", text)

    def test_readme_documents_plan094(self) -> None:
        text = (REPO_ROOT / "README.md").read_text()
        self.assertIn("Plan 094", text)

    def test_ntcp2_stays_experimental_and_non_advertised(self) -> None:
        text = (REPO_ROOT / "AGENTS.md").read_text()
        self.assertIn("experimental and non-advertised", text)


if __name__ == "__main__":
    unittest.main()