# Plan 310 — Destination-group multi-hop pool owner and peer selection

Status at registration: **blocked-on-plan309**

Classification: production architecture + anonymity capability.

Hard dependencies: Plan 309 passed; ADR 0030; retained Plan 305 reference-diversity matrix.

## 1. Objective

Make each Destination group own real multi-hop inbound/outbound client tunnel pools and a bounded peer-selection pipeline, replacing the current service provisioning path that submits one `PeerBuildMaterial` and constructs a one-hop `ShortBuildPath` despite a configured three-hop service profile.

## 2. Why ready after Plan 309

Plan 305 proved the current one-peer build owner cannot support path diversity. Plan 309 supplies the missing group lifecycle owner. This plan connects validated NetDB candidates to that owner and feeds complete paths into the existing short-build machinery.

## 3. Current implementation evidence

- `DestinationConfig::service_compatibility_profile()` requests three hops.
- `provision_all_service_router_material` currently receives one `RouterPeerMaterial`.
- `BuildRequest` owns one `PeerBuildMaterial`.
- `ExploratoryBuildCoordinator::submit` currently creates `hops: vec![request.peer.hop_spec()]`.
- `ShortBuildPath::validate` already rejects a repeated RouterHash.
- Plan 305 retained the exact Java/i2pd diversity source matrix.

## 4. Invariants

1. Three-hop qualified service paths are real, not configuration-only.
2. No RouterHash repeats within one path.
3. Candidate facts originate in validated NetDB.
4. Selection is bounded and cryptographically randomized in production.
5. Scarcity never silently shortens the path or disables diversity.
6. Group traffic, LeaseSet lookup, and publication use group-owned pools rather than exploratory/router shortcuts.
7. Router identity is neither the group Destination identity nor an implicit service-hop substitute.
8. Exploratory-router pool semantics remain separately owned.

## 5. Scope

In scope: multi-peer Destination build request surface; candidate provider; Java-derived client path profile retained from Plan 305; group inbound/outbound pool build/replenishment/expiry; path installation; group LeaseSet publication and lookup routing; typed selection exhaustion; deterministic fixtures.

Out of scope: router exploratory peer policy changes, service lifecycle timing smoothing, Streaming tuning, and public-network anonymity claims.

## 6. Reference profile

Use the exact Plan 305 source matrix as the initial coherent path policy:

- unique RouterHash;
- Java client family-label exclusion semantics, explicitly noting those family labels are not certificate-verified in that reference;
- IPv4 /16 and IPv6 /32 address-proximity exclusion where validated transport metadata is available;
- advertised-port/role exclusions supported by the source matrix;
- no shorter-path result on insufficient eligible peers.

Do not blend i2pd /24 and /56 behavior into this profile. A future ADR may change the coherent target.

## 7. Required production changes

1. Add a multi-hop Destination build request carrying a bounded ordered peer vector; do not ambiguously overload exploratory one-peer semantics.
2. Add a narrow NetDB candidate projection with only RouterHash, build key, eligibility, and reference-required family/network/port facts.
3. Add `DestinationPeerSelector` with deterministic injected RNG in tests and CSPRNG in production.
4. Select complete inbound/outbound paths before build submission.
5. Extend the Destination-group pool owner to replenish, expire, replace, and resource-account multi-hop tunnels.
6. Feed complete peer vectors to `ShortBuildPath` and retain its repeated-router defense.
7. Publish LeaseSets from usable inbound group tunnels and send client lookups/publication through group outbound tunnels.
8. Add typed `NoCandidates` / `InsufficientDiversity` outcomes and aggregate counters without peer identities in diagnostics.

## 8. Ordered work packages

WP1 multi-peer build contract; WP2 NetDB candidate projection; WP3 selector; WP4 group pool construction; WP5 LeaseSet/lookup/publication integration; WP6 expiry/replenishment/resource accounting; WP7 deterministic qualification fixtures.

## 9. Failure, cancellation, restart, and contention

Candidate enumeration and selection are hard-bounded. Build concurrency uses existing resource governors. Cancellation before submit drops candidate state; cancellation after submit uses the existing build owner semantics. Insufficient diversity is a terminal build attempt, not a reason to create a one-hop fallback.

## 10. Compatibility and migration

No service config migration beyond Plan 309. Existing exploratory builds preserve their semantics. Destination-group identity persistence is unchanged.

## 11. Required tests

Three-hop inbound/outbound construction; no repeat; family and network exclusion; missing-metadata semantics; scarcity terminal; no one-hop fallback; deterministic selection; concurrent pool replenishment; LeaseSet contains real inbound leases; group lookup/publication uses group tunnels; distinct groups have distinct pools; shared services consume the same group pools.

## 12. Exact verification commands

Run the full workspace floor plus focused `i2pr-netdb`, `i2pr-tunnel`, `i2pr-client`, and `i2pr-daemon` suites and the destination/netdb/service anonymity boundary scripts.

## 13. Documentation updates

Update service/anonymity architecture diagrams and remove current prose that implies `DestinationConfig::length_hops` alone proves actual service path length.

## 14. Acceptance criteria

Production service builds contain the configured three hops; selector output is the path actually submitted; scarcity is typed and fail-closed; group LeaseSet/publication/lookup use group pools; no router-local identity shortcut exists; all regressions are green.

## 15. Stop conditions

Stop if validated NetDB cannot supply required candidate facts, if multi-hop requests would require a second build stack, or if path correctness requires relaxing existing tunnel crypto/wire invariants.

## 16. Closure evidence required

Reference-matrix lock, candidate fixture matrix, real submitted-path tests, group pool lifecycle evidence, exact command results, and unblock audit for Plans 311/312.

## 17. Handoff

On pass, Plans 311 and 312 become dependency-ready and may proceed in parallel.
