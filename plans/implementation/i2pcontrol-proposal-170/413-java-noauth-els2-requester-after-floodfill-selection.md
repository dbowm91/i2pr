# Plan 413 — Java no-auth ELS2 requester after truthful F selection

Status: **blocked-on-plan437-java-floodfill-selection**.

Subsystem: Proposal 170 / Java ELS2 qualification.

## Authority and objective

Successor to blocked Plan 412. Execute the first stock Java I2P 2.13.0
no-auth requester direction: publish an i2pr type-5 service to controlled
floodfill F, then have Java look up its B33 through F, decrypt it, and carry an
integrity-checked application payload. Plan 412's source review and reusable
SAM STREAM helper are retained; it did not execute a live ELS2 attempt.

The hard dependency is Plan 437's exact-pin Java candidate-selection proof.
Plan 306 records that Java parses the controlled `fR` RouterInfo but does not
select it with an `Unknown` bandwidth tier. Do not infer a query from seeding
F into JC's NetDB. Plans 433 and 436 must close before Plan 437 can satisfy
this prerequisite.

## Invariants and scope

- Java remains byte-for-byte stock at
  `9134f808337b401e8e53c73734c81fab04280c9d`; all endpoints bind loopback.
- The SAM client uses STREAM; if a DATAGRAM helper is later required, pass an
  explicit available loopback `sam.udp.host`/`sam.udp.port` as Plan 411 proved.
- No decoded-record injection, shared in-process NetDB, reference patch,
  public networking, or capability advertisement is allowed.
- Scope is only the NONE Java requester direction. Publisher direction,
  auth/negative matrices, lifecycle rows, and whole-Proposal conformance remain
  under Plan 375 and later named successors.

## Execution gate

Do not register this as active until Plan 437 passes and the exact Java
candidate-selection artifact establishes F as JC's queried floodfill. Then
register/execute a bounded one-attempt lane based on Plan 412's implementation
narrative and `tests/integration/els2/clients/java_sam_stream_b33.py`.
Require the i2pr publication/store acknowledgement, a real Java B33 lookup
and payload through JD1/JD2, sanitized evidence, and a negative-tested
evidence checker. A missing F lookup is a typed stop, never a skip.

Closure must retain the Plan 411 diagnosis, list exact command outcomes and
artifact hashes, verify raw/private data exclusion and process cleanup, and
leave Plans 375, 377, and 378 gated on their remaining matrices.
