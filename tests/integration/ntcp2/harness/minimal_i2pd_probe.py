"""Plan 083 minimal i2pr-to-i2pd NTCP2 wire probe record contract.

The minimal probe runner is a one-direction development diagnostic. It
exists to obtain the first real ``i2pr -> i2pd`` wire result with a
strict stage model and a single compact sanitized record, without
requiring the broad Plan 045/052 release-style evidence finalization
path. The probe record answers one narrow question:

```text
Can the current i2pr initiator authenticate to the pinned i2pd 2.60.0
responder and deliver one exact DeliveryStatus I2NP message?
```

The probe is a development diagnostic, not a release certificate. A
passed probe record does not authorize Plan 079 (repeated
development validation) or Plan 073 (release qualification); those
gates are owned by their own plans. The probe closes only after the
Plan 084 reverse direction also closes with a development decision.

Schemas and bounded sets:

- :data:`SCHEMA` / :data:`SCHEMA_VERSION` -- the locked record marker
  ``i2pr-minimal-i2pd-probe-v1`` (version ``1``).
- :data:`DIRECTION` -- the only allowlisted direction value, the
  Plan 083 primary ``i2pr-to-i2pd-ipv4``.
- :data:`REFERENCE` -- the only allowlisted reference kind, ``i2pd``.
- :data:`STAGES` -- the strictly-increasing ordered stage set the
  probe reports.
- :data:`TERMINAL_RESULTS` -- bounded terminal categories.
- :data:`REASON_CODES` -- bounded fixed reason codes.
- :data:`PROCESS_COUNTER_KEYS` -- the bounded per-process counter
  fields that the probe records separately for preparation and live
  processes.

Field rules:

- Every required field is non-empty and matches its type-specific
  predicate (digest shape, RFC 3339 timestamp, integer range).
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
  ``cleanup_result = clean``, and at least the four canonical
  observed events for the probe direction.
- A non-passed record may carry any bounded ``terminal_result`` and
  ``reason_code``; pre-protocol rejections must use the typed
  ``pre_protocol_rejected`` terminal and the Plan 083 fixed reason
  set rather than ``typed-harness-operation-failed``.

The probe never imports Plan 056/066 candidate, bundle, certificate,
rootless-topology, or Multipass authority. It may be exercised by
focused tests using fake event streams before a real wire attempt is
attempted in a qualified Plan 080 lane.
"""

from __future__ import annotations

import hashlib
import json
import re
from typing import Any, Final


SCHEMA: Final[str] = "i2pr-minimal-i2pd-probe-v1"
SCHEMA_VERSION: Final[int] = 1


DIRECTION: Final[str] = "i2pr-to-i2pd-ipv4"
REFERENCE: Final[str] = "i2pd"


# Ordered stage set. A stage implies no earlier stage was skipped;
# ``highest_stage_reached`` must equal one of these values. A
# pre-protocol blocker stops at ``state_prepared`` (or ``not_started``
# when preparation itself failed); a real protocol blocker stops at
# the highest stage reached before the failing event.
NOT_STARTED: Final[str] = "not_started"
STATE_PREPARED: Final[str] = "state_prepared"
PEER_ROUTER_INFO_IMPORTED: Final[str] = "peer_router_info_imported"
LISTENER_READY: Final[str] = "listener_ready"
TCP_CONNECTED: Final[str] = "tcp_connected"
NOISE_AUTHENTICATED: Final[str] = "noise_authenticated"
SESSION_CONFIRMED_ACCEPTED: Final[str] = "session_confirmed_accepted"
AUTHENTICATED_FRAME_WRITTEN: Final[str] = "authenticated_frame_written"
AUTHENTICATED_FRAME_DECRYPTED: Final[str] = "authenticated_frame_decrypted"
I2NP_DELIVERY_STATUS_DECODED: Final[str] = "i2np_delivery_status_decoded"


STAGES: Final[tuple[str, ...]] = (
    NOT_STARTED,
    STATE_PREPARED,
    PEER_ROUTER_INFO_IMPORTED,
    LISTENER_READY,
    TCP_CONNECTED,
    NOISE_AUTHENTICATED,
    SESSION_CONFIRMED_ACCEPTED,
    AUTHENTICATED_FRAME_WRITTEN,
    AUTHENTICATED_FRAME_DECRYPTED,
    I2NP_DELIVERY_STATUS_DECODED,
)


STAGE_RANK: Final[dict[str, int]] = {stage: index for index, stage in enumerate(STAGES)}


# Bounded terminal results. ``passed`` requires a real wire success;
# ``pre_protocol_rejected`` is reserved for the Plan 082 pre-protocol
# surface (preparation, run identity freeze, scenario render).
PASSED: Final[str] = "passed"
PROTOCOL_REJECTED: Final[str] = "protocol_rejected"
PROTOCOL_TIMEOUT: Final[str] = "protocol_timeout"
PRE_PROTOCOL_REJECTED: Final[str] = "pre_protocol_rejected"
CLEANUP_FAILED: Final[str] = "cleanup_failed"
LANE_INVALID: Final[str] = "lane_invalid"


TERMINAL_RESULTS: Final[frozenset[str]] = frozenset(
    {
        PASSED,
        PROTOCOL_REJECTED,
        PROTOCOL_TIMEOUT,
        PRE_PROTOCOL_REJECTED,
        CLEANUP_FAILED,
        LANE_INVALID,
    }
)


# Bounded reason codes. The probe refuses ``typed-harness-operation-failed``;
# every failure is classified into one of these typed reasons.
REASON_NOT_STARTED: Final[str] = "not_started"
REASON_I2PD_LISTENER_NOT_READY: Final[str] = "i2pd-listener-not-ready"
REASON_I2PR_DIAL_START_FAILED: Final[str] = "i2pr-dial-start-failed"
REASON_TCP_CONNECT_FAILED: Final[str] = "tcp-connect-failed"
REASON_NOISE_SESSION_REQUEST_REJECTED: Final[str] = "noise-session-request-rejected"
REASON_NOISE_SESSION_CREATED_REJECTED: Final[str] = "noise-session-created-rejected"
REASON_SESSION_CONFIRMED_REJECTED: Final[str] = "session-confirmed-rejected"
REASON_PEER_ROUTER_INFO_REJECTED: Final[str] = "peer-router-info-rejected"
REASON_AUTHENTICATED_LINK_INSTALL_FAILED: Final[str] = "authenticated-link-install-failed"
REASON_I2PR_FRAME_WRITE_FAILED: Final[str] = "i2pr-frame-write-failed"
REASON_I2PD_FRAME_AUTHENTICATION_FAILED: Final[str] = "i2pd-frame-authentication-failed"
REASON_I2PD_I2NP_DECODE_FAILED: Final[str] = "i2pd-i2np-decode-failed"
REASON_DELIVERY_STATUS_ID_MISMATCH: Final[str] = "delivery-status-id-mismatch"
REASON_REFERENCE_EVENTS_MISSING: Final[str] = "reference-events-missing"
REASON_CLEANUP_VERIFICATION_FAILED: Final[str] = "cleanup-verification-failed"
REASON_LANE_INVALID: Final[str] = "lane-invalid"
REASON_PRE_PROTOCOL_RENDER_FAILED: Final[str] = "pre-protocol-render-failed"
REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED: Final[str] = "pre-protocol-run-identity-failed"
REASON_PRE_PROTOCOL_PREPARATION_FAILED: Final[str] = "pre-protocol-preparation-failed"
REASON_PRE_PROTOCOL_REFERENCE_FAILED: Final[str] = "pre-protocol-reference-failed"
REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED: Final[str] = "pre-protocol-router-info-validation-failed"
REASON_RUNNER_REFERENCE_PROCESS_NOT_EXECUTED: Final[str] = "runner-reference-process-not-executed"
REASON_RUNNER_REFERENCE_EVENTS_MISSING: Final[str] = "runner-reference-events-missing"
REASON_RUNNER_SYNTHETIC_PROVENANCE_REJECTED: Final[str] = "runner-synthetic-provenance-rejected"
REASON_RUNNER_PROTOCOL_EVENT_UNPROVEN: Final[str] = "runner-protocol-event-unproven"


REASON_CODES: Final[frozenset[str]] = frozenset(
    {
        REASON_NOT_STARTED,
        REASON_I2PD_LISTENER_NOT_READY,
        REASON_I2PR_DIAL_START_FAILED,
        REASON_TCP_CONNECT_FAILED,
        REASON_NOISE_SESSION_REQUEST_REJECTED,
        REASON_NOISE_SESSION_CREATED_REJECTED,
        REASON_SESSION_CONFIRMED_REJECTED,
        REASON_PEER_ROUTER_INFO_REJECTED,
        REASON_AUTHENTICATED_LINK_INSTALL_FAILED,
        REASON_I2PR_FRAME_WRITE_FAILED,
        REASON_I2PD_FRAME_AUTHENTICATION_FAILED,
        REASON_I2PD_I2NP_DECODE_FAILED,
        REASON_DELIVERY_STATUS_ID_MISMATCH,
        REASON_REFERENCE_EVENTS_MISSING,
        REASON_CLEANUP_VERIFICATION_FAILED,
        REASON_LANE_INVALID,
        REASON_PRE_PROTOCOL_RENDER_FAILED,
        REASON_PRE_PROTOCOL_RUN_IDENTITY_FAILED,
        REASON_PRE_PROTOCOL_PREPARATION_FAILED,
        REASON_PRE_PROTOCOL_REFERENCE_FAILED,
        REASON_PRE_PROTOCOL_ROUTER_INFO_VALIDATION_FAILED,
        REASON_RUNNER_REFERENCE_PROCESS_NOT_EXECUTED,
        REASON_RUNNER_REFERENCE_EVENTS_MISSING,
        REASON_RUNNER_SYNTHETIC_PROVENANCE_REJECTED,
        REASON_RUNNER_PROTOCOL_EVENT_UNPROVEN,
    }
)


# Per-process counter keys. The probe records preparation and live
# processes separately so a failed preparation never fabricates a
# live process start. ``started`` increments only after the
# underlying subprocess creation succeeds.
PROCESS_COUNTER_KEYS: Final[frozenset[str]] = frozenset({"started", "exited", "forced"})


PROCESS_KEYS: Final[frozenset[str]] = frozenset(
    {
        "i2pr_prepare",
        "i2pd_prepare",
        "i2pd_listener",
        "i2pr_dialer",
    }
)


# Fixed event names observed from the i2pr side and from the i2pd
# side. The probe records ``event_name``, ``source_side``, and the
# event-record digest as a structured event summary; raw payload and
# transcript bytes are forbidden.
ALLOWED_EVENT_NAMES: Final[frozenset[str]] = frozenset(
    {
        "process_started",
        "listener_ready",
        "peer_router_info_validated",
        "tcp_connected",
        "ntcp2_authenticated",
        "session_confirmed_accepted",
        "frame_emitted",
        "frame_authenticated_and_decrypted",
        "i2np_message_decoded",
        "terminal_clean",
        "terminal_rejected",
    }
)


ALLOWED_EVENT_SIDES: Final[frozenset[str]] = frozenset({"i2pr", "i2pd"})


# Allowed topology kinds. The probe accepts the Plan 046 rootless
# sealed-namespace lane (``rootless-sealed-single-netns``), the
# Plan 080 qualified Multipass guest lane, or the Plan 086/088
# host-loopback development lane (``host-loopback-development``) for
# literal IPv4 loopback protocol execution; other topology kinds are
# rejected by the strict parser. The host-loopback-development
# topology is development-only and never satisfies release
# qualification.
ALLOWED_TOPOLOGY_KINDS: Final[frozenset[str]] = frozenset(
    {
        "rootless-sealed-single-netns",
        "multipass-owned-guest",
        "host-loopback-development",
    }
)

# Development-only topology kinds. Records carrying any of these
# topology values explicitly opt out of release/isolation
# qualification; they never satisfy Plan 079 ``level2-passed`` or any
# Level 3 release predicate.
DEVELOPMENT_ONLY_TOPOLOGY_KINDS: Final[frozenset[str]] = frozenset(
    {
        "host-loopback-development",
    }
)


CLEANUP_RESULTS: Final[frozenset[str]] = frozenset(
    {
        "clean",
        "forced",
        "failed",
        "not-run",
    }
)


HEX40: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{40}$")
HEX64: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{64}$")
RFC3339_RE: Final[re.Pattern[str]] = re.compile(
    r"^\d{4}-\d{2}-\d{2}T\d{2}:\d{2}:\d{2}(?:\.\d+)?(?:Z|[+-]\d{2}:\d{2})$",
)


# Fields that are forbidden in a sanitized record. The probe never
# retains raw payload, private key material, Noise state, transcripts,
# RouterInfo bytes, or arbitrary remote error text.
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


PASSED_REQUIRED_OBSERVED_EVENTS: Final[tuple[str, ...]] = (
    "ntcp2_authenticated",
    "frame_emitted",
    "frame_authenticated_and_decrypted",
    "i2np_message_decoded",
)


class MinimalI2pdProbeError(ValueError):
    """Raised when a probe record violates the contract."""


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise MinimalI2pdProbeError(message)


def canonical_record_digest(record: dict[str, Any]) -> str:
    """Return the canonical SHA-256 digest excluding ``record_sha256``."""

    payload = {key: value for key, value in record.items() if key != "record_sha256"}
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def empty_process_counters() -> dict[str, dict[str, int]]:
    """Return a fresh process-counters skeleton with every counter at zero."""

    return {
        key: {counter: 0 for counter in sorted(PROCESS_COUNTER_KEYS)}
        for key in sorted(PROCESS_KEYS)
    }


def validate_process_counters(value: Any) -> dict[str, dict[str, int]]:
    """Validate the process counter structure.

    Returns the validated dict (a copy) so the caller can mutate it
    without affecting the original input.
    """

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
        counters[process_key] = {counter_key: int(entry[counter_key]) for counter_key in PROCESS_COUNTER_KEYS}
    return counters


def validate_observed_event(event: Any) -> dict[str, Any]:
    """Validate one observed event summary entry."""

    _require(isinstance(event, dict), "observed event must be a JSON object")
    extra = set(event) - {"event_name", "source_side", "event_sha256"}
    _require(not extra, f"observed event forbids unknown field: {extra}")
    event_name = event.get("event_name")
    _require(
        isinstance(event_name, str) and event_name in ALLOWED_EVENT_NAMES,
        "observed event_name not allowlisted",
    )
    source_side = event.get("source_side")
    _require(
        isinstance(source_side, str) and source_side in ALLOWED_EVENT_SIDES,
        "observed source_side not allowlisted",
    )
    event_sha256 = event.get("event_sha256")
    _require(
        isinstance(event_sha256, str) and HEX64.fullmatch(event_sha256),
        "observed event_sha256 must be 64 lowercase hex",
    )
    return dict(event)


def validate_record(record: Any) -> dict[str, Any]:
    """Validate a probe record and return the normalized dict.

    Raises :class:`MinimalI2pdProbeError` on any contract violation.
    The returned dict is a shallow copy safe to mutate without
    affecting the original input.
    """

    _require(isinstance(record, dict), "probe record must be a JSON object")
    record_copy = dict(record)

    for field_name in REQUIRED_FIELDS:
        _require(field_name in record_copy, f"probe record missing field: {field_name}")

    for forbidden in FORBIDDEN_FIELDS:
        _require(
            forbidden not in record_copy,
            f"probe record forbids secret-bearing field: {forbidden}",
        )

    extra_fields = set(record_copy) - set(REQUIRED_FIELDS)
    _require(not extra_fields, f"probe record forbids unknown field: {extra_fields}")

    _require(record_copy["schema"] == SCHEMA, "probe record schema mismatch")
    _require(
        record_copy["schema_version"] == SCHEMA_VERSION,
        "probe record schema_version mismatch",
    )
    _require(
        isinstance(record_copy["run_id"], str) and record_copy["run_id"],
        "probe record run_id must be non-empty string",
    )
    _require(
        HEX40.fullmatch(record_copy["source_commit"]) is not None,
        "probe record source_commit must be 40 lowercase hex",
    )
    _require(
        record_copy["direction"] == DIRECTION,
        f"probe record direction must be {DIRECTION}",
    )
    _require(
        record_copy["reference"] == REFERENCE,
        f"probe record reference must be {REFERENCE}",
    )
    _require(
        isinstance(record_copy["reference_revision"], str)
        and HEX40.fullmatch(record_copy["reference_revision"]),
        "probe record reference_revision must be 40 lowercase hex",
    )
    _require(
        HEX64.fullmatch(record_copy["lane_qualification_sha256"]) is not None,
        "probe record lane_qualification_sha256 must be 64 lowercase hex",
    )
    _require(
        record_copy["topology_kind"] in ALLOWED_TOPOLOGY_KINDS,
        "probe record topology_kind not allowlisted",
    )
    _require(
        isinstance(record_copy["parent_network_state_unchanged"], bool),
        "probe record parent_network_state_unchanged must be bool",
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
            f"probe record {field} must be 64 lowercase hex",
        )
    message_id = record_copy["delivery_status_message_id"]
    _require(
        isinstance(message_id, int)
        and not isinstance(message_id, bool)
        and 1 <= message_id <= 0xFFFFFFFF,
        "probe record delivery_status_message_id must be 1..=0xffffffff",
    )
    observed_events = record_copy["observed_events"]
    _require(
        isinstance(observed_events, list),
        "probe record observed_events must be a list",
    )
    normalized_events = [validate_observed_event(event) for event in observed_events]
    record_copy["observed_events"] = normalized_events

    highest_stage = record_copy["highest_stage_reached"]
    _require(
        isinstance(highest_stage, str) and highest_stage in STAGE_RANK,
        "probe record highest_stage_reached not allowlisted",
    )
    terminal_result = record_copy["terminal_result"]
    _require(
        isinstance(terminal_result, str) and terminal_result in TERMINAL_RESULTS,
        "probe record terminal_result not allowlisted",
    )
    reason_code = record_copy["reason_code"]
    _require(
        isinstance(reason_code, str) and reason_code in REASON_CODES,
        "probe record reason_code not allowlisted",
    )

    validate_process_counters(record_copy["process_counters"])

    cleanup_result = record_copy["cleanup_result"]
    _require(
        isinstance(cleanup_result, str) and cleanup_result in CLEANUP_RESULTS,
        "probe record cleanup_result not allowlisted",
    )

    expected_digest = canonical_record_digest(record_copy)
    digest = record_copy["record_sha256"]
    _require(
        isinstance(digest, str) and HEX64.fullmatch(digest) is not None,
        "probe record record_sha256 must be 64 lowercase hex",
    )
    _require(
        digest == expected_digest,
        "probe record record_sha256 digest mismatch",
    )

    if terminal_result == PASSED:
        _require(
            highest_stage == I2NP_DELIVERY_STATUS_DECODED,
            "passed probe record requires highest_stage_reached = i2np_delivery_status_decoded",
        )
        _require(
            cleanup_result == "clean",
            "passed probe record requires cleanup_result = clean",
        )
        observed_names = {event["event_name"] for event in normalized_events}
        missing = [name for name in PASSED_REQUIRED_OBSERVED_EVENTS if name not in observed_names]
        _require(
            not missing,
            f"passed probe record requires observed events: {missing}",
        )
        _require(
            reason_code == REASON_NOT_STARTED,
            "passed probe record requires reason_code = not_started",
        )

    return record_copy


def build_record(
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
    """Build one finalized probe record and return the dict.

    The function normalizes the process counter structure, validates
    every field, and computes the canonical :data:`record_sha256`
    digest.
    """

    _require(isinstance(run_id, str) and run_id, "build_record: run_id invalid")
    _require(
        HEX40.fullmatch(source_commit) is not None,
        "build_record: source_commit must be 40 lowercase hex",
    )
    _require(
        HEX40.fullmatch(reference_revision) is not None,
        "build_record: reference_revision must be 40 lowercase hex",
    )
    _require(
        HEX64.fullmatch(lane_qualification_sha256) is not None,
        "build_record: lane_qualification_sha256 must be 64 lowercase hex",
    )
    _require(
        topology_kind in ALLOWED_TOPOLOGY_KINDS,
        "build_record: topology_kind not allowlisted",
    )
    _require(
        isinstance(parent_network_state_unchanged, bool),
        "build_record: parent_network_state_unchanged must be bool",
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
            f"build_record: {field} must be 64 lowercase hex",
        )
    _require(
        isinstance(delivery_status_message_id, int)
        and not isinstance(delivery_status_message_id, bool)
        and 1 <= delivery_status_message_id <= 0xFFFFFFFF,
        "build_record: delivery_status_message_id must be 1..=0xffffffff",
    )
    _require(
        highest_stage_reached in STAGE_RANK,
        "build_record: highest_stage_reached not allowlisted",
    )
    _require(
        terminal_result in TERMINAL_RESULTS,
        "build_record: terminal_result not allowlisted",
    )
    _require(
        reason_code in REASON_CODES,
        "build_record: reason_code not allowlisted",
    )
    _require(
        cleanup_result in CLEANUP_RESULTS,
        "build_record: cleanup_result not allowlisted",
    )
    normalized_counters = validate_process_counters(process_counters)

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
    validate_record(record)
    return record


__all__ = [
    "ALLOWED_EVENT_NAMES",
    "ALLOWED_EVENT_SIDES",
    "ALLOWED_TOPOLOGY_KINDS",
    "AUTHENTICATED_FRAME_DECRYPTED",
    "AUTHENTICATED_FRAME_WRITTEN",
    "CLEANUP_FAILED",
    "CLEANUP_RESULTS",
    "DEVELOPMENT_ONLY_TOPOLOGY_KINDS",
    "DIRECTION",
    "FORBIDDEN_FIELDS",
    "I2NP_DELIVERY_STATUS_DECODED",
    "LANE_INVALID",
    "LISTENER_READY",
    "MinimalI2pdProbeError",
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
    "REASON_CODES",
    "REASON_NOT_STARTED",
    "REFERENCE",
    "REQUIRED_FIELDS",
    "SCHEMA",
    "SCHEMA_VERSION",
    "SESSION_CONFIRMED_ACCEPTED",
    "STAGES",
    "STAGE_RANK",
    "STATE_PREPARED",
    "TCP_CONNECTED",
    "TERMINAL_RESULTS",
    "build_record",
    "canonical_record_digest",
    "empty_process_counters",
    "validate_observed_event",
    "validate_process_counters",
    "validate_record",
]
