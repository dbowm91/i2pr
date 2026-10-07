# Plan 376 — Outproxy live failover, restart, and Plan-327 successor closure

Status: **registered-outproxy-live-failover-restart-blocked-on-plan373**

Classification: capability + resilience/security evidence.

Hard dependencies:
- Plan 373 passed.
- Proposal 170/341 passed.
- Proposal 170/343 passed.
- Proposal 170/342 passed scoped.

## Objective

Close the two deliberately unproven rows left by Proposal 170/342:

1. live failover/rotation across multiple configured I2P outproxies;
2. live daemon restart followed by a successful routed request.

This plan is the successor closure for historical Plan 327.

External Java/i2pd router interoperability is not required: Proposal 170 specifies the control
surface, while the security invariant is that every clearnet request is carried only through an I2P
Streaming route to an outproxy.

## Controlled live lane

Extend the existing real-Streaming loopback lane, but use at least two distinct startup-owned
outproxy service destinations:

- **O1** — first configured outproxy;
- **O2** — second configured outproxy;
- **C** — the client tunnel carrying HTTP/CONNECT/SOCKS requests;
- loopback origin fixtures behind O1/O2.

The test process itself must retain no clearnet resolver/socket capability. All target names remain
opaque labels handed to O1/O2.

## Failover requirements

Prove the actual `ProxyList` order and bounded attempt semantics.

Mandatory rows:
- O1 unavailable at Streaming-connect stage -> same request succeeds through O2;
- O1 establishes Streaming but returns upstream-refused -> policy either retries O2 or fails
  according to the frozen failure taxonomy; the expected behavior must be explicit and tested;
- O1 authentication-rejected is not silently retried with credentials at a different security
  boundary unless the current policy explicitly allows it;
- attempt count never exceeds the configured/hard ceiling;
- backoff is bounded and cancellation-aware;
- a list shorter than attempts wraps only according to `OutproxyList::select`;
- all attempts failing returns a typed refusal and opens no direct clearnet socket.

Counters must identify attempt/success/failure classes without exposing target or credential values.

## Rotation requirements

If the provider intentionally implements ordered failover rather than round-robin across requests,
say so and test that exact policy. Do not invent "rotation" semantics merely to satisfy historical
wording.

The closure must distinguish:
- **within-request failover**, which is mandatory;
- **between-request load rotation**, which is required only if the current provider contract claims
  it.

## Restart requirements

Use a real daemon/service-product restart boundary, not serialization alone.

Prove:
1. canonical seven-field outproxy block persists;
2. sealed password remains recoverable only through the router-bound secret owner;
3. startup reconstructs the provider registry before a client request is accepted;
4. a post-restart HTTP CONNECT request reaches an outproxy and application data passes;
5. `.i2p` traffic still bypasses the provider after restart;
6. removing the provider after restart makes clearnet requests fail closed;
7. copied config without the matching router secret cannot recover the credential.

## Request-family coverage

At least one failover row must use HTTP CONNECT and one must use a SOCKS path.

The restart row may use HTTP CONNECT as the canonical proof, but existing forward-HTTP and SOCKS
regressions stay green.

## Security invariants

Re-run and extend the static guards:
- no `ToSocketAddrs`, system DNS, direct clearnet connect, dynamic library loading, or shell
  execution enters the request path;
- `TargetPolicy::AllowsClearnet` is reachable only when the provider registry contains a valid
  I2P-only provider;
- no credential in control output, logs, counters, or evidence;
- cancellation frees every pending Streaming attempt;
- no lock is held across await on the provider/counter path.

## Evidence

Add machine-readable live-lane evidence/checker for:
- endpoint attempted;
- attempt ordinal;
- typed outcome;
- restart generation;
- provider count;
- application success.

Identify endpoints by fixture IDs/hashes only; do not record clearnet target names or credentials if
they are operator-provided values.

Remove the Plan-342 `DOCUMENTED_ABSENCES` entries only when their replacement rows actually pass.

## Acceptance criteria

Plan 376 passes when live multi-endpoint failover and a real post-restart routed request are proven,
all no-direct-clearnet/security guards remain green, and the historical Plan-327 remainder has no
unproven capability row.

Passing Plan 376 is the outproxy prerequisite for Plan 378.
