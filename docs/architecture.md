# Architecture

This document records the current modular-monolith boundaries and ownership.
The implemented common-structure codecs remain structural data handling, not
router behavior or an interoperability claim.

## Four planes

The intended modular monolith is organized into four conceptual planes:

| Plane | Responsibility | Current bounded status |
| --- | --- | --- |
| Data | Protocol representations, authenticated links, messages, and network tunnel traffic | Bounded common-structure and initial I2NP models plus transport-neutral link contracts, NTCP2 state, and controlled runtime TCP integration; no public-network behavior |
| Control | Configuration, lifecycle, health, cancellation, supervision, and resource budgets | Runtime-neutral core contracts plus the `i2pr-runtime` supervisor and bounded socket-owning services |
| Client | Destinations, LeaseSets, streaming, SAM, and I2CP adapters | Not implemented |
| Service | HTTP, SOCKS5, IRC, generic TCP, and local service-tunnel composition | Not implemented |

Network tunnels carry router-to-router I2P traffic and are distinct from
application service tunnels, which eventually connect a local application to a
destination. The latter must not import transport internals or peer-profile
storage.

## Current crate graph

The current bounded workspace has nine crates, including the test-only
simulation crate:

```text
i2pr-proto  <- i2pr-crypto <- i2pr-storage
     ^              ^               ^
     |              |               |
i2pr-core <- i2pr-transport <- i2pr-runtime <- i2pr-daemon (composition root)
     ^             ^       ^          ^
     |             |       |          |
     +-------------+-------+  i2pr-transport-ntcp2
                                   ^       ^
                                   |       |
                         i2pr-proto + i2pr-crypto

i2pr-testkit (test/simulation dependency only; may depend on transport crates)
```

The arrows show dependency direction. `i2pr-proto` owns protocol-facing names,
bounds, typed codec error categories, and the structural I2NP message registry.
It now also owns immutable Mapping,
certificate/key-type, RouterIdentity, Destination, RouterAddress, RouterInfo,
Lease, and classic LeaseSet values. Parsed signed records retain the exact
signed region. Its cursor borrows input, its encoder requires caller-visible
output limits, and strict top-level decoders reject trailing bytes. It has no
runtime, filesystem, CLI, transport, or tracing-subscriber dependency; its
direct external dependencies are the reviewed `sha2` crate for SHA-256 hash
derivation and the narrow `zeroize` wrapper dependency. `i2pr-core` owns
runtime-neutral service, health, lifecycle, cancellation, and resource-domain
types. `i2pr-runtime` owns Tokio, wakeable cancellation, service graph
validation, readiness, latest-state health publication, supervised task
managers, bounded restart policy, graceful/forced shutdown, TCP listeners and
streams, deadline timers, replay-cache state, and link child tasks.
`i2pr-transport`
owns runtime-neutral link, delivery, admission, lifecycle, observation, and
snapshot contracts. `i2pr-transport-ntcp2` depends on those contracts plus
protocol/crypto vocabulary but owns no Tokio, filesystem, socket, NetDB,
tunnel, client, or daemon behavior. `i2pr-runtime` is the sole production
owner of Tokio tasks, sockets, timers, channels, and wakeable cancellation.
`i2pr-testkit`
is a test/simulation-only dependency. It provides a manually wakeable
monotonic clock, domain-separated deterministic RNGs, a bounded manual-pump
scheduler, distinct stream and datagram endpoint pairs, executable fault
scripts, ephemeral identity/RouterInfo factories, topology summaries, and
payload-free replay records. It may depend on `i2pr-core`, `i2pr-runtime`,
`i2pr-proto`, `i2pr-crypto`, and the transport crates, but no production crate may depend on it. The
daemon owns CLI/configuration and is the composition root, but its live command
remains intentionally disabled.

The direction is mechanically checked by
`scripts/check-dependency-direction.sh`. Production crates do not depend on
`i2pr-testkit`, and `i2pr-proto` does not depend on filesystem or crypto
execution. The daemon is the only crate that composes configuration, explicit
identity lifecycle commands, crypto randomness, and storage.

### I2NP codec boundary

`i2pr-proto::i2np` implements the pinned 0.9.69 message identifiers, the
16-byte standard header, the obsolete five-byte SSU header, and the
NTCP2/SSU2 nine-byte short header. Standard payload lengths are checked before
body decoding and the one-byte SHA-256 checksum is verified. The codec caps
payloads at 62,708 bytes, DatabaseLookup exclusions at 512 hashes,
DatabaseSearchReply peers at 16, tunnel-build records at eight, and tunnel
data at its fixed 1,024 bytes.

DatabaseLookup, DatabaseSearchReply, DeliveryStatus, and the structural
DatabaseStore envelope are typed. Classic LeaseSet payloads use the existing
common codec; compressed RouterInfo, LeaseSet2-family records, garlic/data
payloads, and tunnel-build records retain only bounded `Deferred`/`Opaque`
bytes or validated fixed framing. Nested TunnelGateway messages require a
standard I2NP envelope. No I2NP decoder applies clock policy, routes a message,
authenticates a transport, decrypts garlic, performs tunnel cryptography,
updates NetDB, or advertises an I2NP version.

The protocol source tree now exposes internal ownership boundaries without
changing the crate-root API:

```text
i2pr-proto/src/
  common/
    mod.rs       date.rs       hash.rs       keys.rs
    mapping.rs   certificate.rs identity.rs  router_address.rs
    router_info.rs lease.rs
  i2np/
    mod.rs       header.rs     message.rs    netdb.rs
    delivery.rs  tunnel.rs     deferred.rs
```

Each leaf now owns its implementation bodies and only narrowly scoped private
codec helpers cross domain boundaries. `common_impl.rs` and `i2np_impl.rs` are
removed; the crate-root re-export façade remains stable, signed regions and
numeric bounds are unchanged, and decode helpers remain private. Future
protocol work should add behavior to the owning leaf namespace rather than
recreate a compatibility warehouse.

The fuzz workspace is an opt-in nightly test boundary. It depends on the
production protocol crate but never enters the production dependency graph;
its harnesses cap input and perform no filesystem, network, or global-state
work.

### Common-structure boundary

`i2pr-proto::common` validates wire shape, bounded sizes, canonical mapping
order, algorithm-specific public-material lengths, and exact signed-byte
boundaries. It does not verify signatures, generate secrets, decide timestamp
freshness, interpret transport options, publish RouterInfo, or construct
LeaseSet2-family records. Those responsibilities belong to later crypto,
storage, NetDB, and client plans.

### Cryptographic boundary

`i2pr-crypto` implements only the concrete Plan 013 profile: I2P type-7
Ed25519 signatures, type-4 X25519 router public-key derivation, SHA-256
wrappers, constant-time comparisons, and zeroizing private wrappers. Generation
accepts an injected `TryCryptoRng`; production uses the operating-system source
at the daemon boundary, while deterministic RNGs remain test-only inputs. The
crate exposes no generalized provider or plugin API and does not add crypto
operations to `i2pr-proto`.

It can construct a no-capability local RouterInfo, sign the exact retained
`RouterInfo::signed_bytes()` region, and verify that region through the public
identity. Timestamp freshness, transport interpretation, capability policy,
publication, and network interoperability remain outside this boundary.

Plan 032 extends the crypto boundary without changing that policy surface.
`i2pr-crypto` owns zeroizing X25519 private/shared-secret owners for transport
use; `i2pr-transport-ntcp2` owns only protocol composition over reviewed
X25519, SHA-256/HMAC, ChaCha20-Poly1305, AES block, SipHash, subtle, and
zeroize dependencies. Its consuming transcript binds the I2P Noise name and
responder static key, retains the SessionRequest cipher state needed by
SessionConfirmed part one, and produces role-mapped split owners. It has no
RNG, clock, filesystem, socket, Tokio, or generic Noise-provider surface.

### NTCP2 handshake state boundary

Plan 033 adds the first wire/state layer above the cryptographic transcript.
`i2pr-transport-ntcp2::handshake` owns strict, bounded codecs for
SessionRequest, SessionCreated, SessionConfirmed, their fixed options, and the
RouterInfo/options/padding block sequence in SessionConfirmed part two.
Fixed regions require exact lengths; variable cleartext padding in messages 1
and 2 is admitted only after the authenticated options length is checked.
`state_machine` owns consuming `InitiatorState` and `ResponderState` values.
Each transition accepts one typed input and returns one of the following
bounded actions:

```text
initiator:
  RequestRouterInfo -> RequestTimestamp -> RequestPadding(SessionRequest)
    -> RequestPadding(SessionConfirmed) -> Write(SessionRequest)
    -> ReadBounded(SessionCreated) -> RequestTimestamp -> RequestReplay
    -> Write(SessionConfirmed) -> Authenticated
responder:
  ReadBounded(SessionRequest) -> RequestReplay -> RequestTimestamp
    -> RequestPadding(SessionCreated) -> Write(SessionCreated)
    -> ReadExact(SessionConfirmed) -> Authenticated
```

The runtime adapter later fulfills exact/bounded reads and writes, timestamp
and padding requests, replay admission, cancellation, and deadlines; it does
not cross raw sockets, Tokio channels, payload bytes, or peer addresses into
this crate. RouterInfo signature verification and NTCP/NTCP2 version-2
static-key binding happen before an authenticated result is emitted. The
authenticated result owns the role-mapped `SplitKeys`, while its default
diagnostics expose only role and bounded public correlators.

The local compatibility policy is ±60 seconds with replay retention of at
least twice the window, fail-closed replay admission, and 880/848-byte clear
padding maxima for messages 1/2. The production padding distribution and
negotiation remain open; no state-machine result changes support metadata or
publishes an address.

### NTCP2 data-phase boundary

Plan 034 keeps the authenticated data phase synchronous and runtime-neutral:

```text
TransmitReady -> FramePrepared -> TransmitReady | Terminated
ReceiveReady  -> LengthDecoded -> CiphertextCollected
             -> ReceiveReady | Terminated
```

The wire layout is `obfuscated_length:u16_be || ciphertext`, where the clear
ciphertext length is 16..=65,535 bytes and includes the 16-byte
ChaCha20-Poly1305 tag. Plaintext is therefore at most 65,519 bytes. The
receiver advances the SipHash length state while deobfuscating and validates
the clear value before admitting ciphertext storage. AEAD uses empty
associated data; only a successfully opened plaintext reaches
`parse_blocks`.

Each authenticated block owns a 1-byte type, 2-byte big-endian payload length,
and bounded payload. Types 0 (timestamp), 1 (options), 2 (RouterInfo), 3
(I2NP), 4 (termination), and 254 (padding) have typed codecs. Unknown types
are skipped only after authentication and count against the per-frame block
and unknown-byte budgets. SessionConfirmed part two remains structurally
strict. General data-phase parsing is a separate Plan 037 boundary: it must
accept every specification-permitted sequence, including permitted repeated
blocks, while keeping padding final and termination terminal. RouterInfo is
verified and returned as an update candidate; it never mutates NetDB in this
layer.

Outbound I2NP blocks consume `EncodedI2npMessage` and append its bytes without
an implicit clone. Inbound frames retain one bounded authenticated plaintext
owner while parsed I2NP views are borrowed; an explicit receiver handoff may
create the transport owner. Plan 035 established the socket ownership seam;
Plan 037 requires the runtime adapter to attach exact bounded owners to
queued, ciphertext, plaintext, and partial-frame work. Plan 034 itself does
not own those queues. Partial reads, writes, deadlines, cancellation, and
coalescing waits remain runtime concerns.

The current NTCP2 specification defines no periodic data-phase rekey
threshold. This implementation therefore treats the last permitted nonce and
counter exhaustion as terminal and requires a new Noise handshake for rekey or
static-key/IV rotation. No speculative wire rekey is emitted.

### Plan 035 runtime link boundary

Plan 035 adds the first controlled socket layer without moving runtime
ownership into the protocol crates. The production service graph is:

```text
transport-manager (runtime service)
  ├── ntcp2-listener (owned accept loop)
  ├── ntcp2-dialer (bounded connect attempts/backoff)
  ├── replay-cache (bounded expiry owner)
  └── link scopes
        ├── reader child (bounded stream receive)
        └── writer child (bounded stream delivery)
```

`i2pr-runtime` owns every `TcpListener`, `TcpStream`, split half, Tokio
channel, deadline, admission lease, and child join. A listener is a valid
disabled state until an explicit controlled-test configuration enables it. An
unexpected required-service failure is reported through the supervisor rather
than being treated as readiness. The listener admits global, per-IP, and
explicit IPv4/IPv6 subnet limits before expensive cryptography; internal
accounting keys never enter snapshots or default tracing.

The current Plan 035 runtime subset exposes the ownership and I/O seams needed
to translate pure handshake actions into exact or bounded reads, complete
writes, injected timestamps/padding, replay decisions, and typed
cancellation/deadline inputs. It does not yet claim a complete wire-level
handshake or authenticated data-phase driver; that composition is a Plan 036
prerequisite. The bounded link owner still gives the runtime an explicit place
to transfer directional data-phase owners exactly once. Address parsing and
reachability observations are candidates only: they never mutate RouterInfo or
NetDB and never infer an external address from one peer.

Runtime TCP tests are restricted to loopback or an authorized isolated testnet,
use paused Tokio time where possible, and provide no capability-advertisement
or mixed-router evidence. Default diagnostics contain only fixed categories,
synthetic local link IDs, bounded counters, coarse families, and typed outcomes;
raw endpoints, peer hashes, keys, transcripts, frames, payloads, and OS error
text remain outside the default observation boundary.

### Plan 036 validation boundary

Plan 036 adds no production transport API and does not turn the daemon's
disabled `run` command into a live router. The reproducible manual lane lives
under `tests/integration/ntcp2/` and has a separate ownership boundary:

```text
authorized private namespace
  ├── disposable Java I2P / i2pd process or pinned image
  ├── disposable identity and NTCP2 static-key owners
  ├── bounded scenario driver and fixed clock
  └── sanitized typed outcomes + artifact/configuration hashes
```

The lane must reject public endpoints, reseed/bootstrap, operational
identities, and unbounded captures before starting a scenario. Its committed
preflight checks only the pinned manifest and evidence directory; it never
starts a router. The current implementation has not yet composed the pure
Plan 033/034 state machines with the Plan 035 socket owner, so Java I2P and
i2pd results are an explicit blocker in `plans/036-closure.md`, not local
interoperability evidence. The testkit's 0..255 fixed-seed matrix and pure
NTCP2 fuzz campaigns remain useful bounded evidence but cannot advance the
support ledger.

### Plan 037 corrective integration boundary

Plan 037 corrects the local ownership defects exposed by the Plan 036 review.
An accepted inbound stream is now transferred as an `AdmittedInboundStream`
that retains its non-cloneable pending-handshake permit. Link queue entries
reserve item/byte capacity once and release it by RAII on successful write,
failure, cancellation, receiver closure, or child-scope teardown. Reader and
writer children use cancellation-aware read-idle/write deadlines; dial retry
backoff is consulted before connect and cleared only after explicit handshake
authentication.

The general data-phase parser accepts specification-permitted repeated
non-padding blocks and allows Termination after earlier valid blocks, with
only final Padding afterward. SessionConfirmed part-two parsing remains a
separate strict parser. These are local corrective guarantees; the runtime
owns the NTCP2 wire adapter and has been locally validated, but mixed-router
evidence, RouterInfo publication, NetDB mutation, and daemon activation
remain pending.

Generated and reconstructed private seeds are held by zeroizing owners during
crypto operations. Storage encoding and file-read buffers are also zeroizing;
the `DatabaseLookup` reply-key/tag wrappers in `i2pr-proto::i2np::netdb` are
non-cloneable and redact their contents. These measures reduce ordinary
post-use retention but do not provide encrypted reply semantics or defeat
process compromise, allocator copies, swap, core dumps, or every compiler or
platform memory-retention behavior.

### Plan 038 Ubuntu reference-router harness boundary

Plan 038 defines an opt-in evidence harness outside the production daemon. The
first supported host is Ubuntu amd64. Its workflow has two security domains:

```text
preparation (network-enabled, host scope)
  -> verify Ubuntu/toolchain contract
  -> install declared packages
  -> fetch locked Java I2P/i2pd revisions
  -> build, hash, and cache disposable artifacts

execution (network-isolated, per-scenario scope)
  -> create disposable run state and two namespaces
  -> connect namespaces only with a veth pair
  -> reject default routes, DNS, and public egress
  -> generate identities/configuration and exchange RouterInfo locally
  -> run one bounded participant pair and collect typed outcomes
  -> sanitize, destroy secret-bearing state, drain processes, delete namespaces
```

Preparation is the only phase allowed to use package/source network access.
Execution must not download, resolve names, reseed, bootstrap, publish
RouterInfo, mutate NetDB, or contact a public endpoint. `i2pr-runtime` remains
the sole owner of Tokio and sockets; a dedicated non-production
`i2pr-interop` launcher composes it with the runtime-neutral transport and
NTCP2 protocol crates without activating `i2pr-daemon`.

Plan 038/040 i2pr/reference scenarios get one i2pr namespace and one
reference-router namespace. Plan 041's reference-only control uses the separate
`reference_topology.py` owner and names its namespaces `java-*` and `i2pd-*`.
Both veth endpoints leave the host namespace, and each scenario namespace
contains only loopback, its veth interface, and the expected directly
connected routes.
Route isolation is primary; namespace-scoped nftables rules are defense in
depth. The host checker and isolation verifier must fail closed before a
router starts, and cleanup failure is a scenario failure.

The evidence classes are deliberately separate. Environment smoke validates
reference startup and cleanup; Plan 041 reference crosscheck validates Java I2P
against i2pd with explicit network ID 99, staged RouterInfo exchange, and dual
authenticated observations without making an i2pr claim; only bounded authenticated i2pr-to-reference
runs in both directions are mixed-router evidence. Retained records contain
typed outcomes, bounded run metadata, and hashes of sanitized artifacts only.
Raw addresses, peer identities, RouterInfo, I2NP, keys, transcripts, raw logs,
and arbitrary remote error text are deleted.

### Identity storage boundary

`i2pr-storage` stores only the private router identity format described by ADR
0006. It is not a NetDB or public RouterInfo store. It rejects symlinks,
overly-permissive Unix paths, malformed/trailing/oversized data, unsupported
versions and integrity failures; it never regenerates an existing identity.
The explicit create-only operation uses a same-directory temporary file,
flush/sync, an atomic no-replace install, cleanup, and directory sync where the
platform supports it.

The separate `TransportStaticKeyStore` persists an independently generated
NTCP2 X25519 static key, rederived public key, and obfuscation IV in
`ntcp2.static.key`. Its versioned 132-byte record is not derived from or
interchangeable with `router.identity`; it uses the same strict path,
permission, checksum, zeroizing-buffer, and no-replace rules. Address
publication, RouterInfo mutation, and rotation policy remain outside storage.

New identity directories use creation-time mode `0700` on Unix. The standard
library path creates only the final component with restrictive mode and
requires its parent to already exist; recursive missing intermediates are not
silently created. Existing directories are revalidated for symlink, type, and
permission safety. Parent-directory ownership remains an operator threat-model
responsibility, and non-Unix permission/durability semantics remain limited.

### Cancellation scope

`i2pr-core::CancellationToken` remains runtime-neutral bootstrap machinery: an
atomic cancellation flag for synchronous cooperative polling. It records a
cancellation request but does not provide async wake semantics or async wait
and selection operations.

`i2pr-runtime::CancellationToken` is the concrete runtime-facing boundary. It
wraps Tokio's hierarchical cancellation primitive, records one bounded
`CancellationReason`, wakes all current waiters, supports cancellation before
registration, and exposes an async wait that can participate in `select!` with
commands and deadlines. Child tokens inherit parent cancellation without
propagating child cancellation upward. Dropping a handle does not cancel an
unrelated scope.

### Supervised service graph

`i2pr-runtime::ServiceGraph` validates the complete registration set before
spawning work. Service names, counts, descriptions, deadlines, dependencies,
and restart attempts are bounded. Duplicate names, missing/self dependencies,
cycles, invalid timeouts, missing essential services, and restart policies on
non-restartable classifications are rejected. Kahn's algorithm over ordered
sets produces a deterministic dependency-first startup order; independent
services are intentionally started sequentially in this milestone so tests do
not depend on scheduler poll order.

Each manager owns one service future and its child scope. A service must signal
one-shot readiness explicitly. Health is a latest-state `watch` snapshot with
the service identifier, classification, lifecycle, health, restart count,
static failure category, bounded detail, sequence, and runtime monotonic
transition time. Panic payloads and raw service errors never enter completion,
health, or diagnostic data.

Restartable services alone may use an explicit bounded exponential-backoff
policy. Essential failures cancel the graph; degradable and optional failures
remain visible without accidental process termination; restart exhaustion has
an explicit degrade-or-shutdown choice. Service child tasks inherit
cancellation and are joined by their scope before the parent manager reports
completion. The supervisor also retains one bounded owner slot for each
active child scope. If a manager misses the shutdown deadline, the supervisor
aborts the manager, aborts and drains that exact child collection, and only
then publishes final task counts. Scope drop may request abort as a
synchronous last resort but cannot decrement counters; an unconfirmed drain
is reported as typed cleanup failure.

Shutdown first cancels every manager and joins within the configured bounded
deadline. Remaining managers are then aborted and joined. The report records
graceful versus forced cleanup, final typed completions, joined-task count, and
zero remaining owned tasks. No runtime service opens sockets or adds protocol,
NetDB, tunnel, client, API, or plugin behavior.

### Privacy-aware observability and aggregate snapshots

Plan 024 adds a diagnostic boundary without adding a diagnostic data store.
`i2pr-runtime::event` defines fixed event names for service lifecycle,
shutdown, channel/resource outcomes, and simulation completion. Runtime and
testkit code may emit structured events through `tracing`, but only
`i2pr-daemon` installs a subscriber. Fields are limited to validated static
identifiers, typed categories, bounded counters with units, monotonic timing,
and synthetic simulation link/sequence/rule metadata.

`SupervisorSnapshot` projects each service into lifecycle, readiness, health,
classification, restart count, failure category, transition sequence, and
monotonic transition time. It deliberately excludes `HealthDetail` text.
`RuntimeSnapshot::try_new` combines that projection with sorted, capped
`ChannelSnapshot` and `ResourceUsage` values plus optional aggregate testkit
counters. Snapshot assembly is synchronous and does not hold mutable state
across an await; independent owners make the result eventually coherent rather
than transactional. Default `Debug` for health detail is redacted.

The supervisor keeps the latest health value even when no receiver is attached,
so direct snapshots and watch subscribers observe the same state. Service
manager and child-task counters are decremented on every join and forced
cleanup path. A final stopped snapshot must report zero owned service and child
tasks; callers add channel, resource, timer, and synthetic-link invariants from
the same bounded observation boundary.

### Integrated deterministic validation

`crates/i2pr-testkit/tests/milestone_2.rs` exercises five named scenarios:
clean startup/shutdown, bounded overload, restart recovery, essential failure
with a forced non-cooperative optional service, and stream/datagram fault
replay. The scenarios use fixed service graphs, paused Tokio time, the manual
clock, explicit budgets, and bounded yield/step counts. The replay matrix runs
32 fixed root seeds and compares complete privacy-safe `ReplayRecord` values;
it never includes payloads, identities, addresses, or full protocol records.

This validation proves ownership and determinism of the local foundation only.
It does not create a transport adapter, socket, protocol exchange, peer label,
NetDB action, tunnel, client/API listener, or capability advertisement.

## Composition and communication

The daemon will eventually compose supervised services and pass each service
only the narrow handles or capabilities it needs. A global mutable router
context or unrestricted service locator is not an architectural default.

### Transport manager boundary

The transport manager is a runtime-neutral decision boundary. It accepts
bounded authenticated-link candidates, resolves them against the current
local link set, admits delivery requests, records typed closure/backoff and
address observations, and publishes bounded privacy-safe snapshots. It does
not wait on time, open a socket, read a Tokio channel, mutate RouterInfo or
NetDB, select tunnels, score peers, or route application traffic.

State-machine drivers communicate through explicit bounded actions and typed
results rather than async traits. `i2pr-transport-ntcp2` owns the NTCP2
protocol constants, Plan 032 cryptographic/transcript foundation, and Plan 033
handshake wire/state layer and Plan 034 data-phase frames. Plan 035 supplies
the owned runtime socket, deadline, admission, and child-task boundary; a
future plan must translate those actions into complete protocol I/O operations.
The boundary deliberately models only immediate Milestone 3 needs, leaving
SSU2-specific behavior and duplicate winner policy to later plans.

The encoded-I2NP handoff is an owned bounded byte container. It keeps the
canonical authenticated representation stable and prevents repeated
decode/re-encode, while its redacted `Debug` and consuming delivery handoff
keep payloads out of diagnostics and avoid implicit large clones. Resource
leases for `PendingHandshakes`, `ActiveLinks`, and `BufferedBytes` remain
attached to the exact owner until handoff or teardown.

The current identity CLI is deliberately not a runtime service: `identity
generate` is the only operation allowed to create the private identity file,
`identity inspect` only loads and summarizes it, and `run --dry-run` remains
side-effect-free. No identity command opens a listener or publishes a record.

The implemented service model classifies work as essential, restartable,
degradable, or optional. Each long-lived service declares startup dependencies,
readiness, health signals, owned resources, cancellation, and graceful/forced
shutdown behavior through `i2pr-runtime`. Plan 022 now provides the bounded
communication and resource-governor foundation, Plan 023 provides the
deterministic simulation boundary, and Plan 024 provides privacy-aware
snapshots/events and integrated validation while preserving this ownership
boundary.

### Bounded communication and resource ownership

Plan 022 makes the communication boundary concrete without adding a service or
transport. Runtime channels use four explicit patterns:

| Pattern | Contract | Initial overflow policy |
| --- | --- | --- |
| Command | ordered point-to-point instruction, no silent drop | wait only through an explicit deadline and cancellation scope |
| Request | one bounded response path, with response closure surfaced | wait only through the same bounded scope |
| Event | bounded single-consumer observation | drop-newest with an exact counter |
| Latest state | current value plus version and initial absence | coalesce to the newest value through `watch` |

Channel names and owners are bounded metadata. Infrastructure channel capacity
is nonzero and capped at 4,096 slots; caller byte estimates are capped at
1 MiB. A sender reserves a queue slot before it acquires an optional resource
lease, so a cancelled or expired capacity waiter cannot pin a lease. Accepted
items carry their lease through queueing and receiver processing. Receiver drop
deterministically drops queued entries and releases their charges. Shutdown is
out-of-band through the existing cancellation scope and cannot wait behind an
ordinary command queue.

`i2pr-core::ResourceBudget` uses immutable per-runtime limits and immediate
grant-or-deny admission. The initial classes include service tasks, child tasks,
command and event queue items, buffered bytes, simulated stream and datagram
links, pending timers, and test peers, while preserving the existing future
resource vocabulary. `ResourceLease` is non-cloneable and releases exactly one
grant on drop or consuming early release. `ResourceBundle` validates all
requests, rejects duplicates, orders classes deterministically, and commits
all classes atomically; denial leaves usage unchanged. Per-class snapshots
expose bounded limit, current usage, high-water mark, saturating denial
counts, and a saturating release-underflow invariant count. Channel snapshots
expose static metadata, capacity, queue depth, typed
outcome counters, and drop/resource-denial counters without payloads, secrets,
peer identities, or dynamic labels.

### Deterministic simulation boundary

Plan 023 keeps simulation below the future transport boundary. `ManualClock`
stores bounded sleepers in deadline/registration order; advancing time wakes
all due waiters, while dropping the final clock handle closes and wakes pending
sleepers. `TokioClock` is an adapter for callers that need production-style
Tokio timing, but the deterministic test suite uses only `ManualClock`.

`ReproducibilitySeed::derive` hashes a root seed and a bounded domain label, so
topology, link directions, fault decisions, identities, and messages do not
share mutable RNG state. Fault scripts match only bounded metadata (link,
direction, unit kind, sequence/range/every-N, and deterministic probability).
Rules compose in declaration order: delays add, duplicates are hard-capped,
reorder reverses bounded sequence groups, truncation is applied to the unit,
and disconnect/reset are explicit terminal outcomes.

The scheduler orders deliveries by `(deadline, link, direction,
order-sequence, sequence, duplicate-index)`. It reserves receiver capacity and
Plan 022 leases before a payload enters the pending map. Pending deliveries and
buffered bytes are bounded; due work remains queued when a receiver is full.
Stream reads preserve ordered bytes and half-close/EOF semantics. Datagram
receives preserve complete packet boundaries and return a synthetic source
address. The first API is testkit-specific rather than `AsyncRead`/
`AsyncWrite`, so transport plans retain control of adapter semantics.

The harness is a manual pump with the runtime cancellation token and Plan 022
budget contracts. It does not spawn a scheduler task; services used in a
simulation remain owned by the caller's Plan 021 supervisor or child scope.
`run_until_idle` has an explicit step bound. Shutdown purges pending units and
resets endpoint waiters. Link resource leases remain owned by live endpoint
handles and are released when those handles are dropped, which makes ownership
visible in final snapshots rather than silently detaching it.

## External boundaries

Future `synvoid` integration belongs behind a local Unix-socket or loopback
service boundary. It is not a routing-core dependency. Future `eggsec`
integration belongs in `i2pr-testkit`, private-testnet orchestration, and
stable fixtures; production routing code must not expose unrestricted testing
hooks.

The project is a modular monolith, not a runtime plugin platform. Compile-time
components or authenticated out-of-process interfaces are preferred to
in-process Rust plugins.
