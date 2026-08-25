# Plan 127 — Milestone 6 destination-session routing final closure

## Status

- **Execute only after Plan 126 passes.**
- Predecessor: `plans/126-m6-ecies-destination-ratchet-corrective-foundation.md`.
- Successor: `plans/128-m6-streaming-wire-protocol-corrective-closure.md`.
- Purpose: close the remaining Plan 121 / 122 / 124 local destination-layer gaps with the corrected ECIES ratchet.

## Objective

Compose the Plan 126 bound ECIES session lifecycle with Standard LeaseSet2, destination-owned tunnel pools, the corrected Plan 124 Garlic-through-tunnel path, destination ownership, reverse routing, and application delivery.

The authoritative trajectory for this pass is:

```text
A bound NS + A LeaseSet2
 -> A outbound destination tunnel
 -> A OBEP
 -> explicit authenticated-router-link-bypassed-local-seam
 -> B inbound destination tunnel
 -> B destination owner
 -> B ECIES open
 -> validate/bind A LeaseSet2 to authenticated A static key
 -> exact A application payload

B NSR
 -> B outbound destination tunnel selected from A LeaseSet2
 -> seam
 -> A inbound destination tunnel
 -> A destination owner
 -> A pending NSR context
 -> exact B application payload

A ES -> B over same routing architecture
B ES -> A over same routing architecture
```

No Streaming packet logic belongs in Plan 127. Use opaque application/I2NP Data payloads so destination/session correctness is isolated.

## 1. Preserve the Plan 124 primary composition fix

Do not regress:

```text
I2NP Data
 -> ECIES encrypted destination payload
 -> I2NP Garlic carrier
 -> destination outbound tunnel
```

`forward_cells()` must continue to receive the standard-encoded I2NP Garlic carrier, never the plaintext inner Data envelope.

The only network omission remains the explicit seam **after** real outbound endpoint processing and **before** remote router ingress:

```text
authenticated-router-link-bypassed-local-seam
```

The seam must pass the exact OBEP action unchanged; it may drop/reorder/duplicate for tests, but must not decrypt, re-encrypt, rewrite the target gateway, or rewrite the tunnel id.

## 2. Fix New Session sender binding

The Plan 126 NS open yields an authenticated Alice static X25519 public key, but that key alone is not a full Destination.

The ECIES specification requires Bob to discover a LeaseSet for Alice whose type-4 public key matches the static key from the NS before the pending session can be bound/routed back to Alice.

For the M6 repliable path, require the initial bound NS to bundle A's current Standard LeaseSet2.

Required processing order at B:

```text
B tunnel owner selected before ECIES
 -> authenticate/decrypt NS
 -> obtain authenticated A static X25519 key
 -> decode all payload blocks
 -> find bundled DatabaseStore(Standard LeaseSet2)
 -> validate LS2 signature, time, structure, leases
 -> derive A DestinationHash from the LeaseSet's contained Destination
 -> verify LS2 usable type-4 X25519 key == authenticated A static key from NS
 -> only then bind provisional ECIES state to A DestinationHash
 -> make validated A LS2 available to B routing
```

Do not derive `sender_destination` from:

```text
NS static key bytes
NS representative
NSR tag
ES tag
```

If the LS2 signature is valid but its type-4 key does not match the authenticated NS static key, reject the binding and do not send an NSR.

## 3. Correct bundled LeaseSet2 processing

At the source floor, the bundled-LS2 path is incomplete and `record_accepted_lease_set2()` is effectively a no-op.

Correct the ownership model so a successful inbound bound NS returns or records a typed result containing the validated sender LS2.

Do not validate Alice's bundled LeaseSet2 under Bob's local destination hash. The expected NetDB key is derived from the **Destination contained in Alice's LeaseSet2**.

Suggested dispatch outcome information:

```text
NewSessionProcessed {
    local_destination,
    remote_destination_hash,
    validated_remote_lease_set2: Option<ValidatedLeaseSet2>,
    ...
}
```

Exact API names are discretionary.

The caller/runtime composition should be able to install that validated record into B's `DestinationRouting` without parsing raw payload bytes a second time.

## 4. Make reverse routing a production composition

After B accepts/binds A's LS2:

```text
B DestinationRouting registers validated A LS2
B can select a non-expired A Lease2
B can obtain A type-4 static public key
B can select a destination-owned outbound tunnel
```

No test-only direct route may be substituted for this.

If the architecture has separate NetDB store and per-destination active-remote cache, add one explicit typed handoff rather than duplicating validation.

## 5. Add an explicit NSR send path

The first reply to a bound NS must use Plan 126's retained New Session Reply context, not create an unrelated fresh New Session merely because `encrypt_to_remote()` has no established ES slot yet.

Refactor the session/routing API so the outbound composer can choose among:

```text
BoundNewSession
NewSessionReply
ExistingSession
```

based on destination-scoped session state.

Possible APIs:

```text
session.encrypt_outbound(...)
```

returning a typed `EciesOutboundMessage`, or a narrower explicit reply method. The exact surface is discretionary, but the state machine must be unambiguous.

`OutboundDeliveryPlan` diagnostics should expose which destination ECIES form was emitted so tests can assert:

```text
first A->B = New Session
first B->A = New Session Reply
next A->B = Existing Session
next B->A = Existing Session
```

Do not infer type from a magic first byte.

## 6. Local target ownership remains separate from remote identity

The inbound tunnel / Lease2 maps the incoming message to one **local** destination context before ECIES processing.

Keep these concepts distinct:

```text
local owner: B DestinationId / B DestinationHash
remote peer identity after binding: A DestinationHash
```

A Garlic clove with `Delivery::Destination(B)` is delivered to B. Its sender identity is not B and must not be inferred from the delivery instruction.

No trial decryption across every local destination key is permitted.

## 7. Plan 127 master trajectory

Create a new focused test, preferably:

```text
crates/i2pr-client/tests/plan127_trajectory.rs
```

Use two independent local destinations A and B. Each side must own:

```text
DestinationIdentity
current signed Standard LeaseSet2
DestinationTunnelPool
one real established outbound destination tunnel
one real established inbound destination tunnel
DestinationRouting
EciesSessionManager
DestinationDispatcher / owner binding
```

### 7.1 A -> B bound New Session

Required assertions:

```text
A chooses B LS2 / Lease2
A's NS contains A's bound static X25519 key cryptographically, not cleartext
A NS payload bundles A's validated current LS2
A outbound tunnel carries I2NP Garlic, not plaintext Data
OBEP target == B selected Lease2 gateway/tunnel id
B inbound tunnel recovers exact Garlic carrier
B owner dispatch selects B only
B authenticates/decrypts NS
B sees exact A static public key
B validates A LS2 under A DestinationHash
A LS2 type-4 key == authenticated NS static key
B binds provisional session to A DestinationHash
B application receives exact payload once
```

### 7.2 B -> A New Session Reply

Required assertions:

```text
B routing uses the A LS2 learned from the authenticated NS payload
B emits NSR from retained Plan126 reply context
B sends through a real B outbound destination tunnel
OBEP targets selected A Lease2
A inbound tunnel recovers exact Garlic carrier
A owner dispatch selects A only
A matches NSR through its pending reply tag/context
A authenticates/decrypts exact reply payload
A pending handshake becomes an established paired session
```

### 7.3 Existing Session both directions

Send at least two messages each direction:

```text
A ES1 -> B
B ES1 -> A
A ES2 -> B
B ES2 -> A
```

Each must traverse destination routing and both tunnel directions.

Assert:

```text
all are Existing Session, not repeated NS/NSR
exact payload
exact-once delivery
session tags advance
old consumed tag cannot be replayed
```

## 8. Correct dispatcher classification

Remove source-floor logic that treats all old `0xE2` messages as NSR.

The destination context should pass raw ECIES encrypted data to `EciesSessionManager`, which returns a typed authenticated outcome such as:

```text
OpenedNewSession
OpenedNewSessionReply
OpenedExistingSession
```

The dispatcher should process payload blocks only **after** session authentication succeeds.

Unknown session tags must produce a bounded rejection and no application plaintext.

## 9. Failure and security cases

Add deterministic tests for:

```text
valid A LS2 but static key != NS authenticated static key -> reject binding
invalid A LS2 signature -> reject
expired A LS2 -> no reverse route / no NSR
NS tamper -> no plaintext / no binding
NSR wrong tag -> reject
NSR replay -> no duplicate session install / no duplicate payload
ES unknown tag -> reject
ES replay -> reject
ES ciphertext tamper -> reject
wrong inbound owner -> no trial decryption
removed destination owner -> typed UnknownDestination
full application queue -> typed bounded failure
session expiry -> stale ES tags fail; new outbound establishes a new session deliberately
```

One malformed remote must not poison an unrelated valid session.

## 10. Resource and lifecycle rules

Preserve or strengthen ceilings for:

```text
pending NS handshakes
pending NSR contexts
inbound sessions per remote
outbound session per remote (spec expects one current outbound)
tag look-ahead
replay state
active remote LeaseSet2 entries
application queue bytes/messages
```

Outbound sessions should expire before retained inbound sessions where practical, consistent with the ECIES specification.

## 11. Do not add unrelated runtime work

Explicitly out of scope:

```text
Streaming packet fixes (Plan 128)
SAM
I2CP socket server
HTTP/SOCKS
NTCP2/SSU2 activation
external routers
Python harnesses
Docker/VM/namespaces
```

## 12. Validation

Run:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked -p i2pr-crypto --all-targets
cargo +1.95.0 test --locked -p i2pr-proto --all-targets
cargo +1.95.0 test --locked -p i2pr-client --all-targets
cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets
cargo +1.95.0 test --locked -p i2pr-netdb --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

## 13. Explicit acceptance criteria

- [ ] Plan 126's real bound NS/NSR/ES format is the only destination ECIES format used by routing.
- [ ] Initial repliable NS automatically/explicitly bundles the local current Standard LS2 needed for reverse binding.
- [ ] The receiver obtains the sender's authenticated static key from NS decryption.
- [ ] The bundled sender LS2 is validated under its own contained DestinationHash.
- [ ] The bundled LS2 type-4 key must match the authenticated NS static key.
- [ ] Successful binding yields a typed remote DestinationHash and validated LS2.
- [ ] Reverse routing installs/reuses that validated LS2 without raw reparse or false validation under the local destination.
- [ ] B's first reply is a real NSR from retained NS state.
- [ ] A accepts NSR through its pending tag/context.
- [ ] A->B and B->A then use Existing Session in both directions.
- [ ] Every NS/NSR/ES trajectory uses real destination-owned outbound/inbound tunnel roles and only the explicit post-OBEP local seam.
- [ ] Plan 124's Garlic-not-plaintext OBEP byte identity remains green.
- [ ] Replay, mismatch, wrong-owner, stale-LS2, tamper, and capacity cases fail boundedly.
- [ ] No external interoperability claim is made.

## Handoff on success

Update statuses to:

```text
plan_121 = passed-corrected-ecies-destination-session-layer-local
plan_122 = passed-corrected-local-destination-routing
plan_124 = passed-corrected-destination-routing-local-closure
plan_127 = passed-destination-session-routing-final-closure
milestone6_local_product = not-closed
next = plans/128-m6-streaming-wire-protocol-corrective-closure.md
```

The word `local` is mandatory. Mixed-router destination ECIES interoperability remains separate evidence debt.