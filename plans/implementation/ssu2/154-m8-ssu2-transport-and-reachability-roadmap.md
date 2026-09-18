# Plan 154 — Milestone 8 SSU2 v2 transport and reachability roadmap

Status: **Milestone 8 planning authority; implementation blocked by Plan 153**.

Registered: **2026-09-03**.

Depends on:

- Milestone 7 localhost SAM closure via Plan 151;
- narrow M6 robustness correction via Plan 152;
- Plan 153 authority/CI hygiene closure before any Milestone 8 production code is changed.

Execution sequence:

```text
Plan 153  post-M7 authority / CI hygiene
    ↓
Plan 155  SSU2 v2 protocol foundation and addresses
    ↓
Plan 156  handshake, token, RouterInfo establishment
    ↓
Plan 157  data phase, ACK/loss, fragmentation/reassembly
    ↓
Plan 158  UDP runtime adapter and local session product
    ↓
Plan 159  path validation, publication, transport selection
    ↓
Plan 160  peer test and relay reachability
    ↓
Plan 161  independent IPv4 interop and final M8 closure
```

## 1. Milestone objective

Implement classical **SSU2 protocol version 2** as the router's UDP transport while preserving the existing transport/runtime boundaries and keeping reachability/address publication conservative.

Milestone 8 closes when i2pr can establish incoming and outgoing authenticated SSU2 v2 sessions over real UDP datagrams, exchange I2NP messages with a pinned independent router implementation, recover correctly under bounded loss/reordering, perform required path/reachability functions, and leave all public/nonlocal activation explicitly opt-in and non-advertised until supported by evidence.

## 2. Scope decision: SSU2 v2 first

The current classical SSU2 specification is complete and the existing repository dossier already targets it. The upstream ecosystem also has emerging PQ-hybrid SSU2 protocol versions 3/4.

For the MVP Milestone 8:

```text
ssu2_v2_classical = required
ssu2_pq_v3_v4 = deferred-compatibility-watch
ssu1 = not-implemented / legacy-reject
```

Rules:

- implement the SSU2 v2 wire protocol exactly;
- version parsing must be explicit and bounded;
- version 3/4 inputs must be classified/rejected safely rather than accidentally parsed as v2;
- do not add ML-KEM/PQ dependencies in this milestone;
- do not copy PQ negotiation semantics into v2;
- record PQ support as future compatibility debt in `specs/support.toml`/protocol documentation when Plan 155 refreshes the dossier.

## 3. Architecture decision

### 3.1 New runtime-neutral protocol crate

Add:

```text
crates/i2pr-transport-ssu2
```

It follows the architectural pattern of `i2pr-transport-ntcp2`:

- no Tokio;
- no socket creation;
- no async runtime;
- strict bounded data types/codecs/state machines;
- crypto through existing reviewed crates/wrappers;
- deterministic actions emitted for the runtime to fulfill.

Classical SSU2 should require no new bespoke cryptographic implementation. Reuse the workspace's X25519, ChaCha20, ChaCha20-Poly1305, SHA-256/HMAC/HKDF-capable primitives as specified.

### 3.2 Reuse the existing generic transport manager

`i2pr-transport` already owns:

- link lifecycle;
- delivery contracts;
- link admission/duplicate resolution;
- bounded resource accounting;
- dial/backoff concepts;
- privacy-safe reachability observations.

Do **not** create an SSU2-specific second transport manager.

Add `TransportKind::Ssu2` and only the smallest transport-neutral extensions actually required by UDP semantics.

### 3.3 UDP ownership remains in `i2pr-runtime`

Only `i2pr-runtime` may own production UDP sockets, Tokio timers, and supervised socket/session tasks.

Preferred runtime topology:

```text
one UDP receive owner per socket/address family
        ↓
cheap datagram classification/admission
        ↓
bounded session table + central scheduler
        ↓
runtime-neutral SSU2 state machines
        ↓
existing TransportManager / I2NP delivery contracts
```

Avoid:

- one task per packet;
- one timer per packet;
- unbounded datagram queues;
- protocol crates calling `UdpSocket` directly;
- duplicate generic transport/resource frameworks.

## 4. Environment contract

Milestone 8 must remain executable in the same constrained development environment:

```text
root/sudo              = no
Linux namespaces       = no
Docker                  = no
VM/Multipass            = no
systemd                 = no
public I2P network      = no
localhost UDP           = yes
GitHub-hosted Ubuntu    = yes
external source checkout = yes, ephemeral/pinned
```

Do not resurrect the historical Milestone 3 rootless/VM harness architecture.

Real localhost UDP datagrams are sufficient for the local product and independent direct-session interop lanes.

## 5. Independent interoperability policy

Milestone 8 must not repeat the previous mistake of making a heavyweight multi-router harness a prerequisite for ordinary protocol progress.

### Mandatory final independent implementation

Use **i2pd 2.61.0**, exact release commit:

```text
635b013a612ff47278ef02acf8580a28e10e26c5
```

as the mandatory independent SSU2 v2 implementation for Plan 161.

Required directions:

```text
i2pr initiator -> i2pd responder
i2pd initiator -> i2pr responder
```

Both must use real UDP datagrams, not direct calls into protocol objects.

### Secondary reference

Java I2P 2.13.0 exact release commit:

```text
9134f808337b401e8e53c73734c81fab04280c9d
```

is a preferred secondary reference lane. If a narrow rootless standalone invocation is practical, run it. If Java orchestration requires disproportionate daemon/network setup, record it as nonblocking secondary-reference debt rather than blocking M8.

One mature independent implementation in both directions is sufficient for this milestone's direct SSU2 claim.

### Claim boundary

Even after Plan 161, do not claim full public-router interoperability unless the evidence actually includes NetDB/tunnel/public-network behavior. The intended M8 claim is narrower:

```text
SSU2 v2 direct authenticated session interoperability + I2NP transport
```

## 6. Source refresh requirement

Before Plan 155 changes code, refresh the SSU2 research ledger:

```text
specs/SOURCES.md
specs/protocols/09-ssu2.md
specs/IMPLEMENTATIONS.md
specs/support.toml
```

The current dossier is a strong starting point, but M8 is a refresh trigger by the repository's own rules.

At minimum record:

- the exact official SSU2 v2 spec snapshot used;
- Proposal 159/165 relationship to the current spec;
- current PQ SSU2 v3/v4 compatibility-watch status;
- exact Java I2P 2.13.0 and i2pd 2.61.0 implementation revisions;
- any deployed behavior observed in current reference code that the specification leaves implementation-defined.

Specifications are normative. Reference implementations are used to resolve ambiguity and build interoperability evidence, not copied as source.

## 7. Milestone decomposition

### Plan 155 — protocol/address/block foundation

Create the crate, extend transport kind, implement strict RouterAddress handling and bounded packet/block primitives plus vectors. No handshake state machine and no UDP sockets.

### Plan 156 — handshake/token establishment

Implement Noise XK handshake, header protection, Retry/token flow, RouterInfo establishment and bounded replay/retransmit state machines. Still no UDP socket ownership.

### Plan 157 — data-phase reliability

Implement packet numbers/replay windows, ACK ranges and delayed/immediate ACK policy, loss/retransmission, congestion state, I2NP fragmentation/reassembly, key rotation and termination in deterministic runtime-neutral state machines.

### Plan 158 — UDP runtime/local product

Create the supervised UDP adapter in `i2pr-runtime`, integrate the generic transport manager, and prove i2pr↔i2pr local sessions using real localhost datagrams.

### Plan 159 — path/reachability/selection

Add path validation/migration, conservative external-address observation/publication state, and deterministic NTCP2/SSU2 transport selection/fallback policy.

### Plan 160 — peer test and relay

Implement the reachability PeerTest/relay roles and deterministic NAT-like multi-endpoint tests. Public introducer service remains disabled by default.

### Plan 161 — independent final closure

Run exact-pinned i2pd in both directions over real localhost UDP, exchange authenticated I2NP messages, harden the evidence ledger, and close M8 on an exact hosted revision.

## 8. Cross-cutting security/resource rules

All Plan 155–161 work must preserve:

- datagram length checked before expensive parsing;
- cheap version/network/connection-ID rejection before avoidable DH/AEAD work;
- token/retry responses bounded against amplification;
- bounded pending handshakes and active sessions;
- bounded packet history, ACK ranges, retransmit metadata, reassembly state, path candidates, relay/peer-test state;
- no unbounded channels;
- no per-packet task/timer explosion;
- explicit idle/handshake/reassembly/path-validation expiry;
- authenticated data exposed only after AEAD verification;
- secret-bearing state non-Clone/redacted/zeroized where applicable;
- OS CSPRNG for runtime token/session/key material;
- deterministic RNG only in tests/vectors;
- privacy-safe snapshots (no raw peer IP history beyond what runtime operation requires; no secrets/payloads in logs/evidence).

## 9. Congestion/loss-control policy

Where SSU2 intentionally leaves algorithm choice to implementations, begin with a conservative, auditable byte-count controller rather than optimizing throughput.

Requirements:

- bounded congestion window;
- RTT/RTO estimators with explicit min/max bounds;
- central loss/retransmit scheduler;
- ACK-only packets not charged as congestion-controlled data when the spec says so;
- no ACK-of-ACK loop;
- retransmission reconstructs a new packet from still-needed I2NP fragments/current ACK state rather than replaying cached ciphertext;
- tests prioritize correctness/predictable memory over peak packets/sec.

Do not import a full QUIC stack or inherit unspecified QUIC semantics.

## 10. Reachability/publication policy

A single unauthenticated address observation must never publish a reachable SSU2 address.

Plan 159/160 must define a typed conservative state machine for:

```text
unknown
observed-unconfirmed
peer-tested / corroborated
reachable
firewalled-with-introducers
unreachable
```

Publication consumes a validated snapshot; the packet codec does not mutate NetDB directly.

IPv4 direct SSU2 is mandatory for M8 closure. IPv6 data structures/socket separation are mandatory, but independent IPv6 interop may remain documented debt if hosted infrastructure cannot provide meaningful testing.

## 11. Transport selection policy

SSU2 and NTCP2 selection must share the existing transport manager rather than competing side systems.

At minimum:

1. reuse an existing authenticated link first;
2. respect per-transport/per-peer backoff;
3. select only addresses that are structurally valid and compatible;
4. prefer direct/reachable candidates over relay-only candidates;
5. deterministic tie-breaking;
6. a failed SSU2 attempt must not poison a viable NTCP2 path unless peer-wide policy explicitly requires it;
7. do not activate historical NTCP2 public advertising merely to test fallback.

## 12. Evidence hierarchy

Milestone 8 evidence should distinguish:

```text
protocol-vectors
runtime-neutral-state-machine
localhost-real-UDP
independent-direct-interop
public-network/mixed-router
```

Plan 161 may close the first four. The fifth remains separate unless explicitly executed later.

Every final evidence row must be command-derived. Use the Plan 151 lesson: no unconditional `passed` ledger entries.

## 13. Validation floor for every executable pass

Each Plan 155–161 must run its focused tests plus the repository floor appropriate to its touched crates. At minimum before closing any pass:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
```

Once an SSU2 vector/boundary checker exists, it becomes part of this floor for all later M8 passes.

## 14. Milestone 8 final acceptance

Plan 161 may close Milestone 8 only if all of these are true:

1. SSU2 v2 RouterAddress/address-family handling is strict and bounded.
2. Incoming/outgoing v2 handshakes and Retry/token flows are implemented.
3. Data-phase packet authentication, replay protection, ACK/loss recovery and I2NP fragmentation/reassembly are bounded and deterministic under fault tests.
4. Runtime UDP ownership respects existing architecture and resource contracts.
5. Real localhost i2pr↔i2pr UDP sessions exchange I2NP messages bidirectionally.
6. Path validation/migration rejects spoofed endpoint changes.
7. Reachability publication is conservative and cannot be triggered by one unauthenticated observation.
8. Peer-test/relay state is bounded and anti-amplification tested.
9. Transport selection/fallback is deterministic and preserves viable alternatives.
10. Exact pinned i2pd 2.61.0 interoperates in both direct-session directions.
11. Authenticated bidirectional I2NP exchange is proven across the independent boundary.
12. External evidence is generated from executed commands on the exact closing head.
13. Routine CI and the manual SSU2 external workflow pass on the closing head.
14. SSU2 remains disabled/non-advertised until the milestone's explicit activation policy says otherwise.
15. SSU2 PQ v3/v4 remain explicitly deferred, not accidentally claimed.
16. SSU1 remains unimplemented/rejected.
17. IPv6 independent interop is either passed or explicitly retained as infrastructure-limited debt.
18. No public-network, NetDB, tunnel, or anonymity claim is inferred from direct SSU2 evidence.

Expected closing classification:

```text
milestone8_ssu2_v2_local = passed
milestone8_ssu2_v2_interop = passed-via-i2pd
ssu2_pq_v3_v4 = deferred
ssu1 = not-implemented
milestone6_interoperable = not-yet-claimed   # unless separately closed
next_product_layer = milestone9-planning
```

## 15. Handoff

First execute Plan 153. After it passes, Plan 155 is the first Milestone 8 implementation plan. Execute Plans 155–161 in order; do not skip forward based on aggregate workspace green status.