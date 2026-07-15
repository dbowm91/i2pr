# Tunnel construction and tunnel messages

Status: **required**  
Primary roadmap milestone: **5**  
Dependencies: common structures, I2NP, NetDB, cryptography and a working router transport

## Scope

I2P network tunnels are unidirectional paths used to carry I2NP messages anonymously. This dossier covers tunnel IDs, tunnel-build request/reply messages and records, ECIES-X25519 build encryption, gateway batching, tunnel-data encryption, fragmentation/reassembly, participant roles, lifecycle and tunnel testing.

Inbound and outbound tunnels are separate. A bidirectional application flow therefore depends on multiple unidirectional tunnel pools; this must remain explicit in the architecture.

## Authoritative sources

- [Tunnel creation with ECIES-X25519](https://i2p.net/en/docs/specs/tunnel-creation-ecies/), pinned in [SOURCES.md](../SOURCES.md), updated 2025-06 and accurate for 0.9.66.
- [I2NP specification](https://i2p.net/en/docs/specs/i2np/) for tunnel-build, tunnel-gateway, tunnel-data and tunnel-test messages.
- [Tunnel routing overview](https://i2p.net/en/docs/overview/tunnel-routing/) for roles and lifecycle intent.
- Proposals 152, 157 and 168 for ECIES records, short build messages and bandwidth parameters.
- Legacy tunnel-creation material only when needed for mixed-hop compatibility.

The current ECIES specification identifies the long ECIES build-record format as deprecated/obsolete and directs implementations to the short-record format. Mixed ElGamal/ECIES handling exists for network migration; `i2pr` should implement it only to the extent required by the chosen peer set and MVP compatibility goals.

## Required MVP roles

Implement explicit role-specific state:

- **creator/builder** — constructs encrypted per-hop records and processes replies;
- **inbound gateway** — receives messages from the creator’s remote side and injects them into the inbound tunnel;
- **participant** — decrypts one layer and forwards to the next hop;
- **outbound endpoint** — removes the final tunnel layer and applies delivery instructions;
- **local inbound endpoint/outbound gateway** — connects tunnel messages to local NetDB, garlic and destination services.

Role separation is important for auditing secrets and resource ownership. A participant must not gain creator-only path knowledge through shared state.

## Required construction behavior

- Use current short tunnel-build messages/records for ECIES routers.
- Generate independent ephemeral X25519 material per hop as specified.
- Reproduce exact Noise-N transcript, HKDF, ChaCha20/Poly1305, reply encryption and record ordering.
- Populate and validate receive tunnel IDs, next-hop router hash/tunnel ID, layer/IV keys, flags, request time, expiration and current build options.
- Support reject response codes without exposing excessive local policy detail.
- Randomize record position/order as required and prevent a hop from learning path position beyond protocol leakage.
- Bound pending builds globally, per peer, per pool and per destination.
- Correlate replies without trusting unauthenticated or replayed records.
- Clean up partial builds and key material on timeout, rejection, malformed reply, cancellation or peer disconnect.

Tunnel-build bandwidth parameters and tunnel testing are required by recent I2NP feature levels. Their exact mandatory behavior must be reconciled with the I2NP version `i2pr` advertises.

## Tunnel data plane

Implement:

- tunnel gateway batching with bounded latency and bytes;
- layered tunnel-message encryption/decryption using protocol-defined keys and IV handling;
- delivery instructions for local, router, tunnel and destination targets;
- fragmentation and reassembly across tunnel messages;
- out-of-order fragment handling where permitted;
- duplicate/replay suppression and message expiration;
- strict per-message, per-tunnel and per-peer reassembly budgets;
- forwarding queues with bandwidth accounting and backpressure;
- no participant-side semantic parsing beyond what forwarding requires.

A malformed fragment sequence must terminate only the affected reassembly/message unless the specification or abuse policy requires stronger action. It must not allocate according to an untrusted claimed final size without a hard cap.

## Tunnel pools and lifecycle

The first pool implementation must support exploratory inbound/outbound tunnels and later destination-specific pools. It must define:

- target quantity and length policy;
- build-ahead, replacement and expiration timing;
- success/failure and peer-avoidance inputs;
- tunnel testing before use where required;
- graceful draining versus immediate destruction;
- selection among usable tunnels without exposing deterministic fingerprints;
- bounded concurrent builds during startup or network degradation.

Tunnel length and peer selection are anonymity policy, not wire format. Keep them outside codecs and crypto records.

## Transit participation

Transit mode is required by the MVP under explicit resource policy. Admission must consider:

- bandwidth and queue capacity;
- active transit tunnel limits;
- per-peer/subnet diversity and abuse controls;
- supported build-record/key type;
- requested expiration and bandwidth parameters;
- hidden/floodfill/router operational mode;
- local shutdown/degraded state.

A rejection should be protocol-correct and inexpensive. Never reserve large buffers or spawn long-lived tasks before admission succeeds.

## Implementation references

- Java I2P: `router/java/src/net/i2p/router/tunnel`, tunnel-build handlers, gateway/fragmentation code and tunnel pools.
- I2P+: corresponding tunnel packages; compare pool management, peer selection and bandwidth/queue hardening.
- i2pd: `libi2pd/Tunnel*`, build-message, gateway and transit sources.
- Emissary/go-i2p: tunnel packages plus `lib/i2np/build_record_crypto.go` and related I2NP routing.

Compare short-record codecs, mixed-hop behavior, response codes, replay filters, expiration windows, fragment limits, tunnel testing, build concurrency and transit rejection policy. Avoid importing peer-selection algorithms without separate anonymity/security review.

## Required tests

- Fixed ECIES short build-request/reply vectors with per-hop transcript checks.
- Record-position randomization and wrong-hop/wrong-key tests.
- Replay, stale/future request time, invalid expiration and duplicate build tests.
- All accept/reject response paths and partial build cleanup.
- Creator and all transit roles in deterministic in-memory multi-hop simulations.
- Gateway batching boundaries and cancellation.
- Fragmentation/reassembly at every boundary, including lost, reordered, duplicate and conflicting fragments.
- Reassembly memory pressure across many tunnels/peers.
- Tunnel expiration, replacement, draining and failed test behavior under virtual time.
- Mixed-router builds through Java I2P and i2pd peers in both creator and transit roles.
- Bandwidth/backpressure tests proving transit traffic cannot starve essential local control work.
- Fuzz targets for build records, delivery instructions and fragment parsers.

## Deferred and compatibility behavior

- Deprecated long ECIES records: compatibility-only if current mixed-router testing requires them.
- ElGamal router identity generation: excluded; mixed tunnels may still require constructing legacy records for legacy hops, subject to Milestone 5 research.
- Variable tunnel duration or experimental options: deferred unless required by current specification/version advertisement.
- Advanced peer selection, cover traffic and adaptive path-length experimentation: post-MVP and non-default.
- Public-network transit participation: deferred until controlled testnet resource and privacy review passes.

## Open decisions

1. Whether the MVP must build mixed ECIES/ElGamal tunnels or may restrict candidate peers to current ECIES routers.
2. Exact tunnel-build and reassembly size/count budgets.
3. Pool quantity/length defaults for low-resource and balanced profiles.
4. Data structures that isolate creator path knowledge from participant state.
5. Tunnel-test message cadence and failure thresholds required by the selected I2NP API version.
6. Transit admission/rejection codes and anti-fingerprinting policy.
7. How bandwidth accounting integrates with the router-wide resource governor without adding timing-dependent deadlocks.