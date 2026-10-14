from __future__ import annotations

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
from run_plan458_sam_product import parse_attempt_receipt_lines  # noqa: E402


class Plan474BuildWaitReceiptTests(unittest.TestCase):
    def test_wait_receipt_keeps_only_bounded_coordinator_counts(self) -> None:
        parsed = parse_attempt_receipt_lines([
            'WARN destination inbound build attempt build_attempt_id=4 '
            'inbound_routed=0 inbound_orphans=0 inbound_orphans_peer_match=0 '
            'inbound_orphans_message_match=0 inbound_orphans_peer_id_plus_one=0 '
            'inbound_orphans_peer_id_minus_one=0 inbound_orphans_peer_id_other=1 '
            'inbound_gateway_messages=3 '
            'inbound_gateway_peer_match=1 inbound_gateway_route_match=0 '
            'authenticated_candidates=2 candidate_peer_matches=1 '
            'candidate_message_id_matches=1 candidate_peer_match_id_plus_one=1 '
            'candidate_peer_match_id_minus_one=0 candidate_peer_match_id_other=0 '
            'pending_builds=1 '
            'transition="inbound_build_wait_expired"',
        ])
        self.assertEqual(parsed["attempts_observed"], 0)
        self.assertEqual(parsed["stage_counts"], {"inbound_build_wait_expired": 1})
        self.assertEqual(
            parsed["inbound_build_wait_receipts"],
            [{"inbound_routed": 0, "inbound_orphans": 0,
              "inbound_orphans_peer_match": 0,
              "inbound_orphans_message_match": 0,
              "inbound_orphans_peer_id_plus_one": 0,
              "inbound_orphans_peer_id_minus_one": 0,
              "inbound_orphans_peer_id_other": 1,
              "inbound_gateway_messages": 3,
              "inbound_gateway_peer_match": 1,
              "inbound_gateway_route_match": 0,
              "authenticated_candidates": 2,
              "candidate_peer_matches": 1,
              "candidate_message_id_matches": 1,
              "candidate_peer_match_id_plus_one": 1,
              "candidate_peer_match_id_minus_one": 0,
              "candidate_peer_match_id_other": 0,
              "pending_builds": 1}],
        )
        self.assertNotIn("build_attempt_id", str(parsed))

    def test_malformed_or_non_expiry_lines_do_not_create_receipts(self) -> None:
        parsed = parse_attempt_receipt_lines([
            'WARN destination inbound build attempt inbound_routed=0 '
            'inbound_orphans=0 inbound_orphans_peer_match=0 '
            'inbound_orphans_message_match=0 inbound_orphans_peer_id_plus_one=0 '
            'inbound_orphans_peer_id_minus_one=0 inbound_orphans_peer_id_other=1 '
            'inbound_gateway_messages=3 '
            'inbound_gateway_peer_match=1 inbound_gateway_route_match=0 '
            'transition="inbound_build_wait_expired"',
            'INFO transition="install_committed" inbound_routed=1 '
            'inbound_orphans=0 pending_builds=0',
        ])
        self.assertEqual(parsed["inbound_build_wait_receipts"], [])

    def test_ansi_colored_daemon_lines_still_parse(self) -> None:
        parsed = parse_attempt_receipt_lines([
            '\x1b[2m2026-10-14T03:09:39.742192Z\x1b[0m INFO '
            '\x1b[2mi2pr_daemon::service_product\x1b[0m: '
            'destination build source-owned delivery result '
            '\x1b[3mdirection\x1b[0m\x1b[2m=\x1b[0mOutbound '
            '\x1b[3moriginator_write_disposition\x1b[0m\x1b[2m=\x1b[0m'
            'AllOriginatorDeliveriesEmitted '
            '\x1b[3mtransition\x1b[0m\x1b[2m=\x1b[0m"build_wait_expired"',
        ])
        self.assertEqual(
            parsed["source_owned_delivery_dispositions"],
            {"outbound:all-emitted": 1},
        )


if __name__ == "__main__":
    unittest.main()
