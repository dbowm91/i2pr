# Plan 344 — Plan 326 re-audit against the delivered crypto and publication work

Status: **passed-reaudit-found-and-closed-one-mode-coverage-gap-326-still-blocked-on-external-transcript**

Closure record:
[`plans/closure/i2pcontrol-proposal-170/344-status.md`](../../closure/i2pcontrol-proposal-170/344-status.md)

## Current implementation progress

Completed and closed on 2026-10-05. This is the re-audit
[`326-status.md`](../../closure/i2pcontrol-proposal-170/326-status.md) explicitly
asked for instead of reopening Plan 326 on Plan 325. All three sub-items it
listed are now closed, and the re-audit **found one real gap**, which is fixed
and inversion-proven.

**Plan 326 remains blocked, and its status token is corrected rather than
cleared.** Its true remaining gate is not a provider question and not a
mode-coverage question; it is the external type-11 transcript, which is Plan
335.

Classification: audit + one corrective evidence pass.

Hard dependencies: Plan 326 closed blocked; Plans 330, 332, 333, 337, 338
passed; Plan 331 passed.

Subsystem: `i2pcontrol-proposal-170`.

## Objective

Plan 326's closure record carried a dated correction listing three items it
believed were still open:

1. Plan 325's Red25519 provider question;
2. the "all ten encryption modes" breadth;
3. control-plane publication evidence.

It recommended re-auditing against 330/332/333 rather than reopening on 325. This
plan performs that re-audit against the acceptance criteria Plan 326's own plan
of record sets, and records the true remaining gate.

## Why this plan does not close 326

Plan 326's acceptance criteria are stricter than its sub-items:

> "Plan 326 closes only when every Proposal LeaseSet mode has its exact
> implemented semantics **and the encrypted-LS2 path works end-to-end through
> real publication and lookup**."

> "No mode may pass from parser acceptance or inert storage."

and its evidence list requires:

> "external lookup/publication against at least one independent implementation
> before capability claim."

The three sub-items can all close and Plan 326 still cannot, because the
external requirement is gated on the type-11 transcript divergence in Plan 335.
The re-audit's job was to find the true blocker, and this is it.

## Scope

In scope: the three sub-items; a mechanical coverage census of the ten
canonical spellings against the *class* of evidence Plan 326 demands; the
corrective evidence pass for any gap found.

Out of scope: the type-11 transcript itself (Plan 335); any capability or
advertisement change; any production behaviour change beyond test coverage; the
outproxy line (Plan 342).

## Invariants

1. No mode is counted as covered on parser acceptance alone.
2. A refused mode's evidence is a by-name refusal, never inert storage.
3. A false diagnosis is corrected by dated amendment, never by silent rewrite.
4. The audit reports what it did not verify as plainly as what it did.

## Stop conditions

Stop and re-audit if closing a sub-item would require weakening a row, relaxing
an acceptance criterion, or asserting external interoperability that has not
been measured.
