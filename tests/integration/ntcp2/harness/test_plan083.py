"""Plan 083 minimal i2pr-to-i2pd wire probe test matrix.

The Plan 083 test matrix exercises the canonical usage of the
``minimal_i2pd_probe`` schema:

- the direction and reference are locked to ``i2pr-to-i2pd-ipv4``
  and ``i2pd``;
- the strictly-increasing stage model reports one bounded value per
  record;
- the positive passed record requires the final stage, all four
  canonical observed events, clean cleanup, and ``reason_code =
  not_started``;
- pre-protocol rejections use the typed
  ``pre_protocol_rejected`` terminal with a bounded reason code from
  the Plan 083 fixed set;
- protocol rejections and timeouts use the typed terminal and reason;
- a generic ``typed-harness-operation-failed`` reason never closes a
  record;
- the schema enforces the Plan 082 run-identity contract;
- the probe never imports Plan 056/066 candidate, bundle,
  certificate, rootless-topology, or Multipass authority.

The tests use the in-process schema only; they never launch a
subprocess and never need the real i2pd binary.
"""

from __future__ import annotations

import unittest

import minimal_i2pd_probe as probe


def _hex(value: str, length: int = 64) -> str:
    return value * length


def _minimal_passed_record(
    *,
    observed_events: list[dict[str, str]] | None = None,
    highest_stage: str = probe.I2NP_DELIVERY_STATUS_DECODED,
    terminal_result: str = probe.PASSED,
    reason_code: str = probe.REASON_NOT_STARTED,
    cleanup_result: str = "clean",
) -> dict[str, object]:
    events = observed_events
    if events is None:
        events = [
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
        ]
    return probe.build_record(
        run_id="plan083-passed",
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
        delivery_status_message_id=0x04200001,
        observed_events=list(events),
        highest_stage_reached=highest_stage,
        terminal_result=terminal_result,
        reason_code=reason_code,
        process_counters={
            "i2pr_prepare": {"started": 1, "exited": 1, "forced": 0},
            "i2pd_prepare": {"started": 1, "exited": 1, "forced": 0},
            "i2pd_listener": {"started": 1, "exited": 1, "forced": 0},
            "i2pr_dialer": {"started": 1, "exited": 1, "forced": 0},
        },
        cleanup_result=cleanup_result,
    )


class Plan083DirectionAndReferenceTests(unittest.TestCase):
    def test_direction_is_i2pr_to_i2pd_ipv4(self) -> None:
        record = _minimal_passed_record()
        self.assertEqual(record["direction"], "i2pr-to-i2pd-ipv4")

    def test_reference_is_i2pd(self) -> None:
        record = _minimal_passed_record()
        self.assertEqual(record["reference"], "i2pd")

    def test_reverse_direction_is_not_allowlisted_in_schema(self) -> None:
        record = _minimal_passed_record()
        record["direction"] = "i2pd-to-i2pr-ipv4"
        record["record_sha256"] = probe.canonical_record_digest(record)
        with self.assertRaises(probe.MinimalI2pdProbeError):
            probe.validate_record(record)


class Plan083StageModelTests(unittest.TestCase):
    def test_stages_are_strictly_ordered_with_final_decoded(self) -> None:
        ordered = list(probe.STAGES)
        self.assertEqual(ordered[0], probe.NOT_STARTED)
        self.assertEqual(ordered[-1], probe.I2NP_DELIVERY_STATUS_DECODED)

    def test_passed_record_uses_final_stage(self) -> None:
        record = _minimal_passed_record()
        self.assertEqual(record["highest_stage_reached"], probe.I2NP_DELIVERY_STATUS_DECODED)

    def test_pre_protocol_record_uses_state_prepared(self) -> None:
        record = _minimal_passed_record(
            observed_events=[],
            highest_stage=probe.STATE_PREPARED,
            terminal_result=probe.PRE_PROTOCOL_REJECTED,
            reason_code=probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            cleanup_result="not-run",
        )
        self.assertEqual(record["highest_stage_reached"], probe.STATE_PREPARED)
        self.assertEqual(record["terminal_result"], probe.PRE_PROTOCOL_REJECTED)

    def test_protocol_rejection_preserves_highest_stage(self) -> None:
        record = _minimal_passed_record(
            observed_events=[],
            highest_stage=probe.NOISE_AUTHENTICATED,
            terminal_result=probe.PROTOCOL_REJECTED,
            reason_code=probe.REASON_NOISE_SESSION_REQUEST_REJECTED,
            cleanup_result="clean",
        )
        self.assertEqual(record["highest_stage_reached"], probe.NOISE_AUTHENTICATED)
        self.assertEqual(record["terminal_result"], probe.PROTOCOL_REJECTED)

    def test_protocol_timeout_uses_typed_reason(self) -> None:
        record = _minimal_passed_record(
            observed_events=[],
            highest_stage=probe.SESSION_CONFIRMED_ACCEPTED,
            terminal_result=probe.PROTOCOL_TIMEOUT,
            reason_code=probe.REASON_NOISE_SESSION_REQUEST_REJECTED,
            cleanup_result="forced",
        )
        self.assertEqual(record["terminal_result"], probe.PROTOCOL_TIMEOUT)


class Plan083ReasonCodeTests(unittest.TestCase):
    def test_reason_codes_are_bounded(self) -> None:
        self.assertIn(probe.REASON_TCP_CONNECT_FAILED, probe.REASON_CODES)
        self.assertIn(probe.REASON_NOISE_SESSION_REQUEST_REJECTED, probe.REASON_CODES)
        self.assertIn(probe.REASON_SESSION_CONFIRMED_REJECTED, probe.REASON_CODES)
        self.assertIn(probe.REASON_PEER_ROUTER_INFO_REJECTED, probe.REASON_CODES)
        self.assertIn(probe.REASON_AUTHENTICATED_LINK_INSTALL_FAILED, probe.REASON_CODES)
        self.assertIn(probe.REASON_I2PR_FRAME_WRITE_FAILED, probe.REASON_CODES)
        self.assertIn(probe.REASON_I2PD_FRAME_AUTHENTICATION_FAILED, probe.REASON_CODES)
        self.assertIn(probe.REASON_I2PD_I2NP_DECODE_FAILED, probe.REASON_CODES)
        self.assertIn(probe.REASON_DELIVERY_STATUS_ID_MISMATCH, probe.REASON_CODES)
        self.assertIn(probe.REASON_REFERENCE_EVENTS_MISSING, probe.REASON_CODES)
        self.assertIn(probe.REASON_CLEANUP_VERIFICATION_FAILED, probe.REASON_CODES)
        self.assertIn(probe.REASON_LANE_INVALID, probe.REASON_CODES)
        self.assertIn(probe.REASON_PRE_PROTOCOL_RENDER_FAILED, probe.REASON_CODES)
        self.assertIn(probe.REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED, probe.REASON_CODES)
        self.assertIn(probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED, probe.REASON_CODES)
        self.assertIn(probe.REASON_PRE_PROTOCOL_REFERENCE_FAILED, probe.REASON_CODES)
        self.assertIn(probe.REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED, probe.REASON_CODES)

    def test_generic_failure_reason_is_not_allowlisted(self) -> None:
        self.assertNotIn("typed-harness-operation-failed", probe.REASON_CODES)

    def test_pre_protocol_preparation_failure_record_validates(self) -> None:
        record = _minimal_passed_record(
            observed_events=[],
            highest_stage=probe.STATE_PREPARED,
            terminal_result=probe.PRE_PROTOCOL_REJECTED,
            reason_code=probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            cleanup_result="not-run",
        )
        probe.validate_record(record)

    def test_pre_protocol_run_identity_failure_record_validates(self) -> None:
        record = _minimal_passed_record(
            observed_events=[],
            highest_stage=probe.NOT_STARTED,
            terminal_result=probe.PRE_PROTOCOL_REJECTED,
            reason_code=probe.REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED,
            cleanup_result="not-run",
        )
        probe.validate_record(record)


class Plan083ObservedEventTests(unittest.TestCase):
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

    def test_passed_record_requires_event_name_set(self) -> None:
        events = [
            {
                "event_name": "frame_emitted",
                "source_side": "i2pr",
                "event_sha256": _hex("6", 64),
            },
        ]
        with self.assertRaises(probe.MinimalI2pdProbeError):
            _minimal_passed_record(observed_events=events)


class Plan083ProcessCounterTests(unittest.TestCase):
    def test_preparation_and_live_counters_are_separate(self) -> None:
        record = _minimal_passed_record()
        counters = record["process_counters"]
        self.assertIn("i2pr_prepare", counters)
        self.assertIn("i2pd_prepare", counters)
        self.assertIn("i2pd_listener", counters)
        self.assertIn("i2pr_dialer", counters)

    def test_pre_protocol_rejection_still_records_preparation(self) -> None:
        record = probe.build_record(
            run_id="plan083-preprotocol",
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
            delivery_status_message_id=0x04200003,
            observed_events=[],
            highest_stage_reached=probe.STATE_PREPARED,
            terminal_result=probe.PRE_PROTOCOL_REJECTED,
            reason_code=probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
            process_counters={
                "i2pr_prepare": {"started": 1, "exited": 1, "forced": 0},
                "i2pd_prepare": {"started": 0, "exited": 0, "forced": 0},
                "i2pd_listener": {"started": 0, "exited": 0, "forced": 0},
                "i2pr_dialer": {"started": 0, "exited": 0, "forced": 0},
            },
            cleanup_result="not-run",
        )
        counters = record["process_counters"]
        # Pre-protocol record carries zero live-process starts; it
        # has not yet executed a live i2pr dial or i2pd listener.
        self.assertEqual(counters["i2pr_dialer"]["started"], 0)
        self.assertEqual(counters["i2pd_listener"]["started"], 0)
        self.assertEqual(counters["i2pr_prepare"]["started"], 1)


class Plan083TopologyTests(unittest.TestCase):
    def test_rootless_sealed_topology_is_allowlisted(self) -> None:
        self.assertIn("rootless-sealed-single-netns", probe.ALLOWED_TOPOLOGY_KINDS)

    def test_multipass_owned_guest_topology_is_allowlisted(self) -> None:
        self.assertIn("multipass-owned-guest", probe.ALLOWED_TOPOLOGY_KINDS)

    def test_public_topology_is_not_allowlisted(self) -> None:
        self.assertNotIn("public-network", probe.ALLOWED_TOPOLOGY_KINDS)


class Plan083ProvenanceTests(unittest.TestCase):
    def test_record_binds_source_commit(self) -> None:
        record = _minimal_passed_record()
        self.assertEqual(len(record["source_commit"]), 40)
        self.assertTrue(all(character in "0123456789abcdef" for character in record["source_commit"]))

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
        self.assertEqual(record["delivery_status_message_id"], 0x04200001)


class Plan083BoundaryTests(unittest.TestCase):
    def test_module_does_not_import_release_authority(self) -> None:
        source_path = probe.__file__
        if source_path is None:
            self.skipTest("module path unavailable")
        import pathlib
        import re
        text = pathlib.Path(source_path).read_text(encoding="utf-8")
        # Strip comments and docstrings before checking forbidden
        # identifiers so the module may reference the upstream plans
        # in plain prose without importing their authority.
        code_only = re.sub(r"\"\"\".*?\"\"\"", "", text, flags=re.DOTALL)
        code_only = re.sub(r"#.*", "", code_only)
        for forbidden in (
            "verify_milestone3_certificate",
            "candidate_record",
            "evidence_bundle",
            "from .rootless_topology",
            "from .multipass",
        ):
            self.assertNotIn(forbidden, code_only, f"probe module imports release authority: {forbidden}")

    def test_module_references_plan083_documentation(self) -> None:
        source_path = probe.__file__
        if source_path is None:
            self.skipTest("module path unavailable")
        import pathlib
        text = pathlib.Path(source_path).read_text(encoding="utf-8")
        self.assertIn("Plan 083", text)


if __name__ == "__main__":
    unittest.main()
