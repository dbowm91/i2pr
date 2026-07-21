# `i2pr-transport-ntcp2` — Deep Dive

Runtime-neutral NTCP2 cryptography and protocol codecs: Noise XK
handshake, bounded message framing, AEAD data-phase frames with
obfuscated lengths, and deterministic consuming state machines.

Path: `crates/i2pr-transport-ntcp2/`

## Purpose

`i2pr-transport-ntcp2` owns the protocol mechanics — **no I/O, no
sockets, no `async fn`, no Tokio.** Every public API is synchronous,
side-effect-free (or has explicit transitions), and consumes/transfers
ownership through bounded byte vectors. The runtime adapter fulfills
`HandshakeAction` / `FrameAction` requests externally.

It does own:

- The full Noise XK transcript composition (`Transcript`), with
  per-stage role and stage checks.
- The consuming initiator and responder state machines
  (`InitiatorState`, `ResponderState`).
- Bounded handshake message codecs: `SessionRequest`,
  `SessionCreated`, `SessionConfirmed`, options blocks,
  `ConfirmedPayload`.
- The data-phase cipher + SipHash length-masking owners
  (`TransmitState`, `ReceiveState`).
- The block parser with unknown-block budget
  (`Block`, `ParsedBlocks`, `TerminationBlock`, etc.).
- Address parsing: `Ntcp2RouterAddress`, `Ntcp2Endpoint`,
  `ConfiguredListenAddress`, `ResolvedDialTarget`.
- AES-CBC ephemeral obfuscation, HMAC-SHA256 KDF, deterministic
  replay-cache reference.

## Module layout

| File | Lines | Responsibility | Main public types |
| --- | --- | --- | --- |
| `src/lib.rs` | — | Module declarations + 9-item address re-export | (re-exports) |
| `src/address.rs` | ~1150 | Strict `RouterAddress` parsing, endpoint resolution, I2P-base64 decoding | `Ntcp2RouterAddress`, `Ntcp2AddressMaterial`, `Ntcp2Endpoint`, `Ntcp2ObfuscationIv`, `ConfiguredListenAddress`, `ResolvedDialTarget`, `Ntcp2Capabilities`, `Ntcp2TransportStyle`, `Ntcp2AddressError` |
| `src/block.rs` | ~1015 | Bounded data-phase payload block codec | `Block`, `DecodedBlock`, `ParsedBlocks`, `TimestampBlock`, `OptionsBlock`, `RouterInfoBlock`, `I2npMessageBlock`, `PaddingBlock`, `TerminationBlock`, `TerminationReason`, `ReceivedI2npBlock`, `BlockError` |
| `src/constants.rs` | — | Protocol-dossier-derived constants and labels | `PROTOCOL_NAME`, `KEY_LENGTH`, `HASH_LENGTH`, `NONCE_LENGTH`, `AUTH_TAG_LENGTH`, `MAX_FRAME_LENGTH`, `MAX_NONCE`, `ASK_LABEL`, `SIPHASH_LABEL`, plus ~20 more |
| `src/crypto.rs` | ~1057 | Noise XK transcript, `CipherState`, AES obfuscation, SipHash, KDF, `SplitKeys` | `Transcript`, `CipherState`, `AesObfuscationState`, `SipHashState`, `SplitKeys`, `PublicKeyBytes`, `TranscriptHash`, `Role`, `AeadKey`, `Ntcp2CryptoError` |
| `src/frame.rs` | ~633 | `TransmitState` / `ReceiveState`; `EncodedFrame` / `ReceivedFrame`; `FrameAction` | `TransmitState`, `ReceiveState`, `EncodedFrame`, `ReceivedFrame`, `AuthenticatedPlaintext`, `FrameLength`, `FrameAssemblyPolicy`, `FrameAction`, `FrameError` |
| `src/handshake.rs` | ~1155 | `SessionRequest` / `SessionCreated` / `SessionConfirmed` codecs, options, payload, clock skew, replay cache | `SessionRequest`, `SessionCreated`, `SessionConfirmed`, `SessionRequestOptions`, `SessionCreatedOptions`, `ConfirmedPayload`, `ClockSkewPolicy`, `ReplayToken`, `ReplayDecision`, `ReferenceReplayCache`, `AuthenticatedPeer`, `HandshakeError` |
| `src/state_machine.rs` | ~1114 | Consuming initiator/responder state machines; `HandshakeAction` / `HandshakeInput` | `InitiatorState`, `ResponderState`, `AuthenticatedHandshake`, `NegotiatedParameters`, `HandshakeTransition`, `HandshakeAction`, `HandshakeInput`, `HandshakeBytes`, `PaddingMessage`, `TimestampPurpose` |

Integration test: `tests/handshake.rs` (328 lines) — full deterministic
initiator-responder handshake, `SessionConfirmed` decode at every
partial boundary, cancellation/deadline/disconnect terminal actions,
RouterInfo validation.

## Public surface (crate-root re-exports — `lib.rs:12-23`)

```rust
pub mod address;
pub mod block;
pub mod constants;
pub mod crypto;
pub mod frame;
pub mod handshake;
pub mod state_machine;

pub use address::{
    ConfiguredListenAddress, Ntcp2AddressError, Ntcp2AddressMaterial,
    Ntcp2Capabilities, Ntcp2Endpoint, Ntcp2ObfuscationIv,
    Ntcp2RouterAddress, Ntcp2TransportStyle, ResolvedDialTarget,
};
```

Other public items are accessed via module path
(`i2pr_transport_ntcp2::crypto::Transcript`, etc.).

## Key state machines and frame structures

### Noise transcript (`crypto.rs:395-782`)

`Transcript` implements the NTCP2 Noise XK pattern with explicit stage
progression:

```
Initial → Message1Complete → Message1Padded → Message2Complete
        → Message2Padded → StaticEncrypted → Confirmed → Split
```

- `new()` initializes with `PROTOCOL_NAME` hash + responder static
  key binding.
- `session_request` / `accept_session_request` — Message 1.
- `mix_padding` — cleartext padding after AEAD frame.
- `session_created` / `accept_session_created` — Message 2.
- `encrypt_static` / `decrypt_static` /
  `decrypt_static_unchecked` — Message 3 part 1.
- `mix_static_secret` — separate SE DH for the unchecked path.
- `encrypt_confirmed_payload` / `decrypt_confirmed_payload` —
  Message 3 part 2.
- `split` — derives directional `SplitKeys` from chaining key.

### Initiator state machine (`state_machine.rs:391-767`)

`InitiatorState` wraps `InitiatorPhase` with 8 states:

```
NeedRouterInfo → NeedRequestTimestamp → NeedRequestPadding
              → NeedConfirmedPadding → AwaitCreated
              → NeedPeerTimestamp → NeedConfirmedReplay → Done
```

Each `transition()` consumes self and returns
`HandshakeTransition<Self>` with bounded actions.

### Responder state machine (`state_machine.rs:817-1114`)

`ResponderState` wraps `ResponderPhase` with 6 states:

```
NeedRequest → AwaitReplay → NeedPeerTimestamp
            → NeedCreatedPadding → AwaitConfirmed → Done
```

Mirrors the initiator with complementary role checks.

### Data-phase owners (`frame.rs:268-516`)

- `TransmitState` (`frame.rs:268`) — owns a `CipherState` +
  `SipHashState`, seals plaintext/blocks into an `EncodedFrame`.
- `ReceiveState` (`frame.rs:385`) — 3-stage pipeline
  `Ready → AwaitingCiphertext → Terminated`; deobfuscates length,
  then authenticates, **then** exposes `ReceivedFrame`.
- `FrameAction` (`frame.rs:519`) — runtime-neutral `Write(EncodedFrame)`
  or `Terminate(TerminationBlock)`.

### AES obfuscation (`crypto.rs:222-283`)

`AesObfuscationState` — two-block AES-256-CBC chain for obfuscating
ephemeral X25519 keys. Advances the CBC chain on each encrypt/decrypt,
binding consecutive messages.

### SipHash frame-length masking (`crypto.rs:286-341`)

`SipHashState` — directional SipHash-2-4 stream producing 16-bit XOR
masks for frame-length obfuscation.

## Bounds, errors, and rejection rules

### Nonce boundary
- `NonceCounter` (`crypto.rs:134-153`) starts at 0, increments per
  seal/open. Returns `NonceExhausted` if it would exceed
  `MAX_NONCE = u64::MAX - 1`. The forbidden `2^64 - 1` is **never
  emitted** — the check fires before any increment produces it.

### Frame length bounds
- Ciphertext: `16..=65535` bytes (`constants.rs:25`,
  `crypto.rs:319`).
- Plaintext max: `65519` (`constants.rs:27`).
- Wire frame max: `65537` (`constants.rs:29`).

### Block parser rules (`block.rs:781-906`)
- Max 256 blocks per frame.
- Unknown block bytes capped at 4096 aggregate
  (`MAX_UNKNOWN_BLOCK_BYTES`).
- Termination may follow earlier valid non-padding blocks, but is the last
  non-padding block and permits only trailing Padding; it is accepted once
  with at most 256 additional bytes.
- Padding may appear once and must be final.
- General data-phase non-padding blocks may repeat where the specification
  permits; the separate SessionConfirmed payload parser remains strict.
- Unknown block types (5-223, 255): authenticated and skipped as
  bounded padding.

### Public-key rejection
`PublicKeyBytes::new()` (`crypto.rs:32-37`) rejects the all-zero
encoding (low-order point).

### Handshake message bounds
- `SessionRequest`: fixed 32-byte encrypted ephemeral + 32-byte
  encrypted options + up to 880 bytes padding.
- `SessionCreated`: same, padding up to 848 bytes.
- `SessionConfirmed`: part 1 = 48 bytes, part 2 up to 65487 bytes.

### Reserved-byte rejection
- `SessionRequestOptions::decode` (`handshake.rs:225-231`) rejects
  non-zero reserved bytes at offsets 6-7 and 12-15.
- `SessionCreatedOptions::decode` (`handshake.rs:280-289`) rejects
  non-zero reserved at offsets 0-1, 4-7, 12-15.

### Transcript stage enforcement
Every `Transcript` method checks `role` and `stage`, returning
`WrongRole` or `InvalidState` on mismatch. Compounded by the
consuming-API design — the caller cannot reach an invalid state
without a visible transition.

## Dependencies

`Cargo.toml:10-19`:

| Dependency | Workspace | Purpose |
| --- | --- | --- |
| `i2pr-crypto` | path | Ed25519/X25519/SHA-256 helpers |
| `i2pr-proto` | path | Wire types |
| `i2pr-transport` | path | Link/manager contracts |
| `aes` | workspace | AES-CBC ephemeral obfuscation |
| `chacha20poly1305` | workspace | AEAD |
| `hmac` | workspace | KDF |
| `sha2` | workspace | Transcript hash |
| `siphasher` | workspace | Frame-length masking |
| `thiserror` | workspace | Error derives |
| `zeroize` | workspace | Memory wiping |

`std::net::{IpAddr, SocketAddr}` appear in `address.rs` but **only as
pure data carriers** — no socket operations. Compliant with AGENTS.md
runtime boundaries.

## Tests

### Unit tests (inline)
- `crypto.rs:809-1057` — 6 tests covering nonce boundary, AES
  round-trip, full transcript cross-match, wrong-key rejection,
  AEAD tag mutation, independent primitives.
- `handshake.rs:974-1155` — 6 tests covering options encode/decode,
  reserved-byte rejection, truncation, padding bounds, block
  ordering in `ConfirmedPayload`, clock-skew/replay boundaries,
  base64 decoding.
- `block.rs:908-1015` — 4 tests covering canonical round-trip,
  unknown-byte budget, malformed order/duplicate rejection,
  termination typing, committed hex fixtures.
- `frame.rs:535-633` — 3 tests covering length obfuscation,
  authenticated round-trip with tag-mutation terminal failure,
  block assembly with deterministic padding.
- `address.rs:854-1150` — 7 tests covering IPv4/IPv6 parsing,
  structural RouterAddress, listen/dial type distinction,
  duplicate/conflicting option rejection, host/port/key/IV
  validation, debug redaction.

### Integration test
`tests/handshake.rs` — full deterministic initiator-responder
handshake driving both state machines through all transitions and
verifying matching data-phase keys. Plus `SessionConfirmed` decode at
every partial boundary, terminal actions, and RouterInfo signature +
transport-key binding rejection.

### Fixture-driven vector tests
Fixtures under `tests/fixtures/ntcp2/crypto/`:

| Fixture | Used by |
| --- | --- |
| `vectors.tsv` | `crypto.rs` — x25519, protocol-name hash, transcript initial/final, SessionRequest/Created/Confirmed AEAD, ChaCha20-Poly1305 seal, AES-CBC ephemeral, split-KDF |
| `data-phase-frame.hex` | `frame.rs` — sealed timestamp frame |
| `data-phase-blocks.hex` | `block.rs` — committed positive parse |
| `data-phase-malformed.hex` | `block.rs` — committed negative parse |
| `storage-static-key.hex` | `i2pr-storage` round-trip |
| `manifest.tsv` | All of the above |

All fixtures are loaded via `include_str!` at compile time. Manifest
integrity is enforced by `scripts/check-ntcp2-vectors.sh`.

## Distinctive design choices

1. **Two-phase static-key recovery (responder)** —
   `decrypt_static_unchecked` (`crypto.rs:626-652`) decrypts Alice's
   static key without an expected value, then `mix_static_secret`
   (`crypto.rs:658-672`) completes the SE DH. Required by the XK
   pattern: the responder must recover the static key before
   computing the SE shared secret.
2. **Post-KDF2 cipher for SessionConfirmed part one** —
   `encrypt_static`, `decrypt_static`, and `decrypt_static_unchecked`
   all use the current handshake cipher at `Message2Padded`, which is
   the post-KDF2 key. The NTCP2 dossier specifies that part one
   reuses the same AEAD key as SessionCreated options, matching
   both Java I2P and i2pd.
3. **Forbidden nonce safety** — counter increments *after* emitting
   the current value; the check fires before producing `2^64 - 1`.
4. **Bespoke two-block AES-CBC** — `AesObfuscationState`
   implements the NTCP2 dossier's XOR-feedback chain manually,
   **not** standard AES-CBC.
5. **SipHash-2-4 for length masking** — keys derived from a KDF
   label and evolves directionally.
6. **Split KDF: ASK + SipHash** — `Transcript::split`
   (`crypto.rs:709-747`) derives an "ASK" (Additional SipHash Key)
   from the chaining key first, then directional SipHash keys from
   that.
7. **Consuming API** — `Transcript`, `InitiatorState`, and
   `ResponderState` all consume `self` and return a new instance.
   The Rust type system enforces Noise state-machine invariants.
8. **Test-only hidden constructors** —
   `CipherState::from_key_for_test`, `SipHashState::from_material_for_test`,
   `PublicKeyBytes::from_bytes_for_test` are `#[doc(hidden)]`.
9. **Unknown block types are authenticated, not rejected** — they
   are skipped as bounded padding under the AEAD tag. This is
   forward-compatible.
10. **Deep `validate_router_info`** (`handshake.rs:870-923`) —
    structural decode + signature verification + identity-hash
    computation + X25519 key-type check + NTCP2 address option
    extraction + static-key binding. All without network access.

## Cross-references

- [Overview](overview.md)
- [i2pr-transport](i2pr-transport.md) — provides the link/manager
  contracts this crate is built against.
- [i2pr-crypto](i2pr-crypto.md) — provides `X25519PrivateKey` /
  `TransportStaticKey` (used as the source of the responder static
  key).
- [i2pr-runtime](i2pr-runtime.md) — owns the runtime service
  `Ntcp2RuntimeService` that drives these state machines.
- [i2pr-testkit](i2pr-testkit.md) — provides
  `Ntcp2DataPhaseDriver` and handshake fuzz targets.
- Plan-of-record:
  - `plans/032-m3-ntcp2-crypto-transcript-and-vectors.md`
  - `plans/033-m3-ntcp2-handshake-state-machines.md`
  - `plans/034-m3-ntcp2-data-phase-and-blocks.md`
- Closures: `plans/032-closure.md` … `plans/034-closure.md`.
- Synthetic interoperability lane: `tests/integration/ntcp2/`
  (manifest enforced by `scripts/check-ntcp2-interoperability.sh`).
