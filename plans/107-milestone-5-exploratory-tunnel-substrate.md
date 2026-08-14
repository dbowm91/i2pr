# Plan 107: Milestone 5 exploratory tunnel substrate

## Status and authority

- Status: **planned; active Milestone 5 implementation plan**.
- Authority parent: Plan 102 (`plans/102-milestone-4-routerinfo-netdb-authority-and-roadmap.md`)
  and Plan 102 amendment (`plans/102-amendment-exploratory-tunnel-dependency.md`).
- Authority siblings: Plans 103, 104, 105, 106 — all closed locally.
- Date: 2026-08-14.
- Roadmap milestone: 5 (Network tunnel data plane and exploratory tunnels).

This plan is the first Milestone 5 implementation plan. It implements
the **substrate** required by the NetDB seam: typed tunnel identity,
a bounded exploratory tunnel pool, the wire-framing codecs the
build messages and tunnel-data messages require, and a runtime-
neutral `ReplyPathProvider` that turns the existing inbound
exploratory tunnel pool into the `ReplyPath` tokens the Plan 105
lookup state machine requires.

A complete ECIES-X25519 build-encryption implementation, a live
mixed-router tunnel build execution, transit participation, and
destination-specific tunnel pools are out of scope for Plan 107 and
will be split into subsequent Milestone 5 plans (Plan 108+).
Plan 107 is large enough on its own: it adds a new crate, a typed
substrate, and a real reply-path handoff that removes the
`BlockedExploratoryTunnelUnavailable` blocker under deterministic
test conditions.

## Executive decision

The Plan 105 NetDB lookup state machine refuses to emit a
standards-conformant `DatabaseLookup` until the runtime adapter
supplies a `ReplyPath` token (`Plan 105 §4` and the Plan 105
amendment). Today the `i2pr-daemon` `NetDbSeam` always reports
`ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable` because
no exploratory tunnel pool exists.

Plan 107 builds the smallest runtime-neutral exploratory pool and
reply-path provider that:

1. lets the seam report `Available` when at least one valid inbound
   exploratory tunnel is registered;
2. refuses to expose any tunnel build that requires ECIES-X25519
   encryption without the corresponding cryptographic primitive;
3. keeps the codec surface, pool surface, and state machine
   transport-neutral so Plan 108 can add the live build without
   rewrites.

Plan 107 deliberately does **not** activate NTCP2, advertise
tunnels, or run a live mixed-router build. The exploratory pool
is filled through an injected `TunnelRegistrar`; the production
registrar that performs real builds will be implemented in Plan
108+ and remains out of scope here.

## Why this sequence is correct

Plan 105 already exposes:

- `LookupId`, `LookupKind`, `ReplyPath`, `WaiterSet`,
  `CoalescedTargets` — typed lookup identity and the explicit
  exploratory-tunnel handoff token (`crates/i2pr-netdb/src/lookup_id.rs`);
- `LookupAction` vocabulary: `SendDatabaselookup`,
  `NeedExploratoryReplyPath`, `Complete`
  (`crates/i2pr-netdb/src/lookup_action.rs`);
- a `ReplyPathSink` trait that lets the runtime adapter accept a
  reply path for a pending lookup.

Plan 105 deliberately stopped at the **boundary** — it does not
own a tunnel pool, a build encryption primitive, or a reply-path
provider. The seam
(`crates/i2pr-daemon/src/netdb_seam.rs:69-71`) currently hard-codes
the blocked status because Milestone 5 has not landed.

Plan 107 closes the boundary by:

- introducing a new runtime-neutral crate `i2pr-tunnel` that owns
  tunnel identity, exploratory pool, build-record codec surface,
  reply-path provider, and build-cryptography seam;
- wiring the seam to consult the provider before reporting the
  exploratory path status, so a real registered inbound tunnel
  flips the status to `Available`;
- exposing tunnel IDs, lease sets, role types, and bounded pool
  replacement policy.

The substrate is large enough that splitting into a separate crate
keeps the dependency direction clean (`i2pr-tunnel` consumes
`i2pr-proto`, `i2pr-crypto`, `i2pr-core`, and `i2pr-netdb`; it
does **not** depend on `i2pr-runtime`, `i2pr-daemon`, or any
transport crate).

## Scope

In scope for Plan 107:

- new `i2pr-tunnel` runtime-neutral crate
  (`crates/i2pr-tunnel/`):
  - `TunnelId`, `TunnelRole`, `TunnelDirection`,
    `TunnelLifetime` types;
  - `ExploratoryPoolConfig` with bounded `max_inbound`,
    `max_outbound`, `length`, `lifetime_seconds`,
    `build_concurrency`, `failure_threshold`;
  - `ExploratoryPool` with deterministic replacement, expiry,
    failure accounting, and the bounded `select_inbound_reply_path`
    selector;
  - `ReplyPathProvider` trait with a default implementation that
    delegates to an injected `ExploratoryPool`;
  - typed build-record framing surface
    (`ShortBuildRecordLayout`, `VariableBuildRecordLayout`,
    `BuildRequestKind`, `BuildRecordParseError`) over the
    existing `DeferredBuildRecords` bytes from `i2pr-proto`;
  - ECIES-X25519 build encryption **seam** (typed error, key
    material wrappers, full cryptography deferred to Plan 108);
  - `TunnelDataMessage` helper that exposes layered decryption
    output as bounded plaintext bytes (full layered encryption is
    deferred to Plan 108; the framing helper is wired and
    unit-tested with deterministic AES-256-CBC round-trips
    supplied by the testkit);
  - unit + property tests covering pool selection, expiry,
    replacement, capacity, deterministic seed, and the
    reply-path round-trip;
- `i2pr-netdb` exposes the new `ReplyPathProvider` trait and a
  default adapter (`&dyn ReplyPathProvider`) so the daemon seam
  can call into the exploratory pool;
- `i2pr-daemon`:
  - `NetDbSeam::path_status` consults the injected provider;
  - `NetDbSeam::begin_lookup` accepts a `&dyn ReplyPathProvider`
    and feeds a path into the underlying `RouterInfoLookup`
    through the `ReplyPathSink`;
  - new `tunnels::ExploratoryPoolRegistrar` test-only scaffolding
    that fills the pool with deterministic tunnels (full live
    registration remains out of scope);
- updated `AGENTS.md`, `README.md`, `docs/architecture/*.md`,
  `docs/protocol-support.md`, `specs/support.toml` to record
  Milestone 5 progress and the new crate;
- updated tests + fixtures.

Out of scope for Plan 107 (will land in Plan 108+):

- live ECIES-X25519 build-encryption primitive implementation and
  consensus against pinned Java I2P / i2pd vectors;
- live mixed-router tunnel build execution;
- transit participation (Milestone 11);
- destination-specific tunnel pools (Milestone 6);
- LeaseSet publication from tunnel records;
- tunnel-data layered encryption for outbound endpoint and
  inbound gateway paths beyond a deterministic AES round-trip
  self-test;
- garlic routing (Milestone 6).

## Architecture and dependency constraints

```text
i2pr-proto -----+
 i2pr-crypto ---+----> i2pr-tunnel
 i2pr-core -----+
 i2pr-netdb ----+

i2pr-runtime ----> i2pr-daemon
```

`i2pr-tunnel` must remain runtime-neutral: no `tokio`, no
`std::net`, no `std::fs`, no sockets. It depends only on
`i2pr-proto`, `i2pr-crypto`, `i2pr-core`, and `i2pr-netdb`.
`i2pr-tunnel` does **not** depend on `i2pr-runtime` or
`i2pr-daemon`.

The daemon runtime adapter (Plan 108+) will own the actual build
orchestration tasks and the link owner that injects an inbound
tunnel registration; that adapter is not part of Plan 107.

## Protocol authority

Implement against the repository dossier first:

```text
specs/protocols/01-common-identity-crypto.md
specs/protocols/02-i2np.md
specs/protocols/05-tunnels.md
specs/SOURCES.md
specs/CONFORMANCE.md
```

Open decisions deferred from the dossier that Plan 107 resolves:

- **Build record size and shape.** Plan 107 supports both
  `SHORT_BUILD_RECORD_SIZE` (218) and `VARIABLE_BUILD_RECORD_SIZE`
  (528) layouts behind a typed enum and does not commit to either
  as the production default; the default selector remains
  Short until Plan 008 introduces the live build, at which point
  the production default will be set to `Short`.
- **Tunnel length policy.** Plan 107 keeps a single
  `length_hops: u8` (default 2, hard ceiling 8) field in the pool
  config; policy projection lives outside the codec.
- **Tunnel lifetime policy.** Plan 107 defaults to 10 minutes and
  caps at 30 minutes.
- **Inbound endpoint reply instructions.** Plan 107 does not yet
  emit a full LeaseSet2 reply instruction; the existing classic
  `Lease` shape from `i2pr-proto::Lease` is used to represent the
  exploratory inbound tunnel endpoint.

Plan 107 does not pull forward any Milestone 6, Milestone 11, or
Milestone 12 decisions.

## Security and resource policy

Plan 107 inherits and reinforces the Plan 102 invariants:

1. Every pool slot, retry counter, build queue, lease buffer, and
   tunnel-ID generator is bounded.
2. Time is injected into the exploratory pool where practical;
   `ExploratoryPool::advance_time(now)` returns deterministic
   expiry, replacement, and failure-accounting decisions.
3. Secret-bearing material (`TunnelLayerKeys`,
   `BuildEncryptionKeys`) is `Zeroize`-derived, non-`Debug`,
   non-cloneable, and never copied through `Debug`-formatting
   helpers.
4. The `ReplyPathProvider` interface returns a `ReplyPath` only
   after confirming the inbound tunnel is in the
   `Established` state and the lifetime window has not yet
   elapsed; expired or failed tunnels are filtered out.
5. The exploratory pool refuses to register more than the
   configured maximum; the registrar returns a typed
   `PoolFull` error and never silently drops the registration.
6. Build encryption surface is gated behind an injected
   `BuildCryptography` trait that returns a typed
   `BuildCryptographyUnavailable` error until Plan 008 supplies
   the live primitive; Plan 107 only validates that the seam
   rejects calls to the absent primitive.

## Test strategy

Layered tests as Plan 102 specifies:

```text
pure unit/property tests
    -> deterministic pool state-machine tests
    -> tunnel codec round-trip tests
    -> reply-path provider integration tests
    -> seam status / begin_lookup behaviour tests
    -> daemon integration test of bootstrap-with-pool
```

Plan 107 does not introduce any new CI workflow, any new Python
harness, or any new plan-number-specific test infrastructure.

## Plan 107 acceptance criteria

Plan 107 closes when all of the following are true:

1. `i2pr-tunnel` crate exists, compiles, and passes its own unit
   tests.
2. `ExploratoryPool::register_inbound(TunnelRegistration)` enforces
   the configured `max_inbound` ceiling and returns typed errors.
3. `ReplyPathProvider` from `i2pr-netdb` is implemented by an
   `ExploratoryPoolAdapter` in `i2pr-tunnel` and consumed by
   `NetDbSeam`.
4. `NetDbSeam::path_status` reports
   `ExploratoryPathStatus::Available` when the pool contains at
   least one valid inbound tunnel and
   `ExploratoryPathStatus::BlockedExploratoryTunnelUnavailable`
   otherwise.
5. `NetDbSeam::begin_lookup` consumes a `&dyn ReplyPathProvider`
   and translates a `NeedExploratoryReplyPath` action into a real
   `SendDatabaselookup` action when the provider returns a path;
   it continues to emit `Complete { final_state =
   LookupFinalState::PathUnavailable }` when the provider returns
   `None`.
6. The deterministic test suite proves pool selection is stable
   for the same seed and that expired tunnels are filtered out.
7. The cryptographic surface is a typed seam only; no live build
   is exercised, and `i2pr-tunnel::build::BuildCryptography::unavailable()`
   is called from at least one unit test.
8. `AGENTS.md`, `README.md`, `docs/architecture/i2pr-tunnel.md`,
   `docs/architecture/overview.md`, `docs/architecture/i2pr-netdb.md`,
   `docs/protocol-support.md`, and `specs/support.toml` are
   updated.
9. Workspace `cargo fmt`, `cargo check`, `cargo test`,
   `cargo clippy --all-features -- -D warnings`,
   `cargo doc --no-deps`, the runtime boundary checks, and the
   static ntcp2 interoperability checks all pass on this host.
10. The Plan 046 rootless checker continues to report the same
    pre-existing baseline failure unrelated to Plan 107.

## Handoff command

The implementation agent should begin with the **crate skeleton and
identity types only**, validate them, then move to the pool,
then to the provider, then to the seam wire-up, and finally to
documentation and tests. Close Plan 107 with the closure record
`plans/107-status.md` and the implementation-floor entries in
`specs/support.toml`.

The next executable implementation after Plan 107 is Plan 108
(ECIES-X25519 build-encryption primitive and live tunnel build).
