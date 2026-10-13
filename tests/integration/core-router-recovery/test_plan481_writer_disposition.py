"""Sanitized Plan 481 build writer receipt parsing controls."""

import sys
from pathlib import Path
import unittest

ROOT = Path(__file__).resolve().parents[3]
sys.path.insert(0, str(ROOT / "tests/integration/core-router-recovery"))

from run_plan458_sam_product import parse_attempt_receipt_lines  # noqa: E402


class Plan481WriterDispositionTests(unittest.TestCase):
    def test_accepts_only_allowlisted_source_owned_disposition(self):
        parsed = parse_attempt_receipt_lines([
            'direction="Inbound" originator_write_disposition="AllOriginatorDeliveriesEmitted" '
            'transition="build_wait_expired" message="destination build source-owned delivery result" '
            'attempt_id=912 i2np_message_id=987654',
            'direction="Outbound" originator_write_disposition="PartialEmission" '
            'transition="build_wait_expired" message="destination build source-owned delivery result"',
        ])
        self.assertEqual(
            parsed["source_owned_delivery_dispositions"],
            {"inbound:all-emitted": 1, "outbound:partial-emission": 1},
        )
        self.assertNotIn("912", str(parsed))
        self.assertNotIn("987654", str(parsed))

    def test_rejects_unrecognized_or_unattributed_disposition(self):
        parsed = parse_attempt_receipt_lines([
            'direction="Outbound" originator_write_disposition="Forged" '
            'transition="build_wait_expired" message="destination build source-owned delivery result"',
            'originator_write_disposition="AllOriginatorDeliveriesEmitted" '
            'message="destination build source-owned delivery result"',
        ])
        self.assertEqual(parsed["source_owned_delivery_dispositions"], {})


if __name__ == "__main__":
    unittest.main()
