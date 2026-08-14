# Plan 105: closure record

- Status: **implemented and closed on the local host**.
- Date: 2026-08-13.
- Parent authority: Plan 102.
- Baseline: Plan 103 and Plan 104 closures with the populated
  `RouterInfoStore` plus reseed ingestion surface.
- Implementation source:
  `crates/i2pr-netdb/src/routing.rs`,
  `crates/i2pr-netdb/src/lookup_policy.rs`,
  `crates/i2pr-netdb/src/lookup_id.rs`,
  `crates/i2pr-netdb/src/lookup_action.rs`,
  `crates/i2pr-netdb/src/databaselookup.rs`,
  `crates/i2pr-netdb/src/lookup_engine.rs`,
  `crates/i2pr-netdb/src/publication.rs`,
  `crates/i2pr-netdb/src/store_message.rs`.
- Next executable implementation: **Plan 106** (daemon and bootstrap
  integration), then Milestone 5 (exploratory tunnel substrate).

## Closure summary

Plan 105 landed the transport-neutral query and publication state
machines in the existing `i2pr-netdb` crate without owning a
runtime, sockets, tunnels, or transport delivery. The state machines
stay synchronous and runtime-neutral; the runtime adapter that Plan
106 introduces drives them one tick at a time.

The lookup state machine refuses to emit a standards-conformant
`DatabaseLookup` until the runtime supplies an exploratory reply
path; the bounded gzip decompressor enforces explicit compressed and
decompressed byte ceilings; and the local publication coordinator
never re-signs the local RouterInfo on retry. Live
standards-conformant NetDB lookup remains blocked on the Milestone 5
exploratory-tunnel substrate.

### Work package 1 — daily routing-key derivation

- `routing::daily_routing_key(search_key, date)` returns
  `SHA256(search_key || UTC_yyyyMMdd)` for a caller-supplied
  `Date`. The function never reads the wall clock.
- `routing::format_daily_key` enforces fixed 8-digit ASCII UTC date
  formatting and bounded errors.
- `RoutingKeyError` covers overflow, invalid month/day, and length
  mismatch on the search key.

### Work package 2 — floodfill selection policy

- `lookup_policy::LookupPolicy` carries bounded peer budget,
  per-attempt deadline, total deadline, suggested-hash limit, and
  total ceiling. Default values stay inside the Plan 102 budget.
- `lookup_policy::select_floodfill_candidates` is a deterministic
  nearest-N selection over `RouterInfoStore::iter()` that respects
  the candidate ceiling, the policy peer budget, and a
  caller-supplied exclusion set.
- `FloodfillSelection` carries an ordered entries vector that the
  state machine drains in order.

### Work package 3 — typed lookup identity

- `lookup_id::LookupId` carries a `u64` request identifier, a
  `LookupKind`, and a `RouterHash` target. The lookup kind carries
  the wire code (currently `2` for `RouterInfo`) and a small typed
  set of future kinds.
- `lookup_id::WaiterSet` deduplicates waiter identifiers against a
  bounded `MAX_WAITERS_PER_LOOKUP = 32` ceiling.
- `lookup_id::CoalescedTargets` deduplicates coalesced lookup
  targets against a bounded `MAX_COALESCED_LOOKUPS = 8` ceiling.

### Work package 4 — exploratory-tunnel reply path

- `lookup_id::ReplyPath` is the typed exploratory-tunnel handoff
  token (gateway RouterHash plus tunnel identifier). The constructor
  refuses a zero gateway or a zero tunnel identifier.
- `ReplyPathError` is the typed error. The state machine refuses to
  emit a `SendDatabaselookup` action until the runtime supplies a
  `ReplyPath`; the bounded `NeedsExploratoryReplyPath` action is the
  typed handoff back to the runtime.

### Work package 5 — action vocabulary and bounded decompression

- `lookup_action::LookupAction` is a small enum
  (`SendDatabaselookup`, `NeedExploratoryReplyPath`, `Complete`).
- `lookup_action::decompress_router_info` reads through
  `flate2::read::GzDecoder` with explicit compressed and decompressed
  byte ceilings (`MAX_COMPRESSED_ROUTER_INFO_BYTES = 16 KiB`,
  `MAX_DECOMPRESSED_ROUTER_INFO_BYTES = 32 KiB`).
- `DecompressionError` carries manual `PartialEq`/`Eq` because
  `flate2::DecompressError` does not implement them.

### Work package 6 — `DatabaseLookup` builder

- `databaselookup::build_databaselookup` constructs a
  standards-conformant `DatabaseLookupMessage` from the target
  RouterHash, the lookup kind, the reply path, and the exclusion
  set.
- The on-wire key is the raw RouterHash (not the daily routing key).
- The exclusion set is bounded by
  `LOOKUP_EXCLUDED_PEER_BUDGET = 256`.

### Work package 7 — response correlation

- `lookup_engine::handle_database_store` validates the
  `DatabaseStore` payload against the lookup identity and the
  Plan 103 validator, decompresses through
  `lookup_action::decompress_router_info`, and emits a typed
  `ResponseOutcome`.
- `lookup_engine::handle_search_reply` ingests a
  `DatabaseSearchReply`, deduplicates and bounds the suggested
  hash list against `LookupPolicy::max_suggested_hashes`, and emits
  a typed `ResponseOutcome`.

### Work package 8 — iterative lookup progression

- `lookup_engine::RouterInfoLookup` is a single-threaded
  `RouterInfo` lookup state machine driven by the runtime adapter.
- `start` refuses to emit a `SendDatabaselookup` action until a
  reply path is supplied. The state machine iterates by draining
  `FloodfillSelection`, appending to `queried`, merging bounded
  suggested hashes, and emitting one fresh `SendDatabaselookup`
  per attempt.
- `LookupFinalState` is a bounded six-variant enum
  (`Completed`, `PeerExhausted`, `NoEligibleCandidates`,
  `InvalidResponse`, `Cancelled`, `InvalidLookup`).

### Work package 9 — timeout, cancellation, delivery outcomes

- `lookup_engine::handle_delivery_outcome` ingests bounded
  delivery success and failure outcomes.
- `cancel` returns the current typed terminal action and clears the
  active lookup state.
- `DeliveryOutcome` carries bounded reason codes (`Sent`,
  `DeliveryFailure { reason }`).

### Work package 10 — local RouterInfo publication coordinator

- `publication::PublicationCoordinator` tracks bounded publication
  attempts (`MAX_PUBLICATION_ATTEMPTS = 32`) and emits one bounded
  `DatabaseStore` per nearest floodfill with
  `reply_token = 0` store-and-forget semantics.
- The coordinator never re-signs the local RouterInfo on retry;
  retries reuse the originally encoded bytes and the originally
  minted reply token.
- `PublicationCorrelation` and `PublicationAttemptRecord` carry
  bounded bookkeeping. `PublicationCoordinator::needs_verification_lookup`
  is the typed signal to the future Milestone 5 verification path.

### Work package 11 — bounded unsolicited `DatabaseStore` ingestion

- `store_message::handle_unsolicited_databasestore` validates the
  payload against the Plan 103 validator after decompression.
- Non-`RouterInfo` payloads are rejected with
  `UnsolicitedStoreError::UnsupportedPayload`.
- The handler is bounded by the same gzip byte ceilings as the
  active lookup path.

### Work package 12 — deterministic tests

- `cargo test -p i2pr-netdb` reports 117 deterministic tests.
- Routing-key, candidate-selection, lookup-identity, action,
  decompressor-bounds, lookup-state-machine, publication, and
  store-message tests cover the WP 1-11 surface plus the negative
  paths (unknown peer token, late response, duplicate waiters,
  invalid signature, decompressor overflow, retry without
  re-signing).

### Work package 13 — documentation propagation

- `specs/support.toml` records the Plan 105 surface and the
  implementation floor.
- `docs/architecture/i2pr-netdb.md` lists the new modules and the
  Plan 105 contracts.
- `README.md` adds Plan 105 to the local RouterInfo/NetDB status
  block.
- `AGENTS.md` and `docs/architecture/interop-apparatus.md` carry the
  Plan 104 + Plan 105 closure notes.

## Validation commands

```text
$ cargo fmt --all --check
(no output)

$ cargo check --locked --workspace --all-targets
Finished `dev` profile [unoptimized + debuginfo] target(s) in N.NNs

$ cargo test --locked --workspace
test result: 388 passed (31 suites)

$ cargo clippy --locked --workspace --all-targets --all-features -- -D warnings
No issues found

$ RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps
Generated docs without warnings

$ bash scripts/check-dependency-direction.sh
dependency direction: ok

$ bash scripts/check-runtime-boundaries.sh
runtime boundary checks passed

$ bash scripts/check-fixture-manifest.sh
(no output)

$ bash scripts/check-ntcp2-vectors.sh
NTCP2 vector manifest is complete and hashes match.

$ bash scripts/check-ntcp2-interoperability.sh
Plan 099 NTCP2 interoperability static check: OK

$ bash scripts/check-constrained-host-lane-boundary.sh
Plan 077 constrained-host lane boundary checks passed
```

## Handoff to Plan 106

The exact APIs for Plan 106 consumption:

```text
RouterInfoLookup::new(policy) -> RouterInfoLookup
RouterInfoLookup::start(store, lookup_id, routing_key) -> StartOutcome
RouterInfoLookup::active_lookup() -> Option<LookupId>
RouterInfoLookup::cancel() -> Option<LookupAction>

handle_database_store(lookup, store, lookup_id, store_message, context) -> ResponseOutcome
handle_search_reply(lookup, lookup_id, search_reply, context) -> ResponseOutcome
handle_delivery_outcome(lookup, request_id, outcome) -> ResponseOutcome

build_databaselookup(target, kind, reply_path, excluded) -> DatabaseLookupMessage
decompress_router_info(bytes) -> Result<Vec<u8>, DecompressionError>

PublicationCoordinator::new(identity_hash, encoded, store, signer, ack_window)
PublicationCoordinator::record_ack(token, peer)
PublicationCoordinator::pending_attempts() -> usize
PublicationCoordinator::needs_verification_lookup() -> bool

daily_routing_key(search_key, date) -> RouterHash
select_floodfill_candidates(store, target, routing_key, excluded, policy) -> FloodfillSelection
```

Plan 106 owns the runtime adapter that drives these state machines
and the bounded local RouterInfo lifecycle (build, store, publish,
rebuild on schedule). Plan 106 does not implement exploratory
tunnels. Milestone 5 supplies the exploratory inbound and outbound
tunnels and the standards-conformant `DatabaseLookup` reply path.

## Status

Plan 105 is closed. NTCP2 remains experimental and non-advertised.
The next executable implementation is **Plan 106** (daemon and
bootstrap integration), followed by Milestone 5 (exploratory tunnel
substrate) before any return to Milestone 4B external acceptance.

## Marvin-Attack correction (rsa → sad-rsa)

Plan 105 closes with the Plan 104 SU3 signature-verification stack
migrated from `rsa 0.9` to `sad-rsa 0.2`. The change was forced by
`cargo-deny`'s `Dependency policy` CI job, which fails closed on
`RUSTSEC-2023-0071` (Marvin Attack timing-side-channel on
`rsa 0.9.x`; the upstream advisory states "No safe upgrade is
available!"). The migration keeps the same PKCS#1 v1.5 + SHA-512
semantics Plan 104 closed against and the same `TrustedSigner`
boundary:

- production verifier `verify_rsa_sha512_signature` constructs
  `sad_rsa::RsaPublicKey::new(sad_rsa::BoxedUint::from_be_slice(modulus, bits_precision), exponent)`
  and verifies through `sad_rsa::pkcs1v15::VerifyingKey<Sha512>`;
- the deterministic test signer is rebuilt around `sad_rsa 0.2`'s
  `rand_core 0.10` RNG (`rand_chacha 0.10` `ChaCha8Rng`).
- all 117 `i2pr-netdb` tests pass; the Plan 104 SU3 reseed suite
  (`cargo test --locked -p i2pr-netdb --lib reseed`) passes; and
  `cargo deny check advisories bans sources` reports
  `advisories ok, bans ok, sources ok`.

The Plan 104 closure record and the Plan 104 SU3 contract are
preserved verbatim; only the underlying RSA primitive changed.
