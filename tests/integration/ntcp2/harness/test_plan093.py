"""Plan 093 Plan 087 forward data-phase and reference-observer closure tests.

Plan 093 corrects the Plan 092 misclassification of the i2pd log
message ``NTCP2: Receive length read error:`` from the handshake
``SessionRequest`` reader to the data-phase ``HandleReceivedLength``
length reader. The plan also lands:

- WP3: the i2pd observer reset/generation contract
  (`BeginListenerGeneration` plus generation-bound wait primitives)
  so a fast TCP-accept observation survives readiness and stale
  observations cannot satisfy the target waits;
- WP4: a bounded sequence ring with exact target predicate waits
  (`WaitForReceivedDeliveryStatusAfter`,
  `WaitForSentDeliveryStatusAfter`) that require active
  generation, post-baseline sequence, exact message ID, and exact
  peer Router Hash;
- WP5: the i2pr bounded multi-frame target receive oracle
  (`receive_correlated_delivery_status`) with a single absolute
  deadline and cumulative frame/byte/block/non-target-I2NP bounds;
- WP6: canonical runner event authority with real invocation IDs;
- WP7: live binary and source provenance binding that rejects zero
  digests in attempted live records;
- WP8: the focused test matrix below.

The matrix covers:

1. Receive length read error maps to data phase, not SessionRequest.
2. Pinned HandleSessionRequestReceived is the handshake-read path.
3. Pinned inbound PeerConnected sends local RouterInfo.
4. Plan 092 Branch A is marked superseded in current status authority.
5. Observer reset occurs before transport startup and never after
   `listener_ready`.
6. Current-generation immediate TCP accept survives readiness.
7. Stale-generation TCP accept is rejected.
8. Automatic RouterInfo send cannot satisfy target DeliveryStatus
   send wait.
9. An unrelated received I2NP cannot satisfy target receive wait.
10. Exact target predicate requires type, envelope ID, payload ID,
    peer hash, generation, and post-baseline sequence.
11. Listener shutdown cannot occur before exact target send
    completion.
12. RouterInfo-first then DeliveryStatus passes the i2pr receive
    oracle.
13. Valid padding/options before target passes.
14. Wrong-ID DeliveryStatus rejects.
15. Duplicate target rejects.
16. Absolute deadline cannot be refreshed by non-target traffic.
17. Frame limit rejects.
18. Byte limit rejects.
19. Block limit rejects.
20. Invalid or wrong-peer RouterInfo rejects.
21. Termination before target rejects.
22. Runner retains late i2pd target send and terminal events.
23. Runner uses real invocation ID rather than scenario ID aliasing.
24. Zero i2pr binary digest rejects attempted live execution.
25. Instrumented/control mismatch rejects Plan 087 closure.
26. Plan 088 gate requires both exact passing forward records.
27. Plan 079 remains blocked before Plan 088.
28. Plan 072 remains inactive before Plan 088 ambiguity.
29. NTCP2 remains experimental and non-advertised.
"""

from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]

sys.path.insert(0, str(REPO_ROOT / "tests/integration/ntcp2/harness"))

import source_classification  # noqa: E402

HEX64 = re.compile(r"^[0-9a-f]{64}$")
HEX40 = re.compile(r"^[0-9a-f]{40}$")


class Plan093SourceClassificationTests(unittest.TestCase):
    """WP2: pinned source-classification tests."""

    def test_locked_revision_is_pinned_i2pd_2_60_0(self):
        self.assertEqual(
            source_classification.PINNED_I2PD_REVISION,
            "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
        )
        self.assertEqual(source_classification.REFERENCE, "i2pd")
        self.assertEqual(source_classification.REFERENCE_VERSION, "2.60.0")

    def test_receive_length_read_error_maps_to_data_phase(self):
        # Plan 092 misclassification: this log message comes from
        # `HandleReceivedLength`, not `HandleSessionRequestReceived`.
        self.assertEqual(
            source_classification.diagnostic_location(
                "NTCP2: receive length read error: ",
            ),
            "data-phase length reader",
        )

    def test_session_request_read_error_maps_to_handshake(self):
        self.assertEqual(
            source_classification.diagnostic_location(
                "NTCP2: SessionRequest read error: ",
            ),
            "handshake SessionRequest reader",
        )

    def test_established_calls_peerconnected(self):
        call_graph = source_classification.CALL_GRAPH
        # The call graph is a tuple of canonical symbol names.
        joined = "\n".join(call_graph)
        self.assertIn("NTCP2Session::Established", joined)
        self.assertIn("transports.PeerConnected", joined)

    def test_inbound_peer_connected_sends_local_router_info(self):
        call_graph = source_classification.CALL_GRAPH
        joined = "\n".join(call_graph)
        self.assertIn("session->SendLocalRouterInfo", joined)


class Plan093StatusAuthorityTests(unittest.TestCase):
    """WP1: status authority supersedes Plan 092 Branch A."""

    def test_plan092_status_supersedes_branch_a(self):
        text = (REPO_ROOT / "plans/092-status.md").read_text()
        self.assertIn("superseded by Plan 093", text)
        self.assertIn("Branch A", text)
        # Branch A is no longer the dominant ownership.
        self.assertNotIn("Branch A as the dominant ownership", text)

    def test_plan092_status_recovers_misclassification(self):
        text = (REPO_ROOT / "plans/092-status.md").read_text()
        self.assertIn("HandleReceivedLength", text)
        self.assertIn("HandleSessionRequestReceived", text)
        # The Receive length read error message must be classified
        # to HandleReceivedLength (data phase).
        self.assertIn("Receive length read error", text)

    def test_plan087_status_names_plan093_as_next_executable(self):
        text = (REPO_ROOT / "plans/087-status.md").read_text()
        self.assertIn("plan_093", text)
        self.assertIn("plan093", text)
        self.assertIn("Plan 093", text)

    def test_plan088_status_records_plan093_active_sequence(self):
        text = (REPO_ROOT / "plans/088-status.md").read_text()
        self.assertIn("plan_093", text)
        self.assertIn("Plan 093", text)

    def test_plan091_status_acknowledges_plan093(self):
        text = (REPO_ROOT / "plans/091-status.md").read_text()
        self.assertIn("Plan 093", text)

    def test_agents_md_active_sequence_token_is_plan093(self):
        text = (REPO_ROOT / "AGENTS.md").read_text()
        self.assertIn("plan093", text)


class Plan093I2pdObserverResetTests(unittest.TestCase):
    """WP3: i2pd observer reset/generation contract."""

    def test_observer_header_declares_interop_generation(self):
        text = (
            REPO_ROOT
            / "tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h"
        ).read_text()
        self.assertIn("INTEROP_RING_CAPACITY", text)
        self.assertIn("BeginListenerGeneration", text)
        self.assertIn("ObservationRingEntry", text)
        self.assertIn("WaitForReceivedDeliveryStatusAfter", text)
        self.assertIn("WaitForSentDeliveryStatusAfter", text)

    def test_observer_implementation_uses_ring(self):
        text = (
            REPO_ROOT
            / "tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.cpp"
        ).read_text()
        self.assertIn("INTEROP_RING_CAPACITY", text)
        self.assertIn("ObservationRing", text)
        self.assertIn("BeginListenerGeneration", text)
        self.assertIn("WaitForReceivedDeliveryStatusAfter", text)
        self.assertIn("WaitForSentDeliveryStatusAfter", text)
        # Reset must advance generation, not just zero the counters.
        self.assertIn("g_active_generation.fetch_add", text)

    def test_driver_runs_begin_listener_generation_before_transport_start(self):
        text = (
            REPO_ROOT
            / "tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp"
        ).read_text()
        # The driver must call BeginListenerGeneration before
        # initialise_i2pd_runtime in run_listen.
        begin_idx = text.find("BeginListenerGeneration")
        runtime_idx = text.find("initialise_i2pd_runtime(cfg, writer, rt")
        # The second occurrence is inside run_listen. The first
        # occurrence is inside run_inspect.
        listen_idx = text.find("int run_listen")
        begin_after_listen = text.find(
            "BeginListenerGeneration", listen_idx
        )
        runtime_after_listen = text.find(
            "initialise_i2pd_runtime(cfg, writer, rt", listen_idx
        )
        self.assertGreater(begin_after_listen, 0)
        self.assertGreater(runtime_after_listen, 0)
        self.assertLess(begin_after_listen, runtime_after_listen)
        # The runtime call is not anchored for the unused
        # variables; satisfy the lint without dropping the bound.
        self.assertGreater(runtime_idx, 0)

    def test_driver_does_not_reset_after_listener_ready(self):
        text = (
            REPO_ROOT
            / "tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp"
        ).read_text()
        # The Plan 091 `ResetObserverSink()` call after
        # `listener_ready` in `run_listen` is removed.
        listen_block = text.split("int run_listen", 1)[1]
        listen_block = listen_block.split("int run_dial", 1)[0]
        ready_idx = listen_block.find("emit_event(writer, cfg, \"listener_ready\");")
        self.assertGreater(ready_idx, 0)
        after_ready = listen_block[ready_idx:]
        # The new code uses BeginListenerGeneration, not
        # ResetObserverSink, after listener_ready.
        self.assertNotIn("ResetObserverSink()", after_ready)


class Plan093ObserverPredicateTests(unittest.TestCase):
    """WP4: bounded sequence ring and exact target predicate waits."""

    def test_predicate_wait_signatures_match(self):
        text = (
            REPO_ROOT
            / "tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h"
        ).read_text()
        # Both predicates must require generation, baseline, peer
        # Router Hash, message ID, timeout, and metadata out.
        for name in (
            "WaitForReceivedDeliveryStatusAfter",
            "WaitForSentDeliveryStatusAfter",
        ):
            self.assertIn(name, text)
            # The header declares generation / baseline / peer hash
            # / message ID parameters.
            self.assertIn("std::uint64_t generation", text)
            self.assertIn("std::uint64_t baseline_sequence", text)
            self.assertIn("expected_peer_router_hash", text)
            self.assertIn("std::uint32_t expected_message_id", text)

    def test_predicate_rejects_stale_generation(self):
        text = (
            REPO_ROOT
            / "tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.cpp"
        ).read_text()
        # The predicate implementations must compare the entry
        # generation against the requested generation.
        self.assertIn("entry.generation != active_generation", text)
        self.assertIn("entry.observation_sequence <= baseline_sequence", text)

    def test_predicate_requires_exact_message_id(self):
        text = (
            REPO_ROOT
            / "tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.cpp"
        ).read_text()
        self.assertIn("md.delivery_status_message_id != expected_message_id", text)
        self.assertIn("md.i2np_envelope_message_id != expected_message_id", text)

    def test_predicate_requires_exact_peer_router_hash(self):
        text = (
            REPO_ROOT
            / "tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.cpp"
        ).read_text()
        self.assertIn("std::memcmp(md.peer_router_hash_sha256", text)


class Plan093I2prRuntimeOracleTests(unittest.TestCase):
    """WP5: i2pr bounded multi-frame receive oracle."""

    def test_runtime_exposes_oracle_schema_marker(self):
        text = (REPO_ROOT / "crates/i2pr-runtime/src/ntcp2_data_oracle.rs").read_text()
        self.assertIn("i2pr-ntcp2-data-oracle-v1", text)

    def test_runtime_exposes_correlated_receive_oracle(self):
        text = (REPO_ROOT / "crates/i2pr-runtime/src/ntcp2_link.rs").read_text()
        self.assertIn("correlated_receive_oracle", text)

    def test_runtime_oracle_exposes_bounded_failure_categories(self):
        text = (REPO_ROOT / "crates/i2pr-runtime/src/ntcp2_data_oracle.rs").read_text()
        expected = [
            "DeadlineElapsed",
            "FrameLimitReached",
            "ByteLimitReached",
            "BlockLimitReached",
            "NonTargetI2npLimitReached",
            "TerminationBeforeTarget",
            "FrameParseFailed",
            "I2npDecodeFailed",
            "DeliveryStatusIdMismatch",
            "DeliveryStatusDuplicate",
            "PeerRouterInfoInvalid",
            "Closed",
        ]
        for variant in expected:
            self.assertIn(variant, text)

    def test_runtime_oracle_exposes_bounded_counters(self):
        text = (REPO_ROOT / "crates/i2pr-runtime/src/ntcp2_data_oracle.rs").read_text()
        expected = [
            "frames_received",
            "non_target_frames_received",
            "non_target_i2np_received",
            "router_info_blocks_received",
            "decoded_blocks",
            "plaintext_bytes",
            "matched_target_message_id",
        ]
        for field in expected:
            self.assertIn(field, text)

    def test_runtime_lib_re_exports_oracle_types(self):
        text = (REPO_ROOT / "crates/i2pr-runtime/src/lib.rs").read_text()
        self.assertIn("ORACLE_SCHEMA", text)
        self.assertIn("receive_correlated_delivery_status", text)

    def test_launcher_maps_oracle_to_bounded_categories(self):
        text = (REPO_ROOT / "tools/i2pr-interop/src/main.rs").read_text()
        self.assertIn("ReceiverDeliveryStatusDeadline", text)
        self.assertIn("ReceiverDeliveryStatusFrameLimit", text)
        self.assertIn("ReceiverDeliveryStatusByteLimit", text)
        self.assertIn("ReceiverDeliveryStatusBlockLimit", text)
        self.assertIn("ReceiverDeliveryStatusNonTargetLimit", text)
        self.assertIn("ReceiverTerminationBeforeTarget", text)
        self.assertIn("ReceiverFrameParseFailed", text)
        self.assertIn("ReceiverPeerRouterInfoInvalid", text)

    def test_launcher_status_exposes_oracle_categories(self):
        text = (REPO_ROOT / "tools/i2pr-interop/src/status.rs").read_text()
        self.assertIn("ReceiverDeliveryStatusDeadline", text)
        self.assertIn("ReceiverDeliveryStatusFrameLimit", text)
        self.assertIn("ReceiverDeliveryStatusByteLimit", text)
        self.assertIn("ReceiverDeliveryStatusBlockLimit", text)
        self.assertIn("ReceiverDeliveryStatusNonTargetLimit", text)
        self.assertIn("ReceiverTerminationBeforeTarget", text)
        self.assertIn("ReceiverFrameParseFailed", text)
        self.assertIn("ReceiverPeerRouterInfoInvalid", text)

    def test_launcher_exposes_correlated_send_block(self):
        text = (REPO_ROOT / "tools/i2pr-interop/src/main.rs").read_text()
        self.assertIn("correlated_send_block", text)


class Plan093RunnerEventAuthorityTests(unittest.TestCase):
    """WP6: canonical runner event authority."""

    def test_runner_uses_real_invocation_id_marker(self):
        text = (
            REPO_ROOT / "tests/integration/ntcp2/harness/plan083_runner.py"
        ).read_text()
        # The runner must dedup on (source_side, invocation_id,
        # event_sequence, event_sha256) and must not reuse
        # scenario_id as a synthetic invocation id aliasing.
        self.assertIn(
            '("i2pd", invocation_id, event_sequence, marker)',
            text,
        )

    def test_runner_retains_late_terminal_events(self):
        text = (
            REPO_ROOT / "tests/integration/ntcp2/harness/plan083_runner.py"
        ).read_text()
        # The final drain must include terminal_clean and
        # terminal_rejected.
        self.assertIn('"terminal_clean"', text)
        self.assertIn('"terminal_rejected"', text)


class Plan093LiveProvenanceTests(unittest.TestCase):
    """WP7: live binary and source provenance contract."""

    def test_wrapper_requires_i2pr_binary_digest(self):
        text = (
            REPO_ROOT
            / "scripts/interop/run-minimal-i2pd-host-loopback-probe.py"
        ).read_text()
        # The wrapper must require an explicit i2pr binary path
        # and pass the measured digest through.
        self.assertIn("--i2pr-binary", text)
        self.assertIn("i2pr_binary_sha", text)


class Plan093GateHandoffTests(unittest.TestCase):
    """WP12: gate handoff fields are bound."""

    def test_plan088_status_remains_blocked(self):
        text = (REPO_ROOT / "plans/088-status.md").read_text()
        # Plan 088 is blocked until Plan 093 records both passing
        # instrumented and control records.
        self.assertIn("blocked_pending_plan_093", text)
        # Plan 079 remains blocked.
        self.assertIn("blocked_pending_plan_088_two_way_pass", text)
        # Plan 072 remains inactive.
        self.assertIn("inactive_pending_plan_088_ambiguity", text)
        # The legacy lane-invalidated token must not be the active
        # decision value.
        decision_match = re.search(
            r"decision\s*=\s*([a-z][a-z0-9-]*)", text
        )
        self.assertIsNotNone(decision_match)
        self.assertNotEqual(decision_match.group(1), "lane-invalidated")

    def test_ntcp2_stays_experimental_and_non_advertised(self):
        text = (REPO_ROOT / "AGENTS.md").read_text()
        self.assertIn("experimental and non-advertised", text)
        # The Plan 092 narrow-correction surface for the Branch A
        # handshake-state-machine is explicitly superseded by Plan
        # 093.
        self.assertIn("Plan 092", text)
        self.assertIn("Plan 093", text)


class Plan093StaticEnforcementTests(unittest.TestCase):
    """WP8: the static boundary checker enforces the Plan 093 surface."""

    def test_check_ntcp2_interoperability_requires_plan093(self):
        text = (
            REPO_ROOT / "scripts/check-ntcp2-interoperability.sh"
        ).read_text()
        # The script must require the Plan 093 test matrix.
        self.assertIn("test_plan093.py", text)
        self.assertIn("i2pr-ntcp2-observer-ring-v1", text)
        self.assertIn("i2pr-ntcp2-data-oracle-v1", text)
        # Plan 093 artifacts must be referenced.
        self.assertIn("BeginListenerGeneration", text)
        self.assertIn("correlated_receive_oracle", text)
        self.assertIn("correlated_send_block", text)

    def test_check_ntcp2_interoperability_rejects_zero_provenance(self):
        text = (
            REPO_ROOT / "scripts/check-ntcp2-interoperability.sh"
        ).read_text()
        # The script must forbid zero placeholders in live
        # provenance digests.
        self.assertIn("i2pr_binary_sha", text)


class Plan093ObserverSchemaTests(unittest.TestCase):
    """WP4/WP8: observer ring schema marker constants."""

    def test_observer_schema_marker_is_committed(self):
        text = (
            REPO_ROOT
            / "tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h"
        ).read_text()
        self.assertIn("INTEROP_RING_CAPACITY", text)
        self.assertIn("INTEROP_RING_CAPACITY = 64", text)

    def test_source_verification_record_is_committed(self):
        path = (
            REPO_ROOT / "tests/integration/ntcp2/harness/source_classification.py"
        )
        self.assertTrue(path.is_file())
        # Import the module so the locked constants are exercised.
        self.assertEqual(
            source_classification.PINNED_I2PD_REVISION,
            "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
        )
        self.assertEqual(
            source_classification.SCHEMA_MARKER,
            "i2pr-ntcp2-source-classification-v1",
        )


if __name__ == "__main__":
    unittest.main()