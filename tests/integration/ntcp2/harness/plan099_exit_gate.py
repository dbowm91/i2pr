"""Plan 099 NTCP2 development interop exit gate.

Plan 100 reduces the development exit vocabulary to exactly three
values and makes the classifier stage-aware so it is compatible
with the workflow's sequential gating:

- ``passed`` -- all four per-attempt records carry
  ``terminal_result = passed`` and ``cleanup_result = clean``.
- ``protocol-defect-localized`` -- at least one executed primary
  direction has a real post-TCP wire stage
  (``highest_stage_reached`` at or after ``tcp_connected``) and
  then fails before the required correlated DeliveryStatus pass.
  A skipped downstream attempt cannot erase that classification.
- ``environment-or-harness-blocked`` -- the earliest nonpassing
  path is a pre-TCP preparation, build, reference startup, or
  workflow/API failure.

A skipped attempt is represented explicitly as ``not-run`` /
``skipped-after-prerequisite`` (via the workflow's ``load_or_blocked``
helper, which supplies an ``ENVIRONMENT_OR_HARNESS_BLOCKED``
placeholder); the classifier inspects only attempts that actually
ran. The classifier never requires reverse evidence to classify a
forward protocol defect.
"""

from __future__ import annotations

import hashlib
import json
from typing import Any, Final


SCHEMA: Final[str] = "i2pr-ntcp2-plan099-summary-v1"
SCHEMA_VERSION: Final[int] = 1


PASSED: Final[str] = "passed"
PROTOCOL_DEFECT_LOCALIZED: Final[str] = "protocol-defect-localized"
ENVIRONMENT_OR_HARNESS_BLOCKED: Final[str] = "environment-or-harness-blocked"


DEVELOPMENT_RESULT_VALUES: Final[frozenset[str]] = frozenset(
    {PASSED, PROTOCOL_DEFECT_LOCALIZED, ENVIRONMENT_OR_HARNESS_BLOCKED}
)


# Workflow-facing attempt-slot label keys. The workflow uses these
# to index the attempt_records dict that ``build_summary`` writes.
ATTEMPT_SLOTS: Final[tuple[str, ...]] = (
    "forward_instrumented",
    "forward_control",
    "reverse_instrumented",
    "reverse_control",
)


# Terminal result and cleanup result sentinel values that satisfy
# the per-attempt ``passed`` predicate. The classifier treats an
# attempt as passed only when both fields carry these sentinels.
PASSED_TERMINAL_RESULT: Final[str] = "passed"
PASSED_CLEANUP_RESULT: Final[str] = "clean"


# Stage threshold below which an attempt is treated as a
# pre-TCP/harness failure. ``tcp_connected`` (rank 4 in
# ``minimal_i2pd_probe.STAGES``) and any later stage count as
# authentic post-TCP protocol evidence; lower stages are still
# preparation, address parsing, or peer import. The classifier
# hard-codes the literal stage name to avoid a circular import
# between this module and ``minimal_i2pd_probe``.
PROTOCOL_STAGE_THRESHOLD: Final[str] = "tcp_connected"

PROTOCOL_STAGE_RANK: Final[dict[str, int]] = {
    "not_started": 0,
    "state_prepared": 1,
    "peer_router_info_imported": 2,
    "listener_ready": 3,
    "tcp_connected": 4,
    "noise_authenticated": 5,
    "session_confirmed_accepted": 6,
    "authenticated_frame_written": 7,
    "authenticated_frame_decrypted": 8,
    "i2np_delivery_status_decoded": 9,
}


# Attempt-record placeholder values emitted by the workflow when a
# per-direction record is missing or invalid. These are explicit
# "skipped-after-prerequisite" sentinels; they do not constitute
# authentic evidence and are excluded from the highest-stage
# inspection by the classifier.
SKIPPED_TERMINAL_RESULT: Final[str] = "skipped-after-prerequisite"
SKIPPED_CLEANUP_RESULT: Final[str] = "not-run"


SKIPPED_SENTINELS: Final[frozenset[str]] = frozenset(
    {SKIPPED_TERMINAL_RESULT, ENVIRONMENT_OR_HARNESS_BLOCKED}
)


class Plan099ExitGateError(ValueError):
    """Raised when a summary record violates the contract."""


def _is_skipped(record: Any) -> bool:
    """Return ``True`` when the attempt never produced an authentic
    record (skipped after its prerequisite failed, file missing, or
    JSON invalid)."""
    if not isinstance(record, dict):
        return True
    if not record.get("present", True):
        return True
    terminal = record.get("terminal_result")
    cleanup = record.get("cleanup_result")
    if terminal in SKIPPED_SENTINELS or cleanup == SKIPPED_CLEANUP_RESULT:
        return True
    return False


def _is_passed(record: Any) -> bool:
    """Return ``True`` when the attempt satisfied the full
    ``passed + clean`` contract."""
    return (
        isinstance(record, dict)
        and record.get("terminal_result") == PASSED_TERMINAL_RESULT
        and record.get("cleanup_result") == PASSED_CLEANUP_RESULT
    )


def _stage_at_or_after_threshold(record: Any) -> bool:
    """Return ``True`` when the attempt reached ``tcp_connected``
    or any later wire stage before terminating."""
    if not isinstance(record, dict):
        return False
    stage = record.get("highest_stage_reached")
    if not isinstance(stage, str):
        return False
    rank = PROTOCOL_STAGE_RANK.get(stage, -1)
    threshold = PROTOCOL_STAGE_RANK[PROTOCOL_STAGE_THRESHOLD]
    return rank >= threshold


def classify_exit_result(
    forward_instrumented: Any,
    forward_control: Any,
    reverse_instrumented: Any,
    reverse_control: Any,
) -> str:
    """Return the bounded Plan 099 development exit result.

    - All four executed primary attempts pass cleanly → ``passed``.
    - At least one executed primary attempt reached the post-TCP
      wire stage (``tcp_connected`` or later) and then failed →
      ``protocol-defect-localized``. A skipped downstream attempt
      cannot mask this classification.
    - Otherwise → ``environment-or-harness-blocked``. This includes
      pre-TCP preparation, build, reference startup, and workflow/
      API failures.
    """
    executed = [
        forward_instrumented,
        forward_control,
        reverse_instrumented,
        reverse_control,
    ]
    if all(_is_passed(record) for record in executed):
        return PASSED
    for record in executed:
        if _is_skipped(record):
            continue
        if _stage_at_or_after_threshold(record) and not _is_passed(record):
            return PROTOCOL_DEFECT_LOCALIZED
    return ENVIRONMENT_OR_HARNESS_BLOCKED


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
    ntcp2: str,
    attempt_records: dict[str, dict[str, Any]],
) -> dict[str, Any]:
    """Build the bounded Plan 099 development summary record.

    The ``ntcp2`` keyword carries the current NTCP2 advertisement
    status (e.g. ``experimental-non-advertised``). The classifier
    does not consume it; the field is retained on the summary so
    downstream consumers can record the bound advertisement state.
    """
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
            "passed" if _is_passed(forward_instrumented) else "nonpassing"
        ),
        "forward_control_result": (
            "passed" if _is_passed(forward_control) else "nonpassing"
        ),
        "reverse_instrumented_result": (
            "passed" if _is_passed(reverse_instrumented) else "nonpassing"
        ),
        "reverse_control_result": (
            "passed" if _is_passed(reverse_control) else "nonpassing"
        ),
        "status": classify_exit_result(
            forward_instrumented,
            forward_control,
            reverse_instrumented,
            reverse_control,
        ),
        "ntcp2": ntcp2,
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
    "ATTEMPT_SLOTS",
    "DEVELOPMENT_RESULT_VALUES",
    "ENVIRONMENT_OR_HARNESS_BLOCKED",
    "PASSED",
    "PASSED_CLEANUP_RESULT",
    "PASSED_TERMINAL_RESULT",
    "PROTOCOL_DEFECT_LOCALIZED",
    "PROTOCOL_STAGE_THRESHOLD",
    "Plan099ExitGateError",
    "SCHEMA",
    "SCHEMA_VERSION",
    "SKIPPED_CLEANUP_RESULT",
    "SKIPPED_TERMINAL_RESULT",
    "build_summary",
    "canonical_summary_digest",
    "classify_exit_result",
    "validate_summary",
]