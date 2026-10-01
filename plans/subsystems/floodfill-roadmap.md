# M12 Floodfill Roadmap

Status: Plans 270–276 passed; Plan 277 stopped at the runtime integration boundary; Plan 282 is the ready corrective; type-5 support remains deferred.

Long-term references:
- GUARDRAILS.md
- specs/CONFORMANCE.md
- specs/support.toml
- specs/SOURCES.md
- specs/protocols/04-reseed-netdb.md
- specs/protocols/02-i2np.md
- plans/implementation/workspace-foundation/000-mvp-roadmap.md

Related ADRs:
- docs/adr/0026-staged-interoperability-progression-and-java-debt.md
- `docs/adr/0027-floodfill-role-provenance-and-advertisement.md` is the M12 implementation authority; Plan 270 records source refresh and reconciles downstream requirements.

## 1. Purpose and ownership boundary

M12 adds Floodfill as an optional NetDB role inside the existing router. It is not a new
router implementation and it does not replace the M4 client-side NetDB machinery.

The floodfill subsystem owns server-side NetDB policy: provenance and disclosure eligibility,
bounded store/lookup serving, LeaseSet-family storage required for serving, direct replication,
maintenance, persistence policy, role health, and truthful floodfill capability advertisement.

Existing crates retain their current ownership:
- i2pr-proto owns wire structures and canonical codecs.
- i2pr-crypto owns protocol-specific cryptographic wrappers.
- i2pr-netdb owns runtime-neutral validation, stores, selection, and floodfill state machines.
- i2pr-netdb-persist owns persistence/restart composition.
- transports own authenticated links, never NetDB mutation or RouterInfo publication.
- i2pr-daemon owns runtime effects, queues, supervision, transport/tunnel dispatch, health
  snapshots, and local RouterInfo publication composition.

## 2. Work classification

Plans 270-272 are architecture/invariant foundations. Plans 273-277 are capability
construction. Plan 278 is one-family external experimental qualification. Plan 279 is the
second-family/full-advertisement gate and M12 closure.

No plan before 279 authorizes broad normal-daemon floodfill advertisement.

## 3. Non-goals

M12 does not add a reseed server, public-network stress testing, automatic UPnP/NAT-PMP,
a new peer-profile subsystem, recursive NetDB lookup serving, public production-readiness
claims, or changes to M11 transit semantics.

ElGamal floodfill-router reply encryption is not required unless Plan 270 finds current
minimum-version interoperability requires it. Current ECIES supplied-key reply encryption is
required.

## 4. Current state

Authority entering M12:
- Plan 268 passed the one-family M11 experimental qualification.
- Plan 269 is the required global roadmap/support cleanup and must close before Plan 270
  executes.
- M4 already supplies validated RouterInfo storage, reseed/cache reload, daily routing-key
  derivation, XOR-distance and nearest-floodfill selection, client-side iterative lookup, and
  local publication coordination.
- M6 supplies a bounded Standard LeaseSet2 validator/store but only for its current subset.
- I2NP already parses DatabaseStore, DatabaseLookup, DatabaseSearchReply, and reply-encryption
  fields.
- SSU2 has controlled authenticated router I2NP delivery and deterministic publication
  snapshots.
- LocalRouterInfoBuilder intentionally rejects caps=f and normal local RouterInfo construction
  does not yet compose a qualified transport address.
- Unsolicited store handling is currently RouterInfo-only and does not implement floodfill
  serving semantics.
- MetaLeaseSet and EncryptedLeaseSet are not yet authoritative served record types.

## 5. Target architecture

The target is a runtime-neutral FloodfillService over validated stores and explicit inbound
provenance. The service consumes decoded messages plus authenticated context and emits typed
actions; it does not own sockets, Tokio tasks, tunnel pools, or filesystem I/O.

Core model:

~~~text
authenticated inbound I2NP
  -> namespace/provenance classification
  -> record validation + per-type quota
  -> floodfill server policy
       store -> ack decision -> replication eligibility
       lookup -> store hit OR bounded search reply
  -> typed effects
  -> daemon dispatch over direct SSU2 or explicit reply tunnel

maintenance
  -> expiry/replacement/conflict
  -> daily + next-day routing-key replication
  -> persistence/restart revalidation
  -> role-health evaluation
  -> truthful RouterInfo capability publication/withdrawal
~~~

Validation and provenance stay distinct. A cryptographically valid record is not necessarily
safe to answer from or flood. Main-router and client-tunnel namespaces must not leak into one
another.

## 6. Dependency graph

~~~text
269 global roadmap/support reconciliation
 -> 270 M12 architecture authority + source/ADR freeze
 -> 271 NetDB provenance, namespace, and disclosure foundation
 -> 280 reviewed Red25519 verifier/provider prerequisite (stopped)
 -> 281 type-5 support-floor correction (passed; type 5 deferred)
 -> 272 complete floodfill record validation/storage surface
 -> 273 inbound DatabaseStore service + acknowledgements/throttles
 -> 274 DatabaseLookup/DSRM service + ECIES reply protection
 -> 275 direct flood replication + routing-key rollover policy
 -> 276 persistence, maintenance, and resource-governance closure
 -> 277 daemon composition + role lifecycle + controlled advertisement (stopped partial)
 -> 282 runtime publication/reply-delivery contract corrective
 -> 278 exact-pinned i2pd one-family qualification
 -> 279 second-family qualification + normal opt-in activation + M12 closure
~~~

A corrective plan may be inserted only when closure evidence identifies a concrete defect or
missing requirement. Do not grow an open-ended external harness chain.

## 7. Milestones

| Plan | State | i2pr token at registration | Implementation | Closure |
|---|---|---|---|---|
| 270 | closed | passed-m12-architecture-authority-source-refresh-and-adr-freeze | plans/implementation/floodfill/270-m12-architecture-authority.md | plans/closure/floodfill/270-status.md |
| 271 | passed | passed-m12-provenance-namespace-and-disclosure-foundation | plans/implementation/floodfill/271-m12-netdb-provenance-segmentation.md | plans/closure/floodfill/271-status.md |
| 272 | passed | passed-m12-record-validation-storage-types-1-3-7-type5-deferred | plans/implementation/floodfill/272-m12-floodfill-record-validation-storage.md | plans/closure/floodfill/272-status.md |
| 273 | passed | passed-m12-bounded-databasestore-service | plans/implementation/floodfill/273-m12-databasestore-service.md | plans/closure/floodfill/273-status.md |
| 274 | passed | passed-m12-lookup-serving-bounded-dsrm-and-ecies-replies | plans/implementation/floodfill/274-m12-databaselookup-service-and-reply-protection.md | plans/closure/floodfill/274-status.md |
| 275 | passed | passed-m12-bounded-direct-replication-and-daily-routing-key-rollover | plans/implementation/floodfill/275-m12-direct-replication-and-routing-key-rollover.md | plans/closure/floodfill/275-status.md |
| 276 | passed | passed-m12-versioned-floodfill-persistence-maintenance-and-resource-governance | plans/implementation/floodfill/276-m12-persistence-maintenance-resource-governance.md | plans/closure/floodfill/276-status.md |
| 277 | stopped | stopped-m12-daemon-runtime-publication-and-reply-adapter-contract-required | plans/implementation/floodfill/277-m12-daemon-role-lifecycle-and-controlled-advertisement.md | plans/closure/floodfill/277-status.md |
| 282 | ready | registered-m12-runtime-publication-delivery-corrective-ready | plans/implementation/floodfill/282-m12-runtime-publication-and-reply-delivery-contract-corrective.md | future |
| 278 | blocked | registered-m12-i2pd-qualification-blocked-on-plan282 | plans/implementation/floodfill/278-m12-i2pd-controlled-qualification.md | future |
| 279 | blocked | registered-m12-full-advertisement-blocked-on-plan278 | plans/implementation/floodfill/279-m12-second-family-qualification-and-activation.md | future |
| 280 | stopped | stopped-no-acceptable-maintained-i2p-red25519-provider | plans/implementation/floodfill/280-m12-red25519-provider-qualification.md | plans/closure/floodfill/280-status.md |
| 281 | passed | passed-m12-record-floor-corrected-type5-deferred | plans/implementation/floodfill/281-m12-encrypted-leaseset-floor-correction.md | plans/closure/floodfill/281-status.md |

## 8. Cross-cutting requirements

- Every peer-controlled count, byte length, queue, map, cryptographic operation, retry, and
  direct-dial fanout has an explicit ceiling.
- No peer-controlled panic path.
- Main-router and client NetDB namespaces are explicit; a client-tunnel-learned LeaseSet may
  never become router-level answer material by accident.
- Record validity, provenance, disclosure eligibility, and replication eligibility are
  separate typed decisions.
- Daily routing keys are local selection values only; wire messages continue to carry real
  hashes.
- Flood replication is direct only and zero-reply-token only.
- A zero-token replicated store is never re-flooded.
- Expired LeaseSets and RouterInfos older than the normative flood age are not replicated.
- EncryptedLeaseSet contents remain opaque to a floodfill; outer structure/signature/freshness
  are validated without decryption.
- Secret reply keys/tags are redacted and zeroized.
- Floodfill work is lower priority than router-owned/client work and participates in global
  resource governance.
- caps=f follows readiness and health; configuration alone cannot create advertisement.
- Broad normal-daemon advertisement remains unavailable until Plan 279 satisfies ADR 0026.

## 9. Verification strategy

Plans 271-277 require deterministic local tests with caller-supplied time and reproducible
fixtures. Boundary scripts must statically enforce the runtime-neutral/server-versus-daemon
seams. Fuzz/property coverage is required for new record parsers and server-message entry
points.

Plan 278 uses exact-pinned i2pd 2.61.0 at
635b013a612ff47278ef02acf8580a28e10e26c5 for controlled one-family progression.

Plan 279 uses Java I2P 2.13.0 at
9134f808337b401e8e53c73734c81fab04280c9d, or a plan-recorded same-family equivalent only
if the exact Java target is demonstrably unsuitable. Java/I2P+ counts as one family. No
reference source may be patched to obtain a pass.

## 10. Risks and decision points

Primary risks are metadata disclosure through NetDB namespace confusion, reply amplification,
storage/crypto DoS, incorrect routing-key rollover behavior, stale capability advertisement,
and conflating one-family progression with full advertisement.

Plan 270 freezes exploration-hit behavior and type-7 policy; Plan 281 defers type-5 until an
I2P-compatible Red25519 provider is reviewed. Plan 272 implements the currently supported
record floor, unsupported signature/key policy, persistence scope, and RouterInfo
version/capability semantics. Plan 280 stopped because the workspace lacks a compatible
Red25519 (type 11) verifier for EncryptedLeaseSet validation; no local primitive or unverified
substitute is authorized.

## 11. Completion definition

M12 is complete only when:
- the router serves bounded correct NetDB store/lookup behavior in a controlled mixed-router
  testnet;
- all supported NetDB records are signature/freshness validated before serving/flooding;
- namespace/provenance rules prevent client/router NetDB disclosure;
- direct replication, rollover, expiry, persistence, restart, throttling, and amplification
  controls are verified;
- health loss removes caps=f and stops admitting new floodfill work;
- two independent implementation families interoperate for the advertised surface;
- normal-daemon floodfill remains opt-in and default-off;
- no critical/high finding remains open.

## 12. Milestone status summary

M11 is closed for experimental progression. Plans 270, 271, 281, and 272 have passed. Plans 273–276
have passed on the type-5-deferred floor. Plan 277 stopped at the runtime-owned publication and
delivery boundary; Plan 282 is the sole ready corrective and retains the useful Plan 277 partial
implementation. Plans 278–279 remain blocked in dependency order behind Plan 282;
Plan 280 remains stopped pending a separately reviewed I2P-compatible Red25519 provider.
