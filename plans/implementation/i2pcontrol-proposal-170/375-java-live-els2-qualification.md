# Plan 375 — Stock Java I2P bidirectional Encrypted LeaseSet2 qualification

Status: **registered-java-live-els2-qualification-blocked-on-plan373**

Classification: external interoperability + capability evidence.

Hard dependencies:
- Plan 373 passed.
- Proposal 170/346 passed.
- Proposal 170/350 passed.
- Proposal 170/351 passed.

Historical source: Proposal 170/347 stopped before either Java direction executed.

## Objective

Close the two Java I2P rows of the original ELS2 matrix using stock Java I2P without fabricating a
bandwidth tier and without requiring Java to select i2pr as a tunnel peer.

## Reference freeze

Start from the Plan-347 Java baseline:

`i2p/i2p.i2p@9134f808337b401e8e53c73734c81fab04280c9d` (2.13.0).

Before execution:
- build/use an unmodified reference artifact;
- record exact router/version/JVM;
- source-freeze the encrypted LeaseSet publisher/consumer and client-auth configuration surfaces;
- record current i2pr SHA/Cargo.lock;
- keep ADR 0030's prohibition on invented bandwidth-tier letters unchanged.

## Topology: queried floodfill, not tunnel peer

Reuse the proven Plans 303/306 topology pattern:

- **R** — i2pr service/client router under qualification;
- **F** — controlled i2pr floodfill, separate from R;
- **JC** — stock Java publisher/requester;
- **JD1/JD2** — stock Java non-floodfill relay peers for JC's client tunnels.

JC's floodfill view must resolve to F while JD1/JD2 remain the peers used to construct Java's own
tunnels.

This plan must assert that:
- Java reaches i2pr as a **queried floodfill**;
- no row depends on Java choosing i2pr as OBEP/IBGW/participant;
- no bandwidth-tier letter is added;
- the existing caps-arity / reachable-peer findings remain unrelated to this NetDB query path.

## Java-side driver

Build a bounded test driver using stock Java configuration/public APIs only.

It must:
- create an encrypted Java service and obtain the extended B32/B33 address;
- create a Java client to an externally supplied B33 address;
- configure overlapping lookup-secret / PSK / DH authorization through stock surfaces;
- expose a loopback application endpoint;
- export sanitized status sufficient to prove store, lookup, and connection outcome.

Permitted mechanisms include stock I2PTunnel/I2CP/public Java APIs and generated configuration.
Do not patch Java source or copy Java implementation code into i2pr.

## Mandatory directions

### A. i2pr publisher/service -> Java consumer/client

Prove:
1. R publishes its type-5 record to F;
2. JC performs a real blinded-key DatabaseLookup to F through its own Java-built tunnels;
3. F returns type 5;
4. Java verifies/decrypts it;
5. Java connects to the i2pr encrypted service;
6. an application payload round-trips.

### B. Java publisher/service -> i2pr consumer/client

Prove:
1. JC publishes a stock Java type-5 record to F;
2. F stores it;
3. R resolves the B33 through the production Plan-351 path;
4. R receives type 5 from F and validates/decrypts it;
5. the inner destination is installed only after its signature binds to the B33 signing key;
6. an application payload round-trips.

## Capability matrix

No-auth is mandatory both directions.

Source-prove Java 2.13.0 support/configuration for:
- lookup secret;
- PSK client authorization;
- DH/X25519 client authorization.

Execute every overlapping mode. Unsupported reference modes require a pinned-source
`reference-not-applicable` classification.

For auth rows include authorized success and wrong/missing credential failure.

## Reference-control rows

Where practical, add Java -> Java control rows on the same F topology. These are diagnostic, not a
substitute for the two i2pr directions.

If a Java control row fails, stop attribution until the harness/reference setup is repaired.

## Persistence/rollover/negative matrix

Mirror Plan 374:
- i2pr restart and reconnect from Java;
- adjacent-day deterministic blinded-key transition plus current-day live row;
- malformed B33;
- wrong secret/key;
- stale/expired record;
- tampered signature/ciphertext;
- wrong blinded storage key;
- strict Proposal-146 type-11 form rejected by stock Java.

## Evidence artifact

Commit a bounded artifact/checker with:
- exact Java/i2pr pins;
- topology role and RouterInfo/caps facts needed to prove F is queried, not selected as a tunnel peer;
- store/lookup/application provenance;
- transcript profile;
- auth mode;
- sanitized failure classification.

The checker must fail if a Java success row could have been satisfied by local record injection or
by a topology that bypasses F.

## Acceptance criteria

Plan 375 passes only when both i2pr<->Java directions complete the real type-5 publication,
blinded lookup, decrypt/inner-LS2, and application path using stock unmodified Java I2P, with no
fabricated bandwidth tier.

Passing Plan 375 unblocks the Java half of Plan 377.
