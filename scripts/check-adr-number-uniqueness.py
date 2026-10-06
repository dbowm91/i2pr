#!/usr/bin/env python3
"""Guard ADR number ownership in ``docs/adr/``.

An ADR number is a load-bearing reference key: status tokens, supersession
chains, closure cross-references, ``specs/support.toml`` ADR lists, and source
comments all cite it by number. Two documents sharing a number make those
citations ambiguous -- ADR 0029's "partially superseded by ADR 0030" does not
name a file. Plan numbers already have an equivalent guard in
``scripts/check-global-plan-number-uniqueness.py``; this closes the same gap
for ADRs.

Run it with ``python3``, never ``bash``: ``bash`` garbles this script and exits
2, which is indistinguishable from content drift.

Scope: identity only. This guard does not read ADR content, status tokens, or
supersession chains.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from pathlib import Path


# Authority for the tolerated set below. Cross-check every entry against this
# document before changing it, and add a row to its ADR-number table in the
# same change.
LEDGER = "plans/global-number-collision-ledger.md"

# The finite, explicitly recorded ADR-number collisions that already exist in
# docs/adr/. Each entry maps an ADR number to the EXACT set of files that
# currently claim it. A collision is tolerated only when the observed file set
# equals the recorded set exactly, so neither a third claimant nor a renamed
# file is grandfathered in.
#
# Adding an exemption requires editing this literal, so it shows up in review.
# Never widen it with a glob, a prefix match, or a silent `continue`.
TOLERATED_DUPLICATES = {
    # ADR 0030 -- pre-existing duplicate, both records `Accepted`. Plan 353
    # instructed that these be left untouched and the ambiguity recorded rather
    # than silently renumbered; resolving what 0029 meant is an ADR-level
    # decision, not a tooling fix.
    "0030": frozenset(
        {
            "0030-destination-linkability-domains-service-lifecycle-and-i2pd-streaming.md",
            "0030-loopback-controlled-floodfill-reachability-advertisement.md",
        }
    ),
    # ADR 0032 -- one managed-native-app decision and one Proposal 170
    # (ELS2 type-11) decision.
    "0032": frozenset(
        {
            "0032-managed-native-app-process-and-capability-boundary.md",
            "0032-els2-type11-signature-profile-boundary.md",
        }
    ),
    # ADR 0033 -- one portable-service-tunnel decision and one Proposal 170
    # (ELS2 consumer lookup) decision. The ledger records that a third,
    # byte-identical 0032 copy of the portable ADR was dropped during the
    # portable-branch merge; it must not reappear.
    "0033": frozenset(
        {
            "0033-portable-service-tunnel-policy-core-and-adapters.md",
            "0033-els2-consumer-lookup-identity-and-install-key.md",
        }
    ),
}

# A four-digit number, a hyphen, then a lowercase slug. Deliberately strict:
# every real file in docs/adr/ matches it today, and an unparseable name is an
# error rather than a silent skip. See ADR_NAME_EXPECTATION below.
ADR_NAME = re.compile(r"^(\d{4})-[a-z0-9][a-z0-9._-]*\.md$")
ADR_NAME_EXPECTATION = "NNNN-<lowercase-slug>.md (four-digit zero-padded ADR number)"


def find_errors(adr_root: Path) -> list[str]:
    """Return one message per violated invariant; empty means the tree is clean.

    Fails closed. Every regular file under ``adr_root`` must be an ADR filename
    this guard can parse -- a name it cannot parse is reported, not skipped,
    because a guard that skips what it cannot see is not a guard.
    """
    if not adr_root.is_dir():
        return [f"ADR root {adr_root.as_posix()} is not a directory"]

    owners: dict[str, list[str]] = defaultdict(list)
    errors: list[str] = []

    for path in sorted(p for p in adr_root.rglob("*") if p.is_file()):
        relative = path.relative_to(adr_root).as_posix()
        match = ADR_NAME.match(path.name)
        if match is None:
            errors.append(
                f"unparseable ADR filename {relative}: expected {ADR_NAME_EXPECTATION}"
            )
            continue
        owners[match.group(1)].append(relative)

    for number, paths in sorted(owners.items()):
        if len(paths) <= 1:
            continue
        allowed = TOLERATED_DUPLICATES.get(number)
        if allowed is not None and set(paths) == set(allowed):
            continue
        rendered = ", ".join(sorted(paths))
        if allowed is None:
            errors.append(
                f"ADR {number} is claimed by {len(paths)} files and is not a recorded "
                f"collision in {LEDGER}: {rendered}"
            )
        else:
            expected = ", ".join(sorted(allowed))
            errors.append(
                f"ADR {number} claims {rendered}, which does not match the recorded "
                f"collision in {LEDGER} ({expected})"
            )

    # A tolerated entry naming a file that is no longer in the tree means the
    # exemption has rotted: someone renamed or removed an ADR and the ledger
    # plus this script now describe a tree that does not exist. Report it so the
    # exemption is retired deliberately rather than decaying into a dead skip.
    for number, allowed in sorted(TOLERATED_DUPLICATES.items()):
        observed = set(owners.get(number, ()))
        missing = sorted(set(allowed) - observed)
        if missing:
            errors.append(
                f"tolerated ADR {number} collision is stale: {', '.join(missing)} "
                f"no longer exists in {adr_root.as_posix()}; retire the entry in "
                f"{LEDGER} and in TOLERATED_DUPLICATES"
            )

    return errors


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--adr-root", type=Path, default=Path("docs/adr"))
    args = parser.parse_args()
    errors = find_errors(args.adr_root)
    if errors:
        print("ADR-number ownership check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("ADR-number ownership check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())