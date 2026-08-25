# Plan 129 — Milestone 6 integrated destination + Streaming final gate

## Status

- **Execute only after Plans 126, 127, and 128 pass.**
- This is the final local-product gate for Milestone 6.
- It must not introduce another transport/harness program.
- On success, the next product layer is SAM baseline planning (Milestone 7).

## Objective

Prove that the corrected destination and Streaming layers compose through the actual local product architecture in both directions.

The test must not transfer `TransportSendRequest` directly between Streaming managers.

The authoritative path is:

```text
Streaming packet
 -> canonical I2P protocol-6 gzip ClientPayload
 -> I2NP Data
 -> corrected bound ECIES NS / NSR / ES
 -> I2NP Garlic
 -> local destination outbound tunnel
 -> outbound participant(s)
 -> OBEP TUNNEL(remote gateway, remote tunnel id)
 -> authenticated-router-link-bypassed-local-seam
 -> remote inbound gateway
 -> inbound participant(s)
 -> local inbound endpoint
 -> remote Destination owner
 -> corrected ECIES tag/session authentication
 -> I2NP Data
 -> canonical gzip ClientPayload decode
 -> protocol == 6
 -> destination-port / local Streaming listener
 -> Streaming packet
```

## 1. Finish the Streaming destination adapter boundary

Plan 125 added an outbound `StreamingDestinationAdapter::send()` but no complete inbound inverse.

Refactor into one small runtime-neutral composition surface.

Suggested conceptual split:

```text
StreamingOutboundAdapter
    TransportSendRequest -> OutboundDeliveryPlan

StreamingInboundAdapter
    authenticated destination payload -> decoded protocol-6 packet dispatch
```

The exact names are discretionary; one combined adapter is also acceptable.

The adapter owns no sockets, DNS, Tokio task spawning, or router transport.

## 2. Correct outbound adapter sizing

At the source floor:

```text
MAX_STREAMING_ADAPTER_PAYLOAD_BYTES = MAX_STREAMING_PAYLOAD_BYTES
```

but `TransportSendRequest.application_payload` is the **gzip-encoded complete Streaming packet**, not the application payload inside a Streaming packet.

After Plan 128, these are explicitly different concepts.

The outbound adapter should bound the encoded client payload against the client-payload/I2NP limits, for example `MAX_CLIENT_PAYLOAD_BYTES`, not the negotiated Streaming application payload MTU.

Remove the redundant source-floor behavior where the adapter constructs an inner I2NP Data envelope and discards it before calling `compose_outbound_delivery()`. There should be one canonical Data-envelope construction owner.

## 3. Implement inbound protocol-6 dispatch

After `DestinationDispatcher` authenticates/decrypts the ECIES Garlic payload, the local destination receives an encoded inner I2NP message.

The inbound adapter must:

```text
decode standard I2NP message
require I2npBody::Data
obtain Data payload bytes
decode canonical I2P gzip ClientPayload
require protocol == 6 for Streaming path
read source_port and destination_port
select the owning local destination's StreamingManager
pass only the decoded Streaming packet bytes to process_inbound_packet()
```

Non-protocol-6 client payloads must not be accidentally fed to Streaming. Return a typed unsupported-protocol outcome for future datagram/I2CP layers.

Destination ports are I2P ports; do not apply local TCP privileged-port policy.

## 4. Use one narrow two-destination integration fixture

Create:

```text
crates/i2pr-client/tests/plan129_trajectory.rs
```

or an equivalently narrow test file.

A test-local helper may represent a local destination runtime containing:

```text
identity
signed current LS2
tunnel pool / explicit tunnel roles
DestinationRouting
EciesSessionManager
DestinationDispatcher
StreamingManager
outbound/inbound adapters
```

Do not create a general simulation framework or another crate unless production code genuinely needs it.

## 5. Master A -> B SYN trajectory

Set up:

```text
A Streaming listener/client context
B Streaming listener on destination port
A knows validated B LS2
B does not need pre-seeded A LS2; it learns A from the bound NS payload
```

Required flow:

```text
A StreamingManager.connect(B)
 -> canonical initial SYN from Plan 128
 -> gzip ClientPayload protocol 6
 -> outbound adapter
 -> I2NP Data
 -> Plan 127 bound New Session, bundling current A LS2
 -> I2NP Garlic
 -> A destination outbound tunnel
 -> A OBEP TUNNEL to selected B Lease2
 -> exact local router-link seam
 -> B inbound destination tunnel
 -> B destination owner
 -> ECIES NS auth/decrypt
 -> A static key matched to bundled A LS2
 -> B routing learns A LS2
 -> inner Data
 -> gzip decode
 -> B StreamingManager validates signed SYN
 -> B listener reports pending inbound connection
```

Assertions:

```text
no plaintext Streaming bytes cross the router-link seam
first destination ECIES form is New Session
A remains OutboundSynSent
B has exactly one pending inbound stream
A LS2 binding is established before B can route the reply
```

## 6. Master B -> A SYN-response trajectory

Application/policy accepts B's pending inbound stream.

Required flow:

```text
B emits canonical Plan 128 SYN response
 -> gzip protocol 6
 -> B outbound adapter
 -> Plan 127 New Session Reply using retained A NS context
 -> B outbound destination tunnel selected from authenticated A LS2
 -> seam
 -> A inbound destination tunnel
 -> A destination owner
 -> A pending NSR/tag context
 -> ECIES auth/decrypt
 -> Data/gzip/Streaming decode
 -> A verifies B signed SYN response
 -> A learns B receive stream id
 -> negotiated payload max = min(A, B)
 -> A Established
```

B must also be in the accepted/Established state according to the local state model.

Assertions:

```text
B first reply ECIES form == New Session Reply
response has no replay NACK field
response NO_ACK is false
A only becomes Established after this exact reverse destination path succeeds
```

## 7. Existing Session data in both directions

After handshake, send nontrivial application bytes in both directions.

Use payloads that require more than a tiny single-frame happy path where practical, while staying deterministic.

Required:

```text
A write -> one or more Streaming packets -> ES -> B exact ordered bytes
B write -> one or more Streaming packets -> ES -> A exact ordered bytes
```

Assert the destination ECIES form is Existing Session for all steady-state packets.

No additional bound New Session should be created while the established session is healthy.

## 8. ACK/NACK and retransmission over the integrated path

The direct `VirtualWire` tests from Plan 123 may remain as fast unit coverage, but they are not closure evidence.

For Plan 129, inject faults only at seams that preserve the protocol work being tested.

### Drop

Drop one typed router-delivery action **after real OBEP processing**.

Advance `ManualClock` to the retransmission deadline.

Assert:

```text
sender retransmits
retransmission again traverses gzip -> ECIES -> outbound tunnel
receiver delivers application bytes once
ACK eventually clears tracked packet
```

### Duplicate

Duplicate an exact router-delivery action at the same seam.

Assert:

```text
ECIES/Streaming duplicate handling does not deliver application bytes twice
state remains healthy
```

### Reorder

Queue at least two post-OBEP actions and deliver them in reverse order.

Assert:

```text
receiver reorders by Streaming sequence
application observes original byte order
NACK/ACK state converges
```

Do not simulate loss by directly deleting a `TransportSendRequest` before encryption/tunnel processing in the authoritative tests.

## 9. Corruption tests at correct layers

### Invalid Streaming signature with valid destination encryption

Construct/corrupt a Streaming control packet before it is passed to destination encryption, then send it through the full destination path.

Expected:

```text
ECIES succeeds
client payload gzip succeeds
Streaming signature verification fails
no connection state transition / no app data
```

### Bad gzip CRC with valid destination encryption

Create a malformed protocol-6 gzip payload and encrypt/tunnel it normally.

Expected:

```text
ECIES succeeds
gzip decode fails typed CRC error
StreamingManager never sees packet
```

### ECIES ciphertext tamper

Tamper the recovered I2NP Garlic encrypted body before destination dispatch, or use an equivalent test seam after tunnel recovery and before ECIES open.

Expected:

```text
ECIES AEAD fails
no inner Data / gzip / Streaming processing
```

Keep the tamper seam explicit in the test name/comment so it is not confused with the router-link seam.

## 10. CLOSE over the integrated path

Exercise a graceful close in both directions.

Required behavior:

```text
A sends signed CLOSE through full ES destination path
B authenticates/verifies and enters close state
B sends its required CLOSE/close response through reverse full path
A completes graceful close only after peer response according to connection policy
resources/session tracking eventually release boundedly
```

Do not mark both sides closed simply because A locally queued CLOSE.

## 11. RESET over the integrated path

Send a signed RESET through the full destination path.

Assert:

```text
ECIES and gzip decode succeed
RESET signature verifies using established peer identity
receiver terminates stream immediately
queued application data is not delivered afterward
unrelated streams remain unaffected
```

## 12. 0-RTT scope

The current implementation may continue to omit application data in the initial SYN.

Required distinction:

```text
OutboundSynSent may support bounded pre-response data in the future
but it is not equivalent to Established
```

Plan 129 does not need to add 0-RTT payload sending to close Milestone 6.

## 13. Local architecture/evidence review

After all tests pass, perform one narrow review answering only:

> Can a future SAM v3 adapter consume the local destination + Streaming API without bypassing LeaseSet2, ECIES, Garlic, destination tunnels, or Streaming wire semantics?

Required answer for closure: **yes**.

Verify:

```text
SAM would call destination/Streaming APIs, not ECIES internals
SAM would not own tunnel selection
SAM would not own NTCP2/SSU2
SAM would not need a direct client-to-client shortcut
```

If the answer is no because of a concrete local API/product defect, fix that defect in Plan 129. Do not create a broad new validation plan.

## 14. Documentation and status synchronization

On success update at minimum:

```text
plans/121-status.md
plans/122-status.md
plans/123-status.md
plans/124-status.md
plans/125-status.md
plans/126-status.md
plans/127-status.md
plans/128-status.md
plans/129-status.md
README.md
AGENTS.md
docs/architecture/i2pr-client.md
docs/architecture/overview.md
docs/protocol-support.md
specs/support.toml
```

Remove or correct stale claims such as:

```text
Streaming not implemented
Milestone 6 already closed by Plan 125
Plan 123 first interoperable Streaming layer
```

Do not state mixed-router interoperability.

## 15. Validation

Run the complete local gate:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 test --locked -p i2pr-crypto --all-targets
cargo +1.95.0 test --locked -p i2pr-proto --all-targets
cargo +1.95.0 test --locked -p i2pr-client --all-targets
cargo +1.95.0 test --locked -p i2pr-tunnel --all-targets
cargo +1.95.0 test --locked -p i2pr-netdb --all-targets
cargo +1.95.0 test --locked -p i2pr-daemon --all-targets
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
```

A known retired rootless-supervisor script must not become a Milestone 6 blocker.

There is no requirement to run Docker, namespaces, Multipass, or live I2P.

## 16. Explicit closure criteria

Plan 129 closes only when every item is true:

- [ ] Plan 126 ECIES bound NS/NSR/ES primitives and session manager are green.
- [ ] Plan 127 proves bound sender LS2/static-key validation, reverse routing, NSR, and ES both directions over actual destination tunnels.
- [ ] Plan 128 Streaming flags/options/signatures/replay/MTU are current-wire-correct.
- [ ] Outbound Streaming adapter accepts a full encoded client payload under the correct bound, not `MAX_STREAMING_PAYLOAD_BYTES`.
- [ ] Inbound adapter decodes I2NP Data -> gzip -> protocol/ports -> Streaming packet.
- [ ] Initial A SYN reaches B only through the full destination stack.
- [ ] B SYN response reaches A only through the full reverse destination stack and uses NSR.
- [ ] A does not become Established before that response arrives/authenticates.
- [ ] Steady-state A->B and B->A data uses Existing Session.
- [ ] Integrated drop causes real retransmission and exact-once delivery.
- [ ] Integrated duplicate is idempotent.
- [ ] Integrated reordering yields ordered application bytes.
- [ ] Invalid Streaming signature is rejected after otherwise-valid destination delivery.
- [ ] Bad gzip CRC is rejected before Streaming processing.
- [ ] ECIES tamper yields no plaintext.
- [ ] Graceful CLOSE completes through peer response over the full path.
- [ ] RESET terminates through the full path.
- [ ] Resource ceilings remain explicit and tests do not introduce unbounded queues/state.
- [ ] No direct `VirtualWire<TransportSendRequest>` is cited as Milestone 6 closure evidence.
- [ ] No NTCP2/SSU2/live-network dependency is introduced.
- [ ] Documentation/status authority is internally consistent.

## 17. Final successful classification

Only after all criteria pass:

```text
plan_119 = passed-leaseset2-protocol-foundation
plan_120 = passed-destination-lifecycle-and-pools
plan_121 = passed-corrected-ecies-destination-session-layer-local
plan_122 = passed-corrected-local-destination-routing
plan_123 = passed-corrected-streaming-wire-local
plan_124 = passed-corrected-destination-routing-local-closure
plan_125 = superseded-by-final-corrective-closure
plan_126 = passed-ecies-destination-ratchet-corrective-foundation
plan_127 = passed-destination-session-routing-final-closure
plan_128 = passed-streaming-wire-protocol-corrective-closure
plan_129 = passed-milestone6-integrated-local-product-gate
milestone6_local_product = passed
milestone6_interoperable = not-yet-claimed
external_acceptance_debt = retained-separately
router_construction = may-continue
next_product_layer = SAM baseline planning (Milestone 7)
```

Do not add another Milestone 6 corrective plan on success.