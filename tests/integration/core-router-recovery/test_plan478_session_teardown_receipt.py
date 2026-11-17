#!/usr/bin/env python3
"""Offline parser controls for Plan 478 SAM session-teardown attribution."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_plan458_sam_product import parse_attempt_receipt_lines  # noqa: E402


FULL_LINE = (
    'INFO cause_code=2 session_live=1 driver_live=0 '
    'transition="session_teardown" '
    'SAM session torn down'
)


class Plan478SessionTeardownReceiptTests(unittest.TestCase):
    def test_full_attribution_line_parses_all_three_counts(self) -> None:
        parsed = parse_attempt_receipt_lines([FULL_LINE])
        # The teardown line carries no build_attempt_id, so it
        # contributes no stage count; the receipts list is the evidence.
        self.assertEqual(parsed["stage_counts"], {})
        self.assertEqual(
            parsed["session_teardown_receipts"],
            [{
                "cause_code": 2, "session_live": 1, "driver_live": 0,
            }],
        )

    def test_nonbinary_flags_refuse_receipt(self) -> None:
        for bad in (
            FULL_LINE.replace("session_live=1", "session_live=2"),
            FULL_LINE.replace("driver_live=0", "driver_live=9"),
        ):
            parsed = parse_attempt_receipt_lines([bad])
            self.assertEqual(parsed["session_teardown_receipts"], [])

    def test_unknown_cause_code_refuses_receipt(self) -> None:
        bad = FULL_LINE.replace("cause_code=2", "cause_code=7")
        parsed = parse_attempt_receipt_lines([bad])
        self.assertEqual(parsed["session_teardown_receipts"], [])

    def test_missing_keys_do_not_create_receipts(self) -> None:
        parsed = parse_attempt_receipt_lines([
            'INFO cause_code=2 session_live=1 '
            'transition="session_teardown"',
            'INFO transition="stream_accept_pump_started" x',
        ])
        self.assertEqual(parsed["session_teardown_receipts"], [])
        self.assertEqual(parsed["stage_counts"], {})

    def test_ansi_colored_daemon_lines_still_parse(self) -> None:
        parsed = parse_attempt_receipt_lines([
            '\x1b[2m2026-10-16T00:00:00.000000Z\x1b[0m INFO '
            '\x1b[2mi2pr_daemon::sam\x1b[0m: ' + FULL_LINE,
        ])
        self.assertEqual(len(parsed["session_teardown_receipts"]), 1)
        self.assertEqual(
            parsed["session_teardown_receipts"][0]["cause_code"], 2,
        )

    def test_receipts_carry_counts_only(self) -> None:
        parsed = parse_attempt_receipt_lines([FULL_LINE])
        self.assertNotIn("build_attempt_id", str(parsed["session_teardown_receipts"]))
        for receipt in parsed["session_teardown_receipts"]:
            self.assertTrue(all(isinstance(value, int) for value in receipt.values()))


if __name__ == "__main__":
    unittest.main()
