# Plan 126 — Milestone 6 ECIES destination-ratchet corrective foundation

## Status

- **Ready for execution.**
- Source floor: `523d5dcd87f6c04853a016f7b54e3922697ffb2b`.
- Predecessor evidence: Plans 121, 124, 125.
- Successor: `plans/127-m6-destination-session-routing-final-closure.md`.
- This is a **wire/session correction**, not a transport or live-network plan.

## Objective

Replace the current i2pr-internal destination ECIES representation with the current I2P ECIES-X25519-AEAD-Ratchet contract needed by repliable destination traffic.

The pass must produce a bounded, destination-scoped session manager that can execute this real lifecycle using production primitives:

```text
Alice bound New Session -> Bob
Bob New Session Reply -> Alice
Alice Existing Session -> Bob
Bob Existing Session -> Alice
```

No tunnel or NetDB composition is required for Plan 126; Plan 127 will add those layers after the cryptographic/session layer is trustworthy.

## 1. Source-floor defects to correct

### 1.1 Wrong Noise protocol initializer

Current code defines:

```text
Noise_NK_25519_ChaChaPoly_SHA256
```

The destination ECIES specification is based on Noise IK and specifies the I2P KDF initializer:

```text
Noise_IKelg2_25519_ChaChaPoly_SHA256
```

Do not preserve the current initializer for backward compatibility with i2pr-only ciphertext.

### 1.2 New Session wire format is i2pr-specific

The source-floor `NewSessionMessage` serializes approximately:

```text
0xE0
clear static key (32)
Elligator2 representative (32)
ciphertext
```

That is not the I2P bound New Session encrypted-data format.

For the bound/repliable form required by Streaming, the canonical encrypted data is:

```text
Elligator2-encoded Alice ephemeral public key     32 bytes, clear
Alice static public key section                   32 bytes encrypted
Poly1305 MAC for static-key section               16 bytes
payload section                                   variable encrypted
Poly1305 MAC for payload section                  16 bytes
```

There is no i2pr message-type byte inside the encrypted Garlic payload.

### 1.3 Bound static key is wrong ownership

`seal_new_session()` currently creates/returns a generated static secret instead of consuming the local destination's published type-4 X25519 key.

For repliable traffic, Alice's authenticated static key in the New Session must be the X25519 key that Bob can later match against Alice's LeaseSet2.

Required production ownership:

```text
DestinationIdentity / current published LS2 owns Alice static secret/public
ECIES primitive borrows that key for the bound New Session
session manager never invents a replacement destination static key
```

### 1.4 New Session Reply / Existing Session are misclassified

Current code uses `0xE2` as a combined NSR/Existing marker. The actual protocol uses 8-byte ratchet session tags.

Canonical high-level forms:

```text
New Session Reply:
    session tag                         8
    Bob Elligator2 ephemeral key       32
    key-section ciphertext/MAC         16 for zero-length data
    encrypted payload + MAC            payload + 16

Existing Session:
    session tag                         8
    encrypted payload + MAC            payload + 16
```

Classification must be driven by bounded tag/session lookup, not a leading type byte.

### 1.5 Session manager discards or miskeys required state

At the source floor:

- `PendingHandshakeRecord::new()` discards the installed outbound session state supplied by `seal_new_session()`;
- `accept_new_session()` installs state keyed by an ephemeral representative-derived value rather than a bound far-end Destination;
- `accept_new_session_reply()` installs only one side of the resulting session relationship;
- tag look-ahead configuration exists but is not the authoritative classifier for incoming Existing Session messages;
- replay-cache state is not sufficient closure evidence;
- the manager has no complete production method for Bob to issue an NSR using retained New Session context.

### 1.6 Existing Plan 121 test does not prove bidirectional ES

The current deterministic Plan 121 test uses lower-level primitives and checks that Alice cannot decrypt her own outbound Existing Session message. It does not prove:

```text
Alice outbound ES decrypts with Bob inbound state
Bob outbound ES decrypts with Alice inbound state
```

Replace the closure evidence; retain useful component assertions.

## 2. Normative protocol floor

Use:

- `https://i2p.net/en/docs/specs/ecies/` — current destination ratchet specification;
- `https://i2p.net/en/proposals/144-ecies-x25519-aead-ratchet/` — design/background where useful;
- `https://i2p.net/en/docs/specs/common-structures/` — LeaseSet2 / type-4 key ownership.

Important current specification constraints:

- crypto type is 4;
- bound sessions use an IK-shaped handshake;
- the actual I2P initializer is `Noise_IKelg2_25519_ChaChaPoly_SHA256`;
- Alice's static key is encrypted/authenticated inside the New Session;
- repliable traffic should include the static key;
- Bob must bind that static key to Alice's full Destination by finding a LeaseSet whose type-4 key matches it before a ratchet-layer reply is routable;
- NSR is prefixed by a session tag and Bob ephemeral key;
- Existing Session messages are prefixed by session tags;
- session contexts are local-destination-scoped and must never be shared across local destinations.

## 3. Phase A — freeze the exact supported M6 subset

Do not attempt the entire optional ECIES feature set.

Plan 126 MUST support:

```text
bound New Session (reply expected)
New Session Reply
Existing Session
type-4 X25519 LeaseSet2 static keys
DateTime + Garlic Clove + Padding payload blocks already needed by M6
bounded session tags / replay handling
```

Explicitly defer unless already correct and trivial to preserve:

```text
zero-static/unbound New Session
one-time anonymous mode
multicast
PQ hybrid ECIES
MessageNumbers block
Options block
Termination block
full optional DH-ratchet NextKey policy beyond what is required to establish the base NSR/ES pair
```

If an unsupported form is received, fail with a typed protocol error. Do not silently interpret it as the bound form.

Create/update a small reference note such as:

```text
specs/references/ecies-destination-ratchet.md
```

recording the exact supported subset and normative sections.

## 4. Phase B — replace pseudo-wire message types

Primary file:

```text
crates/i2pr-crypto/src/ecies.rs
```

Remove protocol-visible `0xE0` / `0xE2` markers from destination ECIES encrypted data.

Represent the three canonical forms with typed Rust values whose fields correspond to real wire fields. Exact type names are discretionary, but the API should make impossible states difficult.

Suggested shape:

```text
BoundNewSessionMessage
    representative
    encrypted_static_section
    encrypted_payload_section

NewSessionReplyMessage
    tag
    representative
    key_section_mac/ciphertext
    encrypted_payload_section

ExistingSessionMessage
    tag
    encrypted_payload_section
```

The standard I2NP Garlic envelope remains outside this layer and already provides message framing/length.

Required byte-layout tests must assert exact offsets and total lengths from the specification.

## 5. Phase C — implement the exact bound New Session transcript/KDF

Do not adapt the current simplified NK-shaped transcript by renaming constants.

Implement the specification sequence exactly, including:

```text
Noise_IKelg2_25519_ChaChaPoly_SHA256 initialization
MixHash / MixKey ordering
Bob published static public key in the transcript
Alice fresh Elligator2 ephemeral key
es DH
Alice destination static key encrypted in static-key section
ss DH
payload AEAD with the correct nonce and associated transcript hash
```

The primitive API must receive Alice's actual local destination X25519 static secret/public key and Bob's type-4 LS2 public key.

Suggested API direction:

```text
seal_bound_new_session(
    alice_static_secret,
    alice_static_public,
    alice_ephemeral,
    bob_static_public,
    payload,
    ...
) -> (wire_message, alice_handshake_state)
```

Do not expose secrets through `Debug`, `Clone` without justification, logs, serde, or status surfaces.

`curve25519-elligator2` remains the approved Elligator2 implementation. Do not hand-roll Elligator2.

## 6. Phase D — return authenticated binding material from New Session open

Bob must learn the authenticated Alice static X25519 public key from the decrypted static-key section.

A successful open should return a typed result containing at least:

```text
plaintext payload
alice_static_public
retained NS transcript / chaining context needed for NSR
Bob-side inbound session state needed for later A->B Existing Session
```

Do not derive a far-end Destination hash from:

```text
the static key bytes alone
the Elligator2 representative
a session tag
```

The full Destination binding happens in Plan 127 after matching the authenticated static key to a validated LeaseSet2.

## 7. Phase E — implement exact New Session Reply

Implement the official NSR tag-set and reply handshake from the retained New Session context.

At minimum verify:

```text
reply tag derived from the NS chain using the specified SessionReplyTags KDF
Bob uses a fresh Elligator2 ephemeral key
reply performs the specified ee / se transitions
key section is the specified zero-length authenticated section
payload AEAD uses the correct transcript / nonce
Bob obtains outbound state for B->A Existing Session
Alice obtains inbound state for B->A Existing Session
Alice retains/updates outbound state for A->B Existing Session as required by the base ratchet
```

A reply retransmission may create more than one valid NSR per specification; Plan 126 only needs a bounded correct single-reply path plus replay/duplicate handling sufficient for the local M6 closure.

## 8. Phase F — make session tags the receive classifier

Refactor `EciesSessionManager` so incoming encrypted data is classified by bounded tag state.

Required model:

```text
pending NSR tags -> identify replies to locally-originated New Sessions
active inbound ES tags -> identify Existing Session traffic
otherwise -> candidate New Session, if minimum NS length permits
```

Do not use a first-byte message type.

Do not trial-decrypt ciphertext across every local destination. The owning local destination context is selected before ECIES processing by the inbound tunnel/destination mapping.

Within one destination context, tag look-ahead must be bounded by the existing configuration ceiling.

## 9. Phase G — correct session-manager state ownership

A local destination manager must retain paired state by **far-end Destination identity once bound**.

Before Plan 127 binds a received NS to a full Destination, use an explicit provisional object keyed by the authenticated Alice static public key / retained handshake context. Do not pretend that provisional state is already destination-bound.

For an Alice-originated session:

```text
on NS send:
    retain Alice outbound state to Bob
    retain pending NSR context/tagset

on NSR receive:
    authenticate against pending context
    install Alice inbound state from Bob
    transition pending -> established pair
```

For Bob receiving Alice NS:

```text
on NS open:
    retain Bob inbound state from Alice provisionally
    retain reply context

Plan 127 binds Alice static key to Alice Destination via LS2

on NSR send:
    install Bob outbound state to Alice
    preserve Bob inbound state from Alice
```

After establishment:

```text
A outbound ES -> B inbound ES
B outbound ES -> A inbound ES
```

Do not silently clone a session, seal with the clone, and then fail to persist the advanced ratchet state.

## 10. Phase H — true primitive/manager closure trajectory

Create a deterministic test that does NOT use tunnels or routing yet.

Use two destination X25519 static keypairs A and B and deterministic ephemeral RNG.

Required flow:

```text
A manager creates bound NS using A static key and B published static key
B manager opens NS and observes authenticated A static public key + exact payload
B provisional session is explicitly bound to synthetic far-end DestinationHash A for this unit layer
B manager creates NSR from retained NS context
A manager matches NSR through pending reply tag and decrypts exact reply payload
A sends ES #1 -> B opens exact payload
B sends ES #1 -> A opens exact payload
A sends ES #2 -> B opens exact payload with advanced tag
B sends ES #2 -> A opens exact payload with advanced tag
```

Required negative tests:

```text
wrong Bob static key -> NS authentication fails
changed encrypted static section -> fails
changed NS payload -> fails
wrong NSR tag -> fails boundedly
changed NSR ephemeral/key section -> fails
replayed NSR after acceptance -> rejected/ignored without duplicate install
unknown ES tag -> no plaintext
replayed ES tag -> rejected
out-of-window ES tag -> bounded rejection
wrong destination context -> no trial decryption / no plaintext
```

## 11. Independent/reference evidence

There are no convenient official byte vectors in the ECIES prose specification. Do not compensate by creating a large harness.

Use three small evidence classes:

1. **exact structural assertions** from the official wire diagrams;
2. **fixed deterministic intermediate-value tests** for protocol-name hash, HKDF/MixKey outputs, transcript hashes, AEAD sections, reply tags, and first ES tags using frozen private/public inputs;
3. **clean-room source cross-check** against current Java I2P and/or i2pd behavior for any ambiguous ordering/length. Record provenance in `specs/references/`.

If a Java/i2pd fixture can be generated offline easily, add one. It is useful but not a blocker if exact normative equations and independent intermediate values are established.

Do not claim mixed-router interoperability from same-process round trips.

## 12. Validation

Run at minimum:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked -p i2pr-crypto --all-targets
cargo +1.95.0 test --locked -p i2pr-proto --all-targets
cargo +1.95.0 test --locked -p i2pr-client --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Retired rootless-host checks are not acceptance criteria.

## 13. Explicit acceptance criteria

Plan 126 closes only when all are true:

- [ ] The destination ECIES initializer is the current I2P `Noise_IKelg2_25519_ChaChaPoly_SHA256` contract.
- [ ] Bound NS bytes match the current I2P field ordering and contain no i2pr-only message-type byte.
- [ ] Alice's bound static key is the local destination's published type-4 X25519 key, encrypted/authenticated in the NS static-key section.
- [ ] Bob's NS open returns the authenticated Alice static public key and retained reply/session context.
- [ ] NSR bytes match the current I2P tag + ephemeral + key-section + payload layout and contain no i2pr-only type byte.
- [ ] ES bytes match the current I2P tag + encrypted-payload layout.
- [ ] Incoming NSR/ES classification is tag-driven and bounded.
- [ ] The manager retains the outbound state created on NS send instead of discarding it.
- [ ] Bob can create a production NSR from the state retained while opening the NS.
- [ ] A->B and B->A Existing Session traffic decrypts through the opposite side's paired state.
- [ ] Replay/tag advancement tests prove old tags cannot be reused.
- [ ] No session state is shared across local destination contexts.
- [ ] Current useful DateTime/Garlic/Padding payload-block tests remain green.
- [ ] No NTCP2/SSU2/live-network/SAM/I2CP/proxy work is introduced.

## Handoff on success

Write `plans/126-status.md` with:

```text
plan_121 = corrected-ecies-ratchet-foundation-awaiting-plan127-binding
plan_126 = passed-ecies-destination-ratchet-corrective-foundation
milestone6_local_product = not-closed
next = plans/127-m6-destination-session-routing-final-closure.md
```

Do not restore Plan 121/122/124 final closure until Plan 127 proves destination binding and routing through tunnels.