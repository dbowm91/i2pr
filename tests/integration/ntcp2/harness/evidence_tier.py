"""Plan 068 evidence-tier types and tier separation rules.

ADR 0023 (``docs/adr/0023-staged-ntcp2-interoperability-evidence.md``)
separates NTCP2 interoperability evidence into four bounded tiers.
This module exposes:

- :data:`LOCAL_CONFORMANCE` — Level 0 deterministic local conformance.
- :data:`EXTERNAL_LOOPBACK_SMOKE` — Level 1 host loopback smoke.
- :data:`REPEATED_DEVELOPMENT_INTEROP` — Level 2 development validation.
- :data:`CONDITIONAL_DIFFERENTIAL` — Level 2D Emissary differential.
- :data:`RELEASE_QUALIFICATION` — Level 3 release qualification.
- :func:`is_valid_tier` — validate a tier string.
- :func:`all_tiers` — enumerate every allowlisted tier.
- :func:`assert_release_record_tier` — refuse lower-tier records inside
  a release bundle.
- :func:`development_record_cannot_satisfy_release` — refuse development
  records inside a release bundle.

The tier separation rules are:

- A record declares exactly one tier.
- ``external-loopback-smoke`` cannot satisfy development or release
  predicates.
- ``repeated-development-interop`` cannot satisfy release predicates.
- ``conditional-differential`` cannot substitute for the required
  Java + i2pd release-qualification matrix.
- Historical Plan 052/053/056/058/059/066 bundle readers remain
  readable for audit; no existing release schema is silently
  reinterpreted.

The function bodies are intentionally minimal and table-driven. The
module has no I/O and no external dependencies.
"""

from __future__ import annotations

from typing import Final


LOCAL_CONFORMANCE: Final[str] = "local-conformance"
EXTERNAL_LOOPBACK_SMOKE: Final[str] = "external-loopback-smoke"
REPEATED_DEVELOPMENT_INTEROP: Final[str] = "repeated-development-interop"
CONDITIONAL_DIFFERENTIAL: Final[str] = "conditional-differential"
RELEASE_QUALIFICATION: Final[str] = "release-qualification"


ALL_TIERS: Final[tuple[str, ...]] = (
    LOCAL_CONFORMANCE,
    EXTERNAL_LOOPBACK_SMOKE,
    REPEATED_DEVELOPMENT_INTEROP,
    CONDITIONAL_DIFFERENTIAL,
    RELEASE_QUALIFICATION,
)


_RELEASE_REQUIRED: Final[frozenset[str]] = frozenset({RELEASE_QUALIFICATION})


class EvidenceTierError(ValueError):
    """Raised when an evidence record violates the tier-separation rules."""


def is_valid_tier(value: object) -> bool:
    """Return ``True`` when ``value`` is one of the allowlisted tier strings."""

    return isinstance(value, str) and value in ALL_TIERS


def all_tiers() -> tuple[str, ...]:
    """Return every allowlisted tier value."""

    return ALL_TIERS


def tier_satisfies_release(tier: object) -> bool:
    """Return ``True`` only when ``tier`` is ``release-qualification``."""

    return tier == RELEASE_QUALIFICATION


def tier_satisfies_development(tier: object) -> bool:
    """Return ``True`` when ``tier`` meets a development (Level 2) predicate.

    Both ``release-qualification`` and ``repeated-development-interop``
    satisfy a development predicate; lower tiers do not.
    """

    return tier in (RELEASE_QUALIFICATION, REPEATED_DEVELOPMENT_INTEROP)


def assert_release_record_tier(record: object) -> None:
    """Refuse a release bundle record whose tier is not release-grade.

    The release bundle validator must reject every record whose
    ``evidence_tier`` field is missing or carries a lower-tier value.
    """

    if not isinstance(record, dict):
        raise EvidenceTierError("release record must be a JSON object")
    tier = record.get("evidence_tier")
    if tier not in _RELEASE_REQUIRED:
        raise EvidenceTierError(
            "release bundle cannot accept lower-tier record "
            f"(evidence_tier={tier!r})"
        )


def development_record_cannot_satisfy_release(record: object) -> bool:
    """Return ``True`` when ``record`` must not be accepted by a release validator.

    Returns ``True`` when the record is **not** acceptable to a release
    validator (lower tier, missing tier, or non-dict). Returns ``False``
    only when the record is acceptable to a release validator.
    """

    try:
        assert_release_record_tier(record)
        return False
    except EvidenceTierError:
        return True


def smoke_record_must_not_pretend_release(record: object) -> bool:
    """Return ``True`` when a record is a smoke record offered as release.

    A legitimate smoke record carries ``evidence_tier =
    external-loopback-smoke``. A record that declares a stronger tier
    while otherwise looking like a smoke record is rejected.
    """

    if not isinstance(record, dict):
        return False
    declared_tier = record.get("evidence_tier")
    if declared_tier not in (REPEATED_DEVELOPMENT_INTEROP, RELEASE_QUALIFICATION):
        return False
    schema = record.get("schema", "")
    return schema.startswith("i2pr-ntcp2-loopback-smoke")