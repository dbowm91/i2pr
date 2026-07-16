#!/usr/bin/env python3
"""Compare sanitized host state snapshots while ignoring the canonical VM."""

from __future__ import annotations

import argparse
import json
from pathlib import Path


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--baseline", type=Path, required=True)
    parser.add_argument("--actual", type=Path, required=True)
    parser.add_argument("--canonical", required=True)
    args = parser.parse_args()
    expected = json.loads(args.baseline.read_text(encoding="utf-8"))
    actual = json.loads(args.actual.read_text(encoding="utf-8"))
    unrelated = lambda value: [entry for entry in value if entry.get("name") != args.canonical]
    if unrelated(expected.get("instances", [])) != unrelated(actual.get("instances", [])):
        raise SystemExit("unrelated Multipass instance state changed")
    for key in ("route_sha256", "link_sha256", "firewall_sha256", "host_userns_clone", "host_apparmor_restrict_unprivileged_userns", "host_apparmor_enabled"):
        if expected.get(key) != actual.get(key):
            raise SystemExit(f"host state changed: {key}")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
