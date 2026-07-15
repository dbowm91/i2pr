# Initial security model

Milestone 0 establishes boundaries and validation behavior. It does not claim
anonymity, privacy, censorship resistance, production readiness, or protocol
security.

## Assets

Current and future assets include router identity keys, peer and destination metadata,
RouterInfo/LeaseSet data, tunnel state, client traffic, configuration, local
service endpoints, resource capacity, and diagnostic output. The bootstrap has
an explicit persistent identity path but does not create one during validation,
dry-run, or live runtime startup.

## Adversaries and trust boundaries

The design treats the following as untrusted:

- Remote unauthenticated peers and authenticated-but-malicious peers.
- Malicious or malformed SAM/I2CP and application clients once those adapters exist.
- Malformed command-line arguments and local TOML configuration.
- Corrupted or stale persisted network state.
- Malicious or misleading reseed material.
- Resource-exhaustion attempts and oversized inputs.
- Dependencies, build tools, CI actions, and other supply-chain inputs.

Trust boundaries are the protocol decoder, configuration parser, persisted-state
loader, client adapters, local service boundary, and daemon composition root.
Each future boundary must validate input before handing a narrower capability to
the next subsystem.

## Objectives and required properties

Future implementations must use explicit size/count/time/nesting limits,
complete-consumption parsing where canonical encodings require it, bounded
queues, deadlines, cancellation, and cleanup. Resource exhaustion must cause
rejection, deferral, backoff, or shedding rather than unbounded memory growth.
Persisted network data must be revalidated on load, and security-sensitive
writes must be atomic or recoverable.

The initial I2NP codec applies a 62,708-byte payload ceiling before body
allocation, checks standard-header lengths and checksums, rejects unknown
message identifiers, caps DatabaseLookup exclusions at 512 and search replies
at 16 peers, and validates fixed tunnel/build framing. Compressed, encrypted,
and cryptographic body semantics remain explicitly deferred. Deferred payload
wrappers redact bytes from `Debug`; callers must opt into accessing their
contents. No parser performs clock, routing, NetDB, tunnel, garlic, or
transport side effects.

Secret-bearing types must avoid accidental `Debug`, `Display`, unrestricted
serialization, and unnecessary cloning. Production cryptography must use
reviewed implementations; deterministic randomness belongs only to tests and
explicit reproducibility tooling. Plan 013's private Ed25519 and X25519
wrappers zeroize on drop and expose bytes only to explicit storage methods.

Plan 015 extends that boundary to transient ownership: generated and
reconstructed seed buffers, serialized identity write buffers, file-read
buffers, and decoded DatabaseLookup reply keys/tags use zeroizing owners where
they are retained. Reply-secret wrappers are non-cloneable and redact their
debug output. This is memory hygiene for ordinary success and failure paths,
not encrypted-reply implementation or a guarantee against a process that is
already compromised.

The initial identity storage threat model is permission hardening rather than
encryption at rest. On Unix, the data directory must have no group/world mode
bits and generated identity files are mode 0600. Storage uses a versioned
fixed-format record, SHA-256 integrity, strict revalidation, and atomic
create-only installation. A checksum cannot protect against an attacker with
write access to both the file and its directory, so operators must protect
ownership and backups; passphrase encryption is deferred to a separate ADR.
Corrupt or unsupported identity state fails closed and is never silently
replaced.

New Unix identity directories are created with mode `0700` at creation time;
the standard-library implementation requires an existing parent and does not
recursively create missing intermediate components. Existing directories and
symlink paths are rejected when unsafe. The parent directory remains an
operator responsibility: an attacker who can write the parent can replace the
identity path even if the child mode is private.

Zeroization does not defeat allocator copies, compiler/platform retention,
swap, hibernation images, core dumps, crash reporters, process snapshots, or
an attacker with process memory access. Non-Unix permission and directory
durability semantics are limited by the platform and are not a production
security claim.

`identity inspect` reports only the storage path and public algorithm IDs. It
does not print private seeds, a private serialization, or a full router hash.

## NTCP2 cryptographic foundation threats and controls

Plan 032 adds local cryptographic composition but no network activation. The
NTCP2 static X25519 key and its published AES obfuscation IV are independent
from the RouterIdentity and are persisted in a separate versioned, checksummed,
create-only record. Immediate restart reuses both values; silent replacement
or rotation would invalidate cached RouterAddresses and is rejected. The
record is still plaintext private material protected by restrictive directory
and file permissions, not encryption at rest.

The transcript binds the exact I2P Noise name, empty prologue, responder
static key, role, and message ordering. SessionRequest/Created/Confirmed
stages are consuming owners; SessionConfirmed part one uses the retained
SessionRequest cipher state at nonce 1, and split invalidates the handshake
owner. This prevents accidental state reuse and transcript confusion but does
not protect a process whose memory, swap, core dump, or crash artifacts are
compromised.

X25519 all-zero shared secrets are rejected before KDF use. ChaCha20-
Poly1305 counters are checked before use and never emit `2^64 - 1`; a nonce
reuse or counter wrap would compromise authenticated encryption and therefore
terminates the bounded state operation. AES-CBC ephemeral obfuscation is DPI
obfuscation using public RouterHash/IV inputs, not authentication or secrecy.
SipHash material is directional and used only for the two-byte length mask;
it is not a substitute for AEAD authentication.

Private keys, chaining keys, cipher keys, split keys, and shared secrets use
non-cloneable zeroizing owners with no `Debug`, `Display`, serde, or payload
formatting. Public keys and transcript hashes use typed wrappers with redacted
diagnostics. Fixed vectors contain synthetic test values only; the validator
and support ledger prevent local vectors from becoming interoperability or
capability evidence.

## Runtime supervision threats and controls

Plan 021 adds a concrete non-networked runtime boundary without changing the
protocol support claim. Tokio is confined to `i2pr-runtime`; protocol, crypto,
storage, and runtime-neutral core crates remain executor-free.

Long-lived task leaks are treated as resource-exhaustion vulnerabilities. A
supervisor owns every service manager through an owned `JoinSet`, and each
service owns a bounded child scope. Shutdown cancels all scopes, joins within a
bounded deadline, aborts remaining managers, and joins aborted handles before
returning. Child scopes abort their remaining children on drop as a final
guard, while normal service completion explicitly joins them.

Plan 025 closes the false-zero cleanup risk by retaining each active manager's
bounded child-scope owner in the supervisor. A forced manager abort is followed
by abort-and-drain of that exact child collection; child counters decrement
only after join results, and a bounded failed drain is reported as typed
`FailedCleanup` evidence. A synchronous scope drop can still request abort as a
last resort, but it cannot claim termination. Service-selected
`RequestedShutdown` without observed cancellation is classified as an
unexpected clean exit, preventing an essential service from disappearing
while the graph remains ready.

Cancellation races are handled by Tokio's hierarchical cancellation primitive:
registration and cancellation are wake-safe, cancellation is idempotent, a
bounded static reason is recorded once, parent cancellation reaches children,
and child cancellation cannot reach a parent. The runtime-neutral atomic token
is retained only for synchronous contracts and is not used as async service
cancellation.

Restart policy is explicit and bounded. Only services classified
`Restartable` may restart; attempts, exponential delay, service timeouts, and
the router-wide service count are capped. Zero-delay hot loops are rejected.
Restart exhaustion must choose degradation or coordinated shutdown. Dependency
failure marks dependent snapshots degraded and cancels their owned managers so
they cannot remain ready after a hard dependency is gone.

Panic and join failures become static completion categories. Panic payloads,
raw errors, secrets, peer data, addresses, and arbitrary user text are not
formatted into health snapshots or normal diagnostics. Health uses a bounded
latest-state watch snapshot rather than an unbounded event log.

Forced abort is cleanup evidence, not a claim that arbitrary code can be made
graceful. A non-cooperative service is stopped at the configured deadline and
reported as forced. No Plan 021 service binds sockets, connects to peers,
performs DNS, touches NetDB, constructs tunnels, exposes client listeners, or
advertises protocol capabilities.

## Transport contract threats and controls

Plan 031 adds only the ownership vocabulary needed before NTCP2 wire work. The
transport manager owns bounded link candidates, authenticated-link admission,
delivery queue ownership, lifecycle observations, and typed outcomes. The
runtime owns every future socket, Tokio channel, timer, reader/writer task, and
cancellation scope. `i2pr-transport-ntcp2` remains a pure protocol crate and
cannot open sockets or perform filesystem, NetDB, tunnel, or client work.

## NTCP2 handshake threats and controls

Plan 033 adds bounded handshake parsing and consuming state transitions without
activating a network adapter. SessionRequest and SessionCreated accept only a
fixed 32-byte obfuscated ephemeral field, a fixed authenticated 16-byte
options payload, and bounded cleartext padding. SessionConfirmed requires the
fixed 48-byte static-key frame plus the negotiated second frame; its plaintext
may contain only RouterInfo, Options, then Padding. Lengths are checked before
allocation; fixed regions are exact and trailing bytes in the part-two block
sequence are rejected.

The responder and initiator use injected timestamp values with a documented
±60-second policy and no wall-clock access in the protocol crate. Replay
tokens are bounded SHA-256 values derived from encrypted ephemeral material;
the reference cache expires entries deterministically and fails closed on
replay, capacity, or unavailable decisions. Neither timestamps nor replay
tokens enter default logs.

RouterInfo is structurally decoded, its retained signed region is verified,
and the NTCP/NTCP2 version-2 `s` option must match the X25519 static key
authenticated by SessionConfirmed. RouterIdentity hash and static-key
mismatches are distinct typed failures. Structural parsing never becomes an
authentication claim, and no accepted RouterInfo mutates NetDB.

Consuming states prevent retransmission or resumption after a failed
transition. Actions retain only bounded owned handshake bytes or redacted
typed decisions; secret transcript/cipher owners are consumed into the final
split-key result. Random production padding, partial-stream adaptation,
timeouts, cancellation, and probing-resistance behavior remain the
runtime-owned follow-up boundary.

## NTCP2 data-phase threats and controls

Plan 034 deobfuscates the length prefix with a direction-specific SipHash
state and rejects the clear length before allocating ciphertext. The wire
prefix itself may be any two bytes, so validation is deliberately performed
after XOR rather than on the attacker-controlled obfuscated value. Ciphertext
is authenticated with empty associated data before any block header or unknown
type is inspected; tag failure, malformed blocks, invalid ordering, and
counter exhaustion put the receive/transmit owner into a terminal state.

The authenticated plaintext has explicit limits for total bytes, block count,
unknown bytes, options, RouterInfo, I2NP messages, padding, and termination
metadata. General data-phase non-padding blocks may repeat where permitted;
Termination is accepted after earlier valid blocks but must be the final
non-padding block, and Padding remains single and final. Invalid blocks after
Termination, trailing headers, and oversized fields fail closed. RouterInfo
signature and authenticated-link static-key checks produce candidates only;
they do not update NetDB. Unknown blocks are treated as bounded padding only
after authentication and cannot bypass the aggregate budget.

Transmit and receive owners contain independent cipher and length counters.
Counters advance once for an accepted frame; failed authentication cannot be
reused because the owner is terminal. The protocol dossier defines no
periodic in-session rekey threshold, so this layer never invents one: a fresh
Noise handshake is required after exhaustion or static-key/IV rotation. The
forbidden nonce value `2^64 - 1` is never emitted.

Partial length/ciphertext reads are retained by the future runtime adapter in
bounded owners, not inferred as frame alignment. Deterministic testkit cases
cover split prefixes, one-byte writes, truncation, duplicate frames,
backpressure, cancellation, and teardown. Debug and error values expose only
lengths, counts, typed categories, and terminal state; they never expose
payloads, keys, tags, identities, addresses, or remote text.

Transport payloads cross the manager boundary as bounded owned encoded-I2NP
messages. The owner validates nonzero and maximum lengths at construction,
preserves authenticated bytes, exposes no implicit large-payload clone, and
uses explicit consuming handoff. Delivery requests carry only a redacted peer
reference, payload owner, bounded monotonic expiry, and a runtime-owned
response capability. Typed outcomes distinguish no-link, queue/resource
denial, deadline, cancellation, replacement, closure, protocol termination,
and identity mismatch without retaining remote error text.

Pending handshakes, active links, buffered bytes, and queue items use existing
`ResourceClass` leases. Admission is immediate grant-or-deny; every accepted
lease stays with its exact candidate, link, or queued payload until handoff,
completion, rejection, cancellation, or drop. Capacity-one, exact-limit, and
limit-plus-one cases are deterministic tests, and valid teardown must return
usage to zero without an underflow signal.

Link IDs are local bounded correlators and are not derived from peer identity.
Peer references wrap the redacted public identity digest but do not expose full
bytes or mutable profile state. Default snapshots contain only bounded local
IDs, transport/direction/lifecycle categories, queue counters, rounded age,
typed termination, and resource usage. They exclude addresses, ports, hashes,
keys, transcripts, payloads, and dynamic peer labels. Duplicate-link inputs
and decisions are representable, but the winner policy remains deferred to
Plan 035 rather than being guessed here.

## Plan 035 runtime TCP threats and controls

Plan 035 is the first phase allowed to open TCP sockets, but only through
`i2pr-runtime` and only for controlled local/private scenarios. The listener is
disabled unless an explicit test configuration enables it. Every accepted
socket is immediately covered by global, per-IP, and IPv4 `/24` or IPv6 `/64`
pending-handshake admission before cryptography. Admission keys are bounded
internal counters; raw addresses never enter default events or snapshots.

Slowloris reads, stalled writes, connect storms, and duplicate candidates are
bounded by nonzero capped connect/handshake/read-idle/write/queue/drain
deadlines, bounded queues and bytes, replay capacity, active-link limits, and
expiring dial backoff records. Replay-cache capacity fails closed. The runtime
uses typed outcomes for overload, cancellation, deadline, identity mismatch,
replacement, closure, and protocol termination rather than retaining OS error
text or peer-controlled messages.

The listener/dialer owns sockets through the supervisor; each runtime-managed
link registers exactly one reader and one writer child. A child failure cancels
its sibling, and both are joined before closure. The current subset does not
yet drive authenticated NTCP2 frames or claim end-to-end I2NP delivery.
Forced shutdown aborts and
drains the owned scope, and counters/leases are released only after join or
explicit bounded cleanup. Stale close notifications carry the local link ID so
they cannot remove a replacement. Outbound I2NP owners remain consuming and
are reserved for the later authenticated data-phase driver; they are not
claimed as delivered by the Plan 035 raw link helper.

NTCP2 address parsing accepts only validated literal fields and separates
configured literals from resolved dial targets. Reachability is an observation
candidate only: one peer cannot infer or publish an external address, and the
runtime never mutates RouterInfo or NetDB. Runtime TCP and malformed/fault
tests use loopback, the deterministic testkit, or an authorized isolated
testnet; no public-network stress or mutation is permitted. Local TCP success,
self-handshakes, and synthetic vectors remain non-advertised evidence.

## Bounded communication and resource-governor threats

Plan 022 treats queue exhaustion and slow consumers as explicit denial-of-
service conditions. Command and request queues wait only under a caller-owned
deadline and wakeable cancellation scope; event queues use a documented
drop-newest policy with counters; latest-state consumers receive only the
current version and can detect closure. Shutdown cancellation is a separate
path, so a full ordinary queue cannot starve supervisor teardown.

Request response senders are one-shot and are dropped when the requester is
cancelled or its deadline expires. A service that drops its response path is
reported as response closure rather than leaving an unbounded waiter. Queue
items own resource leases through enqueue and processing, and dropping a
receiver drops queued items so their charges are released.

Resource accounting is immediate-grant or immediate-denial under one bounded
lock; it has no hidden asynchronous waiter queue. Bundle requests validate all
classes before mutating usage, reject duplicate classes, and commit atomically,
so an exhausted class cannot leave partial grants. Limits are immutable for a
budget lifetime, class counts and estimates are bounded, and high-water/denial
counters saturate rather than wrap. Lease drop, consuming release, panic
unwind, cancellation, and forced task cleanup are all release paths. Snapshot
metadata contains only static class/channel identifiers and bounded counters;
payloads, secrets, peer identities, addresses, and destinations are excluded.
Release amounts greater than current usage are treated as an internal
accounting fault: usage is bounded back to zero, a saturating typed
underflow counter is recorded, and cleanup remains non-panicking. The fault
must remain visible in resource snapshots so a double release or ownership
bug cannot be mistaken for valid cleanup.

## Deterministic simulation threats and controls

Plan 023 is a local test boundary, not an emulation of I2P transports or a
security/interoperability claim. The scheduler opens no sockets, resolves no
names, and has no public-network path. Fault scripts are executable only in
the testkit and authorized isolated testnets; malformed or stress traffic must
not be sent to live peers.

Simulation inputs have hard limits for sleepers, pending deliveries, buffered
bytes, receiver queues, datagrams, stream segments, fault rules, duplicate
expansion, peers, and idle steps. Queue admission and resource leases happen
before scheduled payload ownership. Due work stays pending under receiver
backpressure, and reset/close paths purge or release queued ownership. A
manual clock never polls wall time; dropping its final handle deterministically
fails pending sleeps.

Reproducibility uses domain-separated seed derivation rather than one shared
mutable RNG, preventing task interleaving from changing unrelated components.
Replay records contain root seed, scenario, safe sequence/rule/outcome
categories, final monotonic time, and bounded resource/queue snapshots. They
never contain payloads, private keys, destinations, real addresses, or full
RouterInfo bytes. `TestPeer` deliberately has a redacted `Debug` impl and
keeps deterministic private identity material in memory-only zeroizing crypto
owners. Link leases remain attached to live endpoint handles, so tests must
drop those handles before asserting zero active links.

The harness does not hide detached Tokio tasks: it manually pumps the
scheduler, exposes the runtime cancellation token, and leaves any supervised
service ownership with the caller's Plan 021 graph. Teardown snapshots are
therefore evidence about queued simulation resources, not proof of a live
router's network cleanup behavior.

## Observability and non-claims

Plan 024 makes the default diagnostic boundary explicit. Fixed event names are
used for service registration/start/ready/failure/restart/stop, shutdown,
channel rejection, resource denial, and testkit fault/completion events. Safe
fields are validated static service/channel identifiers, classifications,
lifecycle and typed failure categories, bounded restart/counter values,
capacity/depth/usage values with units, monotonic durations, and synthetic
simulation link/sequence/rule metadata. The daemon owns subscriber
initialization; runtime and testkit crates never install global subscribers.

`HealthDetail` remains bounded for internal control flow, but its `Debug`
implementation is redacted. Aggregate runtime snapshots use a service
projection that omits detail text and retain only sorted, bounded channel and
resource snapshots. Snapshot generation performs no await while holding a
mutable runtime lock and is documented as an eventually coherent point-in-time
observation. No default event or snapshot retains full router hashes,
destination identities, keys, session material, LeaseSet secrets, packet
bodies, user traffic, filesystem paths, arbitrary error/panic text, precise
per-peer timing histories, or unbounded identity-bearing labels.

Plan 024's integrated scenarios validate clean startup/shutdown, bounded
overload, deterministic restart recovery, essential failure with forced
cleanup, and stream/datagram fault replay. They use only synthetic services,
manual time, fixed seeds, and bounded step counts. They do not prove anonymity,
privacy against traffic analysis, resilience, transport authentication,
interoperability, or safe public-network operation.

Nothing in the bootstrap proves anonymity, resistance to traffic analysis,
correctness against hostile peers, complete identity interoperability, secure
recovery/rotation, protocol interoperability, or safe public-network
operation. Local crypto/storage tests do not replace mixed-router evidence.
Malformed and stress tests must run only in an authorized isolated testnet.

## Plan 036 evidence and artifact sanitation

The Plan 036 integration path is a manual evidence boundary, not a public
network feature. Its manifest pins Java I2P and i2pd revisions, requires a
synthetic private network with reseed/bootstrap disabled, and requires
disposable identities and static keys. The committed preflight rejects private
key markers, identity/static-key files, and packet captures from the evidence
directory. Completed runs may retain only typed outcomes and hashes of
sanitized artifacts/configuration; raw addresses, peer identities, RouterInfo,
I2NP, keys, transcripts, and remote error text must be deleted.

The current checkout has no mixed-router artifacts or results because the
complete runtime wire adapter and authorized testnet are unavailable. This is
recorded as a blocker in `plans/036-closure.md`; neither the fixed-seed testkit
matrix nor pure fuzz campaigns are treated as interoperability evidence.
