# `i2pr-transport-ssu2` — Deep Dive

Runtime-neutral SSU2 v2 protocol implementation: strict RouterAddress
validation, structural packet-header codecs, the bounded
authenticated-plaintext block vocabulary, the complete Noise XK
establishment handshake with header protection, the bounded one-use
token lifecycle, RouterInfo establishment binding, consuming
initiator/responder state machines, and the authenticated data-phase
session with reliability/fragmentation. No UDP sockets.

Path: `crates/i2pr-transport-ssu2/`

## Purpose

`i2pr-transport-ssu2` owns the SSU2 v2 protocol mechanics that can be
expressed without I/O — **no Tokio, no sockets, no `async`
functions, no timers, no tasks.** Every public API is synchronous
and bounded. Plan 155 landed the address/header/block foundation;
Plan 156 added the Noise XK handshake and establishment state
machines; Plan 157 added the authenticated data-phase session
(`session.rs`). The supervised UDP adapter in `i2pr-runtime`
(`Ssu2RuntimeService`) landed in Plan 158 and drives exactly these
machines over real sockets.

It does own:

- Spec-traced constants (versions, header/message/block IDs, size
  bounds, handshake schedules, token quotas) in `constants.rs`.
- Strict `RouterAddress` parsing: direct, introducer-only, and
  unpublished-static forms, plus distinct listen/dial types
  (`address.rs`).
- Structural long/short header codecs with exact-size discipline
  (`header.rs`).
- Datagram length validation and header/payload splitting
  (`packet.rs`).
- The bounded authenticated-plaintext block vocabulary with
  unknown-block budget and terminal-ordering rules (`block.rs`).
- The Noise XK transcript, header protection, token-payload AEAD,
  and data-phase key derivation (`crypto.rs`, Plan 156).
- Establishment message codecs, cheap prevalidation, confirmed
  reassembly, and RouterInfo binding (`handshake.rs`, Plan 156).
- The bounded one-use token table (`token.rs`, Plan 156).
- Consuming initiator/responder establishment machines with bounded
  retransmit/deadline actions (`state_machine.rs`, Plan 156).
- The authenticated data-phase session with replay window, ACK
  scheduling, loss/congestion control, fragmentation/reassembly,
  duplicate suppression, and termination/idle handling
  (`session.rs`, Plan 157), plus three Plan 158 runtime-support APIs:
  the single-shot `queue_new_token` control (in-band future-handshake
  tokens), the side-effect-free `matches_inbound` trial match for
  socket receive routing, and the read-only `outbound_pending`
  depth accessor for send admission.
- The `PeerId::hash` read-only accessor in `i2pr-transport`
  (`identity.rs`, Plan 158) that lets the runtime bind dial backoff
  without touching redacted diagnostics.

It does **not** own UDP sockets (those live in `i2pr-runtime` since
Plan 158), peer-test/relay roles, or transport selection. Those
belong to Plans 159–161.

## Module layout

Declared in `src/lib.rs:28-36`. No subdirectories. Integration
trajectories live in `tests/handshake.rs`.

| File | Responsibility | Main public types |
| --- | --- | --- |
| `src/lib.rs` | Crate root, module declarations + re-exports | (re-exports) |
| `src/constants.rs` | SSU2-dossier-derived constants with source comments | `SSU2_VERSION`, `SSU2_NETWORK_ID`, `NOISE_PROTOCOL_NAME`, message-type IDs, block-type IDs, token quotas, resend schedules, bound ceilings |
| `src/address.rs` | Strict `RouterAddress` parsing, introducers, listen/dial types, I2P-base64 decoding | `Ssu2RouterAddress`, `Ssu2AddressMaterial`, `Ssu2Endpoint`, `Ssu2Capabilities`, `Ssu2Introducer`, `Ssu2AddressClass`, `ConfiguredListenAddress`, `ResolvedDialTarget`, `Ssu2TransportStyle`, `Ssu2AddressError` |
| `src/header.rs` | Long/short header encode/decode, message-type vocabulary | `MessageType`, `HeaderForm`, `LongHeader`, `SessionConfirmedHeader`, `DataHeader`, `HeaderError` |
| `src/packet.rs` | Datagram length classes, header/payload split | `DatagramLengthClass`, `PacketHeader`, `SplitPacket`, `split_packet`, `PacketError` |
| `src/block.rs` | Bounded payload block codec | `Block`, `DecodedBlock`, `ParsedBlocks`, `AckBlock`, `TerminationBlock`, `TerminationReason`, `RelayResponseCode`, `PeerTestBlock`, `ReceivedI2npBlock`, `BlockError` |
| `src/crypto.rs` | Noise XK transcript, header protection, token AEAD, data keys | `Ssu2Transcript`, `Ssu2PublicKey`, `IntroKey`, `Role`, `Ssu2SplitKeys`, `DataCipher`, `TranscriptHash`, `Ssu2CryptoError` |
| `src/handshake.rs` | Establishment codecs, prevalidation, reassembly, RouterInfo binding | `TokenRequest`, `RetryMessage`, `SessionRequestParts`, `SessionCreatedParts`, `ConfirmedReassembly`, `AuthenticatedPeer`, `ClockSkewPolicy`, `HandshakeReplayCache`, `ReplayToken`, `RouterInfoFreshness`, `HandshakeError` |
| `src/token.rs` | Bounded one-use token table | `TokenStore`, `Ssu2Token`, `TokenError`, `retry_response_budget` |
| `src/state_machine.rs` | Consuming initiator/responder machines | `Initiator`, `Responder`, `InitiatorConfig`, `ResponderConfig`, `HandshakeAction`, `AuthenticatedSsu2Session`, `DeadlineKind`, `TerminateReason`, `DropCategory`, `StateMachineError` |
| `src/session.rs` | Authenticated data-phase session | `Ssu2Session`, `SessionConfig`, `SessionCounters`, `SessionEvent`, `SessionAction`, `SessionError`, `ReceiveOutcome`, `DropReason` (+ Plan 158: `queue_new_token`, `matches_inbound`, `outbound_pending`) |

## Public surface

Crate-root re-exports (`lib.rs:38-72`):

```rust
pub mod address;
pub mod block;
pub mod constants;
pub mod crypto;
pub mod handshake;
pub mod header;
pub mod packet;
pub mod session;
pub mod state_machine;
pub mod token;

pub use address::{...};
pub use block::{...};
pub use crypto::{...};
pub use handshake::{...};
pub use header::{...};
pub use packet::{...};
pub use state_machine::{...};
pub use token::{...};
```

`pub trait` count: **zero.** Like `i2pr-transport`, every contract
is a concrete struct/enum.

## Key contracts

### RouterAddress (`address.rs`)

- `Ssu2RouterAddress::parse` — strict validation of a structural
  `RouterAddress`: style must be `SSU2`; `v=2` required
  (anything else is `UnsupportedVersion`, including PQ-hybrid
  v3/v4); `s` is a 32-byte I2P-base64 static key (all-zero
  rejected); `i` is a 32-byte intro key (required with an
  endpoint or introducers, forbidden for the unpublished
  static-only form); `host` must be a numeric IP (hostnames
  refused); `port` is canonical decimal `1..=65535`;
  `host`/`port` must appear together; `mtu` is optional and
  validated `1280..=9000`; `caps` is a bounded graphic string
  with duplicate-known-flag rejection (`4`/`6` families,
  `B` peer-test, `C` relay; other letters ignored for forward
  compatibility); up to 3 introducer groups
  (`ihostN`/`iportN`/`ikeyN`/`itagN`, dense from 0, nonzero
  tags); unknown options rejected.
- `address_class()` — `Direct` / `DirectWithIntroducers` /
  `IntroducerOnly` / `UnpublishedStatic`. Describes present
  contact material only; never implies reachability or
  publication approval.
- `ConfiguredListenAddress` / `ResolvedDialTarget` — distinct
  endpoint + material types with no socket ownership; dial
  targets require an exact literal endpoint match.
- `Debug` redacts endpoints and key material everywhere.

### Headers (`header.rs`)

- `MessageType` — the 8 assigned types (0/1/2/6/7/9/10/11);
  unassigned bytes are `UnknownMessageType`.
- `HeaderForm::classify_prefix` — long vs short from the
  spec-defined type byte at offset 12, not heuristics.
- `LongHeader` — exact 32 bytes; version must be 2
  (`UnsupportedVersion`), network ID must be 2
  (`InvalidNetworkId`), reserved flags must be 0, source and
  destination connection IDs must differ.
- `SessionConfirmedHeader` — exact 16 bytes; packet number must
  be 0; frag byte is number 0..=14 over total 1..=15 with
  number < total.
- `DataHeader` — exact 16 bytes; reserved flag bits and
  `moreflags` must be 0; bit 0 is the immediate-ACK request.
- All decoders reject short inputs (`Truncated`) and trailing
  bytes (`TrailingBytes`).

### Datagrams (`packet.rs`)

- `DatagramLengthClass::classify` — 40-byte minimum, 1452-byte
  IPv6 max, 1472-byte IPv4 max, without touching packet bytes.
- `split_packet` — validates length, classifies the form,
  decodes the exact header, then requires the minimum
  authenticated tail (24 bytes; 56 for SessionRequest/
  SessionCreated, which carry the 32-byte ephemeral key)
  before exposing the opaque post-header bytes. No crypto.

### Blocks (`block.rs`)

- 20 outbound `Block` variants covering the spec table 0–10,
  12–13, 15–21, and 254. Type 11 (NextNonce, spec TODO)
  decodes as `UnsupportedBlock`; reserved 14/255 and
  experimental 224–253 skip under the unknown budget.
- `encode_blocks` / `parse_blocks` enforce: at most 64 blocks,
  1024 aggregate unknown bytes, Padding at most once and last,
  Termination at most once and last-non-padding. All other
  known blocks may repeat. SessionConfirmed RouterInfo-first is
  enforced by the handshake payload check (`handshake.rs`), not
  here.
- Per-block strictness: DateTime exactly 4; Options 12+
  (fixed-point ratios, no ordering assumption); RouterInfo
  flags limited to bits 0–1 with frag byte exactly `0x01`
  (never fragmented) and a 4096-byte ceiling; I2NP/first-fragment
  require the 9-byte header with nonempty bodies;
  follow-on fragments require number 1..=127 with nonempty
  bodies; ACK ranges reject (0,0) pairs with a 128-range cap
  (interpretation belongs to Plan 157); Address is 6 or 18
  bytes, port first; RelayTagRequest empty; RelayTag nonzero;
  NewToken exactly 12; FirstPacketNumber exactly 4;
  Congestion 1+ bytes; path data capped at 1024;
  relay/peer-test signatures retained as bounded (1024)
  opaque evidence with strict fixed-prefix parsing
  (verification belongs to Plan 160).

### Noise transcript (`crypto.rs`)

- `Ssu2Transcript` — consuming initiator/responder transcript for
  the SSU2-specific Noise XK pattern
  (`Noise_XKchaobfse+hs1+hs2+hs3_25519_ChaChaPoly_SHA256`):
  initial `SHA256(protocol_name)` chaining with null-prologue and
  responder-static mixes; `e,es` / `e,ee` / `s,se` stages with
  role-gated transitions (`WrongRole`/`InvalidState`
  otherwise); the `es` cipher is retained for the static-key
  frame (`n = 1`); `split()` derives `k_ab`/`k_ba` via
  `HKDF(ck, ZEROLEN, "", 64)` and then the data-phase
  `HKDF(key, ZEROLEN, "HKDFSSU2DataKeys", 64)` into
  `(k_data, k_header_2)` per direction (Plan 157 correction; the
  AEAD cipher uses `k_data`, header protection uses `k_header_2`
  with the receiver intro key as `k_header_1`).
- `apply_header_protection` / `remove_header_protection` — the
  Header Encryption KDF verbatim: ChaCha20 masks over the first
  16 header bytes keyed by the packet's trailing MAC bytes, plus
  the 48-byte (Request/Created) or 16-byte (Retry/TokenRequest)
  third-part stream under the zero nonce. Two recorded
  interpretation notes (the `n: 1` annotation, inclusive index
  notation) live in the module docs.
- `seal_token_payload` / `open_token_payload` — Retry/TokenRequest
  AEAD under the intro key with the header packet number as nonce
  and the cleartext header as associated data.
- `session_created_header_key` / `session_confirmed_header_key` /
  `derive_data_keys` — the `SessCreateHeader`, `SessionConfirmed`,
  and `HKDFSSU2DataKeys` labeled derivations.
- Secrets (`ChainKey`, cipher keys) are zeroizing owners without
  `Debug`/`Clone`; DH inputs arrive as checked
  `X25519SharedSecret` values so private keys never enter the
  transcript; nonces refuse the forbidden `2^64 - 1`.

### Establishment codecs (`handshake.rs`)

- `build/parse_token_request`, `build/parse_retry` — intro-key
  AEAD payloads (DateTime required; Address required in Retry;
  Termination optional in Retry, which then carries a zero
  token); Retry enforces the 3x amplification budget and the
  64-byte padding cap.
- `prevalidate_long_datagram` — symmetric-only cheap gate
  (length class, deprotection, exact header decode,
  version/network/type, minimum tail) before any DH, payload
  AEAD, or session allocation.
- `build/parse_session_request`, `build/parse_session_created` —
  header plus ephemeral plus transcript-ciphertext assembly with
  the phase-correct protection keys.
- `build_session_confirmed` / `ConfirmedReassembly` — bounded
  fragmentation (≤15 fragments, 32 KiB aggregate) and exact
  reassembly with duplicate/conflict detection; `frag0`'s header
  is the Noise associated data.
- `validate_router_info` — deep establishment binding without
  touching NetDB: structural decode, signature, expected-hash
  check, `v=2` SSU2 address presence, static-`s` binding against
  the handshake peer (constant-time), intro-`i` shape where
  required, and caller-supplied publication freshness. Returns
  `AuthenticatedPeer` (hash, static key, validated bytes).
- `ClockSkewPolicy::handshake` (±120 s) and
  `HandshakeReplayCache` (bounded, caller-time) cover timestamp
  and ephemeral replay handling.

### Token lifecycle (`token.rs`)

- `TokenStore` — bounded one-use table (256 global, 4 per
  source, 30 s lifetime by default): tokens bind the exact
  source socket address (IP and port; v4/v6 separation falls out
  of the comparison), issuance evicts the oldest entry in a full
  quota deterministically, consumption removes the entry so
  reuse fails closed, `expire` releases accounting, and `rotate`
  models key/generator restart. Randomness and time are
  caller-supplied; `Ssu2Token` rejects zero and redacts `Debug`.

### State machines (`state_machine.rs`)

- `Initiator` — `begin` (TokenRequest without a token,
  SessionRequest with one), `on_retry` (fresh ephemeral,
  token-bearing request), `on_session_created` (Noise completion,
  SessionConfirmed emission, `Established`), `on_timeout`
  (identical-byte resend per the spec schedules, then
  `RetriesExhausted`/`HandshakeTimeout`), `cancel`.
- `Responder` — `on_token_request` (Retry, no DH, no state),
  `on_session_request` (Retry for tokenless; token → replay →
  skew gates, then the single admitted DH and SessionCreated;
  duplicates resend the identical Created), `on_session_confirmed`
  (bounded reassembly, static/payload open, RouterInfo binding,
  `Established`), `on_timeout`, `cancel`.
- Actions are `WriteDatagram` / `ArmDeadline` / `Established` /
  `Terminate` / `DropSilently`; the crate never sleeps, opens
  sockets, or reads clocks. `AuthenticatedSsu2Session` carries
  only what Plan 157 needs: peer material, directional ciphers,
  connection IDs, observed endpoint, and the local MTU.

### Data-phase session (`session.rs`)

- `Ssu2Session::new(config, keys)` — consumes the establishment
  splits plus explicit intro keys; owns send packet numbers (no wrap,
  `u32::MAX` exhaustion), the 128-packet replay bitmap with
  future-jump cap, pending ACK state with one deadline, RTT/RTO/cwnd
  state, sent provenance (256 entries), pending retransmit fragments,
  outbound messages, reassembly (16 messages / 256 KiB), the
  delivered-ID duplicate cache (128 entries), and idle/termination
  state. The full v2 packet number rides the short header, so
  reconstruction is the documented identity plus window policy.
- `queue_i2np_message` — splits encoded messages into fixed
  1024-byte semantic fragments (single-fragment fast path as complete
  blocks, 64-fragment ceiling); `poll_transmit(now)` seals at most one
  MTU-aware datagram (ACK first, then controls, retransmits, new
  fragments) with one AEAD seal plus header protection, honoring the
  congestion gate for ack-eliciting packets (ACK-only always passes).
- `receive_datagram(now_ms, now_secs, bytes)` — ordered pipeline
  returning `ReceiveOutcome` with typed `SessionEvent`s only after
  authentication; replays/tag failures mutate counters alone.
- `poll(now_ms, now_secs)` — drives ACK deadlines, RTO backoff with
  bounded `Timeout` termination, idle timeout, and reassembly expiry.
- `SessionCounters` — privacy-safe counts only (packets, ACKs,
  losses, retransmits, flight, cwnd, reassembly, termination).
- Outbound controls are single-shot; per-message delivery failure
  past the retransmission ceiling is silent by design (counters only)
  while the session stays usable.

## Errors

All error types implement `Display + Error + Eq + PartialEq`
(`Clone + Copy` where the payload allows). No
protocol-vs-operational mixing.

| Error | Module | Semantics |
| --- | --- | --- |
| `Ssu2AddressError` | `address.rs` | Style/version/option/host/port/key/introducer/endpoint failures, including `UnsupportedVersion` and `TooManyIntroducers` |
| `HeaderError` | `header.rs` | Truncation, trailing bytes, unknown type, wrong form, version/network/flag/connection-ID/fragment failures |
| `PacketError` | `packet.rs` | Datagram length classes, header failures, short authenticated tail |
| `BlockError` | `block.rs` | Truncation, over-length, count/budget ceilings, ordering, per-block malformation, `UnsupportedBlock`, oversize payloads |
| `Ssu2CryptoError` | `crypto.rs` | Invalid public key, field bounds, nonce exhaustion, authentication failure, wrong role/state, static mismatch, KDF rejection |
| `HandshakeError` | `handshake.rs` | Truncation/bounds, header/packet/block/crypto mapping, timestamp skew, replay, token rejection, amplification, fragment faults, RouterInfo binding failures |
| `TokenError` | `token.rs` | Zero/unknown/expired/reused/wrong-source token, full table |
| `StateMachineError` | `state_machine.rs` | Handshake/crypto/wrapper mapping, invalid-state driving |
| `SessionError` | `session.rs` | Packet/header/crypto/block mapping, session mismatch, replay/old/future, ACK underflow/invalid, packet-number exhaustion, queue/history/reassembly ceilings, conflict, terminated/policy denial |

## Dependencies

`Cargo.toml:10-20`:

```toml
[dependencies]
chacha20.workspace = true
chacha20poly1305.workspace = true
hmac.workspace = true
i2pr-crypto = { path = "../i2pr-crypto" }
i2pr-proto = { path = "../i2pr-proto" }
i2pr-transport = { path = "../i2pr-transport" }
rand_core = { workspace = true, features = ["os_rng"] }
sha2.workspace = true
thiserror.workspace = true
zeroize.workspace = true
```

`i2pr-crypto` provides the checked X25519 DH, the
RFC 5869 HKDF helpers, and RouterInfo signature verification
(the NTCP2 precedent); transcript `MixKey` policy and all SSU2
labels stay local. No runtime, socket, or async dependencies.
`std::net` appears only as pure data carriers (`IpAddr`,
`SocketAddr`) spelled without the banned literal — matching the
`i2pr-transport-ntcp2` precedent and the runtime-boundary script.

## Tests

103 tests: 66 synchronous unit tests (inline) plus 20 integration
trajectories in `tests/handshake.rs` plus 17 data-phase trajectories
in `tests/data_phase.rs`, plus 14 committed fixtures
under `tests/fixtures/ssu2/` (pinned by `manifest.tsv`,
enforced by `scripts/check-ssu2-vectors.sh`). Plan 158 added the
inline `queued_new_token_round_trips_as_typed_event` and
`inbound_matching_is_side_effect_free` unit tests; the 9-test
real-loopback product suite lives in
`crates/i2pr-runtime/tests/ssu2_local.rs` (see
[i2pr-runtime](i2pr-runtime.md)).

| Area | Coverage |
| --- | --- |
| `constants.rs` | Noise name length, header-layout arithmetic, datagram-bound ordering |
| `address.rs` | Direct IPv4/IPv6, introducer-only, direct-with-introducers, unpublished-static, v3/v4/unknown versions, duplicates/conflicts/unknown options, hostname/port/key/IV/mtu/caps/introducer negatives, endpoint mismatch, missing intro key, debug redaction |
| `header.rs` | Type round-trip + form classification, long exactness + version/network/flag/ID negatives, confirmed zero-packet + frag shape, data flags + immediate ACK, committed long/short fixtures |
| `packet.rs` | Length classes without touching bytes, split validation order, X + auth-tail minima, short/oversize/bad-header rejection |
| `block.rs` | All-21-block round trip, relay accept/reject shapes, truncation at every byte boundary, count/unknown/oversize ceilings, per-block malformation, unknown/reserved skip budget, committed positive/malformed fixtures |
| `crypto.rs` | Full initiator/responder transcript to matching split keys, role/stage gating, tag mutation + wrong-key rejection, header-protection round trip + wrong-key/short negatives, token AEAD round trip + nonce binding, labeled derivations, nonce ceiling |
| `handshake.rs` | TokenRequest round trip + skew stale/future, wrong-intro/truncation rejection, Retry round trip + amplification budget + zero-token rules + clock-skew termination, request build/parse round trip, confirmed fragmentation + duplicate/gap handling, replay cache bounds, cheap prevalidation drops |
| `token.rs` | One-use round trip, zero rejection, expiry + release, wrong source/port/family closure, per-source + global eviction, rotation, retry budget |
| `tests/handshake.rs` | Full tokenless Retry trajectory to matching directional keys, cached-token trajectory, token valid/expired/wrong-source/reuse/rotation/unknown matrix with pre-DH fail-closed evidence, identical-byte resends, duplicate Created/Confirmed handling, deadline exhaustion + per-phase cancellation, tag-mutation isolation, 6-case RouterInfo binding matrix, RouterInfo-not-first rejection, 200-datagram cheap flood with bounded state, amplification budget, secret redaction, 6 committed handshake vectors (one with raw-primitive independent derivation) |
| `tests/data_phase.rs` | Bidirectional multi-message exchange, DATA-loss fresh retransmission with exact once-delivery, ACK-loss recovery without loops, duplicate/replay/corruption/reorder, first/middle/final fragment loss recovery, fragment reorder/duplicate/conflict, reassembly exact-capacity/max+1 with total cleanup, outbound-queue exact-capacity/max+1, congestion-gate boundedness, prolonged-loss bounded termination, idle timeout, termination lifecycle, two-session isolation, 2 committed data-phase vectors reproduced byte-for-byte |
| `session.rs` (unit) | Second-HKDF key shape, ACK underflow without mutation, duplicate-ACK idempotency, sent-history exact eviction, per-fragment ceiling silence, packet-number exhaustion, wrap boundaries, NextNonce rekey boundary, NewToken queue round trip (Plan 158), side-effect-free inbound matching (Plan 158) |

## Distinctive design choices

1. **Version classification, not version parsing** — v3/v4 hit
   `UnsupportedVersion`, keeping the PQ-hybrid door visibly
   closed without a malformed-v2 mislabel.
2. **Strict unknown options** — unlike NTCP2's `pq` carve-out,
   no deployed SSU2 extra option is currently accepted; any
   future one needs an explicit allowlist entry.
3. **Normative pseudocode over annotations** — where the spec's
   Header Encryption KDF pseudocode and a raw-contents `n: 1`
   note disagree on the ephemeral-region nonce, the pseudocode
   governs; the choice is documented and vector-pinned.
4. **Inclusive-index reading** — `keydata[0:31]`, packet IV
   windows, and similar ranges are 32/12-byte inclusive spans;
   the NTCP2 `MixKey` precedent confirms the construction.
5. **Token before DH, always** — unknown/expired/reused/misbound
   tokens drop before any expensive operation; the flood test
   proves bounded state under 200 cheap invalid datagrams.
6. **No NetDB mutation in the handshake** — RouterInfo binding
   returns validated bytes; publication/freshness policy beyond
   the explicit window belongs to the caller.
7. **Caller-supplied determinism** — secrets, connection IDs,
   packet numbers, token bytes, and both clocks arrive as
   parameters; the machines hold no RNG and read no time.
8. **No new crypto dependency** — X25519/HKDF/signature reuse
   comes from `i2pr-crypto`; only transcript sequencing and
   SSU2 labels are local, per the Plan 156 dependency review.

## Cross-references

- [Overview](overview.md)
- [i2pr-transport](i2pr-transport.md) — provides
  `TransportKind::Ssu2`, link/manager contracts, and
  `EncodedI2npMessage`.
- [i2pr-transport-ntcp2](i2pr-transport-ntcp2.md) — sibling
  protocol crate; structural precedent for address/header/
  block discipline and consuming transcripts.
- [Dependency graph](dependency-graph.md) — the
  `i2pr-transport-ssu2` allowlist row.
- [i2pr-runtime](i2pr-runtime.md) — owns the Plan 158 UDP sockets
  and central scheduler that drive these machines.
- Plan-of-record:
  `plans/156-m8-ssu2-v2-handshake-token-and-routerinfo.md`,
  `plans/157-m8-ssu2-v2-data-phase-reliability-and-fragmentation.md`,
  and `plans/158-m8-ssu2-udp-runtime-and-local-session-product.md`.
- Closure: `plans/156-status.md`, `plans/157-status.md`, and
  `plans/158-status.md`.
- Dossier: `specs/protocols/09-ssu2.md`,
  `specs/SOURCES.md` (Milestone 8 refresh).
