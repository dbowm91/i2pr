# Plan 377 — ELS2 external evidence convergence and successor closure: status

Status: **blocked-plans-374-and-375-not-passed-no-convergence-possible**

Plan of record:
[`377-els2-external-evidence-convergence.md`](../../implementation/i2pcontrol-proposal-170/377-els2-external-evidence-convergence.md).

Classification: external-evidence convergence + support-claim gate.

This plan **did not pass**, and it cannot: it is a convergence plan, and both of
its inputs do not exist. Converging zero of four directions is not a partial
result, it is no result.

## Why it cannot run

The plan's four-direction matrix requires real router-to-router traffic:

| Publisher | Consumer | Status |
|---|---|---|
| i2pr | i2pd | **no executed row** — Plan 374 blocked |
| i2pd | i2pr | **no executed row** — Plan 374 blocked |
| i2pr | Java I2P | **no executed row** — Plan 375 blocked |
| Java I2P | i2pr | **no executed row** — Plan 375 blocked |

Each row requires the whole chain in one line: real `DatabaseStore` type 5,
storage under the blinded key, a real blinded-key `DatabaseLookup`, outer
deployed-profile verification, authorization/decrypt, inner LS2 validation, and
a streaming/application payload. The plan states that no crypto-only row can
satisfy the table, and that is correct: Plan 346's crypto-boundary
cross-verification is a signature comparison over controlled inputs. It touches
no router, no NetDB, and no tunnel, and it is not a substitute.

Plan 346, /350 and /351 are passed and were importable inputs to this plan. They
import cleanly and say what they say. They are simply not enough, and the plan
says so itself.

## What was executed anyway

The convergence plan has nothing to converge, so the useful work was verifying
that the parts which *can* be checked independently are sound. They are:

- the **Plan 378 §1 re-freeze** of the current Open Proposal 170, executed here
  because it gates the whole external vocabulary and depends on nothing external;
- a **local-gate sweep** of every Proposal 170 checker, all green.

The re-freeze result and the gate sweep are recorded in
[`378-status.md`](378-status.md) rather than duplicated here, since Plan 378 is
where they are consumed.

## Findings by severity

- **critical / high: none.** No defect was found; no product code was changed.
- **low (recorded, not fixed here)**: Plan 377's registered dependency list names
  "Plan 373 passed" alongside 374 and 375. That is harmless but now slightly
  stale in spirit — 373 passed, and the two that matter did not. Recording the
  distinction is the whole point of Plan 373's reconciliation work, so it is
  restated here rather than edited into the plan.

## What is explicitly **not** claimed

- No ELS2 interoperability claim, and no `full-proposal-conformant` support
  token. Plan 377 is the plan that would let 378 consider that token, and it
  cannot.
- Type 5 remains `advertised = false`. `specs/support.toml` is unchanged by this
  record.
- No auth-mode matrix is produced; the single converged matrix the plan asks for
  cannot exist without the four rows.

## Roadmap disposition and unblock audit

- **Roadmap disposition: blocked**, on its two hard dependencies. Plan 377 is
  purely downstream: it adds no protocol features and discovers no code defects
  of its own, so nothing else in this subsystem waits on it except Plan 378.
- **Unblock audit, executed per `plans/README.md`:**

| Plan | Other hard dependencies | All closed? | Disposition |
|---|---|---|---|
| 378 | Plan 376 passed; Plan 377 **not passed** | no | stays blocked on 377 alone |

Plan 378 has now discharged everything it can: its §1 re-freeze is clean and its
local gates are green. It is blocked on exactly one thing — this plan — which is
blocked on exactly two things — the unwritten ELS2 drivers in Plans 374 and 375.

No corrective pass is registered: no defect was found.

## Limitations

- This record asserts nothing about the behaviour of either reference router
  under real ELS2 traffic, because none was exercised.
- The reference freeze that makes the next pass cheaper lives in
  [`tests/integration/els2/reference-freeze.md`](../../../tests/integration/els2/reference-freeze.md).