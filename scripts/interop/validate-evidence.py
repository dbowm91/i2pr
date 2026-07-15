#!/usr/bin/env python3
"""Validate committed Plan 038 evidence without exposing its contents."""

from __future__ import annotations

import sys
from pathlib import Path

sys.path.insert(0, str(Path(__file__).resolve().parents[2] / "tests/integration/ntcp2"))
from harness.evidence import EvidenceError, validate_file  # noqa: E402


def main() -> int:
    evidence = Path(__file__).resolve().parents[2] / "tests/integration/ntcp2/evidence"
    records = sorted(path for path in evidence.glob("*.json") if path.is_file())
    for path in records:
        try:
            validate_file(path)
        except EvidenceError:
            print("invalid sanitized NTCP2 evidence record", file=sys.stderr)
            return 1
    if records:
        print(f"validated {len(records)} sanitized NTCP2 evidence record(s)")
    else:
        print("no sanitized mixed-router records committed; absence is not a success")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
