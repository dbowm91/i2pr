# Plan 124 — Milestone 6 Plan 122 destination-routing corrective closure

## Status

- **Ready for execution**.
- Date: **2026-08-24**.
- Source floor: `704bf00b82e7c556e46432520a2b8f325a88f15f`.
- Predecessor implementation: Plan 122 (`passed-local-destination-routing` as currently recorded, but this plan reopens that closure).
- Dependent implementation: Plan 123 must be treated as **provisional** until this plan closes, because Plan 123 is required to route through the Plan 122 product path.
- Successor: [`125-m6-streaming-corrective-and-local-closure.md`](125-m6-streaming-corrective-and-local-closure.md).
- This plan **supersedes the currently documented idea that “Plan 124” is a generic streaming runtime/UDP/TCP adapter**. Do not implement that path. The current defect is below it and must be corrected first.

## Objective

Correct the concrete Plan 122 composition defect where the destination routing layer creates an ECIES-protected Garlic message but sends the plaintext inner I2NP `Data` envelope into the outbound tunnel data plane.

Then add the missing successful deterministic A -> B -> A trajectory proving that the production destination-routing composition actually carries encrypted Garlic through:

```text
local destination
 -> Standard LeaseSet2 resolution
 -> lease selection
 -> ECIES Garlic construction
 -> I2NP Garlic carrier
 -> destination-owned outbound tunnel
 -> OBEP TUNNEL delivery to selected remote lease
 -> one explicit local router-link seam
 -> remote inbound gateway / participant / local endpoint
 -> destination-owner dispatch
 -> ECIES authentication/decryption
 -> inner I2NP Data
 -> exact application payload
```

Plan 124 is complete only when the bytes entering and leaving the tunnel path are the expected encrypted Garlic carrier. Merely constructing an encrypted object alongside a plaintext tunnel send does not satisfy this plan.

This is an ordinary local Rust composition correction. It requires no socket namespace, Docker, VM, Python interop harness, live I2P network, or authenticated NTCP2 activation.

---

# 1. Why Plan 122 is reopened

At the source floor, `crates/i2pr-client/src/routing.rs::compose_outbound_delivery()` performs these two separate operations:

```text
1. build ECIES Garlic payload
2. retain it in OutboundDeliveryPlan.encrypted_message
```

but the tunnel invocation is effectively:

```rust
forward_cells(&header, &inner_envelope_bytes, ...)
```

where `inner_envelope_bytes` is the plaintext standard-encoded I2NP `Data` message.

Therefore the actual production tunnel cells are not carrying the ECIES Garlic message.

This violates the Plan 122 required path:

```text
I2NP Data
 -> ECIES Garlic
 -> destination outbound tunnel
 -> TUNNEL(remote_gateway, remote_tunnel_id, Garlic)
```

and invalidates the strongest current Plan 122 closure claim.

A second closure defect is test coverage. `crates/i2pr-client/tests/plan122_trajectory.rs` contains useful component tests, but the nominal Phase-F test does not call `compose_outbound_delivery()` and there is no successful production A -> B -> A trajectory through both tunnel directions and authenticated destination dispatch.

Do not delete the existing tests. Correct and extend them so they become regression coverage for the actual product path.

---

# 2. Authority correction at start of execution

Before implementation work, update the active status documentation so another executor cannot treat Plan 122/123 as fully closed while this correction is in progress.

At minimum:

```text
plans/122-status.md
plans/123-status.md
README.md
AGENTS.md
```

Temporary execution state:

```text
plan_122 = corrective-reopened-plan124
plan_123 = provisional-blocked-on-plan122-correction
milestone6_local_product = not-closed
next = Plan 124
```

Do not erase prior evidence. Preserve the useful Plan 122 component results and Plan 123 streaming-core work; only correct the closure classification.

The following remain unchanged:

```text
plan_118 = closed
plan_119 = passed-leaseset2-protocol-foundation
plan_120 = passed-destination-lifecycle-and-pools
plan_121 = passed-ecies-destination-session-layer
plan_117 = closed-for-progression-with-evidence-gap
ntcp2 = experimental-non-advertised
normal_daemon_ntcp2 = disabled
```

---

# 3. Normative layering to preserve

The destination payload order is:

```text
application bytes
 -> I2NP Data message
 -> ECIES payload block / Garlic Clove
 -> ECIES New Session or Existing Session ciphertext
 -> I2NP Garlic message
 -> tunnel-message fragment/reassembly layer
 -> outbound tunnel transforms
 -> remote TUNNEL delivery instruction
```

The encrypted destination message must therefore be wrapped in the repository's standard I2NP Garlic carrier before it is handed to the destination outbound tunnel role.

The tunnel layer must remain ignorant of ECIES semantics. It receives encoded I2NP Garlic bytes just as it would receive any other I2NP message.

Do not:

```text
pass plaintext Data to the tunnel while returning ciphertext separately
teach i2pr-tunnel how to perform destination ECIES
put destination private/session state in i2pr-tunnel
add a client-to-client direct shortcut
use the local router-link seam before outbound tunnel processing
```

---

# 4. Phase A — make the encrypted Garlic carrier the tunnel payload

Primary file:

```text
crates/i2pr-client/src/routing.rs
```

Refactor `compose_outbound_delivery()` so one canonical value represents what is sent through the outbound tunnel.

Required sequence:

```text
request.inner_envelope
 -> standard-encode inner I2NP Data
 -> build Garlic Clove(s)
 -> EciesSessionManager encrypt_to_remote(...)
 -> encode ECIES New Session / Existing Session bytes
 -> wrap those bytes in I2npBody::Garlic(...)
 -> standard-encode the I2NP Garlic message
 -> forward_cells(..., garlic_i2np_bytes, ...)
```

Use the existing bounded `OpaqueMessageBody` / `DeferredPayload` carrier if that remains the repository's intended I2NP Garlic wire owner. Do not introduce a duplicate Garlic message type merely for Plan 124.

The I2NP Garlic message should receive an ordinary message id and expiration according to existing repository conventions. Keep those values deterministic in tests and caller-supplied/CSPRNG-backed where production semantics require it.

### OutboundDeliveryPlan correction

`OutboundDeliveryPlan` currently exposes both:

```text
inner_envelope_bytes
encrypted_message
cells
```

Retain diagnostic values only when they cannot be confused with the actual transmitted payload.

Recommended additions/changes:

```text
garlic_i2np_bytes: Vec<u8>
```

and document:

```text
cells are derived from garlic_i2np_bytes
inner_envelope_bytes are pre-encryption evidence only
encrypted_message is the ECIES body carried inside garlic_i2np_bytes
```

If a smaller public surface is cleaner, remove redundant raw-byte fields after tests are migrated. Do not retain duplicate sources of truth merely for tests.

---

# 5. Phase B — add a byte-identity regression at the OBEP boundary

Create a focused regression that would fail on the current source floor.

Required assertions after real outbound tunnel processing:

```text
OBEP delivery target router == selected Lease2 gateway
OBEP delivery tunnel id == selected Lease2 tunnel id
OBEP recovered I2NP message decodes as Garlic
Garlic opaque payload == OutboundDeliveryPlan.encrypted_message.message_bytes()
OBEP recovered message bytes == OutboundDeliveryPlan.garlic_i2np_bytes
OBEP recovered message bytes != OutboundDeliveryPlan.inner_envelope_bytes
```

The strongest invariant is:

> The only destination application plaintext below the ECIES boundary is inside authenticated ciphertext; the router/tunnel transport path carries an I2NP Garlic carrier, never the inner application `Data` message.

Do not satisfy the test by comparing two helper-produced buffers that never traverse the tunnel data plane. Recover the message from the actual outbound endpoint result.

---

# 6. Phase C — construct the missing successful A -> B path

Extend `crates/i2pr-client/tests/plan122_trajectory.rs` or create a new narrowly named integration test file if that makes the production path clearer.

Use two distinct destination identities A and B.

Each side needs:

```text
independent DestinationIdentity
independent EciesSessionManager
independent DestinationTunnelPool
real EstablishedMaterial for one usable outbound destination tunnel
real EstablishedMaterial for one usable inbound destination tunnel
validated signed Standard LeaseSet2
DestinationRouting state
DestinationDispatcher / local destination registration
```

Do not use a helper that bypasses `EstablishedMaterial`, `OutboundGatewayRole`, `InboundGatewayRole`, participant processing, or local endpoint recovery.

## Required A -> B trajectory

```text
A creates payload "hello"
 -> A has or resolves B validated LS2
 -> A selects a non-expired B Lease2
 -> A compose_outbound_delivery()
 -> outbound tunnel transforms/forwards I2NP Garlic
 -> OBEP emits typed TUNNEL delivery to B lease gateway/id
 -> explicit authenticated-router-link-bypassed-local-seam
 -> B inbound gateway receives TunnelGateway / payload
 -> B inbound tunnel data plane executes
 -> B local endpoint recovers exact I2NP Garlic
 -> owner mapping selects B only
 -> B DestinationDispatcher invokes B EciesSessionManager
 -> AEAD succeeds
 -> Garlic payload blocks are decoded
 -> bundled A LS2 is validated when present
 -> B application queue receives exactly "hello" once
```

The local seam is permitted only at the router transport boundary after A's outbound endpoint. It must pass the exact OBEP output to B's ingress representation without decrypting, re-encrypting, rewriting, or shortcutting the tunnel id.

Record the test/evidence label exactly as local:

```text
authenticated-router-link-bypassed-local-seam
```

Do not label it external interoperability.

---

# 7. Phase D — prove B -> A reply and Existing Session

The reverse path must not be a direct function call.

After B validates/learns A's LS2 from the authenticated initial exchange:

```text
B queues "world"
 -> B selects A Lease2
 -> B session manager emits NSR / correct reply state
 -> B outbound destination tunnel
 -> OBEP TUNNEL delivery to A inbound lease
 -> same explicit local router-link seam
 -> A inbound tunnel
 -> A owner dispatch
 -> A authenticates/decrypts
 -> A application receives exactly "world"
```

Then send one Existing Session application payload each direction.

Require:

```text
A -> B Existing Session exact-once
B -> A Existing Session exact-once
consumed tag cannot be reused
wrong destination context cannot decrypt
```

---

# 8. Phase E — inbound ownership and ciphertext isolation

The Plan 122 status currently claims destination-owned inbound routing. Turn that claim into a successful-path test rather than only malformed-input rejection.

Required checks:

```text
B inbound tunnel maps to B DestinationId
A cannot be selected as a decrypt candidate for B-owned inbound tunnel
removed/expired inbound tunnel has no owner mapping
owner mapping removal and role removal are atomic at composition layer
ciphertext is not broadcast across destination session managers
```

No trial-decryption across all local destination keys.

---

# 9. Phase F — stale LeaseSet / lease behavior

Before every send, ensure the selected Lease2 is still valid at caller-supplied time.

Required deterministic cases:

```text
expired LS2 -> send does not proceed
all leases inside safety margin -> no usable lease
newer valid LS2 replaces stale cache entry
send after refresh selects from the new record
X25519 key rotation does not silently reuse a session bound to a different key
```

If current session policy does not yet support key rotation, fail closed and require a new session. Do not silently preserve a stale static-key binding.

---

# 10. Phase G — fault tests

Add bounded failures for at least:

```text
tampered ECIES ciphertext
wrong remote tunnel id
unknown/removed inbound owner
full destination application queue
lookup timeout / no usable LS2
malformed I2NP Garlic carrier
Garlic body that authenticates but contains malformed payload blocks
bundled LS2 whose static key conflicts with authenticated New Session binding
```

Each failure must:

```text
return a typed error/outcome
not deliver application plaintext
not leak to another destination
not allocate unbounded state
not poison a valid unrelated ECIES session
```

---

# 11. Tests that must be removed or strengthened

Do not delete useful component tests, but the following pattern is no longer acceptable as closure evidence:

```rust
let _ = (request, session, outbound_role, ...);
```

A test named as an outbound composition test must execute the composer and inspect the actual result.

Similarly, a malformed dummy Garlic rejection test is useful negative coverage but cannot stand in for successful inbound dispatch.

Plan 124 closure requires at least one real successful production-seam round trip.

---

# 12. Validation commands

Run at minimum:

```bash
cargo +1.95.0 fmt --all --check
cargo +1.95.0 check --locked --workspace --all-targets
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

Do not make the retired rootless-supervisor path a Plan 124 acceptance requirement.

---

# 13. Documentation/status synchronization on success

On successful closure update:

```text
plans/122-status.md
plans/123-status.md
README.md
AGENTS.md
docs/architecture/i2pr-client.md
docs/architecture/i2pr-tunnel.md
docs/protocol-support.md
specs/support.toml
```

Required final Plan 122 classification:

```text
plan_122 = passed-corrected-local-destination-routing
plan_122_transport_boundary = authenticated-router-link-bypassed-local-seam
plan_122_external_interop = not-claimed
```

Plan 123 must remain provisional until Plan 125 executes:

```text
plan_123 = provisional-awaiting-plan125-correction
```

Do not restore `passed-minimal-streaming-core` merely because Plan 124 fixes its lower layer.

---

# 14. Explicit acceptance criteria

Plan 124 is complete only when all are true:

- [ ] `compose_outbound_delivery()` feeds encoded I2NP Garlic bytes, not plaintext inner I2NP Data bytes, into the outbound tunnel role.
- [ ] The ECIES message bytes are carried inside the I2NP Garlic body that traverses the tunnel.
- [ ] A regression test proves actual OBEP-recovered bytes equal the composed I2NP Garlic carrier and differ from the plaintext inner Data envelope.
- [ ] The selected Lease2 gateway and tunnel id survive to the OBEP TUNNEL delivery unchanged.
- [ ] A successful A -> B New Session trajectory uses real destination-owned outbound and inbound tunnel roles.
- [ ] The only network omission is the explicit local seam after outbound endpoint processing and before remote router ingress.
- [ ] The local seam does not decrypt/re-encrypt/rewrite destination payloads or tunnel identity.
- [ ] B dispatches the recovered ciphertext to B's destination context only.
- [ ] B authenticates/decrypts before delivering exact application payload bytes.
- [ ] Bundled A LeaseSet2 validation/static-key binding is exercised successfully.
- [ ] B -> A reply traverses the symmetric routing architecture.
- [ ] Existing Session messages work in both directions over that architecture.
- [ ] Replay/tag reuse/wrong-owner/tamper/stale-lease/capacity tests fail boundedly.
- [ ] Existing Plan 119/120/121 functionality remains green.
- [ ] No NTCP2/SSU2 activation, SAM, I2CP socket API, HTTP/SOCKS proxy, Python harness, Docker, namespace, VM, or public-I2P work is introduced.
- [ ] Workspace tests, clippy, docs, and boundary scripts are green.
- [ ] Status authority is synchronized and Plan 123 remains provisional for Plan 125.

## Handoff on success

```text
plan_124 = passed-plan122-corrective-closure
plan_122 = passed-corrected-local-destination-routing
plan_123 = provisional-awaiting-plan125-correction
milestone6_local_product = not-yet-closed
next = plans/125-m6-streaming-corrective-and-local-closure.md
```
