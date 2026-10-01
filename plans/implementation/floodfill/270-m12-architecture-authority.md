# Plan 270 — M12 floodfill architecture authority, source refresh, and ADR freeze

Status at registration:
**registered-m12-architecture-authority-blocked-on-plan269**

Classification: architecture/invariant planning. No production capability.

Hard dependency: Plan 269 closed.

## 1. Objective

Freeze the M12 architectural and interoperability contract before floodfill production code is
written. Refresh the exact source ledger for the NetDB/Floodfill surface, create the M12 ADR,
and reconcile the floodfill roadmap/protocol dossier with decisions that subsequent plans can
implement without guessing.

## 2. Why this plan is blocked but fully specified

Plan 268 makes M12 planning dependency-ready, but Plan 269 owns the global current-state cleanup.
Executing this plan before 269 closes risks reintroducing contradictory current-state authority.
The Plan 269 unblock audit must move Plan 270 to ready.

## 3. Current implementation evidence

Preserve and explicitly inventory:
- i2pr-netdb daily_routing_key(), XOR distance, nearest() and nearest_floodfill();
- bounded RouterInfoStore and Standard LeaseSet2 store;
- client-side RouterInfo/LeaseSet2 lookup machinery and DatabaseSearchReply ingestion;
- I2NP DatabaseStore/Lookup/SearchReply codecs and reply-encryption parse surface;
- RouterInfo-only unsolicited-store handler;
- i2pr-netdb-persist revalidation of RouterInfo cache/reseed material;
- SSU2 authenticated router-I2NP daemon lane and publication snapshots;
- LocalRouterInfoBuilder rejection of caps=f and unreviewed capability letters;
- ADR 0026 one-family progression versus two-family full-advertisement policy.

## 4. Required decisions

Create docs/adr/0027-floodfill-role-provenance-and-advertisement.md and freeze:

1. Floodfill is a server role layered on i2pr-netdb; RouterInfoLookup remains a client state
   machine and is not made bidirectional.
2. Explicit NetDB namespaces and provenance: main-router versus client namespace; direct
   authenticated peer versus client/tunnel path; unsolicited/published store versus lookup
   response; zero-token replicated store versus publisher store.
3. Disclosure and replication eligibility are derived from validation plus provenance; a valid
   record is not automatically answerable/floodable.
4. Current record support floor for broad floodfill operation: RouterInfo and DatabaseStore
   types 1, 3, 5, and 7. Type 5 remains opaque; a floodfill validates outer
   signature/freshness/flags but does not decrypt.
5. Exact key derivation, signature domain, freshness, unpublished-bit, offline-signature, and
   replacement policy for types 1/3/5/7.
6. DatabaseLookup compatibility policy for lookup types 00/01/10/11, exploration-hit behavior,
   excluded-peer handling, and DSRM result count (default 3, hard local ceiling <=16).
7. Current ECIES supplied-key reply format: one 32-byte key, one 8-byte tag, nonce 0, tag as
   associated data. ElGamal floodfill-router reply support is deferred unless current reference
   qualification demonstrates it is required.
8. Flood replication: validated-newer records only; direct connection only; reply token zero;
   recipient must not re-flood; fanout 3; current and next-day routing-key behavior; RI maximum
   flood age one hour; no expired LeaseSet flood.
9. Role states and advertisement gating. Configuration intent cannot mint caps=f; a typed
   readiness/health authority is required.
10. Persistence policy by record class and conservative restart provenance.
11. Evidence policy: Plan 278 may satisfy experimental one-family progression; Plan 279 is the
    second-family gate required before normal-daemon broad caps=f advertisement under ADR 0026.
12. RouterInfo router.version policy: advertise only the API/support level actually implemented;
    do not copy the current spec version as branding.

## 5. Source refresh

Re-verify specs/SOURCES.md against:
- official I2P common structures, I2NP, ECIES router reply encryption, encrypted LeaseSet, and
  network database documentation;
- exact-pinned i2pd 2.61.0 commit 635b013a612ff47278ef02acf8580a28e10e26c5;
- exact-pinned Java I2P 2.13.0 commit 9134f808337b401e8e53c73734c81fab04280c9d.

Reference implementations are behavioral oracles, not source to copy. Record implementation
differences separately from normative requirements.

If the official source has materially advanced beyond the existing 8859602... website pin,
record the new exact commit and the changed semantics in the ADR before downstream execution.

## 6. Scope

In scope: ADR, source ledger, protocol dossier status/decision sections, roadmap refinement,
support claim vocabulary, architecture boundaries, and explicit acceptance matrices.

Out of scope: Rust production code, new dependencies, floodfill activation, external test
execution, config changes, capability advertisement.

## 7. Work packages

A. Re-read exact official structures/I2NP/NetDB/ECIES pages and compare Java/i2pd only where the
spec is ambiguous.

B. Produce a requirement matrix for DatabaseStore types, lookup types, reply encryption,
flooding, expiry, persistence, role advertisement, and provenance.

C. Write ADR 0027 with rejected alternatives and security rationale.

D. Update specs/protocols/04-reseed-netdb.md and the floodfill roadmap so every Plan 271-279
requirement points to a frozen authority.

E. Add a compact M12 support/evidence vocabulary to specs/CONFORMANCE.md without claiming
implementation.

## 8. Failure / restart / contention semantics

Documentation-only. If upstream sources disagree materially, stop and record the disagreement;
do not select behavior from convenience. No current code/result is relabeled.

## 9. Compatibility and migration

None. This plan must not change wire, config, storage, RouterInfo, or runtime behavior.

## 10. Required verification

~~~bash
git diff --check
git diff --name-only <plan270-parent>..HEAD -- 'crates/**' 'tests/**' 'tools/**' '.github/**' 'Cargo.toml' 'Cargo.lock'
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
~~~

The production/test/workflow/dependency diff must be empty.

## 11. Documentation updates

Required: ADR 0027, specs/SOURCES.md M12 refresh, specs/protocols/04-reseed-netdb.md decision
resolution, plans/subsystems/floodfill-roadmap.md reconciliation, and any conformance text
needed to state the M12 evidence tiers.

## 12. Acceptance criteria

- Every decision in section 4 is explicit and source-backed.
- No open protocol ambiguity remains that would force Plans 271-277 to guess.
- Record support floor and unsupported behavior are explicit.
- One-family versus two-family advertisement gates are explicit.
- Plan 271 ownership and data model are sufficiently specified to implement.
- No production code or capability claim changes.
- No critical/high architecture finding remains open.

## 13. Stop conditions

Stop if current official specifications materially contradict the assumed record/reply model,
if the required broad record floor would require unsupported cryptography not yet scoped, or
if current reference implementations disagree on a security-sensitive behavior not resolved by
the specification. Register a focused ADR/research corrective instead of guessing.

## 14. Closure evidence

Record exact source revisions, requirement matrix, decisions/rejected alternatives, files changed,
verification results, and the unblock audit. On pass, move only Plan 271 to ready.

## 15. Handoff

Plan 271 implements provenance/namespace/disclosure foundations only; no floodfill serving or
advertisement begins in Plan 271.
