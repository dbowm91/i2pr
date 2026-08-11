"""Plan 099 NTCP2 development interop exit gate.

Bounded vocabulary:
  two-way-development-smoke-passed
  forward-wire-defect
  reverse-wire-defect
  environment-or-build-blocked

A two-way smoke pass requires all four per-attempt records to carry
``terminal_result = passed`` and ``cleanup_result = clean``.
"""

from __future__ import annotations

import hashlib
import json
from typing import Any, Final


SCHEMA: Final[str] = "i2pr-ntcp2-plan099-summary-v1"
SCHEMA_VERSION: Final[int] = 1

TWO_WAY: Final[str] = "two-way-development-smoke-passed"
FORWARD_DEFECT: Final[str] = "forward-wire-defect"
REVERSE_DEFECT: Final[str] = "reverse-wire-defect"
BLOCKED: Final[str] = "environment-or-build-blocked"

DEVELOPMENT_RESULT_VALUES: Final[frozenset[str]] = frozenset(
    {TWO_WAY, FORWARD_DEFECT, REVERSE_DEFECT, BLOCKED}
)


class Plan099ExitGateError(ValueError):
    """Raised when a summary record violates the contract."""


def _passed(record: Any) -> bool:
    return (
        isinstance(record, dict)
        and record.get("terminal_result") == "passed"
        and record.get("cleanup_result") == "clean"
    )


def classify_exit_result(
    forward_instrumented: Any,
    forward_control: Any,
    reverse_instrumented: Any,
    reverse_control: Any,
) -> str:
    """Return the bounded development exit result for the four records.

    - All four pass → ``two-way-development-smoke-passed``.
    - Forward pair fails, reverse pair passes → ``forward-wire-defect``.
    - Reverse pair fails, forward pair passes → ``reverse-wire-defect``.
    - Otherwise → ``environment-or-build-blocked``.
    """
    forward_pair = _passed(forward_instrumented) and _passed(forward_control)
    reverse_pair = _passed(reverse_instrumented) and _passed(reverse_control)
    if forward_pair and reverse_pair:
        return TWO_WAY
    forward_failed = not _passed(forward_instrumented) or not _passed(forward_control)
    reverse_failed = not _passed(reverse_instrumented) or not _passed(reverse_control)
    if forward_failed and reverse_pair:
        return FORWARD_DEFECT
    if reverse_failed and forward_pair:
        return REVERSE_DEFECT
    return BLOCKED


def canonical_summary_digest(summary: dict[str, Any]) -> str:
    payload = {k: v for k, v in summary.items() if k != "summary_sha256"}
    return hashlib.sha256(
        json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    ).hexdigest()


def build_summary(
    *,
    workflow_run_id: str,
    workflow_run_attempt: int,
    source_commit: str,
    runner_label: str,
    runner_arch: str,
    reference_revision: str,
    topology_kind: str,
    network_id: int,
    bind_address: str,
    development_only: bool,
    release_qualified: bool,
    isolation_qualified: bool,
    i2pr_binary_sha256: str,
    i2pd_instrumented_binary_sha256: str,
    i2pd_control_binary_sha256: str,
    forward_instrumented: dict[str, Any],
    forward_control: dict[str, Any],
    reverse_instrumented: dict[str, Any],
    reverse_control: dict[str, Any],
    ntcp2_status: str,
    attempt_records: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    """Build the bounded Plan 099 development summary record."""
    summary: dict[str, Any] = {
        "schema": SCHEMA,
        "schema_version": SCHEMA_VERSION,
        "workflow_run_id": workflow_run_id,
        "workflow_run_attempt": int(workflow_run_attempt),
        "source_commit": source_commit,
        "runner_label": runner_label,
        "runner_arch": runner_arch,
        "reference_revision": reference_revision,
        "topology_kind": topology_kind,
        "network_id": int(network_id),
        "bind_address": bind_address,
        "development_only": bool(development_only),
        "release_qualified": bool(release_qualified),
        "isolation_qualified": bool(isolation_qualified),
        "i2pr_binary_sha256": i2pr_binary_sha256,
        "i2pd_instrumented_binary_sha256": i2pd_instrumented_binary_sha256,
        "i2pd_control_binary_sha256": i2pd_control_binary_sha256,
        "forward_instrumented_result": (
            "passed" if _passed(forward_instrumented) else "nonpassing"
        ),
        "forward_control_result": (
            "passed" if _passed(forward_control) else "nonpassing"
        ),
        "reverse_instrumented_result": (
            "passed" if _passed(reverse_instrumented) else "nonpassing"
        ),
        "reverse_control_result": (
            "passed" if _passed(reverse_control) else "nonpassing"
        ),
        "cleanup": (
            "clean"
            if _passed(forward_instrumented)
            and _passed(forward_control)
            and _passed(reverse_instrumented)
            and _passed(reverse_control)
            else "nonpassing"
        ),
        "status": classify_exit_result(
            forward_instrumented,
            forward_control,
            reverse_instrumented,
            reverse_control,
        ),
        "ntcp2": ntcp2_status,
        "attempt_records": dict(attempt_records),
    }
    summary["summary_sha256"] = canonical_summary_digest(summary)
    return summary


def validate_summary(summary: Any) -> dict[str, Any]:
    """Validate the Plan 099 development summary record."""
    if not isinstance(summary, dict):
        raise Plan099ExitGateError("summary must be a JSON object")
    if summary.get("schema") != SCHEMA:
        raise Plan099ExitGateError("summary schema mismatch")
    if summary.get("schema_version") != SCHEMA_VERSION:
        raise Plan099ExitGateError("summary schema_version mismatch")
    if summary.get("status") not in DEVELOPMENT_RESULT_VALUES:
        raise Plan099ExitGateError("summary status not in bounded vocabulary")
    digest = summary.get("summary_sha256")
    if not isinstance(digest, str) or len(digest) != 64:
        raise Plan099ExitGateError("summary summary_sha256 must be 64 hex chars")
    if digest != canonical_summary_digest(summary):
        raise Plan099ExitGateError("summary summary_sha256 digest mismatch")
    return dict(summary)


__all__ = [
    "BLOCKED",
    "DEVELOPMENT_RESULT_VALUES",
    "FORWARD_DEFECT",
    "Plan099ExitGateError",
    "REVERSE_DEFECT",
    "SCHEMA",
    "SCHEMA_VERSION",
    "TWO_WAY",
    "build_summary",
    "canonical_summary_digest",
    "classify_exit_result",
    "validate_summary",
]
