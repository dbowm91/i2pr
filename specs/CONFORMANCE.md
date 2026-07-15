# Protocol conformance and evidence policy

## Claim model

`i2pr` must not claim protocol support from code presence alone. A protocol or feature may be marked implemented only when the applicable evidence below exists:

1. strict decode and canonical encode tests;
2. authoritative golden vectors or independently generated cross-implementation vectors;
3. malformed, truncated, oversized and semantically invalid input tests;
4. state-machine success, failure, timeout, cancellation and teardown tests;
5. explicit memory, queue, task, retry and cryptographic-work bounds;
6. replay, duplicate, expiry and clock-skew tests where relevant;
7. mixed-router interoperability against at least two independent implementations for router-to-router protocols;
8. documentation of unsupported and compatibility-only behavior;
9. no advertised RouterInfo, I2NP, API or transport capability beyond the tested subset.

## Machine-readable support ledger

`specs/support.toml` is a declarative inventory of planned protocol surfaces;
it is not a capability registry. Its schema is versioned by the top-level
integer `schema` field. Each `[[surface]]` entry contains `id`, `protocol`,
`structure`, `scope`, `status`, `evidence` (an array of repository references),
and `advertised` (a boolean).

Plan 037's local NTCP2 corrections are evidence for bounded ownership and
parser behavior only. They do not satisfy the mixed-router requirement: Java
I2P and i2pd results must include sanitized run records, artifact/configuration
hashes, and reproduction identifiers before any NTCP2 row can advance or be
advertised.

The allowed `status` values are `not-implemented`, `implemented`,
`compatibility`, `experimental`, `deferred`, `legacy-reject`, and `open`.
`not-implemented` is the initial state for Milestone 1. `implemented` is only
valid after the claim requirements above are met for that exact surface;
`scope` uses the planning labels from `specs/README.md` and does not itself
make a support claim. An entry may set `advertised` to `true` only after the
capability-advertisement requirements below are met. Empty `evidence` and
`advertised = false` therefore describe the initial, non-claiming ledger.

Java I2P and I2P+ share lineage and count as one implementation family for independence. The preferred router-to-router interoperability pair is Java I2P or I2P+ plus i2pd. Emissary/go-i2p should be added where its current implementation is complete enough for the tested surface.

Plan 036's required mixed-router evidence is recorded separately from local
codec/runtime evidence. The pinned manual lane is
`tests/integration/ntcp2/manifest.toml`; its preflight must fail closed for
public networking, reseed/bootstrap, operational identities, unpinned
artifacts, and unsanitized evidence. A local self-handshake, loopback socket,
fixed-seed simulation, or fuzz result cannot satisfy the two-implementation,
two-direction requirement.

Plans 038/040/041 define how that evidence may be prepared, not a relaxation
of the claim model. Plan 041's reference-only Java I2P/i2pd crosscheck is a
control for the harness and never counts as i2pr interoperability. The
Ubuntu-only harness has a network-enabled preparation phase
for declared packages and locked reference builds, followed by a
network-isolated execution phase. Execution requires disposable namespaces
with only a veth connection, no default routes, no DNS, and no public egress;
isolation and cleanup are part of the result. Environment smoke and a Java
I2P/i2pd reference crosscheck validate the harness only. They cannot advance
an i2pr support row. Only sanitized, bounded authenticated i2pr-to-reference
runs in both directions can supply mixed-router evidence for NTCP2.

Plan 043 makes the evidence path an ordered build-system contract. The
promotion sequence is `contract`, `reference-build`,
`reference-offline-reuse`, `environment-smoke`,
`reference-crosscheck-ipv4`, `i2pr-handshake-smoke-ipv4`, `full-matrix`,
`evidence-validation`, and `cleanup-verification`. Preparation may use the
declared Ubuntu package/source network access; execution must consume verified
offline caches in disposable namespaces with no default route, DNS, or public
egress. The reference-only crosscheck must pass before i2pr scenarios are
eligible, but it never advances an i2pr support row.

The exact host/cache contract is recorded in
[`tests/integration/ntcp2/references.lock.toml`](../tests/integration/ntcp2/references.lock.toml)
and the operational commands and current status are recorded in
[`tests/integration/ntcp2/README.md`](../tests/integration/ntcp2/README.md) and
[`docs/architecture/interop-apparatus.md`](../docs/architecture/interop-apparatus.md).
Evidence records and the Plan 043 aggregate manifest are specified in
[`tests/integration/ntcp2/evidence/README.md`](../tests/integration/ntcp2/evidence/README.md).
Validation must reject missing/extra passed records, placeholders, digest
mismatches, incomplete direction coverage, forbidden material, and failed
cleanup verification. The retained artifact allowlist is sanitized JSON and
approved hashes only.

Cleanup is an independent conformance property: an always-run cleanup path and
clean-host verifier must prove zero residual prefixed namespaces/veths,
reference or launcher processes, secret-bearing run roots, forbidden retained
files, and attributable host nftables/routes/forwarding changes. A protocol
pass with failed cleanup verification is not a pass.

The current checkout has no sanitized i2pr-to-reference record or completed
successful aggregate manifest. The workflow-integrated clean-host verifier is
present, but no successful Plan 043 run has established conformance. Plan 044
implements the mixed-router composition, directional scenario expansion,
strict launcher rendering, and the non-echo data-phase oracle, but no
authorized mixed-router execution has been completed. Therefore NTCP2 remains
`experimental` and `advertised = false`; workflow scaffolding, local launcher
success, loopback, vectors, testkit output, and Plan 041 reference-control
records cannot change that status.

## Source-to-code traceability

Every protocol module should identify:

- the dossier in this directory;
- the official specification path and pinned commit used during implementation;
- relevant proposal numbers;
- the external-standard revision, if any;
- the test-vector origin;
- deliberate deviations or stricter validation;
- any compatibility behavior inferred from implementation evidence.

This may be recorded in module documentation, a nearby `README`, test metadata, or an implementation plan. Avoid scattering unexplained protocol constants through runtime code.

## Decoder policy

Network, disk, reseed and local-API inputs are untrusted. Decoders must:

- enforce a caller-visible maximum before allocation;
- use checked arithmetic for offsets, lengths, counts and time computations;
- distinguish truncation, malformed encoding, unsupported type, semantic invalidity and policy rejection;
- consume exactly the expected input for strict top-level decoding;
- reject duplicate fields or keys where the format requires uniqueness;
- validate canonical ordering where signatures or hashes depend on canonical bytes;
- preserve the signed byte representation when reserialization could change verification semantics;
- avoid recursive structures without explicit depth limits;
- never panic on arbitrary bytes;
- avoid retaining attacker-controlled backing buffers after parsing unless bounded and intentional.

Unknown blocks or options may be ignored only when the specification explicitly defines forward-compatible skipping. The parser must still validate the enclosing length and resource bounds.

For Plan 034 NTCP2 frames, AEAD verification is a mandatory gate before block
iteration, unknown-block skipping, or semantic output. Transmit and receive
counters are independent and direction-specific; accepted frames advance once,
failed authentication is terminal, and the forbidden nonce value is never
emitted. The data-phase dossier defines no periodic rekey threshold, so
counter exhaustion requires a fresh Noise handshake. Fuzz and malformed tests
must preserve these invariants while remaining bounded and payload-redacted.

## Encoder policy

Encoders must:

- produce deterministic canonical output where the protocol defines canonicalization;
- reject values that cannot be represented without truncation;
- calculate exact encoded length before or during bounded emission;
- avoid implicit platform-width integer conversions;
- emit only capability/version combinations supported by the current runtime;
- keep private key material, session keys and plaintext authentication data out of logs and `Debug` output.

Round-trip tests are necessary but insufficient because two matching bugs may round-trip. Include fixed expected bytes and cross-implementation decoding.

## State-machine policy

Transport, NetDB, tunnel, garlic, streaming and API protocols must use explicit states and legal transitions. Each state machine must define:

- accepted messages/events per state;
- deadlines and retry budgets;
- duplicate and reordered input behavior;
- cancellation points;
- owned resources and cleanup on every terminal path;
- peer-visible errors or silent-drop behavior;
- whether malformed input terminates a message, session, transport link, tunnel build, destination or client connection.

No retry loop may be unbounded. Backoff, peer rotation and global concurrency limits must be tested under deterministic time.

## Cryptographic conformance

Do not implement cryptographic primitives locally. Wrap reviewed libraries with protocol-specific key and nonce types.

Tests must cover:

- official or independently verified positive vectors;
- invalid keys, signatures, tags and authentication data;
- nonce/counter boundary behavior;
- all-zero or low-order X25519 results according to the relevant specification/library contract;
- key-type and encoded-length mismatch;
- domain-separation and network-ID inputs;
- transcript/hash changes from one-bit mutations;
- key erasure or bounded lifetime where library support permits;
- failure without unauthenticated plaintext exposure.

Legacy algorithms required only for reading deployed data must be isolated from new identity generation and ordinary emission policy.

## Interoperability matrix

Each milestone should maintain an executable or machine-readable matrix similar to:

| Protocol | Direction/role | Java I2P | i2pd | I2P+ | Emissary/go-i2p | Evidence |
|---|---|---:|---:|---:|---:|---|
| NTCP2 | initiator | pending | pending | family duplicate | optional | test log/vector |
| NTCP2 | responder | pending | pending | family duplicate | optional | test log/vector |
| NetDB lookup | requester | pending | pending | family duplicate | optional | trace/result |
| Tunnel build | creator | pending | pending | family duplicate | optional | testnet artifact |
| Transit tunnel | participant | pending | pending | family duplicate | optional | testnet artifact |
| Streaming | connect/listen | pending | pending | family duplicate | optional | client transcript |
| SAM | client-facing server | client tests | client tests | client tests | optional | protocol transcript |
| SSU2 | initiator/responder | pending | pending | family duplicate | optional | packet/test logs |
| I2CP | router-facing server | client tests | client tests | client tests | optional | protocol transcript |

Interoperability tests must run only in an authorized private or controlled mixed-router testnet until the milestone plan explicitly permits public-network observation.

## Fuzzing targets

At minimum, fuzz:

- all top-level common-structure and I2NP decoders;
- RouterInfo, Destination, LeaseSet and signed-container parsing;
- NTCP2 and SSU2 plaintext block parsers after authenticated decryption;
- handshake state transition inputs with deterministic crypto seams where safe;
- tunnel build records and tunnel message fragmentation/reassembly;
- garlic clove and ECIES payload parsing;
- streaming packets and option blocks;
- SAM and I2CP framing, tokenization and option parsing;
- HTTP proxy request-line/header rewriting and SOCKS negotiation.

Fuzz harnesses must have bounded input size and should assert no panic, no excessive allocation, no infinite loop and stable error classification where practical.

Plan 014 maintains these entry points in the separate nightly `fuzz/`
workspace: every public common decoder (`date`, `date32`, `hash`, `mapping`,
certificate/key certificate, key-and-cert, identity, destination, address,
RouterInfo, Lease, and LeaseSet), the three I2NP header decoders, and an
`i2np_bodies` dispatch target covering each independently complex I2NP body.
The smoke script is opt-in and bounded; its fuzz-only dependency is excluded
from production workspace and MSRV checks.

## Differential tests

Use differential testing selectively. Valuable comparisons include:

- canonical structure serialization;
- Base64/Base32 and hash derivation;
- signature verification and RouterInfo hashes;
- NTCP2/SSU2 block encoding after supplying identical keys/nonces;
- tunnel build-record crypto;
- streaming packet encoding;
- SAM command parsing and response status.

Do not expose private test keys to public infrastructure or depend on nondeterministic production routers for unit tests. Prefer local fixtures and dedicated test identities.

## Security regression corpus

Every protocol parser should retain minimized fixtures for discovered failures:

- truncation at every field boundary;
- maximum and maximum-plus-one lengths/counts;
- duplicate, unknown and out-of-order fields;
- expired, future-dated and skewed timestamps;
- invalid signatures and authenticated-encryption tags;
- replayed handshakes, packets, I2NP IDs and tunnel records;
- decompression/archive expansion limits for reseed bundles;
- fragmented messages exceeding per-message or per-peer budgets;
- slow-read/slow-write behavior and partial frames;
- cancellation during cryptographic work, persistence and publication.

A production bug is not closed until a fixture or deterministic test prevents recurrence.

## Capability advertisement

Capability publication is a security and interoperability contract. Before changing `router.version`, RouterInfo capabilities, transport addresses/options, LeaseSet type support, SAM version negotiation or I2CP behavior:

1. identify the exact feature implications in the official specifications;
2. verify all implied mandatory behavior is implemented;
3. add mixed-router tests for the changed claim;
4. test downgrade/unsupported peers;
5. update the relevant dossier and protocol-support matrix.

`i2pr` should initially advertise the lowest truthful current feature level compatible with its implemented subset, not mimic another router’s release string.

## Evidence retention

Store stable protocol vectors and minimized malformed fixtures in the repository. Store large captures, generated testnets and sensitive operational logs outside Git history, with scripts and hashes sufficient to reproduce them. Redact live peer identities, IP addresses, destination keys, session keys and potentially identifying timing data before retaining or publishing artifacts.
