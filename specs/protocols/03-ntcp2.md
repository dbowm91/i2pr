# NTCP2 transport

Status: **required**  
Primary roadmap milestone: **3**  
Role: first interoperable router-to-router transport

## Scope

NTCP2 provides authenticated point-to-point transport of complete I2NP messages over TCP. It is not a general-purpose byte stream for applications. The protocol combines a Noise XK handshake with I2P-specific key obfuscation, padding, framing, payload blocks, replay/skew checks and RouterInfo address fields.

## Authoritative sources

- [NTCP2 specification](https://i2p.net/en/docs/specs/ntcp2/), pinned in [SOURCES.md](../SOURCES.md), updated 2026-03 and accurate for 0.9.69.
- [Proposal 111](https://github.com/i2p/i2p.website/blob/88596022920bdf99f27db27688faf4f204792fcd/static/proposals/111-ntcp-2.txt) for design and migration background.
- [Common structures](https://i2p.net/en/docs/specs/common-structures/) for RouterInfo/address options and keys.
- Noise Protocol Framework revision identified by the official specification.
- RFC 7748 and the ChaCha20-Poly1305/SipHash references named by the official specification.

The official protocol is based on `Noise_XK_25519_ChaChaPoly_SHA256` with I2P-specific transcript/KDF identifiers and framing extensions. A generic Noise XK library is not sufficient without implementing those exact extensions.

## Plan 032 foundation evidence

Plan 032 implements only the non-I/O cryptographic composition needed by the
later handshake plan. The selected protocol name is
`Noise_XKaesobfse+hs2+hs3_25519_ChaChaPoly_SHA256`, with an empty prologue,
SHA-256 transcript hashing, HMAC-SHA256 KDF steps, ChaCha20-Poly1305 cipher
states, AES-256-CBC ephemeral-key obfuscation, and SipHash-2-4 length material.
The source-linked constants are centralized in
`crates/i2pr-transport-ntcp2/src/constants.rs`; dependency and Noise choices
are recorded in [ADR 0011](../../docs/adr/0011-ntcp2-crypto-and-static-key-storage.md).

`crates/i2pr-transport-ntcp2/src/crypto.rs` provides consuming role-aware
stages for SessionRequest, SessionCreated, and SessionConfirmed. It does not
parse complete messages, RouterInfo, options, padding policy, frames, blocks,
timestamps, sockets, or runtime events. SessionConfirmed part one deliberately
uses the retained SessionRequest cipher state at nonce 1 before the `se` KDF.

`i2pr-storage::TransportStaticKeyStore` persists the independent X25519 static
key and published IV at `ntcp2.static.key` in the private router data
directory. This record is separate from `router.identity`, strict and
checksummed, create-only, permission-hardened, and not a publication API.

Deterministic evidence is under `tests/fixtures/ntcp2/crypto/` and is checked
by `scripts/check-ntcp2-vectors.sh`. The corpus includes independent
Python-cryptography primitive/transcript-composition values and synthetic
storage bytes. It is local experimental evidence only; it is not Java I2P or
i2pd interoperability evidence and does not authorize capability advertisement.

## Plan 033 handshake implementation boundary

The runtime-neutral Plan 033 layer implements the three establishment messages
using the 0.9.69 layouts above:

| Message | Exact wire regions | Local bounds and checks |
| --- | --- | --- |
| SessionRequest | 32-byte AES-obfuscated X, 32-byte AEAD options frame, cleartext padding | 64-byte minimum; 65535-byte wire maximum; 880-byte non-PQ padding maximum; options version/reserved bytes, timestamp, network ID, and negotiated message-3-part-2 length are checked |
| SessionCreated | 32-byte AES-obfuscated Y using the continued message-1 AES state, 32-byte AEAD options frame, cleartext padding | 64-byte minimum; 65535-byte wire maximum; 848-byte non-PQ padding maximum; all reserved bytes are zero and timestamp/padding lengths agree |
| SessionConfirmed | fixed 48-byte encrypted Alice static frame, negotiated encrypted part-two frame | total is at most 65535 bytes; part two is 16..65487 bytes including its tag; plaintext blocks are strictly RouterInfo, optional Options, optional Padding |

The codecs reject truncation, impossible lengths, excessive padding, unknown
blocks, duplicate optional blocks, and malformed trailing bytes before
allocating peer-controlled regions. Message-1/2 trailing regions are admitted
only as the cleartext padding whose authenticated option length was declared;
the consuming state machines request bounded reads,
writes, timestamp, replay, padding, and local RouterInfo through typed actions;
they do not own those effects. RouterInfo is decoded and signature-verified,
then its NTCP/NTCP2 version-2 `s` option is compared with the authenticated
X25519 static key before an authenticated result is emitted.

For local compatibility evidence, the initial skew policy is ±60 seconds and
the replay retention is at least twice that window. Replay tokens are SHA-256
digests of the encrypted ephemeral field, and replay admission is fail-closed
for replay, cache-full, or unavailable decisions. The specification leaves
production padding distribution/negotiation and the age of an older NetDB
RouterInfo open; those choices remain deferred and are not capability claims.

## Plan 034 data-phase implementation boundary

The runtime-neutral implementation follows the deployed data-phase layout:

| Surface | Wire/limit | Multiplicity and ordering | Output/policy boundary |
| --- | --- | --- | --- |
| Frame | 2-byte obfuscated length; clear ciphertext 16..=65,535 bytes; ChaCha20-Poly1305 tag 16 bytes; plaintext at most 65,519 bytes | one length per complete frame; no allocation before clear validation | `TransmitState`/`ReceiveState`; empty associated data |
| Timestamp (0) | exactly 4 bytes, unsigned Unix seconds | at most one; ordinary data ordering | typed timestamp; injected clock/skew policy remains outside |
| Options (1) | at least 12, at most 4,096 bytes; fixed u8/u16 fields plus bounded extensions | at most one | typed bounded options, no string policy |
| RouterInfo (2) | flags plus verified uncompressed RouterInfo; data-phase encoded bytes at most 65,515 | at most one; no NetDB mutation | signed/key-bound update candidate |
| I2NP (3) | complete 9-byte NTCP2 short header plus bounded body; no fragmentation | multiple allowed | consuming outbound owner; borrowed authenticated inbound view |
| Termination (4) | 8-byte valid-frame count, 1-byte reason, at most 256 additional bytes | at most one; terminal and not combined with application/control blocks | bounded reason enum; no remote text retention |
| Padding (254) | 0..=65,516 bytes | at most one and last | length-only observation |
| Unknown | 3-byte header plus bounded body | skipped only after AEAD; max 256 blocks and 4,096 unknown bytes | treated as bounded padding, never policy |

The parser rejects truncated headers/bodies, invalid fixed lengths, duplicate
control blocks, padding or termination order violations, oversized fields,
malformed RouterInfo/signatures, and trailing plaintext. AEAD authentication
always precedes block parsing. `i2pr-transport-ntcp2` does not own sockets,
runtime queues, coalescing waits, deadlines, cancellation, NetDB mutation,
RouterInfo publication, or capability advertisement.

The current specification has no in-session periodic data-phase rekey
threshold. Plan 034 therefore makes the last permitted nonce/counter terminal
and requires a fresh Noise handshake for rekey or static-key/IV rotation. A
future plan may add a compatibility-approved rekey protocol only after its
wire behavior is specified and independently evidenced.

## Required MVP behavior

### RouterInfo and key material

- Parse and validate NTCP2 RouterAddress options, including host/port, static key and initialization-vector material.
- Generate persistent transport static keys independently from ephemeral handshake keys.
- Publish only addresses actually reachable under the router’s current policy.
- Keep address observation and RouterInfo mutation outside the transport codec/state machine.

### Handshake

Implement explicit initiator and responder states for:

- SessionRequest;
- SessionCreated;
- SessionConfirmed;
- transition to independent transmit/receive data cipher states.

The implementation must exactly reproduce I2P transcript hashing, KDF labels, ephemeral-key obfuscation, cleartext and encrypted padding rules, option fields, timestamp/skew validation, RouterInfo transmission/validation and authentication failure behavior.

### Data phase

Implement:

- obfuscated two-byte frame lengths;
- authenticated payload frames and nonce/counter progression;
- all required block types for I2NP delivery, RouterInfo, timestamp, padding, termination and options as defined by the current specification;
- bounded unknown-block skipping only when permitted;
- coalescing and padding without unbounded delay or allocation;
- orderly termination and abrupt failure cleanup;
- rekeying behavior exactly as specified.

The maximum wire message and decoded block limits must be constants derived from the specification and reconciled with deployed-router behavior. A valid maximum must not imply that every peer receives that allocation eagerly.

### Link management

The transport-neutral manager must own dial policy, duplicate-link resolution, replacement, per-peer connection limits, retry/backoff and outbound queue limits. NTCP2 owns protocol state and authenticated delivery, not peer scoring, NetDB mutation or tunnel selection.

## Security requirements

- Bound simultaneous incoming handshakes before expensive cryptographic operations.
- Apply read, write, handshake and idle deadlines.
- Reject replayed or implausibly skewed handshakes according to documented policy.
- Validate the authenticated peer RouterIdentity against the expected RouterInfo/target for outbound sessions.
- Handle partial TCP reads/writes without assuming frame alignment.
- Prevent attacker-selected frame lengths from causing large allocations.
- Use constant-time library verification for tags and keys where provided.
- Treat all-zero/invalid X25519 results according to the selected crypto library and specification.
- Release buffers, queue permits, socket tasks and key material on every failure/cancellation path.
- Avoid detailed remote error responses that become a probing oracle.

## Implementation references

- Java I2P: `router/java/src/net/i2p/router/transport/ntcp`, especially `NTCP2Options`, `NTCP2Payload`, inbound/outbound establishment state, `NTCPConnection` and `NTCPTransport`.
- I2P+: the matching package, including `OutboundNTCP2State`; inspect recent hardening and scheduling differences.
- i2pd: `libi2pd/NTCP2.h` and `libi2pd/NTCP2.cpp`.
- Emissary/go-i2p: `lib/transport/ntcp2` and `lib/router/router_ntcp2.go`.

Compare transcript/KDF constants, maximum padding, timestamp policy, frame/block parsing, RouterInfo handling, duplicate sessions, termination reasons and rekey thresholds. Generate differential vectors from fixed keys rather than relying only on live handshakes.

## Required tests

- Official and independently generated handshake vectors for every message and KDF stage.
- Initiator and responder interoperability with Java I2P and i2pd in a controlled testnet.
- Optional third-party validation against I2P+ and Emissary/go-i2p.
- One-bit mutation tests for obfuscated keys, options, transcript inputs and authentication tags.
- Replay, stale/future timestamp and wrong-network/identity tests.
- Minimum, maximum and excessive padding.
- Partial reads/writes at every field/frame boundary.
- Zero-length, maximum-length and oversized frame declarations.
- Unknown, duplicate and invalid block sequences.
- Rekey/counter-boundary tests.
- Slowloris, stalled write, queue saturation and cancellation tests.
- Duplicate simultaneous inbound/outbound connection resolution.
- Fuzzing of authenticated plaintext block parsing and deterministic handshake state transitions.

## Plan 035 runtime boundary

Plan 035 adds only a runtime adapter around the pure handshake and data-phase
surfaces above. `i2pr-runtime` owns TCP sockets, partial reads/writes,
deadlines, cancellation, replay retention, admission, bounded queues, and
joined reader/writer children. `i2pr-transport-ntcp2` remains free of Tokio,
DNS, filesystem, sockets, RouterInfo mutation, and publication policy.
The checked-in Plan 035 implementation is the controlled ownership subset: it
provides exact-I/O helpers, listener/dial lifecycle, admission, replay,
backoff, and joined raw-link children, but does not claim complete wire-level
handshake or authenticated data-phase execution. That composition is deferred
to Plan 036.

The adapter validates NTCP2 RouterAddress literals before dialing: `host` is a
literal IPv4/IPv6 address, `port` is decimal 1..=65535, the static key is an
exact 32-byte value, and the obfuscation IV is an exact 16-byte value. Duplicate
or conflicting fields are rejected and unsupported fields are not silently
interpreted. Configured addresses and resolved socket targets are distinct
types. Observed endpoints produce bounded family/reachability observations only;
they do not infer an external address or update RouterInfo/NetDB.

The controlled runtime policy uses global, per-IP, IPv4 `/24`, and IPv6 `/64`
pending-handshake limits before expensive cryptography. Replay capacity fails
closed, expiry ordering is deterministic, and dial retry/backoff records are
bounded and cancellable. The default duplicate rule is a deterministic local /
remote hash direction rule with bounded drain and stale-close protection; mixed
Java I2P/i2pd evidence remains a Plan 036 prerequisite. Loopback tests are
local lifecycle evidence only and do not advance this dossier's support claim.

## Plan 036 evidence boundary

Plan 036 supplies the controlled-lane manifest at
`tests/integration/ntcp2/manifest.toml`, pinned to Java I2P 2.12.0 revision
`2800040` and i2pd 2.60.0 revision `f618e41`, plus a repository-side
preflight and sanitized evidence format. The lane requires a synthetic
private network, disabled reseed/bootstrap, disposable identities and static
keys, fixed clocks, explicit bounds, and teardown before any result is
retained. The preflight does not run routers or claim a result.

This checkout has not executed the required Java I2P and i2pd runs: the daemon
still keeps live activation disabled and the complete wire-level composition of
the pure handshake/data owners with the runtime socket owner is not present.
That is an explicit Plan 036 blocker. The 0..255 deterministic testkit matrix,
pure parser/state tests, and fuzz campaigns are local evidence only; they do
not advance `specs/support.toml` or capability advertisement.

## Deferred and excluded behavior

- NTCP1 compatibility: legacy-reject; the MVP explicitly excludes NTCP1.
- Shared-port NTCP1/NTCP2 detection: deferred unless current deployment requires it despite NTCP1 exclusion.
- Pluggable transports or experimental NTCP2 variants: deferred.
- Automatic address discovery/NAT mapping: outside the codec; addressed by reachability policy and SSU2 work.
- Hybrid/PQ NTCP2 variants: compatibility watch pending official deployment requirements.

## Open decisions before implementation

1. Reviewed Rust crates for X25519, ChaCha20-Poly1305, SHA-256/HMAC/HKDF, AES obfuscation and SipHash behavior.
2. Maximum concurrent incoming/outgoing handshakes and per-IP/per-subnet admission policy.
3. Exact clock-skew window and whether repeated skew failures affect peer profile/backoff.
4. Duplicate-link winner rules that interoperate without churn while remaining transport-neutral.
5. Buffer ownership and zero-copy boundaries between TCP framing, decrypted blocks and I2NP dispatch.
6. Padding policy that remains compliant without creating a stable `i2pr` fingerprint.
7. Whether the first milestone publishes IPv4 only or includes IPv6 address/listener conformance immediately.
