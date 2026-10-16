#!/usr/bin/env python3
"""Offline parser controls for Plan 478 STREAM ACCEPT SYN-wait attribution."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_plan458_sam_product import parse_attempt_receipt_lines  # noqa: E402


FULL_LINE = (
    'WARN syn_observed=4 syn_accepted=1 syn_rejected=3 '
    'syn_rejected_decode=0 syn_rejected_validation=1 '
    'syn_rejected_signature=2 syn_rejected_no_listener=0 '
    'syn_rejected_backlog_full=0 '
    'accept_backlog_start=0 accept_backlog_end=1 '
    'transition="stream_accept_syn_wait_expired" '
    'STREAM ACCEPT wait ended without Established'
)


class Plan478AcceptSynWaitReceiptTests(unittest.TestCase):
    def test_full_attribution_line_parses_all_ten_counts(self) -> None:
        parsed = parse_attempt_receipt_lines([FULL_LINE])
        self.assertEqual(parsed["stage_counts"], {})
        self.assertEqual(
            parsed["stream_accept_syn_wait_receipts"],
            [{
                "syn_observed": 4, "syn_accepted": 1, "syn_rejected": 3,
                "syn_rejected_decode": 0, "syn_rejected_validation": 1,
                "syn_rejected_signature": 2, "syn_rejected_no_listener": 0,
                "syn_rejected_backlog_full": 0,
                "accept_backlog_start": 0, "accept_backlog_end": 1,
            }],
        )
        # The ingress invariant always holds on a parsed receipt.
        receipt = parsed["stream_accept_syn_wait_receipts"][0]
        self.assertEqual(
            receipt["syn_accepted"] + receipt["syn_rejected"],
            receipt["syn_observed"],
        )
        # The reject-cause split always sums to the aggregate.
        self.assertEqual(
            receipt["syn_rejected_decode"]
            + receipt["syn_rejected_validation"]
            + receipt["syn_rejected_signature"]
            + receipt["syn_rejected_no_listener"]
            + receipt["syn_rejected_backlog_full"],
            receipt["syn_rejected"],
        )

    def test_ingress_sum_mismatch_refuses_receipt(self) -> None:
        bad = FULL_LINE.replace("syn_rejected=3", "syn_rejected=0")
        parsed = parse_attempt_receipt_lines([bad])
        self.assertEqual(parsed["stream_accept_syn_wait_receipts"], [])

    def test_reject_split_sum_mismatch_refuses_receipt(self) -> None:
        bad = FULL_LINE.replace("syn_rejected_signature=2", "syn_rejected_signature=1")
        parsed = parse_attempt_receipt_lines([bad])
        self.assertEqual(parsed["stream_accept_syn_wait_receipts"], [])

    def test_five_key_pre_split_line_no_longer_parses(self) -> None:
        # Pre-split daemon lines lack the reject-cause keys;
        # fail-closed refusal (same shape as the 25->28 transition).
        legacy = (
            'WARN syn_observed=3 syn_accepted=1 syn_rejected=2 '
            'accept_backlog_start=0 accept_backlog_end=1 '
            'transition="stream_accept_syn_wait_expired"'
        )
        parsed = parse_attempt_receipt_lines([legacy])
        self.assertEqual(parsed["stream_accept_syn_wait_receipts"], [])

    def test_missing_keys_do_not_create_receipts(self) -> None:
        parsed = parse_attempt_receipt_lines([
            'WARN syn_observed=0 syn_accepted=0 '
            'transition="stream_accept_syn_wait_expired"',
            'INFO transition="stream_accept_pump_started" x',
        ])
        self.assertEqual(parsed["stream_accept_syn_wait_receipts"], [])
        self.assertEqual(parsed["stage_counts"], {})

    def test_ansi_colored_daemon_lines_still_parse(self) -> None:
        parsed = parse_attempt_receipt_lines([
            '\x1b[2m2026-10-16T00:00:00.000000Z\x1b[0m WARN '
            '\x1b[2mi2pr_daemon::sam\x1b[0m: ' + FULL_LINE,
        ])
        self.assertEqual(len(parsed["stream_accept_syn_wait_receipts"]), 1)
        self.assertEqual(
            parsed["stream_accept_syn_wait_receipts"][0]["syn_observed"], 4,
        )

    def test_receipts_carry_counts_only(self) -> None:
        parsed = parse_attempt_receipt_lines([FULL_LINE])
        self.assertNotIn("build_attempt_id", str(parsed["stream_accept_syn_wait_receipts"]))
        for receipt in parsed["stream_accept_syn_wait_receipts"]:
            self.assertTrue(all(isinstance(value, int) for value in receipt.values()))


if __name__ == "__main__":
    unittest.main()
