# Plan 160 — Milestone 8 SSU2 peer test and relay reachability

Status: **registered; execute after Plan 159 passes**.

Depends on Plan 159. Blocks Plan 161.

## 1. Goal

Implement the SSU2 v2 reachability mechanisms required for ordinary firewalled-router operation: three-party PeerTest, relay requester/introducer/target flows, hole-punch handling, bounded introducer records, anti-amplification/resource policy, and integration with Plan 159's conservative reachability/publication state.

This pass uses deterministic/local multi-endpoint testing; it does not require the public I2P network.

## 2. General design rule

PeerTest and relay are session-level protocol state machines, not special UDP daemons.

Keep responsibilities separated:

```text
i2pr-transport-ssu2
  message/block validation
  signatures/freshness structures
  runtime-neutral peer-test/relay states

 i2pr-runtime
  UDP send/receive
  endpoints/time/randomness
  admission/rate limits
  state ownership/scheduling

 reachability/publication policy
  consumes authenticated results
  decides state/publication
```

Do not let a decoded PeerTest/Relay block mutate RouterInfo/NetDB directly.

## 3. PeerTest roles and correlation

Implement the exact current SSU2 PeerTest roles (commonly Alice/Bob/Charlie in reference terminology) and message sequence.

Model each active test with a typed unique/correlation identifier and explicit role/state.

Required properties:

- concurrent tests cannot consume each other's messages;
- endpoint/peer/session role checks at every transition;
- test nonce/correlation values OS-random in production;
- freshness/deadline checks;
- exact signature verification where the current spec signs peer-test data;
- maximum concurrent tests globally/per peer/per source;
- one central expiry scheduler, not one task per test;
- duplicate/reordered valid messages are idempotent where protocol permits;
- stale/unknown/wrong-role messages cannot create reachability evidence.

## 4. PeerTest reachability outputs

Do not reduce results to a single boolean too early.

Return typed outcomes such as:

```text
DirectReachabilityConfirmed { family, observed_endpoint, evidence_peers }
AddressMismatch { ... }
FirewalledLikely { ... }
PeerTestInconclusive { reason }
PeerTestRejected { reason }
```

Plan 159's reachability policy consumes those results with corroboration/expiry rules.

Requirements:

- a single unauthenticated datagram never confirms reachability;
- contradictory authenticated tests do not simply let the latest packet win;
- direct publication requires the policy's required corroboration/confirmation level;
- IPv4 and IPv6 evidence are separate;
- evidence expires/withdraws.

## 5. Relay protocol roles

Implement the current SSU2 v2 relay control flows required for a firewalled router to use introducers and, when explicitly enabled, to act as an introducer.

At minimum cover current equivalents of:

```text
RelayRequest
RelayResponse
RelayIntro
HolePunch
```

Resolve exact block/message forms and signatures from the refreshed spec.

### Requester role

- select only authenticated/current introducer records;
- bounded number of concurrent requests;
- verify response tag/status/signature/freshness;
- correlate introduction/hole-punch to the exact request;
- transition to a normal direct SSU2 handshake only through the spec-defined endpoint/token flow;
- failure/backoff does not grow introducer/request state unboundedly.

### Introducer role

May be implemented for protocol completeness but remains **disabled by default**.

When enabled in controlled tests:

- admit only authenticated eligible sessions/peers;
- per-peer/source/global quotas;
- bound relay tags and lifetime;
- verify requester message/signature/freshness;
- no response amplification beyond protocol limits;
- refuse when resource/health policy disallows service;
- expire tags deterministically;
- shutdown removes advertised/active introducer state.

### Target role

- validate RelayIntro against expected/authenticated introducer context;
- verify signed/fresh data;
- emit only bounded required HolePunch/handshake initiation traffic;
- replay/stale intro cannot trigger repeated amplification.

## 6. Introducer records and RouterInfo publication

Create one bounded validated introducer-record owner with fields required by v2, e.g. peer identity/reference, relay tag, endpoint/family, expiration, and authenticated provenance.

Rules:

- maximum introducers published is explicit and spec-compatible;
- choose only live/recent authenticated introducers;
- deterministic replacement/expiry;
- never publish stale/failed records;
- firewalled SSU2 RouterAddress output from Plan 159 consumes this validated set;
- direct host/port and introducer-only publication follow the reachability state, not a caller boolean;
- introducer public service remains disabled unless configuration explicitly enables it after tests.

## 7. Anti-amplification and abuse controls

Add explicit quotas/counters for:

```text
PeerTest starts/responses
RelayRequest responses
RelayIntro emissions
HolePunch emissions
tags per peer
tags globally
concurrent tests/relays
response bytes before validation
```

Per-IP/subnet/global rate limits belong to runtime/admission policy.

Tests must prove:

- unauthenticated floods are cheap-dropped/rate-limited;
- one source cannot cause unbounded signed/crypto responses;
- invalid signature/freshness requests do not allocate long-lived relay state;
- amplification ratio remains within protocol rules before source validation;
- quota release occurs on timeout/cancel/close.

## 8. Deterministic NAT-like local test topology

Do not use Linux namespaces/VMs.

Build a deterministic test topology from multiple loopback UDP endpoints and a narrow test-only forwarding/NAT mapper if needed:

```text
Alice   firewalled/requester
Bob     authenticated introducer / PeerTest helper
Charlie independent third peer / PeerTest helper
Target  relay target as needed
```

The forwarding layer may rewrite source endpoints to model NAT behavior, but application/protocol traffic must still cross real `UdpSocket` datagrams.

No direct calls should move PeerTest/Relay wire bytes between live runtime instances in the final black-box acceptance.

## 9. Required PeerTest scenarios

- direct-reachable IPv4 result with expected external address agreement;
- firewalled/inbound-unreachable result;
- observed address mismatch;
- third-peer refusal/timeout -> inconclusive, not false confirmation;
- invalid signature;
- stale timestamp/data;
- wrong role/session/correlation ID;
- duplicate/reordered messages;
- two+ concurrent tests with crossing message schedules proving isolation;
- per-peer/global exact-capacity/max+1;
- cancellation/shutdown cleanup;
- contradictory authenticated observations feed conservative policy state.

## 10. Required relay scenarios

- firewalled requester uses valid introducer and reaches target through introduction/hole-punch then normal SSU2 establishment;
- second request with distinct tag does not cross-contaminate first;
- unknown/expired relay tag;
- stale request/intro;
- invalid signature;
- target unreachable;
- introducer session closes mid-request;
- requester cancels;
- repeated relay request replay does not amplify indefinitely;
- rate/quota exact-capacity/max+1;
- introducer records expire and disappear from publication snapshot;
- disabling introducer service produces explicit refusal/no advertisement.

The successful product path must ultimately transition back into the normal Plan 158 handshake/session machinery rather than a relay-specific fake session.

## 11. Reachability state integration

Feed only authenticated typed PeerTest/relay outcomes to Plan 159's state owner.

Acceptance must show:

- confirmed direct IPv4 evidence permits the configured publication policy to build a direct v2 address;
- firewalled result removes/withholds direct claim and may publish validated introducers;
- inconclusive test does not flip state to reachable/unreachable arbitrarily;
- evidence expiry withdraws stale publication;
- contradictory evidence transitions through an explicit policy state rather than last-write-wins;
- a relay success does not itself prove direct inbound reachability.

## 12. Privacy/logging

Do not log:

- token/challenge values;
- full relay tags if they are operationally sensitive;
- raw peer-test signed payloads;
- application I2NP payloads;
- private keys.

Diagnostics may include role, result category, bounded counters, duration bucket, family, and redacted peer references.

Add a failure-path privacy regression similar in spirit to Plan 151 where practical.

## 13. Non-goals

No:

- public I2P participation;
- production-default introducer service;
- large distributed NAT lab;
- TURN/STUN/UPnP/NAT-PMP implementation;
- SSU1 compatibility;
- PQ SSU2;
- final independent-router interop (Plan 161).

## 14. Validation

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked -p i2pr-runtime --all-targets
cargo test --locked -p i2pr-daemon --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh
```

## 15. Acceptance criteria

Plan 160 passes only when:

1. PeerTest roles/state/correlation are explicit and bounded.
2. concurrent tests cannot consume/corrupt one another's state.
3. signatures/freshness/role/endpoint checks are enforced.
4. direct/firewalled/inconclusive/mismatch outcomes are typed and policy-safe.
5. no single unauthenticated observation can confirm/publicize reachability.
6. relay requester/introducer/target state machines are implemented for required v2 roles.
7. relay success transitions into the normal SSU2 handshake, not a special fake link.
8. relay/peer-test responses obey anti-amplification and quota rules.
9. exact-capacity/max+1 tests exist for active tests/relays/tags/rate state.
10. deterministic real-UDP NAT-like topology proves the successful PeerTest and relay trajectories.
11. invalid signatures/stale/replays/unknown tags are bounded and do not leak state.
12. validated introducers feed the Plan 159 publication builder and expire cleanly.
13. introducer service remains disabled by default.
14. cancellation/shutdown returns all test/relay/tag resources to baseline.
15. privacy/logging regression is green.
16. no public-network/independent-router claim is made.
17. full workspace/SSU2 quality floor passes.
18. `plans/160-status.md` advances only to Plan 161.

## 16. Stop conditions

Stop and narrow if:

- current spec/reference relay signatures or role transitions cannot be reconciled;
- successful relay requires bypassing normal SessionRequest/SSU2 authentication;
- meaningful NAT-like tests require privileged network namespaces rather than loopback endpoint mapping;
- abuse tests show signing/crypto/state work grows unbounded with spoofed source count.

Prefer a smaller supported relay subset with explicit debt over a permissive or unsafe introducer implementation.