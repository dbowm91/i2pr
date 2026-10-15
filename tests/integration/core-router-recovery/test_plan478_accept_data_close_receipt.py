#!/usr/bin/env python3
"""Offline parser controls for Plan 478 ACCEPT data-phase close attribution."""

import sys
import tempfile
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_plan458_sam_product import (  # noqa: E402
    parse_attempt_receipt_lines,
    read_driver_leg_failure,
)
import plan458_sam_driver as driver  # noqa: E402


FULL_LINE = (
    'WARN data_observed=10 data_accepted=10 data_rejected=0 '
    'data_payload_bytes=2328 '
    'canon_data_observed=2 canon_data_accepted=2 canon_data_rejected=0 '
    'canon_data_payload_bytes=0 '
    'garlic_authenticated=12 garlic_rejected=0 payloads_dequeued=12 '
    'streaming_accepted=2 '
    'connection_state_code=1 undrained_bytes=0 reset=0 '
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
}


class Plan478AcceptDataCloseReceiptTests(unittest.TestCase):
    def test_full_attribution_line_parses_all_fifteen_counts(self) -> None:
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


if __name__ == "__main__":
    unittest.main()
