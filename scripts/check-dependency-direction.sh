#!/usr/bin/env bash
set -euo pipefail

# Keep this check dependency-free: cargo metadata is the source of truth for
# the workspace manifests and Python is used only from the standard library.
cargo metadata --no-deps --format-version 1 | python3 -c '
import json
import sys

metadata = json.load(sys.stdin)
packages = {package["name"]: package for package in metadata["packages"]}

expected = {
    "i2pr-proto": set(),
    "i2pr-crypto": {"i2pr-proto"},
    "i2pr-core": set(),
    "i2pr-transport": {"i2pr-core", "i2pr-proto"},
    "i2pr-transport-ntcp2": {
        "i2pr-crypto", "i2pr-proto", "i2pr-transport"
    },
    "i2pr-testkit": {
        "i2pr-core", "i2pr-crypto", "i2pr-proto", "i2pr-runtime",
        "i2pr-transport", "i2pr-transport-ntcp2",
    },
    "i2pr-storage": {"i2pr-crypto"},
    "i2pr-daemon": {
        "i2pr-core",
        "i2pr-proto",
        "i2pr-crypto",
        "i2pr-runtime",
        "i2pr-storage",
        "i2pr-transport",
    },
    "i2pr-runtime": {"i2pr-core", "i2pr-transport"},
}

for name, allowed in expected.items():
    if name not in packages:
        raise SystemExit(f"missing workspace package: {name}")
    direct = {
        dependency["name"]
        for dependency in packages[name]["dependencies"]
        if dependency["name"].startswith("i2pr-")
    }
    unexpected = direct - allowed
    if unexpected:
        raise SystemExit(
            f"{name} has forbidden direct workspace dependencies: {sorted(unexpected)}"
        )

print("dependency direction: ok")
'
