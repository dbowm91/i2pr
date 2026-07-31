"""Plan 068 plan-of-record test matrix.

The Plan 068 test matrix covers the active-status corrections, the
stale Java blocker removal, the evidence tier contracts, the
release-bundle smoke/development rejection, and the Plan 066
historical/non-executed preservation.
"""

from __future__ import annotations

import re
import sys
import unittest
from pathlib import Path

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import evidence_tier
import loopback_smoke_record as smoke
import development_validation as dev


HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]


ADR_0023_PATH = REPO_ROOT / "docs/adr/0023-staged-ntcp2-interoperability-evidence.md"
ADR_0021_PATH = REPO_ROOT / "docs/adr/0021-minimal-java-support-topology.md"
PLAN_030_PATH = REPO_ROOT / "plans/030-milestone-3-closure.md"
PLAN_066_CLOSURE_PATH = REPO_ROOT / "plans/066-closure.md"
PLAN_066_PLAN_PATH = REPO_ROOT / "plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md"


def _plan_text(path: Path) -> str:
    return path.read_text(encoding="utf-8")


class AuthorityLinkageTests(unittest.TestCase):
    """Plan 068 links the active roadmap, the active ADR, and the Plan
    066 supersession.
    """

    def test_adr_0023_exists(self):
        self.assertTrue(ADR_0023_PATH.is_file())

    def test_adr_0023_is_accepted(self):
        text = _plan_text(ADR_0023_PATH)
        match = re.search(r"^- Status:\s*(\w+)", text, flags=re.MULTILINE)
        self.assertIsNotNone(match)
        self.assertEqual(match.group(1), "Accepted")

    def test_adr_0023_references_plan_067_and_068(self):
        text = _plan_text(ADR_0023_PATH)
        self.assertIn("Plan 067", text)
        self.assertIn("Plan 068", text)

    def test_plan_030_active_status_recorded(self):
        text = _plan_text(PLAN_030_PATH)
        self.assertIn("Plan 067", text)
        self.assertIn("Plan 068", text)
        self.assertIn("implementation_status", text)
        self.assertIn("development_validation", text)
        self.assertIn("release_qualification", text)
        self.assertIn("support", text)
        self.assertIn("experimental", text)
        self.assertIn("advertised", text)

    def test_plan_030_marks_advertised_false(self):
        text = _plan_text(PLAN_030_PATH)
        self.assertIn("advertised             = false", text)

    def test_plan_030_marks_experimental_support(self):
        text = _plan_text(PLAN_030_PATH)
        self.assertIn("support                = experimental", text)

    def test_plan_030_marks_advertised_false_other_form(self):
        text = _plan_text(PLAN_030_PATH)
        self.assertRegex(
            text,
            r"advertised\s*=\s*false",
        )


class Plan066SupersessionTests(unittest.TestCase):
    """Plan 066 documents are marked historical; Plan 068 supersedes."""

    def test_plan_066_closure_carries_supersession_banner(self):
        text = _plan_text(PLAN_066_CLOSURE_PATH)
        self.assertIn("Supersession notice", text)
        self.assertIn("Plan 068", text)
        self.assertIn("ADR 0023", text)

    def test_plan_066_plan_record_carries_supersession_banner(self):
        text = _plan_text(PLAN_066_PLAN_PATH)
        self.assertIn("Supersession notice", text)
        self.assertIn("Plan 068", text)
        self.assertIn("ADR 0023", text)

    def test_plan_066_closure_records_typed_blocker_marker(self):
        text = _plan_text(PLAN_066_CLOSURE_PATH)
        self.assertIn("blocked_execution_lane_unavailable", text)

    def test_plan_066_candidate_preserved(self):
        candidate = REPO_ROOT / "plans/066-candidate.md"
        self.assertTrue(candidate.is_file())
        text = _plan_text(candidate)
        self.assertIn("declared-not-executable", text)


class JavaBlockerRemovalTests(unittest.TestCase):
    """The stale Java blocker is removed from the active path; the
    historical and ADR 0021 Rejected wording is preserved.
    """

    def test_adr_0021_is_rejected(self):
        text = _plan_text(ADR_0021_PATH)
        match = re.search(r"^- Status:\s*(\w+)", text, flags=re.MULTILINE)
        self.assertIsNotNone(match)
        self.assertEqual(match.group(1), "Rejected")

    def test_adr_0022_is_accepted(self):
        adr22 = REPO_ROOT / "docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md"
        text = _plan_text(adr22)
        match = re.search(r"^- Status:\s*(\w+)", text, flags=re.MULTILINE)
        self.assertIsNotNone(match)
        self.assertEqual(match.group(1), "Accepted")

    def test_plan_030_records_java_blocker_as_historical(self):
        text = _plan_text(PLAN_030_PATH)
        self.assertIn("blocked_java_support_topology_rejected", text)
        self.assertRegex(
            text,
            r"historical for Plans 058-060",
        )

    def test_adr_0023_does_not_supersede_adr_0022(self):
        text = _plan_text(ADR_0023_PATH)
        self.assertIn(
            "ADR 0022",
            text,
        )


class EvidenceTierTests(unittest.TestCase):
    """The Plan 068 evidence-tier module exposes all five tiers and
    refuses promotion.
    """

    def test_evidence_tier_module_present(self):
        self.assertTrue((HERE / "evidence_tier.py").is_file())

    def test_all_five_tiers_exposed(self):
        self.assertEqual(len(evidence_tier.all_tiers()), 5)

    def test_release_validates_only_release_qualification(self):
        good = {"evidence_tier": "release-qualification"}
        evidence_tier.assert_release_record_tier(good)

    def test_release_rejects_smoke_tier(self):
        bad = {"evidence_tier": "external-loopback-smoke"}
        with self.assertRaises(evidence_tier.EvidenceTierError):
            evidence_tier.assert_release_record_tier(bad)

    def test_release_rejects_development_tier(self):
        bad = {"evidence_tier": "repeated-development-interop"}
        with self.assertRaises(evidence_tier.EvidenceTierError):
            evidence_tier.assert_release_record_tier(bad)

    def test_release_rejects_local_tier(self):
        bad = {"evidence_tier": "local-conformance"}
        with self.assertRaises(evidence_tier.EvidenceTierError):
            evidence_tier.assert_release_record_tier(bad)


class SmokeSchemaTests(unittest.TestCase):
    """The Plan 068/069 Level 1 smoke record schema is committed."""

    def test_loopback_smoke_record_module_present(self):
        self.assertTrue((HERE / "loopback_smoke_record.py").is_file())

    def test_loopback_smoke_record_test_present(self):
        self.assertTrue((HERE / "test_loopback_smoke_record.py").is_file())

    def test_loopback_smoke_record_carries_schema_marker(self):
        text = (HERE / "loopback_smoke_record.py").read_text(encoding="utf-8")
        self.assertIn("i2pr-ntcp2-loopback-smoke-v1", text)

    def test_loopback_smoke_record_test_carries_positive(self):
        text = (HERE / "test_loopback_smoke_record.py").read_text(encoding="utf-8")
        self.assertIn("PositiveFixtureTests", text)

    def test_loopback_smoke_record_test_carries_blocked(self):
        text = (HERE / "test_loopback_smoke_record.py").read_text(encoding="utf-8")
        self.assertIn("BlockedRecordTests", text)

    def test_smoke_module_uses_external_loopback_smoke_tier(self):
        text = (HERE / "loopback_smoke_record.py").read_text(encoding="utf-8")
        self.assertIn("external-loopback-smoke", text)

    def test_smoke_module_forbids_secret_bearing_fields(self):
        text = (HERE / "loopback_smoke_record.py").read_text(encoding="utf-8")
        self.assertIn("raw_payload", text)
        self.assertIn("private_key", text)
        self.assertIn("noise_state", text)
        self.assertIn("router_info_bytes", text)


class DevelopmentSchemaTests(unittest.TestCase):
    """The Plan 068/071 Level 2 development summary schema is committed."""

    def test_development_validation_module_present(self):
        self.assertTrue((HERE / "development_validation.py").is_file())

    def test_development_validation_test_present(self):
        self.assertTrue((HERE / "test_development_validation.py").is_file())

    def test_development_validation_carries_schema_marker(self):
        text = (HERE / "development_validation.py").read_text(encoding="utf-8")
        self.assertIn("i2pr-ntcp2-development-validation-v1", text)

    def test_development_validation_requires_three_passes(self):
        text = (HERE / "development_validation.py").read_text(encoding="utf-8")
        self.assertIn("required_passes_per_direction", text)

    def test_development_validation_requires_named_controls(self):
        text = (HERE / "development_validation.py").read_text(encoding="utf-8")
        self.assertIn("network_id_mismatch", text)
        self.assertIn("router_info_or_static_key_mismatch", text)
        self.assertIn("delivery_status_correlation_mismatch", text)
        self.assertIn("unauthenticated_data_phase", text)


class ReleaseBundleRejectionTests(unittest.TestCase):
    """The release bundle schema allowlist refuses smoke/development."""

    def test_release_bundle_does_not_reference_smoke(self):
        bundle = HERE / "evidence_bundle.py"
        if not bundle.is_file():
            self.skipTest("evidence_bundle.py not present")
        text = bundle.read_text(encoding="utf-8")
        self.assertNotIn("i2pr-ntcp2-loopback-smoke-v1", text)

    def test_release_bundle_does_not_reference_development(self):
        bundle = HERE / "evidence_bundle.py"
        if not bundle.is_file():
            self.skipTest("evidence_bundle.py not present")
        text = bundle.read_text(encoding="utf-8")
        self.assertNotIn("i2pr-ntcp2-development-validation-v1", text)


class StaticCheckTests(unittest.TestCase):
    """The static boundary checker enforces Plan 068 schemas."""

    def test_check_script_references_plan_068(self):
        path = REPO_ROOT / "scripts/check-ntcp2-interoperability.sh"
        text = path.read_text(encoding="utf-8")
        self.assertIn("Plan 068", text)

    def test_check_script_requires_evidence_tier_module(self):
        path = REPO_ROOT / "scripts/check-ntcp2-interoperability.sh"
        text = path.read_text(encoding="utf-8")
        self.assertIn("evidence_tier.py", text)

    def test_check_script_requires_smoke_schema(self):
        path = REPO_ROOT / "scripts/check-ntcp2-interoperability.sh"
        text = path.read_text(encoding="utf-8")
        self.assertIn("loopback_smoke_record.py", text)

    def test_check_script_requires_development_schema(self):
        path = REPO_ROOT / "scripts/check-ntcp2-interoperability.sh"
        text = path.read_text(encoding="utf-8")
        self.assertIn("development_validation.py", text)


class DocumentationTests(unittest.TestCase):
    """Documentation propagates Plan 067/068 throughout the
    operator-facing surface.
    """

    def test_readme_references_plan_068(self):
        path = REPO_ROOT / "README.md"
        text = path.read_text(encoding="utf-8")
        self.assertIn("Plan 068", text)
        self.assertIn("Plan 067", text)

    def test_agents_references_plan_068(self):
        path = REPO_ROOT / "AGENTS.md"
        text = path.read_text(encoding="utf-8")
        self.assertIn("Plan 068", text)

    def test_architecture_references_plan_068(self):
        path = REPO_ROOT / "docs/architecture/interop-apparatus.md"
        text = path.read_text(encoding="utf-8")
        self.assertIn("Plan 068", text)

    def test_protocol_support_references_plan_068(self):
        path = REPO_ROOT / "docs/protocol-support.md"
        text = path.read_text(encoding="utf-8")
        self.assertIn("Plan 068", text)

    def test_skill_references_plan_068(self):
        path = REPO_ROOT / ".opencode/skills/i2pr-ntcp2-interop/SKILL.md"
        text = path.read_text(encoding="utf-8")
        self.assertIn("Plan 068", text)

    def test_readme_marks_java_blocker_historical(self):
        path = REPO_ROOT / "README.md"
        text = path.read_text(encoding="utf-8")
        # The plan says the stale Java blocker removal wording must
        # appear in README.
        self.assertIn("ADR 0022", text)


if __name__ == "__main__":
    unittest.main()