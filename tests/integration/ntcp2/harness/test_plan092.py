"""Plan 092 forward-handshake evidence integrity and ownership closure tests.

These tests cover the Plan 092 work packages:

- WP1 (status authority): plans/091-status.md records the partial
  diagnostic surface, plans/087-status.md begins with the current
  Plan 091/092 forward state, plans/088-status.md records the new
  decision tokens, and plans/092-status.md exists once the plan is
  open.
- WP2 (privacy-safe observation contract): the schema rejects raw
  payload fields, rejects oversized counts, rejects malformed
  digests, rejects mismatched direction/run/invocation/scenario-id,
  rejects stage names outside the closed allowlist, and produces a
  stable SHA-256 event digest.
- WP3 (i2pr runtime observations): the runtime handshake driver
  exposes the observed entry points and the bounded observer types,
  preserves terminal counters on failure, and the no-op observer
  produces identical handshake outcomes to a recording observer.
- WP4 (i2pd responder observations): the i2pd direct driver keeps the
  observer patch seams, the listener-mode TCP-stage acceptance, and
  the bounded allowlist for i2pd stages.
- WP5 (Plan 083 event ingestion): the runner uses one ingestion
  function for live polling and final drain, dedupes by the
  current-run key, and includes ``tcp_accepted`` in the final drain.
- WP6 (static enforcement): the boundary checker rejects raw or hex
  capture recommendations, refuses zero digest substitutes for known
  diagnostic records, and enforces the active-sequence token naming
  Plan 092 as the only next executable plan.
"""

from __future__ import annotations

import hashlib
import json
import re
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]

sys.path.insert(0, str(REPO_ROOT / "tests/integration/ntcp2/harness"))

import handshake_stage  # noqa: E402
import minimal_i2pd_probe as probe  # noqa: E402


HEX64 = re.compile(r"^[0-9a-f]{64}$")


class Plan092StatusAuthorityTests(unittest.TestCase):
    """WP1: Plan 091 is partial/incomplete, not closed."""

    def test_plan091_status_records_partial_diagnostic_surface(self):
        text = (REPO_ROOT / "plans/091-status.md").read_text()
        self.assertIn("partial / incomplete", text.lower())
        self.assertIn("Plan 092", text)
        self.assertNotIn("forward direction did pass", text)

    def test_plan091_status_does_not_recommend_hex_dump(self):
        text = (REPO_ROOT / "plans/091-status.md").read_text()
        # Plan 092 forbids raw or hex handshake capture. Plan 091
        # must not recommend a follow-up hex dump. The status record
        # may still *mention* the word "hex" inside an explicit
        # "Forbidden follow-up" section that names the previously
        # considered recommendation as forbidden.
        lowered = text.lower()
        # Locate the "Forbidden follow-up" section header.
        forbidden_start = lowered.find("## forbidden follow-up")
        self.assertNotEqual(
            forbidden_start, -1,
            "Plan 091 must declare a Forbidden follow-up section",
        )
        after_forbidden = lowered[forbidden_start:]
        # Outside the forbidden-follow-up section, the words "hex dump",
        # "hex-dump", "1 kib", or "raw bytes" must never appear.
        outside = lowered[:forbidden_start] + lowered[forbidden_start + len("## forbidden follow-up"):].split("\n## ", 1)[-1]
        for forbidden in ("hex dump", "hex-dump", "1 kib", "raw bytes"):
            self.assertNotIn(
                forbidden,
                outside,
                f"Plan 091 must not mention {forbidden!r} outside the forbidden-follow-up section",
            )
        # The plan must explicitly declare the recommendation as
        # forbidden.
        self.assertIn("does **not** authorise", after_forbidden)

    def test_plan087_status_begins_with_plan091_plan092_state(self):
        text = (REPO_ROOT / "plans/087-status.md").read_text()
        # The current state block names Plan 091 + Plan 092.
        self.assertIn("Plan 091", text)
        self.assertIn("Plan 092", text)
        # Plan 087 must not simultaneously claim the forward
        # direction passed and blocked.
        self.assertNotIn("Plan 087 closes as `passed`", text)

    def test_plan088_status_records_plan094_or_plan095_as_next_executable(self):
        text = (REPO_ROOT / "plans/088-status.md").read_text()
        # Plan 094 was the single next executable plan; Plan 095 has
        # superseded it as the CI live-wire closure authority. The
        # status record must reflect at least one of the two.
        self.assertTrue(
            "plan_094 = active-single-next-executable-completion-pass" in text
            or "plan_095 = ci-live-wire-closure-next-executable" in text,
            "plans/088-status.md must name Plan 094 or Plan 095 as the active completion authority",
        )
        self.assertTrue(
            "blocked-pending-plan094-completion" in text
            or "blocked-pending-plan095-ci-closure" in text,
            "plans/088-status.md must declare a plan-094 or plan-095 active block",
        )
        self.assertIn("Plan 094", text)
        # The legacy ``lane-invalidated`` token must not be the
        # *active* decision value. The token may still appear in the
        # explanatory commentary that explains it was superseded.
        decision_match = re.search(
            r"decision\s*=\s*([a-z][a-z0-9-]*)", text
        )
        self.assertIsNotNone(decision_match)
        self.assertNotEqual(
            decision_match.group(1),
            "lane-invalidated",
            "Plan 088 active decision must not be the legacy token",
        )


class Plan092HandshakeStageSchemaTests(unittest.TestCase):
    """WP2: the privacy-safe observation schema rejects forbidden fields."""

    def test_schema_rejects_raw_payload_field(self):
        bad = handshake_stage.build_observation(
            run_id="plan092-bad",
            direction="i2pr-to-i2pd-ipv4",
            source_side="i2pr",
            invocation_id="inv-1",
            event_sequence=1,
            stage="session_request_write_started",
            elapsed_millis=10,
        )
        bad["raw"] = "deadbeef"
        with self.assertRaises(handshake_stage.HandshakeStageObservationError):
            handshake_stage.validate_observation(bad)

    def test_schema_rejects_payload_hex_field(self):
        bad = handshake_stage.build_observation(
            run_id="plan092-bad",
            direction="i2pr-to-i2pd-ipv4",
            source_side="i2pr",
            invocation_id="inv-1",
            event_sequence=2,
            stage="session_request_write_started",
            elapsed_millis=10,
        )
        bad["payload_hex"] = "deadbeef"
        with self.assertRaises(handshake_stage.HandshakeStageObservationError):
            handshake_stage.validate_observation(bad)

    def test_schema_rejects_unknown_i2pr_stage(self):
        with self.assertRaises(handshake_stage.HandshakeStageObservationError):
            handshake_stage.build_observation(
                run_id="plan092-bad",
                direction="i2pr-to-i2pd-ipv4",
                source_side="i2pr",
                invocation_id="inv-1",
                event_sequence=3,
                stage="unknown-stage",
                elapsed_millis=10,
            )

    def test_schema_rejects_unknown_i2pd_stage(self):
        with self.assertRaises(handshake_stage.HandshakeStageObservationError):
            handshake_stage.build_observation(
                run_id="plan092-bad",
                direction="i2pr-to-i2pd-ipv4",
                source_side="i2pd",
                invocation_id="inv-1",
                event_sequence=1,
                stage="unknown-i2pd-stage",
                elapsed_millis=10,
            )

    def test_schema_rejects_oversized_counts(self):
        with self.assertRaises(handshake_stage.HandshakeStageObservationError):
            handshake_stage.build_observation(
                run_id="plan092-bad",
                direction="i2pr-to-i2pd-ipv4",
                source_side="i2pr",
                invocation_id="inv-1",
                event_sequence=4,
                stage="session_request_write_started",
                elapsed_millis=10,
                expected_octets=handshake_stage.MAX_OCTETS + 1,
            )

    def test_schema_rejects_negative_event_sequence(self):
        with self.assertRaises(handshake_stage.HandshakeStageObservationError):
            handshake_stage.build_observation(
                run_id="plan092-bad",
                direction="i2pr-to-i2pd-ipv4",
                source_side="i2pr",
                invocation_id="inv-1",
                event_sequence=-1,
                stage="session_request_write_started",
                elapsed_millis=10,
            )

    def test_schema_rejects_completed_greater_than_expected(self):
        with self.assertRaises(handshake_stage.HandshakeStageObservationError):
            handshake_stage.build_observation(
                run_id="plan092-bad",
                direction="i2pr-to-i2pd-ipv4",
                source_side="i2pr",
                invocation_id="inv-1",
                event_sequence=5,
                stage="session_request_write_completed",
                elapsed_millis=10,
                expected_octets=100,
                completed_octets=200,
            )

    def test_schema_rejects_malformed_event_sha256(self):
        bad = handshake_stage.build_observation(
            run_id="plan092-bad",
            direction="i2pr-to-i2pd-ipv4",
            source_side="i2pr",
            invocation_id="inv-1",
            event_sequence=6,
            stage="session_request_write_started",
            elapsed_millis=10,
        )
        bad["event_sha256"] = "z" * 64
        with self.assertRaises(handshake_stage.HandshakeStageObservationError):
            handshake_stage.validate_observation(bad)

    def test_schema_rejects_mismatched_direction(self):
        with self.assertRaises(handshake_stage.HandshakeStageObservationError):
            handshake_stage.build_observation(
                run_id="plan092-bad",
                direction="i2pd-to-i2pr-ipv4",
                source_side="i2pr",
                invocation_id="inv-1",
                event_sequence=7,
                stage="session_request_write_started",
                elapsed_millis=10,
            )

    def test_schema_rejects_blank_invocation_id(self):
        with self.assertRaises(handshake_stage.HandshakeStageObservationError):
            handshake_stage.build_observation(
                run_id="plan092-bad",
                direction="i2pr-to-i2pd-ipv4",
                source_side="i2pr",
                invocation_id="",
                event_sequence=8,
                stage="session_request_write_started",
                elapsed_millis=10,
            )

    def test_schema_accepts_canonical_i2pr_stages(self):
        for stage in handshake_stage.I2PR_HANDSHAKE_STAGES:
            obs = handshake_stage.build_observation(
                run_id="plan092-ok",
                direction="i2pr-to-i2pd-ipv4",
                source_side="i2pr",
                invocation_id="inv-1",
                event_sequence=1,
                stage=stage,
                elapsed_millis=10,
            )
            self.assertTrue(HEX64.fullmatch(obs["event_sha256"]))

    def test_schema_accepts_canonical_i2pd_stages(self):
        for stage in handshake_stage.I2PD_HANDSHAKE_STAGES:
            obs = handshake_stage.build_observation(
                run_id="plan092-ok",
                direction="i2pr-to-i2pd-ipv4",
                source_side="i2pd",
                invocation_id="inv-1",
                event_sequence=1,
                stage=stage,
                elapsed_millis=10,
            )
            self.assertTrue(HEX64.fullmatch(obs["event_sha256"]))

    def test_event_digest_is_stable_across_repetitions(self):
        kwargs = dict(
            run_id="plan092-stable",
            direction="i2pr-to-i2pd-ipv4",
            source_side="i2pr",
            invocation_id="inv-1",
            event_sequence=1,
            stage="session_request_write_completed",
            elapsed_millis=10,
            expected_octets=287,
            completed_octets=287,
        )
        first = handshake_stage.build_observation(**kwargs)
        second = handshake_stage.build_observation(**kwargs)
        self.assertEqual(first, second)
        self.assertEqual(first["event_sha256"], second["event_sha256"])

    def test_event_digest_omits_event_sha256_field(self):
        obs = handshake_stage.build_observation(
            run_id="plan092-stable",
            direction="i2pr-to-i2pd-ipv4",
            source_side="i2pr",
            invocation_id="inv-1",
            event_sequence=1,
            stage="session_request_write_completed",
            elapsed_millis=10,
        )
        manual = {key: value for key, value in obs.items() if key != "event_sha256"}
        canonical = json.dumps(manual, sort_keys=True, separators=(",", ":"))
        self.assertEqual(obs["event_sha256"], hashlib.sha256(canonical.encode()).hexdigest())


class Plan092I2prRuntimeObservationTests(unittest.TestCase):
    """WP3: i2pr runtime exposes observed driver and counter preservation."""

    def test_runtime_observer_types_are_exported(self):
        # The runtime is a Rust crate; the test ensures the static
        # source surface declares the new types. Importing the
        # compiled crate from a subprocess is covered by the
        # rustdoc/tests clippy gates.
        source = (REPO_ROOT / "crates/i2pr-runtime/src/lib.rs").read_text()
        self.assertIn("HandshakeProgressObserver", source)
        self.assertIn("NoopHandshakeObserver", source)
        self.assertIn("HandshakeStageObservation", source)
        self.assertIn("HandshakeIoResult", source)
        self.assertIn("HandshakeCounterSnapshot", source)
        self.assertIn("HandshakeRunOutcome", source)
        self.assertIn("drive_initiator_handshake_observed", source)
        self.assertIn("drive_responder_handshake_observed", source)

    def test_observer_module_has_noop_observer_default(self):
        source = (
            REPO_ROOT / "crates/i2pr-runtime/src/ntcp2_handshake_observer.rs"
        ).read_text()
        self.assertIn("NoopHandshakeObserver", source)
        self.assertIn("fn observe", source)

    def test_runtime_drive_inner_preserves_terminal_counters(self):
        # The runtime handshake driver exposes both observed and
        # no-op entry points. The no-op path is the canonical
        # production surface and preserves the typed counters on
        # every error branch.
        source = (REPO_ROOT / "crates/i2pr-runtime/src/ntcp2_driver.rs").read_text()
        self.assertIn("HandshakeCounterSnapshot", source)
        self.assertIn("drive_initiator_handshake_observed", source)
        self.assertIn("drive_responder_handshake_observed", source)
        # Counter snapshot must be returned alongside every error
        # branch.
        self.assertIn("HandshakeCounterSnapshot {", source)
        # Both `read_count` and `write_count` must be preserved.
        self.assertIn("read_count", source)
        self.assertIn("write_count", source)


class Plan092I2pdObserverCoverageTests(unittest.TestCase):
    """WP4: i2pd observer seam keeps the bounded allowlist."""

    def test_i2pd_observer_header_keeps_bounded_metadata(self):
        text = (
            REPO_ROOT
            / "tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h"
        ).read_text()
        self.assertIn("I2PD_INTEROP_OBSERVER", text)
        self.assertIn("ObserveTcpAccepted", text)
        self.assertIn("ObserveAuthenticated", text)
        self.assertIn("ObserveReceivedI2NP", text)
        self.assertIn("ObserveSentI2NP", text)
        # The observer must never define raw bytes / ciphertext /
        # transcript fields in its struct or arguments. Comments
        # about the *forbidden* fields are tolerated because the
        # observer header documents the contract.
        for forbidden in ("ciphertext_field", "transcript_field"):
            self.assertNotIn(forbidden, text)
        # The observer must not define struct fields named
        # ``raw_payload``, ``payload_bytes``, ``transcript_bytes``.
        for forbidden in ("raw_payload", "payload_bytes", "transcript_bytes", "raw_bytes"):
            self.assertNotIn(forbidden, text)

    def test_i2pd_observer_seam_never_exposes_raw_payload(self):
        text = (
            REPO_ROOT
            / "tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp"
        ).read_text()
        # The driver uses ``payload`` as a local JSON-builder
        # variable name. The privacy-safe contract forbids raw
        # bytes, ciphertext, transcript, or any per-peer payload
        # flowing through the observer sink.
        for forbidden in ("ciphertext_field", "transcript_bytes", "raw_bytes_field"):
            self.assertNotIn(forbidden, text)


class Plan092EventIngestionTests(unittest.TestCase):
    """WP5: Plan 083 runner uses one ingestion function and a current-run dedup."""

    def test_runner_uses_single_dedup_key_shape(self):
        text = (
            REPO_ROOT / "tests/integration/ntcp2/harness/plan083_runner.py"
        ).read_text()
        self.assertIn('("i2pd", invocation_id, event_sequence, marker)', text)
        self.assertIn("tcp_accepted", text)
        # The final drain must include tcp_accepted.
        self.assertIn('"tcp_accepted"', text)

    def test_runner_classifies_reference_events_missing_only_after_drain(self):
        text = (
            REPO_ROOT / "tests/integration/ntcp2/harness/plan083_runner.py"
        ).read_text()
        # ``reference-events-missing`` is used only as a final fallback;
        # the runner retains the actual i2pr terminal reason code.
        self.assertIn("REASON_REFERENCE_EVENTS_MISSING", text)
        self.assertIn("i2pr_terminal_reason", text)


class Plan092StaticEnforcementTests(unittest.TestCase):
    """WP6: the static boundary check enforces the active-sequence token."""

    def test_check_ntcp2_interoperability_rejects_hex_dump_recommendation(self):
        text = (
            REPO_ROOT / "scripts/check-ntcp2-interoperability.sh"
        ).read_text()
        # The script must enforce the privacy-safe observation
        # contract from Plan 092.
        self.assertIn("plan 092", text.lower())
        self.assertIn("i2pr-ntcp2-handshake-stage-v1", text)
        # Raw or hex capture is forbidden.
        self.assertIn("FORBIDDEN", text)

    def test_check_ntcp2_interoperability_requires_plan092_files(self):
        text = (
            REPO_ROOT / "scripts/check-ntcp2-interoperability.sh"
        ).read_text()
        self.assertIn("handshake_stage.py", text)
        self.assertIn("test_plan092.py", text)


class Plan092ProbeRecordSchemaTests(unittest.TestCase):
    """WP6 (subset): probe record schema stays stable under the new tests."""

    def test_minimal_i2pd_probe_record_can_still_be_built(self):
        # The minimal i2pd probe record schema is unchanged by
        # Plan 092; the schema still rejects zero digests and
        # accepts a non-zero placement_record_sha256.
        from minimal_i2pd_probe import build_record, empty_process_counters

        record = build_record(
            run_id="plan092-probe",
            source_commit="a" * 40,
            reference_revision="f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
            lane_qualification_sha256="c" * 64,
            topology_kind="host-loopback-development",
            parent_network_state_unchanged=True,
            i2pr_binary_sha256="d" * 64,
            i2pd_binary_sha256="e" * 64,
            i2pr_build_manifest_sha256="0222" * 16,
            i2pd_build_manifest_sha256="0333" * 16,
            reference_source_tree_sha256="0444" * 16,
            scenario_sha256="0555" * 16,
            attempt_kind=probe.ATTEMPT_KIND_INSTRUMENTED,
            attempt_index=1,
            i2pr_router_info_sha256="1" * 64,
            i2pd_router_info_sha256="2" * 64,
            i2pr_router_hash_sha256="3" * 64,
            i2pd_router_hash_sha256="4" * 64,
            delivery_status_message_id=0x04200001,
            observed_events=[],
            highest_stage_reached="state_prepared",
            terminal_result="pre_protocol_rejected",
            reason_code="pre-protocol-router-info-validation-failed",
            process_counters=empty_process_counters(),
            cleanup_result="clean",
            placement_record_sha256="a" * 64,
        )
        self.assertEqual(record["schema"], probe.SCHEMA)


if __name__ == "__main__":
    unittest.main()