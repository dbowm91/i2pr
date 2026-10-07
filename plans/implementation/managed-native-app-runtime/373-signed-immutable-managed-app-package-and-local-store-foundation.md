# Plan 373 — signed immutable managed-app package and local store foundation

Status: **passed-managed-app-signed-package-store-foundation**.

Classification: **invariant + infrastructure**.

Roadmap:
- `plans/subsystems/managed-native-app-runtime-roadmap.md`

Hard dependencies:
- Managed native app runtime/369 is closed: the trusted `i2pr-appd` / `i2pr-apphost` runtime exists.
- Managed native app runtime/370 is closed: application `hello` identity is decodable and canonical.
- Managed native app runtime/371 is closed: application-runtime startup may degrade without taking down the router.

Interface dependencies:
- ADR 0032 (managed app process/capability boundary).
- ADR 0035 (private manager protocol and inherited authority).
- `i2pr-app-proto::Manifest`, `AppId`, `PublisherId`, `AppVersion`, `PackagePath`.
- `i2pr-appd::authority::LaunchAuthority` remains sealed; this plan does not create its production caller.

Successor:
- Plan 374 consumes the verified immutable store to provide persistent administrator policy, a production launch catalog, restart-safe selection/autostart, and the offline administrative CLI.

## Objective

Define and implement the first locally installable managed-application package format and immutable package store.

A package accepted by Plan 373 must have:

1. a bounded single-file `.i2prapp` container;
2. an exact Plan-345/370 manifest;
3. a complete signed file inventory;
4. an Ed25519 publisher key whose fingerprint is the package's `PublisherId`;
5. a strict publisher signature over the package metadata;
6. every payload byte verified against the signed inventory before commit;
7. no install script, hook, symlink, device, hardlink, archive permission, remote fetch, or executable side effect;
8. an atomic, immutable-on-success local store entry keyed by publisher/app/version/artifact digest.

This plan proves package **integrity and publisher-key identity**, not operator trust or permission grants. A correctly signed package is not automatically trusted, selected, or runnable.

## Why this plan is ready

Plan 369 deliberately ships `EmptyCatalog`: the production manager has no package store, no signature verifier, no publisher trust owner, and no grant persistence. That is now the only reason the shipped manager cannot obtain a production launch authority.

The package boundary can be built independently of live launch policy. Keeping it separate prevents archive parsing, signature verification, filesystem transactions, and administrator grant semantics from becoming one oversized security milestone.

## Research findings that shape the plan

### Package verification is local; repository update security is later

The Update Framework (TUF) is the appropriate reference for a future remote repository/update layer because it addresses rollback, freeze, mix-and-match, malicious mirror, and key-compromise classes. Those are repository metadata problems, not prerequisites for verifying a local package supplied by an operator.

Plan 373 therefore has **no network client and no TUF implementation**. It produces a local artifact/store model that a later TUF-backed repository can safely target without changing package identity.

Sigstore bundles are also not the sole v1 trust root: public identity/transparency infrastructure is useful for public release provenance, but i2pr must be able to verify a package offline and over I2P-only distribution. A future package may carry additional provenance, but the local publisher signature remains independently verifiable.

### Use the existing crypto substrate

The workspace already pins `ed25519-dalek = 2.2` and `sha2 = 0.10`. Do not upgrade the crypto stack merely for this plan. Use strict Ed25519 verification and SHA-256 fingerprints/digests through the existing dependency authority.

### Use a deliberately small ZIP profile

A standard single-file archive is useful for handoff/distribution, but generic extraction is too permissive. The current `zip` crate supports Stored entries without compression features and its path API warns that raw archive names are unsafe.

Add `zip 8.6.x` with `default-features = false`, subject to the normal dependency/license/advisory review. Its MSRV is below i2pr's Rust 1.89 floor. The package verifier **must not call a generic extract-all API**.

The v1 package profile permits Stored regular-file entries only. No compression is a feature here: it removes decompression bombs and codec-specific parser surface from the first trust milestone. Compression can be added later without changing package identity because payload hashes are over uncompressed bytes.

## ADR decision required

Add ADR 0036 (next free ADR at implementation start) freezing:

- package identity = `(publisher_key_id, app_id, app_version, artifact_digest)`;
- `PublisherId` in a signed package is the lowercase SHA-256 fingerprint of the 32-byte Ed25519 public key;
- signatures prove publisher-key possession, **not trust or permissions**;
- administrator grants bind to publisher-key identity + `AppId`, never display name or version alone;
- package files are immutable content; application data lives outside package versions;
- no package executes code during verify/install/update/uninstall;
- local package trust is separate from future TUF/Sigstore/repository provenance;
- downgrade/selection semantics belong to Plan 374, not the package parser.

If ADR 0036 is not still free, use the next free ADR number and update this plan during implementation without renumbering Plan 373.

## Package format v1

File extension: `.i2prapp`.

Container: single ZIP file, v1 profile below.

### Required metadata entries

Exactly one of each:

```text
manifest.json
inventory.json
publisher.ed25519
signature.ed25519
```

Payload entries are:

```text
payload/<PackagePath>
```

No other top-level namespace is accepted in v1.

### ZIP profile

Required:
- single-disk archive;
- Stored/no-compression entries only;
- no encryption;
- no directory entries;
- no duplicate names;
- no absolute names;
- no NUL;
- no `.` / `..` components;
- no backslash;
- every payload suffix must round-trip through `PackagePath`;
- Unix symlink/device/FIFO/socket type bits, when present, are rejected rather than interpreted;
- archive mode bits do not become filesystem authority;
- UTF-8/ASCII package paths only through the existing `PackagePath` grammar;
- portable collision check rejects two payload paths equal under ASCII case-folding;
- Windows-reserved path components (`CON`, `PRN`, `AUX`, `NUL`, `COM1..9`, `LPT1..9`, case-insensitive) are rejected on every OS so one signed package has one portable meaning.

Named ceilings, frozen in the package crate:
- archive bytes: 512 MiB;
- payload files: 4,096;
- one payload file: 256 MiB;
- total uncompressed payload: 512 MiB;
- inventory bytes: 1 MiB;
- metadata entry bytes use the existing manifest ceiling plus exact key/signature sizes;
- path ceiling remains `PackagePath`'s 512-byte limit.

All arithmetic is checked before allocation/write.

### Inventory

`inventory.json` is a strict JSON array of records:

```text
{
  "path": "<PackagePath without payload/ prefix>",
  "size": <u64>,
  "sha256": "<64 lowercase hex chars>",
  "executable": <bool>
}
```

Rules:
- deny unknown fields;
- duplicate object fields fail;
- records are strictly sorted by path bytes;
- no duplicate or case-fold-colliding paths;
- every payload entry has exactly one record;
- every inventory record has exactly one payload entry;
- entrypoint paths from the manifest exist in inventory and are marked executable;
- UI resources, if present, exist in inventory;
- executable is the only permission bit package metadata may request;
- no executable bit can imply setuid/setgid/sticky or platform ACLs.

### Publisher identity

`publisher.ed25519` is exactly 32 raw public-key bytes.

`publisher_key_id = lowercase_hex(SHA256(publisher.ed25519))`.

The manifest's `publisher_id` must exactly equal that 64-character fingerprint. A free-form publisher label is display metadata for a later layer; it is not security identity.

### Signature transcript

`signature.ed25519` is exactly 64 raw bytes.

Verify Ed25519 strictly over:

```text
"I2PR-APP-PACKAGE-V1\0"
|| u32be(manifest_len) || exact manifest.json bytes
|| u32be(inventory_len) || exact inventory.json bytes
|| publisher.ed25519
```

The signature binds exact metadata bytes and the public key. Payload integrity is transitively bound through the signed inventory.

Do not canonicalize/re-serialize JSON before verification. Decode and validate the same exact bytes that were signed.

## New package/store owner

Add a workspace crate, expected name:

```text
i2pr-app-package
```

It is part of the application-runtime trust zone, not router core.

Allowed dependencies include:
- `i2pr-app-proto`;
- existing `ed25519-dalek`;
- existing `sha2`;
- `serde` / `serde_json`;
- reviewed `zip` with default features disabled;
- standard filesystem APIs.

It must not depend on:
- `i2pr-daemon`;
- `i2pr-runtime`;
- `i2pr-api`;
- `i2pr-client`;
- `i2pr-i2pcontrol`;
- transport/tunnel/NetDB crates;
- network clients.

## Store layout

Under an injected application-store root:

```text
packages/
  <publisher_key_id>/
    <app_id>/
      <version>/
        <artifact_sha256>/
          manifest.json
          inventory.json
          publisher.ed25519
          signature.ed25519
          payload/...
          receipt.json
.staging/
admin.lock
```

`receipt.json` is manager-generated non-authority metadata containing bounded values such as artifact digest, verified publisher fingerprint, app id/version and install generation. It never substitutes for re-verification.

Two different artifacts claiming the same publisher/app/version are a conflict, not an overwrite. Reinstalling the identical artifact is idempotent.

## Installation transaction

The source package is untrusted and may be concurrently mutable.

Required sequence:

1. acquire the package-admin transaction lock;
2. reject source metadata/size before expensive work when possible;
3. copy the source package once into a fresh manager-owned staging file under the store root while computing the artifact SHA-256;
4. flush/sync the staged file as supported by the platform;
5. open and parse **that staged copy**, never the source again;
6. validate ZIP structure and metadata ceilings;
7. verify publisher key/fingerprint and metadata signature;
8. manually materialize payload entries into a fresh private staging directory while hashing/counting bytes;
9. prove exact inventory equality and manifest entrypoint/UI references;
10. write the verification receipt;
11. flush files and directories where the platform supports durable directory sync;
12. atomically rename the completed staging directory to its immutable package identity path;
13. release the lock.

A crash before rename leaves only discardable staging. A crash after rename leaves a complete verified package.

Never unpack into an existing package directory.

## File materialization

- Parents are created by the store itself inside a newly-created private staging root.
- Files use create-new semantics.
- No archive path is ever passed directly to a generic extraction API.
- Archive timestamps/uid/gid/ACL/xattrs are ignored.
- Final files are owner-readable; writable permission is removed from package payload where supported.
- Only inventory entries marked executable receive an executable bit on Unix.
- On Windows, the executable property is semantic and the normal file is created; execution still depends on the selected entrypoint.
- Package mutation by the same OS administrator remains outside the isolation threat model, but Plan 374 re-verifies selected packages before authority construction so accidental/local tamper fails closed.

## Invariants

1. A signature never implies publisher trust, grant, selection, autostart, launch profile, or network authority.
2. `PublisherId` cannot be chosen independently of the signing key for signed packages.
3. Package identity does not depend on router identity or I2P Destination identity.
4. No install-time code executes.
5. No package path can escape the private staging/store root.
6. Payload set and bytes exactly match the signed inventory.
7. No symlink/hardlink/device/archive permission is materialized.
8. Same version + different artifact is never silently replaced.
9. No network access or remote repository logic exists.
10. No `LaunchAuthority` is created by this plan.
11. No managed-app capability/support claim changes.
12. Existing Plan-369 process boundaries remain intact.

## Scope

### In scope

- ADR 0036 or next free equivalent;
- `.i2prapp` v1 package specification;
- `i2pr-app-package` crate;
- exact publisher fingerprint/signature verifier;
- strict inventory parser;
- Stored-only ZIP reader profile;
- immutable local package store;
- atomic staging/commit/recovery cleanup;
- verify/install/list/inspect/remove-unreferenced library operations;
- package fixture builder/test vectors;
- dependency/runtime/static guards;
- fuzz/property tests for package metadata/path/archive parsing.

### Out of scope

- administrator publisher trust state;
- grants;
- selected version;
- rollback authorization;
- autostart/restart policy;
- production `LaunchCatalog`;
- user-facing admin CLI;
- live app launch/stop;
- app data deletion;
- OS sandbox;
- brokered clearnet;
- Proposal 170;
- remote repositories/downloads;
- TUF/Sigstore integration;
- compression.

## Work packages

### WP1 — ADR and package-v1 freeze
Freeze identity, transcript, archive profile, limits, and store transaction before filesystem implementation.

### WP2 — package contract and verification
Implement metadata parsing, publisher fingerprint, strict Ed25519 verification, inventory equality, portable path rules, and golden vectors.

### WP3 — immutable store
Implement staged copy, bounded manual materialization, receipt, atomic commit, idempotency/conflict handling, and abandoned-staging cleanup.

### WP4 — adversarial package corpus
Exercise traversal, aliases, duplicates, case collisions, Windows names, symlink mode, huge sizes/counts, malformed central directory, signature/key substitution, manifest/inventory mismatch, extra/missing payload, and crash cut points.

### WP5 — guards/docs/closure
Register package trust-zone dependency rules, documentation and full-floor evidence.

## Failure, cancellation, restart and contention semantics

- Invalid signature/key/fingerprint: no final store entry.
- Invalid archive/path/inventory: no final store entry.
- Existing identical package: idempotent success.
- Same publisher/app/version with different artifact: typed conflict.
- Out of space/write failure: staging remains non-authoritative and is cleanable.
- Crash before commit rename: final path absent.
- Crash after commit rename: package complete.
- Two installers: serialized by `std::fs::File::lock` / `try_lock` (Rust 1.89 baseline); no additional locking dependency.
- Reader/list operation encountering staging ignores it.
- Cleanup never follows links and never deletes outside the exact staging namespace.
- No retry loop.

## Compatibility and migration

Managed apps remain unreleased.

No existing package format exists, so `.i2prapp` v1 has no migration obligation.

The current `Manifest` schema remains v1; this plan constrains how a signed package binds it but does not reinterpret requested capabilities as grants.

No router config/support/advertisement change.

## Required tests

### Cryptographic identity
- official/independent Ed25519 verification vectors plus package golden;
- wrong key/signature/message all reject;
- changed manifest byte rejects;
- changed inventory byte rejects;
- manifest publisher id not matching key fingerprint rejects;
- key/substitution attempt rejects.

### Archive/path
- traversal/absolute/backslash/NUL rejected;
- duplicate ZIP name rejected;
- case-fold collision rejected;
- Windows reserved component rejected on every host;
- symlink/device/directory entry rejected;
- compression/encryption rejected;
- max exact accepted, max+1 rejected;
- checked total-size overflow rejected before allocation.

### Inventory
- sorted exact list accepted;
- unsorted/duplicate record rejected;
- missing/extra payload rejected;
- size/hash mismatch rejected;
- manifest entrypoint absent/not executable rejected;
- UI path absent rejected.

### Store transaction
- staged source is the only bytes verified/materialized;
- source mutation after staged copy cannot change install result;
- identical reinstall idempotent;
- same version/different artifact conflict;
- simulated failure at every pre-rename phase leaves no authoritative install;
- completed rename yields fully re-verifiable store entry;
- abandoned staging cleanup cannot escape staging root;
- two writer attempts serialize/fail predictably.

### Structural
- no network dependency;
- no router-core dependency;
- no process execution/shell/hooks;
- no `LaunchAuthority` construction.

## Exact verification commands

Focused commands expected after implementation:

```text
cargo fmt --all --check
cargo check --locked -p i2pr-app-package --all-targets
cargo test --locked -p i2pr-app-package --all-targets -- --test-threads=1
cargo clippy --locked -p i2pr-app-package --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked -p i2pr-app-package --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
python3 scripts/check-managed-app-package-boundary.py
python3 scripts/check-managed-app-package-boundary.py --self-test
python3 scripts/check-global-plan-number-uniqueness.py
python3 scripts/check-adr-number-uniqueness.py
python3 -m unittest discover -s tests/planning -p 'test_*.py'
cargo deny check advisories bans sources
```

The closure must also run the complete current `AGENTS.md` routine floor.

## Documentation updates

- ADR 0036 (or next free);
- `specs/references/managed-app-package-v1.md`;
- new package/store architecture deep dive;
- dependency graph;
- security model;
- tooling/guard docs;
- managed-app roadmap;
- registry;
- closure.

## Acceptance criteria

Plan 373 passes only when:

1. one exact bounded `.i2prapp` v1 profile is frozen;
2. publisher identity is cryptographically derived from the Ed25519 public key;
3. exact manifest/inventory bytes are signature-bound;
4. every payload byte/set is inventory-bound;
5. unsafe archive/path/file-type forms fail before authoritative commit;
6. install uses one private staged copy and never re-reads mutable source;
7. commit is atomic and immutable/version-conflict semantics are proven;
8. package parsing/extraction owns no network/process/router capability;
9. package signature grants no trust/capability/launch authority;
10. dependency and negative guards are load-bearing;
11. focused and full routine floors pass.

## Stop conditions

Stop and register a corrective/successor if:
- secure materialization requires a generic extract-all path;
- package identity cannot be made portable across supported platforms;
- a new crypto primitive/provider is needed rather than existing Ed25519/SHA-256;
- local verification requires network/transparency access;
- package install requires live manager/admin IPC;
- filesystem atomicity cannot be made crash-safe enough to distinguish committed from staging on a supported platform.

## Closure evidence required

- ADR/spec freeze;
- dependency review for `zip` including features/MSRV/license/advisories/transitives;
- signature transcript/golden vectors;
- adversarial archive matrix;
- install crash-cut matrix;
- exact source-mutation staging proof;
- conflict/idempotency evidence;
- boundary-checker mutation evidence;
- full routine floor;
- support/config diff;
- unblock audit for Plan 374.

## Handoff notes

Do not add a production launch catalog in Plan 373. A verified package is merely cryptographically attributable content.

Plan 374 is the authority milestone: it decides which publisher keys the operator trusts, which capabilities are granted, which exact installed version is selected, and whether any application becomes launchable.
