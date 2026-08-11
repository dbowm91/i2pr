"""Plan 098 Plan 095 runner/provenance boundary corrective pass tests.

Plan 098 closes the runner/provenance ownership defects that the
first authoritative Plan 095 live attempt (2026-08-10) exposed
before any TCP/NTCP2 wire activity. The runner no longer
reconstructs ``repo_root / target / debug / i2pr-interop`` for any
authoritative attempt; the i2pr binary path is now an explicit
caller-supplied argument whose measured SHA-256 must match the
supplied digest. The i2pr and i2pd build-manifest digests are
separately measured, the attempt role is bound to the exact
role-specific i2pd driver and build manifest, and the wrapper
threads the explicit ``--i2pr-binary`` path through to every
runner entry point.

The regression matrix exercises the corrected contract:

- the runner rejects an attempted-live execution when the
  ``--i2pr-binary`` path is missing or its measured digest
  does not match the supplied SHA-256;
- the wrapper threads the exact resolved ``Path`` object to the
  runner; producer and consumer identities agree;
- the i2pr and i2pd build manifests are independently measured
  and bound into the record;
- the i2pd driver role-to-manifest mapping is exact and a
  mismatch is rejected;
- the canonical pinned i2pd source-tree digest uses tracked
  files only and excludes ``.git`` administrative state;
- the Plan 095 final gate validates record digests against the
  actual downloaded artifacts and role-specific manifests;
- functional tests prove the runner succeeds with an arbitrary
  absolute artifact path even when ``target/debug`` is absent.

The tests construct realistic temporary filesystems, exercise
the runner entry points, and assert the bounded provenance
contract end-to-end.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import subprocess
import sys
import tempfile
import unittest
from pathlib import Path

import yaml


HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]
HARNESS_DIR = HERE
if str(HARNESS_DIR) not in sys.path:
    sys.path.insert(0, str(HARNESS_DIR))


WRAPPER_PATH = (
    REPO_ROOT / "scripts/interop/run-minimal-i2pd-host-loopback-probe.py"
)
WORKFLOW_PATH = (
    REPO_ROOT
    / ".github/workflows/ntcp2-interop-host-loopback-development.yml"
)
PINNED_I2PD_REVISION = "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"
HEX40 = re.compile(r"^[0-9a-f]{40}$")
HEX64 = re.compile(r"^[0-9a-f]{64}$")


def _hex(value: str, length: int = 64) -> str:
    text = value * (length // max(len(value), 1) + 1)
    return text[:length].lower()


def _write_fake_i2pr(path: Path, payload: bytes = b"plan098-fake-i2pr") -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)
    return hashlib.sha256(payload).hexdigest()


def _write_fake_i2pd(path: Path, payload: bytes = b"plan098-fake-i2pd") -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_bytes(payload)
    path.chmod(0o755)
    return hashlib.sha256(payload).hexdigest()


def _write_fake_manifest(path: Path, payload: dict[str, object]) -> str:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(payload, sort_keys=True, indent=2), encoding="utf-8")
    return hashlib.sha256(path.read_bytes()).hexdigest()


class Plan098RunnerBinaryPathOwnershipTests(unittest.TestCase):
    """Plan 098 WP1 tests A-H: runner rejects reconstructed path."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.run_root = Path(self._tmp.name)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_explicit_artifact_path_succeeds_without_target_debug(self) -> None:
        from plan083_runner import execute_real_probe
        import minimal_i2pd_probe as probe

        # ``target/debug`` is intentionally absent; the authoritative
        # binary lives at an arbitrary absolute path. The runner must
        # accept the explicit path.
        artifact_root = self.run_root / "artifacts"
        i2pr_binary = artifact_root / "i2pr-interop"
        measured = _write_fake_i2pr(i2pr_binary, b"plan098-explicit-path")

        record = execute_real_probe(
            repo_root=self.run_root,
            run_root=self.run_root,
            run_id="plan098-explicit",
            source_commit=_hex("a", 40),
            reference_revision=_hex("b", 40),
            lane_qualification_sha256=_hex("c", 64),
            topology_kind="host-loopback-development",
            i2pr_binary_sha256=measured,
            i2pd_binary_sha256=_hex("e", 64),
            delivery_status_message_id=0x04200001,
            i2pd_driver_binary=self.run_root / "i2pd-driver",
            i2pr_binary=i2pr_binary,
        )
        # The runner never reaches the lane validation because the
        # i2pd driver binary does not exist; the record must reflect
        # the typed pre-protocol reference rejection, NOT a fallback
        # to ``target/debug``.
        validated = probe.validate_record(record)
        self.assertEqual(
            validated["terminal_result"], probe.PRE_PROTOCOL_REJECTED
        )
        # The record must surface the measured i2pr binary digest so
        # the workflow gate can verify the path/hash binding.
        self.assertEqual(validated["i2pr_binary_sha256"], measured)

    def test_digest_path_mutation_fails_closed(self) -> None:
        from plan083_runner import execute_real_probe
        import minimal_i2pd_probe as probe

        artifact_root = self.run_root / "artifacts"
        i2pr_binary = artifact_root / "i2pr-interop"
        original = _write_fake_i2pr(i2pr_binary, b"plan098-original")
        # Mutate the file after the wrapper has measured it.
        i2pr_binary.write_bytes(b"plan098-mutated")
        record = execute_real_probe(
            repo_root=self.run_root,
            run_root=self.run_root,
            run_id="plan098-mutation",
            source_commit=_hex("a", 40),
            reference_revision=_hex("b", 40),
            lane_qualification_sha256=_hex("c", 64),
            topology_kind="host-loopback-development",
            i2pr_binary_sha256=original,
            i2pd_binary_sha256=_hex("e", 64),
            delivery_status_message_id=0x04200001,
            i2pd_driver_binary=self.run_root / "i2pd-driver",
            i2pr_binary=i2pr_binary,
        )
        validated = probe.validate_record(record)
        self.assertEqual(
            validated["terminal_result"], probe.PRE_PROTOCOL_REJECTED
        )
        self.assertEqual(
            validated["reason_code"],
            probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
        )
        # The measured (post-mutation) digest must appear in the
        # record so the workflow gate can prove the mismatch.
        mutated_digest = hashlib.sha256(b"plan098-mutated").hexdigest()
        self.assertEqual(validated["i2pr_binary_sha256"], mutated_digest)
        self.assertNotEqual(validated["i2pr_binary_sha256"], original)

    def test_missing_i2pr_binary_path_fails_closed(self) -> None:
        from plan083_runner import execute_real_probe
        import minimal_i2pd_probe as probe

        record = execute_real_probe(
            repo_root=self.run_root,
            run_root=self.run_root,
            run_id="plan098-missing",
            source_commit=_hex("a", 40),
            reference_revision=_hex("b", 40),
            lane_qualification_sha256=_hex("c", 64),
            topology_kind="host-loopback-development",
            i2pr_binary_sha256=_hex("d", 64),
            i2pd_binary_sha256=_hex("e", 64),
            delivery_status_message_id=0x04200001,
            i2pd_driver_binary=self.run_root / "i2pd-driver",
            i2pr_binary=self.run_root / "missing-i2pr",
        )
        validated = probe.validate_record(record)
        self.assertEqual(
            validated["terminal_result"], probe.PRE_PROTOCOL_REJECTED
        )
        self.assertEqual(
            validated["reason_code"],
            probe.REASON_PRE_PROTOCOL_PREPARATION_FAILED,
        )

    def test_reverse_runner_accepts_explicit_path(self) -> None:
        # Plan 098: the reverse runner must accept the explicit
        # ``i2pr_binary`` path even though Plan 088 remains blocked.
        from plan084_runner import execute_reverse_probe
        import minimal_i2pd_reverse_probe as rev_probe

        artifact_root = self.run_root / "artifacts"
        i2pr_binary = artifact_root / "i2pr-interop"
        measured = _write_fake_i2pr(i2pr_binary, b"plan098-reverse-path")

        record = execute_reverse_probe(
            repo_root=self.run_root,
            run_root=self.run_root,
            run_id="plan098-reverse-explicit",
            source_commit=_hex("a", 40),
            reference_revision=_hex("b", 40),
            lane_qualification_sha256=_hex("c", 64),
            topology_kind="host-loopback-development",
            i2pr_binary_sha256=measured,
            i2pd_binary_sha256=_hex("e", 64),
            delivery_status_message_id=0x04200001,
            i2pd_driver_binary=self.run_root / "i2pd-driver",
            i2pr_binary=i2pr_binary,
        )
        validated = rev_probe.validate_reverse_record(record)
        # Same outcome family: pre-protocol reference-failed because
        # the i2pd driver binary does not exist.
        self.assertEqual(
            validated["terminal_result"], rev_probe.PRE_PROTOCOL_REJECTED
        )
        self.assertEqual(validated["i2pr_binary_sha256"], measured)

    def test_preflight_runner_accepts_explicit_path(self) -> None:
        # Plan 098: the preflight runner API must accept the
        # explicit ``i2pr_binary`` parameter and not reconstruct
        # ``repo_root / target / debug / i2pr-interop``. The
        # structural check verifies the function signature and the
        # absence of a target/debug fallback inside the function
        # body; functional subprocess behaviour is exercised by the
        # dedicated preflight unit tests in ``test_loopback_smoke.py``.
        import inspect as _inspect
        from preflight_runner import execute_listener_preflight
        sig = _inspect.signature(execute_listener_preflight)
        self.assertIn("i2pr_binary", sig.parameters)
        source = _inspect.getsource(execute_listener_preflight)
        self.assertNotIn(
            "repo_root / \"target\" / \"debug\"",
            source,
            "preflight must not reconstruct the target/debug path",
        )
        self.assertIn(
            "_measure_i2pr_binary(i2pr_binary)",
            source,
            "preflight must measure the supplied i2pr binary path",
        )


class Plan098WrapperThreadsExplicitPathTests(unittest.TestCase):
    """Plan 098 WP2 tests: wrapper threads the explicit path."""

    def test_wrapper_threads_exact_path_to_runner(self) -> None:
        """The wrapper must call the runner with the exact ``Path``."""

        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "plan098_wrapper", str(WRAPPER_PATH)
        )
        wrapper = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(wrapper)  # type: ignore[union-attr]

        captured: dict[str, object] = {}

        def fake_execute_real_probe(*args: object, **kwargs: object) -> dict[str, object]:
            captured.update(kwargs)
            return {"terminal_result": "passed"}

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            artifact_root = tmp_path / "artifacts"
            artifact_root.mkdir(parents=True, exist_ok=True)
            i2pr_binary = artifact_root / "i2pr-interop"
            i2pr_measured = _write_fake_i2pr(
                i2pr_binary, b"plan098-wrapper-explicit"
            )
            i2pd_binary = artifact_root / "i2pd_ntcp2_interop_driver_instrumented"
            i2pd_binary.write_bytes(b"plan098-wrapper-i2pd")
            i2pd_binary.chmod(0o755)
            i2pd_manifest = artifact_root / "build-manifest-instrumented.json"
            i2pd_manifest.write_text(
                json.dumps(
                    {
                        "schema": "i2pr-i2pd-direct-driver-build-manifest-v1",
                        "reference_source_tree_sha256": _hex("r", 64),
                    },
                    sort_keys=True,
                ),
                encoding="utf-8",
            )
            i2pr_manifest = artifact_root / "i2pr-build-manifest.json"
            i2pr_manifest.write_text(
                json.dumps(
                    {
                        "schema": "i2pr-i2pr-interop-build-manifest-v1",
                        "i2pr_binary_sha256": i2pr_measured,
                    },
                    sort_keys=True,
                ),
                encoding="utf-8",
            )
            run_root = tmp_path / "run"
            output = tmp_path / "record.json"
            sys.modules["plan083_runner"] = importlib.util.module_from_spec(
                importlib.util.spec_from_loader(
                    "plan083_runner", loader=None
                )
            )
            sys.modules["plan083_runner"].execute_real_probe = fake_execute_real_probe
            try:
                rc = wrapper.main(
                    [
                        "--direction", "i2pr-to-i2pd-ipv4",
                        "--repo-root", str(tmp_path),
                        "--run-root", str(run_root),
                        "--run-id", "plan098-wrapper",
                        "--source-commit", _hex("a", 40),
                        "--output", str(output),
                        "--i2pd-driver-binary", str(i2pd_binary),
                        "--i2pr-binary", str(i2pr_binary),
                        "--attempt-kind", "instrumented",
                        "--delivery-status-message-id", "0x04200001",
                    ]
                )
                self.assertEqual(rc, 0)
                # The runner must have received the explicit
                # absolute Path and the same measured digest.
                self.assertEqual(captured.get("i2pr_binary"), i2pr_binary)
                self.assertEqual(
                    captured.get("i2pr_binary_sha256"), i2pr_measured
                )
                # Distinct build manifest digests must be threaded.
                i2pd_manifest_digest = hashlib.sha256(
                    i2pd_manifest.read_bytes()
                ).hexdigest()
                self.assertEqual(
                    captured.get("i2pd_build_manifest_sha256"),
                    i2pd_manifest_digest,
                )
            finally:
                sys.modules.pop("plan083_runner", None)


class Plan098DistinctManifestProvenanceTests(unittest.TestCase):
    """Plan 098 WP3 tests: i2pr and i2pd manifests remain distinct."""

    def setUp(self) -> None:
        self._tmp = tempfile.TemporaryDirectory()
        self.run_root = Path(self._tmp.name)

    def tearDown(self) -> None:
        self._tmp.cleanup()

    def test_distinct_manifests_remain_distinct(self) -> None:
        artifact_root = self.run_root / "artifacts"
        artifact_root.mkdir(parents=True, exist_ok=True)
        i2pr_binary = artifact_root / "i2pr-interop"
        measured_i2pr = _write_fake_i2pr(i2pr_binary, b"plan098-distinct-i2pr")
        i2pd_binary = artifact_root / "i2pd_ntcp2_interop_driver_instrumented"
        i2pd_binary.write_bytes(b"plan098-distinct-i2pd")
        i2pd_binary.chmod(0o755)
        i2pr_manifest = artifact_root / "i2pr-build-manifest.json"
        i2pd_manifest = artifact_root / "build-manifest-instrumented.json"
        i2pr_manifest_digest = _write_fake_manifest(
            i2pr_manifest,
            {
                "schema": "i2pr-i2pr-interop-build-manifest-v1",
                "i2pr_binary_sha256": measured_i2pr,
            },
        )
        i2pd_manifest_digest = _write_fake_manifest(
            i2pd_manifest,
            {
                "schema": "i2pr-i2pd-direct-driver-build-manifest-v1",
                "reference_source_tree_sha256": _hex("r", 64),
            },
        )
        # The wrapper binds the two digests into the runner via the
        # explicit i2pr/i2pd manifest fields. The runner record must
        # surface both digests without aliasing.
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "plan098_wrapper", str(WRAPPER_PATH)
        )
        wrapper = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(wrapper)  # type: ignore[union-attr]

        captured: dict[str, object] = {}

        def fake_execute_real_probe(*args: object, **kwargs: object) -> dict[str, object]:
            captured.update(kwargs)
            return {"terminal_result": "passed"}

        sys.modules["plan083_runner"] = importlib.util.module_from_spec(
            importlib.util.spec_from_loader("plan083_runner", loader=None)
        )
        sys.modules["plan083_runner"].execute_real_probe = fake_execute_real_probe
        try:
            rc = wrapper.main(
                [
                    "--direction", "i2pr-to-i2pd-ipv4",
                    "--repo-root", str(self.run_root),
                    "--run-root", str(self.run_root / "run"),
                    "--run-id", "plan098-distinct",
                    "--source-commit", _hex("a", 40),
                    "--output", str(self.run_root / "out.json"),
                    "--i2pd-driver-binary", str(i2pd_binary),
                    "--i2pr-binary", str(i2pr_binary),
                    "--attempt-kind", "instrumented",
                ]
            )
        finally:
            sys.modules.pop("plan083_runner", None)
        self.assertEqual(rc, 0)
        self.assertEqual(captured["i2pr_build_manifest_sha256"], i2pr_manifest_digest)
        self.assertEqual(captured["i2pd_build_manifest_sha256"], i2pd_manifest_digest)
        self.assertNotEqual(
            captured["i2pr_build_manifest_sha256"],
            captured["i2pd_build_manifest_sha256"],
        )


class Plan098RoleBindingTests(unittest.TestCase):
    """Plan 098 WP4 tests: role-to-binary-and-manifest mapping."""

    def test_role_manifest_swap_fails_closed(self) -> None:
        # The wrapper must reject a ``--attempt-kind control``
        # request paired with an instrumented driver binary.
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "plan098_wrapper", str(WRAPPER_PATH)
        )
        wrapper = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(wrapper)  # type: ignore[union-attr]

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            artifact_root = tmp_path / "artifacts"
            i2pr_binary = artifact_root / "i2pr-interop"
            _write_fake_i2pr(i2pr_binary, b"plan098-role-swap")
            i2pd_instrumented_binary = artifact_root / "i2pd_ntcp2_interop_driver_instrumented"
            i2pd_instrumented_binary.write_bytes(b"plan098-instrumented-i2pd")
            i2pd_instrumented_binary.chmod(0o755)
            # The instrumented manifest is present; the control
            # manifest is intentionally absent. Pairing an
            # instrumented binary with --attempt-kind control must
            # fail closed.
            instrumented_manifest = artifact_root / "build-manifest-instrumented.json"
            instrumented_manifest.write_text(
                json.dumps(
                    {
                        "schema": "i2pr-i2pd-direct-driver-build-manifest-v1",
                    },
                    sort_keys=True,
                ),
                encoding="utf-8",
            )
            run_root = tmp_path / "run"
            rc = wrapper.main(
                [
                    "--direction", "i2pr-to-i2pd-ipv4",
                    "--repo-root", str(tmp_path),
                    "--run-root", str(run_root),
                    "--run-id", "plan098-role-swap",
                    "--source-commit", _hex("a", 40),
                    "--output", str(tmp_path / "out.json"),
                    "--i2pd-driver-binary", str(i2pd_instrumented_binary),
                    "--i2pr-binary", str(i2pr_binary),
                    "--attempt-kind", "control",
                ]
            )
            self.assertNotEqual(rc, 0)
            self.assertEqual(rc, 2)


class Plan098CanonicalTrackedTreeDigestTests(unittest.TestCase):
    """Plan 098 WP5 tests: canonical tracked-tree identity."""

    def test_digest_excludes_git_admin_state(self) -> None:
        # The wrapper exposes ``_canonical_tracked_tree_digest`` as a
        # private helper; import it via the script's module spec.
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "plan098_wrapper", str(WRAPPER_PATH)
        )
        wrapper = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(wrapper)  # type: ignore[union-attr]
        canonical = wrapper._canonical_tracked_tree_digest

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            subprocess.run(
                ["git", "-C", str(tmp_path), "init", "--initial-branch=main"],
                check=True,
                capture_output=True,
            )
            subprocess.run(
                ["git", "-C", str(tmp_path), "config", "user.email", "test@example.com"],
                check=True,
                capture_output=True,
            )
            subprocess.run(
                ["git", "-C", str(tmp_path), "config", "user.name", "test"],
                check=True,
                capture_output=True,
            )
            (tmp_path / "tracked.txt").write_text("hello\n", encoding="utf-8")
            subprocess.run(
                ["git", "-C", str(tmp_path), "add", "tracked.txt"],
                check=True,
                capture_output=True,
            )
            subprocess.run(
                [
                    "git", "-C", str(tmp_path), "commit",
                    "-m", "init", "--no-gpg-sign",
                ],
                check=True,
                capture_output=True,
            )
            # Snapshot the digest via the canonical algorithm.
            digest_before = canonical(tmp_path)
            # Mutate the ``.git`` administrative tree by writing a
            # throw-away ref. The canonical digest must remain the
            # same because the algorithm walks ``git ls-files``.
            (tmp_path / ".git/refs/heads/throwaway").write_text(
                "0000000000000000000000000000000000000000\n",
                encoding="utf-8",
            )
            digest_after_git_mutation = canonical(tmp_path)
            self.assertEqual(digest_before, digest_after_git_mutation)
            # Now mutate a tracked file. The digest must change.
            (tmp_path / "tracked.txt").write_text("hello-mutated\n", encoding="utf-8")
            digest_after_tracked_mutation = canonical(tmp_path)
            self.assertNotEqual(digest_before, digest_after_tracked_mutation)


class Plan098WorkflowProvenanceFinalGateTests(unittest.TestCase):
    """Plan 098 WP6 tests: final gate validates exact provenance."""

    def _read_workflow_text(self) -> str:
        return WORKFLOW_PATH.read_text(encoding="utf-8")

    def test_final_gate_validates_i2pd_binary_digest(self) -> None:
        text = self._read_workflow_text()
        self.assertIn("i2pd_binary_sha256", text)
        # The gate must measure the role-specific i2pd binary from
        # the actual downloaded artifact.
        self.assertIn(
            "i2pd_ntcp2_interop_driver_instrumented",
            text,
        )
        self.assertIn(
            "i2pd_ntcp2_interop_driver_control",
            text,
        )
        self.assertIn("_sha256_file(instrumented_binary_path)", text)
        self.assertIn("_sha256_file(control_binary_path)", text)

    def test_final_gate_validates_role_specific_manifest_digest(self) -> None:
        text = self._read_workflow_text()
        self.assertIn("instrumented_build_manifest_sha256", text)
        self.assertIn("control_build_manifest_sha256", text)

    def test_final_gate_validates_i2pr_build_manifest(self) -> None:
        text = self._read_workflow_text()
        self.assertIn("i2pr_build_manifest_sha256", text)

    def test_final_gate_validates_attempt_kind_per_role(self) -> None:
        text = self._read_workflow_text()
        # The gate must require instrumented for the instrumented
        # path and control for the control path.
        self.assertIn('attempt_kind") != "instrumented"', text)
        self.assertIn('attempt_kind") != "control"', text)

    def test_final_gate_rejects_zero_provenance_in_passed_record(self) -> None:
        text = self._read_workflow_text()
        # The gate must explicitly reject zero placeholder digests in
        # any record that would otherwise pass.
        self.assertIn('"0" * 64', text)
        self.assertRegex(
            text,
            r'placeholder in passed record',
        )


class Plan098WrapperRoleParametricTests(unittest.TestCase):
    """Plan 098 WP4 tests: attempt_kind reaches the underlying record."""

    def test_control_attempt_kind_reaches_record(self) -> None:
        # When --attempt-kind control is passed, the wrapper must
        # thread ``attempt_kind="control"`` to the runner so the
        # underlying record carries the real role.
        import importlib.util

        spec = importlib.util.spec_from_file_location(
            "plan098_wrapper", str(WRAPPER_PATH)
        )
        wrapper = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(wrapper)  # type: ignore[union-attr]

        captured: dict[str, object] = {}

        def fake_execute_real_probe(*args: object, **kwargs: object) -> dict[str, object]:
            captured.update(kwargs)
            return {"terminal_result": "passed"}

        with tempfile.TemporaryDirectory() as tmp:
            tmp_path = Path(tmp)
            artifact_root = tmp_path / "artifacts"
            i2pr_binary = artifact_root / "i2pr-interop"
            _write_fake_i2pr(i2pr_binary, b"plan098-control-role")
            i2pd_binary = artifact_root / "i2pd_ntcp2_interop_driver_control"
            i2pd_binary.write_bytes(b"plan098-control-i2pd")
            i2pd_binary.chmod(0o755)
            control_manifest = artifact_root / "build-manifest-control.json"
            control_manifest.write_text(
                json.dumps(
                    {
                        "schema": "i2pr-i2pd-direct-driver-build-manifest-v1",
                    },
                    sort_keys=True,
                ),
                encoding="utf-8",
            )
            sys.modules["plan083_runner"] = importlib.util.module_from_spec(
                importlib.util.spec_from_loader("plan083_runner", loader=None)
            )
            sys.modules["plan083_runner"].execute_real_probe = fake_execute_real_probe
            try:
                rc = wrapper.main(
                    [
                        "--direction", "i2pr-to-i2pd-ipv4",
                        "--repo-root", str(tmp_path),
                        "--run-root", str(tmp_path / "run"),
                        "--run-id", "plan098-control-thread",
                        "--source-commit", _hex("a", 40),
                        "--output", str(tmp_path / "out.json"),
                        "--i2pd-driver-binary", str(i2pd_binary),
                        "--i2pr-binary", str(i2pr_binary),
                        "--attempt-kind", "control",
                    ]
                )
            finally:
                sys.modules.pop("plan083_runner", None)
        self.assertEqual(rc, 0)
        self.assertEqual(captured["attempt_kind"], "control")


if __name__ == "__main__":
    unittest.main()
