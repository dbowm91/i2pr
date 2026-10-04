# Plan 319 — Proposal 170 planning-authority and global-number reconciliation

Status: **passed-prop170-planning-authority-and-global-number-reconciliation** — see [`plans/closure/i2pcontrol-proposal-170/319-status.md`](../../closure/i2pcontrol-proposal-170/319-status.md).

Classification: planning invariant + tooling corrective. No protocol/runtime behavior changes are authorized.

Hard dependencies: none. This plan corrects the present planning authority and is the first dependency of the full-conformance continuation.

## Objective

Repair the planning system after the completed 286–297 qualified-profile sequence exposed two authority defects: global plan numbers 296 and 297 were independently reused by the Proposal 170 and anonymity workstreams, and the active registry/roadmap now contain duplicate and contradictory Proposal 170 state.

Do not renumber, move, rewrite, or relabel executed historical plans or closure records. Preserve Git history and closure truth. Reconcile future authority around the collision instead.

## Frozen facts entering the plan

- Proposal 170 Plans 286–297 have executed closure records.
- Anonymity Plans 296–297 also have executed/historical records.
- plans/README.md defines plan numbering as global because status tokens and cross-references depend on it.
- The Proposal 170 registry currently contains duplicate detailed rows and labels the workstream active and fully closed simultaneously.
- The Proposal 170 roadmap says the qualified 286–297 workstream is complete but also retains stale dependency-gated language.
- Research after Plan 297 established that the implemented public contract is a qualified i2pr profile, not the exact Proposal 170 wire contract. Plans 320–328 own that continuation.

## Required work

### 1. Historical collision ledger

Create one durable planning document recording the 296/297 collision. It must identify all four implementation/closure authorities by subsystem-qualified identity, e.g. Prop170/296 versus Anonymity/296, without inventing replacement historical numbers.

Future prose that would be ambiguous must use a subsystem qualifier for these four historical plans.

The ledger is a planning repair only. Existing status tokens, filenames, commits, closure evidence, and support entries remain immutable.

### 2. Registry reconciliation

Reduce the Proposal 170 section of plans/registry.md to one active-roadmap row, one current-authority paragraph, and one row per genuinely current or historically important plan. Remove duplicate 286–291 rows and stale “remainder dependency-gated” language.

The active row must distinguish:
- historical qualified profile: Plans 286–297 closed;
- current full-conformance continuation: Plans 319–328;
- current dependency-ready plan: 319 only until this corrective closes.

Do not rewrite anonymity or floodfill authority while removing the ambiguous global-number presentation.

### 3. Roadmap reconciliation

Update the Proposal 170 roadmap so the 286–297 sequence is explicitly a closed historical qualified profile, not “full Proposal 170”. Preserve its evidence table.

Make Plans 319–328 the current continuation graph and state clearly that the frontend remains out of scope.

### 4. Future uniqueness checker

Add a repository checker that rejects creation of more than one implementation-plan owner for a new global NNN across all subsystem directories.

Rules:
- one implementation owner per new global NNN;
- closure/status/amendment files may repeat that NNN only within the same owning subsystem;
- the known historical 296/297 collisions are an explicit finite allowlist with exact paths;
- no wildcard grandfathering;
- a new collision outside the allowlist fails CI with both paths printed.

Wire the checker into the routine planning/CI floor and document it in plans/README.md and AGENTS.md where appropriate.

### 5. Claim vocabulary

Freeze these planning terms:
- qualified-profile-closed: the 286–297 experimental profile and its explicit incompatibilities are closed as implemented;
- canonical-wire: exact Proposal 170 names, parameter shapes, return types, and action semantics;
- full-proposal-conformant: canonical wire plus every Proposal-required capability either operational or explicitly allowed by the Proposal to be implementation-dependent, with live external evidence.

No current document may use “fully closed” or equivalent in a way that implies full-proposal-conformant.

## Verification

Run the repository planning/link checks plus a dedicated uniqueness-checker fixture suite:
- a new unique number passes;
- duplicate implementation NNN in another subsystem fails;
- same-plan closure in owner subsystem passes;
- exact historical Prop170/296, Prop170/297, Anonymity/296, Anonymity/297 paths pass only through the finite allowlist;
- a fifth 296/297 owner fails.

Verify all live registry links and the Proposal 170 roadmap links resolve.

## Acceptance criteria

Plan 319 passes only when:
1. no executed historical plan/closure is rewritten;
2. the 296/297 collision is explicit and unambiguous in current authority;
3. duplicate/stale Proposal 170 registry rows are gone;
4. current roadmap/registry agree that 286–297 is a qualified historical profile and 319–328 is the continuation;
5. future global-number collisions are mechanically prevented;
6. no production/runtime/spec-support code changes occur.

Closure unblocks Plan 320.
