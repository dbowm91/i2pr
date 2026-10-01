# Plan 271 — M12 NetDB provenance, namespace segmentation, and disclosure foundation

Status: **passed-m12-provenance-namespace-and-disclosure-foundation**

Classification: security invariant / infrastructure.

Hard dependency: Plan 270 passed with ADR 0027 accepted.

## 1. Objective

Add the typed provenance and namespace model required to prevent NetDB disclosure confusion before
any floodfill server path exists. Preserve existing client lookup behavior while making it
impossible for client-tunnel-learned or lookup-response-only material to become router-level
answer/flood material through an implicit store API.

## 2. Current evidence

RouterInfoStore and LeaseSet2Store currently model validated values and quotas, not why/how a value
was learned. store_message.rs has only a coarse Accept/Reject unsolicited RouterInfo policy.
Java reference behavior distinguishes router-level and client NetDB contexts and distinguishes
published/unsolicited data from lookup replies. The project guardrails already require narrow
capabilities, bounded state, and privacy-safe behavior.

## 3. Invariants

- Validation state and provenance state are separate.
- Namespace identity is explicit on every server-eligible record.
- Main-router queries never read a client namespace.
- Client-namespace queries never fall back to main-router LeaseSets unless ADR 0027 explicitly
  permits a narrow RouterInfo exception.
- Lookup-response provenance cannot silently become publish-safe.
- Zero-token flood replicas cannot silently become re-flood candidates.
- No raw destination/router hash appears in Debug or metric labels.
- Metadata count/bytes are bounded independently of record bytes.
- Existing M4/M6 client lookup APIs retain behavior unless migrated explicitly.

## 4. Required production changes

Add runtime-neutral types in i2pr-netdb, names finalized by implementation but equivalent to:
- NetDbNamespace: MainRouter or bounded opaque ClientNamespaceId.
- InboundProvenance: authenticated direct peer / router tunnel / client tunnel / local.
- StorePurpose: PublishedStore, LookupResponse, FloodReplica, LocalPublication.
- RecordProvenance with observed time and authenticated source category.
- DisclosureDecision / ReplicationDecision derived by pure policy.
- ProvenancedRecord metadata wrapper or side index keyed by record identity and type.

Do not embed sockets, tunnel objects, daemon handles, or client Destination keys in these types.

## 5. Scope

In scope: metadata model, bounded metadata stores/indexes, migration adapters for current RI/LS2
stores, pure disclosure policy, namespace-safe getters/iterators, and deterministic tests.

Out of scope: new record codecs, inbound DatabaseStore serving, lookup replies, flooding,
persistence format changes, daemon composition, caps=f.

## 6. Work packages

A. Define opaque namespace/source/purpose types and privacy-safe Debug.

B. Add bounded metadata accounting with explicit count and byte ceilings. Capacity failure must
not mutate existing authoritative state.

C. Add insertion APIs that require provenance for server-authority paths. Retain narrowly named
client-only adapters where existing lookup code needs them; mark ambiguous APIs internal or
deprecated rather than changing semantics silently.

D. Implement pure policy functions:
- may_answer_router_lookup
- may_answer_client_lookup
- may_replicate
- may_persist
All return typed reasons, not booleans only.

E. Add namespace-safe iteration/selection views so later DSRM and flooding code cannot accidentally
iterate client-only data.

F. Add static/boundary checks forbidding i2pr-netdb from importing daemon/runtime transport
implementation types.

## 7. Failure / cancellation / restart / contention semantics

No async tasks. Insertions fail closed on metadata quota exhaustion. Replacement of record bytes
must update metadata atomically in one synchronous operation. A failed provenance update cannot
leave a new record with old disclosure authority.

Restart semantics are not implemented here; Plan 276 will persist/rederive metadata. Tests must
prove a caller cannot fabricate a permissive default by omitting provenance.

## 8. Compatibility and migration

No wire/config/storage format changes. Existing client APIs may receive compatibility constructors
that assign an explicit ClientLookupResponse or MainLookupResponse purpose; no unrestricted
Default implementation may create answerable/floodable provenance.

## 9. Required tests

At minimum:
- main/client namespace isolation in both directions;
- lookup-response-only record denied router answer/flood;
- published direct record receives only ADR-authorized eligibility;
- zero-token replica is not re-floodable;
- metadata count/byte quota exhaustion is atomic;
- replacement cannot retain stale permissive provenance;
- redacted Debug;
- deterministic time/provenance expiry;
- existing RouterInfo and LeaseSet2 client lookup suites unchanged.

## 10. Exact verification

~~~bash
cargo fmt --all --check
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked -p i2pr-daemon --test netdb_integration
cargo clippy --locked -p i2pr-netdb --all-targets -- -D warnings
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
~~~

Add and run scripts/check-m12-floodfill-boundaries.sh if Plan 270 specified it.

## 11. Documentation

Document namespace/provenance semantics in the M12 ADR and NetDB architecture docs. Update support
matrices only to say the infrastructure exists; floodfill capability remains unclaimed.

## 12. Acceptance criteria

- Every server-relevant record access requires an explicit namespace/provenance-aware API.
- The required negative isolation tests pass.
- No unbounded metadata collection exists.
- Existing client NetDB behavior remains green.
- No floodfill message is served or replicated.
- caps=f remains rejected.
- No critical/high finding remains open.

## 13. Stop conditions

Stop if current record stores cannot adopt provenance without ambiguous dual authority, if a
namespace identifier would require retaining sensitive Destination material, or if an existing
caller depends on cross-namespace leakage. Register a corrective migration plan instead of
adding a bypass.

## 14. Closure evidence

Include API migration table, quota/accounting tests, isolation test names, dependency checks,
and source diff review. On pass, move only Plan 272 to ready.

## 15. Handoff

Plan 272 may add record types only through the provenance-aware storage contract established here.
