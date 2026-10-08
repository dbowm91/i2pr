# `i2pr-app-package` — signed package verification and local store

Path: `crates/i2pr-app-package/`. Plan 382 owns the `.i2prapp` v1 verifier and
immutable local package store. It proves signed content integrity and publisher
key identity; it is not an administrator-policy or launch-authority owner.

## Purpose

This crate verifies a bounded Stored-only ZIP profile, validates the raw
single-disk central directory (including duplicate names hidden by ZIP's name
index), validates a signed
manifest/inventory, and installs package payload into an immutable local store.
It may use filesystem APIs only beneath a caller-injected store root. It owns no
network client, router state, runtime, process launch, trust decision, grant,
selection, or `LaunchAuthority` construction.

## Module layout

| Module | File | Responsibility | Key public types |
| --- | --- | --- | --- |
| crate root | `src/lib.rs` | Signature transcript, archive verifier, strict inventory, immutable store, materialization, cleanup | `PackageStore`, `PackageIdentity`, `VerifiedPackage`, `InstalledPackage`, `PackageError` |

## Public surface

The crate root exports the package identity and inventory records, read-only
verification (`verify_file`), and `PackageStore::{open, install, list,
verify_installed, remove_unreferenced, cleanup_staging, package_path}`.

## Key contracts

- Archive, file-count, single-payload, total-payload, inventory, and manifest
  ceilings are named constants and checked before materialization.
- Signature verification uses strict Ed25519 over exact metadata bytes.
- Inventory paths are canonical `PackagePath` values, sorted, unique, and
  portable against case-fold and Windows-reserved-name collisions.
- Install copies a mutable source exactly once to staging. The staged copy is
  then the only archive opened.
- Payload bytes are manually streamed to create-new files while hashing; no
  generic extract-all API is used.
- A same-version artifact conflict fails. Identical reinstall re-verifies the
  installed copy and returns it idempotently.
- Receipts are consistency metadata and never replace signature or payload
  verification.
- Package identity grants no trust, capabilities, selection, or launch right.

## Errors

`PackageError` distinguishes I/O, malformed archives/metadata, named limits,
signature failure, version conflict, and missing packages. Diagnostics do not
include package contents or key bytes.

## Dependencies

Workspace dependency: `i2pr-app-proto`. External dependencies: the already
workspace-pinned `ed25519-dalek`, `sha2`, `serde`, `serde_json`, `thiserror`,
and `zip 8.6`. The ZIP dependency already exists for NetDB reseed handling; this
crate accepts only Stored entries and calls no generic extraction API. The
workspace ZIP features remain unchanged because NetDB uses deflate.

## Tests

Unit tests build signed package fixtures and cover valid verify/install,
idempotent reinstall, same-version artifact conflict, installed metadata
tampering, verification of the staged snapshot after source replacement,
signature rejection, traversal/extra-entry rejection, compression rejection,
duplicate ZIP entries, and symlink-safe abandoned-staging cleanup. The package
boundary checker verifies dependency and forbidden capability seams and has
executable negative controls.

## Distinctive design choices

1. The source artifact is copied once before any parsing that can affect the
   installed result.
2. Exact JSON bytes are signature-checked without canonicalization.
3. Stored-only ZIP entries avoid decompressor resource and parser surface.
4. Archive names are mapped manually into `payload/` only after path validation.
5. Archive permission metadata never controls resulting filesystem modes.
6. Each version directory is content-addressed and immutable after commit.
7. A signature authenticates a publisher key, not operator policy.

## Cross-references

- ADR 0036: signed managed-app packages and immutable local store.
- `specs/references/managed-app-package-v1.md`.
- Plan 382 closure: `plans/closure/managed-native-app-runtime/382-status.md`.
- Next policy owner: Plan 383.
