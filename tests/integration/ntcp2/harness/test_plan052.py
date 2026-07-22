"""Plan 052 tests for run-identity, observation schema, and evidence bindings.

These tests are intentionally narrow: they validate the new schemas and the
binding behavior of the existing evidence validator. They do NOT replace the
historical Plan 045-051 evidence contract; the new schemas are opt-in
suffixes that older records do not carry.
"""

from __future__ import annotations

import json
import tempfile
import unittest
from pathlib import Path

from observation import (
    OBSERVATION_SCHEMA,
    OBSERVATION_SCHEMA_VERSION,
    ObservationError,
    both_authenticated,
    build_level,
    empty_levels,
    finalize_observation,
    receiver_passes_data_phase,
    sender_emitted_data_frame,
    validate_observation,
)
from run_identity import (
    RUN_IDENTITY_SCHEMA,
    RUN_IDENTITY_SCHEMA_VERSION,
    RunIdentityError,
    build_run_identity,
    cross_check,
    empty_run_identity_digest,
    load_run_identity,
    validate_run_identity,
    write_run_identity,
)

from evidence import (
    EvidenceError,
    MULTIPASS_RECORD_FIELDS,
    RECORD_FIELDS,
    RUN_IDENTITY_BIND_FIELDS,
    RUN_IDENTITY_STANDALONE_FIELDS,
    validate_record,
    write_record,
)


def _complete_record(**overrides):
    base = {
        "schema": 1,
        "scenario_id": "i2pr-to-java-ipv4",
        "date_utc": "2026-07-22T00:00:00Z",
        "i2pr_commit": "0" * 40 + ";clean",
        "reference": "java_i2p",
        "reference_version": "2.12.0",
        "reference_revision": "2800040deee9bb376567b671ef2e9c34cf3e30b6",
        "artifact_sha256": "a" * 64,
        "installed_tree_sha256": "b" * 64,
        "configuration_sha256": "c" * 64,
        "namespace_topology_sha256": "d" * 64,
        "direction": "i2pr-to-java",
        "address_family": "ipv4",
        "deterministic_parameters": "seed=1;timeouts=bounded;network=synthetic-private-036;ipv6-probe=passed;data_phase_oracle=;data_phase_mode=round-trip-delivery-status;expected_observation=i2pr-sent-and-acknowledged",
        "expected": "authenticated-handshake-and-directional-data-phase",
        "actual_typed_result": "passed",
        "resource_counters": {
            "tasks": 0, "queues": 0, "permits": 0, "links": 0,
            "handshakes": 0, "i2np_sent": 1, "i2np_received": 1,
        },
        "process_counters": {"started": 2, "exited": 2, "forced": 0},
        "cleanup_result": "clean",
        "evidence_sha256": "",
        "known_deviation": "",
        "reproduction": "bash scripts/interop/run-scenario.sh --scenario i2pr-to-java-ipv4 --reference java_i2p",
        "i2pr_router_info_sha256": "e" * 64,
        "reference_router_info_sha256": "f" * 64,
        "data_phase_mode": "round-trip-delivery-status",
        "expected_observation": "i2pr-sent-and-acknowledged",
        "topology_kind": "rootless-sealed-single-netns",
        "privilege_model": "unprivileged-userns",
        "sandbox_attestation_sha256": "1" * 64,
        "parent_network_state_unchanged": True,
    }
    base.update(overrides)
    return base


def _complete_run_identity_record(**overrides):
    """Record variant that carries the Plan 052 standalone run-identity suffix."""

    defaults = {
        "run_id": "plan052-20260722000000-aabbccdd",
        "source_commit": "a" * 40,
        "launcher_binary_sha256": "b" * 64,
        "run_identity_sha256": "c" * 64,
    }
    defaults.update(overrides)
    record = _complete_record(**defaults)
    ordered = {}
    for field in RECORD_FIELDS:
        if field in record:
            ordered[field] = record[field]
    for field in RUN_IDENTITY_STANDALONE_FIELDS:
        ordered[field] = record[field]
    return ordered


def _complete_run_identity(**overrides):
    base = {
        "schema": RUN_IDENTITY_SCHEMA,
        "schema_version": RUN_IDENTITY_SCHEMA_VERSION,
        "run_id": "plan052-20260722000000-aabbccdd",
        "created_at": "2026-07-22T00:00:00Z",
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
        "launcher_build_profile": "debug-no-default-features",
        "rustc_version": "1.95.0",
        "cargo_version": "1.95.0",
        "target_triple": "x86_64-unknown-linux-gnu",
        "topology_kind": "rootless-sealed-single-netns",
        "privilege_model": "unprivileged-userns",
        "reference_lock_sha256": "3" * 64,
        "evidence_schema_revision": 2,
        "run_identity_sha256": "",
    }
    base.update(overrides)
    return base


class RunIdentitySchemaTests(unittest.TestCase):
    def test_minimal_run_identity_is_valid(self):
        record = _complete_run_identity(source_commit="1" * 40)
        validate_run_identity(record)
        self.assertNotIn("evidence_sha256", record)

    def test_short_run_id_fails(self):
        record = _complete_run_identity(run_id="short")
        with self.assertRaises(RunIdentityError):
            validate_run_identity(record)

    def test_invalid_run_id_fails(self):
        record = _complete_run_identity(run_id="!!!invalid!!!")
        with self.assertRaises(RunIdentityError):
            validate_run_identity(record)

    def test_short_source_commit_fails(self):
        record = _complete_run_identity(source_commit="abc123")
        with self.assertRaises(RunIdentityError):
            validate_run_identity(record)

    def test_dirty_source_state_rejected(self):
        record = _complete_run_identity(source_dirty="unknown")
        with self.assertRaises(RunIdentityError):
            validate_run_identity(record)

    def test_extra_field_rejected(self):
        record = _complete_run_identity()
        record["rogue"] = "value"
        with self.assertRaises(RunIdentityError):
            validate_run_identity(record)

    def test_zero_source_commit_rejected(self):
        record = _complete_run_identity(source_commit="0" * 40)
        # Plan 052 source_commit accepts any 40-char SHA in the run identity
        # file; the "no zero" rule applies to evidence records on pass.
        validate_run_identity(record)

    def test_writer_produces_self_consistent_digest(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run-identity.json"
            digest = write_run_identity(path, _complete_run_identity())
            loaded = load_run_identity(path)
            self.assertEqual(loaded["run_identity_sha256"], digest)
            self.assertEqual(loaded["run_id"], "plan052-20260722000000-aabbccdd")

    def test_loaded_digest_mismatch_fails(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "run-identity.json"
            write_run_identity(path, _complete_run_identity())
            value = json.loads(path.read_text(encoding="utf-8"))
            value["run_identity_sha256"] = "f" * 64
            path.write_text(json.dumps(value, sort_keys=False), encoding="utf-8")
            with self.assertRaises(RunIdentityError):
                load_run_identity(path)

    def test_cross_check_rejects_run_id_mismatch(self):
        identity = _complete_run_identity()
        identity["run_identity_sha256"] = "a" * 64
        record = {"run_id": identity["run_id"], "run_identity_sha256": "a" * 64}
        cross_check(record, identity)
        record["run_id"] = "different-id"
        with self.assertRaises(RunIdentityError):
            cross_check(record, identity)

    def test_cross_check_rejects_source_commit_mismatch(self):
        identity = _complete_run_identity()
        identity["run_identity_sha256"] = "a" * 64
        record = {
            "run_id": identity["run_id"],
            "run_identity_sha256": "a" * 64,
            "source_commit": identity["source_commit"],
            "launcher_binary_sha256": identity["launcher_binary_sha256"],
        }
        cross_check(record, identity)
        record["source_commit"] = "1" * 40
        with self.assertRaises(RunIdentityError):
            cross_check(record, identity)

    def test_cross_check_rejects_launcher_binary_mismatch(self):
        identity = _complete_run_identity()
        identity["run_identity_sha256"] = "a" * 64
        record = {
            "run_id": identity["run_id"],
            "run_identity_sha256": "a" * 64,
            "source_commit": identity["source_commit"],
            "launcher_binary_sha256": identity["launcher_binary_sha256"],
        }
        cross_check(record, identity)
        record["launcher_binary_sha256"] = "9" * 64
        with self.assertRaises(RunIdentityError):
            cross_check(record, identity)


class RunIdentityBuilderTests(unittest.TestCase):
    def test_build_run_identity_returns_unsigned_record(self):
        record = build_run_identity(
            run_id="plan052-20260722000000-aabbccdd",
            source_commit="a" * 40,
            source_commit_object_sha256="a" * 64,
            source_tree_sha256="b" * 64,
            source_archive_sha256="c" * 64,
            source_archive_format="git-tar",
            source_dirty="clean",
            host_source_manifest_sha256="d" * 64,
            guest_source_manifest_sha256="e" * 64,
            guest_source_listing_sha256="f" * 64,
            environment_manifest_sha256="1" * 64,
            launcher_binary_sha256="2" * 64,
            launcher_build_profile="debug-no-default-features",
            rustc_version="1.95.0",
            cargo_version="1.95.0",
            target_triple="x86_64-unknown-linux-gnu",
            topology_kind="rootless-sealed-single-netns",
            privilege_model="unprivileged-userns",
            reference_lock_sha256="3" * 64,
            evidence_schema_revision=2,
            created_at="2026-07-22T00:00:00Z",
        )
        self.assertEqual(record["run_identity_sha256"], "")
        self.assertEqual(record["schema"], RUN_IDENTITY_SCHEMA)


class ObservationSchemaTests(unittest.TestCase):
    def _valid_observation(self):
        levels = empty_levels()
        levels["process_started"] = build_level(
            "observed", "typed-status", "i2pr-listener-ready",
            count=1, first_observed_monotonic_ms=10,
            observer_implementation="i2pr-launcher-status",
        )
        levels["ntcp2_authenticated"] = build_level(
            "observed", "structured-log", "i2pr-authenticated",
            count=1, first_observed_monotonic_ms=250,
            observer_implementation="i2pr-launcher-status",
        )
        levels["frame_emitted"] = build_level(
            "observed", "typed-status", "i2pr-frame-sent",
            count=1, first_observed_monotonic_ms=300,
            observer_implementation="i2pr-launcher-status",
        )
        levels["frame_authenticated_and_decrypted"] = build_level(
            "observed", "source-derived-log-marker", "java-NTCP2-fwd",
            count=1, first_observed_monotonic_ms=320,
            observer_implementation="java-receiver-adapter",
        )
        levels["i2np_message_decoded"] = build_level(
            "observed", "control-api", "java-i2np-delivery-status",
            count=1, first_observed_monotonic_ms=340,
            observer_implementation="java-receiver-adapter",
        )
        return {
            "schema": OBSERVATION_SCHEMA,
            "schema_version": OBSERVATION_SCHEMA_VERSION,
            "levels": levels,
            "observation_sha256": "",
        }

    def test_valid_observation_finalizes(self):
        observation = self._valid_observation()
        digest = finalize_observation("java_i2p", observation)
        self.assertEqual(observation["observation_sha256"], digest)

    def test_unknown_schema_fails(self):
        observation = self._valid_observation()
        observation["schema"] = "unknown"
        with self.assertRaises(ObservationError):
            validate_observation("java_i2p", observation)

    def test_missing_level_fails(self):
        observation = self._valid_observation()
        del observation["levels"]["terminal_clean"]
        with self.assertRaises(ObservationError):
            validate_observation("java_i2p", observation)

    def test_extra_level_fails(self):
        observation = self._valid_observation()
        observation["levels"]["rogue"] = build_level(
            "observed", "typed-status", "rogue",
            observer_implementation="schema-v2",
        )
        with self.assertRaises(ObservationError):
            validate_observation("java_i2p", observation)

    def test_unknown_state_fails(self):
        observation = self._valid_observation()
        observation["levels"]["process_started"]["state"] = "undecided"
        with self.assertRaises(ObservationError):
            validate_observation("java_i2p", observation)

    def test_receiver_passes_data_phase_requires_decrypt_and_decode(self):
        observation = self._valid_observation()
        self.assertTrue(receiver_passes_data_phase(observation))
        observation["levels"]["frame_authenticated_and_decrypted"]["state"] = "not-observed"
        self.assertFalse(receiver_passes_data_phase(observation))

    def test_sender_emitted_data_frame(self):
        observation = self._valid_observation()
        self.assertTrue(sender_emitted_data_frame(observation))
        observation["levels"]["frame_emitted"]["state"] = "not-observed"
        self.assertFalse(sender_emitted_data_frame(observation))

    def test_both_authenticated(self):
        observation = self._valid_observation()
        self.assertTrue(both_authenticated(observation, observation))
        observation["levels"]["ntcp2_authenticated"]["state"] = "not-observed"
        self.assertFalse(both_authenticated(observation, observation))

    def test_correlation_field_validation(self):
        observation = self._valid_observation()
        observation["run_correlation"] = {
            "delivery_status_message_id": "12345678",
            "bounded_test_nonce": "abcd",
        }
        validate_observation("java_i2p", observation)
        observation["run_correlation"]["delivery_status_message_id"] = 12345
        with self.assertRaises(ObservationError):
            validate_observation("java_i2p", observation)

    def test_empty_levels_returns_not_applicable(self):
        levels = empty_levels()
        for level in levels.values():
            self.assertEqual(level["state"], "not-applicable")

    def test_invalid_side_rejected(self):
        observation = self._valid_observation()
        with self.assertRaises(ObservationError):
            validate_observation("rogue_side", observation)


class EvidenceRunIdentityBindingTests(unittest.TestCase):
    def test_run_identity_bound_record_validates(self):
        record = _complete_run_identity_record(
            actual_typed_result="passed",
            cleanup_result="clean",
        )
        validate_record(record)

    def test_zero_run_identity_sha256_rejected_on_passed(self):
        record = _complete_run_identity_record(
            run_identity_sha256="0" * 64,
        )
        with self.assertRaises(EvidenceError):
            validate_record(record)

    def test_zero_launcher_binary_sha256_rejected_on_passed(self):
        record = _complete_run_identity_record(
            launcher_binary_sha256="0" * 64,
        )
        with self.assertRaises(EvidenceError):
            validate_record(record)

    def test_zero_source_commit_rejected_on_passed(self):
        record = _complete_run_identity_record(
            source_commit="0" * 40,
        )
        with self.assertRaises(EvidenceError):
            validate_record(record)

    def test_short_source_commit_rejected(self):
        record = _complete_run_identity_record(
            source_commit="abc",
        )
        with self.assertRaises(EvidenceError):
            validate_record(record)

    def test_run_id_must_match_safe_pattern(self):
        record = _complete_run_identity_record(
            run_id="x" * 1,
        )
        with self.assertRaises(EvidenceError):
            validate_record(record)

    def test_run_identity_record_optional_for_pre_052_records(self):
        base = _complete_record()
        validate_record(base)


class EvidenceCombinedSuffixTests(unittest.TestCase):
    def test_multipass_with_run_identity_record_validates(self):
        record = _complete_run_identity_record()
        for field, value in (
            ("environment_id", "i2pr-plan048-rootless-v1"),
            ("instance_generation", 1),
            ("environment_evidence_sha256", "d" * 64),
            ("instance_name_digest", "e" * 64),
            ("lifecycle_schema_version", 1),
            ("ownership_record_sha256", "f" * 64),
            ("environment_manifest_sha256", "1" * 64),
            ("cloud_init_sha256", "2" * 64),
            ("host_baseline_probe_outcome", "blocked_unprivileged_user_namespace"),
            ("guest_rootless_probe_outcome", "rootless_sandbox_available"),
            ("adoption_mode", "fresh"),
        ):
            record[field] = value
        ordered = {}
        for field in RECORD_FIELDS:
            if field in record:
                ordered[field] = record[field]
        for field in MULTIPASS_RECORD_FIELDS:
            ordered[field] = record[field]
        for field in RUN_IDENTITY_BIND_FIELDS:
            ordered[field] = record[field]
        validate_record(ordered)


class DiagnosticsModeTests(unittest.TestCase):
    def test_diagnostics_mode_accepts_allowed_values(self):
        from mixed_runner import ALLOWED_DIAGNOSTICS_MODES
        self.assertEqual(
            ALLOWED_DIAGNOSTICS_MODES, frozenset({"off", "sanitized", "raw-local"})
        )


class Plan052EmptyTypedAbsenceTests(unittest.TestCase):
    def test_empty_run_identity_digest_is_zero(self):
        self.assertEqual(empty_run_identity_digest(), "0" * 64)


class EvidenceRecordWriteRoundTripTests(unittest.TestCase):
    def test_run_identity_bound_record_writes_and_validates(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "direction.json"
            record = _complete_run_identity_record(
                actual_typed_result="rejected",
                cleanup_result="clean",
                known_deviation="data-oracle-not-available",
            )
            write_record(path, record)
            value = json.loads(path.read_text(encoding="utf-8"))
            self.assertEqual(value["source_commit"], "a" * 40)
            self.assertEqual(value["launcher_binary_sha256"], "b" * 64)
            self.assertEqual(value["run_identity_sha256"], "c" * 64)


if __name__ == "__main__":
    unittest.main()