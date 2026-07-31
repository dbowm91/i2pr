"""Plan 068 evidence-tier test matrix.

The tier-separation rules in
``tests/integration/ntcp2/harness/evidence_tier.py`` enforce ADR 0023.
This module is the canonical test surface.
"""

from __future__ import annotations

import sys
import unittest
from pathlib import Path

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import evidence_tier


class EvidenceTierAcceptanceTests(unittest.TestCase):
    """Every evidence-tier value is accepted by :func:`is_valid_tier`."""

    def test_local_conformance_accepted(self):
        self.assertTrue(evidence_tier.is_valid_tier("local-conformance"))

    def test_external_loopback_smoke_accepted(self):
        self.assertTrue(evidence_tier.is_valid_tier("external-loopback-smoke"))

    def test_repeated_development_interop_accepted(self):
        self.assertTrue(
            evidence_tier.is_valid_tier("repeated-development-interop"),
        )

    def test_conditional_differential_accepted(self):
        self.assertTrue(evidence_tier.is_valid_tier("conditional-differential"))

    def test_release_qualification_accepted(self):
        self.assertTrue(evidence_tier.is_valid_tier("release-qualification"))


class EvidenceTierRejectionTests(unittest.TestCase):
    """Unknown tiers and non-string values are rejected."""

    def test_unknown_tier_rejected(self):
        self.assertFalse(evidence_tier.is_valid_tier("preliminary"))
        self.assertFalse(evidence_tier.is_valid_tier("smoke"))
        self.assertFalse(evidence_tier.is_valid_tier(""))
        self.assertFalse(evidence_tier.is_valid_tier("RELEASE-QUALIFICATION"))

    def test_non_string_rejected(self):
        self.assertFalse(evidence_tier.is_valid_tier(None))
        self.assertFalse(evidence_tier.is_valid_tier(0))
        self.assertFalse(evidence_tier.is_valid_tier(["release-qualification"]))
        self.assertFalse(evidence_tier.is_valid_tier({"value": "release-qualification"}))

    def test_all_tiers_returns_five_values(self):
        self.assertEqual(len(evidence_tier.all_tiers()), 5)

    def test_all_tiers_contains_release_qualification(self):
        self.assertIn("release-qualification", evidence_tier.all_tiers())


class ReleasePredicateTests(unittest.TestCase):
    """The release predicate accepts only release-qualification tier."""

    def test_release_qualification_satisfies_release(self):
        self.assertTrue(
            evidence_tier.tier_satisfies_release("release-qualification"),
        )

    def test_development_does_not_satisfy_release(self):
        self.assertFalse(
            evidence_tier.tier_satisfies_release("repeated-development-interop"),
        )

    def test_smoke_does_not_satisfy_release(self):
        self.assertFalse(
            evidence_tier.tier_satisfies_release("external-loopback-smoke"),
        )

    def test_differential_does_not_satisfy_release(self):
        self.assertFalse(
            evidence_tier.tier_satisfies_release("conditional-differential"),
        )

    def test_local_does_not_satisfy_release(self):
        self.assertFalse(
            evidence_tier.tier_satisfies_release("local-conformance"),
        )


class DevelopmentPredicateTests(unittest.TestCase):
    """The development predicate accepts release and repeated-development tiers."""

    def test_release_satisfies_development(self):
        self.assertTrue(
            evidence_tier.tier_satisfies_development("release-qualification"),
        )

    def test_repeated_development_satisfies_development(self):
        self.assertTrue(
            evidence_tier.tier_satisfies_development(
                "repeated-development-interop",
            ),
        )

    def test_smoke_does_not_satisfy_development(self):
        self.assertFalse(
            evidence_tier.tier_satisfies_development("external-loopback-smoke"),
        )

    def test_differential_does_not_satisfy_development(self):
        self.assertFalse(
            evidence_tier.tier_satisfies_development("conditional-differential"),
        )


class ReleaseBundleRejectionTests(unittest.TestCase):
    """The release bundle validator rejects lower-tier records."""

    def test_release_record_accepted(self):
        record = {"evidence_tier": "release-qualification"}
        evidence_tier.assert_release_record_tier(record)

    def test_smoke_record_rejected_in_release(self):
        record = {"evidence_tier": "external-loopback-smoke"}
        with self.assertRaisesRegex(
            evidence_tier.EvidenceTierError,
            "lower-tier record",
        ):
            evidence_tier.assert_release_record_tier(record)

    def test_development_record_rejected_in_release(self):
        record = {"evidence_tier": "repeated-development-interop"}
        with self.assertRaisesRegex(
            evidence_tier.EvidenceTierError,
            "lower-tier record",
        ):
            evidence_tier.assert_release_record_tier(record)

    def test_differential_record_rejected_in_release(self):
        record = {"evidence_tier": "conditional-differential"}
        with self.assertRaisesRegex(
            evidence_tier.EvidenceTierError,
            "lower-tier record",
        ):
            evidence_tier.assert_release_record_tier(record)

    def test_missing_tier_rejected_in_release(self):
        record = {"direction": "i2pr-to-i2pd-ipv4"}
        with self.assertRaisesRegex(
            evidence_tier.EvidenceTierError,
            "lower-tier record",
        ):
            evidence_tier.assert_release_record_tier(record)

    def test_non_dict_rejected_in_release(self):
        with self.assertRaises(evidence_tier.EvidenceTierError):
            evidence_tier.assert_release_record_tier(["release-qualification"])
        with self.assertRaises(evidence_tier.EvidenceTierError):
            evidence_tier.assert_release_record_tier("release-qualification")


class SmokePromotionGuardTests(unittest.TestCase):
    """A smoke record cannot masquerade as a stronger tier."""

    def test_release_record_not_blocked_by_development_guard(self):
        record = {"evidence_tier": "release-qualification"}
        self.assertFalse(
            evidence_tier.development_record_cannot_satisfy_release(record),
        )

    def test_smoke_record_blocked_by_development_guard(self):
        record = {"evidence_tier": "external-loopback-smoke"}
        self.assertTrue(
            evidence_tier.development_record_cannot_satisfy_release(record),
        )

    def test_smoke_record_pretending_release_blocked(self):
        record = {
            "schema": "i2pr-ntcp2-loopback-smoke-v1",
            "evidence_tier": "release-qualification",
        }
        self.assertTrue(
            evidence_tier.smoke_record_must_not_pretend_release(record),
        )

    def test_release_record_with_release_schema_not_a_smoke(self):
        record = {
            "schema": "i2pr-milestone3-certificate-v1",
            "evidence_tier": "release-qualification",
        }
        self.assertFalse(
            evidence_tier.smoke_record_must_not_pretend_release(record),
        )


class Adr0023LinkageTests(unittest.TestCase):
    """The evidence-tier module references the active ADR 0023 authority."""

    def test_module_docstring_references_adr_0023(self):
        doc = evidence_tier.__doc__ or ""
        self.assertIn("ADR 0023", doc)
        self.assertIn("0023-staged-ntcp2-interoperability-evidence", doc)


if __name__ == "__main__":
    unittest.main()