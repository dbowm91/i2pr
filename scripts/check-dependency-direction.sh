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
    "i2pr-app-manager-proto": {"i2pr-app-proto"},
    "i2pr-app-proto": set(),
    # Plan 369 §6: `i2pr-appd` is a separate runtime trust zone. It may reach the
    # managed-app contracts and nothing else. The absent entries are the point:
    # naming this crate is what makes a future `i2pr-appd -> i2pr-daemon` or
    # `i2pr-appd -> i2pr-runtime` edge a hard failure instead of an unreviewed
    # way for the manager to reach router internals.
    "i2pr-appd": {"i2pr-app-manager-proto", "i2pr-app-proto"},
    "i2pr-proto": set(),
    "i2pr-crypto": {"i2pr-proto"},
    "i2pr-core": set(),
    "i2pr-transport": {"i2pr-core", "i2pr-proto"},
    "i2pr-transport-ntcp2": {
        "i2pr-crypto", "i2pr-proto", "i2pr-transport"
    },
    "i2pr-transport-ssu2": {
        "i2pr-crypto", "i2pr-proto", "i2pr-transport"
    },
    "i2pr-testkit": {
        "i2pr-core", "i2pr-crypto", "i2pr-proto", "i2pr-runtime",
        "i2pr-transport", "i2pr-transport-ntcp2",
    },
    "i2pr-storage": {"i2pr-crypto"},
    "i2pr-su3": set(),
    "i2pr-netdb": {"i2pr-crypto", "i2pr-proto", "i2pr-su3"},
    "i2pr-netdb-persist": {
        "i2pr-crypto", "i2pr-netdb", "i2pr-proto", "i2pr-storage"
    },
    "i2pr-daemon": {
        "i2pr-addressbook",
        "i2pr-app-manager-proto",
        "i2pr-app-proto",
        "i2pr-api",
        "i2pr-client",
        "i2pr-console",
        "i2pr-core",
        "i2pr-proto",
        "i2pr-crypto",
        "i2pr-i2pcontrol",
        "i2pr-netdb",
        "i2pr-netdb-persist",
        "i2pr-runtime",
        "i2pr-service-tunnels",
        "i2pr-storage",
        "i2pr-su3",
        "i2pr-transport",
        "i2pr-tunnel",
    },
    "i2pr-runtime": {
        "i2pr-core", "i2pr-crypto", "i2pr-proto", "i2pr-transport", "i2pr-transport-ntcp2",
        "i2pr-transport-ssu2",
    },
    "i2pr-client": {
        "i2pr-core", "i2pr-crypto", "i2pr-netdb", "i2pr-proto", "i2pr-tunnel"
    },
    "i2pr-service-tunnels": {
        "i2pr-client", "i2pr-proto"
    },
    "i2pr-api": {
        "i2pr-client", "i2pr-crypto", "i2pr-proto", "i2pr-tunnel"
    },
    "i2pr-i2pcontrol": set(),
    "i2pr-addressbook": {"i2pr-proto"},
    # Plan 356: the browser console owns only its own application surface.
    # It has no workspace dependencies at all, which keeps it from becoming
    # a back door into the router-owner crates.
    "i2pr-console": set(),
    # Closed coverage gap (recorded in AGENTS.md): these two members were
    # absent from this map, so a new forbidden edge in either would have
    # passed CI silently. Their allowlists mirror their manifests.
    "i2pr-tunnel": {"i2pr-core", "i2pr-crypto", "i2pr-netdb", "i2pr-proto"},
    "i2pr-interop": {
        "i2pr-crypto",
        "i2pr-proto",
        "i2pr-runtime",
        "i2pr-storage",
        "i2pr-transport",
        "i2pr-transport-ntcp2",
    },
}

for name, allowed in expected.items():
    if name not in packages:
        raise SystemExit(f"missing workspace package: {name}")
    direct = {
        dependency["name"]
        for dependency in packages[name]["dependencies"]
        if dependency["kind"] in (None, "normal")
        if dependency["name"].startswith("i2pr-")
    }
    unexpected = direct - allowed
    if unexpected:
        raise SystemExit(
            f"{name} has forbidden direct workspace dependencies: {sorted(unexpected)}"
        )

print("dependency direction: ok")
'
