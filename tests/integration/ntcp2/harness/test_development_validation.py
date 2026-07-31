"""Plan 068/071 Level 2 development validation summary contract tests."""

from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import development_validation as dev


def _direction_entry(direction: str, passes: int = 3, attempts: int = 3) -> dict:
    return {
        "direction": direction,
        "attempts": attempts,
        "passes": passes,
        "observed_message_id": 0x0420_0003,
        "observed_local_router_hash_sha256": "2" * 64,
        "observed_peer_router_hash_sha256": "3" * 64,
    }


def _positive_fixture() -> dict:
    record = {
        "schema": dev.SCHEMA,
        "schema_version": dev.SCHEMA_VERSION,
        "evidence_tier": dev.EVIDENCE_TIER,
        "source_commit": "c" * 40,
        "reference_name": "i2pd",
        "reference_version": "2.60.0",
        "reference_revision": "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
        "directions": {
            "i2pr-to-i2pd-ipv4": _direction_entry("i2pr-to-i2pd-ipv4"),
            "i2pd-to-i2pr-ipv4": _direction_entry("i2pd-to-i2pr-ipv4"),
        },
        "required_passes_per_direction": 3,
        "observed_passes_per_direction": {
            "i2pr-to-i2pd-ipv4": 3,
            "i2pd-to-i2pr-ipv4": 3,
        },
        "negative_controls": {
            "network_id_mismatch": "rejected",
            "router_info_or_static_key_mismatch": "rejected",
            "delivery_status_correlation_mismatch": "rejected",
            "unauthenticated_data_phase": "rejected",
        },
        "cleanup_passed": True,
        "network_audit_summary": {
            "i2pr-to-i2pd-ipv4": "configuration-only",
            "i2pd-to-i2pr-ipv4": "configuration-only",
        },
        "status": "passed",
        "summary_sha256": "",
    }
    record["summary_sha256"] = dev.canonical_summary_digest(record)
    return record


class PositiveFixtureTests(unittest.TestCase):
    """The canonical positive fixture validates."""

    def test_positive_fixture_validates(self):
        record = _positive_fixture()
        out = dev.validate_development_validation_summary(record)
        self.assertEqual(out["schema"], dev.SCHEMA)


class FieldPresenceTests(unittest.TestCase):
    """Required fields are enforced strictly."""

    def test_missing_required_field_rejected(self):
        record = _positive_fixture()
        del record["status"]
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "missing field: status",
        ):
            dev.validate_development_validation_summary(record)

    def test_unknown_field_rejected(self):
        record = _positive_fixture()
        record["unexpected_field"] = "value"
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "forbids unknown field",
        ):
            dev.validate_development_validation_summary(record)

    def test_non_dict_rejected(self):
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "JSON object",
        ):
            dev.validate_development_validation_summary("not a dict")


class TierAndReferenceTests(unittest.TestCase):
    """Tier, source_commit, and reference metadata are bounded."""

    def test_unknown_tier_rejected(self):
        record = _positive_fixture()
        record["evidence_tier"] = "external-loopback-smoke"
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "evidence_tier",
        ):
            dev.validate_development_validation_summary(record)

    def test_source_commit_must_be_sha1_width(self):
        record = _positive_fixture()
        record["source_commit"] = "ab" * 32
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "40 lowercase hex",
        ):
            dev.validate_development_validation_summary(record)

    def test_unknown_reference_name_rejected(self):
        record = _positive_fixture()
        record["reference_name"] = "emissary"
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "reference_name not allowlisted",
        ):
            dev.validate_development_validation_summary(record)

    def test_empty_directions_rejected(self):
        record = _positive_fixture()
        record["directions"] = {}
        record["observed_passes_per_direction"] = {}
        record["network_audit_summary"] = {}
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "non-empty mapping",
        ):
            dev.validate_development_validation_summary(record)


class PerDirectionTests(unittest.TestCase):
    """Per-direction entries follow the bounded contract."""

    def test_unknown_direction_rejected(self):
        record = _positive_fixture()
        record["directions"] = {
            "i2pr-to-emissary-ipv4": _direction_entry("i2pr-to-emissary-ipv4"),
        }
        record["observed_passes_per_direction"] = {
            "i2pr-to-emissary-ipv4": 3,
        }
        record["network_audit_summary"] = {
            "i2pr-to-emissary-ipv4": "configuration-only",
        }
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "direction not allowlisted",
        ):
            dev.validate_development_validation_summary(record)

    def test_attempts_must_be_positive(self):
        record = _positive_fixture()
        record["directions"]["i2pr-to-i2pd-ipv4"]["attempts"] = 0
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "attempts must be positive",
        ):
            dev.validate_development_validation_summary(record)

    def test_passes_must_be_le_attempts(self):
        record = _positive_fixture()
        record["directions"]["i2pr-to-i2pd-ipv4"]["passes"] = 4
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "passes must be int",
        ):
            dev.validate_development_validation_summary(record)

    def test_observed_message_id_zero_rejected(self):
        record = _positive_fixture()
        record["directions"]["i2pr-to-i2pd-ipv4"]["observed_message_id"] = 0
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "nonzero u32",
        ):
            dev.validate_development_validation_summary(record)

    def test_observed_local_router_hash_must_be_64_hex(self):
        record = _positive_fixture()
        record["directions"]["i2pr-to-i2pd-ipv4"][
            "observed_local_router_hash_sha256"
        ] = "f" * 40
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "64 lowercase hex",
        ):
            dev.validate_development_validation_summary(record)


class PassedSummaryTests(unittest.TestCase):
    """A passed summary must meet every Plan 071 criterion."""

    def test_passed_requires_required_passes_per_direction(self):
        record = _positive_fixture()
        record["observed_passes_per_direction"]["i2pr-to-i2pd-ipv4"] = 2
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "requires i2pr-to-i2pd-ipv4 passes",
        ):
            dev.validate_development_validation_summary(record)

    def test_passed_requires_cleanup_passed(self):
        record = _positive_fixture()
        record["cleanup_passed"] = False
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "cleanup_passed = true",
        ):
            dev.validate_development_validation_summary(record)

    def test_passed_requires_network_audit(self):
        record = _positive_fixture()
        record["network_audit_summary"]["i2pr-to-i2pd-ipv4"] = "not-run"
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "audit = not-run",
        ):
            dev.validate_development_validation_summary(record)

    def test_passed_requires_negative_controls(self):
        record = _positive_fixture()
        del record["negative_controls"]["unauthenticated_data_phase"]
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "negative control: unauthenticated_data_phase",
        ):
            dev.validate_development_validation_summary(record)

    def test_passed_requires_negative_control_outcome_rejected(self):
        record = _positive_fixture()
        record["negative_controls"]["unauthenticated_data_phase"] = "passed"
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "unauthenticated_data_phase = rejected",
        ):
            dev.validate_development_validation_summary(record)

    def test_passed_requires_strace_allowlist(self):
        record = _positive_fixture()
        record["network_audit_summary"]["i2pr-to-i2pd-ipv4"] = "strace-allowlist"
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        dev.validate_development_validation_summary(record)


class BlockedSummaryTests(unittest.TestCase):
    """A blocked or failed summary does not require the Plan 071 passes."""

    def test_blocked_with_zero_passes_accepted(self):
        record = _positive_fixture()
        record["status"] = "blocked"
        record["observed_passes_per_direction"] = {
            "i2pr-to-i2pd-ipv4": 0,
            "i2pd-to-i2pr-ipv4": 0,
        }
        record["directions"]["i2pr-to-i2pd-ipv4"]["passes"] = 0
        record["directions"]["i2pd-to-i2pr-ipv4"]["passes"] = 0
        record["cleanup_passed"] = False
        record["negative_controls"] = {}
        record["network_audit_summary"] = {
            "i2pr-to-i2pd-ipv4": "not-run",
            "i2pd-to-i2pr-ipv4": "not-run",
        }
        record["summary_sha256"] = dev.canonical_summary_digest(record)
        dev.validate_development_validation_summary(record)


class DigestTests(unittest.TestCase):
    """The :func:`canonical_summary_digest` helper is reproducible."""

    def test_digest_changes_when_field_changes(self):
        record = _positive_fixture()
        first = record["summary_sha256"]
        record2 = copy.deepcopy(record)
        record2["status"] = "failed"
        record2["summary_sha256"] = dev.canonical_summary_digest(record2)
        self.assertNotEqual(first, record2["summary_sha256"])

    def test_digest_mismatch_rejected(self):
        record = _positive_fixture()
        record["summary_sha256"] = "f" * 64
        with self.assertRaisesRegex(
            dev.DevelopmentValidationError,
            "summary_sha256 digest mismatch",
        ):
            dev.validate_development_validation_summary(record)


if __name__ == "__main__":
    unittest.main()