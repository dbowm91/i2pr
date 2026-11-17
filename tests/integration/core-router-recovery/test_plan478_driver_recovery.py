#!/usr/bin/env python3
"""Offline controls for the Plan 478 fresh-handshake recovery leg."""

import sys
import unittest
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parent))
import plan458_sam_driver as driver  # noqa: E402


class ForwardRecoveryTests(unittest.TestCase):
    def test_first_try_success_never_recovers(self) -> None:
        calls = []

        def fake_run_leg(*args):
            calls.append(args)
            return {"length": 1, "sha256": "x"}

        real = driver.run_leg
        driver.run_leg = fake_run_leg
        try:
            sent, recovery = driver.run_forward_leg_with_recovery(
                "s", "r", "sid", "rid", "remote", b"p", 1, sleep_fn=lambda _: None,
            )
        finally:
            driver.run_leg = real
        self.assertEqual(sent, {"length": 1, "sha256": "x"})
        self.assertEqual(recovery, {})
        self.assertEqual(len(calls), 1)

    def test_failed_first_handshake_retries_exactly_once(self) -> None:
        calls = []
        slept = []

        def fake_run_leg(*args):
            calls.append(args)
            if len(calls) == 1:
                raise RuntimeError("STREAM CONNECT did not reach Established")
            return {"length": 2, "sha256": "y"}

        real = driver.run_leg
        driver.run_leg = fake_run_leg
        try:
            sent, recovery = driver.run_forward_leg_with_recovery(
                "s", "r", "sid", "rid", "remote", b"p", 1,
                sleep_fn=lambda seconds: slept.append(seconds),
            )
        finally:
            driver.run_leg = real
        self.assertEqual(sent, {"length": 2, "sha256": "y"})
        self.assertTrue(recovery["forward_recovered"])
        self.assertIn("did not reach Established", recovery["forward_first_error"])
        self.assertEqual(len(calls), 2)
        self.assertEqual(slept, [5])

    def test_double_failure_raises_without_third_attempt(self) -> None:
        calls = []

        def fake_run_leg(*args):
            calls.append(args)
            raise TimeoutError("accepting SAM stream did not finish")

        real = driver.run_leg
        driver.run_leg = fake_run_leg
        try:
            with self.assertRaises(TimeoutError):
                driver.run_forward_leg_with_recovery(
                    "s", "r", "sid", "rid", "remote", b"p", 1,
                    sleep_fn=lambda _: None,
                )
        finally:
            driver.run_leg = real
        self.assertEqual(len(calls), 2)


if __name__ == "__main__":
    unittest.main()
