"""Plan 082 pre-protocol runner contract tests."""

from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from mixed_runner import _freeze_minimal_run_identity, _router_hash_sha256


class Plan082ContractTests(unittest.TestCase):
    def test_identity_freeze_is_canonical_and_excludes_self_digest(self) -> None:
        from mixed_runner import MixedDirection

        metadata = type("Metadata", (), {"source_revision": "b" * 40, "artifact_sha256": "c" * 64})()
        direction = MixedDirection(
            "i2pr-to-i2pd-ipv4", "i2pd", "development", "i2pr-to-i2pd",
            "ipv4", "minimum-variable-maximum", "typed", "i2pr", "i2pd",
        )
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            digest = _freeze_minimal_run_identity(
                run_dir=root,
                run_id="run-1",
                source_commit="a" * 40,
                direction=direction,
                metadata=metadata,
                i2pr_router_info_sha256="1" * 64,
                i2pd_router_info_sha256="2" * 64,
                i2pr_router_hash_sha256="3" * 64,
                i2pd_router_hash_sha256="4" * 64,
                i2pr_binary_sha256="5" * 64,
                i2pd_binary_sha256="6" * 64,
                delivery_status_message_id=7,
            )
            encoded = (root / "run-identity.json").read_bytes()
            self.assertEqual(digest, hashlib.sha256(encoded).hexdigest())
            self.assertNotIn(b"run_identity_sha256", encoded)
            self.assertEqual(json.loads(encoded)["schema"], "i2pr-minimal-run-identity-v1")

    def test_router_hash_uses_identity_prefix_not_file_name(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "arbitrary-name"
            data = bytearray(391)
            data[384] = 5
            data[385:387] = (4).to_bytes(2, "big")
            path.write_bytes(data)
            self.assertEqual(_router_hash_sha256(path), hashlib.sha256(data[:391]).hexdigest())

    def test_no_generation_scenario_is_constructed(self) -> None:
        source = Path(__file__).with_name("mixed_runner.py").read_text(encoding="utf-8")
        self.assertNotIn('execution_id + "-gen"', source)
        self.assertNotIn('run_identity_sha256=""', source)


if __name__ == "__main__":
    unittest.main()
