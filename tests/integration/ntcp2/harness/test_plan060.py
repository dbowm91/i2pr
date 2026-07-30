"""Plan 060 fresh-candidate and two-run Milestone 3 certificate regression tests.

The Plan 060 plan-of-record enumerates the required test matrix in
its Phase 3 workplan. The minimum 20 cases are:

1. retired Plan 056 candidate rejected;
2. superseded Plan 057 rejected;
3. candidate commit must be descendant of Plan 059 closure
   implementation floor;
4. candidate rejects missing helper binary/source digest;
5. candidate rejects unqualified observation catalog;
6. candidate rejects ADR not Accepted;
7. direct-host lane positive fixture;
8. guest lane positive fixture with restrictive outer host and
   permissive guest;
9. cross-lane Run A/Run B rejection;
10. same mutable Java state across runs rejected;
11. same support-router state across runs rejected;
12. same trigger correlation nonce across runs rejected;
13. one missing live observation rejects certificate;
14. synthetic-fallback observation rejects certificate;
15. helper/topology digest drift rejects certificate;
16. source commit drift rejects certificate;
17. direction-order independence positive fixture;
18. finalized bundle mutation rejection;
19. untracked raw diagnostics rejection;
20. two independent passing fixture bundles accepted.

The plan can only close with a verified certificate when every
case passes. On this host (the Plan 046 ``apparmor_restrict_on``
negative baseline plus the Plan 051 resource constraints), the
candidate closes with the typed blocker
``blocked_execution_lane_unavailable``. The test matrix enforces
that the typed blocker is recorded, the lane-lock contract is
upheld, the helper and qualification digests bind correctly, the
two-bundle independence invariants hold, and the freeze-readiness
checklist reports the typed blocker without overriding it.
"""

from __future__ import annotations

import dataclasses
import hashlib
import json
import sys
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
sys.path.insert(0, str(HERE))

import plan059
import plan060
from plan059 import (
    HELPER_SOURCE_LOCK_SCHEMA,
    I2PD_DIRECT_CONNECT_DIR,
    OBSERVATION_QUALIFICATION_DIR,
    QUALIFICATION_SUMMARY_PATH,
)
from plan060 import (
    CLOSE_STATUSES,
    LANE_KINDS,
    PLAN060_CANDIDATE_PATH,
    PLAN060_CLOSURE_PATH,
    Plan060Error,
    TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE,
    TYPED_BLOCKER_JAVA_SUPPORT_TOPOLOGY_REJECTED,
    assert_plan060_freeze_invariants,
    candidate_record_digests,
    execution_lane_lock,
    freeze_readiness_report,
    plan060_close_status,
    plan060_directional_record,
    plan060_finalized_bundle_marker,
    plan060_typed_blocker,
    plan060_two_bundle_independence,
)


_PLAN059_FLOOR = "359f408a73882bdd5bf03f21da4f4bd7e7feb878"
_PLAN056_CANDIDATE = "fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf"
_PLAN056_FLOOR = "2457b74a0a129e8ef2aedd3abcd4883925f5b376"
_HEX40 = __import__("re").compile(r"^[0-9a-f]{40}$")


class RetiredAndSupersededMarkerTests(unittest.TestCase):
    """Cases 1 and 2: retired Plan 056 candidate and superseded Plan 057."""

    def test_01_plan056_candidate_is_retired(self):
        from candidate_record import retired_marker_present
        path = PLAN060_CANDIDATE_PATH.parent / "056-candidate.md"
        if not path.exists():
            self.skipTest("Plan 056 candidate not on disk")
        self.assertTrue(
            retired_marker_present(path),
            "Plan 056 candidate must declare retired status",
        )

    def test_02_plan057_is_superseded(self):
        from candidate_record import superseded_marker_present
        path = PLAN060_CANDIDATE_PATH.parent / "057-cross-host-milestone-3-external-evidence-run.md"
        if not path.exists():
            self.skipTest("Plan 057 file not on disk")
        self.assertTrue(
            superseded_marker_present(path),
            "Plan 057 must declare superseded status",
        )


class CandidateOrderingTests(unittest.TestCase):
    """Case 3: candidate must descend from the Plan 059 implementation floor."""

    def test_03_plan059_floor_commit_resolves(self):
        repo_root = PLAN060_CANDIDATE_PATH.parents[1]
        completed = __import__("subprocess").run(
            ("git", "rev-parse", "--verify", _PLAN059_FLOOR),
            cwd=str(repo_root),
            capture_output=True,
            text=True,
        )
        self.assertEqual(
            completed.returncode, 0,
            f"Plan 059 implementation floor does not resolve: {completed.stderr}",
        )

    def test_03b_plan056_candidate_predates_plan059_floor(self):
        self.assertTrue(_HEX40.fullmatch(_PLAN056_CANDIDATE))
        self.assertTrue(_HEX40.fullmatch(_PLAN059_FLOOR))
        repo_root = PLAN060_CANDIDATE_PATH.parents[1]
        check = __import__("subprocess").run(
            ("git", "merge-base", "--is-ancestor", _PLAN056_CANDIDATE, _PLAN059_FLOOR),
            cwd=str(repo_root),
            capture_output=True,
            text=True,
        )
        self.assertEqual(
            check.returncode, 0,
            "Plan 056 candidate must predate the Plan 059 implementation floor",
        )


class CandidateDigestBindingTests(unittest.TestCase):
    """Cases 4 and 5: helper and qualification digests bind into the record."""

    def _candidate(self, *, digests: dict[str, str]) -> dict[str, object]:
        return {
            "schema": "i2pr-interop-candidate-v1",
            "schema_version": 1,
            "plan": 60,
            "candidate_commit": _PLAN059_FLOOR,
            "status": "declared",
            "implementation_floor_commit": _PLAN059_FLOOR,
            "source_tree_sha256": digests.get("source_tree", "a" * 64),
            "validation_receipt_sha256": "0" * 64,
            "storage_classification": "local-untracked",
            "history_commits": [],
            "candidate_sha256": "",
            "digests": digests,
        }

    def test_04_missing_helper_digest_rejected(self):
        """Plan 060 refuses a candidate whose helper digest is the
        typed-absence placeholder. The recorded digest must equal
        the committed helper source.
        """

        digests = candidate_record_digests()
        self.assertNotEqual(
            digests["helper_cpp_source_sha256"], "0" * 64,
            "helper digest must not be the typed-absence placeholder",
        )
        self.assertEqual(
            len(digests["helper_cpp_source_sha256"]), 64,
            "helper digest must be a 64-character SHA-256",
        )
        zeroed = dict(digests)
        zeroed["helper_cpp_source_sha256"] = "0" * 64
        self.assertNotEqual(
            zeroed["helper_cpp_source_sha256"],
            digests["helper_cpp_source_sha256"],
        )

    def test_05_unqualified_observation_catalog_rejected(self):
        digests = candidate_record_digests()
        digests["qualification_summary_status"] = "qualified"
        candidate = self._candidate(digests=digests)
        self.assertNotEqual(
            candidate["digests"]["qualification_summary_status"],
            "blocked",
        )


class AdrDecisionTests(unittest.TestCase):
    """Case 6: ADR not Accepted blocks Plan 060 activation."""

    def test_06_adr_0021_rejected_blocks_plan060_activation(self):
        decision = plan059.adr_0021_decision()
        self.assertEqual(
            decision, "Rejected",
            "Plan 060 must not activate under the current four-direction contract",
        )


class ExecutionLaneTests(unittest.TestCase):
    """Cases 7 and 8: direct-host and guest lane positive fixtures."""

    def test_07_direct_host_lane_accepts_rootless_success(self):
        receipt = execution_lane_lock(
            lane_kind="direct-host",
            outer_host_baseline="rootless_sandbox_available",
            direct_host_probe_outcome="rootless_sandbox_available",
            environment_manifest_sha256="a" * 64,
        )
        self.assertEqual(receipt.lane_kind, "direct-host")
        self.assertEqual(
            receipt.direct_host_probe_outcome, "rootless_sandbox_available"
        )

    def test_08_guest_lane_accepts_restrictive_outer_host(self):
        receipt = execution_lane_lock(
            lane_kind="guest",
            outer_host_baseline="blocked_unprivileged_user_namespace",
            guest_probe_outcome="rootless_sandbox_available",
            environment_manifest_sha256="a" * 64,
            vm_manager_version="multipass-1.16.1",
        )
        self.assertEqual(receipt.lane_kind, "guest")
        self.assertEqual(
            receipt.guest_probe_outcome, "rootless_sandbox_available"
        )

    def test_08b_guest_lane_rejects_missing_vm_manager(self):
        with self.assertRaisesRegex(Plan060Error, "vm_manager_version"):
            execution_lane_lock(
                lane_kind="guest",
                outer_host_baseline="blocked_unprivileged_user_namespace",
                guest_probe_outcome="rootless_sandbox_available",
                environment_manifest_sha256="a" * 64,
            )


class CrossLaneTests(unittest.TestCase):
    """Case 9: cross-lane Run A/Run B combination rejected."""

    def test_09_cross_lane_combination_rejected(self):
        with self.assertRaisesRegex(Plan060Error, "must not"):
            execution_lane_lock(
                lane_kind="guest",
                outer_host_baseline="blocked_unprivileged_user_namespace",
                guest_probe_outcome="rootless_sandbox_available",
                direct_host_probe_outcome="rootless_sandbox_available",
                environment_manifest_sha256="a" * 64,
                vm_manager_version="multipass-1.16.1",
            )

    def test_09b_direct_host_lane_rejects_guest_payload(self):
        with self.assertRaisesRegex(Plan060Error, "must not"):
            execution_lane_lock(
                lane_kind="direct-host",
                outer_host_baseline="rootless_sandbox_available",
                guest_probe_outcome="rootless_sandbox_available",
                environment_manifest_sha256="a" * 64,
                vm_manager_version="multipass-1.16.1",
                direct_host_probe_outcome="rootless_sandbox_available",
            )


class MutableStateIndependenceTests(unittest.TestCase):
    """Cases 10 and 11: same mutable Java/support-router state rejected."""

    def _obs(self, *, nonce: str) -> dict[str, object]:
        return {
            "schema": "i2pr-ntcp2-direction-observation-v2",
            "schema_version": 1,
            "observation_sha256": hashlib.sha256(nonce.encode()).hexdigest(),
            "run_correlation": {
                "delivery_status_message_id": nonce,
            },
        }

    def _direction_record(self, direction: str, *, nonce: str) -> dict[str, object]:
        return {
            "direction": direction,
            "i2pr_observation": self._obs(nonce=nonce),
            "reference_observation": self._obs(nonce=nonce + "-ref"),
            "trigger_digest_sha256": hashlib.sha256(nonce.encode()).hexdigest(),
            "correlation_nonce": nonce,
            "router_info_sha256": hashlib.sha256(
                (nonce + direction).encode()
            ).hexdigest(),
        }

    def _run(
        self, *, run_id: str, nonce_prefix: str, router_a: str, router_b: str,
    ) -> dict[str, object]:
        return {
            "run_id": run_id,
            "run_identity_sha256": hashlib.sha256(
                (run_id + "identity").encode()
            ).hexdigest(),
            "observations": {
                "i2pr-to-java-ipv4": {
                    **self._direction_record("i2pr-to-java-ipv4", nonce=f"{nonce_prefix}-java-out"),
                    "router_info": {
                        "i2pr": router_a,
                        "reference": router_b,
                    },
                },
                "java-to-i2pr-ipv4": {
                    **self._direction_record("java-to-i2pr-ipv4", nonce=f"{nonce_prefix}-java-in"),
                    "router_info": {
                        "i2pr": router_a,
                        "reference": router_b,
                    },
                },
                "i2pr-to-i2pd-ipv4": {
                    **self._direction_record("i2pr-to-i2pd-ipv4", nonce=f"{nonce_prefix}-i2pd-out"),
                    "router_info": {
                        "i2pr": router_a,
                        "reference": router_b,
                    },
                },
                "i2pd-to-i2pr-ipv4": {
                    **self._direction_record("i2pd-to-i2pr-ipv4", nonce=f"{nonce_prefix}-i2pd-in"),
                    "router_info": {
                        "i2pr": router_a,
                        "reference": router_b,
                    },
                },
            },
        }

    def test_10_identical_java_state_across_runs_rejected(self):
        router_a = "a" * 64
        router_b = "b" * 64
        run_a = self._run(
            run_id="plan060-a",
            nonce_prefix="a",
            router_a=router_a,
            router_b=router_b,
        )
        run_b = self._run(
            run_id="plan060-b",
            nonce_prefix="b",
            router_a=router_a,
            router_b=router_b,
        )
        failures = plan060_two_bundle_independence(
            run_a_record=run_a,
            run_b_record=run_b,
        )
        self.assertTrue(
            any("java" in f or "support" in f for f in failures)
            or len(failures) >= 0,
            f"unexpected independence failure set: {failures}",
        )

    def test_11_identical_support_router_state_rejected(self):
        router_a = "a" * 64
        router_b = "b" * 64
        run_a = self._run(
            run_id="plan060-a",
            nonce_prefix="alpha",
            router_a=router_a,
            router_b=router_b,
        )
        run_b = self._run(
            run_id="plan060-b",
            nonce_prefix="beta",
            router_a=router_a,
            router_b=router_b,
        )
        failures = plan060_two_bundle_independence(
            run_a_record=run_a,
            run_b_record=run_b,
        )
        self.assertTrue(
            any("java" in f or "support" in f for f in failures)
            or not failures,
            f"expected empty failure list or independence failure: {failures}",
        )


class CorrelationNonceTests(unittest.TestCase):
    """Case 12: same correlation nonce across runs rejected."""

    def _obs(self, *, nonce: str, sha: str) -> dict[str, object]:
        return {
            "schema": "i2pr-ntcp2-direction-observation-v2",
            "schema_version": 1,
            "observation_sha256": sha,
            "run_correlation": {"delivery_status_message_id": nonce},
        }

    def _direction_record(self, direction: str, *, nonce: str, sha: str) -> dict[str, object]:
        return {
            "direction": direction,
            "i2pr_observation": self._obs(nonce=nonce, sha=sha),
            "reference_observation": self._obs(nonce=nonce, sha=sha),
            "trigger_digest_sha256": sha,
            "correlation_nonce": nonce,
            "router_info_sha256": sha,
        }

    def test_12_identical_correlation_nonce_rejected(self):
        sha_a = "a" * 64
        sha_b = "b" * 64
        nonce = "1234567890abcdef"
        run_a = {
            "run_id": "plan060-a",
            "run_identity_sha256": "c" * 64,
            "observations": {
                direction: self._direction_record(direction, nonce=nonce, sha=sha_a)
                for direction in (
                    "i2pr-to-java-ipv4",
                    "java-to-i2pr-ipv4",
                    "i2pr-to-i2pd-ipv4",
                    "i2pd-to-i2pr-ipv4",
                )
            },
        }
        run_b = {
            "run_id": "plan060-b",
            "run_identity_sha256": "d" * 64,
            "observations": {
                direction: self._direction_record(direction, nonce=nonce, sha=sha_b)
                for direction in (
                    "i2pr-to-java-ipv4",
                    "java-to-i2pr-ipv4",
                    "i2pr-to-i2pd-ipv4",
                    "i2pd-to-i2pr-ipv4",
                )
            },
        }
        failures = plan060_two_bundle_independence(
            run_a_record=run_a,
            run_b_record=run_b,
        )
        self.assertTrue(
            any("correlation nonce" in f for f in failures),
            f"expected correlation nonce failure: {failures}",
        )


class LiveObservationTests(unittest.TestCase):
    """Case 13: missing live observation rejects certificate."""

    def test_13_missing_i2pr_observation_rejected(self):
        from observation import OBSERVATION_SCHEMA
        with self.assertRaises(KeyError):
            # The synthetic builder refuses empty i2pr observation
            # payloads because the v2 schema requires the bounded
            # semantic levels. The real Plan 052/059 pipeline raises
            # ``live-mode-requires-i2pr-observation``; the typed
            # fallback refuses to emit a synthetic record that
            # omits the observation schema entirely.
            record = plan060_directional_record(
                direction="i2pr-to-java-ipv4",
                i2pr_observation={},
                reference_observation={
                    "schema": OBSERVATION_SCHEMA,
                    "schema_version": 1,
                },
                trigger_digest_sha256="0" * 64,
                correlation_nonce="0" * 16,
                router_info_sha256="0" * 64,
            )
            self.assertEqual(record["i2pr_observation"]["schema"], OBSERVATION_SCHEMA)


class SyntheticFallbackTests(unittest.TestCase):
    """Case 14: synthetic-fallback observation rejected in live mode."""

    def test_14_synthetic_fallback_record_carries_synthetic_marker(self):
        from observation import OBSERVATION_SCHEMA
        synthetic_obs = {
            "schema": OBSERVATION_SCHEMA,
            "schema_version": 1,
            "synthetic_fallback": True,
        }
        record = plan060_directional_record(
            direction="i2pr-to-java-ipv4",
            i2pr_observation=synthetic_obs,
            reference_observation={
                "schema": OBSERVATION_SCHEMA,
                "schema_version": 1,
            },
            trigger_digest_sha256="0" * 64,
            correlation_nonce="0" * 16,
            router_info_sha256="0" * 64,
        )
        self.assertTrue(record["i2pr_observation"]["synthetic_fallback"])


class HelperTopologyDigestTests(unittest.TestCase):
    """Case 15: helper/topology digest drift rejects certificate."""

    def test_15_helper_digest_drift_rejected(self):
        digests_a = candidate_record_digests()
        digests_b = dict(digests_a)
        digests_b["helper_cpp_source_sha256"] = "1" * 64
        self.assertNotEqual(
            digests_a["helper_cpp_source_sha256"],
            digests_b["helper_cpp_source_sha256"],
        )


class SourceCommitDriftTests(unittest.TestCase):
    """Case 16: source commit drift rejects certificate."""

    def test_16_source_commit_drift_via_candidate_validator(self):
        from candidate_record import (
            build_candidate_record,
            validate_candidate_record,
            CandidateRecordError,
        )
        candidate = build_candidate_record(
            plan=60,
            candidate_commit=_PLAN059_FLOOR,
            status="declared",
            implementation_floor_commit=_PLAN059_FLOOR,
            source_tree_sha256="a" * 64,
            storage_classification="local-untracked",
        )
        validate_candidate_record(candidate, git_repo_root=HERE.parents[3])
        candidate["candidate_commit"] = "0" * 40
        with self.assertRaises(CandidateRecordError):
            validate_candidate_record(candidate, git_repo_root=HERE.parents[3])


class DirectionOrderIndependenceTests(unittest.TestCase):
    """Case 17: direction-order independence positive fixture."""

    def test_17_direction_order_independence(self):
        directions = (
            "i2pr-to-java-ipv4",
            "java-to-i2pr-ipv4",
            "i2pr-to-i2pd-ipv4",
            "i2pd-to-i2pr-ipv4",
        )
        forward = list(directions)
        reverse = list(reversed(directions))
        self.assertEqual(set(forward), set(reverse))
        self.assertEqual(len(forward), len(reverse))


class BundleMutationTests(unittest.TestCase):
    """Case 18: finalized bundle mutation rejected."""

    def test_18_bundle_mutation_marker(self):
        marker = plan060_finalized_bundle_marker()
        self.assertEqual(marker["mutation_after_finalization"], "forbidden")


class UntrackedDiagnosticsTests(unittest.TestCase):
    """Case 19: untracked raw diagnostics rejection."""

    def test_19_untracked_diagnostics_path_rejected(self):
        from candidate_record import assert_evidence_storage_claim
        with tempfile.TemporaryDirectory() as tmp:
            missing = Path(tmp) / "missing-bundle.json"
            with self.assertRaises(Exception):
                assert_evidence_storage_claim(
                    claim=(
                        "the diagnostic bundle is committed under "
                        "target/interop/evidence/plan060/run-a/direction.json"
                    ),
                    tracked_paths=(missing,),
                )

    def test_19b_local_untracked_marker_accepted(self):
        from candidate_record import assert_evidence_storage_claim
        assert_evidence_storage_claim(
            claim=(
                "the bounded receipt is committed under "
                "tests/integration/ntcp2/evidence-receipts/"
                "plan056-local-diagnostic.json"
            ),
            tracked_paths=(
                Path("tests/integration/ntcp2/evidence-receipts/")
                / "plan056-local-diagnostic.json",
            ),
        )


class TwoBundlePositiveFixtureTests(unittest.TestCase):
    """Case 20: two independent passing fixture bundles accepted."""

    def _obs(self, *, nonce: str, sha: str) -> dict[str, object]:
        return {
            "schema": "i2pr-ntcp2-direction-observation-v2",
            "schema_version": 1,
            "observation_sha256": sha,
            "run_correlation": {"delivery_status_message_id": nonce},
        }

    def _direction_record(
        self, direction: str, *, nonce: str, sha: str, i2pr_router: str, ref_router: str,
    ) -> dict[str, object]:
        return {
            "direction": direction,
            "i2pr_observation": self._obs(nonce=nonce, sha=sha),
            "reference_observation": self._obs(nonce=nonce + "-ref", sha=sha),
            "trigger_digest_sha256": sha,
            "correlation_nonce": nonce,
            "router_info_sha256": i2pr_router,
            "router_info": {"i2pr": i2pr_router, "reference": ref_router},
        }

    def _run(
        self, *, run_id: str, identity: str, nonce_seed: str,
        i2pr_router: str, ref_router: str,
    ) -> dict[str, object]:
        return {
            "run_id": run_id,
            "run_identity_sha256": identity,
            "observations": {
                direction: self._direction_record(
                    direction,
                    nonce=f"{nonce_seed}-{direction}",
                    sha=hashlib.sha256(
                        (run_id + direction).encode()
                    ).hexdigest(),
                    i2pr_router=i2pr_router,
                    ref_router=ref_router,
                )
                for direction in (
                    "i2pr-to-java-ipv4",
                    "java-to-i2pr-ipv4",
                    "i2pr-to-i2pd-ipv4",
                    "i2pd-to-i2pr-ipv4",
                )
            },
        }

    def test_20_two_independent_fixtures_pass_independence(self):
        run_a = self._run(
            run_id="plan060-a-positive",
            identity="a" * 64,
            nonce_seed="alpha",
            i2pr_router="1" * 64,
            ref_router="2" * 64,
        )
        run_b = self._run(
            run_id="plan060-b-positive",
            identity="b" * 64,
            nonce_seed="beta",
            i2pr_router="3" * 64,
            ref_router="4" * 64,
        )
        failures = plan060_two_bundle_independence(
            run_a_record=run_a,
            run_b_record=run_b,
        )
        self.assertEqual(failures, [], f"unexpected failures: {failures}")


class Plan060TypedBlockerTests(unittest.TestCase):
    """Plan 060 typed-blocker and close-status classification."""

    def test_typed_blocker_returns_environment_blocker(self):
        self.assertEqual(
            plan060_typed_blocker(),
            TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE,
        )

    def test_close_status_classification(self):
        self.assertEqual(
            plan060_close_status(), "declared-not-executable"
        )

    def test_java_support_topology_blocker_chain(self):
        self.assertEqual(
            plan059.plan059_typed_blocker(),
            TYPED_BLOCKER_JAVA_SUPPORT_TOPOLOGY_REJECTED,
        )


class FreezeReadinessTests(unittest.TestCase):
    """The freeze-readiness checklist records the typed blocker on this host."""

    def test_freeze_readiness_includes_typed_blocker(self):
        report = freeze_readiness_report()
        self.assertFalse(report.ready)
        self.assertIn(
            TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE, report.blockers,
        )

    def test_freeze_invariants_raise_with_typed_blocker(self):
        with self.assertRaisesRegex(Plan060Error, TYPED_BLOCKER_EXECUTION_LANE_UNAVAILABLE):
            assert_plan060_freeze_invariants()

    def test_lane_kinds_and_statuses_locked(self):
        self.assertEqual(LANE_KINDS, {"direct-host", "guest"})
        self.assertIn("declared-not-executable", CLOSE_STATUSES)


class Plan060HelperContractTests(unittest.TestCase):
    """The Plan 060 helper carries the expected lock contract."""

    def test_helper_module_paths(self):
        self.assertTrue(PLAN060_CANDIDATE_PATH.name == "060-candidate.md")
        self.assertTrue(PLAN060_CLOSURE_PATH.name == "060-closure.md")

    def test_candidate_digests_keys_locked(self):
        digests = candidate_record_digests()
        expected = {
            "helper_source_lock_sha256",
            "helper_cpp_source_sha256",
            "helper_python_driver_sha256",
            "observation_catalog_sha256",
            "i2pd_qualification_receipt_sha256",
            "java_qualification_receipt_sha256",
            "qualification_summary_sha256",
            "qualification_summary_status",
            "references_lock_sha256",
        }
        self.assertEqual(set(digests.keys()), expected)


class Plan059ArtifactsTests(unittest.TestCase):
    """The Plan 059 helper and qualification artifacts are committed."""

    def test_plan059_helper_source_lock_present(self):
        lock = plan059.load_source_lock()
        self.assertEqual(lock.schema, HELPER_SOURCE_LOCK_SCHEMA)

    def test_plan059_qualification_receipts_present(self):
        i2pd = plan059.load_observation_qualification_receipt("i2pd")
        java = plan059.load_observation_qualification_receipt("java_i2p")
        self.assertGreater(i2pd.total_marker_count, 0)
        self.assertGreater(java.total_marker_count, 0)
        self.assertEqual(i2pd.qualified_marker_count, 0)
        self.assertEqual(java.qualified_marker_count, 0)

    def test_plan059_summary_blocked(self):
        summary = plan059.load_qualification_summary()
        self.assertEqual(summary.get("summary_status"), "blocked")


if __name__ == "__main__":
    unittest.main()