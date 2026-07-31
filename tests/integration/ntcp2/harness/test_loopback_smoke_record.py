"""Plan 068/069 Level 1 loopback smoke record contract tests."""

from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path

if __package__ in {None, ""}:
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import loopback_smoke_record as smoke


def _positive_fixture() -> dict:
    """Build a fully valid positive smoke record."""

    record: dict = {
        "schema": smoke.SCHEMA,
        "schema_version": smoke.SCHEMA_VERSION,
        "evidence_tier": smoke.EVIDENCE_TIER,
        "run_id": "smoke-2026-07-31-i2pr-to-i2pd",
        "source_commit": "a" * 40,
        "reference_name": "i2pd",
        "reference_version": "2.60.0",
        "reference_revision": "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e",
        "direction": "i2pr-to-i2pd-ipv4",
        "started_utc": "2026-07-31T12:00:00Z",
        "completed_utc": "2026-07-31T12:00:30Z",
        "local_router_hash_sha256": "0" * 64,
        "peer_router_hash_sha256": "1" * 64,
        "delivery_status_message_id": 0x0420_0002,
        "tcp_connected": True,
        "ntcp2_authenticated": True,
        "frame_emitted": True,
        "frame_authenticated_and_decrypted": True,
        "i2np_message_decoded": True,
        "cleanup_clean": True,
        "network_audit": "configuration-only",
        "result": "passed",
        "failure_stage": "none",
        "failure_reason": "none",
        "record_sha256": "",
    }
    record["record_sha256"] = smoke.canonical_record_digest(record)
    return record


class PositiveFixtureTests(unittest.TestCase):
    """The canonical positive fixture validates."""

    def test_positive_fixture_validates(self):
        record = _positive_fixture()
        out = smoke.validate_loopback_smoke_record(record)
        self.assertEqual(out["schema"], smoke.SCHEMA)


class FieldPresenceTests(unittest.TestCase):
    """Required fields are enforced strictly."""

    def test_missing_field_rejected(self):
        record = _positive_fixture()
        del record["direction"]
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "smoke record missing field: direction",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_unknown_field_rejected(self):
        record = _positive_fixture()
        record["unexpected_field"] = "value"
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "unknown field",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_arbitrary_secret_field_rejected(self):
        record = _positive_fixture()
        record["secret_session_key"] = "value"
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "unknown field",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_non_dict_rejected(self):
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "JSON object",
        ):
            smoke.validate_loopback_smoke_record(["not", "a", "dict"])

    def test_schema_marker_mismatch_rejected(self):
        record = _positive_fixture()
        record["schema"] = "i2pr-ntcp2-loopback-smoke-v2"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "schema mismatch",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_schema_version_mismatch_rejected(self):
        record = _positive_fixture()
        record["schema_version"] = 99
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "schema_version mismatch",
        ):
            smoke.validate_loopback_smoke_record(record)


class TierAndMetadataTests(unittest.TestCase):
    """Tier, run_id, source_commit, and direction are bounded."""

    def test_unknown_evidence_tier_rejected(self):
        record = _positive_fixture()
        record["evidence_tier"] = "release-qualification"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "evidence_tier",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_empty_run_id_rejected(self):
        record = _positive_fixture()
        record["run_id"] = ""
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "run_id",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_source_commit_must_be_sha1_width(self):
        record = _positive_fixture()
        record["source_commit"] = "ab" * 32
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "40 lowercase hex",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_unknown_direction_rejected(self):
        record = _positive_fixture()
        record["direction"] = "i2pr-to-emissary-ipv4"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "direction not allowlisted",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_unknown_reference_name_rejected(self):
        record = _positive_fixture()
        record["reference_name"] = "emissary"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "reference_name not allowlisted",
        ):
            smoke.validate_loopback_smoke_record(record)


class TimestampAndHashTests(unittest.TestCase):
    """Timestamps and Router Hashes follow the bounded contract."""

    def test_invalid_started_utc_rejected(self):
        record = _positive_fixture()
        record["started_utc"] = "yesterday"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "RFC 3339",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_40_hex_local_router_hash_rejected(self):
        record = _positive_fixture()
        record["local_router_hash_sha256"] = "f" * 40
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "64 lowercase hex",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_invalid_peer_router_hash_rejected(self):
        record = _positive_fixture()
        record["peer_router_hash_sha256"] = "not-a-hash"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "64 lowercase hex",
        ):
            smoke.validate_loopback_smoke_record(record)


class MessageIdTests(unittest.TestCase):
    """The DeliveryStatus message_id is a nonzero u32."""

    def test_zero_message_id_rejected(self):
        record = _positive_fixture()
        record["delivery_status_message_id"] = 0
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "nonzero u32",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_overflow_message_id_rejected(self):
        record = _positive_fixture()
        record["delivery_status_message_id"] = 0x1_0000_0000
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "nonzero u32",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_bool_message_id_rejected(self):
        record = _positive_fixture()
        record["delivery_status_message_id"] = True
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "must be int",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_string_message_id_rejected(self):
        record = _positive_fixture()
        record["delivery_status_message_id"] = "12345"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "must be int",
        ):
            smoke.validate_loopback_smoke_record(record)


class PassedRecordTests(unittest.TestCase):
    """A passed record requires every positive boolean and clean cleanup."""

    def test_passed_requires_tcp_connected(self):
        record = _positive_fixture()
        record["tcp_connected"] = False
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "tcp_connected",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_passed_requires_ntcp2_authenticated(self):
        record = _positive_fixture()
        record["ntcp2_authenticated"] = False
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "ntcp2_authenticated",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_passed_requires_frame_emitted(self):
        record = _positive_fixture()
        record["frame_emitted"] = False
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "frame_emitted",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_passed_requires_frame_authenticated(self):
        record = _positive_fixture()
        record["frame_authenticated_and_decrypted"] = False
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "frame_authenticated_and_decrypted",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_passed_requires_i2np_decoded(self):
        record = _positive_fixture()
        record["i2np_message_decoded"] = False
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "i2np_message_decoded",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_passed_requires_cleanup_clean(self):
        record = _positive_fixture()
        record["cleanup_clean"] = False
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "cleanup_clean",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_passed_forbids_network_audit_not_run(self):
        record = _positive_fixture()
        record["network_audit"] = "not-run"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "network_audit = not-run",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_passed_allows_strace_allowlist(self):
        record = _positive_fixture()
        record["network_audit"] = "strace-allowlist"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        smoke.validate_loopback_smoke_record(record)


class BlockedRecordTests(unittest.TestCase):
    """A blocked record may report a typed preflight reason."""

    def test_blocked_with_preflight_stage_accepted(self):
        record = _positive_fixture()
        record["result"] = "blocked"
        record["failure_stage"] = "preflight"
        record["failure_reason"] = "blocked_unprivileged_user_namespace"
        record["tcp_connected"] = False
        record["ntcp2_authenticated"] = False
        record["frame_emitted"] = False
        record["frame_authenticated_and_decrypted"] = False
        record["i2np_message_decoded"] = False
        record["cleanup_clean"] = False
        record["network_audit"] = "not-run"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        smoke.validate_loopback_smoke_record(record)

    def test_unknown_failure_stage_rejected(self):
        record = _positive_fixture()
        record["result"] = "blocked"
        record["failure_stage"] = "ghost-stage"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "failure_stage not allowlisted",
        ):
            smoke.validate_loopback_smoke_record(record)


class SecretFieldRejectionTests(unittest.TestCase):
    """Raw payload, private key, Noise state, RouterInfo bytes are forbidden."""

    def test_raw_payload_field_rejected(self):
        record = _positive_fixture()
        record["raw_payload"] = "abcd"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "secret-bearing field: raw_payload",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_private_key_field_rejected(self):
        record = _positive_fixture()
        record["private_key"] = "abcd"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "secret-bearing field: private_key",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_noise_state_field_rejected(self):
        record = _positive_fixture()
        record["noise_state"] = "abcd"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "secret-bearing field: noise_state",
        ):
            smoke.validate_loopback_smoke_record(record)

    def test_router_info_bytes_field_rejected(self):
        record = _positive_fixture()
        record["router_info_bytes"] = "0102030405"
        record["record_sha256"] = smoke.canonical_record_digest(record)
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "secret-bearing field: router_info_bytes",
        ):
            smoke.validate_loopback_smoke_record(record)


class DigestTests(unittest.TestCase):
    """The :func:`canonical_record_digest` helper is reproducible."""

    def test_digest_changes_when_field_changes(self):
        record = _positive_fixture()
        first = record["record_sha256"]
        record2 = copy.deepcopy(record)
        record2["failure_reason"] = "none-but-typo"
        record2["record_sha256"] = smoke.canonical_record_digest(record2)
        self.assertNotEqual(first, record2["record_sha256"])

    def test_digest_mismatch_rejected(self):
        record = _positive_fixture()
        record["record_sha256"] = "f" * 64
        with self.assertRaisesRegex(
            smoke.LoopbackSmokeRecordError,
            "record_sha256 digest mismatch",
        ):
            smoke.validate_loopback_smoke_record(record)


if __name__ == "__main__":
    unittest.main()