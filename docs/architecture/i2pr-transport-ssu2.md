# `i2pr-transport-ssu2` — Deep Dive

Runtime-neutral SSU2 v2 protocol foundation: strict RouterAddress
validation, structural packet-header codecs, and the bounded
authenticated-plaintext block vocabulary. No handshake, no header
protection, no UDP sockets.

Path: `crates/i2pr-transport-ssu2/`

## Purpose

`i2pr-transport-ssu2` owns the SSU2 v2 protocol mechanics that can be
expressed without I/O — **no Tokio, no sockets, no `async`
functions, no timers, no tasks.** Every public API is synchronous
and bounded. Later plans add the Noise XK handshake and header
protection (156), the data-phase state machines (157), and the
supervised UDP adapter in `i2pr-runtime` (158).

It does own:

- Spec-traced constants (versions, header/message/block IDs, size
  bounds) in `constants.rs`.
- Strict `RouterAddress` parsing: direct, introducer-only, and
  unpublished-static forms, plus distinct listen/dial types
  (`address.rs`).
- Structural long/short header codecs with exact-size discipline
  (`header.rs`).
- Datagram length validation and header/payload splitting
  (`packet.rs`).
- The bounded authenticated-plaintext block vocabulary with
  unknown-block budget and terminal-ordering rules (`block.rs`).

It does **not** own a handshake transcript, token lifecycle,
replay window, ACK/loss controller, reassembly state, UDP sockets,
peer-test/relay roles, or transport selection. Those belong to
Plans 156–161.

## Module layout

Declared in `src/lib.rs:14-18`. No subdirectories.

| File | Responsibility | Main public types |
| --- | --- | --- |
| `src/lib.rs` | Crate root, module declarations + re-exports | (re-exports) |
| `src/constants.rs` | SSU2-dossier-derived constants with source comments | `SSU2_VERSION`, `SSU2_NETWORK_ID`, `NOISE_PROTOCOL_NAME`, message-type IDs, block-type IDs, bound ceilings |
| `src/address.rs` | Strict `RouterAddress` parsing, introducers, listen/dial types, I2P-base64 decoding | `Ssu2RouterAddress`, `Ssu2AddressMaterial`, `Ssu2Endpoint`, `Ssu2Capabilities`, `Ssu2Introducer`, `Ssu2AddressClass`, `ConfiguredListenAddress`, `ResolvedDialTarget`, `Ssu2TransportStyle`, `Ssu2AddressError` |
| `src/header.rs` | Long/short header encode/decode, message-type vocabulary | `MessageType`, `HeaderForm`, `LongHeader`, `SessionConfirmedHeader`, `DataHeader`, `HeaderError` |
| `src/packet.rs` | Datagram length classes, header/payload split | `DatagramLengthClass`, `PacketHeader`, `SplitPacket`, `split_packet`, `PacketError` |
| `src/block.rs` | Bounded payload block codec | `Block`, `DecodedBlock`, `ParsedBlocks`, `AckBlock`, `TerminationBlock`, `TerminationReason`, `RelayResponseCode`, `PeerTestBlock`, `ReceivedI2npBlock`, `BlockError` |

## Public surface

Crate-root re-exports (`lib.rs:20-37`):

```rust
pub mod address;
pub mod block;
pub mod constants;
pub mod header;
pub mod packet;

pub use address::{...};
pub use block::{...};
pub use header::{...};
pub use packet::{...};
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
  known blocks may repeat. (The SessionConfirmed
  RouterInfo-first rule is a handshake-payload concern for
  Plan 156.)
- Per-block strictness: DateTime exactly 4; Options 12+
  (fixed-point ratios, no ordering assumption); RouterInfo
  flags limited to bits 0–1 with frag byte exactly `0x01`
  (never fragmented) and a 4096-byte ceiling — signature
  verification belongs to Plan 156; I2NP/first-fragment
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
- RelayResponse accept responses split the fresh 8-byte token
  from the fixed body tail; Bob rejections carry no
  endpoint/signature; Charlie rejections may carry both but
  never a token. PeerTest messages 2/4 require the 32-byte
  hash, messages 1–4 require a signature, 5–7 leave it
  optional, and the tested version must be 2.

## Errors

All error types implement `Display + Error + Clone + Copy +
Debug + Eq + PartialEq` (address errors clone a bounded
version string instead). No protocol-vs-operational mixing.

| Error | Module | Semantics |
| --- | --- | --- |
| `Ssu2AddressError` | `address.rs` | Style/version/option/host/port/key/introducer/endpoint failures, including `UnsupportedVersion` and `TooManyIntroducers` |
| `HeaderError` | `header.rs` | Truncation, trailing bytes, unknown type, wrong form, version/network/flag/connection-ID/fragment failures |
| `PacketError` | `packet.rs` | Datagram length classes, header failures, short authenticated tail |
| `BlockError` | `block.rs` | Truncation, over-length, count/budget ceilings, ordering, per-block malformation, `UnsupportedBlock`, oversize payloads |

## Dependencies

`Cargo.toml:10-13`:

```toml
[dependencies]
i2pr-proto     = { path = "../i2pr-proto" }
i2pr-transport = { path = "../i2pr-transport" }
thiserror.workspace = true
```

No crypto, runtime, socket, or async dependencies. `std::net`
appears only as pure data carriers (`IpAddr`, `SocketAddr`) —
no socket operations — matching the `i2pr-transport-ntcp2`
precedent and the runtime-boundary script.

## Tests

27 synchronous unit tests (inline), plus 5 committed fixtures
under `tests/fixtures/ssu2/` (pinned by `manifest.tsv`,
enforced by `scripts/check-ssu2-vectors.sh`):

| Area | Coverage |
| --- | --- |
| `constants.rs` | Noise name length, header-layout arithmetic, datagram-bound ordering |
| `address.rs` | Direct IPv4/IPv6, introducer-only, direct-with-introducers, unpublished-static, v3/v4/unknown versions, duplicates/conflicts/unknown options, hostname/port/key/IV/mtu/caps/introducer negatives, endpoint mismatch, missing intro key, debug redaction |
| `header.rs` | Type round-trip + form classification, long exactness + version/network/flag/ID negatives, confirmed zero-packet + frag shape, data flags + immediate ACK, committed long/short fixtures |
| `packet.rs` | Length classes without touching bytes, split validation order, X + auth-tail minima, short/oversize/bad-header rejection |
| `block.rs` | All-21-block round trip, relay accept/reject shapes, truncation at every byte boundary, count/unknown/oversize ceilings, per-block malformation, unknown/reserved skip budget, committed positive/malformed fixtures |

## Distinctive design choices

1. **Version classification, not version parsing** — v3/v4 hit
   `UnsupportedVersion`, keeping the PQ-hybrid door visibly
   closed without a malformed-v2 mislabel.
2. **Strict unknown options** — unlike NTCP2's `pq` carve-out,
   no deployed SSU2 extra option is currently accepted; any
   future one needs an explicit allowlist entry.
3. **Structural RouterInfo only** — flag/frag/size checks now,
   signature verification in Plan 156, so the foundation
   cannot be mistaken for an establishment claim.
4. **Opaque relay/peer-test signatures** — fixed prefixes are
   strict, trailing signatures are bounded evidence;
   verification lives with the Plan 160 roles.
5. **Token-from-tail split** — RelayResponse accept tokens come
   from the fixed 8-byte body tail, avoiding a signature-length
   oracle the foundation cannot resolve.
6. **Reachable unknown budget** — the 1024-byte ceiling sits
   under the datagram-scale payload cap, so the test actually
   exercises it.
7. **No new crypto dependency** — I2P-base64 and key-shape
   checks are local; PQ stays out of the tree per Plan 154.

## Cross-references

- [Overview](overview.md)
- [i2pr-transport](i2pr-transport.md) — provides
  `TransportKind::Ssu2`, link/manager contracts, and
  `EncodedI2npMessage`.
- [i2pr-transport-ntcp2](i2pr-transport-ntcp2.md) — sibling
  protocol crate; structural precedent for address/header/
  block discipline.
- [Dependency graph](dependency-graph.md) — the
  `i2pr-transport-ssu2` allowlist row.
- Plan-of-record:
  `plans/155-m8-ssu2-v2-protocol-foundation-and-addresses.md`.
- Closure: `plans/155-status.md`.
- Dossier: `specs/protocols/09-ssu2.md`,
  `specs/SOURCES.md` (Milestone 8 refresh).
