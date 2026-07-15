# ADR 0001: Modular monolith and crate-boundary strategy

- Status: Accepted
- Date: 2026-07-15

## Context

The MVP spans protocol codecs, transports, routing, clients, and service
tunnels. Splitting them into processes before contracts stabilize would add
deployment and failure complexity; placing everything in one crate would blur
security and ownership boundaries.

## Decision

Start with one foreground daemon process composed from focused Rust crates.
Boundaries follow protocol churn, security boundaries, ownership, and
dependency direction. The initial workspace contains only `i2pr-proto`,
`i2pr-core`, `i2pr-daemon`, and `i2pr-testkit`. The daemon is the composition
root, and lower crates do not depend on it.

## Consequences

The project gets compile-time dependency checks and simple local operation
without prematurely creating empty packages. Narrow crate APIs can evolve with
detailed plans. Process isolation remains available for future external service
boundaries, but runtime-loadable Rust plugins are not part of the MVP.

## Alternatives

An all-in-one crate was rejected because it weakens ownership and review
boundaries. A distributed service architecture was rejected because it would
stabilize IPC and deployment contracts before the wire and lifecycle contracts
are understood.

## Review triggers

Revisit this decision if a component requires independent deployment or
privilege isolation, or if the crate graph becomes an obstacle to audits.
