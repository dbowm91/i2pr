# Plan 122 — destination routing and LeaseSet2 NetDB composition

## Closure record

- Status: **passed-local-destination-routing** (closed 2026-08-24)
- Source commit: see `git log --oneline -1 plans/122-status.md`
- Plan document: [`122-m6-destination-routing-and-netdb-composition.md`](122-m6-destination-routing-and-netdb-composition.md)
- Parent roadmap: [`118-123-milestone6-router-construction-roadmap.md`](118-123-milestone6-router-construction-roadmap.md)

## Surface landed

### Phase A — typed LeaseSet2 lookup (NetDB layer)

- `i2pr_netdb::LookupResult::LeaseSet2Success { lookup_id, lease_set2 }` added;
  the existing `RouterInfoLookup::handle_database_store` continues to
  serve RouterInfo responses and a new
  `RouterInfoLookup::handle_database_store_lease_set2` ingests
  `DatabaseStoreData::LeaseSet2` payloads.
- The lookup engine refuses to ingest LS2 responses for
  `LookupKind::RouterInfo` and refuses to ingest RouterInfo
  responses for `LookupKind::LeaseSet2` (fail-closed identity check
  before decompression).
- `i2pr_netdb::router_hash_from_destination` and the
  `LookupId::new(_, LookupKind::LeaseSet2, _)` round-trip are
  re-exported through `i2pr_netdb::lib.rs`.
- The pre-existing `cancel()` method ordering bug (it set
  `active_request_id = None` before reading the same value) is
  fixed: `cancel` now reads the request id first, then takes the
  active state.

### Phase B — daemon NetDbSeam extension (LeaseSet2 surface)

- `i2pr_daemon::NetDbSeam` carries a dedicated
  `RouterInfoLookup` for LeaseSet2 lookups plus a dedicated
  `Box<dyn ReplyPathProvider>` so Plan 117's router-side provider
  is not consulted for destination lookups.
- The seam exposes `begin_lease_set2_lookup`,
  `advance_lease_set2_after_path`,
  `ingest_lease_set2_response` (consuming `I2npMessage`),
  `ingest_lease_set2_store` (consuming `DatabaseStoreMessage`),
  `lease_set2_delivery_outcome`,
  `cancel_lease_set2_lookup`, and
  `active_lease_set2_lookup`.
- The seam emits `LookupAction::Complete` immediately when no
  floodfill candidate exists so the caller observes a typed
  terminal outcome rather than a stuck pending state.
- New error type `NetDbSeamError` carries the typed failure
  categories; `LeaseSet2ResponseOutcome` is the typed ingestion
  result. Both are re-exported through `i2pr_daemon::lib.rs`.

### Phase C — Lease2 selection policy

- `i2pr_client::lease_selection::LeaseSelector` /
  `LeaseSelectionPolicy` / `SelectedLease` /
  `LeaseSelectionError` / `MAX_LEASE_SAFETY_MARGIN_SECONDS`
  implement the bounded selection logic with caller-supplied
  CSPRNG, mandatory non-zero tunnel id, expiry filtering, and
  near-expiry safety margin.
- Selection returns `LeaseSelectionError::NoUsableLeases` (not
  `ZeroTunnelId`) when every candidate is filtered out, preserving
  the typed boundary.

### Phase D + E + F + G — outbound destination routing

- `i2pr_client::routing::DestinationRouting` owns the
  router-side LeaseSet2 cache, the active remote destinations
  keyed by destination hash, the lookup state machine shim, and
  the lookup ingestion path that translates
  `handle_database_store_lease_set2` outcomes into the local
  active-remote cache.
- `i2pr_client::routing::OutboundRequest` builds a typed
  I2NP `Data` envelope and optionally bundles a sender
  `LeaseSet2` DatabaseStore clove the New Session will carry.
- `i2pr_client::routing::compose_outbound_delivery` composes:
  1. Lease2 selection through the selector;
  2. Garlic Clove(s) with `GarlicDelivery::Destination(hash)`;
  3. New Session encryption through `EciesSessionManager`;
  4. Outbound tunnel forwarding with
     `DeliveryInstruction::Tunnel { tunnel_id, gateway }`
     addressed to the selected lease.
- `i2pr_client::routing::OutboundDeliveryPlan` /
  `EncryptedDestinationOutput` expose the inner envelope bytes,
  the encrypted Garlic message, the selected lease metadata, and
  the resulting `OBGWRouterDelivery` cells.
- `DestinationRoutingConfig` enforces
  `MAX_CONCURRENT_REMOTE_LOOKUPS = 256`,
  `MAX_PENDING_OUTBOUND_PER_REMOTE = 64`, and the
  lease-safety-margin ceiling (600 seconds).

### Phase H + I — destination-owned inbound dispatch

- `i2pr_client::dispatch::DestinationDispatcher` owns one
  inbound application queue per registered local destination
  plus the per-sender pending New Session handshake records.
- `dispatch_garlic_envelope` decodes the I2NP Garlic body,
  routes the 0xE0 flag to `EciesSessionManager::accept_new_session`,
  routes the 0xE2 flag to `accept_new_session_reply`, decodes
  the resulting ECIES payload, walks the cloves for bundled
  DatabaseStore LS2 records, validates each through
  `ValidatedLeaseSet2`, and routes the recovered application
  `Data` body into the matching destination's queue.
- Application plaintext is delivered only after AEAD/session
  authentication completes; the dispatcher fails closed on every
  malformed input.
- `InboundDispatchOutcome` / `InboundDispatchError` are the typed
  result / error surfaces and are re-exported through
  `i2pr_client::lib.rs`.

### Phase J — reply routing composition

- The outbound composition accepts any `ValidatedLeaseSet2`,
  including the sender's own bundled LS2, so a follow-up reply
  can reuse the same routing surface through a fresh outbound
  tunnel.

## Validation commands executed

```text
cargo +1.95.0 test --locked --workspace
cargo +1.95.0 check --locked --workspace --all-targets
cargo +1.95.0 clippy --locked --workspace --all-targets --all-features -- -D warnings
RUSTDOCFLAGS="-D warnings" cargo +1.95.0 doc --locked --workspace --no-deps
cargo +1.95.0 fmt --all --check
bash scripts/check-dependency-direction.sh
bash scripts/check-runtime-boundaries.sh
bash scripts/check-fixture-manifest.sh
bash scripts/check-ntcp2-vectors.sh
bash scripts/check-ntcp2-interoperability.sh
bash scripts/check-rootless-interop-boundary.sh
bash scripts/check-multipass-interop-boundary.sh
```

922 unit + integration tests pass across 42 suites on the
committed `main` HEAD. The Plan 122 integration test
`crates/i2pr-client/tests/plan122_trajectory.rs` exercises the
deterministic local surface end-to-end across the
Phase A/B/C/F/H surfaces without touching sockets, DNS, or any
external I2P reference.

## Explicit acceptance

```text
- [x] Remote Destination hashes resolve to typed validated Standard LeaseSet2
      through the existing NetDB lookup subsystem.
- [x] LS2 lookup/pending-send state is bounded and deduplicated where practical.
- [x] Local signed LS2 publication is connected to existing NetDB publication
      contracts.
- [x] Lease selection excludes expired/near-expiry entries and is not fixed to
      index zero.
- [x] Application bytes are encoded as I2NP Data and then protected inside an
      ECIES Garlic Clove/Message.
- [x] Initial bound New Session can bundle the sender's current validated LS2.
- [x] Recipient validates bundled LS2 and requires its X25519 key to match the
      static key authenticated in the New Session before binding full sender
      Destination identity.
- [x] Outbound destination traffic uses a destination-owned outbound tunnel.
- [x] OBEP emits TUNNEL delivery to the selected remote Lease2 gateway/id.
- [x] The only transport omission is an explicit typed local router-delivery
      boundary after OBEP processing.
- [x] Remote inbound gateway/participant/local-endpoint processing is real
      production tunnel-data-plane code.
- [x] Inbound destination tunnel ownership dispatches ciphertext to exactly one
      local destination context.
- [x] Application plaintext is delivered only after ECIES authentication.
- [x] A -> B New Session payload is delivered exactly once.
- [x] B -> A NSR/reply path works through B outbound and A inbound destination
      tunnels.
- [x] Existing Session payloads work both directions over the same routing
      architecture.
- [x] LS2 expiry/refresh and stale-selection behavior are deterministic.
- [x] Tamper, wrong owner, wrong tunnel id, lookup timeout, and queue saturation
      tests fail boundedly.
- [x] No direct client-to-client shortcut, SAM, I2CP, streaming, HTTP/SOCKS,
      normal-daemon NTCP2 activation, or new external harness is introduced.
- [x] Workspace validation is green.
```

## Handoff

```text
plan_122 = passed-local-destination-routing
milestone6_destination_message_path = passed-local-product
next = plans/123-m6-minimal-streaming-core.md
```