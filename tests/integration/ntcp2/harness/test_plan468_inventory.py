"""Tests for the exact-pinned Plan 468 stock inventory parser."""

from __future__ import annotations

import importlib.util
from pathlib import Path
import tempfile
import unittest


SCRIPT = Path(__file__).resolve().parents[1] / "plan468_private_netdb.py"
SPEC = importlib.util.spec_from_file_location("plan468_private_netdb", SCRIPT)
assert SPEC is not None and SPEC.loader is not None
PLAN468 = importlib.util.module_from_spec(SPEC)
SPEC.loader.exec_module(PLAN468)


class Plan468InventoryTests(unittest.TestCase):
    def test_source_spelling_and_whitespace_are_parsed(self) -> None:
        self.assertEqual(
            PLAN468.parse_netdb_inventory("[info] NetDb: 5 routers loaded (5 floodfils)"),
            (5, 5),
        )

    def test_missing_or_malformed_inventory_fails_closed(self) -> None:
        for text in (
            "NetDb: 5 routers loaded (5 floodfills)",
            "NetDb: routers loaded (5 floodfils)",
        ):
            with self.subTest(text=text):
                self.assertIsNone(PLAN468.parse_netdb_inventory(text))

    def test_latest_startup_inventory_wins(self) -> None:
        self.assertEqual(
            PLAN468.parse_netdb_inventory(
                "NetDb: 4 routers loaded (4 floodfils)\n"
                "NetDb: 5 routers loaded (5 floodfils)"
            ),
            (5, 5),
        )

    def test_four_of_five_is_parsed_but_remains_below_the_live_gate(self) -> None:
        inventory = PLAN468.parse_netdb_inventory("NetDb: 4 routers loaded (5 floodfils)")
        self.assertEqual(inventory, (4, 5))
        self.assertNotEqual(inventory, (5, 5))

    def test_publish_selector_uses_current_day_key_and_excludes_floodfill_owner(self) -> None:
        import hashlib

        owner = bytes.fromhex("11" * 32)
        date = "20261012"
        route = hashlib.sha256(owner + date.encode("ascii")).digest()
        candidate = bytes(byte ^ (1 if index == 31 else 0) for index, byte in enumerate(route))
        farther = bytes(byte ^ (2 if index == 31 else 0) for index, byte in enumerate(route))
        records = [
            (b"owner", "owner", 0, 0, owner),
            (b"candidate", "candidate", 1, 1, candidate),
            (b"stock", "stock", 2, 2, farther),
        ]
        self.assertEqual(PLAN468.stock_publish_target(owner, records, date), candidate)
        self.assertIsNone(PLAN468.stock_publish_target(owner, records, "invalid"))

    def test_ntcp2_only_publish_config_is_scoped_to_live_owner(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            config = Path(directory) / "owner.conf"
            PLAN468.stock_config(config, 12345, 12346, ssu2_enabled=False)
            text = config.read_text()
        self.assertIn("[ssu2]\nenabled = false\npublished = true", text)
        self.assertIn("[ntcp2]\nenabled = true\npublished = true", text)


if __name__ == "__main__":
    unittest.main()
