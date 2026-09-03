# Plan 155 — Milestone 8 SSU2 v2 protocol foundation and addresses

Status: **registered; first M8 executable after Plan 153 closes**.

Depends on:

- Plan 153 passed;
- Plan 154 roadmap authority.

Blocks Plan 156.

## 1. Goal

Create the runtime-neutral SSU2 v2 protocol crate and establish strict, vector-backed address/header/block primitives without implementing a live handshake or opening UDP sockets.

This pass should leave a small, auditable foundation analogous to the early `i2pr-transport-ntcp2` protocol work.

## 2. First task: refresh source authority

Before code changes, refresh the Milestone 8 source ledger because the repository explicitly treats milestone start as a refresh trigger.

Review and update as required:

```text
specs/SOURCES.md
specs/IMPLEMENTATIONS.md
specs/protocols/09-ssu2.md
specs/support.toml
docs/protocol-support.md   # only through the existing generator if generated
```

Record:

- exact official SSU2 v2 spec snapshot/revision used;
- current spec `accurateFor` metadata;
- Proposal 159 and 165 only as historical/design context where the current spec supersedes them;
- current PQ-hybrid SSU2 v3/v4 compatibility-watch status;
- Java I2P 2.13.0 exact commit `9134f808337b401e8e53c73734c81fab04280c9d`;
- i2pd 2.61.0 exact commit `635b013a612ff47278ef02acf8580a28e10e26c5`;
- clean-room restriction: specifications/observed behavior may be used, implementation code is reference-only.

Do not advance a protocol-support surface to advertised/production merely because the dossier was refreshed.

## 3. Create `i2pr-transport-ssu2`

Add workspace member:

```text
crates/i2pr-transport-ssu2
```

Required crate properties:

- `#![forbid(unsafe_code)]` or workspace-equivalent;
- no Tokio dependency;
- no socket/file/network I/O;
- no async runtime ownership;
- dependencies flow only downward to existing protocol/crypto/transport-neutral crates and reviewed crypto libraries;
- no dependency on `i2pr-runtime`, `i2pr-daemon`, or `i2pr-testkit` in production dependencies.

Likely module split:

```text
src/lib.rs
src/constants.rs
src/address.rs
src/header.rs
src/block.rs
src/packet.rs
```

Do not add handshake/data-session state machines yet; reserve modules/names only if actually useful.

## 4. Extend the generic transport kind

Add:

```rust
TransportKind::Ssu2
```

to `i2pr-transport` and update every exhaustive match/test/documentation site.

Do not introduce an SSU2-specific `TransportManager`.

Acceptance requires existing NTCP2 transport-manager tests remain unchanged in semantics.

## 5. SSU2 RouterAddress model

Implement strict runtime-neutral parsing/validation for classical SSU2 v2 RouterAddress material.

At minimum model:

- transport style / version (`v=2`);
- static X25519 key `s` exactly 32 bytes after I2P Base64 decoding;
- intro key `i` exactly 32 bytes;
- IP host/address where present;
- UDP port;
- MTU with SSU2 minimum policy (1280) and a conservative upper bound;
- capabilities/options required to distinguish direct/firewalled/introducer addresses;
- introducer metadata as current spec requires, with explicit bounded count;
- IPv4 vs IPv6 family without DNS/hostname resolution inside the protocol crate.

### Address rules

- reject duplicate/conflicting singleton options;
- reject malformed Base64 and wrong key lengths;
- reject zero/invalid ports;
- reject hostnames where the SSU2 address field requires numeric IP material;
- reject impossible family/address combinations;
- reject v3/v4 as unsupported-version values with a distinct typed error, not malformed-v2 parsing;
- reject unknown future versions safely;
- preserve unknown noncritical options only if required for canonical RouterAddress round-trip; otherwise reject/ignore according to the official structure rules and document the choice;
- `Debug` must not dump private/local secret material. Public RouterAddress keys may be represented only in privacy-safe abbreviated/redacted form consistent with current repo policy.

Distinguish at the type level where practical:

```text
direct endpoint
firewalled/introducer-only material
configured listen address
resolved dial target
```

Do not allow a parser result to imply reachability/publication approval.

## 6. Constants and packet/header primitives

Define constants from the current spec with source comments/tests, including:

- protocol/version identifiers;
- network ID field constraints;
- connection-ID sizes;
- token size;
- minimum/maximum datagram/packet sizes;
- header sizes for long/short forms;
- packet/message type IDs;
- block type IDs;
- maximum reasonable block counts and aggregate unknown-block budget;
- maximum RouterInfo fragment count/size accepted during establishment;
- I2NP fragment metadata field bounds.

Avoid magic numbers in later plans.

### Header types

Implement strict encode/decode data types for the SSU2 packet/header structures needed by later handshakes and data packets, but do not perform cryptographic header protection yet.

Parsing must:

- require exact minimum bytes before field access;
- distinguish long vs short header forms using the spec-defined context rather than heuristic guessing;
- reject unsupported versions/network IDs as typed errors;
- expose connection IDs and packet numbers only after structural validation;
- consume/return exact slices rather than silently accepting trailing bytes where the structure is exact-sized.

## 7. Bounded payload block codec

Implement the authenticated plaintext block vocabulary required for M8, with strict length/count/order rules from the current SSU2 spec.

Expected block families include, where current v2 spec uses them:

```text
DateTime
Address
Options
RouterInfo / RouterInfo fragments
I2NP message
First I2NP fragment
Follow-on I2NP fragment
ACK
RelayRequest
RelayResponse
RelayIntro
PeerTest
NewToken
PathChallenge
PathResponse
Termination
Padding
```

Do not guess block IDs or layouts from memory; resolve them from the refreshed spec.

For each block:

- explicit maximum body length;
- strict required field sizes;
- no panics on truncation;
- bounded collection counts;
- typed errors;
- unknown-block behavior exactly according to current spec, with an aggregate byte/count ceiling;
- Padding/Termination ordering restrictions enforced where specified.

This plan may define ACK/block structures, but ACK range interpretation/loss semantics belong to Plan 157.

## 8. Fixture/vector discipline

Create an SSU2 fixture namespace, for example:

```text
tests/fixtures/ssu2/
```

with a manifest governed by the repository fixture rules.

Add a narrow checker:

```text
scripts/check-ssu2-vectors.sh
```

or extend the fixture-manifest mechanism if it cleanly supports the same invariant.

Fixtures must have explicit provenance:

- spec-derived constructed vector;
- clean-room independently generated reference vector;
- sanitized captured bytes, if later introduced.

Never commit private keys/tokens that are real runtime secrets. Deterministic test keys are permitted when explicitly marked test-only and non-operational.

Required vectors/tests in this pass:

- direct IPv4 RouterAddress parse/serialize;
- IPv6 structural parse/serialize;
- introducer/firewalled form;
- malformed key lengths/base64;
- duplicate/conflicting options;
- version 2 accepted;
- version 3/4 classified unsupported/deferred;
- unknown version rejected safely;
- representative long/short header structure fixtures;
- every implemented block positive round-trip;
- truncation at every meaningful field boundary for representative blocks;
- unknown-block byte/count ceiling;
- over-limit RouterInfo/I2NP fragment metadata rejection.

## 9. Documentation

Add/update:

```text
docs/architecture/i2pr-transport-ssu2.md
specs/protocols/09-ssu2.md
specs/support.toml
plans/155-status.md   # on closure
```

Architecture documentation must state clearly:

- protocol crate is runtime-neutral;
- UDP is not active yet;
- no handshake/data-phase interoperability is claimed;
- v2 is target, v3/v4 PQ deferred;
- SSU1 unsupported.

## 10. Non-goals

Do not implement in this plan:

- Noise handshake transcript;
- TokenRequest/Retry state machine;
- AEAD/header protection;
- packet-number replay window;
- ACK/loss controller;
- I2NP reassembly state machine;
- UDP sockets/runtime service;
- peer tests/relay;
- transport selection;
- public address publication;
- independent interoperability.

## 11. Validation

At minimum:

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo test --locked -p i2pr-transport --all-targets
cargo test --locked -p i2pr-transport-ssu2 --all-targets
cargo test --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ssu2-vectors.sh
```

Update static boundary scripts if necessary so they explicitly forbid Tokio/socket I/O in `i2pr-transport-ssu2` rather than relying on convention.

## 12. Acceptance criteria

Plan 155 passes only when:

1. refreshed SSU2 source authority is recorded.
2. new crate builds under MSRV/current toolchain.
3. dependency direction is correct and mechanically enforced.
4. `TransportKind::Ssu2` is integrated without changing NTCP2 semantics.
5. v2 direct and introducer RouterAddress forms are strictly modeled.
6. v3/v4 PQ versions are distinctly classified unsupported/deferred.
7. malformed/duplicate/conflicting address options fail with typed errors.
8. packet/header structural codecs are bounded and fixture-backed.
9. required v2 plaintext blocks implemented in this foundation round-trip correctly.
10. malformed/truncated/oversized block cases are deterministic and panic-free.
11. unknown-block handling is bounded and spec-correct.
12. fixture manifest/vector checker is green.
13. protocol/runtime boundary checker covers the new crate.
14. no UDP socket is opened anywhere by the new crate.
15. no handshake/interoperability/public support claim is made.
16. full workspace quality floor passes.
17. `plans/155-status.md` records exact tests/results and sets `next_executable_plan = 156` only after closure.

## 13. Stop conditions

Stop and narrow before proceeding if:

- current v2 spec and reference implementations disagree on a wire field in a way the spec cannot resolve;
- supporting the required RouterAddress form requires changing shared common-structure serialization unexpectedly;
- a new crypto dependency appears necessary for classical v2 foundation;
- the new crate cannot remain runtime-neutral.

Do not paper over ambiguity with permissive parsing.