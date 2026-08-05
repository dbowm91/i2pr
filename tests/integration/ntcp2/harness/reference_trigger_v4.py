"""Plan 062 reference trigger record schema v4 and helpers.

This module defines the locked machine-readable trigger record
(``i2pr-reference-trigger-v4``) plus the bounded ``TriggerOutcome``
and ``TriggerHelperKind`` enumerations required by Plan 062.

The Plan 062 v4 schema supersedes the Plan 055 v3 schema
(``trigger_record.py``). The v3 module is retained for the bounded
historical-reader path; v3 records cannot contribute to a new
passing bundle. Plan 062 v4 introduces:

- 64-lowercase-hex Router Hash for both the local (sender) and peer
  (target) routers, replacing the Plan 055 v3 40-lowercase-hex
  width;
- mandatory per-run DeliveryStatus ``message_id`` in
  ``1..=0xffffffff``;
- explicit per-side correlation between sender, receiver, trigger,
  and scenario;
- new structured observation fields
  (``connection_established_observed``,
  ``sender_frame_write_observed``) that distinguish a successful
  transport request from a successful frame write and from a
  successful receiver-side frame decryption;
- per-direction ``direction`` field;
- explicit ``helper_build_manifest_sha256`` binding the helper build
  contract;
- explicit ``observer_patch_sha256`` binding the i2pd passive
  observer patch digest;
- explicit ``local_router_hash_sha256``,
  ``local_router_info_sha256`` binding the sender-side RouterInfo
  hash and digest.

The v4 schema rejects:

- v3 trigger records;
- any Router Hash that is not exactly 64 lowercase hex characters;
- any attempted record with all-zero helper/source/build/observer
  provenance digests;
- any ``delivery_status_message_id`` outside ``1..=0xffffffff``;
- any ``attempt_count`` other than exactly ``1`` for an attempted
  record;
- any ``completed_monotonic_ms`` earlier than
  ``started_monotonic_ms``;
- any target address outside the synthetic declared set;
- unknown fields and missing fields.

The canonical digest is computed over the unsigned record and
recorded as ``trigger_sha256``.
"""

from __future__ import annotations

import hashlib
import json
import re
from enum import Enum
from typing import Any


TRIGGER_SCHEMA = "i2pr-reference-trigger-v4"
TRIGGER_SCHEMA_VERSION = 4

# Plan 062 references the same locked revisions as Plan 055.
_REFERENCES = {"java_i2p", "i2pd"}


class TriggerHelperKind(Enum):
    """Plan 062 source-locked helper kinds.

    The Java direct helper uses the upstream stripped-router
    architecture; the i2pd direct helper uses the pinned transport
    seam with a compile-time-gated passive observer. The Plan 062
    v4 schema refuses any helper that bypasses authenticated
    transport or that requires patching cryptography.
    """

    I2PD_DIRECT_HELPER = "i2pd-direct-helper"
    JAVA_DIRECT_HELPER = "java-direct-helper"


class TriggerOutcome(Enum):
    """Plan 062 bounded trigger outcomes.

    A ``passed`` direction must use the full Plan 062 receiver
    observation predicate; ``connected`` or ``authenticated`` alone
    cannot mark a direction passed.
    """

    NOT_REQUIRED_I2PR_INITIATOR = "not-required-i2pr-initiator"
    REQUESTED = "requested"
    CONNECTED = "connected"
    AUTHENTICATED = "authenticated"
    REJECTED_TARGET_ROUTER_INFO = "rejected-target-router-info"
    REJECTED_TARGET_ENDPOINT = "rejected-target-endpoint"
    DIRECT_TRIGGER_NOT_SOURCE_LOCKED = "direct-trigger-not-source-locked"
    DIRECT_TRIGGER_API_UNAVAILABLE = "direct-trigger-api-unavailable"
    DIRECT_TRIGGER_CALLBACK_TIMEOUT = "direct-trigger-callback-timeout"
    DIRECT_TRIGGER_HELPER_FAILED = "direct-trigger-helper-failed"
    CLEANUP_FAILED = "cleanup-failed"


_REQUIRED_FIELDS = (
    "schema",
    "schema_version",
    "run_id",
    "scenario_id",
    "direction",
    "reference",
    "reference_version",
    "reference_revision",
    "helper_kind",
    "helper_binary_sha256",
    "helper_source_sha256",
    "helper_build_manifest_sha256",
    "helper_pinned_inputs_sha256",
    "source_inspection_record_sha256",
    "observer_patch_sha256",
    "run_identity_sha256",
    "local_router_hash_sha256",
    "peer_router_hash_sha256",
    "local_router_info_sha256",
    "peer_router_info_sha256",
    "peer_ntcp2_static_key_sha256",
    "target_address",
    "target_port",
    "delivery_status_message_id",
    "attempted",
    "attempt_count",
    "outcome",
    "reason_code",
    "transport_request_observed",
    "connection_established_observed",
    "sender_frame_write_observed",
    "started_monotonic_ms",
    "completed_monotonic_ms",
    "sanitized_detail",
    "trigger_sha256",
    "topology_kind",
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
_SYNTHETIC_TARGETS = {"192.0.2.1", "192.0.2.2"}
_LOOPBACK_TARGETS = {"127.0.0.1"}
_LOOPBACK_TOPOLOGY_KIND = "host-loopback-development"


class TriggerRecordError(ValueError):
    """Raised when a v4 trigger record fails validation."""


def _scan(value: Any) -> None:
    if isinstance(value, str):
        if any(forbidden in value for forbidden in (
            "-----BEGIN", "router.identity", "ntcp2.static.key",
            "/home/", "/root/", "RouterInfo", "I2NP",
        )):
            raise TriggerRecordError("trigger contains forbidden path or payload text")
    elif isinstance(value, dict):
        for child in value.values():
            _scan(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _scan(child)


def _reference_metadata(reference: str) -> tuple[str, str]:
    if reference == "java_i2p":
        return "2.12.0", "2800040deee9bb376567b671ef2e9c34cf3e30b6"
    if reference == "i2pd":
        return "2.60.0", "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"
    raise TriggerRecordError(f"unknown-reference:{reference}")


def _check_hex64(field: str, value: Any) -> None:
    if not isinstance(value, str) or not _HEX64.fullmatch(value):
        raise TriggerRecordError(f"trigger-{field}-invalid")


def validate_trigger_record(
    record: dict[str, Any],
    *,
    run_identity_sha256: str | None = None,
    finalized: bool = False,
) -> None:
    """Validate a Plan 062 v4 trigger record.

    Raises ``TriggerRecordError`` when any required field is missing,
    has the wrong type, or carries a value that breaks the locked
    v4 contract. The optional ``run_identity_sha256`` argument
    enforces the Plan 052 bundle cross-check. ``finalized=True``
    requires a non-empty 64-hex trigger digest.
    """

    if not isinstance(record, dict):
        raise TriggerRecordError("trigger-record-must-be-object")
    missing = [field for field in _REQUIRED_FIELDS if field not in record]
    if missing:
        raise TriggerRecordError(f"trigger-record-missing:{','.join(missing)}")
    extra = set(record) - set(_REQUIRED_FIELDS)
    if extra:
        raise TriggerRecordError(f"trigger-record-unknown-fields:{','.join(sorted(extra))}")
    if record["schema"] != TRIGGER_SCHEMA:
        raise TriggerRecordError("trigger-record-schema-invalid")
    if record["schema_version"] != TRIGGER_SCHEMA_VERSION:
        raise TriggerRecordError("trigger-record-schema-version-invalid")
    _scan(record)
    if not _RUN_ID.fullmatch(record["run_id"]):
        raise TriggerRecordError("trigger-run-id-invalid")
    if not isinstance(record["scenario_id"], str) or not record["scenario_id"]:
        raise TriggerRecordError("trigger-scenario-id-missing")
    if record["direction"] not in _DIRECTIONS:
        raise TriggerRecordError("trigger-direction-not-allowlisted")
    if record["reference"] not in _REFERENCES:
        raise TriggerRecordError("trigger-reference-not-allowlisted")
    expected_version, expected_revision = _reference_metadata(record["reference"])
    if record["reference_version"] != expected_version:
        raise TriggerRecordError("trigger-reference-version-mismatch")
    if not _REVISION.fullmatch(record["reference_revision"]) or record["reference_revision"] != expected_revision:
        raise TriggerRecordError("trigger-reference-revision-mismatch")
    try:
        helper_kind = TriggerHelperKind(record["helper_kind"])
    except ValueError as exc:
        raise TriggerRecordError("trigger-helper-kind-not-allowlisted") from exc
    if record["reference"] == "i2pd" and helper_kind != TriggerHelperKind.I2PD_DIRECT_HELPER:
        raise TriggerRecordError("i2pd-trigger-must-use-i2pd-direct-helper")
    if record["reference"] == "java_i2p" and helper_kind != TriggerHelperKind.JAVA_DIRECT_HELPER:
        raise TriggerRecordError("java-trigger-must-use-java-direct-helper")
    for field in (
        "helper_binary_sha256",
        "helper_source_sha256",
        "helper_build_manifest_sha256",
        "helper_pinned_inputs_sha256",
        "source_inspection_record_sha256",
    ):
        _check_hex64(field, record[field])
    _check_hex64("observer_patch_sha256", record["observer_patch_sha256"])
    if record["attempted"] and (
        record["helper_binary_sha256"] == "0" * 64
        or record["helper_source_sha256"] == "0" * 64
        or record["helper_build_manifest_sha256"] == "0" * 64
        or record["helper_pinned_inputs_sha256"] == "0" * 64
        or record["source_inspection_record_sha256"] == "0" * 64
        or record["observer_patch_sha256"] == "0" * 64
    ):
        raise TriggerRecordError("trigger-zero-provenance-digest-with-attempted")
    _check_hex64("local_router_hash_sha256", record["local_router_hash_sha256"])
    _check_hex64("peer_router_hash_sha256", record["peer_router_hash_sha256"])
    _check_hex64("local_router_info_sha256", record["local_router_info_sha256"])
    _check_hex64("peer_router_info_sha256", record["peer_router_info_sha256"])
    _check_hex64("peer_ntcp2_static_key_sha256", record["peer_ntcp2_static_key_sha256"])
    if record["local_router_hash_sha256"] == "0" * 64 and record["attempted"]:
        raise TriggerRecordError("trigger-local-router-hash-zero-with-attempted")
    if record["peer_router_hash_sha256"] == "0" * 64 and record["attempted"]:
        raise TriggerRecordError("trigger-peer-router-hash-zero-with-attempted")
    if record["target_address"] not in _SYNTHETIC_TARGETS:
        topology_kind_value = str(record.get("topology_kind", ""))
        if topology_kind_value != _LOOPBACK_TOPOLOGY_KIND:
            raise TriggerRecordError("trigger-target-address-not-synthetic")
        if record["target_address"] not in _LOOPBACK_TARGETS:
            raise TriggerRecordError("trigger-target-address-not-loopback")
    if not isinstance(record["target_port"], int) or not 1 <= record["target_port"] <= 65535:
        raise TriggerRecordError("trigger-target-port-invalid")
    message_id = record["delivery_status_message_id"]
    if not isinstance(message_id, int) or isinstance(message_id, bool):
        raise TriggerRecordError("trigger-delivery-status-message-id-not-int")
    if message_id < 1 or message_id > 0xFFFFFFFF:
        raise TriggerRecordError("trigger-delivery-status-message-id-out-of-range")
    if not isinstance(record["attempted"], bool):
        raise TriggerRecordError("trigger-attempted-not-bool")
    if not isinstance(record["attempt_count"], int) or record["attempt_count"] < 0:
        raise TriggerRecordError("trigger-attempt-count-invalid")
    if record["attempted"] and record["attempt_count"] != 1:
        raise TriggerRecordError("trigger-attempt-count-other-than-one-shot")
    try:
        TriggerOutcome(record["outcome"])
    except ValueError as exc:
        raise TriggerRecordError("trigger-outcome-not-allowlisted") from exc
    if not isinstance(record["reason_code"], str) or not record["reason_code"]:
        raise TriggerRecordError("trigger-reason-code-missing")
    for field in (
        "transport_request_observed",
        "connection_established_observed",
        "sender_frame_write_observed",
    ):
        if not isinstance(record[field], bool):
            raise TriggerRecordError(f"trigger-{field.replace('_', '-')}-not-bool")
    if not isinstance(record["started_monotonic_ms"], int) or record["started_monotonic_ms"] < 0:
        raise TriggerRecordError("trigger-started-monotonic-invalid")
    if not isinstance(record["completed_monotonic_ms"], int) or record["completed_monotonic_ms"] < 0:
        raise TriggerRecordError("trigger-completed-monotonic-invalid")
    if record["completed_monotonic_ms"] < record["started_monotonic_ms"]:
        raise TriggerRecordError("trigger-completed-before-started")
    if not isinstance(record["sanitized_detail"], str):
        raise TriggerRecordError("trigger-sanitized-detail-not-string")
    if record["run_identity_sha256"]:
        _check_hex64("run_identity_sha256", record["run_identity_sha256"])
    if finalized and not _HEX64.fullmatch(str(record["trigger_sha256"])):
        raise TriggerRecordError("trigger-sha256-not-digest")
    if run_identity_sha256 is not None:
        record_run_identity = record.get("run_identity_sha256")
        if record_run_identity is None:
            raise TriggerRecordError("trigger-run-identity-sha256-missing")
        if record_run_identity != run_identity_sha256:
            raise TriggerRecordError("trigger-run-identity-mismatch")


def compute_trigger_sha256(record: dict[str, Any]) -> str:
    """Return the canonical digest over the unsigned v4 record payload."""

    unsigned = {key: value for key, value in record.items() if key != "trigger_sha256"}
    canonical = json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()


def finalize_trigger_record(
    record: dict[str, Any],
    *,
    run_identity_sha256: str | None = None,
) -> str:
    """Validate, finalize, and return the canonical v4 trigger digest."""

    validate_trigger_record(record, run_identity_sha256=run_identity_sha256)
    record["trigger_sha256"] = compute_trigger_sha256(record)
    validate_trigger_record(record, run_identity_sha256=run_identity_sha256, finalized=True)
    return record["trigger_sha256"]


def build_trigger_record(
    *,
    run_id: str,
    scenario_id: str,
    direction: str,
    reference: str,
    helper_kind: TriggerHelperKind,
    helper_binary_sha256: str,
    helper_source_sha256: str,
    helper_build_manifest_sha256: str,
    helper_pinned_inputs_sha256: str,
    source_inspection_record_sha256: str,
    observer_patch_sha256: str,
    run_identity_sha256: str,
    local_router_hash_sha256: str,
    peer_router_hash_sha256: str,
    local_router_info_sha256: str,
    peer_router_info_sha256: str,
    peer_ntcp2_static_key_sha256: str,
    target_address: str,
    target_port: int,
    delivery_status_message_id: int,
    attempted: bool,
    attempt_count: int,
    outcome: TriggerOutcome,
    reason_code: str,
    transport_request_observed: bool,
    connection_established_observed: bool,
    sender_frame_write_observed: bool,
    started_monotonic_ms: int,
    completed_monotonic_ms: int,
    sanitized_detail: str,
    topology_kind: str = "rootless-sealed-single-netns",
) -> dict[str, Any]:
    """Construct a Plan 062 v4 trigger record with bounded provenance.

    The helper, source, build, observer, and run-identity digests
    are mandatory fields. The ``delivery_status_message_id`` is
    bound into the canonical digest so a sender, receiver, scenario,
    or trigger with a mismatched message ID fails the bundle
    cross-check.
    """

    version, revision = _reference_metadata(reference)
    record: dict[str, Any] = {
        "schema": TRIGGER_SCHEMA,
        "schema_version": TRIGGER_SCHEMA_VERSION,
        "run_id": run_id,
        "scenario_id": scenario_id,
        "direction": direction,
        "reference": reference,
        "reference_version": version,
        "reference_revision": revision,
        "helper_kind": helper_kind.value,
        "helper_binary_sha256": helper_binary_sha256,
        "helper_source_sha256": helper_source_sha256,
        "helper_build_manifest_sha256": helper_build_manifest_sha256,
        "helper_pinned_inputs_sha256": helper_pinned_inputs_sha256,
        "source_inspection_record_sha256": source_inspection_record_sha256,
        "observer_patch_sha256": observer_patch_sha256,
        "run_identity_sha256": run_identity_sha256,
        "local_router_hash_sha256": local_router_hash_sha256,
        "peer_router_hash_sha256": peer_router_hash_sha256,
        "local_router_info_sha256": local_router_info_sha256,
        "peer_router_info_sha256": peer_router_info_sha256,
        "peer_ntcp2_static_key_sha256": peer_ntcp2_static_key_sha256,
        "target_address": target_address,
        "target_port": target_port,
        "delivery_status_message_id": delivery_status_message_id,
        "attempted": attempted,
        "attempt_count": attempt_count,
        "outcome": outcome.value,
        "reason_code": reason_code,
        "transport_request_observed": transport_request_observed,
        "connection_established_observed": connection_established_observed,
        "sender_frame_write_observed": sender_frame_write_observed,
        "started_monotonic_ms": started_monotonic_ms,
        "completed_monotonic_ms": completed_monotonic_ms,
        "sanitized_detail": sanitized_detail,
        "topology_kind": topology_kind,
        "trigger_sha256": "",
    }
    finalize_trigger_record(record, run_identity_sha256=run_identity_sha256)
    return record


def is_source_locked_helper(helper_kind: TriggerHelperKind) -> bool:
    """Return True when the helper is a direct transport seam."""

    return helper_kind in {
        TriggerHelperKind.I2PD_DIRECT_HELPER,
        TriggerHelperKind.JAVA_DIRECT_HELPER,
    }


__all__ = [
    "TRIGGER_SCHEMA",
    "TRIGGER_SCHEMA_VERSION",
    "TriggerHelperKind",
    "TriggerOutcome",
    "TriggerRecordError",
    "build_trigger_record",
    "compute_trigger_sha256",
    "finalize_trigger_record",
    "is_source_locked_helper",
    "validate_trigger_record",
]
