# `i2pr-transport-ntcp2` — Deep Dive

Runtime-neutral NTCP2 cryptography and protocol codecs: Noise XK
transcript, bounded handshake message codecs, consuming handshake state
machines, AEAD data-phase frames with SipHash-obfuscated lengths, and a
bounded data-phase block layer.

Path: `crates/i2pr-transport-ntcp2/`

> **Status: experimental and non-advertised.** NTCP2 is **not** enabled in the
> normal daemon path — `default_ntcp2_enabled()` returns `false`
> (`crates/i2pr-daemon/src/config.rs:718`) and every NTCP2 surface in
> `specs/support.toml` carries `status = "experimental"` with
> `advertised = false`. The retained NTCP2 interoperability **development**
> result is `protocol-defect-localized` at `noise_authenticated`
> (`plans/closure/ntcp2-transport/099-status.md:15-16`): the reference router
> authenticated the Noise handshake and i2pr then reported
> `terminal_rejected / reference-events-missing` before it observed the peer's
> `ntcp2_authenticated` event. A localized protocol defect is **not**
> interoperability. This crate is correct, deterministic, locally
> vector-tested protocol composition — it is not evidence that NTCP2 works
> against real peers.

## Purpose

`i2pr-transport-ntcp2` owns NTCP2 **protocol mechanics**: the Noise XK
transcript, handshake message codecs, handshake sequencing, the data-phase
AEAD/length-masking state, the data-phase block layer, and strict NTCP
`RouterAddress` validation. Every public API is synchronous, contains no
`async fn`, no Tokio, and no socket or filesystem operation, and moves bounded
byte vectors by ownership.

The crate **emits actions; the runtime fulfills them.** `InitiatorState` and
`ResponderState` return `HandshakeAction` values (read, write, timestamp,
replay, padding, RouterInfo) that only `i2pr-runtime` can actually perform;
the runtime returns the results as `HandshakeInput`. Deadlines, cancellation,
wall clock, padding randomness, replay storage, and local RouterInfo
retrieval are all runtime-owned seams. `FrameAction` is the analogous
data-phase request enum.

It must NOT own: sockets, timers, channels, task spawning, NetDB mutation,
capability advertisement, or publication policy (ADR 0010, ADR 0012,
ADR 0014). `lib.rs:9` declares `#![forbid(unsafe_code)]` and `lib.rs:10`
`#![warn(missing_docs)]`.

The NTCP2 static key is **not** part of `router.identity`. It lives in its own
versioned record in `i2pr-storage` (`storage.ntcp2-static-key` in
`specs/support.toml`), holding the static public key plus obfuscation IV with
no derivation coupling to the RouterIdentity file (ADR 0011,
`plans/closure/ntcp2-transport/032-closure.md`). `crates/i2pr-storage/src/lib.rs:1388`
covers the round-trip without identity coupling.

## Module layout

Line counts from `wc -l` at the current revision.

| Module | File | Lines | Responsibility | Key public types |
| --- | --- | --- | --- | --- |
| crate root | `src/lib.rs` | 23 | `#![forbid(unsafe_code)]`, 7 `pub mod`, 9-item `address` re-export | (re-exports only) |
| `address` | `src/address.rs` | 1169 | Strict NTCP `RouterAddress` parsing, option validation, endpoint resolution, listen/dial type separation, I2P-base64 material | `Ntcp2RouterAddress`, `Ntcp2AddressMaterial`, `Ntcp2Endpoint`, `Ntcp2ObfuscationIv`, `Ntcp2Capabilities`, `Ntcp2TransportStyle`, `ConfiguredListenAddress`, `ResolvedDialTarget`, `Ntcp2AddressError` |
| `block` | `src/block.rs` | 1107 | Bounded data-phase block encode/parse, unknown-block budget, termination typing, `i2pr-proto`/`i2pr-transport` I2NP reuse | `Block`, `DecodedBlock`, `ParsedBlocks`, `parse_blocks`, `encode_blocks`, `TimestampBlock`, `OptionsBlock`, `RouterInfoBlock`, `I2npMessageBlock`, `ReceivedI2npBlock`, `PaddingBlock`, `TerminationBlock`, `TerminationReason`, `BlockError` |
| `constants` | `src/constants.rs` | 64 | Dossier-derived protocol constants, protocol name, KDF labels | 26 `pub const` values (see below) |
| `crypto` | `src/crypto.rs` | 1048 | Noise XK `Transcript`, `CipherState`, AES-CBC obfuscation, SipHash-2-4 masking, HMAC-SHA256 KDF, `SplitKeys` | `Transcript`, `CipherState`, `AesObfuscationState`, `SipHashState`, `SplitKeys`, `PublicKeyBytes`, `TranscriptHash`, `Role`, `AeadKey`, `Ntcp2CryptoError` |
| `frame` | `src/frame.rs` | 633 | Directional data-phase owners, wire framing, block assembly policy, `FrameAction` | `TransmitState`, `ReceiveState`, `EncodedFrame`, `ReceivedFrame`, `AuthenticatedPlaintext`, `FrameLength`, `FrameAssemblyPolicy`, `into_directional_states`, `FrameAction`, `FrameError` |
| `handshake` | `src/handshake.rs` | 1161 | `SessionRequest`/`SessionCreated`/`SessionConfirmed` codecs, options blocks, confirmed payload, clock skew, replay cache, RouterInfo validation | `SessionRequest`, `SessionCreated`, `SessionConfirmed`, `SessionRequestOptions`, `SessionCreatedOptions`, `ConfirmedPayload`, `ClockSkewPolicy`, `ReplayToken`, `ReplayDecision`, `ReferenceReplayCache`, `AuthenticatedPeer`, `validate_router_info`, `HandshakeError` |
| `state_machine` | `src/state_machine.rs` | 1299 | Consuming initiator/responder state machines; `HandshakeAction`/`HandshakeInput` | `InitiatorState`, `ResponderState`, `AuthenticatedHandshake`, `NegotiatedParameters`, `HandshakeTransition`, `HandshakeAction`, `HandshakeInput`, `HandshakeBytes`, `PaddingMessage`, `TimestampPurpose` |

`InitiatorPhase`, `ResponderPhase`, `TranscriptStage`, `ReceiveStage`,
`InitiatorCommon`, `ResponderCommon`, and `ChainKey` are private; only the
state-set names below are observable.

One integration test file: `tests/handshake.rs` (346 lines).

## Public surface

The only crate-root re-exports are the nine `address` items
(`src/lib.rs:20-23`):

```rust
pub mod address;
pub mod block;
pub mod constants;
pub mod crypto;
pub mod frame;
pub mod handshake;
pub mod state_machine;

pub use address::{
    ConfiguredListenAddress, Ntcp2AddressError, Ntcp2AddressMaterial, Ntcp2Capabilities,
    Ntcp2Endpoint, Ntcp2ObfuscationIv, Ntcp2RouterAddress, Ntcp2TransportStyle, ResolvedDialTarget,
};
```

Everything else is reached by module path, e.g.
`i2pr_transport_ntcp2::crypto::Transcript`,
`i2pr_transport_ntcp2::state_machine::InitiatorState`.

### `crypto` (`src/crypto.rs`)

| Item | Signature / note |
| --- | --- |
| `PublicKeyBytes` | `new([u8;32])` rejects the all-zero low-order encoding; `Debug` prints `<redacted>` |
| `TranscriptHash` | `as_bytes() -> &[u8;32]`; public evidence, not a secret |
| `Role` | `Initiator` \| `Responder` |
| `AeadKey` | `#[derive(Zeroize)] #[zeroize(drop)]`, `pub` but no public constructor |
| `CipherState` | `seal(&[u8], aad)`, `open(&[u8], aad)`, `from_key_for_test` (`#[doc(hidden)]`) |
| `AesObfuscationState` | `new(router_hash, iv)`, `encrypt(&PublicKeyBytes)`, `decrypt` |
| `SipHashState` | `obfuscate_length(u16)`, `deobfuscate_length(u16)`, `mask_length` (alias), `from_material_for_test` (`#[doc(hidden)]`) |
| `SplitKeys` | `transmit()`, `receive()`, `transmit_lengths()`, `receive_lengths()`, `into_parts()` |
| `Transcript` | consuming; `new`, `role`, `responder_static`, `mix_hash`, `mix_padding`, `session_request`, `accept_session_request`, `session_created`, `accept_session_created`, `encrypt_static`, `decrypt_static`, `decrypt_static_unchecked`, `mix_static_secret`, `encrypt_confirmed_payload`, `decrypt_confirmed_payload`, `split`; plus `evidence_hash` (`#[doc(hidden)]`) |

### `frame` (`src/frame.rs`)

`EncodedFrame` (`as_bytes`, `len`, `is_empty`, `into_bytes`; `Debug` prints
length only), `FrameLength { ciphertext_length, plaintext_length }`,
`AuthenticatedPlaintext` (`as_bytes`, `len`, `is_empty`, `parse`),
`ReceivedFrame` (`length`, `plaintext`, `termination`),
`FrameAssemblyPolicy` (public fields + `new`, `coalescing_allowed`),
`TransmitState` (`new`, `frames_sent`, `is_terminated`, `seal_plaintext`,
`seal_blocks`), `ReceiveState` (`new`, `frames_received`, `is_terminated`,
`decode_length`, `open_ciphertext`, `open_wire_frame`),
`into_directional_states(SplitKeys)`, `FrameAction`.

### `handshake` (`src/handshake.rs`)

`NTCP2_VERSION: u8 = 2`, `DEFAULT_NETWORK_ID: u8 = 2`,
`SessionRequestOptions`, `SessionCreatedOptions`, `SessionRequest`,
`SessionCreated`, `SessionConfirmed`, `ConfirmedPayload`, `ClockSkewPolicy`
(`new`, `default_compatibility`, `maximum_delta`, `replay_retention`,
`classify`), `ReplayToken`, `ReplayDecision`, `ReferenceReplayCache`,
`AuthenticatedPeer`, `validate_router_info`.

### `state_machine` (`src/state_machine.rs`)

`PaddingMessage` (`SessionRequest` \| `SessionCreated` \| `SessionConfirmed`),
`TimestampPurpose` (`SessionRequest` \| `SessionCreated` \| `PeerValidation`),
`HandshakeBytes`, `HandshakeAction`, `HandshakeInput`,
`NegotiatedParameters` (5 public fields), `AuthenticatedHandshake`
(`role`, `peer`, `negotiated`, `split_keys`, `into_data_phase`),
`HandshakeTransition<S> { state, actions }`, `InitiatorState`, `ResponderState`
(including `phase_label() -> &'static str`).

### `address` / `block` / `constants`

`address` additionally exposes `NTCP2_MIN_PORT`, `NTCP2_MAX_PORT`,
`NTCP2_STATIC_PUBLIC_KEY_LENGTH`, `NTCP2_OBFUSCATION_IV_LENGTH`,
`NTCP2_ROUTER_ADDRESS_VERSION`. `block` exposes the 8 `BLOCK_*` type codes,
`BLOCK_HEADER_LENGTH`, `OPTIONS_MIN_LENGTH`, `MAX_OPTIONS_BYTES`,
`MAX_ROUTER_INFO_BYTES`, `MAX_PADDING_BYTES`,
`MAX_TERMINATION_ADDITIONAL_BYTES`, `MAX_BLOCK_COUNT`,
`MAX_UNKNOWN_BLOCK_BYTES`.

## Key contracts

### Noise XK transcript composition (`src/crypto.rs:395-774`)

`Transcript` is consuming: every method takes `self` and returns a new value,
so an illegal ordering is unrepresentable rather than merely checked. The
private `TranscriptStage` (`src/crypto.rs:384-392`) has exactly **seven**
stages:

```
Initial → Message1Complete → Message1Padded → Message2Complete
        → Message2Padded → StaticEncrypted → Confirmed
```

`split()` is a method, not a stage; it requires `Confirmed` and an existing
cipher. Every public method checks both `role` and `stage` first and returns
`WrongRole` or `InvalidState` (`src/crypto.rs:288-299` in `state_machine.rs`
maps these into `HandshakeError::TranscriptMismatch`).

Message-by-message, what each carries:

1. **`new(role, responder_static)`** — binds the symmetric state: `h` and the
   initial chaining key both start at `SHA256(PROTOCOL_NAME)`; then
   `h = SHA256(h || PROLOGUE || responder_static)`. `PROLOGUE` is empty
   (`constants.rs:10`).
2. **Message 1 — `session_request` / `accept_session_request`** (`Initial`).
   `h = SHA256(h || ephemeral)`; then the initiator DH `es` and `mix_key`;
   then one AEAD frame carrying the 16-byte options block. AAD is the current
   `h`; `h` is then advanced over the **ciphertext**.
3. **Message 1 padding — `mix_padding`** (`Message1Complete →
   Message1Padded`). Cleartext padding is mixed into `h` **after** the AEAD
   frame, never authenticated. Capped at
   `max(MAX_SESSION_REQUEST_PADDING, MAX_SESSION_CREATED_PADDING) = 880`.
4. **Message 2 — `session_created` / `accept_session_created`**
   (`Message1Padded`). Same shape: mix responder ephemeral, `ee` DH +
   `mix_key`, one AEAD frame over the 16-byte `SessionCreatedOptions`.
5. **Message 2 padding — `mix_padding`** (`Message2Complete →
   Message2Padded`), same cleartext treatment.
6. **Message 3 part one — `encrypt_static` / `decrypt_static` /
   `decrypt_static_unchecked`** (`Message2Padded`). A single AEAD frame
   carrying Alice's 32-byte static public key, AAD = current `h`, then `h`
   over the ciphertext, then the `se` DH via `mix_key`
   (`→ StaticEncrypted`). All three variants use the **same post-KDF2 cipher
   state that encrypted the SessionCreated options** — SessionConfirmed part
   one reuses that key and the next nonce, matching the dossier, Java I2P,
   and i2pd (ADR 0011).
7. **Message 3 part two — `encrypt_confirmed_payload` /
   `decrypt_confirmed_payload`** (`StaticEncrypted`). One AEAD frame over the
   `ConfirmedPayload` plaintext, AAD = current `h`, `h` advanced over the
   ciphertext (`→ Confirmed`).
8. **`split()`** (`Confirmed`). From the chaining key:
   `temp = HMAC(ck, [])`; `k_ab = HMAC(temp, [1])`;
   `k_ba = HMAC(temp, k_ab || [2])`. Then the SipHash side: an "ASK"
   (additional SipHash key) master from `HMAC(temp, ASK_LABEL || [1])`, and
   SipHash material from `HMAC(ask_master, h || SIPHASH_LABEL)` expanded with
   the same `[1]`/`[2]` index pattern. The initiator maps
   `(k_ab, sip_ab)` to transmit and `(k_ba, sip_ba)` to receive; the responder
   maps them swapped (`src/crypto.rs:732-735`).

`decrypt_static_unchecked` + `mix_static_secret` are a deliberate two-step
responder path (`src/crypto.rs:616-667`): the responder must first recover
Alice's static public key, then compute the `se` DH with it, then finish
message 3. The unchecked read is *not* authentication — the caller must bind
the returned key against the validated RouterInfo
(`validate_router_info`, `src/state_machine.rs:1216-1221`) before treating the
handshake as authenticated.

### Initiator state machine (`src/state_machine.rs:332-833`)

`InitiatorPhase` has **nine** states:

```
NeedRouterInfo → NeedRequestTimestamp → NeedRequestPadding → NeedConfirmedPadding
               → AwaitCreated → AwaitCreatedPadding → NeedPeerTimestamp
               → NeedConfirmedReplay → Done
```

`AwaitCreatedPadding` exists only on the initiator's inbound path: when
`SessionCreatedOptions.padding_length == 0` the initiator mixes empty padding
inline and jumps straight to `NeedPeerTimestamp`; otherwise it enters
`AwaitCreatedPadding` and issues a follow-up `ReadExact`
(`src/state_machine.rs:663-702`).

### Responder state machine (`src/state_machine.rs:844-1245`)

`ResponderPhase` has **seven** states:

```
NeedRequest → AwaitReplay → AwaitRequestPadding → NeedPeerTimestamp
            → NeedCreatedPadding → AwaitConfirmed → Done
```

`AwaitRequestPadding` mirrors the initiator's `AwaitCreatedPadding`: it
exists only when the authenticated `padding_length` is non-zero
(`src/state_machine.rs:1034-1067`). `ResponderState::phase_label()` returns a
closed set of redacted labels — `need_request`, `await_replay`,
`await_request_padding`, `need_peer_timestamp`, `need_created_padding`,
`await_confirmed`, `done` — so a terminal failure can be classified by stage
without leaking protocol bytes (`src/state_machine.rs:1251-1261`).

### How the machines report need-more-bytes / done / rejected

There is no "partial" return type. A machine consumes itself, applies exactly
one `HandshakeInput`, and returns `HandshakeTransition<Self>`; the phase name
*is* the machine's memory.

- **Need more bytes** — the transition's `actions` contain a read request
  (`ReadExact { length }`). The initiator emits `ReadExact` for the
  `SessionCreated` minimum (`src/state_machine.rs:622`), for the negotiated
  `SessionCreated` padding (`:698`), and the responder for `SessionRequest`
  minimum (`:927`), `SessionRequest` padding (`:1063`), and the exact
  `SessionConfirmed` total of `SESSION_CONFIRMED_PART1_LENGTH +
  expected_part2_length` (`:1179-1183`). `ReadBounded { minimum, maximum }`
  is a declared variant but is **not currently emitted** by either machine;
  every length is known before the read.
- **Done** — the phase becomes `Done` and `actions` contain
  `Authenticated(AuthenticatedHandshake)`. The initiator emits
  `Write(SessionConfirmed)` + `Authenticated` in one transition
  (`:824-828`); the responder emits `Authenticated` alone (`:1235-1240`).
  A state in `Done` has no further legal transition.
- **Rejected** — two distinguishable paths. A *protocol* rejection returns
  `Err(HandshakeError)` directly and yields no state; an *operational*
  rejection (cancel, deadline, disconnect) returns a normal transition into
  `Done` carrying `Terminate(HandshakeError::Cancelled / DeadlineExpired /
  Disconnected)` (`:455-478`, `:950-973`), so the runtime always sees a typed
  terminal action. An input that does not match the current phase falls
  through to `_ => Err(HandshakeError::StateViolation)` (`:830`, `:1242`).

`HandshakeTransition` is created through a private `transition()` helper with
a `debug_assert!` that `actions.len() <= MAX_HANDSHAKE_ACTIONS = 32`
(`src/state_machine.rs:279-282`); `HandshakeBytes::new` rejects any owned byte
vector over `MAX_HANDSHAKE_BUFFERED_INPUT` (`src/state_machine.rs:52-57`).

### `HandshakeAction` — the full enum (`src/state_machine.rs:90-133`)

Nine variants; every one is a request the runtime must fulfill. None waits,
performs I/O, reads a clock, or touches a replay store.

| Variant | Payload |
| --- | --- |
| `ReadBounded` | `{ minimum: usize, maximum: usize }` — declared, not currently emitted |
| `ReadExact` | `{ length: usize }` |
| `Write` | `HandshakeBytes` (bounded, `Debug` prints length only) |
| `RequestTimestamp` | `{ purpose: TimestampPurpose }` |
| `RequestReplay` | `{ token: ReplayToken, retention: u64 }` |
| `RequestPadding` | `{ message: PaddingMessage, maximum: usize }` |
| `RequestRouterInfo` | `{ maximum: usize }` |
| `Authenticated` | `AuthenticatedHandshake` |
| `Terminate` | `HandshakeError` |

`HandshakeInput` (`src/state_machine.rs:177-194`) is the mirror: `Bytes`,
`Timestamp`, `Replay`, `Padding`, `RouterInfo`, `Cancelled`,
`DeadlineExpired`, `Disconnected`.

### `FrameAction` — the full enum (`src/frame.rs:519-524`)

Two variants: `Write(EncodedFrame)` and `Terminate(TerminationBlock)`. It is
the data-phase request enum; the frame owners return `Result<_, FrameError>`
and the runtime adapter maps those results into `FrameAction`.

### AES-CBC ephemeral obfuscation (`src/crypto.rs:222-283`)

`AesObfuscationState::new(router_hash, iv)` sets the AES-256 **key** to the
peer router hash and the **chain/IV** to the published 16-byte obfuscation IV
(`Ntcp2ObfuscationIv`). It is bespoke, not a library CBC mode: for each
16-byte chunk of the 32-byte ephemeral key it XORs the input with the current
chain, runs one raw `Aes256::encrypt_block`, writes the result out, and sets
the chain to that ciphertext block (`src/crypto.rs:243-257`); decrypt
mirrors it with `decrypt_block` and the *previous* chain
(`src/crypto.rs:267-280`).

**This is obfuscation, not confidentiality.** The key is the peer's router
hash, which is public and published in the RouterInfo. The construction exists
to keep the static-looking ephemeral field from being trivially linkable
across sessions, and the advancing chain binds consecutive messages to a
shared obfuscation position. The real ephemeral agreement happens underneath
in the transcript's X25519 DH. `encrypt` returns via
`from_bytes_for_test` because ciphertext is not a low-order point to be
rejected; `decrypt` returns `PublicKeyBytes::new` and does reject an all-zero
plaintext.

### ChaCha20-Poly1305 data phase (`src/crypto.rs:155-220`, `src/frame.rs:268-496`)

`CipherState` owns one `AeadKey` plus a `NonceCounter`. **Two directional
keys** come from the split: `SplitKeys::transmit`/`receive` become
`TransmitState`/`ReceiveState` via `into_directional_states`.

- **Nonce/IV construction** — the 12-byte nonce is 4 zero bytes followed by the
  8-byte **little-endian** counter (`src/crypto.rs:149-151`). The counter
  starts at 0 and advances once per `seal`/`open`; the two directions have
  independent counters, so both peers can send nonce 0 simultaneously.
- **Forbidden nonce** — `MAX_NONCE = u64::MAX - 1` (`constants.rs:60`). The
  check is `if self.0 > MAX_NONCE { return Err(NonceExhausted) }` *before*
  emitting and incrementing, so `2^64 - 1` is never produced and the counter
  can never wrap (`src/crypto.rs:141-152`, exercised by
  `nonce_boundaries_are_checked_before_reuse`). `TransmitState` latches
  `terminated = true` on `NonceExhausted` (`src/frame.rs:313-315`).
- **AAD** — the data phase uses **empty associated data** (`&[]` at
  `src/frame.rs:310` and `src/frame.rs:446`), matching ADR 0013. Contrast the
  handshake, where AAD is the current transcript hash
  (`src/crypto.rs:763`, `:770`).
- **Handshake nonce reuse is deliberate** — `encrypt_static` does not call
  `mix_key` between the SessionCreated options frame and the part-one frame,
  so part one is the *next* nonce of the same key (ADR 0011).

### Directional SipHash frame-length masking (`src/crypto.rs:285-341`)

`SipHashState::new(material)` splits the 32-byte material into
`key1 = LE(material[0..8])`, `key2 = LE(material[8..16])`, and an 8-byte IV
`material[16..24]` (`src/crypto.rs:293-301`). `next_mask()` runs
`SipHasher24::new_with_keys(key1, key2)` over the current 8-byte IV, replaces
the IV with the little-endian 64-bit digest, and returns its first two bytes
as a `u16` mask. The wire length is `clear ^ mask`, written **big-endian**
(`src/frame.rs:324`) and read as `u16::from_be_bytes`
(`src/frame.rs:420`).

The security property is that the mask is a keyed PRF stream, so the
obfuscated prefix is indistinguishable from a random 16-bit value and **must
not become a length oracle**:

- `obfuscate_length` validates the *clear* length is in `16..=65535` before
  XOR (`src/crypto.rs:318-323`), so a caller cannot use it to mint an
  out-of-range value.
- `deobfuscate_length` accepts **any** `u16` on the wire and validates only
  *after* XOR (`src/crypto.rs:329-335`). The source comment is explicit:
  validating the pre-XOR value would reject valid frames whose obfuscated
  prefix happens to land below 16. Post-XOR validation is the only check that
  preserves a single failure mode for a corrupt prefix.
- The receive side also enforces exact ciphertext length against the
  deobfuscated value, and terminates on any mismatch
  (`src/frame.rs:442-445`).

Each direction has its own `SipHashState`, so the two peers' length streams
are independent.

### `block.rs` block layer vs `frame.rs` framing layer vs `i2pr-proto`

They are three distinct layers and the doc should not conflate them:

- **`i2pr-proto`** supplies the *envelope* types this crate decodes into:
  `RouterAddress`, `Hash`, `Date`, `Mapping`, `CryptoKeyType::X25519`,
  `RouterInfo`, and the constants `MAX_COMMON_STRUCTURE_SIZE` /
  `SHORT_TRANSPORT_HEADER_SIZE`. This crate does not redefine them.
- **`i2pr-transport`** supplies `AddressFamily` (address layer) and
  `EncodedI2npMessage` / `MAX_I2NP_MESSAGE_BYTES` (block layer), so the NTCP
  I2NP block validates against the same shared bound as every other transport
  rather than a private copy.
- **`block.rs`** is the *payload block* layer inside an already
  authenticated plaintext: `encode_blocks` and `parse_blocks` handle the
  3-byte `type + u16 length` header, the 7 decoded kinds, and the aggregate
  unknown-byte budget. It is transport-agnostic and knows nothing about
  framing or ciphers.
- **`frame.rs`** is the *framing* layer: it owns the wire length prefix, the
  AEAD boundary, the directional counters, and assembly policy. It consumes
  `block` output and never parses a block header itself; `ReceivedFrame`
  exposes `AuthenticatedPlaintext`, and only that type's `parse()` calls into
  `block::parse_blocks`.

Order on the receive path is therefore: deobfuscate length →
authenticate the whole ciphertext → wrap in `AuthenticatedPlaintext` →
`parse_blocks` → expose `ReceivedFrame`. A block-parse failure is terminal
for that receive owner (`src/frame.rs:460-463`).

### Bound constants and typed errors

`constants.rs` (26 values, all `pub`):

| Constant | Value |
| --- | --- |
| `PROTOCOL_NAME` | `b"Noise_XKaesobfse+hs2+hs3_25519_ChaChaPoly_SHA256"` |
| `PROLOGUE` | `b""` |
| `KEY_LENGTH` | 32 |
| `HASH_LENGTH` | 32 |
| `NONCE_LENGTH` | 12 |
| `AUTH_TAG_LENGTH` | 16 |
| `AES_BLOCK_LENGTH` | 16 |
| `OPTION_BLOCK_LENGTH` | 16 |
| `MAX_FRAME_LENGTH` | 65_535 |
| `MAX_FRAME_PLAINTEXT` | `MAX_FRAME_LENGTH - AUTH_TAG_LENGTH` = 65_519 |
| `MAX_WIRE_FRAME_LENGTH` | `MAX_FRAME_LENGTH + 2` = 65_537 |
| `MAX_SESSION_CONFIRMED_PART2` | 65_487 |
| `MAX_SESSION_CONFIRMED_PART2_PLAINTEXT` | 65_471 |
| `HANDSHAKE_EPHEMERAL_LENGTH` | 32 |
| `HANDSHAKE_OPTIONS_FRAME_LENGTH` | `OPTION_BLOCK_LENGTH + AUTH_TAG_LENGTH` = 32 |
| `MIN_HANDSHAKE_MESSAGE_LENGTH` | 64 |
| `SESSION_CONFIRMED_PART1_LENGTH` | 48 |
| `MAX_HANDSHAKE_MESSAGE_LENGTH` | 65_535 |
| `MAX_SESSION_CONFIRMED_LENGTH` | 65_535 |
| `MAX_ROUTER_INFO_PAYLOAD` | 65_536 |
| `MAX_CONFIRMED_OPTIONS` | 4_096 |
| `MAX_HANDSHAKE_ACTIONS` | 32 |
| `MAX_HANDSHAKE_BUFFERED_INPUT` | 65_535 |
| `MAX_SESSION_REQUEST_PADDING` | 880 |
| `MAX_SESSION_CREATED_PADDING` | 848 |
| `MAX_NONCE` | `u64::MAX - 1` |
| `ASK_LABEL` | `b"ask"` |
| `SIPHASH_LABEL` | `b"siphash"` |

Module-local bounds: `frame.rs` `MIN_FRAME_LENGTH = 16`,
`MAX_PLAINTEXT_LENGTH = 65_519`, `FRAME_OVERHEAD = 18`; `block.rs`
`BLOCK_DATETIME/OPTIONS/ROUTER_INFO/I2NP/TERMINATION/PADDING/FUTURE = 0/1/2/3/4/254/255`,
`BLOCK_EXPERIMENTAL_MIN/MAX = 224/253`, `OPTIONS_MIN_LENGTH = 12`,
`MAX_OPTIONS_BYTES = 4_096`, `MAX_ROUTER_INFO_BYTES = 65_515`,
`MAX_PADDING_BYTES = 65_516`, `MAX_TERMINATION_ADDITIONAL_BYTES = 256`,
`MAX_BLOCK_COUNT = 256`, `MAX_UNKNOWN_BLOCK_BYTES = 4_096`; `address.rs`
`NTCP2_MIN_PORT = 1`, `NTCP2_MAX_PORT = 65535`,
`NTCP2_STATIC_PUBLIC_KEY_LENGTH = 32`, `NTCP2_OBFUSCATION_IV_LENGTH = 16`,
`NTCP2_ROUTER_ADDRESS_VERSION = 2`; `handshake.rs` `NTCP2_VERSION = 2`,
`DEFAULT_NETWORK_ID = 2`.

### `Ntcp2CryptoError` (10 variants, `src/crypto.rs:81-115`)

`InvalidPublicKey`, `FieldTooLarge`,
`FrameLengthOutOfRange { length: u16 }`, `NonceExhausted`,
`EncryptionFailed`, `AuthenticationFailed`, `InvalidState`, `WrongRole`,
`PeerStaticMismatch`, `KdfInput`.

### `HandshakeError` (22 variants, `src/handshake.rs:27-103`)

`Truncated`, `InvalidFixedLength`, `ExcessivePadding`, `MalformedOptions`,
`DeobfuscationFailure`, `AuthenticationFailure`, `TranscriptMismatch`,
`InvalidKeyAgreement`, `WrongNetwork`, `StaleTimestamp`, `FutureTimestamp`,
`ReplayDetected`, `ReplayCacheUnavailable`, `PeerIdentityMismatch`,
`TransportStaticKeyMismatch`, `RouterInfoMalformed`,
`RouterInfoSignatureInvalid`, `UnsupportedPeerKey`, `StateViolation`,
`Cancelled`, `DeadlineExpired`, `Disconnected`, `LocalPolicyDenied`,
`Codec(#[from] CodecError)`, `Crypto(#[from] Ntcp2CryptoError)`.

### `FrameError` (10 variants, `src/frame.rs:28-56`)

`TruncatedLength`, `InvalidLength`, `TruncatedCiphertext`,
`AuthenticationFailure`, `CounterExhausted`, `StateViolation`,
`PayloadTooLarge`, `Blocks(#[source] BlockError)`,
`Crypto(#[source] Ntcp2CryptoError)`.

### `BlockError` (15 variants, `src/block.rs:56-102`)

`Truncated`, `LengthExceedsFrame`, `ExcessiveBlockCount`,
`ExcessiveUnknownBytes`, `InvalidLength`, `DuplicateBlock`, `InvalidOrder`,
`InvalidTermination`, `RouterInfoMalformed`, `RouterInfoSignatureInvalid`,
`PeerIdentityMismatch`, `PeerStaticKeyMismatch`, `I2npMalformed`,
`OptionsMalformed`, `PayloadTooLarge`.

### `Ntcp2AddressError` (13 variants, `src/address.rs:42-95`)

`UnsupportedTransportStyle`, `UnknownOption`, `DuplicateOption`, `MissingOption`,
`ConflictingOptions`, `InvalidOptionValue`, `HostnameNotAllowed`,
`InvalidPort`, `PortOutOfRange`, `MissingEndpoint`, `EndpointMismatch`,
`InvalidStaticPublicKey` (+ the carried bounded metadata fields for the
option/port variants).

### Block parser rules (`src/block.rs:776-880`)

- Input over `MAX_FRAME_PLAINTEXT` is `PayloadTooLarge`.
- At most `MAX_BLOCK_COUNT = 256` blocks; the 257th is
  `ExcessiveBlockCount`.
- Every header and body is bounds-checked with `checked_add`; a header that
  does not fit is `Truncated`, an overflowing length is
  `LengthExceedsFrame`.
- **Padding** (254) may appear at most once (`DuplicateBlock` otherwise) and
  must be final — any non-padding block after it is `InvalidOrder`.
- **Termination** (4) may appear at most once and must be the final
  non-padding block (`InvalidOrder` otherwise). Its body is
  `9..=9 + MAX_TERMINATION_ADDITIONAL_BYTES` = `9..=265` bytes: 8-byte
  big-endian valid-frame count, 1-byte reason, and up to 256 additional bytes
  whose content is **not retained** (`additional_length` only). Any other
  length is `InvalidTermination`.
- Specification-permitted non-padding blocks **may repeat** where the
  specification allows; the separate `ConfirmedPayload` parser is the strict
  one and rejects repeats.
- Unknown types are **authenticated and skipped** as bounded padding, with an
  aggregate `MAX_UNKNOWN_BLOCK_BYTES = 4_096` budget
  (`ExcessiveUnknownBytes` on checked-add overflow or budget excess).
  `DecodedBlock::Unknown { block_type, length }` records the skip.
- `BLOCK_DATETIME` requires exactly 4 body bytes. `BLOCK_I2NP` requires
  `SHORT_TRANSPORT_HEADER_SIZE <= body <= MAX_I2NP_MESSAGE_BYTES`.

### Handshake message bounds

- `SessionRequest` / `SessionCreated`: 32-byte AES-obfuscated ephemeral +
  exactly `HANDSHAKE_OPTIONS_FRAME_LENGTH = 32` bytes of sealed options frame
  (a 16-byte options block plus its 16-byte tag) + cleartext padding capped at
  880 / 848 respectively, and the whole message capped at
  `MAX_HANDSHAKE_MESSAGE_LENGTH = 65_535`. The minimum complete message is
  `MIN_HANDSHAKE_MESSAGE_LENGTH = 64`.
- `SessionConfirmed`: part one exactly `SESSION_CONFIRMED_PART1_LENGTH = 48`
  (32-byte static key + tag); part two in
  `AUTH_TAG_LENGTH..=MAX_SESSION_CONFIRMED_PART2` = `16..=65_487`; total
  capped at 65_535. `SessionConfirmed::decode` requires the input length to
  **equal** `48 + expected_part2_length` and distinguishes short
  (`Truncated`) from long (`InvalidFixedLength`)
  (`src/handshake.rs:543-549`).
- `SessionRequestOptions::decode` additionally requires
  `session_confirmed_part2_length` to be in
  `AUTH_TAG_LENGTH..=MAX_SESSION_CONFIRMED_PART2`, so a peer cannot negotiate
  an impossible part two (`src/handshake.rs:235-239`).

### Reserved-byte rejection

All fields are big-endian.

- `SessionRequestOptions` (16 bytes): 0 network_id, 1 version (must be
  `NTCP2_VERSION`), 2–3 padding_length, 4–5 session_confirmed_part2_length,
  **6–7 reserved must be zero**, 8–11 timestamp, **12–15 reserved must be
  zero** (`src/handshake.rs:219-247`).
- `SessionCreatedOptions` (16 bytes): **0–1 reserved**, 2–3 padding_length,
  **4–5 reserved**, **6–7 reserved**, 8–11 timestamp, **12–15 reserved**
  (`src/handshake.rs:278-301`).
- `ConfirmedPayload::decode` requires router block type `== 2`, router length
  `>= 1`, and reserved RouterInfo flag bits `body[0] & 0xfe == 0`; the only
  blocks permitted after the RouterInfo are one Options block
  (`>= 12`, `<= MAX_CONFIRMED_OPTIONS`) and one Padding block, in that order.
  Anything else is `MalformedOptions` (`src/handshake.rs:629-677`).

### `validate_router_info` (`src/handshake.rs:876-978`)

Structural decode, signature verification, identity-hash computation, X25519
key-type check, NTCP2 address option extraction, and transport static-key
binding — all without network access. The initiator calls it on its **own**
RouterInfo before sending SessionRequest; the responder calls it on Alice's
RouterInfo from the confirmed payload, with the expected peer hash and the
static key recovered by `decrypt_static_unchecked`.

## Dependencies

`Cargo.toml:10-20` — production dependencies, all workspace-inherited or
path:

| Dependency | Workspace | Purpose in this crate |
| --- | --- | --- |
| `i2pr-crypto` | path | `X25519PrivateKey` / `X25519SharedSecret` (state machines), `sha256`, `constant_time_eq` (transcript), `CryptoError` (error mapping) |
| `i2pr-proto` | path | `RouterInfo`, `RouterAddress`, `Mapping`, `Hash`, `Date`, `CryptoKeyType::X25519`, `MAX_COMMON_STRUCTURE_SIZE`, `SHORT_TRANSPORT_HEADER_SIZE` |
| `i2pr-transport` | path | `AddressFamily`, `EncodedI2npMessage`, `MAX_I2NP_MESSAGE_BYTES` |
| `aes` | workspace | AES-256 block primitive for ephemeral obfuscation |
| `chacha20poly1305` | workspace | ChaCha20-Poly1305 AEAD (handshake + data phase) |
| `hmac` | workspace | HMAC-SHA256 KDF / HKDF-style chaining |
| `sha2` | workspace | SHA-256 transcript hashing |
| `siphasher` | workspace | SipHash-2-4 length masking |
| `thiserror` | workspace | Error derives |
| `zeroize` | workspace | Drop-time zeroization of `AeadKey` and `ChainKey` |

There is **no `[dev-dependencies]` section**: the crate has no
development-only dependencies at all. `tests/handshake.rs` reaches its
`RouterIdentityBundle` fixtures through `i2pr-crypto` and `i2pr-proto`, which
are already production dependencies.

`scripts/check-dependency-direction.sh:18-20` pins the allowlist to exactly
`{"i2pr-crypto", "i2pr-proto", "i2pr-transport"}`, which matches the manifest
and the observed source usage. The checker also asserts the crate does **not**
depend on `i2pr-runtime`, `i2pr-daemon`, or `i2pr-testkit`, and
`i2pr-runtime` is the only production consumer allowed to reach in
(`scripts/check-dependency-direction.sh:51-53`).

### Where the ciphers live (no clean split)

`i2pr-crypto` has a `pub mod chacha` (`crates/i2pr-crypto/src/lib.rs:49`),
but it is a **bare ChaCha20 stream layer** (`chacha20_xor_layer`,
`chacha20_xor_layer_owned`, `LayerCipherKey`, `CHACHA20_BLOCK_LENGTH = 64`) —
no Poly1305, no AEAD. **This crate does not use it.** NTCP2 goes straight to
the `chacha20poly1305` crate from `crypto.rs:13` and owns the nonce counter,
the forbidden-nonce check, and the AAD policy itself, because those are
NTCP2-specific protocol requirements. So the division is:

- `i2pr-crypto::chacha` — ChaCha20 *stream* layer for the other protocols
  that need raw keystream.
- `i2pr-crypto` (X25519, SHA-256, constant-time compare, red25519) — shared
  primitives NTCP2 consumes.
- `i2pr-transport-ntcp2::crypto` — all NTCP2-specific AEAD, transcript, and
  KDF composition, deliberately not hidden behind a generic provider API
  (ADR 0011).

### Runtime-neutrality (verified, not assumed)

`scripts/check-runtime-boundaries.sh` greps this crate's `src/` for
`tokio::`, `std::net`, `std::fs`, `TcpStream`, `TcpListener`, `UdpSocket`,
`UnixStream`, `OpenOptions`, `File::`, for `async fn` / `async_trait`, and for
`i2pr-(netdb|tunnel|client)`; it also rejects any `tokio` line in a non-runtime
manifest. Running it on this revision: **`runtime boundary checks passed`**
(exit 0). `Cargo.toml` has no `tokio` dependency.

One honest nuance: `address.rs:9-13` imports
`std::{fmt, net::{IpAddr, SocketAddr}, str::FromStr}`. These are **pure data
carriers** — the crate formats, compares, and returns them; it never calls
`connect`, `bind`, `to_socket_addrs`, or any socket API. Because the import is
inside a grouped `use std::{…}`, the checker's literal `std::net` grep does not
match it, so the automated pass is not evidence about that import; the
substantive claim (no socket operations) is verified by reading `address.rs`
in full.

## Tests

`cargo test --locked -p i2pr-transport-ntcp2 --all-targets` →
**31 unit + 4 integration = 35 passing, 0 failed, 0 ignored.**

### Inline unit tests (31)

| Module | Count | Coverage |
| --- | --- | --- |
| `crypto.rs:827-1047` | 6 | nonce boundary at `MAX_NONCE`; AES chain round-trip over two ephemeral fields; full initiator/responder cross-match to the committed final transcript hash; wrong-key + Debug redaction; AEAD tag mutation; every committed primitive vector row |
| `handshake.rs:1011-1160` | 6 | options encode/decode exactness and reserved-byte rejection; truncation and padding preservation; limit accepted / limit+1 rejected; `ConfirmedPayload` ordered-block strictness; replay + clock-skew boundary determinism; RouterInfo static-key base64 decode without exposing bytes |
| `block.rs:899-1105` | 6 | canonical round-trip with bounded unknown bytes; repeated non-padding + late termination accepted; encoder accepts repeats; malformed order / duplicates / trailing headers rejected; termination typed without retaining additional text; both committed block fixtures consumed |
| `address.rs:924-1160` | 9 | complete IPv4/IPv6 material; structural address with cost/expiration; static-key-only address is neither listen nor dial; configured vs resolved types distinct and runtime-neutral; duplicate/conflicting/unknown options; invalid hosts/ports/material/encodings; non-I2P alphabet, non-canonical padding, bad versions; invalid caps and endpoint mismatch; Debug redaction |
| `frame.rs:566-632` | 3 | length obfuscation + typed truncation on a partial prefix; authenticated round-trip against the committed frame fixture with terminal tag-mutation failure; bounded deterministic block assembly |
| `state_machine.rs:1270-1298` | 1 | `responder_phase_label()` is the bounded `need_request` identifier and the label set is closed and redacted |

### Integration test (`tests/handshake.rs`, 346 lines, 4 tests)

`deterministic_initiator_and_responder_complete_with_matching_data_keys`,
`session_confirmed_decode_is_exact_at_every_partial_boundary`,
`cancellation_deadline_and_disconnect_are_terminal_actions`,
`router_info_signature_and_transport_key_binding_fail_closed`. Together they
drive both machines through every transition and assert the split keys agree.

### Committed-vector-backed tests

Fixtures live in the repo-level corpus `tests/fixtures/ntcp2/crypto/`, gated by
`scripts/check-ntcp2-vectors.sh`, which validates manifest completeness, ID
and path uniqueness, category (`positive` / `malformed`), provenance, and the
sha256 of every file — plus a reverse check that no file in the corpus is
unlisted — and requires 13 named rows in `vectors.tsv`.

| Fixture | Manifest ID | Consumed by |
| --- | --- | --- |
| `vectors.tsv` | `crypto-vectors` | `crypto.rs:807` (`include_str!`) — 13 rows: `x25519-alice-public`, `x25519-bob-public`, `x25519-shared`, `protocol-name-hash`, `transcript-initial-hash`, `session-request-aead`, `session-created-aead`, `session-confirmed-static-aead`, `session-confirmed-payload-aead`, `transcript-final-hash`, `chacha20poly1305-seal`, `aes-cbc-ephemeral`, `split-kdf` |
| `data-phase-frame.hex` | `data-phase-frame` | `frame.rs:586` — full sealed wire frame, byte-exact |
| `data-phase-blocks.hex` | `data-phase-blocks` | `block.rs:1094` — positive parse, 2 blocks |
| `data-phase-malformed.hex` | `data-phase-malformed` | `block.rs:1099` — negative parse, must be `DuplicateBlock` |
| `storage-static-key.hex` | `storage-static-key` | **not this crate** — `crates/i2pr-storage/src/lib.rs:1407` |
| `manifest.tsv` | — | the checker itself |

All four fixture-consuming tests use `include_str!` at compile time, so a
committed-byte change breaks the build rather than silently passing at runtime.

### Bounded negative and malformed coverage

- Nonce: `MAX_NONCE` emitted, next call `NonceExhausted`; the forbidden
  `2^64 - 1` is unreachable.
- Public key: all-zero encoding rejected as `InvalidPublicKey`.
- AEAD: one flipped ciphertext bit and one flipped AAD/tag byte both yield
  `AuthenticationFailure` / `FrameError::AuthenticationFailure` **and** latch
  the receive owner terminated.
- Lengths: partial 2-byte prefix → `TruncatedLength`; a wire frame whose
  ciphertext length disagrees with the deobfuscated length →
  `TruncatedCiphertext`; bounds tested at limit and limit+1.
- Padding: over-long request/created padding → `ExcessivePadding`; a padding
  length mismatch → `ExcessivePadding` (too many) or `InvalidFixedLength`
  (too few).
- Blocks: duplicate padding, padding-not-final, termination-not-final,
  duplicate termination, truncated header, trailing 1–2 byte header, and a
  termination body outside `9..=265`.
- Handshake: wrong role, wrong stage, wrong network id, stale and future peer
  timestamps, replay, replay cache full/unavailable, RouterInfo signature
  failure, unsupported peer key type, static-key binding mismatch, and
  cancellation / deadline / disconnect.
- Address: duplicate/conflicting/unknown options, reserved flag bits,
  out-of-range port, endpoint mismatch, non-I2P base64 alphabet,
  non-canonical padding, and bad version.

### Determinism

Every test is single-threaded, clock-free, and byte-exact. Timestamps are
injected as literal `u64` values through `HandshakeInput::Timestamp`; no test
reads a wall clock, sleeps, or opens a socket. The replay reference
(`ReferenceReplayCache`) and `ClockSkewPolicy` are pure functions of their
inputs. Run with `--test-threads=1` per the repo routine.

## Distinctive design choices

1. **The runtime supplies the clock.** No state machine calls `Instant` or
   `SystemTime`; it emits `RequestTimestamp { purpose }` and receives a
   `u64`, which is what makes every handshake test fully deterministic.
2. **Cancellation is a transition, not an error.** `Cancelled`,
   `DeadlineExpired`, and `Disconnected` return a normal
   `HandshakeTransition` into `Done` carrying `Terminate(..)`, so the runtime
   always observes a typed terminal action instead of an `Err` it might drop.
3. **Unchecked static read is explicitly unauthenticated.**
   `decrypt_static_unchecked` is documented in-source as requiring the caller
   to bind the returned key against the validated RouterInfo before
   treating the handshake as authenticated — the split exists only because the
   XK responder needs Alice's static key to compute `se`.
4. **Post-XOR length validation.** `deobfuscate_length` validates the clear
   length after XOR, not the wire value, so a valid frame whose obfuscated
   prefix lands below 16 is not rejected — the mask stays free of a length
   oracle.
5. **The forbidden nonce cannot be produced.** The counter checks
   `> MAX_NONCE` before emitting and before incrementing, so `u64::MAX` is
   never handed to ChaCha20-Poly1305 and the counter cannot wrap.
6. **Bespoke two-block AES chain, not a library CBC mode.** The router hash is
   a *public* AES key, so this is obfuscation of the static-looking ephemeral
   field only; the actual secrecy is the X25519 DH underneath.
7. **Unknown blocks are authenticated and skipped, not rejected**, under a
   4 KiB aggregate budget — forward compatibility under a real AEAD tag.
8. **Consuming APIs throughout.** `Transcript`, `InitiatorState`, and
   `ResponderState` all take `self` and return a new instance, so an illegal
   ordering is unrepresentable and each stage check is a belt-and-braces
   assertion rather than the only guard.
9. **Redacted diagnostics by construction.** `HandshakeBytes`, `EncodedFrame`,
   `TransmitState`, `ReceiveState`, and `PublicKeyBytes` all implement `Debug`
   by hand and emit lengths or `Done` labels, never bytes — protocol bytes
   cannot leak into a log through a derived formatter.
10. **A closed, non-peer-derived phase label set.** `phase_label()` lets the
    runtime classify a terminal responder-stage failure without exposing any
    protocol byte, and the set is asserted closed by test.

## Cross-references

Architecture deep dives (relative, all verified to resolve):

- [Overview](overview.md)
- [Dependency graph](dependency-graph.md) — mirrors
  `scripts/check-dependency-direction.sh`
- [Tooling](tooling.md) — the scripts and lanes
- [i2pr-transport](i2pr-transport.md) — `AddressFamily`,
  `EncodedI2npMessage`, `MAX_I2NP_MESSAGE_BYTES`, link/manager contracts
- [i2pr-crypto](i2pr-crypto.md) — `X25519PrivateKey`, `X25519SharedSecret`,
  `sha256`, `constant_time_eq`, and the separate `chacha` stream layer
- [i2pr-proto](i2pr-proto.md) — `RouterInfo`, `RouterAddress`, `Mapping`,
  `Hash`, `Date`, `CryptoKeyType`
- [i2pr-runtime](i2pr-runtime.md) — owns `Ntcp2RuntimeService`, the only
  production consumer that fulfills `HandshakeAction`
- [i2pr-storage](i2pr-storage.md) — the separate versioned NTCP2 static-key
  record (ADR 0011)
- [i2pr-daemon](i2pr-daemon.md) — composition root; NTCP2 defaults to
  disabled
- [i2pr-testkit](i2pr-testkit.md) — `Ntcp2DataPhaseDriver` and milestone
  trajectories
- [interop-apparatus](interop-apparatus.md) — the **closed, read-only**
  historical NTCP2 interoperability lane

ADRs (`docs/adr/`):

- [ADR 0010](../adr/0010-transport-contracts-and-crate-boundaries.md) — transport
  contracts and crate boundaries
- [ADR 0011](../adr/0011-ntcp2-crypto-and-static-key-storage.md) — NTCP2 crypto
  composition and static-key persistence
- [ADR 0012](../adr/0012-ntcp2-handshake-state-machines.md) — bounded handshake
  state machines
- [ADR 0013](../adr/0013-ntcp2-data-phase-and-blocks.md) — data-phase frames and
  blocks (empty AAD)
- [ADR 0014](../adr/0014-ntcp2-runtime-link-manager-and-address-policy.md) —
  runtime link manager and address policy

Plan-of-record and closure records:

- `plans/subsystems/ntcp2-transport-roadmap.md`
- `plans/closure/ntcp2-transport/032-closure.md` (crypto + static-key storage),
  `033-closure.md` (handshake), `034-closure.md` (data phase and blocks),
  `035-closure.md` (runtime link manager)
- `plans/closure/ntcp2-transport/099-status.md` — the retained
  `protocol-defect-localized` at `noise_authenticated` result
- `plans/implementation/ntcp2-transport/100-plan099-exit-gate-cleanup-and-router-handoff.md`,
  `101-daemon-ntcp2-activation-safety-and-router-handoff-correction.md` —
  Plan 101 is the daemon-activation guard
- `plans/registry.md:28` — "Plans 030–101 exited (defect localized, daemon
  NTCP2 disabled); new NTCP2 work needs a new plan-of-record"
- `specs/support.toml` — every NTCP2 surface is `status = "experimental"`,
  `advertised = false`

Historical interoperability lane: `tests/integration/ntcp2/` (manifest
`tests/integration/ntcp2/manifest.toml`, retained read-only; historical
apparatus enforced by `scripts/check-ntcp2-interoperability.sh`). Fixture
corpus: `tests/fixtures/ntcp2/crypto/`, integrity enforced by
`scripts/check-ntcp2-vectors.sh`.
