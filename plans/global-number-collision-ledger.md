# Historical global plan-number collision ledger

Status: authoritative planning clarification; no historical plan or closure was
renumbered or rewritten. The Plan 349 collision records a parallel-branch
numbering conflict discovered when the managed-runtime corrective branch was
rebased onto the already-closed portable-service-tunnel work. Both plan
authorities remain subsystem-qualified; future global numbers remain unique.

The global `NNN` convention was violated when Proposal 170 and Anonymity each
received Plans 296 and 297. All four executed authorities remain intact under
their original subsystem paths. These are distinct subsystem-qualified plans:

| Qualified plan | Implementation authority | Closure authority |
|---|---|---|
| Managed native app runtime/349 | [`plans/implementation/managed-native-app-runtime/349-managed-app-v1-direction-broker-network-policy-corrective.md`](implementation/managed-native-app-runtime/349-managed-app-v1-direction-broker-network-policy-corrective.md) | [`plans/closure/managed-native-app-runtime/349-status.md`](closure/managed-native-app-runtime/349-status.md) |
| Portable service-tunnels/349 | [`plans/implementation/portable-service-tunnels/349-portable-service-tunnel-boundary-and-ownership-contract.md`](implementation/portable-service-tunnels/349-portable-service-tunnel-boundary-and-ownership-contract.md) | [`plans/closure/portable-service-tunnels/349-status.md`](closure/portable-service-tunnels/349-status.md) |
| Proposal 170/296 | [`plans/implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md`](implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md) | [`plans/closure/i2pcontrol-proposal-170/296-status.md`](closure/i2pcontrol-proposal-170/296-status.md) |
| Proposal 170/297 | [`plans/implementation/i2pcontrol-proposal-170/297-local-tls-identity-for-use-ssl.md`](implementation/i2pcontrol-proposal-170/297-local-tls-identity-for-use-ssl.md) | [`plans/closure/i2pcontrol-proposal-170/297-status.md`](closure/i2pcontrol-proposal-170/297-status.md) |
| Anonymity/296 | [`plans/implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md`](implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md) | [`plans/closure/anonymity/296-status.md`](closure/anonymity/296-status.md) |
| Anonymity/297 | [`plans/implementation/anonymity/297-http-anonymity-profile-convergence-and-differential-qualification.md`](implementation/anonymity/297-http-anonymity-profile-convergence-and-differential-qualification.md) | [`plans/closure/anonymity/297-status.md`](closure/anonymity/297-status.md) |

The managed-runtime/349 and portable-service-tunnels/349 collision is a
preserved historical identifier conflict: the portable line was closed and its
Plan 349 authority predates integration with the independent managed-runtime
Plan 349 corrective. It is not an alias or shared milestone. The two Proposal
170 entries closed the
experimental `qualified-profile-closed` scope, including explicitly recorded
incompatibilities. They do not establish `canonical-wire` or
`full-proposal-conformant`. Plan 297's historical phrase “workstream is fully
closed” is preserved as written; this ledger and the later Proposal 170
roadmap/registry authority scope that phrase to the qualified profile only.

Future prose discussing a colliding plan must include its subsystem qualifier.
New global implementation-plan numbers have exactly one owning subsystem.
`scripts/check-global-plan-number-uniqueness.py` encodes only the finite,
explicitly recorded collisions as exact implementation paths; it does not
grandfather additional owners or paths.
