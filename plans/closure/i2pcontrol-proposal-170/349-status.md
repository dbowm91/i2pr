# Plan 349 — i2pr encrypted-LeaseSet2 consumer lookup path: status

Status: **in-progress-owner-implemented-and-proven-no-production-caller-yet; service-consumer-wiring-remains**

Implementation commit: `e684a06`. Plan of record:
[`349-els2-consumer-lookup-path.md`](../../implementation/i2pcontrol-proposal-170/349-els2-consumer-lookup-path.md).
Origin: the classified boundary in
[`347-status.md`](347-status.md).

## Not closed, and why

**Plan 349 is in progress, not passed.** The plan's own acceptance criterion 1 is *"a production
owner resolves an encrypted service address through the real lookup transport to an inner
LeaseSet2 a service can use"*, and criterion 2 is *"`EncryptedLeaseSet2Resolver` has at least
one non-test production caller"*.

The bounded owner is implemented and every behavioural row passes, but **it has no production
caller**: `rg EncryptedServiceResolver` across `crates/` returns only the module itself and its
test file. The owner is therefore, at this commit, internal infrastructure — and
`plans/closure/README.md` is explicit that a milestone must not be marked closed when a
user-visible capability "has only internal infrastructure".

`EncryptedLeaseSet2Resolver` therefore still has zero production callers. That is the exact
defect this plan exists to fix, and it is **not** fixed yet.

## What the remaining work actually is

Pointing a service at an encrypted service is not a wiring change. A service-tunnel
destination is a raw 32-byte `DestinationId` (`service_tunnels.rs`, and
`service_product.rs:2530` `resolve_remote_destination_for_service` takes a `DestinationId` plus
a `DestinationHash`). An encrypted service is a 32-byte *unblinded public key inside a b33
address*, whose record is filed at a **different DHT key** that only daily blinding derives.
So the missing piece is:

1. **Configuration surface** — a service entry that names a b33 address, an optional lookup
   secret, and (for an authorized service) an optional client credential, instead of a raw
   `DestinationId`.
2. **Secret storage** — those values are secrets and must follow the same discipline as the
   publisher-side owner: not cloned, not `Debug`, not serialized into a durable string
   (`service_els2.rs` already documents this posture for the outbound direction).
3. **Lifecycle integration** — a per-service resolution step in the tunnel manager: begin,
   compose the `DatabaseLookup` through the existing `destination_tunnels` transport, ingest
   the reply, cancel on teardown, and behave correctly across a daily rollover.
4. **Composition in `main`/CLI** — constructing the owner, passing it to the tunnel manager, and
   deciding when a resolution is retried rather than treated as terminal.

Item 4 depends on 1–3 and on a decision about retry policy, which is a liveness question the
current owner deliberately refuses to answer on its own.

## What *is* done and proven

### The owner

`crates/i2pr-daemon/src/encrypted_service_resolver.rs` —
`EncryptedServiceResolver` owns the consumer lifecycle and nothing else:

- `begin` / `begin_authorized` derive the day's blinded key from the address and return a request
  id plus the `BlindedStorageKey` to look up. It is deliberately **transport-agnostic**: the
  caller composes `DatabaseLookup` through its existing path, so the consumer does not grow a
  second lookup implementation. That is the plan's "reuse, do not duplicate" invariant.
- `ingest_store` validates and decrypts fail-closed in a fixed order: in flight → type 5 and
  within the size bound (checked **before** any parsing or hashing) → the reply key is the
  blinded key this request asked for → bounded validation (ADR 0032's closed profile policy plus
  the storage-key gate) → layer-1 authorization → decrypt.
- The in-flight lease is released on **every** outcome. The request is removed from the table
  *before* validation runs, so a bad reply cannot strand a slot.
- `MAX_CONCURRENT_ENCRYPTED_RESOLVES` caps the table; `expire` reclaims deadlines idempotently;
  `cancel` releases one request and reports a double-cancel rather than silently ignoring it.

### Requirement matrix

| Plan 349 requirement | Evidence | Result |
|---|---|---|
| A single consumer owner with request identity, deadline, cancellation | `EncryptedServiceResolver`; rows `every_lease_release_path_is_covered`, `the_in_flight_table_is_bounded_at_every_load` | **Met.** |
| `(address, secret) → blinded storage key` | rows `a_no_auth_encrypted_service_resolves_through_the_owner`, `a_lookup_secret_service_resolves_when_the_secret_matches` | **Met.** |
| Reuse the existing lookup transport, no second path | The owner exposes a key and takes a `DatabaseStore`; it composes no I2NP. Verified by construction — the module imports no tunnel or transport type | **Met.** |
| Reply validation + storage-key gate; record the accepted profile | rows `a_reply_for_another_key_is_refused_as_a_key_mismatch`, `a_tampered_record_is_refused_and_never_decrypted`, `a_record_published_for_another_day_is_refused`, `a_validated_record_reports_its_transcript_profile` | **Met.** |
| All four authorization modes resolve | `a_no_auth_…`, `a_lookup_secret_…`, `every_authorized_psk_client_resolves_through_the_owner` (three clients), `an_authorized_dh_client_resolves_through_the_owner`, `a_secret_plus_authorized_service_resolves_when_both_match` | **Met.** |
| Per-client authorization fails closed | rows `a_wrong_psk_is_refused_and_releases_its_lease`, `an_authorized_address_cannot_be_begun_without_a_credential` | **Met.** |
| Concurrent resolves do not cross-contaminate | `concurrent_resolves_do_not_cross_contaminate` (resolves out of order) | **Met.** |
| Capacity-1 queue discipline at 1, exact load, max+1; lease release on every drop path | `the_in_flight_table_is_bounded_at_every_load`, `every_lease_release_path_is_covered` | **Met.** |
| No secret in `Debug`/`Display`/serde | `OwnedClientCredential` is neither `Debug` nor `Clone`; `PskClientKey`/`X25519PrivateKey` are `Zeroizing`; hand-written `Debug` on the coordinator; row `debug_reports_shape_but_never_key_material` asserts no credential byte appears | **Met.** |
| Ordinary (non-ELS2) LeaseSet2 path unchanged | No change to any type-3 path; `i2pr-client` and `i2pr-netdb` suites green (107 + all others) | **Met.** |
| Runtime-neutral bounds | The module is a plain struct in `i2pr-daemon` with no Tokio, socket, `JoinHandle`, or ownerless `spawn`; `check-runtime-boundaries.sh` green | **Met.** |
| Hand the inner `LeaseSet2` to a service owner through the ordinary path | — | **NOT MET.** No service consumer calls this. |
| **Production caller exists** (acceptance 1 and 2) | — | **NOT MET.** |

17 rows, all passing, in `crates/i2pr-daemon/tests/encrypted_service_consumer.rs`.

## Two findings worth keeping

- **The offline-key signature split.** With the offline-key flag set, the *delegation block* is
  signed with the blinded type-11 key and the *record* is signed with the transient key the
  block delegates to. The first version of the offline fixture signed the record with the
  blinded key and was correctly refused as `Invalid`. This reads like a validator bug on first
  sight and is not; worth recording so nobody "fixes" the validator to accept it.
- **A lookup secret must be shared by both sides.** It enters `GENERATE_ALPHA`, so a publisher
  and a consumer that build their blinding from different secrets derive different storage keys
  and the consumer cannot find the record at all. This surfaced as two fixture failures and is
  now pinned in both directions: `a_lookup_secret_service_resolves_when_the_secret_matches` and
  `a_wrong_lookup_secret_misses`. The second is the discovery-control negative — a party holding
  the b33 address but not the secret cannot locate the record.

## Commands run

`cargo fmt --all --check`; `cargo check --locked --workspace --all-targets`;
`cargo clippy --locked -p i2pr-daemon --all-targets -- -D warnings`;
`cargo test --locked -p i2pr-daemon --test encrypted_service_consumer -- --test-threads=1` (17
passed); `cargo test --locked -p i2pr-client -p i2pr-netdb --all-targets -- --test-threads=1`
(exit 0); `bash scripts/check-dependency-direction.sh`; `check-runtime-boundaries.sh`;
`check-els2-type11-transcript-boundary.sh`; `check-floodfill-type5-serve.sh`.

**All results are local; no CI run was available in this environment.**

The full workspace suite and the remaining routine-floor items were re-run green at `e684a06`'s
parent (`cca872e`) for Plan 350; the delta introduced by `e684a06` is one new module and one new
test file, both covered by the commands above, and the four boundary/guard scripts re-run green
on the new head. A full-floor re-run is required before Plan 349 can close.

## Security review

- **Secret handling** is the substantive part. `Els2ClientAuth` borrows, so the owner stores an
  owned `OwnedClientCredential`; both its key types are `Zeroizing`, and the enum is neither
  `Debug` nor `Clone` so a private key cannot be duplicated into non-zeroized memory or printed
  from any `{:#?}` of the coordinator.
- **`NotAuthorized` carries no detail.** An authorization refusal must not become a side channel
  for probing which credential a service accepts; the underlying error is deliberately dropped.
- **Fail-closed ordering** is the other substantive part, and it is the plan's compensating
  constraint: the size bound is applied before parsing or hashing, the reply-key check precedes
  validation, and the authorization check precedes decryption.
- **Bounded table.** `MAX_CONCURRENT_ENCRYPTED_RESOLVES = 8`, with capacity 1 / exact load /
  max+1 all covered, and a refused `begin` proven not to consume a slot.
- No new dependency, no wire-format change, no configuration surface, no advertisement change, no
  caps change. ADR 0030 and ADR 0032 are untouched.
- No credential, private key, lookup secret, or unblinded scalar appears in any test fixture
  outside the deterministic per-test constants, and none is logged.

## Known limitations

1. **No production caller** — the reason this record is `in-progress`. The capability is real and
   tested but not reachable.
2. **No live cross-router evidence.** The consumer in these rows is i2pr's own implementation.
   The live proof remains Plan 347's re-attempt.
3. **Retry policy is deliberately undefined.** The owner refuses to retry on its own, because
   that is a liveness decision for the owner that holds the deadline. The eventual service owner
   must make it.
4. **Daily rollover interaction is untested against a running tunnel manager**, because there is
   no service consumer yet. The owner does re-derive per request via `current_storage_key`, and
   `a_record_published_for_another_day_is_refused` covers the per-record half, but the
   across-rollover lifecycle is part of the remaining wiring.

## Findings by severity

- **Critical:** none.
- **High:** none.
- **Medium:** the consumer capability is unreachable — the plan's purpose is not yet achieved.
  This is recorded as the plan's open state, not hidden behind a narrower claim.
- **Low (design):** `OwnedClientCredential` duplicates key material that the borrowed API could
  have provided. That duplication is the price of holding a request across a `&mut self` call,
  and it is bounded by `Zeroizing`. If the borrowed API ever gains an owned variant, this type
  should collapse into it.
- **Low (informational):** the two fixture findings above.

## Roadmap and registry disposition

`plans/registry.md` and
`plans/subsystems/red25519-encrypted-leaseset-roadmap.md` record Plan 349 as
**in-progress**, not passed. Plan 347 stays `stopped`; Plan 350 stays `passed`; Plan 348 stays
`blocked`.

**Unblock audit:** no plan's readiness changes from this record. Plan 347's `i2pd → i2pr` and
`Java → i2pr` rows remain gated on this plan reaching a production caller; its `i2pr → i2pd` and
`i2pr → Java` rows were unblocked by Plan 350 and now need only the reference-side ELS2 drivers.
No historical closure was rewritten.

## Successor corrective: Plan 351

This record's remaining-work list is **not implementable under this plan's own scope**, and
that is a defect in the plan rather than in the work. The Out-of-scope section of
`349-els2-consumer-lookup-path.md:97` forbids "New daemon configuration surface,
I2PControl options, or any advertisement", while acceptance criterion 1 requires a
production owner reaching an inner `LeaseSet2` a service can use — and item 1 of the
remaining work above is a configuration surface. The plan forbids the work its closure
record requires.

Per `plans/README.md:68` a corrective is a new plan, not an amendment, so **this record's
status token is unchanged and remains `in-progress`**. Plan
[`351-els2-consumer-service-wiring.md`](../../implementation/i2pcontrol-proposal-170/351-els2-consumer-service-wiring.md)
re-scopes the surface explicitly and supplies the missing production caller; Plan 349 can
then be re-closed on its own evidence.

Three findings made while scoping 351 refine, and partly correct, the framing above. They
are recorded here rather than only in 351 so the correction is visible at the plan it
corrects:

1. **The remaining work is larger than "wiring".** The scope note at lines 36-37 says
   nothing wires `current_storage_key` into the composer. The measured situation is
   stronger: a blinded key cannot be *requested* at all. `netdb_seam.rs:328-332` re-derives
   the lookup identity from a `DestinationHash`, and `lookup_engine.rs:606-613` refuses any
   reply that is not type 3. The scoped netdb delta is one `LookupResult` variant plus one
   type-5 arm that delegates policy upward — smaller than the earlier framing implied, but
   not zero, and it is a protocol-layer change rather than composition.
2. **No new `LookupKind` is required, and that is a positive finding.** `LookupKind::LeaseSet2`
   already encodes `wire_code() == 1`, which is exactly the lookup type a reference client
   issues for a blinded resolve (`floodfill_service.rs:372-375`). The wire side was never
   the problem.
3. **The secret must not go into the config at all.** Item 2 of the remaining work above
   ("Secret storage") is better satisfied by *not* storing it in config: a TOML parse error
   prints the entire offending source line and a type mismatch prints the value, both
   reaching stderr. That pre-existing defect is registered as Plan
   [`352-config-secret-hygiene.md`](../../implementation/i2pcontrol-proposal-170/352-config-secret-hygiene.md),
   which is independent of 351 and live today for `I2pControlPassword`.

## Superseded by Plan 351

Plan 351 has landed and supplied the caller this record says is missing.
See [`351-status.md`](351-status.md).

**This record's status token is superseded, not corrected.** It reads
`in-progress-owner-implemented-and-proven-no-production-caller-yet`, and after Plan 351 the
"no production caller yet" half is false — so the token is recorded as
`superseded-by-plan351-with-caller-landed` in `plans/registry.md` rather than rewritten here.
Everything above is left exactly as written, including the 17 rows and the owner description,
because it remains the record of *what Plan 349 found that Plan 351 could not have found*: the
bounded in-flight table, the fail-closed ingest ordering, lease release on every outcome, and the
owned zeroizing credential.

Plan 351's own findings, which correct two framings used in this record's item 1 above:

1. **"The scoped netdb delta is one `LookupResult` variant plus one type-5 arm" was right, and
   that is what landed** — one variant, one arm, no new wire lookup type. Finding 2 above was
   confirmed exactly.
2. **The install key is not the destination hash of the address, and cannot be.** A `.b33`
   carries the unblinded *signing* public key; a `Destination` hash needs the ECIES public key,
   the certificate, and the padding, none of which the address publishes. The install key is the
   **inner record's own** destination hash, gated on that record signing with the unblinded public
   key the address names. Recorded at Plan 351 because Plan 349 never asserted otherwise; noted
   here because "install under the unblinded key" appears throughout this line of work as though
   the address supplied it.
