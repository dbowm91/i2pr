# Plan 120 — Milestone 6 destination lifecycle and dedicated tunnel pools

## Status

- **Ready after Plan 119 closes**.
- Date: **2026-08-19**.
- Parent roadmap: [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md).
- Predecessor: Plan 119 Standard LeaseSet2 foundation.
- Primary product output: the first real `i2pr-client` destination runtime.
- Garlic encryption and remote destination delivery remain Plan 121/122 work.

## Objective

Create `i2pr-client` as the owner of local Destination lifecycle, destination
private keys, destination-specific tunnel demand, LeaseSet2 lifecycle, and
application-facing destination message queues.

At closure, a local destination must be able to:

```text
start
 -> own isolated signing + X25519 encryption keys
 -> request/maintain dedicated inbound and outbound tunnel pools
 -> observe established inbound tunnel metadata
 -> construct Lease2 entries
 -> build and sign a valid Standard LeaseSet2
 -> rotate/republish that LS2 as tunnels change
 -> shut down and release all destination-owned state
```

No Garlic session is required yet. The plan establishes the runtime object that
Plan 121 will attach ECIES state to and Plan 122 will use for actual destination
routing.

---

# 1. Crate and dependency boundary

Add the workspace crate anticipated by the MVP architecture:

```text
crates/i2pr-client/
```

Intended dependency direction:

```text
i2pr-client
  -> i2pr-core
  -> i2pr-proto
  -> i2pr-crypto
  -> i2pr-netdb
  -> i2pr-tunnel

NOT:
i2pr-client -> i2pr-daemon
i2pr-tunnel -> i2pr-client
i2pr-netdb -> i2pr-client
```

`i2pr-daemon` remains the future composition root. Plan 120 may add a minimal
daemon integration seam only if needed to prove ownership/lifecycle, but client
policy must remain in `i2pr-client`.

Update `scripts/check-dependency-direction.sh` only to encode the intended new
edge; do not weaken unrelated boundaries.

Suggested initial modules:

```text
lib.rs
identity.rs
registry.rs
pool.rs
leaseset.rs
message.rs       // local bounded message contracts only; no Garlic yet
```

Exact names may follow repository conventions.

---

# 2. Destination identity and secret ownership

Introduce an explicit local destination identity owner.

It must own at least:

```text
Destination public structure
Destination signing private key
Destination signing public key
static X25519 private key for ECIES destination encryption
static X25519 public key for LeaseSet2
local Destination hash / identifier
```

Do not conflate router identity with destination identity.

Required invariants:

1. destination signing keys are independent of router signing keys;
2. destination X25519 static keys are independent of router NTCP2/tunnel keys;
3. two local destinations never share private key objects implicitly;
4. private key material is not `Debug`-revealing;
5. secret types should zeroize on drop where the underlying primitive permits;
6. key cloning is minimized and prohibited at public lifecycle boundaries unless
   a concrete use requires it;
7. public identity can be cloned/serialized independently from secrets.

Use existing `i2pr-crypto` wrappers where possible. Do not introduce a second
X25519 implementation.

For this plan, destination key persistence is optional unless the existing
storage architecture makes it cheap and clean. If persistence is deferred,
state that clearly and make ephemeral destination creation deterministic in
tests. Do not block the destination runtime on a new encrypted-keystore project.

---

# 3. Destination identifiers and handles

Create a non-secret handle suitable for daemon/API ownership later.

Conceptual shape:

```text
DestinationId(Hash)
DestinationHandle
DestinationCommand
DestinationEvent
```

Use the existing service/lifecycle patterns from `i2pr-core` rather than
inventing a client-specific supervision framework.

The public handle should expose only capabilities required by callers, such as:

```text
identity / destination hash query
state query
request shutdown
enqueue application payload (may remain not-routable until Plan 122)
subscribe/receive local payload event
```

Do not expose private key references, tunnel layer keys, or mutable pool internals.

---

# 4. Bounded destination registry

Implement a router-local registry of destination runtimes.

Required controls:

```text
maximum local destinations
maximum aggregate destination command queue depth
maximum per-destination pending application messages
maximum per-destination pending bytes
explicit duplicate Destination hash rejection
```

The registry must not be an unbounded `HashMap<Hash, Arc<...>>` with hidden
lifetime retention.

Required lifecycle states should be explicit, for example:

```text
Initializing
BuildingTunnels
Usable
Degraded
Stopping
Stopped
```

Avoid a large state enum if existing service health contracts already express
part of this. The important point is deterministic readiness: a destination is
not `Usable` merely because keys exist.

---

# 5. Destination-specific tunnel pools

The current `ExploratoryPool` proves the tunnel substrate but is semantically
router-wide. Plan 120 must add destination-specific pool ownership without
forking tunnel cryptography/data-plane code.

Preferred architecture:

```text
shared i2pr-tunnel tunnel builder / established tunnel representations
        |
        +-- ExploratoryPool (router NetDB)
        |
        +-- DestinationTunnelPool (i2pr-client ownership/policy)
```

The client pool may be implemented as a new reusable generic pool type in
`i2pr-tunnel` if exploratory and destination policy can share lifecycle logic
cleanly. Do not move destination policy into `i2pr-tunnel` merely for reuse.

Minimum pool policy:

```text
inbound target count       configurable/bounded
outbound target count      configurable/bounded
minimum usable count       explicit
replacement-before-expiry  supported
failed build accounting    bounded
expiry                     deterministic
selection                   deterministic from caller RNG/policy
```

Default test topology may use one or two tunnels each direction; do not encode
MVP test counts as protocol constants.

---

# 6. Reuse real `EstablishedMaterial`

Destination pools must consume the same successful short-build material used by
the exploratory runtime.

Do not create placeholder established tunnels in production.

Required trajectory:

```text
ShortBuildStateMachine Established
 -> take_established_material()
 -> destination pool registration/activation
 -> destination-owned inbound/outbound usable tunnel
```

Preserve one-shot secret ownership.

If `ExploratoryPool` currently contains functionality that should be generalized
(e.g. slot lifetime, activation ownership, expiry), extract the smallest shared
abstraction rather than copy/paste the entire pool.

Any generalization must keep Plan 116/117 tests green.

---

# 7. Inbound lease derivation

A published LeaseSet2 advertises inbound destination tunnels.

For each usable inbound tunnel, expose the public routing metadata needed to
construct a `Lease2`:

```text
tunnel gateway RouterIdentity hash
gateway receive TunnelId
lease expiration seconds
```

Secret tunnel layer keys must never enter the LeaseSet2 builder.

Define the mapping carefully from the creator's established inbound path:

```text
remote first hop = inbound gateway advertised in Lease2
its receive tunnel ID = Lease2 tunnel_id
lease expiration <= actual tunnel usability window
```

Never advertise a lease past the local pool's own tunnel expiry.

Plan 120 should choose a conservative publication expiration offset that leaves
room for propagation and replacement. Keep the exact policy configurable or in a
single documented constant rather than scattering timestamps.

Required tests:

```text
inbound_established_tunnel_yields_correct_lease2_gateway
lease2_tunnel_id_matches_gateway_receive_id
lease2_expiry_does_not_exceed_tunnel_expiry
failed_or_expired_tunnel_not_advertised
replacement_tunnel_enters_next_leaseset
```

---

# 8. Local LeaseSet2 generation and signing

Use Plan 119's unsigned/canonical LS2 construction path.

Required flow:

```text
DestinationIdentity public Destination
 + destination static X25519 public key
 + current usable inbound Lease2 list
 + published/expires policy
 -> canonical unsigned Standard LeaseSet2
 -> signature preimage 0x03 || unsigned LS2 bytes
 -> sign with Destination signing private key
 -> finalized typed LeaseSet2
 -> self-verify through i2pr-netdb validation
```

The result is not considered publication-ready until it passes the same
validation used for received LS2 entries.

This self-validation catches construction/verification drift.

The client layer should own the private signing operation. Do not put private
key access into `i2pr-proto` or `i2pr-netdb`.

---

# 9. LeaseSet2 lifecycle

Introduce a small state machine for local LS2 lifecycle.

Required states/causes:

```text
no usable inbound tunnels
initial LS2 ready
publication requested/pending (typed event only; network composition Plan 122)
published-at / last generated version
replacement required because tunnel set changed
replacement required before lease expiry
stopping: do not generate new LS2
```

The `published` timestamp has one-second resolution. Ensure regenerated LS2
versions do not accidentally reuse a non-increasing published timestamp where
that would prevent NetDB replacement.

Use a caller-provided deterministic clock.

Do not wall-clock sleep in tests.

Required tests:

```text
first_usable_inbound_set_generates_ls2
same_state_does_not_regenerate_unboundedly
replacement_has_monotonic_published_time
pool_change_rotates_ls2
approaching_expiry_rotates_ls2
zero_usable_inbound_tunnels_marks_destination_not_publishable
```

---

# 10. Application payload queues without routing

Define the bounded local message contracts Plan 122 will consume.

For example:

```text
DestinationPayload {
    protocol: u8/u16 as appropriate,
    from/to metadata only when protocol requires,
    bytes: bounded Vec<u8>,
}
```

Do not invent the streaming protocol here.

A destination may accept a caller's pending outbound payload into a bounded
queue, but because remote LS2 resolution and Garlic are not yet implemented it
must return a typed `RoutingUnavailable`/`NotImplementedUntilPlan122` state or
retain only the explicitly bounded test-facing queue.

Never directly inject plaintext into tunnel delivery as a temporary shortcut.
That would become architectural debt and bypass Plan 121.

---

# 11. Shutdown and failure cleanup

Destination shutdown must release:

```text
private identity owner
pending application messages
destination tunnel-pool registrations
runtime data-plane roles owned solely by that destination
LeaseSet lifecycle state
future ECIES session slot (empty in Plan 120)
```

A pool build failure must not kill the router service. It should degrade the
individual destination and schedule bounded replacement according to policy.

A destination with insufficient tunnels must not advertise a stale/invalid LS2
as if it were healthy.

Tests:

```text
destination_shutdown_releases_pool_entries
build_failure_degrades_not_panics
failed_pool_replacement_is_bounded
registry_removal_drops_destination_state
one_destination_failure_does_not_mutate_other_destination
```

---

# 12. Deterministic full local trajectory

Add one integration test using real production seams up to the point this plan
owns.

Required trajectory:

```text
create Destination A
 -> construct real inbound + outbound ShortBuild state machines
 -> transition them to Established using existing deterministic responder/reference-safe local fixture
 -> transfer real EstablishedMaterial
 -> activate destination tunnel pools
 -> derive Lease2 entries from inbound public routing metadata
 -> construct and sign Standard LeaseSet2
 -> validate through i2pr-netdb
 -> mark Destination A usable/publication-ready
 -> advance deterministic time toward tunnel expiry
 -> replace inbound tunnel
 -> generate newer LS2 with new lease set
 -> shut down
 -> verify pool/registry/resource cleanup
```

Do not use placeholder tunnel secrets or hand-built LS2 bytes.

---

# 13. Resource defaults

Every default should be centralized and test-overridable.

At minimum define limits for:

```text
max local destinations
max inbound tunnels per destination
max outbound tunnels per destination
max pending destination messages
max pending destination bytes
max simultaneous builds/replacements per destination
```

The exact production defaults may remain conservative/experimental. Acceptance
is about explicit bounds and behavior, not optimizing throughput yet.

---

# 14. Documentation and support updates

On closure update:

```text
README.md
AGENTS.md
docs/architecture/i2pr-client.md        (new, preferred)
docs/architecture/i2pr-tunnel.md        only where shared pool changes require it
docs/protocol-support.md / current support authority
specs/support.toml
plans/120-status.md only if existing project convention requires a concise closure record
```

Avoid creating separate corrective/status/handoff documents unless a real defect
requires them. The implementation commit and one concise status record are
preferred.

---

# 15. Validation commands

At minimum:

```bash
cargo fmt --all --check
cargo test --locked -p i2pr-client --all-targets
cargo test --locked -p i2pr-tunnel --all-targets
cargo test --locked -p i2pr-netdb --all-targets
cargo test --locked --workspace
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
```

No live router interoperability lane is required.

---

# 16. Explicit acceptance criteria

Plan 120 is complete only when:

- [ ] `crates/i2pr-client` exists and follows the intended dependency direction.
- [ ] A local destination owns independent signing and static X25519 secrets.
- [ ] Secret-bearing destination values do not reveal bytes via `Debug`.
- [ ] At least two local destinations can coexist without shared secret/session
      or tunnel-pool state.
- [ ] The destination registry is explicitly bounded.
- [ ] Destination inbound/outbound tunnel counts and build/replacement work are
      bounded.
- [ ] Destination pools consume real one-shot `EstablishedMaterial`.
- [ ] No production placeholder established tunnel path is introduced.
- [ ] A usable inbound tunnel maps to the correct Lease2 gateway hash and receive
      tunnel ID.
- [ ] Advertised Lease2 expiry never exceeds actual tunnel usability.
- [ ] A destination constructs a canonical Standard LeaseSet2 containing its
      X25519 type-4 public key.
- [ ] LS2 is signed with the destination signing private key and self-validates
      through `i2pr-netdb`.
- [ ] LS2 `published` values advance deterministically when replacement is
      required.
- [ ] Tunnel failure/expiry rotates destination LS2 state without leaking stale
      leases.
- [ ] Destination shutdown releases pool/registry/message state.
- [ ] A deterministic production-seam integration test covers startup -> real
      tunnels -> LS2 -> rotation -> shutdown.
- [ ] No Garlic encryption, plaintext tunnel-delivery shortcut, SAM, I2CP,
      streaming, or external transport activation is added.
- [ ] Workspace validation is green.

## Handoff

On closure:

```text
plan_120 = passed-destination-lifecycle-and-pools
local_destination = keys+tunnels+signed-ls2-ready
next = plans/121-m6-ecies-garlic-session-layer.md
```
