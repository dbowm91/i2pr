# `i2pr-app-package-build` — deterministic managed-app package builder

This public leaf crate builds signed `.i2prapp` archives from exact manifest
bytes, a payload directory, and an Ed25519 signing key. It sorts inventory
paths, writes Stored ZIP entries, records executable bits from source file
permissions, rejects symlinks and special files, and enforces the package v1
size bounds. It reopens the artifact and checks its inventory, signature, and
payload digests before returning success.

The builder depends on the app protocol contract and ZIP's no-compression
feature surface only. It has no dependency on the mutable router package
store, manager protocol, trust database, grants, launch catalog, runtime, or
network. A signature proves key possession only; installation always runs the
router's independent canonical verifier before operator policy is consulted.

The crate is semver-managed independently from the router binary. The
deterministic profile is package-format v1; changing that profile or the
signature transcript requires a package-format version change.
