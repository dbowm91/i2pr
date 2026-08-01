"""Plan 082 bounded i2pr state-preparation tests."""

from __future__ import annotations

import json
import subprocess
import tempfile
import unittest
from pathlib import Path

from i2pr import _parse_preparation


class PreparationRecordTests(unittest.TestCase):
    def test_accepts_only_the_bounded_success_shape(self) -> None:
        digest = "a" * 64
        record = _parse_preparation(
            json.dumps(
                {
                    "schema": "i2pr-interop-state-prepared-v1",
                    "result": "prepared",
                    "router_hash_sha256": digest,
                    "router_info_sha256": digest,
                    "ntcp2_address_count": 1,
                }
            )
        )
        self.assertIsNotNone(record)

    def test_rejects_extra_or_zero_address_fields(self) -> None:
        digest = "a" * 64
        value = {
            "schema": "i2pr-interop-state-prepared-v1",
            "result": "prepared",
            "router_hash_sha256": digest,
            "router_info_sha256": digest,
            "ntcp2_address_count": 0,
            "path": "/secret",
        }
        self.assertIsNone(_parse_preparation(json.dumps(value)))


class PreparationCommandTests(unittest.TestCase):
    def test_command_produces_signed_state_without_a_live_socket(self) -> None:
        binary = Path(__file__).resolve().parents[4] / "target/debug/i2pr-interop"
        if not binary.is_file():
            self.skipTest("build the i2pr-interop launcher first")
        with tempfile.TemporaryDirectory() as directory:
            state = Path(directory) / "state"
            completed = subprocess.run(
                [
                    str(binary),
                    "ntcp2",
                    "prepare",
                    "--state-dir",
                    str(state),
                    "--local-address",
                    "192.0.2.1",
                    "--local-port",
                    "45680",
                    "--network-id",
                    "99",
                    "--deterministic-seed",
                    "7",
                ],
                capture_output=True,
                text=True,
                check=False,
            )
            self.assertEqual(completed.returncode, 0, completed.stderr)
            record = json.loads(completed.stdout)
            self.assertEqual(record["result"], "prepared")
            self.assertTrue((state / "router.info").is_file())
            self.assertEqual((state / "router.info").stat().st_mode & 0o777, 0o600)


if __name__ == "__main__":
    unittest.main()
