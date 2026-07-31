"""Plan 068/069 Level 1 host-loopback smoke record contract.

The Level 1 record answers whether i2pr can complete one NTCP2 and
exchange one exact I2NP message with an independent implementation on
the current host loopback. The record is diagnostic/development
evidence only; it cannot satisfy a release-qualification predicate
(see :mod:`evidence_tier`).

Schema: ``i2pr-ntcp2-loopback-smoke-v1``.

Required fields:

- ``schema`` (constant ``i2pr-ntcp2-loopback-smoke-v1``).
- ``schema_version`` (constant ``1``).
- ``evidence_tier`` (must be ``external-loopback-smoke``).
- ``run_id`` (non-empty string, sanitized).
- ``source_commit`` (40-lowercase-hex SHA-1).
- ``reference_name`` (allowlisted: ``i2pd`` or ``java_i2p``).
- ``reference_version`` (e.g., ``2.60.0`` or ``2.12.0``).
- ``reference_revision`` (40-lowercase-hex or 64-lowercase-hex).
- ``direction`` (allowlisted: ``i2pr-to-i2pd-ipv4``,
  ``i2pd-to-i2pr-ipv4``, ``i2pr-to-java-ipv4``,
  ``java-to-i2pr-ipv4``).
- ``started_utc`` (RFC 3339 string).
- ``completed_utc`` (RFC 3339 string).
- ``local_router_hash_sha256`` (64-lowercase-hex).
- ``peer_router_hash_sha256`` (64-lowercase-hex).
- ``delivery_status_message_id`` (nonzero u32).
- ``tcp_connected`` (bool).
- ``ntcp2_authenticated`` (bool).
- ``frame_emitted`` (bool).
- ``frame_authenticated_and_decrypted`` (bool).
- ``i2np_message_decoded`` (bool).
- ``cleanup_clean`` (bool).
- ``network_audit`` (``strace-allowlist`` | ``configuration-only`` |
  ``not-run``).
- ``result`` (``passed`` | ``failed`` | ``blocked``).
- ``failure_stage`` (bounded set; ``none`` when ``result = passed``).
- ``failure_reason`` (free-form bounded; ``none`` when ``result = passed``).
- ``record_sha256`` (64-lowercase-hex SHA-256 of canonical JSON excluding
  itself).

Rules:

- A passed record requires every positive boolean field to be ``True``,
  ``cleanup_clean = True``, and ``network_audit != not-run``.
- A passed record must not include raw payload, private key, Noise
  state, or full RouterInfo bytes.
- ``record_sha256`` covers the canonical JSON serialization excluding
  itself.
- The record is diagnostic/development evidence only.
"""

from __future__ import annotations

import hashlib
import json
import re
from typing import Any, Final


SCHEMA: Final[str] = "i2pr-ntcp2-loopback-smoke-v1"
SCHEMA_VERSION: Final[int] = 1

EVIDENCE_TIER: Final[str] = "external-loopback-smoke"


DIRECTIONS: Final[frozenset[str]] = frozenset({
    "i2pr-to-i2pd-ipv4",
    "i2pd-to-i2pr-ipv4",
    "i2pr-to-java-ipv4",
    "java-to-i2pr-ipv4",
})


REFERENCE_NAMES: Final[frozenset[str]] = frozenset({"i2pd", "java_i2p"})


NETWORK_AUDIT_VALUES: Final[frozenset[str]] = frozenset({
    "strace-allowlist",
    "configuration-only",
    "not-run",
})


RESULT_VALUES: Final[frozenset[str]] = frozenset({
    "passed",
    "failed",
    "blocked",
})


FAILURE_STAGE_VALUES: Final[frozenset[str]] = frozenset({
    "none",
    "preflight",
    "build",
    "process-start",
    "router-info",
    "connect",
    "handshake-request",
    "handshake-created",
    "handshake-confirmed",
    "data-frame-write",
    "data-frame-authentication",
    "i2np-decode",
    "correlation",
    "cleanup",
    "network-audit",
    "timeout",
})


FORBIDDEN_FIELDS: Final[frozenset[str]] = frozenset({
    "raw_payload",
    "private_key",
    "noise_state",
    "router_info_bytes",
    "session_state",
    "static_key",
    "router_identity",
})


SHA1_RE: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{64}$")
RFC3339_RE: Final[re.Pattern[str]] = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$",
)


REQUIRED_FIELDS: Final[tuple[str, ...]] = (
    "schema",
    "schema_version",
    "evidence_tier",
    "run_id",
    "source_commit",
    "reference_name",
    "reference_version",
    "reference_revision",
    "direction",
    "started_utc",
    "completed_utc",
    "local_router_hash_sha256",
    "peer_router_hash_sha256",
    "delivery_status_message_id",
    "tcp_connected",
    "ntcp2_authenticated",
    "frame_emitted",
    "frame_authenticated_and_decrypted",
    "i2np_message_decoded",
    "cleanup_clean",
    "network_audit",
    "result",
    "failure_stage",
    "failure_reason",
    "record_sha256",
)


ALLOWED_FIELDS: Final[frozenset[str]] = frozenset(REQUIRED_FIELDS)


class LoopbackSmokeRecordError(ValueError):
    """Raised when a smoke record violates the contract."""


def _require(cond: bool, msg: str) -> None:
    if not cond:
        raise LoopbackSmokeRecordError(msg)


def canonical_record_digest(record: dict[str, Any]) -> str:
    """Return the canonical SHA-256 digest excluding ``record_sha256``."""

    payload = {k: v for k, v in record.items() if k != "record_sha256"}
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def validate_loopback_smoke_record(record: Any) -> dict[str, Any]:
    """Validate a smoke record and return the normalized dict.

    Raises :class:`LoopbackSmokeRecordError` on any contract violation.
    """

    _require(isinstance(record, dict), "smoke record must be a JSON object")
    record = dict(record)

    for field_name in REQUIRED_FIELDS:
        _require(field_name in record, f"smoke record missing field: {field_name}")

    for forbidden in FORBIDDEN_FIELDS:
        _require(
            forbidden not in record,
            f"smoke record forbids secret-bearing field: {forbidden}",
        )

    for field_name in record:
        _require(
            field_name in ALLOWED_FIELDS,
            f"smoke record forbids unknown field: {field_name}",
        )

    _require(record["schema"] == SCHEMA, "smoke record schema mismatch")
    _require(
        record["schema_version"] == SCHEMA_VERSION,
        "smoke record schema_version mismatch",
    )
    _require(
        record["evidence_tier"] == EVIDENCE_TIER,
        "smoke record evidence_tier must be external-loopback-smoke",
    )
    _require(
        isinstance(record["run_id"], str) and record["run_id"],
        "smoke record run_id must be non-empty string",
    )
    _require(
        SHA1_RE.match(record["source_commit"]) is not None,
        "smoke record source_commit must be 40 lowercase hex",
    )
    _require(
        record["reference_name"] in REFERENCE_NAMES,
        "smoke record reference_name not allowlisted",
    )
    _require(
        isinstance(record["reference_version"], str) and record["reference_version"],
        "smoke record reference_version must be non-empty string",
    )
    _require(
        SHA1_RE.match(record["reference_revision"]) is not None
        or SHA256_RE.match(record["reference_revision"]) is not None,
        "smoke record reference_revision must be 40 or 64 lowercase hex",
    )
    _require(
        record["direction"] in DIRECTIONS,
        "smoke record direction not allowlisted",
    )
    _require(
        RFC3339_RE.match(record["started_utc"]) is not None,
        "smoke record started_utc must be RFC 3339",
    )
    _require(
        RFC3339_RE.match(record["completed_utc"]) is not None,
        "smoke record completed_utc must be RFC 3339",
    )
    _require(
        SHA256_RE.match(record["local_router_hash_sha256"]) is not None,
        "smoke record local_router_hash_sha256 must be 64 lowercase hex",
    )
    _require(
        SHA256_RE.match(record["peer_router_hash_sha256"]) is not None,
        "smoke record peer_router_hash_sha256 must be 64 lowercase hex",
    )
    _require(
        isinstance(record["delivery_status_message_id"], int)
        and not isinstance(record["delivery_status_message_id"], bool),
        "smoke record delivery_status_message_id must be int",
    )
    _require(
        1 <= record["delivery_status_message_id"] <= 0xFFFFFFFF,
        "smoke record delivery_status_message_id must be nonzero u32",
    )
    for bool_field in (
        "tcp_connected",
        "ntcp2_authenticated",
        "frame_emitted",
        "frame_authenticated_and_decrypted",
        "i2np_message_decoded",
        "cleanup_clean",
    ):
        _require(
            isinstance(record[bool_field], bool),
            f"smoke record {bool_field} must be bool",
        )
    _require(
        record["network_audit"] in NETWORK_AUDIT_VALUES,
        "smoke record network_audit not allowlisted",
    )
    _require(
        record["result"] in RESULT_VALUES,
        "smoke record result not allowlisted",
    )
    _require(
        record["failure_stage"] in FAILURE_STAGE_VALUES,
        "smoke record failure_stage not allowlisted",
    )
    _require(
        isinstance(record["failure_reason"], str),
        "smoke record failure_reason must be string",
    )

    if record["result"] == "passed":
        for bool_field in (
            "tcp_connected",
            "ntcp2_authenticated",
            "frame_emitted",
            "frame_authenticated_and_decrypted",
            "i2np_message_decoded",
            "cleanup_clean",
        ):
            _require(
                record[bool_field] is True,
                f"passed smoke record requires {bool_field} = true",
            )
        _require(
            record["network_audit"] != "not-run",
            "passed smoke record forbids network_audit = not-run",
        )
        _require(
            record["failure_stage"] == "none",
            "passed smoke record requires failure_stage = none",
        )

    expected_digest = canonical_record_digest(record)
    _require(
        SHA256_RE.match(record["record_sha256"]) is not None,
        "smoke record record_sha256 must be 64 lowercase hex",
    )
    _require(
        record["record_sha256"] == expected_digest,
        "smoke record record_sha256 digest mismatch",
    )

    return record