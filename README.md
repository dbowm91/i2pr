# i2pr

An experimental I2P router written in Rust. **Not production-ready.** Not suitable for anonymity, privacy, censorship resistance, or security-sensitive workloads.

## Status

`i2pr` is under active development. The workspace builds and passes tests, but significant work remains before the router is functional on the I2P network.

**Implemented:**
- Bounded wire protocol codecs (`i2pr-proto`)
- Cryptographic wrappers: Ed25519, X25519, AES, ChaCha20-Poly1305, HMAC, SipHash, HKDF-SHA256 (`i2pr-crypto`)
- Versioned private-identity storage (`i2pr-storage`)
- Runtime-neutral transport and service contracts (`i2pr-transport`, `i2pr-transport-ntcp2`)
- Tokio-owned runtime with supervision, cancellation, and bounded channels (`i2pr-runtime`)
- Deterministic testkit with seeded randomness and fault injection (`i2pr-testkit`)
- RouterInfo validation, bounded local NetDB store, and local signed RouterInfo construction (`i2pr-netdb`)
- Persistent RouterInfo cache, SU3 reseed verification, and reseed ingestion (`i2pr-netdb-persist`)
- Transport-neutral lookup, store, and publication state machines (`i2pr-netdb`)
- Daemon bootstrap integration with `NetDbSeam` (`i2pr-daemon`)
- Exploratory tunnel substrate, pool, and reply-path provider (`i2pr-tunnel`). The Q0 independent-consumer seam (pinned Emissary) has been exercised against the production `ShortBuildStateMachine` + `ShortBuildI2npBridge`; the OBEP returned TunnelGateway + Garlic inner message. Q1/Q2 and external delivery are still pending.
- ECIES-X25519 short tunnel-build cryptography — Plan 111 final
  local short-build conformance correction landing plus the Plan
  112 outbound pre-delivery closure. Outbound construction is
  locally conformant against the current official I2P Tunnel
  Creation Specification: canonical Noise-N null-prologue
  `MixHash`, single-HKDF `es` derivation, 218-byte encrypted
  envelope, 202-byte reply plaintext, slot byte at offset 4 of
  the 12-byte nonce, 8-byte OBEP garlic reply tag, explicit
  per-hop tunnel IDs, role-aware `MessageHopProcessor`, frozen
  independent fixed vectors, CSPRNG-filled post-Mapping padding,
  Plan 112 direction/role topology validation, Plan 112 STBM/OTBRM
  count-prefixed contract helper, Plan 113 deployed-reference-
  compatible inbound construction: the real request retains the
  fixed fields + Mapping/padding layout, while exactly one
  originator fake carries `hash16 || fresh X25519 pub32 || random
  remainder` with creator-side integrity verification. This is
  reference-compatible for the unresolved final-spec prose, not a
  strict final-spec conformance claim for that one semantic, and
  Plan 114 terminal-routing and tunnel-chain correction: explicit
  outbound `outbound_reply_router` and inbound
  `originator_hash` terminal-routing metadata, intermediate
  `hops[i].next_tunnel == hops[i+1].receive_tunnel` chain
  continuity enforced at both the high-level
  `ShortBuildPath::validate()` boundary and the public
  lower-level `prepare_short_build_message()` entry point, and
  strict outbound/inbound E2E trajectories that deterministically
  reach `Established` without the prior permissive acceptance.
  See
  [`plans/111-short-build-final-local-conformance-correction.md`](plans/111-short-build-final-local-conformance-correction.md),
  [`plans/112-113-post-plan111-pre-delivery-corrective-roadmap.md`](plans/112-113-post-plan111-pre-delivery-corrective-roadmap.md),
  [`plans/112-outbound-short-build-pre-delivery-closure.md`](plans/112-outbound-short-build-pre-delivery-closure.md),
  [`plans/112-status.md`](plans/112-status.md),
  [`plans/113-status.md`](plans/113-status.md),
  [`plans/114-status.md`](plans/114-status.md),
  and the pinned evidence note
  [`specs/references/short-build-inbound-creator-key.md`](specs/references/short-build-inbound-creator-key.md),
  the conformance fixture in
  [`crates/i2pr-tunnel/src/conformance_fixtures.rs`](crates/i2pr-tunnel/src/conformance_fixtures.rs),
  the frozen fixed-vector oracle in
  [`crates/i2pr-tunnel/src/fixed_vectors.rs`](crates/i2pr-tunnel/src/fixed_vectors.rs),
  and the Rust-only reference provenance test in
  [`crates/i2pr-tunnel/tests/plan111_reference_vectors.rs`](crates/i2pr-tunnel/tests/plan111_reference_vectors.rs).
- Plan 115 canonical production I2NP bridge:
  `ShortBuildI2npBridge::wrap_deliver_action` consumes a
  `ShortBuildAction::Deliver`, validates the
  `1 + count * 218` count-prefixed STBM body, splits the count
  byte from the raw records, builds
  `DeferredBuildRecords::new(count, 218, …)`, wraps in
  `I2npBody::ShortTunnelBuild`, encodes with the requested
  standard or short-transport I2NP header, and round-trips
  through the standard-header decoder to assert the recovered
  body equals the original count-prefixed payload exactly. The
  bridge never double-prefixes the STBM record count, never
  mutates, reorders, or regenerates records, and never logs raw
  record bytes. Plan 115 closes as
  `passed-emissary-q0-construction-and-obep-reply-only`:
  the i2pr-emitted STBM is consumed by Emissary's native
  short-build handler (the same code path Emissary uses in
  production), Emissary replies with a `TunnelGateway` wrapping
  a Garlic inner message, and a feedback channel is returned.
  Plinned Emissary revision: `9b43484a21d5a1291c4881cdae62a36c527f8c0f`
  (emissary-core 0.4.0). Q1 (authenticated transport delivery)
  and Q2 (reply round-trip to `Established`) remain pending and
  depend on a qualified external delivery lane. See
  [`plans/115-status.md`](plans/115-status.md) and
  [`plans/115-handoff.md`](plans/115-handoff.md).
- Runtime-neutral `ShortBuildStateMachine`, success-only
  `ShortBuildRegistrar`, and deterministic `DeterministicResponder`
  peer simulator (`i2pr-tunnel`)
- Plan 116 local tunnel data plane + final closure + terminal
  cleanup pass: real `EstablishedMaterial` transfer from
  `ShortBuildStateMachine` into `ExploratoryPool`,
  `#[cfg(test)]`-only placeholder APIs, canonical I2P Tunnel
  Message Specification fragment overheads, first-fragment
  delivery retention through reassembly, exact-duplicate
  no-op accounting (`T1`), first-delivery metadata in duplicate
  identity (`T2`), and the full outbound-to-inbound tunnel
  trajectory with exact-byte equality for both the unfragmented
  and the fragmented cases, including the out-of-order
  fragmented trajectory (`T3`) (`i2pr-tunnel`; see
  [`plans/116-status.md`](plans/116-status.md))
- Plan 117 exploratory NetDB composition (closed for
  progression with reference evidence gap): Phases A–F land
   the typed `DatabaseLookupMessage` and `DatabaseStoreMessage`
   carriers on `LookupAction` and `PublicationAttemptRecord`,
   the metadata-retaining one-shot `ExploratoryPool::activate`
   seam, the bounded `DataPlaneRegistry` for activated local
   roles, the daemon `NetDbSeam` composition state machine, the
   outbound `OutboundGatewayRole` `DatabaseLookup`/`DatabaseStore`
   tunnel-data composition, and the inbound
   `LocalInboundEndpointRole` `TunnelData` dispatch
   (`crates/i2pr-daemon::inbound_dispatch`,
   `crates/i2pr-daemon::outbound_lookup`).
   The corrective closure plan
   ([`plans/117-corrective-closure.md`](plans/117-corrective-closure.md))
   corrected the four routing/framing/activation/readiness
   defects (C1–C4), proved the all-i2pr production-seam
   trajectory with real `EstablishedMaterial` (Phase G), and
   achieved `passed-emissary-wire-format-compatibility` against
   pinned Emissary revision
   `9b43484a21d5a1291c4881cdae62a36c527f8c0f` (historical parser
   evidence). The corrected in-tree native test reaches Emissary
   OBEP admission and reply AEAD opening but rejects the pinned
   reference's request-prefixed reply plaintext during strict
   i2pr Mapping decoding, so native publication/lookup evidence
   is not claimed. Plan 117 closes as
   `closed-for-progression-with-evidence-gap` per Plan 118;
   `router_construction = may-continue`. The next executable
   plan is **Plan 119** (LeaseSet2 protocol foundation) under
   [`plans/118-123-milestone6-router-construction-roadmap.md`](plans/118-123-milestone6-router-construction-roadmap.md);
   see [`plans/117-status.md`](plans/117-status.md) and
   [`plans/117-handoff.md`](plans/117-handoff.md).
 - Multi-record short tunnel-build construction: randomized slot
  allocation, originator + padding fake records, raw ChaCha20
  preprocessing/postprocessing (slot byte at offset 4 of the
  12-byte nonce), and the one-byte-count STBM/OTBRM payload
  framing validated through
  `validate_count_prefixed_short_payload` /
  `encode_count_prefixed_short_payload` (Plan 110 closed; Plan
  109 corrected the byte-11 regression to byte 4; Plan 112 made
  the count-prefixed contract explicit and makes the state-machine
  delivery action validate the exact prefix and payload length)
- Multi-record short tunnel-build construction: randomized slot
  allocation, originator + padding fake records, raw ChaCha20
  preprocessing/postprocessing (slot byte at offset 4 of the
  12-byte nonce), and the one-byte-count STBM/OTBRM payload
  framing validated through
  `validate_count_prefixed_short_payload` /
  `encode_count_prefixed_short_payload` (Plan 110 closed; Plan
  111 corrected the byte-11 regression to byte 4; Plan 112 made
  the count-prefixed contract explicit and makes the state-machine
  delivery action validate the exact prefix and payload length)
- CLI daemon with config validation, identity generation, and dry-run (`i2pr-daemon`)

**Not implemented:**
- Live NTCP2 or SSU2 transport (NTCP2 experimental, non-advertised)
- Live mixed-router tunnel build execution (depends on a qualified
  external delivery lane)
- NetDB lookup/publication over the network (Plan 117 §8/§10
  composition is local-only; the exploratory outbound path is
  wired through `DataPlaneRegistry` and `OutboundGatewayRole`,
  but the network transport adapter still owns the NTCP2/SSU2
  handshake surface)
- I2NP message handling and router dispatch
- Streaming, SAM, I2CP (the Milestone 6 frontier — see
  [`plans/118-123-milestone6-router-construction-roadmap.md`](plans/118-123-milestone6-router-construction-roadmap.md))
- Client proxies (HTTP, SOCKS5)
- Any network-facing behavior

The NTCP2 development interoperability result is `protocol-defect-localized` at `noise_authenticated`. No passed mixed-router NTCP2 result exists.

Plan 115 Emissary Q0 construction + native OBEP reply: passed locally (`plans/115-status.md`).

Plan 117 status: `closed-for-progression-with-evidence-gap` per Plan 118. The Plan 117 local production composition (Phase G) is retained; the corrected native reference test reached Emissary OBEP admission and reply AEAD opening but rejected the pinned reference's request-prefixed reply plaintext during strict i2pr Mapping decoding. The reference-side defect is localized to the pinned Emissary revision; no upstream correction is available.

Plan 119 status: `passed-leaseset2-protocol-foundation` (`plans/119-status.md`). The ordinary online-signed published Standard LeaseSet2 carrier is wired into `i2pr-proto` (40-byte `Lease2`, `LeaseSet2Header`, `LeaseSet2EncryptionKey`, canonical `Mapping` options, signature domain `0x03 || signed_bytes`) and into `i2pr-netdb` (`ValidatedLeaseSet2`, `LeaseSet2Store`, `DestinationHash`, `LookupKind::LeaseSet2`). `DatabaseStoreData::LeaseSet2` replaces the type-3 `Deferred` payload for the ordinary subset; types 5/7 remain explicitly deferred.

Plan 120 status: `passed-destination-lifecycle-and-pools` (`plans/120-status.md`). Plan 120 lands the first `i2pr-client` destination runtime: local destination identity (independent Ed25519 signing + X25519 static keys, non-`Clone`, non-`Debug` secrets), destination-specific tunnel pools that consume real one-shot `EstablishedMaterial`, local Standard LeaseSet2 construction and signing with self-validation through `i2pr-netdb`, LeaseSet2 lifecycle with bounded rotation/withdrawal, bounded local payload contracts that never inject plaintext into tunnel delivery, and a router-local destination registry with explicit capacity and duplicate-rejection guards. The `plan_120_deterministic_local_trajectory` integration test drives the full production-seam trajectory through `i2pr-tunnel` and `i2pr-netdb`.

Plan 121 status: `passed-ecies-destination-session-layer` (`plans/121-status.md`). Plan 121 lands the first real ECIES-X25519-AEAD-Ratchet destination Garlic/session layer: the `curve25519-elligator2 = 0.1.0-alpha.2` primitive audit (Plan 121 §2 / §12), the wrapped ECIES primitives in `i2pr-crypto`, the bounded structural Garlic payload block codec in `i2pr-proto` (`EciesPayloadSequence` with DateTime-first / Garlic Clove / last-only Padding policy), and the bounded destination-context `EciesSessionManager` in `i2pr-client`. Plan 121's i2pr-internal ECIES dialect (flag-byte framing, per-session random "static" keys, single shared tag chain) was superseded by **Plan 126**; see below.

Plan 126 status: `passed-ecies-destination-ratchet-corrective-foundation` (`plans/126-status.md`). Plan 126 replaces the superseded Plan 121 destination ECIES dialect with the normative I2P ECIES-X25519-AEAD-Ratchet contract: bound New Session (Elligator2 ephemeral representative, static-key section carrying Alice's derived public key, no flag bytes), one-shot SessionReplyTags New Session Reply window, Noise Split into directional `k_ab`/`k_ba` tag sets with `AttachPayloadKDF`, canonical tag/key index alignment (tags 1-based on wire, keys/nonces 0-based), ES AEAD with tag associated data and `0x00000000 || LE64(index)` nonces, and typed rejection of unbound New Sessions and duplicate ephemerals. The corrected primitives live in `crates/i2pr-crypto/src/ecies.rs` (31 frozen independently-derived conformance vectors, provenance in [`specs/references/ecies-destination-ratchet.md`](specs/references/ecies-destination-ratchet.md)); the corrected manager lives in `crates/i2pr-client/src/session.rs` (paired sessions keyed by remote static key, bounded remove-on-hit inbound tag windows, pre-derived pending reply windows, provisional responder state). Plan 126 does not restore the Plan 121/122/124 final-closure claims until Plan 127 proves destination binding through tunnels. The manager-level trajectory is `plan_126_full_manager_lifecycle_bidirectional_exact_once`; the primitive-level trajectory is `plan_126_corrected_deterministic_local_trajectory`. NTCP2 stays disabled; nothing is advertised.

Plan 127 status: **`passed-destination-session-routing-final-closure`** ([`plans/127-status.md`](plans/127-status.md)). Plan 127 composes the corrected Plan 126 ratchet with Standard LeaseSet2 binding, destination-owned tunnel pools, and the Plan 124 Garlic-through-tunnel path, closing the Plan 121/122/124 local destination-layer gaps with the word `local`: `plan_121 = passed-corrected-ecies-destination-session-layer-local`, `plan_124 = passed-corrected-destination-routing-local-closure`. The session manager now owns an unambiguous outbound form state machine (`PlannedOutboundForm`: retained NSR context → live Existing Session → fresh bound New Session) exposed through `encrypt_to_remote` and typed `form_name()` diagnostics; `compose_outbound_delivery` builds the payload per planned form, so every fresh bound New Session bundles the local destination's current signed Standard LeaseSet2 (`SendError::MissingBundledLeaseSet2` otherwise). The dispatcher enforces the Plan 127 §2 binding order — authenticate/decrypt, decode all payload blocks, require exactly one bundled sender LeaseSet2 validated under its **own contained Destination hash**, verify its usable type-4 X25519 key equals the authenticated NS static key, and only then bind — deriving the remote identity exclusively from the validated record (never from static-key bytes or tags) and dropping the reply context on any binding failure so no NSR can be emitted for an unbindable session. Reverse routing uses the explicit `DestinationRouting::install_remote_lease_set2` typed handoff (router-side store + active-remote cache in one step, bounded by the new `MAX_ACTIVE_REMOTES` ceiling). The master trajectory `plan_127_master_trajectory_ns_nsr_es_bidirectional_exact_once` drives two destinations through real participant→OBEP and IBGW→endpoint chains, the exact-byte `authenticated-router-link-bypassed-local-seam`, LS2 binding, retained-context NSR, and four exact-once Existing Session messages; fifteen further deterministic tests cover every §9 failure case (key mismatch, invalid signature, expired/malformed/missing LS2, tamper, wrong-tag/replayed NSR, unknown/tampered/replayed ES, wrong owner without trial decryption, removed owner, queue-full, expiry, ceiling enforcement). No Streaming logic participates; no external interoperability is claimed.

Plan 128 status: **`passed-streaming-wire-protocol-corrective-closure`** ([`plans/128-status.md`](plans/128-status.md)). Plan 128 corrects the Streaming packet wire format below the retained Plan 125 RFC 1952 gzip layer to match the current I2P Streaming specification: the normative flag map (`SYNCHRONIZE 0x0001` through `OFFLINE_SIGNATURE 0x0800`, reserved `0xF000` rejected on receipt) with M6 policy sets `INITIAL_SYN_FLAGS = 0x04A9`, `SYN_RESPONSE_FLAGS = 0x00A9`, `CLOSE_FLAGS = 0x000A`, and `RESET_FLAGS = 0x000C`; removal of every invented option-data TLV (type/length records are gone; unparsed trailing option bytes fail closed); a flag-driven `StreamingOptions` codec that writes DELAY / FROM / MAX / SIGNATURE in normative order with `MAX_PACKET_SIZE` as a 2-byte big-endian integer bounding the **payload only** (`DEFAULT_ADVERTISED_MAX_PAYLOAD = 1730`; full packet bounds are an independent checked sum, not `1730 - 22`); variable-length raw final signatures whose length comes from signing-key context (the FROM destination, or the peer key retained in connection state for established-connection CLOSE/RESET without FROM since 0.9.20) instead of a hard-coded 64-byte TLV; and the canonical preimage that signs the complete packet once over zeroed signature-placeholder bytes (`encode_with_placeholder` + `install_packet_signature`) - verification zeroes exactly the located signature bytes. The initial SYN carries eight Proposal 164 replay NACK words holding the receiver Destination hash covered by the signature; the SYN response carries zero NACKs, no NO_ACK, and a valid `ackThrough`; `validate_syn_policy` is split into `validate_initial_syn` / `validate_syn_response`. Connections retain the peer signing key; CLOSE/RESET require signatures and verify against the retained peer identity (fixing the previous wrong-side local-key RESET verification); unknown standalone signed control fails closed. Negotiation is `min(local advertised, remote advertised)` exercised with intentionally different values in tests. Wire fixtures live in `crates/i2pr-proto/tests/plan128_wire.rs` with provenance in [`specs/references/streaming-packet-wire.md`](specs/references/streaming-packet-wire.md); manager trajectories live in `crates/i2pr-client/tests/plan128_trajectory.rs`. No destination-tunnel, SAM, transport, or live-network work is introduced; Streaming interoperability is not claimed. The next executable plan is **Plan 129** (integrated Milestone 6 local-product gate) per [`plans/129-m6-integrated-destination-streaming-final-gate.md`](plans/129-m6-integrated-destination-streaming-final-gate.md).

Plan 122 status: `passed-corrected-local-destination-routing` (`plans/122-status.md`). Plan 122 composes the Plan 119 LeaseSet2 NetDB surface, the Plan 120 destination runtime, the Plan 121 ECIES Garlic session layer, and the Plan 116 tunnel data plane into a complete local destination routing pipeline. Phases A–J land: the `LookupEngine` extended with `handle_database_store_lease_set2` and a `LookupResult::LeaseSet2Success` variant; the daemon `NetDbSeam` extended with `begin_lease_set2_lookup`, `ingest_lease_set2_response`, `cancel_lease_set2_lookup`, and a dedicated lease-set reply-path provider (`i2pr-daemon`); the bounded `LeaseSelector` and `LeaseSelectionPolicy` in `i2pr-client` (`lease_selection.rs`); the typed `OutboundRequest` builder, the `compose_outbound_delivery` planner, and the `DestinationRouting` cache in `i2pr-client::routing`; the `DestinationDispatcher` in `i2pr-client::dispatch` with Garlic decryption, bundled LeaseSet2 validation, and per-destination inbound queues; and the deterministic local end-to-end `plan_122_two_destination_local_composition` trajectory covering lease selection, LeaseSet2 cache insertion, outbound composition, and dispatcher rejection of unsigned Garlic envelopes. The bounded router-delivery seam produces `OBGWRouterDelivery` cells for the future transport adapter; the only transport omission is the explicit `authenticated-router-link-bypassed-local-seam` label Plan 122 calls for. The next executable plan is **Plan 123** (minimal streaming core) per [`plans/118-123-milestone6-router-construction-roadmap.md`](plans/118-123-milestone6-router-construction-roadmap.md).

Plan 123 status: `passed-corrected-minimal-streaming-local` (`plans/123-status.md`, restored by Plan 125). Plan 123 lands the first I2P Streaming core: a strict synchronous, deterministic, Tokio-free `StreamingManager` in `i2pr-client::streaming` with a per-destination outbound and inbound connection table, listener backlogs, send/receive window and congestion policies, retransmit/timeout policy, and a typed event surface. The wire codec lives in `i2pr-proto::streaming` (`StreamingPacket`, `StreamingPacketBuilder`, `StreamingFlags`, signed-SYN replay-binding NACK hash that binds against the receiver's local destination hash, signed-CLOSE and signed-RESET option region with Ed25519 signature, canonical `build_signature_preimage`, and the protocol-6 `ClientPayload` envelope with the canonical RFC 1952 gzip wire format — no SHA-256 integrity prefix, no custom compressed-length prefix, bounded decompressed-size enforcement, explicit trailing-byte rejection). Sixteen deterministic integration tests in `crates/i2pr-client/tests/plan123_trajectory.rs` cover the signed SYN payload round trip, canonical preimage signature verification, SYN replay binding rejection, MAX_PACKET_SIZE_INCLUDED policy, corrupt-signature rejection, the full two-destination SYN → data → CLOSE trajectory, loss recovery via retransmit, duplicate packet idempotence, RESET termination, send window backpressure, connection table ceiling, and signed-CLOSE / signed-RESET packet shapes. Plan 123 remains runtime-neutral: it composes with Plan 122's destination routing for outbound delivery and Plan 122's dispatch for inbound routing; it never owns sockets, timers, or DNS. Six additional deterministic tests in `crates/i2pr-client/tests/plan125_trajectory.rs` cover the real Plan 125 §6/§7 SYN / SYN-response lifecycle, the RFC 1952 gzip wire format, the stream-id ownership contract, the `SystemClock` monotonicity fix, and the listener outcome surface.

Plan 125 status: `passed-milestone6-local-corrective-closure` (`plans/125-status.md`). Plan 125 closes Milestone 6 as the local-product gate. It replaces the custom Plan 123 `ClientPayload` framing with the canonical RFC 1952 gzip wire format (no SHA-256, no custom compressed-length prefix, with frozen independently-derived test fixture), restores the real Plan 125 §6/§7 SYN / SYN-response lifecycle (originator SYN uses `sendStreamId = 0`; the recipient emits a signed SYN response with `sendStreamId = originator receiveStreamId` and `receiveStreamId = recipient-selected id`; both sides transition to `Established` only after the response path completes), splits stream-id ownership between `local_receive_stream_id` and `peer_receive_stream_id`, negotiates the maximum packet payload as `min(local, remote)`, fixes the broken `SystemClock` to anchor at an origin `Instant` and report elapsed time, and adds the runtime-neutral `StreamingDestinationAdapter` (`crates/i2pr-client/src/streaming_adapter.rs`) that bridges `TransportSendRequest` into the Plan 122 `compose_outbound_delivery` pipeline. The Plan 123 VirtualWire tests are retained as fast Streaming-only fault tests; the new `plan125_trajectory` tests cover the real handshake lifecycle, the gzip wire format, the listener outcome, and the bidirectional `inbound_by_stream` / `outbound_by_stream` connection lookup. Plan 125 does not enable NTCP2, SAM, I2CP, Docker, namespaces, Multipass, or any public-network access. The next executable product work is the SAM baseline planning (Milestone 7) per the Milestone 6 router-construction roadmap.

Plan 124 status: `passed-plan122-corrective-closure` (`plans/124-status.md`). Plan 124 corrects the Plan 122 composition defect where `compose_outbound_delivery` built an ECIES Garlic envelope but fed the plaintext inner I2NP `Data` envelope into the outbound tunnel role. The corrected composition now wraps the encrypted envelope in an `I2npBody::Garlic` carrier and feeds the standard-encoded I2NP Garlic message bytes into `OutboundGatewayRole::forward_cells`. `OutboundDeliveryPlan` exposes `garlic_i2np_bytes: Vec<u8>` as the canonical carrier the tunnel data plane must observe; `inner_envelope_bytes` is retained for diagnostic comparison only. The new byte-identity regression test `plan_124_phase_a_b_compose_emits_garlic_through_obep` proves the OBEP recovery equals `garlic_i2np_bytes` and never equals `inner_envelope_bytes`. The successful A → B trajectory `plan_124_trajectory_a_to_b_carries_garlic_through_obep` drives two destination identities with real outbound and inbound `EstablishedMaterial` through every tunnel role, the canonical local seam `authenticated-router-link-bypassed-local-seam`, and `DestinationDispatcher` to surface the exact application payload. Eleven Plan 124 deterministic tests in `crates/i2pr-client/tests/plan124_trajectory.rs` cover Phases A, B, C, D (existing-session carrier), E (ciphertext isolation, unregister atomically drops ownership), F (stale lease), and G (tampered / malformed / non-Garlic fault paths). The `DestinationDispatcher` now binds `DestinationId` → `DestinationHash` through `bind_destination_hash`; `lookup_local_destination` fails closed on `UnknownDestination` without trial-decryption across all registered destinations. NTCP2 remains experimental and non-advertised; Plan 124 does not enable NTCP2, SAM, I2CP, Docker, namespaces, Multipass, or any public-network access. The next executable plan is **Plan 125** (Streaming protocol-6 framing + canonical gzip/CRC32 + reply round-trip) per [`plans/118-123-milestone6-router-construction-roadmap.md`](plans/118-123-milestone6-router-construction-roadmap.md).

## Workspace

```text
crates/
  i2pr-proto/               Wire types, codecs, constants, validation, Standard LeaseSet2 carrier (Plan 119)
  i2pr-crypto/              Protocol-specific cryptographic wrappers
  i2pr-storage/             Atomic persistence and migration support
  i2pr-core/                Shared contracts, lifecycle, budgets, health
  i2pr-transport/           Transport-neutral link management and selection
  i2pr-transport-ntcp2/     NTCP2 protocol implementation (no I/O)
  i2pr-runtime/             Tokio-owned supervision, cancellation, and I/O
  i2pr-netdb/               RouterInfo validation, NetDB store, lookup, publication, Standard LeaseSet2 validation and bounded store (Plan 119)
  i2pr-netdb-persist/       Persistent cache and SU3 reseed ingestion
  i2pr-tunnel/              Tunnel identity, exploratory pool, ECIES-X25519 short-build cryptography (Plan 111/112 outbound local conformance; Plan 113 inbound reference-compatible policy; Plan 114 terminal routing and tunnel-chain correction; Plan 115 canonical production I2NP bridge; Plan 116 final local closure + terminal cleanup), runtime-neutral build state machine, reply-path provider, Plan 117 outbound/inbound exploratory NetDB composition
  i2pr-client/              Local destination identity, dedicated tunnel pools, signed Standard LeaseSet2 generation and lifecycle, bounded local payload contracts, router-local destination registry (Plan 120), ECIES-X25519-AEAD-Ratchet destination Garlic session layer with bound NS/NSR/ES lifecycle and LeaseSet2 sender binding (Plan 121/126/127), destination routing / LeaseSet2 NetDB composition with production reverse routing (Plan 122, corrected by Plan 124/127), protocol-correct I2P Streaming core with signed SYN/SYN-response handshake, sequence/ACK/NACK, retransmit, congestion, and the `StreamingDestinationAdapter` bridging into Plan 122 (Plan 125)
  i2pr-daemon/              CLI, configuration, composition, supervision, Plan 117 outbound/inbound dispatch
  i2pr-testkit/             Deterministic simulation and adversarial fixtures
tools/
  i2pr-interop/             Non-production interop launcher (test only)
```

## Building and testing

Requires Rust 1.95.0 (pinned via `rust-toolchain.toml`); MSRV is 1.88.

```text
cargo fmt --all --check
cargo check --workspace --all-targets
cargo test --workspace
cargo clippy --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

## Architecture

**Modular monolith** — one process composed from focused crates. Crate boundaries follow security boundaries and protocol ownership.

**Wire compatibility** — protocol codecs and crypto state machines are separate from router policy. Peer selection, transit, and resource allocation vary by profile without changing wire behavior.

**Explicit trust boundaries** — all network, client, and configuration inputs are untrusted until validated. Subsystems receive only the capabilities they require.

**Bounded execution** — queues, buffers, handshakes, sessions, tunnel builds, and API clients have explicit limits with deadlines, cancellation, and cleanup.

**Defensive Rust** — safe Rust by default; `unsafe` forbidden in protocol and crypto crates. Secret-bearing types avoid logging, cloning, and serialization.

**Testability** — deterministic clocks, seeded randomness, in-memory transports, and fault injection are first-class.

## MVP direction

The feature MVP includes: CLI router daemon, persistent identity, I2NP handling, NTCP2/SSU2 transport, NetDB client/floodfill, tunnel construction, destination/LeaseSet management, streaming, SAM/I2CP interfaces, HTTP/SOCKS5 proxies, and bounded resource accounting.

Development targets a smaller interoperable-router milestone before the complete MVP. The current product milestone is **Milestone 6** (destinations, garlic, LeaseSet2, streaming), sequenced in [`plans/118-123-milestone6-router-construction-roadmap.md`](plans/118-123-milestone6-router-construction-roadmap.md) and its corrective roadmap [`plans/126-129-milestone6-final-corrective-roadmap.md`](plans/126-129-milestone6-final-corrective-roadmap.md). Plan 119 closed as `passed-leaseset2-protocol-foundation`; Plan 120 closed as `passed-destination-lifecycle-and-pools`; Plan 121 closed as `passed-ecies-destination-session-layer` (superseded dialect corrected by Plan 126, final local closure restored by Plan 127); Plan 122 closed as `passed-corrected-local-destination-routing` (Plan 124 corrective closure); Plan 123 closed as `passed-corrected-minimal-streaming-local` (restored by Plan 125); Plan 124 closed as `passed-corrected-destination-routing-local-closure`; Plan 125 closed as `passed-milestone6-local-corrective-closure`; Plan 126 closed as `passed-ecies-destination-ratchet-corrective-foundation`; Plan 127 closed as **`passed-destination-session-routing-final-closure`**; Plan 128 closed as **`passed-streaming-wire-protocol-corrective-closure`** (restoring `plan_123 = passed-corrected-streaming-wire-local`). Milestone 6 local product is **not closed**: the next executable plan is **Plan 129** (integrated Milestone 6 local-product gate) per [`plans/129-m6-integrated-destination-streaming-final-gate.md`](plans/129-m6-integrated-destination-streaming-final-gate.md).

## License

No license selected yet. Do not copy code from I2P+, i2pd, Emissary, or other routers until license compatibility is reviewed. Specifications and observed behavior may be used for clean-room implementation.
