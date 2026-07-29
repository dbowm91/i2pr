"""Validation and matching for the locked reference observation catalog."""

from __future__ import annotations

import hashlib
import re
import tomllib
from pathlib import Path
from typing import Any

SCHEMA = "i2pr-reference-observation-catalog-v1"
REVISION = 1
REFERENCES = {
    "java_i2p": ("2.12.0", "2800040deee9bb376567b671ef2e9c34cf3e30b6"),
    "i2pd": ("2.60.0", "f618e417dbd0b7c5956af8f0d5a6b0ee78caf35e"),
}
LEVELS = {"ntcp2_authenticated", "frame_authenticated_and_decrypted", "i2np_message_decoded"}
SANITIZERS = {"strip-ipv4-endpoint-prefix"}
_HANDSHAKE_MARKERS = {"SessionConfirmed sent", "SessionConfirmed from", "NTCP2 connection established"}


class CatalogError(ValueError):
    pass


def _default_path() -> Path:
    return Path(__file__).resolve().parents[1] / "reference-observation-catalog.toml"


def load_catalog(path: Path | None = None) -> dict[str, Any]:
    source = path or _default_path()
    try:
        catalog = tomllib.loads(source.read_text(encoding="utf-8"))
    except (OSError, tomllib.TOMLDecodeError) as exc:
        raise CatalogError("observation-catalog-unreadable") from exc
    validate_catalog(catalog)
    return catalog


def validate_catalog(catalog: dict[str, Any]) -> None:
    if catalog.get("schema") != SCHEMA or catalog.get("revision") != REVISION:
        raise CatalogError("observation-catalog-schema-invalid")
    for reference, (version, revision) in REFERENCES.items():
        section = catalog.get(reference)
        if not isinstance(section, dict) or section.get("version") != version or section.get("revision") != revision:
            raise CatalogError(f"{reference}-revision-mismatch")
        observations = section.get("observations")
        if not isinstance(observations, list):
            raise CatalogError(f"{reference}-observations-missing")
        seen: set[str] = set()
        for entry in observations:
            if not isinstance(entry, dict):
                raise CatalogError("observation-entry-invalid")
            level = entry.get("semantic_level")
            if level not in LEVELS or level in seen:
                raise CatalogError("observation-semantic-level-invalid-or-duplicate")
            seen.add(level)
            for field in ("source_path", "symbol", "marker", "sanitization_rule"):
                if not isinstance(entry.get(field), str) or not entry[field]:
                    raise CatalogError(f"observation-{field}-missing")
            if entry["sanitization_rule"] not in SANITIZERS:
                raise CatalogError("observation-sanitizer-unrecognized")
            if not isinstance(entry.get("minimum_count"), int) or entry["minimum_count"] < 1:
                raise CatalogError("observation-minimum-count-invalid")
            if level != "ntcp2_authenticated" and entry["marker"] in _HANDSHAKE_MARKERS:
                raise CatalogError("handshake-marker-claims-data-level")
        if seen != LEVELS:
            raise CatalogError(f"{reference}-observation-levels-incomplete")


def entries_for(catalog: dict[str, Any], reference: str) -> dict[str, dict[str, Any]]:
    validate_catalog(catalog)
    if reference not in REFERENCES:
        raise CatalogError("unknown-reference")
    return {entry["semantic_level"]: entry for entry in catalog[reference]["observations"]}


def marker_for(catalog: dict[str, Any], reference: str, level: str) -> dict[str, Any]:
    entries = entries_for(catalog, reference)
    try:
        return entries[level]
    except KeyError as exc:
        raise CatalogError("unknown-observation-level") from exc


def match_marker(catalog: dict[str, Any], reference: str, level: str, line: str) -> bool:
    marker = marker_for(catalog, reference, level)["marker"]
    return marker in line


def observation_catalog_digest(path: Path | None = None) -> str:
    source = path or _default_path()
    return hashlib.sha256(source.read_bytes()).hexdigest()


def sanitize_marker_line(line: str) -> str:
    return re.sub(r"\b(?:192\.0\.2|198\.51\.100|203\.0\.113)\.\d+\b(?::\d+)?", "synthetic-endpoint", line)


def drift_against_markdown(markdown_path: Path, digests: tuple[str, str]) -> bool:
    """Return True when both digests appear in the markdown.

    The catalog and the documentation agree only when both digests are
    present. The function returns True on match and False on drift.
    """

    if not markdown_path.is_file():
        return False
    text = markdown_path.read_text(encoding="utf-8", errors="replace")
    return all(digest in text for digest in digests)

