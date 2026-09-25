# Plan 253 — M11 live daemon transit and data-plane corrective

Status at registration:
**registered-ready-m11-live-daemon-transit-data-plane-corrective**

Baseline:
`c94038cfde953051da1a68a1ab29342727daa58b`

Corrects the completion interpretation of Plan 252 while retaining the useful Plan 252
runtime-neutral full-message ShortTunnelBuild implementation.

## Objective

Finish the actual daemon/runtime transit composition that Plan 252 intended to prove before
any exact-pinned i2pd qualification begins.

Plan 252 successfully landed the runtime-neutral full-message STBM processor. Its daemon
closure, however, over-claimed several behaviors that are still scaffolding or absent in
production:

1. `TransitIngressGate` / `TransitBuildService` are not called by the live authenticated
   SSU2/router-I2NP owner;
2. established `TunnelData` forwarding uses a placeholder that returns cell data unchanged
   instead of the registration's derived layer/IV keys;
3. duplicate/replay handling is claimed but the daemon placeholder does not use the
   canonical `DuplicateWindow` role machinery;
4. OBEP `TunnelData` is dropped instead of applying the final layer and delivery/reassembly
   semantics;
5. IBGW data-plane input is not represented role-correctly;
6. code-30 outcomes discard decoded routing metadata and the delivery helper deliberately
   drops them;
7. build delivery currently passes transformed **body bytes** to `RouterDeliveryService`
   instead of proving construction of the complete I2NP ShortTunnelBuild /
   OutboundTunnelBuildReply message with the required message id / routing semantics;
8. accepted registration rollback only handles the pre-delivery `NoActiveSession` error,
   not `QueueFull`, `ResourceDenied`, `TooLarge`, `DeadlineElapsed`, or `Cancelled`;
9. `cancel()` marks the service cancelled but does not drain active registrations/secrets,
   and `expire()` becomes a no-op after cancellation;
10. the peer index is an unconstrained `BTreeMap` despite being documented as bounded;
11. `TransitHopMaterial` owns private X25519 key material but is cloneable;
12. several Plan 252 tests assert routing metadata or test names rather than the required
    production behavior.

Plan 253 is the corrective authority for those defects.

The exact-pinned i2pd qualification moves to unregistered Plan 254.

## Classification

**Infrastructure corrective.**

This plan must establish a real, disabled-by-default, controlled daemon consumer of the
already-correct full-message build transaction and the canonical transit data plane. It
does not advertise transit capability, enable public participation, or claim M11
interoperability.

## Why the original verification missed the defects

Plan 252's runtime-neutral tests were strong, but the daemon tests were almost entirely
module-local. They instantiated `TransitBuildService` directly rather than exercising the
owner that consumes live `Ssu2InboundI2np` events.

Several tests also verified only a subset of their requirement:

- the TunnelData "success" test asserted next router/tunnel ids but not transformed bytes;
- replay/duplicate claims were not driven through canonical duplicate-window state;
- code-30 tests asserted transformed payload existence, not delivery/routing;
- the cancellation test asserted future dispatch failure but never asserted registry drain;
- the delivery rollback test exercised only `NoActiveSession`;
- creator-correlation tests did not prove the live ingress ordering;
- the peer-index test proved only that a fresh map starts empty.

Plan 253 must replace those proxy assertions with direct state/wire/owner tests.

## Canonical invariants

### Preserve the good Plan 252 build core

- `i2pr_tunnel::process_short_build_message` remains the single production message-level
  build cryptographic transaction.
- The local request is opened once and keys are derived once.
- The daemon never receives `replyKey`, `LayerKeys`, Noise state, or decrypted build
  plaintext.
- The daemon never reseals a build reply or applies the multirecord ChaCha transform.
- Accepted registration commits only after complete transformed-message construction.
- Valid local policy rejection returns the transformed code-30 record set with zero
  registration.
- Malformed/unauthenticated/ambiguous-slot input fails closed.

### Transit data-plane secrets stay in i2pr-tunnel

Do not repair TunnelData by copying `LayerKeys` into the daemon or by writing another AES
transform helper there.

The runtime-neutral tunnel crate must own the mutable established transit data-plane state
required to process cells:

- layer/IV keys;
- expected receive tunnel;
- authenticated previous peer;
- bounded duplicate/replay window;
- exact expiry;
- next-hop information;
- OBEP bounded reassembly state where required.

Prefer factoring/reusing the existing `roles.rs` primitives
(`TunnelLayerTransform`, `DuplicateWindow`, participant role state, OBEP parser/reassembler,
IBGW builder) over introducing a parallel implementation.

### Role-correct data plane

The corrective must model each selected role according to its actual established input:

- **Participant**: authenticated inbound `TunnelData` -> receive-id/previous-peer/expiry/
  replay check -> canonical participant layer transform -> one next-hop `TunnelData`.
- **OBEP**: authenticated inbound `TunnelData` -> receive-id/previous-peer/expiry/replay
  check -> final participant-layer transform -> tunnel-message parse/reassembly ->
  `RouterDeliveryAction` (LOCAL/ROUTER/TUNNEL) when complete. Do not forward OBEP input as
  another TunnelData cell.
- **IBGW**: accepts the role-appropriate `TunnelGateway` input addressed to its receive
  tunnel, applies the canonical gateway/tunnel-message construction and first layer, and
  emits one or more next-hop `TunnelData` cells. Do not pretend IBGW is a normal
  TunnelData participant.

If a narrower M11 role subset is chosen instead, stop and update the roadmap/claim before
implementation; do not silently keep tests that imply all three roles.

### Authenticated provenance

- Previous peer comes only from authenticated `Ssu2InboundI2np::peer`.
- Build request next-router / next-tunnel fields never establish the previous peer.
- TunnelData processing must compare the authenticated peer against the registration's
  stored previous peer before any transform or replay-window mutation.

### Bounded ownership

- No per-cell or per-build detached task.
- Every queue/index/window/reassembly structure has an explicit hard capacity.
- No silent eviction of an unexpired registration or replay token.
- Transit remains disabled by default and unadvertised.
- No public config surface is required merely to exercise the controlled qualification
  lane.
- Secret-owning persistent material is move-only unless a documented cryptographic
  ownership reason requires otherwise.

## Work package A — narrow Plan 252 authority and write failing regressions first

Before production changes, add direct failing tests demonstrating the current gaps:

1. live authenticated inbound ShortTunnelBuild does not reach the gate even when the
   controlled owner is enabled;
2. current TunnelData "forward" returns unchanged bytes;
3. exact replay is accepted/forwarded by the placeholder path;
4. OBEP TunnelData is dropped rather than endpoint-processed;
5. code-30 result cannot be routed because the route was discarded;
6. build delivery bytes are not a complete decodable I2NP build message;
7. QueueFull / ResourceDenied / TooLarge / DeadlineElapsed / Cancelled leave accepted
   registration state live;
8. cancellation leaves active registration state live;
9. peer-index insertion can grow past its documented bound.

These tests are part of the corrective evidence and may not be replaced by comments or
static source assertions.

## Work package B — canonical runtime-neutral transit data-plane state

Introduce or refactor a transit data-plane surface in `i2pr-tunnel` so the daemon never
needs raw layer keys.

A preferred shape is one of:

```text
TransitRegistry::process_tunnel_data(...)
TransitRegistry::process_tunnel_gateway(...)
```

or an equivalent mutable role-state API.

Requirements:

- mutate the registration in place; do not clone secret-owning registration state;
- reuse canonical duplicate-token computation/window semantics;
- use exact stored expiry;
- perform previous-peer check before duplicate-window mutation;
- use canonical participant transform;
- support OBEP final-layer parse/reassembly and semantic delivery action;
- support IBGW TunnelGateway -> TunnelData generation if IBGW remains in the M11 role
  claim;
- return typed runtime-neutral outcomes containing only non-secret routing/delivery facts.

If the existing `roles.rs` types cannot be reused without cloning `LayerKeys`, factor their
shared internal state/transform helpers rather than adding another daemon-specific crypto
path.

Required direct tests:

10. Participant transformed bytes match a fixed canonical role/vector, not the input bytes;
11. participant exact replay is rejected on the second call;
12. wrong previous peer fails before replay-window mutation;
13. expiry fails before transform;
14. OBEP final layer yields the expected RouterDeliveryAction for an unfragmented vector;
15. OBEP replay is rejected;
16. OBEP fragmented input uses bounded reassembly and produces one completed action;
17. IBGW role input produces the same next-hop cell(s) as the canonical IBGW primitive;
18. role-mismatched inputs fail closed without state corruption.

## Work package C — preserve route on rejection and construct real I2NP build messages

Do not collapse a valid rejection to `Rejected { reason, payload }` without routing facts.

The daemon-facing build outcome must retain:

- accepted/rejected disposition;
- role-specific `TransitBuildRoute`;
- transformed record set;
- receive tunnel only where a registration actually exists;
- internal rejection reason for local metrics only.

Participant/IBGW code-30:
- forward the transformed record set as the correct ShortTunnelBuild continuation to the
  decoded next router;
- use the decoded next message id in the actual I2NP header/message construction;
- install no registration.

OBEP code-30:
- terminate STBM and emit the transformed records as an
  OutboundTunnelBuildReply using the decoded reply router/message id;
- honor the reply tunnel field using the repository's canonical router/tunnel delivery
  semantics when nonzero;
- install no registration.

Accepted builds follow the same wire construction, with an installed registration.

Do not pass a bare STBM/OTBRM **body** to `RouterDeliveryService` if that service expects a
complete encoded I2NP message.

Required tests decode the exact bytes handed to `RouterDeliveryService`:

19. accepted Participant delivery decodes as ShortTunnelBuild with exact next message id
    and transformed record body;
20. accepted IBGW build continuation does the same;
21. accepted OBEP delivery decodes as OutboundTunnelBuildReply with exact reply id;
22. Participant/IBGW code-30 delivery uses the same correct route/message shape;
23. OBEP code-30 delivery uses the correct OTBRM route/message shape;
24. nonzero OBEP reply-tunnel routing is represented through the canonical tunnel-delivery
    path rather than discarded;
25. no rejection taxonomy leaks into the wire.

## Work package D — wire the gate into the real authenticated ingress owner

The corrective must have a production caller.

Integrate `TransitIngressGate` into the daemon owner that actually consumes
`Ssu2InboundI2np` from the SSU2 runtime.

Requirements:

- ordinary/default product construction leaves the gate disabled;
- a controlled constructor/test/qualification path may enable it without adding a public
  advertised mode;
- ShortTunnelBuild is decoded/classified through the existing canonical router-I2NP
  dispatcher;
- creator-correlated build traffic is resolved before transit admission;
- authenticated ingress peer is passed unchanged;
- accepted/rejected build dispatch is sent through the existing bounded router-delivery
  seam;
- no duplicate decoder or transport loop.

For inbound TunnelData, establish deterministic owner ordering:

1. existing local/creator/service receive-id ownership first;
2. transit registry ownership second when controlled transit is enabled;
3. unknown id fails closed.

No cell may be transformed twice or be offered to both owners after one claims it.

Add one integration test that starts the real controlled SSU2 owner/pump (loopback or
injected transport seam), injects an authenticated inbound STBM, and observes the transit
gate outcome through that owner. Direct calls to `TransitBuildService::route_short_build`
do not satisfy this requirement.

Required tests:

26. disabled live owner preserves existing `TunnelBuildReserved` behavior;
27. enabled live owner routes authenticated STBM through transit once;
28. creator-correlated STBM bypasses transit in the live owner;
29. existing creator/service TunnelData receive id wins over transit;
30. transit-owned TunnelData id is processed exactly once by transit;
31. unknown receive id is dropped without mutating either registry.

## Work package E — complete delivery rollback and lifecycle cleanup

Accepted build state survives only if its required build delivery was actually admitted.

For every accepted build dispatch, rollback the just-committed registration when delivery
returns any terminal non-accepted result:

- `QueueFull`;
- `ResourceDenied`;
- `NoActiveSession`;
- `TooLarge`;
- `DeadlineElapsed`;
- `Cancelled`;
- request/message construction error.

Only `RouterDeliveryOutcome::Accepted` may leave the accepted registration live.

Valid policy rejection owns no registration and therefore never invokes rollback.

Cancellation/shutdown:

- `cancel()` must synchronously drain all active registrations and drop/zeroize their
  secret owners;
- pending admission state must return to zero;
- bounded peer/session index is cleared;
- new dispatch fails closed;
- shutdown does not rely on the 600-second expiry timer to destroy secrets;
- `disable()` / owner drop remains an equivalent full drain.

Add an explicit `drain`/`clear` registry API if needed; it must return/drop owned entries
without Clone.

Required tests:

32. each terminal delivery outcome above leaves active count at baseline;
33. Accepted delivery retains exactly one registration;
34. cancellation after N active registrations immediately yields active=0/pending=0;
35. cancellation drops secret owners before return (prove through state/Drop test seam, not
    timing);
36. repeated cancellation/drain is idempotent;
37. shutdown through the live owner drains transit before the SSU2 owner completes.

## Work package F — make the peer/session mapping actually bounded

The current `BTreeMap<Hash, PeerId>` has no capacity check.

Prefer eliminating the duplicate index if the existing authenticated SSU2 session owner can
perform the lookup directly through a bounded typed seam.

If a transit-local index remains:

- set a hard maximum derived from the authoritative session/resource ceiling;
- `install_peer` returns a typed result;
- duplicate router updates may replace the existing value without increasing count;
- new insertion at capacity fails closed;
- no silent eviction;
- session-close removes the entry;
- cancellation/shutdown clears it.

Required tests:

38. capacity cannot be exceeded;
39. duplicate update does not consume a second slot;
40. removal allows later insertion;
41. live session-close path removes the mapping;
42. cancellation clears the index.

## Work package G — secret ownership cleanup and static guards

Remove `Clone` from `TransitHopMaterial` unless implementation demonstrates a necessary
single-secret shared ownership model with zeroization guarantees. Prefer move-only
ownership.

Extend `scripts/check-m11-transit-boundaries.sh` narrowly to fail if:

- production daemon reintroduces a placeholder/no-op TunnelData transform;
- production daemon directly uses `TunnelLayerTransform` or raw `LayerKeys`;
- `TransitBuildService` has no production caller outside its own test module once controlled
  composition is enabled;
- rejection delivery discards `TransitBuildRoute`;
- terminal non-Accepted router-delivery outcomes bypass rollback;
- secret-owning transit material regains Clone;
- transit peer/session state is an unbounded collection without an explicit capacity
  contract.

Keep the guard small and semantic; do not create another large source-text harness.

## Failure, cancellation, restart, and contention semantics

- Malformed/unauthenticated build: no reply, no state, no delivery.
- Valid policy reject: transformed 30 reply follows role-correct build routing, no
  registration.
- Accepted build + message-construction failure: rollback before returning.
- Accepted build + any non-Accepted router-delivery outcome: rollback before returning.
- Accepted build + delivery Accepted: registration remains until expiry/shutdown/explicit
  terminal failure.
- TunnelData wrong peer / unknown id / expired / duplicate / malformed: fail closed before
  forwarding.
- Participant success: exactly one transformed next-hop cell.
- OBEP success: zero or one semantic delivery action per completed message/reassembly event.
- IBGW success: bounded one-or-more cell vector from canonical fragmentation rules.
- Cancellation/shutdown: immediate deterministic drain of registrations, pending state,
  peer mapping, replay/reassembly state, and secret material.
- Restart: empty transit state; no persistence or replay of transit secrets.

## Compatibility and migration

No persisted-state migration and no public wire-format change.

No new dependency is expected. If one is proposed, stop for dependency review.

Default behavior remains unchanged: controlled transit is disabled and unadvertised.

Plan 252's runtime-neutral full-message build processor is retained. This corrective should
not rewrite its crypto semantics unless a new direct regression proves a defect.

## Exact verification floor

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-tunnel --all-targets -- --test-threads=1
cargo test --locked -p i2pr-daemon --all-targets -- --test-threads=1
cargo test --locked --workspace --all-targets -- --test-threads=1
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
cargo test --locked --workspace --doc
cargo deny check advisories bans sources
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-m11-transit-boundaries.sh
bash scripts/check-m6-mixed-router-acceptance-evidence.sh
bash scripts/check-exploratory-tunnel-evidence.sh
git diff --check
```

Additionally run all new focused integration targets exactly.

Unlike the Plan 252 local verification record, **full workspace tests are mandatory** for
Plan 253 closure.

GitHub Actions on the exact closure/implementation SHA must be fully green.

Do not run Plan 254's external i2pd lane as a substitute for these local/live-owner
requirements.

## Documentation updates on implementation

- write `plans/closure/transit-tunnels/253-status.md` with a direct requirement-to-test
  matrix;
- update `plans/closure/transit-tunnels/252-status.md` current-authority pointer without
  deleting the historical Plan 252 evidence;
- update `plans/subsystems/transit-tunnels-roadmap.md`;
- update `plans/registry.md`;
- update `specs/support.toml`;
- update `specs/protocols/05-tunnels.md`;
- update `docs/architecture/i2pr-daemon.md` with the actual live owner and role-correct
  data-plane flow;
- register Plan 254 only after the unblock audit proves this corrective closed.

## Acceptance criteria

Plan 253 closes only when all of the following are directly proven:

1. Plan 252 full-message STBM crypto tests remain green unchanged;
2. a live authenticated SSU2/router-I2NP owner actually calls the controlled transit gate;
3. default/ordinary daemon behavior remains transit-disabled;
4. authenticated previous-peer provenance survives the real owner path;
5. Participant TunnelData bytes are transformed using the stored canonical keys;
6. exact replay is rejected by a bounded duplicate window;
7. wrong peer fails before replay/window mutation;
8. OBEP performs final-layer processing and returns canonical semantic delivery;
9. IBGW uses role-correct TunnelGateway -> TunnelData behavior if IBGW remains in scope;
10. code-30 outcomes preserve route and are actually delivered in the correct STBM/OTBRM
    shape;
11. delivered build bytes decode as complete I2NP messages with exact message ids and body;
12. nonzero reply-tunnel routing is not discarded;
13. every non-Accepted build delivery outcome rolls accepted registration back;
14. cancellation/shutdown immediately drains active/pending/peer/secret state;
15. peer/session lookup state has a hard enforced bound;
16. creator/service and transit receive-id ownership cannot double-process TunnelData;
17. `TransitHopMaterial` or equivalent secret owner is move-only;
18. no daemon-side duplicate crypto/data-plane implementation remains;
19. full focused/workspace/clippy/doc/deny/static verification is green;
20. ordinary GitHub Actions is green on the exact implementation/closure SHA;
21. no capability/version/RouterInfo/listener/public-network change lands.

This remains infrastructure-only. M11 capability is still unclaimed.

## Stop conditions

Stop and register a narrower architectural decision/corrective instead of improvising if:

- canonical role reuse requires cloning secret-owned state across owners;
- the live ingress owner cannot distinguish creator/service TunnelData ownership from
  transit ownership without a new registry contract;
- OBEP reply-tunnel delivery requires wire behavior not represented by current canonical
  router/tunnel delivery APIs;
- IBGW controlled composition requires a separate lifecycle owner that would make this
  plan exceed one bounded ownership boundary;
- external/reference behavior is needed to decide a local API contract before Plan 254;
- a new dependency or public configuration surface is proposed.

If IBGW must be split, narrow the Plan 253 closure claim explicitly and register that
missing role before Plan 254; do not silently qualify a partial role set as M11.

## Closure authority and successor

On registration:

- Plan 252 becomes retained infrastructure with corrective required via Plan 253;
- Plan 253 is the next executable plan;
- Plan 254 is reserved, unregistered, for exact-pinned i2pd controlled transit
  qualification;
- M12 floodfill remains deferred until the controlled M11 qualification is complete.

## Handoff notes

Do not begin with i2pd.

First make the existing false-positive daemon tests fail for the correct reason, then
repair the runtime-neutral data-plane ownership and live owner composition.

The most important source smell to remove is the comment in
`forward_participant_layer` that explicitly describes the current no-op as a placeholder.
The corrective is not complete while any equivalent placeholder remains.

Do not satisfy the live-owner requirement with another direct unit call to
`TransitBuildService`. The test must traverse the owner that receives authenticated
`Ssu2InboundI2np`.

Do not satisfy TunnelData correctness by merely asserting next router/tunnel ids. Assert
the transformed bytes and replay behavior against the canonical role/vector.
