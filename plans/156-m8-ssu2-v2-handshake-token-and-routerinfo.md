# Plan 156 — Milestone 8 SSU2 v2 handshake, token, and RouterInfo establishment

Status: **registered; execute after Plan 155 passes**.

Depends on Plan 155. Blocks Plan 157.

## 1. Goal

Implement the complete runtime-neutral SSU2 v2 establishment protocol: cryptographic transcript, SessionRequest/SessionCreated/SessionConfirmed, TokenRequest/Retry, source-bound token lifecycle, RouterInfo establishment, replay/timeout/retransmit transitions, and authenticated session-key output.

No UDP sockets are opened in this pass.

## 2. Cryptographic transcript

Implement the exact SSU2 v2 Noise pattern and protocol identifier from the refreshed official specification. The current baseline uses the SSU2-specific Noise XK construction over X25519 / ChaCha20-Poly1305 / SHA-256 with SSU2 header/ephemeral obfuscation rules.

Requirements:

- consuming/explicit state-machine API similar in spirit to NTCP2;
- initiator/responder roles cannot accidentally call the other's transition;
- transcript hash/chaining key stages explicit;
- responder static key binding explicit;
- all handshake headers included as associated data where specified;
- ChaCha20 header/ephemeral obfuscation/protection implemented exactly, not replaced with a generic packet-protection scheme;
- nonce/counter boundaries checked before overflow;
- secret keys zeroized/redacted and not casually Clone-able;
- no home-grown X25519/ChaChaPoly/SHA implementation.

Add fixed KDF/transcript/header-protection vectors with independent derivation/provenance.

## 3. Handshake message codecs and states

Implement exact v2 message processing for:

```text
TokenRequest
Retry
SessionRequest
SessionCreated
SessionConfirmed
```

Use explicit initiator and responder states. A suggested conceptual flow:

```text
initiator:
  NeedTokenOrRequest
  AwaitRetryOrCreated
  NeedSessionRequest
  AwaitCreated
  NeedConfirmed
  Established / Failed

responder:
  AwaitTokenOrRequest
  NeedRetry | NeedCreated
  AwaitConfirmed
  Established / Failed
```

Use the actual protocol transitions rather than forcing this exact enum shape if the spec requires more states.

Every transition must produce bounded actions such as:

```text
WriteDatagram(bytes)
ArmDeadline(kind, at)
ValidateToken(...)
ValidateRouterInfo(...)
Established(keys, authenticated_peer, negotiated_parameters)
Terminate(reason)
DropSilently(category)
```

The protocol crate must not sleep, open sockets, spawn tasks, or read wall-clock time directly.

## 4. Cheap prevalidation before expensive work

Before avoidable DH/AEAD/session allocation, reject or silently drop as appropriate:

- impossible datagram lengths;
- unsupported protocol version;
- wrong network ID;
- structurally impossible message type/header;
- connection-ID mismatch when determinable;
- invalid/missing Retry token when the current path requires validation.

Tests must prove a flood of syntactically cheap invalid requests does not allocate unbounded handshake/session state or invoke expensive crypto through the deterministic driver.

Do not reveal detailed authentication failure information to an unauthenticated source.

## 5. Token lifecycle

Implement the current SSU2 token semantics rather than inventing a stateless cookie design.

Baseline requirements:

- token value is exactly the current spec-defined size (classical v2: 8 random bytes if unchanged by refreshed source);
- OS CSPRNG at runtime; deterministic generator injection only in tests;
- token bound to the source endpoint/address data required by the spec;
- one-use semantics;
- explicit short expiration;
- bounded token table/cache;
- bounded tokens per source/subnet and globally;
- key/generator restart semantics explicitly tested;
- consumed/expired tokens removed and resource accounting released;
- duplicate/replayed token use fails closed;
- wrong-source/wrong-port token use fails closed;
- Retry response amplification bounds enforced by the action/state layer.

If the refreshed v2 specification explicitly prohibits stateless opaque HMAC-style tokens because replay/reuse cannot be tracked, **do not implement stateless tokens** merely for convenience.

Token state can live in a runtime-neutral bounded owner driven by caller-supplied time/randomness. Production OS randomness/time fulfillment belongs to runtime in Plan 158.

## 6. Retry / anti-amplification path

Prove both establishment paths in deterministic tests:

```text
no valid token:
  TokenRequest/SessionRequest as specified
  -> Retry
  -> valid token-bearing SessionRequest
  -> SessionCreated
  -> SessionConfirmed
  -> Established

valid cached token:
  SessionRequest
  -> SessionCreated
  -> SessionConfirmed
  -> Established
```

Before source validation, emitted response bytes must stay within the specification's amplification limits.

Track Retry/request retransmission state centrally with bounded counters; do not create a timer object per datagram.

## 7. RouterInfo establishment and identity binding

Implement the RouterInfo transfer/reassembly required during SessionConfirmed and validate it deeply before exposing `AuthenticatedPeer`.

At minimum verify:

- structural RouterInfo decode;
- RouterInfo signature;
- peer RouterIdentity hash expected by the dial/accept context where known;
- network ID/version requirements;
- SSU2 RouterAddress presence/compatibility;
- static SSU2 public key `s` matches the authenticated handshake peer;
- intro key `i` shape as required;
- endpoint/address consistency where the spec permits/mandates binding;
- RouterInfo fragment sequence/count/aggregate byte ceilings;
- stale/future publication time policy according to current spec/router policy.

A SessionConfirmed carrying a valid signature but wrong static SSU2 key must fail authentication.

Do not mutate NetDB inside the handshake codec. Return validated authenticated material to the caller.

## 8. Handshake retransmission/replay/timeout

Implement runtime-neutral handshake recovery state:

- bounded attempt counts;
- deterministic exponential or spec-defined resend schedule with ceiling;
- duplicate SessionRequest/Created/Confirmed handling according to v2 semantics;
- replay cache/token for establishment messages where required;
- explicit handshake deadline;
- cancellation/disconnect terminal actions;
- state release on all failure paths.

Do not make the state machine depend on `tokio::time`.

## 9. Session output

Successful establishment should yield one narrow owned structure containing only what Plan 157 needs, e.g.:

```text
AuthenticatedSsu2Session
  authenticated peer identity/hash
  directional data-phase keys/material
  connection IDs
  negotiated endpoint/family/MTU constraints
  packet-number initial state
  peer RouterInfo handoff
```

Avoid exposing raw intermediate handshake secrets after split/establishment.

## 10. Required tests

### Vectors

- transcript initial hash/chaining state;
- SessionRequest protection/unprotection;
- SessionCreated protection/unprotection;
- SessionConfirmed authentication;
- split/directional data keys;
- header/ephemeral ChaCha20 masks;
- tag mutation/wrong static key rejection.

### State-machine trajectories

- full tokenless Retry trajectory;
- full valid-token trajectory;
- initiator/responder independently drive to matching established keys;
- request retry after one dropped datagram;
- created retry/duplicate handling;
- confirmed duplicate handling;
- deadline exhaustion;
- cancellation at every major phase;
- malformed/truncated message at representative boundaries.

### Token matrix

- valid;
- consumed/reuse;
- expired;
- wrong IP;
- wrong port if endpoint-bound;
- unknown token;
- table exact capacity;
- max+1 fails closed;
- per-source quota;
- restart/rotation semantics;
- deterministic eviction/expiry behavior.

### RouterInfo matrix

- valid single/fragmented transfer;
- missing fragment;
- duplicate/conflicting fragment;
- aggregate max/max+1;
- bad signature;
- wrong peer identity;
- wrong SSU2 static key;
- unsupported SSU2 version;
- malformed address material.

## 11. Likely files

```text
crates/i2pr-transport-ssu2/src/crypto.rs
crates/i2pr-transport-ssu2/src/handshake.rs
crates/i2pr-transport-ssu2/src/token.rs
crates/i2pr-transport-ssu2/src/state_machine.rs
crates/i2pr-transport-ssu2/tests/handshake.rs
tests/fixtures/ssu2/*
docs/architecture/i2pr-transport-ssu2.md
plans/156-status.md
```

Reuse existing crypto wrappers where they provide exact protocol primitives. Add small reviewed wrappers to `i2pr-crypto` only when they represent genuinely shared protocol-safe functionality; do not move SSU2 transcript policy into the generic crypto crate.

## 12. Non-goals

No:

- UDP sockets;
- active transport registration;
- data-phase ACK/loss implementation;
- I2NP fragmentation/reassembly lifecycle;
- path migration;
- peer test/relay;
- public address publication;
- independent-router execution.

## 13. Validation

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-crypto --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh
```

## 14. Acceptance criteria

Plan 156 passes only when:

1. v2 Noise transcript/KDF/header protection is vector-backed.
2. no secret-bearing type leaks through `Debug`/logs.
3. TokenRequest/Retry/SessionRequest/Created/Confirmed codecs are strict and bounded.
4. full initiator/responder tokenless trajectory reaches matching session keys.
5. cached valid-token trajectory reaches matching session keys.
6. invalid/expired/reused/wrong-source tokens fail before avoidable expensive session work.
7. token storage is bounded at exact-capacity/max+1 and cleans up on expiration/consumption.
8. prevalidation has deterministic cheap-drop tests.
9. handshake resend/deadline state is bounded and no timer-per-packet API is introduced.
10. RouterInfo fragments are bounded and reassembled exactly.
11. RouterInfo signature/identity/static-SSU2-key binding is enforced.
12. malformed/tag-mutated/wrong-key handshakes never produce authenticated material.
13. successful output contains only needed post-handshake state; intermediate secrets are released/zeroized.
14. no UDP/socket/Tokio dependency enters the protocol crate.
15. full quality/vector/boundary floor passes.
16. `plans/156-status.md` records evidence and advances only to Plan 157.

## 15. Stop conditions

Stop and write a narrow corrective if:

- official v2 token semantics differ materially from the current dossier assumptions;
- reproducing reference vectors requires an unexplained transcript/KDF divergence;
- RouterInfo establishment requires changing shared RouterInfo semantics in a way that affects NTCP2/M6;
- a required primitive cannot be represented safely with existing reviewed crypto libraries.

Do not proceed to the data phase with a partially understood handshake.