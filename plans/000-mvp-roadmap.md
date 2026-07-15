# i2pr MVP roadmap

## Document purpose

This roadmap defines the path from an empty repository to the first feature-complete `i2pr` MVP: a CLI-first Rust I2P router with NTCP2 and SSU2, NetDB and floodfill behavior, network tunnel construction and participation, destinations and streaming, SAM and I2CP, and basic HTTP, SOCKS5, IRC, client, and server service tunnels.

This document is architectural and milestone-oriented. Each milestone should receive one or more detailed implementation plans before code is written.

## MVP definition

The MVP is complete only when `i2pr` can:

- Start as a foreground CLI daemon with validated configuration and persistent identity.
- Reseed and join an I2P network.
- Establish and accept interoperable NTCP2 and SSU2 sessions.
- Parse, validate, route, and originate required I2NP messages.
- Maintain a bounded NetDB containing validated RouterInfo and LeaseSet records.
- Perform NetDB lookups and publications.
- Construct and maintain exploratory and destination tunnel pools.
- Participate in transit tunnels under explicit resource policy.
- Operate as a floodfill router when configured and eligible.
- Create local destinations and publish LeaseSets.
- Provide interoperable I2P streaming.
- Serve SAM and I2CP clients.
- Provide HTTP and SOCKS5 client proxies.
- Provide generic TCP client and server tunnels.
- Provide IRC client and IRC server profiles or adapters.
- Shut down cleanly, recover from restart, and remain bounded under malformed or excessive input.
- Demonstrate sustained interoperability in a mixed-router testnet.

The MVP is not considered production-ready merely by satisfying this functional list. A later production-readiness effort will require broader audits, long-duration public-network observation, operational hardening, packaging, and independent review.

## Non-goals for the MVP

The MVP does not require:

- A web console, desktop GUI, or TUI.
- BitTorrent, mail, address-book applications, or bundled eepsite hosting software.
- Clearnet outproxy operation.
- Runtime-loadable third-party plugins.
- SSU1 or legacy NTCP1 support.
- Mobile platform support.
- Browser integration.
- Automatic UPnP or NAT-PMP support unless required to close SSU2 reachability.
- Formal anonymity proofs.
- Protocol experimentation enabled by default.
- Synvoid or eggsec as mandatory runtime dependencies.

## Cross-cutting requirements

Every milestone must preserve the guardrails in `GUARDRAILS.md`.

All untrusted-input paths must have explicit limits. Long-lived services must be supervised. Queues must be bounded. State machines must have cancellation and cleanup behavior. Protocol implementation must include negative and malformed-input testing, not only successful exchanges.

Each milestone must update the protocol support matrix, architecture documentation, and known-limitations documentation as appropriate.

## Target workspace dependency direction

The intended workspace is:

```text
crates/
  i2pr-proto
  i2pr-crypto
  i2pr-core
  i2pr-transport
  i2pr-transport-ntcp2
  i2pr-transport-ssu2
  i2pr-netdb
  i2pr-tunnel
  i2pr-client
  i2pr-api
  i2pr-service-tunnels
  i2pr-storage
  i2pr-daemon
  i2pr-testkit
```

The dependency direction should remain acyclic and generally follow:

```text
proto
  ├── crypto wrappers
  ├── transport implementations
  ├── netdb
  ├── tunnel
  └── client

core contracts
  ├── transport manager
  ├── netdb
  ├── tunnel
  ├── client
  ├── storage adapters
  └── daemon composition

client
  ├── API adapters
  └── service tunnels
```

`i2pr-daemon` is the composition root. Lower crates must not depend on it.

## Milestone 0: Repository foundation and project skeleton

### Objective

Create a buildable, auditable Rust workspace with initial crate boundaries, development policy, CI, linting, dependency controls, architecture documentation, and test infrastructure placeholders.

### Required outcomes

- Root workspace manifest and toolchain policy.
- Initial crate skeletons with `#![forbid(unsafe_code)]` where required.
- CLI daemon placeholder that supports `--help`, `--version`, and a non-networked `check-config` path.
- Configuration schema placeholder with strict unknown-field handling.
- Typed error conventions.
- Workspace lint configuration.
- Formatting, clippy, tests, documentation, and dependency-audit CI.
- Initial architecture dependency diagram.
- Protocol support matrix initialized to “not implemented.”
- Testkit crate with controllable clock and deterministic RNG interfaces, even if initial implementations are minimal.
- ADR process and initial architecture decisions.

### Exit criteria

- Clean checkout builds and tests on supported development platforms.
- CI enforces formatting, clippy, tests, documentation, and dependency policy.
- Dependency direction is documented and mechanically checked where practical.
- No network listeners or protocol claims are introduced.

## Milestone 1: Protocol model, codecs, identity, and storage primitives

### Objective

Implement the trustworthy foundation for I2P data representation, strict parsing, signing, identity persistence, and test vectors.

### Scope

- Integer and string primitives used by I2P encodings.
- Mapping and certificate formats.
- RouterIdentity and Destination.
- RouterAddress and RouterInfo.
- Lease, LeaseSet variants required by later milestones.
- I2NP message envelope and initial required message types.
- Canonical serialization and strict decoding.
- Signature verification and protocol-specific key wrappers.
- Atomic encrypted or permission-hardened router identity storage.
- Persistent format versioning.
- Golden vectors, malformed vectors, property tests, and fuzz targets.

### Security requirements

- Explicit maximum sizes and counts.
- Full input consumption on strict decoders.
- No secret-bearing `Debug` output.
- No untrusted data accepted from disk without revalidation.
- No locally implemented cryptographic primitives.

### Exit criteria

- Round-trip and golden-vector tests pass.
- Malformed and boundary inputs fail deterministically without panic.
- Router identity can be generated, saved atomically, reloaded, and used to sign a RouterInfo.
- Fuzz harnesses cover the initial public parsers.

## Milestone 2: Core service framework and deterministic network testkit

### Objective

Establish the lifecycle, resource, communication, and simulation infrastructure used by all router subsystems.

### Scope

- Service handle/task/health conventions.
- Cancellation and shutdown tokens.
- Essential, restartable, degradable, and optional service classifications.
- Bounded command and event channels.
- Router-wide resource governor and scoped leases.
- Clock abstraction limited to protocol/state-machine needs.
- Deterministic seeded randomness for tests.
- In-memory stream and datagram links.
- Fault injection for loss, delay, duplication, reordering, truncation, and disconnect.
- Test peer and identity factories.
- Structured privacy-aware tracing conventions.

### Exit criteria

- A simulated service graph starts, reports readiness, handles cancellation, and shuts down without leaked tasks.
- Resource leases are released on success, error, timeout, and cancellation.
- Tests can run without wall-clock sleeps.
- Fault-injected links produce reproducible results from a recorded seed.

## Milestone 3: NTCP2 transport and transport-neutral link management

### Objective

Implement the first interoperable router-to-router transport and establish transport-neutral delivery contracts.

### Scope

- NTCP2 RouterInfo address parsing and publication fields.
- Incoming and outgoing handshake state machines.
- Replay protection and clock-skew policy.
- Framing, padding, encryption, and message blocks.
- Connection replacement and duplicate handling.
- Bounded read/write queues.
- Transport-neutral peer link handle.
- Dial policy, backoff, timeout, and connection limits.
- Delivery outcome events.
- Address and reachability observations emitted without direct NetDB mutation.

### Exit criteria

- `i2pr` completes NTCP2 handshakes with at least two independent router implementations in an authorized testnet.
- Required I2NP messages can be exchanged over an authenticated link.
- Malformed handshakes, replay attempts, excessive padding, oversized frames, and slow peers are rejected within bounded resources.
- Connection shutdown releases all tasks, buffers, and resource leases.

## Milestone 4: Reseeding, NetDB client, and RouterInfo publication

### Objective

Allow `i2pr` to acquire initial peers, maintain validated network data, query the distributed database, and publish its RouterInfo.

### Scope

- Reseed bundle acquisition and signature verification.
- Multiple reseed source policy and failure handling.
- Validated RouterInfo storage with expiry and quotas.
- Peer selection primitives separated from stored observations.
- DatabaseLookup, DatabaseStore, DatabaseSearchReply, and delivery-status handling.
- Query state machines with deadlines, retries, deduplication, and cancellation.
- RouterInfo publisher driven by validated address snapshots.
- Publication verification and republish policy.
- Persistence and corruption recovery for NetDB cache and peer profiles.

### Exit criteria

- Fresh `i2pr` instance can reseed and populate a bounded RouterInfo database.
- RouterInfo lookups complete through live peers in a mixed-router testnet.
- Local RouterInfo is published and independently verified.
- Invalid signatures, stale entries, conflicting data, and storage corruption are handled safely.

## Milestone 5: Network tunnel data plane and exploratory tunnels

### Objective

Implement unidirectional I2P network tunnel construction and message forwarding sufficient for exploratory NetDB traffic.

### Scope

- Tunnel IDs and tunnel message formats.
- Tunnel gateway batching.
- Fragmentation and reassembly.
- Build request and reply codecs.
- Current build encryption formats required for interoperability.
- Tunnel builder state machine.
- Inbound gateway, participant, outbound endpoint, and local endpoint roles.
- Exploratory inbound and outbound tunnel pools.
- Rotation, expiry, replacement, and failure accounting.
- Peer selection policy inputs without hard-coding policy in codecs.
- Bandwidth and queue accounting.

### Exit criteria

- Exploratory tunnel pairs are built through mixed-router peers.
- NetDB messages can travel through exploratory tunnels.
- Fragmented and out-of-order tunnel messages are handled correctly within strict bounds.
- Expired, failed, cancelled, and partially constructed tunnels clean up all state.
- Tunnel behavior is reproducible in deterministic simulation.

## Milestone 6: Destinations, garlic routing, LeaseSets, and minimal streaming

### Objective

Create the first usable end-to-end I2P client destination.

### Scope

- Destination lifecycle and key ownership.
- Destination-specific inbound and outbound tunnel pools.
- Garlic message construction and processing.
- Session tags and replay controls required by the selected LeaseSet/garlic formats.
- LeaseSet creation, publication, lookup, refresh, and expiry.
- Local destination message routing.
- Streaming handshake, sequencing, acknowledgement, retransmission, close, and reset behavior.
- Listener and outbound connect API internal to `i2pr-client`.
- Deterministic congestion and retransmission tests.

### Exit criteria

- Two `i2pr` destinations communicate over a private testnet.
- An `i2pr` destination communicates bidirectionally with an independent I2P implementation.
- A basic byte stream survives ordinary packet loss and reordering.
- Destination shutdown cancels streams, LeaseSet work, and dedicated tunnel pools cleanly.

### Interoperable-router checkpoint

Completion of Milestones 0 through 6 constitutes the first minimal interoperable router checkpoint. At this point, pause feature expansion for a corrective interoperability and architecture review.

The checkpoint must include:

- Mixed-router testing.
- Resource and lifecycle review.
- Fuzzing status review.
- Dependency and license review.
- Protocol support matrix update.
- Architecture drift analysis.

## Milestone 7: SAM baseline

### Objective

Expose destination and streaming functionality through an interoperable SAM interface.

### Scope

- SAM HELLO negotiation.
- Session creation and destruction.
- STREAM connect, accept, forward, close, and status behavior.
- Naming lookup.
- Clear declared support for SAM 3.1 and selected later features.
- Loopback-only default listener.
- Per-client and per-session resource limits.
- Strict line, token, option, and payload bounds.

### Exit criteria

- At least two independent SAM client implementations can use `i2pr` for streaming.
- Unsupported versions and options return correct explicit errors.
- Client disconnect and malformed command sequences release all session resources.

## Milestone 8: SSU2 transport and reachability

### Objective

Add the current UDP transport without coupling its protocol-specific behavior to NetDB, tunnels, or destinations.

### Scope

- SSU2 address and option handling.
- Token issuance and validation.
- Session establishment and termination.
- Packet numbers, acknowledgements, retransmission, fragmentation, and reassembly.
- Replay and source-address validation.
- IPv4 and IPv6 socket separation where appropriate.
- Peer test and relay behavior required for ordinary reachability.
- Shared transport selection policy across NTCP2 and SSU2.
- Published address snapshot updates.

### Exit criteria

- Incoming and outgoing SSU2 sessions interoperate with independent routers over IPv4.
- IPv6 behavior is tested where infrastructure permits.
- Spoofed-source and token-exhaustion scenarios do not trigger unbounded cryptographic work.
- Transport selection and fallback behave predictably under injected failure.

## Milestone 9: I2CP

### Objective

Provide the lower-level I2P client protocol over the shared destination and streaming services.

### Scope

- I2CP handshake and connection states.
- Session creation, reconfiguration, and destruction.
- Destination and LeaseSet interactions.
- Message send, receive, status, and bandwidth messages required by MVP clients.
- Tunnel option validation and policy projection.
- Session ownership and cleanup.
- Loopback-only default listener and non-loopback security policy.

### Exit criteria

- Selected independent I2CP clients can create a session and exchange data.
- Invalid tunnel and session options receive protocol-correct status responses.
- Client options are either honored, explicitly rejected, or documented as unsupported; they are not silently ignored.

## Milestone 10: Generic service tunnels, HTTP, SOCKS5, and IRC

### Objective

Provide the application-facing tunnel set required by the MVP without contaminating routing-core boundaries.

### Scope

- Generic TCP client tunnel: local listener to remote I2P destination.
- Generic TCP server tunnel: local I2P destination to loopback or Unix-socket target.
- HTTP proxy for `.i2p` destinations.
- SOCKS5 `CONNECT` support.
- IRC client tunnel profile and required filtering/rewriting.
- IRC server tunnel profile.
- Per-listener and per-connection limits.
- Destination selection and optional dedicated tunnel pools.
- Transactional service startup, reload, and shutdown.

### Non-goals

- SOCKS UDP associate.
- Clearnet HTTP outproxy.
- Transparent proxying.
- Arbitrary remote administrative exposure.

### Exit criteria

- Browser HTTP traffic reaches an eepsite through `i2pr`.
- SOCKS5-capable clients connect to I2P services.
- Generic client and server tunnels move bidirectional byte streams reliably.
- IRC client and server use cases interoperate with selected I2P IRC infrastructure in authorized ordinary testing.

## Milestone 11: Transit participation and bandwidth policy

### Objective

Allow the router to contribute network capacity safely and predictably.

### Scope

- Transit tunnel admission policy.
- Build-request validation and response.
- Per-tunnel and aggregate bandwidth accounting.
- Queue priority and shedding.
- Router capability publication.
- Participating-tunnel lifecycle, expiry, and cleanup.
- Abuse resistance and peer-specific limits.
- Configurable participation profiles.

### Exit criteria

- Router accepts and forwards transit tunnels within configured budgets.
- Resource use remains bounded under excessive build requests and traffic.
- Capability publication accurately reflects current policy and health.
- Graceful shutdown drains or safely terminates transit state according to documented policy.

## Milestone 12: Floodfill role

### Objective

Implement bounded, correct floodfill participation as an optional NetDB role.

### Scope

- Floodfill eligibility checks.
- Store, lookup, and search-reply service behavior.
- RouterInfo and LeaseSet quotas.
- Replication and peer selection.
- Expiry, replacement, and conflict policy.
- Publication verification support for other routers.
- Health-based role withdrawal.
- Disk persistence and restart recovery.
- Floodfill-specific abuse and load controls.

### Exit criteria

- `i2pr` serves correct NetDB responses in a private mixed-router testnet.
- Stored data remains bounded and signature-validated.
- Role advertisement is withdrawn when health or resource eligibility fails.
- Adversarial lookup, store, amplification, and churn scenarios remain within configured limits.

## Milestone 13: CLI operations, reload, observability, and persistence closure

### Objective

Turn the protocol implementation into an operable CLI router daemon.

### Scope

- Commands for run, check-config, status, peers, NetDB, tunnels, destinations, services, and identity inspection.
- Authenticated local control interface or equivalent daemon command channel.
- Transactional configuration reload.
- Configuration-field reload classification.
- Structured privacy-preserving logs.
- Bounded metrics.
- Health and readiness reporting.
- Atomic persistent state and migration behavior.
- Graceful and forced shutdown.
- Crash and restart recovery tests.

### Exit criteria

- Operators can diagnose transport, NetDB, tunnel, destination, and service health without exposing sensitive identifiers by default.
- Invalid reloads make no runtime changes.
- Supported live changes apply transactionally.
- Restart restores identity and validated persistent state without stale task or session assumptions.

## Milestone 14: MVP verification and security closure

### Objective

Verify that the assembled router satisfies the functional MVP and the project’s defensive requirements.

### Required verification tracks

#### Protocol interoperability

- Current I2P/I2P+ router.
- Current i2pd router.
- Emissary where behavior is relevant.
- Multiple SAM and I2CP clients.
- Mixed NTCP2 and SSU2 conditions.

#### Reliability

- Long-duration mixed-router operation.
- Repeated restart and reload cycles.
- Network partitions and recovery.
- Packet loss, delay, duplication, and reordering.
- Disk-full, corrupted-cache, and partial-write scenarios.

#### Adversarial resilience

- Parser fuzzing.
- Stateful protocol fuzzing.
- Handshake CPU and memory pressure.
- Queue saturation.
- NetDB poisoning and churn.
- Tunnel build floods and fragment exhaustion.
- SAM and I2CP malformed state sequences.
- Streaming sequence and retransmission abuse.
- HTTP, SOCKS5, IRC, and generic forwarding abuse.

#### Governance

- License and provenance audit.
- Dependency and unsafe-code audit.
- Secret-handling review.
- Logging and metrics privacy review.
- Architecture dependency review.
- Known limitations and threat model update.

### MVP exit criteria

The feature MVP may be declared only when:

- All required capabilities in the MVP definition are demonstrated.
- No known critical or high-severity issue remains open.
- Resource bounds are documented and tested.
- Sustained mixed-router testing succeeds.
- Fuzz targets have meaningful coverage and no unresolved crashers.
- Configuration, shutdown, and recovery paths are verified.
- Documentation accurately describes supported and unsupported behavior.
- The project is still explicitly labeled experimental unless a separate production-readiness review changes that status.

## Post-MVP directions

Potential post-MVP work includes:

- Synvoid deployment adapter and metadata boundary.
- Eggsec private-testnet orchestration and richer adversarial harnesses.
- Additional SAM 3.2/3.3 features and datagrams.
- Address-book and naming services.
- NAT-PMP, PCP, and UPnP adapters.
- Alternative async runtime evaluation.
- Additional transport proposals.
- Performance optimization after profiling.
- Packaging and operating-system service integration.
- TUI or web administration clients over an authenticated local control API.
- Research profiles for alternative peer, tunnel, and capacity policy.

## Planning discipline

Each milestone must be preceded by a detailed plan that includes:

- Current repository state.
- Exact scope and non-goals.
- Files and crates expected to change.
- Public and internal contracts.
- State machines and lifecycle ownership.
- Resource limits and failure behavior.
- Dependency changes.
- Test matrix.
- Documentation updates.
- Acceptance criteria.
- Handoff checklist.

Milestones may be split or reordered when implementation evidence justifies it, but dependency order and security prerequisites must not be bypassed for superficial feature progress.
