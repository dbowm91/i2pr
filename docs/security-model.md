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
