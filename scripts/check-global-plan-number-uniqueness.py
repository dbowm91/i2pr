#!/usr/bin/env python3
"""Guard global implementation-plan number ownership.

Closure records and supporting documents may repeat a number within the
owning subsystem. Cross-subsystem implementation ownership is forbidden,
except for the two exact historical collisions recorded below.
"""

from __future__ import annotations

import argparse
import re
import sys
from collections import defaultdict
from pathlib import Path


HISTORICAL_COLLISIONS = {
    "349": {
        "implementation/managed-native-app-runtime/349-managed-app-v1-direction-broker-network-policy-corrective.md",
        "implementation/portable-service-tunnels/349-portable-service-tunnel-boundary-and-ownership-contract.md",
    },
    "296": {
        "implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md",
        "implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md",
    },
    "297": {
        "implementation/i2pcontrol-proposal-170/297-local-tls-identity-for-use-ssl.md",
        "implementation/anonymity/297-http-anonymity-profile-convergence-and-differential-qualification.md",
    },
}
PLAN_NAME = re.compile(r"^(\d{3})-[^/]+\.md$")


def find_collisions(plans_root: Path) -> list[str]:
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
        allowed = HISTORICAL_COLLISIONS.get(number)
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
    errors = find_collisions(args.plans_root)
    if errors:
        print("Global plan-number ownership check failed:", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1
    print("Global plan-number ownership check passed.")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
