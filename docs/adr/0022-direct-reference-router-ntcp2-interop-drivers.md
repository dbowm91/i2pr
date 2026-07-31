# ADR 0022: Direct reference router NTCP2 interoperability drivers

- Status: Accepted
- Date: 2026-07-30
- Decision owner: repository maintainer
- Parent roadmap: Plan 061
- Parent plan: Plan 062
- Supersedes: conclusion of ADR 0021 (rejected by Plan 058, recorded for the
  purposes of explicit supersession)

## Context

ADR 0021 (`docs/adr/0021-minimal-java-support-topology.md`) was
Rejected by the Plan 058 record and candidate integrity closure
pass. The rejection closed the four-direction Milestone 3 contract
against the pinned Java I2P 2.12.0 revision
(`2800040deee9bb376567b671ef2e9c34cf3e30b6`) under the previous
`java-minimal-support-topology` decision. The current Plan 060
candidate is therefore `declared-not-executable` on this host.

Plan 061 re-opens the four-direction contract by replacing the
SAM/I2CP/support-topology premise with two-process direct transport
drivers for Java I2P and i2pd. The Plan 062 source-verification
record (`tests/integration/ntcp2/reference-drivers/source-verification.md`)
inspected the exact pinned source revisions and documented every
API the future drivers must call.

The Plan 062 evidence contract correction requires the same
drivers to emit Plan 062 `i2pr-reference-event-v1` records,
64-hex SHA-256 Router Hashes, exact per-run DeliveryStatus
correlation, and a strict two-process topology with no support
router, SAM, I2CP, floodfill, reseed, or tunnel pool.

## Decision

ADR 0022 is **Accepted**. The primary NTCP2 interoperability
topology uses two-process direct transport drivers that exercise
the pinned Java I2P 2.12.0 and i2pd 2.60.0 reference routers
without SAM, I2CP, floodfill, reseed, tunnels, support routers, or
external public-network paths. The drivers are test-only artifacts
under `tests/integration/ntcp2/reference-drivers/` and never become
production dependencies of `i2pr-daemon` or any lower production
crate.

### Java I2P 2.12.0 — direct stripped-router driver

The Java driver must use the upstream stripped-router architecture
(an embedded `Router` plus dummy client/NetDB/peer-manager/tunnel
facades) to run the real NTCP/NTCP2 transport with a directly
imported peer RouterInfo and a real `OutNetMessage` DeliveryStatus
dispatch. The driver must:

1. construct an embedded `net.i2p.router.Router` with the
   Plan 062 mandatory property set (network ID 99, NTCP enabled,
   UDP/UPnP/reseed disabled, synthetic host/port, dummy facades
   enabled);
2. invoke `Router.runRouter()` from a dedicated thread with a
   bounded readiness deadline;
3. verify the local signed `RouterInfo` (signature, network ID,
   Router Hash, address, NTCP2 `s` key, NTCP2 `i` IV);
4. write the local `RouterInfo` to the declared exchange path;
5. import the peer `RouterInfo` via `DummyNetworkDatabaseFacade.store(...)`
   and verify `lookupRouterInfoLocally(...)` returns the same
   identity and digest;
6. dispatch a real `OutNetMessage` carrying a `DeliveryStatusMessage`
   with the exact per-run message ID via `OutNetMessagePool.add(...)`;
7. register a receive handler on `InNetMessagePool` for
   `DeliveryStatusMessage.MESSAGE_TYPE` (constant value 10) and
   verify the decoded message ID plus the sender `Hash`;
8. emit Plan 062 `i2pr-reference-event-v1` records for
   `process_started`, `listener_ready`, `router_info_exported`,
   `peer_router_info_validated`, `tcp_connected`,
   `ntcp2_authenticated`, `frame_emitted` (dial),
   `frame_authenticated_and_decrypted` (listen),
   `i2np_message_decoded` (listen), and `terminal_clean`;
9. shut down the embedded router through the upstream lifecycle
   (no `System.exit` inside the driver).

The driver must not implement, bypass, or patch NTCP2 cryptography,
handshake transcript, frame encoding, signature verification, or
transport acceptance policy. The dummy facades are the only
client/NetDB/peer-manager/tunnel substitution; the NTCP/NTCP2
transport is the unmodified pinned code path.

### i2pd 2.60.0 — direct driver with passive observer

The i2pd driver must use the pinned i2pd context, the real NTCP2
transport, a directly imported peer `RouterInfo`, and a
compile-time-gated passive observer placed after successful AEAD
verification and `FromNTCP2`-equivalent I2NP conversion. The driver
must:

1. initialize the pinned i2pd context in the source-verified order
   (configuration, filesystem, crypto, local router context,
   transport singleton, NetDB, NTCP2-only transports, router
   context services for DeliveryStatus dispatch);
2. verify the local signed `RouterInfo` (signature, network ID 99,
   exact local Router Hash, NTCP2 IPv4 address, exact synthetic
   host/port, NTCP2 `s` key, NTCP2 `i` IV);
3. write the local `RouterInfo` to the declared exchange path;
4. import the peer `RouterInfo` via `i2p::data::netdb.AddRouterInfo(...)`
   and verify `i2p::data::netdb.FindRouter(expected_ident_hash)`
   returns the same identity and digest;
5. construct a real `DeliveryStatus` I2NP message via
   `i2p::data::CreateDeliveryStatusMsg(message_id)` and submit
   through `i2p::transport::transports->SendMessage(target_ident_hash,
   message_ptr)` exactly once;
6. wait boundedly for the established NTCP2 session and the sender
   observer completion;
7. emit Plan 062 `i2pr-reference-event-v1` records with the exact
   message ID, peer Router Hash, frame sequence, and bytes
   transferred;
8. shut down in strict reverse ownership order (router context
   services, transports, NetDB, crypto).

The passive observer must:

- be guarded by the `I2PD_INTEROP_OBSERVER` macro so an
  uninstrumented control build remains source-identical to the
  unmodified pinned tree (modulo the macro expansion);
- be placed only after successful AEAD verification, block-bounds
  validation, and NTCP2-to-I2NP conversion (the receiver seam) or
  after successful asynchronous socket write for the frame
  containing the exact DeliveryStatus message (the sender seam);
- return `void`, be `noexcept`, never block transport threads on
  unbounded I/O, write only to an owned bounded sink, and expose no
  raw payload bytes, private keys, Noise state, frame keys, IV
  state, or transcript.

The driver must not patch NTCP2 cryptography, handshake transcript,
framing, signature verification, or transport acceptance policy.
The pinned tree wins over current upstream examples; if the pinned
APIs differ from upstream, the Plan 062 source-verification record
governs.

### Two-process topology and isolation

Each primary direction runs exactly:

- one i2pr process;
- one reference driver process;
- one rootless sealed network namespace (Plan 046) or one
  equivalently isolated Multipass guest (Plan 048/049);
- one veth pair;
- two synthetic IPv4 addresses (`192.0.2.1` and `192.0.2.2`);
- private network ID 99;
- no default route, no DNS, no reseed, no public egress.

There is no support router, no SAM trigger, no I2CP trigger, no
HTTP/I2PControl trigger, no floodfill, no tunnel pool, no SAM
streaming, and no third router process in the primary topology.
The Plan 046 rootless sealed-namespace lane or the Plan 048/049
Multipass recovery lane enforces isolation authority.

### Observer constraints

The i2pd passive observer is:

- compile-time gated (`I2PD_INTEROP_OBSERVER` macro);
- behavior-neutral (the instrumented binary may differ from the
  uninstrumented binary only in observation output);
- passive (no return-path alteration, no control-flow change, no
  protocol-state change);
- bounded (owned sink; drop with a typed local counter on sink
  unavailability rather than altering transport behavior).

The observer must not gate, retry, queue, or alter the
`SendMessage` return value. The uninstrumented control build proves
behavioral equivalence through identical sealed i2pd-to-i2pd control
scenarios.

### Rejected alternatives (preserved for audit)

The following alternatives remain rejected by Plan 062:

- **SAM v3 streaming trigger.** Plan 061 explicitly rejects SAM and
  HTTP triggers because SAM requires a registered destination and an
  outbound tunnel pool, neither of which is permitted by the two-process
  primary topology.
- **HTTP/I2PControl trigger.** I2PControl cannot prove
  authenticated transport and is forbidden by Plan 055 Workstream A.
- **Generic log parsing.** Plan 062 requires structured events;
  generic log phrases cannot satisfy the data-phase predicate.
- **Full private I2P mini-network.** A multi-router network
  introduces extra moving parts that Plan 055 D2 forbids; the
  two-process topology is the smallest topology that meets the source
  prerequisites.
- **Cryptography patches.** Plan 055 Workstream A rule 1 forbids any
  patch of pinned cryptographic behavior.
- **Future-upstream-version dependency.** The pinned revisions are
  authoritative; re-pinning requires a separate explicit ADR.

### Consequences

- The Plan 062 v4 trigger schema
  (`tests/integration/ntcp2/harness/reference_trigger_v4.py`) is the
  authoritative schema for the four primary directions.
- The Plan 062 reference-event schema
  (`tests/integration/ntcp2/harness/reference_event.py`) is the
  authoritative per-direction event shape.
- 64-hex SHA-256 Router Hash replaces the Plan 055 40-hex SHA-1
  width across the active schema.
- Exact per-run DeliveryStatus message ID is mandatory for every
  primary direction.
- Plan 060 candidate is retired and cannot be active execution
  authority.
- Future candidate implementation floor must be Plan 065 closure or
  later.
- Reference drivers and observers are test/integration code only;
  they never become production dependencies.

### Reference prerequisites (preserved for audit)

The Plan 062 source-verification record
(`tests/integration/ntcp2/reference-drivers/source-verification.md`)
records the exact pinned Java I2P 2.12.0 and i2pd 2.60.0 APIs. The
ADR explicitly relies on that record; any divergence between the
ADR and the verification record is resolved in favor of the
verification record.

## Review triggers

A new ADR must accept any future change to the two-process direct
topology. Triggers include:

- a future pinned Java revision exposes a different
  `NTCPTransport` constructor signature;
- a future pinned i2pd revision removes `Transports::SendMessage`
  or the `I2PD_INTEROP_OBSERVER` macro seam becomes impossible
  without behavior change;
- a future harness requirement mandates SAM, I2CP, or HTTP triggers
  for primary directions;
- the Plan 046 rootless sealed-namespace lane or the Plan 048/049
  Multipass recovery lane is replaced by an isolation topology that
  cannot enforce the no-public-egress contract.

## References

- `plans/061-ntcp2-direct-reference-driver-corrective-roadmap.md`
- `plans/062-ntcp2-evidence-contract-and-architecture-correction.md`
- `plans/063-java-i2p-stripped-router-direct-ntcp2-driver.md`
- `plans/064-i2pd-direct-ntcp2-driver-and-observer-correction.md`
- `plans/076-real-pinned-i2pd-library-and-direct-driver-construction.md`
- `plans/065-ntcp2-canonical-integration-and-live-qualification.md`
- `plans/066-fresh-candidate-and-authoritative-ntcp2-two-run-closure.md`
- `docs/adr/0021-minimal-java-support-topology.md`
- `tests/integration/ntcp2/reference-drivers/source-verification.md`
- `tests/integration/ntcp2/harness/reference_trigger_v4.py`
- `tests/integration/ntcp2/harness/reference_event.py`
