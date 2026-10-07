# Plan 373 — Proposal 170 authority, support, and successor-state reconciliation: status

Status: **passed-authority-support-and-successor-state-reconciled-without-capability-promotion**

Plan of record:
[`373-prop170-authority-support-reconciliation.md`](../../implementation/i2pcontrol-proposal-170/373-prop170-authority-support-reconciliation.md).

Classification: planning/support corrective. **No production Rust or configuration behavior
change.** Zero `crates/*` files touched.

Implementation commits: this record and its edits land together as the Plan 373 change; the
commit id is recorded in `plans/registry.md` and the git history.

## What this plan was

Plan 373 is a truth-surface reconciliation, not an implementation. Its premise is stated plainly
in the plan: *the implementation moved faster than the summary surfaces, and this plan must not
promote capability merely because code exists*. It took the passed/stopped closure records as
inputs and rewrote the surfaces that had drifted behind them.

It was the hard dependency for Plans 374, 375, and 376, so its own acceptance criterion was
whether those three could be truthfully unblocked afterwards.

## Requirement-to-evidence matrix

| # | Requirement (plan §) | Status | Evidence |
|---|---|---|---|
| 1 | `plans/registry.md` recomputed from closure authority (§1) | pass | 9 named corrections landed; see below |
| 2 | `specs/support.toml` stale *reasons* corrected, posture unchanged (§2) | pass | 6 strings replaced; **0 metadata booleans changed** |
| 3 | `specs/CONFORMANCE.md` evidence vocabulary reconciled (§3) | pass | 4 vocabulary distinctions written explicitly |
| 4 | Both roadmaps reconciled, 374–378 registered as current authority (§4) | pass | 2 roadmaps, 5 edits, §17/§9 tables current |
| 5 | Ledger metadata corrected; prose matches guard implementation (§5) | pass | Proposal 170/352 closure state + guard-derivation prose |
| 6 | Both uniqueness guards run; 373–378 each have one owner | pass | both guards exit 0 |
| 7 | No production Rust/config behavior change | pass | diff touches `plans/` and `specs/` only |
| 8 | Machine-readable/support validation still parses | pass | `tomllib` parse, 88 surfaces |
| 9 | No support/advertisement boolean promoted | pass | `git diff -U0 specs/support.toml` over `advertised\|status\|scope\|protocol\|structure\|id` returns nothing |
| 10 | Stale phrases removed or marked historical | pass | 9-phrase sweep, 6 surfaces, all clean |

## The corrections, by class

### Registry (`plans/registry.md`)

Every one of these asserted something a passed closure had already contradicted:

- **"Plans 322 and 327 remain closed blocked on missing router owners."** Plan 322 passed: its
  43-addition gap census is zero after Plans 339/340. Corrected, including the mid-paragraph
  sentence "Plan 322 therefore remains blocked on the three **transit** selectors alone", which
  Plan 340 also closed.
- **"Plan 334 is blocked with its control plane complete but control-owned tunnels off the
  publication path entirely."** Proposal 170/334 was reclosed **passed** after Plans 337 (one
  shared `ServiceTunnelManager`, ADR 0031) and 338 (one identity-store owner + rollback reaching
  the manager). A control-created server does reach the publication path.
- **Plan 327 reclassified** from "blocked on missing production owners" to blocked on the narrower
  residual Plan 342 actually recorded — live failover and post-restart routing — with Plan 376
  named as the owner.
- **Plan 342 labelled a scoped pass, not a Plan-327 closure**, which is exactly what its own token
  (`passed-loopback-wire-lane-landed-live-failover-rotation-unproven`) says.
- **Plan 325 reclassified** from a live gate to a provider-survey record whose question Plans
  330/331 answered.
- **Plan 347 marked historical stopped**, with both of its recorded causes corrected forward.
- **Plan 348 marked blocked historical and superseded forward by Plan 378**, with its now-stale
  dependency model named: 347 stopped rather than being able to pass, and 342 is passed-*scoped*
  rather than passed.
- **Plans 326 and 335 rows** said "Plan 347 … is `ready`". 347 is stopped; Plans 374/375 own the
  live rows and Plan 377 converges them.
- **Every colliding historical number is now subsystem-qualified** on the surfaces this plan
  touched, including `ADR 0032 (Proposal 170)` where the bare number was ambiguous.

### `specs/support.toml`

Six stale *reasons* replaced; **posture deliberately untouched**:

- `common.leaseset2-family` claimed *"The type-5 transcript is i2pr-only … i2pd and Java I2P
  cannot verify it today"* and *"No floodfill serving is claimed."* Both false since Plans
  346/350/351. Now distinguishes five things the old single sentence conflated: strict
  Proposal-146 Red25519 qualification; deployed ELS2 type-11 qualification **at the crypto
  boundary**; local type-5 publication/store/serve/consume capability; missing **live
  cross-router** ELS2 evidence; and missing live outproxy failover/restart evidence.
- `prop170.destination-algorithm-policy` said *"Red25519 remains gated by blocked Plan 325."*
  The gate closed at Plans 330/331 and was superseded at Plan 346.
- `control.i2pcontrol-leaseset-modes` said the type-11 transcript is *"unverifiable by both"* and
  *"Plan 335 owns that lane."* Both halves stale: the transcript cross-verifies, and Plan 335 is
  a stopped historical record whose lanes moved to 374/375.
- M12 metadata said *"type5 deferred"* and *"Plan 281 deferred EncryptedLeaseSet type 5"* as a
  live gap, and `next_executable_plan` named **plan342, plan346, and plan345 as ready** when all
  three have since passed. It also carried `active_plan = plan284`, a staleness the registry had
  already corrected.

### `specs/CONFORMANCE.md`

Plan §3's four vocabulary distinctions are now written into the spec rather than left implicit:

1. **Crypto-boundary cross-verification is not ELS2 interoperability.** The old bullet said
   *"no `specs/support.toml` entry exists, no daemon configuration exposes it … the daemon still
   has nothing that publishes it"* — all false since Plans 334/350/351, and it buried the real
   limitation underneath four obsolete claims.
2. **Real ELS2 interoperability requires the whole chain in one row**: real `DatabaseStore` type 5
   → storage under the blinded key → real blinded-key `DatabaseLookup` → deployed-profile
   verification → authorization/decrypt → inner LS2 → streaming/application payload.
3. **Loopback outproxy evidence is not live failover/restart evidence.** The Plan 343 section
   still asserted *"no request path uses it"*; Plan 342 wired all four, but proved one loopback
   fixture, so the section header and its first bullet were both rewritten.
4. **A final Proposal-170 claim imports exact external artifacts rather than re-labeling local
   tests** — recorded where the 348 history and the 378 forward gate meet.

### Collision ledger

- Proposal 170/352's closure cell said **"not closed — `registered-preexisting-config-secret-leak-paths`"**.
  It has passed since `2416c30`. Both the red25519 roadmap and the ledger repeated the stale
  `ready`/`registered` state.
- The Rules section said the plan-number guard *"encodes only the finite, explicitly recorded plan
  collisions above"*. It does not: since the Plan 372 defect it **derives** its tolerated exact
  sets from this ledger. Prose corrected to describe the implementation, and the derivation plus
  the fail-closed unreadable-ledger behaviour recorded.

## Commands run (local)

No CI run is claimed for this plan: it changes no code, and every check it asserts is a
repository-consistency check that executes locally. Labelled local honestly.

```text
python3 scripts/check-global-plan-number-uniqueness.py   → exit 0
python3 scripts/check-adr-number-uniqueness.py           → exit 0
python3 -c "import tomllib; tomllib.load(open('specs/support.toml','rb'))" → parses, 88 surfaces
bash scripts/check-dependency-direction.sh               → dependency direction: ok
git status --porcelain (non plans//specs//docs/ paths)    → NONE
git diff -U0 specs/support.toml over advertised|status|scope|protocol|structure|id → NONE
9-phrase stale sweep over 6 surfaces                      → all clean
```

The `cargo` routine floor was **not** re-run for this plan: the diff contains no `.rs`, no
`Cargo.toml`, and no `Cargo.lock` line, so no compilation, lint, doc, or test input changed.
Running it would assert evidence for nothing this plan did. That is a deliberate scope decision
recorded here, not an omission — and the next plan that touches code runs it in full.

## Invariant review

- **Capability advertisement unchanged** — the plan's central constraint, verified mechanically
  above rather than asserted. `specs/support.toml` has 88 surfaces before and after with identical
  `advertised`/`status`/`scope` values; type 5 and full-Proposal-170 flags stay `false`.
- **No historical record rewritten.** Every closure record under
  `plans/closure/i2pcontrol-proposal-170/` is byte-unchanged. Forward corrections are written into
  the *summary* surfaces with an explicit "corrected by Plan 373" attribution, which is the shape
  this repo already uses for Plan 335. Plan 347's own text still records what it believed when it
  stopped, which is the point of a stopped record.
- **No renumbering.** No plan number, no ADR number, and no filename moved. The ledger's collision
  tables keep every historical entry; only a closure *state* cell was corrected.

## Findings by severity

- **critical / high: none.**
- **medium: none open.** One class was found and closed in-line: three separate surfaces (registry
  paragraph, registry per-plan rows, support metadata) each carried their own copy of the 347
  blocker diagnosis, so correcting one left the others contradicting it. All three were corrected
  together, which is why this plan has to touch all six files rather than one.
- **low (recorded, not fixed here).** `specs/support.toml`'s `next_executable_plan` field is a
  hand-maintained ready-set that this plan had to correct for the third time in this lineage
  (Plan 372's inventory-drift finding, Plan 367's count drift, this). The `i2pr-planning` skill
  forbids mirroring plan state outside `plans/`, and that field is exactly that: a duplicated
  ready-set that has now gone stale three times. **It is left in place and truthful, but closing
  the class needs its own plan-of-record** — deleting a machine-readable field is a support-schema
  decision, not a reconciliation this plan may make unilaterally.
- **low (recorded, not fixed here).** The registry repeats long closure narratives verbatim inside
  table cells, which is what let these claims drift without any checker noticing. The duplication
  is bounded and existing; making registry rows link-only is a structural change outside Plan 373's
  scope.

## Roadmap disposition and unblock audit

- **Roadmap disposition: closed.** Both roadmaps (§15–§17 of the Proposal 170 roadmap, §8–§9 of
  the Red25519 roadmap) and the registry now agree with closure authority. Plans 374–378 are
  registered as the current authority in both.
- **Unblock audit, executed per `plans/README.md`:** for each registered plan listing Plan 373 as a
  hard dependency, every *other* hard dependency was checked against its closure record.

| Plan | Other hard dependencies | All closed? | Disposition |
|---|---|---|---|
| 374 | Proposal 170/346 passed, /350 passed, /351 passed | yes | **unblocked → ready** |
| 375 | Proposal 170/346 passed, /350 passed, /351 passed | yes | **unblocked → ready** |
| 376 | Proposal 170/341 passed, /343 passed, /342 passed (scoped) | yes — and 342's own scope statement names exactly Plan 376's residuals | **unblocked → ready** |
| 377 | Plans 374, 375 | no — both were themselves blocked on 373 | stays blocked; **unblocked in the same commit by 374 and 375** |
| 378 | Plans 376, 377 | no | stays blocked; unblocked when 376 and 377 close |

No corrective pass is registered: this plan found no unclosed requirement and no code defect.

## Limitations

- Plan 373 proves the *summary* surfaces are truthful. It does not and cannot make the absent
  evidence exist: live cross-router ELS2 (374/375/377), live outproxy failover and restart (376),
  and the final integrated Proposal-170 gate (378) all remain open.
- Reconciliation was scoped to the surfaces this plan names. A doc-vs-source audit across all of
  `docs/architecture/` was not performed; if that audit finds further stale claims, it is a
  separate plan-of-record, not an extension of this one.
- Exact-head CI is not claimed, for the reason given under "Commands run".