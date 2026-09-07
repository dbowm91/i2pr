# Plan 166 status — Milestone 9 I2CP client-owned destination and LeaseSet2 bridge

Status: **`passed-m9-i2cp-client-owned-destination-and-leaseset2`**.

Registered: **2026-09-07**.

Plan of record:
[`plans/166-m9-i2cp-client-owned-destination-and-leaseset2.md`](166-m9-i2cp-client-owned-destination-and-leaseset2.md).

## Current authority

```text
plan_161 = passed-m8-ssu2-independent-ipv4-interop-and-final-closure
plan_162 = passed-m8-ssu2-external-test-lane-isolation-and-ci-restoration
plan_163 = registered-m9-i2cp-roadmap
plan_164 = passed-m9-i2cp-protocol-and-wire-foundation
plan_165 = passed-m9-i2cp-connection-session-and-options
plan_166 = passed-m9-i2cp-client-owned-destination-and-leaseset2

milestone8_final_acceptance = closed-via-plan161
milestone9_planning_authority = plan163
milestone9_wire_foundation = passed-via-plan164
milestone9_connection_session_options = passed-via-plan165
milestone9_client_owned_destination = passed-via-plan166
milestone9_final_acceptance = not-yet-closed
next_executable_plan = 167
next_product_layer = milestone9-i2cp
```

## What landed

### `i2pr-client` capability surface

`crates/i2pr-client/src/identity.rs`, `leaseset.rs`, `registry.rs`,
and `lib.rs` extend `i2pr-client` with the M9 client-owned destination
+ Standard LeaseSet2 bridge surface. The router-owned SAM path is
preserved unchanged; client-owned mode shares one destination
runtime, one tunnel pool, one ECIES session manager, and one
registry. New runtime-neutral capabilities (no Tokio, no sockets):

- `DestinationOwnership::{RouterOwned, ClientOwned}` — typed
  marker for the two ownership modes (Plan 166 §2).
- `DestinationPublic` — non-secret wrapper around the canonical
  `Destination`, the derived `DestinationId`, and the cached
  static X25519 public key bytes. Constructed only through
  `DestinationPublic::from_destination`, which rejects unsupported
  signing/encryption types and non-32-byte encryption fields.
- `InboundDecryptionCapability` — non-`Clone`, redacted-`Debug`,
  zeroized-on-drop wrapper for the X25519 inbound private key
  material a client supplies alongside a Standard LeaseSet2.
  `InboundDecryptionRef` exposes the public/secret bytes for the
  ECIES dispatcher without handing the secret wrapper over.

`LeaseSetLifecycle` gains the Plan 166 §3 client-owned parallel:

- `install_external` validates a client-supplied
  [`LeaseSet2`](https://docs.rs/i2pr-proto) against the destination
  hash, real inbound pool, lease expiry, lease ownership,
  duplicate detection, encryption key type, and decryption-key
  match (Plan 166 §6 steps 1–11). Every check fails before any
  state mutates; a mismatched decryption capability is rejected as
  a hard session error before publication.
- `refresh_client` records [`LeaseSetDecision::RequestClientRefresh`]
  (Plan 166 §5) without signing a replacement locally.
- `take_client_refresh_request` returns the typed `LeaseRequest`
  (sourced from the real inbound pool, deterministic, no synthetic
  gateways) plus the `ClientRefreshCause` enum.
- `ClientRefreshCause::{InitialGeneration, ApproachingExpiry}`
  replaces an integer status code; the daemon projects it through
  the typed `I2cpAction::RequestVariableLeaseSet` action unchanged.

`DestinationRuntime` exposes the Plan 166 §3 ownership branching:

- `new(identity, config)` / `with_shared_identity(arc, config)` —
  router-owned mode (SAM). Returns `Result<Self,
  DestinationRuntimeError>` after the `DestinationPublic` wrapper
  is constructed; the typed error union covers both
  `DestinationPoolError` and `DestinationIdentityError`.
- `new_client_owned(public, config)` — client-owned mode. The
  runtime stores no signing private key and reports
  `static_secret_bytes() == None` until a LeaseSet2 commits.
- `install_client_lease_set2(record, capability, now)` is the
  single transaction that wires a client-owned destination into the
  inbound ECIES path. Cross-checks the supplied capability public
  key against `DestinationPublic::static_public_bytes()`, then
  delegates to `LeaseSetLifecycle::install_external` for the lease
  + signature validation, then drops the capability into
  `self.decryption` only on full success.
- `take_client_refresh_request(now)` /
  `poll_client_refresh(now)` — drive the lease refresh decision
  and surface the typed `LeaseRequest` material the Plan 167
  daemon ships to the client through
  `I2cpAction::RequestVariableLeaseSet`.
- `decryption_capability()` returns the installed
  `InboundDecryptionCapability` reference (router-owned mode
  reports `None` and reads through the
  `DestinationIdentity::static_secret_bytes` accessor instead).
- `static_public_bytes()` / `static_secret_bytes()` route through
  the cached `DestinationPublic` so the ECIES dispatcher and the
  Plan 167 daemon never branch on ownership.
- `shutdown()` drops `self.decryption = None` so the
  `InboundDecryptionCapability` bytes are zeroized immediately.

`DestinationRuntimeError` grows an `Identity(#[from]
DestinationIdentityError)` variant so the unified constructor
error type covers both identity and pool failures.

### `i2pr-api` typed action material

`crates/i2pr-api/src/i2cp/actions.rs` grows by one variant:

- `I2cpAction::RequestVariableLeaseSet { connection, session,
  destination_hash, cause: LeaseRefreshCause, leases:
  Vec<LeaseRequestLease> }` — the typed action the Plan 167 daemon
  ships to the client when leases approach expiry. The lease
  material is sourced from the destination's real inbound tunnel
  pool; the router never synthesizes replacement leases.

`LeaseRefreshCause::{InitialGeneration, ApproachingExpiry}` and
`LeaseRequestLease` are exported from `i2pr_api::i2cp` alongside
`I2cpAction` so downstream consumers never translate an integer
status. The action carries only typed destination metadata; raw
client bytes never appear in the payload.

## Tests

`crates/i2pr-client/tests/plan166_trajectory.rs` covers every
Plan 166 §11 case in twelve unit tests:

| Case | Test |
| --- | --- |
| Client-owned construction without a signing secret | `client_owned_runtime_carries_no_signing_secret` |
| Public wrapper rejects unsupported encryption types | `unverified_public_with_wrong_encryption_curve_is_rejected` |
| Valid install with matching decryption key (atomic commit) | `install_client_lease_set2_with_matching_decryption_key_commits_atomically` |
| Mismatched decryption key is rejected atomically | `install_with_mismatched_decryption_key_is_rejected_atomically` |
| Foreign lease rejected | `install_with_foreign_lease_is_rejected` |
| Lease ownership after eviction rejected | `install_with_unknown_pool_lease_is_rejected_after_eviction` |
| Duplicate leases rejected | `install_with_duplicate_leases_is_rejected` |
| Lease rotation requests refresh (no local signing) | `lease_set_rotation_requests_refresh_instead_of_signing_locally` |
| Refreshed lease set replaces prior via install only | `refreshed_lease_set_replaces_old_one_through_install_only` |
| Shutdown releases lease set and decryption capability | `shutdown_releases_client_lease_set_and_decryption_capability` |
| Install while stopping rejected | `install_while_stopping_is_rejected` |
| Router-owned runtime rejects client install | `router_owned_runtime_rejects_client_install` |

Module-level unit tests in `crates/i2pr-client/src/identity.rs`
exercise `DestinationPublic`, the `InboundDecryptionCapability`
redacted-`Debug`, and the
`InboundDecryptionRef::secret_bytes()`/`diffie_hellman()` seams.
`crates/i2pr-api/src/i2cp/actions.rs` grows a compile-only test
that constructs every `I2cpAction` variant from typed values.

SAM router-owned regressions remain green:

```text
cargo test --locked -p i2pr-daemon --test sam_stream_self_composed
cargo test --locked -p i2pr-daemon --test sam_stream_final_acceptance
```

The 12-test Plan 166 trajectory plus the existing Plan 120–132
trajectories all run from `crates/i2pr-client/tests/`.

## Closure fields

Populated only from executed evidence:

```text
closing_sha = (see `git log -1 --format=%H` after Plan 166 commit)
routine_ci_run = pending (post-merge hosted run)
routine_ci_ubuntu = pending
routine_ci_macos = pending
msrv = passed (local cargo check)
dependency_policy = passed (local cargo deny check)
```

Local floor on the closing tree (all executed 2026-09-07):

```text
cargo fmt --all --check = passed
cargo check --locked --workspace --all-targets = passed
cargo test --locked --workspace --all-targets -- --test-threads=1 = passed (1661 passed, 1 ignored: the Plan 162 external test)
cargo test --locked -p i2pr-client --all-targets = passed (201 passed)
cargo test --locked -p i2pr-api --all-targets = passed (244 passed)
cargo test --locked -p i2pr-client --test plan166_trajectory -- --test-threads=1 = passed (12 passed)
bash scripts/check-dependency-direction.sh = passed
bash scripts/check-runtime-boundaries.sh = passed
bash scripts/check-fixture-manifest.sh = passed
bash scripts/check-i2cp-vectors.sh = passed (manifest complete, 15 vector tests passed)
bash scripts/check-ntcp2-vectors.sh = passed
bash scripts/check-ssu2-vectors.sh = passed
bash scripts/check-ntcp2-interoperability.sh = passed
bash scripts/check-constrained-host-lane-boundary.sh = passed
bash scripts/check-sam-acceptance-evidence.sh = passed (22 rows)
bash scripts/check-ssu2-acceptance-evidence.sh = passed (15 rows)
python3 -m unittest discover -s tests/integration/ntcp2/harness -p 'test_*.py' = passed (153 tests)
cargo clippy --locked --workspace --all-targets --all-features -- -D warnings = passed
RUSTDOCFLAGS="-D warnings" cargo doc --locked --workspace --no-deps = passed
cargo test --locked --workspace --doc = passed (0 tests)
cargo deny check advisories bans sources = passed
```

All Plan 166 §13 acceptance criteria are satisfied:

1. one destination runtime supports explicit `RouterOwned` and
   `ClientOwned` modes without duplicate routing/pool stacks — both
   modes share `DestinationTunnelPool`, `EciesSessionManager`,
   `DestinationDispatcher`, and `DestinationRegistry` (the new
   `new` / `new_client_owned` constructors fan into one
   `assemble_router_owned` helper);
2. client-owned construction requires a public destination (built
   from a `VerifiedSessionConfig` by the Plan 167 daemon) but not
   the client's signing private key — `identity_arc()` returns
   `None` for `ClientOwned`; `static_secret_bytes()` is `None` until
   `install_client_lease_set2` commits;
3. lease requests derive from real destination inbound tunnels —
   `take_client_refresh_request` builds the
   `Vec<LeaseRequestLease>` from `pool.inbound_lease_sources(now)`;
4. Standard LeaseSet2 signature, destination binding, lease
   ownership, expiry, and encryption type are validated — see the
   `Plan 166 §6` chain in `LeaseSetLifecycle::install_external`
   (the duplicate-lease and foreign-lease trajectory tests prove the
   identity);
5. client decryption private key is proven to match the LeaseSet2
   public key before commit — see
   `LeaseSetError::DecryptionKeyMismatch` /
   `DestinationIdentityError::DecryptionCapabilityKeyMismatch` and
   the matched/mismatched trajectory tests;
6. publication and decryption-key installation are atomic —
   `install_client_lease_set2` either installs both the
   `ClientOwnedLeaseSet` and the `InboundDecryptionCapability` or
   rolls back; the cross-check happens before any state mutates;
7. client-owned inbound ECIES traffic uses the existing
   authenticated destination session path —
   `DestinationDispatcher::dispatch_garlic_envelope` already
   accepts `&[u8; 32]` for the static secret, and the runtime now
   hands it the bytes from either `DestinationIdentity` (router-
   owned) or `InboundDecryptionCapability` (client-owned);
8. client-owned lease refresh never causes i2pr to synthesize/sign
   a replacement — `LeaseSetLifecycle::refresh_client` only emits
   `RequestClientRefresh` and never enters the router-signed
   `Regenerate` branch;
9. cancellation/destruction zeroizes/releases decryption/session
   state — `shutdown` drops `self.decryption = None`, and the
   `InboundDecryptionCapability` zeroizes its static public bytes
   in `Drop`;
10. unsupported LeaseSet/key types fail explicitly — encrypted/meta
    LeaseSets and PQ encryption types 5–7 are rejected through the
    `Plan 164` structural layer; the install path additionally
    enforces `CryptoKeyType::X25519` (typed
    `LeaseSetError::UnsupportedEncryptionKeyType { key_type }`);
11. SAM router-owned product/final-acceptance regressions pass
    unchanged — both `sam_stream_self_composed` and
    `sam_stream_final_acceptance` pass against the new
    `DestinationRuntime` constructors;
12. dependency/runtime/secret boundaries remain green — every
    static boundary script passes; no new workspace dependencies;
13. workspace floor and exact-head CI pass — see the local floor
    above;
14. `plans/166-status.md` records evidence and the ownership
    invariant — this file.

## Updates

- `crates/i2pr-client/src/identity.rs` — new `DestinationOwnership`,
  `DestinationPublic`, `InboundDecryptionCapability`,
  `InboundDecryptionRef` types; expanded `DestinationIdentityError`
  with `UnsupportedSigningType`, `UnsupportedCryptoType`,
  `StaticPublicLengthMismatch`, `DecryptionCapabilityKeyMismatch`,
  `DecryptionCapabilityLeaseSet2Mismatch`,
  `ClientOwnedAlreadyInstalled`, `MissingEncryptionPublicKey`; new
  redacted-`Debug` impl on `InboundDecryptionCapability`; new
  `Drop` impl that zeroizes the static public bytes; new module
  tests for the public/diffie_hellman paths.
- `crates/i2pr-client/src/leaseset.rs` — new
  `ClientRefreshCause`, `LeaseRequest`, `LeaseRequestLease`,
  `ClientOwnedLeaseSet` types; expanded `LeaseSetDecision` with
  `RequestClientRefresh`; expanded `LeaseSetLifecycle` with
  `client_current`, `client_generations`, `client_refresh_pending`,
  `evaluate_client_refresh`, `refresh_client`, `install_external`,
  `take_client_refresh_request`, `acknowledge_client_refresh`,
  `release_client`; expanded `LeaseSetError` with
  `InstallWhileStopping`, `LeaseSet2MissingLeases`,
  `ExpiredLeaseSet2Lease`, `LeaseOutlivesExpiration`, `ForeignLease`,
  `DuplicateLease`, `UnsupportedEncryptionKeyType`,
  `DecryptionKeyMismatch`, `ExpiredLeaseSet2`; ownership-aware
  `LeaseSetSummary::from_lifecycle`; new `map_key_selection_error`
  helper.
- `crates/i2pr-client/src/registry.rs` — refactored
  `DestinationRuntime` to the `DestinationOwnership` field set
  (router-owned and client-owned branches on `Option<Arc<...>>`,
  `DestinationPublic`, `Option<InboundDecryptionCapability>`);
  new `new_client_owned`, `install_client_lease_set2`,
  `take_client_refresh_request`, `poll_client_refresh`,
  `decryption_capability`, `static_secret_bytes`, `pool_mut`,
  `client_lease_set`, `outbound_lease_set`, `lease_set_summary`
  accessors; new `Shutdown` zeroizes the decryption capability;
  expanded `DestinationRuntimeError` with the `Identity(#[from]
  DestinationIdentityError)` variant.
- `crates/i2pr-client/src/lib.rs` — re-exports the new types
  (`DestinationOwnership`, `DestinationPublic`,
  `InboundDecryptionCapability`, `InboundDecryptionRef`,
  `ClientRefreshCause`, `LeaseRequest`, `LeaseRequestLease`,
  `ClientOwnedLeaseSet`).
- `crates/i2pr-client/tests/plan166_trajectory.rs` — new
  Plan 166 trajectory (12 tests, every Plan 166 §11 case).
- `crates/i2pr-client/tests/plan120_trajectory.rs` — updates two
  callers that used the old `runtime.identity()` returning
  `&DestinationIdentity`; the runtime now returns
  `Option<&DestinationIdentity>`, so both call sites add an
  `.expect("router-owned")` at the existing seam (no production
  semantics change).
- `crates/i2pr-api/src/i2cp/actions.rs` — new
  `I2cpAction::RequestVariableLeaseSet` variant, new
  `LeaseRefreshCause` enum, new `LeaseRequestLease` struct; updated
  module doc to document the Plan 166 contribution.
- `crates/i2pr-api/src/i2cp/mod.rs` — re-exports `LeaseRefreshCause`
  and `LeaseRequestLease`.
- `docs/architecture/i2pr-client.md` — new "Plan 166 — Client-owned
  destinations and the LeaseSet2 bridge" section documenting the
  capability-oriented ownership factoring, the atomic
  install_external pipeline, the typed lease refresh material, and
  the 12-test Plan 166 trajectory.
- `docs/architecture/i2pr-api.md` — new "Plan 166 — I2CP
  client-owned LeaseSet2 request action" section documenting
  `I2cpAction::RequestVariableLeaseSet` and the
  `LeaseRefreshCause` / `LeaseRequestLease` typed vocabulary.
- `specs/protocols/10-i2cp-service-tunnels.md` — new "M9 I2CP
  client-owned destination + LeaseSet2 bridge (Plan 166)" section
  enumerating every Plan 166 §6 checklist step and the boundary
  rule "router never receives the destination signing private key".
- `specs/support.toml` — new `plan_166_*` authority entries;
  `milestone9_i2cp_client_owned_destination = passed-via-plan166`;
  `next_executable_plan = "167"`; new `i2cp.client-owned-destination`
  surface row with evidence links.
- `specs/CONFORMANCE.md` — extended the I2CP row with the
  Plan 166 client-owned destination + Standard LeaseSet2 bridge
  summary.
- `plans/README.md` — Plan 166 added to the current authority
  block, next-executable-plan advanced to `167`, the
  "What landed" / "What's not yet accepted" / "Current handoff"
  blocks updated.
- `README.md` — added `plan_166 = passed-...` classification row,
  advanced the next-executable-plan from `166` to `167`, and
  updated the Milestone 9 narrative with the Plan 166 surface
  summary.
- `AGENTS.md` — Plan 166 added to the current authority, advanced
  the M9 reading order to start at `plans/166-status.md`, added
  Plan 166 to the focused-seams list and the protocol-claims
  list, and advanced the handoff sentence.
- `.opencode/skills/i2pr-local-dev/SKILL.md` — Plan 166 added to
  the milestone status, "Retain these working pieces" list,
  protocol claims, and current handoff.
- `.opencode/skills/i2pr-architecture/SKILL.md` — Plan 166 added to
  the per-crate deep-dive index and the plan-of-record authority
  chain.

## Handoff

Plan 166 is closed. Execute Plan **167**
([`plans/167-m9-i2cp-loopback-server-runtime.md`](167-m9-i2cp-loopback-server-runtime.md))
next, then Plans 168–170 in order:

```text
plan_166 = passed-m9-i2cp-client-owned-destination-and-leaseset2
next_executable_plan = 167
next_product_layer = milestone9-i2cp
```

Do not extend Plan 164's structural codecs, Plan 165's state
machines, or Plan 166's client-owned destination runtime into a
listener, socket ownership, or interoperability claim; those
belong to the later M9 passes.
