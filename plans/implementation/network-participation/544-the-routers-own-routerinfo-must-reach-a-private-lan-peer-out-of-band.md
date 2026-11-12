# Plan 544 — the router's own RouterInfo must reach a private-LAN peer out of band

**Status:** registered. **Subsystem:** network-participation.
**Registered:** 2026-11-12. **Successor to:** Plan 543.

## 1. Objective

Two `i2pr` routers on an isolated private LAN must be able to find each other,
so that the two-host SSU2 qualification topology Plans 431/449/472/476/493 have
been blocked on can actually be exercised.

A router publishes its RouterInfo to the NetDB, and its peers learn it from a
floodfill. An **isolated** network has no floodfill. The SSU2 publication path
is additionally loopback-clamped by design
(`crates/i2pr-runtime/src/ssu2_runtime.rs:3671`), so on a private-LAN bind it
refuses with `InvalidPublication` — observed in production on the arm64 lab
node, not inferred.

The only remaining route is the one every I2P lab handoff has always used: the
operator copies the record across. That requires the router to **produce** the
record in the form the peer's cache loader will accept. It does not.

This plan makes the router write its own signed RouterInfo to
`<data_dir>/export/<router-hash>`, where the filename is the lowercase
hexadecimal router hash — the exact name the peer's cache loader derives its
expected key from. The operator's step becomes a copy, never a decode.

## 2. Why ready

**Every piece exists.** The router already encodes its own record for the
inspection snapshot (`crates/i2pr-daemon/src/lib.rs`, beside
`publish_local_router_info_b64`), the crate already derives the hash
(`i2pr_netdb::router_hash`, `crates/i2pr-netdb/src/router_info.rs:80`), and the
peer's loader already accepts a hash-named cache entry
(`crates/i2pr-netdb-persist/src/cache_loader.rs:272`). What is missing is the
file.

**The topology is real and was verified.** Two hosts on `192.168.182.0/24`
(`.143` x86_64 and `.20` aarch64), bidirectional UDP proven on the lab ports,
and the arm64 node running `i2pr 0.1.0` with both transports bound on its
RFC1918 address.

**The end-to-end path was proven on loopback before this plan was registered.**
Two `i2pr` instances, one exporting and the other seeded from that file,
produced `record_count=1` on the peer and then
`authenticated RouterInfo store applied to bootstrap peer store
transport_family="ssu2" inserted=true` — an authenticated SSU2 session carrying
I2NP between two `i2pr` routers. This plan formalises that, with the evidence
the repository requires.

## 3. Current implementation evidence

### 3.1 The router never writes its own record

The publication site encodes the record and hands it to the inspection seam and
the watch channel:

```text
// crates/i2pr-daemon/src/lib.rs
match i2pr_netdb::encode_router_info(info.as_ref()) {
    Ok(encoded) => match inspection
        .publish_local_router_info_b64(encoded.trim_end_matches('='))
```

Nothing writes it to disk. On the arm64 lab node the data dir after a run
held only `router.identity`, `ssu2.transport.identity` and
`ntcp2.transport.identity`.

### 3.2 The peer's loader requires the hash as the filename

```text
// crates/i2pr-netdb-persist/src/cache_loader.rs:272
let expected_key = router_info_name_hash(&name);
let validated =
    match ValidatedRouterInfo::from_router_info(info, expected_key, validation_context) {
```

A record under any other name is refused as a key mismatch and counted
`invalid`. The name is not advisory; it is the expected key.

### 3.3 `netdb/routers/` is the router's own cache

The loader reads `<data_dir>/netdb/routers/`
(`crates/i2pr-storage/src/lib.rs:829`) and inserts what it finds into the
router's **own** store. A router that found itself there would hold its own
record as a peer, so the export must not go there.

### 3.4 SSU2 publication is loopback-clamped

```text
// crates/i2pr-runtime/src/ssu2_runtime.rs:3671
if !bound.ip().is_loopback() {
    return Err(Ssu2PublicationUnavailable::InvalidPublication);
}
```

and it additionally requires `ReachabilityState::Reachable` (`:3662`), which
the same function's comment records that static loopback-homogeneous traffic
cannot reach. So `ssu2.advertise` is inert in every posture, and the private-LAN
node logs `the re-dated RouterInfo was not installed reason=InvalidPublication`.

### 3.5 `publish_router_hash` has no production caller

`crates/i2pr-daemon/src/i2pcontrol_inspection.rs:450` implements a bounded,
validated router-hash publication. Its only callers are its own unit tests.
This is the same "mechanism present, no caller" shape Plans 542 and 543 closed;
this plan does not use it, because it sidesteps the RPC's frozen selector
inventory, but the dead surface is recorded as a finding.

### 3.6 What the plan does not claim

- It does not claim the LAN run reaches T4/T5. It claims the routers can find
  each other and establish an authenticated session; the correlated two-way
  reply and the peer-initiated session are the next tiers.
- It does not claim anything about stock routers. i2pr-to-i2pr is one topology;
  i2pd and Java I2P admission is Plan 493's question.
- It does not claim publication to a public NetDB.

## 4. Invariants

1. **The export is the router's own record, unmodified.** The bytes written are
   the bytes the router signs and publishes; the plan adds no transformation.
2. **The filename is the record's hash.** A peer's loader derives its expected
   key from the name, so a wrong name is a silent rejection.
3. **The export never enters the router's own store.** It is written to
   `export/`, not `netdb/routers/`.
4. **The export is bounded and permissioned.** The directory is `0700` and the
   file `0600`, matching the data dir's own posture.
5. **A record whose hash cannot be derived is skipped, counted and logged**,
   never written under a guessed name.
6. **No new surface is reachable.** No listener, no socket, no configuration
   key, no dependency.
7. **No advertisement.** `router.version` stays `0.9.62` and the inventory's
   advertised count stays **zero**.
8. **The predecessors' lanes and guards stay byte-identical.**

## 5. Scope

### 5.1 In scope

- `crates/i2pr-daemon/src/lib.rs` — the export helper and its call site.

### 5.2 Out of scope

- The SSU2 publication clamp and its reachability gate.
- The I2PControl selector inventory.
- The cache loader's validation rules.
- The peer pool's selection.
- Stock-router admission.

### 5.3 Out of scope, with the owner named

- **`ssu2.advertise` is inert in every posture.**
  (`crates/i2pr-runtime/src/ssu2_runtime.rs:3671`) **Owner:** the plan that
  gives the runtime a lab-side reachability class, if one is ever wanted.
- **`publish_router_hash` has no production caller.**
  (`crates/i2pr-daemon/src/i2pcontrol_inspection.rs:450`) **Owner:** a
  successor that either retires it or wires it.
- **The `[floodfill]` gate on the dual-transport record.**
  (`crates/i2pr-daemon/src/lib.rs:1002`) The two-transport record is gated on
  `config.floodfill.qualification_profile`, so a lab operator must set the
  **floodfill** key to get an SSU2 endpoint into the record at all. **Owner:**
  a successor that documents or decouples it.
- **T4/T5 and stock-router admission** — Plan 493's topology and the SSU2
  external lane.

## 6. Required production changes

### 6.1 `crates/i2pr-daemon/src/lib.rs`

1. `fn export_local_router_info(data_dir: &Path, encoded: &[u8]) -> io::Result<Option<(String, PathBuf)>>`
   — decodes the record, derives the hash, writes
   `<data_dir>/export/<hex>` with `0700`/`0600`, and returns the hash and path.
2. The call site, beside the existing inspection publication, logging the hash
   and the path.

## 7. Design decisions

### 7.1 A file, not an RPC selector

`i2p.router.info` already returns the record, so the hash is the only missing
piece. Adding a `router.hash` selector would touch the frozen RouterInfo
selector inventory (`crates/i2pr-i2pcontrol/src/conformance.rs`), which is
conformance evidence. A file needs no wire change at all.

### 7.2 The hash is the filename because the peer demands it

The alternative — write a readable name and let the operator rename — moves a
mechanical certainty into a human step that fails silently, because a wrong name
is a `KeyMismatch` on the peer with no way to tell it from an invalid record.

### 7.3 `export/`, not `netdb/routers/`

The latter is the router's own load path. Writing there would make the router
hold its own record as a peer on the next start, which is a correctness defect
this plan must not introduce to fix a convenience one.

### 7.4 The helper returns `Option`

A record whose hash cannot be derived is a local condition. Returning an error
would make the publication path fail for a record the router has already
signed; returning `Ok(None)` keeps the failure commensurate with its cause.

## 8. Work packages

1. **WP1 — the helper.** Decode, hash, write, permission.
2. **WP2 — the call site.** Beside the inspection publication.
3. **WP3 — the lane.** The export's name is its hash, and it round-trips.
4. **WP4 — the guard.** The `export/`/`netdb/routers/` separation.
5. **WP5 — the negative controls.**
6. **WP6 — the documentation and the inventory.**
7. **WP7 — the two-host LAN run.**
8. **WP8 — the floor and the closure record.**

## 9. Failure, cancellation, restart and contention semantics

**Failure.** A write that fails is logged and does not fail the publication.
The record is public material, so a failed write leaks nothing.

**Cancellation.** The helper is synchronous and one-shot per publication.

**Restart.** The export is rewritten on each publication, so it always matches
the current record.

**Contention.** None: the directory is written once per publication.

## 10. Compatibility and migration

**No wire change.** **No configuration key.** **No persisted state** beyond the
export file, which is derived. **No manifest change.**

## 11. Required tests

### 11.1 The lane

The export's **name equals the hash derived from its own contents**, and the
written record round-trips through the cache loader's validation with that
name — the property the peer depends on.

### 11.2 The guard

The export is written to `export/` and never to `netdb/routers/`; the filename
is the hash; the file is `0600` in a `0700` directory; no listener, socket,
configuration key or dependency is added; nothing is advertised; the
predecessors' evidence is byte-identical.

## 12. Exact verification commands

```text
cargo fmt --all --check
cargo check --locked --workspace --all-targets
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
cargo test --locked -p i2pr-daemon --test router_info_export -- --test-threads=1
cargo test --locked -p i2pr-daemon --test router_info_export_boundary -- --test-threads=1
```

plus the routine floor, with this plan's two steps added.

## 13. Documentation updates

`specs/support.toml` (surface `netdb.router-info-export`), `specs/CONFORMANCE.md`,
`specs/QUALIFICATION-GATES.md`, `docs/architecture/i2pr-daemon.md`, and this
plan's amendment log.

## 14. Acceptance criteria and stop conditions

Accepted when the export's name is its own hash, the record round-trips through
the loader, the export never enters `netdb/routers/`, no key or dependency is
added, nothing is advertised, and the two-host run establishes an authenticated
session.

**Stops** if the fix would require weakening the SSU2 publication clamp, adding
a configuration key, changing a predecessor's guard, or claiming a tier the run
did not reach.

## 15. Closure evidence required

Commits; the floor table; lane and guard rows; negative controls with the
restoration proof; the requirement-to-evidence matrix; the invariant, failure,
migration and security reviews; limitations, findings, roadmap disposition and
the unblock audit.

## 16. Handoff notes

- Experimental, default-off, non-advertised. The export is public material; it
  carries no secret.
- Do not move the export into `netdb/routers/`.
- Do not rename the file. A wrong name is a silent `KeyMismatch`.
- The LAN run needs `[floodfill].qualification_profile = "controlled_loopback"`
  to get a two-transport record at all.
- The predecessors' closure records are **not** rewritten.
- A successor must not treat this as publication: it is a lab handoff, and the
  router still does not publish to a NetDB it cannot reach.

## 17. Amendment log

### 17.1 — 2026-11-12, registration

Plan 544 is registered after the end-to-end path was proven by hand on loopback
and the arm64 lab node was brought up. The scope is the **export only**: the
SSU2 clamp, the selector inventory, the `[floodfill]` coupling and T4/T5 are
named with their owners and not attempted here.
