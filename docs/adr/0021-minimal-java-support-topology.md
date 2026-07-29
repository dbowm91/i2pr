# ADR 0021: Minimal sealed Java support topology for Plan 055

- Status: Rejected (Plan 058 record and candidate integrity closure pass)
- Date: 2026-07-29
- Decision owner: repository maintainer decision (Plan 058)
- Supersedes: 2026-07-29 Proposed draft (Plan 055 Workstream D gate)

## Context

Plan 055 Workstream C source-inspected the pinned Java I2P 2.12.0
revision (`2800040deee9bb376567b671ef2e9c34cf3e30b6`) and
documented its candidate outbound seams in
`tests/integration/ntcp2/reference-trigger-contracts.md`. The
source-locked call graph for `NTCPTransport.outboundMessageReady()`
(`router/java/src/net/i2p/router/transport/ntcp/NTCPTransport.java:373`)
requires:

- a fully-initialized `RouterContext` (constructed through
  `new NTCPTransport(RouterContext ctx, X25519KeyFactory xdh)` at
  line 153);
- a populated NetDB holding the target's `IdentHash`;
- a synthetic `OutNetMessage` staged through the outbound queue;
- and the `_conByIdent.put(ih, con)` precondition at line 395.

Plan 055 C5 explicitly rejects any helper that requires patching
cryptography, handshake state, frame encoding, or connection
acceptance. Plan 055 Workstream A rule 1 forbids patching transport
behavior. There is no public API surface in the pinned Java source
that accepts a single imported `RouterInfo` and dispatches a
transport-level outbound NTCP2 dial without the prerequisites above.
The Java direct-helper path therefore fails closed with the
typed decision
`java-direct-helper-rejected-global-context-not-isolatable`.

The Java reference-initiated direction (`java-to-i2pr-ipv4`) cannot
be qualified through a direct helper. The harness must either:

1. reject the direction as a typed blocker until an unrelated future
   revision exposes a usable direct seam, or
2. qualify the direction through a **minimal sealed support
   topology** that provides just enough I2P infrastructure for the
   pinned Java reference to import the i2pr `RouterInfo` and
   dispatch one transport-level outbound NTCP2 dial to it.

This ADR authorizes option 2 under the strict constraints of Plan
055 Workstream D.

## Decision

This ADR is **Rejected** by the repository maintainer decision
recorded in Plan 058. The repository does not implement the
minimal sealed Java support topology, and the four-direction
Milestone 3 contract cannot close with the current pinned Java
I2P 2.12.0 revision under the Plan 060 execution contract.

The decision is rejected because:

1. The Plan 046 host-side rootless probe returns
   `blocked_unprivileged_user_namespace` on this host, so the
   sealed support topology cannot be exercised on this host.
2. The Plan 060 execution contract requires two independent
   reproducible bundles from the same source commit. Exercising
   the support topology in a Multipass recovery guest would
   require a separate ADR-accepted topology implementation that
   this repository does not implement.
3. The Plan 058 record and candidate integrity invariant
   requires that active candidate records cannot reference
   missing required implementation artifacts. The Java support
   topology is a missing required implementation artifact for
   the `java-to-i2pr-ipv4` direction under the four-direction
   contract.
4. The pinned Java I2P 2.12.0 revision does not expose a
   transport-only direct seam, and the proposed support topology
   would require importing a synthetic `RouterInfo` and provisioning
   a support router in a sealed topology. The repository
   maintainer decision is to keep `java-to-i2pr-ipv4` blocked
   for the pinned Java revision rather than introduce a
   support topology that depends on extra moving parts.

## Consequences of rejection

- `java-to-i2pr-ipv4` remains a typed blocker for the pinned
  Java I2P 2.12.0 revision.
- The four-direction Milestone 3 contract cannot close against the
  pinned Java I2P 2.12.0 revision. A separate future plan must
  either choose a different pinned Java revision that exposes a
  transport-only direct seam, or revise the closure contract
  through a new ADR.
- Plan 059 must close with the typed blocker
  `blocked_java_support_topology_rejected` and must not start
  reference-side implementation work that depends on the
  support topology.
- Plan 060 must not start under the current four-direction
  contract.
- NTCP2 remains experimental and non-advertised; Milestone 3
  remains open.

## Reference prerequisites (preserved for audit)

The original proposed decision (Rejected) was:

> When the Java direct-helper investigation fails closed (Plan 055
> C5), the harness may implement a minimal sealed support topology
> with the following properties: ...

The proposed-decision properties are preserved verbatim below as
an audit record. The implementation notes that follow them remain
the original proposed notes; they were never implemented.

- **Roles.** One pinned Java reference router acting as the
  requested peer, plus one pinned support router that supplies
  the minimum NetDB visibility required for the pinned Java
  reference to dial the imported i2pr `RouterInfo`. No other
  roles are permitted.
- **Topology.** All routers run inside a single sealed rootless
  network namespace or an equivalently attested isolated topology
  (Plan 046 sealed namespace or Plan 048 Multipass guest).
- **No public egress.** No default external route, no DNS, no
  reseed URLs, no public floodfill access. Static `RouterInfo`
  exchange only via the disposable filesystem shared between
  the support router and the pinned Java reference.
- **Determinism.** Fixed synthetic addresses, fixed support-router
  identities, fixed NTCP2 ports. No `peer test`, no periodic
  floodfill refresh.
- **No support-router traffic toward i2pr.** The support router
  must not directly dial the i2pr `RouterInfo`; only the pinned
  Java reference may open an NTCP2 connection to the i2pr
  responder. Support-router connections to i2pr are
  diagnostic-only and cannot satisfy the Plan 052 directional
  predicate (Plan 055 D3).
- **Cleanup.** Every support process must be cleaned up by the
  harness. The parent network state must remain unchanged
  (Plan 055 D5).
- **Inventory.** The exact support-router count, identity, and
  configuration must be recorded in the per-direction trigger
  record (`trigger_record.source_inspection_record_sha256`).
- **Topology justification.** The Plan 055 D2 ADR gate is met
  by citing:
  - the pinned Java source's `outboundMessageReady` precondition;
  - the `_conByIdent.put(ih, con)` requirement on a fully
    populated NetDB;
  - the absence of any public API to dispatch a transport-only
    dial without NetDB context.

## Rejected alternatives (preserved from the original proposal)

- **Patching the Java router.** Plan 055 Workstream A rule 1 and
  rule 12 forbid cryptography/transport patches for qualification.
- **Using the SAM v3 streaming seam.** SAM requires a registered
  destination and outbound tunnel pool, neither of which is
  permitted by Plan 055 Workstream D3.
- **A larger support topology (floodfill cluster, exploratory
  tunnels).** Plan 055 D2 mandates the smallest candidate
  topology that meets the source prerequisites. The support-router
  inventory above is the minimum required to satisfy the pinned
  Java outbound path.
- **Skipping the direction.** A typed blocker is permitted until
  qualification passes; silently skipping a primary direction
  invalidates the four-direction bundle.

## Review triggers

- A future Java revision exposes a direct transport dial API. In
  that case a new ADR must accept that future topology and supersede
  this rejection. The rejection does not block a future pinned
  revision from being approved; it documents the decision against
  Java I2P 2.12.0.
- The Plan 046 rootless sealed-namespace gate becomes unavailable
  or is replaced by another isolated topology. The rejection of
  this ADR is independent of the gate; the rejection is recorded
  because the support topology introduces additional moving parts
  without a clear net benefit on this host.

## References

- `plans/055-reference-initiated-ntcp2-trigger-and-topology-qualification-pass.md`
- `plans/058-plan056-record-and-candidate-integrity-closure-pass.md`
- `tests/integration/ntcp2/reference-trigger-contracts.md`
- `tests/integration/ntcp2/reference-observation-catalog.toml`
- `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md`