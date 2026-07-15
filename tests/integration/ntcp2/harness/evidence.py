"""Sanitized, typed evidence records for the Plan 038 harness.

This module deliberately has no router, socket, or network dependencies. Raw
logs and secret-bearing run state remain outside the record schema and are
deleted by the runner before a scenario is considered complete.
"""

from __future__ import annotations

import hashlib
import json
import os
import re
import tempfile
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
_HEX40 = re.compile(r"^[0-9a-f]{40};(?:clean|dirty)$")
_HEX64 = re.compile(r"^[0-9a-f]{64}$")


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
    if record["reference"] not in {"java_i2p", "i2pd"}:
        raise EvidenceError("non-canonical reference identifier")
    expected_version = "2.12.0" if record["reference"] == "java_i2p" else "2.60.0"
    if record["reference_version"] != expected_version:
        raise EvidenceError("reference version does not match identifier")
    if not _HEX40.fullmatch(str(record["i2pr_commit"])):
        raise EvidenceError("i2pr commit is not an exact commit plus disposition")
    if not re.fullmatch(r"[0-9a-f]{40}", str(record["reference_revision"])):
        raise EvidenceError("reference revision is not a full object ID")
    for field in ("artifact_sha256", "installed_tree_sha256", "configuration_sha256", "namespace_topology_sha256"):
        value = str(record[field])
        if not _HEX64.fullmatch(value):
            raise EvidenceError(f"{field} is not a SHA-256 digest")
        if record["actual_typed_result"] == "passed" and value == "0" * 64:
            raise EvidenceError(f"passed record contains a zero-filled {field}")
    if record["actual_typed_result"] == "passed":
        if record["cleanup_result"] not in {"clean", "forced"}:
            raise EvidenceError("passed record did not clean up")
        if record["i2pr_commit"] == "record-at-execution":
            raise EvidenceError("passed record contains an execution placeholder")
    if record["evidence_sha256"] and not _HEX64.fullmatch(str(record["evidence_sha256"])):
        raise EvidenceError("evidence digest is not a SHA-256 digest")


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
    fd, temporary = tempfile.mkstemp(prefix=f".{path.name}.", dir=path.parent)
    try:
        with os.fdopen(fd, "w", encoding="utf-8") as handle:
            handle.write(json.dumps(record, sort_keys=False, separators=(",", ":")) + "\n")
            handle.flush()
            os.fsync(handle.fileno())
        os.chmod(temporary, 0o600)
        os.replace(temporary, path)
    finally:
        if os.path.exists(temporary):
            os.unlink(temporary)
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
    if not value["evidence_sha256"]:
        raise EvidenceError("evidence record is not finalized")
    unsigned = dict(value)
    expected = unsigned["evidence_sha256"]
    unsigned["evidence_sha256"] = ""
    actual = hashlib.sha256(json.dumps(unsigned, sort_keys=True, separators=(",", ":")).encode()).hexdigest()
    if actual != expected:
        raise EvidenceError("evidence digest mismatch")
