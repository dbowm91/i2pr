# Plan 395 — encrypted type-5 publication coordinator corrective

Status: **blocked-post-admission-reverse-els2-delivery-plan-396**.

Subsystem: Proposal 170 / Red25519 + ELS2.

## Objective

Make the existing destination publication coordinator admit and publish the
encrypted type-5 DatabaseStore records already produced for services with ELS2
material. Preserve the blinded storage key and use the existing outbound tunnel,
NetDB candidate selection, bounded pending map, response correlation, and Plan 393
failure cancellation. Then rerun the exact-pinned NONE reverse lane.

## Evidence and constraints

Plan 394's exact-pinned run localized all 35 begin rejections to the body/key
validation category. `service_publication_store` emits
`DatabaseStoreData::EncryptedLeaseSet` for an encrypted service, while
`begin_ls2_publication` currently only accepts Standard LeaseSet2. Plan 346 and
ADR 0032 govern the deployed type-11 signature profile; Plan 350 governs type-5
store/serve behavior. Do not alter the type-11 transcript, type-5 wire format,
credentials, support inventory, or advertisement.

## Invariants

1. Stock i2pd 2.61.0 at exact SHA
   `635b013a612ff47278ef02acf8580a28e10e26c5`; `MAX_ATTEMPTS=1`.
2. Encrypted records retain their own blinded storage key, never the floodfill key
   or the service's unblinded destination hash.
3. Standard LeaseSet2 publication remains valid.
4. Composition, request-construction, and rejected-delivery paths release the
   intent; admitted sends retain correlation until completion or bounded expiry.
5. Bounded diagnostics expose no raw record, hash, peer, request id, or log.
6. Payloads remain on loopback SAM/private mesh; no direct transport fallback.

## Work packages

1. Trace type-5 DatabaseStore encode/decode and the existing coordinator request
   path. Define a typed accepted-record contract that supports Standard LS2 and
   encrypted type 5 without weakening key ownership checks.
2. Add deterministic positive and negative tests for type-5 admission, wrong-key
   rejection, Standard LS2 compatibility, capacity max/max+1, cancellation, and
   response correlation.
3. Keep the outbound composer and floodfill selection keyed to the record's
   destination-derived storage key; do not introduce a parallel publisher.
4. Run focused coordinator/publication tests, guards, and one exact-pinned NONE
   lane. If a later stage fails, register a successor.

## Acceptance

The reverse control-created encrypted service sends a valid type-5 DatabaseStore
through the real outbound tunnel, receives the stock NONE payload, and existing
Standard LeaseSet2 behavior and bounded lifecycle regressions pass. Then proceed
to authorized PSK/DH and remaining Proposal 170 gates only under their registered
plans. No capability or support claim is promoted by this plan.
