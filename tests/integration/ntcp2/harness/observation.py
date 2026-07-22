"""Plan 052 receiver-side observation schema v2.

Each bounded Plan 045-052 direction produces per-side observation levels.
The schema is source-neutral so the i2pr responder, Java I2P receiver, and
i2pd receiver all emit the same shape. A direction record carries one
``observation`` object per side with bounded levels:

- ``process_started``
- ``listener_ready``
- ``tcp_connected``
- ``ntcp2_authenticated``
- ``frame_emitted``
- ``frame_authenticated_and_decrypted``
- ``i2np_message_decoded``
- ``terminal_clean``

Each level carries ``state`` (``observed``, ``not-observed``, ``not-applicable``),
``source`` (``typed-status``, ``structured-log``, ``source-derived-log-marker``,
``control-api``), ``evidence_code``, optional ``count``, ordering timestamp,
``sanitized_detail``, and ``observer_implementation``.

The Plan 052 directional predicate (D2) requires the receiver to observe
``frame_authenticated_and_decrypted`` AND ``i2np_message_decoded``; the
sender must observe ``frame_emitted``. Handshake-only observations cannot
satisfy the data phase.

Schema name:

```text
i2pr-ntcp2-direction-observation-v2
```
"""

from __future__ import annotations

import hashlib
import json
import re
from typing import Any


OBSERVATION_SCHEMA = "i2pr-ntcp2-direction-observation-v2"
OBSERVATION_SCHEMA_VERSION = 2

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
    "structured-log",
    "source-derived-log-marker",
    "control-api",
}

_REQUIRED_LEVEL_KEYS = (
    "state",
    "source",
    "evidence_code",
    "sanitized_detail",
    "observer_implementation",
)


_HEX64 = re.compile(r"^[0-9a-f]{64}$")
_HEX40 = re.compile(r"^[0-9a-f]{40}$")
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


def validate_observation(side: str, observation: dict[str, Any]) -> None:
    """Validate one side observation against the v2 schema."""

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
    correlation = observation.get("run_correlation", {})
    if not isinstance(correlation, dict):
        raise ObservationError("run_correlation must be a JSON object when present")
    for field in ("delivery_status_message_id", "bounded_test_nonce"):
        if field in correlation and not isinstance(correlation[field], str):
            raise ObservationError(f"run_correlation.{field} must be a string when present")
    if observation.get("observation_sha256") and not _HEX64.fullmatch(str(observation["observation_sha256"])):
        raise ObservationError("observation_sha256 is not a SHA-256 digest")


def finalize_observation(side: str, observation: dict[str, Any]) -> str:
    """Validate, finalize, and return the canonical observation digest."""

    validate_observation(side, observation)
    unsigned = dict(observation)
    unsigned["observation_sha256"] = ""
    canonical = json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
    digest = hashlib.sha256(canonical).hexdigest()
    observation["observation_sha256"] = digest
    validate_observation(side, observation)
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
    """Build one observation level entry for the v2 schema."""

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
            observer_implementation="observation-schema-v2",
        )
        for level in _OBSERVATION_LEVELS
    }


def receiver_passes_data_phase(observation: dict[str, Any]) -> bool:
    """Apply Plan 052 D2 receiver-side data-phase predicate."""

    levels = observation.get("levels", {})
    if levels.get("frame_authenticated_and_decrypted", {}).get("state") != "observed":
        return False
    if levels.get("i2np_message_decoded", {}).get("state") != "observed":
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