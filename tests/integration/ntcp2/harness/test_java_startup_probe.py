"""Plan 052 Java startup probe unit tests.

These tests cover the probe argument parsing, entropy classification, and
allowlisted inventory. They do NOT spawn the Java router; the probe is
exercised end-to-end on real Java installations during the Plan 052
controlled matrix (Workstream E2).
"""

from __future__ import annotations

import json
import os
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent


class JavaStartupProbeArgumentTests(unittest.TestCase):
    def test_help_runs(self):
        result = subprocess.run(
            [sys.executable, str(HERE / "java_startup_probe.py"), "--help"],
            capture_output=True, text=True, check=False,
        )
        self.assertEqual(result.returncode, 0)
        self.assertIn("reference-install", result.stdout)

    def test_rejects_unknown_data_state(self):
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "java_startup_probe.py"),
                    "--reference-install", str(Path(directory) / "install"),
                    "--data-dir", str(Path(directory) / "data"),
                    "--data-state", "rogue",
                    "--launcher", "runplain",
                    "--namespace", "outer",
                    "--output", str(Path(directory) / "out.json"),
                ],
                capture_output=True, text=True, check=False,
            )
            self.assertNotEqual(result.returncode, 0)

    def test_rejects_unknown_launcher(self):
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "java_startup_probe.py"),
                    "--reference-install", str(Path(directory) / "install"),
                    "--data-dir", str(Path(directory) / "data"),
                    "--data-state", "empty",
                    "--launcher", "rogue",
                    "--namespace", "outer",
                    "--output", str(Path(directory) / "out.json"),
                ],
                capture_output=True, text=True, check=False,
            )
            self.assertNotEqual(result.returncode, 0)

    def test_rejects_unknown_namespace(self):
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "java_startup_probe.py"),
                    "--reference-install", str(Path(directory) / "install"),
                    "--data-dir", str(Path(directory) / "data"),
                    "--data-state", "empty",
                    "--launcher", "runplain",
                    "--namespace", "rogue",
                    "--output", str(Path(directory) / "out.json"),
                ],
                capture_output=True, text=True, check=False,
            )
            self.assertNotEqual(result.returncode, 0)

    def test_rejects_zero_attempts(self):
        with tempfile.TemporaryDirectory() as directory:
            result = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "java_startup_probe.py"),
                    "--reference-install", str(Path(directory) / "install"),
                    "--data-dir", str(Path(directory) / "data"),
                    "--data-state", "empty",
                    "--launcher", "runplain",
                    "--namespace", "outer",
                    "--attempts", "0",
                    "--output", str(Path(directory) / "out.json"),
                ],
                capture_output=True, text=True, check=False,
            )
            self.assertNotEqual(result.returncode, 0)

    def test_rejects_missing_launcher(self):
        with tempfile.TemporaryDirectory() as directory:
            install = Path(directory) / "install"
            data = Path(directory) / "data"
            output = Path(directory) / "out.json"
            result = subprocess.run(
                [
                    sys.executable,
                    str(HERE / "java_startup_probe.py"),
                    "--reference-install", str(install),
                    "--data-dir", str(data),
                    "--data-state", "empty",
                    "--launcher", "runplain",
                    "--namespace", "outer",
                    "--attempts", "1",
                    "--output", str(output),
                ],
                capture_output=True, text=True, check=False,
            )
            self.assertNotEqual(result.returncode, 0)
            # Probe always writes a sanitized failure record for the run;
            # the failure is signalled via exit code AND via the failures
            # list inside the record.
            self.assertTrue(output.exists())
            payload = json.loads(output.read_text(encoding="utf-8"))
            self.assertGreater(len(payload["failures"]), 0)


class JavaStartupProbeEntropyTests(unittest.TestCase):
    def test_entropy_classifier_returns_typed_value(self):
        from java_startup_probe import _entropy_class
        result = _entropy_class()
        self.assertIn(result, {"ok", "degraded", "unavailable", "not-tested"})

    def test_entropy_classifier_reads_urandom(self):
        from java_startup_probe import _entropy_class
        result = _entropy_class()
        if os.path.exists("/dev/urandom"):
            self.assertIn(result, {"ok", "degraded"})


class JavaStartupProbeInventoryTests(unittest.TestCase):
    def test_inventory_allowlisted_returns_empty_for_no_files(self):
        from java_startup_probe import _inventory_allowlisted
        with tempfile.TemporaryDirectory() as directory:
            inventory = _inventory_allowlisted(Path(directory))
            self.assertEqual(inventory, [])

    def test_inventory_allowlisted_rejects_non_private_mode(self):
        from java_startup_probe import _inventory_allowlisted, ProbeError
        with tempfile.TemporaryDirectory() as directory:
            data = Path(directory)
            target = data / "router.config"
            target.write_text("config", encoding="ascii")
            target.chmod(0o644)
            with self.assertRaises(ProbeError):
                _inventory_allowlisted(data)

    def test_inventory_allowlisted_returns_size_and_sha256(self):
        from java_startup_probe import _inventory_allowlisted
        with tempfile.TemporaryDirectory() as directory:
            data = Path(directory)
            target = data / "router.config"
            target.write_text("router-config-body", encoding="ascii")
            target.chmod(0o600)
            inventory = _inventory_allowlisted(data)
            self.assertEqual(len(inventory), 1)
            entry = inventory[0]
            self.assertEqual(entry["name"], "router.config")
            self.assertEqual(entry["size"], "18")
            self.assertEqual(len(entry["sha256"],), 64)


class JavaStartupProbeDataStateTests(unittest.TestCase):
    def test_empty_state_creates_directory(self):
        from java_startup_probe import _ensure_data_state
        with tempfile.TemporaryDirectory() as directory:
            data = Path(directory) / "data"
            _ensure_data_state(template=Path(""), data_dir=data, data_state="empty")
            self.assertTrue(data.exists())

    def test_fresh_seed_state_writes_private_seed(self):
        from java_startup_probe import _ensure_data_state
        with tempfile.TemporaryDirectory() as directory:
            data = Path(directory) / "data"
            _ensure_data_state(template=Path(""), data_dir=data, data_state="fresh-unique-seed")
            seed = data / "prngseed.rnd"
            self.assertTrue(seed.exists())
            self.assertEqual(seed.stat().st_size, 64)
            self.assertEqual(seed.stat().st_mode & 0o777, 0o600)

    def test_unknown_state_raises(self):
        from java_startup_probe import _ensure_data_state, ProbeError
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ProbeError):
                _ensure_data_state(
                    template=Path(""),
                    data_dir=Path(directory),
                    data_state="unknown-state",
                )

    def test_config_only_requires_template(self):
        from java_startup_probe import _ensure_data_state, ProbeError
        with tempfile.TemporaryDirectory() as directory:
            with self.assertRaises(ProbeError):
                _ensure_data_state(
                    template=Path("/nonexistent"),
                    data_dir=Path(directory),
                    data_state="config-only",
                )


if __name__ == "__main__":
    unittest.main()