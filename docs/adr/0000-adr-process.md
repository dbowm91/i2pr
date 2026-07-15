# ADR 0000: ADR process and status vocabulary

- Status: Accepted
- Date: 2026-07-15

## Context

Architecture choices need durable rationale while the router contracts are
still changing. Plans define implementation scope; ADRs record decisions that
affect multiple plans.

## Decision

Each ADR has a sequential number, a focused title, status, context, decision,
consequences, alternatives, and review triggers. Status values are:

- **Proposed**: under discussion and not a binding implementation constraint.
- **Accepted**: the current project decision.
- **Superseded**: replaced by a later accepted ADR; link the replacement.
- **Deprecated**: no longer recommended but retained for historical context.

New ADRs should be added before code when a cross-cutting decision is needed.
Changes to an accepted decision should create a new ADR or explicitly amend the
existing record with owner review.

## Consequences

The repository can explain why boundaries and dependencies exist without
pretending that future protocol APIs are stable. ADRs complement, but do not
replace, `GUARDRAILS.md`, plans, or protocol dossiers.

## Review triggers

Review this process if the project adopts a formal governance or release
process, or if ADRs become insufficient for security-sensitive decisions.
