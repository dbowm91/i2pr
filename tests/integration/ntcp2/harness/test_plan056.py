"""Plan 056 two-bundle certificate verification tests.

These tests cover every Plan 056 Workstream 6.1/6.3 check:
the positive fixture (two independently-generated bundles pass),
and the negative fixtures that each independently prevent the
certificate from being issued.

Every negative fixture is built from the positive base and mutates
exactly one cross-bundle invariant so the failure reason is unambiguous.
"""

from __future__ import annotations

import copy
import json
import os
import shutil
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]
if str(HERE) not in sys.path:
    sys.path.insert(0, str(HERE))

from evidence_bundle import (
    DIRECTION_CLASSES,
    ENVIRONMENT_CLASSES,
    PRIMARY_DIRECTIONS,
    finalize_bundle,
    write_json_atomic,
)
from run_identity import build_run_identity
from verify_milestone3_certificate import (
    CERTIFICATE_SCHEMA,
    CertificateError,
    verify_certificate,
)

_SAMPLE_RUN_IDENTITY_A = {
    "schema": "i2pr-interop-run-identity-v1",
    "schema_version": 1,
    "run_id": "plan056-a-20260101000000-aabbcc01",
    "created_at": "2026-01-01T00:00:00Z",
    "source_commit": "0" * 40,
    "source_commit_object_sha256": "a" * 64,
    "source_tree_sha256": "b" * 64,
    "source_archive_sha256": "c" * 64,
    "source_archive_format": "git-tar",
    "source_dirty": "clean",
    "host_source_manifest_sha256": "d" * 64,
    "guest_source_manifest_sha256": "e" * 64,
    "guest_source_listing_sha256": "f" * 64,
    "environment_manifest_sha256": "1" * 64,
    "launcher_binary_sha256": "2" * 64,
    "launcher_build_profile": "measured-executable",
    "rustc_version": "rustc 1.95.0 (abcdef 2026-01-01)",
    "cargo_version": "cargo 1.95.0 (abcdef 2026-01-01)",
    "target_triple": "x86_64-unknown-linux-gnu",
    "topology_kind": "rootless-sealed-single-netns",
    "privilege_model": "unprivileged-userns",
    "reference_lock_sha256": "3" * 64,
    "evidence_schema_revision": 2,
    "run_identity_sha256": "4" * 64,
}

_SAMPLE_RUN_IDENTITY_B = {
    **_SAMPLE_RUN_IDENTITY_A,
    "run_id": "plan056-b-20260101000000-aabbcc02",
    "created_at": "2026-01-02T00:00:00Z",
    "guest_source_manifest_sha256": "5" * 64,
    "guest_source_listing_sha256": "6" * 64,
    "run_identity_sha256": "7" * 64,
}


def _finalize_run_identity(payload: dict[str, object]) -> dict[str, object]:
    """Finalize ``payload`` by computing its canonical digest."""

    unsigned = dict(payload)
    unsigned["run_identity_sha256"] = ""
    digest = __import__("hashlib").sha256(
        json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()
    payload["run_identity_sha256"] = digest
    return payload


def _build_observation(
    *, scenario_id: str, run_id: str, nonce: str, i2pr_authenticated: bool = True,
    i2np_decoded: bool = True, frame_decrypted: bool = True,
    initiator: str | None = None,
) -> dict[str, object]:
    if initiator is None:
        initiator = "i2pr" if scenario_id.startswith("i2pr-to-") else (
            "java_i2p" if scenario_id.startswith("java-to-") else "i2pd"
        )
    receiver_kind = "java_i2p" if "java" in scenario_id else (
        "i2pd" if "i2pd" in scenario_id else "i2pr"
    )
    if initiator == "i2pr":
        sender_side, receiver_side = "i2pr", receiver_kind
    else:
        sender_side, receiver_side = initiator, "i2pr"
    observation_payload: dict[str, object] = {
        "schema": "i2pr-ntcp2-direction-observation-v2",
        "schema_version": 2,
        "scenario_id": scenario_id,
        "run_id": run_id,
        "run_correlation": {"delivery_status_message_id": nonce, "bounded_test_nonce": nonce},
        "sides": {
            sender_side: {
                "schema": "i2pr-ntcp2-direction-observation-v2",
                "schema_version": 2,
                "side": sender_side,
                "levels": _observation_levels(
                    frame_emitted=True, decrypted=True, decoded=True,
                    authenticated=i2pr_authenticated,
                ),
            },
            receiver_side: {
                "schema": "i2pr-ntcp2-direction-observation-v2",
                "schema_version": 2,
                "side": receiver_side,
                "levels": _observation_levels(
                    frame_emitted=False,
                    decrypted=frame_decrypted,
                    decoded=i2np_decoded,
                    authenticated=frame_decrypted,
                ),
            },
        },
        "observation_sha256": "",
    }
    for value in observation_payload["sides"].values():
        from observation import finalize_observation
        finalize_observation(value["side"], value)
    observation_payload["observation_sha256"] = __import__("hashlib").sha256(
        json.dumps(
            {**observation_payload, "observation_sha256": ""},
            sort_keys=True,
            separators=(",", ":"),
        ).encode()
    ).hexdigest()
    return observation_payload


def _observation_levels(
    *, frame_emitted: bool, decrypted: bool, decoded: bool, authenticated: bool,
) -> dict[str, object]:
    def level(state: str, code: str) -> dict[str, object]:
        return {
            "state": state,
            "source": "typed-status",
            "evidence_code": code,
            "sanitized_detail": "",
            "observer_implementation": "test-fixture",
        }
    return {
        "process_started": level("observed", "process-started"),
        "listener_ready": level("observed", "listener-ready"),
        "tcp_connected": level("observed", "tcp-connected"),
        "ntcp2_authenticated": level("observed" if authenticated else "not-observed", "ntcp2-authenticated"),
        "frame_emitted": level("observed" if frame_emitted else "not-observed", "frame-emitted"),
        "frame_authenticated_and_decrypted": level(
            "observed" if decrypted else "not-observed", "frame-authenticated-and-decrypted",
        ),
        "i2np_message_decoded": level(
            "observed" if decoded else "not-observed", "i2np-message-decoded",
        ),
        "terminal_clean": level("observed", "terminal-clean"),
    }


def _direction_record(
    *, scenario_id: str, run_id: str, nonce: str, result: str = "passed",
) -> dict[str, object]:
    return {
        "schema": "i2pr-mixed-router-direction-v2",
        "schema_version": 2,
        "run_id": run_id,
        "scenario_id": scenario_id,
        "reference": "java_i2p" if "java" in scenario_id else "i2pd",
        "initiator": "i2pr" if scenario_id.startswith("i2pr-to-") else (
            "java_i2p" if scenario_id.startswith("java-to-") else "i2pd"
        ),
        "responder": "i2pr" if not scenario_id.startswith("i2pr-to-") else (
            "java_i2p" if "java" in scenario_id else "i2pd"
        ),
        "actual_typed_result": result,
        "reason_code": "passed",
        "router_info": {
            "state": "not-applicable",
            "sha256": None,
        },
        "reference_metadata": {"kind": "java_i2p" if "java" in scenario_id else "i2pd", "state": "passed"},
        "runtime_counters": {"frames_sent": 1, "frames_received": 1, "i2np_messages_decoded": 1},
        "observation_sha256": "",
        "trigger_sha256": "",
        "attestation_sha256": "",
        "cleanup_sha256": "",
        "nonce": nonce,
    }


def _attestation_record(*, scenario_id: str, run_id: str) -> dict[str, object]:
    return {
        "schema": "i2pr-mixed-router-attestation-v2",
        "schema_version": 2,
        "run_id": run_id,
        "scenario_id": scenario_id,
        "topology_kind": "rootless-sealed-single-netns",
        "privilege_model": "unprivileged-userns",
        "authorization": "passed",
        "parent_network_state_unchanged": True,
    }


def _cleanup_record(
    *, scenario_id: str, run_id: str, cleanup_result: str = "clean",
) -> dict[str, object]:
    return {
        "schema": "i2pr-mixed-router-cleanup-v2",
        "schema_version": 2,
        "run_id": run_id,
        "scenario_id": scenario_id,
        "processes": {"expected": 2, "started": 2, "exited": 2, "forced": 0},
        "residual_processes": False,
        "topology_cleanup": cleanup_result,
        "final_result": cleanup_result,
    }


def _trigger_record(
    *, scenario_id: str, run_id: str, nonce: str,
) -> dict[str, object]:
    is_i2pr_initiator = scenario_id.startswith("i2pr-to-")
    if is_i2pr_initiator:
        return {
            "schema": "i2pr-reference-trigger-v3",
            "schema_version": 3,
            "run_id": run_id,
            "scenario_id": scenario_id,
            "mode": "not-required-i2pr-initiator",
            "attempted": False,
            "outcome": "not-required-i2pr-initiator",
            "reason_code": "i2pr-is-transport-initiator",
            "helper_kind": "not-applicable",
            "helper_binary_sha256": "0" * 64,
            "helper_source_sha256": "0" * 64,
            "helper_compiler": "",
            "helper_pinned_inputs_sha256": "0" * 64,
            "source_inspection_record_sha256": "0" * 64,
            "target_router_hash": "0" * 40,
            "target_router_info_sha256": "0" * 64,
            "target_ntcp2_static_key_sha256": "0" * 64,
            "target_address": "192.0.2.2",
            "target_port": 45680,
            "correlation_nonce": "0" * 16,
            "attempt_count": 0,
            "transport_request_observed": False,
            "connection_callback_observed": False,
            "started_monotonic_ms": 0,
            "completed_monotonic_ms": 0,
            "sanitized_detail": "",
            "trigger_sha256": "",
            "run_identity_sha256": _SAMPLE_RUN_IDENTITY_A["run_identity_sha256"],
        }
    return {
        "schema": "i2pr-reference-trigger-v3",
        "schema_version": 3,
        "run_id": run_id,
        "scenario_id": scenario_id,
        "reference": "java_i2p" if "java" in scenario_id else "i2pd",
        "reference_version": "2.12.0" if "java" in scenario_id else "2.60.0",
        "reference_revision": (
            "2800040deee9bb376567b671ef2e9c34cf3e30b6"
            if "java" in scenario_id
            else "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"
        ),
        "mode": "reference-trigger-attempted",
        "attempted": True,
        "outcome": "authenticated",
        "reason_code": "helper-attempted-and-acknowledged",
        "helper_kind": (
            "java-direct-helper" if "java" in scenario_id else "i2pd-direct-helper"
        ),
        "helper_binary_sha256": "a" * 64,
        "helper_source_sha256": "b" * 64,
        "helper_compiler": "rustc-1.95",
        "helper_pinned_inputs_sha256": "c" * 64,
        "source_inspection_record_sha256": "d" * 64,
        "target_router_hash": "e" * 40,
        "target_router_info_sha256": "f" * 64,
        "target_ntcp2_static_key_sha256": "0" * 64,
        "target_address": "192.0.2.2",
        "target_port": 45680,
        "correlation_nonce": nonce,
        "attempt_count": 1,
        "transport_request_observed": True,
        "connection_callback_observed": True,
        "started_monotonic_ms": 1000,
        "completed_monotonic_ms": 2500,
        "sanitized_detail": "fixture trigger dispatch",
        "trigger_sha256": "",
        "run_identity_sha256": _SAMPLE_RUN_IDENTITY_A["run_identity_sha256"],
    }


def _write_bundle(root: Path, identity: dict[str, object], *, nonce_seed: str) -> dict[str, object]:
    """Populate *root* with a valid Plan 052 evidence bundle.

    Returns a mapping describing the nonce assignments per direction so the
    test can copy the bundle while keeping observation/trigger independence.
    """

    if root.exists():
        shutil.rmtree(root)
    root.mkdir(parents=True)
    identity_payload = _finalize_run_identity(copy.deepcopy(identity))
    (root / "run-identity.json").write_text(
        json.dumps(identity_payload, separators=(",", ":")), encoding="utf-8",
    )
    env_dir = root / "environment"
    env_dir.mkdir()
    for filename in ENVIRONMENT_CLASSES:
        if filename.endswith(".sha256"):
            (env_dir / filename).write_text(
                "0" * 64 + "  parent-network.txt\n", encoding="ascii",
            )
        else:
            (env_dir / filename).write_text(
                json.dumps({"schema": "env", "filename": filename}, separators=(",", ":")) + "\n",
                encoding="utf-8",
            )
    nonces: dict[str, str] = {}
    for index, direction in enumerate(PRIMARY_DIRECTIONS):
        nonce = f"{nonce_seed}-{direction}-{index:04d}"
        nonces[direction] = nonce
        for direction_class in DIRECTION_CLASSES:
            class_dir = root / direction_class
            class_dir.mkdir(exist_ok=True)
            path = class_dir / f"{direction}.json"
            if direction_class == "observations":
                payload = _build_observation(
                    scenario_id=direction,
                    run_id=identity_payload["run_id"],
                    nonce=nonce,
                )
            elif direction_class == "directions":
                payload = _direction_record(
                    scenario_id=direction, run_id=identity_payload["run_id"], nonce=nonce,
                )
            elif direction_class == "attestations":
                payload = _attestation_record(
                    scenario_id=direction, run_id=identity_payload["run_id"],
                )
            elif direction_class == "cleanup":
                payload = _cleanup_record(
                    scenario_id=direction, run_id=identity_payload["run_id"],
                )
            elif direction_class == "triggers":
                payload = _trigger_record(
                    scenario_id=direction, run_id=identity_payload["run_id"], nonce=nonce,
                )
            else:
                raise AssertionError(f"unknown direction class: {direction_class}")
            path.write_text(
                json.dumps(payload, sort_keys=True, separators=(",", ":")) + "\n",
                encoding="utf-8",
            )
            os.chmod(path, 0o600)
    return {"nonces": nonces}


def _refinalize(root: Path) -> None:
    """Re-finalize a test bundle after mutating its contents."""

    for filename in ("manifest.json", "manifest.sha256"):
        path = root / filename
        if path.exists():
            path.unlink()
    identity = json.loads((root / "run-identity.json").read_text())
    finalize_bundle(root, identity["run_id"])


def _build_two_bundles(tmp: Path) -> tuple[Path, Path, dict[str, object]]:
    run_a = tmp / "run-a"
    run_b = tmp / "run-b"
    meta_a = _write_bundle(run_a, _SAMPLE_RUN_IDENTITY_A, nonce_seed="nonce-a")
    meta_b = _write_bundle(run_b, _SAMPLE_RUN_IDENTITY_B, nonce_seed="nonce-b")
    finalize_bundle(run_a, _SAMPLE_RUN_IDENTITY_A["run_id"])
    finalize_bundle(run_b, _SAMPLE_RUN_IDENTITY_B["run_id"])
    return run_a, run_b, {**meta_a, "b": meta_b}


class CertificatePositiveTests(unittest.TestCase):
    def test_two_independent_bundles_pass(self):
        with tempfile.TemporaryDirectory() as directory:
            run_a, run_b, _ = _build_two_bundles(Path(directory))
            certificate = verify_certificate(run_a, run_b)
            self.assertTrue(certificate["verified"], certificate)
            self.assertEqual(certificate["schema"], CERTIFICATE_SCHEMA)
            self.assertEqual(certificate["source_commit"], "0" * 40)
            self.assertEqual(
                certificate["launcher_binary_sha256"], "2" * 64,
            )
            for direction in PRIMARY_DIRECTIONS:
                self.assertEqual(
                    certificate["run_a_directions"][direction]["result"], "passed",
                )
                self.assertEqual(
                    certificate["run_b_directions"][direction]["result"], "passed",
                )
                self.assertEqual(
                    certificate["run_a_directions"][direction]["cleanup"], "clean",
                )
                self.assertEqual(
                    certificate["run_b_directions"][direction]["receiver_data_phase"],
                    "observed",
                )

    def test_allowlisted_divergent_fields_accepted(self):
        with tempfile.TemporaryDirectory() as directory:
            run_a, run_b, _ = _build_two_bundles(Path(directory))
            certificate = verify_certificate(run_a, run_b)
            divergent = certificate["allowlisted_divergent_fields"]
            self.assertIn("run_id", divergent)
            self.assertIn("run_identity_sha256", divergent)
            self.assertNotIn("source_commit", divergent)


class CertificateNegativeTests(unittest.TestCase):
    def _certificate_with(self, mutate):
        with tempfile.TemporaryDirectory() as directory:
            run_a, run_b, _ = _build_two_bundles(Path(directory))
            mutate(run_a, run_b)
            return verify_certificate(run_a, run_b)

    def _assert_failure_mentions(self, certificate: dict[str, object], substring: str) -> None:
        joined = "\n".join(certificate["failures"])
        self.assertIn(substring, joined, msg=joined)

    def test_same_run_id_rejected(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            payload_a = json.loads((run_a / "run-identity.json").read_text())
            payload_b = json.loads((run_b / "run-identity.json").read_text())
            payload_b["run_id"] = payload_a["run_id"]
            payload_b = _finalize_run_identity(payload_b)
            (run_b / "run-identity.json").write_text(
                json.dumps(payload_b, separators=(",", ":")), encoding="utf-8",
            )
            _refinalize(run_b)
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(certificate, "run_a.run_id == run_b.run_id")

    def test_source_commit_mismatch_rejected(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            payload_b = json.loads((run_b / "run-identity.json").read_text())
            payload_b["source_commit"] = "1" + "0" * 39
            payload_b = _finalize_run_identity(payload_b)
            (run_b / "run-identity.json").write_text(
                json.dumps(payload_b, separators=(",", ":")), encoding="utf-8",
            )
            _refinalize(run_b)
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(certificate, "provenance mismatch: 'source_commit'")

    def test_launcher_digest_mismatch_rejected(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            payload_b = json.loads((run_b / "run-identity.json").read_text())
            payload_b["launcher_binary_sha256"] = "9" * 64
            payload_b = _finalize_run_identity(payload_b)
            (run_b / "run-identity.json").write_text(
                json.dumps(payload_b, separators=(",", ":")), encoding="utf-8",
            )
            _refinalize(run_b)
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(
            certificate, "provenance mismatch: 'launcher_binary_sha256'",
        )

    def test_reference_lock_digest_mismatch_rejected(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            payload_b = json.loads((run_b / "run-identity.json").read_text())
            payload_b["reference_lock_sha256"] = "8" * 64
            payload_b = _finalize_run_identity(payload_b)
            (run_b / "run-identity.json").write_text(
                json.dumps(payload_b, separators=(",", ":")), encoding="utf-8",
            )
            _refinalize(run_b)
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(
            certificate, "provenance mismatch: 'reference_lock_sha256'",
        )

    def test_copied_observation_digest_rejected(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            for direction in PRIMARY_DIRECTIONS:
                src = run_a / "observations" / f"{direction}.json"
                dst = run_b / "observations" / f"{direction}.json"
                shutil.copyfile(src, dst)
                os.chmod(dst, 0o600)
            _refinalize(run_b)
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(certificate, "identical observation_sha256")

    def test_copied_correlation_message_id_rejected(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            payload_a = json.loads((run_a / "observations" / "i2pr-to-java-ipv4.json").read_text())
            nonce = payload_a["run_correlation"]["delivery_status_message_id"]
            payload_b = json.loads((run_b / "observations" / "i2pr-to-java-ipv4.json").read_text())
            payload_b["run_correlation"]["delivery_status_message_id"] = nonce
            (run_b / "observations" / "i2pr-to-java-ipv4.json").write_text(
                json.dumps(payload_b, sort_keys=True, separators=(",", ":")) + "\n",
                encoding="utf-8",
            )
            os.chmod(run_b / "observations" / "i2pr-to-java-ipv4.json", 0o600)
            _refinalize(run_b)
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(certificate, "identical correlation message id")

    def test_missing_direction_rejected(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            (run_b / "directions" / "i2pr-to-java-ipv4.json").unlink()
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(certificate, "missing directions/i2pr-to-java-ipv4.json")

    def test_one_rejected_direction_prevents_certificate(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            path = run_b / "directions" / "i2pr-to-java-ipv4.json"
            payload = json.loads(path.read_text())
            payload["actual_typed_result"] = "rejected"
            payload["reason_code"] = "i2pr-initiator-handshake-failed"
            path.write_text(
                json.dumps(payload, sort_keys=True, separators=(",", ":")) + "\n",
                encoding="utf-8",
            )
            os.chmod(path, 0o600)
            _refinalize(run_b)
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(certificate, "actual_typed_result != passed")

    def test_receiver_handshake_only_observation_prevents_certificate(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            path = run_b / "observations" / "i2pr-to-java-ipv4.json"
            payload = json.loads(path.read_text())
            payload["sides"]["java_i2p"]["levels"]["frame_authenticated_and_decrypted"]["state"] = "not-observed"
            payload["sides"]["java_i2p"]["levels"]["i2np_message_decoded"]["state"] = "not-observed"
            from observation import finalize_observation
            finalize_observation("java_i2p", payload["sides"]["java_i2p"])
            payload["observation_sha256"] = ""
            import hashlib as _hashlib
            payload["observation_sha256"] = _hashlib.sha256(
                json.dumps({**payload, "observation_sha256": ""}, sort_keys=True, separators=(",", ":")).encode()
            ).hexdigest()
            path.write_text(
                json.dumps(payload, sort_keys=True, separators=(",", ":")) + "\n",
                encoding="utf-8",
            )
            os.chmod(path, 0o600)
            _refinalize(run_b)
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(
            certificate, "receiver java_i2p does not satisfy data-phase predicate",
        )

    def test_cleanup_failure_prevents_certificate(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            path = run_b / "cleanup" / "i2pr-to-java-ipv4.json"
            payload = json.loads(path.read_text())
            payload["topology_cleanup"] = "failed"
            payload["final_result"] = "failed"
            path.write_text(
                json.dumps(payload, sort_keys=True, separators=(",", ":")) + "\n",
                encoding="utf-8",
            )
            os.chmod(path, 0o600)
            _refinalize(run_b)
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(certificate, "cleanup_result='failed'")

    def test_parent_network_change_prevents_certificate(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            path = run_b / "attestations" / "i2pr-to-java-ipv4.json"
            payload = json.loads(path.read_text())
            payload["parent_network_state_unchanged"] = False
            path.write_text(
                json.dumps(payload, sort_keys=True, separators=(",", ":")) + "\n",
                encoding="utf-8",
            )
            os.chmod(path, 0o600)
            _refinalize(run_b)
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(certificate, "parent_network_state_unchanged is False")

    def test_raw_diagnostics_file_prevents_certificate(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            (run_b / "manifest.json").unlink()
            (run_b / "manifest.sha256").unlink()
            (run_b / "raw.log").write_text("transcript-leak\n", encoding="utf-8")
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        joined = "\n".join(certificate["failures"])
        self.assertTrue(
            "forbidden raw-diagnostics file" in joined
            or "raw.log" in joined,
            msg=joined,
        )

    def test_undeclared_bundle_path_prevents_certificate(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            extras = run_b / "extras"
            extras.mkdir()
            (extras / "secret.json").write_text("{}", encoding="utf-8")
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(certificate, "unexpected file in staging directory")

    def test_support_topology_mismatch_rejected(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            payload_b = json.loads((run_b / "run-identity.json").read_text())
            payload_b["topology_kind"] = "privileged-dual-netns-veth"
            payload_b = _finalize_run_identity(payload_b)
            (run_b / "run-identity.json").write_text(
                json.dumps(payload_b, separators=(",", ":")), encoding="utf-8",
            )
            _refinalize(run_b)
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(
            certificate, "provenance mismatch: 'topology_kind'",
        )

    def test_unauthorized_divergent_field_rejected(self):
        def mutate(run_a: Path, run_b: Path) -> None:
            payload_b = json.loads((run_b / "run-identity.json").read_text())
            payload_b["launcher_build_profile"] = "broken-profile"
            payload_b = _finalize_run_identity(payload_b)
            (run_b / "run-identity.json").write_text(
                json.dumps(payload_b, separators=(",", ":")), encoding="utf-8",
            )
            _refinalize(run_b)
        certificate = self._certificate_with(mutate)
        self.assertFalse(certificate["verified"])
        self._assert_failure_mentions(
            certificate, "provenance mismatch: 'launcher_build_profile'",
        )

    def test_no_bundles_returns_failure(self):
        with tempfile.TemporaryDirectory() as directory:
            certificate = verify_certificate(
                Path(directory) / "missing-a", Path(directory) / "missing-b",
            )
            self.assertFalse(certificate["verified"])
            joined = "\n".join(certificate["failures"])
            self.assertIn("missing-a", joined)


if __name__ == "__main__":
    unittest.main()
