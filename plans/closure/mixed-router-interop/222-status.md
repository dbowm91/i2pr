# Plan 222 status — M6 Java client-NetDB/OCMOSJ narrowing corrective

Status: **`registered-ready-m6-java-client-netdb-ocmosj-narrowing-corrective`**.

Plan of record:
[`222-m6-java-client-netdb-ocmosj-narrowing-corrective.md`](../../implementation/mixed-router-interop/222-m6-java-client-netdb-ocmosj-narrowing-corrective.md).

## Registration basis

Exact-pinned Java I2P 2.13.0 source review verifies that Plan 221 should not be
executed unchanged.

### Selector-equivalence correction

The real `IterativeSearchJob`:

- computes the daily routing key with
  `ctx.routingKeyGenerator().getRoutingKey(key)`;
- calls `FloodfillPeerSelector` with the routing key, not the original
  Destination hash;
- uses effective `netdb.searchLimit + EXTRA_PEERS`, not a fixed N=3; and
- performs the lookup through `ctx.clientNetDb(fromDestinationHash)`.

Client sub-databases share the main k-buckets and peer selector, but maintain
their own transient LeaseSet storage; their `getAllRouters()` fallback is
empty.

### Stronger public per-message observation

The pinned public `I2PSession` API supports listener-enabled sends returning a
nonce:

```java
long sendMessage(..., SendMessageOptions options,
                 SendMessageStatusListener listener)
```

The same nonce is used for router admission and OCMOSJ terminal status
notifications. This allows exact reverse-send correlation without patching
Java, reflection, or private-state inspection.

Plan 222 therefore repairs selector equivalence first, then uses
`SendMessageStatusListener` as the primary OCMOSJ discriminator. Exact
router stats/log correlation is conditional fallback only.

## Current authority

```text
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required
plan_221 = superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective
plan_222 = registered-ready-m6-java-client-netdb-ocmosj-narrowing-corrective

plan_201 = blocked-pending-plan222-client-netdb-ocmosj-narrowing
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan222-narrowing
```

No topology/bootstrap/protocol corrective is authorized by this registration.
