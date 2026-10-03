# Plan 314 — Plan 310 corrective: production multi-hop build contract, NetDB selector, and deterministic three-hop proof

Status at registration: **registered-plan310-multihop-build-corrective-ready**

Classification: production architecture + anonymity capability foundation.

Hard dependencies: Plan 309 passed; Plan 310 blocked closure; ADR 0030; retained Plan 305 reference-diversity matrix.

This plan does **not** depend on Plan 308, a public I2P network, or a live multi-router i2pd testnet.

## 1. Objective

Correct the first half of blocked Plan 310 by making the production Destination build path accept a complete bounded multi-hop path selected from validated NetDB candidates, and prove that an exact three-hop inbound/outbound path traverses the existing production short-build wire/cryptographic machinery.

This plan ends at established multi-hop material plus a typed handoff to the Destination-group pool owner. Plan 315 owns installation/replenishment/LeaseSet and service-traffic consumption of those pools.

## 2. Why this corrective is needed

Plan 310 stopped at a real architecture defect:

- `ServiceProduct` bootstraps one `RouterPeerMaterial`;
- `BuildRequest` owns one `PeerBuildMaterial`;
- `ExploratoryBuildCoordinator::submit` constructs `hops: vec![request.peer.hop_spec()]`;
- `DestinationConfig::service_compatibility_profile()` nevertheless declares three hops.

The failed pass also risked conflating two different evidence questions:

1. whether i2pr selects, encodes, cryptographically processes, and establishes a correct three-hop path; and
2. whether three independent external routers can be assembled into a controlled live I2P topology.

Only the first is required to correct this production boundary. A live three-router topology additionally requires independently reachable RouterInfos, NetDB bootstrap/peer knowledge, transport sessions, and forwarding between several routers. That is reusable interoperability infrastructure, not a prerequisite for proving the production build contract.

Per `plans/README.md`, the blocked Plan 310 record remains historical authority. This corrective does not rewrite it.

## 3. Existing implementation evidence

The implementation agent must re-confirm these facts on its baseline:

- `i2pr-tunnel::ShortBuildPath` already owns `Vec<HopSpec>`, bounds the hop count, rejects repeated RouterHash values, enforces intermediate tunnel-id continuity, and validates direction-specific role topology.
- `ShortBuildStateMachine` already constructs/cryptographically processes multi-record short build messages.
- Existing deterministic tests drive two-hop inbound and outbound trajectories through the real per-hop `MessageHopProcessor` path and back to `Established`.
- `RouterInfoStore` accepts only `ValidatedRouterInfo` and exposes bounded canonical iteration.
- `DestinationTunnelCoordinator` already owns the authoritative bounded RouterInfo store used by Destination NetDB operations.
- `tests/integration/anonymity/reference-diversity-matrix.md` freezes the selected Java-derived service-path policy: unique RouterHash, Java family-label exclusion semantics, IPv4 /16 and IPv6 /32 proximity exclusions where metadata exists, role/port exclusions supported by that source review, and fail-closed scarcity.
- `DestinationTunnelPool` already exists, but group ownership and consumption of established material are deferred to Plan 315.

## 4. Evidence decision

Plan 314 closure MUST NOT require a live external three-hop path.

Counted Plan 314 evidence is deterministic/local but must execute the same production types and cryptographic state machine used by live builds:

- synthetic routers must be real signed `RouterInfo` values validated through `ValidatedRouterInfo`;
- selected peers must enter the production Destination build request, not a test-only path builder;
- three-hop trajectory tests must use `ShortBuildStateMachine`, the production short-build cryptography, and the ordinary per-hop processor/reply path;
- established material must be extracted from the ordinary state-machine result.

A fixture that directly fabricates `EstablishedMaterial`, bypasses selection, bypasses `ShortBuildStateMachine`, or simply asserts `length_hops == 3` is not closure evidence.

A future isolated multi-router i2pd network may add interoperability confidence, but absence of that infrastructure cannot block this plan.

## 5. Invariants

1. Qualified service paths contain exactly the configured three remote hops; no one- or two-hop fallback is allowed.
2. No RouterHash repeats within one path.
3. Production candidate facts originate only from validated NetDB state.
4. The selected coherent Java-derived diversity profile remains exactly the retained Plan 305 matrix; do not mix i2pd /24 or /56 rules into it.
5. Missing metadata follows an explicit fail-closed/unqualified policy; it never silently means "different".
6. Selection is bounded and cryptographically randomized in production; deterministic injected randomness is test-only.
7. Exploratory one-peer semantics remain separately named and unchanged.
8. There is one short-build wire/cryptographic engine. Destination support may have a distinct policy/request owner, but it must not fork or duplicate the short-build implementation.
9. Candidate scarcity terminates before build submission with typed `NoCandidates` / `InsufficientDiversity`-class outcomes.
10. Runtime diagnostics expose counts/categories only, never peer RouterHashes, addresses, family strings, or build keys.
11. No public-network or production-anonymity claim follows from this deterministic proof.

## 6. Required architecture

Target composition:

~~~text
Destination group requests build
        |
        v
authoritative DestinationTunnelCoordinator RouterInfoStore
        |
        | bounded validated projection
        v
DestinationPeerCandidate[]
        |
        v
DestinationPeerSelector
  Java-derived diversity policy
  CSPRNG production / seeded RNG tests
        |
        v
QualifiedDestinationPath { exactly 3 ordered peers }
        |
        v
DestinationBuildRequest
        |
        v
shared short-build attempt core
  ShortBuildPath -> ShortBuildStateMachine
        |
        v
EstablishedMaterial
        |
        +----> Plan 315 DestinationGroup pool owner

ExploratoryBuildRequest (one peer)
        |
        +----> same shared short-build attempt core
~~~

Do not expose the mutable RouterInfo store to service-tunnel code. Prefer a narrow bounded candidate projection owned at the daemon/NetDB composition seam.

## 7. Required production changes

### WP1 — Separate request semantics without duplicating the build stack

Refactor the current single-peer build surface so exploratory and Destination requests are explicit.

Acceptable shape:

- retain a one-peer `ExploratoryBuildRequest` or equivalent;
- add a `DestinationBuildRequest` carrying a bounded ordered peer vector;
- extract one private/shared submission core that constructs and owns `ShortBuildPath`, `ShortBuildStateMachine`, delivery, pending-attempt state, reply routing, cancellation, and established-material extraction.

Do not merely change the exploratory request from one peer to `Vec<PeerBuildMaterial>` and thereby blur its policy semantics. Do not create a second short-build codec/crypto/state machine.

The Destination request must validate exact hop count before submission and must preserve the selector's order byte-for-byte/element-for-element into `ShortBuildPath.hops`.

### WP2 — Add a bounded validated NetDB candidate projection

Add the narrow facts required by the retained Java profile, sourced from `ValidatedRouterInfo` records in the authoritative `DestinationTunnelCoordinator` store.

The projection should contain only what selection/build requires, such as:

- RouterHash;
- static build encryption public key;
- validated/parsed family label when available under the selected profile;
- normalized IPv4 /16 and IPv6 /32 buckets derived from validated advertised transports;
- advertised ports/role facts required by the retained matrix;
- eligibility facts already owned by NetDB.

Do not copy a second RouterInfo store into service-product or service-tunnel state. Do not return mutable store access. Candidate enumeration must have a hard maximum even though the underlying store is already bounded.

### WP3 — Implement `DestinationPeerSelector`

Implement one selector above path construction.

It must:

- request exactly the configured service hop count;
- reject duplicate RouterHash;
- apply the frozen Java family/network/role/port rules;
- define explicit missing-metadata behavior;
- use a CSPRNG in production;
- accept deterministic RNG injection in unit/integration tests;
- bound candidate scans and selection attempts;
- return a complete ordered path or a typed terminal;
- never reduce hop count or disable a rule on scarcity.

The selector may retain aggregate rejection counters by category. It may not retain/export peer identities in diagnostics.

### WP4 — Remove the single-reference peer as the service-path owner

`dial_and_bootstrap_router_only` may continue authenticating a configured reference and inserting its validated RouterInfo into the authoritative store for existing interop purposes, but its returned one-peer `RouterPeerMaterial` must no longer define the qualified service path.

Destination build preparation must consume the candidate provider/selector result from the authoritative store.

If a constrained external fixture contains only one eligible router, the qualified three-hop request must return typed insufficient diversity. Do not preserve historical M10 one-peer behavior by silently shortening the anonymity-qualified profile. If a legacy external test requires one-peer semantics, keep that test explicitly outside Plan 314 anonymity evidence and behind an already-supported or narrowly test-only interop profile; do not change the qualified service default.

### WP5 — Prove exact three-hop short-build trajectories deterministically

Generalize the existing deterministic two-hop `short.rs` trajectory helpers to arbitrary bounded hop counts and add strict three-hop rows.

Required role sequences:

- outbound: Participant, Participant, OutboundEndpoint;
- inbound: InboundGateway, Participant, Participant.

For each direction:

1. create three distinct deterministic router identities/static X25519 keys;
2. construct an exact valid tunnel-id chain;
3. prepare through `ShortBuildStateMachine`;
4. run every real hop through the production per-hop message processor with accepted responses;
5. feed the resulting reply through the ordinary reply postprocessor;
6. require terminal `Established`;
7. extract established material;
8. assert direction, exact hop count, exact ordered RouterHashes, role topology, and first-hop routing metadata.

These tests prove the multi-hop wire/cryptographic path without requiring sockets or an external router.

### WP6 — Prove selector-to-submission continuity

Add a daemon-level deterministic test using validated synthetic RouterInfos in the authoritative candidate source.

The test must prove:

- the selector chooses exactly three eligible peers;
- their order is the order supplied to the Destination build request;
- the path submitted to the shared short-build core contains those same three peers;
- `Installed` / established output reports three hops;
- replacing one candidate with a same-family, same-network, duplicate, missing-required-metadata, or otherwise excluded candidate produces the documented typed result;
- insufficient diversity causes zero build submission.

A test-only observation may expose hop count and synthetic fixture indices inside the test module, but production diagnostics must not expose real peer identities.

## 8. Failure, cancellation, restart, and contention

Candidate enumeration and selection are bounded before submission. Cancellation before submit drops temporary candidate state. Cancellation after submit uses the shared build-attempt owner and must zero/drop cryptographic context exactly as current short-build cancellation does.

Pending build limits remain authoritative and shared; Destination work must not create an independent unbounded retry queue. A process restart loses ephemeral selection/build attempts and reconstructs candidates from validated NetDB state.

Typed scarcity is an ordinary terminal attempt, not a reason to mutate policy.

## 9. Compatibility and migration

No persistent format or public service configuration migration is required.

Exploratory tunnel behavior remains one-peer at its request boundary. The qualified service profile becomes truthful: when configured for three hops, production submits three hops or fails closed.

Historical external one-peer service fixtures are not anonymity evidence. If they cannot operate after this correction, preserve them only through an explicit non-qualified interop mode rather than weakening the three-hop profile.

No new dependency is expected.

## 10. Required tests

At minimum:

- validated RouterInfo candidate projection;
- projection bound/capacity;
- build-key extraction from validated identity;
- Java-family exclusion;
- IPv4 /16 exclusion;
- IPv6 /32 exclusion;
- port/role rules present in the retained matrix;
- missing-metadata semantics;
- deterministic seeded selection reproducibility;
- CSPRNG production constructor/type path;
- exact-three selection;
- duplicate rejection;
- scarcity/no-candidate typed terminals;
- no shorter-path fallback;
- outbound strict three-hop cryptographic trajectory;
- inbound strict three-hop cryptographic trajectory;
- established-material hop-order round trip;
- selector-to-submission exact continuity;
- zero submission on selector failure;
- exploratory one-peer regression.

## 11. Exact verification commands

Run, at minimum:

~~~text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
cargo test --locked -p i2pr-client --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
bash scripts/check-service-anonymity-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-dependency-direction.sh
git diff --check
~~~

No external i2pd command is required for Plan 314 closure.

## 12. Documentation updates

Document the distinction between:

- configuration intent;
- deterministic proof that the production selector/build engine constructs real three-hop established material; and
- later external multi-router interoperability.

Update architecture prose that still describes service Destination material as originating from one `ExploratoryBuildCoordinator` peer.

Do not claim public-network anonymity or deployed-path diversity.

## 13. Acceptance criteria

Plan 314 passes only if all are true:

1. a separately named Destination multi-hop request exists;
2. one shared short-build wire/crypto engine serves exploratory and Destination attempts;
3. the authoritative validated NetDB store supplies bounded candidate facts;
4. the retained Java-derived selector returns exactly three peers or a typed terminal;
5. selector output is the exact ordered path submitted to `ShortBuildPath`;
6. strict deterministic three-hop inbound and outbound trajectories reach `Established` through production cryptographic processing;
7. established material preserves the exact three-hop topology;
8. insufficient diversity results in zero build submission and no shorter fallback;
9. exploratory one-peer semantics remain intact;
10. the required local/workspace/security floor is green.

This closes only the build-contract/selector portion of blocked Plan 310. It does not close group pool ownership or service consumption.

## 14. Stop conditions

Stop and write a closure record rather than improvising if:

- required Java-profile facts cannot be projected from validated RouterInfo without adding an unvalidated side channel;
- the only implementation path duplicates the short-build crypto/state machine;
- exact three-hop role/forwarding semantics fail existing `ShortBuildPath` validation;
- a required candidate rule would need raw peer address retention outside the bounded selector seam;
- completing this plan would require implementing group pool lifecycle or graceful drain rather than handing established material to Plan 315.

## 15. Closure evidence required

Record:

- production request/core ownership diagram;
- exact candidate projection schema and bounds;
- selector policy matrix tied to the retained Plan 305 source matrix;
- deterministic seeded selection matrix;
- strict three-hop inbound/outbound cryptographic trajectory results;
- selector-to-submission continuity result;
- scarcity/no-submission result;
- exploratory regression;
- exact verification commands/results;
- compatibility/security findings;
- unblock audit for Plan 315 only.

## 16. Handoff

On pass, Plan 315 becomes dependency-ready.

Plans 311 and 312 remain blocked until Plan 315 closes the group-owned pool and Destination-operation integration requirements. Plan 308 remains independent.
