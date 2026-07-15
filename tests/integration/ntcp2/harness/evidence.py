"""Sanitized, typed evidence records for the Plan 038 harness.

This module deliberately has no router, socket, or network dependencies. Raw
logs and secret-bearing run state remain outside the record schema and are
deleted by the runner before a scenario is considered complete.
"""

from __future__ import annotations

import hashlib
import json
import re
from pathlib import Path
from typing import Any

RECORD_FIELDS = (
    "schema",
    "scenario_id",
    "date_utc",
    "i2pr_commit",
    "reference",
    "reference_version",
    "reference_revision",
    "artifact_sha256",
    "installed_tree_sha256",
    "configuration_sha256",
    "namespace_topology_sha256",
    "direction",
    "address_family",
    "deterministic_parameters",
    "expected",
    "actual_typed_result",
    "resource_counters",
    "process_counters",
    "cleanup_result",
    "evidence_sha256",
    "known_deviation",
    "reproduction",
)

_FORBIDDEN = re.compile(
    r"(?:-----BEGIN[^\n]*PRIVATE KEY-----|router\.identity|ntcp2\.static\.key|"
    r"\.pcap(?:ng)?\b|RouterInfo|I2NP|/home/|/root/|[A-Fa-f0-9]{80,})",
    re.IGNORECASE,
)
_ENDPOINT = re.compile(r"(?:\d{1,3}\.){3}\d{1,3}:\d+|\[[0-9a-f:]+\]:\d+", re.IGNORECASE)


class EvidenceError(ValueError):
    """Raised when a result would cross the sanitized evidence boundary."""


def _scan(value: Any) -> None:
    if isinstance(value, str):
        if _FORBIDDEN.search(value) or _ENDPOINT.search(value):
            raise EvidenceError("record contains forbidden secret, payload, path, or endpoint text")
    elif isinstance(value, dict):
        for key, child in value.items():
            _scan(key)
            _scan(child)
    elif isinstance(value, (list, tuple)):
        for child in value:
            _scan(child)


def validate_record(record: dict[str, Any]) -> None:
    """Validate the exact typed-record shape and sanitation rules."""

    if tuple(record) != RECORD_FIELDS:
        raise EvidenceError("record fields do not match the locked schema")
    if record["schema"] != 1:
        raise EvidenceError("unsupported evidence schema")
    for field in RECORD_FIELDS:
        if field not in record:
            raise EvidenceError(f"missing evidence field: {field}")
    _scan(record)
    if record["actual_typed_result"] not in {
        "passed",
        "rejected",
        "skipped_ipv6",
        "blocked_missing_driver",
        "blocked_host_contract",
        "failed_cleanup",
    }:
        raise EvidenceError("unknown typed result")
    if record["cleanup_result"] not in {"clean", "forced", "failed", "not-started"}:
        raise EvidenceError("unknown cleanup result")


def write_record(path: Path, record: dict[str, Any]) -> str:
    """Validate and write one canonical JSON record, returning its hash."""

    validate_record(record)
    unsigned = dict(record)
    unsigned["evidence_sha256"] = ""
    canonical = json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()
    digest = hashlib.sha256(canonical).hexdigest()
    record["evidence_sha256"] = digest
    validate_record(record)
    path.parent.mkdir(mode=0o700, parents=True, exist_ok=True)
    path.write_text(json.dumps(record, sort_keys=False, separators=(",", ":")) + "\n", encoding="utf-8")
    path.chmod(0o600)
    return digest


def validate_file(path: Path) -> None:
    """Validate one JSON evidence record from disk."""

    try:
        value = json.loads(path.read_text(encoding="utf-8"))
    except (OSError, UnicodeError, json.JSONDecodeError) as exc:
        raise EvidenceError("evidence file is not valid UTF-8 JSON") from exc
    if not isinstance(value, dict):
        raise EvidenceError("evidence record must be a JSON object")
    validate_record(value)
