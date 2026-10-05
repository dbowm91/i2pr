#!/usr/bin/env python3
"""Check the reviewed public declarations of the reusable crate against a snapshot.

This dependency-free source snapshot is intentionally conservative: it records public
modules, root re-exports, and public item declarations. Review any snapshot update as an
API change; it is not a substitute for reviewing Rust signatures and semver impact.
"""

from __future__ import annotations

import argparse
import re
import sys
from pathlib import Path

ROOT = Path(__file__).resolve().parents[1]
SRC = ROOT / "crates/i2pr-service-tunnels/src"
SNAPSHOT = ROOT / "crates/i2pr-service-tunnels/API-SNAPSHOT.txt"
DECLARATION = re.compile(
    r"\bpub\s+(?:(?:async|unsafe|const)\s+)*(fn|struct|enum|trait|type|const|static|mod)\s+([A-Za-z_][A-Za-z0-9_]*)"
)
COMMENT = re.compile(r"//[^\n]*|/\*.*?\*/", re.S)


def parse_root_exports(source: str) -> set[str]:
    source = COMMENT.sub("", source)
    exports: set[str] = set()
    for match in re.finditer(r"\bpub\s+use\s+([^;]+);", source, re.S):
        statement = re.sub(r"//[^\n]*|/\*.*?\*/", "", match.group(1), flags=re.S)
        if "{" in statement and "}" in statement:
            prefix, names = statement.split("{", 1)
            names = names.rsplit("}", 1)[0]
            module = prefix.strip().rstrip(":")
            for raw_name in names.split(","):
                name = raw_name.strip()
                if not name:
                    continue
                exports.add(f"root-use:{module}::{name}")
        else:
            exports.add(f"root-use:{statement.strip()}")
    return exports


def declarations(source: str, module: str) -> set[str]:
    result = set()
    for kind, name in DECLARATION.findall(source):
        result.add(f"{module}:{kind}:{name}")
    return result


def extract() -> set[str]:
    entries: set[str] = set()
    for path in sorted(SRC.rglob("*.rs")):
        relative = path.relative_to(SRC).with_suffix("")
        module = "::".join(relative.parts)
        source = COMMENT.sub("", path.read_text(encoding="utf-8"))
        entries |= declarations(source, module)
        if path == SRC / "lib.rs":
            entries |= parse_root_exports(source)
    return entries


def self_test() -> None:
    fixture = "pub fn plan350_positive_control() {}\npub(crate) fn private_control() {}"
    found = declarations(fixture, "fixture")
    if "fixture:fn:plan350_positive_control" not in found:
        raise RuntimeError("API snapshot detector positive control failed")
    if any("private_control" in item for item in found):
        raise RuntimeError("API snapshot detector includes crate-private items")


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--write", action="store_true", help="write the reviewed snapshot")
    args = parser.parse_args()
    self_test()
    current = extract()
    rendered = "".join(f"{entry}\n" for entry in sorted(current))
    if args.write:
        SNAPSHOT.write_text(rendered, encoding="utf-8")
        print(f"wrote {len(current)} reviewed API declarations to {SNAPSHOT.relative_to(ROOT)}")
        return 0
    try:
        expected = SNAPSHOT.read_text(encoding="utf-8")
    except FileNotFoundError:
        print(f"missing API snapshot: {SNAPSHOT}", file=sys.stderr)
        return 1
    if rendered != expected:
        print("portable service-tunnel API differs from its reviewed snapshot", file=sys.stderr)
        print("review the source/API change, then run: python3 scripts/check-portable-service-tunnel-api.py --write", file=sys.stderr)
        return 1
    print(f"portable service-tunnel API snapshot passed ({len(current)} declarations)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
