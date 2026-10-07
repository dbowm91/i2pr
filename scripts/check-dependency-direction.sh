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
    # Plan 373: signed package verification is a leaf trust-zone owner. Its
    # cryptographic/parser/filesystem dependencies are external only.
    "i2pr-app-package": {"i2pr-app-proto"},
    # Plan 369 §6: `i2pr-appd` is a separate runtime trust zone. It may reach the
    # managed-app contracts and nothing else. The absent entries are the point:
    # naming this crate is what makes a future `i2pr-appd -> i2pr-daemon` or
    # `i2pr-appd -> i2pr-runtime` edge a hard failure instead of an unreviewed
    # way for the manager to reach router internals.
    "i2pr-appd": {"i2pr-app-manager-proto", "i2pr-app-proto", "i2pr-app-state"},
    # Plan 374 policy evaluation may use the verified package store and app
    # protocol types, but cannot construct appd launch authority.
    "i2pr-app-state": {"i2pr-app-package", "i2pr-app-proto"},
    "i2pr-appctl": {"i2pr-app-package", "i2pr-app-proto", "i2pr-app-state"},
    # Plan 369 §6: `i2pr-apphost` is the *other* side of that trust zone. It is
    # not allowed to depend on `i2pr-appd` either: the two are separate
    # processes, and linking one into the other would make the zone boundary a
    # naming convention instead of a build constraint. Naming it here is what
    # makes a future `i2pr-apphost -> i2pr-appd` edge a hard failure.
    "i2pr-apphost": {"i2pr-app-manager-proto", "i2pr-app-proto"},
    # Plan 369 §G: `i2pr-app-fixture` is evidence tooling, not product code. It is
    # the *third* thing in the app trust zone and the only one that may depend on
    # `i2pr-appd` -- the fixture manager has to run the real manager to qualify it.
    # The allowlist is still the point: naming this crate here is what makes a
    # future `i2pr-appd -> i2pr-app-fixture` edge (a production manager that could
    # launch the fixture) a hard failure rather than an unreviewed one. No router
    # crate may name it at all, which is asserted by the absence of any such entry.
    "i2pr-app-fixture": {
        "i2pr-app-manager-proto",
        "i2pr-app-proto",
        "i2pr-appd",
    },
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

# Fail closed in the other direction too. This loop iterates the *map*, so a
# workspace member with no entry is invisible to it: deleting an entry would
# make that crate the forbidden edges unreported rather than reported, which is
# the same silent pass the map is supposed to prevent. `check-console-boundaries.sh`
# rule 7 asserted the same set, but it belongs here, in the script that owns the
# map -- otherwise a reader who runs only this check gets a false all-clear.
# Plan 369 WP5 negative mutation N11 caught exactly this.
unmapped = sorted(
    name
    for name in packages
    if name.startswith("i2pr-") and name not in expected
)
if unmapped:
    raise SystemExit(
        f"workspace members absent from the expected map: {unmapped}; add an "
        "explicit allowlist entry for each so its edges are actually checked"
    )

print("dependency direction: ok")
'
