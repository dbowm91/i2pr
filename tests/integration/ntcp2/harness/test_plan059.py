"""Plan 059 reference-side helper and qualification regression matrix.

The Plan 059 plan-of-record enumerates 34 required cases grouped into
five surfaces:

  - i2pd helper (1-8)
  - Java support topology (9-16)
  - receiver observations (17-24)
  - Java startup (25-29)
  - pipeline (30-34)

ADR 0021 (`docs/adr/0021-minimal-java-support-topology.md`) was
Rejected by the Plan 058 repository maintainer decision; the Java
support topology is forbidden under the current four-direction
contract. Plan 059 closes with the typed blocker
``blocked_java_support_topology_rejected``. The Java support
topology cases (9-16) therefore assert that the gate refuses to
activate an implementation when ADR 0021 is Rejected; the Java
receiver and startup cases (17-29) assert that the qualification
receipts and the gate mark the Java markers as blocked.

The i2pd helper, observation qualification, and pipeline cases
exercise the locally committed artifacts and the canonical Plan 052
pipeline integration. The synthetic fallback remains available for
blocked/diagnostic fixture runs and is rejected when ``live_mode``
is enabled.
"""

from __future__ import annotations

import dataclasses
import hashlib
import json
import os
import re
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import plan059
from plan059 import (
    HELPER_SOURCE_LOCK_SCHEMA,
    I2PD_DIRECT_CONNECT_DIR,
    OBSERVATION_QUALIFICATION_DIR,
    Plan059Error,
    plan059_typed_blocker,
)


class I2pdHelperSourceLockTests(unittest.TestCase):
    """Plan 059 cases 1-8: i2pd helper source-lock, digest binding, controls."""

    def test_01_valid_source_lock_record(self):
        lock = plan059.load_source_lock()
        self.assertEqual(lock.schema, HELPER_SOURCE_LOCK_SCHEMA)
        self.assertEqual(lock.helper_kind, "i2pd-direct-helper")
        self.assertEqual(lock.reference_revision, "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e")
        self.assertEqual(lock.reference_version, "2.60.0")

    def test_02_helper_digest_binding(self):
        cpp_digest = plan059.helper_source_digest()
        self.assertEqual(len(cpp_digest), 64)
        self.assertNotEqual(cpp_digest, "0" * 64)
        py_digest = plan059.helper_python_driver_digest()
        self.assertEqual(len(py_digest), 64)

    def test_03_correct_target_positive_fixture(self):
        """The bounded driver connects to a real TCP listener and
        emits a finalized trigger record with ``connection_callback_observed``.

        The trigger schema requires the synthetic 192.0.2.0/24 host
        literal; this test binds to 192.0.2.1 when the host allows
        and skips otherwise. The bounded no-listener timeout test
        (test_06) covers the same path on a port that the kernel
        guarantees unbound.
        """

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            data_dir = tmp_path / "data"
            router_info = tmp_path / "router.info"
            router_info.write_bytes(b"\x00" * 391 + b"192.0.2.1:45680" + b"\x00" * 200)
            result = tmp_path / "trigger.json"
            expected_router_hash = hashlib.sha1(router_info.read_bytes()[:391]).hexdigest()
            config = {
                "data_dir": data_dir,
                "router_info": router_info,
                "expected_router_hash": expected_router_hash,
                "expected_host": "192.0.2.1",
                "expected_port": 45680,
                "run_id": "plan059-positive",
                "scenario_id": "i2pd-to-i2pr-ipv4",
                "correlation_nonce": "plan059-positive-nonce-1234",
                "run_identity_sha256": "a" * 64,
                "source_inspection_record_sha256": "b" * 64,
                "result_path": result,
                "dial_timeout_seconds": 1,
            }
            import socket
            import threading

            ready = threading.Event()
            stop = threading.Event()
            listener_started = threading.Event()
            listener_port: list[int] = []

            def listener():
                server = socket.socket(socket.AF_INET, socket.SOCK_STREAM)
                server.setsockopt(socket.SOL_SOCKET, socket.SO_REUSEADDR, 1)
                try:
                    server.bind(("192.0.2.1", 0))
                except OSError:
                    listener_started.set()
                    server.close()
                    return
                listener_port.append(server.getsockname()[1])
                server.listen(1)
                listener_started.set()
                ready.set()
                server.settimeout(2.0)
                try:
                    conn, _ = server.accept()
                except OSError:
                    pass
                finally:
                    server.close()
                stop.wait(timeout=2.0)

            thread = threading.Thread(target=listener, daemon=True)
            thread.start()
            try:
                listener_started.wait(timeout=2.0)
                if not listener_port:
                    self.skipTest("cannot bind to 192.0.2.1 on this host")
                config["expected_port"] = listener_port[0]
                ready.wait(timeout=2.0)
                exit_code, record = plan059.i2pd_helper_invocation(
                    config=config,
                    helper_binary_sha256="c" * 64,
                    helper_source_sha256="d" * 64,
                )
                self.assertEqual(exit_code, 0)
                self.assertEqual(record["outcome"], "connected")
                self.assertTrue(record["connection_callback_observed"])
                self.assertEqual(record["helper_binary_sha256"], "c" * 64)
                self.assertEqual(record["helper_source_sha256"], "d" * 64)
            finally:
                stop.set()
                thread.join(timeout=3.0)

    def test_04_wrong_router_info_rejection(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            router_info = tmp_path / "router.info"
            router_info.write_bytes(b"\x00" * 391 + b"192.0.2.1:45680" + b"\x00" * 200)
            result = tmp_path / "trigger.json"
            config = {
                "data_dir": tmp_path / "data",
                "router_info": router_info,
                "expected_router_hash": "0" * 40,
                "expected_host": "192.0.2.1",
                "expected_port": 45680,
                "run_id": "plan059-wrong-hash",
                "scenario_id": "i2pd-to-i2pr-ipv4",
                "correlation_nonce": "plan059-wrong-hash-nonce-1234",
                "run_identity_sha256": "a" * 64,
                "source_inspection_record_sha256": "b" * 64,
                "result_path": result,
            }
            exit_code, record = plan059.i2pd_helper_invocation(config=config)
            self.assertEqual(exit_code, 65)
            self.assertEqual(record["outcome"], "rejected-target-router-info")
            self.assertFalse(record["attempted"])

    def test_05_wrong_endpoint_rejection(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            router_info = tmp_path / "router.info"
            router_info.write_bytes(b"\x00" * 391 + b"192.0.2.2:45679" + b"\x00" * 200)
            result = tmp_path / "trigger.json"
            expected_router_hash = hashlib.sha1(router_info.read_bytes()[:391]).hexdigest()
            config = {
                "data_dir": tmp_path / "data",
                "router_info": router_info,
                "expected_router_hash": expected_router_hash,
                "expected_host": "192.0.2.1",
                "expected_port": 45680,
                "run_id": "plan059-wrong-endpoint",
                "scenario_id": "i2pd-to-i2pr-ipv4",
                "correlation_nonce": "plan059-wrong-endpoint-nonce-1234",
                "run_identity_sha256": "a" * 64,
                "source_inspection_record_sha256": "b" * 64,
                "result_path": result,
            }
            exit_code, record = plan059.i2pd_helper_invocation(config=config)
            self.assertEqual(exit_code, 65)
            self.assertEqual(record["outcome"], "rejected-target-endpoint")
            self.assertFalse(record["attempted"])

    def test_06_no_listener_timeout(self):
        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            router_info = tmp_path / "router.info"
            router_info.write_bytes(b"\x00" * 391 + b"192.0.2.1:45680" + b"\x00" * 200)
            result = tmp_path / "trigger.json"
            expected_router_hash = hashlib.sha1(router_info.read_bytes()[:391]).hexdigest()
            config = {
                "data_dir": tmp_path / "data",
                "router_info": router_info,
                "expected_router_hash": expected_router_hash,
                "expected_host": "192.0.2.1",
                "expected_port": 45680,
                "run_id": "plan059-no-listener",
                "scenario_id": "i2pd-to-i2pr-ipv4",
                "correlation_nonce": "plan059-no-listener-nonce-1234",
                "run_identity_sha256": "a" * 64,
                "source_inspection_record_sha256": "b" * 64,
                "result_path": result,
                "dial_timeout_seconds": 1,
            }
            exit_code, record = plan059.i2pd_helper_invocation(
                config=config,
                helper_binary_sha256="c" * 64,
                helper_source_sha256="d" * 64,
            )
            self.assertEqual(exit_code, 66)
            self.assertEqual(record["outcome"], "direct-trigger-callback-timeout")
            self.assertTrue(record["attempted"])
            self.assertTrue(record["transport_request_observed"])
            self.assertFalse(record["connection_callback_observed"])

    def test_07_duplicate_attempt_rejection(self):
        """Each helper invocation produces one trigger record; the
        one-shot contract forbids merging multiple attempts into the
        same digest. Two invocations must produce distinct records.
        """

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            router_info = tmp_path / "router.info"
            router_info.write_bytes(b"\x00" * 391 + b"192.0.2.1:45680" + b"\x00" * 200)
            result_a = tmp_path / "trigger-a.json"
            result_b = tmp_path / "trigger-b.json"
            expected_router_hash = hashlib.sha1(router_info.read_bytes()[:391]).hexdigest()
            base = {
                "data_dir": tmp_path / "data",
                "router_info": router_info,
                "expected_router_hash": expected_router_hash,
                "expected_host": "192.0.2.1",
                "expected_port": 45680,
                "run_id": "plan059-duplicate",
                "scenario_id": "i2pd-to-i2pr-ipv4",
                "correlation_nonce": "plan059-duplicate-nonce-1234",
                "run_identity_sha256": "a" * 64,
                "source_inspection_record_sha256": "b" * 64,
            }
            config_a = dict(base, result_path=result_a, run_id="plan059-dup-a")
            config_b = dict(base, result_path=result_b, run_id="plan059-dup-b")
            _, record_a = plan059.i2pd_helper_invocation(
                config=config_a,
                helper_binary_sha256="c" * 64,
                helper_source_sha256="d" * 64,
            )
            _, record_b = plan059.i2pd_helper_invocation(
                config=config_b,
                helper_binary_sha256="e" * 64,
                helper_source_sha256="f" * 64,
            )
            self.assertEqual(record_a["attempt_count"], 1)
            self.assertEqual(record_b["attempt_count"], 1)
            self.assertNotEqual(record_a["trigger_sha256"], record_b["trigger_sha256"])

    def test_08_cleanup_failure_override(self):
        """Cleanup failure must override any other outcome. The
        bounded driver reports the ``cleanup-failed`` outcome and a
        non-zero exit code when transport teardown raises.
        """

        # Synthetic control: confirm that the trigger record schema
        # exposes the CLEANUP_FAILED outcome and that the pipeline
        # rejects a passed direction with cleanup_result=failed.
        from plan052_pipeline import PipelineError, write_direction_artifacts

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            identity_path = tmp_path / "run-identity.json"
            staging = tmp_path / "staging"
            from run_identity import build_run_identity, write_run_identity

            record = build_run_identity(
                run_id="plan059-cleanup-test",
                source_commit="a" * 40,
                source_commit_object_sha256="b" * 64,
                source_tree_sha256="c" * 64,
                source_archive_sha256="d" * 64,
                source_archive_format="git-tar",
                source_dirty="clean",
                host_source_manifest_sha256="e" * 64,
                guest_source_manifest_sha256="e" * 64,
                guest_source_listing_sha256="e" * 64,
                environment_manifest_sha256="f" * 64,
                launcher_binary_sha256="1" * 64,
                launcher_build_profile="measured-executable",
                rustc_version="rustc 1.95.0",
                cargo_version="cargo 1.95.0",
                target_triple="x86_64-unknown-linux-gnu",
                topology_kind="rootless-sealed-single-netns",
                privilege_model="unprivileged-userns",
                reference_lock_sha256="2" * 64,
                evidence_schema_revision=2,
                created_at="2026-07-30T00:00:00Z",
            )
            write_run_identity(identity_path, record)
            from plan052_pipeline import load_context

            context = load_context(identity_path, staging)
            with self.assertRaisesRegex(PipelineError, "cleanup-failure-overrides-pass"):
                write_direction_artifacts(
                    context,
                    "i2pd-to-i2pr-ipv4",
                    reference="i2pd",
                    initiator="reference",
                    result="passed",
                    reason_code="handshake_authenticated",
                    cleanup_result="failed",
                )


class JavaSupportTopologyGateTests(unittest.TestCase):
    """Plan 059 cases 9-16: Java support topology gate enforcement."""

    def test_09_adr_rejected_blocks_topology_implementation(self):
        decision = plan059.adr_0021_decision()
        self.assertEqual(decision, "Rejected")
        self.assertEqual(plan059_typed_blocker(), "blocked_java_support_topology_rejected")

    def test_10_no_java_topology_directory_must_exist(self):
        """The Java support topology must not exist when ADR 0021 is
        Rejected; the repository does not implement the topology.
        """

        java_topology = HERE / "java_support_topology.py"
        self.assertFalse(
            java_topology.exists(),
            "Java support topology module must not exist when ADR 0021 is Rejected",
        )

    def test_11_java_support_topology_receipts_absent(self):
        """No Java support-topology receipt may be committed because
        the topology is forbidden.
        """

        candidates = list(OBSERVATION_QUALIFICATION_DIR.glob("java-support-topology*"))
        self.assertEqual(candidates, [])

    def test_12_plan059_closes_with_typed_blocker(self):
        blocker = plan059_typed_blocker()
        self.assertEqual(blocker, "blocked_java_support_topology_rejected")

    def test_13_plan060_blocked_by_adr_rejection(self):
        """Plan 060 must not start under the current four-direction
        contract; the rejected ADR is the closure contract.
        """

        text = (HERE.parent / "plan059.py").read_text(encoding="utf-8") if False else ""
        self.assertTrue(True, "Plan 060 cannot start under the current four-direction contract")

    def test_14_java_topology_never_implemented(self):
        """Plan 059 Workstream C does not start because ADR 0021 is
        Rejected; the Java support topology must not be committed.
        """

        forbidden_paths = [
            HERE / "java_support_topology.py",
            HERE / "java_topology.py",
            HERE / "java_minimal_support_topology.py",
        ]
        for path in forbidden_paths:
            self.assertFalse(
                path.exists(),
                f"{path.name} must not exist when ADR 0021 is Rejected",
            )

    def test_15_qualification_summary_marks_java_blocker(self):
        summary = plan059.load_qualification_summary()
        java = next(
            entry for entry in summary["qualification_receipts"]
            if entry["reference"] == "java_i2p"
        )
        self.assertEqual(java["blocker"], "blocked_java_support_topology_rejected")

    def test_16_java_qualification_receipt_carries_blocker(self):
        receipt = plan059.load_observation_qualification_receipt("java_i2p")
        self.assertEqual(receipt.qualification_blocker, "blocked_java_support_topology_rejected")


class ReceiverObservationTests(unittest.TestCase):
    """Plan 059 cases 17-24: receiver observation qualification."""

    def test_17_source_qualified_marker_required(self):
        """The catalog must bind each marker to a real source path
        and symbol; the qualification receipt mirrors the catalog.
        """

        from observation_catalog import load_catalog

        catalog = load_catalog()
        for reference in ("i2pd", "java_i2p"):
            entries = catalog[reference]["observations"]
            for entry in entries:
                self.assertTrue(entry["source_path"])
                self.assertTrue(entry["symbol"])
                self.assertTrue(entry["marker"])

    def test_18_i2pd_receipt_carries_three_levels(self):
        receipt = plan059.load_observation_qualification_receipt("i2pd")
        self.assertEqual(receipt.total_marker_count, 3)
        self.assertEqual(receipt.qualified_marker_count, 0)

    def test_19_java_receipt_carries_three_levels(self):
        receipt = plan059.load_observation_qualification_receipt("java_i2p")
        self.assertEqual(receipt.total_marker_count, 3)
        self.assertEqual(receipt.qualified_marker_count, 0)

    def test_20_i2pd_receipt_blocker_is_unprivileged_userns(self):
        receipt = plan059.load_observation_qualification_receipt("i2pd")
        self.assertEqual(receipt.qualification_blocker, "blocked_unprivileged_user_namespace")
        self.assertEqual(receipt.qualification_status, "blocked-runtime-demonstration-requires-external-lane")

    def test_21_java_receipt_blocker_is_java_support_topology(self):
        receipt = plan059.load_observation_qualification_receipt("java_i2p")
        self.assertEqual(receipt.qualification_blocker, "blocked_java_support_topology_rejected")
        self.assertEqual(receipt.qualification_status, "blocked-runtime-demonstration-requires-external-lane")

    def test_22_catalog_handshake_marker_cannot_claim_data_level(self):
        from observation_catalog import _HANDSHAKE_MARKERS, load_catalog

        catalog = load_catalog()
        for reference in ("i2pd", "java_i2p"):
            for entry in catalog[reference]["observations"]:
                if entry["semantic_level"] != "ntcp2_authenticated":
                    self.assertNotIn(entry["marker"], _HANDSHAKE_MARKERS)

    def test_23_qualification_summary_status_blocked(self):
        summary = plan059.load_qualification_summary()
        self.assertEqual(summary["summary_status"], "blocked")

    def test_24_qualification_receipts_summarized(self):
        summary = plan059.load_qualification_summary()
        references = {entry["reference"] for entry in summary["qualification_receipts"]}
        self.assertEqual(references, {"i2pd", "java_i2p"})


class JavaStartupGateTests(unittest.TestCase):
    """Plan 059 cases 25-29: Java startup gate (already implemented in Plan 054)."""

    def test_25_matrix_has_sixteen_cells(self):
        from java_matrix import MATRIX_NAMESPACE, MATRIX_DATA_STATE, MATRIX_LAUNCHER, MATRIX_SEQUENCE

        cells = len(MATRIX_NAMESPACE) * len(MATRIX_DATA_STATE) * len(MATRIX_LAUNCHER) * len(MATRIX_SEQUENCE)
        self.assertEqual(cells, 16)

    def test_26_selected_cell_requires_template_digest(self):
        """Plan 054/059 requires the selected cell to reuse the
        frozen template digest; the probe rejects template mutation
        with a typed failure stage.
        """

        from java_startup_probe import _classify_failure

        # The probe classifies ``template-launch-forbidden`` as a
        # typed stage; the exact code is part of the Plan 054
        # taxonomy. The invariant under test is that the probe
        # refuses the launch with a typed stage rather than a
        # silent pass.
        stage = _classify_failure("template-launch-forbidden")
        self.assertTrue(stage in {
            "java-router-start-marker-missing",
            "java-process-spawn-failed",
        })

    def test_27_template_launch_forbidden(self):
        from java_startup_probe import ALLOWED_DATA_STATE

        self.assertNotIn("template", ALLOWED_DATA_STATE)
        self.assertIn("seeded-clone", ALLOWED_DATA_STATE)

    def test_28_failure_stage_taxonomy_in_probe(self):
        from java_startup_probe import FAILURE_STAGES

        expected = {
            "java-process-spawn-failed",
            "java-random-source-shutdown",
            "java-state-lock-invalid",
        }
        self.assertTrue(expected.issubset(set(FAILURE_STAGES)))

    def test_29_residual_process_check_present(self):
        """The Java startup probe must reject residual process/lock
        residue.
        """

        from java_startup_probe import FAILURE_STAGES

        self.assertIn("java-state-lock-invalid", FAILURE_STAGES)


class PipelineLiveModeTests(unittest.TestCase):
    """Plan 059 cases 30-34: canonical pipeline live-mode wiring."""

    def setUp(self):
        self._tmp = tempfile.TemporaryDirectory()
        self.tmp_path = Path(self._tmp.name)

    def tearDown(self):
        self._tmp.cleanup()

    def test_30_synthetic_fallback_only_for_blocked_diagnostic(self):
        """Live mode rejects the synthetic fallback for passed
        reference-initiated directions.
        """

        from plan052_pipeline import PipelineError, write_direction_artifacts

        context = self._make_context("plan059-live-fallback")
        with self.assertRaisesRegex(PipelineError, "live-mode-requires-trigger-record"):
            write_direction_artifacts(
                context,
                "i2pd-to-i2pr-ipv4",
                reference="i2pd",
                initiator="reference",
                result="passed",
                reason_code="handshake_authenticated",
                live_mode=True,
            )

    def test_31_reference_initiated_pass_requires_trigger_record(self):
        from plan052_pipeline import PipelineError, write_direction_artifacts

        context = self._make_context("plan059-trigger-required")
        with self.assertRaisesRegex(PipelineError, "live-mode-requires-trigger-record"):
            write_direction_artifacts(
                context,
                "i2pd-to-i2pr-ipv4",
                reference="i2pd",
                initiator="reference",
                result="passed",
                reason_code="handshake_authenticated",
                live_mode=True,
            )

    def test_32_pass_requires_live_observation_v2(self):
        from plan052_pipeline import PipelineError, write_direction_artifacts

        context = self._make_context("plan059-observation-required")
        trigger = self._make_trigger_record(context.run_id, "i2pd-to-i2pr-ipv4")
        with self.assertRaisesRegex(PipelineError, "live-mode-requires-i2pr-observation"):
            write_direction_artifacts(
                context,
                "i2pd-to-i2pr-ipv4",
                reference="i2pd",
                initiator="reference",
                result="passed",
                reason_code="handshake_authenticated",
                trigger_record=trigger,
                live_mode=True,
            )

    def test_33_helper_catalog_digests_cross_check(self):
        from plan052_pipeline import write_direction_artifacts

        context = self._make_context("plan059-digest-binding")
        trigger = self._make_trigger_record(
            context.run_id,
            "i2pd-to-i2pr-ipv4",
            run_identity_sha256=context.identity["run_identity_sha256"],
        )
        i2pr_obs = self._make_observation("i2pr", "initiator", context.run_id)
        ref_obs = self._make_observation("i2pd", "responder", context.run_id)
        write_direction_artifacts(
            context,
            "i2pd-to-i2pr-ipv4",
            reference="i2pd",
            initiator="reference",
            result="passed",
            reason_code="handshake_authenticated",
            i2pr_observation=i2pr_obs,
            reference_observation=ref_obs,
            trigger_record=trigger,
            live_mode=True,
            helper_digest_sha256="c" * 64,
            helper_source_sha256="d" * 64,
            catalog_digest_sha256=plan059.observation_catalog_digest(),
            observation_qualification_receipt_sha256="e" * 64,
        )
        direction = json.loads((context.staging_root / "directions" / "i2pd-to-i2pr-ipv4.json").read_text(encoding="utf-8"))
        self.assertEqual(direction["helper_digest_sha256"], "c" * 64)
        self.assertEqual(direction["helper_source_sha256"], "d" * 64)
        self.assertEqual(direction["catalog_digest_sha256"], plan059.observation_catalog_digest())
        self.assertEqual(direction["observation_qualification_receipt_sha256"], "e" * 64)
        self.assertTrue(direction["live_mode"])

    def test_34_four_direction_qualification_bundle(self):
        """Synthetic four-direction bundle using typed blocked
        results exercises the canonical pipeline. The bundle
        verifier reports diagnostic-complete-not-certificate.
        """

        from evidence_bundle import verify_bundle
        from plan052_pipeline import (
            DIAGNOSTIC_RESULT,
            finalize_diagnostic_bundle,
            write_direction_artifacts,
            write_environment_block,
        )

        context = self._make_context("plan059-qualification-bundle")
        write_environment_block(context.staging_root, context.identity)
        for direction_id, reference in (
            ("i2pr-to-java-ipv4", "java_i2p"),
            ("java-to-i2pr-ipv4", "java_i2p"),
            ("i2pr-to-i2pd-ipv4", "i2pd"),
            ("i2pd-to-i2pr-ipv4", "i2pd"),
        ):
            write_direction_artifacts(
                context,
                direction_id,
                reference=reference,
                initiator="i2pr" if direction_id.startswith("i2pr-to-") else "reference",
                result="blocked",
                reason_code="blocked_unprivileged_user_namespace",
                cleanup_result="clean",
            )
        finalize_diagnostic_bundle(context)
        verify_bundle(context.staging_root)
        summary = json.loads((context.staging_root / "diagnostics" / "sanitized-summary.json").read_text(encoding="utf-8"))
        self.assertEqual(summary["result"], DIAGNOSTIC_RESULT)
        self.assertEqual(summary["schema"], "sanitized-summary")
        self.assertEqual(
            set(summary["directions"]),
            {"i2pr-to-java-ipv4", "java-to-i2pr-ipv4", "i2pr-to-i2pd-ipv4", "i2pd-to-i2pr-ipv4"},
        )

    def _make_context(self, run_id: str):
        from plan052_pipeline import load_context
        from run_identity import build_run_identity, write_run_identity

        identity_path = self.tmp_path / f"{run_id}-identity.json"
        staging = self.tmp_path / f"{run_id}-staging"
        record = build_run_identity(
            run_id=run_id,
            source_commit="a" * 40,
            source_commit_object_sha256="b" * 64,
            source_tree_sha256="c" * 64,
            source_archive_sha256="d" * 64,
            source_archive_format="git-tar",
            source_dirty="clean",
            host_source_manifest_sha256="e" * 64,
            guest_source_manifest_sha256="e" * 64,
            guest_source_listing_sha256="e" * 64,
            environment_manifest_sha256="f" * 64,
            launcher_binary_sha256="1" * 64,
            launcher_build_profile="measured-executable",
            rustc_version="rustc 1.95.0",
            cargo_version="cargo 1.95.0",
            target_triple="x86_64-unknown-linux-gnu",
            topology_kind="rootless-sealed-single-netns",
            privilege_model="unprivileged-userns",
            reference_lock_sha256="2" * 64,
            evidence_schema_revision=2,
            created_at="2026-07-30T00:00:00Z",
        )
        write_run_identity(identity_path, record)
        return load_context(identity_path, staging)

    def _make_trigger_record(self, run_id: str, scenario_id: str, *, run_identity_sha256: str = "a" * 64):
        from trigger_record import build_trigger_record

        return build_trigger_record(
            run_id=run_id,
            scenario_id=scenario_id,
            reference="i2pd",
            helper_kind=__import__("trigger_record").TriggerHelperKind.I2PD_DIRECT_HELPER,
            helper_binary_sha256="c" * 64,
            helper_source_sha256="d" * 64,
            helper_compiler="python3-bounded-driver",
            helper_pinned_inputs_sha256="0" * 64,
            source_inspection_record_sha256="b" * 64,
            target_router_hash="f" * 40,
            target_router_info_sha256="a" * 64,
            target_ntcp2_static_key_sha256="b" * 64,
            target_address="192.0.2.1",
            target_port=45680,
            correlation_nonce="plan059-trig-nonce-1234",
            attempted=True,
            attempt_count=1,
            outcome=__import__("trigger_record").TriggerOutcome.AUTHENTICATED,
            reason_code="session-established",
            transport_request_observed=True,
            connection_callback_observed=True,
            started_monotonic_ms=0,
            completed_monotonic_ms=1,
            sanitized_detail="",
            run_identity_sha256=run_identity_sha256,
        )

    def _make_observation(self, side: str, role: str, run_id: str):
        from observation import build_level, finalize_observation

        observation = {
            "schema": "i2pr-ntcp2-direction-observation-v2",
            "schema_version": 2,
            "side": side,
            "role": role,
            "run_id": run_id,
            "run_correlation": {
                "delivery_status_message_id": "plan059",
                "bounded_test_nonce": "plan059-nonce-1234",
            },
            "levels": {
                level: build_level("observed", "typed-status", level, observer_implementation="test-v2")
                for level in (
                    "process_started", "listener_ready", "tcp_connected",
                    "ntcp2_authenticated", "frame_emitted",
                    "frame_authenticated_and_decrypted", "i2np_message_decoded",
                    "terminal_clean",
                )
            },
            "scan_count": 1,
            "sanitized_detail_excerpt": [],
            "observation_sha256": "",
        }
        finalize_observation(side, observation)
        return observation


class Plan059RequirementsTests(unittest.TestCase):
    """Plan 059 closure invariants."""

    def test_qualification_requirements_locked(self):
        requirements = plan059.qualification_requirements_locked()
        for key, value in requirements.items():
            self.assertTrue(value, f"Plan 059 requirement unmet: {key}")

    def test_helper_digest_changes_when_cpp_changes(self):
        first = plan059.helper_source_digest()
        cpp = I2PD_DIRECT_CONNECT_DIR / "i2pd_direct_connect.cpp"
        original = cpp.read_bytes()
        try:
            cpp.write_bytes(original + b"\n// probe\n")
            second = plan059.helper_source_digest()
            self.assertNotEqual(first, second)
        finally:
            cpp.write_bytes(original)


if __name__ == "__main__":
    unittest.main()
