# Plan 292 — Exact TunnelManager option matrix and non-cryptographic runtime completion

Status: in-progress-prop170-option-matrix

Classification: capability + contract hardening.

Hard dependencies: Plans 290 and 291 closed.

## Objective

Recompute the complete Proposal 170 tunnel-type × option applicability matrix for i2pr and implement every dependency-ready non-cryptographic option with strict apply-or-reject semantics.

This is the pass that prevents "accepted and stored" from being mislabeled as TunnelManager support.

## Matrix authority

Generate a machine-readable matrix from the pinned Proposal contract and compare it against the project-owned Emissary full-support matrix as a secondary oracle. Do not copy Emissary dispositions blindly: i2pr owners and capabilities differ.

For every cell record exactly one disposition:
- apply: a named runtime/persistence owner consumes it;
- not_applicable: the Proposal does not apply it to the family;
- blocked_primitive: applicable but a required primitive is genuinely absent and assigned to Plan 293;
- invalid_alias/compatibility-only only if the pinned wire contract requires such a classification.

Tests must prove matrix cardinality and named blocked identities so no option silently disappears.

## Runtime categories in this plan

Implement all dependency-ready cells in categories such as:
- listener/target interface and port;
- target destination and I2P port;
- tunnel length/quantity/backup/variance where the existing destination/tunnel pool can truthfully project them;
- StartOnLoad and lifecycle flags;
- profile/interactive-vs-bulk mappings where they affect real streaming windows;
- idle reduce/close/new-destination behavior where real destination ownership can perform it;
- HTTP/SOCKS proxy authentication and bounded access controls;
- proxy lists, SSL proxy selection, jump-list/address-helper behavior only through I2P-routed policy;
- presentation UseSSL between the local application and tunnel endpoint where the Proposal field requires it, with explicit local TLS identity policy;
- HTTP unique-local-address-per-client and multihoming/reply-bundling where applicable;
- filter/access/rate/period options;
- client/server local bind and target policies;
- Streamr local UDP/port/subscription options.

Names above are categories; the exact wire keys and family applicability come only from the frozen machine-readable inventory.

## Tunnel shaping integration

i2pr DestinationConfig already has bounded inbound/outbound target counts and hop length. Extend it carefully rather than adding a control-only configuration object.

Requirements:
- Proposal bounds are enforced at the control boundary and again by internal typed constructors;
- a valid Proposal value must map to actual pool behavior;
- backup quantity/variance requires real pool semantics before marking apply;
- rebuild-required versus immediate changes are explicit and transactional;
- no control option may bypass global destination/tunnel resource ceilings;
- zero-hop is accepted only where the underlying i2pr destination policy explicitly permits it and the Proposal family allows it.

## Lifecycle/identity semantics

Options that can rotate or rebuild a destination must state:
- whether public destination identity remains stable;
- whether tunnel pools rebuild in place;
- whether active streams drain or reset;
- whether the change is allowed while running;
- how persistence rollback behaves.

Do not silently rotate a server destination because a tunable changed.

## Security

- secret values use redacted wrappers and are filtered from rawConfig/output;
- path-valued options are confined beneath owned roots or rejected;
- outproxy/proxy configuration may never open direct clearnet sockets unless the architecture explicitly authorizes an I2P-routed provider;
- TLS options do not imply interception/MITM capability;
- unsupported options fail before listener/destination/task allocation.

## Plan 293 boundary

At the end of Plan 292, the only allowed blocked_primitive cells are those requiring deep primitives in these classes:
- dynamic destination SigType;
- encrypted/blinded LeaseSet security and client authorization;
- UseOutproxyPlugin/provider semantics if no safe provider exists after the noncrypto audit.

Any other blocked cell requires either implementation in this plan or an explicit corrective plan justified by closure evidence; do not create a vague residual bucket.

## Evidence

- generated matrix completeness/cardinality checks;
- one positive and one negative test per apply-capability family;
- apply-or-reject allocation guards;
- live runtime snapshot showing each applied option changed its owner;
- restart persistence and rollback;
- running-edit rebuild/drain semantics;
- secret redaction;
- cross-family not_applicable rejection;
- no regression of Plans 289–291 lifecycle/data paths.

## Acceptance criteria

Plan 292 closes when every non-deep applicable option is consumed by a real owner and the machine-readable matrix leaves only the narrowly defined Plan 293 primitive residuals. Parser acceptance, round-trip storage, and rawConfig echo are never sufficient evidence.
