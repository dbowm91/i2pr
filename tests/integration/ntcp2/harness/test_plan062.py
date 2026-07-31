"""Plan 062 evidence-contract and architecture correction tests.

These tests cover the Plan 062 work packages WP1-WP5:

- WP1: the source-verification record exists for the pinned Java
  I2P 2.12.0 and i2pd 2.60.0 revisions;
- WP2: ADR 0022 (direct reference router NTCP2 interop drivers) is
  Accepted and explicitly supersedes ADR 0021;
- WP3: the v4 trigger schema, reference-event v1 schema, and v3
  observation schema are committed and active;
- WP4: the v4 trigger schema and v3 observation schema are
  mandatory for primary IPv4 mixed-router directions and the v3
  schema cannot contribute to a new passing bundle;
- WP5: Plan 060 is retired and cannot be active execution
  authority; the future candidate implementation floor must be
  Plan 065 closure or later.

These tests are unit tests on the contract surface. They do not
launch a router or run the harness.
"""

from __future__ import annotations

import re
import unittest
from pathlib import Path


HERE = Path(__file__).resolve().parent
REPO_ROOT = HERE.parents[3]


class WP1SourceVerificationTests(unittest.TestCase):
    def test_source_verification_record_exists(self):
        path = REPO_ROOT / "tests/integration/ntcp2/reference-drivers/source-verification.md"
        self.assertTrue(path.is_file())

    def test_source_verification_records_java_pinned_revision(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/source-verification.md").read_text()
        self.assertIn("2800040deee9bb376567b671ef2e9c34cf3e30b6", text)

    def test_source_verification_records_i2pd_pinned_revision(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/source-verification.md").read_text()
        self.assertIn("f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e", text)

    def test_source_verification_references_64_hex_router_hash(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/reference-drivers/source-verification.md").read_text()
        self.assertIn("64 lowercase hexadecimal characters", text)
        self.assertIn("32 bytes", text)


class WP2ADR0022Tests(unittest.TestCase):
    def test_adr_0022_exists(self):
        path = REPO_ROOT / "docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md"
        self.assertTrue(path.is_file())

    def test_adr_0022_status_is_accepted(self):
        text = (REPO_ROOT / "docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md").read_text()
        match = re.search(r"^- Status:\s*Accepted\b", text, flags=re.MULTILINE)
        self.assertIsNotNone(match)

    def test_adr_0022_supersedes_adr_0021(self):
        text = (REPO_ROOT / "docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md").read_text()
        self.assertIn("Supersedes", text)
        self.assertIn("ADR 0021", text)
        self.assertIn("Rejected", text)

    def test_adr_0022_rejects_sam_and_http_triggers(self):
        text = (REPO_ROOT / "docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md").read_text()
        self.assertIn("SAM", text)
        self.assertIn("HTTP", text)
        self.assertIn("rejected", text.lower())

    def test_adr_0022_records_two_process_topology(self):
        text = (REPO_ROOT / "docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md").read_text()
        self.assertIn("two-process", text)
        self.assertIn("rootless sealed network namespace", text)


class WP3SchemaImplementationTests(unittest.TestCase):
    def test_v4_trigger_schema_module_exists(self):
        path = REPO_ROOT / "tests/integration/ntcp2/harness/reference_trigger_v4.py"
        self.assertTrue(path.is_file())

    def test_v4_trigger_schema_marker(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/harness/reference_trigger_v4.py").read_text()
        self.assertIn('TRIGGER_SCHEMA = "i2pr-reference-trigger-v4"', text)
        self.assertIn("TRIGGER_SCHEMA_VERSION = 4", text)

    def test_reference_event_schema_module_exists(self):
        path = REPO_ROOT / "tests/integration/ntcp2/harness/reference_event.py"
        self.assertTrue(path.is_file())

    def test_reference_event_schema_marker(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/harness/reference_event.py").read_text()
        self.assertIn('EVENT_SCHEMA = "i2pr-reference-event-v1"', text)
        self.assertIn("EVENT_SCHEMA_VERSION = 1", text)

    def test_v3_observation_schema_module_exists(self):
        path = REPO_ROOT / "tests/integration/ntcp2/harness/observation_v3.py"
        self.assertTrue(path.is_file())

    def test_v3_observation_schema_marker(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/harness/observation_v3.py").read_text()
        self.assertIn('OBSERVATION_SCHEMA = "i2pr-ntcp2-direction-observation-v3"', text)
        self.assertIn("OBSERVATION_SCHEMA_VERSION = 3", text)


class WP4SchemaMigrationTests(unittest.TestCase):
    def test_evidence_bundle_allowlists_v4_trigger(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/harness/evidence_bundle.py").read_text()
        self.assertIn('"i2pr-reference-trigger-v4"', text)

    def test_evidence_bundle_allowlists_v3_observation(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/harness/evidence_bundle.py").read_text()
        self.assertIn('"i2pr-ntcp2-direction-observation-v3"', text)

    def test_evidence_bundle_allowlists_reference_event_v1(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/harness/evidence_bundle.py").read_text()
        self.assertIn('"i2pr-reference-event-v1"', text)

    def test_pipeline_accepts_v4_trigger_record(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/harness/plan052_pipeline.py").read_text()
        self.assertIn("reference_trigger_v4", text)
        self.assertIn("TRIGGER_SCHEMA_V4", text)

    def test_pipeline_accepts_v3_observation_record(self):
        text = (REPO_ROOT / "tests/integration/ntcp2/harness/plan052_pipeline.py").read_text()
        self.assertIn("observation_v3", text)


class WP5Plan060RetirementTests(unittest.TestCase):
    def test_plan_060_candidate_retired(self):
        text = (REPO_ROOT / "plans/060-candidate.md").read_text()
        match = re.search(
            r"Status:.*retired|Retired by Plan 062",
            text,
            flags=re.IGNORECASE,
        )
        self.assertIsNotNone(match)

    def test_plan_060_candidate_retirement_marker(self):
        text = (REPO_ROOT / "plans/060-candidate.md").read_text().lower()
        self.assertIn("retired by plan 062", text)

    def test_plan_060_closure_supersession_marker(self):
        text = (REPO_ROOT / "plans/060-closure.md").read_text()
        self.assertIn("Superseded by Plan 062", text)

    def test_plan_060_preserves_typed_blocker_marker(self):
        text = (REPO_ROOT / "plans/060-closure.md").read_text()
        self.assertIn("blocked_execution_lane_unavailable", text)

    def test_plan_060_plan_of_record_retired_marker(self):
        text = (REPO_ROOT / "plans/060-fresh-candidate-and-two-run-milestone3-certificate-closure-pass.md").read_text()
        self.assertIn("retired (Plan 062", text)

    def test_future_candidate_floor_is_plan_065(self):
        text = (REPO_ROOT / "plans/060-candidate.md").read_text().lower()
        self.assertIn("plan 065", text)

    def test_adr_0021_rejected_decision_preserved(self):
        text = (REPO_ROOT / "docs/adr/0021-minimal-java-support-topology.md").read_text()
        match = re.search(r"^- Status:\s*Rejected\b", text, flags=re.MULTILINE)
        self.assertIsNotNone(match)


class RoadmapChainTests(unittest.TestCase):
    def test_plan_061_roadmap_references_plan_062(self):
        text = (REPO_ROOT / "plans/061-ntcp2-direct-reference-driver-corrective-roadmap.md").read_text()
        self.assertIn("Plan 062", text)

    def test_plan_063_plan_of_record_exists(self):
        path = REPO_ROOT / "plans/063-java-i2p-stripped-router-direct-ntcp2-driver.md"
        self.assertTrue(path.is_file())

    def test_plan_064_plan_of_record_exists(self):
        path = REPO_ROOT / "plans/064-i2pd-direct-ntcp2-driver-and-observer-correction.md"
        self.assertTrue(path.is_file())

    def test_plan_065_plan_of_record_exists(self):
        path = REPO_ROOT / "plans/065-ntcp2-canonical-integration-and-live-qualification.md"
        self.assertTrue(path.is_file())

    def test_plan_066_plan_of_record_exists(self):
        path = REPO_ROOT / "plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md"
        self.assertTrue(path.is_file())

    def test_plan_063_references_source_verification(self):
        text = (REPO_ROOT / "plans/063-java-i2p-stripped-router-direct-ntcp2-driver.md").read_text().lower()
        self.assertTrue(
            "source-verification" in text or "source verification" in text,
            "Plan 063 must reference the source-verification record",
        )

    def test_plan_064_references_source_verification(self):
        text = (REPO_ROOT / "plans/064-i2pd-direct-ntcp2-driver-and-observer-correction.md").read_text().lower()
        self.assertTrue(
            "source verification" in text or "source-verification" in text,
            "Plan 064 must reference the source-verification record",
        )


class NonGoalsGuardTests(unittest.TestCase):
    def test_plan_062_does_not_implement_drivers(self):
        text = (REPO_ROOT / "plans/062-ntcp2-evidence-contract-and-architecture-correction.md").read_text()
        # The plan file uses the phrase "Do not start Java or i2pd
        # driver code in this plan" in the small-model handoff
        # instructions section. The non-goals list also explicitly
        # excludes driver implementation.
        self.assertIn("Do not start Java or i2pd driver code in this plan", text)
        self.assertIn("implement Java or i2pd drivers", text)


if __name__ == "__main__":
    unittest.main()
