# Plan 223 status — M6 Java Destination identity / LeaseSet2 crypto-separation corrective

Status: **`registered-ready-m6-java-destination-identity-crypto-separation-corrective`**.

Plan of record:
[`223-m6-java-destination-identity-crypto-separation-corrective.md`](../../implementation/mixed-router-interop/223-m6-java-destination-identity-crypto-separation-corrective.md).

## Registration basis

Plan 222 closed the client-NetDB/OCMOSJ narrowing with:

```text
P222-CORRECTED-ATTRIBUTION OCMOSJ-UNSUPPORTED-ENCRYPTION
```

Exact-pinned Java I2P 2.13.0 source review after that closure identified a
more specific, earlier status-17 branch than Plan 222 distinguished:

```java
if (_to.getEncType() != EncType.ELGAMAL_2048) {
    dieFatal(MessageStatusMessage.STATUS_SEND_FAILURE_UNSUPPORTED_ENCRYPTION);
    return;
}
```

This guard executes in
`OutboundClientMessageOneShotJob.runJob()` before the client-NetDB lookup
and before the later LeaseSet2 key-intersection check.

Current i2pr router-owned Destination generation embeds the local X25519
static public key into the Destination and advertises X25519/type 4 in the
Destination key certificate. Java's own `I2PClientImpl.createDestination()`
instead keeps the legacy 256-byte Destination public-key slot in its
ElGamal/type-0 identity shape; modern X25519 destination encryption is
advertised separately in LeaseSet2.

The current repository already contains evidence that these concepts should
be independent:

- `DestinationIdentity::from_imported()` documents that Java/i2pd populate
  the Destination encryption-public field with legacy/unused material;
- `DestinationPublic::from_destination()` accepts legacy ElGamal/type 0
  Destinations;
- `build_signed_lease_set2()` independently emits X25519/type 4 from the
  router-owned static X25519 key;
- the Java helper requests `i2cp.leaseSetType=3` and
  `i2cp.leaseSetEncType=4`.

Plan 223 first proves the early guard against the exact external-lane
Destination bytes, then permits the narrow identity/LS2 separation fix. If
status 17 persists after that correction, the plan records source
`LeaseSetKeys`, target LS2 key types, and the exact compatibility
intersection, then stops for a successor rather than implementing a second
unrelated corrective.

## Current authority

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = retained-diagnostic-instrumentation-attribution-invalid-superseded-by-plan220
plan_220 = passed-m6-java-plan219-diagnostic-attribution-corrective-with-selector-equivalence-followup-required
plan_221 = superseded-before-execution-by-plan222-client-netdb-ocmosj-narrowing-corrective
plan_222 = passed-m6-java-client-netdb-ocmosj-narrowing-corrective
plan_223 = registered-ready-m6-java-destination-identity-crypto-separation-corrective

plan_201 = blocked-pending-plan223-destination-identity-crypto-separation-corrective
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure-pending-plan223

next_executable_plan = 223-m6-java-destination-identity-crypto-separation-corrective
```

## Registration constraints

Plan 223 does not authorize:

- Java source changes;
- ElGamal end-to-end destination encryption;
- LS2 downgrade from X25519/type 4;
- RouterInfo/router-identity crypto changes;
- bootstrap/floodfill/client-NetDB selector changes;
- SAM pivot;
- tunnel/topology tuning;
- public-I2P participation;
- persistent identity migration hidden inside the corrective.

The exact Java/i2pd pins remain frozen.

## Closure requirement

This registration record is not completion evidence. Replace the current
status only after implementation is committed, exact-head verification is
run, and the closure record contains the full Plan-223 evidence template and
dependency/unblock audit.

Until then:

```text
milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable             = not-yet-claimed
```
