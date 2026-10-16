#!/usr/bin/env python3
"""Offline parser controls for Plan 464/465 sanitized attempt receipts."""

import unittest
from pathlib import Path
import sys

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_plan458_sam_product import parse_attempt_receipt_lines


class AttemptReceiptTests(unittest.TestCase):
    def test_correlates_selection_admission_emission_reply_and_install(self):
        receipt = parse_attempt_receipt_lines(
            [
                'INFO destination build attempt direction=Inbound i2np_message_id=120 '
                'build_attempt_id=7 transition="selected"',
                'INFO destination build attempt build_attempt_id=7 '
                'transition="delivery_admitted"',
                'INFO SSU2 message send receipt i2np_message_ids=[120] '
                'transition="udp_emitted"',
                'INFO destination build attempt build_attempt_id=7 '
                'transition="authenticated_reply_matched"',
                'INFO destination build attempt build_attempt_id=7 '
                'transition="install_committed"',
            ]
        )
        self.assertEqual(receipt["attempts_observed"], 1)
        self.assertEqual(receipt["udp_emission_correlated_attempts"], 1)
        self.assertEqual(receipt["response_matched_attempts"], 1)
        self.assertEqual(receipt["install_committed_attempts"], 1)
        self.assertEqual(receipt["stage_counts"]["delivery_admitted"], 1)
        self.assertEqual(receipt["transport_stage_counts"]["udp_emitted"], 1)

    def test_distinguishes_admission_without_emission(self):
        receipt = parse_attempt_receipt_lines(
            [
                'INFO build_attempt_id=8 i2np_message_id=221 transition="selected"',
                'INFO build_attempt_id=8 transition="delivery_admitted"',
                'WARN i2np_message_ids=[221] transition="datagram_staging_rejected" '
                'SSU2 message send receipt',
            ]
        )
        self.assertEqual(receipt["attempts_observed"], 1)
        self.assertEqual(receipt["udp_emission_correlated_attempts"], 0)
        self.assertEqual(receipt["transport_stage_counts"]["datagram_staging_rejected"], 1)

    def test_multiple_fragments_collapse_to_one_attempt_emission(self):
        receipt = parse_attempt_receipt_lines(
            [
                'INFO build_attempt_id=9 i2np_message_id=322 transition="selected"',
                'INFO i2np_message_ids=[322] transition="udp_emitted" SSU2 message send receipt',
                'INFO i2np_message_ids=[322] transition="udp_emitted" SSU2 message send receipt',
            ]
        )
        self.assertEqual(receipt["udp_emission_correlated_attempts"], 1)
        self.assertEqual(receipt["transport_stage_counts"]["udp_emitted"], 2)

    def test_unattributed_or_malformed_events_do_not_invent_attempts(self):
        receipt = parse_attempt_receipt_lines(
            [
                'INFO transition="delivery_admitted"',
                'INFO build_attempt_id=not-a-number transition="selected"',
                'INFO build_attempt_id=10 i2np_message_id=x transition="selected"',
            ]
        )
        self.assertEqual(receipt["attempts_observed"], 0)
        self.assertEqual(receipt["udp_emission_correlated_attempts"], 0)

    def test_postbuild_summary_retains_only_bounded_owner_and_lease_counts(self):
        receipt = parse_attempt_receipt_lines(
            [
                'WARN inbound_receive_count=2 lease_count=3 '
                'inbound_owner_registered=false inbound_owner_count=1 '
                'router-network SAM destination summary incomplete'
            ]
        )
        self.assertEqual(
            receipt["postbuild_summary"],
            {
                "inbound_receive_count": 2,
                "lease_count": 3,
                "inbound_owner_registered": False,
                "inbound_owner_count": 1,
            },
        )

    def test_owner_handoff_and_eviction_receipts_are_identity_free(self):
        receipt = parse_attempt_receipt_lines(
            [
                'INFO inbound_receive_count=2 inbound_owner_count=2 '
                'message="destination inbound owners installed"',
                'INFO removed_owner=true removed_receive=true '
                'message="destination inbound owner removed on pool eviction"',
            ]
        )
        self.assertEqual(receipt["owner_install_receipts"], [
            {"receive_count": 2, "owner_count": 2}
        ])
        self.assertEqual(receipt["owner_eviction_events"], 1)
        self.assertEqual(receipt["owner_eviction_removed_owner"], 1)
        self.assertEqual(receipt["owner_eviction_removed_receive"], 1)

    def test_bootstrap_outbound_commit_counts_as_install(self):
        # Plan 478 (run 0h6f38y_): the bootstrap install used to be
        # silent, so a bootstrap-only run read as
        # match-without-install. The bootstrap path now emits the
        # same install_committed shape as the service sites; a
        # bootstrap match + commit with no service lines must read
        # install_committed_attempts=1.
        receipt = parse_attempt_receipt_lines(
            [
                'INFO destination build attempt build_attempt_id=1 '
                'direction=Outbound transition="authenticated_reply_matched"',
                'INFO destination build attempt build_attempt_id=1 '
                'direction=Outbound transition="install_committed"',
            ]
        )
        self.assertEqual(receipt["response_matched_attempts"], 1)
        self.assertEqual(receipt["install_committed_attempts"], 1)


if __name__ == "__main__":
    unittest.main()
