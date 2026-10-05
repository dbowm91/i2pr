# `i2pr-transport-ssu2` — Deep Dive

Runtime-neutral SSU2 v2 protocol implementation: strict RouterAddress
validation, structural packet-header codecs, the bounded
authenticated-plaintext block vocabulary, the complete Noise XK
establishment handshake with header protection, the bounded one-use
token lifecycle, RouterInfo establishment binding, consuming
initiator/responder state machines, the authenticated data-phase
session with reliability/fragmentation, authenticated path validation
with per-path MTU state, deterministic address-publication
snapshots, PeerTest roles with typed outcomes, relay
requester/introducer/target machines with HolePunch, and validated
introducer records. Parser-only `pq` tolerance; no UDP sockets.

Path: `crates/i2pr-transport-ssu2/`

## Purpose

`i2pr-transport-ssu2` owns the SSU2 v2 protocol mechanics that can be
expressed without I/O — **no Tokio, no sockets, no `async` functions,
no timers, no tasks.** Every public API is synchronous and bounded.
`src/lib.rs:24` declares `#![forbid(unsafe_code)]`.

The crate root is the single statement of that boundary: "This crate
owns protocol values and sequencing only: no Tokio, no sockets, no
filesystem I/O, no async functions, no timers, and no task ownership"
(`src/lib.rs:11-14`). Verified: `grep -rnE 'tokio|std::net|std::fs|async fn|spawn'
crates/i2pr-transport-ssu2/src` matches only one prose line
(`src/state_machine.rs:6`, "spawns tasks, reads randomness, …" as a
description of what the crate does *not* do). `std::net` appears in
`tests/` only, spelled as pure data carriers (`IpAddr`, `Ipv4Addr`,
`Ipv6Addr`, `SocketAddr`) for loopback test addresses, never as a
socket type. `bash scripts/check-runtime-boundaries.sh` passes.

Plans 155–157 built the crate in layers: Plan 155 landed the
address/header/block foundation; Plan 156 added the Noise XK
handshake, token table, and establishment state machines; Plan 157
added the authenticated data-phase session (`session.rs`); Plan 159
added the path-validation machine (`path.rs`), the publication
snapshot builder (`publication.rs`), and `note_path_migrated`; Plan
160 added PeerTest roles (`peer_test.rs`), relay machines with
HolePunch (`relay.rs`), and validated introducer records
(`introducer.rs`).

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
  and the two-step data-phase key derivation (`crypto.rs`).
- Establishment message codecs, cheap prevalidation, confirmed
  reassembly, and RouterInfo binding (`handshake.rs`).
- The bounded one-use token table (`token.rs`).
- Consuming initiator/responder establishment machines with bounded
  retransmit/deadline actions (`state_machine.rs`).
- The authenticated data-phase session with replay window, ACK
  scheduling, loss/congestion control, fragmentation/reassembly,
  duplicate suppression, and termination/idle handling (`session.rs`),
  plus the runtime-support APIs `queue_new_token` (single-shot
  in-band future-handshake tokens), `matches_inbound` (side-effect-free
  trial match for socket receive routing), `matches_data_header`,
  `request_bootstrap_ack`, and `outbound_pending` (read-only depth
  accessor for send admission).
- Parser-only tolerance of the SSU2 `pq` option
  (`Ssu2PqKem`/`PqCapabilities`, `MAX_SSU2_PQ_SCHEMES`). See the
  dedicated boundary note below — this is **not** a PQ session.

It does **not** own UDP sockets (those live in `i2pr-runtime` as
`Ssu2RuntimeService` and `Ssu2PeerRelayService` since Plan 158), and
it does not own transport selection (generic manager concern, Plan
159). Independent interop is proven by Plan 161 in both direct IPv4
directions against exact-pinned i2pd 2.61.0
(`635b013a612ff47278ef02acf8580a28e10e26c5`) over real loopback UDP
(`plans/closure/ssu2/161-status.md`, the 15-row fail-closed lane in
`tests/integration/ssu2/run-independent.sh`, and its CI-enforced
checker `scripts/check-ssu2-acceptance-evidence.sh`).

### The `pq` boundary — parser tolerance only

**This is the most misreadable surface in the crate.** Read it
carefully.

Java I2P 2.13.0 *unconditionally publishes* an SSU2 `pq` KEM-scheme
option, so an i2pr parser that rejected unknown options outright
would refuse to parse a live Java RouterAddress. `address.rs`
therefore tolerates `pq` at the **parse** layer only
(`src/address.rs:51`, `:53-57`, `:1305-1325`):

- `MAX_SSU2_PQ_SCHEMES: usize = 8` (`src/address.rs:57`) bounds the
  scheme list. The pinned Java router publishes two (`"4,3"`); eight
  covers any plausible production variant without unbounded parser
  allocation.
- `Ssu2PqKem` (`src/address.rs:179-199`) is a diagnostic
  enumeration: `MlKem512` (Java PQ id `3`), `MlKem768` (id `4`), and
  `Unknown(u8)` for anything else. Its own doc comment states "i2pr
  does not implement any KEM listed here; the value is purely
  diagnostic."
- `PqCapabilities` (`src/address.rs:207-248`) is the bounded
  wire-order scheme list with `empty()`, `from_parts()`, `schemes()`,
  `as_wire()`, and `is_supported()`. The `is_supported()` doc comment
  is explicit: "This is purely a diagnostic flag and is **not**
  branched on by any production code path."

The invariants that follow, all verified in source:

- The **session layer stays classical X25519 only.** `crypto.rs`
  contains no KEM reference of any kind; DH is `X25519SharedSecret`
  throughout (`src/crypto.rs:59`, `:366`, `:401`, `:424`, `:453`).
- **Publication stays pq-free.** `publication.rs` carries the test
  `publication_never_emits_pq` (Plan 197), asserting `key != "pq"`
  for every emitted option (`src/publication.rs:558-590`).
- **ML-KEM is not implemented, not claimed, and not silently
  enabled.** `specs/support.toml:155` records
  `ssu2_pq_v3_v4 = "deferred-compatibility-watch"`.

So: parsing a `pq` option proves nothing about key exchange, and no
SSU2 session in this router is post-quantum. A reader must not come
away from this crate believing PQ session establishment exists.

## Module layout

Declared in `src/lib.rs:27-41` — 15 modules, no subdirectories.
Integration trajectories live in `tests/handshake.rs`,
`tests/data_phase.rs`, `tests/path_validation.rs`, and
`tests/peer_relay.rs`.

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- | --- |
| — | `src/lib.rs` | 111 | Crate root: `#![forbid(unsafe_code)]`, module declarations, re-exports | (re-exports only; no own types) |
| `address` | `src/address.rs` | 1868 | Strict `RouterAddress` parsing, introducers, listen/dial types, I2P-base64 decode, `pq` tolerance | `Ssu2RouterAddress`, `Ssu2AddressMaterial`, `Ssu2Endpoint`, `Ssu2Capabilities`, `Ssu2Introducer`, `Ssu2AddressClass`, `Ssu2TransportStyle`, `Ssu2PqKem`, `PqCapabilities`, `ConfiguredListenAddress`, `ResolvedDialTarget`, `Ssu2AddressError`, `MAX_SSU2_PQ_SCHEMES` |
| `block` | `src/block.rs` | 2526 | Bounded authenticated-plaintext block codec (largest module) | `Block`, `DecodedBlock`, `ParsedBlocks`, `TimestampBlock`, `OptionsBlock`, `RouterInfoBlock`, `I2npMessageBlock`, `FirstFragmentBlock`, `FollowOnFragmentBlock`, `TerminationBlock`, `RelayRequestBlock`, `RelayResponseBlock`, `RelayIntroBlock`, `PeerTestBlock`, `AckBlock`, `AddressBlock`, `RelayTagBlock`, `NewTokenBlock`, `PathChallengeBlock`, `PathResponseBlock`, `FirstPacketNumberBlock`, `CongestionBlock`, `PaddingBlock`, `RelayResponseCode`, `TerminationReason`, `BlockError`, `encode_blocks`, `parse_blocks` |
| `constants` | `src/constants.rs` | 365 | SSU2-dossier-derived constants with source comments | `SSU2_VERSION`, `SSU2_NETWORK_ID`, `SSU2_DEFERRED_VERSIONS`, `NOISE_PROTOCOL_NAME`, all `MESSAGE_*`/`BLOCK_*` IDs, all `MAX_*`/`DATA_*` bounds, resend schedules |
| `crypto` | `src/crypto.rs` | 1365 | Noise XK transcript, header protection, token AEAD, two-step data keys | `Ssu2Transcript`, `Ssu2PublicKey`, `IntroKey`, `Role`, `Ssu2SplitKeys`, `DataDirectionKeys`, `DataCipher`, `TranscriptHash`, `Ssu2CryptoError`, `apply_header_protection`, `remove_header_protection`, `seal_token_payload`, `open_token_payload`, `session_created_header_key`, `session_confirmed_header_key`, `derive_data_keys`, `protocol_initial_hash` |
| `handshake` | `src/handshake.rs` | 1389 | Establishment codecs, cheap prevalidation, reassembly, RouterInfo binding | `TokenRequest`, `RetryMessage`, `SessionRequestParts`, `SessionCreatedParts`, `ConfirmedReassembly`, `AuthenticatedPeer`, `ClockSkewPolicy`, `HandshakeReplayCache`, `ReplayToken`, `ReplayDecision`, `RouterInfoFreshness`, `HandshakeError`, `prevalidate_long_datagram`, `validate_router_info`, `split_confirmed_jumbo` |
| `header` | `src/header.rs` | 634 | Long/short header encode/decode, message-type vocabulary | `MessageType`, `HeaderForm`, `LongHeader`, `SessionConfirmedHeader`, `DataHeader`, `HeaderError` |
| `introducer` | `src/introducer.rs` | 384 | Validated introducer records | `IntroducerTable`, `IntroducerRecord`, `IntroducerProvenance`, `IntroducerError`, `MAX_INTRODUCER_RECORDS`, `MAX_PUBLISHED_INTRODUCERS`, `INTRODUCER_RECORD_LIFETIME_SECS` |
| `packet` | `src/packet.rs` | 256 | Datagram length classes, header/payload split | `DatagramLengthClass`, `PacketHeader`, `SplitPacket`, `PacketError`, `split_packet` |
| `path` | `src/path.rs` | 713 | Authenticated path validation + per-path MTU | `PathValidator`, `ValidatedPath`, `PathEvent`, `PathError`, `PathCounters`, `MAX_PATH_CANDIDATES`, `MAX_CANDIDATES_PER_FAMILY`, `MAX_PATH_CHALLENGES_PER_SESSION`, `PATH_CANDIDATE_MTU`, `PATH_CHALLENGE_LENGTH`, `PATH_VALIDATION_TIMEOUT_MS` |
| `peer_test` | `src/peer_test.rs` | 1842 | PeerTest roles/correlation/typed outcomes | `PeerTestTable`, `PeerTestOutcome`, `PeerTestRole`, `PeerTestState`, `PeerTestError`, `PeerTestCounters`, `peer_test_preimage`, `verify_peer_test_signature`, `peer_test_conn_ids`, `build_out_of_session_peer_test`, `parse_out_of_session_peer_test`, `check_peer_test_freshness`, `MAX_PEER_TESTS_GLOBAL`, `MAX_PEER_TESTS_PER_PEER`, `PEER_TEST_TIMEOUT_MS`, `PEER_TEST_MAX_CLOCK_SKEW_SECONDS`, `PEER_TEST_SIGNATURE_PROLOGUE` |
| `publication` | `src/publication.rs` | 593 | Deterministic publication snapshots | `PublicationRequest`, `PublicationPolicy`, `PublicationOutcome`, `Ssu2PublicationSnapshot`, `WithholdReason`, `PublicationError`, `build_publication_snapshot`, `parse_snapshot` |
| `relay` | `src/relay.rs` | 1436 | Relay requester/introducer/target + HolePunch | `RelayRequester`, `RelayIntroducer`, `RelayTarget`, `RelayCounters`, `RelayError`, `HolePunchMessage`, `hole_punch_conn_ids`, `build_hole_punch`, `parse_hole_punch`, `relay_request_preimage`, `relay_response_preimage`, `verify_relay_request`, `verify_relay_response`, `check_relay_freshness`, `relay_response_budget`, `MAX_RELAY_REQUESTS_GLOBAL`, `MAX_RELAY_REQUESTS_PER_PEER`, `MAX_RELAY_TAGS_GLOBAL`, `MAX_RELAY_TAGS_PER_PEER`, `RELAY_TAG_LIFETIME_SECS`, `RELAY_REQUEST_TIMEOUT_MS`, `RELAY_MAX_CLOCK_SKEW_SECONDS`, `RELAY_REQUEST_PROLOGUE`, `RELAY_RESPONSE_PROLOGUE`, `MAX_HOLE_PUNCH_PAYLOAD_BYTES` |
| `session` | `src/session.rs` | 2949 | Authenticated data-phase session (largest module) | `Ssu2Session`, `SessionConfig`, `SessionCounters`, `SessionEvent`, `SessionAction`, `SessionError`, `ReceiveOutcome`, `DropReason` |
| `state_machine` | `src/state_machine.rs` | 1572 | Consuming initiator/responder machines | `Initiator`, `Responder`, `InitiatorConfig`, `InitiatorSecrets`, `ResponderConfig`, `ResponderParams`, `RetryAnswer`, `ConfirmedParams`, `HandshakeAction`, `DatagramBytes`, `DeadlineKind`, `TerminateReason`, `DropCategory`, `AuthenticatedSsu2Session`, `PeerNewToken`, `StateMachineError` |
| `token` | `src/token.rs` | 435 | Bounded one-use token table | `TokenStore`, `Ssu2Token`, `TokenError`, `retry_response_budget` |

`src/` totals **18,540 lines**; `tests/` totals 4,284.

## Public surface

`pub trait` count: **zero.** Like `i2pr-transport`, every contract
is a concrete struct/enum. The crate-root re-exports
(`src/lib.rs:43-111`) are the complete public surface:

```rust
pub use address::{
    ConfiguredListenAddress, MAX_SSU2_PQ_SCHEMES, PqCapabilities, ResolvedDialTarget,
    Ssu2AddressClass, Ssu2AddressError, Ssu2AddressMaterial, Ssu2Capabilities, Ssu2Endpoint,
    Ssu2Introducer, Ssu2PqKem, Ssu2RouterAddress, Ssu2TransportStyle,
};
pub use block::{
    AckBlock, AddressBlock, Block, BlockError, CongestionBlock, DecodedBlock, FirstFragmentBlock,
    FirstPacketNumberBlock, FollowOnFragmentBlock, I2npMessageBlock, NewTokenBlock, OptionsBlock,
    PaddingBlock, ParsedBlocks, PathChallengeBlock, PathResponseBlock, PeerTestBlock,
    RelayIntroBlock, RelayRequestBlock, RelayResponseBlock, RelayResponseCode, RelayTagBlock,
    RouterInfoBlock, TerminationBlock, TerminationReason, TimestampBlock,
};
pub use crypto::{
    DataCipher, DataDirectionKeys, IntroKey, Role, Ssu2CryptoError, Ssu2PublicKey, Ssu2SplitKeys,
    Ssu2Transcript, TranscriptHash, derive_data_keys, open_token_payload, protocol_initial_hash,
    session_confirmed_header_key, session_created_header_key,
};
pub use handshake::{
    AuthenticatedPeer, ClockSkewPolicy, ConfirmedReassembly, HandshakeError, HandshakeReplayCache,
    ReplayDecision, ReplayToken, RetryMessage, RouterInfoFreshness, SessionCreatedParts,
    SessionRequestParts, TokenRequest, build_confirmed_payload, build_retry,
    build_session_confirmed, build_session_created, build_session_request, build_token_request,
    find_establishment_token, parse_retry, parse_session_created, parse_session_request,
    parse_token_request, prevalidate_long_datagram, require_first_router_info, require_timestamp,
    session_confirmed_first_header, split_confirmed_jumbo, validate_router_info,
};
pub use header::{
    DataHeader, HeaderError, HeaderForm, LongHeader, MessageType, SessionConfirmedHeader,
};
pub use introducer::{
    INTRODUCER_RECORD_LIFETIME_SECS, IntroducerError, IntroducerProvenance, IntroducerRecord,
    IntroducerTable, MAX_INTRODUCER_RECORDS, MAX_PUBLISHED_INTRODUCERS,
};
pub use packet::{DatagramLengthClass, PacketError};
pub use path::{
    MAX_CANDIDATES_PER_FAMILY, MAX_PATH_CANDIDATES, MAX_PATH_CHALLENGES_PER_SESSION,
    PATH_CANDIDATE_MTU, PATH_CHALLENGE_LENGTH, PATH_VALIDATION_TIMEOUT_MS, PathCounters, PathError,
    PathEvent, PathValidator, ValidatedPath,
};
pub use peer_test::{
    MAX_PEER_TESTS_GLOBAL, MAX_PEER_TESTS_PER_PEER, PEER_TEST_MAX_CLOCK_SKEW_SECONDS,
    PEER_TEST_SIGNATURE_PROLOGUE, PEER_TEST_TIMEOUT_MS, PeerTestCounters, PeerTestError,
    PeerTestOutcome, PeerTestRole, PeerTestState, PeerTestTable, build_out_of_session_peer_test,
    check_peer_test_freshness, parse_out_of_session_peer_test, peer_test_conn_ids,
    peer_test_preimage, verify_peer_test_signature,
};
pub use publication::{
    PublicationError, PublicationOutcome, PublicationPolicy, PublicationRequest,
    Ssu2PublicationSnapshot, WithholdReason, build_publication_snapshot, parse_snapshot,
};
pub use relay::{
    HolePunchMessage, MAX_HOLE_PUNCH_PAYLOAD_BYTES, MAX_RELAY_REQUESTS_GLOBAL,
    MAX_RELAY_REQUESTS_PER_PEER, MAX_RELAY_TAGS_GLOBAL, MAX_RELAY_TAGS_PER_PEER,
    RELAY_MAX_CLOCK_SKEW_SECONDS, RELAY_REQUEST_PROLOGUE, RELAY_REQUEST_TIMEOUT_MS,
    RELAY_RESPONSE_PROLOGUE, RELAY_TAG_LIFETIME_SECS, RelayCounters, RelayError, RelayIntroducer,
    RelayRequester, RelayTarget, build_hole_punch, check_relay_freshness, hole_punch_conn_ids,
    parse_hole_punch, relay_request_preimage, relay_response_budget, relay_response_preimage,
    verify_relay_request, verify_relay_response,
};
pub use session::{
    DropReason, ReceiveOutcome, SessionAction, SessionConfig, SessionCounters, SessionError,
    SessionEvent, Ssu2Session,
};
pub use state_machine::{
    AuthenticatedSsu2Session, ConfirmedParams, DatagramBytes, DeadlineKind, DropCategory,
    HandshakeAction, Initiator, InitiatorConfig, InitiatorSecrets, PeerNewToken, Responder,
    ResponderConfig, ResponderParams, RetryAnswer, StateMachineError, TerminateReason,
};
pub use token::{Ssu2Token, TokenError, TokenStore, retry_response_budget};
```

`packet` deliberately re-exports only `DatagramLengthClass` and
`PacketError` at the root; `split_packet`, `PacketHeader`, and
`SplitPacket<'a>` remain reachable as
`i2pr_transport_ssu2::packet::*`. `block::encode_blocks` /
`block::parse_blocks` and `crypto::apply_header_protection` /
`remove_header_protection` are likewise module-public, not
root-re-exported.

`session` exposes no free functions — its whole API is inherent
methods on `Ssu2Session`: `new`, `request_bootstrap_ack`,
`matches_inbound`, `matches_data_header`, `outbound_pending`,
`note_path_migrated`, `queue_i2np_message`, `queue_path_challenge`,
`queue_path_response`, `queue_new_token`, `initiate_termination`,
`queue_relay_request`, `queue_relay_response`, `queue_relay_intro`,
`queue_peer_test`, `queue_address`, `poll_transmit`,
`receive_datagram`, `poll`, `next_deadline_ms`
(`src/session.rs:586-2488`).

## Key contracts

### RouterAddress (`address.rs`)

- `Ssu2RouterAddress::parse` — strict validation of a structural
  `RouterAddress`: style must be `SSU2`; `v=2` required
  (anything else is `UnsupportedVersion`, including PQ-hybrid
  v3/v4, which `SSU2_DEFERRED_VERSIONS = [3, 4]` classifies as
  deferred rather than malformed); `s` is a 32-byte I2P-base64
  static key (all-zero rejected); `i` is a 32-byte intro key
  (required with an endpoint or introducers, forbidden for the
  unpublished static-only form); `host` must be a numeric IP
  (hostnames refused); `port` is canonical decimal `1..=65535`;
  `host`/`port` must appear together; `mtu` is optional and
  validated `1280..=9000` (`SSU2_MIN_MTU`/`SSU2_MAX_MTU`); `caps` is
  a bounded graphic string (≤ `MAX_SSU2_CAPS_BYTES` = 16) with
  duplicate-known-flag rejection (`4`/`6` families, `B` peer-test,
  `C` relay; other letters ignored for forward compatibility); up to
  3 introducer groups (`ihostN`/`iportN`/`ikeyN`/`itagN`, dense from
  0, nonzero tags); unknown options rejected; `pq` tolerated as
  described above.
- `address_class()` — `Direct` / `DirectWithIntroducers` /
  `IntroducerOnly` / `UnpublishedStatic`. Describes present
  contact material only; never implies reachability or publication
  approval.
- `ConfiguredListenAddress` / `ResolvedDialTarget` — distinct
  endpoint + material types with no socket ownership; dial targets
  require an exact literal endpoint match.
- `Debug` redacts endpoints and key material everywhere.
- Supporting enablers for publication: `pub(crate) encode_i2p_base64`
  (promoted from test-only) and `Ssu2Capabilities::parse` for strict
  caps construction.

### Headers (`header.rs`)

- `MessageType` — the 8 assigned types (0/1/2/6/7/9/10/11, i.e.
  SessionRequest, SessionCreated, SessionConfirmed, Data, PeerTest,
  Retry, TokenRequest, HolePunch); unassigned bytes are
  `UnknownMessageType`.
- `HeaderForm::classify_prefix` — long vs short from the
  spec-defined type byte at offset 12, not heuristics.
- `LongHeader` — exact 32 bytes (`LONG_HEADER_LENGTH`); version must
  be 2 (`UnsupportedVersion`), network ID must be 2
  (`InvalidNetworkId`), reserved flags must be 0
  (`InvalidFlags`), and source/destination connection IDs must
  differ (`IdenticalConnectionIds`).
- `SessionConfirmedHeader` — exact 16 bytes; packet number must be
  0 (`InvalidPacketNumber`); frag byte is number `0..=14` over total
  `1..=15` with number < total (`InvalidFragmentInfo`).
- `DataHeader` — exact 16 bytes; reserved flag bits and
  `moreflags` must be 0; bit 0 is the immediate-ACK request.
- All decoders reject short inputs (`Truncated`) and trailing bytes
  (`TrailingBytes`) — exact-consumption discipline.

### Datagrams (`packet.rs`)

- `DatagramLengthClass::classify` — `MIN_DATAGRAM_LENGTH` = 40
  minimum, `MAX_DATAGRAM_IPV6_LENGTH` = 1452, `MAX_DATAGRAM_IPV4_LENGTH`
  = 1472, without touching packet bytes (`TooShort`,
  `ExceedsIpv6Maximum`, `ExceedsIpv4Maximum`).
- `split_packet` — validates length, classifies the form, decodes
  the exact header (mapping `HeaderError` into `PacketError::Header`),
  then requires the minimum authenticated tail
  (`MIN_POST_HEADER_BYTES` = 24; 56 for SessionRequest/SessionCreated,
  which carry the 32-byte `HANDSHAKE_EPHEMERAL_LENGTH`) before
  exposing the opaque post-header bytes. No crypto.

### Blocks (`block.rs`)

- **21 outbound `Block` variants** covering the spec table 0–10,
  12–13, 15–21, and 254. Type 11 (NextNonce, spec TODO) decodes as
  `UnsupportedBlock`; reserved 14/255 and experimental 224–253 skip
  under the unknown budget.
- `encode_blocks` / `parse_blocks` enforce: at most
  `MAX_BLOCK_COUNT` = 64 blocks, `MAX_UNKNOWN_BLOCK_BYTES` = 1024
  aggregate unknown bytes, Padding at most once and last, Termination
  at most once and last-non-padding (`InvalidOrder`,
  `DuplicateBlock`, `InvalidTermination`). All other known blocks may
  repeat. SessionConfirmed RouterInfo-first is enforced by the
  handshake payload check (`handshake.rs`), not here.
- Per-block strictness: DateTime exactly 4; Options 12+
  (fixed-point ratios, no ordering assumption); RouterInfo flags
  limited to bits 0–1 with frag byte exactly `0x01` (never
  fragmented) and a `MAX_ROUTER_INFO_BLOCK_BYTES` = 4096 ceiling;
  I2NP/first-fragment require the 9-byte header with nonempty bodies;
  follow-on fragments require number `1..=127` with nonempty bodies;
  ACK ranges reject (0,0) pairs with a `MAX_ACK_RANGES` = 128-range
  cap; Address is 6 or 18 bytes, port first; RelayTagRequest empty;
  RelayTag nonzero; NewToken exactly 12; FirstPacketNumber exactly 4;
  Congestion `1..=64` bytes; path data capped at
  `MAX_PATH_DATA_BYTES` = 1024; relay/peer-test signatures retained as
  bounded (`MAX_SIGNATURE_BYTES` = 1024) opaque evidence with strict
  fixed-prefix parsing (verification lives in `peer_test.rs` /
  `relay.rs`).

### Noise transcript (`crypto.rs`)

- `Ssu2Transcript` — consuming initiator/responder transcript for
  the SSU2-specific Noise XK pattern
  (`Noise_XKchaobfse+hs1+hs2+hs3_25519_ChaChaPoly_SHA256`, 52 bytes,
  pinned by `NOISE_PROTOCOL_NAME_LENGTH`):
  initial `SHA256(protocol_name)` chaining with null-prologue and
  responder-static mixes; `e,es` / `e,ee` / `s,se` stages with
  role-gated transitions (`WrongRole`/`InvalidState` otherwise); the
  request ciphertext is mixed exactly once (when the SessionRequest is
  sealed/accepted, never re-mixed at SessionCreated); the
  first-fragment SessionConfirmed short header is mixed before the
  static-key frame; the post-`ee` cipher is retained for that frame
  (`n = 1`).
- `protocol_initial_hash()` — the initial `h` value, exposed for
  vector reproduction.
- **The two-step DATA KDF (corrected).** `split()`
  (`src/crypto.rs:620-640`) requires the `Confirmed` stage, then:
  1. `HKDF(chaining_key, ZEROLEN, "", 64)` → `(k_ab, k_ba)`, assigned
     by role (`Initiator` → `(k_ab, k_ba)`, `Responder` →
     `(k_ba, k_ab)`).
  2. **Per direction**, `derive_data_keys(directional_key)` runs
     `HKDF(key, ZEROLEN, "HKDFSSU2DataKeys", 64)`
     (`src/crypto.rs:830-835`) and splits the 64 bytes into
     `(k_data, k_header_2)`.

  The AEAD cipher uses `k_data`; header protection uses `k_header_2`,
  with `k_header_1` equal to the **receiver's intro key**, supplied by
  the session layer. Intermediate Noise secrets are released at
  `split()`; only the directional `DataDirectionKeys` survive. The
  Created-stage and Confirmed-stage corrections were verified against
  the pinned i2pd 2.61.0 implementation during Plan 161 direction-A
  work.
- `apply_header_protection` / `remove_header_protection` — the
  Header Encryption KDF: two ChaCha20 keystreams over the first 8 and
  next 8 header bytes respectively, keyed by `k_header_1` and
  `k_header_2` with the packet's trailing 24 bytes read as two 12-byte
  IVs, plus a third stream under the all-zero nonce covering the
  48-byte (Request/Created: header tail + ephemeral key) or 16-byte
  (Retry/TokenRequest/PeerTest/HolePunch) region. Short headers have
  no third part. Two recorded interpretation notes (the `n: 1`
  annotation, inclusive index notation) live in the module docs, and
  the spec pseudocode governs where it and the raw-contents note
  disagree.
- `seal_token_payload` / `open_token_payload` — Retry/TokenRequest
  AEAD under the intro key with the header packet number as nonce and
  the cleartext header as associated data.
- `session_created_header_key` / `session_confirmed_header_key` —
  the `SessCreateHeader` and `SessionConfirmed` labeled derivations.
- Secrets (`ChainKey`, cipher keys) are zeroizing owners without
  `Debug`/`Clone`; DH inputs arrive as checked `X25519SharedSecret`
  values so private keys never enter the transcript; nonces refuse the
  forbidden `2^64 - 1` (`NonceExhausted`).

### Establishment codecs (`handshake.rs`)

- `build/parse_token_request`, `build/parse_retry` — intro-key
  AEAD payloads (DateTime required; Address required in Retry;
  Termination optional in Retry, which then carries a zero token);
  Retry enforces the 3x amplification budget
  (`RETRY_AMPLIFICATION_NUMERATOR`/`DENOMINATOR` = 3/1) and the
  `MAX_RETRY_PADDING_BYTES` = 64 cap.
- `prevalidate_long_datagram` — symmetric-only cheap gate (length
  class, deprotection, exact header decode, version/network/type,
  minimum tail) before any DH, payload AEAD, or session allocation.
  The handshake flood test drives 200 such datagrams.
- `build/parse_session_request`, `build/parse_session_created` —
  header plus ephemeral plus transcript-ciphertext assembly with the
  phase-correct protection keys.
- `build_session_confirmed` / `ConfirmedReassembly` /
  `split_confirmed_jumbo` — bounded fragmentation
  (`MAX_SESSION_CONFIRMED_FRAGMENTS` = 15, 32 KiB aggregate via
  `MAX_CONFIRMED_REASSEMBLED_BYTES`) and exact reassembly with
  duplicate/conflict detection; `frag0`'s header is the Noise
  associated data.
- `validate_router_info` — **the anti-spoofing core.** Deep
  establishment binding without touching NetDB: structural decode,
  signature verification, expected-hash check, `v=2` SSU2 address
  presence, static-`s` binding against the handshake peer
  (constant-time), intro-`i` shape where required, and
  caller-supplied publication freshness. Returns `AuthenticatedPeer`
  (hash, static key, validated bytes).
- `require_first_router_info`, `require_timestamp`,
  `find_establishment_token`, `session_confirmed_first_header`,
  `build_confirmed_payload` — the ordering/gating helpers the machines
  share.
- `ClockSkewPolicy::handshake` (±`HANDSHAKE_MAX_CLOCK_SKEW_SECONDS`
  = 120 s) and `HandshakeReplayCache` (bounded at
  `MAX_HANDSHAKE_REPLAY_ENTRIES` = 512, caller-supplied time,
  retention `2*D` = 240 s) cover timestamp and ephemeral replay
  handling. `ReplayDecision` is the typed verdict.

### Token lifecycle (`token.rs`)

The one-use table is `TokenStore`. The three named session
entry-points (`queue_new_token`, `matches_inbound`,
`outbound_pending`) are the **runtime-support surface on
`Ssu2Session`**, not the table itself; the table's own API is
`issue` / `consume`.

- `TokenStore::issue(source, now, token_bytes, responder_conn_id)`
  — releases expired entries first, then deterministically evicts the
  oldest entry in scope when a per-source or global quota is full, and
  binds the token to the exact source socket address. The optional
  `responder_conn_id` records the responder connection ID the token
  was announced with (Retry source ID) so a later SessionCreated
  reuses it rather than minting a second ID the initiator never saw.
- `TokenStore::consume(token, source, now) -> Result<Option<u64>>` —
  **single use is enforced by removal**: the entry is located by exact
  value, checked for expiry and exact source, then
  `self.entries.remove(index)`. A second presentation therefore finds
  no entry and fails closed as `UnknownToken`. (`TokenError::ReusedToken`
  exists in the enum and in the "recoverable" classification, but the
  removal path means reuse is reported as `UnknownToken`.)
- Expiry: `now.saturating_sub(entry.issued_at) > lifetime_seconds`
  removes the entry and returns `ExpiredToken`, releasing the
  accounting. Default lifetime `TOKEN_LIFETIME_SECONDS` = 30 s.
  `expire(now)` releases expired entries without a presentation.
- Quotas: `MAX_TOKENS_GLOBAL` = 256, `MAX_TOKENS_PER_SOURCE` = 4.
- `rotate()` models key/generator restart by advancing the epoch so
  every prior token stops matching.
- `source_is_ipv6(source)` documents the v4/v6 non-transfer rule; the
  separation itself falls out of the exact `SocketAddr` comparison
  (IP *and* port).
- Randomness and time are caller-supplied. `Ssu2Token` rejects zero
  (`ZeroToken`) and redacts `Debug`.
- `retry_response_budget(request_length)` — saturating 3x Retry
  anti-amplification bound.

### State machines (`state_machine.rs`)

State sets are **private** enums (`src/state_machine.rs:366-370`,
`:870-874`); only the transitions are public.

- `InitiatorState` — `AwaitRetryOrCreated`, `Established`.
- `ResponderState` — `AwaitRequest`, `AwaitConfirmed`.
- `Initiator` — `begin` (TokenRequest without a token,
  SessionRequest with one), `on_retry` (fresh ephemeral,
  token-bearing request), `on_session_created` (Noise completion,
  SessionConfirmed emission, `Established`), `on_unexpected` (returns
  self plus a silent-drop action list), `on_timeout` (identical-byte
  resend per the spec schedules, then
  `RetriesExhausted`/`HandshakeTimeout`), `cancel`.
- `Responder` — `new`, `classify` (cheap `MessageType` probe for
  receive routing), `on_token_request` (Retry, no DH, no state),
  `on_session_request` (Retry for tokenless; token → replay → skew
  gates, then the single admitted DH and SessionCreated; duplicates
  resend the identical Created), `on_session_confirmed` (bounded
  reassembly, static/payload open, RouterInfo binding,
  `Established`), `on_timeout`, `cancel`.
- Actions are `WriteDatagram(DatagramBytes)` /
  `ArmDeadline { kind, at_ms }` / `Established(AuthenticatedSsu2Session)`
  / `Terminate(TerminateReason)` / `DropSilently(DropCategory)`. The
  crate never sleeps, opens sockets, or reads clocks.
- `DeadlineKind` — `TokenRequest`, `SessionRequest`, `SessionCreated`,
  `SessionConfirmed`, `Handshake`. Resend schedules are
  `TOKEN_REQUEST_RESEND_DELAYS_MS` = [3000, 6000],
  `SESSION_REQUEST_RESEND_DELAYS_MS` = [1250, 2500, 5000],
  `SESSION_CREATED_RESEND_DELAYS_MS` = [1000, 2000, 4000],
  `SESSION_CONFIRMED_RESEND_DELAYS_MS` = [1250, 2500, 5000], with
  `MAX_HANDSHAKE_ATTEMPTS` = 4 and `HANDSHAKE_DEADLINE_MS` = 20,000.
- `TerminateReason` — `HandshakeTimeout`, `RetriesExhausted`,
  `Cancelled`, `AuthenticationFailed`, `RouterInfoRejected`,
  `TokenRejected`, `ReplayDetected`, `PeerTerminated`,
  `ProtocolViolation`. `DropCategory` — `Malformed`,
  `VersionNetworkType`, `ConnectionIdMismatch`, `BadToken`, `Replay`,
  `ClockSkew`, `PeerTerminated`, `Unexpected`. Both are local
  diagnostics, never reflected to an unauthenticated source.
- `AuthenticatedSsu2Session` carries only what the data phase needs:
  peer material, directional keys, both connection IDs, observed
  endpoint, local MTU, and an optional in-band `PeerNewToken`.

### Data-phase session (`session.rs`)

- `Ssu2Session::new(config, keys)` — consumes the establishment
  splits plus explicit intro keys; owns send packet numbers (no wrap;
  `MAX_PACKET_NUMBER` = `u64::MAX - 1`, `PacketNumberExhausted` at
  exhaustion), the `DATA_REPLAY_WINDOW_PACKETS` = 128 replay bitmap
  with a `DATA_MAX_FUTURE_JUMP` = 1024 forward-jump cap, pending ACK
  state with one deadline, RTT/RTO/cwnd state, sent provenance
  (`DATA_MAX_SENT_PACKETS` = 256 entries), pending retransmit
  fragments (`DATA_MAX_PENDING_RETRANSMIT_FRAGMENTS` = 256),
  outbound messages, reassembly (`DATA_MAX_REASSEMBLY_MESSAGES` = 16 /
  `DATA_MAX_REASSEMBLY_BYTES` = 256 KiB), the delivered-ID duplicate
  cache (`DATA_DUP_CACHE_ENTRIES` = 128), and idle/termination state.
  The full v2 packet number rides the short header, so reconstruction
  is the documented identity plus window policy.
- **Strict bounded ACK scheduling.** Delayed ACK defaults to
  `DATA_DEFAULT_ACK_DELAY_MS` = 25 ms with immediate ACK at
  `DATA_DEFAULT_IMMEDIATE_ACK_DELAY_MS` = 5 ms before RTT samples
  exist; both are then bounded by the spec's `RTT/6` and `RTT/16`
  guidance. Scheduling is per-session and loop-free; ACK-only packets
  always pass the congestion gate, ack-eliciting packets do not.
  `AckUnderflow` is raised without mutating state.
- **Fresh retransmission with RTT/RTO/congestion control.** RFC
  6298-style: `DATA_INITIAL_RTO_MS`/`DATA_MIN_RTO_MS` = 1000,
  `DATA_MAX_RTO_MS` = 60,000, with
  `DATA_MAX_FRAGMENT_RETRANSMISSIONS` = 5 before a message fails
  silently (counters only) while the session stays usable. The
  congestion controller is byte-count based:
  `DATA_MSS_BYTES` = 1220, `DATA_MIN_CWND_BYTES` = 2×MSS,
  `DATA_DEFAULT_CWND_BYTES` = 10×MSS, `DATA_MAX_CWND_BYTES` = 60,000.
  Overheads are `DATA_IPV4_OVERHEAD_BYTES` = 60 and
  `DATA_IPV6_OVERHEAD_BYTES` = 80 (`MTU - 60` / `MTU - 80`).
- **Exact fragmentation/reassembly with duplicate suppression.**
  `queue_i2np_message` splits encoded messages into fixed 1024-byte
  semantic fragments (single-fragment fast path as complete blocks,
  `MAX_I2NP_FRAGMENTS` = 64 ceiling; the budget is
  `max_payload_bytes - 64` clamped to 64..=1024). Reassembly is
  exact-capacity tested with `ReassemblyConflict` on divergent
  duplicates, and `DATA_DUP_RETENTION_SECONDS` = 600 bounds
  duplicate suppression at the delivery boundary.
- `poll_transmit(now)` seals at most one MTU-aware datagram (ACK
  first, then controls, retransmits, new fragments) with one AEAD seal
  plus header protection.
- `receive_datagram(now_ms, now_secs, bytes)` — ordered pipeline
  returning `ReceiveOutcome` with typed `SessionEvent`s **only after
  authentication**; replays/tag failures mutate counters alone.
- `poll(now_ms, now_secs)` / `next_deadline_ms(now_ms)` — drives ACK
  deadlines, RTO backoff with bounded `Timeout` termination after
  `DATA_MAX_CONSECUTIVE_RTO_BEFORE_TERMINATE` = 5 consecutive
  expiries, the `DATA_DEFAULT_IDLE_TIMEOUT_MS` = 300,000 idle timeout,
  and reassembly expiry.
- Rekey: `NextNonce` block type 11 is the spec's rotation hook;
  `session.rs` carries the boundary test for it even though the block
  codec itself refuses type 11 as `UnsupportedBlock` on decode.
- `note_path_migrated()` — declares every unacked sent packet lost
  through the bounded requeue policy (fresh generation, per-fragment
  ceiling, silent per-message failure past it), then resets
  bytes-in-flight to zero, the window to its minimum, and the
  consecutive-RTO state. Called by the runtime after the validator
  promotes; without the requeue, in-flight messages would strand.
- Outbound controls are single-shot; per-message delivery failure
  past the retransmission ceiling is silent by design.
- `SessionCounters` — privacy-safe counts only (packets, ACKs, losses,
  retransmits, flight, cwnd, reassembly, termination). No hashes,
  nonces, or endpoints.

### Path validation (`path.rs`)

- `PathValidator::new(endpoint, mtu)` — one validated path, no
  candidates; MTU range-checked `1280..=9000`.
- `note_authenticated_packet(endpoint, challenge, now_ms)` — the
  caller guarantees the session already authenticated and
  replay-filtered the datagram. Validated source: no effect. Known
  candidate: no effect (one challenge each). New endpoint: bounded
  admission (`MAX_PATH_CANDIDATES` = 4 global,
  `MAX_CANDIDATES_PER_FAMILY` = 2,
  `MAX_PATH_CHALLENGES_PER_SESSION` = 8, nonzero
  `PATH_CHALLENGE_LENGTH` = 32-byte challenge) then exactly one
  `PathEvent::ChallengeToSend`. **Never migrates** on its own.
- `on_path_response(endpoint, data, now_ms)` — promotes only on an
  exact challenge match from the tracked candidate endpoint
  (`PathEvent::Validated { previous, current }`); wrong/stale values
  are rejected and counted while the candidate survives to its
  `PATH_VALIDATION_TIMEOUT_MS` = 10,000 deadline; expired/unknown
  endpoints fail typed without migration. The validated MTU is path
  policy and survives migration.
- `poll_expired(now_ms)` / `next_deadline_ms()` — deadline expiry
  (retains the old path) and the central-scheduler input.
- `effective_mtu()` / `validated_payload_bytes()` /
  `candidate_payload_bytes()` — fragmentation budgets; candidates are
  pinned to the `PATH_CANDIDATE_MTU` 1280 minimum. There is no
  packet-driven MTU setter: `ValidatedPath::with_mtu` is the only
  writer, reserved for configured/validated sources.

### Publication snapshots (`publication.rs`)

- `build_publication_snapshot(PublicationRequest)` — deterministic
  `Direct` / `Firewalled` / `Withheld` decision from public keys only
  (zero keys rejected; private material is never an input).
- Direct host/port requires `Reachable` *plus* explicit
  `allow_direct`; anything weaker yields the unpublished static-only
  firewalled form (no fabricated address). Non-empty introducers
  without `allow_introducers` fail closed
  (`IntroducersUnvalidated`). `TooManyIntroducers` bounds the set.
- Output options are canonical (sorted); `Ssu2PublicationSnapshot`
  carries `evidence_expires_secs`, and `is_expired(now)` marks
  withdrawal. `parse_snapshot` round-trips through the strict
  production parser.
- **Publication stays pq-free**, asserted by the in-crate
  `publication_never_emits_pq` test.
- `introducer::IntroducerTable::validated_introducers` feeds this
  builder (live records only, capped at `MAX_PUBLISHED_INTRODUCERS`
  = 3).
- No capability/version/RouterInfo advertisement beyond this tested
  subset; the router does not advertise SSU2 publicly.

### PeerTest roles (`peer_test.rs`)

- `PeerTestTable` — bounded Alice/Bob/Charlie machine
  (`MAX_PEER_TESTS_GLOBAL` = 8, `MAX_PEER_TESTS_PER_PEER` = 2,
  nonzero nonces, `PEER_TEST_TIMEOUT_MS` = 10,000 central-expiry
  deadlines, one scheduler input, no task per test). Correlation is by
  nonce; concurrent tests cannot consume each other's messages.
- Exact spec `PeerTestValidate` preimages
  (`PEER_TEST_SIGNATURE_PROLOGUE` = `b"PeerTestValidate"`,
  `peer_test_preimage`, `verify_peer_test_signature`): Msgs 1,2
  Alice-signed without `ahash`; Msgs 3,4 Charlie-signed with `ahash`;
  Msgs 5–7 optional. Only Ed25519 verifies (`UnsupportedSigner`
  otherwise; multi-algorithm interop is recorded as Plan 161 debt).
- Gates per ingest: correlation → role/state → sender → family →
  freshness (±`PEER_TEST_MAX_CLOCK_SKEW_SECONDS` = 120 s) → signature
  → status code. Unknown/stale/wrong-role messages never create
  reachability evidence.
- Typed `PeerTestOutcome` (`PeerTestRole` = `Alice`/`Bob`/`Charlie`):
  `DirectReachabilityConfirmed` (family, observed endpoint, evidence
  peers), `AddressMismatch`, `FirewalledLikely`, `Inconclusive`,
  `Rejected`. Msg 4 alone never confirms; unsigned Msgs 5–7 never
  confirm alone; contradiction yields mismatch, never last-write-wins.
- Out-of-session type-7 codecs (`peer_test_conn_ids`,
  `build/parse_out_of_session_peer_test`) under the receiver intro key
  with nonce-derived connection IDs.
- Redacted `Debug`/counters (counts only, no hashes/nonces/endpoints).

### Relay roles (`relay.rs`)

- `RelayRequester` (Alice) — bounded concurrent requests
  (`MAX_RELAY_REQUESTS_GLOBAL` = 8, `MAX_RELAY_REQUESTS_PER_PEER` = 2)
  with response/tag/nonce correlation, distinct-tag isolation,
  HolePunch verification against nonce-derived connection IDs, and
  readiness to transition into the normal handshake (never a fake
  session).
- `RelayIntroducer` (Bob) — disabled unless explicitly enabled
  (`ServiceDisabled`); live-tag checks bound to Alice
  (`MAX_RELAY_TAGS_GLOBAL` = 16, `MAX_RELAY_TAGS_PER_PEER` = 4,
  `RELAY_TAG_LIFETIME_SECS` = 120), 3x anti-amplification budget
  enforced before crypto, replay idempotency (no second intro
  emission), deterministic `RELAY_REQUEST_TIMEOUT_MS` = 10,000 expiry,
  and `shutdown` clearing all state.
- `RelayTarget` (Charlie) — introducer-context validation of
  `RelayIntro` (Alice's forwarded signature under the
  `RelayRequestData` preimage), single HolePunch emission per intro
  with bounded replay suppression.
- Exact `RelayRequestData` / `RelayAgreementOK` preimages
  (`RELAY_REQUEST_PROLOGUE` = `b"RelayRequestData"`,
  `RELAY_RESPONSE_PROLOGUE` = `b"RelayAgreementOK"`,
  `relay_request_preimage`, `relay_response_preimage`,
  `verify_relay_request`, `verify_relay_response`); only Ed25519
  verifies. Freshness ±`RELAY_MAX_CLOCK_SKEW_SECONDS` = 120 s.
- HolePunch codec (`hole_punch_conn_ids`, `build/parse_hole_punch`)
  under Alice's intro key: DateTime + Address + RelayResponse
  payload, `Dest = (nonce << 32) | nonce`, payload capped at
  `MAX_HOLE_PUNCH_PAYLOAD_BYTES` = 512.
- Redacted `Debug` everywhere (counts/enablement only).

### Introducer records (`introducer.rs`)

- `IntroducerTable` — the single bounded validated-record owner
  (`MAX_INTRODUCER_RECORDS` = 8: peer hash, endpoint, intro key,
  nonzero tag, expiry `INTRODUCER_RECORD_LIFETIME_SECS` = 600 s,
  `IntroducerProvenance`). Deterministic oldest-expiry replacement,
  failed-peer removal (never publish stale/failed), live selection
  capped at `MAX_PUBLISHED_INTRODUCERS` = 3 with stable ordering,
  expiry withdrawal, and `validated_introducers` conversion for the
  publication builder.
- Redacted `Debug` (counts only).

### Errors

All error types implement `Display + Error + Eq + PartialEq`
(`Clone + Copy` where the payload allows). No protocol-vs-operational
mixing. Full variant sets:

| Error | Module | Variants |
| --- | --- | --- |
| `Ssu2AddressError` | `address.rs` | `UnsupportedTransportStyle`, `UnsupportedVersion { .. }`, `UnknownOption`, `DuplicateOption { .. }`, `MissingOption { .. }`, `ConflictingOptions { .. }`, `InvalidOptionValue { .. }`, `HostnameNotAllowed`, `InvalidPort`, `PortOutOfRange`, `TooManyIntroducers`, `InvalidIntroducer { .. }` |
| `HeaderError` | `header.rs` | `Truncated`, `TrailingBytes`, `UnknownMessageType`, `WrongHeaderForm`, `UnsupportedVersion`, `InvalidNetworkId`, `InvalidFlags`, `IdenticalConnectionIds`, `InvalidPacketNumber`, `InvalidFragmentInfo` |
| `PacketError` | `packet.rs` | `TooShort`, `ExceedsIpv6Maximum`, `ExceedsIpv4Maximum`, `Header`, `PayloadTooShort` |
| `BlockError` | `block.rs` | `Truncated`, `LengthExceedsPayload`, `ExcessiveBlockCount`, `ExcessiveUnknownBytes`, `InvalidLength`, `DuplicateBlock`, `InvalidOrder`, `InvalidTermination`, `RouterInfoMalformed`, `I2npMalformed`, `FragmentMalformed`, `AckMalformed`, `AddressMalformed`, `RelayMalformed`, `PeerTestMalformed`, `OptionsMalformed`, `UnsupportedBlock`, `PayloadTooLarge` |
| `Ssu2CryptoError` | `crypto.rs` | `InvalidPublicKey`, `FieldTooLarge`, `NonceExhausted`, `AuthenticationFailed`, `InvalidState`, `WrongRole`, `PeerStaticMismatch`, `KdfInput`, `WrapperRejected(#[from] i2pr_crypto::CryptoError)`, `Initiator(..)`, `Responder(..)` |
| `HandshakeError` | `handshake.rs` | `Truncated`, `TooLong`, `PayloadTooShort`, `Header`, `Packet`, `Blocks`, `Crypto`, `MissingTimestamp`, `StaleTimestamp`, `FutureTimestamp`, `ReplayDetected`, `ReplayCacheFull`, `TokenRejected`, `AmplificationExceeded`, `BadFragment`, `DuplicateFragment`, `IncompleteFragments`, `AggregateTooLarge`, `RouterInfoMalformed`, `RouterInfoSignatureInvalid`, `PeerIdentityMismatch`, `UnsupportedPeerKey`, `MissingSsu2Address`, `TransportStaticKeyMismatch`, `IntroKeyInvalid`, `RouterInfoStale`, `RouterInfoFuture`, `RouterInfoNotFirst`, `LocalPolicyDenied` |
| `TokenError` | `token.rs` | `ZeroToken`, `UnknownToken`, `ExpiredToken`, `ReusedToken`, `WrongSource`, `TableFull` |
| `StateMachineError` | `state_machine.rs` | `Handshake`, `Crypto`, `Wrapper(#[from] i2pr_crypto::CryptoError)`, `InvalidState` |
| `SessionError` | `session.rs` | `Packet`, `Header`, `Crypto`, `Blocks`, `NotForSession`, `Replay`, `TooOld`, `FutureJump`, `AckUnderflow`, `AckInvalid`, `PacketNumberExhausted`, `SentHistoryFull`, `RetransmitQueueFull`, `OutboundQueueFull`, `MessageTooLarge`, `ReassemblyFull`, `ReassemblyConflict`, `Terminated`, `LocalPolicyDenied` |
| `PathError` | `path.rs` | `TooManyCandidates`, `FamilyQuotaExceeded`, `ChallengeBudgetExhausted`, `InvalidChallenge`, `NotACandidate`, `ExpiredCandidate`, `ChallengeMismatch`, `InvalidMtu` |
| `PublicationError` | `publication.rs` | `InvalidKey`, `InvalidMtu`, `IntroducersUnvalidated`, `TooManyIntroducers` |
| `PeerTestError` | `peer_test.rs` | `TooManyTests`, `PeerQuotaExceeded`, `DuplicateNonce`, `UnknownTest`, `WrongRole`, `SenderMismatch`, `InvalidSignature`, `UnsupportedSigner`, `StaleTimestamp`, `UnsupportedVersion`, `Expired`, `Cancelled` |
| `RelayError` | `relay.rs` | `TooManyRequests`, `PeerQuotaExceeded`, `TooManyTags`, `TagQuotaExceeded`, `DuplicateCorrelation`, `UnknownCorrelation`, `InvalidTag`, `WrongRole`, `SenderMismatch`, `InvalidSignature`, `UnsupportedSigner`, `StaleTimestamp`, `UnsupportedVersion`, `Expired`, `ServiceDisabled`, `InvalidHolePunch`, `Cancelled` |
| `IntroducerError` | `introducer.rs` | `ZeroTag`, `TableFull`, `Unknown`, `Expired` |

### Bound constants at a glance

| Constant | Value | Module |
| --- | --- | --- |
| `SSU2_VERSION` / `SSU2_NETWORK_ID` | 2 / 2 | `constants.rs` |
| `SSU2_DEFERRED_VERSIONS` | `[3, 4]` | `constants.rs` |
| `LONG_HEADER_LENGTH` / `SHORT_HEADER_LENGTH` | 32 / 16 | `constants.rs` |
| `CONNECTION_ID_LENGTH` / `PACKET_NUMBER_LENGTH` / `TOKEN_LENGTH` | 8 / 4 / 8 | `constants.rs` |
| `HEADER_COMMON_PREFIX_LENGTH` | 13 | `constants.rs` |
| `MIN_DATAGRAM_LENGTH` | 40 | `constants.rs` |
| `MAX_DATAGRAM_IPV4_LENGTH` / `MAX_DATAGRAM_IPV6_LENGTH` | 1472 / 1452 | `constants.rs` |
| `MIN_POST_HEADER_BYTES` / `HANDSHAKE_EPHEMERAL_LENGTH` | 24 / 32 | `constants.rs` |
| `SSU2_MIN_MTU` / `SSU2_MAX_MTU` | 1280 / 9000 | `constants.rs` |
| `MAX_SSU2_INTRODUCERS` / `MAX_SSU2_CAPS_BYTES` | 3 / 16 | `constants.rs` |
| `MAX_BLOCK_COUNT` / `MAX_UNKNOWN_BLOCK_BYTES` | 64 / 1024 | `constants.rs` |
| `MAX_ROUTER_INFO_BLOCK_BYTES` | 4096 | `constants.rs` |
| `MAX_I2NP_FRAGMENTS` / `MAX_FRAGMENT_NUMBER` | 64 / 127 | `constants.rs` |
| `MAX_ACK_RANGES` | 128 | `constants.rs` |
| `MAX_SIGNATURE_BYTES` / `MAX_PATH_DATA_BYTES` | 1024 / 1024 | `constants.rs` |
| `MAX_SESSION_CONFIRMED_FRAGMENTS` / `MAX_CONFIRMED_REASSEMBLED_BYTES` | 15 / 32768 | `constants.rs` |
| `MAX_RETRY_PADDING_BYTES` | 64 | `constants.rs` |
| `RETRY_AMPLIFICATION_NUMERATOR` / `_DENOMINATOR` | 3 / 1 | `constants.rs` |
| `TOKEN_LIFETIME_SECONDS` / `MAX_TOKENS_GLOBAL` / `MAX_TOKENS_PER_SOURCE` | 30 / 256 / 4 | `constants.rs` |
| `MAX_HANDSHAKE_ATTEMPTS` / `HANDSHAKE_DEADLINE_MS` | 4 / 20000 | `constants.rs` |
| `MAX_HANDSHAKE_REPLAY_ENTRIES` | 512 | `constants.rs` |
| `HANDSHAKE_MAX_CLOCK_SKEW_SECONDS` | 120 | `constants.rs` |
| `DATA_REPLAY_WINDOW_PACKETS` / `DATA_MAX_FUTURE_JUMP` | 128 / 1024 | `constants.rs` |
| `DATA_MAX_SENT_PACKETS` / `DATA_MAX_PENDING_RETRANSMIT_FRAGMENTS` | 256 / 256 | `constants.rs` |
| `DATA_MAX_FRAGMENT_RETRANSMISSIONS` | 5 | `constants.rs` |
| `DATA_MAX_REASSEMBLY_MESSAGES` / `DATA_MAX_REASSEMBLY_BYTES` | 16 / 262144 | `constants.rs` |
| `DATA_DUP_CACHE_ENTRIES` / `DATA_DUP_RETENTION_SECONDS` | 128 / 600 | `constants.rs` |
| `DATA_INITIAL_RTO_MS` / `DATA_MIN_RTO_MS` / `DATA_MAX_RTO_MS` | 1000 / 1000 / 60000 | `constants.rs` |
| `DATA_DEFAULT_ACK_DELAY_MS` / `DATA_DEFAULT_IMMEDIATE_ACK_DELAY_MS` | 25 / 5 | `constants.rs` |
| `DATA_MSS_BYTES` / `DATA_MIN_CWND_BYTES` / `DATA_DEFAULT_CWND_BYTES` / `DATA_MAX_CWND_BYTES` | 1220 / 2440 / 12200 / 60000 | `constants.rs` |
| `DATA_DEFAULT_IDLE_TIMEOUT_MS` | 300000 | `constants.rs` |
| `DATA_MAX_CONSECUTIVE_RTO_BEFORE_TERMINATE` | 5 | `constants.rs` |
| `DATA_IPV4_OVERHEAD_BYTES` / `DATA_IPV6_OVERHEAD_BYTES` | 60 / 80 | `constants.rs` |
| `MAX_SSU2_PQ_SCHEMES` | 8 | `address.rs` |
| `MAX_PATH_CANDIDATES` / `MAX_CANDIDATES_PER_FAMILY` | 4 / 2 | `path.rs` |
| `MAX_PATH_CHALLENGES_PER_SESSION` / `PATH_CHALLENGE_LENGTH` | 8 / 32 | `path.rs` |
| `PATH_VALIDATION_TIMEOUT_MS` / `PATH_CANDIDATE_MTU` | 10000 / 1280 | `path.rs` |
| `MAX_PEER_TESTS_GLOBAL` / `MAX_PEER_TESTS_PER_PEER` | 8 / 2 | `peer_test.rs` |
| `PEER_TEST_TIMEOUT_MS` / `PEER_TEST_MAX_CLOCK_SKEW_SECONDS` | 10000 / 120 | `peer_test.rs` |
| `MAX_RELAY_REQUESTS_GLOBAL` / `MAX_RELAY_REQUESTS_PER_PEER` | 8 / 2 | `relay.rs` |
| `MAX_RELAY_TAGS_GLOBAL` / `MAX_RELAY_TAGS_PER_PEER` | 16 / 4 | `relay.rs` |
| `RELAY_TAG_LIFETIME_SECS` / `RELAY_REQUEST_TIMEOUT_MS` | 120 / 10000 | `relay.rs` |
| `MAX_HOLE_PUNCH_PAYLOAD_BYTES` | 512 | `relay.rs` |
| `MAX_INTRODUCER_RECORDS` / `MAX_PUBLISHED_INTRODUCERS` | 8 / 3 | `introducer.rs` |
| `INTRODUCER_RECORD_LIFETIME_SECS` | 600 | `introducer.rs` |

## Dependencies

`crates/i2pr-transport-ssu2/Cargo.toml:10-23` — exactly ten
production dependencies and one dev-dependency:

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

[dev-dependencies]
rand_chacha.workspace = true
```

The allowlist row in `scripts/check-dependency-direction.sh:21-23`
matches the three workspace-internal path dependencies exactly:

```bash
"i2pr-transport-ssu2": {
    "i2pr-crypto", "i2pr-proto", "i2pr-transport"
},
```

`i2pr-runtime` is the only production consumer allowed to depend on
this crate (`scripts/check-dependency-direction.sh:51-54`).

**Where the ciphers actually live — the division is not a clean
split.** `i2pr-crypto` provides the checked X25519 DH
(`X25519PrivateKey`, `X25519SharedSecret`), the RFC 5869 HKDF helper
`hkdf_sha256_extract_and_expand`, `constant_time_eq`, `sha256`,
`router_identity_hash`, `verify_router_info`, and `verify_signature`
— but this crate calls the AEAD and raw stream primitives **directly
from the upstream crates**, not through `i2pr-crypto`:

- `chacha20::ChaCha20` with `KeyIvInit`/`StreamCipherSeek` for the
  header-protection keystreams (`src/crypto.rs:55-56`).
- `chacha20poly1305::ChaCha20Poly1305` for the data-phase AEAD and the
  intro-key token payload AEAD (`src/crypto.rs:57`, `:253`, `:272`,
  `:729`, `:750`, `:775`, `:797`).
- `sha2` and `hmac` for `SHA256` and HMAC-SHA256 primitives.

Separately, `i2pr-crypto` *does* now have a `chacha` module
(`crates/i2pr-crypto/src/chacha.rs`) — but it is **not** an SSU2
dependency surface. It is the raw RFC 7539 §2.4 layer-encryption
primitive added for Plan 332's encrypted LeaseSet2 layers, with a
*pinned initial block counter of 1* and a 12-byte RFC 8439 nonce
(`LAYER_INITIAL_BLOCK_COUNTER`). SSU2 header protection needs a
seekable stream with a different nonce/IV construction, so it uses the
upstream crate directly rather than that wrapper. The correct
statement is therefore: "SSU2 owns its own AEAD/stream sequencing and
borrows only DH/HKDF/hash/verify from `i2pr-crypto`" — not "all
primitives come from `i2pr-crypto`."

No runtime, socket, or async dependencies. `std::net` appears only as
pure data carriers (`IpAddr`, `SocketAddr`), spelled without the
banned literal in the crate's own source.

## Tests

**189 tests in the crate**, all passing
(`cargo test -p i2pr-transport-ssu2 --all-targets`):

| Target | Count |
| --- | --- |
| `src/lib.rs` inline unit tests (133 total across 15 modules) | 133 |
| `tests/handshake.rs` | 22 |
| `tests/data_phase.rs` | 17 |
| `tests/path_validation.rs` | 9 |
| `tests/peer_relay.rs` | 8 |

Inline unit-test distribution by module: `address.rs` 27,
`path.rs` 13, `session.rs` 13, `peer_test.rs` 12, `handshake.rs` 11,
`crypto.rs` 10, `token.rs` 9, `publication.rs` 8, `relay.rs` 7,
`block.rs` 6, `header.rs` 6, `introducer.rs` 4, `constants.rs` 3,
`packet.rs` 3, `state_machine.rs` 1, `lib.rs` 0.

**Committed vector corpus.** 13 fixtures (13 `.hex` files; the
directory also holds `README.md` and `manifest.tsv`) under
`tests/fixtures/ssu2/`, hash-pinned in `manifest.tsv` and enforced by
`bash scripts/check-ssu2-vectors.sh` (which verifies every row's
sha256, rejects manifest escapes, rejects unlisted files, and requires
the five baseline rows `long-header`, `short-header-data`,
`short-header-confirmed`, `blocks-positive`, `blocks-malformed`):

- **Plan 155 foundation (5, `spec-derived-constructed-vector`):**
  `long-header`, `short-header-data`, `short-header-confirmed`,
  `blocks-positive`, `blocks-malformed` (the only `malformed` row).
- **Plan 156 handshake (6, `locally-authored-deterministic-vector`):**
  `handshake-initial`, `header-protection-request`, `token-request`,
  `token-retry`, `session-created-full`, `session-confirmed-frag` —
  driving `tests/handshake.rs`.
- **Plan 157 data phase (2):** `data-phase-first`, `data-phase-ack`,
  reproduced byte-for-byte in `tests/data_phase.rs`.

Fixture bytes are committed and must not be edited; after any change
`bash scripts/check-fixture-manifest.sh` and
`bash scripts/check-ssu2-vectors.sh` must both pass.

| Area | Coverage |
| --- | --- |
| `constants.rs` | Noise name length, header-layout arithmetic, datagram-bound ordering |
| `address.rs` | Direct IPv4/IPv6, introducer-only, direct-with-introducers, unpublished-static, v3/v4/unknown versions, duplicates/conflicts/unknown options, hostname/port/key/IV/mtu/caps/introducer negatives, endpoint mismatch, missing intro key, `pq` tolerance, debug redaction |
| `header.rs` | Type round-trip + form classification, long exactness + version/network/flag/ID negatives, confirmed zero-packet + frag shape, data flags + immediate ACK, committed long/short fixtures |
| `packet.rs` | Length classes without touching bytes, split validation order, X + auth-tail minima, short/oversize/bad-header rejection |
| `block.rs` | All-21-block round trip, relay accept/reject shapes, truncation at every byte boundary, count/unknown/oversize ceilings, per-block malformation, unknown/reserved skip budget, committed positive/malformed fixtures |
| `crypto.rs` | Full initiator/responder transcript to matching split keys, role/stage gating, tag mutation + wrong-key rejection, header-protection round trip + wrong-key/short negatives, token AEAD round trip + nonce binding, labeled derivations, two-step data KDF shape, nonce ceiling |
| `handshake.rs` | TokenRequest round trip + skew stale/future, wrong-intro/truncation rejection, Retry round trip + amplification budget + zero-token rules + clock-skew termination, request build/parse round trip, confirmed fragmentation + duplicate/gap handling, replay cache bounds, cheap prevalidation drops |
| `token.rs` | One-use round trip, zero rejection, expiry + release, wrong source/port/family closure, per-source + global eviction, rotation, retry budget |
| `state_machine.rs` | Resend schedules, deadline exhaustion, per-phase cancellation, tag-mutation isolation |
| `tests/handshake.rs` | Full tokenless Retry trajectory to matching directional keys, cached-token trajectory, token valid/expired/wrong-source/reuse/rotation/unknown matrix with pre-DH fail-closed evidence, identical-byte resends, duplicate Created/Confirmed handling, deadline exhaustion + per-phase cancellation, 6-case RouterInfo binding matrix, RouterInfo-not-first rejection, 200-datagram cheap flood with bounded state, amplification budget, secret redaction, 6 committed handshake vectors (one with raw-primitive independent derivation) |
| `tests/data_phase.rs` | Bidirectional multi-message exchange, DATA-loss fresh retransmission with exact once-delivery, ACK-loss recovery without loops, duplicate/replay/corruption/reorder, first/middle/final fragment loss recovery, fragment reorder/duplicate/conflict, reassembly exact-capacity/max+1 with total cleanup, outbound-queue exact-capacity/max+1, congestion-gate boundedness, prolonged-loss bounded termination, idle timeout, termination lifecycle, two-session isolation, 2 committed data-phase vectors reproduced byte-for-byte |
| `session.rs` (unit) | Second-HKDF key shape, ACK underflow without mutation, duplicate-ACK idempotency, sent-history exact eviction, per-fragment ceiling silence, packet-number exhaustion, wrap boundaries, NextNonce rekey boundary, NewToken queue round trip, side-effect-free inbound matching |
| `path.rs` (unit) | No-challenge on validated source, single bounded candidate, wrong-response rejection with candidate survival, exact-once migration with proof consumption, timeout retention, global/family quotas, v4/v6 separation, conservative candidate MTU, no packet-driven MTU, zero-challenge rejection, challenge-budget exhaustion, scheduler deadlines |
| `publication.rs` (unit) | Deterministic canonical round trip, firewalled form without direct address, direct opt-out, expiry withdrawal, unvalidated-introducer rejection, zero-key/bad-MTU fail-closed, no private material, `publication_never_emits_pq` |
| `tests/path_validation.rs` | Real sealed-packet trajectories: unauthenticated/replay rejection without candidates, bounded candidate creation, wrong-response rejection, exact-once migration with continued bidirectional delivery, timeout retention, v4/v6 cross-validation refusal, migration congestion reset with semantic retention and fresh retransmit, minimum-MTU control fit |
| `peer_test.rs` (unit) | Spec preimage field order, full Alice trajectory to direct confirmation, unsigned-Msg 7 downgrade, contradiction without last-wins, invalid/stale/wrong-role/unknown boundedness, duplicate idempotency, concurrent isolation, exact quotas, expiry/cancel baselines, refusal/inconclusive neutrality, signer rejection, v6 separation |
| `relay.rs` (unit) | Request/response preimage order, HolePunch conn-ID rule, HolePunch round trip with wrong-key rejection, requester correlation with distinct-tag isolation, disabled-by-default quotas with replay idempotency, target validation with replay suppression, quota/expiry baselines |
| `introducer.rs` (unit) | Bounded deterministic live selection, failed-peer withdrawal, zero-tag/overflow eviction, publication-shape conversion |
| `tests/peer_relay.rs` | Sealed-session carriage of RelayRequest/Response/Intro and PeerTest Msg 4 into their tables, out-of-session PeerTest/HolePunch round trips, introducer→publication→expiry integration, conservative reachability consumption, privacy regression |

Determinism: every secret, connection ID, packet number, token byte,
challenge, timestamp, and both clocks is a caller-supplied
parameter. No test needs an RNG the crate cannot inject — `rand_core`
with `os_rng` and dev-dependency `rand_chacha` provide the test-side
deterministic generator.

Runtime-side suites that exercise this crate's machines over real
sockets (owned by `i2pr-runtime`, not by this crate) live in
`crates/i2pr-runtime/tests/ssu2_local.rs` (10 tests),
`crates/i2pr-runtime/tests/ssu2_peer_relay.rs` (7 tests), and the
inline module tests in `crates/i2pr-runtime/src/ssu2_runtime.rs`; see
[i2pr-runtime](i2pr-runtime.md).

## Distinctive design choices

1. **Version classification, not version parsing** — v3/v4 hit
   `UnsupportedVersion` and are separately tagged by
   `SSU2_DEFERRED_VERSIONS`, keeping the PQ-hybrid door visibly closed
   without a malformed-v2 mislabel.
2. **Exactly one tolerated SSU2 option, and it is inert** — `pq` is
   the sole carve-out, and it feeds only a diagnostic enumeration
   (`Ssu2PqKem`/`PqCapabilities`) that no production path branches on,
   while the session stays classical X25519 and publication stays
   pq-free. Every other unknown option is still rejected outright, so
   any future one needs an explicit allowlist entry.
3. **Normative pseudocode over annotations** — where the spec's Header
   Encryption KDF pseudocode and a raw-contents `n: 1` note disagree on
   the ephemeral-region nonce, the pseudocode governs; the choice is
   documented and vector-pinned.
4. **Inclusive-index reading** — `keydata[0:31]`, packet IV windows,
   and similar ranges are 32/12-byte inclusive spans; the NTCP2
   `MixKey` precedent confirms the construction.
5. **Token before DH, always** — unknown/expired/reused/misbound
   tokens drop before any expensive operation; the 200-datagram flood
   test proves bounded state under cheap invalid input.
6. **One-use by removal, not by a flag** — `TokenStore::consume`
   removes the entry, so a second presentation is indistinguishable
   from an unknown token and fails closed without a separate reuse
   counter to keep correct.
7. **No NetDB mutation in the handshake** — RouterInfo binding
   returns validated bytes; publication/freshness policy beyond the
   explicit window belongs to the caller.
8. **Migration never strands** — `note_path_migrated` requeues unacked
   fragments through the bounded loss policy instead of clearing
   provenance.
9. **Caller-supplied determinism** — secrets, connection IDs, packet
   numbers, token bytes, challenges, and both clocks arrive as
   parameters; the machines hold no RNG and read no time.
10. **No new crypto dependency for DH/HKDF/verify** — X25519, HKDF,
    SHA-256, and signature verification reuse `i2pr-crypto`; only
    transcript sequencing and SSU2 labels are local, per the Plan 156
    dependency review.
11. **Correlation by nonce, never by source** — NAT rewrites and
    concurrent schedules cannot confuse peer tests or relays because
    every transition keys on the test/relay nonce plus role/state,
    with the source family as a separation check only.
12. **Unsigned corroboration never confirms** — out-of-session
    messages without signatures advance the machine but downgrade the
    outcome to inconclusive rather than confirming.
13. **Relay success proves firewalled, never direct** — introducer use
    feeds firewalled-class evidence; direct publication still needs
    corroborated direct proof.

## Cross-references

- [Overview](overview.md) — crate index and data flow.
- [i2pr-transport](i2pr-transport.md) — provides
  `TransportKind::Ssu2`, the link/manager contracts,
  `EncodedI2npMessage`, `PeerId::hash`, and the `reachability`
  consumer of typed `PeerTestOutcome` / `RelayFirewalledSignal`.
- [i2pr-transport-ntcp2](i2pr-transport-ntcp2.md) — sibling
  protocol crate; structural precedent for address/header/block
  discipline and consuming transcripts.
- [i2pr-crypto](i2pr-crypto.md) — X25519/HKDF/SHA-256/signature
  provider, plus the separate ELS2 layer-encryption `chacha` module.
- [i2pr-runtime](i2pr-runtime.md) — owns the UDP sockets
  (`Ssu2RuntimeService`, `Ssu2PeerRelayService`) and central scheduler
  that drive these machines.
- [Dependency graph](dependency-graph.md) — the
  `i2pr-transport-ssu2` allowlist row.
- [Tooling](tooling.md) — scripts, fixtures, lanes, and CI.
- ADR [0002 — Tokio runtime boundary](../adr/0002-tokio-runtime-boundary.md)
  — the no-Tokio/no-socket rule this crate is the exemplar of.
- ADR [0010 — Transport contracts and crate
  boundaries](../adr/0010-transport-contracts-and-crate-boundaries.md)
  — where the protocol/runtime split is decided.
- ADR [0005 — Crypto dependency
  selection](../adr/0005-crypto-dependency-selection.md) — why
  primitives are borrowed rather than reimplemented.
- Plan-of-record:
  `plans/implementation/ssu2/155-m8-ssu2-v2-protocol-foundation-and-addresses.md`,
  `156-m8-ssu2-v2-handshake-token-and-routerinfo.md`,
  `157-m8-ssu2-v2-data-phase-reliability-and-fragmentation.md`,
  `158-m8-ssu2-udp-runtime-and-local-session-product.md`,
  `159-m8-ssu2-path-validation-publication-and-transport-selection.md`,
  `160-m8-ssu2-peer-test-and-relay-reachability.md`,
  `162-m8-ssu2-external-test-lane-isolation-and-ci-restoration.md`.
- Closure: `plans/closure/ssu2/155-status.md` through
  `plans/closure/ssu2/160-status.md`, and
  `plans/closure/ssu2/161-status.md` (**PASSED** — the external
  interop result).
- Interop evidence lane: `tests/integration/ssu2/run-independent.sh`
  (15 fail-closed rows) with
  `scripts/check-ssu2-acceptance-evidence.sh` and the manual
  `.github/workflows/ssu2-external.yml`.
- Fixture corpus: `tests/fixtures/ssu2/` with
  `tests/fixtures/ssu2/manifest.tsv`, checked by
  `scripts/check-ssu2-vectors.sh`.
- Support inventory: `specs/support.toml` (`ssu1 = "not-implemented"`,
  `ssu2_pq_v3_v4 = "deferred-compatibility-watch"`,
  `ssu2_direct_ipv4 = "passed"`, `ssu2_ipv6_structure = "passed"`,
  `ssu2_ipv6_interop = "infrastructure-limited-debt"`).
- Dossier: `specs/protocols/09-ssu2.md` and
  `specs/SOURCES.md` (Milestone 8 refresh).

## Status and bounded interop scope

SSU2 is the only transport in this repository with a **closed
external-interoperability result**. The scope is deliberately narrow
and must not be read wider:

- Plan 161 **passed both directions (A and B)** — i2pr as initiator and
  as responder — plus cached-token and compact malformed rows, against
  **exact-pinned i2pd 2.61.0** (`635b013a612ff47278ef02acf8580a28e10e26c5`,
  unmodified, unprivileged, ephemeral, `127.0.0.1` only) over real
  loopback UDP. All 24 mandatory acceptance criteria are evidenced in
  `plans/closure/ssu2/161-status.md`.
- **Milestone 8 final acceptance is `closed-via-plan161` within exactly
  that bounded direct-IPv4 loopback scope.**
- Java I2P 2.13.0 is recorded as nonblocking **secondary** debt, not a
  passing interop result; Plan 162 was a lane-isolation corrective that
  is itself closed.
- **IPv6 is structurally passed but externally unproven**:
  `ssu2_ipv6_structure = "passed"`,
  `ssu2_ipv6_interop = "infrastructure-limited-debt"`.
- **No public advertisement.** The router does not advertise SSU2 to
  the public network, no public-network/NetDB/tunnel/destination claim
  follows from Plan 161, and the RouterInfo transports remain
  non-advertised beyond the tested subset.
- `ssu1 = "not-implemented"`; `ssu2_pq_v3_v4` is
  `deferred-compatibility-watch`, and per the `pq` boundary above, no
  PQ session exists.
