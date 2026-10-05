# Plan 349 — i2pr encrypted LeaseSet2 consumer lookup path

Status: **in-progress-owner-implemented-and-proven-no-production-caller-yet; service-consumer-wiring-remains**

Classification: capability (the consumer half of the type-5 lifecycle) + architecture gap
closure. Origin: the classified boundary in
[`347-status.md`](../../closure/i2pcontrol-proposal-170/347-status.md).

Hard dependencies:
- Plan 346 passed (ADR 0032; the bounded type-11 transcript profile and its static guard).
- Plan 347 stopped with a written, stable boundary describing exactly what is missing — this
  plan's specification.
- Plans 332/333 passed (type-5 framing, layer crypto, client authorization) and Plan 344
  passed (all ten `EncryptLeaseSet` modes have real evidence rows).

Subsystem: `i2pcontrol-proposal-170`.

## The gap

i2pr can **publish** an encrypted LeaseSet2 and cannot **resolve** one. The asymmetry is
invisible from the publisher side, which is why it survived Plans 332–338 and only surfaced
when Plan 347 tried to run the consumer direction of a live cross-router matrix.

Measured, at Plan 347's head:

- `EncryptedLeaseSet2Resolver` has **zero production callers**. `rg` over `crates/` finds it
  only in `crates/i2pr-client/tests/els2_publish_resolve.rs` and
  `crates/i2pr-client/tests/els2_authorized_publish_resolve.rs`. Every round trip is
  in-process.
- The **transport already exists and is key-agnostic**:
  `crates/i2pr-daemon/src/netdb_tunnels.rs:386` `begin_tunnel_lookup` and `:447`
  `compose_lookup_via_tunnel` take the lookup target as a plain `Hash`
  (`:460-462`), so a blinded storage key is structurally acceptable. Their only callers are
  tests (`netdb_tunnel_live.rs`, `netdb_tunnel_external.rs`, `streaming_tunnel_external.rs`).
- Nothing wires `current_storage_key` into that composer, validates the reply as
  `ValidatedEncryptedLeaseSet2`, decrypts the layers, or hands the inner `LeaseSet2` to a
  service. That composition does not exist.
- The publisher side is real and live: `service_product.rs:3654-3685` builds the
  `DatabaseStoreMessage` (type-5 branch `:3679-3683`) and `publish_service_ls2_for_service`
  at `:3707-3811` sends it over a tunnel, requiring `RouterDeliveryOutcome::Accepted` per
  cell.

So the missing work is not cryptography, not framing, and not transport. It is **one missing
composition on the consumer side**, and it is the reason Plan 347's `i2pd → i2pr` direction is
unreachable.

## Objective

Give i2pr a production owner for the encrypted-LeaseSet2 **client** lifecycle, so a
destination with a `.b32.i2p` address can resolve the type-5 record and use the inner
`LeaseSet2` — completing the type-5 lifecycle symmetrically with the publisher.

The acceptance path this plan must make real:

```text
encrypted service address (b33 / b32.i2p)
 -> lookup secret applied -> daily blinded key
 -> blinded DHT storage key
 -> DatabaseLookup over the existing key-agnostic lookup composer
 -> DatabaseSearchReply
 -> ValidatedEncryptedLeaseSet2 (deployed/strict type-11 profile, ADR 0032)
 -> storage-key gate -> freshness gate
 -> layer-1/layer-2 decrypt
 -> inner LeaseSet2 (or MetaLeaseSet) validation
 -> handed to a service owner as an ordinary LeaseSet2
```

## In scope

1. **A single consumer owner.** One named owner in the runtime/daemon composition root holds
   the resolve lifecycle: in-flight request identity, deadline, cancellation, and the
   `(address, secret) -> blinded storage key` derivation. It must not be a free function and
   it must not be reachable only from tests.
2. **Wiring the existing lookup transport.** Compose the `DatabaseLookup` for the blinded
   storage key through `netdb_tunnels` rather than adding a second lookup path. Reuse, do not
   duplicate: no new unbounded queue, no new timer owner, no new socket.
3. **Reply validation and the storage-key gate.** A reply whose `DatabaseStore` key does not
   equal the requested blinded key must be refused before decrypt, and the accepted transcript
   profile must be recorded.
4. **Decrypt and inner validation**, then hand the inner `LeaseSet2` to a service owner through
   the same path an ordinary type-3 publication uses, so downstream code cannot tell the
   difference and gains no ELS2-specific branch.
5. **Per-client authorization on the consumer side** for the PSK and DH forms Plan 333
   implemented on the server side, so an unauthorized client fails *before* it can use the
   inner LeaseSet.

## Out of scope

- **The Java bandwidth-tier design.** Stock Java I2P refuses to dial i2pr without a caps tier
  that ADR 0030 forbids inventing (Plan 306, exhausted across three counted attempts). That is
  a RouterInfo advertisement policy question with its own plan of record and it is not an
  ELS2 problem. This plan must not touch `caps`, `R`, or advertisement to chase it.
- **Any change to the type-11 transcript policy** (ADR 0032). Plan 347 explicitly forbade
  changing it ad hoc, and it is correct.
- **Executing Plan 347's matrix.** This plan delivers the i2pr-side capability and local
  evidence; the live cross-router run stays Plan 347's re-attempt, for the i2pd directions.
- New daemon configuration surface, I2PControl options, or any advertisement.

## Invariants

- No unbounded channel, queue, or in-flight table. Every request has an owner, a deadline, and
  a cancellation path; the lease is released on receive, drop, timeout, cancel, panic, and
  teardown.
- `i2pr-netdb` and `i2pr-client` stay runtime-neutral: no `tokio`, no `std::net`, no
  `std::fs`, no raw `JoinHandle`, no ownerless `spawn`.
- The consumer path must be able to fail closed at every step: wrong secret, wrong day, wrong
  storage key, malformed record, wrong credential, unauthorized client, tampered ciphertext.
- No secret reaches evidence, logs, or errors. The lookup secret is zeroized and never copied
  into a second long-lived string, matching the publisher-side owner.
- Dependency direction is preserved; no production crate may depend on `i2pr-testkit`.
- **Capacity-1 queue discipline**: cover empty, exact load, and max+1, and assert lease release
  on every drop path.

## Required evidence

### Lifecycle rows
- no-auth resolve end to end against a store the same process published;
- lookup-secret resolve with the correct secret, a wrong secret, and a missing secret;
- PSK client authorization: authorized succeeds, wrong PSK fails, unconfigured client fails;
- DH/X25519: authorized succeeds, wrong private key fails, multiple authorized clients stay
  bounded and independently usable;
- ordinary (non-ELS2) LeaseSet2 resolution is byte- and behaviour-identical, and the consumer
  path is not ELS2-aware above the owner.

### Negative rows
- wrong blinded storage key on the reply;
- expired / stale type-5 record;
- tampered outer signature and tampered ciphertext;
- a strict-only Proposal-146 signature and a deployed signature both accepted **only** through
  the bounded compatibility verifier, never the generic type-11 API;
- a malformed `.b32.i2p` / b33 address rejected at construction, not as a lookup miss;
- cancellation and deadline release the in-flight lease.

### Runtime/owner rows
- a restart drops no owned state and leaks no lease;
- concurrent resolves for distinct addresses do not cross-contaminate;
- the in-flight bound is enforced at capacity 1, exact load, and max+1.

## Production changes

Expected: a new consumer owner module in the runtime/daemon composition root, the wiring from
that owner into the existing `netdb_tunnels` lookup composer, and a per-client authorization
step for the consumer direction. No new dependency, no wire change, no advertisement change.

## Verification

The full `AGENTS.md` routine floor, plus the focused suites:

```text
cargo test --locked -p i2pr-client --test els2_publish_resolve -- --test-threads=1
cargo test --locked -p i2pr-client --test els2_authorized_publish_resolve -- --test-threads=1
cargo test --locked -p i2pr-daemon --test <new consumer lane> -- --test-threads=1
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-els2-type11-transcript-boundary.sh
```

Rows must drive the production composition root, not private helpers: the existing precedent is
`i2pr-daemon`'s black-box suites, which drive behaviour only through a real listener or the
real composition, and `sam_stream_self_composed.rs`, which must not call private bridge or
driver APIs.

> **Progress.** The bounded owner (`EncryptedServiceResolver`,
> `crates/i2pr-daemon/src/encrypted_service_resolver.rs`) and its 17 rows are done and green. The
> plan is **not** closed, because the owner has **no production caller**: a service-tunnel
> destination is a raw 32-byte `DestinationId`, so pointing one at a b33 address needs new
> configuration surface, secret storage, and tunnel-lifecycle integration. The remaining work and
> the requirement matrix are in
> [`349-status.md`](../../closure/i2pcontrol-proposal-170/349-status.md).

## Acceptance criteria

Plan 349 passes only when:

1. a production owner resolves an encrypted service address through the real lookup transport
   to an inner `LeaseSet2` a service can use;
2. `EncryptedLeaseSet2Resolver` has at least one non-test production caller, and a static guard
   fails if that regresses to zero;
3. all four authorization modes have consumer-side rows, and every negative row fails closed;
4. the ordinary LeaseSet2 path is unchanged and the consumer is not ELS2-aware above its owner;
5. the in-flight bound, lease release on every drop path, and restart behaviour are proven;
6. ADR 0032's guard still passes and no transcript policy changed;
7. exact-head routine CI is green.

## Acceptance effect on Plan 347

Passing Plan 349 makes Plan 347's `i2pd → i2pr` direction reachable and its `i2pr → i2pd`
direction executable, so Plan 347 can be re-attempted **for the i2pd directions**. The two
Java directions remain blocked on the bandwidth-tier design regardless of this plan, and this
plan must not be credited with unblocking them.

## Closure evidence required

Commit, a requirement-to-evidence matrix, commands with local/CI outcomes labelled truthfully,
the runtime-neutrality and bounded-queue reviews, the secret-handling review, a statement that
no Java or i2pd source was vendored, known limitations, findings by severity, and the roadmap
disposition.
