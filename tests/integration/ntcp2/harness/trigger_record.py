"""Plan 055 reference-trigger record schema and helpers.

This module defines the locked machine-readable trigger record
(``i2pr-reference-trigger-v3``) plus bounded ``TriggerOutcome`` and
``TriggerHelperKind`` enumerations used by the reference-initiated
NTCP2 directions.

The previous ``reference_trigger.py`` SAM v3 helper was insufficient
for Plan 055: a successful SAM session is not a successful NTCP2
transport dial, and a failed trigger could not be distinguished from
a successful one without a per-attempt, source-locked, helper-bound
record. This module supplies:

- a complete ``TriggerRecord`` payload schema with the required fields
  documented in plan 055 Workstream A1;
- strict ``validate_trigger_record`` checks (malformed target hash,
  wrong endpoint type, zero helper digests, unknown helper kinds,
  attempt counts other than the declared one-shot contract, and
  run-identity mismatches);
- ``TriggerOutcome`` and ``TriggerHelperKind`` bounded enumerations
  covering the explicit outcomes required by the plan;
- ``finalize_trigger_record`` returning the canonical record digest
  used by Plan 052 evidence bundles;
- ``compute_trigger_sha256`` for direction-record binding.

Helpers live under test/integration tooling; this module never
becomes a production dependency.
"""

from __future__ import annotations

import hashlib
import json
import re
from enum import Enum
from typing import Any


TRIGGER_SCHEMA = "i2pr-reference-trigger-v3"
TRIGGER_SCHEMA_VERSION = 3

# Reference identifiers locked in plan 055 Workstream B and C.
_REFERENCES = {"java_i2p", "i2pd"}

# Bounded helper kinds. The plan rejects any helper that bypasses
# authenticated transport or that requires patching cryptography.
class TriggerHelperKind(Enum):
    I2PD_DIRECT_HELPER = "i2pd-direct-helper"
    JAVA_DIRECT_HELPER = "java-direct-helper"
    JAVA_MINIMAL_SUPPORT_TOPOLOGY = "java-minimal-support-topology"


# Bounded trigger outcomes. A ``passed`` direction must use the full
# observation predicate; ``connected`` alone cannot mark a direction
# passed (Plan 055 Workstream A2).
class TriggerOutcome(Enum):
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
    SUPPORT_TOPOLOGY_NOT_APPROVED = "support-topology-not-approved"
    SUPPORT_TOPOLOGY_NOT_READY = "support-topology-not-ready"
    CLEANUP_FAILED = "cleanup-failed"


_REQUIRED_FIELDS = (
    "schema",
    "schema_version",
    "run_id",
    "scenario_id",
    "reference",
    "reference_version",
    "reference_revision",
    "helper_kind",
    "helper_binary_sha256",
    "helper_source_sha256",
    "target_router_hash",
    "target_router_info_sha256",
    "target_ntcp2_static_key_sha256",
    "target_address",
    "target_port",
    "correlation_nonce",
    "attempted",
    "attempt_count",
    "outcome",
    "reason_code",
    "transport_request_observed",
    "connection_callback_observed",
    "started_monotonic_ms",
    "completed_monotonic_ms",
    "sanitized_detail",
    "trigger_sha256",
    # Plan 055 A3: helper build provenance is mandatory and bound to
    # the digest; the run identity field is required for cross-bundle
    # binding (Plan 055 E2).
    "helper_compiler",
    "helper_pinned_inputs_sha256",
    "source_inspection_record_sha256",
    "run_identity_sha256",
)

_HEX40 = re.compile(r"^[0-9a-f]{40}$")
_HEX64 = re.compile(r"^[0-9a-f]{64}$")
_REVISION = re.compile(r"^[0-9a-f]{40}$")
_RUN_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,46}[a-z0-9])?$")
_NONCE = re.compile(r"^[a-zA-Z0-9_-]{8,128}$")


class TriggerRecordError(ValueError):
    """Raised when a trigger record fails validation."""


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


def validate_trigger_record(
    record: dict[str, Any],
    *,
    run_identity_sha256: str | None = None,
    attempt_count: int | None = None,
    finalized: bool = False,
) -> None:
    """Validate a Plan 055 trigger record.

    Raises ``TriggerRecordError`` when any required field is missing,
    has the wrong type, or carries a value that breaks the locked
    contract (Plan 055 Workstream A1 + A2 + A4).

    If ``run_identity_sha256`` is supplied, the trigger's
    ``run_identity_sha256`` field must agree. ``attempt_count`` enforces
    the declared one-shot contract when provided. ``finalized=True``
    requires a non-empty trigger digest.
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
    if record["reference"] == "java_i2p" and helper_kind not in {
        TriggerHelperKind.JAVA_DIRECT_HELPER,
        TriggerHelperKind.JAVA_MINIMAL_SUPPORT_TOPOLOGY,
    }:
        raise TriggerRecordError("java-trigger-helper-kind-invalid")
    if not _HEX64.fullmatch(record["helper_binary_sha256"]):
        raise TriggerRecordError("trigger-helper-binary-sha256-invalid")
    if not _HEX64.fullmatch(record["helper_source_sha256"]):
        raise TriggerRecordError("trigger-helper-source-sha256-invalid")
    if not _HEX64.fullmatch(record["helper_pinned_inputs_sha256"]):
        raise TriggerRecordError("trigger-helper-pinned-inputs-sha256-invalid")
    if not _HEX64.fullmatch(record["source_inspection_record_sha256"]):
        raise TriggerRecordError("trigger-source-inspection-sha256-invalid")
    if not isinstance(record["helper_compiler"], str):
        raise TriggerRecordError("trigger-helper-compiler-not-string")
    if record["attempted"] and (
        record["helper_binary_sha256"] == "0" * 64
        or record["helper_source_sha256"] == "0" * 64
    ):
        raise TriggerRecordError("trigger-zero-helper-digest-with-attempted")
    if not _HEX40.fullmatch(record["target_router_hash"]):
        raise TriggerRecordError("trigger-target-router-hash-invalid")
    if not _HEX64.fullmatch(record["target_router_info_sha256"]):
        raise TriggerRecordError("trigger-target-router-info-sha256-invalid")
    if not _HEX64.fullmatch(record["target_ntcp2_static_key_sha256"]):
        raise TriggerRecordError("trigger-target-static-key-sha256-invalid")
    if record["target_address"] not in {"192.0.2.1", "192.0.2.2"}:
        raise TriggerRecordError("trigger-target-address-not-synthetic")
    if not isinstance(record["target_port"], int) or not 1 <= record["target_port"] <= 65535:
        raise TriggerRecordError("trigger-target-port-invalid")
    if not _NONCE.fullmatch(record["correlation_nonce"]):
        raise TriggerRecordError("trigger-correlation-nonce-invalid")
    if not isinstance(record["attempted"], bool):
        raise TriggerRecordError("trigger-attempted-not-bool")
    if not isinstance(record["attempt_count"], int) or record["attempt_count"] < 0:
        raise TriggerRecordError("trigger-attempt-count-invalid")
    if attempt_count is not None and record["attempt_count"] != attempt_count:
        raise TriggerRecordError("trigger-attempt-count-mismatch")
    # The plan locks the one-shot helper contract at attempt_count == 1
    # when the trigger was attempted. Retry qualification runs new
    # helper processes with new attempt records.
    if record["attempted"] and record["attempt_count"] != 1:
        raise TriggerRecordError("trigger-attempt-count-other-than-one-shot")
    try:
        TriggerOutcome(record["outcome"])
    except ValueError as exc:
        raise TriggerRecordError("trigger-outcome-not-allowlisted") from exc
    if not isinstance(record["reason_code"], str) or not record["reason_code"]:
        raise TriggerRecordError("trigger-reason-code-missing")
    if not isinstance(record["transport_request_observed"], bool):
        raise TriggerRecordError("trigger-transport-request-observed-not-bool")
    if not isinstance(record["connection_callback_observed"], bool):
        raise TriggerRecordError("trigger-connection-callback-observed-not-bool")
    if not isinstance(record["started_monotonic_ms"], int) or record["started_monotonic_ms"] < 0:
        raise TriggerRecordError("trigger-started-monotonic-invalid")
    if not isinstance(record["completed_monotonic_ms"], int) or record["completed_monotonic_ms"] < 0:
        raise TriggerRecordError("trigger-completed-monotonic-invalid")
    if record["completed_monotonic_ms"] < record["started_monotonic_ms"]:
        raise TriggerRecordError("trigger-completed-before-started")
    if not isinstance(record["sanitized_detail"], str):
        raise TriggerRecordError("trigger-sanitized-detail-not-string")
    if not isinstance(record["run_identity_sha256"], str) or (
        record["run_identity_sha256"] and not _HEX64.fullmatch(record["run_identity_sha256"])
    ):
        raise TriggerRecordError("trigger-run-identity-sha256-format-invalid")
    if finalized and not _HEX64.fullmatch(record["trigger_sha256"]):
        raise TriggerRecordError("trigger-sha256-not-digest")
    if run_identity_sha256 is not None:
        record_run_identity = record.get("run_identity_sha256")
        if record_run_identity is None:
            raise TriggerRecordError("trigger-run-identity-sha256-missing")
        if record_run_identity != run_identity_sha256:
            raise TriggerRecordError("trigger-run-identity-mismatch")


def compute_trigger_sha256(record: dict[str, Any]) -> str:
    """Return the canonical digest over the unsigned record payload."""

    unsigned = {key: value for key, value in record.items() if key != "trigger_sha256"}
    canonical = json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()


def finalize_trigger_record(
    record: dict[str, Any],
    *,
    run_identity_sha256: str | None = None,
) -> str:
    """Validate, finalize, and return the canonical digest."""

    validate_trigger_record(record, run_identity_sha256=run_identity_sha256)
    record["trigger_sha256"] = compute_trigger_sha256(record)
    validate_trigger_record(record, run_identity_sha256=run_identity_sha256, finalized=True)
    return record["trigger_sha256"]


def build_trigger_record(
    *,
    run_id: str,
    scenario_id: str,
    reference: str,
    helper_kind: TriggerHelperKind,
    helper_binary_sha256: str,
    helper_source_sha256: str,
    helper_compiler: str,
    helper_pinned_inputs_sha256: str,
    source_inspection_record_sha256: str,
    target_router_hash: str,
    target_router_info_sha256: str,
    target_ntcp2_static_key_sha256: str,
    target_address: str,
    target_port: int,
    correlation_nonce: str,
    attempted: bool,
    attempt_count: int,
    outcome: TriggerOutcome,
    reason_code: str,
    transport_request_observed: bool,
    connection_callback_observed: bool,
    started_monotonic_ms: int,
    completed_monotonic_ms: int,
    sanitized_detail: str,
    run_identity_sha256: str,
) -> dict[str, Any]:
    """Construct a Plan 055 trigger record with bounded provenance.

    Helper build provenance (Plan 055 A3) is recorded as mandatory
    fields and participates in the canonical digest. The
    ``run_identity_sha256`` field binds the trigger to the Plan 052
    bundle identity (Plan 055 E2).
    """

    version, revision = _reference_metadata(reference)
    record: dict[str, Any] = {
        "schema": TRIGGER_SCHEMA,
        "schema_version": TRIGGER_SCHEMA_VERSION,
        "run_id": run_id,
        "scenario_id": scenario_id,
        "reference": reference,
        "reference_version": version,
        "reference_revision": revision,
        "helper_kind": helper_kind.value,
        "helper_binary_sha256": helper_binary_sha256,
        "helper_source_sha256": helper_source_sha256,
        "helper_compiler": helper_compiler,
        "helper_pinned_inputs_sha256": helper_pinned_inputs_sha256,
        "source_inspection_record_sha256": source_inspection_record_sha256,
        "target_router_hash": target_router_hash,
        "target_router_info_sha256": target_router_info_sha256,
        "target_ntcp2_static_key_sha256": target_ntcp2_static_key_sha256,
        "target_address": target_address,
        "target_port": target_port,
        "correlation_nonce": correlation_nonce,
        "attempted": attempted,
        "attempt_count": attempt_count,
        "outcome": outcome.value,
        "reason_code": reason_code,
        "transport_request_observed": transport_request_observed,
        "connection_callback_observed": connection_callback_observed,
        "started_monotonic_ms": started_monotonic_ms,
        "completed_monotonic_ms": completed_monotonic_ms,
        "sanitized_detail": sanitized_detail,
        "run_identity_sha256": run_identity_sha256,
        "trigger_sha256": "",
    }
    finalize_trigger_record(record, run_identity_sha256=run_identity_sha256)
    return record


def is_source_locked_helper(helper_kind: TriggerHelperKind) -> bool:
    """Return True when the helper is a direct transport seam (Plan 055 E1)."""

    return helper_kind in {
        TriggerHelperKind.I2PD_DIRECT_HELPER,
        TriggerHelperKind.JAVA_DIRECT_HELPER,
    }


def is_support_topology_helper(helper_kind: TriggerHelperKind) -> bool:
    """Return True when the helper requires a minimal support topology."""

    return helper_kind is TriggerHelperKind.JAVA_MINIMAL_SUPPORT_TOPOLOGY