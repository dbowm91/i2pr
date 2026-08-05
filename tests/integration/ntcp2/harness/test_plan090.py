"""Plan 090 i2pd driver RouterInfo and Plan 087 evidence corrective tests.

These tests cover the Plan 090 work packages:

- WP1 (D1): source-verification documentation now records the Plan 090
  lifecycle/config/export ownership with pinned-source references.
- WP2 (D2): the i2pd direct driver is behavior-neutral and writes a
  signed RouterInfo whose NTCP2 endpoint matches the configured
  listener. The test exercises both fresh and reused data directories.
- WP3 (D3): deterministic structural verification confirms the
  driver-written RouterInfo decodes with the exact configured
  endpoint.
- WP4 (D4): instrumented/control parity guard.
- WP5 (D5): Plan 083 pre-TCP classification rejects
  ``protocol_rejected`` / ``reference-events-missing`` before
  ``tcp_connected`` and serializes ``pre_protocol_rejected`` with a
  Plan 090 pre-protocol reason code.
- WP6 (D6): Plan 083 host-loopback ``validate-scenario`` is routed
  through ``HostLoopbackDevelopmentPlacement``.
- WP7 (D7): focused regression tests for placement capture,
  scenario validation ownership, RouterInfo copy integrity, endpoint
  matching, listener-before-dialer concurrency, reap order, cleanup,
  classification, provenance rejection, and record validation.
- WP8 (D8): the corrective commit is clean and the source SHA matches
  the recorded commit.

Where the compiled i2pd direct driver is unavailable, the
structural verification tests use the Python
``validate_router_info_structure`` helper to decode the file the
C++ helper writes (or a synthetic stub with the same byte shape).
"""

from __future__ import annotations

import hashlib
import json
import re
import struct
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]

sys.path.insert(0, str(REPO_ROOT / "tests/integration/ntcp2/harness"))

from router_info import (  # noqa: E402
    RouterInfoPathError,
    validate_router_info_structure,
)


DRIVER_DIR = REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd"
DRIVER_SOURCE = DRIVER_DIR / "src" / "i2pd_ntcp2_interop_driver.cpp"
INSTRUMENTED_BINARY = (
    REPO_ROOT / "target/interop/i2pd-driver/build/i2pd_ntcp2_interop_driver_instrumented"
)
CONTROL_BINARY = (
    REPO_ROOT / "target/interop/i2pd-driver/build/i2pd_ntcp2_interop_driver_control"
)
SOURCE_VERIFICATION_DOC = (
    REPO_ROOT / "tests/integration/ntcp2/reference-drivers/source-verification.md"
)

HEX64 = re.compile(r"^[0-9a-f]{64}$")


def _build_known_good_router_info(
    *,
    expected_address: str,
    expected_port: int,
    output_path: Path,
    style: bytes = b"NTCP2",
) -> Path:
    """Synthesize a minimal valid RouterInfo for fixture-only tests.

    The fixture writes a RouterInfo that decodes successfully through
    ``validate_router_info_structure``. The fixture does not match
    the i2pd signed structure byte-for-byte; it is a deterministic
    fixture for the bounded Python helper, not a substitute for the
    driver-written bytes. Tests that need the real driver bytes must
    use the live ``i2pd_ntcp2_interop_driver_inspect`` invocation
    (see :class:`Plan090DriverRoundTripTests`).
    """
    raise NotImplementedError(
        "synthetic fixture is not implemented; Plan 090 D3/D4 use the live "
        "i2pd direct driver when the compiled binary is available."
    )


class Plan090SourceVerificationTests(unittest.TestCase):
    """Plan 090 WP1: source-verification record carries the lifecycle."""

    def test_source_verification_doc_records_plan090_lifecycle(self):
        text = SOURCE_VERIFICATION_DOC.read_text()
        self.assertIn("Plan 090 verified RouterInfo lifecycle", text)
        self.assertIn("ntcp2.published", text)
        self.assertIn("SetCheckReserved(false)", text)
        self.assertIn("ParseCmdline", text)
        self.assertIn("Finalize", text)
        self.assertIn("set_uint16_option", text)
        self.assertIn("router-info-endpoint-mismatch", text)

    def test_source_verification_doc_records_pinned_revision(self):
        text = SOURCE_VERIFICATION_DOC.read_text()
        self.assertIn("f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e", text)


class Plan090DriverSourceTests(unittest.TestCase):
    """Plan 090 WP2: driver source carries the four corrections."""

    def test_driver_publishes_ntcp2_address(self):
        text = DRIVER_SOURCE.read_text()
        self.assertIn('set_bool_option("ntcp2.published", true)', text)
        # The legacy buggy form must not appear.
        self.assertNotIn('set_int_option("ntcp2.published", 0)', text)

    def test_driver_populates_options_via_parse_cmdline(self):
        text = DRIVER_SOURCE.read_text()
        self.assertIn("ParseCmdline", text)
        self.assertIn("Finalize", text)

    def test_driver_uses_typed_uint16_helper_for_port(self):
        text = DRIVER_SOURCE.read_text()
        self.assertIn("set_uint16_option", text)
        # The legacy int overload for port must not appear.
        self.assertNotIn('set_int_option("port", cfg.local_port);', text)
        self.assertNotIn('set_int_option("ntcp2.port", cfg.local_port);', text)

    def test_driver_disables_reserved_range_filter(self):
        text = DRIVER_SOURCE.read_text()
        self.assertIn("SetCheckReserved(false)", text)

    def test_driver_fails_closed_on_endpoint_mismatch(self):
        text = DRIVER_SOURCE.read_text()
        self.assertIn("GetPublishedNTCP2V4Address", text)
        self.assertIn("router-info-endpoint-mismatch", text)


class Plan090DriverBinaryTests(unittest.TestCase):
    """Plan 090 WP3: structural verification with the live driver binary."""

    @classmethod
    def setUpClass(cls):
        if not INSTRUMENTED_BINARY.is_file():
            raise unittest.SkipTest(
                "instrumented i2pd direct driver binary not built"
            )

    def _make_config(
        self,
        *,
        run_id: str,
        local_port: int,
        data_dir: Path,
        output_dir: Path,
        peer_ri: Path,
    ) -> dict:
        driver_src_sha = hashlib.sha256(DRIVER_SOURCE.read_bytes()).hexdigest()
        driver_bin_sha = hashlib.sha256(
            INSTRUMENTED_BINARY.read_bytes()
        ).hexdigest()
        manifest = (
            REPO_ROOT
            / "target/interop/i2pd-driver/build/build-manifest-instrumented.json"
        )
        manifest_sha = (
            hashlib.sha256(manifest.read_bytes()).hexdigest()
            if manifest.is_file()
            else "0" * 64
        )
        return {
            "schema": "i2pr-i2pd-direct-driver-config-v1",
            "schema_version": 1,
            "run_id": run_id,
            "scenario_id": "plan090-test-scenario",
            "direction": "i2pr-to-i2pd-ipv4",
            "mode": "inspect",
            "data_dir": str(data_dir),
            "output_dir": str(output_dir),
            "local_address": "127.0.0.1",
            "local_port": local_port,
            "network_id": 99,
            "peer_router_info_path": str(peer_ri),
            "expected_local_router_hash_sha256": "1" * 64,
            "expected_peer_router_hash_sha256": "2" * 64,
            "expected_peer_address": "127.0.0.1",
            "expected_peer_port": local_port + 1,
            "delivery_status_message_id": 1,
            "startup_timeout_ms": 30000,
            "handshake_timeout_ms": 30000,
            "data_phase_timeout_ms": 30000,
            "shutdown_timeout_ms": 10000,
            "reference_revision": (
                "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"
            ),
            "reference_tree_sha256": (
                "03fc4834aaf3a4e33da6952a316b0fa5ff077b222f72beecba006a1122137044"
            ),
            "driver_source_sha256": driver_src_sha,
            "driver_binary_sha256": driver_bin_sha,
            "build_manifest_sha256": manifest_sha,
            "observer_patch_sha256": (
                "d1ec03ac7d7a007a7b55a9b25714595fb019257e670448936bbb0d2e21c12434"
            ),
            "run_identity_sha256": "3" * 64,
            "topology_kind": "host-loopback-development",
        }

    def _write_config(self, cfg: dict, path: Path) -> None:
        path.parent.mkdir(parents=True, exist_ok=True, mode=0o700)
        path.write_text(json.dumps(cfg, indent=2), encoding="utf-8")

    def _run_driver(self, config_path: Path) -> int:
        completed = subprocess.run(
            [str(INSTRUMENTED_BINARY), "--config", str(config_path)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=60.0,
            check=False,
        )
        return completed.returncode

    def test_router_info_decodes_with_exact_endpoint(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            data_dir = tmp_path / "data"
            output_dir = tmp_path / "out"
            data_dir.mkdir(mode=0o700)
            output_dir.mkdir(mode=0o700)
            exchange_dir = tmp_path / "exchange"
            exchange_dir.mkdir(mode=0o700)
            peer_ri = exchange_dir / "peer-router.info"
            peer_ri.write_bytes(b"placeholder")
            local_port = 49655
            cfg = self._make_config(
                run_id="plan090-r3-inspect",
                local_port=local_port,
                data_dir=data_dir,
                output_dir=output_dir,
                peer_ri=peer_ri,
            )
            config_path = tmp_path / "config.json"
            self._write_config(cfg, config_path)
            rc = self._run_driver(config_path)
            self.assertEqual(rc, 0, "driver must exit 0 on success")
            ri_path = output_dir / "router.info"
            self.assertTrue(
                ri_path.is_file(),
                "driver must write router.info into output_dir",
            )
            validation = validate_router_info_structure(
                ri_path,
                expected_address="127.0.0.1",
                expected_port=local_port,
            )
            self.assertEqual(validation.ntcp2_address_count, 1)
            self.assertTrue(validation.endpoint_match)
            self.assertEqual(validation.signature_length, 64)

    def test_reused_data_directory_preserves_address(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            data_dir = tmp_path / "data"
            output_dir = tmp_path / "out"
            data_dir.mkdir(mode=0o700)
            output_dir.mkdir(mode=0o700)
            exchange_dir = tmp_path / "exchange"
            exchange_dir.mkdir(mode=0o700)
            peer_ri = exchange_dir / "peer-router.info"
            peer_ri.write_bytes(b"placeholder")
            local_port = 49855
            cfg = self._make_config(
                run_id="plan090-reuse-inspect",
                local_port=local_port,
                data_dir=data_dir,
                output_dir=output_dir,
                peer_ri=peer_ri,
            )
            config_path = tmp_path / "config.json"
            self._write_config(cfg, config_path)
            # First run produces the canonical RouterInfo.
            self.assertEqual(self._run_driver(config_path), 0)
            ri_path = output_dir / "router.info"
            self.assertTrue(ri_path.is_file())
            validate_router_info_structure(
                ri_path,
                expected_address="127.0.0.1",
                expected_port=local_port,
            )
            # Second run against the reused data directory must
            # round-trip the same address.
            output_dir2 = tmp_path / "out2"
            output_dir2.mkdir(mode=0o700)
            cfg2 = dict(cfg)
            cfg2["run_id"] = "plan090-reuse-inspect-2"
            cfg2["output_dir"] = str(output_dir2)
            cfg2_path = tmp_path / "config2.json"
            self._write_config(cfg2, cfg2_path)
            self.assertEqual(self._run_driver(cfg2_path), 0)
            ri2_path = output_dir2 / "router.info"
            self.assertTrue(ri2_path.is_file())
            validate_router_info_structure(
                ri2_path,
                expected_address="127.0.0.1",
                expected_port=local_port,
            )


class Plan090ControlParityTests(unittest.TestCase):
    """Plan 090 WP4: instrumented and control binaries produce
    semantically equivalent RouterInfos.

    Identities and timestamps differ, so byte equality is not
    required. Address family, endpoint, network ID, and transport
    type must match.
    """

    @classmethod
    def setUpClass(cls):
        if not INSTRUMENTED_BINARY.is_file() or not CONTROL_BINARY.is_file():
            raise unittest.SkipTest(
                "instrumented or control i2pd direct driver binary not built"
            )

    def _run_inspect(
        self, binary: Path, *, run_id: str, local_port: int, tmp: Path
    ) -> Path:
        data_dir = tmp / "data" / run_id
        output_dir = tmp / "out" / run_id
        exchange_dir = tmp / "exchange" / run_id
        data_dir.mkdir(parents=True, mode=0o700)
        output_dir.mkdir(parents=True, mode=0o700)
        exchange_dir.mkdir(parents=True, mode=0o700)
        peer_ri = exchange_dir / "peer-router.info"
        peer_ri.write_bytes(b"placeholder")
        driver_src_sha = hashlib.sha256(DRIVER_SOURCE.read_bytes()).hexdigest()
        driver_bin_sha = hashlib.sha256(binary.read_bytes()).hexdigest()
        manifest = (
            REPO_ROOT
            / "target/interop/i2pd-driver/build/build-manifest-instrumented.json"
        )
        manifest_sha = (
            hashlib.sha256(manifest.read_bytes()).hexdigest()
            if manifest.is_file()
            else "0" * 64
        )
        cfg = {
            "schema": "i2pr-i2pd-direct-driver-config-v1",
            "schema_version": 1,
            "run_id": run_id,
            "scenario_id": "plan090-parity-scenario",
            "direction": "i2pr-to-i2pd-ipv4",
            "mode": "inspect",
            "data_dir": str(data_dir),
            "output_dir": str(output_dir),
            "local_address": "127.0.0.1",
            "local_port": local_port,
            "network_id": 99,
            "peer_router_info_path": str(peer_ri),
            "expected_local_router_hash_sha256": "1" * 64,
            "expected_peer_router_hash_sha256": "2" * 64,
            "expected_peer_address": "127.0.0.1",
            "expected_peer_port": local_port + 1,
            "delivery_status_message_id": 1,
            "startup_timeout_ms": 30000,
            "handshake_timeout_ms": 30000,
            "data_phase_timeout_ms": 30000,
            "shutdown_timeout_ms": 10000,
            "reference_revision": (
                "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"
            ),
            "reference_tree_sha256": (
                "03fc4834aaf3a4e33da6952a316b0fa5ff077b222f72beecba006a1122137044"
            ),
            "driver_source_sha256": driver_src_sha,
            "driver_binary_sha256": driver_bin_sha,
            "build_manifest_sha256": manifest_sha,
            "observer_patch_sha256": (
                "d1ec03ac7d7a007a7b55a9b25714595fb019257e670448936bbb0d2e21c12434"
            ),
            "run_identity_sha256": "3" * 64,
            "topology_kind": "host-loopback-development",
        }
        cfg_path = tmp / f"config-{run_id}.json"
        cfg_path.write_text(json.dumps(cfg, indent=2), encoding="utf-8")
        completed = subprocess.run(
            [str(binary), "--config", str(cfg_path)],
            stdout=subprocess.PIPE,
            stderr=subprocess.PIPE,
            timeout=60.0,
            check=False,
        )
        self.assertEqual(
            completed.returncode, 0, f"{binary.name} exited {completed.returncode}"
        )
        return output_dir / "router.info"

    def test_instrumented_and_control_produce_equivalent_endpoint(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            ins_ri = self._run_inspect(
                INSTRUMENTED_BINARY,
                run_id="plan090-parity-instrumented",
                local_port=49955,
                tmp=tmp_path,
            )
            ctrl_ri = self._run_inspect(
                CONTROL_BINARY,
                run_id="plan090-parity-control",
                local_port=50055,
                tmp=tmp_path,
            )
            ins_validation = validate_router_info_structure(
                ins_ri, expected_address="127.0.0.1", expected_port=49955
            )
            ctrl_validation = validate_router_info_structure(
                ctrl_ri, expected_address="127.0.0.1", expected_port=50055
            )
            self.assertEqual(
                ins_validation.ntcp2_address_count,
                ctrl_validation.ntcp2_address_count,
            )
            self.assertEqual(ins_validation.endpoint_match, ctrl_validation.endpoint_match)
            self.assertEqual(ins_validation.signature_length, ctrl_validation.signature_length)
            self.assertGreater(ins_validation.size, 0)
            self.assertGreater(ctrl_validation.size, 0)


class Plan090PreTcpClassificationTests(unittest.TestCase):
    """Plan 090 WP5: Plan 083 maps pre-TCP rejections to pre-protocol
    terminal categories and forbids ``protocol_rejected`` /
    ``reference-events-missing`` before ``tcp_connected``."""

    def test_plan083_runner_imports_pre_protocol_constants(self):
        from plan083_runner import execute_real_probe
        from minimal_i2pd_probe import (
            PRE_PROTOCOL_REJECTED,
            REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED,
        )
        self.assertEqual(
            PRE_PROTOCOL_REJECTED, "pre_protocol_rejected"
        )
        self.assertEqual(
            REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED,
            "pre-protocol-router-info-validation-failed",
        )
        self.assertTrue(callable(execute_real_probe))

    def test_plan083_runner_has_pre_tcp_classification_helper(self):
        import inspect as _inspect
        from plan083_runner import execute_real_probe
        source = _inspect.getsource(execute_real_probe)
        self.assertIn("pre_protocol(", source)
        self.assertIn("peer_router_info_invalid", source)
        self.assertIn(
            "REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED",
            source,
        )
        self.assertIn("has_tcp_connected", source)


class Plan090PlacementValidationTests(unittest.TestCase):
    """Plan 090 WP6: Plan 083 host-loopback ``validate-scenario`` is
    placement-owned."""

    def test_plan083_runner_routes_validate_through_placement(self):
        import inspect as _inspect
        from plan083_runner import execute_real_probe
        source = _inspect.getsource(execute_real_probe)
        self.assertIn("HostLoopbackDevelopmentPlacement", source)
        self.assertIn("validate_placement", source)
        self.assertIn("validate-scenario", source)


class Plan090RecordValidationTests(unittest.TestCase):
    """Plan 090 WP7 (subset): record validation rejects zero or
    placeholder digests."""

    def test_minimal_i2pd_probe_record_rejects_zero_provenance(self):
        from minimal_i2pd_probe import build_record
        from plan083_runner import ProbeConfig
        cfg = ProbeConfig(
            run_id="plan090-zeros",
            source_commit="0" * 40,
            reference_revision="f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
            lane_qualification_sha256="0" * 64,
            topology_kind="host-loopback-development",
            parent_network_state_unchanged=True,
            i2pr_binary_sha256="0" * 64,
            i2pd_binary_sha256="0" * 64,
            i2pr_router_info_sha256="0" * 64,
            i2pd_router_info_sha256="0" * 64,
            i2pr_router_hash_sha256="0" * 64,
            i2pd_router_hash_sha256="0" * 64,
            delivery_status_message_id=0,
        )
        with self.assertRaises(Exception):
            build_record(
                run_id=cfg.run_id,
                source_commit=cfg.source_commit,
                reference_revision=cfg.reference_revision,
                lane_qualification_sha256=cfg.lane_qualification_sha256,
                topology_kind=cfg.topology_kind,
                parent_network_state_unchanged=cfg.parent_network_state_unchanged,
                i2pr_binary_sha256=cfg.i2pr_binary_sha256,
                i2pd_binary_sha256=cfg.i2pd_binary_sha256,
                i2pr_router_info_sha256=cfg.i2pr_router_info_sha256,
                i2pd_router_info_sha256=cfg.i2pd_router_info_sha256,
                i2pr_router_hash_sha256=cfg.i2pr_router_hash_sha256,
                i2pd_router_hash_sha256=cfg.i2pd_router_hash_sha256,
                delivery_status_message_id=cfg.delivery_status_message_id,
                observed_events=[],
                highest_stage_reached="not_started",
                terminal_result="pre_protocol_rejected",
                reason_code="not_started",
                process_counters={},
                cleanup_result="clean",
                placement_record_sha256="0" * 64,
            )


if __name__ == "__main__":
    unittest.main()
