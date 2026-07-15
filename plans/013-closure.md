# Milestone 1 Plan C closure: identity, cryptographic wrappers, and storage

## Scope and outcome

This record closes `plans/013-m1-identity-crypto-storage.md`. The bounded
implementation adds one concrete generated router identity profile, local
RouterInfo signing/verification, a versioned private identity file, and
explicit non-networked CLI lifecycle commands. It does not add transport,
NetDB, reseed, tunnel, client, capability-advertisement, or live daemon
behavior.

## Selected algorithms and dependencies

- I2P signature type 7: EdDSA over Ed25519.
- I2P router encryption type 4: X25519, with little-endian protocol bytes.
- Type-5 key certificate binds the selected public-key types.
- `ed25519-dalek` 2.2 with `std`/`zeroize`, default features disabled.
- `x25519-dalek` 2.0.1 with `static_secrets`/`zeroize`, default features disabled.
- `zeroize` 1.8 range (lock 1.9.0) with derive support, `subtle` 2.6 without default features,
  `rand_core` 0.9 with the `os_rng` feature at the crypto boundary, and the
  existing workspace `sha2` 0.10 dependency.

The dependency purpose, feature set, MSRV review, unsafe exposure, maintenance
posture, and alternatives are recorded in ADR 0005. No local cryptographic
primitive, private fixture, or copied router implementation code was added.

## Changed files

- `Cargo.toml`, `Cargo.lock` — six-crate workspace and reviewed dependencies.
- `crates/i2pr-crypto/` — zeroizing private wrappers, injected RNG generation,
  public-key derivation, SHA-256/constant-time helpers, Ed25519 signing and
  strict verification, RouterInfo signing path, and mutation tests.
- `crates/i2pr-storage/` — explicit 184-byte version-1 format, bounded strict
  parser, SHA-256 integrity, Unix permission checks, symlink rejection,
  atomic create-only installation, reload, corruption, boundary, and
  concurrency tests.
- `crates/i2pr-daemon/` — `identity generate` and `identity inspect`, typed
  identity error/exit categories, and no-secret output tests.
- `scripts/check-dependency-direction.sh` — six-crate dependency allowlist.
- `README.md`, `AGENTS.md`, `CONTRIBUTING.md` — current boundaries, lifecycle,
  secret handling, and quality expectations.
- `docs/architecture.md`, `docs/security-model.md` — crate graph, crypto and
  storage boundaries, permissions, backup responsibilities, and non-claims.
- `docs/adr/0004-router-identity-algorithms.md` through
  `docs/adr/0007-explicit-identity-first-run.md` — algorithm, dependency,
  storage, and first-run decisions.
- `docs/protocol-support.md`, `specs/support.toml`,
  `specs/protocols/01-common-identity-crypto.md`, `specs/SOURCES.md` — local
  experimental evidence without an interoperability or capability claim.

## Storage format and security behavior

The private file is exactly 184 bytes: a 24-byte magic/version/algorithm/
length header, 128 bytes of signing/encryption private and derived public
material, and a 32-byte SHA-256 integrity value. All widths and lengths are
explicit and version 2 has no implicit migration. Files are bounded to 4 KiB,
fully consumed, rederived on load, and rejected on truncation, trailing data,
unsupported version/algorithm, checksum mismatch, or public/private mismatch.

On Unix, identity directories must have no group/world permission bits and
generated files use mode 0600. Symlinks and non-regular files are rejected.
Writes use a same-directory temporary file, write/sync, atomic no-replace
hard-link installation, temporary cleanup, and directory sync. A normal rename
would permit a concurrent create-only writer to replace the first identity, so
the no-replace hard-link install is an intentional safety deviation from the
plan's portable rename wording. Non-Unix directory-sync behavior is documented
as a platform limitation. Storage is integrity-protected but not encrypted;
passphrase-backed at-rest encryption remains out of scope.

## CLI and policy

Identity creation is explicit. `check-config` and `run --dry-run` do not create
directories or identity files. `identity generate` creates the configured data
directory if needed, refuses an existing path, and uses injected OS randomness.
`identity inspect` loads and revalidates the file and reports only public
algorithm identifiers. Corrupt state is never silently regenerated. No command
opens a listener, publishes RouterInfo, performs reseeding, or advertises a
capability.

## Quality-command results

| Command | Result |
| --- | --- |
| `rtk cargo fmt --all --check` | passed |
| `rtk cargo check --workspace` | passed during implementation |
| `rtk cargo test --workspace` | passed — 66 tests |
| `rtk cargo clippy --workspace --all-targets --all-features -- -D warnings` | passed |
| `RUSTDOCFLAGS="-D warnings" rtk cargo doc --workspace --no-deps` | passed |
| `rtk bash scripts/check-dependency-direction.sh` | passed |
| `rtk cargo deny check advisories bans sources` | passed; duplicate `rand_core` 0.6/0.9 warning is required by `x25519-dalek` and the workspace RNG stack |
| `rtk rustup run 1.85.0 cargo check --workspace --all-targets` | passed |

## CI evidence and known limitations

The pushed implementation commit `7469e74` passed [GitHub Actions CI run
29389400514](https://github.com/dbowm91/i2pr/actions/runs/29389400514): Ubuntu
MSRV, dependency policy, macOS quality, and Ubuntu quality all passed. GitHub
reported only the existing non-blocking `actions/checkout@v4` Node.js 20
deprecation annotations.

The support ledger remains `experimental` and `advertised = false`: no
independent router vectors or mixed-router interoperability tests have been
added. The selected identity profile does not generate legacy or hybrid/PQ
identities. Storage integrity does not defend against an attacker with write
access to the parent directory, and non-Unix durability/permission behavior
needs platform-specific follow-up before a production claim.

No private test keys are committed. Deterministic tests derive ephemeral keys
from seeded test RNGs in memory and do not write private fixture files.
