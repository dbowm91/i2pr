# Historical global plan-number collision ledger

Status: authoritative planning clarification; no historical plan or closure was
renumbered or rewritten.

The global `NNN` convention was violated when Proposal 170 and Anonymity each
received Plans 296 and 297. All four executed authorities remain intact under
their original subsystem paths. These are distinct subsystem-qualified plans:

| Qualified plan | Implementation authority | Closure authority |
|---|---|---|
| Proposal 170/296 | [`plans/implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md`](implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md) | [`plans/closure/i2pcontrol-proposal-170/296-status.md`](closure/i2pcontrol-proposal-170/296-status.md) |
| Proposal 170/297 | [`plans/implementation/i2pcontrol-proposal-170/297-local-tls-identity-for-use-ssl.md`](implementation/i2pcontrol-proposal-170/297-local-tls-identity-for-use-ssl.md) | [`plans/closure/i2pcontrol-proposal-170/297-status.md`](closure/i2pcontrol-proposal-170/297-status.md) |
| Anonymity/296 | [`plans/implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md`](implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md) | [`plans/closure/anonymity/296-status.md`](closure/anonymity/296-status.md) |
| Anonymity/297 | [`plans/implementation/anonymity/297-http-anonymity-profile-convergence-and-differential-qualification.md`](implementation/anonymity/297-http-anonymity-profile-convergence-and-differential-qualification.md) | [`plans/closure/anonymity/297-status.md`](closure/anonymity/297-status.md) |

The collision is historical and finite. The two Proposal 170 entries closed the
experimental `qualified-profile-closed` scope, including explicitly recorded
incompatibilities. They do not establish `canonical-wire` or
`full-proposal-conformant`. Plan 297's historical phrase “workstream is fully
closed” is preserved as written; this ledger and the later Proposal 170
roadmap/registry authority scope that phrase to the qualified profile only.

Future prose discussing any of these four plans must include its subsystem
qualifier. New global implementation-plan numbers have exactly one owning
subsystem. `scripts/check-global-plan-number-uniqueness.py` encodes the only
permitted cross-subsystem collisions as these four exact implementation paths;
it does not grandfather additional owners or paths.
