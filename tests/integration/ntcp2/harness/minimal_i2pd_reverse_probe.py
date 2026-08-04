"""Plan 084 i2pd-to-i2pr reverse probe record contract.

The reverse probe is the second direction of the Plan 081-084
minimal two-way development probe sequence. It is a one-direction
development diagnostic, structurally identical to the Plan 083 forward
probe except for the role assignment:

```text
i2pr = responder/listener
i2pd = initiator/dialer and DeliveryStatus sender
```

The probe answers one narrow question:

```text
Can the pinned i2pd 2.60.0 initiator authenticate to the current i2pr
responder and deliver one exact DeliveryStatus I2NP message?
```

The probe is a development diagnostic, not a release certificate. A
passed probe record does not authorize Plan 079 (repeated development
validation) or Plan 073 (release qualification); those gates are
owned by their own plans. The probe closes only after Plan 084 writes
exactly one of the bounded development decisions to
``plans/084-status.md``.

The schema inherits every bounded set from the Plan 083 forward probe
(stages, terminal results, reason codes, observed-event names,
process-counter structure) so the two directions produce
comparability-validated records. The reverse direction uses different
``process_counters`` keys because the role assignment is reversed:

```text
Plan 083 forward: i2pr_prepare, i2pd_prepare, i2pd_listener, i2pr_dialer
Plan 084 reverse: i2pr_prepare, i2pd_prepare, i2pr_listener, i2pd_dialer
```

Schemas and bounded sets:

- :data:`SCHEMA` / :data:`SCHEMA_VERSION` -- the locked record marker
  ``i2pr-minimal-i2pd-reverse-probe-v1`` (version ``1``).
- :data:`DIRECTION` -- the only allowlisted direction value, the
  Plan 084 primary ``i2pd-to-i2pr-ipv4``.
- :data:`REFERENCE` -- the only allowlisted reference kind, ``i2pd``.
- :data:`STAGES` -- the strictly-increasing ordered stage set the
  probe reports (inherited from the Plan 083 forward probe).
- :data:`TERMINAL_RESULTS` -- bounded terminal categories.
- :data:`REASON_CODES` -- bounded fixed reason codes.
- :data:`PROCESS_COUNTER_KEYS` -- the bounded per-process counter
  fields that the probe records separately for preparation and live
  processes.
- :data:`PROCESS_KEYS` -- the reverse-direction process-key set.

Field rules:

- Every required field is non-empty and matches its type-specific
  predicate (digest shape, integer range).
- ``delivery_status_message_id`` is an integer in ``1..=0xffffffff``
  (Plan 062/065 u32 contract). Zero is rejected.
- ``observed_events`` is a list of normalized event summaries with
  fixed names and bounded digests; raw payload bytes, private keys,
  Noise state, transcripts, and arbitrary remote error text are
  forbidden.
- ``record_sha256`` covers the canonical JSON serialization excluding
  the digest field itself; a mismatch fails closed.
- A ``passed`` record requires ``highest_stage_reached =
  i2np_delivery_status_decoded``, ``terminal_result = passed``,
  ``cleanup_result = clean``, the four canonical observed events for
  the reverse direction, and ``reason_code = not_started``.
- The reverse direction's canonical events are: the i2pr side emits
  ``listener_ready``, ``ntcp2_authenticated``,
  ``frame_authenticated_and_decrypted``, and ``i2np_message_decoded``;
  the i2pd side emits ``frame_emitted`` and ``tcp_connected`` during
  the same handshake/data-phase sequence.
- A non-passed record may carry any bounded ``terminal_result`` and
  ``reason_code``; pre-protocol rejections must use the typed
  ``pre_protocol_rejected`` terminal and the Plan 083/084 fixed
  reason set rather than ``typed-harness-operation-failed``.

The probe never imports Plan 056/066 candidate, bundle, certificate,
rootless-topology, or Multipass authority. It reuses the Plan 083
stage and reason taxonomies from ``minimal_i2pd_probe``.
"""

from __future__ import annotations

import hashlib
import json
import re
from typing import Any, Final

from minimal_i2pd_probe import (
    ALLOWED_EVENT_NAMES,
    ALLOWED_EVENT_SIDES,
    ALLOWED_TOPOLOGY_KINDS,
    AUTHENTICATED_FRAME_DECRYPTED,
    AUTHENTICATED_FRAME_WRITTEN,
    CLEANUP_FAILED,
    CLEANUP_RESULTS,
    I2NP_DELIVERY_STATUS_DECODED,
    LANE_INVALID,
    LISTENER_READY,
    NOISE_AUTHENTICATED,
    NOT_STARTED,
    PASSED,
    PASSED_REQUIRED_OBSERVED_EVENTS,
    PEER_ROUTER_INFO_IMPORTED,
    PRE_PROTOCOL_REJECTED,
    PROCESS_COUNTER_KEYS,
    PROTOCOL_REJECTED,
    PROTOCOL_TIMEOUT,
    REASON_AUTHENTICATED_LINK_INSTALL_FAILED,
    REASON_CLEANUP_VERIFICATION_FAILED,
    REASON_CODES,
    REASON_DELIVERY_STATUS_ID_MISMATCH,
    REASON_I2PD_FRAME_AUTHENTICATION_FAILED,
    REASON_I2PD_I2NP_DECODE_FAILED,
    REASON_I2PD_LISTENER_NOT_READY,
    REASON_I2PR_DIAL_START_FAILED,
    REASON_I2PR_FRAME_WRITE_FAILED,
    REASON_LANE_INVALID,
    REASON_NOISE_SESSION_CREATED_REJECTED,
    REASON_NOISE_SESSION_REQUEST_REJECTED,
    REASON_NOT_STARTED,
    REASON_PEER_ROUTER_INFO_REJECTED,
    REASON_PRE_PROTOCOL_PREPARATION_FAILED,
    REASON_PRE_PROTOCOL_REFERENCE_FAILED,
    REASON_PRE_PROTOCOL_RENDER_FAILED,
    REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED,
    REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED,
    REASON_REFERENCE_EVENTS_MISSING,
    REASON_SESSION_CONFIRMED_REJECTED,
    REASON_TCP_CONNECT_FAILED,
    SESSION_CONFIRMED_ACCEPTED,
    STAGES,
    STAGE_RANK,
    STATE_PREPARED,
    TCP_CONNECTED,
    TERMINAL_RESULTS,
    canonical_record_digest,
    validate_observed_event,
    validate_process_counters,
)


SCHEMA: Final[str] = "i2pr-minimal-i2pd-reverse-probe-v1"
SCHEMA_VERSION: Final[int] = 1


DIRECTION: Final[str] = "i2pd-to-i2pr-ipv4"
REFERENCE: Final[str] = "i2pd"


# Plan 084 reverse-direction process keys. Preparation of both sides is
# unchanged from Plan 083; the live process assignment is reversed
# because the i2pr is the responder/listener and the i2pd is the
# initiator/dialer.
PROCESS_KEYS: Final[frozenset[str]] = frozenset(
    {
        "i2pr_prepare",
        "i2pd_prepare",
        "i2pr_listener",
        "i2pd_dialer",
    }
)


# Reverse-direction canonical observed events. The probe must observe
# the four mandatory events for a passing record: the i2pr side
# observes ``listener_ready``, ``ntcp2_authenticated``,
# ``frame_authenticated_and_decrypted`` (data-phase AEAD) and
# ``i2np_message_decoded`` (DeliveryStatus decoded); the i2pd side
# observes ``tcp_connected`` and ``frame_emitted``. Every other event
# in :data:`ALLOWED_EVENT_NAMES` is also permitted in the record but
# is not required for a passed outcome.
REVERSE_PASSED_REQUIRED_OBSERVED_EVENTS: Final[tuple[str, ...]] = (
    "ntcp2_authenticated",
    "frame_emitted",
    "frame_authenticated_and_decrypted",
    "i2np_message_decoded",
)


REQUIRED_FIELDS: Final[tuple[str, ...]] = (
    "schema",
    "schema_version",
    "run_id",
    "source_commit",
    "direction",
    "reference",
    "reference_revision",
    "lane_qualification_sha256",
    "topology_kind",
    "parent_network_state_unchanged",
    "i2pr_binary_sha256",
    "i2pd_binary_sha256",
    "i2pr_router_info_sha256",
    "i2pd_router_info_sha256",
    "i2pr_router_hash_sha256",
    "i2pd_router_hash_sha256",
    "delivery_status_message_id",
    "observed_events",
    "highest_stage_reached",
    "terminal_result",
    "reason_code",
    "process_counters",
    "cleanup_result",
    "record_sha256",
)


HEX40: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{40}$")
HEX64: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{64}$")


FORBIDDEN_FIELDS: Final[frozenset[str]] = frozenset(
    {
        "raw_payload",
        "private_key",
        "noise_state",
        "router_info_bytes",
        "session_state",
        "static_key",
        "router_identity",
        "frame_transcript",
        "transcript_bytes",
    }
)


class ReverseProbeError(ValueError):
    """Raised when a reverse probe record violates the contract."""


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise ReverseProbeError(message)


def empty_reverse_process_counters() -> dict[str, dict[str, int]]:
    """Return a fresh reverse-direction process-counters skeleton."""

    return {
        key: {counter: 0 for counter in sorted(PROCESS_COUNTER_KEYS)}
        for key in sorted(PROCESS_KEYS)
    }


def validate_reverse_process_counters(value: Any) -> dict[str, dict[str, int]]:
    """Validate the reverse-direction process counter structure."""

    _require(isinstance(value, dict), "process_counters must be a JSON object")
    extra_keys = set(value) - PROCESS_KEYS
    _require(not extra_keys, f"process_counters forbids unknown process: {extra_keys}")
    missing_keys = PROCESS_KEYS - set(value)
    _require(not missing_keys, f"process_counters missing process: {missing_keys}")
    counters: dict[str, dict[str, int]] = {}
    for process_key in sorted(PROCESS_KEYS):
        entry = value[process_key]
        _require(isinstance(entry, dict), f"process_counters[{process_key}] must be object")
        entry_extra = set(entry) - PROCESS_COUNTER_KEYS
        _require(
            not entry_extra,
            f"process_counters[{process_key}] forbids unknown counter: {entry_extra}",
        )
        entry_missing = PROCESS_COUNTER_KEYS - set(entry)
        _require(
            not entry_missing,
            f"process_counters[{process_key}] missing counter: {entry_missing}",
        )
        for counter_key in sorted(PROCESS_COUNTER_KEYS):
            counter_value = entry[counter_key]
            _require(
                isinstance(counter_value, int) and not isinstance(counter_value, bool),
                f"process_counters[{process_key}][{counter_key}] must be int",
            )
            _require(
                counter_value >= 0,
                f"process_counters[{process_key}][{counter_key}] must be non-negative",
            )
        counters[process_key] = {
            counter_key: int(entry[counter_key]) for counter_key in PROCESS_COUNTER_KEYS
        }
    return counters


def validate_reverse_record(record: Any) -> dict[str, Any]:
    """Validate a Plan 084 reverse-direction probe record.

    Raises :class:`ReverseProbeError` on any contract violation. The
    returned dict is a shallow copy safe to mutate without affecting
    the original input.
    """

    _require(isinstance(record, dict), "reverse probe record must be a JSON object")
    record_copy = dict(record)

    for field_name in REQUIRED_FIELDS:
        _require(field_name in record_copy, f"reverse probe record missing field: {field_name}")

    for forbidden in FORBIDDEN_FIELDS:
        _require(
            forbidden not in record_copy,
            f"reverse probe record forbids secret-bearing field: {forbidden}",
        )

    extra_fields = set(record_copy) - set(REQUIRED_FIELDS)
    _require(not extra_fields, f"reverse probe record forbids unknown field: {extra_fields}")

    _require(record_copy["schema"] == SCHEMA, "reverse probe record schema mismatch")
    _require(
        record_copy["schema_version"] == SCHEMA_VERSION,
        "reverse probe record schema_version mismatch",
    )
    _require(
        isinstance(record_copy["run_id"], str) and record_copy["run_id"],
        "reverse probe record run_id must be non-empty string",
    )
    _require(
        HEX40.fullmatch(record_copy["source_commit"]) is not None,
        "reverse probe record source_commit must be 40 lowercase hex",
    )
    _require(
        record_copy["direction"] == DIRECTION,
        f"reverse probe record direction must be {DIRECTION}",
    )
    _require(
        record_copy["reference"] == REFERENCE,
        f"reverse probe record reference must be {REFERENCE}",
    )
    _require(
        isinstance(record_copy["reference_revision"], str)
        and HEX40.fullmatch(record_copy["reference_revision"]),
        "reverse probe record reference_revision must be 40 lowercase hex",
    )
    _require(
        HEX64.fullmatch(record_copy["lane_qualification_sha256"]) is not None,
        "reverse probe record lane_qualification_sha256 must be 64 lowercase hex",
    )
    _require(
        record_copy["topology_kind"] in ALLOWED_TOPOLOGY_KINDS,
        "reverse probe record topology_kind not allowlisted",
    )
    _require(
        isinstance(record_copy["parent_network_state_unchanged"], bool),
        "reverse probe record parent_network_state_unchanged must be bool",
    )
    for field in (
        "i2pr_binary_sha256",
        "i2pd_binary_sha256",
        "i2pr_router_info_sha256",
        "i2pd_router_info_sha256",
        "i2pr_router_hash_sha256",
        "i2pd_router_hash_sha256",
    ):
        _require(
            HEX64.fullmatch(record_copy[field]) is not None,
            f"reverse probe record {field} must be 64 lowercase hex",
        )
    message_id = record_copy["delivery_status_message_id"]
    _require(
        isinstance(message_id, int)
        and not isinstance(message_id, bool)
        and 1 <= message_id <= 0xFFFFFFFF,
        "reverse probe record delivery_status_message_id must be 1..=0xffffffff",
    )
    observed_events = record_copy["observed_events"]
    _require(
        isinstance(observed_events, list),
        "reverse probe record observed_events must be a list",
    )
    normalized_events = [validate_observed_event(event) for event in observed_events]
    record_copy["observed_events"] = normalized_events

    highest_stage = record_copy["highest_stage_reached"]
    _require(
        isinstance(highest_stage, str) and highest_stage in STAGES,
        "reverse probe record highest_stage_reached not allowlisted",
    )
    terminal_result = record_copy["terminal_result"]
    _require(
        isinstance(terminal_result, str) and terminal_result in TERMINAL_RESULTS,
        "reverse probe record terminal_result not allowlisted",
    )
    reason_code = record_copy["reason_code"]
    _require(
        isinstance(reason_code, str) and reason_code in REASON_CODES,
        "reverse probe record reason_code not allowlisted",
    )

    validate_reverse_process_counters(record_copy["process_counters"])

    cleanup_result = record_copy["cleanup_result"]
    _require(
        isinstance(cleanup_result, str) and cleanup_result in CLEANUP_RESULTS,
        "reverse probe record cleanup_result not allowlisted",
    )

    expected_digest = canonical_record_digest(record_copy)
    digest = record_copy["record_sha256"]
    _require(
        isinstance(digest, str) and HEX64.fullmatch(digest) is not None,
        "reverse probe record record_sha256 must be 64 lowercase hex",
    )
    _require(
        digest == expected_digest,
        "reverse probe record record_sha256 digest mismatch",
    )

    if terminal_result == PASSED:
        _require(
            highest_stage == I2NP_DELIVERY_STATUS_DECODED,
            "passed reverse probe record requires highest_stage_reached = i2np_delivery_status_decoded",
        )
        _require(
            cleanup_result == "clean",
            "passed reverse probe record requires cleanup_result = clean",
        )
        observed_names = {event["event_name"] for event in normalized_events}
        missing = [
            name
            for name in REVERSE_PASSED_REQUIRED_OBSERVED_EVENTS
            if name not in observed_names
        ]
        _require(
            not missing,
            f"passed reverse probe record requires observed events: {missing}",
        )
        _require(
            reason_code == REASON_NOT_STARTED,
            "passed reverse probe record requires reason_code = not_started",
        )

    return record_copy


def build_reverse_record(
    *,
    run_id: str,
    source_commit: str,
    reference_revision: str,
    lane_qualification_sha256: str,
    topology_kind: str,
    parent_network_state_unchanged: bool,
    i2pr_binary_sha256: str,
    i2pd_binary_sha256: str,
    i2pr_router_info_sha256: str,
    i2pd_router_info_sha256: str,
    i2pr_router_hash_sha256: str,
    i2pd_router_hash_sha256: str,
    delivery_status_message_id: int,
    observed_events: list[dict[str, Any]],
    highest_stage_reached: str,
    terminal_result: str,
    reason_code: str,
    process_counters: dict[str, dict[str, int]],
    cleanup_result: str,
) -> dict[str, Any]:
    """Build one finalized reverse probe record and return the dict.

    The function normalizes the reverse-direction process counter
    structure, validates every field, and computes the canonical
    :data:`record_sha256` digest.
    """

    _require(isinstance(run_id, str) and run_id, "build_reverse_record: run_id invalid")
    _require(
        HEX40.fullmatch(source_commit) is not None,
        "build_reverse_record: source_commit must be 40 lowercase hex",
    )
    _require(
        HEX40.fullmatch(reference_revision) is not None,
        "build_reverse_record: reference_revision must be 40 lowercase hex",
    )
    _require(
        HEX64.fullmatch(lane_qualification_sha256) is not None,
        "build_reverse_record: lane_qualification_sha256 must be 64 lowercase hex",
    )
    _require(
        topology_kind in ALLOWED_TOPOLOGY_KINDS,
        "build_reverse_record: topology_kind not allowlisted",
    )
    _require(
        isinstance(parent_network_state_unchanged, bool),
        "build_reverse_record: parent_network_state_unchanged must be bool",
    )
    for field, value in (
        ("i2pr_binary_sha256", i2pr_binary_sha256),
        ("i2pd_binary_sha256", i2pd_binary_sha256),
        ("i2pr_router_info_sha256", i2pr_router_info_sha256),
        ("i2pd_router_info_sha256", i2pd_router_info_sha256),
        ("i2pr_router_hash_sha256", i2pr_router_hash_sha256),
        ("i2pd_router_hash_sha256", i2pd_router_hash_sha256),
    ):
        _require(
            HEX64.fullmatch(value) is not None,
            f"build_reverse_record: {field} must be 64 lowercase hex",
        )
    _require(
        isinstance(delivery_status_message_id, int)
        and not isinstance(delivery_status_message_id, bool)
        and 1 <= delivery_status_message_id <= 0xFFFFFFFF,
        "build_reverse_record: delivery_status_message_id must be 1..=0xffffffff",
    )
    _require(
        highest_stage_reached in STAGES,
        "build_reverse_record: highest_stage_reached not allowlisted",
    )
    _require(
        terminal_result in TERMINAL_RESULTS,
        "build_reverse_record: terminal_result not allowlisted",
    )
    _require(
        reason_code in REASON_CODES,
        "build_reverse_record: reason_code not allowlisted",
    )
    _require(
        cleanup_result in CLEANUP_RESULTS,
        "build_reverse_record: cleanup_result not allowlisted",
    )
    normalized_counters = validate_reverse_process_counters(process_counters)

    record: dict[str, Any] = {
        "schema": SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "run_id": run_id,
        "source_commit": source_commit,
        "direction": DIRECTION,
        "reference": REFERENCE,
        "reference_revision": reference_revision,
        "lane_qualification_sha256": lane_qualification_sha256,
        "topology_kind": topology_kind,
        "parent_network_state_unchanged": bool(parent_network_state_unchanged),
        "i2pr_binary_sha256": i2pr_binary_sha256,
        "i2pd_binary_sha256": i2pd_binary_sha256,
        "i2pr_router_info_sha256": i2pr_router_info_sha256,
        "i2pd_router_info_sha256": i2pd_router_info_sha256,
        "i2pr_router_hash_sha256": i2pr_router_hash_sha256,
        "i2pd_router_hash_sha256": i2pd_router_hash_sha256,
        "delivery_status_message_id": int(delivery_status_message_id),
        "observed_events": [validate_observed_event(event) for event in observed_events],
        "highest_stage_reached": highest_stage_reached,
        "terminal_result": terminal_result,
        "reason_code": reason_code,
        "process_counters": normalized_counters,
        "cleanup_result": cleanup_result,
        "record_sha256": "",
    }
    record["record_sha256"] = canonical_record_digest(record)
    validate_reverse_record(record)
    return record


# Re-export the inherited bounded sets so the runner can import the
# reverse schema as a single namespace without depending on the
# forward probe module.
__all__ = [
    "ALLOWED_EVENT_NAMES",
    "ALLOWED_EVENT_SIDES",
    "ALLOWED_TOPOLOGY_KINDS",
    "CLEANUP_FAILED",
    "CLEANUP_RESULTS",
    "DIRECTION",
    "FORBIDDEN_FIELDS",
    "I2NP_DELIVERY_STATUS_DECODED",
    "LANE_INVALID",
    "LISTENER_READY",
    "NOISE_AUTHENTICATED",
    "NOT_STARTED",
    "PASSED",
    "PASSED_REQUIRED_OBSERVED_EVENTS",
    "PEER_ROUTER_INFO_IMPORTED",
    "PRE_PROTOCOL_REJECTED",
    "PROCESS_COUNTER_KEYS",
    "PROCESS_KEYS",
    "PROTOCOL_REJECTED",
    "PROTOCOL_TIMEOUT",
    "REASON_AUTHENTICATED_LINK_INSTALL_FAILED",
    "REASON_CLEANUP_VERIFICATION_FAILED",
    "REASON_CODES",
    "REASON_DELIVERY_STATUS_ID_MISMATCH",
    "REASON_I2PD_FRAME_AUTHENTICATION_FAILED",
    "REASON_I2PD_I2NP_DECODE_FAILED",
    "REASON_I2PD_LISTENER_NOT_READY",
    "REASON_I2PR_DIAL_START_FAILED",
    "REASON_I2PR_FRAME_WRITE_FAILED",
    "REASON_LANE_INVALID",
    "REASON_NOISE_SESSION_CREATED_REJECTED",
    "REASON_NOISE_SESSION_REQUEST_REJECTED",
    "REASON_NOT_STARTED",
    "REASON_PEER_ROUTER_INFO_REJECTED",
    "REASON_PRE_PROTOCOL_PREPARATION_FAILED",
    "REASON_PRE_PROTOCOL_REFERENCE_FAILED",
    "REASON_PRE_PROTOCOL_RENDER_FAILED",
    "REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED",
    "REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED",
    "REASON_REFERENCE_EVENTS_MISSING",
    "REASON_SESSION_CONFIRMED_REJECTED",
    "REASON_TCP_CONNECT_FAILED",
    "REFERENCE",
    "REQUIRED_FIELDS",
    "REVERSE_PASSED_REQUIRED_OBSERVED_EVENTS",
    "ReverseProbeError",
    "SCHEMA",
    "SCHEMA_VERSION",
    "SESSION_CONFIRMED_ACCEPTED",
    "STAGES",
    "STAGE_RANK",
    "STATE_PREPARED",
    "TCP_CONNECTED",
    "TERMINAL_RESULTS",
    "AUTHENTICATED_FRAME_DECRYPTED",
    "AUTHENTICATED_FRAME_WRITTEN",
    "build_reverse_record",
    "canonical_record_digest",
    "empty_reverse_process_counters",
    "validate_reverse_process_counters",
    "validate_reverse_record",
]