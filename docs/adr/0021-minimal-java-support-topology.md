# ADR 0021: Minimal sealed Java support topology for Plan 055

- Status: Proposed (Plan 055 Workstream D gate)
- Date: 2026-07-29
- Decision owners: repository maintainers

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

When the Java direct-helper investigation fails closed (Plan 055
C5), the harness may implement a minimal sealed support topology
with the following properties:

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

## Consequences

- The Java reference-initiated direction becomes qualifyable on a
  host that can run the sealed support topology. The harness must
  record the support topology digest in the trigger record and
  prove that no support-router traffic satisfies the direction.
- The Plan 052 diagnostic bundle remains a typed
  `diagnostic-complete-not-certificate` until Plan 056 produces two
  complete reproducible runs.
- The support topology is a fallback of last resort. Any future
  Java revision that exposes a usable direct seam must replace this
  fallback and bump the helper kind back to `java-direct-helper`.
- The Plan 055 D5 control experiments become mandatory for any
  qualification run that uses the support topology. A run that
  fails the support-router removal control is a typed blocker.
- NTCP2 remains experimental and non-advertised; Milestone 3 is
  still open.

## Rejected alternatives

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

- A future Java revision exposes a direct transport dial API;
  in that case Plan 056 (or its successor) must add a Java direct
  helper, source-lock it against the new pinned revision, and
  supersede this ADR.
- The Plan 046 rootless sealed-namespace gate becomes unavailable
  or is replaced by another isolated topology; this ADR must be
  re-issued against the new isolation contract.
- Plan 055 Workstream C source-inspection finds a direct seam in
  the pinned Java 2.12.0 revision; this ADR is deprecated.

## References

- `plans/055-reference-initiated-ntcp2-trigger-and-topology-qualification-pass.md`
- `tests/integration/ntcp2/reference-trigger-contracts.md`
- `tests/integration/ntcp2/reference-observation-catalog.toml`
- `docs/adr/0017-rootless-sealed-namespace-interop-evidence.md`