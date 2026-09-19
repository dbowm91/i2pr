# Plan 224 status — M6 Java NO_LEASESET lookup-path attribution

Status: **`registered-ready-m6-java-no-leaseset-lookup-path-attribution`**.

Plan of record:
[`224-m6-java-no-leaseset-lookup-path-attribution.md`](../../implementation/mixed-router-interop/224-m6-java-no-leaseset-lookup-path-attribution.md).

## Registration basis

Plan 223 closed with:

```text
P223-NEXT-BOUNDARY ordered_statuses=[1, 21]
```

The Java helper accepts the tracked reverse send, but OCMOSJ reports
`STATUS_SEND_FAILURE_NO_LEASESET (21)`. Plan 223 also proves:

- the target Destination identity shape is now Java-compatible;
- Standard LS2 remains X25519/type 4;
- helper/source LeaseSetKeys are present and X25519-capable;
- target LS is absent in Router A's helper client DB before send;
- the exact selector is nonempty and contains Router B;
- i2pr publishes its local LS2 specifically toward Router B;
- no reverse payload arrives in the frozen 45-second window.

Exact-pinned Java I2P 2.13.0 source review shows that status 21 is not precise
enough to authorize a corrective. The missing target LS may originate at
publication/store, actual query dispatch, Router-B answering, encrypted reply
delivery, or helper client-subDB installation.

Plan 224 therefore performs attribution only.

## Source-backed narrowing

The pinned Java source establishes:

1. client LS lookups run through `IterativeSearchJob` with the helper's
   client outbound and inbound tunnels;
2. a floodfill answers an LS DLM only from a current LeaseSet marked
   `receivedAsPublished`;
3. an LS DSM arriving down a client tunnel is tagged with that client hash and
   routed to that exact client sub-DB;
4. `InNetMessagePool` stores a matching DSM inline before the lookup-success
   reply job is queued.

The fourth point rules out a speculative "lookup success raced client-DB
storage" explanation.

The first missing evidence is whether Router B's main NetDB actually contains
a current, query-answerable copy of the exact i2pr target LS2 after the
existing publication path.

## Current authority

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required
plan_221 = superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective
plan_222 = passed-m6-java-client-netdb-ocmosj-narrowing-corrective
plan_223 = passed-m6-java-destination-identity-crypto-separation-corrective-with-next-boundary-no-leaseset
plan_224 = registered-ready-m6-java-no-leaseset-lookup-path-attribution

plan_201 = blocked-pending-plan224-no-leaseset-lookup-path-attribution
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan224-attribution

next_executable_plan = 224-m6-java-no-leaseset-lookup-path-attribution
```

## Registration constraints

Plan 224 does not authorize a production correction.

It may add only:

- read-only Java main/client NetDB LS snapshots;
- scratch-only class-specific logger configuration;
- whitelist-only sanitized lookup-path facts;
- Rust test-driver/classifier evidence;
- static guards and tests.

It may not change:

- i2pr LS2 publication behavior;
- Java NetDB state;
- helper client options;
- tunnel pools;
- floodfill roles;
- selector behavior;
- SAM;
- topology;
- public-network participation.

Any concrete fix belongs to Plan 225 after Plan 224 closes with exact
attribution.

## Closure requirement

This status is registration only.

Closure must replace the token above with one exact P224 result and record all
required Plan-224 snapshots, sanitized trace facts, verification results, and
dependency audit.

Until then:

```text
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable             = not-yet-claimed
```
