# Dependency Graph — Detail

The crate-layer dependency direction is enforced by
[`scripts/check-dependency-direction.sh`](../../scripts/check-dependency-direction.sh),
which is the single source of truth. The `expected` dictionary embedded in
that script maps each crate name to its **allowed** set of direct workspace
(`i2pr-*`) production dependencies. This document is the human-readable
mirror of that dictionary.

The checker is **allowlist-subtractive**: it fails only when a crate
declares an edge that is *not* in its allowed set
(`unexpected = direct - allowed`). An allowlisted edge that no crate
actually uses is **not** an error. That asymmetry is why the
actual-vs-allowed notes below matter to a reviewer.

Scope of enforcement:

- The checker's main loop iterates the keys of `expected`, which on its own
  would make a member with **no** entry invisible -- deleting an entry would
  make that crate's forbidden edges unreported rather than reported.
  `scripts/check-console-boundaries.sh` rule 7 caught that set, but Plan 369
  WP5 negative mutation **N11** proved the gap was real (removing the
  `i2pr-app-fixture` entry still printed `dependency direction: ok`), so the
  dependency script now **also fails closed on any unmapped `i2pr-*` member**.
  The console rule is retained as a redundant check.
- The coverage gap recorded in `AGENTS.md` (missing `i2pr-tunnel` and
  `tools/i2pr-interop` keys) was **closed** when the console was added;
  both now have explicit allowlists.

## Allowlist

Production edges only (reads as "may depend on"). `cargo metadata` is the
authority for what is *actual*; `expected` is the authority for what is
*allowed*. Dev-dependencies are excluded from the production graph — the
checker filters `kind in (None, "normal")` — and are listed separately in
[Dev-dependency edges](#dev-dependency-edges).

| Crate | Allowed production `i2pr-*` deps (checker) | Actual | External production deps |
| --- | --- | --- | --- |
| `i2pr-proto` | (none) | none | `flate2`, `sha2`, `zeroize` |
| `i2pr-core` | (none) | none | (none) |
| `i2pr-su3` | (none) | none | `sad-rsa`, `sha2`, `thiserror`, `x509-parser` |
| `i2pr-i2pcontrol` (Plan 286) | (none) | none | `serde`, `serde_json`, `thiserror` |
| `i2pr-app-proto` (Plan 345) | (none) | none | `serde`, `serde_json`, `thiserror` |
| `i2pr-app-manager-proto` (Plan 368) | `i2pr-app-proto` | same | `serde`, `serde_json`, `thiserror` |
| `i2pr-app-package` (Plan 373) | `i2pr-app-proto` | same | `ed25519-dalek`, `serde`, `serde_json`, `sha2`, `thiserror`, `zip` |
| `i2pr-app-state` (Plan 374) | `i2pr-app-package`, `i2pr-app-proto` | same | `serde`, `serde_json`, `thiserror` |
| `i2pr-appctl` (Plan 374) | `i2pr-app-package`, `i2pr-app-proto`, `i2pr-app-state` | same | `clap` |
| `i2pr-appd` (Plans 369/374) | `i2pr-app-manager-proto`, `i2pr-app-proto`, `i2pr-app-state` | same | `rand_core`, `thiserror`, `tokio` |
| `i2pr-apphost` (Plan 369) | `i2pr-app-manager-proto`, `i2pr-app-proto` | same | `thiserror`, `tokio`, `tokio-util` |
| `i2pr-app-fixture` (Plan 369 WP5, **evidence tooling**) | `i2pr-app-manager-proto`, `i2pr-app-proto`, `i2pr-appd` | same | `serde_json`, `thiserror`, `tokio` |
| `i2pr-crypto` | `i2pr-proto` | same | `chacha20`, `chacha20poly1305`, `curve25519-dalek` (Plan 330), `ed25519-dalek`, `elligator2` (Plan 131; replaces the retired `curve25519-elligator2 0.1.0-alpha.2`), `hmac`, `rand_core`, `sha2`, `subtle`, `thiserror`, `x25519-dalek`, `zeroize` |
| `i2pr-addressbook` (Plan 294) | `i2pr-proto` | same | `base64ct`, `serde`, `serde_json`, `thiserror` |
| `i2pr-storage` | `i2pr-crypto` | same | `rand_core`, `thiserror`, `zeroize` |
| `i2pr-netdb` | `i2pr-crypto`, `i2pr-proto`, `i2pr-su3` | same | `base64ct`, `flate2`, `sha2`, `thiserror`, `zip` |
| `i2pr-transport` | `i2pr-core`, `i2pr-proto` | same | (none) |
| `i2pr-netdb-persist` (Plan 104) | `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto`, `i2pr-storage` | same | `sha2`, `thiserror` |
| `i2pr-transport-ntcp2` | `i2pr-crypto`, `i2pr-proto`, `i2pr-transport` | same | `aes`, `chacha20poly1305`, `hmac`, `sha2`, `siphasher`, `thiserror`, `zeroize` |
| `i2pr-transport-ssu2` (Plans 155–158) | `i2pr-crypto`, `i2pr-proto`, `i2pr-transport` | same | `chacha20`, `chacha20poly1305`, `hmac`, `rand_core`, `sha2`, `thiserror`, `zeroize` |
| `i2pr-tunnel` | `i2pr-core`, `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto` | same | `aes`, `cbc`, `chacha20`, `chacha20poly1305`, `rand_core`, `sha2`, `thiserror`, `x25519-dalek`, `zeroize` |
| `i2pr-client` (Plans 120/121) | `i2pr-core`, `i2pr-crypto`, `i2pr-netdb`, `i2pr-proto`, `i2pr-tunnel` | same | `rand_chacha`, `rand_core`, `thiserror`, `x25519-dalek`, `zeroize` |
| `i2pr-api` (Plan 136; extended Plan 164+) | `i2pr-client`, `i2pr-crypto`, `i2pr-proto`, `i2pr-tunnel` | same | `rand_core`, `thiserror`, `zeroize` |
| `i2pr-runtime` | `i2pr-core`, `i2pr-crypto`, `i2pr-proto`, `i2pr-transport`, `i2pr-transport-ntcp2`, `i2pr-transport-ssu2` | same | `futures-util`, `rand_core`, `tokio`, `tokio-util`, `tracing`, `zeroize` |
| `i2pr-service-tunnels` (Plans 174/175; portability Plans 349–351) | `i2pr-client` | none — **`i2pr-client` is allowed-but-unused** | `base64ct`, `sha2`, `subtle`, `thiserror`, `zeroize` |
| `i2pr-console` (Plans 356–358) | (none) | none | `argon2`, `axum` (built without `tokio`), `rand_core`, `serde`, `serde_json`, `toml`, `zeroize` |
| `i2pr-daemon` | 18 crates — enumerated in full in the [next section](#i2pr-daemon-composition-root) | same 18 | `chacha20poly1305`, `clap`, `eggserve-server`, `flate2`, `quick-xml`, `rand_chacha`, `rand_core`, `rcgen` (Plan 287), `rustix`, `rustls`, `rustls-pki-types`, `serde`, `serde_json`, `subtle`, `thiserror`, `tokio`, `tokio-rustls`, `toml`, `tracing`, `tracing-subscriber`, `webpki-roots`, `x509-parser`, `zeroize` (`rustls-pemfile` was removed before Plan 287 closure for RUSTSEC-2025-0134) |
| `i2pr-testkit` (test-only) | `i2pr-core`, `i2pr-crypto`, `i2pr-proto`, `i2pr-runtime`, `i2pr-transport`, `i2pr-transport-ntcp2` | same | `rand_chacha`, `rand_core`, `sha2`, `tokio` |

Notes on the allowlisted-but-unused and legacy rows:

- `i2pr-service-tunnels` may depend on `i2pr-client` but has no internal
  production dependencies after Plan 350's full-tree audit.
  ADR 0033 and `specs/references/portable-service-tunnel-core-v1.md`
  freeze this crate as the reusable policy/filter owner; adapters retain
  socket, runtime, persistence, and transport lifecycle ownership. Plan 350
  completed the full dependency-use proof and removed the unused path edge.
  The `i2pr-client` edge is explicitly allowed for future
  destination/Streaming reuse. Plan 341 added `zeroize` for the
  runtime-neutral outbound-secret policy and deliberately did **not** add
  an AEAD here: the crate may not depend on `i2pr-crypto`, so the
  concrete `OutboundSecretStore` implementation (ChaCha20-Poly1305,
  keyed by HKDF over the router signing seed) lives in `i2pr-daemon`, and
  this crate holds only the trait, the stored-form framing, and the
  fail-closed default.
- `i2pr-testkit` is the only non-production library crate in `crates/`.
  It is allowed to reach `i2pr-runtime` and the transport crates, and it
  does. It is **not** allowed to reach `i2pr-transport-ssu2`, and does
  not. See [`i2pr-testkit`](#i2pr-testkit) for the inbound rule.

### `i2pr-daemon` composition root

`i2pr-daemon` is the CLI/config/composition root and is the only crate
allowed to see 18 workspace dependencies at once:

1. `i2pr-addressbook`
2. `i2pr-api`
3. `i2pr-app-manager-proto`
4. `i2pr-app-proto`
5. `i2pr-client`
6. `i2pr-console`
7. `i2pr-core`
8. `i2pr-crypto`
9. `i2pr-i2pcontrol`
10. `i2pr-netdb`
11. `i2pr-netdb-persist`
12. `i2pr-proto`
13. `i2pr-runtime`
14. `i2pr-service-tunnels`
15. `i2pr-storage`
16. `i2pr-su3`
17. `i2pr-transport`
18. `i2pr-tunnel`

All 18 are actually declared, and the set is not transitive: e.g. the
daemon reaches `i2pr-transport-ssu2` only through `i2pr-runtime`, never
directly.

## Zero-dependency and leaf crates

Six crates have **no** workspace production dependencies, and the
checker pins each of them to the empty set:

- `i2pr-proto` — the foundational namespace and error categories; it may
  not depend on any `i2pr-*` crate, so it can never cycle.
- `i2pr-core` — zero deps of any kind (no `i2pr-*` and no external
  crates).
- `i2pr-su3` — bounded, runtime-neutral SU3 framing and signature
  verification. Leaf.
- `i2pr-i2pcontrol` — runtime-neutral Proposal 170 control wire/domain
  contract. Leaf; only `i2pr-daemon` may depend on it.
- `i2pr-app-proto` — runtime-neutral managed-app contract (Plan 345).
  Leaf.
- `i2pr-app-manager-proto` — private trusted AppManager protocol
  contract (Plan 368). Depends only on `i2pr-app-proto`; only
  `i2pr-daemon` may depend on it.
- `i2pr-console` — the loopback router console substrate (Plans
  356–358). Leaf *and* socketless: it may not depend on any `i2pr-*`
  crate, so the console cannot reach router state directly. Every value
  it renders arrives through the `ControlClient` trait, and the only
  implementation of that trait lives in `i2pr-daemon`.

`i2pr-transport` is the smallest non-leaf crate: its only production
dependencies are `i2pr-core` and `i2pr-proto`, and it has **no external
production dependencies at all**.

## Dev-dependency edges

Two `i2pr-*` edges exist **only** as `[dev-dependencies]`. Neither
appears in the production graph above, and the checker cannot see them
because it filters `kind in (None, "normal")`:

| Crate | Dev-only edge | Declared at |
| --- | --- | --- |
| `i2pr-proto` | → `i2pr-crypto` | `crates/i2pr-proto/Cargo.toml` `[dev-dependencies]` |
| `i2pr-addressbook` | → `i2pcontrol` (`i2pr-i2pcontrol`) | `crates/i2pr-addressbook/Cargo.toml` `[dev-dependencies]` |

The `i2pr-proto` dev edge is a deliberate, acyclic test-time direction:
`i2pr-proto` still has **no** production `i2pr-*` edge, so the cycle ban
on the production graph is intact. The `i2pr-addressbook` dev edge is
Plan 294 test interop with the control contract and does not widen the
runtime-neutral canonical-naming production surface.

## ASCII dependency graph

Every arrow below is a real production edge, verified against
`cargo metadata --no-deps --format-version 1` (filtered to `kind in
(None, "normal")` and `i2pr-*` names). Nodes are grouped into tiers by
depth; a tier assignment is a layout convenience, not an additional
enforced rule.

```text
Tier 0 — zero workspace production dependencies
  i2pr-proto          i2pr-core          i2pr-su3          i2pr-i2pcontrol
  i2pr-app-proto      i2pr-console

Tier 1
  i2pr-crypto         --> i2pr-proto
  i2pr-addressbook    --> i2pr-proto

Tier 2
  i2pr-storage        --> i2pr-crypto
  i2pr-netdb          --> i2pr-crypto, i2pr-proto, i2pr-su3

Tier 3
  i2pr-transport          --> i2pr-core, i2pr-proto
  i2pr-netdb-persist      --> i2pr-crypto, i2pr-netdb, i2pr-proto, i2pr-storage
  i2pr-transport-ntcp2    --> i2pr-crypto, i2pr-proto, i2pr-transport
  i2pr-transport-ssu2     --> i2pr-crypto, i2pr-proto, i2pr-transport

Tier 4
  i2pr-tunnel     --> i2pr-core, i2pr-crypto, i2pr-netdb, i2pr-proto
  i2pr-client     --> i2pr-core, i2pr-crypto, i2pr-netdb, i2pr-proto,
                      i2pr-tunnel

Tier 5
  i2pr-api              --> i2pr-client, i2pr-crypto, i2pr-proto, i2pr-tunnel
  i2pr-runtime          --> i2pr-core, i2pr-crypto, i2pr-proto,
                           i2pr-transport, i2pr-transport-ntcp2,
                           i2pr-transport-ssu2
  i2pr-service-tunnels  (no internal crate dependencies)

Tier 6 — composition root
i2pr-daemon    --> i2pr-addressbook, i2pr-api, i2pr-app-manager-proto,
                     i2pr-app-proto, i2pr-client,
                     i2pr-console, i2pr-core, i2pr-crypto, i2pr-i2pcontrol,
                     i2pr-netdb, i2pr-netdb-persist, i2pr-proto, i2pr-runtime,
                     i2pr-service-tunnels, i2pr-storage, i2pr-su3,
                     i2pr-transport, i2pr-tunnel
```

The same edges as maximal chains. Each `<--` is a single real production
edge, and an edge is listed once per chain, so a missing edge cannot be
hidden inside a branching drawing:

```text
  i2pr-proto  <-- i2pr-crypto  <-- i2pr-storage  <-- i2pr-netdb-persist
  i2pr-crypto <-- i2pr-netdb   <-- i2pr-netdb-persist

  i2pr-proto  <-- i2pr-tunnel
  i2pr-core   <-- i2pr-tunnel
  i2pr-crypto <-- i2pr-netdb  <-- i2pr-tunnel
  i2pr-tunnel <-- i2pr-client  <-- i2pr-api
  i2pr-crypto <-- i2pr-client
  i2pr-core   <-- i2pr-client
  i2pr-proto  <-- i2pr-client

  i2pr-core   <-- i2pr-transport
  i2pr-proto  <-- i2pr-transport
  i2pr-transport <-- i2pr-transport-ntcp2  <-- i2pr-runtime
  i2pr-transport <-- i2pr-transport-ssu2   <-- i2pr-runtime
  i2pr-crypto <-- i2pr-runtime
  i2pr-proto  <-- i2pr-runtime
  i2pr-core   <-- i2pr-runtime

  i2pr-crypto <-- i2pr-api
  i2pr-proto  <-- i2pr-api

  i2pr-console <-- i2pr-daemon

  i2pr-runtime <-- i2pr-daemon
  i2pr-netdb   <-- i2pr-daemon
```

The managed-app trust zone is a **separate island** from router core. The
package/state crates hold no process authority; appd alone consumes validated
decisions and creates launch authority. The manager and apphost are related by
a process rather than a crate edge.

```text
  i2pr-app-proto           <-- i2pr-appd          <-- i2pr-app-fixture
  i2pr-app-proto           <-- i2pr-apphost            (evidence tooling)
  i2pr-app-proto           <-- i2pr-app-package <-- i2pr-app-state <-- i2pr-appctl
  i2pr-app-package         <-- i2pr-app-state  <-- i2pr-appd
  i2pr-app-manager-proto   <-- i2pr-appd
  i2pr-app-manager-proto   <-- i2pr-apphost
  i2pr-app-manager-proto   <-- i2pr-app-fixture

  i2pr-app-proto           <-- i2pr-daemon
  i2pr-app-manager-proto   <-- i2pr-daemon
```

There is deliberately **no** `i2pr-apphost -> i2pr-appd` edge and none may be
added. The manager starts the apphost as a child process over an inherited
anonymous pipe; if the apphost could link the manager, the component that
execs applications could also link the component that authorises them, and the
trust zone would be a naming convention rather than a build constraint.

`i2pr-transport-ntcp2` and `i2pr-transport-ssu2` are siblings that share
`i2pr-transport`; there is deliberately **no** edge between them, and
`check-runtime-boundaries.sh` forbids one. That is why the chains above
run each of them separately instead of drawing a branch.

Arrows point from a crate to the crate it may depend on, so these chains
read right-to-left. Note that `i2pr-netdb-persist` consumes `i2pr-storage`
and `i2pr-netdb`; an earlier revision of this document drew that
direction backwards, and it was corrected against
`crates/i2pr-netdb-persist/Cargo.toml` in the 2026-08-27 architecture
audit.

## Reverse edges (i.e. "may NOT depend on")

These restate the allowlist above as prohibitions, with the reasoning
that makes each one non-obvious.

- `i2pr-proto` may not depend on any `i2pr-*` crate.
- `i2pr-crypto` may not depend on `i2pr-storage` (or above).
- `i2pr-core` may not depend on anything `i2pr-*`.
- `i2pr-netdb` may not depend on `i2pr-storage`, `i2pr-transport`,
  `i2pr-transport-ntcp2`, `i2pr-runtime`, `i2pr-daemon`, or
  `i2pr-testkit` (Plan 103; the cache seam goes in `i2pr-storage` and
  composition goes in `i2pr-netdb-persist`).
- `i2pr-netdb` may not depend on `i2pr-client`: the LeaseSet2 validation
  path is consumed by the client, and the client does not flow back into
  NetDB.
- `i2pr-tunnel` may not depend on `i2pr-client` (Plan 120: composition
  flows from `i2pr-tunnel` upward only; the client reuses
  `BoundedTunnelPool` but does not live inside `i2pr-tunnel`).
- `i2pr-client` may not depend on `i2pr-daemon`; the daemon is the
  composition root, not a client library.
- `i2pr-api` may not depend on `i2pr-daemon`, `i2pr-runtime`,
  `i2pr-netdb`, `i2pr-storage`, or `i2pr-testkit` (Plan 136; it sits
  between `i2pr-client` and `i2pr-daemon`). Its `i2pr-tunnel` edge is
  explicitly allowed (Plan 164+ data-plane composition).
- `i2pr-netdb-persist` may not depend on `i2pr-transport`,
  `i2pr-transport-ntcp2`, `i2pr-runtime`, `i2pr-daemon`, or
  `i2pr-testkit` (Plan 104).
- `i2pr-transport` may not depend on `i2pr-transport-ntcp2`,
  `i2pr-runtime`, `i2pr-daemon`, `i2pr-testkit`, `i2pr-netdb`,
  `i2pr-tunnel`, or `i2pr-client`.
- `i2pr-transport-ntcp2` may not depend on `i2pr-runtime`,
  `i2pr-daemon`, or `i2pr-testkit`.
- `i2pr-transport-ssu2` may not depend on `i2pr-runtime`, `i2pr-daemon`,
  `i2pr-testkit`, `i2pr-transport-ntcp2`, `i2pr-netdb`, `i2pr-tunnel`,
  or `i2pr-client` (Plans 155–157; runtime-neutral protocol /
  establishment / data phase only). Its approved `i2pr-crypto` edge
  (Plan 156) reuses checked X25519/HKDF/signature verification; the
  transcript policy stays local.
- `i2pr-runtime` may not depend on `i2pr-daemon`. Its direct
  `i2pr-transport-ntcp2` edge is the approved Plan 042 runtime
  composition boundary; its `i2pr-transport-ssu2` + `i2pr-crypto` +
  `i2pr-proto` edges are the approved Plan 158 UDP-ownership boundary
  (sockets, randomness/time, admission, scheduler, manager promotion).
- `i2pr-i2pcontrol` may not depend on `i2pr-daemon`, `i2pr-runtime`,
  `i2pr-testkit`, `i2pr-netdb`, `i2pr-client`, `i2pr-service-tunnels`,
  `i2pr-tunnel`, or any `i2pr-transport-*` crate (Plan 286;
  runtime-neutral contract only). Conversely, no router core crate may
  depend on `i2pr-i2pcontrol`; only `i2pr-daemon` adapts the contract to
  router state.
- `i2pr-addressbook` may depend only on `i2pr-proto` (Plan 294;
  structural destination validation only). Conversely, no router core
  crate may depend on `i2pr-addressbook`; only `i2pr-daemon` composes the
  owner into SAM, service tunnels, and control consumers, and
  `i2pr-storage` persists opaque generations only.
- `i2pr-console` may not depend on any `i2pr-*` crate (Plans 356–358;
  the console substrate is socketless and state-free). It also may not
  depend on `i2pr-runtime` or `tokio`: `axum` is built with
  `default-features = false` and **without** the `tokio` feature, so the
  crate compiles as a synchronous, runtime-neutral router. Only
  `i2pr-daemon` may depend on it, and the daemon — not the console —
  owns the listener, the `ControlClient` implementation, and the control
  principal.
- `i2pr-appd` may not depend on `i2pr-daemon`, `i2pr-runtime`, or any router
  crate (Plans 369/374). Its only production edges are the two wire contracts
  and `i2pr-app-state`; only appd crosses a validated local policy decision to
  launch authority.
- `i2pr-app-package`, `i2pr-app-state`, and `i2pr-appctl` may not depend on
  daemon/runtime/router protocol owners. Neither package nor policy code can
  construct `LaunchAuthority`.
- `i2pr-apphost` may not depend on `i2pr-appd`, `i2pr-daemon`, or `i2pr-runtime`.
  The manager and the apphost are related by a **process**, not a crate edge;
  naming `i2pr-appd` here would invert the trust zone by letting the component
  that execs applications link the component that authorises them.
- `i2pr-appd` and `i2pr-apphost` may not depend on `i2pr-app-fixture`, and **no
  router crate may name the fixture at all** (Plan 369 WP5; it is evidence
  tooling). `i2pr-app-fixture -> i2pr-appd` is the one allowed direction: the
  fixture manager has to run the real manager to qualify it.
- **No production crate may depend on `i2pr-testkit`.** This rule is
  *not* in `check-dependency-direction.sh`; see
  [`i2pr-testkit`](#i2pr-testkit).

## Coverage gaps

`check-dependency-direction.sh` now has one `expected` key per workspace
member: 25 keys for 25 members (24 crates under `crates/` plus
`tools/i2pr-interop`). There is no longer a crate whose production edges
escape comparison.

The gap this document used to describe — `i2pr-tunnel` had no
`expected` key, so its inbound rules were prose-only — was **closed** as
part of Plan 356. `"i2pr-tunnel": {"i2pr-core", "i2pr-crypto",
"i2pr-netdb", "i2pr-proto"}` and `"i2pr-interop": {"i2pr-crypto",
"i2pr-proto", "i2pr-runtime", "i2pr-storage", "i2pr-transport",
"i2pr-transport-ntcp2"}` now pin both crates to their real edges. The fix
went into the script rather than into the prose: nothing about the edges
changed, only their enforcement.

One factual correction to the rules above: `i2pr-tunnel` reverse edges
**are** machine-enforced as of that change. The statement that they are
not has been removed rather than left to rot.

### The `tokio` manifest rule does not match the manifests it scans

`scripts/check-runtime-boundaries.sh` scans every `crates/*/Cargo.toml`
and rejects `^(tokio|tokio-util)[[:space:]]*=`. Every manifest in the
workspace declares Tokio in the inherited form `tokio.workspace = true`,
which that anchored pattern does not match, so the rule currently never
fires. This is recorded as an observation about the enforcement script,
not a change to it. In practice `i2pr-daemon` and `tools/i2pr-interop`
also declare `tokio`, so the intended policy is really "only
`i2pr-runtime` and `i2pr-testkit` own async runtime state", not "only
these two manifests may name the crate".

## Non-production members

### `i2pr-testkit`

- **Position in the DAG:** a test-only fixture crate that depends
  *outward* on `i2pr-core`, `i2pr-crypto`, `i2pr-proto`, `i2pr-runtime`,
  `i2pr-transport`, and `i2pr-transport-ntcp2`. It is the only crate
  permitted to depend on `i2pr-runtime` other than `i2pr-daemon`.
- **Inbound rule:** no production crate may depend on it. This is
  enforced by `scripts/check-runtime-boundaries.sh`, lines **38–42**, not
  by `check-dependency-direction.sh`:

  ```bash
  testkit_dependents=$(grep -En 'i2pr-testkit' "$root/crates"/*/Cargo.toml || true)
  if printf '%s\n' "$testkit_dependents" | grep -Ev 'crates/i2pr-testkit/Cargo.toml' | grep -Eq .; then
    echo "production crate depends on i2pr-testkit" >&2
    exit 1
  fi
  ```

  The glob `"$root/crates"/*/Cargo.toml` covers all 21 crate manifests,
  and `grep -En` scans whole files without regard to TOML section, so the
  rule catches the string in `[dependencies]`, `[dev-dependencies]`, and
  `[build-dependencies]` alike. It is a text-level check, so a renamed
  or aliased dependency would evade it, and it does **not** cover
  `tools/`.
- Currently `crates/i2pr-testkit/Cargo.toml` is the only manifest that
  mentions `i2pr-testkit` at all.

### `tools/i2pr-interop` (package `i2pr-interop`)

- **Position:** a non-production, `publish = false` test launcher that
  lives outside `crates/`. It is a workspace member, so it appears in
  `cargo metadata`, and it declares the Tokio runtime directly.
- **Checker-policed: yes, for production edges.** `i2pr-interop` gained
  an `expected` key in the same Plan 356 change that closed the
  `i2pr-tunnel` gap, so a new `i2pr-*` production edge in this tools
  crate now fails `check-dependency-direction.sh`. The *dev-dependency*
  rule (`i2pr-testkit` may not be reached from production) is still
  text-scoped to `crates/*/Cargo.toml` and does not cover `tools/`;
  `i2pr-interop` reaches no testkit crate, so no edge exists to miss.
- **Actual production edges:** `i2pr-crypto`, `i2pr-proto`,
  `i2pr-runtime`, `i2pr-storage`, `i2pr-transport`, and
  `i2pr-transport-ntcp2` — i.e. it deliberately reaches the runtime and
  transport crates from outside `crates/`. External production deps are
  `clap`, `rand_chacha`, `rand_core`, `serde`, `tokio`, and `toml`.
- **Practical consequence:** this crate is now compared against a real
  allowlist, but `check-runtime-boundaries.sh` still does not inspect
  `tools/`, so its Tokio usage remains outside the runtime-boundary
  policy.

### `i2pr-app-fixture` (Plan 369 WP5)

- **Position:** evidence tooling that lives in `crates/` but never reaches a
  shipped build. It holds the native fixture application and the fixture
  manager; the manager runs the real `i2pr_appd::Appd` against a test launch
  catalog, which is why `i2pr-app-fixture -> i2pr-appd` is its one unusual
  edge.
- **Inbound rule: none may depend on it, and no production *source* may even
  name it.** This is stronger than the `i2pr-testkit` rule because a production
  crate does not need a dependency edge to name an executable on disk — and
  naming the fixture manager binary is exactly how the shipped manager would be
  talked into launching it.
  `scripts/check-managed-app-process-boundary.py` rule 2 enforces both halves
  (a manifest edge, and the tokens `i2pr-app-fixture` / `i2pr_app_fixture` in
  production Rust). Its rule 1 additionally pins the three blessed spawn sites,
  so the fixture can only be reached through `i2pr-apphost`, which is the only
  component permitted to exec an application.
- **Actual production edges:** `i2pr-app-manager-proto`, `i2pr-app-proto`,
  `i2pr-appd`. External: `serde_json`, `thiserror`, `tokio`.

## Runtime boundaries (orthogonal enforcement)

These are enforced by
[`scripts/check-runtime-boundaries.sh`](../../scripts/check-runtime-boundaries.sh)
and are separate from the dependency-direction check:

- No `unbounded_channel` / `unbounded::<` / `UnboundedSender` /
  `UnboundedReceiver` in `i2pr-runtime`, `i2pr-testkit`, or
  `i2pr-service-tunnels` source.
- No `thread::sleep`, `mem::forget`, or raw `JoinHandle` in
  `i2pr-runtime` / `i2pr-testkit`; every `tokio::spawn` must retain an
  explicit owner (bound to `let`, `push(`, or `JoinSet`).
- No `tokio::*`, `std::net`, `std::fs`, `TcpStream`, `TcpListener`,
  `UdpSocket`, etc. in `i2pr-transport` / `i2pr-transport-ntcp2` /
  `i2pr-transport-ssu2`.
- No `tokio::*`, listeners, streams, tasks, or timers in
  `i2pr-service-tunnels` (runtime-neutral; the daemon owns sockets).
  `IpAddr`/`SocketAddr` values are allowed as validated data.
- No `async fn`, `async_trait`, `i2pr-netdb`, `i2pr-tunnel`, or
  `i2pr-client` in the transport contract crates; they stay synchronous
  and independent of routing clients.
- No UI/frontend dependency (`egui`, `iced`, `tauri`, `dioxus`, `yew`,
  `leptos`, `slint`) may enter `i2pr-i2pcontrol`.
- Test-only `dangerous()` TLS accept-any verifiers are forbidden in
  production `i2pr-daemon` source.
- `i2pr-console` has its own pair of checkers rather than folding into
  `check-runtime-boundaries.sh`:
  [`check-console-boundaries.sh`](../../scripts/check-console-boundaries.sh)
  (no socket, no `i2pr-*` dependency, no `tokio`/`std::net`, no
  unguarded router-state reach) and
  [`check-console-browser-security.sh`](../../scripts/check-console-browser-security.sh)
  (CSP and hardening headers applied centrally, no `Server` advertisement,
  no plaintext-password retention past config parse, no vendored
  third-party theme).
- See [Coverage gaps](#coverage-gaps) for the `tokio` manifest rule,
  which is currently inert against `tokio.workspace = true`.

## How to verify

From the repository root:

```bash
# Allowlist check: exits 0 and prints "dependency direction: ok".
bash scripts/check-dependency-direction.sh

# Orthogonal boundary checks.
bash scripts/check-runtime-boundaries.sh
bash scripts/check-service-tunnel-boundaries.sh
bash scripts/check-console-boundaries.sh
bash scripts/check-console-browser-security.sh

# Actual production edges, straight from the manifests.
cargo metadata --no-deps --format-version 1 | python3 -c '
import json, sys
for p in json.load(sys.stdin)["packages"]:
    print(p["name"], sorted(
        d["name"] for d in p["dependencies"]
        if d["kind"] in (None, "normal") and d["name"].startswith("i2pr-")))
'

# Actual dev-only workspace edges.
cargo metadata --no-deps --format-version 1 | python3 -c '
import json, sys
for p in json.load(sys.stdin)["packages"]:
    prod = {d["name"] for d in p["dependencies"] if d["kind"] in (None, "normal")}
    dev = sorted({d["name"] for d in p["dependencies"]
                  if d["kind"] == "dev" and d["name"].startswith("i2pr-")} - prod)
    if dev:
        print(p["name"], dev)
'
```

To confirm which crates the checker actually inspects, read the
`expected` dictionary in the script — it is the same Python the checker
runs, so it cannot drift from the enforcement behaviour.

## Cross-references

Architecture index and tooling:

- [Overview](overview.md)
- [Tooling, scripts, fixtures, and lanes](tooling.md)
- [Interop apparatus](interop-apparatus.md)
- [`scripts/check-dependency-direction.sh`](../../scripts/check-dependency-direction.sh)
- [`scripts/check-runtime-boundaries.sh`](../../scripts/check-runtime-boundaries.sh)
- [`scripts/check-service-tunnel-boundaries.sh`](../../scripts/check-service-tunnel-boundaries.sh)
- [`AGENTS.md`](../../AGENTS.md)
- Workspace manifest: [`Cargo.toml`](../../Cargo.toml)

Per-crate deep dives (one per crate in `crates/`):

- [`i2pr-proto.md`](i2pr-proto.md) — foundational namespace, zero
  production `i2pr-*` deps
- [`i2pr-crypto.md`](i2pr-crypto.md)
- [`i2pr-core.md`](i2pr-core.md) — zero deps of any kind
- [`i2pr-su3.md`](i2pr-su3.md) — leaf
- [`i2pr-i2pcontrol.md`](i2pr-i2pcontrol.md) — leaf
- [`i2pr-addressbook.md`](i2pr-addressbook.md)
- [`i2pr-storage.md`](i2pr-storage.md)
- [`i2pr-netdb.md`](i2pr-netdb.md)
- [`i2pr-netdb-persist.md`](i2pr-netdb-persist.md)
- [`i2pr-transport.md`](i2pr-transport.md) — no external production deps
- [`i2pr-transport-ntcp2.md`](i2pr-transport-ntcp2.md)
- [`i2pr-transport-ssu2.md`](i2pr-transport-ssu2.md)
- [`i2pr-tunnel.md`](i2pr-tunnel.md)
- [`i2pr-client.md`](i2pr-client.md)
- [`i2pr-api.md`](i2pr-api.md)
- [`i2pr-runtime.md`](i2pr-runtime.md)
- [`i2pr-service-tunnels.md`](i2pr-service-tunnels.md) — allowlisted but
  unused `i2pr-client` edge
- [`i2pr-console.md`](i2pr-console.md) — leaf, socketless console
  substrate
- [`i2pr-daemon.md`](i2pr-daemon.md) — composition root, 17 edges
- [`i2pr-testkit.md`](i2pr-testkit.md) — test-only, no production
  dependents
