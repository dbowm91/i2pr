# Portable Service-Tunnel Core Roadmap

Status: complete — Plans 349–351, 359, and 379 all closed. The owner selected MIT, so the legal-selection gate Plan 350 recorded is lifted; publication remains disabled for a different, named **technical** reason (see §12). This parallel portability/reuse work line does not reopen M10 product closure and does not implement a SAM client, SAM daemon, Python binding, C ABI, tunnel WebUI, or application sidecar in i2pr.

Long-term references:
- `GUARDRAILS.md`
- `specs/CONFORMANCE.md`
- `specs/support.toml`
- `docs/architecture/overview.md`
- `docs/architecture/dependency-graph.md`
- `plans/subsystems/service-tunnels-roadmap.md`
- `plans/subsystems/sam-roadmap.md`
- `plans/subsystems/anonymity-roadmap.md`
- `plans/subsystems/managed-native-app-runtime-roadmap.md`

Related ADRs:
- Plan 349 must add the durable portability/ownership ADR before changing the reusable crate boundary.
- Existing service-tunnel and anonymity ADRs remain authoritative for Destination/linkability and filtering semantics; this workstream must reference rather than rewrite them.

## 1. Purpose and ownership boundary

Turn the already runtime-neutral `i2pr-service-tunnels` policy/protocol crate into a deliberately reusable boundary that can be consumed by a future independent SAM library/tunnel-manager repository without copying privacy filters, service-profile logic, access policy, Destination/linkability rules, or resource ceilings.

The intended ownership split is:

```text
                           i2pr-service-tunnels
                     reusable policy / filtering core
                         /                       \
                        /                         \
          i2pr native service runtime        external adapter
             i2pr-daemon / Streaming        e.g. future SAM repo
                        |                         |
                  native router path          SAM transport path
```

`i2pr` owns the reusable service-profile semantics and their security/privacy invariants. A downstream repository owns SAM protocol/client implementation, Python/C bindings, process/daemon lifecycle, tunnel configuration-file syntax, WebUI, and application-sidecar packaging.

This workstream must not create a second SAM implementation inside i2pr and must not move socket/task ownership into `i2pr-service-tunnels`.

## 2. Work classification

- **Invariant:** one source of truth for service-tunnel filtering/privacy/access/linkability semantics; transport adapters cannot bypass policy by selecting another runtime; Destination sharing remains explicit; bounded/fail-closed behavior remains intact.
- **Infrastructure:** stable public Rust API boundary, dependency cleanup, package metadata, consumer-facing documentation, adapter-facing value types/contracts, and an external-consumer conformance fixture.
- **Capability:** a future external repository may use the crate to construct SAM-backed client/server tunnel products. Plans 349–351 do not themselves provide that user-visible capability.
- **Polish:** crates.io publication, generated API documentation, release automation, richer examples, Python/C documentation, and tunnel-manager UX follow the core portability closure and owner-selected licensing decision.

## 3. Non-goals

Plans 349–351 do not:
- implement SAM 3.3, PRIMARY/MASTER compatibility, DATAGRAM2/3, or any other SAM wire behavior;
- modify i2pr's existing SAM server milestone authority;
- implement a separate tunnel daemon, WebUI, CLI manager, app sidecar, Python binding, or C ABI;
- copy code from Yosemite, sam3, go-sam-go, sam-forwarder, Java I2P, i2pd, I2P+, or Emissary;
- choose a repository license on the owner's behalf;
- make service tunnels non-loopback by default or advertise a new router capability;
- reopen M10 Plan 215 closure;
- make `i2pr-service-tunnels` own Tokio, sockets, timers, DNS, filesystem I/O, process lifecycle, or router internals.

## 4. Current state

M10 is closed at Plan 215. The reusable core already exists and is unusually close to the desired external boundary:

- `crates/i2pr-service-tunnels` explicitly owns runtime-neutral configuration, destination references, access policy, protocol filters, profile logic, bounded errors/events, and generation/diff helpers.
- It forbids unsafe code and currently owns no sockets, Tokio tasks, timers, filesystem access, NetDB mutation, transport internals, or Garlic/I2NP construction.
- Its modules include access/auth/config/connect/destination/events/generation/http/irc/socks5/streamr/outproxy and related privacy/resource policy.
- The package is `publish = false`. Its Cargo manifest declares one workspace dependency, `i2pr-proto`, added by Proposal 170/351 and permitted by the Plan 359 amendment.
- The owner selected MIT in the Plan 379 registration branch and every workspace member manifest now inherits that license, so `cargo metadata` reports `license = "MIT"` for all 26 packages.
- Plan 379 ran the package-graph audit Plan 350 declined to run. The result is a **technical** blocker, not the legal one: the `i2pr-proto` edge is path-only with no `version` requirement, and `cargo package` refuses to stage a dependency without one. `publish = false` therefore stays.
- External-consumer conformance is not currently part of the test floor; existing verification is primarily workspace/internal composition.

The intended future SAM repository does not yet belong to this roadmap. It is a downstream consumer once the portability contract closes.

## 5. Target architecture

The reusable surface stays policy-oriented and transport-neutral:

```text
external config/parser
       |
       v
ServiceTunnelSpec / ServiceTunnelSet
       |
       +--> destination/linkability policy
       +--> access/rate policy
       +--> HTTP/SOCKS/IRC/CONNECT filters
       +--> resource/time/idle policy
       +--> generation/diff helpers
       |
       v
adapter-owned runtime
  - connect/accept stream
  - identify authenticated peer
  - resolve destination
  - bind local endpoint
  - own timers/backpressure/reconnect
```

The core may define bounded adapter-facing metadata/value types where necessary, but not async runtime traits that force Tokio or one executor. The downstream adapter remains responsible for translating its transport's authenticated identity and stream/datagram metadata into the reusable policy inputs.

Destination-group semantics are part of the portable contract. A downstream adapter that maps explicit shared groups to a shared transport identity/session must preserve the same linkability domain; it may not silently split or merge Destinations.

## 6. Dependency graph

```text
349 portability ownership + public-contract freeze
  -> 350 package/API/dependency stabilization
       -> 351 external-adapter conformance fixture + downstream handoff
            -> future separate SAM repository (unregistered here)
```

Plans 349–351 passed, Plan 359 amended the boundary to permit exactly one `i2pr-proto` edge for canonical encrypted-service parsing, and Plan 379 reconciled the MIT selection, the current external-consumer pin, the package-distribution posture, and the obsolete branch. The public core is Git-consumable and nothing was published.

## 7. Milestones

| Plan | State | i2pr token | Classification | Implementation | Closure |
|---|---|---|---|---|---|
| 349 | closed | `passed-portable-service-tunnel-boundary-and-ownership-contract` | invariant + infrastructure | `plans/implementation/portable-service-tunnels/349-portable-service-tunnel-boundary-and-ownership-contract.md` | `plans/closure/portable-service-tunnels/349-status.md` |
| 350 | closed | `passed-portable-service-tunnel-package-api-stabilization-publication-blocked-by-license-selection` | infrastructure + polish | `plans/implementation/portable-service-tunnels/350-service-tunnel-package-api-and-dependency-stabilization.md` | `plans/closure/portable-service-tunnels/350-status.md` |
| 351 | closed | `passed-portable-service-tunnel-external-adapter-conformance-and-sam-handoff` | infrastructure | `plans/implementation/portable-service-tunnels/351-external-adapter-conformance-and-sam-handoff-contract.md` | `plans/closure/portable-service-tunnels/351-status.md` |
| 359 | closed | `passed-portable-core-boundary-one-permitted-proto-edge-amendment` | invariant + planning amendment | `plans/implementation/portable-service-tunnels/359-portable-core-boundary-proto-edge-amendment.md` | `plans/closure/portable-service-tunnels/359-status.md` (executed inline in `2416c30b`, not as a dedicated pass; Plan 379 wrote the record additively) |
| 379 | closed | `passed-mit-license-and-portable-service-tunnel-cleanup-with-named-crates-io-blocker` | polish + infrastructure + invariant preservation | `plans/implementation/portable-service-tunnels/379-mit-license-and-portable-service-tunnel-cleanup.md` | `plans/closure/portable-service-tunnels/379-status.md` |

## 8. Cross-cutting requirements

- M10's current product semantics remain unchanged unless an explicit corrective with regression evidence proves a defect.
- `i2pr-service-tunnels` remains runtime-neutral and `#![forbid(unsafe_code)]`.
- No public API may require `i2pr-daemon`, `i2pr-runtime`, live NetDB stores, tunnel pools, transport managers, or router-global context.
- External consumers must receive the same bounded parsing/filtering/access behavior as i2pr's native composition.
- Destination/linkability policy must remain explicit; adapters may not infer sharing from tunnel type or capacity.
- Peer-based server filters operate on authenticated I2P Destination identity/hash supplied by the adapter. Unauthenticated metadata may not be promoted to authenticated peer identity.
- HTTP privacy/server filters, SOCKS/IRC policy, and access/rate limits must remain transport-independent.
- No external implementation source is copied into this work line. Specifications and independently observed behavior remain valid references.
- MIT is now owner-selected. Package publication still requires a separate technical package-graph audit and must remain disabled unless that audit passes; Plan 379 performs the audit but does not publish.
- Public API stabilization must minimize accidental commitment to M10-internal naming or plan-number vocabulary.
- Compatibility with Rust MSRV/workspace policy remains required.

## 9. Verification strategy

Evidence proceeds in three levels:

1. **Boundary proof (349):** source/dependency inventory, public-surface audit, ADR/spec freeze, static guards proving the reusable core has no runtime/router-internal ownership.
2. **Packaging/API proof (350):** compile/test/doc/clippy floor, dependency cleanup proof, `cargo package` dry-run when legally/package-metadata eligible, semver/public-API review, internal i2pr consumer compatibility.
3. **External-consumer proof (351):** a fixture outside the i2pr workspace graph imports only the reusable crate/public artifacts and drives representative generic/HTTP/SOCKS/IRC/access/linkability behavior without `i2pr-daemon`, `i2pr-runtime`, or private modules.

The final external fixture is transport-fake/deterministic. It does not need a real SAM router; the purpose is to prove that a future SAM adapter has sufficient public seams without duplicating policy.

## 10. Risks and decision points

Primary risks:
- turning a portability effort into a second runtime/transport abstraction framework;
- stabilizing internal M10 implementation details as public API accidentally;
- moving router-specific types into the external contract simply because i2pr already has them;
- duplicating service-profile logic in a downstream SAM repository rather than exposing the correct reusable primitive;
- making shared-Destination behavior transport-specific and creating anonymity/linkability drift;
- enabling package publication despite the *unresolved* package-graph prerequisite — MIT is selected, but `cargo package` still refuses the path-only `i2pr-proto` edge;
- over-coupling the core to one async runtime for adapter convenience;
- conflating the future SAM library's clean-room protocol implementation with reuse of i2pr-owned service-tunnel policy.

If API stabilization reveals a genuine split between reusable profile policy and i2pr-only orchestration, Plan 349 must record that boundary before Plan 350 moves files/crates. Do not perform an opportunistic repository split without the ADR.

## 11. Completion definition

This workstream is complete when:
- a durable ADR/spec states exactly which service-tunnel semantics i2pr owns for reuse and which runtime concerns remain adapter-owned;
- `i2pr-service-tunnels` has no accidental/dead router-protocol dependency and no forbidden runtime ownership;
- the intended public Rust surface is documented and semver-reviewed;
- package metadata is externally consumable, with publication gated truthfully on a **named** prerequisite rather than on a stale legal condition;
- i2pr's existing service-tunnel consumers remain behaviorally unchanged;
- an out-of-workspace consumer fixture can use generic/server/client, access, destination/linkability, and representative HTTP/SOCKS/IRC filtering solely through public APIs;
- the downstream SAM handoff documents identity/session mapping, authenticated peer requirements, lifecycle ownership, errors, and capability boundaries without implementing SAM in i2pr;
- static guards prevent future dependencies on daemon/runtime/router internals from entering the reusable core unnoticed.

## 12. Milestone status summary

Plan 349 passed. ADR 0033 and the portable-core reference freeze the ownership/public-contract boundary; no internal crate split is required.

Plan 350 passed its package/API stabilization. Its closure correctly records the then-current license blocker and remains immutable. The owner has since selected MIT; Plan 379 owns current Cargo-license convergence and the remaining technical packageability audit.

Plan 351 passed. `bash scripts/check-portable-service-tunnel-consumer.sh` proves the public boundary from true external-consumer fixtures — the historical Plan 350 revision and, since Plan 379, a second fixture pinned to a post-Plan-359 revision — and the downstream SAM handoff assigns identity, filtering, lifecycle, and wire/runtime ownership to `dbowm91/i2pr-sam`.

Plan 359's one-edge amendment is on `main` and now has a conventional closure record, written additively by Plan 379; the record re-runs the amendment's mutation evidence rather than inheriting it. No SAM implementation or new user-visible tunnel product is authorized in i2pr by these registrations.

Plan 379 passed. Two findings changed the picture rather than confirming it. First, the
MIT license had never reached Cargo: `[workspace.package] license` existed but no member
manifest inherited it, so `cargo metadata` reported `license = null` for all 26
packages. All now inherit MIT, and `scripts/check-license-metadata.py` asserts both the
static manifest metadata and the resolved values, so the gap cannot reopen silently.

Second, MIT was **not** the only publication gate. `cargo package -p
i2pr-service-tunnels` fails because the Plan 359 `i2pr-proto` edge is path-only with no
`version` requirement. The posture is `git-consumable`, and
`crates.io-blocked-by-publishable-versioned-i2pr-proto` — a policy choice rather than an
unpackageable crate, since `i2pr-proto` itself packages and verifies cleanly today.
Clearing it is not authorized here and needs its own plan of record.

A second external-consumer fixture, pinned past the Plan 359 edge, proves that edge is
reachable from outside the workspace; the Plan 351 pin is retained unchanged as the
before/after comparison. Plan 359's closure record was written additively. The obsolete
`plans/349-351-portable-service-tunnels` branch was classified (10 of 11 commits
patch-equivalent on `main`, the 11th superseded, its one unique file a byte-identical ADR
duplicate) and deleted; its 409 generated `target/` artifacts were never on `main` and a
new guard asserts none is ever tracked. No SAM implementation or new user-visible tunnel
product is authorized in i2pr, and nothing was published.
