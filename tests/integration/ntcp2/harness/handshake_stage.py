"""Plan 092 privacy-safe handshake stage observation contract.

The observation schema is owned by the runtime/tooling boundary and
never carries raw payload, private key material, Noise state,
transcript bytes, ciphertext, plaintext, RouterInfo bytes, packet
captures, or arbitrary remote error text. It records only operation
metadata: the bounded stage name, expected/completed octet counts,
the typed I/O result, and the current invocation/run correlation
digests. Two closed allowlists are exposed:

- :data:`I2PR_HANDSHAKE_STAGES` — the bounded set of allowed
  initiator (i2pr) stages emitted by the runtime handshake driver.
- :data:`I2PD_HANDSHAKE_STAGES` — the bounded set of allowed
  responder (i2pd) stages emitted by the pinned i2pd transport
  observer patch.

The schema refuses unknown stages, unknown fields, oversized counts,
negative counts, malformed digests, mismatched direction, mismatched
run/invocation correlation, and observation payloads that contain
any of the forbidden substrings. Event canonicalization produces a
stable SHA-256 digest for every sanitized event so the runner can
correlate observations by ``event_sha256`` without retaining the
underlying payload.
"""

from __future__ import annotations

import hashlib
import json
import re
from typing import Any, Final


SCHEMA: Final[str] = "i2pr-ntcp2-handshake-stage-v1"
SCHEMA_VERSION: Final[int] = 1


DIRECTION: Final[str] = "i2pr-to-i2pd-ipv4"
SOURCE_SIDE_I2PR: Final[str] = "i2pr"
SOURCE_SIDE_I2PD: Final[str] = "i2pd"
ALLOWED_SOURCE_SIDES: Final[frozenset[str]] = frozenset({SOURCE_SIDE_I2PR, SOURCE_SIDE_I2PD})

IO_RESULT_NOT_APPLICABLE: Final[str] = "not-applicable"
IO_RESULT_COMPLETED: Final[str] = "completed"
IO_RESULT_EOF: Final[str] = "eof"
IO_RESULT_CLOSED: Final[str] = "closed"
IO_RESULT_TIMEOUT: Final[str] = "timeout"
IO_RESULT_CANCELLED: Final[str] = "cancelled"
IO_RESULT_FAILED: Final[str] = "failed"
ALLOWED_IO_RESULTS: Final[frozenset[str]] = frozenset(
    {
        IO_RESULT_NOT_APPLICABLE,
        IO_RESULT_COMPLETED,
        IO_RESULT_EOF,
        IO_RESULT_CLOSED,
        IO_RESULT_TIMEOUT,
        IO_RESULT_CANCELLED,
        IO_RESULT_FAILED,
    }
)


I2PR_HANDSHAKE_STAGES: Final[tuple[str, ...]] = (
    "initiator_state_initialized",
    "session_request_encode_started",
    "session_request_encode_completed",
    "session_request_write_started",
    "session_request_write_completed",
    "session_request_write_failed",
    "session_created_read_started",
    "session_created_read_completed",
    "session_created_read_eof",
    "session_created_process_started",
    "session_created_process_completed",
    "session_created_process_failed",
    "session_confirmed_write_started",
    "session_confirmed_write_completed",
    "noise_authenticated",
)
ALLOWED_I2PR_STAGES: Final[frozenset[str]] = frozenset(I2PR_HANDSHAKE_STAGES)


I2PD_HANDSHAKE_STAGES: Final[tuple[str, ...]] = (
    "tcp_accepted",
    "session_request_prefix_read_started",
    "session_request_prefix_read_completed",
    "session_request_prefix_read_eof",
    "session_request_body_read_started",
    "session_request_body_read_completed",
    "session_request_body_read_eof",
    "session_request_process_started",
    "session_request_process_completed",
    "session_request_process_failed",
    "session_created_write_started",
    "session_created_write_completed",
    "session_created_write_failed",
    "session_confirmed_read_started",
    "session_confirmed_read_completed",
    "session_confirmed_process_failed",
    "noise_authenticated",
)
ALLOWED_I2PD_STAGES: Final[frozenset[str]] = frozenset(I2PD_HANDSHAKE_STAGES)


REQUIRED_FIELDS: Final[tuple[str, ...]] = (
    "schema",
    "schema_version",
    "run_id",
    "direction",
    "source_side",
    "invocation_id",
    "event_sequence",
    "stage",
    "elapsed_millis",
    "event_sha256",
)

OPTIONAL_FIELDS: Final[tuple[str, ...]] = (
    "expected_octets",
    "completed_octets",
    "io_result",
    "peer_router_hash_sha256",
)

FORBIDDEN_FIELDS: Final[frozenset[str]] = frozenset(
    {
        "raw",
        "payload",
        "payload_hex",
        "transcript",
        "transcript_hex",
        "ciphertext",
        "plaintext",
        "key",
        "private_key",
        "ephemeral_private",
        "nonce",
        "iv",
        "padding_contents",
        "router_info_bytes",
        "packet_capture",
        "raw_bytes",
        "raw_octets",
        "raw_buffer",
        "socket_address",
    }
)

# Bounded maximum value for expected/completed octet counts and elapsed
# milliseconds. The cap reflects the largest single NTCP2 message
# body plus a small margin so the schema refuses unbounded values.
MAX_OCTETS: Final[int] = 65_536
MAX_ELAPSED_MILLIS: Final[int] = 600_000


RUN_ID_RE: Final[re.Pattern[str]] = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?$")
HEX64: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{64}$")


class HandshakeStageObservationError(ValueError):
    """Raised when an observation violates the privacy-safe contract."""


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise HandshakeStageObservationError(message)


def canonical_event_digest(event: dict[str, Any]) -> str:
    """Return the canonical SHA-256 digest of a sanitized event.

    The canonical serialization excludes the ``event_sha256`` field
    so the digest covers exactly the typed observation payload that
    the runner ingests.
    """

    payload = {key: value for key, value in event.items() if key != "event_sha256"}
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def validate_observation(observation: Any) -> dict[str, Any]:
    """Validate one observation and return the normalized dict.

    Raises :class:`HandshakeStageObservationError` on any contract
    violation. The returned dict is a shallow copy safe to mutate
    without affecting the original input.
    """

    _require(
        isinstance(observation, dict), "handshake observation must be a JSON object"
    )
    record_copy = dict(observation)

    forbidden_in_record = set(record_copy) & FORBIDDEN_FIELDS
    _require(
        not forbidden_in_record,
        f"handshake observation forbids fields: {forbidden_in_record}",
    )

    for field_name in REQUIRED_FIELDS:
        _require(
            field_name in record_copy,
            f"handshake observation missing required field: {field_name}",
        )

    extra_fields = set(record_copy) - set(REQUIRED_FIELDS) - set(OPTIONAL_FIELDS)
    _require(
        not extra_fields,
        f"handshake observation forbids unknown fields: {extra_fields}",
    )

    _require(record_copy["schema"] == SCHEMA, "handshake observation schema mismatch")
    _require(
        record_copy["schema_version"] == SCHEMA_VERSION,
        "handshake observation schema_version mismatch",
    )

    run_id = record_copy["run_id"]
    _require(
        isinstance(run_id, str)
        and RUN_ID_RE.fullmatch(run_id) is not None,
        "handshake observation run_id must be a bounded lowercase identifier",
    )

    _require(
        record_copy["direction"] == DIRECTION,
        f"handshake observation direction must be {DIRECTION}",
    )

    source_side = record_copy["source_side"]
    _require(
        isinstance(source_side, str) and source_side in ALLOWED_SOURCE_SIDES,
        "handshake observation source_side must be i2pr or i2pd",
    )

    invocation_id = record_copy["invocation_id"]
    _require(
        isinstance(invocation_id, str) and invocation_id,
        "handshake observation invocation_id must be non-empty string",
    )

    event_sequence = record_copy["event_sequence"]
    _require(
        isinstance(event_sequence, int)
        and not isinstance(event_sequence, bool)
        and event_sequence >= 0,
        "handshake observation event_sequence must be non-negative int",
    )

    stage = record_copy["stage"]
    if source_side == SOURCE_SIDE_I2PR:
        _require(
            isinstance(stage, str) and stage in ALLOWED_I2PR_STAGES,
            "handshake observation stage not allowlisted for i2pr",
        )
    else:
        _require(
            isinstance(stage, str) and stage in ALLOWED_I2PD_STAGES,
            "handshake observation stage not allowlisted for i2pd",
        )

    elapsed_millis = record_copy["elapsed_millis"]
    _require(
        isinstance(elapsed_millis, int)
        and not isinstance(elapsed_millis, bool)
        and 0 <= elapsed_millis <= MAX_ELAPSED_MILLIS,
        f"handshake observation elapsed_millis must be 0..={MAX_ELAPSED_MILLIS}",
    )

    if "expected_octets" in record_copy:
        expected_octets = record_copy["expected_octets"]
        _require(
            isinstance(expected_octets, int)
            and not isinstance(expected_octets, bool)
            and 0 <= expected_octets <= MAX_OCTETS,
            f"handshake observation expected_octets must be 0..={MAX_OCTETS}",
        )

    if "completed_octets" in record_copy:
        completed_octets = record_copy["completed_octets"]
        _require(
            isinstance(completed_octets, int)
            and not isinstance(completed_octets, bool)
            and 0 <= completed_octets <= MAX_OCTETS,
            f"handshake observation completed_octets must be 0..={MAX_OCTETS}",
        )
        if "expected_octets" in record_copy:
            _require(
                completed_octets <= record_copy["expected_octets"],
                "handshake observation completed_octets must not exceed expected_octets",
            )

    if "io_result" in record_copy:
        io_result = record_copy["io_result"]
        _require(
            isinstance(io_result, str) and io_result in ALLOWED_IO_RESULTS,
            "handshake observation io_result not allowlisted",
        )

    if "peer_router_hash_sha256" in record_copy:
        peer_router_hash_sha256 = record_copy["peer_router_hash_sha256"]
        _require(
            isinstance(peer_router_hash_sha256, str)
            and HEX64.fullmatch(peer_router_hash_sha256) is not None,
            "handshake observation peer_router_hash_sha256 must be 64 lowercase hex",
        )

    digest = record_copy["event_sha256"]
    _require(
        isinstance(digest, str) and HEX64.fullmatch(digest) is not None,
        "handshake observation event_sha256 must be 64 lowercase hex",
    )

    expected_digest = canonical_event_digest(record_copy)
    _require(
        digest == expected_digest,
        "handshake observation event_sha256 digest mismatch",
    )

    return record_copy


def build_observation(
    *,
    run_id: str,
    direction: str,
    source_side: str,
    invocation_id: str,
    event_sequence: int,
    stage: str,
    elapsed_millis: int,
    expected_octets: int | None = None,
    completed_octets: int | None = None,
    io_result: str | None = None,
    peer_router_hash_sha256: str | None = None,
) -> dict[str, Any]:
    """Build one finalized observation and return the dict.

    The function validates every field, normalizes optional fields
    to ``None`` when omitted, and computes the canonical
    ``event_sha256`` digest over the sanitized payload.
    """

    _require(
        isinstance(run_id, str) and RUN_ID_RE.fullmatch(run_id) is not None,
        "build_observation: run_id invalid",
    )
    _require(
        direction == DIRECTION,
        f"build_observation: direction must be {DIRECTION}",
    )
    _require(
        isinstance(source_side, str) and source_side in ALLOWED_SOURCE_SIDES,
        "build_observation: source_side must be i2pr or i2pd",
    )
    _require(
        isinstance(invocation_id, str) and invocation_id,
        "build_observation: invocation_id must be non-empty string",
    )
    _require(
        isinstance(event_sequence, int)
        and not isinstance(event_sequence, bool)
        and event_sequence >= 0,
        "build_observation: event_sequence must be non-negative int",
    )
    if source_side == SOURCE_SIDE_I2PR:
        _require(
            isinstance(stage, str) and stage in ALLOWED_I2PR_STAGES,
            "build_observation: stage not allowlisted for i2pr",
        )
    else:
        _require(
            isinstance(stage, str) and stage in ALLOWED_I2PD_STAGES,
            "build_observation: stage not allowlisted for i2pd",
        )
    _require(
        isinstance(elapsed_millis, int)
        and not isinstance(elapsed_millis, bool)
        and 0 <= elapsed_millis <= MAX_ELAPSED_MILLIS,
        f"build_observation: elapsed_millis must be 0..={MAX_ELAPSED_MILLIS}",
    )

    record: dict[str, Any] = {
        "schema": SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "run_id": run_id,
        "direction": direction,
        "source_side": source_side,
        "invocation_id": invocation_id,
        "event_sequence": int(event_sequence),
        "stage": stage,
        "elapsed_millis": int(elapsed_millis),
    }
    if expected_octets is not None:
        _require(
            isinstance(expected_octets, int)
            and not isinstance(expected_octets, bool)
            and 0 <= expected_octets <= MAX_OCTETS,
            f"build_observation: expected_octets must be 0..={MAX_OCTETS}",
        )
        record["expected_octets"] = int(expected_octets)
    if completed_octets is not None:
        _require(
            isinstance(completed_octets, int)
            and not isinstance(completed_octets, bool)
            and 0 <= completed_octets <= MAX_OCTETS,
            f"build_observation: completed_octets must be 0..={MAX_OCTETS}",
        )
        if expected_octets is not None:
            _require(
                completed_octets <= expected_octets,
                "build_observation: completed_octets must not exceed expected_octets",
            )
        record["completed_octets"] = int(completed_octets)
    if io_result is not None:
        _require(
            isinstance(io_result, str) and io_result in ALLOWED_IO_RESULTS,
            "build_observation: io_result not allowlisted",
        )
        record["io_result"] = io_result
    if peer_router_hash_sha256 is not None:
        _require(
            isinstance(peer_router_hash_sha256, str)
            and HEX64.fullmatch(peer_router_hash_sha256) is not None,
            "build_observation: peer_router_hash_sha256 must be 64 lowercase hex",
        )
        record["peer_router_hash_sha256"] = peer_router_hash_sha256
    record["event_sha256"] = canonical_event_digest(record)
    validate_observation(record)
    return record


__all__ = [
    "ALLOWED_IO_RESULTS",
    "ALLOWED_I2PD_STAGES",
    "ALLOWED_I2PR_STAGES",
    "ALLOWED_SOURCE_SIDES",
    "DIRECTION",
    "FORBIDDEN_FIELDS",
    "HandshakeStageObservationError",
    "I2PD_HANDSHAKE_STAGES",
    "I2PR_HANDSHAKE_STAGES",
    "IO_RESULT_CANCELLED",
    "IO_RESULT_CLOSED",
    "IO_RESULT_COMPLETED",
    "IO_RESULT_EOF",
    "IO_RESULT_FAILED",
    "IO_RESULT_NOT_APPLICABLE",
    "IO_RESULT_TIMEOUT",
    "MAX_ELAPSED_MILLIS",
    "MAX_OCTETS",
    "OPTIONAL_FIELDS",
    "REQUIRED_FIELDS",
    "SCHEMA",
    "SCHEMA_VERSION",
    "SOURCE_SIDE_I2PD",
    "SOURCE_SIDE_I2PR",
    "build_observation",
    "canonical_event_digest",
    "validate_observation",
]