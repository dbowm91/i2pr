# ADR 0028: Proposal 170 as the parallel router control-plane contract

- Status: Accepted
- Date: 2026-10-02
- Decision owner: repository maintainer
- Related: ADR 0001, ADR 0002, ADR 0026, GUARDRAILS.md, specs/CONFORMANCE.md, M10 service tunnels, M12 floodfill, Proposal 170 Plans 286–295

## Context

Proposal 170, I2PControl Expansion, is Open and remains pinned at its 2026-05-20 revision. Its stated motivation is a unified router-management interface that can be shared across router implementations. The base I2PControl documentation continues to define JSON-RPC 2.0, API version 1 authentication, and token-bearing protected requests.

This matches i2pr's intended long-term shape: first construct a functional router, while exposing a standard administrative plane that future frontends can consume without being coupled to i2pr internals. The frontend is not part of this decision.

i2pr already has the required ownership substrate for much of Proposal 170: router identity and RouterInfo publication, NetDB, NTCP2/SSU2 runtime state, exploratory/client/transit tunnel owners, SAM, I2CP, and the M10 ServiceTunnelManager. It does not yet have an I2PControl server, a canonical address-book service, repliable datagrams for Streamr, or the complete Proposal 170 tunnel option surface.

The project also owns substantial Proposal 170 work in the eggstack/emissary fork. At research time the fork is GitHub parented to eepnet/emissary, fork master is 6885a945d25a5ae61bc68191d27c5816bc3df4c9, upstream master is 9b43484a21d5a1291c4881cdae62a36c527f8c0f, and the current upstream tree has no i2pcontrol paths. The repository maintainer states that the fork-specific Proposal 170 work was produced by this project and was not merged upstream. That provenance statement is the project authority for the narrow reuse exception below; it is not a general statement about unrelated Emissary code.

The Java Proposal 170 implementation remains an open pull request, i2p/i2p.plugins.i2pcontrol PR 6, currently headed by 45bb593000408071dd376b78848fdc246dccd964. i2pd provides the older/adopted I2PControl extensions used by several Proposal 170 selectors and ClientServicesInfo, but not the complete new Proposal 170 surface.

## Decision

### 1. Proposal 170 is a parallel subsystem

Proposal 170 / I2PControl is a separate subsystem roadmap and global-plan sequence. It is not inserted into the M12 dependency chain and does not gate continued router-mainline development.

The workstream may consume closed router capabilities from M7–M11 and later M12 capabilities when available. Mainline milestones never depend on the administrative API unless a later ADR explicitly changes that direction.

### 2. A dedicated runtime-neutral control-contract crate

Create a workspace crate named i2pr-i2pcontrol. It owns:

- bounded JSON-RPC request/response domain types and exact I2PControl error vocabulary;
- exact method names, RouterInfo selectors, ClientServicesInfo selectors, AddressBook operations, TunnelManager actions/types/options, and response shapes;
- protocol-level validation, bounds, secret classification, and conformance fixtures;
- no sockets, Tokio tasks, filesystem ownership, router state, transport internals, NetDB stores, tunnel pools, or service runtimes.

The crate is an administrative protocol adapter, not a frontend framework and not a second router core.

### 3. The daemon owns exposure and composition

i2pr-daemon owns the I2PControl listener, TLS, authentication token lifetime, failed-auth throttling, request budgets/deadlines, supervision, cancellation, and translation from typed Proposal 170 requests into narrow capabilities supplied by existing router owners.

The default is disabled and loopback-only. Remote exposure is fail-closed and requires explicit operator-owned TLS material. No plaintext fallback is authorized.

### 4. Existing router owners remain authoritative

Proposal 170 may inspect or command existing subsystems only through narrow snapshot/control handles. It must not gain unrestricted RouterContext-style access.

NetDB remains authoritative for peer/record state. Transport/runtime owners remain authoritative for active sessions and reachability. Destination/tunnel owners remain authoritative for tunnel pools. The M10 ServiceTunnelManager remains the only service-tunnel runtime. SAM and I2CP remain their own listener/session owners.

A missing source is unavailable; it is not represented by fabricated zero, false, empty, or adjacent data.

### 5. TunnelManager is an adapter over M10 plus bounded capability growth

Proposal 170 control-owned tunnel definitions use separate administrative persistence from startup TOML definitions. Startup-owned definitions may be inspected but are not silently rewritten by I2PControl. Name collisions and ownership changes fail explicitly.

The seven Proposal lifecycle actions operate through one ServiceTunnelManager composition. Missing Proposal tunnel families are implemented by composing/extending existing i2pr primitives. Streamr is the exception that requires a new bounded repliable-datagram substrate, but that substrate still lives under the existing destination/router ownership model.

### 6. AddressBook is a router service, not I2PControl shadow state

Create a runtime-neutral i2pr-addressbook subsystem with atomic/recoverable storage support. The same owner used by ordinary .i2p name resolution is exposed through Proposal 170. I2PControl never maintains a disconnected administrative copy.

Network subscription refresh tasks remain daemon-owned. Request-selected filesystem paths do not become unrestricted filesystem capabilities.

### 7. Narrow fork-specific reuse is authorized

The current blanket repository warning against copying router code is narrowed for this workstream only.

Fork-specific Proposal 170 / I2PControl source in eggstack/emissary may be reused after Plan 286 records an exact path/commit provenance manifest. Preserve applicable source notices and record modified/imported origin. Reuse protocol/domain/security/conformance code preferentially. Emissary's Yosemite-specific backends and unrelated upstream code are not authorized by this exception and must not be imported merely because they are adjacent.

This decision does not select a repository-wide i2pr license.

#### Amendment (Plan 329, 2026-10-04): cryptographic implementation is excluded from the reuse exception

The exception above covers Proposal 170 **administrative, control-plane, and protocol/domain** code
only. It does **not** cover the Emissary Red25519 or Encrypted LeaseSet2 cryptographic
implementation, and no such code may be copied, translated, transliterated, adapted, or used as
implementation text.

- Red25519/ELS2 code in i2pr is an independent implementation written from the normative I2P
  specifications over a reviewed curve library, with Java I2P and i2pd as readable
  ambiguity/interoperability references only.
- Emissary may be invoked only as a **post-implementation behavioral oracle**, and only after the
  i2pr implementation commit under test is frozen. Its Red25519/ELS2 source stays unread even
  then; a plan that needs to read it is out of bounds and must be re-registered.
- No fixture may be derived from reading Emissary internals.

Authority: Plan 329, the frozen clean-room record
[`specs/references/red25519-clean-room-freeze.md`](../../specs/references/red25519-clean-room-freeze.md)
and worksheet
[`specs/references/red25519-algorithm-worksheet.md`](../../specs/references/red25519-algorithm-worksheet.md).
This amendment narrows §7; it does not rewrite it, and it does not alter Plan 286's historical
classification of the fork paths that are unaffected.

### 8. Conformance is multidimensional

Every Proposal 170 capability is classified independently across at least:

- wire contract;
- truthful source;
- runtime effect;
- persistence/atomicity where mutating;
- disabled/default feature isolation;
- security/secret handling;
- evidence.

Parser acceptance or persisted inert options are not runtime support. Full support is claimed only when every applicable cell has a real source/effect or a protocol-permitted explicit neutral disposition.

### 9. Frontend design is explicitly deferred

No plan in this workstream adds a web UI, desktop UI, TUI, router console, frontend state model, or frontend-specific RPC extension. Future frontends consume the standard control plane.

## Rejected alternatives

- Put Proposal 170 inside i2pr-api: that crate is the SAM/I2CP application-protocol layer and would become an oversized mixed-purpose boundary.
- Port the Emissary i2pcontrol directory wholesale: this would duplicate service/destination ownership and import Yosemite-specific runtime assumptions.
- Implement TunnelManager as an independent tunnel stack: this creates split-brain lifecycle and secret ownership.
- Store AddressBook state only inside I2PControl: normal name resolution and administrative state would diverge.
- Make Proposal 170 a prerequisite for M12 or later router functionality: an administrative API must not block core router construction.
- Build the frontend concurrently: it would prematurely couple UI decisions to a still-open proposal.

## Review triggers

Revisit this ADR through a new accepted ADR if Proposal 170 changes materially; base I2PControl authentication/version semantics change; the project adopts a repository-wide license that requires different provenance handling; a future design proposes non-loopback exposure without explicit TLS/authentication; or a frontend requires a nonstandard control extension rather than consuming the standardized plane.
