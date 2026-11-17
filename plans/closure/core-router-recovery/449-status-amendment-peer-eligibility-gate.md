# Plan 449 status amendment — the live column is now carried by the Plan 476 executor

**Status token:** `passed-portable-preflight-infrastructure-only` (**unchanged**).
This amendment does **not** advance Plan 449's token and does **not** close its live
column. It records that Plan 495's peer-eligibility gate is now bound into that
column's machinery, and states precisely what remains.

**Amends:** [`449-status.md`](449-status.md), which remains unchanged and remains
the historical account of the infrastructure pass.
**Implementation commits:**
- `7968d81f` — *feat(qualification): land the Plan 476 guarded per-host receipt executor*;
- `ad1326f7` — *fix(qualification): close the defects an independent review of Plan 476 found*;
- `7e055e09` — *fix(daemon): collapse the receipt export tick's let-chain for clippy*.
**Plan:** [`449-portable-ssu2-independent-lan-qualification-lane.md`](../../implementation/core-router-recovery/449-portable-ssu2-independent-lan-qualification-lane.md).

## The two columns, restated

Plan 449's closure record already separated the two columns, and this amendment
keeps them separate rather than merging them into one pass.

| Column | State | What would close it |
| --- | --- | --- |
| **Portable preflight infrastructure** | **Passed** — unchanged since `deb1060` | already closed |
| **Live two-host qualification** | **Not run** — environment-blocked | an owner-authorized two-host private LAN, an operator run of the Plan 476 executor on both hosts, and the owner-exchanged reconciliation |

Nothing in this work changes the second row. The second row was not run before
and it is not run now.

## What changed

Plan 495 amended this plan's WP4, WP5 and WP6 so that the live column must run
the **peer-eligibility preflight** before any process launch, must record the
**claim-tier ladder**, and must classify every outcome as exactly one outcome
class **with its cause** — and must never report a peer-side refusal as an
`i2pr` defect. Until now that amendment had machinery only in the sense that a
checker existed.

Plan 476's Tier A pass delivered the missing half:

| Piece | Owner | State |
| --- | --- | --- |
| The canonical gate model and its offline enforcement | Plan 495 | delivered (`specs/QUALIFICATION-GATES.md`, `scripts/plan495-peer-eligibility.py`) |
| The guarded per-host executor that runs the gate before it starts a router | Plan 476 | **delivered** (`tests/integration/ssu2/plan476_live_executor.py`, commit `7968d81f`) |
| The daemon's sanitized local receipt export the executor reads | Plan 476 | **delivered** (`crates/i2pr-daemon/src/ssu2_qualification_export.rs`, `[ssu2] qualification_receipt_path`) |
| The machine enforcement that keeps the executor bound to the gate | Plan 476 | **delivered** (`scripts/check-plan476-gate-binding.py`) |
| The operator procedure | Plan 476 | **delivered** (`docs/operations/ssu2-private-lan-qualification.md` §9) |
| **An authorized two-host private LAN** | **the operator** | **absent on this host** |
| **The live run itself** | **the operator** | **not performed** |

The plan's own text now binds to real machinery rather than to a promise: the
live column's procedure is the executor's, the executor evaluates the pinned
peer's admission and selection predicates before it starts anything, and the
resulting evidence carries the tier ladder and the outcome taxonomy.

## The live column's remaining scope, exactly

When an owner supplies an authorized two-host pair, the remaining work is
bounded and has no open source-side component:

1. The owner supplies the Plan 449 inventory (owner consent, the two RFC1918
   endpoints, the pinned i2pd source and binary) **outside** the repository, with
   the same `--epoch` nonce on both hosts.
2. On the i2pr host: the operator runs the executor with `--role i2pr`, which
   starts the daemon under the `private_lan_qualification` profile, lets the
   daemon's sanitized receipt export accumulate the local observation, and emits
   that host's receipt.
3. On the stock i2pd host: the operator runs the executor with `--role i2pd`,
   which starts the unmodified pinned i2pd and records the peer-side observation
   from i2pd's own local state.
4. The owner exchanges the two host receipts manually and runs `reconcile`, which
   produces the Plan 495 lane evidence: the T0–T8 ladder, the outcome class, the
   cause and the attempts consumed.

There is no remaining implementation work in this column. What is missing is an
environment and an operator decision.

## Evidence

| Requirement | Evidence | Result |
| --- | --- | --- |
| The live column runs the peer-eligibility preflight before any process launch | `execute_run` step 2 in `tests/integration/ssu2/plan476_live_executor.py` runs the Plan 495 gate before the only spawn site; the executor's negative controls assert that a gate refusal starts no process | Passed at source scope |
| The live column records the claim-tier ladder | `build_tiers` emits all nine Plan 495 tiers with their reasons; the reconciled evidence is validated by the Plan 495 checker and is not written if the checker rejects it | Passed at source scope |
| The live column classifies every outcome with its cause | the gate's own outcome class and cause are carried through; `i2pr-defect` is never emitted by the executor | Passed at source scope |
| A peer-side refusal is never reported as an `i2pr` defect | the executor has no code path that emits `i2pr-defect`; an unattributable lane is `timeout`/`unattributed` | Passed at source scope |
| The gate binding is enforced, not asserted | `scripts/check-plan476-gate-binding.py` compares the Rust and Python phase vocabularies element by element, resolves which file the executor's gate was loaded from, requires the gate to be evaluated before the spawn in source order, forbids the executor from carrying an `i2pr-defect` literal, and requires the simulator to be unable to connect | Passed; in the routine floor and in CI |
| The bound machinery survives an adversarial review | an independent verification pass against the plan of record found three high-severity defects in the executor — a peer-side refusal published as a lane `pass`, the live lane terminating the router before a session, and no child reap on an error — and all were fixed before this amendment was written (`plans/closure/core-router-recovery/476-status-amendment-tier-a-guarded-process-executor.md`) | Passed |
| The preflight stays non-emitting and never starts a router | `scripts/plan449-ssu2-preflight.py` is unchanged and still starts no process and sends no packet | Passed; unchanged |
| The two-column separation survives | this record's two-column table, and `specs/CONFORMANCE.md`'s binding section, which states that no live two-host SSU2 result may be claimed | Passed |
| Authenticated independently addressed SSU2 in both directions, I2NP delivery, restart/withdrawal, negative controls | no authorized host pair exists on this host; no live command was run; no packet was sent | **Not run** |

## Security, compatibility and findings

- **Medium — the live topology remains unavailable.** Unchanged from the base
  record, and now with a smaller gap: the executor exists, so the blocker is the
  environment and the operator's authorization, not a missing implementation.
- The executor starts only the declared local router, signals only processes it
  started, and never reaches the peer host over the network. It does not SSH, does
  not discover a peer address, does not fetch a reseed, and never touches the
  public Internet.
- The receipt exchange between the two hosts is manual and owner-controlled, and
  the receipts carry digests and phase counts only — no key, no RouterInfo, no
  address, no endpoint, no raw log.
- No dependency was added. `specs/support.toml` is unchanged: 94 surfaces, 88
  `experimental`, 5 `deferred`, 1 `ready`, 0 advertised. No public `R`/`f`
  claim, no support promotion, and no reachability or transit claim follows from
  this work.

## Disposition

Plan 449's status token is **unchanged**: `passed-portable-preflight-infrastructure-only`.
Its live column remains environment-blocked, and this amendment does not close
it. Plan 431 remains stopped; Plan 433 does not become ready; Plan 493 remains
blocked on the authorized two-host topology and on Plan 435. **No plan changed
state.**

The result is still **not** public SSU2 qualification, not a two-family
conformance result, and not evidence for normal public router participation.
