# Plan 121 — Milestone 6 ECIES-X25519 Garlic/session layer

## Status

- **Ready after Plan 120 closes**.
- Date: **2026-08-19**.
- Parent roadmap: [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md).
- Predecessor: Plan 120 local destination lifecycle + dedicated tunnel pools.
- Scope: modern destination-to-destination ECIES encryption and Garlic payload
  semantics, entirely runtime-neutral/local.
- Destination routing through NetDB/tunnels is Plan 122.

## Objective

Replace the current opaque/deferred Garlic handling with the minimum complete
ECIES-X25519-AEAD-Ratchet destination session layer needed for ordinary
bidirectional destination traffic.

At closure, two local `i2pr-client` destination contexts must be able to execute:

```text
Alice knows Bob's X25519 key from Bob's LeaseSet2
 -> Alice creates bound ECIES New Session Garlic message
 -> Bob validates/decrypts it exactly once
 -> Bob creates New Session Reply
 -> Alice validates/decrypts it
 -> Alice/Bob derive bounded paired session state
 -> Existing Session Garlic messages work in both directions
 -> replay, wrong destination context, invalid tag/MAC, and stale DateTime fail closed
```

Do not route these messages across destination tunnels yet. This plan proves the
cryptographic/message/session layer before Plan 122 composes it with NetDB and
tunnel routing.

---

# 1. Normative references and supported subset

Primary references:

```text
https://i2p.net/en/docs/specs/ecies/
https://i2p.net/en/docs/specs/i2np/
https://i2p.net/en/docs/specs/common-structures/
```

Implement the ordinary deployed destination protocol, crypto type 4.

Required subset:

```text
X25519 static destination keys
Elligator2-encoded new-session ephemeral key
Noise-derived New Session key schedule
New Session with destination binding
New Session Reply
Existing Session tag lookup / key derivation
DateTime payload block
Garlic Clove payload block
Padding payload block
Options only if required by the normative base path; otherwise explicit unsupported
bounded tag look-ahead
replay prevention
paired inbound/outbound session context
```

Deferred unless needed to complete the ordinary streaming-oriented path:

```text
Multicast
zero-static-key mode
protocol-layer responses
MessageNumbers block if still outside deployed required baseline
Termination block
PQ hybrid ECIES
router ECIES Garlic (different context/spec)
ElGamal/AES+SessionTags destination sessions
```

Do not accept unsupported blocks as meaningful defaults.

---

# 2. Cryptographic implementation boundary

No locally invented curve or Elligator primitive is permitted.

The repository already has:

```text
x25519-dalek
ChaCha20-Poly1305
HKDF/HMAC-SHA256
SHA-256
zeroize
```

The missing primitive is the specific Elligator2 mapping/representative support
required by I2P ECIES new-session ephemeral keys.

## Phase A0 — dependency gate

Before implementing protocol code:

1. inspect current maintained Rust options for Curve25519 Elligator2;
2. prefer a maintained pure-Rust implementation with compatible license,
   constant-time considerations documented by upstream, and support for the
   mapping semantics I2P actually requires;
3. pin a specific version/revision according to repository dependency policy;
4. record license, MSRV, `unsafe` usage, transitive dependency delta, and API
   stability;
5. verify against official I2P/reference vectors or independent known-good
   behavior before using it in production paths.

If no acceptable dependency exists, **stop the implementation at this explicit
crypto dependency gate** and document the blocker. Do not hand-roll Elligator2
inside i2pr merely to continue the plan.

Wrap the chosen primitive in `i2pr-crypto` so `i2pr-client` does not depend on
its raw third-party API.

---

# 3. Secret-bearing types

Add narrowly scoped crypto wrappers, conceptually:

```text
DestinationStaticSecret
DestinationStaticPublic
EciesEphemeralSecret
EciesEphemeralRepresentative
EciesHandshakeState
EciesSessionKey
EciesRatchetKey
EciesSessionTag
```

Exact naming may follow current crypto conventions.

Requirements:

- secret values do not expose bytes in `Debug`;
- secret values zeroize on drop where possible;
- public/session-tag values may be copied only where protocol state requires;
- ephemeral private keys are single-use;
- an Elligator2 representative is not treated interchangeably with a decoded
  X25519 public key;
- X25519 little-endian wire semantics are explicit at the wrapper boundary;
- low-order/invalid key behavior follows the X25519 library and protocol policy
  with typed errors.

Do not let client routing code manipulate raw 32-byte secret arrays.

---

# 4. ECIES payload block codec

The ECIES decrypted payload is a typed block sequence, not the legacy ElGamal
Clove Set.

Implement strict block framing with explicit maximum total payload and maximum
block count.

Required blocks:

### DateTime

```text
block type = 0
size       = 4
value      = unsigned Unix seconds
```

For New Session it is required and must be the first block.

### Garlic Clove

Implement the ECIES Garlic Clove block format from the ECIES specification.
It is intentionally different from the legacy ElGamal `GarlicClove`/Clove Set.

Important invariants:

```text
one clove per payload block
clove may not fragment across blocks/frames
clove carries Garlic delivery instructions + short-form I2NP message
legacy clove certificate/id/expiration fields are absent
nested I2NP header uses the ECIES/NTCP2 9-byte short format required by spec
```

Keep ECIES and legacy garlic/clove structures separate types to prevent accidental
cross-encoding.

### Padding

Bounded opaque padding is allowed only where the specification permits and must
be last when required by ordering rules.

### Options

Implement only if required for the supported path. Otherwise parse the type and
return an explicit unsupported block error rather than ignoring security-relevant
options.

Required malformed tests:

```text
block_declared_length_overrun
block_count_limit
new_session_without_datetime
new_session_datetime_not_first
padding_not_last
clove_crosses_block_boundary
unknown_required_block_rejected
```

---

# 5. Garlic delivery instructions

Reuse the existing common/I2NP delivery-instruction primitives only if their
wire format is exactly the Garlic Clove delivery-instruction format.

Do not reuse tunnel-message delivery instructions; they are a different format.

Required ordinary destination cases:

```text
LOCAL
DESTINATION
```

ROUTER/TUNNEL delivery may be implemented if needed by the ECIES spec and cleanly
supported by existing types, but destination local delivery is the mandatory
Milestone 6 path.

For a destination-directed clove, validate the target hash/identity according to
current I2P semantics and keep it distinct from the outer destination chosen by
LeaseSet routing.

---

# 6. New Session with binding

For ordinary bidirectional destination traffic and future streaming, implement
New Session with destination binding.

The initiator Alice knows Bob's static X25519 key from Bob's validated LeaseSet2.
Alice also has her own static X25519 destination key from Plan 120.

Required conceptual transcript:

```text
Bob static public known
Alice fresh Elligator2-representable ephemeral
 -> Noise/I2P initial hash/chaining-key initialization
 -> es
 -> encrypt Alice static key / binding section
 -> ss
 -> encrypt payload section
 -> output Garlic encrypted body
```

Follow the exact I2P-defined protocol name, KDF personalization/info strings,
hash mixing, nonce values, associated data, and section boundaries. Do not infer
from NTCP2 simply because both use Noise and ChaChaPoly.

New Session payload must begin with DateTime and normally contain a Garlic Clove
for the application/destination message.

Every retransmission that requires a new-session message must follow the
specification's ephemeral-key freshness requirement; do not reuse the old
new-session ephemeral key merely because application bytes are the same.

---

# 7. New Session receive path and replay prevention

Bob must:

```text
classify candidate New Session
 -> reject duplicate/replayed ephemeral representative early when possible
 -> Elligator2-decode ephemeral key
 -> derive/decrypt binding section
 -> validate Alice static key/binding semantics
 -> derive/decrypt payload
 -> require recent DateTime
 -> apply bounded replay cache/Bloom-equivalent policy
 -> parse blocks
 -> create paired inbound/outbound session state
 -> deliver Garlic cloves only after authentication succeeds
```

Replay state must be explicitly bounded by count/time/bytes. A bounded hash set
is acceptable for the deterministic MVP if a Bloom filter adds complexity
without value; document the memory bound and expiry policy.

No unauthenticated clove bytes may be delivered upstream.

---

# 8. New Session Reply

Implement the protocol-defined reply and state transition.

Required behavior:

```text
Bob accepts bound NS
 -> Bob derives reply material / fresh ratchet state
 -> Bob emits cleartext reply tag + encrypted reply payload per spec
 -> Alice locates pending initiating session
 -> Alice authenticates/decrypts NSR
 -> Alice installs paired session state
```

No payload block is mandatory in NSR; support a minimal authenticated empty/ACK
reply plus Garlic Clove/Padding where required for tests.

Session installation must be transactional: an invalid NSR must not partially
replace the previous outbound/inbound state.

---

# 9. Existing Session and tag sets

Implement the minimum data-phase session/tag machinery required for sequential
bidirectional destination messaging.

Session state should explicitly own:

```text
far-end Destination identity/hash
current outbound ratchet/tag-set state
current inbound ratchet/tag-set state
paired session relationship
next expected/generation counters needed by current deployed format
expiry / last-used
```

The ECIES spec permits a large theoretical tag set. Do **not** precompute all
possible tags.

Use bounded look-ahead with a conservative default and a hard maximum.

Required invariants:

- one outbound active session per local-destination/far-destination binding where
  the supported protocol requires it;
- inbound and outbound sessions are paired according to the spec;
- session state belongs to one local Destination context only;
- consumed tags cannot be used twice;
- skipped/look-ahead tags are bounded;
- old sessions expire on deterministic time policy;
- failed decrypt does not advance ratchet/tag state;
- successful decrypt advances state exactly once.

---

# 10. Destination-context session manager

Place session lifecycle in `i2pr-client`, not `i2pr-crypto`.

Conceptual API:

```text
EciesSessionManager::encrypt_new(remote_ls2, clove, now, rng)
EciesSessionManager::decrypt_incoming(garlic, now)
EciesSessionManager::encrypt_existing(remote_destination, clove, now)
EciesSessionManager::advance_time(now)
```

The manager must have explicit bounds:

```text
max outbound sessions per destination
max inbound sessions per destination
max pending new-session handshakes
max tag look-ahead
max replay-cache entries
max total retained session bytes (or a defensible count-derived cap)
session idle/max lifetime
```

If limits are reached, shed/expire according to deterministic policy; never grow
unboundedly because a peer sends random candidate tags.

---

# 11. I2NP Garlic carrier

Replace the current `OpaqueMessageBody` use for the supported ECIES destination
Garlic path with a typed carrier at the correct layer.

Do not remove the ability to retain an opaque Garlic body where protocol context
does not yet identify which decryption scheme applies. The outer I2NP Garlic
message alone does not necessarily prove which destination/session context
should decrypt it.

Preferred separation:

```text
i2pr-proto
  GarlicMessageBody { encrypted bytes bounded }
  ECIES payload block structures

i2pr-client
  context-aware ECIES decrypt/classify/session lookup
```

The encrypted outer bytes should remain opaque to generic I2NP routing. Crypto
semantics activate only at the owning destination context.

---

# 12. Independent vector/evidence strategy

Because this is security-sensitive crypto protocol code, self-round-trip tests
are insufficient.

For each foundational stage, freeze independent known-good vectors or use a
small temporary reference implementation test that does not call the production
code under test.

Required evidence should cover at least:

```text
Elligator2 encode/decode representative behavior
X25519 DH input/output
initial protocol hash/chaining key
New Session section keys
New Session encrypted fixture
New Session Reply fixture
first Existing Session tag/key fixture
ECIES payload block bytes
```

Preferred sources, in order:

1. official I2P published vectors if available;
2. pinned Java I2P / Emissary/i2pd source behavior through a narrow test-only
   oracle with license-clean clean-room boundary;
3. an independent local oracle assembled from low-level primitives only.

Record provenance. Do not generate vectors once from the production functions
and call them independent.

---

# 13. Deterministic integration trajectory

Required local integration test using two real Plan 120 destination contexts:

```text
A creates signed LS2 with X25519 key
B creates signed LS2 with X25519 key
A imports/validates B LS2
B imports/validates A LS2

A encrypts bound New Session Garlic clove to B
 -> B decrypts/authenticates
 -> B observes exact clove payload once

B emits New Session Reply
 -> A authenticates
 -> paired sessions installed

A -> B Existing Session message
B -> A Existing Session message
 -> exact payloads delivered once

replay A's NS
 -> rejected
replay consumed existing-session tag
 -> rejected
wrong local destination context attempts decrypt
 -> rejected without cross-context state mutation
advance deterministic time
 -> sessions/replay state expire within bounds
```

No tunnel delivery is needed in this plan.

---

# 14. Fuzzing / adversarial tests

Add fuzz targets or property/adversarial tests for public untrusted parsers:

```text
ECIES payload block decoder
Garlic clove block decoder
session message classifier
```

Minimum negative cases:

```text
truncated ephemeral representative
invalid Elligator representative
truncated MAC
bad MAC
oversized payload
block length overflow
excessive block count
stale/future DateTime outside policy
replayed NS
random unknown session tags
replayed consumed tag
wrong remote static key
wrong local destination context
```

All fail without panic, unbounded allocation, or partial plaintext delivery.

---

# 15. Validation commands

At minimum:

```bash
cargo fmt --all --check
cargo test --locked -p i2pr-crypto --all-targets
cargo test --locked -p i2pr-proto --all-targets
cargo test --locked -p i2pr-client --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

Run relevant fuzz/property targets according to current repository practice.

No authenticated router transport is required.

---

# 16. Explicit acceptance criteria

Plan 121 is complete only when:

- [ ] An acceptable non-hand-rolled Elligator2 dependency/wrapper is selected,
      documented, pinned, and validated; or the plan stops explicitly at that
      dependency gate without insecure fallback.
- [ ] Secret-bearing ECIES types do not reveal key bytes via `Debug` and zeroize
      where supported.
- [ ] ECIES payload blocks are strictly bounded and enforce ordering rules.
- [ ] ECIES Garlic Clove is represented separately from the legacy ElGamal Clove
      Set format.
- [ ] New Session with destination binding matches independent/frozen evidence.
- [ ] New Session receive path authenticates before delivering any clove.
- [ ] DateTime freshness and bounded replay prevention are enforced.
- [ ] New Session Reply installs session state transactionally.
- [ ] Existing Session messages work in both directions.
- [ ] Tag look-ahead and session counts are bounded.
- [ ] Consumed tags/replayed New Sessions are rejected.
- [ ] Failed decrypt does not advance session state.
- [ ] Session state is isolated per local destination.
- [ ] A two-destination deterministic trajectory proves NS -> NSR -> Existing
      Session both directions with exact-once payload delivery.
- [ ] Independent crypto/vector provenance is recorded; tests are not purely
      self-derived.
- [ ] No direct destination tunnel routing, SAM, I2CP, streaming, legacy
      ElGamal/AES session implementation, or normal-daemon transport activation
      is introduced.
- [ ] Workspace tests/clippy/docs and boundary checks pass.

## Handoff

On closure:

```text
plan_121 = passed-ecies-destination-session-layer
next = plans/122-m6-destination-routing-and-netdb-composition.md
```
