# Plan 034 closure: NTCP2 data phase, frame protection, and blocks

Status: implemented as a runtime-neutral, non-advertised experimental subset.
No socket, Tokio task, NetDB mutation, router publication, capability
advertisement, or public-network test was added.

## State and ownership

```text
TransmitReady --seal--> TransmitReady
      |                    |
      +-- counter failure or termination --> Terminated

ReceiveReady --decode length--> AwaitingCiphertext
      |                                  |
      |                                  +-- authenticate and parse --> ReceiveReady
      |                                  +-- terminal block ----------> Terminated
      +-- malformed length/auth/block -------------------------------> Terminated
```

`TransmitState` owns one directional ChaCha20-Poly1305 state and one
directional SipHash length state. `ReceiveState` owns the corresponding
independent receive states. `SplitKeys::into_parts` and
`AuthenticatedHandshake::into_data_phase` make direction selection consuming;
there is no shared mutable counter or key owner.

## Wire and block layouts

| Item | Layout and bound |
| --- | --- |
| Frame | `u16_be` obfuscated ciphertext length, followed by ciphertext and its 16-byte AEAD tag; clear ciphertext length 16..=65535; plaintext maximum 65519 bytes |
| Length mask | SipHash-derived mask advances once per accepted length; clear length is validated before ciphertext allocation |
| AEAD | ChaCha20-Poly1305 with empty associated data; failed authentication is terminal and never exposes blocks |
| Block header | `type:u8 || length:u16_be || payload`, with payload bounded by the authenticated plaintext |
| Timestamp | type 0, exactly 4-byte rounded Unix seconds |
| Options | type 1, at least 12 bytes, at most 4096 bytes, fixed padding/dummy/delay fields plus bounded extensions |
| RouterInfo | type 2, flags plus verified RouterInfo, at most 65515 bytes; candidate output only, no NetDB mutation |
| I2NP | type 3, bounded complete NTCP2 short-header message, consuming transport owner on transmit and explicit owned handoff on receive |
| Termination | type 4, 8-byte valid-frame count plus one-byte typed reason and at most 256 additional bytes, which are not retained |
| Padding | type 254, at most 65516 bytes, at most once, and only at the end (or after termination) |
| Unknown | authenticated and skipped only within a 256-block and 4096-byte aggregate budget |

Known control blocks are singletons except I2NP. Termination is terminal for
application processing. Padding is last. Duplicate controls, malformed lengths,
trailing bytes, invalid termination shapes, and invalid ordering are typed
errors.

## Buffer and resource ownership

| Owner | Lifetime and release rule |
| --- | --- |
| Obfuscated length | two-byte stack value; deobfuscated before reservation |
| Ciphertext | `ReceiveState`/testkit partial buffer, bounded by the decoded frame length |
| Authenticated plaintext | `AuthenticatedPlaintext`, created only after AEAD and strict block validation |
| Parsed blocks | borrows `AuthenticatedPlaintext`; I2NP remains borrowed until `into_owned` |
| Outbound I2NP | `I2npMessageBlock` consumes `EncodedI2npMessage`; no full-message clone on assembly |
| Outbound frame | `EncodedFrame` owns exactly one bounded length-prefixed wire buffer |
| Partial driver | one-byte stream queues and partial-frame buffer; disconnect clears all buffers and records discarded bytes |

`Ntcp2DataPhaseDriver` provides deterministic one-byte write/read pumping,
multiple-frame stream handling, partial truncation, bounded queue admission,
and cleanup counters. Plan 035 may replace these synchronous queues with
runtime-owned leases and channels without moving runtime ownership into this
crate.

## Support matrix

| Surface | Evidence | Claim |
| --- | --- | --- |
| Directional frame states | `crates/i2pr-transport-ntcp2/src/frame.rs`, state-machine handoff tests | Local structural evidence only |
| Canonical blocks | `src/block.rs`, positive/malformed fixtures, strict parser tests | Local structural evidence only |
| I2NP handoff | consuming `I2npMessageBlock`, borrowed receive view, owned conversion | Bounded handoff; body semantics remain outside this plan |
| RouterInfo binding | existing strict decode/signature verification plus peer hash/static-key validation | Candidate validation only; no publication |
| Partial I/O and cleanup | `crates/i2pr-testkit/src/ntcp2.rs` | Deterministic testkit evidence only |
| Fuzzing | `ntcp2_blocks` and `ntcp2_frames` targets, existing corpus smoke lane | Compilation and bounded local smoke evidence; no network testing |

Both `ntcp2.data-phase-frames` and `ntcp2.data-phase-blocks` are recorded in
`specs/support.toml` as `experimental` and `advertised = false`. The same
non-claim is recorded in `docs/protocol-support.md`.

## Nonce and rekey policy

Frame and SipHash counters advance once per successfully accepted frame. The
forbidden nonce value `2^64 - 1` is never emitted; exhaustion is terminal and
requires a fresh handshake. The current pinned NTCP2 data-phase specification
defines counter progression but does not define an in-session periodic rekey
threshold or rekey derivation. Therefore this plan does not invent one. This
is the explicit stop-condition resolution recorded in ADR 0013; Plan 035 must
retain the terminal behavior unless a pinned specification revision supplies
the missing rule. See the [NTCP2 specification](https://beta.i2p.net/en/docs/specs/ntcp2/)
and `docs/adr/0013-ntcp2-data-phase-and-blocks.md`.

## Vector provenance

`data-phase-blocks.hex` and `data-phase-malformed.hex` are locally authored
synthetic plaintext fixtures. `data-phase-frame.hex` is a locally authored
fixed-key frame fixture using test keys `0x11` and `0x22`; it is consumed by a
Rust test and is not an operational capture. Existing Plan 032 vectors retain
their independent Python provenance. `manifest.tsv` and
`check-ntcp2-vectors.sh` cover all committed files. No Java I2P/i2pd frame
capture was imported, so these fixtures are not interoperability evidence.

## Evidence

Focused and workspace validation completed for the implementation:

- formatting, workspace check, workspace tests, workspace clippy, and docs;
- `i2pr-transport`, `i2pr-transport-ntcp2`, and `i2pr-testkit` tests;
- dependency-direction and runtime-boundary scripts;
- fixture manifest and NTCP2 vector checks;
- Rust 1.85 MSRV check, nightly fuzz-workspace compilation, cargo-deny checks;
- deterministic fuzz smoke with both Plan 034 targets;
- `git diff --check` before commit.

The tests cover partial length prefixes, authenticated tag mutation,
malformed/duplicate/unknown/order-bounded blocks, termination behavior,
one-byte stream pumping, bounded backpressure, partial disconnect cleanup, and
terminal state reuse attempts.

## Plan 035 prerequisites

Plan 035 may add runtime-owned queue/resource leases, socket adapters, timing
and coalescing policy, cancellation/deadlines, and actual link lifecycle
integration. It must preserve the consuming directional state owners,
authenticate-before-parse ordering, bounded allocation, privacy-safe
diagnostics, explicit terminal outcomes, and the non-advertised support state.
It must also resolve periodic rekey only from a pinned protocol rule or keep
counter exhaustion terminal; public-network traffic remains out of scope for
negative and fault testing.
