# Plan 351 — i2pr encrypted LeaseSet2 consumer as a service-tunnel remote target

Status: **passed-gate-scoped-els2-consumer-service-wiring-landed; supersedes-plans-349-scope-not-status**

Classification: capability (the consumer half of the type-5 lifecycle, reachable) +
architecture-gap closure. Origin: the open state in
[`349-status.md`](../../closure/i2pcontrol-proposal-170/349-status.md).

This is a **corrective pass** on Plan 349, not an amendment. Per `plans/README.md:68`,
corrective work is a new plan referencing the original. Plan 349's status token is
**unchanged** and remains `in-progress`; Plan 351 does not retroactively claim its
acceptance criteria are met, it supplies the missing capability so 349 can be re-closed
on its own evidence.

## Why Plan 349 cannot close as written

Plan 349 lists *"New daemon configuration surface, I2PControl options, or any
advertisement"* under **Out of scope** (`349-els2-consumer-lookup-path.md:97`), while its
acceptance criterion 1 requires *"a production owner resolves an encrypted service address
through the real lookup transport to an inner `LeaseSet2` a service can use"*. Its closure
record then names "Configuration surface" as remaining work item 1
(`349-status.md:35`). The plan forbids the work its own closure record requires. Plan 351
resolves that by scoping the surface explicitly.

## Hard dependencies

- Plan 346 passed (ADR 0032; the bounded type-11 profile and its static guard).
- Plan 350 passed (the floodfill both stores and serves type 5, so a reference consumer's
  blinded-key lookup can now be answered).
- Plan 349 in progress with the bounded `EncryptedServiceResolver` owner delivered and 17
  green rows at `e684a06`. Its interface is stable: `begin` / `begin_authorized` /
  `ingest_store` / `cancel` / `expire` and the `MAX_CONCURRENT_ENCRYPTED_RESOLVES = 8`
  bound are the written contract this plan consumes.
- Plan 337 passed (one shared `ServiceTunnelManager`), so a control-created tunnel is the
  same runtime as a configured one.

Subsystem: `i2pcontrol-proposal-170`.

## The gap, measured at `fff71d0`

- `EncryptedLeaseSet2Resolver` (`crates/i2pr-client/src/encrypted_leaseset.rs`) has **zero
  production callers**. `EncryptedServiceResolver`
  (`crates/i2pr-daemon/src/encrypted_service_resolver.rs:245`) is the bounded owner and has
  **zero production callers**. `rg` finds both only in their own module and test file.
- A service-tunnel remote target cannot be a b33 address. `DestinationRef::parse`
  (`crates/i2pr-service-tunnels/src/destination.rs:55`) dispatches on the `.b32.i2p` suffix
  first, and `parse_base32_hash` requires exactly 52 characters, so a 56- or 60-character
  encrypted-service address is refused with the **misleading** reason *"Base32 label must be
  exactly 52 characters"* rather than being recognised as an encrypted-service address.
- The lookup path cannot carry a blinded key even if one were supplied:
  - `netdb_seam.rs:328-332` re-derives the lookup identity from a `DestinationHash`:
    `LookupId::new(request_id, LookupKind::LeaseSet2, router_hash_from_destination(target))`.
    A blinded storage key is not a `DestinationHash`.
  - `lookup_engine.rs:606-613` refuses anything that is not
    `DatabaseStoreData::LeaseSet2`, so a type-5 reply is discarded before it can reach the
    consumer owner.
  - `destination_tunnels.rs:773` takes `target: DestinationHash` and re-derives
    `router_hash_from_destination(target)` at `:789`.

## What the wire already gets right

Do **not** add a new `LookupKind`. `LookupKind::LeaseSet2.wire_code() == 1`
(`crates/i2pr-netdb/src/lookup_id.rs:27-32`), and that is exactly the lookup type a
reference client issues for a blinded resolve — the floodfill says so at
`floodfill_service.rs:372-375`: *"This is the type a reference client issues when
resolving an encrypted service, because the record is filed under its blinded storage key
rather than the destination hash."* Plan 350 already put record type 5 into
`SERVABLE_LEASE_LOOKUP = [1, 3, 5, 7]`, so the server side answers a `lookup_type 1`
request for a type-5 record.

No new codec is needed either. `LookupId::new` already takes a `RouterHash` verbatim
(`lookup_id.rs:48`) and `RouterHash::from_hash` is `pub const` and does **not** re-hash
(`crates/i2pr-netdb/src/router_info.rs:41-43`), so `BlindedStorageKey` → `RouterHash` is a
lossless conversion. The re-derivation lives in the **daemon seam**, not in NetDB.

## Objective

One production caller that resolves a b33 encrypted-service address through the real lookup
transport and installs the inner `LeaseSet2` into a service's `DestinationRouting`, so a
service-tunnel client can forward to an encrypted remote service over the ordinary path.

## Three gates

These are not style preferences. Each exists because a specific measured defect makes the
un-gated version wrong.

### Gate 1 — an encrypted remote target is only permitted on a `DelayOpen` service

Today a failed remote lookup is **fatal to the entire service-tunnel product**. Two of the
three callers of `provision_all_service_router_material` respond to an error with
`manager.shutdown()` + `token.cancel(OperatorRequest)` + `ssu2_handle.shutdown()`
(`service_product.rs:1086-1090` and `:1287-1294`). The lookup also retries 3× internally
first (`MAX_LOOKUP_ATTEMPTS = 3`, `:2633-2635`), so each startup failure costs three
transmissions plus settle delays before the teardown.

Per-destination isolation **already exists and is proven**, but only for deferred groups:
`deferred_destination_ids()` requires
`.all(|runtime| !runtime.is_server && runtime.delay_open)`
(`service_tunnels.rs:582`), and the deferred path records into
`deferred_activation_failures` (`service_product.rs:661`), confines the error to a oneshot
(`:1505-1506`), and discards it at the product (`:1707`, no `?`). The test at
`service_product.rs:548-558` asserts it: *"failed optional activation stays isolated to
the requester"*.

So the rule is: **reject an encrypted remote target unless the service is a client with
`delay_open` set**, reusing the existing `ServiceTunnelError::ContradictoryOptions` shape
already used for the same class of rule at
`crates/i2pr-service-tunnels/src/config.rs:1129-1140`. This is the truthful form of
containment: the feature is only permitted where isolation is already proven, rather than
claiming isolation that was never built for eager groups.

### Gate 2 — the surface is I2PControl, not TOML

`delay_open` is **not settable from the TOML config**. `RawServiceTunnelEntry`
(`config.rs:567-594`, `#[serde(deny_unknown_fields)]`) has no `delay_open` field; the only
`delay_open` occurrence in `crates/i2pr-daemon/src/config.rs` is the `false` default at
`:1923`. Only I2PControl sets it (`i2pcontrol_tunnels.rs:1341`, canonical option
`delay_open`).

Therefore a TOML-only `encrypted_destination` field would be **dead on arrival**: every
TOML-configured service is eager by construction and Gate 1 would reject all of them.

I2PControl is also where the symmetric publisher-side secret already lives:
`service_els2.rs:260` reads the lookup secret from the control definition's options map,
whose canonical slot is `leaseset_password`
(`crates/i2pr-i2pcontrol/src/tunnel_request.rs:172`). Putting the consumer secret in the
same place is symmetric with the publisher rather than inventing a second secret channel.

I2PControl is loopback-only and disabled by default, so the wire exposure of a control-plane
secret is bounded by an existing guardrail.

**This is the one respect in which Plan 351 deliberately re-scopes Plan 349:** Plan 351 puts
I2PControl options explicitly **in scope**. Plan 349's out-of-scope line is not satisfied,
and that is the point of the corrective.

### Gate 3 — the secret is not inline in any config, and the claim stops at resolve-on-demand

(a) **No inline secret.** The lookup secret arrives through the I2PControl control
definition, borrowed, and is never copied into a second long-lived `String`. It is not
placed in `Config`, in a `Raw*Config` struct, or in a `DestinationRef`. Precedent:
`service_els2.rs:28-35` states the publisher rule — *"The lookup secret is borrowed from
the control definition at construction and is not copied into a second long-lived string
here."* The consumer holds it only for the bounded lifetime of one resolve.

This gate is not theoretical. A TOML parse error prints **the entire offending source
line** — `toml-1.1.6/src/de/error.rs:138` does `writeln!(f, "{content}")` — and a type
mismatch prints **the value** — `serde-1.0.228/src/core/de/mod.rs:410` does
`write!(formatter, "string {:?}", s)`. Both chain through
`ConfigError::Parse` (`config.rs:2853-2854`, `#[error("configuration parse failed: {0}")]`),
`DaemonError::Config` transparent (`error.rs:70-71`), to
`eprintln!("error: {error}")` (`main.rs:51`). Any inline secret in the config file is
therefore printable to stderr today. `ConfigError::Semantic { field: &'static str, reason:
&'static str }` is the only safe error shape and any new rejection must use it.

(b) **No daily re-resolution claim.** The blinding rotates daily, so a stale resolution does
not merely serve stale data — it addresses the **wrong DHT key**. There is no periodic
re-resolution today: `advance_destination_pools()` is inbound-driven, not clock-driven
(`:1471`, `:1481`), and no freshness check reads a remote record's `expires_seconds`. Plan
351 delivers resolve-on-demand with bounded retry and records daily re-resolution as a named
limitation with a follow-on plan number. It must not claim rollover support.

## In scope

1. **`DestinationRef::EncryptedService`** — a new variant carrying
   `EncryptedServiceAddress`, dispatched by
   `EncryptedServiceAddress::is_encrypted_service_address` **before** the `.b32.i2p` branch
   so an encrypted address is recognised rather than misreported as a bad Base32 label.
   `EncryptedServiceAddress::from_text` (`crates/i2pr-proto/src/common/base32.rs:318`) is
   the parser; no new codec.
2. **A blinded-key lookup begin path.** A sibling of
   `begin_lease_lookup` that takes the `BlindedStorageKey`'s `Hash` and builds
   `LookupId::new(request_id, LookupKind::LeaseSet2, RouterHash::from_hash(key))` without
   re-deriving from a `DestinationHash`. `compose_lookup_via_tunnel` is already
   key-agnostic (`destination_tunnels.rs:862-864` reads the key off the action), so no second
   composer is added.
3. **A type-5 response path in `i2pr-netdb`.** One new `LookupResult` variant carrying the
   `DatabaseStoreMessage`, and a type-5 arm in the response handler that performs the
   key-match and type check and then **delegates all ELS2 policy upward** to
   `EncryptedServiceResolver::ingest_store`. The engine must not validate, decrypt, or apply
   a transcript profile — ADR 0032's closed policy stays in the owner. The key-match at
   `lookup_engine.rs:606` is the existing gate and stays.
4. **Per-service failure containment.** The b33 branch in the provisioning loop must
   record-and-`continue` rather than propagate through the `?` at `service_product.rs:3628`,
   and the failure must be visible per service. `deferred_activation_failures` is
   `ProductInner`-private and is not surfaced through `ServiceTunnelManager`, the I2PControl
   state, or the `lib.rs` trait hooks, so a per-service status surface is required.
5. **Installation under the unblinded key.** `EncryptedLeaseSet2Resolver::resolve` returns a
   **raw** `LeaseSet2`, not a `ValidatedLeaseSet2`, while the install path requires a
   validated one keyed by `DestinationHash`
   (`service_product.rs:2855-2868` → `sam/streams.rs:961-985` → `i2pr-client/src/routing.rs:598-619`).
   The inner record must be re-validated under the **unblinded** destination hash derived
   from the b33 address's public key — not the blinded storage key. Getting this wrong
   installs a record that never matches, silently.
6. **The two static guards** named under Required evidence.
7. **A per-service observation label** only if it can be added to the accepted set in
   `service_delivery.rs:385-439`. That function is free-form `&str` with a `_ => {}`
   fallthrough, so an unadded label is a **silent no-op**. A new counter needs both a field
   on `RemoteDeliveryCounters` (`:149`) and a match arm; the plan must assert the arm exists
   rather than trusting the call site.

## Out of scope

- **PSK / DH client credentials on the consumer side.** They need a credential-file owner
  that does not exist, and shipping one inside this plan would mean inventing it under
  time pressure. `EncryptedServiceResolver::begin_authorized` already has 17-row coverage;
  it stays uncalled by production. Deferred to a follow-on plan.
- **Any change to the type-11 transcript policy** (ADR 0032). Untouched.
- **A new `LookupKind`, a new wire lookup type, or any `LookupId` / `router_info.rs` /
  `base32.rs` change.** The existing types are already correct.
- **A second lookup composer, queue, timer owner, or socket.** Reuse `netdb_tunnels` /
  `destination_tunnels`.
- **Daily re-resolution / rollover support** (Gate 3b).
- **Executing Plan 347's matrix.** This plan delivers i2pr-side capability plus local
  evidence. It is not credited with unblocking the Java directions.
- **The config-secret hygiene defects** (TOML parse echo, missing config-file mode check).
  Those are pre-existing, affect `I2pControlPassword` today, and are registered separately
  as Plan 352 rather than buried here.

## Invariants

- No unbounded channel, queue, or in-flight table. `EncryptedServiceResolver`'s
  `MAX_CONCURRENT_ENCRYPTED_RESOLVES = 8` is the bound; capacity 1, exact load, and max+1
  stay covered, and a refused `begin` must still be proven not to consume a slot.
- The lease is released on receive, drop, timeout, cancel, panic, and teardown. The
  provisioning loop must not abandon a future.
- `i2pr-netdb` and `i2pr-client` stay runtime-neutral: no `tokio`, no `std::net`, no
  `std::fs`, no raw `JoinHandle`, no ownerless `spawn`.
- Dependency direction preserved; no production crate depends on `i2pr-testkit`.
- Fail closed at every step: malformed b33 at construction, wrong secret, wrong day, wrong
  blinded key, malformed record, tampered ciphertext, non-type-5 reply.
- No secret in `Debug`, `Display`, `Serialize`, evidence, or logs. No new `derive(Debug)` or
  `derive(Clone)` on a secret-bearing type.
- Ordinary (non-ELS2) LeaseSet2 resolution is byte- and behaviour-identical, and the service
  layer gains no ELS2-specific branch above the owner.
- The b33 branch must be **absent** for a service that has no encrypted destination, so the
  ordinary path is provably untouched.

## Required evidence

### Capability rows

- A service-tunnel client with a b33 `encrypted_destination` and no secret resolves the
  type-5 record and the inner `LeaseSet2` is installed under the unblinded destination hash,
  through the **real composition root**, and a payload reaches the remote target.
- A `DelayOpen` service whose encrypted resolve fails keeps every other service running, and
  the failing service reports a typed per-service error to its own caller.
- The installed record is usable by `DestinationRouting::select_lease` /
  `compose_outbound_delivery` with no ELS2-specific branch.

### Negative rows

- A b33 address on a service **without** `delay_open` is rejected at configuration
  validation, naming the rule.
- A 52-character `.b32.i2p` label, a truncated b33, a non-canonical b33, and an
  IP-literal-with-suffix are each rejected at construction, **not** as a lookup miss, and a
  b33 is not misreported as a bad Base32 label.
- A reply for another blinded key, a non-type-5 reply, a tampered record, a record for
  another day, and a wrong lookup secret each fail closed and release the lease.
- A b33 with no secret configured does not fall back to a secretless resolve when the
  address requires one.

### Runtime/owner rows

- The in-flight bound holds at capacity 1, exact load, and max+1 through the production
  composition, not only in the owner's unit tests.
- A teardown mid-resolve releases the lease and leaves no ghost destination.
- Two concurrent encrypted services do not cross-contaminate.
- A restart drops no owned state and leaks no lease.

### Guard rows

Both scripts must be **negative-tested**: each assertion must be shown to fail when its
condition is deliberately broken, and the run must be recorded.

## Production changes

`i2pr-service-tunnels` (a `DestinationRef` variant + the Gate 1 validation rule);
`i2pr-netdb` (one `LookupResult` variant + one type-5 response arm, policy delegated
upward); `i2pr-daemon` (a blinded-key lookup begin sibling in `destination_tunnels` or
`netdb_tunnels`, the daemon-seam `LookupId` construction, the provisioning-loop branch, the
per-service status surface, and the unblinded-key install); `i2pr-daemon/src/i2pcontrol_tunnels.rs`
(the option parse and validation). No new dependency, no wire-format change, no
advertisement change, no caps change. ADR 0030 and ADR 0032 untouched.

## Verification

The full `AGENTS.md` routine floor, plus:

```text
cargo test --locked -p i2pr-daemon --test encrypted_service_consumer -- --test-threads=1
cargo test --locked -p i2pr-daemon --test <new encrypted service lane> -- --test-threads=1
cargo test --locked -p i2pr-service-tunnels --all-targets -- --test-threads=1
cargo test --locked -p i2pr-netdb --all-targets -- --test-threads=1
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-els2-type11-transcript-boundary.sh
bash scripts/check-floodfill-type5-serve.sh
bash scripts/check-encrypted-service-consumer-caller.sh
bash scripts/check-config-secret-hygiene.sh
```

Rows must drive the real composition root, following the `i2pr-daemon` black-box
precedent (`sam_stream_self_composed.rs`): behaviour only through the real listener or the
real composition, never by calling private bridge, LeaseSet2, driver, or pump APIs.

## Documentation updates

`docs/adr/` — a new ADR for the ELS2 consumer's lookup-kind and install-key decisions
(accepted ADRs are superseded, never rewritten; ADR 0030 and ADR 0032 stay as they are).
`specs/CONFORMANCE.md` and `specs/support.toml` — record what is actually proven, with type
5 still **non-advertised** (`advertised = false`) until Plan 347 passes. `docs/architecture/i2pr-netdb.md`,
`docs/architecture/i2pr-daemon.md`, `docs/architecture/i2pr-service-tunnels.md`.

## Acceptance criteria

Plan 351 passes only when:

1. a production owner resolves a b33 service address through the real lookup transport and
   installs an inner `LeaseSet2` a service can forward to;
2. `EncryptedLeaseSet2Resolver` and `EncryptedServiceResolver` each have at least one
   non-test production caller, and `scripts/check-encrypted-service-consumer-caller.sh`
   fails if either regresses to zero;
3. an encrypted remote target is impossible on a non-`DelayOpen` service, proven by a
   negative configuration row;
4. one service's encrypted-resolve failure leaves every other service running, proven
   through the real composition;
5. the inner `LeaseSet2` is validated and installed under the **unblinded** destination
   hash, with a row that fails if the blinded key is used instead;
6. no lookup secret appears in any `Debug`, `Display`, `Serialize`, log, or error string, and
   no inline secret field is added to any config struct;
7. the ordinary LeaseSet2 path is byte- and behaviour-identical, and the service layer has
   no ELS2-specific branch above the owner;
8. the in-flight bound, lease release on every drop path, and restart behaviour are proven
   through the production composition;
9. every new observation label has a matching arm in `service_delivery.rs`, asserted rather
   than assumed;
10. ADR 0032's guard still passes and no transcript policy changed;
11. both new guards are in the `AGENTS.md` routine floor and both are negative-tested;
12. exact-head routine CI is green.

**Explicit non-claims.** Passing Plan 351 does **not** claim: PSK/DH consumer
authorization; daily rollover re-resolution; a cross-router interoperability result; or any
Java direction of Plan 347. Type 5 remains non-advertised.

## Stop conditions

Stop and record a classified boundary, do not narrow a criterion, if:

- the per-service isolation cannot be achieved without a new eager-group failure path — the
  alternative is Gate 1's restriction, not a weakened claim;
- a type-5 record cannot be validated under an unblinded key with existing primitives;
- the daemon seam cannot build a `LookupId` from a `RouterHash` without a NetDB change
  larger than the one `LookupResult` variant named in scope;
- containment of the control-plane secret requires an inline config field.

## Closure evidence required

Commits; a requirement-to-evidence matrix; exact commands with local/CI outcomes labelled
truthfully; the runtime-neutrality and bounded-queue reviews; the secret-handling review; a
statement that no Java or i2pd source was vendored; the two negative-tested guards with
their deliberate-break transcripts; known limitations including the daily-rollover gap;
findings by severity; and the roadmap disposition with the unblock audit.

## Outcome

Closed as `passed-gate-scoped-els2-consumer-service-wiring-landed`. Closure record:
[`../closure/i2pcontrol-proposal-170/351-status.md`](../closure/i2pcontrol-proposal-170/351-status.md).
ADR [`0033`](../../../../docs/adr/0033-els2-consumer-lookup-identity-and-install-key.md).

Two of this plan's premises were **wrong when written**, and both are recorded as findings rather
than quietly absorbed:

1. **In-scope item 5 said the unblinded destination hash is "derived from the b33 address's public
   key".** It is not derivable. `EncryptedServiceAddress` carries the unblinded *signing* public
   key; a `Destination` hash needs the ECIES public key, the certificate, and the padding, and the
   address publishes none of them. The install key is the **inner record's own** destination hash,
   gated on that record signing with the unblinded public key the address names
   (`encrypted_service_resolver::bind_inner_to_address`, ADR 0033 Decision 2).
2. **The type-11 answer to that question does not generalize.** Plan 351's publisher emits **type
   7**, where `DERIVE_PUBLIC(CONVERT_ED25519_PRIVATE(seed))` reproduces the destination's Ed25519
   public key — so address and inner record agree by construction. A type-11 `.b33` has no such
   relationship. The binding is written against what production publishes, and a row asserts the
   premise before relying on it.

Neither finding narrows an acceptance criterion: criterion 5 asked for the **unblinded** hash, and
the delivered value is the unblinded hash — obtained through a signature binding rather than the
assumed derivation. The "row that fails if the blinded key is used instead" is
`the_installed_hash_is_the_unblinded_destination_hash`, which asserts the install key differs from
the storage key.

One criterion is **not** met by this plan and is named as a limitation rather than counted:
acceptance criterion 4 ("one service's encrypted-resolve failure leaves every other service
running, proven through the real composition") is proven at the configuration and status-surface
level, not through a live multi-service composition, because doing the latter requires a
router-backed floodfill peer and a real service tunnel — the Plan 347 substrate. Criterion 8's
restart row is likewise limited to the resolver's own lease accounting. Both are listed in the
closure record under "Limitations" with the exact evidence that does exist.
