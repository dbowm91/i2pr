#!/usr/bin/env python3
"""Offline parser controls for Plan 478 STREAM handshake-wait attribution."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_plan458_sam_product import parse_attempt_receipt_lines  # noqa: E402


FULL_LINE = (
    'WARN synack_observed=0 synack_accepted=0 synack_rejected=0 '
    'garlic_authenticated=0 garlic_rejected=0 payloads_dequeued=0 '
    'streaming_accepted=0 tunnel_data_received=4 tunnel_data_cells_accepted=4 '
    'garlic_owner_matches=2 garlic_owner_misses=4 direct_garlic_received=0 '
    'miss_service_unowned=1 miss_exploratory_known=1 miss_retired_service=2 '
    'miss_unknown=0 '
    'send_delivered=8 send_unknown_peer=0 send_missing_factory=0 '
    'send_factory_exhausted=0 send_delivery_failed=1 send_failed_ls2_missing=0 '
    'send_failed_ls2_invalid=0 send_failed_ls2_install=0 send_failed_bridge=1 '
    'transition="stream_connect_handshake_wait_expired" '
    'STREAM CONNECT wait ended without Established'
)


class Plan478HandshakeWaitReceiptTests(unittest.TestCase):
    def test_full_attribution_line_parses_all_twenty_five_counts(self) -> None:
        parsed = parse_attempt_receipt_lines([FULL_LINE])
        # The daemon TIMEOUT warn carries no build_attempt_id, so it
        # contributes no stage count; the receipts list is the evidence.
        self.assertEqual(parsed["stage_counts"], {})
        self.assertEqual(
            parsed["stream_connect_handshake_wait_receipts"],
            [{
                "synack_observed": 0, "synack_accepted": 0, "synack_rejected": 0,
                "garlic_authenticated": 0, "garlic_rejected": 0,
                "payloads_dequeued": 0, "streaming_accepted": 0,
                "tunnel_data_received": 4, "tunnel_data_cells_accepted": 4,
                "garlic_owner_matches": 2, "garlic_owner_misses": 4,
                "direct_garlic_received": 0,
                "miss_service_unowned": 1, "miss_exploratory_known": 1,
                "miss_retired_service": 2, "miss_unknown": 0,
                "send_delivered": 8, "send_unknown_peer": 0,
                "send_missing_factory": 0, "send_factory_exhausted": 0,
                "send_delivery_failed": 1,
                "send_failed_ls2_missing": 0, "send_failed_ls2_invalid": 0,
                "send_failed_ls2_install": 0, "send_failed_bridge": 1,
            }],
        )
        # The class counts always sum to the aggregate miss count.
        receipt = parsed["stream_connect_handshake_wait_receipts"][0]
        self.assertEqual(
            receipt["miss_service_unowned"]
            + receipt["miss_exploratory_known"]
            + receipt["miss_retired_service"]
            + receipt["miss_unknown"],
            receipt["garlic_owner_misses"],
        )
        # The send split always sums to the send-failure aggregate.
        self.assertEqual(
            receipt["send_failed_ls2_missing"]
            + receipt["send_failed_ls2_invalid"]
            + receipt["send_failed_ls2_install"]
            + receipt["send_failed_bridge"],
            receipt["send_delivery_failed"],
        )

    def test_class_sum_mismatch_refuses_receipt(self) -> None:
        bad = FULL_LINE.replace("garlic_owner_misses=4", "garlic_owner_misses=9")
        parsed = parse_attempt_receipt_lines([bad])
        self.assertEqual(parsed["stream_connect_handshake_wait_receipts"], [])

    def test_send_split_sum_mismatch_refuses_receipt(self) -> None:
        bad = FULL_LINE.replace("send_failed_bridge=1", "send_failed_bridge=0")
        parsed = parse_attempt_receipt_lines([bad])
        self.assertEqual(parsed["stream_connect_handshake_wait_receipts"], [])

    def test_sixteen_key_legacy_line_no_longer_parses(self) -> None:
        # Pre-split daemon lines lack the send keys; fail-closed
        # refusal (same shape as the 12->15->16 transitions).
        legacy = (
            'WARN synack_observed=0 synack_accepted=0 synack_rejected=0 '
            'garlic_authenticated=0 garlic_rejected=0 payloads_dequeued=0 '
            'streaming_accepted=0 tunnel_data_received=4 '
            'tunnel_data_cells_accepted=4 garlic_owner_matches=2 '
            'garlic_owner_misses=4 direct_garlic_received=0 '
            'miss_service_unowned=1 miss_exploratory_known=1 '
            'miss_retired_service=2 miss_unknown=0 '
            'transition="stream_connect_handshake_wait_expired"'
        )
        parsed = parse_attempt_receipt_lines([legacy])
        self.assertEqual(parsed["stream_connect_handshake_wait_receipts"], [])

    def test_missing_keys_do_not_create_receipts(self) -> None:
        parsed = parse_attempt_receipt_lines([
            'WARN synack_observed=0 synack_accepted=0 '
            'transition="stream_connect_handshake_wait_expired"',
            'INFO transition="install_committed" tunnel_data_received=1',
        ])
        self.assertEqual(parsed["stream_connect_handshake_wait_receipts"], [])
        self.assertEqual(parsed["stage_counts"], {})

    def test_ansi_colored_daemon_lines_still_parse(self) -> None:
        parsed = parse_attempt_receipt_lines([
            '\x1b[2m2026-10-15T00:00:00.000000Z\x1b[0m WARN '
            '\x1b[2mi2pr_daemon::sam\x1b[0m: ' + FULL_LINE,
        ])
        self.assertEqual(len(parsed["stream_connect_handshake_wait_receipts"]), 1)
        self.assertEqual(
            parsed["stream_connect_handshake_wait_receipts"][0]["tunnel_data_received"], 4,
        )

    def test_receipts_carry_counts_only(self) -> None:
        parsed = parse_attempt_receipt_lines([FULL_LINE])
        self.assertNotIn("build_attempt_id", str(parsed["stream_connect_handshake_wait_receipts"]))
        for receipt in parsed["stream_connect_handshake_wait_receipts"]:
            self.assertTrue(all(isinstance(value, int) for value in receipt.values()))


if __name__ == "__main__":
    unittest.main()
