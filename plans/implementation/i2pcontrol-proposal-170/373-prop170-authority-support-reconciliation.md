# Plan 373 — Proposal 170 authority, support, and successor-state reconciliation

Status: **registered-prop170-authority-support-reconciliation-ready**

Classification: planning/support corrective. No production behavior change.

Subsystem: `i2pcontrol-proposal-170`.

## Objective

Bring the canonical planning and support surfaces back into agreement with the code and authoritative
closure records before the next external qualification phase starts.

This plan exists because the implementation moved faster than the summary surfaces. It must not
promote capability merely because code exists.

## Inputs that are already authoritative

Treat these closure outcomes as facts, not work to redo:

- Proposal 170/322: passed after 339/340; all 43 RouterInfo additions have owners.
- Proposal 170/342: passed **scoped**; request paths and loopback wire lane work, but live
  multi-outproxy failover and post-restart request evidence are absent, so historical 327 is not
  fully closed.
- Proposal 170/346: passed; strict Proposal-146 Red25519 and the deployed ELS2 type-11 profile are
  separate typed constructions.
- Proposal 170/347: stopped at 0/4 full directions; its signature stage is closed.
- Proposal 170/350: passed; controlled floodfill stores and serves type 5.
- Proposal 170/349: superseded by Proposal 170/351.
- Proposal 170/351: passed gate-scoped; a B33 encrypted service has a production service-tunnel
  consumer path.
- Proposal 170/352: passed; config error redaction and password-bearing file-mode gate landed.
- Proposal 170/348: historical blocked final gate; its Proposal re-freeze was clean but its
  dependency model is now stale.
- Type 5 and unqualified full Proposal 170 remain non-advertised/unclaimed.

## Required reconciliation

### 1. `plans/registry.md`

Recompute the Proposal 170 top-level summary from closure authority.

Correct at least:
- no claim that Plan 322 is still blocked;
- no claim that Proposal 170/334 is blocked;
- no claim that i2pr still lacks a type-5 consumer path;
- no claim that Java needs a bandwidth-tier design for the ELS2 matrix;
- Plan 342 is scoped-pass, not a complete Plan-327 closure;
- Proposal 170/352 is passed, not ready;
- Plan 347 is historical stopped evidence; Plans 374/375 are its reference-specific successors;
- Plan 348 is historical blocked; Plan 378 is the forward final gate.

Every colliding historical number must be subsystem-qualified.

### 2. `specs/support.toml`

Correct stale factual reasons without changing support posture prematurely.

Remove/replace claims that:
- type 5 is deferred because no Red25519 provider exists;
- Red25519 remains gated by Plan 325;
- ELS2 type-11 is i2pr-only and Java/i2pd cannot verify it;
- the controlled floodfill cannot store/serve type 5;
- the consumer resolver has no production caller;
- the Plan-334 Java/i2pd differential is unexecuted at the signature boundary.

The replacement must distinguish:
- strict Proposal-146 Red25519 qualification;
- deployed ELS2 type-11 qualification at the crypto boundary;
- local type-5 publication/store/serve/consume capability;
- missing **live cross-router** ELS2 evidence;
- missing live outproxy failover/restart evidence.

Keep type 5 / Proposal-170 advertisement flags unchanged unless an already-passed closure explicitly
authorized promotion. Plan 377/378 own future promotion.

### 3. `specs/CONFORMANCE.md`

Reconcile evidence vocabulary:
- crypto-boundary cross-verification is not ELS2 interoperability;
- loopback outproxy evidence is not live failover/restart evidence;
- real ELS2 interoperability requires store + lookup + decrypt + inner-LS2 + application data;
- a final Proposal-170 claim imports exact external artifacts rather than re-labeling local tests.

### 4. Roadmaps

Reconcile:
- `plans/subsystems/i2pcontrol-proposal-170-roadmap.md`;
- `plans/subsystems/red25519-encrypted-leaseset-roadmap.md`.

Retire stale forward language around 347/348 and register 374–378 as current authority.

### 5. Collision ledger and guard prose

Do **not** renumber historical collisions.

Correct stale ledger metadata, including Proposal 170/352's closure state, and ensure prose matches
the current guard implementation (the plan-number guard derives its tolerated exact sets from the
ledger rather than maintaining a second hand-written allowlist).

Run both global plan-number and ADR-number guards. New Plans 373–378 must each have one owner.

## Required evidence

- diff showing no production Rust/config behavior change;
- machine-readable/support validation still parses;
- planning uniqueness guards pass;
- all new 373–378 paths are unique;
- no support/advertisement boolean is promoted by this plan;
- grep/checker evidence that stale phrases listed above are either removed from current authority or
  explicitly marked historical.

## Acceptance criteria

Plan 373 passes when the registry, both roadmaps, support inventory, conformance vocabulary, and
collision ledger agree on the same current state and leave capability advertisement unchanged.

Passing Plan 373 unblocks Plans 374, 375, and 376 in parallel.
