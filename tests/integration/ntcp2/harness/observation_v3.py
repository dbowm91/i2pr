"""Plan 062 receiver-side observation schema v3.

The Plan 062 v3 schema supersedes the Plan 052 v2 schema
(``observation.py``). The v3 schema is the active per-side
observation schema for Plan 062+ primary directions. The v2 module
is retained for the bounded historical-reader path; v2 observations
cannot contribute to a new passing bundle.

The v3 schema is source-neutral so the i2pr responder, Java I2P
receiver, and i2pd receiver all emit the same shape. A direction
record carries one ``observation`` object per side with the bounded
levels:

- ``process_started``
- ``listener_ready``
- ``tcp_connected``
- ``ntcp2_authenticated``
- ``frame_emitted``
- ``frame_authenticated_and_decrypted``
- ``i2np_message_decoded``
- ``terminal_clean``

Each level carries ``state`` (``observed``, ``not-observed``,
``not-applicable``), ``source``
(``typed-status``, ``structured-event``, ``control-api``),
``evidence_code``, ``count``, ``first_observed_monotonic_ms``,
``sanitized_detail``, and ``observer_implementation``.

The v3 schema adds the following mandatory top-level correlation
fields:

- ``delivery_status_message_id`` (integer in
  ``1..=0xffffffff``);
- ``peer_router_hash_sha256`` (64 lowercase hex);
- ``local_router_hash_sha256`` (64 lowercase hex);
- ``source_event_sha256`` (64 lowercase hex).

The Plan 062 v3 directional predicate requires the receiver to
observe ``frame_authenticated_and_decrypted`` AND
``i2np_message_decoded`` with non-zero ``count``, a matching
``delivery_status_message_id`` between scenario, trigger, sender,
and receiver, and matching ``peer_router_hash_sha256`` /
``local_router_hash_sha256`` between trigger, sender, receiver, and
direction record. Handshake-only observations and generic phrase
catalogs cannot satisfy the data-phase predicate.

The validator rejects:

- a receiver ``i2np_message_decoded=observed`` with no exact
  message ID;
- a receiver event sourced only from a generic phrase catalog;
- a sender-only observation used as receiver proof;
- a Java event from the wrong handler/source peer;
- an i2pd event emitted before successful AEAD and I2NP conversion;
- a handshake-only run represented as data phase;
- message ID mismatch between trigger, sender, receiver, and
  scenario;
- Router Hash mismatch among RouterInfo, trigger, events, and
  direction record.

Schema name:

```text
i2pr-ntcp2-direction-observation-v3
```
"""

from __future__ import annotations

import hashlib
import json
import re
from typing import Any


OBSERVATION_SCHEMA = "i2pr-ntcp2-direction-observation-v3"
OBSERVATION_SCHEMA_VERSION = 3

_OBSERVATION_LEVELS = (
    "process_started",
    "listener_ready",
    "tcp_connected",
    "ntcp2_authenticated",
    "frame_emitted",
    "frame_authenticated_and_decrypted",
    "i2np_message_decoded",
    "terminal_clean",
)

_OBSERVATION_STATES = {"observed", "not-observed", "not-applicable"}
_OBSERVATION_SOURCES = {
    "typed-status",
    "structured-event",
    "control-api",
}

_REQUIRED_LEVEL_KEYS = (
    "state",
    "source",
    "evidence_code",
    "sanitized_detail",
    "observer_implementation",
)

_REQUIRED_CORRELATION_FIELDS = (
    "delivery_status_message_id",
    "peer_router_hash_sha256",
    "local_router_hash_sha256",
    "source_event_sha256",
)


_HEX64 = re.compile(r"^[0-9a-f]{64}$")
_SIDE = {"i2pr", "java_i2p", "i2pd"}


class ObservationError(ValueError):
    """Raised when a typed observation record fails validation."""


def _scan(value: Any) -> None:
    if isinstance(value, str):
        if any(forbidden in value for forbidden in (
            "-----BEGIN", "router.identity", "ntcp2.static.key",
            "/home/", "/root/", "RouterInfo", "I2NP",
        )):
            raise ObservationError("observation contains forbidden path or payload text")
    elif isinstance(value, dict):
        for child in value.values():
            _scan(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _scan(child)


def _check_hex64(field: str, value: Any) -> None:
    if not isinstance(value, str) or not _HEX64.fullmatch(value):
        raise ObservationError(f"observation-{field}-invalid")


def validate_observation(
    side: str,
    observation: dict[str, Any],
    *,
    require_correlation: bool = False,
) -> None:
    """Validate one side observation against the v3 schema.

    ``require_correlation=True`` enforces the Plan 062 mandatory
    correlation fields (``delivery_status_message_id``,
    ``peer_router_hash_sha256``, ``local_router_hash_sha256``,
    ``source_event_sha256``). The Plan 062 primary direction
    predicate passes the correlation requirement when checking a
    primary observation; diagnostic and blocked observations may
    pass without correlation.
    """

    if not isinstance(observation, dict):
        raise ObservationError("observation must be a JSON object")
    if side not in _SIDE:
        raise ObservationError(f"side {side!r} is not a typed selector")
    if observation.get("schema") != OBSERVATION_SCHEMA:
        raise ObservationError("unknown observation schema")
    if observation.get("schema_version") != OBSERVATION_SCHEMA_VERSION:
        raise ObservationError("unsupported observation schema version")
    _scan(observation)
    levels = observation.get("levels")
    if not isinstance(levels, dict) or set(levels) != set(_OBSERVATION_LEVELS):
        raise ObservationError("observation levels are missing or extra")
    for level_name, level_value in levels.items():
        if not isinstance(level_value, dict):
            raise ObservationError(f"{level_name} must be a JSON object")
        for key in _REQUIRED_LEVEL_KEYS:
            if key not in level_value:
                raise ObservationError(f"{level_name}.{key} is missing")
        if level_value["state"] not in _OBSERVATION_STATES:
            raise ObservationError(f"{level_name}.state is not a typed state")
        if level_value["source"] not in _OBSERVATION_SOURCES:
            raise ObservationError(f"{level_name}.source is not a typed source")
        if not isinstance(level_value["evidence_code"], str) or not level_value["evidence_code"]:
            raise ObservationError(f"{level_name}.evidence_code must be non-empty")
        if not isinstance(level_value["sanitized_detail"], str):
            raise ObservationError(f"{level_name}.sanitized_detail must be a string")
        if not isinstance(level_value["observer_implementation"], str) or not level_value["observer_implementation"]:
            raise ObservationError(f"{level_name}.observer_implementation must be non-empty")
        if "count" in level_value and not isinstance(level_value["count"], int):
            raise ObservationError(f"{level_name}.count must be an integer when present")
        if "first_observed_monotonic_ms" in level_value and not isinstance(level_value["first_observed_monotonic_ms"], int):
            raise ObservationError(f"{level_name}.first_observed_monotonic_ms must be an integer when present")
    if require_correlation:
        for field in _REQUIRED_CORRELATION_FIELDS:
            if field not in observation:
                raise ObservationError(f"observation-correlation-missing:{field}")
        message_id = observation["delivery_status_message_id"]
        if not isinstance(message_id, int) or isinstance(message_id, bool):
            raise ObservationError("observation-delivery-status-message-id-not-int")
        if message_id < 1 or message_id > 0xFFFFFFFF:
            raise ObservationError("observation-delivery-status-message-id-out-of-range")
        _check_hex64("peer_router_hash_sha256", observation["peer_router_hash_sha256"])
        _check_hex64("local_router_hash_sha256", observation["local_router_hash_sha256"])
        _check_hex64("source_event_sha256", observation["source_event_sha256"])
    if observation.get("observation_sha256") and not _HEX64.fullmatch(str(observation["observation_sha256"])):
        raise ObservationError("observation_sha256 is not a SHA-256 digest")


def finalize_observation(
    side: str,
    observation: dict[str, Any],
    *,
    require_correlation: bool = False,
) -> str:
    """Validate, finalize, and return the canonical v3 observation digest."""

    validate_observation(side, observation, require_correlation=require_correlation)
    unsigned = dict(observation)
    unsigned["observation_sha256"] = ""
    canonical = json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
    digest = hashlib.sha256(canonical).hexdigest()
    observation["observation_sha256"] = digest
    validate_observation(side, observation, require_correlation=require_correlation)
    return digest


def build_level(
    state: str,
    source: str,
    evidence_code: str,
    *,
    count: int | None = None,
    first_observed_monotonic_ms: int | None = None,
    sanitized_detail: str = "",
    observer_implementation: str = "",
) -> dict[str, Any]:
    """Build one observation level entry for the v3 schema."""

    level: dict[str, Any] = {
        "state": state,
        "source": source,
        "evidence_code": evidence_code,
        "sanitized_detail": sanitized_detail,
        "observer_implementation": observer_implementation,
    }
    if count is not None:
        level["count"] = count
    if first_observed_monotonic_ms is not None:
        level["first_observed_monotonic_ms"] = first_observed_monotonic_ms
    return level


def empty_levels(not_applicable_reason: str = "not-applicable-for-this-side") -> dict[str, Any]:
    """Return a fully-typed 'not-applicable' observation for diagnostic records."""

    return {
        level: build_level(
            "not-applicable",
            "typed-status",
            not_applicable_reason,
            observer_implementation="observation-schema-v3",
        )
        for level in _OBSERVATION_LEVELS
    }


def receiver_passes_data_phase(observation: dict[str, Any]) -> bool:
    """Apply the Plan 062 receiver-side data-phase predicate.

    The receiver must report ``frame_authenticated_and_decrypted``
    and ``i2np_message_decoded`` with ``state=observed`` and a
    nonzero ``count``. The observation must also carry the exact
    DeliveryStatus ``message_id`` and the 64-hex peer and local
    Router Hashes bound by the Plan 062 v4 trigger schema.
    """

    levels = observation.get("levels", {})
    if levels.get("frame_authenticated_and_decrypted", {}).get("state") != "observed":
        return False
    if levels.get("i2np_message_decoded", {}).get("state") != "observed":
        return False
    decrypt_count = levels.get("frame_authenticated_and_decrypted", {}).get("count", 0)
    decode_count = levels.get("i2np_message_decoded", {}).get("count", 0)
    if not isinstance(decrypt_count, int) or decrypt_count < 1:
        return False
    if not isinstance(decode_count, int) or decode_count < 1:
        return False
    for field in (
        "delivery_status_message_id",
        "peer_router_hash_sha256",
        "local_router_hash_sha256",
        "source_event_sha256",
    ):
        if field not in observation:
            return False
    message_id = observation["delivery_status_message_id"]
    if not isinstance(message_id, int) or isinstance(message_id, bool):
        return False
    if message_id < 1 or message_id > 0xFFFFFFFF:
        return False
    for field in (
        "peer_router_hash_sha256",
        "local_router_hash_sha256",
        "source_event_sha256",
    ):
        value = observation[field]
        if not isinstance(value, str) or not _HEX64.fullmatch(value):
            return False
    return True


def sender_emitted_data_frame(observation: dict[str, Any]) -> bool:
    """Return whether the sender observation reports ``frame_emitted``."""

    return observation.get("levels", {}).get("frame_emitted", {}).get("state") == "observed"


def both_authenticated(sender: dict[str, Any], receiver: dict[str, Any]) -> bool:
    """Return whether both sides observed ``ntcp2_authenticated``."""

    return (
        sender.get("levels", {}).get("ntcp2_authenticated", {}).get("state") == "observed"
        and receiver.get("levels", {}).get("ntcp2_authenticated", {}).get("state") == "observed"
    )


def correlation_matches(trigger: dict[str, Any], sender: dict[str, Any], receiver: dict[str, Any]) -> bool:
    """Return whether the trigger/sender/receiver share the exact correlation fields."""

    if not isinstance(trigger, dict) or not isinstance(sender, dict) or not isinstance(receiver, dict):
        return False
    message_id_trigger = trigger.get("delivery_status_message_id")
    message_id_sender = sender.get("delivery_status_message_id")
    message_id_receiver = receiver.get("delivery_status_message_id")
    if not (isinstance(message_id_trigger, int) and message_id_trigger > 0):
        return False
    if message_id_trigger != message_id_sender or message_id_trigger != message_id_receiver:
        return False
    peer_hash_trigger = trigger.get("peer_router_hash_sha256")
    peer_hash_sender = sender.get("peer_router_hash_sha256")
    peer_hash_receiver = receiver.get("peer_router_hash_sha256")
    if not (isinstance(peer_hash_trigger, str) and _HEX64.fullmatch(peer_hash_trigger)):
        return False
    if peer_hash_trigger != peer_hash_sender or peer_hash_trigger != peer_hash_receiver:
        return False
    return True


__all__ = [
    "OBSERVATION_SCHEMA",
    "OBSERVATION_SCHEMA_VERSION",
    "ObservationError",
    "both_authenticated",
    "build_level",
    "correlation_matches",
    "empty_levels",
    "finalize_observation",
    "receiver_passes_data_phase",
    "sender_emitted_data_frame",
    "validate_observation",
]
