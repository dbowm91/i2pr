# Plan 305 — Target-scoped Destination lifecycle and tunnel peer-diversity ownership

Status at registration: **registered-anonymity-target-scope-and-path-diversity-ownership**

Classification: invariant + production architecture/capability corrective. This plan follows
the stopped Plan 300 record and establishes the missing owners required before anonymity
qualification can resume.

Hard dependencies:
- Plan 296 closed and its service-boundary invariant remains mandatory.
- `plans/closure/anonymity/300-status.md` is the authoritative stop record.
- ADR 0029 governs Destination isolation, reference convergence, fail-closed diversity, and
  claim scope.
- Existing Destination/NetDB/tunnel/service-tunnel owners remain authoritative; this plan
  composes them rather than creating a second routing stack.

This plan is independent of Plan 304 and may execute in parallel.

## 1. Objective

Establish two missing production ownership boundaries:

1. Multi-target HTTP and SOCKS client services must select a bounded ephemeral client
   Destination by **canonical remote Destination hash**, so unrelated remote Destinations do
   not share a client identity under the qualified dedicated policy.
2. Destination tunnel construction must select peers through one explicit
   reference-derived candidate/diversity policy **before** `ShortBuildPath` is constructed,
   with typed insufficient-diversity outcomes and no silent weakening.

Retain Plan 300's repeated-router rejection and three-hop service compatibility profile.
This plan establishes architecture and deterministic local evidence; it does not itself
close the broader Plan 300 anonymity qualification.

## 2. Why this is dependency-ready

Plan 300 localized the blockers precisely:

- `ServiceRuntime` owns one `destination_id` per service. HTTP and SOCKS accept arbitrary
  remote I2P targets, so the same service Destination currently reaches unrelated targets.
- Fixed-target Generic and IRC clients do not have that fanout problem.
- `ShortBuildPath::validate` now rejects duplicate router hashes, but it receives an
  already-resolved path. There is no candidate-selection owner carrying family/network
  metadata where richer diversity policy can be enforced.

These are architectural ownership gaps. No external capture is required to create the
owners, and Plan 304 is therefore not a dependency.

## 3. Current implementation evidence

The implementation agent must start from these facts and verify them again on its baseline:

- `ServiceTunnelManager` holds one runtime map, one Destination registry, and one
  `destination_id` on each `ServiceRuntime`.
- HTTP and SOCKS resolve the remote target per connection but call Streaming through that
  service runtime's single Destination.
- Client identities are ephemeral CSPRNG identities; server Destination persistence is a
  separate storage contract and must not change.
- `DestinationPolicy::Dedicated` documents one destination/pool per service, while
  `SharedClientGroup` is the only explicit sharing surface.
- Service tunnel Destinations now use
  `DestinationConfig::service_compatibility_profile()` with three hops.
- `ShortBuildPath::validate` rejects a repeated `RouterHash`.
- The tunnel build crate consumes selected `HopSpec`/router identities but does not own a
  NetDB peer-candidate search policy.
- i2pr-netdb already owns validated RouterInfo/provenance data; do not duplicate a NetDB
  inside the service or tunnel layer.

## 4. Invariants that must not regress

1. Server Destination identities and their persistence/restart semantics are unchanged.
2. Fixed-target Generic/IRC client services remain one dedicated ephemeral identity unless
   the explicit sharing policy says otherwise.
3. Under qualified dedicated HTTP/SOCKS behavior, two distinct canonical remote Destination
   hashes never share the same client Destination identity.
4. Aliases resolving to the same Destination hash map to the same target scope; local alias
   text is never an isolation key.
5. Resource exhaustion fails closed. It must never fall back to a service-wide/shared
   identity merely to keep a connection working.
6. `SharedClientGroup` remains explicit, linkable, and outside the qualified isolated
   profile. Its runtime semantics must be truthful.
7. Child client identities are ephemeral; no new persistence of client private keys.
8. Every child Destination runtime/pool has a bounded owner, cancellation path, build
   concurrency accounting, and lifecycle generation.
9. Path selection never repeats a router within one path.
10. Exact family/network diversity rules are source-derived from frozen Java/i2pd behavior
    before production enforcement; do not invent prefix/family rules from intuition.
11. Candidate exhaustion returns a typed degraded/unqualified/failure result. Do not shorten
    the path, reuse a forbidden peer, or silently ignore diversity constraints.
12. i2pr-netdb namespace/provenance separation and tunneled client lookup/publication remain
    unchanged.
13. Production path selection uses cryptographic randomness where the existing owner
    requires it; deterministic selection is test-only.

## 5. Scope

### In scope

- A bounded target-scope owner under the existing service-tunnel manager.
- Canonical target keying by resolved remote Destination hash.
- Lazy ephemeral Destination creation, lookup/reuse, refcount/lease, drain, expiry, and
  service-generation cancellation for HTTP/SOCKS.
- Explicit resource accounting and a hard target-scope ceiling.
- Truthful `SharedClientGroup` composition or explicit configuration rejection if the
  documented sharing contract is not actually implemented.
- A candidate metadata contract sourced from validated NetDB RouterInfo.
- A destination-tunnel peer selector owned above `ShortBuildPath`.
- Exact-pinned Java/i2pd source research for family/subnet/path-overlap policy.
- Typed candidate rejection/exhaustion outcomes.
- Integration of the selector into service-Destination tunnel building.
- Deterministic local fixtures and static/boundary checks.

### Explicitly out of scope

- HTTP header convergence (Plan 297 successor).
- Streaming fingerprint tuning (Plan 299 successor).
- Public I2P qualification.
- Broad router-wide peer profiling/reputation systems.
- Persistence of HTTP/SOCKS client identities.
- Automatic sharing across services.
- A second NetDB, Destination registry, tunnel pool implementation, or path builder.
- Relaxing hop count below the three-hop service compatibility profile on scarcity.
- Claiming family/subnet resistance beyond the exact enforced rules.

## 6. Target architecture

~~~text
HTTP/SOCKS listener runtime
        |
resolve alias/B32 -> canonical Destination hash
        |
TargetDestinationOwner
  key = (service generation, canonical remote Destination hash)
        |
        +--> ephemeral DestinationRuntime + 3-hop pools
        |       refcount / idle drain / bounded capacity
        |
        +--> no cross-target fallback

Destination pool build request
        |
i2pr-netdb candidate adapter
(validated RouterInfo-derived metadata)
        |
DestinationPeerSelector
(reference-derived exclusion/diversity policy)
        |
QualifiedPath | InsufficientDiversity | NoCandidates
        |
ShortBuildPath::validate
(structural final guard, incl. no repeated router)
~~~

Policy ownership is intentional: NetDB supplies validated facts, the Destination/client
layer owns privacy selection policy, and the tunnel crate remains the final structural/wire
validator.

## 7. Ordered work packages

### WP1 — Freeze the reference diversity contract

Inspect the exact source pins already frozen by the anonymity workstream and write a compact
reference matrix covering, where present:

- same-router exclusion;
- router-family exclusion;
- IP/subnet or transport-address proximity exclusion;
- inbound/outbound endpoint reuse restrictions;
- same-peer reuse across tunnels/pools;
- scarcity/degraded behavior;
- any distinction between exploratory and client/Destination pools.

Record exact source file/symbol/commit authority. If Java and i2pd differ, choose one
coherent reference profile for the qualified i2pr service-Destination path and document the
difference. Do not combine isolated constants into a unique hybrid unless the behavior is
demonstrably common.

If the sources do not establish a candidate rule, leave that dimension unqualified rather
than inventing it.

### WP2 — Define the target-scope contract

Introduce a runtime-neutral target key containing the canonical remote Destination hash
only; requested alias, hostname spelling, URL path, and port must not create a new identity
scope for the same remote Destination.

Introduce a bounded owner with explicit outcomes such as:

- existing scope acquired;
- new scope created;
- capacity exhausted;
- scope draining;
- Destination creation failed;
- service generation cancelled.

The exact names may follow local conventions, but failures must be typed rather than free
text at the architecture seam.

Set and document a hard per-service target-scope ceiling. The value must be justified
against Destination registry capacity, tunnel-pool cost, and existing per-service/aggregate
connection ceilings. Active scopes may not be evicted. If the ceiling is reached and no
idle scope is eligible for bounded reclamation, the new connection fails closed.

### WP3 — Implement target-scoped client Destination lifecycle

For dedicated HTTP/SOCKS services:

1. resolve the remote Destination;
2. derive the canonical target key from its Destination hash;
3. acquire/create the target-scoped ephemeral Destination runtime;
4. ensure its inbound/outbound pools use the service compatibility profile;
5. register its delivery driver, inbound owner/indexes, outbound signal, and resource
   accounting through the existing manager machinery;
6. open Streaming through the target-scoped Destination;
7. release the scope when the connection ends;
8. reclaim only idle scopes under a bounded policy;
9. drain/cancel every child when its service generation drains.

Do not provision a service-wide fallback identity for multi-target traffic. A listener-level
runtime may retain service accounting/config state without being a traffic identity owner.

Client target identities rotate across daemon restart; same-target reuse within one live
scope is intentional to avoid unbounded identity/tunnel churn.

### WP4 — Make explicit sharing truthful

Trace `SharedClientGroup` from configuration through runtime composition.

- If it already creates one shared client Destination, add tests proving exactly which
  services/targets share it and surface a local warning/diagnostic that it is linkable and
  excluded from the qualified anonymity profile.
- If the config type exists but runtime composition does not actually honor sharing, either
  implement one bounded group owner through the same lifecycle machinery or fail such
  configuration explicitly until a dedicated follow-up. Do not leave a documented policy
  with different runtime semantics.
- Never let an implicit capacity fallback enter a shared group.

### WP5 — Define candidate metadata at the NetDB boundary

Expose only the facts the selected reference policy needs, for example:

- RouterHash;
- validated family identifier when present;
- normalized routable transport-address network bucket(s) when source-derived policy uses
  them;
- eligibility/ban/capability facts already owned by NetDB.

Do not copy entire RouterInfo objects into the tunnel selector. Do not retain peer IP
addresses in anonymity evidence; tests may use synthetic addresses.

Missing optional metadata must have explicit semantics: reject for a rule that requires the
fact, or mark the candidate unqualified according to the frozen policy. It must not silently
act as "different."

### WP6 — Add one Destination peer-selection owner

Implement the selector above path construction, preferably in the Destination/client policy
layer with a narrow candidate-provider trait supplied by daemon/NetDB composition.

Required behavior:

- bounded candidate input and selection attempts;
- duplicate-hash exclusion;
- frozen reference-backed family/network exclusions;
- any frozen inbound/outbound endpoint-reuse rule;
- CSPRNG selection among eligible candidates in production;
- deterministic injected RNG in tests;
- typed per-candidate rejection reason counters;
- typed terminal `InsufficientDiversity` / `NoCandidates` (or local equivalents);
- no automatic hop-count reduction or policy disablement.

`ShortBuildPath::validate` keeps the duplicate-router structural check as defense in depth.

### WP7 — Integrate with service-Destination pool construction

Wire the selector into the actual three-hop service Destination tunnel build path. Prove
that the path consumed by build-state creation came from the selector and that an
insufficient-diversity terminal prevents build submission.

Do not change exploratory-router pool policy in this plan unless the same existing owner is
unavoidably shared; if so, preserve exploratory semantics explicitly and test both policy
profiles.

### WP8 — Add bounded diagnostics and privacy-safe evidence

Expose counters/categories only:

- target scopes current/created/reused/drained/capacity-rejected;
- cross-target fallback count (must remain zero);
- candidate offered/rejected by reason;
- selector insufficient-diversity/no-candidate terminals;
- final path hop count and duplicate/family/network-policy result as synthetic test facts.

Never export Destination private identities or real peer address material.

## 8. Failure, cancellation, restart, and contention semantics

### Target scopes

- Creation is transactional: no map/index entry becomes visible until the Destination
  runtime and required driver/index ownership are ready.
- Concurrent first connections to the same target coalesce on one creation; they must not
  race into two independently published identities.
- Failure rolls back every partially installed index/permit.
- Active scope refcounts prevent idle reclamation.
- Service-generation cancellation marks children draining, refuses new acquisitions, drains
  active connections under existing service deadlines, then removes drivers/indexes/runtime
  state.
- Daemon restart creates new client target identities. Server identities retain their
  existing persistence behavior.

### Path selection

- Candidate enumeration and path selection are bounded by explicit count/attempt ceilings.
- Cancellation aborts before build submission and drops temporary candidate metadata.
- Insufficient diversity is a terminal typed result for that build request.
- No fallback may lower hop count, reuse an excluded family/network peer, or bypass the
  selector.
- Contending pool replenishments share the existing build-concurrency/resource owners rather
  than spawning an independent retry loop.

## 9. Compatibility and migration

Dedicated Generic/IRC client behavior and every server service remain unchanged.

Dedicated HTTP/SOCKS behavior intentionally changes from one service identity across many
remote Destinations to target-scoped identities. This strengthens the documented isolation
goal but can change what a remote service observes as the client Destination when the user
moves between sites. Same canonical remote Destination reuse remains stable for the lifetime
of its target scope.

No config migration should be required for the qualified dedicated behavior. Explicit
`SharedClientGroup` remains an opt-in linkable mode and must be documented as such.

No persisted client identity format is introduced. Existing server-Destination storage
formats must remain byte-compatible.

## 10. Required tests

### Target isolation

- two different remote Destination hashes produce different client identities;
- two aliases resolving to the same hash reuse one target scope;
- two ports on the same remote Destination reuse one target scope;
- concurrent first acquisition for one target yields one child identity;
- target A cannot receive target B's Destination id/driver/index;
- capacity exhaustion fails closed with zero cross-target fallback;
- idle reclamation never removes an active scope;
- generation reconcile/drain cancels every child exactly once;
- daemon restart rotates client target identities;
- server Destination persistence is unchanged;
- explicit shared-group semantics are tested truthfully.

### Path diversity

- duplicate router rejected both by selector and final path validation;
- same-family candidates rejected when the frozen policy requires it;
- same-network candidates rejected using the exact frozen network rule when applicable;
- missing family/address metadata follows the documented fail-closed/unqualified semantics;
- inbound/outbound endpoint reuse follows the frozen policy;
- deterministic seeded RNG selects only eligible peers;
- candidate exhaustion returns the typed terminal with no build submission;
- no shorter-path fallback occurs;
- ordinary eligible fixtures still build three-hop service paths.

### Composition/regression

- tunneled client NetDB lookup/publication still uses the owning target Destination;
- NetDB namespace/provenance tests remain green;
- service-boundary leak checker remains green;
- fixed-target Generic/IRC and persistent server service tests remain green.

## 11. Exact verification commands

The implementation should run the repository floor plus focused lanes equivalent to:

```bash
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1
cargo test --locked -p i2pr-client --all-targets -- --test-threads=1
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-service-anonymity-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-dependency-direction.sh
```

If a new selector/target-scope static checker is added, its seeded-negative self-check is
also mandatory.

## 12. Documentation updates

- Add the exact target-scope lifecycle/resource contract near the service-tunnel docs.
- Document `SharedClientGroup` as explicit linkability and state its actual runtime
  semantics.
- Record the exact-pinned reference diversity source matrix and selected coherent policy.
- Update the anonymity roadmap/registry at closure.
- Do not broaden `docs/security-model.md` beyond Plan 296's existing narrow boundary
  statement until integrated qualification eventually passes.

## 13. Acceptance criteria

Plan 305 passes only if all are true:

1. Dedicated HTTP/SOCKS connections to distinct canonical Destination hashes use distinct
   client Destination identities.
2. Same-hash aliases/ports reuse the same bounded live target scope.
3. Target-scope capacity/cancellation/reconcile paths are bounded and fail closed with zero
   cross-target identity fallback.
4. Explicit sharing semantics are truthful, tested, and marked unqualified/linkable.
5. Client target identities remain ephemeral and server persistence is unchanged.
6. A single candidate metadata/provider + Destination peer selector owner exists before
   `ShortBuildPath`.
7. The selector enforces every diversity dimension supported by the frozen coherent
   reference policy and returns typed scarcity outcomes.
8. Three-hop service paths do not silently degrade under scarcity.
9. Plan 300's repeated-router guard remains active.
10. NetDB namespace/tunneled-client invariants and the full workspace/security floor remain
    green.
11. No broad path/anonymity qualification claim is added.

## 14. Stop conditions

Stop and write the exact architectural boundary if:

- exact-pinned reference source does not support a proposed family/network rule;
- required family/network metadata is not available from validated RouterInfo without a
  separate NetDB architecture change;
- target-scoped composition would require persisting client private identities;
- the only implementation route creates an independent Destination registry/tunnel stack;
- `DestinationPolicy` public semantics require a breaking redesign rather than an internal
  lifecycle refinement;
- resource accounting cannot place a hard bound on child Destination/tunnel creation;
- a scarcity fallback would be required to preserve availability.

Do not invent a prefix rule, silently reinterpret missing metadata, or trade isolation for
availability.

## 15. Closure evidence required

The closure record must include:

- implementation commit(s);
- exact reference source matrix for diversity policy;
- target-scope state/lifecycle diagram and chosen hard ceilings with rationale;
- deterministic target-isolation test matrix;
- explicit shared-group behavior evidence;
- deterministic path-selector rejection/exhaustion matrix;
- proof that selector output feeds actual service-Destination build submission;
- server-persistence/fixed-target regression results;
- NetDB namespace/tunneled-client regression results;
- routine workspace/clippy/docs/boundary command results;
- finding severity table;
- unblock audit for a fresh Plan-300 qualification successor.

## 16. Handoff

On pass, register a fresh corrective successor to Plan 300 that executes the target-isolation
and path-diversity qualification matrix using these production owners. Do not edit Plan
300's stopped record. Plan 301 remains blocked until the HTTP/Streaming branch enabled by
Plan 304 and the Destination/path branch enabled by this plan have each produced passing
qualification successors.
