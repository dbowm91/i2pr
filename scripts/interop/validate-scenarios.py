#!/usr/bin/env python3
"""Validate the checked-in Plan 038 scenario schema."""

from __future__ import annotations

import sys
import tomllib
from pathlib import Path


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
    print("validated eight Plan 038 scenario definitions")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
