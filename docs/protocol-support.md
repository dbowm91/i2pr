# Protocol support matrix

Current Plan 118 override (2026-08-19): Plan 117 is closed for progression
with the typed disposition `closed-for-progression-with-evidence-gap`. The
local Phase G production composition remains passed; the corrected in-tree
Emissary test reaches native OBEP admission and reply AEAD opening, then
rejects the pinned reference's request-prefixed reply during strict i2pr
Mapping decoding. The reference-side defect is localized to the pinned
Emissary revision
`9b43484a21d5a1291c4881cdae62a36c527f8c0f`; Plan 118 Phase B1 confirmed
no usable upstream correction exists, so the bounded Plan 117 native
closure campaign is closed for progression. Native mixed-router NetDB
evidence is not claimed. Router construction is `may-continue`. Plan 119
closed as `passed-leaseset2-protocol-foundation` per
[`plans/119-status.md`](../plans/119-status.md); the ordinary
online-signed published Standard LeaseSet2 carrier is wired into
`i2pr-proto` and `i2pr-netdb` and `DatabaseStoreData::LeaseSet2`
replaces the type-3 `Deferred` payload for the ordinary subset.
EncryptedLeaseSet, MetaLeaseSet, blinded, offline-signing, leased, and
PQ-hybrid variants remain future work tracked by the Milestone 6
roadmap. Plan 121 closed as `passed-ecies-destination-session-layer`
per [`plans/121-status.md`](../plans/121-status.md); the
`EciesSessionManager` in `i2pr-client` owns the bounded
ECIES-X25519-AEAD-Ratchet destination session layer with the
trajectory test `plan_121_deterministic_local_trajectory`.
Plan 122 closed as `passed-corrected-local-destination-routing` per
[`plans/122-status.md`](../plans/122-status.md) and
[`plans/124-status.md`](../plans/124-status.md); it composes the
Plan 119 LeaseSet2 lookup surface, the Plan 120 destination runtime,
the Plan 121 ECIES session layer, and the Plan 116 tunnel data plane
into the first complete local destination routing pipeline. The
trajectory test `plan_122_two_destination_local_composition` drives
the full Phase A/B/C/F/H path against real `i2pr-netdb` and
`i2pr-client` surfaces without touching sockets, DNS, or any
external I2P reference.

Plan 124 closed as `passed-plan122-corrective-closure` per
[`plans/124-status.md`](../plans/124-status.md); it corrects the
Plan 122 composition defect where `compose_outbound_delivery`
retained an ECIES Garlic envelope but fed the plaintext inner I2NP
`Data` envelope into the outbound tunnel role. The corrected
composition wraps the encrypted envelope in an `I2npBody::Garlic`
carrier and feeds the standard-encoded I2NP Garlic message bytes
into the outbound tunnel data plane. `OutboundDeliveryPlan` exposes
`garlic_i2np_bytes: Vec<u8>` as the canonical carrier; `inner_envelope_bytes`
is retained for diagnostic comparison only. The eleven deterministic
tests in
[`crates/i2pr-client/tests/plan124_trajectory.rs`](../crates/i2pr-client/tests/plan124_trajectory.rs)
cover Phases A–G, including the canonical
`authenticated-router-link-bypassed-local-seam` boundary, the
byte-identity regression at the OBEP, and the successful A → B → A
New Session trajectory through real destination-owned outbound and
inbound tunnel roles. Plan 124 does not activate NTCP2, SSU2, SAM,
I2CP, Docker, namespaces, Multipass, or any public-network access.

Plan 125's corrective surface (canonical gzip framing plus the real
SYN/SYN-response lifecycle) landed; its final classification is
`superseded-by-final-corrective-closure`
([`plans/125-status.md`](../plans/125-status.md)) after the
post-Plan-125 audit, with Plan 123 restored as
`passed-corrected-streaming-wire-local` by Plans 128/129
([`plans/123-status.md`](../plans/123-status.md)). Plan 125 replaces the
custom Plan 123 `ClientPayload` framing with the canonical RFC 1952
gzip wire format (no SHA-256 prefix, no custom compressed-length
prefix; bounded decompressed-size enforcement; explicit
trailing-byte rejection) and restores the real Plan 125 §6/§7 SYN
/ SYN-response lifecycle (originator SYN uses `sendStreamId = 0`;
recipient SYN response uses `sendStreamId = originator receiveStreamId`
and `receiveStreamId = recipient-selected id`). Stream-id ownership
splits between `local_receive_stream_id` and `peer_receive_stream_id`;
the maximum packet payload is negotiated as `min(local, remote)`;
the broken `SystemClock` is fixed to anchor at an origin `Instant`
and report elapsed time. The wire codec lives in
[`crates/i2pr-proto/src/streaming/`](../crates/i2pr-proto/src/streaming/)
(`StreamingPacket`, `StreamingPacketBuilder`, `StreamingFlags`,
`validate_initial_syn` / `validate_syn_response`,
`encode_syn_replay_binding`, `verify_syn_replay_binding`,
`build_signature_preimage`, flag-ordered raw final signatures with
Ed25519 — Plan 128 corrected the wire format; see below — and the
canonical RFC 1952 gzip protocol-6 `ClientPayload` envelope).
The streaming runtime lives in
[`crates/i2pr-client/src/streaming/`](../crates/i2pr-client/src/streaming/)
(synchronous, Tokio-free, deterministic-clock `StreamingManager`
with per-destination outbound and inbound connection tables, listener
backlogs, send/receive window and congestion policies,
retransmit/timeout policy, typed event surface, real Plan 125 §6/§7
SYN / SYN-response lifecycle, bidirectional `inbound_by_stream` /
`outbound_by_stream` connection lookup). The runtime-neutral
`StreamingDestinationAdapter` lives in
[`crates/i2pr-client/src/streaming_adapter.rs`](../crates/i2pr-client/src/streaming_adapter.rs)
and bridges `TransportSendRequest` into the Plan 122
`compose_outbound_delivery` pipeline. Sixteen deterministic
integration tests in
[`crates/i2pr-client/tests/plan123_trajectory.rs`](../crates/i2pr-client/tests/plan123_trajectory.rs)
remain as fast Streaming-only VirtualWire fault tests; six
additional deterministic tests in
[`crates/i2pr-client/tests/plan125_trajectory.rs`](../crates/i2pr-client/tests/plan125_trajectory.rs)
cover the real handshake lifecycle, the RFC 1952 gzip wire format,
the stream-id ownership contract, the `SystemClock` monotonicity
fix, and the listener outcome surface.
verification, SYN replay binding rejection,
MAX_PACKET_SIZE_INCLUDED policy, corrupt-signature rejection, the
full two-destination SYN → data → CLOSE trajectory, loss recovery
via retransmit, duplicate packet idempotence, RESET termination,
send window backpressure, connection table ceiling, and signed
CLOSE / signed RESET packet shapes. The streaming layer is
runtime-neutral; it composes with Plan 124's `compose_outbound_delivery`
for outbound composition and Plan 124's `DestinationDispatcher` for
inbound routing. The next executable plan is **Plan 125** (Streaming
protocol-6 framing correction + reply round-trip) under
[`plans/118-123-milestone6-router-construction-roadmap.md`](../plans/118-123-milestone6-router-construction-roadmap.md);
see [`plans/117-status.md`](../plans/117-status.md) and
[`plans/118-planning-authority-cleanup-and-plan117-disposition.md`](../plans/118-planning-authority-cleanup-and-plan117-disposition.md).
The historical row text below is retained for auditability.

Plan 128 closed as `passed-streaming-wire-protocol-corrective-closure`
per [`plans/128-status.md`](../plans/128-status.md) and restored Plan
123 as `passed-corrected-streaming-wire-local`. Plan 128 corrects the
`i2pr-proto::streaming` packet codec and the `i2pr-client::streaming`
control packets to match the current I2P Streaming specification while
preserving Plan 125's RFC 1952 gzip client-payload work and real
SYN/SYN-response state progression. Normative provenance lives in
[`specs/references/streaming-packet-wire.md`](../specs/references/streaming-packet-wire.md).
The corrected surface: the normative flag map (`SYNCHRONIZE 0x0001`
through `OFFLINE_SIGNATURE 0x0800`, reserved `0xF000` rejected) with
M6 policy sets `INITIAL_SYN_FLAGS = 0x04A9`, `SYN_RESPONSE_FLAGS =
0x00A9`, `CLOSE_FLAGS = 0x000A`, `RESET_FLAGS = 0x000C`; removal of
every invented option-data TLV (unparsed trailing option bytes are
rejected fail-closed); a flag-driven `StreamingOptions` /
`StreamingOptionDecodeContext` / `SignatureLocation` codec writing
DELAY → FROM → MAX → SIGNATURE in normative order; `MAX_PACKET_SIZE`
as a 2-byte big-endian integer bounding the payload only with
`DEFAULT_ADVERTISED_MAX_PAYLOAD = 1730` and an independently checked
full packet ceiling; variable-length raw final signatures whose length
comes from signing-key context (FROM destination, or the peer signing
key retained on the connection for established-connection CLOSE/RESET
without FROM since 0.9.20); the canonical preimage that signs the
complete packet once over zeroed signature-placeholder bytes;
`peek_streaming_header` two-phase routing decode; typed
`UnsupportedOfflineSignature` / `SignatureContextUnavailable` /
`CloseMissingSignature` / `ResetMissingSignature`. The initial SYN
carries eight Proposal 164 replay NACK words holding the receiver
Destination hash covered by the signature; the SYN response carries
zero NACKs, no NO_ACK, and a valid `ackThrough`; `validate_syn_policy`
is split into `validate_initial_syn` / `validate_syn_response`;
negotiation is `min(local advertised, remote advertised)`. Wire
fixtures: [`crates/i2pr-proto/tests/plan128_wire.rs`](../crates/i2pr-proto/tests/plan128_wire.rs);
manager trajectories:
[`crates/i2pr-client/tests/plan128_trajectory.rs`](../crates/i2pr-client/tests/plan128_trajectory.rs).
No destination-tunnel, SAM, external transport, or live-network work
was introduced; Streaming interoperability is not claimed until
independent-router evidence exists.

Plan 129 is now `superseded-by-plan130-final-gate`; its integrated
topology remains the closure path. Plan 130 closed as
`superseded-by-plan131-final-local-correctness-gate` (historical
evidence retained in `plans/130-status.md`). Plan 131 is the current
Milestone 6 authority (`passed-milestone6-final-local-correctness-closure`,
`plans/131-status.md`) and adds the Plan 131 corrections on top of
the Plan 130 surface: production Elligator2 representatives now
randomize the inverse-map branch as well as the high bits via the
reviewed `elligator2 = 0.1.0` primitive (`curve25519-elligator2
= 0.1.0-alpha.2` retired; independent frozen fixtures in
`specs/references/elligator2-production-representation.md`), ordinary
Streaming application data starts at sequence 1 with seq 0 owned by
SYN/SYN-response/plain-ACK, `ackThrough == 0` is a flag-driven valid
acknowledgement (no "zero means absent" rule), NACK-aware cumulative
ACKs retain explicitly NACKed packets per the Java `ackPackets`
contract, receiver ACK/NACK views follow `MessageInputStream.updateAcks`
with bounded NACK generation, a synchronous `poll_acks(now_ms)` emits
coalesced standalone delayed ACKs on the 750 ms reference default with
piggyback suppression and no ACK-of-ACK loop, the wire destination_port
owns listener dispatch (exact match, wildcard port-0 fallback, typed
`NoMatchingListener`) with enforced established port tuples
(`PortTupleMismatch`), tunnel duplicate windows persist across fixture
deliveries with exact replays failing as typed `DuplicateCell`, and
seven Plan 131 trajectories in `plan131_trajectory.rs` plus eleven
Plan 130 trajectories in `plan130_trajectory.rs` close the gate.
Plan 131 additionally requires connection-owned I2P ports asserted
(not authoritative) on every established `send_data`/`send_close`/
`send_reset` and side-effect-free oversized `send_data` rollback
(no sequence allocation, no window mutation, contiguous sequence
recovery for the next valid packet). Historically:

Plan 129 closed as `passed-milestone6-integrated-local-product-gate`
per [`plans/129-status.md`](../plans/129-status.md) and is the final
local-product gate for Milestone 6. It completes the adapter boundary:
the outbound adapter bounds the gzip-encoded complete Streaming packet
against the client-payload/I2NP limit
(`MAX_STREAMING_ADAPTER_PAYLOAD_BYTES = MAX_CLIENT_PAYLOAD_BYTES`, not
the negotiated payload MTU) and builds no redundant inner I2NP Data
envelope (`OutboundRequest::new` inside the routing composer is the
single canonical Data-envelope owner); the inbound adapter decodes
standard I2NP -> requires `I2npBody::Data` -> decodes the canonical
protocol-6 gzip client payload -> requires protocol 6 (typed
`UnsupportedProtocol` outcome for future datagram/I2CP layers) ->
reads I2P source/destination ports -> passes only decoded Streaming
packet bytes to the owning local destination's `StreamingManager`.
The integrated core gaps fixed in place: real retransmission over the
destination path (`StreamingManager::poll_retransmits`, attempt
capped), cumulative end-to-end ACKs applied on receipt and clearing
tracked packets, ordered delivered-byte surfacing after reorder
(`RecvWindowDecision::Delivered` carries drained entries;
`drain_delivered()`), and CLOSE/RESET completion policy where a side
never marks itself Closed merely because it queued a CLOSE and nothing
delivers after RESET. The authoritative evidence is
[`crates/i2pr-client/tests/plan129_trajectory.rs`](../crates/i2pr-client/tests/plan129_trajectory.rs)
(twelve deterministic tests over the full stack: master SYN /
SYN-response both directions, steady-state Existing Session data with
exact ordered bytes, post-OBEP drop/duplicate/reorder faults,
signature/gzip-CRC/ECIES-tamper corruption at protocol-appropriate
layers, graceful CLOSE through peer response, RESET with survivor
streams, non-protocol-6 dispatch, ceiling bounds, 0-RTT scope). The
§13 SAM-readiness review answer is **yes**.
`milestone6_local_product = passed`;
`milestone6_interoperable = not-yet-claimed`; Streaming stays
experimental and non-advertised until independent-router evidence
exists. The next product layer is SAM baseline planning (Milestone 7).

Plan 130 (`superseded-by-plan131-final-local-correctness-gate`,
historical evidence in [`plans/130-status.md`](../plans/130-status.md),
[`crates/i2pr-client/tests/plan130_trajectory.rs`](../crates/i2pr-client/tests/plan130_trajectory.rs),
and the reference addenda in
[`specs/references/streaming-packet-wire.md`](../specs/references/streaming-packet-wire.md)
plus
[`specs/references/elligator2-production-representation.md`](../specs/references/elligator2-production-representation.md))
landed the wire/runtime corrective closure but left four Plan 131
acceptance items open.

Plan 131 (`passed-milestone6-final-local-correctness-closure`,
[`plans/131-status.md`](../plans/131-status.md),
[`crates/i2pr-client/tests/plan131_trajectory.rs`](../crates/i2pr-client/tests/plan131_trajectory.rs))
is the current Milestone 6 closure authority. On top of the Plan 130
surface it adds: production Elligator2 branch randomization via the
reviewed `elligator2 = 0.1.0` primitive (`curve25519-elligator2
= 0.1.0-alpha.2` retired; provenance in
[`specs/references/elligator2-production-representation.md`](../specs/references/elligator2-production-representation.md));
independent three-layer replay separation (tunnel duplicate window,
consumed ECIES session tag, fresh-ECIES-reseal Streaming sequence);
connection-owned I2P port tuple asserted (not authoritative) on
every established `send_data` / `send_close` / `send_reset`; and
side-effect-free oversized `send_data` rollback (no sequence
allocation, no window mutation, contiguous sequence recovery for
the next valid packet). Plan 130 sequence / ACK / NACK / port / ECIES
surface (sequence space SYN=0/response=0/first-app=1; semantic ACK
presence where `ackThrough == 0` acknowledges the handshake slot;
NACK-aware cumulative ACKs retaining explicitly NACKed packets;
bounded receiver ACK/NACK views per `MessageInputStream.updateAcks`;
synchronous coalescing
delayed standalone ACKs (750 ms default) via `poll_acks(now_ms)` with
piggyback suppression and no ACK-of-ACK loop; wire destination-port
listener authority (exact match → wildcard port-0 → typed
`NoMatchingListener`) with enforced established tuples; persistent
tunnel duplicate windows with typed `DuplicateCell` replay rejection)
remains intact and is the retained Plan 130 evidence. Plan 129's
historical gate remains the topology of record. Plan 132
(`passed-milestone6-final-evidence-and-transactional-closure`,
[`plans/132-status.md`](../plans/132-status.md)) is the current
Milestone 6 closure authority. On top of the Plan 131 surface
it adds: strict Elligator2 receive-domain validation masking the
two free high bits and rejecting `r >= 2^254 - 10` via the
`is_canonical_elligator_representative` helper before delegating
to `elligator2::from_representative` in
[`crates/i2pr-crypto/src/ecies.rs`](../crates/i2pr-crypto/src/ecies.rs);
three independent layer-isolated replay trajectories
(`DuplicateToken`, `UnknownSessionTag`, sequence-dedup) with an
artifact-preserving test seam in
[`crates/i2pr-client/tests/plan132_trajectory.rs`](../crates/i2pr-client/tests/plan132_trajectory.rs);
and `&mut self` transactional send ordering on
`send_data`/`send_close`/`send_reset` (Phase 1 immutable
validation → Phase 2 fallible wire construction → Phase 3
single commit point with `assert_eq!(sequence, planned_sequence)`
guarantee).

This matrix is intentionally explicit: every row describes the exact evidence
available, not just code presence. “Experimental structural subset” means
bounded codecs exist and are tested locally, but no mixed-router interoperability
or capability claim exists.

The fine-grained, machine-readable inventory through the current Milestone 3
corrective integration is
[`specs/support.toml`](../specs/support.toml). Structural entries may be marked
`experimental` with repository evidence, but remain `advertised = false`; the
ledger does not itself publish protocol capabilities.

Plan 031 adds transport-neutral link, delivery, lifecycle, and resource
contracts. Plan 032 adds a Tokio-free NTCP2 cryptographic/transcript foundation
plus static-key persistence, and Plan 033 adds bounded handshake codecs and
consuming action-driven state machines. These are experimental local evidence,
not complete NTCP2 protocol support; no transport capability is advertised or
published in RouterInfo.

Plan 037 records local corrections to admission ownership, deadline-enforced
link I/O, queue RAII, and general data-phase block ordering. It does not add a
complete socket-to-state-machine adapter or mixed-router evidence; NTCP2 rows
therefore remain experimental and non-advertised.
Plan 034 adds runtime-neutral authenticated data frames, strict payload
blocks, and deterministic partial-I/O evidence. The current specification has
no in-session rekey threshold; counter exhaustion remains terminal and requires
a fresh handshake. This is still local evidence only; no sockets, NetDB
mutation, mixed-router interoperability, or transport capability is claimed.
Plan 035 adds controlled runtime-owned TCP lifecycle, strict NTCP2 address
interpretation, admission, replay/backoff, and joined link-child ownership.
Loopback/private socket tests are local lifecycle evidence only; public
listeners, automatic address publication, NetDB mutation, mixed-router
interoperability, and capability advertisement remain excluded.
Plan 036 adds the pinned, manual interoperability manifest, sanitized-evidence
format, preflight check, and fixed-seed 0..255 local validation campaign. The
runtime-owned NTCP2 wire adapter is implemented and locally validated; mixed-
router harness composition and authorized evidence are pending; NTCP2 remains
experimental and non-advertised.

Plans 038/040/041 document the Ubuntu-only, amd64-only harness for resolving
that blocker; Plan 041 adds a reference-only Java I2P/i2pd control crosscheck
but does not change any row in this matrix. Preparation may use
declared package/source network access to build and hash pinned references.
Execution is a separate fail-closed phase using disposable namespaces joined
only by a veth pair, with no default route, DNS, or public egress. Environment
smoke and Java I2P/i2pd reference crosscheck are harness validation only. An
i2pr mixed-router claim still requires sanitized bounded authenticated runs
against each reference in both directions, plus the evidence and
advertisement requirements in `specs/CONFORMANCE.md`.

| Protocol area | Status | Planned milestone | Specification/source starting point | Test-vector status | Interoperability status |
| --- | --- | --- | --- | --- | --- |
| Common identity, keys, and certificates | Experimental structural subset plus local type-4/type-7 execution | 1 | `specs/protocols/01-common-identity-crypto.md`, pinned source in `specs/SOURCES.md` | Locally authored structural bytes, Ed25519 mutation tests, and X25519 derivation tests; no independent router vectors | None |
| Router identity generation and local RouterInfo signing | Experimental local lifecycle | 1 | `plans/013-m1-identity-crypto-storage.md`, ADRs 0004 and 0007 | Deterministic injected-RNG generation, exact signed-region verification, save/reload and mutation tests | None |
| Private router identity storage | Experimental local persistence | 1 | `plans/013-m1-identity-crypto-storage.md`, ADR 0006 | Version/length/truncation/integrity/permission/concurrency tests; no external storage interoperability claim | None |
| I2NP envelope and header variants | Experimental structural subset; not advertised | 1, 3–6 | `specs/protocols/02-i2np.md`, pinned 0.9.69 source in `specs/SOURCES.md` | Locally authored standard/short vectors, truncation, size, checksum, and trailing-byte tests; hashed fixture manifest | None |
| I2NP type registry and selected body codecs | Experimental structural subset; NetDB body semantics deferred | 1, 4 | `specs/protocols/02-i2np.md`, `crates/i2pr-proto/src/i2np/mod.rs` | Fixed and malformed local vectors for DatabaseLookup, DatabaseSearchReply, DeliveryStatus, DatabaseStore framing, and fixed tunnel framing | None |
| I2NP tunnel, garlic, data, and later record semantics | Deferred or framing-only | 1, 5–6 | `specs/protocols/02-i2np.md`, `specs/protocols/05-tunnels.md`, `specs/protocols/06-garlic-ecies-leasesets.md` | Bounded `Deferred`/`Opaque` retention and shape checks only; no crypto or state-machine vectors | None |
| NTCP2 crypto/transcript foundation | Experimental local subset; not advertised | 3 | `specs/protocols/03-ntcp2.md`, ADR 0011, `plans/036-closure.md`, `plans/037-closure.md` | Independent deterministic primitive/transcript vectors and corrective review; no router interoperability run | `tests/integration/ntcp2/manifest.toml` pinned but execution blocked; the Plan 046 rootless variant reports `blocked_unprivileged_user_namespace` on the host recorded in `plans/046-closure.md`; the Plan 066 fresh-candidate pass is `declared-not-executable` on this host under the historical Plan 058/060 two-lane contract, see `plans/066-closure.md`; the Plan 067 active roadmap records Level 1 smoke and Level 2 development validation lanes for the host loopback |
| NTCP2 handshake codecs and state machines | Experimental local subset; not advertised | 3 | `specs/protocols/03-ntcp2.md`, ADR 0012, `plans/036-closure.md`, `plans/037-closure.md` | Fixed/malformed/bounded state and policy tests plus local corrective campaign; no mixed-router interoperability | Required Java I2P/i2pd lanes blocked; Plan 046 rootless lane is closed with `blocked_unprivileged_user_namespace`; Plan 066 fresh-candidate pass is `declared-not-executable` on this host with the typed blocker `blocked_execution_lane_unavailable`; see `plans/066-closure.md` and `specs/CONFORMANCE.md`; the Plan 067/068 active roadmap separates evidence into local-conformance, external-loopback-smoke, repeated-development-interop, conditional-differential, and release-qualification tiers (ADR 0023) |
| NTCP2 authenticated data frames and payload blocks | Experimental local subset; not advertised | 3 | `specs/protocols/03-ntcp2.md`, ADR 0013, `plans/036-closure.md`, `plans/037-closure.md` | Deterministic frame/block vectors, corrected repeated-block/termination ordering tests, partial-I/O cleanup, and local campaign; no mixed-router interoperability | Required Java I2P/i2pd lanes blocked; Plan 046 rootless lane is closed with a typed blocker (`plans/046-closure.md`); the Plan 066 fresh-candidate pass is `declared-not-executable` on this host, see `plans/066-closure.md`; the Plan 067/068/074 active roadmap defines Plan 069 (host-loopback smoke), Plan 075 (runner integrity), Plan 076 (real i2pd driver), and Plan 079 (repeated i2pd validation) for development interoperability; no real mixed-router attempt has yet occurred |
| NTCP2 runtime link manager, addresses, and controlled TCP lifecycle | Experimental local subset; not advertised | 3 | `specs/protocols/03-ntcp2.md`, ADR 0014, `plans/036-closure.md`, `plans/037-closure.md` | Bounded address/admission/replay/backoff/duplicate/RAII cleanup tests plus loopback lifecycle and preflight; runtime-owned wire adapter implemented and locally validated, mixed-router evidence pending | Required Java I2P/i2pd lanes blocked; Plan 046 rootless lane is closed with a typed blocker (`plans/046-closure.md`); the Plan 066 fresh-candidate pass is `declared-not-executable` on this host, see `plans/066-closure.md`; the Plan 067/068 active roadmap keeps NTCP2 experimental and non-advertised |
| Reseed and RouterInfo publication | Plans 103–107 implemented locally on this host: RouterInfo validation, bounded local NetDB, persistent cache, SU3 reseed, transport-neutral query state machines, daemon bootstrap integration, and the Milestone 5 exploratory tunnel substrate. Plans 108–112 corrected and locally validated the ECIES-X25519 short-build wire, cryptography, records, slot/fake-record handling, and outbound construction. Plan 113 reconciles inbound construction against pinned Java I2P and i2pd source: the fixed request record remains the canonical 154-byte layout, and the creator-side inbound originator fake is exactly `hash16 || fresh X25519 public key || random remainder`; the final specification text discrepancy is recorded under the explicit `reference-compatible-spec-text-discrepancy` policy. Plan 114 closes four post-Plan-113 routing/composition defects: explicit outbound `outbound_reply_router` and inbound `originator_hash` terminal-routing fields, intermediate `hops[i].next_tunnel == hops[i+1].receive_tunnel` chain continuity enforced at both the high-level `ShortBuildPath::validate()` boundary and the public lower-level `prepare_short_build_message()` entry point, and strict outbound/inbound E2E trajectories that deterministically reach `Established`. Plan 115 closes the canonical production I2NP bridge (`ShortBuildI2npBridge`) so the already count-prefixed `ShortBuildAction::Deliver.message` is wrapped in a single complete I2NP type-25 message without double-prefixing the STBM record count byte and with a round-trip body equality assertion. Plan 115 Q0 construction + native OBEP reply has passed locally against pinned Emissary at 9b43484a21d5a1291c4881cdae62a36c527f8c0f: the i2pr-produced STBM is consumed by Emissary's native short-build handler, Emissary replies with `TunnelGateway` + Garlic inner message + feedback channel. Q1 (authenticated transport delivery) and Q2 (reply round-trip to `Established`) remain pending on a qualified external delivery lane; no live mixed-router tunnel execution or transit participation has been attained. | 4/5 | `specs/protocols/04-reseed-netdb.md`, `specs/protocols/05-tunnels.md`, `plans/103-status.md`, `plans/113-status.md`, `plans/114-status.md`, `plans/115-status.md`, `specs/references/short-build-inbound-creator-key.md` | Local signed-region verification, bounded NetDB/reseed/bootstrap checks, short-build request/reply vectors, role/topology validation, randomized slot and fake-record construction, originator-fake integrity checks, deterministic multi-hop inbound trajectory, terminal routing field validation, intermediate tunnel-id chain continuity validation, strict trajectory E2E tests, the canonical production I2NP bridge no-double-prefix invariant, and the Plan 115 Q0 Emissary-native OBEP consumption test; no live mixed-router tunnel execution or transit participation | None |
| Network tunnels and transit participation | Experimental local subset; outbound short tunnel-build locally conformant against fixed vectors; inbound construction locally reference-compatible under Plan 113's `reference-compatible-spec-text-discrepancy` policy; exploratory substrate implemented; Plan 114 closed terminal routing and tunnel-chain corrections; Plan 115 added the canonical production I2NP bridge and Q0 construction + native OBEP reply (passed locally against pinned Emissary); live mixed-router build pending | 5 | `specs/protocols/05-tunnels.md`, `plans/107-milestone-5-exploratory-tunnel-substrate.md`, `plans/111-short-build-final-local-conformance-correction.md`, `plans/112-status.md`, `plans/113-status.md`, `plans/114-status.md`, `plans/115-status.md`, `plans/115-handoff.md`, `specs/references/short-build-inbound-creator-key.md` | Bounded tunnel identity/pool/build crypto types, role-validated short-build state machine, randomized multi-record construction, exactly one inbound originator fake with creator-side integrity verification, explicit terminal routing fields, intermediate tunnel-id chain continuity enforcement, strict trajectory E2E tests, deterministic local inbound trajectory, the canonical production I2NP bridge no-double-prefix invariant, and the Plan 115 Q0 Emissary-native OBEP consumption test; no live mixed-router tunnel execution or transit participation | None |
| Classic LeaseSet structural codec | Experimental structural subset; LeaseSet2-family deferred | 6 | `specs/protocols/06-garlic-ecies-leasesets.md` | Local Lease/LeaseSet vectors and negative tests; no independent router vectors | None |
| Standard LeaseSet2 structural codec (ordinary online-signed published subset) | Experimental | 6 | `specs/protocols/06-garlic-ecies-leasesets.md`, [`plans/119-status.md`](../plans/119-status.md), [`plans/121-status.md`](../plans/121-status.md), [`plans/122-status.md`](../plans/122-status.md) | 40-byte Lease2, LeaseSet2Header, LeaseSet2EncryptionKey, canonical Mapping options, signature domain `0x03 || signed_bytes`, strict fresh/Lease accounting, frozen wire-format fixture, i2np-level DatabaseStore round-trip, Plan 122 lookup-engine `handle_database_store_lease_set2` ingestion path with `LookupResult::LeaseSet2Success`, Plan 122 `router_hash_from_destination` helper, and Plan 122 daemon NetDbSeam `begin_lease_set2_lookup`/`ingest_lease_set2_response`/`cancel_lease_set2_lookup`; EncryptedLeaseSet / MetaLeaseSet / blinded / offline-signing / leased / PQ-hybrid variants remain deferred | None |
| ECIES-X25519-AEAD-Ratchet destination session layer | Experimental structural subset | 6 | `specs/protocols/06-garlic-ecies-leasesets.md`, [`plans/121-status.md`](../plans/121-status.md) | Bounded `EciesSessionManager` with `MAX_OUTBOUND_SESSIONS_PER_REMOTE = 16`, `MAX_INBOUND_SESSIONS_PER_REMOTE = 16`, `MAX_PENDING_NEW_SESSIONS = 64`, `MAX_TAG_LOOK_AHEAD = 32`, `MAX_REPLAY_CACHE_ENTRIES = 64`, `DEFAULT_SESSION_IDLE_SECONDS = 600`, `MAX_SESSION_IDLE_SECONDS = 1800`; structural Garlic payload block codec (`GarlicClove`, DateTime + Garlic encryption envelope) in `i2pr-proto`; deterministic two-destination NS → NSR → Existing Session trajectory with exact-once payload delivery, tag ratchet advancement, and replay rejection (`plan_121_deterministic_local_trajectory`); no remote router exercise | None |
| Destination routing and Garlic composition | Experimental structural subset | 6 | [`plans/122-status.md`](../plans/122-status.md) | Bounded `LeaseSelector` / `LeaseSelectionPolicy` with expiry / safety-margin / zero-tunnel-id / destination-mismatch / uniform-distribution rejection; typed `OutboundRequest` builder; `compose_outbound_delivery` planner that drives ECIES encryption then `OutboundGatewayRole::forward_cells` with `DeliveryInstruction::Tunnel` targeting the selected lease; `DestinationRouting` cache with bounded `MAX_CONCURRENT_REMOTE_LOOKUPS = 256` and `MAX_PENDING_OUTBOUND_PER_REMOTE = 64`; `DestinationDispatcher` inbound surface with bounded `MAX_INBOUND_DESTINATIONS = 256`, `MAX_INBOUND_PENDING_MESSAGES = 256`, `MAX_INBOUND_PAYLOAD_BYTES_PER_DESTINATION = 512 * 1024`; deterministic two-destination Phase A/B/C/F/H local composition (`plan_122_two_destination_local_composition`); router-delivery seam emits `OBGWRouterDelivery` cells addressed to the local outbound creator's first hop; authenticated-router link between outbound endpoint and remote inbound gateway remains a transport-level omission | None |
| EncryptedLeaseSet and MetaLeaseSet | Deferred | 6 | `specs/protocols/06-garlic-ecies-leasesets.md` | Explicit `DatabaseStoreData::Deferred` framing only | None |
| I2P streaming | Experimental minimal core | 6 | `specs/protocols/07-streaming.md`, [`plans/128-status.md`](../plans/128-status.md), [`plans/130-status.md`](../plans/130-status.md), [`specs/references/streaming-packet-wire.md`](../specs/references/streaming-packet-wire.md) | Synchronous Tokio-free deterministic `StreamingManager` in `i2pr-client::streaming` with per-destination outbound and inbound connection tables, listener backlogs, send/receive window and congestion policies, retransmit/timeout policy, and a typed event surface. Wire codec in `i2pr-proto::streaming` (Plan 128 normative form): normative flag map with policy sets `0x04A9`/`0x00A9`/`0x000A`/`0x000C`, no option-data TLVs, flag-driven option order, 2-byte big-endian payload-only MAX_PACKET_SIZE defaulting to 1730, raw final signatures from signing-key context, canonical zeroed-placeholder preimage signed once, eight Proposal 164 replay NACK words on the initial SYN only, retained peer signing key for CLOSE/RESET verification without FROM, and the protocol-6 RFC 1952 gzip `ClientPayload` envelope (Plan 125). Sixteen deterministic integration tests in `plan123_trajectory` cover the signed-SYN payload round trip, canonical preimage signature verification, SYN replay binding rejection, MAX_PACKET_SIZE_INCLUDED policy, corrupt-signature rejection, the full two-destination SYN → data → CLOSE trajectory, loss recovery via retransmit, duplicate packet idempotence, RESET termination, send window backpressure, connection table ceiling, and signed CLOSE / signed RESET packet shapes. The streaming layer is runtime-neutral; the combined outbound/inbound `StreamingDestinationAdapter` (Plan 129) bridges it into Plan 122's `compose_outbound_delivery` and `DestinationDispatcher`, and the Plan 129 integrated gate (`plan129_trajectory`, twelve deterministic tests) drives the complete destination stack in both directions. Interoperability against an independent router is not claimed. | None |
| SAM | Not implemented | 7 | `specs/protocols/08-sam.md` | None imported | None |
| SSU2 | Not implemented | 8 | `specs/protocols/09-ssu2.md` | None imported | None |
| I2CP | Not implemented | 9 | `specs/protocols/10-i2cp-service-tunnels.md` | None imported | None |
| Service tunnels | Not implemented | 10 | `specs/protocols/10-i2cp-service-tunnels.md` | None imported | None |

The workspace may name the `common` and `i2np` namespaces and now includes the
non-networked `i2pr-runtime` supervision crate, but runtime infrastructure is
not protocol support evidence. Plan
013 adds local type-4/type-7 execution plus a private identity file. These
local operations do not establish mixed-router protocol support, complete
signature/encryption coverage, transport support, network compatibility, or
capability advertisement. Legacy NTCP and SSU1 are outside the MVP target
unless a later plan explicitly changes scope.

The I2NP implementation recognizes the pinned message identifiers and strictly
decodes standard, obsolete-SSU, and NTCP2/SSU2 short headers. It fully models
the structural fields of DatabaseLookup, DatabaseSearchReply, DeliveryStatus,
and DatabaseStore; only classic LeaseSet payloads reuse an existing structural
codec. Compressed RouterInfo, LeaseSet2-family records, tunnel-build record
cryptography, garlic/data semantics, duplicate/expiry policy, routing,
transport authentication, and capability advertisement remain deferred. No
I2NP row is `advertised = true`, and no row claims mixed-router support.

DatabaseLookup legacy and ECIES reply-key/tag wrappers are non-cloneable and
zeroizing structural containers. They provide memory hygiene only; they do not
implement encrypted reply semantics, key derivation, decryption, or NetDB
behavior.

Each future protocol row must be updated with exact targeted proposal/spec
revisions, limits, malformed-input behavior, vectors, and mixed-router evidence
before its status changes.

### Plan 048 evidence-environment notice

Plan 048 adds only a disposable Multipass recovery environment for the Plan
046 rootless lane. The current host remains the AppArmor-restricted negative
baseline, the guest applies permissive policy only inside its VM, and the
canonical cache is `target/interop/cache`. A guest probe, matrix, or exported
reference control result does not advance any support row. NTCP2 remains
experimental and non-advertised until sanitized mixed-router conformance
evidence satisfies `specs/CONFORMANCE.md`.

### Plan 060 fresh-candidate and two-run Milestone 3 certificate closure pass

Plan 060 was the execution-only fresh-candidate and two-run
Milestone 3 certificate closure pass. Plan 060 is now **retired
by Plan 062** (Plan 062 evidence-contract and architecture
correction pass). The Plan 060 candidate record
(`plans/060-candidate.md`) is preserved verbatim for audit; the
Plan 060 closure record (`plans/060-closure.md`) carries the
explicit "Superseded by Plan 062" marker. Future candidates must
descend from the Plan 065 implementation floor or later and must
use the Plan 062 v4 trigger schema, the Plan 062 reference-event
v1 schema, the Plan 062 v3 observation schema, and the 64-hex
SHA-256 Router Hash contract.

Plan 060 inherited the rejected Java-support-topology premise
(ADR 0021 Rejected by Plan 058). Plan 062 ADR 0022 (Accepted)
replaces that premise with two-process direct transport drivers.
The historical Plan 060 typed blocker on this host is
`blocked_execution_lane_unavailable` and the historical candidate
status is `declared-not-executable`. The Plan 046 rootless
sealed-namespace probe reports
`blocked_unprivileged_user_namespace` on this host; the Plan
048/049 Multipass recovery lane is the canonical external path
but cannot complete on this constrained host (per Plan 051).

The Plan 060 implementation surface
(`tests/integration/ntcp2/harness/plan060.py`, the Plan 060 test
matrix, the static boundary checker extension, the candidate
record `plans/060-candidate.md`, and the closure record
`plans/060-closure.md`) is preserved as an audit record. NTCP2
remains experimental and non-advertised until a future pinned
Java revision exposes a transport-only direct seam (or ADR 0021
is re-issued) and either the Plan 046 rootless sealed-namespace
lane or the Plan 048/049 Multipass guest lane becomes runnable
on a host with the resources Plan 051 required.

### Plan 062 NTCP2 evidence-contract and architecture correction

Plan 062 is the evidence-contract and architecture correction
pass. Plan 062 does not implement the Java or i2pd drivers and
does not perform an authoritative external interoperability run;
those belong to Plans 063 and 064.

Plan 062 lands:

- `docs/adr/0022-direct-reference-router-ntcp2-interop-drivers.md`
  (Accepted) replacing the rejected Java-support-topology
  premise with two-process direct transport drivers for Java I2P
  and i2pd. ADR 0022 explicitly supersedes the conclusion of
  ADR 0021 without rewriting ADR 0021.
- `tests/integration/ntcp2/reference-drivers/source-verification.md`
  — the source-locked API inspection record for the pinned Java
  I2P 2.12.0 and i2pd 2.60.0 revisions.
- `tests/integration/ntcp2/harness/reference_trigger_v4.py` — the
  Plan 062 v4 trigger schema (`i2pr-reference-trigger-v4`) with
  64-lowercase-hex Router Hash, per-run DeliveryStatus
  `message_id` (`1..=0xffffffff`), and full provenance digests.
- `tests/integration/ntcp2/harness/reference_event.py` — the
  Plan 062 reference-event v1 schema
  (`i2pr-reference-event-v1`) recording per-driver structured
  events with exact DeliveryStatus message ID correlation for
  data-phase events.
- `tests/integration/ntcp2/harness/observation_v3.py` — the Plan
  062 v3 observation schema
  (`i2pr-ntcp2-direction-observation-v3`) with the mandatory
  correlation fields `delivery_status_message_id`,
  `peer_router_hash_sha256`, `local_router_hash_sha256`, and
  `source_event_sha256`. The v3 receiver pass predicate requires
  nonzero decrypt and decode counts and rejects
  generic-phrase-only sources.
- The historical `trigger_record.py` (v3) and `observation.py`
  (v2) modules remain readable for historical inspection but
  cannot contribute to a new passing bundle.

Plan 062 does not close any interoperability claim. NTCP2 stays
experimental and non-advertised; Milestone 3 stays open until a
verified Milestone 3 certificate is produced under ADR 0023 Level 3
release qualification.

### Plan 065 NTCP2 canonical integration and live qualification

Plan 065 establishes the implementation floor from which Plan 066
may cut a candidate. The plan does not perform an authoritative
external live qualification run; the four primary IPv4 directions
remain typed blockers until the Plan 046 rootless sealed-namespace
lane or the Plan 048/049 Multipass recovery lane can produce a
fresh 10/10 qualification on the pinned Java 2.12.0 and i2pd
2.60.0 references.

Plan 065 lands:

- The strict launcher scenario schema is bumped to
  `i2pr-launcher-scenario-v2` (`schema_version` 2). The strict
  parser requires the per-run DeliveryStatus `message_id` in
  `1..=0xffffffff`, the 64-lowercase-hex expected sender and
  receiver Router Hashes, the `reference_driver_mode` field
  allowlisted to `java-direct-driver` or `i2pd-direct-driver`, and
  the `run_identity_sha256` 64-lowercase-hex digest. The historical
  schema 1 path is rejected.
- The i2pr sender and receiver bind the typed counters
  `delivery_status_message_id` and `expected_peer_router_hash_sha256`
  to the status record. The hard-coded `0x0420_0001` DeliveryStatus
  authority is removed.
- The i2pr sender and receiver emit the bounded Plan 065 typed
  failure categories (`SenderDeliveryStatusMessageIdZero`,
  `SenderRouterIdentityMismatch`,
  `SenderDeliveryStatusConstructionFailed`,
  `SenderFrameQueueAmbiguous`, `SenderFrameWriteFailed`,
  `SenderMultiplePrimaryDeliveryStatusEmitted`,
  `SenderCancellationObserved`, `ReceiverFrameReadFailed`,
  `ReceiverFrameAuthenticationFailed`, `ReceiverI2npDecodeFailed`,
  `ReceiverDeliveryStatusMissing`,
  `ReceiverDeliveryStatusIdMismatch`,
  `ReceiverDeliveryStatusDuplicate`,
  `ReceiverPeerIdentityMismatch`,
  `ReceiverDeliveryStatusTimestampInvalid`). The broad
  `DataPhaseFailed` reason is no longer emitted on the receiver
  side.
- The canonical mixed-runner wires the new scenario primary fields
  through `render_and_validate` for both the i2pr initiator and
  responder paths. The `_plan065_primary_fields` helper derives
  the DeliveryStatus `message_id` from the run identity and the
  correlation nonce; the `_reference_driver_mode_for` helper
  returns the source-locked driver mode for a reference kind. The
  runner rejects SAM, HTTP, I2PControl, support-topology, and
  synthetic-fallback helpers for any primary direction.
- The Plan 065 test matrix (`test_plan065.py`) covers scenario
  v2 acceptance and rejection, DeliveryStatus message ID derivation
  uniqueness, status counter contract, reference trigger v4
  correlation, observation v3 correlation, pass predicate exact
  message ID and Router Hash correlation, support-router rejection,
  Plan 060 candidate retirement, and the Plan 066 implementation
  floor marker.
- The static boundary checker
  (`scripts/check-ntcp2-interoperability.sh`) enforces the Plan
  065 schema marker, the required primary fields, the bounded
  typed failure categories, the absence of the hard-coded
  `0x0420_0001` DeliveryStatus authority, and the Plan 065 test
  matrix existence.

Plan 065 does not close any interoperability claim. NTCP2 stays
experimental and non-advertised; Milestone 3 stays open until a
verified Milestone 3 certificate is produced under ADR 0023 Level 3
release qualification.

### Plan 067 staged interoperability corrective roadmap

Plan 067 is the **active** Milestone 3 corrective roadmap. Plan 067
supersedes Plan 066 as the active execution authority. Plan 066
remains an immutable historical record of the unavailable
release-qualification lane on the constrained host.

Plan 067 separates NTCP2 interoperability evidence into four bounded
tiers:

- **Level 0 — local conformance.** Deterministic local protocol and
  runtime ownership.
- **Level 1 — external loopback smoke.** Two real processes on the
  host loopback. i2pd is the primary initial validator. Emissary is
  conditional. No rootless namespace, no Multipass, no candidate
  freeze, no two-bundle certificate, no reviewer record.
- **Level 2 — repeated development interoperability.** Both
  directions against the primary independent validator (pinned i2pd
  2.60.0), three fresh-state repetitions per direction, exact
  message and identity correlation, bounded negative controls.
- **Level 3 — release qualification.** Java I2P 2.12.0 and i2pd
  2.60.0, isolated no-public-egress lane, reproducible
  source/reference provenance, exact authenticated data-phase
  message correlation, independent fresh state, sanitized durable
  evidence. The Plan 066 certificate verifier may be reused at Level
  3.

Java and i2pd remain required for release qualification. NTCP2 stays
experimental and non-advertised; Milestone 3 stays open until a Level
3 run produces a verified certificate.

### Plan 068 staged evidence and authority correction

Plan 068 implements the staged-evidence and authority correction
that Plan 067 proposes. Plan 068 lands:

- `docs/adr/0023-staged-ntcp2-interoperability-evidence.md`
  (Accepted). ADR 0023 separates evidence into four bounded tiers
  and forbids lower-tier promotion into release bundles. ADR 0023
  does not supersede ADR 0022's direct-driver decision.
- `tests/integration/ntcp2/harness/evidence_tier.py` — the
  evidence-tier constants (`local-conformance`,
  `external-loopback-smoke`, `repeated-development-interop`,
  `conditional-differential`, `release-qualification`) and
  tier-separation rules. The release bundle validators refuse every
  record whose tier is missing or lower than `release-qualification`.
- `tests/integration/ntcp2/harness/loopback_smoke_record.py` — the
  Level 1 smoke record schema
  (`i2pr-ntcp2-loopback-smoke-v1`). A passed record requires every
  positive boolean, `cleanup_clean = true`, and `network_audit` not
  equal to `not-run`. Raw payload, private key, Noise state, and
  full RouterInfo bytes are forbidden.
- `tests/integration/ntcp2/harness/development_validation.py` —
  the Level 2 development-validation summary schema
  (`i2pr-ntcp2-development-validation-v1`). A passed summary
  requires three fresh-state passes per direction, four named
  negative controls reporting `rejected`, `cleanup_passed = true`,
  and an explicit network audit per direction.
- The Plan 068 test matrices (`test_evidence_tier.py`,
  `test_loopback_smoke_record.py`,
  `test_development_validation.py`).
- `scripts/check-ntcp2-interoperability.sh` extended to enforce
  the new schema modules, the new test matrices, the ADR 0023
  acceptance marker, and the release-bundle smoke/development
  rejection. The historical plan surfaces (Plan 055/056/058/059/060
  /062/063/064/065/066 freeze-readiness invariants) remain intact.

Plan 068 also removes the stale `blocked_java_support_topology_rejected`
interpretation from the active Java path: ADR 0021 remains Rejected
and the Java support topology remains forbidden, but the ADR 0022
direct Java driver is the active Java architecture. Java may still be
unavailable because of host/runtime/build defects, but not because
ADR 0021 forbids the already accepted replacement architecture.

The focused closure baseline for Plans 069-073 is the touched-code
test suite plus `cargo fmt --all --check`, `cargo check --workspace
--all-targets`, `cargo test --workspace`,
`scripts/check-dependency-direction.sh`, and
`scripts/check-runtime-boundaries.sh`. Full historical harness
matrices, rootless checks, and Multipass checks remain available for
explicit integration checkpoints but are not required for Level 1
or Level 2 closures.

NTCP2 stays experimental and non-advertised. No external pass has
yet occurred.

### Plan 074 real-driver and constrained-host corrective roadmap

Plan 074 is a historical corrective roadmap for Milestone 3 NTCP2
interoperability. Plan 074 superseded Plan 070 and reclassified the
implemented Plan 069 lane as orchestration scaffolding and fake-process
test coverage only; the corrected lane became Plan 075's runner
integrity pass. Plan 074 is no longer active execution authority; the
Plan 081 amendment is the active corrective roadmap, with Plan 082
implemented, Plan 083 next, and Plan 084 reverse.

The corrected repository state is:

```text
plan_068_staged_evidence = implemented
plan_069_runner_scaffolding = implemented_but_not_valid_mixed_router_lane
real_i2pd_driver = not_implemented
real_i2pd_library_linkage = absent
real_reference_process_in_plan069_runner = absent
real_mixed_router_attempts = 0
current_rootless_namespace_lane = unavailable
multipass_lane = unreliable_or_unavailable
support = experimental
advertised = false
normal_daemon_activation = disabled
```

The constrained-host lane decision is ordered: existing accessible
rootful Docker daemon (`--network none`), QEMU TCG guest (`-nic
none`), inherited connected TCP descriptors plus
`no_new_privs`/seccomp for reduced-scope protocol diagnostics,
manually triggered dedicated remote Linux runner, and a typed
no-full-runtime-lane blocker. Rootless namespaces, bubblewrap,
rootless Podman/Docker, user-level systemd `PrivateNetwork`, and
repeated Multipass recovery are not active work items on the known
host.

### Plan 075 Plan 069 runner integrity and evidence correction

Plan 075 corrects the Plan 069 runner so it is structurally
incapable of producing a mixed-router pass unless it launches one
real i2pr process and one configured real reference process and
consumes authentic structured events from both. The corrected
runner must launch the reference role through the configured
reference driver via
`tests/integration/ntcp2/reference-drivers/i2pd/run-driver.sh`,
bind every accepted event to a measured reference process binary
digest, implementation name, run ID, direction, Router Hash pair,
and exact DeliveryStatus message ID, derive milestones only from
validated structured events, refuse synthetic provenance fallback
hashes, and fail closed with one of the typed blockers
`runner-reference-process-not-executed`,
`runner-reference-events-missing`,
`runner-synthetic-provenance-rejected`, or
`runner-protocol-event-unproven`.

Plan 075 does not build i2pd, run a real mixed-router direction,
add Docker/QEMU/namespaces/CI, change NTCP2 protocol code, or
produce a Level 2 or Level 3 record. The Plan 078 attempt stopped
before TCP and is not protocol evidence; the corrected active
sequence is Plan 082 → Plan 083 → Plan 084.

### Plan 077 constrained-host execution lane

Plan 077 closes its provisioning work with a typed no-full-runtime-lane
record. The current host cannot access its Docker daemon and has no QEMU
system emulator; `PR_SET_NO_NEW_PRIVS` is available only for the explicitly
reduced inherited-descriptor diagnostic. No protocol run occurred, so the
support row remains experimental and non-advertised. Plan 078 requires a
separately qualified full-runtime lane.

Plan 078 used the Plan 080-qualified guest but stopped before TCP at the i2pr
pre-protocol RouterInfo stage. That result is not protocol evidence. No
support-ledger status changed and NTCP2 remains experimental and
non-advertised. See [`plans/078-status.md`](../plans/078-status.md) and
[`plans/080-status.md`](../plans/080-status.md).

## Active status correction (2026-08-13)

The active Milestone 4 parent authority is
[Plan 102](../plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md)
and the
[Plan 102 amendment](../plans/102-amendment-exploratory-tunnel-dependency.md)
clarifies that live RouterInfo lookup through a direct router
transport is not a substitute for the standard exploratory-tunnel
path. Plans 103, 104, 105, and 106 have all landed locally on this
host. Plan 103 created the runtime-neutral `i2pr-netdb` workspace
crate (`crates/i2pr-netdb/`) with cryptographic/temporal validation,
a bounded in-memory store with deterministic
replacement/conflict/expiry, peer-selection primitives, and local
signed RouterInfo construction. Plan 104 added the persistent
`ByteCache` and SU3 reseed trust pipeline; Plan 105 added the
transport-neutral lookup state machines. Plan 106 composes these
surfaces into the real `i2pr` daemon with a bounded bootstrap
pipeline (`[netdb]`/`[reseed]` configuration, `BootstrapState`
vocabulary, `bootstrap_daemon`, `netdb-bootstrap` service) and a
runtime-facing `NetDbSeam` that reports
`BlockedExploratoryTunnelUnavailable` until Milestone 5 supplies
exploratory tunnels. The Plan 103/104/105/106 closure records are
`plans/103-status.md`, `plans/104-status.md`, `plans/105-status.md`,
and `plans/106-status.md`. The next executable implementation is
Milestone 5 exploratory tunnels → Milestone 4B external acceptance.

The retained Milestone 3 forward-direction closure lane was governed by
[Plan 095](../plans/095-ci-host-loopback-live-wire-evidence-lane.md), with
[Plan 096](../plans/096-plan095-ci-workflow-correctness-and-pre-dispatch-closure.md),
[Plan 097](../plans/097-plan095-artifact-path-and-cleanup-corrective-pass.md),
and
[Plan 098](../plans/098-plan095-runner-provenance-boundary-corrective-pass.md)
as closed corrective passes that restored execution correctness before
the next authoritative dispatch. The Plan 082 prepare / validate-scenario
surface is implemented and closed; Plans 083 and 084 are implemented and
reclassified as execution-pending. The Plan 084 historical
`lane-invalidated` closure is reclassified as "runner implementation
completed; required reverse wire execution never occurred" and the
active development decision now lives in `plans/088-status.md`. The
Plan 078/080 attempt stopped pre-protocol and did not produce a TCP,
NTCP2, authenticated-frame, or I2NP result. Plan 082 prepares authentic
i2pr state and real RouterInfo/hash/run-identity fields, the Rust
`validate-scenario` command parses the strict live scenario without opening
a peer, and the mixed runner asserts both peer identities and the frozen
run identity before any live process. This changes diagnostic ownership
only; it does not change any support row.

Plan 085 introduced the bounded `host-loopback-development` topology
kind that allows literal IPv4 loopback protocol execution on the
constrained host. Plan 086 enabled the lane and proved a listener-only
preflight; Plan 086 closed as `host-loopback-development-ready` on this
host. Plan 087 ran the first real `i2pr -> i2pd` forward direction under
the development lane; Plan 090, Plan 091, Plan 092, Plan 093, and Plan
094 applied i2pd direct driver corrections and runner/provenance
authority corrections. Plan 094's live closure environment is blocked
on this host, and Plan 095 supersedes that path with a manual GitHub
Actions `ubuntu-24.04` host-loopback evidence lane. The first
authoritative Plan 095 manual CI dispatch on 2026-08-10 advanced through
the full contract/build/forward-instrumented job graph but failed
closed with `terminal_result = pre_protocol_rejected /
pre-protocol-preparation-failed` before any TCP or NTCP2 wire activity.
Plan 098 reclassified that result as a pre-protocol runner/provenance
failure (the runner reconstructed a non-authoritative
`target/debug/i2pr-interop` path instead of consuming the
wrapper-supplied `--i2pr-binary` path) and corrected the
runner/provenance ownership boundary before any future dispatch. Plan
088 runs the reverse `i2pd -> i2pr` direction and issues the active
development decision; on this host the recorded Plan 088 decision
remains `insufficient-evidence` until Plan 095 closes with a passing
instrumented and a passing control forward record from the same CI
evidence pair.

NTCP2 remains experimental and non-advertised, and Plan 079 remains
blocked pending the Plan 088 decision. Plan 072 remains inactive. It
requires a real wire-stage i2pr/i2pd disagreement that source and
specification review cannot own, plus
`decision = ambiguous-reference-divergence` and one exact diagnostic
question in [`plans/088-status.md`](../plans/088-status.md). The
historical `lane-invalidated` and `same-stage-two-way-i2pr-defect`
tokens are forbidden by the static boundary checker. Preparation-only
and pre-protocol results cannot activate Emissary or change this
support ledger. The
[Plan 072/079 gate amendment](../plans/072-079-gate-amendment-plan-088.md)
records the active gate authority.

The current status of the active sequence is:

```text
plan_099 = passed-pruning-and-exit
plan_095 = ci-live-wire-lane-corrected-awaiting-one-authoritative-run
plan_098 = passed-runner-provenance-boundary-correction (historical)
plan_087 = open-pending-plan095-ci-forward-evidence-pair
plan_088 = blocked-pending-plan095-ci-closure
plan_079 = deferred-to-pre-activation-checkpoint
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
normal_daemon_activation = disabled
router_construction = next
```

### Plan 095 CI host-loopback live-wire evidence lane

[Plan 095](../plans/095-ci-host-loopback-live-wire-evidence-lane.md)
implements the GitHub Actions `ubuntu-24.04` host-loopback live-wire
evidence lane that runs the Plan 086 `host-loopback-development` topology
on a fresh VM. The lane is **development-only**; it never satisfies a
release or isolation qualification and cannot become a Milestone 3
certificate. The workflow lives at
`.github/workflows/ntcp2-interop-host-loopback-development.yml` with a
manual `workflow_dispatch` trigger only, `contents: read` permissions,
and the contract/build/forward-instrumented/forward-control/validate-gate
job sequence.

The CI environment blocker vocabulary is bounded
(`ci_binary_execution_blocked`, `ci_loopback_bind_blocked`,
`ci_loopback_connect_blocked`, `ci_reference_build_blocked`,
`ci_artifact_transfer_blocked`, `ci_disk_space_blocked`,
`ci_unexpected_runner_environment`); CI inability is never reported
as a protocol failure. Plan 095 supersedes the Plan 094 assumption that
the Plan 046 rootless sealed-namespace lane or the Plan 048/049
Multipass guest must become runnable before development-only forward
evidence can close.

### Plan 096/097/098 Plan 095 corrective passes

[Plan 096](../plans/096-plan095-ci-workflow-correctness-and-pre-dispatch-closure.md)
closed four demonstrated workflow defects before the first authoritative
dispatch: explicit i2pr build path, disjoint sanitized evidence,
embedded Python import audit, and canonical tracked-source digest. The
static regression matrix `test_plan096.py` (36 cases) and the
pre-dispatch audit `scripts/check-plan095-workflow.sh` are green
locally.

[Plan 097](../plans/097-plan095-artifact-path-and-cleanup-corrective-pass.md)
closed two narrow workflow defects that remained after Plan 096:
artifact-path ownership (one canonical absolute `BUILD_OUTPUT` path used
by every producer, verifier, manifest generator, artifact uploader, and
live consumer) and disposable run-root cleanup (strict `rm -rf --` with
an exact path guard and an unsuppressed absence assertion). The static
regression matrix `test_plan097.py` (45 cases) and the extended
pre-dispatch audit are green locally.

[Plan 098](../plans/098-plan095-runner-provenance-boundary-corrective-pass.md)
closed the runner/provenance ownership boundary that the first
authoritative Plan 095 dispatch exposed on 2026-08-10. The forward,
reverse, and preflight runners now accept an explicit `i2pr_binary: Path`
argument and rehash the supplied file bytes against `i2pr_binary_sha256`
before any subprocess launch. The wrapper threads the exact
caller-supplied `--i2pr-binary` path to every runner, exposes
`--attempt-kind` for instrumented/control role binding, and refuses
role/binary mismatches. The i2pr and i2pd build-manifest digests are
independently measured; the runner no longer aliases a generic manifest
digest into both artifact classes. The Plan 095 final gate validates
record digests against the actual downloaded artifacts and role-specific
manifests. The static regression matrix `test_plan098.py` (15 cases) is
green locally. The 2026-08-10 result is reclassified as a pre-protocol
runner/provenance failure with no TCP or NTCP2 wire conclusion.

### Plan 099 Milestone 3 interop exit, harness reduction, and router buildout

[Plan 099](../plans/099-ntcp2-interop-exit-harness-simplification-and-router-build-unblock.md)
is the corrective and exit plan from the multi-job CI/provenance
expansion. It corrects the central Plan 099 implementation finding
— the instrumented i2pd transport libraries were never actually
compiled from the patched source tree — and it freezes
interoperability architecture growth. The build script now produces
two separate i2pd archive sets (`I2PD_INSTRUMENTED_LIB_DIR` and
`I2PD_PRISTINE_LIB_DIR`), and the pristine control driver uses
native `Transports::SendMessage`, `Transports::IsConnected`, and
`TransportSession::IsEstablished` instead of observer APIs the
control build cannot emit. The development exit gate vocabulary is
bounded to three values: `passed`, `protocol-defect-localized`,
`environment-or-harness-blocked`.

[Plan 100](../plans/100-plan099-exit-gate-cleanup-and-router-handoff.md)
is the one-time active cleanup authority that repairs the exit gate
(D1, D2), hardens the i2pd observer proof (D3), and removes the
divergent source-tree digest fallback (D4). The Plan 099 single-job
CI workflow was dispatched exactly once from the Plan 100 correction
commit and the bounded replacement runs consumed two narrow direct
corrections before the bound forward-instrumented attempt reached
authentic post-TCP protocol evidence.

After Plan 100:

```text
plan_099 = closed-protocol-defect-localized
plan_100 = passed-exit-cleanup-and-handoff
plan_095 = historical-superseded-by-plan099-single-job-lane
plan_087 = historical-development-sequence-superseded-by-plan100
plan_088 = historical-development-sequence-superseded-by-plan100
plan_079 = deferred-to-pre-normal-ntcp2-activation-and-public-network-checkpoint
plan_072 = inactive-pending-plan088-ambiguity
ntcp2    = experimental-non-advertised
normal_daemon_activation = disabled
router_construction = next
development_interop = protocol-defect-localized
exact_wire_stage = noise_authenticated
external_netdb_over_ntcp2 = blocked
```

Plan 099 deleted all Plan 052–098 plan-number-specific Python test
and runner files after migrating unique functional assertions into
the bounded functional test set
(`test_execution_lane.py`, `test_i2pd_direct_driver.py`,
`test_i2pd_direct_control.py`, `test_minimal_i2pd_probe.py`). The
`scripts/check-ntcp2-interoperability.sh` static boundary check
was trimmed from 1870 to 124 lines and now enforces only durable
invariants (NTCP2 remains experimental/non-advertised, the
production daemon does not accidentally activate NTCP2, the direct
reference driver is test-only, no public-network/reseed/SAM/I2CP
fallback in the development smoke, the pinned reference revision
exists, and functional interop tests exist). The
`scripts/check-plan095-workflow.sh` and
`scripts/check-ntcp2-loopback-smoke-boundary.sh` scripts were
removed entirely. The CI workflow file was reduced from 988 lines
to a single `development-interop` job and now performs build and
execute in the same fresh job with no cross-job binary artifact
transfer. The i2pd driver build script now produces two separate
instrumented and pristine archive sets, the driver CMake consumes
them through explicit `I2PD_INSTRUMENTED_LIB_DIR` and
`I2PD_PRISTINE_LIB_DIR` variables, and Plan 100 D3 hard-asserts
that the pristine archive carries exactly zero observer references
and the instrumented archive carries at least one.

Plan 100 does not enable NTCP2 in normal daemon operation, does not
advertise NTCP2 in production RouterInfo, does not depend on NTCP2
for real NetDB peer exchange, and does not authorize public-network
bootstrap. Plan 079's 3/3 repeated-direction validation campaign is
moved to the pre-normal-activation / pre-public-network integration
checkpoint rather than gating offline/local router development.

## Plan 102 Milestone 4 RouterInfo/NetDB authority and the Plan 102 amendment (active parent; amendment closed)

[Plan 102](../plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md)
is the active Milestone 4 parent authority that supersedes the
historical Milestone 3 "active" blocks for the purpose of
continuing router development. The retained Plan 099/100/101
NTCP2 result above is preserved as the authoritative NTCP2
development record. Plans 103/104/105/106 closed locally; Plan 107
lands the first Milestone 5 implementation surface (the
exploratory tunnel substrate); Plan 108 landed the local
short-build architecture but its wire/cryptographic algorithm
diverges from the current official I2P Tunnel Creation
Specification.
The amendment closure and future-plan unblock audit are recorded in
[`plans/102-amendment-status.md`](../plans/102-amendment-status.md).

### Plan 102 amendment — exploratory-tunnel dependency

[Plan 102 amendment](../plans/102-amendment-exploratory-tunnel-dependency.md)
corrects an over-optimistic wording in the first Plan 102 draft.
The current I2P `DatabaseLookup` operation uses an outbound
exploratory tunnel and requests the response through an inbound
exploratory tunnel; exploratory tunnels are Milestone 5 scope.
Therefore a standards-conformant live RouterInfo lookup cannot
complete inside the Plan 103–106 implementation sequence merely
by re-entering NTCP2 or another direct router transport.

The authoritative Plan 102 sequence is:

```text
Plan 103  RouterInfo validation + bounded local NetDB                 [closed]
    -> Plan 104  persistent cache + SU3 reseed trust/ingestion         [closed]
      -> Plan 105  transport-neutral lookup/store/publication states     [closed]
      -> Plan 106  daemon/bootstrap integration                          [closed]
      -> Plan 107  exploratory tunnel substrate                          [closed]
      -> Plan 108  local short-build architecture                        [superseded-by-plan109]
      -> Plan 109  exact short-record + Noise-N/KDF correction           [superseded-by-plan111]
      -> Plan 110  multi-record preprocessing + local conformance close  [superseded-by-plan111]
      -> Plan 111  final local short-build conformance correction         [passed-final-local-short-build-conformance]
      -> Plan 112  outbound pre-delivery closure                           [passed-outbound-pre-delivery-closure]
      -> Plan 113  inbound reference reconciliation                        [passed-inbound-reference-reconciliation]
      -> Plan 114  terminal routing + tunnel-chain correction               [passed-terminal-routing-chain-correction]
      -> Plan 115  canonical production I2NP bridge + Q0 native Emissary OBEP reply [passed-emissary-q0-construction-and-obep-reply-only]
      -> Plan 116  local tunnel data plane                                 [passed-final-local-closure]
      -> Plan 117  exploratory NetDB composition                           [closed-for-progression-with-evidence-gap]
       -> Plan 118  planning authority cleanup + Plan 117 disposition       [closed]
 -> Plan 119  LeaseSet2 protocol foundation                           [passed-leaseset2-protocol-foundation]
        -> Plan 120  destination lifecycle and tunnel pools                  [passed-destination-lifecycle-and-pools]
        -> Plan 121  ECIES-X25519 Garlic/session layer                        [passed-ecies-destination-session-layer]
        -> Plan 122  destination routing and NetDB composition                [passed-corrected-local-destination-routing]
        -> Plan 123  minimal streaming core                                    [passed-corrected-streaming-wire-local]
        -> Plan 124  destination-routing corrective closure                   [passed-corrected-destination-routing-local-closure]
        -> Plan 125  Streaming corrective closure                             [superseded-by-final-corrective-closure]
        -> Plan 126  ECIES-X25519 ratchet corrective foundation                [passed-ecies-destination-ratchet-corrective-foundation; production representation corrected by Plan 130]
        -> Plan 127  destination-session routing final closure                 [passed-destination-session-routing-final-closure]
        -> Plan 128  Streaming wire-protocol corrective closure                 [passed-streaming-wire-protocol-corrective-closure]
        -> Plan 129  integrated destination+Streaming final gate                [superseded-by-plan130-final-gate]
        -> Plan 130  final wire/runtime corrective closure                       [superseded-by-plan131-final-local-correctness-gate]
        -> Plan 131  final local correctness closure                             [passed-milestone6-final-local-correctness-closure] (milestone6_local_product = passed; interoperability not claimed)
```

Plan 106 closed the local/bootstrap implementation phase; Plan 107
landed the runtime-neutral exploratory pool, the typed build-record
layout surface, the build-cryptography seam, and the reply-path
provider that flips the Plan 106 NetDB seam from
`BlockedExploratoryTunnelUnavailable` to `Available` once a real
inbound tunnel is registered. Plan 108 landed the local
short-record construction architecture but its wire/cryptographic
algorithm is **not** protocol-conformant against the current
official I2P Tunnel Creation Specification; see
[`plans/108-conformance-amendment.md`](../plans/108-conformance-amendment.md)
and the Plan 109/110 corrective roadmap at
[`plans/109-110-plan108-short-build-protocol-conformance-corrective-roadmap.md`](../plans/109-110-plan108-short-build-protocol-conformance-corrective-roadmap.md).
Plan 109 corrected the wire format, Noise-N transcript, layer-encryption
type, request/reply key derivation, response codes, and 218-byte
envelope layout but missed four defects that Plan 111 reopens.
Plan 110 added randomized slot allocation, fake records, raw ChaCha20
preprocessing/postprocessing, and the one-byte-count STBM/OTBRM
payload framing but inherited the Plan 109 defects. Plan 111 closed
as `passed-final-local-short-build-conformance` and corrected the
remaining defects (Noise null prologue, single-HKDF `es` split, slot
byte at offset 4, 8-byte OBEP garlic tag, explicit per-hop tunnel
IDs, role-aware hop processor, frozen independent fixed vectors).
Inbound creator-ephemeral layout remains
`reference-compatible-spec-text-discrepancy` under Plan 113: pinned Java I2P
and i2pd agree on the originator fake, but the final-spec prose still does
not define a concrete separate plaintext creator-key encoding. No guessed
field is added; see `../specs/references/short-build-inbound-creator-key.md`.
Plan 114 closed four post-Plan-113 routing/composition defects
(explicit outbound `outbound_reply_router` / inbound `originator_hash`
terminal-routing, intermediate tunnel-id chain continuity enforcement,
strict outbound/inbound E2E trajectories), and Plan 115 closed the
canonical production I2NP bridge (`ShortBuildI2npBridge` in
`crates/i2pr-tunnel/src/bridge.rs`) with the no-double-prefix
STBM record count byte invariant and closed the Plan 115 Q0
independent-consumer seam on this host as
`passed-emissary-q0-construction-and-obep-reply-only` against pinned
Emissary revision `9b43484a21d5a1291c4881cdae62a36c527f8c0f`
(emissary-core 0.4.0); the i2pr-produced STBM is consumed by
Emissary's native short-build handler, Emissary replies with
`TunnelGateway` + Garlic inner message + feedback channel. The
historical Plan 115 Branch E closure
(`blocked-no-bounded-independent-consumer-seam`) is superseded by
this Q0 completion but is preserved as historical context in
[`plans/115-status.md`](../plans/115-status.md) and
[`plans/115-handoff.md`](../plans/115-handoff.md). Q1 (authenticated
transport delivery) and Q2 (reply round-trip to `Established`)
remain pending on a qualified external delivery lane. Milestone
4A is now
`local-foundation-complete-short-build-outbound-conformant-fixed-vectors`
with `inbound_short_build = locally-reference-compatible`,
`canonical_i2np_bridge = locally-conformant-no-double-prefix`, and
`independent_short_build = passed-emissary-q0-native-consumer`.
Plan 116 closed the local tunnel data plane
(`passed-final-local-closure` via the
completion/correction/terminal-cleanup sequence). Plan 117 closed
per Plan 118 as `closed-for-progression-with-evidence-gap`: the
local Phase G production composition remains passed, while the
corrected in-tree native Emissary reference test rejected the
pinned reference's request-prefixed reply during strict i2pr
Mapping decoding; the reference-side defect is localized to the
pinned Emissary revision, and Plan 118 Phase B1 confirmed no
upstream correction is available. Router construction is
`may-continue`. Plan 119 closed as `passed-leaseset2-protocol-foundation`
per [`plans/119-status.md`](../plans/119-status.md); Plan 120 closed
as `passed-destination-lifecycle-and-pools` and lands the first
`i2pr-client` destination runtime; Plan 121 closed as
`passed-ecies-destination-session-layer` and lands the first real
ECIES-X25519-AEAD-Ratchet destination Garlic/session layer (audited
`curve25519-elligator2 = 0.1.0-alpha.2` primitive, wrapped ECIES
primitives in `i2pr-crypto`, bounded structural Garlic payload block
codec in `i2pr-proto`, and bounded destination-context
`EciesSessionManager` in `i2pr-client`). Plan 122 closed as
`passed-corrected-local-destination-routing` per [`plans/122-status.md`](../plans/122-status.md)
and [`plans/124-status.md`](../plans/124-status.md)
and composes the Plan 119 LeaseSet2 lookup surface, the Plan 120
destination runtime, the Plan 121 ECIES session layer, and the
Plan 116 tunnel data plane into the first complete local
destination routing pipeline. The next executable plan is
**Plan 123** (minimal streaming core)
under the Milestone 6 router-construction roadmap at
[`plans/118-123-milestone6-router-construction-roadmap.md`](../plans/118-123-milestone6-router-construction-roadmap.md).

### Plan 113 inbound short-build reconciliation

Plan 113 closes the inbound creator-key discrepancy as
`passed-inbound-reference-reconciliation`. The real 154-byte request keeps
the fixed fields plus Mapping/padding; exactly one separately randomized
originator fake carries `hash16 || fresh X25519 pub32 || random remainder`
and is integrity-checked after reply processing. This matches the pinned Java
I2P and i2pd construction, but remains
`reference-compatible-spec-text-discrepancy` rather than strict final-spec
text conformance because the specification does not define a concrete
plaintext creator-key encoding. See
[`specs/references/short-build-inbound-creator-key.md`](../specs/references/short-build-inbound-creator-key.md)
and [`plans/113-status.md`](../plans/113-status.md).
