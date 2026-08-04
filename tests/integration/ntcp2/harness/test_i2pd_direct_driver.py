"""Plan 064 i2pd direct NTCP2 driver test matrix.

These tests cover the Plan 064 work packages WP1-WP11:

- WP1: source-verification assertions and bounded read-only contract;
- WP2: strict config parser and inspect mode round-trip;
- WP3: pinned initialization gate (typed host blocker on Plan 046);
- WP4: dial RouterInfo import and DeliveryStatus submission contract;
- WP5: listener receive handler contract;
- WP6: passive observer compile-time gating and behavior neutrality;
- WP7: shutdown verification contract;
- WP8: Python harness adapter contract;
- WP9: i2pd-to-i2pd sealed control is a bounded local simulation
  (the harness records a typed host-environment blocker on this host
  rather than claiming an external run);
- WP10: qualification state and 10/10 freshness invariants are
  recorded in the qualification receipt;
- WP11: Plan 059 helper supersedure (D1-D8) is enforced at the static
  boundary check and recorded in the compatibility stub.

The tests do not link the helper against the pinned i2pd libraries
on the Plan 046 ``apparmor_restrict_on`` negative baseline: the lane
is blocked, and Plan 064 forbids converting that blocker into a
synthetic pass. The tests assert the contract surface and the typed
host blocker.
"""

from __future__ import annotations

import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]

sys.path.insert(0, str(REPO_ROOT / "tests/integration/ntcp2/harness"))

from i2pd_direct_driver import (  # noqa: E402
    ALLOWED_CONFIG_FIELDS,
    ALLOWED_DIRECTIONS,
    ALLOWED_MODES,
    CONFIG_SCHEMA,
    CONFIG_VERSION,
    I2PD_BUILD_SCHEMA,
    I2PD_BUILD_SCRIPT,
    I2PD_DRIVER_SOURCE,
    I2PD_OBSERVER_HEADER,
    I2PD_OBSERVER_PATCH,
    I2PD_OBSERVER_SOURCE,
    I2PD_RUN_SCRIPT,
    I2PD_SOURCE_LOCK,
    Plan064Error,
    REFERENCE_NAME,
    REFERENCE_REVISION,
    REFERENCE_VERSION,
    build_manifest_schema_digest,
    control_binary_digest,
    helper_source_digest,
    i2pd_direct_driver_invocation,
    instrumented_binary_digest,
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
from reference_trigger_v4 import (  # noqa: E402
    TriggerHelperKind,
    TriggerOutcome,
    build_trigger_record,
)


def _base_config(**overrides):
    config = {
        "schema": CONFIG_SCHEMA,
        "schema_version": CONFIG_VERSION,
        "run_id": "plan064-test-fixture",
        "scenario_id": "plan064-test-scenario",
        "direction": "i2pr-to-i2pd-ipv4",
        "mode": "inspect",
        "data_dir": "/tmp/opencode/plan064-fixture/data",
        "output_dir": "/tmp/opencode/plan064-fixture/out",
        "local_address": "192.0.2.1",
        "local_port": 45678,
        "network_id": 99,
        "peer_router_info_path": "/tmp/opencode/plan064-fixture/peer-router.info",
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
    }
    config.update(overrides)
    return config


class I2pdDriverArtifactsPresentTests(unittest.TestCase):
    def test_i2pd_driver_source_present(self):
        self.assertTrue(I2PD_DRIVER_SOURCE.is_file())

    def test_i2pd_driver_source_lock_present(self):
        self.assertTrue(I2PD_SOURCE_LOCK.is_file())

    def test_i2pd_observer_header_exposes_wait_primitives(self):
        header = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd/src/interop_observer.h").read_text()
        # Plan 083 wait primitives for the driver to block boundedly
        # on observer-recorded events.
        self.assertIn("WaitForAuthenticated", header)
        self.assertIn("WaitForReceivedI2NP", header)
        self.assertIn("WaitForSentI2NP", header)
        self.assertIn("ObserveAuthenticated", header)
        self.assertTrue(I2PD_OBSERVER_HEADER.is_file())

    def test_i2pd_observer_source_present(self):
        self.assertTrue(I2PD_OBSERVER_SOURCE.is_file())

    def test_i2pd_observer_patch_present(self):
        self.assertTrue(I2PD_OBSERVER_PATCH.is_file())

    def test_i2pd_build_manifest_schema_present(self):
        self.assertTrue(I2PD_BUILD_SCHEMA.is_file())

    def test_i2pd_build_script_present(self):
        self.assertTrue(I2PD_BUILD_SCRIPT.is_file())

    def test_i2pd_run_script_present(self):
        self.assertTrue(I2PD_RUN_SCRIPT.is_file())

    def test_i2pd_driver_source_lock_marker(self):
        text = I2PD_SOURCE_LOCK.read_text()
        self.assertIn('"$schema": "i2pr-i2pd-direct-driver-source-lock-v1"', text)
        self.assertIn('"helper_kind": "i2pd-direct-driver"', text)
        self.assertIn(REFERENCE_REVISION, text)
        self.assertIn('"schema_marker": "i2pr-reference-trigger-v4"', text)

    def test_i2pd_build_manifest_schema_marker(self):
        text = I2PD_BUILD_SCHEMA.read_text()
        self.assertIn("i2pr-i2pd-direct-driver-build-manifest-v1", text)

    def test_i2pd_observer_patch_marker(self):
        patch = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd/patches/i2pd-2.60.0-interop-observer.patch").read_text()
        # Plan 083 authentication observer seam is added inside the
        # Established() method body.
        self.assertIn("Plan 083 authenticated observer seam", patch)
        self.assertIn("ObserveAuthenticated", patch)
        text = I2PD_OBSERVER_PATCH.read_text()
        self.assertIn("I2PD_INTEROP_OBSERVER", text)
        self.assertIn("Plan 076", text)

    def test_i2pd_source_lock_records_linked_marker(self):
        text = I2PD_SOURCE_LOCK.read_text()
        self.assertIn('"linked_marker_macro": "I2PD_PLAN076_LINKED"', text)
        self.assertIn('"linked_marker_required": true', text)

    def test_i2pd_source_lock_records_pinned_library_digests(self):
        text = I2PD_SOURCE_LOCK.read_text()
        self.assertIn("50042c72c2080842531600395a183a9d91d189f7ee89d8097542fac9746c23cd", text)
        self.assertIn("c6368790c66777ad74b0b824a0cbaf24d34dd2d4ec54485ffa1e662746a1c986", text)
        self.assertIn("c3881a526f0bee7385f336289bf6c846889d90661c665065de898be52565b997", text)

    def test_i2pd_build_manifest_schema_requires_library_digests(self):
        text = I2PD_BUILD_SCHEMA.read_text()
        self.assertIn("i2pd_libraries_sha256", text)
        self.assertIn("linked_i2pd_sources", text)
        self.assertIn("observer_compile_time_gated", text)

    def test_i2pd_cmakelists_uses_linked_marker_and_lib_dir(self):
        cmake = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd/CMakeLists.txt").read_text()
        self.assertIn("I2PD_PLAN076_LINKED=1", cmake)
        self.assertIn("I2PD_LIB_DIR", cmake)
        self.assertIn("I2PD_PATCHED_TREE", cmake)
        self.assertIn("I2PD_PRISTINE_TREE", cmake)

    def test_i2pd_build_driver_script_builds_pinned_libraries(self):
        script = I2PD_BUILD_SCRIPT.read_text()
        self.assertIn("WITH_LIBRARY=ON", script)
        self.assertIn("WITH_BINARY=OFF", script)
        self.assertIn("libi2pd.a", script)
        self.assertIn("libi2pdclient.a", script)
        self.assertIn("libi2pdlang.a", script)
        self.assertIn("patch -p1 --fuzz=0", script)

    def test_i2pd_driver_source_uses_real_i2pd_api(self):
        source = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd/src/i2pd_ntcp2_interop_driver.cpp").read_text()
        self.assertIn("i2p::transport::Transports::SendMessage", source)
        self.assertIn("i2p::crypto::InitCrypto", source)
        self.assertIn("i2p::context.Init", source)
        self.assertIn("i2p::data::netdb.Start", source)
        self.assertIn("i2p::data::netdb.AddRouterInfo", source)
        self.assertIn("i2p::data::netdb.FindRouter", source)
        self.assertIn("i2p::transport::transports.Start(true", source)
        self.assertIn("i2p::fs::DetectDataDir", source)
        self.assertIn("i2p::CreateDeliveryStatusMsg", source)
        self.assertIn("I2PD_PLAN076_LINKED", source)
        # Plan 064 terminal rejection "pinned-libraries-not-linked" must remain
        # as the typed blocker in absence of the linked marker, but it must no
        # longer mark listen/dial as unreachable.
        self.assertIn("pinned-libraries-not-linked", source)
        # Plan 083 listen/dial modes wait boundedly for real wire
        # events; the wait primitives come from the observer header.
        self.assertIn("WaitForReceivedI2NP", source)
        self.assertIn("WaitForSentI2NP", source)
        self.assertIn("WaitForAuthenticated", source)
        self.assertIn("listening-handshake-timeout", source)
        self.assertIn("dialing-send-timeout", source)

    def test_i2pd_qualification_receipt_carries_linked_library_digests(self):
        receipt_path = (
            REPO_ROOT
            / "tests/integration/ntcp2/qualification/i2pd-direct-driver.json"
        )
        if not receipt_path.is_file():
            self.skipTest("qualification receipt not yet committed")
        receipt = json.loads(receipt_path.read_text())
        self.assertEqual(receipt["real_linked_i2pd_libraries"], True)
        self.assertEqual(receipt["observability_seam_native"], True)
        self.assertEqual(receipt["plan_076_local_build_complete"], True)
        self.assertEqual(
            receipt["reference_tree_sha256"],
            "03fc4834aaf3a4e33da6952a316b0fa5ff077b222f72beecba006a1122137044",
        )
        # Source digests from this repo (re-measured; tests may update if
        # the source changes, but the values must be 64 hex characters).
        for field in (
            "driver_source_sha256",
            "observer_header_sha256",
            "observer_source_sha256",
            "observer_patch_sha256",
            "source_lock_sha256",
            "build_manifest_schema_sha256",
            "libi2pd_sha256",
            "libi2pdclient_sha256",
            "libi2pdlang_sha256",
        ):
            value = receipt[field]
            self.assertEqual(len(value), 64, f"{field} not 64 hex: {value!r}")
            int(value, 16)


class I2pdSourceLockLoadTests(unittest.TestCase):
    def test_load_source_lock(self):
        lock = load_source_lock()
        self.assertEqual(lock.reference_name, REFERENCE_NAME)
        self.assertEqual(lock.reference_version, REFERENCE_VERSION)
        self.assertEqual(lock.reference_revision, REFERENCE_REVISION)
        self.assertEqual(lock.helper_kind, "i2pd-direct-driver")

    def test_load_source_lock_rejects_missing(self):
        with self.assertRaises(Plan064Error):
            load_source_lock(REPO_ROOT / "tests/integration/ntcp2/missing-source-lock.json")


class I2pdStrictConfigValidationTests(unittest.TestCase):
    def test_base_config_validates(self):
        validate_strict_config(_base_config())

    def test_unknown_field_rejected(self):
        config = _base_config(unexpected_field="attacker")
        with self.assertRaisesRegex(Plan064Error, "config-unknown-field"):
            validate_strict_config(config)

    def test_schema_mismatch_rejected(self):
        config = _base_config(schema="i2pr-evil-config-v9")
        with self.assertRaisesRegex(Plan064Error, "config-schema-mismatch"):
            validate_strict_config(config)

    def test_wrong_revision_rejected(self):
        config = _base_config(reference_revision="not-a-revision")
        with self.assertRaisesRegex(Plan064Error, "config-reference-revision-mismatch"):
            validate_strict_config(config)

    def test_wrong_network_id_rejected(self):
        config = _base_config(network_id=2)
        with self.assertRaisesRegex(Plan064Error, "config-network-id-not-99"):
            validate_strict_config(config)

    def test_local_address_outside_synthetic_rejected(self):
        config = _base_config(local_address="198.51.100.1")
        with self.assertRaisesRegex(Plan064Error, "config-local-address-not-synthetic"):
            validate_strict_config(config)

    def test_40_hex_local_router_hash_rejected(self):
        config = _base_config(expected_local_router_hash_sha256="a" * 40)
        with self.assertRaisesRegex(Plan064Error, "config-local-router-hash-not-64-hex"):
            validate_strict_config(config)

    def test_zero_router_hash_rejected(self):
        config = _base_config(expected_peer_router_hash_sha256="0" * 64)
        with self.assertRaisesRegex(Plan064Error, "config-peer-router-hash-zero-provenance"):
            validate_strict_config(config)

    def test_zero_observer_patch_sha_rejected(self):
        config = _base_config(observer_patch_sha256="0" * 64)
        with self.assertRaisesRegex(Plan064Error, "config-observer-patch-sha256-zero-provenance"):
            validate_strict_config(config)

    def test_zero_delivery_status_message_id_rejected(self):
        config = _base_config(delivery_status_message_id=0)
        with self.assertRaisesRegex(Plan064Error, "config-delivery-status-message-id-out-of-range"):
            validate_strict_config(config)

    def test_overflow_message_id_rejected(self):
        config = _base_config(delivery_status_message_id=0x100000000)
        with self.assertRaisesRegex(Plan064Error, "config-delivery-status-message-id-out-of-range"):
            validate_strict_config(config)

    def test_direction_not_allowlisted_rejected(self):
        config = _base_config(direction="i2pr-to-rogue-router-ipv4")
        with self.assertRaisesRegex(Plan064Error, "config-direction-not-allowlisted"):
            validate_strict_config(config)

    def test_mode_not_allowlisted_rejected(self):
        config = _base_config(mode="rogue-mode")
        with self.assertRaisesRegex(Plan064Error, "config-mode-not-allowlisted"):
            validate_strict_config(config)

    def test_local_port_out_of_range_rejected(self):
        config = _base_config(local_port=70_000)
        with self.assertRaisesRegex(Plan064Error, "config-local-port-out-of-range"):
            validate_strict_config(config)

    def test_handshake_timeout_zero_rejected(self):
        config = _base_config(handshake_timeout_ms=0)
        with self.assertRaisesRegex(Plan064Error, "config-handshake-timeout-out-of-range"):
            validate_strict_config(config)

    def test_shutdown_timeout_overrun_rejected(self):
        config = _base_config(shutdown_timeout_ms=120_000)
        with self.assertRaisesRegex(Plan064Error, "config-shutdown-timeout-out-of-range"):
            validate_strict_config(config)

    def test_peer_address_outside_synthetic_rejected(self):
        config = _base_config(expected_peer_address="203.0.113.10")
        with self.assertRaisesRegex(Plan064Error, "config-peer-address-not-synthetic"):
            validate_strict_config(config)

    def test_run_id_invalid_rejected(self):
        config = _base_config(run_id="INVALID-RUN-ID")
        with self.assertRaisesRegex(Plan064Error, "config-run-id-invalid"):
            validate_strict_config(config)

    def test_config_render_round_trip(self):
        config = _base_config()
        rendered = render_strict_config(config)
        reparsed = json.loads(rendered)
        validate_strict_config(reparsed)
        self.assertEqual(reparsed, config)


class I2pdContractSurfaceTests(unittest.TestCase):
    def test_helper_kind_is_i2pd_direct(self):
        helpers = {member.value for member in TriggerHelperKind}
        self.assertIn("i2pd-direct-helper", helpers)

    def test_helper_source_digest_is_hex64(self):
        digest = helper_source_digest()
        self.assertEqual(len(digest), 64)
        int(digest, 16)

    def test_observer_header_digest_is_hex64(self):
        digest = observer_header_digest()
        self.assertEqual(len(digest), 64)
        int(digest, 16)

    def test_observer_source_digest_is_hex64(self):
        digest = observer_source_digest()
        self.assertEqual(len(digest), 64)
        int(digest, 16)

    def test_observer_patch_digest_is_hex64(self):
        digest = observer_patch_digest()
        self.assertEqual(len(digest), 64)
        int(digest, 16)

    def test_build_manifest_schema_digest_is_hex64(self):
        digest = build_manifest_schema_digest()
        self.assertEqual(len(digest), 64)
        int(digest, 16)

    def test_qualification_requirements_locked(self):
        matrix = qualification_requirements_locked()
        for key, value in matrix.items():
            self.assertIs(value, True, f"Plan 064 requirement missing: {key}")

    def test_typed_blocker_unprivileged_user_namespace(self):
        blocker = plan064_typed_blocker_for_host("blocked_unprivileged_user_namespace")
        self.assertEqual(blocker, "blocked_unprivileged_user_namespace")

    def test_typed_blocker_execution_lane_unavailable(self):
        blocker = plan064_typed_blocker_for_host("blocked_execution_lane_unavailable")
        self.assertEqual(blocker, "blocked_execution_lane_unavailable")

    def test_typed_blocker_default_host(self):
        blocker = plan064_typed_blocker_for_host(None)
        self.assertEqual(blocker, "blocked_no_external_qualification_recorded")


class I2pdInvokeStubTests(unittest.TestCase):
    """The i2pd direct driver requires the compiled C++ binary. Where
    the binary is unavailable (which is the common test-runner
    environment), the Python adapter must surface the typed blocker
    rather than synthesise success."""

    def test_invocation_rejects_missing_driver_binary(self):
        with tempfile.TemporaryDirectory() as tmp:
            config = _base_config(
                data_dir=str(Path(tmp) / "data"),
                output_dir=str(Path(tmp) / "out"),
            )
            fake_binary = Path(tmp) / "no-such-binary"
            exit_code, record = i2pd_direct_driver_invocation(
                config=config,
                driver_binary=fake_binary,
                run_identity_sha256="8" * 64,
                helper_source_sha256=helper_source_digest(),
                helper_binary_sha256="5" * 64,
                build_manifest_sha256="6" * 64,
                helper_build_manifest_sha256="6" * 64,
                source_inspection_record_sha256="9" * 64,
                observer_patch_sha256=observer_patch_digest(),
                result_path=Path(tmp) / "trigger.json",
            )
            self.assertEqual(exit_code, 65)
            self.assertEqual(
                record["outcome"],
                TriggerOutcome.DIRECT_TRIGGER_NOT_SOURCE_LOCKED.value,
            )
            self.assertEqual(record["reason_code"], "driver-binary-missing")
            self.assertFalse(record["attempted"])

    def test_qualification_receipt_carries_blocker(self):
        receipt_path = (
            REPO_ROOT
            / "tests/integration/ntcp2/qualification/i2pd-direct-driver.json"
        )
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


class I2pdStructuredEventContractTests(unittest.TestCase):
    """Plan 064 WP6: structured events must validate against the Plan
    062 reference-event v1 schema. The i2pd helper emits the same
    field set as the Plan 062 Python event builder."""

    def test_process_started_event_validates(self):
        event = build_event(
            run_id="plan064-test-fixture",
            scenario_id="plan064-test-scenario",
            direction="i2pr-to-i2pd-ipv4",
            implementation="i2pd-direct-driver",
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
            run_id="plan064-test-fixture",
            scenario_id="plan064-test-scenario",
            direction="i2pr-to-i2pd-ipv4",
            implementation="i2pd-direct-driver",
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
            run_id="plan064-test-fixture",
            scenario_id="plan064-test-scenario",
            direction="i2pr-to-i2pd-ipv4",
            implementation="i2pd-direct-driver",
            implementation_revision=REFERENCE_REVISION,
            driver_binary_sha256="5" * 64,
            local_router_hash_sha256="1" * 64,
            peer_router_hash_sha256="2" * 64,
            monotonic_ms=3,
            event_kind=EventKind.TERMINAL_CLEAN,
            event_sequence=2,
        )
        validate_event(event)


class I2pdPlan059SupersedureTests(unittest.TestCase):
    """Plan 064 WP11: the Plan 059 helper carries the eight Plan 064
    defects (D1-D8) and is replaced by this driver. The legacy
    ``i2pd_direct_connect`` source file is preserved as a fail-closed
    compatibility stub that exits non-zero and never runs the helper
    logic. The static boundary check enforces that the legacy path
    carries the supersedure marker."""

    def test_legacy_helper_is_fail_closed_stub(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/i2pd_direct_connect.cpp").read_text()
        self.assertIn("supersedure marker", text.lower())
        self.assertIn("Plan 064", text)

    def test_legacy_source_lock_carries_supersedure_note(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/source-lock.json").read_text()
        self.assertIn("supersedure", text.lower())
        self.assertIn("Plan 064", text)
        self.assertIn("Plan 064 driver", text)

    def test_legacy_helper_kind_is_preserved(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/i2pd_direct_connect/source-lock.json").read_text()
        self.assertIn('"helper_kind": "i2pd-direct-helper"', text)
        self.assertIn('"schema_marker": "i2pr-reference-trigger-v3"', text)


if __name__ == "__main__":
    unittest.main()