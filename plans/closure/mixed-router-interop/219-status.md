# Plan 219 status — M6 Java reverse-delivery root-cause investigation

Status: **`registered-ready-m6-java-reverse-delivery-root-cause-investigation`**.

Plan of record:
[`219-m6-java-reverse-delivery-root-cause-investigation.md`](../../implementation/mixed-router-interop/219-m6-java-reverse-delivery-root-cause-investigation.md).

## Registration basis

Plan 218 proved criteria 1–9 of its Java destination path and stopped at the
Java → i2pr reverse-delivery boundary. Its current
`java-floodfill-candidate=0` attribution is not precise enough to
authorize a corrective because:

- `java-floodfill-capable` currently proves the
  `router.floodfillParticipant=true` config line, not a live RouterInfo
  `f` capability;
- the floodfill positive/empty rows are coarse global log greps;
- pinned Java `FloodfillPeerSelector` starts from
  `peerManager().getPeersByCapability('f')`;
- RouterInfo store normally updates PeerManager capability indexing; and
- public helper reverse sends use `clientNetDb(_from)`, whose fallback
  behavior differs from the main NetDB.

Plan 219 owns evidence refinement and exact root-cause classification only.

## Current authority

```text
plan_217 = passed-m6-java-closure-harness-and-evidence-corrective
plan_218 = stopped-m6-java-second-family-direct-i2cp-inbound-delivery-boundary
plan_219 = registered-ready-m6-java-reverse-delivery-root-cause-investigation

plan_201 = blocked-pending-plan219-root-cause-classification
plan_205 = retained-deferred-conditional-after-plan218-direct-i2cp-requalification
plan_204 = blocked-on-m6-java-second-family-closure

milestone6_java_mixed_router_interop = not-yet-passed
milestone6_interoperable = not-yet-claimed
```

No corrective Plan 220 is registered. Plan 219 must first emit exactly one
J219-A..J terminal classification.
