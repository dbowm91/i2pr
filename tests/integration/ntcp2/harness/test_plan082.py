"""Plan 082 pre-protocol runner contract tests."""

from __future__ import annotations

import hashlib
import json
import tempfile
import unittest
from pathlib import Path

from unittest import mock

from i2pr import I2prAdapter
from interop_topology import ProcessPlacement
from mixed_runner import (
    PRE_PROTOCOL_REASONS,
    MixedRunError,
    _assert_frozen_run_identity,
    _assert_pre_protocol_material,
    _freeze_minimal_run_identity,
    _router_hash_sha256,
)


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
                source_commit="a" * 40 + ";clean",
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
            self.assertEqual(json.loads(encoded)["source_commit"], "a" * 40)

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

    def test_pre_protocol_reason_catalog_contains_every_required_stage(self) -> None:
        self.assertTrue(
            {
                "i2pr-state-preparation-failed",
                "i2pr-preparation-record-invalid",
                "i2pr-router-info-missing",
                "i2pr-router-info-validation-failed",
                "i2pr-router-hash-invalid",
                "reference-state-preparation-failed",
                "reference-router-info-validation-failed",
                "reference-router-hash-invalid",
                "run-identity-freeze-failed",
                "live-scenario-render-failed",
                "listener-process-start-failed",
                "dialer-process-start-failed",
            }.issubset(PRE_PROTOCOL_REASONS)
        )

    def test_pre_protocol_material_requires_distinct_authentic_values(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            i2pr_info = root / "i2pr.info"
            reference_info = root / "i2pd.info"
            i2pr_info.write_bytes(b"i2pr")
            reference_info.write_bytes(b"i2pd")
            _assert_pre_protocol_material(
                i2pr_router_info_path=i2pr_info,
                reference_router_info_path=reference_info,
                i2pr_router_info_sha256=hashlib.sha256(b"i2pr").hexdigest(),
                reference_router_info_sha256=hashlib.sha256(b"i2pd").hexdigest(),
                i2pr_router_hash_sha256="1" * 64,
                reference_router_hash_sha256="2" * 64,
            )
            with self.assertRaisesRegex(MixedRunError, "reference-router-hash-invalid"):
                _assert_pre_protocol_material(
                    i2pr_router_info_path=i2pr_info,
                    reference_router_info_path=reference_info,
                    i2pr_router_info_sha256=hashlib.sha256(b"i2pr").hexdigest(),
                    reference_router_info_sha256=hashlib.sha256(b"i2pd").hexdigest(),
                    i2pr_router_hash_sha256="1" * 64,
                    reference_router_hash_sha256="1" * 64,
                )

    def test_frozen_run_identity_rejects_mutation(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            path = root / "run-identity.json"
            path.write_bytes(b"{}\n")
            digest = hashlib.sha256(path.read_bytes()).hexdigest()
            _assert_frozen_run_identity(root, digest)
            path.write_bytes(b'{"mutated":true}\n')
            with self.assertRaisesRegex(MixedRunError, "run-identity-freeze-failed"):
                _assert_frozen_run_identity(root, digest)

    def test_failed_live_process_creation_does_not_count_as_started(self) -> None:
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            binary = root / "target" / "debug" / "i2pr-interop"
            binary.parent.mkdir(parents=True)
            binary.write_bytes(b"binary")
            run_root = root / "run"
            placement = ProcessPlacement(
                topology_kind="rootless-sealed-single-netns",
                actor="i2pr",
                command_prefix=(),
            )
            adapter = I2prAdapter(root, run_root, placement=placement)
            with mock.patch("i2pr.BoundedProcess.start", side_effect=OSError("failed")):
                with self.assertRaisesRegex(RuntimeError, "listener-process-start-failed"):
                    adapter.start("listen")
            self.assertIsNone(adapter.process)


if __name__ == "__main__":
    unittest.main()
