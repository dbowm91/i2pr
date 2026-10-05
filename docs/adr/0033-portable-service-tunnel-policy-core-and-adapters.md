# ADR 0032: Portable service-tunnel policy core and transport adapters

- Status: Accepted
- Date: 2026-10-05
- Decision owner: repository maintainer (Plan 349)
- Related: ADR 0001, ADR 0002, ADR 0030 (`destination-linkability-domains-service-lifecycle-and-i2pd-streaming.md`), Plans 349–351

## Context

`i2pr-service-tunnels` already contains bounded service-profile validation and HTTP, SOCKS, IRC, access, destination/linkability, rate, and resource policy. The crate is runtime-neutral, while the daemon owns sockets, tasks, Streaming composition, persistence, and router state. A future independent transport implementation should reuse those semantics instead of copying them, without moving transport ownership into this crate.

## Decision

`i2pr-service-tunnels` is the canonical reusable owner of transport-independent service-tunnel policy and filtering semantics. Native i2pr and independently implemented transports are adapters. An adapter supplies bounded normalized values and authenticated peer identity where required, then owns all transport and lifecycle work.

Destination sharing is explicit policy. Dedicated groups remain distinct; services share identity only when their validated specifications name the same configured group. An adapter must preserve that mapping and may not merge or split groups for convenience.

Peer-dependent server policy accepts only identity/hash material authenticated by the underlying I2P transport. Hostnames, nicknames, local socket addresses, and unverified claims are not peer identity.

The core remains runtime-neutral: it owns no sockets, timers, async executor, filesystem, DNS, process, SAM session, router context, NetDB store, transport, or tunnel pool. Adapters own listener/connect/accept operations, task lifecycle, clocks, backpressure, reconnect, key persistence, and local target connections. The core makes bounded policy decisions and exposes pure parsing/filtering/state operations.

Once Plan 350 names the supported public surface, that surface follows the repository's pre-1.0 semver policy: compatible additions may occur in `0.1.x`, but source-breaking changes require an explicit migration/review. Internal M10 names are not a compatibility promise solely because they are currently public.

This is architectural reuse of i2pr-authored policy code, not permission to copy implementation code from other routers or SAM libraries. SAM wire/session behavior remains the responsibility of a separate downstream implementation. Package publication remains disabled until the repository owner selects and records a license.

## Consequences

- Plans 350–351 may stabilize bounded public value types and prove an external consumer, but may not add runtime or SAM ownership.
- The adapter contract is specified in `specs/references/portable-service-tunnel-core-v1.md`.
- `scripts/check-service-tunnel-boundaries.sh` guards forbidden ownership and includes positive controls so the checks must remain capable of detecting violations.
- No M10 behavior, support inventory, or router capability claim changes.
