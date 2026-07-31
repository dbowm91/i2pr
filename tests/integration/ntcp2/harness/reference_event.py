"""Plan 062 reference event schema v1 and helpers.

This module defines the locked per-side event shape emitted by the
Plan 063 Java direct driver and the Plan 064 i2pd direct driver
plus their observer patch:

```text
schema = i2pr-reference-event-v1
schema_version = 1
```

Every event is a bounded JSON object that records one observable
fact about a reference driver process: process startup, listener
readiness, exported RouterInfo, validated peer RouterInfo, TCP
connection acceptance, NTCP2 authentication, frame emission, frame
authenticated-and-decrypted, I2NP message decoded, terminal clean,
or terminal rejected.

The schema is intentionally source-neutral so the Java adapter and
the i2pd adapter emit the same shape. The Plan 052 evidence bundle
correlates events by ``run_id``, ``scenario_id``, ``direction``,
``event_sequence``, and ``event_sha256``. The bundle cross-check
rejects:

- events from before the run cursor (``event_sequence`` is strictly
  increasing per process);
- events from a peer Router Hash different from the run identity
  peer Router Hash;
- events that name a forbidden path, key, payload, or transcript
  string;
- data-phase events that lack the exact DeliveryStatus
  ``message_id``, the I2NP type, and the frame sequence;
- duplicate ``event_sequence`` per process.

A generic log phrase cannot satisfy any event. The Plan 063 Java
handler emits the data-phase events from the receive-handler
invocation after NTCP2 frame authentication and I2NP conversion.
The Plan 064 i2pd observer emits the same data-phase events from
the post-AEAD, post-conversion seam. A handshake-only run cannot
produce data-phase events.

The on-disk evidence bundle keeps the structured event records
without raw payload bytes, private keys, Noise state, frame keys,
IV state, transcripts, or arbitrary remote error text.
"""

from __future__ import annotations

import hashlib
import json
import re
from enum import Enum
from typing import Any


EVENT_SCHEMA = "i2pr-reference-event-v1"
EVENT_SCHEMA_VERSION = 1


class EventKind(Enum):
    """Bounded event kinds emitted by the Plan 063 and Plan 064 drivers."""

    PROCESS_STARTED = "process_started"
    LISTENER_READY = "listener_ready"
    ROUTER_INFO_EXPORTED = "router_info_exported"
    PEER_ROUTER_INFO_VALIDATED = "peer_router_info_validated"
    TCP_CONNECTED = "tcp_connected"
    NTCP2_AUTHENTICATED = "ntcp2_authenticated"
    FRAME_EMITTED = "frame_emitted"
    FRAME_AUTHENTICATED_AND_DECRYPTED = "frame_authenticated_and_decrypted"
    I2NP_MESSAGE_DECODED = "i2np_message_decoded"
    TERMINAL_CLEAN = "terminal_clean"
    TERMINAL_REJECTED = "terminal_rejected"


_DATA_PHASE_EVENTS = {
    EventKind.FRAME_EMITTED,
    EventKind.FRAME_AUTHENTICATED_AND_DECRYPTED,
    EventKind.I2NP_MESSAGE_DECODED,
}


_REQUIRED_COMMON_FIELDS = (
    "schema",
    "schema_version",
    "run_id",
    "scenario_id",
    "direction",
    "implementation",
    "implementation_revision",
    "driver_binary_sha256",
    "local_router_hash_sha256",
    "peer_router_hash_sha256",
    "monotonic_ms",
    "event_kind",
    "event_sequence",
    "event_sha256",
)


_REQUIRED_DATA_PHASE_FIELDS = (
    "delivery_status_message_id",
    "i2np_type",
    "frame_sequence",
)


_HEX64 = re.compile(r"^[0-9a-f]{64}$")
_REVISION = re.compile(r"^[0-9a-f]{40}$")
_RUN_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?$")
_DIRECTIONS = {
    "i2pr-to-java-ipv4",
    "java-to-i2pr-ipv4",
    "i2pr-to-i2pd-ipv4",
    "i2pd-to-i2pr-ipv4",
}
_KNOWN_DATA_PHASE_I2NP_TYPES = {10}  # DeliveryStatus


class ReferenceEventError(ValueError):
    """Raised when a reference event fails validation."""


def _scan(value: Any) -> None:
    if isinstance(value, str):
        if any(forbidden in value for forbidden in (
            "-----BEGIN", "router.identity", "ntcp2.static.key",
            "/home/", "/root/", "RouterInfo", "I2NP",
        )):
            raise ReferenceEventError("event contains forbidden path or payload text")
    elif isinstance(value, dict):
        for child in value.values():
            _scan(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _scan(child)


def _check_hex64(field: str, value: Any) -> None:
    if not isinstance(value, str) or not _HEX64.fullmatch(value):
        raise ReferenceEventError(f"event-{field}-invalid")


def validate_event(
    event: dict[str, Any],
    *,
    seen_event_sequences: set[int] | None = None,
    expected_peer_router_hash_sha256: str | None = None,
    allow_terminal_only: bool = True,
) -> None:
    """Validate one Plan 062 reference event record.

    ``seen_event_sequences`` enforces the strictly-increasing
    per-process invariant when supplied. The
    ``expected_peer_router_hash_sha256`` argument enforces the
    cross-record Router Hash continuity rule when supplied.
    """

    if not isinstance(event, dict):
        raise ReferenceEventError("event must be a JSON object")
    missing = [field for field in _REQUIRED_COMMON_FIELDS if field not in event]
    if missing:
        raise ReferenceEventError(f"event-record-missing:{','.join(missing)}")
    if event["schema"] != EVENT_SCHEMA:
        raise ReferenceEventError("event-schema-invalid")
    if event["schema_version"] != EVENT_SCHEMA_VERSION:
        raise ReferenceEventError("event-schema-version-invalid")
    _scan(event)
    if not _RUN_ID.fullmatch(event["run_id"]):
        raise ReferenceEventError("event-run-id-invalid")
    if not isinstance(event["scenario_id"], str) or not event["scenario_id"]:
        raise ReferenceEventError("event-scenario-id-missing")
    if event["direction"] not in _DIRECTIONS:
        raise ReferenceEventError("event-direction-not-allowlisted")
    if not isinstance(event["implementation"], str) or not event["implementation"]:
        raise ReferenceEventError("event-implementation-missing")
    if not _REVISION.fullmatch(event["implementation_revision"]):
        raise ReferenceEventError("event-implementation-revision-invalid")
    _check_hex64("driver_binary_sha256", event["driver_binary_sha256"])
    _check_hex64("local_router_hash_sha256", event["local_router_hash_sha256"])
    _check_hex64("peer_router_hash_sha256", event["peer_router_hash_sha256"])
    if expected_peer_router_hash_sha256 is not None and (
        event["peer_router_hash_sha256"] != expected_peer_router_hash_sha256
    ):
        raise ReferenceEventError("event-peer-router-hash-mismatch")
    if not isinstance(event["monotonic_ms"], int) or event["monotonic_ms"] < 0:
        raise ReferenceEventError("event-monotonic-ms-invalid")
    try:
        kind = EventKind(event["event_kind"])
    except ValueError as exc:
        raise ReferenceEventError("event-kind-not-allowlisted") from exc
    if not isinstance(event["event_sequence"], int) or event["event_sequence"] < 0:
        raise ReferenceEventError("event-sequence-invalid")
    if seen_event_sequences is not None:
        if event["event_sequence"] in seen_event_sequences:
            raise ReferenceEventError("event-sequence-duplicate")
        seen_event_sequences.add(event["event_sequence"])
    if not _HEX64.fullmatch(str(event["event_sha256"])):
        raise ReferenceEventError("event-sha256-not-digest")

    if kind in _DATA_PHASE_EVENTS:
        for field in _REQUIRED_DATA_PHASE_FIELDS:
            if field not in event:
                raise ReferenceEventError(f"event-data-phase-missing:{field}")
        message_id = event["delivery_status_message_id"]
        if not isinstance(message_id, int) or isinstance(message_id, bool):
            raise ReferenceEventError("event-delivery-status-message-id-not-int")
        if message_id < 1 or message_id > 0xFFFFFFFF:
            raise ReferenceEventError("event-delivery-status-message-id-out-of-range")
        if not isinstance(event["i2np_type"], int) or event["i2np_type"] not in _KNOWN_DATA_PHASE_I2NP_TYPES:
            raise ReferenceEventError("event-i2np-type-invalid-for-data-phase")
        if not isinstance(event["frame_sequence"], int) or event["frame_sequence"] < 0:
            raise ReferenceEventError("event-frame-sequence-invalid")
    elif allow_terminal_only:
        for field in _REQUIRED_DATA_PHASE_FIELDS:
            if field in event:
                raise ReferenceEventError(
                    f"event-data-phase-field-not-allowed:{field}"
                )


def _digest_payload(payload: dict[str, Any]) -> str:
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()


def build_event(
    *,
    run_id: str,
    scenario_id: str,
    direction: str,
    implementation: str,
    implementation_revision: str,
    driver_binary_sha256: str,
    local_router_hash_sha256: str,
    peer_router_hash_sha256: str,
    monotonic_ms: int,
    event_kind: EventKind,
    event_sequence: int,
    delivery_status_message_id: int | None = None,
    i2np_type: int | None = None,
    frame_sequence: int | None = None,
) -> dict[str, Any]:
    """Build one finalized Plan 062 reference event record."""

    event: dict[str, Any] = {
        "schema": EVENT_SCHEMA,
        "schema_version": EVENT_SCHEMA_VERSION,
        "run_id": run_id,
        "scenario_id": scenario_id,
        "direction": direction,
        "implementation": implementation,
        "implementation_revision": implementation_revision,
        "driver_binary_sha256": driver_binary_sha256,
        "local_router_hash_sha256": local_router_hash_sha256,
        "peer_router_hash_sha256": peer_router_hash_sha256,
        "monotonic_ms": monotonic_ms,
        "event_kind": event_kind.value,
        "event_sequence": event_sequence,
        "event_sha256": "",
    }
    if event_kind in _DATA_PHASE_EVENTS:
        if (
            delivery_status_message_id is None
            or i2np_type is None
            or frame_sequence is None
        ):
            raise ReferenceEventError(
                "data-phase-event-requires-delivery-status-message-id-and-i2np-type"
            )
        event["delivery_status_message_id"] = delivery_status_message_id
        event["i2np_type"] = i2np_type
        event["frame_sequence"] = frame_sequence
    event["event_sha256"] = _digest_payload(event)
    validate_event(event)
    return event


def expected_event_sequence(prev: int | None) -> int:
    """Return the strictly-increasing next event sequence."""

    if prev is None:
        return 0
    return prev + 1


def known_data_phase_event_kinds() -> frozenset[EventKind]:
    """Return the set of event kinds that require data-phase fields."""

    return frozenset(_DATA_PHASE_EVENTS)


__all__ = [
    "EVENT_SCHEMA",
    "EVENT_SCHEMA_VERSION",
    "EventKind",
    "ReferenceEventError",
    "build_event",
    "expected_event_sequence",
    "known_data_phase_event_kinds",
    "validate_event",
]
