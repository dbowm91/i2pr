# Portable Service-Tunnel Core Roadmap

Status: active — Plans 349–351 registered as a new parallel portability/reuse work line. This workstream does not reopen M10 product closure and does not implement a SAM client, SAM daemon, Python binding, C ABI, tunnel WebUI, or application sidecar in i2pr.

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
- The package is currently `publish = false`.
- Its Cargo manifest declares a path dependency on `i2pr-proto`; registration-time source review found no direct production-code use of `i2pr_proto` in the top-level modules inspected, so Plan 350 must prove whether that dependency is dead before removing it.
- The repository explicitly has no selected repository-wide license. Public package publication therefore cannot be enabled merely as a mechanical Cargo change.
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

Plan 349 is dependency-ready. Plans 350 and 351 are registered but blocked on their predecessor's closure so API/package work does not outrun the ownership contract.

## 7. Milestones

| Plan | State | i2pr token | Classification | Implementation | Closure |
|---|---|---|---|---|---|
| 349 | ready | `registered-portable-service-tunnel-boundary` | invariant + infrastructure | `plans/implementation/portable-service-tunnels/349-portable-service-tunnel-boundary-and-ownership-contract.md` | future `plans/closure/portable-service-tunnels/349-status.md` |
| 350 | blocked | `registered-portable-service-tunnel-package-api-stabilization-blocked-on-349` | infrastructure + polish | `plans/implementation/portable-service-tunnels/350-service-tunnel-package-api-and-dependency-stabilization.md` | future `plans/closure/portable-service-tunnels/350-status.md` |
| 351 | blocked | `registered-portable-service-tunnel-external-adapter-conformance-blocked-on-350` | infrastructure | `plans/implementation/portable-service-tunnels/351-external-adapter-conformance-and-sam-handoff-contract.md` | future `plans/closure/portable-service-tunnels/351-status.md` |

## 8. Cross-cutting requirements

- M10's current product semantics remain unchanged unless an explicit corrective with regression evidence proves a defect.
- `i2pr-service-tunnels` remains runtime-neutral and `#![forbid(unsafe_code)]`.
- No public API may require `i2pr-daemon`, `i2pr-runtime`, live NetDB stores, tunnel pools, transport managers, or router-global context.
- External consumers must receive the same bounded parsing/filtering/access behavior as i2pr's native composition.
- Destination/linkability policy must remain explicit; adapters may not infer sharing from tunnel type or capacity.
- Peer-based server filters operate on authenticated I2P Destination identity/hash supplied by the adapter. Unauthenticated metadata may not be promoted to authenticated peer identity.
- HTTP privacy/server filters, SOCKS/IRC policy, and access/rate limits must remain transport-independent.
- No external implementation source is copied into this work line. Specifications and independently observed behavior remain valid references.
- Package publication cannot be enabled until licensing is explicitly selected by the repository owner and represented in Cargo/repository metadata.
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
- enabling package publication despite the repository's explicit no-license state;
- over-coupling the core to one async runtime for adapter convenience;
- conflating the future SAM library's clean-room protocol implementation with reuse of i2pr-owned service-tunnel policy.

If API stabilization reveals a genuine split between reusable profile policy and i2pr-only orchestration, Plan 349 must record that boundary before Plan 350 moves files/crates. Do not perform an opportunistic repository split without the ADR.

## 11. Completion definition

This workstream is complete when:
- a durable ADR/spec states exactly which service-tunnel semantics i2pr owns for reuse and which runtime concerns remain adapter-owned;
- `i2pr-service-tunnels` has no accidental/dead router-protocol dependency and no forbidden runtime ownership;
- the intended public Rust surface is documented and semver-reviewed;
- package metadata is externally consumable, with actual publication gated truthfully on explicit licensing;
- i2pr's existing service-tunnel consumers remain behaviorally unchanged;
- an out-of-workspace consumer fixture can use generic/server/client, access, destination/linkability, and representative HTTP/SOCKS/IRC filtering solely through public APIs;
- the downstream SAM handoff documents identity/session mapping, authenticated peer requirements, lifecycle ownership, errors, and capability boundaries without implementing SAM in i2pr;
- static guards prevent future dependencies on daemon/runtime/router internals from entering the reusable core unnoticed.

## 12. Milestone status summary

Plan 349 is ready. It freezes the ownership/public-contract boundary and decides whether any internal split is actually required.

Plan 350 is registered but blocked on Plan 349. It performs package/dependency/API stabilization and must treat licensing as an explicit publication gate, not an assumption.

Plan 351 is registered but blocked on Plan 350. It proves the public boundary from a true external-consumer fixture and produces the clean downstream handoff for a future independent SAM library/tunnel-manager repository.

No SAM implementation or new user-visible tunnel product is authorized by these registrations.
