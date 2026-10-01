# Protocol conformance and evidence policy

## Claim model

`i2pr` must not claim protocol support from code presence alone. A protocol or feature may be marked implemented only when the applicable evidence below exists:

1. strict decode and canonical encode tests;
2. authoritative golden vectors or independently generated cross-implementation vectors;
3. malformed, truncated, oversized and semantically invalid input tests;
4. state-machine success, failure, timeout, cancellation and teardown tests;
5. explicit memory, queue, task, retry and cryptographic-work bounds;
6. replay, duplicate, expiry and clock-skew tests where relevant;
7. mixed-router interoperability against at least two independent implementations for router-to-router protocols;
8. documentation of unsupported and compatibility-only behavior;
9. no advertised RouterInfo, I2NP, API or transport capability beyond the tested subset.

Java I2P and I2P+ share lineage and count as one implementation family for independence. The preferred router-to-router interoperability pair is Java I2P or I2P+ plus i2pd. Emissary/go-i2p should be added where its current implementation is complete enough for the tested surface.


### Evidence tiers for progression versus full conformance

ADR 0026 distinguishes an experimental development gate from the full claim above.

For **experimental development progression**, a non-advertised subsystem may continue
after local conformance requirements and at least one exact-pinned independent
implementation demonstrate the relevant controlled external path. This authorizes later
implementation work only; it does not authorize public exposure, production-readiness
language, broad advertisement, or a full interoperability claim.

For **full router-to-router conformance or broad capability advertisement**, item 7 above
remains mandatory: at least two independent implementation families must interoperate for
the claimed router-to-router surface.

For **client/application protocols** such as Streaming, SAM, I2CP, and service-tunnel
profiles, require independent evidence appropriate to the surface. A second full router
family is not automatically a hard gate merely because the path traverses routers.

Exact-pinned i2pd Plan 193 satisfies M6 experimental mixed-router progression. The Java
full-router lane remains compatibility debt at Plan 247. Full two-family M6 router
conformance remains not claimed.

M11 transit remains retained/unclaimed and `advertised=false`. Plans 255-258 retain their
documented infrastructure/corrections, including Plan 258's externally proven canonical
multicell IBGW emission. Plan 259's transit-endpoint topology inventory is retained but its
receipt-is-OBEP-only conclusion is not current authority: exact-pinned i2pd distinguishes
`TransitTunnelEndpoint(false)` from a creator-owned `InboundTunnel` that sets the incoming
message owner to a destination-pool tunnel before LOCAL garlic dispatch. Plan 260 is
retained-blocked: its seven creator-owned source locks, explicit-peer lock,
fragment-id hardening, tuple validator, and receipt harness all landed with local
rows green, and the dedicated receiver 1-hop `[i2pr]` inbound was exhibited live
(`[A,A]` IBGW accepts plus SAM STATUS OK on a fresh mesh) — but delivery stops
on two exact boundaries: forward-path garlic death at B's endpoint for A-side
senders (zero ingress on counted ids over four multicell-forcing rounds) and
late-run mesh sustainability (establishment 1/3, outbound collapse without
floodfill). Plan 261 is retained-blocked: its B-sender lane work
all landed with local rows green and zero production diff (four
B-side source locks, B SAM plumbing with fail-closed env gate,
`m11-tx-b` sender shape, terminal-signature instrumentation), and
live execution proved the B-sender topology through addressing (B
outbound `[i2pr]` established, B-side LeaseSet resolved from the
floodfill store, 4/4 sends naming counted `[A,A]` ids) — but
delivery stops on one exact boundary: the B3 self-delivery
loopback gap (self-targeted OBEP TUNNEL actions terminate
`NoActiveSession` at the session peer seam, zero ingress, zero
socket receipt). Plan 262 is retained-blocked: its dedicated IBGW
state, exact receive-id ownership, source-neutral seam, and
self-delivery loopback arm all landed with local rows green and
live execution flipped B3 with socket receipt on a healthy mesh
(diag4 `terminal-garlic-self:0/ingress:6/socket:1` with
tuple-bound multicell on `514bf12`), but two same-SHA full-matrix
attempts stop on mesh-sustainability signatures (IBGW-data relay,
SAM timeout). Plan 263 is retained-blocked: its harness-only
sustainability proofs landed with zero production diff and
re-proved receipt (`0/18/1` tuple-bound) + IBGW multicell (max 2)
on `9bd2f39a`, but two same-SHA single-mesh attempts stop on
sustainability signatures (single-cell-only window, receipt
starvation). Plan 264 is retained-blocked: its per-epoch lane +
composition gate landed with zero production diff and proved
the gate (5 epochs 2/2 on `6ab9dc2d`), but emission epochs stop
on window signatures (ibgw-data 1/2, receipt 0/2,
participant-data 1/2, replay 0/2). Plan 265 executed that fixed-budget contract
(24 retained attempts on qualification SHA `4682920e`, zero
production diff, zero i2pr semantic failures) and is
retained-blocked: `ibgw-data` 4/8 and `participant-lifecycle`
5/8 closed, `receipt` 1/8 against a required 2. Plan 266 executed
its ladder contract (8 retained `receipt` attempts on
qualification SHA `a9803ca`, zero production diff, zero i2pr
semantic failures) and is retained-blocked: `receipt` 1/8 against
a required 2 with rung distribution r1x2/r4x2/r6x3/r7x1, every
rung-6 attempt an anchored accepted-id drop population. Plan 267
executed its disposition contract (8 retained `receipt` attempts
on qualification SHA `315fb0d`, zero production diff, zero i2pr
semantic failures) and is retained-blocked: `receipt` 0/8 against
a required 2 with 427 drop rows dominated by accepted-yet-not-
found with delays in minutes. Plan 268 executed its path contract
(8 retained `receipt` attempts on qualification SHA `cc9b40c`,
zero production diff, zero i2pr semantic failures) and closed the
family 3/8 with the install-path divergence falsified (353 paths
all dispatched, zero bypass) with exact-head ordinary CI green, so
ADR 0026's one-family M11 experimental qualification is passed.
Every
attempt remains in the denominator; opportunity is classified
before semantic output; any opportunity-present i2pr
contradiction hard-fails; successful closure requires at least
two qualifying successes per family. This is an M11-specific
evidence composition, not a general relaxation of the
conformance policy. Public transit, RouterInfo capability,
router.version, public-network participation, and broad two-family conformance remain
unauthorized.

### M12 floodfill evidence vocabulary

ADR 0027 defines five M12 evidence tiers: `architecture-frozen`, `local-validated`,
`one-family-experimental`, `two-family-qualified`, and `normal-opt-in-activated`.
They are separate authority transitions: architecture or local tests do not claim
interoperability; one-family evidence permits controlled experimental progression only;
two independent implementation families are required before broad floodfill capability
advertisement; normal activation additionally requires explicit operator opt-in and live
readiness/health. Current state is `architecture-frozen` in progress under Plan 270;
there is no M12 implementation, floodfill serving, or `caps=f` claim. Plan 272 proceeds on Plan 271 plus the Plan 281 type-5-deferred support floor (RouterInfo plus DatabaseStore types 1, 3, and 7); Plan 280 stopped with no acceptable maintained Rust provider for Red25519 (signature type 11), so type-5 EncryptedLeaseSet records remain deferred until a separately reviewed provider plan passes. No tier implies
public-network operation, production readiness, anonymity, or privacy guarantees.

## Source-to-code traceability

Every protocol module should identify:

- the dossier in this directory;
- the official specification path and pinned commit used during implementation;
- relevant proposal numbers;
- the external-standard revision, if any;
- the test-vector origin;
- deliberate deviations or stricter validation;
- any compatibility behavior inferred from implementation evidence.

This may be recorded in module documentation, a nearby `README`, test metadata, or an implementation plan. Avoid scattering unexplained protocol constants through runtime code.

## Decoder policy

Network, disk, reseed and local-API inputs are untrusted. Decoders must:

- enforce a caller-visible maximum before allocation;
- use checked arithmetic for offsets, lengths, counts and time computations;
- distinguish truncation, malformed encoding, unsupported type, semantic invalidity and policy rejection;
- consume exactly the expected input for strict top-level decoding;
- reject duplicate fields or keys where the format requires uniqueness;
- validate canonical ordering where signatures or hashes depend on canonical bytes;
- preserve the signed byte representation when reserialization could change verification semantics;
- avoid recursive structures without explicit depth limits;
- never panic on arbitrary bytes;
- avoid retaining attacker-controlled backing buffers after parsing unless bounded and intentional.

Unknown blocks or options may be ignored only when the specification explicitly defines forward-compatible skipping. The parser must still validate the enclosing length and resource bounds.

## Encoder policy

Encoders must:

- produce deterministic canonical output where the protocol defines canonicalization;
- reject values that cannot be represented without truncation;
- calculate exact encoded length before or during bounded emission;
- avoid implicit platform-width integer conversions;
- emit only capability/version combinations supported by the current runtime;
- keep private key material, session keys and plaintext authentication data out of logs and `Debug` output.

Round-trip tests are necessary but insufficient because two matching bugs may round-trip. Include fixed expected bytes and cross-implementation decoding.

## State-machine policy

Transport, NetDB, tunnel, garlic, streaming and API protocols must use explicit states and legal transitions. Each state machine must define:

- accepted messages/events per state;
- deadlines and retry budgets;
- duplicate and reordered input behavior;
- cancellation points;
- owned resources and cleanup on every terminal path;
- peer-visible errors or silent-drop behavior;
- whether malformed input terminates a message, session, transport link, tunnel build, destination or client connection.

No retry loop may be unbounded. Backoff, peer rotation and global concurrency limits must be tested under deterministic time.

## Cryptographic conformance

Do not implement cryptographic primitives locally. Wrap reviewed libraries with protocol-specific key and nonce types.

Tests must cover:

- official or independently verified positive vectors;
- invalid keys, signatures, tags and authentication data;
- nonce/counter boundary behavior;
- all-zero or low-order X25519 results according to the relevant specification/library contract;
- key-type and encoded-length mismatch;
- domain-separation and network-ID inputs;
- transcript/hash changes from one-bit mutations;
- key erasure or bounded lifetime where library support permits;
- failure without unauthenticated plaintext exposure.

Legacy algorithms required only for reading deployed data must be isolated from new identity generation and ordinary emission policy.

## Interoperability matrix

Each milestone should maintain an executable or machine-readable matrix similar to:

| Protocol | Direction/role | Java I2P | i2pd | I2P+ | Emissary/go-i2p | Evidence |
|---|---|---:|---:|---:|---:|---|
| NTCP2 | initiator | pending | pending | family duplicate | optional | test log/vector |
| NTCP2 | responder | pending | pending | family duplicate | optional | test log/vector |
| NetDB lookup | requester | pending | pending | family duplicate | optional | trace/result |
| Tunnel build | creator | pending | pending | family duplicate | optional | testnet artifact |
| Transit tunnel | participant / IBGW / OBEP | retained/deferred | Plan 258 multicell emission retained; Plan 259 endpoint-model conclusion corrected via retained-blocked Plan 260 (creator-owned build exhibited, receipt blocked on forward/sustainability boundaries); Plan 261 B-sender topology proven through counted-id addressing, receipt blocked on the B3 self-delivery loopback boundary; Plan 262 ownership corrected with receipt proven live (diag4 `0/6/1` tuple-bound) but full matrix stopped on sustainability; Plan 263 harness landed with receipt (`0/18/1`) + multicell (max 2) re-proven on `9bd2f39a` but single-mesh two-pass stopped on sustainability; Plan 264 lane + gate landed with 5 epochs 2/2 on `6ab9dc2d` but emission epochs stopped on windows; Plan 265 executed the manifest-v5 fixed budget on `4682920e` with zero semantic contradictions and closed `ibgw-data` (4/8) + `participant-lifecycle` (5/8) while `receipt` reached 1/8; Plan 266 executed the ladder contract on `a9803ca` with zero semantic contradictions and reached `receipt` 1/8 with every rung-6 attempt an anchored accepted-id drop population; Plan 267 executed the disposition contract on `315fb0d` with zero semantic contradictions and reached `receipt` 0/8 with 427 drop rows dominated by accepted-yet-not-found with delays in minutes; Plan 268 executed the path contract on `cc9b40c` with zero semantic contradictions and closed `receipt` 3/8 with the install-path divergence falsified; full receipt-capable M11 qualification complete with exact-head ordinary CI green | family duplicate | optional | Plans 254-264 retained evidence + Plan 265 retained-blocked evidence (24 retained attempts, one qualification SHA, input-side opportunity classification, zero tolerated i2pr semantic contradictions) + Plan 266 retained-blocked evidence (8 retained attempts, ladder rows, zero semantic contradictions) + Plan 267 retained-blocked evidence (8 retained attempts, disposition/timing rows, zero semantic contradictions) + Plan 268 passed evidence (8 retained attempts, install-path rows, 3 completions, composition passed, zero semantic contradictions, exact-head CI green) |
| Streaming | connect/listen | retained/deferred full-router compatibility at Plan 247 | passed bidirectional matrix via Plan 193 | family duplicate | optional | Plan 193 external transcript + Plan 247 retained Java boundary |
| SAM | client-facing server | client tests | client tests | client tests | optional | protocol transcript |
| SSU2 | initiator/responder | pending (secondary debt) | direct IPv4 loopback both directions via Plan 161 lane | family duplicate | optional | `plans/closure/ssu2/161-status.md`, `tests/integration/ssu2/run-independent.sh` |
| Router I2NP preflight | daemon-owned dispatch/delivery | n/a (router-internal) | bidirectional DeliveryStatus control via Plan 184 lane (no tunnel/NetDB/Streaming claim) | n/a | n/a | `plans/closure/mixed-router-interop/184-status.md`, `tests/integration/m6-interop/run-preflight.sh` |
| I2CP | router-facing server | client tests | client tests | client tests | optional | protocol transcript; Plan 164 structural codecs (`crates/i2pr-api/src/i2cp/`), Plan 165 connection/session/option state machines (typed `ConnectionStateMachine`, canonical `SessionConfig` verification with injected clock, bounded option disposition + `DestinationConfig` projection, bounded `SessionRegistry` with reserve/commit/rollback, reconfiguration taxonomy, typed `I2cpAction` vocabulary), Plan 166 client-owned destination + Standard LeaseSet2 bridge (`DestinationOwnership`, `DestinationPublic`, non-`Clone` redacted `InboundDecryptionCapability`, atomic `install_client_lease_set2` with signature + lease ownership + expiry + decryption-key match, typed `LeaseRequest` and `I2cpAction::RequestVariableLeaseSet` sourced from real inbound tunnels), Plan 167 loopback server runtime (`crates/i2pr-daemon/src/i2cp.rs`: `0x2a` preamble, incremental `FrameDecoder`, per-connection `ChildScope`, supervised admission semaphore, bounded per-connection read/write ceilings, atomic `install_client_lease_set2`, single cleanup path; disabled by default, loopback-only), Plan 168 message data plane (`crates/i2pr-api/src/i2cp/data_plane.rs` adds the bounded `I2cpMessageOutcome` vocabulary, `I2cpDataPlaneAction::{EnqueueOutboundPayload, DeliverInboundPayload, ResolveDestinationLookup}`, `PendingStatusTable`, `InboundPayloadQueue` with `WIRE_OVERHEAD_BYTES = 14`, and per-session ceilings `MAX_PENDING_OUTBOUND_MESSAGES_PER_SESSION = 64`, `MAX_PENDING_STATUS_CORRELATIONS_PER_SESSION = 128`, `MAX_INBOUND_PAYLOAD_FRAMES_PER_SESSION = 64`, `MAX_INBOUND_PAYLOAD_BYTES_PER_SESSION = 64 KiB`, `MAX_DESTINATION_LOOKUP_HORIZON = 10 s`, `MAX_MESSAGE_EXPIRATION_HORIZON = 1 h`; `crates/i2pr-daemon/src/i2cp.rs` projects them into the existing `i2pr_client::DestinationRuntime::enqueue_outbound` seam and drains `MessagePayload` inbound frames through a `tokio::sync::Notify`; cross-session local loopback shortcut routes two active session destinations through the receiving session's inbound queue; `DestLookup` resolves through the local destination registry; `GetBandwidthLimits` returns the config-derived client ceiling; eighteen real-TCP black-box tests in `crates/i2pr-daemon/tests/i2cp_message_data_plane.rs` exercise every Plan 168 §11 case), and Plan 169 self-composed local product and hardening (`crates/i2pr-daemon/src/i2cp.rs` adds `handle_reconfigure_session` + `apply_reconfigure` + `ReconfigurationOutcome`; `I2cpSessionState::last_options` carries the atomic reconfigure baseline; `handle_destroy_session` drains the per-session Plan 168 data-plane bookkeeping synchronously so repeated DestroySession/CreateSession cycles retain zero inbound queue, status correlation, or outbound slot; 31 real-TCP black-box tests across `crates/i2pr-daemon/tests/i2cp_final_acceptance.rs` (5 tests), `crates/i2pr-daemon/tests/i2cp_adversarial_matrix.rs` (20 tests), and `crates/i2pr-daemon/tests/i2cp_resource_matrix.rs` (6 tests) cover every Plan 169 §4/§5/§6/§7 case; Plan 171 retains that surface and hardens the common per-connection terminal path (`handle_connection` shuts the TCP stream down explicitly before bookkeeping release; the strict wrong-preamble row plus its non-paused companion prove the close with zeroed baselines); SAM router-owned product regressions and the Plan 167 listener regression in `i2cp_loopback.rs` remain green, and Plan 170 independent clients and final closure (exact-pinned Java I2P 2.13.0 `9134f808337b401e8e53c73734c81fab04280c9d` and go-i2cp `b529ee1c10a6011558b4d69fc9436a4afc489eac` exchange digest-matched 25 B/32 KiB payloads both directions through the loopback daemon under `tests/integration/i2cp/run-independent.sh` (9 fail-closed rows) with `scripts/check-i2cp-acceptance-evidence.sh` enforced in routine CI and manual `.github/workflows/i2cp-external.yml`; daemon deltas are `ReplyAndFollowup` RequestVariableLeaseSet after CreateSession, `0.x.y` version negotiation, empty-auth GetDate acceptance, `messageReliability=none` best-effort mapping, and the ElGamal-legacy-slot policy relocation where SessionConfig/`DestinationPublic` accept the legacy slot and X25519 is enforced at `install_client_lease_set2`, fail-closed for legacy slots); no `HostLookup`/`HostReply` resolution, no remote-I2CP/public-network claim, Milestone 9 final acceptance closed via Plan 172 (experimental, loopback-only; Plan 170 wire/data-plane retained-passed, independent LeaseSet2 lifecycle passed-via-plan172 with 24 fail-closed rows) |

Interoperability tests must run only in an authorized private or controlled mixed-router testnet until the milestone plan explicitly permits public-network observation.

### Milestone 10 service-tunnel profile ledger

Bounded M10 application profile (experimental, loopback-only,
disabled by default; Plans 174–180 local product, Plan 182 local
round-trip corrective, Plan 181 independent-application-client
evidence retained, Plan 202 production remote
Destination/Streaming composition, Plan 203 positive remote HTTP
+ IRC application interop):

- generic TCP client/server tunnels with digest-matched byte
  round-trip (small, >=32 KiB multi-segment, reverse, half-close
  EOF propagation, siblings) and restart-stable persistent
  server destinations;
- HTTP/1.1 `.i2p` proxy plus CONNECT with the conservative
  privacy rewrite (User-Agent replaced, Referer/From stripped),
  `.i2p`-only targets, hop-by-hop stripping, smuggling
  rejection, and bounded typed 400/403/502 responses;
- SOCKS5 no-auth DOMAINNAME CONNECT with hostname-at-proxy
  semantics and the default 443-only port policy;
- IRC client profile with the runtime-neutral privacy filter
  (USER/PING/QUIT/PART rewrites, CTCP ACTION allowed, DCC and
  unsupported CTCP dropped, unknown commands dropped);
- IRC server profile with the authenticated peer-Destination
  hostname projection (`<52-char base32>.b32.i2p`) and bounded
  registration interception;
- bounded transactional reconcile with generation/draining
  lifecycle and unified cross-service resource accounting;
- a typed `ServiceDestinationDelivery` capability surface owned
  by the `ServiceTunnelManager` and installed once per daemon
  via `install_router_delivery_handle`; the typed
  `RoutingDecision::LocalCoOwned` / `RemoteRouter` /
  `RemoteUnresolved` enum drives the resolve path; the bounded
  `RemoteDeliveryCounters` surface emits twelve positive
  observations on every counted path; positive Direction A
  external drivers cover the `m10_remote_destination_streaming_composition`
  transport row and the `m10_positive_remote_http_and_irc_application_interop`
  application rows against exact-pinned i2pd 2.61.0.

Explicitly unsupported or deferred (fail-closed, never silently
bridged): clearnet outproxy; SOCKS UDP ASSOCIATE; SOCKS BIND;
SOCKS4; SOCKS username/password auth; transparent proxying;
HTTP/2 or HTTP/3 proxy termination; TLS interception; IRC DCC;
WEBIRC and cloaked-hostname extensions; arbitrary remote admin
exposure; general address-book/subscription management; transit
or floodfill router roles (M11/M12); broad public-network
interoperability beyond exactly demonstrated rows. Remote
independent-I2P HTTP/IRC service interop is now proven on the
dedicated M6 interop lane through Plan 203
(`passed-m10-positive-remote-http-and-irc-application-interop`),
the two `remote-independent-*` rows flip `blocked → passed-on-env`
when the lane provisions the SSU2 endpoint + bind tuple and the
driver emits the `http-remote-application-established` /
`irc-remote-application-established` evidence keys. M10 final acceptance is authoritative through the hosted Plan 215 re-verification of Plan 214. Plan 248 supersedes Plan 204's Java-dependent convergence gate without relabeling the Java lane. See `plans/closure/service-tunnels/204-status.md`,
`plans/closure/service-tunnels/203-status.md`, and `plans/closure/service-tunnels/202-status.md`.

## Fuzzing targets

At minimum, fuzz:

- all top-level common-structure and I2NP decoders;
- RouterInfo, Destination, LeaseSet and signed-container parsing;
- NTCP2 and SSU2 plaintext block parsers after authenticated decryption;
- handshake state transition inputs with deterministic crypto seams where safe;
- tunnel build records and tunnel message fragmentation/reassembly;
- garlic clove and ECIES payload parsing;
- streaming packets and option blocks;
- SAM and I2CP framing, tokenization and option parsing;
- HTTP proxy request-line/header rewriting and SOCKS negotiation.

Fuzz harnesses must have bounded input size and should assert no panic, no excessive allocation, no infinite loop and stable error classification where practical.

## Differential tests

Use differential testing selectively. Valuable comparisons include:

- canonical structure serialization;
- Base64/Base32 and hash derivation;
- signature verification and RouterInfo hashes;
- NTCP2/SSU2 block encoding after supplying identical keys/nonces;
- tunnel build-record crypto;
- streaming packet encoding;
- SAM command parsing and response status.

Do not expose private test keys to public infrastructure or depend on nondeterministic production routers for unit tests. Prefer local fixtures and dedicated test identities.

## Security regression corpus

Every protocol parser should retain minimized fixtures for discovered failures:

- truncation at every field boundary;
- maximum and maximum-plus-one lengths/counts;
- duplicate, unknown and out-of-order fields;
- expired, future-dated and skewed timestamps;
- invalid signatures and authenticated-encryption tags;
- replayed handshakes, packets, I2NP IDs and tunnel records;
- decompression/archive expansion limits for reseed bundles;
- fragmented messages exceeding per-message or per-peer budgets;
- slow-read/slow-write behavior and partial frames;
- cancellation during cryptographic work, persistence and publication.

A production bug is not closed until a fixture or deterministic test prevents recurrence.

## Capability advertisement

Capability publication is a security and interoperability contract. Before changing `router.version`, RouterInfo capabilities, transport addresses/options, LeaseSet type support, SAM version negotiation or I2CP behavior:

1. identify the exact feature implications in the official specifications;
2. verify all implied mandatory behavior is implemented;
3. add mixed-router tests for the changed claim;
4. test downgrade/unsupported peers;
5. update the relevant dossier and protocol-support matrix.

`i2pr` should initially advertise the lowest truthful current feature level compatible with its implemented subset, not mimic another router’s release string.

## Evidence retention

Store stable protocol vectors and minimized malformed fixtures in the repository. Store large captures, generated testnets and sensitive operational logs outside Git history, with scripts and hashes sufficient to reproduce them. Redact live peer identities, IP addresses, destination keys, session keys and potentially identifying timing data before retaining or publishing artifacts.