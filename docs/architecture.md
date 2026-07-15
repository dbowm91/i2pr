# Architecture

This document records the current modular-monolith boundaries and ownership.
The implemented common-structure codecs remain structural data handling, not
router behavior or an interoperability claim.

## Four planes

The intended modular monolith is organized into four conceptual planes:

| Plane | Responsibility | Current bounded status |
| --- | --- | --- |
| Data | Protocol representations, authenticated links, messages, and network tunnel traffic | Bounded common-structure and initial I2NP models plus Ed25519/X25519 wrappers and local RouterInfo signing; no sockets or network behavior |
| Control | Configuration, lifecycle, health, cancellation, supervision, and resource budgets | Runtime-neutral core contracts plus the concrete non-networked `i2pr-runtime` supervisor |
| Client | Destinations, LeaseSets, streaming, SAM, and I2CP adapters | Not implemented |
| Service | HTTP, SOCKS5, IRC, generic TCP, and local service-tunnel composition | Not implemented |

Network tunnels carry router-to-router I2P traffic and are distinct from
application service tunnels, which eventually connect a local application to a
destination. The latter must not import transport internals or peer-profile
storage.

## Current crate graph

The current bounded workspace has seven crates, including the test-only
simulation crate:

```text
i2pr-proto  <- i2pr-crypto <- i2pr-storage
     ^              ^               ^
     |              |               |
i2pr-core <- i2pr-runtime <- i2pr-daemon (composition root)
     ^             ^
     |             |
 i2pr-testkit (test/simulation dependency only)
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
managers, bounded restart policy, and graceful/forced shutdown. `i2pr-testkit`
provides deterministic clocks, randomness, and bounded fault vocabulary for
tests. The daemon owns CLI/configuration and is the composition root, but its
live command remains intentionally disabled.

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
    mod.rs       date.rs       keys.rs       mapping.rs
    certificate.rs  identity.rs  router_info.rs  lease.rs
  i2np/
    mod.rs       header.rs     netdb.rs      delivery.rs
    tunnel.rs    deferred.rs
```

The private `common_impl.rs` and `i2np_impl.rs` units retain the existing
strict codec glue and helper visibility while grouped leaf modules expose only
stable structural names. This compatibility-oriented arrangement avoids
making parsing helpers public merely to complete a mechanical split. Future
protocol work should add behavior to the owning leaf namespace and preserve
the crate-root re-export façade.

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

Generated and reconstructed private seeds are held by zeroizing owners during
crypto operations. Storage encoding and file-read buffers are also zeroizing;
the `DatabaseLookup` reply-key/tag wrappers in `i2pr-proto::i2np::netdb` are
non-cloneable and redact their contents. These measures reduce ordinary
post-use retention but do not provide encrypted reply semantics or defeat
process compromise, allocator copies, swap, core dumps, or every compiler or
platform memory-retention behavior.

### Identity storage boundary

`i2pr-storage` stores only the private router identity format described by ADR
0006. It is not a NetDB or public RouterInfo store. It rejects symlinks,
overly-permissive Unix paths, malformed/trailing/oversized data, unsupported
versions and integrity failures; it never regenerates an existing identity.
The explicit create-only operation uses a same-directory temporary file,
flush/sync, an atomic no-replace install, cleanup, and directory sync where the
platform supports it.

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
completion.

Shutdown first cancels every manager and joins within the configured bounded
deadline. Remaining managers are then aborted and joined. The report records
graceful versus forced cleanup, final typed completions, joined-task count, and
zero remaining owned tasks. No runtime service opens sockets or adds protocol,
NetDB, tunnel, client, API, or plugin behavior.

## Composition and communication

The daemon will eventually compose supervised services and pass each service
only the narrow handles or capabilities it needs. A global mutable router
context or unrestricted service locator is not an architectural default.

The current identity CLI is deliberately not a runtime service: `identity
generate` is the only operation allowed to create the private identity file,
`identity inspect` only loads and summarizes it, and `run --dry-run` remains
side-effect-free. No identity command opens a listener or publishes a record.

The implemented service model classifies work as essential, restartable,
degradable, or optional. Each long-lived service declares startup dependencies,
readiness, health signals, owned resources, cancellation, and graceful/forced
shutdown behavior through `i2pr-runtime`. Later Plans 022–024 may add bounded
channels, resource-governor integration, deterministic network simulation,
and observability; they must preserve this ownership boundary.

## External boundaries

Future `synvoid` integration belongs behind a local Unix-socket or loopback
service boundary. It is not a routing-core dependency. Future `eggsec`
integration belongs in `i2pr-testkit`, private-testnet orchestration, and
stable fixtures; production routing code must not expose unrestricted testing
hooks.

The project is a modular monolith, not a runtime plugin platform. Compile-time
components or authenticated out-of-process interfaces are preferred to
in-process Rust plugins.
