# ADR 0036: signed managed-app packages and immutable local store

Status: **Accepted** (Plan 373)

## Context

Plan 369 intentionally shipped an empty launch catalog. Managed applications
need an offline package format whose publisher identity and contents can be
verified before a later administrator-policy layer decides whether anything
may run. Package verification must not become an implicit trust or permission
decision.

## Decision

1. A package identity is `(publisher_key_id, app_id, app_version,
   artifact_digest)`.
2. `publisher_key_id` and the signed manifest's `PublisherId` are the lowercase
   hexadecimal SHA-256 fingerprint of the exact 32-byte Ed25519 public key.
3. Ed25519 signatures prove possession of that key. They do not prove trust,
   permission, selection, autostart, or launch authority.
4. The signature covers the exact manifest and inventory bytes plus the public
   key using the domain-separated transcript in
   `specs/references/managed-app-package-v1.md`. JSON is not reserialized before
   verification.
5. Package files are immutable content. Application data is stored separately
   from package versions.
6. Verification, installation, update, and removal execute no package code or
   hook. Package installation has no network path.
7. Local package identity is independent from future TUF, Sigstore, or repository
   provenance. Repository freshness and rollback protection belong to a future
   repository/update plan.
8. Administrator trust and grants bind to publisher-key identity and `AppId`,
   never a display name or version alone. Plan 374 owns those decisions.
9. Downgrade and version selection are explicit administrator operations, not
   consequences of package installation or version ordering.

## Consequences

The package crate may verify and store an artifact but cannot construct
`LaunchAuthority`, trust a publisher, select a version, or grant capabilities.
The v1 ZIP profile is Stored-only and manually materialized. Compression,
remote repositories, and transparency integration require later owners.

## References

- Plan 373: signed immutable package and local store foundation.
- `specs/references/managed-app-package-v1.md`.
- ADR 0032: managed native-app process and capability boundary.
- ADR 0035: private manager protocol and inherited authority.
