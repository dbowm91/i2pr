#!/usr/bin/env python3
"""Offline parser controls for Plan 478 ACCEPT data-phase close attribution."""

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_plan458_sam_product import (  # noqa: E402
    clean_lane_evidence,
    parse_attempt_receipt_lines,
    read_driver_leg_failure,
    should_wait_for_accept_close,
    wait_for_accept_close_receipt,
)
import plan458_sam_driver as driver  # noqa: E402


FULL_LINE = (
    'WARN data_observed=10 data_accepted=10 data_rejected=0 '
    'data_payload_bytes=2328 '
    'canon_data_observed=2 canon_data_accepted=2 canon_data_rejected=0 '
    'canon_data_payload_bytes=0 '
    'garlic_authenticated=12 garlic_rejected=0 payloads_dequeued=12 '
    'streaming_accepted=2 '
    'connection_state_code=1 undrained_bytes=0 reset=0 bridge_present=1 '
    'transition="stream_accept_data_phase_closed" '
    'STREAM ACCEPT data phase closed'
)

TORN_DOWN_LINE = (
    'WARN data_observed=0 data_accepted=0 data_rejected=0 '
    'data_payload_bytes=0 '
    'canon_data_observed=0 canon_data_accepted=0 canon_data_rejected=0 '
    'canon_data_payload_bytes=0 '
    'garlic_authenticated=0 garlic_rejected=0 payloads_dequeued=0 '
    'streaming_accepted=0 '
    'connection_state_code=0 undrained_bytes=0 reset=1 bridge_present=0 '
    'transition="stream_accept_data_phase_closed" '
    'STREAM ACCEPT data phase closed'
)

EXPECTED = {
    "data_observed": 10, "data_accepted": 10, "data_rejected": 0,
    "data_payload_bytes": 2328,
    "canon_data_observed": 2, "canon_data_accepted": 2,
    "canon_data_rejected": 0, "canon_data_payload_bytes": 0,
    "garlic_authenticated": 12, "garlic_rejected": 0,
    "payloads_dequeued": 12, "streaming_accepted": 2,
    "connection_state_code": 1, "undrained_bytes": 0, "reset": 0,
    "bridge_present": 1,
}


class Plan478AcceptDataCloseReceiptTests(unittest.TestCase):
    def test_full_attribution_line_parses_all_sixteen_counts(self) -> None:
        parsed = parse_attempt_receipt_lines([FULL_LINE])
        # The daemon close warn carries no build_attempt_id, so it
        # contributes no stage count; the receipts list is the evidence.
        self.assertEqual(parsed["stage_counts"], {})
        self.assertEqual(
            parsed["stream_accept_data_phase_close_receipts"], [EXPECTED],
        )

    def test_mirror_invariant_mismatch_refuses_receipt(self) -> None:
        bad = FULL_LINE.replace("data_rejected=0", "data_rejected=3")
        parsed = parse_attempt_receipt_lines([bad])
        self.assertEqual(parsed["stream_accept_data_phase_close_receipts"], [])

    def test_canon_invariant_mismatch_refuses_receipt(self) -> None:
        bad = FULL_LINE.replace(
            "canon_data_accepted=2", "canon_data_accepted=5",
        )
        parsed = parse_attempt_receipt_lines([bad])
        self.assertEqual(parsed["stream_accept_data_phase_close_receipts"], [])

    def test_torn_down_shape_parses_with_bridge_absent(self) -> None:
        # Teardown-before-close reads all zeros with state 0 and
        # bridge_present=0 by construction; it must parse (it is
        # evidence of teardown, not a malformed receipt).
        parsed = parse_attempt_receipt_lines([TORN_DOWN_LINE])
        self.assertEqual(
            parsed["stream_accept_data_phase_close_receipts"],
            [{
                "data_observed": 0, "data_accepted": 0, "data_rejected": 0,
                "data_payload_bytes": 0,
                "canon_data_observed": 0, "canon_data_accepted": 0,
                "canon_data_rejected": 0, "canon_data_payload_bytes": 0,
                "garlic_authenticated": 0, "garlic_rejected": 0,
                "payloads_dequeued": 0, "streaming_accepted": 0,
                "connection_state_code": 0, "undrained_bytes": 0,
                "reset": 1, "bridge_present": 0,
            }],
        )

    def test_nonbinary_bridge_present_refuses_receipt(self) -> None:
        bad = FULL_LINE.replace("bridge_present=1", "bridge_present=2")
        parsed = parse_attempt_receipt_lines([bad])
        self.assertEqual(parsed["stream_accept_data_phase_close_receipts"], [])

    def test_fifteen_key_legacy_line_no_longer_parses(self) -> None:
        # Pre-bridge_present daemon lines fail closed (same shape as
        # the 16->25 handshake transition).
        legacy = FULL_LINE.replace(" bridge_present=1", "")
        parsed = parse_attempt_receipt_lines([legacy])
        self.assertEqual(parsed["stream_accept_data_phase_close_receipts"], [])


class AcceptPumpStartCountTests(unittest.TestCase):
    START_LINE = (
        'INFO transition="stream_accept_pump_started" '
        'raw stream driver started for inbound stream'
    )

    def test_absent_by_default(self) -> None:
        parsed = parse_attempt_receipt_lines([FULL_LINE])
        self.assertEqual(parsed["accept_pump_started"], 0)

    def test_each_start_line_counts_once(self) -> None:
        parsed = parse_attempt_receipt_lines([
            self.START_LINE,
            self.START_LINE,
            'INFO transition="install_committed" build_attempt_id=1',
        ])
        self.assertEqual(parsed["accept_pump_started"], 2)
        # Pump starts carry no build_attempt_id: never a stage count.
        self.assertNotIn("stream_accept_pump_started", parsed["stage_counts"])

    def test_ansi_start_line_counts(self) -> None:
        parsed = parse_attempt_receipt_lines([
            '\x1b[2m2026-10-16T00:00:00.000000Z\x1b[0m INFO ' + self.START_LINE,
        ])
        self.assertEqual(parsed["accept_pump_started"], 1)


class BinaryProvenanceTests(unittest.TestCase):
    def test_provenance_shape(self) -> None:
        from run_plan458_sam_product import router_binary_provenance
        provenance = router_binary_provenance()
        self.assertEqual(
            set(provenance),
            {"bin_mtime", "bin_size", "git_head", "git_dirty_files"},
        )
        self.assertIsInstance(provenance["bin_size"], int)
        self.assertIsInstance(provenance["git_dirty_files"], int)
        # Read-only against the real tree: HEAD resolves, binary stat
        # succeeds when the lane's binary exists.
        self.assertNotEqual(provenance["git_head"], "unknown")

    def test_missing_keys_do_not_create_receipts(self) -> None:
        parsed = parse_attempt_receipt_lines([
            'WARN data_observed=10 data_accepted=10 '
            'transition="stream_accept_data_phase_closed"',
            'INFO transition="install_committed" tunnel_data_received=1',
        ])
        self.assertEqual(parsed["stream_accept_data_phase_close_receipts"], [])
        self.assertEqual(parsed["stage_counts"], {})

    def test_ansi_colored_daemon_lines_still_parse(self) -> None:
        parsed = parse_attempt_receipt_lines([
            '\x1b[2m2026-10-15T00:00:00.000000Z\x1b[0m WARN '
            '\x1b[2mi2pr_daemon::sam\x1b[0m: ' + FULL_LINE,
        ])
        self.assertEqual(len(parsed["stream_accept_data_phase_close_receipts"]), 1)
        self.assertEqual(
            parsed["stream_accept_data_phase_close_receipts"][0]["data_payload_bytes"],
            2328,
        )

    def test_receipts_carry_counts_only(self) -> None:
        parsed = parse_attempt_receipt_lines([FULL_LINE])
        self.assertNotIn(
            "build_attempt_id",
            str(parsed["stream_accept_data_phase_close_receipts"]),
        )
        for receipt in parsed["stream_accept_data_phase_close_receipts"]:
            self.assertTrue(all(isinstance(value, int) for value in receipt.values()))

    def test_handshake_and_close_receipts_parse_independently(self) -> None:
        parsed = parse_attempt_receipt_lines([
            'WARN synack_observed=0 synack_accepted=0 synack_rejected=0 '
            'garlic_authenticated=0 garlic_rejected=0 payloads_dequeued=0 '
            'streaming_accepted=0 tunnel_data_received=0 '
            'tunnel_data_cells_accepted=0 garlic_owner_matches=0 '
            'garlic_owner_misses=0 direct_garlic_received=0 '
            'miss_service_unowned=0 miss_exploratory_known=0 '
            'miss_retired_service=0 miss_unknown=0 '
            'send_delivered=0 send_unknown_peer=0 send_missing_factory=0 '
            'send_factory_exhausted=0 send_delivery_failed=0 '
            'send_failed_ls2_missing=0 send_failed_ls2_invalid=0 '
            'send_failed_ls2_install=0 send_failed_bridge=0 '
            'transition="stream_connect_handshake_wait_expired" x',
            FULL_LINE,
        ])
        self.assertEqual(len(parsed["stream_connect_handshake_wait_receipts"]), 1)
        self.assertEqual(len(parsed["stream_accept_data_phase_close_receipts"]), 1)


class DriverLegFailureSidecarTests(unittest.TestCase):
    def test_missing_sidecar_reads_none(self) -> None:
        with tempfile.TemporaryDirectory() as scratch:
            self.assertIsNone(
                read_driver_leg_failure(Path(scratch) / "result.json"),
            )

    def test_write_then_read_round_trip(self) -> None:
        with tempfile.TemporaryDirectory() as scratch:
            evidence = str(Path(scratch) / "result.json")
            record = driver.write_leg_failure_sidecar(
                evidence, "i2pd-to-i2pr",
                "short stream read (0/2328) after 12.3s", 12.3,
            )
            self.assertEqual(record["leg"], "i2pd-to-i2pr")
            read = read_driver_leg_failure(Path(evidence))
            self.assertEqual(read["leg"], "i2pd-to-i2pr")
            self.assertIn("short stream read (0/2328)", read["error"])
            self.assertEqual(read["elapsed_s"], 12.3)

    def test_error_text_truncated_to_bound(self) -> None:
        with tempfile.TemporaryDirectory() as scratch:
            evidence = str(Path(scratch) / "result.json")
            driver.write_leg_failure_sidecar(
                evidence, "i2pr-to-i2pd", "x" * 500, 3.0,
            )
            read = read_driver_leg_failure(Path(evidence))
            self.assertTrue(len(read["error"]) <= 200)

    def test_wrong_schema_or_leg_refused(self) -> None:
        with tempfile.TemporaryDirectory() as scratch:
            sidecar = Path(scratch) / "result.json.leg-failure.json"
            sidecar.write_text(
                '{"schema": "something-else", "leg": "i2pd-to-i2pr", '
                '"error": "boom", "elapsed_s": 1.0}\n',
                encoding="utf-8",
            )
            self.assertIsNone(read_driver_leg_failure(Path(scratch) / "result.json"))
            sidecar.write_text(
                '{"schema": "plan458-sam-leg-failure-v1", "leg": "sideways", '
                '"error": "boom", "elapsed_s": 1.0}\n',
                encoding="utf-8",
            )
            self.assertIsNone(read_driver_leg_failure(Path(scratch) / "result.json"))

    def test_out_of_range_elapsed_refused(self) -> None:
        with tempfile.TemporaryDirectory() as scratch:
            sidecar = Path(scratch) / "result.json.leg-failure.json"
            sidecar.write_text(
                '{"schema": "plan458-sam-leg-failure-v1", '
                '"leg": "i2pd-to-i2pr", "error": "boom", "elapsed_s": 99999}\n',
                encoding="utf-8",
            )
            self.assertIsNone(read_driver_leg_failure(Path(scratch) / "result.json"))

    def test_startup_clean_removes_stale_sidecar(self) -> None:
        with tempfile.TemporaryDirectory() as scratch:
            evidence = str(Path(scratch) / "result.json")
            driver.write_leg_failure_sidecar(
                evidence, "i2pr-to-i2pd", "previous run failure", 40.2,
            )
            self.assertIsNotNone(read_driver_leg_failure(Path(evidence)))
            clean_lane_evidence(Path(scratch))
            self.assertIsNone(read_driver_leg_failure(Path(evidence)))

    def test_sidecar_path_matches_lane_reader(self) -> None:
        with tempfile.TemporaryDirectory() as scratch:
            evidence = str(Path(scratch) / "result.json")
            self.assertEqual(
                Path(driver.sidecar_path(evidence)),
                Path(scratch) / "result.json.leg-failure.json",
            )


class AcceptCloseGraceWaitTests(unittest.TestCase):
    def test_reverse_leg_arms_the_wait(self) -> None:
        self.assertTrue(should_wait_for_accept_close(
            {"leg": "i2pd-to-i2pr", "error": "short read", "elapsed_s": 99.0},
        ))

    def test_forward_or_no_failure_skips_the_wait(self) -> None:
        self.assertFalse(should_wait_for_accept_close(
            {"leg": "i2pr-to-i2pd", "error": "timeout", "elapsed_s": 40.2},
        ))
        self.assertFalse(should_wait_for_accept_close(None))
        self.assertFalse(should_wait_for_accept_close({"leg": "sideways"}))

    def test_present_marker_returns_at_once(self) -> None:
        with tempfile.TemporaryDirectory() as scratch:
            stdout = Path(scratch) / "i2pr.stdout"
            stdout.write_bytes(
                b'INFO transition="stream_accept_data_phase_closed" x\n',
            )
            found, waited = wait_for_accept_close_receipt(stdout, budget_s=5.0)
            self.assertTrue(found)
            self.assertLessEqual(waited, 5.0)

    def test_absent_marker_waits_out_the_budget(self) -> None:
        with tempfile.TemporaryDirectory() as scratch:
            stdout = Path(scratch) / "i2pr.stdout"
            stdout.write_bytes(b"INFO nothing relevant\n")
            found, waited = wait_for_accept_close_receipt(stdout, budget_s=1.0)
            self.assertFalse(found)
            self.assertGreaterEqual(waited, 1.0)

    def test_late_marker_is_observed_inside_budget(self) -> None:
        import threading

        with tempfile.TemporaryDirectory() as scratch:
            stdout = Path(scratch) / "i2pr.stdout"
            stdout.write_bytes(b"INFO starting\n")

            def flush_late():
                import time as _time
                _time.sleep(0.6)
                with open(stdout, "ab") as handle:
                    handle.write(
                        b'WARN transition="stream_accept_data_phase_closed" '
                        b'data_observed=0 x\n',
                    )

            worker = threading.Thread(target=flush_late)
            worker.start()
            try:
                found, waited = wait_for_accept_close_receipt(stdout, budget_s=5.0)
            finally:
                worker.join()
            self.assertTrue(found)
            self.assertLess(waited, 5.0)


if __name__ == "__main__":
    unittest.main()
