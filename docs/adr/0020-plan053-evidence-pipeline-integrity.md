# ADR 0020: Plan 053 evidence-pipeline integrity boundary

- Status: Accepted for Plan 053
- Date: 2026-07-29
- Decision owners: repository maintainers

## Context

Plan 052 defined run identity, observation-v2, and atomic bundle schemas, but
the canonical rootless and Multipass execution path still emitted legacy
single-direction records. Bundle primitives also allowed hash/serialization
drift, decorative manifest checksums, and acknowledgements inside finalized
bundles.

## Decision

The Plan 053 pipeline uses one measured, clean source identity created before
the first direction. The identity is copied into the bundle staging root,
frozen by its exact on-disk digest, and cross-checked by every direction,
attestation, trigger, observation, and cleanup artifact. The canonical path
passes explicit run ID, identity path, staging path, and
`milestone-3-v2` profile arguments through each trust boundary.

Every primary direction writes all five artifact classes regardless of whether
execution is blocked, rejected, or successful. Missing source-locked reference
receiver observations are typed `not-observed` rejections; they are never
replaced with a pass or `not-applicable` value. Cleanup failures replace any
protocol result with a failed-cleanup result.

Bundle manifests contain only normalized bundle-relative regular files from the
allowlisted layout. JSON is serialized once and hashed over the exact bytes
written. `manifest.sha256` is mandatory and verified before manifest contents
are trusted. Finalization refuses an existing manifest, and export verifies the
staging and copied trees before an atomic rename. The export acknowledgement is
written beside the immutable bundle using a bundle-relative path.

## Consequences

- A blocked local run can produce a complete `diagnostic-complete-not-certificate`
  bundle without being mistaken for interoperability evidence.
- Historical records remain readable through their existing validator but
  cannot satisfy the Plan 053 profile without provenance bindings.
- Raw run state remains outside the bundle and is deleted by the runner.
- Java startup qualification, source-locked reference receiver markers, direct
  reference triggers, and the two-run Milestone 3 certificate remain separate
  external gates.
- NTCP2 remains experimental and non-advertised.

## Rejected alternatives

- Inferring identity or staging paths from the current working directory would
  permit cross-run mixing and is rejected.
- Filling absent provenance or receiver fields with zeroes or success values
  would hide blockers and is rejected.
- Mutating a finalized bundle with an export receipt would invalidate its
  manifest and is rejected.
