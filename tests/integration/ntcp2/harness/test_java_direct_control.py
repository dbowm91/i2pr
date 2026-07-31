"""Plan 063 Java-to-Java sealed control tests.

The Plan 063 Java direct driver supports a sealed two-process
control topology: one ``listen`` instance plus one ``dial`` instance
that exchange a real RouterInfo and a real correlated DeliveryStatus
inside a sealed namespace.

This module is the bounded local control seam. On the Plan 046
``apparmor_restrict_on`` negative baseline the lane returns
``blocked_unprivileged_user_namespace`` and the tests assert the
typed host blocker rather than converting it into a synthetic pass.
The Java-to-Java control evidence is not i2pr evidence; it is driver
qualification evidence only.
"""

from __future__ import annotations

import json
import os
import socket
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]

sys.path.insert(0, str(REPO_ROOT / "tests/integration/ntcp2/harness"))

from java_direct_driver import (  # noqa: E402
    JAVA_HELPER_BUILD_SCRIPT,
    JAVA_HELPER_RUN_SCRIPT,
    JAVA_HELPER_SOURCE_LOCK,
    REFERENCE_REVISION,
    Plan063Error,
    build_helper,
    load_source_lock,
    plan063_typed_blocker_for_host,
    render_strict_config,
)
from reference_event import (  # noqa: E402
    EVENT_SCHEMA,
    EVENT_SCHEMA_VERSION,
    EventKind,
    build_event,
    validate_event,
)


def _java_cache_path() -> Path:
    cache_root = REPO_ROOT / "target" / "interop" / "cache" / "java_i2p"
    if not cache_root.is_dir():
        return Path("")
    for entry in cache_root.iterdir():
        if entry.is_dir() and (entry / "lib" / "router.jar").is_file():
            return entry
    return Path("")


def _base_config(**overrides):
    config = {
        "schema": "i2pr-java-direct-driver-config-v1",
        "schema_version": 1,
        "run_id": "plan063-control-fixture",
        "scenario_id": "plan063-control-scenario",
        "direction": "java-to-i2pr-ipv4",
        "mode": "listen",
        "data_dir": "/tmp/opencode/plan063-control/data",
        "output_dir": "/tmp/opencode/plan063-control/out",
        "local_address": "192.0.2.1",
        "local_port": 45678,
        "network_id": 99,
        "peer_router_info_path": "/tmp/opencode/plan063-control/peer-router.info",
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
        "classpath_manifest_sha256": "7" * 64,
        "run_identity_sha256": "8" * 64,
    }
    config.update(overrides)
    return config


class ControlTopologyContractTests(unittest.TestCase):
    """The Java-to-Java sealed control topology is two processes
    exchanging a real RouterInfo and a real correlated DeliveryStatus.
    The topology contract is recorded in the source-verification
    record and is the Plan 063 WP9 qualification gate."""

    def test_topology_is_two_process_direct(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/source-verification.md").read_text()
        self.assertIn("two-process", text)
        self.assertIn("rootless sealed", text)

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
        self.assertEqual(lock.reference_version, "2.12.0")


class ControlBlockedOnThisHostTests(unittest.TestCase):
    """On the Plan 046 ``apparmor_restrict_on`` negative baseline
    the lane returns ``blocked_unprivileged_user_namespace``. Plan
    063 forbids converting the blocker into a synthetic pass; the
    tests assert the typed blocker and refuse to claim qualification."""

    def test_blocker_is_unprivileged_user_namespace(self):
        blocker = plan063_typed_blocker_for_host("blocked_unprivileged_user_namespace")
        self.assertEqual(blocker, "blocked_unprivileged_user_namespace")

    def test_qualification_receipt_carries_blocker(self):
        receipt_path = REPO_ROOT / "tests/integration/ntcp2/qualification/java-direct-driver.json"
        if not receipt_path.is_file():
            self.skipTest("qualification receipt not yet committed")
        receipt = json.loads(receipt_path.read_text())
        self.assertFalse(receipt.get("qualified"))
        self.assertEqual(
            receipt.get("qualification_blocker"),
            "blocked_unprivileged_user_namespace",
        )
        self.assertEqual(receipt["listen_passes"], 0)
        self.assertEqual(receipt["dial_passes"], 0)


class ControlDriverInspectionTests(unittest.TestCase):
    """The driver must pass inspect mode without ever opening a
    socket or starting a router process. The driver source itself
    performs this guarantee."""

    @classmethod
    def setUpClass(cls):
        cls.cache = _java_cache_path()
        if not cls.cache:
            return

    def test_inspect_mode_emits_process_started_and_terminal_clean(self):
        if not self.cache:
            self.skipTest("pinned Java cache not present in this checkout")
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "build"
            driver_jar = build_helper(
                repo_root=REPO_ROOT,
                java_cache=self.cache,
                output_dir=output_dir,
            )
            data_dir = Path(tmp) / "data"
            data_dir.mkdir(parents=True, exist_ok=True)
            config_path = Path(tmp) / "driver-config.json"
            config = _base_config(
                data_dir=str(data_dir),
                output_dir=str(Path(tmp) / "out"),
                mode="inspect",
            )
            config_path.write_text(render_strict_config(config), encoding="utf-8")
            completed = subprocess.run(
                [
                    "java",
                    "-Xmx512m",
                    "-Djava.awt.headless=true",
                    f"-Di2p.dir.base={self.cache}",
                    f"-Di2p.dir.config={self.cache}",
                    f"-Di2p.dir.router={data_dir}",
                    "-classpath",
                    f"{driver_jar}:{self.cache}/lib/*",
                    "i2pr.ntcp2.JavaNtcp2InteropDriver",
                    "inspect",
                    "--config",
                    str(config_path),
                ],
                stdout=subprocess.PIPE,
                stderr=subprocess.PIPE,
                timeout=30,
                check=False,
            )
            self.assertEqual(
                completed.returncode,
                0,
                msg=completed.stderr.decode("utf-8", errors="replace"),
            )
            events_path = Path(config["output_dir"]) / "events.ndjson"
            self.assertTrue(events_path.is_file())
            events = [json.loads(line) for line in events_path.read_text().splitlines() if line.strip()]
            self.assertEqual(len(events), 2)
            self.assertEqual(events[0]["event_kind"], EventKind.PROCESS_STARTED.value)
            self.assertEqual(events[1]["event_kind"], EventKind.TERMINAL_CLEAN.value)
            for event in events:
                self.assertEqual(event["schema"], EVENT_SCHEMA)
                self.assertEqual(event["schema_version"], EVENT_SCHEMA_VERSION)
                validate_event(event)


if __name__ == "__main__":
    unittest.main()
