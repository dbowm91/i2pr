"""Shared Plan 054 observation helpers for the reference adapters.

This module is the single home for:

- the bounded log cursor;
- the post-cursor exact-match scanner;
- the observation-v2 builder shared by both ``java_i2p.py`` and
  ``i2pd.py`` adapters.

Adapters that previously returned a flat ``authenticated``/``not-observed``
string still expose ``authenticated_observation``; the Plan 052 observation
record is built through ``collect_observation`` and goes through the
machine-readable observation catalog (TOML). The catalog is loaded lazily
on first call so tests that do not need observation-v2 do not pay the
TOML parse cost.
"""

from __future__ import annotations

import hashlib
import json
from dataclasses import dataclass
from pathlib import Path
from typing import Any

try:
    from .observation_catalog import (
        CatalogError,
        entries_for,
        load_catalog,
        match_marker,
    )
except ImportError:  # unittest discovery uses the flat path.
    from observation_catalog import (  # type: ignore
        CatalogError,
        entries_for,
        load_catalog,
        match_marker,
    )


_OBSERVATION_LEVELS = (
    "process_started",
    "listener_ready",
    "tcp_connected",
    "ntcp2_authenticated",
    "frame_emitted",
    "frame_authenticated_and_decrypted",
    "i2np_message_decoded",
    "terminal_clean",
)


@dataclass(frozen=True)
class LogCursor:
    run_id: str
    log_path: Path
    start_offset: int = 0

    def open(self) -> tuple[object, int]:
        handle = open(self.log_path, "rb")
        handle.seek(self.start_offset)
        return handle, handle.tell()

    def resume_offset(self) -> int:
        if not self.log_path.is_file():
            return 0
        return self.log_path.stat().st_size


def _empty_observation_levels(reason: str = "not-observed") -> dict[str, dict[str, str]]:
    return {
        level: {
            "state": "not-observed",
            "source": "typed-status",
            "evidence_code": reason,
            "sanitized_detail": "",
            "observer_implementation": "observation-schema-v2",
        }
        for level in _OBSERVATION_LEVELS
    }


def _scan_log_for_levels(
    cursor: LogCursor,
    *,
    reference: str,
    catalog: dict | None,
    correlation: dict[str, str] | None = None,
) -> tuple[dict[str, dict[str, Any]], int, list[str]]:
    if catalog is None:
        catalog = load_catalog()
    entries = entries_for(catalog, reference)
    levels: dict[str, dict[str, Any]] = {}
    for level in _OBSERVATION_LEVELS:
        if level == "frame_emitted" or level == "terminal_clean":
            levels[level] = {
                "state": "not-applicable",
                "source": "typed-status",
                "evidence_code": "not-applicable-for-this-side",
                "sanitized_detail": "",
                "observer_implementation": f"{reference}-observation-catalog-v1",
            }
        elif level in entries:
            levels[level] = {
                "state": "not-observed",
                "source": "source-derived-log-marker",
                "evidence_code": "not-observed",
                "sanitized_detail": "",
                "observer_implementation": f"{reference}-observation-catalog-v1",
            }
        else:
            levels[level] = {
                "state": "not-observed",
                "source": "typed-status",
                "evidence_code": "not-observed",
                "sanitized_detail": "",
                "observer_implementation": "observation-schema-v2",
            }
    sanitized: list[str] = []
    scanned = 0
    correlation_keys = {value for value in (correlation or {}).values() if value}
    if not cursor.log_path.is_file():
        return levels, 0, sanitized
    try:
        handle, _ = cursor.open()
    except OSError:
        return levels, 0, sanitized
    try:
        text = handle.read().decode("utf-8", errors="replace")
    finally:
        handle.close()
    for line in text.splitlines():
        scanned += 1
        for level, entry in entries.items():
            if match_marker(catalog, reference, level, line):
                sanitized_line = _sanitize(line, entry.get("sanitization_rule", "strip-ipv4-endpoint-prefix"))
                if correlation_keys and level == "i2np_message_decoded" and not any(key in line for key in correlation_keys):
                    continue
                bucket = levels[level]
                if bucket["state"] != "observed":
                    bucket["state"] = "observed"
                    bucket["evidence_code"] = level
                    bucket["sanitized_detail"] = sanitized_line
                    bucket["count"] = 1
                else:
                    bucket["count"] = int(bucket.get("count", 1)) + 1
                if level == "ntcp2_authenticated":
                    _propagate_handshake_to_handshake_levels(levels)
                sanitized.append(sanitized_line)
    return levels, scanned, sanitized


def _propagate_handshake_to_handshake_levels(levels: dict[str, dict[str, Any]]) -> None:
    for level in ("process_started", "listener_ready", "tcp_connected"):
        if level in levels and levels[level]["state"] != "observed":
            levels[level] = {
                "state": "observed",
                "source": "typed-status",
                "evidence_code": level,
                "sanitized_detail": "derived-from-handshake-marker",
                "observer_implementation": "observation-schema-v2",
                "count": 1,
            }


def _sanitize(line: str, rule: str) -> str:
    if rule == "strip-ipv4-endpoint-prefix":
        try:
            from .observation_catalog import sanitize_marker_line
        except ImportError:
            from observation_catalog import sanitize_marker_line  # type: ignore
        return sanitize_marker_line(line)
    return line


def _observation_digest(payload: dict[str, Any]) -> str:
    canonical = json.dumps(payload, sort_keys=True, separators=(",", ":")).encode()
    return hashlib.sha256(canonical).hexdigest()


def build_observation(
    *,
    side: str,
    role: str,
    run_id: str,
    cursor: LogCursor,
    correlation: dict[str, str] | None = None,
    catalog: dict | None = None,
    fallback: str = "not-observed",
) -> dict[str, Any]:
    """Build one finalized Plan 052 observation-v2 record for ``side``."""

    try:
        levels, scan_count, sanitized = _scan_log_for_levels(cursor, reference=side, catalog=catalog, correlation=correlation)
    except CatalogError:
        levels = _empty_observation_levels(fallback)
        scan_count = 0
        sanitized = []
    observation: dict[str, Any] = {
        "schema": "i2pr-ntcp2-direction-observation-v2",
        "schema_version": 2,
        "side": side,
        "role": role,
        "run_id": run_id,
        "run_correlation": correlation or {},
        "levels": levels,
        "scan_count": scan_count,
        "sanitized_detail_excerpt": sanitized[:4],
        "observation_sha256": "",
    }
    observation["observation_sha256"] = _observation_digest({k: v for k, v in observation.items() if k != "observation_sha256"})
    return observation


def _empty_observation(reason: str = "not-started") -> dict[str, Any]:
    return {
        "schema": "i2pr-ntcp2-direction-observation-v2",
        "schema_version": 2,
        "side": "unknown",
        "levels": _empty_observation_levels(reason),
        "observation_sha256": "0" * 64,
    }
