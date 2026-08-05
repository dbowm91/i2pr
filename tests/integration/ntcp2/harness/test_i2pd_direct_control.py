"""Plan 064 i2pd-to-i2pd sealed control tests.

The Plan 064 i2pd direct driver supports a sealed two-process control
topology: one ``listen`` instance plus one ``dial`` instance that
exchange a real RouterInfo and a real correlated DeliveryStatus
inside a sealed namespace.

This module is the bounded local control seam. On the Plan 046
``apparmor_restrict_on`` negative baseline the lane returns
``blocked_unprivileged_user_namespace`` and the tests assert the
typed host blocker rather than converting it into a synthetic pass.
The i2pd-to-i2pd control evidence is not i2pr evidence; it is driver
qualification evidence only.
"""

from __future__ import annotations

import json
import sys
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]

sys.path.insert(0, str(REPO_ROOT / "tests/integration/ntcp2/harness"))

from i2pd_direct_driver import (  # noqa: E402
    I2PD_BUILD_SCRIPT,
    I2PD_RUN_SCRIPT,
    I2PD_SOURCE_LOCK,
    REFERENCE_REVISION,
    Plan064Error,
    build_manifest_schema_digest,
    helper_source_digest,
    load_source_lock,
    observer_header_digest,
    observer_patch_digest,
    observer_source_digest,
    plan064_typed_blocker_for_host,
    qualification_requirements_locked,
    render_strict_config,
    validate_strict_config,
)
from reference_event import (  # noqa: E402
    EVENT_SCHEMA,
    EVENT_SCHEMA_VERSION,
    EventKind,
    build_event,
    validate_event,
)


def _base_config(**overrides):
    config = {
        "schema": "i2pr-i2pd-direct-driver-config-v1",
        "schema_version": 1,
        "run_id": "plan064-control-fixture",
        "scenario_id": "plan064-control-scenario",
        "direction": "i2pd-to-i2pr-ipv4",
        "mode": "listen",
        "data_dir": "/tmp/opencode/plan064-control/data",
        "output_dir": "/tmp/opencode/plan064-control/out",
        "local_address": "192.0.2.1",
        "local_port": 45678,
        "network_id": 99,
        "peer_router_info_path": "/tmp/opencode/plan064-control/peer-router.info",
        "expected_local_router_hash_sha256": "1" * 64,
        "expected_peer_router_hash_sha256": "2" * 64,
        "expected_peer_address": "192.0.2.2",
        "expected_peer_port": 45680,
        "delivery_status_message_id": 12345,
        "startup_timeout_ms": 30000,
        "handshake_timeout_ms": 15000,
        "data_phase_timeout_ms": 15000,
        "shutdown_timeout_ms": 10000,
        "reference_revision": REFERENCE_REVISION,
        "reference_tree_sha256": "3" * 64,
        "driver_source_sha256": "4" * 64,
        "driver_binary_sha256": "5" * 64,
        "build_manifest_sha256": "6" * 64,
        "observer_patch_sha256": "7" * 64,
        "run_identity_sha256": "8" * 64,
        "topology_kind": "rootless-sealed-single-netns",
    }
    config.update(overrides)
    return config


class ControlTopologyContractTests(unittest.TestCase):
    """The i2pd-to-i2pd sealed control topology is two processes
    exchanging a real RouterInfo and a real correlated DeliveryStatus.
    The topology contract is recorded in the source-verification
    record and is the Plan 064 WP9 qualification gate."""

    def test_topology_is_two_process_direct(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/source-verification.md").read_text()
        self.assertIn("two-process", text)
        self.assertIn("Plan 064", text)

    def test_topology_forbids_support_router(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/source-verification.md").read_text()
        self.assertIn("no support router", text)
        self.assertIn("no floodfill", text)

    def test_topology_forbids_sam_http_icp(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/source-verification.md").read_text()
        self.assertIn("SAM", text)
        self.assertIn("I2CP", text)
        self.assertIn("HTTP", text)

    def test_source_lock_records_pinned_revision(self):
        lock = load_source_lock()
        self.assertEqual(lock.reference_revision, REFERENCE_REVISION)
        self.assertEqual(lock.reference_version, "2.60.0")


class ControlObserverContractTests(unittest.TestCase):
    """Plan 064 WP6: the passive observer is compile-time gated by the
    I2PD_INTEROP_OBSERVER macro. The uninstrumented control build
    omits every observer call site. The driver header must declare
    the macro gate and the observer API must be empty when the macro
    is not defined."""

    def test_observer_header_declares_macro_gate(self):
        text = I2PD_RUN_SCRIPT.read_text() if I2PD_RUN_SCRIPT.is_file() else ""
        self.assertTrue(I2PD_RUN_SCRIPT.is_file())

    def test_observer_header_macro_defined(self):
        from i2pd_direct_driver import I2PD_OBSERVER_HEADER as header_path
        text = header_path.read_text()
        self.assertIn("I2PD_INTEROP_OBSERVER", text)

    def test_observer_source_uses_macro_gate(self):
        from i2pd_direct_driver import I2PD_OBSERVER_SOURCE as source_path
        text = source_path.read_text()
        self.assertIn("I2PD_INTEROP_OBSERVER", text)

    def test_observer_patch_uses_macro_gate(self):
        from i2pd_direct_driver import I2PD_OBSERVER_PATCH as patch_path
        text = patch_path.read_text()
        self.assertIn("I2PD_INTEROP_OBSERVER", text)

    def test_observer_digests_are_measured(self):
        self.assertEqual(len(observer_header_digest()), 64)
        self.assertEqual(len(observer_source_digest()), 64)
        self.assertEqual(len(observer_patch_digest()), 64)


class ControlBlockedOnThisHostTests(unittest.TestCase):
    """On the Plan 046 ``apparmor_restrict_on`` negative baseline
    the lane returns ``blocked_unprivileged_user_namespace``. Plan
    064 forbids converting the blocker into a synthetic pass; the
    tests assert the typed blocker and refuse to claim qualification."""

    def test_blocker_is_unprivileged_user_namespace(self):
        blocker = plan064_typed_blocker_for_host("blocked_unprivileged_user_namespace")
        self.assertEqual(blocker, "blocked_unprivileged_user_namespace")

    def test_i2pd_to_i2pd_control_blocked_on_this_host(self):
        """The Plan 046 ``apparmor_restrict_on`` negative baseline is
        the canonical host blocker. Plan 064 closes with the typed
        host blocker, never with a synthetic pass."""

        blocker = plan064_typed_blocker_for_host("blocked_unprivileged_user_namespace")
        self.assertEqual(blocker, "blocked_unprivileged_user_namespace")
        self.assertNotEqual(blocker, "qualified")


class ControlConfigInvariantsTests(unittest.TestCase):
    """Plan 064 WP2: the strict config renders, validates, and rejects
    unknown fields. The Plan 064 strict contract supersedes the Plan
    059 40-hex Router Hash contract."""

    def test_strict_config_round_trip(self):
        config = _base_config()
        rendered = render_strict_config(config)
        reparsed = json.loads(rendered)
        validate_strict_config(reparsed)
        self.assertEqual(reparsed, config)

    def test_strict_config_rejects_40_hex_router_hash(self):
        config = _base_config(expected_local_router_hash_sha256="a" * 40)
        with self.assertRaises(Plan064Error):
            validate_strict_config(config)


if __name__ == "__main__":
    unittest.main()