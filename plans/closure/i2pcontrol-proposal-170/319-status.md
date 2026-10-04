# Plan 319 status — Proposal 170 planning authority and global-number reconciliation

Status: **`passed-prop170-planning-authority-and-global-number-reconciliation`**.

Plan of record: [`plans/implementation/i2pcontrol-proposal-170/319-planning-authority-and-global-number-reconciliation.md`](../../implementation/i2pcontrol-proposal-170/319-planning-authority-and-global-number-reconciliation.md).

Implementation commit: `1eeac1f80b60d06db6081f00fce39cc52d422d2a` (`plan(319): reconcile Proposal 170 planning authority`).

## Requirement-to-evidence matrix

| Requirement | Evidence | Result |
|---|---|---|
| Preserve the historical 296/297 collisions without editing executed plans or closures | [`plans/global-number-collision-ledger.md`](../../global-number-collision-ledger.md) identifies Proposal 170/296, Proposal 170/297, Anonymity/296, and Anonymity/297 by exact implementation and closure paths. The historical files were not changed. | PASS |
| Reconcile duplicate and stale Proposal 170 registry authority | [`plans/registry.md`](../../registry.md) has one Proposal 170 roadmap row and one current-authority paragraph; duplicate 288/289 rows were removed. Historical profile, continuation, and dependency readiness are explicit. | PASS |
| Reconcile the Proposal 170 roadmap and continuation graph | [`plans/subsystems/i2pcontrol-proposal-170-roadmap.md`](../../subsystems/i2pcontrol-proposal-170-roadmap.md) scopes Plans 286–297 to the qualified profile, defines the claim vocabulary, retains the existing evidence table, marks 319 active at implementation time, and keeps the frontend out of scope. | PASS |
| Mechanically reject new cross-subsystem number collisions with finite exceptions | [`scripts/check-global-plan-number-uniqueness.py`](../../../scripts/check-global-plan-number-uniqueness.py) checks implementation owners and closure ownership; its only exceptions are the four exact historical implementation paths for 296/297. CI and the documented routine floor invoke it and its fixture suite. | PASS |
| Freeze accurate claim vocabulary without rewriting historical closure text | The roadmap and registry define `qualified-profile-closed`, `canonical-wire`, and `full-proposal-conformant`; the collision ledger scopes the preserved historical “fully closed” phrase to the qualified profile. | PASS |
| Avoid production, runtime, and support-inventory changes | Diff is planning authority, checker/test tooling, CI, and contributor documentation only. No production code, protocol specs/support entries, or dependency manifests changed. | PASS |

## Verification

All results below are local execution results on the implementation commit.

| Command | Result |
|---|---|
| `python3 scripts/check-global-plan-number-uniqueness.py` | PASS — live repository scan reported no unapproved cross-subsystem ownership. |
| `python3 -m unittest discover -s tests/planning -p 'test_*.py'` | PASS — 6 tests; unique number, cross-subsystem duplicate, same-owner closure, misplaced closure, exact historical exceptions, and fifth historical owner. |
| Inline Markdown local-link resolution over `plans/registry.md`, the Proposal 170 roadmap, and the collision ledger | PASS — no broken local links. |
| `git diff --check` | PASS. |

The workspace Rust floor and unrelated product acceptance lanes were not run: this plan changes no Rust or product behavior. CI now runs the number checker and fixture suite on future pushes.

## Migration, security, and limitations

- No historical plan number, filename, status token, commit, closure evidence, or support entry was changed.
- The allowlist is finite and path-exact. A third owner or an additional colliding implementation path fails closed and prints the paths. Closure records for a known owner are accepted only under that owning subsystem.
- The checker intentionally establishes planning uniqueness, not protocol capability or conformance.
- No dependencies, secret handling, runtime lifecycle, listener policy, or network behavior changed.
- Findings: critical 0, high 0, medium 0, low 0.

## Roadmap disposition and unblock audit

Plan 319 is closed as **`passed-prop170-planning-authority-and-global-number-reconciliation`**. The Proposal 170/Anonymity historical collision is preserved and disambiguated by subsystem-qualified identity. The frontend remains out of scope.

Audited every registered Proposal 170 continuation plan and its dependency graph. Plan 320 listed Plan 319 as its only hard dependency; that dependency is now closed and its canonical-wire scope has a stable written contract. Plan 320 is therefore moved from blocked to **ready** in this closure's status change. Plans 321–328 remain blocked on Plan 320 or its downstream prerequisites. No other registered plan lists Plan 319 as a dependency. M12/mainline readiness is unchanged.
