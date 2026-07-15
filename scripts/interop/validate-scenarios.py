#!/usr/bin/env python3
"""Validate the checked-in Plan 038 and Plan 041 scenario schemas."""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tests/integration/ntcp2"))
from harness.reference_scenario import ReferenceScenarioError, load_reference_scenario


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
    print("validated eight Plan 038 scenarios and two Plan 041 reference-pair scenarios")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
