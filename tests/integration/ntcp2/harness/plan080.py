"""Plan 080 stale-instance inspection and lane-qualification closure helpers.

Plan 080 is the stale-Multipass-instance inspection and lane-qualification
closure pass that extends the Plan 074/077/078 constrained-host execution
lane with guest inspection, qualification record writing, and the
typed-blocker/close-status contract for the current host.

On this host the Plan 046 ``apparmor_restrict_on`` negative baseline
plus the Plan 051 resource constraints cause Plan 080 to close with
the typed blocker ``blocked_execution_lane_unavailable``; the
qualification record reports ``lane-blocked``.

This module provides:

- :func:`plan080_typed_blocker` — canonical typed blocker;
- :func:`plan080_close_status` — canonical close-status classifier;
- :func:`plan080_lane_qualification_digest` — validate and return the
  qualification record digest;
- :func:`plan080_guest_inspect_record` — build a sanitized stale-instance
  inspection record;
- :func:`plan080_qualification_writer` — build a validated qualification
  record with a measured digest.
"""

from __future__ import annotations

import datetime as dt
import hashlib
import json
import re
from typing import Any, Final

if __package__ in {None, ""}:
    import sys
    from pathlib import Path
    sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

import execution_lane


SCHEMA: Final[str] = "i2pr-plan080-closure-v1"
SCHEMA_VERSION: Final[int] = 1
TYPED_BLOCKER: Final[str] = "blocked_execution_lane_unavailable"
INSPECT_SCHEMA: Final[str] = "i2pr-plan080-stale-instance-inspection-v1"

CLOSE_STATUS_IN_PROGRESS: Final[str] = "in-progress"
CLOSE_STATUS_QUALIFIED: Final[str] = "lane-qualified"
CLOSE_STATUS_BLOCKED: Final[str] = "lane-blocked"

ALLOWED_CLOSE_STATUSES: Final[frozenset[str]] = frozenset({
    CLOSE_STATUS_IN_PROGRESS,
    CLOSE_STATUS_QUALIFIED,
    CLOSE_STATUS_BLOCKED,
})

SHA256_RE: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{64}$")

INSPECT_REQUIRED_FIELDS: Final[frozenset[str]] = frozenset({
    "schema",
    "schema_version",
    "run_id",
    "instance_name",
    "multipass_list_entry",
    "host_lifecycle_record_present",
    "ownership_contract_present",
    "remediation_class",
    "notes",
    "recorded_utc",
    "record_sha256",
})


class Plan080Error(ValueError):
    """Raised when a Plan 080 invariant or contract is violated."""


def _require(condition: bool, message: str) -> None:
    if not condition:
        raise Plan080Error(message)


def _canonical_digest(record: dict[str, Any], *, digest_field: str = "record_sha256") -> str:
    payload = {key: value for key, value in record.items() if key != digest_field}
    encoded = json.dumps(payload, sort_keys=True, separators=(",", ":"), ensure_ascii=True)
    return hashlib.sha256(encoded.encode("utf-8")).hexdigest()


def _validate_digest(value: Any, field: str) -> None:
    _require(
        isinstance(value, str) and SHA256_RE.fullmatch(value) is not None,
        f"{field} must be 64 lowercase hex",
    )


def plan080_typed_blocker() -> str:
    """Return the canonical typed blocker that closes Plan 080 on this host.

    The host is the Plan 046 ``apparmor_restrict_on`` negative
    baseline; the Plan 046 rootless sealed-namespace lane returns
    ``blocked_unprivileged_user_namespace``. The Plan 048/049
    Multipass recovery lane is the canonical external path but
    cannot complete on this constrained host (per Plan 051). Plan
    080 therefore records the typed environment blocker
    ``blocked_execution_lane_unavailable``.
    """

    return TYPED_BLOCKER


def plan080_close_status() -> str:
    """Return the canonical close-status classification for Plan 080.

    Plan 080 does not produce a qualified lane on this host.
    The qualification record carries the typed blocker and the
    close-status is ``lane-blocked``.
    """

    return CLOSE_STATUS_BLOCKED


def plan080_lane_qualification_digest(record: dict[str, Any]) -> str:
    """Validate *record* via the execution-lane validator and return its digest.

    The record must pass :func:`execution_lane.validate_qualification_record`.
    Returns ``record["record_sha256"]`` after validation.
    """

    try:
        validated = execution_lane.validate_qualification_record(record)
    except execution_lane.ExecutionLaneError as exc:
        raise Plan080Error(str(exc)) from exc
    _validate_digest(validated.get("record_sha256", ""), "record_sha256")
    return str(validated["record_sha256"])


def plan080_guest_inspect_record(
    *,
    run_id: str,
    instance_name: str,
    multipass_list_entry: dict[str, Any],
    host_lifecycle_record_present: bool,
    ownership_contract_present: bool,
    remediation_class: str,
    notes: str,
    recorded_utc: str | None = None,
    **kwargs: Any,
) -> dict[str, Any]:
    """Build a sanitized stale-instance inspection record.

    The record schema is :data:`INSPECT_SCHEMA`. Unknown fields
    are rejected. The ``record_sha256`` is computed over the
    canonical JSON excluding itself.
    """

    _require(not kwargs, f"unknown fields rejected: {sorted(kwargs)}")
    _require(isinstance(run_id, str) and run_id, "run_id must be non-empty string")
    _require(isinstance(instance_name, str) and instance_name, "instance_name must be non-empty string")
    _require(isinstance(multipass_list_entry, dict), "multipass_list_entry must be object")
    _require(isinstance(host_lifecycle_record_present, bool), "host_lifecycle_record_present must be bool")
    _require(isinstance(ownership_contract_present, bool), "ownership_contract_present must be bool")
    _require(isinstance(remediation_class, str) and remediation_class, "remediation_class must be non-empty string")
    _require(isinstance(notes, str), "notes must be string")

    if recorded_utc is None:
        recorded_utc = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")

    record: dict[str, Any] = {
        "schema": INSPECT_SCHEMA,
        "schema_version": 1,
        "run_id": run_id,
        "instance_name": instance_name,
        "multipass_list_entry": multipass_list_entry,
        "host_lifecycle_record_present": host_lifecycle_record_present,
        "ownership_contract_present": ownership_contract_present,
        "remediation_class": remediation_class,
        "notes": notes,
        "recorded_utc": recorded_utc,
    }

    record["record_sha256"] = _canonical_digest(record)
    return record


def plan080_qualification_writer(
    probe: dict[str, Any],
    *,
    guest_inspect_record: dict[str, Any] | None = None,
    artifact_digests: dict[str, str] | None = None,
    qualified: bool,
    selected_lane: str = "remote-manual",
    scope: str = "full-runtime",
    full_runtime_lane: str = "available",
    reduced_scope_lane: str = "unavailable",
    loopback_only_proven: bool = True,
    no_public_interface_proven: bool = True,
    control_connection_passed: bool = True,
    result_export_passed: bool = True,
    cleanup_passed: bool = True,
    reason_code: str = "lane-qualified",
    reason_codes: list[str] | None = None,
    recorded_utc: str | None = None,
) -> dict[str, Any]:
    """Build and validate a qualification record from the supplied fields.

    The record shape mirrors :func:`execution_lane.no_lane_qualification`
    but allows the caller to supply ``qualified=True`` with the
    corresponding allowlist values. The record is validated by
    :func:`execution_lane.validate_qualification_record` before being
    returned.
    """

    if recorded_utc is None:
        recorded_utc = dt.datetime.now(dt.timezone.utc).replace(microsecond=0).isoformat().replace("+00:00", "Z")

    if artifact_digests is None:
        artifact_digests = {}

    _require(isinstance(probe, dict), "probe must be an object")
    _require(isinstance(artifact_digests, dict), "artifact_digests must be an object")
    for name, digest in artifact_digests.items():
        _require(isinstance(name, str) and name and "/" not in name and ".." not in name,
                 "artifact digest name invalid")
        _require(isinstance(digest, str) and SHA256_RE.fullmatch(digest) is not None,
                 f"artifact_digests.{name} must be 64 lowercase hex")

    if qualified:
        _require(scope == "full-runtime",
                 "only a full-runtime record may be qualified")
        _require(full_runtime_lane == "available",
                 "qualified record requires a full-runtime lane")

    host_or_image_metadata: dict[str, Any] = {}
    if "host_architecture" in probe:
        host_or_image_metadata["architecture"] = probe["host_architecture"]

    record: dict[str, Any] = {
        "schema": execution_lane.QUALIFICATION_SCHEMA,
        "schema_version": execution_lane.SCHEMA_VERSION,
        "selected_lane": selected_lane,
        "scope": scope,
        "host_or_image_metadata": host_or_image_metadata,
        "artifact_digests": dict(artifact_digests),
        "loopback_only_proven": loopback_only_proven,
        "no_public_interface_proven": no_public_interface_proven,
        "control_connection_passed": control_connection_passed,
        "result_export_passed": result_export_passed,
        "cleanup_passed": cleanup_passed,
        "qualified": qualified,
        "reason_code": reason_code,
        "reason_codes": list(reason_codes or []),
        "full_runtime_lane": full_runtime_lane,
        "reduced_scope_lane": reduced_scope_lane,
        "recorded_utc": recorded_utc,
    }

    record["record_sha256"] = execution_lane.canonical_digest(record)
    execution_lane.validate_qualification_record(record)
    return record


__all__ = [
    "ALLOWED_CLOSE_STATUSES",
    "CLOSE_STATUS_BLOCKED",
    "CLOSE_STATUS_IN_PROGRESS",
    "CLOSE_STATUS_QUALIFIED",
    "INSPECT_SCHEMA",
    "INSPECT_REQUIRED_FIELDS",
    "Plan080Error",
    "SCHEMA",
    "SCHEMA_VERSION",
    "TYPED_BLOCKER",
    "plan080_close_status",
    "plan080_guest_inspect_record",
    "plan080_lane_qualification_digest",
    "plan080_qualification_writer",
    "plan080_typed_blocker",
]
