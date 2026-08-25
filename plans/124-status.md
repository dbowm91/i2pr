# Plan 124 status — primary composition fix retained; full closure reopened

## Current authority

- Status: **`primary-composition-fix-retained-full-closure-reopened-plan127`**.
- Milestone 6 authority: `126-129-milestone6-final-corrective-roadmap.md`.
- Final destination-layer closure: `127-m6-destination-session-routing-final-closure.md`.

## What remains valid

Plan 124 corrected a real and important defect:

```text
before: outbound tunnel received plaintext inner I2NP Data
now:    outbound tunnel receives standard-encoded I2NP Garlic carrying encrypted destination bytes
```

The byte-identity regression proving `garlic_i2np_bytes` crosses the outbound tunnel and is not equal to `inner_envelope_bytes` remains authoritative and must not regress.

The successful local A -> B New Session-shaped trajectory also remains useful composition evidence for the tunnel boundary.

## Why the full Plan 124 label is reopened

The original Plan 124 acceptance required a complete reverse/session lifecycle:

```text
A -> B New Session
B -> A New Session Reply
A -> B Existing Session
B -> A Existing Session
```

The current suite explicitly does not complete the real NSR session handshake, and the lower ECIES/session layer now requires Plan 126 correction before those requirements can be satisfied honestly.

## Current classification

```text
plan_124_primary_plaintext_fix = passed-retained
plan_124_full_closure = reopened-plan127
milestone6_local_product = not-closed
next = Plan 126, then Plan 127
```

The only permitted network omission remains `authenticated-router-link-bypassed-local-seam` after real OBEP processing.