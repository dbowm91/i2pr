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

Default logs and metrics must not expose full router hashes, destination
identities, keys, session material, LeaseSet secrets, packet bodies, user
traffic, or unbounded identity-bearing labels. Detailed identity-rich tracing
requires an explicit unsafe-debug mode and warning.

Nothing in the bootstrap proves anonymity, resistance to traffic analysis,
correctness against hostile peers, complete identity interoperability, secure
recovery/rotation, protocol interoperability, or safe public-network
operation. Local crypto/storage tests do not replace mixed-router evidence.
Malformed and stress tests must run only in an authorized isolated testnet.
