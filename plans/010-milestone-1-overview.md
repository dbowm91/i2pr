# Milestone 1 overview: protocol model, codecs, identity, and storage primitives

## Objective

Implement the trustworthy data and persistence foundation required by every later router subsystem without opening sockets, joining I2P, advertising capabilities, or implementing transport/tunnel state machines.

Milestone 1 turns `i2pr-proto` from a namespace skeleton into a bounded, testable protocol model; introduces protocol-specific cryptographic wrappers; establishes persistent router identity storage; and creates the evidence and fuzzing infrastructure required before later networking work.

## Governing sources

Implementation must follow:

- `GUARDRAILS.md`
- `specs/README.md`
- `specs/CONFORMANCE.md`
- `specs/protocols/01-common-identity-crypto.md`
- `specs/protocols/02-i2np.md`
- relevant pinned source entries in `specs/SOURCES.md`

When specification and implementation evidence disagree, record the conflict and stop the affected feature rather than silently selecting behavior.

## Preconditions

- `plans/002-milestone-0-corrective-closure.md` is complete.
- CI is green.
- MSRV policy is tested.
- The protocol-support ledger reports Milestone 1 surfaces as not implemented.
- No unresolved licensing or provenance issue affects planned fixtures or dependencies.

## Plan set and dependency order

Execute these plans in order unless a handoff explicitly proves the dependency is unnecessary:

1. `plans/011-m1-codec-foundation.md`
2. `plans/012-m1-common-structures.md`
3. `plans/013-m1-identity-crypto-storage.md`
4. `plans/014-m1-i2np-evidence-fuzzing-closure.md`

Agents may work in parallel only where file ownership and type contracts are already fixed. Avoid parallel edits to core codec traits, error enums, identity types, or workspace manifests.

## Intended workspace changes

Milestone 1 may add:

```text
crates/
  i2pr-proto/
  i2pr-crypto/
  i2pr-storage/
```

`i2pr-crypto` and `i2pr-storage` should be created only when their first concrete implementation plan reaches that step. Do not create transport, NetDB, tunnel, client, API, or service-tunnel crates.

Expected dependency direction:

```text
i2pr-proto        i2pr-core
     ^                ^
     |                |
i2pr-crypto            |
     ^                 |
     |                 |
i2pr-storage ----------+
     ^
     |
i2pr-daemon

i2pr-testkit may appear only in dev-dependencies.
```

The exact relationship between `i2pr-proto` and `i2pr-crypto` must avoid a cycle. Protocol types should describe algorithm identifiers and encoded public material; cryptographic execution should live behind wrappers in `i2pr-crypto`. If signing convenience methods are needed on protocol values, implement them in the crypto crate through extension functions or dedicated builders rather than reversing dependency direction.

## Milestone-wide constraints

### No networking

Do not:

- bind TCP or UDP sockets;
- reseed;
- perform NetDB operations;
- construct tunnels;
- implement SAM or I2CP listeners;
- emit a RouterInfo to a live router;
- advertise any transport or router capability.

### Strict input handling

All public decoders must:

- receive or enforce an explicit maximum;
- use checked arithmetic;
- distinguish truncation, malformed encoding, unsupported type, semantic invalidity, and policy rejection;
- consume exactly the expected bytes for strict top-level decoding;
- reject noncanonical representations where required;
- avoid panic on arbitrary input;
- avoid attacker-controlled unbounded allocation.

### API restraint

Do not introduce speculative abstractions such as generic transport codecs, universal cryptographic providers, pluggable serialization frameworks, or broad storage backends. Add interfaces only for concrete Milestone 1 consumers.

### Secrets

Secret-bearing values must not implement revealing `Debug`, `Display`, serialization, or cloning without explicit justification. Filesystem persistence must use atomic replacement and restrictive permissions where supported.

### Provenance

Every fixed vector and fixture must identify its source and license/provenance. Generated vectors must record the generator and inputs. Do not copy code or opaque fixture corpora from another router.

## Required deliverables

By Milestone 1 closure, the repository should contain:

- bounded read cursor and bounded canonical encoder facilities;
- stable protocol error taxonomy;
- common integer, string, mapping, certificate, key-certificate, identity, destination, router-address, RouterInfo, Lease, and required LeaseSet representations;
- initial I2NP envelope and the message types needed by Milestones 3–6;
- protocol-specific key/signature wrappers using reviewed libraries;
- router identity generation and persistence;
- RouterInfo signing and verification;
- golden, malformed, boundary, property, and fuzz tests;
- source-to-code traceability records;
- updated protocol-support ledger and known limitations.

## Milestone exit criteria

- All workspace quality checks pass on normal and MSRV toolchains.
- Public decoders have fixed positive vectors and negative/boundary coverage.
- Fuzz targets cover all public top-level Milestone 1 parsers.
- A router identity can be generated, stored atomically, reloaded, and used to sign and verify a canonical RouterInfo.
- Disk-loaded data is fully revalidated.
- Unsupported algorithms and structures fail with stable typed errors.
- No secret material appears in logs, snapshots, or default debug output.
- No network behavior or capability advertisement exists.
- Protocol support is marked implemented only for the exact codec and cryptographic surfaces supported by evidence; router-level interoperability remains unclaimed.

## Handoff standard

Each plan handoff must include:

- changed files and public APIs;
- dependency additions and feature flags;
- source/specification references;
- test-vector provenance;
- exact commands and results;
- fuzz targets added and seed corpus location;
- security decisions and resource limits;
- unsupported behavior and open questions;
- deviations from the plan.

A handoff that merely states that tests pass is insufficient.