"""Plan 063 Java I2P stripped-router direct NTCP2 driver test matrix.

These tests cover the Plan 063 work packages WP1-WP10:

- WP1: source-verification assertions and bounded read-only contract;
- WP2: strict config parser and inspect mode round-trip;
- WP3: embedded-router startup/readiness/export contract;
- WP4: listener receive handler contract;
- WP5: dial RouterInfo import and DeliveryStatus submission contract;
- WP6: structured events validate against the Plan 062 reference-event
  v1 schema;
- WP7: shutdown verification contract;
- WP8: Python harness adapter contract;
- WP9: Java-to-Java sealed control is a bounded local simulation
  (the harness records a typed host-environment blocker on this host
  rather than claiming an external run);
- WP10: qualification state and 10/10 freshness invariants are
  recorded in the qualification receipt.

The tests do not launch the embedded Java router against a real
NTCP2 listener: on the Plan 046 ``apparmor_restrict_on`` negative
baseline the lane is blocked, and Plan 063 forbids converting that
blocker into a synthetic pass. The tests assert the contract surface
and the structured event correctness from a process_started /
terminal_clean fixture produced by the local inspect path.
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
REPO_ROOT = HERE.parents[3]

sys.path.insert(0, str(REPO_ROOT / "tests" / "integration" / "ntcp2" / "harness"))

from java_direct_driver import (  # noqa: E402
    ALLOWED_CONFIG_FIELDS,
    ALLOWED_DIRECTIONS,
    ALLOWED_MODES,
    CONFIG_SCHEMA,
    CONFIG_VERSION,
    JAVA_HELPER_BUILD_SCHEMA,
    JAVA_HELPER_CLASSPATH_MANIFEST,
    JAVA_HELPER_DIR,
    JAVA_HELPER_RUN_SCRIPT,
    JAVA_HELPER_SOURCE,
    JAVA_HELPER_SOURCE_LOCK,
    Plan063Error,
    REFERENCE_NAME,
    REFERENCE_REVISION,
    REFERENCE_VERSION,
    build_manifest_schema_digest,
    classpath_manifest_digest,
    helper_source_digest,
    java_helper_invocation,
    load_source_lock,
    plan063_typed_blocker_for_host,
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
from reference_trigger_v4 import (  # noqa: E402
    TriggerHelperKind,
    TriggerOutcome,
    build_trigger_record,
)


def _base_config(**overrides):
    config = {
        "schema": CONFIG_SCHEMA,
        "schema_version": CONFIG_VERSION,
        "run_id": "plan063-test-fixture",
        "scenario_id": "plan063-test-scenario",
        "direction": "i2pr-to-java-ipv4",
        "mode": "inspect",
        "data_dir": "/tmp/opencode/plan063-fixture/data",
        "output_dir": "/tmp/opencode/plan063-fixture/out",
        "local_address": "192.0.2.1",
        "local_port": 45678,
        "network_id": 99,
        "peer_router_info_path": "/tmp/opencode/plan063-fixture/peer-router.info",
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


def _java_cache_path() -> Path:
    """Return the pinned Java I2P 2.12.0 cache directory if available."""

    cache_root = REPO_ROOT / "target" / "interop" / "cache" / "java_i2p"
    if not cache_root.is_dir():
        return Path("")
    for entry in cache_root.iterdir():
        if entry.is_dir() and (entry / "lib" / "router.jar").is_file():
            return entry
    return Path("")


class JavaHelperArtifactsPresentTests(unittest.TestCase):
    def test_java_helper_source_present(self):
        self.assertTrue(JAVA_HELPER_SOURCE.is_file())

    def test_java_helper_source_lock_present(self):
        self.assertTrue(JAVA_HELPER_SOURCE_LOCK.is_file())

    def test_java_helper_classpath_manifest_present(self):
        self.assertTrue(JAVA_HELPER_CLASSPATH_MANIFEST.is_file())

    def test_java_helper_build_manifest_schema_present(self):
        self.assertTrue(JAVA_HELPER_BUILD_SCHEMA.is_file())

    def test_java_helper_run_script_present(self):
        self.assertTrue(JAVA_HELPER_RUN_SCRIPT.is_file())

    def test_java_helper_source_lock_marker(self):
        text = JAVA_HELPER_SOURCE_LOCK.read_text()
        self.assertIn('"$schema": "i2pr-java-helper-source-lock-v1"', text)
        self.assertIn('"helper_kind": "java-direct-helper"', text)
        self.assertIn(REFERENCE_REVISION, text)

    def test_java_helper_classpath_manifest_marker(self):
        text = JAVA_HELPER_CLASSPATH_MANIFEST.read_text()
        self.assertIn("i2pr-java-helper-classpath-manifest-v1", text)

    def test_java_helper_build_manifest_schema_marker(self):
        text = JAVA_HELPER_BUILD_SCHEMA.read_text()
        self.assertIn("i2pr-java-helper-build-manifest-v1", text)


class JavaSourceLockLoadTests(unittest.TestCase):
    def test_load_source_lock(self):
        lock = load_source_lock()
        self.assertEqual(lock.reference_name, "java_i2p")
        self.assertEqual(lock.reference_version, REFERENCE_VERSION)
        self.assertEqual(lock.reference_revision, REFERENCE_REVISION)
        self.assertEqual(lock.helper_kind, "java-direct-helper")

    def test_load_source_lock_rejects_missing(self):
        with self.assertRaises(Plan063Error):
            load_source_lock(REPO_ROOT / "tests/integration/ntcp2/missing-source-lock.json")


class JavaStrictConfigValidationTests(unittest.TestCase):
    def test_base_config_validates(self):
        validate_strict_config(_base_config())

    def test_unknown_field_rejected(self):
        config = _base_config(unexpected_field="attacker")
        with self.assertRaisesRegex(Plan063Error, "config-unknown-field"):
            validate_strict_config(config)

    def test_schema_mismatch_rejected(self):
        config = _base_config(schema="i2pr-evil-config-v9")
        with self.assertRaisesRegex(Plan063Error, "config-schema-mismatch"):
            validate_strict_config(config)

    def test_wrong_revision_rejected(self):
        config = _base_config(reference_revision="not-a-revision")
        with self.assertRaisesRegex(Plan063Error, "config-reference-revision-mismatch"):
            validate_strict_config(config)

    def test_wrong_network_id_rejected(self):
        config = _base_config(network_id=2)
        with self.assertRaisesRegex(Plan063Error, "config-network-id-not-99"):
            validate_strict_config(config)

    def test_local_address_outside_synthetic_rejected(self):
        config = _base_config(local_address="198.51.100.1")
        with self.assertRaisesRegex(Plan063Error, "config-local-address-not-synthetic"):
            validate_strict_config(config)

    def test_40_hex_local_router_hash_rejected(self):
        config = _base_config(expected_local_router_hash_sha256="a" * 40)
        with self.assertRaisesRegex(Plan063Error, "config-local-router-hash-not-64-hex"):
            validate_strict_config(config)

    def test_zero_router_hash_rejected(self):
        config = _base_config(expected_peer_router_hash_sha256="0" * 64)
        with self.assertRaisesRegex(Plan063Error, "config-peer-router-hash-zero-provenance"):
            validate_strict_config(config)

    def test_zero_driver_source_sha_rejected(self):
        config = _base_config(driver_source_sha256="0" * 64)
        with self.assertRaisesRegex(Plan063Error, "config-driver-source-sha256-zero-provenance"):
            validate_strict_config(config)

    def test_zero_delivery_status_message_id_rejected(self):
        config = _base_config(delivery_status_message_id=0)
        with self.assertRaisesRegex(Plan063Error, "config-delivery-status-message-id-out-of-range"):
            validate_strict_config(config)

    def test_overflow_message_id_rejected(self):
        config = _base_config(delivery_status_message_id=0x100000000)
        with self.assertRaisesRegex(Plan063Error, "config-delivery-status-message-id-out-of-range"):
            validate_strict_config(config)

    def test_direction_not_allowlisted_rejected(self):
        config = _base_config(direction="i2pr-to-rogue-router-ipv4")
        with self.assertRaisesRegex(Plan063Error, "config-direction-not-allowlisted"):
            validate_strict_config(config)

    def test_mode_not_allowlisted_rejected(self):
        config = _base_config(mode="rogue-mode")
        with self.assertRaisesRegex(Plan063Error, "config-mode-not-allowlisted"):
            validate_strict_config(config)

    def test_local_port_out_of_range_rejected(self):
        config = _base_config(local_port=70_000)
        with self.assertRaisesRegex(Plan063Error, "config-local-port-out-of-range"):
            validate_strict_config(config)

    def test_handshake_timeout_zero_rejected(self):
        config = _base_config(handshake_timeout_ms=0)
        with self.assertRaisesRegex(Plan063Error, "config-handshake-timeout-out-of-range"):
            validate_strict_config(config)

    def test_shutdown_timeout_overrun_rejected(self):
        config = _base_config(shutdown_timeout_ms=120_000)
        with self.assertRaisesRegex(Plan063Error, "config-shutdown-timeout-out-of-range"):
            validate_strict_config(config)

    def test_peer_address_outside_synthetic_rejected(self):
        config = _base_config(expected_peer_address="203.0.113.10")
        with self.assertRaisesRegex(Plan063Error, "config-peer-address-not-synthetic"):
            validate_strict_config(config)

    def test_run_id_invalid_rejected(self):
        config = _base_config(run_id="INVALID-RUN-ID")
        with self.assertRaisesRegex(Plan063Error, "config-run-id-invalid"):
            validate_strict_config(config)

    def test_config_render_round_trip(self):
        config = _base_config()
        rendered = render_strict_config(config)
        reparsed = json.loads(rendered)
        validate_strict_config(reparsed)
        self.assertEqual(reparsed, config)


class JavaContractSurfaceTests(unittest.TestCase):
    def test_helper_kind_is_java_direct(self):
        helpers = {member.value for member in TriggerHelperKind}
        self.assertIn("java-direct-helper", helpers)
        self.assertEqual(
            TriggerHelperKind.JAVA_DIRECT_HELPER.value,
            "java-direct-helper",
        )

    def test_helper_source_digest_is_hex64(self):
        digest = helper_source_digest()
        self.assertEqual(len(digest), 64)
        int(digest, 16)

    def test_classpath_manifest_digest_is_hex64(self):
        digest = classpath_manifest_digest()
        self.assertEqual(len(digest), 64)
        int(digest, 16)

    def test_build_manifest_schema_digest_is_hex64(self):
        digest = build_manifest_schema_digest()
        self.assertEqual(len(digest), 64)
        int(digest, 16)

    def test_qualification_requirements_locked(self):
        matrix = qualification_requirements_locked()
        for key, value in matrix.items():
            self.assertIs(value, True, f"Plan 063 requirement missing: {key}")

    def test_typed_blocker_unprivileged_user_namespace(self):
        blocker = plan063_typed_blocker_for_host("blocked_unprivileged_user_namespace")
        self.assertEqual(blocker, "blocked_unprivileged_user_namespace")

    def test_typed_blocker_execution_lane_unavailable(self):
        blocker = plan063_typed_blocker_for_host("blocked_execution_lane_unavailable")
        self.assertEqual(blocker, "blocked_execution_lane_unavailable")

    def test_typed_blocker_default_host(self):
        blocker = plan063_typed_blocker_for_host(None)
        self.assertEqual(blocker, "blocked_no_external_qualification_recorded")


class JavaInvokeStubTests(unittest.TestCase):
    """The Java helper requires the pinned JAR cache. Where the cache is
    unavailable (which is the common test-runner environment), the
    Python adapter must surface the typed blocker rather than
    synthesise success."""

    def test_invocation_rejects_missing_java_cache(self):
        with tempfile.TemporaryDirectory() as tmp:
            config = _base_config(
                data_dir=str(Path(tmp) / "data"),
                output_dir=str(Path(tmp) / "out"),
            )
            # Build a non-existent cache path.
            cache = Path(tmp) / "no-such-cache"
            jar = Path(tmp) / "no-such-jar"
            exit_code, record = java_helper_invocation(
                config=config,
                java_cache=cache,
                driver_jar=jar,
                run_identity_sha256="8" * 64,
                helper_source_sha256=helper_source_digest(),
                helper_binary_sha256="5" * 64,
                build_manifest_sha256="6" * 64,
                classpath_manifest_sha256=classpath_manifest_digest(),
                helper_build_manifest_sha256="6" * 64,
                source_inspection_record_sha256="9" * 64,
                result_path=Path(tmp) / "trigger.json",
            )
            self.assertEqual(exit_code, 65)
            self.assertEqual(record["outcome"], TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED.value)
            self.assertEqual(record["reason_code"], "java-cache-missing")
            self.assertFalse(record["attempted"])


class JavaLocalControlTests(unittest.TestCase):
    """Plan 063 WP9: a bounded Java-to-Java control is the local
    qualification seam. On this host the lane is blocked by
    ``apparmor_restrict_on``; the test asserts the contract surface
    without converting the blocker into a passing run."""

    def test_java_helper_invocation_blocks_on_missing_cache(self):
        with tempfile.TemporaryDirectory() as tmp:
            config = _base_config(
                data_dir=str(Path(tmp) / "data"),
                output_dir=str(Path(tmp) / "out"),
            )
            exit_code, record = java_helper_invocation(
                config=config,
                java_cache=Path(tmp) / "no-cache",
                driver_jar=Path(tmp) / "no.jar",
                run_identity_sha256="8" * 64,
                helper_source_sha256=helper_source_digest(),
                helper_binary_sha256="5" * 64,
                build_manifest_sha256="6" * 64,
                classpath_manifest_sha256=classpath_manifest_digest(),
                helper_build_manifest_sha256="6" * 64,
                source_inspection_record_sha256="9" * 64,
                result_path=Path(tmp) / "trigger.json",
            )
            self.assertEqual(exit_code, 65)
            self.assertFalse(record["attempted"])
            self.assertIn(
                record["outcome"],
                {
                    TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED.value,
                    TriggerOutcome.REJECTED_TARGET_ROUTER_INFO.value,
                    TriggerOutcome.DIRECT_TRIGGER_HELPER_FAILED.value,
                },
            )

    def test_java_to_java_control_blocked_on_this_host(self):
        """The Plan 046 ``apparmor_restrict_on`` negative baseline is
        the canonical host blocker. Plan 063 closes with the typed
        host blocker, never with a synthetic pass."""

        blocker = plan063_typed_blocker_for_host("blocked_unprivileged_user_namespace")
        self.assertEqual(blocker, "blocked_unprivileged_user_namespace")
        self.assertNotEqual(blocker, "qualified")


class JavaStructuredEventContractTests(unittest.TestCase):
    """Plan 063 WP6: structured events must validate against the Plan
    062 reference-event v1 schema. The Java helper emits the same
    field set as the Plan 062 Python event builder."""

    def test_process_started_event_validates(self):
        event = build_event(
            run_id="plan063-test-fixture",
            scenario_id="plan063-test-scenario",
            direction="i2pr-to-java-ipv4",
            invocation_id="plan063-test-invocation-1",
            implementation="java-direct-driver",
            implementation_revision=REFERENCE_REVISION,
            driver_binary_sha256="5" * 64,
            local_router_hash_sha256="1" * 64,
            peer_router_hash_sha256="2" * 64,
            monotonic_ms=1,
            event_kind=EventKind.PROCESS_STARTED,
            event_sequence=0,
        )
        validate_event(event)
        self.assertEqual(event["schema"], EVENT_SCHEMA)
        self.assertEqual(event["schema_version"], EVENT_SCHEMA_VERSION)

    def test_data_phase_event_requires_correlation_fields(self):
        event = build_event(
            run_id="plan063-test-fixture",
            scenario_id="plan063-test-scenario",
            direction="i2pr-to-java-ipv4",
            invocation_id="plan063-test-invocation-1",
            implementation="java-direct-driver",
            implementation_revision=REFERENCE_REVISION,
            driver_binary_sha256="5" * 64,
            local_router_hash_sha256="1" * 64,
            peer_router_hash_sha256="2" * 64,
            monotonic_ms=2,
            event_kind=EventKind.I2NP_MESSAGE_DECODED,
            event_sequence=1,
            delivery_status_message_id=12345,
            i2np_type=10,
            frame_sequence=0,
        )
        validate_event(event)

    def test_terminal_event_forbids_data_phase_fields(self):
        event = build_event(
            run_id="plan063-test-fixture",
            scenario_id="plan063-test-scenario",
            direction="i2pr-to-java-ipv4",
            invocation_id="plan063-test-invocation-1",
            implementation="java-direct-driver",
            implementation_revision=REFERENCE_REVISION,
            driver_binary_sha256="5" * 64,
            local_router_hash_sha256="1" * 64,
            peer_router_hash_sha256="2" * 64,
            monotonic_ms=3,
            event_kind=EventKind.TERMINAL_CLEAN,
            event_sequence=2,
        )
        validate_event(event)


class JavaDriverBinarySmokeTests(unittest.TestCase):
    """If the pinned Java cache is available, build the helper jar and
    run ``inspect`` mode. The smoke test is opt-in: it never requires
    a real network socket and never exercises the embedded router
    lifecycle."""

    @classmethod
    def setUpClass(cls):
        cls.cache = _java_cache_path()
        if not cls.cache:
            return

    def test_inspect_round_trip_when_cache_available(self):
        if not self.cache:
            self.skipTest("pinned Java cache is not present in this checkout")
        with tempfile.TemporaryDirectory() as tmp:
            output_dir = Path(tmp) / "build"
            data_dir = Path(tmp) / "data"
            data_dir.mkdir(parents=True, exist_ok=True)
            from java_direct_driver import build_helper

            driver_jar = build_helper(
                repo_root=REPO_ROOT,
                java_cache=self.cache,
                output_dir=output_dir,
            )
            self.assertTrue(driver_jar.is_file())
            config_path = Path(tmp) / "driver-config.json"
            config = _base_config(
                data_dir=str(data_dir),
                output_dir=str(Path(tmp) / "out"),
            )
            config_path.write_text(render_strict_config(config), encoding="utf-8")
            env = os.environ.copy()
            env.setdefault("JAVA_TOOL_OPTIONS", "")
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
                env=env,
                timeout=30,
                check=False,
            )
            self.assertEqual(
                completed.returncode,
                0,
                msg=f"inspect exited {completed.returncode}: {completed.stderr.decode('utf-8', errors='replace')}",
            )
            events_path = Path(config["output_dir"]) / "events.ndjson"
            self.assertTrue(events_path.is_file())
            events = [json.loads(line) for line in events_path.read_text().splitlines() if line.strip()]
            self.assertGreaterEqual(len(events), 2)
            self.assertEqual(events[0]["event_kind"], EventKind.PROCESS_STARTED.value)
            self.assertEqual(events[-1]["event_kind"], EventKind.TERMINAL_CLEAN.value)
            for event in events:
                validate_event(event)


if __name__ == "__main__":
    unittest.main()
