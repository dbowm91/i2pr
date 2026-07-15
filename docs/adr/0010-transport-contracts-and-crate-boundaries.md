# ADR 0010: Transport contracts and crate boundaries

- Status: Accepted
- Date: 2026-07-15

## Context

Milestone 3 needs a stable ownership boundary before NTCP2 handshake and data
code can be implemented. Transport links have peer-controlled inputs, bounded
queues, identity-correlation risks, and long-lived runtime work. Putting all of
that into `i2pr-runtime` would couple protocol decisions to Tokio; putting
Tokio or sockets in the protocol crates would make deterministic state-machine
tests and later transport replacement harder to audit.

## Decision

The workspace uses the following dependency direction:

```text
i2pr-proto <- i2pr-crypto <- i2pr-storage
     ^              ^               ^
     |              |               |
i2pr-core <- i2pr-transport <- i2pr-runtime <- i2pr-daemon
     ^             ^       ^          ^
     |             |       |          |
     +-------------+-------+  i2pr-transport-ntcp2

i2pr-testkit may depend on the transport crates for synthetic tests only.
```

`i2pr-transport` owns runtime-neutral transport-manager vocabulary: bounded
peer and link references, encoded-I2NP ownership, delivery requests/outcomes,
link lifecycle, duplicate-resolution decisions, address/reachability
observations, resource admission, and privacy-safe snapshots.

`i2pr-transport-ntcp2` owns the future NTCP2 address, constants, cryptographic
wrappers, handshake, frame, block, and state-machine modules. In this plan it
is only a Tokio-free, filesystem-free skeleton and does not claim handshake,
encryption, socket, or interoperability support.

`i2pr-runtime` remains the sole production owner of Tokio tasks, sockets,
timers, channels, wakeable cancellation, and supervised reader/writer
children. The daemon remains the composition root. The testkit is outside the
production dependency graph.

Protocol state machines are driven through explicit bounded input/output
actions and typed results. No async traits are introduced: the runtime owns
waiting, partial I/O, deadlines, cancellation, and cleanup while a pure state
machine owns protocol transitions.

The transport boundary carries canonical encoded I2NP message bytes in a
validated, bounded owned container. This preserves authenticated bytes and
avoids repeated decode/re-encode. The container has redacted diagnostics, no
implicit large-payload clone, and an explicit consuming handoff. Delivery
requests carry only a bounded peer reference, payload owner, monotonic expiry,
and a runtime-owned one-shot response capability.

The contracts intentionally cover only immediate NTCP2 and future SSU2 link
management needs. They do not model every transport feature, peer policy,
NetDB mutation, tunnel selection, client routing, or application session.

## Alternatives considered

- Putting Tokio directly in `i2pr-transport-ntcp2` was rejected because it
  would mix protocol transitions with executor and I/O ownership.
- A generic plugin transport framework was rejected because a closed transport
  kind and narrow contracts are easier to bound and audit before SSU2 evidence.
- Raw `Vec<u8>` everywhere was rejected because it loses maximum/ownership
  evidence and makes payload-bearing diagnostics and accidental clones likely.
- Exposing raw Tokio channels or sockets across crate boundaries was rejected
  because it leaks runtime ownership and unbounded implementation choices.
- Merging all transport logic into `i2pr-runtime` was rejected because it
  would make protocol state and deterministic tests depend on Tokio.
- Guessing duplicate-link winner policy was rejected; Plan 035 will decide it
  from implementation and interoperability evidence.

## Consequences

The transport crates remain pure and testable, while runtime adapters carry
the complexity of waiting, cancellation, socket I/O, and task ownership.
Transport implementations must use explicit bounds, typed outcomes, exact
resource leases, and redacted observations. The boundary is intentionally
small, so later plans may add capabilities only with a bounded plan and
reviewed dependency decision.

## Review triggers

Review this decision if a second runtime is selected, if a transport requires
runtime ownership to cross the crate boundary, if duplicate resolution needs
more policy than the current decision surface can represent, or if a later
transport requires a new independently bounded resource class.
