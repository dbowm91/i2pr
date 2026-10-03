# ADR 0030: Destination linkability domains, service lifecycle separation, and i2pd Streaming convergence

- Status: Accepted
- Date: 2026-10-02
- Decision owner: repository maintainer
- Related: ADR 0029, M10 service tunnels, Destinations/Streaming, Plans 296–305
- Supersedes in part: ADR 0029 §5; narrows ADR 0029 §§3 and 6 for future anonymity work

## Context

Plans 296–305 corrected direct service-boundary branding leaks and then exposed two planning mistakes.

First, the prior anonymity plan treated a client Destination as though it must be isolated per remote target. That is stricter than I2P's normal service model and removes useful, intentional composition. A Destination is itself an application-level identity and linkability domain. A user may deliberately run several client tunnels through the same Destination, or expose several server services through one Destination on different I2P ports.

The exact-pinned i2pd 2.61.0 source supports this model. `ClientContext.cpp` has a shared local Destination; HTTP and SOCKS may use it; configured client tunnels may reuse one keys file; server tunnels may reuse a Destination and are distinguished by inbound I2P port. Sharing therefore belongs in the normal architecture, not outside the anonymity-qualified model.

The more important security boundary is between a Destination and the router that hosts it. A remote Destination may intentionally learn that two services share a Destination. It must not thereby learn the router identity, transport address, implementation release/build, local host state, or a stable router-specific service fingerprint.

Second, Plans 298/304 required a hostile packet-control interface inside all reference Streaming implementations. Stock Java I2P and i2pd do not expose the same pre-Streaming-manager test seam. Java and i2pd are already observably different at this layer, so requiring i2pr to resemble both risks creating a third profile. Future i2pr Streaming qualification should converge on one deployed implementation coherently.

Exact-pinned Java I2P 2.13.0 source also establishes useful lifecycle reference facts:
- `installer/resources/clients.config` configures I2PTunnel with `delay=-1`;
- `LoadClientAppsJob` interprets negative delay as waiting until the router reaches `RUNNING`, polling at one-second intervals;
- `TunnelPoolManager::buildTunnels` starts a newly-created client inbound pool first and schedules the outbound pool one second later;
- `TunnelPool` defines a ten-minute tunnel lifetime;
- `Router.shutdownGracefully()` documents a zero-to-eleven-minute graceful shutdown and derives remaining time from participating-tunnel expiration plus clock fudge.

These facts inform lifecycle design but do not prove that Java implements a dedicated service-uptime anonymity defense. i2pr will preserve the security goal without overstating reference intent.

## Decision

### 1. A Destination is an explicit linkability domain

A Destination identity is the unit remote applications may correlate. Multiple services may intentionally share one Destination.

Sharing is not an anonymity failure when it is explicit. It means exactly what the protocol implies: peers that observe the same Destination may correlate those services.

Dedicated service configuration remains useful as shorthand for an implicit one-service Destination group. It is not the only qualified configuration.

### 2. Destination groups replace target-level isolation as the default architecture

Future service composition uses a Destination-group owner.

A named group owns one Destination identity, its inbound/outbound client tunnel pools, LeaseSet lifecycle, publication/lookup context, delivery driver, resource accounting, and restart/drain state. Service definitions reference the group.

Client services of different kinds, including HTTP, SOCKS, IRC, and generic clients, may share a group deliberately. Server services may share a persistent group and use distinct I2P destination ports. Multiple instances of the same service kind are valid.

Resource ceilings remain mandatory. "Any number of tunnels" means no semantic one-per-kind restriction, not unbounded memory, keys, tunnels, or tasks.

### 3. Shared client/server groups are permitted but explicitly linkable

A client-only group may use an ephemeral identity. A group containing any server service must use a persistent identity so its published Destination remains stable.

If client and server services share that persistent group, client activity is intentionally linkable to the public server Destination. Configuration and local diagnostics must state this clearly. It is allowed because the operator explicitly chose the group.

An optional stronger target-isolated client mode may be added later, but it is not required for the default anonymity qualification.

### 4. Router identity remains outside every Destination group

Destination keys are generated and stored independently of router identity keys.

No application-facing or backend-facing bytes synthesized by a service tunnel may contain router identity, RouterInfo hash, router transport address, router implementation release/build identifier, local hostname/IP/path, or other router-local state unless the I2P protocol at that exact layer requires it.

LeaseSet lookup/publication and service traffic must use the Destination group's own tunnel context. Router-plane shortcuts that allow a remote service to infer the hosting RouterInfo are not qualified.

### 5. Server-port multiplexing is first-class

A persistent Destination group may expose multiple server services on distinct I2P ports. Duplicate inbound I2P ports within one group fail validation.

Backend targets may differ by service. The remote peer sees one Destination with multiple services, which is intentional and protocol-native.

### 6. Service lifecycle must not be mechanically tied to process lifetime

Service groups do not become network-visible merely because the router process started.

Activation waits for router operational readiness and usable Destination tunnel material. Initial pool construction may be staged where source-locked reference behavior justifies it.

Graceful router shutdown has a service-drain phase distinct from per-connection shutdown. During graceful drain, the router stops creating replacement service tunnels and stops refreshing service publication as appropriate, while already-published leases/tunnels may age out naturally. The network stack remains alive long enough to service that bounded drain.

The compatibility target is one normal tunnel lifetime, with a hard ceiling no greater than the Java reference's documented eleven-minute graceful window. Immediate/hard shutdown remains available.

Exact startup hold and drain defaults are fixed by implementation plans from source-locked reference behavior and deterministic lifecycle tests. They must not be guessed from a single constant.

### 7. Streaming fingerprint convergence targets i2pd

For Streaming observable-profile work, the compatibility authority is exact-pinned i2pd 2.61.0 at `635b013a612ff47278ef02acf8580a28e10e26c5`.

Java I2P remains useful background information but is not a pass/fail dependency for Streaming fingerprint convergence.

Measurement is directional. When measuring i2pd client behavior, i2pr owns the controlled remote Destination and observes incoming Streaming behavior. When measuring i2pd server behavior, i2pr owns the controlled probing client and observes i2pd responses. This avoids requiring an invasive packet-control API inside i2pd.

Source constants may define candidate dimensions, but production tuning still requires externally observable evidence wherever feasible.

### 8. HTTP qualification is separate from hostile Streaming qualification

HTTP proxy rewriting can be compared using ordinary controlled Destinations and does not depend on a raw hostile Streaming adapter.

Future HTTP differential work may compare Java and i2pd where both are straightforward to run. If their observable HTTP profiles diverge, i2pd is the preferred coherent target unless a common deployed behavior exists. The implementation must not create a third hybrid fingerprint.

### 9. Input sanitation and router-unlinkability are immediate gates

Before deeper profile convergence, every current service direction is audited for:
- router/version/build tokens;
- local alias/hostname/IP/path reflection;
- spoofable proxy-added metadata;
- header or line injection;
- control-character and delimiter smuggling;
- unbounded peer-controlled diagnostic reflection;
- accidental RouterInfo or transport metadata propagation.

These are direct security invariants, not optional profile-polish work.

## Consequences

The stopped Plans 297–305 remain historical evidence and are not rewritten.

The prior Plan 305 requirement that unrelated HTTP/SOCKS targets must receive distinct Destination identities is superseded for future work. Explicit group sharing is qualified behavior. The unresolved architecture defect found by Plan 305 remains valid in a different form: the production service path still lacks a true group-owned multi-hop Destination pool owner and currently constructs one-peer service build paths despite carrying a three-hop configuration.

Future Proposal 170 TunnelManager/http-server work must compose over Destination groups rather than recreate one Destination per service. HTTP server metadata injection must be opt-in and may identify the remote client Destination only; it must never identify the hosting router.

## Rejected alternatives

- Mandatory Destination-per-target isolation as the default: removes legitimate I2P composition and treats intentional application linkage as a router anonymity defect.
- One global shared Destination for every service: maximizes accidental linkability and makes operator intent ambiguous.
- Implicit sharing under capacity pressure: silently changes the anonymity domain and is forbidden.
- Matching Java and i2pd simultaneously at Streaming: likely creates a third implementation profile because the references already diverge.
- Randomizing every observable behavior: may create a unique fingerprint and does not replace joining a deployed profile.
- Immediate service activation and teardown tied directly to process start/exit: creates avoidable uptime correlation.

## Review triggers

Revisit this ADR if i2pd's deployed Streaming profile changes materially, Destination-group semantics prove incompatible with I2P protocol behavior, service lifecycle drain materially harms correctness or shutdown safety, a new server tunnel family adds identity-bearing backend metadata, or i2pr adopts a broader anonymity claim than scoped router/Destination unlinkability.
