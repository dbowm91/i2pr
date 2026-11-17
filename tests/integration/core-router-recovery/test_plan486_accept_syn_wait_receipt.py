#!/usr/bin/env python3
"""Offline parser controls for Plan 486 per-instance ACCEPT SYN-wait attribution."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_plan458_sam_product import parse_attempt_receipt_lines  # noqa: E402


FULL_LINE = (
    'WARN syn_observed=1 syn_accepted=1 syn_rejected=0 '
    'syn_rejected_decode=0 syn_rejected_validation=0 '
    'syn_rejected_signature=0 syn_rejected_no_listener=0 '
    'syn_rejected_backlog_full=0 '
    'canon_syn_observed=2 canon_syn_accepted=0 canon_syn_rejected=2 '
    'canon_syn_rejected_decode=0 canon_syn_rejected_validation=0 '
    'canon_syn_rejected_signature=0 canon_syn_rejected_no_listener=2 '
    'canon_syn_rejected_backlog_full=0 '
    'accept_backlog_start=0 accept_backlog_end=1 '
    'transition="stream_accept_syn_wait_expired" '
    'STREAM ACCEPT wait ended without Established'
)

EXPECTED_RECEIPT = {
    "syn_observed": 1, "syn_accepted": 1, "syn_rejected": 0,
    "syn_rejected_decode": 0, "syn_rejected_validation": 0,
    "syn_rejected_signature": 0, "syn_rejected_no_listener": 0,
    "syn_rejected_backlog_full": 0,
    "canon_syn_observed": 2, "canon_syn_accepted": 0, "canon_syn_rejected": 2,
    "canon_syn_rejected_decode": 0, "canon_syn_rejected_validation": 0,
    "canon_syn_rejected_signature": 0, "canon_syn_rejected_no_listener": 2,
    "canon_syn_rejected_backlog_full": 0,
    "accept_backlog_start": 0, "accept_backlog_end": 1,
}


class Plan486AcceptSynWaitReceiptTests(unittest.TestCase):
    def test_full_attribution_line_parses_all_eighteen_counts(self) -> None:
        parsed = parse_attempt_receipt_lines([FULL_LINE])
        self.assertEqual(parsed["stage_counts"], {})
        self.assertEqual(
            parsed["stream_accept_syn_wait_receipts"],
            [EXPECTED_RECEIPT],
        )
        # Both instances satisfy the ingress invariant independently.
        receipt = parsed["stream_accept_syn_wait_receipts"][0]
        self.assertEqual(
            receipt["syn_accepted"] + receipt["syn_rejected"],
            receipt["syn_observed"],
        )
        self.assertEqual(
            receipt["canon_syn_accepted"] + receipt["canon_syn_rejected"],
            receipt["canon_syn_observed"],
        )
        # Both reject-cause splits sum to their aggregate.
        self.assertEqual(
            receipt["syn_rejected_decode"]
            + receipt["syn_rejected_validation"]
            + receipt["syn_rejected_signature"]
            + receipt["syn_rejected_no_listener"]
            + receipt["syn_rejected_backlog_full"],
            receipt["syn_rejected"],
        )
        self.assertEqual(
            receipt["canon_syn_rejected_decode"]
            + receipt["canon_syn_rejected_validation"]
            + receipt["canon_syn_rejected_signature"]
            + receipt["canon_syn_rejected_no_listener"]
            + receipt["canon_syn_rejected_backlog_full"],
            receipt["canon_syn_rejected"],
        )

    def test_mirror_ingress_sum_mismatch_refuses_receipt(self) -> None:
        bad = FULL_LINE.replace("syn_rejected=0", "syn_rejected=1")
        parsed = parse_attempt_receipt_lines([bad])
        self.assertEqual(parsed["stream_accept_syn_wait_receipts"], [])

    def test_mirror_split_sum_mismatch_refuses_receipt(self) -> None:
        bad = FULL_LINE.replace(
            "syn_rejected_backlog_full=0 canon", "syn_rejected_backlog_full=1 canon",
        )
        parsed = parse_attempt_receipt_lines([bad])
        self.assertEqual(parsed["stream_accept_syn_wait_receipts"], [])

    def test_canon_ingress_sum_mismatch_refuses_receipt(self) -> None:
        bad = FULL_LINE.replace("canon_syn_rejected=2", "canon_syn_rejected=0")
        parsed = parse_attempt_receipt_lines([bad])
        self.assertEqual(parsed["stream_accept_syn_wait_receipts"], [])

    def test_canon_split_sum_mismatch_refuses_receipt(self) -> None:
        bad = FULL_LINE.replace(
            "canon_syn_rejected_no_listener=2", "canon_syn_rejected_no_listener=1",
        )
        parsed = parse_attempt_receipt_lines([bad])
        self.assertEqual(parsed["stream_accept_syn_wait_receipts"], [])

    def test_pre_split_summed_line_no_longer_parses(self) -> None:
        # Pre-486 daemon lines carry only the summed shape; the
        # summed shape concealed a total instance miss, so it is
        # refused (same fail-closed shape as the 25->28 and
        # 5-key->10-key transitions).
        legacy = (
            'WARN syn_observed=4 syn_accepted=1 syn_rejected=3 '
            'syn_rejected_decode=0 syn_rejected_validation=1 '
            'syn_rejected_signature=2 syn_rejected_no_listener=0 '
            'syn_rejected_backlog_full=0 '
            'accept_backlog_start=0 accept_backlog_end=1 '
            'transition="stream_accept_syn_wait_expired"'
        )
        parsed = parse_attempt_receipt_lines([legacy])
        self.assertEqual(parsed["stream_accept_syn_wait_receipts"], [])

    def test_instance_miss_shape_parses_with_zero_mirror(self) -> None:
        # The m8v2xk5q lane shape: mirror never saw a SYN while
        # canonical rejected two for no_listener. The split keeps
        # this distinguishable from "no SYN ever arrived".
        line = (
            'WARN syn_observed=0 syn_accepted=0 syn_rejected=0 '
            'syn_rejected_decode=0 syn_rejected_validation=0 '
            'syn_rejected_signature=0 syn_rejected_no_listener=0 '
            'syn_rejected_backlog_full=0 '
            'canon_syn_observed=2 canon_syn_accepted=0 canon_syn_rejected=2 '
            'canon_syn_rejected_decode=0 canon_syn_rejected_validation=0 '
            'canon_syn_rejected_signature=0 canon_syn_rejected_no_listener=2 '
            'canon_syn_rejected_backlog_full=0 '
            'accept_backlog_start=0 accept_backlog_end=0 '
            'transition="stream_accept_syn_wait_expired"'
        )
        parsed = parse_attempt_receipt_lines([line])
        self.assertEqual(len(parsed["stream_accept_syn_wait_receipts"]), 1)
        receipt = parsed["stream_accept_syn_wait_receipts"][0]
        self.assertEqual(receipt["syn_observed"], 0)
        self.assertEqual(receipt["canon_syn_rejected_no_listener"], 2)

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
            parsed["stream_accept_syn_wait_receipts"][0]["syn_observed"], 1,
        )

    def test_receipts_carry_counts_only(self) -> None:
        parsed = parse_attempt_receipt_lines([FULL_LINE])
        self.assertNotIn("build_attempt_id", str(parsed["stream_accept_syn_wait_receipts"]))
        for receipt in parsed["stream_accept_syn_wait_receipts"]:
            self.assertTrue(all(isinstance(value, int) for value in receipt.values()))


if __name__ == "__main__":
    unittest.main()
