# i2pr-app-package-build

Builds deterministic signed `.i2prapp` archives from exact manifest bytes, a
payload directory, and an Ed25519 signing key. Inventory paths are sorted,
files are stored without compression, special files and symlinks are rejected,
and package size limits follow the router's package v1 verifier.

The builder only constructs artifacts. A valid signature does not establish
publisher trust and grants no launch authority. The router verifies every
artifact again before installation.
