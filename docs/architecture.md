# Architecture

Top-level architecture narrative. The detailed crate ownership map,
dependency graph, tooling, and per-crate deep-dives live in
[`docs/architecture/`](architecture/). This file records the
modular-monolith boundaries, ownership rules, and the conceptual
planes; it is intentionally short. Read
[`docs/architecture/overview.md`](architecture/overview.md) for the
bird's-eye view and the deep-dive index, then follow the deep-dive
links.

> Status: experimental. Not production-ready. Not for anonymity or
> security-sensitive workloads. See `README.md` and `GUARDRAILS.md`.

## Conceptual planes

| Plane | Responsibility | Current bounded status |
| --- | --- | --- |
| Foundation | Wire codecs, crypto wrappers, runtime-neutral service contracts, signed containers | Bounded common-structure and I2NP models, Standard LeaseSet2 (Plan 119), Streaming wire format (Plan 128), SU3 envelope verification, zero-dependency core contracts; no I/O |
| Data | Authenticated links, I2NP messages, network tunnel traffic, garlic | Transport-neutral link contracts, SSU2 v2 protocol (Plan 161, bounded direct-IPv4 loopback vs exact-pinned i2pd), NTCP2 state machines (closed at `protocol-defect-localized` / `noise_authenticated`), runtime-owned local TCP integration; no public-network behavior |
| Network state | RouterInfo / LeaseSet2 / ELS2 validation, stores, lookup, publication, tunnel construction | Bounded RouterInfo + LeaseSet2 stores, coalesced lookup and publication machines, floodfill record/provenance model on the type 0/1/3/7 floor; **no floodfill advertisement, no Encrypted LeaseSet2 capability advertised** |
| Control | Configuration, lifecycle, health, cancellation, supervision, resource budgets, identity persistence | Runtime-neutral core contracts plus the `i2pr-runtime` supervisor, versioned atomic identity storage, and bounded socket-owning services |
| Client / service | Destinations, LeaseSets, streaming, SAM, I2CP, I2PControl, naming, HTTP/SOCKS5/IRC/generic service tunnels | M6 local product closed via Plan 134 (`milestone6_interoperable = not-yet-claimed`); M7 SAM 3.1 localhost via Plan 151; M8 SSU2 via Plan 161; M9 I2CP loopback via Plan 172; M10 service tunnels via Plans 174–180/182 + 213–215 |

Network tunnels carry router-to-router I2P traffic and are distinct
from application service tunnels, which eventually connect a local
application to a destination. The latter must not import transport
internals or peer-profile storage.

## Crate graph

The full allowlist and ASCII diagram live in
[`docs/architecture/dependency-graph.md`](architecture/dependency-graph.md).
The dependency direction is mechanically checked by
`scripts/check-dependency-direction.sh`. The current workspace has
**19 crates** under `crates/` (18 production + `i2pr-testkit`) plus the
non-production `tools/i2pr-interop/` launcher binary. `i2pr-testkit` is
a test/simulation crate; no production crate may depend on it, and
`scripts/check-runtime-boundaries.sh` enforces that by globbing every
`crates/*/Cargo.toml` (so `[dev-dependencies]` is covered too).

```text
                i2pr-core            i2pr-su3           i2pr-i2pcontrol
              (zero deps)         (zero deps)         (zero deps)
                  |                    |                     |
i2pr-proto <- i2pr-crypto                |                     |
    ^    ^            ^                 |                     |
    |    |      i2pr-storage            |                     |
    |    |            |                 |                     |
    +----+------------+--------+        |                     |
         |                     |        |                     |
  i2pr-transport         i2pr-netdb <---+                     |
    ^  ^      ^               ^                              |
    |  |      |               |                              |
i2pr-transport-ntcp2   i2pr-netdb-persist                    |
i2pr-transport-ssu2         ^                               |
    ^                      |                               |
    |                      |                               |
i2pr-runtime               |                               |
    ^                      |                               |
i2pr-tunnel <--------------+                               |
    ^                      |                               |
i2pr-client ----------------+                               |
    ^                      |                               |
i2pr-api  i2pr-addressbook |                               |
    ^      i2pr-service-tunnels                            |
    +-----------------------+                               |
                                                             |
i2pr-daemon (composition root; 15 workspace deps) <----------+

i2pr-testkit (test-only)   tools/i2pr-interop (non-production)
```

The arrows show dependency direction, always from the dependent crate
toward its dependency. `i2pr-proto` owns protocol-facing names,
bounds, typed codec error categories, and the structural I2NP / I2NP
Garlic / Standard LeaseSet2 / Streaming / I2CP-Data-body wire codecs.
`i2pr-core` owns runtime-neutral service contracts, cancellation
tokens, and resource budgets. `i2pr-runtime` owns Tokio, wakeable
cancellation, the service graph, supervised task managers, bounded
restart policy, graceful/forced shutdown, TCP/UDP listeners, streams,
deadline timers, replay-cache state, and link child tasks.
`i2pr-transport-ntcp2` owns the NTCP2 protocol implementation (Noise XK
handshake, AES-CBC ephemeral obfuscation, ChaCha20-Poly1305 data
phase, SipHash length masking, deterministic state machines) but no
Tokio or socket. `i2pr-runtime` is the sole production owner of Tokio
tasks, sockets, timers, channels, and wakeable cancellation.

Note that `i2pr-netdb-persist` is composed by the **daemon**, not by
`i2pr-runtime` — the runtime layer has no filesystem concern. It is the
only composition crate that sits between `i2pr-storage` bytes and
`i2pr-netdb` validation.

`i2pr-tunnel` (exploratory + transit substrate) and `i2pr-client`
(Milestone 6 local product) compose on top of the lower crates.
`i2pr-client` depends on `i2pr-core` / `i2pr-crypto` / `i2pr-netdb` /
`i2pr-proto` / `i2pr-tunnel`; it never composes back into
`i2pr-tunnel` / `i2pr-netdb` and never imports `i2pr-daemon`.
`i2pr-addressbook` owns canonical `.i2p` naming and
`i2pr-i2pcontrol` owns the Proposal 170 wire/domain contract; both are
runtime-neutral leaves that the daemon projects into its listeners.

## Production ownership rules

The boundary contract is enforced by scripts under `scripts/`:

| Script | Catches |
| --- | --- |
| `check-dependency-direction.sh` | Crate-layer DAG violations |
| `check-runtime-boundaries.sh` | `unbounded_channel`, `tokio::*` / `std::net` / `std::fs` in transport, i2pcontrol and service-tunnel crates, raw `JoinHandle`s, `tokio::spawn` without an owner, `async fn` in transport contracts, `i2pr-testkit` referenced by any `crates/*/Cargo.toml`. NOTE: this script has **no `i2pr-api` section**, so its "passed" result is not evidence for that crate — see [`i2pr-api.md`](architecture/i2pr-api.md). It also greps `std::net` literally, so an import hidden in a grouped `use std::{…}` can evade it. |
| `check-fixture-manifest.sh` | Drift in the I2NP fixture corpus |
| `check-ntcp2-vectors.sh` | Drift in the NTCP2 crypto vector corpus |
| `check-ntcp2-interoperability.sh` | Forbidden artifacts in the synthetic private NTCP2 interoperability lane |
| `check-rootless-interop-boundary.sh` | Plan 046 rootless lane constraints (no `sudo` / `ip netns` / `nft` / `setcap` / `--privileged` / `--network host`; no silent privileged fallback) |
| `check-multipass-interop-boundary.sh` | Plan 048/049/050/051 Multipass recovery lane (no global `multipass purge`; no host policy mutation) |
| `check-constrained-host-lane-boundary.sh` | Plan 077 constrained-host selection-order boundaries |
| `check-sam-acceptance-evidence.sh` | Plan 151 SAM evidence integrity (no synthetic `passed` rows) |
| `check-ssu2-acceptance-evidence.sh` | Plan 161 SSU2 evidence integrity (no synthetic `passed` rows) |
| `check-ssu2-vectors.sh` | Drift in the SSU2 v2 fixture corpus |
| `check-i2cp-vectors.sh` | Drift in the I2CP fixture corpus |
| `check-i2cp-acceptance-evidence.sh` | Plan 170/172 I2CP evidence integrity (no synthetic `passed` rows) |
| `check-service-tunnel-boundaries.sh` | Plan 180 M10 runtime-neutral invariants |
| `check-service-tunnel-acceptance-evidence.sh` | Plan 181/213/214/215 service-tunnel evidence integrity (no synthetic `passed` rows) |
| `check-exploratory-tunnel-evidence.sh` | Plan 185 exploratory-tunnel evidence integrity |
| `check-netdb-tunnel-evidence.sh` | Plan 186 NetDB-over-tunnel evidence integrity |
| `check-destination-tunnel-evidence.sh` | Plan 187/192 destination-tunnel evidence integrity |
| `check-streaming-tunnel-evidence.sh` | Plan 193 Streaming-tunnel evidence integrity |
| `check-m6-mixed-router-acceptance-evidence.sh` | Plan 189 §8 / 194 / 196 / 197 / 200 / 201 cross-family M6 evidence integrity |
| `check-m6-final-closure-evidence.sh` | Plan 198/204 evidence-consuming final gate (manual external-workflow only, not routine CI) |
| `check-m11-transit-qualification-evidence.sh` | Plan 268 M11 one-family transit qualification evidence integrity |
| `check-m12-floodfill-qualification-evidence.sh` | Plan 279 §9 M12 floodfill qualification evidence integrity (supports `--self-test`) |
| `check-i2pcontrol-acceptance-evidence.sh` | Proposal 170 / I2PControl evidence integrity |

Two gaps in this table are worth recording rather than hiding:

- `scripts/check-m12-floodfill-boundaries.sh` **currently exits 1 on
  repo head**. It still enforces the Plan 281 "type 5 is deferred"
  floor by grepping `DatabaseStoreData::EncryptedLeaseSet` in
  `crates/i2pr-netdb/src`, which Plans 332/333 now legitimately
  populate. It is in neither the `AGENTS.md` routine floor nor
  `.github/workflows/ci.yml`, so the failure is silent. Resolving it
  needs a plan owner's call (retire the rule or re-scope it), not a
  docs edit.
- `scripts/check-plan095-workflow.sh` (Plan 095 manual live-wire
  workflow) was pruned by the Plan 099 harness reduction and is no
  longer on disk; historical references to it are audit context only
  and must not be linked as live commands.

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
Streaming) is closed locally via Plan 134. Under ADR 0026 / Plan 248,
exact-pinned i2pd Plan 193 closes experimental mixed-router progression;
full Java-router compatibility is retained/deferred at Plan 247 and full
two-family router conformance remains unclaimed. The SAM 3.1 localhost product is closed via
Plan 151 (see [`plans/closure/sam/151-status.md`](../plans/closure/sam/151-status.md));
the I2CP loopback product is closed via Plan 172; the M10 remote
generic + HTTP/IRC application product is closed via Plans 214–215.

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
  (`0000` through `0031`; ADRs are append-only, and a superseded ADR
  keeps its original text plus a supersedure marker). The
  plan-of-record is `plans/implementation/<subsystem>/NNN-*.md` plus
  its closure record under `plans/closure/<subsystem>/NNN-status.md`,
  indexed by `plans/registry.md`. When closing a milestone, attach a
  closure record with commands, results, and evidence.

## Cross-references

- [`docs/architecture/overview.md`](architecture/overview.md) —
  bird's-eye view, crate graph, crate index, data-flow narrative
- [`docs/architecture/dependency-graph.md`](architecture/dependency-graph.md) —
  per-crate allowlist + ASCII graph
- [`docs/architecture/tooling.md`](architecture/tooling.md) —
  scripts, fixtures, integration lanes, CI, fuzz
- [`docs/architecture/interop-apparatus.md`](architecture/interop-apparatus.md) —
  the interoperability/evidence apparatus: evidence classes,
  reference pins, fail-closed lane discipline, the live lanes, and
  the **historical** NTCP2 interop surface (Plans 038–100, closed at
  `protocol-defect-localized` / `noise_authenticated`)
- [`docs/architecture/audit/`](architecture/audit/) — past
  doc-vs-source drift audits
- [`docs/architecture/i2pr-<crate>.md`](architecture/) — per-crate
  deep-dives (**19 crates**, one per workspace member)
- [`docs/security-model.md`](security-model.md) — secret-bearing
  types, memory hygiene, codec error policy
- [`docs/protocol-support.md`](protocol-support.md) — generated
  from `specs/support.toml`
- [`specs/CONFORMANCE.md`](../specs/CONFORMANCE.md) — what counts
  as evidence
- [`AGENTS.md`](../AGENTS.md) — repository guidelines
- [`.opencode/skills/`](../.opencode/skills/) — loadable skill
  bundles for OpenCode sessions


## Router-role status

Plan 268 closes M11 one-family experimental progression. Public transit
remains disabled, non-advertised, and unclaimed (ADR 0026, ADR 0031 for
the single shared service-tunnel manager).

M12 floodfill is the active role lane. Plans 270–276 passed the type
0/1/3/7 floor (architecture, provenance, record validation/storage,
bounded DatabaseStore/DatabaseLookup services, replication, versioned
persistence, bounded maintenance, resource leases); Plan 280 stopped
for want of a maintained Red25519 provider, and Plan 281 deferred
EncryptedLeaseSet type 5. Both of those gaps were later closed
in-repo: Plan 330 passed an independent Red25519 implementation, Plan
331 passed qualification with a reference signature-transcript
divergence recorded, and Plans 332/333 passed the ELS2 type-5
foundation plus PSK/DH client authorization. **No capability is
advertised** (`common.leaseset2-family` is `advertised = false`) and
no live interoperability is claimed, because per ADR 0005 neither
i2pd nor Java I2P can verify the transcript.

Floodfill advertisement, daemon role lifecycle, and second-family
qualification remain unimplemented and unclaimed.

> Authority note: `plans/registry.md` and parts of
> `specs/support.toml` lag the closure records. `plans/closure/<subsystem>/NNN-status.md`
> wins — see `plans/README.md` for the ordering. Current known lags:
> the registry still reports Proposal 170 Plans 322 and 334 as blocked
> (both are `passed`/reclosed-passed), and the `m12_*` support rows
> still describe the superseded Plan 280/281 stopped/deferred state.
