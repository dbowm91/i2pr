# Plan 250 — M11 transit foundation semantic and ownership corrective

Status at registration:
**registered-ready-m11-transit-foundation-semantic-and-ownership-corrective**

Baseline: `958c06171a6d60dc3d1866ed8b7d93937d001d6f`

This corrective supersedes only the completion interpretation of Plan 249. The useful
Plan 249 architecture and execution evidence remain retained.

## Objective

Correct the runtime-neutral M11 transit contract before any daemon/runtime consumer is
written.

Post-closure review found nine defects:

1. `previous_peer` is derived from local `hop_identity` rather than authenticated sender;
2. accepted encrypted replies seal request `m/r/l` instead of response `b`;
3. well-formed admission rejection returns `Err` instead of a sealed code-30 outcome;
4. request creation/expiration validation is oriented incorrectly;
5. pending ceilings are not backed by pending reservation state;
6. secret-owning transit types implement `Clone`;
7. unknown registry removal may panic;
8. required tests use proxy/implicit assertions for several acceptance rows;
9. `layer_state_seed` is a mutable secret argument with no protocol role.

No daemon/router-I2NP wiring belongs in this plan.

## Protocol authority

Use the current ECIES tunnel-creation specification and the existing pinned source set.

Required semantics:

- request time is creation time; the current request expiration is 600 seconds;
- short replies carry response options in the encrypted reply Mapping;
- accepted replies use option `b` when `m` or `r` was requested;
- `b >= m` when `m` exists;
- `b` may be below or above `r`;
- the visible reply byte is limited to accept (0) and bandwidth reject (30).

Do not widen the response vocabulary.

## Invariants

- `i2pr-tunnel` remains runtime-neutral: no Tokio, sockets, filesystem, tasks, or runtime
  synchronization.
- authenticated previous peer and local router identity are distinct typed inputs.
- transit secret owners are move-only across public boundaries.
- no untrusted tunnel id may cause a panic.
- no active registration exists before a successful accepted reply is constructed and
  sealed.
- a valid/authenticated build rejected by local policy returns a canonical sealed code-30
  outcome and zero active state.
- malformed or unauthenticated records fail closed without manufacturing a policy reply.
- stored expiration never extends beyond protocol creation time + 600 seconds.
- no daemon/config/RouterInfo/router.version/public-network change.

## Work package A — authenticated previous-peer provenance

Extend `TransitBuildContext` with caller-supplied authenticated sender identity, e.g.
`previous_peer: TunnelPeer`.

`hop_identity` remains the local router identity used for ECIES record authentication.
Never derive the sender from it.

Use the supplied sender for:

- `TransitAdmissionReservation`;
- per-peer active/pending accounting;
- `TransitHopRegistration.previous_peer`.

Required tests:

1. local identity and previous peer deliberately differ;
2. accepted registration stores exactly the supplied previous peer;
3. two senders have independent accounting;
4. one sender can hit a per-peer ceiling while another remains eligible;
5. no code path substitutes local identity.

Plan 252 will later prove `router_i2np` supplies authenticated transport provenance.

## Work package B — wire outcome correction

Reshape the transaction so a protocol-valid policy rejection is a normal wire outcome,
not a fatal transaction error.

Preferred semantics:

```text
Result<TransitBuildOutcome, TransitFatalError>
```

Equivalent naming is acceptable, but:

- accepted outcome contains a sealed code-0 reply and committed registration;
- rejected outcome contains a sealed code-30 reply, typed internal reason, and zero
  committed state;
- authentication/codec/RNG/seal failures that prevent an honest reply remain fatal errors.

For accepted requests, build `TransitBandwidthReply` first and seal its BuildOptions.
Never seal request-side `m/r/l` as reply options.

Bandwidth allocation:

```text
allocatable = min(available_bandwidth, optional_per_tunnel_cap)
```

- reject only if `m > allocatable`;
- `r > cap` alone is not a mandatory reject;
- if `r` exists, `b` may be `min(r, allocatable)` when any `m` remains satisfied;
- if only `m` exists, `b >= m`;
- if only `r` exists, emit a positive local-policy `b`;
- if neither exists, empty reply Mapping is valid.

For a well-formed local rejection, derive the required reply material, seal code 30, release
the pending reservation, and leave the registry unchanged. The main transaction must return
that sealed reply; Plan 252 must not reconstruct cryptographic context.

Mandatory tests decrypt/open the actual sealed record and decode `ShortReplyRecord`:

6. m-only acceptance -> reply has `b` and no `m/r/l`;
7. r-only acceptance -> reply has `b` and no `m/r/l`;
8. m+r acceptance -> wire `b >= m`;
9. no m/r -> empty accepted Mapping;
10. disabled -> sealed 30, zero state;
11. degraded/shutdown -> sealed 30, zero state;
12. global active full -> sealed 30, unchanged state;
13. per-peer active full -> sealed 30, unchanged state;
14. insufficient m -> sealed 30, zero state;
15. r above local cap but satisfiable m -> accepted with legal `b`.

## Work package C — creation time and expiration

Correct the validity direction.

Required model:

```text
creation = decoded request creation time
expiry   = creation + 600 seconds

reject if creation > now + bounded_future_skew
reject if expiry <= now
otherwise stored expiry = expiry
```

Account for the protocol's minute-rounded creation time without permitting arbitrary
future timestamps. Skew is a plausibility allowance, not added tunnel lifetime.

Tests:

16. current creation accepted;
17. just-inside future skew accepted;
18. beyond future skew rejected;
19. still inside 600-second lifetime accepted;
20. at/after expiry rejected;
21. stored expiry equals creation + 600, not now + 600;
22. repeated validation cannot extend lifetime.

## Work package D — real pending reservations

Introduce a bounded runtime-neutral admission-state owner for pending work. Exact names may
vary, but it must expose:

- global pending count;
- per-peer pending count;
- opaque reservation token;
- reserve before bounded reply derivation/sealing;
- release on every reject/fatal path;
- commit/release transition after accepted registry insertion;
- bounded peer accounting;
- no Clone on live reservation tokens.

Do not use active registry count as pending count.

Tests:

23. real outstanding reservations hit global pending ceiling;
24. real outstanding reservations hit per-peer pending ceiling;
25. one peer does not consume another peer's per-peer limit;
26. policy reject releases pending;
27. RNG failure releases pending;
28. seal failure releases pending;
29. registry conflict releases pending;
30. accepted commit returns pending to baseline;
31. double release/commit cannot underflow.

## Work package E — secret ownership and panic-free registry

Remove `Clone` from all public types that own or transitively own `LayerKeys`, including:

- `TransitHopRole`;
- `TransitHopRegistration`;
- `TransitRegistry`;
- new live reservation/accepted-state owners.

Borrow layer keys for reply sealing instead of cloning them.

Remove the unused `layer_state_seed` transaction argument unless a concrete protocol role
is proven. Do not keep unused secret material for API compatibility.

Make unknown removal non-panicking. Preferred API:

```text
fn remove(...) -> Result<TransitHopRegistration, TransitRegistryError>
```

Use `UnknownReceiveTunnelId` if `Result` remains the contract.

Add `scripts/check-m11-transit-boundaries.sh` (or equivalent small guard) to reject:

- Clone derives on secret-owning public transit types;
- panic/expect in unknown-id removal;
- runtime imports in `transit.rs`.

Tests/guards:

32. unknown removal returns typed failure;
33. successful removal returns owned state exactly once;
34. Debug does not expose keys;
35. outcome Debug does not dump the 218-byte encrypted reply;
36. static boundary guard is green.

## Work package F — replace proxy Plan 249 tests

Directly prove the original rows that were not actually covered:

- #21 global pending full;
- #23 per-peer pending full;
- #29 participant exact receive/next/previous-peer tuple;
- #30 real IBGW acceptance and role registration;
- #31 exact OBEP role state;
- #33 deterministic seal-failure injection + rollback;
- #36 actual previous-peer mismatch through the registered role;
- #37 actual duplicate-cell rejection through the registered role.

The closure matrix must map every corrective requirement to an actual test name.
"Implicitly covered" is not acceptable for these rows.

## Out of scope

- `router_i2np.rs` composition;
- daemon actors/locks/tasks;
- transport/network delivery;
- config schema and metrics;
- capability/version advertisement;
- external i2pd execution;
- Java source-lock CI hygiene (Plan 251);
- public transit/floodfill;
- legacy build formats.

## Failure, restart, contention

No failure after pending reservation may leak pending count.
No failure before accepted commit may create active state.
Accepted commit leaves pending at baseline.

Transit state remains ephemeral; restart starts with empty active/pending state.

Do not add synchronization primitives. Plan 252 will choose runtime ownership after this
single-owner contract is stable.

## Compatibility and migration

No disk migration or dependency addition. This is an experimental, non-advertised Rust API;
update workspace consumers atomically and do not retain compatibility shims for incorrect
Plan 249 semantics.

## Verification

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-tunnel -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-m11-transit-boundaries.sh
```

The workspace test command may remain red only for the separately registered Plan 251
Java source-lock environment defect. Plan 250 closure must distinguish focused transit
truth from repository-wide CI truth.

Plan 252 must not be registered until Plans 250 and 251 are both closed and ordinary CI is
green.

## Documentation on implementation

Update Plan 249 current authority, Plan 250 closure, transit roadmap, registry,
`specs/support.toml`, and `specs/protocols/05-tunnels.md`. Register Plan 252 only after the
unblock audit confirms Plan 250 + Plan 251 + green ordinary CI.

## Acceptance criteria

Plan 250 closes only when direct evidence proves:

1. caller-supplied authenticated previous peer is used;
2. accepted encrypted replies contain correct `b` semantics;
3. valid policy rejects return sealed code 30 with zero state;
4. expired/future requests are handled in the correct direction with no lifetime extension;
5. global/per-peer pending ceilings use real outstanding reservations;
6. every reservation terminal path returns to baseline;
7. public secret owners are move-only;
8. unknown removal is panic-free;
9. unused secret-seed API is removed or justified by protocol;
10. repaired direct tests replace the proxy rows;
11. no daemon/config/advertisement/public-network change lands;
12. focused fmt/check/test/clippy/doc/deny/boundary evidence is green.

M11 capability remains unclaimed.

## Stop conditions

Stop and write a narrower follow-up if code-30 reply construction needs a missing crypto
primitive, canonical reply decryption cannot be tested, pending safety requires runtime
synchronization, previous-peer correction exposes a deeper role mismatch, Date/expiration
codec semantics contradict the official model, or a new dependency would be required.

## Handoff for smaller models

Write failing tests first for:

1. distinct previous-peer provenance;
2. encrypted accepted `b` round trip;
3. encrypted rejected code-30 round trip;
4. time boundaries;
5. actual pending reservations;
6. non-panicking remove.

Then minimally change the public transit contract. Do not start in `i2pr-daemon` and do not
preserve the incorrect Plan 249 API for compatibility.
