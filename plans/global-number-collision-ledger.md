# Historical global number-collision ledger

Status: authoritative planning clarification; no historical plan, closure, or ADR was
renumbered or rewritten. This ledger covers both **plan numbers** (`plans/`) and
**ADR numbers** (`docs/adr/`), because both are load-bearing reference keys:
status tokens, supersession chains, closure cross-references, `specs/support.toml`
ADR lists, and source comments all cite them by number.

The convention was violated twice:

1. **Plans 296/297** — Proposal 170 and Anonymity each received 296 and 297.
2. **Plans 349–352 and ADRs 0032/0033** — discovered when six parallel branches were
   merged into `main`. The portable-service-tunnel, managed-native-app-runtime, and
   Proposal 170 lines were all extended past Plan 348 **in parallel**, each numbering
   from the same next-free global number.

All authorities remain intact under their original subsystem paths. None is an alias
or shared milestone of another.

## Plan-number collisions

| Qualified plan | Implementation authority | Closure authority |
|---|---|---|
| Proposal 170/296 | [`implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md`](implementation/i2pcontrol-proposal-170/296-tunnel-pool-shaping-and-bundling-residuals.md) | [`closure/i2pcontrol-proposal-170/296-status.md`](closure/i2pcontrol-proposal-170/296-status.md) |
| Proposal 170/297 | [`implementation/i2pcontrol-proposal-170/297-local-tls-identity-for-use-ssl.md`](implementation/i2pcontrol-proposal-170/297-local-tls-identity-for-use-ssl.md) | [`closure/i2pcontrol-proposal-170/297-status.md`](closure/i2pcontrol-proposal-170/297-status.md) |
| Proposal 170/349 | [`implementation/i2pcontrol-proposal-170/349-els2-consumer-lookup-path.md`](implementation/i2pcontrol-proposal-170/349-els2-consumer-lookup-path.md) | [`closure/i2pcontrol-proposal-170/349-status.md`](closure/i2pcontrol-proposal-170/349-status.md) |
| Proposal 170/350 | [`implementation/i2pcontrol-proposal-170/350-floodfill-type5-serve-path.md`](implementation/i2pcontrol-proposal-170/350-floodfill-type5-serve-path.md) | [`closure/i2pcontrol-proposal-170/350-status.md`](closure/i2pcontrol-proposal-170/350-status.md) |
| Proposal 170/351 | [`implementation/i2pcontrol-proposal-170/351-els2-consumer-service-wiring.md`](implementation/i2pcontrol-proposal-170/351-els2-consumer-service-wiring.md) | [`closure/i2pcontrol-proposal-170/351-status.md`](closure/i2pcontrol-proposal-170/351-status.md) |
| Proposal 170/352 | [`implementation/i2pcontrol-proposal-170/352-config-secret-hygiene.md`](implementation/i2pcontrol-proposal-170/352-config-secret-hygiene.md) | [`closure/i2pcontrol-proposal-170/352-status.md`](closure/i2pcontrol-proposal-170/352-status.md) (`passed-structural-toml-error-redaction-and-conditional-at-rest-mode-gate`) |
| Managed native app runtime/349 | [`implementation/managed-native-app-runtime/349-managed-app-v1-direction-broker-network-policy-corrective.md`](implementation/managed-native-app-runtime/349-managed-app-v1-direction-broker-network-policy-corrective.md) | [`closure/managed-native-app-runtime/349-status.md`](closure/managed-native-app-runtime/349-status.md) |
| Managed native app runtime/352 | [`implementation/managed-native-app-runtime/352-managed-app-mapped-ipv6-policy-canonicalization-corrective.md`](implementation/managed-native-app-runtime/352-managed-app-mapped-ipv6-policy-canonicalization-corrective.md) | [`closure/managed-native-app-runtime/352-status.md`](closure/managed-native-app-runtime/352-status.md) |
| Portable service-tunnels/349 | [`implementation/portable-service-tunnels/349-portable-service-tunnel-boundary-and-ownership-contract.md`](implementation/portable-service-tunnels/349-portable-service-tunnel-boundary-and-ownership-contract.md) | [`closure/portable-service-tunnels/349-status.md`](closure/portable-service-tunnels/349-status.md) |
| Portable service-tunnels/350 | [`implementation/portable-service-tunnels/350-service-tunnel-package-api-and-dependency-stabilization.md`](implementation/portable-service-tunnels/350-service-tunnel-package-api-and-dependency-stabilization.md) | [`closure/portable-service-tunnels/350-status.md`](closure/portable-service-tunnels/350-status.md) |
| Portable service-tunnels/351 | [`implementation/portable-service-tunnels/351-external-adapter-conformance-and-sam-handoff-contract.md`](implementation/portable-service-tunnels/351-external-adapter-conformance-and-sam-handoff-contract.md) | [`closure/portable-service-tunnels/351-status.md`](closure/portable-service-tunnels/351-status.md) |
| SAM/368 | [`implementation/sam/368-sam33-primary-subsession-shared-destination-profile.md`](implementation/sam/368-sam33-primary-subsession-shared-destination-profile.md) | [`closure/sam/368-status.md`](closure/sam/368-status.md) (`passed-sam33-primary-subsession-shared-destination-profile`) |
| Managed native app runtime/368 | [`implementation/managed-native-app-runtime/368-trusted-appmanager-bridge-and-manager-protocol-foundation.md`](implementation/managed-native-app-runtime/368-trusted-appmanager-bridge-and-manager-protocol-foundation.md) | [`closure/managed-native-app-runtime/368-status.md`](closure/managed-native-app-runtime/368-status.md) (`passed-trusted-appmanager-bridge-and-manager-protocol-foundation`) |
| Anonymity/296 | [`implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md`](implementation/anonymity/296-service-boundary-implementation-neutrality-and-leak-regression.md) | [`closure/anonymity/296-status.md`](closure/anonymity/296-status.md) |
| Anonymity/297 | [`implementation/anonymity/297-http-anonymity-profile-convergence-and-differential-qualification.md`](implementation/anonymity/297-http-anonymity-profile-convergence-and-differential-qualification.md) | [`closure/anonymity/297-status.md`](closure/anonymity/297-status.md) |

**Plan 349 has three independent owners.** The ELS2 consumer lookup path, the
portable service-tunnel boundary, and the managed native app v1 corrective are three
separate milestones that happen to share a number. Proposal 170/349 was superseded by
Proposal 170/351 (`superseded-by-plan351`); that supersession chain is **within**
Proposal 170 and is unaffected by the collision. Similarly
`superseded-by-plan350`-style chains elsewhere stay inside their owning subsystem.

**Plan 368 has two independent owners, recorded 2026-10-07.** The SAM 3.3
primary/subsession shared-destination profile and the managed native app runtime's
trusted AppManager bridge and manager protocol foundation are unrelated milestones
that were numbered from the same next-free global number by two parallel lines, and
the collision became observable only when both landed on `main`. The
managed-application line was integrated from
`plans/368-369-managed-app-runtime-foundation`; the SAM line was already on `main`.

Neither plan is renumbered and neither authority is rewritten or made an alias of the
other. Cite them subsystem-qualified — **SAM/368** and **Managed native app
runtime/368** — and read the owning subsystem's roadmap and registry rows for status.
The two are unrelated in scope, ownership, and evidence: SAM/368 concerns the SAM 3.3
wire profile and `specs/support.toml`, while Managed native app runtime/368 concerns
the private daemon-to-`AppManager` protocol, its bridge, and ADR 0035.

## ADR-number collisions

| ADR no. | Qualified ADR | Authority |
|---|---|---|
| 0030 | Destination linkability domains, service lifecycle, and i2pd Streaming | [`docs/adr/0030-destination-linkability-domains-service-lifecycle-and-i2pd-streaming.md`](../docs/adr/0030-destination-linkability-domains-service-lifecycle-and-i2pd-streaming.md) |
| 0030 | Loopback controlled floodfill reachability advertisement | [`docs/adr/0030-loopback-controlled-floodfill-reachability-advertisement.md`](../docs/adr/0030-loopback-controlled-floodfill-reachability-advertisement.md) |
| 0032 | Managed native app process and capability boundary | [`docs/adr/0032-managed-native-app-process-and-capability-boundary.md`](../docs/adr/0032-managed-native-app-process-and-capability-boundary.md) |
| 0032 | ELS2 type-11 signature-profile boundary | [`docs/adr/0032-els2-type11-signature-profile-boundary.md`](../docs/adr/0032-els2-type11-signature-profile-boundary.md) |
| 0033 | Portable service-tunnel policy core and adapters | [`docs/adr/0033-portable-service-tunnel-policy-core-and-adapters.md`](../docs/adr/0033-portable-service-tunnel-policy-core-and-adapters.md) |
| 0033 | ELS2 consumer lookup identity and install key | [`docs/adr/0033-els2-consumer-lookup-identity-and-install-key.md`](../docs/adr/0033-els2-consumer-lookup-identity-and-install-key.md) |

**ADR 0030 is pre-existing and explicitly out of scope.** Plan 353 §"ADR authority"
instructed that the older duplicate 0030 records be left untouched and the limitation
recorded rather than silently renumbered. That instruction is honoured here.

**The portable-service-tunnel ADR was renumbered once, deliberately.** It was authored
as ADR 0032 on the portable branch; Plan 353 §C moved it to `0033-portable-service-tunnel-policy-core-and-adapters.md`
because ADR 0032 was already taken by the managed-app process boundary. During the
merge of the portable branch, a second copy named `0032-portable-service-tunnel-policy-core-and-adapters.md`
was still present in that branch's tree. It was **byte-identical to the 0033 file apart
from the number in its title line** — one ADR double-filed under two numbers, not two
decisions — so the duplicate copy was dropped and the single 0033 authority was kept.
This is the one renumbering performed in this reconciliation, and it was performed
*before* the ADR reached `main`, which is why it did not require rewriting history.

## Rules

- Future prose discussing any colliding plan or ADR **must** include its subsystem
  qualifier (for example `Proposal 170/351` or `ADR 0032 (Proposal 170)`).
- New global implementation-plan numbers have exactly one owning subsystem. New ADR
  numbers must be checked against `docs/adr/` for a free number before filing.
- `scripts/check-global-plan-number-uniqueness.py` **derives** its tolerated exact sets
  from the plan-collision table above rather than maintaining a second hand-written
  allowlist. The table in this file is the single place a collision is declared, so a
  new collision cannot be recorded here and forgotten in the guard. The exact-set
  requirement is unchanged: a collision is tolerated only when the set of owners on disk
  equals the set declared above, so a *third* owner of a recorded number, or a renamed
  plan, still fails. Corrected by Plan 373; see
  [`closure/i2pcontrol-proposal-170/373-status.md`](closure/i2pcontrol-proposal-170/373-status.md).
  The guard also fails closed when this ledger cannot be read or has no
  `## Plan-number collisions` section, rather than degrading to an empty or permissive
  allowlist.
- **The ADR-number coverage gap is closed (Plan 361).**
  `scripts/check-adr-number-uniqueness.py` is now the ADR-side equivalent guard, run
  with `python3` alongside `scripts/check-global-plan-number-uniqueness.py`. It encodes
  **only** the three ADR-number collisions in the table above — 0030, 0032, and 0033 —
  as a literal set of exact filenames, cross-referenced to this ledger. A collision is
  tolerated only when the observed file set matches a recorded set **exactly**, so
  neither a third claimant nor a renamed file is grandfathered in. Adding an
  exemption requires editing that literal in the script, so it shows up in review;
  there is no glob and no silent skip. An ADR filename the guard cannot parse is an
  **error**, not a skip, and an exemption naming a file that no longer exists is
  reported as stale so it is retired deliberately instead of decaying into a dead
  skip. Nothing in this ledger changes the underlying fact that the three collisions
  exist: ADR 0030 remains ambiguous, and resolving what ADR 0029 meant is still an
  ADR-level decision, not a tooling fix. The guard enforces **identity only** — not
  ADR content, status tokens, or supersession chains.

## Scope notes on the two Proposal 170/296 and /297 entries

They closed the experimental `qualified-profile-closed` scope, including explicitly
recorded incompatibilities. They do not establish `canonical-wire` or
`full-proposal-conformant`. Plan 297's historical phrase "workstream is fully closed"
is preserved as written; this ledger and the later Proposal 170 roadmap/registry
authority scope that phrase to the qualified profile only.
