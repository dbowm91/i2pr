#!/usr/bin/env python3
"""Guard global implementation-plan number ownership.

Closure records and supporting documents may repeat a number within the
owning subsystem. Cross-subsystem implementation ownership is forbidden, except
for the exact collisions declared in `plans/global-number-collision-ledger.md`,
which this guard reads rather than restating.

Exit codes: 0 = ownership is clean; 1 = a violation; 2 = the ledger could not
be read, so the check could not run.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from pathlib import Path


# The tolerated collisions are NOT hardcoded here. They are read from
# `plans/global-number-collision-ledger.md`, which is the authority they name
# in their own first line.
#
# This block previously carried a hand-maintained dict that duplicated the
# ledger's table exactly. That is the same defect Plan 372 was registered to
# close -- a published inventory maintained in two places by hand, where one
# side goes stale silently -- and it failed the moment a collision appeared:
# adding the ledger row alone left this guard red, which is what surfaced it.
# Deriving the allowlist from the ledger makes the ledger the single place a
# collision is declared, and a new collision cannot be recorded in one place
# and forgotten in the other.
#
# The exact-set requirement is unchanged: a collision is tolerated only when
# the set of owners on disk equals the set the ledger declares. A *third*
# owner of 368, or a renamed plan, still fails.

LEDGER = Path("plans/global-number-collision-ledger.md")
LEDGER_ROW = re.compile(r"^\|\s*(?P<label>[^|]+?)\s*\|\s*\[(?P<impl>[^\]]+)\]\([^)]*\)\s*\|")


def load_tolerated_collisions(plans_root: Path) -> dict[str, set[str]]:
    """Build the tolerated-collision allowlist from the ledger.

    Fails closed. A ledger that cannot be read or whose plan-collision table
    cannot be found raises, because the alternative -- treating an unreadable
    authority as an empty allowlist -- would turn every recorded collision into
    a hard failure, and treating it as a permissive one would defeat the guard.
    """
    ledger = plans_root.parent / LEDGER if plans_root.parent != Path(".") else LEDGER
    if not ledger.is_file():
        raise RuntimeError(
            f"collision ledger not found at {ledger}; refusing to check plan-number "
            f"ownership without its authority"
        )
    text = ledger.read_text(encoding="utf-8")

    tolerated: dict[str, set[str]] = {}
    in_table = False
    saw_section = False
    for line in text.splitlines():
        if line.startswith("## "):
            in_table = line.startswith("## Plan-number collisions")
            saw_section = saw_section or in_table
            continue
        if not in_table or not line.startswith("|"):
            continue
        row = LEDGER_ROW.match(line)
        if not row:
            continue
        # The qualified label is "<Subsystem>/<number>"; the number is the key.
        number = row.group("label").rsplit("/", 1)[-1].strip()
        if not number.isdigit():
            continue
        # The link text is backticked in the ledger (`path.md`); the paths on
        # disk are not, so the backticks must come off or the set comparison
        # below can never match.
        tolerated.setdefault(number, set()).add(
            row.group("impl").strip().strip("`")
        )

    # A *missing section* fails closed, because it means the ledger's shape
    # changed and this parser is reading nothing rather than reading "none".
    # An empty table does not: a repository with no historical collisions is a
    # legitimate state, and its correct allowlist is the empty set, under which
    # every collision found on disk still fails.
    if not saw_section:
        raise RuntimeError(
            f"no `## Plan-number collisions` section in {ledger}; the ledger's shape "
            f"has changed, so collisions cannot be checked against it"
        )
    return tolerated


PLAN_NAME = re.compile(r"^(\d{3})-[^/]+\.md$")


def find_collisions(plans_root: Path, tolerated: dict[str, set[str]] | None = None) -> list[str]:
    owners: dict[str, dict[str, set[str]]] = defaultdict(lambda: defaultdict(set))
    implementations = plans_root / "implementation"
    for path in sorted(implementations.rglob("*.md")):
        match = PLAN_NAME.match(path.name)
        if not match:
            continue
        number = match.group(1)
        relative = path.relative_to(plans_root).as_posix()
        subsystem = path.parent.relative_to(implementations).as_posix()
        owners[number][subsystem].add(relative)

    errors: list[str] = []
    for number, by_subsystem in sorted(owners.items()):
        if len(by_subsystem) <= 1:
            continue
        paths = {path for subsystem_paths in by_subsystem.values() for path in subsystem_paths}
        allowed = (tolerated or {}).get(number)
        if allowed is not None and paths == allowed:
            continue
        rendered = ", ".join(sorted(paths))
        errors.append(f"global plan {number} has multiple subsystem owners: {rendered}")

    for path in sorted((plans_root / "closure").rglob("*.md")):
        match = PLAN_NAME.match(path.name)
        if not match:
            continue
        number = match.group(1)
        owning_subsystems = set(owners.get(number, {}))
        subsystem = path.parent.relative_to(plans_root / "closure").as_posix()
        if owning_subsystems and subsystem not in owning_subsystems:
            relative = path.relative_to(plans_root).as_posix()
            expected = ", ".join(sorted(owning_subsystems))
            errors.append(
                f"closure {relative} is outside the owning subsystem(s) for plan {number}: {expected}"
            )
    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--plans-root", type=Path, default=Path("plans"))
    args = parser.parse_args()
    try:
        tolerated = load_tolerated_collisions(args.plans_root)
    except RuntimeError as exc:
        print(f"Global plan-number ownership check could not run: {exc}", file=sys.stderr)
        return 2
    errors = find_collisions(args.plans_root, tolerated)
    if errors:
        print("Global plan-number ownership check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Global plan-number ownership check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
