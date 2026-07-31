"""Plan 068/071 Level 2 development validation summary contract.

The Level 2 summary aggregates repeated i2pd fresh-state passes per
direction with bounded negative controls. The summary is
development-validation evidence only; it cannot satisfy a
release-qualification predicate (see :mod:`evidence_tier`).

Schema: ``i2pr-ntcp2-development-validation-v1``.

Required fields:

- ``schema`` (constant ``i2pr-ntcp2-development-validation-v1``).
- ``schema_version`` (constant ``1``).
- ``evidence_tier`` (must be ``repeated-development-interop``).
- ``source_commit`` (40-lowercase-hex SHA-1).
- ``reference_name`` (allowlisted: ``i2pd`` or ``java_i2p``).
- ``reference_version`` (e.g., ``2.60.0``).
- ``reference_revision`` (40 or 64 lowercase hex).
- ``directions`` (mapping direction -> per-direction summary).
- ``required_passes_per_direction`` (positive int, bounded).
- ``observed_passes_per_direction`` (mapping direction -> int).
- ``negative_controls`` (mapping name -> result).
- ``cleanup_passed`` (bool).
- ``network_audit_summary`` (mapping direction -> audit value).
- ``status`` (``passed`` | ``failed`` | ``blocked``).
- ``summary_sha256`` (64-lowercase-hex digest excluding itself).

Each per-direction summary requires:

- ``direction`` (allowlisted).
- ``attempts`` (positive int).
- ``passes`` (nonnegative int, <= attempts).
- ``observed_message_id`` (nonzero u32).
- ``observed_local_router_hash_sha256`` (64 lowercase hex).
- ``observed_peer_router_hash_sha256`` (64 lowercase hex).

Rules:

- A passed summary requires every direction ``passes ==
  required_passes_per_direction``.
- A passed summary requires every named negative control to report
  ``outcome = rejected``.
- A passed summary requires ``cleanup_passed = True``.
- A passed summary requires every ``network_audit_summary`` value to be
  ``strace-allowlist`` or ``configuration-only``.
- ``summary_sha256`` covers the canonical JSON serialization excluding
  itself.
"""

from __future__ import annotations

import hashlib
import json
import re
from typing import Any, Final


SCHEMA: Final[str] = "i2pr-ntcp2-development-validation-v1"
SCHEMA_VERSION: Final[int] = 1

EVIDENCE_TIER: Final[str] = "repeated-development-interop"


DIRECTIONS: Final[frozenset[str]] = frozenset({
    "i2pr-to-i2pd-ipv4",
    "i2pd-to-i2pr-ipv4",
    "i2pr-to-java-ipv4",
    "java-to-i2pr-ipv4",
})


REFERENCE_NAMES: Final[frozenset[str]] = frozenset({"i2pd", "java_i2p"})


STATUS_VALUES: Final[frozenset[str]] = frozenset({
    "passed",
    "failed",
    "blocked",
})


NEGATIVE_CONTROL_OUTCOMES: Final[frozenset[str]] = frozenset({
    "rejected",
    "passed",
    "skipped",
})


NETWORK_AUDIT_VALUES: Final[frozenset[str]] = frozenset({
    "strace-allowlist",
    "configuration-only",
    "not-run",
})


REQUIRED_PER_DIRECTION_FIELDS: Final[tuple[str, ...]] = (
    "direction",
    "attempts",
    "passes",
    "observed_message_id",
    "observed_local_router_hash_sha256",
    "observed_peer_router_hash_sha256",
)


REQUIRED_FIELDS: Final[tuple[str, ...]] = (
    "schema",
    "schema_version",
    "evidence_tier",
    "source_commit",
    "reference_name",
    "reference_version",
    "reference_revision",
    "directions",
    "required_passes_per_direction",
    "observed_passes_per_direction",
    "negative_controls",
    "cleanup_passed",
    "network_audit_summary",
    "status",
    "summary_sha256",
)


ALLOWED_FIELDS: Final[frozenset[str]] = frozenset(REQUIRED_FIELDS)


SHA1_RE: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{40}$")
SHA256_RE: Final[re.Pattern[str]] = re.compile(r"^[0-9a-f]{64}$")


REQUIRED_NEGATIVE_CONTROLS: Final[frozenset[str]] = frozenset({
    "network_id_mismatch",
    "router_info_or_static_key_mismatch",
    "delivery_status_correlation_mismatch",
    "unauthenticated_data_phase",
})


class DevelopmentValidationError(ValueError):
    """Raised when a development summary violates the contract."""


def _require(cond: bool, msg: str) -> None:
    if not cond:
        raise DevelopmentValidationError(msg)


def canonical_summary_digest(summary: dict[str, Any]) -> str:
    """Return the canonical SHA-256 digest excluding ``summary_sha256``."""

    payload = {k: v for k, v in summary.items() if k != "summary_sha256"}
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":"))
    return hashlib.sha256(canonical.encode("utf-8")).hexdigest()


def _validate_per_direction(entry: Any) -> dict[str, Any]:
    _require(isinstance(entry, dict), "development direction entry must be dict")
    for field_name in REQUIRED_PER_DIRECTION_FIELDS:
        _require(
            field_name in entry,
            f"development direction entry missing field: {field_name}",
        )
    _require(
        entry["direction"] in DIRECTIONS,
        "development direction not allowlisted",
    )
    _require(
        isinstance(entry["attempts"], int) and entry["attempts"] > 0,
        "development attempts must be positive int",
    )
    _require(
        isinstance(entry["passes"], int) and 0 <= entry["passes"] <= entry["attempts"],
        "development passes must be int in [0, attempts]",
    )
    _require(
        isinstance(entry["observed_message_id"], int)
        and 1 <= entry["observed_message_id"] <= 0xFFFFFFFF,
        "development observed_message_id must be nonzero u32",
    )
    _require(
        SHA256_RE.match(entry["observed_local_router_hash_sha256"]) is not None,
        "development local_router_hash_sha256 must be 64 lowercase hex",
    )
    _require(
        SHA256_RE.match(entry["observed_peer_router_hash_sha256"]) is not None,
        "development peer_router_hash_sha256 must be 64 lowercase hex",
    )
    return entry


def validate_development_validation_summary(summary: Any) -> dict[str, Any]:
    """Validate a development summary and return the normalized dict."""

    _require(isinstance(summary, dict), "development summary must be JSON object")
    summary = dict(summary)

    for field_name in REQUIRED_FIELDS:
        _require(
            field_name in summary,
            f"development summary missing field: {field_name}",
        )

    for field_name in summary:
        _require(
            field_name in ALLOWED_FIELDS,
            f"development summary forbids unknown field: {field_name}",
        )

    _require(summary["schema"] == SCHEMA, "development summary schema mismatch")
    _require(
        summary["schema_version"] == SCHEMA_VERSION,
        "development summary schema_version mismatch",
    )
    _require(
        summary["evidence_tier"] == EVIDENCE_TIER,
        "development summary evidence_tier must be repeated-development-interop",
    )
    _require(
        SHA1_RE.match(summary["source_commit"]) is not None,
        "development summary source_commit must be 40 lowercase hex",
    )
    _require(
        summary["reference_name"] in REFERENCE_NAMES,
        "development summary reference_name not allowlisted",
    )
    _require(
        isinstance(summary["reference_version"], str) and summary["reference_version"],
        "development summary reference_version must be non-empty string",
    )
    _require(
        SHA1_RE.match(summary["reference_revision"]) is not None
        or SHA256_RE.match(summary["reference_revision"]) is not None,
        "development summary reference_revision must be 40 or 64 lowercase hex",
    )
    _require(
        isinstance(summary["required_passes_per_direction"], int)
        and 1 <= summary["required_passes_per_direction"] <= 1024,
        "development summary required_passes_per_direction must be int in [1, 1024]",
    )
    _require(
        isinstance(summary["directions"], dict) and summary["directions"],
        "development summary directions must be non-empty mapping",
    )
    for direction, entry in summary["directions"].items():
        _require(
            direction in DIRECTIONS,
            "development summary direction not allowlisted",
        )
        _validate_per_direction(entry)

    _require(
        isinstance(summary["observed_passes_per_direction"], dict),
        "development summary observed_passes_per_direction must be mapping",
    )
    for direction, count in summary["observed_passes_per_direction"].items():
        _require(
            direction in DIRECTIONS,
            "development summary observed direction not allowlisted",
        )
        _require(
            isinstance(count, int) and 0 <= count <= 1024,
            "development summary observed_passes_per_direction count must be int",
        )

    _require(
        isinstance(summary["negative_controls"], dict),
        "development summary negative_controls must be mapping",
    )
    for name, outcome in summary["negative_controls"].items():
        _require(
            outcome in NEGATIVE_CONTROL_OUTCOMES,
            f"development summary negative control outcome not allowlisted: {name}",
        )

    _require(
        isinstance(summary["cleanup_passed"], bool),
        "development summary cleanup_passed must be bool",
    )

    _require(
        isinstance(summary["network_audit_summary"], dict),
        "development summary network_audit_summary must be mapping",
    )
    for direction, audit in summary["network_audit_summary"].items():
        _require(
            direction in DIRECTIONS,
            "development summary network_audit_summary direction not allowlisted",
        )
        _require(
            audit in NETWORK_AUDIT_VALUES,
            "development summary network audit value not allowlisted",
        )

    _require(
        summary["status"] in STATUS_VALUES,
        "development summary status not allowlisted",
    )

    if summary["status"] == "passed":
        _require(
            summary["cleanup_passed"] is True,
            "passed development summary requires cleanup_passed = true",
        )
        for direction in summary["directions"]:
            observed = summary["observed_passes_per_direction"].get(direction, 0)
            _require(
                observed == summary["required_passes_per_direction"],
                f"passed development summary requires {direction} passes = "
                f"{summary['required_passes_per_direction']}",
            )
            audit = summary["network_audit_summary"].get(direction)
            _require(
                audit in ("strace-allowlist", "configuration-only"),
                f"passed development summary forbids {direction} audit = {audit}",
            )
        for control in REQUIRED_NEGATIVE_CONTROLS:
            _require(
                control in summary["negative_controls"],
                f"passed development summary requires negative control: {control}",
            )
            _require(
                summary["negative_controls"][control] == "rejected",
                f"passed development summary requires {control} = rejected",
            )

    expected = canonical_summary_digest(summary)
    _require(
        SHA256_RE.match(summary["summary_sha256"]) is not None,
        "development summary summary_sha256 must be 64 lowercase hex",
    )
    _require(
        summary["summary_sha256"] == expected,
        "development summary summary_sha256 digest mismatch",
    )

    return summary