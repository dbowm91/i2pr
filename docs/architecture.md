# Architecture

Top-level architecture narrative. The detailed crate ownership map,
dependency graph, tooling, and per-crate deep-dives live in
[`docs/architecture/`](architecture/). This file records the
modular-monolith boundaries, ownership rules, and the four conceptual
planes; it is intentionally short. Read
[`docs/architecture/overview.md`](architecture/overview.md) for the
bird's-eye view, then follow the deep-dive links.

> Status: experimental. Not production-ready. Not for anonymity or
> security-sensitive workloads. See `README.md` and `GUARDRAILS.md`.

## Four planes

| Plane | Responsibility | Current bounded status |
| --- | --- | --- |
| Data | Protocol representations, authenticated links, messages, network tunnel traffic | Bounded common-structure and I2NP models, Standard LeaseSet2 (Plan 119), Streaming wire format (Plan 128), transport-neutral link contracts, NTCP2 state, runtime-owned local TCP integration; no public-network behavior |
| Control | Configuration, lifecycle, health, cancellation, supervision, resource budgets | Runtime-neutral core contracts plus the `i2pr-runtime` supervisor and bounded socket-owning services |
| Client | Destinations, LeaseSets, streaming, SAM, I2CP adapters | Milestone 6 local product closed via Plan 134 (destinations, garlic, LS2, Streaming); SAM baseline planning (Milestone 7) is next |
| Service | HTTP, SOCKS5, IRC, generic TCP, local service tunnels | Not implemented |

Network tunnels carry router-to-router I2P traffic and are distinct
from application service tunnels, which eventually connect a local
application to a destination. The latter must not import transport
internals or peer-profile storage.

## Crate graph

The full allowlist and ASCII diagram live in
[`docs/architecture/dependency-graph.md`](architecture/dependency-graph.md).
The dependency direction is mechanically checked by
`scripts/check-dependency-direction.sh`. The current workspace has
13 production crates under `crates/` plus the non-production
`tools/i2pr-interop/` launcher binary. `i2pr-testkit` is a
test/simulation crate; no production crate may depend on it.

```text
i2pr-proto  <- i2pr-crypto <- i2pr-storage
     ^              ^               ^
     |              |               |
i2pr-core <- i2pr-transport <- i2pr-runtime <- i2pr-daemon (composition root)
     ^             ^              ^
     |             |              |
     +-------------+  i2pr-transport-ntcp2
                          ^
                          |
                    i2pr-proto + i2pr-crypto

i2pr-netdb  <- i2pr-netdb-persist <- i2pr-runtime <- i2pr-daemon
   (RouterInfo validation, LS2 store)    (Plan 104 cache + reseed)
                                       ^
                                  i2pr-tunnel
                                       ^
                                  i2pr-client
                                  (Milestone 6 + SAM)

i2pr-testkit (test/simulation only; no production crate may depend on it)
tools/i2pr-interop (non-production launcher; never activates i2pr-daemon)
```

The arrows show dependency direction. `i2pr-proto` owns protocol-
facing names, bounds, typed codec error categories, and the
structural I2NP / I2NP Garlic / Standard LeaseSet2 / Streaming wire
codecs. `i2pr-core` owns runtime-neutral service contracts,
cancellation tokens, and resource budgets. `i2pr-runtime` owns
Tokio, wakeable cancellation, the service graph, supervised task
managers, bounded restart policy, graceful/forced shutdown, TCP
listeners, streams, deadline timers, replay-cache state, and link
child tasks. `i2pr-transport-ntcp2` owns the NTCP2 protocol
implementation (Noise XK handshake, AES-CBC ephemeral obfuscation,
ChaCha20-Poly1305 data phase, SipHash length masking, deterministic
state machines) but no Tokio or socket. `i2pr-runtime` is the sole
production owner of Tokio tasks, sockets, timers, channels, and
wakeable cancellation.

`i2pr-tunnel` (Milestone 5 substrate) and `i2pr-client` (Milestone 6
local product) compose on top of `i2pr-runtime` and the lower
crates. `i2pr-client` depends on `i2pr-core` / `i2pr-crypto` /
`i2pr-netdb` / `i2pr-proto` / `i2pr-tunnel`; it never composes back
into `i2pr-tunnel` / `i2pr-netdb` and never imports `i2pr-daemon`.

## Production ownership rules

The boundary contract is enforced by scripts under `scripts/`:

| Script | Catches |
| --- | --- |
| `check-dependency-direction.sh` | Crate-layer DAG violations |
| `check-runtime-boundaries.sh` | `unbounded_channel`, `tokio::*` / `std::net` / `std::fs` in transport crates, raw `JoinHandle`s, `tokio::spawn` without an owner, `async fn` in transport contracts, `i2pr-testkit` referenced by a production crate |
| `check-fixture-manifest.sh` | Drift in the I2NP fixture corpus |
| `check-ntcp2-vectors.sh` | Drift in the NTCP2 crypto vector corpus |
| `check-ntcp2-interoperability.sh` | Forbidden artifacts in the synthetic private NTCP2 interoperability lane |
| `check-rootless-interop-boundary.sh` | Plan 046 rootless lane constraints (no `sudo` / `ip netns` / `nft` / `setcap` / `--privileged` / `--network host`; no silent privileged fallback) |
| `check-multipass-interop-boundary.sh` | Plan 048/049/050/051 Multipass recovery lane (no global `multipass purge`; no host policy mutation) |
| `check-constrained-host-lane-boundary.sh` | Plan 077 constrained-host selection-order boundaries |
| `check-plan095-workflow.sh` | Plan 095 manual live-wire workflow artifact-path drift and cleanup-guard violations |

Production crates do not depend on `i2pr-testkit`, and `i2pr-proto`
does not depend on filesystem or crypto execution. The daemon is
the only crate that composes configuration, explicit identity
lifecycle commands, crypto randomness, and storage; the daemon
**does not** activate NTCP2 in the production service graph.

## How data flows at runtime

A live `i2pr run` (Plan 106) follows this sequence:

1. `i2pr-daemon` parses CLI flags, loads and validates the TOML
   config under `deny_unknown_fields` (including the `[netdb]` and
   `[reseed]` sections), maps errors to stable exit codes, then
   runs `bootstrap_daemon` before starting the supervisor.
2. `i2pr-storage` loads the router identity from
   `<data_dir>/router.identity` and (separately) the NTCP2 static
   key from `<data_dir>/ntcp2.static.key`. Either file can be
   generated, but never silently replaced.
3. `i2pr-netdb` validates and self-validates the local RouterInfo
   through `LocalRouterInfoBuilder`; refuses any transport address
   or forbidden capability letter under the Plan 101 activation
   guard.
4. `i2pr-netdb-persist` loads and revalidates the persistent
   RouterInfo cache through the Plan 104 `CacheLoader`, then runs
   the optional bounded offline SU3 reseed through
   `ReseedIngestor`.
5. `i2pr-netdb` keeps the populated `RouterInfoStore`,
   `CoalescedRouterInfoLookup`, `PublicationCoordinator`, the
   transport-neutral state machines, and the Standard LeaseSet2
   store (`ValidatedLeaseSet2`, `LeaseSet2Store`, `LookupKind::
   LeaseSet2`) ready for the Milestone 5 runtime adapter.
   `i2pr-daemon`'s `NetDbSeam` exposes them through a stable
   surface; under Plan 117 it consults an injected
   `i2pr_netdb::ReplyPathProvider` implementation backed by
   `i2pr-tunnel`'s exploratory pool, so a registered inbound
   tunnel flips the seam's status to `Available` and a
   `NeedExploratoryReplyPath` lookup action is converted into a
   real path on the state machine.
6. `i2pr-runtime` builds a `ServiceGraph`, topologically validates
   it before startup, then spawns one supervisor manager per
   service via a `JoinSet`. Each service receives a narrowed
   `ServiceContext` (name, cancellation, readiness, health, child
   scope) — never a direct handle to the supervisor.
7. `i2pr-transport-ntcp2` is declared but **not yet used** in the
   production daemon. It implements the protocol: Noise XK
   handshake, AES-CBC ephemeral obfuscation, ChaCha20-Poly1305
   data phase, directional SipHash frame-length masking,
   deterministic handshake state machines. The Plan 101 NTCP2
   activation guard keeps the daemon from registering
   `ntcp2-transport`.
8. `i2pr-transport` sits underneath as the runtime-neutral link
   manager: `LinkState` FSM, `TransportManager` admission with
   RAII leases, duplicate-resolution policy, privacy-safe
   `TransportSnapshot`.
9. `i2pr-core` provides lifecycle, health snapshots, cancellation
   tokens, and the shared `ResourceBudget` governor.
10. `i2pr-client` (Plan 120+) owns the local destination runtime:
    identity (Plan 120), ECIES-X25519-AEAD-Ratchet session layer
    (Plan 126), destination routing and LeaseSet2 binding
    (Plan 122/127), Streaming core and adapter (Plan 128, with the
    Plan 134 receive-window ACK ceiling closure).
11. `i2pr-proto` and `i2pr-crypto` stay at the bottom — no one
    depends on anything above them except the test and integration
    layers.
12. `i2pr-testkit` is used only by tests. It exercises the same
    crates through a `NetworkScheduler`, `ManualClock`,
    `Ntcp2DataPhaseDriver`, and a 128-bit `ReproducibilitySeed`.
    Tests use `#[tokio::test(start_paused = true)]`; no
    wall-clock sleeps, no real sockets, no DNS, no public-network
    traffic.

The Milestone 6 local product (destinations, garlic, LS2,
Streaming) is closed locally via Plan 134; independent-router
interoperability is tracked separately as external acceptance
debt. The next product layer is SAM baseline planning (Milestone
7).

## Conventions

These apply across every crate and are enforced by workspace
lints, script gates, and review:

- `#![forbid(unsafe_code)]` on every crate (workspace lint
  `unsafe_code = "deny"`).
- `unexpected_cfgs = "deny"`, `unused_must_use = "warn"`.
- Clippy denies `dbg_macro`, `todo`, `unimplemented`.
- `crate/secret` owners are non-cloneable, non-`Debug`, and
  `zeroize::Zeroize` on drop; the NTCP2 forbidden nonce
  `2^64 - 1` is never emitted.
- Codec errors are typed; decode/encode results are never
  swallowed.
- NTCP2 static-key/IV material lives in the separate versioned
  `i2pr-storage` record — never derived from or overwrite the
  router identity record.
- Configuration, protocol, and persisted data are treated as
  hostile: explicit bounds, rejection of unknown or trailing
  bytes, no validation side effects, and always a tested
  negative path.
- All architecture/security decisions live under `docs/adr/`
  (`0001` through `0025`); the plan-of-record is the active
  `plans/NNN-*.md` plus its closure document. When closing a
  milestone, attach a closure record with commands, results, and
  evidence.

## Cross-references

- [`docs/architecture/overview.md`](architecture/overview.md) —
  bird's-eye view, crate graph, crate index, data-flow narrative
- [`docs/architecture/dependency-graph.md`](architecture/dependency-graph.md) —
  per-crate allowlist + ASCII graph
- [`docs/architecture/tooling.md`](architecture/tooling.md) —
  scripts, fixtures, integration lanes, CI, fuzz
- [`docs/architecture/interop-apparatus.md`](architecture/interop-apparatus.md) —
  historical NTCP2 interop apparatus (Plan 038–100; substantially
  stale per the 2026-08-27 audit; rewrite deferred to the next
  interop-surface plan)
- [`docs/architecture/audit/`](architecture/audit/) — past
  doc-vs-source drift audits
- [`docs/architecture/i2pr-<crate>.md`](architecture/) — per-crate
  deep-dives (13 crates)
- [`docs/security-model.md`](security-model.md) — secret-bearing
  types, memory hygiene, codec error policy
- [`docs/protocol-support.md`](protocol-support.md) — generated
  from `specs/support.toml`
- [`specs/CONFORMANCE.md`](../specs/CONFORMANCE.md) — what counts
  as evidence
- [`AGENTS.md`](../AGENTS.md) — repository guidelines
- [`.opencode/skills/`](../.opencode/skills/) — loadable skill
  bundles for OpenCode sessions
