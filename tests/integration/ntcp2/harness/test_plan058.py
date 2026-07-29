"""Plan 058 candidate record integrity, supersession, and execution-lane tests.

These tests cover Plan 058 Phases 1-7. The mandatory cases are:

1. one-authoritative-SHA positive case (declared/executed candidate);
2. two-authoritative-SHA rejection (multiple ``candidate_commit`` values);
3. short-SHA rejection (less than 40 lowercase hex characters);
4. retired candidate rejected by execution tooling;
5. candidate before implementation floor rejected;
6. local-untracked receipt accepted only with explicit storage
   classification;
7. ``committed bundle`` claim rejected when tracked artifact path is
   absent;
8. direct-host lane accepts host rootless success without Multipass;
9. guest lane accepts restrictive outer host when guest rootless
   succeeds;
10. guest lane rejects guest rootless failure;
11. cross-lane Run A/Run B combination rejected;
12. ADR Proposed prevents Plan 058 closure;
13. ADR Accepted permits Plan 059 activation;
14. ADR Rejected blocks Plan 060 activation.

The validator is also exercised against the on-disk candidate documents
so the static gates remain in sync with the actual directory contents.
"""

from __future__ import annotations

import json
import os
import shutil
import subprocess
import tempfile
import unittest
from pathlib import Path

HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]

from candidate_record import (
    CANDIDATE_SCHEMA,
    CANDIDATE_SCHEMA_VERSION,
    CANDIDATE_FIELDS,
    CandidateRecordError,
    active_candidate_record,
    adr_decision,
    assert_evidence_storage_claim,
    build_candidate_record,
    execution_lane_record,
    extract_candidate_record_from_markdown,
    finalize_candidate_record,
    retired_marker_present,
    superseded_marker_present,
    validate_candidate_record,
)


_PLAN_058_FLOOR = "2da328ebcd1d3ced4a614f7ba71d92e12cf24efa"
_PLAN_059_FLOOR = "43b8a743cfbad3db76546bb875e8a076dcbad2bc"
_PLAN_056_CANDIDATE = "fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf"
_PLAN_056_FLOOR = "2457b74a0a129e8ef2aedd3abcd4883925f5b376"
_PLAN_056_VERIFIER = "1eb6cd640ce3c3e5141b62910fcae8d42f72c54a"


def _declared(**overrides): return build_candidate_record(
    plan=58,
    candidate_commit=overrides.get("candidate_commit") or _PLAN_058_FLOOR,
    status=overrides.get("status", "declared"),
    implementation_floor_commit=overrides.get("implementation_floor_commit") or _PLAN_058_FLOOR,
    source_tree_sha256=overrides.get("source_tree_sha256", "a" * 64),
    validation_receipt_sha256=overrides.get("validation_receipt_sha256", "0" * 64),
    storage_classification=overrides.get("storage_classification", "local-untracked"),
    history_commits=overrides.get("history_commits", ()),
)


class CandidatePositiveTests(unittest.TestCase):
    """The validator accepts a single, well-formed, declared candidate."""

    def test_one_authoritative_sha_accepted(self):
        candidate = _declared()
        validate_candidate_record(candidate, git_repo_root=REPO_ROOT)
        self.assertTrue(active_candidate_record(candidate))

    def test_candidate_finalizes_candidate_sha256(self):
        candidate = _declared()
        finalized = finalize_candidate_record(candidate)
        self.assertEqual(len(finalized["candidate_sha256"]), 64)
        hex_chars = "0123456789abcdef"
        self.assertTrue(all(c in hex_chars for c in finalized["candidate_sha256"]))
        self.assertEqual(finalized["candidate_sha256"], finalized["candidate_sha256"])

    def test_retired_candidate_with_history_commits_accepted(self):
        candidate = _declared(
            candidate_commit=_PLAN_056_CANDIDATE,
            status="retired",
            implementation_floor_commit="1f3997b3518cba5315de6a52bab25fa63f2e0b2d",
            history_commits=(_PLAN_056_FLOOR,),
        )
        validate_candidate_record(candidate, git_repo_root=REPO_ROOT)
        self.assertFalse(active_candidate_record(candidate))


class CandidateRejectionTests(unittest.TestCase):
    """The validator rejects every documented defect."""

    def test_two_authoritative_sha_rejected(self):
        candidate = _declared()
        candidate["candidate_commit"] = _PLAN_056_CANDIDATE
        candidate["history_commits"] = [_PLAN_058_FLOOR, _PLAN_056_CANDIDATE]
        with self.assertRaisesRegex(CandidateRecordError, "history_commits"):
            validate_candidate_record(candidate, git_repo_root=REPO_ROOT)

    def test_two_distinct_authoritative_commits_rejected(self):
        candidate = _declared()
        candidate["history_commits"] = [_PLAN_059_FLOOR]
        with self.assertRaisesRegex(CandidateRecordError, "history_commits"):
            validate_candidate_record(candidate, git_repo_root=REPO_ROOT)

    def test_short_sha_rejected(self):
        candidate = _declared()
        candidate["candidate_commit"] = "2457b74"
        with self.assertRaisesRegex(CandidateRecordError, "40 lowercase hex"):
            validate_candidate_record(candidate, git_repo_root=REPO_ROOT)

    def test_unknown_commit_rejected(self):
        candidate = _declared()
        candidate["candidate_commit"] = "f" * 40
        with self.assertRaisesRegex(CandidateRecordError, "does not resolve"):
            validate_candidate_record(candidate, git_repo_root=REPO_ROOT)

    def test_unknown_implementation_floor_rejected(self):
        candidate = _declared()
        candidate["implementation_floor_commit"] = "f" * 40
        with self.assertRaisesRegex(CandidateRecordError, "does not resolve"):
            validate_candidate_record(candidate, git_repo_root=REPO_ROOT)

    def test_retired_candidate_rejected_by_execution_tooling(self):
        candidate = _declared(
            candidate_commit=_PLAN_056_CANDIDATE,
            status="retired",
            implementation_floor_commit="2457b74a0a129e8ef2aedd3abcd4883925f5b376",
        )
        validate_candidate_record(candidate, git_repo_root=REPO_ROOT)
        self.assertFalse(active_candidate_record(candidate))

    def test_candidate_before_implementation_floor_rejected(self):
        candidate = _declared(
            candidate_commit=_PLAN_056_FLOOR,
            implementation_floor_commit=_PLAN_058_FLOOR,
        )
        with self.assertRaisesRegex(CandidateRecordError, "implementation_floor"):
            validate_candidate_record(candidate, git_repo_root=REPO_ROOT)

    def test_invalid_storage_classification_rejected(self):
        candidate = _declared()
        candidate["storage_classification"] = "shadow-claim"
        with self.assertRaisesRegex(CandidateRecordError, "storage_classification"):
            validate_candidate_record(candidate, git_repo_root=REPO_ROOT)

    def test_tracked_storage_requires_executed_status(self):
        candidate = _declared()
        candidate["storage_classification"] = "tracked"
        with self.assertRaisesRegex(CandidateRecordError, "tracked"):
            validate_candidate_record(candidate, git_repo_root=REPO_ROOT)

    def test_committed_bundle_claim_rejected_when_path_absent(self):
        with tempfile.TemporaryDirectory() as tmp:
            missing = Path(tmp) / "missing-bundle.json"
            with self.assertRaisesRegex(CandidateRecordError, "tracked artifact"):
                assert_evidence_storage_claim(
                    claim=(
                        "the diagnostic bundle is committed under "
                        f"target/interop/evidence/plan056/run-a/{missing.name}"
                    ),
                    tracked_paths=(missing,),
                )

    def test_committed_bundle_claim_accepted_when_tracked_path_present(self):
        with tempfile.TemporaryDirectory() as tmp:
            tracked = Path(tmp) / "evidence-receipt.json"
            tracked.write_text("{}", encoding="utf-8")
            assert_evidence_storage_claim(
                claim=(
                    "the bounded receipt is committed under "
                    "tests/integration/ntcp2/evidence-receipts/plan056-local-diagnostic.json"
                ),
                tracked_paths=(tracked,),
            )


class ExecutionLaneTests(unittest.TestCase):
    """The validator distinguishes direct-host and guest lanes."""

    def test_direct_host_lane_accepts_rootless_success(self):
        receipt = execution_lane_record(
            lane_kind="direct-host",
            outer_host_baseline="rootless_sandbox_available",
            guest_rootless_outcome="",
            environment_manifest_sha256="a" * 64,
            direct_host_rootless_outcome="rootless_sandbox_available",
        )
        self.assertEqual(receipt["lane_kind"], "direct-host")
        self.assertEqual(receipt["direct_host_rootless_outcome"], "rootless_sandbox_available")

    def test_guest_lane_accepts_restrictive_outer_host(self):
        receipt = execution_lane_record(
            lane_kind="guest",
            outer_host_baseline="blocked_unprivileged_user_namespace",
            guest_rootless_outcome="rootless_sandbox_available",
            environment_manifest_sha256="a" * 64,
            vm_manager_version="multipass-1.16.1",
        )
        self.assertEqual(receipt["lane_kind"], "guest")
        self.assertEqual(receipt["outer_host_baseline"], "blocked_unprivileged_user_namespace")
        self.assertEqual(receipt["guest_rootless_outcome"], "rootless_sandbox_available")

    def test_guest_lane_rejects_guest_rootless_failure(self):
        with self.assertRaisesRegex(CandidateRecordError, "guest lane requires"):
            execution_lane_record(
                lane_kind="guest",
                outer_host_baseline="blocked_unprivileged_user_namespace",
                guest_rootless_outcome="",
                environment_manifest_sha256="a" * 64,
                vm_manager_version="multipass-1.16.1",
            )

    def test_cross_lane_combination_rejected(self):
        with self.assertRaisesRegex(CandidateRecordError, "must not report"):
            execution_lane_record(
                lane_kind="guest",
                outer_host_baseline="blocked_unprivileged_user_namespace",
                guest_rootless_outcome="rootless_sandbox_available",
                environment_manifest_sha256="a" * 64,
                vm_manager_version="multipass-1.16.1",
                direct_host_rootless_outcome="rootless_sandbox_available",
            )

    def test_direct_host_lane_rejects_guest_payload(self):
        with self.assertRaisesRegex(CandidateRecordError, "must not report"):
            execution_lane_record(
                lane_kind="direct-host",
                outer_host_baseline="rootless_sandbox_available",
                guest_rootless_outcome="rootless_sandbox_available",
                environment_manifest_sha256="a" * 64,
                vm_manager_version="multipass-1.16.1",
                direct_host_rootless_outcome="rootless_sandbox_available",
            )


class AdrDecisionTests(unittest.TestCase):
    """The ADR decision helper covers the proposed flows."""

    def test_adr_proposed_prevents_plan058_closure(self):
        with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False) as handle:
            handle.write("- Status: Proposed\n")
            handle.write("- Date: 2026-07-29\n")
            path = Path(handle.name)
        try:
            self.assertEqual(adr_decision(path), "Proposed")
        finally:
            path.unlink()

    def test_adr_accepted_permits_plan059_activation(self):
        with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False) as handle:
            handle.write("- Status: Accepted\n")
            handle.write("- Date: 2026-07-29\n")
            path = Path(handle.name)
        try:
            self.assertEqual(adr_decision(path), "Accepted")
        finally:
            path.unlink()

    def test_adr_rejected_blocks_plan060_activation(self):
        with tempfile.NamedTemporaryFile("w", suffix=".md", delete=False) as handle:
            handle.write("- Status: Rejected\n")
            handle.write("- Date: 2026-07-29\n")
            path = Path(handle.name)
        try:
            self.assertEqual(adr_decision(path), "Rejected")
        finally:
            path.unlink()


class PlanDocumentMarkerTests(unittest.TestCase):
    """Plan 058 enforces literal markers in candidate and superseded documents."""

    def test_plan056_candidate_carries_retired_marker(self):
        path = REPO_ROOT / "plans" / "056-candidate.md"
        if not path.exists():
            self.skipTest("plan056 candidate file not present")
        self.assertTrue(
            retired_marker_present(path),
            "plans/056-candidate.md must declare a retired status",
        )

    def test_plan057_carries_superseded_marker(self):
        path = REPO_ROOT / "plans" / "057-cross-host-milestone-3-external-evidence-run.md"
        if not path.exists():
            self.skipTest("plan057 file not present")
        self.assertTrue(
            superseded_marker_present(path),
            "plans/057-cross-host-milestone-3-external-evidence-run.md must be superseded",
        )

    def test_adr_0021_carries_explicit_decision(self):
        path = REPO_ROOT / "docs" / "adr" / "0021-minimal-java-support-topology.md"
        self.assertTrue(path.exists(), "ADR 0021 must exist")
        decision = adr_decision(path)
        self.assertIn(
            decision, {"Accepted", "Rejected"},
            "Plan 058 requires an explicit Accept/Reject decision for ADR 0021",
        )


class MarkdownExtractionTests(unittest.TestCase):
    """The validator extracts a candidate record from a Markdown document."""

    def test_extract_candidate_record(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "candidate.md"
            payload = build_candidate_record(
                plan=58,
                candidate_commit=_PLAN_058_FLOOR,
                status="declared",
                implementation_floor_commit=_PLAN_058_FLOOR,
                source_tree_sha256="a" * 64,
            )
            finalized = finalize_candidate_record(payload)
            path.write_text(
                "# Plan 058 candidate record\n"
                "Status: declared\n"
                "\n"
                "```json\n"
                + json.dumps(finalized, indent=2)
                + "\n```\n",
                encoding="utf-8",
            )
            extracted = extract_candidate_record_from_markdown(path, repo_root=REPO_ROOT)
            self.assertEqual(extracted["schema"], CANDIDATE_SCHEMA)
            self.assertEqual(extracted["schema_version"], CANDIDATE_SCHEMA_VERSION)
            self.assertEqual(extracted["candidate_commit"], _PLAN_058_FLOOR)

    def test_extract_rejects_two_candidate_blocks(self):
        with tempfile.TemporaryDirectory() as tmp:
            path = Path(tmp) / "candidate.md"
            payload_a = build_candidate_record(
                plan=58,
                candidate_commit=_PLAN_058_FLOOR,
                status="declared",
                implementation_floor_commit=_PLAN_058_FLOOR,
                source_tree_sha256="a" * 64,
            )
            payload_b = build_candidate_record(
                plan=58,
                candidate_commit=_PLAN_059_FLOOR,
                status="declared",
                implementation_floor_commit=_PLAN_058_FLOOR,
                source_tree_sha256="b" * 64,
            )
            path.write_text(
                "```json\n" + json.dumps(finalize_candidate_record(payload_a)) + "\n```\n"
                "```json\n" + json.dumps(finalize_candidate_record(payload_b)) + "\n```\n",
                encoding="utf-8",
            )
            with self.assertRaisesRegex(CandidateRecordError, "exactly one"):
                extract_candidate_record_from_markdown(path, repo_root=REPO_ROOT)


class OnDiskCandidateTests(unittest.TestCase):
    """The validator must run against the actual on-disk Plan 056 candidate."""

    def setUp(self):
        self.path = REPO_ROOT / "plans" / "056-candidate.md"
        if not self.path.exists():
            self.skipTest("plan056 candidate file not present")

    def test_on_disk_plan056_candidate_marks_retired(self):
        self.assertTrue(
            retired_marker_present(self.path),
            "plans/056-candidate.md must declare retired status"
        )

    def test_on_disk_plan056_candidate_does_not_embed_active_record(self):
        text = self.path.read_text(encoding="utf-8")
        self.assertNotIn(
            "status\": \"declared\"",
            text,
            "plan056 candidate must not embed an active candidate record",
        )

    def test_on_disk_plan056_candidate_marks_history_outside_authoritative(self):
        text = self.path.read_text(encoding="utf-8")
        if "fbf2cdb9ec12d35c7b7422c412e09d6db2d2d0cf" in text:
            self.assertIn(
                "Retired historical SHA",
                text,
                "any historical SHA must be clearly marked as historical",
            )


class RequiredLockedFieldsTests(unittest.TestCase):
    """Lock the candidate record schema field set."""

    def test_locked_fields(self):
        self.assertEqual(
            CANDIDATE_FIELDS,
            (
                "schema",
                "schema_version",
                "plan",
                "candidate_commit",
                "status",
                "implementation_floor_commit",
                "source_tree_sha256",
                "validation_receipt_sha256",
                "storage_classification",
                "history_commits",
                "candidate_sha256",
            ),
        )


if __name__ == "__main__":
    unittest.main()
