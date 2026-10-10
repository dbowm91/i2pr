# M12 Floodfill Roadmap

Status: Plans 270–276 passed; Plans 277–282 and 278 stopped with retained bridge/route/delivery/qualification work; Plan 283 passed (Option 3 peer-test evidence driver + controlled activation); Plan 284 registered as the RouterInfo reference-acceptance corrective; type-5 support remains deferred.

**Recovery continuation (2026-10-10):** [Plan 437](../implementation/floodfill/437-truthful-bandwidth-tier-and-java-selection.md), blocked on Plans 433/436, derives an evidence-backed RouterInfo bandwidth class and re-proves stock Java floodfill **selection** without fabricated letters. [Plan 438](../implementation/floodfill/438-two-family-floodfill-and-normal-optin.md), blocked on 437, qualifies full i2pd+Java controlled matrix and guarded normal-daemon opt-in/withdrawal. These are forward successors to stopped Plan 306; earlier closure evidence and its blocked history are not rewritten. [Recovery roadmap](core-router-recovery-roadmap.md). Normal caps=f stays forbidden until two-family and health qualifications genuinely pass.

**Corrective successor (2026-10-10):** [Plan 444](../implementation/floodfill/444-configured-shared-bandwidth-class-and-java-selection-control.md) is registered, blocked on Plan 440, to derive a truthful enforceable **configured shared bandwidth** class independently of historical idle measured utilization, and to separate Java peer selection from the SAM bootstrap deadlock with independent known-good floodfill peers. Source reference: Emissary `subsystem/bandwidth.rs::BandwidthTracker::new` derives the shared class from configured bandwidth × share, while usage meters govern congestion. The official I2P network database docs identify class `O` for 128–256 KB/s shared bandwidth and a 128 KB/s floodfill minimum (check the pinned Java mapping/units during implementation); no manual cap override, no public claim. Plan 444 feeds **Plan 437**; original 437/438 normal-role prerequisites remain required. [Cross-subsystem graph](core-router-recovery-roadmap.md).

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
 -> 282 runtime publication/reply-delivery contract corrective (stopped; mechanics retained)
 -> 283 third-class evidence and controlled activation corrective (passed, Option 3)
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
| 282 | stopped | stopped-m12-activation-blocked-on-above-floor-reachability-evidence-corrective-via-plan283 | plans/implementation/floodfill/282-m12-runtime-publication-and-reply-delivery-contract-corrective.md | plans/closure/floodfill/282-status.md |
| 283 | passed | passed-m12-third-class-evidence-and-controlled-activation | plans/implementation/floodfill/283-m12-third-class-evidence-and-controlled-activation.md | plans/closure/floodfill/283-status.md |
| 278 | stopped | stopped-m12-reference-client-rejects-the-controlled-routerinfo-before-any-matrix-row | plans/implementation/floodfill/278-m12-i2pd-controlled-qualification.md | plans/closure/floodfill/278-status.md |
| 284 | retained | registered-m12-controlled-publication-reference-acceptance-corrective | plans/implementation/floodfill/284-m12-controlled-publication-reference-acceptance-corrective.md | **Closure record missing.** Gate 1 (one shared `netId`/`router.version` declaration, `8d6e102`) landed here; Gate 2 was handed to and closed by Plan 285 (`a16ae15`). Delivered, not open — but `plans/closure/floodfill/284-status.md` does not exist, so this plan has no closure record of its own. The `ready` state previously shown here was stale: the work is done. |
| 285 | retained | retained-m12-peer-testing-implemented-and-declared-pending-mixed-router-evidence | plans/implementation/floodfill/285-m12-peer-testing-i2np-surface-and-version-declaration.md | future |
| 302 | passed | passed-m12-floodfill-reply-wire-form-and-replication-answered | plans/implementation/floodfill/302-m12-floodfill-reply-wire-form-and-replication-corrective.md | plans/closure/floodfill/302-status.md |
| 303 | passed | passed-m12-matrix-execution-with-unseeded-publisher-trigger | plans/implementation/floodfill/303-m12-matrix-execution-with-unseeded-publisher-trigger.md | plans/closure/floodfill/303-status.md |
| 279 | stopped | stopped-m12-java-never-initiates-to-caps-f-only-controlled-ri | plans/implementation/floodfill/279-m12-second-family-qualification-and-activation.md | plans/closure/floodfill/279-status.md |
| 306 | stopped | stopped-m12-java-requires-bandwidth-tier-beyond-reviewed-fR | plans/implementation/floodfill/306-m12-loopback-reachability-caps-and-java-requalification.md | plans/closure/floodfill/306-status.md |
| 364 | passed | invariant corrective — the boundary guard for this milestone was silently failing | plans/implementation/floodfill/364-m12-boundary-guard-stale-floor.md | plans/closure/floodfill/364-status.md (`passed-guard-green-in-floor-and-ci-with-nine-traced-assertions`) |
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
- Broad normal-daemon advertisement remains unavailable until the second-family gate passes (Plans 279 and 306 stopped; reopen needs a truthful bandwidth-class design).

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
delivery boundary; Plan 282 corrected the route/delivery/lifecycle mechanics and stopped at
the above-floor reachability evidence boundary with that work retained. Plan 283 passed the
third-class evidence strategy (Option 3 peer-test driver) plus activation/withdrawal
completion. Plan 278 built and executed its exact-pinned i2pd lane, passed controlled
activation, and stopped at the reference-client RouterInfo-acceptance boundary before any
matrix row, retaining the lane, the two-phase stable-identity activation flow, and the
boundary diagnostics. Two reference gates are localized. `RouterInfo.cpp:508` marks a record
unreachable unless it carries both `netId` and `router.version`, which is the proximate cause
of the 64/64 rejection. `RouterInfo::IsEligibleFloodfill` then requires
`router.version >= 0.9.62` with no high-bandwidth alternative and is consulted on every
peer-side floodfill insert, so seeding and wire-learned paths both hit it. i2pr can claim
neither that version nor `O` honestly, so Plan 284 closes at a recorded conformance boundary
rather than an implementation fix.

Plan 284 delivered the mechanical half of the reference gate: the controlled floodfill,
withdrawal, and controlled SSU2 identity paths now share one `netId`/`router.version`
declaration, so the reference no longer marks the controlled record unreachable at parse
time.

Plan 285 delivered the eligibility half. `TunnelTestMessage` (231) is implemented with a
reference-derived golden vector, exact-consumption decoding, a typed dispatcher arm, and a
bounded responder/tracker pair in `i2pr-daemon::peer_test`. `i2pr` now implements the
complete I2NP message surface the pinned reference enumerates, so the declaration is 0.9.62,
pinned to that surface by a test that fails if the claim outruns the implementation. It
closes **retained**: the `specs/CONFORMANCE.md` mixed-router step is unmet and needs one
bounded exact-pinned external attempt, so Plan 278 is unblocked to `ready` but no external
row is claimed.
Plan 279 stopped after three frozen-budget Java attempts at the caps-gated
initiation boundary (see `plans/closure/floodfill/279-status.md`); Plan 280
remains stopped pending a separately reviewed
I2P-compatible Red25519 provider.

The post-284/285 bounded attempt ran and spent Plan 278's frozen budget. It closed both
admission gates live (the reference client loads the controlled record as its only
floodfill, publishes to i2pr, 18 lookups flow) but stopped at matrix F on two findings
(278-status §11–§12, owned by Plan 302): every floodfill reply is dropped by the
reference as expired because the old encoder wrote the 16-byte standard header while
the SSU2 session layer reads the 9-byte short-transport form (proven wire defect), and
no `DirectFlood` was planned for the accepted store (open: idempotent insert vs empty
plan). Zero matrix rows claimed.

Plan 302 passed on head `389846f`: the outer floodfill encoding is now
short-transport (inner clove stays standard), and the single fresh-budget attempt
proves it live — the reference logs `Publishing confirmed`, zero expired drops
for i2pr-sent messages, lookup record answers consumed (hits 0→2). The §12.2
question is answered both locally and live: the publisher store is an idempotent
re-publication of the seeded client key (`outcome=["idempotent"]`,
`replication_offered=0`), so no replication is offered by design. Matrix F is
structurally unpassable while the lane seeds the publisher key; ready Plan 303
owns the single trigger change (seed A/B only) plus matrix A–I execution.
Plan 279 executed after the Plan 303 pass and stopped as recorded above.

Plan 303 passed on head `c069e6c`: with the single lane change (seed A/B
only, driver-only diff), the one fresh-budget attempt passes all 10 matrix
rows — the publisher store inserts with replication offered, 4 zero-token
direct replica stores land, the reference confirms the publish with zero
expired drops, and all 7 lookups are answered. The one-family (i2pd 2.61.0)
controlled matrix is closed. Plan 279 executed its three frozen-budget Java
attempts and stopped at the publisher rendezvous on all three (stock Java
loads, verifies, and floodfill-lists the controlled RI but never initiates
transport to it); its lane, rendezvous, relay mesh, census, withdrawal, and
evidence checker are retained. Plan 306 decided the reachability caps
(ADR 0030), requalified i2pd on the fR record, and stopped at the
bandwidth-tier selection boundary (no executable in-bounds next step;
reopen needs a truthful bandwidth-class design). Plan 278 stays
stopped as history.
