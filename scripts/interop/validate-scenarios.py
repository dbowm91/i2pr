#!/usr/bin/env python3
"""Validate the checked-in Plan 038, Plan 041, and Plan 044 scenario schemas."""

from __future__ import annotations

import re
import sys
import tomllib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tests/integration/ntcp2"))
from harness.reference_scenario import ReferenceScenarioError, load_reference_scenario


_MIXED_FIELDS = frozenset({
    "id", "reference", "profile", "direction", "address_family",
    "padding", "expected", "initiator", "responder",
})
_MIXED_ID = re.compile(r"^[a-z0-9](?:[a-z0-9-]{0,60}[a-z0-9])?$")
_MIXED_DIRECTIONS = frozenset({"i2pr-to-reference", "reference-to-i2pr"})
_MIXED_REFERENCES = frozenset({"java_i2p", "i2pd"})
_MIXED_PAD = frozenset({"minimum-variable-maximum"})
_MIXED_SMOKES = frozenset({"handshake-smoke"})
_MIXED_EXPECTED = frozenset({
    "authenticated-handshake-and-bounded-i2np-exchange",
})
_MIXED_ROLES = frozenset({"i2pr", "java_i2p", "i2pd"})


def _validate_mixed_scenario(value: dict) -> str:
    unknown = frozenset(value) - _MIXED_FIELDS
    if unknown:
        raise ValueError(f"mixed-scenario unknown fields: {sorted(unknown)}")
    sid = value.get("id", "")
    if not isinstance(sid, str) or not _MIXED_ID.fullmatch(sid):
        raise ValueError(f"mixed-scenario invalid id: {sid}")
    if value.get("reference") not in _MIXED_REFERENCES:
        raise ValueError(f"mixed-scenario invalid reference: {value.get('reference')}")
    if value.get("profile") not in _MIXED_SMOKES:
        raise ValueError(f"mixed-scenario invalid profile: {value.get('profile')}")
    if value.get("direction") not in _MIXED_DIRECTIONS:
        raise ValueError(f"mixed-scenario invalid direction: {value.get('direction')}")
    if value.get("address_family") not in {"ipv4", "ipv6"}:
        raise ValueError(f"mixed-scenario invalid address_family")
    if value.get("padding") not in _MIXED_PAD:
        raise ValueError(f"mixed-scenario invalid padding: {value.get('padding')}")
    if value.get("expected") not in _MIXED_EXPECTED:
        raise ValueError(f"mixed-scenario invalid expected: {value.get('expected')}")
    if value.get("initiator") not in _MIXED_ROLES:
        raise ValueError(f"mixed-scenario invalid initiator: {value.get('initiator')}")
    if value.get("responder") not in _MIXED_ROLES:
        raise ValueError(f"mixed-scenario invalid responder: {value.get('responder')}")
    if value.get("initiator") == value.get("responder"):
        raise ValueError("mixed-scenario initiator and responder must differ")
    ref = value["reference"]
    direction = value["direction"]
    initiator = value["initiator"]
    responder = value["responder"]
    if direction == "i2pr-to-reference" and initiator != "i2pr":
        raise ValueError("i2pr-to-reference direction requires i2pr as initiator")
    if direction == "i2pr-to-reference" and responder == "i2pr":
        raise ValueError("i2pr-to-reference direction requires reference as responder")
    if direction == "reference-to-i2pr" and responder != "i2pr":
        raise ValueError("reference-to-i2pr direction requires i2pr as responder")
    if direction == "reference-to-i2pr" and initiator == "i2pr":
        raise ValueError("reference-to-i2pr direction requires reference as initiator")
    if direction == "i2pr-to-reference" and initiator == ref:
        raise ValueError("i2pr-to-reference direction cannot have reference as initiator")
    if direction == "reference-to-i2pr" and responder == ref:
        raise ValueError("reference-to-i2pr direction cannot have reference as responder")
    return sid


def main() -> int:
    root = Path(__file__).resolve().parents[2]
    manifest = tomllib.loads((root / "tests/integration/ntcp2/manifest.toml").read_text())
    expected = {item["id"] for item in manifest["scenario"]}
    files = sorted((root / "tests/integration/ntcp2/scenarios").glob("*.toml"))
    values = [tomllib.loads(path.read_text()).get("scenario", {}) for path in files]
    actual = {value.get("id") for value in values}
    if len(files) != 8 or actual != expected or any(not value.get("reference") for value in values):
        print("Plan 038 scenario files do not match the eight-entry manifest", file=sys.stderr)
        return 1
    reference_root = root / "tests/integration/ntcp2/reference-scenarios"
    reference_manifest = tomllib.loads((reference_root / "manifest.toml").read_text(encoding="utf-8"))
    reference_files = sorted(reference_root.glob("reference-*.toml"))
    expected_reference = {item["id"] for item in reference_manifest["scenario"]}
    try:
        actual_reference = {load_reference_scenario(path).scenario_id for path in reference_files}
    except (ReferenceScenarioError, OSError, UnicodeError, tomllib.TOMLDecodeError) as exc:
        print(f"invalid Plan 041 reference-pair scenario: {exc}", file=sys.stderr)
        return 1
    if len(reference_files) != 2 or actual_reference != expected_reference:
        print("Plan 041 reference-pair scenario files do not match their manifest", file=sys.stderr)
        return 1
    mixed_root = root / "tests/integration/ntcp2/mixed-scenarios"
    mixed_manifest = tomllib.loads((mixed_root / "manifest.toml").read_text(encoding="utf-8"))
    mixed_files = sorted(mixed_root.glob("[!.]*.toml"))
    mixed_files = [f for f in mixed_files if f.name != "manifest.toml"]
    expected_mixed = {item["id"] for item in mixed_manifest["scenario"]}
    mixed_ids: set[str] = set()
    for path in mixed_files:
        raw = tomllib.loads(path.read_text(encoding="utf-8"))
        value = raw.get("scenario", {})
        try:
            sid = _validate_mixed_scenario(value)
            mixed_ids.add(sid)
        except (ValueError, OSError, UnicodeError, tomllib.TOMLDecodeError) as exc:
            print(f"invalid Plan 044 mixed scenario {path.name}: {exc}", file=sys.stderr)
            return 1
    if len(mixed_files) != 4 or mixed_ids != expected_mixed:
        print(
            f"Plan 044 mixed scenario files do not match their manifest "
            f"(found {len(mixed_files)} files, expected {len(expected_mixed)} ids)",
            file=sys.stderr,
        )
        return 1
    print(
        "validated eight Plan 038 scenarios, two Plan 041 reference-pair scenarios, "
        "and four Plan 044 mixed-router directional scenarios"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
