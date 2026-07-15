# Protocol support matrix

This matrix is intentionally explicit: every row describes the exact evidence
available, not just code presence. “Experimental structural subset” means
bounded codecs exist and are tested locally, but no mixed-router interoperability
or capability claim exists.

The fine-grained, machine-readable inventory for Milestone 1 is
[`specs/support.toml`](../specs/support.toml). Structural entries may be marked
`experimental` with repository evidence, but remain `advertised = false`; the
ledger does not itself publish protocol capabilities.

Plan 031 adds transport-neutral link, delivery, lifecycle, and resource
contracts. Plan 032 adds a Tokio-free NTCP2 cryptographic/transcript foundation
plus static-key persistence, and Plan 033 adds bounded handshake codecs and
consuming action-driven state machines. These are experimental local evidence,
not complete NTCP2 protocol support; no transport capability is advertised or
published in RouterInfo.

| Protocol area | Status | Planned milestone | Specification/source starting point | Test-vector status | Interoperability status |
| --- | --- | --- | --- | --- | --- |
| Common identity, keys, and certificates | Experimental structural subset plus local type-4/type-7 execution | 1 | `specs/protocols/01-common-identity-crypto.md`, pinned source in `specs/SOURCES.md` | Locally authored structural bytes, Ed25519 mutation tests, and X25519 derivation tests; no independent router vectors | None |
| Router identity generation and local RouterInfo signing | Experimental local lifecycle | 1 | `plans/013-m1-identity-crypto-storage.md`, ADRs 0004 and 0007 | Deterministic injected-RNG generation, exact signed-region verification, save/reload and mutation tests | None |
| Private router identity storage | Experimental local persistence | 1 | `plans/013-m1-identity-crypto-storage.md`, ADR 0006 | Version/length/truncation/integrity/permission/concurrency tests; no external storage interoperability claim | None |
| I2NP envelope and header variants | Experimental structural subset; not advertised | 1, 3–6 | `specs/protocols/02-i2np.md`, pinned 0.9.69 source in `specs/SOURCES.md` | Locally authored standard/short vectors, truncation, size, checksum, and trailing-byte tests; hashed fixture manifest | None |
| I2NP type registry and selected body codecs | Experimental structural subset; NetDB body semantics deferred | 1, 4 | `specs/protocols/02-i2np.md`, `crates/i2pr-proto/src/i2np/mod.rs` | Fixed and malformed local vectors for DatabaseLookup, DatabaseSearchReply, DeliveryStatus, DatabaseStore framing, and fixed tunnel framing | None |
| I2NP tunnel, garlic, data, and later record semantics | Deferred or framing-only | 1, 5–6 | `specs/protocols/02-i2np.md`, `specs/protocols/05-tunnels.md`, `specs/protocols/06-garlic-ecies-leasesets.md` | Bounded `Deferred`/`Opaque` retention and shape checks only; no crypto or state-machine vectors | None |
| NTCP2 crypto/transcript foundation | Experimental local subset; not advertised | 3 | `specs/protocols/03-ntcp2.md`, ADR 0011 | Independent deterministic primitive/transcript vectors and hashed manifest; no router interoperability | None |
| NTCP2 handshake codecs and state machines | Experimental local subset; not advertised | 3 | `specs/protocols/03-ntcp2.md`, ADR 0012 | Fixed/malformed/bounded state and policy tests; no mixed-router interoperability, sockets, or capability claim | None |
| Reseed and RouterInfo publication | Not implemented | 4 | `specs/protocols/04-reseed-netdb.md` | None imported | None |
| Network tunnels and transit participation | Not implemented | 5 | `specs/protocols/05-tunnels.md` | None imported | None |
| Classic LeaseSet structural codec | Experimental structural subset; LeaseSet2-family deferred | 6 | `specs/protocols/06-garlic-ecies-leasesets.md` | Local Lease/LeaseSet vectors and negative tests; no independent router vectors | None |
| LeaseSet2, EncryptedLeaseSet, and MetaLeaseSet | Deferred | 6 | `specs/protocols/06-garlic-ecies-leasesets.md` | Explicit rejection/deferred framing only | None |
| I2P streaming | Not implemented | 6 | `specs/protocols/07-streaming.md` | None imported | None |
| SAM | Not implemented | 7 | `specs/protocols/08-sam.md` | None imported | None |
| SSU2 | Not implemented | 8 | `specs/protocols/09-ssu2.md` | None imported | None |
| I2CP | Not implemented | 9 | `specs/protocols/10-i2cp-service-tunnels.md` | None imported | None |
| Service tunnels | Not implemented | 10 | `specs/protocols/10-i2cp-service-tunnels.md` | None imported | None |

The workspace may name the `common` and `i2np` namespaces and now includes the
non-networked `i2pr-runtime` supervision crate, but runtime infrastructure is
not protocol support evidence. Plan
013 adds local type-4/type-7 execution plus a private identity file. These
local operations do not establish mixed-router protocol support, complete
signature/encryption coverage, transport support, network compatibility, or
capability advertisement. Legacy NTCP and SSU1 are outside the MVP target
unless a later plan explicitly changes scope.

The I2NP implementation recognizes the pinned message identifiers and strictly
decodes standard, obsolete-SSU, and NTCP2/SSU2 short headers. It fully models
the structural fields of DatabaseLookup, DatabaseSearchReply, DeliveryStatus,
and DatabaseStore; only classic LeaseSet payloads reuse an existing structural
codec. Compressed RouterInfo, LeaseSet2-family records, tunnel-build record
cryptography, garlic/data semantics, duplicate/expiry policy, routing,
transport authentication, and capability advertisement remain deferred. No
I2NP row is `advertised = true`, and no row claims mixed-router support.

DatabaseLookup legacy and ECIES reply-key/tag wrappers are non-cloneable and
zeroizing structural containers. They provide memory hygiene only; they do not
implement encrypted reply semantics, key derivation, decryption, or NetDB
behavior.

Each future protocol row must be updated with exact targeted proposal/spec
revisions, limits, malformed-input behavior, vectors, and mixed-router evidence
before its status changes.
